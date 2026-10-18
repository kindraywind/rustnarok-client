//! One small window per whisper partner, the friend setup that opens them, the block list and the away message.
//!
//! Sending still uses the chat name box's packet (`MapSession::whisper`, CZ_WHISPER `0x0096`). The block list is
//! ZC_WHISPER_LIST `0x00D4`; allowing a name again is `MapSession::set_whisper(name, true)`, the same as `/in`.

use std::path::{Path, PathBuf};

use rustnarok_client::text::Font;

use crate::window::{Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, contains, screen};

/// "Save Chat as Text File", "Register as a Friend".
const SAVE_MESSAGE: usize = 312;
const FRIEND_MESSAGE: usize = 358;
/// "Open 1:1 Chat between Friends", "…between Strangers".
const FRIENDS_MESSAGE: usize = 359;
const STRANGERS_MESSAGE: usize = 361;
/// "Please enter Away Message."
pub const AWAY_PROMPT: usize = 944;

/// When an incoming whisper opens a 1:1 window. Both off: it never does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OpenSettings {
    pub friends: bool,
    pub strangers: bool,
}

impl OpenSettings {
    /// The box that matches the sender.
    pub fn opens(self, friend: bool) -> bool {
        if friend { self.friends } else { self.strangers }
    }

    pub fn decode(text: &str) -> Self {
        let mut settings = Self::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else { continue };
            let on = value.trim() == "1";
            match key.trim() {
                "friends" => settings.friends = on,
                "strangers" => settings.strangers = on,
                _ => {}
            }
        }
        settings
    }

    pub fn encode(self) -> String {
        format!("friends={}\nstrangers={}\n", u8::from(self.friends), u8::from(self.strangers))
    }

    pub fn load() -> Self {
        crate::settings_path("whisper").and_then(|path| std::fs::read_to_string(path).ok()).map(|text| Self::decode(&text)).unwrap_or_default()
    }

    pub fn save(self) {
        if let Some(path) = crate::settings_path("whisper") {
            let _ = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(path, self.encode()));
        }
    }
}

/// Players whispered with this session, newest first, each name once.
pub fn note_recent(names: &mut Vec<String>, name: &str) {
    let name = name.trim();
    if name.is_empty() {
        return;
    }
    names.retain(|known| !known.eq_ignore_ascii_case(name));
    names.insert(0, name.to_owned());
    // ponytail: the last 10; the client's own cap isn't known.
    names.truncate(10);
}

/// An away message answers each sender once, until the text changes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Away {
    message: String,
    answered: Vec<String>,
}

impl Away {
    pub fn text(&self) -> Option<&str> {
        (!self.message.is_empty()).then_some(&self.message)
    }

    /// Empty text turns the reply off.
    pub fn set(&mut self, text: &str) {
        self.message = text.trim().to_owned();
        self.answered.clear();
    }

    /// The reply to send, if this sender has not been answered for the current text.
    pub fn reply(&mut self, from: &str) -> Option<String> {
        let from = from.trim();
        if self.message.is_empty() || from.is_empty() || self.answered.iter().any(|known| known.eq_ignore_ascii_case(from)) {
            return None;
        }
        self.answered.push(from.to_owned());
        Some(self.message.clone())
    }
}

// ponytail: 220×168, starting at (160, 140) and stepped 24px so a second window stays visible. The menu is two
// text rows under a "Menu" button; the client's own frame and menu art would replace this.
const WIDTH: f32 = 220.0;
const HEIGHT: f32 = 168.0;
const LINE: f32 = 14.0;
const MENU_BUTTON: [f32; 2] = [WIDTH - 48.0, TITLE_HEIGHT + 3.0];
const MENU_BUTTON_SIZE: [f32; 2] = [42.0, 14.0];
const MENU_ROW: f32 = TITLE_HEIGHT + 20.0;
const INPUT_BOX: ([f32; 2], [f32; 2]) = ([4.0, HEIGHT - 22.0], [WIDTH - 8.0, 16.0]);
const LOG_TOP: f32 = TITLE_HEIGHT + 22.0;
const LOG_BOTTOM: f32 = HEIGHT - 26.0;
const LINE_CAP: usize = 200;
const INPUT_CAP: usize = 80;

#[derive(Debug, PartialEq)]
pub enum TalkHit {
    Title(TitleHit),
    Input,
    Menu,
    Save,
    Friend,
    Window,
}

#[derive(Debug, PartialEq)]
pub enum TalkClick {
    Closed,
    Save(String, Vec<String>),
    Friend(String),
    None,
}

struct Talk {
    partner: String,
    lines: Vec<String>,
    input: String,
    typing: bool,
    menu: bool,
    frame: Frame,
}

/// The open 1:1 windows. The last one is the front (Esc closes it).
#[derive(Default)]
pub struct Talks {
    windows: Vec<Talk>,
}

impl Talks {
    pub fn is_empty(&self) -> bool {
        self.windows.is_empty()
    }

    pub fn typing(&self) -> bool {
        self.windows.iter().any(|talk| talk.typing)
    }

    pub fn blur(&mut self) {
        for talk in &mut self.windows {
            talk.typing = false;
        }
    }

    /// `windows` lists each one as `Whisper:<name>`.
    pub fn window_names(&self) -> Vec<String> {
        self.windows.iter().filter(|talk| talk.frame.open).map(|talk| format!("Whisper:{}", talk.partner)).collect()
    }

    /// Opens that partner's window, or brings the open one to the front.
    pub fn ensure(&mut self, partner: &str) {
        let partner = partner.trim();
        if partner.is_empty() {
            return;
        }
        if let Some(index) = self.index_of(partner) {
            self.bring_to_front(index);
            let talk = self.windows.last_mut().unwrap();
            talk.frame.open = true;
            talk.frame.minimized = false;
            return;
        }
        let step = self.windows.len() as f32;
        let mut frame = Frame::new([160.0 + step * 24.0, 140.0 + step * 24.0], WIDTH);
        frame.open = true;
        self.windows.push(Talk { partner: partner.to_owned(), lines: Vec::new(), input: String::new(), typing: false, menu: false, frame });
    }

    pub fn push_line(&mut self, partner: &str, line: String) {
        let Some(talk) = self.windows.iter_mut().find(|talk| talk.partner.eq_ignore_ascii_case(partner)) else { return };
        talk.lines.push(line);
        if talk.lines.len() > LINE_CAP {
            let extra = talk.lines.len() - LINE_CAP;
            talk.lines.drain(0..extra);
        }
    }

    pub fn close_front(&mut self) {
        self.windows.pop();
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<(usize, TalkHit)> {
        self.windows.iter().enumerate().rev().find_map(|(index, talk)| talk.hit(cursor).map(|hit| (index, hit)))
    }

    pub fn click(&mut self, index: usize, hit: TalkHit, cursor: [f32; 2]) -> TalkClick {
        if index >= self.windows.len() {
            return TalkClick::None;
        }
        self.bring_to_front(index);
        let last = self.windows.len() - 1;
        match hit {
            TalkHit::Title(TitleHit::Close) => {
                self.windows.remove(last);
                TalkClick::Closed
            }
            TalkHit::Title(TitleHit::Title) => {
                self.windows[last].frame.start_drag(cursor);
                TalkClick::None
            }
            TalkHit::Title(TitleHit::Minimize) => {
                self.windows[last].frame.minimized = !self.windows[last].frame.minimized;
                TalkClick::None
            }
            TalkHit::Input => {
                for (place, talk) in self.windows.iter_mut().enumerate() {
                    talk.typing = place == last;
                }
                TalkClick::None
            }
            TalkHit::Menu => {
                self.windows[last].menu = !self.windows[last].menu;
                TalkClick::None
            }
            TalkHit::Save => {
                let talk = &mut self.windows[last];
                talk.menu = false;
                TalkClick::Save(talk.partner.clone(), talk.lines.clone())
            }
            TalkHit::Friend => {
                let partner = self.windows[last].partner.clone();
                self.windows[last].menu = false;
                TalkClick::Friend(partner)
            }
            TalkHit::Window => TalkClick::None,
        }
    }

    pub fn type_text(&mut self, text: &str) {
        let Some(talk) = self.windows.iter_mut().find(|talk| talk.typing) else { return };
        let room = INPUT_CAP.saturating_sub(talk.input.chars().count());
        talk.input.extend(text.chars().filter(|character| !character.is_control()).take(room));
    }

    pub fn backspace(&mut self) {
        if let Some(talk) = self.windows.iter_mut().find(|talk| talk.typing) {
            talk.input.pop();
        }
    }

    /// The focused window's partner and the text just sent.
    pub fn take_input(&mut self) -> Option<(String, String)> {
        let talk = self.windows.iter_mut().find(|talk| talk.typing)?;
        let text = std::mem::take(&mut talk.input);
        let text = text.trim().to_owned();
        (!text.is_empty()).then(|| (talk.partner.clone(), text))
    }

    pub fn drag_to(&mut self, cursor: [f32; 2]) {
        for talk in &mut self.windows {
            talk.frame.drag_to(cursor, HEIGHT);
        }
    }

    pub fn drop_drag(&mut self) -> bool {
        self.windows.iter_mut().fold(false, |dragged, talk| talk.frame.drop_drag() | dragged)
    }

    pub fn button_boxes(&self) -> Vec<(String, [f32; 4])> {
        let mut boxes = Vec::new();
        for talk in &self.windows {
            if !talk.frame.open || talk.frame.minimized {
                continue;
            }
            let place = |at: [f32; 2], size: [f32; 2]| {
                let [x, y] = talk.frame.at(at);
                [x, y, size[0], size[1]]
            };
            boxes.push((format!("whisper_input:{}", talk.partner), place(INPUT_BOX.0, INPUT_BOX.1)));
            boxes.push((format!("whisper_menu:{}", talk.partner), place(MENU_BUTTON, MENU_BUTTON_SIZE)));
            if talk.menu {
                boxes.push((format!("whisper_save:{}", talk.partner), place([4.0, MENU_ROW], [WIDTH - 8.0, LINE])));
                boxes.push((format!("whisper_friend:{}", talk.partner), place([4.0, MENU_ROW + LINE], [WIDTH - 8.0, LINE])));
            }
        }
        boxes
    }

    pub fn draw<'a>(&self, layer: &mut Layer<'a>, chrome: &'a Chrome, font: &Font, messages: &[String], pointer: ([f32; 2], bool)) {
        let label = |id: usize, fallback: &str| messages.get(id).filter(|text| !text.is_empty()).cloned().unwrap_or_else(|| fallback.to_owned());
        for talk in &self.windows {
            if !talk.frame.open {
                continue;
            }
            talk.frame.draw_title(layer, chrome, &talk.partner, pointer.0);
            if talk.frame.minimized {
                continue;
            }
            let at = |local: [f32; 2]| talk.frame.at(local);
            layer.sprites.push((&chrome.white, at([0.0, TITLE_HEIGHT]), [WIDTH, HEIGHT - TITLE_HEIGHT]));
            for x in [0.0, WIDTH - 1.0] {
                layer.sprites.push((&chrome.border, at([x, TITLE_HEIGHT]), [1.0, HEIGHT - TITLE_HEIGHT]));
            }
            layer.sprites.push((&chrome.border, at([0.0, HEIGHT - 1.0]), [WIDTH, 1.0]));
            layer.sprites.push((&chrome.border, at(MENU_BUTTON), MENU_BUTTON_SIZE));
            layer.texts.push(("Menu".to_owned(), at([MENU_BUTTON[0] + 6.0, MENU_BUTTON[1] + 1.0]), [0, 0, 0]));
            if talk.menu {
                let save = label(SAVE_MESSAGE, "Save Chat as Text File");
                let friend = label(FRIEND_MESSAGE, "Register as a Friend");
                layer.sprites.push((&chrome.border, at([4.0, MENU_ROW]), [WIDTH - 8.0, LINE]));
                layer.sprites.push((&chrome.border, at([4.0, MENU_ROW + LINE]), [WIDTH - 8.0, LINE]));
                layer.texts.push((save, at([8.0, MENU_ROW]), [0, 0, 0]));
                layer.texts.push((friend, at([8.0, MENU_ROW + LINE]), [0, 0, 0]));
            }
            let rows = ((LOG_BOTTOM - LOG_TOP) / LINE) as usize;
            let shown = talk.lines.len().saturating_sub(rows);
            for (row, line) in talk.lines.iter().skip(shown).enumerate() {
                let text = window_fit(font, line, WIDTH - 12.0);
                // ponytail: black on the white window. The chat's yellow would vanish there.
                layer.texts.push((text, at([6.0, LOG_TOP + row as f32 * LINE]), [0, 0, 0]));
            }
            layer.sprites.push((&chrome.border, at(INPUT_BOX.0), INPUT_BOX.1));
            let typed = if talk.typing { format!("{}_", talk.input) } else { talk.input.clone() };
            layer.texts.push((typed, at([INPUT_BOX.0[0] + 3.0, INPUT_BOX.0[1] + 1.0]), [0, 0, 0]));
        }
    }

    fn index_of(&self, partner: &str) -> Option<usize> {
        self.windows.iter().position(|talk| talk.partner.eq_ignore_ascii_case(partner))
    }

    fn bring_to_front(&mut self, index: usize) {
        if index + 1 < self.windows.len() {
            let talk = self.windows.remove(index);
            self.windows.push(talk);
        }
    }
}

impl Talk {
    fn hit(&self, cursor: [f32; 2]) -> Option<TalkHit> {
        let local = self.frame.local(cursor, HEIGHT)?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(TalkHit::Title(hit));
        }
        if self.frame.minimized {
            return Some(TalkHit::Window);
        }
        if self.menu && contains([4.0, MENU_ROW], [WIDTH - 8.0, LINE], local) {
            return Some(TalkHit::Save);
        }
        if self.menu && contains([4.0, MENU_ROW + LINE], [WIDTH - 8.0, LINE], local) {
            return Some(TalkHit::Friend);
        }
        if contains(MENU_BUTTON, MENU_BUTTON_SIZE, local) {
            return Some(TalkHit::Menu);
        }
        if contains(INPUT_BOX.0, INPUT_BOX.1, local) {
            return Some(TalkHit::Input);
        }
        Some(TalkHit::Window)
    }
}

fn window_fit(font: &Font, text: &str, width: f32) -> String {
    if font.width(text) <= width {
        return text.to_owned();
    }
    let mut cut = text.to_owned();
    while !cut.is_empty() && font.width(&format!("{cut}...")) > width {
        cut.pop();
    }
    format!("{cut}...")
}

/// Writes the log next to the screenshots (`ScreenShot/whisper-<name>.txt`).
pub fn save_log(folder: &Path, partner: &str, lines: &[String]) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(folder)?;
    let safe: String = partner.chars().filter(|character| !matches!(character, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')).collect();
    let path = folder.join(format!("whisper-{safe}.txt"));
    std::fs::write(&path, lines.join("\n"))?;
    Ok(path)
}

// ponytail: 280×70, just left of the party window. Checkboxes are bordered squares; the client's radio art would replace them.
const SETUP_W: f32 = 280.0;
const SETUP_H: f32 = TITLE_HEIGHT + 52.0;
const SETUP_ROW: f32 = 22.0;

#[derive(Debug, PartialEq)]
pub enum SetupHit {
    Title(TitleHit),
    Friends,
    Strangers,
    Window,
}

#[derive(Debug, PartialEq)]
pub enum SetupClick {
    Close,
    Changed(OpenSettings),
}

pub struct FriendSetup {
    pub frame: Frame,
    pub settings: OpenSettings,
}

impl FriendSetup {
    pub fn new(settings: OpenSettings, beside: [f32; 2]) -> Self {
        let mut frame = Frame::new([(beside[0] - SETUP_W - 8.0).max(0.0), beside[1]], SETUP_W);
        frame.open = true;
        Self { frame, settings }
    }

    pub fn height() -> f32 {
        SETUP_H
    }

    fn row_at(row: usize) -> [f32; 2] {
        [8.0, TITLE_HEIGHT + 6.0 + row as f32 * SETUP_ROW]
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<SetupHit> {
        let local = self.frame.local(cursor, SETUP_H)?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(SetupHit::Title(hit));
        }
        if contains(Self::row_at(0), [SETUP_W - 16.0, SETUP_ROW], local) {
            return Some(SetupHit::Friends);
        }
        if contains(Self::row_at(1), [SETUP_W - 16.0, SETUP_ROW], local) {
            return Some(SetupHit::Strangers);
        }
        Some(SetupHit::Window)
    }

    pub fn click(&mut self, hit: SetupHit) -> Option<SetupClick> {
        match hit {
            SetupHit::Title(TitleHit::Close) => Some(SetupClick::Close),
            SetupHit::Friends => {
                self.settings.friends = !self.settings.friends;
                Some(SetupClick::Changed(self.settings))
            }
            SetupHit::Strangers => {
                self.settings.strangers = !self.settings.strangers;
                Some(SetupClick::Changed(self.settings))
            }
            _ => None,
        }
    }

    pub fn button_boxes(&self) -> Vec<(&'static str, [f32; 4])> {
        if !self.frame.open || self.frame.minimized {
            return Vec::new();
        }
        let place = |row: usize| {
            let [x, y] = self.frame.at(Self::row_at(row));
            [x, y, SETUP_W - 16.0, SETUP_ROW]
        };
        vec![("friends", place(0)), ("strangers", place(1))]
    }

    pub fn draw<'a>(&self, layer: &mut Layer<'a>, chrome: &'a Chrome, messages: &[String], cursor: [f32; 2]) {
        if !self.frame.open {
            return;
        }
        // ponytail: the window's title isn't in the spec; the two checkbox strings are.
        self.frame.draw_title(layer, chrome, "1:1 Chat", cursor);
        if self.frame.minimized {
            return;
        }
        let at = |local: [f32; 2]| self.frame.at(local);
        layer.sprites.push((&chrome.white, at([0.0, TITLE_HEIGHT]), [SETUP_W, SETUP_H - TITLE_HEIGHT]));
        for x in [0.0, SETUP_W - 1.0] {
            layer.sprites.push((&chrome.border, at([x, TITLE_HEIGHT]), [1.0, SETUP_H - TITLE_HEIGHT]));
        }
        layer.sprites.push((&chrome.border, at([0.0, SETUP_H - 1.0]), [SETUP_W, 1.0]));
        let label = |id: usize, fallback: &str| messages.get(id).filter(|text| !text.is_empty()).cloned().unwrap_or_else(|| fallback.to_owned());
        let rows = [
            (label(FRIENDS_MESSAGE, "Open 1:1 Chat between Friends"), self.settings.friends),
            (label(STRANGERS_MESSAGE, "Open 1:1 Chat between Strangers"), self.settings.strangers),
        ];
        for (row, (text, on)) in rows.into_iter().enumerate() {
            let corner = Self::row_at(row);
            layer.sprites.push((&chrome.border, at(corner), [12.0, 12.0]));
            if on {
                layer.sprites.push((&chrome.border, at([corner[0] + 3.0, corner[1] + 3.0]), [6.0, 6.0]));
            }
            layer.texts.push((text, at([corner[0] + 16.0, corner[1]]), [0, 0, 0]));
        }
    }
}

// ponytail: 220px wide at (16, 180), one 20px row per name, "Allow" as text. The client's row art would replace it.
const BLOCK_W: f32 = 220.0;
const BLOCK_ROW: f32 = 20.0;
const ALLOW: [f32; 2] = [48.0, 16.0];
const BLOCK_POS: [f32; 2] = [16.0, 180.0];

#[derive(Debug, PartialEq)]
pub enum BlockHit {
    Title(TitleHit),
    Allow(String),
    Window,
}

pub struct BlockList {
    pub frame: Frame,
    names: Vec<String>,
    empty: String,
}

impl BlockList {
    pub fn new(names: Vec<String>, empty: String) -> Self {
        let mut frame = Frame::new(BLOCK_POS, BLOCK_W);
        frame.open = true;
        Self { frame, names, empty }
    }

    pub fn names(&self) -> &[String] {
        &self.names
    }

    pub fn remove(&mut self, name: &str) {
        self.names.retain(|known| !known.eq_ignore_ascii_case(name));
    }

    pub fn height(&self) -> f32 {
        TITLE_HEIGHT + 8.0 + self.names.len().max(1) as f32 * BLOCK_ROW
    }

    fn allow_at(row: usize) -> [f32; 2] {
        [BLOCK_W - ALLOW[0] - 6.0, TITLE_HEIGHT + 4.0 + row as f32 * BLOCK_ROW + 2.0]
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<BlockHit> {
        let local = self.frame.local(cursor, self.height())?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(BlockHit::Title(hit));
        }
        if self.frame.minimized {
            return Some(BlockHit::Window);
        }
        for (row, name) in self.names.iter().enumerate() {
            if contains(Self::allow_at(row), ALLOW, local) {
                return Some(BlockHit::Allow(name.clone()));
            }
        }
        Some(BlockHit::Window)
    }

    pub fn button_boxes(&self) -> Vec<(String, [f32; 4])> {
        if !self.frame.open || self.frame.minimized {
            return Vec::new();
        }
        self.names
            .iter()
            .enumerate()
            .map(|(row, name)| {
                let [x, y] = self.frame.at(Self::allow_at(row));
                (format!("allow:{name}"), [x, y, ALLOW[0], ALLOW[1]])
            })
            .collect()
    }

    pub fn draw<'a>(&self, layer: &mut Layer<'a>, chrome: &'a Chrome, cursor: [f32; 2]) {
        if !self.frame.open {
            return;
        }
        // ponytail: "Block List" until the client's title for /ex is known.
        self.frame.draw_title(layer, chrome, "Block List", cursor);
        if self.frame.minimized {
            return;
        }
        let height = self.height();
        let at = |local: [f32; 2]| self.frame.at(local);
        layer.sprites.push((&chrome.white, at([0.0, TITLE_HEIGHT]), [BLOCK_W, height - TITLE_HEIGHT]));
        for x in [0.0, BLOCK_W - 1.0] {
            layer.sprites.push((&chrome.border, at([x, TITLE_HEIGHT]), [1.0, height - TITLE_HEIGHT]));
        }
        layer.sprites.push((&chrome.border, at([0.0, height - 1.0]), [BLOCK_W, 1.0]));
        if self.names.is_empty() {
            layer.texts.push((self.empty.clone(), at([8.0, TITLE_HEIGHT + 6.0]), [0, 0, 0]));
            return;
        }
        for (row, name) in self.names.iter().enumerate() {
            let top = TITLE_HEIGHT + 4.0 + row as f32 * BLOCK_ROW;
            layer.texts.push((name.clone(), at([8.0, top + 2.0]), [0, 0, 0]));
            let button = Self::allow_at(row);
            layer.sprites.push((&chrome.border, at(button), ALLOW));
            // ponytail: the word Allow; the client's button for it isn't identified.
            layer.texts.push(("Allow".to_owned(), at([button[0] + 6.0, button[1] + 1.0]), [0, 0, 0]));
        }
    }
}

// ponytail: a plain box in the middle of the screen, not the message dialog's art. /away opens it.
const AWAY_SIZE: [f32; 2] = [300.0, 96.0];
const AWAY_OK: [f32; 2] = [24.0, 64.0];
const AWAY_CANCEL: [f32; 2] = [160.0, 64.0];
const AWAY_BUTTON: [f32; 2] = [100.0, 22.0];
const AWAY_LIMIT: usize = 80;

#[derive(Debug, PartialEq)]
pub enum AwayHit {
    Ok,
    Cancel,
    Box,
}

pub struct AwayPrompt {
    pub typed: String,
}

impl AwayPrompt {
    pub fn new(typed: String) -> Self {
        Self { typed }
    }

    pub fn type_text(&mut self, text: &str) {
        let room = AWAY_LIMIT.saturating_sub(self.typed.chars().count());
        self.typed.extend(text.chars().filter(|character| !character.is_control()).take(room));
    }

    fn corner() -> [f32; 2] {
        let size = screen();
        [((size[0] - AWAY_SIZE[0]) / 2.0).floor(), ((size[1] - AWAY_SIZE[1]) / 2.0).floor()]
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<AwayHit> {
        let corner = Self::corner();
        let at = |local: [f32; 2]| [corner[0] + local[0], corner[1] + local[1]];
        if contains(at(AWAY_OK), AWAY_BUTTON, cursor) {
            return Some(AwayHit::Ok);
        }
        if contains(at(AWAY_CANCEL), AWAY_BUTTON, cursor) {
            return Some(AwayHit::Cancel);
        }
        contains(corner, AWAY_SIZE, cursor).then_some(AwayHit::Box)
    }

    pub fn button_boxes(&self) -> Vec<(&'static str, [f32; 4])> {
        let corner = Self::corner();
        let place = |local: [f32; 2]| [corner[0] + local[0], corner[1] + local[1], AWAY_BUTTON[0], AWAY_BUTTON[1]];
        vec![("away_ok", place(AWAY_OK)), ("away_cancel", place(AWAY_CANCEL))]
    }

    pub fn draw<'a>(&self, layer: &mut Layer<'a>, chrome: &'a Chrome, messages: &[String], pointer: ([f32; 2], bool)) {
        let corner = Self::corner();
        let at = |local: [f32; 2]| [corner[0] + local[0], corner[1] + local[1]];
        layer.sprites.push((&chrome.white, corner, AWAY_SIZE));
        layer.sprites.push((&chrome.border, corner, [AWAY_SIZE[0], 1.0]));
        layer.sprites.push((&chrome.border, corner, [1.0, AWAY_SIZE[1]]));
        layer.sprites.push((&chrome.border, at([0.0, AWAY_SIZE[1] - 1.0]), [AWAY_SIZE[0], 1.0]));
        layer.sprites.push((&chrome.border, at([AWAY_SIZE[0] - 1.0, 0.0]), [1.0, AWAY_SIZE[1]]));
        let prompt = messages.get(AWAY_PROMPT).filter(|text| !text.is_empty()).cloned().unwrap_or_else(|| "Please enter Away Message.".to_owned());
        layer.texts.push((prompt, at([12.0, 10.0]), [0, 0, 0]));
        layer.sprites.push((&chrome.border, at([12.0, 32.0]), [AWAY_SIZE[0] - 24.0, 20.0]));
        layer.texts.push((format!("{}_", self.typed), at([16.0, 34.0]), [0, 0, 0]));
        let hit = self.hit(pointer.0);
        for (local, words, which) in [(AWAY_OK, "OK", AwayHit::Ok), (AWAY_CANCEL, "Cancel", AwayHit::Cancel)] {
            layer.sprites.push((&chrome.border, at(local), AWAY_BUTTON));
            let color = if hit == Some(which) && pointer.1 { [0x20, 0x40, 0x80] } else { [0, 0, 0] };
            layer.texts.push((words.to_owned(), at([local[0] + 8.0, local[1] + 3.0]), color));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_a_window_only_for_the_box_that_is_on() {
        let off = OpenSettings::default();
        assert!(!off.opens(true) && !off.opens(false));
        assert!(OpenSettings { friends: true, strangers: false }.opens(true));
        assert!(!OpenSettings { friends: true, strangers: false }.opens(false));
        assert!(!OpenSettings { friends: false, strangers: true }.opens(true));
        assert!(OpenSettings { friends: false, strangers: true }.opens(false));
        let both = OpenSettings { friends: true, strangers: true };
        assert!(both.opens(true) && both.opens(false));
        assert_eq!(OpenSettings::decode(""), off);
        assert_eq!(OpenSettings::decode(&both.encode()), both);
    }

    #[test]
    fn recent_names_are_newest_first_and_unique() {
        let mut names = Vec::new();
        note_recent(&mut names, "Poring");
        note_recent(&mut names, "Drops");
        note_recent(&mut names, "poring");
        note_recent(&mut names, "  ");
        assert_eq!(names, ["poring", "Drops"]);
        for index in 0..12 {
            note_recent(&mut names, &format!("N{index}"));
        }
        assert_eq!(names.len(), 10);
        assert_eq!(names[0], "N11");
    }

    #[test]
    fn away_replies_once_per_sender() {
        let mut away = Away::default();
        assert!(away.reply("Poring").is_none());
        away.set("brb");
        assert_eq!(away.reply("Poring").as_deref(), Some("brb"));
        assert!(away.reply("Poring").is_none());
        assert!(away.reply("poring").is_none());
        assert_eq!(away.reply("Drops").as_deref(), Some("brb"));
        away.set("brb");
        assert_eq!(away.reply("Poring").as_deref(), Some("brb"));
        away.set("   ");
        assert!(away.reply("Drops").is_none());
        assert!(away.text().is_none());
    }

    #[test]
    fn one_window_per_partner_and_the_stranger_box() {
        let mut talks = Talks::default();
        let settings = OpenSettings { strangers: true, ..OpenSettings::default() };
        assert!(settings.opens(false));
        talks.ensure("Poring");
        talks.push_line("Poring", "(From Poring) : hi".into());
        talks.ensure("poring");
        assert_eq!(talks.window_names(), ["Whisper:Poring"]);
        assert_eq!(talks.windows[0].lines.len(), 1);
        let input = {
            let [x, y, w, h] = talks.button_boxes().into_iter().find(|(name, _)| name == "whisper_input:Poring").unwrap().1;
            [x + w / 2.0, y + h / 2.0]
        };
        assert_eq!(talks.hit(input).map(|(_, hit)| hit), Some(TalkHit::Input));
        talks.click(0, TalkHit::Input, input);
        talks.type_text("hello");
        assert_eq!(talks.take_input(), Some(("Poring".into(), "hello".into())));
    }

    #[test]
    fn friend_setup_toggles_each_box() {
        let mut setup = FriendSetup::new(OpenSettings::default(), [400.0, 180.0]);
        let point = |setup: &FriendSetup, name: &str| {
            let [x, y, w, h] = setup.button_boxes().into_iter().find(|(found, _)| *found == name).unwrap().1;
            [x + w / 2.0, y + h / 2.0]
        };
        let strangers = point(&setup, "strangers");
        let friends = point(&setup, "friends");
        assert_eq!(setup.click(setup.hit(strangers).unwrap()), Some(SetupClick::Changed(OpenSettings { strangers: true, ..OpenSettings::default() })));
        assert_eq!(setup.click(setup.hit(friends).unwrap()), Some(SetupClick::Changed(OpenSettings { friends: true, strangers: true })));
        assert!(setup.settings.friends && setup.settings.strangers);
        assert_eq!(setup.click(SetupHit::Title(TitleHit::Close)), Some(SetupClick::Close));
    }

    #[test]
    fn block_list_allow_is_the_row_button() {
        let list = BlockList::new(vec!["Poring".into(), "Drops".into()], "none".into());
        let [x, y, w, h] = list.button_boxes().into_iter().find(|(name, _)| name == "allow:Poring").unwrap().1;
        assert_eq!(list.hit([x + w / 2.0, y + h / 2.0]), Some(BlockHit::Allow("Poring".into())));
        let mut list = list;
        list.remove("poring");
        assert_eq!(list.names(), ["Drops"]);
    }

    #[test]
    fn saves_the_log_beside_the_screenshots() {
        let folder = std::env::temp_dir().join(format!("rustnarok-whisper-{}", std::process::id()));
        let path = save_log(&folder, "Po/ring", &["(From Po/ring) : hi".into()]).unwrap();
        assert_eq!(path, folder.join("whisper-Poring.txt"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "(From Po/ring) : hi");
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
