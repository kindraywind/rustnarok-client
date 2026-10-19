//! Vending: setting a shop up from the cart (its name, what goes on sale and at what price) and the vendor's own
//! window of what's still for sale, with the button that closes it.

use std::collections::HashMap;
use std::time::Duration;

use rustnarok_client::grf::Grf;
use rustnarok_client::lua::ItemInfo;
use rustnarok_client::net::{InventoryItem, VendItem};
use rustnarok_client::text::Font;

use crate::inventory;
use crate::render::{Gpu, Texture};
use crate::shop::{self, group};
use crate::window::{self, Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI, contains};

// ponytail: a 280px list of 32px rows like the NPC shop's; calibrate against the real client.
const WIDTH: f32 = 280.0;
const ROW: f32 = 32.0;
/// The vending grids' corner grips: 248, 280 or 312 wide and 2 to 5 rows (the game's 120 to 216 high, as the
/// cart's).
const LEAST_ROWS: usize = 2;
const MOST_ROWS: usize = 5;
const NAME_FIELD: f32 = 24.0;
const FOOTER: f32 = 27.0;
const BUTTON: [f32; 2] = [42.0, 20.0];
const POSITION: [f32; 2] = [330.0, 150.0];
/// "Merchant Shop", "Available Items for Vending".
const SHOP_MESSAGE: usize = 227;
const SETUP_MESSAGE: usize = 232;
const PICKED: [u8; 4] = [0x34, 0x6A, 0xE1, 0x60];
/// The most a price may be; past it the price is held there, with 2467 to say so.
pub const MOST_PRICE: u32 = 1_000_000_000;
pub const PRICE_HELD: usize = 2467;
/// "Price: ", the price box's question.
pub const PRICE_ASKS: usize = 369;
/// OK refused: nothing on sale (2494), no name (225); a price of 0 asks first (604).
const NOTHING_ON_SALE: usize = 2494;
const NO_NAME: usize = 225;
pub const FREE_ASKS: usize = 604;
/// A shop's name: 36 characters.
const NAME_LENGTH: usize = 36;

/// Whether a click on a shop's sign asks for the shop: not on your own, not while dead, not for the shop already open,
/// and at most once a second (`since` the last ask).
pub fn may_ask(sign: u32, me: u32, dead: bool, open: Option<u32>, since: Option<Duration>) -> bool {
    sign != me && !dead && open != Some(sign) && since.is_none_or(|since| since >= Duration::from_secs(1))
}

/// The size both vending windows take: width and rows.
// ponytail: one size kept for both windows; they open at 5 rows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridSize {
    pub width: f32,
    pub rows: usize,
}

impl Default for GridSize {
    fn default() -> Self {
        Self { width: WIDTH, rows: MOST_ROWS }
    }
}

impl GridSize {
    /// A width and rows as far as the grip would take them.
    pub fn clamped(width: f32, rows: usize) -> Self {
        Self { width: shop::snap_width(width), rows: rows.clamp(LEAST_ROWS, MOST_ROWS) }
    }

    /// The size a grip asks for, on a window with `above` px over its rows.
    fn asked([width, height]: [f32; 2], above: f32) -> Self {
        let least = above + LEAST_ROWS as f32 * ROW + FOOTER;
        Self::clamped(width, LEAST_ROWS + window::steps(height, least, ROW, MOST_ROWS - LEAST_ROWS))
    }
}

pub struct Assets {
    pub footer: Texture,
    /// The corner grip.
    pub resize: Texture,
    pub ok: [Texture; 3],
    pub cancel: [Texture; 3],
    pub close: [Texture; 3],
    pub picked: Texture,
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |name: &str| gpu.load_texture(grf, &format!("{UI}{name}.bmp"), false);
        Self {
            footer: image("basic_interface\\btnbar_mid2"),
            resize: image("btn_resize"),
            ok: ["btn_ok", "btn_ok_a", "btn_ok_b"].map(image),
            cancel: ["btn_cancel", "btn_cancel_a", "btn_cancel_b"].map(image),
            close: ["btn_close", "btn_close_a", "btn_close_b"].map(image),
            picked: gpu.solid(PICKED),
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(TitleHit),
    /// The shop's name field.
    Name,
    /// A cart item, by place in the list.
    Row(usize),
    Ok,
    Cancel,
    /// The corner grip.
    Resize,
    Window,
}

/// What a click asks for.
#[derive(Debug, PartialEq)]
pub enum Action {
    /// Ask the price of a cart item (by index).
    Price(u16),
    Open,
    /// Something is on sale for 0: ask (604) before opening.
    AskFree,
    /// OK refused, with this message in the message box.
    Refused(usize),
    Cancel,
}

/// Setting a shop up: its name, and the cart items picked with their prices.
pub struct Setup {
    pub frame: Frame,
    pub name: String,
    pub typing: bool,
    /// Most items it may sell.
    pub most: usize,
    /// Cart index → price.
    pub prices: HashMap<u16, u32>,
    scroll: usize,
    /// Its size, and the corner grip while held.
    pub size: GridSize,
    grip: window::Grip,
}

impl Setup {
    pub fn new(most: u16, size: GridSize) -> Self {
        let mut frame = Frame::new(POSITION, size.width);
        frame.open = true;
        let grip = window::Grip::default();
        Self { frame, name: String::new(), typing: true, most: usize::from(most), prices: HashMap::new(), scroll: 0, size, grip }
    }

    fn height(&self) -> f32 {
        TITLE_HEIGHT + NAME_FIELD + self.size.rows as f32 * ROW + FOOTER
    }

    fn buttons(&self) -> [[f32; 2]; 2] {
        let top = self.height() - FOOTER + 4.0;
        [[self.size.width - 110.0, top], [self.size.width - 60.0, top]]
    }

    pub fn start_resize(&mut self, cursor: [f32; 2]) {
        self.grip.start(cursor, [self.size.width, self.height()]);
    }

    /// While held by its grip, takes the size the pointer asks for; returns whether it's held.
    pub fn resize_to(&mut self, cursor: [f32; 2]) -> bool {
        let Some(asked) = self.grip.asked(cursor) else { return false };
        self.size = GridSize::asked(asked, TITLE_HEIGHT + NAME_FIELD);
        self.frame.set_width(self.size.width);
        true
    }

    pub fn stop_resize(&mut self) -> bool {
        self.grip.stop()
    }

    /// The corner grip on screen (for the test driver).
    pub fn grip_box(&self) -> [f32; 4] {
        let [x, y] = self.frame.at([self.size.width - window::GRIP, self.height() - window::GRIP]);
        [x, y, window::GRIP, window::GRIP]
    }

    /// The cart rows shown, by item id (for the test driver).
    pub fn row_boxes(&self, cart: &[InventoryItem]) -> Vec<(u32, [f32; 4])> {
        let rows = cart.iter().enumerate().skip(self.scroll).take(self.size.rows);
        rows.map(|(place, item)| {
            let [x, y] = self.frame.at([0.0, TITLE_HEIGHT + NAME_FIELD + (place - self.scroll) as f32 * ROW]);
            (item.id, [x, y, self.size.width, ROW])
        })
        .collect()
    }

    /// The name field, OK and Cancel (for the test driver).
    pub fn button_boxes(&self) -> Vec<(&'static str, [f32; 4])> {
        let [ok, cancel] = self.buttons().map(|place| self.frame.at(place));
        let [x, y] = self.frame.at([5.0, TITLE_HEIGHT + 3.0]);
        vec![
            ("vend_name", [x, y, self.size.width - 10.0, 18.0]),
            ("vend_ok", [ok[0], ok[1], BUTTON[0], BUTTON[1]]),
            ("vend_cancel", [cancel[0], cancel[1], BUTTON[0], BUTTON[1]]),
        ]
    }

    pub fn hit(&self, cursor: [f32; 2], cart: &[InventoryItem]) -> Option<Hit> {
        let local = self.frame.local(cursor, self.height())?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(Hit::Title(hit));
        }
        if !self.frame.minimized && window::on_grip([self.size.width, self.height()], local) {
            return Some(Hit::Resize);
        }
        let [ok, cancel] = self.buttons();
        if contains(ok, BUTTON, local) {
            return Some(Hit::Ok);
        }
        if contains(cancel, BUTTON, local) {
            return Some(Hit::Cancel);
        }
        if local[1] < TITLE_HEIGHT + NAME_FIELD {
            return Some(Hit::Name);
        }
        let row = ((local[1] - TITLE_HEIGHT - NAME_FIELD) / ROW) as usize + self.scroll;
        Some(if local[1] < self.height() - FOOTER && row < cart.len() { Hit::Row(row) } else { Hit::Window })
    }

    pub fn click(&mut self, hit: Hit, cart: &[InventoryItem]) -> Option<Action> {
        self.typing = hit == Hit::Name;
        match hit {
            Hit::Title(TitleHit::Close) | Hit::Cancel => Some(Action::Cancel),
            Hit::Title(title) => {
                self.frame.click_title(title);
                None
            }
            // A picked item goes back off sale; another asks its price (while there's room).
            Hit::Row(row) => {
                let index = cart.get(row)?.index;
                if self.prices.remove(&index).is_none() && self.prices.len() < self.most {
                    return Some(Action::Price(index));
                }
                None
            }
            // ponytail: the name is checked before the price of 0, the game checks it after.
            Hit::Ok if self.prices.is_empty() => Some(Action::Refused(NOTHING_ON_SALE)),
            Hit::Ok if self.name.trim().is_empty() => Some(Action::Refused(NO_NAME)),
            Hit::Ok if self.prices.values().any(|&price| price == 0) => Some(Action::AskFree),
            Hit::Ok => Some(Action::Open),
            _ => None,
        }
    }

    /// A cart item's price, no more than 1,000,000,000; returns whether it was held there.
    pub fn set_price(&mut self, index: u16, price: u32) -> bool {
        self.prices.insert(index, price.min(MOST_PRICE));
        price > MOST_PRICE
    }

    pub fn type_text(&mut self, text: &str) {
        let room = NAME_LENGTH.saturating_sub(self.name.chars().count());
        self.name.extend(text.chars().filter(|character| !character.is_control()).take(room));
    }

    /// What goes on sale: (cart index, whole stack, price), in cart order.
    pub fn goods(&self, cart: &[InventoryItem]) -> Vec<(u16, u16, u32)> {
        cart.iter().filter_map(|item| self.prices.get(&item.index).map(|&price| (item.index, item.amount, price))).collect()
    }

    pub fn scroll_by(&mut self, rows: i32, count: usize) {
        self.scroll = (self.scroll as i32 + rows).clamp(0, count.saturating_sub(self.size.rows) as i32) as usize;
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &self,
        layer: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        icons: &'a HashMap<String, Option<Texture>>,
        items: &HashMap<u32, ItemInfo>,
        cart: &[InventoryItem],
        font: &Font,
        messages: &[String],
        pointer: ([f32; 2], bool),
        caret: bool,
    ) {
        let at = |local: [f32; 2]| self.frame.at(local);
        self.frame.draw_title(layer, chrome, messages.get(SETUP_MESSAGE).map_or("", String::as_str), pointer.0);
        let height = self.height();
        let width = self.size.width;
        layer.sprites.push((&chrome.white, at([0.0, TITLE_HEIGHT]), [width, height - TITLE_HEIGHT - FOOTER]));
        // The shop's name, typed in.
        layer.sprites.push((&chrome.border, at([5.0, TITLE_HEIGHT + 3.0]), [width - 10.0, 18.0]));
        layer.sprites.push((&chrome.white, at([6.0, TITLE_HEIGHT + 4.0]), [width - 12.0, 16.0]));
        let name = if self.typing && caret { format!("{}|", self.name) } else { self.name.clone() };
        layer.texts.push((window::fit(font, &name, width - 20.0), at([9.0, TITLE_HEIGHT + 5.0]), [0, 0, 0]));
        // The cart, picked items marked with their price.
        for (place, item) in cart.iter().enumerate().skip(self.scroll).take(self.size.rows) {
            let top = TITLE_HEIGHT + NAME_FIELD + (place - self.scroll) as f32 * ROW;
            let price = self.prices.get(&item.index);
            if price.is_some() {
                layer.sprites.push((&assets.picked, at([1.0, top]), [width - 2.0, ROW]));
            }
            draw_item(layer, icons, items, font, item, at([4.0, top + 4.0]));
            if let Some(price) = price {
                let text = format!("{} z", group(u64::from(*price)));
                layer.texts.push((text.clone(), at([width - 6.0 - font.width(&text), top + 10.0]), [0, 0, 0]));
            }
        }
        let hit = self.hit(pointer.0, cart);
        layer.sprites.push((&assets.footer, at([0.0, height - FOOTER]), [width, FOOTER]));
        let [ok, cancel] = self.buttons();
        for (images, place, target) in [(&assets.ok, ok, Hit::Ok), (&assets.cancel, cancel, Hit::Cancel)] {
            let look = if hit == Some(target) { 1 + usize::from(pointer.1) } else { 0 };
            layer.sprites.push((&images[look], at(place), BUTTON));
        }
        layer.sprites.push((&assets.resize, at([width - window::GRIP, height - window::GRIP]), assets.resize.size));
    }
}

/// The vendor's own shop: what's left for sale, and its close button.
pub struct MyShop {
    pub frame: Frame,
    pub name: String,
    pub goods: Vec<VendItem>,
    /// Its size, and the corner grip while held.
    pub size: GridSize,
    grip: window::Grip,
}

impl MyShop {
    pub fn new(name: String, goods: Vec<VendItem>, size: GridSize) -> Self {
        let mut frame = Frame::new(POSITION, size.width);
        frame.open = true;
        Self { frame, name, goods, size, grip: window::Grip::default() }
    }

    fn height(&self) -> f32 {
        TITLE_HEIGHT + self.size.rows as f32 * ROW + FOOTER
    }

    fn close_button(&self) -> [f32; 2] {
        [self.size.width - 60.0, self.height() - FOOTER + 4.0]
    }

    /// Whether `cursor` is on the corner grip.
    pub fn on_grip(&self, cursor: [f32; 2]) -> bool {
        !self.frame.minimized && self.frame.local(cursor, self.height()).is_some_and(|local| window::on_grip([self.size.width, self.height()], local))
    }

    pub fn start_resize(&mut self, cursor: [f32; 2]) {
        self.grip.start(cursor, [self.size.width, self.height()]);
    }

    /// While held by its grip, takes the size the pointer asks for; returns whether it's held.
    pub fn resize_to(&mut self, cursor: [f32; 2]) -> bool {
        let Some(asked) = self.grip.asked(cursor) else { return false };
        self.size = GridSize::asked(asked, TITLE_HEIGHT);
        self.frame.set_width(self.size.width);
        true
    }

    pub fn stop_resize(&mut self) -> bool {
        self.grip.stop()
    }

    /// The corner grip on screen (for the test driver).
    pub fn grip_box(&self) -> [f32; 4] {
        let [x, y] = self.frame.at([self.size.width - window::GRIP, self.height() - window::GRIP]);
        [x, y, window::GRIP, window::GRIP]
    }

    /// Over the window: true on its close button.
    pub fn hit(&self, cursor: [f32; 2]) -> Option<bool> {
        let local = self.frame.local(cursor, self.height())?;
        Some(contains(self.close_button(), BUTTON, local) || self.frame.title_hit(local) == Some(TitleHit::Close))
    }

    /// Something sold: fewer left, and gone when none are. Returns whether the shop is sold out.
    pub fn sold(&mut self, index: u16, amount: u16) -> bool {
        if let Some(position) = self.goods.iter().position(|good| good.index == index) {
            let left = &mut self.goods[position].item.amount;
            *left = left.saturating_sub(amount);
            if *left == 0 {
                self.goods.remove(position);
            }
        }
        self.goods.is_empty()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &self,
        layer: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        icons: &'a HashMap<String, Option<Texture>>,
        items: &HashMap<u32, ItemInfo>,
        font: &Font,
        messages: &[String],
        pointer: ([f32; 2], bool),
    ) {
        let at = |local: [f32; 2]| self.frame.at(local);
        let title = format!("{} : {}", messages.get(SHOP_MESSAGE).map_or("", String::as_str), self.name);
        self.frame.draw_title(layer, chrome, &title, pointer.0);
        let height = self.height();
        let width = self.size.width;
        layer.sprites.push((&chrome.white, at([0.0, TITLE_HEIGHT]), [width, height - TITLE_HEIGHT - FOOTER]));
        for (place, good) in self.goods.iter().enumerate().take(self.size.rows) {
            let top = TITLE_HEIGHT + place as f32 * ROW;
            draw_item(layer, icons, items, font, &good.item, at([4.0, top + 4.0]));
            let text = format!("{} z", group(u64::from(good.price)));
            layer.texts.push((text.clone(), at([width - 6.0 - font.width(&text), top + 10.0]), [0, 0, 0]));
        }
        layer.sprites.push((&assets.footer, at([0.0, height - FOOTER]), [width, FOOTER]));
        let look = if self.hit(pointer.0) == Some(true) { 1 + usize::from(pointer.1) } else { 0 };
        layer.sprites.push((&assets.close[look], at(self.close_button()), BUTTON));
        layer.sprites.push((&assets.resize, at([width - window::GRIP, height - window::GRIP]), assets.resize.size));
    }
}

/// An item's icon, how many, and its name, from `corner`.
pub(crate) fn draw_item<'a>(
    layer: &mut Layer<'a>,
    icons: &'a HashMap<String, Option<Texture>>,
    items: &HashMap<u32, ItemInfo>,
    font: &Font,
    item: &InventoryItem,
    corner: [f32; 2],
) {
    if let Some(Some(icon)) = items.get(&item.id).and_then(|info| icons.get(&inventory::icon_path(info, item.identified))) {
        layer.sprites.push((icon, corner, icon.size));
    }
    let amount = item.amount.to_string();
    let place = [corner[0] + 24.0 - font.width(&amount), corner[1] + 12.0];
    layer.texts.push((amount.clone(), [place[0] - 1.0, place[1] - 1.0], [255; 3]));
    layer.texts.push((amount, place, [0, 0, 0]));
    let name = inventory::item_name(items, item.id, item.identified, item.refine);
    layer.texts.push((window::fit(font, &name, 150.0), [corner[0] + 30.0, corner[1] + 6.0], [0, 0, 0]));
}

/// The icons the windows show.
pub fn icons(items: &HashMap<u32, ItemInfo>, shown: impl Iterator<Item = (u32, bool)>) -> Vec<String> {
    shown.filter_map(|(id, identified)| items.get(&id).map(|info| inventory::icon_path(info, identified))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vending_grids_resize_like_the_cart() {
        assert_eq!(GridSize::default(), GridSize { width: 280.0, rows: 5 });
        assert_eq!(GridSize::clamped(100.0, 9), GridSize { width: 248.0, rows: 5 });
        assert_eq!(GridSize::clamped(320.0, 0), GridSize { width: 312.0, rows: 2 });
        let mut setup = Setup::new(3, GridSize::default());
        let corner = setup.frame.at([setup.size.width - 5.0, setup.height() - 5.0]);
        assert_eq!(setup.hit(corner, &[]), Some(Hit::Resize));
        setup.start_resize(corner);
        // 40 px narrower is 248; 96 up is three rows fewer (31 more wouldn't be one).
        assert!(setup.resize_to([corner[0] - 40.0, corner[1] - 96.0]));
        assert_eq!(setup.size, GridSize { width: 248.0, rows: 2 });
        assert!(setup.stop_resize() && !setup.resize_to(corner));
        // The open shop takes the same size and grip.
        let mut shop = MyShop::new("Shop".into(), Vec::new(), setup.size);
        let corner = shop.frame.at([shop.size.width - 5.0, shop.height() - 5.0]);
        assert!(shop.on_grip(corner));
        shop.start_resize(corner);
        shop.resize_to([corner[0] + 64.0, corner[1] + 64.0]);
        assert_eq!(shop.size, GridSize { width: 312.0, rows: 4 });
    }

    fn item(index: u16, amount: u16) -> InventoryItem {
        InventoryItem { index, id: 512, amount, identified: true, ..Default::default() }
    }

    #[test]
    fn picks_goods_and_sells_them_down() {
        let cart = [item(2, 5), item(3, 1)];
        let mut setup = Setup::new(2, GridSize::default());
        assert_eq!(setup.click(Hit::Row(0), &cart), Some(Action::Price(2)));
        assert_eq!(setup.click(Hit::Ok, &cart), Some(Action::Refused(NOTHING_ON_SALE)));
        setup.prices.insert(2, 15);
        assert_eq!(setup.click(Hit::Ok, &cart), Some(Action::Refused(NO_NAME)));
        setup.type_text("Apples");
        assert_eq!(setup.click(Hit::Ok, &cart), Some(Action::Open));
        assert_eq!(setup.goods(&cart), [(2, 5, 15)]);
        // Clicking a picked item takes it off.
        assert_eq!(setup.click(Hit::Row(0), &cart), None);
        assert!(setup.prices.is_empty());
        let mut shop = MyShop::new("Apples".into(), vec![VendItem { index: 2, item: item(2, 5), price: 15 }], GridSize::default());
        assert!(!shop.sold(2, 3));
        assert_eq!(shop.goods[0].item.amount, 2);
        assert!(shop.sold(2, 2), "the last one sold closes the shop");
        assert!(shop.goods.is_empty());
    }

    #[test]
    fn signs_ask_once_a_second_and_not_for_your_own() {
        assert!(may_ask(5, 1, false, None, None));
        assert!(!may_ask(1, 1, false, None, None), "your own sign");
        assert!(!may_ask(5, 1, true, None, None), "dead");
        assert!(!may_ask(5, 1, false, Some(5), None), "already open");
        assert!(may_ask(5, 1, false, Some(6), Some(Duration::from_millis(1000))));
        assert!(!may_ask(5, 1, false, None, Some(Duration::from_millis(999))));
    }

    #[test]
    fn prices_go_to_a_billion_and_zero_asks() {
        let cart = [item(2, 5), item(3, 1)];
        let mut setup = Setup::new(2, GridSize::default());
        setup.type_text("A shop name longer than thirty-six characters");
        assert_eq!(setup.name.chars().count(), 36);
        assert!(!setup.set_price(2, 2_500_000));
        assert_eq!(setup.goods(&cart), [(2, 5, 2_500_000)]);
        assert!(setup.set_price(3, 1_500_000_000), "held at the most");
        assert_eq!(setup.prices[&3], MOST_PRICE);
        assert!(!setup.set_price(3, 0));
        assert_eq!(setup.click(Hit::Ok, &cart), Some(Action::AskFree));
    }
}
