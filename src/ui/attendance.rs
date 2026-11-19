//! The attendance check: a calendar of an event's daily rewards. The server opens it, at login while today's reward
//! waits or when the menu's button asks, with the days attended; a click on today's slot asks for the reward, which
//! comes by mail. The event's dates and rewards are the client's own (System/CheckAttendance.lub, or the picked
//! character server's CheckAttendance_<name>.lub).
//!
//! rathena `clif_parse_open_ui` (IN_UI_ATTENDANCE), `clif_ui_open` (OUT_UI_ATTENDANCE), `pc_attendance_counter`,
//! `pc_attendance_claim_reward`.

use std::collections::HashMap;
use std::time::Instant;

use rustnarok_client::grf::Grf;
use rustnarok_client::lua::{Attendance, ItemInfo};
use rustnarok_client::messages;
use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};
use crate::{Layer, UI, contains, inventory, offset, screen, window};

/// ZC_UI_OPEN's kind.
pub const UI_KIND: u8 = 7;
/// "%s Attendance Reward" (the server's name), the event's days (start month and day, end month and day), a slot's
/// "Day %d", the bubble's two lines (click today's, or today's is done), "D-day" over the days left and its unit.
const TITLE: usize = 3465;
const PERIOD: usize = 3466;
const DAY: usize = 3467;
const CLICK: usize = 3468;
const DONE: usize = 3469;
const D_DAY: usize = 3470;
const DAYS: usize = 3471;
/// The chat's line when the reward could not be given.
pub const FAILED: usize = 3472;
const CLOSE: usize = 3045;

/// The window is its picture, 488 × 413 in the middle of the screen; it can't be moved.
pub const SIZE: [f32; 2] = [488.0, 413.0];
/// Twenty slots of 58 × 60, five a row, 62 and 64 apart from (26, 95). In a slot: the reward's icon at (17, 14), its
/// amount at (31, 29) (four digits: (17, 29)), the day's label centred 45 down.
const SLOTS_AT: [f32; 2] = [26.0, 95.0];
const PITCH: [f32; 2] = [62.0, 64.0];
const SLOT: [f32; 2] = [58.0, 60.0];
const COLUMNS: usize = 5;
const MOST: usize = 20;
const ICON_AT: [f32; 2] = [17.0, 14.0];
const AMOUNT_AT: [[f32; 2]; 2] = [[31.0, 29.0], [17.0, 29.0]];
const DAY_DOWN: f32 = 45.0;
/// Text places: the title and the dates centred across the window; the bubble's lines centred in 112 px from x 343
/// (`bubble_tops`); "D-day" centred in 109 px from x 345, the days left in 54 px from x 368
/// (22 px high), their unit after them.
const TITLE_Y: f32 = 41.0;
const PERIOD_Y: f32 = 70.0;
const BUBBLE: [f32; 2] = [343.0, 112.0];
const D_DAY_AT: ([f32; 2], f32) = ([345.0, 109.0], 289.0);
const LEFT_AT: ([f32; 2], f32) = ([368.0, 54.0], 309.0);
const LEFT_SIZE: f32 = 22.0;
const DAYS_AT: [f32; 2] = [425.0, 318.0];
const CLOSE_AT: [f32; 2] = [170.0, 376.0];
const TITLE_COLOR: [u8; 3] = [0x66, 0x3F, 0x20];
const PERIOD_COLOR: [u8; 3] = [0xA4, 0x50, 0x39];
const BUBBLE_COLOR: [u8; 3] = [0xA5, 0x3C, 0x61];
const D_DAY_COLOR: [u8; 3] = [0x39, 0x59, 0x59];
const DAY_COLOR: [u8; 3] = [0x3B, 0x4F, 0xA4];
const CLOSE_COLOR: [u8; 3] = [0x31, 0x31, 0x31];
/// Today's slot swaps its two looks every 100 ms.
const BLINK_MS: u128 = 100;

pub struct Assets {
    background: Texture,
    /// Today's slot: its two looks, and pressed.
    today: [Texture; 3],
    /// Over a slot whose reward was taken: plain, and the one taken today.
    taken: [Texture; 2],
    close: [Texture; 2],
}

impl Assets {
    /// The window's pictures; none when the data has no attendance check.
    pub fn load(gpu: &Gpu, grf: &Grf) -> Option<Self> {
        let image = |name: &str| gpu.try_load_texture(grf, &format!("{UI}check_attendance\\{name}"), false).ok();
        Some(Self {
            background: image("attendance_bg.bmp")?,
            today: [image("bt_slot_a.bmp")?, image("bt_slot_b.bmp")?, image("bt_slot_press.bmp")?],
            taken: [image("bt_slot_off.tga")?, image("bt_slot_complete.tga")?],
            close: [image("bt_ok_normal.bmp")?, image("bt_ok_press.bmp")?],
        })
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    /// A slot, by its day − 1.
    Slot(usize),
    Close,
    Window,
}

pub struct Window {
    /// The days attended, and whether today's reward still waits.
    pub claimed: usize,
    pub claimable: bool,
    /// Today's slot was pressed: it keeps that look.
    pressed: bool,
    /// The days left as the game counts them (`days_left`), when the window opened.
    days_left: i64,
    opened: Instant,
    /// The days left, in their own size.
    left_text: Option<Texture>,
}

/// The days from 1970-01-01 to a YYYYMMDD date.
fn day_number(date: u32) -> i64 {
    let (year, month, day) = (i64::from(date / 10000), i64::from(date / 100 % 100), i64::from(date % 100));
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let of_era = year.rem_euclid(400);
    let of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    era * 146_097 + of_era * 365 + of_era / 4 - of_era / 100 + of_year - 719_468
}

/// The days left at Unix time `now`: D, the seconds to 15:00 UTC on the event's last day (YYYYMMDD); 0 when D ≤ 0,
/// else the day of the year (January 1 = 0) of the moment D seconds after 1970-01-01. Below a year that is the whole
/// days left; past one it starts again from 0, as the game's count does.
pub fn days_left(end: u32, now: i64) -> i64 {
    let left = day_number(end) * 86_400 + 15 * 3600 - now;
    if left <= 0 {
        return 0;
    }
    let mut day = left / 86_400;
    let mut year = 1970;
    loop {
        let length = day_number((year + 1) * 10000 + 101) - day_number(year * 10000 + 101);
        if day < length {
            return day;
        }
        day -= length;
        year += 1;
    }
}

/// The bubble's line tops for `lines` lines: the block centred on y 95 to 172, the later lines opened up
/// (126; 118, 130; 111, 123, 138; 103, 115, 130, 145).
fn bubble_tops(lines: usize) -> Vec<f32> {
    let first = ((77.0 - 15.0 * lines as f32) / 2.0 + 95.0).trunc();
    (0..lines).map(|line| first + 3.0 * (line.saturating_sub(1) + 4 * line) as f32).collect()
}

/// Where the window stands: the middle of the screen.
fn corner() -> [f32; 2] {
    [((screen()[0] - SIZE[0]) / 2.0).trunc(), ((screen()[1] - SIZE[1]) / 2.0).trunc()]
}

/// Slot `index`'s corner in the window.
fn slot(index: usize) -> [f32; 2] {
    [SLOTS_AT[0] + PITCH[0] * (index % COLUMNS) as f32, SLOTS_AT[1] + PITCH[1] * (index / COLUMNS) as f32]
}

impl Window {
    /// The server's `data`: ten times the days attended, plus one when today's reward was taken. `unix` is the Unix
    /// time now, for the days left.
    pub fn open(data: u32, event: &Attendance, unix: i64, now: Instant) -> Self {
        let mut window = Self { claimed: 0, claimable: false, pressed: false, days_left: days_left(event.end, unix), opened: now, left_text: None };
        window.refresh(data, event);
        window
    }

    /// The server's `data` again while it is open: the days attended and whether today's reward waits change; today's
    /// pressed look and the days left stay.
    pub fn refresh(&mut self, data: u32, event: &Attendance) {
        self.claimed = (data / 10) as usize;
        self.claimable = data.is_multiple_of(10) && self.claimed < event.rewards.len();
    }

    /// The days left it shows (0 shows no number).
    pub fn days_left(&self) -> i64 {
        self.days_left
    }

    /// Today's reward was given: `days` attended now.
    pub fn rewarded(&mut self, days: u32) {
        self.claimed = days as usize;
        self.claimable = false;
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let local = offset(cursor, corner().map(|value| -value));
        if !contains([0.0; 2], SIZE, local) {
            return None;
        }
        if contains(CLOSE_AT, self::close_size(), local) {
            return Some(Hit::Close);
        }
        // A slot takes the pixel past its picture too, as the game's does.
        let [x, y] = offset(local, SLOTS_AT.map(|value| -value));
        let inside = x >= 0.0 && y >= 0.0 && x % PITCH[0] <= SLOT[0] && y % PITCH[1] <= SLOT[1];
        let index = (y / PITCH[1]) as usize * COLUMNS + (x / PITCH[0]) as usize;
        Some(if inside && (x / PITCH[0]) < COLUMNS as f32 && index < MOST { Hit::Slot(index) } else { Hit::Window })
    }

    /// Whether `hit` is today's slot with its reward waiting.
    pub fn claims(&self, hit: Hit) -> bool {
        self.claimable && hit == Hit::Slot(self.claimed)
    }

    /// A press on today's slot leaves it pressed.
    pub fn press(&mut self, hit: Hit) {
        self.pressed |= self.claims(hit);
    }

    /// The reward of the slot under `cursor`: its item and amount.
    pub fn reward_at(&self, cursor: [f32; 2], event: &Attendance) -> Option<(u32, u32)> {
        match self.hit(cursor)? {
            Hit::Slot(index) => event.rewards.get(index).copied(),
            _ => None,
        }
    }

    /// Close and the slots with a reward, on screen (for the test driver).
    pub fn button_boxes(&self, event: &Attendance) -> Vec<(String, [f32; 4])> {
        let place = |at: [f32; 2], size: [f32; 2]| {
            let [x, y] = offset(corner(), at);
            [x, y, size[0], size[1]]
        };
        let slots = (0..event.rewards.len().min(MOST)).map(|index| (format!("attendance_day_{}", index + 1), place(slot(index), SLOT)));
        std::iter::once(("attendance_close".to_owned(), place(CLOSE_AT, close_size()))).chain(slots).collect()
    }

    /// Rasterizes the days left (they're larger than the UI's text).
    pub fn prepare(&mut self, gpu: &Gpu, font: &Font) {
        self.left_text = (self.days_left > 0).then(|| gpu.text_sized(font, &self.days_left.to_string(), [255; 3], LEFT_SIZE));
    }

    // ponytail: the game's labels are bold, 11 to 14 px high; all but the days left are the UI's 12 px here.
    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &'a self,
        layer: &mut Layer<'a>,
        assets: &'a Assets,
        icons: &'a HashMap<String, Option<Texture>>,
        items: &HashMap<u32, ItemInfo>,
        font: &Font,
        messages: &[String],
        event: &Attendance,
        server: &str,
        (pointer, down): ([f32; 2], bool),
        now: Instant,
    ) {
        let at = |local: [f32; 2]| offset(corner(), local);
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        let centred = |layer: &mut Layer<'a>, text: &str, [x, width]: [f32; 2], y: f32, color: [u8; 3]| {
            layer.texts.push((text.to_owned(), at([x + ((width - font.width(text)) / 2.0).floor(), y]), color));
        };
        layer.sprites.push((&assets.background, at([0.0; 2]), SIZE));
        centred(layer, &messages::fill(message(TITLE), &[&server]), [0.0, SIZE[0]], TITLE_Y, TITLE_COLOR);
        let [start, end] = [event.start, event.end].map(|date| [date / 100 % 100, date % 100]);
        centred(layer, &messages::fill(message(PERIOD), &[&start[0], &start[1], &end[0], &end[1]]), [0.0, SIZE[0]], PERIOD_Y, PERIOD_COLOR);
        centred(layer, message(D_DAY), D_DAY_AT.0, D_DAY_AT.1, D_DAY_COLOR);
        if let Some(left) = &self.left_text {
            layer.sprites.push((left, at([LEFT_AT.0[0] + ((LEFT_AT.0[1] - left.size[0]) / 2.0).floor(), LEFT_AT.1]), left.size));
        }
        layer.texts.push((message(DAYS).to_owned(), at(DAYS_AT), D_DAY_COLOR));
        // The bubble: today's day to click, or the last one done.
        let bubble = if self.claimable { messages::fill(message(CLICK), &[&(self.claimed + 1)]) } else { messages::fill(message(DONE), &[&self.claimed]) };
        let lines: Vec<String> = bubble.replace("\\n", "\n").lines().flat_map(|line| window::wrap(font, line, BUBBLE[1], usize::MAX)).collect();
        for (line, top) in lines.iter().zip(bubble_tops(lines.len())) {
            centred(layer, line, BUBBLE, top, BUBBLE_COLOR);
        }
        if self.claimable {
            let blink = (now.saturating_duration_since(self.opened).as_millis() / BLINK_MS) as usize & 1;
            layer.sprites.push((&assets.today[if self.pressed { 2 } else { blink }], at(slot(self.claimed)), SLOT));
        }
        for (index, &(item, amount)) in event.rewards.iter().enumerate().take(MOST) {
            let corner = slot(index);
            if let Some(Some(icon)) = items.get(&item).map(|info| inventory::icon_path(info, true)).and_then(|path| icons.get(&path)) {
                layer.sprites.push((icon, at(offset(corner, ICON_AT)), icon.size));
            }
            // The amount, black in a white outline.
            let amount_at = at(offset(corner, AMOUNT_AT[usize::from(amount >= 1000)]));
            for step in [[-1.0, 0.0], [1.0, 0.0], [0.0, -1.0], [0.0, 1.0]] {
                layer.texts.push((amount.to_string(), offset(amount_at, step), [255; 3]));
            }
            layer.texts.push((amount.to_string(), amount_at, [0; 3]));
            centred(layer, &messages::fill(message(DAY), &[&(index + 1)]), [corner[0], SLOT[0]], corner[1] + DAY_DOWN, DAY_COLOR);
        }
        let pressed = down && self.hit(pointer) == Some(Hit::Close);
        layer.sprites.push((&assets.close[usize::from(pressed)], at(CLOSE_AT), close_size()));
        let label = message(CLOSE);
        centred(layer, label, [CLOSE_AT[0], close_size()[0]], CLOSE_AT[1] + ((close_size()[1] - font.line_height()) / 2.0).floor(), CLOSE_COLOR);
    }
}

impl Window {
    /// What was taken is covered, icon and words: a window piece of its own, over `draw`'s. The one taken today has
    /// its own cover.
    pub fn draw_taken<'a>(&self, layer: &mut Layer<'a>, assets: &'a Assets, event: &Attendance) {
        for index in 0..self.claimed.min(event.rewards.len()).min(MOST) {
            let cover = &assets.taken[usize::from(index + 1 == self.claimed && !self.claimable)];
            layer.sprites.push((cover, offset(corner(), slot(index)), SLOT));
        }
    }
}

/// The Close button's size.
fn close_size() -> [f32; 2] {
    [146.0, 30.0]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Unix time at hour:minute UTC on a YYYYMMDD date.
    fn utc(date: u32, hour: i64, minute: i64) -> i64 {
        day_number(date) * 86_400 + hour * 3600 + minute * 60
    }

    fn event() -> Attendance {
        Attendance { start: 20261101, end: 20261130, rewards: (0..20).map(|day| (501 + day, 1)).collect() }
    }

    #[test]
    fn opens_from_the_servers_count() {
        let now = Instant::now();
        // 3 days attended, today's reward waiting: day 4 is the one to click.
        let mut window = Window::open(30, &event(), utc(20261115, 0, 0), now);
        assert_eq!((window.claimed, window.claimable, window.days_left), (3, true, 15));
        let middle = |index: usize| offset(offset(corner(), slot(index)), [29.0, 30.0]);
        assert_eq!(window.hit(middle(3)), Some(Hit::Slot(3)));
        assert!(window.claims(Hit::Slot(3)) && !window.claims(Hit::Slot(2)) && !window.claims(Hit::Close));
        assert_eq!(window.reward_at(middle(19), &event()), Some((520, 1)));
        // The gap between slots and the picture's frame are the window; Close is its button.
        assert_eq!(window.hit(offset(middle(0), [31.0, 0.0])), Some(Hit::Window));
        assert_eq!(window.hit(offset(corner(), [200.0, 390.0])), Some(Hit::Close));
        assert_eq!(window.hit(offset(corner(), [-1.0, 10.0])), None);
        // The reward given: 4 days, nothing waiting.
        window.rewarded(4);
        assert_eq!((window.claimed, window.claimable), (4, false));
        // Today's reward already taken (the 1), or every day done: nothing to click.
        assert!(!Window::open(31, &event(), utc(20261115, 0, 0), now).claimable);
        assert!(!Window::open(200, &event(), utc(20261115, 0, 0), now).claimable);
        // The last day's afternoon and after it: no days left.
        assert_eq!([20261130, 20261201].map(|today| Window::open(0, &event(), utc(today, 15, 0), now).days_left), [0, 0]);
        // Opened again while open: the days change, today's pressed look and the days left stay.
        let mut window = Window::open(30, &event(), utc(20261115, 0, 0), now);
        window.press(Hit::Slot(3));
        window.refresh(40, &event());
        assert_eq!((window.claimed, window.claimable, window.pressed, window.days_left), (4, true, true, 15));
    }

    #[test]
    fn counts_days_left_as_the_game() {
        assert_eq!(days_left(20260106, utc(20251203, 0, 0)), 34);
        // 408 days away: the count starts again after a year.
        assert_eq!(days_left(20271231, utc(20261118, 3, 0)), 43);
        // It turns at 15:00 UTC.
        assert_eq!(days_left(20261130, utc(20261130, 14, 59)), 0);
        assert_eq!(days_left(20261130, utc(20261129, 14, 59)), 1);
        assert_eq!(days_left(20261130, utc(20261129, 15, 1)), 0);
        assert_eq!(days_left(20261130, utc(20261201, 0, 0)), 0);
    }

    #[test]
    fn places_the_bubbles_lines() {
        assert_eq!(bubble_tops(1), [126.0]);
        assert_eq!(bubble_tops(2), [118.0, 130.0]);
        assert_eq!(bubble_tops(3), [111.0, 123.0, 138.0]);
        assert_eq!(bubble_tops(4), [103.0, 115.0, 130.0, 145.0]);
    }

    #[test]
    fn counts_days_across_months_and_years() {
        assert_eq!(day_number(19700101), 0);
        assert_eq!(day_number(20260106) - day_number(20251203), 34);
        assert_eq!(day_number(20240301) - day_number(20240228), 2, "a leap year");
    }
}
