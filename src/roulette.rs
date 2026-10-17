//! The Lucky Roulette: 42 prizes in rows (9 at the bottom up to 3 at the top), a spin bought with gold, silver or
//! bronze points, the prize landing in the RESULT box, and Get Prize to claim it. Its icon opens it.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use rustnarok_client::grf::Grf;
use rustnarok_client::lua::ItemInfo;
use rustnarok_client::net::RoulettePrize;
use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};
use crate::window::{self, BUTTON_SIZE, Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, SCREEN, UI, contains, inventory, offset};

const TITLE: usize = 2641;
const START_TIP: usize = 2684;
const PRIZE_TIP: usize = 2683;
/// The wheel still spins: "The roulette wheel is spinning. Please try again after checking with prizes."
pub const SPINNING: usize = 2640;

const SIZE: [f32; 2] = [640.0, 480.0];
pub const HEIGHT: f32 = TITLE_HEIGHT + SIZE[1];
// ponytail: the slots are measured from roullettebg.bmp and the buttons matched against it (their pictures carry the
// panel behind them); the boxes' and fields' places, the light's pace and the icon's place on screen are estimates.
const SLOT: [f32; 2] = [29.0, 29.0];
const BONUS_AT: [f32; 2] = [58.0, 117.0];
const RESULT_AT: [f32; 2] = [186.0, 126.0];
/// The gold, silver and bronze fields.
const POINT_FIELDS: [[f32; 2]; 3] = [[571.0, 113.0], [571.0, 193.0], [571.0, 273.0]];
const POINT_FIELD_WIDTH: f32 = 33.0;
const BUTTON: [f32; 2] = [99.0, 99.0];
const PRIZE_BUTTON: [f32; 2] = [203.0, 381.0];
const START_BUTTON: [f32; 2] = [339.0, 381.0];
const LIGHT_STEP_MS: u128 = 80;
/// The icon that opens the window, left of the minimap.
const ICON_AT: [f32; 2] = [826.0, 18.0];
const ICON_SIZE: [f32; 2] = [43.0, 43.0];

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
    contains(ICON_AT, ICON_SIZE, cursor)
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
    /// The wheel still spins: say so, send nothing.
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
}

impl Roulette {
    pub fn new() -> Self {
        let mut frame = Frame::new([((SCREEN[0] - SIZE[0]) / 2.0).floor(), ((SCREEN[1] - HEIGHT) / 2.0).floor()], SIZE[0]);
        frame.open = true;
        Self { frame, prizes: Vec::new(), points: [0; 3], bonus: 0, prize: None, spin: None }
    }

    /// A spin's prize and the points left, the wheel starting at `now`.
    pub fn spun(&mut self, row: u8, column: u8, now: Instant) {
        self.prize = Some((row, column));
        self.spin = Some((row, column, now));
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

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let local = self.frame.local(cursor, HEIGHT)?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(Hit::Title(hit));
        }
        if self.frame.minimized {
            return Some(Hit::Window);
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

    pub fn click(&mut self, hit: Hit, now: Instant) -> Option<Action> {
        match hit {
            Hit::Title(TitleHit::Close) => Some(Action::Close),
            Hit::Title(title) => {
                self.frame.click_title(title);
                None
            }
            Hit::Start | Hit::Prize if self.spinning(now) => Some(Action::Busy),
            Hit::Start => Some(Action::Spin),
            Hit::Prize => Some(Action::Claim),
            Hit::Window => None,
        }
    }

    /// The buttons on screen, named for scripted clicks: `start`, `prize`, `close`.
    pub fn button_boxes(&self) -> Vec<(&'static str, [f32; 4])> {
        if !self.frame.open {
            return Vec::new();
        }
        let place = |local: [f32; 2], size: [f32; 2]| {
            let [x, y] = self.frame.at(local);
            [x, y, size[0], size[1]]
        };
        let mut boxes = vec![("close", place(self.frame.close_button(), BUTTON_SIZE))];
        if !self.frame.minimized {
            boxes.push(("start", place(offset(START_BUTTON, [0.0, TITLE_HEIGHT]), BUTTON)));
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
        frame.draw_title(layer, chrome, message(TITLE), pointer.0);
        if frame.minimized {
            return;
        }
        let at = |local: [f32; 2]| frame.at(offset(local, [0.0, TITLE_HEIGHT]));
        let icon = |id: u32| items.get(&id).and_then(|info| icons.get(&inventory::icon_path(info, true))).and_then(Option::as_ref);
        let put = |layer: &mut Layer<'a>, id: u32, corner: [f32; 2]| {
            if let Some(texture) = icon(id) {
                layer.sprites.push((texture, offset(at(corner), [(SLOT[0] - texture.size[0]) / 2.0, (SLOT[1] - texture.size[1]) / 2.0]), texture.size));
            }
        };
        layer.sprites.push((&assets.background, at([0.0, 0.0]), SIZE));
        // The prizes, an amount over 1 in the corner; the lit slot, and the arrow at the spun row.
        for prize in &self.prizes {
            let corner = slot_at(prize.row, prize.column);
            put(layer, prize.item, corner);
            if prize.amount > 1 {
                let text = prize.amount.to_string();
                layer.texts.push((text.clone(), at(offset(corner, [SLOT[0] - font.width(&text), SLOT[1] - 12.0])), [255; 3]));
            }
        }
        if let Some((row, column)) = self.lit(now) {
            layer.sprites.push((&assets.light, at(slot_at(row, column)), SLOT));
            if self.spinning(now) {
                layer.sprites.push((&assets.arrow, at(offset(slot_at(row, 0), [-32.0, -1.0])), assets.arrow.size));
            }
        }
        put(layer, self.bonus, BONUS_AT);
        if let Some(item) = self.shown_prize(now).and_then(|prize| self.item_at(prize)) {
            put(layer, item, RESULT_AT);
        }
        for (field, points) in POINT_FIELDS.iter().zip(self.points) {
            let text = points.to_string();
            layer.texts.push((text.clone(), at(offset(*field, [POINT_FIELD_WIDTH - 2.0 - font.width(&text), 1.0])), [0, 0, 0]));
        }
        // START and Get Prize, pressed while held; their names on hover.
        for (corner, images, hit, tip) in [(START_BUTTON, &assets.start, Hit::Start, START_TIP), (PRIZE_BUTTON, &assets.prize, Hit::Prize, PRIZE_TIP)] {
            let over = self.hit(pointer.0) == Some(hit);
            layer.sprites.push((&images[usize::from(over && pointer.1)], at(corner), BUTTON));
            if over {
                window::tooltip(tooltip, chrome, font, message(tip), offset(pointer.0, [12.0, 12.0]), [255; 3]);
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
    layer.sprites.push((&assets.icon, ICON_AT, ICON_SIZE));
    if icon_hit(cursor) {
        window::tooltip(tooltip, chrome, font, messages.get(TITLE).map_or("", String::as_str), offset(cursor, [12.0, 12.0]), [255; 3]);
    }
}

/// The icon's box on screen, for scripted clicks.
pub fn icon_box() -> [f32; 4] {
    [ICON_AT[0], ICON_AT[1], ICON_SIZE[0], ICON_SIZE[1]]
}

#[cfg(test)]
mod tests {
    use super::*;

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
        // While it spins the prize stays hidden and the buttons only say so; then it shows and stays lit.
        assert!(wheel.spinning(start + Duration::from_secs(5)));
        assert_eq!(wheel.shown_prize(start + Duration::from_secs(5)), None);
        assert_eq!(wheel.click(Hit::Prize, start), Some(Action::Busy));
        assert_eq!(wheel.lit(start + Duration::from_secs(6)), Some((0, 4)));
        assert_eq!(wheel.shown_prize(start + Duration::from_secs(6)), Some((0, 4)));
        assert_eq!(wheel.click(Hit::Prize, start + Duration::from_secs(6)), Some(Action::Claim));
        assert_eq!(wheel.click(Hit::Start, start + Duration::from_secs(6)), Some(Action::Spin));
        assert_eq!(wheel.click(Hit::Title(TitleHit::Close), start), Some(Action::Close));
        // The light ends its run on the prize's column.
        let almost = start + spin_time(0) - Duration::from_millis(1);
        assert_eq!(wheel.lit(almost), Some((0, 4)));
    }
}
