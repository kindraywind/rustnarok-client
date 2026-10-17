//! The item description window (right-click an item): the item's picture, its name, what the client says about it,
//! and the cards in its slots.

use std::collections::HashMap;

use roclient::grf::Grf;
use roclient::lua::ItemInfo;
use roclient::text::Font;

use crate::inventory;
use crate::render::{Gpu, Texture};
use crate::window::Chrome;
use crate::{Layer, UI, contains, offset};

// ponytail: places over basic_interface\collection_bg.bmp (picture at 10,11; text from 100,35 in 18px lines);
// calibrate against the real client.
const SIZE: [f32; 2] = [280.0, 140.0];
const PICTURE: [f32; 2] = [10.0, 11.0];
const TITLE: [f32; 2] = [90.0, 9.0];
const TEXT: [f32; 2] = [100.0, 35.0];
const TEXT_WIDTH: f32 = 150.0;
const LINE: f32 = 18.0;
const LINES: usize = 5;
const CLOSE: [f32; 2] = [SIZE[0] - 14.0, 3.0];
const CARDS_GAP: f32 = 3.0;
const CARD_ROW: f32 = 34.0;
const SLOT: f32 = 26.0;
const BORDER: [u8; 4] = [0xC0, 0xC0, 0xC0, 0xFF];
const SLOT_BORDER: [u8; 4] = [0xC1, 0xC6, 0xC2, 0xFF];

pub struct Assets {
    background: Texture,
    border: Texture,
    slot: Texture,
    close: [Texture; 2],
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |name: &str| gpu.load_texture(grf, &format!("{UI}basic_interface\\{name}.bmp"), false);
        Self {
            background: image("collection_bg"),
            border: gpu.solid(BORDER),
            slot: gpu.solid(SLOT_BORDER),
            close: [image("sys_close_off"), image("sys_close_on")],
        }
    }
}

/// Lines of text, each a run of colored pieces.
pub type ColoredLines = Vec<Vec<([u8; 3], String)>>;

/// The item shown.
#[derive(Debug, Clone, PartialEq)]
pub struct Shown {
    pub item: u32,
    pub identified: bool,
    pub refine: u8,
    pub cards: [u32; 4],
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Close,
    /// The top, to drag by.
    Grip,
    Window,
}

pub struct ItemWindow {
    pub shown: Shown,
    pub position: [f32; 2],
    /// Lines scrolled past.
    scroll: usize,
    /// Where the pointer grabbed the window, while dragging.
    drag: Option<[f32; 2]>,
}

/// The picture of an item (data\texture\유저인터페이스\collection\).
pub fn picture_path(info: &ItemInfo, identified: bool) -> String {
    format!("{UI}collection\\{}.bmp", info.resource(identified))
}

impl ItemWindow {
    pub fn new(shown: Shown) -> Self {
        Self { shown, position: [300.0, 200.0], scroll: 0, drag: None }
    }

    /// How many card slots the item has.
    fn slots(&self, items: &HashMap<u32, ItemInfo>) -> usize {
        items.get(&self.shown.item).filter(|_| self.shown.identified).map_or(0, |info| usize::from(info.slots.min(4)))
    }

    fn height(&self, items: &HashMap<u32, ItemInfo>) -> f32 {
        SIZE[1] + if self.slots(items) > 0 { CARDS_GAP + CARD_ROW } else { 0.0 }
    }

    pub fn hit(&self, cursor: [f32; 2], items: &HashMap<u32, ItemInfo>) -> Option<Hit> {
        let local = offset(cursor, self.position.map(|value| -value));
        if !contains([0.0; 2], [SIZE[0], self.height(items)], local) {
            return None;
        }
        Some(if contains(CLOSE, [11.0, 11.0], local) {
            Hit::Close
        } else if local[1] < TEXT[1] - 6.0 {
            Hit::Grip
        } else {
            Hit::Window
        })
    }

    pub fn start_drag(&mut self, cursor: [f32; 2]) {
        self.drag = Some(offset(cursor, self.position.map(|value| -value)));
    }

    pub fn drag_to(&mut self, cursor: [f32; 2]) {
        if let Some(grab) = self.drag {
            self.position = offset(cursor, grab.map(|value| -value));
        }
    }

    pub fn drop_drag(&mut self) -> bool {
        self.drag.take().is_some()
    }

    pub fn scroll_by(&mut self, rows: i32, lines: usize) {
        self.scroll = (self.scroll as i32 + rows).clamp(0, lines.saturating_sub(LINES) as i32) as usize;
    }

    /// The description, wrapped to the text column (colors kept per piece).
    pub fn lines(&self, items: &HashMap<u32, ItemInfo>, wrap: &dyn Fn(&str, f32) -> ColoredLines) -> ColoredLines {
        let Some(info) = items.get(&self.shown.item) else { return Vec::new() };
        info.description(self.shown.identified).iter().flat_map(|line| wrap(line, TEXT_WIDTH)).collect()
    }

    /// The pictures to load: the item's, and its cards' icons.
    pub fn images(&self, items: &HashMap<u32, ItemInfo>) -> Vec<String> {
        let mut paths: Vec<String> = items.get(&self.shown.item).map(|info| picture_path(info, self.shown.identified)).into_iter().collect();
        paths.extend(self.shown.cards.iter().filter(|&&card| card > 0).filter_map(|card| items.get(card).map(|info| inventory::icon_path(info, true))));
        paths
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &self,
        layer: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        images: &'a HashMap<String, Option<Texture>>,
        items: &HashMap<u32, ItemInfo>,
        font: &Font,
        lines: &[Vec<([u8; 3], String)>],
        pointer: [f32; 2],
    ) {
        let at = |local: [f32; 2]| offset(self.position, local);
        // A white box inside a grey line, the background with its picture frame on top.
        layer.sprites.push((&assets.border, at([0.0, 0.0]), SIZE));
        layer.sprites.push((&chrome.white, at([1.0, 1.0]), [SIZE[0] - 2.0, SIZE[1] - 2.0]));
        layer.sprites.push((&assets.background, at([0.0, 0.0]), assets.background.size));
        let info = items.get(&self.shown.item);
        if let Some(Some(picture)) = info.and_then(|info| images.get(&picture_path(info, self.shown.identified))) {
            layer.sprites.push((picture, at(PICTURE), picture.size));
        }
        let name = inventory::item_name(items, self.shown.item, self.shown.identified, self.shown.refine);
        layer.texts.push((crate::window::fit(font, &name, SIZE[0] - TITLE[0] - 18.0), at(TITLE), [0, 0, 0]));
        let close = &assets.close[usize::from(self.hit(pointer, items) == Some(Hit::Close))];
        layer.sprites.push((close, at(CLOSE), close.size));
        for (row, pieces) in lines.iter().skip(self.scroll).take(LINES).enumerate() {
            let mut x = TEXT[0];
            for (color, piece) in pieces {
                layer.texts.push((piece.clone(), at([x, TEXT[1] + row as f32 * LINE]), *color));
                x += font.width(piece);
            }
        }

        // The cards, one slot each.
        let slots = self.slots(items);
        if slots > 0 {
            let top = SIZE[1] + CARDS_GAP;
            layer.sprites.push((&assets.border, at([0.0, top]), [SIZE[0], CARD_ROW]));
            layer.sprites.push((&chrome.white, at([1.0, top + 1.0]), [SIZE[0] - 2.0, CARD_ROW - 2.0]));
            for slot in 0..slots {
                let corner = at([6.0 + slot as f32 * (SLOT + 4.0), top + 4.0]);
                layer.sprites.push((&assets.slot, corner, [SLOT, SLOT]));
                layer.sprites.push((&chrome.white, offset(corner, [1.0, 1.0]), [SLOT - 2.0, SLOT - 2.0]));
                let card = self.shown.cards[slot];
                if let Some(Some(icon)) = items.get(&card).and_then(|info| images.get(&inventory::icon_path(info, true))) {
                    layer.sprites.push((icon, offset(corner, [1.0, 1.0]), icon.size));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grows_a_card_row_for_slotted_items() {
        let knife = ItemInfo { name: "Knife".into(), resource: "나이프".into(), slots: 3, ..Default::default() };
        let items = HashMap::from([(1201, knife)]);
        let mut window = ItemWindow::new(Shown { item: 1201, identified: true, refine: 0, cards: [0; 4] });
        assert_eq!(window.slots(&items), 3);
        assert_eq!(window.height(&items), SIZE[1] + CARDS_GAP + CARD_ROW);
        window.shown.identified = false;
        assert_eq!(window.slots(&items), 0);
        assert_eq!(window.hit(offset(window.position, [CLOSE[0] + 2.0, CLOSE[1] + 2.0]), &items), Some(Hit::Close));
        window.scroll_by(3, 7);
        assert_eq!(window.scroll, 2);
    }
}
