//! The player's mercenary: its window (Ctrl+R) with its values, contract and buttons, its skill list, and how the
//! client steers it (rathena runs no AI for it: following the player is the client's).

use std::time::{Duration, Instant};

use rustnarok_client::grf::Grf;
use rustnarok_client::net::Mercenary;
use rustnarok_client::text::Font;

use crate::homunculus::{Gauge, gauge, gauge_text};
use crate::render::{Gpu, Texture};
use crate::window::{Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI, contains};

const TITLE_ARCHER: usize = 1181;
const TITLE_SWORDMAN: usize = 1182;
const TITLE_SPEARMAN: usize = 1183;
const TITLE_MONSTER: usize = 1367;
const NAME: usize = 407;
const LEVEL: usize = 408;
const EXPIRATION: usize = 1184;
const LOYALTY: usize = 1185;
const SUMMONS: usize = 1186;
const KILLS: usize = 1187;
pub const SKILLS_TITLE: usize = 1262;
/// The contract's end: a strftime pattern ("%m/%d %H:%M").
pub const CONTRACT_PATTERN: usize = 1260;
pub const EXPIRED: usize = 1266;
pub const DIED: usize = 1267;
pub const RELEASED: usize = 1268;
pub const DISMISS_QUESTION: usize = 1370;

/// The window's title, by the mercenary's job.
pub fn title(job: u16) -> usize {
    match job {
        6017..=6026 => TITLE_ARCHER,
        6027..=6036 => TITLE_SPEARMAN,
        6037..=6046 => TITLE_SWORDMAN,
        _ => TITLE_MONSTER,
    }
}

/// The homunculus's background (it brings its own title strip); places from the window's corner, title included.
const WIDTH: f32 = 280.0;
const HEIGHT: f32 = 180.0;
const POSITION: [f32; 2] = [150.0, 100.0];
/// The background as the homunculus window puts it (one pixel lower: the values' rows were measured so).
pub(crate) const BACKGROUND_TOP: f32 = TITLE_HEIGHT - 16.0;
/// The eight values' right edge and their first row's top; a row every 18 px (the background's underlines).
const VALUE_RIGHT: f32 = 79.0;
const FIRST_ROW: f32 = 23.0;
const ROW: f32 = 18.0;
pub(crate) const TEXT: [u8; 3] = [0, 0, 0];
const LABEL: f32 = 100.0;
const VALUE: f32 = 135.0;
const NAME_TOP: f32 = 24.0;
const NAME_SHADOW: [u8; 3] = [0x64, 0x64, 0x64];
const LEVEL_TOP: f32 = 47.0;
const BUTTON: [f32; 2] = [42.0, 20.0];
const DISMISS: [f32; 2] = [187.0, 44.0];
const SKILLS: [f32; 2] = [232.0, 44.0];
/// HP and SP: 85 × 9 bars, their numbers after them.
const HP_BAR: [f32; 2] = [117.0, 73.0];
const SP_BAR: [f32; 2] = [117.0, 88.0];
const BAR: [f32; 2] = [85.0, 9.0];
const FILL_END: f32 = 4.0;
const NUMBERS_X: f32 = 218.0;
const NUMBERS_UP: f32 = 3.0;
const EXPIRATION_TOP: f32 = 111.0;
const CONTRACT_X: f32 = 155.0;
const CONTRACT_COLOR: [u8; 3] = [0xFF, 0, 0];
/// The lower lines: label place, value place.
const LOWER: [([f32; 2], [f32; 2]); 3] = [([100.0, 135.0], [170.0, 133.0]), ([100.0, 159.0], [170.0, 157.0]), ([230.0, 133.0], [245.0, 157.0])];

pub struct Assets {
    background: Option<Texture>,
    track: Texture,
    /// The blue and red fills: left, middle, right.
    fills: [[Texture; 3]; 2],
    /// Normal, hovered, pressed.
    skill: [Texture; 3],
    dismiss: [Texture; 3],
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let basic = |name: &str| gpu.load_texture(grf, &format!("{UI}basic_interface\\{name}.bmp"), false);
        let button = |name: &str| ["", "_a", "_b"].map(|suffix| gpu.load_texture(grf, &format!("{UI}btn_{name}{suffix}.bmp"), false));
        let pieces = |color: &str| ["left", "mid", "right"].map(|part| basic(&format!("gze{color}_{part}")));
        Self {
            background: gpu.try_load_texture(grf, &format!("{UI}basic_interface\\homuninfo_bg.bmp"), false).ok(),
            track: basic("gze_bg1"),
            fills: [pieces("blue"), pieces("red")],
            skill: button("skill"),
            dismiss: button("fired"),
        }
    }
}

/// `text` with its right edge at `right`, `top` below the background's top.
pub(crate) fn right_text(font: &Font, frame: &Frame, text: String, right: f32, top: f32) -> (String, [f32; 2], [u8; 3]) {
    let x = (right - font.width(&text)).floor();
    (text, frame.at([x, BACKGROUND_TOP + top]), TEXT)
}

/// The ASPD shown for an attack motion (ms).
pub(crate) fn aspd(amotion: u16) -> i32 {
    (2000 - i32::from(amotion)) / 10
}

/// Atk, Matk, Hit, Critical, Def, Mdef, Flee and Aspd beside the background's labels.
pub(crate) fn draw_values(layer: &mut Layer, frame: &Frame, font: &Font, values: [u16; 7], amotion: u16) {
    let aspd = aspd(amotion);
    let values = values.map(|value| value.to_string()).into_iter().chain([aspd.to_string()]);
    for (row, value) in values.enumerate() {
        layer.texts.push(right_text(font, frame, value, VALUE_RIGHT, FIRST_ROW + row as f32 * ROW));
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Close,
    Skills,
    Dismiss,
    /// Anywhere else: the window drags from it.
    Window,
}

pub struct Window {
    pub frame: Frame,
    /// The contract's end as the window writes it, worked out when the mercenary's status comes.
    pub contract: String,
}

impl Default for Window {
    fn default() -> Self {
        Self { frame: Frame::new(POSITION, WIDTH), contract: String::new() }
    }
}

impl Window {
    pub fn height() -> f32 {
        HEIGHT
    }

    /// The buttons on screen (for the test driver).
    pub fn button_boxes(&self) -> Vec<(&'static str, [f32; 4])> {
        if !self.frame.open {
            return Vec::new();
        }
        let place = |at: [f32; 2]| {
            let [x, y] = self.frame.at(at);
            [x, y, BUTTON[0], BUTTON[1]]
        };
        vec![("skill", place(SKILLS)), ("dismiss", place(DISMISS))]
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let local = self.frame.local(cursor, HEIGHT)?;
        Some(if self.frame.title_hit(local) == Some(TitleHit::Close) {
            Hit::Close
        } else if contains(SKILLS, BUTTON, local) {
            Hit::Skills
        } else if contains(DISMISS, BUTTON, local) {
            Hit::Dismiss
        } else {
            Hit::Window
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &'a self,
        layer: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        font: &Font,
        messages: &[String],
        mercenary: &Mercenary,
        job: u16,
        pointer: ([f32; 2], bool),
    ) {
        if !self.frame.open {
            return;
        }
        let frame = &self.frame;
        let at = |local: [f32; 2]| frame.at(local);
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        let m = mercenary;
        match &assets.background {
            Some(background) => layer.sprites.push((background, at([0.0, 0.0]), background.size)),
            None => {
                chrome.draw_title_bar(layer, at([0.0, 0.0]), WIDTH);
                layer.sprites.push((&chrome.white, at([0.0, TITLE_HEIGHT]), [WIDTH, HEIGHT - TITLE_HEIGHT]));
            }
        }
        frame.draw_bare_title(layer, chrome, message(title(job)), pointer.0);
        draw_values(layer, frame, font, [m.atk, m.matk, m.hit, m.critical, m.def, m.mdef, m.flee], m.aspd);
        // The name over a grey copy a pixel right, the level.
        layer.texts.push((message(NAME).to_owned(), at([LABEL, NAME_TOP]), TEXT));
        layer.texts.push((m.name.clone(), at([VALUE + 1.0, NAME_TOP]), NAME_SHADOW));
        layer.texts.push((m.name.clone(), at([VALUE, NAME_TOP]), TEXT));
        layer.texts.push((message(LEVEL).to_owned(), at([LABEL, LEVEL_TOP]), TEXT));
        layer.texts.push((m.level.to_string(), at([VALUE, LEVEL_TOP]), TEXT));
        let hit = self.hit(pointer.0);
        let look = |target: Hit| {
            let hovered = hit == Some(target);
            usize::from(hovered) + usize::from(hovered && pointer.1)
        };
        layer.sprites.push((&assets.dismiss[look(Hit::Dismiss)], at(DISMISS), BUTTON));
        layer.sprites.push((&assets.skill[look(Hit::Skills)], at(SKILLS), BUTTON));
        // HP and SP: the bar with its numbers inside, "now / most" after it.
        for (place, (value, most)) in [(HP_BAR, (m.hp, m.max_hp)), (SP_BAR, (m.sp, m.max_sp))] {
            let gauge = gauge(value, most, BAR[0]);
            if gauge != Gauge::Hidden {
                layer.sprites.push((&assets.track, at(place), BAR));
            }
            if let Gauge::Filled(width, red) = gauge {
                let [left, middle, end] = &assets.fills[usize::from(red)];
                layer.sprites.push((left, at(place), [FILL_END, BAR[1]]));
                layer.sprites.push((middle, at([place[0] + FILL_END, place[1]]), [width, BAR[1]]));
                layer.sprites.push((end, at([place[0] + FILL_END + width, place[1]]), [end.size[0], BAR[1]]));
                // ponytail: centred on the bar.
                let numbers = gauge_text(value, most);
                let x = place[0] + ((BAR[0] - font.width(&numbers)) / 2.0).floor();
                layer.texts.push((numbers, at([x, place[1] - 2.0]), TEXT));
            }
            layer.texts.push((format!("{value} / {most}"), at([NUMBERS_X, place[1] - NUMBERS_UP]), TEXT));
        }
        // The contract's end in red; loyalty, summons and kills.
        layer.texts.push((message(EXPIRATION).to_owned(), at([LABEL, EXPIRATION_TOP]), TEXT));
        layer.texts.push((self.contract.clone(), at([CONTRACT_X, EXPIRATION_TOP]), CONTRACT_COLOR));
        for ((label, value), (label_at, value_at)) in [(LOYALTY, u32::from(m.loyalty)), (SUMMONS, m.summons), (KILLS, m.kills)].into_iter().zip(LOWER) {
            layer.texts.push((message(label).to_owned(), at(label_at), TEXT));
            layer.texts.push((value.to_string(), at(value_at), TEXT));
        }
    }
}

// ponytail: the official client's own follow rule isn't in data.grf: more than 3 cells away, at most twice a second.
const FOLLOW_DISTANCE: i32 = 3;
const FOLLOW_EVERY: Duration = Duration::from_millis(500);

/// What the player last told the mercenary to do; following waits while it's at it.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Order {
    /// Attacking this unit, until it's gone.
    Attack(u32),
    /// Going to a cell, until the player (who stood here) moves.
    Move([i32; 2]),
}

/// How the client steers the mercenary.
pub struct Steering {
    pub following: bool,
    pub order: Option<Order>,
    last_call: Option<Instant>,
}

impl Default for Steering {
    fn default() -> Self {
        Self { following: true, order: None, last_call: None }
    }
}

impl Steering {
    /// Whether to call the mercenary back to the player now, it being at `mercenary` and the player at `player`.
    pub fn should_call(&mut self, mercenary: [i32; 2], player: [i32; 2], now: Instant) -> bool {
        if let Some(Order::Move(from)) = self.order
            && from != player
        {
            self.order = None;
        }
        let far = (mercenary[0] - player[0]).abs().max((mercenary[1] - player[1]).abs()) > FOLLOW_DISTANCE;
        let rested = self.last_call.is_none_or(|last| now.duration_since(last) >= FOLLOW_EVERY);
        let call = self.following && self.order.is_none() && far && rested;
        if call {
            self.last_call = Some(now);
        }
        call
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_by_job() {
        assert_eq!([aspd(500), aspd(505)], [150, 149]);
        assert_eq!([6017, 6026, 6027, 6036, 6037, 6046, 6047, 1002].map(title), [1181, 1181, 1183, 1183, 1182, 1182, 1367, 1367]);
    }

    #[test]
    fn lays_the_window_out() {
        let mut window = Window::default();
        window.frame.open = true;
        let boxes = window.button_boxes();
        // At (150, 100): Dismiss (337, 144), Skills (382, 144); only the close button on the title bar.
        assert_eq!(boxes.iter().map(|(name, [x, y, ..])| (*name, [*x, *y])).collect::<Vec<_>>(), [("skill", [382.0, 144.0]), ("dismiss", [337.0, 144.0])]);
        assert_eq!(window.hit([383.0, 145.0]), Some(Hit::Skills));
        assert_eq!(window.hit([338.0, 145.0]), Some(Hit::Dismiss));
        assert_eq!(window.hit([420.0, 108.0]), Some(Hit::Close));
        assert_eq!(window.hit([400.0, 108.0]), Some(Hit::Window));
        assert_eq!(window.hit([200.0, 279.0]), Some(Hit::Window));
        assert_eq!(window.hit([200.0, 280.0]), None);
        // An 85-px bar: SP red under a quarter, as HP.
        assert_eq!([gauge(25, 100, BAR[0]), gauge(24, 100, BAR[0])], [Gauge::Filled(19.0, false), Gauge::Filled(18.0, true)]);
    }

    #[test]
    fn follows_unless_told_otherwise() {
        let mut steering = Steering::default();
        let now = Instant::now();
        assert!(!steering.should_call([10, 10], [12, 13], now), "within 3 cells");
        assert!(steering.should_call([10, 10], [14, 10], now));
        assert!(!steering.should_call([10, 10], [14, 10], now + Duration::from_millis(100)), "not twice in half a second");
        assert!(steering.should_call([10, 10], [14, 10], now + Duration::from_millis(600)));
        steering.order = Some(Order::Attack(5));
        assert!(!steering.should_call([10, 10], [20, 10], now + Duration::from_secs(2)), "attacking");
        steering.order = Some(Order::Move([14, 10]));
        assert!(!steering.should_call([30, 10], [14, 10], now + Duration::from_secs(3)), "sent somewhere");
        assert!(steering.should_call([30, 10], [15, 10], now + Duration::from_secs(3)), "the player walked on");
        assert_eq!(steering.order, None);
        steering.following = false;
        assert!(!steering.should_call([10, 10], [20, 10], now + Duration::from_secs(4)), "Ctrl+T");
    }
}
