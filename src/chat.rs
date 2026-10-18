//! The chat window at the bottom of the screen: message tabs, the log and the input line.

use rustnarok_client::grf::Grf;
use rustnarok_client::text::Font;

use crate::links::{self, ItemLink};
use crate::render::{Gpu, Texture};
use crate::{Layer, SCREEN, UI, contains};

/// F10's steps: the input bar alone, then logs of 3, 6, 9, 12 and 15 lines.
const STEPS: [usize; 6] = [0, 1, 4, 7, 10, 13];
/// The step a new window has: 96 tall.
const DEFAULT_STEP: usize = 1;
const WIDTH: f32 = 600.0;
const LINE: f32 = 14.0;
const TAB_HEIGHT: f32 = 16.0;
const TAB_WIDTH: f32 = 110.0;
/// The log box under the tabs, window-local; it is the window's height less 44 tall.
const LOG_BOX: [f32; 2] = [5.0, 18.0];
const LOG_WIDTH: f32 = 592.0;
const LOG_UNDER: f32 = 44.0;
/// A line's text in the log box, and its black copy one down and right.
const LINE_TEXT: [f32; 2] = [2.0, 3.0];
/// The input bar, its bottom 24: the whisper name box, its list button, the message box and the two buttons, from
/// the bar's top left.
const INPUT_HEIGHT: f32 = 24.0;
const NAME_BOX: ([f32; 2], [f32; 2]) = ([6.0, 4.0], [90.0, 16.0]);
const LIST_BUTTON: [f32; 2] = [97.0, 3.0];
const MESSAGE_BOX: ([f32; 2], [f32; 2]) = ([110.0, 4.0], [460.0, 16.0]);
const FILTER_BUTTON: [f32; 2] = [572.0, 7.0];
const SIZE_BUTTON: [f32; 2] = [586.0, 7.0];
const BUTTON_SIZE: [f32; 2] = [11.0, 11.0];
/// The stick, battle-option and option buttons right after the two tabs.
const FUNCTION_BUTTONS: [[f32; 2]; 3] = [[2.0 * TAB_WIDTH + 4.0, 4.0], [2.0 * TAB_WIDTH + 19.0, 5.0], [2.0 * TAB_WIDTH + 34.0, 5.0]];
const TAB_MESSAGES: [usize; 2] = [1291, 1292];
/// Party chat: the player's own lines and the others'.
pub const PARTY_OURS: [u8; 3] = [255, 200, 0];
pub const PARTY: [u8; 3] = [255, 200, 200];
const DARK: [u8; 4] = [0, 0, 0, 0x80];
const DARKER: [u8; 4] = [0, 0, 0, 0xBF];
const TAB_BORDER: [u8; 4] = [0x95, 0x95, 0x95, 0xFF];

/// The window's height at step `step`: the input bar alone at 0, else the tabs, the log and the bar.
fn height(step: usize) -> f32 {
    if step == 0 { INPUT_HEIGHT } else { INPUT_HEIGHT + TAB_HEIGHT + LINE * (step + 3) as f32 }
}

/// The lines the log box shows at step `step`: those whose 14 rows fit in it.
fn rows(step: usize) -> usize {
    if step == 0 { 0 } else { ((height(step) - LOG_UNDER - LINE_TEXT[1]) / LINE) as usize }
}

/// Where line `row` of the log box puts its text, window-local; the black copy goes one down and right.
fn line_at(row: usize) -> [f32; 2] {
    [LOG_BOX[0] + LINE_TEXT[0], LOG_BOX[1] + LINE_TEXT[1] + LINE * row as f32]
}

pub struct Assets {
    input: Texture,
    list: Texture,
    base: Texture,
    /// The stick, battle-option and option buttons after the tabs.
    functions: Vec<Texture>,
    dark: Texture,
    darker: Texture,
    white: Texture,
    tab_border: Texture,
    /// Item links' underline.
    link: Texture,
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let basic = format!("{UI}basic_interface\\");
        let image = |name: &str| gpu.load_texture(grf, &format!("{basic}{name}.bmp"), false);
        Self {
            input: image("dialog_bg"),
            list: image("dialog_btn0"),
            base: image("sys_base_off"),
            // ponytail: which images the three buttons are.
            functions: ["stickoff", "battle_option_a", "battle_option2_a"].iter().map(|name| image(name)).collect(),
            dark: gpu.solid(DARK),
            darker: gpu.solid(DARKER),
            white: gpu.solid([255; 4]),
            tab_border: gpu.solid(TAB_BORDER),
            link: gpu.solid([crate::links::COLOR[0], crate::links::COLOR[1], crate::links::COLOR[2], 0xFF]),
        }
    }
}

pub struct Chat {
    /// Every line so far with its color.
    pub lines: Vec<(String, [u8; 3])>,
    /// What's being typed, and the syllable the input method is still composing.
    pub input: String,
    pub preedit: String,
    /// The whisper name box, and whether typing goes there.
    pub whisper: String,
    pub naming: bool,
    /// Which of F10's steps the window is at.
    size: usize,
    /// Lines scrolled back from the newest.
    scroll: usize,
    /// Alt+F10 hides the window. Typing still goes to the line.
    pub hidden: bool,
}

impl Default for Chat {
    fn default() -> Self {
        Self {
            lines: Vec::new(),
            input: String::new(),
            preedit: String::new(),
            whisper: String::new(),
            naming: false,
            size: STEPS.iter().position(|&step| step == DEFAULT_STEP).unwrap_or(0),
            scroll: 0,
            hidden: false,
        }
    }
}

impl Chat {
    fn step(&self) -> usize {
        STEPS[self.size]
    }

    pub fn height(&self) -> f32 {
        height(self.step())
    }

    /// The top of the window; its bottom stays at the screen's.
    fn top(&self) -> f32 {
        SCREEN[1] - self.height()
    }

    fn bar_top(&self) -> f32 {
        SCREEN[1] - INPUT_HEIGHT
    }

    pub fn contains(&self, cursor: [f32; 2]) -> bool {
        !self.hidden && contains([0.0, self.top()], [WIDTH, self.height()], cursor)
    }

    /// Alt+F10: the log and the input bar leave the screen, or come back.
    pub fn toggle_hidden(&mut self) {
        self.hidden = !self.hidden;
    }

    /// F10: the next size, the same cycle as the size button.
    pub fn cycle_size(&mut self) {
        self.size = (self.size + 1) % STEPS.len();
        self.scroll = 0;
    }

    /// Whether a press here is the window's alone: its tabs, the strips around the log and the input bar. The log
    /// box's lines let presses through to the world (the wheel still scrolls them).
    pub fn blocks(&self, cursor: [f32; 2]) -> bool {
        let log = [LOG_BOX[0], self.top() + LOG_BOX[1]];
        let log_size = [LOG_WIDTH, (self.height() - LOG_UNDER).max(0.0)];
        self.contains(cursor) && !contains(log, log_size, cursor)
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
        let most = self.lines.len().saturating_sub(self::rows(self.step()));
        self.scroll = (self.scroll as i32 - rows).clamp(0, most as i32) as usize;
    }

    /// A click on the window: the size button cycles the size; the boxes of the input bar take the caret.
    pub fn click(&mut self, cursor: [f32; 2]) {
        let bar = [0.0, self.bar_top()];
        let at = |offset: [f32; 2]| [bar[0] + offset[0], bar[1] + offset[1]];
        if contains(at(SIZE_BUTTON), BUTTON_SIZE, cursor) {
            self.cycle_size();
        } else if contains(at(NAME_BOX.0), NAME_BOX.1, cursor) {
            self.naming = true;
        } else if contains(at(MESSAGE_BOX.0), MESSAGE_BOX.1, cursor) {
            self.naming = false;
        }
    }

    /// Typing goes to the box with the caret.
    pub fn typing(&mut self) -> &mut String {
        if self.naming { &mut self.whisper } else { &mut self.input }
    }

    /// The whisper-name list next to the battle-mode line (#179). Closed until that list exists.
    pub fn whisper_list_open(&self) -> bool {
        false
    }

    /// Closes the whisper-name list (#179).
    pub fn close_whisper_list(&mut self) {}

    /// Closes the battle-mode line (#179): the typed text goes, the line closes.
    pub fn close_battle_line(&mut self) {
        self.input.clear();
        self.preedit.clear();
        self.naming = false;
    }

    /// The window; the input bar only while the line is `open`.
    /// The window, and the boxes of the item links in its log with their items. `name_of` names an item a link carries
    /// without words.
    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &self,
        layer: &mut Layer<'a>,
        assets: &'a Assets,
        font: &Font,
        messages: &[String],
        open: bool,
        caret: bool,
        name_of: &dyn Fn(&ItemLink) -> String,
    ) -> Vec<([f32; 4], ItemLink)> {
        let mut link_boxes = Vec::new();
        if self.hidden {
            return link_boxes;
        }
        let top = self.top();
        if self.step() > 0 {
            // Tabs: the general log is the open one.
            for (tab, &message) in TAB_MESSAGES.iter().enumerate() {
                let left = tab as f32 * TAB_WIDTH;
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
            for (function, corner) in assets.functions.iter().zip(FUNCTION_BUTTONS) {
                layer.sprites.push((function, [corner[0], top + corner[1]], function.size));
            }

            // The log: translucent black between white edges, newest line at the bottom.
            let corner = [LOG_BOX[0], top + LOG_BOX[1]];
            let size = [LOG_WIDTH, self.height() - LOG_UNDER];
            layer.sprites.push((&assets.dark, corner, size));
            layer.sprites.push((&assets.white, corner, [1.0, size[1]]));
            layer.sprites.push((&assets.white, [corner[0] + size[0] - 1.0, corner[1]], [1.0, size[1]]));
            layer.sprites.push((&assets.white, [2.0 * TAB_WIDTH, corner[1]], [corner[0] + size[0] - 2.0 * TAB_WIDTH, 1.0]));
            let shown = rows(self.step());
            let end = self.lines.len().saturating_sub(self.scroll);
            let start = end.saturating_sub(shown);
            let first_row = shown - (end - start);
            for (row, (line, color)) in self.lines[start..end].iter().enumerate() {
                let [mut x, y] = line_at(first_row + row);
                let y = top + y;
                // Item links in their colour, underlined.
                for (piece, item) in links::pieces(&links::mark(line, name_of)) {
                    let width = font.width(&piece);
                    let color = if item.is_some() { links::COLOR } else { *color };
                    layer.texts.push((piece.clone(), [x + 1.0, y + 1.0], [0, 0, 0]));
                    layer.texts.push((piece, [x, y], color));
                    if let Some(item) = item {
                        let under = y + font.line_height() - 1.0;
                        layer.sprites.push((&assets.link, [x, under], [width, 1.0]));
                        link_boxes.push(([x, y, width, font.line_height()], item));
                    }
                    x += width;
                }
            }
        }
        if !open {
            return link_boxes;
        }

        // The input bar with what's being typed and the caret.
        let bar_top = self.bar_top();
        layer.sprites.push((&assets.input, [0.0, bar_top], assets.input.size));
        layer.sprites.push((&assets.list, [LIST_BUTTON[0], bar_top + LIST_BUTTON[1]], assets.list.size));
        for button in [FILTER_BUTTON, SIZE_BUTTON] {
            layer.sprites.push((&assets.base, [button[0], bar_top + button[1]], assets.base.size));
        }
        let text_top = bar_top + NAME_BOX.0[1] + ((NAME_BOX.1[1] - font.line_height()) / 2.0).round();
        let (name, typed) = if self.naming {
            (format!("{}{}", self.whisper, self.preedit), self.input.clone())
        } else {
            (self.whisper.clone(), format!("{}{}", self.input, self.preedit))
        };
        layer.texts.push((name.clone(), [NAME_BOX.0[0], text_top], [0, 0, 0]));
        layer.texts.push((typed.clone(), [MESSAGE_BOX.0[0], text_top], [0, 0, 0]));
        if caret {
            let end = if self.naming { NAME_BOX.0[0] + font.width(&name) } else { MESSAGE_BOX.0[0] + font.width(&typed) };
            layer.sprites.push((&assets.darker, [end + 1.0, text_top + 1.0], [1.0, font.line_height() - 2.0]));
        }
        link_boxes
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
    /// /zoom: the camera can go further out outdoors, on or off.
    Zoom,
    /// /noctrl, /nc: a click on a monster keeps attacking without Ctrl, on or off.
    NoCtrl,
    /// /battlemode, /bm: battle mode. The chat line closes and letter keys are shortcuts.
    BattleMode,
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
        "/zoom" => Command::Zoom,
        "/battlemode" | "/bm" => Command::BattleMode,
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
        let log_top = SCREEN[1] - 96.0 + LOG_BOX[1];
        // The log's lines: the window has them, the world too.
        assert!(chat.contains([300.0, log_top + 10.0]) && !chat.blocks([300.0, log_top + 10.0]));
        // The input bar and the tabs are the window's alone.
        assert!(chat.blocks([300.0, SCREEN[1] - 10.0]) && chat.blocks([20.0, chat.top() + 5.0]));
        assert!(!chat.contains([700.0, log_top + 10.0]) && !chat.blocks([700.0, log_top + 10.0]));
    }

    #[test]
    fn steps_through_six_sizes() {
        let mut chat = Chat::default();
        let mut heights = Vec::new();
        for _ in 0..7 {
            heights.push((chat.height(), rows(chat.step()), chat.top()));
            chat.cycle_size();
        }
        let tall = [96.0, 138.0, 180.0, 222.0, 264.0, 24.0, 96.0];
        assert_eq!(heights.iter().map(|&(height, ..)| height).collect::<Vec<_>>(), tall);
        assert_eq!(heights.iter().map(|&(_, rows, _)| rows).collect::<Vec<_>>(), [3, 6, 9, 12, 15, 0, 3]);
        // The bottom edge stays put.
        assert!(heights.iter().all(|&(height, _, top)| top + height == SCREEN[1]));
        // The input bar alone: no log to let presses through.
        let mut alone = Chat::default();
        (0..5).for_each(|_| alone.cycle_size());
        assert_eq!(alone.height(), 24.0);
        assert!(alone.blocks([300.0, SCREEN[1] - 10.0]) && !alone.contains([300.0, SCREEN[1] - 30.0]));
    }

    #[test]
    fn puts_lines_14_apart() {
        assert_eq!([line_at(0), line_at(1), line_at(2)], [[7.0, 21.0], [7.0, 35.0], [7.0, 49.0]]);
        // The last of 15 lines still ends inside the 220-tall box of the 264 window.
        assert!(line_at(14)[1] + LINE <= LOG_BOX[1] + height(13) - LOG_UNDER);
    }

    #[test]
    fn places_the_input_bar_controls() {
        let h = 96.0;
        let bar = h - INPUT_HEIGHT;
        let local = |offset: [f32; 2]| [offset[0], bar + offset[1]];
        assert_eq!((local(NAME_BOX.0), NAME_BOX.1), ([6.0, h - 20.0], [90.0, 16.0]));
        assert_eq!(local(LIST_BUTTON), [97.0, h - 21.0]);
        assert_eq!((local(MESSAGE_BOX.0), MESSAGE_BOX.1), ([110.0, h - 20.0], [460.0, 16.0]));
        assert_eq!([local(FILTER_BUTTON), local(SIZE_BUTTON)], [[572.0, h - 17.0], [586.0, h - 17.0]]);
        assert_eq!(FUNCTION_BUTTONS, [[224.0, 4.0], [239.0, 5.0], [254.0, 5.0]]);
        // The boxes take the caret, the size button the size.
        let mut chat = Chat::default();
        chat.click([50.0, SCREEN[1] - 12.0]);
        assert!(chat.naming);
        chat.click([300.0, SCREEN[1] - 12.0]);
        assert!(!chat.naming);
        chat.click([590.0, SCREEN[1] - 12.0]);
        assert_eq!(chat.height(), 138.0);
    }

    #[test]
    fn colors_party_chat() {
        assert_eq!([PARTY_OURS, PARTY], [[255, 200, 0], [255, 200, 200]]);
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
        assert_eq!(command("/camera"), Some(Command::Camera));
        assert_eq!(command("/zoom"), Some(Command::Zoom));
        assert_eq!(command("/bm"), Some(Command::BattleMode));
        assert_eq!(command("/battlemode"), Some(Command::BattleMode));
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
