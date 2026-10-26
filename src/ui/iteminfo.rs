//! The item description window (right-click an item): the item's picture, its name, what the client says about it,
//! and for equipment its random options in boxed rows and its cards and enchants in a strip along the bottom.

use std::collections::HashMap;

use rustnarok_client::grf::Grf;
use rustnarok_client::lua::ItemInfo;
use rustnarok_client::net::{self, InventoryItem, ItemOption};
use rustnarok_client::text::Font;

use crate::inventory::{self, CardNames};
use crate::render::{Gpu, Texture};
use crate::window::Chrome;
use crate::{Layer, UI, contains, offset, pickup};

const WIDTH: f32 = 280.0;
const PICTURE: [f32; 2] = [10.0, 11.0];
/// The title: one line cut to 180 px.
const TITLE: [f32; 2] = [90.0, 10.0];
const TITLE_WIDTH: f32 = 180.0;
/// The text, in 18 px lines; the area is the height less 40 (less 69 with the card strip).
const TEXT: [f32; 2] = [91.0, 31.0];
const LINE: f32 = 18.0;
const TEXT_LESS: f32 = 40.0;
/// Characters a description line holds before it's cut (a Korean character counts two), and a rental's line.
const LINE_COLUMNS: usize = 26;
const RENTAL_COLUMNS: usize = 32;
/// The height: 120 up to four lines, 18 more a line, at most 450. The card strip is 29 more, and at least 175.
const BASE_HEIGHT: f32 = 120.0;
const BASE_LINES: usize = 4;
const STRIP: f32 = 29.0;
const LEAST_WITH_STRIP: f32 = 175.0;
const MOST_HEIGHT: f32 = 450.0;
/// The wheel scrolls this many lines a notch.
const WHEEL_LINES: i32 = 3;
const CLOSE: [f32; 2] = [WIDTH - 14.0, 3.0];
/// Dragged by the top band: above y 17, right of x 60.
const DRAG_BAND: [f32; 2] = [60.0, 17.0];
/// The height grip, 17 in from the right and up from the bottom (above the strip when there is one).
const GRIP_IN: f32 = 17.0;
const GRIP_SIZE: f32 = 16.0;
/// The strip's four cells: 24 square every 25 px from x 4, 27 up from the bottom; a card's name 50 up.
const CELL: f32 = 24.0;
const CELL_STEP: f32 = 25.0;
const CELL_X: f32 = 4.0;
const CELL_UP: f32 = 27.0;
const CELL_NAME_UP: f32 = 50.0;
const CELLS: usize = 4;
/// A random option's row, between the text and the strip: a box the window's width, its text 6 in and 4 down.
// ponytail: row height and text place by eye from a 2x shot; a measured shot would replace them.
const ROW: f32 = 20.0;
const ROW_TEXT: [f32; 2] = [6.0, 4.0];
/// Where it first opens.
pub const DEFAULT_PLACE: [f32; 2] = [280.0, 126.0];
/// A card's window opens here the first time.
pub const CARD_PLACE: [f32; 2] = [280.0, 278.0];
/// The picture button, on a card's window (not an enchant's).
const VIEW: [f32; 2] = [6.0, 6.0];
const VIEW_SIZE: [f32; 2] = [42.0, 20.0];
/// The title's colours: an item with one enchant, a broken item's shadow, the maker's shadow, an unidentified broken
/// item.
const ONE_ENCHANT: [u8; 3] = [0x66, 0x99, 0x00];
const BROKEN_SHADOW: [u8; 3] = [0xFA, 0x50, 0x50];
const MAKER_SHADOW: [u8; 3] = [0xE0, 0xE0, 0xE0];
const UNIDENTIFIED_BROKEN: [u8; 3] = [0x80, 0x80, 0x80];

pub struct Assets {
    /// The background and the sys box under it: plain, then for enchant grades 1 to 4.
    backgrounds: [Texture; 5],
    boxes: [pickup::Assets; 5],
    close: [Texture; 2],
    resize: Texture,
    /// A card cell with nothing in it: an open slot, and past the slots.
    empty_slot: Texture,
    disabled_slot: Texture,
    /// The compare window's own: background, sys box, grip and empty cells.
    compare_background: Texture,
    compare_box: pickup::Assets,
    compare_resize: Texture,
    compare_empty_slot: Texture,
    compare_disabled_slot: Texture,
    /// The picture button: normal, pointed at, pressed.
    view: [Texture; 3],
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |name: &str| gpu.load_texture(grf, &format!("{UI}basic_interface\\{name}.bmp"), false);
        Self {
            backgrounds: std::array::from_fn(|grade| image(&if grade == 0 { "collection_bg".to_owned() } else { format!("collection_bg_g{grade}") })),
            boxes: std::array::from_fn(|grade| pickup::Assets::named(gpu, grf, &if grade == 0 { "sysbox".to_owned() } else { format!("sysbox_g{grade}") })),
            close: [image("sys_close_off"), image("sys_close_on")],
            resize: gpu.load_texture(grf, &format!("{UI}btn_resize.bmp"), false),
            empty_slot: gpu.load_texture(grf, &format!("{UI}empty_card_slot.bmp"), false),
            disabled_slot: gpu.load_texture(grf, &format!("{UI}disable_card_slot.bmp"), false),
            compare_background: image("collection_comparison_bg"),
            compare_box: pickup::Assets::with_middle(gpu, grf, "basic_interface\\sysbox_comparison"),
            compare_resize: image("btn_comparison_resize"),
            compare_empty_slot: image("comparison_empty_card_slot"),
            compare_disabled_slot: image("comparison_disable_card_slot"),
            view: ["", "_a", "_b"].map(|look| gpu.load_texture(grf, &format!("{UI}btn_view{look}.bmp"), false)),
        }
    }
}

/// Lines of text, each a run of colored pieces.
pub type ColoredLines = Vec<Vec<([u8; 3], String)>>;

/// The item shown.
#[derive(Debug, Clone, PartialEq)]
pub struct Shown {
    pub item: u32,
    pub identified: bool,
    pub refine: u8,
    pub cards: [u32; 4],
    /// Its type (rathena's item types), enchant grade and whether it's broken, for its title and card strip.
    pub kind: u8,
    pub grade: u8,
    pub broken: bool,
    /// A rental's end, Unix time in seconds (0: not a rental).
    pub expires: u32,
    /// A card: its info offers the picture button.
    pub card: bool,
    /// Where it goes (0: nothing to compare with), and whether it's worn itself (then it isn't compared).
    pub location: u32,
    pub worn: bool,
    /// Its random options.
    pub options: Vec<ItemOption>,
}

impl Shown {
    pub fn of(item: &InventoryItem) -> Self {
        Self {
            item: item.id,
            identified: item.identified,
            refine: item.refine,
            cards: item.cards,
            kind: item.kind,
            grade: item.grade,
            broken: item.broken,
            expires: item.expires,
            card: item.kind == net::ITEM_CARD,
            location: item.location,
            worn: item.worn != 0,
            options: item.options.clone(),
        }
    }

    /// A card from a description's strip: as an identified card, or as an enchant (past the item's slots), which has no
    /// picture button.
    pub fn card(id: u32, enchant: bool) -> Self {
        Self {
            item: id,
            identified: true,
            refine: 0,
            cards: [0; 4],
            kind: net::ITEM_CARD,
            grade: 0,
            broken: false,
            expires: 0,
            card: !enchant,
            location: 0,
            worn: false,
            options: Vec::new(),
        }
    }

    /// An item from a link: identified, with what the link carries.
    // ponytail: a link doesn't say the item's type; one with slots is taken for equipment.
    pub fn of_link(id: u32, refine: u8, cards: [u32; 4], card: bool, items: &HashMap<u32, ItemInfo>) -> Self {
        let kind = if items.get(&id).is_some_and(|info| info.slots > 0) { net::ITEM_ARMOR } else { 0 };
        // ponytail: nor where it's worn, so a link's item isn't compared.
        Self { item: id, identified: true, refine, cards, kind, grade: 0, broken: false, expires: 0, card, location: 0, worn: false, options: Vec::new() }
    }

    /// A link's item with its random options (id, parameter, value).
    pub fn with_link_options(self, options: &[[u32; 3]]) -> Self {
        let options = options.iter().map(|&[id, param, value]| ItemOption { id: id as u16, value: value as i16, param: param as u8 }).collect();
        Self { options, ..self }
    }

    /// The same item to describe: ID, refine, cards and options (not which stack, how many, or where it was clicked).
    pub fn same_item(&self, other: &Shown) -> bool {
        (self.item, self.refine, self.cards, &self.options) == (other.item, other.refine, other.cards, &other.options)
    }

    /// Its random options' rows: an identified item's options that have a text.
    pub fn option_lines(&self, texts: &HashMap<u16, String>) -> Vec<String> {
        if !self.identified {
            return Vec::new();
        }
        self.options.iter().filter_map(|option| option_text(texts, option)).collect()
    }

    /// As an inventory item, for the name and its colours.
    fn as_item(&self) -> InventoryItem {
        InventoryItem {
            id: self.item,
            identified: self.identified,
            refine: self.refine,
            cards: self.cards,
            kind: self.kind,
            grade: self.grade,
            broken: self.broken,
            amount: 1,
            ..InventoryItem::default()
        }
    }

    /// Identified equipment that isn't a pet egg or accessory: its cards show in a strip along the bottom.
    fn has_strip(&self) -> bool {
        self.identified && matches!(self.kind, net::ITEM_WEAPON | net::ITEM_ARMOR | net::ITEM_SHADOW_GEAR)
    }

    /// Forged or brewed: the strip keeps its place but shows nothing.
    fn forged(&self) -> bool {
        inventory::maker(&self.cards).is_some()
    }
}

/// A random option's text: its template (`NameTable_VAR`) with the value for `%d`; None for an id with no text.
pub fn option_text(texts: &HashMap<u16, String>, option: &ItemOption) -> Option<String> {
    texts.get(&option.id).map(|template| rustnarok_client::messages::fill(template, &[&option.value]))
}

/// The description and the compare window beside it: both take the longer option list and the taller height, so
/// their rows sit level. Only two with card strips: an unidentified item's description has neither rows nor a strip.
/// Call it after both windows' `set_rows`, which clears what it sets.
pub fn pair(description: &mut ItemWindow, compare: &mut ItemWindow) {
    if !(description.shown.has_strip() && compare.shown.has_strip()) {
        return;
    }
    let rows = description.rows.max(compare.rows);
    description.rows = rows;
    compare.rows = rows;
    let floor = description.total().max(compare.total());
    description.floor = floor;
    compare.floor = floor;
}

/// The worn piece a description compares with: the first worn where the item goes (ammo by the ammo slot), unless
/// the item is worn itself.
// ponytail: a piece that fits two slots (accessories) compares with the first found.
pub fn compared<'a>(shown: &Shown, items: &'a [InventoryItem]) -> Option<&'a InventoryItem> {
    items.iter().find(|item| !shown.worn && shown.location != 0 && item.worn & shown.location != 0)
}

/// The place a card's window comes back to: as saved, or its first place when that's off this screen.
pub fn restored_card_place(saved: [f32; 2], screen: [f32; 2]) -> [f32; 2] {
    let on_screen = (0.0..screen[0]).contains(&saved[0]) && (0.0..screen[1]).contains(&saved[1]);
    if on_screen { saved } else { CARD_PLACE }
}

/// The compare window's place: flush left of the description's.
pub fn compare_place(description: [f32; 2]) -> [f32; 2] {
    [description[0] - WIDTH, description[1]]
}

/// "Time Left: %d hours %d minutes".
const TIME_LEFT: usize = 2435;

/// A rental's time left, in red, from its end and the time now (Unix seconds); None for an item that isn't one.
// ponytail: the msgstringtable's end-date line (1255, local time) looks like the official one; std can't do local time,
// so this counts down instead, assuming the server's clock and ours agree.
pub fn rental_line(template: &str, expires: u32, now: u64) -> Option<String> {
    let left = u64::from(expires).saturating_sub(now);
    (expires > 0).then(|| format!("^ff0000{}^000000", rustnarok_client::messages::fill(template, &[&(left / 3600), &(left % 3600 / 60)])))
}

/// A description line cut every `columns` characters, not at word ends: ^RRGGBB codes take none, a Korean (non-ASCII)
/// character takes two and is never split, and "\r\n" breaks it too. `color` carries from line to line.
pub fn cut(line: &str, columns: usize, color: &mut [u8; 3]) -> ColoredLines {
    let mut lines: ColoredLines = vec![Vec::new()];
    let mut used = 0;
    let mut rest = line;
    while let Some(character) = rest.chars().next() {
        if let Some(code) =
            rest.strip_prefix('^').filter(|code| code.len() >= 6 && code.is_char_boundary(6) && code[..6].chars().all(|digit| digit.is_ascii_hexdigit()))
        {
            let value = u32::from_str_radix(&code[..6], 16).unwrap();
            *color = [(value >> 16) as u8, (value >> 8) as u8, value as u8];
            rest = &code[6..];
            continue;
        }
        rest = &rest[character.len_utf8()..];
        if character == '\r' {
            continue;
        }
        let width = if character.is_ascii() { 1 } else { 2 };
        if character == '\n' || used + width > columns {
            lines.push(Vec::new());
            used = 0;
            if character == '\n' {
                continue;
            }
        }
        used += width;
        let line = lines.last_mut().unwrap();
        match line.last_mut() {
            Some((last, piece)) if last == color => piece.push(character),
            _ => line.push((*color, character.to_string())),
        }
    }
    lines
}

/// The window's height for `lines` of text, with the card strip or not.
pub fn fitted_height(lines: usize, strip: bool) -> f32 {
    let extra = lines.saturating_sub(BASE_LINES) as f32 * LINE;
    if strip { (BASE_HEIGHT + STRIP + extra).clamp(LEAST_WITH_STRIP, MOST_HEIGHT) } else { (BASE_HEIGHT + extra).clamp(BASE_HEIGHT, MOST_HEIGHT) }
}

/// The place a saved one comes back to: as saved, or when it's off this screen, the game's spot for the size.
pub fn restored_place(saved: [f32; 2], screen: [f32; 2]) -> [f32; 2] {
    let on_screen = (0.0..screen[0]).contains(&saved[0]) && (0.0..screen[1]).contains(&saved[1]);
    if on_screen { saved } else { [((screen[0] - 640.0) / 2.0 + 280.0).floor(), (126.0 * screen[1] / 480.0).floor()] }
}

/// The title's pieces: its text, colour and shadow colour.
type TitlePieces = Vec<(String, [u8; 3], [u8; 3])>;

/// The title in pieces, each over its shadow: a broken item black (grey unidentified) on red; else black on white,
/// or for an item with enchants the name box's colour for them (#669900 for one); the maker's name in its colour on
/// light grey.
fn title_pieces(name: &str, maker: Option<inventory::MakerRun>, item: &InventoryItem, items: &HashMap<u32, ItemInfo>) -> TitlePieces {
    let (color, shadow) = if item.broken {
        (if item.identified { [0, 0, 0] } else { UNIDENTIFIED_BROKEN }, BROKEN_SHADOW)
    } else {
        match inventory::enchants(item, items) {
            0 => ([0, 0, 0], [255; 3]),
            1 => (ONE_ENCHANT, [255; 3]),
            count => (inventory::ENCHANTED[count.min(4) - 1], [255; 3]),
        }
    };
    match maker.filter(|(range, _)| range.end <= name.len()) {
        Some((range, maker_color)) => [
            (name[..range.start].to_owned(), color, shadow),
            (name[range.clone()].to_owned(), maker_color, MAKER_SHADOW),
            (name[range.end..].to_owned(), color, shadow),
        ]
        .into_iter()
        .filter(|(text, ..)| !text.is_empty())
        .collect(),
        None => vec![(name.to_owned(), color, shadow)],
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
enum Cell {
    /// A card or enchant, and the colour of its name.
    Card(u32, [u8; 3]),
    Open,
    Closed,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Close,
    /// The top band, to drag by.
    Grip,
    /// The height grip.
    Resize,
    /// The illustration button.
    View,
    /// Anywhere else, the card strip too (a click there does nothing).
    Window,
}

pub struct ItemWindow {
    pub shown: Shown,
    pub position: [f32; 2],
    /// Lines scrolled past.
    scroll: usize,
    /// Where the pointer grabbed the window, while dragging.
    drag: Option<[f32; 2]>,
    /// Held by the height grip: where the press was, and the height then.
    resizing: Option<(f32, f32)>,
    /// The text's and strip's height; the option rows come on top.
    height: f32,
    /// Option rows, and the least height, which the window beside it sets (`pair`).
    rows: usize,
    floor: f32,
    /// The height has been set from the text (each time an item is shown).
    fitted: bool,
    /// The compare window: its own look, and its close button does nothing.
    pub compare: bool,
}

/// The picture of an item (data\texture\유저인터페이스\collection\).
pub fn picture_path(info: &ItemInfo, identified: bool) -> String {
    format!("{UI}collection\\{}.bmp", info.resource(identified))
}

impl ItemWindow {
    pub fn new(shown: Shown) -> Self {
        Self { shown, position: DEFAULT_PLACE, scroll: 0, drag: None, resizing: None, height: BASE_HEIGHT, rows: 0, floor: 0.0, fitted: false, compare: false }
    }

    /// The card or enchant in the strip cell under the pointer, and whether it's an enchant (past the slots).
    pub fn card_under(&self, cursor: [f32; 2], items: &HashMap<u32, ItemInfo>) -> Option<(u32, bool)> {
        let cell = self.cell_at(offset(cursor, self.position.map(|value| -value)))?;
        self.card_in(cell, items).map(|card| (card, cell >= self.slots(items)))
    }

    /// The same item again, changed (appraised): its height is fitted to the new text.
    pub fn reshow(&mut self, shown: Shown) {
        self.shown = shown;
        self.scroll = 0;
        self.fitted = false;
    }

    /// Its text and option rows for this frame: fitted once, the rows each time.
    pub fn layout(&mut self, items: &HashMap<u32, ItemInfo>, messages: &[String], now: u64, texts: &HashMap<u16, String>) -> (ColoredLines, Vec<String>) {
        let lines = self.lines(items, messages, now);
        self.fit(lines.len());
        let options = self.shown.option_lines(texts);
        self.set_rows(options.len());
        (lines, options)
    }

    /// Sets the height from the text, once for the item shown.
    pub fn fit(&mut self, lines: usize) {
        if !self.fitted {
            self.height = fitted_height(lines, self.shown.has_strip());
            self.fitted = true;
        }
    }

    /// The option rows it shows, each frame; alone, nothing beside it sets its height.
    pub fn set_rows(&mut self, rows: usize) {
        self.rows = rows;
        self.floor = 0.0;
    }

    /// The whole height: text, option rows and strip.
    fn total(&self) -> f32 {
        (self.height + self.rows as f32 * ROW).max(self.floor)
    }

    /// The top of an option row, window-local.
    fn row_at(&self, row: usize) -> f32 {
        self.upper() + row as f32 * ROW
    }

    /// Where it is and how big, for the test driver.
    pub fn area(&self) -> [f32; 4] {
        [self.position[0], self.position[1], WIDTH, self.total()]
    }

    /// How many slots the item has.
    fn slots(&self, items: &HashMap<u32, ItemInfo>) -> usize {
        items.get(&self.shown.item).filter(|_| self.shown.identified).map_or(0, |info| usize::from(info.slots.min(4)))
    }

    /// How tall the window is above its option rows and strip.
    fn upper(&self) -> f32 {
        self.total() - self.rows as f32 * ROW - if self.shown.has_strip() { STRIP } else { 0.0 }
    }

    /// How many text lines show.
    fn shown_lines(&self) -> usize {
        ((self.upper() - TEXT_LESS) / LINE).max(1.0) as usize
    }

    fn grip_at(&self) -> [f32; 2] {
        [WIDTH - GRIP_IN, self.upper() - GRIP_IN]
    }

    fn view_at(&self) -> [f32; 2] {
        VIEW
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let local = offset(cursor, self.position.map(|value| -value));
        if !contains([0.0; 2], [WIDTH, self.total()], local) {
            return None;
        }
        Some(if contains(CLOSE, [11.0, 11.0], local) {
            Hit::Close
        } else if contains(self.grip_at(), [GRIP_SIZE; 2], local) {
            Hit::Resize
        } else if self.shown.card && contains(self.view_at(), VIEW_SIZE, local) {
            Hit::View
        } else if local[1] < DRAG_BAND[1] && local[0] > DRAG_BAND[0] {
            Hit::Grip
        } else {
            Hit::Window
        })
    }

    /// The strip's cell under `local`: below the strip's top + 3, every 25 px from x 3.
    fn cell_at(&self, local: [f32; 2]) -> Option<usize> {
        if !self.shown.has_strip() || local[1] <= self.total() - (CELL_UP - 1.0) {
            return None;
        }
        let cell = ((local[0] - 3.0) / CELL_STEP).floor();
        (cell >= 0.0 && (cell as usize) < CELLS).then_some(cell as usize)
    }

    /// The card or enchant in a cell: a known item (none on forged or brewed pieces).
    fn card_in(&self, cell: usize, items: &HashMap<u32, ItemInfo>) -> Option<u32> {
        let card = self.shown.cards[cell];
        (!self.shown.forged() && card != 0 && items.contains_key(&card)).then_some(card)
    }

    /// What a strip cell shows: a card or enchant (its name white in a slot, past them in the enchants' colour), else
    /// an open slot or one past the slots; nothing on a forged or brewed piece.
    fn cell(&self, cell: usize, items: &HashMap<u32, ItemInfo>) -> Option<Cell> {
        if self.shown.forged() {
            return None;
        }
        let slots = self.slots(items);
        Some(match self.card_in(cell, items) {
            Some(card) if cell < slots => Cell::Card(card, [255; 3]),
            Some(card) => Cell::Card(card, inventory::ENCHANTED[inventory::enchants(&self.shown.as_item(), items).clamp(1, 4) - 1]),
            None if cell < slots => Cell::Open,
            None => Cell::Closed,
        })
    }

    /// The illustration button and the filled card cells, for the test driver.
    pub fn button_boxes(&self, items: &HashMap<u32, ItemInfo>) -> Vec<(String, [f32; 4])> {
        let mut boxes = Vec::new();
        if self.shown.card {
            let corner = offset(self.position, self.view_at());
            boxes.push(("card_picture".into(), [corner[0], corner[1], VIEW_SIZE[0], VIEW_SIZE[1]]));
        }
        if self.shown.has_strip() {
            for cell in 0..CELLS {
                if let Some(card) = self.card_in(cell, items) {
                    let corner = offset(self.position, [CELL_X + cell as f32 * CELL_STEP, self.total() - CELL_UP]);
                    boxes.push((format!("card:{card}"), [corner[0], corner[1], CELL, CELL]));
                }
            }
        }
        let grip = offset(self.position, self.grip_at());
        boxes.push(("item_resize".into(), [grip[0], grip[1], GRIP_SIZE, GRIP_SIZE]));
        boxes
    }

    pub fn start_drag(&mut self, cursor: [f32; 2]) {
        self.drag = Some(offset(cursor, self.position.map(|value| -value)));
    }

    /// Held by the height grip.
    pub fn start_resize(&mut self, cursor: [f32; 2]) {
        self.resizing = Some((cursor[1], self.height));
    }

    /// Follows the pointer: the window by its top band, or its height by the grip (the width stays 280).
    pub fn drag_to(&mut self, cursor: [f32; 2]) {
        if let Some(grab) = self.drag {
            self.position = offset(cursor, grab.map(|value| -value));
        }
        if let Some((from, height)) = self.resizing {
            let least = if self.shown.has_strip() { LEAST_WITH_STRIP } else { BASE_HEIGHT };
            self.height = (height + cursor[1] - from).clamp(least, MOST_HEIGHT);
            self.fitted = true;
        }
    }

    pub fn drop_drag(&mut self) -> bool {
        self.drag.take().is_some() | self.resizing.take().is_some()
    }

    /// The wheel: 3 lines a notch, while the text is longer than the window shows.
    pub fn scroll_by(&mut self, notches: i32, lines: usize) {
        let last = lines.saturating_sub(self.shown_lines());
        self.scroll = (self.scroll as i32 + notches * WHEEL_LINES).clamp(0, last as i32) as usize;
    }

    /// The description's lines, cut to their characters: a rental's time left (by `now`, Unix seconds) first.
    pub fn lines(&self, items: &HashMap<u32, ItemInfo>, messages: &[String], now: u64) -> ColoredLines {
        let Some(info) = items.get(&self.shown.item) else { return Vec::new() };
        let mut color = [0, 0, 0];
        let mut lines = ColoredLines::new();
        if let Some(rental) = rental_line(messages.get(TIME_LEFT).map_or("", String::as_str), self.shown.expires, now) {
            lines.extend(cut(&rental, RENTAL_COLUMNS, &mut color));
        }
        for line in info.description(self.shown.identified) {
            lines.extend(cut(line, LINE_COLUMNS, &mut color));
        }
        lines
    }

    /// The pictures to load: the item's, and its cards' icons.
    pub fn images(&self, items: &HashMap<u32, ItemInfo>) -> Vec<String> {
        let mut paths: Vec<String> = items.get(&self.shown.item).map(|info| picture_path(info, self.shown.identified)).into_iter().collect();
        paths.extend(self.shown.cards.iter().filter(|&&card| card > 0).filter_map(|card| items.get(card).map(|info| inventory::icon_path(info, true))));
        paths
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &self,
        layer: &mut Layer<'a>,
        tooltip: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        images: &'a HashMap<String, Option<Texture>>,
        items: &HashMap<u32, ItemInfo>,
        (cards, messages, makers): (&CardNames, &[String], &HashMap<u32, Option<String>>),
        font: &Font,
        lines: &[Vec<([u8; 3], String)>],
        options: &[String],
        pointer: [f32; 2],
    ) {
        let at = |local: [f32; 2]| offset(self.position, local);
        // The sys box (its grade's for an enchant grade) with the background on it, and the strip's own box.
        // The compare window has its own.
        let grade = usize::from(self.shown.grade).min(4);
        let (sysbox, background) =
            if self.compare { (&assets.compare_box, &assets.compare_background) } else { (&assets.boxes[grade], &assets.backgrounds[grade]) };
        let [x, y] = self.position;
        pickup::draw_sysbox(layer, chrome, sysbox, [x, y, WIDTH, self.upper()]);
        layer.sprites.push((background, at([0.0, 0.0]), background.size));
        let info = items.get(&self.shown.item);
        if let Some(Some(picture)) = info.and_then(|info| images.get(&picture_path(info, self.shown.identified))) {
            layer.sprites.push((picture, at(PICTURE), picture.size));
        }
        // The title, each piece over its shadow, cut to 180 px.
        // ponytail: " [slots]" ends the title as before.
        let item = self.shown.as_item();
        let (name, maker) = inventory::full_name_maker(items, cards, messages, makers, &item, true);
        let fitted = crate::window::fit(font, &name, TITLE_WIDTH);
        let mut left = TITLE[0];
        for (piece, color, shadow) in title_pieces(&fitted, maker, &item, items) {
            layer.texts.push((piece.clone(), at([left + 1.0, TITLE[1] + 1.0]), shadow));
            layer.texts.push((piece.clone(), at([left, TITLE[1]]), color));
            left += font.width(&piece);
        }
        if self.shown.card {
            let view = &assets.view[usize::from(self.hit(pointer) == Some(Hit::View))];
            layer.sprites.push((view, at(self.view_at()), VIEW_SIZE));
        }
        let close = &assets.close[usize::from(self.hit(pointer) == Some(Hit::Close))];
        layer.sprites.push((close, at(CLOSE), close.size));
        for (row, pieces) in lines.iter().skip(self.scroll).take(self.shown_lines()).enumerate() {
            let mut x = TEXT[0];
            for (color, piece) in pieces {
                layer.texts.push((piece.clone(), at([x, TEXT[1] + row as f32 * LINE]), *color));
                x += font.width(piece);
            }
        }
        let grip = if self.compare { &assets.compare_resize } else { &assets.resize };
        layer.sprites.push((grip, at(self.grip_at()), grip.size));

        // The option rows and the card strip take the plain box (the compare window's own), never a grade's.
        let inner = if self.compare { &assets.compare_box } else { &assets.boxes[0] };
        // A boxed row per random option, then empty ones to keep level with the window beside it.
        for row in 0..self.rows {
            pickup::draw_sysbox(layer, chrome, inner, [x, y + self.row_at(row), WIDTH, ROW]);
            if let Some(text) = options.get(row) {
                layer.texts.push((text.clone(), at(offset([0.0, self.row_at(row)], ROW_TEXT)), [0, 0, 0]));
            }
        }

        // The card strip: four cells, a card or enchant's icon, else an open slot or one past the slots; a forged or
        // brewed piece shows nothing in it. The name of the one under the pointer above it.
        if !self.shown.has_strip() {
            return;
        }
        let height = self.total();
        pickup::draw_sysbox(layer, chrome, inner, [x, y + height - STRIP, WIDTH, STRIP]);
        let (empty, disabled) =
            if self.compare { (&assets.compare_empty_slot, &assets.compare_disabled_slot) } else { (&assets.empty_slot, &assets.disabled_slot) };
        let hovered = self.cell_at(offset(pointer, self.position.map(|value| -value)));
        for cell in 0..CELLS {
            let corner = at([CELL_X + cell as f32 * CELL_STEP, height - CELL_UP]);
            match self.cell(cell, items) {
                Some(Cell::Card(card, color)) => {
                    if let Some(Some(icon)) = items.get(&card).and_then(|info| images.get(&inventory::icon_path(info, true))) {
                        layer.sprites.push((icon, corner, [CELL, CELL]));
                    }
                    if hovered == Some(cell) {
                        let name = items.get(&card).map_or("", |info| info.name.as_str());
                        let place = at([CELL_X + cell as f32 * CELL_STEP, height - CELL_NAME_UP]);
                        crate::window::tooltip(tooltip, chrome, font, name, place, color);
                    }
                }
                Some(Cell::Open) => layer.sprites.push((empty, corner, [CELL, CELL])),
                Some(Cell::Closed) => layer.sprites.push((disabled, corner, [CELL, CELL])),
                None => {}
            }
        }
    }
}

/// Picture names from the card-picture table. A line is `id#name#`, and some lines carry a second name.
pub struct CardPictures {
    names: HashMap<u32, Vec<String>>,
}

const CARD_ILLUST: &str = "data\\num2cardillustnametable.txt";
const CARD_ILLUST_ID: &str = "data\\idnum2cardillustnametable.txt";

impl CardPictures {
    pub fn load(grf: &Grf) -> Self {
        let text = |path: &str| grf.read(path).map(|bytes| encoding_rs::EUC_KR.decode_without_bom_handling(&bytes).0.into_owned()).unwrap_or_default();
        // ponytail: both file names show up in client data. The picture uses the first name on the line. Keeping the
        // file and the column the player's data.grf actually has would replace reading both.
        let mut pictures = Self::parse(&text(CARD_ILLUST));
        for (id, names) in Self::parse(&text(CARD_ILLUST_ID)).names {
            pictures.names.entry(id).or_insert(names);
        }
        pictures
    }

    pub fn parse(text: &str) -> Self {
        let mut names = HashMap::new();
        for line in text.lines() {
            let mut parts = line.split('#');
            let Some(id) = parts.next().and_then(|id| id.trim().parse::<u32>().ok()) else { continue };
            let found: Vec<String> = parts.map(str::trim).filter(|name| !name.is_empty()).map(str::to_owned).collect();
            if !found.is_empty() {
                names.insert(id, found);
            }
        }
        Self { names }
    }

    /// Every name on the card's line. The picture uses the first.
    pub fn names(&self, id: u32) -> &[String] {
        self.names.get(&id).map(Vec::as_slice).unwrap_or(&[])
    }

    /// The picture's name: the first one on the line.
    pub fn name(&self, id: u32) -> Option<&str> {
        self.names(id).first().map(String::as_str)
    }
}

/// `cardbmp\<name>.bmp` under the interface textures.
pub fn cardbmp_path(name: &str) -> String {
    format!("{UI}cardbmp\\{name}.bmp")
}

/// The card's picture: 300 × 417 at ((W − 640) / 2, 0), a title bar with the close button alone and the card's name,
/// the picture unscaled under it.
pub struct CardView {
    pub path: String,
    /// Which card, for its button to close it again.
    pub card: u32,
    pub name: String,
    pub position: [f32; 2],
    drag: Option<[f32; 2]>,
}

const CARD_VIEW: [f32; 2] = [300.0, 417.0];
const CARD_PICTURE: [f32; 2] = [0.0, 17.0];
const CARD_NAME: [f32; 2] = [10.0, 3.0];
const CARD_CLOSE: [f32; 2] = [CARD_VIEW[0] - 14.0, 3.0];

/// Where a card's picture opens on a screen this size.
pub fn card_view_place(screen: [f32; 2]) -> [f32; 2] {
    [((screen[0] - 640.0) / 2.0).floor(), 0.0]
}

#[derive(Debug, PartialEq)]
pub enum ViewHit {
    Close,
    Grip,
    Window,
}

impl CardView {
    pub fn new(path: String, card: u32, name: String, screen: [f32; 2]) -> Self {
        Self { path, card, name, position: card_view_place(screen), drag: None }
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<ViewHit> {
        let local = offset(cursor, self.position.map(|value| -value));
        if !contains([0.0; 2], CARD_VIEW, local) {
            return None;
        }
        Some(if contains(CARD_CLOSE, [11.0, 11.0], local) {
            ViewHit::Close
        } else if local[1] < CARD_PICTURE[1] {
            ViewHit::Grip
        } else {
            ViewHit::Window
        })
    }

    /// Where it is and how big, for the test driver.
    pub fn area(&self) -> [f32; 4] {
        [self.position[0], self.position[1], CARD_VIEW[0], CARD_VIEW[1]]
    }

    /// Its close button, for the test driver.
    pub fn close_box(&self) -> [f32; 4] {
        let [x, y] = offset(self.position, CARD_CLOSE);
        [x, y, 11.0, 11.0]
    }

    pub fn start_drag(&mut self, cursor: [f32; 2]) {
        self.drag = Some(offset(cursor, self.position.map(|value| -value)));
    }

    pub fn drag_to(&mut self, cursor: [f32; 2]) {
        if let Some(grab) = self.drag {
            self.position = offset(cursor, grab.map(|value| -value));
        }
    }

    pub fn drop_drag(&mut self) -> bool {
        self.drag.take().is_some()
    }

    pub fn draw<'a>(&self, layer: &mut Layer<'a>, chrome: &'a Chrome, assets: &'a Assets, images: &'a HashMap<String, Option<Texture>>, pointer: [f32; 2]) {
        let at = |local: [f32; 2]| offset(self.position, local);
        chrome.draw_title_bar(layer, at([0.0, 0.0]), CARD_VIEW[0]);
        layer.texts.push((self.name.clone(), at(offset(CARD_NAME, [1.0, 1.0])), [255; 3]));
        layer.texts.push((self.name.clone(), at(CARD_NAME), [0, 0, 0]));
        let close = &assets.close[usize::from(self.hit(pointer) == Some(ViewHit::Close))];
        layer.sprites.push((close, at(CARD_CLOSE), close.size));
        layer.sprites.push((&chrome.white, at(CARD_PICTURE), [CARD_VIEW[0], CARD_VIEW[1] - CARD_PICTURE[1]]));
        if let Some(Some(picture)) = images.get(&self.path) {
            layer.sprites.push((picture, at(CARD_PICTURE), picture.size));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(item: u32, kind: u8, cards: [u32; 4]) -> Shown {
        Shown {
            item,
            identified: true,
            refine: 0,
            cards,
            kind,
            grade: 0,
            broken: false,
            expires: 0,
            card: false,
            location: 0,
            worn: false,
            options: Vec::new(),
        }
    }

    fn items() -> HashMap<u32, ItemInfo> {
        let info = |name: &str, slots| ItemInfo { name: name.into(), slots, ..Default::default() };
        HashMap::from([(1201, info("Knife", 3)), (2301, info("Cotton Shirt", 0)), (4001, info("Poring Card", 0)), (4700, info("STR+1", 0))])
    }

    #[test]
    fn the_same_item_is_its_id_refine_and_cards() {
        let sword = Shown { refine: 4, ..shown(1201, net::ITEM_WEAPON, [4035, 0, 0, 0]) };
        // Another stack, unidentified or broken: still the same.
        assert!(sword.same_item(&Shown { identified: false, broken: true, ..sword.clone() }));
        assert!(!sword.same_item(&Shown { refine: 5, ..sword.clone() }));
        assert!(!sword.same_item(&Shown { cards: [0; 4], ..sword.clone() }));
        assert!(!sword.same_item(&Shown { item: 1202, ..sword.clone() }));
        // A link: equipment when it has slots.
        assert_eq!(Shown::of_link(1201, 0, [0; 4], false, &items()).kind, net::ITEM_ARMOR);
        assert_eq!(Shown::of_link(4001, 0, [0; 4], true, &items()).kind, 0);
        let linked = Shown::of_link(1201, 0, [0; 4], false, &items()).with_link_options(&[[3, 0, 1]]);
        assert_eq!(linked.options, [ItemOption { id: 3, value: 1, param: 0 }]);
    }

    #[test]
    fn opens_a_cards_window_from_the_strip() {
        let items = items();
        let mut knife = ItemWindow::new(shown(1201, net::ITEM_WEAPON, [4001, 0, 0, 4700]));
        knife.fit(4);
        let cell = |cell: f32| offset(knife.position, [CELL_X + 1.0 + cell * CELL_STEP, knife.height - 20.0]);
        // A card in a slot opens as a card; an enchant past the slots as an enchant, without the picture button.
        assert_eq!(knife.card_under(cell(0.0), &items), Some((4001, false)));
        assert_eq!(knife.card_under(cell(3.0), &items), Some((4700, true)));
        assert_eq!(knife.card_under(cell(1.0), &items), None);
        let card = ItemWindow::new(Shown::card(4001, false));
        let enchant = ItemWindow::new(Shown::card(4700, true));
        let view = offset(DEFAULT_PLACE, [VIEW[0] + 2.0, VIEW[1] + 2.0]);
        assert_eq!((card.hit(view), enchant.hit(view)), (Some(Hit::View), Some(Hit::Window)));
        assert!(!card.shown.has_strip());
        // The picture: 300 × 417 at ((W − 640) / 2, 0).
        assert_eq!((card_view_place([1024.0, 768.0]), card_view_place([1280.0, 720.0])), ([192.0, 0.0], [320.0, 0.0]));
        assert_eq!(CardView::new(String::new(), 4001, String::new(), [1024.0, 768.0]).area(), [192.0, 0.0, 300.0, 417.0]);
        assert_eq!((restored_card_place([300.0, 300.0], [1024.0, 768.0]), restored_card_place([2000.0, 300.0], [1024.0, 768.0])), ([300.0, 300.0], CARD_PLACE));
    }

    #[test]
    fn compares_with_whats_worn_in_the_same_slot() {
        let worn = |index, location| InventoryItem { index, location, worn: location, ..Default::default() };
        let items = [worn(1, 2), worn(2, 0x8000), InventoryItem { index: 3, location: 16, ..Default::default() }];
        let of = |location, worn| Shown { location, worn, ..shown(1101, net::ITEM_WEAPON, [0; 4]) };
        // A weapon with the Knife worn, arrows with arrows worn; no armor worn; the worn piece itself; a link.
        assert_eq!(compared(&of(2, false), &items).map(|item| item.index), Some(1));
        assert_eq!(compared(&of(0x8000, false), &items).map(|item| item.index), Some(2));
        assert_eq!(compared(&of(16, false), &items), None);
        assert_eq!(compared(&of(2, true), &items), None);
        assert_eq!(compared(&of(0, false), &items), None);
        // Flush left of the description.
        assert_eq!(compare_place([500.0, 126.0]), [220.0, 126.0]);
    }

    #[test]
    fn fills_in_an_options_text() {
        let texts = HashMap::from([(3, "STR +%d".to_owned()), (10, "MaxSP +%d%%".to_owned())]);
        let option = |id, value| ItemOption { id, value, param: 0 };
        assert_eq!(option_text(&texts, &option(3, 1)).as_deref(), Some("STR +1"));
        assert_eq!(option_text(&texts, &option(10, 3)).as_deref(), Some("MaxSP +3%"));
        assert_eq!(option_text(&texts, &option(99, 3)), None);
        // The rows: an identified item's options that have a text.
        let mut shown = shown(2101, net::ITEM_ARMOR, [0; 4]);
        shown.options = vec![option(3, 1), option(99, 2), option(10, 3)];
        assert_eq!(shown.option_lines(&texts), ["STR +1", "MaxSP +3%"]);
        assert!(Shown { identified: false, ..shown }.option_lines(&texts).is_empty());
    }

    #[test]
    fn grows_both_windows_to_the_longer_option_list() {
        let window = |rows| {
            let mut window = ItemWindow::new(shown(2101, net::ITEM_ARMOR, [0; 4]));
            window.fit(4);
            window.set_rows(rows);
            window
        };
        // No rows: as before.
        let (mut description, mut worn) = (window(0), window(0));
        pair(&mut description, &mut worn);
        assert_eq!((description.area()[3], worn.area()[3]), (175.0, 175.0));
        // Three beside five: both take five rows, so each row sits at the same height on both sides.
        let (mut description, mut worn) = (window(3), window(5));
        pair(&mut description, &mut worn);
        assert_eq!((description.rows, worn.rows), (5, 5));
        assert_eq!((description.area()[3], worn.area()[3]), (175.0 + 5.0 * ROW, 175.0 + 5.0 * ROW));
        assert_eq!(description.row_at(0), worn.row_at(0));
        // The text keeps its lines; the strip stays at the bottom.
        assert_eq!(description.shown_lines(), window(0).shown_lines());
        assert_eq!(description.upper() + 5.0 * ROW + STRIP, description.area()[3]);
        // A taller text on one side: the other grows to it, rows still level.
        let mut long = window(3);
        long.height = 300.0;
        let mut short = window(3);
        pair(&mut long, &mut short);
        assert_eq!(short.area()[3], long.area()[3]);
        assert_eq!(short.row_at(2), long.row_at(2));
        // An unidentified one beside it: no rows, no strip, its own height.
        let mut unknown = ItemWindow::new(Shown { identified: false, ..shown(2101, net::ITEM_ARMOR, [0; 4]) });
        unknown.fit(2);
        unknown.set_rows(0);
        let mut worn = window(3);
        pair(&mut unknown, &mut worn);
        assert_eq!((unknown.rows, unknown.area()[3]), (0, 120.0));
    }

    #[test]
    fn counts_a_rental_down() {
        let template = "Time Left: %d hours %d minutes";
        assert_eq!(rental_line(template, 0, 1_000).as_deref(), None);
        assert_eq!(rental_line(template, 1_000 + 7_500, 1_000).as_deref(), Some("^ff0000Time Left: 2 hours 5 minutes^000000"));
        assert_eq!(rental_line(template, 1_000 + 59, 1_000).as_deref(), Some("^ff0000Time Left: 0 hours 0 minutes^000000"));
        // Past its end: nothing left.
        assert_eq!(rental_line(template, 1_000, 5_000).as_deref(), Some("^ff0000Time Left: 0 hours 0 minutes^000000"));
    }

    #[test]
    fn fits_its_height_to_the_text() {
        assert_eq!(fitted_height(0, false), 120.0);
        assert_eq!(fitted_height(4, false), 120.0);
        assert_eq!(fitted_height(5, false), 138.0);
        assert_eq!(fitted_height(30, false), 450.0);
        // With the card strip: 29 more, at least 175.
        assert_eq!(fitted_height(4, true), 175.0);
        assert_eq!(fitted_height(7, true), 203.0);
        assert_eq!(fitted_height(30, true), 450.0);
        // Once per item: the grip's height then stays.
        let mut window = ItemWindow::new(shown(2301, net::ITEM_ARMOR, [0; 4]));
        window.fit(7);
        assert_eq!(window.height, 203.0);
        window.start_resize([0.0, 500.0]);
        window.drag_to([40.0, 400.0]);
        assert_eq!(window.height, 175.0);
        window.drag_to([40.0, 900.0]);
        assert_eq!(window.height, 450.0);
        assert!(window.drop_drag());
        window.fit(4);
        assert_eq!(window.height, 450.0);
    }

    #[test]
    fn cuts_lines_by_characters() {
        let text = |lines: ColoredLines| lines.iter().map(|line| line.iter().map(|(_, piece)| piece.as_str()).collect::<String>()).collect::<Vec<_>>();
        let mut color = [0, 0, 0];
        // Colour codes take no room, and the colour carries.
        let lines = cut("^ff0000Attack^000000 power of this sword is 100", 26, &mut color);
        assert_eq!(text(lines.clone()), ["Attack power of this sword", " is 100"]);
        assert_eq!(lines[0][0], ([0xFF, 0, 0], "Attack".to_owned()));
        assert_eq!(color, [0, 0, 0]);
        // A Korean character takes two and isn't split; "\r\n" breaks.
        assert_eq!(text(cut(&format!("{}가나", "a".repeat(23)), 26, &mut color)), [format!("{}가", "a".repeat(23)), "나".to_owned()]);
        assert_eq!(text(cut("one\r\ntwo", 26, &mut color)), ["one", "two"]);
    }

    #[test]
    fn drags_by_its_top_band_only() {
        // Jellopy: no card strip.
        let window = ItemWindow::new(shown(909, 3, [0; 4]));
        let at = |local: [f32; 2]| offset(DEFAULT_PLACE, local);
        assert_eq!(window.hit(at([61.0, 5.0])), Some(Hit::Grip));
        assert_eq!(window.hit(at([59.0, 5.0])), Some(Hit::Window));
        assert_eq!(window.hit(at([61.0, 17.0])), Some(Hit::Window));
        assert_eq!(window.hit(at([CLOSE[0] + 2.0, CLOSE[1] + 2.0])), Some(Hit::Close));
        assert_eq!(window.hit(at([WIDTH - 10.0, 120.0 - 10.0])), Some(Hit::Resize));
        assert_eq!(window.hit(at([WIDTH + 1.0, 5.0])), None);
        // Where it comes back to: as saved, or the game's spot when that's off the screen.
        assert_eq!(restored_place([300.0, 200.0], [1024.0, 768.0]), [300.0, 200.0]);
        assert_eq!(restored_place([1500.0, 200.0], [1024.0, 768.0]), [472.0, 201.0]);
    }

    #[test]
    fn shows_cards_and_enchants_in_the_strip() {
        let items = items();
        let knife = ItemWindow::new(shown(1201, net::ITEM_WEAPON, [4001, 0, 0, 4700]));
        let cells: Vec<_> = (0..CELLS).map(|cell| knife.cell(cell, &items)).collect();
        let one = inventory::ENCHANTED[0];
        assert_eq!(cells, [Some(Cell::Card(4001, [255; 3])), Some(Cell::Open), Some(Cell::Open), Some(Cell::Card(4700, one))]);
        // Its height grip sits above the strip, and a cell under the pointer is no button.
        let mut window = knife;
        window.fit(4);
        assert_eq!(window.grip_at(), [WIDTH - 17.0, 175.0 - 29.0 - 17.0]);
        // Below h - 26.
        assert_eq!(window.cell_at([CELL_X + 1.0, window.height - 26.0]), None);
        let cell = offset(window.position, [CELL_X + 1.0, window.height - 25.0]);
        assert_eq!(window.cell_at(offset(cell, window.position.map(|value| -value))), Some(0));
        assert_eq!(window.hit(cell), Some(Hit::Window));
        // A slotless shirt with two enchants: past the slots, both in the two-enchant colour.
        let shirt = ItemWindow::new(shown(2301, net::ITEM_ARMOR, [0, 0, 4700, 4700]));
        let two = inventory::ENCHANTED[1];
        assert_eq!(shirt.cell(0, &items), Some(Cell::Closed));
        assert_eq!(shirt.cell(2, &items), Some(Cell::Card(4700, two)));
        // Forged: the strip stays, empty. Unidentified, a card or a pet egg: no strip.
        let forged = ItemWindow::new(shown(1201, net::ITEM_WEAPON, [0x00FF, 5 << 8 | 3, 4, 2]));
        assert!(forged.shown.has_strip());
        assert_eq!(forged.cell(0, &items), None);
        assert!(!Shown { identified: false, ..shown(1201, net::ITEM_WEAPON, [0; 4]) }.has_strip());
        assert!(!shown(4001, net::ITEM_CARD, [0; 4]).has_strip());
        assert!(!shown(9001, net::ITEM_PET_EGG, [0; 4]).has_strip());
    }

    #[test]
    fn colours_the_title() {
        let items = items();
        let item = |cards, broken, identified| InventoryItem { id: 2301, kind: net::ITEM_ARMOR, cards, broken, identified, ..Default::default() };
        let pieces = |item: &InventoryItem, maker| title_pieces("Cotton Shirt", maker, item, &items);
        let black = [0, 0, 0];
        assert_eq!(pieces(&item([0; 4], false, true), None), [("Cotton Shirt".to_owned(), black, [255; 3])]);
        assert_eq!(pieces(&item([0, 0, 0, 4700], false, true), None)[0].1, ONE_ENCHANT);
        assert_eq!(pieces(&item([0, 0, 4700, 4700], false, true), None)[0].1, inventory::ENCHANTED[1]);
        assert_eq!(pieces(&item([0; 4], true, true), None), [("Cotton Shirt".to_owned(), black, BROKEN_SHADOW)]);
        assert_eq!(pieces(&item([0; 4], true, false), None)[0].1, UNIDENTIFIED_BROKEN);
        // The maker's name in its colour on light grey.
        let maker = [64, 64, 224];
        assert_eq!(
            pieces(&item([0; 4], false, true), Some((0..6, maker))),
            [("Cotton".to_owned(), maker, MAKER_SHADOW), (" Shirt".to_owned(), black, [255; 3])]
        );
    }

    #[test]
    fn scrolls_three_lines_a_notch() {
        let mut window = ItemWindow::new(shown(2301, net::ITEM_ARMOR, [0; 4]));
        window.fit(4);
        // 175 tall with the strip shows 5 lines.
        window.scroll_by(1, 20);
        assert_eq!(window.scroll, 3);
        window.scroll_by(5, 20);
        assert_eq!(window.scroll, 15);
        window.scroll_by(-9, 20);
        assert_eq!(window.scroll, 0);
    }

    #[test]
    fn names_a_cards_picture_from_a_small_table() {
        let table = CardPictures::parse("4001#poring_card#\n4002#fabre_card#fabre#\n#nope#\n");
        assert_eq!(table.name(4001), Some("poring_card"));
        assert_eq!(table.names(4002), ["fabre_card".to_string(), "fabre".to_string()]);
        assert_eq!(table.name(4002), Some("fabre_card"));
        assert!(table.name(9).is_none());
        assert_eq!(cardbmp_path("poring_card"), format!("{UI}cardbmp\\poring_card.bmp"));
    }
}
