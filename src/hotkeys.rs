//! The hotkey bar (F1-F9, F12 shows or hides it): items and skills kept at hand, rows of nine, and a row of two quick
//! item slots under them that a button opens. Each of the two bars keeps its own row rotation (Ctrl+F12) on the server.

use std::collections::HashMap;

use rustnarok_client::grf::Grf;
use rustnarok_client::lua::{ItemInfo, SkillInfo};
use rustnarok_client::net::{Hotkey, InventoryItem};
use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};
use crate::window::{self, Chrome, Frame};
use crate::{Layer, UI, contains, inventory};

// The game's bar: 290 wide, a 34-high shortcut frame a row (33 apart, overlapping by one), slots 29 apart.
const WIDTH: f32 = 290.0;
const ROW_HEIGHT: f32 = 33.0;
pub const PER_ROW: usize = 9;
const ROWS: usize = 4;
/// The server keeps 38 slots a bar: four rows of nine, then the two quick item slots.
const SLOTS: usize = ROWS * PER_ROW + 2;
const QUICK: [usize; 2] = [ROWS * PER_ROW, ROWS * PER_ROW + 1];
/// The quick row's open and close button at (247, h − 10).
// ponytail: the button's picture isn't known; the plus and minus buttons stand in, 11 square.
const QUICK_BUTTON: [f32; 2] = [247.0, -10.0];
const SLOT: [f32; 2] = [24.0, 24.0];
const SLOT_STEP: f32 = 29.0;
const FIRST_SLOT: [f32; 2] = [16.0, 4.0];
/// The hover box sits a pixel lower than the icon.
const HOVER_DROP: f32 = 1.0;
/// The box filled under the slot pointed at.
// ponytail: its colour (the skin's) is ours.
const HOVER_COLOR: [u8; 4] = [0x82, 0xAA, 0xE6, 0xA0];
const ROW_NUMBER: [f32; 2] = [6.0, 13.0];
const ROW_NUMBER_COLOR: [u8; 3] = [74, 74, 47];
/// The bar switch (hotbar 1 or 2), and the minus and plus buttons by the last row.
const SWITCH: [f32; 2] = [3.0, 4.0];
const SWITCH_SIZE: [f32; 2] = [10.0, 26.0];
const STEP_BUTTON: [f32; 2] = [11.0, 11.0];
const DEFAULT_POSITION: [f32; 2] = [230.0, 0.0];
const DEFAULT_ROWS: usize = 2;

pub struct Assets {
    frame: Texture,
    /// The quick item row's frame.
    // ponytail: this data has no shortcut_frame_item.bmp; the row's frame stands in.
    quick_frame: Texture,
    /// The bar switch for hotbar 1 and 2.
    switch: [Texture; 2],
    minus: Texture,
    plus: Texture,
    /// Refine grade marks, D to A.
    grades: [Texture; 4],
    /// The box under the slot the pointer is on.
    hover: Texture,
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |path: String| gpu.try_load_texture(grf, &path, false).unwrap_or_else(|_| gpu.solid([0, 0, 0, 0]));
        Self {
            frame: image(format!("{UI}shortcut\\shortcut_frame.bmp")),
            quick_frame: gpu
                .try_load_texture(grf, &format!("{UI}shortcut\\shortcut_frame_item.bmp"), false)
                .unwrap_or_else(|_| image(format!("{UI}shortcut\\shortcut_frame.bmp"))),
            hover: gpu.solid(HOVER_COLOR),
            switch: ["shortcut_1_bts", "shortcut_2_bts"].map(|name| image(format!("{UI}{name}.bmp"))),
            minus: image(format!("{UI}shortcut\\btn_minus_normal.bmp")),
            plus: image(format!("{UI}shortcut\\btn_plus_normal.bmp")),
            grades: [1, 2, 3, 4].map(|grade| image(format!("{UI}grade_enchant\\grade_icon{grade}.bmp"))),
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    /// One row fewer, one more.
    Minus,
    Plus,
    /// The bar switch: hotbar 1 or 2.
    Switch,
    /// Opens or closes the quick item row.
    Quick,
    Slot(usize),
    Window,
}

pub struct Hotkeys {
    pub frame: Frame,
    /// Every slot of the bar showing (four rows of nine and the two quick slots): the server's tab `tab`.
    pub keys: Vec<Option<Hotkey>>,
    /// The other bar's slots.
    other: Vec<Option<Hotkey>>,
    /// The bar showing: the server's tab 0 (hotbar 1) or 1 (hotbar 2).
    tab: u16,
    /// How many rows show.
    rows: usize,
    /// Which row each bar (the server's tab) draws first. Ctrl+F12 walks the showing bar's.
    first: [usize; 2],
    /// The quick item row shows under the rows.
    // ponytail: closed until opened, each session.
    pub quick: bool,
    /// A slot is used by a double click; a single press only starts a drag.
    pub clicks: window::Clicks,
}

impl Default for Hotkeys {
    fn default() -> Self {
        let mut frame = Frame::new(DEFAULT_POSITION, WIDTH);
        frame.open = true;
        Self {
            frame,
            keys: vec![None; SLOTS],
            other: vec![None; SLOTS],
            tab: 0,
            rows: DEFAULT_ROWS,
            first: [0; 2],
            quick: false,
            clicks: window::Clicks::default(),
        }
    }
}

impl Hotkeys {
    /// 33 a row plus 10, and 33 more with the quick row.
    pub fn height(&self) -> f32 {
        ROW_HEIGHT * (self.rows + usize::from(self.quick)) as f32 + 10.0
    }

    fn quick_button(&self) -> [f32; 2] {
        [QUICK_BUTTON[0], self.height() + QUICK_BUTTON[1]]
    }

    /// The slots showing: key index and icon corner in the bar, row by row, then the quick row's two.
    // ponytail: the quick slots sit at x 16 and 45 like a row's first two.
    fn shown(&self) -> Vec<(usize, [f32; 2])> {
        let rows = (0..self.rows * PER_ROW).map(|visible| (self.key_index(visible), slot_corner(visible)));
        let quick = QUICK.into_iter().enumerate().filter(|_| self.quick).map(|(column, index)| (index, slot_corner(self.rows * PER_ROW + column)));
        rows.chain(quick).collect()
    }

    /// Where the minus and plus buttons sit: by the last row.
    fn step_buttons(&self) -> [[f32; 2]; 2] {
        let bottom = ROW_HEIGHT * self.rows as f32;
        [[277.0, bottom - 28.0], [277.0, bottom - 13.0]]
    }

    /// The slot (row by row) or button under the pointer.
    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let local = self.frame.local(cursor, self.height())?;
        let [minus, plus] = self.step_buttons();
        if contains(minus, STEP_BUTTON, local) {
            return Some(Hit::Minus);
        }
        if contains(plus, STEP_BUTTON, local) {
            return Some(Hit::Plus);
        }
        if contains(SWITCH, SWITCH_SIZE, local) {
            return Some(Hit::Switch);
        }
        if contains(self.quick_button(), STEP_BUTTON, local) {
            return Some(Hit::Quick);
        }
        Some(self.shown().into_iter().find(|&(_, corner)| contains(hover_corner(corner), SLOT, local)).map_or(Hit::Window, |(index, _)| Hit::Slot(index)))
    }

    /// The server's tab of the bar showing: 0 for hotbar 1, 1 for hotbar 2.
    pub fn tab(&self) -> u16 {
        self.tab
    }

    /// The bar switch: hotbar 1 and 2 trade places.
    pub fn switch(&mut self) {
        std::mem::swap(&mut self.keys, &mut self.other);
        self.tab ^= 1;
    }

    /// The server's list for tab `tab` (0 or 1), the bar showing or the other one, with that bar's rotation.
    pub fn set_tab(&mut self, tab: u16, rotate: u8, keys: Vec<Option<Hotkey>>) {
        let bar = if tab == self.tab { &mut self.keys } else { &mut self.other };
        for (slot, key) in bar.iter_mut().zip(keys) {
            *slot = key;
        }
        if let Some(first) = self.first.get_mut(usize::from(tab)) {
            *first = usize::from(rotate) % ROWS;
        }
    }

    /// Ctrl+F12: the next row of the four is the one the showing bar draws first. The bar's tab and its rotation, to
    /// keep on the server.
    pub fn rotate(&mut self) -> (u16, u8) {
        let first = &mut self.first[usize::from(self.tab)];
        *first = (*first + 1) % ROWS;
        (self.tab, *first as u8)
    }

    /// The row the showing bar draws first.
    fn rotation(&self) -> usize {
        self.first[usize::from(self.tab)]
    }

    /// Each bar's rotation, hotbar 1's first.
    pub fn rotations(&self) -> [usize; 2] {
        self.first
    }

    /// The key index under a visible slot, once the bar's rotation has turned the rows.
    fn key_index(&self, visible: usize) -> usize {
        let row = (self.rotation() + visible / PER_ROW) % ROWS;
        row * PER_ROW + visible % PER_ROW
    }

    pub fn click(&mut self, hit: Hit) {
        match hit {
            Hit::Minus => self.rows = self.rows.saturating_sub(1).max(1),
            Hit::Plus => self.rows = (self.rows + 1).min(ROWS),
            Hit::Switch => self.switch(),
            Hit::Quick => self.quick = !self.quick,
            Hit::Slot(_) | Hit::Window => {}
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &self,
        layer: &mut Layer<'a>,
        tooltip: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        icons: &'a HashMap<String, Option<Texture>>,
        items: &HashMap<u32, ItemInfo>,
        skills: &HashMap<u16, SkillInfo>,
        inventory: &[InventoryItem],
        font: &Font,
        cursor: [f32; 2],
        cooling: &crate::skills::Cooling<'a>,
    ) {
        if !self.frame.open {
            return;
        }
        for row in 0..self.rows {
            let top = row as f32 * ROW_HEIGHT;
            layer.sprites.push((&assets.frame, self.frame.at([0.0, top]), assets.frame.size));
            let number = ((self.rotation() + row) % ROWS + 1).to_string();
            layer.texts.push((number, self.frame.at([ROW_NUMBER[0], top + ROW_NUMBER[1]]), ROW_NUMBER_COLOR));
        }
        if self.quick {
            layer.sprites.push((&assets.quick_frame, self.frame.at([0.0, self.rows as f32 * ROW_HEIGHT]), assets.quick_frame.size));
        }
        layer.sprites.push((&assets.switch[usize::from(self.tab)], self.frame.at(SWITCH), SWITCH_SIZE));
        let [minus, plus] = self.step_buttons();
        layer.sprites.push((&assets.minus, self.frame.at(minus), STEP_BUTTON));
        layer.sprites.push((&assets.plus, self.frame.at(plus), STEP_BUTTON));
        let toggle = if self.quick { &assets.minus } else { &assets.plus };
        layer.sprites.push((toggle, self.frame.at(self.quick_button()), STEP_BUTTON));
        for (index, local) in self.shown() {
            let key = self.keys.get(index).and_then(|key| *key);
            let corner = self.frame.at(local);
            // The slot under the pointer: a box under its icon.
            let hover = self.frame.at(hover_corner(local));
            if contains(hover, SLOT, cursor) {
                layer.sprites.push((&assets.hover, hover, SLOT));
            }
            // A skill: its icon, the name on hover.
            if let Some(Hotkey { skill: true, id, count }) = key.as_ref()
                && let Some(info) = u16::try_from(*id).ok().and_then(|id| skills.get(&id))
            {
                if let Some(Some(icon)) = icons.get(&crate::skills::icon_path(info)) {
                    layer.sprites.push((icon, corner, icon.size));
                }
                // The level it's used at, bottom right, as an item's amount; a clock while it's blocked.
                count_text(layer, font, corner, &count.to_string());
                cooling.draw(layer, *id as u16, corner, [24.0, 24.0]);
                if contains(corner, SLOT, cursor) {
                    window::tooltip(tooltip, chrome, font, &format!("{} Lv {count}", info.name), [corner[0], corner[1] + SLOT[1] + 2.0], [255; 3]);
                }
                continue;
            }
            let Some(Hotkey { skill: false, id, .. }) = key else { continue };
            let Some(info) = items.get(&id) else { continue };
            if let Some(Some(icon)) = icons.get(&inventory::icon_path(info, true)) {
                layer.sprites.push((icon, corner, icon.size));
            }
            // A graded item carries its grade's mark.
            let grade = inventory.iter().find(|item| item.id == id).map_or(0, |item| item.grade);
            if let Some(mark) = usize::from(grade).checked_sub(1).and_then(|index| assets.grades.get(index)) {
                layer.sprites.push((mark, [corner[0], corner[1] + 12.0], mark.size));
            }
            // How many are carried, bottom right.
            let amount: u32 = inventory.iter().filter(|item| item.id == id && item.worn == 0).map(|item| u32::from(item.amount)).sum();
            count_text(layer, font, corner, &amount.to_string());
            if contains(corner, SLOT, cursor) {
                window::tooltip(tooltip, chrome, font, info.name(true), [corner[0], corner[1] + SLOT[1] + 2.0], [255; 3]);
            }
        }
    }
}

impl Hotkeys {
    /// The minus and plus buttons on screen (for the test driver): `hotkey_minus`, `hotkey_plus`.
    pub fn button_boxes(&self) -> Vec<(&'static str, [f32; 4])> {
        if !self.frame.open {
            return Vec::new();
        }
        let [minus, plus] = self.step_buttons();
        [
            ("hotkey_minus", minus, STEP_BUTTON),
            ("hotkey_plus", plus, STEP_BUTTON),
            ("hotkey_switch", SWITCH, SWITCH_SIZE),
            ("hotkey_quick", self.quick_button(), STEP_BUTTON),
        ]
        .map(|(name, local, size)| {
            let [x, y] = self.frame.at(local);
            (name, [x, y, size[0], size[1]])
        })
        .to_vec()
    }

    /// The bar row a shown slot is on.
    pub fn slot_row(&self, slot: usize) -> Option<usize> {
        self.slot_boxes().iter().position(|(index, _)| *index == slot).map(|visible| visible / PER_ROW)
    }

    /// The shown slots on screen: slot number and box.
    pub fn slot_boxes(&self) -> Vec<(usize, [f32; 4])> {
        if !self.frame.open {
            return Vec::new();
        }
        self.shown()
            .into_iter()
            .map(|(index, local)| {
                let corner = self.frame.at(hover_corner(local));
                (index, [corner[0], corner[1], SLOT[0], SLOT[1]])
            })
            .collect()
    }
}

/// Where an amount or level goes: its right edge at the slot's x + 27, its top at the slot's y + 17.
// ponytail: the game's 11 px size isn't one of the font's; it's drawn in the usual one.
fn count_place(corner: [f32; 2], width: f32) -> [f32; 2] {
    [corner[0] + 27.0 - width, corner[1] + 17.0]
}

/// A number at a slot's bottom right: black, outlined in white on four sides.
fn count_text(layer: &mut Layer<'_>, font: &Font, corner: [f32; 2], text: &str) {
    let place = count_place(corner, font.width(text));
    for nudge in [[-1.0, 0.0], [1.0, 0.0], [0.0, -1.0], [0.0, 1.0]] {
        layer.texts.push((text.to_owned(), [place[0] + nudge[0], place[1] + nudge[1]], [255; 3]));
    }
    layer.texts.push((text.to_owned(), place, [0, 0, 0]));
}

/// A slot's icon corner inside the bar.
fn slot_corner(slot: usize) -> [f32; 2] {
    [FIRST_SLOT[0] + (slot % PER_ROW) as f32 * SLOT_STEP, (slot / PER_ROW) as f32 * ROW_HEIGHT + FIRST_SLOT[1]]
}

/// A slot's hover (and hit) box corner from its icon's: a pixel lower.
fn hover_corner([x, y]: [f32; 2]) -> [f32; 2] {
    [x, y + HOVER_DROP]
}

/// What a key let go on slot `to` changes: the slot takes it, and every other slot in that row of 9 holding the same
/// skill or item is emptied. So moving a key along its row moves it, and to another row copies it.
pub fn drop_on(keys: &[Option<Hotkey>], key: Hotkey, to: usize) -> Vec<(usize, Option<Hotkey>)> {
    let row = to / PER_ROW * PER_ROW;
    let same = |held: &Option<Hotkey>| held.is_some_and(|held| held.skill == key.skill && held.id == key.id);
    (row..row + PER_ROW).filter(|&slot| slot != to && keys.get(slot).is_some_and(same)).map(|slot| (slot, None)).chain([(to, Some(key))]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn puts_the_hover_box_under_the_slot() {
        // Column c, row r: (16 + 29c, 5 + 33r), 24 square.
        let hover = |slot| hover_corner(slot_corner(slot));
        assert_eq!((hover(0), hover(3), hover(PER_ROW + 2)), ([16.0, 5.0], [103.0, 5.0], [74.0, 38.0]));
        assert_eq!(SLOT, [24.0, 24.0]);
    }

    #[test]
    fn a_key_moves_along_its_row_and_copies_to_another() {
        let heal = Hotkey { skill: true, id: 28, count: 10 };
        let potion = Hotkey { skill: false, id: 501, count: 30 };
        let mut keys = vec![None; ROWS * PER_ROW];
        keys[0] = Some(heal);
        keys[1] = Some(potion);
        keys[9] = Some(heal);
        // Along row 1: the old slot empties, the target is overwritten (the potion is lost, not swapped).
        assert_eq!(drop_on(&keys, heal, 1), vec![(0, None), (1, Some(heal))]);
        // Into row 2: row 1 keeps it; row 2's other Heal goes.
        assert_eq!(drop_on(&keys, heal, 12), vec![(9, None), (12, Some(heal))]);
        // The same item id as a skill id is another key.
        assert_eq!(drop_on(&keys, Hotkey { skill: false, id: 28, count: 1 }, 2), vec![(2, Some(Hotkey { skill: false, id: 28, count: 1 }))]);
    }

    #[test]
    fn lays_out_the_games_bar() {
        // Slots: icons at (16 + 29c, 4 + 33r), hit boxes a pixel lower.
        assert_eq!((slot_corner(0), slot_corner(8), slot_corner(9), slot_corner(20)), ([16.0, 4.0], [248.0, 4.0], [16.0, 37.0], [74.0, 70.0]));
        assert_eq!(hover_corner(slot_corner(10)), [45.0, 38.0]);
        assert_eq!(count_place([16.0, 4.0], 7.0), [36.0, 21.0]);
        let mut bar = Hotkeys::default();
        bar.frame.open = true;
        assert_eq!((bar.frame.position, bar.rows, bar.height()), ([230.0, 0.0], 2, 76.0));
        let at = |local: [f32; 2]| bar.frame.at(local);
        assert_eq!(bar.hit(at([20.0, 10.0])), Some(Hit::Slot(0)));
        assert_eq!(bar.hit(at([20.0, 43.0])), Some(Hit::Slot(9)));
        assert_eq!(bar.hit(at([5.0, 10.0])), Some(Hit::Switch));
        // The minus and plus buttons by the last row: (277, 66 - 28) and (277, 66 - 13).
        assert_eq!(bar.hit(at([280.0, 41.0])), Some(Hit::Minus));
        assert_eq!(bar.hit(at([280.0, 56.0])), Some(Hit::Plus));
        bar.click(Hit::Plus);
        bar.click(Hit::Plus);
        bar.click(Hit::Plus);
        assert_eq!(bar.rows, 4, "four at most");
        for _ in 0..5 {
            bar.click(Hit::Minus);
        }
        assert_eq!(bar.rows, 1, "one at least");
    }

    #[test]
    fn switches_between_the_two_bars() {
        let key = |id: u32| Some(Hotkey { skill: true, id, count: 1 });
        let mut bar = Hotkeys::default();
        // The server's two tabs: the first shows.
        bar.set_tab(1, 0, vec![key(2)]);
        bar.set_tab(0, 0, vec![key(1)]);
        assert_eq!((bar.tab(), bar.keys[0]), (0, key(1)));
        // The switch shows hotbar 2, and a list for it lands on the bar showing.
        bar.click(Hit::Switch);
        assert_eq!((bar.tab(), bar.keys[0]), (1, key(2)));
        bar.set_tab(1, 0, vec![key(3)]);
        assert_eq!(bar.keys[0], key(3));
        bar.switch();
        assert_eq!((bar.tab(), bar.keys[0], bar.keys.len()), (0, key(1), SLOTS));
    }

    #[test]
    fn keeps_each_bars_rotation() {
        let key = |id: u32| Some(Hotkey { skill: true, id, count: 1 });
        let mut bar = Hotkeys::default();
        bar.frame.open = true;
        // The server's rotations: hotbar 1 at 2, hotbar 2 at 1; all 38 slots kept.
        let mut keys = vec![None; SLOTS];
        keys[18] = key(5);
        keys[37] = key(6);
        bar.set_tab(0, 2, keys);
        bar.set_tab(1, 1, vec![None; SLOTS]);
        assert_eq!((bar.rotations(), bar.keys[37]), ([2, 1], key(6)));
        // Hotbar 1 draws its third row first: slot 18 is the first shown.
        assert_eq!(bar.slot_boxes()[0].0, 18);
        // Ctrl+F12 turns the bar showing and says what to keep; the other bar keeps its own.
        assert_eq!(bar.rotate(), (0, 3));
        assert_eq!(bar.rotate(), (0, 0));
        bar.switch();
        assert_eq!(bar.rotate(), (1, 2));
        assert_eq!(bar.rotations(), [0, 2]);
        assert_eq!(bar.slot_boxes()[0].0, 18, "hotbar 2 now starts at its third row");
    }

    #[test]
    fn opens_the_quick_item_row() {
        let mut bar = Hotkeys::default();
        bar.frame.open = true;
        let at = |bar: &Hotkeys, local: [f32; 2]| bar.frame.at(local);
        // Closed: two rows, the button at (247, h − 10) under them.
        assert_eq!((bar.height(), bar.quick_button()), (76.0, [247.0, 66.0]));
        assert_eq!(bar.hit(at(&bar, [250.0, 70.0])), Some(Hit::Quick));
        assert_eq!(bar.slot_boxes().len(), 2 * PER_ROW);
        bar.click(Hit::Quick);
        // Open: 33 taller, the two quick slots (36, 37) in the row under the last, the button moved down.
        assert_eq!((bar.height(), bar.quick_button()), (109.0, [247.0, 99.0]));
        let quick: Vec<(usize, [f32; 4])> = bar.slot_boxes().into_iter().skip(2 * PER_ROW).collect();
        assert_eq!(quick, [(36, [246.0, 71.0, 24.0, 24.0]), (37, [275.0, 71.0, 24.0, 24.0])]);
        assert_eq!(bar.hit(at(&bar, [50.0, 75.0])), Some(Hit::Slot(37)));
        assert_eq!(bar.slot_row(37), Some(2));
        // The quick slots don't turn with the rows.
        bar.rotate();
        assert_eq!(bar.slot_boxes()[2 * PER_ROW].0, 36);
        bar.click(Hit::Quick);
        assert_eq!(bar.height(), 76.0);
    }
}
