//! A skill's description (a right click on a skill in the skill window, on the hotkey bar or in the guild window's
//! skills): its name and requirements on top, its effects under them, in the data's colours. An active skill's has a
//! button that opens the skill delays beside it.

use std::collections::HashMap;

use rustnarok_client::text::Font;

use crate::lua::SkillInfo;
use crate::render::{Gpu, Texture};
use crate::skills::Cooling;
use crate::window::{self, Chrome};
use crate::{Layer, contains, screen};

pub const WIDTH: f32 = 304.0;
const LINE: f32 = 18.0;
/// The header holds the first two lines, 273 wide; the body the rest, 298 wide.
const HEADER: [f32; 2] = [3.0, 3.0];
const HEADER_WIDTH: f32 = 273.0;
const HEADER_LINES: usize = 2;
const BODY_WIDTH: f32 = 298.0;
/// The standard close button at (w − 14, 3).
const ESC: [f32; 2] = [WIDTH - 14.0, 3.0];
const ESC_SIZE: [f32; 2] = [11.0, 11.0];
// ponytail: this data has no skilldelayinfo pictures nor their messages (4254–4258): a plain framed button, English
// words, and a list of the skills cooling down with the time left.
/// The delay button at (w − its width − 4, 18).
const DELAY_SIZE: [f32; 2] = [40.0, 15.0];
const DELAY: [f32; 2] = [WIDTH - DELAY_SIZE[0] - 4.0, 18.0];
const DELAY_LABEL: &str = "Delay";
const DELAY_TITLE: &str = "Skill Delay";
const DELAY_NONE: &str = "No skill is cooling down.";
/// The delay window beside the description: a title and this many skills.
const DELAYS_WIDTH: f32 = 180.0;
const DELAYS_ROWS: usize = 8;
const FRAME: [u8; 4] = [195, 195, 195, 255];
const UNKNOWN: &str = "Unknown Skill";

/// One line's runs of text, each in its colour.
pub type Line = Vec<([u8; 3], String)>;

/// The data's lines with their `^RRGGBB` codes applied; a colour carries on to the next line until another.
// ponytail: that a colour carries across lines is read from the data (a grey sentence split over two lines).
pub fn colored(lines: &[String]) -> Vec<Line> {
    if lines.is_empty() {
        return vec![vec![([0, 0, 0], UNKNOWN.to_owned())]];
    }
    let mut color = [0, 0, 0];
    lines
        .iter()
        .map(|text| {
            let mut runs: Line = Vec::new();
            let mut rest = text.as_str();
            while !rest.is_empty() {
                if let Some(code) = rest
                    .strip_prefix('^')
                    .filter(|code| code.len() >= 6 && code.is_char_boundary(6) && code[..6].chars().all(|digit| digit.is_ascii_hexdigit()))
                {
                    let value = u32::from_str_radix(&code[..6], 16).unwrap();
                    color = [(value >> 16) as u8, (value >> 8) as u8, value as u8];
                    rest = &code[6..];
                    continue;
                }
                let end = rest[1..].find('^').map_or(rest.len(), |index| index + 1);
                runs.push((color, rest[..end].to_owned()));
                rest = &rest[end..];
            }
            runs
        })
        .collect()
}

/// The window's height for this many lines: the text 18 a line plus 4, at most the screen less 50 (the body
/// scrolls), and the frames' 6.
pub fn height(lines: usize) -> f32 {
    (LINE * lines as f32 + 4.0).min(screen()[1] - 50.0) + 6.0
}

/// Opened from the hotkey bar (its top at `bar_y`, rows 33 apart): at the pointer's x, shifted left to fit, under
/// the row, or over it when the bar is in the lower half of the screen.
pub fn at_hotbar(pointer_x: f32, bar_y: f32, row: usize, height: f32) -> [f32; 2] {
    let x = pointer_x.clamp(0.0, screen()[0] - WIDTH);
    let row_top = bar_y + 33.0 * row as f32;
    let y = if bar_y > screen()[1] / 2.0 { row_top - height } else { row_top + 33.0 };
    [x, y.max(0.0)]
}

/// Opened from the skill window: right of the pointer, kept on screen.
// ponytail: "the control's screen position + the mouse + (24, 0)" read as the pointer + (24, 0).
pub fn at_window(pointer: [f32; 2], height: f32) -> [f32; 2] {
    // 24 px right of the pointer, or left of it when it wouldn't fit.
    let right = pointer[0] + 24.0;
    let x = if right + WIDTH <= screen()[0] { right } else { (pointer[0] - 24.0 - WIDTH).max(0.0) };
    [x, pointer[1].clamp(0.0, (screen()[1] - height).max(0.0))]
}

pub struct Assets {
    frame: Texture,
}

impl Assets {
    pub fn load(gpu: &Gpu) -> Self {
        Self { frame: gpu.solid(FRAME) }
    }
}

pub struct SkillDescription {
    pub skill: u16,
    pub corner: [f32; 2],
    lines: Vec<Line>,
    /// Body lines scrolled past.
    scroll: usize,
    /// It has the close button (not when opened from the skill window: Esc closes it there).
    closable: bool,
    /// An active skill: the delay button shows.
    active: bool,
    /// The skill delay window is open beside it.
    pub delays: bool,
}

impl SkillDescription {
    pub fn new(skill: u16, lines: &[String], closable: bool, active: bool, place: impl Fn(f32) -> [f32; 2]) -> Self {
        let lines = colored(lines);
        let corner = place(height(lines.len()));
        Self { skill, corner, lines, scroll: 0, closable, active, delays: false }
    }

    pub fn size(&self) -> [f32; 2] {
        [WIDTH, height(self.lines.len())]
    }

    /// The delay window's box: at the description's right edge, or its left when that's off screen.
    fn delays_box(&self) -> [f32; 4] {
        let right = self.corner[0] + WIDTH;
        let x = if right + DELAYS_WIDTH <= screen()[0] { right } else { (self.corner[0] - DELAYS_WIDTH).max(0.0) };
        [x, self.corner[1], DELAYS_WIDTH, 10.0 + LINE * (DELAYS_ROWS + 1) as f32]
    }

    pub fn contains(&self, cursor: [f32; 2]) -> bool {
        let [x, y, w, h] = self.delays_box();
        contains(self.corner, self.size(), cursor) || (self.delays && contains([x, y], [w, h], cursor))
    }

    pub fn on_close(&self, cursor: [f32; 2]) -> bool {
        self.closable && contains([self.corner[0] + ESC[0], self.corner[1] + ESC[1]], ESC_SIZE, cursor)
    }

    /// The close button on screen (for the test driver), when it has one.
    pub fn close_box(&self) -> Option<[f32; 4]> {
        self.closable.then(|| [self.corner[0] + ESC[0], self.corner[1] + ESC[1], ESC_SIZE[0], ESC_SIZE[1]])
    }

    pub fn on_delay(&self, cursor: [f32; 2]) -> bool {
        self.active && contains([self.corner[0] + DELAY[0], self.corner[1] + DELAY[1]], DELAY_SIZE, cursor)
    }

    /// The delay button on screen (for the test driver), for an active skill.
    pub fn delay_box(&self) -> Option<[f32; 4]> {
        self.active.then(|| [self.corner[0] + DELAY[0], self.corner[1] + DELAY[1], DELAY_SIZE[0], DELAY_SIZE[1]])
    }

    /// The body lines that fit, after the header.
    fn shown_body(&self) -> usize {
        let room = ((self.size()[1] - 10.0) / LINE) as usize;
        room.saturating_sub(HEADER_LINES.min(self.lines.len()))
    }

    pub fn scroll_by(&mut self, rows: i32) {
        let body = self.lines.len().saturating_sub(HEADER_LINES);
        self.scroll = (self.scroll as i32 + rows).clamp(0, body.saturating_sub(self.shown_body()) as i32) as usize;
    }

    /// The text shown, for the control state.
    pub fn text(&self) -> Vec<String> {
        self.lines.iter().map(|line| line.iter().map(|(_, text)| text.as_str()).collect()).collect()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &self,
        layer: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        font: &Font,
        pointer: [f32; 2],
        cooling: &Cooling,
        skill_info: &HashMap<u16, SkillInfo>,
    ) {
        let [x, y] = self.corner;
        let [w, h] = self.size();
        // ponytail: the rounded corners are left square.
        layer.sprites.push((&assets.frame, [x, y], [w, h]));
        layer.sprites.push((&chrome.white, [x + 2.0, y + 2.0], [w - 4.0, h - 4.0]));
        if self.closable {
            let button = chrome.button(2, self.on_close(pointer));
            layer.sprites.push((button, [x + ESC[0], y + ESC[1]], button.size));
        }
        if self.active {
            let [bx, by] = [x + DELAY[0], y + DELAY[1]];
            layer.sprites.push((&chrome.border, [bx, by], DELAY_SIZE));
            layer.sprites.push((&chrome.white, [bx + 1.0, by + 1.0], [DELAY_SIZE[0] - 2.0, DELAY_SIZE[1] - 2.0]));
            let color = if self.on_delay(pointer) { [0x31, 0x6A, 0xC5] } else { [0, 0, 0] };
            layer.texts.push((DELAY_LABEL.to_owned(), [bx + ((DELAY_SIZE[0] - font.width(DELAY_LABEL)) / 2.0).floor(), by + 1.0], color));
        }
        if self.delays {
            // The skills cooling down, each with the seconds left.
            let name = |skill: u16| skill_info.get(&skill).map_or_else(|| skill.to_string(), |info| info.name.clone());
            let mut rows: Vec<(String, f32)> = cooling.cooldowns.blocked(cooling.now).map(|(skill, left)| (name(skill), left.as_secs_f32())).collect();
            rows.sort_by(|a, b| a.0.cmp(&b.0));
            rows.truncate(DELAYS_ROWS);
            let [dx, dy, dw, dh] = self.delays_box();
            layer.sprites.push((&assets.frame, [dx, dy], [dw, dh]));
            layer.sprites.push((&chrome.white, [dx + 2.0, dy + 2.0], [dw - 4.0, dh - 4.0]));
            layer.texts.push((DELAY_TITLE.to_owned(), [dx + 6.0, dy + 5.0], [0, 0, 0]));
            if rows.is_empty() {
                layer.texts.push((DELAY_NONE.to_owned(), [dx + 6.0, dy + 5.0 + LINE], [0x60; 3]));
            }
            for (row, (name, left)) in rows.into_iter().enumerate() {
                let top = dy + 5.0 + LINE * (row + 1) as f32;
                let time = format!("{left:.1}s");
                layer.texts.push((window::fit(font, &name, dw - 60.0), [dx + 6.0, top], [0, 0, 0]));
                layer.texts.push((time.clone(), [dx + dw - 8.0 - font.width(&time), top], [0x31, 0x6A, 0xC5]));
            }
        }
        let header = self.lines.iter().take(HEADER_LINES);
        let body = self.lines.iter().skip(HEADER_LINES).skip(self.scroll).take(self.shown_body());
        for (row, (line, width)) in header.map(|line| (line, HEADER_WIDTH)).chain(body.map(|line| (line, BODY_WIDTH))).enumerate() {
            let mut left = x + HEADER[0] + 3.0;
            let top = y + HEADER[1] + 2.0 + row as f32 * LINE;
            for (color, text) in line {
                let room = x + HEADER[0] + width - left;
                let shown = window::fit(font, text, room);
                let advance = font.width(&shown);
                layer.texts.push((shown, [left, top], *color));
                left += advance;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_lines_and_carries_them_over() {
        let lines = ["Bash".to_owned(), "Description: ^777777Hits".to_owned(), "hard.^000000 More".to_owned()];
        let shown = colored(&lines);
        assert_eq!(shown[0], [([0, 0, 0], "Bash".to_owned())]);
        assert_eq!(shown[1], [([0, 0, 0], "Description: ".to_owned()), ([0x77, 0x77, 0x77], "Hits".to_owned())]);
        assert_eq!(shown[2], [([0x77, 0x77, 0x77], "hard.".to_owned()), ([0, 0, 0], " More".to_owned())]);
        assert_eq!(colored(&[]), [vec![([0, 0, 0], UNKNOWN.to_owned())]]);
    }

    #[test]
    fn sizes_and_places_it() {
        assert_eq!(height(10), LINE * 10.0 + 10.0);
        // The frames are added after the cap.
        assert_eq!(height(100), screen()[1] - 44.0, "capped; the body scrolls");
        // A bar in the top half: under the slot's row; in the bottom half: over it; x kept on screen.
        assert_eq!(at_hotbar(100.0, 10.0, 0, 190.0), [100.0, 43.0]);
        assert_eq!(at_hotbar(100.0, 10.0, 1, 190.0), [100.0, 76.0]);
        assert_eq!(at_hotbar(screen()[0], 600.0, 1, 190.0), [screen()[0] - WIDTH, 443.0]);
        assert_eq!(at_window([300.0, 200.0], 190.0), [324.0, 200.0]);
        // Flipped left of the pointer when it wouldn't fit on the right.
        assert_eq!(at_window([screen()[0] - 50.0, 200.0], 190.0), [screen()[0] - 74.0 - WIDTH, 200.0]);
    }

    #[test]
    fn has_the_close_and_delay_buttons_only_where_they_belong() {
        let lines = ["Bash".to_owned()];
        let from_window = SkillDescription::new(5, &lines, false, true, |_| [100.0, 100.0]);
        assert_eq!(from_window.close_box(), None, "Esc closes it in the skill window");
        assert!(!from_window.on_close([100.0 + ESC[0] + 2.0, 100.0 + ESC[1] + 2.0]));
        let passive = SkillDescription::new(2, &lines, true, false, |_| [100.0, 100.0]);
        assert_eq!(passive.close_box(), Some([100.0 + WIDTH - 14.0, 103.0, 11.0, 11.0]));
        assert_eq!(passive.delay_box(), None, "a passive skill has no delay");
        assert_eq!(from_window.delay_box(), Some([100.0 + WIDTH - 44.0, 118.0, 40.0, 15.0]));
        // The delay window opens at the right edge, or the left when the right is off screen.
        let mut open = SkillDescription::new(5, &lines, true, true, |_| [100.0, 100.0]);
        open.delays = true;
        assert!(open.contains([100.0 + WIDTH + 10.0, 110.0]));
        open.corner = [screen()[0] - WIDTH, 100.0];
        assert_eq!(open.delays_box()[0], screen()[0] - WIDTH - DELAYS_WIDTH);
    }
}
