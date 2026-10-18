//! The status window (Alt+A): the six stats with their bonuses, the arrows that raise them and what that costs, and
//! Atk, Matk, Hit, Critical, Def, Mdef, Flee, ASPD, status points and guild, over the client's own background.

use std::collections::HashMap;

use rustnarok_client::grf::Grf;
use rustnarok_client::net::{STATUS_ATK, STATUS_BONUS, STATUS_POINTS, STATUS_STR, STATUS_STR_COST};
use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};
use crate::window::{Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI, contains};

// ponytail: places over statuswnd\w_statwin_bg.bmp, rows 16px apart from 6px down; calibrate against the real client.
const WIDTH: f32 = 280.0;
const ROW: f32 = 16.0;
const FIRST: f32 = 6.0;
const VALUE: f32 = 40.0;
const BONUS: f32 = 56.0;
const ARROW: f32 = 77.0;
/// How much around an arrow a click takes.
const ARROW_BOX: [f32; 2] = [11.0, 11.0];
const COST_RIGHT: f32 = 101.0;
const LEFT_COLUMN_RIGHT: f32 = 192.0;
const RIGHT_COLUMN_RIGHT: f32 = 275.0;
const POSITION: [f32; 2] = [200.0, 200.0];
const TITLE_MESSAGE: usize = 105;
const TEXT: [u8; 3] = [0, 0, 0];

pub struct Assets {
    background: Texture,
    /// The arrow, and pressed.
    arrow: [Texture; 2],
    expand: Texture,
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |name: &str| gpu.load_texture(grf, &format!("{UI}{name}.bmp"), false);
        Self {
            background: image("statuswnd\\w_statwin_bg"),
            arrow: [image("basic_interface\\arw_right"), image("basic_interface\\arw_right_on")],
            expand: image("statuswnd\\expand_on_normal"),
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(TitleHit),
    /// The arrow raising a stat (STATUS_STR and on).
    Raise(u16),
    Window,
}

pub struct StatusWindow {
    pub frame: Frame,
}

impl Default for StatusWindow {
    fn default() -> Self {
        Self { frame: Frame::new(POSITION, WIDTH).collapsing() }
    }
}

/// The six stats' short names, Str to Luk.
pub const STATS: [&str; 6] = ["str", "agi", "vit", "int", "dex", "luk"];

/// Where the arrow raising a stat (by row, Str first) sits in the window.
fn arrow_corner(row: u16) -> [f32; 2] {
    [ARROW, TITLE_HEIGHT + FIRST + f32::from(row) * ROW]
}

/// A value with its bonus the way the window writes it: "12 + 3", "12 - 3".
fn pair(base: i64, bonus: i64) -> String {
    if bonus < 0 { format!("{base} - {}", -bonus) } else { format!("{base} + {bonus}") }
}

/// What a stat's arrow does: raise it, when there are points enough.
fn can_raise(status: &HashMap<u16, i64>, stat: u16) -> bool {
    let cost = status.get(&(STATUS_STR_COST + stat - STATUS_STR)).copied().unwrap_or(0);
    cost > 0 && cost <= status.get(&STATUS_POINTS).copied().unwrap_or(0)
}

impl StatusWindow {
    pub fn height(&self, assets: &Assets) -> f32 {
        TITLE_HEIGHT + assets.background.size[1]
    }

    pub fn hit(&self, cursor: [f32; 2], assets: &Assets, status: &HashMap<u16, i64>) -> Option<Hit> {
        let local = self.frame.local(cursor, self.height(assets))?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(Hit::Title(hit));
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

    pub fn click(&mut self, hit: Hit) -> Option<u16> {
        match hit {
            Hit::Title(title) => self.frame.click_title(title),
            Hit::Raise(stat) => return Some(stat),
            Hit::Window => {}
        }
        None
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &self,
        layer: &mut Layer<'a>,
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
        let top = TITLE_HEIGHT;
        layer.sprites.push((&assets.background, at([0.0, top]), assets.background.size));
        let hit = self.hit(pointer.0, assets, status);
        let text_top = |row: usize| top + FIRST + row as f32 * ROW + ((ROW - 2.0 - font.line_height()) / 2.0).floor();
        let right = |layer: &mut Layer<'a>, text: String, edge: f32, row: usize| {
            layer.texts.push((text.clone(), at([edge - font.width(&text), text_top(row)]), TEXT));
        };

        // Str to Luk: the value, the bonus from gear, the arrow while points last, and what going up costs.
        for row in 0..6u16 {
            let stat = STATUS_STR + row;
            let line = usize::from(row);
            layer.texts.push((value(stat).to_string(), at([VALUE, text_top(line)]), TEXT));
            let bonus = value(stat + STATUS_BONUS);
            if bonus != 0 {
                let text = if bonus > 0 { format!("+{bonus}") } else { format!("- {}", -bonus) };
                layer.texts.push((text, at([BONUS, text_top(line)]), TEXT));
            }
            if can_raise(status, stat) {
                let pressed = pointer.1 && hit == Some(Hit::Raise(stat));
                let arrow = &assets.arrow[usize::from(pressed)];
                layer.sprites.push((arrow, at([ARROW, top + FIRST + f32::from(row) * ROW + 2.0]), arrow.size));
            }
            right(layer, value(STATUS_STR_COST + row).to_string(), COST_RIGHT, line);
        }

        // Atk (base + gear), Matk, Hit, Critical on the left; Def, Mdef, Flee, ASPD on the right; then points, guild.
        let [atk, atk2, matk1, matk2, def, def2, mdef, mdef2, hit_rate, flee, flee2, critical, aspd] =
            std::array::from_fn(|place| value(STATUS_ATK + place as u16));
        for (row, text) in [pair(atk, atk2), pair(matk2, matk1), hit_rate.to_string(), critical.to_string()].into_iter().enumerate() {
            right(layer, text, LEFT_COLUMN_RIGHT, row);
        }
        let aspd = (200.0 - aspd as f32 / 10.0).floor();
        for (row, text) in [pair(def, def2), pair(mdef, mdef2), pair(flee, flee2), aspd.to_string()].into_iter().enumerate() {
            right(layer, text, RIGHT_COLUMN_RIGHT, row);
        }
        right(layer, value(STATUS_POINTS).to_string(), RIGHT_COLUMN_RIGHT, 4);
        right(layer, guild.to_owned(), RIGHT_COLUMN_RIGHT, 5);
        // ponytail: the trait panel (4th jobs) behind this bar isn't made yet.
        layer.sprites.push((&assets.expand, at([2.0, top + assets.background.size[1] - assets.expand.size[1] - 3.0]), assets.expand.size));
        if let Some(text) = messages.get(3613) {
            layer.texts.push((text.clone(), at([22.0, top + assets.background.size[1] - assets.expand.size[1] - 1.0]), TEXT));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raises_only_what_points_pay_for() {
        let status = HashMap::from([(STATUS_POINTS, 3), (STATUS_STR_COST, 2), (STATUS_STR_COST + 1, 4)]);
        assert!(can_raise(&status, STATUS_STR));
        assert!(!can_raise(&status, STATUS_STR + 1));
        assert_eq!(pair(10, -2), "10 - 2");
        assert_eq!(pair(10, 3), "10 + 3");
    }

    #[test]
    fn names_the_arrows_points_pay_for() {
        // 5 points: Int (cost 4) and Luk (cost 2) can rise, Str (cost 9) can't, and Agi, Vit and Dex have no cost known.
        let status = HashMap::from([(STATUS_POINTS, 5), (STATUS_STR_COST, 9), (STATUS_STR_COST + 3, 4), (STATUS_STR_COST + 5, 2)]);
        let mut window = StatusWindow::default();
        assert!(window.raise_boxes(&status).is_empty());
        window.frame.open = true;
        let [x, y] = POSITION;
        let row = |row: f32| y + TITLE_HEIGHT + FIRST + row * ROW;
        assert_eq!(
            window.raise_boxes(&status),
            [("raise_int".to_owned(), [x + ARROW, row(3.0), 11.0, 11.0]), ("raise_luk".to_owned(), [x + ARROW, row(5.0), 11.0, 11.0])]
        );
        window.frame.minimized = true;
        assert!(window.raise_boxes(&status).is_empty());
    }
}
