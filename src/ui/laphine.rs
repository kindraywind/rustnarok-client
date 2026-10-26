//! Laphine boxes: synthesis (several materials in, a reward out) and upgrade (one item's refine raised).
//! The two windows share their pictures.

use std::collections::HashMap;

use rustnarok_client::lua::{self, ItemInfo};
use rustnarok_client::net::InventoryItem;
use rustnarok_client::text::Font;

use crate::inventory;
use crate::render::{Gpu, Texture};
use crate::window::{Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI, contains};

// ponytail: where things sit, the text color and the upgrade title aren't knowable. The pictures are 400×250,
// the make and cancel buttons 56×20, an empty slot 32×18.
const WIDTH: f32 = 400.0;
const BG: f32 = 250.0;
const POSITION: [f32; 2] = [300.0, 140.0];
const BUTTON: [f32; 2] = [56.0, 20.0];
const SLOT: f32 = 36.0;
const SLOT_STEP: f32 = 40.0;
const SLOT_COLUMNS: usize = 3;
const SLOTS: [f32; 2] = [248.0, TITLE_HEIGHT + 36.0];
const UPGRADE_SLOT: [f32; 2] = [182.0, TITLE_HEIGHT + 96.0];
const LIST: [f32; 2] = [12.0, TITLE_HEIGHT + 36.0];
const ROW: f32 = 28.0;
const ROW_WIDTH: f32 = 220.0;
const VISIBLE: usize = 5;
const TEXT: [u8; 3] = [0x40, 0x30, 0x20];
/// ponytail: 2896 as the synthesis title is a guess. Nothing in the message table fits the upgrade.
pub const TITLE: usize = 2896;
const UPGRADE_TITLE: &str = "Upgrade";
/// "Required material %d ea."
pub const NEED: usize = 2898;
pub const NOT_ENOUGH: usize = 2897;
pub const REFINE_LOW: usize = 2899;
pub const REFINE_HIGH: usize = 3644;
pub const NOTHING: usize = 3856;
pub const CARDS: usize = 3646;
/// ponytail: the lines for an upgrade's result aren't knowable.
pub const UPGRADE_OK: usize = 2971;
pub const UPGRADE_FAIL: usize = 2972;

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(TitleHit),
    Slot(usize),
    Make,
    Close,
    Window,
}

#[derive(Debug, PartialEq)]
pub enum Action {
    Make,
    Close,
}

/// What dropping an inventory item on the window does.
#[derive(Debug, PartialEq)]
pub enum Put {
    Taken,
    /// Not one of the listed items, already in, or worn: the window stays as it was.
    Quiet,
    /// A message number for the chat.
    Say(usize),
}

pub struct Assets {
    /// Synthesis, then upgrade.
    background: [Option<Texture>; 2],
    /// Out, over, press, disabled.
    make: [Option<Texture>; 4],
    cancel: [Option<Texture>; 4],
    /// Out, over, press, on a filled slot.
    move_item: [Option<Texture>; 3],
    selected: Option<Texture>,
    empty: Option<Texture>,
    notify: Option<Texture>,
    scroll_bg: [Option<Texture>; 3],
    scroll_thumb: [Option<Texture>; 3],
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &rustnarok_client::grf::Grf) -> Self {
        let image = |name: &str| gpu.try_load_texture(grf, &format!("{UI}lapine\\{name}.bmp"), false).ok();
        let states = |name: &str, suffixes: &[&str]| {
            let mut images = [None, None, None, None];
            for (index, suffix) in suffixes.iter().enumerate() {
                images[index] = image(&format!("{name}_{suffix}"));
            }
            images
        };
        Self {
            background: [image("bg_lapine"), image("bg_lapineupgrade")],
            make: states("btn_make", &["out", "over", "press", "dis"]),
            cancel: states("btn_cancel", &["out", "over", "press", "dis"]),
            move_item: {
                let all = states("btn_moveitem", &["out", "over", "press"]);
                [all[0].clone(), all[1].clone(), all[2].clone()]
            },
            selected: image("list_selected_item"),
            empty: image("shadow_item_off"),
            notify: image("icon_notify"),
            scroll_bg: [image("scrollbar_bg_top"), image("scrollbar_bg_mid"), image("scrollbar_bg_btm")],
            scroll_thumb: [image("scrollbar_thumb_out_top"), image("scrollbar_thumb_out_mid"), image("scrollbar_thumb_out_btm")],
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Placed {
    index: u16,
    amount: u16,
    id: u32,
}

pub struct Synthesis {
    pub frame: Frame,
    pub box_id: u32,
    need_count: usize,
    refine_min: u32,
    refine_max: u32,
    sources: Vec<lua::SynthesisSource>,
    slots: Vec<Placed>,
    scroll: usize,
}

impl Synthesis {
    pub fn new(box_id: u32, info: &lua::SynthesisBox) -> Self {
        let mut frame = Frame::new(POSITION, WIDTH);
        frame.open = true;
        Self {
            frame,
            box_id,
            need_count: info.need_count as usize,
            refine_min: info.refine_min,
            refine_max: info.refine_max,
            sources: info.sources.clone(),
            slots: Vec::new(),
            scroll: 0,
        }
    }

    pub fn height() -> f32 {
        TITLE_HEIGHT + BG
    }

    pub fn ready(&self) -> bool {
        self.need_count > 0 && self.slots.len() == self.need_count
    }

    pub fn entries(&self) -> Vec<(u16, u16)> {
        self.slots.iter().map(|slot| (slot.index, slot.amount)).collect()
    }

    pub fn clear(&mut self) {
        self.slots.clear();
    }

    pub fn scroll_by(&mut self, rows: i32) {
        let max = self.sources.len().saturating_sub(VISIBLE);
        self.scroll = (self.scroll as i32 + rows).clamp(0, max as i32) as usize;
    }

    /// Puts the dragged item in the next free slot, with the amount the list wants.
    pub fn put(&mut self, item: &InventoryItem) -> Put {
        let Some(source) = self.sources.iter().find(|source| source.id == item.id) else { return Put::Quiet };
        if self.slots.iter().any(|slot| slot.index == item.index || slot.id == item.id) || item.worn != 0 || self.slots.len() >= self.need_count {
            return Put::Quiet;
        }
        if u32::from(item.amount) < u32::from(source.amount) {
            return Put::Say(NOT_ENOUGH);
        }
        if u32::from(item.refine) < self.refine_min {
            return Put::Say(REFINE_LOW);
        }
        if u32::from(item.refine) > self.refine_max {
            return Put::Say(REFINE_HIGH);
        }
        self.slots.push(Placed { index: item.index, amount: source.amount, id: item.id });
        Put::Taken
    }

    pub fn button_boxes(&self) -> Vec<(String, [f32; 4])> {
        if !self.frame.open || self.frame.minimized {
            return Vec::new();
        }
        let mut boxes = (0..self.need_count).map(|slot| (format!("slot_{slot}"), place(&self.frame, slot_at(slot), [SLOT, SLOT]))).collect::<Vec<_>>();
        boxes.push(("make".into(), place(&self.frame, make_at(), BUTTON)));
        boxes.push(("close".into(), place(&self.frame, close_at(), BUTTON)));
        boxes
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let local = self.frame.local(cursor, Self::height())?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(Hit::Title(hit));
        }
        if contains(make_at(), BUTTON, local) {
            return Some(Hit::Make);
        }
        if contains(close_at(), BUTTON, local) {
            return Some(Hit::Close);
        }
        let slot = (0..self.need_count).find(|&slot| contains(slot_at(slot), [SLOT, SLOT], local));
        Some(slot.map_or(Hit::Window, Hit::Slot))
    }

    pub fn click(&mut self, hit: Hit) -> Option<Action> {
        match hit {
            Hit::Title(TitleHit::Close) | Hit::Close => Some(Action::Close),
            Hit::Title(title) => {
                self.frame.click_title(title);
                None
            }
            Hit::Slot(slot) => {
                if slot < self.slots.len() {
                    self.slots.remove(slot);
                }
                None
            }
            Hit::Make if self.ready() => Some(Action::Make),
            Hit::Make | Hit::Window => None,
        }
    }

    pub fn icons(&self, items: &HashMap<u32, ItemInfo>, carried: &[InventoryItem]) -> Vec<String> {
        let from_list = self.sources.iter().filter_map(|source| items.get(&source.id).map(|info| inventory::icon_path(info, true)));
        let from_slots = self
            .slots
            .iter()
            .filter_map(|slot| carried.iter().find(|item| item.index == slot.index))
            .filter_map(|item| items.get(&item.id).map(|info| inventory::icon_path(info, item.identified)));
        from_list.chain(from_slots).collect()
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
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        draw_frame(layer, chrome, &self.frame, assets, 0, message(TITLE), pointer.0);
        if self.frame.minimized {
            return;
        }
        let at = |local: [f32; 2]| self.frame.at(local);
        if let Some(notify) = &assets.notify {
            layer.sprites.push((notify, at([12.0, TITLE_HEIGHT + 8.0]), notify.size));
        }
        let need = rustnarok_client::messages::fill(message(NEED), &[&self.need_count]);
        layer.texts.push((need, at([30.0, TITLE_HEIGHT + 8.0]), TEXT));
        let rows: Vec<_> = self.sources.iter().map(|source| (source.id, source.amount, source.name.as_str())).collect();
        draw_sources(layer, &self.frame, assets, icons, items, font, pointer.0, &rows, self.scroll);
        draw_scrollbar(layer, &self.frame, assets, self.sources.len(), self.scroll);
        let hit = self.hit(pointer.0);
        for slot in 0..self.need_count {
            draw_slot(layer, &self.frame, assets, icons, items, carried, slot_at(slot), self.slots.get(slot), hit == Some(Hit::Slot(slot)), pointer.1);
        }
        draw_buttons(layer, &self.frame, assets, hit, self.ready(), pointer);
    }
}

pub struct Upgrade {
    pub frame: Frame,
    pub box_id: u32,
    refine_min: u32,
    refine_max: u32,
    no_cards: bool,
    targets: Vec<lua::UpgradeTarget>,
    slot: Option<Placed>,
    scroll: usize,
}

impl Upgrade {
    pub fn new(box_id: u32, info: &lua::UpgradeBox) -> Self {
        let mut frame = Frame::new(POSITION, WIDTH);
        frame.open = true;
        Self {
            frame,
            box_id,
            refine_min: info.refine_min,
            refine_max: info.refine_max,
            no_cards: info.no_cards,
            targets: info.targets.clone(),
            slot: None,
            scroll: 0,
        }
    }

    pub fn height() -> f32 {
        TITLE_HEIGHT + BG
    }

    pub fn ready(&self) -> bool {
        self.slot.is_some()
    }

    pub fn index(&self) -> Option<u16> {
        self.slot.as_ref().map(|slot| slot.index)
    }

    pub fn clear(&mut self) {
        self.slot = None;
    }

    pub fn scroll_by(&mut self, rows: i32) {
        let max = self.targets.len().saturating_sub(VISIBLE);
        self.scroll = (self.scroll as i32 + rows).clamp(0, max as i32) as usize;
    }

    pub fn put(&mut self, item: &InventoryItem) -> Put {
        if self.targets.iter().all(|target| target.id != item.id) || !item.identified || item.worn != 0 {
            return Put::Quiet;
        }
        if u32::from(item.refine) < self.refine_min {
            return Put::Say(REFINE_LOW);
        }
        if u32::from(item.refine) > self.refine_max {
            return Put::Say(REFINE_HIGH);
        }
        if self.no_cards && item.cards.iter().any(|card| *card != 0) {
            return Put::Say(CARDS);
        }
        self.slot = Some(Placed { index: item.index, amount: 1, id: item.id });
        Put::Taken
    }

    pub fn button_boxes(&self) -> Vec<(String, [f32; 4])> {
        if !self.frame.open || self.frame.minimized {
            return Vec::new();
        }
        vec![
            ("slot_0".into(), place(&self.frame, UPGRADE_SLOT, [SLOT, SLOT])),
            ("make".into(), place(&self.frame, make_at(), BUTTON)),
            ("close".into(), place(&self.frame, close_at(), BUTTON)),
        ]
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let local = self.frame.local(cursor, Self::height())?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(Hit::Title(hit));
        }
        if contains(make_at(), BUTTON, local) {
            return Some(Hit::Make);
        }
        if contains(close_at(), BUTTON, local) {
            return Some(Hit::Close);
        }
        if contains(UPGRADE_SLOT, [SLOT, SLOT], local) {
            return Some(Hit::Slot(0));
        }
        Some(Hit::Window)
    }

    pub fn click(&mut self, hit: Hit) -> Option<Action> {
        match hit {
            Hit::Title(TitleHit::Close) | Hit::Close => Some(Action::Close),
            Hit::Title(title) => {
                self.frame.click_title(title);
                None
            }
            Hit::Slot(_) => {
                self.slot = None;
                None
            }
            Hit::Make if self.ready() => Some(Action::Make),
            Hit::Make | Hit::Window => None,
        }
    }

    pub fn icons(&self, items: &HashMap<u32, ItemInfo>, carried: &[InventoryItem]) -> Vec<String> {
        let from_list = self.targets.iter().filter_map(|target| items.get(&target.id).map(|info| inventory::icon_path(info, true)));
        let from_slot = self
            .slot
            .as_ref()
            .and_then(|slot| carried.iter().find(|item| item.index == slot.index))
            .and_then(|item| items.get(&item.id).map(|info| inventory::icon_path(info, item.identified)));
        from_list.chain(from_slot).collect()
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
        _messages: &[String],
        font: &Font,
        pointer: ([f32; 2], bool),
    ) {
        draw_frame(layer, chrome, &self.frame, assets, 1, UPGRADE_TITLE, pointer.0);
        if self.frame.minimized {
            return;
        }
        let rows: Vec<_> = self.targets.iter().map(|target| (target.id, 1, target.name.as_str())).collect();
        draw_sources(layer, &self.frame, assets, icons, items, font, pointer.0, &rows, self.scroll);
        draw_scrollbar(layer, &self.frame, assets, self.targets.len(), self.scroll);
        let hit = self.hit(pointer.0);
        draw_slot(layer, &self.frame, assets, icons, items, carried, UPGRADE_SLOT, self.slot.as_ref(), hit == Some(Hit::Slot(0)), pointer.1);
        draw_buttons(layer, &self.frame, assets, hit, self.ready(), pointer);
    }
}

fn place(frame: &Frame, local: [f32; 2], size: [f32; 2]) -> [f32; 4] {
    let [x, y] = frame.at(local);
    [x, y, size[0], size[1]]
}

fn slot_at(index: usize) -> [f32; 2] {
    [SLOTS[0] + (index % SLOT_COLUMNS) as f32 * SLOT_STEP, SLOTS[1] + (index / SLOT_COLUMNS) as f32 * SLOT_STEP]
}

fn make_at() -> [f32; 2] {
    [WIDTH - 12.0 - BUTTON[0], TITLE_HEIGHT + BG - 12.0 - BUTTON[1]]
}

fn close_at() -> [f32; 2] {
    [make_at()[0] - 8.0 - BUTTON[0], make_at()[1]]
}

fn button_state(hit: Option<Hit>, target: Hit, enabled: bool, pressed: bool) -> usize {
    if !enabled {
        return 3;
    }
    let hovered = hit == Some(target);
    usize::from(hovered) + usize::from(hovered && pressed)
}

fn draw_frame<'a>(layer: &mut Layer<'a>, chrome: &'a Chrome, frame: &Frame, assets: &'a Assets, which: usize, title: &str, cursor: [f32; 2]) {
    frame.draw_title(layer, chrome, title, cursor);
    if frame.minimized {
        return;
    }
    match &assets.background[which] {
        Some(background) => layer.sprites.push((background, frame.at([0.0, TITLE_HEIGHT]), [WIDTH, BG])),
        None => layer.sprites.push((&chrome.white, frame.at([0.0, TITLE_HEIGHT]), [WIDTH, BG])),
    }
}

fn draw_sources<'a>(
    layer: &mut Layer<'a>,
    frame: &Frame,
    assets: &'a Assets,
    icons: &'a HashMap<String, Option<Texture>>,
    items: &HashMap<u32, ItemInfo>,
    font: &Font,
    cursor: [f32; 2],
    rows: &[(u32, u16, &str)],
    scroll: usize,
) {
    let local = frame.local(cursor, TITLE_HEIGHT + BG);
    for (row, (id, amount, fallback)) in rows.iter().enumerate().skip(scroll).take(VISIBLE) {
        let y = LIST[1] + (row - scroll) as f32 * ROW;
        let hovered = local.is_some_and(|local| contains([LIST[0], y], [ROW_WIDTH, ROW], local));
        if hovered && let Some(selected) = &assets.selected {
            layer.sprites.push((selected, frame.at([LIST[0], y]), [ROW_WIDTH, ROW]));
        }
        if let Some(icon) = items.get(id).and_then(|info| icons.get(&inventory::icon_path(info, true))).and_then(Option::as_ref) {
            layer.sprites.push((icon, frame.at([LIST[0] + 2.0, y + 2.0]), icon.size));
        }
        let name = items.get(id).map(|info| info.name(true)).unwrap_or(fallback);
        let label = if *amount > 1 { format!("{name}  {amount}") } else { name.to_owned() };
        layer.texts.push((window_fit(font, &label), frame.at([LIST[0] + 28.0, y + 8.0]), TEXT));
    }
}

fn window_fit(font: &Font, text: &str) -> String {
    crate::window::fit(font, text, ROW_WIDTH - 36.0)
}

fn draw_scrollbar<'a>(layer: &mut Layer<'a>, frame: &Frame, assets: &'a Assets, len: usize, scroll: usize) {
    if len <= VISIBLE {
        return;
    }
    let x = LIST[0] + ROW_WIDTH - 8.0;
    let top = LIST[1];
    let height = VISIBLE as f32 * ROW;
    if let Some(background) = &assets.scroll_bg[1] {
        layer.sprites.push((background, frame.at([x, top]), [background.size[0], height]));
    }
    if let Some(cap) = &assets.scroll_bg[0] {
        layer.sprites.push((cap, frame.at([x, top]), cap.size));
    }
    if let Some(cap) = &assets.scroll_bg[2] {
        layer.sprites.push((cap, frame.at([x, top + height - cap.size[1]]), cap.size));
    }
    if let Some(thumb) = &assets.scroll_thumb[1] {
        let span = (len - VISIBLE) as f32;
        let y = top + (height - thumb.size[1]) * (scroll as f32 / span);
        layer.sprites.push((thumb, frame.at([x, y]), thumb.size));
    }
}

fn draw_slot<'a>(
    layer: &mut Layer<'a>,
    frame: &Frame,
    assets: &'a Assets,
    icons: &'a HashMap<String, Option<Texture>>,
    items: &HashMap<u32, ItemInfo>,
    carried: &[InventoryItem],
    at: [f32; 2],
    placed: Option<&Placed>,
    hovered: bool,
    pressed: bool,
) {
    let Some(placed) = placed else {
        if let Some(empty) = &assets.empty {
            layer.sprites.push((empty, frame.at([at[0] + 2.0, at[1] + 9.0]), empty.size));
        }
        return;
    };
    if let Some(item) = carried.iter().find(|item| item.index == placed.index)
        && let Some(icon) = items.get(&item.id).and_then(|info| icons.get(&inventory::icon_path(info, item.identified))).and_then(Option::as_ref)
    {
        layer.sprites.push((icon, frame.at([at[0] + 2.0, at[1] + 2.0]), icon.size));
    }
    let state = usize::from(hovered) + usize::from(hovered && pressed);
    if let Some(button) = assets.move_item[state].as_ref().or(assets.move_item[0].as_ref()) {
        layer.sprites.push((button, frame.at([at[0] + SLOT - 18.0, at[1] + SLOT - 18.0]), button.size));
    }
}

fn draw_buttons<'a>(layer: &mut Layer<'a>, frame: &Frame, assets: &'a Assets, hit: Option<Hit>, ready: bool, pointer: ([f32; 2], bool)) {
    let paint = |layer: &mut Layer<'a>, states: &'a [Option<Texture>; 4], index: usize, at: [f32; 2]| {
        if let Some(image) = states[index].as_ref().or(states[0].as_ref()) {
            layer.sprites.push((image, frame.at(at), BUTTON));
        }
    };
    paint(layer, &assets.cancel, button_state(hit, Hit::Close, true, pointer.1).min(2), close_at());
    paint(layer, &assets.make, button_state(hit, Hit::Make, ready, pointer.1), make_at());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn carried(id: u32, index: u16, amount: u16, refine: u8) -> InventoryItem {
        InventoryItem { id, index, amount, refine, identified: true, ..Default::default() }
    }

    fn synthesis() -> Synthesis {
        Synthesis::new(
            100000,
            &lua::SynthesisBox {
                need_count: 3,
                refine_min: 0,
                refine_max: 6,
                sources: vec![
                    lua::SynthesisSource { name: "Jellopy".into(), amount: 1, id: 909 },
                    lua::SynthesisSource { name: "Fluff".into(), amount: 2, id: 914 },
                    lua::SynthesisSource { name: "Shell".into(), amount: 1, id: 935 },
                ],
            },
        )
    }

    #[test]
    fn synthesis_takes_the_listed_items_once() {
        let mut window = synthesis();
        assert!(!window.ready());
        assert_eq!(window.click(Hit::Make), None);
        assert_eq!(window.put(&carried(501, 1, 5, 0)), Put::Quiet);
        assert_eq!(window.put(&carried(909, 2, 1, 0)), Put::Taken);
        assert_eq!(window.put(&carried(909, 3, 1, 0)), Put::Quiet);
        assert_eq!(window.put(&InventoryItem { worn: 2, ..carried(914, 4, 2, 0) }), Put::Quiet);
        assert_eq!(window.put(&carried(914, 4, 1, 0)), Put::Say(NOT_ENOUGH));
        assert_eq!(window.put(&carried(914, 4, 2, 7)), Put::Say(REFINE_HIGH));
        assert_eq!(window.put(&carried(935, 5, 4, 0)), Put::Taken);
        let mut low = Synthesis::new(
            1,
            &lua::SynthesisBox {
                need_count: 1,
                refine_min: 5,
                refine_max: 10,
                sources: vec![lua::SynthesisSource { name: "Ore".into(), amount: 1, id: 1010 }],
            },
        );
        assert_eq!(low.put(&carried(1010, 1, 1, 4)), Put::Say(REFINE_LOW));
        assert_eq!(low.put(&carried(1010, 1, 1, 5)), Put::Taken);
        assert_eq!(window.entries(), [(2, 1), (5, 1)]);
        assert!(!window.ready());
        assert_eq!(window.put(&carried(914, 4, 9, 6)), Put::Taken);
        assert!(window.ready());
        assert_eq!(window.entries(), [(2, 1), (5, 1), (4, 2)]);
        assert_eq!(window.click(Hit::Slot(0)), None);
        assert_eq!(window.entries(), [(5, 1), (4, 2)]);
        assert!(!window.ready());
        assert_eq!(window.click(Hit::Make), None);
        window.put(&carried(909, 2, 1, 0));
        assert_eq!(window.click(Hit::Make), Some(Action::Make));
        assert_eq!(window.put(&carried(914, 9, 1, 9)), Put::Quiet);
        assert_eq!(window.click(Hit::Close), Some(Action::Close));
        window.clear();
        assert!(window.entries().is_empty());
        let mut empty = Synthesis::new(1, &lua::SynthesisBox { need_count: 0, refine_min: 0, refine_max: 0, sources: Vec::new() });
        assert!(!empty.ready());
        assert_eq!(empty.click(Hit::Make), None);
        let mut full = Synthesis::new(
            1,
            &lua::SynthesisBox {
                need_count: 1,
                refine_min: 0,
                refine_max: 10,
                sources: vec![
                    lua::SynthesisSource { name: "Red Potion".into(), amount: 1, id: 501 },
                    lua::SynthesisSource { name: "Orange Potion".into(), amount: 5, id: 502 },
                ],
            },
        );
        assert_eq!(full.put(&carried(501, 1, 1, 0)), Put::Taken);
        assert_eq!(full.put(&carried(502, 2, 1, 0)), Put::Quiet);
        let names: Vec<_> = window.button_boxes().into_iter().map(|(name, _)| name).collect();
        assert!(names.contains(&"make".to_owned()));
        assert!(names.contains(&"slot_0".to_owned()));
        assert!(names.contains(&"slot_2".to_owned()));
        assert!(names.contains(&"close".to_owned()));
    }

    #[test]
    fn upgrade_takes_one_target() {
        let mut window = Upgrade::new(
            100626,
            &lua::UpgradeBox {
                refine_min: 0,
                refine_max: 6,
                option_min: 0,
                no_cards: true,
                targets: vec![lua::UpgradeTarget { name: "Gray Wolf Suit".into(), id: 450177 }],
            },
        );
        assert_eq!(window.put(&carried(1201, 1, 1, 0)), Put::Quiet);
        assert_eq!(window.put(&InventoryItem { identified: false, ..carried(450177, 2, 1, 0) }), Put::Quiet);
        assert_eq!(window.put(&InventoryItem { worn: 16, ..carried(450177, 2, 1, 0) }), Put::Quiet);
        assert_eq!(window.put(&carried(450177, 2, 1, 7)), Put::Say(REFINE_HIGH));
        assert_eq!(window.put(&InventoryItem { cards: [4001, 0, 0, 0], ..carried(450177, 2, 1, 0) }), Put::Say(CARDS));
        assert_eq!(window.put(&carried(450177, 2, 1, 0)), Put::Taken);
        assert!(window.ready());
        assert_eq!(window.index(), Some(2));
        assert_eq!(window.click(Hit::Make), Some(Action::Make));
        assert_eq!(window.click(Hit::Slot(0)), None);
        assert!(!window.ready());
        window.no_cards = false;
        assert_eq!(window.put(&InventoryItem { cards: [4001, 0, 0, 0], ..carried(450177, 3, 1, 6) }), Put::Taken);
        let names: Vec<_> = window.button_boxes().into_iter().map(|(name, _)| name).collect();
        assert_eq!(names.iter().filter(|name| name.starts_with("slot_")).count(), 1);
    }
}
