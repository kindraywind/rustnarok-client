//! NPC shops as the client shows them: a window of what the shop sells (or would buy), and one of the deal being
//! made with its total, side by side.

use std::collections::HashMap;

use rustnarok_client::grf::Grf;
use rustnarok_client::lua::ItemInfo;
use rustnarok_client::net::{BuyingItem, ITEM_ARMOR, ITEM_PET_ARMOR, ITEM_PET_EGG, ITEM_WEAPON, InventoryItem, MapSession, MarketKind};
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
/// Barters: "%s %d ea" (an item it costs), "money" (zeny it costs), and the deal window's title.
const COSTS_MESSAGE: usize = 3553;
const MONEY_MESSAGE: usize = 3552;
const EXCHANGE_TITLE: usize = 3689;
const LIMIT_MESSAGE: usize = 1735;
const WANTED_TITLE: usize = 1737;
pub const OVER_WANTED_MESSAGE: usize = 1739;
pub const OVER_LIMIT_MESSAGE: usize = 1740;
const GREY: [u8; 3] = [0x70, 0x70, 0x70];

pub struct Assets {
    strip: Texture,
    bar: Texture,
    /// Buy, sell, cancel and exchange: normal, hovered, pressed.
    buttons: [[Texture; 3]; 4],
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
            buttons: [button("buy"), button("sell"), button("cancel"), button("exchange")],
            selected: gpu.solid(SELECTED),
        }
    }
}

/// Something the shop sells, or something carried it would buy.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Offer {
    pub item: u32,
    pub price: u32,
    /// Selling: the inventory entry and how many are carried.
    pub index: u16,
    pub have: u16,
    pub kind: u8,
    pub identified: bool,
    pub refine: u8,
    /// Markets and barters: how many are left (None: no end).
    pub stock: Option<u32>,
    /// Barters: the items it costs and how many (`price` is the zeny it costs too).
    pub costs: Vec<(u32, u32)>,
    /// Barters: the shop's own index for it, and the inventory index of the item it costs.
    pub shop_index: u32,
    pub cost_index: u16,
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
    /// A player's buying store we are selling into: `(owner, store_id, zeny_limit)`.
    pub buying_store: Option<(u32, u32, u32)>,
    /// A market or barter rather than a plain NPC shop.
    pub market: Option<MarketKind>,
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
        Self {
            selling,
            vendor: None,
            buying_store: None,
            market: None,
            offers,
            deal: Vec::new(),
            frames,
            scroll: [0; 2],
            selected: [None; 2],
            clicks: Clicks::default(),
        }
    }

    /// Opens a seller's window for another player's buying store (0x0818): skips wanted rows at 0 and matches
    /// carried stacks that are identified, not worn, and have no cards.
    pub fn new_buying_store(owner: u32, store_id: u32, limit: u32, wanted: &[BuyingItem], inventory: &[InventoryItem]) -> Self {
        let offers = wanted
            .iter()
            .filter(|item| item.amount > 0)
            .map(|item| {
                let carried = inventory.iter().find(|inv| inv.id == item.item_id && inv.worn == 0 && inv.identified && inv.cards == [0; 4]);
                Offer {
                    item: item.item_id,
                    price: item.price,
                    index: carried.map_or(0, |inv| inv.index),
                    have: carried.map_or(0, |inv| inv.amount),
                    kind: item.kind,
                    identified: true,
                    stock: Some(u32::from(item.amount)),
                    ..Offer::default()
                }
            })
            .collect();
        let mut shop = Self::new(true, offers);
        shop.buying_store = Some((owner, store_id, limit));
        shop
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
        if self.buying_store.is_some() {
            return (offer.have > 0).then_some(Action::Ask(row));
        }
        let single =
            matches!(offer.kind, ITEM_WEAPON | ITEM_ARMOR | ITEM_PET_EGG | ITEM_PET_ARMOR) || ((self.selling || self.vendor.is_some()) && offer.have == 1);
        if single {
            self.add(row, 1);
            return None;
        }
        Some(Action::Ask(row))
    }

    /// Puts `amount` of an offer in the deal. For a buying store, checks the wanted amount (1739) and the buyer's
    /// zeny limit (1740), returning `Some((message_id, arg))` when refused.
    pub fn add_checked(&mut self, offer: usize, amount: u32) -> Option<(usize, Option<u32>)> {
        if amount == 0 {
            return None;
        }
        if let Some((_, _, zeny_limit)) = self.buying_store {
            if self.offers[offer].have == 0 {
                return None;
            }
            let wanted = self.offers[offer].stock.unwrap_or(0);
            let already = self.deal.iter().find(|(row, _)| *row == offer).map_or(0, |&(_, had)| had);
            if already + amount > wanted {
                return Some((OVER_WANTED_MESSAGE, Some(wanted)));
            }
            let new_amount = (already + amount).min(u32::from(self.offers[offer].have));
            if new_amount == already {
                return None;
            }
            let added_cost = u64::from(new_amount - already) * u64::from(self.offers[offer].price);
            if self.total() + added_cost > u64::from(zeny_limit) {
                return Some((OVER_LIMIT_MESSAGE, None));
            }
            match self.deal.iter_mut().find(|(row, _)| *row == offer) {
                Some((_, had)) => *had = new_amount,
                None => self.deal.push((offer, new_amount)),
            }
            return None;
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
        // A market or barter has no more than its stock.
        let limit = self.offers[offer].stock.map_or(limit, |stock| limit.min(stock));
        match self.deal.iter_mut().find(|(row, _)| *row == offer) {
            Some((_, had)) => *had = (*had + amount).min(limit),
            None => self.deal.push((offer, amount.min(limit))),
        }
        None
    }

    /// Puts `amount` of an offer in the deal (selling: no more than carried; equipment: once).
    pub fn add(&mut self, offer: usize, amount: u32) {
        let _ = self.add_checked(offer, amount);
    }

    /// Asks the server for the deal.
    pub fn send(&self, session: &mut MapSession) -> std::io::Result<()> {
        let deal = self.deal.iter().map(|&(offer, amount)| (&self.offers[offer], amount));
        match self.market {
            Some(MarketKind::Market) => return session.market_buy(&deal.map(|(offer, amount)| (offer.item, amount)).collect::<Vec<_>>()),
            Some(MarketKind::Barter) => {
                return session.barter_buy(&deal.map(|(offer, amount)| (offer.item, amount, offer.cost_index, offer.shop_index)).collect::<Vec<_>>());
            }
            Some(MarketKind::Expanded) => {
                return session.expanded_barter_buy(&deal.map(|(offer, amount)| (offer.item, offer.shop_index, amount)).collect::<Vec<_>>());
            }
            None => {}
        }
        if let Some((owner, store_id, _)) = self.buying_store {
            let items: Vec<_> = self.deal.iter().map(|&(offer, amount)| (self.offers[offer].index, self.offers[offer].item, amount as u16)).collect();
            return session.sell_to_buying_store(owner, store_id, &items);
        }
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

    /// After a purchase went through: what was bought leaves the stock, and the deal empties.
    pub fn bought(&mut self, items: &[(u32, u32)]) {
        for &(item, amount) in items {
            if let Some(stock) = self.offers.iter_mut().find(|offer| offer.item == item).and_then(|offer| offer.stock.as_mut()) {
                *stock = stock.saturating_sub(amount);
            }
        }
        self.deal.clear();
        self.selected[1] = None;
    }

    /// What a barter's deal costs, or a row's: "Ore 20 ea, ... money".
    fn costs_text(&self, items: &HashMap<u32, ItemInfo>, messages: &[String], rows: &[(usize, u32)]) -> String {
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        let mut costs: Vec<(u32, u64)> = Vec::new();
        let mut zeny = 0u64;
        for &(offer, amount) in rows {
            let offer = &self.offers[offer];
            zeny += u64::from(offer.price) * u64::from(amount);
            for &(item, each) in &offer.costs {
                match costs.iter_mut().find(|(cost, _)| *cost == item) {
                    Some((_, total)) => *total += u64::from(each) * u64::from(amount),
                    None => costs.push((item, u64::from(each) * u64::from(amount))),
                }
            }
        }
        let mut parts: Vec<String> = costs
            .into_iter()
            .map(|(item, total)| rustnarok_client::messages::fill(message(COSTS_MESSAGE), &[&inventory::item_name(items, item, true, 0), &total]))
            .collect();
        if zeny > 0 {
            parts.push(format!("{} {}", group(zeny), message(MONEY_MESSAGE)));
        }
        parts.join(", ")
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
            let barter = matches!(self.market, Some(MarketKind::Barter | MarketKind::Expanded));
            let title = match &self.vendor {
                Some((.., name)) if window == 0 => name.as_str(),
                _ if self.buying_store.is_some() && window == 0 => message(WANTED_TITLE),
                _ if barter && window == 1 => message(EXCHANGE_TITLE),
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
                let count = amount.or(if self.buying_store.is_some() && window == 0 {
                    offer.stock
                } else if (self.selling || self.vendor.is_some()) && window == 0 {
                    Some(u32::from(offer.have))
                } else {
                    None
                });
                if let Some(count) = count {
                    let text = count.to_string();
                    let place = frame.at([LEFT + 18.0, top + 18.0]);
                    layer.texts.push((text.clone(), offset(place, [-1.0, -1.0]), [255; 3]));
                    layer.texts.push((text, place, [0, 0, 0]));
                }
                let name = inventory::item_name(items, offer.item, offer.identified, offer.refine);
                // ponytail: a barter's costs and a market's stock go on a second line, in grey; where the real
                // client shows them isn't known.
                if barter {
                    layer.texts.push((window::fit(font, &name, WIDTH - LEFT - 40.0), frame.at([LEFT + 32.0, top + 3.0]), [0, 0, 0]));
                    let costs = self.costs_text(items, messages, &[(offer_index, amount.unwrap_or(1))]);
                    layer.texts.push((window::fit(font, &costs, WIDTH - LEFT - 40.0), frame.at([LEFT + 32.0, top + 17.0]), GREY));
                    continue;
                }
                let (name_top, price_top) = if offer.stock.is_some() { (3.0, 3.0) } else { (10.0, 10.0) };
                layer.texts.push((window::fit(font, &name, 115.0), frame.at([LEFT + 32.0, top + name_top]), [0, 0, 0]));
                let price = format!("{} z", group(u64::from(offer.price) * u64::from(amount.unwrap_or(1))));
                layer.texts.push((price.clone(), frame.at([WIDTH - 4.0 - font.width(&price), top + price_top]), [0, 0, 0]));
                if let (Some(stock), 0) = (offer.stock, window) {
                    let left = group(u64::from(stock));
                    layer.texts.push((left.clone(), frame.at([WIDTH - 4.0 - font.width(&left), top + 17.0]), GREY));
                }
            }
            // Footer: the deal's total and buttons.
            let footer = height - FOOTER;
            layer.sprites.push((&assets.bar, frame.at([0.0, footer]), [WIDTH, FOOTER]));
            if let (Some((_, _, limit)), 0) = (self.buying_store, window) {
                let text = rustnarok_client::messages::fill(message(LIMIT_MESSAGE), &[&group(u64::from(limit))]);
                layer.texts.push((window::fit(font, &text, WIDTH - 20.0), frame.at([10.0, footer + 7.0]), [0, 0, 0]));
            }
            if window == 1 {
                let total = if barter {
                    window::fit(font, &self.costs_text(items, messages, &self.deal.iter().map(|&(offer, amount)| (offer, amount)).collect::<Vec<_>>()), 150.0)
                } else {
                    rustnarok_client::messages::fill(message(TOTAL_MESSAGE), &[&group(self.total())])
                };
                layer.texts.push((total, frame.at([10.0, footer + 7.0]), [0, 0, 0]));
                let (deal, cancel) = self.deal_button(1);
                let deal_images = &assets.buttons[if barter { 3 } else { usize::from(self.selling) }];
                layer.sprites.push((&deal_images[look(Hit::Deal)], frame.at(deal), BUTTON));
                layer.sprites.push((&assets.buttons[2][look(Hit::Cancel)], frame.at(cancel), BUTTON));
            }
        }
    }

    pub fn button_boxes(&self) -> Vec<(&'static str, [f32; 4])> {
        let (deal, cancel) = self.deal_button(1);
        let [deal, cancel] = [self.frames[1].at(deal), self.frames[1].at(cancel)];
        vec![("shop_deal", [deal[0], deal[1], BUTTON[0], BUTTON[1]]), ("shop_cancel", [cancel[0], cancel[1], BUTTON[0], BUTTON[1]])]
    }

    pub fn row_boxes(&self) -> Vec<(u32, [f32; 4])> {
        self.rows(0)
            .into_iter()
            .enumerate()
            .skip(self.scroll[0])
            .take(ROWS[0])
            .map(|(place, (offer, _))| {
                let top = self.frames[0].at([0.0, TITLE_HEIGHT + PADDING + (place - self.scroll[0]) as f32 * ROW]);
                (self.offers[offer].item, [top[0], top[1], WIDTH, ROW])
            })
            .collect()
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
        Offer { item, price, index: 2, have, kind, identified: true, ..Offer::default() }
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
        // A market takes no more than its stock, and what's bought leaves it.
        let mut market = Shop::new(false, vec![Offer { stock: Some(3), ..offer(1010, 200, 0, 3) }]);
        market.market = Some(MarketKind::Market);
        market.add(0, 5);
        assert_eq!(market.deal, [(0, 3)]);
        market.bought(&[(1010, 2)]);
        assert_eq!((market.offers[0].stock, market.deal.is_empty()), (Some(1), true));
    }

    #[test]
    fn sells_into_buying_store() {
        let wanted = [BuyingItem { price: 10, amount: 0, kind: 3, item_id: 501 }, BuyingItem { price: 300, amount: 5, kind: 3, item_id: 909 }];
        let inv = [
            // Worn or carded stacks are ignored.
            InventoryItem { index: 2, id: 909, amount: 5, identified: true, worn: 1, ..Default::default() },
            InventoryItem { index: 3, id: 909, amount: 5, identified: true, cards: [4001, 0, 0, 0], ..Default::default() },
            InventoryItem { index: 4, id: 909, amount: 5, identified: true, ..Default::default() },
        ];
        let mut shop = Shop::new_buying_store(2000001, 42, 1000, &wanted, &inv);
        assert_eq!(shop.offers.len(), 1);
        assert_eq!((shop.offers[0].item, shop.offers[0].index, shop.offers[0].have, shop.offers[0].stock), (909, 4, 5, Some(5)));

        // Over wanted amount (6 > 5) -> 1739 with 5.
        assert_eq!(shop.add_checked(0, 6), Some((1739, Some(5))));
        assert!(shop.deal.is_empty());

        // Within wanted amount (4 <= 5), but 4 * 300 = 1200 > 1000 zeny limit -> 1740.
        assert_eq!(shop.add_checked(0, 4), Some((1740, None)));
        assert!(shop.deal.is_empty());

        // 3 * 300 = 900 <= 1000 zeny limit.
        assert_eq!(shop.add_checked(0, 3), None);
        assert_eq!(shop.deal, [(0, 3)]);
        assert_eq!(shop.total(), 900);
    }
}
