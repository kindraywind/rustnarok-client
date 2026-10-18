//! The chat window at the bottom of the screen: two tabs each with its own log, the log and the input line, and the
//! tab settings window that picks which kinds of lines each tab shows.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use rustnarok_client::grf::Grf;
use rustnarok_client::text::Font;

use crate::links::{self, ItemLink, Target};
use crate::render::{Gpu, Texture};
use crate::window::{Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI, contains, screen};

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
/// The whisper-name list that drops from the list button. It grows upward: the bar sits on the screen's bottom.
// ponytail: this button's job is the session's recent whisper names. The list opens upward so it stays on screen.
const LIST_BUTTON_SIZE: [f32; 2] = [12.0, 18.0];
const LIST_ROW: f32 = 16.0;
const LIST_WIDTH: f32 = 140.0;
const MESSAGE_BOX: ([f32; 2], [f32; 2]) = ([110.0, 4.0], [460.0, 16.0]);
const SEND_TO_BUTTON: [f32; 2] = [572.0, 7.0];
const SIZE_BUTTON: [f32; 2] = [586.0, 7.0];
const BUTTON_SIZE: [f32; 2] = [11.0, 11.0];
/// The input bar buttons' tooltips: the whisper list, the receiving group, the size.
const TOOLTIPS: [usize; 3] = [3187, 3188, 3189];
/// The stick, battle-option and option buttons right after the two tabs, and the lock at the strip's right.
const FUNCTION_BUTTONS: [[f32; 2]; 3] = [[2.0 * TAB_WIDTH + 4.0, 4.0], [2.0 * TAB_WIDTH + 19.0, 5.0], [2.0 * TAB_WIDTH + 34.0, 5.0]];
// ponytail: the lock's x; at the right end of the tab strip.
const LOCK_BUTTON: [f32; 2] = [WIDTH - 18.0, 4.0];
const TAB_MESSAGES: [usize; 2] = [1291, 1292];
/// The line over the input bar.
const FRAME_LINE: [u8; 4] = [142, 142, 142, 0xFF];
/// The most the whisper name and the message take, in bytes as sent (CP949).
// ponytail: counted in CP949 bytes, as the server reads them, not in characters.
pub const NAME_MOST: usize = 24;
pub const MESSAGE_MOST: usize = 234;
/// Party chat: the player's own lines and the others'.
pub const PARTY_OURS: [u8; 3] = [255, 200, 0];
pub const PARTY: [u8; 3] = [255, 200, 200];
/// Chat colours the kinds are read from: the player's own line, the guild's and the clan's, a skill's message.
const OWN: [u8; 3] = [0x00, 0xFF, 0x00];
const PUBLIC: [u8; 3] = [255, 255, 255];
const GUILD: [u8; 3] = [180, 255, 180];
const CLAN: [u8; 3] = [180, 220, 255];
const SKILL_MESSAGE: [u8; 3] = [0xCD, 0xCD, 0xFF];
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

/// What kind of line a chat line is, for which tab shows it.
// ponytail: the kinds and how a line's colour tells its kind (whispers say so themselves) are ours.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Public,
    Own,
    Whisper,
    Party,
    Guild,
    Clan,
    /// Party booking's search messages (nothing sends these yet).
    PartySearch,
    Battle,
    System,
}

impl Kind {
    pub const ALL: [Kind; 9] = [Kind::Public, Kind::Own, Kind::Whisper, Kind::Party, Kind::Guild, Kind::Clan, Kind::PartySearch, Kind::Battle, Kind::System];

    /// The kind a line in `color` is.
    pub fn of_color(color: [u8; 3]) -> Self {
        match color {
            PUBLIC => Kind::Public,
            OWN => Kind::Own,
            PARTY | PARTY_OURS => Kind::Party,
            GUILD => Kind::Guild,
            CLAN => Kind::Clan,
            SKILL_MESSAGE => Kind::Battle,
            _ => Kind::System,
        }
    }

    /// Its name in the tab settings window: the game's where it has one.
    // ponytail: the names the game's table doesn't have are ours.
    pub fn label(self, messages: &[String]) -> String {
        let message = |id: usize, fallback: &str| messages.get(id).filter(|text| !text.is_empty()).cloned().unwrap_or_else(|| fallback.to_owned());
        match self {
            Kind::Public => "Public Chat".into(),
            Kind::Own => "My Chat".into(),
            Kind::Whisper => message(1278, "Whisper"),
            Kind::Party => message(1279, "Party Chat"),
            Kind::Guild => message(1280, "Guild Chat"),
            Kind::Clan => "Clan Chat".into(),
            Kind::PartySearch => message(1549, "Party member search"),
            Kind::Battle => "Battle Messages".into(),
            Kind::System => "System Messages".into(),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Kind::Public => "public",
            Kind::Own => "own",
            Kind::Whisper => "whisper",
            Kind::Party => "party",
            Kind::Guild => "guild",
            Kind::Clan => "clan",
            Kind::PartySearch => "party_search",
            Kind::Battle => "battle",
            Kind::System => "system",
        }
    }
}

/// A tab: the kinds of lines it shows, and how far its log is scrolled back.
#[derive(Debug, Clone, PartialEq)]
pub struct Tab {
    pub kinds: HashSet<Kind>,
    scroll: usize,
}

/// The tabs' kinds when nothing is saved: Public everything but battle messages; Battle those, whispers, party and
/// guild.
// ponytail: the defaults are a choice.
pub fn default_kinds() -> [HashSet<Kind>; 2] {
    [Kind::ALL.into_iter().filter(|kind| *kind != Kind::Battle).collect(), [Kind::Battle, Kind::Whisper, Kind::Party, Kind::Guild].into_iter().collect()]
}

/// The tabs' kinds as saved: a line a tab, its kinds' names.
pub fn kinds_from(text: &str) -> Option<[HashSet<Kind>; 2]> {
    let mut lines = text.lines();
    let mut tab = || -> Option<HashSet<Kind>> {
        Some(lines.next()?.split_whitespace().filter_map(|name| Kind::ALL.into_iter().find(|kind| kind.name() == name)).collect())
    };
    Some([tab()?, tab()?])
}

pub fn kinds_text(kinds: &[HashSet<Kind>; 2]) -> String {
    kinds
        .iter()
        .map(|kinds| Kind::ALL.into_iter().filter(|kind| kinds.contains(kind)).map(Kind::name).collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>()
        .join("\n")
}

fn settings_path() -> Option<PathBuf> {
    crate::settings_path("chattabs")
}

/// Where the line typed goes when no key says otherwise (the button at the input bar's right).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum SendTo {
    #[default]
    Public,
    Party,
    Guild,
}

/// How much of `text` fits in `most` bytes as sent (CP949), whole characters.
pub fn capped(text: &str, most: usize) -> &str {
    let mut used = 0;
    for (at, character) in text.char_indices() {
        let mut buffer = [0; 4];
        used += encoding_rs::EUC_KR.encode(character.encode_utf8(&mut buffer)).0.len();
        if used > most {
            return &text[..at];
        }
    }
    text
}

pub struct Assets {
    input: Texture,
    list: Texture,
    base: Texture,
    /// The stick, battle-option and option buttons after the tabs: normal and pressed.
    functions: Vec<[Texture; 2]>,
    /// The lock at the strip's right: locked and not.
    lock: [Texture; 2],
    dark: Texture,
    darker: Texture,
    white: Texture,
    tab_border: Texture,
    frame_line: Texture,
    /// Item and navigation links' underlines.
    link: Texture,
    navi_link: Texture,
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let basic = format!("{UI}basic_interface\\");
        let image = |name: &str| gpu.load_texture(grf, &format!("{basic}{name}.bmp"), false);
        Self {
            input: image("dialog_bg"),
            list: image("dialog_btn0"),
            base: image("sys_base_off"),
            // ponytail: _a is the normal look and _b the pressed one; this data has no battle_option.bmp for the third,
            // so it takes battle_option2.
            functions: vec![
                [image("stickoff_a"), image("stickoff_b")],
                [image("battle_option_a"), image("battle_option_b")],
                [image("battle_option2_a"), image("battle_option2_b")],
            ],
            lock: [image("lock_dragwnd"), image("unlock_dragwnd")],
            dark: gpu.solid(DARK),
            darker: gpu.solid(DARKER),
            white: gpu.solid([255; 4]),
            tab_border: gpu.solid(TAB_BORDER),
            frame_line: gpu.solid(FRAME_LINE),
            link: gpu.solid([crate::links::COLOR[0], crate::links::COLOR[1], crate::links::COLOR[2], 0xFF]),
            navi_link: gpu.solid([crate::links::NAVI_COLOR[0], crate::links::NAVI_COLOR[1], crate::links::NAVI_COLOR[2], 0xFF]),
        }
    }
}

/// What a click on the chat window lands on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Hit {
    Tab(usize),
    Stick,
    Options,
    Lock,
    /// The tab strip's empty part: it drags the window.
    Strip,
    List,
    SendTo,
    Size,
    Name,
    Message,
    Log,
}

pub struct Chat {
    /// Every line so far with its color and kind.
    pub lines: Vec<(String, [u8; 3], Kind)>,
    /// What's being typed, and the syllable the input method is still composing.
    pub input: String,
    pub preedit: String,
    /// The whisper name box, and whether typing goes there.
    pub whisper: String,
    pub naming: bool,
    /// Players whispered with this session, newest first, and whether their list is open.
    pub recent: Vec<String>,
    list_open: bool,
    /// Which of F10's steps the window is at.
    size: usize,
    pub tabs: [Tab; 2],
    /// The tab shown.
    pub tab: usize,
    /// Where the window's bottom left corner is, once moved (else the screen's bottom left).
    corner: Option<[f32; 2]>,
    /// The pointer's offset from the corner while the window is dragged.
    drag: Option<[f32; 2]>,
    /// The lock: no dragging.
    pub locked: bool,
    pub send_to: SendTo,
    /// The tab settings window, while open.
    pub settings: Option<Settings>,
    /// Alt+F10 hides the window. Typing still goes to the line.
    pub hidden: bool,
}

impl Default for Chat {
    fn default() -> Self {
        let kinds = settings_path().and_then(|path| std::fs::read_to_string(path).ok()).and_then(|text| kinds_from(&text)).unwrap_or_else(default_kinds);
        Self::with_kinds(kinds)
    }
}

impl Chat {
    pub fn with_kinds(kinds: [HashSet<Kind>; 2]) -> Self {
        let [public, battle] = kinds;
        Self {
            lines: Vec::new(),
            input: String::new(),
            preedit: String::new(),
            whisper: String::new(),
            naming: false,
            recent: Vec::new(),
            list_open: false,
            size: STEPS.iter().position(|&step| step == DEFAULT_STEP).unwrap_or(0),
            tabs: [Tab { kinds: public, scroll: 0 }, Tab { kinds: battle, scroll: 0 }],
            tab: 0,
            corner: None,
            drag: None,
            locked: false,
            send_to: SendTo::default(),
            settings: None,
            hidden: false,
        }
    }

    fn step(&self) -> usize {
        STEPS[self.size]
    }

    pub fn height(&self) -> f32 {
        height(self.step())
    }

    /// The window's bottom left corner.
    fn corner(&self) -> [f32; 2] {
        self.corner.unwrap_or([0.0, screen()[1]])
    }

    /// Where the window's bottom left corner was moved to (None: the screen's bottom left).
    pub fn corner_moved(&self) -> Option<[f32; 2]> {
        self.corner
    }

    /// A window-local point on screen.
    fn at(&self, local: [f32; 2]) -> [f32; 2] {
        let [left, bottom] = self.corner();
        [left + local[0], bottom - self.height() + local[1]]
    }

    pub fn contains(&self, cursor: [f32; 2]) -> bool {
        !self.hidden && contains(self.at([0.0, 0.0]), [WIDTH, self.height()], cursor)
    }

    /// Alt+F10: the log and the input bar leave the screen, or come back.
    pub fn toggle_hidden(&mut self) {
        self.hidden = !self.hidden;
    }

    /// F10: the next size, the same cycle as the size button.
    pub fn cycle_size(&mut self) {
        self.size = (self.size + 1) % STEPS.len();
        self.tabs.iter_mut().for_each(|tab| tab.scroll = 0);
        self.keep_on_screen();
    }

    /// Whether a press here is the window's alone: its tabs, the strips around the log and the input bar. The log
    /// box's lines let presses through to the world (the wheel still scrolls them).
    pub fn blocks(&self, cursor: [f32; 2]) -> bool {
        self.contains(cursor) && self.hit(cursor) != Some(Hit::Log)
    }

    /// What `cursor` is on.
    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        if !self.contains(cursor) {
            return None;
        }
        let top = self.at([0.0, 0.0]);
        let local = [cursor[0] - top[0], cursor[1] - top[1]];
        let bar = self.height() - INPUT_HEIGHT;
        let on = |corner: [f32; 2], size: [f32; 2]| contains(corner, size, local);
        let in_bar = |offset: [f32; 2], size: [f32; 2]| on([offset[0], bar + offset[1]], size);
        if local[1] >= bar {
            return Some(if in_bar(SIZE_BUTTON, BUTTON_SIZE) {
                Hit::Size
            } else if in_bar(SEND_TO_BUTTON, BUTTON_SIZE) {
                Hit::SendTo
            } else if in_bar(LIST_BUTTON, LIST_BUTTON_SIZE) {
                Hit::List
            } else if in_bar(NAME_BOX.0, NAME_BOX.1) {
                Hit::Name
            } else {
                // The message box, and the rest of the bar with it.
                Hit::Message
            });
        }
        if local[1] < TAB_HEIGHT {
            let small = [14.0, 14.0];
            return Some(if let Some(tab) = (0..2).find(|&tab| on([tab as f32 * TAB_WIDTH, 0.0], [TAB_WIDTH, TAB_HEIGHT])) {
                Hit::Tab(tab)
            } else if on(FUNCTION_BUTTONS[0], small) {
                Hit::Stick
            } else if on(FUNCTION_BUTTONS[1], small) || on(FUNCTION_BUTTONS[2], small) {
                Hit::Options
            } else if on(LOCK_BUTTON, small) {
                Hit::Lock
            } else {
                Hit::Strip
            });
        }
        Some(if on(LOG_BOX, [LOG_WIDTH, bar - LOG_BOX[1]]) { Hit::Log } else { Hit::Strip })
    }

    /// The tabs and buttons on screen (for the test driver).
    pub fn button_boxes(&self) -> Vec<(&'static str, [f32; 4])> {
        if self.hidden {
            return Vec::new();
        }
        let area = |local: [f32; 2], size: [f32; 2]| {
            let [x, y] = self.at(local);
            [x, y, size[0], size[1]]
        };
        let bar = self.height() - INPUT_HEIGHT;
        let mut boxes = vec![
            ("chat_send_to", area([SEND_TO_BUTTON[0], bar + SEND_TO_BUTTON[1]], BUTTON_SIZE)),
            ("chat_size", area([SIZE_BUTTON[0], bar + SIZE_BUTTON[1]], BUTTON_SIZE)),
        ];
        if self.step() > 0 {
            let small = [14.0, 14.0];
            boxes.extend([
                ("chat_tab_0", area([0.0, 0.0], [TAB_WIDTH, TAB_HEIGHT])),
                ("chat_tab_1", area([TAB_WIDTH, 0.0], [TAB_WIDTH, TAB_HEIGHT])),
                ("chat_stick", area(FUNCTION_BUTTONS[0], small)),
                ("chat_options", area(FUNCTION_BUTTONS[2], small)),
                ("chat_lock", area(LOCK_BUTTON, small)),
                ("chat_strip", area([2.0 * TAB_WIDTH + 60.0, 2.0], [100.0, 12.0])),
            ]);
        }
        boxes
    }

    /// The tooltip for the input bar button under `cursor`.
    pub fn tooltip(&self, cursor: [f32; 2]) -> Option<usize> {
        match self.hit(cursor)? {
            Hit::List => Some(TOOLTIPS[0]),
            Hit::SendTo => Some(TOOLTIPS[1]),
            Hit::Size => Some(TOOLTIPS[2]),
            _ => None,
        }
    }

    /// A press: on the empty part of the tab strip it starts a drag (unless locked).
    pub fn press(&mut self, cursor: [f32; 2]) -> bool {
        if self.locked || self.hit(cursor) != Some(Hit::Strip) {
            return false;
        }
        let corner = self.corner();
        self.drag = Some([cursor[0] - corner[0], cursor[1] - corner[1]]);
        true
    }

    /// The pointer moved: a dragged window follows it, kept on the screen.
    pub fn drag_to(&mut self, cursor: [f32; 2]) {
        if let Some(offset) = self.drag {
            self.corner = Some([cursor[0] - offset[0], cursor[1] - offset[1]]);
            self.keep_on_screen();
        }
    }

    /// The drag ends; whether there was one.
    pub fn drop_drag(&mut self) -> bool {
        self.drag.take().is_some()
    }

    fn keep_on_screen(&mut self) {
        if let Some([left, bottom]) = self.corner {
            let screen = screen();
            self.corner = Some([left.clamp(0.0, (screen[0] - WIDTH).max(0.0)), bottom.clamp(self.height(), screen[1])]);
        }
    }

    /// Adds a line of the kind its colour tells.
    pub fn push(&mut self, text: String, color: [u8; 3]) {
        self.push_kind(text, color, Kind::of_color(color));
    }

    pub fn push_kind(&mut self, text: String, color: [u8; 3], kind: Kind) {
        self.lines.push((text, color, kind));
        // New lines show unless the tab is scrolled back.
        for tab in &mut self.tabs {
            if tab.scroll > 0 && tab.kinds.contains(&kind) {
                tab.scroll += 1;
            }
        }
    }

    /// The lines tab `tab` shows, oldest first.
    pub fn tab_lines(&self, tab: usize) -> Vec<&(String, [u8; 3], Kind)> {
        self.lines.iter().filter(|(.., kind)| self.tabs[tab].kinds.contains(kind)).collect()
    }

    /// Scrolls the shown tab's log by `rows` lines (negative: back in time).
    pub fn scroll_by(&mut self, rows: i32) {
        let most = self.tab_lines(self.tab).len().saturating_sub(self::rows(self.step()));
        let tab = &mut self.tabs[self.tab];
        tab.scroll = (tab.scroll as i32 - rows).clamp(0, most as i32) as usize;
    }

    /// Remembers a player whispered with this session.
    pub fn note_recent(&mut self, name: &str) {
        crate::whisper::note_recent(&mut self.recent, name);
    }

    /// A row of the open recent-name list.
    pub fn on_list(&self, cursor: [f32; 2]) -> bool {
        self.list_row(cursor).is_some()
    }

    /// The list button and each open row, for the test driver.
    pub fn list_boxes(&self) -> Vec<(String, [f32; 4])> {
        if self.hidden {
            return Vec::new();
        }
        let mut boxes = vec![("whisper_list".into(), self.list_button_box())];
        if self.list_open {
            for (index, name) in self.recent.iter().enumerate() {
                boxes.push((format!("recent:{name}"), self.row_box(index)));
            }
        }
        boxes
    }

    fn list_button_box(&self) -> [f32; 4] {
        let origin = self.at([LIST_BUTTON[0], self.height() - INPUT_HEIGHT + LIST_BUTTON[1]]);
        [origin[0], origin[1], LIST_BUTTON_SIZE[0], LIST_BUTTON_SIZE[1]]
    }

    fn row_box(&self, index: usize) -> [f32; 4] {
        let [x, y, _, _] = self.list_button_box();
        [x, y - (index as f32 + 1.0) * LIST_ROW, LIST_WIDTH, LIST_ROW]
    }

    fn list_row(&self, cursor: [f32; 2]) -> Option<usize> {
        if !self.list_open {
            return None;
        }
        self.recent.iter().enumerate().find(|(index, _)| contains_box(self.row_box(*index), cursor)).map(|(index, _)| index)
    }

    /// A click on the window: a tab shows its log; the buttons do what they say; the boxes of the input bar take the
    /// caret. The list button drops the recent names; a row fills the name box.
    pub fn click(&mut self, cursor: [f32; 2]) {
        // Rows sit above the button and can fall outside the window, so they come before `hit`.
        if let Some(index) = self.list_row(cursor) {
            self.whisper = self.recent[index].clone();
            self.naming = false;
            self.list_open = false;
            return;
        }
        if self.hit(cursor) == Some(Hit::List) {
            self.list_open = !self.list_open;
            return;
        }
        self.list_open = false;
        match self.hit(cursor) {
            Some(Hit::Tab(tab)) => self.tab = tab,
            // ponytail: "stick" puts the window back at the screen's bottom left.
            Some(Hit::Stick) => self.corner = None,
            Some(Hit::Options) => self.settings = if self.settings.is_some() { None } else { Some(Settings::new(self.tab)) },
            Some(Hit::Lock) => self.locked = !self.locked,
            Some(Hit::Size) => self.cycle_size(),
            // ponytail: "Select receiving group" turns where a line goes: public, the party, the guild.
            Some(Hit::SendTo) => {
                self.send_to = match self.send_to {
                    SendTo::Public => SendTo::Party,
                    SendTo::Party => SendTo::Guild,
                    SendTo::Guild => SendTo::Public,
                }
            }
            Some(Hit::Name) => self.naming = true,
            Some(Hit::Message) => self.naming = false,
            _ => {}
        }
    }

    /// Typing goes to the box with the caret.
    pub fn typing(&mut self) -> &mut String {
        if self.naming { &mut self.whisper } else { &mut self.input }
    }

    /// Types `text` into the box with the caret, up to that box's most.
    pub fn type_text(&mut self, text: &str) {
        let most = if self.naming { NAME_MOST } else { MESSAGE_MOST };
        let typing = self.typing();
        let joined = format!("{typing}{text}");
        *typing = capped(&joined, most).to_owned();
    }

    /// Types `text` whole into the box with the caret if it fits its most (an item link); else nothing.
    pub fn type_whole(&mut self, text: &str) {
        let most = if self.naming { NAME_MOST } else { MESSAGE_MOST };
        let joined = format!("{}{text}", self.typing());
        if capped(&joined, most).len() == joined.len() {
            *self.typing() = joined;
        }
    }

    /// Ticks or unticks `kind` for the settings window's tab, and saves the tabs.
    pub fn toggle_kind(&mut self, kind: Kind) {
        let Some(settings) = &self.settings else { return };
        let kinds = &mut self.tabs[settings.tab].kinds;
        if !kinds.remove(&kind) {
            kinds.insert(kind);
        }
        self.tabs[settings.tab].scroll = 0;
        if let Some(path) = settings_path() {
            self.save_to(&path);
        }
    }

    pub fn save_to(&self, path: &Path) {
        let kinds = [self.tabs[0].kinds.clone(), self.tabs[1].kinds.clone()];
        let _ = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(path, kinds_text(&kinds)));
    }

    /// The recent-name list next to the whisper name box is open.
    pub fn whisper_list_open(&self) -> bool {
        self.list_open
    }

    /// Closes the recent-name list.
    pub fn close_whisper_list(&mut self) {
        self.list_open = false;
    }

    /// Closes the battle-mode line (#179): the typed text goes, the line closes.
    pub fn close_battle_line(&mut self) {
        self.input.clear();
        self.preedit.clear();
        self.naming = false;
    }

    /// The window, and the boxes of the links in its log with their targets; the input bar only while the line is
    /// `open`. `name_of` names an item a link carries without words.
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
    ) -> Vec<([f32; 4], Target)> {
        let mut link_boxes = Vec::new();
        if self.hidden {
            return link_boxes;
        }
        let at = |local: [f32; 2]| self.at(local);
        if self.step() > 0 {
            // Tabs: the shown one in the open look.
            for (tab, &message) in TAB_MESSAGES.iter().enumerate() {
                let [left, top] = at([tab as f32 * TAB_WIDTH, 0.0]);
                let (fill, border) = if tab == self.tab { (&assets.dark, &assets.white) } else { (&assets.darker, &assets.tab_border) };
                layer.sprites.push((fill, [left, top + 1.0], [TAB_WIDTH, TAB_HEIGHT - 1.0]));
                layer.sprites.push((border, [left, top], [TAB_WIDTH, 1.0]));
                layer.sprites.push((border, [left, top], [1.0, TAB_HEIGHT]));
                layer.sprites.push((border, [left + TAB_WIDTH - 1.0, top], [1.0, TAB_HEIGHT]));
                let label = messages.get(message).map_or("", String::as_str);
                let x = (left + (TAB_WIDTH - font.width(label)) / 2.0).round();
                layer.texts.push((label.to_owned(), [x + 1.0, top + 2.0], [0, 0, 0]));
                layer.texts.push((label.to_owned(), [x, top + 1.0], [255; 3]));
            }
            // The buttons after the tabs: the options one pressed while the settings are open.
            for (index, (images, corner)) in assets.functions.iter().zip(FUNCTION_BUTTONS).enumerate() {
                let pressed = (index == 0 && self.corner.is_none()) || (index > 0 && self.settings.is_some());
                let image = &images[usize::from(pressed)];
                layer.sprites.push((image, at(corner), image.size));
            }
            let lock = &assets.lock[usize::from(!self.locked)];
            layer.sprites.push((lock, at(LOCK_BUTTON), lock.size));

            // The log: translucent black between white edges, newest line at the bottom.
            let corner = at(LOG_BOX);
            let size = [LOG_WIDTH, self.height() - LOG_UNDER];
            layer.sprites.push((&assets.dark, corner, size));
            layer.sprites.push((&assets.white, corner, [1.0, size[1]]));
            layer.sprites.push((&assets.white, [corner[0] + size[0] - 1.0, corner[1]], [1.0, size[1]]));
            layer.sprites.push((&assets.white, at([2.0 * TAB_WIDTH, LOG_BOX[1]]), [LOG_BOX[0] + size[0] - 2.0 * TAB_WIDTH, 1.0]));
            let lines = self.tab_lines(self.tab);
            let shown = rows(self.step());
            let end = lines.len().saturating_sub(self.tabs[self.tab].scroll);
            let start = end.saturating_sub(shown);
            let first_row = shown - (end - start);
            for (row, (line, color, _)) in lines[start..end].iter().enumerate() {
                let [mut x, y] = at(line_at(first_row + row));
                // Links in their colour, underlined.
                for (piece, item) in links::pieces(&links::mark(line, name_of)) {
                    let width = font.width(&piece);
                    let color = item.as_ref().map_or(*color, Target::color);
                    layer.texts.push((piece.clone(), [x + 1.0, y + 1.0], [0, 0, 0]));
                    layer.texts.push((piece, [x, y], color));
                    if let Some(item) = item {
                        let under = y + font.line_height() - 1.0;
                        let line = if item.item().is_some() { &assets.link } else { &assets.navi_link };
                        layer.sprites.push((line, [x, under], [width, 1.0]));
                        link_boxes.push(([x, y, width, font.line_height()], item));
                    }
                    x += width;
                }
            }
            // The line over the input bar.
            layer.sprites.push((&assets.frame_line, at([4.0, self.height() - INPUT_HEIGHT]), [592.0, 1.0]));
        }
        if !open {
            return link_boxes;
        }

        // The input bar with what's being typed and the caret.
        let bar = self.height() - INPUT_HEIGHT;
        let in_bar = |offset: [f32; 2]| at([offset[0], bar + offset[1]]);
        layer.sprites.push((&assets.input, in_bar([0.0, 0.0]), assets.input.size));
        layer.sprites.push((&assets.list, in_bar(LIST_BUTTON), assets.list.size));
        for button in [SEND_TO_BUTTON, SIZE_BUTTON] {
            layer.sprites.push((&assets.base, in_bar(button), assets.base.size));
        }
        let [name_x, text_top] = in_bar([NAME_BOX.0[0], NAME_BOX.0[1] + ((NAME_BOX.1[1] - font.line_height()) / 2.0).round()]);
        let message_x = in_bar(MESSAGE_BOX.0)[0];
        let (name, typed) = if self.naming {
            (format!("{}{}", self.whisper, self.preedit), self.input.clone())
        } else {
            (self.whisper.clone(), format!("{}{}", self.input, self.preedit))
        };
        layer.texts.push((name.clone(), [name_x, text_top], [0, 0, 0]));
        layer.texts.push((typed.clone(), [message_x, text_top], [0, 0, 0]));
        if caret {
            let end = if self.naming { name_x + font.width(&name) } else { message_x + font.width(&typed) };
            layer.sprites.push((&assets.darker, [end + 1.0, text_top + 1.0], [1.0, font.line_height() - 2.0]));
        }
        if self.list_open {
            for (index, recent) in self.recent.iter().enumerate() {
                let [x, y, w, h] = self.row_box(index);
                layer.sprites.push((&assets.darker, [x, y], [w, h]));
                layer.texts.push((recent.clone(), [x + 4.0, y + 1.0], [255, 255, 255]));
            }
        }
        link_boxes
    }
}

fn contains_box(box_: [f32; 4], cursor: [f32; 2]) -> bool {
    contains([box_[0], box_[1]], [box_[2], box_[3]], cursor)
}

/// The tab settings window: a box per kind of line for the tab it was opened from.
// ponytail: its layout is ours over the client's chrome.
pub struct Settings {
    pub frame: Frame,
    pub tab: usize,
}

const SETTINGS_WIDTH: f32 = 200.0;
const SETTINGS_ROW: f32 = 18.0;
const SETTINGS_TOP: f32 = TITLE_HEIGHT + 6.0;
const SETTINGS_HEIGHT: f32 = SETTINGS_TOP + Kind::ALL.len() as f32 * SETTINGS_ROW + 8.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SettingsHit {
    Title(TitleHit),
    Kind(Kind),
    Window,
}

impl Settings {
    pub fn new(tab: usize) -> Self {
        let mut frame = Frame::new([620.0, screen()[1] - 360.0], SETTINGS_WIDTH);
        frame.open = true;
        Self { frame, tab }
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<SettingsHit> {
        let local = self.frame.local(cursor, SETTINGS_HEIGHT)?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(SettingsHit::Title(hit));
        }
        let row = ((local[1] - SETTINGS_TOP) / SETTINGS_ROW).floor();
        Some(if local[1] >= SETTINGS_TOP && (row as usize) < Kind::ALL.len() { SettingsHit::Kind(Kind::ALL[row as usize]) } else { SettingsHit::Window })
    }

    /// Each kind's row on screen (for the test driver), named "tab_<kind>".
    pub fn button_boxes(&self) -> Vec<(String, [f32; 4])> {
        Kind::ALL
            .iter()
            .enumerate()
            .map(|(row, kind)| {
                let [x, y] = self.frame.at([8.0, SETTINGS_TOP + row as f32 * SETTINGS_ROW]);
                (format!("tab_{}", kind.name()), [x, y, SETTINGS_WIDTH - 16.0, SETTINGS_ROW])
            })
            .collect()
    }

    pub fn draw<'a>(&self, layer: &mut Layer<'a>, chrome: &'a Chrome, messages: &[String], kinds: &HashSet<Kind>, pointer: [f32; 2]) {
        let at = |local: [f32; 2]| self.frame.at(local);
        let title = messages.get(TAB_MESSAGES[self.tab]).cloned().unwrap_or_default();
        self.frame.draw_title(layer, chrome, &title, pointer);
        if self.frame.minimized {
            return;
        }
        layer.sprites.push((&chrome.white, at([0.0, TITLE_HEIGHT]), [SETTINGS_WIDTH, SETTINGS_HEIGHT - TITLE_HEIGHT]));
        for x in [0.0, SETTINGS_WIDTH - 1.0] {
            layer.sprites.push((&chrome.border, at([x, TITLE_HEIGHT]), [1.0, SETTINGS_HEIGHT - TITLE_HEIGHT]));
        }
        layer.sprites.push((&chrome.border, at([0.0, SETTINGS_HEIGHT - 1.0]), [SETTINGS_WIDTH, 1.0]));
        for (row, kind) in Kind::ALL.iter().enumerate() {
            let mark = if kinds.contains(kind) { "■" } else { "□" };
            layer.texts.push((format!("{mark} {}", kind.label(messages)), at([8.0, SETTINGS_TOP + row as f32 * SETTINGS_ROW]), [0, 0, 0]));
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
    Whispers {
        name: String,
        allow: bool,
    },
    /// /ex alone: the names blocked.
    BlockList,
    /// /exall, /inall: block or allow everyone's whispers.
    AllWhispers {
        allow: bool,
    },
    /// /effect: effects off or on.
    Effect,
    /// /emo, /emotion: the emotion list, open or closed.
    EmotionList,
    /// /memo: where the player stands becomes a Warp Portal destination.
    Memo,
    /// /achievement: the achievement window, open or closed.
    Achievements,
    /// /noshift: a click on a player attacks without Shift, on or off.
    NoShift,
    /// /camera: the camera sticks to the player instead of gliding after them, on or off.
    Camera,
    /// /zoom: the camera can go further out outdoors, on or off.
    Zoom,
    /// /noctrl, /nc: a click on a monster keeps attacking without Ctrl, on or off.
    NoCtrl,
    /// /snap, /itemsnap, /skillsnap: the cursor jumps to the middle of a monster, of an item, or of a monster while
    /// aiming a skill; each on or off.
    Snap,
    ItemSnap,
    SkillSnap,
    /// /battlemode, /bm: battle mode. The chat line closes and letter keys are shortcuts.
    BattleMode,
    /// /battlechat: typed lines go to the battleground team, on or off.
    BattleChat,
    /// /dressroom: the dress room.
    DressRoom,
    /// /blacksmith, /alchemist, /taekwon, /pk: a ranking's top ten (0 to 3).
    Ranking(u16),
    /// /away: the away-message box.
    // ponytail: /away is the command that opens it; the client's own command isn't identified.
    Away,
    /// /navigation, /navi: the navigation window, open or closed.
    Navigation,
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
        "/noshift" => Command::NoShift,
        "/noctrl" | "/nc" => Command::NoCtrl,
        "/snap" => Command::Snap,
        "/itemsnap" => Command::ItemSnap,
        "/skillsnap" => Command::SkillSnap,
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
        "/away" => Command::Away,
        "/navigation" | "/navi" => Command::Navigation,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A chat with the default tabs (not the saved ones).
    fn fresh() -> Chat {
        Chat::with_kinds(default_kinds())
    }

    fn top(chat: &Chat) -> f32 {
        chat.at([0.0, 0.0])[1]
    }

    #[test]
    fn lets_presses_through_the_log() {
        let chat = fresh();
        let log_top = screen()[1] - 96.0 + LOG_BOX[1];
        // The log's lines: the window has them, the world too.
        assert!(chat.contains([300.0, log_top + 10.0]) && !chat.blocks([300.0, log_top + 10.0]));
        // The input bar and the tabs are the window's alone.
        assert!(chat.blocks([300.0, screen()[1] - 10.0]) && chat.blocks([20.0, top(&chat) + 5.0]));
        assert!(!chat.contains([700.0, log_top + 10.0]) && !chat.blocks([700.0, log_top + 10.0]));
    }

    #[test]
    fn steps_through_six_sizes() {
        let mut chat = fresh();
        let mut heights = Vec::new();
        for _ in 0..7 {
            heights.push((chat.height(), rows(chat.step()), top(&chat)));
            chat.cycle_size();
        }
        let tall = [96.0, 138.0, 180.0, 222.0, 264.0, 24.0, 96.0];
        assert_eq!(heights.iter().map(|&(height, ..)| height).collect::<Vec<_>>(), tall);
        assert_eq!(heights.iter().map(|&(_, rows, _)| rows).collect::<Vec<_>>(), [3, 6, 9, 12, 15, 0, 3]);
        // The bottom edge stays put.
        assert!(heights.iter().all(|&(height, _, top)| top + height == screen()[1]));
        // The input bar alone: no log to let presses through.
        let mut alone = fresh();
        (0..5).for_each(|_| alone.cycle_size());
        assert_eq!(alone.height(), 24.0);
        assert!(alone.blocks([300.0, screen()[1] - 10.0]) && !alone.contains([300.0, screen()[1] - 30.0]));
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
        assert_eq!([local(SEND_TO_BUTTON), local(SIZE_BUTTON)], [[572.0, h - 17.0], [586.0, h - 17.0]]);
        assert_eq!(FUNCTION_BUTTONS, [[224.0, 4.0], [239.0, 5.0], [254.0, 5.0]]);
        // The boxes take the caret, the size button the size, the receiving group turns.
        let mut chat = fresh();
        chat.click([50.0, screen()[1] - 12.0]);
        assert!(chat.naming);
        chat.click([300.0, screen()[1] - 12.0]);
        assert!(!chat.naming);
        assert_eq!(chat.tooltip([576.0, screen()[1] - 12.0]), Some(3188));
        chat.click([576.0, screen()[1] - 12.0]);
        assert_eq!(chat.send_to, SendTo::Party);
        chat.click([590.0, screen()[1] - 12.0]);
        assert_eq!(chat.height(), 138.0);
    }

    #[test]
    fn recent_names_fill_the_whisper_box() {
        let mut chat = fresh();
        chat.note_recent("Drops");
        chat.note_recent("Poring");
        let [x, y, w, h] = chat.list_boxes().into_iter().find(|(name, _)| name == "whisper_list").unwrap().1;
        chat.click([x + w / 2.0, y + h / 2.0]);
        assert!(chat.whisper_list_open());
        let [x, y, w, h] = chat.list_boxes().into_iter().find(|(name, _)| name == "recent:Poring").unwrap().1;
        chat.click([x + w / 2.0, y + h / 2.0]);
        assert_eq!(chat.whisper, "Poring");
        assert!(!chat.whisper_list_open());
        chat.close_whisper_list();
    }

    #[test]
    fn picks_tabs_and_routes_lines_by_kind() {
        let mut chat = fresh();
        let strip = top(&chat) + 8.0;
        assert_eq!([chat.hit([50.0, strip]), chat.hit([150.0, strip]), chat.hit([400.0, strip])], [Some(Hit::Tab(0)), Some(Hit::Tab(1)), Some(Hit::Strip)]);
        chat.push("Poring : hi".into(), PUBLIC);
        chat.push("Leader : party".into(), PARTY);
        chat.push_kind("(From A) : psst".into(), [0xFF, 0xFF, 0x00], Kind::Whisper);
        chat.push("You used Bash.".into(), SKILL_MESSAGE);
        let texts = |chat: &Chat, tab: usize| chat.tab_lines(tab).iter().map(|(text, ..)| text.clone()).collect::<Vec<_>>();
        assert_eq!(texts(&chat, 0), ["Poring : hi", "Leader : party", "(From A) : psst"]);
        assert_eq!(texts(&chat, 1), ["Leader : party", "(From A) : psst", "You used Bash."]);
        chat.click([150.0, strip]);
        assert_eq!(chat.tab, 1);
        // Party unticked for Public: party lines leave its log.
        chat.tabs[0].kinds.remove(&Kind::Party);
        assert_eq!(texts(&chat, 0), ["Poring : hi", "(From A) : psst"]);
    }

    #[test]
    fn scrolls_each_tab_on_its_own() {
        let mut chat = fresh();
        for line in 0..10 {
            chat.push(format!("line {line}"), PUBLIC);
        }
        chat.scroll_by(-4);
        assert_eq!((chat.tabs[0].scroll, chat.tabs[1].scroll), (4, 0));
        // A new public line keeps Public where it was scrolled; Battle doesn't show it.
        chat.push("line 10".into(), PUBLIC);
        assert_eq!((chat.tabs[0].scroll, chat.tabs[1].scroll), (5, 0));
        chat.tab = 1;
        chat.scroll_by(-4);
        assert_eq!(chat.tabs[1].scroll, 0, "nothing to scroll back to");
    }

    #[test]
    fn caps_what_is_typed() {
        let mut chat = fresh();
        chat.type_text(&"a".repeat(300));
        assert_eq!(chat.input.len(), MESSAGE_MOST);
        // Korean counts two bytes a syllable as sent.
        chat.naming = true;
        chat.type_text(&"가".repeat(20));
        assert_eq!(chat.whisper.chars().count(), NAME_MOST / 2);
        assert_eq!(capped("ab가", 3), "ab");
        // A link goes in whole or not at all.
        chat.naming = false;
        chat.input.clear();
        chat.type_text(&"a".repeat(230));
        chat.type_whole("<ITEM>Apple<INFO>512</INFO></ITEM>");
        assert_eq!(chat.input.len(), 230);
    }

    #[test]
    fn drags_by_the_strip_and_locks() {
        let mut chat = fresh();
        let strip = [400.0, top(&chat) + 8.0];
        assert!(chat.press(strip));
        chat.drag_to([500.0, strip[1] - 200.0]);
        assert!(chat.drop_drag());
        assert_eq!(chat.corner(), [100.0, screen()[1] - 200.0]);
        // Kept on the screen.
        assert!(chat.press([500.0, top(&chat) + 8.0]));
        chat.drag_to([-500.0, -500.0]);
        chat.drop_drag();
        assert_eq!(chat.corner(), [0.0, chat.height()]);
        chat.locked = true;
        assert!(!chat.press([400.0, top(&chat) + 8.0]));
    }

    #[test]
    fn saves_the_tabs_kinds() {
        let kinds = default_kinds();
        let text = kinds_text(&kinds);
        assert_eq!(text.lines().nth(1), Some("whisper party guild battle"));
        assert_eq!(kinds_from(&text), Some(kinds));
        assert_eq!(kinds_from("public"), None);
    }

    #[test]
    fn colors_party_chat() {
        assert_eq!([PARTY_OURS, PARTY], [[255, 200, 0], [255, 200, 200]]);
        assert_eq!([Kind::of_color(PARTY), Kind::of_color(GUILD), Kind::of_color([0xFF, 0, 0])], [Kind::Party, Kind::Guild, Kind::System]);
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
        assert_eq!(command("/noshift"), Some(Command::NoShift));
        assert_eq!([command("/noctrl"), command("/nc")], [Some(Command::NoCtrl), Some(Command::NoCtrl)]);
        assert_eq!([command("/snap"), command("/itemsnap"), command("/skillsnap")], [Some(Command::Snap), Some(Command::ItemSnap), Some(Command::SkillSnap)]);
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
        assert_eq!([command("/navigation"), command("/navi")], [Some(Command::Navigation), Some(Command::Navigation)]);
        assert_eq!(command("/away"), Some(Command::Away));
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
