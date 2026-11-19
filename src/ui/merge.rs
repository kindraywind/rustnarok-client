//! Item merge: a grid of item cells, one per stack that can merge, each with a check box. The checked stacks of one
//! item merge into the first of them.

use std::collections::HashMap;

use rustnarok_client::lua::ItemInfo;
use rustnarok_client::net::InventoryItem;
use rustnarok_client::text::Font;

use crate::render::Texture;
use crate::window::{self, BUTTON_SIZE, Chrome, Frame, TITLE_HEIGHT};
use crate::{Layer, contains, inventory, offset, screen, vend};

/// "Item Merge"; OK with fewer than two checked (2170); a second item checked (2172); the answers (2171, 2173).
pub const TITLE: usize = 2169;
const TOO_FEW: usize = 2170;
pub const MERGED: usize = 2171;
pub const ONE_KIND: usize = 2172;
pub const TOO_MANY: usize = 2173;
/// The success line's colour.
pub const MERGED_COLOR: [u8; 3] = [0xFF, 0xFF, 0x64];

/// 35 px cells from (10, 20): the window is 20 + 35·columns by 50 + 35·rows, 5…10 columns and 3…6 rows.
const PITCH: f32 = 35.0;
const CELLS_AT: [f32; 2] = [10.0, 20.0];
const CHROME: [f32; 2] = [20.0, 50.0];
const COLUMNS: (usize, usize) = (5, 10);
const ROWS: (usize, usize) = (3, 6);
/// In a cell: the icon, the point a press counts from (closer than HIT_RADIUS) and the check box.
const ICON_AT: [f32; 2] = [5.0, 2.0];
const ICON_MIDDLE: [f32; 2] = [15.0, 12.0];
const HIT_RADIUS: f32 = 10.0;
const CHECK_AT: [f32; 2] = [0.0, 20.0];
const CHECK: [f32; 2] = [10.0, 10.0];
const FOOTER: f32 = 29.0;
const BUTTON: [f32; 2] = [42.0, 20.0];
/// OK and Cancel from the bottom-right corner; the scroll bar from the right edge, under the title bar.
const OK_FROM: [f32; 2] = [110.0, 25.0];
const CANCEL_FROM: [f32; 2] = [60.0, 25.0];
const BAR_FROM: f32 = 15.0;
const BAR_SHORT: f32 = 38.0;

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Close,
    Title,
    Ok,
    Cancel,
    Grip,
    Bar,
    /// A stack, by its place in the list.
    Stack(usize),
    Window,
}

/// What a click asks for.
#[derive(Debug, PartialEq)]
pub enum Action {
    /// Merge these stacks (inventory indexes, the list's order).
    Merge(Vec<u16>),
    /// Say this line, send nothing.
    Refused(usize),
    Cancel,
}

pub struct Merge {
    pub frame: Frame,
    /// The stacks, in the order the server listed them.
    pub stacks: Vec<InventoryItem>,
    checked: Vec<bool>,
    columns: usize,
    rows: usize,
    /// The first row of stacks shown.
    first: usize,
    grip: window::Grip,
    thumb: window::Thumb,
}

impl Merge {
    /// 5 × 3 cells, its top-left corner at the screen's middle.
    pub fn new(stacks: Vec<InventoryItem>) -> Self {
        let [width, height] = screen();
        let mut frame = Frame::new([(width / 2.0).floor(), (height / 2.0).floor()], 0.0);
        frame.open = true;
        let checked = vec![false; stacks.len()];
        let mut merge =
            Self { frame, stacks, checked, columns: COLUMNS.0, rows: ROWS.0, first: 0, grip: window::Grip::default(), thumb: window::Thumb::default() };
        merge.frame.set_width(merge.size()[0]);
        merge
    }

    pub fn size(&self) -> [f32; 2] {
        [CHROME[0] + PITCH * self.columns as f32, CHROME[1] + PITCH * self.rows as f32]
    }

    /// The corner of the cell in column `column`, row `row` on screen of the grid.
    fn cell(column: usize, row: usize) -> [f32; 2] {
        [CELLS_AT[0] + PITCH * column as f32, CELLS_AT[1] + PITCH * row as f32]
    }

    /// The stacks on screen: each one's place in the list and its cell's corner.
    fn shown(&self) -> impl Iterator<Item = (usize, [f32; 2])> + '_ {
        (0..self.rows * self.columns)
            .map(|cell| ((self.first + cell / self.columns) * self.columns + cell % self.columns, Self::cell(cell % self.columns, cell / self.columns)))
            .filter(|(stack, _)| *stack < self.stacks.len())
    }

    /// How many rows the stacks take.
    fn needed(&self) -> usize {
        self.stacks.len().div_ceil(self.columns)
    }

    fn bar(&self) -> window::ScrollBar {
        let [width, height] = self.size();
        window::ScrollBar {
            at: self.frame.at([width - BAR_FROM, TITLE_HEIGHT]),
            height: height - BAR_SHORT,
            shown: self.rows,
            rows: self.needed(),
            first: self.first,
        }
    }

    fn buttons(&self) -> [[f32; 2]; 2] {
        let [width, height] = self.size();
        [[width - OK_FROM[0], height - OK_FROM[1]], [width - CANCEL_FROM[0], height - CANCEL_FROM[1]]]
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let size = self.size();
        let local = self.frame.local(cursor, size[1])?;
        let [ok, cancel] = self.buttons();
        Some(if contains(self.frame.close_button(), BUTTON_SIZE, local) {
            Hit::Close
        } else if local[1] < TITLE_HEIGHT {
            Hit::Title
        } else if contains(ok, BUTTON, local) {
            Hit::Ok
        } else if contains(cancel, BUTTON, local) {
            Hit::Cancel
        } else if self.bar().hit(cursor).is_some() {
            Hit::Bar
        } else if window::on_grip(size, local) {
            Hit::Grip
        } else if let Some(stack) = self.stack_at(local) {
            Hit::Stack(stack)
        } else {
            Hit::Window
        })
    }

    /// The stack a press at `local` is for: its icon's middle closer than 10 px, or its check box.
    fn stack_at(&self, local: [f32; 2]) -> Option<usize> {
        self.shown()
            .find(|(_, corner)| {
                let [x, y] = offset(*corner, ICON_MIDDLE);
                (local[0] - x).hypot(local[1] - y) < HIT_RADIUS || contains(offset(*corner, CHECK_AT), CHECK, local)
            })
            .map(|(stack, _)| stack)
    }

    /// Checks or clears a stack's box. With Ctrl every stack of its item follows (all checked, or all cleared when the
    /// press cleared one and the rest were checked) and every other item's box is cleared. Without Ctrl a stack of
    /// another item than the checked ones is refused (rathena would drop the request without an answer).
    pub fn toggle(&mut self, stack: usize, ctrl: bool) -> Result<(), usize> {
        let Some(id) = self.stacks.get(stack).map(|item| item.id) else { return Ok(()) };
        let mixed = self.stacks.iter().zip(&self.checked).any(|(item, &checked)| checked && item.id != id);
        if !ctrl && !self.checked[stack] && mixed {
            return Err(ONE_KIND);
        }
        self.checked[stack] = !self.checked[stack];
        if ctrl {
            let same = |other: usize| other != stack && self.stacks[other].id == id;
            let all = self.checked[stack] || (0..self.stacks.len()).any(|other| same(other) && !self.checked[other]);
            for (item, checked) in self.stacks.iter().zip(&mut self.checked) {
                *checked = all && item.id == id;
            }
        }
        Ok(())
    }

    /// The checked stacks' indexes, in the list's order (the first keeps everything).
    pub fn picked(&self) -> Vec<u16> {
        self.stacks.iter().zip(&self.checked).filter(|(_, checked)| **checked).map(|(item, _)| item.index).collect()
    }

    /// A press: the close button waits for the click; the title bar drags; the grip resizes; the scroll bar scrolls;
    /// a stack's box toggles (Err: the line refusing it).
    pub fn press(&mut self, cursor: [f32; 2], ctrl: bool) -> Result<(), usize> {
        match self.hit(cursor) {
            Some(Hit::Title) => self.frame.start_drag(cursor),
            Some(Hit::Grip) => self.grip.start(cursor, self.size()),
            Some(Hit::Bar) => {
                if let Some(first) = self.bar().press(cursor, &mut self.thumb) {
                    self.first = first;
                }
            }
            Some(Hit::Stack(stack)) => return self.toggle(stack, ctrl),
            _ => {}
        }
        Ok(())
    }

    /// The pointer moved: the dragged window, the held grip or the held thumb follows it.
    pub fn drag_to(&mut self, cursor: [f32; 2]) {
        self.frame.drag_to(cursor, self.size()[1]);
        if let Some(asked) = self.grip.asked(cursor) {
            self.resize(asked);
        }
        if let Some(first) = self.thumb.first(cursor[1]) {
            self.first = first;
        }
    }

    /// Takes the size asked for in whole cells, within 5…10 columns and 3…6 rows.
    fn resize(&mut self, [width, height]: [f32; 2]) {
        let least = |count: usize, chrome: f32| chrome + PITCH * count as f32;
        self.columns = COLUMNS.0 + window::steps(width, least(COLUMNS.0, CHROME[0]), PITCH, COLUMNS.1 - COLUMNS.0);
        self.rows = ROWS.0 + window::steps(height, least(ROWS.0, CHROME[1]), PITCH, ROWS.1 - ROWS.0);
        self.first = self.first.min(self.needed().saturating_sub(self.rows));
        self.frame.set_width(self.size()[0]);
    }

    /// The button went up: whether a drag, a resize or a thumb ended.
    pub fn drop_drag(&mut self) -> bool {
        self.frame.drop_drag() | self.grip.stop() | self.thumb.stop()
    }

    /// What a click on `hit` asks for.
    pub fn click(&self, hit: Hit) -> Option<Action> {
        match hit {
            Hit::Ok => {
                let picked = self.picked();
                Some(if picked.len() < 2 { Action::Refused(TOO_FEW) } else { Action::Merge(picked) })
            }
            Hit::Cancel | Hit::Close => Some(Action::Cancel),
            _ => None,
        }
    }

    /// The wheel, `notches` down when positive: one less row than shown a notch, three at most, while it scrolls.
    pub fn scroll_by(&mut self, notches: i32) {
        let most = self.needed().saturating_sub(self.rows) as i32;
        let step = (self.rows as i32 - 1).min(3);
        self.first = (self.first as i32 + notches * step).clamp(0, most) as usize;
    }

    /// The stacks' cells on screen (item id, box), named for scripted clicks.
    pub fn cell_boxes(&self) -> Vec<(u32, [f32; 4])> {
        self.shown().map(|(stack, corner)| (self.stacks[stack].id, self.on_screen(corner, [32.0, 32.0]))).collect()
    }

    /// The check boxes (`merge_box_<place in the list>`), OK, Cancel, the close button and the grip on screen.
    pub fn button_boxes(&self) -> Vec<(String, [f32; 4])> {
        let [ok, cancel] = self.buttons();
        let [width, height] = self.size();
        let mut boxes: Vec<_> = self.shown().map(|(stack, corner)| (format!("merge_box_{stack}"), self.on_screen(offset(corner, CHECK_AT), CHECK))).collect();
        boxes.extend(
            [("ok", ok, BUTTON), ("cancel", cancel, BUTTON), ("close", self.frame.close_button(), BUTTON_SIZE)]
                .map(|(name, at, size)| (name.to_owned(), self.on_screen(at, size))),
        );
        boxes.push(("merge_grip".to_owned(), self.on_screen([width - window::GRIP, height - window::GRIP], [window::GRIP; 2])));
        boxes
    }

    fn on_screen(&self, local: [f32; 2], [width, height]: [f32; 2]) -> [f32; 4] {
        let [x, y] = self.frame.at(local);
        [x, y, width, height]
    }

    /// For the test driver: columns, rows and the first row shown.
    pub fn grid(&self) -> [usize; 3] {
        [self.columns, self.rows, self.first]
    }

    pub fn icons(&self, items: &HashMap<u32, ItemInfo>) -> Vec<String> {
        vend::icons(items, self.stacks.iter().map(|item| (item.id, item.identified)))
    }

    /// The title bar with its base picture and close button, the white body, every cell, the stacks with their
    /// amounts and boxes, the scroll bar, the footer with OK and Cancel, the grip, and the name of the stack pointed at.
    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &self,
        layer: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a vend::Assets,
        icons: &'a HashMap<String, Option<Texture>>,
        items: &HashMap<u32, ItemInfo>,
        naming: inventory::Naming,
        messages: &[String],
        font: &Font,
        (cursor, held): ([f32; 2], bool),
    ) {
        let frame = &self.frame;
        let [width, height] = self.size();
        frame.draw_title_only(layer, chrome, messages.get(TITLE).map_or("", String::as_str));
        let base = chrome.button(0, false);
        layer.sprites.push((base, frame.at([3.0, 3.0]), base.size));
        let close = chrome.button(2, contains(frame.at(frame.close_button()), BUTTON_SIZE, cursor));
        layer.sprites.push((close, frame.at(frame.close_button()), close.size));
        layer.sprites.push((&chrome.white, frame.at([0.0, TITLE_HEIGHT]), [width, height - TITLE_HEIGHT]));
        for cell in 0..self.rows * self.columns {
            layer.sprites.push((&assets.cell, frame.at(Self::cell(cell % self.columns, cell / self.columns)), assets.cell.size));
        }
        let hit = self.hit(cursor);
        for (stack, corner) in self.shown() {
            let item = &self.stacks[stack];
            let icon_at = frame.at(offset(corner, ICON_AT));
            if let Some(Some(icon)) = items.get(&item.id).and_then(|info| icons.get(&inventory::icon_path(info, item.identified))) {
                layer.sprites.push((icon, icon_at, icon.size));
            }
            // The amount as the inventory draws it: black over a white outline, from the icon as its cell's from (4, 4).
            let amount = item.amount.to_string();
            let at = offset(icon_at, offset(inventory::amount_at(item.amount), [-4.0, -4.0]));
            for step in [[-1.0, 0.0], [1.0, 0.0], [0.0, -1.0], [0.0, 1.0]] {
                layer.texts.push((amount.clone(), offset(at, step), [255; 3]));
            }
            layer.texts.push((amount, at, [0; 3]));
            let check = &assets.check[usize::from(self.checked[stack])];
            layer.sprites.push((check, frame.at(offset(corner, CHECK_AT)), check.size));
        }
        self.bar().draw(layer, chrome);
        chrome.draw_footer(layer, frame.at([0.0, height]), width, FOOTER);
        let [ok, cancel] = self.buttons();
        for (target, place, images) in [(Hit::Ok, ok, &assets.ok), (Hit::Cancel, cancel, &assets.cancel)] {
            let look = usize::from(hit == Some(target)) + usize::from(hit == Some(target) && held);
            layer.sprites.push((&images[look], frame.at(place), BUTTON));
        }
        layer.sprites.push((&assets.resize, frame.at([width - window::GRIP, height - window::GRIP]), assets.resize.size));
        if let Some(Hit::Stack(stack)) = hit {
            let item = &self.stacks[stack];
            let name = inventory::full_name(items, naming.0, messages, naming.1, item, true);
            window::tooltip(layer, chrome, font, &inventory::hover_text(messages, &name, item), cursor, inventory::name_color(item, items));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stack(index: u16, id: u32) -> InventoryItem {
        InventoryItem { index, id, amount: 1, identified: true, ..Default::default() }
    }

    /// A window at (0, 0) with these stacks.
    fn merge(stacks: Vec<InventoryItem>) -> Merge {
        let mut merge = Merge::new(stacks);
        merge.frame.position = [0.0, 0.0];
        merge
    }

    #[test]
    fn opens_5_by_3_at_the_screen_middle_with_its_buttons() {
        let window = Merge::new(vec![stack(2, 501)]);
        assert_eq!((window.size(), window.frame.position), ([195.0, 155.0], [512.0, 384.0]));
        let window = merge(vec![stack(2, 501)]);
        assert_eq!([Merge::cell(0, 0), Merge::cell(1, 0), Merge::cell(0, 1)], [[10.0, 20.0], [45.0, 20.0], [10.0, 55.0]]);
        assert_eq!(window.buttons(), [[85.0, 130.0], [135.0, 130.0]]);
        assert_eq!(
            [window.hit([86.0, 131.0]), window.hit([176.0, 149.0]), window.hit([182.0, 4.0]), window.hit([170.0, 4.0])],
            [Some(Hit::Ok), Some(Hit::Cancel), Some(Hit::Close), Some(Hit::Title)]
        );
        assert_eq!((window.hit([190.0, 150.0]), window.hit([195.0, 100.0])), (Some(Hit::Grip), None));
        assert_eq!((window.click(Hit::Cancel), window.click(Hit::Close), window.click(Hit::Title)), (Some(Action::Cancel), Some(Action::Cancel), None));
    }

    #[test]
    fn the_grip_resizes_in_whole_cells() {
        let mut window = merge(vec![stack(2, 501)]);
        window.press([190.0, 150.0], false).unwrap();
        // Asking for 270 wide is 7 columns; never under 5 × 3 or over 10 × 6.
        window.drag_to([190.0 + 75.0, 150.0 + 40.0]);
        assert_eq!((window.columns, window.rows, window.size()), (7, 4, [265.0, 190.0]));
        window.drag_to([0.0, 0.0]);
        assert_eq!((window.columns, window.rows), (5, 3));
        window.drag_to([2000.0, 2000.0]);
        assert_eq!((window.columns, window.rows, window.size()), (10, 6, [370.0, 260.0]));
        assert!(window.drop_drag());
        // OK, Cancel and the close button follow the corner.
        assert_eq!(window.buttons(), [[260.0, 235.0], [310.0, 235.0]]);
        assert_eq!(window.hit([370.0 - 14.0, 4.0]), Some(Hit::Close));
    }

    #[test]
    fn a_press_near_an_icon_or_on_a_box_toggles_it() {
        let mut window = merge(vec![stack(2, 501), stack(5, 501)]);
        // The icon's middle in the first cell is (25, 32): 9 px off counts, 11 doesn't.
        assert_eq!((window.hit([34.0, 32.0]), window.hit([36.0, 32.0])), (Some(Hit::Stack(0)), Some(Hit::Window)));
        assert_eq!(window.hit([12.0, 42.0]), Some(Hit::Stack(0)));
        window.press([34.0, 32.0], false).unwrap();
        assert_eq!(window.picked(), [2]);
        window.press([12.0, 42.0], false).unwrap();
        assert_eq!(window.picked(), Vec::<u16>::new());
    }

    #[test]
    fn ctrl_checks_a_whole_kind_and_clears_the_others() {
        let mut window = merge(vec![stack(2, 501), stack(3, 502), stack(4, 501), stack(6, 502), stack(7, 501)]);
        window.toggle(1, false).unwrap();
        assert_eq!(window.toggle(0, false), Err(ONE_KIND));
        window.toggle(2, true).unwrap();
        assert_eq!(window.picked(), [2, 4, 7]);
        // Again on one of them: all of that item cleared.
        window.toggle(4, true).unwrap();
        assert_eq!(window.picked(), Vec::<u16>::new());
        // A press that clears one while another of the item is clear checks them all.
        window.toggle(0, false).unwrap();
        window.toggle(0, true).unwrap();
        assert_eq!(window.picked(), [2, 4, 7]);
    }

    #[test]
    fn ok_sends_the_checked_in_list_order_or_refuses_fewer_than_two() {
        let mut window = merge(vec![stack(9, 501), stack(4, 501), stack(6, 501)]);
        assert_eq!(window.click(Hit::Ok), Some(Action::Refused(TOO_FEW)));
        window.toggle(2, false).unwrap();
        assert_eq!(window.click(Hit::Ok), Some(Action::Refused(TOO_FEW)));
        window.toggle(0, false).unwrap();
        assert_eq!(window.click(Hit::Ok), Some(Action::Merge(vec![9, 6])));
    }

    #[test]
    fn scrolls_when_the_stacks_need_more_rows() {
        let mut window = merge((0..16).map(|index| stack(index, 501)).collect());
        // 16 stacks are 4 rows in 5 × 3: the bar at (180, 17), 13 × 117; the wheel moves two rows, stopping at 1.
        assert_eq!(window.bar().boxes().unwrap()[0], [180.0, 17.0, 13.0, 13.0]);
        window.scroll_by(1);
        assert_eq!(window.first, 1);
        assert_eq!(window.hit([25.0, 32.0]), Some(Hit::Stack(5)));
        window.scroll_by(-1);
        assert_eq!(window.first, 0);
        // An arrow moves one row; growing to four rows shows them all.
        window.press([185.0, 125.0], false).unwrap();
        assert_eq!(window.first, 1);
        window.resize([195.0, 190.0]);
        assert_eq!((window.first, window.bar().boxes()), (0, None));
        window.scroll_by(1);
        assert_eq!(window.first, 0);
    }
}
