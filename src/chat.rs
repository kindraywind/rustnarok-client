//! The chat window at the bottom of the screen: message tabs, the log and the input line.

use rustnarok_client::grf::Grf;
use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};
use crate::{Layer, SCREEN, UI, contains};

// ponytail: layout over dialog_bg.bmp; calibrate against the real client.
/// Log heights the size button cycles through: three lines and up.
const HEIGHTS: [f32; 5] = [42.0, 84.0, 126.0, 168.0, 210.0];
const LINE: f32 = 14.0;
const LEFT: f32 = 3.0;
const BODY_WIDTH: f32 = 594.0;
const TAB_HEIGHT: f32 = 17.0;
const TAB_WIDTH: f32 = 75.0;
/// The grab strip at the top of the log, and its bottom padding.
const HANDLE: f32 = 14.0;
const PADDING: f32 = 5.0;
const TEXT_LEFT: f32 = LEFT + 6.0;
/// The input bar: the whisper name box, its list button, the message field and the two buttons at the right.
const INPUT_HEIGHT: f32 = 24.0;
const LIST_BUTTON: [f32; 2] = [97.0, 3.0];
const MESSAGE_LEFT: f32 = 113.0;
const NAME_LEFT: f32 = 6.0;
const FILTER_BUTTON: [f32; 2] = [576.0, 7.0];
const SIZE_BUTTON: [f32; 2] = [588.0, 7.0];
const BUTTON_SIZE: [f32; 2] = [11.0, 11.0];
const TAB_MESSAGES: [usize; 2] = [1291, 1292];
const DARK: [u8; 4] = [0, 0, 0, 0x80];
const DARKER: [u8; 4] = [0, 0, 0, 0xBF];
const TAB_BORDER: [u8; 4] = [0x95, 0x95, 0x95, 0xFF];

pub struct Assets {
    input: Texture,
    list: Texture,
    base: Texture,
    /// Chat mode, battle options, stick, battle option, minimize, lock: the buttons at the top right.
    functions: Vec<Texture>,
    dark: Texture,
    darker: Texture,
    white: Texture,
    tab_border: Texture,
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let basic = format!("{UI}basic_interface\\");
        let image = |name: &str| gpu.load_texture(grf, &format!("{basic}{name}.bmp"), false);
        Self {
            input: image("dialog_bg"),
            list: image("dialog_btn0"),
            base: image("sys_base_off"),
            functions: ["chatmode_on", "battle_option2_a", "stickoff", "battle_option_a", "wnd_mini_b", "lock_dragwnd"]
                .iter()
                .map(|name| image(name))
                .collect(),
            dark: gpu.solid(DARK),
            darker: gpu.solid(DARKER),
            white: gpu.solid([255; 4]),
            tab_border: gpu.solid(TAB_BORDER),
        }
    }
}

#[derive(Default)]
pub struct Chat {
    /// Every line so far with its color.
    pub lines: Vec<(String, [u8; 3])>,
    /// What's being typed, and the syllable the input method is still composing.
    pub input: String,
    pub preedit: String,
    /// The whisper name box, and whether typing goes there.
    pub whisper: String,
    pub naming: bool,
    size: usize,
    /// Lines scrolled back from the newest.
    scroll: usize,
    /// Alt+F10 hides the window. Typing still goes to the line.
    pub hidden: bool,
}

impl Chat {
    fn log_height(&self) -> f32 {
        HEIGHTS[self.size]
    }

    /// The top of the window (its tabs).
    fn top(&self) -> f32 {
        SCREEN[1] - INPUT_HEIGHT - PADDING - self.log_height() - HANDLE - TAB_HEIGHT
    }

    pub fn contains(&self, cursor: [f32; 2]) -> bool {
        !self.hidden && contains([0.0, self.top()], [600.0, SCREEN[1] - self.top()], cursor)
    }

    /// Alt+F10: the log and the input bar leave the screen, or come back.
    pub fn toggle_hidden(&mut self) {
        self.hidden = !self.hidden;
    }

    /// F10: the next log height, the same cycle as the size button.
    pub fn cycle_size(&mut self) {
        self.size = (self.size + 1) % HEIGHTS.len();
        self.scroll = 0;
    }

    /// Whether a press here is the window's alone: its tabs, its grab strip and the input bar. The log's lines let
    /// presses through to the world (the wheel still scrolls them).
    pub fn blocks(&self, cursor: [f32; 2]) -> bool {
        let log_top = self.top() + TAB_HEIGHT + HANDLE;
        self.contains(cursor) && !contains([0.0, log_top], [600.0, self.log_height()], cursor)
    }

    pub fn push(&mut self, text: String, color: [u8; 3]) {
        self.lines.push((text, color));
        // New lines show unless the log is scrolled back.
        if self.scroll > 0 {
            self.scroll += 1;
        }
    }

    /// Scrolls the log by `rows` lines (negative: back in time).
    pub fn scroll_by(&mut self, rows: i32) {
        let shown = (self.log_height() / LINE) as usize;
        let most = self.lines.len().saturating_sub(shown);
        self.scroll = (self.scroll as i32 - rows).clamp(0, most as i32) as usize;
    }

    /// A click on the window: the size button cycles the log height; the boxes of the input bar take the caret.
    pub fn click(&mut self, cursor: [f32; 2]) {
        let bar_top = SCREEN[1] - INPUT_HEIGHT;
        if contains([SIZE_BUTTON[0], bar_top + SIZE_BUTTON[1]], BUTTON_SIZE, cursor) {
            self.cycle_size();
        } else if contains([0.0, bar_top], [LIST_BUTTON[0], INPUT_HEIGHT], cursor) {
            self.naming = true;
        } else if contains([MESSAGE_LEFT, bar_top], [FILTER_BUTTON[0] - MESSAGE_LEFT, INPUT_HEIGHT], cursor) {
            self.naming = false;
        }
    }

    /// Typing goes to the box with the caret.
    pub fn typing(&mut self) -> &mut String {
        if self.naming { &mut self.whisper } else { &mut self.input }
    }

    pub fn draw<'a>(&self, layer: &mut Layer<'a>, assets: &'a Assets, font: &Font, messages: &[String], caret: bool) {
        if self.hidden {
            return;
        }
        let top = self.top();
        // Tabs: the general log is the open one.
        for (tab, &message) in TAB_MESSAGES.iter().enumerate() {
            let left = LEFT + tab as f32 * TAB_WIDTH;
            let (fill, border) = if tab == 0 { (&assets.dark, &assets.white) } else { (&assets.darker, &assets.tab_border) };
            layer.sprites.push((fill, [left, top + 1.0], [TAB_WIDTH, TAB_HEIGHT - 1.0]));
            layer.sprites.push((border, [left, top], [TAB_WIDTH, 1.0]));
            layer.sprites.push((border, [left, top], [1.0, TAB_HEIGHT]));
            layer.sprites.push((border, [left + TAB_WIDTH - 1.0, top], [1.0, TAB_HEIGHT]));
            let label = messages.get(message).map_or("", String::as_str);
            let x = (left + (TAB_WIDTH - font.width(label)) / 2.0).round();
            layer.texts.push((label.to_owned(), [x + 1.0, top + 2.0], [0, 0, 0]));
            layer.texts.push((label.to_owned(), [x, top + 1.0], [255; 3]));
        }
        let mut x = LEFT + BODY_WIDTH;
        for function in assets.functions.iter().rev() {
            x -= function.size[0] + 3.0;
            layer.sprites.push((function, [x, top + 4.0], function.size));
        }

        // The log: translucent black between white edges, newest line at the bottom.
        let body_top = top + TAB_HEIGHT;
        let body_height = HANDLE + self.log_height() + PADDING;
        layer.sprites.push((&assets.dark, [LEFT, body_top], [BODY_WIDTH, body_height]));
        layer.sprites.push((&assets.white, [LEFT, body_top], [1.0, body_height]));
        layer.sprites.push((&assets.white, [LEFT + BODY_WIDTH - 1.0, body_top], [1.0, body_height]));
        layer.sprites.push((&assets.white, [LEFT + TAB_WIDTH * 2.0, body_top], [BODY_WIDTH - TAB_WIDTH * 2.0, 1.0]));
        let shown = (self.log_height() / LINE) as usize;
        let end = self.lines.len().saturating_sub(self.scroll);
        let start = end.saturating_sub(shown);
        let first_row = shown - (end - start);
        for (row, (line, color)) in self.lines[start..end].iter().enumerate() {
            let y = body_top + HANDLE + (first_row + row) as f32 * LINE - 1.0;
            layer.texts.push((line.clone(), [TEXT_LEFT + 1.0, y + 1.0], [0, 0, 0]));
            layer.texts.push((line.clone(), [TEXT_LEFT, y], *color));
        }

        // The input bar with what's being typed and the caret.
        let bar_top = SCREEN[1] - INPUT_HEIGHT;
        layer.sprites.push((&assets.input, [0.0, bar_top], assets.input.size));
        layer.sprites.push((&assets.list, [LIST_BUTTON[0], bar_top + LIST_BUTTON[1]], assets.list.size));
        for button in [FILTER_BUTTON, SIZE_BUTTON] {
            layer.sprites.push((&assets.base, [button[0], bar_top + button[1]], assets.base.size));
        }
        let text_top = bar_top + (INPUT_HEIGHT - font.line_height()) / 2.0;
        let (name, typed) = if self.naming {
            (format!("{}{}", self.whisper, self.preedit), self.input.clone())
        } else {
            (self.whisper.clone(), format!("{}{}", self.input, self.preedit))
        };
        layer.texts.push((name.clone(), [NAME_LEFT, text_top], [0, 0, 0]));
        layer.texts.push((typed.clone(), [MESSAGE_LEFT, text_top], [0, 0, 0]));
        if caret {
            let end = if self.naming { NAME_LEFT + font.width(&name) } else { MESSAGE_LEFT + font.width(&typed) };
            layer.sprites.push((&assets.darker, [end + 1.0, text_top + 1.0], [1.0, font.line_height() - 2.0]));
        }
    }
}

/// The chat commands the client answers itself (the sound, party and emotion ones are elsewhere).
#[derive(Debug, PartialEq)]
pub enum Command {
    /// /h, /help: the help lines.
    Help,
    /// /w, /who, /player: how many are online.
    Who,
    /// /where: the map and the player's cell.
    Where,
    /// /ex or /in and a name: block or allow that one's whispers.
    Whispers { name: String, allow: bool },
    /// /ex alone: the names blocked.
    BlockList,
    /// /exall, /inall: block or allow everyone's whispers.
    AllWhispers { allow: bool },
    /// /effect: effects off or on.
    Effect,
    /// /emo, /emotion: the emotion list, open or closed.
    EmotionList,
    /// /memo: where the player stands becomes a Warp Portal destination.
    Memo,
    /// /achievement: the achievement window, open or closed.
    Achievements,
    /// /pvpinfo: the player's PvP wins, losses and points.
    PvpInfo,
    /// /noshift: a click on a player attacks without Shift, on or off.
    NoShift,
    /// /camera: the camera sticks to the player instead of gliding after them, on or off.
    Camera,
    /// /noctrl, /nc: a click on a monster keeps attacking without Ctrl, on or off.
    NoCtrl,
    /// /battlechat: typed lines go to the battleground team, on or off.
    BattleChat,
    /// /dressroom: the dress room.
    DressRoom,
    /// /blacksmith, /alchemist, /taekwon, /pk: a ranking's top ten (0 to 3).
    Ranking(u16),
}

/// The command typed, if `text` is one; a name is the rest of the line, quotes taken off.
pub fn command(text: &str) -> Option<Command> {
    let (command, name) = text.split_once(' ').map_or((text, ""), |(command, rest)| (command, rest.trim().trim_matches('"')));
    Some(match command {
        "/h" | "/help" => Command::Help,
        "/w" | "/who" | "/player" => Command::Who,
        "/where" => Command::Where,
        "/ex" if name.is_empty() => Command::BlockList,
        "/ex" | "/in" if !name.is_empty() => Command::Whispers { name: name.to_owned(), allow: command == "/in" },
        "/exall" => Command::AllWhispers { allow: false },
        "/inall" => Command::AllWhispers { allow: true },
        "/effect" => Command::Effect,
        "/emo" | "/emotion" => Command::EmotionList,
        "/achievement" => Command::Achievements,
        "/memo" => Command::Memo,
        "/pvpinfo" => Command::PvpInfo,
        "/noshift" => Command::NoShift,
        "/noctrl" | "/nc" => Command::NoCtrl,
        "/camera" => Command::Camera,
        "/battlechat" => Command::BattleChat,
        "/dressroom" => Command::DressRoom,
        "/blacksmith" => Command::Ranking(0),
        "/alchemist" => Command::Ranking(1),
        "/taekwon" => Command::Ranking(2),
        // Only the bare /pk: /pk on and /pk off are another command.
        "/pk" if name.is_empty() => Command::Ranking(3),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lets_presses_through_the_log() {
        let chat = Chat::default();
        let log_top = SCREEN[1] - INPUT_HEIGHT - PADDING - HEIGHTS[0];
        // The log's lines: the window has them, the world too.
        assert!(chat.contains([300.0, log_top + 10.0]) && !chat.blocks([300.0, log_top + 10.0]));
        // The input bar and the tabs are the window's alone.
        assert!(chat.blocks([300.0, SCREEN[1] - 10.0]) && chat.blocks([20.0, chat.top() + 5.0]));
        assert!(!chat.contains([700.0, log_top + 10.0]) && !chat.blocks([700.0, log_top + 10.0]));
    }

    #[test]
    fn reads_the_clients_commands() {
        for text in ["/h", "/help"] {
            assert_eq!(command(text), Some(Command::Help));
        }
        for text in ["/w", "/who", "/player"] {
            assert_eq!(command(text), Some(Command::Who));
        }
        assert_eq!(command("/where"), Some(Command::Where));
        assert_eq!(command("/memo"), Some(Command::Memo));
        assert_eq!(command("/pvpinfo"), Some(Command::PvpInfo));
        assert_eq!(command("/noshift"), Some(Command::NoShift));
        assert_eq!([command("/noctrl"), command("/nc")], [Some(Command::NoCtrl), Some(Command::NoCtrl)]);
        assert_eq!(command("/battlechat"), Some(Command::BattleChat));
        assert_eq!(command("/dressroom"), Some(Command::DressRoom));
        for (text, kind) in [("/blacksmith", 0), ("/alchemist", 1), ("/taekwon", 2), ("/pk", 3)] {
            assert_eq!(command(text), Some(Command::Ranking(kind)), "{text}");
        }
        assert_eq!(command("/pk on"), None);
        assert_eq!(command("/ex"), Some(Command::BlockList));
        assert_eq!(command("/ex Poring"), Some(Command::Whispers { name: "Poring".into(), allow: false }));
        assert_eq!(command("/in \"Big Poring\" "), Some(Command::Whispers { name: "Big Poring".into(), allow: true }));
        assert_eq!(command("/exall"), Some(Command::AllWhispers { allow: false }));
        assert_eq!(command("/inall"), Some(Command::AllWhispers { allow: true }));
        assert_eq!(command("/effect"), Some(Command::Effect));
        assert_eq!(command("/emo"), Some(Command::EmotionList));
        assert_eq!(command("/achievement"), Some(Command::Achievements));
        assert_eq!(command("/emotion"), Some(Command::EmotionList));
        for text in ["/in", "/whom", "hello /who", "/sit", "/ho"] {
            assert_eq!(command(text), None, "{text}");
        }
    }
}
