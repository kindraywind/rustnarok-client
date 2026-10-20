//! An NPC's choice menu (ZC_MENU_LIST), and the Warp Portal and Teleport list drawn in the same box: 280 wide in the
//! sys box frame with no title bar, 4 to 15 rows of 16 px, OK and Cancel at its bottom right.

use std::time::{Duration, Instant};

use rustnarok_client::grf::Grf;
use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};
use crate::window::{Chrome, Frame};
use crate::{Layer, UI, contains, pickup};

pub const WIDTH: f32 = 280.0;
/// Where it first opens, at every screen size (20 px under the say window's first place); then where it was left.
pub const DEFAULT_POSITION: [f32; 2] = [200.0, 300.0];
/// Up to this many items the window is its smallest; each item more adds a row up to MAX_ROWS, then the list scrolls.
const MIN_ROWS: usize = 4;
const MAX_ROWS: usize = 15;
const MIN_HEIGHT: f32 = 120.0;
const ROW: f32 = 16.0;
/// The selected row's band, across the list at the top of its row.
const BAND: f32 = 15.0;
/// The list: at (12, 26), the window's size less 24 across and 55 down.
const LIST_AT: [f32; 2] = [12.0, 26.0];
const LIST_LESS: [f32; 2] = [24.0, 55.0];
const BAND_FILL: [u8; 4] = [0xF0, 0xF0, 0xF0, 0xFF];
/// The text's left inside the list.
const TEXT_X: f32 = 3.0;
/// OK and Cancel, this far left of the window's right edge and up from its bottom.
const OK_LEFT: f32 = 92.0;
const CANCEL_LEFT: f32 = 46.0;
const BUTTONS_UP: f32 = 24.0;
const BUTTON: [f32; 2] = [42.0, 20.0];
/// The wheel scrolls this many rows a notch.
const WHEEL_ROWS: i64 = 3;
/// A second press on the same row this soon, the pointer not moved, picks it.
const DOUBLE_CLICK: Duration = Duration::from_millis(500);

/// The window's place, kept with the other windows.
pub fn frame() -> Frame {
    let mut frame = Frame::new(DEFAULT_POSITION, WIDTH);
    frame.open = true;
    frame
}

/// How many rows show for `count` items.
fn rows(count: usize) -> usize {
    count.clamp(MIN_ROWS, MAX_ROWS)
}

/// The window's size for `count` items.
pub fn size(count: usize) -> [f32; 2] {
    [WIDTH, MIN_HEIGHT + (rows(count) - MIN_ROWS) as f32 * ROW]
}

/// The list's box inside the window.
fn list_box(count: usize) -> [f32; 4] {
    let [width, height] = size(count);
    [LIST_AT[0], LIST_AT[1], width - LIST_LESS[0], height - LIST_LESS[1]]
}

/// Where OK and Cancel go inside the window.
pub fn buttons(count: usize) -> [[f32; 2]; 2] {
    let [width, height] = size(count);
    [[width - OK_LEFT, height - BUTTONS_UP], [width - CANCEL_LEFT, height - BUTTONS_UP]]
}

/// The list's fill, pixel (14, 10) of colorchip.bmp (white without it), and the selected row's band, a grey on it. The
/// game has the two the other way round.
pub struct Assets {
    list: Texture,
    band: Texture,
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let chip = crate::render::load_image(grf, &format!("{UI}colorchip.bmp")).ok().and_then(|image| image.get_pixel_checked(14, 10).map(|pixel| pixel.0));
        let [red, green, blue, _] = chip.unwrap_or([255; 4]);
        Self { list: gpu.solid([red, green, blue, 255]), band: gpu.solid(BAND_FILL) }
    }
}

/// The items, the selected one, the first row shown, and the last press on a row (when, where in the window, which).
pub struct Menu {
    pub items: Vec<String>,
    pub selected: usize,
    pub first: usize,
    last_press: Option<(Instant, [f32; 2], usize)>,
}

impl Menu {
    /// A new menu has its first item selected.
    pub fn new(items: Vec<String>) -> Self {
        Self { items, selected: 0, first: 0, last_press: None }
    }

    pub fn size(&self) -> [f32; 2] {
        size(self.items.len())
    }

    /// Up or Down: one row, stopping at the ends, with the view following.
    pub fn step(&mut self, down: bool) {
        let last = self.items.len().saturating_sub(1);
        self.selected = if down { (self.selected + 1).min(last) } else { self.selected.saturating_sub(1) };
        let shown = rows(self.items.len());
        if self.selected < self.first {
            self.first = self.selected;
        } else if self.selected >= self.first + shown {
            self.first = self.selected + 1 - shown;
        }
    }

    /// The wheel, `notches` down when positive: 3 rows a notch while there are more items than rows.
    pub fn wheel(&mut self, notches: i32) {
        let last = self.items.len().saturating_sub(rows(self.items.len()));
        self.first = (self.first as i64 + notches as i64 * WHEEL_ROWS).clamp(0, last as i64) as usize;
    }

    /// The item under `local`, a point inside the window.
    pub fn row_at(&self, local: [f32; 2]) -> Option<usize> {
        let [x, y, width, height] = list_box(self.items.len());
        if !contains([x, y], [width, height], local) {
            return None;
        }
        let row = ((local[1] - y) / ROW) as usize;
        let index = self.first + row;
        (row < rows(self.items.len()) && index < self.items.len()).then_some(index)
    }

    /// Whether a press at `local` drags the window: anywhere but the list and the buttons.
    pub fn grips(&self, local: [f32; 2]) -> bool {
        let [x, y, width, height] = list_box(self.items.len());
        !contains([x, y], [width, height], local) && !buttons(self.items.len()).iter().any(|&place| contains(place, BUTTON, local))
    }

    /// A press at `local`: the row under it is selected. Returns true for a double click, the second press on that
    /// row within 500 ms with the pointer where it was.
    pub fn press(&mut self, local: [f32; 2], now: Instant) -> bool {
        let Some(index) = self.row_at(local) else {
            self.last_press = None;
            return false;
        };
        let double = self.last_press.is_some_and(|(at, from, row)| row == index && from == local && now.saturating_duration_since(at) < DOUBLE_CLICK);
        self.selected = index;
        self.last_press = if double { None } else { Some((now, local, index)) };
        double
    }

    /// The frame, the list, the selected row's band and the items at `corner`; the buttons are the caller's. A
    /// `^RRGGBB` in one item colours the rows after it too; a long item is cut at the list's edge.
    pub fn draw<'a>(&self, layer: &mut Layer<'a>, chrome: &'a Chrome, frame: &'a pickup::Assets, assets: &'a Assets, corner: [f32; 2], font: &Font) {
        let [width, height] = self.size();
        pickup::draw_sysbox(layer, chrome, frame, [corner[0], corner[1], width, height]);
        let [x, y, list_width, list_height] = list_box(self.items.len());
        let [x, y] = [corner[0] + x, corner[1] + y];
        layer.sprites.push((&assets.list, [x, y], [list_width, list_height]));
        for (index, line) in item_lines(&self.items, font, list_width - TEXT_X).into_iter().enumerate() {
            let Some(row) = index.checked_sub(self.first).filter(|&row| row < rows(self.items.len())) else { continue };
            let top = y + row as f32 * ROW;
            if index == self.selected {
                layer.sprites.push((&assets.band, [x, top], [list_width, BAND]));
            }
            let mut left = x + TEXT_X;
            for (color, piece) in line {
                let width = font.width(&piece);
                layer.texts.push((piece, [left, top], color));
                left += width;
            }
        }
    }
}

/// Each item's text as far as `width` goes, in colour pieces: a `^RRGGBB` in one item colours the items after it too.
fn item_lines(items: &[String], font: &Font, width: f32) -> crate::iteminfo::ColoredLines {
    let mut color = [0, 0, 0];
    items.iter().map(|item| crate::wrap_plain(font, item, width, 0.0, &mut color, true).swap_remove(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grows_a_row_an_item_from_the_fifth_to_the_fifteenth() {
        assert_eq!((size(0), size(4), buttons(4)), ([280.0, 120.0], [280.0, 120.0], [[188.0, 96.0], [234.0, 96.0]]));
        assert_eq!((size(6), buttons(6)[0]), ([280.0, 152.0], [188.0, 128.0]));
        assert_eq!((size(15), size(20)), ([280.0, 296.0], [280.0, 296.0]));
        assert_eq!(list_box(6), [12.0, 26.0, 256.0, 97.0]);
    }

    #[test]
    fn finds_the_row_under_the_pointer_when_scrolled() {
        let mut menu = Menu::new((1..=20).map(|item| item.to_string()).collect());
        assert_eq!(menu.row_at([20.0, 27.0]), Some(0));
        menu.wheel(1);
        assert_eq!((menu.first, menu.row_at([20.0, 27.0 + 16.0])), (3, Some(4)));
        // Down to the last five: 20 items in 15 rows.
        menu.wheel(9);
        assert_eq!((menu.first, menu.row_at([20.0, 27.0 + 14.0 * 16.0])), (5, Some(19)));
        assert_eq!(menu.row_at([5.0, 27.0]), None);
        // Fewer items than rows: the empty rows pick nothing.
        let menu = Menu::new(vec!["Stay".into(), "Leave".into()]);
        assert_eq!((menu.row_at([20.0, 27.0 + 16.0]), menu.row_at([20.0, 27.0 + 32.0])), (Some(1), None));
    }

    #[test]
    fn steps_to_the_ends_and_keeps_the_selection_shown() {
        let mut menu = Menu::new((1..=20).map(|item| item.to_string()).collect());
        menu.step(false);
        assert_eq!((menu.selected, menu.first), (0, 0));
        for _ in 0..15 {
            menu.step(true);
        }
        assert_eq!((menu.selected, menu.first), (15, 1));
        for _ in 0..10 {
            menu.step(true);
        }
        assert_eq!((menu.selected, menu.first), (19, 5));
        menu.first = 0;
        menu.step(false);
        assert_eq!((menu.selected, menu.first), (18, 4));
    }

    #[test]
    fn carries_a_colour_into_the_next_rows() {
        let font = Font::load();
        let items = ["^FF0000Red".to_owned(), "Still red".to_owned(), "^000000Black".to_owned()];
        let lines = item_lines(&items, &font, 253.0);
        assert_eq!(lines[1], [([0xFF, 0, 0], "Still red".to_owned())]);
        assert_eq!(lines[2], [([0, 0, 0], "Black".to_owned())]);
        // Cut at the list's edge.
        let long = ["W".repeat(80)];
        assert!(font.width(&item_lines(&long, &font, 253.0)[0][0].1) <= 253.0);
    }

    #[test]
    fn picks_on_a_double_click_only() {
        let start = Instant::now();
        let mut menu = Menu::new(vec!["Stay".into(), "Leave".into()]);
        let leave = [20.0, 27.0 + 16.0];
        assert!(!menu.press(leave, start));
        assert_eq!(menu.selected, 1);
        // Too late, then moved, then a real double click.
        assert!(!menu.press(leave, start + Duration::from_millis(600)));
        assert!(!menu.press([21.0, 27.0 + 16.0], start + Duration::from_millis(700)));
        assert!(menu.press([21.0, 27.0 + 16.0], start + Duration::from_millis(900)));
    }
}
