//! NPC shops as the client shows them: a window of what the shop sells (or would buy), and one of the deal being
//! made with its total, side by side.

use std::collections::HashMap;

use rustnarok_client::grf::Grf;
use rustnarok_client::lua::ItemInfo;
use rustnarok_client::net::{ITEM_ARMOR, ITEM_PET_ARMOR, ITEM_PET_EGG, ITEM_WEAPON, MapSession};
use rustnarok_client::text::Font;

use crate::inventory;
use crate::render::{Gpu, Texture};
use crate::window::{self, Chrome, Clicks, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI, contains, offset};

// ponytail: 280px windows of 32px rows; calibrate against the real client.
const WIDTH: f32 = 280.0;
const ROW: f32 = 32.0;
/// Rows each window shows: the goods and the deal.
const ROWS: [usize; 2] = [7, 2];
const POSITIONS: [[f32; 2]; 2] = [[100.0, 100.0], [100.0 + WIDTH + 10.0, 100.0 + 5.0 * ROW]];
const PADDING: f32 = 5.0;
const LEFT: f32 = 16.0;
const FOOTER: f32 = 27.0;
const BUTTON: [f32; 2] = [42.0, 20.0];
const SELECTED: [u8; 4] = [0x34, 0x6A, 0xE1, 0x80];
/// Titles: the goods when buying and selling, the deal when buying and selling.
const TITLES: [[usize; 2]; 2] = [[186, 185], [166, 168]];
/// "합계 : %d z".
const TOTAL_MESSAGE: usize = 3699;

pub struct Assets {
    strip: Texture,
    bar: Texture,
    /// Buy, sell and cancel: normal, hovered, pressed.
    buttons: [[Texture; 3]; 3],
    selected: Texture,
}

impl Assets {
    /// Buy, sell and cancel.
    pub fn buttons(&self) -> [&[Texture; 3]; 3] {
        [&self.buttons[0], &self.buttons[1], &self.buttons[2]]
    }

    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |path: String| gpu.load_texture(grf, &path, false);
        let button = |name: &str| ["", "_a", "_b"].map(|suffix| image(format!("{UI}btn_{name}{suffix}.bmp")));
        Self {
            strip: image(format!("{UI}basic_interface\\itemwin_left.bmp")),
            bar: image(format!("{UI}basic_interface\\btnbar_mid2.bmp")),
            buttons: [button("buy"), button("sell"), button("cancel")],
            selected: gpu.solid(SELECTED),
        }
    }
}

/// Something the shop sells, or something carried it would buy.
#[derive(Debug, Clone, PartialEq)]
pub struct Offer {
    pub item: u32,
    pub price: u32,
    /// Selling: the inventory entry and how many are carried.
    pub index: u16,
    pub have: u16,
    pub kind: u8,
    pub identified: bool,
    pub refine: u8,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(usize, TitleHit),
    Row(usize, usize),
    Deal,
    Cancel,
    Window(usize),
}

/// What a click asks the client to do.
#[derive(Debug, PartialEq)]
pub enum Action {
    /// Buy or sell what's in the deal.
    Deal,
    Close,
    /// Ask how many of an offer to put in the deal.
    Ask(usize),
}

pub struct Shop {
    pub selling: bool,
    /// A player's shop: the vendor, their shop's ID, and its name (the first window's title).
    pub vendor: Option<(u32, u32, String)>,
    pub offers: Vec<Offer>,
    /// Offers in the deal, and how many.
    pub deal: Vec<(usize, u32)>,
    pub frames: [Frame; 2],
    scroll: [usize; 2],
    selected: [Option<usize>; 2],
    clicks: Clicks,
}

impl Shop {
    pub fn new(selling: bool, offers: Vec<Offer>) -> Self {
        let frames = POSITIONS.map(|position| {
            let mut frame = Frame::new(position, WIDTH);
            frame.open = true;
            frame
        });
        Self { selling, vendor: None, offers, deal: Vec::new(), frames, scroll: [0; 2], selected: [None; 2], clicks: Clicks::default() }
    }

    fn height(window: usize) -> f32 {
        TITLE_HEIGHT + PADDING * 2.0 + ROWS[window] as f32 * ROW + FOOTER
    }

    /// The rows a window lists: offers, or the deal's offers with their amounts.
    fn rows(&self, window: usize) -> Vec<(usize, Option<u32>)> {
        if window == 0 {
            (0..self.offers.len()).map(|offer| (offer, None)).collect()
        } else {
            self.deal.iter().map(|&(offer, amount)| (offer, Some(amount))).collect()
        }
    }

    pub fn total(&self) -> u64 {
        self.deal.iter().map(|&(offer, amount)| u64::from(self.offers[offer].price) * u64::from(amount)).sum()
    }

    fn deal_button(&self, window: usize) -> ([f32; 2], [f32; 2]) {
        let top = Self::height(window) - FOOTER + 4.0;
        if window == 1 { ([WIDTH - 62.0 - BUTTON[0], top], [WIDTH - 15.0 - BUTTON[0], top]) } else { ([-100.0; 2], [-100.0; 2]) }
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        for window in [1, 0] {
            let Some(local) = self.frames[window].local(cursor, Self::height(window)) else { continue };
            if let Some(hit) = self.frames[window].title_hit(local) {
                return Some(Hit::Title(window, hit));
            }
            let (deal, cancel) = self.deal_button(window);
            if contains(deal, BUTTON, local) {
                return Some(Hit::Deal);
            }
            if contains(cancel, BUTTON, local) {
                return Some(Hit::Cancel);
            }
            let top = TITLE_HEIGHT + PADDING;
            if local[1] >= top && local[1] < top + ROWS[window] as f32 * ROW {
                let row = ((local[1] - top) / ROW) as usize + self.scroll[window];
                if row < self.rows(window).len() {
                    return Some(Hit::Row(window, row));
                }
            }
            return Some(Hit::Window(window));
        }
        None
    }

    pub fn click(&mut self, hit: Hit) -> Option<Action> {
        match hit {
            // Closing either window ends the deal.
            Hit::Title(_, TitleHit::Close) => return Some(Action::Close),
            Hit::Title(window, title) => self.frames[window].click_title(title),
            Hit::Row(window, row) => {
                self.selected[window] = Some(row);
                // Rows of both windows share the double click tracker: tell them apart.
                if self.clicks.double((window * 1000 + row) as u16) {
                    return self.pick(window, row);
                }
            }
            Hit::Deal => return (!self.deal.is_empty()).then_some(Action::Deal),
            Hit::Cancel => return Some(Action::Close),
            Hit::Window(_) => {}
        }
        None
    }

    /// A double click: an offer goes into the deal (asking how many when there can be more than one), or a deal
    /// row comes out.
    fn pick(&mut self, window: usize, row: usize) -> Option<Action> {
        if window == 1 {
            self.deal.remove(row);
            self.selected[1] = None;
            return None;
        }
        let offer = &self.offers[row];
        let single =
            matches!(offer.kind, ITEM_WEAPON | ITEM_ARMOR | ITEM_PET_EGG | ITEM_PET_ARMOR) || ((self.selling || self.vendor.is_some()) && offer.have == 1);
        if single {
            self.add(row, 1);
            return None;
        }
        Some(Action::Ask(row))
    }

    /// Puts `amount` of an offer in the deal (selling: no more than carried; equipment: once).
    pub fn add(&mut self, offer: usize, amount: u32) {
        if amount == 0 {
            return;
        }
        let single = matches!(self.offers[offer].kind, ITEM_WEAPON | ITEM_ARMOR | ITEM_PET_EGG | ITEM_PET_ARMOR);
        // Selling, or buying from a player: no more than there are.
        let limit = if self.selling || self.vendor.is_some() {
            u32::from(self.offers[offer].have)
        } else if single {
            1
        } else {
            30_000
        };
        match self.deal.iter_mut().find(|(row, _)| *row == offer) {
            Some((_, had)) => *had = (*had + amount).min(limit),
            None => self.deal.push((offer, amount.min(limit))),
        }
    }

    /// Asks the server for the deal.
    pub fn send(&self, session: &mut MapSession) -> std::io::Result<()> {
        if let Some((vendor, shop, _)) = &self.vendor {
            let items: Vec<_> = self.deal.iter().map(|&(offer, amount)| (amount as u16, self.offers[offer].index)).collect();
            return session.buy_from_vendor(*vendor, *shop, &items);
        }
        if self.selling {
            session.sell(&self.deal.iter().map(|&(offer, amount)| (self.offers[offer].index, amount as u16)).collect::<Vec<_>>())
        } else {
            session.buy(&self.deal.iter().map(|&(offer, amount)| (self.offers[offer].item, amount as u16)).collect::<Vec<_>>())
        }
    }

    /// Whether the pointer is on the shop's windows.
    pub fn covers(&self, cursor: [f32; 2]) -> bool {
        self.hit(cursor).is_some()
    }

    pub fn scroll_by(&mut self, cursor: [f32; 2], rows: i32) {
        for (window, frame) in self.frames.iter().enumerate() {
            if frame.local(cursor, Self::height(window)).is_some() {
                let most = self.rows(window).len().saturating_sub(ROWS[window]);
                self.scroll[window] = (self.scroll[window] as i32 + rows).clamp(0, most as i32) as usize;
            }
        }
    }

    pub fn icons(&self, items: &HashMap<u32, ItemInfo>) -> Vec<String> {
        self.offers.iter().filter_map(|offer| items.get(&offer.item).map(|info| inventory::icon_path(info, offer.identified))).collect()
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
        pointer: ([f32; 2], bool),
    ) {
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        let hit = self.hit(pointer.0);
        let look = |target: Hit| {
            let hovered = hit == Some(target);
            usize::from(hovered) + usize::from(hovered && pointer.1)
        };
        for window in 0..2 {
            let frame = &self.frames[window];
            if !frame.open {
                continue;
            }
            let title = match &self.vendor {
                Some((.., name)) if window == 0 => name.as_str(),
                _ => message(TITLES[window][usize::from(self.selling)]),
            };
            frame.draw_title(layer, chrome, title, pointer.0);
            if frame.minimized {
                continue;
            }
            let height = Self::height(window);
            let list_height = height - TITLE_HEIGHT - FOOTER;
            layer.sprites.push((&chrome.white, frame.at([0.0, TITLE_HEIGHT]), [WIDTH, list_height]));
            layer.sprites.push((&assets.strip, frame.at([0.0, TITLE_HEIGHT]), [assets.strip.size[0], list_height]));
            layer.sprites.push((&chrome.border, frame.at([WIDTH - 1.0, TITLE_HEIGHT]), [1.0, list_height]));
            for (place, (offer_index, amount)) in self.rows(window).into_iter().enumerate().skip(self.scroll[window]).take(ROWS[window]) {
                let top = TITLE_HEIGHT + PADDING + (place - self.scroll[window]) as f32 * ROW;
                let offer = &self.offers[offer_index];
                if self.selected[window] == Some(place) {
                    layer.sprites.push((&assets.selected, frame.at([LEFT, top]), [WIDTH - LEFT - 1.0, ROW]));
                }
                let icon = items.get(&offer.item).and_then(|info| icons.get(&inventory::icon_path(info, offer.identified))).and_then(Option::as_ref);
                if let Some(icon) = icon {
                    layer.sprites.push((icon, frame.at([LEFT + 4.0, top + 4.0]), icon.size));
                }
                // How many: carried (selling) on the icon, or in the deal.
                let count = amount.or(((self.selling || self.vendor.is_some()) && window == 0).then_some(u32::from(offer.have)));
                if let Some(count) = count {
                    let text = count.to_string();
                    let place = frame.at([LEFT + 18.0, top + 18.0]);
                    layer.texts.push((text.clone(), offset(place, [-1.0, -1.0]), [255; 3]));
                    layer.texts.push((text, place, [0, 0, 0]));
                }
                let name = inventory::item_name(items, offer.item, offer.identified, offer.refine);
                layer.texts.push((window::fit(font, &name, 115.0), frame.at([LEFT + 32.0, top + 10.0]), [0, 0, 0]));
                let price = format!("{} z", group(u64::from(offer.price) * u64::from(amount.unwrap_or(1))));
                layer.texts.push((price.clone(), frame.at([WIDTH - 4.0 - font.width(&price), top + 10.0]), [0, 0, 0]));
            }
            // Footer: the deal's total and buttons.
            let footer = height - FOOTER;
            layer.sprites.push((&assets.bar, frame.at([0.0, footer]), [WIDTH, FOOTER]));
            if window == 1 {
                let total = rustnarok_client::messages::fill(message(TOTAL_MESSAGE), &[&group(self.total())]);
                layer.texts.push((total, frame.at([10.0, footer + 7.0]), [0, 0, 0]));
                let (deal, cancel) = self.deal_button(1);
                let deal_images = &assets.buttons[usize::from(self.selling)];
                layer.sprites.push((&deal_images[look(Hit::Deal)], frame.at(deal), BUTTON));
                layer.sprites.push((&assets.buttons[2][look(Hit::Cancel)], frame.at(cancel), BUTTON));
            }
        }
    }
}

/// A number with thousands separated by commas, as the client shows prices.
pub fn group(number: u64) -> String {
    let digits = number.to_string();
    let mut grouped = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    grouped
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offer(item: u32, price: u32, have: u16, kind: u8) -> Offer {
        Offer { item, price, index: 2, have, kind, identified: true, refine: 0 }
    }

    #[test]
    fn makes_deals() {
        let mut shop = Shop::new(false, vec![offer(501, 50, 0, 0), offer(1201, 100, 0, ITEM_WEAPON)]);
        shop.add(0, 3);
        shop.add(0, 2);
        shop.add(1, 5);
        // Five potions; one knife however many are asked.
        assert_eq!(shop.deal, [(0, 5), (1, 1)]);
        assert_eq!(shop.total(), 350);
        let mut selling = Shop::new(true, vec![offer(909, 3, 4, 3)]);
        selling.add(0, 10);
        assert_eq!(selling.deal, [(0, 4)]);
        assert_eq!(group(1_234_567), "1,234,567");
    }
}
