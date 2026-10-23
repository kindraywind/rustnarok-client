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
    /// The mirror's cell.
    pub cell: Texture,
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
            cell: image("basic_interface\\itemwin_mid"),
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(TitleHit),
    /// The shop's name box.
    Name,
    /// A row on sale (by place in the goods): its price box, or the rest of it.
    Price(usize),
    Good(usize),
    Ok,
    Cancel,
    Window,
}

/// What a click asks for.
#[derive(Debug, PartialEq)]
pub enum Action {
    Open,
    /// Something is on sale for 0: ask (604) before opening.
    AskFree,
    /// OK refused, with this message in the message box.
    Refused(usize),
    Cancel,
}

/// Setting a shop up ("Opening a stall"): 400 × 260 at (0, 240), its name, and one 42 px row for each cart item on sale
/// with a price box of its own; the cart beside it as an icon grid ("Available Items for Vending", 280 × 120 at
/// (0, 120)) that items are dragged out of and back into.
const SETUP_AT: [f32; 2] = [0.0, 240.0];
const SETUP_SIZE: [f32; 2] = [400.0, 260.0];
const SETUP_TITLE: usize = 224;
const MIRROR_AT: [f32; 2] = [0.0, 120.0];
const MIRROR_SIZE: [f32; 2] = [280.0, 120.0];
/// The mirror's cells: 7 across from (40, 17), 2 rows showing.
const MIRROR_CELLS: [f32; 2] = [40.0, TITLE_HEIGHT];
const MIRROR_COLUMNS: usize = 7;
const MIRROR_ROWS: usize = 2;
const CELL: f32 = 32.0;
const MIRROR_FOOTER: f32 = 21.0;
/// "Name" at (2, 25) and its box at (41, 22), (w − 66) × 16.
const NAME_LABEL: usize = 282;
const NAME_LABEL_AT: [f32; 2] = [2.0, 25.0];
const NAME_BOX_AT: [f32; 2] = [41.0, 22.0];
/// Row k: the icon at (7, 48 + 42k), the name at (39, 57 + 42k) 100 wide, "Price: " at (w − 165, 57 + 42k), its box
/// 94 × 16 at (w − 119, 54 + 42k), and the total under at (w − 165, 72 + 42k).
const GOOD_ROW: f32 = 42.0;
const GOOD_ROWS: usize = 4;
const GOOD_ICON: [f32; 2] = [7.0, 48.0];
const GOOD_NAME: [f32; 2] = [39.0, 57.0];
const GOOD_NAME_WIDTH: f32 = 100.0;
const PRICE_LABEL: [f32; 2] = [165.0, 57.0];
const PRICE_BOX: [f32; 2] = [119.0, 54.0];
const PRICE_BOX_SIZE: [f32; 2] = [94.0, 16.0];
const TOTAL_AT: [f32; 2] = [165.0, 72.0];
/// "Total amount : ", the price less the 5 % tax from 10,000,001 up.
const TOTAL_MESSAGE: usize = 3222;
const TAXED_FROM: u64 = 10_000_001;
/// A price box takes 14 characters, commas included.
const PRICE_CHARACTERS: usize = 14;
/// At most 30,000 of an item go on sale (1705, in red).
pub const MOST_AMOUNT: u32 = 30_000;
pub const TOO_MANY: usize = 1705;

/// The typed box: the shop's name, or a row's price.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Focus {
    Name,
    Price(usize),
}

/// A cart item on sale: how many, and its price as typed (digits only).
#[derive(Debug, PartialEq, Clone)]
pub struct Good {
    pub index: u16,
    pub amount: u16,
    pub typed: String,
}

impl Good {
    pub fn price(&self) -> u64 {
        self.typed.parse().unwrap_or(0)
    }
}

/// What it gets for a price: less 5 % from 10,000,001 up.
pub fn after_tax(price: u64) -> u64 {
    if price >= TAXED_FROM { price - price * 5 / 100 } else { price }
}

/// Setting a shop up: its name, and what's on sale.
pub struct Setup {
    pub frame: Frame,
    pub mirror: Frame,
    pub name: String,
    pub focus: Option<Focus>,
    /// Most items it may sell.
    pub most: usize,
    pub goods: Vec<Good>,
    scroll: usize,
    mirror_scroll: usize,
}

impl Setup {
    pub fn new(most: u16) -> Self {
        let mut frame = Frame::new(SETUP_AT, SETUP_SIZE[0]);
        frame.open = true;
        let mut mirror = Frame::new(MIRROR_AT, MIRROR_SIZE[0]);
        mirror.open = true;
        Self { frame, mirror, name: String::new(), focus: Some(Focus::Name), most: usize::from(most), goods: Vec::new(), scroll: 0, mirror_scroll: 0 }
    }

    pub fn typing(&self) -> bool {
        self.focus.is_some()
    }

    fn height(&self) -> f32 {
        SETUP_SIZE[1]
    }

    fn buttons(&self) -> [[f32; 2]; 2] {
        let top = self.height() - FOOTER + 4.0;
        [[SETUP_SIZE[0] - 110.0, top], [SETUP_SIZE[0] - 60.0, top]]
    }

    fn row_top(&self, row: usize) -> f32 {
        (row - self.scroll) as f32 * GOOD_ROW
    }

    fn price_box(&self, row: usize) -> [f32; 2] {
        [SETUP_SIZE[0] - PRICE_BOX[0], PRICE_BOX[1] + self.row_top(row)]
    }

    /// How many of a cart item (by index) are still in the cart, not on sale.
    pub fn left_of(&self, index: u16, cart: &[InventoryItem]) -> Option<u16> {
        cart.iter().find(|item| item.index == index).map(|item| self.left(item))
    }

    /// How many of a cart item are still in the cart, not on sale.
    fn left(&self, item: &InventoryItem) -> u16 {
        let on_sale = self.goods.iter().find(|good| good.index == item.index).map_or(0, |good| good.amount);
        item.amount.saturating_sub(on_sale)
    }

    /// The mirror's items: the cart's, less what's on sale (none left, not shown).
    fn mirrored<'a>(&'a self, cart: &'a [InventoryItem]) -> impl Iterator<Item = (&'a InventoryItem, u16)> + 'a {
        cart.iter().map(|item| (item, self.left(item))).filter(|&(_, left)| left > 0)
    }

    /// The mirror's cell under `cursor`, by cart index; `Some(None)` over the rest of it.
    pub fn mirror_hit(&self, cursor: [f32; 2], cart: &[InventoryItem]) -> Option<Option<u16>> {
        let local = self.mirror.local(cursor, MIRROR_SIZE[1])?;
        let [x, y] = [local[0] - MIRROR_CELLS[0], local[1] - MIRROR_CELLS[1]];
        if x < 0.0 || y < 0.0 || x >= MIRROR_COLUMNS as f32 * CELL || y >= MIRROR_ROWS as f32 * CELL {
            return Some(None);
        }
        let cell = (self.mirror_scroll + (y / CELL) as usize) * MIRROR_COLUMNS + (x / CELL) as usize;
        Some(self.mirrored(cart).nth(cell).map(|(item, _)| item.index))
    }

    /// Over the setup or its mirror.
    pub fn covers(&self, cursor: [f32; 2]) -> bool {
        self.frame.local(cursor, self.height()).is_some() || self.mirror.local(cursor, MIRROR_SIZE[1]).is_some()
    }

    /// The mirror's cells by item id, the goods' price boxes by item id, and the goods' area (for the test driver).
    pub fn mirror_boxes(&self, cart: &[InventoryItem]) -> Vec<(u32, [f32; 4])> {
        let shown = self.mirrored(cart).skip(self.mirror_scroll * MIRROR_COLUMNS).take(MIRROR_COLUMNS * MIRROR_ROWS);
        shown
            .enumerate()
            .map(|(cell, (item, _))| {
                let place = [MIRROR_CELLS[0] + (cell % MIRROR_COLUMNS) as f32 * CELL, MIRROR_CELLS[1] + (cell / MIRROR_COLUMNS) as f32 * CELL];
                let [x, y] = self.mirror.at(place);
                (item.id, [x, y, CELL, CELL])
            })
            .collect()
    }

    pub fn row_boxes(&self, cart: &[InventoryItem]) -> Vec<(u32, [f32; 4])> {
        let rows = self.goods.iter().enumerate().skip(self.scroll).take(GOOD_ROWS);
        rows.filter_map(|(row, good)| {
            let id = cart.iter().find(|item| item.index == good.index)?.id;
            let [x, y] = self.frame.at(self.price_box(row));
            Some((id, [x, y, PRICE_BOX_SIZE[0], PRICE_BOX_SIZE[1]]))
        })
        .collect()
    }

    /// The name box, the goods' area, OK and Cancel (for the test driver).
    pub fn button_boxes(&self) -> Vec<(&'static str, [f32; 4])> {
        let [ok, cancel] = self.buttons().map(|place| self.frame.at(place));
        let [x, y] = self.frame.at(NAME_BOX_AT);
        let [gx, gy] = self.frame.at([0.0, GOOD_ICON[1] - 4.0]);
        vec![
            ("vend_name", [x, y, SETUP_SIZE[0] - 66.0, 16.0]),
            ("vend_goods", [gx, gy, SETUP_SIZE[0], GOOD_ROWS as f32 * GOOD_ROW]),
            ("vend_ok", [ok[0], ok[1], BUTTON[0], BUTTON[1]]),
            ("vend_cancel", [cancel[0], cancel[1], BUTTON[0], BUTTON[1]]),
        ]
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let local = self.frame.local(cursor, self.height())?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(Hit::Title(hit));
        }
        let [ok, cancel] = self.buttons();
        if contains(ok, BUTTON, local) {
            return Some(Hit::Ok);
        }
        if contains(cancel, BUTTON, local) {
            return Some(Hit::Cancel);
        }
        if contains(NAME_BOX_AT, [SETUP_SIZE[0] - 66.0, 16.0], local) {
            return Some(Hit::Name);
        }
        let top = GOOD_ICON[1] - 4.0;
        let row = ((local[1] - top) / GOOD_ROW).floor();
        if local[1] < top || row as usize >= GOOD_ROWS {
            return Some(Hit::Window);
        }
        let row = row as usize + self.scroll;
        if row >= self.goods.len() {
            return Some(Hit::Window);
        }
        Some(if contains(self.price_box(row), PRICE_BOX_SIZE, local) { Hit::Price(row) } else { Hit::Good(row) })
    }

    pub fn click(&mut self, hit: Hit) -> Option<Action> {
        let held = self.leave_price();
        self.focus = match hit {
            Hit::Name => Some(Focus::Name),
            Hit::Price(row) => Some(Focus::Price(row)),
            _ => None,
        };
        let action = match hit {
            Hit::Title(TitleHit::Close) | Hit::Cancel => Some(Action::Cancel),
            Hit::Title(title) => {
                self.frame.click_title(title);
                None
            }
            // ponytail: the name is checked before the price of 0, the game checks it after.
            Hit::Ok if self.goods.is_empty() => Some(Action::Refused(NOTHING_ON_SALE)),
            Hit::Ok if self.name.trim().is_empty() => Some(Action::Refused(NO_NAME)),
            Hit::Ok if self.goods.iter().any(|good| good.price() == 0) => Some(Action::AskFree),
            Hit::Ok => Some(Action::Open),
            _ => None,
        };
        if held && action.is_none() { Some(Action::Refused(PRICE_HELD)) } else { action }
    }

    /// The price box is left: a price past 1,000,000,000 is held there. Returns whether it was.
    pub fn leave_price(&mut self) -> bool {
        let Some(Focus::Price(row)) = self.focus else { return false };
        self.focus = None;
        let Some(good) = self.goods.get_mut(row) else { return false };
        let held = good.price() > u64::from(MOST_PRICE);
        if held {
            good.typed = MOST_PRICE.to_string();
        }
        held
    }

    /// Puts `amount` of a cart item on sale (more of one already on it adds up); refused past 30,000 (1705), and with
    /// no room left.
    pub fn put(&mut self, index: u16, amount: u32) -> Result<(), Option<usize>> {
        let position = self.goods.iter().position(|good| good.index == index);
        let before = position.map_or(0, |position| u32::from(self.goods[position].amount));
        if before + amount > MOST_AMOUNT {
            return Err(Some(TOO_MANY));
        }
        match position {
            Some(position) => self.goods[position].amount = (before + amount) as u16,
            None if self.goods.len() < self.most => self.goods.push(Good { index, amount: amount as u16, typed: String::new() }),
            None => return Err(None),
        }
        Ok(())
    }

    /// A row comes off sale, back to the cart.
    pub fn take_out(&mut self, row: usize) {
        if row < self.goods.len() {
            self.goods.remove(row);
            self.focus = self.focus.filter(|focus| *focus == Focus::Name);
            self.scroll = self.scroll.min(self.goods.len().saturating_sub(GOOD_ROWS));
        }
    }

    pub fn type_text(&mut self, text: &str) {
        match self.focus {
            Some(Focus::Name) => {
                let room = NAME_LENGTH.saturating_sub(self.name.chars().count());
                self.name.extend(text.chars().filter(|character| !character.is_control()).take(room));
            }
            Some(Focus::Price(row)) => {
                let Some(good) = self.goods.get_mut(row) else { return };
                for digit in text.chars().filter(char::is_ascii_digit) {
                    let longer = format!("{}{digit}", good.typed).trim_start_matches('0').to_owned();
                    if group(longer.parse().unwrap_or(0)).len() <= PRICE_CHARACTERS {
                        good.typed = longer;
                    }
                }
            }
            None => {}
        }
    }

    pub fn backspace(&mut self) {
        match self.focus {
            Some(Focus::Name) => {
                self.name.pop();
            }
            Some(Focus::Price(row)) => {
                if let Some(good) = self.goods.get_mut(row) {
                    good.typed.pop();
                }
            }
            None => {}
        }
    }

    /// What goes on sale: (cart index, how many, price), in the order put on.
    pub fn for_sale(&self) -> Vec<(u16, u16, u32)> {
        self.goods.iter().map(|good| (good.index, good.amount, good.price().min(u64::from(MOST_PRICE)) as u32)).collect()
    }

    /// The wheel over the goods, or over the mirror.
    pub fn scroll_by(&mut self, cursor: [f32; 2], rows: i32, cart: &[InventoryItem]) {
        if self.mirror.local(cursor, MIRROR_SIZE[1]).is_some() {
            let lines = self.mirrored(cart).count().div_ceil(MIRROR_COLUMNS);
            self.mirror_scroll = (self.mirror_scroll as i32 + rows).clamp(0, lines.saturating_sub(MIRROR_ROWS) as i32) as usize;
        } else {
            self.scroll = (self.scroll as i32 + rows).clamp(0, self.goods.len().saturating_sub(GOOD_ROWS) as i32) as usize;
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
        cart: &[InventoryItem],
        font: &Font,
        messages: &[String],
        pointer: ([f32; 2], bool),
        caret: bool,
    ) {
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        self.draw_mirror(layer, chrome, assets, icons, items, cart, font, message(SETUP_MESSAGE), pointer.0);
        let at = |local: [f32; 2]| self.frame.at(local);
        self.frame.draw_title(layer, chrome, message(SETUP_TITLE), pointer.0);
        if self.frame.minimized {
            return;
        }
        let [width, height] = SETUP_SIZE;
        layer.sprites.push((&chrome.white, at([0.0, TITLE_HEIGHT]), [width, height - TITLE_HEIGHT - FOOTER]));
        let boxed = |layer: &mut Layer<'a>, corner: [f32; 2], size: [f32; 2]| {
            layer.sprites.push((&chrome.border, at([corner[0] - 1.0, corner[1] - 1.0]), [size[0] + 2.0, size[1] + 2.0]));
            layer.sprites.push((&chrome.white, at(corner), size));
        };
        // The shop's name.
        layer.texts.push((message(NAME_LABEL).to_owned(), at(NAME_LABEL_AT), [0, 0, 0]));
        boxed(layer, NAME_BOX_AT, [width - 66.0, 16.0]);
        let name = if self.focus == Some(Focus::Name) && caret { format!("{}|", self.name) } else { self.name.clone() };
        layer.texts.push((window::fit(font, &name, width - 72.0), at([NAME_BOX_AT[0] + 3.0, NAME_BOX_AT[1] + 2.0]), [0, 0, 0]));
        // The goods: icon and how many, the name, the price box, and what it comes to.
        for (row, good) in self.goods.iter().enumerate().skip(self.scroll).take(GOOD_ROWS) {
            let Some(item) = cart.iter().find(|item| item.index == good.index) else { continue };
            let top = self.row_top(row);
            let shown = InventoryItem { amount: good.amount, ..item.clone() };
            draw_icon(layer, icons, items, font, &shown, at([GOOD_ICON[0], GOOD_ICON[1] + top]));
            let name = inventory::item_name(items, item.id, item.identified, item.refine);
            layer.texts.push((window::fit(font, &name, GOOD_NAME_WIDTH), at([GOOD_NAME[0], GOOD_NAME[1] + top]), [0, 0, 0]));
            layer.texts.push((message(PRICE_ASKS).to_owned(), at([width - PRICE_LABEL[0], PRICE_LABEL[1] + top]), [0, 0, 0]));
            let place = self.price_box(row);
            boxed(layer, place, PRICE_BOX_SIZE);
            let price = if good.typed.is_empty() { String::new() } else { group(good.price()) };
            let price = if self.focus == Some(Focus::Price(row)) && caret { format!("{price}|") } else { price };
            layer.texts.push((price, at([place[0] + 3.0, place[1] + 2.0]), [0, 0, 0]));
            if good.price() > 0 {
                let total = format!("{}{}", message(TOTAL_MESSAGE), group(after_tax(good.price())));
                layer.texts.push((total, at([width - TOTAL_AT[0], TOTAL_AT[1] + top]), [0, 0, 0]));
            }
        }
        let hit = self.hit(pointer.0);
        layer.sprites.push((&assets.footer, at([0.0, height - FOOTER]), [width, FOOTER]));
        let [ok, cancel] = self.buttons();
        for (images, place, target) in [(&assets.ok, ok, Hit::Ok), (&assets.cancel, cancel, Hit::Cancel)] {
            let look = if hit == Some(target) { 1 + usize::from(pointer.1) } else { 0 };
            layer.sprites.push((&images[look], at(place), BUTTON));
        }
    }

    /// The cart's mirror: what's left in the cart as an icon grid.
    #[allow(clippy::too_many_arguments)]
    fn draw_mirror<'a>(
        &self,
        layer: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        icons: &'a HashMap<String, Option<Texture>>,
        items: &HashMap<u32, ItemInfo>,
        cart: &[InventoryItem],
        font: &Font,
        title: &str,
        cursor: [f32; 2],
    ) {
        let at = |local: [f32; 2]| self.mirror.at(local);
        self.mirror.draw_title(layer, chrome, title, cursor);
        if self.mirror.minimized {
            return;
        }
        let [width, height] = MIRROR_SIZE;
        layer.sprites.push((&chrome.white, at([0.0, TITLE_HEIGHT]), [width, height - TITLE_HEIGHT - MIRROR_FOOTER]));
        for cell in 0..MIRROR_COLUMNS * MIRROR_ROWS {
            let place = [MIRROR_CELLS[0] + (cell % MIRROR_COLUMNS) as f32 * CELL, MIRROR_CELLS[1] + (cell / MIRROR_COLUMNS) as f32 * CELL];
            layer.sprites.push((&assets.cell, at(place), assets.cell.size));
        }
        let shown = self.mirrored(cart).skip(self.mirror_scroll * MIRROR_COLUMNS).take(MIRROR_COLUMNS * MIRROR_ROWS);
        for (cell, (item, left)) in shown.enumerate() {
            let place = [MIRROR_CELLS[0] + (cell % MIRROR_COLUMNS) as f32 * CELL, MIRROR_CELLS[1] + (cell / MIRROR_COLUMNS) as f32 * CELL];
            draw_icon(layer, icons, items, font, &InventoryItem { amount: left, ..item.clone() }, at([place[0] + 4.0, place[1] + 4.0]));
        }
        layer.sprites.push((&assets.footer, at([0.0, height - MIRROR_FOOTER]), [width, MIRROR_FOOTER]));
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

/// An item's icon and how many, from `corner`.
fn draw_icon<'a>(
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
    draw_icon(layer, icons, items, font, item, corner);
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
        let mut shop = MyShop::new("Shop".into(), Vec::new(), GridSize { width: 248.0, rows: 2 });
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
    fn the_setup_lays_out_rows_of_42_beside_the_mirror() {
        let cart = [item(2, 5), item(3, 1)];
        let mut setup = Setup::new(3);
        assert_eq!(setup.frame.position, [0.0, 240.0]);
        assert_eq!(setup.mirror.position, [0.0, 120.0]);
        // The mirror shows the cart, 7 across from (40, 17).
        assert_eq!(setup.mirror_hit([40.0 + 16.0, 120.0 + 17.0 + 16.0], &cart), Some(Some(2)));
        assert_eq!(setup.mirror_hit([72.0 + 16.0, 120.0 + 17.0 + 16.0], &cart), Some(Some(3)));
        assert_eq!(setup.mirror_hit([5.0, 130.0], &cart), Some(None));
        setup.put(2, 3).unwrap();
        setup.put(3, 1).unwrap();
        // What's on sale leaves the mirror: 2 of the 5, and none of the single one.
        assert_eq!(setup.mirror_boxes(&cart).len(), 1);
        assert_eq!(setup.mirrored(&cart).map(|(_, left)| left).collect::<Vec<_>>(), [2]);
        // Row 1's price box: 94 × 16 at (w − 119, 54 + 42).
        assert_eq!(setup.hit([281.0 + 5.0, 240.0 + 96.0 + 5.0]), Some(Hit::Price(1)));
        assert_eq!(setup.hit([60.0, 240.0 + 100.0]), Some(Hit::Good(1)));
        assert_eq!(setup.hit([20.0, 240.0 + 42.0]), Some(Hit::Window));
        assert_eq!(setup.hit([60.0, 240.0 + 30.0]), Some(Hit::Name));
        assert_eq!(setup.row_boxes(&cart)[1].1, [281.0, 336.0, 94.0, 16.0]);
        // Dragged back, it comes off sale.
        setup.take_out(1);
        assert_eq!(setup.for_sale(), [(2, 3, 0)]);
    }

    #[test]
    fn at_most_30000_of_an_item_and_no_more_rows_than_the_skill_gives() {
        let mut setup = Setup::new(1);
        assert_eq!(setup.put(2, 30_001), Err(Some(TOO_MANY)));
        setup.put(2, 29_999).unwrap();
        assert_eq!(setup.put(2, 2), Err(Some(TOO_MANY)), "adding to what's on sale counts too");
        setup.put(2, 1).unwrap();
        assert_eq!(setup.goods[0].amount, 30_000);
        assert_eq!(setup.put(3, 1), Err(None), "one row only");
    }

    #[test]
    fn price_boxes_group_digits_and_take_14_characters() {
        let mut setup = Setup::new(2);
        setup.put(2, 1).unwrap();
        setup.focus = Some(Focus::Price(0));
        setup.type_text("2500000");
        assert_eq!(group(setup.goods[0].price()), "2,500,000");
        setup.goods[0].typed.clear();
        setup.type_text("1234567890123");
        assert_eq!(group(setup.goods[0].price()), "12,345,678,901", "14 characters, commas included");
        setup.backspace();
        assert_eq!(setup.goods[0].typed, "1234567890");
        // Leaving a price past a billion holds it there.
        assert_eq!(setup.click(Hit::Window), Some(Action::Refused(PRICE_HELD)));
        assert_eq!(setup.goods[0].price(), 1_000_000_000);
        assert_eq!(after_tax(10_000_000), 10_000_000);
        assert_eq!(after_tax(10_000_001), 9_500_001);
        assert_eq!(after_tax(2_500_000_000), 2_375_000_000);
    }

    #[test]
    fn ok_needs_goods_a_name_and_asks_about_free_ones() {
        let mut setup = Setup::new(2);
        assert_eq!(setup.click(Hit::Ok), Some(Action::Refused(NOTHING_ON_SALE)));
        setup.put(2, 5).unwrap();
        assert_eq!(setup.click(Hit::Ok), Some(Action::Refused(NO_NAME)));
        setup.focus = Some(Focus::Name);
        setup.type_text("A shop name longer than thirty-six characters");
        assert_eq!(setup.name.chars().count(), 36);
        assert_eq!(setup.click(Hit::Ok), Some(Action::AskFree));
        setup.focus = Some(Focus::Price(0));
        setup.type_text("15");
        assert_eq!(setup.click(Hit::Ok), Some(Action::Open));
        assert_eq!(setup.for_sale(), [(2, 5, 15)]);
    }

    #[test]
    fn my_shop_sells_down() {
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
}
