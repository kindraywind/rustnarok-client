//! Vending: setting a shop up from the cart (its name, what goes on sale and at what price) and the vendor's own
//! window of what's still for sale, with the button that closes it.

use std::collections::HashMap;

use rustnarok_client::grf::Grf;
use rustnarok_client::lua::ItemInfo;
use rustnarok_client::net::{InventoryItem, VendItem};
use rustnarok_client::text::Font;

use crate::inventory;
use crate::render::{Gpu, Texture};
use crate::shop::group;
use crate::window::{self, Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI, contains};

// ponytail: a 280px list of 32px rows like the NPC shop's; calibrate against the real client.
const WIDTH: f32 = 280.0;
const ROW: f32 = 32.0;
const ROWS: usize = 6;
const NAME_FIELD: f32 = 24.0;
const FOOTER: f32 = 27.0;
const BUTTON: [f32; 2] = [42.0, 20.0];
const POSITION: [f32; 2] = [330.0, 150.0];
/// "Merchant Shop", "Available Items for Vending".
const SHOP_MESSAGE: usize = 227;
const SETUP_MESSAGE: usize = 232;
const PICKED: [u8; 4] = [0x34, 0x6A, 0xE1, 0x60];

pub struct Assets {
    footer: Texture,
    ok: [Texture; 3],
    cancel: [Texture; 3],
    close: [Texture; 3],
    picked: Texture,
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |name: &str| gpu.load_texture(grf, &format!("{UI}{name}.bmp"), false);
        Self {
            footer: image("basic_interface\\btnbar_mid2"),
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
    Window,
}

/// What a click asks for.
#[derive(Debug, PartialEq)]
pub enum Action {
    /// Ask the price of a cart item (by index).
    Price(u16),
    Open,
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
}

impl Setup {
    pub fn new(most: u16) -> Self {
        let mut frame = Frame::new(POSITION, WIDTH);
        frame.open = true;
        Self { frame, name: String::new(), typing: true, most: usize::from(most), prices: HashMap::new(), scroll: 0 }
    }

    fn height() -> f32 {
        TITLE_HEIGHT + NAME_FIELD + ROWS as f32 * ROW + FOOTER
    }

    fn buttons() -> [[f32; 2]; 2] {
        let top = Self::height() - FOOTER + 4.0;
        [[WIDTH - 110.0, top], [WIDTH - 60.0, top]]
    }

    pub fn hit(&self, cursor: [f32; 2], cart: &[InventoryItem]) -> Option<Hit> {
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
        let row = ((local[1] - TITLE_HEIGHT - NAME_FIELD) / ROW) as usize + self.scroll;
        Some(if local[1] < Self::height() - FOOTER && row < cart.len() { Hit::Row(row) } else { Hit::Window })
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
            Hit::Ok if !self.prices.is_empty() && !self.name.trim().is_empty() => Some(Action::Open),
            _ => None,
        }
    }

    pub fn type_text(&mut self, text: &str) {
        let room = 80usize.saturating_sub(self.name.len());
        self.name.extend(text.chars().filter(|character| !character.is_control()).take(room));
    }

    /// What goes on sale: (cart index, whole stack, price), in cart order.
    pub fn goods(&self, cart: &[InventoryItem]) -> Vec<(u16, u16, u32)> {
        cart.iter().filter_map(|item| self.prices.get(&item.index).map(|&price| (item.index, item.amount, price))).collect()
    }

    pub fn scroll_by(&mut self, rows: i32, count: usize) {
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
        cart: &[InventoryItem],
        font: &Font,
        messages: &[String],
        pointer: ([f32; 2], bool),
        caret: bool,
    ) {
        let at = |local: [f32; 2]| self.frame.at(local);
        self.frame.draw_title(layer, chrome, messages.get(SETUP_MESSAGE).map_or("", String::as_str), pointer.0);
        let height = Self::height();
        layer.sprites.push((&chrome.white, at([0.0, TITLE_HEIGHT]), [WIDTH, height - TITLE_HEIGHT - FOOTER]));
        // The shop's name, typed in.
        layer.sprites.push((&chrome.border, at([5.0, TITLE_HEIGHT + 3.0]), [WIDTH - 10.0, 18.0]));
        layer.sprites.push((&chrome.white, at([6.0, TITLE_HEIGHT + 4.0]), [WIDTH - 12.0, 16.0]));
        let name = if self.typing && caret { format!("{}|", self.name) } else { self.name.clone() };
        layer.texts.push((window::fit(font, &name, WIDTH - 20.0), at([9.0, TITLE_HEIGHT + 5.0]), [0, 0, 0]));
        // The cart, picked items marked with their price.
        for (place, item) in cart.iter().enumerate().skip(self.scroll).take(ROWS) {
            let top = TITLE_HEIGHT + NAME_FIELD + (place - self.scroll) as f32 * ROW;
            let price = self.prices.get(&item.index);
            if price.is_some() {
                layer.sprites.push((&assets.picked, at([1.0, top]), [WIDTH - 2.0, ROW]));
            }
            draw_item(layer, icons, items, font, item, at([4.0, top + 4.0]));
            if let Some(price) = price {
                let text = format!("{} z", group(u64::from(*price)));
                layer.texts.push((text.clone(), at([WIDTH - 6.0 - font.width(&text), top + 10.0]), [0, 0, 0]));
            }
        }
        let hit = self.hit(pointer.0, cart);
        layer.sprites.push((&assets.footer, at([0.0, height - FOOTER]), [WIDTH, FOOTER]));
        let [ok, cancel] = Self::buttons();
        for (images, place, target) in [(&assets.ok, ok, Hit::Ok), (&assets.cancel, cancel, Hit::Cancel)] {
            let look = if hit == Some(target) { 1 + usize::from(pointer.1) } else { 0 };
            layer.sprites.push((&images[look], at(place), BUTTON));
        }
    }
}

/// The vendor's own shop: what's left for sale, and its close button.
pub struct MyShop {
    pub frame: Frame,
    pub name: String,
    pub goods: Vec<VendItem>,
}

impl MyShop {
    pub fn new(name: String, goods: Vec<VendItem>) -> Self {
        let mut frame = Frame::new(POSITION, WIDTH);
        frame.open = true;
        Self { frame, name, goods }
    }

    fn height(&self) -> f32 {
        TITLE_HEIGHT + ROWS as f32 * ROW + FOOTER
    }

    fn close_button(&self) -> [f32; 2] {
        [WIDTH - 60.0, self.height() - FOOTER + 4.0]
    }

    /// Over the window: true on its close button.
    pub fn hit(&self, cursor: [f32; 2]) -> Option<bool> {
        let local = self.frame.local(cursor, self.height())?;
        Some(contains(self.close_button(), BUTTON, local) || self.frame.title_hit(local) == Some(TitleHit::Close))
    }

    /// Something sold: fewer left, and gone when none are.
    pub fn sold(&mut self, index: u16, amount: u16) {
        if let Some(position) = self.goods.iter().position(|good| good.index == index) {
            let left = &mut self.goods[position].item.amount;
            *left = left.saturating_sub(amount);
            if *left == 0 {
                self.goods.remove(position);
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
        font: &Font,
        messages: &[String],
        pointer: ([f32; 2], bool),
    ) {
        let at = |local: [f32; 2]| self.frame.at(local);
        let title = format!("{} : {}", messages.get(SHOP_MESSAGE).map_or("", String::as_str), self.name);
        self.frame.draw_title(layer, chrome, &title, pointer.0);
        let height = self.height();
        layer.sprites.push((&chrome.white, at([0.0, TITLE_HEIGHT]), [WIDTH, height - TITLE_HEIGHT - FOOTER]));
        for (place, good) in self.goods.iter().enumerate().take(ROWS) {
            let top = TITLE_HEIGHT + place as f32 * ROW;
            draw_item(layer, icons, items, font, &good.item, at([4.0, top + 4.0]));
            let text = format!("{} z", group(u64::from(good.price)));
            layer.texts.push((text.clone(), at([WIDTH - 6.0 - font.width(&text), top + 10.0]), [0, 0, 0]));
        }
        layer.sprites.push((&assets.footer, at([0.0, height - FOOTER]), [WIDTH, FOOTER]));
        let look = if self.hit(pointer.0) == Some(true) { 1 + usize::from(pointer.1) } else { 0 };
        layer.sprites.push((&assets.close[look], at(self.close_button()), BUTTON));
    }
}

/// An item's icon, how many, and its name, from `corner`.
fn draw_item<'a>(
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

    fn item(index: u16, amount: u16) -> InventoryItem {
        InventoryItem { index, id: 512, amount, identified: true, ..Default::default() }
    }

    #[test]
    fn picks_goods_and_sells_them_down() {
        let cart = [item(2, 5), item(3, 1)];
        let mut setup = Setup::new(2);
        assert_eq!(setup.click(Hit::Row(0), &cart), Some(Action::Price(2)));
        setup.prices.insert(2, 15);
        assert_eq!(setup.click(Hit::Ok, &cart), None, "no name yet");
        setup.type_text("Apples");
        assert_eq!(setup.click(Hit::Ok, &cart), Some(Action::Open));
        assert_eq!(setup.goods(&cart), [(2, 5, 15)]);
        // Clicking a picked item takes it off.
        assert_eq!(setup.click(Hit::Row(0), &cart), None);
        assert!(setup.prices.is_empty());
        let mut shop = MyShop::new("Apples".into(), vec![VendItem { index: 2, item: item(2, 5), price: 15 }]);
        shop.sold(2, 3);
        assert_eq!(shop.goods[0].item.amount, 2);
        shop.sold(2, 2);
        assert!(shop.goods.is_empty());
    }
}
