//! The Universal Catalog's store search (ZC_OPEN_SEARCH_STORE_INFO): find an item in the map's vending or buying
//! stores by name and price, then see where a store stands or open it from afar.

use std::collections::HashMap;

use rustnarok_client::grf::Grf;
use rustnarok_client::lua::ItemInfo;
use rustnarok_client::net::CatalogRow;
use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};
use crate::window::{self, Chrome, Clicks, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI, contains, inventory, screen, shop};

const TITLE: usize = 1780;
const SELL: usize = 1778;
const BUY: usize = 1779;
const SHOP_NAME: usize = 1781;
const AMOUNT: usize = 1782;
const PRICE: usize = 1783;
const ATTEMPTS: usize = 1802;
pub const NO_NAME: usize = 1809;
pub const NO_SUCH_ITEM: usize = 1810;
pub const TOO_MANY: usize = 1784;
pub const TOO_EXPENSIVE: usize = 1804;
/// rathena drops a search over its `vending_max_value`.
const MOST_PRICE: u32 = 1_000_000_000;
/// The item count is a byte.
const MOST_ITEMS: usize = 255;

/// The line a failed search shows (ZC_SEARCH_STORE_INFO_FAILED's reason).
pub fn failure_message(reason: u8) -> usize {
    match reason {
        0 => 1803,
        1 => TOO_MANY,
        2 => 1798,
        3 => 1800,
        _ => 1797,
    }
}

/// The items whose identified name holds `text`, ignoring case, in ID order.
pub fn matching_items(text: &str, items: &HashMap<u32, ItemInfo>) -> Vec<u32> {
    let text = text.trim().to_lowercase();
    let mut ids: Vec<u32> = items.iter().filter(|(_, info)| info.name.to_lowercase().contains(&text)).map(|(&id, _)| id).collect();
    ids.sort_unstable();
    ids
}

/// What a search sends (the items, the max and min price), or the line to show instead.
pub fn search(name: &str, min: &str, max: &str, items: &HashMap<u32, ItemInfo>) -> Result<(Vec<u32>, u32, u32), usize> {
    if name.trim().is_empty() {
        return Err(NO_NAME);
    }
    let ids = matching_items(name, items);
    if ids.is_empty() {
        return Err(NO_SUCH_ITEM);
    }
    if ids.len() > MOST_ITEMS {
        return Err(TOO_MANY);
    }
    let price = |text: &str| text.trim().parse::<u64>().unwrap_or(0);
    let (min, max) = (price(min), price(max));
    if min > u64::from(MOST_PRICE) || max > u64::from(MOST_PRICE) {
        return Err(TOO_EXPENSIVE);
    }
    Ok((ids, max as u32, min as u32))
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Field {
    Name,
    Min,
    Max,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(TitleHit),
    /// The store kind: true buying stores.
    Kind(bool),
    Field(Field),
    Search,
    Next,
    Close,
    Row(usize),
    Window,
}

// ponytail: data.grf has no background for this window; the chrome, the kind and the fields on top, the results under
// them and the buttons below. Which word goes with which kind is an estimate too.
const WIDTH: f32 = 440.0;
const KIND_TOP: f32 = TITLE_HEIGHT + 6.0;
const NAME_TOP: f32 = KIND_TOP + 22.0;
const PRICE_TOP: f32 = NAME_TOP + 24.0;
const FIELD_HEIGHT: f32 = 18.0;
const NAME_FIELD: [f32; 2] = [10.0, 300.0];
const MIN_FIELD: [f32; 2] = [60.0, 110.0];
const MAX_FIELD: [f32; 2] = [190.0, 110.0];
const SEARCH: [f32; 2] = [WIDTH - 66.0, NAME_TOP];
const SEARCH_SIZE: [f32; 2] = [56.0, 34.0];
const HEADER_TOP: f32 = PRICE_TOP + 30.0;
const ROW: f32 = 26.0;
const ROWS_TOP: f32 = HEADER_TOP + 18.0;
const ROWS: usize = 10;
const FOOTER_TOP: f32 = ROWS_TOP + ROWS as f32 * ROW + 4.0;
const HEIGHT: f32 = FOOTER_TOP + 28.0;
const NEXT: [f32; 2] = [WIDTH - 110.0, FOOTER_TOP + 4.0];
const NEXT_SIZE: [f32; 2] = [40.0, 20.0];
const CLOSE: [f32; 2] = [WIDTH - 60.0, FOOTER_TOP + 4.0];
const CLOSE_SIZE: [f32; 2] = [42.0, 20.0];
const ITEM_X: f32 = 38.0;
const SHOP_X: f32 = 200.0;
const AMOUNT_RIGHT: f32 = 360.0;
const PRICE_RIGHT: f32 = WIDTH - 10.0;
const TEXT: [u8; 3] = [0, 0, 0];
const GREY: [u8; 3] = [0x80, 0x80, 0x80];

pub struct Assets {
    radio: [Texture; 2],
    search: [Texture; 2],
    next: Texture,
    close: [Texture; 3],
    picked: Texture,
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |path: String| gpu.load_texture(grf, &path, false);
        let basic = format!("{UI}basic_interface\\");
        Self {
            radio: ["radiobtn_off", "radiobtn_on"].map(|name| image(format!("{UI}{name}.bmp"))),
            search: ["검색a", "검색b"].map(|name| image(format!("{basic}{name}.bmp"))),
            next: image(format!("{basic}arw_right.bmp")),
            close: ["", "_a", "_b"].map(|look| image(format!("{UI}btn_close{look}.bmp"))),
            picked: gpu.solid([0xCE, 0xDE, 0xF7, 0xFF]),
        }
    }
}

/// The search window: what's typed, the results so far, and where a store found stands.
pub struct Catalog {
    pub frame: Frame,
    /// Open a store found (effect 1), else show where it stands.
    pub remote: bool,
    pub uses: u8,
    pub buying: bool,
    pub name: String,
    pub min: String,
    pub max: String,
    pub focus: Option<Field>,
    pub rows: Vec<CatalogRow>,
    pub more: bool,
    pub picked: Option<usize>,
    pub mark: Option<[i16; 2]>,
    clicks: Clicks,
}

impl Catalog {
    pub fn new(remote: bool, uses: u8) -> Self {
        let mut frame = Frame::new([((screen()[0] - WIDTH) / 2.0).floor(), 90.0], WIDTH);
        frame.open = true;
        Self {
            frame,
            remote,
            uses,
            buying: false,
            name: String::new(),
            min: String::new(),
            max: String::new(),
            focus: Some(Field::Name),
            rows: Vec::new(),
            more: false,
            picked: None,
            mark: None,
            clicks: Clicks::default(),
        }
    }

    /// A page of results: the first replaces the list, a later one adds to it.
    pub fn page(&mut self, first: bool, more: bool, uses: u8, rows: Vec<CatalogRow>) {
        if first {
            self.rows = rows;
            self.picked = None;
        } else {
            self.rows.extend(rows);
        }
        self.more = more;
        self.uses = uses;
    }

    /// Typing into the field with the caret: prices take digits only.
    pub fn type_text(&mut self, text: &str) {
        let typed = text.chars().filter(|character| !character.is_control());
        match self.focus {
            Some(Field::Name) => self.name.extend(typed),
            Some(Field::Min) => self.min.extend(typed.filter(char::is_ascii_digit)),
            Some(Field::Max) => self.max.extend(typed.filter(char::is_ascii_digit)),
            None => {}
        }
    }

    pub fn backspace(&mut self) {
        match self.focus {
            Some(Field::Name) => self.name.pop(),
            Some(Field::Min) => self.min.pop(),
            Some(Field::Max) => self.max.pop(),
            None => None,
        };
    }

    /// The rows showing: the last ten pages' worth fit; the list shows its first `ROWS` and the newest after paging.
    fn shown(&self) -> std::ops::Range<usize> {
        let start = self.rows.len().saturating_sub(ROWS);
        start..self.rows.len()
    }

    fn field_box(field: Field) -> ([f32; 2], f32) {
        match field {
            Field::Name => ([NAME_FIELD[0], NAME_TOP], NAME_FIELD[1]),
            Field::Min => ([MIN_FIELD[0], PRICE_TOP], MIN_FIELD[1]),
            Field::Max => ([MAX_FIELD[0], PRICE_TOP], MAX_FIELD[1]),
        }
    }

    fn kind_at(buying: bool) -> [f32; 2] {
        [10.0 + if buying { 90.0 } else { 0.0 }, KIND_TOP]
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let local = self.frame.local(cursor, HEIGHT)?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(Hit::Title(hit));
        }
        for buying in [false, true] {
            if contains(Self::kind_at(buying), [80.0, 18.0], local) {
                return Some(Hit::Kind(buying));
            }
        }
        for field in [Field::Name, Field::Min, Field::Max] {
            let (at, width) = Self::field_box(field);
            if contains(at, [width, FIELD_HEIGHT], local) {
                return Some(Hit::Field(field));
            }
        }
        if contains(SEARCH, SEARCH_SIZE, local) {
            return Some(Hit::Search);
        }
        if self.more && contains(NEXT, NEXT_SIZE, local) {
            return Some(Hit::Next);
        }
        if contains(CLOSE, CLOSE_SIZE, local) {
            return Some(Hit::Close);
        }
        for (place, index) in self.shown().enumerate() {
            if contains([4.0, ROWS_TOP + place as f32 * ROW], [WIDTH - 8.0, ROW], local) {
                return Some(Hit::Row(index));
            }
        }
        Some(Hit::Window)
    }

    /// A click on a row picks it; a second one in a moment is a double click (true).
    pub fn click_row(&mut self, index: usize) -> bool {
        self.picked = Some(index);
        self.clicks.double(index as u16)
    }

    /// The rows and buttons on screen (for the test driver): rows by item ID, buttons by name.
    pub fn row_boxes(&self) -> Vec<(u32, [f32; 4])> {
        self.shown()
            .enumerate()
            .map(|(place, index)| {
                let [x, y] = self.frame.at([4.0, ROWS_TOP + place as f32 * ROW]);
                (self.rows[index].item, [x, y, WIDTH - 8.0, ROW])
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
            ("catalog_close", place(CLOSE, CLOSE_SIZE)),
            ("catalog_sell", place(Self::kind_at(false), [80.0, 18.0])),
            ("catalog_buy", place(Self::kind_at(true), [80.0, 18.0])),
        ];
        for (name, field) in [("catalog_name", Field::Name), ("catalog_min", Field::Min), ("catalog_max", Field::Max)] {
            let (at, width) = Self::field_box(field);
            boxes.push((name, place(at, [width, FIELD_HEIGHT])));
        }
        if self.more {
            boxes.push(("catalog_next", place(NEXT, NEXT_SIZE)));
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
        chrome: &'a Chrome,
        assets: &'a Assets,
        icons: &'a HashMap<String, Option<Texture>>,
        items: &HashMap<u32, ItemInfo>,
        font: &Font,
        messages: &[String],
        (pointer, down, caret): ([f32; 2], bool, bool),
    ) {
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        let at = |local: [f32; 2]| self.frame.at(local);
        self.frame.draw_title(layer, chrome, message(TITLE), pointer);
        if self.frame.minimized {
            return;
        }
        layer.sprites.push((&chrome.white, at([0.0, TITLE_HEIGHT]), [WIDTH, HEIGHT - TITLE_HEIGHT]));
        for x in [0.0, WIDTH - 1.0] {
            layer.sprites.push((&chrome.border, at([x, TITLE_HEIGHT]), [1.0, HEIGHT - TITLE_HEIGHT]));
        }
        layer.sprites.push((&chrome.border, at([0.0, HEIGHT - 1.0]), [WIDTH, 1.0]));
        for (buying, label) in [(false, SELL), (true, BUY)] {
            let corner = Self::kind_at(buying);
            let radio = &assets.radio[usize::from(self.buying == buying)];
            layer.sprites.push((radio, at([corner[0], corner[1] + 2.0]), radio.size));
            layer.texts.push((message(label).to_owned(), at([corner[0] + radio.size[0] + 4.0, corner[1]]), TEXT));
        }
        layer.texts.push((rustnarok_client::messages::fill(message(ATTEMPTS), &[&self.uses]), at([220.0, KIND_TOP]), TEXT));
        // The fields: a white box in a grey line; the caret in the one typed into.
        for (field, value) in [(Field::Name, &self.name), (Field::Min, &self.min), (Field::Max, &self.max)] {
            let (corner, width) = Self::field_box(field);
            layer.sprites.push((&chrome.border, at([corner[0] - 1.0, corner[1] - 1.0]), [width + 2.0, FIELD_HEIGHT + 2.0]));
            layer.sprites.push((&chrome.white, at(corner), [width, FIELD_HEIGHT]));
            let shown = if self.focus == Some(field) && caret { format!("{value}|") } else { value.clone() };
            layer.texts.push((window::fit(font, &shown, width - 6.0), at([corner[0] + 3.0, corner[1] + 2.0]), TEXT));
        }
        layer.texts.push((message(PRICE).to_owned(), at([10.0, PRICE_TOP + 2.0]), TEXT));
        layer.texts.push(("~".to_owned(), at([MIN_FIELD[0] + MIN_FIELD[1] + 7.0, PRICE_TOP + 2.0]), TEXT));
        let search_look = usize::from(self.hit(pointer) == Some(Hit::Search) && down);
        layer.sprites.push((&assets.search[search_look], at(SEARCH), SEARCH_SIZE));
        // The results: item, shop, amount, price.
        layer.texts.push((message(SHOP_NAME).to_owned(), at([SHOP_X, HEADER_TOP]), GREY));
        let amount = message(AMOUNT);
        layer.texts.push((amount.to_owned(), at([AMOUNT_RIGHT - font.width(amount), HEADER_TOP]), GREY));
        let price = message(PRICE);
        layer.texts.push((price.to_owned(), at([PRICE_RIGHT - font.width(price), HEADER_TOP]), GREY));
        for (place, index) in self.shown().enumerate() {
            let row = &self.rows[index];
            let top = ROWS_TOP + place as f32 * ROW;
            if self.picked == Some(index) {
                layer.sprites.push((&assets.picked, at([4.0, top]), [WIDTH - 8.0, ROW]));
            }
            if let Some(Some(icon)) = items.get(&row.item).map(|info| inventory::icon_path(info, true)).and_then(|path| icons.get(&path)) {
                layer.sprites.push((icon, at([8.0, top + 1.0]), icon.size));
            }
            let name = inventory::item_name(items, row.item, true, row.refine);
            layer.texts.push((window::fit(font, &name, SHOP_X - ITEM_X - 6.0), at([ITEM_X, top + 6.0]), TEXT));
            layer.texts.push((window::fit(font, &row.shop, AMOUNT_RIGHT - SHOP_X - 40.0), at([SHOP_X, top + 6.0]), TEXT));
            let amount = row.amount.to_string();
            layer.texts.push((amount.clone(), at([AMOUNT_RIGHT - font.width(&amount), top + 6.0]), TEXT));
            let price = shop::group(u64::from(row.price));
            layer.texts.push((price.clone(), at([PRICE_RIGHT - font.width(&price), top + 6.0]), TEXT));
        }
        if self.more {
            layer.sprites.push((&assets.next, at([NEXT[0] + 13.0, NEXT[1] + 3.0]), assets.next.size));
        }
        let close_look = match self.hit(pointer) {
            Some(Hit::Close) => 1 + usize::from(down),
            _ => 0,
        };
        layer.sprites.push((&assets.close[close_look], at(CLOSE), CLOSE_SIZE));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items() -> HashMap<u32, ItemInfo> {
        [(909, "Jellopy"), (910, "Garlet"), (7126, "Large Jellopy"), (501, "Red Potion")]
            .into_iter()
            .map(|(id, name)| (id, ItemInfo { name: name.into(), ..Default::default() }))
            .collect()
    }

    #[test]
    fn turns_a_name_into_items_and_checks_it() {
        let items = items();
        assert_eq!(matching_items("jellopy", &items), [909, 7126]);
        assert_eq!(search("Jellopy", "", "5000", &items), Ok((vec![909, 7126], 5000, 0)));
        assert_eq!(search("  ", "", "", &items), Err(NO_NAME));
        assert_eq!(search("Emperium", "", "", &items), Err(NO_SUCH_ITEM));
        assert_eq!(search("Red", "", "2000000000", &items), Err(TOO_EXPENSIVE));
        let many: HashMap<u32, ItemInfo> = (0..300).map(|id| (id, ItemInfo { name: format!("Box {id}"), ..Default::default() })).collect();
        assert_eq!(search("box", "", "", &many), Err(TOO_MANY));
        assert_eq!([0, 1, 2, 3, 4].map(failure_message), [1803, 1784, 1798, 1800, 1797]);
    }

    #[test]
    fn pages_replace_then_add() {
        let row = |item| CatalogRow { item, ..Default::default() };
        let mut catalog = Catalog::new(false, 10);
        catalog.page(true, true, 9, vec![row(909), row(910)]);
        catalog.page(false, false, 9, vec![row(911)]);
        assert_eq!(catalog.rows.iter().map(|row| row.item).collect::<Vec<_>>(), [909, 910, 911]);
        assert!(!catalog.more);
        catalog.page(true, false, 8, vec![row(501)]);
        assert_eq!((catalog.rows.len(), catalog.uses), (1, 8));
        // Prices take digits only.
        catalog.focus = Some(Field::Max);
        catalog.type_text("12a3");
        assert_eq!(catalog.max, "123");
    }
}
