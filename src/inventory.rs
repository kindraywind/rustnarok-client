//! What the player carries, and its window (Alt+E), laid out like the classic client's inventory.

use std::collections::HashMap;

use rustnarok_client::grf::Grf;
use rustnarok_client::lua::ItemInfo;
use rustnarok_client::messages;
use rustnarok_client::net::{self, InventoryItem};
use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};
use crate::window::{self, Chrome, Clicks, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI, contains, offset};

// ponytail: sizes and offsets of the classic inventory over its bitmaps; calibrate against the real client.
const COLUMNS: usize = 7;
const ROWS: usize = 2;
const CELL: f32 = 32.0;
/// Rows of cells plus 8px above and 10px below them; the tab strip is exactly this tall.
const BODY_HEIGHT: f32 = ROWS as f32 * CELL + 18.0;
const FOOTER_HEIGHT: f32 = 29.0;
/// The tab column, then 16px of white on either side of the cells (27 + 16 + 16 + columns).
const TAB_COLUMN: f32 = 27.0;
const MARGIN: f32 = 16.0;
const WIDTH: f32 = TAB_COLUMN + MARGIN + COLUMNS as f32 * CELL + MARGIN;
const HEIGHT: f32 = TITLE_HEIGHT + BODY_HEIGHT + FOOTER_HEIGHT;
/// The tab strip, after a 1px white edge.
const TABS_AT: [f32; 2] = [1.0, TITLE_HEIGHT];
const TABS_SIZE: [f32; 2] = [20.0, BODY_HEIGHT];
const TABS: usize = 3;
/// The first cell.
const GRID: [f32; 2] = [TAB_COLUMN + MARGIN, TITLE_HEIGHT + 8.0];
const ICON_OFFSET: f32 = 4.0;
const BASE_SLOTS: usize = 100;
/// Where the client first opens it.
const DEFAULT_POSITION: [f32; 2] = [0.0, 172.0];
pub const UNIDENTIFIED: [u8; 3] = [0xAA, 0xAA, 0xAA];

/// The window's own bitmaps, all from data.grf.
pub struct Assets {
    footer: [Texture; 3],
    tabs: [Texture; TABS],
    cell: Texture,
    count: Texture,
    resize: Texture,
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let basic = format!("{UI}basic_interface\\");
        let image = |path: String| gpu.load_texture(grf, &path, false);
        Self {
            footer: ["left", "mid", "right"].map(|part| image(format!("{basic}btnbar_{part}2.bmp"))),
            tabs: std::array::from_fn(|tab| image(format!("{basic}tab_itm_0{}.bmp", tab + 1))),
            cell: image(format!("{basic}itemwin_mid.bmp")),
            count: image(format!("{UI}inventory\\icon_num.bmp")),
            resize: image(format!("{UI}btn_resize.bmp")),
        }
    }
}

/// Where a click on the window landed.
#[derive(Debug, PartialEq)]
pub enum Hit {
    Title(TitleHit),
    Tab(usize),
    /// An item, by inventory index.
    Item(u16),
    Window,
}

pub struct Inventory {
    /// Everything carried, in index order.
    pub items: Vec<InventoryItem>,
    /// Slots beyond the base 100.
    pub expansion: u16,
    /// Its title (a message number): the inventory's, or the cart's.
    pub title: usize,
    /// A cart's own limits: how many kinds, and its weight and most weight.
    pub cart: Option<(u16, [i32; 2])>,
    pub frame: Frame,
    tab: usize,
    /// First row shown.
    scroll: usize,
    clicks: Clicks,
}

impl Default for Inventory {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            expansion: 0,
            title: messages::INVENTORY,
            cart: None,
            frame: Frame::new(DEFAULT_POSITION, WIDTH),
            tab: 0,
            scroll: 0,
            clicks: Clicks::default(),
        }
    }
}

/// The tab an item type is listed under: usable, equipment or everything else.
pub fn tab_of(kind: u8) -> usize {
    match kind {
        net::ITEM_HEALING | net::ITEM_USABLE | net::ITEM_DELAY_CONSUME | net::ITEM_CASH => 0,
        net::ITEM_WEAPON | net::ITEM_ARMOR | net::ITEM_SHADOW_GEAR | net::ITEM_PET_EGG | net::ITEM_PET_ARMOR => 1,
        _ => 2,
    }
}

/// An item's name as the client shows it: refine level, name, card slots.
pub fn item_name(items: &HashMap<u32, ItemInfo>, id: u32, identified: bool, refine: u8) -> String {
    let Some(info) = items.get(&id) else { return id.to_string() };
    let mut name = info.name(identified).to_owned();
    if refine > 0 {
        name = format!("+{refine} {name}");
    }
    if identified && info.slots > 0 {
        name = format!("{name} [{}]", info.slots);
    }
    name
}

/// The icon file of an item.
pub fn icon_path(info: &ItemInfo, identified: bool) -> String {
    format!("{UI}item\\{}.bmp", info.resource(identified))
}

impl Inventory {
    pub fn new(open: bool) -> Self {
        let mut inventory = Self::default();
        inventory.frame.open = open;
        inventory
    }

    /// A fresh listing from the server replaces what we had.
    pub fn clear(&mut self) {
        self.items.clear();
    }

    pub fn extend(&mut self, items: Vec<InventoryItem>) {
        self.items.extend(items);
        self.items.sort_by_key(|item| item.index);
    }

    /// Picked-up items join their stack, or take a new slot.
    pub fn add(&mut self, item: InventoryItem) {
        match self.items.iter_mut().find(|existing| existing.index == item.index) {
            Some(existing) => existing.amount += item.amount,
            None => self.extend(vec![item]),
        }
    }

    pub fn remove(&mut self, index: u16, amount: u16) {
        if let Some(position) = self.items.iter().position(|item| item.index == index) {
            let item = &mut self.items[position];
            item.amount = item.amount.saturating_sub(amount);
            if item.amount == 0 {
                self.items.remove(position);
            }
        }
    }

    pub fn set_amount(&mut self, index: u16, amount: u16) {
        if let Some(item) = self.items.iter_mut().find(|item| item.index == index) {
            let removed = item.amount.saturating_sub(amount);
            self.remove(index, removed);
        }
    }

    /// Where an item is worn now (0: taken off).
    pub fn set_worn(&mut self, index: u16, location: u32) {
        if let Some(item) = self.items.iter_mut().find(|item| item.index == index) {
            item.worn = location;
        }
    }

    pub fn get(&self, index: u16) -> Option<&InventoryItem> {
        self.items.iter().find(|item| item.index == index)
    }

    /// The current tab's items; worn equipment shows in the equipment window instead.
    fn listed(&self) -> impl Iterator<Item = &InventoryItem> {
        self.items.iter().filter(|item| item.worn == 0 && tab_of(item.kind) == self.tab)
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let local = self.frame.local(cursor, HEIGHT)?;
        Some(if let Some(hit) = self.frame.title_hit(local) {
            Hit::Title(hit)
        } else if contains(TABS_AT, TABS_SIZE, local) {
            Hit::Tab((((local[1] - TABS_AT[1]) / (TABS_SIZE[1] / TABS as f32)) as usize).min(TABS - 1))
        } else if let Some(item) = self.cell_at(local).and_then(|cell| self.listed().nth(self.scroll * COLUMNS + cell)) {
            Hit::Item(item.index)
        } else {
            Hit::Window
        })
    }

    fn cell_at(&self, local: [f32; 2]) -> Option<usize> {
        let [x, y] = [local[0] - GRID[0], local[1] - GRID[1]];
        let [column, row] = [(x / CELL).floor(), (y / CELL).floor()];
        (x >= 0.0 && y >= 0.0 && (column as usize) < COLUMNS && (row as usize) < ROWS).then(|| row as usize * COLUMNS + column as usize)
    }

    /// Reacts to a click on the window: returns the item to use on a double click.
    pub fn click(&mut self, hit: Hit) -> Option<u16> {
        match hit {
            Hit::Title(hit) => self.frame.click_title(hit),
            Hit::Tab(tab) => {
                self.tab = tab;
                self.scroll = 0;
            }
            Hit::Item(index) => return self.clicks.double(index).then_some(index),
            Hit::Window => {}
        }
        None
    }

    pub fn height(&self) -> f32 {
        HEIGHT
    }

    /// The listed items' cells on screen (the open tab, the rows scrolled to): inventory index and box.
    pub fn item_boxes(&self) -> Vec<(u16, [f32; 4])> {
        if !self.frame.open || self.frame.minimized {
            return Vec::new();
        }
        self.listed()
            .skip(self.scroll * COLUMNS)
            .take(ROWS * COLUMNS)
            .enumerate()
            .map(|(cell, item)| {
                let corner = self.frame.at([GRID[0] + (cell % COLUMNS) as f32 * CELL, GRID[1] + (cell / COLUMNS) as f32 * CELL]);
                (item.index, [corner[0], corner[1], CELL, CELL])
            })
            .collect()
    }

    pub fn scroll_by(&mut self, rows: i32) {
        let total_rows = self.listed().count().div_ceil(COLUMNS);
        self.scroll = (self.scroll as i32 + rows).clamp(0, total_rows.saturating_sub(ROWS) as i32) as usize;
    }

    /// Draws the window into `layer` and the hovered item's name into `tooltip`.
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
        cursor: [f32; 2],
    ) {
        if !self.frame.open {
            return;
        }
        let at = |local: [f32; 2]| self.frame.at(local);
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        self.frame.draw_title(layer, chrome, message(self.title), cursor);
        if self.frame.minimized {
            return;
        }

        // Body: the striped tab column with its strip, the white item area between borders, and the cells.
        layer.sprites.push((&assets.footer[1], at([0.0, TITLE_HEIGHT]), [TAB_COLUMN, BODY_HEIGHT]));
        layer.sprites.push((&chrome.white, at([0.0, TITLE_HEIGHT]), [TABS_AT[0], BODY_HEIGHT]));
        layer.sprites.push((&assets.tabs[self.tab], at(TABS_AT), assets.tabs[self.tab].size));
        layer.sprites.push((&chrome.white, at([TAB_COLUMN, TITLE_HEIGHT]), [WIDTH - TAB_COLUMN, BODY_HEIGHT]));
        for x in [TAB_COLUMN, WIDTH - 1.0] {
            layer.sprites.push((&chrome.border, at([x, TITLE_HEIGHT]), [1.0, BODY_HEIGHT]));
        }
        for cell in 0..ROWS * COLUMNS {
            let place = [GRID[0] + (cell % COLUMNS) as f32 * CELL, GRID[1] + (cell / COLUMNS) as f32 * CELL];
            layer.sprites.push((&assets.cell, at(place), assets.cell.size));
        }

        // Items: icon, and how many unless it's equipment; the hovered one's name in a tooltip.
        for (cell, item) in self.listed().skip(self.scroll * COLUMNS).take(ROWS * COLUMNS).enumerate() {
            let place = at([GRID[0] + (cell % COLUMNS) as f32 * CELL, GRID[1] + (cell / COLUMNS) as f32 * CELL]);
            let info = items.get(&item.id);
            if let Some(Some(icon)) = info.and_then(|info| icons.get(&icon_path(info, item.identified))) {
                layer.sprites.push((icon, offset(place, [ICON_OFFSET, ICON_OFFSET]), icon.size));
            }
            if tab_of(item.kind) != 1 {
                let amount = item.amount.to_string();
                let corner = offset(place, [CELL - 1.0 - font.width(&amount), 15.0]);
                layer.texts.push((amount.clone(), offset(corner, [-1.0, -1.0]), [255; 3]));
                layer.texts.push((amount, corner, [0, 0, 0]));
            }
            if contains(place, [CELL, CELL], cursor) {
                let name = item_name(items, item.id, item.identified, item.refine);
                let text = messages::fill(message(messages::ITEM_AMOUNT), &[&name, &item.amount]);
                window::tooltip(tooltip, chrome, font, &text, offset(place, [35.0, 0.0]), if item.identified { [255; 3] } else { UNIDENTIFIED });
            }
        }

        // Footer: rounded bar with the item count and the resize grip.
        let footer_top = TITLE_HEIGHT + BODY_HEIGHT;
        let [left, middle, right] = &assets.footer;
        layer.sprites.push((left, at([0.0, footer_top]), left.size));
        layer.sprites.push((middle, at([left.size[0], footer_top]), [WIDTH - left.size[0] - right.size[0], FOOTER_HEIGHT]));
        layer.sprites.push((right, at([WIDTH - right.size[0], footer_top]), right.size));
        layer.sprites.push((&assets.count, at([8.0, footer_top + 8.0]), assets.count.size));
        let slots = self.cart.map_or(BASE_SLOTS + usize::from(self.expansion), |(most, _)| usize::from(most));
        layer.texts.push((format!("{}/{slots}", self.items.len()), at([22.0, footer_top + 8.0]), [0, 0, 0]));
        // A cart says how heavy it is.
        if let Some((_, [weight, most])) = self.cart {
            let text = format!("{} / {}", weight / 10, most / 10);
            layer.texts.push((text.clone(), at([WIDTH - 20.0 - font.width(&text), footer_top + 8.0]), [0, 0, 0]));
        }
        layer.sprites.push((&assets.resize, at([WIDTH - 14.0, HEIGHT - 14.0]), assets.resize.size));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(index: u16, kind: u8, amount: u16) -> InventoryItem {
        InventoryItem { index, id: 501, kind, amount, identified: true, ..Default::default() }
    }

    #[test]
    fn keeps_stacks_and_tabs() {
        let mut inventory = Inventory::new(true);
        inventory.extend(vec![item(3, net::ITEM_HEALING, 5), item(2, net::ITEM_WEAPON, 1), item(4, 3, 9)]);
        assert_eq!(inventory.items.iter().map(|item| item.index).collect::<Vec<_>>(), [2, 3, 4]);
        inventory.add(item(3, net::ITEM_HEALING, 2));
        assert_eq!(inventory.get(3).unwrap().amount, 7);
        inventory.remove(4, 9);
        assert!(inventory.get(4).is_none());
        inventory.set_amount(3, 1);
        assert_eq!(inventory.get(3).unwrap().amount, 1);

        // The first cell of the usable tab holds the potion; the equipment tab lists the weapon.
        let first_cell = offset(inventory.frame.position, offset(GRID, [5.0, 5.0]));
        assert_eq!(inventory.hit(first_cell), Some(Hit::Item(3)));
        let equipment_tab = offset(inventory.frame.position, [5.0, TITLE_HEIGHT + TABS_SIZE[1] / 2.0]);
        assert_eq!(inventory.hit(equipment_tab), Some(Hit::Tab(1)));
        inventory.click(Hit::Tab(1));
        assert_eq!(inventory.hit(first_cell), Some(Hit::Item(2)));
        assert_eq!(inventory.click(Hit::Item(2)), None);
        assert_eq!(inventory.click(Hit::Item(2)), Some(2));
    }
}
