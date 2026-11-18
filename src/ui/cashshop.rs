//! The cash shop: its category tabs, pages of goods, points and the list being bought.

use std::collections::HashMap;

use rustnarok_client::grf::Grf;
use rustnarok_client::lua::ItemInfo;
use rustnarok_client::text::Font;

use crate::inventory;
use crate::iteminfo::picture_path;
use crate::render::{Gpu, Texture};
use crate::shop::group;
use crate::window::{BUTTON_SIZE as TITLE_BUTTON, Chrome, Frame, TitleHit};
use crate::{Layer, UI, contains, offset};

// The window is `img_shop_bg.bmp` (723 x 540), title bar included; every place below is from its top-left corner.
const WIDTH: f32 = 723.0;
const HEIGHT: f32 = 540.0;
// ponytail: opens in the middle of a 1024 x 768 screen; the game's own first place wasn't looked up.
const POSITION: [f32; 2] = [150.0, 114.0];
const BASE_BUTTON: [f32; 2] = [3.0, 3.0];
// The tabs are picture buttons in a row; a tab's name shows while the pointer is on it.
const TABS: [f32; 2] = [8.0, 72.0];
const TAB_STEP: f32 = 58.0;
const TAB_SIZE: [f32; 2] = [56.0, 31.0];
// Nine goods to a page, three across.
const GOODS: [f32; 2] = [8.0, 107.0];
const GOOD_STEP: [f32; 2] = [175.0, 127.0];
const GOOD_SIZE: [f32; 2] = [172.0, 126.0];
const GOOD_COLUMNS: usize = 3;
const PAGE: usize = 9;
// Inside a cell: the name centred across it, the picture, the price ending at PRICE and the button that adds one.
const GOOD_NAME: [f32; 2] = [0.0, 6.0];
const GOOD_NAME_WIDTH: f32 = 168.0;
const GOOD_PICTURE: [f32; 2] = [5.0, 23.0];
const GOOD_PRICE: [f32; 2] = [160.0, 50.0];
const ADD: [f32; 2] = [83.0, 85.0];
const ADD_SIZE: [f32; 2] = [81.0, 24.0];
// The page buttons under the goods, with "page / pages" between the inner two.
const ARROWS: [([f32; 2], [f32; 2]); 4] =
    [([217.0, 493.0], [16.0, 12.0]), ([236.0, 493.0], [11.0, 12.0]), ([290.0, 493.0], [11.0, 12.0]), ([304.0, 493.0], [16.0, 12.0])];
const PAGE_TEXT: [f32; 2] = [247.0, 494.0];
const PAGE_TEXT_WIDTH: f32 = 43.0;
const SEARCH_FIELD: [f32; 2] = [94.0, 518.0];
const SEARCH_FIELD_SIZE: [f32; 2] = [291.0, 14.0];
const SEARCH: [f32; 2] = [392.0, 516.0];
const SEARCH_SIZE: [f32; 2] = [54.0, 18.0];
const REFRESH: [f32; 2] = [452.0, 516.0];
const REFRESH_SIZE: [f32; 2] = [76.0, 18.0];
// The right column: its heading, the cash points (ending at CASH) with the charge button, the list being bought.
const HEADING: [f32; 2] = [537.0, 26.0];
const CASH: [f32; 2] = [627.0, 51.0];
const CHARGE: [f32; 2] = [654.0, 46.0];
const CHARGE_SIZE: [f32; 2] = [64.0, 20.0];
const CART: [f32; 2] = [537.0, 72.0];
const CART_SIZE: [f32; 2] = [185.0, 350.0];
const CART_ROW: [f32; 2] = [3.0, 5.0];
const CART_STEP: f32 = 57.0;
const CART_ROW_SIZE: [f32; 2] = [172.0, 54.0];
const CART_ROWS: usize = 6;
// Inside a row: the picture at half size, the name, the amount in its box with the two arrows, the price, the X.
const ROW_PICTURE: [f32; 2] = [4.0, 0.0];
const ROW_NAME: [f32; 2] = [48.0, 12.0];
const ROW_NAME_WIDTH: f32 = 112.0;
const ROW_AMOUNT: [f32; 2] = [50.0, 33.0];
const ROW_AMOUNT_WIDTH: f32 = 32.0;
const ROW_PRICE: [f32; 2] = [160.0, 34.0];
const ROW_MORE: [f32; 2] = [82.0, 29.0];
const ROW_LESS: [f32; 2] = [82.0, 38.0];
const ROW_ARROW: [f32; 2] = [19.0, 8.0];
const ROW_REMOVE: [f32; 2] = [161.0, 2.0];
const ROW_REMOVE_SIZE: [f32; 2] = [7.0, 7.0];
// The list's own thin scroll bar, down its right edge.
const BAR: f32 = 7.0;
// The points under the list: the labels start at LABELS, the numbers end at NUMBERS.
const LABELS: f32 = 550.0;
const NUMBERS: f32 = 708.0;
const POINT_LINES: [f32; 3] = [434.0, 450.0, 466.0];
const FREE_FIELD: [f32; 2] = [636.0, 448.0];
const FREE_SIZE: [f32; 2] = [62.0, 14.0];
const TOTAL: [f32; 2] = [552.0, 492.0];
const BUY: [f32; 2] = [552.0, 508.0];
const BUY_SIZE: [f32; 2] = [154.0, 23.0];
const TITLE_MESSAGE: usize = 3288;
const CART_MESSAGE: usize = 3289;
const CHARGE_MESSAGE: usize = 3299;
const BUY_MESSAGE: usize = 3300;
const SEARCH_MESSAGE: usize = 3302;
const FREE_MESSAGE: usize = 3303;
const FREE_SPEND_MESSAGE: usize = 3304;
const OWNED_MESSAGE: usize = 3305;
const REFRESH_MESSAGE: usize = 3306;
const ADD_MESSAGE: usize = 1779;
// The game's ninth button is tab 9 ("Account Limited") and its tab 8 has none; rathena's ninth tab is 8, so that one
// takes the place here.
const TAB_MESSAGES: [usize; 9] = [3290, 3291, 3292, 3293, 3294, 3295, 3296, 3297, 3298];
const GREY: [u8; 3] = [0xB1, 0xB1, 0xB1];
const WHITE: [u8; 3] = [255; 3];
const BLACK: [u8; 3] = [0; 3];
const GOOD_NAME_COLOR: [u8; 3] = [0x3B, 0x4F, 0xA4];
const ROW_NAME_COLOR: [u8; 3] = [0x18, 0x24, 0x90];
const PAGE_COLOR: [u8; 3] = [0x21, 0x29, 0x6B];
const SEARCH_LIMIT: usize = 24;

pub struct Assets {
    background: Option<Texture>,
    cart: Option<Texture>,
    cell: Option<Texture>,
    row: Option<Texture>,
    // on, off, disable
    tabs: [[Option<Texture>; 3]; 9],
    // first, previous, next, last; each off and on
    arrows: [[Option<Texture>; 2]; 4],
    // normal, over, press
    add: [Option<Texture>; 3],
    buy: [Option<Texture>; 3],
    charge: [Option<Texture>; 3],
    refresh: [Option<Texture>; 3],
    search: [Option<Texture>; 3],
    more: [Option<Texture>; 3],
    less: [Option<Texture>; 3],
    remove: Option<Texture>,
    bar: [Option<Texture>; 2],
    thumb: Option<Texture>,
    fill: Texture,
    field: Texture,
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |name: &str| gpu.try_load_texture(grf, &format!("{UI}cashshop\\{name}.bmp"), false).ok();
        // A button with no picture of its own for the pointer on it keeps its normal one.
        let pressing = |name: &str| [image(&format!("{name}_normal")), image(&format!("{name}_normal")), image(&format!("{name}_press"))];
        let three = |name: &str| ["normal", "over", "press"].map(|look| image(&format!("{name}_{look}")));
        Self {
            background: image("img_shop_bg"),
            cart: image("img_shop_cart_bg"),
            cell: image("img_shop_itembg"),
            row: image("img_shop_itembg2"),
            tabs: std::array::from_fn(|tab| ["on", "off", "disable"].map(|look| image(&format!("img_shop_tap{tab}_{look}")))),
            arrows: ["L2", "L", "R", "R2"].map(|arrow| ["off", "on"].map(|look| image(&format!("bt_arrow{arrow}_{look}")))),
            add: pressing("btn_add"),
            buy: pressing("btn_buy"),
            charge: pressing("btn_charge"),
            refresh: pressing("btn_refresh"),
            search: three("btn_searchbar"),
            more: three("btn_plus"),
            less: three("btn_minus"),
            remove: image("btn_close"),
            bar: [image("shop_scroll_up"), image("shop_scroll_down")],
            thumb: image("shop_bar_mid"),
            fill: gpu.solid([0xF5, 0xF5, 0xF5, 0xFF]),
            field: gpu.solid([0xCE, 0xD6, 0xEB, 0xFF]),
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(TitleHit),
    Tab(usize),
    /// A cell's button, by its place on the page.
    Add(usize),
    /// A page button: first, previous, next, last.
    Arrow(usize),
    SearchField,
    Search,
    Refresh,
    Charge,
    CartAmount(usize, bool),
    CartRemove(usize),
    /// The list's scroll arrows: up or down.
    CartScroll(bool),
    FreeSpend,
    Buy,
    Window,
}

#[derive(Debug, PartialEq)]
pub enum Action {
    Buy,
    Close,
    /// The search button: `search` needs the item names.
    Search,
}

pub type BuyRequest = (u32, Vec<(u32, u32, u16)>);

#[derive(Debug, PartialEq, Clone, Copy)]
enum Field {
    Free,
    Search,
}

pub struct CashShop {
    pub cash_points: u32,
    pub free_points: u32,
    pub tabs: Vec<Vec<(u32, u32)>>,
    pub cart: Vec<(u32, u32, u16, u32)>,
    pub free_to_spend: u32,
    pub frame: Frame,
    pub selected_tab: usize,
    pub page: usize,
    pub cart_scroll: usize,
    /// A text box has the keyboard: `field` says which.
    pub typing: bool,
    field: Field,
    typed_free: String,
    typed_search: String,
    /// What the last search found, shown in place of the tab's goods: item, price, tab.
    found: Option<Vec<(u32, u32, u16)>>,
}

fn cell_place(cell: usize) -> [f32; 2] {
    offset(GOODS, [(cell % GOOD_COLUMNS) as f32 * GOOD_STEP[0], (cell / GOOD_COLUMNS) as f32 * GOOD_STEP[1]])
}

fn row_place(row: usize) -> [f32; 2] {
    offset(CART, [CART_ROW[0], CART_ROW[1] + row as f32 * CART_STEP])
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
            page: 0,
            cart_scroll: 0,
            typing: false,
            field: Field::Free,
            typed_free: "0".into(),
            typed_search: String::new(),
            found: None,
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
        self.clamp_cart_scroll();
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
            self.cart_scroll = 0;
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
        self.found = None;
        self.page = 0;
        true
    }

    /// The goods the pages show: what the search found, else the tab's. Each with its price and tab.
    fn goods(&self) -> Vec<(u32, u32, u16)> {
        match &self.found {
            Some(found) => found.clone(),
            None => self.tabs[self.selected_tab].iter().map(|&(item, price)| (item, price, self.selected_tab as u16)).collect(),
        }
    }

    /// The goods on the page in front.
    fn shown(&self) -> Vec<(u32, u32, u16)> {
        self.goods().into_iter().skip(self.page * PAGE).take(PAGE).collect()
    }

    pub fn pages(&self) -> usize {
        self.goods().len().div_ceil(PAGE)
    }

    /// A page button: 0 first, 1 previous, 2 next, 3 last.
    pub fn turn(&mut self, arrow: usize) {
        let last = self.pages().saturating_sub(1);
        self.page = match arrow {
            0 => 0,
            1 => self.page.saturating_sub(1),
            2 => (self.page + 1).min(last),
            _ => last,
        };
    }

    /// Shows every tab's goods whose name holds the typed words, as the game does (capitals count); nothing typed
    /// goes back to the tab.
    pub fn search(&mut self, items: &HashMap<u32, ItemInfo>) {
        let wanted = self.typed_search.trim();
        self.found = (!wanted.is_empty()).then(|| {
            let named = |&&(item, _): &&(u32, u32)| inventory::item_name(items, item, true, 0).contains(wanted);
            self.tabs.iter().enumerate().flat_map(|(tab, list)| list.iter().filter(named).map(move |&(item, price)| (item, price, tab as u16))).collect()
        });
        self.page = 0;
    }

    /// Enter in a text box: it lets the keyboard go, and the search box searches.
    pub fn enter(&mut self, items: &HashMap<u32, ItemInfo>) {
        self.typing = false;
        if self.field == Field::Search {
            self.search(items);
        }
    }

    fn cart_most(&self) -> usize {
        self.cart.len().saturating_sub(CART_ROWS)
    }

    pub fn scroll_cart(&mut self, lines: i32) {
        self.cart_scroll = (self.cart_scroll as i64 + i64::from(lines)).clamp(0, self.cart_most() as i64) as usize;
    }

    /// The wheel turned over the window: it scrolls the list being bought.
    pub fn scroll_at(&mut self, cursor: [f32; 2], rows: i32) {
        if self.frame.local(cursor, HEIGHT).is_some_and(|local| contains(CART, CART_SIZE, local)) {
            self.scroll_cart(rows);
        }
    }

    fn clamp_cart_scroll(&mut self) {
        self.cart_scroll = self.cart_scroll.min(self.cart_most());
    }

    /// The list's scroll arrows, at the ends of its bar: up, down.
    fn bar_arrows() -> [[f32; 2]; 2] {
        let x = CART[0] + CART_SIZE[0] - BAR;
        [[x, CART[1]], [x, CART[1] + CART_SIZE[1] - BAR]]
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let local = self.frame.local(cursor, HEIGHT)?;
        // The window has no minimize button: its place is plain title bar.
        match self.frame.title_hit(local) {
            Some(TitleHit::Minimize) => return Some(Hit::Title(TitleHit::Title)),
            Some(hit) => return Some(Hit::Title(hit)),
            None => {}
        }
        for tab in 0..self.tabs.len() {
            if contains(offset(TABS, [tab as f32 * TAB_STEP, 0.0]), TAB_SIZE, local) {
                return Some(Hit::Tab(tab));
            }
        }
        for cell in 0..self.shown().len() {
            if contains(offset(cell_place(cell), ADD), ADD_SIZE, local) {
                return Some(Hit::Add(cell));
            }
        }
        for (arrow, &(place, size)) in ARROWS.iter().enumerate() {
            if contains(place, size, local) {
                return Some(Hit::Arrow(arrow));
            }
        }
        for row in 0..self.cart.len().saturating_sub(self.cart_scroll).min(CART_ROWS) {
            let place = row_place(row);
            let line = row + self.cart_scroll;
            if contains(offset(place, ROW_REMOVE), ROW_REMOVE_SIZE, local) {
                return Some(Hit::CartRemove(line));
            }
            for (spot, more) in [(ROW_MORE, true), (ROW_LESS, false)] {
                if contains(offset(place, spot), ROW_ARROW, local) {
                    return Some(Hit::CartAmount(line, more));
                }
            }
        }
        for (place, down) in Self::bar_arrows().into_iter().zip([false, true]) {
            if contains(place, [BAR, BAR], local) {
                return Some(Hit::CartScroll(down));
            }
        }
        Some(
            [
                (SEARCH_FIELD, SEARCH_FIELD_SIZE, Hit::SearchField),
                (SEARCH, SEARCH_SIZE, Hit::Search),
                (REFRESH, REFRESH_SIZE, Hit::Refresh),
                (CHARGE, CHARGE_SIZE, Hit::Charge),
                (FREE_FIELD, FREE_SIZE, Hit::FreeSpend),
                (BUY, BUY_SIZE, Hit::Buy),
            ]
            .into_iter()
            .find(|&(place, size, _)| contains(place, size, local))
            .map_or(Hit::Window, |(_, _, hit)| hit),
        )
    }

    pub fn click(&mut self, hit: Hit) -> Option<Action> {
        self.typing = matches!(hit, Hit::FreeSpend | Hit::SearchField);
        match hit {
            Hit::Title(TitleHit::Close) => return Some(Action::Close),
            Hit::Title(_) => {}
            Hit::Tab(tab) => _ = self.select_tab(tab),
            Hit::Add(cell) => {
                if let Some(&(item, price, tab)) = self.shown().get(cell) {
                    let amount = self.cart.iter().find(|line| line.0 == item && line.2 == tab).map_or(1, |line| line.1.saturating_add(1));
                    self.add_to_cart(item, amount.min(99), tab, price);
                }
            }
            Hit::Arrow(arrow) => self.turn(arrow),
            Hit::SearchField => self.field = Field::Search,
            Hit::Search => return Some(Action::Search),
            // ponytail: the game's Refresh wasn't read; here it drops the search and shows the tab again.
            Hit::Refresh => {
                self.typed_search.clear();
                self.found = None;
                self.page = 0;
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
            Hit::CartRemove(row) => {
                if let Some(&(item, _, tab, _)) = self.cart.get(row) {
                    self.remove(item, tab);
                }
            }
            Hit::CartScroll(down) => self.scroll_cart(if down { 1 } else { -1 }),
            Hit::FreeSpend => self.field = Field::Free,
            Hit::Buy if !self.cart.is_empty() => return Some(Action::Buy),
            // ponytail: Charge opens the server's payment page in the game; it does nothing here.
            Hit::Charge | Hit::Buy | Hit::Window => {}
        }
        None
    }

    pub fn type_text(&mut self, text: &str) {
        if self.field == Field::Search {
            let room = SEARCH_LIMIT.saturating_sub(self.typed_search.chars().count());
            self.typed_search.extend(text.chars().filter(|letter| !letter.is_control()).take(room));
            return;
        }
        let room = 10usize.saturating_sub(self.typed_free.len());
        let more: String = text.chars().filter(char::is_ascii_digit).take(room).collect();
        self.typed_free.push_str(&more);
        let amount = self.typed_free.parse().unwrap_or(0);
        self.set_free_spend(amount);
    }

    pub fn backspace(&mut self) {
        if self.field == Field::Search {
            self.typed_search.pop();
            return;
        }
        self.typed_free.pop();
        let amount = self.typed_free.parse().unwrap_or(0);
        self.set_free_spend(amount);
    }

    /// The pictures to load: the page's goods and the list's.
    pub fn icons(&self, items: &HashMap<u32, ItemInfo>) -> Vec<String> {
        let shown = self.shown();
        shown
            .iter()
            .map(|line| line.0)
            .chain(self.cart.iter().map(|line| line.0))
            .filter_map(|id| items.get(&id).map(|info| picture_path(info, true)))
            .collect()
    }

    /// Each good on the page with its button, the one that puts it on the list.
    pub fn item_boxes(&self) -> Vec<(u32, [f32; 4])> {
        self.shown()
            .iter()
            .enumerate()
            .map(|(cell, &(item, ..))| {
                let place = self.frame.at(offset(cell_place(cell), ADD));
                (item, [place[0], place[1], ADD_SIZE[0], ADD_SIZE[1]])
            })
            .collect()
    }

    /// The buy button, and the title bar's close button: the window has no other.
    pub fn button_boxes(&self) -> [(&'static str, [f32; 4]); 2] {
        let buy = self.frame.at(BUY);
        let close = self.frame.at(self.frame.close_button());
        [("cashshop_buy", [buy[0], buy[1], BUY_SIZE[0], BUY_SIZE[1]]), ("cashshop_close", [close[0], close[1], TITLE_BUTTON[0], TITLE_BUTTON[1]])]
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
        messages: &[String],
        font: &Font,
        pointer: ([f32; 2], bool),
        caret: bool,
    ) {
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        let at = |local| self.frame.at(local);
        let picture = |item: u32| items.get(&item).and_then(|info| icons.get(&picture_path(info, true))).and_then(Option::as_ref);
        // Text in the middle of a `width`, or ending at `place`.
        let centred = |text: &str, place: [f32; 2], width: f32| at(offset(place, [((width - font.width(text)) / 2.0).floor(), 0.0]));
        let ending = |text: &str, place: [f32; 2]| at(offset(place, [-font.width(text), 0.0]));
        let hit = self.hit(pointer.0);
        // A button's picture: normal, under the pointer, held down.
        let look = |target: Hit| usize::from(hit == Some(target)) + usize::from(hit == Some(target) && pointer.1);
        match &assets.background {
            Some(background) => layer.sprites.push((background, at([0.0, 0.0]), [WIDTH, HEIGHT])),
            None => layer.sprites.push((&assets.fill, at([0.0, 0.0]), [WIDTH, HEIGHT])),
        }
        // The background brings its own title bar: the base and close buttons and the title go on it.
        let base = chrome.button(0, contains(at(BASE_BUTTON), TITLE_BUTTON, pointer.0));
        layer.sprites.push((base, at(BASE_BUTTON), base.size));
        self.frame.draw_bare_title(layer, chrome, message(TITLE_MESSAGE), pointer.0);

        for (tab, &name) in TAB_MESSAGES.iter().enumerate().take(self.tabs.len()) {
            let place = offset(TABS, [tab as f32 * TAB_STEP, 0.0]);
            let state = if self.tabs[tab].is_empty() {
                2
            } else if tab == self.selected_tab && self.found.is_none() {
                0
            } else {
                1
            };
            if let Some(texture) = &assets.tabs[tab][state] {
                layer.sprites.push((texture, at(place), TAB_SIZE));
            }
            if hit == Some(Hit::Tab(tab)) {
                crate::window::tooltip(tooltip, chrome, font, message(name), at(offset(place, [0.0, -20.0])), WHITE);
            }
        }

        for (cell, &(item, price, _)) in self.shown().iter().enumerate() {
            let place = cell_place(cell);
            layer.sprites.push((assets.cell.as_ref().unwrap_or(&assets.fill), at(place), GOOD_SIZE));
            let name = inventory::item_name(items, item, true, 0);
            // The top 24 px of a cell show its whole name, 20 px above the cell.
            if contains(at(place), [GOOD_SIZE[0], 24.0], pointer.0) {
                crate::window::tooltip(tooltip, chrome, font, &name, at(offset(place, [0.0, -20.0])), WHITE);
            }
            let name = crate::window::fit(font, &name, GOOD_NAME_WIDTH);
            layer.texts.push((name.clone(), centred(&name, offset(place, GOOD_NAME), GOOD_SIZE[0]), GOOD_NAME_COLOR));
            if let Some(picture) = picture(item) {
                layer.sprites.push((picture, at(offset(place, GOOD_PICTURE)), picture.size));
            }
            let price = group(u64::from(price));
            layer.texts.push((price.clone(), ending(&price, offset(place, GOOD_PRICE)), WHITE));
            let button = offset(place, ADD);
            if let Some(image) = &assets.add[look(Hit::Add(cell))] {
                layer.sprites.push((image, at(button), ADD_SIZE));
            }
            let label = message(ADD_MESSAGE);
            layer.texts.push((label.to_owned(), centred(label, offset(button, [0.0, ((ADD_SIZE[1] - font.line_height()) / 2.0).floor()]), ADD_SIZE[0]), BLACK));
        }
        for (arrow, &(place, size)) in ARROWS.iter().enumerate() {
            if let Some(image) = &assets.arrows[arrow][usize::from(hit == Some(Hit::Arrow(arrow)))] {
                layer.sprites.push((image, at(place), size));
            }
        }
        let pages = self.pages();
        let page = if pages == 0 { "0 / 0".to_owned() } else { format!("{} / {pages}", self.page + 1) };
        layer.texts.push((page.clone(), centred(&page, PAGE_TEXT, PAGE_TEXT_WIDTH), PAGE_COLOR));

        // The search box is in the background; nothing typed shows what it is for, in grey.
        let searching = self.typing && self.field == Field::Search;
        let (typed, color) = match (self.typed_search.is_empty() && !searching, searching && caret) {
            (true, _) => (message(SEARCH_MESSAGE).to_owned(), GREY),
            (false, true) => (format!("{}|", self.typed_search), BLACK),
            (false, false) => (self.typed_search.clone(), BLACK),
        };
        layer.texts.push((typed, at(offset(SEARCH_FIELD, [2.0, 0.0])), color));
        for (target, place, size, images, label) in [
            (Hit::Search, SEARCH, SEARCH_SIZE, &assets.search, ""),
            (Hit::Refresh, REFRESH, REFRESH_SIZE, &assets.refresh, message(REFRESH_MESSAGE)),
            (Hit::Charge, CHARGE, CHARGE_SIZE, &assets.charge, message(CHARGE_MESSAGE)),
            (Hit::Buy, BUY, BUY_SIZE, &assets.buy, message(BUY_MESSAGE)),
        ] {
            match &images[look(target)] {
                Some(image) => layer.sprites.push((image, at(place), size)),
                None => layer.sprites.push((&chrome.border, at(place), size)),
            }
            layer.texts.push((label.to_owned(), centred(label, offset(place, [0.0, ((size[1] - font.line_height()) / 2.0).floor()]), size[0]), BLACK));
        }

        // The heading over its own shadow, and the cash points beside the coin.
        let heading = message(CART_MESSAGE);
        layer.texts.push((heading.to_owned(), centred(heading, offset(HEADING, [1.0, 1.0]), CART_SIZE[0]), BLACK));
        layer.texts.push((heading.to_owned(), centred(heading, HEADING, CART_SIZE[0]), WHITE));
        let cash = group(u64::from(self.cash_points));
        layer.texts.push((cash.clone(), ending(&cash, CASH), WHITE));

        layer.sprites.push((assets.cart.as_ref().unwrap_or(&assets.fill), at(CART), CART_SIZE));
        for (row, &(item, amount, _, price)) in self.cart.iter().skip(self.cart_scroll).take(CART_ROWS).enumerate() {
            let place = row_place(row);
            let line = row + self.cart_scroll;
            layer.sprites.push((assets.row.as_ref().unwrap_or(&assets.fill), at(place), CART_ROW_SIZE));
            if let Some(picture) = picture(item) {
                layer.sprites.push((picture, at(offset(place, ROW_PICTURE)), [(picture.size[0] / 2.0).floor(), (picture.size[1] / 2.0).floor()]));
            }
            let name = inventory::item_name(items, item, true, 0);
            // The name strip shows the whole name, 10 px above the row.
            if contains(at(offset(place, ROW_NAME)), [ROW_NAME_WIDTH, 12.0], pointer.0) {
                crate::window::tooltip(tooltip, chrome, font, &name, at(offset(place, [0.0, -10.0])), WHITE);
            }
            layer.texts.push((crate::window::fit(font, &name, ROW_NAME_WIDTH), at(offset(place, ROW_NAME)), ROW_NAME_COLOR));
            let amount = amount.to_string();
            layer.texts.push((amount.clone(), centred(&amount, offset(place, ROW_AMOUNT), ROW_AMOUNT_WIDTH), BLACK));
            let price = format!("{price} C");
            layer.texts.push((price.clone(), ending(&price, offset(place, ROW_PRICE)), BLACK));
            for (spot, more, images) in [(ROW_MORE, true, &assets.more), (ROW_LESS, false, &assets.less)] {
                if let Some(image) = &images[look(Hit::CartAmount(line, more))] {
                    layer.sprites.push((image, at(offset(place, spot)), ROW_ARROW));
                }
            }
            if let Some(image) = &assets.remove {
                layer.sprites.push((image, at(offset(place, ROW_REMOVE)), ROW_REMOVE_SIZE));
            }
        }
        // ponytail: the bar's arrows and the wheel scroll the list; its thumb shows the place but can't be dragged.
        for (image, place) in assets.bar.iter().zip(Self::bar_arrows()) {
            if let Some(image) = image {
                layer.sprites.push((image, at(place), [BAR, BAR]));
            }
        }
        if let (Some(thumb), most @ 1..) = (&assets.thumb, self.cart_most()) {
            let track = CART_SIZE[1] - 2.0 * BAR;
            let length = (track * CART_ROWS as f32 / self.cart.len() as f32).floor();
            let top = ((track - length) * self.cart_scroll as f32 / most as f32).floor();
            layer.sprites.push((thumb, at([CART[0] + CART_SIZE[0] - BAR, CART[1] + BAR + top]), [BAR, length]));
        }

        // Free points, how many of them to spend (typed), and the cash points owned.
        let points = |value: u32| format!("{} C", group(u64::from(value)));
        for (id, number, y) in [
            (FREE_MESSAGE, points(self.free_points), POINT_LINES[0]),
            (FREE_SPEND_MESSAGE, "C".to_owned(), POINT_LINES[1]),
            (OWNED_MESSAGE, points(self.cash_points), POINT_LINES[2]),
        ] {
            layer.texts.push((message(id).to_owned(), at([LABELS, y]), BLACK));
            layer.texts.push((number.clone(), ending(&number, [NUMBERS, y]), BLACK));
        }
        layer.sprites.push((&assets.field, at(FREE_FIELD), FREE_SIZE));
        let typed = if self.typing && self.field == Field::Free && caret { format!("{}|", self.typed_free) } else { self.typed_free.clone() };
        layer.texts.push((typed, at(offset(FREE_FIELD, [2.0, 0.0])), BLACK));
        let total = points(self.cart_total());
        layer.texts.push((total.clone(), centred(&total, TOTAL, BUY_SIZE[0]), WHITE));
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
    fn empty_tabs_are_disabled() {
        let mut shop = CashShop::new(100, 50);
        assert!(!shop.select_tab(2));
        shop.append(2, vec![(501, 10)]);
        assert!(shop.select_tab(2));
    }

    fn stock(shop: &mut CashShop, tab: usize, count: u32) {
        shop.append(tab as i16, (0..count).map(|index| (1000 + index, 10)).collect());
    }

    #[test]
    fn pages_hold_nine_goods_and_turn_within_their_ends() {
        let mut shop = CashShop::new(0, 0);
        assert_eq!(shop.pages(), 0);
        stock(&mut shop, 0, 20);
        assert_eq!(shop.pages(), 3);
        shop.turn(1);
        assert_eq!(shop.page, 0);
        shop.turn(2);
        assert_eq!((shop.page, shop.item_boxes().len()), (1, PAGE));
        shop.turn(3);
        assert_eq!((shop.page, shop.item_boxes().len()), (2, 2));
        shop.turn(2);
        assert_eq!(shop.page, 2);
        shop.turn(0);
        assert_eq!(shop.page, 0);
    }

    #[test]
    fn cart_scroll_max_is_lines_beyond_visible() {
        let mut shop = CashShop::new(0, 0);
        shop.cart = (0..9).map(|index| (500 + index, 1, 0, 10)).collect();
        shop.scroll_cart(10);
        assert_eq!(shop.cart_scroll, 3);
        shop.click(Hit::CartScroll(false));
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
        shop.cart = (0..9).map(|index| (500 + index, 1, 0, 10)).collect();
        shop.cart_scroll = 3;
        shop.click(Hit::CartRemove(8));
        assert_eq!((shop.cart.len(), shop.cart_scroll), (8, 2));
        shop.remove(507, 0);
        shop.remove(506, 0);
        assert_eq!(shop.cart_scroll, 0);
    }

    #[test]
    fn select_tab_goes_to_its_first_page() {
        let mut shop = CashShop::new(0, 0);
        stock(&mut shop, 0, 20);
        stock(&mut shop, 1, 20);
        shop.page = 2;
        assert!(shop.select_tab(1));
        assert_eq!(shop.page, 0);
    }

    /// The window's pieces sit where the game puts them on its 723 x 540 background.
    #[test]
    fn pieces_sit_at_the_games_places() {
        let mut shop = CashShop::new(0, 0);
        stock(&mut shop, 0, 20);
        shop.cart = vec![(501, 1, 0, 10), (502, 1, 0, 10)];
        let hit = |shop: &CashShop, x: f32, y: f32| shop.hit(shop.frame.at([x, y]));
        // Tabs: 56 x 31, 58 apart from (8, 72).
        assert_eq!(hit(&shop, 8.0, 72.0), Some(Hit::Tab(0)));
        assert_eq!(hit(&shop, 8.0 + 58.0 * 8.0 + 55.0, 102.0), Some(Hit::Tab(8)));
        assert_eq!(hit(&shop, 65.0, 80.0), Some(Hit::Window));
        // Cells: 175 x 127 apart from (8, 107), the button at (83, 85) in each.
        assert_eq!(hit(&shop, 8.0 + 83.0, 107.0 + 85.0), Some(Hit::Add(0)));
        assert_eq!(hit(&shop, 8.0 + 175.0 * 2.0 + 163.0, 107.0 + 127.0 * 2.0 + 108.0), Some(Hit::Add(8)));
        assert_eq!(hit(&shop, 20.0, 150.0), Some(Hit::Window));
        // The page buttons, the search row, the right column.
        assert_eq!([217.0, 236.0, 290.0, 304.0].map(|x| hit(&shop, x, 493.0)), [0, 1, 2, 3].map(|arrow| Some(Hit::Arrow(arrow))));
        assert_eq!(hit(&shop, 94.0, 518.0), Some(Hit::SearchField));
        assert_eq!(hit(&shop, 392.0, 516.0), Some(Hit::Search));
        assert_eq!(hit(&shop, 452.0, 516.0), Some(Hit::Refresh));
        assert_eq!(hit(&shop, 654.0, 46.0), Some(Hit::Charge));
        assert_eq!(hit(&shop, 636.0, 448.0), Some(Hit::FreeSpend));
        assert_eq!(hit(&shop, 552.0, 508.0), Some(Hit::Buy));
        // The list's rows: 57 apart from (540, 77); more at (82, 29), less at (82, 38), the X at (161, 2).
        assert_eq!(hit(&shop, 540.0 + 82.0, 77.0 + 29.0), Some(Hit::CartAmount(0, true)));
        assert_eq!(hit(&shop, 540.0 + 82.0, 77.0 + 57.0 + 38.0), Some(Hit::CartAmount(1, false)));
        assert_eq!(hit(&shop, 540.0 + 161.0, 77.0 + 57.0 + 2.0), Some(Hit::CartRemove(1)));
        assert_eq!(hit(&shop, 540.0 + 82.0, 77.0 + 114.0 + 29.0), Some(Hit::Window));
        // No minimize button: its place is title bar. The close button closes.
        assert_eq!(hit(&shop, 723.0 - 28.0, 5.0), Some(Hit::Title(TitleHit::Title)));
        assert_eq!(hit(&shop, 723.0 - 12.0, 5.0), Some(Hit::Title(TitleHit::Close)));
        assert_eq!(shop.click(Hit::Title(TitleHit::Close)), Some(Action::Close));
        assert_eq!(hit(&shop, 723.0, 5.0), None);
    }

    #[test]
    fn a_cells_button_adds_that_pages_good() {
        let mut shop = CashShop::new(0, 0);
        stock(&mut shop, 0, 20);
        shop.turn(2);
        let place = shop.item_boxes()[0].1;
        let hit = shop.hit([place[0] + 5.0, place[1] + 5.0]);
        assert_eq!(hit, Some(Hit::Add(0)));
        shop.click(hit.unwrap());
        shop.click(hit.unwrap());
        assert_eq!(shop.cart, [(shop.tabs[0][PAGE].0, 2, 0, 10)]);
    }

    #[test]
    fn search_shows_every_tabs_matches_until_a_tab_or_refresh() {
        // Without item names an item is known by its number.
        let items = HashMap::new();
        let mut shop = CashShop::new(0, 0);
        shop.append(0, vec![(501, 10), (1201, 20)]);
        shop.append(3, vec![(2501, 30)]);
        shop.click(Hit::SearchField);
        assert!(shop.typing);
        shop.type_text("50x");
        shop.backspace();
        shop.enter(&items);
        assert!(!shop.typing);
        assert_eq!(shop.shown(), [(501, 10, 0), (2501, 30, 3)]);
        // A found good goes on the list under its own tab.
        shop.click(Hit::Add(1));
        assert_eq!(shop.cart, [(2501, 1, 3, 30)]);
        assert_eq!(shop.click(Hit::Search), Some(Action::Search));
        shop.click(Hit::Refresh);
        assert_eq!(shop.shown(), [(501, 10, 0), (1201, 20, 0)]);
        shop.type_text("9");
        shop.search(&items);
        assert!(shop.shown().is_empty());
        assert!(shop.select_tab(3));
        assert_eq!(shop.shown(), [(2501, 30, 3)]);
    }

    #[test]
    fn typing_goes_to_the_box_that_was_clicked() {
        let mut shop = CashShop::new(100, 50);
        shop.cart.push((501, 3, 0, 10));
        shop.click(Hit::FreeSpend);
        shop.type_text("2a5");
        assert_eq!(shop.free_to_spend, 25);
        shop.click(Hit::SearchField);
        shop.type_text("Red 5");
        assert_eq!((shop.typed_search.as_str(), shop.free_to_spend), ("Red 5", 25));
        shop.click(Hit::Window);
        assert!(!shop.typing);
    }

    #[test]
    fn unknown_buy_result_maps_to_1228() {
        assert_eq!(buy_result_message(99), 1228);
    }
}
