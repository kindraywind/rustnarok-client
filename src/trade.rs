//! Dealing with another player: the trade window (yours on the left, theirs on the right, zeny under each), and the
//! question when someone asks for a deal.

use std::collections::HashMap;

use roclient::grf::Grf;
use roclient::lua::ItemInfo;
use roclient::net::InventoryItem;
use roclient::text::Font;

use crate::inventory::{self, UNIDENTIFIED};
use crate::render::{Gpu, Texture};
use crate::shop::group;
use crate::window::{self, Chrome, Frame, TITLE_HEIGHT, TitleHit};
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
const POSITION: [f32; 2] = [200.0, 150.0];
const FIELD: [u8; 4] = [0xDD, 0xDD, 0xDD, 0xFF];
const LOCKED: [u8; 4] = [0xCC, 0xCC, 0xCC, 0xFF];
/// What a deal holds at most, kinds of items.
pub const MOST_ITEMS: usize = 10;

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

/// The client shows the partner's character ID as letters, so fakes stand out.
pub fn id_code(id: u32) -> String {
    id.to_string().bytes().map(|digit| char::from(b"ROHUTNASEW"[usize::from(digit - b'0')])).collect()
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(TitleHit),
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
    pub title: String,
    /// Yours and theirs.
    pub items: [Vec<InventoryItem>; 2],
    /// Their zeny; yours is typed in.
    pub their_zeny: u32,
    pub typed: String,
    pub typing: bool,
    /// Each side locked in (yours, theirs).
    pub locked: [bool; 2],
    /// Items asked to go in, by inventory index, until the server says.
    pub pending: HashMap<u16, u32>,
}

impl Trade {
    pub fn new(title: String) -> Self {
        let mut frame = Frame::new(POSITION, WIDTH);
        frame.open = true;
        Self { frame, title, items: [Vec::new(), Vec::new()], their_zeny: 0, typed: "0".to_owned(), typing: false, locked: [false; 2], pending: HashMap::new() }
    }

    pub fn height(&self) -> f32 {
        HEIGHT
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let local = self.frame.local(cursor, HEIGHT)?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(Hit::Title(hit));
        }
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

    pub fn click(&mut self, hit: Hit) -> Option<Action> {
        self.typing = hit == Hit::Zeny && !self.locked[0];
        match hit {
            Hit::Title(TitleHit::Close) | Hit::Cancel => Some(Action::Cancel),
            Hit::Title(title) => {
                self.frame.click_title(title);
                None
            }
            Hit::Ok if !self.locked[0] => Some(Action::Lock(self.typed.parse().unwrap_or(0))),
            Hit::Exchange if self.locked == [true; 2] => Some(Action::Exchange),
            _ => None,
        }
    }

    pub fn type_text(&mut self, text: &str) {
        if self.typed == "0" {
            self.typed.clear();
        }
        let room = 9usize.saturating_sub(self.typed.len());
        self.typed.extend(text.chars().filter(char::is_ascii_digit).take(room));
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
        self.frame.draw_title(layer, chrome, &format!("Trade : {}", self.title), pointer.0);
        if self.frame.minimized {
            return;
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
        layer.texts.push((group(u64::from(self.their_zeny)), at(offset(ZENY[1], [2.0, 2.0])), [0, 0, 0]));
        let ok = if self.locked[0] { &assets.ok[3] } else { &assets.ok[look(Hit::Ok)] };
        let exchange = if self.locked == [true; 2] { &assets.exchange[look(Hit::Exchange)] } else { &assets.exchange[3] };
        for (image, place) in [(ok, OK), (exchange, EXCHANGE), (&assets.cancel[look(Hit::Cancel)], CANCEL)] {
            layer.sprites.push((image, at(place), BUTTON));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spells_ids_and_locks_both_sides() {
        assert_eq!(id_code(150_004), "ONRRRT");
        let mut trade = Trade::new("Tester2".into());
        trade.type_text("12a3");
        assert_eq!(trade.click(Hit::Ok), Some(Action::Lock(123)));
        assert_eq!(trade.click(Hit::Exchange), None);
        trade.locked = [true; 2];
        assert_eq!(trade.click(Hit::Ok), None);
        assert_eq!(trade.click(Hit::Exchange), Some(Action::Exchange));
    }
}
