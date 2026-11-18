//! The Lucky Roulette: 42 prizes in rows (9 at the bottom up to 3 at the top), a spin bought with gold, silver or
//! bronze points, the prize landing in the RESULT box, and Get Prize to claim it. Its icon opens it.

use std::cell::Cell;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use rustnarok_client::grf::Grf;
use rustnarok_client::lua::ItemInfo;
use rustnarok_client::net::RoulettePrize;
use rustnarok_client::text::Font;

use crate::party::greyed;
use crate::render::{Gpu, Texture};
use crate::window::{self, BUTTON_SIZE, Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI, contains, inventory, offset, screen};

const TITLE: usize = 2641;
const START_TIP: usize = 2684;
const PRIZE_TIP: usize = 2683;
/// The wheel still spins: "The roulette wheel is spinning. Please try again after checking with prizes."
pub const SPINNING: usize = 2640;
/// A refused spin's line, and a refused claim's (or a press on close while the wheel turns).
pub const SPIN_REFUSED: [u8; 3] = [0xFF, 0xFF, 0x64];
pub const REFUSED: [u8; 3] = [0xFF, 0x64, 0x64];

const SIZE: [f32; 2] = [640.0, 480.0];
pub const HEIGHT: f32 = TITLE_HEIGHT + SIZE[1];
// Places on the picture, the game's; it sits under the title bar. ponytail: the light's pace is an estimate.
const SLOT: [f32; 2] = [29.0, 29.0];
/// The bonus's icon and the RESULT box's.
const BONUS_AT: [f32; 2] = [60.0, 118.0];
const RESULT_AT: [f32; 2] = [189.0, 128.0];
/// The gold, silver and bronze counts' boxes, 84 px apart; each count right-aligned in its box.
const POINT_FIELDS: [[f32; 2]; 3] = [[575.0, 117.0], [575.0, 201.0], [575.0, 285.0]];
const POINT_FIELD_WIDTH: f32 = 25.0;
/// The stage arrow, right of the row in play's last slot.
const ARROW_X: f32 = 482.0;
/// The title bar's base button; it has no minimize.
const BASE_BUTTON: [f32; 2] = [3.0, 3.0];
/// The top row: no spin goes past it.
const TOP_ROW: u8 = 6;
const BUTTON: [f32; 2] = [99.0, 99.0];
const PRIZE_BUTTON: [f32; 2] = [203.0, 381.0];
const START_BUTTON: [f32; 2] = [339.0, 381.0];
const LIGHT_STEP_MS: u128 = 80;
/// The icon that opens the window: left of the minimap, 187 px in from the screen's right edge and 71 down, so it moves
/// with the screen's width.
const ICON_FROM_RIGHT: f32 = 187.0;
const ICON_TOP: f32 = 71.0;
const ICON_SIZE: [f32; 2] = [43.0, 43.0];

fn icon_at(screen: [f32; 2]) -> [f32; 2] {
    [screen[0] - ICON_FROM_RIGHT, ICON_TOP]
}

pub struct Assets {
    background: Texture,
    /// START and Get Prize: normal, pressed.
    start: [Texture; 2],
    prize: [Texture; 2],
    arrow: Texture,
    icon: Texture,
    light: Texture,
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |name: &str| gpu.load_texture(grf, &format!("{UI}basic_interface\\roullette\\{name}.bmp"), false);
        Self {
            background: image("roullettebg"),
            start: ["startroullette_a", "startroullette_b"].map(image),
            prize: ["getwinprize_a", "getwinprize_b"].map(image),
            arrow: image("stagearrow"),
            icon: image("roulletteicon"),
            light: gpu.solid([255, 255, 120, 110]),
        }
    }
}

/// A prize slot's corner over the background: row 0 at the bottom with 9 slots, each row up one slot shorter and
/// starting one slot further right.
pub fn slot_at(row: u8, column: u8) -> [f32; 2] {
    [158.0 + 36.0 * f32::from(row + column), 326.0 - 40.0 * f32::from(row)]
}

/// The stage arrow's corner on the picture for a row.
fn arrow_at(row: u8) -> [f32; 2] {
    [ARROW_X, 327.0 - 40.0 * f32::from(row)]
}

/// Whether START is there after this frame: there while a spin can be paid (a bronze point, or 10 silver or gold, as
/// rathena asks of a first spin) on a row under the top; gone while the wheel turns; there while a prize waits on a
/// row under the top. Otherwise as it was.
pub fn start_there(was: bool, points: [i32; 3], row: u8, turning: bool, waiting: bool) -> bool {
    let [gold, silver, bronze] = points;
    let payable = row != TOP_ROW && (bronze > 0 || silver >= 10 || gold >= 10);
    if turning { false } else { was || payable || (waiting && row < TOP_ROW) }
}

/// The grey copy of an item's icon, for the slots not in play.
fn grey_path(path: &str) -> String {
    format!("{path}|grey")
}

/// Loads the grey copies of the prizes' icons into `icons`.
pub fn load_grey(icons: &mut HashMap<String, Option<Texture>>, gpu: &Gpu, grf: &Grf, items: &HashMap<u32, ItemInfo>, window: Option<&Roulette>) {
    for prize in window.map_or(&[][..], |window| &window.prizes) {
        let Some(info) = items.get(&prize.item) else { continue };
        let path = inventory::icon_path(info, true);
        icons.entry(grey_path(&path)).or_insert_with(|| crate::render::load_image(grf, &path).ok().map(|image| gpu.texture(&greyed(image), false)));
    }
}

/// How long a spin on a row lasts: rathena ignores the player for max(1, 6 − row) seconds after one.
pub fn spin_time(row: u8) -> Duration {
    Duration::from_secs(6u64.saturating_sub(u64::from(row)).max(1))
}

/// The line for a spin's result (lines 2632 on follow rathena's results: open, close, spin failed, no points).
// ponytail: a lost spin (3) prints nothing; which line the real client shows for it isn't knowable.
pub fn spin_message(result: u8) -> Option<usize> {
    match result {
        1 => Some(2634),
        2 => Some(2635),
        _ => None,
    }
}

/// The line for a claim's result: failed, too many items, too heavy.
pub fn claim_message(result: u8) -> Option<usize> {
    match result {
        1 => Some(2636),
        2 => Some(2637),
        3 => Some(2638),
        _ => None,
    }
}

/// Over the open icon: the window opens.
pub fn icon_hit(cursor: [f32; 2]) -> bool {
    contains(icon_at(screen()), ICON_SIZE, cursor)
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(TitleHit),
    Start,
    Prize,
    Window,
}

#[derive(Debug, PartialEq)]
pub enum Action {
    Spin,
    Claim,
    /// The close button while the wheel turns: say so, stay open.
    Busy,
    Close,
}

pub struct Roulette {
    pub frame: Frame,
    pub prizes: Vec<RoulettePrize>,
    /// Gold, silver and bronze points.
    pub points: [i32; 3],
    pub bonus: u32,
    /// The prize waiting to be claimed: row and column.
    pub prize: Option<(u8, u8)>,
    /// The last spin: its row, the column it stops on, and when it began.
    spin: Option<(u8, u8, Instant)>,
    /// The row in play: the last spin's, or the opening answer's.
    pub row: u8,
    /// Whether START is there (`start_there`, each frame).
    start: Cell<bool>,
}

impl Roulette {
    pub fn new() -> Self {
        let mut frame = Frame::new([(screen()[0] / 2.0 - 320.0).floor(), (screen()[1] / 2.0 - 240.0).floor()], SIZE[0]);
        frame.open = true;
        Self { frame, prizes: Vec::new(), points: [0; 3], bonus: 0, prize: None, spin: None, row: 0, start: Cell::new(false) }
    }

    /// A spin's prize and the points left, the wheel starting at `now`.
    pub fn spun(&mut self, row: u8, column: u8, now: Instant) {
        self.prize = Some((row, column));
        self.spin = Some((row, column, now));
        self.row = row;
    }

    /// Whether START is there now; once shown it stays until the wheel turns.
    pub fn start_shown(&self, now: Instant) -> bool {
        let shown = start_there(self.start.get(), self.points, self.row, self.spinning(now), self.shown_prize(now).is_some());
        self.start.set(shown);
        shown
    }

    /// Whether the row in play is lit: while the wheel turns and while its prize waits.
    fn in_play(&self) -> Option<u8> {
        self.prize.map(|(row, _)| row)
    }

    pub fn spinning(&self, now: Instant) -> bool {
        self.spin.is_some_and(|(row, _, start)| now.saturating_duration_since(start) < spin_time(row))
    }

    /// The prize the RESULT box shows: none while the wheel spins.
    pub fn shown_prize(&self, now: Instant) -> Option<(u8, u8)> {
        if self.spinning(now) { None } else { self.prize }
    }

    /// The slot lit while the wheel spins: it runs along the row and stops on the prize.
    fn lit(&self, now: Instant) -> Option<(u8, u8)> {
        let (row, column, start) = self.spin?;
        let elapsed = now.saturating_duration_since(start);
        let total = spin_time(row);
        if elapsed >= total {
            return Some((row, column));
        }
        let columns = u128::from(9 - row.min(6));
        let steps_left = (total - elapsed).as_millis() / LIGHT_STEP_MS;
        Some((row, ((u128::from(column) + columns * steps_left - steps_left) % columns) as u8))
    }

    pub fn item_at(&self, (row, column): (u8, u8)) -> Option<u32> {
        self.prizes.iter().find(|prize| (prize.row, prize.column) == (row, column)).map(|prize| prize.item)
    }

    /// What's under `cursor`. The title bar drags nothing: only its close button answers.
    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let local = self.frame.local(cursor, HEIGHT)?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(if hit == TitleHit::Close { Hit::Title(hit) } else { Hit::Window });
        }
        let on = |corner: [f32; 2]| contains(offset(corner, [0.0, TITLE_HEIGHT]), BUTTON, local);
        Some(if on(START_BUTTON) {
            Hit::Start
        } else if on(PRIZE_BUTTON) {
            Hit::Prize
        } else {
            Hit::Window
        })
    }

    /// A press: START and Get Prize only while they're there; close waits for the wheel.
    pub fn click(&mut self, hit: Hit, now: Instant) -> Option<Action> {
        match hit {
            Hit::Title(TitleHit::Close) if self.spinning(now) => Some(Action::Busy),
            Hit::Title(TitleHit::Close) => Some(Action::Close),
            Hit::Start if self.start_shown(now) => Some(Action::Spin),
            Hit::Prize if self.shown_prize(now).is_some() => Some(Action::Claim),
            Hit::Title(_) | Hit::Start | Hit::Prize | Hit::Window => None,
        }
    }

    /// The buttons on screen, named for scripted clicks: `close`, and `start` and `prize` while they're there.
    pub fn button_boxes(&self, now: Instant) -> Vec<(&'static str, [f32; 4])> {
        if !self.frame.open {
            return Vec::new();
        }
        let place = |local: [f32; 2], size: [f32; 2]| {
            let [x, y] = self.frame.at(local);
            [x, y, size[0], size[1]]
        };
        let mut boxes = vec![("close", place(self.frame.close_button(), BUTTON_SIZE))];
        if self.start_shown(now) {
            boxes.push(("start", place(offset(START_BUTTON, [0.0, TITLE_HEIGHT]), BUTTON)));
        }
        if self.shown_prize(now).is_some() {
            boxes.push(("prize", place(offset(PRIZE_BUTTON, [0.0, TITLE_HEIGHT]), BUTTON)));
        }
        boxes
    }

    /// The prizes' and the bonus's icons.
    pub fn icons(&self, items: &HashMap<u32, ItemInfo>) -> Vec<String> {
        self.prizes.iter().map(|prize| prize.item).chain([self.bonus]).filter_map(|id| items.get(&id)).map(|info| inventory::icon_path(info, true)).collect()
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
        font: &Font,
        messages: &[String],
        now: Instant,
        pointer: ([f32; 2], bool),
    ) {
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        let frame = &self.frame;
        // The title bar with the base and close buttons (no minimize).
        frame.draw_title_only(layer, chrome, message(TITLE));
        for (button, place) in [(0, BASE_BUTTON), (2, frame.close_button())] {
            let image = chrome.button(button, contains(frame.at(place), BUTTON_SIZE, pointer.0));
            layer.sprites.push((image, frame.at(place), image.size));
        }
        let at = |local: [f32; 2]| frame.at(offset(local, [0.0, TITLE_HEIGHT]));
        let icon = |id: u32, grey: bool| {
            let path = inventory::icon_path(items.get(&id)?, true);
            icons.get(&if grey { grey_path(&path) } else { path }).and_then(Option::as_ref)
        };
        let put = |layer: &mut Layer<'a>, id: u32, corner: [f32; 2], grey: bool| {
            if let Some(texture) = icon(id, grey) {
                layer.sprites.push((texture, at(corner), texture.size));
            }
        };
        layer.sprites.push((&assets.background, at([0.0, 0.0]), SIZE));
        // The prizes, grey but on the row in play, an amount over 1 in the corner; the lit slot, and the arrow at the
        // row in play.
        let in_play = self.in_play();
        for prize in &self.prizes {
            let corner = slot_at(prize.row, prize.column);
            put(layer, prize.item, offset(corner, [1.0, 1.0]), in_play != Some(prize.row));
            if prize.amount > 1 {
                let text = prize.amount.to_string();
                layer.texts.push((text.clone(), at(offset(corner, [SLOT[0] - font.width(&text), SLOT[1] - 12.0])), [255; 3]));
            }
        }
        if let Some((row, column)) = self.lit(now) {
            layer.sprites.push((&assets.light, at(slot_at(row, column)), SLOT));
        }
        if let Some(row) = in_play {
            layer.sprites.push((&assets.arrow, at(arrow_at(row)), assets.arrow.size));
        }
        put(layer, self.bonus, BONUS_AT, false);
        if let Some(item) = self.shown_prize(now).and_then(|prize| self.item_at(prize)) {
            put(layer, item, RESULT_AT, false);
        }
        for (field, points) in POINT_FIELDS.iter().zip(self.points) {
            let text = points.to_string();
            layer.texts.push((text.clone(), at(offset(*field, [POINT_FIELD_WIDTH - font.width(&text), 0.0])), [0, 0, 0]));
        }
        // START and Get Prize while they're there, pressed while held; their names on hover.
        let there = [self.start_shown(now), self.shown_prize(now).is_some()];
        for ((corner, images, hit, tip), there) in
            [(START_BUTTON, &assets.start, Hit::Start, START_TIP), (PRIZE_BUTTON, &assets.prize, Hit::Prize, PRIZE_TIP)].into_iter().zip(there)
        {
            if !there {
                continue;
            }
            let over = self.hit(pointer.0) == Some(hit);
            layer.sprites.push((&images[usize::from(over && pointer.1)], at(corner), BUTTON));
            if over {
                window::tooltip(tooltip, chrome, font, message(tip), at(offset(corner, [0.0, -20.0])), [255; 3]);
            }
        }
    }
}

/// The icon that opens the window, with its name on hover.
pub fn draw_icon<'a>(
    layer: &mut Layer<'a>,
    tooltip: &mut Layer<'a>,
    chrome: &'a Chrome,
    assets: &'a Assets,
    font: &Font,
    messages: &[String],
    cursor: [f32; 2],
) {
    let at = icon_at(screen());
    layer.sprites.push((&assets.icon, at, ICON_SIZE));
    if icon_hit(cursor) {
        window::tooltip(tooltip, chrome, font, messages.get(TITLE).map_or("", String::as_str), offset(at, [0.0, -20.0]), [255; 3]);
    }
}

/// The icon's box on screen, for scripted clicks.
pub fn icon_box() -> [f32; 4] {
    let at = icon_at(screen());
    [at[0], at[1], ICON_SIZE[0], ICON_SIZE[1]]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_icon_keeps_to_the_right_edge() {
        assert_eq!(icon_at([1024.0, 768.0]), [837.0, 71.0]);
        assert_eq!(icon_at([1728.0, 1052.0]), [1541.0, 71.0]);
    }

    #[test]
    fn spins_and_claims() {
        // Row 0 starts at the bottom left; each row up starts a slot further right.
        assert_eq!(slot_at(0, 0), [158.0, 326.0]);
        assert_eq!(slot_at(0, 8), [446.0, 326.0]);
        assert_eq!(slot_at(6, 0), [374.0, 86.0]);
        assert_eq!([0, 2, 4, 6].map(spin_time), [6, 4, 2, 1].map(Duration::from_secs));
        assert_eq!([0, 1, 2, 3].map(spin_message), [None, Some(2634), Some(2635), None]);
        assert_eq!([0, 1, 2, 3].map(claim_message), [None, Some(2636), Some(2637), Some(2638)]);

        let start = Instant::now();
        let mut wheel = Roulette::new();
        wheel.spun(0, 4, start);
        // While it spins the prize stays hidden, the buttons aren't there and close only says so; then it shows and
        // stays lit.
        assert!(wheel.spinning(start + Duration::from_secs(5)));
        assert_eq!(wheel.shown_prize(start + Duration::from_secs(5)), None);
        assert_eq!(wheel.click(Hit::Prize, start), None);
        assert_eq!(wheel.click(Hit::Start, start), None);
        assert_eq!(wheel.click(Hit::Title(TitleHit::Close), start), Some(Action::Busy));
        assert_eq!(wheel.lit(start + Duration::from_secs(6)), Some((0, 4)));
        assert_eq!(wheel.shown_prize(start + Duration::from_secs(6)), Some((0, 4)));
        assert_eq!(wheel.click(Hit::Prize, start + Duration::from_secs(6)), Some(Action::Claim));
        assert_eq!(wheel.click(Hit::Start, start + Duration::from_secs(6)), Some(Action::Spin));
        assert_eq!(wheel.click(Hit::Title(TitleHit::Close), start + Duration::from_secs(6)), Some(Action::Close));
        // The light ends its run on the prize's column.
        let almost = start + spin_time(0) - Duration::from_millis(1);
        assert_eq!(wheel.lit(almost), Some((0, 4)));
    }

    #[test]
    fn counts_arrow_and_icons_sit_in_their_boxes() {
        let window = |place: [f32; 2]| offset(place, [0.0, TITLE_HEIGHT]);
        assert_eq!(POINT_FIELDS.map(window), [[575.0, 134.0], [575.0, 218.0], [575.0, 302.0]]);
        assert_eq!(POINT_FIELDS[0][0] + POINT_FIELD_WIDTH, 600.0);
        assert_eq!([0, 6].map(|row| window(arrow_at(row))), [[482.0, 344.0], [482.0, 104.0]]);
        assert_eq!(window(offset(slot_at(2, 3), [1.0, 1.0])), [339.0, 264.0]);
        assert_eq!((window(RESULT_AT), window(BONUS_AT)), ([189.0, 145.0], [60.0, 135.0]));
    }

    #[test]
    fn only_the_row_in_play_has_colour_and_the_arrow() {
        let start = Instant::now();
        let mut wheel = Roulette::new();
        assert_eq!(wheel.in_play(), None);
        wheel.spun(2, 1, start);
        // Turning, then its prize waiting: row 2.
        assert_eq!(wheel.in_play(), Some(2));
        assert_eq!(wheel.shown_prize(start + spin_time(2)), Some((2, 1)));
        assert_eq!(wheel.in_play(), Some(2));
        // Taken: nothing in play.
        wheel.prize = None;
        assert_eq!(wheel.in_play(), None);
    }

    #[test]
    fn start_and_get_prize_are_there_only_when_they_can_do_something() {
        let now = Instant::now();
        let mut wheel = Roulette::new();
        let named = |wheel: &Roulette, now| wheel.button_boxes(now).into_iter().map(|(name, _)| name).collect::<Vec<_>>();
        // Opened with no points: no START, no Get Prize.
        assert_eq!(named(&wheel, now), ["close"]);
        assert_eq!(wheel.click(Hit::Start, now), None);
        assert!(start_there(false, [0, 0, 1], 0, false, false));
        assert!(start_there(false, [0, 10, 0], 0, false, false));
        assert!(start_there(false, [10, 0, 0], 0, false, false));
        assert!(!start_there(false, [0, 9, 0], 0, false, false));
        // Gone while the wheel turns, whatever the points.
        assert!(!start_there(true, [0, 0, 5], 0, true, false));
        // Back with a prize waiting under the top row, not on it.
        assert!(start_there(false, [0, 0, 0], 3, false, true));
        assert!(!start_there(false, [0, 0, 0], 6, false, true));
        assert!(!start_there(false, [0, 0, 5], 6, false, false));
        // A spin on row 3 with the last point: gone while it turns, back with its prize, still there once it's taken.
        wheel.points = [0, 0, 1];
        assert_eq!(named(&wheel, now), ["close", "start"]);
        wheel.points = [0, 0, 0];
        wheel.spun(3, 0, now);
        assert_eq!(named(&wheel, now), ["close"]);
        let stopped = now + spin_time(3);
        assert_eq!(named(&wheel, stopped), ["close", "start", "prize"]);
        assert_eq!(wheel.click(Hit::Prize, stopped), Some(Action::Claim));
        wheel.prize = None;
        assert_eq!(named(&wheel, stopped), ["close", "start"]);
        assert_eq!(wheel.click(Hit::Start, stopped), Some(Action::Spin));
    }

    #[test]
    fn it_opens_in_the_middle_and_never_moves() {
        let wheel = Roulette::new();
        assert_eq!(wheel.frame.position, [screen()[0] / 2.0 - 320.0, screen()[1] / 2.0 - 240.0].map(f32::floor));
        let at = |local: [f32; 2]| wheel.frame.at(offset(local, [3.0, 3.0]));
        // The title bar, and where other windows have minimize: no drag, nothing to press.
        assert_eq!(wheel.hit(at([300.0, 0.0])), Some(Hit::Window));
        assert_eq!(wheel.hit(at(wheel.frame.minimize_button())), Some(Hit::Window));
        assert_eq!(wheel.hit(at(wheel.frame.close_button())), Some(Hit::Title(TitleHit::Close)));
    }
}
