//! Dealing with another player: the trade window (yours on the left, theirs on the right, zeny under each), and the
//! question when someone asks for a deal.

use std::collections::HashMap;

use rustnarok_client::grf::Grf;
use rustnarok_client::lua::ItemInfo;
use rustnarok_client::net::InventoryItem;
use rustnarok_client::text::Font;

use crate::inventory::{self, UNIDENTIFIED};
use crate::render::{Gpu, Texture};
use crate::shop::group;
use crate::window::{self, Chrome, Frame, TITLE_HEIGHT};
use crate::{Layer, UI, contains, offset};

// ponytail: places over exchange_bg2.bmp; calibrate against the real client.
const WIDTH: f32 = 560.0;
const HEIGHT: f32 = TITLE_HEIGHT + 360.0;
/// The two item boxes: yours, theirs.
const BOXES: [[f32; 2]; 2] = [[2.0, TITLE_HEIGHT + 3.0], [WIDTH - 3.0 - 275.0, TITLE_HEIGHT + 3.0]];
const BOX_SIZE: [f32; 2] = [275.0, 305.0];
const ROWS: usize = 10;
const ROW: f32 = 30.0;
/// The zeny fields under each box.
const ZENY: [[f32; 2]; 2] = [[160.0, TITLE_HEIGHT + 311.0], [440.0, TITLE_HEIGHT + 311.0]];
const ZENY_SIZE: [f32; 2] = [74.0, 17.0];
const BUTTON: [f32; 2] = [42.0, 20.0];
const BUTTON_TOP: f32 = HEIGHT - 4.0 - BUTTON[1];
const OK: [f32; 2] = [5.0, BUTTON_TOP];
const EXCHANGE: [f32; 2] = [260.0, BUTTON_TOP];
const CANCEL: [f32; 2] = [510.0, BUTTON_TOP];
/// The title bar's two captions: "Trade : ", then the partner.
const CAPTION: [f32; 2] = [12.0, 1.0];
const PARTNER: [f32; 2] = [62.0, 1.0];
const FIELD: [u8; 4] = [0xDD, 0xDD, 0xDD, 0xFF];
const LOCKED: [u8; 4] = [0xCC, 0xCC, 0xCC, 0xFF];
/// What a deal holds at most, kinds of items.
pub const MOST_ITEMS: usize = 10;
/// The most zeny a deal takes.
pub const MOST_ZENY: u32 = 1_000_000_000;
/// Typed past the most: "the most is 1,000,000,000".
pub const ZENY_TOO_MUCH: usize = 2467;
/// Equip, unequip or use while trading.
pub const NO_USE: usize = 1699;
/// An item offered already; more than ten kinds.
const OVERLAP: usize = 51;
const FULL: usize = 297;

pub struct Assets {
    background: Texture,
    slot: Texture,
    /// Normal, hovered, pressed; then the greyed-out one.
    ok: [Texture; 4],
    exchange: [Texture; 4],
    cancel: [Texture; 3],
    field: Texture,
    locked: Texture,
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |name: &str| gpu.load_texture(grf, &format!("{UI}{name}.bmp"), false);
        Self {
            background: image("basic_interface\\exchange_bg2"),
            slot: image("basic_interface\\itemwin_mid"),
            ok: ["btn_ok", "btn_ok_a", "btn_ok_b", "btn_ok_dis"].map(image),
            exchange: ["btn_exchange", "btn_exchange_a", "btn_exchange_b", "btn_exchange_dis"].map(image),
            cancel: ["btn_cancel", "btn_cancel_a", "btn_cancel_b"].map(image),
            field: gpu.solid(FIELD),
            locked: gpu.solid(LOCKED),
        }
    }
}

/// The partner's C-Code: the character ID as 10 digits, each swapped through "3867012495", with "-" before the last
/// two (so a fake name with the same look still shows another code).
pub fn c_code(id: u32) -> String {
    let digits: String = format!("{id:010}").bytes().map(|digit| char::from(b"3867012495"[usize::from(digit - b'0')])).collect();
    format!("{}-{}", &digits[..8], &digits[8..])
}

/// The answer to a deal request (0x00e7): the red line it prints, if any. 3 opens the deal.
pub fn answer_line(result: u8) -> Option<usize> {
    match result {
        0..=2 => Some(70 + usize::from(result)),
        4 => Some(69),
        5 => Some(1708),
        _ => None,
    }
}

/// Where the window opens, every time: by the screen's size.
pub fn position(screen: [f32; 2]) -> [f32; 2] {
    [(screen[0] - 640.0) / 2.0 + 100.0, 100.0 * screen[1] / 480.0]
}

/// What the player does with an item while a deal is on.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum ItemAction {
    /// Use or equip.
    Use,
    Unequip,
    /// Let go on the ground, the storage, the cart, the equipment window or an NPC's list.
    DropOn,
}

/// Whether a deal refuses the action: `Some(Some(line))` with a line, `Some(None)` silently, `None` lets it be.
pub fn refusal(trading: bool, action: ItemAction) -> Option<Option<usize>> {
    match action {
        _ if !trading => None,
        ItemAction::Use | ItemAction::Unequip => Some(Some(NO_USE)),
        ItemAction::DropOn => Some(None),
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Ok,
    Exchange,
    Cancel,
    Zeny,
    /// An item in a box (0 yours, 1 theirs) by row.
    Item(usize, usize),
    /// Your box, where items are let go.
    Yours,
    Window,
}

#[derive(Debug, PartialEq)]
pub enum Action {
    /// Lock your side in (with the zeny typed).
    Lock(u32),
    Exchange,
    Cancel,
}

pub struct Trade {
    pub frame: Frame,
    /// "Trade : ", and the partner: name, level and C-Code.
    pub caption: String,
    pub title: String,
    /// Yours and theirs.
    pub items: [Vec<InventoryItem>; 2],
    /// Their zeny; yours is typed in.
    pub their_zeny: u32,
    pub typed: String,
    pub typing: bool,
    /// Each side locked in (yours, theirs).
    pub locked: [bool; 2],
    /// The item asked to go in (inventory index, amount), until the server says; one at a time.
    pub pending: Option<(u16, u32)>,
    /// Cancel and Trade, once pressed, wait for the server.
    pub cancelled: bool,
    pub exchanged: bool,
}

impl Trade {
    pub fn new(caption: String, title: String, screen: [f32; 2]) -> Self {
        let mut frame = Frame::new(position(screen), WIDTH);
        frame.open = true;
        Self {
            frame,
            caption,
            title,
            items: [Vec::new(), Vec::new()],
            their_zeny: 0,
            typed: "0".to_owned(),
            typing: false,
            locked: [false; 2],
            pending: None,
            cancelled: false,
            exchanged: false,
        }
    }

    pub fn height(&self) -> f32 {
        HEIGHT
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let local = self.frame.local(cursor, HEIGHT)?;
        for (place, hit) in [(OK, Hit::Ok), (EXCHANGE, Hit::Exchange), (CANCEL, Hit::Cancel)] {
            if contains(place, BUTTON, local) {
                return Some(hit);
            }
        }
        if contains(ZENY[0], ZENY_SIZE, local) {
            return Some(Hit::Zeny);
        }
        for (side, corner) in BOXES.iter().enumerate() {
            if contains(*corner, BOX_SIZE, local) {
                let row = ((local[1] - corner[1]) / ROW) as usize;
                if row < self.items[side].len() {
                    return Some(Hit::Item(side, row));
                }
                return Some(if side == 0 { Hit::Yours } else { Hit::Window });
            }
        }
        Some(Hit::Window)
    }

    /// Whether items can still go in on your side.
    pub fn open_for_items(&self) -> bool {
        !self.locked[0]
    }

    /// Whether this inventory item may go in: the line saying why not (`Some(None)`: dropped silently, an add is
    /// still on its way).
    pub fn refuse_offer(&self, index: u16) -> Option<Option<usize>> {
        if self.items[0].iter().any(|item| item.index == index) || self.pending.is_some_and(|(pending, _)| pending == index) {
            return Some(Some(OVERLAP));
        }
        if self.items[0].len() >= MOST_ITEMS {
            return Some(Some(FULL));
        }
        self.pending.is_some().then_some(None)
    }

    /// Their zeny as shown: "0" until they press OK.
    pub fn their_zeny_shown(&self) -> u32 {
        if self.locked[1] { self.their_zeny } else { 0 }
    }

    pub fn click(&mut self, hit: Hit) -> Option<Action> {
        self.typing = hit == Hit::Zeny && !self.locked[0];
        match hit {
            Hit::Cancel if !self.cancelled => {
                self.cancelled = true;
                Some(Action::Cancel)
            }
            Hit::Ok if !self.locked[0] => Some(Action::Lock(self.typed.parse().unwrap_or(0))),
            Hit::Exchange if self.exchange_enabled() => {
                self.exchanged = true;
                Some(Action::Exchange)
            }
            _ => None,
        }
    }

    /// OK, Trade and Cancel on screen, by name.
    pub fn button_boxes(&self) -> Vec<(String, [f32; 4])> {
        [("trade_ok", OK), ("trade_exchange", EXCHANGE), ("trade_cancel", CANCEL)]
            .map(|(name, place)| {
                let [x, y] = self.frame.at(place);
                (name.to_owned(), [x, y, BUTTON[0], BUTTON[1]])
            })
            .to_vec()
    }

    /// Trade takes a press while both sides are locked, once.
    pub fn exchange_enabled(&self) -> bool {
        self.locked == [true; 2] && !self.exchanged
    }

    /// Types digits into the zeny; past the most it is the most, and says so.
    pub fn type_text(&mut self, text: &str) -> Option<usize> {
        if self.typed == "0" {
            self.typed.clear();
        }
        self.typed.extend(text.chars().filter(char::is_ascii_digit).take(13usize.saturating_sub(self.typed.len())));
        if self.typed.parse::<u64>().is_ok_and(|zeny| zeny > u64::from(MOST_ZENY)) {
            self.typed = MOST_ZENY.to_string();
            return Some(ZENY_TOO_MUCH);
        }
        None
    }

    pub fn icons(&self, info: &HashMap<u32, ItemInfo>) -> Vec<String> {
        self.items.iter().flatten().filter_map(|item| info.get(&item.id).map(|info| inventory::icon_path(info, item.identified))).collect()
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
        font: &Font,
        pointer: ([f32; 2], bool),
        caret: bool,
    ) {
        let at = |local: [f32; 2]| self.frame.at(local);
        // The title bar has no buttons: the two captions only.
        self.frame.draw_bar(layer, chrome);
        for (text, place) in [(&self.caption, CAPTION), (&self.title, PARTNER)] {
            layer.texts.push((text.clone(), at(offset(place, [1.0, 1.0])), [255; 3]));
            layer.texts.push((text.clone(), at(place), [0, 0, 0]));
        }
        let hit = self.hit(pointer.0);
        let look = |target: Hit| {
            let hovered = hit == Some(target);
            usize::from(hovered) + usize::from(hovered && pointer.1)
        };
        layer.sprites.push((&assets.background, at([0.0, TITLE_HEIGHT]), assets.background.size));
        for (side, corner) in BOXES.iter().enumerate() {
            // A locked side greys out; open ones show a slot per row.
            if self.locked[side] {
                layer.sprites.push((&assets.locked, at(*corner), BOX_SIZE));
            } else {
                for row in 0..ROWS {
                    layer.sprites.push((&assets.slot, at(offset(*corner, [1.0, row as f32 * ROW])), [32.0, ROW]));
                }
            }
            for (row, item) in self.items[side].iter().enumerate() {
                let top = offset(*corner, [0.0, row as f32 * ROW]);
                if let Some(Some(icon)) = items.get(&item.id).and_then(|info| icons.get(&inventory::icon_path(info, item.identified))) {
                    layer.sprites.push((icon, at(offset(top, [4.0, 4.0])), icon.size));
                }
                if inventory::tab_of(item.kind) != 1 {
                    let amount = item.amount.to_string();
                    let place = at(offset(top, [28.0 - font.width(&amount), 15.0]));
                    layer.texts.push((amount.clone(), offset(place, [-1.0, -1.0]), [255; 3]));
                    layer.texts.push((amount, place, [0, 0, 0]));
                }
                let name = inventory::item_name(items, item.id, item.identified, item.refine);
                layer.texts.push((window::fit(font, &name, 190.0), at(offset(top, [34.0, 9.0])), if item.identified { [0, 0, 0] } else { UNIDENTIFIED }));
                if hit == Some(Hit::Item(side, row)) {
                    window::tooltip(tooltip, chrome, font, &name, offset(pointer.0, [12.0, 12.0]), [255; 3]);
                }
            }
        }
        // Zeny: yours typed into a grey field until locked, theirs as text.
        if !self.locked[0] {
            layer.sprites.push((&assets.field, at(ZENY[0]), ZENY_SIZE));
        }
        let typed = if self.typing && caret { format!("{}|", self.typed) } else { self.typed.clone() };
        layer.texts.push((typed, at(offset(ZENY[0], [2.0, 2.0])), [0, 0, 0]));
        layer.texts.push((group(u64::from(self.their_zeny_shown())), at(offset(ZENY[1], [2.0, 2.0])), [0, 0, 0]));
        let ok = if self.locked[0] { &assets.ok[3] } else { &assets.ok[look(Hit::Ok)] };
        let exchange = if self.exchange_enabled() { &assets.exchange[look(Hit::Exchange)] } else { &assets.exchange[3] };
        // ponytail: there is no greyed-out Cancel; once pressed it stays at rest.
        let cancel = if self.cancelled { &assets.cancel[0] } else { &assets.cancel[look(Hit::Cancel)] };
        for (image, place) in [(ok, OK), (exchange, EXCHANGE), (cancel, CANCEL)] {
            layer.sprites.push((image, at(place), BUTTON));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spells_ids_and_locks_both_sides() {
        assert_eq!(c_code(150_004), "33338133-30");
        assert_eq!(c_code(0), "33333333-33");
        assert_eq!(c_code(u32::MAX), "06505246-51");
        let mut trade = Trade::new("Trade : ".into(), "Tester2".into(), [1024.0, 768.0]);
        assert_eq!(trade.frame.position, [292.0, 160.0]);
        assert_eq!(trade.type_text("12a3"), None);
        assert_eq!(trade.click(Hit::Ok), Some(Action::Lock(123)));
        assert_eq!(trade.click(Hit::Exchange), None);
        trade.locked = [true; 2];
        assert_eq!(trade.click(Hit::Ok), None);
        assert_eq!(trade.click(Hit::Exchange), Some(Action::Exchange));
        assert_eq!(trade.click(Hit::Exchange), None, "once");
        assert_eq!(trade.click(Hit::Cancel), Some(Action::Cancel));
        assert_eq!(trade.click(Hit::Cancel), None, "waits for the server");
    }

    #[test]
    fn answers_requests_by_number() {
        assert_eq!([0, 1, 2, 3, 4, 5, 6].map(answer_line), [Some(70), Some(71), Some(72), None, Some(69), Some(1708), None]);
    }

    #[test]
    fn caps_zeny_and_shows_theirs_at_ok() {
        let mut trade = Trade::new(String::new(), String::new(), [800.0, 600.0]);
        assert_eq!(trade.frame.position, [180.0, 125.0]);
        assert_eq!(trade.type_text("999999999"), None);
        assert_eq!(trade.type_text("9"), Some(ZENY_TOO_MUCH));
        assert_eq!(trade.typed, "1000000000");
        trade.their_zeny = 500;
        assert_eq!(trade.their_zeny_shown(), 0);
        trade.locked[1] = true;
        assert_eq!(trade.their_zeny_shown(), 500);
    }

    #[test]
    fn takes_one_add_at_a_time_up_to_ten_kinds() {
        let mut trade = Trade::new(String::new(), String::new(), [1024.0, 768.0]);
        assert_eq!(trade.refuse_offer(5), None);
        trade.pending = Some((5, 1));
        assert_eq!(trade.refuse_offer(5), Some(Some(OVERLAP)));
        assert_eq!(trade.refuse_offer(6), Some(None), "one add on its way");
        trade.pending = None;
        trade.items[0] = (0..10).map(|index| InventoryItem { index, ..Default::default() }).collect();
        assert_eq!(trade.refuse_offer(3), Some(Some(OVERLAP)));
        assert_eq!(trade.refuse_offer(20), Some(Some(FULL)));
    }

    #[test]
    fn blocks_items_while_trading() {
        for action in [ItemAction::Use, ItemAction::Unequip, ItemAction::DropOn] {
            assert_eq!(refusal(false, action), None);
        }
        assert_eq!([ItemAction::Use, ItemAction::Unequip].map(|action| refusal(true, action)), [Some(Some(NO_USE)); 2]);
        assert_eq!(refusal(true, ItemAction::DropOn), Some(None));
    }
}
