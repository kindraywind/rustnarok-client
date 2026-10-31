//! The hotkey bar (F1-F9, F12 shows or hides it): items and skills kept at hand, rows of nine. Each of the two bars
//! keeps its own row rotation (Ctrl+F12) on the server. The quick slot window beside it holds the server's two last
//! slots of hotbar 1; the Control tab's Quick Slots switch shows it.

use std::collections::HashMap;

use rustnarok_client::grf::Grf;
use rustnarok_client::lua::{ItemInfo, SkillInfo};
use rustnarok_client::net::{Hotkey, InventoryItem};
use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};
use crate::window::{self, Chrome, Frame};
use crate::{Layer, UI, contains, inventory, offset, screen};

// The game's bar: 290 wide, a 34-high shortcut frame a row (33 apart, overlapping by one), slots 29 apart.
const WIDTH: f32 = 290.0;
const ROW_HEIGHT: f32 = 33.0;
pub const PER_ROW: usize = 9;
const ROWS: usize = 4;
/// The server keeps 38 slots a bar: four rows of nine, then the two quick slots (only hotbar 1's are used).
const SLOTS: usize = ROWS * PER_ROW + 2;
const QUICK: [usize; 2] = [ROWS * PER_ROW, ROWS * PER_ROW + 1];
/// The quick slot window: its picture's 77×34, at (W − 124, 0) until moved, the slots at (5, 5) and (34, 5), held to
/// move by the striped margin right of x 59.
pub const QUICK_SIZE: [f32; 2] = [77.0, 34.0];
const QUICK_FROM_RIGHT: f32 = 124.0;
const QUICK_FIRST: [f32; 2] = [5.0, 5.0];
const QUICK_GRIP: f32 = 59.0;
const SLOT: [f32; 2] = [24.0, 24.0];
/// The page switch's tooltip, "Switch Hotkey Bar".
const SWITCH_TIP: usize = 3596;
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
/// The bar is held to move by its margins: left of x 15, from x 276 on, above y 4.
const MARGIN_RIGHT: f32 = 276.0;
const DEFAULT_ROWS: usize = 2;
/// What `clear_of` leaves between the bar and the window it moved away from.
const CLEARANCE: f32 = 4.0;

pub struct Assets {
    frame: Texture,
    /// The quick slot window's picture.
    quick: Texture,
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
            quick: image(format!("{UI}basic_interface\\quickslot.bmp")),
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
    /// The quick slot window's margin that moves it.
    QuickGrip,
    Slot(usize),
    Window,
}

pub struct Hotkeys {
    pub frame: Frame,
    /// The bar showing's four rows of nine (the server's tab `tab`), then hotbar 1's two quick slots whichever shows.
    pub keys: Vec<Option<Hotkey>>,
    /// The other bar's rows.
    other: Vec<Option<Hotkey>>,
    /// The bar showing: the server's tab 0 (hotbar 1) or 1 (hotbar 2).
    tab: u16,
    /// How many rows show.
    rows: usize,
    /// Which row each bar (the server's tab) draws first. Ctrl+F12 walks the showing bar's.
    first: [usize; 2],
    /// The quick slot window, open while the Quick Slots switch is on.
    pub quick_window: Frame,
    /// A slot is used by a double click; a single press only starts a drag.
    pub clicks: window::Clicks,
}

impl Default for Hotkeys {
    fn default() -> Self {
        let mut frame = Frame::new(DEFAULT_POSITION, WIDTH);
        frame.open = true;
        let mut quick_window = Frame::new([screen()[0] - QUICK_FROM_RIGHT, 0.0], QUICK_SIZE[0]);
        quick_window.open = true;
        Self {
            frame,
            keys: vec![None; SLOTS],
            other: vec![None; SLOTS],
            tab: 0,
            rows: DEFAULT_ROWS,
            first: [0; 2],
            quick_window,
            clicks: window::Clicks::default(),
        }
    }
}

impl Hotkeys {
    /// 33 a row plus 10.
    pub fn height(&self) -> f32 {
        ROW_HEIGHT * self.rows as f32 + 10.0
    }

    /// The bar's slots showing: key index and icon corner in the bar, row by row.
    fn shown(&self) -> Vec<(usize, [f32; 2])> {
        (0..self.rows * PER_ROW).map(|visible| (self.key_index(visible), slot_corner(visible))).collect()
    }

    /// The quick slot window's two slots while it's open: key index and icon corner on screen (also its hover box).
    fn quick_slots(&self) -> Vec<(usize, [f32; 2])> {
        let slots =
            QUICK.into_iter().enumerate().map(|(column, index)| (index, self.quick_window.at([QUICK_FIRST[0] + column as f32 * SLOT_STEP, QUICK_FIRST[1]])));
        slots.filter(|_| self.quick_window.open).collect()
    }

    /// Where the minus and plus buttons sit: by the last row.
    fn step_buttons(&self) -> [[f32; 2]; 2] {
        let bottom = ROW_HEIGHT * self.rows as f32;
        [[277.0, bottom - 28.0], [277.0, bottom - 13.0]]
    }

    /// The slot (row by row) or button under the pointer; the quick slot window first, drawn over the bar.
    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        if let Some(local) = self.quick_window.local(cursor, QUICK_SIZE[1]) {
            let slot = self.quick_slots().into_iter().find(|&(_, corner)| contains(corner, SLOT, cursor));
            return Some(slot.map_or(if local[0] > QUICK_GRIP { Hit::QuickGrip } else { Hit::Window }, |(index, _)| Hit::Slot(index)));
        }
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
        Some(self.shown().into_iter().find(|&(_, corner)| contains(hover_corner(corner), SLOT, local)).map_or(Hit::Window, |(index, _)| Hit::Slot(index)))
    }

    /// Whether a press here moves the bar: on its margins (left of the slots, right of them or above them), not on a
    /// slot or a button.
    pub fn grabbed(&self, cursor: [f32; 2]) -> bool {
        let Some(local) = self.frame.local(cursor, self.height()).filter(|_| self.quick_window.local(cursor, QUICK_SIZE[1]).is_none()) else {
            return false;
        };
        self.hit(cursor) == Some(Hit::Window) && (local[0] < FIRST_SLOT[0] - 1.0 || local[0] >= MARGIN_RIGHT || local[1] < FIRST_SLOT[1])
    }

    /// Moves the bar right of `other` (x, y, width, height) when they overlap and the bar still fits on a screen `screen_width`
    /// wide that way; false when it stayed.
    pub fn clear_of(&mut self, other: [f32; 4], screen_width: f32) -> bool {
        let [x, y] = self.frame.position;
        let overlaps = x < other[0] + other[2] && other[0] < x + WIDTH && y < other[1] + other[3] && other[1] < y + self.height();
        let clear = (other[0] + other[2] + CLEARANCE).ceil();
        if !self.frame.open || self.frame.dragging() || !overlaps || clear + WIDTH > screen_width {
            return false;
        }
        self.frame.position[0] = clear;
        true
    }

    /// The server's tab of the bar showing: 0 for hotbar 1, 1 for hotbar 2.
    pub fn tab(&self) -> u16 {
        self.tab
    }

    /// The server's tab a slot is kept on: the bar showing's, but hotbar 1's for the quick slots.
    pub fn tab_of(&self, slot: usize) -> u16 {
        if QUICK.contains(&slot) { 0 } else { self.tab }
    }

    /// The bar switch: hotbar 1 and 2 trade their rows (the quick slots stay hotbar 1's).
    pub fn switch(&mut self) {
        self.keys[..QUICK[0]].swap_with_slice(&mut self.other[..QUICK[0]]);
        self.tab ^= 1;
    }

    /// The server's list for tab `tab` (0 or 1), the bar showing or the other one, with that bar's rotation. Only
    /// hotbar 1's quick slots are kept, as the game does.
    pub fn set_tab(&mut self, tab: u16, rotate: u8, keys: Vec<Option<Hotkey>>) {
        let bar = if tab == self.tab { &mut self.keys } else { &mut self.other };
        for (slot, key) in bar[..QUICK[0]].iter_mut().zip(&keys) {
            *slot = *key;
        }
        if tab == 0 {
            for (slot, key) in self.keys[QUICK[0]..].iter_mut().zip(keys.iter().skip(QUICK[0])) {
                *slot = *key;
            }
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
            Hit::QuickGrip | Hit::Slot(_) | Hit::Window => {}
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
        learnt: &[rustnarok_client::net::Skill],
        inventory: &[InventoryItem],
        font: &Font,
        messages: &[String],
        cursor: [f32; 2],
        cooling: &crate::skills::Cooling<'a>,
        keys: &crate::shortcuts::Shortcuts,
    ) {
        if self.frame.open {
            for row in 0..self.rows {
                let top = row as f32 * ROW_HEIGHT;
                layer.sprites.push((&assets.frame, self.frame.at([0.0, top]), assets.frame.size));
                let number = ((self.rotation() + row) % ROWS + 1).to_string();
                layer.texts.push((number, self.frame.at([ROW_NUMBER[0], top + ROW_NUMBER[1]]), ROW_NUMBER_COLOR));
            }
            layer.sprites.push((&assets.switch[usize::from(self.tab)], self.frame.at(SWITCH), SWITCH_SIZE));
            if contains(self.frame.at(SWITCH), SWITCH_SIZE, cursor) {
                let text = messages.get(SWITCH_TIP).map_or("", String::as_str);
                if !text.is_empty() {
                    window::tooltip(tooltip, chrome, font, text, self.frame.at(offset(SWITCH, [0.0, -20.0])), [255; 3]);
                }
            }
            let [minus, plus] = self.step_buttons();
            layer.sprites.push((&assets.minus, self.frame.at(minus), STEP_BUTTON));
            layer.sprites.push((&assets.plus, self.frame.at(plus), STEP_BUTTON));
        }
        if self.quick_window.open {
            layer.sprites.push((&assets.quick, self.quick_window.at([0.0, 0.0]), QUICK_SIZE));
        }
        let bar = if self.tab() == 0 { 1 } else { 2 };
        // Each slot: its key index, its icon's corner, its hover box's (the bar's a pixel lower), and its tooltip's corner:
        // 17 px above the window at the slot's column. The bar's slots name their key; the quick slots don't.
        let bar_slots = self
            .shown()
            .into_iter()
            .filter(|_| self.frame.open)
            .map(|(index, local)| (index, self.frame.at(local), self.frame.at(hover_corner(local)), self.frame.at([local[0] - 1.0, -17.0]), true));
        let quick_slots = self
            .quick_slots()
            .into_iter()
            .enumerate()
            .map(|(column, (index, corner))| (index, corner, corner, self.quick_window.at([4.0 + column as f32 * SLOT_STEP, -17.0]), false));
        // The quick window is drawn over the bar, so its slots go last.
        // ponytail: one layer for both; where they overlap, the bar's icons and numbers show through the quick window.
        for (index, corner, hover, tip_corner, keyed) in bar_slots.chain(quick_slots).collect::<Vec<_>>() {
            let key = self.keys.get(index).and_then(|key| *key);
            let pressed = keyed.then(|| keys.hotbar_key(bar, index).unwrap_or_default());
            // The slot under the pointer: a box under its icon, and its tooltip.
            let pointed = contains(hover, SLOT, cursor);
            if pointed {
                layer.sprites.push((&assets.hover, hover, SLOT));
            }
            let mut tip = |skill: Option<(&str, u16, i32)>, item: Option<(&str, u32)>| {
                if let Some(text) = slot_tip(messages, pressed.as_deref(), skill, item).filter(|_| pointed) {
                    window::tooltip(tooltip, chrome, font, &text, tip_corner, [255; 3]);
                }
            };
            // A skill: its icon; on hover its name, the level it's used at (no higher than learnt) and that level's SP.
            if let Some(Hotkey { skill: true, id, count }) = key.as_ref()
                && let Some(info) = u16::try_from(*id).ok().and_then(|id| skills.get(&id))
            {
                if let Some(Some(icon)) = icons.get(&crate::skills::icon_path(info)) {
                    layer.sprites.push((icon, corner, icon.size));
                }
                // The level it's used at, bottom right, as an item's amount; a clock while it's blocked.
                count_text(layer, font, corner, &count.to_string());
                cooling.draw(layer, *id as u16, corner, [24.0, 24.0]);
                let used = learnt.iter().find(|skill| u32::from(skill.id) == *id).map(|skill| u16::try_from(*count).unwrap_or(0).min(skill.level).max(1));
                let sp = |level: u16| info.sp_amount.get(usize::from(level) - 1).copied().unwrap_or(0);
                tip(used.map(|level| (info.name.as_str(), level, sp(level))), None);
                continue;
            }
            let Some(Hotkey { skill: false, id, .. }) = key else {
                tip(None, None);
                continue;
            };
            let Some(info) = items.get(&id) else {
                tip(None, None);
                continue;
            };
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
            let name = info.name(true);
            tip(None, (amount > 0).then_some((name, amount)));
        }
    }
}

/// A slot's tooltip: the key that uses it (the bar's slots; none on the quick slots), then a skill's
/// "name Use Lv n (Sp : n)" or a carried item's "name: amount" (184; the quick slots "name: n ea.", 183). An empty slot,
/// an item not carried or a skill not learnt shows the key alone.
pub fn slot_tip(messages: &[String], key: Option<&str>, skill: Option<(&str, u16, i32)>, item: Option<(&str, u32)>) -> Option<String> {
    let message = |id: usize| messages.get(id).map_or("", String::as_str);
    let fill = rustnarok_client::messages::fill;
    let text = match (skill, item, key) {
        (Some((name, level, sp)), _, key) => format!("{} {name} Use Lv {level} (Sp : {sp})", key.unwrap_or_default()),
        (None, Some((name, amount)), Some(key)) => fill(message(184), &[&key, &name, &amount]),
        (None, Some((name, amount)), None) => fill(message(183), &[&name, &amount]),
        (None, None, key) => key.unwrap_or_default().to_owned(),
    };
    let text = text.trim_start();
    (!text.is_empty()).then(|| text.to_owned())
}

impl Hotkeys {
    /// The buttons on screen (for the test driver): `hotkey_minus`, `hotkey_plus`, `hotkey_switch`, and the quick slot
    /// window's margin that moves it, `quick_slot_grip`.
    pub fn button_boxes(&self) -> Vec<(&'static str, [f32; 4])> {
        let place = |frame: &Frame, (name, local, size): (&'static str, [f32; 2], [f32; 2])| {
            let [x, y] = frame.at(local);
            (name, [x, y, size[0], size[1]])
        };
        let [minus, plus] = self.step_buttons();
        let bar = [("hotkey_minus", minus, STEP_BUTTON), ("hotkey_plus", plus, STEP_BUTTON), ("hotkey_switch", SWITCH, SWITCH_SIZE)];
        let grip = ("quick_slot_grip", [QUICK_GRIP + 1.0, 0.0], [QUICK_SIZE[0] - QUICK_GRIP - 1.0, QUICK_SIZE[1]]);
        let bar = bar.into_iter().filter(|_| self.frame.open).map(|button| place(&self.frame, button));
        bar.chain([grip].into_iter().filter(|_| self.quick_window.open).map(|button| place(&self.quick_window, button))).collect()
    }

    /// What a right press on `slot` describes: the carried stack of the item on it. A skill, an empty slot or an item
    /// not carried: nothing.
    pub fn carried<'a>(&self, slot: usize, inventory: &'a [InventoryItem]) -> Option<&'a InventoryItem> {
        let key = self.keys.get(slot).copied().flatten().filter(|key| !key.skill)?;
        inventory.iter().find(|item| item.id == key.id)
    }

    /// Where a shown slot's row is, for a description to open under or over it: its window's top and the row in it.
    pub fn slot_place(&self, slot: usize) -> Option<(f32, usize)> {
        if QUICK.contains(&slot) {
            return self.quick_window.open.then_some((self.quick_window.position[1], 0));
        }
        let visible = self.shown().iter().position(|(index, _)| *index == slot).filter(|_| self.frame.open)?;
        Some((self.frame.position[1], visible / PER_ROW))
    }

    /// The shown slots on screen, the bar's then the quick slot window's: slot number and box.
    pub fn slot_boxes(&self) -> Vec<(usize, [f32; 4])> {
        let bar = self.shown().into_iter().filter(|_| self.frame.open).map(|(index, local)| (index, self.frame.at(hover_corner(local))));
        bar.chain(self.quick_slots()).map(|(index, corner)| (index, [corner[0], corner[1], SLOT[0], SLOT[1]])).collect()
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

/// An item on a slot. Its count is 0: rathena keeps it as the hotkey's level, at most 255 in its `hotkey` table, and
/// never reads it for an item (a stack's amount there failed the whole hotkey save). The bar shows the amount carried.
pub fn item_key(id: u32) -> Hotkey {
    Hotkey { skill: false, id, count: 0 }
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
    fn slot_tips_lead_with_the_key() {
        let mut messages = vec![String::new(); 185];
        messages[183] = "%s: %d ea.".into();
        messages[184] = "%s %s: %d".into();
        let heal = Some(("Heal", 10, 40));
        assert_eq!(slot_tip(&messages, Some("F1"), heal, None).as_deref(), Some("F1 Heal Use Lv 10 (Sp : 40)"));
        assert_eq!(slot_tip(&messages, Some("F2"), None, Some(("Red Potion", 30))).as_deref(), Some("F2 Red Potion: 30"));
        // An empty slot, or an item not carried: the key alone; with no key, nothing.
        assert_eq!(slot_tip(&messages, Some("F3"), None, None).as_deref(), Some("F3"));
        assert_eq!(slot_tip(&messages, Some(""), None, None), None);
        // The quick slots name no key and count items as "ea.".
        assert_eq!(slot_tip(&messages, None, heal, None).as_deref(), Some("Heal Use Lv 10 (Sp : 40)"));
        assert_eq!(slot_tip(&messages, None, None, Some(("Red Potion", 30))).as_deref(), Some("Red Potion: 30 ea."));
    }

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
    fn a_right_press_describes_a_carried_item_key() {
        let mut bar = Hotkeys::default();
        bar.keys[0] = Some(Hotkey { skill: true, id: 501, count: 10 });
        bar.keys[2] = Some(item_key(501));
        bar.keys[3] = Some(item_key(909));
        bar.keys[QUICK[0]] = Some(item_key(501));
        let potion = InventoryItem { index: 4, id: 501, amount: 5, ..Default::default() };
        let inventory = [InventoryItem { index: 2, id: 1201, ..Default::default() }, potion.clone()];
        assert_eq!(bar.carried(2, &inventory), Some(&potion));
        assert_eq!(bar.carried(QUICK[0], &inventory), Some(&potion));
        // A skill with the same id, an item not carried, an empty slot: nothing to describe.
        assert_eq!((bar.carried(0, &inventory), bar.carried(3, &inventory), bar.carried(4, &inventory)), (None, None, None));
    }

    #[test]
    fn an_item_key_has_no_count() {
        // Not the stack's amount: over 255 it doesn't fit the server's hotkey level.
        assert_eq!(item_key(569), Hotkey { skill: false, id: 569, count: 0 });
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
        // Held by its margins it moves; not on a slot, a gap between slots, or a button.
        assert!(bar.grabbed(at([12.0, 50.0])) && bar.grabbed(at([100.0, 2.0])) && bar.grabbed(at([285.0, 10.0])));
        assert!(!bar.grabbed(at([20.0, 10.0])) && !bar.grabbed(at([42.0, 10.0])) && !bar.grabbed(at([5.0, 10.0])) && !bar.grabbed(at([280.0, 41.0])));
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
    fn lays_out_the_quick_slot_window() {
        let mut bar = Hotkeys::default();
        // 77×34 at (W − 124, 0), the slots 36 and 37 at (5, 5) and (34, 5); the bar has its two rows only.
        assert_eq!((bar.quick_window.position, bar.quick_window.open, bar.height()), ([900.0, 0.0], true, 76.0));
        let quick: Vec<(usize, [f32; 4])> = bar.slot_boxes().into_iter().skip(2 * PER_ROW).collect();
        assert_eq!(quick, [(36, [905.0, 5.0, 24.0, 24.0]), (37, [934.0, 5.0, 24.0, 24.0])]);
        let at = |local: [f32; 2]| [900.0 + local[0], local[1]];
        assert_eq!(bar.hit(at([6.0, 6.0])), Some(Hit::Slot(36)));
        assert_eq!(bar.hit(at([57.0, 28.0])), Some(Hit::Slot(37)));
        // The striped margin right of x 59 moves it; the rest of it is neither a slot nor the bar's margin.
        assert_eq!(bar.hit(at([65.0, 20.0])), Some(Hit::QuickGrip));
        assert_eq!(bar.hit(at([2.0, 31.0])), Some(Hit::Window));
        assert!(!bar.grabbed(at([2.0, 31.0])));
        // A description opens under or over its window's only row.
        assert_eq!((bar.slot_place(37), bar.slot_place(9)), (Some((0.0, 0)), Some((0.0, 1))));
        // F12 hides the bar, not the quick slots; the switch off hides them.
        bar.frame.open = false;
        assert_eq!(bar.slot_boxes().iter().map(|(slot, _)| *slot).collect::<Vec<_>>(), [36, 37]);
        bar.quick_window.open = false;
        assert_eq!((bar.hit(at([6.0, 6.0])), bar.slot_place(36), bar.slot_boxes().len()), (None, None, 0));
    }

    #[test]
    fn keeps_hotbar_1s_quick_slots_on_either_bar() {
        let key = |id: u32| Some(Hotkey { skill: true, id, count: 1 });
        let list = |row: u32, quick: u32| (0..SLOTS).map(|slot| if slot < QUICK[0] { key(row) } else { key(quick + slot as u32) }).collect();
        let mut bar = Hotkeys::default();
        bar.set_tab(0, 0, list(1, 100));
        bar.set_tab(1, 0, list(2, 200));
        // Only hotbar 1's two are kept, and kept on hotbar 1 when they change.
        assert_eq!((bar.keys[0], bar.keys[36], bar.keys[37], bar.tab_of(36), bar.tab_of(5)), (key(1), key(136), key(137), 0, 0));
        bar.switch();
        assert_eq!((bar.keys[0], bar.keys[36], bar.tab_of(37), bar.tab_of(5)), (key(2), key(136), 0, 1));
        // A list for hotbar 2 while it shows leaves them too.
        bar.set_tab(1, 0, list(3, 300));
        assert_eq!((bar.keys[0], bar.keys[36]), (key(3), key(136)));
        bar.switch();
        assert_eq!((bar.keys[0], bar.keys[37]), (key(1), key(137)));
    }

    #[test]
    fn moves_out_of_the_way_of_a_window_that_grew() {
        let mut bar = Hotkeys::default();
        let at = |bar: &Hotkeys| bar.frame.position;
        // The default place, right of a 220 wide window: nothing to do. A 234 wide one overlaps and the bar steps aside.
        assert!(!bar.clear_of([0.0, 0.0, 220.0, 150.0], 1024.0));
        assert!(bar.clear_of([0.0, 0.0, 234.0, 150.0], 1024.0));
        assert_eq!(at(&bar), [238.0, 0.0]);
        // Not a window below it, nor one it can't clear on a narrow screen.
        assert!(!bar.clear_of([0.0, 300.0, 400.0, 100.0], 1024.0));
        assert!(!bar.clear_of([200.0, 0.0, 300.0, 100.0], 700.0));
        assert_eq!(at(&bar), [238.0, 0.0]);
    }
}
