//! The Navigation window: a search over the navigation tables, what it found in two lists (rows of a name on the left,
//! the places of the picked row on the right), and Find, which starts a navigation to the place picked.

use rustnarok_client::grf::Grf;
use rustnarok_client::lua::NaviTables;
use rustnarok_client::text::Font;

use crate::navigation::{self, Found, Target};
use crate::render::{Gpu, Texture};
use crate::window::{Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI, contains, screen};

pub const SIZE: [f32; 2] = [272.0, 338.0];
/// "Navigation"; "Search result[%d]"; "Failed to set info for location!" (Find with nothing picked).
const TITLE: usize = 2221;
const FOUND: usize = 3210;
pub const NOTHING_PICKED: usize = 2297;
/// The type box and its four lines: "All", "Map", "Npc", "Mob".
const FILTER: [f32; 4] = [29.0, 21.0, 61.0, 18.0];
const FILTERS: [usize; 4] = [2206, 2207, 2208, 2209];
/// The search box (30 characters at most) and the Search button beside it.
const FIELD: [f32; 4] = [94.0, 21.0, 113.0, 18.0];
const MOST: usize = 30;
const SEARCH: [f32; 4] = [207.0, 21.0, 25.0, 18.0];
const FOUND_AT: [f32; 2] = [20.0, 48.0];
/// The two lists, in rows of 16: fifteen on the left, five on the right.
const ROW: f32 = 16.0;
const GROUPS: [f32; 4] = [1.0, 70.0, 138.0, 240.0];
const MEMBERS: [f32; 4] = [139.0, 166.0, 132.0, 80.0];
/// A list's scrollbar: this wide, at its right edge.
const BAR: f32 = 7.0;
/// The picked place's name and its map's picture.
const NAME_AT: [f32; 2] = [141.0, 46.0];
const PICTURE: [f32; 4] = [155.0, 60.0, 100.0, 100.0];
/// The "service" box (its label to the right), and Find.
const SERVICE: [f32; 4] = [146.0, 247.0, 110.0, 12.0];
const FIND: [f32; 4] = [207.0, 263.0, 58.0, 20.0];
const BOTTOM: [f32; 2] = [0.0, 314.0];
const GREY: [u8; 3] = [0xCC, 0xCC, 0xCC];
const INK: [u8; 3] = [0, 0, 0];
const PICKED: [u8; 3] = [0x30, 0x50, 0xC0];

pub struct Assets {
    /// The type box and its arrow; the search box; the Search button and Find (normal, under the pointer).
    filter: [Texture; 2],
    field: Texture,
    search: [Texture; 2],
    find: [Texture; 2],
    /// A row's icon: a map, an NPC, a monster.
    icons: [Texture; 3],
    check: [Texture; 2],
    bottom: Texture,
    grey: Texture,
    black: Texture,
    picked: Texture,
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |name: &str| gpu.load_texture(grf, &format!("{UI}navigation_interface3\\{name}.bmp"), false);
        Self {
            filter: ["bg_combo", "dropdown_on"].map(image),
            field: image("bg_search"),
            search: ["btn_searchbar_normal", "btn_searchbar_over"].map(image),
            find: ["btn_find_normal", "btn_find_over"].map(|name| gpu.load_texture(grf, &format!("{UI}private_airplane\\{name}.bmp"), false)),
            icons: ["icon_list_map", "icon_list_npc", "icon_list_mob"].map(image),
            check: ["checkbox_0", "checkbox_1"].map(|name| gpu.load_texture(grf, &format!("{UI}{name}.bmp"), false)),
            bottom: image("bg_down"),
            grey: gpu.solid([GREY[0], GREY[1], GREY[2], 0xFF]),
            black: gpu.solid([0, 0, 0, 0xFF]),
            picked: gpu.solid([0xD8, 0xE0, 0xF8, 0xFF]),
        }
    }
}

/// A map's picture for the window: the map's own, else the one for insides.
// ponytail: the game picks by the map's type in the tables; here it's whichever picture the data has.
pub fn picture(gpu: &Gpu, grf: &Grf, map: &str) -> Option<Texture> {
    let own = gpu.try_load_texture(grf, &format!("{UI}map\\{map}.bmp"), true);
    own.or_else(|_| gpu.try_load_texture(grf, &format!("{UI}navigation_interface\\Interior.bmp"), true)).ok()
}

/// A control or a row on screen, for the test driver: its name and box, and a row's words and place.
pub type Button<'a> = (String, [f32; 4], Option<String>, Option<&'a Found>);

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(TitleHit),
    Filter,
    /// A line of the open type box.
    FilterLine(usize),
    Field,
    Search,
    /// A row of the left list, of the right list: by its place in the whole list.
    Group(usize),
    Member(usize),
    Service,
    Find,
    Window,
}

/// The navigation window: the search's text and type, what it found and the rows it's shown in, the row and the place
/// picked, and whether routes may take paid services.
pub struct Window {
    pub frame: Frame,
    pub text: String,
    pub typing: bool,
    pub filter: usize,
    pub filter_open: bool,
    pub found: Vec<Found>,
    pub rows: Vec<Vec<usize>>,
    pub row: Option<usize>,
    pub member: usize,
    /// The first row each list shows.
    pub first: [usize; 2],
    pub service: bool,
    /// The picked place's map and its picture, loaded when the pick changes.
    pub picture: Option<(String, Option<Texture>)>,
}

impl Window {
    /// In the middle of the screen, with the service box as the navigation under way has it. The search box takes the
    /// keys only once the player asks for the window or presses the box (`typing`).
    pub fn new(service: bool) -> Self {
        let mut frame = Frame::new([((screen()[0] - SIZE[0]) / 2.0).floor(), ((screen()[1] - SIZE[1]) / 2.0).floor()], SIZE[0]);
        frame.open = true;
        Self {
            frame,
            text: String::new(),
            typing: false,
            filter: 0,
            filter_open: false,
            found: Vec::new(),
            rows: Vec::new(),
            row: None,
            member: 0,
            first: [0, 0],
            service,
            picture: None,
        }
    }

    /// Searches for what's typed (nothing typed: nothing happens) and shows the rows from the top, none picked.
    pub fn search(&mut self, tables: &NaviTables) {
        if self.text.trim().is_empty() {
            return;
        }
        self.found = navigation::search(tables, &self.text);
        self.regroup();
    }

    /// The rows again, by the type box as it stands.
    fn regroup(&mut self) {
        self.rows = navigation::groups(&self.found, self.filter);
        (self.row, self.member, self.first) = (None, 0, [0, 0]);
    }

    pub fn type_text(&mut self, text: &str) {
        let room = MOST.saturating_sub(self.text.chars().count());
        self.text.extend(text.chars().filter(|character| !character.is_control()).take(room));
    }

    /// The place picked: a row's place on the right list.
    pub fn picked(&self) -> Option<&Found> {
        self.rows.get(self.row?).and_then(|row| row.get(self.member)).map(|&index| &self.found[index])
    }

    /// The picked place as a navigation's target, or the message that says there is none.
    pub fn target(&self, tables: &NaviTables) -> Result<Target, Option<usize>> {
        let place = self.picked().ok_or(Some(NOTHING_PICKED))?;
        navigation::target_of(tables, &place.map, place.kind, place.cell, place.monster)
    }

    /// The services a route from here may take: the Kafra and the airship with the box ticked, else none.
    pub fn services(&self) -> u8 {
        if self.service { 101 } else { 0 }
    }

    fn shown(list: [f32; 4]) -> usize {
        (list[3] / ROW) as usize
    }

    /// The rows each list holds: the left one's, the picked row's places.
    fn lengths(&self) -> [usize; 2] {
        [self.rows.len(), self.row.and_then(|row| self.rows.get(row)).map_or(0, Vec::len)]
    }

    /// The wheel over a list scrolls it, a row a notch.
    pub fn scroll_at(&mut self, cursor: [f32; 2], rows: i32) -> bool {
        let Some(local) = self.frame.local(cursor, SIZE[1]) else { return false };
        for (list, area) in [GROUPS, MEMBERS].into_iter().enumerate() {
            if contains([area[0], area[1]], [area[2], area[3]], local) {
                let last = self.lengths()[list].saturating_sub(Self::shown(area));
                self.first[list] = (self.first[list] as i32 + rows).clamp(0, last as i32) as usize;
            }
        }
        true
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let local = self.frame.local(cursor, SIZE[1])?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(Hit::Title(hit));
        }
        if self.frame.minimized {
            return Some(Hit::Window);
        }
        let on = |area: [f32; 4]| contains([area[0], area[1]], [area[2], area[3]], local);
        let row_of = |list: usize, area: [f32; 4]| {
            let row = self.first[list] + ((local[1] - area[1]) / ROW) as usize;
            (on([area[0], area[1], area[2] - BAR, area[3]]) && row < self.lengths()[list]).then_some(row)
        };
        // The open type box's lines lie over what's under them.
        let lines = [FILTER[0], FILTER[1] + FILTER[3], FILTER[2], ROW * FILTERS.len() as f32];
        Some(if self.filter_open && on(lines) {
            Hit::FilterLine(((local[1] - lines[1]) / ROW) as usize)
        } else if on(FILTER) {
            Hit::Filter
        } else if on(FIELD) {
            Hit::Field
        } else if on(SEARCH) {
            Hit::Search
        } else if let Some(row) = row_of(0, GROUPS) {
            Hit::Group(row)
        } else if let Some(row) = row_of(1, MEMBERS) {
            Hit::Member(row)
        } else if on(SERVICE) {
            Hit::Service
        } else if on(FIND) {
            Hit::Find
        } else {
            Hit::Window
        })
    }

    /// A click: everything but the title's buttons, the search and Find, which are the caller's (they need the tables
    /// and the game). Whether the pick changed.
    pub fn click(&mut self, hit: Hit) -> bool {
        self.typing = hit == Hit::Field;
        let open = std::mem::take(&mut self.filter_open);
        match hit {
            Hit::Filter => self.filter_open = !open,
            Hit::FilterLine(line) if line < FILTERS.len() => {
                self.filter = line;
                self.regroup();
                return true;
            }
            Hit::Group(row) => {
                (self.row, self.member, self.first[1]) = (Some(row), 0, 0);
                return true;
            }
            Hit::Member(member) => {
                self.member = member;
                return true;
            }
            Hit::Service => self.service = !self.service,
            _ => {}
        }
        false
    }

    /// The controls and the rows on screen, by name (for the test driver): a row comes with its words and the place it
    /// stands for (a left row: its first).
    pub fn button_boxes(&self, tables: &NaviTables, messages: &[String]) -> Vec<Button<'_>> {
        if !self.frame.open || self.frame.minimized {
            return Vec::new();
        }
        let area = |[x, y, w, h]: [f32; 4]| {
            let [x, y] = self.frame.at([x, y]);
            [x, y, w, h]
        };
        let plain = |name: &str, at: [f32; 4]| (name.to_owned(), area(at), None, None);
        let mut boxes = vec![
            plain("navi_filter", FILTER),
            plain("navi_field", FIELD),
            plain("navi_search", SEARCH),
            plain("navi_service", SERVICE),
            plain("navi_find", FIND),
        ];
        if self.filter_open {
            boxes.extend(
                (0..FILTERS.len()).map(|line| plain(&format!("navi_filter_{line}"), [FILTER[0], FILTER[1] + FILTER[3] + line as f32 * ROW, FILTER[2], ROW])),
            );
        }
        let rows = |list: usize, at: [f32; 4]| {
            (self.first[list]..self.lengths()[list].min(self.first[list] + Self::shown(at)))
                .map(move |row| (row, [at[0], at[1] + (row - self.first[list]) as f32 * ROW, at[2] - BAR, ROW]))
        };
        boxes.extend(rows(0, GROUPS).map(|(row, at)| {
            (format!("navi_group_{row}"), area(at), Some(navigation::group_label(&self.found, &self.rows[row])), Some(&self.found[self.rows[row][0]]))
        }));
        if let Some(picked) = self.row.and_then(|row| self.rows.get(row)) {
            boxes.extend(rows(1, MEMBERS).map(|(row, at)| {
                (
                    format!("navi_member_{row}"),
                    area(at),
                    Some(navigation::member_label(tables, messages, &self.found, picked, row)),
                    Some(&self.found[picked[row]]),
                )
            }));
        }
        boxes
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &'a self,
        layer: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        font: &Font,
        tables: &NaviTables,
        messages: &[String],
        pointer: [f32; 2],
        caret: bool,
    ) {
        if !self.frame.open {
            return;
        }
        let at = |local: [f32; 2]| self.frame.at(local);
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        self.frame.draw_title(layer, chrome, messages.get(TITLE).map_or("Navigation", String::as_str), pointer);
        if self.frame.minimized {
            return;
        }
        let over = |area: [f32; 4]| self.frame.local(pointer, SIZE[1]).is_some_and(|local| contains([area[0], area[1]], [area[2], area[3]], local));
        let fill = |layer: &mut Layer<'a>, texture: &'a Texture, [x, y, w, h]: [f32; 4]| layer.sprites.push((texture, at([x, y]), [w, h]));
        // The body, its border, the line under the search row, and the lines between the lists and the picked place.
        fill(layer, &chrome.white, [0.0, TITLE_HEIGHT, SIZE[0], SIZE[1] - TITLE_HEIGHT]);
        for x in [0.0, SIZE[0] - 1.0] {
            fill(layer, &chrome.border, [x, TITLE_HEIGHT, 1.0, SIZE[1] - TITLE_HEIGHT]);
        }
        fill(layer, &chrome.border, [0.0, SIZE[1] - 1.0, SIZE[0], 1.0]);
        for line in [[0.0, 43.0, SIZE[0], 1.0], [138.0, 44.0, 1.0, 270.0], [139.0, 242.0, 132.0, 1.0]] {
            fill(layer, &assets.grey, line);
        }
        layer.sprites.push((&assets.bottom, at(BOTTOM), assets.bottom.size));
        // The type box with its arrow, the search box with what's typed, the Search button.
        layer.sprites.push((&assets.filter[0], at([FILTER[0], FILTER[1]]), assets.filter[0].size));
        layer.sprites.push((&assets.filter[1], at([FILTER[0] + FILTER[2] - assets.filter[1].size[0], FILTER[1]]), assets.filter[1].size));
        layer.texts.push((message(FILTERS[self.filter]).to_owned(), at([FILTER[0] + 4.0, FILTER[1] + 3.0]), INK));
        layer.sprites.push((&assets.field, at([FIELD[0], FIELD[1]]), assets.field.size));
        let typed = if self.typing && caret { format!("{}_", self.text) } else { self.text.clone() };
        layer.texts.push((crate::window::fit(font, &typed, FIELD[2] - 6.0), at([FIELD[0] + 3.0, FIELD[1] + 3.0]), INK));
        let search = &assets.search[usize::from(over(SEARCH))];
        layer.sprites.push((search, at([SEARCH[0], SEARCH[1]]), search.size));
        layer.texts.push((rustnarok_client::messages::fill(message(FOUND), &[&self.rows.len()]), at(FOUND_AT), INK));
        // The lists: a row's icon and words, the picked one on its bar; a bar at the right when there's more.
        let lengths = self.lengths();
        let list = |layer: &mut Layer<'a>, which: usize, area: [f32; 4], picked: Option<usize>, row: &dyn Fn(usize) -> (Option<u8>, String)| {
            let shown = Self::shown(area);
            for index in self.first[which]..lengths[which].min(self.first[which] + shown) {
                let top = area[1] + (index - self.first[which]) as f32 * ROW;
                if picked == Some(index) {
                    fill(layer, &assets.picked, [area[0], top, area[2] - BAR, ROW]);
                }
                let (kind, words) = row(index);
                let left = match kind.and_then(|kind| assets.icons.get(usize::from(kind).wrapping_sub(1))) {
                    Some(icon) => {
                        layer.sprites.push((icon, at([area[0] + 2.0, top + 2.0]), icon.size));
                        14.0
                    }
                    None => 3.0,
                };
                layer.texts.push((crate::window::fit(font, &words, area[2] - BAR - left - 2.0), at([area[0] + left, top + 2.0]), INK));
            }
            // ponytail: a plain track and thumb; the game's bar has pictures of its own.
            if lengths[which] > shown {
                let (track, thumb) = (area[3], (area[3] * shown as f32 / lengths[which] as f32).max(8.0));
                let top = area[1] + (track - thumb) * self.first[which] as f32 / (lengths[which] - shown) as f32;
                fill(layer, &assets.grey, [area[0] + area[2] - BAR, area[1], BAR, track]);
                fill(layer, &chrome.border, [area[0] + area[2] - BAR + 1.0, top, BAR - 2.0, thumb]);
            }
        };
        list(layer, 0, GROUPS, self.row, &|row| (Some(self.found[self.rows[row][0]].kind), navigation::group_label(&self.found, &self.rows[row])));
        if let Some(picked) = self.row.and_then(|row| self.rows.get(row)) {
            list(layer, 1, MEMBERS, Some(self.member), &|member| (None, navigation::member_label(tables, messages, &self.found, picked, member)));
        }
        // The picked place: its name over its map's picture.
        if let Some(place) = self.picked() {
            fill(layer, &assets.black, PICTURE);
            if let Some((_, Some(picture))) = &self.picture {
                layer.sprites.push((picture, at([PICTURE[0], PICTURE[1]]), [PICTURE[2], PICTURE[3]]));
            }
            layer.texts.push((crate::window::fit(font, &place.name, SIZE[0] - NAME_AT[0] - 4.0), at(NAME_AT), PICKED));
        }
        // The service box, and Find.
        let check = &assets.check[usize::from(self.service)];
        layer.sprites.push((check, at([SERVICE[0], SERVICE[1] + 1.0]), check.size));
        layer.texts.push(("service".to_owned(), at([SERVICE[0] + check.size[0] + 3.0, SERVICE[1]]), INK));
        let find = &assets.find[usize::from(over(FIND))];
        layer.sprites.push((find, at([FIND[0], FIND[1]]), find.size));
        // The open type box's lines, over the rest.
        if self.filter_open {
            let top = FILTER[1] + FILTER[3];
            fill(layer, &chrome.border, [FILTER[0], top, FILTER[2], ROW * FILTERS.len() as f32 + 1.0]);
            fill(layer, &chrome.white, [FILTER[0] + 1.0, top, FILTER[2] - 2.0, ROW * FILTERS.len() as f32]);
            for (line, id) in FILTERS.into_iter().enumerate() {
                let line_top = top + line as f32 * ROW;
                if over([FILTER[0], line_top, FILTER[2], ROW]) {
                    fill(layer, &assets.picked, [FILTER[0] + 1.0, line_top, FILTER[2] - 2.0, ROW]);
                }
                layer.texts.push((message(id).to_owned(), at([FILTER[0] + 4.0, line_top + 2.0]), INK));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustnarok_client::lua::{NaviMonster, NaviNpc};

    fn tables() -> NaviTables {
        NaviTables {
            maps: [("prt_fild08", "Prontera Field"), ("prontera", "Prontera")].map(|(map, shown)| (map.to_owned(), shown.to_owned())).into_iter().collect(),
            npcs: vec![NaviNpc { map: "prontera".into(), name: "Tool Dealer".into(), cell: [30, 40] }],
            monsters: vec![NaviMonster { map: "prt_fild08".into(), id: 1002, name: "Poring".into(), spawns: 70 }],
            ..NaviTables::default()
        }
    }

    fn window(text: &str) -> Window {
        let mut window = Window::new(false);
        window.type_text(text);
        window.search(&tables());
        window
    }

    #[test]
    fn types_thirty_characters_at_most() {
        let mut window = window("");
        assert_eq!((window.typing, window.rows.len()), (false, 0));
        window.type_text(&"a".repeat(40));
        assert_eq!(window.text.len(), 30);
    }

    #[test]
    fn picks_a_row_then_a_place_and_starts_nothing_by_itself() {
        let mut window = window("pront");
        // Two maps: the field with its Poring, and the town.
        assert_eq!(window.rows, [vec![0, 2], vec![1]]);
        assert_eq!(window.target(&tables()), Err(Some(NOTHING_PICKED)));
        let corner = window.frame.position;
        let center = |area: [f32; 4]| [corner[0] + area[0] + 5.0, corner[1] + area[1] + 5.0];
        let (first_row, second_place) = (center(GROUPS), center([MEMBERS[0], MEMBERS[1] + ROW, 0.0, 0.0]));
        assert_eq!(window.hit(first_row), Some(Hit::Group(0)));
        // A row picks its first place; no third row to press.
        assert!(window.click(Hit::Group(0)));
        assert_eq!(window.target(&tables()).map(|target| target.kind()), Ok("map"));
        assert_eq!((window.hit(second_place), window.hit(center([GROUPS[0], GROUPS[1] + 2.0 * ROW, 0.0, 0.0]))), (Some(Hit::Member(1)), Some(Hit::Window)));
        assert!(window.click(Hit::Member(1)));
        assert_eq!(window.target(&tables()).map(|target| (target.kind(), target.monster)), Ok(("monster", Some(1002))));
        // The service box: none of the paid ways, or the Kafra and the airship.
        assert_eq!(window.services(), 0);
        assert!(!window.click(Hit::Service));
        assert_eq!(window.services(), 101);
    }

    #[test]
    fn the_type_box_shows_the_rows_of_one_type() {
        let mut window = window("o");
        assert_eq!(window.rows.len(), 4);
        window.click(Hit::Group(0));
        // It opens on a press and closes on the next, whatever that's on; a line of it sets the type and unpicks.
        window.click(Hit::Filter);
        assert!(window.filter_open);
        let line = [window.frame.position[0] + FILTER[0] + 5.0, window.frame.position[1] + FILTER[1] + FILTER[3] + 2.0 * ROW + 5.0];
        assert_eq!(window.hit(line), Some(Hit::FilterLine(2)));
        assert!(window.click(Hit::FilterLine(2)));
        assert_eq!((window.filter, window.filter_open, window.rows.len(), window.row), (2, false, 1, None));
        assert_eq!(navigation::group_label(&window.found, &window.rows[0]), "[1]Tool Dealer");
        // Closed, the same spot is the left list's.
        assert_ne!(window.hit(line), Some(Hit::FilterLine(2)));
    }

    #[test]
    fn the_wheel_scrolls_a_list_that_has_more() {
        let mut tables = tables();
        tables.npcs.extend((0..20).map(|index| NaviNpc { map: "prontera".into(), name: format!("Guard {index:02}"), cell: [index, 1] }));
        let mut window = Window::new(false);
        window.type_text("guard");
        window.search(&tables);
        let over = [window.frame.position[0] + GROUPS[0] + 5.0, window.frame.position[1] + GROUPS[1] + 5.0];
        // Twenty rows, fifteen shown: five to scroll, no further.
        assert!(window.scroll_at(over, 3));
        assert_eq!(window.first[0], 3);
        window.scroll_at(over, 9);
        assert_eq!((window.first[0], window.hit(over)), (5, Some(Hit::Group(5))));
        window.scroll_at(over, -9);
        assert_eq!(window.first[0], 0);
        assert!(!window.scroll_at([0.0, 0.0], 1));
    }
}
