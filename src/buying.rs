//! Buying stores: setting one up from carried items (store name, zeny limit, and wanted items with price and
//! amount) and the owner's window of what the store still wants to buy.

use rustnarok_client::net::InventoryItem;

use crate::window::Frame;

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
pub const ROW_FORMAT: usize = 1732;
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
    pub fn validate(&self, inventory: &[InventoryItem], zeny: u32) -> Result<(u32, Vec<(u32, u16, u32)>), Option<(usize, Option<u32>)>> {
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
        let limit = self.limit.parse::<u32>().unwrap_or(0);
        if limit == 0 {
            return Err(Some((NO_LIMIT, None)));
        }
        if limit > zeny {
            return Err(Some((LIMIT_OVER_ZENY, None)));
        }
        if self.name.trim().is_empty() {
            return Err(None);
        }
        let items = self.wanted.iter().map(|row| (row.item_id, row.amount, row.price)).collect();
        Ok((limit, items))
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

        setup.limit.clear();
        setup.type_text("1000");
        assert_eq!(setup.validate(&inv, 10_000), Err(None));

        setup.focus = Some(Focus::Name);
        setup.type_text("Buying Jellopy");
        assert_eq!(setup.validate(&inv, 10_000), Ok((1000, vec![(909, 5, 10)])));
    }
}
