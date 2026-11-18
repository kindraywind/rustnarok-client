//! The style shop (`@stylist`, the stylist NPCs): hair style and colour, clothes colour and a few headgears, tried on
//! a figure and bought together. The window is its picture, `bg_styling.bmp` (480 x 330): tabs down the grey left
//! column, the figure with its arrows, the hair colours under it, and at the right the list of what the picks cost.

use std::collections::{BTreeMap, HashMap};

use rustnarok_client::grf::Grf;
use rustnarok_client::lua::{ItemInfo, LookTables, StyleRow, StylingShop};
use rustnarok_client::net::StylePicks;
use rustnarok_client::text::Font;

use crate::actor::{Look, Placed, SpriteSet, Timing};
use crate::render::{Gpu, Texture};
use crate::shop::group;
use crate::window::{self, BUTTON_SIZE, Chrome, Frame, TitleHit};
use crate::{Layer, UI, contains, dressroom, inventory, offset};

/// ZC_UI_OPEN's kind for the stylist. Kind 8 is grade enchanting, which this window doesn't open.
pub const UI_KIND: u8 = 1;
pub const NO_COUPON: usize = 2814;
const OPEN_THE_BOX: usize = 2815;
/// "Item cannot be restored, are you sure want to apply it?", asked before a buy.
pub const ASKS_BUY: usize = 2818;
/// A hair style the table says cannot be dyed.
pub const CANT_DYE: usize = 2834;
const HINT: usize = 2835;
pub const DONE: usize = 2836;
/// "failed": a style change the server refused.
pub const FAILED: usize = 1575;
const ON_HAND: usize = 2773;
const DYE_REMOVAL: usize = 3912;
/// The tabs' names, down the column: Basic Style, Accessory, Second Costume, Wardrobe.
const TAB_NAMES: [usize; 4] = [2837, 2831, 2832, 2833];

/// Jobs that use the Doram hair styles and clothes colors (rathena's MAPID_SUMMONER).
const DORAM_JOBS: [u16; 3] = [4218, 4220, 4308];

/// Hair colour 0, the dye taken off: no row of the table but row -1 of the server's.
pub const NO_DYE: u16 = 0xFFFF;

// Every place is from the picture's top-left corner; the picture brings its own title bar.
const SIZE: [f32; 2] = [480.0, 330.0];
pub const HEIGHT: f32 = SIZE[1];
const TABS: [f32; 2] = [3.0, 21.0];
const TAB_SIZE: [f32; 2] = [37.0, 40.0];
const TAB_STEP: f32 = 40.0;
/// The figure's feet and its shadow; beside it the arrows that step the hair style and, under those, the clothes
/// colour; the button that drops every pick; the two that turn the figure.
const FEET: [f32; 2] = [169.0, 210.0];
const SHADOW: [f32; 2] = [141.0, 198.0];
const STYLE_ARROWS: [[f32; 2]; 2] = [[59.0, 107.0], [279.0, 107.0]];
const CLOTHES_ARROWS: [[f32; 2]; 2] = [[59.0, 134.0], [279.0, 134.0]];
const ARROW_SIZE: [f32; 2] = [8.0, 12.0];
const RESET: [f32; 2] = [55.0, 210.0];
const RESET_SIZE: [f32; 2] = [16.0, 16.0];
const TURNS: [([f32; 2], [f32; 2]); 2] = [([148.0, 229.0], [21.0, 9.0]), ([169.0, 229.0], [20.0, 9.0])];
const BUY: [f32; 2] = [396.0, 306.0];
const BUY_SIZE: [f32; 2] = [80.0, 20.0];
const ZENY_LINE: [f32; 2] = [9.0, 310.0];
/// The dotted line between the figure and the right side.
const DOTTED: [f32; 2] = [297.0, 22.0];
/// The hair colours: their box, the chips four to a row, and the one that takes the dye off.
const COLOR_BOX: [f32; 2] = [119.0, 250.0];
const CHIPS: [f32; 2] = [156.0, 254.0];
const CHIP_SIZE: [f32; 2] = [14.0, 14.0];
const CHIP_STEP: f32 = 18.0;
const CHIPS_ACROSS: usize = 4;
const NO_DYE_CHIP: [f32; 2] = [125.0, 273.0];
const NO_DYE_SIZE: [f32; 2] = [16.0, 16.0];
/// The list of what will be bought: eight rows 35 px apart, each a mark, an icon 8 px higher and a text, over a rule.
const LIST: [f32; 2] = [310.0, 36.0];
const LIST_STEP: f32 = 35.0;
const LIST_ROWS: usize = 8;
const MARK_SIZE: [f32; 2] = [17.0, 13.0];
const LIST_ICON: [f32; 2] = [21.0, -8.0];
const LIST_TEXT: f32 = 48.0;
const LIST_RULE: f32 = 17.0;
const HINT_WIDTH: f32 = 178.0;
/// The headgears' rows on the accessory tab, as wide as the window leaves them.
const ACCESSORY_AT: [f32; 2] = [312.0, 30.0];
const ACCESSORY_ROW: [f32; 2] = [164.0, 16.0];
const BLACK: [u8; 3] = [0, 0, 0];
const WHITE: [u8; 3] = [255, 255, 255];
/// A row that can be paid, and the zeny line; one that can't, struck through.
const BROWN: [u8; 3] = [0x84, 0x5A, 0x5A];
const GREY: [u8; 3] = [0xBF, 0xBF, 0xBF];

/// What the player has picked. 0 leaves that part as it is. Body style is never picked: this version sends 0.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Picks {
    pub hair_color: u16,
    pub hair_style: u16,
    pub clothes_color: u16,
    pub head_top: u16,
    pub head_mid: u16,
    pub head_bottom: u16,
}

impl Picks {
    /// The request. Body style stays 0: the second-costume tab is off.
    pub fn request(self) -> StylePicks {
        StylePicks {
            hair_color: self.hair_color,
            hair_style: self.hair_style,
            clothes_color: self.clothes_color,
            head_top: self.head_top,
            head_mid: self.head_mid,
            head_bottom: self.head_bottom,
            body_style: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Hair,
    Accessory,
}

pub fn is_doram(job: u16) -> bool {
    DORAM_JOBS.contains(&job)
}

fn styles(shop: &StylingShop, job: u16) -> &BTreeMap<u16, StyleRow> {
    if is_doram(job) { &shop.doram_head_style } else { &shop.head_style }
}

fn clothes(shop: &StylingShop, job: u16) -> &BTreeMap<u16, StyleRow> {
    if is_doram(job) { &shop.doram_body_palette } else { &shop.body_palette }
}

fn row(rows: &BTreeMap<u16, StyleRow>, index: u16) -> Option<&StyleRow> {
    (index != 0).then(|| rows.get(&index)).flatten()
}

/// The preview: the real look with the picks put in. Headgear's view is the item's ClassNum. The real look is left as it was.
pub fn preview_look(real: &Look, shop: &StylingShop, picks: Picks, class_num: impl Fn(u32) -> u16) -> Look {
    let mut look = real.clone();
    if picks.hair_color == NO_DYE {
        look.hair_color = 0;
    } else if let Some(row) = row(&shop.head_palette, picks.hair_color) {
        look.hair_color = row.goods as u16;
    }
    if let Some(row) = row(styles(shop, real.job), picks.hair_style) {
        look.hair = row.goods as u16;
    }
    if let Some(row) = row(clothes(shop, real.job), picks.clothes_color) {
        look.clothes_color = row.goods as u16;
    }
    if let Some(row) = row(&shop.top_accessory, picks.head_top) {
        look.head_top = class_num(row.goods);
    }
    if let Some(row) = row(&shop.mid_accessory, picks.head_mid) {
        look.head_mid = class_num(row.goods);
    }
    if let Some(row) = row(&shop.bottom_accessory, picks.head_bottom) {
        look.head_bottom = class_num(row.goods);
    }
    look
}

/// The part of the look a row of the list changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    HairColor,
    HairStyle,
    ClothesColor,
    HeadTop,
    HeadMid,
    HeadBottom,
}

/// What a row of the list is paid with: zeny, a coupon (the item, and the box that holds one), or nothing (the dye
/// taken off).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pay {
    Zeny(u32),
    Coupon(u32, u32),
    Free,
}

/// A row of the list of what will be bought: `goods` is the look it gives (a headgear's item for its icon).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ListRow {
    pub part: Part,
    pub pay: Pay,
    pub goods: u32,
    pub payable: bool,
}

/// The list of what the picks will buy, a row a part, in the order they are charged: hair colour, hair style,
/// clothes colour, then the three headgears. A row can be paid while the zeny carried covers it after the rows above,
/// and a coupon row while a coupon (or its box) is left for it: two rows on one coupon need two.
pub fn list(shop: &StylingShop, job: u16, picks: Picks, zeny: i64, has: impl Fn(u32) -> u32) -> Vec<ListRow> {
    let parts = [
        (Part::HairColor, &shop.head_palette, picks.hair_color),
        (Part::HairStyle, styles(shop, job), picks.hair_style),
        (Part::ClothesColor, clothes(shop, job), picks.clothes_color),
        (Part::HeadTop, &shop.top_accessory, picks.head_top),
        (Part::HeadMid, &shop.mid_accessory, picks.head_mid),
        (Part::HeadBottom, &shop.bottom_accessory, picks.head_bottom),
    ];
    let mut spent = 0i64;
    let mut used: HashMap<(u32, u32), u32> = HashMap::new();
    let mut rows = Vec::new();
    for (part, table, index) in parts {
        if part == Part::HairColor && index == NO_DYE {
            rows.push(ListRow { part, pay: Pay::Free, goods: 0, payable: true });
            continue;
        }
        let Some(entry) = row(table, index) else { continue };
        spent += i64::from(entry.zeny);
        let mut payable = entry.zeny == 0 || spent <= zeny;
        let pay = if entry.item != 0 || entry.box_item != 0 {
            let coupon = (entry.item, entry.box_item);
            let have = has(entry.item) * u32::from(entry.item != 0) + has(entry.box_item) * u32::from(entry.box_item != 0 && entry.box_item != entry.item);
            let taken = used.entry(coupon).or_default();
            payable &= *taken < have;
            *taken += 1;
            Pay::Coupon(entry.item, entry.box_item)
        } else {
            Pay::Zeny(entry.zeny)
        };
        rows.push(ListRow { part, pay, goods: entry.goods, payable });
    }
    rows
}

/// Whether Buy is live: something is picked and every row of the list can be paid.
pub fn can_buy(rows: &[ListRow]) -> bool {
    !rows.is_empty() && rows.iter().all(|row| row.payable)
}

/// The row a step lands on from where the figure is in `rows` (its picked row, else the row of what it wears):
/// the next or the one before, wrapping. 0 when that is what the player already wears: nothing to buy.
fn stepped(rows: &BTreeMap<u16, StyleRow>, pick: u16, worn: u32, forward: bool) -> u16 {
    let keys: Vec<u16> = rows.keys().copied().collect();
    let Some(&last) = keys.last() else { return 0 };
    let at = keys.iter().position(|&key| key == pick).or_else(|| keys.iter().position(|key| rows[key].goods == worn));
    let key = match at {
        Some(at) => keys[dressroom::step(at, keys.len(), forward)],
        None if forward => keys[0],
        None => last,
    };
    if rows[&key].goods == worn { 0 } else { key }
}

/// Whether the hair the figure shows can be dyed: the picked style's row, else the worn style's, says so.
fn can_dye(shop: &StylingShop, real: &Look, picks: Picks) -> bool {
    let rows = styles(shop, real.job);
    let style = row(rows, picks.hair_style).or_else(|| rows.values().find(|row| row.goods == u32::from(real.hair)));
    style.and_then(|row| row.is_coloring) != Some(false)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hit {
    Title(TitleHit),
    Tab(Tab),
    /// A tab that is drawn and does nothing (second costume, wardrobe), by its place down the column.
    DeadTab(usize),
    /// The arrows beside the figure: the hair style or the clothes colour, forward or back.
    Style(bool),
    Clothes(bool),
    HairColor(u16),
    HeadTop(u16),
    HeadMid(u16),
    HeadBottom(u16),
    /// A row's mark in the list.
    Mark(usize),
    Buy,
    Refresh,
    Rotate(i8),
    Window,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    /// Ask before buying these.
    Ask(StylePicks),
    /// Tell a message in a box.
    Notice(usize),
    Close,
}

pub struct Assets {
    background: Texture,
    /// Hair, accessories, second costume, wardrobe: normal, under the pointer, selected.
    tabs: [[Texture; 3]; 4],
    /// The arrows beside the figure, left and right: normal, under the pointer.
    arrows: [[Texture; 2]; 2],
    /// Normal, under the pointer, held down; Buy also switched off.
    buy: [Texture; 4],
    refresh: [Texture; 3],
    rotate: [[Texture; 3]; 2],
    /// color01 .. color08: normal, under the pointer, picked. A missing bitmap is a flat stand-in.
    colors: Vec<[Texture; 3]>,
    /// The chip that takes the dye off: normal, picked.
    no_dye: [Texture; 2],
    color_box: Texture,
    shadow: Texture,
    dotted: Texture,
    rule: Texture,
    /// A list row's box, and the mark over it: can be paid, can't.
    mark_box: Texture,
    marks: [Texture; 2],
    zeny: Texture,
    no_color: Texture,
    selected: Texture,
    strike: Texture,
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |name: &str, fallback: [u8; 4]| {
            [format!("{UI}styleshop\\{name}.bmp"), format!("{UI}stylingshop\\{name}.bmp")]
                .into_iter()
                .find_map(|path| gpu.try_load_texture(grf, &path, false).ok())
                .unwrap_or_else(|| gpu.solid(fallback))
        };
        let grey = [200, 200, 200, 255];
        // ponytail: stand-in chips when the bitmap isn't in this data.grf.
        let chip = |index: usize| {
            let shade = (40 + index * 24) as u8;
            [shade, 80u8.saturating_add(shade / 2), 200u8.saturating_sub(shade), 255]
        };
        Self {
            background: image("bg_styling", [0xE4, 0xD8, 0xC4, 255]),
            tabs: [
                ["tab_hair_off", "tab_hair_off", "tab_hair_on"],
                ["tab_accessories_off", "tab_accessories_off_over", "tab_accessories_on"],
                ["tab_2ndcostume_off"; 3],
                ["tab_favorite_off"; 3],
            ]
            .map(|names| names.map(|name| image(name, [180, 180, 190, 255]))),
            arrows: ["l", "r"].map(|side| ["out", "over"].map(|look| image(&format!("arrow_chr_{side}_upper_{look}"), grey))),
            buy: ["out", "over", "down", "disable"].map(|look| image(&format!("btn_buy_{look}"), [80, 180, 80, 255])),
            refresh: ["out", "over", "press"].map(|look| image(&format!("btn_refresh_{look}"), grey)),
            rotate: ["l", "r"].map(|side| ["out", "over", "press"].map(|look| image(&format!("btn_rotatearrow_{side}_{look}"), grey))),
            colors: (1..=8).map(|index| ["off", "off_over", "on"].map(|look| image(&format!("color{index:02}_{look}"), chip(index)))).collect(),
            no_dye: ["off", "on"].map(|look| image(&format!("color00_{look}"), grey)),
            color_box: image("bg_styling_color", [0xF0, 0xF0, 0xF0, 255]),
            shadow: image("shadow_chr_off", [0, 0, 0, 80]),
            dotted: image("line_dotted_2px", grey),
            rule: image("line_dotted_1px_light", grey),
            mark_box: image("checkbox_off", [255; 4]),
            marks: ["check_able", "notcheck"].map(|name| image(name, grey)),
            zeny: image("icon_zeny", [240, 200, 40, 255]),
            no_color: image("icon_color_off", grey),
            selected: image("select_mark", [255, 220, 80, 180]),
            strike: gpu.solid([GREY[0], GREY[1], GREY[2], 255]),
        }
    }
}

pub struct Window {
    pub frame: Frame,
    tab: Tab,
    pub picks: Picks,
    direction: u8,
    figure: Option<SpriteSet>,
    shown: Option<Look>,
}

fn tab_at(tab: usize) -> [f32; 2] {
    [TABS[0], TABS[1] + tab as f32 * TAB_STEP]
}

/// Hair colour chip `n` (from 0), four to a row.
fn chip_at(n: usize) -> [f32; 2] {
    [CHIPS[0] + (n % CHIPS_ACROSS) as f32 * CHIP_STEP, CHIPS[1] + (n / CHIPS_ACROSS) as f32 * CHIP_STEP]
}

/// The list's row `i`: where its mark is.
fn list_at(row: usize) -> [f32; 2] {
    [LIST[0], LIST[1] + row as f32 * LIST_STEP]
}

fn accessory_at(index: usize) -> [f32; 2] {
    [ACCESSORY_AT[0], ACCESSORY_AT[1] + index as f32 * ACCESSORY_ROW[1]]
}

/// The hair colours with a chip: the table's rows in order.
fn chips(shop: &StylingShop) -> impl Iterator<Item = (usize, u16, &StyleRow)> {
    shop.head_palette.iter().filter(|(row, _)| **row != NO_DYE).enumerate().map(|(n, (row, entry))| (n, *row, entry))
}

/// The headgears' rows on the accessory tab: upper, middle, then lower, each with the hit a press on it makes.
fn accessories(shop: &StylingShop) -> impl Iterator<Item = (usize, Hit, &StyleRow)> {
    let top = shop.top_accessory.iter().map(|(row, entry)| (Hit::HeadTop(*row), entry));
    let mid = shop.mid_accessory.iter().map(|(row, entry)| (Hit::HeadMid(*row), entry));
    let bottom = shop.bottom_accessory.iter().map(|(row, entry)| (Hit::HeadBottom(*row), entry));
    top.chain(mid).chain(bottom).enumerate().map(|(index, (hit, entry))| (index, hit, entry))
}

impl Window {
    /// In the middle of the screen.
    pub fn open() -> Self {
        let screen = crate::screen();
        let mut frame = Frame::new([((screen[0] - SIZE[0]) / 2.0).floor(), ((screen[1] - SIZE[1]) / 2.0).floor()], SIZE[0]);
        frame.open = true;
        Self { frame, tab: Tab::Hair, picks: Picks::default(), direction: 0, figure: None, shown: None }
    }

    /// Rebuilds the preview figure when the look it should show has changed.
    pub fn dress(&mut self, grf: &Grf, tables: &LookTables, real: &Look, shop: &StylingShop, class_num: impl Fn(u32) -> u16) {
        let look = preview_look(real, shop, self.picks, class_num);
        if self.shown.as_ref() != Some(&look) {
            self.shown = Some(look.clone());
            self.figure = SpriteSet::player(grf, &look, tables);
        }
    }

    pub fn pose(&mut self, gpu: &Gpu) -> Vec<Placed> {
        let direction = self.direction;
        self.figure.as_mut().map(|sprites| sprites.flat_frame(gpu, crate::actor::IDLE, direction, 0, Timing::Loop(0.0))).unwrap_or_default()
    }

    /// The pointer is over the window.
    pub fn covers(&self, cursor: [f32; 2]) -> bool {
        self.frame.local(cursor, HEIGHT).is_some()
    }

    fn place(&self, at: [f32; 2], size: [f32; 2]) -> [f32; 4] {
        let [x, y] = self.frame.at(at);
        [x, y, size[0], size[1]]
    }

    /// What can be pressed, on screen (for the test driver): the tabs, the arrows (`hair_style_prev`, `_next`,
    /// `clothes_prev`, `_next`), `hair_color_<row>` and `hair_color_none`, `head_top_<row>` and so on, `buy`, `refresh`,
    /// `rotate_left`, `rotate_right`, `close`.
    pub fn button_boxes(&self, shop: &StylingShop) -> Vec<(String, [f32; 4])> {
        if !self.frame.open {
            return Vec::new();
        }
        let mut boxes = vec![
            ("tab_hair".to_owned(), self.place(tab_at(0), TAB_SIZE)),
            ("tab_accessory".to_owned(), self.place(tab_at(1), TAB_SIZE)),
            ("hair_style_prev".to_owned(), self.place(STYLE_ARROWS[0], ARROW_SIZE)),
            ("hair_style_next".to_owned(), self.place(STYLE_ARROWS[1], ARROW_SIZE)),
            ("clothes_prev".to_owned(), self.place(CLOTHES_ARROWS[0], ARROW_SIZE)),
            ("clothes_next".to_owned(), self.place(CLOTHES_ARROWS[1], ARROW_SIZE)),
            ("buy".to_owned(), self.place(BUY, BUY_SIZE)),
            ("refresh".to_owned(), self.place(RESET, RESET_SIZE)),
            ("rotate_left".to_owned(), self.place(TURNS[0].0, TURNS[0].1)),
            ("rotate_right".to_owned(), self.place(TURNS[1].0, TURNS[1].1)),
            ("close".to_owned(), self.place(self.frame.close_button(), BUTTON_SIZE)),
        ];
        match self.tab {
            Tab::Hair => {
                boxes.extend(chips(shop).map(|(n, row, _)| (format!("hair_color_{row}"), self.place(chip_at(n), CHIP_SIZE))));
                boxes.push(("hair_color_none".to_owned(), self.place(NO_DYE_CHIP, NO_DYE_SIZE)));
            }
            Tab::Accessory => boxes.extend(accessories(shop).map(|(index, hit, _)| {
                let name = match hit {
                    Hit::HeadTop(row) => format!("head_top_{row}"),
                    Hit::HeadMid(row) => format!("head_mid_{row}"),
                    Hit::HeadBottom(row) => format!("head_bottom_{row}"),
                    _ => String::new(),
                };
                (name, self.place(accessory_at(index), ACCESSORY_ROW))
            })),
        }
        boxes
    }

    /// What's under the pointer; `rows` is how many rows the list has (their marks tell why one can't be paid).
    pub fn hit(&self, cursor: [f32; 2], shop: &StylingShop, rows: usize) -> Option<Hit> {
        let local = self.frame.local(cursor, HEIGHT)?;
        // The picture's title bar has a close button only: the rest of it holds the window.
        match self.frame.title_hit(local) {
            Some(TitleHit::Minimize) => return Some(Hit::Title(TitleHit::Title)),
            Some(hit) => return Some(Hit::Title(hit)),
            None => {}
        }
        for (place, hit) in [Hit::Tab(Tab::Hair), Hit::Tab(Tab::Accessory), Hit::DeadTab(2), Hit::DeadTab(3)].into_iter().enumerate() {
            if contains(tab_at(place), TAB_SIZE, local) {
                return Some(hit);
            }
        }
        let fixed = [
            (STYLE_ARROWS[0], ARROW_SIZE, Hit::Style(false)),
            (STYLE_ARROWS[1], ARROW_SIZE, Hit::Style(true)),
            (CLOTHES_ARROWS[0], ARROW_SIZE, Hit::Clothes(false)),
            (CLOTHES_ARROWS[1], ARROW_SIZE, Hit::Clothes(true)),
            (RESET, RESET_SIZE, Hit::Refresh),
            (TURNS[0].0, TURNS[0].1, Hit::Rotate(1)),
            (TURNS[1].0, TURNS[1].1, Hit::Rotate(-1)),
            (BUY, BUY_SIZE, Hit::Buy),
        ];
        if let Some(&(.., hit)) = fixed.iter().find(|&&(place, size, _)| contains(place, size, local)) {
            return Some(hit);
        }
        Some(match self.tab {
            Tab::Hair => {
                if let Some((_, row, _)) = chips(shop).find(|&(n, ..)| contains(chip_at(n), CHIP_SIZE, local)) {
                    Hit::HairColor(row)
                } else if contains(NO_DYE_CHIP, NO_DYE_SIZE, local) {
                    Hit::HairColor(NO_DYE)
                } else if let Some(row) = (0..rows.min(LIST_ROWS)).find(|&row| contains(list_at(row), MARK_SIZE, local)) {
                    Hit::Mark(row)
                } else {
                    Hit::Window
                }
            }
            Tab::Accessory => accessories(shop).find(|&(index, ..)| contains(accessory_at(index), ACCESSORY_ROW, local)).map_or(Hit::Window, |(_, hit, _)| hit),
        })
    }

    /// A click. `real` is the look worn; `buyable` whether every row of the list can be paid (`can_buy`).
    pub fn click(&mut self, hit: Hit, shop: &StylingShop, real: &Look, buyable: bool) -> Option<Action> {
        let toggle = |slot: &mut u16, row: u16| {
            *slot = if *slot == row { 0 } else { row };
        };
        match hit {
            Hit::Title(TitleHit::Close) => return Some(Action::Close),
            Hit::Tab(tab) => self.tab = tab,
            // The arrows step through the job's tables from what the figure shows. A style that can't be dyed drops
            // a picked colour.
            Hit::Style(forward) => {
                self.picks.hair_style = stepped(styles(shop, real.job), self.picks.hair_style, u32::from(real.hair), forward);
                if !can_dye(shop, real, self.picks) {
                    self.picks.hair_color = 0;
                }
            }
            Hit::Clothes(forward) => {
                self.picks.clothes_color = stepped(clothes(shop, real.job), self.picks.clothes_color, u32::from(real.clothes_color), forward)
            }
            // A chip picks its colour; the one worn already is nothing to buy; hair that can't be dyed says so.
            Hit::HairColor(row) => {
                if !can_dye(shop, real, self.picks) {
                    return Some(Action::Notice(CANT_DYE));
                }
                let color = if row == NO_DYE { 0 } else { shop.head_palette.get(&row).map_or(0, |entry| entry.goods) };
                self.picks.hair_color = if color == u32::from(real.hair_color) { 0 } else { row };
            }
            Hit::HeadTop(row) => toggle(&mut self.picks.head_top, row),
            Hit::HeadMid(row) => toggle(&mut self.picks.head_mid, row),
            Hit::HeadBottom(row) => toggle(&mut self.picks.head_bottom, row),
            Hit::Refresh => self.picks = Picks::default(),
            Hit::Rotate(step) => self.direction = (self.direction as i8 + step).rem_euclid(8) as u8,
            Hit::Buy if buyable => return Some(Action::Ask(self.picks.request())),
            Hit::Buy | Hit::Title(_) | Hit::DeadTab(_) | Hit::Mark(_) | Hit::Window => {}
        }
        None
    }

    /// The item icons the list's rows show: a headgear's own, a coupon's.
    pub fn icons(&self, items: &HashMap<u32, ItemInfo>, rows: &[ListRow]) -> Vec<String> {
        rows.iter().filter_map(row_item).filter_map(|id| items.get(&id).map(|info| inventory::icon_path(info, true))).collect()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &'a self,
        layer: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        icons: &'a HashMap<String, Option<Texture>>,
        items: &HashMap<u32, ItemInfo>,
        shop: &StylingShop,
        rows: &[ListRow],
        zeny: i64,
        has: impl Fn(u32) -> u32,
        messages: &[String],
        font: &Font,
        pointer: ([f32; 2], bool),
        placements: &[Placed],
    ) {
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        let at = |local: [f32; 2]| self.frame.at(local);
        // The picture is the whole window, its own title bar included: only the close button goes on it.
        layer.sprites.push((&assets.background, at([0.0, 0.0]), SIZE));
        let close = self.frame.close_button();
        let image = chrome.button(2, contains(at(close), BUTTON_SIZE, pointer.0));
        layer.sprites.push((image, at(close), image.size));
        let (cursor, down) = pointer;
        let hit = self.hit(cursor, shop, rows.len());
        // A button's picture: normal, under the pointer, held down.
        let pressed = |target: Hit| usize::from(hit == Some(target)) + usize::from(hit == Some(target) && down);

        // The tabs down the left column; the pointer on one names it.
        for (place, target) in [Hit::Tab(Tab::Hair), Hit::Tab(Tab::Accessory), Hit::DeadTab(2), Hit::DeadTab(3)].into_iter().enumerate() {
            let state = if target == Hit::Tab(self.tab) { 2 } else { usize::from(hit == Some(target)) };
            layer.sprites.push((&assets.tabs[place][state], at(tab_at(place)), TAB_SIZE));
        }

        // The figure over its shadow, and its controls.
        layer.sprites.push((&assets.shadow, at(SHADOW), assets.shadow.size));
        if let Some(figure) = &self.figure {
            layer.sprites.extend(placements.iter().map(|(key, position, size, _)| (figure.texture(*key), at(offset(FEET, *position)), *size)));
        }
        for (arrows, forward) in [(STYLE_ARROWS, Hit::Style as fn(bool) -> Hit), (CLOTHES_ARROWS, Hit::Clothes)] {
            for (side, place) in arrows.into_iter().enumerate() {
                layer.sprites.push((&assets.arrows[side][usize::from(hit == Some(forward(side == 1)))], at(place), ARROW_SIZE));
            }
        }
        layer.sprites.push((&assets.refresh[pressed(Hit::Refresh)], at(RESET), RESET_SIZE));
        for (side, step) in [(0, 1i8), (1, -1)] {
            layer.sprites.push((&assets.rotate[side][pressed(Hit::Rotate(step))], at(TURNS[side].0), TURNS[side].1));
        }
        layer.sprites.push((&assets.dotted, at(DOTTED), assets.dotted.size));

        match self.tab {
            Tab::Hair => {
                // The hair colours: the one the figure shows is lit.
                let shown = self.shown.as_ref().map_or(0, |look| u32::from(look.hair_color));
                layer.sprites.push((&assets.color_box, at(COLOR_BOX), assets.color_box.size));
                for (n, row, entry) in chips(shop) {
                    let state = if entry.goods == shown { 2 } else { usize::from(hit == Some(Hit::HairColor(row))) };
                    if let Some(images) = assets.colors.get(n) {
                        layer.sprites.push((&images[state], at(chip_at(n)), CHIP_SIZE));
                    }
                }
                layer.sprites.push((&assets.no_dye[usize::from(shown == 0)], at(NO_DYE_CHIP), NO_DYE_SIZE));
                self.draw_list(layer, assets, icons, items, rows, &has, messages, font);
            }
            Tab::Accessory => {
                // ponytail: the headgears as rows of names; the game shows their pictures in a showcase here.
                let picked = [Hit::HeadTop(self.picks.head_top), Hit::HeadMid(self.picks.head_mid), Hit::HeadBottom(self.picks.head_bottom)];
                for (index, target, entry) in accessories(shop) {
                    let place = accessory_at(index);
                    if picked.contains(&target) {
                        layer.sprites.push((&assets.selected, at(place), ACCESSORY_ROW));
                    }
                    let name = items.get(&entry.goods).map(|info| info.name.as_str()).filter(|name| !name.is_empty());
                    let label = window::fit(font, name.unwrap_or(&entry.goods.to_string()), ACCESSORY_ROW[0] - 8.0);
                    layer.texts.push((label, at(offset(place, [4.0, 1.0])), BLACK));
                }
            }
        }

        // The zeny carried, and Buy: switched off until something is picked and every row can be paid.
        layer.texts.push((format!("{} {}  z", message(ON_HAND), group(zeny.max(0) as u64)), at(ZENY_LINE), BROWN));
        let buy = if can_buy(rows) { pressed(Hit::Buy) } else { 3 };
        layer.sprites.push((&assets.buy[buy], at(BUY), BUY_SIZE));

        // Over everything: a tab's name, and why a coupon row can't be paid.
        let tip = match hit {
            Some(Hit::Tab(Tab::Hair)) => Some((message(TAB_NAMES[0]), tab_at(0))),
            Some(Hit::Tab(Tab::Accessory)) => Some((message(TAB_NAMES[1]), tab_at(1))),
            Some(Hit::DeadTab(tab)) => Some((message(TAB_NAMES[tab]), tab_at(tab))),
            Some(Hit::Mark(row)) => match rows.get(row) {
                Some(ListRow { pay: Pay::Coupon(_, box_item), payable: false, .. }) => {
                    Some((message(if *box_item != 0 && has(*box_item) > 0 { OPEN_THE_BOX } else { NO_COUPON }), list_at(row)))
                }
                _ => None,
            },
            _ => None,
        };
        if let Some((text, place)) = tip.filter(|(text, _)| !text.is_empty()) {
            window::tooltip(layer, chrome, font, text, at(offset(place, [0.0, -20.0])), WHITE);
        }
    }

    /// The list of what will be bought: a rule under each of its eight rows, and for each part picked a mark (can be
    /// paid or not), an icon and what it costs; with nothing picked, what to do.
    #[allow(clippy::too_many_arguments)]
    fn draw_list<'a>(
        &'a self,
        layer: &mut Layer<'a>,
        assets: &'a Assets,
        icons: &'a HashMap<String, Option<Texture>>,
        items: &HashMap<u32, ItemInfo>,
        rows: &[ListRow],
        has: &impl Fn(u32) -> u32,
        messages: &[String],
        font: &Font,
    ) {
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        let at = |local: [f32; 2]| self.frame.at(local);
        for row in 0..LIST_ROWS {
            let place = offset(list_at(row), [0.0, LIST_RULE]);
            layer.sprites.push((&assets.rule, at(place), [assets.rule.size[0].min(SIZE[0] - place[0]), assets.rule.size[1]]));
        }
        if rows.is_empty() {
            for (line, text) in window::wrap(font, message(HINT), HINT_WIDTH, LIST_ROWS).into_iter().enumerate() {
                layer.texts.push((text, at(list_at(line)), BLACK));
            }
        }
        for (place, row) in rows.iter().take(LIST_ROWS).enumerate().map(|(index, row)| (list_at(index), row)) {
            layer.sprites.push((&assets.mark_box, at(place), assets.mark_box.size));
            layer.sprites.push((&assets.marks[usize::from(!row.payable)], at(place), MARK_SIZE));
            let item_icon = row_item(row).and_then(|id| items.get(&id)).and_then(|info| icons.get(&inventory::icon_path(info, true))).and_then(Option::as_ref);
            let icon = match (item_icon, row.pay) {
                (Some(icon), _) => icon,
                (None, Pay::Free) => &assets.no_color,
                (None, _) => &assets.zeny,
            };
            layer.sprites.push((icon, at(offset(place, LIST_ICON)), icon.size));
            let text = match row.pay {
                Pay::Zeny(zeny) => format!("{zeny} Zeny"),
                // The coupon's name, or its box's when only the box is carried.
                Pay::Coupon(item, box_item) => {
                    inventory::item_name(items, if item == 0 || (has(item) == 0 && box_item != 0 && has(box_item) > 0) { box_item } else { item }, true, 0)
                }
                Pay::Free => message(DYE_REMOVAL).to_owned(),
            };
            let text = window::fit(font, &text, SIZE[0] - place[0] - LIST_TEXT - 4.0);
            let spot = offset(place, [LIST_TEXT, 0.0]);
            if !row.payable {
                layer.sprites.push((&assets.strike, at(offset(spot, [-2.0, 5.0])), [font.width(&text) + 2.0, 1.0]));
            }
            layer.texts.push((text, at(spot), if row.payable { BROWN } else { GREY }));
        }
    }
}

/// The item whose icon a list row shows: a headgear's own, a coupon's (its box's with no coupon item); none on a
/// row paid in zeny or free.
fn row_item(row: &ListRow) -> Option<u32> {
    match (row.part, row.pay) {
        (Part::HeadTop | Part::HeadMid | Part::HeadBottom, _) => Some(row.goods),
        (_, Pay::Coupon(item, box_item)) => Some(if item != 0 { item } else { box_item }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(goods: u32, zeny: u32, item: u32, box_item: u32, is_coloring: Option<bool>) -> StyleRow {
        StyleRow { goods, zeny, item, box_item, is_coloring, ..StyleRow::default() }
    }

    fn shop() -> StylingShop {
        let mut shop = StylingShop::default();
        for index in 1..=8 {
            shop.head_palette.insert(index, row(u32::from(index), 100_000, 0, 0, None));
        }
        shop.head_style.insert(5, row(5, 100_000, 0, 0, None));
        shop.head_style.insert(6, row(6, 100_000, 0, 0, Some(false)));
        shop.head_style.insert(24, row(24, 0, 7622, 16843, None));
        shop.doram_head_style.insert(1, row(9, 100_000, 0, 0, Some(true)));
        shop.doram_head_style.insert(2, row(2, 100_000, 0, 0, Some(false)));
        shop.body_palette.insert(1, row(0, 0, 6047, 16855, None));
        shop.body_palette.insert(2, row(2, 0, 6046, 16854, None));
        shop.doram_body_palette.insert(2, row(3, 0, 6046, 16854, None));
        shop.top_accessory.insert(1, row(2220, 1_000, 0, 0, None));
        shop.bottom_accessory.insert(1, row(2241, 5_000, 0, 0, None));
        shop.body_style.insert(1, row(0, 0, 6959, 6959, None));
        shop
    }

    fn look() -> Look {
        Look { job: 0, sex: 1, hair: 1, hair_color: 0, clothes_color: 1, head_top: 4, head_mid: 5, head_bottom: 6, ..Look::default() }
    }

    #[test]
    fn previews_the_picks_without_touching_the_real_look() {
        let shop = shop();
        let real = look();
        let picks = Picks { hair_color: 3, hair_style: 5, clothes_color: 2, head_top: 1, ..Picks::default() };
        let preview = preview_look(&real, &shop, picks, |id| if id == 2220 { 16 } else { 0 });
        assert_eq!((preview.hair, preview.hair_color, preview.clothes_color, preview.head_top), (5, 3, 2, 16));
        assert_eq!((preview.head_mid, preview.head_bottom), (5, 6));
        assert_eq!(real, look());
        // Clothes row 1 is color 0, which is a real color, not "no change".
        let dyed = preview_look(&real, &shop, Picks { clothes_color: 1, ..Picks::default() }, |_| 0);
        assert_eq!(dyed.clothes_color, 0);
    }

    #[test]
    fn doram_uses_its_own_hair_and_clothes() {
        let shop = shop();
        let picks = Picks { hair_style: 1, clothes_color: 2, hair_color: 3, ..Picks::default() };
        // Row 1 is not in the human hair list, so that hair stays. Row 5 is, and its goods is the style.
        let human = preview_look(&look(), &shop, picks, |_| 0);
        assert_eq!((human.hair, human.clothes_color, human.hair_color), (1, 2, 3));
        let styled = preview_look(&look(), &shop, Picks { hair_style: 5, ..picks }, |_| 0);
        assert_eq!(styled.hair, 5);
        for job in DORAM_JOBS {
            let doram = preview_look(&Look { job, ..look() }, &shop, picks, |_| 0);
            assert_eq!((doram.hair, doram.clothes_color, doram.hair_color), (9, 3, 3), "{job}");
        }
        // 4217 is a Doram novice, not one of the three summoner jobs, so it uses the human lists.
        let novice = preview_look(&Look { job: 4217, ..look() }, &shop, picks, |_| 0);
        assert_eq!((novice.hair, novice.clothes_color), (1, 2));
    }

    fn parts(rows: &[ListRow]) -> Vec<(Part, Pay, bool)> {
        rows.iter().map(|row| (row.part, row.pay, row.payable)).collect()
    }

    #[test]
    fn lists_what_the_picks_cost_in_the_order_they_are_charged() {
        let shop = shop();
        // A hair colour for zeny and a clothes colour for a coupon: both rows, the coupon one unpaid without it.
        let picks = Picks { hair_color: 3, clothes_color: 2, ..Picks::default() };
        let rows = list(&shop, 0, picks, 100_000, |_| 0);
        assert_eq!(parts(&rows), [(Part::HairColor, Pay::Zeny(100_000), true), (Part::ClothesColor, Pay::Coupon(6046, 16854), false)]);
        assert!(!can_buy(&rows));
        // The coupon, or the box that holds one, pays it.
        assert!(can_buy(&list(&shop, 0, picks, 100_000, |id| u32::from(id == 6046))));
        assert!(can_buy(&list(&shop, 0, picks, 100_000, |id| u32::from(id == 16854))));
        // Too little zeny: the zeny row can't be paid, the coupon row still can.
        assert_eq!(parts(&list(&shop, 0, picks, 99_999, |_| 1)).iter().map(|row| row.2).collect::<Vec<_>>(), [false, true]);
        // Every part, in order; headgears carry their item for the icon; zeny runs out down the list.
        let all = Picks { hair_color: 3, hair_style: 5, clothes_color: 2, head_top: 1, head_bottom: 1, ..Picks::default() };
        let rows = list(&shop, 0, all, 201_000, |_| 1);
        assert_eq!(
            rows.iter().map(|row| row.part).collect::<Vec<_>>(),
            [Part::HairColor, Part::HairStyle, Part::ClothesColor, Part::HeadTop, Part::HeadBottom]
        );
        assert_eq!(rows.iter().map(|row| row.payable).collect::<Vec<_>>(), [true, true, true, true, false]);
        assert_eq!((rows[3].goods, row_item(&rows[3]), row_item(&rows[2]), row_item(&rows[0])), (2220, Some(2220), Some(6046), None));
        // Nothing picked: no rows, and Buy is off.
        assert!(!can_buy(&list(&shop, 0, Picks::default(), 9_000_000, |_| 9)));
    }

    #[test]
    fn two_of_the_same_coupon_need_two() {
        let mut shop = shop();
        // Hair style 24 and clothes row 2 both ask for coupon 7622 (or its box).
        shop.body_palette.insert(2, row(2, 0, 7622, 16843, None));
        let picks = Picks { hair_style: 24, clothes_color: 2, ..Picks::default() };
        let paid = |has: fn(u32) -> u32| list(&shop, 0, picks, 0, has).iter().map(|row| row.payable).collect::<Vec<_>>();
        // One of the coupon and one of the box cover the two rows; one coupon alone covers only the first.
        assert_eq!(paid(|_| 1), [true, true]);
        assert_eq!(paid(|_| 0), [false, false]);
        assert_eq!(paid(|id| u32::from(id == 7622)), [true, false]);
        assert_eq!(paid(|_| 2), [true, true]);
    }

    #[test]
    fn the_dye_taken_off_is_a_free_row() {
        let shop = shop();
        let dyed = Look { hair_color: 4, ..look() };
        let mut window = Window::open();
        assert_eq!(window.click(Hit::HairColor(NO_DYE), &shop, &dyed, false), None);
        assert_eq!(window.picks.hair_color, NO_DYE);
        assert_eq!(preview_look(&dyed, &shop, window.picks, |_| 0).hair_color, 0);
        let rows = list(&shop, 0, window.picks, 0, |_| 0);
        assert_eq!(parts(&rows), [(Part::HairColor, Pay::Free, true)]);
        assert!(can_buy(&rows));
        assert_eq!(window.picks.request().hair_color, 0xFFFF);
        // Undyed hair has no dye to take off; the colour worn is nothing to buy either.
        window.click(Hit::HairColor(NO_DYE), &shop, &look(), false);
        assert_eq!(window.picks.hair_color, 0);
        window.click(Hit::HairColor(4), &shop, &dyed, false);
        assert_eq!(window.picks.hair_color, 0);
        window.click(Hit::HairColor(5), &shop, &dyed, false);
        assert_eq!(window.picks.hair_color, 5);
    }

    #[test]
    fn hair_that_cant_be_dyed_refuses_a_colour_in_a_box() {
        let shop = shop();
        // Style row 6 can't be dyed: wearing it, or having stepped to it, a chip says so and picks nothing.
        let mut window = Window::open();
        assert_eq!(window.click(Hit::HairColor(3), &shop, &Look { hair: 6, ..look() }, false), Some(Action::Notice(CANT_DYE)));
        assert_eq!(window.picks, Picks::default());
        window.picks.hair_style = 6;
        assert_eq!(window.click(Hit::HairColor(3), &shop, &look(), false), Some(Action::Notice(CANT_DYE)));
        // Stepping from a dyeable style with a colour picked onto it drops the colour.
        window.picks = Picks { hair_style: 5, hair_color: 3, ..Picks::default() };
        window.click(Hit::Style(true), &shop, &look(), false);
        assert_eq!(window.picks, Picks { hair_style: 6, ..Picks::default() });
        // A Doram style with isColoring false, and one that says nothing.
        assert_eq!(Window::open().click(Hit::HairColor(1), &shop, &Look { job: 4218, hair: 2, ..look() }, false), Some(Action::Notice(CANT_DYE)));
        assert_eq!(Window::open().click(Hit::HairColor(1), &shop, &Look { hair: 5, ..look() }, false), None);
    }

    #[test]
    fn the_arrows_step_through_the_tables_and_wrap() {
        let shop = shop();
        let mut window = Window::open();
        // Hair style rows 5, 6, 24; the look wears style 1, which has no row: forward starts at the first.
        let steps = |window: &mut Window, forward: bool, real: &Look| {
            window.click(Hit::Style(forward), &shop, real, false);
            window.picks.hair_style
        };
        assert_eq!([true, true, true, true].map(|forward| steps(&mut window, forward, &look())), [5, 6, 24, 5]);
        assert_eq!(steps(&mut window, false, &look()), 24);
        // Wearing style 6: stepping onto it is nothing to buy, and the next step goes on from there.
        let worn = Look { hair: 6, ..look() };
        window.picks = Picks::default();
        assert_eq!([true, false, false, true, true].map(|forward| steps(&mut window, forward, &worn)), [24, 0, 5, 0, 24]);
        // Clothes colours rows 1 (colour 0) and 2 (colour 2); the look wears colour 1.
        window.click(Hit::Clothes(false), &shop, &look(), false);
        assert_eq!(window.picks.clothes_color, 2);
        window.click(Hit::Clothes(true), &shop, &look(), false);
        assert_eq!(window.picks.clothes_color, 1);
        // Reset drops every pick; the turn buttons go round the 8 directions.
        window.click(Hit::Refresh, &shop, &look(), false);
        assert_eq!(window.picks, Picks::default());
        window.click(Hit::Rotate(-1), &shop, &look(), false);
        assert_eq!(window.direction, 7);
        window.click(Hit::Rotate(1), &shop, &look(), false);
        assert_eq!(window.direction, 0);
    }

    #[test]
    fn the_pieces_sit_at_the_pictures_places() {
        let shop = shop();
        let mut window = Window::open();
        window.frame.position = [100.0, 50.0];
        let at = |window: &Window, x: f32, y: f32, rows: usize| window.hit([100.0 + x, 50.0 + y], &shop, rows);
        // The picture's title bar: close at (466, 3), no minimize; the window is 480 x 330.
        assert_eq!([at(&window, 468.0, 5.0, 0), at(&window, 453.0, 5.0, 0)], [Some(Hit::Title(TitleHit::Close)), Some(Hit::Title(TitleHit::Title))]);
        assert_eq!([at(&window, 479.0, 329.0, 0), at(&window, 480.0, 329.0, 0), at(&window, 479.0, 330.0, 0)], [Some(Hit::Window), None, None]);
        // Tabs 37 x 40 down the column from (3, 21); the last two do nothing.
        assert_eq!(
            [21.0, 61.0, 101.0, 141.0, 181.0].map(|y| at(&window, 3.0, y, 0)),
            [Some(Hit::Tab(Tab::Hair)), Some(Hit::Tab(Tab::Accessory)), Some(Hit::DeadTab(2)), Some(Hit::DeadTab(3)), Some(Hit::Window)]
        );
        assert_eq!(window.click(Hit::DeadTab(2), &shop, &look(), false), None);
        // The arrows 8 x 12: hair style at y 107, clothes colour at y 134, x 59 and 279.
        assert_eq!([at(&window, 59.0, 107.0, 0), at(&window, 286.0, 118.0, 0)], [Some(Hit::Style(false)), Some(Hit::Style(true))]);
        assert_eq!([at(&window, 66.0, 145.0, 0), at(&window, 279.0, 134.0, 0)], [Some(Hit::Clothes(false)), Some(Hit::Clothes(true))]);
        // Reset 16 x 16 at (55, 210); the turn buttons at (148, 229) 21 x 9 and (169, 229) 20 x 9; Buy 80 x 20 at (396, 306).
        assert_eq!(
            [at(&window, 55.0, 210.0, 0), at(&window, 70.0, 225.0, 0), at(&window, 71.0, 225.0, 0)],
            [Some(Hit::Refresh), Some(Hit::Refresh), Some(Hit::Window)]
        );
        assert_eq!(
            [at(&window, 148.0, 229.0, 0), at(&window, 168.0, 237.0, 0), at(&window, 169.0, 229.0, 0), at(&window, 188.0, 237.0, 0)],
            [Some(Hit::Rotate(1)), Some(Hit::Rotate(1)), Some(Hit::Rotate(-1)), Some(Hit::Rotate(-1))]
        );
        assert_eq!(
            [at(&window, 396.0, 306.0, 0), at(&window, 475.0, 325.0, 0), at(&window, 395.0, 306.0, 0)],
            [Some(Hit::Buy), Some(Hit::Buy), Some(Hit::Window)]
        );
        // Hair colour chip n at (156 + 18 (n mod 4), 254 + 18 (n div 4)), 14 x 14; the dye-off chip 16 x 16 at (125, 273).
        assert_eq!(
            [at(&window, 156.0, 254.0, 0), at(&window, 210.0, 254.0, 0), at(&window, 156.0, 272.0, 0), at(&window, 223.0, 285.0, 0)],
            [Some(Hit::HairColor(1)), Some(Hit::HairColor(4)), Some(Hit::HairColor(5)), Some(Hit::HairColor(8))]
        );
        assert_eq!([at(&window, 170.0, 254.0, 0), at(&window, 125.0, 273.0, 0)], [Some(Hit::Window), Some(Hit::HairColor(NO_DYE))]);
        // The list: row i's mark 17 x 13 at (310, 36 + 35 i), only for the rows it has.
        assert_eq!(
            [at(&window, 310.0, 36.0, 2), at(&window, 326.0, 83.0, 2), at(&window, 310.0, 106.0, 2)],
            [Some(Hit::Mark(0)), Some(Hit::Mark(1)), Some(Hit::Window)]
        );
        assert_eq!(
            (list_at(3), offset(list_at(3), LIST_ICON), list_at(3)[0] + LIST_TEXT, list_at(3)[1] + LIST_RULE),
            ([310.0, 141.0], [331.0, 133.0], 358.0, 158.0)
        );
        // The accessory tab: the headgears' rows from (312, 30), 16 px each, upper then lower.
        window.click(Hit::Tab(Tab::Accessory), &shop, &look(), false);
        assert_eq!(
            [at(&window, 312.0, 30.0, 0), at(&window, 400.0, 46.0, 0), at(&window, 156.0, 254.0, 0)],
            [Some(Hit::HeadTop(1)), Some(Hit::HeadBottom(1)), Some(Hit::Window)]
        );
    }

    #[test]
    fn buy_asks_only_when_every_row_can_be_paid_and_the_buttons_are_named() {
        let shop = shop();
        let mut window = Window::open();
        // Opens in the middle of the screen.
        let screen = crate::screen();
        assert_eq!(window.frame.position, [((screen[0] - 480.0) / 2.0).floor(), ((screen[1] - 330.0) / 2.0).floor()]);
        window.click(Hit::HairColor(3), &shop, &look(), false);
        window.click(Hit::Clothes(false), &shop, &look(), false);
        assert_eq!(window.click(Hit::Buy, &shop, &look(), false), None);
        match window.click(Hit::Buy, &shop, &look(), true) {
            Some(Action::Ask(picks)) => assert_eq!((picks.hair_color, picks.clothes_color, picks.body_style, picks.hair_style), (3, 2, 0, 0)),
            other => panic!("{other:?}"),
        }
        assert_eq!(window.click(Hit::Title(TitleHit::Close), &shop, &look(), false), Some(Action::Close));
        let names: Vec<_> = window.button_boxes(&shop).into_iter().map(|(name, _)| name).collect();
        for name in [
            "hair_style_prev",
            "hair_style_next",
            "clothes_prev",
            "clothes_next",
            "hair_color_3",
            "hair_color_none",
            "tab_hair",
            "tab_accessory",
            "buy",
            "refresh",
            "rotate_left",
            "rotate_right",
            "close",
        ] {
            assert!(names.iter().any(|got| got == name), "{name} in {names:?}");
        }
        assert!(names.iter().all(|name| !name.contains("body") && !name.contains("costume")));
        window.click(Hit::Tab(Tab::Accessory), &shop, &look(), false);
        let names: Vec<_> = window.button_boxes(&shop).into_iter().map(|(name, _)| name).collect();
        assert!(names.contains(&"head_top_1".to_owned()) && names.contains(&"head_bottom_1".to_owned()) && !names.contains(&"hair_color_3".to_owned()));
    }
}
