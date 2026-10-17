//! The basic information window (Alt+V): name, job, HP and SP, levels and experience, weight and zeny, and the menu
//! of the other windows under it.

use std::collections::HashMap;

use roclient::grf::Grf;
use roclient::net;
use roclient::text::Font;

use crate::render::{Gpu, Texture};
use crate::window::{Chrome, Frame, TITLE_HEIGHT};
use crate::{Layer, UI, contains, offset};

// ponytail: places over w_basewin_bg2.bmp; calibrate against the real client.
const WIDTH: f32 = 220.0;
const HEIGHT: f32 = 150.0;
const BASE_BUTTON: [f32; 2] = [4.0, 3.0];
const MINIMIZE_BUTTON: [f32; 2] = [207.0, 3.0];
const BUTTON_SIZE: [f32; 2] = [11.0, 11.0];
const TITLE: [f32; 2] = [18.0, 2.0];
const NAME: [f32; 2] = [10.0, 20.0];
const JOB: [f32; 2] = [10.0, 33.0];
/// HP and SP: label, bar (135x9, 127px of it for 100%) and the percentage at the right.
const BAR_LABELS: [[f32; 2]; 2] = [[15.0, 50.0], [15.0, 65.0]];
const BARS: [[f32; 2]; 2] = [[35.0, 53.0], [35.0, 68.0]];
const BAR_FULL: f32 = 127.0;
const PERCENT_RIGHT: f32 = WIDTH - 20.0;
const LEVELS: [[f32; 2]; 2] = [[15.0, 101.0], [15.0, 112.0]];
const EXP_BARS: [[f32; 2]; 2] = [[84.0, 104.0], [84.0, 116.0]];
const EXP_SIZE: [f32; 2] = [110.0, 4.0];
const EXTRA_TOP: f32 = 134.0;
const EXTRA_RIGHT: f32 = WIDTH - 5.0;
/// The menu toggle under the window, and the icons it opens, five to a row.
const MENU_TOGGLE_TOP: f32 = HEIGHT;
const MENU_TOP: f32 = HEIGHT + 10.0;
const MENU_CELL: f32 = 44.0;
const MENU_ICON: [f32; 2] = [6.0, 6.0];
const MENU_COLUMNS: usize = 5;
const BASIC_INFO_MESSAGE: usize = 238;
const EXP_BORDER: [u8; 4] = [0xAF, 0xAF, 0xAF, 0xFF];
const EXP_FILL: [u8; 4] = [0x42, 0x62, 0xA5, 0xFF];
const RED: [u8; 3] = [0xFF, 0x00, 0x00];

/// The menu's buttons in order (menu_icon\bt_<name>.bmp).
pub const MENU: [&str; 20] = [
    "status",
    "equip",
    "item",
    "skill",
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
    "keyboard",
    "attendance",
    "adventureragency",
    "repute",
];

pub struct Assets {
    background: Texture,
    /// Blue and red bar pieces: left cap, middle, right cap.
    bars: [[Texture; 3]; 2],
    exp_border: Texture,
    exp_fill: Texture,
    /// Menu toggle closed and open, the menu's back and its buttons.
    toggles: [Texture; 2],
    menu: Texture,
    icons: Vec<Option<Texture>>,
    /// The mail button while unread mail waits.
    mail_new: Option<Texture>,
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let basic = format!("{UI}basic_interface\\");
        let menu = format!("{UI}menu_icon\\");
        let image = |path: String| gpu.load_texture(grf, &path, false);
        Self {
            background: image(format!("{basic}w_basewin_bg2.bmp")),
            bars: ["blue", "red"].map(|color| ["left", "mid", "right"].map(|part| image(format!("{basic}gze{color}_{part}.bmp")))),
            exp_border: gpu.solid(EXP_BORDER),
            exp_fill: gpu.solid(EXP_FILL),
            toggles: [image(format!("{menu}bt_menu_normal.bmp")), image(format!("{menu}bt_menu_close_normal.bmp"))],
            menu: image(format!("{menu}bg_menu.tga")),
            icons: MENU.iter().map(|name| gpu.try_load_texture(grf, &format!("{menu}bt_{name}.bmp"), false).ok()).collect(),
            mail_new: gpu.try_load_texture(grf, &format!("{menu}bt_mail_new.bmp"), false).ok(),
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title,
    Minimize,
    MenuToggle,
    /// A menu button, by its place in `MENU`.
    Menu(usize),
    Window,
}

pub struct BasicInfo {
    pub frame: Frame,
    pub menu_open: bool,
}

impl Default for BasicInfo {
    fn default() -> Self {
        let mut frame = Frame::new([0.0, 0.0], WIDTH);
        frame.open = true;
        Self { frame, menu_open: false }
    }
}

/// What the window shows, from the player's status values.
pub struct Info<'a> {
    pub name: &'a str,
    pub job: &'a str,
    pub status: &'a HashMap<u16, i64>,
    pub unread_mail: bool,
}

impl BasicInfo {
    /// The window, plus the menu toggle and the open menu under it.
    pub fn height(&self) -> f32 {
        if self.frame.minimized {
            TITLE_HEIGHT
        } else if self.menu_open {
            MENU_TOP + MENU.len().div_ceil(MENU_COLUMNS) as f32 * MENU_CELL + MENU_ICON[1]
        } else {
            MENU_TOGGLE_TOP + 9.0
        }
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let local = self.frame.local(cursor, self.height())?;
        Some(if contains(MINIMIZE_BUTTON, BUTTON_SIZE, local) {
            Hit::Minimize
        } else if local[1] < TITLE_HEIGHT {
            Hit::Title
        } else if (MENU_TOGGLE_TOP..MENU_TOP).contains(&local[1]) {
            Hit::MenuToggle
        } else if self.menu_open && local[1] >= MENU_TOP {
            let [column, row] = [((local[0] - MENU_ICON[0]) / MENU_CELL).floor(), ((local[1] - MENU_TOP - MENU_ICON[1]) / MENU_CELL).floor()];
            let button = row as usize * MENU_COLUMNS + column as usize;
            if column >= 0.0 && row >= 0.0 && (column as usize) < MENU_COLUMNS && button < MENU.len() { Hit::Menu(button) } else { Hit::Window }
        } else {
            Hit::Window
        })
    }

    /// Reacts to a click; returns the menu button pressed.
    pub fn click(&mut self, hit: Hit) -> Option<usize> {
        match hit {
            Hit::Minimize => self.frame.minimized = !self.frame.minimized,
            Hit::MenuToggle => self.menu_open = !self.menu_open,
            Hit::Menu(button) => return Some(button),
            Hit::Title | Hit::Window => {}
        }
        None
    }

    pub fn draw<'a>(&self, layer: &mut Layer<'a>, chrome: &'a Chrome, assets: &'a Assets, font: &Font, messages: &[String], info: &Info, cursor: [f32; 2]) {
        if !self.frame.open {
            return;
        }
        let at = |local: [f32; 2]| self.frame.at(local);
        let value = |id: u16| info.status.get(&id).copied().unwrap_or(0);
        // Text with the white shadow the window uses.
        let shadowed = |layer: &mut Layer<'a>, text: String, place: [f32; 2], color: [u8; 3]| {
            layer.texts.push((text.clone(), offset(at(place), [1.0, 1.0]), [255; 3]));
            layer.texts.push((text, at(place), color));
        };

        layer.sprites.push((&assets.background, at([0.0, 0.0]), if self.frame.minimized { [WIDTH, TITLE_HEIGHT] } else { assets.background.size }));
        for place in [BASE_BUTTON, MINIMIZE_BUTTON] {
            let button = usize::from(place == MINIMIZE_BUTTON);
            let image = chrome.button(button, contains(at(place), BUTTON_SIZE, cursor));
            layer.sprites.push((image, at(place), image.size));
        }
        shadowed(layer, messages.get(BASIC_INFO_MESSAGE).cloned().unwrap_or_default(), TITLE, [0, 0, 0]);
        if self.frame.minimized {
            return;
        }

        shadowed(layer, info.name.to_owned(), NAME, [0, 0, 0]);
        shadowed(layer, info.job.to_owned(), JOB, [0, 0, 0]);
        for (bar, (now, most)) in [(net::STATUS_HP, net::STATUS_MAX_HP), (net::STATUS_SP, net::STATUS_MAX_SP)].into_iter().enumerate() {
            let (now, most) = (value(now), value(most));
            shadowed(layer, ["HP", "SP"][bar].to_owned(), BAR_LABELS[bar], [0, 0, 0]);
            let percent = if most > 0 { (now * 100 / most).clamp(0, 100) } else { 0 };
            if percent > 0 {
                let [left, middle, right] = &assets.bars[usize::from(percent < 25)];
                let width = (percent as f32 * BAR_FULL / 100.0).floor();
                layer.sprites.push((left, at(BARS[bar]), left.size));
                layer.sprites.push((middle, at(offset(BARS[bar], [left.size[0], 0.0])), [width, middle.size[1]]));
                layer.sprites.push((right, at(offset(BARS[bar], [left.size[0] + width, 0.0])), right.size));
            }
            let numbers = format!("{now} / {most}");
            let numbers_x = BARS[bar][0] + 4.0 + (BAR_FULL - font.width(&numbers)) / 2.0;
            layer.texts.push((numbers, at([numbers_x.round(), BARS[bar][1] - 3.0]), [0, 0, 0]));
            let percent = format!("{percent}%");
            shadowed(layer, percent.clone(), [PERCENT_RIGHT - font.width(&percent), BAR_LABELS[bar][1]], [0, 0, 0]);
        }

        let levels = [
            (net::STATUS_BASE_LEVEL, "Base Lv.", net::STATUS_BASE_EXP, net::STATUS_NEXT_BASE_EXP),
            (net::STATUS_JOB_LEVEL, "Job Lv.", net::STATUS_JOB_EXP, net::STATUS_NEXT_JOB_EXP),
        ];
        for (row, (level, label, exp, next)) in levels.into_iter().enumerate() {
            shadowed(layer, format!("{label} {}", value(level)), LEVELS[row], [0, 0, 0]);
            let next = value(next);
            if next > 0 {
                layer.sprites.push((&assets.exp_border, at(offset(EXP_BARS[row], [-1.0, -1.0])), [EXP_SIZE[0] + 2.0, EXP_SIZE[1] + 2.0]));
                layer.sprites.push((&chrome.white, at(EXP_BARS[row]), EXP_SIZE));
                let filled = (value(exp) as f32 / next as f32).clamp(0.0, 1.0) * EXP_SIZE[0];
                layer.sprites.push((&assets.exp_fill, at(EXP_BARS[row]), [filled.floor(), EXP_SIZE[1]]));
            }
        }

        // Weight turns red at half the limit; weight comes in tenths.
        let (weight, limit) = (value(net::STATUS_WEIGHT), value(net::STATUS_MAX_WEIGHT));
        let zeny = format!("Zeny : {}", value(net::STATUS_ZENY));
        let weight_text = format!("Weight : {} / {}", weight / 10, limit / 10);
        let zeny_x = EXTRA_RIGHT - font.width(&zeny);
        shadowed(layer, zeny, [zeny_x, EXTRA_TOP], [0, 0, 0]);
        let weight_x = zeny_x - 6.0 - font.width(&weight_text);
        shadowed(layer, weight_text, [weight_x, EXTRA_TOP], if limit > 0 && weight * 2 >= limit { RED } else { [0, 0, 0] });

        // The menu toggle, and the menu when it's open.
        let toggle = &assets.toggles[usize::from(self.menu_open)];
        layer.sprites.push((toggle, at([0.0, MENU_TOGGLE_TOP]), toggle.size));
        if self.menu_open {
            layer.sprites.push((&assets.menu, at([0.0, MENU_TOP]), assets.menu.size));
            for (button, icon) in assets.icons.iter().enumerate() {
                let icon = if MENU[button] == "mail" && info.unread_mail { assets.mail_new.as_ref().or(icon.as_ref()) } else { icon.as_ref() };
                if let Some(icon) = icon {
                    let place =
                        [MENU_ICON[0] + (button % MENU_COLUMNS) as f32 * MENU_CELL, MENU_TOP + MENU_ICON[1] + (button / MENU_COLUMNS) as f32 * MENU_CELL];
                    layer.sprites.push((icon, at(place), icon.size));
                }
            }
        }
    }
}
