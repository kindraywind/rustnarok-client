//! The dress room (/dressroom, ZC_DRESSROOM_OPEN): the player's figure on a stage, trying other hairstyles and colors.
//! A preview only: nothing goes to the server and the player's real look never changes. The window: four tabs over a
//! list of the chosen part's choices on the left, the stage with the figure and two turn arrows on the right.

use rustnarok_client::grf::Grf;
use rustnarok_client::text::Font;

use crate::actor::Look;
use crate::render::{Gpu, Texture};
use crate::window::{self, Chrome, Frame, TitleHit};
use crate::{Layer, UI, contains, offset, screen};

/// The data's lists (data\luafiles514\lua files\dressroom\): each row a label and the value it puts on.
#[derive(Debug, Clone, PartialEq)]
pub struct Lists {
    pub hair: Vec<(String, u16)>,
    pub hair_color: Vec<(String, u16)>,
    pub clothes_color: Vec<(String, u16)>,
}

impl Default for Lists {
    /// What the data's lists hold, for when they can't be read: the values as their labels.
    fn default() -> Self {
        let rows = |values: std::ops::RangeInclusive<u16>| values.map(|value| (value.to_string(), value)).collect();
        Self { hair: rows(1..=29), hair_color: rows(0..=8), clothes_color: rows(0..=3) }
    }
}

impl Lists {
    /// Tab `tab`'s list: Hair Color, Hair Style, Cloth Color; Cloth Pattern has none.
    fn of(&self, tab: usize) -> &[(String, u16)] {
        match tab {
            0 => &self.hair_color,
            1 => &self.hair,
            2 => &self.clothes_color,
            _ => &[],
        }
    }
}

const TITLE: &str = "Dress Room";
/// The tabs' labels: "Hair Color", "Hair Style", "Cloth Color", "Cloth Pattern".
const TABS: [usize; 4] = [2763, 2764, 2765, 2766];
const TURN_LEFT: usize = 2174;
const TURN_RIGHT: usize = 2175;

pub const SIZE: [f32; 2] = [395.0, 205.0];
const BASE_BUTTON: [f32; 2] = [3.0, 3.0];
const TAB_STRIP: [f32; 2] = [4.0, 20.0];
const TAB_SIZE: [f32; 2] = [183.0 / 4.0, 18.0];
// ponytail: how the four tabs share the strip, and which of the two colours is the fill, aren't settled: evenly, the
// chosen one lighter.
const TAB_LIGHT: [u8; 3] = [232, 232, 232];
const TAB_DARK: [u8; 3] = [100, 100, 100];
const LIST: [f32; 2] = [4.0, 40.0];
const LIST_SIZE: [f32; 2] = [183.0, 160.0];
const ROW: f32 = 16.0;
const BAND: f32 = 15.0;
const VISIBLE: usize = 10;
const STAGE_AT: [f32; 2] = [191.0, 20.0];
const STAGE: [f32; 2] = [200.0, 180.0];
/// Where the figure's feet are.
const FEET: [f32; 2] = [291.0, 170.0];
const TURN: [f32; 2] = [13.0, 13.0];
const TURN_LEFT_AT: [f32; 2] = [193.0, 110.0];
const TURN_RIGHT_AT: [f32; 2] = [376.0, 110.0];

/// One step through `count` choices from `index`, wrapping around (the stylist's arrows).
pub fn step(index: usize, count: usize, forward: bool) -> usize {
    if count == 0 {
        0
    } else if forward {
        (index + 1) % count
    } else {
        (index + count - 1) % count
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(TitleHit),
    Tab(usize),
    /// A row of the list, counted from its top whether or not it is scrolled away.
    Row(usize),
    /// A turn arrow: true left.
    Turn(bool),
    Window,
}

pub struct Assets {
    stage: Option<Texture>,
    /// Left, then right.
    turn: [Texture; 2],
    list: Texture,
    /// The chosen row's band, as the NPC menu's.
    band: Texture,
    tabs: [Texture; 2],
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        Self {
            stage: gpu.try_load_texture(grf, &format!("{UI}basic_interface\\dressroomback_0.bmp"), false).ok(),
            turn: ["scroll1left", "scroll1right"].map(|name| gpu.load_texture(grf, &format!("{UI}{name}.bmp"), false)),
            list: gpu.solid([210, 210, 210, 255]),
            band: crate::npc_menu::Assets::load(gpu, grf).band,
            tabs: [TAB_LIGHT, TAB_DARK].map(|[r, g, b]| gpu.solid([r, g, b, 255])),
        }
    }
}

/// The dress room open: the tab, the picks (indexes into the lists) and which way the figure faces.
pub struct Room {
    pub frame: Frame,
    pub tab: usize,
    pub hair: usize,
    pub hair_color: usize,
    pub clothes: usize,
    scroll: usize,
    /// RO's direction, 0 facing south.
    pub direction: u8,
    /// The look changed since the figure was built.
    pub changed: bool,
}

impl Room {
    /// Opens on the first tab and the player's own hair and colors (the first choice for a value the lists don't
    /// have).
    pub fn new(look: &Look, lists: &Lists) -> Self {
        let at = |list: &[(String, u16)], value: u16| list.iter().position(|(_, choice)| *choice == value).unwrap_or(0);
        let [width, height] = screen();
        let mut frame = Frame::new([((width - 640.0) / 2.0 + 200.0).floor(), (200.0 * height / 480.0).floor()], SIZE[0]);
        frame.open = true;
        let mut room = Self {
            frame,
            tab: 0,
            hair: at(&lists.hair, look.hair),
            hair_color: at(&lists.hair_color, look.hair_color),
            clothes: at(&lists.clothes_color, look.clothes_color),
            scroll: 0,
            direction: 0,
            changed: true,
        };
        room.show_pick();
        room
    }

    /// The figure's look: the player's, with the picks put on.
    pub fn look(&self, base: &Look, lists: &Lists) -> Look {
        let pick = |list: &[(String, u16)], index: usize, own: u16| list.get(index).map_or(own, |(_, value)| *value);
        Look {
            hair: pick(&lists.hair, self.hair, base.hair),
            hair_color: pick(&lists.hair_color, self.hair_color, base.hair_color),
            clothes_color: pick(&lists.clothes_color, self.clothes, base.clothes_color),
            ..base.clone()
        }
    }

    /// The chosen tab's picked row (none on Cloth Pattern).
    fn picked(&self) -> Option<usize> {
        [Some(self.hair_color), Some(self.hair), Some(self.clothes), None][self.tab]
    }

    /// Scrolls the list so the picked row shows.
    fn show_pick(&mut self) {
        let row = self.picked().unwrap_or(0);
        self.scroll = self.scroll.min(row).max((row + 1).saturating_sub(VISIBLE));
    }

    pub fn scroll_by(&mut self, rows: i32, lists: &Lists) {
        let most = lists.of(self.tab).len().saturating_sub(VISIBLE);
        self.scroll = (self.scroll as i32 + rows).clamp(0, most as i32) as usize;
    }

    fn tab_at(tab: usize) -> [f32; 2] {
        [TAB_STRIP[0] + tab as f32 * TAB_SIZE[0], TAB_STRIP[1]]
    }

    fn row_at(&self, row: usize) -> [f32; 2] {
        [LIST[0], LIST[1] + (row - self.scroll) as f32 * ROW]
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let local = self.frame.local(cursor, SIZE[1])?;
        // Base and close buttons only: where others have minimize, it is plain title bar.
        match self.frame.title_hit(local) {
            Some(TitleHit::Minimize) => return Some(Hit::Title(TitleHit::Title)),
            Some(hit) => return Some(Hit::Title(hit)),
            None => {}
        }
        if let Some(tab) = (0..TABS.len()).find(|&tab| contains(Self::tab_at(tab), TAB_SIZE, local)) {
            return Some(Hit::Tab(tab));
        }
        if contains(LIST, LIST_SIZE, local) {
            return Some(Hit::Row(self.scroll + ((local[1] - LIST[1]) / ROW) as usize));
        }
        for (left, at) in [(true, TURN_LEFT_AT), (false, TURN_RIGHT_AT)] {
            if contains(at, TURN, local) {
                return Some(Hit::Turn(left));
            }
        }
        Some(Hit::Window)
    }

    /// A click: a tab shows its list, a row puts its value on the figure, a turn arrow turns it; true when the window
    /// closes.
    pub fn click(&mut self, hit: Hit, lists: &Lists) -> bool {
        match hit {
            Hit::Title(TitleHit::Close) => return true,
            Hit::Title(title) => self.frame.click_title(title),
            Hit::Tab(tab) => {
                self.tab = tab;
                self.scroll = 0;
                self.show_pick();
            }
            Hit::Row(row) if row < lists.of(self.tab).len() => {
                match self.tab {
                    0 => self.hair_color = row,
                    1 => self.hair = row,
                    _ => self.clothes = row,
                }
                self.changed = true;
            }
            // RO's directions go counterclockwise from south: turning left is one more.
            Hit::Turn(left) => self.direction = if left { (self.direction + 1) % 8 } else { (self.direction + 7) % 8 },
            Hit::Row(_) | Hit::Window => {}
        }
        false
    }

    /// Where the figure's feet are on screen.
    pub fn feet(&self) -> [f32; 2] {
        self.frame.at(FEET)
    }

    /// The tabs, the rows shown and the turn arrows on screen (for the test driver): `dress_tab_<n>`,
    /// `dress_row_<n>` (counted from the list's top), `dress_turn_left` / `_right`.
    pub fn button_boxes(&self, lists: &Lists) -> Vec<(String, [f32; 4])> {
        let place = |at: [f32; 2], size: [f32; 2]| {
            let [x, y] = self.frame.at(at);
            [x, y, size[0], size[1]]
        };
        let rows = lists.of(self.tab).len();
        let mut boxes: Vec<_> = (0..TABS.len()).map(|tab| (format!("dress_tab_{tab}"), place(Self::tab_at(tab), TAB_SIZE))).collect();
        boxes.extend((self.scroll..rows.min(self.scroll + VISIBLE)).map(|row| (format!("dress_row_{row}"), place(self.row_at(row), [LIST_SIZE[0], ROW]))));
        boxes.push(("dress_turn_left".into(), place(TURN_LEFT_AT, TURN)));
        boxes.push(("dress_turn_right".into(), place(TURN_RIGHT_AT, TURN)));
        boxes
    }

    /// The window; the figure's sprites (posed by the caller at `feet`) go on the stage, under the turn arrows.
    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &self,
        layer: &mut Layer<'a>,
        tooltip: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        font: &Font,
        messages: &[String],
        lists: &Lists,
        figure: Vec<crate::render::Sprite<'a>>,
        pointer: ([f32; 2], bool),
    ) {
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        let at = |local: [f32; 2]| self.frame.at(local);
        chrome.draw_body(layer, at([0.0, 0.0]), SIZE);
        // The title bar with the base and close buttons.
        self.frame.draw_title_only(layer, chrome, TITLE);
        for (button, place) in [(0, BASE_BUTTON), (2, self.frame.close_button())] {
            let image = chrome.button(button, contains(at(place), window::BUTTON_SIZE, pointer.0));
            layer.sprites.push((image, at(place), image.size));
        }
        // The tabs, the chosen one light.
        for (tab, &label) in TABS.iter().enumerate() {
            let chosen = tab == self.tab;
            let place = Self::tab_at(tab);
            layer.sprites.push((&assets.tabs[usize::from(!chosen)], at(place), TAB_SIZE));
            let text = window::fit(font, message(label), TAB_SIZE[0] - 4.0);
            let x = place[0] + ((TAB_SIZE[0] - font.width(&text)) / 2.0).floor();
            layer.texts.push((text, at([x, place[1] + ((TAB_SIZE[1] - font.line_height()) / 2.0).floor()]), if chosen { TAB_DARK } else { TAB_LIGHT }));
        }
        // The list: the chosen tab's labels, the picked row on its band.
        layer.sprites.push((&assets.list, at(LIST), LIST_SIZE));
        let rows = lists.of(self.tab);
        for (row, (label, _)) in rows.iter().enumerate().skip(self.scroll).take(VISIBLE) {
            let place = self.row_at(row);
            if self.picked() == Some(row) {
                layer.sprites.push((&assets.band, at(place), [LIST_SIZE[0], BAND]));
            }
            layer.texts.push((window::fit(font, label, LIST_SIZE[0] - 6.0), at(offset(place, [3.0, 1.0])), [0; 3]));
        }
        // The stage, the figure on it, the turn arrows.
        if let Some(stage) = &assets.stage {
            layer.sprites.push((stage, at(STAGE_AT), STAGE));
        }
        layer.sprites.extend(figure);
        for (index, place) in [TURN_LEFT_AT, TURN_RIGHT_AT].into_iter().enumerate() {
            layer.sprites.push((&assets.turn[index], at(place), TURN));
        }
        if let Some(Hit::Turn(left)) = self.hit(pointer.0) {
            let words = message(if left { TURN_LEFT } else { TURN_RIGHT });
            window::tooltip(tooltip, chrome, font, words, at(offset(if left { TURN_LEFT_AT } else { TURN_RIGHT_AT }, [0.0, -20.0])), [255; 3]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn on(room: &Room, local: [f32; 2]) -> [f32; 2] {
        room.frame.at(local)
    }

    #[test]
    fn the_pieces_sit_where_the_game_puts_them() {
        let lists = Lists::default();
        let room = Room::new(&Look { hair: 5, ..Look::default() }, &lists);
        assert_eq!(SIZE, [395.0, 205.0]);
        let [width, height] = screen();
        assert_eq!(room.frame.position, [((width - 640.0) / 2.0 + 200.0).floor(), (200.0 * height / 480.0).floor()]);
        let hit = |local: [f32; 2]| room.hit(on(&room, local));
        assert_eq!(hit([4.0, 20.0]), Some(Hit::Tab(0)));
        assert_eq!(hit([50.0, 37.0]), Some(Hit::Tab(1)));
        assert_eq!(hit([186.0, 20.0]), Some(Hit::Tab(3)));
        assert_eq!(hit([4.0, 40.0]), Some(Hit::Row(0)));
        assert_eq!(hit([186.0, 55.9]), Some(Hit::Row(0)));
        assert_eq!(hit([100.0, 56.0]), Some(Hit::Row(1)));
        assert_eq!(hit([100.0, 199.0]), Some(Hit::Row(9)));
        assert_eq!(hit([193.0, 110.0]), Some(Hit::Turn(true)));
        assert_eq!(hit([388.0, 122.0]), Some(Hit::Turn(false)));
        assert_eq!(hit([382.0, 5.0]), Some(Hit::Title(TitleHit::Close)));
        assert_eq!(hit([368.0, 5.0]), Some(Hit::Title(TitleHit::Title)));
        assert_eq!(hit([250.0, 100.0]), Some(Hit::Window));
        assert_eq!(hit([250.0, 205.0]), None);
        assert_eq!(room.feet(), on(&room, [291.0, 170.0]));
        let names: Vec<_> = room.button_boxes(&lists).into_iter().map(|(name, _)| name).collect();
        assert_eq!(names.len(), 4 + 9 + 2, "Hair Color lists 9");
        assert!(names.contains(&"dress_tab_1".to_owned()) && names.contains(&"dress_row_8".to_owned()) && names.contains(&"dress_turn_right".to_owned()));
    }

    #[test]
    fn tabs_list_their_labels_and_mark_what_is_worn() {
        let lists = Lists {
            hair: (1..=29).map(|value| (format!("({value}) Hair"), value)).collect(),
            hair_color: vec![("(0) Basic".into(), 0), ("(1) Red".into(), 1)],
            clothes_color: vec![("(0) Basic".into(), 0)],
        };
        let base = Look { hair: 15, hair_color: 1, clothes_color: 0, job: 4001, ..Look::default() };
        let mut room = Room::new(&base, &lists);
        assert_eq!(room.look(&base, &lists), base, "it opens on the player's own");
        assert_eq!((room.tab, room.picked()), (0, Some(1)));
        assert_eq!(lists.of(room.tab)[1].0, "(1) Red");
        room.click(Hit::Tab(1), &lists);
        assert_eq!(room.picked(), Some(14));
        assert_eq!(room.scroll, 5, "the worn row shows");
        assert_eq!(room.hit(on(&room, [100.0, 40.0])), Some(Hit::Row(5)));
        room.click(Hit::Tab(3), &lists);
        assert_eq!((room.picked(), lists.of(3).len()), (None, 0));
        // A row changes that part only; a row past the list's end does nothing.
        room.changed = false;
        room.click(Hit::Tab(1), &lists);
        room.click(Hit::Row(20), &lists);
        let look = room.look(&base, &lists);
        assert_eq!((look.hair, look.hair_color, look.clothes_color, look.job), (21, 1, 0, 4001));
        assert!(room.changed);
        room.click(Hit::Tab(0), &lists);
        room.click(Hit::Row(5), &lists);
        assert_eq!(room.look(&base, &lists).hair_color, 1);
        room.click(Hit::Row(0), &lists);
        assert_eq!(room.look(&base, &lists).hair_color, 0);
        room.scroll_by(5, &lists);
        assert_eq!(room.scroll, 0);
    }

    #[test]
    fn steps_and_wraps() {
        assert_eq!(step(28, 29, true), 0);
        assert_eq!(step(0, 9, false), 8);
        assert_eq!(step(2, 4, true), 3);
        assert_eq!(step(0, 0, true), 0);
    }

    #[test]
    fn turns_and_wraps() {
        let lists = Lists::default();
        let mut room = Room::new(&Look::default(), &lists);
        room.click(Hit::Turn(true), &lists);
        assert_eq!(room.direction, 1);
        room.click(Hit::Turn(false), &lists);
        room.click(Hit::Turn(false), &lists);
        assert_eq!(room.direction, 7);
        for _ in 0..2 {
            room.click(Hit::Turn(true), &lists);
        }
        assert_eq!(room.direction, 1);
        assert!(room.click(Hit::Title(TitleHit::Close), &lists));
    }
}
