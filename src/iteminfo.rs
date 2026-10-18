//! The item description window (right-click an item): the item's picture, its name, what the client says about it,
//! and the cards in its slots.

use std::collections::HashMap;

use rustnarok_client::grf::Grf;
use rustnarok_client::lua::ItemInfo;
use rustnarok_client::net::InventoryItem;
use rustnarok_client::text::Font;

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
/// The illustration button, on a card's info.
// ponytail: the real client's button image isn't known; a labelled chip stands in.
const VIEW: [f32; 2] = [8.0, SIZE[1] - 22.0];
const VIEW_SIZE: [f32; 2] = [64.0, 16.0];
const VIEW_LABEL: &str = "Illust";
/// The word over a compared item's info.
// ponytail: no msgstringtable id was given for this title; "Worn" stands in until one is confirmed.
const WORN_TITLE: &str = "Worn";
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
    /// A broken item's line ("This item has been damaged."), shown first in red.
    pub damaged: Option<String>,
    /// A rental's end, Unix time in seconds (0: not a rental).
    pub expires: u32,
    /// A card: its info offers the picture button.
    pub card: bool,
}

impl Shown {
    pub fn of(item: &InventoryItem, damaged: Option<String>) -> Self {
        Self {
            item: item.id,
            identified: item.identified,
            refine: item.refine,
            cards: item.cards,
            damaged,
            expires: item.expires,
            card: item.kind == rustnarok_client::net::ITEM_CARD,
        }
    }
}

/// "Time Left: %d hours %d minutes".
const TIME_LEFT: usize = 2435;

/// A rental's time left, in red, from its end and the time now (Unix seconds); None for an item that isn't one.
// ponytail: the msgstringtable's end-date line (1255, local time) looks like the official one; std can't do local time,
// so this counts down instead, assuming the server's clock and ours agree.
pub fn rental_line(template: &str, expires: u32, now: u64) -> Option<String> {
    let left = u64::from(expires).saturating_sub(now);
    (expires > 0).then(|| format!("^ff0000{}^000000", rustnarok_client::messages::fill(template, &[&(left / 3600), &(left % 3600 / 60)])))
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Close,
    /// The top, to drag by.
    Grip,
    /// The illustration button.
    View,
    /// A card in a slot, by item id.
    Card(u32),
    Window,
}

pub struct ItemWindow {
    pub shown: Shown,
    pub position: [f32; 2],
    /// Lines scrolled past.
    scroll: usize,
    /// Where the pointer grabbed the window, while dragging.
    drag: Option<[f32; 2]>,
    /// Compared equipment: the title says it's what's worn.
    pub worn: bool,
}

/// The picture of an item (data\texture\유저인터페이스\collection\).
pub fn picture_path(info: &ItemInfo, identified: bool) -> String {
    format!("{UI}collection\\{}.bmp", info.resource(identified))
}

impl ItemWindow {
    pub fn new(shown: Shown) -> Self {
        Self { shown, position: [300.0, 200.0], scroll: 0, drag: None, worn: false }
    }

    /// Beside a hovered cell, the `index`th compared item (they stack downward).
    pub fn place_beside(&mut self, cell: [f32; 4], index: usize, items: &HashMap<u32, ItemInfo>) {
        let height = self.height(items);
        // Past the hovered item's name tooltip, which sits just right of the cell.
        let x = (cell[0] + cell[2] + 150.0).min(crate::screen()[0] - SIZE[0]).max(0.0);
        let y = (cell[1] + index as f32 * (height + 6.0)).min(crate::screen()[1] - height).max(0.0);
        self.position = [x, y];
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
        } else if self.shown.card && contains(VIEW, VIEW_SIZE, local) {
            Hit::View
        } else if let Some(card) = self.card_at(local, items) {
            Hit::Card(card)
        } else if local[1] < TEXT[1] - 6.0 {
            Hit::Grip
        } else {
            Hit::Window
        })
    }

    /// A filled card slot under the pointer, by the card's item id.
    fn card_at(&self, local: [f32; 2], items: &HashMap<u32, ItemInfo>) -> Option<u32> {
        let top = SIZE[1] + CARDS_GAP + 4.0;
        self.card_ids(items).find_map(|(slot, card)| {
            let corner = [6.0 + slot as f32 * (SLOT + 4.0), top];
            contains(corner, [SLOT, SLOT], local).then_some(card)
        })
    }

    /// Filled card slots. 0x00FE and 0x00FF in the first slot mark a brewed or forged item: the other slots hold the
    /// maker and the element, not card ids.
    fn card_ids(&self, items: &HashMap<u32, ItemInfo>) -> impl Iterator<Item = (usize, u32)> {
        let forged = matches!(self.shown.cards[0], 0x00FE | 0x00FF);
        let slots = self.slots(items);
        (0..slots).filter_map(move |slot| {
            let card = self.shown.cards[slot];
            (!forged && card > 0xFF).then_some((slot, card))
        })
    }

    /// The illustration button and the filled card slots, for the test driver.
    pub fn button_boxes(&self, items: &HashMap<u32, ItemInfo>) -> Vec<(String, [f32; 4])> {
        let mut boxes = Vec::new();
        if self.shown.card {
            let corner = offset(self.position, VIEW);
            boxes.push(("card_picture".into(), [corner[0], corner[1], VIEW_SIZE[0], VIEW_SIZE[1]]));
        }
        let top = SIZE[1] + CARDS_GAP + 4.0;
        for (slot, card) in self.card_ids(items) {
            let corner = offset(self.position, [6.0 + slot as f32 * (SLOT + 4.0), top]);
            boxes.push((format!("card:{card}"), [corner[0], corner[1], SLOT, SLOT]));
        }
        boxes
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
    /// The description's lines: a rental's time left (by `now`, Unix seconds) and a broken item's line first.
    pub fn lines(&self, items: &HashMap<u32, ItemInfo>, messages: &[String], now: u64, wrap: &dyn Fn(&str, f32) -> ColoredLines) -> ColoredLines {
        let Some(info) = items.get(&self.shown.item) else { return Vec::new() };
        let rental = rental_line(messages.get(TIME_LEFT).map_or("", String::as_str), self.shown.expires, now);
        let damaged = self.shown.damaged.iter().map(|line| format!("^ff0000{line}^000000"));
        let description = info.description(self.shown.identified).iter().cloned();
        rental.into_iter().chain(damaged).chain(description).flat_map(|line| wrap(&line, TEXT_WIDTH)).collect()
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
        let name = if self.worn { format!("{WORN_TITLE}: {name}") } else { name };
        layer.texts.push((crate::window::fit(font, &name, SIZE[0] - TITLE[0] - 18.0), at(TITLE), [0, 0, 0]));
        if self.shown.card {
            layer.sprites.push((&assets.border, at(VIEW), VIEW_SIZE));
            layer.sprites.push((&chrome.white, at(offset(VIEW, [1.0, 1.0])), [VIEW_SIZE[0] - 2.0, VIEW_SIZE[1] - 2.0]));
            layer.texts.push((VIEW_LABEL.into(), at(offset(VIEW, [4.0, 1.0])), [0, 0, 0]));
        }
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

/// Picture names from the card-picture table. A line is `id#name#`, and some lines carry a second name.
pub struct CardPictures {
    names: HashMap<u32, Vec<String>>,
}

const CARD_ILLUST: &str = "data\\num2cardillustnametable.txt";
const CARD_ILLUST_ID: &str = "data\\idnum2cardillustnametable.txt";

impl CardPictures {
    pub fn load(grf: &Grf) -> Self {
        let text = |path: &str| grf.read(path).map(|bytes| encoding_rs::EUC_KR.decode_without_bom_handling(&bytes).0.into_owned()).unwrap_or_default();
        // ponytail: both file names show up in client data. The picture uses the first name on the line. Keeping the
        // file and the column the player's data.grf actually has would replace reading both.
        let mut pictures = Self::parse(&text(CARD_ILLUST));
        for (id, names) in Self::parse(&text(CARD_ILLUST_ID)).names {
            pictures.names.entry(id).or_insert(names);
        }
        pictures
    }

    pub fn parse(text: &str) -> Self {
        let mut names = HashMap::new();
        for line in text.lines() {
            let mut parts = line.split('#');
            let Some(id) = parts.next().and_then(|id| id.trim().parse::<u32>().ok()) else { continue };
            let found: Vec<String> = parts.map(str::trim).filter(|name| !name.is_empty()).map(str::to_owned).collect();
            if !found.is_empty() {
                names.insert(id, found);
            }
        }
        Self { names }
    }

    /// Every name on the card's line. The picture uses the first.
    pub fn names(&self, id: u32) -> &[String] {
        self.names.get(&id).map(Vec::as_slice).unwrap_or(&[])
    }

    /// The picture's name: the first one on the line.
    pub fn name(&self, id: u32) -> Option<&str> {
        self.names(id).first().map(String::as_str)
    }
}

/// `cardbmp\<name>.bmp` under the interface textures.
pub fn cardbmp_path(name: &str) -> String {
    format!("{UI}cardbmp\\{name}.bmp")
}

/// The card's picture, in its own small window.
pub struct CardView {
    pub path: String,
    pub position: [f32; 2],
    drag: Option<[f32; 2]>,
}

// ponytail: card bmps are about 300 by 400; this window shows them fitted. The real window's size would replace it.
const CARD_VIEW: [f32; 2] = [200.0, 270.0];
const CARD_PICTURE: [f32; 2] = [10.0, 22.0];
const CARD_CLOSE: [f32; 2] = [CARD_VIEW[0] - 14.0, 3.0];

#[derive(Debug, PartialEq)]
pub enum ViewHit {
    Close,
    Grip,
    Window,
}

impl CardView {
    pub fn new(path: String) -> Self {
        Self { path, position: [520.0, 160.0], drag: None }
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<ViewHit> {
        let local = offset(cursor, self.position.map(|value| -value));
        if !contains([0.0; 2], CARD_VIEW, local) {
            return None;
        }
        Some(if contains(CARD_CLOSE, [11.0, 11.0], local) {
            ViewHit::Close
        } else if local[1] < 18.0 {
            ViewHit::Grip
        } else {
            ViewHit::Window
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

    pub fn draw<'a>(&self, layer: &mut Layer<'a>, chrome: &'a Chrome, assets: &'a Assets, images: &'a HashMap<String, Option<Texture>>, pointer: [f32; 2]) {
        let at = |local: [f32; 2]| offset(self.position, local);
        layer.sprites.push((&assets.border, at([0.0, 0.0]), CARD_VIEW));
        layer.sprites.push((&chrome.white, at([1.0, 1.0]), [CARD_VIEW[0] - 2.0, CARD_VIEW[1] - 2.0]));
        if let Some(Some(picture)) = images.get(&self.path) {
            layer.sprites.push((picture, at(CARD_PICTURE), [CARD_VIEW[0] - 20.0, CARD_VIEW[1] - 32.0]));
        }
        let close = &assets.close[usize::from(self.hit(pointer) == Some(ViewHit::Close))];
        layer.sprites.push((close, at(CARD_CLOSE), close.size));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_a_rental_down() {
        let template = "Time Left: %d hours %d minutes";
        assert_eq!(rental_line(template, 0, 1_000).as_deref(), None);
        assert_eq!(rental_line(template, 1_000 + 7_500, 1_000).as_deref(), Some("^ff0000Time Left: 2 hours 5 minutes^000000"));
        assert_eq!(rental_line(template, 1_000 + 59, 1_000).as_deref(), Some("^ff0000Time Left: 0 hours 0 minutes^000000"));
        // Past its end: nothing left.
        assert_eq!(rental_line(template, 1_000, 5_000).as_deref(), Some("^ff0000Time Left: 0 hours 0 minutes^000000"));
    }

    #[test]
    fn grows_a_card_row_for_slotted_items() {
        let knife = ItemInfo { name: "Knife".into(), resource: "나이프".into(), slots: 3, ..Default::default() };
        let items = HashMap::from([(1201, knife)]);
        let mut window = ItemWindow::new(Shown { item: 1201, identified: true, refine: 0, cards: [0; 4], damaged: None, expires: 0, card: false });
        assert_eq!(window.slots(&items), 3);
        assert_eq!(window.height(&items), SIZE[1] + CARDS_GAP + CARD_ROW);
        window.shown.identified = false;
        assert_eq!(window.slots(&items), 0);
        assert_eq!(window.hit(offset(window.position, [CLOSE[0] + 2.0, CLOSE[1] + 2.0]), &items), Some(Hit::Close));
        window.shown.identified = true;
        window.shown.cards[0] = 4001;
        let slot = offset(window.position, [8.0, SIZE[1] + CARDS_GAP + 6.0]);
        assert_eq!(window.hit(slot, &items), Some(Hit::Card(4001)));
        // A forged weapon's later slots are the element and the maker, not cards.
        window.shown.cards = [0x00FF, 5 << 8 | 3, 4, 2];
        assert_eq!(window.hit(slot, &items), Some(Hit::Window));
        window.shown.card = true;
        assert_eq!(window.hit(offset(window.position, [VIEW[0] + 2.0, VIEW[1] + 2.0]), &items), Some(Hit::View));
        window.scroll_by(3, 7);
        assert_eq!(window.scroll, 2);
    }

    #[test]
    fn names_a_cards_picture_from_a_small_table() {
        let table = CardPictures::parse("4001#poring_card#\n4002#fabre_card#fabre#\n#nope#\n");
        assert_eq!(table.name(4001), Some("poring_card"));
        assert_eq!(table.names(4002), ["fabre_card".to_string(), "fabre".to_string()]);
        assert_eq!(table.name(4002), Some("fabre_card"));
        assert!(table.name(9).is_none());
        assert_eq!(cardbmp_path("poring_card"), format!("{UI}cardbmp\\poring_card.bmp"));
    }
}
