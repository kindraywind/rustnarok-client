//! Laphine boxes: synthesis (several materials in, a reward out) and upgrade (one item's refine raised). Both are one
//! 400×250 picture with its own title bar: a text plate and the slots on the left, the carried items the box takes in
//! a list on the right, Cancel and Make in the footer and a notice line beside them.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use rustnarok_client::lua::{self, ItemInfo};
use rustnarok_client::messages;
use rustnarok_client::net::InventoryItem;
use rustnarok_client::text::Font;

use crate::inventory;
use crate::render::{Gpu, Texture};
use crate::window::{self, Chrome, Clicks, Frame, TitleHit};
use crate::{Layer, UI, contains, offset};

pub const SIZE: [f32; 2] = [400.0, 250.0];
const BASE_BUTTON: [f32; 2] = [3.0, 3.0];
const BUTTON: [f32; 2] = [56.0, 20.0];
const CANCEL_AT: [f32; 2] = [280.0, 226.0];
const MAKE_AT: [f32; 2] = [340.0, 226.0];
const MOVE_AT: [f32; 2] = [213.0, 110.0];
const MOVE_SIZE: [f32; 2] = [20.0, 20.0];
/// The notice line: its icon, its text, how long it stays.
const NOTICE_ICON: [f32; 2] = [6.0, 229.0];
const NOTICE_TEXT: [f32; 2] = [25.0, 230.0];
const NOTICE_COLOR: [u8; 3] = [0, 0, 255];
const NOTICE_TIME: Duration = Duration::from_millis(3000);
/// The plate's two lines, centred in `PLATE` (x, width) at these heights.
const LINES_Y: [f32; 2] = [47.0, 63.0];
const LINE_COLORS: [[u8; 3]; 2] = [[81, 122, 205], [66, 74, 132]];
const SYNTHESIS_PLATE: [f32; 2] = [33.0, 160.0];
const UPGRADE_PLATE: [f32; 2] = [35.0, 156.0];
/// Synthesis's "in / needed" counter: each number centred in 14 px from its x.
const COUNT_X: [f32; 2] = [98.0, 115.0];
const COUNT_Y: f32 = 76.0;
const SLOT: f32 = 24.0;
const UPGRADE_SLOT: [f32; 2] = [101.0, 165.0];
/// The list: the area, its rows from (0, 7) one under the other, the scroll bar at its right.
const LIST: [f32; 2] = [241.0, 17.0];
const LIST_SIZE: [f32; 2] = [150.0, 204.0];
const ROW: [f32; 2] = [136.0, 30.0];
const ROW_TOP: f32 = 7.0;
// ponytail: the game scrolls the list by pixels and clips the row cut by its bottom; here it steps whole rows and shows
// the six that fit.
const SHOWN: usize = 6;
const BAR_X: f32 = 141.0;
const BAR_WIDTH: f32 = 9.0;
const PIECE: f32 = 3.0;
const ROW_FILL: [u8; 4] = [247, 247, 255, 255];
const NAME_WIDTH: f32 = 100.0;
const GREY: [u8; 3] = [150, 150, 150];
const OUTLINE: [u8; 3] = [224, 224, 224];

/// Line 1 of the plate: "Synthesis of the required materials".
pub const LINE: usize = 2896;
/// "Insufficient synthesis materials": a refused synthesis.
pub const NOT_ENOUGH: usize = 2897;
/// "Required material %d ea.": a stack too small to go in.
pub const NEED: usize = 2898;
/// An upgrade's item with more refine than the box asks for, or with cards or enchants in: it goes in anyway.
pub const REFINE_ABOVE: usize = 3645;
pub const CARDS: usize = 3646;

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(TitleHit),
    /// A place in the list (row number from the top of the whole list), whether or not a row is there.
    Row(usize),
    List,
    Thumb,
    Slot(usize),
    MoveItem,
    Make,
    Close,
    Window,
}

#[derive(Debug, PartialEq)]
pub enum Action {
    Make,
    Close,
}

/// What a press starts dragging: a row's item or a slot's (inventory index).
#[derive(Debug, PartialEq)]
pub enum Grab {
    Row(u16),
    Slot(u16),
}

pub struct Assets {
    /// Synthesis, then upgrade.
    background: [Option<Texture>; 2],
    /// Out, over, press, disabled.
    make: [Option<Texture>; 4],
    cancel: [Option<Texture>; 4],
    move_item: [Option<Texture>; 4],
    selected: Option<Texture>,
    shadow: Option<Texture>,
    notify: Option<Texture>,
    /// The bar's top, middle and bottom; the thumb's, out, under the pointer and held.
    bar: [Option<Texture>; 3],
    thumb: [[Option<Texture>; 3]; 3],
    row_fill: Texture,
    /// Laid over a dimmed row's icon.
    fade: Texture,
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &rustnarok_client::grf::Grf) -> Self {
        let image = |name: &str| gpu.try_load_texture(grf, &format!("{UI}lapine\\{name}.bmp"), false).ok();
        let states = |name: &str| ["out", "over", "press", "dis"].map(|state| image(&format!("{name}_{state}")));
        let thumb = |state: &str| ["top", "mid", "btm"].map(|piece| image(&format!("scrollbar_thumb_{state}_{piece}")));
        Self {
            background: [image("bg_lapine"), image("bg_lapineupgrade")],
            make: states("btn_make"),
            cancel: states("btn_cancel"),
            move_item: states("btn_moveitem"),
            selected: image("list_selected_item"),
            shadow: image("shadow_item_off"),
            notify: image("icon_notify"),
            bar: ["top", "mid", "btm"].map(|piece| image(&format!("scrollbar_bg_{piece}"))),
            thumb: [thumb("out"), thumb("over"), thumb("press")],
            row_fill: gpu.solid(ROW_FILL),
            fade: gpu.solid([255, 255, 255, 0x90]),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Placed {
    index: u16,
    amount: u16,
}

/// A row of the list: a carried stack, and whether it can go in now.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub index: u16,
    pub usable: bool,
}

pub struct Laphine {
    pub frame: Frame,
    pub box_id: u32,
    upgrade: bool,
    need_count: usize,
    refine: [u32; 2],
    option_min: usize,
    /// The box's own line under 2896.
    need_text: String,
    /// The kinds that go in, with the amount each slot takes (1 for an upgrade).
    sources: Vec<(u32, u16)>,
    slots: Vec<Placed>,
    scroll: usize,
    picked: Option<u16>,
    row_clicks: Clicks,
    slot_clicks: Clicks,
    /// The thumb held: the pointer's height at the press and the first row then.
    thumb: Option<(f32, usize)>,
    /// Make was sent and the server hasn't answered.
    waiting: bool,
    /// The notice line: a message, its number, since when.
    notice: Option<(usize, u16, Instant)>,
}

impl Laphine {
    fn new(upgrade: bool, box_id: u32, need_count: usize, refine: [u32; 2], need_text: &str, sources: Vec<(u32, u16)>) -> Self {
        let screen = crate::screen();
        let mut frame = Frame::new([((screen[0] - SIZE[0]) / 2.0).floor(), ((screen[1] - SIZE[1]) / 2.0).floor()], SIZE[0]);
        frame.open = true;
        Self {
            frame,
            box_id,
            upgrade,
            need_count,
            refine,
            option_min: 0,
            need_text: need_text.to_owned(),
            sources,
            slots: Vec::new(),
            scroll: 0,
            picked: None,
            row_clicks: Clicks::default(),
            slot_clicks: Clicks::default(),
            thumb: None,
            waiting: false,
            notice: None,
        }
    }

    pub fn synthesis(box_id: u32, info: &lua::SynthesisBox) -> Self {
        let sources = info.sources.iter().map(|source| (source.id, source.amount)).collect();
        Self::new(false, box_id, info.need_count as usize, [info.refine_min, info.refine_max], &info.need_text, sources)
    }

    pub fn upgrade(box_id: u32, info: &lua::UpgradeBox) -> Self {
        let sources = info.targets.iter().map(|target| (target.id, 1)).collect();
        let mut window = Self::new(true, box_id, 1, [info.refine_min, info.refine_max], &info.need_text, sources);
        window.option_min = info.option_min as usize;
        window
    }

    /// Make can be pressed: every item in and no answer pending.
    pub fn ready(&self) -> bool {
        self.need_count > 0 && self.slots.len() == self.need_count && !self.waiting
    }

    /// Synthesis's request: (inventory index, amount) per slot.
    pub fn entries(&self) -> Vec<(u16, u16)> {
        self.slots.iter().map(|slot| (slot.index, slot.amount)).collect()
    }

    /// The upgrade's item, by inventory index.
    pub fn index(&self) -> Option<u16> {
        self.slots.first().map(|slot| slot.index)
    }

    fn source(&self, id: u32) -> Option<u16> {
        self.sources.iter().find(|(source, _)| *source == id).map(|(_, amount)| *amount)
    }

    fn placed(&self, index: u16) -> Option<&Placed> {
        self.slots.iter().find(|slot| slot.index == index)
    }

    /// The list: every carried, unworn stack of a kind the box takes. An upgrade's item that is in leaves it; so does
    /// a synthesis stack that went in whole. Synthesis dims the rows that can't go in (the rest of a stack in, too few,
    /// refine out of range); upgrade dims none.
    pub fn rows(&self, carried: &[InventoryItem]) -> Vec<Row> {
        carried
            .iter()
            .filter(|item| item.worn == 0)
            .filter_map(|item| {
                let need = self.source(item.id)?;
                let left = item.amount.saturating_sub(self.placed(item.index).map_or(0, |slot| slot.amount));
                if self.placed(item.index).is_some() && (self.upgrade || left == 0) {
                    return None;
                }
                let refined = (self.refine[0]..=self.refine[1]).contains(&u32::from(item.refine));
                let usable = self.upgrade || (self.placed(item.index).is_none() && item.amount >= need && refined);
                Some(Row { index: item.index, usable })
            })
            .collect()
    }

    fn row_at(&self, row: usize, carried: &[InventoryItem]) -> Option<Row> {
        self.rows(carried).into_iter().nth(row)
    }

    /// Puts a carried stack in, as the list allows; refusals mostly say nothing.
    pub fn put(&mut self, item: &InventoryItem, now: Instant) -> bool {
        let Some(need) = self.source(item.id) else { return false };
        if self.slots.len() >= self.need_count || self.placed(item.index).is_some() || item.worn != 0 {
            return false;
        }
        if !(self.refine[0]..=self.refine[1]).contains(&u32::from(item.refine)) {
            return false;
        }
        if self.upgrade {
            if item.options.len() < self.option_min {
                return false;
            }
            if item.cards.iter().any(|card| *card != 0) {
                self.notice = Some((CARDS, 0, now));
            } else if self.refine[0] != 0 && u32::from(item.refine) > self.refine[0] {
                self.notice = Some((REFINE_ABOVE, 0, now));
            }
        } else if item.amount < need {
            self.notice = Some((NEED, need, now));
            return false;
        }
        self.slots.push(Placed { index: item.index, amount: need });
        if self.picked == Some(item.index) {
            self.picked = None;
        }
        true
    }

    /// Takes an item back out; its row comes back (an upgrade's notice goes with it).
    pub fn take_out(&mut self, index: u16) {
        self.slots.retain(|slot| slot.index != index);
        if self.upgrade {
            self.notice = None;
        }
    }

    fn put_index(&mut self, index: u16, carried: &[InventoryItem], now: Instant) {
        if let Some(item) = carried.iter().find(|item| item.index == index) {
            self.put(item, now);
        }
    }

    fn move_enabled(&self) -> bool {
        self.slots.len() < self.need_count && (!self.upgrade || self.picked.is_some())
    }

    fn range(&self, rows: usize) -> usize {
        rows.saturating_sub(SHOWN)
    }

    pub fn scroll_by(&mut self, rows: i32, carried: &[InventoryItem]) {
        let range = self.range(self.rows(carried).len());
        self.scroll = (self.scroll as i32 + rows).clamp(0, range as i32) as usize;
    }

    /// The thumb's top (from the list's) and length for `rows` rows: the share shown of the bar's height, 9 px at least.
    fn thumb_box(&self, rows: usize) -> [f32; 2] {
        let length = (LIST_SIZE[1] * SHOWN as f32 / rows.max(1) as f32).floor().clamp(3.0 * PIECE, LIST_SIZE[1]);
        let range = self.range(rows);
        let top = if range == 0 { 0.0 } else { ((LIST_SIZE[1] - length) * self.scroll.min(range) as f32 / range as f32).floor() };
        [top, length]
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let local = self.frame.local(cursor, SIZE[1])?;
        // The picture's bar has the base and close buttons only: where others have minimize, it is plain title bar.
        match self.frame.title_hit(local) {
            Some(TitleHit::Minimize) => return Some(Hit::Title(TitleHit::Title)),
            Some(hit) => return Some(Hit::Title(hit)),
            None => {}
        }
        let checks = [(MAKE_AT, BUTTON, Hit::Make), (CANCEL_AT, BUTTON, Hit::Close), (MOVE_AT, MOVE_SIZE, Hit::MoveItem)];
        if let Some((_, _, hit)) = checks.into_iter().find(|(at, size, _)| contains(*at, *size, local)) {
            return Some(hit);
        }
        if contains(LIST, LIST_SIZE, local) {
            let [x, y] = [local[0] - LIST[0], local[1] - LIST[1]];
            if (BAR_X..BAR_X + BAR_WIDTH).contains(&x) {
                // ponytail: the thumb's box is taken for the whole bar's here; a press on the track does nothing.
                return Some(Hit::Thumb);
            }
            if x < ROW[0] && y >= ROW_TOP && y < ROW_TOP + SHOWN as f32 * ROW[1] {
                return Some(Hit::Row(self.scroll + ((y - ROW_TOP) / ROW[1]) as usize));
            }
            return Some(Hit::List);
        }
        let slot = (0..self.slots.len()).find(|&slot| contains(self.slot_at(slot), [SLOT; 2], local));
        Some(slot.map_or(Hit::Window, Hit::Slot))
    }

    /// Where slot `slot` sits: synthesis lays up to five in a row, more in two rows; upgrade has its one place.
    fn slot_at(&self, slot: usize) -> [f32; 2] {
        if self.upgrade {
            return UPGRADE_SLOT;
        }
        slot_place(slot, self.slots.len())
    }

    /// A click (a press let go where it was): a row is picked, a double click puts it in or takes a slot out.
    pub fn click(&mut self, hit: Hit, carried: &[InventoryItem], now: Instant) -> Option<Action> {
        match hit {
            Hit::Title(TitleHit::Close) | Hit::Close => return Some(Action::Close),
            Hit::Title(title) => self.frame.click_title(title),
            Hit::Row(row) => {
                if let Some(row) = self.row_at(row, carried).filter(|row| row.usable) {
                    self.picked = Some(row.index);
                    if self.row_clicks.double(row.index) {
                        self.put_index(row.index, carried, now);
                    }
                }
            }
            Hit::Slot(slot) => {
                if let Some(index) = self.slots.get(slot).map(|slot| slot.index)
                    && self.slot_clicks.double(index)
                {
                    self.take_out(index);
                }
            }
            Hit::MoveItem if self.move_enabled() => {
                if let Some(index) = self.picked {
                    self.put_index(index, carried, now);
                }
            }
            Hit::Make if self.ready() => {
                self.waiting = true;
                return Some(Action::Make);
            }
            Hit::MoveItem | Hit::Make | Hit::List | Hit::Thumb | Hit::Window => {}
        }
        None
    }

    /// A press: the title bar grabs the window, the thumb its list, a row or a slot its item.
    pub fn press(&mut self, cursor: [f32; 2], carried: &[InventoryItem]) -> Option<Grab> {
        match self.hit(cursor)? {
            Hit::Title(TitleHit::Title) => self.frame.start_drag(cursor),
            Hit::Thumb if self.range(self.rows(carried).len()) > 0 => self.thumb = Some((cursor[1], self.scroll)),
            Hit::Row(row) => return self.row_at(row, carried).filter(|row| row.usable).map(|row| Grab::Row(row.index)),
            Hit::Slot(slot) => return self.slots.get(slot).map(|slot| Grab::Slot(slot.index)),
            _ => {}
        }
        None
    }

    /// The thumb follows the pointer while held: the bar's free length stands for the rows that don't fit.
    pub fn drag_thumb(&mut self, y: f32, carried: &[InventoryItem]) {
        let Some((from, first)) = self.thumb else { return };
        let rows = self.rows(carried).len();
        let free = LIST_SIZE[1] - self.thumb_box(rows)[1];
        let range = self.range(rows);
        let moved = if free > 0.0 { ((y - from) * range as f32 / free).round() as i64 } else { 0 };
        self.scroll = (first as i64 + moved).clamp(0, range as i64) as usize;
    }

    pub fn drop_thumb(&mut self) -> bool {
        self.thumb.take().is_some()
    }

    /// A dragged row let go at `cursor`: on the window outside the list it goes in.
    pub fn drop_row(&mut self, index: u16, cursor: [f32; 2], carried: &[InventoryItem], now: Instant) {
        if matches!(self.hit(cursor), Some(hit) if !matches!(hit, Hit::Row(_) | Hit::List | Hit::Thumb)) {
            self.put_index(index, carried, now);
        }
    }

    /// A dragged slot let go at `cursor`: on the list it comes out.
    pub fn drop_slot(&mut self, index: u16, cursor: [f32; 2]) {
        if matches!(self.hit(cursor), Some(Hit::Row(_) | Hit::List | Hit::Thumb)) {
            self.take_out(index);
        }
    }

    /// The carried item under the pointer, a row's or a slot's (for its description).
    pub fn item_at(&self, cursor: [f32; 2], carried: &[InventoryItem]) -> Option<u16> {
        match self.hit(cursor)? {
            Hit::Row(row) => self.row_at(row, carried).map(|row| row.index),
            Hit::Slot(slot) => self.slots.get(slot).map(|slot| slot.index),
            _ => None,
        }
    }

    /// The answer to Make: whether the window closes (0). Otherwise synthesis may make again, and 5 or 7 shows 2897
    /// on the notice line; an upgrade stays waiting. Nothing goes to the chat.
    pub fn answer(&mut self, result: u16, now: Instant) -> bool {
        if result == 0 {
            return true;
        }
        if !self.upgrade {
            self.waiting = false;
            if matches!(result, 5 | 7) {
                self.notice = Some((NOT_ENOUGH, 0, now));
            }
        }
        false
    }

    /// The title: the box item's name.
    pub fn title<'a>(&self, items: &'a HashMap<u32, ItemInfo>) -> &'a str {
        items.get(&self.box_id).map_or("", |info| info.name(true))
    }

    fn notice_text(&self, messages: &[String], now: Instant) -> Option<String> {
        let (id, number, since) = self.notice?;
        (now.saturating_duration_since(since) < NOTICE_TIME).then(|| messages::fill(messages.get(id).map_or("", String::as_str), &[&number]))
    }

    pub fn button_boxes(&self) -> Vec<(String, [f32; 4])> {
        if !self.frame.open {
            return Vec::new();
        }
        let place = |at: [f32; 2], size: [f32; 2]| {
            let [x, y] = self.frame.at(at);
            [x, y, size[0], size[1]]
        };
        let mut boxes: Vec<_> = (0..self.slots.len()).map(|slot| (format!("slot_{slot}"), place(self.slot_at(slot), [SLOT; 2]))).collect();
        boxes.push(("make".into(), place(MAKE_AT, BUTTON)));
        boxes.push(("close".into(), place(CANCEL_AT, BUTTON)));
        boxes.push(("move_item".into(), place(MOVE_AT, MOVE_SIZE)));
        boxes
    }

    /// The rows shown on screen, by item ID.
    pub fn row_boxes(&self, carried: &[InventoryItem]) -> Vec<(u32, [f32; 4])> {
        let id = |index: u16| carried.iter().find(|item| item.index == index).map_or(0, |item| item.id);
        self.rows(carried)
            .into_iter()
            .enumerate()
            .skip(self.scroll)
            .take(SHOWN)
            .map(|(row, Row { index, .. })| {
                let [x, y] = self.frame.at(row_place(row - self.scroll));
                (id(index), [x, y, ROW[0], ROW[1]])
            })
            .collect()
    }

    pub fn icons(&self, items: &HashMap<u32, ItemInfo>, carried: &[InventoryItem]) -> Vec<String> {
        carried
            .iter()
            .filter(|item| self.source(item.id).is_some())
            .filter_map(|item| items.get(&item.id).map(|info| inventory::icon_path(info, item.identified)))
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &self,
        layer: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        icons: &'a HashMap<String, Option<Texture>>,
        items: &HashMap<u32, ItemInfo>,
        carried: &[InventoryItem],
        messages: &[String],
        font: &Font,
        pointer: ([f32; 2], bool),
    ) {
        let now = crate::control::now();
        let at = |local: [f32; 2]| self.frame.at(local);
        match &assets.background[usize::from(self.upgrade)] {
            Some(background) => layer.sprites.push((background, at([0.0, 0.0]), SIZE)),
            None => layer.sprites.push((&chrome.white, at([0.0, 0.0]), SIZE)),
        }
        // The picture's bar: the base and close buttons and the box's name.
        let base = chrome.button(0, contains(at(BASE_BUTTON), window::BUTTON_SIZE, pointer.0));
        layer.sprites.push((base, at(BASE_BUTTON), base.size));
        self.frame.draw_bare_title(layer, chrome, self.title(items), pointer.0);
        let carried_item = |index: u16| carried.iter().find(|item| item.index == index);
        let icon = |item: &InventoryItem| items.get(&item.id).and_then(|info| icons.get(&inventory::icon_path(info, item.identified))).and_then(Option::as_ref);
        let centred = |layer: &mut Layer<'a>, text: &str, [x, width]: [f32; 2], y: f32, color: [u8; 3]| {
            layer.texts.push((text.to_owned(), at([x + ((width - font.width(text)) / 2.0).floor(), y]), color));
        };
        // The plate: 2896, the box's own line, and synthesis's counter.
        let plate = if self.upgrade { UPGRADE_PLATE } else { SYNTHESIS_PLATE };
        centred(layer, messages.get(LINE).map_or("", String::as_str), plate, LINES_Y[0], LINE_COLORS[0]);
        centred(layer, &self.need_text, plate, LINES_Y[1], LINE_COLORS[1]);
        if !self.upgrade {
            for (number, x) in [self.slots.len(), self.need_count].into_iter().zip(COUNT_X) {
                centred(layer, &number.to_string(), [x, 14.0], COUNT_Y, [255; 3]);
            }
        }
        // The slots: only what is in, each on its shadow (synthesis) with its amount.
        let hit = self.hit(pointer.0);
        for (slot, placed) in self.slots.iter().enumerate() {
            let place = self.slot_at(slot);
            if !self.upgrade
                && let Some(shadow) = &assets.shadow
            {
                layer.sprites.push((shadow, at(offset(place, [-4.0, 11.0])), shadow.size));
            }
            let Some(item) = carried_item(placed.index) else { continue };
            if let Some(icon) = icon(item) {
                layer.sprites.push((icon, at(place), icon.size));
            }
            if !self.upgrade {
                amount(layer, at(place), placed.amount);
            }
        }
        // The list: a pale bed per row, the pick's highlight, the icon (synthesis: with what is left of the stack) and
        // the name cut to 100 px; dimmed rows grey.
        for (row, Row { index, usable }) in self.rows(carried).into_iter().enumerate().skip(self.scroll).take(SHOWN) {
            let Some(item) = carried_item(index) else { continue };
            let place = row_place(row - self.scroll);
            layer.sprites.push((&assets.row_fill, at(place), ROW));
            if self.picked == Some(index)
                && let Some(selected) = &assets.selected
            {
                layer.sprites.push((selected, at(offset(place, [32.0, 0.0])), selected.size));
            }
            let icon_at = at(offset(place, [4.0, 3.0]));
            if let Some(icon) = icon(item) {
                layer.sprites.push((icon, icon_at, icon.size));
                if !usable {
                    layer.sprites.push((&assets.fade, icon_at, icon.size));
                }
            }
            if !self.upgrade {
                amount(layer, icon_at, item.amount - self.placed(index).map_or(0, |slot| slot.amount));
            }
            let name = window::fit(font, items.get(&item.id).map_or("", |info| info.name(item.identified)), NAME_WIDTH);
            let corner = at(offset(place, [36.0, 9.0]));
            for step in [[-1.0, 0.0], [1.0, 0.0], [0.0, -1.0], [0.0, 1.0]] {
                layer.texts.push((name.clone(), offset(corner, step), OUTLINE));
            }
            layer.texts.push((name, corner, if usable { [0; 3] } else { GREY }));
        }
        self.draw_bar(layer, assets, self.rows(carried).len(), hit == Some(Hit::Thumb));
        // The buttons, and the notice line while it lasts.
        let state = |target: Hit, enabled: bool| match () {
            _ if !enabled => 3,
            _ if hit == Some(target) => 1 + usize::from(pointer.1),
            _ => 0,
        };
        for (states, place, look) in [
            (&assets.cancel, CANCEL_AT, state(Hit::Close, true)),
            (&assets.make, MAKE_AT, state(Hit::Make, self.ready())),
            (&assets.move_item, MOVE_AT, state(Hit::MoveItem, self.move_enabled())),
        ] {
            if let Some(image) = states[look].as_ref().or(states[0].as_ref()) {
                layer.sprites.push((image, at(place), image.size));
            }
        }
        if let Some(text) = self.notice_text(messages, now) {
            if let Some(notify) = &assets.notify {
                layer.sprites.push((notify, at(NOTICE_ICON), notify.size));
            }
            layer.texts.push((text, at(NOTICE_TEXT), NOTICE_COLOR));
        }
        // A slot under the pointer: its name (with its options), above it.
        if let Some(Hit::Slot(slot)) = hit
            && let Some(item) = self.slots.get(slot).and_then(|placed| carried_item(placed.index))
        {
            let name = items.get(&item.id).map_or("", |info| info.name(item.identified));
            let text = inventory::option_text(messages, name, item);
            window::tooltip(layer, chrome, font, &text, at(offset(self.slot_at(slot), [0.0, -20.0])), [255; 3]);
        }
    }

    /// The list's bar, its full height: the track's ends around repeated middles, and a thumb while rows don't fit.
    fn draw_bar<'a>(&self, layer: &mut Layer<'a>, assets: &'a Assets, rows: usize, hovered: bool) {
        let x = LIST[0] + BAR_X;
        let column = |layer: &mut Layer<'a>, [top, middle, bottom]: &'a [Option<Texture>; 3], y: f32, height: f32| {
            let (Some(top), Some(middle), Some(bottom)) = (top, middle, bottom) else { return };
            layer.sprites.push((top, self.frame.at([x, y]), top.size));
            let between = height - top.size[1] - bottom.size[1];
            layer.sprites.push((middle, self.frame.at([x, y + top.size[1]]), [BAR_WIDTH, between.max(0.0)]));
            layer.sprites.push((bottom, self.frame.at([x, y + height - bottom.size[1]]), bottom.size));
        };
        column(layer, &assets.bar, LIST[1], LIST_SIZE[1]);
        if self.range(rows) > 0 {
            let [top, length] = self.thumb_box(rows);
            let look = if self.thumb.is_some() { 2 } else { usize::from(hovered) };
            column(layer, &assets.thumb[look], LIST[1] + top, length);
        }
    }
}

/// Synthesis's slot `slot` with `count` in: up to five in a row from (33, 162), more in two rows from (25, 152) and
/// (42, 183), 34 px apart.
fn slot_place(slot: usize, count: usize) -> [f32; 2] {
    let step = 34.0 * (slot % 5) as f32;
    match (count > 5, slot >= 5) {
        (false, _) => [33.0 + step, 162.0],
        (true, false) => [25.0 + step, 152.0],
        (true, true) => [42.0 + step, 183.0],
    }
}

/// The `shown`th row on screen, in the window.
fn row_place(shown: usize) -> [f32; 2] {
    [LIST[0], LIST[1] + ROW_TOP + shown as f32 * ROW[1]]
}

/// An item's amount over its icon at `icon`, as the inventory draws it: black with a white outline.
fn amount(layer: &mut Layer, icon: [f32; 2], amount: u16) {
    let text = amount.to_string();
    let corner = offset(icon, offset(inventory::amount_at(amount), [-4.0, -4.0]));
    for step in [[-1.0, 0.0], [1.0, 0.0], [0.0, -1.0], [0.0, 1.0]] {
        layer.texts.push((text.clone(), offset(corner, step), [255; 3]));
    }
    layer.texts.push((text, corner, [0, 0, 0]));
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustnarok_client::net::ItemOption;

    fn carried(id: u32, index: u16, amount: u16, refine: u8) -> InventoryItem {
        InventoryItem { id, index, amount, refine, identified: true, ..Default::default() }
    }

    fn synthesis(need_count: u32, refine: [u32; 2]) -> Laphine {
        Laphine::synthesis(
            100000,
            &lua::SynthesisBox {
                need_count,
                refine_min: refine[0],
                refine_max: refine[1],
                need_text: "Jellopy, Fluff, Shell".into(),
                sources: vec![
                    lua::SynthesisSource { name: "Jellopy".into(), amount: 1, id: 909 },
                    lua::SynthesisSource { name: "Fluff".into(), amount: 2, id: 914 },
                    lua::SynthesisSource { name: "Shell".into(), amount: 1, id: 935 },
                ],
            },
        )
    }

    fn upgrade(refine: [u32; 2], option_min: u32) -> Laphine {
        Laphine::upgrade(
            100626,
            &lua::UpgradeBox {
                refine_min: refine[0],
                refine_max: refine[1],
                option_min,
                no_cards: true,
                need_text: String::new(),
                targets: vec![lua::UpgradeTarget { name: "Gray Wolf Suit".into(), id: 450177 }],
            },
        )
    }

    /// The window's local point `local` on screen.
    fn on(window: &Laphine, local: [f32; 2]) -> [f32; 2] {
        window.frame.at(local)
    }

    #[test]
    fn slots_sit_in_one_row_up_to_five_then_two() {
        assert_eq!(slot_place(0, 1), [33.0, 162.0]);
        assert_eq!((slot_place(0, 5), slot_place(4, 5)), ([33.0, 162.0], [169.0, 162.0]));
        assert_eq!((slot_place(0, 6), slot_place(4, 6), slot_place(5, 6)), ([25.0, 152.0], [161.0, 152.0], [42.0, 183.0]));
        assert_eq!(slot_place(9, 10), [178.0, 183.0]);
    }

    #[test]
    fn rows_and_what_a_point_hits() {
        let window = synthesis(3, [0, 10]);
        assert_eq!((row_place(0), row_place(1), row_place(5)), ([241.0, 24.0], [241.0, 54.0], [241.0, 174.0]));
        let hit = |local: [f32; 2]| window.hit(on(&window, local));
        assert_eq!(hit([241.0, 24.0]), Some(Hit::Row(0)));
        assert_eq!(hit([376.0, 53.9]), Some(Hit::Row(0)));
        assert_eq!(hit([300.0, 54.0]), Some(Hit::Row(1)));
        assert_eq!(hit([300.0, 20.0]), Some(Hit::List));
        assert_eq!(hit([378.0, 40.0]), Some(Hit::List));
        assert_eq!(hit([385.0, 40.0]), Some(Hit::Thumb));
        assert_eq!(hit([300.0, 210.0]), Some(Hit::List));
        assert_eq!(hit([281.0, 227.0]), Some(Hit::Close));
        assert_eq!(hit([341.0, 227.0]), Some(Hit::Make));
        assert_eq!(hit([214.0, 111.0]), Some(Hit::MoveItem));
        assert_eq!(hit([377.0, 5.0]), Some(Hit::Title(TitleHit::Title)));
        assert_eq!(hit([387.0, 5.0]), Some(Hit::Title(TitleHit::Close)));
        assert_eq!(hit([100.0, 170.0]), Some(Hit::Window));
        assert_eq!(hit([100.0, 250.0]), None);
        let items = HashMap::from([(100000, ItemInfo { name: "IDTest Special".into(), ..ItemInfo::default() })]);
        assert_eq!(window.title(&items), "IDTest Special");
        // Opened centred, the picture the whole window.
        let screen = crate::screen();
        assert_eq!(window.frame.position, [((screen[0] - 400.0) / 2.0).floor(), ((screen[1] - 250.0) / 2.0).floor()]);
    }

    #[test]
    fn synthesis_dims_what_cant_go_in() {
        let window = synthesis(3, [2, 6]);
        let bag = [carried(909, 1, 5, 2), carried(914, 2, 1, 2), carried(935, 3, 1, 1), carried(935, 4, 1, 7), carried(501, 5, 9, 0), carried(909, 6, 1, 6)];
        let rows = window.rows(&bag);
        let usable: Vec<_> = rows.iter().map(|row| (row.index, row.usable)).collect();
        assert_eq!(usable, [(1, true), (2, false), (3, false), (4, false), (6, true)]);
    }

    #[test]
    fn synthesis_in_and_out() {
        let now = Instant::now();
        let mut window = synthesis(2, [0, 10]);
        let bag = [carried(909, 1, 1, 0), carried(914, 2, 5, 0), carried(935, 3, 1, 0)];
        assert!(!window.ready());
        // A double click on a row puts it in; the stack went in whole, so its row leaves.
        assert_eq!(window.click(Hit::Row(0), &bag, now), None);
        assert_eq!(window.click(Hit::Row(0), &bag, now), None);
        assert_eq!(window.entries(), [(1, 1)]);
        assert_eq!(window.rows(&bag).iter().map(|row| row.index).collect::<Vec<_>>(), [2, 3]);
        // Picked and sent with the move-item button: two of five go in, the rest stays dimmed.
        window.click(Hit::Row(0), &bag, now);
        assert_eq!(window.picked, Some(2));
        assert!(window.move_enabled());
        window.click(Hit::MoveItem, &bag, now);
        assert_eq!(window.entries(), [(1, 1), (2, 2)]);
        assert_eq!(window.rows(&bag), [Row { index: 2, usable: false }, Row { index: 3, usable: true }]);
        assert!(window.ready());
        assert!(!window.move_enabled());
        // Full: no more go in.
        window.click(Hit::Row(1), &bag, now);
        window.click(Hit::Row(1), &bag, now);
        assert_eq!(window.entries().len(), 2);
        // A double click on a slot takes it out and its row comes back.
        window.click(Hit::Slot(0), &bag, now);
        assert_eq!(window.entries().len(), 2);
        window.click(Hit::Slot(0), &bag, now);
        assert_eq!(window.entries(), [(2, 2)]);
        assert_eq!(window.rows(&bag)[0], Row { index: 1, usable: true });
        assert!(!window.ready());
        // Dragged: a row onto the window goes in, a slot onto the list comes out.
        window.drop_row(1, on(&window, [100.0, 100.0]), &bag, now);
        assert_eq!(window.entries(), [(2, 2), (1, 1)]);
        window.drop_slot(2, on(&window, [100.0, 100.0]));
        assert_eq!(window.entries().len(), 2);
        window.drop_slot(2, on(&window, [300.0, 100.0]));
        assert_eq!(window.entries(), [(1, 1)]);
        window.drop_row(3, on(&window, [300.0, 100.0]), &bag, now);
        assert_eq!(window.entries(), [(1, 1)]);
        // A stack that shrank since it was picked: 2898 with the amount, on the notice line.
        assert!(!window.put(&carried(914, 2, 1, 0), now));
        assert_eq!(window.notice, Some((NEED, 2, now)));
    }

    #[test]
    fn make_and_the_answers() {
        let now = Instant::now();
        let bag = [carried(909, 1, 1, 0), carried(935, 3, 1, 0)];
        let mut window = synthesis(2, [0, 10]);
        assert_eq!(window.click(Hit::Make, &bag, now), None);
        window.put(&bag[0], now);
        assert_eq!(window.click(Hit::Make, &bag, now), None);
        window.put(&bag[1], now);
        assert!(window.ready());
        assert_eq!(window.click(Hit::Make, &bag, now), Some(Action::Make));
        // Waiting: Make is off until the answer.
        assert!(!window.ready());
        assert_eq!(window.click(Hit::Make, &bag, now), None);
        let messages: Vec<String> = (0..3000).map(|id| if id == NOT_ENOUGH { "Insufficient synthesis materials".into() } else { String::new() }).collect();
        for result in [5, 7] {
            assert!(!window.answer(result, now));
            assert!(window.ready());
            assert_eq!(window.notice_text(&messages, now).as_deref(), Some("Insufficient synthesis materials"));
            assert_eq!(window.notice_text(&messages, now + NOTICE_TIME), None);
            window.waiting = true;
        }
        assert!(window.answer(0, now));
        // The upgrade stays waiting on a refusal.
        let mut upgrade = upgrade([0, 10], 0);
        let suit = carried(450177, 9, 1, 0);
        upgrade.put(&suit, now);
        assert_eq!(upgrade.click(Hit::Make, std::slice::from_ref(&suit), now), Some(Action::Make));
        assert!(!upgrade.answer(1, now));
        assert!(!upgrade.ready());
        assert_eq!(upgrade.notice, None);
        assert!(upgrade.answer(0, now));
        assert_eq!(window.click(Hit::Close, &bag, now), Some(Action::Close));
        assert_eq!(window.click(Hit::Title(TitleHit::Close), &bag, now), Some(Action::Close));
    }

    #[test]
    fn upgrade_put_in_rules() {
        let now = Instant::now();
        let options = vec![ItemOption::default(); 2];
        let mut window = upgrade([3, 8], 2);
        assert!(!window.put(&carried(1201, 1, 1, 4), now));
        assert!(!window.put(&InventoryItem { options: options.clone(), ..carried(450177, 1, 1, 2) }, now));
        assert!(!window.put(&InventoryItem { options: options.clone(), ..carried(450177, 1, 1, 9) }, now));
        assert!(!window.put(&InventoryItem { options: vec![ItemOption::default()], ..carried(450177, 1, 1, 4) }, now));
        assert_eq!(window.notice, None);
        // Above the minimum: in, with 3645.
        let above = InventoryItem { options: options.clone(), ..carried(450177, 1, 1, 4) };
        assert!(window.put(&above, now));
        assert_eq!((window.index(), window.notice), (Some(1), Some((REFINE_ABOVE, 0, now))));
        // Only one in: the row leaves, a second is refused, the move-item button is off.
        let bag = [above.clone(), InventoryItem { options: options.clone(), cards: [4001, 0, 0, 0], ..carried(450177, 2, 1, 3) }];
        assert_eq!(window.rows(&bag), [Row { index: 2, usable: true }]);
        assert!(!window.put(&bag[1], now));
        window.click(Hit::Row(0), &bag, now);
        assert!(!window.move_enabled());
        // Out by a double click: the row and nothing on the notice line.
        window.click(Hit::Slot(0), &bag, now);
        window.click(Hit::Slot(0), &bag, now);
        assert_eq!((window.index(), window.notice), (None, None));
        // Cards: in, with 3646, which wins over 3645.
        let carded = InventoryItem { cards: [4001, 0, 0, 0], ..above };
        assert!(window.put(&carded, now));
        assert_eq!(window.notice, Some((CARDS, 0, now)));
        let mut plain = upgrade([0, 10], 0);
        assert!(plain.put(&carried(450177, 3, 1, 6), now));
        assert_eq!(plain.notice, None);
        assert_eq!(plain.slot_at(0), UPGRADE_SLOT);
    }

    #[test]
    fn the_list_scrolls_by_wheel_and_thumb() {
        let mut window = synthesis(3, [0, 10]);
        let bag: Vec<_> = (0..10).map(|index| carried(909 + u32::from(index), index, 1, 0)).collect();
        window.sources = bag.iter().map(|item| (item.id, 1)).collect();
        window.scroll_by(9, &bag);
        assert_eq!(window.scroll, 4);
        assert_eq!(window.thumb_box(10), [204.0 - 122.0, 122.0]);
        window.scroll_by(-9, &bag);
        assert_eq!(window.press(on(&window, [385.0, 30.0]), &bag), None);
        window.drag_thumb(on(&window, [385.0, 30.0])[1] + 82.0, &bag);
        assert_eq!(window.scroll, 4);
        assert!(window.drop_thumb());
        assert_eq!(window.hit(on(&window, [300.0, 24.0])), Some(Hit::Row(4)));
        assert_eq!(window.press(on(&window, [300.0, 24.0]), &bag), Some(Grab::Row(4)));
        let rows = window.row_boxes(&bag);
        assert_eq!(rows.iter().map(|(id, _)| *id).collect::<Vec<_>>(), [913, 914, 915, 916, 917, 918]);
        assert_eq!(rows[0].1, [on(&window, [241.0, 24.0])[0], on(&window, [241.0, 24.0])[1], 136.0, 30.0]);
        let names: Vec<_> = window.button_boxes().into_iter().map(|(name, _)| name).collect();
        assert_eq!(names, ["make", "close", "move_item"]);
    }
}
