//! Chat macros: ten lines the player sets in the macro window (Alt+M) and sends with Alt+1 to Alt+0, as if typed and
//! sent with Enter. Kept in the settings folder's `macros` (`settings_path`), one a line.

use std::path::{Path, PathBuf};

use rustnarok_client::text::Font;

use crate::window::{Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, SCREEN, contains, offset};

pub const COUNT: usize = 10;

// ponytail: the official defaults are an estimate: the emotions Alt+1 to Alt+0 showed in older clients (help line 157,
// MSI_EXPLAIN_TIP10), emotions 0 to 9; replace them with the real client's if they differ.
/// What an unset line holds.
const DEFAULTS: [&str; COUNT] = ["/!", "/?", "/ho", "/lv", "/swt", "/ic", "/an", "/ag", "/$", "/..."];
/// The longest line, as the chat's.
const MOST: usize = 70;

pub fn path() -> Option<PathBuf> {
    crate::settings_path("macros")
}

/// The ten lines from `path`: one a line in the file; the defaults without a file, and for lines it doesn't have.
pub fn load(path: Option<&Path>) -> [String; COUNT] {
    let text = path.and_then(|path| std::fs::read_to_string(path).ok());
    let lines: Vec<&str> = text.as_deref().map(|text| text.lines().collect()).unwrap_or_default();
    std::array::from_fn(|n| lines.get(n).map_or(DEFAULTS[n], |line| line).to_owned())
}

pub fn save(path: Option<&Path>, lines: &[String; COUNT]) {
    let Some(path) = path else { return };
    let text = lines.iter().map(|line| format!("{line}\n")).collect::<String>();
    let _ = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(path, text));
}

// ponytail: ten rows on the window chrome (data.grf has no bitmaps for this window); place it against the real client.
const WIDTH: f32 = 300.0;
const ROW: f32 = 20.0;
const LABEL: f32 = 46.0;
const SIDE: f32 = 7.0;
const HEIGHT: f32 = TITLE_HEIGHT + 6.0 + COUNT as f32 * ROW + 6.0;
const TITLE: usize = 2351;

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(TitleHit),
    /// A line's field, 0 for Alt+1 to 9 for Alt+0.
    Line(usize),
    Window,
}

/// The macro window: which line is being edited, if one is.
#[derive(Default)]
pub struct Window {
    pub editing: Option<usize>,
}

impl Window {
    fn frame() -> Frame {
        let mut frame = Frame::new([((SCREEN[0] - WIDTH) / 2.0).floor(), ((SCREEN[1] - HEIGHT) / 2.0).floor()], WIDTH);
        frame.open = true;
        frame
    }

    fn field(line: usize) -> [f32; 2] {
        [SIDE + LABEL, TITLE_HEIGHT + 6.0 + line as f32 * ROW]
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let frame = Self::frame();
        let local = frame.local(cursor, HEIGHT)?;
        if let Some(hit) = frame.title_hit(local) {
            return Some(Hit::Title(hit));
        }
        let width = WIDTH - SIDE * 2.0 - LABEL;
        Some((0..COUNT).find(|&line| contains(Self::field(line), [width, ROW - 3.0], local)).map_or(Hit::Window, Hit::Line))
    }

    /// Types into the line being edited.
    pub fn type_text(&self, lines: &mut [String; COUNT], text: &str) {
        let Some(line) = self.editing else { return };
        let room = MOST - lines[line].chars().count().min(MOST);
        lines[line].extend(text.chars().filter(|character| !character.is_control()).take(room));
    }

    pub fn backspace(&self, lines: &mut [String; COUNT]) {
        if let Some(line) = self.editing {
            lines[line].pop();
        }
    }

    pub fn draw<'a>(
        &self,
        layer: &mut Layer<'a>,
        chrome: &'a Chrome,
        font: &Font,
        messages: &[String],
        lines: &[String; COUNT],
        pointer: [f32; 2],
        caret: bool,
    ) {
        let frame = Self::frame();
        frame.draw_title(layer, chrome, messages.get(TITLE).map_or("", String::as_str), pointer);
        layer.sprites.push((&chrome.white, frame.at([0.0, TITLE_HEIGHT]), [WIDTH, HEIGHT - TITLE_HEIGHT]));
        let width = WIDTH - SIDE * 2.0 - LABEL;
        for (n, line) in lines.iter().enumerate() {
            let field = Self::field(n);
            // Alt+1 … Alt+9, Alt+0.
            layer.texts.push((format!("Alt+{}", (n + 1) % 10), frame.at([SIDE, field[1] + 1.0]), [0, 0, 0]));
            layer.sprites.push((&chrome.border, frame.at(offset(field, [-1.0, -1.0])), [width + 2.0, ROW - 1.0]));
            layer.sprites.push((&chrome.white, frame.at(field), [width, ROW - 3.0]));
            let shown = crate::window::fit(font, line, width - 8.0);
            let text = if self.editing == Some(n) && caret { format!("{shown}|") } else { shown };
            layer.texts.push((text, frame.at(offset(field, [3.0, 1.0])), [0, 0, 0]));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_saves_and_falls_back() {
        let dir = std::env::temp_dir().join(format!("rustnarok-macros-{}", std::process::id()));
        let path = dir.join("macros");
        let _ = std::fs::remove_dir_all(&dir);
        // No file: the emotion defaults.
        let mut lines = load(Some(&path));
        assert_eq!((lines[0].as_str(), lines[9].as_str()), ("/!", "/..."));
        lines[9] = "hello macro".into();
        save(Some(&path), &lines);
        assert_eq!(load(Some(&path))[9], "hello macro");
        // A file with fewer lines keeps the defaults for the rest.
        std::fs::write(&path, "/sit\n").unwrap();
        let lines = load(Some(&path));
        assert_eq!((lines[0].as_str(), lines[1].as_str()), ("/sit", "/?"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn edits_the_line_clicked() {
        let mut lines = load(None);
        let mut window = Window::default();
        window.type_text(&mut lines, "ignored");
        assert_eq!(lines[3], "/lv");
        let at = |line: usize| offset(Window::frame().at(Window::field(line)), [10.0, 5.0]);
        assert_eq!(window.hit(at(3)), Some(Hit::Line(3)));
        window.editing = Some(3);
        window.backspace(&mut lines);
        window.type_text(&mut lines, "2\u{8}");
        assert_eq!(lines[3], "/l2");
    }
}
