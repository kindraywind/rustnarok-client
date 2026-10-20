//! The refine window (2016+ refiners, `@refineui`): the game's 264×346 machine. An item dropped on its slot, the ores it can be
//! refined with in slots across the top (chance, zeny), Blacksmith's Blessing, Refine and Cancel, and the notice bar.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use rustnarok_client::grf::Grf;
use rustnarok_client::lua::ItemInfo;
use rustnarok_client::net::InventoryItem;
use rustnarok_client::text::Font;

use crate::inventory::{self, CardNames};
use crate::render::{Gpu, Texture};
use crate::shop::group;
use crate::window::{self, Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI, contains, offset};

/// The window: 264 wide and 346 tall, centred on the screen when it opens.
pub const SIZE: [f32; 2] = [264.0, 346.0];
/// The machine's fill and picture.
const FILL_AT: [f32; 2] = [0.0, TITLE_HEIGHT];
const FILL_SIZE: [f32; 2] = [264.0, 302.0];
const MACHINE_AT: [f32; 2] = [1.0, TITLE_HEIGHT];
/// Frames change every 100 ms.
const FRAME_MS: u128 = 100;
/// The ore slots across the top: icon, count, the picked one's frame, and the click area, for ore i.
const SLOT_STEP: f32 = 49.0;
const ICON_Y: f32 = 69.0;
const COUNT_Y: f32 = 107.0;
const SLOT_SIZE: [f32; 2] = [46.0, 52.0];
const SLOT_Y: f32 = 55.0;
const SLOTS: usize = 4;
/// The blessing slot (slot 4).
const BLESSING_X: f32 = 207.0;
/// The anvil's picked ore and blessing icons.
const ANVIL_ORE: [f32; 2] = [81.0, 150.0];
const ANVIL_BLESSING: [f32; 2] = [161.0, 150.0];
const ICON: [f32; 2] = [24.0, 24.0];
/// The item slot and its icon, the grade under it, the name and the rate lines.
const ITEM_SLOT: [f32; 2] = [109.0, 201.0];
const ITEM_SLOT_SIZE: [f32; 2] = [46.0, 46.0];
const ITEM_ICON: [f32; 2] = [120.0, 212.0];
const GRADE_AT: [f32; 2] = [120.0, 224.0];
const NAME_Y: f32 = 246.0;
const RATE_Y: f32 = 31.0;
const REFINE: [f32; 2] = [84.0, 262.0];
const BUTTON: [f32; 2] = [96.0, 48.0];
const NOTICE: [f32; 2] = [6.0, 322.0];
const NOTICE_WIDTH: f32 = 176.0;
const CANCEL: [f32; 2] = [204.0, 322.0];
const CANCEL_SIZE: [f32; 2] = [56.0, 20.0];
const RED: [u8; 3] = [0xCA, 0x15, 0x2E];
const BLUE: [u8; 3] = [0x1C, 0x28, 0xC4];
const NAME_COLOR: [u8; 3] = [0x17, 0x21, 0x46];
const SUCCESS_COLOR: [u8; 3] = [0x1F, 0x2B, 0x42];
const CHANCE_COLOR: [u8; 3] = [0x2A, 0x61, 0xDE];
const LABEL_COLOR: [u8; 3] = [0x12, 0x1B, 0x3E];
const LABEL_OFF_COLOR: [u8; 3] = [0x4A, 0x64, 0x94];
const TITLE: usize = 2966;
const CANCEL_LABEL: usize = 218;
const SUCCESS_LABEL: usize = 2974;
/// Blacksmith's Blessing.
pub const BLESSING_ITEM: u32 = 6635;
/// The notices: Use on, too little zeny, too few blessings, nothing to refine it with.
const USE_ON: usize = 2967;
const NO_ZENY: usize = 2968;
const NO_BLESSINGS: usize = 2969;
pub const CANT_REFINE: usize = 2970;
/// How long a timed notice shows.
const NOTICE_TIME: Duration = Duration::from_secs(3);
/// The warnings for an ore below 100 percent, with Use off, by the ore.
const WARN_988: [u32; 12] = [984, 985, 1010, 1011, 7619, 7620, 1_000_335, 1_000_336, 1_000_368, 1_000_369, 1_000_370, 1_000_371];
const WARN_989: [u32; 12] = [6225, 6226, 6240, 6241, 6327, 6906, 6910, 6911, 1_000_331, 1_000_332, 1_000_333, 1_000_334];
const WARN_990: [u32; 2] = [6223, 6224];

/// How many 100 ms frames have passed since `since`.
fn frames(since: Instant, now: Instant) -> usize {
    (now.saturating_duration_since(since).as_millis() / FRAME_MS) as usize
}

/// The message a below-100% ore warns with (Use off), if its kind does.
pub fn ore_warning(ore: u32) -> Option<usize> {
    if WARN_988.contains(&ore) {
        Some(2988)
    } else if WARN_989.contains(&ore) {
        Some(2989)
    } else if WARN_990.contains(&ore) {
        Some(2990)
    } else {
        None
    }
}

pub struct Assets {
    /// The empty machine, looping; an item in (with blessings, without).
    wait: [Texture; 4],
    ready_a: [Texture; 3],
    ready_b: [Texture; 3],
    fill: Texture,
    /// The picked ore's frame by slot, and the blessing's.
    blue: [Texture; 4],
    red: Texture,
    /// Refine: normal, pressed, disabled.
    refine: [Texture; 3],
    notice: Texture,
    grades: [Texture; 4],
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |name: &str| gpu.load_texture(grf, &format!("{UI}refining_renewal\\{name}.bmp"), false);
        Self {
            wait: [0, 1, 2, 3].map(|frame| image(&format!("bg_refining_wait_0{frame}"))),
            ready_a: [0, 1, 2].map(|frame| image(&format!("bg_refininga_ready_0{frame}"))),
            ready_b: [0, 1, 2].map(|frame| image(&format!("bg_refiningb_ready_0{frame}"))),
            fill: gpu.solid([0xC2, 0xC2, 0xC2, 255]),
            blue: [0, 1, 2, 3].map(|slot| image(&format!("slot_select_blue{slot}"))),
            red: image("slot_select_red"),
            refine: ["normal", "press", "disable"].map(|look| image(&format!("bt_refining_{look}"))),
            notice: image("ico_notice"),
            grades: [1, 2, 3, 4].map(|grade| gpu.load_texture(grf, &format!("{UI}grade_enchant\\grade_icon{grade}.bmp"), false)),
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(TitleHit),
    Ore(usize),
    Blessing,
    /// The picked ore's and the blessing's icons on the anvil.
    AnvilOre,
    AnvilBlessing,
    Item,
    Refine,
    Cancel,
    Window,
}

#[derive(Debug, PartialEq)]
pub enum Action {
    /// Refine the item with this ore, with or without blessings.
    Refine {
        index: u16,
        ore: u32,
        blessings: bool,
    },
    Close,
}

/// A line in the notice bar: a message, red for a refusal or warning, and when a timed one goes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Notice {
    pub message: usize,
    pub red: bool,
    pub until: Option<Instant>,
}

pub struct Refine {
    pub frame: Frame,
    /// The item in the machine (inventory index), once the server lists its ores.
    pub item: Option<u16>,
    /// Ores: item ID, chance (percent), zeny.
    pub materials: Vec<(u32, u8, u32)>,
    /// How many Blacksmith's Blessings protect this attempt (0: none can).
    pub blessings: u8,
    pub use_blessings: bool,
    pub picked: Option<usize>,
    pub notice: Option<Notice>,
    /// When the window opened and when the item went in (the machine's picture frames).
    opened: Instant,
    loaded: Option<Instant>,
    /// A refine has been sent and the server has not answered.
    waiting: bool,
}

impl Refine {
    /// The window, centred on a screen `screen` big.
    pub fn new(screen: [f32; 2], now: Instant) -> Self {
        let mut frame = Frame::new([((screen[0] - SIZE[0]) / 2.0).floor(), ((screen[1] - SIZE[1]) / 2.0).floor()], SIZE[0]);
        frame.open = true;
        Self {
            frame,
            item: None,
            materials: Vec::new(),
            blessings: 0,
            use_blessings: false,
            picked: None,
            notice: None,
            opened: now,
            loaded: None,
            waiting: false,
        }
    }

    /// Where the inventory goes beside it: right of the window, its bottom on the window's.
    pub fn inventory_place(&self, inventory_height: f32) -> [f32; 2] {
        [self.frame.position[0] + SIZE[0], self.frame.position[1] + SIZE[1] - inventory_height]
    }

    /// The notice `message` (red or blue), standing or for 3 seconds from `now`.
    pub fn say(&mut self, message: usize, red: bool, timed: Option<Instant>) {
        self.notice = Some(Notice { message, red, until: timed.map(|now| now + NOTICE_TIME) });
    }

    /// The notice shown at `now`, a timed one gone when its time is.
    pub fn notice_at(&self, now: Instant) -> Option<Notice> {
        self.notice.filter(|notice| notice.until.is_none_or(|until| now < until))
    }

    /// The server's list for an item: its ores, and how many blessings it takes. No ore is picked, Use is off and the notice goes.
    pub fn listed(&mut self, index: u16, blessings: u8, materials: Vec<(u32, u8, u32)>, now: Instant) {
        if self.item != Some(index) {
            self.loaded = Some(now);
        }
        (self.item, self.blessings, self.materials) = (Some(index), blessings, materials);
        (self.picked, self.use_blessings, self.notice, self.waiting) = (None, false, None, false);
    }

    /// The server answered a refine.
    pub fn answered(&mut self) {
        self.waiting = false;
    }

    /// The machine empties.
    pub fn clear(&mut self) {
        (self.item, self.picked, self.use_blessings, self.loaded, self.waiting) = (None, None, false, None, false);
        self.materials.clear();
    }

    /// An item dropped on the item slot: refused with the timed notice when it is not identified. True when it may go in.
    pub fn put(&mut self, identified: bool, now: Instant) -> bool {
        if !identified {
            self.say(CANT_REFINE, true, Some(now));
        }
        identified
    }

    /// The ore slot i shows, or None past the list or the four slots.
    fn ore_slot(&self, i: usize) -> Option<(u32, u8, u32)> {
        self.materials.get(i).filter(|_| i < SLOTS).copied()
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let local = self.frame.local(cursor, SIZE[1])?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(Hit::Title(hit));
        }
        let loaded = self.item.is_some();
        let slot = (0..SLOTS.min(self.materials.len())).find(|&i| contains([11.0 + SLOT_STEP * i as f32, SLOT_Y], SLOT_SIZE, local));
        Some(if let Some(slot) = slot.filter(|_| loaded) {
            Hit::Ore(slot)
        } else if loaded && self.blessings > 0 && contains([BLESSING_X, SLOT_Y], SLOT_SIZE, local) {
            Hit::Blessing
        } else if loaded && self.picked.is_some() && contains(ANVIL_ORE, ICON, local) {
            Hit::AnvilOre
        } else if loaded && self.use_blessings && contains(ANVIL_BLESSING, ICON, local) {
            Hit::AnvilBlessing
        } else if contains(ITEM_SLOT, ITEM_SLOT_SIZE, local) {
            Hit::Item
        } else if contains(REFINE, BUTTON, local) {
            Hit::Refine
        } else if contains(CANCEL, CANCEL_SIZE, local) {
            Hit::Cancel
        } else {
            Hit::Window
        })
    }

    /// The item an icon on the window stands for: the ore of a slot or the anvil, the blessing, the item in the machine.
    pub fn item_at(&self, hit: Hit, carried: &[InventoryItem]) -> Option<ItemAt> {
        match hit {
            Hit::Ore(i) => self.ore_slot(i).map(|(ore, ..)| ItemAt::Plain(ore)),
            Hit::AnvilOre => self.picked.and_then(|row| self.ore_slot(row)).map(|(ore, ..)| ItemAt::Plain(ore)),
            Hit::Blessing | Hit::AnvilBlessing => Some(ItemAt::Plain(BLESSING_ITEM)),
            Hit::Item => self.item.and_then(|index| carried.iter().find(|item| item.index == index)).map(|item| ItemAt::Carried(item.index)),
            _ => None,
        }
    }

    /// Reacts to a click; `count` tells how many of an item are carried, `zeny` the zeny carried, `now` the time.
    pub fn click(&mut self, hit: Hit, count: impl Fn(u32) -> u32, zeny: u32, now: Instant) -> Option<Action> {
        match hit {
            Hit::Title(TitleHit::Close) | Hit::Cancel => return Some(Action::Close),
            Hit::Title(title) => self.frame.click_title(title),
            // Ores not carried can't be picked; one below 100 percent with Use off warns by its kind.
            Hit::Ore(i) => {
                let (ore, chance, _) = self.ore_slot(i)?;
                if count(ore) == 0 {
                    return None;
                }
                self.picked = Some(i);
                if chance < 100
                    && !self.use_blessings
                    && let Some(message) = ore_warning(ore)
                {
                    self.say(message, true, None);
                }
            }
            Hit::Blessing => {
                if count(BLESSING_ITEM) < u32::from(self.blessings) {
                    self.say(NO_BLESSINGS, true, Some(now));
                } else {
                    self.use_blessings = !self.use_blessings;
                    if self.use_blessings {
                        self.say(USE_ON, false, None);
                    } else {
                        self.notice = None;
                    }
                }
            }
            Hit::Refine => {
                let (index, (ore, _, price)) = (self.item?, self.ore_slot(self.picked?)?);
                // The ore picked may be used up (a refine leaves it picked); one answer is waited for.
                if count(ore) == 0 || self.waiting {
                    return None;
                }
                if zeny < price {
                    self.say(NO_ZENY, true, Some(now));
                    return None;
                }
                self.waiting = true;
                return Some(Action::Refine { index, ore, blessings: self.use_blessings });
            }
            Hit::AnvilOre | Hit::AnvilBlessing | Hit::Item | Hit::Window => {}
        }
        None
    }

    pub fn icons(&self, items: &HashMap<u32, ItemInfo>, carried: &[InventoryItem]) -> Vec<String> {
        let item = self.item.and_then(|index| carried.iter().find(|item| item.index == index)).map(|item| item.id);
        item.into_iter()
            .chain(self.materials.iter().take(SLOTS).map(|&(ore, ..)| ore))
            .chain((self.blessings > 0).then_some(BLESSING_ITEM))
            .filter_map(|id| items.get(&id).map(|info| inventory::icon_path(info, true)))
            .collect()
    }

    /// The machine's picture at `now`: waiting frames looping, or the ready ones once and then held.
    fn machine<'a>(&self, assets: &'a Assets, item: Option<&InventoryItem>, now: Instant) -> &'a Texture {
        match (item, self.loaded) {
            (Some(_), Some(since)) => {
                let ready = if self.blessings > 0 { &assets.ready_a } else { &assets.ready_b };
                &ready[frames(since, now).min(ready.len() - 1)]
            }
            _ => &assets.wait[frames(self.opened, now) % assets.wait.len()],
        }
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
        carried: &[InventoryItem],
        messages: &[String],
        names: (&CardNames, &HashMap<u32, Option<String>>),
        font: &Font,
        pointer: ([f32; 2], bool),
        now: Instant,
        zeny: u32,
    ) {
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        let at = |local: [f32; 2]| self.frame.at(local);
        self.frame.draw_title(layer, chrome, message(TITLE), pointer.0);
        if self.frame.minimized {
            return;
        }
        let icon = |id: u32| items.get(&id).and_then(|info| icons.get(&inventory::icon_path(info, true))).and_then(Option::as_ref);
        let count = |id: u32| carried.iter().filter(|item| item.id == id).map(|item| u32::from(item.amount)).sum::<u32>();
        let item = self.item.and_then(|index| carried.iter().find(|item| item.index == index));
        let centered = |text: &str, left: f32, width: f32, top: f32, color: [u8; 3], layer: &mut Layer<'a>| {
            layer.texts.push((text.to_owned(), at([left + ((width - font.width(text)) / 2.0).floor(), top]), color));
        };
        // The machine: a grey fill and its picture.
        layer.sprites.push((&assets.fill, at(FILL_AT), FILL_SIZE));
        let machine = self.machine(assets, item, now);
        layer.sprites.push((machine, at(MACHINE_AT), machine.size));
        let picked = self.picked.and_then(|row| self.ore_slot(row));
        // The chance of the picked ore, in the rate line.
        let rate = [message(SUCCESS_LABEL).to_owned(), format!(" {}%", picked.filter(|_| item.is_some()).map_or(0, |(_, chance, _)| chance))];
        let width = font.width(&rate[0]) + font.width(&rate[1]);
        let left = at([((SIZE[0] - width) / 2.0).floor(), RATE_Y]);
        layer.texts.push((rate[0].clone(), left, SUCCESS_COLOR));
        layer.texts.push((rate[1].clone(), [left[0] + font.width(&rate[0]), left[1]], CHANCE_COLOR));
        if let Some(item) = item {
            // The ores across the top, the picked one framed, "carried/1" under each (red when none).
            for i in 0..SLOTS.min(self.materials.len()) {
                let (ore, ..) = self.materials[i];
                let x = SLOT_STEP * i as f32;
                if self.picked == Some(i) {
                    layer.sprites.push((&assets.blue[i], at([11.0 + x, SLOT_Y]), assets.blue[i].size));
                }
                if let Some(texture) = icon(ore) {
                    layer.sprites.push((texture, at([22.0 + x, ICON_Y]), texture.size));
                }
                let carried_ore = count(ore);
                centered(&format!("{carried_ore}/1"), 10.0 + x, 48.0, COUNT_Y, if carried_ore == 0 { RED } else { [0, 0, 0] }, layer);
            }
            if self.blessings > 0 {
                if self.use_blessings {
                    layer.sprites.push((&assets.red, at([BLESSING_X, SLOT_Y]), assets.red.size));
                }
                if let Some(texture) = icon(BLESSING_ITEM) {
                    layer.sprites.push((texture, at([218.0, ICON_Y]), texture.size));
                }
                let have = count(BLESSING_ITEM);
                centered(&format!("{have}/{}", self.blessings), 206.0, 48.0, COUNT_Y, if have < u32::from(self.blessings) { RED } else { [0, 0, 0] }, layer);
            }
            // The anvil: the picked ore, and the blessing when Use is on.
            if let Some(texture) = picked.and_then(|(ore, ..)| icon(ore)) {
                layer.sprites.push((texture, at(ANVIL_ORE), texture.size));
            }
            if self.use_blessings
                && let Some(texture) = icon(BLESSING_ITEM)
            {
                layer.sprites.push((texture, at(ANVIL_BLESSING), texture.size));
            }
            // The item: its icon and grade in the slot, its name under it.
            if let Some(texture) = icon(item.id) {
                layer.sprites.push((texture, at(ITEM_ICON), texture.size));
            }
            if (1..=4).contains(&item.grade) {
                layer.sprites.push((&assets.grades[usize::from(item.grade - 1)], at(GRADE_AT), assets.grades[0].size));
            }
            let name = window::fit(font, &inventory::full_name(items, names.0, messages, names.1, item, true), SIZE[0] - 8.0);
            centered(&name, 0.0, SIZE[0], NAME_Y, NAME_COLOR, layer);
        }
        // Refine: normal, pressed while held, disabled until an item is in and an ore picked; the zeny at its left.
        let ready = item.is_some() && picked.is_some();
        let pressed = ready && pointer.1 && self.hit(pointer.0) == Some(Hit::Refine);
        let button = &assets.refine[if !ready { 2 } else { usize::from(pressed) }];
        layer.sprites.push((button, at(REFINE), BUTTON));
        let down = if pressed { 1.0 } else { 0.0 };
        centered(message(TITLE), REFINE[0], BUTTON[0], REFINE[1] + 11.0 + down, if ready { LABEL_COLOR } else { LABEL_OFF_COLOR }, layer);
        if let Some((_, _, price)) = picked.filter(|_| ready) {
            let text = group(u64::from(price));
            layer.texts.push((text.clone(), at([REFINE[0] + 83.0 - font.width(&text), REFINE[1] + 28.0 + down]), [255; 3]));
        }
        // The notice bar and Cancel.
        if let Some(notice) = self.notice_at(now) {
            layer.sprites.push((&assets.notice, at(offset(NOTICE, [0.0, 3.0])), assets.notice.size));
            let text = window::fit(font, message(notice.message), NOTICE_WIDTH);
            layer.texts.push((text, at(offset(NOTICE, [18.0, 3.0])), if notice.red { RED } else { BLUE }));
        }
        let hovered = self.hit(pointer.0) == Some(Hit::Cancel);
        layer.sprites.push((chrome.button(0, hovered), at(CANCEL), CANCEL_SIZE));
        centered(message(CANCEL_LABEL), CANCEL[0], CANCEL_SIZE[0], CANCEL[1] + 3.0, [0, 0, 0], layer);
        // The name over an ore, the blessing and the anvil's icons.
        if let Some(hit) = self.hit(pointer.0)
            && matches!(hit, Hit::Ore(_) | Hit::Blessing | Hit::AnvilOre | Hit::AnvilBlessing)
            && let Some(ItemAt::Plain(id)) = self.item_at(hit, carried)
        {
            window::tooltip(tooltip, chrome, font, &inventory::item_name(items, id, true, 0), offset(pointer.0, [12.0, 12.0]), [255; 3]);
        }
        let _ = zeny;
    }
}

/// The item an icon on the window stands for.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum ItemAt {
    /// An item by its number (an ore, the blessing).
    Plain(u32),
    /// The item in the machine, by its inventory index.
    Carried(u16),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(window: &Refine, local: [f32; 2]) -> Option<Hit> {
        window.hit([window.frame.position[0] + local[0], window.frame.position[1] + local[1]])
    }

    #[test]
    fn opens_centred_with_the_inventory_beside_it() {
        let window = Refine::new([1024.0, 768.0], Instant::now());
        assert_eq!(window.frame.position, [380.0, 211.0]);
        // Right of the window, its bottom on the window's bottom: an inventory 300 tall.
        assert_eq!(window.inventory_place(300.0), [380.0 + 264.0, 211.0 + 346.0 - 300.0]);
    }

    #[test]
    fn the_slots_and_buttons_are_hit_at_their_edges() {
        let now = Instant::now();
        let mut window = Refine::new([1024.0, 768.0], now);
        // No item: the slots are not there, the item slot, Refine and Cancel are.
        assert_eq!(at(&window, [12.0, 60.0]), Some(Hit::Window));
        window.listed(4, 1, vec![(1010, 100, 50), (6240, 60, 5000)], now);
        assert_eq!(
            [at(&window, [11.0, 55.0]), at(&window, [56.0, 106.0]), at(&window, [57.0, 106.0]), at(&window, [60.0, 55.0])],
            [Some(Hit::Ore(0)), Some(Hit::Ore(0)), Some(Hit::Window), Some(Hit::Ore(1))]
        );
        // Only two ores listed: no third slot; the blessing slot (x 207) is there with a blessing count.
        assert_eq!(at(&window, [11.0 + 98.0, 60.0]), Some(Hit::Window));
        assert_eq!(
            [at(&window, [207.0, 55.0]), at(&window, [252.0, 106.0]), at(&window, [253.0, 106.0])],
            [Some(Hit::Blessing), Some(Hit::Blessing), Some(Hit::Window)]
        );
        // The item slot x 109 to 155, y 201 to 247; Refine (84, 262) 96×48; Cancel (204, 322) 56×20.
        assert_eq!(
            [at(&window, [109.0, 201.0]), at(&window, [154.0, 246.0]), at(&window, [155.0, 246.0])],
            [Some(Hit::Item), Some(Hit::Item), Some(Hit::Window)]
        );
        assert_eq!(
            [at(&window, [84.0, 262.0]), at(&window, [179.0, 309.0]), at(&window, [180.0, 309.0])],
            [Some(Hit::Refine), Some(Hit::Refine), Some(Hit::Window)]
        );
        assert_eq!(
            [at(&window, [204.0, 322.0]), at(&window, [259.0, 341.0]), at(&window, [260.0, 341.0])],
            [Some(Hit::Cancel), Some(Hit::Cancel), Some(Hit::Window)]
        );
        // The anvil's icons: the picked ore always, the blessing with Use on.
        assert_eq!(at(&window, [81.0, 150.0]), Some(Hit::Window));
        window.picked = Some(0);
        window.use_blessings = true;
        assert_eq!(
            [at(&window, [81.0, 150.0]), at(&window, [161.0, 150.0]), at(&window, [185.0, 150.0])],
            [Some(Hit::AnvilOre), Some(Hit::AnvilBlessing), Some(Hit::Window)]
        );
    }

    #[test]
    fn picking_an_ore_warns_by_its_kind_when_below_100_percent() {
        let now = Instant::now();
        let mut window = Refine::new([1024.0, 768.0], now);
        window.listed(4, 0, vec![(1010, 100, 50), (6240, 60, 5000), (984, 60, 100), (6223, 40, 900), (7000, 50, 100)], now);
        let carried = |id: u32| if id == 7000 { 0 } else { 1 };
        // 100 percent: no warning; 6240 warns with 2989, 984 with 2988, 6223 with 2990 (standing, red).
        window.click(Hit::Ore(0), carried, 1000, now);
        assert_eq!((window.picked, window.notice), (Some(0), None));
        window.click(Hit::Ore(1), carried, 1000, now);
        assert_eq!(window.notice, Some(Notice { message: 2989, red: true, until: None }));
        window.click(Hit::Ore(2), carried, 1000, now);
        assert_eq!(window.notice.unwrap().message, 2988);
        window.click(Hit::Ore(3), carried, 1000, now);
        assert_eq!(window.notice.unwrap().message, 2990);
        // The fourth slot is the fifth ore of the list: there are only four slots, so it can't be picked; the notice stays.
        window.click(Hit::Ore(0), carried, 1000, now);
        assert_eq!(window.notice.unwrap().message, 2990, "an ore of no kind leaves the notice as it was");
        assert!(ore_warning(1_000_371) == Some(2988) && ore_warning(1_000_331) == Some(2989) && ore_warning(7000).is_none());
        // With Use on there is no warning; an ore not carried is not picked.
        window.use_blessings = true;
        window.click(Hit::Ore(1), carried, 1000, now);
        assert_eq!(window.notice.unwrap().message, 2990);
        let nothing = |_: u32| 0;
        window.picked = None;
        window.click(Hit::Ore(1), nothing, 1000, now);
        assert_eq!(window.picked, None);
    }

    #[test]
    fn the_blessing_toggles_with_enough_and_refuses_with_too_few() {
        let now = Instant::now();
        let mut window = Refine::new([1024.0, 768.0], now);
        window.listed(4, 2, vec![(1010, 100, 50)], now);
        // One carried of the two needed: a timed notice 2969 for 3 s, Use stays off.
        window.click(Hit::Blessing, |_| 1, 0, now);
        assert!(!window.use_blessings);
        assert_eq!(window.notice, Some(Notice { message: 2969, red: true, until: Some(now + Duration::from_secs(3)) }));
        assert!(window.notice_at(now + Duration::from_millis(2999)).is_some() && window.notice_at(now + Duration::from_secs(3)).is_none());
        // Two carried: on with a standing blue 2967, then off and the notice cleared.
        window.click(Hit::Blessing, |_| 2, 0, now);
        assert!(window.use_blessings);
        assert_eq!(window.notice, Some(Notice { message: 2967, red: false, until: None }));
        window.click(Hit::Blessing, |_| 2, 0, now);
        assert!(!window.use_blessings && window.notice.is_none());
    }

    #[test]
    fn refines_with_a_carried_ore_and_enough_zeny_and_waits_for_the_answer() {
        let now = Instant::now();
        let mut window = Refine::new([1024.0, 768.0], now);
        window.listed(4, 0, vec![(1010, 100, 50), (6240, 100, 5000)], now);
        let carried = |id: u32| if id == 1010 { 1 } else { 0 };
        // Nothing picked: Refine does nothing; an ore not carried can't be picked.
        assert_eq!(window.click(Hit::Refine, carried, 1000, now), None);
        window.click(Hit::Ore(1), carried, 1000, now);
        assert_eq!(window.picked, None);
        // Too little zeny: a timed notice 2968, nothing sent.
        window.click(Hit::Ore(0), carried, 1000, now);
        assert_eq!(window.click(Hit::Refine, carried, 10, now), None);
        assert_eq!(window.notice, Some(Notice { message: 2968, red: true, until: Some(now + Duration::from_secs(3)) }));
        // Enough: sent once, then nothing more until the server answers.
        assert_eq!(window.click(Hit::Refine, carried, 1000, now), Some(Action::Refine { index: 4, ore: 1010, blessings: false }));
        assert_eq!(window.click(Hit::Refine, carried, 1000, now), None);
        window.answered();
        assert!(window.click(Hit::Refine, carried, 1000, now).is_some());
        // Each list resets the pick, Use and the notice; Cancel and the title's close both close.
        window.listed(4, 1, vec![(1010, 90, 50)], now);
        assert_eq!((window.picked, window.use_blessings, window.notice), (None, false, None));
        assert_eq!(window.click(Hit::Cancel, carried, 0, now), Some(Action::Close));
        assert_eq!(window.click(Hit::Title(TitleHit::Close), carried, 0, now), Some(Action::Close));
    }

    #[test]
    fn an_unidentified_item_is_refused_with_a_timed_notice() {
        let now = Instant::now();
        let mut window = Refine::new([1024.0, 768.0], now);
        assert!(!window.put(false, now));
        assert_eq!(window.notice, Some(Notice { message: CANT_REFINE, red: true, until: Some(now + Duration::from_secs(3)) }));
        assert!(window.put(true, now));
    }

    #[test]
    fn the_machine_waits_in_four_frames_then_holds_the_ready_one() {
        let start = Instant::now();
        let mut window = Refine::new([1024.0, 768.0], start);
        assert_eq!([0, 99, 100, 399, 400].map(|ms| frames(start, start + Duration::from_millis(ms)) % 4), [0, 0, 1, 3, 0]);
        // The blessing count above 0 picks the a pictures, else the b ones; the item going in starts the ready frames.
        window.listed(4, 1, vec![(1010, 100, 50)], start);
        assert!(window.blessings > 0 && window.loaded == Some(start));
        window.listed(4, 0, vec![(1010, 100, 50)], start + Duration::from_secs(1));
        assert!(window.blessings == 0 && window.loaded == Some(start), "the same item again does not start the picture again");
        window.clear();
        assert!(window.loaded.is_none() && window.item.is_none());
    }
}
