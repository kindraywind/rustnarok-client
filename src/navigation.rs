//! Navigation: a route to a map, a cell, an NPC or a monster across maps, by the client's navigation tables
//! (lua::NaviTables). It starts from the server (ZC_NAVIGATION_ACTIVE), a `<NAVI>` link, the navigation window or a typed
//! `/navi prontera 150/150`, says where it stands in the chat, and shows as arrows on the ground along the way to the
//! next warp or the target, with a marker on the cell it heads for.

use std::collections::{HashMap, HashSet};

use rustnarok_client::lua::{NaviLink, NaviTables};
use rustnarok_client::messages::fill;
use rustnarok_client::text::Font;

use crate::Layer;
use crate::window::{Chrome, Frame, TITLE_HEIGHT, TitleHit};

/// A move ordered to a cell this near the target's, on both axes, ends the navigation.
pub const REACHED: i32 = 5;
/// "Do you want to cancel navigation?"
pub const ASKS_STOP: usize = 2239;
/// "Navigation >: %s", around most of what a navigation says in the chat, and what it says: a typed one that can't be
/// read, no way, a map the tables don't have, a route found, there (said bare too), on the monster's map, the way on
/// this map, there already.
const SAYS: usize = 2315;
const UNCLEAR: usize = 2294;
const NO_WAY: usize = 2298;
const NO_MAP: usize = 2301;
const FOLLOW: usize = 2308;
pub const ARRIVED: usize = 2309;
const MONSTER_MAP: usize = 2311;
const GO: usize = 2313;
const THERE: usize = 2314;
/// A route's first step, said whole: "Talk to [%s]", "Please move to [%s].", "Please move to the Airship.", "Use the
/// Warp Portal to move to the next area."
const TALK_TO: usize = 2316;
const MOVE_TO: usize = 2317;
const TO_AIRSHIP: usize = 2319;
const BY_PORTAL: usize = 2320;
/// Arrows and the marker show this many cells from the player, on both axes.
pub const SEEN: i32 = 9;
/// How opaque an arrow is, and the one in five that's brighter; the marker, and its tint on the way to a warp.
pub const ARROW_ALPHA: [f32; 2] = [0x99 as f32 / 255.0, 0xEE as f32 / 255.0];
pub const MARKER_TINT: [f32; 4] = [1.0, 1.0, 1.0, 0xEE as f32 / 255.0];
pub const STEP_MARKER_TINT: [f32; 4] = [0x64 as f32 / 255.0, 0xAA as f32 / 255.0, 1.0, 0xEE as f32 / 255.0];

/// Where to go: a map, and a cell on it or a monster there (or neither: the map itself).
#[derive(Debug, Clone, PartialEq)]
pub struct Target {
    pub map: String,
    pub cell: Option<[u16; 2]>,
    pub monster: Option<u16>,
    /// What the target is called, for the window.
    pub name: String,
}

impl Target {
    /// What it routes to: a monster on its map, a cell, or the map itself.
    pub fn kind(&self) -> &'static str {
        match (self.monster, self.cell) {
            (Some(_), _) => "monster",
            (None, Some(_)) => "cell",
            (None, None) => "map",
        }
    }
}

/// A map name as the tables have it: no ".gat" or ".rsw", lower case.
pub fn map_key(map: &str) -> String {
    map.trim_end_matches(".gat").trim_end_matches(".rsw").to_ascii_lowercase()
}

/// A map as the tables know it: a copy (`iz_ac01_a`: a trailing `_` and a letter) the tables don't have counts as the
/// map it copies when they have that one; the copies have the same warps at the same cells.
pub fn navi_map(tables: &NaviTables, map: &str) -> String {
    let map = map_key(map);
    match map.rsplit_once('_') {
        Some((base, letter))
            if !tables.maps.contains_key(&map) && letter.len() == 1 && letter.as_bytes()[0].is_ascii_lowercase() && tables.maps.contains_key(base) =>
        {
            base.to_owned()
        }
        _ => map,
    }
}

/// Whether a way between maps may be taken with these services (the digits of rathena's NAV_ flags: ones the airship,
/// tens scrolls, hundreds Kafra). Warps and NPCs that send you always may.
// ponytail: which of the tables' kinds 204 and 205 is the Kafra and which the airship is a reading of the data.
pub fn allowed(kind: u32, services: u8) -> bool {
    match kind {
        200 | 201 => true,
        204 => !(services / 100).is_multiple_of(10),
        205 => !services.is_multiple_of(10),
        _ => false,
    }
}

/// What a route search asks of the maps themselves.
pub struct Walks<'a> {
    /// How many cells the player walks to a cell of this map; None when there's no way.
    pub from_player: &'a dyn Fn([u16; 2]) -> Option<u32>,
    /// How many cells it is from a cell of the target's map to the target (0 when that's the map itself or a monster);
    /// None when there's no way.
    pub to_target: &'a dyn Fn([u16; 2]) -> Option<u32>,
}

/// The ways to take from `here` to the target on `to`, fewest ways first. It starts from this map's ways the player
/// can walk to, goes on from each way to the ways that can be walked to from where it arrives (the distance table's
/// lists, in their order), and ends with the first way that arrives where the target can be walked to. None when
/// there's no such chain. A target on this very map is a route too: out and back in by another way.
pub fn route<'a>(tables: &'a NaviTables, here: &str, to: &str, services: u8, walks: &Walks) -> Option<Vec<&'a NaviLink>> {
    let (here, to) = (navi_map(tables, here), navi_map(tables, to));
    let links = &tables.links;
    let by_id: HashMap<u32, usize> = links.iter().enumerate().map(|(index, link)| (link.id, index)).collect();
    // ponytail: data without the distance table: every way of the map a way arrives on counts as walkable to.
    let passes = |index: usize| -> Vec<(usize, u32)> {
        if tables.passes.is_empty() {
            links.iter().enumerate().filter(|(_, next)| next.map == links[index].to).map(|(next, _)| (next, 0)).collect()
        } else {
            tables.passes.get(&links[index].id).into_iter().flatten().filter_map(|&(id, cells)| Some((*by_id.get(&id)?, cells))).collect()
        }
    };
    // Each way reached: the way, the one it was reached from, the cells walked so far.
    let mut reached: Vec<(usize, Option<usize>, u32)> = links
        .iter()
        .enumerate()
        .filter(|(_, link)| link.map == here && allowed(link.kind, services))
        .filter_map(|(index, link)| Some((index, None, (walks.from_player)(link.cell)?)))
        .collect();
    let mut level_of: HashMap<usize, usize> = reached.iter().map(|way| (way.0, 0)).collect();
    let mut from = 0;
    for level in 1..100 {
        let until = reached.len();
        let arrives = |&way: &usize| links[reached[way].0].to == to && (walks.to_target)(links[reached[way].0].to_cell).is_some();
        if let Some(last) = (from..until).find(arrives) {
            let mut chain = Vec::new();
            let mut at = Some(last);
            while let Some(way) = at {
                chain.push(&links[reached[way].0]);
                at = reached[way].1;
            }
            chain.reverse();
            return Some(chain);
        }
        for way in from..until {
            let (index, _, walked) = reached[way];
            for (next, cells) in passes(index) {
                match level_of.get(&next) {
                    // Reached twice in one round: the shorter walk keeps it.
                    Some(&seen) if seen == level => {
                        if let Some(other) = reached[until..].iter_mut().find(|other| other.0 == next)
                            && walked + cells < other.2
                        {
                            (other.1, other.2) = (Some(way), walked + cells);
                        }
                    }
                    Some(_) => {}
                    None if allowed(links[next].kind, services) => {
                        level_of.insert(next, level);
                        reached.push((next, Some(way), walked + cells));
                    }
                    None => {}
                }
            }
        }
        if reached.len() == until {
            return None;
        }
        from = until;
    }
    None
}

fn near(cell: [i32; 2], goal: [u16; 2], within: i32) -> bool {
    (cell[0] - i32::from(goal[0])).abs() <= within && (cell[1] - i32::from(goal[1])).abs() <= within
}

/// Whether a move ordered to `to` on `here` ends the navigation: its target is a cell on this map, `REACHED` or nearer
/// on both axes. A map or a monster ends it when the map loads (`report`).
pub fn reached(tables: &NaviTables, here: &str, to: [i32; 2], target: &Target) -> bool {
    navi_map(tables, here) == navi_map(tables, &target.map) && target.cell.is_some_and(|goal| near(to, goal, REACHED))
}

/// How a navigation stands once it starts or a map loads under it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Outcome {
    Guiding,
    /// There: nothing to guide to.
    Over,
    /// No way from this map.
    NoWay,
}

/// "Navigation >: " and a message.
pub fn says(messages: &[String], id: usize) -> String {
    let message = |id: usize| messages.get(id).map_or("", String::as_str);
    fill(message(SAYS), &[&message(id)])
}

/// The typed navigation that can't be read, as the chat says it.
pub fn unclear(messages: &[String]) -> String {
    says(messages, UNCLEAR)
}

/// What `report` finds: the chat's lines, how the navigation stands, and the cell to head for on this map while it
/// guides.
#[derive(Debug, PartialEq)]
pub struct Report {
    pub lines: Vec<String>,
    pub outcome: Outcome,
    pub next: Option<[u16; 2]>,
}

/// What the chat says when a navigation starts (`start`) or a map loads under one, with the player on `cell` of
/// `here`, how it stands then, and where it heads on this map.
#[allow(clippy::too_many_arguments)]
pub fn report(tables: &NaviTables, messages: &[String], here: &str, cell: [i32; 2], target: &Target, services: u8, start: bool, walks: &Walks) -> Report {
    let message = |id: usize| messages.get(id).cloned().unwrap_or_default();
    let says = |id: usize| says(messages, id);
    let found = |lines: Vec<String>, outcome: Outcome, next: Option<[u16; 2]>| Report { lines, outcome, next };
    let here = navi_map(tables, here);
    if !tables.maps.contains_key(&here) {
        return found(vec![says(NO_MAP)], Outcome::NoWay, None);
    }
    if here == navi_map(tables, &target.map) {
        match (target.cell, target.monster) {
            // Next to the cell: nothing starts. A load right onto it: that's the end, said like any arrival.
            (Some(goal), _) if start && near(cell, goal, 1) => return found(vec![says(THERE)], Outcome::Over, None),
            (Some(goal), _) if !start && near(cell, goal, 0) => return found(vec![message(ARRIVED), says(THERE)], Outcome::Over, None),
            (Some(goal), _) if (walks.from_player)(goal).is_some() => return found(vec![says(GO)], Outcome::Guiding, Some(goal)),
            // No walking there from here: a route out of the map and back in, below.
            (Some(_), _) => {}
            // ponytail: the lines of a load onto the target's map, for a start on it too.
            (None, Some(_)) => return found(vec![message(ARRIVED), says(MONSTER_MAP)], Outcome::Over, None),
            (None, None) => return found(vec![message(ARRIVED), says(ARRIVED)], Outcome::Over, None),
        }
    }
    let Some(first) = route(tables, &here, &target.map, services, walks).and_then(|route| route.first().copied()) else {
        return found(vec![says(NO_WAY)], Outcome::NoWay, None);
    };
    // The route's first step: an NPC to talk to and where it sends you, the airship, or a warp portal.
    let mut lines = vec![says(FOLLOW)];
    match first.kind {
        201..=204 => {
            let shown = tables.maps.get(&first.to).map_or("", String::as_str);
            lines.push(fill(&message(TALK_TO), &[&first.name]));
            lines.push(fill(&message(MOVE_TO), &[&format!(" {shown}[ {} ] ", first.to)]));
        }
        205 => lines.extend([fill(&message(TALK_TO), &[&first.name]), message(TO_AIRSHIP)]),
        _ => lines.push(message(BY_PORTAL)),
    }
    found(lines, Outcome::Guiding, Some(first.cell))
}

/// Which way a step goes, as the arrow pictures are numbered: 0 north (+y), then anticlockwise in eighths (1
/// north-west, 2 west, … 7 north-east).
pub fn step_turn(from: [i32; 2], to: [i32; 2]) -> usize {
    match [(to[0] - from[0]).signum(), (to[1] - from[1]).signum()] {
        [0, 1] => 0,
        [-1, 1] => 1,
        [-1, 0] => 2,
        [-1, -1] => 3,
        [0, -1] => 4,
        [1, -1] => 5,
        [1, 0] => 6,
        _ => 7,
    }
}

fn seen(cell: [i32; 2], player: [i32; 2]) -> bool {
    (cell[0] - player[0]).abs() <= SEEN && (cell[1] - player[1]).abs() <= SEEN
}

/// The arrows on the ground: one on every second cell of the trail from its start, never on its last, where the player
/// (on `player`) sees it: the cell, which way its step goes (`step_turn`), and whether it's a bright one. The bright
/// ones are five arrows apart and step along the trail, one arrow each 200 ms (lit for 100).
pub fn arrows(trail: &[[i32; 2]], player: [i32; 2], ms: u128) -> Vec<([i32; 2], usize, bool)> {
    let lit = (ms % 1000 / 100) as usize;
    (0..trail.len().saturating_sub(1))
        .step_by(2)
        .filter(|&index| seen(trail[index], player))
        .map(|index| (trail[index], step_turn(trail[index], trail[index + 1]), index % 10 == lit))
        .collect()
}

/// The marker's cell: the trail's last, where the player sees it.
pub fn marker(trail: &[[i32; 2]], player: [i32; 2]) -> Option<[i32; 2]> {
    trail.last().copied().filter(|&cell| seen(cell, player))
}

/// Which of the marker's four pictures shows: the next every 150 ms.
pub fn marker_frame(ms: u128) -> usize {
    (ms / 150 % 4) as usize
}

/// Whether `here` is the target's map (a copy of it counts).
pub fn on_target_map(tables: &NaviTables, here: &str, target: &Target) -> bool {
    navi_map(tables, here) == navi_map(tables, &target.map)
}

/// What the navigation window finds for `text`: maps (by name or shown name), NPCs and monsters whose names hold it
/// (any case), at most `most`.
pub fn search(tables: &NaviTables, text: &str, most: usize) -> Vec<Target> {
    let text = text.trim().to_lowercase();
    if text.is_empty() {
        return Vec::new();
    }
    let has = |name: &str| name.to_lowercase().contains(&text);
    let mut maps: Vec<(&String, &String)> = tables.maps.iter().filter(|(map, shown)| has(map) || has(shown)).collect();
    maps.sort();
    let maps = maps.into_iter().map(|(map, shown)| Target { map: map.clone(), cell: None, monster: None, name: format!("{shown} ({map})") });
    let npcs = tables.npcs.iter().filter(|npc| has(&npc.name)).map(|npc| Target {
        map: npc.map.clone(),
        cell: Some(npc.cell),
        monster: None,
        name: format!("{} ({})", npc.name, npc.map),
    });
    let mut seen = HashSet::new();
    let monsters = tables.monsters.iter().filter(|monster| has(&monster.name) && seen.insert((monster.map.clone(), monster.id))).map(|monster| Target {
        map: monster.map.clone(),
        cell: None,
        monster: Some(monster.id),
        name: format!("{} ({})", monster.name, monster.map),
    });
    maps.chain(npcs).chain(monsters).take(most).collect()
}

/// A typed navigation command.
#[derive(Debug, PartialEq)]
pub enum Typed {
    /// A cell of a map, and the services the route may use.
    Go { map: String, cell: [u16; 2], services: u8 },
    /// Its `x/y` or its options can't be read.
    Unclear,
    /// `/naviopen`: the window.
    Open,
}

/// A typed navigation, as the game's help on navigation writes it: `/navi <map> <x>/<y>` (or `/navigation`) by the
/// `usual` services, and `/navi2 <map> <x>/<y> <scroll>/<kafra>/<airship>` (or `/navigation2`), each of those three 1
/// or 0. Words past those are left unread. None for any other line and for one with too few words (`/navi`, `/navi
/// prontera`): that's no command at all.
pub fn typed(text: &str, usual: u8) -> Option<Typed> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let options = match (words.first()?.to_ascii_lowercase().as_str(), words.len()) {
        ("/naviopen", _) => return Some(Typed::Open),
        ("/navi" | "/navigation", 3..) => None,
        ("/navi2" | "/navigation2", 4..) => Some(words[3]),
        _ => return None,
    };
    let go = || {
        let cell = pair(words[2])?;
        let services = match options {
            Some(options) => {
                let [scroll, kafra, airship] = flags(options)?;
                kafra * 100 + scroll * 10 + airship
            }
            None => usual,
        };
        Some(Typed::Go { map: map_key(words[1]), cell, services })
    };
    Some(go().unwrap_or(Typed::Unclear))
}

/// C's atoi: a sign, then digits up to anything else (none: 0).
fn atoi(text: &str) -> i64 {
    let (sign, digits) = match text.strip_prefix('-') {
        Some(digits) => (-1, digits),
        None => (1, text.strip_prefix('+').unwrap_or(text)),
    };
    sign * digits.bytes().take_while(u8::is_ascii_digit).fold(0, |value: i64, digit| (value * 10 + i64::from(digit - b'0')).min(i64::from(u32::MAX)))
}

/// `x/y`: a number on each side of the first "/", both from 0 to 500.
fn pair(text: &str) -> Option<[u16; 2]> {
    let (x, y) = text.split_once('/')?;
    let side = |text: &str| (!text.is_empty()).then(|| atoi(text)).filter(|value| (0..=500).contains(value)).map(|value| value as u16);
    Some([side(x)?, side(y)?])
}

/// `1/0/1`: three parts, each 1 or 0.
fn flags(text: &str) -> Option<[u8; 3]> {
    let mut parts = text.splitn(3, '/');
    let mut flag = || parts.next().filter(|part| !part.is_empty()).map(atoi).filter(|value| (0..=1).contains(value)).map(|value| value as u8);
    Some([flag()?, flag()?, flag()?])
}

/// A navigation under way: its target, the services it may use, and the trail on this map to where it heads next.
pub struct Active {
    pub target: Target,
    pub services: u8,
    pub trail: Vec<[i32; 2]>,
    /// The map and the cell headed for that the trail was made for.
    trail_for: Option<(String, [u16; 2])>,
    /// The map the route was last looked up on, and the cell it heads for there.
    next_for: Option<(String, Option<[u16; 2]>)>,
}

impl Active {
    pub fn new(target: Target, services: u8) -> Self {
        Self { target, services, trail: Vec::new(), trail_for: None, next_for: None }
    }

    /// The cell `report` found to head for on `here`: kept until the next map's report.
    pub fn head_for(&mut self, here: &str, next: Option<[u16; 2]>) {
        self.next_for = Some((map_key(here), next));
    }

    /// Where it heads on `here` (None on a map no report was made for).
    pub fn next_cell(&self, here: &str) -> Option<[u16; 2]> {
        self.next_for.as_ref().filter(|(map, _)| *map == map_key(here)).and_then(|(_, next)| *next)
    }

    /// Makes the trail (with `path`, the cells walked from one cell to another) when the map or the cell headed for
    /// changed: the player's cell, then the way on. It isn't cut as the player walks; once none of its arrows nor its
    /// end is in sight any more it's made again, from where the player is then. One that can't be made stays empty until
    /// the map or the cell headed for changes.
    pub fn update_trail(&mut self, map: &str, cell: [i32; 2], next: Option<[u16; 2]>, path: impl FnOnce([i32; 2], [i32; 2]) -> Option<Vec<[i32; 2]>>) {
        let Some(next) = next else {
            self.trail.clear();
            self.trail_for = None;
            return;
        };
        let key = (map_key(map), next);
        let lost = !self.trail.is_empty() && !self.trail.iter().step_by(2).chain(self.trail.last()).any(|&drawn| seen(drawn, cell));
        if self.trail_for.as_ref() != Some(&key) || lost {
            let walk = path(cell, [i32::from(next[0]), i32::from(next[1])]);
            self.trail = walk.map(|walk| std::iter::once(cell).chain(walk).collect()).unwrap_or_default();
            self.trail_for = Some(key);
        }
    }
}

/// "Navigation", and the window's labels.
const TITLE: usize = 2221;
// ponytail: the window is ours over the client's chrome: a search field, eight results, a stop row.
const WIDTH: f32 = 280.0;
const ROW: f32 = 16.0;
const ROWS: usize = 8;
const FIELD_TOP: f32 = TITLE_HEIGHT + 6.0;
const RESULTS_TOP: f32 = FIELD_TOP + ROW + 6.0;
const STOP_TOP: f32 = RESULTS_TOP + ROWS as f32 * ROW + 4.0;
const HEIGHT: f32 = STOP_TOP + ROW + 6.0;
const LEFT: f32 = 8.0;
const TEXT: [u8; 3] = [0, 0, 0];
const GREY: [u8; 3] = [0x80, 0x80, 0x80];
const PICKED: [u8; 3] = [0x30, 0x50, 0xC0];

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(TitleHit),
    Field,
    Result(usize),
    Stop,
    Window,
}

/// The navigation window: what's typed, what it finds (and whether it found more than it shows), and whether typing
/// goes to it.
pub struct Window {
    pub frame: Frame,
    pub text: String,
    pub typing: bool,
    pub results: Vec<Target>,
    pub more: bool,
}

impl Window {
    pub fn new() -> Self {
        // ponytail: where it opens; right of the middle, clear of the player.
        let mut frame = Frame::new([680.0, 180.0], WIDTH);
        frame.open = true;
        Self { frame, text: String::new(), typing: true, results: Vec::new(), more: false }
    }

    /// Searches again for what's typed: one more than the rows, to know whether the list was cut.
    pub fn search(&mut self, tables: &NaviTables) {
        self.results = search(tables, &self.text, ROWS + 1);
        self.more = self.results.len() > ROWS;
        self.results.truncate(ROWS);
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let local = self.frame.local(cursor, HEIGHT)?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(Hit::Title(hit));
        }
        let row = |top: f32| (top..top + ROW).contains(&local[1]);
        Some(if row(FIELD_TOP) {
            Hit::Field
        } else if row(STOP_TOP) {
            Hit::Stop
        } else if (RESULTS_TOP..RESULTS_TOP + ROWS as f32 * ROW).contains(&local[1]) {
            let index = ((local[1] - RESULTS_TOP) / ROW) as usize;
            if index < self.results.len() { Hit::Result(index) } else { Hit::Window }
        } else {
            Hit::Window
        })
    }

    /// The field, the results' rows (with the target each picks) and the stop row on screen (for the test driver).
    pub fn button_boxes(&self) -> Vec<(String, [f32; 4], Option<&Target>)> {
        if !self.frame.open || self.frame.minimized {
            return Vec::new();
        }
        let area = |top: f32| {
            let [x, y] = self.frame.at([LEFT, top]);
            [x, y, WIDTH - 2.0 * LEFT, ROW]
        };
        let mut boxes: Vec<(String, [f32; 4], Option<&Target>)> = vec![("navi_field".to_owned(), area(FIELD_TOP), None)];
        boxes.extend(
            self.results.iter().enumerate().map(|(index, target)| (format!("navi_result_{index}"), area(RESULTS_TOP + index as f32 * ROW), Some(target))),
        );
        boxes.push(("navi_stop".to_owned(), area(STOP_TOP), None));
        boxes
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &self,
        layer: &mut Layer<'a>,
        chrome: &'a Chrome,
        font: &Font,
        messages: &[String],
        active: Option<&Target>,
        pointer: [f32; 2],
        caret: bool,
    ) {
        if !self.frame.open {
            return;
        }
        let at = |local: [f32; 2]| self.frame.at(local);
        self.frame.draw_title(layer, chrome, messages.get(TITLE).map_or("Navigation", String::as_str), pointer);
        if self.frame.minimized {
            return;
        }
        layer.sprites.push((&chrome.white, at([0.0, TITLE_HEIGHT]), [WIDTH, HEIGHT - TITLE_HEIGHT]));
        for x in [0.0, WIDTH - 1.0] {
            layer.sprites.push((&chrome.border, at([x, TITLE_HEIGHT]), [1.0, HEIGHT - TITLE_HEIGHT]));
        }
        layer.sprites.push((&chrome.border, at([0.0, HEIGHT - 1.0]), [WIDTH, 1.0]));
        // The search field: a framed line with the caret.
        let field = at([LEFT, FIELD_TOP]);
        layer.sprites.push((&chrome.border, field, [WIDTH - 2.0 * LEFT, ROW]));
        layer.sprites.push((&chrome.white, [field[0] + 1.0, field[1] + 1.0], [WIDTH - 2.0 * LEFT - 2.0, ROW - 2.0]));
        let typed = if self.typing && caret { format!("{}_", self.text) } else { self.text.clone() };
        layer.texts.push((typed, [field[0] + 3.0, field[1] + 1.0], TEXT));
        for (index, target) in self.results.iter().enumerate() {
            let color = if active == Some(target) { PICKED } else { TEXT };
            layer.texts.push((label(font, target), at([LEFT, RESULTS_TOP + index as f32 * ROW]), color));
        }
        // The stop row while a navigation runs: what it goes to.
        let stop = match active {
            Some(target) => format!("■ {}", crate::window::fit(font, &target.name, WIDTH - 2.0 * LEFT - 20.0)),
            None => String::new(),
        };
        layer.texts.push((stop, at([LEFT, STOP_TOP]), if active.is_some() { PICKED } else { GREY }));
    }
}

/// What a result's row shows: its target's name, fit to the row.
pub fn label(font: &Font, target: &Target) -> String {
    crate::window::fit(font, &target.name, WIDTH - 2.0 * LEFT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustnarok_client::lua::{NaviMonster, NaviNpc};

    /// A way with an id of its own (its cell's numbers), arriving on (1, 1).
    fn link(map: &str, kind: u32, cell: [u16; 2], to: &str) -> NaviLink {
        NaviLink { id: u32::from(cell[0]) * 1000 + u32::from(cell[1]), map: map.into(), kind, name: String::new(), cell, to: to.into(), to_cell: [1, 1] }
    }

    /// Every cell can be walked to, a cell off.
    const OPEN: Walks = Walks { from_player: &|_| Some(1), to_target: &|_| Some(1) };

    fn tables() -> NaviTables {
        NaviTables {
            maps: [("prt_fild08", "Prontera Field"), ("prontera", "Prontera"), ("prt_in", "Prontera Inside"), ("izlude", "Izlude")]
                .map(|(map, shown)| (map.to_owned(), shown.to_owned()))
                .into_iter()
                .collect(),
            links: vec![
                link("prt_fild08", 200, [170, 375], "prontera"),
                link("prontera", 200, [156, 22], "prt_fild08"),
                link("prontera", 200, [100, 50], "prt_in"),
                link("prontera", 204, [150, 100], "izlude"),
                link("prt_fild08", 200, [380, 200], "izlude"),
            ],
            npcs: vec![NaviNpc { map: "prt_in".into(), name: "Tool Dealer".into(), cell: [30, 40] }],
            monsters: vec![
                NaviMonster { map: "prt_fild08".into(), id: 1002, name: "Poring".into() },
                NaviMonster { map: "prt_fild08".into(), id: 1002, name: "Poring".into() },
            ],
            passes: HashMap::new(),
        }
    }

    #[test]
    fn routes_across_maps() {
        let tables = tables();
        let maps = |route: Option<Vec<&NaviLink>>| route.map(|route| route.iter().map(|link| (link.map.clone(), link.cell)).collect::<Vec<_>>());
        // Two maps on: the field's warp to Prontera, then Prontera's to the inside.
        assert_eq!(maps(route(&tables, "prt_fild08.gat", "prt_in", 0, &OPEN)), Some(vec![("prt_fild08".into(), [170, 375]), ("prontera".into(), [100, 50])]));
        // Prontera to Izlude: the Kafra (204) only with the Kafra allowed; else round by the field.
        assert_eq!(route(&tables, "prontera", "izlude", 100, &OPEN).unwrap().len(), 1);
        assert_eq!(
            maps(route(&tables, "prontera", "izlude", 0, &OPEN)).unwrap().iter().map(|way| way.0.as_str()).collect::<Vec<_>>(),
            ["prontera", "prt_fild08"]
        );
        assert_eq!(route(&tables, "prt_in", "izlude", 111, &OPEN), None);
        // A place on this very map: out and back in.
        assert_eq!(
            maps(route(&tables, "prontera", "prontera", 0, &OPEN)).unwrap().iter().map(|way| way.0.as_str()).collect::<Vec<_>>(),
            ["prontera", "prt_fild08"]
        );
    }

    #[test]
    fn routes_only_by_ways_that_can_be_walked_to() {
        // A town in two halves: the gate (10, 10) leads to the field, the pier (90, 90) to the island; the field's own
        // warps go back to the gate's half (20, 20) and to the island.
        let way = |map: &str, cell: [u16; 2], to: &str, to_cell: [u16; 2]| NaviLink { to_cell, ..link(map, 200, cell, to) };
        let mut tables = tables();
        tables.links = vec![
            way("town", [10, 10], "field", [5, 5]),
            way("town", [90, 90], "island", [1, 1]),
            way("field", [6, 6], "town", [20, 20]),
            way("field", [50, 50], "island", [9, 9]),
            way("island", [2, 2], "town", [91, 91]),
        ];
        let id = |cell: [u16; 2]| u32::from(cell[0]) * 1000 + u32::from(cell[1]);
        // From where each way arrives: the field's two warps from the gate, the gate from the field, the pier from the
        // island; nothing from the island's far shore (9, 9).
        tables.passes = [
            (id([10, 10]), vec![(id([6, 6]), 3), (id([50, 50]), 60)]),
            (id([90, 90]), vec![(id([2, 2]), 2)]),
            (id([6, 6]), vec![(id([10, 10]), 12)]),
            (id([50, 50]), vec![]),
            (id([2, 2]), vec![(id([90, 90]), 2)]),
        ]
        .into_iter()
        .collect();
        let cells = |route: Option<Vec<&NaviLink>>| route.map(|route| route.iter().map(|link| link.cell).collect::<Vec<_>>());
        // In the gate's half only the gate can be walked to: the island is the field's far warp, not the pier.
        let west = Walks { from_player: &|cell| (cell == [10, 10]).then_some(4), to_target: &|_| Some(1) };
        assert_eq!(cells(route(&tables, "town", "island", 0, &west)), Some(vec![[10, 10], [50, 50]]));
        // From the pier's half: the pier.
        let east = Walks { from_player: &|cell| (cell == [90, 90]).then_some(4), to_target: &|_| Some(1) };
        assert_eq!(cells(route(&tables, "town", "island", 0, &east)), Some(vec![[90, 90]]));
        // A cell on the island only its near shore (1, 1) can walk to: the far warp's arrival doesn't end the search,
        // and nothing leads on from it. From the pier's half it's the pier.
        let shore = |from: [u16; 2]| move |cell: [u16; 2]| (cell == from).then_some(4);
        let near_shore = |cell: [u16; 2]| (cell == [1, 1]).then_some(3);
        assert_eq!(cells(route(&tables, "town", "island", 0, &Walks { from_player: &shore([10, 10]), to_target: &near_shore })), None);
        assert_eq!(cells(route(&tables, "town", "island", 0, &Walks { from_player: &shore([90, 90]), to_target: &near_shore })), Some(vec![[90, 90]]));
        // To the pier's half of the town from the gate's half: no way by these warps; from the island, its warp.
        let pier_half = |cell: [u16; 2]| (cell == [91, 91]).then_some(2);
        assert_eq!(cells(route(&tables, "town", "town", 0, &Walks { from_player: &shore([10, 10]), to_target: &pier_half })), None);
        assert_eq!(cells(route(&tables, "island", "town", 0, &Walks { from_player: &|_| Some(1), to_target: &pier_half })), Some(vec![[2, 2]]));
        // Nowhere to walk to at all: no route.
        assert_eq!(route(&tables, "town", "island", 0, &Walks { from_player: &|_| None, to_target: &|_| Some(1) }), None);
    }

    #[test]
    fn reads_a_typed_navigation() {
        let go = |map: &str, cell, services| Some(Typed::Go { map: map.into(), cell, services });
        assert_eq!(typed("/navi prontera 100/100", 101), go("prontera", [100, 100], 101));
        assert_eq!(typed("/NAVIGATION Prontera.gat 150/180 and more", 101), go("prontera", [150, 180], 101));
        // /navi2's options: scrolls (tens), the Kafra (hundreds), the airship (ones).
        assert_eq!(typed("/navi2 prontera 100/90 0/1/0", 101), go("prontera", [100, 90], 100));
        assert_eq!(typed("/navigation2 prontera 100/90 1/0/1", 101), go("prontera", [100, 90], 11));
        assert_eq!(typed("/navi2 prontera 100/100 1/1/1", 101), go("prontera", [100, 100], 111));
        // Numbers as C reads them: what isn't one is 0, and what follows one is dropped.
        assert_eq!(typed("/navi prontera abc/def", 101), go("prontera", [0, 0], 101));
        assert_eq!(typed("/navi prontera 100/100/9", 101), go("prontera", [100, 100], 101));
        assert_eq!(typed("/navi prontera 500/+7x", 101), go("prontera", [500, 7], 101));
        // An x/y or options that can't be read.
        for text in [
            "/navi prontera 100 100",
            "/navi prontera 100,100 x",
            "/navi prontera /100",
            "/navi prontera 100/",
            "/navi prontera 501/100",
            "/navi prontera 100/-1",
            "/navi2 prontera 100 100 1/1/1",
            "/navi2 prontera 100/100 111",
            "/navi2 prontera 100/100 1/1",
            "/navi2 prontera 100/100 1//1",
            "/navi2 prontera 100/100 1/2/0",
        ] {
            assert_eq!(typed(text, 101), Some(Typed::Unclear), "{text}");
        }
        assert_eq!((typed("/naviopen", 101), typed("/NaviOpen now", 101)), (Some(Typed::Open), Some(Typed::Open)));
        // Too few words is no command, like any other line.
        for text in ["/navi", "/navi prontera", "/navigation prontera", "/navi2 prontera 100/100", "/navx prontera 1/2", "navi prontera 1/2", ""] {
            assert_eq!(typed(text, 101), None, "{text}");
        }
    }

    /// The messages a navigation says, each under its own number.
    fn messages() -> Vec<String> {
        let mut messages = vec![String::new(); 2321];
        for (id, text) in [
            (SAYS, "Navigation >: %s"),
            (UNCLEAR, "Invalid syntax."),
            (NO_WAY, "Failed to find a path."),
            (NO_MAP, "Map doesn't support directions"),
            (FOLLOW, "A route was found."),
            (ARRIVED, "Arrived at the target map."),
            (MONSTER_MAP, "The mob's map."),
            (GO, "Please go to indicated direction."),
            (THERE, "The goal has been reached"),
            (TALK_TO, "Navigation >: Talk to [%s]"),
            (MOVE_TO, "Navigation >: Please move to [%s]."),
            (TO_AIRSHIP, "Navigation >: Please move to the Airship."),
            (BY_PORTAL, "Navigation >: Use the Warp Portal to move to the next area."),
        ] {
            messages[id] = text.into();
        }
        messages
    }

    #[test]
    fn says_where_a_navigation_stands() {
        let (mut tables, messages) = (tables(), messages());
        tables.links[3].name = "Kafra Employee".into();
        tables.links.push(NaviLink { name: "Airship Staff".into(), ..link("izlude", 205, [200, 50], "prt_in") });
        let cell = Target { map: "prontera".into(), cell: Some([150, 150]), monster: None, name: String::new() };
        let found = |here: &str, at: [i32; 2], target: &Target, services: u8, start: bool| report(&tables, &messages, here, at, target, services, start, &OPEN);
        let report = |here: &str, at: [i32; 2], target: &Target, services: u8, start: bool| {
            let found = found(here, at, target, services, start);
            (found.lines, found.outcome)
        };
        let said = |lines: &[&str], outcome| (lines.iter().map(|line| line.to_string()).collect::<Vec<_>>(), outcome);
        // A cell on this map: guiding; next to it at the start, nothing starts.
        assert_eq!(report("prontera.gat", [100, 100], &cell, 0, true), said(&["Navigation >: Please go to indicated direction."], Outcome::Guiding));
        assert_eq!(report("prontera", [149, 151], &cell, 0, true), said(&["Navigation >: The goal has been reached"], Outcome::Over));
        assert_eq!(report("prontera", [148, 150], &cell, 0, true).1, Outcome::Guiding);
        // A load onto its map: guiding to the cell, or over when right on it.
        assert_eq!(report("prontera", [149, 151], &cell, 0, false).1, Outcome::Guiding);
        assert_eq!(
            report("prontera", [150, 150], &cell, 0, false),
            said(&["Arrived at the target map.", "Navigation >: The goal has been reached"], Outcome::Over)
        );
        // Another map: the route's first step, a warp portal here.
        let by_portal = ["Navigation >: A route was found.", "Navigation >: Use the Warp Portal to move to the next area."];
        assert_eq!(report("prt_fild08", [10, 10], &cell, 0, true), said(&by_portal, Outcome::Guiding));
        assert_eq!(report("prt_fild08", [10, 10], &cell, 0, false), said(&by_portal, Outcome::Guiding));
        // The Kafra: who to talk to and where she sends you. The airship: its own line.
        let izlude = Target { map: "izlude".into(), cell: None, monster: None, name: String::new() };
        let by_kafra = ["Navigation >: A route was found.", "Navigation >: Talk to [Kafra Employee]", "Navigation >: Please move to [ Izlude[ izlude ] ]."];
        assert_eq!(report("prontera", [10, 10], &izlude, 100, true), said(&by_kafra, Outcome::Guiding));
        let inside = Target { map: "prt_in".into(), cell: Some([30, 40]), monster: None, name: String::new() };
        let by_airship = ["Navigation >: A route was found.", "Navigation >: Talk to [Airship Staff]", "Navigation >: Please move to the Airship."];
        assert_eq!(report("izlude", [10, 10], &inside, 1, true), said(&by_airship, Outcome::Guiding));
        // No way (the airship not allowed), and a map the tables don't have.
        assert_eq!(report("izlude", [10, 10], &inside, 0, true), said(&["Navigation >: Failed to find a path."], Outcome::NoWay));
        assert_eq!(report("nowhere", [10, 10], &inside, 111, false), said(&["Navigation >: Map doesn't support directions"], Outcome::NoWay));
        // A map or a monster: over on its map.
        assert_eq!(
            report("izlude", [1, 1], &izlude, 0, false),
            said(&["Arrived at the target map.", "Navigation >: Arrived at the target map."], Outcome::Over)
        );
        let poring = Target { map: "prt_fild08".into(), cell: None, monster: Some(1002), name: String::new() };
        assert_eq!(report("prt_fild08", [1, 1], &poring, 0, false), said(&["Arrived at the target map.", "Navigation >: The mob's map."], Outcome::Over));
        assert_eq!(unclear(&messages), "Navigation >: Invalid syntax.");
        // Where it heads: the cell on its own map, the first way's cell on another, nowhere once it's over.
        let heads =
            [found("prontera", [100, 100], &cell, 0, true), found("prt_fild08", [10, 10], &cell, 0, true), found("prontera", [150, 150], &cell, 0, false)];
        assert_eq!(heads.map(|found| found.next), [Some([150, 150]), Some([170, 375]), None]);
        // A cell on this map that can't be walked to from here: out by a way that can, and back in.
        let walled = Walks { from_player: &|at| (at != [150, 150]).then_some(9), to_target: &|_| Some(1) };
        let round = super::report(&tables, &messages, "prontera", [100, 100], &cell, 0, true, &walled);
        assert_eq!((round.lines[0].as_str(), round.outcome, round.next), ("Navigation >: A route was found.", Outcome::Guiding, Some([156, 22])));
        // With no way out that comes back: no way.
        let stuck = Walks { from_player: &|_| None, to_target: &|_| Some(1) };
        assert_eq!(super::report(&tables, &messages, "prontera", [100, 100], &cell, 0, true, &stuck).outcome, Outcome::NoWay);
    }

    #[test]
    fn ends_on_a_move_near_the_cell() {
        let tables = tables();
        let target = Target { map: "prontera".into(), cell: Some([150, 150]), monster: None, name: String::new() };
        // Five cells on both axes, on its map only.
        assert!(reached(&tables, "prontera.gat", [155, 145], &target) && !reached(&tables, "prontera", [156, 150], &target));
        assert!(!reached(&tables, "prontera", [150, 144], &target) && !reached(&tables, "prt_fild08", [150, 150], &target));
        // A map or a monster never ends by a move.
        assert!(!reached(&tables, "prontera", [150, 150], &Target { cell: None, ..target }));
    }

    #[test]
    fn keeps_where_it_heads_for_the_map_it_was_found_on() {
        let target = Target { map: "prontera".into(), cell: Some([150, 150]), monster: None, name: String::new() };
        let mut active = Active::new(target, 111);
        assert_eq!(active.next_cell("prt_fild08"), None);
        active.head_for("prt_fild08.gat", Some([170, 375]));
        assert_eq!((active.next_cell("prt_fild08"), active.next_cell("prontera")), (Some([170, 375]), None));
        active.head_for("prontera", Some([150, 150]));
        assert_eq!((active.next_cell("prontera.gat"), active.next_cell("prt_fild08")), (Some([150, 150]), None));
    }

    #[test]
    fn takes_a_copy_map_for_the_map_it_copies() {
        let mut tables = tables();
        tables.maps.extend(
            [("iz_ac01", "Academy"), ("iz_ac02", "Academy 2"), ("rag_fes", "Festival"), ("rag_fes_a", "Festival A")]
                .map(|(map, shown)| (map.to_owned(), shown.to_owned())),
        );
        tables.links.extend([link("iz_ac01", 200, [100, 39], "iz_ac02"), link("rag_fes_a", 200, [10, 10], "prontera")]);
        // From the copy: the warp the map it copies has; on the target map's copy: the target's cell, and there.
        let chuck = Target { map: "iz_ac02".into(), cell: Some([148, 110]), monster: None, name: String::new() };
        let messages = messages();
        let next = |here: &str| report(&tables, &messages, here, [1, 1], &chuck, 0, true, &OPEN).next;
        assert_eq!(route(&tables, "iz_ac01_a", "iz_ac02", 0, &OPEN).unwrap().len(), 1);
        assert_eq!((next("iz_ac01_a.gat"), next("iz_ac02_a")), (Some([100, 39]), Some([148, 110])));
        assert!(reached(&tables, "iz_ac02_a", [148, 112], &chuck) && !reached(&tables, "iz_ac02_a", [148, 116], &chuck));
        assert!(reached(&tables, "iz_ac02", [148, 112], &Target { map: "iz_ac02_b".into(), ..chuck.clone() }));
        // A map the tables have under its own name keeps it; a copy of a map they don't have stays itself.
        assert_eq!(
            (navi_map(&tables, "rag_fes_a"), navi_map(&tables, "geffen_a"), navi_map(&tables, "iz_ac01_ab")),
            ("rag_fes_a".into(), "geffen_a".into(), "iz_ac01_ab".into())
        );
        assert_eq!(route(&tables, "rag_fes_a", "prontera", 0, &OPEN).unwrap()[0].cell, [10, 10]);
        assert!(!reached(&tables, "rag_fes_a", [1, 1], &Target { map: "rag_fes".into(), cell: Some([1, 1]), monster: None, name: String::new() }));
    }

    #[test]
    fn keeps_the_trail_while_its_in_sight() {
        let target = Target { map: "prontera".into(), cell: Some([150, 150]), monster: None, name: String::new() };
        let mut active = Active::new(target, 111);
        let made = std::cell::Cell::new(0);
        // A straight walk east, one cell a step.
        let path = |from: [i32; 2], to: [i32; 2]| {
            made.set(made.get() + 1);
            Some((from[0] + 1..=to[0]).map(|x| [x, from[1]]).collect())
        };
        // Made once, from the player's own cell on; walking along it doesn't cut it or make it again.
        active.update_trail("prontera", [140, 150], Some([150, 150]), path);
        active.update_trail("prontera.gat", [145, 150], Some([150, 150]), path);
        assert_eq!((made.get(), active.trail.len(), active.trail[0], active.trail[10]), (1, 11, [140, 150], [150, 150]));
        // Nine cells off its nearest arrow it's still in sight; ten off, it's made again from there.
        active.update_trail("prontera", [140, 141], Some([150, 150]), path);
        assert_eq!(made.get(), 1);
        active.update_trail("prontera", [130, 150], Some([150, 150]), path);
        assert_eq!((made.get(), active.trail[0], active.trail.len()), (2, [130, 150], 21));
        // Another cell to head for: made again. One that can't be made stays empty, and isn't tried again.
        active.update_trail("prontera", [130, 150], Some([160, 150]), path);
        assert_eq!((made.get(), active.trail.len()), (3, 31));
        let none = |_: [i32; 2], _: [i32; 2]| {
            made.set(made.get() + 1);
            None
        };
        active.update_trail("prontera", [130, 150], Some([170, 150]), none);
        active.update_trail("prontera", [131, 150], Some([170, 150]), none);
        assert_eq!((made.get(), active.trail.is_empty()), (4, true));
        active.update_trail("prontera", [131, 150], None, path);
        assert!(active.trail.is_empty());
    }

    #[test]
    fn puts_arrows_on_every_second_cell_in_sight() {
        // East along y = 50 from x = 100, then north-east for the last step.
        let mut trail: Vec<[i32; 2]> = (100..=124).map(|x| [x, 50]).collect();
        trail.push([125, 51]);
        let cells = |player: [i32; 2], ms: u128| arrows(&trail, player, ms);
        // From the trail's start: cells 100, 102, … up to nine cells off; all point east (6), the first is lit at 0 ms.
        let near = cells([100, 50], 0);
        assert_eq!(near.iter().map(|arrow| arrow.0[0]).collect::<Vec<_>>(), [100, 102, 104, 106, 108]);
        assert!(near.iter().all(|arrow| arrow.1 == 6) && near[0].2 && near[1..].iter().all(|arrow| !arrow.2));
        // 200 ms on the next arrow is the lit one; in between none is. Five arrows apart, two are lit at once.
        assert_eq!(cells([100, 50], 200).iter().map(|arrow| arrow.2).collect::<Vec<_>>(), [false, true, false, false, false]);
        assert!(cells([100, 50], 100).iter().all(|arrow| !arrow.2));
        assert_eq!(cells([105, 50], 0).iter().filter(|arrow| arrow.2).map(|arrow| arrow.0[0]).collect::<Vec<_>>(), [100, 110]);
        // The last arrow is on the cell before the end and turns with its step; the end itself has none, and off to the
        // side past nine cells nothing shows.
        assert_eq!(cells([125, 51], 0).last(), Some(&([124, 50], 7, false)));
        assert!(cells([110, 60], 0).is_empty() && !cells([110, 59], 0).is_empty());
        assert!(arrows(&[[5, 5]], [5, 5], 0).is_empty() && arrows(&[], [5, 5], 0).is_empty());
        // The marker sits on the end while that's in sight, and turns every 150 ms.
        assert_eq!((marker(&trail, [116, 50]), marker(&trail, [115, 50]), marker(&[], [0, 0])), (Some([125, 51]), None, None));
        assert_eq!([0, 149, 150, 599, 600].map(marker_frame), [0, 0, 1, 3, 0]);
    }

    #[test]
    fn picks_results_in_the_window() {
        let mut window = Window::new();
        window.text = "pront".into();
        window.search(&tables());
        let boxes = window.button_boxes();
        assert_eq!(boxes[0].0, "navi_field");
        let [x, y, w, h] = boxes[2].1;
        assert_eq!(window.hit([x + w / 2.0, y + h / 2.0]), Some(Hit::Result(1)));
        let [x, y, w, h] = boxes.last().unwrap().1;
        assert_eq!((boxes.last().unwrap().0.as_str(), window.hit([x + w / 2.0, y + h / 2.0])), ("navi_stop", Some(Hit::Stop)));
    }

    #[test]
    fn says_what_each_result_picks_and_whether_the_list_was_cut() {
        let mut tables = tables();
        let mut window = Window::new();
        window.text = "tool".into();
        window.search(&tables);
        let rows: Vec<_> = window.button_boxes().into_iter().filter_map(|(name, _, target)| Some((name, target?.clone()))).collect();
        assert_eq!(
            rows,
            [("navi_result_0".to_owned(), Target { map: "prt_in".into(), cell: Some([30, 40]), monster: None, name: "Tool Dealer (prt_in)".into() })]
        );
        assert_eq!((rows[0].1.kind(), window.more), ("cell", false));
        assert_eq!([search(&tables, "pront", 1)[0].kind(), search(&tables, "poring", 1)[0].kind()], ["map", "monster"]);
        // One more match than the rows: the rows stay full and the window says there's more.
        tables.maps.extend((0..ROWS).map(|index| (format!("prt_fild{index:02}"), format!("Prontera Field {index}"))));
        window.text = "pront".into();
        window.search(&tables);
        assert_eq!((window.results.len(), window.more), (ROWS, true));
        // Exactly as many as the rows: not cut.
        tables.maps.retain(|map, _| !["prt_in", "prontera", "prt_fild08"].contains(&map.as_str()));
        window.search(&tables);
        assert_eq!((window.results.len(), window.more), (ROWS, false));
    }

    #[test]
    fn turns_the_arrow_with_the_step() {
        // North is 0, then anticlockwise: north-west, west, south-west, south, south-east, east, north-east.
        let turns = [[0, 1], [-1, 1], [-1, 0], [-1, -1], [0, -1], [1, -1], [1, 0], [1, 1]].map(|[x, y]| step_turn([10, 10], [10 + x, 10 + y]));
        assert_eq!(turns, [0, 1, 2, 3, 4, 5, 6, 7]);
    }

    #[test]
    fn finds_maps_npcs_and_monsters() {
        let tables = tables();
        let names = |text: &str| search(&tables, text, 10).into_iter().map(|target| target.name).collect::<Vec<_>>();
        assert_eq!(names("pront"), ["Prontera (prontera)", "Prontera Field (prt_fild08)", "Prontera Inside (prt_in)"]);
        assert_eq!(names("TOOL"), ["Tool Dealer (prt_in)"]);
        // A monster once per map, its target the map.
        assert_eq!(search(&tables, "poring", 10), [Target { map: "prt_fild08".into(), cell: None, monster: Some(1002), name: "Poring (prt_fild08)".into() }]);
        assert!(search(&tables, " ", 10).is_empty());
    }
}
