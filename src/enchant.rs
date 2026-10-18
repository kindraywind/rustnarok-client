//! The enchant window (2021+ enchanters): an item dropped in, then a random enchant, a chosen perfect one,
//! an upgrade, or a reset. The table comes from the client's Lua; rathena only checks and answers.

use std::collections::HashMap;

use rustnarok_client::grf::Grf;
use rustnarok_client::lua::{EnchantSlot, EnchantTable, ItemInfo};
use rustnarok_client::net::InventoryItem;
use rustnarok_client::text::Font;

use crate::inventory;
use crate::render::{Gpu, Texture};
use crate::shop::group;
use crate::window::{self, Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI, contains, offset};

// ponytail: which tab image is which, and every place over the enchantui pictures, is an estimate.
const WIDTH: f32 = 690.0;
pub const HEIGHT: f32 = TITLE_HEIGHT + 470.0;
const POSITION: [f32; 2] = [300.0, 40.0];
const TAB_TOP: f32 = TITLE_HEIGHT + 12.0;
const TAB_LEFT: f32 = 16.0;
const TAB_SIZE: [f32; 2] = [65.0, 22.0];
const TAB_GAP: f32 = 4.0;
const ITEM_AT: [f32; 2] = [16.0, TITLE_HEIGHT + 44.0];
const ITEM_SIZE: [f32; 2] = [150.0, 30.0];
const SOCKET_AT: [f32; 2] = [16.0, TITLE_HEIGHT + 90.0];
const SOCKET_SIZE: [f32; 2] = [106.0, 80.0];
const SOCKET_STEP: [f32; 2] = [110.0, 84.0];
const ROW_LEFT: f32 = 250.0;
const ROW_TOP: f32 = TITLE_HEIGHT + 90.0;
const ROW_STEP: f32 = 26.0;
const ROW_SIZE: [f32; 2] = [260.0, 23.0];
const VISIBLE: usize = 8;
const ACTION_AT: [f32; 2] = [520.0, TITLE_HEIGHT + 400.0];
const ACTION_SIZE: [f32; 2] = [140.0, 40.0];
const GREY: [u8; 3] = [0x90, 0x90, 0x90];

/// "Enchantment successful!"
pub const SUCCESS: usize = 3857;
/// "Enchantment failed!" The packet carries the number; a failure of either kind leaves the cards alone.
#[allow(dead_code)]
pub const FAILED: usize = 3858;
/// "Enchantment Information not found."
pub const NOT_FOUND: usize = 3859;
/// "Enchantment Conditions are not met."
pub const CONDITIONS: usize = 3860;
/// "Insufficient Zeny."
pub const NO_ZENY: usize = 2968;
/// "Insufficient amount of materials."
pub const NO_MATERIALS: usize = 3709;
const TAB_LABELS: [usize; 4] = [3833, 3834, 3835, 3836];
const ENCHANT_LABEL: usize = 3838;
const UPGRADE_LABEL: usize = 3840;
const RESET_LABEL: usize = 3841;
const TITLE: usize = 3838;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Normal,
    Perfect,
    Upgrade,
    Reset,
}

const TABS: [Tab; 4] = [Tab::Normal, Tab::Perfect, Tab::Upgrade, Tab::Reset];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TabSet {
    pub normal: bool,
    pub perfect: bool,
    pub upgrade: bool,
    pub reset: bool,
}

impl TabSet {
    fn has(self, tab: Tab) -> bool {
        match tab {
            Tab::Normal => self.normal,
            Tab::Perfect => self.perfect,
            Tab::Upgrade => self.upgrade,
            Tab::Reset => self.reset,
        }
    }

    fn first(self) -> Tab {
        TABS.into_iter().find(|tab| self.has(*tab)).unwrap_or(Tab::Normal)
    }
}

/// The first free enchant slot: the first of `order` whose card is 0 and which isn't one of the item's own card slots.
pub fn next_slot(order: &[u8], cards: [u32; 4], item_slots: u8) -> Option<u8> {
    order.iter().copied().find(|&slot| usize::from(slot) < 4 && slot >= item_slots && cards[usize::from(slot)] == 0)
}

/// Which tabs have something to do for this item.
pub fn tabs_for(table: &EnchantTable, cards: [u32; 4], item_slots: u8) -> TabSet {
    let next = next_slot(&table.slot_order, cards, item_slots).and_then(|slot| table.slots[usize::from(slot)].as_ref());
    let upgrade = (u32::from(item_slots)..4).any(|slot| {
        let card = cards[slot as usize];
        card != 0 && table.slots[slot as usize].as_ref().is_some_and(|info| info.upgrades.iter().any(|(item, _)| *item == card))
    });
    let enchanted = (u32::from(item_slots)..4).any(|slot| cards[slot as usize] != 0);
    TabSet {
        normal: next.is_some_and(EnchantSlot::random),
        perfect: next.is_some_and(|slot| !slot.perfect.is_empty()),
        upgrade,
        reset: table.reset.as_ref().is_some_and(|reset| reset.enabled && reset.rate > 0) && enchanted,
    }
}

/// Why an item can't go in the window, if it can't. Broken items, the equipment switch and random options are
/// rathena's to refuse.
pub fn refuse_item(table: &EnchantTable, id: u32, worn: u32, refine: u8, grade: u8) -> Option<usize> {
    (worn != 0 || refine < table.min_refine || grade < table.min_grade || !table.targets.contains(&id)).then_some(CONDITIONS)
}

/// Why a request can't be sent: too little zeny, then too few materials (summed over stacks).
pub fn refuse_cost(zeny_cost: u32, materials: &[(u32, u32)], zeny: u32, mut count: impl FnMut(u32) -> u32) -> Option<usize> {
    if zeny < zeny_cost {
        Some(NO_ZENY)
    } else if materials.iter().any(|&(id, need)| count(id) < need) {
        Some(NO_MATERIALS)
    } else {
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Random or perfect: `slot` receives `result`.
    Fill { slot: u8 },
    /// Upgrade: `slot` becomes `result`.
    Upgrade { slot: u8 },
    /// Reset: every slot from the item's own card slots on is cleared.
    Reset,
}

/// The item's cards after an answer. A failure changes nothing.
pub fn cards_after(outcome: Outcome, mut cards: [u32; 4], item_slots: u8, result: u32, success: bool) -> [u32; 4] {
    if !success {
        return cards;
    }
    match outcome {
        Outcome::Fill { slot } | Outcome::Upgrade { slot } if usize::from(slot) < 4 => cards[usize::from(slot)] = result,
        Outcome::Reset => {
            for card in cards.iter_mut().skip(usize::from(item_slots)) {
                *card = 0;
            }
        }
        Outcome::Fill { .. } | Outcome::Upgrade { .. } => {}
    }
    cards
}

#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    Random { table: u64, index: u16 },
    Perfect { table: u64, index: u16, item: u32 },
    Upgrade { table: u64, index: u16, slot: u16 },
    Reset { table: u64, index: u16 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Sent {
    Fill { slot: u8 },
    Upgrade { slot: u8 },
    Reset,
}

#[derive(Debug, PartialEq)]
pub enum Hit {
    Title(TitleHit),
    Tab(Tab),
    Row(usize),
    Action,
    Window,
}

#[derive(Debug, PartialEq)]
pub enum Action {
    Send(Request),
    Refuse(usize),
    Close,
}

struct Shown {
    /// What `targets` calls this row (`pick_item`).
    pick: u32,
    slot: u8,
    name: u32,
    result: Option<u32>,
    zeny: u32,
    materials: Vec<(u32, u32)>,
    /// A random enchant's weight, out of 100000.
    weight: Option<u32>,
}

pub struct Assets {
    background: Texture,
    /// Normal, perfect, upgrade, reset; each out, pressed, disabled.
    tabs: [[Texture; 3]; 4],
    item: Texture,
    row: Texture,
    row_on: Texture,
    sockets: [Texture; 4],
    socket_on: Texture,
    /// The action button, on and disabled.
    action: [Texture; 2],
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let folder = format!("{UI}enchantui\\");
        // Older data (2022 kRO) has no enchantui folder: blank pictures then, rather than no client at all.
        let image = |name: &str| gpu.try_load_texture(grf, &format!("{folder}{name}.bmp"), false).unwrap_or_else(|_| gpu.solid([0, 0, 0, 0]));
        // ponytail: random / confirm / grow / reset guessed as the Normal / Perfect / Upgrade / Reset tabs.
        let tab = |name: &str| [image(&format!("tab_enc_{name}_out")), image(&format!("tab_enc_{name}_pressed")), image(&format!("tab_enc_{name}_disable"))];
        Self {
            background: image("bg_enchantui"),
            tabs: [tab("random"), tab("confirm"), tab("grow"), tab("reset")],
            item: image("btn_enc_item"),
            row: image("bg_enc_menu0"),
            row_on: image("bg_enc_menu1"),
            sockets: [image("btn_socket0"), image("btn_socket1"), image("btn_socket2"), image("btn_socket3")],
            socket_on: image("btn_socket_select"),
            action: [image("btn_enc"), image("btn_enc_dis")],
        }
    }
}

pub struct Enchant {
    pub frame: Frame,
    table_id: u64,
    table: EnchantTable,
    pub item: Option<u16>,
    item_slots: u8,
    tab: Tab,
    picked: Option<usize>,
    scroll: usize,
    sent: Option<Sent>,
}

impl Enchant {
    pub fn open(table_id: u64, table: EnchantTable) -> Self {
        let mut frame = Frame::new(POSITION, WIDTH);
        frame.open = true;
        Self { frame, table_id, table, item: None, item_slots: 0, tab: Tab::Normal, picked: None, scroll: 0, sent: None }
    }

    /// Drops an item in. `Err(Some(id))` is a message; `Err(None)` means a request is still waiting, so the item
    /// that's already in stays — the answer has to land on it.
    pub fn try_item(&mut self, item: &InventoryItem, slots: u8) -> Result<(), Option<usize>> {
        if self.sent.is_some() {
            return Err(None);
        }
        if let Some(message) = refuse_item(&self.table, item.id, item.worn, item.refine, item.grade) {
            return Err(Some(message));
        }
        self.item = Some(item.index);
        self.item_slots = slots;
        self.scroll = 0;
        self.picked = Some(0);
        self.sent = None;
        self.tab = tabs_for(&self.table, item.cards, slots).first();
        Ok(())
    }

    /// The write failed, so no answer is coming and another item can go in.
    pub fn forget_request(&mut self) {
        self.sent = None;
    }

    pub fn scroll_by(&mut self, rows: i32, item: Option<&InventoryItem>) {
        let max = self.rows(item).len().saturating_sub(VISIBLE) as i32;
        self.scroll = (self.scroll as i32 + rows).clamp(0, max) as usize;
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let local = self.frame.local(cursor, HEIGHT)?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(Hit::Title(hit));
        }
        let tab = (0..4).find(|&index| contains(tab_at(index), TAB_SIZE, local));
        let row = (0..VISIBLE).find(|&row| contains(row_at(row), ROW_SIZE, local));
        Some(match (tab, row) {
            (Some(index), _) => Hit::Tab(TABS[index]),
            (_, Some(row)) => Hit::Row(row),
            _ if contains(ACTION_AT, ACTION_SIZE, local) => Hit::Action,
            _ => Hit::Window,
        })
    }

    pub fn click(&mut self, hit: Hit, item: Option<&InventoryItem>, count: impl FnMut(u32) -> u32, zeny: u32) -> Option<Action> {
        let cards = item.map(|item| item.cards).unwrap_or([0; 4]);
        let enabled = tabs_for(&self.table, cards, self.item_slots);
        match hit {
            Hit::Title(TitleHit::Close) => return Some(Action::Close),
            Hit::Title(title) => self.frame.click_title(title),
            Hit::Tab(tab) if enabled.has(tab) => {
                self.tab = tab;
                self.picked = Some(0);
                self.scroll = 0;
            }
            Hit::Row(row) => {
                let index = self.scroll + row;
                if index < self.rows(item).len() {
                    self.picked = Some(index);
                }
            }
            Hit::Action => return self.act(item, count, zeny),
            Hit::Tab(_) | Hit::Window => {}
        }
        None
    }

    /// The cards after `0x0B9F`, from the request that was sent.
    pub fn cards_after(&self, message: u32, result: u32, cards: [u32; 4]) -> [u32; 4] {
        let Some(sent) = self.sent else { return cards };
        let success = message == SUCCESS as u32;
        let outcome = match sent {
            Sent::Fill { slot } => Outcome::Fill { slot },
            Sent::Upgrade { slot } => Outcome::Upgrade { slot },
            Sent::Reset => Outcome::Reset,
        };
        cards_after(outcome, cards, self.item_slots, result, success)
    }

    fn act(&mut self, item: Option<&InventoryItem>, count: impl FnMut(u32) -> u32, zeny: u32) -> Option<Action> {
        if self.sent.is_some() {
            return None;
        }
        let item = item?;
        let cards = item.cards;
        let rows = self.rows(Some(item));
        let request = match self.tab {
            Tab::Normal => {
                let slot = next_slot(&self.table.slot_order, cards, self.item_slots)?;
                let info = self.table.slots[usize::from(slot)].as_ref()?;
                if !info.random() {
                    return None;
                }
                if let Some(message) = refuse_cost(info.cost.zeny, &info.cost.materials, zeny, count) {
                    return Some(Action::Refuse(message));
                }
                self.sent = Some(Sent::Fill { slot });
                Request::Random { table: self.table_id, index: item.index }
            }
            Tab::Perfect => {
                let slot = next_slot(&self.table.slot_order, cards, self.item_slots)?;
                let row = rows.get(self.picked?)?;
                if let Some(message) = refuse_cost(row.zeny, &row.materials, zeny, count) {
                    return Some(Action::Refuse(message));
                }
                self.sent = Some(Sent::Fill { slot });
                Request::Perfect { table: self.table_id, index: item.index, item: row.pick }
            }
            Tab::Upgrade => {
                let row = rows.get(self.picked?)?;
                if let Some(message) = refuse_cost(row.zeny, &row.materials, zeny, count) {
                    return Some(Action::Refuse(message));
                }
                self.sent = Some(Sent::Upgrade { slot: row.slot });
                Request::Upgrade { table: self.table_id, index: item.index, slot: u16::from(row.slot) }
            }
            Tab::Reset => {
                let reset = self.table.reset.as_ref()?;
                if !tabs_for(&self.table, cards, self.item_slots).reset {
                    return None;
                }
                if let Some(message) = refuse_cost(reset.cost.zeny, &reset.cost.materials, zeny, count) {
                    return Some(Action::Refuse(message));
                }
                self.sent = Some(Sent::Reset);
                Request::Reset { table: self.table_id, index: item.index }
            }
        };
        Some(Action::Send(request))
    }

    fn rows(&self, item: Option<&InventoryItem>) -> Vec<Shown> {
        let Some(item) = item.filter(|item| self.item == Some(item.index)) else { return Vec::new() };
        let cards = item.cards;
        match self.tab {
            Tab::Normal => {
                let Some(slot) = next_slot(&self.table.slot_order, cards, self.item_slots) else { return Vec::new() };
                let Some(info) = self.table.slots[usize::from(slot)].as_ref() else { return Vec::new() };
                info.rate_list(item.grade)
                    .iter()
                    .map(|&(id, weight)| Shown {
                        pick: id,
                        slot,
                        name: id,
                        result: None,
                        zeny: info.cost.zeny,
                        materials: info.cost.materials.clone(),
                        weight: Some(weight),
                    })
                    .collect()
            }
            Tab::Perfect => {
                let Some(slot) = next_slot(&self.table.slot_order, cards, self.item_slots) else { return Vec::new() };
                let Some(info) = self.table.slots[usize::from(slot)].as_ref() else { return Vec::new() };
                info.perfect
                    .iter()
                    .map(|perfect| Shown {
                        pick: perfect.item,
                        slot,
                        name: perfect.item,
                        result: None,
                        zeny: perfect.cost.zeny,
                        materials: perfect.cost.materials.clone(),
                        weight: None,
                    })
                    .collect()
            }
            Tab::Upgrade => (self.item_slots..4)
                .filter_map(|slot| {
                    let card = cards[usize::from(slot)];
                    let upgrade = self.table.slots[usize::from(slot)].as_ref()?.upgrades.iter().find(|(item, _)| *item == card)?;
                    Some(Shown {
                        pick: card,
                        slot,
                        name: card,
                        result: Some(upgrade.1.result),
                        zeny: upgrade.1.cost.zeny,
                        materials: upgrade.1.cost.materials.clone(),
                        weight: None,
                    })
                })
                .collect(),
            Tab::Reset => Vec::new(),
        }
    }

    pub fn button_boxes(&self) -> Vec<(&'static str, [f32; 4])> {
        if !self.frame.open || self.frame.minimized {
            return Vec::new();
        }
        let boxes = vec![
            ("normal", self.place(tab_at(0), TAB_SIZE)),
            ("perfect", self.place(tab_at(1), TAB_SIZE)),
            ("upgrade", self.place(tab_at(2), TAB_SIZE)),
            ("reset", self.place(tab_at(3), TAB_SIZE)),
            ("action", self.place(ACTION_AT, ACTION_SIZE)),
            ("item", self.place(ITEM_AT, ITEM_SIZE)),
            ("close", self.place(self.frame.close_button(), window::BUTTON_SIZE)),
        ];
        boxes
    }

    pub fn row_boxes(&self, item: Option<&InventoryItem>) -> Vec<(u32, [f32; 4])> {
        if !self.frame.open || self.frame.minimized {
            return Vec::new();
        }
        self.rows(item)
            .iter()
            .enumerate()
            .skip(self.scroll)
            .take(VISIBLE)
            .map(|(index, row)| (row.pick, self.place(row_at(index - self.scroll), ROW_SIZE)))
            .collect()
    }

    fn place(&self, at: [f32; 2], size: [f32; 2]) -> [f32; 4] {
        let [x, y] = self.frame.at(at);
        [x, y, size[0], size[1]]
    }

    pub fn icons(&self, items: &HashMap<u32, ItemInfo>, carried: &[InventoryItem]) -> Vec<String> {
        let item = self.item.and_then(|index| carried.iter().find(|item| item.index == index));
        let mut ids: Vec<u32> = item.map(|item| item.id).into_iter().chain(item.into_iter().flat_map(|item| item.cards)).filter(|id| *id != 0).collect();
        for row in self.rows(item) {
            ids.push(row.name);
            ids.extend(row.result);
            ids.extend(row.materials.iter().map(|(id, _)| *id));
        }
        ids.retain(|id| items.contains_key(id));
        ids.sort();
        ids.dedup();
        ids.into_iter().filter_map(|id| items.get(&id).map(|info| inventory::icon_path(info, true))).collect()
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
        let at = |local: [f32; 2]| self.frame.at(local);
        self.frame.draw_title(layer, chrome, message(TITLE), pointer.0);
        if self.frame.minimized {
            return;
        }
        let icon = |id: u32| items.get(&id).and_then(|info| icons.get(&inventory::icon_path(info, true))).and_then(Option::as_ref);
        let name_of = |id: u32| items.get(&id).map(|info| info.name.clone()).unwrap_or_else(|| id.to_string());
        layer.sprites.push((&assets.background, at([0.0, TITLE_HEIGHT]), [WIDTH, 470.0]));
        let carried_item = self.item.and_then(|index| carried.iter().find(|item| item.index == index));
        let cards = carried_item.map(|item| item.cards).unwrap_or([0; 4]);
        let enabled = tabs_for(&self.table, cards, self.item_slots);
        for (index, tab) in TABS.into_iter().enumerate() {
            let look = if !enabled.has(tab) {
                2
            } else if self.tab == tab {
                1
            } else {
                0
            };
            layer.sprites.push((&assets.tabs[index][look], at(tab_at(index)), TAB_SIZE));
            let label = message(TAB_LABELS[index]);
            let color = if enabled.has(tab) { [0, 0, 0] } else { GREY };
            layer.texts.push((label.to_owned(), at(offset(tab_at(index), [(TAB_SIZE[0] - font.width(label)) / 2.0, 4.0])), color));
        }
        layer.sprites.push((&assets.item, at(ITEM_AT), ITEM_SIZE));
        if let Some(item) = carried_item
            && let Some(texture) = icon(item.id)
        {
            layer.sprites.push((texture, at(offset(ITEM_AT, [4.0, 4.0])), [22.0, 22.0]));
            let name = window::fit(font, &inventory::item_name(items, item.id, true, item.refine), 110.0);
            layer.texts.push((name, at(offset(ITEM_AT, [30.0, 8.0])), [0, 0, 0]));
        }
        let next = carried_item.and_then(|_| next_slot(&self.table.slot_order, cards, self.item_slots));
        for (slot, card) in cards.iter().enumerate() {
            let column = slot % 2;
            let row = slot / 2;
            let place = offset(SOCKET_AT, [SOCKET_STEP[0] * column as f32, SOCKET_STEP[1] * row as f32]);
            layer.sprites.push((&assets.sockets[slot], at(place), SOCKET_SIZE));
            if next == Some(slot as u8) {
                layer.sprites.push((&assets.socket_on, at(place), SOCKET_SIZE));
            }
            if let Some(texture) = icon(*card) {
                layer.sprites.push((texture, at(offset(place, [40.0, 28.0])), [24.0, 24.0]));
            }
        }
        let rows = self.rows(carried_item);
        if let Some(header) = self.header(carried_item, items, font) {
            layer.texts.push((header, at([ROW_LEFT, ROW_TOP - 18.0]), [0, 0, 0]));
        }
        for (index, row) in rows.iter().enumerate().skip(self.scroll).take(VISIBLE) {
            let place = row_at(index - self.scroll);
            let picked = self.picked == Some(index);
            layer.sprites.push((if picked { &assets.row_on } else { &assets.row }, at(place), ROW_SIZE));
            if let Some(texture) = icon(row.name) {
                layer.sprites.push((texture, at(offset(place, [2.0, 2.0])), [18.0, 18.0]));
            }
            let mut text = name_of(row.name);
            if let Some(result) = row.result {
                text = format!("{text} > {}", name_of(result));
            }
            if let Some(weight) = row.weight {
                text = format!("{text}  {}%", weight / 1000);
            } else if self.tab != Tab::Normal {
                text = format!("{text}  {} z{}", group(u64::from(row.zeny)), materials_text(&row.materials, items));
            }
            layer.texts.push((window::fit(font, &text, ROW_SIZE[0] - 28.0), at(offset(place, [24.0, 4.0])), [0, 0, 0]));
        }
        let normal_ready = next_slot(&self.table.slot_order, cards, self.item_slots)
            .and_then(|slot| self.table.slots[usize::from(slot)].as_ref())
            .is_some_and(EnchantSlot::random);
        let ready = self.sent.is_none()
            && carried_item.is_some()
            && match self.tab {
                Tab::Reset => enabled.reset,
                Tab::Normal => normal_ready,
                _ => self.picked.and_then(|index| rows.get(index)).is_some(),
            };
        layer.sprites.push((&assets.action[usize::from(!ready)], at(ACTION_AT), ACTION_SIZE));
        let label = message(match self.tab {
            Tab::Upgrade => UPGRADE_LABEL,
            Tab::Reset => RESET_LABEL,
            _ => ENCHANT_LABEL,
        });
        layer.texts.push((label.to_owned(), at([ACTION_AT[0] + (ACTION_SIZE[0] - font.width(label)) / 2.0, ACTION_AT[1] + 12.0]), [0, 0, 0]));
    }

    fn header(&self, item: Option<&InventoryItem>, items: &HashMap<u32, ItemInfo>, font: &Font) -> Option<String> {
        let item = item?;
        let text = match self.tab {
            Tab::Normal => {
                let slot = next_slot(&self.table.slot_order, item.cards, self.item_slots)?;
                let info = self.table.slots[usize::from(slot)].as_ref()?;
                let chance = info.success_rate.saturating_add(info.bonus(item.grade)) / 1000;
                format!("{chance}%  {} z{}", group(u64::from(info.cost.zeny)), materials_text(&info.cost.materials, items))
            }
            Tab::Reset => {
                let reset = self.table.reset.as_ref()?;
                let chance = reset.rate / 1000;
                format!("{chance}%  {} z{}", group(u64::from(reset.cost.zeny)), materials_text(&reset.cost.materials, items))
            }
            _ => return None,
        };
        Some(window::fit(font, &text, 420.0))
    }
}

fn materials_text(materials: &[(u32, u32)], items: &HashMap<u32, ItemInfo>) -> String {
    let mut text = String::new();
    for &(id, amount) in materials {
        let name = items.get(&id).map(|info| info.name.as_str()).unwrap_or("");
        text.push_str(&format!(", {name} x{amount}"));
    }
    text
}

fn tab_at(index: usize) -> [f32; 2] {
    [TAB_LEFT + index as f32 * (TAB_SIZE[0] + TAB_GAP), TAB_TOP]
}

fn row_at(index: usize) -> [f32; 2] {
    [ROW_LEFT, ROW_TOP + index as f32 * ROW_STEP]
}

#[cfg(test)]
mod tests {
    use rustnarok_client::lua::PerfectEnchant;

    use super::*;

    fn cost(zeny: u32, materials: Vec<(u32, u32)>) -> rustnarok_client::lua::EnchantCost {
        rustnarok_client::lua::EnchantCost { zeny, materials }
    }

    fn slot(perfect: Vec<PerfectEnchant>, upgrades: Vec<(u32, rustnarok_client::lua::EnchantUpgrade)>, rates: Vec<(u8, Vec<(u32, u32)>)>) -> EnchantSlot {
        EnchantSlot { cost: cost(1_000, vec![(25669, 1)]), success_rate: 80_000, grade_bonus: vec![(1, 5_000)], rates, perfect, upgrades }
    }

    fn table() -> EnchantTable {
        let perfect = vec![PerfectEnchant { item: 4818, cost: cost(100_000, vec![(25669, 20)]) }];
        let upgrade = vec![(4818, rustnarok_client::lua::EnchantUpgrade { result: 4817, cost: cost(200_000, vec![(25669, 40)]) })];
        let mut slots: [Option<EnchantSlot>; 4] = [None, None, None, None];
        slots[3] = Some(slot(perfect.clone(), upgrade.clone(), vec![]));
        slots[2] = Some(slot(perfect, vec![], vec![(0, vec![(4818, 100_000)])]));
        EnchantTable {
            targets: vec![600028],
            slot_order: vec![3, 2],
            min_refine: 7,
            min_grade: 1,
            approve_random_opt: false,
            reset: Some(rustnarok_client::lua::EnchantReset { enabled: true, rate: 100_000, cost: cost(500_000, vec![]) }),
            slots,
        }
    }

    fn item(cards: [u32; 4]) -> InventoryItem {
        InventoryItem { index: 4, id: 600028, refine: 9, grade: 1, cards, identified: true, ..Default::default() }
    }

    #[test]
    fn next_slot_follows_the_order_and_stops_when_none_are_free() {
        let order = [3, 2];
        assert_eq!(next_slot(&order, [0; 4], 2), Some(3));
        // A slot below the item's own card slots is skipped, even when it's first in the order.
        assert_eq!(next_slot(&[1, 3], [0; 4], 2), Some(3));
        assert_eq!(next_slot(&order, [0, 0, 0, 4818], 2), Some(2));
        assert_eq!(next_slot(&order, [0, 0, 4818, 4818], 2), None);
    }

    #[test]
    fn tabs_follow_the_item() {
        let table = table();
        let open = tabs_for(&table, [0; 4], 2);
        assert_eq!((open.normal, open.perfect, open.upgrade, open.reset), (false, true, false, false));
        let filled = tabs_for(&table, [0, 0, 0, 4818], 2);
        // Slot 3 is taken, so the next slot is 2, which has random enchants; slot 3 can be upgraded and reset.
        assert_eq!((filled.normal, filled.perfect, filled.upgrade, filled.reset), (true, true, true, true));
        let mut no_reset = table.clone();
        no_reset.reset.as_mut().unwrap().rate = 0;
        assert!(!tabs_for(&no_reset, [0, 0, 0, 4818], 2).reset);
    }

    #[test]
    fn refuses_what_the_client_can_check() {
        let table = table();
        assert_eq!(refuse_item(&table, 1201, 0, 9, 1), Some(CONDITIONS));
        assert_eq!(refuse_item(&table, 600028, 2, 9, 1), Some(CONDITIONS));
        assert_eq!(refuse_item(&table, 600028, 0, 6, 1), Some(CONDITIONS));
        assert_eq!(refuse_item(&table, 600028, 0, 9, 0), Some(CONDITIONS));
        assert_eq!(refuse_item(&table, 600028, 0, 9, 1), None);
        let count = |id: u32| if id == 25669 { 10 } else { 0 };
        assert_eq!(refuse_cost(100_000, &[(25669, 20)], 50_000, count), Some(NO_ZENY));
        assert_eq!(refuse_cost(100_000, &[(25669, 20)], 100_000, count), Some(NO_MATERIALS));
        // Two stacks of 15 are enough once the caller has summed them.
        assert_eq!(refuse_cost(100_000, &[(25669, 20)], 100_000, |_| 30), None);
    }

    #[test]
    fn cards_change_only_on_a_success() {
        let cards = [0, 0, 0, 0];
        assert_eq!(cards_after(Outcome::Fill { slot: 3 }, cards, 2, 4818, true), [0, 0, 0, 4818]);
        assert_eq!(cards_after(Outcome::Fill { slot: 3 }, cards, 2, 4818, false), cards);
        assert_eq!(cards_after(Outcome::Upgrade { slot: 3 }, [0, 0, 0, 4818], 2, 4817, true), [0, 0, 0, 4817]);
        assert_eq!(cards_after(Outcome::Upgrade { slot: 3 }, [0, 0, 0, 4818], 2, 0, false), [0, 0, 0, 4818]);
        assert_eq!(cards_after(Outcome::Reset, [9, 8, 4818, 4817], 2, 0, true), [9, 8, 0, 0]);
        assert_eq!(cards_after(Outcome::Reset, [9, 8, 4818, 4817], 2, 0, false), [9, 8, 4818, 4817]);
    }

    #[test]
    fn the_window_sends_a_perfect_enchant_and_remembers_the_slot() {
        let mut window = Enchant::open(33, table());
        let worn = InventoryItem { worn: 2, ..item([0; 4]) };
        assert_eq!(window.try_item(&worn, 2), Err(Some(CONDITIONS)));
        let item = item([0; 4]);
        window.try_item(&item, 2).unwrap();
        assert_eq!(window.tab, Tab::Perfect);
        let count = |id: u32| if id == 25669 { 20 } else { 0 };
        assert_eq!(window.click(Hit::Action, Some(&item), count, 50_000), Some(Action::Refuse(NO_ZENY)));
        assert!(window.sent.is_none());
        let sent = window.click(Hit::Action, Some(&item), count, 100_000);
        assert_eq!(sent, Some(Action::Send(Request::Perfect { table: 33, index: 4, item: 4818 })));
        // Another drop while that request is out would forget the slot, so it's refused and the item stays.
        let other = InventoryItem { index: 9, ..item };
        assert_eq!(window.try_item(&other, 2), Err(None));
        assert_eq!(window.item, Some(4));
        assert_eq!(window.cards_after(SUCCESS as u32, 4818, item.cards), [0, 0, 0, 4818]);
        assert_eq!(window.cards_after(FAILED as u32, 0, item.cards), item.cards);
        window.forget_request();
        window.try_item(&other, 2).unwrap();
        assert_eq!(window.item, Some(9));
    }
}
