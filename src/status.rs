//! The status window (Alt+A): the six stats with their bonuses, the arrows that raise them and what that costs, and
//! Atk, Matk, Hit, Critical, Def, Mdef, Flee, ASPD, status points and guild; under them the 4th jobs' trait panel. The
//! labels are the backgrounds' own.

use std::collections::HashMap;

use rustnarok_client::grf::Grf;
use rustnarok_client::net::{STATUS_ATK, STATUS_BONUS, STATUS_POINTS, STATUS_STR, STATUS_STR_COST};
use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};
use crate::window::{Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI, contains, offset};

const WIDTH: f32 = 280.0;
/// The window's height, title bar and background, with the trait panel shut and open.
const SHUT_HEIGHT: f32 = 141.0;
const OPEN_HEIGHT: f32 = 241.0;
const ROW: f32 = 16.0;
/// Str to Luk: text tops from y 23, the value at x 36, the bonus at x 49 (from a value of 100 at x 54, 2 lower), the
/// raise arrow at (78, 24 + 16 a row), what raising costs right-aligned to x 101.
const STAT_TOP: f32 = 23.0;
const VALUE: f32 = 36.0;
const BONUS: f32 = 49.0;
const BONUS_FROM_100: [f32; 2] = [54.0, 2.0];
const ARROW: f32 = 78.0;
const ARROW_BOX: [f32; 2] = [11.0, 11.0];
const COST_RIGHT: f32 = 101.0;
/// Atk to Critical and Def to the guild: right-aligned, text tops from y 22.
const DERIVED_TOP: f32 = 22.0;
const LEFT_COLUMN_RIGHT: f32 = 189.0;
const RIGHT_COLUMN_RIGHT: f32 = 275.0;
const POSITION: [f32; 2] = [185.0, 300.0];
const TITLE_MESSAGE: usize = 105;
const TEXT: [u8; 3] = [0, 0, 0];
/// The 4th jobs' traits (rathena's SP_POW to SP_CRT), what raising each costs (SP_UPOW on), the trait points
/// (SP_TRAITPOINT), and the stats they make (SP_PATK to SP_CRATE).
pub const TRAIT_POW: u16 = 219;
const TRAIT_COST: u16 = 247;
const TRAIT_POINTS: u16 = 231;
const TRAIT_STATS: u16 = 225;
const TRAITS: [&str; 6] = ["POW", "STA", "WIS", "SPL", "CON", "CRT"];
/// "Trait Status": the bar that opens and shuts the panel, at (2, 121).
const TRAIT_STATUS: usize = 3613;
const BAR: [f32; 2] = [2.0, 121.0];
const BAR_SIZE: [f32; 2] = [276.0, 18.0];
const BAR_TEXT: [f32; 2] = [22.0, 122.0];
/// Pow to Crt in the panel: text tops from y 144, the "+" at (78, 145 + 16 a row).
const TRAIT_ROWS: usize = 6;
const TRAIT_TOP: f32 = 144.0;
/// P.ATK, S.MATK, RES, MRES, H.PLUS, C.RATE and the trait points (225 to 231): right edge and text top.
const TRAIT_STAT_PLACES: [[f32; 2]; 7] = [[189.0, 143.0], [189.0, 159.0], [275.0, 143.0], [275.0, 159.0], [189.0, 175.0], [189.0, 191.0], [275.0, 207.0]];
const RES: u16 = 227;
const MRES: u16 = 228;

pub struct Assets {
    /// The background, with the panel shut and open.
    backgrounds: [Texture; 2],
    /// The arrow (the same pressed).
    arrow: Texture,
    /// The Trait Status bar: shut, shut pressed, open, open pressed.
    bar: [Texture; 4],
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |name: &str| gpu.load_texture(grf, &format!("{UI}{name}.bmp"), false);
        Self {
            backgrounds: [image("statuswnd\\w_statwin_bg"), image("statuswnd\\w_ex_statwin_bg")],
            arrow: image("basic_interface\\arw_right"),
            bar: ["expand_on_normal", "expand_on_press", "expand_off_normal", "expand_off_press"].map(|name| image(&format!("statuswnd\\{name}"))),
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(TitleHit),
    /// The arrow raising a stat (STATUS_STR and on).
    Raise(u16),
    /// The Trait Status bar: the panel open or shut.
    Traits,
    /// The "+" raising a trait (TRAIT_POW and on).
    RaiseTrait(u16),
    Window,
}

pub struct StatusWindow {
    pub frame: Frame,
    /// A raise (a stat's or a trait's) waits for the server's answer: until it comes, clicks ask nothing.
    pub waiting: bool,
    /// The trait panel under the stats is open.
    pub traits_open: bool,
}

impl Default for StatusWindow {
    /// Open for every job the first time; the choice isn't saved.
    // ponytail: a new login makes a new window, so the panel opens again then, not only once per run.
    fn default() -> Self {
        Self { frame: Frame::new(POSITION, WIDTH).collapsing(), waiting: false, traits_open: true }
    }
}

/// Whether a trait's "+" shows: a trait point or more and a cost that isn't 0 (not compared with the points).
pub fn can_raise_trait(status: &HashMap<u16, i64>, trait_id: u16) -> bool {
    let cost = status.get(&(TRAIT_COST + trait_id - TRAIT_POW)).copied().unwrap_or(0);
    cost != 0 && status.get(&TRAIT_POINTS).copied().unwrap_or(0) >= 1
}

/// Whether a click on a trait's "+" asks: trait points and a cost above 0.
fn trait_raise_sends(status: &HashMap<u16, i64>, trait_id: u16) -> bool {
    let cost = status.get(&(TRAIT_COST + trait_id - TRAIT_POW)).copied().unwrap_or(0);
    cost > 0 && status.get(&TRAIT_POINTS).copied().unwrap_or(0) > 0
}

/// The labels' areas (80 × 12, window-local corners) and their messages: the stats and what they make, and with the
/// trait panel open the traits and theirs. Resting on one shows its message 16 left and 13 up of the corner.
const LABEL_SIZE: [f32; 2] = [80.0, 12.0];
const LABEL_TIP: [f32; 2] = [-16.0, -13.0];
const STAT_LABELS: [([f32; 2], usize); 16] = [
    ([6.0, 25.0], 3112),
    ([6.0, 41.0], 3113),
    ([6.0, 57.0], 3114),
    ([6.0, 73.0], 3115),
    ([6.0, 89.0], 3116),
    ([6.0, 105.0], 3117),
    ([108.0, 24.0], 3118),
    ([108.0, 40.0], 3124),
    ([108.0, 56.0], 3120),
    ([108.0, 72.0], 3121),
    ([108.0, 88.0], 3123),
    ([108.0, 104.0], 3122),
    ([194.0, 24.0], 3119),
    ([194.0, 40.0], 3125),
    ([194.0, 56.0], 3126),
    ([194.0, 72.0], 3127),
];
const TRAIT_LABELS: [([f32; 2], usize); 13] = [
    ([6.0, 146.0], 3614),
    ([6.0, 162.0], 3615),
    ([6.0, 178.0], 3616),
    ([6.0, 194.0], 3617),
    ([6.0, 210.0], 3618),
    ([6.0, 226.0], 3619),
    ([108.0, 145.0], 3620),
    ([108.0, 161.0], 3621),
    ([108.0, 177.0], 3624),
    ([108.0, 193.0], 3625),
    ([108.0, 209.0], 3626),
    ([194.0, 145.0], 3622),
    ([194.0, 161.0], 3623),
];

/// The label under a window-local point: where its tooltip goes and its message.
fn label_at(local: [f32; 2], traits_open: bool) -> Option<([f32; 2], usize)> {
    let panel: &[_] = if traits_open { &TRAIT_LABELS } else { &[] };
    STAT_LABELS.iter().chain(panel).find(|(corner, _)| contains(*corner, LABEL_SIZE, local)).map(|&(corner, id)| (offset(corner, LABEL_TIP), id))
}

/// Where a trait's "+" sits in the window (by row, POW first).
fn trait_plus(row: u16) -> [f32; 2] {
    [ARROW, TRAIT_TOP + 1.0 + f32::from(row) * ROW]
}

/// The six stats' short names, Str to Luk.
pub const STATS: [&str; 6] = ["str", "agi", "vit", "int", "dex", "luk"];

/// Where the arrow raising a stat (by row, Str first) sits in the window.
fn arrow_corner(row: u16) -> [f32; 2] {
    [ARROW, STAT_TOP + 1.0 + f32::from(row) * ROW]
}

/// A value with its bonus the way the window writes it: "12 + 3".
fn pair(base: i64, bonus: i64) -> String {
    format!("{base} + {bonus}")
}

/// A bonus and where it goes on a row with text top `top`: "+3" or "-3" at x 49, or from a value of 100 at x 54, 2
/// lower; nothing for 0.
fn bonus_text(base: i64, bonus: i64, top: f32) -> Option<(String, [f32; 2])> {
    let text = if bonus > 0 { format!("+{bonus}") } else { bonus.to_string() };
    let place = if base >= 100 { [BONUS_FROM_100[0], top + BONUS_FROM_100[1]] } else { [BONUS, top] };
    (bonus != 0).then_some((text, place))
}

/// A trait's bonus: only above 0.
fn trait_bonus_text(base: i64, bonus: i64, top: f32) -> Option<(String, [f32; 2])> {
    bonus_text(base, bonus.max(0), top)
}

/// A trait stat as shown: RES and MRES never below 0.
fn trait_stat(status: &HashMap<u16, i64>, id: u16) -> i64 {
    let value = status.get(&id).copied().unwrap_or(0);
    if matches!(id, RES | MRES) { value.max(0) } else { value }
}

/// What a stat's arrow does: raise it, when there are points enough.
fn can_raise(status: &HashMap<u16, i64>, stat: u16) -> bool {
    let cost = status.get(&(STATUS_STR_COST + stat - STATUS_STR)).copied().unwrap_or(0);
    cost > 0 && cost <= status.get(&STATUS_POINTS).copied().unwrap_or(0)
}

/// The Trait Status bar's picture: shut or open, pressed or not.
fn bar_image(open: bool, pressed: bool) -> usize {
    usize::from(open) * 2 + usize::from(pressed)
}

impl StatusWindow {
    pub fn height(&self) -> f32 {
        if self.traits_open { OPEN_HEIGHT } else { SHUT_HEIGHT }
    }

    pub fn hit(&self, cursor: [f32; 2], status: &HashMap<u16, i64>) -> Option<Hit> {
        let local = self.frame.local(cursor, self.height())?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(Hit::Title(hit));
        }
        if contains(BAR, BAR_SIZE, local) {
            return Some(Hit::Traits);
        }
        if self.traits_open {
            for row in 0..TRAIT_ROWS as u16 {
                if trait_raise_sends(status, TRAIT_POW + row) && contains(trait_plus(row), ARROW_BOX, local) {
                    return Some(Hit::RaiseTrait(TRAIT_POW + row));
                }
            }
        }
        for row in 0..6u16 {
            let stat = STATUS_STR + row;
            if can_raise(status, stat) && contains(arrow_corner(row), ARROW_BOX, local) {
                return Some(Hit::Raise(stat));
            }
        }
        Some(Hit::Window)
    }

    /// The arrows on screen that raise a stat now (for the test driver), named `raise_str` to `raise_luk`.
    pub fn raise_boxes(&self, status: &HashMap<u16, i64>) -> Vec<(String, [f32; 4])> {
        if !self.frame.open || self.frame.minimized {
            return Vec::new();
        }
        (0..6u16)
            .filter(|&row| can_raise(status, STATUS_STR + row))
            .map(|row| {
                let [x, y] = self.frame.at(arrow_corner(row));
                (format!("raise_{}", STATS[usize::from(row)]), [x, y, ARROW_BOX[0], ARROW_BOX[1]])
            })
            .collect()
    }

    /// The "+" boxes on screen that raise a trait now (for the test driver), named `raise_pow` to `raise_crt`, and the
    /// Trait Status bar (`traits`).
    pub fn trait_boxes(&self, status: &HashMap<u16, i64>) -> Vec<(String, [f32; 4])> {
        if !self.frame.open || self.frame.minimized {
            return Vec::new();
        }
        let [x, y] = self.frame.at(BAR);
        let mut boxes = vec![("traits".to_owned(), [x, y, BAR_SIZE[0], BAR_SIZE[1]])];
        if self.traits_open {
            boxes.extend((0..TRAIT_ROWS as u16).filter(|&row| can_raise_trait(status, TRAIT_POW + row)).map(|row| {
                let [x, y] = self.frame.at(trait_plus(row));
                (format!("raise_{}", TRAITS[usize::from(row)].to_lowercase()), [x, y, ARROW_BOX[0], ARROW_BOX[1]])
            }));
        }
        boxes
    }

    /// A click: a stat to raise (by status id), or a trait to raise, unless a raise still waits for its answer.
    pub fn click(&mut self, hit: Hit) -> Option<u16> {
        match hit {
            Hit::Title(title) => self.frame.click_title(title),
            Hit::Raise(_) | Hit::RaiseTrait(_) if self.waiting => {}
            Hit::Raise(stat) | Hit::RaiseTrait(stat) => {
                self.waiting = true;
                return Some(stat);
            }
            Hit::Traits => self.traits_open = !self.traits_open,
            Hit::Window => {}
        }
        None
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &self,
        layer: &mut Layer<'a>,
        tooltip: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        font: &Font,
        messages: &[String],
        status: &HashMap<u16, i64>,
        guild: &str,
        pointer: ([f32; 2], bool),
    ) {
        if !self.frame.open {
            return;
        }
        let at = |local: [f32; 2]| self.frame.at(local);
        let value = |id: u16| status.get(&id).copied().unwrap_or(0);
        self.frame.draw_title(layer, chrome, messages.get(TITLE_MESSAGE).map_or("", String::as_str), pointer.0);
        if self.frame.minimized {
            return;
        }
        let background = &assets.backgrounds[usize::from(self.traits_open)];
        layer.sprites.push((background, at([0.0, TITLE_HEIGHT]), background.size));
        let hit = self.hit(pointer.0, status);
        // A label's description where the pointer rests on it: split at its "\n", in its colours.
        // ponytail: it shows at once, like this client's other tooltips.
        if let Some((corner, id)) = self.frame.local(pointer.0, self.height()).and_then(|local| label_at(local, self.traits_open)) {
            let mut color = [255; 3];
            let lines: Vec<_> = messages
                .get(id)
                .map_or("", String::as_str)
                .split("\\n")
                .flat_map(|line| crate::wrap_plain(font, line, f32::INFINITY, 0.0, &mut color, false))
                .collect();
            crate::window::tooltip_lines(tooltip, chrome, font, &lines, at(corner));
        }
        let right = |layer: &mut Layer<'a>, text: String, [edge, top]: [f32; 2]| {
            layer.texts.push((text.clone(), at([edge - font.width(&text), top]), TEXT));
        };

        // Str to Luk: the value, the bonus from gear, the arrow while points last, and what going up costs.
        for row in 0..6u16 {
            let stat = STATUS_STR + row;
            let top = STAT_TOP + f32::from(row) * ROW;
            layer.texts.push((value(stat).to_string(), at([VALUE, top]), TEXT));
            if let Some((text, place)) = bonus_text(value(stat), value(stat + STATUS_BONUS), top) {
                layer.texts.push((text, at(place), TEXT));
            }
            if can_raise(status, stat) {
                layer.sprites.push((&assets.arrow, at(arrow_corner(row)), assets.arrow.size));
            }
            right(layer, value(STATUS_STR_COST + row).to_string(), [COST_RIGHT, top]);
        }

        // Atk (base + gear), Matk, Hit, Critical on the left; Def, Mdef, Flee, ASPD on the right; then points, guild.
        let [atk, atk2, matk1, matk2, def, def2, mdef, mdef2, hit_rate, flee, flee2, critical, aspd] =
            std::array::from_fn(|place| value(STATUS_ATK + place as u16));
        let derived = |row: usize| DERIVED_TOP + row as f32 * ROW;
        for (row, text) in [pair(atk, atk2), pair(matk2, matk1), hit_rate.to_string(), critical.to_string()].into_iter().enumerate() {
            right(layer, text, [LEFT_COLUMN_RIGHT, derived(row)]);
        }
        let aspd = (200.0 - aspd as f32 / 10.0).floor();
        let right_column = [pair(def, def2), pair(mdef, mdef2), pair(flee, flee2), aspd.to_string(), value(STATUS_POINTS).to_string(), guild.to_owned()];
        for (row, text) in right_column.into_iter().enumerate() {
            right(layer, text, [RIGHT_COLUMN_RIGHT, derived(row)]);
        }
        // The Trait Status bar, and the panel when it's open.
        let bar = &assets.bar[bar_image(self.traits_open, pointer.1 && hit == Some(Hit::Traits))];
        layer.sprites.push((bar, at(BAR), bar.size));
        if let Some(text) = messages.get(TRAIT_STATUS) {
            layer.texts.push((text.clone(), at(BAR_TEXT), TEXT));
        }
        if !self.traits_open {
            return;
        }
        for row in 0..TRAIT_ROWS as u16 {
            let trait_id = TRAIT_POW + row;
            let top = TRAIT_TOP + f32::from(row) * ROW;
            layer.texts.push((value(trait_id).to_string(), at([VALUE, top]), TEXT));
            if let Some((text, place)) = trait_bonus_text(value(trait_id), value(trait_id + STATUS_BONUS), top) {
                layer.texts.push((text, at(place), TEXT));
            }
            if can_raise_trait(status, trait_id) {
                layer.sprites.push((&assets.arrow, at(trait_plus(row)), assets.arrow.size));
            }
            right(layer, value(TRAIT_COST + row).to_string(), [COST_RIGHT, top]);
        }
        for (id, place) in (TRAIT_STATS..).zip(TRAIT_STAT_PLACES) {
            right(layer, trait_stat(status, id).to_string(), place);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shows_a_traits_plus_with_any_trait_point() {
        // POW costs 2 and STA 4, with 3 trait points: both "+" show (the cost isn't compared); one with no cost doesn't.
        let status = HashMap::from([(TRAIT_POINTS, 3), (TRAIT_COST, 2), (TRAIT_COST + 1, 4)]);
        assert!(can_raise_trait(&status, TRAIT_POW) && can_raise_trait(&status, TRAIT_POW + 1) && !can_raise_trait(&status, TRAIT_POW + 2));
        assert!(trait_raise_sends(&status, TRAIT_POW + 1));
        // No trait points: none shows, none asks.
        let none = HashMap::from([(TRAIT_POINTS, 0), (TRAIT_COST, 2)]);
        assert!(!can_raise_trait(&none, TRAIT_POW) && !trait_raise_sends(&none, TRAIT_POW));
    }

    #[test]
    fn waits_for_each_raises_answer() {
        let mut window = StatusWindow::default();
        assert_eq!(window.click(Hit::Raise(STATUS_STR)), Some(STATUS_STR));
        // A second before the answer asks nothing, a trait's neither.
        assert_eq!(window.click(Hit::Raise(STATUS_STR)), None);
        assert_eq!(window.click(Hit::RaiseTrait(TRAIT_POW)), None);
        // The answer, taken or refused, lets the next go.
        window.waiting = false;
        assert_eq!(window.click(Hit::RaiseTrait(TRAIT_POW)), Some(TRAIT_POW));
        assert!(window.waiting);
    }

    #[test]
    fn finds_the_label_under_the_pointer() {
        // Str's area from (6, 25): its message 16 left and 13 up of the corner.
        assert_eq!(label_at([7.0, 26.0], true), Some(([-10.0, 12.0], 3112)));
        assert_eq!(label_at([85.0, 36.0], true), Some(([-10.0, 12.0], 3112)));
        assert_eq!(label_at([86.0, 26.0], true), None);
        assert_eq!(label_at([200.0, 75.0], false), Some(([178.0, 59.0], 3127)));
        // Res only while the panel is open.
        assert_eq!(label_at([200.0, 150.0], true), Some(([178.0, 132.0], 3622)));
        assert_eq!(label_at([200.0, 150.0], false), None);
    }

    #[test]
    fn raises_only_what_points_pay_for() {
        let status = HashMap::from([(STATUS_POINTS, 3), (STATUS_STR_COST, 2), (STATUS_STR_COST + 1, 4)]);
        assert!(can_raise(&status, STATUS_STR));
        assert!(!can_raise(&status, STATUS_STR + 1));
        assert_eq!(pair(10, 3), "10 + 3");
        assert_eq!(pair(10, -2), "10 + -2");
    }

    #[test]
    fn names_the_arrows_points_pay_for() {
        // 5 points: Int (cost 4) and Luk (cost 2) can rise, Str (cost 9) can't, and Agi, Vit and Dex have no cost known.
        let status = HashMap::from([(STATUS_POINTS, 5), (STATUS_STR_COST, 9), (STATUS_STR_COST + 3, 4), (STATUS_STR_COST + 5, 2)]);
        let mut window = StatusWindow::default();
        assert!(window.raise_boxes(&status).is_empty());
        window.frame.open = true;
        let [x, y] = POSITION;
        assert_eq!(
            window.raise_boxes(&status),
            [("raise_int".to_owned(), [x + 78.0, y + 24.0 + 48.0, 11.0, 11.0]), ("raise_luk".to_owned(), [x + 78.0, y + 24.0 + 80.0, 11.0, 11.0])]
        );
        window.frame.minimized = true;
        assert!(window.raise_boxes(&status).is_empty());
    }

    #[test]
    fn opens_with_the_trait_panel_at_the_games_place() {
        let mut window = StatusWindow::default();
        window.frame.open = true;
        assert_eq!(POSITION, [185.0, 300.0]);
        assert!(window.traits_open);
        assert_eq!(window.height(), 241.0);
        // POW costs 2 of 10 points: its "+" at (78, 145); the bar at (2, 121).
        let status = HashMap::from([(TRAIT_POINTS, 10), (TRAIT_COST, 2), (TRAIT_COST + 5, 3)]);
        let [x, y] = POSITION;
        assert_eq!(
            window.trait_boxes(&status),
            [
                ("traits".to_owned(), [x + 2.0, y + 121.0, 276.0, 18.0]),
                ("raise_pow".to_owned(), [x + 78.0, y + 145.0, 11.0, 11.0]),
                ("raise_crt".to_owned(), [x + 78.0, y + 145.0 + 80.0, 11.0, 11.0])
            ]
        );
        assert_eq!(window.hit([x + 80.0, y + 147.0], &status), Some(Hit::RaiseTrait(TRAIT_POW)));
        // The bar shuts it: 141 tall, the "+" gone.
        let bar = window.hit([x + 100.0, y + 130.0], &status).unwrap();
        assert_eq!(window.click(bar), None);
        assert_eq!(window.height(), 141.0);
        assert_eq!(window.trait_boxes(&status).len(), 1);
        // The bar's picture: expand_on while shut, expand_off while open, each with its pressed one.
        assert_eq!([bar_image(false, false), bar_image(false, true), bar_image(true, false), bar_image(true, true)], [0, 1, 2, 3]);
    }

    #[test]
    fn writes_bonuses_at_their_place() {
        assert_eq!(bonus_text(99, 10, 23.0), Some(("+10".to_owned(), [49.0, 23.0])));
        assert_eq!(bonus_text(100, 10, 23.0), Some(("+10".to_owned(), [54.0, 25.0])));
        assert_eq!(bonus_text(50, -3, 39.0), Some(("-3".to_owned(), [49.0, 39.0])));
        assert_eq!(bonus_text(50, 0, 23.0), None);
        // A trait's bonus shows only above 0; RES and MRES never below 0.
        assert_eq!(trait_bonus_text(5, -2, 144.0), None);
        assert_eq!(trait_bonus_text(5, 0, 144.0), None);
        assert_eq!(trait_bonus_text(100, 2, 144.0), Some(("+2".to_owned(), [54.0, 146.0])));
        let status = HashMap::from([(RES, -5), (MRES, -1), (TRAIT_STATS, -5)]);
        assert_eq!([trait_stat(&status, RES), trait_stat(&status, MRES), trait_stat(&status, TRAIT_STATS)], [0, 0, -5]);
    }
}
