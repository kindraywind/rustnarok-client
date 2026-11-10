//! Navigation: a route to a map, a cell, an NPC or a monster across maps, by the client's navigation tables
//! (lua::NaviTables). It starts from the server (ZC_NAVIGATION_ACTIVE), a `<NAVI>` link, the navigation window or a typed
//! `/navi prontera 150/150` (the window itself is `ui/navi_window.rs`), says where it stands in the chat, and shows as arrows on the ground along the way to the
//! next warp or the target, with a marker on the cell it heads for.

use std::collections::{HashMap, HashSet};

use rustnarok_client::lua::{NaviLink, NaviTables};
use rustnarok_client::messages::fill;

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
const NO_INFO: usize = 2300;
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
    #[cfg(test)]
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

/// The target a start names by its type: 0 a cell of the map, 1 the map itself, 2 the NPC standing on that very cell,
/// 3 a monster of the map by its id. Err with the message to say ("No Information") when the tables have no such NPC
/// or monster there, and with none (nothing at all happens) for any other type or a map the tables don't have.
pub fn target_of(tables: &NaviTables, map: &str, kind: u8, cell: [u16; 2], monster: u16) -> Result<Target, Option<usize>> {
    let map = navi_map(tables, map);
    let shown = tables.maps.get(&map).ok_or(None)?.clone();
    let named = |name: &str| format!("{name} ({map})");
    match kind {
        0 => Ok(Target { cell: Some(cell), monster: None, name: shown, map }),
        1 => Ok(Target { cell: None, monster: None, name: shown, map }),
        2 => {
            let npc = tables.npcs.iter().find(|npc| npc.map == map && npc.cell == cell).ok_or(Some(NO_INFO))?;
            Ok(Target { cell: Some(cell), monster: None, name: named(&npc.name), map: map.clone() })
        }
        3 => {
            let found = tables.monsters.iter().find(|found| found.map == map && found.id == monster).ok_or(Some(NO_INFO))?;
            Ok(Target { cell: None, monster: Some(monster), name: named(&found.name), map: map.clone() })
        }
        _ => Err(None),
    }
}

/// What a `<NAVI>` link asks for.
#[derive(Debug, PartialEq)]
pub enum Linked {
    /// A navigation to its place, read by this type (`target_of`), by these services, with the window opened or not;
    /// the monster's id for type 3.
    Go { kind: u8, services: u8, window: bool, monster: u16 },
    /// The window, looking the map's name up.
    Search,
}

/// A link's fields after its map, x and y (`rest`: type, options, window, id; none on a link that ends at y). A short
/// link is the NPC on that cell, by the Kafra and the airship, with the window. Options are a number whose hundreds
/// allow the Kafra, tens scrolls and ones the airship; −222 asks for a search instead.
pub fn linked(rest: Option<[i32; 4]>) -> Option<Linked> {
    let Some([kind, options, window, id]) = rest else { return Some(Linked::Go { kind: 2, services: 101, window: true, monster: 0 }) };
    if options == -222 {
        return Some(Linked::Search);
    }
    let (kafra, left) = if options >= 100 { (1, options - 100) } else { (0, options) };
    let (scroll, left) = if left >= 10 { (1, left - 10) } else { (0, left) };
    let services = kafra * 100 + scroll * 10 + u8::from(left >= 1);
    Some(Linked::Go { kind: u8::try_from(kind).ok()?, services, window: window != 0, monster: id.clamp(0, i32::from(u16::MAX)) as u16 })
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

/// One thing a search found: a map (kind 1), an NPC (2) or a monster (3), as `target_of` numbers them.
#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    pub kind: u8,
    pub map: String,
    pub cell: [u16; 2],
    pub monster: u16,
    /// What it's sorted and grouped by: the map's own name, the NPC's, the monster's.
    pub name: String,
    /// How many of a monster its map spawns.
    pub spawns: u32,
    /// A monster listed because its map matched: it goes in the map's group.
    pub with_map: bool,
}

/// What the navigation window finds for `text`: every word of it (split at spaces) must be in the name, in any case.
/// A map's name is its own and its shown one (the insides, "_in" maps, are left out, but for gl_in01), and a map found
/// brings its monsters; an NPC's and a monster's is its own. Sorted by name, last first.
pub fn search(tables: &NaviTables, text: &str) -> Vec<Found> {
    let words: Vec<String> = text.split(' ').filter(|word| !word.is_empty()).map(str::to_lowercase).collect();
    if words.is_empty() {
        return Vec::new();
    }
    let has = |name: &str| words.iter().all(|word| name.to_lowercase().contains(word.as_str()));
    let monster = |found: &rustnarok_client::lua::NaviMonster, with_map| Found {
        kind: 3,
        map: found.map.clone(),
        cell: [0, 0],
        monster: found.id,
        name: found.name.clone(),
        spawns: found.spawns,
        with_map,
    };
    // A map's row twice over (the table lists some) is one.
    let mut seen = HashSet::new();
    let monsters: Vec<_> = tables.monsters.iter().filter(|found| seen.insert((found.map.as_str(), found.id))).collect();
    let mut maps: Vec<(&String, &String)> = tables.maps.iter().filter(|(map, _)| !map.contains("_in") || *map == "gl_in01").collect();
    maps.sort();
    let mut found = Vec::new();
    for (map, _) in maps.into_iter().filter(|(map, shown)| has(&format!("{map} {shown} "))) {
        found.push(Found { kind: 1, map: map.clone(), cell: [0, 0], monster: 0, name: map.clone(), spawns: 0, with_map: false });
        found.extend(monsters.iter().filter(|found| found.map == *map).map(|found| monster(found, true)));
    }
    found.extend(tables.npcs.iter().filter(|npc| has(&npc.name)).map(|npc| Found {
        kind: 2,
        map: npc.map.clone(),
        cell: npc.cell,
        monster: 0,
        name: npc.name.clone(),
        spawns: 0,
        with_map: false,
    }));
    found.extend(monsters.iter().filter(|found| has(&found.name)).map(|found| monster(found, false)));
    found.sort_by(|first, second| second.name.cmp(&first.name));
    found
}

/// The rows of the window's left list, each the places (by their index in `found`) it stands for, by the type filter
/// (0 all, 1 maps, 2 NPCs, 3 monsters). With all or maps, each map is a row of its own with the monsters it brought,
/// first; then what else the filter lets through, one row a kind and name.
pub fn groups(found: &[Found], filter: usize) -> Vec<Vec<usize>> {
    let mut rows: Vec<Vec<usize>> = Vec::new();
    let mut placed = vec![false; found.len()];
    if filter <= 1 {
        for (index, map) in found.iter().enumerate().filter(|(_, place)| place.kind == 1) {
            let brought = found.iter().enumerate().filter(|(_, place)| place.with_map && place.map == map.map).map(|(other, _)| other);
            let row: Vec<usize> = std::iter::once(index).chain(brought).collect();
            row.iter().for_each(|&member| placed[member] = true);
            rows.push(row);
        }
    }
    for (index, place) in found.iter().enumerate() {
        if placed[index] || !(filter == 0 || usize::from(place.kind) == filter) || place.kind == 1 {
            continue;
        }
        match rows.iter_mut().find(|row| (found[row[0]].kind, &found[row[0]].name) == (place.kind, &place.name)) {
            Some(row) => row.push(index),
            None => rows.push(vec![index]),
        }
    }
    rows
}

/// How thick a monster is on its map, in the game's words (none when the tables give no count).
pub fn density(spawns: u32) -> Option<usize> {
    Some(match spawns {
        0 => return None,
        1..=4 => 2453,
        5..=19 => 2452,
        20..=49 => 2451,
        50..=99 => 2450,
        _ => 2449,
    })
}

/// A left row's words: how many places it stands for, and their name.
pub fn group_label(found: &[Found], row: &[usize]) -> String {
    format!("[{}]{}", row.len(), found[row[0]].name)
}

/// A right row's words, for the place `member` of the row `row`: a map's shown name; an NPC's map and cell; a
/// monster's map (in a map's row: its own name) and how thick it is there.
pub fn member_label(tables: &NaviTables, messages: &[String], found: &[Found], row: &[usize], member: usize) -> String {
    let place = &found[row[member]];
    let thick = || density(place.spawns).and_then(|id| messages.get(id)).map_or(String::new(), |word| format!(" ({word})"));
    match place.kind {
        1 => tables.maps.get(&place.map).cloned().unwrap_or_default(),
        2 => format!("{} [{},{}]", place.map, place.cell[0], place.cell[1]),
        _ if found[row[0]].kind == 1 => format!("{}{}", place.name, thick()),
        _ => format!("{}{}", place.map, thick()),
    }
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
                NaviMonster { map: "prt_fild08".into(), id: 1002, name: "Poring".into(), spawns: 70 },
                NaviMonster { map: "prt_fild08".into(), id: 1002, name: "Poring".into(), spawns: 70 },
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
        let mut messages = vec![String::new(); 2454];
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
            (2449, "Very High"),
            (2450, "High"),
            (2453, "Very Low"),
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
    fn names_a_target_by_its_type() {
        let tables = tables();
        let kind = |target: Result<Target, Option<usize>>| target.map(|target| (target.kind(), target.cell, target.monster, target.name));
        // A cell and the map itself, named as the map is shown.
        assert_eq!(kind(target_of(&tables, "Prontera.gat", 0, [150, 150], 0)), Ok(("cell", Some([150, 150]), None, "Prontera".into())));
        assert_eq!(kind(target_of(&tables, "prontera", 1, [150, 150], 0)), Ok(("map", None, None, "Prontera".into())));
        // The NPC on that very cell, or "No Information"; a monster of the map by its id, or the same.
        assert_eq!(kind(target_of(&tables, "prt_in", 2, [30, 40], 0)), Ok(("cell", Some([30, 40]), None, "Tool Dealer (prt_in)".into())));
        assert_eq!(target_of(&tables, "prt_in", 2, [30, 41], 0), Err(Some(NO_INFO)));
        assert_eq!(kind(target_of(&tables, "prt_fild08", 3, [0, 0], 1002)), Ok(("monster", None, Some(1002), "Poring (prt_fild08)".into())));
        assert_eq!(target_of(&tables, "prontera", 3, [0, 0], 1002), Err(Some(NO_INFO)));
        // Any other type, or a map the tables don't have: nothing.
        assert_eq!((target_of(&tables, "prontera", 4, [1, 1], 0), target_of(&tables, "nowhere", 2, [1, 1], 0)), (Err(None), Err(None)));
    }

    #[test]
    fn reads_a_links_fields() {
        let go = |kind, services, window, monster| Some(Linked::Go { kind, services, window, monster });
        // It ends at y: the NPC there, by the Kafra and the airship, with the window.
        assert_eq!(linked(None), go(2, 101, true, 0));
        // rathena's own: a cell, no services, no window.
        assert_eq!(linked(Some([0, 0, 0, 0])), go(0, 0, false, 0));
        // The options' hundreds, tens and ones; the window; a monster's id.
        assert_eq!(linked(Some([0, 101, 1, 0])), go(0, 101, true, 0));
        assert_eq!(linked(Some([3, 111, 0, 1002])), go(3, 111, false, 1002));
        assert_eq!(
            [linked(Some([1, 10, 0, 0])), linked(Some([1, 1, 0, 0])), linked(Some([1, 100, 0, 0]))],
            [go(1, 10, false, 0), go(1, 1, false, 0), go(1, 100, false, 0)]
        );
        assert_eq!(linked(Some([1, -5, 2, -3])), go(1, 0, true, 0));
        // −222: a search. A type that's no type: nothing.
        assert_eq!((linked(Some([3, -222, 0, 0])), linked(Some([-1, 0, 0, 0]))), (Some(Linked::Search), None));
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
    fn turns_the_arrow_with_the_step() {
        // North is 0, then anticlockwise: north-west, west, south-west, south, south-east, east, north-east.
        let turns = [[0, 1], [-1, 1], [-1, 0], [-1, -1], [0, -1], [1, -1], [1, 0], [1, 1]].map(|[x, y]| step_turn([10, 10], [10 + x, 10 + y]));
        assert_eq!(turns, [0, 1, 2, 3, 4, 5, 6, 7]);
    }

    #[test]
    fn finds_maps_npcs_and_monsters() {
        let mut tables = tables();
        tables.monsters.push(NaviMonster { map: "izlude".into(), id: 1002, name: "Poring".into(), spawns: 3 });
        tables.monsters.push(NaviMonster { map: "prt_fild08".into(), id: 1007, name: "Fabre".into(), spawns: 0 });
        let names = |text: &str| search(&tables, text).into_iter().map(|found| (found.kind, found.name, found.map)).collect::<Vec<_>>();
        let place = |kind: u8, name: &str, map: &str| (kind, name.to_owned(), map.to_owned());
        // A map by its own name or its shown one, any case, with its monsters; the insides are left out. Last name first.
        assert_eq!(names("FIELD"), [place(1, "prt_fild08", "prt_fild08"), place(3, "Poring", "prt_fild08"), place(3, "Fabre", "prt_fild08")]);
        assert_eq!(
            names("pront"),
            [place(1, "prt_fild08", "prt_fild08"), place(1, "prontera", "prontera"), place(3, "Poring", "prt_fild08"), place(3, "Fabre", "prt_fild08")]
        );
        // Every word must be there; an NPC and a monster by their own names only (not their map's).
        assert_eq!(names("tool deal"), [place(2, "Tool Dealer", "prt_in")]);
        assert!(names("tool prt_in").is_empty() && names("prt_in").is_empty() && names("  ").is_empty());
        // A monster once per map, however often the table lists it.
        assert_eq!(names("poring"), [place(3, "Poring", "prt_fild08"), place(3, "Poring", "izlude")]);
    }

    #[test]
    fn groups_what_it_found() {
        let (mut tables, messages) = (tables(), messages());
        tables.monsters.push(NaviMonster { map: "izlude".into(), id: 1002, name: "Poring".into(), spawns: 3 });
        tables.npcs.push(NaviNpc { map: "izlude".into(), name: "Tool Dealer".into(), cell: [10, 20] });
        // "o" finds two maps, two NPCs and the Porings: by name and with their map.
        let found = search(&tables, "o");
        let rows = |filter: usize| groups(&found, filter).iter().map(|row| group_label(&found, row)).collect::<Vec<_>>();
        // All: each map with the monsters it brought, then a row a name.
        assert_eq!(rows(0), ["[2]prt_fild08", "[1]prontera", "[2]Tool Dealer", "[2]Poring"]);
        assert_eq!(rows(1), ["[2]prt_fild08", "[1]prontera"]);
        assert_eq!(rows(2), ["[2]Tool Dealer"]);
        // Monsters only: the ones their map brought too, one row a name.
        assert_eq!(rows(3), ["[3]Poring"]);
        // The right list: a map's shown name and its monster with how thick it is; an NPC's map and cell; a monster's map.
        let all = groups(&found, 0);
        let members = |row: usize| (0..all[row].len()).map(|member| member_label(&tables, &messages, &found, &all[row], member)).collect::<Vec<_>>();
        assert_eq!(members(0), ["Prontera Field", "Poring (High)"]);
        assert_eq!(members(2), ["prt_in [30,40]", "izlude [10,20]"]);
        assert_eq!(members(3), ["prt_fild08 (High)", "izlude (Very Low)"]);
        assert_eq!(
            [0, 1, 4, 5, 19, 20, 49, 50, 99, 100].map(density),
            [None, Some(2453), Some(2453), Some(2452), Some(2452), Some(2451), Some(2451), Some(2450), Some(2450), Some(2449)]
        );
    }
}
