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

// ponytail: sizes and offsets of the classic cart window over its bitmaps; calibrate against the real client.
/// Columns it opens with, and how far its corner grip takes them (248 to 312 wide in 32 px steps).
const COLUMNS: usize = 7;
const COLUMN_LIMITS: std::ops::RangeInclusive<usize> = 6..=8;
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
const EQUIPMENT_TAB: usize = 1;
/// A Swap Equipment item's mark: its backing drawn at this alpha, then the mark, from the icon's corner.
const SWAP_ALPHA: f32 = 34.0 / 255.0;
/// The name box: its corner this far above the cell.
const NAME_BOX_ABOVE: f32 = 16.0;
/// "Compare equips" and "Lock item drop".
const COMPARE: usize = 2842;
const LOCK_DROP: usize = 2843;
/// The first cell.
const GRID: [f32; 2] = [TAB_COLUMN + MARGIN, TITLE_HEIGHT + 8.0];
const ICON_OFFSET: f32 = 4.0;
pub const BASE_SLOTS: usize = 100;
/// Where the client first opens the cart.
const DEFAULT_POSITION: [f32; 2] = [0.0, 172.0];

// The player's inventory (the cart above keeps its older look).
/// Where it first opens, and the size it asks for then.
const INVENTORY_POSITION: [f32; 2] = [0.0, 120.0];
const INVENTORY_SIZE: [f32; 2] = [280.0, 120.0];
/// The first cell's corner, and the width right of the cells: 40 + 32·columns + 16 wide.
const CELLS_AT: [f32; 2] = [40.0, TITLE_HEIGHT];
const CELLS_RIGHT: f32 = 16.0;
/// The footer, and the title and footer together.
const INVENTORY_FOOTER: f32 = 21.0;
const CHROME_HEIGHT: f32 = TITLE_HEIGHT + INVENTORY_FOOTER;
/// How far a size may be asked for before it snaps.
const WIDTH_RANGE: [f32; 2] = [232.0, 320.0];
const HEIGHT_RANGE: [f32; 2] = [102.0, 240.0];
/// The tab strip, 17 wide; each tab as tall as its longest label's characters stacked, plus this.
const STRIP_AT: [f32; 2] = [3.0, 18.0];
const STRIP_WIDTH: f32 = 17.0;
const TAB_PADDING: f32 = 10.0;
/// Use, Eqp, Etc, Fav.
const TAB_LABELS: [usize; TABS] = [1465, 2050, 1471, 2051];
const TAB_BORDER: [u8; 4] = [0xC3, 0xC3, 0xC3, 0xFF];
/// The line between the strip and the cells.
const STRIP_LINE: [u8; 4] = [0xC0, 0xC0, 0xC0, 0xFF];
const STRIP_LINE_X: f32 = 20.0;
/// Footer controls: their corners, x from the left and y up from the bottom.
const EXPAND_AT: [f32; 2] = [23.0, 17.0];
const COUNT_AT: [f32; 2] = [35.0, 15.0];
const LOCK_AT: [f32; 2] = [79.0, 16.0];
const COMPARE_AT: [f32; 2] = [103.0, 17.0];
/// "Expand inventory limit".
const EXPAND_TIP: usize = 3560;
/// The count: what's carried, red once it reaches the limit.
const COUNT_COLOR: [u8; 3] = [0x23, 0x23, 0x23];
const FULL_COLOR: [u8; 3] = [0xFF, 0x00, 0x00];
/// The wheel moves this many rows a notch (fewer when fewer show).
const WHEEL_ROWS: usize = 3;

/// The inventory's width asked for, snapped: 248, 280 or 312 (6, 7 or 8 columns).
pub fn snap_width(asked: f32) -> f32 {
    match asked.clamp(WIDTH_RANGE[0], WIDTH_RANGE[1]) {
        width if width <= 248.0 => 248.0,
        width if width < 312.0 => 280.0,
        _ => 312.0,
    }
}

/// The inventory's height asked for, snapped down to `strip` + 38 + 32·k, and at least `strip` + 38.
pub fn snap_height(asked: f32, strip: f32) -> f32 {
    let least = strip + CHROME_HEIGHT;
    let height = asked.clamp(HEIGHT_RANGE[0], HEIGHT_RANGE[1]).max(least);
    least + CELL * ((height - least) / CELL).floor()
}

/// The columns and rows of cells an inventory this big holds.
fn cells_of([width, height]: [f32; 2]) -> [usize; 2] {
    [((width - CELLS_AT[0]) / CELL) as usize, ((height - CHROME_HEIGHT) / CELL).max(0.0) as usize]
}

/// How tall the tab strip is for tabs `tab` px tall: neighbours share a border line.
fn strip_height(tab: f32) -> f32 {
    1.0 + TABS as f32 * (tab - 1.0)
}
pub const UNIDENTIFIED: [u8; 3] = [0xAA, 0xAA, 0xAA];
/// A bound item's name.
pub const BOUND: [u8; 3] = [0xFF, 0xFF, 0x00];
/// An item's name by how many enchants it has, 1 to 4.
pub const ENCHANTED: [[u8; 3]; 4] = [[0xAD, 0xFF, 0x2F], [0x00, 0xBF, 0xFF], [0xFF, 0xA5, 0x00], [0x94, 0x00, 0xD3]];

/// The cart's word after an amount, "ea".
const CART_EA: usize = 588;

/// What names the items: the card names, the makers' names, and the random options' texts when the Extras show them.
pub type Names<'a> = (&'a CardNames, &'a HashMap<u32, Option<String>>, Option<&'a HashMap<u16, String>>);

/// The hover name box's text: the name and how many (183); an identified item with random options also says how many
/// options (2551).
pub fn hover_text(messages: &[String], name: &str, item: &InventoryItem) -> String {
    let message = |id: usize| messages.get(id).map_or("", String::as_str);
    match item.options.len() {
        options if item.identified && options > 0 => messages::fill(message(messages::ITEM_OPTIONS_AMOUNT), &[&name, &options, &item.amount]),
        _ => messages::fill(message(messages::ITEM_AMOUNT), &[&name, &item.amount]),
    }
}

/// A name balloon's text: the name, or for an identified item with random options "%s [%d Option]" (2600).
pub fn option_text(messages: &[String], name: &str, item: &InventoryItem) -> String {
    match item.options.len() {
        options if item.identified && options > 0 => messages::fill(messages.get(messages::ITEM_OPTIONS).map_or("", String::as_str), &[&name, &options]),
        _ => name.to_owned(),
    }
}

/// The colour of an item's name in its name box and balloons, the first that fits: unidentified grey; by its enchants;
/// bound yellow; else white. A broken item has none of its own.
pub fn name_color(item: &InventoryItem, items: &HashMap<u32, ItemInfo>) -> [u8; 3] {
    if !item.identified {
        return UNIDENTIFIED;
    }
    match enchants(item, items) {
        0 if item.bind == 2 => BOUND,
        0 => [255; 3],
        count => ENCHANTED[count.min(4) - 1],
    }
}

/// An item's enchants: the cards past its slots that are known items. Pet eggs and accessories, forged and brewed
/// pieces (and anything whose first card marks a pet) have none.
pub fn enchants(item: &InventoryItem, items: &HashMap<u32, ItemInfo>) -> usize {
    if matches!(item.kind, net::ITEM_PET_EGG | net::ITEM_PET_ARMOR) || maker(&item.cards).is_some() || item.cards[0] == PET_CARD {
        return 0;
    }
    let slots = items.get(&item.id).map_or(0, |info| usize::from(info.slots));
    item.cards.iter().skip(slots).filter(|card| **card != 0 && items.contains_key(card)).count()
}

/// Ctrl + right press on a cell: whether to say there's no Magnifier (any tab, any item), and whether to ask to
/// identify it (only an unidentified one on the Eqp tab, also right after that line; the server turns it down).
pub fn ctrl_identify(tab: usize, identified: bool, magnifier: bool) -> (bool, bool) {
    (!magnifier, tab == EQUIPMENT_TAB && !identified)
}

/// The amount's corner in its cell: under 1000 at (18, 19), longer at (4, 19).
pub fn amount_at(amount: u16) -> [f32; 2] {
    if amount < 1000 { [18.0, 19.0] } else { [4.0, 19.0] }
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
    /// The inventory's own: its footer, the side pieces, the tabs' fill and lines, and the footer icons.
    plain_footer: [Texture; 3],
    side: [Texture; 2],
    tab_border: Texture,
    strip_line: Texture,
    lock: [Texture; 2],
    compare: [Texture; 2],
    tabs: [Texture; TABS],
    cell: Texture,
    count: Texture,
    resize: Texture,
    /// Marks an item in the Swap Equipment set, and the faint box under that mark.
    switch: Texture,
    switch_back: Texture,
    /// A new item's mark, and the enchant grades' icons 1 to 4.
    new_mark: Texture,
    grades: [Texture; 4],
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let basic = format!("{UI}basic_interface\\");
        let image = |path: String| gpu.load_texture(grf, &path, false);
        Self {
            footer: ["left", "mid", "right"].map(|part| image(format!("{basic}btnbar_{part}2.bmp"))),
            plain_footer: ["left", "mid", "right"].map(|part| image(format!("{basic}btnbar_{part}.bmp"))),
            side: ["left", "right"].map(|part| image(format!("{basic}itemwin_{part}.bmp"))),
            tab_border: gpu.solid(TAB_BORDER),
            strip_line: gpu.solid(STRIP_LINE),
            lock: ["off", "on"].map(|look| image(format!("{UI}inventory\\item_drop_lock_{look}.bmp"))),
            compare: ["off", "on"].map(|look| image(format!("{UI}inventory\\item_compare_{look}.bmp"))),
            // ponytail: the data's three strips (tab_itm_01..03). The favourites tab reuses the etc strip, and the
            // column is split into four equal hit areas. A four-tab strip would replace both.
            tabs: std::array::from_fn(|tab| image(format!("{basic}tab_itm_0{}.bmp", tab.min(2) + 1))),
            cell: image(format!("{basic}itemwin_mid.bmp")),
            count: image(format!("{UI}inventory\\icon_num.bmp")),
            resize: image(format!("{UI}btn_resize.bmp")),
            switch: image(format!("{UI}swap_equipment\\ico_change.bmp")),
            switch_back: crate::render::load_image(grf, &format!("{UI}swap_equipment\\bg_change.bmp"))
                .map(|mut image| {
                    image.pixels_mut().for_each(|pixel| pixel.0[3] = (f32::from(pixel.0[3]) * SWAP_ALPHA).round() as u8);
                    gpu.texture(&image, false)
                })
                .unwrap_or_else(|_| gpu.solid([0, 0, 0, 0])),
            new_mark: image(format!("{basic}new_item.bmp")),
            grades: std::array::from_fn(|grade| image(format!("{UI}grade_enchant\\grade_icon{}.bmp", grade + 1))),
        }
    }
}

/// The inventory's two boxes, kept with the settings (`lock_drop` then `compare`, each 0 or 1). Compare starts on when
/// it was never saved, as in the game (`ItemComparison`, on when unset).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Boxes {
    pub lock_drop: bool,
    pub compare: bool,
}

impl Boxes {
    pub fn parse(text: &str) -> Self {
        let mut words = text.split_whitespace();
        let on = |word: Option<&str>| word == Some("1");
        Self { lock_drop: on(words.next()), compare: words.next().is_none_or(|word| word == "1") }
    }

    pub fn text(self) -> String {
        format!("{} {}", u8::from(self.lock_drop), u8::from(self.compare))
    }

    fn path() -> Option<std::path::PathBuf> {
        crate::settings_path("inventory")
    }

    pub fn load() -> Self {
        Self::path().and_then(|path| std::fs::read_to_string(path).ok()).map_or(Self::parse(""), |text| Self::parse(&text))
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
    /// Ask to grow the inventory.
    Expand,
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
    /// Items that came since the inventory was last closed (by index): they carry the new mark.
    new_marks: HashSet<u16>,
    /// Open at the last `note_open`: closing it clears the marks.
    was_open: bool,
    /// Lock item drop, and compare the worn item while pointing at an unworn one.
    pub lock_drop: bool,
    pub compare: bool,
    /// Columns and rows of cells shown.
    columns: usize,
    rows: usize,
    /// Held by the corner grip: where the press was, and the columns and rows then (and the size, for the inventory).
    resizing: Option<([f32; 2], [usize; 2], [f32; 2])>,
    /// How many rows the grip takes it to.
    row_limits: std::ops::RangeInclusive<usize>,
    /// The cart window, which keeps the older look; else the player's inventory.
    is_cart: bool,
    /// The inventory's height, as snapped, and its tabs' height.
    inventory_height: f32,
    tab_height: f32,
}

impl Default for Inventory {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            expansion: 0,
            title: messages::INVENTORY,
            cart: None,
            frame: Frame::new(INVENTORY_POSITION, INVENTORY_SIZE[0]).collapsing(),
            tab: 0,
            scroll: 0,
            clicks: Clicks::default(),
            new_marks: HashSet::new(),
            was_open: false,
            lock_drop: false,
            compare: false,
            columns: COLUMNS,
            rows: *ROWS.start(),
            resizing: None,
            row_limits: ROWS,
            is_cart: false,
            inventory_height: INVENTORY_SIZE[1],
            tab_height: DEFAULT_TAB_HEIGHT,
        }
        .resized(INVENTORY_SIZE)
    }
}

/// Tabs for three-character labels at a 15 px line, until the labels are known (`set_tab_labels`).
const DEFAULT_TAB_HEIGHT: f32 = 3.0 * 15.0 + TAB_PADDING;

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

/// An item's name from its ID alone, as for an item the game knows no more of: identified, "+N " for a refine the caller
/// knows, the name and " [slots]"; unidentified, the unidentified name only.
pub fn item_name(items: &HashMap<u32, ItemInfo>, id: u32, identified: bool, refine: u8) -> String {
    let Some(info) = items.get(&id) else { return id.to_string() };
    if !identified {
        return info.name(false).to_owned();
    }
    let mut name = info.name(true).to_owned();
    if refine > 0 {
        name = format!("+{refine} {name}");
    }
    if info.slots > 0 {
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
/// card0 of a forged weapon and of a brewed or crafted item; the maker's character ID is in cards 2 and 3.
const FORGED: u32 = 0x00FF;
const BREWED: u32 = 0x00FE;
/// A pet egg's first card.
const PET_CARD: u32 = 0x0100;
/// "Double", "Triple", "Quadruple": one card in two, three or four slots.
const REPEATS: usize = 446;
/// "Very Strong ", "Very Very Strong ", "Very Very Very Strong ": a forged weapon's Star Crumbs.
const VERY_STRONG: usize = 459;
/// "'s ", then "'s Fire ", "'s Water ", "'s Wind ", "'s Earth ": after the maker's name.
const MAKERS: usize = 450;
const NAMELESS: usize = 581;
/// A wedding ring's words after the maker's name, instead of "'s ".
const WEDDING_RINGS: [u32; 2] = [2634, 2635];
const WEDDED: usize = 2976;
/// "Beloved ": before a renamed pet's egg or accessory.
const BELOVED: usize = 756;

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

    /// A card's word in item names, if it has one.
    pub fn word(&self, card: u32) -> Option<&str> {
        self.words.get(&card).map(String::as_str).filter(|word| !word.is_empty())
    }

    /// The words that go before the name (or `after` it), in slot order: each card that has one, once, with how many
    /// slots hold it.
    fn of(&self, cards: &[u32; 4], after: bool) -> Vec<(&str, usize)> {
        let mut words = Vec::new();
        for (slot, card) in cards.iter().enumerate() {
            if cards[..slot].contains(card) || self.after.contains(card) != after {
                continue;
            }
            if let Some(word) = self.words.get(card).filter(|word| !word.is_empty()) {
                words.push((word.as_str(), cards.iter().filter(|other| *other == card).count()));
            }
        }
        words
    }
}

/// A forged or brewed item's maker (character ID), if it is one.
pub fn maker(cards: &[u32; 4]) -> Option<u32> {
    matches!(cards[0], FORGED | BREWED).then(|| (cards[2] & 0xFFFF) | (cards[3] << 16))
}

/// Pet eggs and pet accessories, by ID.
fn pet_item(id: u32) -> bool {
    (9001..=9499).contains(&id) || (10_000..=10_999).contains(&id)
}

/// What item names take besides the item table and the messages: the cards' words and the makers' names.
pub type Naming<'a> = (&'a CardNames, &'a HashMap<u32, Option<String>>);

/// An item's whole name, as every window shows it; see `full_name_maker`.
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

/// The maker's name piece of a forged or brewed item's name, "Nameless" too.
pub const MAKER_COLOR: [u8; 3] = [64, 64, 224];
/// Where in a name the maker's name is, and its colour.
pub type MakerRun = (std::ops::Range<usize>, [u8; 3]);

/// An item's whole name, and where in it the maker's name is with its colour:
/// - unidentified: the unidentified name only;
/// - a renamed pet's egg or accessory (card3 1): "Beloved " and the name;
/// - brewed or crafted (card0 0xFE, card1 0), of any type, and forged equipment (card0 0xFF, or 0xFE with card1):
///   "+N ", the grade "[X] ", "Very Strong " for 1 to 3 Star Crumbs, the maker's name (`makers` by character ID,
///   unknown: "Nameless"), "'s " or its element's words ("With Mr/Miss" on a wedding ring), the name; no slots;
/// - other equipment: "+N ", "[X] ", the cards' words before the name ("Double " and so on for a card in several
///   slots), the name, the words after it (each followed by its " Double"), then " [slots]" when `slots`;
/// - anything else: the name, then " [slots]" when `slots`.
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
    let message = |id: usize| messages.get(id).map_or("", String::as_str);
    if pet_item(item.id) && item.cards[3] == 1 {
        return (format!("{}{}", message(BELOVED), info.name(true)), None);
    }
    let slot_count = if slots && info.slots > 0 { format!(" [{}]", info.slots) } else { String::new() };
    let equipment = matches!(item.kind, net::ITEM_WEAPON | net::ITEM_ARMOR | net::ITEM_SHADOW_GEAR);
    let made = item.cards[0] == BREWED && item.cards[1] == 0 || equipment && maker(&item.cards).is_some();
    if !equipment && !made {
        return (format!("{}{slot_count}", info.name(true)), None);
    }
    let mut name = String::new();
    if item.refine > 0 {
        name.push_str(&format!("+{} ", item.refine));
    }
    if let Some(grade) = ["D", "C", "B", "A"].get(usize::from(item.grade).wrapping_sub(1)) {
        name.push_str(&format!("[{grade}] "));
    }
    if made {
        // card1: the Star Crumbs times 5 in its high byte, the element in its low byte.
        if let Some(strength) = [5, 10, 15].iter().position(|&crumbs| (item.cards[1] >> 8) & 0xFF == crumbs) {
            name.push_str(message(VERY_STRONG + strength));
        }
        let start = name.len();
        let maker = maker(&item.cards).unwrap_or_default();
        name.push_str(makers.get(&maker).and_then(Option::as_deref).unwrap_or(message(NAMELESS)));
        let maker_run = Some((start..name.len(), MAKER_COLOR));
        // rathena's water, earth, fire and wind (1 to 4) have lines of their own; past 4, nothing.
        let after_maker = match item.cards[1] & 0xFF {
            0 if WEDDING_RINGS.contains(&item.id) => Some(WEDDED),
            0 => Some(MAKERS),
            1 => Some(MAKERS + 2),
            2 => Some(MAKERS + 4),
            3 => Some(MAKERS + 1),
            4 => Some(MAKERS + 3),
            _ => None,
        };
        name.push_str(after_maker.map_or("", message));
        name.push_str(info.name(true));
        return (name, maker_run);
    }
    // A pet's egg or accessory keeps the pet's number in its cards: no words.
    let words = |after| if pet_item(item.id) { Vec::new() } else { cards.of(&item.cards, after) };
    let repeat = |count: usize| (2..=4).contains(&count).then(|| message(REPEATS + count - 2));
    for (word, count) in words(false) {
        if let Some(repeat) = repeat(count) {
            name.push_str(&format!("{repeat} "));
        }
        name.push_str(&format!("{word} "));
    }
    name.push_str(info.name(true));
    for (word, count) in words(true) {
        name.push_str(&format!(" {word}"));
        if let Some(repeat) = repeat(count) {
            name.push_str(&format!(" {repeat}"));
        }
    }
    name.push_str(&slot_count);
    (name, None)
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

    /// The cart: the older inventory look, 2 to 5 rows.
    pub fn cart() -> Self {
        let mut cart = Self { rows: *CART_ROWS.start(), row_limits: CART_ROWS, is_cart: true, columns: COLUMNS, ..Self::default() };
        cart.frame = Frame::new(DEFAULT_POSITION, width(COLUMNS)).collapsing();
        cart
    }

    /// The tabs' height from their labels, written down one character under the other.
    pub fn set_tab_labels(&mut self, messages: &[String], font: &Font) {
        let longest = TAB_LABELS.iter().map(|&id| messages.get(id).map_or(0, |label| label.chars().count())).max().unwrap_or(0);
        self.tab_height = longest as f32 * font.line_height() + TAB_PADDING;
        self.set_size(self.size());
    }

    /// The inventory snapped to a size asked for.
    fn resized(mut self, asked: [f32; 2]) -> Self {
        self.resize_inventory(asked);
        self
    }

    fn resize_inventory(&mut self, [width, height]: [f32; 2]) {
        let size = [snap_width(width), snap_height(height, strip_height(self.tab_height))];
        [self.columns, self.rows] = cells_of(size);
        self.inventory_height = size[1];
        self.frame.set_width(size[0]);
        self.scroll_by(0);
    }

    /// A fresh listing from the server replaces what we had.
    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// Items as they come: the inventory keeps the order they arrive in, the cart sorts them by index.
    pub fn extend(&mut self, items: Vec<InventoryItem>) {
        self.items.extend(items);
        if self.is_cart {
            self.items.sort_by_key(|item| item.index);
        }
    }

    /// Picked-up items join their stack where it is, or take a new slot at the end.
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

    /// Where an item is worn now (0: taken off). One taken off goes to the end of the inventory's list.
    pub fn set_worn(&mut self, index: u16, location: u32) {
        let Some(position) = self.items.iter().position(|item| item.index == index) else { return };
        let taken_off = self.items[position].worn != 0 && location == 0;
        self.items[position].worn = location;
        if taken_off && !self.is_cart {
            let item = self.items.remove(position);
            self.items.push(item);
        }
    }

    /// An item that came through a pickup: marked new until the inventory closes.
    pub fn mark_new(&mut self, index: u16) {
        self.new_marks.insert(index);
    }

    /// Each frame: the inventory closing clears the new marks, when its open tab lists something.
    pub fn note_open(&mut self) {
        if self.was_open && !self.frame.open && self.listed().next().is_some() {
            self.new_marks.clear();
        }
        self.was_open = self.frame.open;
    }

    /// The items carrying the new mark, by index (for the test driver).
    pub fn new_marked(&self) -> Vec<u16> {
        let mut marked: Vec<u16> = self.new_marks.iter().copied().collect();
        marked.sort_unstable();
        marked
    }

    /// The open tab.
    pub fn open_tab(&self) -> usize {
        self.tab
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
        } else if !self.is_cart {
            self.inventory_hit(local)
        } else if contains(TABS_AT, tabs, local) {
            Hit::Tab((((local[1] - TABS_AT[1]) / (tabs[1] / TABS as f32)) as usize).min(TABS - 1))
        } else if let Some(item) = self.cell_at(local).and_then(|cell| self.listed().nth(self.scroll * self.columns + cell)) {
            Hit::Item(item.index)
        } else {
            Hit::Window
        })
    }

    /// The inventory's tabs, footer controls and cells (the cart goes by the older layout).
    fn inventory_hit(&self, local: [f32; 2]) -> Hit {
        if let Some(tab) = (0..TABS).find(|&tab| contains(self.tab_corner(tab), [STRIP_WIDTH, self.tab_height], local)) {
            return Hit::Tab(tab);
        }
        if let Some((_, hit)) = self.footer_controls().into_iter().find(|(area, _)| contains([area[0], area[1]], [area[2], area[3]], local)) {
            return hit;
        }
        match self.cell_at(local).and_then(|cell| self.listed().nth(self.scroll * self.columns + cell)) {
            Some(item) => Hit::Item(item.index),
            None => Hit::Window,
        }
    }

    /// A tab's corner in the strip: neighbours share a border line.
    fn tab_corner(&self, tab: usize) -> [f32; 2] {
        [STRIP_AT[0], STRIP_AT[1] + tab as f32 * (self.tab_height - 1.0)]
    }

    /// The footer's icons, window-local: expand, the drop lock, compare.
    fn footer_controls(&self) -> [([f32; 4], Hit); 3] {
        let bottom = self.height();
        let place = |[x, up]: [f32; 2], [width, height]: [f32; 2]| [x, bottom - up, width, height];
        [(place(EXPAND_AT, [11.0, 14.0]), Hit::Expand), (place(LOCK_AT, [14.0, 11.0]), Hit::LockDrop), (place(COMPARE_AT, [12.0, 12.0]), Hit::Compare)]
    }

    fn cell_at(&self, local: [f32; 2]) -> Option<usize> {
        // The inventory's cells take x from 40 to its width less 20.
        if !self.is_cart && !(CELLS_AT[0]..self.width() - 20.0).contains(&local[0]) {
            return None;
        }
        let grid = self.grid();
        let [x, y] = [local[0] - grid[0], local[1] - grid[1]];
        let [column, row] = [(x / CELL).floor(), (y / CELL).floor()];
        (x >= 0.0 && y >= 0.0 && (column as usize) < self.columns && (row as usize) < self.rows).then(|| row as usize * self.columns + column as usize)
    }

    /// Reacts to a click on the window: returns the item to use on a double click.
    pub fn click(&mut self, hit: Hit) -> Option<u16> {
        match hit {
            Hit::Title(hit) => self.frame.click_title(hit),
            // The inventory keeps its scroll row across tabs, as far as the new tab goes.
            Hit::Tab(tab) => {
                self.tab = tab;
                if self.is_cart {
                    self.scroll = 0;
                }
                self.scroll_by(0);
            }
            Hit::Item(index) => return self.clicks.double(index).then_some(index),
            // Not kept: off at every start.
            Hit::LockDrop => self.lock_drop = !self.lock_drop,
            Hit::Compare => {
                self.compare = !self.compare;
                self.boxes().save();
            }
            Hit::Resize | Hit::Window | Hit::Expand => {}
        }
        None
    }

    pub fn height(&self) -> f32 {
        if self.is_cart { TITLE_HEIGHT + self.body_height() + FOOTER_HEIGHT } else { self.inventory_height }
    }

    fn width(&self) -> f32 {
        if self.is_cart { width(self.columns) } else { CELLS_AT[0] + self.columns as f32 * CELL + CELLS_RIGHT }
    }

    /// The first cell's corner.
    fn grid(&self) -> [f32; 2] {
        if self.is_cart { GRID } else { CELLS_AT }
    }

    /// Rows of cells plus 8px above and 10px below them; the tab strip is exactly this tall.
    fn body_height(&self) -> f32 {
        self.rows as f32 * CELL + 18.0
    }

    /// Its width and height on screen.
    pub fn window_size(&self) -> [f32; 2] {
        [self.width(), self.height()]
    }

    /// Its columns and rows of cells.
    pub fn size(&self) -> [usize; 2] {
        [self.columns, self.rows]
    }

    /// Takes `columns` and `rows` as far as the grip would go; the items reflow.
    pub fn set_size(&mut self, [columns, rows]: [usize; 2]) {
        if !self.is_cart {
            return self.resize_inventory([CELLS_AT[0] + columns as f32 * CELL + CELLS_RIGHT, CHROME_HEIGHT + rows as f32 * CELL]);
        }
        self.columns = columns.clamp(*COLUMN_LIMITS.start(), *COLUMN_LIMITS.end());
        self.rows = rows.clamp(*self.row_limits.start(), *self.row_limits.end());
        self.frame.set_width(self.width());
        self.scroll_by(0);
    }

    /// Held by the corner grip at `cursor`.
    pub fn start_resize(&mut self, cursor: [f32; 2]) {
        self.resizing = Some((cursor, self.size(), [self.width(), self.height()]));
    }

    /// While held by its grip, the corner follows the pointer from the press in whole cells (the window stays
    /// opaque); returns whether it's held.
    pub fn resize_to(&mut self, cursor: [f32; 2]) -> bool {
        let Some((from, [columns, rows], [width, height])) = self.resizing else { return false };
        // The inventory: the size asked for by the pointer, snapped (see `snap_width` and `snap_height`).
        if !self.is_cart {
            self.resize_inventory([width + cursor[0] - from[0], height + cursor[1] - from[1]]);
            return true;
        }
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

    /// The saved boxes: compare. The drop lock starts off every time.
    pub fn apply_boxes(&mut self, boxes: Boxes) {
        self.compare = boxes.compare;
    }

    /// The four tabs on screen (for the test driver): `tab:item`, `tab:equip`, `tab:etc`, `tab:favorite`.
    pub fn tab_boxes(&self) -> Vec<(&'static str, [f32; 4])> {
        if !self.frame.open || self.frame.minimized {
            return Vec::new();
        }
        if !self.is_cart {
            return ["tab:item", "tab:equip", "tab:etc", "tab:favorite"]
                .into_iter()
                .enumerate()
                .map(|(tab, name)| {
                    let [x, y] = self.frame.at(self.tab_corner(tab));
                    (name, [x, y, STRIP_WIDTH, self.tab_height])
                })
                .collect();
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
        if !self.frame.open || self.frame.minimized || self.is_cart {
            return Vec::new();
        }
        self.footer_controls()
            .into_iter()
            .map(|([x, y, width, height], hit)| {
                let name = match hit {
                    Hit::Expand => "expand",
                    Hit::LockDrop => "lock_drop",
                    _ => "compare",
                };
                let [x, y] = self.frame.at([x, y]);
                (name, [x, y, width, height])
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
                let grid = self.grid();
                let corner = self.frame.at([grid[0] + (cell % self.columns) as f32 * CELL, grid[1] + (cell / self.columns) as f32 * CELL]);
                (item.index, [corner[0], corner[1], CELL, CELL])
            })
            .collect()
    }

    pub fn scroll_by(&mut self, rows: i32) {
        let total_rows = self.listed().count().div_ceil(self.columns);
        self.scroll = (self.scroll as i32 + rows).clamp(0, total_rows.saturating_sub(self.rows) as i32) as usize;
    }

    /// The wheel: the inventory moves min(3, rows − 1) rows a notch, the cart one.
    pub fn wheel(&mut self, notches: i32) {
        let step = if self.is_cart { 1 } else { WHEEL_ROWS.min(self.rows.saturating_sub(1)).max(1) };
        self.scroll_by(notches * step as i32);
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
        (cards, makers, option_texts): Names,
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

        if self.is_cart {
            // Body: the striped tab column with its strip, the white item area between borders, and the cells.
            layer.sprites.push((&assets.footer[1], at([0.0, TITLE_HEIGHT]), [TAB_COLUMN, self.body_height()]));
            layer.sprites.push((&chrome.white, at([0.0, TITLE_HEIGHT]), [TABS_AT[0], self.body_height()]));
            layer.sprites.push((&assets.tabs[self.tab], at(TABS_AT), assets.tabs[self.tab].size));
            layer.sprites.push((&chrome.white, at([TAB_COLUMN, TITLE_HEIGHT]), [self.width() - TAB_COLUMN, self.body_height()]));
            for x in [TAB_COLUMN, self.width() - 1.0] {
                layer.sprites.push((&chrome.border, at([x, TITLE_HEIGHT]), [1.0, self.body_height()]));
            }
        } else {
            self.draw_inventory_body(layer, chrome, assets, messages, font);
        }
        let grid = self.grid();
        for cell in 0..self.rows * self.columns {
            let place = [grid[0] + (cell % self.columns) as f32 * CELL, grid[1] + (cell / self.columns) as f32 * CELL];
            layer.sprites.push((&assets.cell, at(place), assets.cell.size));
        }

        // Items. The inventory's cells: the new mark, the icon, the amount on everything (outlined), the enchant
        // grade, the Swap Equipment mark; the cart's: the icon, how many unless it's equipment. The hovered one's name
        // in a box above the cell (the cart's beside it).
        for (cell, item) in self.listed().skip(self.scroll * self.columns).take(self.rows * self.columns).enumerate() {
            let place = at([grid[0] + (cell % self.columns) as f32 * CELL, grid[1] + (cell / self.columns) as f32 * CELL]);
            let info = items.get(&item.id);
            if !self.is_cart && self.new_marks.contains(&item.index) {
                layer.sprites.push((&assets.new_mark, place, assets.new_mark.size));
            }
            if let Some(Some(icon)) = info.and_then(|info| icons.get(&icon_path(info, item.identified))) {
                layer.sprites.push((icon, offset(place, [ICON_OFFSET, ICON_OFFSET]), icon.size));
            }
            let amount = item.amount.to_string();
            if self.is_cart {
                // ponytail: the mark's corner isn't knowable.
                if item.switch != 0 {
                    layer.sprites.push((&assets.switch, place, assets.switch.size));
                }
                if tab_of(item.kind) != 1 {
                    let corner = offset(place, [CELL - 1.0 - font.width(&amount), 15.0]);
                    layer.texts.push((amount.clone(), offset(corner, [-1.0, -1.0]), [255; 3]));
                    layer.texts.push((amount, corner, [0, 0, 0]));
                }
            } else {
                let corner = offset(place, amount_at(item.amount));
                for step in [[-1.0, 0.0], [1.0, 0.0], [0.0, -1.0], [0.0, 1.0]] {
                    layer.texts.push((amount.clone(), offset(corner, step), [255; 3]));
                }
                layer.texts.push((amount, corner, [0, 0, 0]));
                if let Some(grade) = assets.grades.get(usize::from(item.grade).wrapping_sub(1)) {
                    layer.sprites.push((grade, offset(place, [4.0, 16.0]), grade.size));
                }
                if item.switch != 0 {
                    let corner = offset(place, [ICON_OFFSET, ICON_OFFSET]);
                    layer.sprites.push((&assets.switch_back, corner, assets.switch_back.size));
                    layer.sprites.push((&assets.switch, corner, assets.switch.size));
                }
            }
            if contains(place, [CELL, CELL], cursor) {
                let name = full_name(items, cards, messages, makers, item, true);
                // The cart counts as "name n ea" (588) where the inventory says "name: n ea." (183); both "[N Option]" (2551).
                let text = if self.is_cart && !(item.identified && !item.options.is_empty()) {
                    format!("{name} {} {}", item.amount, messages.get(CART_EA).map_or("", String::as_str))
                } else {
                    hover_text(messages, &name, item)
                };
                let corner = offset(place, [0.0, -NAME_BOX_ABOVE]);
                // The Extras' item options (`option_texts` given): a row each under the name, the box growing upward.
                let options: Vec<String> = option_texts
                    .filter(|_| item.identified)
                    .map_or_else(Vec::new, |texts| item.options.iter().filter_map(|option| crate::iteminfo::option_text(texts, option)).collect());
                if options.is_empty() {
                    window::tooltip(tooltip, chrome, font, &text, corner, name_color(item, items));
                } else {
                    let mut lines = vec![vec![(name_color(item, items), text)]];
                    lines.extend(options.into_iter().map(|option| vec![([255; 3], option)]));
                    let corner = offset(corner, [0.0, -((lines.len() - 1) as f32 * font.line_height())]);
                    window::tooltip_lines(tooltip, chrome, font, &lines, corner);
                }
            }
        }

        if !self.is_cart {
            return self.draw_inventory_footer(layer, tooltip, chrome, assets, messages, font, cursor);
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
    }

    /// The inventory under its title bar: the side pieces every 8 px, the tab strip, the line beside it, white beside
    /// and under the cells (which the caller draws).
    fn draw_inventory_body<'a>(&self, layer: &mut Layer<'a>, chrome: &'a Chrome, assets: &'a Assets, messages: &[String], font: &Font) {
        let at = |local: [f32; 2]| self.frame.at(local);
        let [width, height] = [self.width(), self.height()];
        let body = height - CHROME_HEIGHT;
        let [left, right] = &assets.side;
        for step in 0..(body / 8.0).ceil() as usize {
            let y = TITLE_HEIGHT + step as f32 * 8.0;
            if height > INVENTORY_SIZE[1] {
                layer.sprites.push((left, at([0.0, y]), left.size));
            }
            layer.sprites.push((right, at([width - right.size[0], y]), right.size));
        }
        // The tabs, top to bottom: white with a grey border, all but the open one closed on the cells' side; the label
        // one character under the other.
        // ponytail: each label centred across its tab, from 5 px down.
        for (tab, &label) in TAB_LABELS.iter().enumerate() {
            let [x, y] = self.tab_corner(tab);
            layer.sprites.push((&assets.tab_border, at([x, y]), [STRIP_WIDTH, self.tab_height]));
            let inner_right = if tab == self.tab { STRIP_WIDTH - 1.0 } else { STRIP_WIDTH - 2.0 };
            layer.sprites.push((&chrome.white, at([x + 1.0, y + 1.0]), [inner_right, self.tab_height - 2.0]));
            let label = messages.get(label).map_or("", String::as_str);
            for (row, character) in label.chars().enumerate() {
                let text = character.to_string();
                let left = x + ((STRIP_WIDTH - font.width(&text)) / 2.0).floor();
                layer.texts.push((text, at([left, y + TAB_PADDING / 2.0 + row as f32 * font.line_height()]), [0, 0, 0]));
            }
        }
        layer.sprites.push((&assets.strip_line, at([STRIP_LINE_X, TITLE_HEIGHT]), [1.0, body]));
        layer.sprites.push((&chrome.white, at([STRIP_LINE_X, TITLE_HEIGHT]), [CELLS_AT[0] - STRIP_LINE_X, body]));
        let below = TITLE_HEIGHT + self.rows as f32 * CELL;
        layer.sprites.push((&chrome.white, at([STRIP_LINE_X, below]), [width - 24.0, height - INVENTORY_FOOTER - below]));
    }

    /// The inventory's footer: the plain bar, the expand icon, the count (red when full), the drop lock and compare,
    /// each with its tooltip.
    #[allow(clippy::too_many_arguments)]
    fn draw_inventory_footer<'a>(
        &self,
        layer: &mut Layer<'a>,
        tooltip: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        messages: &[String],
        font: &Font,
        cursor: [f32; 2],
    ) {
        let at = |local: [f32; 2]| self.frame.at(local);
        let [width, height] = [self.width(), self.height()];
        let top = height - INVENTORY_FOOTER;
        let [left, middle, right] = &assets.plain_footer;
        layer.sprites.push((left, at([0.0, top]), left.size));
        layer.sprites.push((middle, at([left.size[0], top]), [width - left.size[0] - right.size[0], INVENTORY_FOOTER]));
        layer.sprites.push((right, at([width - right.size[0], top]), right.size));
        let [expand, lock, compare] = self.footer_controls().map(|(area, _)| area);
        layer.sprites.push((&assets.count, at([expand[0], expand[1]]), assets.count.size));
        layer.sprites.push((&assets.lock[usize::from(self.lock_drop)], at([lock[0], lock[1]]), [lock[2], lock[3]]));
        layer.sprites.push((&assets.compare[usize::from(self.compare)], at([compare[0], compare[1]]), [compare[2], compare[3]]));
        let [carried, most] = [self.items.len(), BASE_SLOTS + usize::from(self.expansion)];
        let count = at([COUNT_AT[0], height - COUNT_AT[1]]);
        let carried_text = carried.to_string();
        layer.texts.push((carried_text.clone(), count, count_color(carried, most)));
        layer.texts.push((format!("/{most}"), offset(count, [font.width(&carried_text), 0.0]), [0, 0, 0]));
        layer.sprites.push((&assets.resize, at([width - GRIP, height - GRIP]), assets.resize.size));
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        for (area, tip) in [(expand, EXPAND_TIP), (lock, LOCK_DROP), (compare, COMPARE)] {
            let corner = at([area[0], area[1]]);
            if contains(corner, [area[2], area[3]], cursor) {
                window::tooltip(tooltip, chrome, font, message(tip), offset(corner, [area[2] + 4.0, 0.0]), [255; 3]);
            }
        }
    }
}

/// The count's colour: red once what's carried reaches the limit.
fn count_color(carried: usize, most: usize) -> [u8; 3] {
    if carried >= most { FULL_COLOR } else { COUNT_COLOR }
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
    fn snaps_the_inventory_to_the_games_sizes() {
        assert_eq!([240.0, 290.0, 330.0].map(snap_width), [248.0, 280.0, 312.0]);
        assert_eq!([100.0, 170.0, 300.0].map(|height| snap_height(height, 82.0)), [120.0, 152.0, 216.0]);
        assert_eq!([100.0, 255.0, 400.0].map(|height| snap_height(height, 217.0)), [255.0; 3]);
        assert_eq!([cells_of([248.0, 120.0]), cells_of([280.0, 152.0]), cells_of([312.0, 216.0]), cells_of([280.0, 255.0])], [[6, 2], [7, 3], [8, 5], [7, 6]]);
        // A first start: at (0, 120), 280 wide, the cells right under the title bar.
        let inventory = Inventory::new(true);
        assert_eq!((inventory.frame.position, inventory.width(), inventory.grid()), ([0.0, 120.0], 280.0, [40.0, 17.0]));
        assert_eq!(inventory.height(), 255.0, "three-character tabs: one height");
    }

    #[test]
    fn resizes_by_its_corner_grip() {
        let mut inventory = Inventory::new(true);
        inventory.tab_height = 21.25;
        inventory.set_size([7, 2]);
        assert_eq!((inventory.width(), inventory.height()), (280.0, 120.0));
        let corner = inventory.frame.at([inventory.width() - 5.0, inventory.height() - 5.0]);
        assert_eq!(inventory.hit(corner), Some(Hit::Resize));
        // The corner follows the pointer from the press, then snaps: 40 right and 70 down is 312 by 184.
        inventory.start_resize(corner);
        assert!(inventory.resize_to([corner[0] + 40.0, corner[1] + 70.0]));
        assert_eq!((inventory.size(), inventory.width(), inventory.height()), ([8, 4], 312.0, 184.0));
        inventory.resize_to([corner[0] - 300.0, corner[1] - 500.0]);
        assert_eq!(inventory.size(), [6, 2]);
        assert!(inventory.stop_resize() && !inventory.resize_to(corner));
        // The cart keeps its whole-cell steps and 2 to 5 rows; items reflow, and the scroll stays in range.
        let mut cart = Inventory::cart();
        cart.items = (0..20).map(|index| InventoryItem { index, id: 501, kind: net::ITEM_HEALING, amount: 1, ..Default::default() }).collect();
        cart.set_size([6, 2]);
        cart.scroll_by(10);
        assert_eq!(cart.scroll, 2, "20 items in 6 columns: 4 rows, 2 shown");
        cart.set_size([8, 5]);
        assert_eq!(cart.scroll, 0);
        assert_eq!(cart.width(), width(8));
    }

    #[test]
    fn hits_cells_from_40_to_its_width_less_20() {
        let mut inventory = Inventory::new(true);
        inventory.extend((0..60).map(|index| item(index, net::ITEM_HEALING, 1)).collect());
        let width = inventory.width();
        let at = |x: f32| inventory.hit(inventory.frame.at([x, 20.0]));
        assert!(matches!(at(40.0), Some(Hit::Item(_))) && matches!(at(width - 21.0), Some(Hit::Item(_))));
        assert_eq!((at(39.0), at(width - 20.0)), (Some(Hit::Window), Some(Hit::Window)));
        // The tabs stack in the strip; the footer's icons.
        assert_eq!(inventory.hit(inventory.frame.at([10.0, 18.0 + 54.0 + 5.0])), Some(Hit::Tab(1)));
        let bottom = inventory.height();
        assert_eq!(inventory.hit(inventory.frame.at([25.0, bottom - 10.0])), Some(Hit::Expand));
        assert_eq!(inventory.hit(inventory.frame.at([82.0, bottom - 10.0])), Some(Hit::LockDrop));
        assert_eq!(inventory.hit(inventory.frame.at([106.0, bottom - 10.0])), Some(Hit::Compare));
    }

    #[test]
    fn keeps_the_scroll_row_across_tabs() {
        let mut inventory = Inventory::new(true);
        // 60 potions (9 rows of 7) and 30 etc items (5 rows).
        inventory.extend((0..60).map(|index| item(index, net::ITEM_HEALING, 1)).chain((60..90).map(|index| item(index, 3, 1))).collect());
        inventory.click(Hit::Tab(2));
        inventory.wheel(1);
        assert_eq!(inventory.scroll, 0, "5 rows of etc fit in 6");
        inventory.click(Hit::Tab(0));
        inventory.wheel(1);
        assert_eq!(inventory.scroll, 3, "the wheel moves 3 rows");
        inventory.click(Hit::Tab(2));
        assert_eq!(inventory.scroll, 0, "kept, as far as etc's rows go");
        inventory.click(Hit::Tab(0));
        inventory.scroll_by(2);
        inventory.click(Hit::Tab(1));
        inventory.click(Hit::Tab(0));
        assert_eq!(inventory.scroll, 0, "the empty equipment tab took it back to the top");
        // Two rows showing: the wheel moves one.
        inventory.tab_height = 21.25;
        inventory.set_size([7, 2]);
        inventory.wheel(1);
        assert_eq!(inventory.scroll, 1);
        // The count turns red at the limit.
        assert_eq!((count_color(99, 100), count_color(100, 100)), (COUNT_COLOR, FULL_COLOR));
    }

    #[test]
    fn names_items_in_full() {
        let info = |name: &str, unidentified: &str, slots| ItemInfo { name: name.into(), unidentified_name: unidentified.into(), slots, ..Default::default() };
        let items: HashMap<u32, ItemInfo> = [
            (1201, info("Knife", "Dagger", 3)),
            (501, info("Red Potion", "Red Potion", 0)),
            (2634, info("Wedding Ring", "Ring", 0)),
            (9001, info("Poring Egg", "Poring Egg", 0)),
        ]
        .into_iter()
        .collect();
        let cards = CardNames::parse("4001#Lucky#\n4014#of Hermes#\n4002#Fabre#\n4003##\n", "4014#\n");
        let mut messages = vec![String::new(); 3000];
        for (id, text) in [
            (446, "Double"),
            (447, "Triple"),
            (448, "Quadruple"),
            (450, "'s "),
            (451, "'s Fire "),
            (452, "'s Water "),
            (459, "Very Strong "),
            (460, "Very Very Strong "),
            (581, "Nameless"),
            (756, "Beloved "),
            (2976, " with "),
        ] {
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
        // A card in several slots: "Double " before a word that goes first, " Double" after one that goes last.
        assert_eq!(name(&knife(0, 0, [4001, 4001, 0, 0]), false), "Double Lucky Knife");
        assert_eq!(name(&knife(0, 0, [4001, 4001, 4001, 4001]), false), "Quadruple Lucky Knife");
        assert_eq!(name(&knife(0, 0, [4014, 4014, 4014, 0]), false), "Knife of Hermes Triple");
        // Different cards in slot order, each once; one with no word is skipped.
        assert_eq!(name(&knife(0, 0, [4014, 4002, 4001, 4002]), true), "Double Fabre Lucky Knife of Hermes [3]");
        assert_eq!(name(&knife(0, 0, [4003, 4003, 9999, 0]), false), "Knife");
        // Forged: Star Crumbs, the maker, the element (fire 3; water 1), and no slots.
        let forged = |element: u32, crumbs: u32, maker: u32| knife(0, 0, [0x00FF, (crumbs * 5) << 8 | element, maker & 0xFFFF, maker >> 16]);
        assert_eq!(name(&forged(3, 1, 150_004), true), "Very Strong LaneA1's Fire Knife");
        assert_eq!(name(&forged(1, 2, 150_004), false), "Very Very Strong LaneA1's Water Knife");
        assert_eq!(name(&forged(0, 0, 999), false), "Nameless's Knife");
        assert_eq!(name(&InventoryItem { refine: 5, ..forged(5, 0, 150_004) }, false), "+5 LaneA1Knife");
        let (_, run) = full_name_maker(&items, &cards, &messages, &makers, &forged(0, 0, 999), false);
        assert_eq!(run, Some((0..8, MAKER_COLOR)), "Nameless in the maker's colour");
        assert_eq!(maker(&forged(0, 0, 150_004).cards), Some(150_004));
        // A wedding ring has its own words after the maker.
        let ring = InventoryItem { id: 2634, kind: net::ITEM_ARMOR, ..forged(0, 0, 150_004) };
        assert_eq!(name(&ring, false), "LaneA1 with Wedding Ring");
        // Brewed: the maker's name on a potion too.
        let potion = InventoryItem { id: 501, amount: 3, identified: true, kind: net::ITEM_HEALING, ..Default::default() };
        let brewed = InventoryItem { cards: [0x00FE, 0, 150_004 & 0xFFFF, 150_004 >> 16], ..potion.clone() };
        assert_eq!(name(&brewed, true), "LaneA1's Red Potion");
        // Unidentified: its own name only; not equipment: the name.
        assert_eq!(name(&InventoryItem { identified: false, ..knife(7, 0, [4001, 0, 0, 0]) }, true), "Dagger");
        assert_eq!(name(&potion, true), "Red Potion");
        // A pet's egg: its cards hold the pet's number, not cards; renamed, it's "Beloved".
        let egg = |renamed| InventoryItem { id: 9001, identified: true, kind: net::ITEM_ARMOR, cards: [0x0100, 4001, 0, renamed], ..Default::default() };
        assert_eq!([name(&egg(0), true), name(&egg(1), true)], ["Poring Egg", "Beloved Poring Egg"]);
        // From the ID alone: unidentified shows no refine or slots.
        assert_eq!([item_name(&items, 1201, true, 4), item_name(&items, 1201, false, 4)], ["+4 Knife [3]", "Dagger"]);
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
        // A broken item has no colour of its own; a bound one is yellow.
        let items = HashMap::new();
        assert_eq!([name_color(&knife, &items), name_color(&shirt, &items)], [[255; 3], BOUND]);
    }

    #[test]
    fn colours_names_unidentified_then_by_enchants_then_bound() {
        let info = |slots| ItemInfo { slots, ..Default::default() };
        let items: HashMap<u32, ItemInfo> = [(2301, info(1)), (4700, info(0)), (4001, info(0))].into_iter().collect();
        let shirt = |cards: [u32; 4]| InventoryItem { id: 2301, kind: net::ITEM_ARMOR, identified: true, cards, ..Default::default() };
        // One slot (a card there doesn't count); two enchants after it.
        assert_eq!(name_color(&shirt([4001, 0, 0, 0]), &items), [255; 3]);
        assert_eq!(name_color(&shirt([4001, 4700, 4700, 0]), &items), [0x00, 0xBF, 0xFF]);
        assert_eq!(name_color(&shirt([0, 4700, 0, 0]), &items), [0xAD, 0xFF, 0x2F]);
        // An unknown card isn't an enchant; unidentified beats enchants, enchants beat bound.
        assert_eq!(name_color(&shirt([0, 9999, 0, 0]), &items), [255; 3]);
        assert_eq!(name_color(&InventoryItem { identified: false, ..shirt([0, 4700, 4700, 4700]) }, &items), UNIDENTIFIED);
        assert_eq!(name_color(&InventoryItem { bind: 2, ..shirt([0, 4700, 4700, 4700]) }, &items), [0xFF, 0xA5, 0x00]);
        assert_eq!(name_color(&InventoryItem { bind: 2, ..shirt([0; 4]) }, &items), BOUND);
        // Forged pieces have none.
        assert_eq!(name_color(&shirt([0x00FF, 4700, 4700, 0]), &items), [255; 3]);
        // The amount's place.
        assert_eq!((amount_at(999), amount_at(1000)), ([18.0, 19.0], [4.0, 19.0]));
    }

    #[test]
    fn keeps_the_order_items_came_in() {
        let mut inventory = Inventory::new(true);
        // A login list as it came, not by index.
        inventory.extend(vec![item(5, net::ITEM_HEALING, 1), item(2, net::ITEM_HEALING, 1)]);
        // A new slot goes to the end; a stack stays where it is.
        inventory.add(item(1, net::ITEM_HEALING, 1));
        inventory.add(item(5, net::ITEM_HEALING, 3));
        let order = |inventory: &Inventory| inventory.items.iter().map(|item| item.index).collect::<Vec<_>>();
        assert_eq!(order(&inventory), [5, 2, 1]);
        assert_eq!(inventory.get(5).unwrap().amount, 4);
        // A weapon taken off goes to the end.
        inventory.extend(vec![InventoryItem { worn: 2, ..item(9, net::ITEM_WEAPON, 1) }, item(7, net::ITEM_HEALING, 1)]);
        inventory.set_worn(9, 0);
        assert_eq!(order(&inventory), [5, 2, 1, 7, 9]);
        // The cart keeps index order.
        let mut cart = Inventory::cart();
        cart.extend(vec![item(5, net::ITEM_HEALING, 1), item(2, net::ITEM_HEALING, 1)]);
        assert_eq!(order(&cart), [2, 5]);
    }

    #[test]
    fn clears_new_marks_when_closed_on_a_listed_tab() {
        let mut inventory = Inventory::new(true);
        inventory.extend(vec![item(1, net::ITEM_HEALING, 1)]);
        inventory.mark_new(1);
        inventory.note_open();
        // Closed on the empty Eqp tab: the marks stay.
        inventory.click(Hit::Tab(1));
        inventory.frame.open = false;
        inventory.note_open();
        assert!(inventory.new_marks.contains(&1));
        // Open again on Use, then closed: gone.
        inventory.frame.open = true;
        inventory.note_open();
        inventory.click(Hit::Tab(0));
        inventory.frame.open = false;
        inventory.note_open();
        assert!(inventory.new_marks.is_empty());
    }

    #[test]
    fn identifies_with_ctrl_only_on_the_eqp_tab() {
        // No Magnifier: the line on any tab; an unidentified piece on Eqp is still asked for.
        assert_eq!(ctrl_identify(0, false, false), (true, false));
        assert_eq!(ctrl_identify(1, false, false), (true, true));
        assert_eq!(ctrl_identify(FAVORITES_TAB, false, true), (false, false));
        assert_eq!(ctrl_identify(1, false, true), (false, true));
        assert_eq!(ctrl_identify(1, true, true), (false, false));
    }

    #[test]
    fn keeps_stacks_and_tabs() {
        let mut inventory = Inventory::new(true);
        inventory.extend(vec![item(3, net::ITEM_HEALING, 5), item(2, net::ITEM_WEAPON, 1), item(4, 3, 9)]);
        assert_eq!(inventory.items.iter().map(|item| item.index).collect::<Vec<_>>(), [3, 2, 4], "as they came");
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
    fn keeps_compare_but_not_the_drop_lock() {
        let mut inventory = Inventory::new(true);
        inventory.apply_boxes(Boxes { lock_drop: true, compare: true });
        assert!(!inventory.lock_drop && inventory.compare);
    }

    #[test]
    fn the_drop_lock_stops_a_ground_drop() {
        assert!(may_ground_drop(false));
        assert!(!may_ground_drop(true));
        assert_eq!(Boxes::parse("1 0"), Boxes { lock_drop: true, compare: false });
        assert_eq!(Boxes::parse(""), Boxes { lock_drop: false, compare: true });
        assert_eq!(Boxes::parse("0 1").text(), "0 1");
        assert_eq!(Boxes::parse("").text(), "0 1");
    }

    #[test]
    fn hover_text_counts_an_identified_items_options() {
        let mut messages = vec![String::new(); 2552];
        messages[messages::ITEM_AMOUNT] = "%s: %d ea.".into();
        messages[messages::ITEM_OPTIONS_AMOUNT] = "%s [%d Option] : %d Quantity".into();
        let option = rustnarok_client::net::ItemOption { id: 1, value: 1, param: 0 };
        let item = InventoryItem { identified: true, amount: 1, options: vec![option, option], ..InventoryItem::default() };
        assert_eq!(hover_text(&messages, "Sword", &item), "Sword [2 Option] : 1 Quantity");
        assert_eq!(hover_text(&messages, "Sword", &InventoryItem { identified: false, ..item }), "Sword: 1 ea.");
        assert_eq!(hover_text(&messages, "Apple", &InventoryItem { identified: true, amount: 3, ..InventoryItem::default() }), "Apple: 3 ea.");
    }
}
