//! Kafra storage: one stored stack a row with its icon, amount and name, the client's eight tabs down the left, its
//! count and a search field, and boxes that open a small window for each tab's category. Items move in and out by
//! dragging them between it (or a category window) and the inventory.

use std::collections::HashMap;

use rustnarok_client::grf::Grf;
use rustnarok_client::lua::ItemInfo;
use rustnarok_client::net::InventoryItem;
use rustnarok_client::text::Font;

use crate::inventory::{self, CardNames};
use crate::render::{Gpu, Texture};
use crate::window::{self, Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI, contains, offset};

const WIDTH: f32 = 280.0;
const CELL: f32 = 32.0;
/// Rows: 12 at the game's least height (458), one more for each 32 px the corner grip adds (it only changes the
/// height), as far as 0.7 of the screen's height. Rows = (h − 74) / 32: the title bar, the rows, the 57 px footer.
pub const LEAST_ROWS: usize = 12;
const LEAST_HEIGHT: f32 = 458.0;
const CHROME_HEIGHT: f32 = 74.0;
const FOOTER_HEIGHT: f32 = 57.0;
/// The side pieces down both edges (every 8 px from y 17), the tab strip over the left one, white beside it. The strip
/// is `tab_item_0n.bmp` (n the picked tab + 1): eight tabs of 29 px.
const SIDE: f32 = 20.0;
const TABS_AT: [f32; 2] = [0.0, 18.0];
const TABS_SIZE: [f32; 2] = [20.0, 235.0];
const TAB_HEIGHT: f32 = 29.0;
pub const TABS: usize = 8;
const STRIP_WHITE: f32 = 28.0;
/// Row r: the cell at (40, 17 + 32r) with white beside it to x 264, the icon 4 in, the name at (80, 29 + 32r), 180
/// wide, or on two lines at 23 + 32r and 36 + 32r; the name box 16 px above the cell.
const CELL_X: f32 = 40.0;
const ROWS_TOP: f32 = TITLE_HEIGHT;
const ROW_WHITE: [f32; 2] = [72.0, 264.0];
const ICON_OFFSET: f32 = 4.0;
const NAME_X: f32 = 80.0;
const NAME_TOP: f32 = 12.0;
const NAME_LINES: [f32; 2] = [6.0, 19.0];
const NAME_WIDTH: f32 = 180.0;
const NAME_SHADOW: [u8; 3] = [0xE0, 0xE0, 0xE0];
/// A name's colour by enchants: one is #669900 here, the rest as the inventory's name box.
const ONE_ENCHANT: [u8; 3] = [0x66, 0x99, 0x00];
const NAME_BOX_ABOVE: f32 = 16.0;
/// The footer's bottom row, up from the window's bottom: the count's icon and text, the search box (moved right once
/// the height has changed), its button, Close.
const COUNT_ICON: [f32; 2] = [6.0, 21.0];
const COUNT: [f32; 2] = [22.0, 19.0];
const SEARCH: [f32; 2] = [65.0, 23.0];
const SEARCH_MOVED: f32 = 77.0;
const SEARCH_SIZE: [f32; 2] = [121.0, 18.0];
const FIND: [f32; 2] = [185.0, 23.0];
const CLOSE: [f32; 2] = [220.0, 23.0];
const CLOSE_SIZE: [f32; 2] = [42.0, 20.0];
const COUNT_COLOR: [u8; 3] = [0x23, 0x23, 0x23];
const FULL_COLOR: [u8; 3] = [0xFF, 0x00, 0x00];
/// The wheel moves this many rows a notch (one less than the rows when fewer).
const WHEEL_ROWS: usize = 3;

/// The most rows at a screen `height` tall: 458 + 32k up to ⌊0.7·height⌋, only 12 under 700.
pub fn most_rows(height: f32) -> usize {
    LEAST_ROWS + window::steps((0.7 * height).floor(), LEAST_HEIGHT, CELL, usize::MAX)
}

/// The window's height with `rows` rows.
fn height(rows: usize) -> f32 {
    CHROME_HEIGHT + rows as f32 * CELL
}
const DEFAULT_POSITION: [f32; 2] = [360.0, 200.0];
const TITLE_MESSAGE: usize = 169;
/// The footer's category boxes, Use to Etc: 23 × 23 at (5 + 26i, h − 52), `<name>-1.bmp` on and `<name>-2.bmp` off.
const BOX_AT: [f32; 2] = [5.0, 52.0];
const BOX_STEP: f32 = 26.0;
const BOX_SIZE: f32 = 23.0;
const BOX_IMAGES: [&str; TABS] = ["소비", "캐시", "방어구", "무기", "의상", "투사체", "카드", "기타"];
/// A category window: its title, where it first opens, and its size at every opening (as far as 320 × 336 by its
/// grip). The title bar, a white body w × (h − 38) from y 17 and a 21 px footer; row r's cell at (10, 17 + 32r), the
/// icon 4 in, the name at (50, 29 + 32r) cut to w − 76, the grade at (14, 33 + 32r); the pointer takes x 20 to 52.
const CATEGORY_TITLES: [usize; TABS] = [1465, 1466, 1467, 1468, 3151, 1469, 1470, 1471];
const CATEGORY_PLACES: [[f32; 2]; TABS] =
    [[210.0, 50.0], [60.0, 50.0], [210.0, 205.0], [60.0, 205.0], [210.0, 360.0], [60.0, 360.0], [210.0, 515.0], [60.0, 515.0]];
const CATEGORY_SIZE: [f32; 2] = [140.0, 145.0];
const CATEGORY_MOST: [f32; 2] = [320.0, 336.0];
const CATEGORY_FOOTER: f32 = 21.0;
const CATEGORY_CELL_X: f32 = 10.0;
const CATEGORY_NAME_X: f32 = 50.0;
const CATEGORY_POINTER: [f32; 2] = [20.0, 52.0];

pub struct Assets {
    footer: [Texture; 3],
    /// A category window's footer.
    bar: [Texture; 3],
    side: [Texture; 2],
    /// The tab strip with each tab picked, and each category's box on and off.
    tabs: Vec<Texture>,
    boxes: Vec<[Option<Texture>; 2]>,
    cell: Texture,
    count: Texture,
    search_box: Texture,
    search: [Texture; 3],
    close: [Texture; 3],
    resize: Texture,
    grades: [Texture; 4],
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let basic = format!("{UI}basic_interface\\");
        let image = |path: String| gpu.load_texture(grf, &path, false);
        let maybe = |path: String| gpu.try_load_texture(grf, &path, false).ok();
        Self {
            footer: ["left", "mid", "right"].map(|part| image(format!("{basic}btnbar_{part}3.bmp"))),
            bar: ["left", "mid", "right"].map(|part| image(format!("{basic}btnbar_{part}.bmp"))),
            side: ["left", "right"].map(|part| image(format!("{basic}itemwin_{part}.bmp"))),
            tabs: (1..=TABS).filter_map(|tab| maybe(format!("{basic}tab_item_0{tab}.bmp"))).collect(),
            boxes: BOX_IMAGES.iter().map(|name| [1, 2].map(|look| maybe(format!("{basic}{name}-{look}.bmp")))).collect(),
            cell: image(format!("{basic}itemwin_mid.bmp")),
            count: image(format!("{UI}inventory\\icon_num.bmp")),
            search_box: image(format!("{UI}bg_search.bmp")),
            search: ["normal", "over", "press"].map(|look| image(format!("{UI}bt_search_{look}.bmp"))),
            close: ["", "_a", "_b"].map(|look| image(format!("{basic}btn_close{look}.bmp"))),
            resize: image(format!("{UI}btn_resize.bmp")),
            grades: std::array::from_fn(|grade| image(format!("{UI}grade_enchant\\grade_icon{}.bmp", grade + 1))),
        }
    }
}

/// The tab an item is listed under, of the eight (Use, Cash, Armors, Weapons, Costumes, Ammo, Card, Etc): by the type
/// byte as the server sends it, and a costume (iteminfo's `costume`) on Costumes only, whatever its type. Other types
/// are listed nowhere.
fn tab_of(item: &InventoryItem, info: &HashMap<u32, ItemInfo>) -> Option<usize> {
    if info.get(&item.id).is_some_and(|info| info.costume) {
        return Some(4);
    }
    match item.kind {
        0..=2 => Some(0),
        18 => Some(1),
        4 | 11..=14 => Some(2),
        5 | 8 | 9 | 15 => Some(3),
        10 | 16 | 17 | 19 => Some(5),
        6 => Some(6),
        3 | 7 => Some(7),
        _ => None,
    }
}

/// The tab under a point in the window: eight bands of 29 px down the strip.
fn tab_at(local: [f32; 2]) -> Option<usize> {
    contains(TABS_AT, TABS_SIZE, local).then(|| (((local[1] - TABS_AT[1]) / TAB_HEIGHT) as usize).min(TABS - 1))
}

/// A category window's size for the size its grip asks: 140 to 300 wide in 32 px steps, 145 to 305 tall in steps of
/// 32 from 160.
fn category_size([width, height]: [f32; 2]) -> [f32; 2] {
    let [width, height] = [width.clamp(CATEGORY_SIZE[0], CATEGORY_MOST[0]), height.clamp(CATEGORY_SIZE[1], CATEGORY_MOST[1])];
    [CATEGORY_SIZE[0] + CELL * ((width - CATEGORY_SIZE[0]) / CELL).floor(), CATEGORY_SIZE[1] + CELL * ((height - 160.0) / CELL).trunc()]
}

/// A small window listing one tab's stored items, opened by its box in the storage's footer.
pub struct Category {
    pub frame: Frame,
    size: [f32; 2],
    scroll: usize,
    grip: window::Grip,
}

impl Category {
    fn new(tab: usize) -> Self {
        Self { frame: Frame::new(CATEGORY_PLACES[tab], CATEGORY_SIZE[0]), size: CATEGORY_SIZE, scroll: 0, grip: window::Grip::default() }
    }

    /// Opens at 140 × 145 where it was last, at the top.
    fn open(&mut self) {
        self.size = CATEGORY_SIZE;
        self.frame.set_width(CATEGORY_SIZE[0]);
        self.scroll = 0;
        self.frame.open = true;
        self.frame.minimized = false;
    }

    pub fn height(&self) -> f32 {
        self.size[1]
    }

    fn rows(&self) -> usize {
        ((self.size[1] - 38.0) / CELL).floor() as usize
    }

    /// The row under a point in the window: x 20 to 52, 10 px right of the drawn cell.
    fn row_at(&self, local: [f32; 2]) -> Option<usize> {
        let row = ((local[1] - ROWS_TOP) / CELL).floor();
        let across = [CATEGORY_POINTER[0], ROWS_TOP];
        (contains(across, [CATEGORY_POINTER[1] - CATEGORY_POINTER[0], self.rows() as f32 * CELL], local) && row >= 0.0).then_some(row as usize)
    }

    pub fn on_grip(&self, cursor: [f32; 2]) -> bool {
        self.frame.open && self.frame.local(cursor, self.height()).is_some_and(|local| window::on_grip(self.size, local))
    }

    pub fn start_resize(&mut self, cursor: [f32; 2]) {
        self.grip.start(cursor, self.size);
    }

    /// While held, both ways in the game's steps, back at the top; whether it's held.
    pub fn resize_to(&mut self, cursor: [f32; 2]) -> bool {
        let Some(asked) = self.grip.asked(cursor) else { return false };
        self.size = category_size(asked);
        self.frame.set_width(self.size[0]);
        self.scroll = 0;
        true
    }

    pub fn stop_resize(&mut self) -> bool {
        self.grip.stop()
    }
}

/// A row's name: one line when it fits 180 px, else cut by character into two, the second ending in "..." when it
/// still doesn't fit. Each line with where it starts in the name.
fn name_lines(font: &Font, name: &str) -> Vec<(usize, String)> {
    if font.width(name) <= NAME_WIDTH {
        return vec![(0, name.to_owned())];
    }
    let mut cut = 0;
    for (at, character) in name.char_indices() {
        if font.width(&name[..at + character.len_utf8()]) > NAME_WIDTH {
            break;
        }
        cut = at + character.len_utf8();
    }
    vec![(0, name[..cut].to_owned()), (cut, window::fit(font, &name[cut..], NAME_WIDTH))]
}

/// A name's colour by its enchants: black, #669900 for one, then the name box's.
fn name_color(enchants: usize) -> [u8; 3] {
    match enchants {
        0 => [0, 0, 0],
        1 => ONE_ENCHANT,
        count => inventory::ENCHANTED[count.min(4) - 1],
    }
}

/// A line's pieces in their colours: the maker's name (a byte range of the whole name) in its own.
fn line_pieces(start: usize, line: &str, maker: Option<&inventory::MakerRun>, color: [u8; 3]) -> Vec<(String, [u8; 3])> {
    let Some((range, maker_color)) = maker else { return vec![(line.to_owned(), color)] };
    let local = |at: usize| at.saturating_sub(start).min(line.len());
    let (from, to) = (local(range.start), local(range.end));
    if !line.is_char_boundary(from) || !line.is_char_boundary(to) {
        return vec![(line.to_owned(), color)];
    }
    [(&line[..from], color), (&line[from..to], *maker_color), (&line[to..], color)]
        .into_iter()
        .filter(|(text, _)| !text.is_empty())
        .map(|(text, color)| (text.to_owned(), color))
        .collect()
}

/// The count's colour: red once the storage is full.
fn count_color([count, most]: [u16; 2]) -> [u8; 3] {
    if count >= most { FULL_COLOR } else { COUNT_COLOR }
}

/// The storage's title: the name the server opened it with, else "Storage".
pub fn title<'a>(name: &'a str, messages: &'a [String]) -> &'a str {
    if name.is_empty() { messages.get(TITLE_MESSAGE).map_or("", String::as_str) } else { name }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(TitleHit),
    Tab(usize),
    /// A stored item's icon, by storage index.
    Item(u16),
    Search,
    Find,
    Close,
    /// A category's box in the footer.
    Box(usize),
    /// The corner grip.
    Resize,
    Window,
}

pub struct Storage {
    pub items: Vec<InventoryItem>,
    /// How many kinds are stored, and how many can be.
    pub count: [u16; 2],
    pub frame: Frame,
    /// The name the server opened it with (empty: "Storage").
    pub name: String,
    /// The open tab, of the strip's.
    tab: usize,
    scroll: usize,
    /// What's typed in the search field, and whether typing goes there.
    pub search: String,
    pub typing: bool,
    /// Rows shown, and the corner grip while held.
    rows: usize,
    grip: window::Grip,
    /// The height has changed since it opened (or opened taller): the search box sits further right.
    resized: bool,
    /// Each tab's category window, and which are on (a bit each, Use first): those open with the storage.
    pub categories: [Category; TABS],
    pub mask: u8,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            count: [0; 2],
            frame: Frame::new(DEFAULT_POSITION, WIDTH),
            name: String::new(),
            tab: 0,
            scroll: 0,
            search: String::new(),
            typing: false,
            rows: LEAST_ROWS,
            grip: window::Grip::default(),
            resized: false,
            categories: std::array::from_fn(Category::new),
            mask: 0,
        }
    }
}

impl Storage {
    /// The server starts listing it: the window opens on its first tab, under the name it gave, with the category
    /// windows that were on.
    pub fn open(&mut self, name: String) {
        let frame = std::mem::replace(&mut self.frame, Frame::new(DEFAULT_POSITION, WIDTH));
        let categories = std::mem::replace(&mut self.categories, std::array::from_fn(Category::new));
        *self = Self { frame, categories, mask: self.mask, rows: self.rows, name, resized: self.rows != LEAST_ROWS, ..Self::default() };
        self.frame.open = true;
        self.frame.minimized = false;
        for (tab, category) in self.categories.iter_mut().enumerate() {
            if self.mask & 1 << tab != 0 {
                category.open();
            }
        }
    }

    /// Close (or its title's close): every category window shuts with it, and they open again with it.
    pub fn close(&mut self) {
        self.frame.open = false;
        self.typing = false;
        for category in &mut self.categories {
            category.frame.open = false;
        }
    }

    /// A category's box pressed: on opens its window at 140 × 145, off shuts it.
    pub fn toggle_category(&mut self, tab: usize) {
        self.mask ^= 1 << tab;
        if self.mask & 1 << tab != 0 {
            self.categories[tab].open();
        } else {
            self.categories[tab].frame.open = false;
        }
    }

    /// Esc on a category window: it shuts and its box goes off.
    pub fn close_category(&mut self, tab: usize) {
        self.mask &= !(1 << tab);
        self.categories[tab].frame.open = false;
    }

    /// The opening lists, in the order they come.
    pub fn extend(&mut self, items: Vec<InventoryItem>) {
        self.items.extend(items);
    }

    /// An item stored: onto its stack where it is, or a new stack at the end.
    pub fn add(&mut self, item: InventoryItem) {
        match self.items.iter_mut().find(|stored| stored.index == item.index) {
            Some(stored) => stored.amount = stored.amount.saturating_add(item.amount),
            None => self.items.push(item),
        }
    }

    pub fn remove(&mut self, index: u16, amount: u32) {
        if let Some(position) = self.items.iter().position(|item| item.index == index) {
            let item = &mut self.items[position];
            item.amount = item.amount.saturating_sub(amount.min(u32::from(u16::MAX)) as u16);
            if item.amount == 0 {
                self.items.remove(position);
            }
        }
    }

    pub fn get(&self, index: u16) -> Option<&InventoryItem> {
        self.items.iter().find(|item| item.index == index)
    }

    /// The open tab's items, or every match while searching.
    fn listed<'s>(&'s self, info: &'s HashMap<u32, ItemInfo>) -> impl Iterator<Item = &'s InventoryItem> {
        let search = self.search.to_lowercase();
        self.items.iter().filter(move |item| {
            if !search.is_empty() {
                return inventory::item_name(info, item.id, item.identified, item.refine).to_lowercase().contains(&search);
            }
            tab_of(item, info) == Some(self.tab)
        })
    }

    /// A tab's items, in the list's order (a category window's).
    fn in_tab<'s>(&'s self, tab: usize, info: &'s HashMap<u32, ItemInfo>) -> impl Iterator<Item = &'s InventoryItem> {
        self.items.iter().filter(move |item| tab_of(item, info) == Some(tab))
    }

    pub fn height(&self) -> f32 {
        height(self.rows)
    }

    /// A footer place, given up from the window's bottom, in the window.
    fn footer_at(&self, [x, up]: [f32; 2]) -> [f32; 2] {
        [x, self.height() - up]
    }

    fn search_at(&self) -> [f32; 2] {
        self.footer_at([if self.resized { SEARCH_MOVED } else { SEARCH[0] }, SEARCH[1]])
    }

    /// The row under a point in the window: only its icon cell, x 40 to 72.
    fn row_at(&self, local: [f32; 2]) -> Option<usize> {
        let row = ((local[1] - ROWS_TOP) / CELL).floor();
        (contains([CELL_X, ROWS_TOP], [CELL, self.rows as f32 * CELL], local) && row >= 0.0).then_some(row as usize)
    }

    /// The tabs and the listed items' icon cells on screen (for the test driver): `storage_tab:<n>`, `storage_item:<id>`.
    pub fn button_boxes(&self, assets: &Assets, info: &HashMap<u32, ItemInfo>) -> Vec<(String, [f32; 4])> {
        if !self.frame.open {
            return Vec::new();
        }
        let place = |local: [f32; 2], size: [f32; 2]| {
            let [x, y] = self.frame.at(local);
            [x, y, size[0], size[1]]
        };
        let _ = assets;
        let tabs = (0..TABS).map(|tab| (format!("storage_tab:{tab}"), place([TABS_AT[0], TABS_AT[1] + tab as f32 * TAB_HEIGHT], [SIDE, TAB_HEIGHT])));
        let mut boxes: Vec<_> = tabs.collect();
        boxes.extend((0..TABS).map(|tab| (format!("storage_box:{tab}"), place(self.box_at(tab), [BOX_SIZE, BOX_SIZE]))));
        boxes.push(("storage_close".into(), place(self.footer_at(CLOSE), CLOSE_SIZE)));
        let rows = self.listed(info).skip(self.scroll).take(self.rows).enumerate();
        boxes.extend(rows.map(|(row, item)| (format!("storage_item:{}", item.id), place([CELL_X, ROWS_TOP + row as f32 * CELL], [CELL, CELL]))));
        // Each open category window's rows: `storage_cat<tab>_item:<id>`, and its grip `storage_cat<tab>_resize`.
        for (tab, category) in self.categories.iter().enumerate().filter(|(_, category)| category.frame.open) {
            let rows = self.in_tab(tab, info).skip(category.scroll).take(category.rows()).enumerate();
            boxes.extend(rows.map(|(row, item)| {
                let [x, y] = category.frame.at([CATEGORY_POINTER[0], ROWS_TOP + row as f32 * CELL]);
                (format!("storage_cat{tab}_item:{}", item.id), [x, y, CATEGORY_POINTER[1] - CATEGORY_POINTER[0], CELL])
            }));
            let [x, y] = category.frame.at([category.size[0] - window::GRIP, category.size[1] - window::GRIP]);
            boxes.push((format!("storage_cat{tab}_resize"), [x, y, window::GRIP, window::GRIP]));
        }
        boxes
    }

    /// A category's box in the footer, in the window.
    fn box_at(&self, tab: usize) -> [f32; 2] {
        self.footer_at([BOX_AT[0] + tab as f32 * BOX_STEP, BOX_AT[1]])
    }

    /// Its rows.
    pub fn rows(&self) -> usize {
        self.rows
    }

    /// Takes `rows` as far as the grip would go at this screen size.
    pub fn set_rows(&mut self, rows: usize) {
        self.rows = rows.clamp(LEAST_ROWS, most_rows(crate::screen()[1]));
    }

    /// The corner grip on screen while the window is open (for the test driver).
    pub fn grip_box(&self) -> Option<[f32; 4]> {
        let [x, y] = self.frame.at([WIDTH - window::GRIP, self.height() - window::GRIP]);
        (self.frame.open && !self.frame.minimized).then_some([x, y, window::GRIP, window::GRIP])
    }

    /// Whether `cursor` is on the corner grip.
    pub fn on_grip(&self, cursor: [f32; 2]) -> bool {
        !self.frame.minimized && self.frame.local(cursor, self.height()).is_some_and(|local| window::on_grip([WIDTH, self.height()], local))
    }

    /// Held by the corner grip at `cursor`.
    pub fn start_resize(&mut self, cursor: [f32; 2]) {
        self.grip.start(cursor, [WIDTH, self.height()]);
    }

    /// While held, the bottom follows the pointer from the press, a row per 32 px (rounding toward the least); returns
    /// whether it's held.
    pub fn resize_to(&mut self, cursor: [f32; 2]) -> bool {
        let Some([_, asked]) = self.grip.asked(cursor) else { return false };
        let rows = self.rows;
        self.set_rows(LEAST_ROWS + window::steps(asked, height(LEAST_ROWS), CELL, usize::MAX));
        self.resized |= self.rows != rows;
        true
    }

    pub fn stop_resize(&mut self) -> bool {
        self.grip.stop()
    }

    pub fn hit(&self, cursor: [f32; 2], assets: &Assets, info: &HashMap<u32, ItemInfo>) -> Option<Hit> {
        let local = self.frame.local(cursor, self.height())?;
        // The title bar has the close button only.
        match self.frame.title_hit(local) {
            Some(TitleHit::Minimize) => return Some(Hit::Title(TitleHit::Title)),
            Some(hit) => return Some(Hit::Title(hit)),
            None => {}
        }
        if let Some(tab) = tab_at(local) {
            return Some(Hit::Tab(tab));
        }
        if let Some(tab) = (0..TABS).find(|&tab| contains(self.box_at(tab), [BOX_SIZE; 2], local)) {
            return Some(Hit::Box(tab));
        }
        if contains(self.footer_at(CLOSE), CLOSE_SIZE, local) {
            return Some(Hit::Close);
        }
        if contains(self.footer_at(FIND), assets.search[0].size, local) {
            return Some(Hit::Find);
        }
        if contains(self.search_at(), SEARCH_SIZE, local) {
            return Some(Hit::Search);
        }
        if window::on_grip([WIDTH, self.height()], local) {
            return Some(Hit::Resize);
        }
        let _ = assets;
        let item = self.row_at(local).and_then(|row| self.listed(info).nth(self.scroll + row));
        Some(item.map_or(Hit::Window, |item| Hit::Item(item.index)))
    }

    /// A click: true when it asks to close the storage. A tab keeps the scroll row as far as the new tab's list goes,
    /// and the search.
    pub fn click(&mut self, hit: Hit, info: &HashMap<u32, ItemInfo>) -> bool {
        self.typing = hit == Hit::Search;
        match hit {
            Hit::Title(TitleHit::Close) | Hit::Close => return true,
            Hit::Title(title) => self.frame.click_title(title),
            Hit::Tab(tab) => {
                self.tab = tab;
                let total = self.listed(info).count();
                self.scroll = self.scroll.min(total.saturating_sub(self.rows));
            }
            // A box acts on the press (`toggle_category`).
            Hit::Box(_) | Hit::Item(_) | Hit::Search | Hit::Find | Hit::Resize | Hit::Window => {}
        }
        false
    }

    /// The wheel: 3 rows a notch (fewer when fewer show), as far as the list goes.
    pub fn scroll_by(&mut self, notches: i32, assets: &Assets, info: &HashMap<u32, ItemInfo>) {
        let _ = assets;
        let step = WHEEL_ROWS.min(self.rows - 1) as i32;
        let total = self.listed(info).count();
        self.scroll = (self.scroll as i32 + notches * step).clamp(0, total.saturating_sub(self.rows) as i32) as usize;
    }

    /// What's under the pointer in a category window: its title (no buttons), a row's item, the grip, or the window.
    pub fn category_hit(&self, tab: usize, cursor: [f32; 2], info: &HashMap<u32, ItemInfo>) -> Option<Hit> {
        let category = &self.categories[tab];
        if !category.frame.open {
            return None;
        }
        let local = category.frame.local(cursor, category.height())?;
        if local[1] < TITLE_HEIGHT {
            return Some(Hit::Title(TitleHit::Title));
        }
        if window::on_grip(category.size, local) {
            return Some(Hit::Resize);
        }
        let item = category.row_at(local).and_then(|row| self.in_tab(tab, info).nth(category.scroll + row));
        Some(item.map_or(Hit::Window, |item| Hit::Item(item.index)))
    }

    /// The wheel over a category window: as the main window's.
    pub fn category_scroll_by(&mut self, tab: usize, notches: i32, info: &HashMap<u32, ItemInfo>) {
        let total = self.in_tab(tab, info).count();
        let category = &mut self.categories[tab];
        let rows = category.rows();
        let step = WHEEL_ROWS.min(rows.saturating_sub(1)).max(1) as i32;
        category.scroll = (category.scroll as i32 + notches * step).clamp(0, total.saturating_sub(rows) as i32) as usize;
    }

    /// Typing while the search field has the caret.
    pub fn type_text(&mut self, text: &str) {
        self.search.extend(text.chars().filter(|character| !character.is_control()));
        self.scroll = 0;
    }

    pub fn icons(&self, info: &HashMap<u32, ItemInfo>) -> Vec<String> {
        if !self.frame.open {
            return Vec::new();
        }
        self.items.iter().filter_map(|item| info.get(&item.id).map(|info| inventory::icon_path(info, item.identified))).collect()
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
        (messages, cards, makers): (&[String], &CardNames, &HashMap<u32, Option<String>>),
        font: &Font,
        pointer: ([f32; 2], bool),
        caret: bool,
    ) {
        if !self.frame.open {
            return;
        }
        let at = |local: [f32; 2]| self.frame.at(local);
        self.frame.draw_bar(layer, chrome);
        self.frame.draw_bare_title(layer, chrome, title(&self.name, messages), pointer.0);
        let hit = self.hit(pointer.0, assets, items);
        let look = |target: Hit| {
            let hovered = hit == Some(target);
            usize::from(hovered) + usize::from(hovered && pointer.1)
        };
        let h = self.height();

        // Body: the side pieces, the tab strip over the left one, white beside it, then each row's cell and white.
        let [left, right] = &assets.side;
        for step in 0..((h - 38.0) / 8.0).ceil() as usize {
            let y = TITLE_HEIGHT + step as f32 * 8.0;
            layer.sprites.push((left, at([0.0, y]), left.size));
            layer.sprites.push((right, at([WIDTH - SIDE, y]), right.size));
        }
        if let Some(strip) = assets.tabs.get(self.tab) {
            layer.sprites.push((strip, at(TABS_AT), strip.size));
        }
        layer.sprites.push((&chrome.white, at([SIDE, TITLE_HEIGHT]), [STRIP_WHITE, h - 38.0]));
        for row in 0..self.rows {
            let top = ROWS_TOP + row as f32 * CELL;
            layer.sprites.push((&assets.cell, at([CELL_X, top]), assets.cell.size));
            layer.sprites.push((&chrome.white, at([ROW_WHITE[0], top]), [ROW_WHITE[1] - ROW_WHITE[0], CELL]));
        }
        // Each row's item: the icon, its amount outlined, the name in its enchants' colour, the grade.
        for (row, item) in self.listed(items).skip(self.scroll).take(self.rows).enumerate() {
            let top = ROWS_TOP + row as f32 * CELL;
            let place = at([CELL_X, top]);
            let info = items.get(&item.id);
            if let Some(Some(icon)) = info.and_then(|info| icons.get(&inventory::icon_path(info, item.identified))) {
                layer.sprites.push((icon, offset(place, [ICON_OFFSET, ICON_OFFSET]), icon.size));
            }
            let amount = item.amount.to_string();
            let corner = offset(place, inventory::amount_at(item.amount));
            for step in [[-1.0, 0.0], [1.0, 0.0], [0.0, -1.0], [0.0, 1.0]] {
                layer.texts.push((amount.clone(), offset(corner, step), [255; 3]));
            }
            layer.texts.push((amount, corner, [0, 0, 0]));
            let (name, maker) = inventory::full_name_maker(items, cards, messages, makers, item, false);
            let color = name_color(inventory::enchants(item, items));
            let lines = name_lines(font, &name);
            for (number, (start, line)) in lines.iter().enumerate() {
                let y = if lines.len() == 1 { NAME_TOP } else { NAME_LINES[number] };
                let mut x = NAME_X;
                for (piece, color) in line_pieces(*start, line, maker.as_ref(), color) {
                    layer.texts.push((piece.clone(), at([x + 1.0, top + y + 1.0]), NAME_SHADOW));
                    layer.texts.push((piece.clone(), at([x, top + y]), color));
                    x += font.width(&piece);
                }
            }
            if let Some(grade) = assets.grades.get(usize::from(item.grade).wrapping_sub(1)) {
                layer.sprites.push((grade, offset(place, [4.0, 16.0]), grade.size));
            }
            if contains(place, [CELL, CELL], pointer.0) {
                let name = inventory::full_name(items, cards, messages, makers, item, true);
                window::tooltip(tooltip, chrome, font, &name, offset(place, [0.0, -NAME_BOX_ABOVE]), inventory::name_color(item, items));
            }
        }

        // Footer: the count (red when full), the search box, its button, Close.
        let [left, middle, right] = &assets.footer;
        let footer_top = h - FOOTER_HEIGHT;
        layer.sprites.push((left, at([0.0, footer_top]), left.size));
        layer.sprites.push((middle, at([left.size[0], footer_top]), [WIDTH - left.size[0] - right.size[0], FOOTER_HEIGHT]));
        layer.sprites.push((right, at([WIDTH - right.size[0], footer_top]), right.size));
        // The category boxes along the footer's top row, on or off.
        for (tab, images) in assets.boxes.iter().enumerate() {
            if let Some(image) = &images[usize::from(self.mask & 1 << tab == 0)] {
                layer.sprites.push((image, at(self.box_at(tab)), [BOX_SIZE, BOX_SIZE]));
            }
        }
        layer.sprites.push((&assets.count, at(self.footer_at(COUNT_ICON)), assets.count.size));
        let count = self.count[0].to_string();
        let [x, y] = self.footer_at(COUNT);
        layer.texts.push((count.clone(), at([x, y]), count_color(self.count)));
        layer.texts.push((format!("/{}", self.count[1]), at([x + 1.0 + font.width(&count), y]), [0, 0, 0]));
        layer.sprites.push((&assets.search_box, at(self.search_at()), SEARCH_SIZE));
        let shown = if self.typing && caret { format!("{}|", self.search) } else { self.search.clone() };
        layer.texts.push((window::fit(font, &shown, SEARCH_SIZE[0] - 6.0), at(offset(self.search_at(), [3.0, 3.0])), [0, 0, 0]));
        let find = &assets.search[look(Hit::Find)];
        layer.sprites.push((find, at(self.footer_at(FIND)), find.size));
        layer.sprites.push((&assets.close[look(Hit::Close)], at(self.footer_at(CLOSE)), CLOSE_SIZE));
        layer.sprites.push((&assets.resize, at([WIDTH - window::GRIP, h - window::GRIP]), assets.resize.size));
    }
}

impl Storage {
    /// A category window: the title bar (no buttons), a white body, a cell and the item for each row, the footer and
    /// the grip.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_category<'a>(
        &self,
        tab: usize,
        layer: &mut Layer<'a>,
        tooltip: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        icons: &'a HashMap<String, Option<Texture>>,
        items: &HashMap<u32, ItemInfo>,
        (messages, cards, makers): (&[String], &CardNames, &HashMap<u32, Option<String>>),
        font: &Font,
        pointer: [f32; 2],
    ) {
        let category = &self.categories[tab];
        if !category.frame.open {
            return;
        }
        let at = |local: [f32; 2]| category.frame.at(local);
        let [w, h] = category.size;
        category.frame.draw_title_only(layer, chrome, messages.get(CATEGORY_TITLES[tab]).map_or("", String::as_str));
        layer.sprites.push((&chrome.white, at([0.0, TITLE_HEIGHT]), [w, h - 38.0]));
        for row in 0..category.rows() {
            layer.sprites.push((&assets.cell, at([CATEGORY_CELL_X, ROWS_TOP + row as f32 * CELL]), assets.cell.size));
        }
        for (row, item) in self.in_tab(tab, items).skip(category.scroll).take(category.rows()).enumerate() {
            let top = ROWS_TOP + row as f32 * CELL;
            let place = at([CATEGORY_CELL_X, top]);
            let info = items.get(&item.id);
            if let Some(Some(icon)) = info.and_then(|info| icons.get(&inventory::icon_path(info, item.identified))) {
                layer.sprites.push((icon, offset(place, [ICON_OFFSET, ICON_OFFSET]), icon.size));
            }
            let amount = item.amount.to_string();
            let corner = offset(place, inventory::amount_at(item.amount));
            for step in [[-1.0, 0.0], [1.0, 0.0], [0.0, -1.0], [0.0, 1.0]] {
                layer.texts.push((amount.clone(), offset(corner, step), [255; 3]));
            }
            layer.texts.push((amount, corner, [0, 0, 0]));
            let (name, _) = inventory::full_name_maker(items, cards, messages, makers, item, false);
            let name = window::fit(font, &name, w - 76.0);
            layer.texts.push((name.clone(), at([CATEGORY_NAME_X + 1.0, top + NAME_TOP + 1.0]), NAME_SHADOW));
            layer.texts.push((name, at([CATEGORY_NAME_X, top + NAME_TOP]), name_color(inventory::enchants(item, items))));
            if let Some(grade) = assets.grades.get(usize::from(item.grade).wrapping_sub(1)) {
                layer.sprites.push((grade, offset(place, [4.0, 16.0]), grade.size));
            }
            // The name box comes up over the pointer's cell, 10 px right of the drawn one.
            let pointer_cell = at([CATEGORY_POINTER[0], top]);
            if contains(pointer_cell, [CATEGORY_POINTER[1] - CATEGORY_POINTER[0], CELL], pointer) {
                let name = inventory::full_name(items, cards, messages, makers, item, true);
                window::tooltip(tooltip, chrome, font, &name, offset(at([NAME_X - CELL_X, top]), [0.0, -NAME_BOX_ABOVE]), inventory::name_color(item, items));
            }
        }
        let [left, middle, right] = &assets.bar;
        let footer_top = h - CATEGORY_FOOTER;
        layer.sprites.push((left, at([0.0, footer_top]), left.size));
        layer.sprites.push((middle, at([left.size[0], footer_top]), [w - left.size[0] - right.size[0], CATEGORY_FOOTER]));
        layer.sprites.push((right, at([w - right.size[0], footer_top]), right.size));
        layer.sprites.push((&assets.resize, at([w - window::GRIP, h - window::GRIP]), assets.resize.size));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resizes_its_height_in_the_games_steps() {
        // h = 74 + 32 a row: 458, 490, 522; up to 0.7 of the screen, only 12 under 700.
        assert_eq!([12, 13, 14].map(height), [458.0, 490.0, 522.0]);
        assert_eq!([600.0, 699.0, 768.0, 1080.0].map(most_rows), [12, 12, 14, 21]);
        let mut storage = Storage::default();
        assert_eq!((storage.rows(), storage.frame.position), (LEAST_ROWS, [360.0, 200.0]));
        storage.open(String::new());
        assert!(!storage.resized);
        let corner = storage.frame.at([WIDTH - 5.0, storage.height() - 5.0]);
        assert!(storage.on_grip(corner));
        storage.start_resize(corner);
        // 31 px down isn't a row yet; 32 is; the width never changes. Once the height changed, the search box moves.
        assert!(storage.resize_to([corner[0] + 100.0, corner[1] + 31.0]));
        assert_eq!(storage.rows(), 12);
        assert_eq!(storage.search_at(), [65.0, 458.0 - 23.0]);
        storage.resize_to([corner[0], corner[1] + 64.0]);
        assert_eq!(storage.rows(), most_rows(crate::screen()[1]).min(14));
        storage.resize_to([corner[0], corner[1] - 500.0]);
        assert_eq!(storage.rows(), LEAST_ROWS);
        assert!(storage.stop_resize() && !storage.resize_to(corner));
        if most_rows(crate::screen()[1]) > LEAST_ROWS {
            assert_eq!(storage.search_at(), [77.0, 458.0 - 23.0]);
        }
        // Kept rows survive the next opening, which counts as changed; a kept value out of range is clamped.
        storage.set_rows(13);
        storage.open("Guild Storage".into());
        assert_eq!(storage.rows(), 13.min(most_rows(crate::screen()[1])));
        assert_eq!(storage.resized, storage.rows() != LEAST_ROWS);
        storage.set_rows(99);
        assert_eq!(storage.rows(), most_rows(crate::screen()[1]));
    }

    use rustnarok_client::net;

    fn item(index: u16, kind: u8, location: u32) -> InventoryItem {
        InventoryItem { index, id: 2220, kind, amount: 1, location, identified: true, ..Default::default() }
    }

    #[test]
    fn sorts_into_the_clients_tabs() {
        let none = HashMap::new();
        let tab = |kind: u8| tab_of(&item(1, kind, 0), &none);
        // Use, Cash, Armors, Weapons, (Costumes), Ammo, Card, Etc by the type byte; others nowhere.
        assert_eq!([0, 1, 2].map(tab), [Some(0); 3]);
        assert_eq!(tab(18), Some(1));
        assert_eq!([4, 11, 12, 13, 14].map(tab), [Some(2); 5], "Fly Wing (11) is on Armors");
        assert_eq!([5, 8, 9, 15].map(tab), [Some(3); 4]);
        assert_eq!([10, 16, 17, 19].map(tab), [Some(5); 4]);
        assert_eq!((tab(6), tab(3), tab(7)), (Some(6), Some(7), Some(7)));
        assert_eq!([20, 99].map(tab), [None; 2]);
        // A costume by iteminfo, whatever its type (not by where it's worn).
        let costume = HashMap::from([(2220, ItemInfo { costume: true, ..Default::default() })]);
        assert_eq!(tab_of(&item(1, 4, 0x400), &costume), Some(4));
        assert_eq!(tab_of(&item(1, 4, 0x400), &none), Some(2));
    }

    #[test]
    fn picks_tabs_in_29_px_bands() {
        assert_eq!([18.0, 46.0, 47.0, 252.0].map(|y| tab_at([10.0, y])), [Some(0), Some(0), Some(1), Some(7)]);
        assert_eq!((tab_at([20.0, 30.0]), tab_at([5.0, 253.0])), (None, None));
    }

    #[test]
    fn keeps_the_scroll_row_across_tabs() {
        let mut storage = Storage::default();
        storage.open(String::new());
        let none = HashMap::new();
        // 30 usable and 20 etc items: row 4 of Use, over to Etc and back, is still row 4.
        storage.extend((0..30).map(|index| item(index, 0, 0)).chain((30..50).map(|index| item(index, 3, 0))).collect());
        storage.scroll = 4;
        storage.click(Hit::Tab(7), &none);
        assert_eq!(storage.scroll, 4);
        storage.click(Hit::Tab(0), &none);
        assert_eq!(storage.scroll, 4);
        // A tab with fewer rows keeps the last there is; the search stays.
        storage.search = "x".into();
        storage.click(Hit::Tab(6), &none);
        assert_eq!((storage.scroll, storage.search.as_str()), (0, "x"));
    }

    #[test]
    fn sizes_a_category_window_by_its_grip() {
        assert_eq!([139.0, 171.0, 172.0, 400.0].map(|w| category_size([w, 145.0])[0]), [140.0, 140.0, 172.0, 300.0]);
        assert_eq!([150.0, 191.0, 192.0, 400.0].map(|h| category_size([140.0, h])[1]), [145.0, 145.0, 177.0, 305.0]);
        // The pointer box is 10 px right of the drawn cell: x 20 and 51 hit, 10 and 52 don't.
        let category = Category::new(0);
        assert_eq!([20.0, 51.0, 10.0, 52.0].map(|x| category.row_at([x, 20.0])), [Some(0), Some(0), None, None]);
        assert_eq!((category.rows(), category.row_at([30.0, 17.0 + 96.0])), (3, None));
    }

    #[test]
    fn reopens_the_categories_that_were_on() {
        let mut storage = Storage::default();
        storage.open(String::new());
        storage.toggle_category(0);
        storage.toggle_category(3);
        assert_eq!(storage.mask, 0b1001);
        assert!(storage.categories[0].frame.open && storage.categories[3].frame.open);
        assert_eq!((storage.categories[0].frame.position, storage.categories[0].size), ([210.0, 50.0], [140.0, 145.0]));
        // Close shuts them all and keeps the mask; the next opening brings them back at 140 × 145.
        storage.categories[0].size = [300.0, 305.0];
        storage.close();
        assert!(storage.categories.iter().all(|category| !category.frame.open) && storage.mask == 0b1001);
        storage.open(String::new());
        assert!(storage.categories[0].frame.open && storage.categories[3].frame.open && !storage.categories[1].frame.open);
        assert_eq!(storage.categories[0].size, [140.0, 145.0]);
        // Esc shuts one and turns its box off: it doesn't come back.
        storage.close_category(0);
        storage.close();
        storage.open(String::new());
        assert_eq!(storage.mask, 0b1000);
        assert!(!storage.categories[0].frame.open && storage.categories[3].frame.open);
    }

    #[test]
    fn keeps_the_order_items_came_in() {
        let mut storage = Storage::default();
        storage.open(String::new());
        storage.extend(vec![item(4, 3, 0), item(2, net::ITEM_HEALING, 0), item(9, 3, 0)]);
        // A stack stays where it is; a new one goes last; a removal closes up.
        storage.add(InventoryItem { amount: 2, ..item(2, net::ITEM_HEALING, 0) });
        storage.add(item(1, 3, 0));
        storage.remove(4, 1);
        assert_eq!(storage.items.iter().map(|item| (item.index, item.amount)).collect::<Vec<_>>(), [(2, 3), (9, 1), (1, 1)]);
    }

    #[test]
    fn takes_the_pointer_on_the_icon_cell_only() {
        let storage = Storage::default();
        assert_eq!(storage.row_at([40.0, 17.0]), Some(0));
        assert_eq!(storage.row_at([71.0, 17.0 + 32.0 * 11.0 + 31.0]), Some(11));
        assert_eq!(storage.row_at([39.0, 20.0]), None);
        assert_eq!(storage.row_at([72.0, 20.0]), None);
        assert_eq!(storage.row_at([50.0, 17.0 + 32.0 * 12.0]), None);
        // The amount's place, as the inventory's cells.
        assert_eq!((inventory::amount_at(999), inventory::amount_at(1000)), ([18.0, 19.0], [4.0, 19.0]));
    }

    #[test]
    fn writes_long_names_on_two_lines() {
        let font = Font::load();
        assert_eq!(name_lines(&font, "Knife"), [(0, "Knife".to_owned())]);
        let long = "Very Very Strong Fire Blade of the Faraway Mountains";
        let lines = name_lines(&font, long);
        assert_eq!(lines.len(), 2);
        // Cut by character: the first line is as much as fits, the second goes on from there.
        assert!(font.width(&lines[0].1) <= NAME_WIDTH);
        assert!(font.width(&long[..lines[0].1.len() + 1]) > NAME_WIDTH);
        assert_eq!(lines[1].0, lines[0].1.len());
        assert!(long.ends_with(&lines[1].1));
        let longer = long.repeat(3);
        let lines = name_lines(&font, &longer);
        assert!(lines[1].1.ends_with("...") && font.width(&lines[1].1) <= NAME_WIDTH);
        // The maker's name keeps its colour across a line.
        let maker = (6..12, inventory::MAKER_COLOR);
        assert_eq!(
            line_pieces(4, "AAbbbbbbCC", Some(&maker), [0, 0, 0]),
            [("AA".to_owned(), [0, 0, 0]), ("bbbbbb".to_owned(), inventory::MAKER_COLOR), ("CC".to_owned(), [0, 0, 0])]
        );
        assert_eq!([0, 1, 2, 4].map(name_color), [[0, 0, 0], ONE_ENCHANT, inventory::ENCHANTED[1], inventory::ENCHANTED[3]]);
    }

    #[test]
    fn counts_red_when_full_and_scrolls_three_rows() {
        assert_eq!(count_color([99, 100]), COUNT_COLOR);
        assert_eq!(count_color([100, 100]), FULL_COLOR);
        let messages = vec!["".to_owned(); 170]
            .into_iter()
            .enumerate()
            .map(|(id, text)| if id == TITLE_MESSAGE { "Storage".to_owned() } else { text })
            .collect::<Vec<_>>();
        assert_eq!((title("", &messages), title("Guild Storage", &messages)), ("Storage", "Guild Storage"));
        assert_eq!(WHEEL_ROWS.min(LEAST_ROWS - 1), 3);
    }
}
