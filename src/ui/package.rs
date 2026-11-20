//! The package pick (the game's 0x2731 CUISelectPackageItemBox): using a box that the data's package table
//! (SelectPackageItem_Cln) lists opens it instead of using the item. The sets are on the left, the picked set's items on
//! the right; Confirm asks (3975) and then sends the picked row (CZ_USE_PACKAGEITEM), and the server swaps the box for
//! the items. Centred on every open, never saved; any spot that's no control drags it.

use std::collections::HashMap;

use crate::render::{Gpu, Texture};
use crate::text::Font;
use crate::window::{BUTTON_SIZE, Chrome, Frame, ScrollBar, TITLE_HEIGHT, Thumb};
use crate::{Layer, UI, contains, inventory, offset};
use rustnarok_client::grf::Grf;
use rustnarok_client::lua::{ItemInfo, PackageItem, PackageSet};

pub const SIZE: [f32; 2] = [387.0, 283.0];
/// Confirm, Close, and the question before sending ("You have selected %s. …", the set's name).
const CONFIRM: usize = 3384;
const CLOSE: usize = 3045;
pub const QUESTION: usize = 3975;
/// The footer strip's top.
const FOOTER_TOP: f32 = 254.0;
/// The title's box: black with a white shadow, centred in the bar's height.
const TITLE_AT: [f32; 2] = [18.0, 0.0];
/// The set list: its area, the scroll bar on its left, rows of 159 × 34 right of it, the name 11 px in.
const SETS: [f32; 4] = [1.0, 17.0, 172.0, 238.0];
const SET_ROW: [f32; 2] = [159.0, 34.0];
const SET_NAME: f32 = 11.0;
/// The item list: its area, rows of 200 × 34, the scroll bar at x 200 of it.
const ITEMS: [f32; 4] = [173.0, 17.0, 213.0, 238.0];
const ITEM_ROW: [f32; 2] = [200.0, 34.0];
const SHOWN: usize = 7;
/// The text buttons, 42 × 20.
const BUTTON: [f32; 2] = [42.0, 20.0];
const CONFIRM_AT: [f32; 2] = [279.0, 259.0];
const CLOSE_AT: [f32; 2] = [331.0, 259.0];
/// An item row's icon (its grade mark 12 down, the count at (14, 15)), and its name: a 148 × 24 box, cut to it.
const ICON_AT: [f32; 2] = [6.0, 5.0];
const GRADE_AT: [f32; 2] = [0.0, 12.0];
const COUNT_AT: [f32; 2] = [14.0, 15.0];
const NAME_AT: [f32; 2] = [45.0, 5.0];
const NAME_BOX: [f32; 2] = [148.0, 24.0];
const TEXT: [u8; 3] = [0, 0, 0];
const NAME_SHADOW: [u8; 3] = [0xE7; 3];

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hit {
    Title,
    Close,
    Set(usize),
    SetBar,
    Item(usize),
    ItemBar,
    Confirm,
    Window,
}

/// The rows' pictures (the folder's bg_select, btn_set_empty, btn_select_* and btn_close_* aren't this window's).
pub struct Assets {
    /// A set row: listed, picked.
    set_rows: [Option<Texture>; 2],
    /// An item row: plain, rental, random options, both.
    item_rows: [Option<Texture>; 4],
    grades: [Option<Texture>; 4],
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |name: String| gpu.try_load_texture(grf, &name, false).ok();
        let row = |name: &str| image(format!("{UI}selectpackage\\{name}.bmp"));
        Self {
            set_rows: ["btn_set_list", "btn_set_press"].map(row),
            item_rows: ["item_list", "item_list_time", "item_list_ranop", "item_list_timeran"].map(row),
            grades: [1, 2, 3, 4].map(|grade| image(format!("{UI}grade_enchant\\grade_icon{grade}.bmp"))),
        }
    }
}

pub struct PackagePick {
    /// The box's inventory index and item ID.
    pub index: u16,
    pub item: u32,
    pub sets: Vec<PackageSet>,
    pub picked: Option<usize>,
    pub set_first: usize,
    pub item_first: usize,
    pub thumb: Thumb,
    /// The held thumb is the set list's (else the item list's).
    thumb_on_sets: bool,
    /// Confirm's question is up.
    pub asked: bool,
    pub frame: Frame,
}

/// Where it opens: the middle of the screen.
pub fn place(screen: [f32; 2]) -> [f32; 2] {
    [((screen[0] - SIZE[0]) / 2.0).floor(), ((screen[1] - SIZE[1]) / 2.0).floor()]
}

/// The picture an item's row uses: rental, random options, both, or plain.
fn row_kind(item: &PackageItem) -> usize {
    usize::from(item.hours > 0) + 2 * usize::from(item.random_option)
}

impl PackagePick {
    pub fn new(index: u16, item: u32, sets: Vec<PackageSet>, screen: [f32; 2]) -> Self {
        let mut frame = Frame::new(place(screen), SIZE[0]);
        frame.open = true;
        Self { index, item, sets, picked: None, set_first: 0, item_first: 0, thumb: Thumb::default(), thumb_on_sets: false, asked: false, frame }
    }

    pub fn items(&self) -> &[PackageItem] {
        self.picked.and_then(|set| self.sets.get(set)).map_or(&[], |set| &set.items)
    }

    pub fn set_bar(&self) -> ScrollBar {
        ScrollBar { at: self.frame.at([SETS[0], SETS[1]]), height: SETS[3], shown: SHOWN, rows: self.sets.len(), first: self.set_first }
    }

    pub fn item_bar(&self) -> ScrollBar {
        let at = self.frame.at([ITEMS[0] + ITEM_ROW[0], ITEMS[1]]);
        ScrollBar { at, height: ITEMS[3], shown: SHOWN, rows: self.items().len(), first: self.item_first }
    }

    /// A set row's corner inside the window: right of the bar.
    fn set_row(row: usize) -> [f32; 2] {
        [SETS[0] + ScrollBar::WIDTH, SETS[1] + row as f32 * SET_ROW[1]]
    }

    fn item_row(row: usize) -> [f32; 2] {
        [ITEMS[0], ITEMS[1] + row as f32 * ITEM_ROW[1]]
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let local = self.frame.local(cursor, SIZE[1])?;
        if contains(self.frame.close_button(), BUTTON_SIZE, local) {
            return Some(Hit::Close);
        }
        if local[1] < TITLE_HEIGHT {
            return Some(Hit::Title);
        }
        if contains(CONFIRM_AT, BUTTON, local) {
            return Some(Hit::Confirm);
        }
        if contains(CLOSE_AT, BUTTON, local) {
            return Some(Hit::Close);
        }
        if self.set_bar().hit(cursor).is_some() {
            return Some(Hit::SetBar);
        }
        if self.item_bar().hit(cursor).is_some() {
            return Some(Hit::ItemBar);
        }
        let set = (0..SHOWN).find(|&row| contains(Self::set_row(row), SET_ROW, local)).map(|row| row + self.set_first);
        if let Some(set) = set.filter(|&set| set < self.sets.len()) {
            return Some(Hit::Set(set));
        }
        let item = (0..SHOWN).find(|&row| contains(Self::item_row(row), ITEM_ROW, local)).map(|row| row + self.item_first);
        if let Some(item) = item.filter(|&item| item < self.items().len()) {
            return Some(Hit::Item(item));
        }
        Some(Hit::Window)
    }

    /// A press on a set picks it (its items from the top); on a bar, scrolls that list.
    pub fn press(&mut self, cursor: [f32; 2]) {
        match self.hit(cursor) {
            Some(Hit::Set(set)) => {
                self.picked = Some(set);
                self.item_first = 0;
            }
            Some(Hit::SetBar) => {
                self.thumb_on_sets = true;
                self.set_first = self.set_bar().press(cursor, &mut self.thumb).unwrap_or(self.set_first);
            }
            Some(Hit::ItemBar) => {
                self.thumb_on_sets = false;
                self.item_first = self.item_bar().press(cursor, &mut self.thumb).unwrap_or(self.item_first);
            }
            _ => {}
        }
    }

    /// A held thumb follows the pointer (whichever list it belongs to).
    pub fn drag_thumb(&mut self, y: f32) {
        let Some(first) = self.thumb.first(y) else { return };
        if self.thumb_on_sets {
            self.set_first = first;
        } else {
            self.item_first = first;
        }
    }

    /// The wheel over a list scrolls it a row a step.
    pub fn wheel(&mut self, cursor: [f32; 2], rows: i32) {
        let Some(local) = self.frame.local(cursor, SIZE[1]) else { return };
        if local[0] < ITEMS[0] {
            self.set_first = (self.set_first as i32 + rows).clamp(0, self.set_bar().range() as i32) as usize;
        } else {
            self.item_first = (self.item_first as i32 + rows).clamp(0, self.item_bar().range() as i32) as usize;
        }
    }

    /// The picked set, for the question.
    pub fn picked_set(&self) -> Option<&PackageSet> {
        self.picked.and_then(|set| self.sets.get(set))
    }

    /// Item icons the window shows.
    pub fn icons(&self, items: &HashMap<u32, ItemInfo>) -> Vec<String> {
        self.items().iter().filter_map(|item| items.get(&item.item)).map(|info| inventory::icon_path(info, true)).collect()
    }

    /// The buttons and rows on screen (for the test driver): `package_confirm`, `package_close`, `package_set:<n>`.
    pub fn button_boxes(&self) -> Vec<(String, [f32; 4])> {
        let place = |name: String, local: [f32; 2], [w, h]: [f32; 2]| {
            let [x, y] = self.frame.at(local);
            (name, [x, y, w, h])
        };
        let mut boxes = vec![place("package_confirm".into(), CONFIRM_AT, BUTTON), place("package_close".into(), CLOSE_AT, BUTTON)];
        for row in 0..SHOWN.min(self.sets.len().saturating_sub(self.set_first)) {
            boxes.push(place(format!("package_set:{}", self.set_first + row), Self::set_row(row), SET_ROW));
        }
        boxes
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &self,
        layer: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        icons: &'a HashMap<String, Option<Texture>>,
        items: &HashMap<u32, ItemInfo>,
        messages: &[String],
        font: &Font,
        cursor: [f32; 2],
    ) {
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        let corner = self.frame.position;
        // The frame: the title bar, a white body inside a 1 px outline, the 29 px footer along y 254.
        chrome.draw_title_bar(layer, corner, SIZE[0]);
        chrome.draw_body(layer, corner, SIZE);
        chrome.draw_footer(layer, offset(corner, [0.0, SIZE[1]]), SIZE[0], SIZE[1] - FOOTER_TOP);
        let base = chrome.button(0, false);
        layer.sprites.push((base, self.frame.at([3.0, 3.0]), base.size));
        let on_close = self.frame.local(cursor, SIZE[1]).is_some_and(|local| contains(self.frame.close_button(), BUTTON_SIZE, local));
        let close = chrome.button(2, on_close);
        layer.sprites.push((close, self.frame.at(self.frame.close_button()), close.size));
        // The title: the box's name, black over a white shadow.
        let title = items.get(&self.item).map_or_else(String::new, |info| info.name(true).to_owned());
        let title_at = self.frame.at(offset(TITLE_AT, [0.0, ((TITLE_HEIGHT - font.line_height()) / 2.0).trunc()]));
        layer.texts.push((title.clone(), offset(title_at, [1.0, 1.0]), [255; 3]));
        layer.texts.push((title, title_at, TEXT));
        // The sets: the picked one pressed, the name left in it and centred in its height.
        // ponytail: the name is cut to whole letters, not clipped at the 148 px box's edge.
        for (row, set) in self.sets.iter().enumerate().skip(self.set_first).take(SHOWN) {
            let at = self.frame.at(Self::set_row(row - self.set_first));
            if let Some(picture) = &assets.set_rows[usize::from(self.picked == Some(row))] {
                layer.sprites.push((picture, at, SET_ROW));
            }
            let name = crate::window::fit(font, &set.name, SET_ROW[0] - SET_NAME);
            layer.texts.push((name, offset(at, [SET_NAME, ((SET_ROW[1] - font.line_height()) / 2.0).trunc()]), TEXT));
        }
        self.set_bar().draw(layer, chrome);
        // The picked set's items: the row by its kind, the icon with its grade and count, the name (with the refine).
        // ponytail: no tooltip or description over the icon yet.
        for (row, item) in self.items().iter().enumerate().skip(self.item_first).take(SHOWN) {
            let at = self.frame.at(Self::item_row(row - self.item_first));
            if let Some(picture) = &assets.item_rows[row_kind(item)] {
                layer.sprites.push((picture, at, ITEM_ROW));
            }
            let icon_at = offset(at, ICON_AT);
            if let Some(Some(icon)) = items.get(&item.item).and_then(|info| icons.get(&inventory::icon_path(info, true))) {
                layer.sprites.push((icon, icon_at, icon.size));
            }
            if let Some(Some(grade)) = usize::from(item.grade).checked_sub(1).and_then(|grade| assets.grades.get(grade)) {
                layer.sprites.push((grade, offset(icon_at, GRADE_AT), grade.size));
            }
            if item.count > 0 {
                let count = offset(icon_at, COUNT_AT);
                for step in [[-1.0, 0.0], [1.0, 0.0], [0.0, -1.0], [0.0, 1.0]] {
                    layer.texts.push((item.count.to_string(), offset(count, step), [255; 3]));
                }
                layer.texts.push((item.count.to_string(), count, TEXT));
            }
            let name = crate::window::fit(font, &inventory::item_name(items, item.item, true, item.refine), NAME_BOX[0]);
            let name_at = offset(at, offset(NAME_AT, [0.0, ((NAME_BOX[1] - font.line_height()) / 2.0).trunc()]));
            layer.texts.push((name.clone(), offset(name_at, [1.0, 1.0]), NAME_SHADOW));
            layer.texts.push((name, name_at, TEXT));
        }
        self.item_bar().draw(layer, chrome);
        for (place, label) in [(CONFIRM_AT, CONFIRM), (CLOSE_AT, CLOSE)] {
            let label = message(label);
            let width = BUTTON[0].max(font.width(label) + 8.0);
            let on = self.frame.local(cursor, SIZE[1]).is_some_and(|local| contains(place, [width, BUTTON[1]], local));
            chrome.draw_text_button(layer, font, self.frame.at(place), width, label, usize::from(on));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: [f32; 2] = [1024.0, 768.0];

    fn set(number: u32, items: usize) -> PackageSet {
        let item = PackageItem { item: 501, count: 1, hours: 0, refine: 0, random_option: false, grade: 0 };
        PackageSet { number, name: format!("Set {number}"), items: vec![item; items] }
    }

    #[test]
    fn picks_a_set_and_lists_its_items() {
        let mut pick = PackagePick::new(8, 57913, (0..9).map(|number| set(number, number as usize + 1)).collect(), SCREEN);
        assert_eq!(pick.frame.position, [318.0, 242.0]);
        let at = |local: [f32; 2]| offset([318.0, 242.0], local);
        assert_eq!(pick.hit(at([100.0, 8.0])), Some(Hit::Title));
        assert_eq!(pick.hit(at([376.0, 6.0])), Some(Hit::Close));
        assert_eq!(pick.hit(at([290.0, 265.0])), Some(Hit::Confirm));
        assert_eq!(pick.hit(at([340.0, 265.0])), Some(Hit::Close));
        // Nothing picked: no items; the set list scrolls (9 sets, 7 shown), its bar on the left.
        assert!(pick.items().is_empty());
        assert_eq!(pick.hit(at([5.0, 30.0])), Some(Hit::SetBar));
        assert_eq!(pick.hit(at([60.0, 17.0 + 34.0 + 5.0])), Some(Hit::Set(1)));
        pick.press(at([60.0, 17.0 + 34.0 + 5.0]));
        assert_eq!((pick.picked, pick.items().len(), pick.picked_set().map(|set| set.number)), (Some(1), 2, Some(1)));
        assert_eq!(pick.hit(at([200.0, 17.0 + 34.0 + 5.0])), Some(Hit::Item(1)));
        assert_eq!(pick.hit(at([200.0, 17.0 + 2.0 * 34.0 + 5.0])), Some(Hit::Window), "past the set's items");
        pick.wheel(at([60.0, 100.0]), 5);
        assert_eq!(pick.set_first, 2, "as far as it scrolls");
        assert_eq!(pick.hit(at([60.0, 22.0])), Some(Hit::Set(2)));
    }

    #[test]
    fn a_row_shows_rental_and_random_options() {
        let item = PackageItem { item: 1, count: 1, hours: 0, refine: 0, random_option: false, grade: 0 };
        let kinds =
            [(0, false), (24, false), (0, true), (24, true)].map(|(hours, random_option)| row_kind(&PackageItem { hours, random_option, ..item.clone() }));
        assert_eq!(kinds, [0, 1, 2, 3]);
    }
}
