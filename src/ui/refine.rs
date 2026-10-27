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
/// The hammering lasts 14 frames; the sounds at frames 7 and 12; the pictures of the result.
const PROCESS_FRAMES: usize = 14;
const SOUND_FRAMES: [usize; 2] = [7, 12];
const HAMMER_SOUNDS: [&str; 2] = ["effect\\refine\\bs_refine_1.wav", "effect\\refine\\bs_refine_2.wav"];
const SUCCESS_SOUND: &str = "effect\\refine\\bs_refine_success.wav";
const FAILED_SOUND: &str = "effect\\refine\\bs_refine_failed.wav";
/// The result pictures: 17 success frames looping 13 to 16, 20 fail frames looping 18 and 19.
const SUCCESS_FRAMES: usize = 17;
const FAIL_FRAMES: usize = 20;
/// Back and Again with their places and sizes.
const BACK: [f32; 2] = [20.0, 267.0];
const BACK_SIZE: [f32; 2] = [101.0, 38.0];
const AGAIN: [f32; 2] = [142.0, 267.0];
const AGAIN_SIZE: [f32; 2] = [101.0, 50.0];
const RESULT_NAME_Y: f32 = 249.0;
const BACK_LABEL: usize = 2973;
const AGAIN_LABEL: usize = 3241;
const AGAIN_RATE: usize = 3285;
const BROKEN_LABEL: usize = 3246;
const DONE_LABEL: usize = 2971;
const NOT_DONE_LABEL: usize = 2972;
/// The notices at the end of the hammering (5 s): nothing to pick, no ore carried, too little zeny, too few blessings.
const NOTE_GONE: usize = 3242;
const NOTE_NO_ORE: usize = 3243;
const NOTE_NO_ZENY: usize = 3244;
const NOTE_FEW_BLESSINGS: usize = 3245;
const END_NOTICE: Duration = Duration::from_secs(5);
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

/// The chat line for each result: success, broken, lowered, failed.
const RESULT_CHAT: [usize; 4] = [498, 499, 1537, 499];

/// The picture of the result at its `n`th frame: success 17 frames then 13 to 16 looping, the others 20 then 18 and 19.
pub fn result_frame(success: bool, n: usize) -> usize {
    match (success, n) {
        (true, 0..=16) => n,
        (true, _) => 13 + (n - 13) % 4,
        (false, 0..=19) => n,
        (false, _) => 18 + (n - 18) % 2,
    }
}

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
    /// The hammering with Use on, without, and its last frames; the success and fail pictures.
    process_a: [Texture; 9],
    process_b: [Texture; 9],
    process_c: [Texture; 5],
    success: Vec<Texture>,
    fail: Vec<Texture>,
    /// Back and Again, success then fail: normal, pressed (Again: and disabled).
    back: [[Texture; 2]; 2],
    again: [[Texture; 3]; 2],
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
            process_a: std::array::from_fn(|frame| image(&format!("bg_refininga_process_{frame:02}"))),
            process_b: std::array::from_fn(|frame| image(&format!("bg_refiningb_process_{frame:02}"))),
            // The hammering's last five frames are numbered 9 to 13.
            process_c: std::array::from_fn(|frame| image(&format!("bg_refining_process_{:02}", frame + 9))),
            success: (0..SUCCESS_FRAMES).map(|frame| image(&format!("bg_refining_success_{frame:02}"))).collect(),
            fail: (0..FAIL_FRAMES).map(|frame| image(&format!("bg_refining_fail_{frame:02}"))).collect(),
            back: ["success", "fail"].map(|kind| ["normal", "press"].map(|look| image(&format!("bt_refining_return_{kind}_{look}")))),
            again: ["success", "fail"].map(|kind| ["normal", "press", "disable"].map(|look| image(&format!("bt_refining_again_{kind}_{look}")))),
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
    Back,
    Again,
    Window,
}

/// What the window asks its owner to do as time goes: a sound to play, a chat line (a message number, in #00FFFF).
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Cue {
    Sound(&'static str),
    Chat(usize),
}

/// A list the server sent: the item, its blessing count and its ores (item, chance, zeny).
type Listed = (u16, u8, Vec<(u32, u8, u32)>);

/// The hammering under way.
struct Process {
    start: Instant,
    result: u16,
    /// Use was on when it began (the picture set, the blessing's icon).
    blessing: bool,
    /// The sounds already made, by frame; and the list the server sent meanwhile, applied at the end.
    heard: [bool; 2],
    list: Option<Listed>,
}

/// The result on show after the hammering: until Back or Again.
struct Shown {
    result: u16,
    since: Instant,
    /// Again may be pressed.
    again: bool,
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
    process: Option<Process>,
    shown: Option<Shown>,
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
            process: None,
            shown: None,
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
        // The list that follows a result is kept until the hammering ends.
        if let Some(process) = &mut self.process {
            process.list = Some((index, blessings, materials));
            return;
        }
        if self.item != Some(index) {
            self.loaded = Some(now);
        }
        (self.item, self.blessings, self.materials) = (Some(index), blessings, materials);
        (self.picked, self.use_blessings, self.notice, self.waiting) = (None, false, None, false);
    }

    /// The server's result (0 success, 1 broken, 2 lowered, 3 failed): the hammering starts, with the item in.
    pub fn refined(&mut self, result: u16, now: Instant) {
        self.waiting = false;
        if self.item.is_some() {
            self.process = Some(Process { start: now, result, blessing: self.use_blessings, heard: [false; 2], list: None });
            self.shown = None;
        }
    }

    /// Runs the hammering to `now`: its two sounds at frames 7 and 12 and, at its end, the result's sound, the new list's checks (with
    /// `count` carried of an item and `zeny`), the notice and the chat line.
    pub fn advance(&mut self, now: Instant, count: impl Fn(u32) -> u32, zeny: u32) -> Vec<Cue> {
        let mut cues = Vec::new();
        let Some(process) = &mut self.process else { return cues };
        let frame = frames(process.start, now);
        for (k, heard) in process.heard.iter_mut().enumerate() {
            if frame >= SOUND_FRAMES[k] && !*heard {
                *heard = true;
                cues.push(Cue::Sound(HAMMER_SOUNDS[k]));
            }
        }
        if frame < PROCESS_FRAMES {
            return cues;
        }
        let Some(process) = self.process.take() else { return cues };
        let result = process.result;
        cues.push(Cue::Sound(if result == 0 { SUCCESS_SOUND } else { FAILED_SOUND }));
        // The ore that was picked, looked up by its number in the new list.
        let ore = self.picked.and_then(|row| self.ore_slot(row)).map(|(ore, ..)| ore);
        if let Some((index, blessings, materials)) = process.list {
            (self.item, self.blessings, self.materials) = (Some(index), blessings, materials);
        }
        let timed = |message: usize| Notice { message, red: true, until: Some(now + END_NOTICE) };
        let mut again = result != 1;
        let mut note = None;
        // The ore: a list with none, one that lost it, or one none of which is carried.
        self.picked = None;
        let ore_note = if self.materials.is_empty() {
            again = false;
            Some(CANT_REFINE)
        } else if let Some((row, ore)) = ore.and_then(|ore| self.materials.iter().position(|&(listed, ..)| listed == ore).map(|row| (row, ore))) {
            self.picked = Some(row);
            (count(ore) == 0).then(|| {
                again = false;
                NOTE_NO_ORE
            })
        } else {
            again = false;
            Some(NOTE_GONE)
        };
        // Again costs the picked ore's zeny.
        if let Some((_, _, price)) = self.picked.and_then(|row| self.ore_slot(row))
            && zeny < price
        {
            again = false;
            note = Some(timed(NOTE_NO_ZENY));
        }
        // Use stays on only with a blessing count in the new list and enough carried.
        if process.blessing {
            if self.blessings == 0 {
                self.use_blessings = false;
                note = Some(timed(NOTE_GONE));
            } else {
                self.use_blessings = true;
                note = Some(Notice { message: USE_ON, red: false, until: None });
                if count(BLESSING_ITEM) < u32::from(self.blessings) {
                    self.use_blessings = false;
                    again = false;
                    note = Some(timed(NOTE_FEW_BLESSINGS));
                }
            }
        }
        self.notice = ore_note.map(timed).or(note);
        self.shown = Some(Shown { result, since: process.start + std::time::Duration::from_millis((PROCESS_FRAMES as u64) * FRAME_MS as u64), again });
        cues.push(Cue::Chat(RESULT_CHAT[usize::from(result).min(3)]));
        cues
    }

    /// The machine empties.
    pub fn clear(&mut self) {
        (self.item, self.picked, self.use_blessings, self.loaded, self.waiting) = (None, None, false, None, false);
        (self.process, self.shown) = (None, None);
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
        // The hammering takes everything; the result shows Back and Again beside the item.
        if self.process.is_some() {
            return Some(Hit::Window);
        }
        if let Some(shown) = &self.shown {
            return Some(if contains(BACK, BACK_SIZE, local) {
                Hit::Back
            } else if contains(AGAIN, AGAIN_SIZE, local) && shown.again {
                Hit::Again
            } else if contains(ITEM_SLOT, ITEM_SLOT_SIZE, local) {
                Hit::Item
            } else if contains(CANCEL, CANCEL_SIZE, local) {
                Hit::Cancel
            } else {
                Hit::Window
            });
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
        // While it hammers, Cancel and the close button do nothing.
        if self.process.is_some() && matches!(hit, Hit::Title(TitleHit::Close) | Hit::Cancel) {
            return None;
        }
        match hit {
            Hit::Title(TitleHit::Close) | Hit::Cancel => return Some(Action::Close),
            // Back: the notice goes, nothing is picked, Use is off; a broken item empties the machine, else the ready picture returns.
            Hit::Back => {
                let broken = self.shown.take().is_some_and(|shown| shown.result == 1);
                (self.notice, self.picked, self.use_blessings) = (None, None, false);
                if broken {
                    self.clear();
                } else {
                    self.loaded = Some(now);
                }
            }
            // Again: the same item, ore and Use; the next hammering starts when the server answers.
            Hit::Again => {
                let (index, (ore, ..)) = (self.item?, self.ore_slot(self.picked?)?);
                if self.waiting || !self.shown.as_ref().is_some_and(|shown| shown.again) {
                    return None;
                }
                self.shown = None;
                self.loaded = Some(now);
                self.waiting = true;
                return Some(Action::Refine { index, ore, blessings: self.use_blessings });
            }
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
        // The machine: a grey fill and its picture (the hammering, the result, or waiting with or without an item).
        let hammer = self.process.as_ref().map(|process| (process, frames(process.start, now).min(PROCESS_FRAMES - 1)));
        let shown = self.shown.as_ref();
        layer.sprites.push((&assets.fill, at(FILL_AT), FILL_SIZE));
        let machine = match (hammer, shown) {
            (Some((process, frame)), _) if frame < 9 => {
                let set = if process.blessing { &assets.process_a } else { &assets.process_b };
                &set[frame]
            }
            (Some((_, frame)), _) => &assets.process_c[frame - 9],
            (None, Some(shown)) => {
                let n = result_frame(shown.result == 0, frames(shown.since, now));
                if shown.result == 0 { &assets.success[n] } else { &assets.fail[n] }
            }
            (None, None) => self.machine(assets, item, now),
        };
        layer.sprites.push((machine, at(MACHINE_AT), machine.size));
        let picked = self.picked.and_then(|row| self.ore_slot(row));
        // The top line: the chance of the picked ore (up to frame 3 of the hammering), or the result.
        let slots = hammer.is_none() && shown.is_none();
        if let Some(shown) = shown {
            let line = message(if shown.result == 0 { DONE_LABEL } else { NOT_DONE_LABEL });
            centered(line, 0.0, SIZE[0], RATE_Y, SUCCESS_COLOR, layer);
        } else if hammer.is_none_or(|(_, frame)| frame <= 3) {
            let rate = [message(SUCCESS_LABEL).to_owned(), format!(" {}%", picked.filter(|_| item.is_some()).map_or(0, |(_, chance, _)| chance))];
            let width = font.width(&rate[0]) + font.width(&rate[1]);
            let left = at([((SIZE[0] - width) / 2.0).floor(), RATE_Y]);
            layer.texts.push((rate[0].clone(), left, SUCCESS_COLOR));
            layer.texts.push((rate[1].clone(), [left[0] + font.width(&rate[0]), left[1]], CHANCE_COLOR));
        }
        if item.is_some() && slots {
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
        }
        // The anvil: the picked ore (on the hammering's first frame only), and the blessing when Use is on (to frame 12).
        let ore_on = match hammer {
            Some((_, frame)) => frame == 0,
            None => shown.is_none(),
        };
        let blessing_on = match hammer {
            Some((process, frame)) => process.blessing && frame <= 12,
            None => shown.is_none() && self.use_blessings,
        };
        if item.is_some() || hammer.is_some() {
            if ore_on && let Some(texture) = picked.and_then(|(ore, ..)| icon(ore)) {
                layer.sprites.push((texture, at(ANVIL_ORE), texture.size));
            }
            if blessing_on && let Some(texture) = icon(BLESSING_ITEM) {
                layer.sprites.push((texture, at(ANVIL_BLESSING), texture.size));
            }
        }
        // The item: its icon and grade in the slot, its name under it (lower after a result; a broken one says so instead).
        if let Some(item) = item {
            if let Some(texture) = icon(item.id) {
                layer.sprites.push((texture, at(ITEM_ICON), texture.size));
            }
            if (1..=4).contains(&item.grade) {
                layer.sprites.push((&assets.grades[usize::from(item.grade - 1)], at(GRADE_AT), assets.grades[0].size));
            }
            if shown.is_none_or(|shown| shown.result != 1) {
                // The whole name without " [slots]", with its options' count.
                let name = inventory::option_text(messages, &inventory::full_name(items, names.0, messages, names.1, item, false), item);
                let name = window::fit(font, &name, SIZE[0] - 8.0);
                centered(&name, 0.0, SIZE[0], if shown.is_some() { RESULT_NAME_Y } else { NAME_Y }, NAME_COLOR, layer);
            }
        }
        if shown.is_some_and(|shown| shown.result == 1) {
            centered(message(BROKEN_LABEL), 0.0, SIZE[0], RESULT_NAME_Y, NAME_COLOR, layer);
        }
        if let Some(shown) = shown {
            // Back and Again, the success pictures after a success: the labels, the chance and the zeny on Again.
            let look = usize::from(shown.result != 0);
            let over = |place: [f32; 2], size: [f32; 2]| pointer.1 && contains(at(place), size, pointer.0);
            let (back_down, again_down) = (over(BACK, BACK_SIZE), shown.again && over(AGAIN, AGAIN_SIZE));
            let ink = if look == 0 { [0x1F, 0x1C, 0x0E] } else { LABEL_COLOR };
            layer.sprites.push((&assets.back[look][usize::from(back_down)], at(BACK), BACK_SIZE));
            centered(message(BACK_LABEL), BACK[0], BACK_SIZE[0], BACK[1] + if back_down { 15.0 } else { 14.0 }, ink, layer);
            let again = &assets.again[look][if !shown.again { 2 } else { usize::from(again_down) }];
            layer.sprites.push((again, at(AGAIN), AGAIN_SIZE));
            let down = if again_down { 1.0 } else { 0.0 };
            if shown.again {
                centered(message(AGAIN_LABEL), AGAIN[0], AGAIN_SIZE[0], AGAIN[1] + 9.0 + down, ink, layer);
                let chance = picked.map_or(0, |(_, chance, _)| chance);
                let rate = rustnarok_client::messages::fill(message(AGAIN_RATE), &[&chance]);
                centered(&rate, AGAIN[0], AGAIN_SIZE[0], AGAIN[1] + 22.0 + down, ink, layer);
                let price = group(u64::from(picked.map_or(0, |(_, _, price)| price)));
                centered(&price, AGAIN[0], AGAIN_SIZE[0], AGAIN[1] + 38.0 + down, [255; 3], layer);
            } else {
                centered(
                    message(AGAIN_LABEL),
                    AGAIN[0],
                    AGAIN_SIZE[0],
                    AGAIN[1] + 14.0,
                    if look == 0 { [0x1F, 0x1C, 0x0E] } else { [0x33, 0x43, 0x6F] },
                    layer,
                );
            }
        } else {
            // Refine: normal, pressed while held, disabled until an item is in and an ore picked (and while it hammers); the zeny at its left.
            let ready = item.is_some() && picked.is_some() && hammer.is_none();
            let pressed = ready && pointer.1 && self.hit(pointer.0) == Some(Hit::Refine);
            let button = &assets.refine[if !ready { 2 } else { usize::from(pressed) }];
            layer.sprites.push((button, at(REFINE), BUTTON));
            let down = if pressed { 1.0 } else { 0.0 };
            centered(message(TITLE), REFINE[0], BUTTON[0], REFINE[1] + 11.0 + down, if ready { LABEL_COLOR } else { LABEL_OFF_COLOR }, layer);
            if let Some((_, _, price)) = picked.filter(|_| ready) {
                let text = group(u64::from(price));
                layer.texts.push((text.clone(), at([REFINE[0] + 83.0 - font.width(&text), REFINE[1] + 28.0 + down]), [255; 3]));
            }
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
        window.waiting = false;
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

    /// A window with an item in (index 4), ore 1010 picked of a list of 1010 and 6240, hammering from `start`.
    fn hammering(result: u16, blessing: bool, start: Instant) -> Refine {
        let mut window = Refine::new([1024.0, 768.0], start);
        window.listed(4, if blessing { 1 } else { 0 }, vec![(1010, 100, 50), (6240, 100, 5000)], start);
        window.picked = Some(0);
        window.use_blessings = blessing;
        window.refined(result, start);
        window
    }

    #[test]
    fn the_result_pictures_loop_their_last_frames() {
        // Success: 17 frames then 13 to 16 over and over; the others: 20 then 18 and 19.
        assert_eq!([0, 16, 17, 18, 19, 20, 21].map(|n| result_frame(true, n)), [0, 16, 13, 14, 15, 16, 13]);
        assert_eq!([0, 19, 20, 21, 22].map(|n| result_frame(false, n)), [0, 19, 18, 19, 18]);
    }

    #[test]
    fn the_hammering_sounds_at_frames_7_and_12_and_ends_after_14_frames() {
        let start = Instant::now();
        let mut window = hammering(0, false, start);
        let at = |frame: u64| start + Duration::from_millis(100 * frame);
        let none = |_: u32| 1;
        assert!(window.advance(at(6), none, 99_999).is_empty());
        assert_eq!(window.advance(at(7), none, 99_999), [Cue::Sound("effect\\refine\\bs_refine_1.wav")]);
        assert!(window.advance(at(8), none, 99_999).is_empty(), "heard once");
        assert_eq!(window.advance(at(12), none, 99_999), [Cue::Sound("effect\\refine\\bs_refine_2.wav")]);
        assert!(window.process.is_some() && window.advance(at(13), none, 99_999).is_empty());
        // Cancel and the close button do nothing meanwhile.
        assert_eq!(window.click(Hit::Cancel, none, 0, at(13)), None);
        assert_eq!(window.click(Hit::Title(TitleHit::Close), none, 0, at(13)), None);
        // At the end: the success sound, then the chat line 498; the result shows with Again on.
        let cues = window.advance(at(14), none, 99_999);
        assert_eq!(cues, [Cue::Sound("effect\\refine\\bs_refine_success.wav"), Cue::Chat(498)]);
        assert!(window.process.is_none() && window.shown.as_ref().is_some_and(|shown| shown.result == 0 && shown.again));
        assert_eq!(window.click(Hit::Cancel, none, 0, at(14)), Some(Action::Close));
    }

    #[test]
    fn each_result_has_its_sound_and_chat_line() {
        let start = Instant::now();
        let end = start + Duration::from_millis(1400);
        for (result, sound, line) in [(0, "success", 498), (1, "failed", 499), (2, "failed", 1537), (3, "failed", 499)] {
            let mut window = hammering(result, false, start);
            let cues = window.advance(end, |_| 1, 99_999);
            assert_eq!(cues[cues.len() - 2..], [Cue::Sound(if sound == "success" { SUCCESS_SOUND } else { FAILED_SOUND }), Cue::Chat(line)], "{result}");
        }
        // A break disables Again.
        let mut broken = hammering(1, false, start);
        broken.advance(end, |_| 1, 99_999);
        assert!(!broken.shown.as_ref().unwrap().again);
    }

    #[test]
    fn a_list_that_comes_during_the_hammering_is_applied_at_its_end() {
        let start = Instant::now();
        let mut window = hammering(0, false, start);
        window.listed(4, 0, vec![(6240, 100, 5000)], start + Duration::from_millis(500));
        assert_eq!(window.materials.len(), 2, "kept, not applied");
        window.advance(start + Duration::from_millis(1400), |_| 1, 99_999);
        // The ore 1010 is gone from the new list: nothing picked, the notice 3242 for 5 s, Again off.
        assert_eq!(window.materials, [(6240, 100, 5000)]);
        assert_eq!(window.picked, None);
        let notice = window.notice.unwrap();
        assert_eq!((notice.message, notice.red, notice.until), (3242, true, Some(start + Duration::from_millis(1400) + Duration::from_secs(5))));
        assert!(!window.shown.as_ref().unwrap().again);
    }

    #[test]
    fn the_end_of_the_hammering_checks_the_new_list_zeny_and_blessings() {
        let start = Instant::now();
        let end = start + Duration::from_millis(1400);
        let run = |blessing: bool, list: Vec<(u32, u8, u32)>, blessings: u8, count: &dyn Fn(u32) -> u32, zeny: u32| {
            let mut window = hammering(0, blessing, start);
            window.listed(4, blessings, list, start);
            window.advance(end, count, zeny);
            (window.notice.map(|notice| (notice.message, notice.red)), window.shown.unwrap().again, window.use_blessings)
        };
        let list = vec![(1010, 100, 50), (6240, 100, 5000)];
        // Everything fine: Again on, no notice. Too little zeny for the picked ore: 3244. None carried: 3243.
        assert_eq!(run(false, list.clone(), 0, &|_| 1, 99_999), (None, true, false));
        assert_eq!(run(false, list.clone(), 0, &|_| 1, 10), (Some((3244, true)), false, false));
        assert_eq!(run(false, list.clone(), 0, &|id| u32::from(id != 1010), 99_999), (Some((3243, true)), false, false));
        // No ores at all: 2970. Use on: a list with no blessing count turns it off with 3242; with one, the standing 2967; too few
        // carried: off with 3245 and Again off.
        assert_eq!(run(false, Vec::new(), 0, &|_| 1, 99_999), (Some((2970, true)), false, false));
        assert_eq!(run(true, list.clone(), 0, &|_| 1, 99_999), (Some((3242, true)), true, false));
        assert_eq!(run(true, list.clone(), 1, &|_| 1, 99_999), (Some((2967, false)), true, true));
        assert_eq!(run(true, list, 2, &|_| 1, 99_999), (Some((3245, true)), false, false));
    }

    #[test]
    fn back_empties_a_broken_machine_and_returns_the_ready_picture_otherwise() {
        let start = Instant::now();
        let end = start + Duration::from_millis(1400);
        let mut broken = hammering(1, false, start);
        broken.advance(end, |_| 1, 99_999);
        assert_eq!(broken.click(Hit::Back, |_| 1, 0, end), None);
        assert!(broken.item.is_none() && broken.shown.is_none() && broken.materials.is_empty());
        let mut lowered = hammering(2, true, start);
        lowered.advance(end, |_| 1, 99_999);
        lowered.click(Hit::Back, |_| 1, 0, end);
        assert!(lowered.item.is_some() && lowered.shown.is_none() && lowered.picked.is_none() && !lowered.use_blessings && lowered.notice.is_none());
        assert_eq!(lowered.loaded, Some(end));
    }

    #[test]
    fn again_sends_the_same_refine_and_waits_for_the_next_hammering() {
        let start = Instant::now();
        let end = start + Duration::from_millis(1400);
        let mut window = hammering(3, false, start);
        window.listed(4, 0, vec![(1010, 100, 50)], start);
        window.advance(end, |_| 1, 99_999);
        assert_eq!(window.click(Hit::Again, |_| 1, 99_999, end), Some(Action::Refine { index: 4, ore: 1010, blessings: false }));
        assert!(window.shown.is_none() && window.waiting);
        // The next result starts the next hammering.
        window.refined(0, end);
        assert!(window.process.is_some() && !window.waiting);
    }
}
