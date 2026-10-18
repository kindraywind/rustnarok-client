//! The battleground queue (the menu's battle button): the battlegrounds to apply for, a picked one's details and its
//! apply buttons, and while queued the wait and its cancel button.

use std::collections::HashMap;

use rustnarok_client::grf::Grf;
use rustnarok_client::lua::Battleground;
use rustnarok_client::messages;
use rustnarok_client::net::{QUEUE_ALONE, QUEUE_GUILD, QUEUE_PARTY};
use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};
use crate::window::{self, Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, SCREEN, UI, contains};

const TITLE: usize = 2149;
/// "Name:", "Goal:", "Format:", "Level:", "Win:", "Draw:", "Loss:".
const LABELS: [usize; 7] = [2139, 2140, 2141, 2142, 2143, 2144, 2145];
const FORMAT: usize = 2150;
const QUEUED_NAME: usize = 2136;
const QUEUED_NUMBER: usize = 2138;
/// The questions before applying, alone, as a party, as a guild; and before leaving the queue.
pub const APPLY_ASKS: [usize; 3] = [2146, 2147, 2148];
pub const CANCEL_ASKS: usize = 1544;
/// The lobby's question, its note, and the time left to answer.
pub const LOBBY_ASKS: usize = 2127;
pub const LOBBY_NOTE: [usize; 4] = [2128, 2129, 2130, 2131];
pub const LOBBY_TIME: usize = 2134;
/// rathena's time to answer (`bg_on_ready_expire`).
pub const LOBBY_SECONDS: u64 = 20;
/// Entering, accepted.
pub const LOBBY_ACCEPTED: usize = 1542;

/// The line for an apply result (`e_bg_queue_apply_ack`); 0 has rathena's own line.
// ponytail: the lines follow the results up to 8; 9 (leader only) and 10 (class) by meaning, since the table has the
// class line before the leader's.
pub fn apply_result(result: u8) -> Option<usize> {
    match result {
        1..=8 => Some(2098 + usize::from(result)),
        9 => Some(2108),
        10 => Some(2107),
        _ => None,
    }
}

/// The line for leaving the queue: left, or not in one.
// ponytail: which lines answer ZC_ACK_ENTRY_QUEUE_CANCEL is an estimate.
pub fn cancel_result(result: u8) -> usize {
    if result == 1 { 2110 } else { 2112 }
}

/// The level line: up to a level, from a level, a range, or none.
pub fn level_line(messages: &[String], (kind, [low, high]): (u8, [u32; 2])) -> String {
    let message = |id: usize| messages.get(id).map_or("", String::as_str);
    match kind {
        1 => messages::fill(message(2151), &[&low]),
        2 => messages::fill(message(2152), &[&low]),
        3 => messages::fill(message(2153), &[&low, &high]),
        _ => message(2154).to_owned(),
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(TitleHit),
    /// A battleground of the list, by its place.
    Row(usize),
    /// An apply button: `QUEUE_ALONE`, `QUEUE_PARTY` or `QUEUE_GUILD`.
    Apply(u16),
    Cancel,
    Window,
}

// ponytail: the window's size and where each piece goes are estimates: the list on the left, the picked one's picture
// and details on the right, the apply buttons under them; while queued, the wait and the cancel button.
const WIDTH: f32 = 470.0;
const HEIGHT: f32 = 330.0;
const LIST_LEFT: f32 = 8.0;
const LIST_WIDTH: f32 = 170.0;
const LIST_TOP: f32 = TITLE_HEIGHT + 28.0;
const ROW: f32 = 20.0;
const DETAIL_LEFT: f32 = 190.0;
const PICTURE_TOP: f32 = TITLE_HEIGHT + 8.0;
const PICTURE: [f32; 2] = [128.0, 128.0];
const DETAIL_TOP: f32 = PICTURE_TOP + PICTURE[1] + 8.0;
const DETAIL_LINE: f32 = 18.0;
const VALUE_LEFT: f32 = DETAIL_LEFT + 60.0;
const BUTTON: [f32; 2] = [80.0, 22.0];
const BUTTONS_TOP: f32 = HEIGHT - 32.0;
const BUTTON_STEP: f32 = 88.0;
const TEXT: [u8; 3] = [0, 0, 0];

/// A button's looks: out, over, pressed, off.
type Looks = [Texture; 4];

pub struct Assets {
    apply: [Looks; 3],
    cancel: Looks,
    name_bg: Texture,
    picked: Texture,
    /// The battlegrounds' pictures by file name, and the one for none picked.
    pictures: HashMap<String, Texture>,
    none: Texture,
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf, list: &[Battleground]) -> Self {
        let folder = format!("{UI}battle_field\\");
        // Missing pictures (older data) are blank rather than no client.
        let image = |name: &str| gpu.try_load_texture(grf, &format!("{folder}{name}.bmp"), false).unwrap_or_else(|_| gpu.solid([0, 0, 0, 0]));
        let looks = |name: &str| [image(name), image(&format!("{name}_a")), image(&format!("{name}_b")), image(&format!("{name}_c"))];
        let pictures = list.iter().map(|entry| (entry.picture.clone(), image(entry.picture.trim_end_matches(".bmp")))).collect();
        Self {
            apply: [looks("apply_individual"), looks("apply_party"), looks("apply_guild")],
            cancel: looks("cancel_waiting"),
            name_bg: image("field_name_bg"),
            picked: gpu.solid([0xCE, 0xDE, 0xF7, 0xFF]),
            pictures,
            none: image("no_battle"),
        }
    }
}

const KINDS: [u16; 3] = [QUEUE_ALONE, QUEUE_PARTY, QUEUE_GUILD];

pub struct QueueWindow {
    pub frame: Frame,
    pub picked: Option<usize>,
}

impl Default for QueueWindow {
    fn default() -> Self {
        Self { frame: Frame::new([((SCREEN[0] - WIDTH) / 2.0).floor(), 120.0], WIDTH), picked: None }
    }
}

impl QueueWindow {
    /// Whether the window is under the pointer.
    pub fn covers(&self, cursor: [f32; 2]) -> bool {
        self.frame.local(cursor, HEIGHT).is_some()
    }

    /// The apply buttons the picked battleground offers, left to right.
    fn applies(&self, list: &[Battleground]) -> Vec<u16> {
        let Some(entry) = self.picked.and_then(|index| list.get(index)) else { return Vec::new() };
        KINDS.into_iter().zip(entry.applies).filter(|(_, offered)| *offered).map(|(kind, _)| kind).collect()
    }

    fn button_at(place: usize) -> [f32; 2] {
        [DETAIL_LEFT + place as f32 * BUTTON_STEP, BUTTONS_TOP]
    }

    fn row_box(row: usize) -> ([f32; 2], [f32; 2]) {
        ([LIST_LEFT, LIST_TOP + row as f32 * ROW], [LIST_WIDTH, ROW])
    }

    /// `queued`: the battleground queued for, if any (the list then gives way to the wait).
    pub fn hit(&self, cursor: [f32; 2], list: &[Battleground], queued: bool) -> Option<Hit> {
        let local = self.frame.local(cursor, HEIGHT)?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(Hit::Title(hit));
        }
        if queued {
            return Some(if contains(Self::button_at(0), BUTTON, local) { Hit::Cancel } else { Hit::Window });
        }
        if let Some(row) = (0..list.len()).find(|&row| {
            let (corner, size) = Self::row_box(row);
            contains(corner, size, local)
        }) {
            return Some(Hit::Row(row));
        }
        for (place, kind) in self.applies(list).into_iter().enumerate() {
            if contains(Self::button_at(place), BUTTON, local) {
                return Some(Hit::Apply(kind));
            }
        }
        Some(Hit::Window)
    }

    /// The rows (by battleground name) and buttons (`individual`, `party`, `guild`, `cancel`), for the test driver.
    pub fn button_boxes(&self, list: &[Battleground], queued: bool) -> Vec<(String, [f32; 4])> {
        if !self.frame.open || self.frame.minimized {
            return Vec::new();
        }
        let place = |corner: [f32; 2], size: [f32; 2]| {
            let [x, y] = self.frame.at(corner);
            [x, y, size[0], size[1]]
        };
        if queued {
            return vec![("cancel".to_owned(), place(Self::button_at(0), BUTTON))];
        }
        let rows = list.iter().enumerate().map(|(row, entry)| {
            let (corner, size) = Self::row_box(row);
            (entry.name.clone(), place(corner, size))
        });
        let buttons = self.applies(list).into_iter().enumerate().map(|(index, kind)| {
            let name = match kind {
                QUEUE_ALONE => "individual",
                QUEUE_PARTY => "party",
                _ => "guild",
            };
            (name.to_owned(), place(Self::button_at(index), BUTTON))
        });
        rows.chain(buttons).collect()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &self,
        layer: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        font: &Font,
        messages: &[String],
        list: &[Battleground],
        queued: Option<&(String, Option<u32>)>,
        (pointer, down): ([f32; 2], bool),
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
        let hit = self.hit(pointer, list, queued.is_some());
        let button = |layer: &mut Layer<'a>, looks: &'a Looks, corner: [f32; 2], over: bool| {
            let look = if over { 1 + usize::from(down) } else { 0 };
            layer.sprites.push((&looks[look], at(corner), BUTTON));
        };
        // While queued: which battleground, how many wait, and cancelling.
        if let Some((name, number)) = queued {
            layer.texts.push((messages::fill(message(QUEUED_NAME), &[name]), at([DETAIL_LEFT, DETAIL_TOP]), TEXT));
            if let Some(number) = number {
                layer.texts.push((messages::fill(message(QUEUED_NUMBER), &[number]), at([DETAIL_LEFT, DETAIL_TOP + DETAIL_LINE]), TEXT));
            }
            let picture =
                list.iter().find(|entry| rustnarok_client::net::queue_name(&entry.name) == *name).and_then(|entry| assets.pictures.get(&entry.picture));
            let picture = picture.unwrap_or(&assets.none);
            layer.sprites.push((picture, at([DETAIL_LEFT, PICTURE_TOP]), PICTURE));
            button(layer, &assets.cancel, Self::button_at(0), hit == Some(Hit::Cancel));
            return;
        }
        // The list, the picked one shaded.
        layer.sprites.push((&assets.name_bg, at([LIST_LEFT, TITLE_HEIGHT + 6.0]), [LIST_WIDTH, 18.0]));
        layer.texts.push((message(TITLE).to_owned(), at([LIST_LEFT + 6.0, TITLE_HEIGHT + 8.0]), TEXT));
        for (row, entry) in list.iter().enumerate() {
            let (corner, size) = Self::row_box(row);
            if self.picked == Some(row) {
                layer.sprites.push((&assets.picked, at(corner), size));
            }
            layer.texts.push((window::fit(font, &entry.display, LIST_WIDTH - 8.0), at([corner[0] + 4.0, corner[1] + 3.0]), TEXT));
        }
        // The picked one: its picture, its details, the ways to apply.
        let Some(entry) = self.picked.and_then(|index| list.get(index)) else {
            layer.sprites.push((&assets.none, at([DETAIL_LEFT, PICTURE_TOP]), PICTURE));
            return;
        };
        layer.sprites.push((assets.pictures.get(&entry.picture).unwrap_or(&assets.none), at([DETAIL_LEFT, PICTURE_TOP]), PICTURE));
        let values = [
            entry.display.clone(),
            entry.goal.clone(),
            messages::fill(message(FORMAT), &[&entry.sizes[0], &entry.sizes[1]]),
            level_line(messages, entry.condition),
            entry.rewards[0].clone(),
            entry.rewards[1].clone(),
            entry.rewards[2].clone(),
        ];
        for (line, (label, value)) in LABELS.iter().zip(values).enumerate() {
            let top = DETAIL_TOP + line as f32 * DETAIL_LINE;
            layer.texts.push((message(*label).to_owned(), at([DETAIL_LEFT, top]), TEXT));
            layer.texts.push((window::fit(font, &value, WIDTH - VALUE_LEFT - 8.0), at([VALUE_LEFT, top]), TEXT));
        }
        for (place, kind) in self.applies(list).into_iter().enumerate() {
            let index = KINDS.iter().position(|&each| each == kind).unwrap_or(0);
            button(layer, &assets.apply[index], Self::button_at(place), hit == Some(Hit::Apply(kind)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list() -> Vec<Battleground> {
        let entry = |name: &str, applies| Battleground { name: name.into(), display: name.into(), applies, ..Default::default() };
        vec![entry("Tierra Gorge", [true, true, true]), entry("KVM (Level 80 and up)", [true, false, true])]
    }

    #[test]
    fn picks_a_battleground_and_offers_its_applies() {
        let mut window = QueueWindow::default();
        window.frame.open = true;
        let list = list();
        let at = |local: [f32; 2]| window.frame.at(local);
        assert_eq!(window.hit(at([20.0, LIST_TOP + ROW + 5.0]), &list, false), Some(Hit::Row(1)));
        assert_eq!(window.hit(at([DETAIL_LEFT + 5.0, BUTTONS_TOP + 5.0]), &list, false), Some(Hit::Window), "nothing picked yet");
        window.picked = Some(1);
        // KVM offers alone and guild: the second button is the guild's.
        assert_eq!(window.hit(at([DETAIL_LEFT + 5.0, BUTTONS_TOP + 5.0]), &list, false), Some(Hit::Apply(QUEUE_ALONE)));
        assert_eq!(window.hit(at([DETAIL_LEFT + BUTTON_STEP + 5.0, BUTTONS_TOP + 5.0]), &list, false), Some(Hit::Apply(QUEUE_GUILD)));
        let names: Vec<String> = window.button_boxes(&list, false).into_iter().map(|(name, _)| name).collect();
        assert_eq!(names, ["Tierra Gorge", "KVM (Level 80 and up)", "individual", "guild"]);
        // Queued: only the wait and cancelling.
        assert_eq!(window.hit(at([DETAIL_LEFT + 5.0, BUTTONS_TOP + 5.0]), &list, true), Some(Hit::Cancel));
        assert_eq!(window.button_boxes(&list, true).into_iter().map(|(name, _)| name).collect::<Vec<_>>(), ["cancel"]);
    }

    #[test]
    fn says_results_and_levels() {
        assert_eq!([0, 1, 6, 8, 9, 10].map(apply_result), [None, Some(2099), Some(2104), Some(2106), Some(2108), Some(2107)]);
        assert_eq!((cancel_result(1), cancel_result(0)), (2110, 2112));
        let mut messages = vec![String::new(); 2155];
        messages[2151] = "LV %d and lower".into();
        messages[2152] = "LV %d and higher".into();
        messages[2153] = "LV %d ~ %d".into();
        messages[2154] = "No restrictions".into();
        assert_eq!(level_line(&messages, (1, [59, 0])), "LV 59 and lower");
        assert_eq!(level_line(&messages, (2, [80, 0])), "LV 80 and higher");
        assert_eq!(level_line(&messages, (3, [60, 79])), "LV 60 ~ 79");
        assert_eq!(level_line(&messages, (0, [0, 0])), "No restrictions");
    }
}
