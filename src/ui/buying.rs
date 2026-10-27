//! Buying stores: setting one up from carried items (store name, zeny limit, and wanted items with price and
//! amount) and the owner's window of what the store still wants to buy.

use std::collections::HashMap;

use rustnarok_client::lua::ItemInfo;
use rustnarok_client::messages;
use rustnarok_client::net::{BuyingItem, InventoryItem};
use rustnarok_client::text::Font;

use crate::render::Texture;
use crate::shop::group;
use crate::vend::{self, Assets};
use crate::window::{self, Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, contains};

// ponytail: a 280px window of 32px rows like vend.rs; calibrate against the real client.
pub const WIDTH: f32 = 280.0;
pub const ROW: f32 = 32.0;
pub const ROWS: usize = 6;
pub const NAME_FIELD: f32 = 24.0;
pub const LIMIT_FIELD: f32 = 24.0;
pub const FOOTER: f32 = 27.0;
pub const BUTTON: [f32; 2] = [42.0, 20.0];
pub const POSITION: [f32; 2] = [330.0, 150.0];

pub const MAX_PRICE: u32 = 99_990_000;
pub const MAX_TOTAL_AMOUNT: u32 = 9999;

pub const SETUP_TITLE: usize = 1720;
pub const LIMIT_LABEL: usize = 1723;
pub const NO_ITEM: usize = 1724;
pub const NO_PRICE: usize = 1725;
pub const PRICE_TOO_HIGH: usize = 1726;
pub const NO_AMOUNT: usize = 1727;
pub const AMOUNT_TOO_HIGH: usize = 1728;
pub const DUPLICATE_ITEM: usize = 1729;
pub const NO_LIMIT: usize = 1730;
pub const LIMIT_OVER_ZENY: usize = 1731;
pub const LIMIT_FORMAT: usize = 1735;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Focus {
    Name,
    Limit,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct WantedRow {
    pub index: u16,
    pub item_id: u32,
    pub price: u32,
    pub amount: u16,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(TitleHit),
    /// The store's name field.
    Name,
    /// The purchase zeny limit field.
    Limit,
    /// A carried item row, by place in `candidates(inventory)`.
    Row(usize),
    Ok,
    Cancel,
    Window,
}

/// What a click on the setup window asks for.
#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    /// Ask the price (and then amount) of an inventory item: `(index, item_id)`.
    Price(u16, u32),
    Open,
    Cancel,
    /// Refused, with this message in the message box (a second stack of a wanted item: 1729).
    Refused(usize),
}

/// A store's name left blank: "Please Name your Shop." (225).
pub const NO_NAME: usize = 225;

/// Opening our buying store failed (0x0812): the line to say, and whether the setup closes.
// ponytail: rathena sends 1, 2 and 8; 9 and 10 are the official client's.
pub fn open_failed(result: u16) -> Option<(usize, bool)> {
    match result {
        1 => Some((1741, false)),
        2 => Some((1742, false)),
        8 | 9 => Some((1797, result == 9)),
        10 => Some((3229, true)),
        _ => None,
    }
}

/// Our store's buying stopped (0x081a): the line, in red; the store stays open.
pub fn buyer_failed(result: u16) -> Option<usize> {
    match result {
        3 => Some(1743),
        4 => Some(1744),
        _ => None,
    }
}

/// Setting up a buying store: its name, its zeny limit, and the carried items picked with their prices and amounts.
pub struct Setup {
    pub frame: Frame,
    pub name: String,
    pub limit: String,
    pub focus: Option<Focus>,
    /// Most items the store may ask for (2 from the license item, 5 from the skill).
    pub most: usize,
    pub wanted: Vec<WantedRow>,
    pub scroll: usize,
}

/// Carried items that can be picked in the setup window (unworn, identified).
pub fn candidates(inventory: &[InventoryItem]) -> Vec<&InventoryItem> {
    inventory.iter().filter(|item| item.worn == 0 && item.identified).collect()
}

pub type Validated = Result<(u32, Vec<(u32, u16, u32)>), Option<(usize, Option<u32>)>>;

impl Setup {
    pub fn new(slots: u8) -> Self {
        let mut frame = Frame::new(POSITION, WIDTH);
        frame.open = true;
        Self { frame, name: String::new(), limit: String::new(), focus: Some(Focus::Name), most: usize::from(slots), wanted: Vec::new(), scroll: 0 }
    }

    pub fn set_price(&mut self, index: u16, item_id: u32, price: u32) {
        if let Some(row) = self.wanted.iter_mut().find(|row| row.index == index) {
            row.price = price;
        } else if self.wanted.len() < self.most {
            self.wanted.push(WantedRow { index, item_id, price, amount: 0 });
        }
    }

    pub fn set_amount(&mut self, index: u16, amount: u16) {
        if let Some(row) = self.wanted.iter_mut().find(|row| row.index == index) {
            row.amount = amount;
        }
    }

    pub fn type_text(&mut self, text: &str) {
        match self.focus {
            Some(Focus::Name) => {
                let room = 80usize.saturating_sub(self.name.len());
                self.name.extend(text.chars().filter(|character| !character.is_control()).take(room));
            }
            Some(Focus::Limit) => {
                let room = 10usize.saturating_sub(self.limit.len());
                self.limit.extend(text.chars().filter(char::is_ascii_digit).take(room));
            }
            None => {}
        }
    }

    pub fn backspace(&mut self) {
        match self.focus {
            Some(Focus::Name) => {
                self.name.pop();
            }
            Some(Focus::Limit) => {
                self.limit.pop();
            }
            None => {}
        }
    }

    /// Checks the setup before sending 0x0811: returns `Ok((limit, items))` where each item is `(item_id, amount, price)`,
    /// `Err(None)` when the store name is blank (don't send), or `Err(Some((message_id, item_id)))` on a validation error.
    pub fn validate(&self, inventory: &[InventoryItem], zeny: u32) -> Validated {
        if self.wanted.is_empty() {
            return Err(Some((NO_ITEM, None)));
        }
        for row in &self.wanted {
            if row.price == 0 {
                return Err(Some((NO_PRICE, Some(row.item_id))));
            }
            if row.price > MAX_PRICE {
                return Err(Some((PRICE_TOO_HIGH, Some(row.item_id))));
            }
            if row.amount == 0 {
                return Err(Some((NO_AMOUNT, Some(row.item_id))));
            }
            let carried: u32 = inventory.iter().filter(|item| item.id == row.item_id).map(|item| u32::from(item.amount)).sum();
            if carried + u32::from(row.amount) > MAX_TOTAL_AMOUNT {
                return Err(Some((AMOUNT_TOO_HIGH, Some(row.item_id))));
            }
        }
        for (i, row) in self.wanted.iter().enumerate() {
            if self.wanted[..i].iter().any(|earlier| earlier.item_id == row.item_id) {
                return Err(Some((DUPLICATE_ITEM, None)));
            }
        }
        let limit = self.limit.parse::<u64>().unwrap_or(0);
        if limit == 0 {
            return Err(Some((NO_LIMIT, None)));
        }
        if limit > u64::from(zeny) {
            return Err(Some((LIMIT_OVER_ZENY, None)));
        }
        if self.name.trim().is_empty() {
            return Err(None);
        }
        let items = self.wanted.iter().map(|row| (row.item_id, row.amount, row.price)).collect();
        Ok((limit as u32, items))
    }

    pub fn height() -> f32 {
        TITLE_HEIGHT + NAME_FIELD + LIMIT_FIELD + ROWS as f32 * ROW + FOOTER
    }

    pub fn buttons() -> [[f32; 2]; 2] {
        let top = Self::height() - FOOTER + 4.0;
        [[WIDTH - 110.0, top], [WIDTH - 60.0, top]]
    }

    pub fn hit(&self, cursor: [f32; 2], inventory: &[InventoryItem]) -> Option<Hit> {
        let list = candidates(inventory);
        let local = self.frame.local(cursor, Self::height())?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(Hit::Title(hit));
        }
        let [ok, cancel] = Self::buttons();
        if contains(ok, BUTTON, local) {
            return Some(Hit::Ok);
        }
        if contains(cancel, BUTTON, local) {
            return Some(Hit::Cancel);
        }
        if local[1] < TITLE_HEIGHT + NAME_FIELD {
            return Some(Hit::Name);
        }
        if local[1] < TITLE_HEIGHT + NAME_FIELD + LIMIT_FIELD {
            return Some(Hit::Limit);
        }
        let row = ((local[1] - TITLE_HEIGHT - NAME_FIELD - LIMIT_FIELD) / ROW) as usize + self.scroll;
        Some(if local[1] < Self::height() - FOOTER && row < list.len() { Hit::Row(row) } else { Hit::Window })
    }

    pub fn click(&mut self, hit: Hit, inventory: &[InventoryItem]) -> Option<Action> {
        self.focus = match hit {
            Hit::Name => Some(Focus::Name),
            Hit::Limit => Some(Focus::Limit),
            _ => None,
        };
        match hit {
            Hit::Title(TitleHit::Close) | Hit::Cancel => Some(Action::Cancel),
            Hit::Title(title) => {
                self.frame.click_title(title);
                None
            }
            Hit::Row(row) => {
                let item = *candidates(inventory).get(row)?;
                if let Some(pos) = self.wanted.iter().position(|wanted| wanted.index == item.index) {
                    self.wanted.remove(pos);
                    None
                } else if self.wanted.iter().any(|wanted| wanted.item_id == item.id) {
                    Some(Action::Refused(DUPLICATE_ITEM))
                } else if self.wanted.len() < self.most {
                    Some(Action::Price(item.index, item.id))
                } else {
                    None
                }
            }
            Hit::Ok => Some(Action::Open),
            _ => None,
        }
    }

    pub fn scroll_by(&mut self, rows: i32, inventory: &[InventoryItem]) {
        let count = candidates(inventory).len();
        self.scroll = (self.scroll as i32 + rows).clamp(0, count.saturating_sub(ROWS) as i32) as usize;
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &self,
        layer: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        icons: &'a HashMap<String, Option<Texture>>,
        items: &HashMap<u32, ItemInfo>,
        naming: crate::inventory::Naming,
        inventory: &[InventoryItem],
        font: &Font,
        messages: &[String],
        pointer: ([f32; 2], bool),
        caret: bool,
    ) {
        let at = |local: [f32; 2]| self.frame.at(local);
        self.frame.draw_title(layer, chrome, messages.get(SETUP_TITLE).map_or("", String::as_str), pointer.0);
        if self.frame.minimized {
            return;
        }
        let height = Self::height();
        layer.sprites.push((&chrome.white, at([0.0, TITLE_HEIGHT]), [WIDTH, height - TITLE_HEIGHT - FOOTER]));
        // Store name field.
        layer.sprites.push((&chrome.border, at([5.0, TITLE_HEIGHT + 3.0]), [WIDTH - 10.0, 18.0]));
        layer.sprites.push((&chrome.white, at([6.0, TITLE_HEIGHT + 4.0]), [WIDTH - 12.0, 16.0]));
        let name = if self.focus == Some(Focus::Name) && caret { format!("{}|", self.name) } else { self.name.clone() };
        layer.texts.push((window::fit(font, &name, WIDTH - 20.0), at([9.0, TITLE_HEIGHT + 5.0]), [0, 0, 0]));
        // Zeny limit label and field.
        let limit_top = TITLE_HEIGHT + NAME_FIELD;
        let label = messages.get(LIMIT_LABEL).map_or("Purchase Zeny Limit", String::as_str);
        layer.texts.push((window::fit(font, label, 130.0), at([6.0, limit_top + 5.0]), [0, 0, 0]));
        layer.sprites.push((&chrome.border, at([140.0, limit_top + 3.0]), [WIDTH - 145.0, 18.0]));
        layer.sprites.push((&chrome.white, at([141.0, limit_top + 4.0]), [WIDTH - 147.0, 16.0]));
        let limit = if self.focus == Some(Focus::Limit) && caret { format!("{}|", self.limit) } else { self.limit.clone() };
        layer.texts.push((window::fit(font, &limit, WIDTH - 155.0), at([144.0, limit_top + 5.0]), [0, 0, 0]));
        // Candidate items, picked ones highlighted with amount x price.
        let list = candidates(inventory);
        for (place, item) in list.iter().enumerate().skip(self.scroll).take(ROWS) {
            let top = TITLE_HEIGHT + NAME_FIELD + LIMIT_FIELD + (place - self.scroll) as f32 * ROW;
            let picked = self.wanted.iter().find(|wanted| wanted.index == item.index);
            if picked.is_some() {
                layer.sprites.push((&assets.picked, at([1.0, top]), [WIDTH - 2.0, ROW]));
            }
            vend::draw_item(layer, icons, items, naming, messages, font, item, at([4.0, top + 4.0]));
            if let Some(wanted) = picked {
                let text = format!("{} x {} z", wanted.amount, group(u64::from(wanted.price)));
                layer.texts.push((text.clone(), at([WIDTH - 6.0 - font.width(&text), top + 10.0]), [0, 0, 0]));
            }
        }
        let hit = self.hit(pointer.0, inventory);
        layer.sprites.push((&assets.footer, at([0.0, height - FOOTER]), [WIDTH, FOOTER]));
        let [ok, cancel] = Self::buttons();
        for (images, place, target) in [(&assets.ok, ok, Hit::Ok), (&assets.cancel, cancel, Hit::Cancel)] {
            let look = if hit == Some(target) { 1 + usize::from(pointer.1) } else { 0 };
            layer.sprites.push((&images[look], at(place), BUTTON));
        }
    }

    pub fn button_boxes(&self) -> Vec<(&'static str, [f32; 4])> {
        let at = |local: [f32; 2]| self.frame.at(local);
        let [ok, cancel] = Self::buttons();
        let [ok, cancel] = [at(ok), at(cancel)];
        let name = at([5.0, TITLE_HEIGHT + 3.0]);
        let limit = at([140.0, TITLE_HEIGHT + NAME_FIELD + 3.0]);
        vec![
            ("buying_ok", [ok[0], ok[1], BUTTON[0], BUTTON[1]]),
            ("buying_cancel", [cancel[0], cancel[1], BUTTON[0], BUTTON[1]]),
            ("buying_name", [name[0], name[1], WIDTH - 10.0, 18.0]),
            ("buying_limit", [limit[0], limit[1], WIDTH - 145.0, 18.0]),
        ]
    }

    pub fn row_boxes(&self, inventory: &[InventoryItem]) -> Vec<(u32, [f32; 4])> {
        candidates(inventory)
            .into_iter()
            .enumerate()
            .skip(self.scroll)
            .take(ROWS)
            .map(|(place, item)| {
                let top = self.frame.at([0.0, TITLE_HEIGHT + NAME_FIELD + LIMIT_FIELD + (place - self.scroll) as f32 * ROW]);
                (item.id, [top[0], top[1], WIDTH, ROW])
            })
            .collect()
    }
}

/// The owner's active buying store window: what it still wants to buy, its remaining zeny limit, and its close button.
pub struct MyStore {
    pub frame: Frame,
    pub name: String,
    pub limit: u32,
    pub items: Vec<BuyingItem>,
}

impl MyStore {
    pub fn new(name: String, limit: u32, items: Vec<BuyingItem>) -> Self {
        let mut frame = Frame::new(POSITION, WIDTH);
        frame.open = true;
        Self { frame, name, limit, items }
    }

    pub fn height(&self) -> f32 {
        TITLE_HEIGHT + LIMIT_FIELD + ROWS as f32 * ROW + FOOTER
    }

    pub fn close_button(&self) -> [f32; 2] {
        [WIDTH - 60.0, self.height() - FOOTER + 4.0]
    }

    /// Over the window: true on its close button.
    pub fn hit(&self, cursor: [f32; 2]) -> Option<bool> {
        let local = self.frame.local(cursor, self.height())?;
        Some(contains(self.close_button(), BUTTON, local) || self.frame.title_hit(local) == Some(TitleHit::Close))
    }

    /// Someone sold to our buying store (0x09E6): lower that item's wanted amount (drop the row at 0) and update the limit.
    pub fn bought(&mut self, item_id: u32, amount: u16, limit_left: u32) {
        self.limit = limit_left;
        if let Some(position) = self.items.iter().position(|item| item.item_id == item_id) {
            let left = &mut self.items[position].amount;
            *left = left.saturating_sub(amount);
            if *left == 0 {
                self.items.remove(position);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &self,
        layer: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        icons: &'a HashMap<String, Option<Texture>>,
        items: &HashMap<u32, ItemInfo>,
        naming: crate::inventory::Naming,
        font: &Font,
        messages: &[String],
        pointer: ([f32; 2], bool),
    ) {
        let at = |local: [f32; 2]| self.frame.at(local);
        let title = format!("{} : {}", messages.get(SETUP_TITLE).map_or("", String::as_str), self.name);
        self.frame.draw_title(layer, chrome, &title, pointer.0);
        let height = self.height();
        layer.sprites.push((&chrome.white, at([0.0, TITLE_HEIGHT]), [WIDTH, height - TITLE_HEIGHT - FOOTER]));
        let limit_line = messages::fill(messages.get(LIMIT_FORMAT).map_or("Price limit: %s Zeny", String::as_str), &[&group(u64::from(self.limit))]);
        layer.texts.push((window::fit(font, &limit_line, WIDTH - 12.0), at([6.0, TITLE_HEIGHT + 5.0]), [0, 0, 0]));
        for (place, wanted) in self.items.iter().enumerate().take(ROWS) {
            let top = TITLE_HEIGHT + LIMIT_FIELD + place as f32 * ROW;
            let display = InventoryItem { id: wanted.item_id, amount: wanted.amount, kind: wanted.kind, identified: true, ..Default::default() };
            vend::draw_item(layer, icons, items, naming, messages, font, &display, at([4.0, top + 4.0]));
            let text = format!("{} z", group(u64::from(wanted.price)));
            layer.texts.push((text.clone(), at([WIDTH - 6.0 - font.width(&text), top + 10.0]), [0, 0, 0]));
        }
        layer.sprites.push((&assets.footer, at([0.0, height - FOOTER]), [WIDTH, FOOTER]));
        let look = if self.hit(pointer.0) == Some(true) { 1 + usize::from(pointer.1) } else { 0 };
        layer.sprites.push((&assets.close[look], at(self.close_button()), BUTTON));
    }

    pub fn button_boxes(&self) -> Vec<(&'static str, [f32; 4])> {
        let close = self.frame.at(self.close_button());
        vec![("buying_close", [close[0], close[1], BUTTON[0], BUTTON[1]])]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(index: u16, id: u32, amount: u16) -> InventoryItem {
        InventoryItem { index, id, amount, identified: true, ..Default::default() }
    }

    #[test]
    fn validates_buying_store_setup() {
        let inv = [item(2, 909, 10), item(3, 909, 1), item(4, 512, 1)];
        let mut setup = Setup::new(5);

        assert_eq!(setup.validate(&inv, 10_000), Err(Some((1724, None))));

        setup.set_price(2, 909, 0);
        assert_eq!(setup.validate(&inv, 10_000), Err(Some((1725, Some(909)))));

        setup.set_price(2, 909, 99_990_001);
        assert_eq!(setup.validate(&inv, 10_000), Err(Some((1726, Some(909)))));

        setup.set_price(2, 909, 10);
        assert_eq!(setup.validate(&inv, 10_000), Err(Some((1727, Some(909)))));

        // Carried Jellopy (909) across stacks is 10 + 1 = 11; 11 + 9989 = 10000 > 9999.
        setup.set_amount(2, 9989);
        assert_eq!(setup.validate(&inv, 10_000), Err(Some((1728, Some(909)))));

        setup.set_amount(2, 5);
        setup.set_price(3, 909, 10);
        setup.set_amount(3, 1);
        assert_eq!(setup.validate(&inv, 10_000), Err(Some((1729, None))));

        setup.wanted.pop();
        assert_eq!(setup.validate(&inv, 10_000), Err(Some((1730, None))));

        setup.focus = Some(Focus::Limit);
        setup.type_text("20000");
        assert_eq!(setup.validate(&inv, 10_000), Err(Some((1731, None))));

        // Past u32::MAX is still more than carried, not "no limit".
        setup.limit.clear();
        setup.type_text("9999999999");
        assert_eq!(setup.validate(&inv, u32::MAX), Err(Some((1731, None))));

        setup.limit.clear();
        setup.type_text("1000");
        assert_eq!(setup.validate(&inv, 10_000), Err(None));

        setup.focus = Some(Focus::Name);
        setup.type_text("Buying Jellopy");
        assert_eq!(setup.validate(&inv, 10_000), Ok((1000, vec![(909, 5, 10)])));
    }

    #[test]
    fn picks_and_unpicks_candidates() {
        let inv = [item(2, 909, 5), item(3, 512, 1)];
        let mut setup = Setup::new(1);
        assert_eq!(setup.click(Hit::Row(0), &inv), Some(Action::Price(2, 909)));
        setup.set_price(2, 909, 10);
        setup.set_amount(2, 5);
        // Slot limit reached (1): clicking another row does nothing.
        assert_eq!(setup.click(Hit::Row(1), &inv), None);
        // Clicking the picked row unpicks it.
        assert_eq!(setup.click(Hit::Row(0), &inv), None);
        assert!(setup.wanted.is_empty());
        // A second stack of a wanted item is refused when picked (1729).
        let inv = [item(2, 909, 5), item(3, 909, 1)];
        let mut setup = Setup::new(5);
        setup.set_price(2, 909, 10);
        assert_eq!(setup.click(Hit::Row(1), &inv), Some(Action::Refused(DUPLICATE_ITEM)));
        assert_eq!(setup.wanted.len(), 1);
    }

    #[test]
    fn failures_say_their_line_and_close_the_setup_or_keep_the_store() {
        assert_eq!(open_failed(1), Some((1741, false)));
        assert_eq!(open_failed(8), Some((1797, false)));
        assert_eq!(open_failed(9), Some((1797, true)));
        assert_eq!(open_failed(10), Some((3229, true)));
        assert_eq!(open_failed(5), None);
        assert_eq!((buyer_failed(3), buyer_failed(4), buyer_failed(1)), (Some(1743), Some(1744), None));
    }

    #[test]
    fn buys_items_down_in_my_store() {
        let mut store = MyStore::new(
            "Buying Jellopy".into(),
            1000,
            vec![BuyingItem { price: 10, amount: 5, kind: 3, item_id: 909 }, BuyingItem { price: 20, amount: 2, kind: 0, item_id: 512 }],
        );
        store.bought(909, 2, 980);
        assert_eq!(store.limit, 980);
        assert_eq!(store.items[0].amount, 3);
        store.bought(909, 3, 950);
        assert_eq!(store.limit, 950);
        assert_eq!(store.items, [BuyingItem { price: 20, amount: 2, kind: 0, item_id: 512 }]);
    }
}
