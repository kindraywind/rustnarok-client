//! The basic information window (Alt+V): name, job, HP and SP, levels and experience, weight and zeny, and the menu
//! of the other windows under it.

use std::collections::HashMap;

use rustnarok_client::grf::Grf;
use rustnarok_client::net;
use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};
use crate::window::{Chrome, Frame, TITLE_HEIGHT};
use crate::{Layer, UI, contains, offset};

const WIDTH: f32 = 220.0;
const BASE_BUTTON: [f32; 2] = [3.0, 3.0];
const MINIMIZE_BUTTON: [f32; 2] = [206.0, 3.0];
const BUTTON_SIZE: [f32; 2] = [11.0, 11.0];
/// The title: white at (18, 3) under black at (17, 2); the only text with a shadow.
const TITLE: [f32; 2] = [17.0, 2.0];
/// HP and SP bars: 135 by 9, filled 127 px at most.
const BAR_SIZE: [f32; 2] = [135.0, 9.0];
const BAR_FULL: f32 = 127.0;
const EXP_SIZE: [f32; 2] = [112.0, 6.0];
const EXP_INNER: f32 = 110.0;
/// Zeny's right edge, and the gap to the weight left of it.
const ZENY_RIGHT: f32 = 215.0;
const WEIGHT_GAP: f32 = 10.0;
/// The menu window under the basic info: the toggle bar (219 by 9) 4 down, the menu's back 13 down, and its buttons
/// (32 by 34) from (9, 23), 42 and 43 apart, five to a row.
const TOGGLE_AT: f32 = 4.0;
const TOGGLE_HEIGHT: f32 = 9.0;
const MENU_AT: f32 = 13.0;
const MENU_BUTTONS: [f32; 2] = [9.0, 23.0];
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
};

// ponytail: the 4th jobs' window keeps the places it had over w_basewin_bg2.bmp.
const FOURTH: Layout = Layout {
    height: 150.0,
    mini: 71.0,
    name: [10.0, 20.0],
    job: [10.0, 33.0],
    labels: [[15.0, 50.0], [15.0, 65.0]],
    bars: [[35.0, 53.0], [35.0, 68.0]],
    percent: None,
    levels: [[15.0, 101.0], [15.0, 112.0]],
    exp_bars: [[84.0, 103.0], [84.0, 115.0]],
    extra: 134.0,
};

pub fn layout(fourth: bool) -> &'static Layout {
    if fourth { &FOURTH } else { &NORMAL }
}

/// Whether the weight shows red: at or over the server's overweight percent of the limit.
pub fn heavy(weight: i64, limit: i64, percent: u32) -> bool {
    limit > 0 && weight * 100 / limit >= i64::from(percent)
}

/// An HP or SP bar's fill: its width (127 px at most) and whether it's red (under a quarter); none when there's
/// nothing to show, or more than the most.
pub fn bar_fill(now: i64, most: i64) -> Option<(f32, bool)> {
    (most > 0 && now > 0 && now <= most).then(|| ((BAR_FULL * now as f32 / most as f32).trunc(), now * 4 < most))
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

/// The mini window's first line, and the shorter one when that's too wide: levels, job and base exp to a tenth.
pub fn mini_lines(base: i64, job: &str, job_level: i64, exp: i64, next: i64) -> [String; 2] {
    let percent = if next > 0 { (exp * 1000 / next) as f64 / 10.0 } else { 100.0 };
    [format!("Lv.{base:2} / {job} / Lv.{job_level:2} / Exp. {percent:.1} %"), format!("Lv{base:2} / {job} / Lv{job_level:2} / Exp{percent:.1}%")]
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

/// The buttons the game shows.
const SHOWN: [&str; 14] = ["status", "equip", "item", "skill", "party", "guild", "quest", "map", "navigation", "option", "bank", "mail", "keyboard", "repute"];

/// Buttons shown besides the game's fourteen, for windows this client has that open from nowhere else.
// ponytail: the game hides battle and shop; here battle shows when the data lists battlegrounds (their queue opens
// from it, #107) and shop opens the cash shop.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Features {
    pub battle: bool,
    pub shop: bool,
}

/// The buttons that show, in the game's order.
pub fn visible(features: Features) -> Vec<&'static str> {
    ORDER.into_iter().filter(|name| SHOWN.contains(name) || (*name == "battle" && features.battle) || (*name == "shop" && features.shop)).collect()
}

/// Doram have their own status button.
pub fn doram(job: u16) -> bool {
    matches!(job, 4218 | 4220 | 4308)
}

pub struct Assets {
    /// The window's back: the game's, and the 4th jobs'; the mini window's.
    backgrounds: [Texture; 2],
    mini: Texture,
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
            mini: image(format!("{basic}basewin_mini.bmp")),
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
    pub menu_open: bool,
    /// The player is a 4th job (the taller window).
    pub fourth: bool,
    /// The player is a Doram (their own status button).
    pub doram: bool,
    pub features: Features,
}

impl Default for BasicInfo {
    fn default() -> Self {
        let mut frame = Frame::new([0.0, 0.0], WIDTH);
        frame.open = true;
        Self { frame, menu_open: false, fourth: false, doram: false, features: Features::default() }
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
}

impl BasicInfo {
    /// The window, plus the menu toggle and the open menu under it.
    pub fn height(&self) -> f32 {
        let layout = layout(self.fourth);
        if self.frame.minimized {
            layout.mini
        } else if self.menu_open {
            self.button_corner(0)[1] + visible(self.features).len().div_ceil(MENU_COLUMNS) as f32 * MENU_STEP[1]
        } else {
            self.menu_top()
        }
    }

    /// The menu window starts at the basic info's bottom edge: its toggle bar 4 down, the menu's back 13 down.
    fn toggle_top(&self) -> f32 {
        layout(self.fourth).height + TOGGLE_AT
    }

    fn menu_top(&self) -> f32 {
        layout(self.fourth).height + MENU_AT
    }

    /// The `place`th visible button's corner in the window.
    pub fn button_corner(&self, place: usize) -> [f32; 2] {
        let base = layout(self.fourth).height;
        [MENU_BUTTONS[0] + (place % MENU_COLUMNS) as f32 * MENU_STEP[0], base + MENU_BUTTONS[1] + (place / MENU_COLUMNS) as f32 * MENU_STEP[1]]
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let local = self.frame.local(cursor, self.height())?;
        Some(if contains(MINIMIZE_BUTTON, BUTTON_SIZE, local) {
            Hit::Minimize
        } else if local[1] < TITLE_HEIGHT {
            Hit::Title
        } else if self.frame.minimized {
            Hit::Window
        } else if (self.toggle_top()..self.toggle_top() + TOGGLE_HEIGHT).contains(&local[1]) {
            Hit::MenuToggle
        } else if self.menu_open && local[1] >= self.menu_top() {
            visible(self.features)
                .into_iter()
                .enumerate()
                .find(|(place, _)| contains(self.button_corner(*place), MENU_BUTTON_SIZE, local))
                .map_or(Hit::Window, |(_, name)| Hit::Menu(name))
        } else {
            Hit::Window
        })
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
        (self.frame.open && !self.frame.minimized).then(|| {
            let [x, y] = self.frame.at([0.0, self.toggle_top()]);
            [x, y, WIDTH, TOGGLE_HEIGHT]
        })
    }

    pub fn menu_boxes(&self) -> Vec<(&'static str, [f32; 4])> {
        if !self.frame.open || !self.menu_open || self.frame.minimized {
            return Vec::new();
        }
        visible(self.features)
            .into_iter()
            .enumerate()
            .map(|(place, name)| {
                let [x, y] = self.frame.at(self.button_corner(place));
                (name, [x, y, MENU_BUTTON_SIZE[0], MENU_BUTTON_SIZE[1]])
            })
            .collect()
    }

    pub fn draw<'a>(&self, layer: &mut Layer<'a>, chrome: &'a Chrome, assets: &'a Assets, font: &Font, messages: &[String], info: &Info, cursor: [f32; 2]) {
        if !self.frame.open {
            return;
        }
        let layout = layout(self.fourth);
        let at = |local: [f32; 2]| self.frame.at(local);
        let value = |id: u16| info.status.get(&id).copied().unwrap_or(0);
        let text = |layer: &mut Layer<'a>, text: String, place: [f32; 2], color: [u8; 3]| layer.texts.push((text, at(place), color));

        let background = if self.frame.minimized { &assets.mini } else { &assets.backgrounds[usize::from(self.fourth)] };
        let height = if self.frame.minimized { layout.mini } else { layout.height };
        layer.sprites.push((background, at([0.0, 0.0]), [WIDTH, height]));
        for place in [BASE_BUTTON, MINIMIZE_BUTTON] {
            let button = usize::from(place == MINIMIZE_BUTTON);
            let image = chrome.button(button, contains(at(place), BUTTON_SIZE, cursor));
            layer.sprites.push((image, at(place), image.size));
        }
        let [(hp, max_hp), (sp, max_sp)] = [(value(net::STATUS_HP), value(net::STATUS_MAX_HP)), (value(net::STATUS_SP), value(net::STATUS_MAX_SP))];
        // Minimised: the name in the title, a line of levels and exp, and one of HP and SP (each red under a quarter).
        if self.frame.minimized {
            text(layer, info.name.to_owned(), TITLE, BLACK);
            let [long, short] = mini_lines(
                value(net::STATUS_BASE_LEVEL),
                info.job,
                value(net::STATUS_JOB_LEVEL),
                value(net::STATUS_BASE_EXP),
                value(net::STATUS_NEXT_BASE_EXP),
            );
            if font.width(&long) <= WIDTH - 18.0 {
                text(layer, long, [9.0, 20.0], BLACK);
            } else {
                text(layer, short.clone(), [((WIDTH - font.width(&short)) / 2.0).floor(), 20.0], BLACK);
            }
            let mut x = 9.0;
            for (part, low) in [(format!("HP {hp} / {max_hp}"), hp * 4 < max_hp), (" | ".to_owned(), false), (format!("SP {sp} / {max_sp}"), sp * 4 < max_sp)] {
                let width = font.width(&part);
                text(layer, part, [x, 36.0], if low { RED } else { BLACK });
                x += width;
            }
            return;
        }
        let title = messages.get(BASIC_INFO_MESSAGE).cloned().unwrap_or_default();
        text(layer, title.clone(), [TITLE[0] + 1.0, TITLE[1] + 1.0], [255; 3]);
        text(layer, title, TITLE, BLACK);
        text(layer, info.name.to_owned(), layout.name, BLACK);
        text(layer, info.job.to_owned(), layout.job, BLACK);
        for (bar, (now, most)) in [(hp, max_hp), (sp, max_sp)].into_iter().enumerate() {
            let corner = layout.bars[bar];
            text(layer, ["HP", "SP"][bar].to_owned(), layout.labels[bar], BLACK);
            layer.sprites.push((&assets.bar_back, at(corner), BAR_SIZE));
            if let Some((width, red)) = bar_fill(now, most) {
                let [left, middle, right] = &assets.bars[usize::from(red)];
                layer.sprites.push((left, at(corner), left.size));
                layer.sprites.push((middle, at(offset(corner, [left.size[0], 0.0])), [width, middle.size[1]]));
                layer.sprites.push((right, at(offset(corner, [left.size[0] + width, 0.0])), right.size));
            }
            let numbers = bar_text(now, most);
            text(layer, numbers.clone(), [corner[0] + ((BAR_SIZE[0] - font.width(&numbers)) / 2.0).floor(), corner[1] - 2.0], BLACK);
            let percent = format!("{}%", if most > 0 { (now * 100 / most).clamp(0, 100) } else { 0 });
            let x = layout.percent.unwrap_or(WIDTH - 20.0 - font.width(&percent));
            text(layer, percent, [x, layout.labels[bar][1]], BLACK);
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
            }
        }

        // Weight (in tenths) turns red at the server's overweight point.
        let (weight, limit) = (value(net::STATUS_WEIGHT), value(net::STATUS_MAX_WEIGHT));
        let zeny = zeny_text(value(net::STATUS_ZENY));
        let weight_text = format!("Weight : {:3} / {:3}", weight / 10, limit / 10);
        let zeny_x = ZENY_RIGHT - font.width(&zeny);
        text(layer, zeny, [zeny_x, layout.extra], BLACK);
        let weight_x = zeny_x - WEIGHT_GAP - font.width(&weight_text);
        text(layer, weight_text, [weight_x, layout.extra], if heavy(weight, limit, info.overweight) { RED } else { BLACK });

        // The menu toggle, and the menu when it's open.
        let marked = info.unread_mail || info.reward_waiting || info.new_items || info.new_level || info.new_job_level;
        let toggle = match &assets.toggle_new {
            Some(new) if !self.menu_open && marked => new,
            _ => &assets.toggles[usize::from(self.menu_open)],
        };
        layer.sprites.push((toggle, at([0.0, self.toggle_top()]), toggle.size));
        if self.menu_open {
            layer.sprites.push((&assets.menu, at([0.0, self.menu_top()]), assets.menu.size));
            for (place, name) in visible(self.features).into_iter().enumerate() {
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
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lays_out_by_job() {
        assert!(!fourth_job(0) && !fourth_job(4054) && fourth_job(4252) && fourth_job(4329) && fourth_job(4360) && !fourth_job(4300));
        let normal = layout(false);
        assert_eq!(
            (normal.height, normal.mini, normal.name, normal.bars[1], normal.levels[1], normal.exp_bars[0]),
            (134.0, 55.0, [9.0, 22.0], [35.0, 68.0], [17.0, 96.0], [85.0, 89.0])
        );
        assert_eq!((layout(true).height, layout(true).mini), (150.0, 71.0));
        // The menu toggle follows the window's height.
        let mut window = BasicInfo::default();
        assert_eq!(window.height(), 134.0 + MENU_AT);
        window.fourth = true;
        assert_eq!(window.height(), 150.0 + MENU_AT);
    }

    #[test]
    fn shows_the_games_buttons_on_its_grid() {
        let base = visible(Features::default());
        assert_eq!(base, ["status", "equip", "item", "skill", "party", "guild", "quest", "map", "navigation", "option", "bank", "mail", "keyboard", "repute"]);
        // Battle and shop come in their places in the game's order.
        let more = visible(Features { battle: true, shop: true });
        assert_eq!(more.len(), 16);
        assert_eq!((more[6], more[13]), ("battle", "shop"));
        // Five a row, 42 and 43 apart, from (9, 23) under the window.
        let mut window = BasicInfo::default();
        assert_eq!(
            (window.button_corner(0), window.button_corner(4), window.button_corner(5), window.button_corner(13)),
            ([9.0, 157.0], [177.0, 157.0], [9.0, 200.0], [135.0, 243.0])
        );
        window.frame.open = true;
        window.menu_open = true;
        let corner = window.button_corner(6);
        assert_eq!(window.hit(window.frame.at([corner[0] + 5.0, corner[1] + 5.0])), Some(Hit::Menu("quest")));
        assert_eq!(window.menu_boxes().len(), 14);
        assert_eq!(window.height(), 134.0 + 23.0 + 3.0 * 43.0);
        window.fourth = true;
        assert_eq!(window.button_corner(0), [9.0, 173.0]);
    }

    #[test]
    fn fills_bars_and_says_numbers() {
        assert_eq!(bar_fill(40, 40), Some((127.0, false)));
        assert_eq!(bar_fill(9, 40), Some((28.0, true)), "under a quarter: red");
        assert_eq!(bar_fill(10, 40), Some((31.0, false)));
        assert_eq!((bar_fill(41, 40), bar_fill(0, 40), bar_fill(5, 0)), (None, None, None));
        assert_eq!(bar_text(7, 40), "  7  /   40");
        assert_eq!(zeny_text(1234567), "Zeny : 1,234,567");
        assert_eq!(zeny_text(999), "Zeny : 999");
        // Red at the overweight point, which the server may move.
        assert!(!heavy(999, 2000, 50) && heavy(1000, 2000, 50) && heavy(1800, 2000, 90) && !heavy(1799, 2000, 90) && !heavy(5, 0, 50));
        assert_eq!(mini_lines(5, "Novice", 3, 25, 200)[0], "Lv. 5 / Novice / Lv. 3 / Exp. 12.5 %");
        assert_eq!(mini_lines(99, "Novice", 10, 0, 0)[1], "Lv99 / Novice / Lv10 / Exp100.0%");
    }
}
