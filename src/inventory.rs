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
            switch: image(format!("{UI}swap_equipment\\ico_change.bmp")),
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
    let Some(info) = items.get(&item.id) else { return item.id.to_string() };
    if !item.identified {
        return info.name(false).to_owned();
    }
    let equipment = matches!(item.kind, net::ITEM_WEAPON | net::ITEM_ARMOR | net::ITEM_SHADOW_GEAR);
    if !equipment {
        return info.name(true).to_owned();
    }
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
            name.push_str(makers.get(&maker).and_then(Option::as_deref).unwrap_or(message(NAMELESS)));
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

    /// The three tabs on screen (for the test driver): `tab:item`, `tab:equip`, `tab:etc`.
    pub fn tab_boxes(&self) -> Vec<(&'static str, [f32; 4])> {
        if !self.frame.open || self.frame.minimized {
            return Vec::new();
        }
        let height = TABS_SIZE[1] / TABS as f32;
        ["tab:item", "tab:equip", "tab:etc"]
            .into_iter()
            .enumerate()
            .map(|(tab, name)| {
                let [x, y] = self.frame.at([TABS_AT[0], TABS_AT[1] + tab as f32 * height]);
                (name, [x, y, TABS_SIZE[0], height])
            })
            .collect()
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
        let equipment_tab = offset(inventory.frame.position, [5.0, TITLE_HEIGHT + TABS_SIZE[1] / 2.0]);
        assert_eq!(inventory.hit(equipment_tab), Some(Hit::Tab(1)));
        inventory.click(Hit::Tab(1));
        assert_eq!(inventory.hit(first_cell), Some(Hit::Item(2)));
        assert_eq!(inventory.click(Hit::Item(2)), None);
        assert_eq!(inventory.click(Hit::Item(2)), Some(2));
    }
}
