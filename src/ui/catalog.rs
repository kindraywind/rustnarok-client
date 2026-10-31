//! The Universal Catalog's store search (ZC_OPEN_SEARCH_STORE_INFO): find an item, or a card in one, in the map's
//! vending or buying stores by name and price, then see where a store stands or open it from afar.

use std::collections::HashMap;

use rustnarok_client::grf::Grf;
use rustnarok_client::lua::ItemInfo;
use rustnarok_client::net::{CatalogRow, InventoryItem};
use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};
use crate::window::{self, BUTTON_SIZE, Chrome, Clicks, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI, contains, inventory, offset, screen, shop};

const TITLE: usize = 1780;
const SELL: usize = 1778;
const BUY: usize = 1779;
const SHOP_NAME: usize = 1781;
const ITEM: usize = 1060;
const AMOUNT: usize = 1782;
const PRICE: usize = 1783;
const SIMILAR: usize = 1788;
/// The card box's tooltip.
const CARD_TIP: usize = 1801;
const ATTEMPTS: usize = 1802;
const PREV: usize = 1053;
const NEXT: usize = 1054;
const SEARCHING: usize = 1102;
/// A Gold catalog asks before it opens a store from afar.
pub const OPEN_STALL_ASKS: usize = 1785;
pub const NO_NAME: usize = 1809;
pub const NO_SUCH_ITEM: usize = 1799;
pub const NO_SUCH_CARD: usize = 1812;
pub const TOO_MANY: usize = 1784;
pub const BAD_PRICE: usize = 1804;
/// Items and cards one search may name together.
const MOST_NAMES: usize = 20;
/// Each box takes this many characters.
const MOST_TEXT: usize = 50;

/// What a failed search (ZC_SEARCH_STORE_INFO_FAILED's reason) shows: a line in the window, or a message box.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Failure {
    Status(usize),
    Box(usize),
}

pub fn failure(reason: u8) -> Option<Failure> {
    match reason {
        0 => Some(Failure::Status(1803)),
        1 => Some(Failure::Status(TOO_MANY)),
        2 => Some(Failure::Status(1798)),
        3 => Some(Failure::Box(1800)),
        4 => Some(Failure::Box(1797)),
        _ => None,
    }
}

/// A name found by what was typed: holding it ("Include similar items?"), else exactly it. Letter case counts.
fn found(name: &str, typed: &str, similar: bool) -> bool {
    if similar { name.contains(typed) } else { name == typed }
}

/// The items `typed` finds, in ID order: by the item's name or its card word; with `cards_only`, cards alone.
pub fn lookup(typed: &str, cards_only: bool, similar: bool, items: &HashMap<u32, ItemInfo>, cards: &inventory::CardNames) -> Vec<u32> {
    let mut ids: Vec<u32> = items
        .iter()
        .filter(|(id, info)| {
            let word = cards.word(**id);
            !info.name.is_empty()
                && (!cards_only || word.is_some())
                && (found(&info.name, typed, similar) || word.is_some_and(|word| found(word, typed, similar)))
        })
        .map(|(&id, _)| id)
        .collect();
    ids.sort_unstable();
    ids
}

/// A price box's number: empty is 0; anything that isn't plainly a number (commas, leading zeros, too big) is none.
fn price(text: &str) -> Option<u32> {
    if text.is_empty() {
        return Some(0);
    }
    text.parse::<i32>().ok().filter(|price| *price >= 0 && price.to_string() == text).map(|price| price as u32)
}

/// What a search sends (CZ_SEARCH_STORE_INFO).
#[derive(Debug, PartialEq)]
pub struct Query {
    pub items: Vec<u32>,
    pub cards: Vec<u32>,
    pub max: u32,
    pub min: u32,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Field {
    Name,
    Card,
    Min,
    Max,
}

/// A result's columns: the icon, the shop's name, the item's, the amount and the price.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Column {
    Icon,
    Shop,
    Item,
    Amount,
    Price,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(TitleHit),
    /// The store kind: true buying stores.
    Kind(bool),
    Field(Field),
    /// "Include similar items?"
    Similar,
    Search,
    Prev,
    Next,
    Row(usize, Column),
    Window,
}

const WIDTH: f32 = 445.0;
const HEIGHT: f32 = 380.0;
/// Where it opens, in 1024 × 768 screen units.
const PLACE: [f32; 2] = [300.0, 100.0];
const BASE_BUTTON: [f32; 2] = [3.0, 3.0];
const LABELS: [(&str, f32); 4] = [("Type", 5.0), ("Item", 98.0), ("Card", 185.0), ("Zeny", 273.0)];
const LABEL_TOP: f32 = 20.0;
const LABEL: [u8; 3] = [255, 60, 127];
/// The Sell and Buy radios, each this big; Buy is over Sell, and the boxes over both.
const KINDS: [f32; 2] = [10.0, 50.0];
const KIND_TOP: f32 = 40.0;
const KIND_SIZE: [f32; 2] = [WIDTH / 2.0, 12.0];
const FIELD_TOP: f32 = 36.0;
const FIELD_SIZE: [f32; 2] = [76.0, 16.0];
const TILDE: [f32; 2] = [353.0, 38.0];
const SIMILAR_AT: [f32; 2] = [100.0, 60.0];
const SIMILAR_SIZE: [f32; 2] = [160.0, 12.0];
const ATTEMPTS_RIGHT: [f32; 2] = [WIDTH - 60.0, 60.0];
const SEARCH: [f32; 2] = [WIDTH - 45.0, 56.0];
const SEARCH_SIZE: [f32; 2] = [42.0, 20.0];
const LIST: [f32; 2] = [5.0, 80.0];
const LIST_WIDTH: f32 = WIDTH - 10.0;
const HEADER_HEIGHT: f32 = 20.0;
const ROWS_TOP: f32 = 100.0;
const ROW: f32 = 25.0;
const ROWS: usize = 10;
/// The columns' left edges (the icon's from the list's); the price's right edge is the list's.
const SHOP_COLUMN: f32 = 50.0;
const ITEM_COLUMN: f32 = 160.0;
const AMOUNT_COLUMN: f32 = 330.0;
const PRICE_COLUMN: f32 = 360.0;
const PRICE_RIGHT: f32 = 435.0;
/// A double click opens a store only on its name.
const SHOP_CLICKS: [f32; 2] = [51.0, 160.0];
const ICON_X: f32 = 13.0;
const GRADE_DOWN: f32 = 12.0;
const SHOP_X: f32 = 55.0;
const SHOP_WIDTH: f32 = 100.0;
const ITEM_X: f32 = 165.0;
const ITEM_WIDTH: f32 = 160.0;
const ROW_TEXT: f32 = 7.0;
const HEADER_TEXT: f32 = 85.0;
/// Prev and Next stand this far either side of the middle; the page number and the status line on their row.
const PAGE_GAP: f32 = 20.0;
const FOOTER_TOP: f32 = HEIGHT - 21.0;
const STATUS: [f32; 2] = [5.0, HEIGHT - 20.0];
// ponytail: Prev and Next take clicks on a fixed box beside the middle (the hit test has no font for their text's
// width); their words fit inside it.
const PAGE_BUTTON: [f32; 2] = [40.0, 14.0];
const TEXT: [u8; 3] = [0, 0, 0];
const RED: [u8; 3] = [255, 0, 0];
const PAGE_BUTTON_TEXT: [u8; 3] = [0x8E, 0x8E, 0x8E];
const PAGE_BUTTON_HOVER: [u8; 3] = [0x00, 0xB7, 0xEF];

/// A price's colours by its size: the text's, and the one drawn over it 1 px to the left (if any).
pub fn price_colors(price: u32) -> ([u8; 3], Option<[u8; 3]>) {
    match price {
        1_000_000_000.. => ([255, 0, 127], Some([255, 0, 0])),
        100_000_000.. => ([200, 200, 100], Some([0, 0, 0])),
        10_000_000.. => ([255, 0, 0], None),
        1_000_000.. => ([0, 255, 0], Some([0, 0, 0])),
        100_000.. => ([0, 0, 255], None),
        10_000.. => ([255, 30, 255], None),
        1_000.. => ([255, 255, 0], Some([255, 0, 0])),
        100.. => ([0, 255, 255], Some([0, 0, 255])),
        10.. => ([200, 0, 200], Some([0, 0, 255])),
        _ => ([0, 255, 255], Some([0, 0, 0])),
    }
}

pub struct Assets {
    radio: [Texture; 2],
    check: [Texture; 2],
    /// The search button: normal, pointed at, pressed.
    search: [Texture; 3],
    grades: [Texture; 4],
    header: Texture,
    picked: Texture,
    grid: Texture,
    /// A box's inside, and a box's while it's off.
    field: [Texture; 2],
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |path: String| gpu.load_texture(grf, &path, false);
        Self {
            radio: ["radiobtn_off", "radiobtn_on"].map(|name| image(format!("{UI}{name}.bmp"))),
            check: ["checkbox_0", "checkbox_1"].map(|name| image(format!("{UI}{name}.bmp"))),
            search: ["btn_find", "btn_find_a", "btn_find_b"].map(|name| image(format!("{UI}{name}.bmp"))),
            grades: [1, 2, 3, 4].map(|grade| image(format!("{UI}grade_enchant\\grade_icon{grade}.bmp"))),
            header: gpu.solid([0xFE, 0xFF, 0xDF, 0xFF]),
            picked: gpu.solid([0xCA, 0xDF, 0xFB, 0xFF]),
            grid: gpu.solid([0xC0, 0xC0, 0xC0, 0xFF]),
            field: [gpu.solid([0xE8, 0xE8, 0xE8, 0xFF]), gpu.solid([0xCA, 0xCA, 0xCA, 0xFF])],
        }
    }
}

/// The search window: what's typed, the results so far, the page shown, and where a store found stands.
pub struct Catalog {
    pub frame: Frame,
    /// A Gold catalog: a store found opens (after asking), else its place is shown.
    pub remote: bool,
    pub uses: u8,
    pub buying: bool,
    pub name: String,
    pub card: String,
    pub min: String,
    pub max: String,
    /// "Include similar items?": a name holding what's typed, else that name exactly. On at first.
    pub similar: bool,
    pub focus: Option<Field>,
    pub rows: Vec<CatalogRow>,
    /// The server has another page.
    pub more: bool,
    /// The page shown, from 1; ten rows a page.
    pub page: usize,
    /// A search is out (the search button is off).
    pub searching: bool,
    /// The line under the list: why a search failed.
    pub status: Option<usize>,
    pub mark: Option<[i16; 2]>,
    clicks: Clicks,
}

impl Catalog {
    pub fn new(remote: bool, uses: u8) -> Self {
        let [width, height] = screen();
        let mut frame = Frame::new([(PLACE[0] * width / 1024.0).floor(), (PLACE[1] * height / 768.0).floor()], WIDTH);
        frame.open = true;
        Self {
            frame,
            remote,
            uses,
            buying: false,
            name: String::new(),
            card: String::new(),
            min: String::new(),
            max: String::new(),
            similar: true,
            focus: Some(Field::Name),
            rows: Vec::new(),
            more: false,
            page: 1,
            searching: false,
            status: None,
            mark: None,
            clicks: Clicks::default(),
        }
    }

    /// The card and price boxes take typing only once an item is named.
    pub fn enabled(&self, field: Field) -> bool {
        field == Field::Name || !self.name.is_empty()
    }

    pub fn can_search(&self) -> bool {
        !self.name.is_empty() && !self.searching
    }

    /// What a search sends, or the line to show instead. It names at most 20 items and cards; a price box holds a
    /// plain number, and the least price no more than the most (when that's given).
    pub fn query(&self, items: &HashMap<u32, ItemInfo>, cards: &inventory::CardNames) -> Result<Query, usize> {
        if self.name.is_empty() {
            return Err(NO_NAME);
        }
        let found_items = lookup(&self.name, false, self.similar, items, cards);
        if found_items.is_empty() {
            return Err(NO_SUCH_ITEM);
        }
        let found_cards = if self.card.is_empty() { Vec::new() } else { lookup(&self.card, true, self.similar, items, cards) };
        if !self.card.is_empty() && found_cards.is_empty() {
            return Err(NO_SUCH_CARD);
        }
        if found_items.len() + found_cards.len() > MOST_NAMES {
            return Err(TOO_MANY);
        }
        match (price(&self.min), price(&self.max)) {
            (Some(min), Some(max)) if self.max.is_empty() || min <= max => Ok(Query { items: found_items, cards: found_cards, max, min }),
            _ => Err(BAD_PRICE),
        }
    }

    /// A search sent: the line clears and the button waits for the answer.
    pub fn sent(&mut self) {
        self.status = None;
        self.searching = true;
    }

    /// A page of results: the first replaces the list (and shows page 1), a later one adds to it.
    pub fn page(&mut self, first: bool, more: bool, uses: u8, rows: Vec<CatalogRow>) {
        if first {
            self.rows = rows;
            self.page = 1;
        } else {
            self.rows.extend(rows);
        }
        self.more = more;
        self.uses = uses;
        self.searching = false;
    }

    /// A failed search: its line, or the message box to show. Nothing found also takes an attempt and empties the
    /// list.
    pub fn failed(&mut self, reason: u8) -> Option<usize> {
        self.searching = false;
        if reason == 0 {
            self.rows.clear();
            self.page = 1;
            self.uses = self.uses.saturating_sub(1);
        }
        match failure(reason)? {
            Failure::Status(message) => {
                self.status = Some(message);
                None
            }
            Failure::Box(message) => Some(message),
        }
    }

    /// The line under the list: why a search failed, or "Searching..." while one is out with nothing listed.
    pub fn status_line(&self) -> Option<usize> {
        self.status.or((self.searching && self.rows.is_empty()).then_some(SEARCHING))
    }

    /// The page row (Prev, the page, Next) shows with results. The game draws it always, where a long status line
    /// runs into it.
    fn paged(&self) -> bool {
        !self.rows.is_empty()
    }

    pub fn can_prev(&self) -> bool {
        self.page > 1
    }

    pub fn can_next(&self) -> bool {
        self.page * ROWS < self.rows.len() || self.more
    }

    pub fn prev(&mut self) {
        if self.can_prev() {
            self.page -= 1;
        }
    }

    /// The next page: one already here, else the server's (true: ask for it).
    pub fn next(&mut self) -> bool {
        if self.page * ROWS < self.rows.len() {
            self.page += 1;
            false
        } else if self.more {
            self.page += 1;
            true
        } else {
            false
        }
    }

    /// Typing into the box with the caret, up to 50 characters: prices take digits only.
    pub fn type_text(&mut self, text: &str) {
        let Some(field) = self.focus.filter(|&field| self.enabled(field)) else { return };
        let typed = text.chars().filter(|character| !character.is_control());
        let digits = matches!(field, Field::Min | Field::Max);
        let value = self.value_mut(field);
        for character in typed.filter(|character| !digits || character.is_ascii_digit()) {
            if value.chars().count() < MOST_TEXT {
                value.push(character);
            }
        }
    }

    pub fn backspace(&mut self) {
        if let Some(field) = self.focus.filter(|&field| self.enabled(field)) {
            self.value_mut(field).pop();
        }
    }

    /// Tab: the next box, in the order they stand.
    pub fn tab(&mut self) {
        self.focus = Some(match self.focus {
            Some(Field::Name) if self.enabled(Field::Card) => Field::Card,
            Some(Field::Card) => Field::Min,
            Some(Field::Min) => Field::Max,
            _ => Field::Name,
        });
    }

    fn value_mut(&mut self, field: Field) -> &mut String {
        match field {
            Field::Name => &mut self.name,
            Field::Card => &mut self.card,
            Field::Min => &mut self.min,
            Field::Max => &mut self.max,
        }
    }

    fn value(&self, field: Field) -> &str {
        match field {
            Field::Name => &self.name,
            Field::Card => &self.card,
            Field::Min => &self.min,
            Field::Max => &self.max,
        }
    }

    /// The rows on the page shown.
    fn shown(&self) -> std::ops::Range<usize> {
        let start = ((self.page - 1) * ROWS).min(self.rows.len());
        start..(start + ROWS).min(self.rows.len())
    }

    fn field_at(field: Field) -> [f32; 2] {
        let x = match field {
            Field::Name => 100.0,
            Field::Card => 187.0,
            Field::Min => 275.0,
            Field::Max => 364.0,
        };
        [x, FIELD_TOP]
    }

    fn kind_at(buying: bool) -> [f32; 2] {
        [KINDS[usize::from(buying)], KIND_TOP]
    }

    fn page_buttons() -> [[f32; 2]; 2] {
        [[WIDTH / 2.0 - PAGE_GAP - PAGE_BUTTON[0], FOOTER_TOP], [WIDTH / 2.0 + PAGE_GAP, FOOTER_TOP]]
    }

    fn row_top(place: usize) -> f32 {
        ROWS_TOP + place as f32 * ROW
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let local = self.frame.local(cursor, HEIGHT)?;
        if let Some(hit) = self.frame.title_hit(local) {
            // No minimize button here.
            return Some(Hit::Title(if hit == TitleHit::Minimize { TitleHit::Title } else { hit }));
        }
        for field in [Field::Name, Field::Card, Field::Min, Field::Max] {
            if contains(Self::field_at(field), FIELD_SIZE, local) {
                return Some(Hit::Field(field));
            }
        }
        for buying in [true, false] {
            if contains(Self::kind_at(buying), KIND_SIZE, local) {
                return Some(Hit::Kind(buying));
            }
        }
        if contains(SIMILAR_AT, SIMILAR_SIZE, local) {
            return Some(Hit::Similar);
        }
        if contains(SEARCH, SEARCH_SIZE, local) {
            return Some(Hit::Search);
        }
        let [prev, next] = Self::page_buttons();
        if self.paged() && contains(prev, PAGE_BUTTON, local) {
            return Some(Hit::Prev);
        }
        if self.paged() && contains(next, PAGE_BUTTON, local) {
            return Some(Hit::Next);
        }
        for (place, index) in self.shown().enumerate() {
            if contains([LIST[0], Self::row_top(place)], [LIST_WIDTH, ROW], local) {
                let column = match local[0] {
                    x if x < SHOP_CLICKS[0] => Column::Icon,
                    x if x < SHOP_CLICKS[1] => Column::Shop,
                    x if x < AMOUNT_COLUMN => Column::Item,
                    x if x < PRICE_COLUMN => Column::Amount,
                    _ => Column::Price,
                };
                return Some(Hit::Row(index, column));
            }
        }
        Some(Hit::Window)
    }

    /// A click on a row: a second one in a moment is a double click (true).
    pub fn click_row(&mut self, index: usize) -> bool {
        self.clicks.double(index as u16)
    }

    /// The rows on screen (for the test driver), each by item ID: its shop name, where a double click opens it.
    pub fn row_boxes(&self) -> Vec<(u32, [f32; 4])> {
        self.shown()
            .enumerate()
            .map(|(place, index)| {
                let [x, y] = self.frame.at([SHOP_CLICKS[0], Self::row_top(place)]);
                (self.rows[index].item, [x, y, SHOP_CLICKS[1] - SHOP_CLICKS[0], ROW])
            })
            .collect()
    }

    pub fn button_boxes(&self) -> Vec<(&'static str, [f32; 4])> {
        let place = |at: [f32; 2], size: [f32; 2]| {
            let [x, y] = self.frame.at(at);
            [x, y, size[0], size[1]]
        };
        let mut boxes = vec![
            ("catalog_search", place(SEARCH, SEARCH_SIZE)),
            ("catalog_close", place(self.frame.close_button(), BUTTON_SIZE)),
            ("catalog_sell", place(Self::kind_at(false), [KINDS[1] - KINDS[0], KIND_SIZE[1]])),
            ("catalog_buy", place(Self::kind_at(true), [Self::field_at(Field::Name)[0] - KINDS[1], KIND_SIZE[1]])),
            ("catalog_similar", place(SIMILAR_AT, SIMILAR_SIZE)),
        ];
        for (name, field) in [("catalog_name", Field::Name), ("catalog_card", Field::Card), ("catalog_min", Field::Min), ("catalog_max", Field::Max)] {
            boxes.push((name, place(Self::field_at(field), FIELD_SIZE)));
        }
        let [prev, next] = Self::page_buttons();
        if self.can_prev() {
            boxes.push(("catalog_prev", place(prev, PAGE_BUTTON)));
        }
        if self.can_next() {
            boxes.push(("catalog_next", place(next, PAGE_BUTTON)));
        }
        boxes
    }

    /// Item icons the rows show, to load before drawing.
    pub fn icons(&self, items: &HashMap<u32, ItemInfo>) -> Vec<String> {
        self.shown().filter_map(|index| items.get(&self.rows[index].item).map(|info| inventory::icon_path(info, true))).collect()
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
        font: &Font,
        (messages, cards, makers): (&[String], &inventory::CardNames, &HashMap<u32, Option<String>>),
        (pointer, down, caret): ([f32; 2], bool, bool),
    ) {
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        let at = |local: [f32; 2]| self.frame.at(local);
        let hit = self.hit(pointer);
        // The title bar with the base and close buttons (no minimize), over the body box.
        self.frame.draw_title_only(layer, chrome, message(TITLE));
        for (button, place) in [(0, BASE_BUTTON), (2, self.frame.close_button())] {
            let image = chrome.button(button, contains(at(place), BUTTON_SIZE, pointer));
            layer.sprites.push((image, at(place), image.size));
        }
        if self.frame.minimized {
            return;
        }
        layer.sprites.push((&chrome.white, at([0.0, TITLE_HEIGHT]), [WIDTH, HEIGHT - TITLE_HEIGHT]));
        for x in [0.0, WIDTH - 1.0] {
            layer.sprites.push((&chrome.border, at([x, TITLE_HEIGHT]), [1.0, HEIGHT - TITLE_HEIGHT]));
        }
        layer.sprites.push((&chrome.border, at([0.0, HEIGHT - 1.0]), [WIDTH, 1.0]));
        // ponytail: the labels are bold and a size smaller in the game; one font here, so the same size.
        for (label, x) in LABELS {
            for [dx, dy] in [[-1.0, 0.0], [1.0, 0.0], [0.0, -1.0], [0.0, 1.0]] {
                layer.texts.push((label.to_owned(), at([x + dx, LABEL_TOP + dy]), [255; 3]));
            }
            layer.texts.push((label.to_owned(), at([x, LABEL_TOP]), LABEL));
        }
        for (buying, label) in [(false, SELL), (true, BUY)] {
            let corner = Self::kind_at(buying);
            let radio = &assets.radio[usize::from(self.buying == buying)];
            layer.sprites.push((radio, at(corner), radio.size));
            layer.texts.push((message(label).to_owned(), at([corner[0] + 13.0, corner[1] - 1.0]), TEXT));
        }
        // The boxes: a grey line round light grey, darker while off; the caret in the one typed into.
        for field in [Field::Name, Field::Card, Field::Min, Field::Max] {
            let corner = Self::field_at(field);
            layer.sprites.push((&assets.grid, at(corner), FIELD_SIZE));
            let inside = &assets.field[usize::from(!self.enabled(field))];
            layer.sprites.push((inside, at([corner[0] + 1.0, corner[1] + 1.0]), [FIELD_SIZE[0] - 2.0, FIELD_SIZE[1] - 2.0]));
            let value = self.value(field);
            let shown = if self.focus == Some(field) && caret { format!("{value}|") } else { value.to_owned() };
            layer.texts.push((window::fit(font, &shown, FIELD_SIZE[0] - 4.0), at([corner[0] + 2.0, corner[1]]), TEXT));
        }
        layer.texts.push(("~".to_owned(), at(TILDE), TEXT));
        let check = &assets.check[usize::from(self.similar)];
        layer.sprites.push((check, at(SIMILAR_AT), check.size));
        layer.texts.push((message(SIMILAR).to_owned(), at([SIMILAR_AT[0] + check.size[0] + 3.0, SIMILAR_AT[1] - 1.0]), TEXT));
        let attempts = rustnarok_client::messages::fill(message(ATTEMPTS), &[&self.uses]);
        layer.texts.push((attempts.clone(), at([ATTEMPTS_RIGHT[0] - font.width(&attempts), ATTEMPTS_RIGHT[1] - 1.0]), TEXT));
        let search_look = match hit {
            Some(Hit::Search) if self.can_search() => 1 + usize::from(down),
            _ => 0,
        };
        layer.sprites.push((&assets.search[search_look], at(SEARCH), SEARCH_SIZE));
        // The list: a yellow header with its words, column lines, and a grey outline with cut corners round ten rows.
        layer.sprites.push((&assets.header, at(LIST), [LIST_WIDTH, HEADER_HEIGHT]));
        layer.sprites.push((&assets.grid, at([LIST[0], ROWS_TOP]), [LIST_WIDTH, 1.0]));
        let list_height = HEADER_HEIGHT + ROWS as f32 * ROW + 1.0;
        for (corner, size) in crate::overhead::border([LIST_WIDTH, list_height]) {
            layer.sprites.push((&assets.grid, at(offset(LIST, corner)), size));
        }
        let column_lines = |layer: &mut Layer<'a>, top: f32, height: f32| {
            for (x, width) in [(SHOP_COLUMN, ITEM_COLUMN - SHOP_COLUMN), (AMOUNT_COLUMN, PRICE_COLUMN - AMOUNT_COLUMN)] {
                for edge in [x, x + width - 1.0] {
                    layer.sprites.push((&assets.grid, at([edge, top]), [1.0, height]));
                }
            }
        };
        column_lines(layer, LIST[1], HEADER_HEIGHT + 1.0);
        let centred = |layer: &mut Layer<'a>, text: &str, left: f32, width: f32, top: f32, color: [u8; 3]| {
            layer.texts.push((text.to_owned(), at([left + ((width - font.width(text)) / 2.0).trunc(), top]), color));
        };
        for (label, left, right) in [
            (SHOP_NAME, SHOP_COLUMN, ITEM_COLUMN),
            (ITEM, ITEM_COLUMN, AMOUNT_COLUMN),
            (AMOUNT, AMOUNT_COLUMN, PRICE_COLUMN),
            (PRICE, PRICE_COLUMN, PRICE_RIGHT),
        ] {
            centred(layer, message(label), left, right - left, HEADER_TEXT, TEXT);
        }
        let pointed = match hit {
            Some(Hit::Row(index, _)) => Some(index),
            _ => None,
        };
        for (place, index) in self.shown().enumerate() {
            let row = &self.rows[index];
            let top = Self::row_top(place);
            // The row the pointer is on is the picked one.
            if pointed == Some(index) {
                layer.sprites.push((&assets.picked, at([LIST[0], top]), [LIST_WIDTH, ROW]));
            }
            column_lines(layer, top, ROW + 1.0);
            if place + 1 < ROWS {
                layer.sprites.push((&assets.grid, at([LIST[0], top + ROW]), [LIST_WIDTH, 1.0]));
            }
            if let Some(Some(icon)) = items.get(&row.item).map(|info| inventory::icon_path(info, true)).and_then(|path| icons.get(&path)) {
                layer.sprites.push((icon, at([ICON_X, top]), icon.size));
            }
            if let Some(grade) = assets.grades.get(usize::from(row.grade).wrapping_sub(1)) {
                layer.sprites.push((grade, at([ICON_X, top + GRADE_DOWN]), grade.size));
            }
            let text_top = top + ROW_TEXT;
            let shop_name = row.shop.split('\r').next().unwrap_or_default();
            layer.texts.push((window::fit(font, shop_name, SHOP_WIDTH), at([SHOP_X, text_top]), TEXT));
            let item = item(row);
            let name = inventory::full_name(items, cards, messages, makers, &item, true);
            layer.texts.push((window::fit(font, &name, ITEM_WIDTH), at([ITEM_X, text_top]), TEXT));
            centred(layer, &row.amount.to_string(), AMOUNT_COLUMN, PRICE_COLUMN - AMOUNT_COLUMN, text_top, TEXT);
            let price = format!("{} Z", shop::group(u64::from(row.price)));
            let left = PRICE_RIGHT - font.width(&price);
            let (color, over) = price_colors(row.price);
            layer.texts.push((price.clone(), at([left, text_top]), color));
            if let Some(over) = over {
                layer.texts.push((price, at([left - 1.0, text_top]), over));
            }
        }
        // Prev and Next either side of the page number; the status line in red at the left.
        if self.paged() {
            let [prev, next] = Self::page_buttons();
            let prev_text = message(PREV);
            for (text, corner, enabled, pointed) in [
                (prev_text, [prev[0] + PAGE_BUTTON[0] - font.width(prev_text), prev[1]], self.can_prev(), hit == Some(Hit::Prev)),
                (message(NEXT), next, self.can_next(), hit == Some(Hit::Next)),
            ] {
                let color = if enabled && pointed { PAGE_BUTTON_HOVER } else { PAGE_BUTTON_TEXT };
                layer.texts.push((text.to_owned(), at(corner), color));
            }
            centred(layer, &format!(" {} ", self.page), 0.0, WIDTH, STATUS[1], TEXT);
        }
        if let Some(status) = self.status_line() {
            layer.texts.push((message(status).to_owned(), at(STATUS), RED));
        }
        // At rest on a result: over the icon or the item, the item's whole name; over the shop, the shop's name; in the
        // item's colour, at the pointed cell of a 32 px grid (the game's list tooltips). Over the card box: what to type.
        if let Some(Hit::Row(index, _)) = hit {
            let [x, y] = [pointer[0] - self.frame.position[0], pointer[1] - self.frame.position[1]];
            let (row, item) = (&self.rows[index], item(&self.rows[index]));
            let text = match x.floor() as i32 {
                6..=49 | 161..=329 => Some(inventory::full_name(items, cards, messages, makers, &item, true)),
                51..=159 => Some(row.shop.split('\r').next().unwrap_or_default().to_owned()),
                _ => None,
            };
            if let Some(text) = text {
                window::tooltip(tooltip, chrome, font, &text, at(tip_place([x, y])), inventory::name_color(&item, items));
            }
        } else if hit == Some(Hit::Field(Field::Card)) {
            window::tooltip(tooltip, chrome, font, message(CARD_TIP), offset(pointer, [12.0, 12.0]), [255; 3]);
        }
    }
}

/// Where a list's tooltip goes for the pointer at `local`: the pointed cell's corner on a 32 px grid from (40, 17), a
/// pixel down.
fn tip_place([x, y]: [f32; 2]) -> [f32; 2] {
    [40.0 + 32.0 * ((x - 40.0) / 32.0).trunc(), 32.0 * ((y - 17.0) / 32.0).trunc() + 1.0]
}

/// A result's item, for its name and description.
pub fn item(row: &CatalogRow) -> InventoryItem {
    InventoryItem {
        id: row.item,
        kind: row.kind,
        amount: row.amount,
        identified: true,
        cards: row.cards,
        options: row.options.clone(),
        refine: row.refine,
        grade: row.grade,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describes_a_result_with_its_cards_and_options() {
        let option = rustnarok_client::net::ItemOption { id: 3, value: 2, param: 0 };
        let row = CatalogRow { item: 1201, kind: 5, cards: [4001, 0, 0, 0], options: vec![option], refine: 7, grade: 2, ..Default::default() };
        let item = item(&row);
        assert_eq!((item.id, item.kind, item.cards, item.refine, item.grade, item.identified), (1201, 5, [4001, 0, 0, 0], 7, 2, true));
        assert_eq!(item.options, row.options);
    }

    fn items() -> HashMap<u32, ItemInfo> {
        [(909, "Jellopy"), (910, "Garlet"), (7126, "Large Jellopy"), (501, "Red Potion"), (4001, "Poring Card"), (4002, "Fabre Card")]
            .into_iter()
            .map(|(id, name)| (id, ItemInfo { name: name.into(), ..Default::default() }))
            .collect()
    }

    fn cards() -> inventory::CardNames {
        inventory::CardNames::parse("4001#Lucky#\n4002#Vital#\n", "")
    }

    fn named(name: &str, card: &str, min: &str, max: &str) -> Catalog {
        let mut catalog = Catalog::new(false, 10);
        (catalog.name, catalog.card, catalog.min, catalog.max) = (name.into(), card.into(), min.into(), max.into());
        catalog
    }

    #[test]
    fn finds_names_holding_the_text_or_exactly_it_with_letter_case() {
        let (items, cards) = (items(), cards());
        assert_eq!(lookup("Jellopy", false, true, &items, &cards), [909, 7126]);
        assert_eq!(lookup("Jellopy", false, false, &items, &cards), [909]);
        assert!(lookup("jellopy", false, true, &items, &cards).is_empty());
        // A card by its name or its word; the card box finds cards alone.
        assert_eq!(lookup("Lucky", true, true, &items, &cards), [4001]);
        assert_eq!(lookup("Card", true, true, &items, &cards), [4001, 4002]);
        assert!(lookup("Jellopy", true, true, &items, &cards).is_empty());
    }

    #[test]
    fn checks_a_search_before_sending_it() {
        let (items, cards) = (items(), cards());
        let query = named("Jellopy", "Lucky", "", "5000").query(&items, &cards);
        assert_eq!(query, Ok(Query { items: vec![909, 7126], cards: vec![4001], max: 5000, min: 0 }));
        assert_eq!(named("", "", "", "").query(&items, &cards), Err(NO_NAME));
        assert_eq!(named("Emperium", "", "", "").query(&items, &cards), Err(NO_SUCH_ITEM));
        assert_eq!(named("Jellopy", "Garlet", "", "").query(&items, &cards), Err(NO_SUCH_CARD));
        // The least price over the most, a leading zero, or a number past an int: the price line.
        for (min, max) in [("500", "100"), ("0100", ""), ("", "3000000000")] {
            assert_eq!(named("Jellopy", "", min, max).query(&items, &cards), Err(BAD_PRICE));
        }
        assert!(named("Jellopy", "", "500", "").query(&items, &cards).is_ok());
        let many: HashMap<u32, ItemInfo> = (0..21).map(|id| (id, ItemInfo { name: format!("Box {id}"), ..Default::default() })).collect();
        assert_eq!(named("Box", "", "", "").query(&many, &cards), Err(TOO_MANY));
        // Exactly the name with "similar" off: one box of the 21.
        let mut exact = named("Box 7", "", "", "");
        exact.similar = false;
        assert_eq!(exact.query(&many, &cards).map(|query| query.items), Ok(vec![7]));
    }

    #[test]
    fn boxes_wait_for_an_item_name_and_hold_fifty_characters() {
        let mut catalog = Catalog::new(false, 10);
        catalog.focus = Some(Field::Card);
        catalog.type_text("Lucky");
        assert_eq!(catalog.card, "");
        catalog.focus = Some(Field::Name);
        catalog.tab();
        assert_eq!(catalog.focus, Some(Field::Name));
        catalog.type_text(&"x".repeat(60));
        assert_eq!(catalog.name.len(), 50);
        catalog.tab();
        assert_eq!(catalog.focus, Some(Field::Card));
        catalog.tab();
        catalog.type_text("12a3");
        assert_eq!(catalog.min, "123");
        catalog.tab();
        catalog.tab();
        assert_eq!(catalog.focus, Some(Field::Name));
    }

    #[test]
    fn pages_ten_rows_and_asks_the_server_past_them() {
        let row = |item| CatalogRow { item, ..Default::default() };
        let mut catalog = Catalog::new(false, 10);
        catalog.sent();
        assert_eq!(catalog.status_line(), Some(SEARCHING));
        catalog.page(true, true, 9, (0..10).map(row).collect());
        assert_eq!((catalog.searching, catalog.status_line(), catalog.can_prev(), catalog.can_next()), (false, None, false, true));
        assert!(catalog.next());
        catalog.page(false, false, 9, (10..13).map(row).collect());
        assert_eq!((catalog.page, catalog.shown()), (2, 10..13));
        assert!(!catalog.can_next());
        catalog.prev();
        assert!(!catalog.next());
        assert_eq!(catalog.page, 2);
        // A new search's first page goes back to page 1.
        catalog.page(true, false, 8, vec![row(501)]);
        assert_eq!((catalog.page, catalog.rows.len(), catalog.uses), (1, 1, 8));
    }

    #[test]
    fn failures_show_a_line_or_a_box() {
        let mut catalog = Catalog::new(false, 10);
        catalog.page(true, false, 10, vec![CatalogRow::default()]);
        catalog.sent();
        assert_eq!(catalog.failed(0), None);
        assert_eq!((catalog.status_line(), catalog.rows.len(), catalog.uses, catalog.searching), (Some(1803), 0, 9, false));
        assert_eq!(catalog.failed(1), None);
        assert_eq!(catalog.status, Some(TOO_MANY));
        assert_eq!([3, 4, 9].map(|reason| catalog.failed(reason)), [Some(1800), Some(1797), None]);
    }

    #[test]
    fn puts_row_tooltips_on_a_32_px_grid() {
        assert_eq!(tip_place([200.0, 130.0]), [200.0, 97.0]);
        assert_eq!(tip_place([20.0, 112.0]), [40.0, 65.0]);
    }

    #[test]
    fn colours_prices_by_their_size() {
        assert_eq!(price_colors(5), ([0, 255, 255], Some([0, 0, 0])));
        assert_eq!(price_colors(1_500), ([255, 255, 0], Some([255, 0, 0])));
        assert_eq!(price_colors(250_000), ([0, 0, 255], None));
        assert_eq!(price_colors(1_000_000_000), ([255, 0, 127], Some([255, 0, 0])));
    }
}
