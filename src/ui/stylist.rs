//! The style shop (`@stylist`, the stylist NPCs): hair style and color, clothes color and a few headgears, tried on a
//! preview and bought together.
//!
//! ponytail: where the pieces sit, which bitmap goes where and the text colors aren't in the sources. The places
//! below are estimated against `bg_styling.bmp` (480×330) and the named bitmaps in `styleshop\`.

use std::collections::{BTreeMap, HashMap};

use rustnarok_client::grf::Grf;
use rustnarok_client::lua::{ItemInfo, LookTables, StyleRow, StylingShop};
use rustnarok_client::net::StylePicks;
use rustnarok_client::text::Font;

use crate::actor::{Look, Placed, SpriteSet, Timing};
use crate::render::{Gpu, Texture};
use crate::window::{self, BUTTON_SIZE, Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI, contains, offset};

/// ZC_UI_OPEN's kind for the stylist. Kind 8 is grade enchanting, which this window doesn't open.
pub const UI_KIND: u8 = 1;
pub const TITLE: usize = 2813;
pub const NO_COUPON: usize = 2814;
pub const HAIR_COLOR_LABEL: usize = 2763;
pub const HAIR_STYLE_LABEL: usize = 2764;
pub const CLOTHES_LABEL: usize = 2765;
pub const ACCESSORY_LABEL: usize = 2831;
/// A hair style the table says cannot be dyed.
pub const CANT_DYE: usize = 2834;
const HINT: usize = 2835;
pub const DONE: usize = 2836;
const ROTATE_LEFT_LABEL: usize = 2174;
const ROTATE_RIGHT_LABEL: usize = 2175;
pub const NO_ZENY: usize = 2968;
/// ponytail: no client line fits a failed style change, so the result is printed plainly.
pub const FAILED_LINE: &str = "Style change failed.";

/// Jobs that use the Doram hair styles and clothes colors (rathena's MAPID_SUMMONER).
const DORAM_JOBS: [u16; 3] = [4218, 4220, 4308];

// ponytail: estimated layout.
const WIDTH: f32 = 480.0;
const BODY: f32 = 330.0;
pub const HEIGHT: f32 = TITLE_HEIGHT + BODY;
const POSITION: [f32; 2] = [272.0, 190.0];
const TAB_AT: [f32; 2] = [12.0, TITLE_HEIGHT + 6.0];
const TAB_SIZE: [f32; 2] = [37.0, 40.0];
const TAB_STEP: f32 = 40.0;
const STYLE_AT: [f32; 2] = [248.0, TITLE_HEIGHT + 28.0];
const STYLE_SIZE: [f32; 2] = [216.0, 16.0];
const VISIBLE_STYLES: usize = 8;
const COLOR_AT: [f32; 2] = [248.0, TITLE_HEIGHT + 186.0];
const CLOTHES_AT: [f32; 2] = [248.0, TITLE_HEIGHT + 224.0];
const COLOR_SIZE: [f32; 2] = [14.0, 14.0];
const COLOR_STEP: f32 = 18.0;
const ACCESSORY_AT: [f32; 2] = [248.0, TITLE_HEIGHT + 28.0];
const ACCESSORY_ROW: [f32; 2] = [216.0, 16.0];
const BUY_AT: [f32; 2] = [384.0, TITLE_HEIGHT + 292.0];
const BUY_SIZE: [f32; 2] = [80.0, 20.0];
const REFRESH_AT: [f32; 2] = [292.0, TITLE_HEIGHT + 292.0];
const REFRESH_SIZE: [f32; 2] = [80.0, 20.0];
const ROTATE_LEFT: [f32; 2] = [16.0, TITLE_HEIGHT + 200.0];
const ROTATE_RIGHT: [f32; 2] = [196.0, TITLE_HEIGHT + 200.0];
const ROTATE_SIZE: [f32; 2] = [22.0, 22.0];
const PREVIEW_FEET: [f32; 2] = [118.0, TITLE_HEIGHT + 200.0];
const PRICE_AT: [f32; 2] = [16.0, TITLE_HEIGHT + 292.0];
const BLACK: [u8; 3] = [0, 0, 0];
const WHITE: [u8; 3] = [255, 255, 255];

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

    fn any(self) -> bool {
        self != Self::default()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Hair,
    Accessory,
}

/// Zeny and coupons a set of picks costs, in the order rathena charges them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Quote {
    pub zeny: u32,
    /// Coupon item and the box that holds it, one entry per picked row that needs one.
    pub coupons: Vec<(u32, u32)>,
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

/// The rows a buy would change, in the order rathena applies them.
fn picked(shop: &StylingShop, job: u16, picks: Picks) -> Vec<&StyleRow> {
    let mut rows = Vec::new();
    for (list, index) in [
        (&shop.head_palette, picks.hair_color),
        (styles(shop, job), picks.hair_style),
        (clothes(shop, job), picks.clothes_color),
        (&shop.top_accessory, picks.head_top),
        (&shop.mid_accessory, picks.head_mid),
        (&shop.bottom_accessory, picks.head_bottom),
    ] {
        if let Some(row) = row(list, index) {
            rows.push(row);
        }
    }
    rows
}

/// The preview: the real look with the picks put in. Headgear's view is the item's ClassNum. The real look is left as it was.
pub fn preview_look(real: &Look, shop: &StylingShop, picks: Picks, class_num: impl Fn(u32) -> u16) -> Look {
    let mut look = real.clone();
    if let Some(row) = row(&shop.head_palette, picks.hair_color) {
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

pub fn quote(shop: &StylingShop, job: u16, picks: Picks) -> Quote {
    let mut quote = Quote::default();
    for row in picked(shop, job, picks) {
        quote.zeny = quote.zeny.saturating_add(row.zeny);
        if row.item != 0 || row.box_item != 0 {
            quote.coupons.push((row.item, row.box_item));
        }
    }
    quote
}

/// Why a buy is refused, as a message id, or nothing when it can be sent. A hair color on a style that cannot be
/// dyed, then the zeny, then each coupon (or its box). Nothing is sent when one of these fails: rathena would have
/// charged the earlier parts and stopped.
pub fn refusal(shop: &StylingShop, job: u16, hair: u16, picks: Picks, zeny: i64, has: impl Fn(u32) -> u32) -> Option<usize> {
    if picks.hair_color != 0 {
        let style = row(styles(shop, job), picks.hair_style).or_else(|| styles(shop, job).values().find(|row| row.goods == u32::from(hair)));
        if style.and_then(|row| row.is_coloring) == Some(false) {
            return Some(CANT_DYE);
        }
    }
    let quote = quote(shop, job, picks);
    if zeny < i64::from(quote.zeny) {
        return Some(NO_ZENY);
    }
    let mut needed: HashMap<(u32, u32), u32> = HashMap::new();
    for (item, box_item) in quote.coupons {
        *needed.entry((item, box_item)).or_default() += 1;
    }
    for ((item, box_item), count) in needed {
        let mut have = 0;
        if item != 0 {
            have += has(item);
        }
        if box_item != 0 && box_item != item {
            have += has(box_item);
        }
        if have < count {
            return Some(NO_COUPON);
        }
    }
    None
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hit {
    Title(TitleHit),
    Tab(Tab),
    HairStyle(u16),
    HairColor(u16),
    ClothesColor(u16),
    HeadTop(u16),
    HeadMid(u16),
    HeadBottom(u16),
    Buy,
    Refresh,
    Rotate(i8),
    Scroll(i8),
    Window,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    Buy(StylePicks),
    /// A message id.
    Refuse(usize),
    Close,
}

pub struct Assets {
    background: Texture,
    /// Hair and accessory: off, on, over. The second-costume tab is only the disabled image.
    tabs: [[Texture; 3]; 2],
    costume_off: Texture,
    buy: [Texture; 3],
    refresh: [Texture; 3],
    rotate: [[Texture; 3]; 2],
    /// color00 .. color08: off, on, over. A missing bitmap is a flat stand-in.
    colors: Vec<[Texture; 3]>,
    shadow: Texture,
    zeny: Texture,
    selected: Texture,
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |name: &str, fallback: [u8; 4]| {
            [format!("{UI}styleshop\\{name}"), format!("{UI}stylingshop\\{name}")]
                .into_iter()
                .find_map(|path| gpu.try_load_texture(grf, &path, false).ok())
                .unwrap_or_else(|| gpu.solid(fallback))
        };
        let states = |stem: &str, names: [&str; 3], fallback: [u8; 4]| names.map(|state| image(&format!("{stem}_{state}.bmp"), fallback));
        // ponytail: stand-in chips when the bitmap isn't in this data.grf.
        let chip = |index: usize| {
            let shade = (40 + index * 24) as u8;
            [shade, 80u8.saturating_add(shade / 2), 200u8.saturating_sub(shade), 255]
        };
        Self {
            background: image("bg_styling.bmp", [0xE4, 0xD8, 0xC4, 255]),
            tabs: [states("tab_hair", ["off", "on", "over"], [180, 140, 90, 255]), states("tab_accessories", ["off", "on", "over"], [140, 140, 160, 255])],
            costume_off: image("tab_2ndcostume_disable.bmp", [120, 120, 120, 255]),
            buy: states("btn_buy", ["out", "over", "down"], [80, 180, 80, 255]),
            refresh: states("btn_refresh", ["out", "over", "down"], [180, 180, 180, 255]),
            rotate: [
                states("btn_rotatearrow_l", ["out", "over", "down"], [200, 200, 200, 255]),
                states("btn_rotatearrow_r", ["out", "over", "down"], [200, 200, 200, 255]),
            ],
            colors: (0..9).map(|index| states(&format!("color{index:02}"), ["off", "on", "over"], chip(index))).collect(),
            shadow: image("shadow_chr_on.bmp", [0, 0, 0, 80]),
            zeny: image("icon_zeny.bmp", [240, 200, 40, 255]),
            selected: image("select_mark.bmp", [255, 220, 80, 180]),
        }
    }
}

pub struct Window {
    pub frame: Frame,
    tab: Tab,
    pub picks: Picks,
    scroll: usize,
    direction: u8,
    figure: Option<SpriteSet>,
    shown: Option<Look>,
}

impl Window {
    pub fn open() -> Self {
        let mut frame = Frame::new(POSITION, WIDTH);
        frame.open = true;
        Self { frame, tab: Tab::Hair, picks: Picks::default(), scroll: 0, direction: 0, figure: None, shown: None }
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

    /// The pointer is over the window, title bar included.
    pub fn covers(&self, cursor: [f32; 2]) -> bool {
        self.frame.local(cursor, HEIGHT).is_some()
    }

    fn place(&self, at: [f32; 2], size: [f32; 2]) -> [f32; 4] {
        let [x, y] = self.frame.at(at);
        [x, y, size[0], size[1]]
    }

    fn style_rows(&self, shop: &StylingShop, job: u16) -> Vec<u16> {
        styles(shop, job).keys().copied().skip(self.scroll).take(VISIBLE_STYLES).collect()
    }

    pub fn button_boxes(&self, shop: &StylingShop, job: u16) -> Vec<(String, [f32; 4])> {
        if !self.frame.open || self.frame.minimized {
            return Vec::new();
        }
        let mut boxes = vec![
            ("tab_hair".to_owned(), self.place(TAB_AT, TAB_SIZE)),
            ("tab_accessory".to_owned(), self.place([TAB_AT[0] + TAB_STEP, TAB_AT[1]], TAB_SIZE)),
            ("buy".to_owned(), self.place(BUY_AT, BUY_SIZE)),
            ("refresh".to_owned(), self.place(REFRESH_AT, REFRESH_SIZE)),
            ("rotate_left".to_owned(), self.place(ROTATE_LEFT, ROTATE_SIZE)),
            ("rotate_right".to_owned(), self.place(ROTATE_RIGHT, ROTATE_SIZE)),
            ("close".to_owned(), self.place(self.frame.close_button(), BUTTON_SIZE)),
        ];
        match self.tab {
            Tab::Hair => {
                for (index, row) in self.style_rows(shop, job).into_iter().enumerate() {
                    boxes.push((format!("hair_style_{row}"), self.place(style_at(index), STYLE_SIZE)));
                }
                for (index, row) in shop.head_palette.keys().copied().enumerate() {
                    boxes.push((format!("hair_color_{row}"), self.place(chip_at(COLOR_AT, index), COLOR_SIZE)));
                }
                for (index, row) in clothes(shop, job).keys().copied().enumerate() {
                    boxes.push((format!("clothes_color_{row}"), self.place(chip_at(CLOTHES_AT, index), COLOR_SIZE)));
                }
            }
            Tab::Accessory => {
                for (index, row) in shop.top_accessory.keys().copied().enumerate() {
                    boxes.push((format!("head_top_{row}"), self.place(accessory_at(index), ACCESSORY_ROW)));
                }
                let mid = shop.top_accessory.len();
                for (index, row) in shop.mid_accessory.keys().copied().enumerate() {
                    boxes.push((format!("head_mid_{row}"), self.place(accessory_at(mid + index), ACCESSORY_ROW)));
                }
                let low = mid + shop.mid_accessory.len();
                for (index, row) in shop.bottom_accessory.keys().copied().enumerate() {
                    boxes.push((format!("head_bottom_{row}"), self.place(accessory_at(low + index), ACCESSORY_ROW)));
                }
            }
        }
        boxes
    }

    pub fn hit(&self, cursor: [f32; 2], shop: &StylingShop, job: u16) -> Option<Hit> {
        let local = self.frame.local(cursor, HEIGHT)?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(Hit::Title(hit));
        }
        if self.frame.minimized {
            return Some(Hit::Window);
        }
        if contains(TAB_AT, TAB_SIZE, local) {
            return Some(Hit::Tab(Tab::Hair));
        }
        if contains([TAB_AT[0] + TAB_STEP, TAB_AT[1]], TAB_SIZE, local) {
            return Some(Hit::Tab(Tab::Accessory));
        }
        // The second-costume tab is drawn and does nothing.
        if contains(BUY_AT, BUY_SIZE, local) {
            return Some(Hit::Buy);
        }
        if contains(REFRESH_AT, REFRESH_SIZE, local) {
            return Some(Hit::Refresh);
        }
        if contains(ROTATE_LEFT, ROTATE_SIZE, local) {
            return Some(Hit::Rotate(1));
        }
        if contains(ROTATE_RIGHT, ROTATE_SIZE, local) {
            return Some(Hit::Rotate(-1));
        }
        match self.tab {
            Tab::Hair => {
                if let Some(row) =
                    self.style_rows(shop, job).into_iter().enumerate().find(|(index, _)| contains(style_at(*index), STYLE_SIZE, local)).map(|(_, row)| row)
                {
                    return Some(Hit::HairStyle(row));
                }
                if let Some(row) =
                    shop.head_palette.keys().copied().enumerate().find(|(index, _)| contains(chip_at(COLOR_AT, *index), COLOR_SIZE, local)).map(|(_, row)| row)
                {
                    return Some(Hit::HairColor(row));
                }
                if let Some(row) = clothes(shop, job)
                    .keys()
                    .copied()
                    .enumerate()
                    .find(|(index, _)| contains(chip_at(CLOTHES_AT, *index), COLOR_SIZE, local))
                    .map(|(_, row)| row)
                {
                    return Some(Hit::ClothesColor(row));
                }
                if styles(shop, job).len() > VISIBLE_STYLES && contains([STYLE_AT[0] + STYLE_SIZE[0] - 16.0, STYLE_AT[1] - 16.0], [16.0, 14.0], local) {
                    return Some(Hit::Scroll(-1));
                }
            }
            Tab::Accessory => {
                if let Some(row) =
                    shop.top_accessory.keys().copied().enumerate().find(|(index, _)| contains(accessory_at(*index), ACCESSORY_ROW, local)).map(|(_, row)| row)
                {
                    return Some(Hit::HeadTop(row));
                }
                let mid = shop.top_accessory.len();
                if let Some(row) = shop
                    .mid_accessory
                    .keys()
                    .copied()
                    .enumerate()
                    .find(|(index, _)| contains(accessory_at(mid + *index), ACCESSORY_ROW, local))
                    .map(|(_, row)| row)
                {
                    return Some(Hit::HeadMid(row));
                }
                let low = mid + shop.mid_accessory.len();
                if let Some(row) = shop
                    .bottom_accessory
                    .keys()
                    .copied()
                    .enumerate()
                    .find(|(index, _)| contains(accessory_at(low + *index), ACCESSORY_ROW, local))
                    .map(|(_, row)| row)
                {
                    return Some(Hit::HeadBottom(row));
                }
            }
        }
        Some(Hit::Window)
    }

    pub fn click(&mut self, hit: Hit, shop: &StylingShop, job: u16, hair: u16, zeny: i64, has: impl Fn(u32) -> u32) -> Option<Action> {
        let toggle = |slot: &mut u16, row: u16| {
            *slot = if *slot == row { 0 } else { row };
        };
        match hit {
            Hit::Title(TitleHit::Close) => return Some(Action::Close),
            Hit::Title(title) => self.frame.click_title(title),
            Hit::Tab(tab) => {
                self.tab = tab;
                self.scroll = 0;
            }
            Hit::HairStyle(row) => toggle(&mut self.picks.hair_style, row),
            Hit::HairColor(row) => toggle(&mut self.picks.hair_color, row),
            Hit::ClothesColor(row) => toggle(&mut self.picks.clothes_color, row),
            Hit::HeadTop(row) => toggle(&mut self.picks.head_top, row),
            Hit::HeadMid(row) => toggle(&mut self.picks.head_mid, row),
            Hit::HeadBottom(row) => toggle(&mut self.picks.head_bottom, row),
            Hit::Refresh => self.picks = Picks::default(),
            Hit::Rotate(step) => self.direction = (self.direction as i8 + step).rem_euclid(8) as u8,
            Hit::Scroll(step) => {
                let len = styles(shop, job).len();
                let max = len.saturating_sub(VISIBLE_STYLES);
                self.scroll = (self.scroll as i32 + i32::from(step)).clamp(0, max as i32) as usize;
            }
            Hit::Buy => {
                if !self.picks.any() {
                    return None;
                }
                if let Some(message) = refusal(shop, job, hair, self.picks, zeny, has) {
                    return Some(Action::Refuse(message));
                }
                return Some(Action::Buy(self.picks.request()));
            }
            Hit::Window => {}
        }
        None
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &'a self,
        layer: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        items: &HashMap<u32, ItemInfo>,
        shop: &StylingShop,
        job: u16,
        messages: &[String],
        font: &Font,
        pointer: ([f32; 2], bool),
        placements: &[Placed],
    ) {
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        let at = |local: [f32; 2]| self.frame.at(local);
        self.frame.draw_title(layer, chrome, message(TITLE), pointer.0);
        if self.frame.minimized {
            return;
        }
        layer.sprites.push((&assets.background, at([0.0, TITLE_HEIGHT]), [WIDTH, BODY]));
        let (cursor, down) = pointer;
        let hit = self.hit(cursor, shop, job);
        let pressed = |target: Hit| usize::from(hit == Some(target)) + usize::from(hit == Some(target) && down);
        for (index, tab) in [Tab::Hair, Tab::Accessory].into_iter().enumerate() {
            let state = if self.tab == tab { 1 } else { [0, 2, 2][pressed(Hit::Tab(tab)).min(2)] };
            let image = &assets.tabs[index][state];
            layer.sprites.push((image, at([TAB_AT[0] + index as f32 * TAB_STEP, TAB_AT[1]]), TAB_SIZE));
        }
        layer.sprites.push((&assets.costume_off, at([TAB_AT[0] + 2.0 * TAB_STEP, TAB_AT[1]]), TAB_SIZE));

        layer.sprites.push((&assets.shadow, at([PREVIEW_FEET[0] - 24.0, PREVIEW_FEET[1] - 8.0]), [48.0, 16.0]));
        if let Some(figure) = &self.figure {
            layer.sprites.extend(placements.iter().map(|(key, position, size, _)| (figure.texture(*key), at(offset(PREVIEW_FEET, *position)), *size)));
        }
        for (index, step) in [(0, 1i8), (1, -1)] {
            let image = &assets.rotate[index][pressed(Hit::Rotate(step)).min(2)];
            let place = if index == 0 { ROTATE_LEFT } else { ROTATE_RIGHT };
            layer.sprites.push((image, at(place), ROTATE_SIZE));
        }
        if hit == Some(Hit::Rotate(1)) {
            window::tooltip(layer, chrome, font, message(ROTATE_LEFT_LABEL), offset(at(ROTATE_LEFT), [0.0, -24.0]), WHITE);
        }
        if hit == Some(Hit::Rotate(-1)) {
            window::tooltip(layer, chrome, font, message(ROTATE_RIGHT_LABEL), offset(at(ROTATE_RIGHT), [0.0, -24.0]), WHITE);
        }

        match self.tab {
            Tab::Hair => {
                layer.texts.push((message(HAIR_STYLE_LABEL).to_owned(), at([STYLE_AT[0], STYLE_AT[1] - 14.0]), BLACK));
                for (index, row) in self.style_rows(shop, job).into_iter().enumerate() {
                    let place = style_at(index);
                    if self.picks.hair_style == row {
                        layer.sprites.push((&assets.selected, at(place), STYLE_SIZE));
                    }
                    let goods = styles(shop, job).get(&row).map(|row| row.goods).unwrap_or(0);
                    layer.texts.push((format!("{row}: {goods}"), at(offset(place, [4.0, 1.0])), BLACK));
                }
                self.draw_colors(layer, assets, &shop.head_palette, COLOR_AT, self.picks.hair_color, message(HAIR_COLOR_LABEL));
                self.draw_colors(layer, assets, clothes(shop, job), CLOTHES_AT, self.picks.clothes_color, message(CLOTHES_LABEL));
            }
            Tab::Accessory => {
                layer.texts.push((message(ACCESSORY_LABEL).to_owned(), at([ACCESSORY_AT[0], ACCESSORY_AT[1] - 14.0]), BLACK));
                let draw_list = |layer: &mut Layer<'a>, rows: &BTreeMap<u16, StyleRow>, start: usize, picked: u16| {
                    for (index, (row, entry)) in rows.iter().enumerate() {
                        let place = accessory_at(start + index);
                        if picked == *row {
                            layer.sprites.push((&assets.selected, at(place), ACCESSORY_ROW));
                        }
                        let name = items.get(&entry.goods).map(|info| info.name.as_str()).filter(|name| !name.is_empty()).unwrap_or("");
                        let label = if name.is_empty() { format!("{row}: {}", entry.goods) } else { format!("{row}: {name}") };
                        layer.texts.push((label, at(offset(place, [4.0, 1.0])), BLACK));
                    }
                };
                draw_list(layer, &shop.top_accessory, 0, self.picks.head_top);
                draw_list(layer, &shop.mid_accessory, shop.top_accessory.len(), self.picks.head_mid);
                draw_list(layer, &shop.bottom_accessory, shop.top_accessory.len() + shop.mid_accessory.len(), self.picks.head_bottom);
            }
        }

        let quote = quote(shop, job, self.picks);
        layer.sprites.push((&assets.zeny, at(PRICE_AT), [16.0, 16.0]));
        let mut price = quote.zeny.to_string();
        for (item, box_item) in &quote.coupons {
            let id = if *item != 0 { *item } else { *box_item };
            let name = items.get(&id).map(|info| info.name.as_str()).filter(|name| !name.is_empty()).map(str::to_owned).unwrap_or_else(|| id.to_string());
            price.push_str(&format!(" + {name}"));
        }
        layer.texts.push((price, at(offset(PRICE_AT, [20.0, 1.0])), BLACK));
        layer.texts.push((message(HINT).to_owned(), at([16.0, TITLE_HEIGHT + 248.0]), BLACK));

        let buy = &assets.buy[pressed(Hit::Buy).min(2)];
        layer.sprites.push((buy, at(BUY_AT), BUY_SIZE));
        let refresh = &assets.refresh[pressed(Hit::Refresh).min(2)];
        layer.sprites.push((refresh, at(REFRESH_AT), REFRESH_SIZE));
    }

    fn draw_colors<'a>(&self, layer: &mut Layer<'a>, assets: &'a Assets, rows: &BTreeMap<u16, StyleRow>, origin: [f32; 2], picked: u16, label: &str) {
        let at = |local: [f32; 2]| self.frame.at(local);
        layer.texts.push((label.to_owned(), at([origin[0], origin[1] - 14.0]), BLACK));
        let last = assets.colors.len().saturating_sub(1);
        for (index, (row, entry)) in rows.iter().enumerate() {
            // The chip is the color the row applies (clothes row 1 is color 0).
            let chip = (entry.goods as usize).min(last).min(8);
            let state = usize::from(picked == *row);
            if let Some(images) = assets.colors.get(chip) {
                layer.sprites.push((&images[state], at(chip_at(origin, index)), COLOR_SIZE));
            }
        }
    }
}

fn style_at(index: usize) -> [f32; 2] {
    [STYLE_AT[0], STYLE_AT[1] + index as f32 * STYLE_SIZE[1]]
}

fn chip_at(origin: [f32; 2], index: usize) -> [f32; 2] {
    [origin[0] + index as f32 * COLOR_STEP, origin[1]]
}

fn accessory_at(index: usize) -> [f32; 2] {
    [ACCESSORY_AT[0], ACCESSORY_AT[1] + index as f32 * ACCESSORY_ROW[1]]
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

    #[test]
    fn prices_the_picks_and_refuses_what_it_cant_pay() {
        let shop = shop();
        let picks = Picks { hair_color: 3, clothes_color: 2, ..Picks::default() };
        let cost = quote(&shop, 0, picks);
        assert_eq!(cost.zeny, 100_000);
        assert_eq!(cost.coupons, vec![(6046, 16854)]);
        assert_eq!(refusal(&shop, 0, 1, picks, 100_000, |_| 0), Some(NO_COUPON));
        assert_eq!(refusal(&shop, 0, 1, picks, 99_999, |_| 1), Some(NO_ZENY));
        assert_eq!(refusal(&shop, 0, 1, picks, 100_000, |id| u32::from(id == 6046)), None);
        // The box stands in for the coupon.
        assert_eq!(refusal(&shop, 0, 1, picks, 100_000, |id| u32::from(id == 16854)), None);
        // Two rows that need the same coupon need two of them.
        let both = Picks { hair_color: 3, clothes_color: 1, ..Picks::default() };
        let _ = both;
        let doubled = Picks { clothes_color: 2, hair_style: 24, ..Picks::default() };
        // Different coupons: each is checked on its own.
        assert_eq!(quote(&shop, 0, doubled).coupons, vec![(7622, 16843), (6046, 16854)]);
        assert_eq!(refusal(&shop, 0, 1, doubled, 0, |id| u32::from(id == 7622 || id == 6046)), None);
        let twice = Picks { hair_style: 24, ..Picks::default() };
        // One row, one coupon.
        assert_eq!(refusal(&shop, 0, 1, twice, 0, |id| u32::from(id == 7622)), None);
        // Hat and a beard add their zeny.
        let gear = Picks { head_top: 1, head_bottom: 1, ..Picks::default() };
        assert_eq!(quote(&shop, 0, gear).zeny, 6_000);
        assert!(quote(&shop, 0, gear).coupons.is_empty());
    }

    #[test]
    fn two_of_the_same_coupon_need_two() {
        let mut shop = shop();
        // Hair style 24 and clothes row 2 both ask for coupon 7622 (or its box).
        shop.body_palette.insert(2, row(2, 0, 7622, 16843, None));
        let picks = Picks { hair_style: 24, clothes_color: 2, ..Picks::default() };
        assert_eq!(quote(&shop, 0, picks).coupons, vec![(7622, 16843), (7622, 16843)]);
        // One of the coupon and one of the box cover the two rows.
        assert_eq!(refusal(&shop, 0, 1, picks, 0, |_| 1), None);
        assert_eq!(refusal(&shop, 0, 1, picks, 0, |_| 0), Some(NO_COUPON));
        // One coupon and no box covers only one of the two rows.
        assert_eq!(refusal(&shop, 0, 1, picks, 0, |id| u32::from(id == 7622)), Some(NO_COUPON));
        assert_eq!(refusal(&shop, 0, 1, picks, 0, |_| 2), None);
    }

    #[test]
    fn refuses_a_hair_color_on_a_style_that_cant_be_dyed() {
        let shop = shop();
        let dyed = Picks { hair_style: 6, hair_color: 1, ..Picks::default() };
        assert_eq!(refusal(&shop, 0, 1, dyed, 9_000_000, |_| 1), Some(CANT_DYE));
        // The style on its own is fine.
        assert_eq!(refusal(&shop, 0, 1, Picks { hair_style: 6, ..Picks::default() }, 9_000_000, |_| 1), None);
        // The current hair is that style, and only a color is picked.
        assert_eq!(refusal(&shop, 0, 6, Picks { hair_color: 1, ..Picks::default() }, 9_000_000, |_| 1), Some(CANT_DYE));
        // A Doram style with isColoring false.
        let doram = Picks { hair_style: 2, hair_color: 1, ..Picks::default() };
        assert_eq!(refusal(&shop, 4218, 1, doram, 9_000_000, |_| 1), Some(CANT_DYE));
        // The same row on a human has no such flag (row 5), so the color is allowed.
        assert_eq!(refusal(&shop, 0, 1, Picks { hair_style: 5, hair_color: 1, ..Picks::default() }, 200_000, |_| 1), None);
    }

    #[test]
    fn the_request_leaves_body_style_at_zero_and_the_buttons_are_named() {
        let shop = shop();
        let mut window = Window::open();
        assert_eq!(window.click(Hit::HairColor(3), &shop, 0, 1, 0, |_| 0), None);
        assert_eq!(window.click(Hit::HairColor(3), &shop, 0, 1, 0, |_| 0), None);
        assert_eq!(window.picks.hair_color, 0);
        window.click(Hit::HairStyle(5), &shop, 0, 1, 0, |_| 0);
        window.click(Hit::ClothesColor(2), &shop, 0, 1, 0, |_| 0);
        window.click(Hit::Refresh, &shop, 0, 1, 0, |_| 0);
        assert_eq!(window.picks, Picks::default());
        window.click(Hit::HairColor(3), &shop, 0, 1, 0, |_| 0);
        window.click(Hit::ClothesColor(2), &shop, 0, 1, 0, |_| 0);
        assert_eq!(window.click(Hit::Buy, &shop, 0, 1, 0, |_| 0), Some(Action::Refuse(NO_ZENY)));
        assert_eq!(window.click(Hit::Buy, &shop, 0, 1, 50_000, |_| 1), Some(Action::Refuse(NO_ZENY)));
        match window.click(Hit::Buy, &shop, 0, 1, 100_000, |id| u32::from(id == 6046)) {
            Some(Action::Buy(picks)) => {
                assert_eq!((picks.hair_color, picks.clothes_color, picks.body_style, picks.hair_style), (3, 2, 0, 0));
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(window.click(Hit::Title(TitleHit::Close), &shop, 0, 1, 0, |_| 0), Some(Action::Close));
        let names: Vec<_> = window.button_boxes(&shop, 0).into_iter().map(|(name, _)| name).collect();
        for name in ["hair_style_5", "hair_color_3", "clothes_color_2", "tab_hair", "tab_accessory", "buy", "refresh", "rotate_left", "rotate_right", "close"] {
            assert!(names.iter().any(|got| got == name), "{name} in {names:?}");
        }
        assert!(names.iter().all(|name| !name.contains("body") && !name.contains("costume")));
    }
}
