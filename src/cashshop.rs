//! The cash shop: its category tabs, goods, points and the list being bought.

use std::collections::HashMap;

use rustnarok_client::grf::Grf;
use rustnarok_client::lua::ItemInfo;
use rustnarok_client::text::Font;

use crate::inventory;
use crate::render::{Gpu, Texture};
use crate::shop::group;
use crate::window::{Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI, contains, offset};

// ponytail: the cash shop layout is estimated from its bitmaps; calibrate it against the real client.
const WIDTH: f32 = 700.0;
const HEIGHT: f32 = 430.0;
const POSITION: [f32; 2] = [162.0, 90.0];
const TABS: [f32; 2] = [10.0, TITLE_HEIGHT + 8.0];
const TAB_SIZE: [f32; 2] = [74.0, 24.0];
const GOODS: [f32; 2] = [12.0, TITLE_HEIGHT + 42.0];
const GOOD_SIZE: [f32; 2] = [137.0, 62.0];
const GOOD_COLUMNS: usize = 3;
const GOOD_ROWS: usize = 5;
const CART: [f32; 2] = [438.0, TITLE_HEIGHT + 42.0];
const CART_SIZE: [f32; 2] = [250.0, 238.0];
const CART_ROW: f32 = 38.0;
const CART_ROWS: usize = 5;
const FREE_FIELD: [f32; 2] = [520.0, 337.0];
const FREE_SIZE: [f32; 2] = [120.0, 18.0];
const BUY: [f32; 2] = [548.0, 384.0];
const CLOSE: [f32; 2] = [642.0, 384.0];
const BUTTON_SIZE: [f32; 2] = [42.0, 20.0];
const TITLE_MESSAGE: usize = 3288;
const CART_MESSAGE: usize = 3289;
const CASH_MESSAGE: usize = 1858;
const FREE_MESSAGE: usize = 1857;
const FREE_SPEND_MESSAGE: usize = 1306;
const TAB_MESSAGES: [usize; 9] = [3290, 3291, 3292, 3293, 3294, 3295, 3296, 3297, 3298];
const GREY: [u8; 3] = [0x88, 0x88, 0x88];

pub struct Assets {
    background: Option<Texture>,
    tab: Option<Texture>,
    tab_on: Option<Texture>,
    buy: [Option<Texture>; 3],
    close: [Option<Texture>; 3],
    fill: Texture,
    selected: Texture,
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |name: &str| gpu.try_load_texture(grf, &format!("{UI}cashshop\\{name}.bmp"), false).ok();
        Self {
            background: image("cashshop_bg"),
            tab: image("tab"),
            tab_on: image("tab_on"),
            buy: ["normal", "over", "press"].map(|look| image(&format!("btn_buy_{look}"))),
            close: ["normal", "over", "press"].map(|look| image(&format!("btn_close_{look}"))),
            fill: gpu.solid([0xF5, 0xF5, 0xF5, 0xFF]),
            selected: gpu.solid([0xD5, 0xE5, 0xFF, 0xFF]),
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(TitleHit),
    Tab(usize),
    Item(usize),
    CartAmount(usize, bool),
    FreeSpend,
    Buy,
    Close,
    Window,
}

#[derive(Debug, PartialEq)]
pub enum Action {
    Buy,
    Close,
}

pub type BuyRequest = (u32, Vec<(u32, u32, u16)>);

pub struct CashShop {
    pub cash_points: u32,
    pub free_points: u32,
    pub tabs: Vec<Vec<(u32, u32)>>,
    pub cart: Vec<(u32, u32, u16, u32)>,
    pub free_to_spend: u32,
    pub frame: Frame,
    pub selected_tab: usize,
    pub goods_scroll: usize,
    pub cart_scroll: usize,
    pub typing: bool,
    typed_free: String,
}

impl CashShop {
    pub fn new(cash_points: u32, free_points: u32) -> Self {
        let mut frame = Frame::new(POSITION, WIDTH);
        frame.open = true;
        Self {
            cash_points,
            free_points,
            tabs: vec![Vec::new(); 9],
            cart: Vec::new(),
            free_to_spend: 0,
            frame,
            selected_tab: 0,
            goods_scroll: 0,
            cart_scroll: 0,
            typing: false,
            typed_free: String::new(),
        }
    }

    pub fn add_to_cart(&mut self, item: u32, amount: u32, tab: u16, price: u32) -> bool {
        if !(1..=99).contains(&amount) {
            return false;
        }
        if let Some(line) = self.cart.iter_mut().find(|line| line.0 == item && line.2 == tab) {
            *line = (item, amount, tab, price);
        } else {
            self.cart.push((item, amount, tab, price));
        }
        self.cap_free_spend();
        true
    }

    pub fn set_amount(&mut self, item: u32, tab: u16, amount: u32) -> bool {
        if !(1..=99).contains(&amount) {
            return false;
        }
        let Some(line) = self.cart.iter_mut().find(|line| line.0 == item && line.2 == tab) else { return false };
        line.1 = amount;
        self.cap_free_spend();
        true
    }

    pub fn remove(&mut self, item: u32, tab: u16) -> bool {
        let Some(position) = self.cart.iter().position(|line| line.0 == item && line.2 == tab) else { return false };
        self.cart.remove(position);
        self.cap_free_spend();
        true
    }

    pub fn cart_total(&self) -> u32 {
        self.cart.iter().fold(0, |total, &(_, amount, _, price)| total.saturating_add(price.saturating_mul(amount)))
    }

    pub fn set_free_spend(&mut self, amount: u32) {
        self.free_to_spend = amount.min(self.free_points).min(self.cart_total());
        self.typed_free = self.free_to_spend.to_string();
    }

    fn cap_free_spend(&mut self) {
        self.set_free_spend(self.free_to_spend);
    }

    pub fn buy_request(&self, confirmed: bool) -> Option<BuyRequest> {
        (confirmed && !self.cart.is_empty()).then(|| (self.free_to_spend, self.cart.iter().map(|&(item, amount, tab, _)| (item, amount, tab)).collect()))
    }

    pub fn clear_cart_on_success(&mut self, result: u16, cash: u32, free: u32) {
        if result == 0 {
            self.cash_points = cash;
            self.free_points = free;
            self.cart.clear();
            self.set_free_spend(0);
        }
    }

    pub fn append(&mut self, tab: i16, list: Vec<(u32, u32)>) {
        if let Some(items) = usize::try_from(tab).ok().and_then(|tab| self.tabs.get_mut(tab)) {
            items.extend(list);
        }
    }

    pub fn select_tab(&mut self, tab: usize) -> bool {
        if self.tabs.get(tab).is_none_or(Vec::is_empty) {
            return false;
        }
        self.selected_tab = tab;
        true
    }

    pub fn scroll_goods(&mut self, _rows: i32) {}

    pub fn scroll_cart(&mut self, _lines: i32) {}

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let local = self.frame.local(cursor, HEIGHT)?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(Hit::Title(hit));
        }
        for tab in 0..self.tabs.len() {
            if contains(offset(TABS, [tab as f32 * TAB_SIZE[0], 0.0]), TAB_SIZE, local) {
                return Some(Hit::Tab(tab));
            }
        }
        for cell in 0..GOOD_ROWS * GOOD_COLUMNS {
            let place = offset(GOODS, [(cell % GOOD_COLUMNS) as f32 * GOOD_SIZE[0], (cell / GOOD_COLUMNS) as f32 * GOOD_SIZE[1]]);
            if contains(place, GOOD_SIZE, local) && cell < self.tabs[self.selected_tab].len() {
                return Some(Hit::Item(cell));
            }
        }
        for row in 0..self.cart.len().min(CART_ROWS) {
            let y = CART[1] + 20.0 + row as f32 * CART_ROW;
            if contains([CART[0] + CART_SIZE[0] - 42.0, y], [18.0, 18.0], local) {
                return Some(Hit::CartAmount(row, false));
            }
            if contains([CART[0] + CART_SIZE[0] - 21.0, y], [18.0, 18.0], local) {
                return Some(Hit::CartAmount(row, true));
            }
        }
        Some(if contains(FREE_FIELD, FREE_SIZE, local) {
            Hit::FreeSpend
        } else if contains(BUY, BUTTON_SIZE, local) {
            Hit::Buy
        } else if contains(CLOSE, BUTTON_SIZE, local) {
            Hit::Close
        } else {
            Hit::Window
        })
    }

    pub fn click(&mut self, hit: Hit) -> Option<Action> {
        self.typing = hit == Hit::FreeSpend;
        match hit {
            Hit::Title(TitleHit::Close) | Hit::Close => return Some(Action::Close),
            Hit::Title(title) => self.frame.click_title(title),
            Hit::Tab(tab) => _ = self.select_tab(tab),
            Hit::Item(cell) => {
                if let Some(&(item, price)) = self.tabs[self.selected_tab].get(cell) {
                    let amount =
                        self.cart.iter().find(|line| line.0 == item && usize::from(line.2) == self.selected_tab).map_or(1, |line| line.1.saturating_add(1));
                    self.add_to_cart(item, amount.min(99), self.selected_tab as u16, price);
                }
            }
            Hit::CartAmount(row, more) => {
                if let Some(&(item, amount, tab, _)) = self.cart.get(row) {
                    if more {
                        self.set_amount(item, tab, amount.saturating_add(1).min(99));
                    } else if amount == 1 {
                        self.remove(item, tab);
                    } else {
                        self.set_amount(item, tab, amount - 1);
                    }
                }
            }
            Hit::Buy if !self.cart.is_empty() => return Some(Action::Buy),
            Hit::FreeSpend | Hit::Buy | Hit::Window => {}
        }
        None
    }

    pub fn type_text(&mut self, text: &str) {
        let room = 10usize.saturating_sub(self.typed_free.len());
        let more: String = text.chars().filter(char::is_ascii_digit).take(room).collect();
        self.typed_free.push_str(&more);
        let amount = self.typed_free.parse().unwrap_or(0);
        self.set_free_spend(amount);
    }

    pub fn backspace(&mut self) {
        self.typed_free.pop();
        let amount = self.typed_free.parse().unwrap_or(0);
        self.set_free_spend(amount);
    }

    pub fn icons(&self, items: &HashMap<u32, ItemInfo>) -> Vec<String> {
        self.tabs.iter().flatten().filter_map(|(id, _)| items.get(id).map(|info| inventory::icon_path(info, true))).collect()
    }

    pub fn item_boxes(&self) -> Vec<(u32, [f32; 4])> {
        self.tabs[self.selected_tab]
            .iter()
            .take(GOOD_ROWS * GOOD_COLUMNS)
            .enumerate()
            .map(|(cell, &(item, _))| {
                let place = self.frame.at(offset(GOODS, [(cell % GOOD_COLUMNS) as f32 * GOOD_SIZE[0], (cell / GOOD_COLUMNS) as f32 * GOOD_SIZE[1]]));
                (item, [place[0], place[1], GOOD_SIZE[0], GOOD_SIZE[1]])
            })
            .collect()
    }

    pub fn button_boxes(&self) -> [(&'static str, [f32; 4]); 2] {
        let buy = self.frame.at(BUY);
        let close = self.frame.at(CLOSE);
        [("cashshop_buy", [buy[0], buy[1], BUTTON_SIZE[0], BUTTON_SIZE[1]]), ("cashshop_close", [close[0], close[1], BUTTON_SIZE[0], BUTTON_SIZE[1]])]
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
        caret: bool,
    ) {
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        let at = |local| self.frame.at(local);
        self.frame.draw_title(layer, chrome, message(TITLE_MESSAGE), pointer.0);
        if self.frame.minimized {
            return;
        }
        match &assets.background {
            Some(background) => layer.sprites.push((background, at([0.0, TITLE_HEIGHT]), [WIDTH, HEIGHT - TITLE_HEIGHT])),
            None => layer.sprites.push((&assets.fill, at([0.0, TITLE_HEIGHT]), [WIDTH, HEIGHT - TITLE_HEIGHT])),
        }
        let hit = self.hit(pointer.0);
        for (tab, &message_id) in TAB_MESSAGES.iter().enumerate().take(self.tabs.len()) {
            let place = offset(TABS, [tab as f32 * TAB_SIZE[0], 0.0]);
            if let Some(texture) = if tab == self.selected_tab { assets.tab_on.as_ref() } else { assets.tab.as_ref() } {
                layer.sprites.push((texture, at(place), TAB_SIZE));
            }
            let label = message(message_id);
            let color = if self.tabs[tab].is_empty() { GREY } else { [0, 0, 0] };
            layer.texts.push((label.to_owned(), at(offset(place, [(TAB_SIZE[0] - font.width(label)) / 2.0, 5.0])), color));
        }
        for cell in 0..GOOD_ROWS * GOOD_COLUMNS {
            let place = offset(GOODS, [(cell % GOOD_COLUMNS) as f32 * GOOD_SIZE[0], (cell / GOOD_COLUMNS) as f32 * GOOD_SIZE[1]]);
            let fill = if hit == Some(Hit::Item(cell)) { &assets.selected } else { &assets.fill };
            layer.sprites.push((fill, at(place), offset(GOOD_SIZE, [-2.0, -2.0])));
            let Some(&(item, price)) = self.tabs[self.selected_tab].get(cell) else { continue };
            if let Some(Some(icon)) = items.get(&item).and_then(|info| icons.get(&inventory::icon_path(info, true))) {
                layer.sprites.push((icon, at(offset(place, [4.0, 14.0])), icon.size));
            }
            let name = inventory::item_name(items, item, true, 0);
            layer.texts.push((crate::window::fit(font, &name, GOOD_SIZE[0] - 38.0), at(offset(place, [36.0, 12.0])), [0, 0, 0]));
            layer.texts.push((format!("{} C", group(u64::from(price))), at(offset(place, [36.0, 32.0])), [0x40, 0x40, 0x90]));
        }
        layer.sprites.push((&assets.fill, at(CART), CART_SIZE));
        layer.texts.push((message(CART_MESSAGE).to_owned(), at(offset(CART, [5.0, 3.0])), [0, 0, 0]));
        for (row, &(item, amount, _, price)) in self.cart.iter().take(CART_ROWS).enumerate() {
            let place = offset(CART, [5.0, 20.0 + row as f32 * CART_ROW]);
            let name = inventory::item_name(items, item, true, 0);
            layer.texts.push((crate::window::fit(font, &name, 140.0), at(place), [0, 0, 0]));
            layer.texts.push((format!("{} C  x{amount}", group(u64::from(price))), at(offset(place, [0.0, 17.0])), [0x40, 0x40, 0x90]));
            layer.texts.push(("−".into(), at(offset(place, [CART_SIZE[0] - 47.0, 2.0])), [0, 0, 0]));
            layer.texts.push(("+".into(), at(offset(place, [CART_SIZE[0] - 26.0, 2.0])), [0, 0, 0]));
        }
        for (id, value, place) in [(CASH_MESSAGE, self.cash_points, [438.0, 292.0]), (FREE_MESSAGE, self.free_points, [438.0, 314.0])] {
            layer.texts.push((format!("{}: {}", message(id), group(u64::from(value))), at(place), [0, 0, 0]));
        }
        layer.texts.push((message(FREE_SPEND_MESSAGE).to_owned(), at([438.0, 340.0]), [0, 0, 0]));
        layer.sprites.push((&chrome.border, at(offset(FREE_FIELD, [-1.0, -1.0])), offset(FREE_SIZE, [2.0, 2.0])));
        layer.sprites.push((&assets.fill, at(FREE_FIELD), FREE_SIZE));
        let typed = if self.typing && caret { format!("{}|", self.typed_free) } else { self.typed_free.clone() };
        layer.texts.push((typed, at(offset(FREE_FIELD, [3.0, 3.0])), [0, 0, 0]));
        let look = |target| usize::from(hit == Some(target)) + usize::from(hit == Some(target) && pointer.1);
        for (target, place, images, fallback) in [(Hit::Buy, BUY, &assets.buy, "Buy"), (Hit::Close, CLOSE, &assets.close, "Close")] {
            if let Some(image) = &images[look(target)] {
                layer.sprites.push((image, at(place), BUTTON_SIZE));
            } else {
                layer.sprites.push((&chrome.border, at(place), BUTTON_SIZE));
                layer.texts.push((fallback.into(), at(offset(place, [5.0, 4.0])), [0, 0, 0]));
            }
        }
        layer.texts.push((format!("Total: {} C", group(u64::from(self.cart_total()))), at([438.0, 365.0]), [0, 0, 0]));
    }
}

pub fn buy_result_message(result: u16) -> usize {
    // ponytail: these messages follow the server reasons until every result is observed.
    [2530, 1228, 1254, 1230, 2536, 3568, 1229, 1940, 1814, 1944, 1945, 1946, 1947].get(usize::from(result)).copied().unwrap_or(1228)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cart_accepts_amounts_one_through_ninety_nine() {
        let mut shop = CashShop::new(100, 50);
        assert_eq!(shop.tabs.len(), 9);
        assert!(!shop.add_to_cart(501, 0, 0, 10));
        assert!(!shop.add_to_cart(501, 100, 0, 10));
        assert!(shop.add_to_cart(501, 1, 0, 10));
        assert!(shop.add_to_cart(502, 99, 0, 20));
    }

    #[test]
    fn cart_updates_and_removes_an_item_in_a_tab() {
        let mut shop = CashShop::new(100, 50);
        shop.cart.push((501, 1, 3, 10));
        assert!(shop.add_to_cart(501, 7, 3, 10));
        assert_eq!(shop.cart, [(501, 7, 3, 10)]);
        assert!(shop.set_amount(501, 3, 99));
        assert_eq!(shop.cart[0].1, 99);
        assert!(!shop.set_amount(501, 3, 0));
        assert!(!shop.set_amount(501, 3, 100));
        assert!(shop.remove(501, 3));
        assert!(shop.cart.is_empty());
    }

    #[test]
    fn cart_totals_price_times_amount() {
        let mut shop = CashShop::new(100, 50);
        shop.cart = vec![(501, 3, 0, 10), (1201, 2, 4, 125)];
        assert_eq!(shop.cart_total(), 280);
    }

    #[test]
    fn free_spend_is_capped_by_points_and_cart_total() {
        let mut shop = CashShop::new(100, 50);
        shop.cart.push((501, 3, 0, 10));
        shop.set_free_spend(40);
        assert_eq!(shop.free_to_spend, 30);
        shop.cart[0].1 = 10;
        shop.set_free_spend(75);
        assert_eq!(shop.free_to_spend, 50);
    }

    #[test]
    fn buy_request_needs_a_cart_and_confirmation() {
        let mut shop = CashShop::new(100, 50);
        assert_eq!(shop.buy_request(true), None);
        shop.cart.push((501, 2, 3, 10));
        shop.free_to_spend = 15;
        assert_eq!(shop.buy_request(false), None);
        assert_eq!(shop.buy_request(true), Some((15, vec![(501, 2, 3)])));
    }

    #[test]
    fn successful_buy_clears_cart_and_updates_points() {
        let mut shop = CashShop::new(100, 50);
        shop.cart.push((501, 2, 3, 10));
        shop.free_to_spend = 15;
        shop.clear_cart_on_success(1, 90, 40);
        assert_eq!((shop.cash_points, shop.free_points, shop.cart.len()), (100, 50, 1));
        shop.clear_cart_on_success(0, 90, 40);
        assert_eq!((shop.cash_points, shop.free_points, shop.free_to_spend), (90, 40, 0));
        assert!(shop.cart.is_empty());
    }

    #[test]
    fn maps_buy_results_to_messages() {
        // ponytail: result messages follow the issue table until every server reason is observed.
        let messages = (0..=12).map(buy_result_message).collect::<Vec<_>>();
        assert_eq!(messages, [2530, 1228, 1254, 1230, 2536, 3568, 1229, 1940, 1814, 1944, 1945, 1946, 1947]);
    }

    #[test]
    fn empty_tabs_are_disabled_and_close_is_an_action() {
        let mut shop = CashShop::new(100, 50);
        assert!(!shop.select_tab(2));
        shop.append(2, vec![(501, 10)]);
        assert!(shop.select_tab(2));
        assert_eq!(shop.click(Hit::Close), Some(Action::Close));
    }

    fn stock(shop: &mut CashShop, tab: usize, count: u32) {
        shop.append(tab as i16, (0..count).map(|index| (1000 + index, 10)).collect());
    }

    #[test]
    fn goods_scroll_clamps_at_both_ends() {
        let mut shop = CashShop::new(0, 0);
        stock(&mut shop, 0, 20);
        shop.scroll_goods(-1);
        assert_eq!(shop.goods_scroll, 0);
        shop.scroll_goods(1);
        assert_eq!(shop.goods_scroll, 1);
        shop.scroll_goods(50);
        assert_eq!(shop.goods_scroll, 2);
        shop.scroll_goods(-50);
        assert_eq!(shop.goods_scroll, 0);
    }

    #[test]
    fn small_tabs_cannot_scroll() {
        let mut shop = CashShop::new(0, 0);
        stock(&mut shop, 0, GOOD_ROWS as u32 * GOOD_COLUMNS as u32);
        shop.scroll_goods(3);
        assert_eq!(shop.goods_scroll, 0);
        stock(&mut shop, 1, 4);
        shop.selected_tab = 1;
        shop.scroll_goods(3);
        assert_eq!(shop.goods_scroll, 0);
    }

    #[test]
    fn twenty_items_scroll_two_rows() {
        let mut shop = CashShop::new(0, 0);
        stock(&mut shop, 0, 20);
        shop.scroll_goods(i32::MAX);
        assert_eq!(shop.goods_scroll, 2);
    }

    #[test]
    fn cart_scroll_max_is_lines_beyond_visible() {
        let mut shop = CashShop::new(0, 0);
        shop.cart = (0..8).map(|index| (500 + index, 1, 0, 10)).collect();
        shop.scroll_cart(10);
        assert_eq!(shop.cart_scroll, 3);
        shop.scroll_cart(-1);
        assert_eq!(shop.cart_scroll, 2);
        shop.scroll_cart(-10);
        assert_eq!(shop.cart_scroll, 0);
        shop.cart.truncate(CART_ROWS);
        shop.scroll_cart(5);
        assert_eq!(shop.cart_scroll, 0);
    }

    #[test]
    fn removing_cart_lines_reclamps_cart_scroll() {
        let mut shop = CashShop::new(0, 0);
        shop.cart = (0..8).map(|index| (500 + index, 1, 0, 10)).collect();
        shop.cart_scroll = 3;
        assert!(shop.remove(507, 0));
        assert_eq!(shop.cart_scroll, 2);
        shop.remove(506, 0);
        shop.remove(505, 0);
        shop.remove(504, 0);
        assert_eq!(shop.cart_scroll, 0);
    }

    #[test]
    fn select_tab_resets_goods_scroll() {
        let mut shop = CashShop::new(0, 0);
        stock(&mut shop, 0, 20);
        stock(&mut shop, 1, 20);
        shop.goods_scroll = 2;
        assert!(shop.select_tab(1));
        assert_eq!(shop.goods_scroll, 0);
    }

    #[test]
    fn item_boxes_show_only_the_visible_window() {
        let mut shop = CashShop::new(0, 0);
        stock(&mut shop, 0, 20);
        assert_eq!(shop.item_boxes().len(), GOOD_ROWS * GOOD_COLUMNS);
        shop.scroll_goods(2);
        let boxes = shop.item_boxes();
        assert_eq!(boxes.len(), GOOD_ROWS * GOOD_COLUMNS);
        assert_eq!(boxes[0].0, shop.tabs[0][shop.goods_scroll * GOOD_COLUMNS].0);
        assert_eq!(boxes.last().unwrap().0, shop.tabs[0][19].0);
    }

    #[test]
    fn hit_item_cell_is_relative_to_the_scroll() {
        let mut shop = CashShop::new(0, 0);
        stock(&mut shop, 0, 20);
        shop.scroll_goods(1);
        let place = shop.item_boxes()[0].1;
        let hit = shop.hit([place[0] + 5.0, place[1] + 5.0]);
        assert_eq!(hit, Some(Hit::Item(0)));
        shop.click(hit.unwrap());
        assert_eq!(shop.cart, [(shop.tabs[0][GOOD_COLUMNS].0, 1, 0, 10)]);
    }

    #[test]
    fn unknown_buy_result_maps_to_1228() {
        assert_eq!(buy_result_message(99), 1228);
    }
}
