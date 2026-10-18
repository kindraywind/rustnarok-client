//! Kafra storage: the window of what's stored, with the 2018+ client's tabs down the left, its count and a search
//! field. Items move in and out by dragging them between it and the inventory.

use std::collections::HashMap;

use rustnarok_client::grf::Grf;
use rustnarok_client::lua::ItemInfo;
use rustnarok_client::messages;
use rustnarok_client::net::{self, InventoryItem};
use rustnarok_client::text::Font;

use crate::inventory::{self, UNIDENTIFIED};
use crate::render::{Gpu, Texture};
use crate::window::{self, Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI, contains, offset};

// ponytail: the inventory's cells and margins in a grid; calibrate against the real client.
const COLUMNS: usize = 7;
const CELL: f32 = 32.0;
/// Rows of cells: 12 at the game's least height (458), one more for each 32 px the corner grip adds (it only changes
/// the height), as far as 0.7 of the screen's height.
// ponytail: the game's storage is a one-column list; this grid takes its row steps.
pub const LEAST_ROWS: usize = 12;
const LEAST_HEIGHT: f32 = 458.0;
const FOOTER_HEIGHT: f32 = 29.0;
const TAB_COLUMN: f32 = 27.0;
const MARGIN: f32 = 16.0;
const WIDTH: f32 = TAB_COLUMN + MARGIN + COLUMNS as f32 * CELL + MARGIN;
const TABS_AT: [f32; 2] = [1.0, TITLE_HEIGHT];
const GRID: [f32; 2] = [TAB_COLUMN + MARGIN, TITLE_HEIGHT + 8.0];
const ICON_OFFSET: f32 = 4.0;
/// The search field and the close button sit in the footer.
const SEARCH_SIZE: [f32; 2] = [130.0, 18.0];
const CLOSE_SIZE: [f32; 2] = [42.0, 20.0];

/// The most rows at a screen `height` tall: 458 + 32k up to ⌊0.7·height⌋, only 12 under 700.
pub fn most_rows(height: f32) -> usize {
    LEAST_ROWS + window::steps((0.7 * height).floor(), LEAST_HEIGHT, CELL, usize::MAX)
}

/// The window's height with `rows` rows.
fn height(rows: usize) -> f32 {
    TITLE_HEIGHT + rows as f32 * CELL + 18.0 + FOOTER_HEIGHT
}
const DEFAULT_POSITION: [f32; 2] = [290.0, 100.0];
const TITLE_MESSAGE: usize = 169;
/// Where the tab strips' tabs end: the client's eight (usable, cash, armor, weapon, costume, ammo, card, etc.) and
/// the translated data's seven, which have no costume tab. Measured from the bitmaps.
const EDGES_EIGHT: [f32; 8] = [23.0, 51.0, 88.0, 114.0, 140.0, 178.0, 204.0, 235.0];
const EDGES_SEVEN: [f32; 7] = [25.0, 53.0, 77.0, 115.0, 151.0, 178.0, 209.0];
/// Where costumes go on the headgear and garment.
const COSTUME: u32 = 0x400 | 0x800 | 0x1000 | 0x2000;

pub struct Assets {
    footer: [Texture; 3],
    /// The tab strip with each tab picked.
    tabs: Vec<Texture>,
    cell: Texture,
    count: Texture,
    search: [Texture; 3],
    close: [Texture; 3],
    resize: Texture,
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let basic = format!("{UI}basic_interface\\");
        let image = |path: String| gpu.load_texture(grf, &path, false);
        let mut tabs: Vec<_> = (1..=8).filter_map(|tab| gpu.try_load_texture(grf, &format!("{basic}tab_itm_ex_0{tab}.bmp"), false).ok()).collect();
        // The strip's height tells which it is (translated data can leave the original's eighth image behind).
        tabs.truncate(Self::edges_for(tabs.first().map_or(0.0, |strip| strip.size[1])).len());
        Self {
            footer: ["left", "mid", "right"].map(|part| image(format!("{basic}btnbar_{part}2.bmp"))),
            tabs,
            cell: image(format!("{basic}itemwin_mid.bmp")),
            count: image(format!("{UI}inventory\\icon_num.bmp")),
            search: ["normal", "over", "press"].map(|look| image(format!("{UI}bt_search_{look}.bmp"))),
            close: ["", "_a", "_b"].map(|look| image(format!("{basic}btn_close{look}.bmp"))),
            resize: image(format!("{UI}btn_resize.bmp")),
        }
    }

    fn edges_for(height: f32) -> &'static [f32] {
        if height >= EDGES_EIGHT[7] { &EDGES_EIGHT } else { &EDGES_SEVEN }
    }

    fn edges(&self) -> &'static [f32] {
        Self::edges_for(self.tabs.first().map_or(0.0, |strip| strip.size[1]))
    }
}

/// The tab an item is listed under, of the client's eight.
fn tab_of(item: &InventoryItem) -> usize {
    match item.kind {
        net::ITEM_HEALING | net::ITEM_USABLE | net::ITEM_DELAY_CONSUME => 0,
        net::ITEM_CASH => 1,
        net::ITEM_ARMOR | net::ITEM_SHADOW_GEAR if item.location & COSTUME != 0 => 4,
        net::ITEM_ARMOR | net::ITEM_SHADOW_GEAR | net::ITEM_PET_EGG | net::ITEM_PET_ARMOR => 2,
        net::ITEM_WEAPON => 3,
        net::ITEM_AMMO => 5,
        net::ITEM_CARD => 6,
        _ => 7,
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(TitleHit),
    Tab(usize),
    /// A stored item, by storage index.
    Item(u16),
    Search,
    Find,
    Close,
    /// The corner grip.
    Resize,
    Window,
}

pub struct Storage {
    pub items: Vec<InventoryItem>,
    /// How many kinds are stored, and how many can be.
    pub count: [u16; 2],
    pub frame: Frame,
    /// The open tab, of the strip's.
    tab: usize,
    scroll: usize,
    /// What's typed in the search field, and whether typing goes there.
    pub search: String,
    pub typing: bool,
    /// Rows of cells shown, and the corner grip while held.
    rows: usize,
    grip: window::Grip,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            count: [0; 2],
            frame: Frame::new(DEFAULT_POSITION, WIDTH),
            tab: 0,
            scroll: 0,
            search: String::new(),
            typing: false,
            rows: LEAST_ROWS,
            grip: window::Grip::default(),
        }
    }
}

impl Storage {
    /// The server starts listing it: the window opens on its first tab.
    pub fn open(&mut self) {
        let frame = std::mem::replace(&mut self.frame, Frame::new(DEFAULT_POSITION, WIDTH));
        *self = Self { frame, rows: self.rows, ..Self::default() };
        self.frame.open = true;
        self.frame.minimized = false;
    }

    pub fn close(&mut self) {
        self.frame.open = false;
        self.typing = false;
    }

    pub fn extend(&mut self, items: Vec<InventoryItem>) {
        self.items.extend(items);
        self.items.sort_by_key(|item| item.index);
    }

    /// An item stored: onto its stack, or into a new place.
    pub fn add(&mut self, item: InventoryItem) {
        match self.items.iter_mut().find(|stored| stored.index == item.index) {
            Some(stored) => stored.amount = stored.amount.saturating_add(item.amount),
            None => self.extend(vec![item]),
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
    fn listed<'s>(&'s self, assets: &Assets, info: &'s HashMap<u32, ItemInfo>) -> impl Iterator<Item = &'s InventoryItem> {
        let search = self.search.to_lowercase();
        let seven = assets.edges().len() < 8;
        self.items.iter().filter(move |item| {
            if !search.is_empty() {
                return inventory::item_name(info, item.id, item.identified, item.refine).to_lowercase().contains(&search);
            }
            let tab = tab_of(item);
            // Seven tabs: costumes are armor, and the rest move up one.
            let tab = if seven && tab == 4 {
                2
            } else if seven && tab > 4 {
                tab - 1
            } else {
                tab
            };
            tab == self.tab
        })
    }

    pub fn height(&self) -> f32 {
        height(self.rows)
    }

    fn body_height(&self) -> f32 {
        self.rows as f32 * CELL + 18.0
    }

    fn footer_top(&self) -> f32 {
        TITLE_HEIGHT + self.body_height()
    }

    fn search_at(&self) -> [f32; 2] {
        [72.0, self.footer_top() + 5.0]
    }

    fn close_at(&self) -> [f32; 2] {
        [WIDTH - 15.0 - 42.0, self.footer_top() + 5.0]
    }

    /// Its rows of cells.
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
        self.set_rows(LEAST_ROWS + window::steps(asked, height(LEAST_ROWS), CELL, usize::MAX));
        true
    }

    pub fn stop_resize(&mut self) -> bool {
        self.grip.stop()
    }

    pub fn hit(&self, cursor: [f32; 2], assets: &Assets, info: &HashMap<u32, ItemInfo>) -> Option<Hit> {
        let local = self.frame.local(cursor, self.height())?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(Hit::Title(hit));
        }
        let edges = assets.edges();
        if contains(TABS_AT, [20.0, edges[edges.len() - 1]], local) {
            let y = local[1] - TABS_AT[1];
            return Some(Hit::Tab(edges.iter().position(|&edge| y < edge).unwrap_or(0)));
        }
        if contains(self.close_at(), CLOSE_SIZE, local) {
            return Some(Hit::Close);
        }
        let button = assets.search[0].size;
        if contains(offset(self.search_at(), [SEARCH_SIZE[0] - button[0], 0.0]), button, local) {
            return Some(Hit::Find);
        }
        if contains(self.search_at(), SEARCH_SIZE, local) {
            return Some(Hit::Search);
        }
        if !self.frame.minimized && window::on_grip([WIDTH, self.height()], local) {
            return Some(Hit::Resize);
        }
        let [x, y] = [local[0] - GRID[0], local[1] - GRID[1]];
        let [column, row] = [(x / CELL).floor(), (y / CELL).floor()];
        if x >= 0.0 && y >= 0.0 && (column as usize) < COLUMNS && (row as usize) < self.rows {
            let cell = (self.scroll + row as usize) * COLUMNS + column as usize;
            if let Some(item) = self.listed(assets, info).nth(cell) {
                return Some(Hit::Item(item.index));
            }
        }
        Some(Hit::Window)
    }

    /// A click: true when it asks to close the storage.
    pub fn click(&mut self, hit: Hit) -> bool {
        self.typing = hit == Hit::Search;
        match hit {
            Hit::Title(TitleHit::Close) | Hit::Close => return true,
            Hit::Title(title) => self.frame.click_title(title),
            Hit::Tab(tab) => {
                self.tab = tab;
                self.scroll = 0;
                self.search.clear();
            }
            Hit::Item(_) | Hit::Search | Hit::Find | Hit::Resize | Hit::Window => {}
        }
        false
    }

    pub fn scroll_by(&mut self, rows: i32, assets: &Assets, info: &HashMap<u32, ItemInfo>) {
        let total_rows = self.listed(assets, info).count().div_ceil(COLUMNS);
        self.scroll = (self.scroll as i32 + rows).clamp(0, total_rows.saturating_sub(self.rows) as i32) as usize;
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
        messages: &[String],
        font: &Font,
        pointer: ([f32; 2], bool),
        caret: bool,
    ) {
        if !self.frame.open {
            return;
        }
        let at = |local: [f32; 2]| self.frame.at(local);
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        self.frame.draw_title(layer, chrome, message(TITLE_MESSAGE), pointer.0);
        if self.frame.minimized {
            return;
        }
        let hit = self.hit(pointer.0, assets, items);
        let look = |target: Hit| {
            let hovered = hit == Some(target);
            usize::from(hovered) + usize::from(hovered && pointer.1)
        };

        // Body: the tab column and its strip, the white item area, the cells.
        layer.sprites.push((&assets.footer[1], at([0.0, TITLE_HEIGHT]), [TAB_COLUMN, self.body_height()]));
        layer.sprites.push((&chrome.white, at([0.0, TITLE_HEIGHT]), [TABS_AT[0], self.body_height()]));
        if let Some(strip) = assets.tabs.get(self.tab) {
            layer.sprites.push((strip, at(TABS_AT), strip.size));
        }
        layer.sprites.push((&chrome.white, at([TAB_COLUMN, TITLE_HEIGHT]), [WIDTH - TAB_COLUMN, self.body_height()]));
        for x in [TAB_COLUMN, WIDTH - 1.0] {
            layer.sprites.push((&chrome.border, at([x, TITLE_HEIGHT]), [1.0, self.body_height()]));
        }
        for cell in 0..self.rows * COLUMNS {
            let place = [GRID[0] + (cell % COLUMNS) as f32 * CELL, GRID[1] + (cell / COLUMNS) as f32 * CELL];
            layer.sprites.push((&assets.cell, at(place), assets.cell.size));
        }
        for (cell, item) in self.listed(assets, items).skip(self.scroll * COLUMNS).take(self.rows * COLUMNS).enumerate() {
            let place = at([GRID[0] + (cell % COLUMNS) as f32 * CELL, GRID[1] + (cell / COLUMNS) as f32 * CELL]);
            let info = items.get(&item.id);
            if let Some(Some(icon)) = info.and_then(|info| icons.get(&inventory::icon_path(info, item.identified))) {
                layer.sprites.push((icon, offset(place, [ICON_OFFSET, ICON_OFFSET]), icon.size));
            }
            if inventory::tab_of(item.kind) != 1 {
                let amount = item.amount.to_string();
                let corner = offset(place, [CELL - 1.0 - font.width(&amount), 15.0]);
                layer.texts.push((amount.clone(), offset(corner, [-1.0, -1.0]), [255; 3]));
                layer.texts.push((amount, corner, [0, 0, 0]));
            }
            if contains(place, [CELL, CELL], pointer.0) {
                let name = inventory::item_name(items, item.id, item.identified, item.refine);
                let text = messages::fill(message(messages::ITEM_AMOUNT), &[&name, &item.amount]);
                window::tooltip(tooltip, chrome, font, &text, offset(place, [35.0, 0.0]), if item.identified { [255; 3] } else { UNIDENTIFIED });
            }
        }

        // Footer: the count, the search field, the close button.
        let [left, middle, right] = &assets.footer;
        layer.sprites.push((left, at([0.0, self.footer_top()]), left.size));
        layer.sprites.push((middle, at([left.size[0], self.footer_top()]), [WIDTH - left.size[0] - right.size[0], FOOTER_HEIGHT]));
        layer.sprites.push((right, at([WIDTH - right.size[0], self.footer_top()]), right.size));
        layer.sprites.push((&assets.count, at([8.0, self.footer_top() + 8.0]), assets.count.size));
        layer.texts.push((format!("{}/{}", self.count[0], self.count[1]), at([22.0, self.footer_top() + 8.0]), [0, 0, 0]));
        layer.sprites.push((&chrome.border, at(offset(self.search_at(), [-1.0, -1.0])), [SEARCH_SIZE[0] + 2.0, SEARCH_SIZE[1] + 2.0]));
        layer.sprites.push((&chrome.white, at(self.search_at()), SEARCH_SIZE));
        let shown = if self.typing && caret { format!("{}|", self.search) } else { self.search.clone() };
        layer.texts.push((window::fit(font, &shown, SEARCH_SIZE[0] - 28.0), at(offset(self.search_at(), [3.0, 3.0])), [0, 0, 0]));
        let find = &assets.search[look(Hit::Find)];
        layer.sprites.push((find, at(offset(self.search_at(), [SEARCH_SIZE[0] - find.size[0], 0.0])), find.size));
        layer.sprites.push((&assets.close[look(Hit::Close)], at(self.close_at()), CLOSE_SIZE));
        layer.sprites.push((&assets.resize, at([WIDTH - window::GRIP, self.height() - window::GRIP]), assets.resize.size));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resizes_its_height_in_the_games_steps() {
        // 458 + 32k up to 0.7 of the screen: 12 to 14 rows at 768 high, only 12 under 700.
        assert_eq!([600.0, 699.0, 768.0, 1080.0].map(most_rows), [12, 12, 14, 21]);
        let mut storage = Storage::default();
        assert_eq!(storage.rows(), LEAST_ROWS);
        storage.frame.open = true;
        let corner = storage.frame.at([WIDTH - 5.0, storage.height() - 5.0]);
        assert!(storage.on_grip(corner));
        storage.start_resize(corner);
        // 31 px down isn't a row yet; 32 is; the width never changes.
        assert!(storage.resize_to([corner[0] + 100.0, corner[1] + 31.0]));
        assert_eq!(storage.rows(), 12);
        storage.resize_to([corner[0], corner[1] + 64.0]);
        assert_eq!(storage.rows(), most_rows(crate::screen()[1]).min(14));
        storage.resize_to([corner[0], corner[1] - 500.0]);
        assert_eq!(storage.rows(), LEAST_ROWS);
        assert!(storage.stop_resize() && !storage.resize_to(corner));
        // Kept rows survive the next opening; a kept value out of range is clamped.
        storage.set_rows(13);
        storage.open();
        assert_eq!(storage.rows(), 13.min(most_rows(crate::screen()[1])));
        storage.set_rows(99);
        assert_eq!(storage.rows(), most_rows(crate::screen()[1]));
    }

    fn item(index: u16, kind: u8, location: u32) -> InventoryItem {
        InventoryItem { index, id: 2220, kind, amount: 1, location, identified: true, ..Default::default() }
    }

    #[test]
    fn sorts_into_the_clients_tabs() {
        assert_eq!(tab_of(&item(1, net::ITEM_HEALING, 0)), 0);
        assert_eq!(tab_of(&item(1, net::ITEM_ARMOR, 0x100)), 2);
        assert_eq!(tab_of(&item(1, net::ITEM_ARMOR, 0x400)), 4);
        assert_eq!(tab_of(&item(1, net::ITEM_CARD, 0)), 6);
        assert_eq!(tab_of(&item(1, 3, 0)), 7);
        let mut storage = Storage::default();
        storage.open();
        storage.extend(vec![item(4, 3, 0), item(2, net::ITEM_HEALING, 0)]);
        storage.add(InventoryItem { amount: 2, ..item(2, net::ITEM_HEALING, 0) });
        assert_eq!(storage.get(2).unwrap().amount, 3);
        storage.remove(4, 1);
        assert!(storage.get(4).is_none());
        assert_eq!(EDGES_EIGHT.len(), 8);
    }
}
