//! A skill's description (a right click on a skill in the skill window or on the hotkey bar): its name and
//! requirements on top, its effects under them, in the data's colours.

use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};
use crate::window::{self, Chrome};
use crate::{Layer, SCREEN, contains};

pub const WIDTH: f32 = 304.0;
const LINE: f32 = 18.0;
/// The header holds the first two lines, 273 wide; the body the rest, 298 wide.
const HEADER: [f32; 2] = [3.0, 3.0];
const HEADER_WIDTH: f32 = 273.0;
const HEADER_LINES: usize = 2;
const BODY_WIDTH: f32 = 298.0;
const ESC: [f32; 2] = [WIDTH - 14.0, 3.0];
const ESC_SIZE: [f32; 2] = [12.0, 12.0];
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

/// The window's height for this many lines: 18 a line, plus the frames, at most the screen less 50.
pub fn height(lines: usize) -> f32 {
    (LINE * lines as f32 + 4.0 + 6.0).min(SCREEN[1] - 50.0)
}

/// Opened from the hotkey bar: at the pointer's x (kept on screen), under the slot's row, or over it when the bar is
/// in the lower half of the screen.
pub fn at_hotbar(pointer_x: f32, row: [f32; 2], height: f32) -> [f32; 2] {
    let x = pointer_x.clamp(0.0, SCREEN[0] - WIDTH);
    let y = if row[0] < SCREEN[1] / 2.0 { row[1] } else { row[0] - height };
    [x, y.max(0.0)]
}

/// Opened from the skill window: right of the pointer, kept on screen.
// ponytail: "the control's screen position + the mouse + (24, 0)" read as the pointer + (24, 0).
pub fn at_window(pointer: [f32; 2], height: f32) -> [f32; 2] {
    [(pointer[0] + 24.0).clamp(0.0, SCREEN[0] - WIDTH), pointer[1].clamp(0.0, (SCREEN[1] - height).max(0.0))]
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
}

impl SkillDescription {
    pub fn new(skill: u16, lines: &[String], place: impl Fn(f32) -> [f32; 2]) -> Self {
        let lines = colored(lines);
        let corner = place(height(lines.len()));
        Self { skill, corner, lines, scroll: 0 }
    }

    pub fn size(&self) -> [f32; 2] {
        [WIDTH, height(self.lines.len())]
    }

    pub fn contains(&self, cursor: [f32; 2]) -> bool {
        contains(self.corner, self.size(), cursor)
    }

    pub fn on_close(&self, cursor: [f32; 2]) -> bool {
        contains([self.corner[0] + ESC[0], self.corner[1] + ESC[1]], ESC_SIZE, cursor)
    }

    /// The close button on screen (for the test driver).
    pub fn close_box(&self) -> [f32; 4] {
        [self.corner[0] + ESC[0], self.corner[1] + ESC[1], ESC_SIZE[0], ESC_SIZE[1]]
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

    pub fn draw<'a>(&self, layer: &mut Layer<'a>, chrome: &'a Chrome, assets: &'a Assets, font: &Font, pointer: [f32; 2]) {
        let [x, y] = self.corner;
        let [w, h] = self.size();
        // ponytail: the rounded corners are left square.
        layer.sprites.push((&assets.frame, [x, y], [w, h]));
        layer.sprites.push((&chrome.white, [x + 2.0, y + 2.0], [w - 4.0, h - 4.0]));
        let esc_color = if self.on_close(pointer) { [0x31, 0x6A, 0xC5] } else { [0x60, 0x60, 0x60] };
        layer.texts.push(("x".to_owned(), [x + ESC[0] + 2.0, y + ESC[1] - 2.0], esc_color));
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
        assert_eq!(height(10), 190.0);
        assert_eq!(height(100), SCREEN[1] - 50.0, "capped; the body scrolls");
        // From a hotbar row near the top: under it; near the bottom: over it; x kept on screen.
        assert_eq!(at_hotbar(100.0, [10.0, 42.0], 190.0), [100.0, 42.0]);
        assert_eq!(at_hotbar(900.0, [700.0, 732.0], 190.0), [SCREEN[0] - WIDTH, 510.0]);
        assert_eq!(at_window([300.0, 200.0], 190.0), [324.0, 200.0]);
    }
}
