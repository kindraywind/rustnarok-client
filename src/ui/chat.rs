//! The chat window at the bottom of the screen: two tabs each with its own log, the log and the input line, and the
//! tab settings window that picks which kinds of lines each tab shows.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use rustnarok_client::grf::Grf;
use rustnarok_client::text::Font;

use crate::line_edit::{Caret, History, Move};
use crate::links::{self, ItemLink, Target};
use crate::render::{Gpu, Texture};
use crate::window::{Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI, contains, screen};

/// The size steps go 0 to 15; F10 walks 0 → 1 → 4 → 7 → 10 → 13 → 0, dragging the top edge gives any.
const MOST_STEP: usize = 15;
/// The step a new window has: 96 tall.
const DEFAULT_STEP: usize = 1;
const WIDTH: f32 = 600.0;
const LINE: f32 = 14.0;
const TAB_HEIGHT: f32 = 16.0;
/// The tab strip starts at x 3; a tab is as wide as its label and 10, the label cut to 600 / tabs less 10.
const STRIP_X: f32 = 3.0;
const TAB_PADDING: f32 = 10.0;
/// A tab's width before the first frame measures its label.
const UNMEASURED_TAB: f32 = 50.0;
/// The most tabs and torn-off windows together, and "Cannot add more." past it; new tabs are "NewTab_<n>".
const MOST_TABS: usize = 10;
pub const CANNOT_ADD_MORE: usize = 1578;
const NEW_TAB: &str = "NewTab_";
/// A new tab must leave this much of the window's width to the buttons.
const STRIP_ROOM: f32 = 90.0;
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
const LIST_BUTTON_SIZE: [f32; 2] = [8.0, 18.0];
/// The whisper-name list: 112 wide, 16 px rows, five at most (the names, then a blank one).
const LIST_ROW: f32 = 16.0;
const LIST_WIDTH: f32 = 112.0;
const LIST_ROWS: usize = 5;
/// "No Whisper List.", in red.
pub const NO_WHISPER_LIST: usize = 192;
/// Each tab keeps this many lines; a line is cut into pieces of at most this many bytes, then wrapped at this width.
pub const MOST_LINES: usize = 240;
const PIECE_BYTES: usize = 94;
pub const WRAP_WIDTH: f32 = 576.0;
/// The scrollbar at the log's right: its width and its arrows' height.
const SCROLLBAR: f32 = 13.0;
const ARROW: f32 = 10.0;
/// The lock, at the right of the top row. A press in the 9×9 just right of x 580 toggles it (y 1 to 13).
const LOCK_AT: [f32; 2] = [WIDTH - 20.0, 4.0];
const LOCK_HIT: [f32; 2] = [9.0, 9.0];
/// The invisible strip along the top that resizes the window, and the tallest it goes.
const RESIZE: ([f32; 2], [f32; 2]) = ([3.0, 16.0], [574.0, 6.0]);
const TOP_ROW: f32 = 20.0;
const MESSAGE_BOX: ([f32; 2], [f32; 2]) = ([110.0, 4.0], [460.0, 16.0]);
const SEND_TO_BUTTON: [f32; 2] = [572.0, 7.0];
const SIZE_BUTTON: [f32; 2] = [586.0, 7.0];
const BUTTON_SIZE: [f32; 2] = [11.0, 11.0];
/// The input bar buttons' tooltips: the whisper list, the receiving group, the size.
const TOOLTIPS: [usize; 3] = [3187, 3188, 3189];
const TAB_MESSAGES: [usize; 2] = [1291, 1292];
/// The line over the input bar.
const FRAME_LINE: [u8; 4] = [142, 142, 142, 0xFF];
/// The most the whisper name and the message take, in bytes as sent (CP949).
// ponytail: counted in CP949 bytes, as the server reads them, not in characters.
pub const NAME_MOST: usize = 24;
pub const MESSAGE_MOST: usize = 234;
/// Battle messages (category 16), the party members' (17) in party chat's pink.
pub const BATTLE_OWN: [u8; 3] = [255, 255, 0];
/// Party chat: the player's own lines and the others'.
pub const PARTY_OURS: [u8; 3] = [255, 200, 0];
pub const PARTY: [u8; 3] = [255, 200, 200];
/// Chat colours the kinds are read from: the player's own line, the guild's and the clan's, a skill's message.
const OWN: [u8; 3] = [0x00, 0xFF, 0x00];
const PUBLIC: [u8; 3] = [255, 255, 255];
const GUILD: [u8; 3] = [180, 255, 180];
const CLAN: [u8; 3] = [180, 220, 255];
const SKILL_MESSAGE: [u8; 3] = [0xCD, 0xCD, 0xFF];
/// Pure red: an error line the game shows in every tab.
const ERROR_RED: [u8; 3] = [0xFF, 0x00, 0x00];
/// The log's and the shown tab's black, the other tabs' darker one; the caret.
const DARK: [u8; 4] = [0, 0, 0, 0x66];
const TAB_DARK: [u8; 4] = [0, 0, 0, 0x99];
const DARKER: [u8; 4] = [0, 0, 0, 0xBF];
const TAB_BORDER: [u8; 4] = [0x95, 0x95, 0x95, 0xFF];

/// The window's height at step `step`: the input bar alone at 0, else the tabs, the log and the bar.
fn height(step: usize) -> f32 {
    if step == 0 { INPUT_HEIGHT } else { INPUT_HEIGHT + TAB_HEIGHT + LINE * (step + 3) as f32 }
}

/// F10's next step: 0 → 1, else up to the next of 1, 4, 7, 10, 13 (a dragged step snaps to it), past 15 back to 0.
fn next_step(step: usize) -> usize {
    match step {
        0 => 1,
        _ => Some(3 * ((step + 3) / 3) + 1).filter(|&next| next <= MOST_STEP).unwrap_or(0),
    }
}

/// The step a window dragged to `height` gets: 0 under 72, else one per 14 px, at most 15.
fn step_for(height: f32) -> usize {
    if height < 72.0 { 0 } else { (((height - 72.0) / 14.0) as usize + 1).min(MOST_STEP) }
}

/// The lines the log box shows at step `step`: those whose 14 rows fit in it.
fn rows(step: usize) -> usize {
    if step == 0 { 0 } else { ((height(step) - LOG_UNDER - LINE_TEXT[1]) / LINE) as usize }
}

/// Where line `row` of the log box puts its text, window-local; the black copy goes one down and right.
fn line_at(row: usize) -> [f32; 2] {
    [LOG_BOX[0] + LINE_TEXT[0], LOG_BOX[1] + LINE_TEXT[1] + LINE * row as f32]
}

/// The game's category of a chat line (its t, 0 to 25), for which tab shows it.
// ponytail: how a line's colour tells its kind (whispers say so themselves) is ours; the kinds the call sites name are the game's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// 0: system notices and errors.
    General,
    /// 1: others' chat, and ours (`Own`, shown with it).
    Public,
    Whisper,
    Party,
    Guild,
    /// 5: ally chat (nothing sends it yet).
    Ally,
    /// 6: "You got %s (%d)."
    ItemLog,
    Equipment,
    Status,
    PartyItemLog,
    PartyStatus,
    SkillFailure,
    PartyConfig,
    DamagedEquipment,
    /// 14: the woe broadcasts.
    Woe,
    /// 15: party booking's search messages (nothing sends these yet).
    PartySearch,
    /// 16 and 17: damage lines, ours and party members'.
    Battle,
    PartyBattle,
    Experience,
    Quest,
    Battlefield,
    Clan,
    DisplayCall,
    GuildExp,
    EquipProperties,
    /// 25: broadcasts, in every tab.
    Broadcast,
    /// Our own chat line: category 1, but its speaker isn't ours to pick (a double click does nothing).
    Own,
    /// What an agent on the control socket does, shown in its own tab (not in `ALL`: no settings, nothing saved).
    Mcp,
}

impl Kind {
    /// The game's categories in order, 0 to 25.
    pub const ALL: [Kind; 26] = [
        Kind::General,
        Kind::Public,
        Kind::Whisper,
        Kind::Party,
        Kind::Guild,
        Kind::Ally,
        Kind::ItemLog,
        Kind::Equipment,
        Kind::Status,
        Kind::PartyItemLog,
        Kind::PartyStatus,
        Kind::SkillFailure,
        Kind::PartyConfig,
        Kind::DamagedEquipment,
        Kind::Woe,
        Kind::PartySearch,
        Kind::Battle,
        Kind::PartyBattle,
        Kind::Experience,
        Kind::Quest,
        Kind::Battlefield,
        Kind::Clan,
        Kind::DisplayCall,
        Kind::GuildExp,
        Kind::EquipProperties,
        Kind::Broadcast,
    ];

    /// The kind a line in `color` is.
    pub fn of_color(color: [u8; 3]) -> Self {
        match color {
            PUBLIC => Kind::Public,
            OWN => Kind::Own,
            PARTY | PARTY_OURS => Kind::Party,
            GUILD => Kind::Guild,
            CLAN => Kind::Clan,
            SKILL_MESSAGE => Kind::SkillFailure,
            _ => Kind::General,
        }
    }

    /// The category a tab's options are about: our own chat is public chat.
    fn category(self) -> Kind {
        if self == Kind::Own { Kind::Public } else { self }
    }

    /// Its name in the tab settings window: the game's message, or ours where the data has none (4028-4031 are past the
    /// end of its table).
    pub fn label(self, messages: &[String]) -> String {
        let (id, fallback) = match self {
            Kind::General => (1291, "Chat"),
            Kind::Public | Kind::Own => (1277, "Public Chat"),
            Kind::Whisper => (1278, "Whisper"),
            Kind::Party => (1279, "Party Chat"),
            Kind::Guild => (1280, "Guild Chat"),
            Kind::Ally => (4031, "Ally Chat"),
            Kind::ItemLog => (1281, "Item Log"),
            Kind::Equipment => (1282, "Equipment On/Off"),
            Kind::Status => (1283, "Status Log"),
            Kind::PartyItemLog => (1284, "Party Member Item Log"),
            Kind::PartyStatus => (1285, "Party Member Status Log"),
            Kind::SkillFailure => (1286, "Skill Failure"),
            Kind::PartyConfig => (1287, "Party Configuration"),
            Kind::DamagedEquipment => (1288, "Damaged Equipment"),
            Kind::Woe => (1331, "WOE Information"),
            Kind::PartySearch => (1549, "Party member search"),
            Kind::Battle => (1618, "Battle message"),
            Kind::PartyBattle => (1619, "Party member's battle message"),
            Kind::Experience => (1620, "Experience message"),
            Kind::Quest => (1622, "Quest information"),
            Kind::Battlefield => (1623, "Battlefield message"),
            Kind::Clan => (2365, "Clan Chat"),
            Kind::DisplayCall => (2980, "Display Call messages"),
            Kind::GuildExp => (4028, "Guild EXP tribute"),
            Kind::EquipProperties => (4030, "Equipment properties"),
            Kind::Broadcast => (0, "Broadcasts"),
            Kind::Mcp => (0, "MCP"),
        };
        messages.get(id).filter(|text| id != 0 && !text.is_empty()).cloned().unwrap_or_else(|| fallback.to_owned())
    }

    fn name(self) -> &'static str {
        match self {
            Kind::General => "general",
            Kind::Public => "public",
            Kind::Own => "own",
            Kind::Whisper => "whisper",
            Kind::Party => "party",
            Kind::Guild => "guild",
            Kind::Ally => "ally",
            Kind::ItemLog => "item_log",
            Kind::Equipment => "equipment",
            Kind::Status => "status",
            Kind::PartyItemLog => "party_item_log",
            Kind::PartyStatus => "party_status",
            Kind::SkillFailure => "skill_failure",
            Kind::PartyConfig => "party_config",
            Kind::DamagedEquipment => "damaged_equipment",
            Kind::Woe => "woe",
            Kind::PartySearch => "party_search",
            Kind::Battle => "battle",
            Kind::PartyBattle => "party_battle",
            Kind::Experience => "experience",
            Kind::Quest => "quest",
            Kind::Battlefield => "battlefield",
            Kind::Clan => "clan",
            Kind::DisplayCall => "display_call",
            Kind::GuildExp => "guild_exp",
            Kind::EquipProperties => "equip_properties",
            Kind::Broadcast => "broadcast",
            Kind::Mcp => "mcp",
        }
    }
}

/// A tab's name: one of the game's messages (its first two tabs), or a text (new and renamed tabs, the MCP tab).
#[derive(Debug, Clone, PartialEq)]
pub enum Label {
    Message(usize),
    Text(String),
}

impl Label {
    pub fn text<'a>(&'a self, messages: &'a [String]) -> &'a str {
        match self {
            Label::Message(id) => messages.get(*id).map_or("", String::as_str),
            Label::Text(text) => text,
        }
    }

    /// As saved: "#<id>" for a message.
    fn saved(&self) -> String {
        match self {
            Label::Message(id) => format!("#{id}"),
            Label::Text(text) => text.clone(),
        }
    }

    fn from_saved(text: &str) -> Self {
        text.strip_prefix('#').and_then(|id| id.parse().ok()).map_or_else(|| Label::Text(text.to_owned()), Label::Message)
    }
}

/// A tab: its name, the kinds of lines it shows, and how far its log is scrolled back.
#[derive(Debug, Clone, PartialEq)]
pub struct Tab {
    pub label: Label,
    pub kinds: HashSet<Kind>,
    /// Its log, wrapped, oldest first (the last 240); pieces still to wrap; how many rows back it is scrolled.
    rows: Vec<Row>,
    fresh: Vec<Row>,
    scroll: usize,
}

impl Tab {
    fn new(label: Label, kinds: HashSet<Kind>) -> Self {
        Self { label, kinds, rows: Vec::new(), fresh: Vec::new(), scroll: 0 }
    }

    /// The MCP tab, for what an agent on the control socket does: not saved, no settings.
    fn is_mcp(&self) -> bool {
        self.kinds.contains(&Kind::Mcp)
    }
}

/// A row of a log: its text, colour, kind and speaker (public, party, guild and clan lines).
pub type Row = (String, [u8; 3], Kind, Option<String>);

/// Who said a line: for public, party, guild and clan lines, what comes before the first " :" if under 24 bytes.
pub fn speaker(text: &str, kind: Kind) -> Option<String> {
    if !matches!(kind, Kind::Public | Kind::Party | Kind::Guild | Kind::Clan) {
        return None;
    }
    let name = &text[..text.find(" :")?];
    (!name.is_empty() && name.len() < NAME_MOST).then(|| name.to_owned())
}

/// A line cut into pieces of at most 94 bytes, at a space or a newline where it can; a line with links stays whole.
pub fn pieces(text: &str) -> Vec<String> {
    if links::has_links(text) {
        return vec![text.to_owned()];
    }
    let mut pieces = Vec::new();
    let mut rest = text;
    while rest.len() > PIECE_BYTES {
        let mut cut = PIECE_BYTES;
        while !rest.is_char_boundary(cut) {
            cut -= 1;
        }
        let at = rest[..cut].rfind([' ', '\n']).filter(|&at| at > 0).unwrap_or(cut);
        pieces.push(rest[..at].to_owned());
        rest = rest[at..].trim_start_matches([' ', '\n']);
    }
    pieces.push(rest.trim_end_matches('\n').to_owned());
    pieces
}

/// A piece wrapped at `width` as `measure` measures it: at the last space that fits, else between characters.
pub fn wrap(text: &str, width: f32, measure: &dyn Fn(&str) -> f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut rest = text;
    while measure(rest) > width {
        // The longest start that fits, at least one character.
        let first = rest.chars().next().map_or(rest.len(), char::len_utf8);
        let fit =
            rest.char_indices().map(|(at, character)| at + character.len_utf8()).take_while(|&end| measure(&rest[..end]) <= width).last().unwrap_or(first);
        let at = rest[..fit].rfind(' ').filter(|&at| at > 0).unwrap_or(fit);
        lines.push(rest[..at].to_owned());
        rest = rest[at..].trim_start_matches(' ');
    }
    lines.push(rest.to_owned());
    lines
}

/// The tabs' kinds when nothing is saved: the game's tab 1 shows its first 14 options (categories 0 to 13); its tab 2 none,
/// here the battle messages.
// ponytail: the game's own battle tab starts with every option off; ours shows the damage lines, as a player's saved tab does.
pub fn default_kinds() -> [HashSet<Kind>; 2] {
    [Kind::ALL[..14].iter().copied().collect(), [Kind::Battle, Kind::PartyBattle].into_iter().collect()]
}

/// The first line of a saved tabs file: "v2" holds the game's two tabs' kinds (25 categories; the first version's
/// weren't), "v3" the lock and every tab's name and kinds.
const KINDS_HEADER: &str = "v2";
const TABS_HEADER: &str = "v3";

fn kinds_of(names: &str) -> HashSet<Kind> {
    names.split_whitespace().filter_map(|name| Kind::ALL.into_iter().find(|kind| kind.name() == name)).collect()
}

fn kinds_names(kinds: &HashSet<Kind>) -> String {
    Kind::ALL.into_iter().filter(|kind| kinds.contains(kind)).map(Kind::name).collect::<Vec<_>>().join(" ")
}

/// The tabs as saved and whether the window was locked: "v3", "locked 0|1", then a line a tab, "tab", its name
/// and its kinds between tab characters. A "v2" file is the game's two tabs' kinds, unlocked.
pub fn tabs_from(text: &str) -> Option<(Vec<Tab>, bool)> {
    let mut lines = text.lines();
    match lines.next()? {
        KINDS_HEADER => {
            let mut next = || lines.next().map(kinds_of);
            let tabs = TAB_MESSAGES.map(|id| next().map(|kinds| Tab::new(Label::Message(id), kinds)));
            Some((tabs.into_iter().collect::<Option<_>>()?, false))
        }
        TABS_HEADER => {
            let locked = lines.next()?.strip_prefix("locked ")? == "1";
            let tabs: Vec<Tab> = lines
                .filter_map(|line| {
                    let mut fields = line.split('\t');
                    (fields.next()? == "tab").then_some(())?;
                    let label = Label::from_saved(fields.next()?);
                    Some(Tab::new(label, kinds_of(fields.next().unwrap_or(""))))
                })
                .collect();
            (!tabs.is_empty()).then_some((tabs, locked))
        }
        _ => None,
    }
}

/// The tabs as `tabs_from` reads them; the MCP tab isn't kept.
pub fn tabs_text(tabs: &[Tab], locked: bool) -> String {
    let lines = tabs.iter().filter(|tab| !tab.is_mcp()).map(|tab| format!("tab\t{}\t{}", tab.label.saved(), kinds_names(&tab.kinds)));
    [TABS_HEADER.to_owned(), format!("locked {}", u8::from(locked))].into_iter().chain(lines).collect::<Vec<_>>().join("\n")
}

/// The game's two tabs, with their kinds when nothing is saved.
fn default_tabs() -> Vec<Tab> {
    TAB_MESSAGES.into_iter().zip(default_kinds()).map(|(id, kinds)| Tab::new(Label::Message(id), kinds)).collect()
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

/// Who a unit in a hit is to the player.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Role {
    Me,
    Party,
    Other,
}

/// The battle message of a hit (categories 16 and 17), if the player or a party member is in it: damage we take
/// (" [name] inflicts 'N' damage points on you."), damage we deal ("[name] receives 'N' damage points."), and a member
/// attacked or attacking.
// ponytail: the client picks the pair by the map's type (a table it reads); here both pairs show on every map.
pub fn battle_line(messages: &[String], source: (Role, &str), target: (Role, &str), amount: i32) -> Option<(String, [u8; 3], Kind)> {
    if amount <= 0 {
        return None;
    }
    let template = |id: usize| messages.get(id).map_or("", String::as_str);
    Some(match (source.0, target.0) {
        (_, Role::Me) => (crate::messages::fill(template(crate::BATTLE_ON_YOU), &[&source.1, &amount]), BATTLE_OWN, Kind::Battle),
        (Role::Me, _) => (crate::messages::fill(template(crate::BATTLE_RECEIVES), &[&target.1, &amount]), BATTLE_OWN, Kind::Battle),
        (_, Role::Party) => (crate::messages::fill(template(crate::BATTLE_ATTACKED_BY), &[&target.1, &source.1, &amount]), PARTY, Kind::PartyBattle),
        (Role::Party, _) => (crate::messages::fill(template(crate::BATTLE_ATTACKS), &[&source.1, &target.1, &amount]), PARTY, Kind::PartyBattle),
        _ => return None,
    })
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
    /// Remove tab, add tab, tear-off and tab settings: each normal and pressed.
    remove_tab: [Texture; 2],
    add_tab: [Texture; 2],
    tear: [Texture; 2],
    tab_settings: [Texture; 2],
    /// The lock at the strip's right: locked and not.
    lock: [Texture; 2],
    dark: Texture,
    tab_dark: Texture,
    darker: Texture,
    white: Texture,
    tab_border: Texture,
    frame_line: Texture,
    /// Item and navigation links' underlines.
    link: Texture,
    navi_link: Texture,
    /// The scrollbar: its arrows (up, down), black track and thumb; the whisper list's picked row.
    arrows: [Texture; 2],
    track: Texture,
    thumb: Texture,
    picked: Texture,
    /// The selection behind typed text.
    selection: Texture,
    /// Battle mode's icon: the line open, closed.
    mode_icon: [Texture; 2],
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let basic = format!("{UI}basic_interface\\");
        let image = |name: &str| gpu.load_texture(grf, &format!("{basic}{name}.bmp"), false);
        Self {
            input: image("dialog_bg"),
            list: image("dialog_btn0"),
            base: image("sys_base_off"),
            // ponytail: _a is the normal look and _b the pressed one.
            remove_tab: [image("wnd_mini_a"), image("wnd_mini_b")],
            add_tab: [image("battle_option_a"), image("battle_option_b")],
            tear: [image("stickoff_a"), image("stickoff_b")],
            tab_settings: [image("battle_option2_a"), image("battle_option2_b")],
            lock: [image("lock_dragwnd"), image("unlock_dragwnd")],
            dark: gpu.solid(DARK),
            tab_dark: gpu.solid(TAB_DARK),
            darker: gpu.solid(DARKER),
            white: gpu.solid([255; 4]),
            tab_border: gpu.solid(TAB_BORDER),
            frame_line: gpu.solid(FRAME_LINE),
            link: gpu.solid([crate::links::COLOR[0], crate::links::COLOR[1], crate::links::COLOR[2], 0xFF]),
            navi_link: gpu.solid([crate::links::NAVI_COLOR[0], crate::links::NAVI_COLOR[1], crate::links::NAVI_COLOR[2], 0xFF]),
            arrows: [image("dialscr_up"), image("dialscr_down")],
            track: gpu.solid([0, 0, 0, 0xFF]),
            // ponytail: dialscr_bar stretched as the thumb.
            thumb: image("dialscr_bar"),
            picked: gpu.solid([0x73, 0x9C, 0xEF, 0xFF]),
            // ponytail: the selection's colour is a choice.
            selection: gpu.solid([0xA8, 0xC8, 0xF0, 0xFF]),
            mode_icon: [image("chatmode_on"), image("chatmode_off")],
        }
    }
}

/// What a click asks the game to show: a red chat line, or a notice box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Said {
    Line(usize),
    Notice(usize),
}

/// What a click on the chat window lands on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Hit {
    Tab(usize),
    /// Remove the tab shown (when there's another).
    RemoveTab,
    /// Add a tab, "NewTab_<n>", showing nothing until its settings say.
    AddTab,
    /// Tear the tab off the bottom of the screen (back to the corner, here).
    Stick,
    /// Tab settings.
    Options,
    Lock,
    /// The bare window outside the top row (the margins beside the log, the input bar's gaps between its boxes, all
    /// of the bar while battle mode's line is closed): it focuses the message box and drags the window, unlocked.
    Strip,
    /// The rest of the top row: nothing.
    TopRow,
    /// The strip along the top edge that resizes the window.
    Resize,
    /// Battle mode's icon: opens or closes the line.
    ModeIcon,
    /// The log's scrollbar: its arrows, and the track above or below the thumb.
    ScrollUp,
    ScrollDown,
    Track(bool),
    List,
    SendTo,
    Size,
    Name,
    Message,
    Log,
}

/// One button of the top row: where it sits, how big it is, and the driver's name for it.
struct RowButton {
    hit: Hit,
    x: f32,
    size: [f32; 2],
    name: &'static str,
}

const fn row_button(hit: Hit, x: f32, width: f32, height: f32, name: &'static str) -> RowButton {
    RowButton { hit, x, size: [width, height], name }
}

/// The driver's names for the tabs, left to right (ten and the MCP tab at most).
const TAB_NAMES: [&str; MOST_TABS + 1] =
    ["chat_tab_0", "chat_tab_1", "chat_tab_2", "chat_tab_3", "chat_tab_4", "chat_tab_5", "chat_tab_6", "chat_tab_7", "chat_tab_8", "chat_tab_9", "chat_tab_10"];

/// Unlocked, right to left from the lock: remove, add, tear-off, settings, the chat-mode icon.
const OPEN_ROW: [RowButton; 5] = [
    row_button(Hit::RemoveTab, 570.0, 8.0, 8.0, "chat_remove_tab"),
    row_button(Hit::AddTab, 560.0, 8.0, 8.0, "chat_add_tab"),
    row_button(Hit::Stick, 544.0, 14.0, 9.0, "chat_stick"),
    row_button(Hit::Options, 533.0, 9.0, 9.0, "chat_options"),
    row_button(Hit::ModeIcon, 517.0, 14.0, 9.0, "chat_mode"),
];
/// Locked: only tab settings, then the icon.
const LOCKED_ROW: [RowButton; 2] = [row_button(Hit::Options, 569.0, 9.0, 9.0, "chat_options"), row_button(Hit::ModeIcon, 553.0, 14.0, 9.0, "chat_mode")];

fn top_row(locked: bool) -> &'static [RowButton] {
    if locked { &LOCKED_ROW } else { &OPEN_ROW }
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
    /// The carets of the message box and the name box, and the lines sent for Up / Down.
    carets: [Caret; 2],
    history: History,
    /// The whisper-name list, in the order names came, and whether it is dropped down.
    pub recent: Vec<String>,
    list_open: bool,
    /// The size step, 0 to 15.
    size: usize,
    pub tabs: Vec<Tab>,
    /// The tab shown.
    pub tab: usize,
    /// Each tab's width, from its label as last measured.
    tab_widths: Vec<f32>,
    /// Where the window's bottom left corner is, once moved (else the screen's bottom left).
    corner: Option<[f32; 2]>,
    /// The pointer's offset from the corner while the window is held (it moves only unlocked), and when it was
    /// grabbed or let go (and how faded it was then): it's half see-through while held.
    drag: Option<[f32; 2]>,
    held: Option<std::time::Instant>,
    let_go: Option<(std::time::Instant, f32)>,
    /// While the top edge is dragged: the pointer's height and the window's when it started.
    resize: Option<(f32, f32)>,
    /// Clicks on the log's rows, for double clicks.
    clicks: crate::window::Clicks,
    /// The lock: no dragging.
    pub locked: bool,
    pub send_to: SendTo,
    /// The tab settings window, while open.
    pub settings: Option<Settings>,
    /// Alt+F10 hides the window. Typing still goes to the line.
    pub hidden: bool,
    /// Battle mode's icon, while battle mode is on: whether the line is open.
    pub mode_icon: Option<bool>,
    /// Esc closed the line with text in it: the next key no shortcut takes empties the box.
    stale: bool,
    /// What was last kept of the window's place.
    saved: String,
}

impl Default for Chat {
    fn default() -> Self {
        let saved = settings_path().and_then(|path| std::fs::read_to_string(path).ok()).and_then(|text| tabs_from(&text));
        let (tabs, locked) = saved.unwrap_or_else(|| (default_tabs(), false));
        let mut chat = Self::with_tabs(tabs);
        chat.locked = locked;
        let kept = crate::settings_path("chat_window").filter(|_| crate::keeps_windows()).and_then(|path| std::fs::read_to_string(path).ok());
        if let Some((corner, size, hidden)) = kept.as_deref().and_then(|text| Self::from_saved(text, screen())) {
            (chat.corner, chat.size, chat.hidden) = (corner, size, hidden);
        }
        chat.saved = chat.saved_text();
        if crate::mcp::shared() {
            chat.add_mcp_tab();
        }
        chat
    }
}

impl Chat {
    /// Whether the MCP tab is the one shown: what is typed then goes to the agent.
    pub fn on_mcp_tab(&self) -> bool {
        self.tabs[self.tab].is_mcp()
    }

    /// A tab after the others for what an agent on the control socket does (`Kind::Mcp` lines).
    pub fn add_mcp_tab(&mut self) {
        if !self.tabs.iter().any(Tab::is_mcp) {
            self.tabs.push(Tab::new(Label::Text("MCP".into()), HashSet::from([Kind::Mcp])));
        }
    }

    pub fn with_tabs(tabs: Vec<Tab>) -> Self {
        Self {
            lines: Vec::new(),
            input: String::new(),
            preedit: String::new(),
            whisper: String::new(),
            naming: false,
            carets: [Caret::default(); 2],
            history: History::default(),
            recent: Vec::new(),
            list_open: false,
            size: DEFAULT_STEP,
            tabs,
            tab: 0,
            tab_widths: Vec::new(),
            corner: None,
            drag: None,
            held: None,
            let_go: None,
            resize: None,
            clicks: crate::window::Clicks::default(),
            locked: false,
            send_to: SendTo::default(),
            settings: None,
            hidden: false,
            mode_icon: None,
            stale: false,
            saved: String::new(),
        }
    }

    /// The window as kept between logins (the game's M_CHATWNDX/Y, HEIGHT, STATUS, SHOW): its bottom left corner once
    /// moved ("-" where it never was), its F10 step and whether Alt+F10 hid it.
    pub fn saved_text(&self) -> String {
        let place = self.corner.map_or_else(|| "- -".to_owned(), |[x, y]| format!("{} {}", x as i32, y as i32));
        format!("{place} {} {}", self.size, u8::from(self.hidden))
    }

    /// A kept window: its corner (None, the screen's bottom left, when it never moved or wouldn't be wholly on
    /// `screen` at that step), its size step and whether it was hidden.
    pub fn from_saved(text: &str, screen: [f32; 2]) -> Option<(Option<[f32; 2]>, usize, bool)> {
        let fields: Vec<&str> = text.split_whitespace().collect();
        let [x, y, size, hidden] = fields[..] else { return None };
        let size: usize = size.parse().ok().filter(|&size| size <= MOST_STEP)?;
        let corner = match (x.parse::<f32>(), y.parse::<f32>()) {
            (Ok(x), Ok(y)) => Some([x, y]).filter(|&[x, y]| x >= 0.0 && x + WIDTH <= screen[0] && y - height(size) >= 0.0 && y <= screen[1]),
            _ => None,
        };
        Some((corner, size, hidden == "1"))
    }

    /// Keeps the window's place, size and hiding when they changed.
    pub fn save_if_changed(&mut self) {
        let text = self.saved_text();
        if text != self.saved
            && crate::keeps_windows()
            && let Some(path) = crate::settings_path("chat_window")
        {
            let _ = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(&path, &text));
            self.saved = text;
        }
    }

    fn step(&self) -> usize {
        self.size
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

    /// Tab `tab`'s width: its label's and 10 (measured in `layout`).
    fn tab_width(&self, tab: usize) -> f32 {
        self.tab_widths.get(tab).copied().unwrap_or(UNMEASURED_TAB)
    }

    /// Tab `tab`'s top left corner, window-local: the tabs sit side by side from x 3.
    fn tab_corner(&self, tab: usize) -> [f32; 2] {
        [STRIP_X + (0..tab).map(|tab| self.tab_width(tab)).sum::<f32>(), 0.0]
    }

    /// The tabs' average width: their backgrounds are drawn this far apart.
    fn average_tab(&self) -> f32 {
        (0..self.tabs.len()).map(|tab| self.tab_width(tab)).sum::<f32>() / self.tabs.len() as f32
    }

    /// How see-through to draw the window now: half while it's held.
    pub fn faded(&self) -> f32 {
        crate::window::hold_fade(self.held, self.let_go, crate::control::now())
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

    /// F10: the next size, the same cycle as the size button; the bottom edge stays.
    pub fn cycle_size(&mut self) {
        self.set_step(next_step(self.size));
    }

    fn set_step(&mut self, step: usize) {
        self.size = step;
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
            // Battle mode's closed line takes the bar's boxes and buttons away: all of it is the bare window then.
            let line_open = self.mode_icon != Some(false);
            return Some(if !line_open {
                Hit::Strip
            } else if in_bar(SIZE_BUTTON, BUTTON_SIZE) {
                Hit::Size
            } else if in_bar(SEND_TO_BUTTON, BUTTON_SIZE) {
                Hit::SendTo
            } else if in_bar(LIST_BUTTON, LIST_BUTTON_SIZE) {
                Hit::List
            } else if in_bar(NAME_BOX.0, NAME_BOX.1) {
                Hit::Name
            } else if in_bar(MESSAGE_BOX.0, MESSAGE_BOX.1) {
                Hit::Message
            } else {
                Hit::Strip
            });
        }
        if self.step() > 0 && on(RESIZE.0, RESIZE.1) {
            return Some(Hit::Resize);
        }
        if self.step() > 0 && on(self.scrollbar_corner(), [SCROLLBAR, self.log_height()]) {
            let y = local[1] - LOG_BOX[1];
            return Some(if y < ARROW {
                Hit::ScrollUp
            } else if y >= self.log_height() - ARROW {
                Hit::ScrollDown
            } else {
                Hit::Track(y < self.thumb().0)
            });
        }
        if local[1] < TAB_HEIGHT {
            if let Some(tab) = (0..self.tabs.len()).find(|&tab| on(self.tab_corner(tab), [self.tab_width(tab), TAB_HEIGHT])) {
                return Some(Hit::Tab(tab));
            }
            let on_row = (1.0..=13.0).contains(&local[1]);
            if on_row && local[0] > LOCK_AT[0] && local[0] < LOCK_AT[0] + LOCK_HIT[0] {
                return Some(Hit::Lock);
            }
            for button in top_row(self.locked) {
                let hit = if button.hit == Hit::ModeIcon {
                    self.mode_icon.is_some() && on_row && (button.x..button.x + button.size[0]).contains(&local[0])
                } else {
                    on([button.x, 4.0], button.size)
                };
                if hit {
                    return Some(button.hit);
                }
            }
            return Some(Hit::TopRow);
        }
        Some(if on(LOG_BOX, [LOG_WIDTH, self.log_height()]) {
            Hit::Log
        } else if local[1] <= TOP_ROW {
            Hit::TopRow
        } else {
            Hit::Strip
        })
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
            boxes.extend(
                (0..self.tabs.len()).map(|tab| (TAB_NAMES[tab.min(TAB_NAMES.len() - 1)], area(self.tab_corner(tab), [self.tab_width(tab), TAB_HEIGHT]))),
            );
            boxes.extend([
                ("chat_lock", area(LOCK_AT, LOCK_HIT)),
                ("chat_strip", area([0.0, TOP_ROW + 4.0], [LOG_BOX[0] - 1.0, 12.0])),
                ("chat_resize", area([RESIZE.0[0] + 300.0, RESIZE.0[1]], [100.0, RESIZE.1[1]])),
                ("chat_scroll_up", area(self.scrollbar_corner(), [SCROLLBAR, ARROW])),
                ("chat_scroll_down", area([self.scrollbar_corner()[0], LOG_BOX[1] + self.log_height() - ARROW], [SCROLLBAR, ARROW])),
            ]);
            // The mode icon only while it's drawn (`hit` takes it only then).
            let shown = |button: &&RowButton| button.hit != Hit::ModeIcon || self.mode_icon.is_some();
            boxes.extend(top_row(self.locked).iter().filter(shown).map(|button| (button.name, area([button.x, 4.0], button.size))));
        }
        // The input bar's gap left of the name box: the bare window, which drags it.
        boxes.push(("chat_grip", area([0.0, bar + NAME_BOX.0[1]], [NAME_BOX.0[0], NAME_BOX.1[1]])));
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

    /// A press: on the top edge's strip it starts resizing (locked or not); on the bare window outside the top row
    /// it focuses the message box and grabs the window, which goes half see-through and, unlocked, follows the pointer.
    pub fn press(&mut self, cursor: [f32; 2]) -> bool {
        match self.hit(cursor) {
            Some(Hit::Resize) => {
                self.resize = Some((cursor[1], self.height()));
                true
            }
            Some(Hit::Strip) => {
                let corner = self.corner();
                self.drag = Some([cursor[0] - corner[0], cursor[1] - corner[1]]);
                self.held = Some(crate::control::now());
                self.let_go = None;
                self.naming = false;
                true
            }
            _ => false,
        }
    }

    /// The pointer moved: a dragged window follows it (unlocked), its edges snapping to the screen's and kept where it
    /// can be grabbed again; a resized one takes the step its new height gives (the bottom edge stays).
    pub fn drag_to(&mut self, cursor: [f32; 2]) {
        if let Some((start, height)) = self.resize {
            let step = step_for(height + start - cursor[1]);
            if step != self.size {
                self.set_step(step);
            }
        }
        if let Some(offset) = self.drag.filter(|_| !self.locked) {
            let height = self.height();
            let [x, top] = crate::window::dragged_to([cursor[0] - offset[0], cursor[1] - offset[1] - height], [WIDTH, height]);
            self.corner = Some([x, top + height]);
        }
    }

    /// The drag ends; whether there was one.
    pub fn drop_drag(&mut self) -> bool {
        if let Some(held) = self.held.take() {
            let now = crate::control::now();
            self.let_go = Some((now, crate::window::hold_fade(Some(held), None, now)));
        }
        self.resize.take().is_some() | self.drag.take().is_some()
    }

    /// A taller step keeps the bottom edge, but not past the screen's top: the tabs stay where they can be reached.
    fn keep_on_screen(&mut self) {
        let height = self.height();
        if let Some([_, bottom]) = &mut self.corner {
            *bottom = bottom.max(height);
        }
    }

    /// Adds a line of the kind its colour tells.
    pub fn push(&mut self, text: String, color: [u8; 3]) {
        self.push_kind(text, color, Kind::of_color(color));
    }

    /// A line for the tabs that show its kind, cut into pieces; `layout` wraps them.
    pub fn push_kind(&mut self, text: String, color: [u8; 3], kind: Kind) {
        let speaker = speaker(&text, kind);
        // Broadcasts and the pure red errors are in every tab.
        let forced = kind == Kind::Broadcast || (kind == Kind::General && color == ERROR_RED);
        for tab in self.tabs.iter_mut().filter(|tab| forced || tab.kinds.contains(&kind.category())) {
            tab.fresh.extend(pieces(&text).into_iter().map(|piece| (piece, color, kind, speaker.clone())));
        }
        self.lines.push((text, color, kind));
    }

    /// Wraps the new pieces at 576 px and keeps each tab's last 240 lines. A tab at the bottom follows the new lines;
    /// one scrolled back stays on what it shows.
    pub fn layout(&mut self, measure: &dyn Fn(&str) -> f32) {
        let shown = rows(self.step());
        for tab in &mut self.tabs {
            for (piece, color, kind, speaker) in std::mem::take(&mut tab.fresh) {
                let wrapped = if links::has_links(&piece) { vec![piece] } else { wrap(&piece, WRAP_WIDTH, measure) };
                if tab.scroll > 0 {
                    tab.scroll += wrapped.len();
                }
                tab.rows.extend(wrapped.into_iter().map(|row| (row, color, kind, speaker.clone())));
            }
            let over = tab.rows.len().saturating_sub(MOST_LINES);
            tab.rows.drain(..over);
            tab.scroll = tab.scroll.min(tab.rows.len().saturating_sub(shown));
        }
    }

    /// Measures the tabs' labels: each tab is its label's width and 10, the label cut to 600 / tabs less 10.
    pub fn measure_tabs(&mut self, messages: &[String], measure: &dyn Fn(&str) -> f32) {
        let most = self.label_room();
        self.tab_widths = self.tabs.iter().map(|tab| measure(tab.label.text(messages)).min(most) + TAB_PADDING).collect();
    }

    /// The widest a tab's label is drawn.
    fn label_room(&self) -> f32 {
        WIDTH / self.tabs.len() as f32 - TAB_PADDING
    }

    /// The rows tab `tab` shows, oldest first (pieces not yet wrapped at the end).
    pub fn tab_lines(&self, tab: usize) -> Vec<&Row> {
        self.tabs[tab].rows.iter().chain(&self.tabs[tab].fresh).collect()
    }

    /// Scrolls the shown tab's log by `rows` lines (negative: back in time).
    pub fn scroll_by(&mut self, rows: i32) {
        let most = self.tabs[self.tab].rows.len().saturating_sub(self::rows(self.step()));
        let tab = &mut self.tabs[self.tab];
        tab.scroll = (tab.scroll as i32 - rows).clamp(0, most as i32) as usize;
    }

    /// How far the shown tab is scrolled back, in rows.
    pub fn scroll(&self) -> usize {
        self.tabs[self.tab].scroll
    }

    /// PageUp / PageDown: the shown rows less one; with Ctrl, to the top or the bottom.
    pub fn page(&mut self, down: bool, to_end: bool) {
        let rows = if to_end { MOST_LINES as i32 } else { (self::rows(self.step()) as i32 - 1).max(1) };
        self.scroll_by(if down { rows } else { -rows });
    }

    /// The scrollbar's corner, window-local, and the log box's height.
    fn scrollbar_corner(&self) -> [f32; 2] {
        [LOG_BOX[0] + LOG_WIDTH - SCROLLBAR, LOG_BOX[1]]
    }

    fn log_height(&self) -> f32 {
        self.height() - LOG_UNDER
    }

    /// The thumb between the arrows: its top and height, window-local.
    // ponytail: the thumb's least height is a choice.
    fn thumb(&self) -> (f32, f32) {
        let track = self.log_height() - 2.0 * ARROW;
        let total = self.tabs[self.tab].rows.len().max(1);
        let shown = rows(self.step()).min(total);
        let height = (track * shown as f32 / total as f32).max(8.0).min(track);
        let first = total - shown - self.tabs[self.tab].scroll.min(total - shown);
        let room = total - shown;
        let top = if room == 0 { track - height } else { (track - height) * first as f32 / room as f32 };
        (LOG_BOX[1] + ARROW + top, height)
    }

    /// The speaker of the log row under `cursor`, if it has one.
    pub fn speaker_at(&self, cursor: [f32; 2]) -> Option<&str> {
        if self.hit(cursor) != Some(Hit::Log) {
            return None;
        }
        let local_y = cursor[1] - self.at([0.0, 0.0])[1] - LOG_BOX[1] - LINE_TEXT[1];
        let row = (local_y / LINE).floor();
        let (start, end) = self.shown_range();
        let index = start + usize::try_from(row as i64).ok()?;
        (index < end).then(|| self.tabs[self.tab].rows[index].3.as_deref()).flatten()
    }

    /// The shown tab's rows on screen: from the scroll back, filling from the top.
    fn shown_range(&self) -> (usize, usize) {
        let rows = &self.tabs[self.tab].rows;
        let end = rows.len().saturating_sub(self.tabs[self.tab].scroll);
        (end.saturating_sub(self::rows(self.step())), end)
    }

    /// A click on the log: the speaker of the row when it completes a double click there.
    pub fn log_click(&mut self, cursor: [f32; 2]) -> Option<String> {
        if self.hit(cursor) != Some(Hit::Log) {
            return None;
        }
        let row = ((cursor[1] - self.at([0.0, 0.0])[1] - LOG_BOX[1] - LINE_TEXT[1]) / LINE).floor().max(0.0) as u16;
        if !self.clicks.double(row) {
            return None;
        }
        self.speaker_at(cursor).map(str::to_owned)
    }

    /// Adds a name to the end of the whisper-name list, if it isn't there.
    pub fn note_recent(&mut self, name: &str) {
        let name = name.trim();
        if !name.is_empty() && !self.recent.iter().any(|known| known.eq_ignore_ascii_case(name)) {
            self.recent.push(name.to_owned());
        }
    }

    /// A double click on a speaker's line: the name joins the list and the name box, the caret goes to the message.
    pub fn whisper_to(&mut self, name: &str) {
        self.note_recent(name);
        self.whisper = name.to_owned();
        self.naming = false;
    }

    /// A whisper came in: the window shows, and so does the log if it was down to the input bar.
    pub fn whispered_by(&mut self, name: &str) {
        self.note_recent(name);
        self.hidden = false;
        if self.size == 0 {
            self.set_step(1);
        }
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
            for (row, name) in self.list_entries().into_iter().enumerate() {
                boxes.push((if name.is_empty() { "recent:".to_owned() } else { format!("recent:{name}") }, self.row_box(row)));
            }
        }
        boxes
    }

    fn list_button_box(&self) -> [f32; 4] {
        let origin = self.at([LIST_BUTTON[0], self.height() - INPUT_HEIGHT + LIST_BUTTON[1]]);
        [origin[0], origin[1], LIST_BUTTON_SIZE[0], LIST_BUTTON_SIZE[1]]
    }

    /// The list's rows: the names then a blank one, the last five when there are more.
    // ponytail: the client's list scrolls past five; here it shows the newest.
    fn list_entries(&self) -> Vec<&str> {
        let entries: Vec<&str> = self.recent.iter().map(String::as_str).chain([""]).collect();
        entries[entries.len().saturating_sub(LIST_ROWS)..].to_vec()
    }

    /// A row of the dropped list: its bottom one pixel over the input bar's top, at x 1.
    fn row_box(&self, row: usize) -> [f32; 4] {
        let height = self.list_entries().len() as f32 * LIST_ROW;
        let [x, y] = self.at([1.0, self.height() - INPUT_HEIGHT - height + 1.0]);
        [x, y + row as f32 * LIST_ROW, LIST_WIDTH, LIST_ROW]
    }

    fn list_row(&self, cursor: [f32; 2]) -> Option<usize> {
        if !self.list_open {
            return None;
        }
        (0..self.list_entries().len()).find(|&row| contains_box(self.row_box(row), cursor))
    }

    /// A click on the window: a tab shows its log; the buttons do what they say; the boxes of the input bar take the
    /// caret. The list button drops the whisper names (an empty list says so); a row fills the name box, the blank
    /// one empties it. What the game should show for it, if anything.
    pub fn click(&mut self, cursor: [f32; 2]) -> Option<Said> {
        // Rows sit above the button and can fall outside the window, so they come before `hit`.
        if let Some(row) = self.list_row(cursor) {
            self.whisper = self.list_entries()[row].to_owned();
            self.naming = false;
            self.list_open = false;
            return None;
        }
        if self.hit(cursor) == Some(Hit::List) {
            if self.recent.is_empty() {
                return Some(Said::Line(NO_WHISPER_LIST));
            }
            self.list_open = !self.list_open;
            return None;
        }
        self.list_open = false;
        match self.hit(cursor) {
            Some(Hit::ScrollUp) => self.scroll_by(-1),
            Some(Hit::ScrollDown) => self.scroll_by(1),
            Some(Hit::Track(above)) => self.page(!above, false),
            Some(Hit::Tab(tab)) => self.select_tab(tab),
            // Locked, these buttons aren't there.
            Some(Hit::AddTab) if !self.locked => return self.add_tab().err().map(Said::Notice),
            Some(Hit::RemoveTab) if !self.locked => self.remove_tab(),
            // Tear-off puts the window back at the screen's bottom left.
            Some(Hit::Stick) if !self.locked => self.corner = None,
            // The same button again closes it; the MCP tab has none.
            Some(Hit::Options) => {
                self.settings = (self.settings.is_none() && !self.on_mcp_tab()).then(|| Settings::new(self.tab));
            }
            Some(Hit::Lock) => {
                self.locked = !self.locked;
                self.save_tabs();
            }
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
            Some(Hit::Message | Hit::Strip) => self.naming = false,
            _ => {}
        }
        None
    }

    /// Shows tab `tab`; the tab settings window, if open, switches to it.
    fn select_tab(&mut self, tab: usize) {
        self.tab = tab;
        if self.settings.is_some() {
            self.settings = (!self.on_mcp_tab()).then(|| Settings::new(tab));
        }
    }

    /// A tab "NewTab_<n>" after the others (n counts it), showing nothing until its settings say. Refused with the
    /// message to show when there are ten, or when the strip has no room for it.
    // ponytail: a tab the strip has no room for is refused; the game makes it a torn-off window instead.
    pub fn add_tab(&mut self) -> Result<(), usize> {
        let fits = (self.tabs.len() + 1) as f32 * self.average_tab() <= WIDTH - STRIP_ROOM;
        // The agent's tab isn't the game's: it doesn't count.
        let tabs = self.tabs.iter().filter(|tab| !tab.is_mcp()).count();
        if tabs >= MOST_TABS || !fits {
            return Err(CANNOT_ADD_MORE);
        }
        let label = Label::Text(format!("{NEW_TAB}{}", tabs + 1));
        // After the agent's tab it would sit past the game's; before it, with the others.
        let at = self.tabs.iter().position(Tab::is_mcp).unwrap_or(self.tabs.len());
        self.tabs.insert(at, Tab::new(label, HashSet::new()));
        if self.tab >= at && self.tabs.len() > at + 1 {
            self.tab += 1;
        }
        self.save_tabs();
        Ok(())
    }

    /// Removes the tab shown, if it isn't the last; the one before it shows next.
    pub fn remove_tab(&mut self) {
        if self.tabs.len() <= 1 {
            return;
        }
        self.tabs.remove(self.tab);
        self.tab_widths.clear();
        self.tab = self.tab.saturating_sub(1);
        self.settings = None;
        self.save_tabs();
    }

    /// The control socket's `chat_focus`: which box has the caret, "name" or "message".
    pub fn focus(&self) -> &'static str {
        if self.naming { "name" } else { "message" }
    }

    /// The box with the caret, its caret, and the most it takes.
    fn focused(&mut self) -> (&mut String, &mut Caret, usize) {
        if self.naming { (&mut self.whisper, &mut self.carets[1], NAME_MOST) } else { (&mut self.input, &mut self.carets[0], MESSAGE_MOST) }
    }

    /// Types `text` at the caret of the box that has it (over the selection), up to that box's most.
    pub fn type_text(&mut self, text: &str) {
        let (typing, caret, most) = self.focused();
        caret.insert(typing, text, &|candidate: &str| capped(candidate, most).len() == candidate.len());
    }

    /// Types `text` whole at the caret if the box still takes it all (an item link); else nothing.
    pub fn type_whole(&mut self, text: &str) {
        let (typing, caret, most) = self.focused();
        let mut trial = typing.clone();
        let mut trial_caret = *caret;
        trial_caret.insert(&mut trial, text, &|_: &str| true);
        if capped(&trial, most).len() == trial.len() {
            (*typing, *caret) = (trial, trial_caret);
        }
    }

    /// Backspace and Delete in the box with the caret.
    pub fn backspace(&mut self) {
        let (typing, caret, _) = self.focused();
        caret.backspace(typing);
    }

    pub fn delete(&mut self) {
        let (typing, caret, _) = self.focused();
        caret.delete(typing);
    }

    /// Left, Right, Home, End; with Shift they select.
    pub fn go(&mut self, to: Move, select: bool) {
        let (typing, caret, _) = self.focused();
        caret.go(typing, to, select);
    }

    pub fn select_all(&mut self) {
        let (typing, caret, _) = self.focused();
        caret.select_all(typing);
    }

    /// The selected text, for the clipboard.
    pub fn selected(&self) -> String {
        let (typing, caret) = if self.naming { (&self.whisper, &self.carets[1]) } else { (&self.input, &self.carets[0]) };
        caret.selection(typing).map(|range| typing[range].to_owned()).unwrap_or_default()
    }

    /// Takes the selection out (Ctrl+X); returns it.
    pub fn cut(&mut self) -> String {
        let (typing, caret, _) = self.focused();
        caret.take_selection(typing)
    }

    /// Where the caret and the selection are in the box with the caret, in bytes.
    pub fn caret(&self) -> (usize, Option<std::ops::Range<usize>>) {
        let (typing, caret) = if self.naming { (&self.whisper, &self.carets[1]) } else { (&self.input, &self.carets[0]) };
        (caret.at(typing), caret.selection(typing))
    }

    /// A line was sent: kept for Up / Down.
    pub fn sent(&mut self, line: &str) {
        self.history.sent(line);
        self.carets[0].reset();
    }

    /// Up / Down: in the message box the lines sent (the unsent text comes back past the newest); in the name box
    /// the whisper names, a blank after the last.
    pub fn recall(&mut self, up: bool) {
        if self.naming {
            let names: Vec<&str> = self.recent.iter().map(String::as_str).chain([""]).collect();
            let at = names.iter().position(|name| *name == self.whisper);
            let next = match (at, up) {
                (None, true) => names.len().saturating_sub(2),
                (None, false) => 0,
                (Some(at), true) => at.saturating_sub(1),
                (Some(at), false) => (at + 1).min(names.len() - 1),
            };
            self.whisper = names[next].to_owned();
            self.carets[1].reset();
            return;
        }
        let line = if up { self.history.up(&self.input) } else { self.history.down() };
        if let Some(line) = line {
            self.input = line.to_owned();
            self.carets[0].reset();
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
        self.save_tabs();
    }

    /// The "all on" box: every option on, or off when they all were on. Saves the tabs.
    pub fn set_all(&mut self) {
        let Some(settings) = &self.settings else { return };
        let tab = &mut self.tabs[settings.tab];
        flip_all(&mut tab.kinds);
        tab.scroll = 0;
        self.save_tabs();
    }

    /// Keeps the tabs and the lock for the next login.
    fn save_tabs(&self) {
        if let Some(path) = settings_path().filter(|_| !cfg!(test)) {
            self.save_to(&path);
        }
    }

    pub fn save_to(&self, path: &Path) {
        let _ = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(path, tabs_text(&self.tabs, self.locked)));
    }

    /// Closes the battle-mode line: the typed text stays in the box until a key no shortcut takes, and the
    /// whisper-name list goes with the line.
    pub fn close_battle_line(&mut self) {
        self.preedit.clear();
        self.naming = false;
        self.stale = !self.input.is_empty();
        self.list_open = false;
    }

    /// The line opened again: what was typed is kept.
    pub fn reopened(&mut self) {
        self.stale = false;
        self.naming = false;
    }

    /// A key reached the closed line and no shortcut took it: text left by Esc goes.
    pub fn drop_stale(&mut self) {
        if std::mem::take(&mut self.stale) {
            self.input.clear();
            self.carets[0].reset();
        }
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
            // Tabs: the backgrounds every average width from x 4, the shown one's black, the others darker and a pixel
            // lower; over them each tab's outline (the others grey, with a white line along their bottom) and label.
            let spacing = self.average_tab();
            for tab in 0..self.tabs.len() {
                let shown = tab == self.tab;
                let [left, top] = at([4.0 + spacing * tab as f32, 0.0]);
                let (fill, lower) = if shown { (&assets.dark, 1.0) } else { (&assets.tab_dark, 2.0) };
                layer.sprites.push((fill, [left, top + lower], [spacing - 1.0, TAB_HEIGHT + 1.0 - lower]));
                let [left, top] = at(self.tab_corner(tab));
                let width = self.tab_width(tab);
                let border = if shown { &assets.white } else { &assets.tab_border };
                layer.sprites.push((border, [left, top], [width, 1.0]));
                layer.sprites.push((border, [left, top], [1.0, TAB_HEIGHT]));
                layer.sprites.push((border, [left + width - 1.0, top], [1.0, TAB_HEIGHT]));
                if !shown {
                    layer.sprites.push((&assets.white, [left, top + TAB_HEIGHT - 1.0], [width, 1.0]));
                }
                let label = crate::window::fit(font, self.tabs[tab].label.text(messages), self.label_room());
                let x = (left + (width - font.width(&label)) / 2.0).round();
                let y = top + if shown { 1.0 } else { 2.0 };
                layer.texts.push((label.clone(), [x + 1.0, y + 1.0], [0, 0, 0]));
                layer.texts.push((label, [x, y], [255; 3]));
            }
            // The row, right to left from the lock. Locking leaves only tab settings and moves the icon.
            for button in top_row(self.locked) {
                let images = match button.hit {
                    Hit::RemoveTab => &assets.remove_tab,
                    Hit::AddTab => &assets.add_tab,
                    Hit::Stick => &assets.tear,
                    Hit::Options => &assets.tab_settings,
                    Hit::ModeIcon => {
                        if let Some(open) = self.mode_icon {
                            let icon = &assets.mode_icon[usize::from(!open)];
                            layer.sprites.push((icon, at([button.x, 4.0]), button.size));
                        }
                        continue;
                    }
                    _ => continue,
                };
                let pressed = (button.hit == Hit::Stick && self.corner.is_none()) || (button.hit == Hit::Options && self.settings.is_some());
                let image = &images[usize::from(pressed)];
                layer.sprites.push((image, at([button.x, 4.0]), image.size));
            }
            let lock = &assets.lock[usize::from(!self.locked)];
            layer.sprites.push((lock, at(LOCK_AT), lock.size));

            // The log: translucent black between white edges, newest line at the bottom.
            let corner = at(LOG_BOX);
            let tabs_end = self.tab_corner(self.tabs.len())[0];
            let size = [LOG_WIDTH, self.height() - LOG_UNDER];
            layer.sprites.push((&assets.dark, corner, size));
            layer.sprites.push((&assets.white, corner, [1.0, size[1]]));
            layer.sprites.push((&assets.white, [corner[0] + size[0] - 1.0, corner[1]], [1.0, size[1]]));
            layer.sprites.push((&assets.white, at([tabs_end, LOG_BOX[1]]), [LOG_BOX[0] + size[0] - tabs_end, 1.0]));
            // The rows from the top down, then the scrollbar at the right.
            let (start, end) = self.shown_range();
            for (row, (line, color, ..)) in self.tabs[self.tab].rows[start..end].iter().enumerate() {
                let [mut x, y] = at(line_at(row));
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
            let [bar_x, bar_y] = self.scrollbar_corner();
            layer.sprites.push((&assets.track, at([bar_x, bar_y]), [SCROLLBAR, self.log_height()]));
            let (thumb_top, thumb_height) = self.thumb();
            layer.sprites.push((&assets.thumb, at([bar_x + 1.0, thumb_top]), [SCROLLBAR - 3.0, thumb_height]));
            layer.sprites.push((&assets.arrows[0], at([bar_x + 1.0, bar_y]), [ARROW; 2]));
            layer.sprites.push((&assets.arrows[1], at([bar_x + 1.0, bar_y + self.log_height() - ARROW]), [ARROW; 2]));
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
        // The box with the caret shows what the input method is composing at the caret, and its selection.
        let (at, selection) = self.caret();
        let (focused, focused_x) = if self.naming { (&self.whisper, name_x) } else { (&self.input, message_x) };
        let shown = format!("{}{}{}", &focused[..at], self.preedit, &focused[at..]);
        if let Some(range) = selection {
            let left = focused_x + font.width(&focused[..range.start]);
            layer.sprites.push((&assets.selection, [left, text_top], [font.width(&focused[range]), font.line_height()]));
        }
        let (name, typed) = if self.naming { (shown, self.input.clone()) } else { (self.whisper.clone(), shown) };
        layer.texts.push((name, [name_x, text_top], [0, 0, 0]));
        layer.texts.push((typed, [message_x, text_top], [0, 0, 0]));
        if caret {
            let x = focused_x + font.width(&format!("{}{}", &focused[..at], self.preedit));
            layer.sprites.push((&assets.darker, [x + 1.0, text_top + 1.0], [1.0, font.line_height() - 2.0]));
        }
        // ponytail: the list's look (white rows, the picked name blue) is ours.
        if self.list_open {
            for (row, name) in self.list_entries().into_iter().enumerate() {
                let [x, y, w, h] = self.row_box(row);
                let picked = !name.is_empty() && name == self.whisper;
                layer.sprites.push((if picked { &assets.picked } else { &assets.white }, [x, y], [w, h]));
                layer.texts.push((name.to_owned(), [x + 3.0, y + 2.0], [0, 0, 0]));
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
    /// The first row shown of `SETTINGS_KINDS`.
    first: usize,
}

const SETTINGS_WIDTH: f32 = 280.0;
const SETTINGS_HEIGHT: f32 = 200.0;
const SETTINGS_ROW: f32 = 20.0;
const SETTINGS_TOP: f32 = TITLE_HEIGHT + 5.0;
const SETTINGS_SHOWN: usize = 8;
const SETTINGS_ARROW: f32 = 12.0;
/// The "all on" box: ticked when every option is on, and a press sets all.
const SETTINGS_ALL: ([f32; 2], [f32; 2]) = ([20.0, SETTINGS_HEIGHT - 16.0], [70.0, 12.0]);
/// The message after the tab's name in the title.
const SETTINGS_TITLE: usize = 1777;
/// The kinds the settings window lists, in the game's order. Party search stays a kind; the game skips that row.
const SETTINGS_KINDS: [Kind; 24] = [
    Kind::General,
    Kind::Public,
    Kind::Whisper,
    Kind::Party,
    Kind::Guild,
    Kind::Ally,
    Kind::ItemLog,
    Kind::Equipment,
    Kind::Status,
    Kind::PartyItemLog,
    Kind::PartyStatus,
    Kind::SkillFailure,
    Kind::PartyConfig,
    Kind::DamagedEquipment,
    Kind::Woe,
    Kind::Battle,
    Kind::PartyBattle,
    Kind::Experience,
    Kind::Quest,
    Kind::Battlefield,
    Kind::Clan,
    Kind::DisplayCall,
    Kind::GuildExp,
    Kind::EquipProperties,
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SettingsHit {
    Title(TitleHit),
    Kind(Kind),
    /// The "all on" box.
    All,
    ScrollUp,
    ScrollDown,
    Window,
}

impl Settings {
    pub fn new(tab: usize) -> Self {
        let mut frame = Frame::new([620.0, screen()[1] - 360.0], SETTINGS_WIDTH);
        frame.open = true;
        Self { frame, tab, first: 0 }
    }

    /// The rows on screen: their place in the window and their kind.
    fn rows(&self) -> impl Iterator<Item = (usize, Kind)> + '_ {
        SETTINGS_KINDS.iter().copied().skip(self.first).take(SETTINGS_SHOWN).enumerate()
    }

    /// Scrolls the list by `rows`.
    pub fn scroll(&mut self, rows: i32) {
        self.first = (self.first as i32 + rows).clamp(0, (SETTINGS_KINDS.len() - SETTINGS_SHOWN) as i32) as usize;
    }

    fn arrow(&self, down: bool) -> ([f32; 2], [f32; 2]) {
        let y = if down { SETTINGS_TOP + SETTINGS_SHOWN as f32 * SETTINGS_ROW - SETTINGS_ARROW } else { SETTINGS_TOP };
        ([SETTINGS_WIDTH - 15.0, y], [SETTINGS_ARROW, SETTINGS_ARROW])
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<SettingsHit> {
        let local = self.frame.local(cursor, SETTINGS_HEIGHT)?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(SettingsHit::Title(hit));
        }
        let on = |(at, size): ([f32; 2], [f32; 2])| local[0] >= at[0] && local[0] < at[0] + size[0] && local[1] >= at[1] && local[1] < at[1] + size[1];
        if on(SETTINGS_ALL) {
            return Some(SettingsHit::All);
        }
        if on(self.arrow(false)) {
            return Some(SettingsHit::ScrollUp);
        }
        if on(self.arrow(true)) {
            return Some(SettingsHit::ScrollDown);
        }
        let row = ((local[1] - SETTINGS_TOP) / SETTINGS_ROW).floor();
        Some(match self.rows().nth(row.max(0.0) as usize) {
            Some((_, kind)) if local[1] >= SETTINGS_TOP && local[0] < SETTINGS_WIDTH - 17.0 => SettingsHit::Kind(kind),
            _ => SettingsHit::Window,
        })
    }

    /// Each kind's row on screen (for the test driver), named "tab_<kind>", and the scroll arrows and the all box.
    pub fn button_boxes(&self) -> Vec<(String, [f32; 4])> {
        let boxed = |name: String, at: [f32; 2], size: [f32; 2]| {
            let [x, y] = self.frame.at(at);
            (name, [x, y, size[0], size[1]])
        };
        let mut boxes: Vec<_> = self
            .rows()
            .map(|(row, kind)| boxed(format!("tab_{}", kind.name()), [8.0, SETTINGS_TOP + row as f32 * SETTINGS_ROW], [SETTINGS_WIDTH - 26.0, SETTINGS_ROW]))
            .collect();
        for (name, (at, size)) in [("tab_all", SETTINGS_ALL), ("tab_scroll_up", self.arrow(false)), ("tab_scroll_down", self.arrow(true))] {
            boxes.push(boxed(name.to_owned(), at, size));
        }
        boxes
    }

    pub fn draw<'a>(&self, layer: &mut Layer<'a>, chrome: &'a Chrome, messages: &[String], tab: &Tab, pointer: [f32; 2]) {
        let at = |local: [f32; 2]| self.frame.at(local);
        let kinds = &tab.kinds;
        let title = format!("{} {}", tab.label.text(messages), messages.get(SETTINGS_TITLE).map_or("", |text| text.trim())).trim().to_owned();
        self.frame.draw_title(layer, chrome, &title, pointer);
        if self.frame.minimized {
            return;
        }
        layer.sprites.push((&chrome.white, at([0.0, TITLE_HEIGHT]), [SETTINGS_WIDTH, SETTINGS_HEIGHT - TITLE_HEIGHT]));
        for x in [0.0, SETTINGS_WIDTH - 1.0] {
            layer.sprites.push((&chrome.border, at([x, TITLE_HEIGHT]), [1.0, SETTINGS_HEIGHT - TITLE_HEIGHT]));
        }
        layer.sprites.push((&chrome.border, at([0.0, SETTINGS_HEIGHT - 1.0]), [SETTINGS_WIDTH, 1.0]));
        for (row, kind) in self.rows() {
            let mark = if kinds.contains(&kind) { "■" } else { "□" };
            layer.texts.push((format!("{mark} {}", kind.label(messages)), at([8.0, SETTINGS_TOP + row as f32 * SETTINGS_ROW]), [0, 0, 0]));
        }
        for (down, mark) in [(false, "▲"), (true, "▼")] {
            layer.texts.push((
                mark.to_owned(),
                at(self.arrow(down).0),
                if (down && self.first + SETTINGS_SHOWN < SETTINGS_KINDS.len()) || (!down && self.first > 0) { [0, 0, 0] } else { [160; 3] },
            ));
        }
        let all = if all_on(kinds) { "■" } else { "□" };
        layer.texts.push((format!("{all} All"), at(SETTINGS_ALL.0), [0, 0, 0]));
    }
}

/// The "all on" box's press: all on, or all off when they were.
fn flip_all(kinds: &mut HashSet<Kind>) {
    if all_on(kinds) {
        kinds.clear();
    } else {
        kinds.extend(SETTINGS_KINDS.iter().chain(&[Kind::PartySearch]));
    }
}

/// Whether every option the window lists is on (the party search row too, which it hides).
fn all_on(kinds: &HashSet<Kind>) -> bool {
    SETTINGS_KINDS.iter().chain(&[Kind::PartySearch]).all(|kind| kinds.contains(kind))
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
    /// /quake: camera shakes on or off.
    Quake,
    /// /mineffect: lighter effects on or off.
    MinEffect,
    /// /fog: the map's depth fog on or off.
    Fog,
    /// /mable: Spirit Marble's spirits shown or hidden.
    Mable,
    /// /aura: the max-level aura in full or simplified; /aura2: none at all.
    Aura,
    AuraOff,
    /// /emo, /emotion: the emotion list, open or closed.
    EmotionList,
    /// /memo: where the player stands becomes a Warp Portal destination.
    Memo,
    /// /achievement: the achievement window, open or closed.
    Achievements,
    /// /noshift: a click on a player attacks without Shift, on or off.
    NoShift,
    /// /shopping, /sh: a shop's sign opens on one click (on) or only on a double click (off).
    Shopping,
    /// /loginout (/li): the guild notice and members' log-in lines on or off; /notalkmsg2 (/nm2): guild chat hidden.
    LoginOut,
    NoGuildTalk,
    /// /emblem: guild and clan emblems next to hovered players' names, on or off.
    Emblem,
    /// /notrade, /nt: deal requests are refused at once, on or off.
    NoTrade,
    /// /camera: the camera sticks to the player instead of gliding after them, on or off.
    Camera,
    /// /zoom: the camera can go further out outdoors, on or off.
    Zoom,
    /// /noctrl, /nc: a click on a monster keeps attacking without Ctrl, on or off.
    NoCtrl,
    /// /skillfail, /sf: the red lines for some skill failures, on or off.
    SkillFail,
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
    /// /cashshop: the cash shop; /battleground: the battleground queue, open or closed. The game opens them from
    /// elsewhere than the menu.
    // ponytail: the commands' names are ours.
    CashShop,
    BattleQueue,
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
        "/quake" => Command::Quake,
        "/mineffect" => Command::MinEffect,
        "/fog" => Command::Fog,
        "/mable" => Command::Mable,
        "/aura" => Command::Aura,
        "/aura2" => Command::AuraOff,
        "/emo" | "/emotion" => Command::EmotionList,
        "/achievement" => Command::Achievements,
        "/memo" => Command::Memo,
        "/noshift" => Command::NoShift,
        "/shopping" | "/sh" => Command::Shopping,
        "/loginout" | "/li" => Command::LoginOut,
        "/notalkmsg2" | "/nm2" => Command::NoGuildTalk,
        "/emblem" => Command::Emblem,
        "/notrade" | "/nt" => Command::NoTrade,
        "/noctrl" | "/nc" => Command::NoCtrl,
        "/skillfail" | "/sf" => Command::SkillFail,
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
        "/cashshop" => Command::CashShop,
        "/battleground" => Command::BattleQueue,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A chat with the default tabs (not the saved ones), measured at 6 px a character with "Public" and "Battle".
    fn fresh() -> Chat {
        let mut chat = Chat::with_tabs(default_tabs());
        chat.measure_tabs(&labels(), &six_px);
        chat
    }

    fn six_px(text: &str) -> f32 {
        text.chars().count() as f32 * 6.0
    }

    fn labels() -> Vec<String> {
        let mut messages = vec![String::new(); 1300];
        messages[1291] = "Public".into();
        messages[1292] = "Battle".into();
        messages
    }

    fn middle_of(chat: &Chat, name: &str) -> [f32; 2] {
        let [x, y, w, h] = chat.button_boxes().into_iter().find(|(box_name, _)| *box_name == name).unwrap_or_else(|| panic!("{name}")).1;
        [x + w / 2.0, y + h / 2.0]
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
        assert_eq!([0, 1, 4, 7, 10, 13, 2, 3, 15].map(next_step), [1, 4, 7, 10, 13, 0, 4, 7, 0]);
        assert_eq!([60.0, 72.0, 96.0, 292.0, 400.0].map(step_for), [0, 1, 2, 15, 15]);
        assert_eq!([height(0), height(1), height(15)], [24.0, 96.0, 292.0]);
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
        // The boxes take the caret, the size button the size, the receiving group turns.
        let mut chat = fresh();
        chat.click([50.0, screen()[1] - 12.0]);
        assert!(chat.naming);
        assert_eq!(chat.focus(), "name");
        chat.click([300.0, screen()[1] - 12.0]);
        assert!(!chat.naming);
        assert_eq!(chat.focus(), "message");
        assert_eq!(chat.tooltip([576.0, screen()[1] - 12.0]), Some(3188));
        chat.click([576.0, screen()[1] - 12.0]);
        assert_eq!(chat.send_to, SendTo::Party);
        chat.click([590.0, screen()[1] - 12.0]);
        assert_eq!(chat.height(), 138.0);
    }

    #[test]
    fn recent_names_fill_the_whisper_box() {
        let mut chat = fresh();
        let middle = |chat: &Chat, name: &str| {
            let [x, y, w, h] = chat.list_boxes().into_iter().find(|(box_name, _)| box_name == name).unwrap().1;
            [x + w / 2.0, y + h / 2.0]
        };
        // An empty list says so.
        assert_eq!(chat.click(middle(&chat, "whisper_list")), Some(Said::Line(NO_WHISPER_LIST)));
        assert!(!chat.list_open);
        // Names go at the end, once each; a received whisper doesn't fill the box.
        chat.whispered_by("Drops");
        chat.note_recent("Poring");
        chat.note_recent("drops");
        assert_eq!(chat.recent, ["Drops", "Poring"]);
        assert_eq!(chat.whisper, "");
        assert_eq!(chat.click(middle(&chat, "whisper_list")), None);
        assert!(chat.list_open);
        let names: Vec<String> = chat.list_boxes().into_iter().skip(1).map(|(name, _)| name).collect();
        assert_eq!(names, ["recent:Drops", "recent:Poring", "recent:"]);
        // 112 wide, its bottom one pixel over the input bar's top, at x 1.
        let [x, y, w, h] = chat.row_box(2);
        assert_eq!([x, y + h, w], [1.0, screen()[1] - INPUT_HEIGHT + 1.0, 112.0]);
        chat.click(middle(&chat, "recent:Poring"));
        assert_eq!(chat.whisper, "Poring");
        assert!(!chat.list_open);
        // The blank row empties the box; at most five rows, the newest.
        chat.click(middle(&chat, "whisper_list"));
        chat.click(middle(&chat, "recent:"));
        assert_eq!(chat.whisper, "");
        (0..6).for_each(|index| chat.note_recent(&format!("N{index}")));
        assert_eq!(chat.list_entries(), ["N2", "N3", "N4", "N5", ""]);
        // A whisper brings back a hidden window and a log down to the bar.
        chat.hidden = true;
        chat.size = 0;
        chat.whispered_by("Drops");
        assert!(!chat.hidden && chat.size == 1);
    }

    #[test]
    fn cuts_wraps_and_keeps_240_lines() {
        // Pieces of at most 94 bytes, at a space where there is one.
        let long = format!("{} {}", "a".repeat(90), "b".repeat(20));
        assert_eq!(pieces(&long), ["a".repeat(90), "b".repeat(20)]);
        assert_eq!(pieces(&"c".repeat(100)), ["c".repeat(94), "c".repeat(6)]);
        assert_eq!(pieces(&"가".repeat(40)).iter().map(String::len).collect::<Vec<_>>(), [93, 27]);
        let link = format!("{} <ITEM>Apple<INFO>512</INFO></ITEM>", "x".repeat(100));
        assert_eq!(pieces(&link).len(), 1);
        // Wrapped at 576 px: 6 px a character here, so 96 to a row.
        let measure = |text: &str| text.chars().count() as f32 * 6.0;
        let words = vec!["word"; 30].join(" ");
        let rows = wrap(&words, WRAP_WIDTH, &measure);
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|row| measure(row) <= WRAP_WIDTH && !row.starts_with(' ')));
        // 300 lines: the oldest 60 go.
        let mut chat = fresh();
        for line in 0..300 {
            chat.push(format!("line {line}"), PUBLIC);
        }
        chat.layout(&measure);
        let tab = chat.tab_lines(0);
        assert_eq!((tab.len(), tab[0].0.as_str()), (MOST_LINES, "line 60"));
        // A 120-byte line takes two rows (cut at 94 bytes), still 240 in all.
        chat.push("z ".repeat(60), PUBLIC);
        chat.layout(&measure);
        assert_eq!(chat.tab_lines(0).len(), MOST_LINES);
        assert_eq!(chat.tab_lines(0).last().unwrap().0, "z ".repeat(13));
    }

    #[test]
    fn fills_from_the_top_and_names_speakers() {
        let mut chat = fresh();
        chat.push("Bob : hi".into(), PUBLIC);
        chat.push_kind("( From Ann ) : psst".into(), [0xFF, 0xFF, 0x00], Kind::Whisper);
        chat.layout(&|text: &str| text.len() as f32);
        assert_eq!(chat.shown_range(), (0, 2));
        // The first row sits at the top of the log, the second under it.
        let rows: Vec<[f32; 2]> = (0..3)
            .map(|index| {
                let [x, y] = chat.at(line_at(index));
                [x + 4.0, y + 4.0]
            })
            .collect();
        let row = |index: usize| rows[index];
        assert_eq!(chat.speaker_at(row(0)), Some("Bob"));
        assert_eq!(chat.speaker_at(row(1)), None, "a whisper names no speaker");
        assert_eq!(chat.speaker_at(row(2)), None, "an empty row");
        assert_eq!(chat.log_click(row(0)), None);
        assert_eq!(chat.log_click(row(0)).as_deref(), Some("Bob"));
        chat.whisper_to("Bob");
        assert_eq!((chat.whisper.as_str(), chat.recent.as_slice()), ("Bob", ["Bob".to_owned()].as_slice()));
        assert_eq!([speaker("Leader : party", Kind::Party), speaker("You used Bash.", Kind::General)], [Some("Leader".into()), None]);
        assert_eq!(speaker(&format!("{} : long", "n".repeat(24)), Kind::Public), None);
    }

    #[test]
    fn picks_tabs_and_routes_lines_by_kind() {
        let mut chat = fresh();
        let strip = top(&chat) + 8.0;
        // "Public" and "Battle" are 36 px here: tabs 46 wide from x 3.
        assert_eq!([chat.hit([20.0, strip]), chat.hit([70.0, strip]), chat.hit([400.0, strip])], [Some(Hit::Tab(0)), Some(Hit::Tab(1)), Some(Hit::TopRow)]);
        assert_eq!([chat.tab_corner(1), chat.tab_corner(2)], [[49.0, 0.0], [95.0, 0.0]]);
        chat.push("Poring : hi".into(), PUBLIC);
        chat.push("Leader : party".into(), PARTY);
        chat.push_kind("( From A ) : psst".into(), [0xFF, 0xFF, 0x00], Kind::Whisper);
        chat.push("You used Bash.".into(), SKILL_MESSAGE);
        chat.push_kind("Poring hits you".into(), [0xFF, 0xFF, 0x00], Kind::Battle);
        let texts = |chat: &Chat, tab: usize| chat.tab_lines(tab).iter().map(|(text, ..)| text.clone()).collect::<Vec<_>>();
        // Tab 1 has the first 14 categories (a skill failure is one), tab 2 the battle lines.
        assert_eq!(texts(&chat, 0), ["Poring : hi", "Leader : party", "( From A ) : psst", "You used Bash."]);
        assert_eq!(texts(&chat, 1), ["Poring hits you"]);
        chat.click([70.0, strip]);
        assert_eq!(chat.tab, 1);
        // Party unticked for Public: new party lines stay out of its log.
        chat.tabs[0].kinds.remove(&Kind::Party);
        chat.push("Leader : again".into(), PARTY);
        assert_eq!(texts(&chat, 0), ["Poring : hi", "Leader : party", "( From A ) : psst", "You used Bash."]);
        // Ticked for Battle: it gets them.
        chat.tabs[1].kinds.insert(Kind::Party);
        chat.push("Leader : third".into(), PARTY);
        assert_eq!(texts(&chat, 1).last().map(String::as_str), Some("Leader : third"));
    }

    #[test]
    fn words_the_battle_messages() {
        let mut messages = vec![String::new(); 1700];
        messages[crate::BATTLE_ON_YOU] = " [%s] inflicts '%d' damage points on you.".into();
        messages[crate::BATTLE_ATTACKED_BY] = " [%s] attacked by [%s] with '%d' damage points.".into();
        messages[crate::BATTLE_RECEIVES] = " [%s] receives '%d' damage points.".into();
        messages[crate::BATTLE_ATTACKS] = " [%s] attacks [%s] with '%d' damage points.".into();
        let line = |source: (Role, &str), target: (Role, &str), amount| battle_line(&messages, source, target, amount);
        let (me, ann, poring) = ((Role::Me, "Me"), (Role::Party, "Ann"), (Role::Other, "Poring"));
        assert_eq!(line(poring, me, 12), Some((" [Poring] inflicts '12' damage points on you.".into(), BATTLE_OWN, Kind::Battle)));
        assert_eq!(line(me, poring, 40), Some((" [Poring] receives '40' damage points.".into(), BATTLE_OWN, Kind::Battle)));
        assert_eq!(line(poring, ann, 7), Some((" [Ann] attacked by [Poring] with '7' damage points.".into(), PARTY, Kind::PartyBattle)));
        assert_eq!(line(ann, poring, 9), Some((" [Ann] attacks [Poring] with '9' damage points.".into(), PARTY, Kind::PartyBattle)));
        // Neither of us, a miss, and a party member hitting us all work out as the player's own.
        assert_eq!([line(poring, (Role::Other, "Drops"), 5), line(poring, me, 0), line(poring, me, -1)], [None, None, None]);
        assert_eq!(line(ann, me, 3).map(|line| line.2), Some(Kind::Battle));
    }

    #[test]
    fn shows_broadcasts_and_red_errors_in_every_tab() {
        let mut chat = fresh();
        chat.tabs[0].kinds.clear();
        chat.push_kind("Server: hello".into(), [0xFF, 0xFF, 0x00], Kind::Broadcast);
        chat.push("Cannot do that".into(), ERROR_RED);
        chat.push("Notice".into(), [0xFF, 0xFF, 0x64]);
        let texts = |chat: &Chat, tab: usize| chat.tab_lines(tab).iter().map(|(text, ..)| text.clone()).collect::<Vec<_>>();
        assert_eq!(texts(&chat, 0), ["Server: hello", "Cannot do that"]);
        assert_eq!(texts(&chat, 1), ["Server: hello", "Cannot do that"]);
        // Our own chat is public chat for the tabs.
        chat.tabs[0].kinds.insert(Kind::Public);
        chat.push("Me : hi".into(), OWN);
        assert_eq!(texts(&chat, 0).last().map(String::as_str), Some("Me : hi"));
    }

    #[test]
    fn has_an_mcp_tab_only_when_added() {
        let mut chat = fresh();
        let strip = top(&chat) + 8.0;
        let texts = |chat: &Chat, tab: usize| chat.tab_lines(tab).iter().map(|(text, ..)| text.clone()).collect::<Vec<_>>();
        // Without it the third tab's place is the strip, and an MCP line shows nowhere.
        assert_eq!(chat.hit([250.0, strip]), Some(Hit::TopRow));
        chat.push_kind("click (1, 2)".into(), [200, 170, 255], Kind::Mcp);
        assert!(chat.tabs.iter().all(|tab| texts(&chat, chat.tabs.iter().position(|other| other == tab).unwrap()).is_empty()));
        chat.add_mcp_tab();
        chat.add_mcp_tab();
        assert_eq!(chat.tabs.len(), 3);
        chat.measure_tabs(&labels(), &six_px);
        assert_eq!(chat.hit([110.0, strip]), Some(Hit::Tab(2)));
        assert!(chat.button_boxes().iter().any(|(name, _)| *name == "chat_tab_2"));
        // Its lines go to it alone, and nothing else goes to it.
        chat.push_kind("click (1, 2)".into(), [200, 170, 255], Kind::Mcp);
        chat.push("Poring : hi".into(), PUBLIC);
        assert_eq!(texts(&chat, 2), ["click (1, 2)"]);
        assert_eq!(texts(&chat, 0), ["Poring : hi"]);
        assert_eq!(texts(&chat, 1), Vec::<String>::new());
        // Picking it: it has no tab settings, and it isn't saved.
        chat.click([110.0, strip]);
        assert_eq!(chat.tab, 2);
        assert!(chat.on_mcp_tab());
        chat.click(middle_of(&chat, "chat_options"));
        assert!(chat.settings.is_none());
        assert_eq!(tabs_from(&tabs_text(&chat.tabs, false)).map(|(tabs, _)| tabs.len()), Some(2));
        assert_eq!(chat.tabs[2].label.text(&[]), "MCP");
    }

    #[test]
    fn scrolls_each_tab_on_its_own() {
        let measure = |text: &str| text.len() as f32;
        let mut chat = fresh();
        for line in 0..20 {
            chat.push(format!("line {line}"), PUBLIC);
        }
        chat.layout(&measure);
        chat.scroll_by(-4);
        assert_eq!((chat.tabs[0].scroll, chat.tabs[1].scroll), (4, 0));
        // A new public line keeps Public where it was scrolled; Battle doesn't show it.
        chat.push("line 20".into(), PUBLIC);
        chat.layout(&measure);
        assert_eq!((chat.tabs[0].scroll, chat.tabs[1].scroll), (5, 0));
        // Three wheel notches: nine rows; PageUp the shown rows less one; Ctrl+PageDown the bottom.
        chat.scroll_by(-9);
        assert_eq!(chat.tabs[0].scroll, 14);
        chat.page(true, false);
        assert_eq!(chat.tabs[0].scroll, 12);
        chat.page(true, true);
        assert_eq!(chat.tabs[0].scroll, 0);
        chat.page(false, true);
        assert_eq!(chat.tabs[0].scroll, 21 - 3);
        // At the bottom it follows new lines.
        chat.page(true, true);
        chat.push("line 21".into(), PUBLIC);
        chat.layout(&measure);
        assert_eq!(chat.tabs[0].scroll, 0);
        chat.tab = 1;
        chat.scroll_by(-4);
        assert_eq!(chat.tabs[1].scroll, 0, "nothing to scroll back to");
    }

    #[test]
    fn recalls_lines_and_edits_at_the_caret() {
        let mut chat = fresh();
        for line in ["a", "b", "c"] {
            chat.type_text(line);
            let typed = std::mem::take(&mut chat.input);
            chat.sent(&typed);
        }
        chat.type_text("draft");
        chat.recall(true);
        chat.recall(true);
        assert_eq!(chat.input, "b");
        chat.recall(false);
        chat.recall(false);
        assert_eq!(chat.input, "draft");
        // Shift+Home selects to the start, Ctrl+X cuts it, a paste goes in at the caret.
        chat.go(Move::Left, false);
        chat.go(Move::Home, true);
        assert_eq!(chat.selected(), "draf");
        assert_eq!(chat.cut(), "draf");
        assert_eq!((chat.input.as_str(), chat.caret().0), ("t", 0));
        chat.type_text("pas");
        chat.delete();
        assert_eq!(chat.input, "pas");
        chat.select_all();
        chat.backspace();
        assert_eq!(chat.input, "");
        // Up / Down in the name box walk the whisper names, a blank after the last.
        chat.naming = true;
        chat.note_recent("Ann");
        chat.note_recent("Bob");
        chat.recall(true);
        assert_eq!(chat.whisper, "Bob");
        chat.recall(true);
        assert_eq!(chat.whisper, "Ann");
        chat.recall(false);
        chat.recall(false);
        assert_eq!(chat.whisper, "");
    }

    #[test]
    fn shows_the_battle_mode_icon_and_keeps_text_esc_left() {
        let mut chat = fresh();
        let at = |chat: &Chat, local: [f32; 2]| [local[0], top(chat) + local[1]];
        let x_of = |chat: &Chat, name: &str| chat.button_boxes().into_iter().find(|(button, _)| *button == name).map(|(_, box_)| box_[0]);
        // Out of battle mode there's no icon to press, so no target for it.
        assert_eq!(x_of(&chat, "chat_mode"), None);
        chat.mode_icon = Some(true);
        // Unlocked, right to left from the lock: remove, add, tear-off, settings, icon.
        assert_eq!(
            ["chat_remove_tab", "chat_add_tab", "chat_stick", "chat_options", "chat_mode", "chat_lock"].map(|name| x_of(&chat, name)),
            [Some(570.0), Some(560.0), Some(544.0), Some(533.0), Some(517.0), Some(580.0)]
        );
        // Locking leaves only settings and moves the icon.
        chat.locked = true;
        assert_eq!(["chat_options", "chat_mode", "chat_remove_tab", "chat_stick"].map(|name| x_of(&chat, name)), [Some(569.0), Some(553.0), None, None]);
        chat.locked = false;
        chat.mode_icon = None;
        // Out of battle mode the icon's place is empty; in it, (518, 5) hits it unlocked and (554, 5) locked.
        assert_ne!(chat.hit(at(&chat, [518.0, 5.0])), Some(Hit::ModeIcon));
        chat.mode_icon = Some(true);
        assert_eq!(chat.hit(at(&chat, [518.0, 5.0])), Some(Hit::ModeIcon));
        assert_ne!(chat.hit(at(&chat, [518.0, 14.0])), Some(Hit::ModeIcon));
        chat.locked = true;
        assert_eq!(chat.hit(at(&chat, [554.0, 5.0])), Some(Hit::ModeIcon));
        assert_ne!(chat.hit(at(&chat, [518.0, 5.0])), Some(Hit::ModeIcon));
        // Not while the log is down to the input bar.
        chat.locked = false;
        chat.size = 0;
        assert_eq!(chat.hit(at(&chat, [518.0, 5.0])), Some(Hit::Message));
        // Esc keeps "abc" and closes the whisper list; opening again keeps the text; a key on the closed line empties it.
        chat.input = "abc".into();
        chat.note_recent("Ann");
        chat.click(at(&chat, [LIST_BUTTON[0] + 1.0, chat.height() - INPUT_HEIGHT + LIST_BUTTON[1] + 1.0]));
        assert!(chat.list_open);
        chat.close_battle_line();
        assert!(!chat.list_open);
        chat.reopened();
        assert_eq!(chat.input, "abc");
        chat.close_battle_line();
        chat.drop_stale();
        assert_eq!(chat.input, "");
        assert!(Settings::new(0).button_boxes().iter().all(|(name, _)| name != "tab_party_search"));
    }

    #[test]
    fn lists_the_games_categories_in_a_scrolling_window() {
        let mut settings = Settings::new(0);
        settings.frame.open = true;
        let at = |settings: &Settings, name: &str| {
            let (_, [x, y, width, height]) = settings.button_boxes().into_iter().find(|(box_name, _)| box_name == name).unwrap_or_else(|| panic!("{name}"));
            [x + width / 2.0, y + height / 2.0]
        };
        // Eight rows at a time, the game's first eight.
        let names: Vec<String> =
            settings.button_boxes().into_iter().map(|(name, _)| name).filter(|name| !name.starts_with("tab_scroll") && name != "tab_all").collect();
        assert_eq!(names, ["tab_general", "tab_public", "tab_whisper", "tab_party", "tab_guild", "tab_ally", "tab_item_log", "tab_equipment"]);
        assert_eq!(settings.hit(at(&settings, "tab_guild")), Some(SettingsHit::Kind(Kind::Guild)));
        assert_eq!(settings.hit(at(&settings, "tab_all")), Some(SettingsHit::All));
        assert_eq!(settings.hit(at(&settings, "tab_scroll_down")), Some(SettingsHit::ScrollDown));
        // Scrolling stops at both ends; the last row is the equipment properties, and the party search row isn't listed.
        settings.scroll(100);
        let names: Vec<String> = settings.button_boxes().into_iter().map(|(name, _)| name).collect();
        assert!(names.contains(&"tab_equip_properties".to_owned()) && !names.contains(&"tab_general".to_owned()));
        assert_eq!(settings.hit(at(&settings, "tab_party_battle")), Some(SettingsHit::Kind(Kind::PartyBattle)));
        settings.scroll(-100);
        assert_eq!(settings.rows().next(), Some((0, Kind::General)));
        assert!(SETTINGS_KINDS.iter().all(|kind| *kind != Kind::PartySearch) && SETTINGS_KINDS.len() + 1 == Kind::ALL.len() - 1);
        // The all box: everything on, then everything off.
        let mut kinds = default_kinds()[0].clone();
        assert!(!all_on(&kinds));
        flip_all(&mut kinds);
        assert!(all_on(&kinds));
        flip_all(&mut kinds);
        assert!(kinds.is_empty());
        // Labels come from the game's table, else ours.
        let mut messages = vec![String::new(); 1700];
        messages[1618] = "Battle message".into();
        assert_eq!(
            [Kind::Battle.label(&messages), Kind::Ally.label(&messages), Kind::Experience.label(&messages)],
            ["Battle message", "Ally Chat", "Experience message"]
        );
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
    fn drags_by_the_bare_window_and_resizes_by_the_top_edge() {
        let mut chat = fresh();
        // The top row, the log and the boxes never drag; the margin beside the log does.
        assert!(!chat.press([400.0, top(&chat) + 8.0]));
        assert!(!chat.press([300.0, top(&chat) + 40.0]));
        let margin = [2.0, top(&chat) + 40.0];
        assert_eq!(chat.hit(margin), Some(Hit::Strip));
        assert!(chat.press(margin));
        chat.drag_to([102.0, margin[1] - 200.0]);
        assert!(chat.drop_drag());
        assert_eq!(chat.corner(), [100.0, screen()[1] - 200.0]);
        // So do the input bar's gaps: left of the name box, between the boxes, under them; the press also takes the
        // caret to the message box.
        let bar = screen()[1] - 200.0 - INPUT_HEIGHT;
        let gaps = [[103.0, bar + 12.0], [207.0, bar + 12.0], [400.0, bar + 22.0], [400.0, bar + 2.0], [684.0, bar + 12.0]];
        assert_eq!(gaps.map(|gap| chat.hit(gap)), [Some(Hit::Strip); 5]);
        assert_eq!([chat.hit([300.0, bar + 12.0]), chat.hit([150.0, bar + 12.0])], [Some(Hit::Message), Some(Hit::Name)]);
        chat.naming = true;
        assert!(chat.press(gaps[0]));
        assert!(!chat.naming);
        // Within 10 px of the screen's edge it snaps flush; out past it, it stays where it can be grabbed.
        // Grabbed 3 right of the left edge and 12 over the bottom: a corner at (5, 761) is 5 and 7 off.
        chat.drag_to([8.0, screen()[1] - 7.0 - 12.0]);
        assert_eq!(chat.corner(), [0.0, screen()[1]]);
        chat.drag_to([-900.0, -500.0]);
        chat.drop_drag();
        assert_eq!(chat.corner(), [-560.0, chat.height()]);
        // With battle mode's line closed the boxes are gone: all of the bar grabs.
        chat.corner = None;
        chat.mode_icon = Some(false);
        assert_eq!(chat.hit([300.0, screen()[1] - 12.0]), Some(Hit::Strip));
        chat.mode_icon = None;
        // Locked it is held (and fades) but stays.
        chat.locked = true;
        let grip = middle_of(&chat, "chat_grip");
        assert!(chat.press(grip));
        assert!(chat.held.is_some());
        chat.drag_to([grip[0] + 100.0, grip[1] - 100.0]);
        assert!(chat.drop_drag());
        assert_eq!(chat.corner_moved(), None);
        // The strip along the top resizes, locked too, keeping the bottom edge: 152 px tall makes step 6.
        chat.corner = None;
        let edge = [300.0, top(&chat) + 18.0];
        assert_eq!(chat.hit(edge), Some(Hit::Resize));
        assert!(chat.press(edge));
        chat.drag_to([300.0, edge[1] - 56.0]);
        assert_eq!((chat.size, chat.height()), (6, 166.0));
        chat.drag_to([300.0, edge[1] - 1000.0]);
        assert_eq!(chat.height(), 292.0);
        chat.drag_to([300.0, edge[1] + 50.0]);
        assert_eq!(chat.size, 0);
        assert!(chat.drop_drag());
        assert_eq!(top(&chat) + chat.height(), screen()[1]);
    }

    #[test]
    fn adds_and_removes_tabs() {
        let mut chat = fresh();
        // A new tab after the two, as wide as its label and 10 ("NewTab_3" is 48 px here), showing nothing.
        assert_eq!(chat.click(middle_of(&chat, "chat_add_tab")), None);
        chat.measure_tabs(&labels(), &six_px);
        assert_eq!(chat.tabs[2].label, Label::Text("NewTab_3".into()));
        assert!(chat.tabs[2].kinds.is_empty());
        assert_eq!((chat.tab_corner(2), chat.tab_width(2)), ([95.0, 0.0], 58.0));
        chat.push("Poring : hi".into(), PUBLIC);
        assert!(chat.tab_lines(2).is_empty());
        // Its settings tick what it shows.
        chat.click(middle_of(&chat, "chat_tab_2"));
        chat.click(middle_of(&chat, "chat_options"));
        assert_eq!(chat.settings.as_ref().map(|settings| settings.tab), Some(2));
        chat.toggle_kind(Kind::Public);
        chat.push("Poring : again".into(), PUBLIC);
        assert_eq!(chat.tab_lines(2).len(), 1);
        // Picking another tab moves the settings window to it.
        chat.click(middle_of(&chat, "chat_tab_0"));
        assert_eq!(chat.settings.as_ref().map(|settings| settings.tab), Some(0));
        // Ten at most: then "Cannot add more." in a box. Before the MCP tab, which stays last.
        chat.add_mcp_tab();
        let four_px = |text: &str| text.chars().count() as f32 * 4.0;
        let mut said = None;
        for _ in 0..10 {
            chat.measure_tabs(&labels(), &four_px);
            said = chat.click(middle_of(&chat, "chat_add_tab"));
        }
        assert_eq!(said, Some(Said::Notice(CANNOT_ADD_MORE)));
        assert_eq!(chat.tabs.len(), MOST_TABS + 1);
        assert!(chat.tabs.last().unwrap().is_mcp() && chat.tabs[9].label == Label::Text("NewTab_10".into()));
        // Wider labels fill the strip first: a tab that wouldn't leave 90 px to the buttons isn't made either.
        let mut full = fresh();
        let wide = |text: &str| text.chars().count() as f32 * 12.0;
        while full.add_tab().is_ok() {
            full.measure_tabs(&labels(), &wide);
        }
        assert!(full.tabs.len() < MOST_TABS && (full.tabs.len() + 1) as f32 * full.average_tab() > WIDTH - STRIP_ROOM);
        // Eleven tabs share the strip: the labels are cut to 600 / 11 less 10.
        chat.tabs[3].label = Label::Text("a long name for a tab".into());
        chat.measure_tabs(&labels(), &six_px);
        assert_eq!(chat.tab_width(3), WIDTH / 11.0);
        // Remove takes the tab shown; the one before it shows next; the last tab stays.
        chat.click(middle_of(&chat, "chat_tab_3"));
        chat.click(middle_of(&chat, "chat_remove_tab"));
        assert_eq!((chat.tabs.len(), chat.tab, chat.settings.is_none()), (MOST_TABS, 2, true));
        assert!(chat.tabs.iter().all(|tab| tab.label != Label::Text("a long name for a tab".into())));
        while chat.tabs.len() > 1 {
            chat.remove_tab();
        }
        chat.remove_tab();
        assert_eq!((chat.tabs.len(), chat.tab), (1, 0));
        // Locked, add and remove aren't there.
        chat.locked = true;
        assert!(chat.button_boxes().iter().all(|(name, _)| !["chat_add_tab", "chat_remove_tab"].contains(name)));
    }

    #[test]
    fn pages_by_the_scrollbar() {
        let mut chat = fresh();
        for line in 0..20 {
            chat.push(format!("line {line}"), PUBLIC);
        }
        chat.layout(&|text: &str| text.len() as f32);
        let boxes = chat.button_boxes();
        let middle = |name: &str| {
            let [x, y, w, h] = boxes.iter().find(|(box_name, _)| *box_name == name).unwrap().1;
            [x + w / 2.0, y + h / 2.0]
        };
        chat.click(middle("chat_scroll_up"));
        assert_eq!(chat.tabs[0].scroll, 1);
        chat.click(middle("chat_scroll_down"));
        assert_eq!(chat.tabs[0].scroll, 0);
        assert!(chat.blocks(middle("chat_scroll_up")), "the scrollbar is the window's");
    }

    #[test]
    fn keeps_the_window_where_it_was() {
        let screen = [1024.0, 768.0];
        let mut chat = fresh();
        assert_eq!(chat.saved_text(), "- - 1 0");
        chat.cycle_size();
        chat.cycle_size();
        chat.corner = Some([100.0, 500.0]);
        chat.hidden = true;
        let text = chat.saved_text();
        assert_eq!(text, "100 500 7 1");
        assert_eq!(Chat::from_saved(&text, screen), Some((Some([100.0, 500.0]), 7, true)));
        // Off the screen at that step (the 180-tall log needs 180 above its bottom): back at the bottom left.
        assert_eq!(Chat::from_saved("100 100 7 0", screen), Some((None, 7, false)));
        assert_eq!(Chat::from_saved("900 500 1 0", screen), Some((None, 1, false)));
        assert_eq!((Chat::from_saved("- - 16 0", screen), Chat::from_saved("junk", screen)), (None, None));
    }

    #[test]
    fn saves_the_tabs_and_the_lock() {
        let mut tabs = default_tabs();
        tabs.push(Tab::new(Label::Text("Trade talk".into()), HashSet::from([Kind::Public, Kind::Whisper])));
        let text = tabs_text(&tabs, true);
        assert_eq!(text.lines().take(2).collect::<Vec<_>>(), ["v3", "locked 1"]);
        assert_eq!(text.lines().nth(3), Some("tab\t#1292\tbattle party_battle"));
        assert_eq!(text.lines().nth(4), Some("tab\tTrade talk\tpublic whisper"));
        assert_eq!(tabs_from(&text), Some((tabs, true)));
        // A tab with nothing ticked keeps its empty kinds.
        let empty = vec![Tab::new(Label::Text("NewTab_3".into()), HashSet::new())];
        assert_eq!(tabs_from(&tabs_text(&empty, false)), Some((empty, false)));
        // The two-tab file before names and the lock: the game's two tabs, unlocked.
        let old = format!("v2\n{}\nbattle", kinds_names(&default_kinds()[0]));
        assert_eq!(
            tabs_from(&old).map(|(tabs, locked)| (tabs.iter().map(|tab| tab.label.clone()).collect::<Vec<_>>(), locked)),
            Some((vec![Label::Message(1291), Label::Message(1292)], false))
        );
        // A file from before the game's 25 categories, or cut short, is dropped for the defaults.
        assert_eq!(tabs_from("public own\nbattle"), None);
        assert_eq!(tabs_from("v2\npublic"), None);
        assert_eq!(tabs_from("v3\nlocked 0"), None);
    }

    #[test]
    fn colors_party_chat() {
        assert_eq!([PARTY_OURS, PARTY], [[255, 200, 0], [255, 200, 200]]);
        assert_eq!([Kind::of_color(PARTY), Kind::of_color(GUILD), Kind::of_color([0xFF, 0, 0])], [Kind::Party, Kind::Guild, Kind::General]);
        assert_eq!(Kind::of_color(SKILL_MESSAGE), Kind::SkillFailure);
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
        assert_eq!(command("/shopping"), Some(Command::Shopping));
        assert_eq!(command("/sh"), Some(Command::Shopping));
        assert_eq!((command("/loginout"), command("/li")), (Some(Command::LoginOut), Some(Command::LoginOut)));
        assert_eq!((command("/notalkmsg2"), command("/nm2")), (Some(Command::NoGuildTalk), Some(Command::NoGuildTalk)));
        assert_eq!(command("/emblem"), Some(Command::Emblem));
        assert_eq!([command("/notrade"), command("/nt")], [Some(Command::NoTrade), Some(Command::NoTrade)]);
        assert_eq!([command("/noctrl"), command("/nc")], [Some(Command::NoCtrl), Some(Command::NoCtrl)]);
        assert_eq!([command("/skillfail"), command("/sf")], [Some(Command::SkillFail), Some(Command::SkillFail)]);
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
        assert_eq!([command("/cashshop"), command("/battleground")], [Some(Command::CashShop), Some(Command::BattleQueue)]);
        assert_eq!(command("/away"), Some(Command::Away));
        assert_eq!(command("/ex"), Some(Command::BlockList));
        assert_eq!(command("/ex Poring"), Some(Command::Whispers { name: "Poring".into(), allow: false }));
        assert_eq!(command("/in \"Big Poring\" "), Some(Command::Whispers { name: "Big Poring".into(), allow: true }));
        assert_eq!(command("/exall"), Some(Command::AllWhispers { allow: false }));
        assert_eq!(command("/inall"), Some(Command::AllWhispers { allow: true }));
        assert_eq!(command("/effect"), Some(Command::Effect));
        assert_eq!(command("/quake"), Some(Command::Quake));
        assert_eq!(command("/mineffect"), Some(Command::MinEffect));
        assert_eq!(command("/fog"), Some(Command::Fog));
        assert_eq!(command("/mable"), Some(Command::Mable));
        assert_eq!(command("/emo"), Some(Command::EmotionList));
        assert_eq!(command("/achievement"), Some(Command::Achievements));
        assert_eq!(command("/emotion"), Some(Command::EmotionList));
        for text in ["/in", "/whom", "hello /who", "/sit", "/ho"] {
            assert_eq!(command(text), None, "{text}");
        }
    }
}
