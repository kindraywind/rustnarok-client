//! What the player carries, and its window (Alt+E), laid out like the classic client's inventory.

use std::collections::{HashMap, HashSet};

use rustnarok_client::grf::Grf;
use rustnarok_client::lua::ItemInfo;
use rustnarok_client::messages;
use rustnarok_client::net::{self, InventoryItem};
use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};
use crate::window::{self, Chrome, Clicks, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI, contains, offset};

// ponytail: sizes and offsets of the classic inventory over its bitmaps; calibrate against the real client.
/// Columns it opens with, and how far its corner grip takes them (248 to 312 wide in 32 px steps).
const COLUMNS: usize = 7;
const MOST_COLUMNS: std::ops::RangeInclusive<usize> = 6..=8;
/// Its tab strip is taller than the 240 px the grip allows, so it's always 6 rows high: the grip only widens it.
const ROWS: std::ops::RangeInclusive<usize> = 6..=6;
/// The cart has no tab strip: 120 to 216 high, so 2 to 5 rows.
const CART_ROWS: std::ops::RangeInclusive<usize> = 2..=5;
const CELL: f32 = 32.0;
const FOOTER_HEIGHT: f32 = 29.0;
/// The tab column, then 16px of white on either side of the cells (27 + 16 + 16 + columns).
const TAB_COLUMN: f32 = 27.0;
const MARGIN: f32 = 16.0;
/// The tab strip, after a 1px white edge; as tall as the body.
const TABS_AT: [f32; 2] = [1.0, TITLE_HEIGHT];
const TAB_WIDTH: f32 = 20.0;
const TABS: usize = 4;
/// The corner grip: the bottom-right 15 px of the window.
const GRIP: f32 = 15.0;
/// Favourites, after Item, Equip and Etc.
const FAVORITES_TAB: usize = 3;
/// "Compare equips" and "Lock item drop".
const COMPARE: usize = 2842;
const LOCK_DROP: usize = 2843;
/// The two boxes in the footer, and how wide each one is (box plus its label).
// ponytail: where the boxes sit isn't knowable; they share the footer, after the count.
const CHECK_X: f32 = 90.0;
const CHECK_SPAN: f32 = 86.0;
/// The first cell.
const GRID: [f32; 2] = [TAB_COLUMN + MARGIN, TITLE_HEIGHT + 8.0];
const ICON_OFFSET: f32 = 4.0;
pub const BASE_SLOTS: usize = 100;
/// Where the client first opens it.
const DEFAULT_POSITION: [f32; 2] = [0.0, 172.0];
pub const UNIDENTIFIED: [u8; 3] = [0xAA, 0xAA, 0xAA];
/// Broken and bound items' names.
// ponytail: rathena's comment says the client shows bound items in yellow; the exact colors aren't knowable.
pub const BROKEN: [u8; 3] = [0xFF, 0x50, 0x50];
pub const BOUND: [u8; 3] = [0xFF, 0xFF, 0x00];

/// The color of an item's name: broken red, bound yellow, unidentified grey.
pub fn name_color(item: &InventoryItem) -> [u8; 3] {
    if item.broken {
        BROKEN
    } else if item.bind == 2 {
        BOUND
    } else if item.identified {
        [255; 3]
    } else {
        UNIDENTIFIED
    }
}

/// The egg of the pet that's out: rathena marks it broken and still lists it when it resends the inventory. An egg
/// is armor worn nowhere.
pub fn hidden_egg(item: &InventoryItem) -> bool {
    item.broken && item.kind == net::ITEM_ARMOR && item.location == 0
}

/// What a resent list has broken that wasn't before (`before`: the broken items' indexes then).
pub fn newly_broken<'a>(before: &[u16], items: &'a [InventoryItem]) -> Vec<&'a InventoryItem> {
    items.iter().filter(|item| item.broken && !hidden_egg(item) && !before.contains(&item.index)).collect()
}

/// The window's own bitmaps, all from data.grf.
pub struct Assets {
    footer: [Texture; 3],
    tabs: [Texture; TABS],
    cell: Texture,
    count: Texture,
    resize: Texture,
    /// Marks an item in the Swap Equipment set.
    switch: Texture,
    /// Off, then on.
    checkbox: [Texture; 2],
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let basic = format!("{UI}basic_interface\\");
        let image = |path: String| gpu.load_texture(grf, &path, false);
        Self {
            footer: ["left", "mid", "right"].map(|part| image(format!("{basic}btnbar_{part}2.bmp"))),
            // ponytail: the data's three strips (tab_itm_01..03). The favourites tab reuses the etc strip, and the
            // column is split into four equal hit areas. A four-tab strip would replace both.
            tabs: std::array::from_fn(|tab| image(format!("{basic}tab_itm_0{}.bmp", tab.min(2) + 1))),
            cell: image(format!("{basic}itemwin_mid.bmp")),
            count: image(format!("{UI}inventory\\icon_num.bmp")),
            resize: image(format!("{UI}btn_resize.bmp")),
            switch: image(format!("{UI}swap_equipment\\ico_change.bmp")),
            checkbox: ["0", "1"].map(|look| image(format!("{UI}checkbox_{look}.bmp"))),
        }
    }
}

/// The inventory's two boxes, kept with the settings (`lock_drop` then `compare`, each 0 or 1).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Boxes {
    pub lock_drop: bool,
    pub compare: bool,
}

impl Boxes {
    pub fn parse(text: &str) -> Self {
        let mut words = text.split_whitespace();
        let on = |word: Option<&str>| word == Some("1");
        Self { lock_drop: on(words.next()), compare: on(words.next()) }
    }

    pub fn text(self) -> String {
        format!("{} {}", u8::from(self.lock_drop), u8::from(self.compare))
    }

    fn path() -> Option<std::path::PathBuf> {
        crate::settings_path("inventory")
    }

    pub fn load() -> Self {
        Self::path().and_then(|path| std::fs::read_to_string(path).ok()).map(|text| Self::parse(&text)).unwrap_or_default()
    }

    pub fn save(self) {
        let Some(path) = Self::path() else { return };
        let _ = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(path, self.text()));
    }
}

/// Letting an item go over the ground drops it. The lock stops that: no amount box, no drop.
pub fn may_ground_drop(lock_drop: bool) -> bool {
    !lock_drop
}

/// Worn items that sit in a slot the hovered item can go in. Only an unworn piece of equipment is compared.
pub fn compared_worn<'a>(hovered: &InventoryItem, items: &'a [InventoryItem]) -> Vec<&'a InventoryItem> {
    if hovered.worn != 0 || hovered.location == 0 || !equipment(hovered.kind) {
        return Vec::new();
    }
    items.iter().filter(|item| item.index != hovered.index && item.worn & hovered.location != 0).collect()
}

fn equipment(kind: u8) -> bool {
    matches!(kind, net::ITEM_WEAPON | net::ITEM_ARMOR | net::ITEM_SHADOW_GEAR | net::ITEM_PET_ARMOR)
}

/// Where a click on the window landed.
#[derive(Debug, PartialEq)]
pub enum Hit {
    Title(TitleHit),
    Tab(usize),
    /// An item, by inventory index.
    Item(u16),
    /// Lock item drop.
    LockDrop,
    /// Compare equips.
    Compare,
    /// The corner grip.
    Resize,
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
    /// Lock item drop, and compare the worn item while pointing at an unworn one.
    pub lock_drop: bool,
    pub compare: bool,
    /// Columns and rows of cells shown.
    columns: usize,
    rows: usize,
    /// Held by the corner grip: where the press was, and the columns and rows then.
    resizing: Option<([f32; 2], [usize; 2])>,
    /// How many rows the grip takes it to.
    row_limits: std::ops::RangeInclusive<usize>,
}

impl Default for Inventory {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            expansion: 0,
            title: messages::INVENTORY,
            cart: None,
            frame: Frame::new(DEFAULT_POSITION, width(COLUMNS)).collapsing(),
            tab: 0,
            scroll: 0,
            clicks: Clicks::default(),
            lock_drop: false,
            compare: false,
            columns: COLUMNS,
            rows: *ROWS.start(),
            resizing: None,
            row_limits: ROWS,
        }
    }
}

/// The window's width for `columns` of cells.
fn width(columns: usize) -> f32 {
    TAB_COLUMN + MARGIN + columns as f32 * CELL + MARGIN
}

/// The tab an item is listed under: a favourite only on the favourites tab, the rest by type.
pub fn listed_tab(item: &InventoryItem) -> usize {
    if item.favorite { FAVORITES_TAB } else { tab_of(item.kind) }
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

/// The cards' words in item names: each card's word (cardprefixnametable.txt), and the cards whose word goes after the
/// name (cardpostfixnametable.txt).
#[derive(Default)]
pub struct CardNames {
    words: HashMap<u32, String>,
    after: HashSet<u32>,
}

const CARD_WORDS: &str = "data\\cardprefixnametable.txt";
const CARDS_AFTER: &str = "data\\cardpostfixnametable.txt";
/// card0 of a forged weapon and of a brewed item; the maker's character ID is in cards 2 and 3.
const FORGED: u32 = 0x00FF;
const BREWED: u32 = 0x00FE;
/// "Very Strong ", "Very Very Strong ", "Very Very Very Strong ": a forged weapon's Star Crumbs.
const VERY_STRONG: usize = 459;
/// "'s ", then "'s Fire ", "'s Ice ", "'s Wind ", "'s Earth ": after the maker's name.
const MAKERS: usize = 450;
const NAMELESS: usize = 581;

impl CardNames {
    pub fn load(grf: &Grf) -> Self {
        let text = |path: &str| grf.read(path).map(|bytes| encoding_rs::EUC_KR.decode_without_bom_handling(&bytes).0.into_owned()).unwrap_or_default();
        Self::parse(&text(CARD_WORDS), &text(CARDS_AFTER))
    }

    /// `<card>#<word>#` lines, and `<card>#` lines for those after the name.
    pub fn parse(words: &str, after: &str) -> Self {
        let id = |line: &str| line.split('#').next().and_then(|id| id.trim().parse::<u32>().ok());
        Self {
            words: words.lines().filter_map(|line| Some((id(line)?, line.split('#').nth(1)?.trim().to_owned()))).collect(),
            after: after.lines().filter_map(id).collect(),
        }
    }
}

/// A forged or brewed item's maker (character ID), if it is one.
pub fn maker(cards: &[u32; 4]) -> Option<u32> {
    matches!(cards[0], FORGED | BREWED).then(|| (cards[2] & 0xFFFF) | (cards[3] << 16))
}

/// An item's whole name, as pickups and the inventory show it: unidentified, the unidentified name only; equipment,
/// "+N " and the grade "[X] ", then its cards' words before and after the name, or for a forged or brewed one "Very
/// Strong " and the maker's name with its element; then " [slots]" when `slots`. `makers` names makers by character
/// ID (unknown: "Nameless").
pub fn full_name(
    items: &HashMap<u32, ItemInfo>,
    cards: &CardNames,
    messages: &[String],
    makers: &HashMap<u32, Option<String>>,
    item: &InventoryItem,
    slots: bool,
) -> String {
    full_name_maker(items, cards, messages, makers, item, slots).0
}

/// The maker's name piece of a forged or brewed item's name: blue, or red when the maker has no name ("Nameless").
pub const MAKER_COLOR: [u8; 3] = [64, 64, 224];
pub const NAMELESS_COLOR: [u8; 3] = [224, 64, 64];
/// Where in a name the maker's name is, and its colour.
pub type MakerRun = (std::ops::Range<usize>, [u8; 3]);

/// The full name, and where in it the maker's name is with its colour (forged or brewed items).
pub fn full_name_maker(
    items: &HashMap<u32, ItemInfo>,
    cards: &CardNames,
    messages: &[String],
    makers: &HashMap<u32, Option<String>>,
    item: &InventoryItem,
    slots: bool,
) -> (String, Option<MakerRun>) {
    let Some(info) = items.get(&item.id) else { return (item.id.to_string(), None) };
    if !item.identified {
        return (info.name(false).to_owned(), None);
    }
    let equipment = matches!(item.kind, net::ITEM_WEAPON | net::ITEM_ARMOR | net::ITEM_SHADOW_GEAR);
    if !equipment {
        return (info.name(true).to_owned(), None);
    }
    let mut maker_run = None;
    let message = |id: usize| messages.get(id).map_or("", String::as_str);
    let mut name = String::new();
    if item.refine > 0 {
        name.push_str(&format!("+{} ", item.refine));
    }
    if let Some(grade) = ["D", "C", "B", "A"].get(usize::from(item.grade).wrapping_sub(1)) {
        name.push_str(&format!("[{grade}] "));
    }
    let mut after = String::new();
    match maker(&item.cards) {
        Some(maker) => {
            let strength = (item.cards[1] >> 8) / 5;
            if (1..=3).contains(&strength) {
                name.push_str(message(VERY_STRONG + strength as usize - 1));
            }
            let start = name.len();
            let known = makers.get(&maker).and_then(Option::as_deref);
            name.push_str(known.unwrap_or(message(NAMELESS)));
            maker_run = Some((start..name.len(), if known.is_some() { MAKER_COLOR } else { NAMELESS_COLOR }));
            // Water, earth, fire, wind (rathena's 1 to 4) have lines of their own; none is plain "'s ".
            let element = match item.cards[1] & 0x0F {
                1 => 2,
                2 => 4,
                3 => 1,
                4 => 3,
                _ => 0,
            };
            name.push_str(message(MAKERS + element));
        }
        None => {
            // ponytail: each card says its word once, in card order; the game's "Double"/"Triple" for repeats isn't
            // done.
            let mut seen = HashSet::new();
            for card in item.cards.iter().filter(|&&card| card != 0 && seen.insert(card)) {
                let Some(word) = cards.words.get(card) else { continue };
                if cards.after.contains(card) {
                    after.push_str(&format!(" {word}"));
                } else {
                    name.push_str(&format!("{word} "));
                }
            }
        }
    }
    name.push_str(info.name(true));
    name.push_str(&after);
    if slots && info.slots > 0 {
        name.push_str(&format!(" [{}]", info.slots));
    }
    (name, maker_run)
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

    /// The cart: the inventory's look, 2 to 5 rows.
    pub fn cart() -> Self {
        Self { rows: *CART_ROWS.start(), row_limits: CART_ROWS, ..Self::default() }
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
        self.items.iter().filter(|item| item.worn == 0 && listed_tab(item) == self.tab)
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let local = self.frame.local(cursor, self.height())?;
        let tabs = [TAB_WIDTH, self.body_height()];
        Some(if let Some(hit) = self.frame.title_hit(local) {
            Hit::Title(hit)
        } else if !self.frame.minimized && contains([self.width() - GRIP, self.height() - GRIP], [GRIP; 2], local) {
            Hit::Resize
        } else if contains(TABS_AT, tabs, local) {
            Hit::Tab((((local[1] - TABS_AT[1]) / (tabs[1] / TABS as f32)) as usize).min(TABS - 1))
        } else if let Some(hit) = self.check_at(local) {
            hit
        } else if let Some(item) = self.cell_at(local).and_then(|cell| self.listed().nth(self.scroll * self.columns + cell)) {
            Hit::Item(item.index)
        } else {
            Hit::Window
        })
    }

    /// The lock and compare boxes, in the player's inventory only (the cart shares this window).
    fn check_at(&self, local: [f32; 2]) -> Option<Hit> {
        if self.cart.is_some() {
            return None;
        }
        let top = TITLE_HEIGHT + self.body_height();
        if !(top..top + FOOTER_HEIGHT).contains(&local[1]) || local[0] < CHECK_X {
            return None;
        }
        match ((local[0] - CHECK_X) / CHECK_SPAN).floor() as i32 {
            0 => Some(Hit::LockDrop),
            1 => Some(Hit::Compare),
            _ => None,
        }
    }

    fn cell_at(&self, local: [f32; 2]) -> Option<usize> {
        let [x, y] = [local[0] - GRID[0], local[1] - GRID[1]];
        let [column, row] = [(x / CELL).floor(), (y / CELL).floor()];
        (x >= 0.0 && y >= 0.0 && (column as usize) < self.columns && (row as usize) < self.rows).then(|| row as usize * self.columns + column as usize)
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
            Hit::LockDrop => {
                self.lock_drop = !self.lock_drop;
                self.boxes().save();
            }
            Hit::Compare => {
                self.compare = !self.compare;
                self.boxes().save();
            }
            Hit::Resize | Hit::Window => {}
        }
        None
    }

    pub fn height(&self) -> f32 {
        TITLE_HEIGHT + self.body_height() + FOOTER_HEIGHT
    }

    fn width(&self) -> f32 {
        width(self.columns)
    }

    /// Rows of cells plus 8px above and 10px below them; the tab strip is exactly this tall.
    fn body_height(&self) -> f32 {
        self.rows as f32 * CELL + 18.0
    }

    /// Its columns and rows of cells.
    pub fn size(&self) -> [usize; 2] {
        [self.columns, self.rows]
    }

    /// Takes `columns` and `rows` as far as the grip would go; the items reflow.
    pub fn set_size(&mut self, [columns, rows]: [usize; 2]) {
        self.columns = columns.clamp(*MOST_COLUMNS.start(), *MOST_COLUMNS.end());
        self.rows = rows.clamp(*self.row_limits.start(), *self.row_limits.end());
        self.frame.set_width(self.width());
        self.scroll_by(0);
    }

    /// Held by the corner grip at `cursor`.
    pub fn start_resize(&mut self, cursor: [f32; 2]) {
        self.resizing = Some((cursor, self.size()));
    }

    /// While held by its grip, the corner follows the pointer from the press in whole cells (the window stays
    /// opaque); returns whether it's held.
    pub fn resize_to(&mut self, cursor: [f32; 2]) -> bool {
        let Some((from, [columns, rows])) = self.resizing else { return false };
        let step = |start: usize, moved: f32| (start as f32 + moved / CELL).round().max(0.0) as usize;
        self.set_size([step(columns, cursor[0] - from[0]), step(rows, cursor[1] - from[1])]);
        true
    }

    /// Lets go of the grip; returns whether it was held.
    pub fn stop_resize(&mut self) -> bool {
        self.resizing.take().is_some()
    }

    pub fn boxes(&self) -> Boxes {
        Boxes { lock_drop: self.lock_drop, compare: self.compare }
    }

    pub fn apply_boxes(&mut self, boxes: Boxes) {
        self.lock_drop = boxes.lock_drop;
        self.compare = boxes.compare;
    }

    /// The four tabs on screen (for the test driver): `tab:item`, `tab:equip`, `tab:etc`, `tab:favorite`.
    pub fn tab_boxes(&self) -> Vec<(&'static str, [f32; 4])> {
        if !self.frame.open || self.frame.minimized {
            return Vec::new();
        }
        let height = self.body_height() / TABS as f32;
        ["tab:item", "tab:equip", "tab:etc", "tab:favorite"]
            .into_iter()
            .enumerate()
            .map(|(tab, name)| {
                let [x, y] = self.frame.at([TABS_AT[0], TABS_AT[1] + tab as f32 * height]);
                (name, [x, y, TAB_WIDTH, height])
            })
            .collect()
    }

    /// The lock and compare boxes on screen.
    pub fn check_boxes(&self) -> Vec<(&'static str, [f32; 4])> {
        if !self.frame.open || self.frame.minimized || self.cart.is_some() {
            return Vec::new();
        }
        let y = self.frame.at([0.0, TITLE_HEIGHT + self.body_height()])[1];
        ["lock_drop", "compare"]
            .into_iter()
            .enumerate()
            .map(|(index, name)| {
                let x = self.frame.at([CHECK_X + index as f32 * CHECK_SPAN, 0.0])[0];
                (name, [x, y, CHECK_SPAN, FOOTER_HEIGHT])
            })
            .collect()
    }

    /// The listed items' cells on screen (the open tab, the rows scrolled to): inventory index and box.
    pub fn item_boxes(&self) -> Vec<(u16, [f32; 4])> {
        if !self.frame.open || self.frame.minimized {
            return Vec::new();
        }
        self.listed()
            .skip(self.scroll * self.columns)
            .take(self.rows * self.columns)
            .enumerate()
            .map(|(cell, item)| {
                let corner = self.frame.at([GRID[0] + (cell % self.columns) as f32 * CELL, GRID[1] + (cell / self.columns) as f32 * CELL]);
                (item.index, [corner[0], corner[1], CELL, CELL])
            })
            .collect()
    }

    pub fn scroll_by(&mut self, rows: i32) {
        let total_rows = self.listed().count().div_ceil(self.columns);
        self.scroll = (self.scroll as i32 + rows).clamp(0, total_rows.saturating_sub(self.rows) as i32) as usize;
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
        (cards, makers): (&CardNames, &HashMap<u32, Option<String>>),
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
        layer.sprites.push((&assets.footer[1], at([0.0, TITLE_HEIGHT]), [TAB_COLUMN, self.body_height()]));
        layer.sprites.push((&chrome.white, at([0.0, TITLE_HEIGHT]), [TABS_AT[0], self.body_height()]));
        layer.sprites.push((&assets.tabs[self.tab], at(TABS_AT), assets.tabs[self.tab].size));
        layer.sprites.push((&chrome.white, at([TAB_COLUMN, TITLE_HEIGHT]), [self.width() - TAB_COLUMN, self.body_height()]));
        for x in [TAB_COLUMN, self.width() - 1.0] {
            layer.sprites.push((&chrome.border, at([x, TITLE_HEIGHT]), [1.0, self.body_height()]));
        }
        for cell in 0..self.rows * self.columns {
            let place = [GRID[0] + (cell % self.columns) as f32 * CELL, GRID[1] + (cell / self.columns) as f32 * CELL];
            layer.sprites.push((&assets.cell, at(place), assets.cell.size));
        }

        // Items: icon, and how many unless it's equipment; the hovered one's name in a tooltip.
        for (cell, item) in self.listed().skip(self.scroll * self.columns).take(self.rows * self.columns).enumerate() {
            let place = at([GRID[0] + (cell % self.columns) as f32 * CELL, GRID[1] + (cell / self.columns) as f32 * CELL]);
            let info = items.get(&item.id);
            if let Some(Some(icon)) = info.and_then(|info| icons.get(&icon_path(info, item.identified))) {
                layer.sprites.push((icon, offset(place, [ICON_OFFSET, ICON_OFFSET]), icon.size));
            }
            // ponytail: the mark's corner isn't knowable.
            if item.switch != 0 {
                layer.sprites.push((&assets.switch, place, assets.switch.size));
            }
            if tab_of(item.kind) != 1 {
                let amount = item.amount.to_string();
                let corner = offset(place, [CELL - 1.0 - font.width(&amount), 15.0]);
                layer.texts.push((amount.clone(), offset(corner, [-1.0, -1.0]), [255; 3]));
                layer.texts.push((amount, corner, [0, 0, 0]));
            }
            if contains(place, [CELL, CELL], cursor) {
                let name = full_name(items, cards, messages, makers, item, true);
                let text = messages::fill(message(messages::ITEM_AMOUNT), &[&name, &item.amount]);
                window::tooltip(tooltip, chrome, font, &text, offset(place, [35.0, 0.0]), name_color(item));
            }
        }

        // Footer: rounded bar with the item count and the resize grip.
        let footer_top = TITLE_HEIGHT + self.body_height();
        let [left, middle, right] = &assets.footer;
        layer.sprites.push((left, at([0.0, footer_top]), left.size));
        layer.sprites.push((middle, at([left.size[0], footer_top]), [self.width() - left.size[0] - right.size[0], FOOTER_HEIGHT]));
        layer.sprites.push((right, at([self.width() - right.size[0], footer_top]), right.size));
        layer.sprites.push((&assets.count, at([8.0, footer_top + 8.0]), assets.count.size));
        let slots = self.cart.map_or(BASE_SLOTS + usize::from(self.expansion), |(most, _)| usize::from(most));
        layer.texts.push((format!("{}/{slots}", self.items.len()), at([22.0, footer_top + 8.0]), [0, 0, 0]));
        // A cart says how heavy it is.
        if let Some((_, [weight, most])) = self.cart {
            let text = format!("{} / {}", weight / 10, most / 10);
            layer.texts.push((text.clone(), at([self.width() - 20.0 - font.width(&text), footer_top + 8.0]), [0, 0, 0]));
        }
        layer.sprites.push((&assets.resize, at([self.width() - GRIP, self.height() - GRIP]), assets.resize.size));
        // The player's inventory: lock item drop, then compare equips.
        if self.cart.is_none() {
            for (index, (on, id)) in [(self.lock_drop, LOCK_DROP), (self.compare, COMPARE)].into_iter().enumerate() {
                let check = &assets.checkbox[usize::from(on)];
                let x = CHECK_X + index as f32 * CHECK_SPAN;
                let y = footer_top + ((FOOTER_HEIGHT - check.size[1]) / 2.0).floor();
                layer.sprites.push((check, at([x, y]), check.size));
                let label = window::fit(font, message(id), CHECK_SPAN - check.size[0] - 4.0);
                layer.texts.push((label, at([x + check.size[0] + 2.0, y]), [0, 0, 0]));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cart_stops_at_five_rows() {
        let mut cart = Inventory::cart();
        assert_eq!(cart.size(), [7, 2]);
        cart.set_size([8, 9]);
        assert_eq!(cart.size(), [8, 5]);
        cart.set_size([1, 1]);
        assert_eq!(cart.size(), [6, 2]);
    }

    #[test]
    fn resizes_by_its_corner_grip() {
        let mut inventory = Inventory::default();
        inventory.frame.open = true;
        let corner = inventory.frame.at([inventory.width() - 5.0, inventory.height() - 5.0]);
        assert_eq!(inventory.hit(corner), Some(Hit::Resize));
        // From 7 x 6: 40 px right makes 8 columns (whole cells, from the press); its height never changes.
        let height = inventory.height();
        inventory.start_resize(corner);
        assert!(inventory.resize_to([corner[0] + 40.0, corner[1] - 70.0]));
        assert_eq!(inventory.size(), [8, 6]);
        assert_eq!((inventory.width(), inventory.height()), (width(8), height));
        inventory.resize_to([corner[0] - 300.0, corner[1] + 500.0]);
        assert_eq!(inventory.size(), [6, 6]);
        assert!(inventory.stop_resize() && !inventory.resize_to(corner));
        // Items reflow into the columns shown, and the scroll stays in range.
        let mut cart = Inventory::cart();
        cart.items = (0..20).map(|index| InventoryItem { index, id: 501, kind: net::ITEM_HEALING, amount: 1, ..Default::default() }).collect();
        cart.set_size([6, 2]);
        cart.scroll_by(10);
        assert_eq!(cart.scroll, 2, "20 items in 6 columns: 4 rows, 2 shown");
        cart.set_size([8, 5]);
        assert_eq!(cart.scroll, 0);
    }

    #[test]
    fn names_items_in_full() {
        let info = |name: &str, unidentified: &str, slots| ItemInfo { name: name.into(), unidentified_name: unidentified.into(), slots, ..Default::default() };
        let items: HashMap<u32, ItemInfo> = [(1201, info("Knife", "Dagger", 3)), (501, info("Red Potion", "Red Potion", 0))].into_iter().collect();
        let cards = CardNames::parse("4001#Lucky#\n4014#of Hermes#\n", "4014#\n");
        let mut messages = vec![String::new(); 600];
        for (id, text) in [(450, "'s "), (451, "'s Fire "), (452, "'s Ice "), (459, "Very Strong "), (460, "Very Very Strong "), (581, "Nameless")] {
            messages[id] = text.into();
        }
        let makers: HashMap<u32, Option<String>> = [(150_004, Some("LaneA1".to_owned()))].into_iter().collect();
        let knife = |refine, grade, cards: [u32; 4]| InventoryItem {
            id: 1201,
            amount: 1,
            identified: true,
            kind: net::ITEM_WEAPON,
            refine,
            grade,
            cards,
            ..Default::default()
        };
        let name = |item: &InventoryItem, slots| full_name(&items, &cards, &messages, &makers, item, slots);
        // Refined, graded, carded before and after the name; slots in the chat line, not the popup.
        assert_eq!(name(&knife(7, 4, [4001, 4014, 0, 0]), true), "+7 [A] Lucky Knife of Hermes [3]");
        assert_eq!(name(&knife(0, 0, [4001, 4001, 0, 0]), false), "Lucky Knife");
        // Forged: Star Crumbs, the maker, the element (fire 3; water 1 is ice).
        let forged = |element: u32, crumbs: u32, maker: u32| knife(0, 0, [0x00FF, (crumbs * 5) << 8 | element, maker & 0xFFFF, maker >> 16]);
        assert_eq!(name(&forged(3, 1, 150_004), false), "Very Strong LaneA1's Fire Knife");
        assert_eq!(name(&forged(1, 2, 150_004), false), "Very Very Strong LaneA1's Ice Knife");
        assert_eq!(name(&forged(0, 0, 999), false), "Nameless's Knife");
        assert_eq!(maker(&forged(0, 0, 150_004).cards), Some(150_004));
        // Unidentified: its own name only; not equipment: the name.
        assert_eq!(name(&InventoryItem { identified: false, ..knife(7, 0, [4001, 0, 0, 0]) }, true), "Dagger");
        let potion = InventoryItem { id: 501, amount: 3, identified: true, kind: net::ITEM_HEALING, ..Default::default() };
        assert_eq!(name(&potion, true), "Red Potion");
    }

    fn item(index: u16, kind: u8, amount: u16) -> InventoryItem {
        InventoryItem { index, id: 501, kind, amount, identified: true, ..Default::default() }
    }

    #[test]
    fn tells_broken_and_bound_items() {
        let knife = InventoryItem { index: 4, id: 1201, kind: net::ITEM_WEAPON, location: 2, identified: true, broken: true, ..Default::default() };
        let egg = InventoryItem { index: 5, id: 9001, kind: net::ITEM_ARMOR, broken: true, identified: true, ..Default::default() };
        let shirt = InventoryItem { index: 6, id: 2301, kind: net::ITEM_ARMOR, location: 16, identified: true, bind: 2, ..Default::default() };
        let items = [knife.clone(), egg.clone(), shirt.clone()];
        // Only the knife is newly broken: the egg is the pet's, and one broken before stays quiet.
        assert_eq!(newly_broken(&[], &items).iter().map(|item| item.index).collect::<Vec<_>>(), [4]);
        assert!(newly_broken(&[4], &items).is_empty());
        assert!(hidden_egg(&egg) && !hidden_egg(&knife));
        assert_eq!([&knife, &shirt].map(name_color), [BROKEN, BOUND]);
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
        let equipment_tab = offset(inventory.frame.position, [5.0, TITLE_HEIGHT + inventory.body_height() / TABS as f32 * 1.5]);
        assert_eq!(inventory.hit(equipment_tab), Some(Hit::Tab(1)));
        inventory.click(Hit::Tab(1));
        assert_eq!(inventory.hit(first_cell), Some(Hit::Item(2)));
        assert_eq!(inventory.click(Hit::Item(2)), None);
        assert_eq!(inventory.click(Hit::Item(2)), Some(2));
    }

    #[test]
    fn lists_a_favourite_only_on_its_tab() {
        let plain = InventoryItem { index: 1, id: 501, kind: net::ITEM_HEALING, amount: 1, identified: true, ..Default::default() };
        let favourite = InventoryItem { favorite: true, ..plain.clone() };
        let etc = InventoryItem { index: 2, id: 909, kind: 3, amount: 1, identified: true, ..Default::default() };
        assert_eq!(listed_tab(&plain), 0);
        assert_eq!(listed_tab(&etc), 2);
        assert_eq!(listed_tab(&favourite), FAVORITES_TAB);
        // A favourite weapon leaves the equipment tab too.
        let weapon = InventoryItem { index: 3, kind: net::ITEM_WEAPON, favorite: true, ..plain.clone() };
        assert_eq!(listed_tab(&weapon), FAVORITES_TAB);

        let mut inventory = Inventory::new(true);
        inventory.extend(vec![favourite, etc]);
        let first_cell = offset(inventory.frame.position, offset(GRID, [5.0, 5.0]));
        inventory.click(Hit::Tab(2));
        assert_eq!(inventory.hit(first_cell), Some(Hit::Item(2)));
        inventory.click(Hit::Tab(FAVORITES_TAB));
        assert_eq!(inventory.hit(first_cell), Some(Hit::Item(1)));
        inventory.click(Hit::Tab(0));
        assert_eq!(inventory.hit(first_cell), Some(Hit::Window));
    }

    #[test]
    fn the_drop_lock_stops_a_ground_drop() {
        assert!(may_ground_drop(false));
        assert!(!may_ground_drop(true));
        assert_eq!(Boxes::parse("1 0"), Boxes { lock_drop: true, compare: false });
        assert_eq!(Boxes::parse("0 1").text(), "0 1");
        assert_eq!(Boxes::parse("").text(), "0 0");
    }

    #[test]
    fn matches_worn_items_in_the_hovered_slots() {
        let sword = InventoryItem { index: 1, kind: net::ITEM_WEAPON, location: 2, ..Default::default() };
        let worn = InventoryItem { index: 2, kind: net::ITEM_WEAPON, location: 2, worn: 2, ..Default::default() };
        let shield = InventoryItem { index: 3, kind: net::ITEM_ARMOR, location: 32, worn: 32, ..Default::default() };
        let potion = InventoryItem { index: 4, kind: net::ITEM_HEALING, ..Default::default() };
        let items = [sword.clone(), worn.clone(), shield.clone(), potion.clone()];
        assert_eq!(compared_worn(&sword, &items).iter().map(|item| item.index).collect::<Vec<_>>(), [2]);
        // A two-hander covers the weapon and the shield.
        let hands = InventoryItem { index: 5, kind: net::ITEM_WEAPON, location: 2 | 32, ..Default::default() };
        assert_eq!(compared_worn(&hands, &items).iter().map(|item| item.index).collect::<Vec<_>>(), [2, 3]);
        // Already worn, or not equipment: nothing beside it.
        assert!(compared_worn(&worn, &items).is_empty());
        assert!(compared_worn(&potion, &items).is_empty());
        // Both accessory slots, when the hovered piece can go in either.
        let left = InventoryItem { index: 6, kind: net::ITEM_ARMOR, location: 8, worn: 8, ..Default::default() };
        let right = InventoryItem { index: 7, kind: net::ITEM_ARMOR, location: 128, worn: 128, ..Default::default() };
        let ring = InventoryItem { index: 8, kind: net::ITEM_ARMOR, location: 8 | 128, ..Default::default() };
        assert_eq!(compared_worn(&ring, &[left, right]).iter().map(|item| item.index).collect::<Vec<_>>(), [6, 7]);
    }
}
