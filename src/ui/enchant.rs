//! The enchant window (2021+ enchanters): an item dropped in, then a random enchant, a chosen perfect one,
//! an upgrade, or a reset. The table comes from the client's Lua; rathena only checks and answers.

use std::collections::HashMap;

use rustnarok_client::grf::Grf;
use rustnarok_client::lua::{EnchantSlot, EnchantTable, ItemInfo};
use rustnarok_client::net::InventoryItem;
use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};
use crate::window::{self, Chrome, Frame, TitleHit};
use crate::{Layer, UI, contains, offset};
use crate::{inventory, iteminfo};

// The places are the game's own; `page` turns a place on the 470×377 page under the tabs into one in the window.
const WIDTH: f32 = 690.0;
pub const HEIGHT: f32 = 470.0;
const POSITION: [f32; 2] = [167.0, 60.0];
/// The carried items it takes, down the left: 200×50 rows from under the title, 8 shown.
const TARGET_TOP: f32 = 17.0;
const TARGET_SIZE: [f32; 2] = [200.0, 50.0];
const TARGETS_SHOWN: usize = 8;
/// Normal, Upgrade and Reset.
const TABS_AT: [f32; 2] = [273.0, 18.0];
const TAB_SIZE: [f32; 2] = [65.0, 22.0];
/// Random and Perfect, on the Normal and Upgrade pages.
const SUB_TABS_AT: [f32; 2] = [227.0, 41.0];
const SUB_TAB_SIZE: [f32; 2] = [38.0, 20.0];
const PAGE: [f32; 2] = [220.0, 40.0];
const PAGE_SIZE: [f32; 2] = [470.0, 377.0];
/// The item's picture in the middle of the four sockets.
const PICTURE_AT: [f32; 2] = page(112.0, 5.0);
const PICTURE_SIZE: [f32; 2] = [75.0, 100.0];
/// Card slot 0 to 3: top left, bottom left, bottom right, top right; each a socket with its card's name under it.
const SLOTS: [[f32; 2]; 4] = [page(6.0, 20.0), page(29.0, 105.0), page(164.0, 105.0), page(187.0, 20.0)];
const SOCKET_SIZE: [f32; 2] = [44.0, 44.0];
const SLOT_NAME_WIDTH: f32 = 96.0;
/// Up to 8 materials, 4 to a row.
const MATERIALS_AT: [f32; 2] = page(4.0, 200.0);
const MATERIAL_STEP: [f32; 2] = [74.0, 52.0];
const MATERIAL_SIZE: [f32; 2] = [70.0, 50.0];
/// The zeny, right-aligned in the TAX box.
const ZENY_AT: [f32; 2] = page(150.0, 309.0);
const ZENY_WIDTH: f32 = 100.0;
const ACTION_AT: [f32; 2] = page(80.0, 330.0);
const ACTION_SIZE: [f32; 2] = [140.0, 40.0];
/// The enchants down the right: 150×30 rows, 12 shown.
const ROW_AT: [f32; 2] = page(300.0, 0.0);
const ROW_SIZE: [f32; 2] = [150.0, 30.0];
const VISIBLE: usize = 12;
/// The box across the bottom.
const NOTE_AT: [f32; 2] = [14.0, 425.0];

/// An item icon (a material's, a card's) and where it sits in its box.
const ICON_SIZE: [f32; 2] = [24.0, 24.0];
const MATERIAL_ICON: [f32; 2] = [6.0, 10.0];
const CARD_ICON: [f32; 2] = [10.0, 10.0];
/// A material's numbers, carried over "/needed", right-aligned at the box's right: near white outlined in blue with
/// enough carried, red without an outline when short.
const MATERIAL_NUMBERS: [f32; 2] = [70.0, 17.0];
const ENOUGH: [u8; 3] = [240, 240, 240];
const ENOUGH_OUTLINE: [u8; 3] = [50, 85, 150];
const SHORT: [u8; 3] = [230, 60, 90];

const fn page(x: f32, y: f32) -> [f32; 2] {
    [PAGE[0] + x, PAGE[1] + y]
}

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

/// The three pages the tabs along the top open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Normal,
    Upgrade,
    Reset,
}

const PAGES: [Page; 3] = [Page::Normal, Page::Upgrade, Page::Reset];

impl Tab {
    fn page(self) -> Page {
        match self {
            Tab::Normal | Tab::Perfect => Page::Normal,
            Tab::Upgrade => Page::Upgrade,
            Tab::Reset => Page::Reset,
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Hit {
    Title(TitleHit),
    Page(Page),
    /// Random (false) or Perfect (true).
    Sub(bool),
    /// A row of the carried items on the left.
    Target(usize),
    /// A row of the enchants on the right.
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
}

/// The item under the pointer, whose name the tooltip shows.
#[derive(Debug, PartialEq)]
pub enum Hovered<'b> {
    /// A carried item: a row of the left list, or the one in the picture.
    Carried(&'b InventoryItem),
    /// An enchant row's, a material's or a card's.
    Item(u32),
}

pub struct Assets {
    background: Texture,
    /// The page under the tabs: plain (Reset), blue (Random) and yellow (Perfect).
    pages: [Texture; 3],
    /// Normal, Upgrade and Reset; each out and chosen.
    tabs: [[Texture; 2]; 3],
    /// Random and Perfect; each out, chosen and off.
    sub_tabs: [[Texture; 3]; 2],
    /// A carried item's row: out, pointed at, chosen.
    target: [Texture; 3],
    /// An enchant's row: out, pointed at, chosen.
    row: [Texture; 3],
    /// A card slot's socket: out, pointed at, the one the enchant goes in.
    socket: [Texture; 3],
    /// A material's box: too few, enough.
    material: [Texture; 2],
    /// The enchant button: out, pointed at, held, off.
    action: [Texture; 4],
    grades: [Texture; 4],
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        // Older data (2022 kRO) has no enchantui folder: blank pictures then, rather than no client at all.
        let load = |path: String| gpu.try_load_texture(grf, &path, false).unwrap_or_else(|_| gpu.solid([0, 0, 0, 0]));
        let image = |name: &str| load(format!("{UI}enchantui\\{name}.bmp"));
        let states = |name: &str, states: [&str; 3]| states.map(|state| image(&format!("{name}{state}")));
        Self {
            background: image("bg_enchantui"),
            pages: [image("bg_subpage"), image("bg_subpageb"), image("bg_subpagey")],
            tabs: ["common", "grow", "reset"].map(|name| [image(&format!("tab_enc_{name}_out")), image(&format!("tab_enc_{name}_select"))]),
            sub_tabs: ["random", "confirm"].map(|name| states(&format!("tab_enc_{name}_"), ["out", "pressed", "disable"])),
            target: states("bg_enc_gear", ["", "_over", "_press"]),
            row: states("btn_enc_item", ["", "_over", "_press"]),
            socket: states("btn_socket_", ["out", "over", "select"]),
            material: [image("bg_enc_off"), image("bg_enc_on")],
            action: [image("btn_enc"), image("btn_enc_over"), image("btn_enc_press"), image("btn_enc_dis")],
            grades: [1, 2, 3, 4].map(|grade| load(format!("{UI}grade_enchant\\grade_icon{grade}.bmp"))),
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
    target_scroll: usize,
    sent: Option<Sent>,
}

impl Enchant {
    pub fn open(table_id: u64, table: EnchantTable) -> Self {
        let mut frame = Frame::new(POSITION, WIDTH);
        frame.open = true;
        Self { frame, table_id, table, item: None, item_slots: 0, tab: Tab::Normal, picked: None, scroll: 0, target_scroll: 0, sent: None }
    }

    /// Takes an item in (a row on the left, or one dropped on the window). `Err(Some(id))` is a message; `Err(None)`
    /// means a request is still waiting, so the item that's already in stays — the answer has to land on it.
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

    /// What the tabs offer for the item in; nothing before one is.
    fn enabled(&self, item: Option<&InventoryItem>) -> TabSet {
        item.map_or(TabSet { normal: false, perfect: false, upgrade: false, reset: false }, |item| tabs_for(&self.table, item.cards, self.item_slots))
    }

    fn chosen<'b>(&self, carried: &'b [InventoryItem]) -> Option<&'b InventoryItem> {
        self.item.and_then(|index| carried.iter().find(|item| item.index == index))
    }

    /// The carried items it takes (none worn), in the inventory's order.
    fn targets<'b>(&self, carried: &'b [InventoryItem]) -> Vec<&'b InventoryItem> {
        carried.iter().filter(|item| item.worn == 0 && self.table.targets.contains(&item.id)).collect()
    }

    /// The wheel scrolls the list it's over.
    pub fn wheel(&mut self, cursor: [f32; 2], rows: i32, carried: &[InventoryItem]) {
        let on_targets = self.frame.local(cursor, HEIGHT).is_some_and(|local| local[0] < PAGE[0]);
        let (count, shown) = if on_targets { (self.targets(carried).len(), TARGETS_SHOWN) } else { (self.rows(self.chosen(carried)).len(), VISIBLE) };
        let scroll = if on_targets { &mut self.target_scroll } else { &mut self.scroll };
        *scroll = (*scroll as i32 + rows).clamp(0, count.saturating_sub(shown) as i32) as usize;
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let local = self.frame.local(cursor, HEIGHT)?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(Hit::Title(hit));
        }
        let page = (0..3).find(|&index| contains(tab_at(index), TAB_SIZE, local)).map(|index| Hit::Page(PAGES[index]));
        let sub = (0..2).find(|&index| self.tab.page() != Page::Reset && contains(sub_tab_at(index), SUB_TAB_SIZE, local)).map(|index| Hit::Sub(index == 1));
        let target = (0..TARGETS_SHOWN).find(|&row| contains(target_at(row), TARGET_SIZE, local)).map(Hit::Target);
        let row = (0..VISIBLE).find(|&row| contains(row_at(row), ROW_SIZE, local)).map(Hit::Row);
        let action = contains(ACTION_AT, ACTION_SIZE, local).then_some(Hit::Action);
        Some(page.or(sub).or(target).or(row).or(action).unwrap_or(Hit::Window))
    }

    /// The item under the pointer: anywhere on a row of either list, or on the picture, a material's icon or a card's.
    pub fn hovered<'b>(&self, cursor: [f32; 2], carried: &'b [InventoryItem]) -> Option<Hovered<'b>> {
        let local = self.frame.local(cursor, HEIGHT)?;
        if self.frame.title_hit(local).is_some() {
            return None;
        }
        let on = |at: [f32; 2], size: [f32; 2]| contains(at, size, local);
        let targets = self.targets(carried);
        if let Some(row) = (0..TARGETS_SHOWN).find(|&row| on(target_at(row), TARGET_SIZE)) {
            return targets.get(self.target_scroll + row).map(|&item| Hovered::Carried(item));
        }
        let item = self.chosen(carried);
        if on(PICTURE_AT, PICTURE_SIZE) {
            return item.map(Hovered::Carried);
        }
        if let Some(row) = (0..VISIBLE).find(|&row| on(row_at(row), ROW_SIZE)) {
            return self.rows(item).get(self.scroll + row).map(|row| Hovered::Item(row.result.unwrap_or(row.name)));
        }
        let materials = self.cost(item).map(|(_, materials)| materials).unwrap_or_default();
        if let Some(&(id, _)) =
            materials.iter().take(8).enumerate().find(|&(index, _)| on(offset(material_at(index), MATERIAL_ICON), ICON_SIZE)).map(|(_, material)| material)
        {
            return Some(Hovered::Item(id));
        }
        let cards = item.map_or([0; 4], |item| item.cards);
        (0..4).find(|&slot| cards[slot] != 0 && on(offset(socket_at(slot), CARD_ICON), ICON_SIZE)).map(|slot| Hovered::Item(cards[slot]))
    }

    pub fn click(&mut self, hit: Hit, carried: &[InventoryItem], slots: impl Fn(u32) -> u8, count: impl FnMut(u32) -> u32, zeny: u32) -> Option<Action> {
        let item = self.chosen(carried);
        let enabled = self.enabled(item);
        let tab = match hit {
            Hit::Title(TitleHit::Close) => return Some(Action::Close),
            // It has no minimize button.
            Hit::Title(_) | Hit::Window => return None,
            Hit::Page(Page::Normal) => [Tab::Normal, Tab::Perfect].into_iter().find(|tab| enabled.has(*tab)),
            Hit::Page(Page::Upgrade) => Some(Tab::Upgrade).filter(|tab| enabled.has(*tab)),
            Hit::Page(Page::Reset) => Some(Tab::Reset).filter(|tab| enabled.has(*tab)),
            Hit::Sub(perfect) => Some(if perfect { Tab::Perfect } else { Tab::Normal }).filter(|tab| self.tab.page() == Page::Normal && enabled.has(*tab)),
            Hit::Target(row) => {
                let target = *self.targets(carried).get(self.target_scroll + row)?;
                return match self.try_item(target, slots(target.id)) {
                    Err(Some(message)) => Some(Action::Refuse(message)),
                    _ => None,
                };
            }
            Hit::Row(row) => {
                let index = self.scroll + row;
                if index < self.rows(item).len() {
                    self.picked = Some(index);
                }
                return None;
            }
            Hit::Action => return self.act(item, count, zeny),
        };
        if let Some(tab) = tab {
            self.tab = tab;
            self.picked = Some(0);
            self.scroll = 0;
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
                    .map(|&(id, _)| Shown { pick: id, slot, name: id, result: None, zeny: info.cost.zeny, materials: info.cost.materials.clone() })
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
                    })
                })
                .collect(),
            Tab::Reset => Vec::new(),
        }
    }

    /// The zeny and materials of what the button would do now.
    fn cost(&self, item: Option<&InventoryItem>) -> Option<(u32, Vec<(u32, u32)>)> {
        let item = item?;
        match self.tab {
            Tab::Normal => {
                let slot = next_slot(&self.table.slot_order, item.cards, self.item_slots)?;
                let info = self.table.slots[usize::from(slot)].as_ref()?;
                Some((info.cost.zeny, info.cost.materials.clone()))
            }
            Tab::Reset => self.table.reset.as_ref().map(|reset| (reset.cost.zeny, reset.cost.materials.clone())),
            Tab::Perfect | Tab::Upgrade => self.rows(Some(item)).into_iter().nth(self.picked?).map(|row| (row.zeny, row.materials)),
        }
    }

    /// The slot the enchant goes in: the next free one, or the picked upgrade's.
    fn aimed_slot(&self, item: Option<&InventoryItem>) -> Option<u8> {
        let item = item?;
        match self.tab {
            Tab::Normal | Tab::Perfect => next_slot(&self.table.slot_order, item.cards, self.item_slots),
            Tab::Upgrade => self.rows(Some(item)).get(self.picked?).map(|row| row.slot),
            Tab::Reset => None,
        }
    }

    fn ready(&self, item: Option<&InventoryItem>) -> bool {
        let Some(item) = item.filter(|_| self.sent.is_none()) else { return false };
        let enabled = self.enabled(Some(item));
        match self.tab {
            Tab::Normal => enabled.normal,
            Tab::Reset => enabled.reset,
            Tab::Perfect | Tab::Upgrade => self.picked.is_some_and(|index| index < self.rows(Some(item)).len()),
        }
    }

    pub fn button_boxes(&self, carried: &[InventoryItem]) -> Vec<(String, [f32; 4])> {
        if !self.frame.open {
            return Vec::new();
        }
        let mut boxes: Vec<(String, [f32; 4])> =
            ["normal", "upgrade", "reset"].into_iter().enumerate().map(|(index, name)| (name.to_owned(), self.place(tab_at(index), TAB_SIZE))).collect();
        if self.tab.page() != Page::Reset {
            boxes.extend(["random", "perfect"].into_iter().enumerate().map(|(index, name)| (name.to_owned(), self.place(sub_tab_at(index), SUB_TAB_SIZE))));
        }
        boxes.push(("action".to_owned(), self.place(ACTION_AT, ACTION_SIZE)));
        // Where a dragged item goes in.
        boxes.push(("item".to_owned(), self.place(PICTURE_AT, PICTURE_SIZE)));
        boxes.push(("close".to_owned(), self.place(self.frame.close_button(), window::BUTTON_SIZE)));
        let materials = self.cost(self.chosen(carried)).map(|(_, materials)| materials).unwrap_or_default();
        boxes.extend(
            materials
                .iter()
                .take(8)
                .enumerate()
                .map(|(index, (id, _))| (format!("material_{id}"), self.place(offset(material_at(index), MATERIAL_ICON), ICON_SIZE))),
        );
        let targets = self.targets(carried).into_iter().skip(self.target_scroll).take(TARGETS_SHOWN);
        boxes.extend(targets.enumerate().map(|(row, item)| (format!("target_{}", item.id), self.place(target_at(row), TARGET_SIZE))));
        boxes
    }

    pub fn row_boxes(&self, carried: &[InventoryItem]) -> Vec<(u32, [f32; 4])> {
        if !self.frame.open {
            return Vec::new();
        }
        self.rows(self.chosen(carried))
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
        let item = self.chosen(carried);
        let mut ids: Vec<u32> = item.into_iter().flat_map(|item| item.cards).collect();
        ids.extend(self.targets(carried).iter().map(|item| item.id));
        for row in self.rows(item) {
            ids.push(row.name);
            ids.extend(row.result);
        }
        ids.extend(self.cost(item).into_iter().flat_map(|(_, materials)| materials).map(|(id, _)| id));
        ids.sort();
        ids.dedup();
        let mut paths: Vec<String> = ids.into_iter().filter_map(|id| items.get(&id).map(|info| inventory::icon_path(info, true))).collect();
        paths.extend(item.and_then(|item| items.get(&item.id)).map(|info| iteminfo::picture_path(info, true)));
        paths
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &self,
        layer: &mut Layer<'a>,
        tooltip: &mut Layer<'a>,
        naming: inventory::Naming,
        chrome: &'a Chrome,
        assets: &'a Assets,
        icons: &'a HashMap<String, Option<Texture>>,
        items: &HashMap<u32, ItemInfo>,
        carried: &[InventoryItem],
        messages: &[String],
        font: &Font,
        pointer: ([f32; 2], bool),
    ) {
        const BLACK: [u8; 3] = [0, 0, 0];
        let at = |local: [f32; 2]| self.frame.at(local);
        let over = |local: [f32; 2], size: [f32; 2]| contains(at(local), size, pointer.0);
        let icon = |id: u32| items.get(&id).and_then(|info| icons.get(&inventory::icon_path(info, true))).and_then(Option::as_ref);
        let name_of = |id: u32| items.get(&id).map(|info| info.name.clone()).unwrap_or_else(|| id.to_string());
        // The background brings the bar's place; the bar, the close button and the title go over it.
        layer.sprites.push((&assets.background, at([0.0, 0.0]), [WIDTH, HEIGHT]));
        self.frame.draw_bar(layer, chrome);
        self.frame.draw_bare_title(layer, chrome, messages.get(TITLE).map_or("", String::as_str), pointer.0);
        let item = self.chosen(carried);
        let cards = item.map_or([0; 4], |item| item.cards);
        let enabled = self.enabled(item);

        for (row, target) in self.targets(carried).into_iter().skip(self.target_scroll).take(TARGETS_SHOWN).enumerate() {
            let place = target_at(row);
            let look = if self.item == Some(target.index) { 2 } else { usize::from(over(place, TARGET_SIZE)) };
            layer.sprites.push((&assets.target[look], at(place), TARGET_SIZE));
            if let Some(texture) = icon(target.id) {
                layer.sprites.push((texture, at(offset(place, [6.0, 13.0])), [24.0, 24.0]));
            }
            if (1..=4).contains(&target.grade) {
                layer.sprites.push((&assets.grades[usize::from(target.grade) - 1], at(offset(place, [166.0, 7.0])), [12.0, 12.0]));
            }
            let name = window::fit(font, &inventory::item_name(items, target.id, target.identified, target.refine), 152.0);
            layer.texts.push((name, at(offset(place, [38.0, 19.0])), BLACK));
        }

        let page = self.tab.page();
        for (index, tab) in PAGES.into_iter().enumerate() {
            layer.sprites.push((&assets.tabs[index][usize::from(page == tab)], at(tab_at(index)), TAB_SIZE));
        }
        let background = match self.tab {
            Tab::Reset => 0,
            Tab::Normal => 1,
            Tab::Perfect | Tab::Upgrade => 2,
        };
        layer.sprites.push((&assets.pages[background], at(PAGE), PAGE_SIZE));
        // ponytail: an upgrade has one set result, so the Upgrade page's Random is off and its Perfect chosen.
        let sub_tabs = match page {
            Page::Normal => Some([(enabled.normal, self.tab == Tab::Normal), (enabled.perfect, self.tab == Tab::Perfect)]),
            Page::Upgrade => Some([(false, false), (enabled.upgrade, true)]),
            Page::Reset => None,
        };
        for (index, (on, chosen)) in sub_tabs.into_iter().flatten().enumerate() {
            let look = if !on { 2 } else { usize::from(chosen) };
            layer.sprites.push((&assets.sub_tabs[index][look], at(sub_tab_at(index)), SUB_TAB_SIZE));
        }

        if let Some(texture) =
            item.and_then(|item| items.get(&item.id)).and_then(|info| icons.get(&iteminfo::picture_path(info, true))).and_then(Option::as_ref)
        {
            layer.sprites.push((texture, at(PICTURE_AT), PICTURE_SIZE));
        }
        let aimed = self.aimed_slot(item);
        for (slot, &card) in cards.iter().enumerate() {
            // The right two's name boxes sit 10 px in.
            let name_x = if slot < 2 { 0.0 } else { 10.0 };
            let socket = socket_at(slot);
            let look = if aimed == Some(slot as u8) { 2 } else { usize::from(over(socket, SOCKET_SIZE)) };
            layer.sprites.push((&assets.socket[look], at(socket), SOCKET_SIZE));
            if card == 0 {
                continue;
            }
            if let Some(texture) = icon(card) {
                layer.sprites.push((texture, at(offset(socket, CARD_ICON)), ICON_SIZE));
            }
            let name = window::fit(font, &name_of(card), SLOT_NAME_WIDTH);
            let name_at = offset(SLOTS[slot], [name_x + (SLOT_NAME_WIDTH - font.width(&name)) / 2.0, 44.0]);
            layer.texts.push((name, at(name_at), BLACK));
        }

        if let Some((zeny, materials)) = self.cost(item) {
            for (index, &(id, need)) in materials.iter().take(8).enumerate() {
                let place = material_at(index);
                let have: u32 = carried.iter().filter(|carried| carried.id == id).map(|carried| u32::from(carried.amount)).sum();
                layer.sprites.push((&assets.material[usize::from(have >= need)], at(place), MATERIAL_SIZE));
                if let Some(texture) = icon(id) {
                    layer.sprites.push((texture, at(offset(place, MATERIAL_ICON)), ICON_SIZE));
                }
                for (line, text) in [have.to_string(), format!("/{need}")].into_iter().enumerate() {
                    let [right, top] = MATERIAL_NUMBERS;
                    let corner = at(offset(place, [right - font.width(&text), top + 14.0 * line as f32]));
                    if have < need {
                        layer.texts.push((text, corner, SHORT));
                        continue;
                    }
                    for step in [[-1.0, 0.0], [1.0, 0.0], [0.0, -1.0], [0.0, 1.0]] {
                        layer.texts.push((text.clone(), offset(corner, step), ENOUGH_OUTLINE));
                    }
                    layer.texts.push((text, corner, ENOUGH));
                }
            }
            let zeny = zeny.to_string();
            layer.texts.push((zeny.clone(), at([ZENY_AT[0] + ZENY_WIDTH - font.width(&zeny), ZENY_AT[1]]), BLACK));
        }
        let look = if !self.ready(item) {
            3
        } else if over(ACTION_AT, ACTION_SIZE) {
            1 + usize::from(pointer.1)
        } else {
            0
        };
        layer.sprites.push((&assets.action[look], at(ACTION_AT), ACTION_SIZE));

        for (index, row) in self.rows(item).iter().enumerate().skip(self.scroll).take(VISIBLE) {
            let place = row_at(index - self.scroll);
            // A random enchant's rows only list what may come.
            let look = if self.tab != Tab::Normal && self.picked == Some(index) { 2 } else { usize::from(over(place, ROW_SIZE)) };
            layer.sprites.push((&assets.row[look], at(place), ROW_SIZE));
            let shown = row.result.unwrap_or(row.name);
            if let Some(texture) = icon(shown) {
                layer.sprites.push((texture, at(offset(place, [2.0, 3.0])), [24.0, 24.0]));
            }
            layer.texts.push((window::fit(font, &name_of(shown), 101.0), at(offset(place, [28.0, 9.0])), BLACK));
        }
        if let Some(note) = self.note(item) {
            layer.texts.push((note, at(NOTE_AT), BLACK));
        }
        // The name of the item under the pointer, over it, in the item's colour, with its options' count.
        let hovered = match self.hovered(pointer.0, carried) {
            Some(Hovered::Carried(item)) => Some(item.clone()),
            Some(Hovered::Item(id)) => Some(InventoryItem { id, identified: true, ..Default::default() }),
            None => None,
        };
        if let Some(item) = hovered {
            let (cards, makers) = naming;
            let name = inventory::full_name(items, cards, messages, makers, &item, true);
            let text = inventory::option_text(messages, &name, &item);
            window::tooltip(tooltip, chrome, font, &text, offset(pointer.0, [0.0, -20.0]), inventory::name_color(&item, items));
        }
    }

    /// The box across the bottom: the chance of a random enchant or a reset.
    // ponytail: what the game writes there wasn't read; the chance is what this window had nowhere else to show.
    fn note(&self, item: Option<&InventoryItem>) -> Option<String> {
        let item = item?;
        let chance = match self.tab {
            Tab::Normal => {
                let slot = next_slot(&self.table.slot_order, item.cards, self.item_slots)?;
                let info = self.table.slots[usize::from(slot)].as_ref()?;
                info.success_rate.saturating_add(info.bonus(item.grade))
            }
            Tab::Reset => self.table.reset.as_ref()?.rate,
            Tab::Perfect | Tab::Upgrade => return None,
        };
        Some(format!("{}%", chance / 1000))
    }
}

fn tab_at(index: usize) -> [f32; 2] {
    [TABS_AT[0] + index as f32 * TAB_SIZE[0], TABS_AT[1]]
}

fn sub_tab_at(index: usize) -> [f32; 2] {
    [SUB_TABS_AT[0] + index as f32 * SUB_TAB_SIZE[0], SUB_TABS_AT[1]]
}

fn target_at(row: usize) -> [f32; 2] {
    [0.0, TARGET_TOP + row as f32 * TARGET_SIZE[1]]
}

fn material_at(index: usize) -> [f32; 2] {
    offset(MATERIALS_AT, [MATERIAL_STEP[0] * (index % 4) as f32, MATERIAL_STEP[1] * (index / 4) as f32])
}

/// A card slot's socket: the two on the left have it on their right, the two on the right on their left.
fn socket_at(slot: usize) -> [f32; 2] {
    offset(SLOTS[slot], [if slot < 2 { 59.0 } else { 3.0 }, 1.0])
}

fn row_at(row: usize) -> [f32; 2] {
    [ROW_AT[0], ROW_AT[1] + row as f32 * ROW_SIZE[1]]
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
        assert_eq!(window.click(Hit::Action, std::slice::from_ref(&item), |_| 2, count, 50_000), Some(Action::Refuse(NO_ZENY)));
        assert!(window.sent.is_none());
        let sent = window.click(Hit::Action, std::slice::from_ref(&item), |_| 2, count, 100_000);
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

    #[test]
    fn the_name_goes_with_the_item_under_the_pointer() {
        let mut window = Enchant::open(33, table());
        let item = item([0, 0, 0, 4818]);
        window.try_item(&item, 2).unwrap();
        window.tab = Tab::Perfect;
        let carried = std::slice::from_ref(&item);
        let at = |local: [f32; 2]| window.frame.at(offset(local, [5.0, 5.0]));
        // Anywhere on a left row, and the picture: the carried item itself.
        assert_eq!(window.hovered(at(offset(target_at(0), [150.0, 30.0])), carried), Some(Hovered::Carried(&item)));
        assert_eq!(window.hovered(at(PICTURE_AT), carried), Some(Hovered::Carried(&item)));
        // An enchant row, a material's icon (not the rest of its box) and a card's icon (not an empty socket).
        assert_eq!(window.hovered(at(offset(row_at(0), [100.0, 0.0])), carried), Some(Hovered::Item(4818)));
        window.tab = Tab::Normal;
        assert_eq!(window.hovered(at(offset(material_at(0), MATERIAL_ICON)), carried), Some(Hovered::Item(25669)));
        assert_eq!(window.hovered(at(offset(material_at(0), [50.0, 30.0])), carried), None);
        assert_eq!(window.hovered(at(offset(socket_at(3), CARD_ICON)), carried), Some(Hovered::Item(4818)));
        assert_eq!(window.hovered(at(offset(socket_at(0), CARD_ICON)), carried), None);
        // An empty left row, and the title bar.
        assert_eq!(window.hovered(at(target_at(1)), carried), None);
        assert_eq!(window.hovered(at([300.0, 0.0]), carried), None);
    }

    #[test]
    fn the_left_list_holds_the_carried_items_it_takes() {
        let mut window = Enchant::open(33, table());
        let low = InventoryItem { index: 2, refine: 6, ..item([0; 4]) };
        let worn = InventoryItem { index: 3, worn: 2, ..item([0; 4]) };
        let other = InventoryItem { index: 5, id: 1201, ..item([0; 4]) };
        let carried = [low, worn, other, item([0; 4])];
        // Neither the worn one nor one the table doesn't name is listed.
        assert_eq!(window.targets(&carried).iter().map(|item| item.index).collect::<Vec<_>>(), [2, 4]);
        let none = |_| 0;
        assert_eq!(window.click(Hit::Target(0), &carried, |_| 2, none, 0), Some(Action::Refuse(CONDITIONS)));
        assert_eq!(window.item, None);
        assert_eq!(window.click(Hit::Target(1), &carried, |_| 2, none, 0), None);
        assert_eq!((window.item, window.tab), (Some(4), Tab::Perfect));
        // The Normal tab opens the page on the kind that can be done: Random has nothing for this item.
        window.click(Hit::Sub(false), &carried, |_| 2, none, 0);
        assert_eq!(window.tab, Tab::Perfect);
        window.click(Hit::Page(Page::Upgrade), &carried, |_| 2, none, 0);
        assert_eq!(window.tab, Tab::Perfect);
    }
}
