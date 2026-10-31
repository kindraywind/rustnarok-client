//! The basic information window (Alt+V): name, job, HP and SP, levels and experience, weight and zeny, and the menu
//! of the other windows under it.

use std::collections::HashMap;

use rustnarok_client::grf::Grf;
use rustnarok_client::net;
use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};
use crate::window::{Chrome, Frame, TITLE_HEIGHT};
use crate::shortcuts::{Action, Shortcuts};
use crate::{Layer, UI, contains, offset};

const WIDTH: f32 = 220.0;
const BASE_BUTTON: [f32; 2] = [3.0, 3.0];
const BUTTON_SIZE: [f32; 2] = [11.0, 11.0];
/// The title: white at (18, 3) under black at (17, 2); the only text with a shadow.
const TITLE: [f32; 2] = [17.0, 2.0];
/// The minimised window: the name in its title bar, and under the bar a grey box with 3 px cut corners round white.
const MINI_NAME: [f32; 2] = [17.0, 3.0];
const MINI_BOX_TOP: f32 = 17.0;
const MINI_EDGE: [u8; 4] = [200, 200, 200, 0xFF];
/// HP and SP bars: 135 by 9, filled 127 px at most.
const BAR_SIZE: [f32; 2] = [135.0, 9.0];
const BAR_FULL: f32 = 127.0;
const EXP_SIZE: [f32; 2] = [112.0, 6.0];
const EXP_INNER: f32 = 110.0;
/// The minimised lines' left margin (and the right one), and the least a centred short line keeps either side.
const MINI_MARGIN: f32 = 9.0;
const MINI_MIN_MARGIN: f32 = 6.0;
/// Margins for the weight and zeny line, and the gap between them.
const WEIGHT_MARGIN: f32 = 3.0;
const ZENY_MARGIN: f32 = 5.0;
const WEIGHT_GAP: f32 = 10.0;
/// The menu strip under the basic info, joined to its bottom edge: the toggle bar (219 by 9) there, the menu's back 9
/// down, and its buttons (32 by 34) from (9, 19), 42 and 43 apart, five to a row.
const TOGGLE_AT: f32 = 0.0;
const TOGGLE_HEIGHT: f32 = 9.0;
const MENU_AT: f32 = 9.0;
const MENU_BUTTONS: [f32; 2] = [9.0, 19.0];
const MENU_STEP: [f32; 2] = [42.0, 43.0];
const MENU_BUTTON_SIZE: [f32; 2] = [32.0, 34.0];
const MENU_COLUMNS: usize = 5;
const BASIC_INFO_MESSAGE: usize = 238;
const EXP_BORDER: [u8; 4] = [0xAF, 0xAF, 0xAF, 0xFF];
const EXP_FILL: [u8; 4] = [0x56, 0x7F, 0xCF, 0xFF];
const RED: [u8; 3] = [0xFF, 0x00, 0x00];
const BLACK: [u8; 3] = [0, 0, 0];
/// rathena's overweight point when the server hasn't said (`natural_heal_weight_rate`).
pub const OVERWEIGHT: u32 = 50;

/// 4th jobs keep the taller window.
pub fn fourth_job(job: u16) -> bool {
    matches!(job, 4252..=4299 | 4302..=4329 | 4355 | 4360)
}

/// Where things go in the window: the game's 134-high one, or 4th jobs' 150-high one.
#[derive(Debug, PartialEq)]
pub struct Layout {
    pub height: f32,
    pub mini: f32,
    pub name: [f32; 2],
    pub job: [f32; 2],
    pub labels: [[f32; 2]; 2],
    pub bars: [[f32; 2]; 2],
    /// The percent's left edge (None: right-aligned at the window's edge less 20).
    pub percent: Option<f32>,
    pub levels: [[f32; 2]; 2],
    pub exp_bars: [[f32; 2]; 2],
    pub extra: f32,
    /// 4th jobs: the AP row's label and bar, and the three labels' messages (HP, SP, AP).
    pub ap: Option<([f32; 2], [f32; 2])>,
    pub label_messages: Option<[usize; 3]>,
}

const NORMAL: Layout = Layout {
    height: 134.0,
    mini: 55.0,
    name: [9.0, 22.0],
    job: [9.0, 36.0],
    labels: [[17.0, 51.0], [17.0, 65.0]],
    bars: [[35.0, 53.0], [35.0, 68.0]],
    percent: Some(173.0),
    levels: [[17.0, 84.0], [17.0, 96.0]],
    exp_bars: [[85.0, 89.0], [85.0, 101.0]],
    extra: 116.0,
    ap: None,
    label_messages: None,
};

// ponytail: the x places the 4th jobs' window doesn't give (levels, exp bars, the percentages) and the name and job
// follow the 134-tall window's.
const FOURTH: Layout = Layout {
    height: 149.0,
    mini: 71.0,
    name: [9.0, 22.0],
    job: [9.0, 36.0],
    labels: [[9.0, 51.0], [9.0, 65.0]],
    bars: [[35.0, 53.0], [35.0, 68.0]],
    percent: Some(173.0),
    levels: [[17.0, 99.0], [17.0, 111.0]],
    exp_bars: [[85.0, 104.0], [85.0, 116.0]],
    extra: 131.0,
    ap: Some(([9.0, 80.0], [35.0, 83.0])),
    label_messages: Some([3068, 3069, 3628]),
};
/// AP and its most (rathena's SP_AP, SP_MAXAP).
const STATUS_AP: u16 = 232;
const STATUS_MAX_AP: u16 = 233;

pub fn layout(fourth: bool) -> &'static Layout {
    if fourth { &FOURTH } else { &NORMAL }
}

/// Whether the weight shows red: at or over the server's overweight percent of the limit.
pub fn heavy(weight: i64, limit: i64, percent: u32) -> bool {
    limit > 0 && weight * 100 / limit >= i64::from(percent)
}

/// An HP or SP bar's fill: its width (127 px at most, 0 at zero, when only the red caps show) and whether it's red
/// (under a quarter); none with no most, or more than the most (then the bar isn't drawn at all).
pub fn bar_fill(now: i64, most: i64) -> Option<(f32, bool)> {
    (most > 0 && (0..=most).contains(&now)).then(|| ((BAR_FULL * now as f32 / most as f32).trunc(), now * 4 < most))
}

/// A bar's percentage: "%d%%" of now·100/most, not cut at 100.
pub fn percent_text(now: i64, most: i64) -> String {
    format!("{}%", if most > 0 { now * 100 / most } else { 0 })
}

/// Where the minimised window's short first line starts: centred, at least 3 from the left.
pub fn mini_line_x(window_width: f32, width: f32) -> f32 {
    ((window_width - width) / 2.0).floor().max(3.0)
}

/// The minimised window's grey box under the title bar, its bottom corners cut 3 px: (x, y, width, height) pieces.
pub fn mini_box(width: f32, height: f32) -> Vec<[f32; 4]> {
    let body = height - MINI_BOX_TOP - 3.0;
    let mut pieces = vec![[0.0, MINI_BOX_TOP, width, body]];
    pieces.extend((0..3).map(|row| {
        let inset = row as f32 + 1.0;
        [inset, MINI_BOX_TOP + body + row as f32, width - 2.0 * inset, 1.0]
    }));
    pieces
}

/// The window's width: wide enough that the weight and zeny line fits inside with margins on both sides, and at least 220.
pub fn window_width(weight: i64, limit: i64, zeny: i64, font: &Font) -> f32 {
    let weight_text = format!("Weight : {:3} / {:3}", weight / 10, limit / 10);
    let zeny_text = zeny_text(zeny);
    let needed = WEIGHT_MARGIN + font.width(&weight_text) + WEIGHT_GAP + font.width(&zeny_text) + ZENY_MARGIN;
    needed.ceil().max(WIDTH)
}

/// The numbers over a bar: "%3d  /  %3d".
pub fn bar_text(now: i64, most: i64) -> String {
    format!("{now:3}  /  {most:3}")
}

/// Zeny in comma groups.
pub fn zeny_text(zeny: i64) -> String {
    let digits = zeny.unsigned_abs().to_string();
    let mut grouped = String::new();
    for (place, digit) in digits.chars().enumerate() {
        if place > 0 && (digits.len() - place).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    format!("Zeny : {}{grouped}", if zeny < 0 { "-" } else { "" })
}

/// The exp bar's hover text: "%.1f %%" to a tenth, rounded down; none at the top level (no next, or 99999999) or
/// past the next.
pub fn exp_tip(exp: i64, next: i64) -> Option<String> {
    (next > 0 && next != 99_999_999 && exp <= next).then(|| format!("{:.1} %", (exp * 1000 / next) as f64 / 10.0))
}

/// The mini window's first line, and the shorter one when that's too wide: levels, job and base exp to a tenth.
pub fn mini_lines(base: i64, job: &str, job_level: i64, exp: i64, next: i64) -> [String; 2] {
    let percent = if next > 0 { (exp * 1000 / next) as f64 / 10.0 } else { 100.0 };
    [format!("Lv.{base:2} / {job} / Lv.{job_level:2} / Exp. {percent:.1} %"), format!("Lv{base:2} / {job} / Lv{job_level:2} / Exp{percent:.1}%")]
}

/// The minimised window's second line: HP and SP (and a 4th job's AP), each red under a quarter.
fn mini_parts(hp: (i64, i64), sp: (i64, i64), ap: Option<(i64, i64)>) -> Vec<(String, bool)> {
    let mut parts = vec![(format!("HP {} / {}", hp.0, hp.1), hp.0 * 4 < hp.1), (" | ".to_owned(), false), (format!("SP {} / {}", sp.0, sp.1), sp.0 * 4 < sp.1)];
    if let Some(ap) = ap {
        parts.extend([(" | ".to_owned(), false), (format!("AP {} / {}", ap.0, ap.1), false)]);
    }
    parts
}

/// A bar's numbers that fit its 135 px: the game's padded "now  /  most", else without the padding, else in k and M.
pub fn bar_text_fitting(now: i64, most: i64, font: &Font) -> String {
    let room = BAR_SIZE[0] - 6.0;
    let units = |value: i64| match value {
        1_000_000.. => format!("{:.1}M", value as f64 / 1e6),
        10_000.. => format!("{}k", value / 1000),
        _ => value.to_string(),
    };
    [bar_text(now, most), format!("{now}/{most}"), format!("{}/{}", units(now), units(most))]
        .into_iter()
        .find(|text| font.width(text) <= room)
        .unwrap_or_else(|| format!("{}/{}", units(now), units(most)))
}

/// The menu's buttons in the game's order (menu_icon\bt_<name>.bmp).
pub const ORDER: [&str; 24] = [
    "status",
    "equip",
    "item",
    "skill",
    "booking",
    "party",
    "guild",
    "battle",
    "quest",
    "map",
    "navigation",
    "option",
    "bank",
    "rec",
    "mail",
    "achievement",
    "tip",
    "shop",
    "keyboard",
    "attendance",
    "adventurerAgency",
    "repute",
    "adventureguide",
    "probability",
];

/// The buttons that show, in the game's order: all of its list but battle and booking (rows of 5, 5, 5, 5 and 2). The
/// shop button opens the cash shop, as /cashshop does; the battleground queue stays on /battleground, the party board
/// on its shortcut.
pub fn visible() -> Vec<&'static str> {
    ORDER.into_iter().filter(|&name| name != "battle" && name != "booking").collect()
}

/// Doram have their own status button.
pub fn doram(job: u16) -> bool {
    matches!(job, 4218 | 4220 | 4308)
}

pub struct Assets {
    /// The window's back: the game's, and the 4th jobs'; the mini window's.
    backgrounds: [Texture; 2],
    /// The minimised window's grey edge.
    mini_edge: Texture,
    /// HP and SP bars' back.
    bar_back: Texture,
    /// Blue and red bar pieces: left cap, middle, right cap.
    bars: [[Texture; 3]; 2],
    exp_border: Texture,
    exp_fill: Texture,
    /// Menu toggle closed and open, the menu's back and its buttons.
    toggles: [Texture; 2],
    menu: Texture,
    icons: HashMap<&'static str, Option<Texture>>,
    /// The status button for Doram.
    status_doram: Option<Texture>,
    /// The mail button while unread mail waits, and the achievement button while a reward waits.
    mail_new: Option<Texture>,
    achievement_new: Option<Texture>,
    item_new: Option<Texture>,
    /// The status and skill buttons after a level up, and the toggle while any mark waits in the closed menu.
    status_new: Option<Texture>,
    skill_new: Option<Texture>,
    toggle_new: Option<Texture>,
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let basic = format!("{UI}basic_interface\\");
        let menu = format!("{UI}menu_icon\\");
        let image = |path: String| gpu.load_texture(grf, &path, false);
        Self {
            backgrounds: [image(format!("{basic}basewin_bg2.bmp")), image(format!("{basic}w_basewin_bg2.bmp"))],
            mini_edge: gpu.solid(MINI_EDGE),
            bar_back: image(format!("{basic}gze_bg1.bmp")),
            bars: ["blue", "red"].map(|color| ["left", "mid", "right"].map(|part| image(format!("{basic}gze{color}_{part}.bmp")))),
            exp_border: gpu.solid(EXP_BORDER),
            exp_fill: gpu.solid(EXP_FILL),
            toggles: [image(format!("{menu}bt_menu_normal.bmp")), image(format!("{menu}bt_menu_close_normal.bmp"))],
            menu: image(format!("{menu}bg_menu.tga")),
            icons: ORDER.iter().map(|name| (*name, gpu.try_load_texture(grf, &format!("{menu}bt_{}.bmp", name.to_lowercase()), false).ok())).collect(),
            status_doram: gpu.try_load_texture(grf, &format!("{menu}bt_status_doram.bmp"), false).ok(),
            mail_new: gpu.try_load_texture(grf, &format!("{menu}bt_mail_new.bmp"), false).ok(),
            achievement_new: gpu.try_load_texture(grf, &format!("{menu}bt_achievement_new.bmp"), false).ok(),
            item_new: gpu.try_load_texture(grf, &format!("{menu}bt_item_new.bmp"), false).ok(),
            status_new: gpu.try_load_texture(grf, &format!("{menu}bt_status_new.bmp"), false).ok(),
            skill_new: gpu.try_load_texture(grf, &format!("{menu}bt_skill_new.bmp"), false).ok(),
            toggle_new: gpu.try_load_texture(grf, &format!("{menu}bt_menu_new_normal.bmp"), false).ok(),
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title,
    Minimize,
    MenuToggle,
    /// A menu button, by name.
    Menu(&'static str),
    Window,
}

pub struct BasicInfo {
    pub frame: Frame,
    pub width: f32,
    pub menu_open: bool,
    /// The player is a 4th job (the taller window).
    pub fourth: bool,
    /// The player is a Doram (their own status button).
    pub doram: bool,
    /// What was last kept of the window.
    saved: String,
}

impl Default for BasicInfo {
    fn default() -> Self {
        let mut frame = Frame::new([0.0, 0.0], WIDTH);
        frame.open = true;
        Self { frame, width: WIDTH, menu_open: false, fourth: false, doram: false, saved: String::new() }
    }
}

/// What the window shows, from the player's status values.
pub struct Info<'a> {
    pub name: &'a str,
    pub job: &'a str,
    pub status: &'a HashMap<u16, i64>,
    pub unread_mail: bool,
    pub reward_waiting: bool,
    /// Something was picked up since the inventory was last open.
    pub new_items: bool,
    /// A base (job) level up the status (skill) window hasn't shown yet.
    pub new_level: bool,
    pub new_job_level: bool,
    /// The server's overweight point, in percent of the limit.
    pub overweight: u32,
    /// The shortcuts, for the keys the menu buttons name.
    pub keys: Option<&'a Shortcuts>,
}

impl BasicInfo {
    /// Sizes the window to what it shows: the weight and zeny line, or minimised the levels line and the HP and SP
    /// line, with margins. Whether it grew.
    pub fn update_width(&mut self, font: &Font, status: &HashMap<u16, i64>, job: &str) -> bool {
        let value = |id: u16| status.get(&id).copied().unwrap_or(0);
        let width = if self.frame.minimized {
            let layout = layout(self.fourth);
            let ap = layout.ap.map(|_| (value(STATUS_AP), value(STATUS_MAX_AP)));
            let line: f32 = mini_parts((value(net::STATUS_HP), value(net::STATUS_MAX_HP)), (value(net::STATUS_SP), value(net::STATUS_MAX_SP)), ap)
                .iter()
                .map(|(part, _)| font.width(part))
                .sum();
            let [_, short] =
                mini_lines(value(net::STATUS_BASE_LEVEL), job, value(net::STATUS_JOB_LEVEL), value(net::STATUS_BASE_EXP), value(net::STATUS_NEXT_BASE_EXP));
            (MINI_MARGIN * 2.0 + line).max(font.width(&short) + MINI_MIN_MARGIN).ceil().max(WIDTH)
        } else {
            window_width(value(net::STATUS_WEIGHT), value(net::STATUS_MAX_WEIGHT), value(net::STATUS_ZENY), font)
        };
        let grew = width > self.width;
        self.width = width;
        self.frame.set_width(width);
        grew
    }

    /// Where the window, its menu strip and its open menu are: x, y, width, height.
    pub fn bounds(&self) -> [f32; 4] {
        [self.frame.position[0], self.frame.position[1], self.width, self.height()]
    }

    /// The minimize button's corner inside the window (14 px from the right edge).
    pub fn minimize_button(&self) -> [f32; 2] {
        [self.width - 14.0, 3.0]
    }

    /// The window as kept between logins (the game's BASICINFOWNDINFO): x, y, width, its own height (55 or 71
    /// minimised, 134 or 149 not) and whether it shows.
    pub fn saved_text(&self) -> String {
        let [x, y] = self.frame.position;
        format!("{} {} {} {} {}", x as i32, y as i32, self.width as i32, self.body() as i32, u8::from(self.frame.open))
    }

    /// A kept window's place, whether it was minimised and whether it showed; None unless it's at least 220 wide, one of the
    /// window's heights, and wholly on `screen`.
    pub fn from_saved(text: &str, screen: [f32; 2]) -> Option<([f32; 2], bool, bool)> {
        let values: Vec<i32> = text.split_whitespace().map(|value| value.parse().ok()).collect::<Option<_>>()?;
        let [x, y, w, h, shown] = values[..] else { return None };
        let [x, y, w, h] = [x, y, w, h].map(|value| value as f32);
        let sizes = [NORMAL.mini, NORMAL.height, FOURTH.mini, FOURTH.height];
        let on_screen = x >= 0.0 && y >= 0.0 && x + w <= screen[0] && y + h <= screen[1];
        (w >= WIDTH && sizes.contains(&h) && on_screen).then_some(([x, y], h == NORMAL.mini || h == FOURTH.mini, shown != 0))
    }

    /// The window as it was left, or the default.
    pub fn load(screen: [f32; 2]) -> Self {
        let mut window = Self::default();
        let kept = crate::settings_path("basic_info").filter(|_| crate::keeps_windows()).and_then(|path| std::fs::read_to_string(path).ok());
        if let Some((position, minimized, shown)) = kept.as_deref().and_then(|text| Self::from_saved(text, screen)) {
            window.frame.position = position;
            window.frame.minimized = minimized;
            window.frame.open = shown;
        }
        window.saved = window.saved_text();
        window
    }

    /// Keeps the window's place and state when they changed.
    pub fn save_if_changed(&mut self) {
        let text = self.saved_text();
        if text != self.saved
            && crate::keeps_windows()
            && let Some(path) = crate::settings_path("basic_info")
        {
            let _ = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(&path, &text));
            self.saved = text;
        }
    }

    /// The window's own height: the minimised one's (55, or 71 for a 4th job) or the full one's.
    pub fn body(&self) -> f32 {
        let layout = layout(self.fourth);
        if self.frame.minimized { layout.mini } else { layout.height }
    }

    /// The window, plus the menu toggle and the open menu under it.
    pub fn height(&self) -> f32 {
        if self.menu_open { self.button_corner(0)[1] + visible().len().div_ceil(MENU_COLUMNS) as f32 * MENU_STEP[1] } else { self.menu_top() }
    }

    /// The menu is its own strip at the window's bottom edge (minimised or not): its toggle bar at the edge, the menu's
    /// back 9 down.
    fn toggle_top(&self) -> f32 {
        self.body() + TOGGLE_AT
    }

    fn menu_top(&self) -> f32 {
        self.body() + MENU_AT
    }

    /// The `place`th visible button's corner in the window.
    pub fn button_corner(&self, place: usize) -> [f32; 2] {
        let base = self.body();
        [MENU_BUTTONS[0] + (place % MENU_COLUMNS) as f32 * MENU_STEP[0], base + MENU_BUTTONS[1] + (place / MENU_COLUMNS) as f32 * MENU_STEP[1]]
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        // Its own reach, minimised or not: the window and the menu strip under it.
        let local = [cursor[0] - self.frame.position[0], cursor[1] - self.frame.position[1]];
        if !self.frame.open || !contains([0.0, 0.0], [self.width, self.height()], local) {
            return None;
        }
        Some(if contains(self.minimize_button(), BUTTON_SIZE, local) {
            Hit::Minimize
        } else if local[1] < TITLE_HEIGHT {
            Hit::Title
        } else if (self.toggle_top()..self.toggle_top() + TOGGLE_HEIGHT).contains(&local[1]) {
            Hit::MenuToggle
        } else if self.menu_open && local[1] >= self.menu_top() {
            visible()
                .into_iter()
                .enumerate()
                .find(|(place, _)| contains(self.button_corner(*place), MENU_BUTTON_SIZE, local))
                .map_or(Hit::Window, |(_, name)| Hit::Menu(name))
        } else {
            Hit::Window
        })
    }

    /// Alt+V: the small window and the full one in turn (a closed window opens as it was).
    pub fn mini_full(&mut self) {
        if self.frame.open {
            self.frame.minimized = !self.frame.minimized;
        } else {
            self.frame.open = true;
        }
    }

    /// Reacts to a click; returns the menu button pressed.
    pub fn click(&mut self, hit: Hit) -> Option<&'static str> {
        match hit {
            Hit::Minimize => self.frame.minimized = !self.frame.minimized,
            Hit::MenuToggle => self.menu_open = !self.menu_open,
            Hit::Menu(button) => return Some(button),
            Hit::Title | Hit::Window => {}
        }
        None
    }

    /// The strip that opens and closes the menu (for the test driver).
    pub fn toggle_box(&self) -> Option<[f32; 4]> {
        self.frame.open.then(|| {
            let [x, y] = self.frame.at([0.0, self.toggle_top()]);
            [x, y, self.width, TOGGLE_HEIGHT]
        })
    }

    pub fn menu_boxes(&self) -> Vec<(&'static str, [f32; 4])> {
        if !self.frame.open || !self.menu_open {
            return Vec::new();
        }
        visible()
            .into_iter()
            .enumerate()
            .map(|(place, name)| {
                let [x, y] = self.frame.at(self.button_corner(place));
                (name, [x, y, MENU_BUTTON_SIZE[0], MENU_BUTTON_SIZE[1]])
            })
            .collect()
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
        info: &Info,
        cursor: [f32; 2],
    ) {
        if !self.frame.open {
            return;
        }
        let layout = layout(self.fourth);
        let at = |local: [f32; 2]| self.frame.at(local);
        let value = |id: u16| info.status.get(&id).copied().unwrap_or(0);
        let text = |layer: &mut Layer<'a>, text: String, place: [f32; 2], color: [u8; 3]| layer.texts.push((text, at(place), color));

        if self.frame.minimized {
            // The standard title bar over a grey box with cut corners, white inside.
            chrome.draw_title_bar(layer, at([0.0, 0.0]), self.width);
            for [x, y, w, h] in mini_box(self.width, layout.mini) {
                layer.sprites.push((&assets.mini_edge, at([x, y]), [w, h]));
            }
            layer.sprites.push((&chrome.white, at([1.0, MINI_BOX_TOP]), [self.width - 2.0, layout.mini - MINI_BOX_TOP - 2.0]));
        } else {
            let background = &assets.backgrounds[usize::from(self.fourth)];
            layer.sprites.push((background, at([0.0, 0.0]), [self.width, layout.height]));
        }
        let minimize_button = self.minimize_button();
        for place in [BASE_BUTTON, minimize_button] {
            let button = usize::from(place == minimize_button);
            let image = chrome.button(button, contains(at(place), BUTTON_SIZE, cursor));
            layer.sprites.push((image, at(place), image.size));
        }
        let [(hp, max_hp), (sp, max_sp)] = [(value(net::STATUS_HP), value(net::STATUS_MAX_HP)), (value(net::STATUS_SP), value(net::STATUS_MAX_SP))];
        // Minimised: the name in the title, a line of levels and exp, and one of HP and SP (each red under a quarter).
        if self.frame.minimized {
            text(layer, info.name.to_owned(), MINI_NAME, BLACK);
            let [long, short] = mini_lines(
                value(net::STATUS_BASE_LEVEL),
                info.job,
                value(net::STATUS_JOB_LEVEL),
                value(net::STATUS_BASE_EXP),
                value(net::STATUS_NEXT_BASE_EXP),
            );
            if font.width(&long) <= self.width - MINI_MARGIN * 2.0 {
                text(layer, long, [9.0, 20.0], BLACK);
            } else {
                let x = mini_line_x(self.width, font.width(&short));
                text(layer, short, [x, 20.0], BLACK);
            }
            let mut x = MINI_MARGIN;
            // ponytail: a 4th job's AP after SP on the same line.
            let ap = layout.ap.map(|_| (value(STATUS_AP), value(STATUS_MAX_AP)));
            for (part, low) in mini_parts((hp, max_hp), (sp, max_sp), ap) {
                let width = font.width(&part);
                text(layer, part, [x, 36.0], if low { RED } else { BLACK });
                x += width;
            }
            if let Some((tip, corner)) = self.draw_menu(layer, assets, info, messages, cursor) {
                crate::window::tooltip(tooltip, chrome, font, &tip, corner, [255; 3]);
            }
            return;
        }
        let title = messages.get(BASIC_INFO_MESSAGE).cloned().unwrap_or_default();
        text(layer, title.clone(), [TITLE[0] + 1.0, TITLE[1] + 1.0], [255; 3]);
        text(layer, title, TITLE, BLACK);
        text(layer, info.name.to_owned(), layout.name, BLACK);
        text(layer, info.job.to_owned(), layout.job, BLACK);
        // HP and SP, and AP for 4th jobs, each a bar the same way.
        // ponytail: the AP fill uses the HP and SP pieces.
        let mut rows = vec![(layout.labels[0], layout.bars[0], hp, max_hp), (layout.labels[1], layout.bars[1], sp, max_sp)];
        if let Some((label, corner)) = layout.ap {
            rows.push((label, corner, value(STATUS_AP), value(STATUS_MAX_AP)));
        }
        for (bar, (label, corner, now, most)) in rows.into_iter().enumerate() {
            let name = layout.label_messages.and_then(|ids| messages.get(ids[bar]).filter(|text| !text.is_empty()).cloned());
            text(layer, name.unwrap_or_else(|| ["HP", "SP", "AP"][bar].to_owned()), label, BLACK);
            // Over the most, the bar isn't drawn at all: no back, no fill, no numbers.
            if now <= most {
                layer.sprites.push((&assets.bar_back, at(corner), BAR_SIZE));
                if let Some((width, red)) = bar_fill(now, most) {
                    let [left, middle, right] = &assets.bars[usize::from(red)];
                    layer.sprites.push((left, at(corner), left.size));
                    layer.sprites.push((middle, at(offset(corner, [left.size[0], 0.0])), [width, middle.size[1]]));
                    layer.sprites.push((right, at(offset(corner, [left.size[0] + width, 0.0])), right.size));
                }
                let numbers = bar_text_fitting(now, most, font);
                text(layer, numbers.clone(), [corner[0] + ((BAR_SIZE[0] - font.width(&numbers)) / 2.0).floor(), corner[1] - 2.0], BLACK);
            }
            let percent = percent_text(now, most);
            let x = layout.percent.unwrap_or(self.width - 20.0 - font.width(&percent));
            text(layer, percent, [x, label[1]], BLACK);
        }

        let levels = [
            (net::STATUS_BASE_LEVEL, "Base Lv.", net::STATUS_BASE_EXP, net::STATUS_NEXT_BASE_EXP),
            (net::STATUS_JOB_LEVEL, "Job Lv.", net::STATUS_JOB_EXP, net::STATUS_NEXT_JOB_EXP),
        ];
        for (row, (level, label, exp, next)) in levels.into_iter().enumerate() {
            text(layer, format!("{label} {}", value(level)), layout.levels[row], BLACK);
            // At the top level there's no next: no bar.
            let next = value(next);
            if next > 0 {
                let corner = layout.exp_bars[row];
                layer.sprites.push((&assets.exp_border, at(corner), EXP_SIZE));
                layer.sprites.push((&chrome.white, at(offset(corner, [1.0, 1.0])), [EXP_INNER, EXP_SIZE[1] - 2.0]));
                let filled = (EXP_INNER * value(exp).clamp(0, next) as f32 / next as f32).trunc();
                layer.sprites.push((&assets.exp_fill, at(offset(corner, [1.0, 1.0])), [filled, EXP_SIZE[1] - 2.0]));
                // Resting on the bar shows how far along it is, 16 px above its corner.
                if contains(at(corner), EXP_SIZE, cursor)
                    && let Some(tip) = exp_tip(value(exp), next)
                {
                    crate::window::tooltip(tooltip, chrome, font, &tip, at(offset(corner, [0.0, -16.0])), [255; 3]);
                }
            }
        }

        // Weight (in tenths) turns red at the server's overweight point.
        let (weight, limit) = (value(net::STATUS_WEIGHT), value(net::STATUS_MAX_WEIGHT));
        let zeny = zeny_text(value(net::STATUS_ZENY));
        let weight_text = format!("Weight : {:3} / {:3}", weight / 10, limit / 10);
        let zeny_x = self.width - ZENY_MARGIN - font.width(&zeny);
        text(layer, zeny, [zeny_x, layout.extra], BLACK);
        let weight_x = zeny_x - WEIGHT_GAP - font.width(&weight_text);
        text(layer, weight_text, [weight_x, layout.extra], if heavy(weight, limit, info.overweight) { RED } else { BLACK });
        // The bottom left strip shows the weight in percent, 19 px above it.
        let height = layout.height;
        if limit > 0 && contains(at([0.0, height - 20.0]), [100.0, 20.0], cursor) {
            crate::window::tooltip(tooltip, chrome, font, &format!("Weight {}%", weight * 100 / limit), at([0.0, height - 39.0]), [255; 3]);
        }

        if let Some((tip, corner)) = self.draw_menu(layer, assets, info, messages, cursor) {
            crate::window::tooltip(tooltip, chrome, font, &tip, corner, [255; 3]);
        }
    }

    /// The menu under the window (also under the minimised one): its toggle bar, and the buttons when it's open.
    /// Returns the tooltip of the button under the pointer and its corner.
    fn draw_menu<'a>(&self, layer: &mut Layer<'a>, assets: &'a Assets, info: &Info, messages: &[String], cursor: [f32; 2]) -> Option<(String, [f32; 2])> {
        let at = |local: [f32; 2]| self.frame.at(local);
        let mut tip = None;
        // The menu toggle, and the menu when it's open.
        let marked = info.unread_mail || info.reward_waiting || info.new_items || info.new_level || info.new_job_level;
        let toggle = match &assets.toggle_new {
            Some(new) if !self.menu_open && marked => new,
            _ => &assets.toggles[usize::from(self.menu_open)],
        };
        layer.sprites.push((toggle, at([0.0, self.toggle_top()]), [self.width, TOGGLE_HEIGHT]));
        if self.menu_open {
            layer.sprites.push((&assets.menu, at([0.0, self.menu_top()]), assets.menu.size));
            for (place, name) in visible().into_iter().enumerate() {
                let icon = assets.icons.get(name).and_then(Option::as_ref);
                let icon = match name {
                    "mail" if info.unread_mail => assets.mail_new.as_ref().or(icon),
                    "achievement" if info.reward_waiting => assets.achievement_new.as_ref().or(icon),
                    "item" if info.new_items => assets.item_new.as_ref().or(icon),
                    "status" if info.new_level => assets.status_new.as_ref().or(icon),
                    "status" if self.doram => assets.status_doram.as_ref().or(icon),
                    "skill" if info.new_job_level => assets.skill_new.as_ref().or(icon),
                    _ => icon,
                };
                if let Some(icon) = icon {
                    layer.sprites.push((icon, at(self.button_corner(place)), icon.size));
                }
                // Its name and the key that opens its window, 20 px above it.
                if contains(at(self.button_corner(place)), MENU_BUTTON_SIZE, cursor) {
                    tip = menu_tip(name, messages, |action| info.keys.and_then(|keys| keys.keys_for(action)))
                        .map(|text| (text, at(offset(self.button_corner(place), [0.0, -20.0]))));
                }
            }
        }
        tip
    }
}

/// A menu button's tooltip: "name (key)" with the key of the window it opens, the name alone without one; Option's key
/// is always Esc.
pub fn menu_tip(button: &str, messages: &[String], keys: impl Fn(Action) -> Option<String>) -> Option<String> {
    let (message, action) = match button {
        "status" => (105, Some(Action::Status)),
        "equip" => (104, Some(Action::Equipment)),
        "item" => (106, Some(Action::Inventory)),
        "skill" => (283, Some(Action::Skills)),
        "party" => (103, Some(Action::Party)),
        "guild" => (3134, Some(Action::Guild)),
        "quest" => (1317, Some(Action::Quests)),
        "map" => (2859, Some(Action::WorldMap)),
        "navigation" => (2353, None),
        "option" => return messages.get(265).filter(|name| !name.is_empty()).map(|name| format!("{name} (Esc)")),
        "bank" => (3135, Some(Action::Bank)),
        "rec" => (2350, None),
        "mail" => (3136, Some(Action::Mail)),
        "achievement" => (2646, Some(Action::Achievements)),
        "tip" => (3239, Some(Action::Tip)),
        "shop" => (3137, None),
        "keyboard" => (3128, None),
        "attendance" => (3473, None),
        "adventurerAgency" => (3514, Some(Action::PartyBoard)),
        "repute" => (3827, Some(Action::Reputation)),
        "adventureguide" => (4053, None),
        "probability" => (4119, None),
        _ => return None,
    };
    let name = messages.get(message).filter(|name| !name.is_empty())?;
    Some(match action.and_then(keys) {
        Some(key) => format!("{name} ({key})"),
        None => name.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alt_v_switches_small_and_full() {
        let mut window = BasicInfo::default();
        window.mini_full();
        assert!(window.frame.open && window.frame.minimized);
        window.mini_full();
        assert!(window.frame.open && !window.frame.minimized);
        window.frame.open = false;
        window.mini_full();
        assert!(window.frame.open && !window.frame.minimized);
    }

    #[test]
    fn menu_buttons_name_their_window_and_its_key() {
        let mut messages = vec![String::new(); 3200];
        messages[106] = "Inventory".into();
        messages[265] = "Option".into();
        messages[2353] = "Navigation".into();
        let keys = |action| (action == Action::Inventory).then(|| "Alt+E".to_owned());
        assert_eq!(menu_tip("item", &messages, keys).as_deref(), Some("Inventory (Alt+E)"));
        assert_eq!(menu_tip("option", &messages, keys).as_deref(), Some("Option (Esc)"));
        assert_eq!(menu_tip("navigation", &messages, keys).as_deref(), Some("Navigation"));
        // No words for it, no tooltip.
        assert_eq!(menu_tip("bank", &messages, keys), None);
    }

    #[test]
    fn exp_bar_tip_is_a_tenth_of_a_percent_rounded_down() {
        assert_eq!(exp_tip(4235, 10_000).as_deref(), Some("42.3 %"));
        assert_eq!(exp_tip(0, 500).as_deref(), Some("0.0 %"));
        assert_eq!(exp_tip(500, 500).as_deref(), Some("100.0 %"));
        // None at the top level or past the next.
        assert_eq!([exp_tip(5, 0), exp_tip(5, 99_999_999), exp_tip(600, 500)], [None, None, None]);
    }

    #[test]
    fn lays_out_by_job() {
        assert!(!fourth_job(0) && !fourth_job(4054) && fourth_job(4252) && fourth_job(4329) && fourth_job(4360) && !fourth_job(4300));
        let normal = layout(false);
        assert_eq!(
            (normal.height, normal.mini, normal.name, normal.bars[1], normal.levels[1], normal.exp_bars[0]),
            (134.0, 55.0, [9.0, 22.0], [35.0, 68.0], [17.0, 96.0], [85.0, 89.0])
        );
        // A 4th job's: 149 tall, a third bar for AP, the rows where w_basewin_bg2 has them.
        let fourth = layout(true);
        assert_eq!((fourth.height, fourth.mini, fourth.labels, fourth.ap), (149.0, 71.0, [[9.0, 51.0], [9.0, 65.0]], Some(([9.0, 80.0], [35.0, 83.0]))));
        assert_eq!((fourth.levels, fourth.exp_bars, fourth.extra), ([[17.0, 99.0], [17.0, 111.0]], [[85.0, 104.0], [85.0, 116.0]], 131.0));
        assert_eq!(fourth.label_messages, Some([3068, 3069, 3628]));
        // The AP bar fills as the others: 30 of 120 AP.
        assert_eq!(bar_fill(30, 120), Some((31.0, false)));
        // The menu toggle follows the window's height.
        let mut window = BasicInfo::default();
        assert_eq!(window.height(), 134.0 + MENU_AT);
        window.fourth = true;
        assert_eq!(window.height(), 149.0 + MENU_AT);
    }

    #[test]
    fn shows_the_games_buttons_on_its_grid() {
        // The 22 names in order (all but battle and booking).
        assert_eq!(
            visible(),
            [
                "status",
                "equip",
                "item",
                "skill",
                "party",
                "guild",
                "quest",
                "map",
                "navigation",
                "option",
                "bank",
                "rec",
                "mail",
                "achievement",
                "tip",
                "shop",
                "keyboard",
                "attendance",
                "adventurerAgency",
                "repute",
                "adventureguide",
                "probability",
            ]
        );
        // Five a row, 42 and 43 apart, from (9, 19) under the window.
        let mut window = BasicInfo::default();
        let first = window.button_corner(0);
        assert_eq!(
            (first, window.button_corner(4), window.button_corner(5), window.button_corner(13)),
            ([9.0, 153.0], [177.0, 153.0], [9.0, 196.0], [135.0, 239.0])
        );
        // The last two in the fifth row at x 9 and 51, 172 below the first.
        assert_eq!(window.button_corner(20), [9.0, first[1] + 172.0]);
        assert_eq!(window.button_corner(21), [51.0, first[1] + 172.0]);
        window.frame.open = true;
        window.menu_open = true;
        let corner = window.button_corner(6);
        assert_eq!(window.hit(window.frame.at([corner[0] + 5.0, corner[1] + 5.0])), Some(Hit::Menu("quest")));
        assert_eq!(window.menu_boxes().len(), 22);
        assert_eq!(window.height(), 134.0 + 19.0 + 5.0 * 43.0);
        window.fourth = true;
        assert_eq!(window.button_corner(0), [9.0, 168.0]);
    }

    #[test]
    fn keeps_the_menu_under_the_minimised_window() {
        let mut window = BasicInfo::default();
        window.frame.minimized = true;
        // The toggle joined to the 55-tall window's bottom, the buttons from (9, 19) under it; 71 for a 4th job.
        assert_eq!(window.toggle_box(), Some([0.0, 55.0, WIDTH, TOGGLE_HEIGHT]));
        assert_eq!(window.hit(window.frame.at([100.0, 55.0])), Some(Hit::MenuToggle));
        assert_eq!(window.hit(window.frame.at([100.0, 63.0])), Some(Hit::MenuToggle));
        assert_ne!(window.hit(window.frame.at([100.0, 54.0])), Some(Hit::MenuToggle));
        window.menu_open = true;
        assert_eq!((window.button_corner(0), window.menu_boxes().len()), ([9.0, 74.0], 22));
        let corner = window.button_corner(6);
        assert_eq!(window.hit(window.frame.at([corner[0] + 5.0, corner[1] + 5.0])), Some(Hit::Menu("quest")));
        window.fourth = true;
        assert_eq!(window.button_corner(0), [9.0, 90.0]);
    }

    #[test]
    fn keeps_the_window_where_it_was() {
        let screen = [1024.0, 768.0];
        let mut window = BasicInfo::default();
        window.frame.position = [300.0, 200.0];
        window.frame.minimized = true;
        let text = window.saved_text();
        assert_eq!(text, "300 200 220 55 1");
        assert_eq!(BasicInfo::from_saved(&text, screen), Some(([300.0, 200.0], true, true)));
        // The full window, a 4th job's, shut.
        assert_eq!(BasicInfo::from_saved("0 0 220 149 0", screen), Some(([0.0, 0.0], false, false)));
        // A wrong width or height, or off the screen: the default instead.
        for bad in ["0 0 200 134 1", "0 0 220 100 1", "900 0 220 134 1", "0 700 220 134 1", "-5 0 220 134 1", "junk"] {
            assert_eq!(BasicInfo::from_saved(bad, screen), None, "{bad}");
        }
    }

    #[test]
    fn fills_bars_and_says_numbers() {
        assert_eq!(bar_fill(40, 40), Some((127.0, false)));
        assert_eq!(bar_fill(9, 40), Some((28.0, true)), "under a quarter: red");
        assert_eq!(bar_fill(10, 40), Some((31.0, false)));
        // Over the most: no bar; at zero only the red caps (no fill); no most: nothing.
        assert_eq!((bar_fill(41, 40), bar_fill(0, 40), bar_fill(5, 0)), (None, Some((0.0, true)), None));
        // The percentage isn't cut at 100.
        assert_eq!([percent_text(41, 40), percent_text(0, 40), percent_text(20, 40), percent_text(5, 0)], ["102%", "0%", "50%", "0%"]);
        // The short mini line: centred, never nearer the left than 3.
        assert_eq!((mini_line_x(WIDTH, 200.0), mini_line_x(WIDTH, 500.0)), (((WIDTH - 200.0) / 2.0).floor(), 3.0));
        // The grey box under the title bar, its bottom corners cut 3 px.
        let pieces = mini_box(WIDTH, 55.0);
        assert_eq!(pieces[0], [0.0, 17.0, WIDTH, 35.0]);
        assert_eq!(pieces[1..], [[1.0, 52.0, WIDTH - 2.0, 1.0], [2.0, 53.0, WIDTH - 4.0, 1.0], [3.0, 54.0, WIDTH - 6.0, 1.0]]);
        assert_eq!(bar_text(7, 40), "  7  /   40");
        assert_eq!(zeny_text(1234567), "Zeny : 1,234,567");
        assert_eq!(zeny_text(999), "Zeny : 999");
        // Red at the overweight point, which the server may move.
        assert!(!heavy(999, 2000, 50) && heavy(1000, 2000, 50) && heavy(1800, 2000, 90) && !heavy(1799, 2000, 90) && !heavy(5, 0, 50));
        assert_eq!(mini_lines(5, "Novice", 3, 25, 200)[0], "Lv. 5 / Novice / Lv. 3 / Exp. 12.5 %");
        assert_eq!(mini_lines(99, "Novice", 10, 0, 0)[1], "Lv99 / Novice / Lv10 / Exp100.0%");
    }

    #[test]
    fn window_width_grows_for_long_numbers() {
        let font = Font::load();
        // Short numbers: width is at least 220.
        let w_short = window_width(1000, 2000, 500, &font);
        assert_eq!(w_short, WIDTH);

        // 1026 / 2630 weight and 100,000,000 zeny:
        let w_100m = window_width(10260, 26300, 100_000_000, &font);
        assert!(w_100m > WIDTH, "w_100m = {w_100m} should grow beyond {WIDTH}");

        // 5-digit weights and 2,147,483,647 zeny:
        let w_max = window_width(999990, 999990, 2_147_483_647, &font);
        assert!(w_max > w_100m, "w_max = {w_max} should grow beyond {w_100m}");

        // Check that with w_100m and w_max, both margins are preserved.
        for (weight, limit, zeny, w) in [(10260, 26300, 100_000_000, w_100m), (999990, 999990, 2_147_483_647, w_max)] {
            let weight_text = format!("Weight : {:3} / {:3}", weight / 10, limit / 10);
            let zeny_text = zeny_text(zeny);
            let zeny_x = w - ZENY_MARGIN - font.width(&zeny_text);
            let weight_x = zeny_x - WEIGHT_GAP - font.width(&weight_text);
            assert!(weight_x >= WEIGHT_MARGIN, "weight_x {weight_x} should be >= {WEIGHT_MARGIN}");
            assert_eq!(zeny_x + font.width(&zeny_text), w - ZENY_MARGIN);
        }
    }
    #[test]
    fn minimised_window_grows_for_its_lines_and_bars_shrink_their_numbers() {
        let font = Font::load();
        let mut window = BasicInfo::default();
        window.frame.minimized = true;
        let status = |hp: i64| -> HashMap<u16, i64> {
            HashMap::from([
                (net::STATUS_HP, hp),
                (net::STATUS_MAX_HP, hp),
                (net::STATUS_SP, hp / 3),
                (net::STATUS_MAX_SP, hp / 3),
                (net::STATUS_BASE_LEVEL, 99),
            ])
        };
        // Small numbers fit the usual width; six-digit HP and a long job name need more, and each line then fits.
        assert!(!window.update_width(&font, &status(535), "Novice") && window.width == WIDTH);
        assert!(window.update_width(&font, &status(123_456), "Imperial Guard"));
        let line = mini_parts((123_456, 123_456), (41_152, 41_152), None).iter().map(|(part, _)| font.width(part)).sum::<f32>();
        assert!(window.width >= 2.0 * MINI_MARGIN + line, "{} for a line of {line}", window.width);
        // A bar's numbers keep to its 135 px, down to k and M.
        for most in [535, 123_456, 123_456_789, 2_147_483_647] {
            assert!(font.width(&bar_text_fitting(most, most, &font)) <= BAR_SIZE[0], "{most}");
        }
        assert_eq!(bar_text_fitting(7, 40, &font), "  7  /   40");
    }
}
