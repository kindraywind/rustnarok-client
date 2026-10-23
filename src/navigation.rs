//! Navigation: a route to a map, a cell, an NPC or a monster across maps, by the client's navigation tables
//! (lua::NaviTables). It starts from the server (ZC_NAVIGATION_ACTIVE), a `<NAVI>` link or the navigation window, and
//! shows as an arrow over the player's head and a trail on the minimap to the next warp or the target.

use std::collections::{HashMap, HashSet, VecDeque};

use rustnarok_client::lua::{NaviLink, NaviTables};
use rustnarok_client::text::Font;

use crate::Layer;
use crate::window::{Chrome, Frame, TITLE_HEIGHT, TitleHit};

/// How near the target cell counts as there, in cells.
// ponytail: the client's own arrival distance isn't known; two cells.
pub const ARRIVED: i32 = 2;
/// The minimap trail: a dot every this many cells.
pub const TRAIL_STEP: usize = 3;

/// Where to go: a map, and a cell on it or a monster there (or neither: the map itself).
#[derive(Debug, Clone, PartialEq)]
pub struct Target {
    pub map: String,
    pub cell: Option<[u16; 2]>,
    pub monster: Option<u16>,
    /// What the target is called, for the window.
    pub name: String,
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

/// The ways to take from `from` to `to`, fewest maps first (ties: the tables' order); empty when already there.
pub fn route<'a>(tables: &'a NaviTables, from: &str, to: &str, services: u8) -> Option<Vec<&'a NaviLink>> {
    let (from, to) = (navi_map(tables, from), navi_map(tables, to));
    if from == to {
        return Some(Vec::new());
    }
    let mut ways: HashMap<&str, Vec<&NaviLink>> = HashMap::new();
    for link in tables.links.iter().filter(|link| allowed(link.kind, services)) {
        ways.entry(link.map.as_str()).or_default().push(link);
    }
    let mut came: HashMap<&str, &NaviLink> = HashMap::new();
    let mut seen: HashSet<&str> = HashSet::from([from.as_str()]);
    let mut queue = VecDeque::from([from.as_str()]);
    while let Some(map) = queue.pop_front() {
        for &link in ways.get(map).into_iter().flatten() {
            if !seen.insert(link.to.as_str()) {
                continue;
            }
            came.insert(link.to.as_str(), link);
            if link.to == to {
                let mut path = vec![link];
                while let Some(&back) = came.get(path.last().unwrap().map.as_str()) {
                    path.push(back);
                }
                path.reverse();
                return Some(path);
            }
            queue.push_back(link.to.as_str());
        }
    }
    None
}

/// Where to head on `here`: the first way off this map on the route, or the target's cell on its own map (None for a
/// map or monster target there: that's arrived, as is a map with no route).
pub fn next_cell(tables: &NaviTables, here: &str, target: &Target, services: u8) -> Option<[u16; 2]> {
    if navi_map(tables, here) == navi_map(tables, &target.map) {
        return target.cell;
    }
    route(tables, here, &target.map, services)?.first().map(|link| link.cell)
}

/// Whether the player at `cell` on `here` has got there.
pub fn arrived(tables: &NaviTables, here: &str, cell: [i32; 2], target: &Target) -> bool {
    navi_map(tables, here) == navi_map(tables, &target.map)
        && target.cell.is_none_or(|goal| (cell[0] - i32::from(goal[0])).abs().max((cell[1] - i32::from(goal[1])).abs()) <= ARRIVED)
}

/// The arrow (navi_grid_0..7: south, south-west, west, north-west, north, north-east, east, south-east) for a screen
/// direction `delta` (x right, y down).
pub fn arrow(delta: [f32; 2]) -> usize {
    // Clockwise from straight down, in eighths.
    let angle = (-delta[0]).atan2(delta[1]).rem_euclid(std::f32::consts::TAU);
    (angle / (std::f32::consts::TAU / 8.0)).round() as usize % 8
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

/// A navigation under way: its target, the services it may use, and the trail on this map to where it heads next.
pub struct Active {
    pub target: Target,
    pub services: u8,
    pub trail: Vec<[i32; 2]>,
    /// The map, the player's cell and the cell headed for that the trail was made for.
    trail_for: Option<(String, [i32; 2], [u16; 2])>,
    /// The map `next_cell` was found for and what it found: the route only changes with the map.
    next_for: Option<(String, Option<[u16; 2]>)>,
}

impl Active {
    pub fn new(target: Target, services: u8) -> Self {
        Self { target, services, trail: Vec::new(), trail_for: None, next_for: None }
    }

    /// `next_cell` on `here`, found again only when the map changed.
    pub fn next_cell(&mut self, tables: &NaviTables, here: &str) -> Option<[u16; 2]> {
        let here = map_key(here);
        match &self.next_for {
            Some((map, next)) if *map == here => *next,
            _ => {
                let next = next_cell(tables, &here, &self.target, self.services);
                self.next_for = Some((here, next));
                next
            }
        }
    }

    /// Makes the trail again (with `path`, the cells walked from one cell to another) when the map, the player's cell or
    /// the cell headed for changed.
    pub fn update_trail(&mut self, map: &str, cell: [i32; 2], next: Option<[u16; 2]>, path: impl FnOnce([i32; 2], [i32; 2]) -> Option<Vec<[i32; 2]>>) {
        let Some(next) = next else {
            self.trail.clear();
            self.trail_for = None;
            return;
        };
        let key = (map_key(map), cell, next);
        if self.trail_for.as_ref() != Some(&key) {
            self.trail = path(cell, [i32::from(next[0]), i32::from(next[1])]).unwrap_or_default();
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

/// The navigation window: what's typed, what it finds, and whether typing goes to it.
pub struct Window {
    pub frame: Frame,
    pub text: String,
    pub typing: bool,
    pub results: Vec<Target>,
}

impl Window {
    pub fn new() -> Self {
        // ponytail: where it opens; right of the middle, clear of the player.
        let mut frame = Frame::new([680.0, 180.0], WIDTH);
        frame.open = true;
        Self { frame, text: String::new(), typing: true, results: Vec::new() }
    }

    /// Searches again for what's typed.
    pub fn search(&mut self, tables: &NaviTables) {
        self.results = search(tables, &self.text, ROWS);
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

    /// The results' rows and the stop row on screen (for the test driver).
    pub fn button_boxes(&self) -> Vec<(String, [f32; 4])> {
        if !self.frame.open || self.frame.minimized {
            return Vec::new();
        }
        let area = |top: f32| {
            let [x, y] = self.frame.at([LEFT, top]);
            [x, y, WIDTH - 2.0 * LEFT, ROW]
        };
        let mut boxes: Vec<(String, [f32; 4])> = vec![("navi_field".to_owned(), area(FIELD_TOP))];
        boxes.extend((0..self.results.len()).map(|index| (format!("navi_result_{index}"), area(RESULTS_TOP + index as f32 * ROW))));
        boxes.push(("navi_stop".to_owned(), area(STOP_TOP)));
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
            let name = crate::window::fit(font, &target.name, WIDTH - 2.0 * LEFT);
            layer.texts.push((name, at([LEFT, RESULTS_TOP + index as f32 * ROW]), color));
        }
        // The stop row while a navigation runs: what it goes to.
        let stop = match active {
            Some(target) => format!("■ {}", crate::window::fit(font, &target.name, WIDTH - 2.0 * LEFT - 20.0)),
            None => String::new(),
        };
        layer.texts.push((stop, at([LEFT, STOP_TOP]), if active.is_some() { PICKED } else { GREY }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustnarok_client::lua::{NaviMonster, NaviNpc};

    fn link(map: &str, kind: u32, cell: [u16; 2], to: &str) -> NaviLink {
        NaviLink { map: map.into(), kind, name: String::new(), cell, to: to.into(), to_cell: [0, 0] }
    }

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
        }
    }

    #[test]
    fn routes_across_maps() {
        let tables = tables();
        // Two maps on: the field's warp to Prontera, then Prontera's to the inside.
        let path = route(&tables, "prt_fild08.gat", "prt_in", 0).unwrap();
        assert_eq!(path.iter().map(|link| (link.map.as_str(), link.cell)).collect::<Vec<_>>(), [("prt_fild08", [170, 375]), ("prontera", [100, 50])]);
        assert_eq!(route(&tables, "prontera", "prontera", 0), Some(Vec::new()));
        // Prontera to Izlude: the Kafra (204) only with the Kafra allowed; else round by the field.
        assert_eq!(route(&tables, "prontera", "izlude", 100).unwrap().len(), 1);
        assert_eq!(route(&tables, "prontera", "izlude", 0).unwrap().iter().map(|link| link.map.as_str()).collect::<Vec<_>>(), ["prontera", "prt_fild08"]);
        assert_eq!(route(&tables, "prt_in", "izlude", 111), None);
    }

    #[test]
    fn heads_for_the_next_warp_then_the_target() {
        let tables = tables();
        let target = Target { map: "prontera".into(), cell: Some([150, 150]), monster: None, name: String::new() };
        assert_eq!(next_cell(&tables, "prt_fild08", &target, 111), Some([170, 375]));
        assert_eq!(next_cell(&tables, "prontera", &target, 111), Some([150, 150]));
        // A navigation under way finds it once per map, and again on the next one.
        let mut active = Active::new(target.clone(), 111);
        assert_eq!(active.next_cell(&tables, "prt_fild08.gat"), Some([170, 375]));
        assert_eq!(active.next_cell(&tables, "prt_fild08"), Some([170, 375]));
        assert_eq!(active.next_cell(&tables, "prontera.gat"), Some([150, 150]));
        assert!(!arrived(&tables, "prontera", [150, 147], &target) && arrived(&tables, "prontera", [148, 152], &target));
        assert!(!arrived(&tables, "prt_fild08", [150, 150], &target));
        let map_only = Target { cell: None, ..target };
        assert!(arrived(&tables, "prontera.gat", [1, 1], &map_only));
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
        assert_eq!(route(&tables, "iz_ac01_a", "iz_ac02", 0).unwrap().len(), 1);
        assert_eq!(next_cell(&tables, "iz_ac01_a.gat", &chuck, 0), Some([100, 39]));
        assert_eq!(next_cell(&tables, "iz_ac02_a", &chuck, 0), Some([148, 110]));
        assert!(arrived(&tables, "iz_ac02_a", [148, 112], &chuck) && !arrived(&tables, "iz_ac02_a", [148, 115], &chuck));
        assert!(arrived(&tables, "iz_ac02", [148, 112], &Target { map: "iz_ac02_b".into(), ..chuck.clone() }));
        // A map the tables have under its own name keeps it; a copy of a map they don't have stays itself.
        assert_eq!(
            (navi_map(&tables, "rag_fes_a"), navi_map(&tables, "geffen_a"), navi_map(&tables, "iz_ac01_ab")),
            ("rag_fes_a".into(), "geffen_a".into(), "iz_ac01_ab".into())
        );
        assert_eq!(route(&tables, "rag_fes_a", "prontera", 0).unwrap()[0].cell, [10, 10]);
        assert!(!arrived(&tables, "rag_fes_a", [1, 1], &Target { map: "rag_fes".into(), cell: None, monster: None, name: String::new() }));
    }

    #[test]
    fn remakes_the_trail_only_when_needed() {
        let target = Target { map: "prontera".into(), cell: Some([150, 150]), monster: None, name: String::new() };
        let mut active = Active::new(target, 111);
        let made = std::cell::Cell::new(0);
        let path = |from: [i32; 2], to: [i32; 2]| {
            made.set(made.get() + 1);
            Some(vec![from, to])
        };
        active.update_trail("prontera", [140, 140], Some([150, 150]), path);
        active.update_trail("prontera.gat", [140, 140], Some([150, 150]), path);
        assert_eq!((made.get(), active.trail.clone()), (1, vec![[140, 140], [150, 150]]));
        active.update_trail("prontera", [141, 140], Some([150, 150]), path);
        assert_eq!(made.get(), 2);
        active.update_trail("prontera", [141, 140], None, path);
        assert!(active.trail.is_empty());
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
    fn points_the_arrow() {
        // Down the screen is south (0), clockwise in eighths: left is west (2), up north (4), right east (6).
        assert_eq!([arrow([0.0, 10.0]), arrow([-10.0, 0.0]), arrow([0.0, -10.0]), arrow([10.0, 0.0])], [0, 2, 4, 6]);
        assert_eq!([arrow([-7.0, 7.0]), arrow([7.0, -7.0]), arrow([7.0, 7.0])], [1, 5, 7]);
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
