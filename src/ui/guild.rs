//! The guild window (Alt+G): its tabs, the guild's info with its emblem, its members, positions, skills, the expel
//! list and the notice. Also guild emblems as the server sends them.

use std::collections::HashMap;
use std::io::Read;

use image::RgbaImage;
use rustnarok_client::grf::Grf;
use rustnarok_client::lua::SkillInfo;
use rustnarok_client::net::{GuildInfo, GuildMember, GuildPosition, GuildRelation, Skill};
use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};
use crate::skills;
use crate::window::{self, Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI, contains, offset};

/// 400 × 317 for every tab: the title bar, the tab strip (0, 17, 400, 24), the page from y 41, the footer from y 288.
const WIDTH: f32 = 400.0;
const HEIGHT: f32 = 317.0;
const STRIP: [f32; 4] = [0.0, TITLE_HEIGHT, WIDTH, 24.0];
/// Six tab cells 64 × 23 every 66 px from (1, 18); labels in 62 × 15 boxes from (3, 22).
const TAB: [f32; 2] = [64.0, 23.0];
const TAB_STEP: f32 = 66.0;
const TAB_LABEL: [f32; 2] = [3.0, 22.0];
const TAB_LABEL_WIDTH: f32 = 62.0;
const STRIP_COLOR: [u8; 4] = [0xB4, 0xB4, 0xB4, 0xFF];
const EDGE: [u8; 4] = [0xC8, 0xC8, 0xC8, 0xFF];
const PAGE_TOP: f32 = 41.0;
const FOOTER: f32 = 29.0;
const LINE: f32 = 20.0;
/// Rows a list (members, positions, the expelled) shows at once; the wheel scrolls it.
const ROWS: usize = 11;
/// Where it first opens, on Guild Info, and again when its last place isn't all on the screen.
const POSITION: [f32; 2] = [0.0, 34.0];
/// The tabs a guild's menu flag must have (ZC_ACK_GUILD_MENUINTERFACE): Guild Info and Guild Skill always open.
const TAB_RIGHTS: [u32; 6] = [0, 0x01, 0x02, 0, 0x10, 0x80];
/// The menu flag until the server says it: every tab.
pub const MENU_UNKNOWN: i32 = -1;
/// The tabs' labels: info, members, positions, skills, expel list, notice.
const TABS: [usize; 6] = [340, 341, 342, 343, 344, 345];
pub const MEMBERS: usize = 1;
const POSITIONS: usize = 2;
const SKILLS: usize = 3;
const EXPELLED: usize = 4;
/// Info lines down the left from (8, 49), 16 px apart: name, level, master, members, average level, castles.
const LABELS: [usize; 6] = [328, 329, 330, 331, 332, 333];
const INFO: [f32; 2] = [8.0, 49.0];
const INFO_LINE: f32 = 16.0;
const MEMBERS_LINE: usize = 3;
/// The online count after the members line: the icon 16 past the line's width, 2 px higher.
const ONLINE_GAP: f32 = 16.0;
/// On the right: the exp (red from guild level 50), "Emblem", the emblem 1:1 in a #709FED box.
const EXP: [f32; 2] = [200.0, 49.0];
const EXP_LABEL: usize = 335;
const RED_EXP_LEVEL: u32 = 50;
const EMBLEM_LABEL: [f32; 2] = [200.0, 73.0];
const EMBLEM_LABEL_MESSAGE: usize = 336;
const EMBLEM_BOX: [f32; 4] = [299.0, 64.0, 26.0, 26.0];
const EMBLEM_BOX_COLOR: [u8; 4] = [0x70, 0x9F, 0xED, 0xFF];
const EMBLEM: [f32; 2] = [300.0, 65.0];
/// The master's Disband button on the info page.
const DISBAND_AT: [f32; 2] = [321.0, 292.0];
const DISBAND_SIZE: [f32; 2] = [64.0, 20.0];
pub const EMBLEM_SIZE: [f32; 2] = [24.0, 24.0];
/// The guardians' monster IDs: they carry their castle's guild emblem too.
const GUARDIANS: [u16; 3] = [1285, 1286, 1287];

/// Whether a unit carries its guild's emblem over its head on a siege map: other players in a guild and the guardians,
/// not while out of sight.
pub fn wears_head_emblem(player: bool, job: u16, guild: u32, cloaked: bool, ours: bool) -> bool {
    !ours && guild != 0 && !cloaked && (player || GUARDIANS.contains(&job))
}

/// The emblem's box over a unit (left, top, right, bottom): `floor(10·W/640)·p` left of and `floor(100·H/480)·p`
/// above its feet on screen, `p` its perspective factor (`perspective`); it doesn't grow with the zoom.
pub fn head_emblem_box(feet: [f32; 2], screen: [f32; 2], perspective: f32) -> [f32; 4] {
    let left = feet[0] - (10.0 * screen[0] / 640.0).floor() * perspective;
    let top = feet[1] - (100.0 * screen[1] / 480.0).floor() * perspective;
    [left, top, left + EMBLEM_SIZE[0], top + EMBLEM_SIZE[1]]
}
/// A unit's perspective factor from the screen pixels a sprite pixel takes where it stands, on a screen `height`
/// high: sprites are drawn 1.34·height/768 screen pixels a sprite pixel at a factor of 1.
pub fn perspective(screen_per_sprite: f32, height: f32) -> f32 {
    screen_per_sprite * 768.0 / (1.34 * height)
}

/// Member columns: name, position, job, level.
/// The member grid: 1-px #C8C8C8 lines, the outline, the header's rule, five rows 44 apart, the column lines; the
/// header's labels (Name, Position, Job, Level, Tax, Contribution) at y 51 and their cells (left edges, 6 to 381).
const GRID_LINE: [u8; 4] = [0xC8, 0xC8, 0xC8, 0xFF];
const GRID: [f32; 4] = [5.0, 46.0, 377.0, 241.0];
const GRID_HEADER: f32 = 66.0;
const MEMBER_ROW: f32 = 44.0;
const MEMBER_ROWS: usize = 5;
const GRID_COLUMNS: [f32; 5] = [104.0, 170.0, 224.0, 264.0, 304.0];
const COLUMNS: [(usize, f32); 6] = [(407, 10.0), (503, 109.0), (504, 175.0), (408, 229.0), (506, 269.0), (507, 309.0)];
const CELLS: [f32; 7] = [6.0, 104.0, 170.0, 224.0, 264.0, 304.0, 381.0];
const SORTED_FILL: [u8; 4] = [0xCE, 0xD8, 0xEB, 0xFF];
const ONLINE_BAND: [u8; 4] = [0xE2, 0xFF, 0xE4, 0xFF];
const DATE_TEXT: [u8; 3] = [0x40, 0x40, 0x40];
/// "Show guild member login status": the box and its label.
const LOGIN_STATUS: usize = 2864;
const LOGIN_BOX: ([f32; 2], [f32; 2]) = ([12.0, 299.0], [240.0, 12.0]);
const NAMELESS: usize = 581;
/// The access date's format and line.
const DATE_FORMAT: usize = 3011;
const ACCESS_DATE: usize = 3012;
/// Position columns: rank, title, invite, expel, storage, tax %.
/// The Position and Expel List tabs' cell grid: #C8C8C8 at (4, 45, 378, 229), white cells over it, 12 rows of 19 (the
/// header and 11), the columns' widths; the headers' labels at y 49.
const CELL_GRID: [f32; 4] = [4.0, 45.0, 378.0, 229.0];
const CELL_ROW: f32 = 19.0;
const LIST_ROWS: usize = 11;
const POSITION_WIDTHS: [f32; 6] = [50.0, 100.0, 60.0, 60.0, 60.0, 60.0];
const EXPEL_WIDTHS: [f32; 2] = [82.0, 292.0];
const POSITION_COLUMNS: [(usize, f32); 6] = [(510, 8.0), (511, 59.0), (512, 160.0), (513, 221.0), (2499, 282.0), (514, 343.0)];
/// The master's edits: the title box 96 × 16, the rights' 12 × 12 boxes, the tax box 26 × 16 (2 digits) and its "%".
const TITLE_BOX: [f32; 2] = [96.0, 16.0];
const TAX_BOX: [f32; 2] = [26.0, 16.0];
const TITLE_MOST: usize = 30;
const FIELD_FILL: [u8; 4] = [0xF2, 0xF2, 0xF2, 0xFF];
/// The Announcement tab: "Title:" and "Contents", the subject box (60 characters), the text box (119), the count, Send.
const NOTICE_SUBJECT: ([f32; 2], [f32; 2]) = ([50.0, 66.0], [340.0, 16.0]);
const NOTICE_TEXT: ([f32; 2], [f32; 2]) = ([5.0, 104.0], [390.0, 156.0]);
const NOTICE_SEND: [f32; 2] = [354.0, 293.0];
const SUBJECT_MOST: usize = 60;
const TEXT_MOST: usize = 119;
pub const NOTICE: usize = 5;
/// What the invite, expel and storage columns check.
const PERMISSIONS: [u32; 3] = [GuildPosition::INVITE, GuildPosition::EXPEL, GuildPosition::STORAGE];
/// Expel list columns: name, reason.
const EXPEL_COLUMNS: [(usize, f32); 2] = [(407, 8.0), (462, 91.0)];
/// The info page's allies and antagonists, stacked on its right: each heading, its box under it, and three 16 px
/// rows in it (rathena's default max_guild_alliance).
const RELATION_HEADINGS: [usize; 2] = [338, 339];
const RELATION_HEADING_TOP: [f32; 2] = [139.0, 213.0];
const RELATION_BOX_TOP: [f32; 2] = [155.0, 229.0];
const RELATION_LEFT: f32 = 200.0;
const RELATION_SIZE: [f32; 2] = [168.0, 48.0];
const RELATION_ROW_TOP: [f32; 2] = [157.0, 231.0];
const RELATION_ROW: f32 = 16.0;
const RELATION_NAME_LEFT: f32 = 204.0;
/// Where a right click finds a row: x 201 to 287.
const RELATION_HIT: [f32; 2] = [201.0, 287.0];
/// The selected row's highlight.
const RELATION_MARK: [f32; 2] = [80.0, 16.0];
const RELATION_ROWS: usize = 3;
/// The info lines on the left stop short of the boxes.
const INFO_RIGHT: f32 = 196.0;
/// The menu's hover color, on the selected relation.
// ponytail: the highlight's colour is a choice.
const HOVER: [u8; 4] = [0x73, 0x9C, 0xEF, 0xFF];
/// The result lines' colours.
const CYAN: [u8; 3] = [0x00, 0xFF, 0xFF];
const YELLOW: [u8; 3] = [0xFF, 0xFF, 0x00];
const RED: [u8; 3] = [0xFF, 0x00, 0x00];
/// An emblem's BMP is under 2 KB; more than this is no emblem.
const EMBLEM_BYTES: u64 = 64 * 1024;

pub struct Assets {
    footer: [Texture; 3],
    strip: Texture,
    edge: Texture,
    emblem_box: Texture,
    /// The Info tab's online icon.
    online_icon: Texture,
    /// The emblem's frame (over heads), and the position page's checkboxes (off, on).
    frame: Texture,
    check: [Texture; 2],
    hover: Texture,
    white: Texture,
    /// The member grid's lines, its online band and sorted header, and the sort arrows (up, down).
    grid: Texture,
    online: Texture,
    sorted: Texture,
    arrows: [Texture; 2],
    /// The drop-down button (normal, hover, pressed), OK, and the reason box's edit fill.
    drop: [Texture; 3],
    ok: [Texture; 3],
    edit: Texture,
    /// The skills page: the level up button, Use and Close, and the bands of a skill costing no SP and one not learned.
    level_up: [Texture; 3],
    use_button: [Texture; 3],
    close_button: [Texture; 3],
    no_sp: Texture,
    unlearned: Texture,
    /// The edit boxes' fill, and Send.
    field: Texture,
    send: [Texture; 3],
    disband: [Texture; 3],
}

impl Assets {
    /// The frame drawn behind an emblem.
    pub fn emblem_frame(&self) -> &Texture {
        &self.frame
    }

    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |name: &str| gpu.load_texture(grf, &format!("{UI}{name}.bmp"), false);
        Self {
            footer: ["left", "mid", "right"].map(|part| image(&format!("basic_interface\\btnbar_{part}2"))),
            strip: gpu.solid(STRIP_COLOR),
            edge: gpu.solid(EDGE),
            emblem_box: gpu.solid(EMBLEM_BOX_COLOR),
            online_icon: image("basic_interface\\grp_online"),
            frame: image("emblem_frame"),
            check: ["checkbox_0", "checkbox_1"].map(image),
            hover: gpu.solid(HOVER),
            white: gpu.solid([0xFF; 4]),
            grid: gpu.solid(GRID_LINE),
            online: gpu.solid(ONLINE_BAND),
            sorted: gpu.solid(SORTED_FILL),
            arrows: ["guild\\guild_arrow_up", "guild\\guild_arrow_down"].map(image),
            drop: ["basic_interface\\txtbox_btn_a", "basic_interface\\txtbox_btn_b", "basic_interface\\txtbox_btn_c"].map(image),
            ok: ["btn_ok", "btn_ok_a", "btn_ok_b"].map(image),
            edit: gpu.solid(EDIT_FILL),
            level_up: ["basic_interface\\skill_up_a", "basic_interface\\skill_up_b", "basic_interface\\skill_up_c"].map(image),
            use_button: ["btn_use", "btn_use_a", "btn_use_b"].map(image),
            close_button: ["btn_close", "btn_close_a", "btn_close_b"].map(image),
            no_sp: gpu.solid([0x70, 0xD7, 0xED, 0xFF]),
            unlearned: gpu.solid([0xB4, 0xB4, 0xB4, 0xFF]),
            field: gpu.solid(FIELD_FILL),
            send: ["btn_send", "btn_send_a", "btn_send_b"].map(image),
            disband: ["btn_disband", "btn_disband_a", "btn_disband_b"].map(image),
        }
    }
}

/// The player's guild.
#[derive(Default)]
pub struct Guild {
    pub id: u32,
    pub name: String,
    pub master: bool,
    /// The emblem's version (0: none).
    pub emblem: u32,
    pub info: Option<GuildInfo>,
    pub members: Vec<GuildMember>,
    pub positions: HashMap<u32, String>,
    /// Positions' permissions and tax.
    pub position_info: Vec<GuildPosition>,
    pub skills: Vec<Skill>,
    pub skill_points: u16,
    /// Who was expelled, and why.
    pub expelled: Vec<(String, String)>,
    pub notice: (String, String),
    /// Allied guilds and antagonists.
    pub relations: Vec<GuildRelation>,
    /// Our position's rights (ZC_UPDATE_GDID): invite 0x01, expel 0x10.
    pub mode: u32,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(TitleHit),
    Tab(usize),
    /// A guild skill's row, or its level up button; the skills page's Use and Close.
    Skill(u16),
    LevelUp(u16),
    SkillUse,
    SkillClose,
    /// An ally or an antagonist on the info page (a click selects it, the master's right click ends it): its guild ID,
    /// and whether it's an antagonist.
    Relation(u32, bool),
    /// The master's Disband on the info page.
    Disband,
    /// On the positions page, the master's: a permission box (position, bit), a title, a tax.
    Permission(u32, u32),
    PositionTitle(u32),
    Tax(u32),
    /// OK for the master's position edits; a row of the Position or Expel list; the Announcement tab's boxes and Send.
    PositionsOk,
    ListRow(usize),
    NoticeSubject,
    NoticeText,
    NoticeSend,
    /// A row of the members page, by its place in `Guild::members`.
    Member(usize),
    /// The members page: a header cell (sorting by its column), the login status box, anywhere else on the page.
    SortBy(usize),
    /// The master's drop-down on a member's row (by place in `members`), a position in its list, OK for the moves.
    Drop(usize),
    PickPosition(u32),
    MovesOk,
    LoginStatus,
    MemberPage,
    Window,
}

/// What the right-click menu on the selected member's row offers.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum MemberChoice {
    Leave,
    Expel,
    /// The master hands the guild over.
    Assign,
    PartyInvite,
    Whisper,
    Mail,
}

impl MemberChoice {
    pub fn message(self) -> usize {
        match self {
            Self::Leave => 508,
            Self::Expel => 509,
            Self::Assign => 2923,
            Self::PartyInvite => 1330,
            Self::Whisper => 2043,
            Self::Mail => 2808,
        }
    }
}

/// Colours of the guild's lines: red, cyan, yellow, and the notice's light yellow.
pub const LINE_RED: [u8; 3] = [0xFF, 0x00, 0x00];
pub const LINE_CYAN: [u8; 3] = [0x00, 0xFF, 0xFF];
pub const LINE_YELLOW: [u8; 3] = [0xFF, 0xFF, 0x00];
pub const NOTICE_LINE: [u8; 3] = [0xFF, 0xFF, 0x80];

/// The line for ZC_ACK_REQ_JOIN_GUILD's answer: 0 378, 1 379, 3 381, 4 2090 in red, 2 380 in cyan; others none.
pub fn invite_result_line(result: u8) -> Option<(usize, [u8; 3])> {
    Some(match result {
        0 => (378, LINE_RED),
        1 => (379, LINE_RED),
        2 => (380, LINE_CYAN),
        3 => (381, LINE_RED),
        4 => (2090, LINE_RED),
        _ => return None,
    })
}

/// The notice's lines: "[ subject ]" and "[ text ]", each when it isn't empty.
pub fn notice_lines(subject: &str, text: &str) -> Vec<String> {
    [subject, text].into_iter().filter(|part| !part.is_empty()).map(|part| format!("[ {part} ]")).collect()
}

/// Someone left or was expelled (ZC_ACK_LEAVE_GUILD, ZC_ACK_BAN_GUILD): each line's message, what fills its %s, its
/// colour; all guild lines. Us: "You left the guild." (489) or "You have been expelled
/// from the Guild." (490) in red; another (`name`, "Nameless" (581) when not in the list): their line (364 or 370)
/// and the reason's (365 or 371) in yellow.
pub fn leaving_lines(us: bool, expelled: bool, name: Option<&str>, reason: &str, nameless: &str) -> Vec<(usize, String, [u8; 3])> {
    if us {
        return vec![(if expelled { 490 } else { 489 }, String::new(), LINE_RED)];
    }
    let name = name.unwrap_or(nameless).to_owned();
    let (who, why) = if expelled { (370, 371) } else { (364, 365) };
    vec![(who, name, LINE_YELLOW), (why, reason.to_owned(), LINE_YELLOW)]
}

/// Whether the player menu offers the guild invitation: we hold the invite right, and the player is in no guild.
pub fn offers_invite(guild: Option<&Guild>, their_guild: u32) -> bool {
    guild.is_some_and(Guild::can_invite) && their_guild == 0
}

const INVITE_RIGHT: u32 = 0x01;
const EXPEL_RIGHT: u32 = 0x10;

impl Guild {
    pub fn can_invite(&self) -> bool {
        self.master || self.mode & INVITE_RIGHT != 0
    }

    pub fn can_expel(&self) -> bool {
        self.master || self.mode & EXPEL_RIGHT != 0
    }

    /// The menu on a member (by its place in `members`): our own row, leaving (not for the master); another's,
    /// expelling (the master, or the expel right on anyone but the master) and, for the master, handing the guild
    /// over; while they're online, asking them into our party (`invite_party`: we lead one they aren't in) and 1:1
    /// chat; last, a mail.
    pub fn member_menu(&self, me: u32, index: usize, invite_party: bool) -> Vec<MemberChoice> {
        let Some(member) = self.members.get(index) else { return Vec::new() };
        if member.character == me {
            return if self.master { Vec::new() } else { vec![MemberChoice::Leave] };
        }
        let mut choices = Vec::new();
        if self.master || (self.can_expel() && member.position != 0) {
            choices.push(MemberChoice::Expel);
        }
        if self.master {
            choices.push(MemberChoice::Assign);
        }
        if member.online {
            if invite_party {
                choices.push(MemberChoice::PartyInvite);
            }
            choices.push(MemberChoice::Whisper);
        }
        choices.push(MemberChoice::Mail);
        choices
    }

    /// A position's permissions and tax as known (none known: nothing set).
    pub fn position(&self, id: u32) -> GuildPosition {
        self.position_info.iter().find(|position| position.id == id).cloned().unwrap_or(GuildPosition { id, ..Default::default() })
    }

    pub fn title(&self, id: u32) -> String {
        self.positions.get(&id).cloned().unwrap_or_default()
    }

    /// ZC_ACK_CHANGE_GUILD_POSITIONINFO: the position's title, permissions and tax, added if not known yet.
    pub fn change_position(&mut self, position: GuildPosition, title: String) {
        self.positions.insert(position.id, title);
        match self.position_info.iter_mut().find(|known| known.id == position.id) {
            Some(known) => *known = position,
            None => {
                self.position_info.push(position);
                self.position_info.sort_by_key(|position| position.id);
            }
        }
    }

    /// ZC_ACK_REQ_CHANGE_MEMBERS: a member's new position.
    pub fn move_member(&mut self, character: u32, position: u32) {
        if let Some(member) = self.members.iter_mut().find(|member| member.character == character) {
            member.position = position;
        }
    }
}

/// The info page's columns: allies, then antagonists, the first rows of each.
fn columns(relations: &[GuildRelation]) -> [Vec<&GuildRelation>; 2] {
    [false, true].map(|antagonist| relations.iter().filter(|relation| relation.antagonist == antagonist).take(RELATION_ROWS).collect())
}

/// A relation row's corner and size, where a click finds it (column 0 allies, 1 antagonists).
fn relation_box(column: usize, row: usize) -> ([f32; 2], [f32; 2]) {
    ([RELATION_HIT[0], RELATION_ROW_TOP[column] + row as f32 * RELATION_ROW], [RELATION_HIT[1] - RELATION_HIT[0], RELATION_ROW])
}

/// An alliance request's result: its line, and its colour (cyan when allied). rathena's numbers are told as they come.
pub fn ally_result(result: u8) -> (usize, [u8; 3]) {
    let line = if result <= 4 { 394 + usize::from(result) } else { 1717 };
    (line, if result == 2 { CYAN } else { YELLOW })
}

/// Setting an antagonist: its line, and its colour (cyan when set).
pub fn antagonist_result(result: u8) -> (usize, [u8; 3]) {
    match result {
        0 => (495, CYAN),
        1 | 2 => (495 + usize::from(result), RED),
        _ => (1718, RED),
    }
}

/// What a reason box asks for: leaving, or expelling a member (account, character).
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Reasoned {
    Leave,
    Expel(u32, u32),
}

/// The reason box (Leave Guild, Expel): 280 × 80, its label, an edit box of up to 39 characters, OK and Cancel.
#[derive(Debug, PartialEq, Clone)]
pub struct ReasonBox {
    pub kind: Reasoned,
    pub text: String,
}

const REASON_SIZE: [f32; 2] = [280.0, 80.0];
const REASON_LABEL: [f32; 2] = [15.0, 6.0];
const REASON_EDIT: ([f32; 2], [f32; 2]) = ([15.0, 22.0], [248.0, 16.0]);
const REASON_OK: [f32; 2] = [179.0, 48.0];
const REASON_CANCEL: [f32; 2] = [225.0, 48.0];
const REASON_BUTTON: [f32; 2] = [42.0, 20.0];
const REASON_MOST: usize = 39;
const EDIT_FILL: [u8; 4] = [0xE8, 0xE8, 0xE8, 0xFF];

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum ReasonHit {
    Ok,
    Cancel,
    Box,
}

impl ReasonBox {
    pub fn new(kind: Reasoned) -> Self {
        Self { kind, text: String::new() }
    }

    /// Its corner: ((W − 640)/2 + 200, 120·H/480).
    pub fn at(screen: [f32; 2]) -> [f32; 2] {
        [(screen[0] - 640.0) / 2.0 + 200.0, 120.0 * screen[1] / 480.0]
    }

    pub fn hit(&self, cursor: [f32; 2], screen: [f32; 2]) -> Option<ReasonHit> {
        let at = Self::at(screen);
        if contains(offset(at, REASON_OK), REASON_BUTTON, cursor) {
            Some(ReasonHit::Ok)
        } else if contains(offset(at, REASON_CANCEL), REASON_BUTTON, cursor) {
            Some(ReasonHit::Cancel)
        } else {
            contains(at, REASON_SIZE, cursor).then_some(ReasonHit::Box)
        }
    }

    pub fn type_text(&mut self, text: &str) {
        let room = REASON_MOST.saturating_sub(self.text.chars().count());
        self.text.extend(text.chars().filter(|character| !character.is_control()).take(room));
    }

    /// "Please enter the reason of Withdrawal." or "... of Expulsion."
    fn label(&self) -> usize {
        if self.kind == Reasoned::Leave { 523 } else { 524 }
    }

    /// OK and Cancel, for the test driver.
    pub fn button_boxes(&self, screen: [f32; 2]) -> Vec<(&'static str, [f32; 4])> {
        let at = Self::at(screen);
        [("ok", REASON_OK), ("cancel", REASON_CANCEL)]
            .map(|(name, place)| {
                let [x, y] = offset(at, place);
                (name, [x, y, REASON_BUTTON[0], REASON_BUTTON[1]])
            })
            .to_vec()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &self,
        layer: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        (ok, cancel): (&'a [Texture; 3], &'a [Texture; 3]),
        font: &Font,
        messages: &[String],
        screen: [f32; 2],
        pointer: ([f32; 2], bool),
        caret: bool,
    ) {
        let at = Self::at(screen);
        layer.sprites.push((&chrome.white, at, REASON_SIZE));
        for (corner, size) in [
            ([0.0, 0.0], [REASON_SIZE[0], 1.0]),
            ([0.0, REASON_SIZE[1] - 1.0], [REASON_SIZE[0], 1.0]),
            ([0.0, 0.0], [1.0, REASON_SIZE[1]]),
            ([REASON_SIZE[0] - 1.0, 0.0], [1.0, REASON_SIZE[1]]),
        ] {
            layer.sprites.push((&chrome.border, offset(at, corner), size));
        }
        let label = messages.get(self.label()).map_or("", String::as_str).to_owned();
        layer.texts.push((label, offset(at, REASON_LABEL), [0, 0, 0]));
        let (edit, size) = REASON_EDIT;
        layer.sprites.push((&assets.edit, offset(at, edit), size));
        let shown = window::fit(font, &self.text, size[0] - 6.0);
        let caret_x = font.width(&shown);
        layer.texts.push((shown, offset(at, [edit[0] + 3.0, edit[1] + 1.0]), [0, 0, 0]));
        if caret {
            layer.sprites.push((&chrome.border, offset(at, [edit[0] + 3.0 + caret_x, edit[1] + 2.0]), [1.0, 12.0]));
        }
        let hit = self.hit(pointer.0, screen);
        let look = |target: ReasonHit| usize::from(hit == Some(target)) + usize::from(hit == Some(target) && pointer.1);
        layer.sprites.push((&ok[look(ReasonHit::Ok)], offset(at, REASON_OK), REASON_BUTTON));
        layer.sprites.push((&cancel[look(ReasonHit::Cancel)], offset(at, REASON_CANCEL), REASON_BUTTON));
    }
}

/// The master's position drop-downs on the member rows: the button (18 × 18 at (150, t + 12)), the list (150 wide,
/// 16 a row, at most 80 tall, at (88, t + 34)), and OK (42 × 20 at (339, 294)) while moves are pending; OK rests
/// 400 ms after it sends.
const DROP_BUTTON: [f32; 2] = [18.0, 18.0];
const DROP_LIST_WIDTH: f32 = 150.0;
const DROP_ROW: f32 = 16.0;
const DROP_ROWS: usize = 5;
const MOVES_OK: ([f32; 2], [f32; 2]) = ([339.0, 294.0], [42.0, 20.0]);
pub const MOVES_REST: std::time::Duration = std::time::Duration::from_millis(400);

fn drop_button(row: usize) -> [f32; 2] {
    [150.0, 67.0 + row as f32 * MEMBER_ROW + 12.0]
}

fn drop_list(row: usize) -> [f32; 2] {
    [88.0, 67.0 + row as f32 * MEMBER_ROW + 34.0]
}

/// A position picked for a member: the last pick for them wins.
pub fn pick_move(pending: &mut Vec<(u32, u32, u32)>, account: u32, character: u32, position: u32) {
    pending.retain(|&(_, each, _)| each != character);
    pending.push((account, character, position));
}

/// The guild window's text box with the caret.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum GuildTyping {
    Title(u32),
    Tax(u32),
    NoticeSubject,
    NoticeText,
}

pub struct GuildWindow {
    pub frame: Frame,
    pub tab: usize,
    /// Rows of the open list scrolled past.
    scroll: usize,
    /// The relation selected on the info page (guild ID, antagonist).
    pub selected: Option<(u32, bool)>,
    /// The members page: the member selected (by character), the column sorted by and whether up, and whether
    /// online members come first.
    pub member: Option<u32>,
    pub sort: Option<(usize, bool)>,
    pub online_first: bool,
    /// The master's pending moves (account, character, position), the drop-down open (its member, by place in
    /// `members`, and rows scrolled), and when OK may send again.
    pub pending: Vec<(u32, u32, u32)>,
    pub drop: Option<(usize, usize)>,
    pub moves_rest_until: Option<std::time::Instant>,
    /// The skill selected on the skills page.
    pub skill: Option<u16>,
    /// The master's pending position edits (each position as edited, and its title), when OK may send them again,
    /// the list row selected, the box being typed in, and the notice being written (subject, text).
    pub position_edits: Vec<(GuildPosition, String)>,
    pub positions_rest_until: Option<std::time::Instant>,
    pub list_row: Option<usize>,
    pub typing: Option<GuildTyping>,
    pub notice_draft: (String, String),
    /// Which tabs open, by the guild's menu flag (MENU_UNKNOWN until the server says).
    pub menu: i32,
}

impl Default for GuildWindow {
    fn default() -> Self {
        Self {
            frame: Frame::new(POSITION, WIDTH),
            tab: 0,
            scroll: 0,
            selected: None,
            member: None,
            sort: None,
            online_first: false,
            pending: Vec::new(),
            drop: None,
            moves_rest_until: None,
            skill: None,
            position_edits: Vec::new(),
            positions_rest_until: None,
            list_row: None,
            typing: None,
            notice_draft: (String::new(), String::new()),
            menu: MENU_UNKNOWN,
        }
    }
}

/// The members in the order shown: the packet's (rathena: the master first), or by a column, up or down; online
/// members first while asked, each group keeping that order. ponytail: Position sorts by rank, Job by job ID, Tax keeps
/// the order (the list carries no tax).
pub fn member_order(members: &[GuildMember], sort: Option<(usize, bool)>, online_first: bool) -> Vec<usize> {
    let mut order: Vec<usize> = (0..members.len()).collect();
    if let Some((column, up)) = sort {
        order.sort_by(|&a, &b| {
            let [a, b] = [&members[a], &members[b]];
            let by = match column {
                0 => a.name.cmp(&b.name),
                1 => a.position.cmp(&b.position),
                2 => a.job.cmp(&b.job),
                3 => a.level.cmp(&b.level),
                5 => a.contribution.cmp(&b.contribution),
                _ => std::cmp::Ordering::Equal,
            };
            if up { by } else { by.reverse() }
        });
    }
    if online_first {
        order.sort_by_key(|&index| !members[index].online);
    }
    order
}

/// Every position's ID in order from 0, for the drop-down.
fn position_ids(guild: &Guild) -> Vec<u32> {
    let mut ids: Vec<u32> = guild.positions.keys().copied().collect();
    ids.sort_unstable();
    ids
}

/// The member grid's header cell under a window-local point.
fn header_cell(local: [f32; 2]) -> Option<usize> {
    if !(47.0..GRID_HEADER).contains(&local[1]) {
        return None;
    }
    (0..6).find(|&column| (CELLS[column]..CELLS[column + 1]).contains(&local[0]))
}

/// The row of the member grid under a window-local point (5 rows from y 66, x 5–382).
fn grid_row(local: [f32; 2]) -> Option<usize> {
    let inside = (GRID[0]..GRID[0] + GRID[2]).contains(&local[0]) && (GRID_HEADER..GRID_HEADER + MEMBER_ROW * MEMBER_ROWS as f32).contains(&local[1]);
    inside.then(|| ((local[1] - GRID_HEADER) / MEMBER_ROW) as usize)
}

/// A day's date from Unix time (UTC) in a strftime-style template: `%y`, `%m`, `%d`, `%Y`.
// ponytail: UTC; the client's local date would need the time zone.
pub fn format_date(unix: u32, template: &str) -> String {
    let days = i64::from(unix) / 86_400;
    // Civil date from days since 1970-01-01.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    template
        .replace("%Y", &format!("{year:04}"))
        .replace("%y", &format!("{:02}", year.rem_euclid(100)))
        .replace("%m", &format!("{month:02}"))
        .replace("%d", &format!("{day:02}"))
}

/// A contribution's colour: red at the most it can hold (2147483647), else black.
pub fn contribution_color(contribution: u32) -> [u8; 3] {
    if contribution >= i32::MAX as u32 { RED } else { [0, 0, 0] }
}

/// The key a member's head is loaded under in the icons.
pub fn head_key(member: &GuildMember) -> String {
    format!("guildhead:{}:{}:{}:{}", member.job, member.sex, member.hair, member.hair_color)
}

/// The skills page: one column of rows 34 apart (7 show), the selected row's band (104, 43 + y, 164 × 28), the icon at
/// (44, 45 + y), the name at (112, 46 + y), "Lv : n" at (113, 58 + y) and the SP at (212, 58 + y); the master's level up
/// button at (78, 45 + y); Use and Close at (46, 293) and (92, 293); "SkillPoint : n" at (300, 297).
const SKILL_ROW: f32 = 34.0;
const SKILL_ROWS: usize = 7;
const SKILL_BAND: ([f32; 2], [f32; 2]) = ([104.0, 43.0], [164.0, 28.0]);
const SKILL_ICON: [f32; 2] = [44.0, 45.0];
const SKILL_NAME: [f32; 2] = [112.0, 46.0];
const SKILL_LEVEL: [f32; 2] = [113.0, 58.0];
const SKILL_SP: [f32; 2] = [212.0, 58.0];
const SKILL_UP: [f32; 2] = [78.0, 45.0];
const SKILL_USE: [f32; 2] = [46.0, 293.0];
const SKILL_CLOSE: [f32; 2] = [92.0, 293.0];
const SKILL_BUTTON: [f32; 2] = [42.0, 20.0];
const SKILL_POINT_AT: [f32; 2] = [300.0, 297.0];
const PASSIVE: usize = 286;
const UNLEARNED_NAME: [u8; 3] = [0x78, 0x78, 0x78];
/// The right click's one entry, "View Skill Info".
pub const VIEW_SKILL_INFO: usize = 322;

/// The selected row's band: an active skill's, one costing no SP, one not learned.
pub fn skill_band(skill: &Skill) -> [u8; 4] {
    if skill.level == 0 {
        [0xB4, 0xB4, 0xB4, 0xFF]
    } else if skill.sp == 0 {
        [0x70, 0xD7, 0xED, 0xFF]
    } else {
        [0x70, 0x9F, 0xED, 0xFF]
    }
}

/// The SP line once learned: "Sp : n", or "Passive" (286) for a skill that targets nothing.
pub fn skill_sp_text(skill: &Skill, passive: &str) -> String {
    if skill.kind == 0 { passive.to_owned() } else { format!("Sp : {}", skill.sp) }
}

/// The row a press picks on the skills page, before scrolling: rows on a 32-px pitch from y 41.
fn skill_row_at(local: [f32; 2]) -> Option<usize> {
    (local[1] >= 41.0).then(|| ((local[1] - 41.0) / 32.0) as usize).filter(|&row| row < SKILL_ROWS)
}

/// Whether the menu flag opens a tab.
fn tab_allowed(tab: usize, menu: i32) -> bool {
    TAB_RIGHTS.get(tab).is_some_and(|&bit| bit == 0 || menu as u32 & bit != 0)
}

/// The tab a press picks: x / 66 on the strip (17 ≤ y ≤ 41), not the open one.
fn tab_at(local: [f32; 2], open: usize) -> Option<usize> {
    let tab = (local[0] / TAB_STEP).floor();
    ((TITLE_HEIGHT..=PAGE_TOP).contains(&local[1]) && (0.0..TABS.len() as f32).contains(&tab) && tab as usize != open).then_some(tab as usize)
}

/// Whether a window `size` big at `place` is all on the screen.
fn on_screen(place: [f32; 2], size: [f32; 2], screen: [f32; 2]) -> bool {
    place[0] >= 0.0 && place[1] >= 0.0 && place[0] + size[0] <= screen[0] && place[1] + size[1] <= screen[1]
}

/// The members line: the members listed of the most.
fn members_line(label: &str, listed: usize, most: u32) -> String {
    format!("{label} : {listed} / {most}")
}

/// Where the online icon goes after the members line, by that line's width.
fn online_icon_x(line_width: f32) -> f32 {
    line_width + ONLINE_GAP
}

/// The exp line's colour: red from guild level 50.
fn exp_color(level: u32) -> [u8; 3] {
    if level >= RED_EXP_LEVEL { RED } else { [0, 0, 0] }
}

/// A guild emblem from the server: a zlib-compressed BMP, its magenta keyed out.
pub fn decode_emblem(data: &[u8]) -> Option<RgbaImage> {
    let mut bmp = Vec::new();
    flate2::read::ZlibDecoder::new(data).take(EMBLEM_BYTES).read_to_end(&mut bmp).ok()?;
    decode_emblem_file(&bmp)
}

/// An emblem as the web server gives it: the BMP file itself.
// ponytail: BMP only; rathena's web server also keeps GIF emblems, which need the image crate's gif feature.
pub fn decode_emblem_file(bmp: &[u8]) -> Option<RgbaImage> {
    let image = image::load_from_memory_with_format(bmp, image::ImageFormat::Bmp).ok()?;
    Some(crate::render::key_out(image.to_rgba8()))
}

/// Downloads a guild's emblem from rathena's web server (`/emblem/download`), for servers that keep emblems there
/// rather than answer CZ_REQ_GUILD_EMBLEM_IMG. It waits on the network: call it off the frame.
pub fn download_emblem(address: &str, auth: &crate::booking::Auth, guild: u32) -> std::io::Result<Vec<u8>> {
    crate::booking::post(address, "/emblem/download", &emblem_fields(auth, guild))
}

fn emblem_fields(auth: &crate::booking::Auth, guild: u32) -> [(&'static str, String); 4] {
    [("AID", auth.account.to_string()), ("AuthToken", auth.token.clone()), ("WorldName", auth.world.clone()), ("GDID", guild.to_string())]
}

/// A cell of the grid (column, row 0 the header): (5 + Σ(w + 1), 46 + 19·row, w, 18), kept inside the grid.
fn cell(widths: &[f32], column: usize, row: usize) -> [f32; 4] {
    let x = 5.0 + widths[..column].iter().map(|width| width + 1.0).sum::<f32>();
    let right = CELL_GRID[0] + CELL_GRID[2] - 1.0;
    [x, CELL_GRID[1] + 1.0 + CELL_ROW * row as f32, widths[column].min(right - x).max(0.0), CELL_ROW - 1.0]
}

/// A list row's text line: y = 68 + 19·k.
fn list_text_y(row: usize) -> f32 {
    68.0 + CELL_ROW * row as f32
}

/// The list row under a window-local point, before scrolling.
fn list_row_at(local: [f32; 2]) -> Option<usize> {
    let top = CELL_GRID[1] + 1.0 + CELL_ROW;
    let inside = (5.0..381.0).contains(&local[0]) && (top..top + CELL_ROW * LIST_ROWS as f32).contains(&local[1]);
    inside.then(|| ((local[1] - top) / CELL_ROW) as usize)
}

/// A position row's parts the master edits: the title box, the three rights' boxes, the tax box.
fn position_parts(row: usize) -> [(PositionPart, [f32; 2], [f32; 2]); 5] {
    let y = list_text_y(row);
    [
        (PositionPart::Title, [58.0, y - 2.0], TITLE_BOX),
        (PositionPart::Permission(PERMISSIONS[0]), [160.0, y], [12.0, 12.0]),
        (PositionPart::Permission(PERMISSIONS[1]), [221.0, y], [12.0, 12.0]),
        (PositionPart::Permission(PERMISSIONS[2]), [282.0, y], [12.0, 12.0]),
        (PositionPart::Tax, [343.0, y - 2.0], TAX_BOX),
    ]
}

#[derive(Clone, Copy)]
enum PositionPart {
    Title,
    Permission(u32),
    Tax,
}

impl PositionPart {
    fn hit(self, id: u32) -> Hit {
        match self {
            PositionPart::Title => Hit::PositionTitle(id),
            PositionPart::Permission(bit) => Hit::Permission(id, bit),
            PositionPart::Tax => Hit::Tax(id),
        }
    }

    /// Its target name for the test driver.
    fn name(self, id: u32) -> String {
        match self {
            PositionPart::Title => format!("title:{id}"),
            PositionPart::Permission(GuildPosition::INVITE) => format!("invite:{id}"),
            PositionPart::Permission(GuildPosition::EXPEL) => format!("expel:{id}"),
            PositionPart::Permission(_) => format!("storage:{id}"),
            PositionPart::Tax => format!("tax:{id}"),
        }
    }
}

impl GuildWindow {
    pub fn height(&self) -> f32 {
        HEIGHT
    }

    /// Whether a skill's row has the level up button: the master's, with points left, on an upgradable skill.
    fn levels_up(&self, guild: &Guild, skill: &Skill) -> bool {
        guild.master && guild.skill_points > 0 && skill.upgradable
    }

    /// Alt+G or the menu's guild button: open on the last tab at the last place, unless that place isn't all on the
    /// screen: then at (0, 34) on Guild Info.
    pub fn open(&mut self, screen: [f32; 2]) {
        self.frame.open = true;
        if !on_screen(self.frame.position, [WIDTH, HEIGHT], screen) {
            self.frame.position = POSITION;
            self.show_tab(0);
        }
    }

    pub fn show_tab(&mut self, tab: usize) {
        self.tab = tab;
        self.scroll = 0;
        self.typing = None;
        self.list_row = None;
    }

    /// A click on the members page: a row selects its member, elsewhere clears; a header cell sorts by its column
    /// (up, then flipped by the same column again); the box puts online members first.
    pub fn click_members(&mut self, hit: Hit, guild: &Guild) {
        match hit {
            Hit::Member(index) => self.member = guild.members.get(index).map(|member| member.character),
            Hit::SortBy(column) => {
                self.sort = Some(match self.sort {
                    Some((sorted, up)) if sorted == column => (column, !up),
                    _ => (column, true),
                });
            }
            Hit::LoginStatus => self.online_first = !self.online_first,
            Hit::MemberPage => self.member = None,
            Hit::Drop(index) => self.drop = if self.drop.is_some_and(|(open, _)| open == index) { None } else { Some((index, 0)) },
            Hit::PickPosition(position) => {
                if let Some(member) = self.drop.take().and_then(|(index, _)| guild.members.get(index)) {
                    pick_move(&mut self.pending, member.account, member.character, position);
                }
            }
            _ => {}
        }
        if !matches!(hit, Hit::Drop(_) | Hit::PickPosition(_)) {
            self.drop = None;
        }
    }

    /// The cell grid: grey under it, white cells, 12 rows.
    fn draw_grid<'a>(&self, layer: &mut Layer<'a>, assets: &'a Assets, widths: &[f32]) {
        let at = |local: [f32; 2]| self.frame.at(local);
        layer.sprites.push((&assets.grid, at([CELL_GRID[0], CELL_GRID[1]]), [CELL_GRID[2], CELL_GRID[3]]));
        for row in 0..=LIST_ROWS {
            for column in 0..widths.len() {
                let [x, y, w, h] = cell(widths, column, row);
                layer.sprites.push((&assets.white, at([x, y]), [w, h]));
            }
        }
    }

    /// A position as edited so far: the pending edit, else as known.
    pub fn shown_position(&self, guild: &Guild, id: u32) -> (GuildPosition, String) {
        self.position_edits.iter().find(|(position, _)| position.id == id).cloned().unwrap_or_else(|| (guild.position(id), guild.title(id)))
    }

    /// The pending edit of a position, made from it as known the first time.
    fn edit(&mut self, guild: &Guild, id: u32) -> &mut (GuildPosition, String) {
        if !self.position_edits.iter().any(|(position, _)| position.id == id) {
            self.position_edits.push((guild.position(id), guild.title(id)));
        }
        self.position_edits.iter_mut().find(|(position, _)| position.id == id).unwrap()
    }

    /// The master's click on a position's part: a right's box switches it, a box takes the caret.
    pub fn click_position(&mut self, hit: Hit, guild: &Guild) {
        match hit {
            Hit::Permission(id, bit) => self.edit(guild, id).0.mode ^= bit,
            Hit::PositionTitle(id) => {
                self.edit(guild, id);
                self.typing = Some(GuildTyping::Title(id));
            }
            Hit::Tax(id) => {
                self.edit(guild, id);
                self.typing = Some(GuildTyping::Tax(id));
            }
            Hit::ListRow(row) => self.list_row = Some(row),
            _ => {}
        }
    }

    /// Typing into the box with the caret: a title (30 characters), a tax (2 digits), the notice's subject (60) or
    /// text (119).
    pub fn type_text(&mut self, guild: &Guild, typed: &str) {
        let typed = typed.chars().filter(|character| !character.is_control());
        let push = |into: &mut String, most: usize, typed: &mut dyn Iterator<Item = char>| {
            let room = most.saturating_sub(into.chars().count());
            into.extend(typed.take(room));
        };
        match self.typing {
            Some(GuildTyping::Title(id)) => push(&mut self.edit(guild, id).1, TITLE_MOST, &mut typed.into_iter()),
            Some(GuildTyping::Tax(id)) => {
                let entry = self.edit(guild, id);
                let mut digits = entry.0.tax.to_string();
                if digits == "0" {
                    digits.clear();
                }
                push(&mut digits, 2, &mut typed.filter(char::is_ascii_digit));
                entry.0.tax = digits.parse().unwrap_or(0);
            }
            Some(GuildTyping::NoticeSubject) => push(&mut self.notice_draft.0, SUBJECT_MOST, &mut typed.into_iter()),
            Some(GuildTyping::NoticeText) => push(&mut self.notice_draft.1, TEXT_MOST, &mut typed.into_iter()),
            None => {}
        }
    }

    pub fn backspace(&mut self, guild: &Guild) {
        match self.typing {
            Some(GuildTyping::Title(id)) => drop(self.edit(guild, id).1.pop()),
            Some(GuildTyping::Tax(id)) => {
                let entry = self.edit(guild, id);
                entry.0.tax /= 10;
            }
            Some(GuildTyping::NoticeSubject) => drop(self.notice_draft.0.pop()),
            Some(GuildTyping::NoticeText) => drop(self.notice_draft.1.pop()),
            None => {}
        }
    }

    /// OK for the position edits: all of them to send, unless OK is resting.
    pub fn take_position_edits(&mut self, now: std::time::Instant) -> Option<Vec<(GuildPosition, String)>> {
        if self.position_edits.is_empty() || self.positions_rest_until.is_some_and(|until| now < until) {
            return None;
        }
        self.positions_rest_until = Some(now + MOVES_REST);
        self.typing = None;
        Some(std::mem::take(&mut self.position_edits))
    }

    /// OK for the pending moves (or Enter): all of them to send, unless OK is resting.
    pub fn take_moves(&mut self, now: std::time::Instant) -> Option<Vec<(u32, u32, u32)>> {
        if self.pending.is_empty() || self.moves_rest_until.is_some_and(|until| now < until) {
            return None;
        }
        self.moves_rest_until = Some(now + MOVES_REST);
        Some(std::mem::take(&mut self.pending))
    }

    pub fn hit(&self, cursor: [f32; 2], guild: &Guild) -> Option<Hit> {
        let local = self.frame.local(cursor, HEIGHT)?;
        // The title bar has the close button only.
        match self.frame.title_hit(local) {
            Some(TitleHit::Minimize) => return Some(Hit::Title(TitleHit::Title)),
            Some(hit) => return Some(Hit::Title(hit)),
            None => {}
        }
        if let Some(tab) = tab_at(local, self.tab).filter(|&tab| tab_allowed(tab, self.menu)) {
            return Some(Hit::Tab(tab));
        }
        if self.tab == 0 && guild.master && contains(DISBAND_AT, DISBAND_SIZE, local) {
            return Some(Hit::Disband);
        }
        if self.tab == 0 {
            for (column, relations) in columns(&guild.relations).into_iter().enumerate() {
                for (row, relation) in relations.into_iter().enumerate() {
                    let (corner, size) = relation_box(column, row);
                    if contains(corner, size, local) {
                        return Some(Hit::Relation(relation.id, relation.antagonist));
                    }
                }
            }
        }
        if self.tab == POSITIONS && guild.master {
            for (row, position) in guild.position_info.iter().skip(self.scroll).take(LIST_ROWS).enumerate() {
                if let Some((part, ..)) = position_parts(row).into_iter().find(|(_, corner, size)| contains(*corner, *size, local)) {
                    return Some(part.hit(position.id));
                }
            }
            if !self.position_edits.is_empty() && contains(MOVES_OK.0, MOVES_OK.1, local) {
                return Some(Hit::PositionsOk);
            }
        }
        if self.tab == POSITIONS || self.tab == EXPELLED {
            let count = if self.tab == POSITIONS { guild.position_info.len() } else { guild.expelled.len() };
            if let Some(row) = list_row_at(local).map(|row| row + self.scroll).filter(|&row| row < count) {
                return Some(Hit::ListRow(row));
            }
        }
        if self.tab == NOTICE && guild.master {
            for (hit, (corner, size)) in [(Hit::NoticeSubject, NOTICE_SUBJECT), (Hit::NoticeText, NOTICE_TEXT), (Hit::NoticeSend, (NOTICE_SEND, MOVES_OK.1))] {
                if contains(corner, size, local) {
                    return Some(hit);
                }
            }
        }
        if self.tab == MEMBERS {
            let order = member_order(&guild.members, self.sort, self.online_first);
            if guild.master {
                // The open list first: it covers the rows under it.
                if let Some((index, scroll)) = self.drop
                    && let Some(row) = order.iter().skip(self.scroll).take(MEMBER_ROWS).position(|&shown| shown == index)
                {
                    let ids = position_ids(guild);
                    let shown = ids.len().saturating_sub(scroll).min(DROP_ROWS);
                    let corner = drop_list(row);
                    if contains(corner, [DROP_LIST_WIDTH, shown as f32 * DROP_ROW], local) {
                        let picked = ((local[1] - corner[1]) / DROP_ROW) as usize + scroll;
                        return ids.get(picked).map(|&id| Hit::PickPosition(id));
                    }
                }
                for (row, &index) in order.iter().skip(self.scroll).take(MEMBER_ROWS).enumerate() {
                    if contains(drop_button(row), DROP_BUTTON, local) {
                        return Some(Hit::Drop(index));
                    }
                }
                if !self.pending.is_empty() && contains(MOVES_OK.0, MOVES_OK.1, local) {
                    return Some(Hit::MovesOk);
                }
            }
            if let Some(column) = header_cell(local) {
                return Some(Hit::SortBy(column));
            }
            if contains(LOGIN_BOX.0, LOGIN_BOX.1, local) {
                return Some(Hit::LoginStatus);
            }
            if let Some(&index) = grid_row(local).and_then(|row| order.get(self.scroll + row)) {
                return Some(Hit::Member(index));
            }
            if local[1] > PAGE_TOP && local[1] < HEIGHT - FOOTER {
                return Some(Hit::MemberPage);
            }
        }
        if self.tab == SKILLS {
            for (row, skill) in guild.skills.iter().skip(self.scroll).take(SKILL_ROWS).enumerate() {
                if self.levels_up(guild, skill) && contains(offset(SKILL_UP, [0.0, row as f32 * SKILL_ROW]), [24.0, 24.0], local) {
                    return Some(Hit::LevelUp(skill.id));
                }
            }
            if contains(SKILL_USE, SKILL_BUTTON, local) {
                return Some(Hit::SkillUse);
            }
            if contains(SKILL_CLOSE, SKILL_BUTTON, local) {
                return Some(Hit::SkillClose);
            }
            if let Some(skill) = skill_row_at(local).and_then(|row| guild.skills.get(row + self.scroll)) {
                return Some(Hit::Skill(skill.id));
            }
        }
        Some(Hit::Window)
    }

    /// The relations on the info page, named after their guilds.
    pub fn relation_boxes(&self, guild: &Guild) -> Vec<(String, [f32; 4])> {
        if !self.frame.open || self.frame.minimized || self.tab != 0 {
            return Vec::new();
        }
        let rows = columns(&guild.relations)
            .into_iter()
            .enumerate()
            .flat_map(|(column, relations)| relations.into_iter().enumerate().map(move |(row, relation)| (column, row, relation)));
        rows.map(|(column, row, relation)| {
            let (corner, size) = relation_box(column, row);
            let [x, y] = self.frame.at(corner);
            (relation.name.clone(), [x, y, size[0], size[1]])
        })
        .collect()
    }

    /// The positions page's parts (the master's) and the members page's rows, for the test driver: `title:<id>`,
    /// `invite:<id>`, `expel:<id>`, `storage:<id>`, `tax:<id>`, `member:<name>`.
    pub fn button_boxes(&self, guild: &Guild) -> Vec<(String, [f32; 4])> {
        if !self.frame.open || self.frame.minimized {
            return Vec::new();
        }
        let place = |corner: [f32; 2], size: [f32; 2]| {
            let [x, y] = self.frame.at(corner);
            [x, y, size[0], size[1]]
        };
        match self.tab {
            POSITIONS if guild.master => {
                let mut boxes: Vec<(String, [f32; 4])> = guild
                    .position_info
                    .iter()
                    .skip(self.scroll)
                    .take(LIST_ROWS)
                    .enumerate()
                    .flat_map(|(row, position)| position_parts(row).map(|(part, corner, size)| (part.name(position.id), place(corner, size))))
                    .collect();
                if !self.position_edits.is_empty() {
                    boxes.push(("positions_ok".to_owned(), place(MOVES_OK.0, MOVES_OK.1)));
                }
                boxes
            }
            // The Announcement tab: `notice_subject`, `notice_text`, `notice_send`.
            NOTICE if guild.master => vec![
                ("notice_subject".to_owned(), place(NOTICE_SUBJECT.0, NOTICE_SUBJECT.1)),
                ("notice_text".to_owned(), place(NOTICE_TEXT.0, NOTICE_TEXT.1)),
                ("notice_send".to_owned(), place(NOTICE_SEND, MOVES_OK.1)),
            ],
            // The info page: the master's `guild_disband`.
            0 if guild.master => vec![("guild_disband".to_owned(), place(DISBAND_AT, DISBAND_SIZE))],
            // The skills page: `skill:<id>` rows, `skill_up:<id>` buttons, `skill_use` and `skill_close`.
            SKILLS => {
                let mut boxes = Vec::new();
                for (row, skill) in guild.skills.iter().skip(self.scroll).take(SKILL_ROWS).enumerate() {
                    let y = row as f32 * SKILL_ROW;
                    boxes.push((format!("skill:{}", skill.id), place(offset(SKILL_BAND.0, [0.0, y]), SKILL_BAND.1)));
                    if self.levels_up(guild, skill) {
                        boxes.push((format!("skill_up:{}", skill.id), place(offset(SKILL_UP, [0.0, y]), [24.0, 24.0])));
                    }
                }
                boxes.push(("skill_use".to_owned(), place(SKILL_USE, SKILL_BUTTON)));
                boxes.push(("skill_close".to_owned(), place(SKILL_CLOSE, SKILL_BUTTON)));
                boxes
            }
            MEMBERS => {
                let mut boxes: Vec<(String, [f32; 4])> = member_order(&guild.members, self.sort, self.online_first)
                    .into_iter()
                    .skip(self.scroll)
                    .take(MEMBER_ROWS)
                    .enumerate()
                    .map(|(row, index)| {
                        let top = GRID_HEADER + 1.0 + row as f32 * MEMBER_ROW;
                        (format!("member:{}", guild.members[index].name), place([6.0, top], [375.0, MEMBER_ROW - 1.0]))
                    })
                    .collect();
                for (column, name) in ["name", "position", "job", "level", "tax", "contribution"].into_iter().enumerate() {
                    boxes.push((format!("sort:{name}"), place([CELLS[column], 47.0], [CELLS[column + 1] - CELLS[column], GRID_HEADER - 47.0])));
                }
                boxes.push(("login_status".to_owned(), place(LOGIN_BOX.0, LOGIN_BOX.1)));
                // The master's drop-downs (`drop:<name>`), the open list's positions (`position:<id>`), `moves_ok`.
                if guild.master {
                    let order = member_order(&guild.members, self.sort, self.online_first);
                    for (row, &index) in order.iter().skip(self.scroll).take(MEMBER_ROWS).enumerate() {
                        boxes.push((format!("drop:{}", guild.members[index].name), place(drop_button(row), DROP_BUTTON)));
                        if self.drop.is_some_and(|(open, _)| open == index) {
                            let scroll = self.drop.map_or(0, |(_, scroll)| scroll);
                            for (line, id) in position_ids(guild).into_iter().skip(scroll).take(DROP_ROWS).enumerate() {
                                let corner = offset(drop_list(row), [0.0, line as f32 * DROP_ROW]);
                                boxes.push((format!("position:{id}"), place(corner, [DROP_LIST_WIDTH, DROP_ROW])));
                            }
                        }
                    }
                    if !self.pending.is_empty() {
                        boxes.push(("moves_ok".to_owned(), place(MOVES_OK.0, MOVES_OK.1)));
                    }
                }
                boxes
            }
            _ => Vec::new(),
        }
    }

    /// The wheel scrolls the open list.
    pub fn scroll_by(&mut self, rows: i32, guild: &Guild) {
        // An open drop-down list scrolls first; the members page shows 5 rows.
        if self.tab == MEMBERS
            && let Some((index, scroll)) = self.drop
        {
            let most = position_ids(guild).len().saturating_sub(DROP_ROWS);
            self.drop = Some((index, (scroll as i32 + rows).clamp(0, most as i32) as usize));
            return;
        }
        if self.tab == MEMBERS {
            self.scroll = (self.scroll as i32 + rows).clamp(0, guild.members.len().saturating_sub(MEMBER_ROWS) as i32) as usize;
            return;
        }
        let total = match self.tab {
            MEMBERS => guild.members.len(),
            SKILLS => guild.skills.len().saturating_sub(SKILL_ROWS - ROWS.min(SKILL_ROWS)),
            POSITIONS => guild.position_info.len(),
            EXPELLED => guild.expelled.len(),
            _ => 0,
        };
        let shown = if matches!(self.tab, POSITIONS | EXPELLED) { LIST_ROWS } else { ROWS };
        self.scroll = (self.scroll as i32 + rows).clamp(0, total.saturating_sub(shown) as i32) as usize;
    }

    /// The skill icons the open page shows, to load before drawing.
    pub fn icons(&self, guild: &Guild, info: &HashMap<u16, SkillInfo>) -> Vec<String> {
        if !self.frame.open || self.tab != SKILLS {
            return Vec::new();
        }
        guild.skills.iter().filter_map(|skill| info.get(&skill.id).map(skills::icon_path)).collect()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &self,
        layer: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        icons: &'a HashMap<String, Option<Texture>>,
        font: &Font,
        messages: &[String],
        guild: &Guild,
        (job_names, skill_info): (&HashMap<u16, String>, &HashMap<u16, SkillInfo>),
        emblem: Option<&'a Texture>,
        pointer: ([f32; 2], bool),
    ) {
        if !self.frame.open {
            return;
        }
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        let at = |local: [f32; 2]| self.frame.at(local);
        // The title bar with no title, the close button alone (its key's tooltip is drawn with the other windows').
        self.frame.draw_bar(layer, chrome);
        self.frame.draw_bare_title(layer, chrome, "", pointer.0);
        let hit = self.hit(pointer.0, guild);
        // The tab strip: grey, its cells white for the open tab, light grey for the others; the labels left in them.
        // ponytail: each label left-aligned in its box.
        let [x, y, w, h] = STRIP;
        layer.sprites.push((&assets.strip, at([x, y]), [w, h]));
        for (tab, &label) in TABS.iter().enumerate() {
            let left = tab as f32 * TAB_STEP;
            let cell = if tab == self.tab { &assets.white } else { &assets.edge };
            layer.sprites.push((cell, at([left + 1.0, TITLE_HEIGHT + 1.0]), TAB));
            let text = window::fit(font, message(label), TAB_LABEL_WIDTH);
            layer.texts.push((text, at([left + TAB_LABEL[0], TAB_LABEL[1]]), [0, 0, 0]));
        }
        // The page: white with a grey edge left and right; the footer along the bottom.
        layer.sprites.push((&assets.edge, at([0.0, PAGE_TOP]), [WIDTH, HEIGHT - PAGE_TOP]));
        layer.sprites.push((&assets.white, at([1.0, PAGE_TOP]), [WIDTH - 2.0, HEIGHT - PAGE_TOP]));
        let [left, middle, right] = &assets.footer;
        let footer_top = HEIGHT - FOOTER;
        layer.sprites.push((left, at([0.0, footer_top]), left.size));
        layer.sprites.push((middle, at([left.size[0], footer_top]), [WIDTH - left.size[0] - right.size[0], FOOTER]));
        layer.sprites.push((right, at([WIDTH - right.size[0], footer_top]), right.size));

        let text = |layer: &mut Layer<'a>, value: String, place: [f32; 2], color: [u8; 3]| layer.texts.push((value, at(place), color));
        match self.tab {
            // Info: the guild's name, level, master, members (with the online count), levels and castles down the left;
            // its exp and emblem on the right.
            0 => {
                if guild.master {
                    let look = if hit == Some(Hit::Disband) { 1 + usize::from(pointer.1) } else { 0 };
                    layer.sprites.push((&assets.disband[look], at(DISBAND_AT), DISBAND_SIZE));
                }
                let info = guild.info.clone().unwrap_or_default();
                let values =
                    [guild.name.clone(), info.level.to_string(), info.master.clone(), String::new(), info.average_level.to_string(), info.territory.clone()];
                for (row, (label, value)) in LABELS.iter().zip(values).enumerate() {
                    let top = INFO[1] + row as f32 * INFO_LINE;
                    let line = if row == MEMBERS_LINE {
                        members_line(message(*label), guild.members.len(), info.max_members)
                    } else {
                        format!("{} : {value}", message(*label))
                    };
                    let line = window::fit(font, &line, INFO_RIGHT - INFO[0]);
                    if row == MEMBERS_LINE {
                        let icon = online_icon_x(font.width(&line));
                        layer.sprites.push((&assets.online_icon, at([icon, top - 2.0]), assets.online_icon.size));
                        text(layer, format!(" {}", info.members), [icon + assets.online_icon.size[0], top], [0, 0, 0]);
                    }
                    text(layer, line, [INFO[0], top], [0, 0, 0]);
                }
                text(layer, format!("{} : {}", message(EXP_LABEL), info.exp), EXP, exp_color(info.level));
                text(layer, message(EMBLEM_LABEL_MESSAGE).to_owned(), EMBLEM_LABEL, [0, 0, 0]);
                let [x, y, w, h] = EMBLEM_BOX;
                layer.sprites.push((&assets.emblem_box, at([x, y]), [w, h]));
                if let Some(emblem) = emblem {
                    layer.sprites.push((emblem, at(EMBLEM), EMBLEM_SIZE));
                }
                // Allies, then antagonists: each heading over a grey box with its rows; the selected one marked.
                for (column, relations) in columns(&guild.relations).into_iter().enumerate() {
                    text(layer, message(RELATION_HEADINGS[column]).to_owned(), [RELATION_LEFT, RELATION_HEADING_TOP[column]], [0, 0, 0]);
                    let (left, top) = (RELATION_LEFT, RELATION_BOX_TOP[column]);
                    let w = RELATION_SIZE[0];
                    layer.sprites.push((&assets.edge, at([left, top]), RELATION_SIZE));
                    for (row, relation) in relations.into_iter().enumerate() {
                        let top = RELATION_ROW_TOP[column] + row as f32 * RELATION_ROW;
                        if self.selected == Some((relation.id, relation.antagonist)) {
                            layer.sprites.push((&assets.hover, at([RELATION_NAME_LEFT, top]), RELATION_MARK));
                        }
                        let name = window::fit(font, &relation.name, left + w - 2.0 - RELATION_NAME_LEFT);
                        text(layer, name, [RELATION_NAME_LEFT, top + 2.0], [0, 0, 0]);
                    }
                }
            }
            // Members: the grid, the sorted column's header, five rows (the online band and head, name, position, job,
            // level, tax, contribution, the access date), and the login status box.
            MEMBERS => {
                if let Some((column, up)) = self.sort {
                    layer.sprites.push((&assets.sorted, at([CELLS[column], 47.0]), [CELLS[column + 1] - CELLS[column], 19.0]));
                    let (label, x) = COLUMNS[column];
                    let arrow = &assets.arrows[usize::from(!up)];
                    layer.sprites.push((arrow, at([x + font.width(message(label)) + 2.0, 53.0]), [7.0, 6.0]));
                }
                for &(label, x) in &COLUMNS {
                    text(layer, message(label).to_owned(), [x, 51.0], [0, 0, 0]);
                }
                let order = member_order(&guild.members, self.sort, self.online_first);
                for (row, &index) in order.iter().skip(self.scroll).take(MEMBER_ROWS).enumerate() {
                    let member = &guild.members[index];
                    let top = 67.0 + row as f32 * MEMBER_ROW;
                    if member.online {
                        layer.sprites.push((&assets.online, at([6.0, top]), [375.0, 43.0]));
                    }
                    if self.member == Some(member.character) {
                        layer.sprites.push((&assets.hover, at([6.0, top]), [375.0, 43.0]));
                    }
                    if member.online
                        && let Some(Some(head)) = icons.get(&head_key(member))
                    {
                        layer.sprites.push((head, at([((44.0 - head.size[0]) / 2.0).floor() + 6.0, top + 3.0]), head.size));
                    }
                    let name = if member.name.is_empty() { message(NAMELESS).to_owned() } else { member.name.clone() };
                    text(layer, window::fit(font, &name, 51.0), [43.0, top + 12.0], [0, 0, 0]);
                    // A pending move shows the position picked. ponytail: the picked name stands where the position is.
                    let shown_position =
                        self.pending.iter().find(|&&(_, character, _)| character == member.character).map_or(member.position, |&(.., position)| position);
                    let position = guild.positions.get(&shown_position).cloned().unwrap_or_default();
                    text(layer, window::fit(font, &position, 49.0), [109.0, top + 16.0], [0, 0, 0]);
                    let job = job_names.get(&member.job).cloned().unwrap_or_else(|| member.job.to_string());
                    text(layer, window::fit(font, &job, 39.0), [175.0, top + 16.0], [0, 0, 0]);
                    let right = |layer: &mut Layer<'a>, value: String, edge: f32, color: [u8; 3]| {
                        let x = edge - font.width(&value);
                        text(layer, value, [x, top + 16.0], color);
                    };
                    right(layer, member.level.to_string(), 260.0, [0, 0, 0]);
                    // ponytail: the member list carries no tax.
                    right(layer, format!("{:3.0}%", 0.0), 300.0, [0, 0, 0]);
                    right(layer, member.contribution.to_string(), 377.0, contribution_color(member.contribution));
                    let date = format_date(member.last_login, message(DATE_FORMAT));
                    let line = rustnarok_client::messages::fill(message(ACCESS_DATE), &[&date]);
                    text(layer, line, [8.0, top + 33.0], DATE_TEXT);
                }
                // The grid over the bands.
                let [x, y, w, h] = GRID;
                for (corner, size) in [([x, y], [w, 1.0]), ([x, y + h], [w + 1.0, 1.0]), ([x, y], [1.0, h]), ([x + w, y], [1.0, h])] {
                    layer.sprites.push((&assets.grid, at(corner), size));
                }
                for k in 0..=MEMBER_ROWS {
                    layer.sprites.push((&assets.grid, at([x, GRID_HEADER + MEMBER_ROW * k as f32]), [w, 1.0]));
                }
                for column in GRID_COLUMNS {
                    layer.sprites.push((&assets.grid, at([column, y]), [1.0, h]));
                }
                // The master's drop-downs, the open list, and OK while moves are pending.
                if guild.master {
                    for (row, &index) in order.iter().skip(self.scroll).take(MEMBER_ROWS).enumerate() {
                        let look = match hit {
                            Some(Hit::Drop(over)) if over == index => 1 + usize::from(pointer.1),
                            _ => 0,
                        };
                        layer.sprites.push((&assets.drop[look], at(drop_button(row)), DROP_BUTTON));
                    }
                    if let Some((index, scroll)) = self.drop
                        && let Some(row) = order.iter().skip(self.scroll).take(MEMBER_ROWS).position(|&shown| shown == index)
                    {
                        let ids = position_ids(guild);
                        let corner = drop_list(row);
                        let shown = ids.len().saturating_sub(scroll).min(DROP_ROWS);
                        layer.sprites.push((&chrome.border, at(corner), [DROP_LIST_WIDTH, shown as f32 * DROP_ROW]));
                        layer.sprites.push((&assets.white, at(offset(corner, [1.0, 1.0])), [DROP_LIST_WIDTH - 2.0, shown as f32 * DROP_ROW - 2.0]));
                        for (line, id) in ids.iter().skip(scroll).take(DROP_ROWS).enumerate() {
                            let top = corner[1] + line as f32 * DROP_ROW;
                            if hit == Some(Hit::PickPosition(*id)) {
                                layer.sprites.push((&assets.hover, at([corner[0] + 1.0, top]), [DROP_LIST_WIDTH - 2.0, DROP_ROW]));
                            }
                            let name = guild.positions.get(id).cloned().unwrap_or_default();
                            text(layer, window::fit(font, &name, DROP_LIST_WIDTH - 8.0), [corner[0] + 4.0, top + 1.0], [0, 0, 0]);
                        }
                    }
                    if !self.pending.is_empty() {
                        let look = if hit == Some(Hit::MovesOk) { 1 + usize::from(pointer.1) } else { 0 };
                        layer.sprites.push((&assets.ok[look], at(MOVES_OK.0), MOVES_OK.1));
                    }
                }
                let check = &assets.check[usize::from(self.online_first)];
                layer.sprites.push((check, at(LOGIN_BOX.0), check.size));
                text(layer, message(LOGIN_STATUS).to_owned(), [LOGIN_BOX.0[0] + 14.0, LOGIN_BOX.0[1] - 1.0], [0, 0, 0]);
            }
            // Positions: the cell grid; rank, title, rights and tax; the master edits them in place, OK sends.
            POSITIONS => {
                self.draw_grid(layer, assets, &POSITION_WIDTHS);
                for &(label, x) in &POSITION_COLUMNS {
                    text(layer, message(label).to_owned(), [x, 49.0], [0, 0, 0]);
                }
                for (row, known) in guild.position_info.iter().skip(self.scroll).take(LIST_ROWS).enumerate() {
                    let y = list_text_y(row);
                    if self.list_row == Some(row + self.scroll) {
                        layer.sprites.push((&assets.hover, at([5.0, y - 3.0]), [376.0, 18.0]));
                    }
                    let (position, title) = self.shown_position(guild, known.id);
                    text(layer, position.rank.to_string(), [8.0, y], [0, 0, 0]);
                    if guild.master {
                        let [(_, title_at, _), rights @ .., (_, tax_at, _)] = position_parts(row);
                        layer.sprites.push((&assets.field, at(title_at), TITLE_BOX));
                        text(layer, window::fit(font, &title, TITLE_BOX[0] - 4.0), [title_at[0] + 2.0, y - 2.0], [0, 0, 0]);
                        for (part, corner, _) in rights {
                            let PositionPart::Permission(bit) = part else { continue };
                            layer.sprites.push((&assets.check[usize::from(position.mode & bit != 0)], at(corner), [12.0, 12.0]));
                        }
                        layer.sprites.push((&assets.field, at(tax_at), TAX_BOX));
                        text(layer, position.tax.to_string(), [tax_at[0] + 2.0, y - 2.0], [0, 0, 0]);
                        text(layer, "%".to_owned(), [373.0, y], [0, 0, 0]);
                    } else {
                        text(layer, window::fit(font, &title, 96.0), [59.0, y - 2.0], [0, 0, 0]);
                        for (bit, x) in PERMISSIONS.iter().zip([160.0, 221.0, 282.0]) {
                            layer.sprites.push((&assets.check[usize::from(position.mode & bit != 0)], at([x, y]), [10.0, 10.0]));
                        }
                        text(layer, format!("{} %", position.tax), [343.0, y - 2.0], [0, 0, 0]);
                    }
                }
                if guild.master && !self.position_edits.is_empty() {
                    let look = if hit == Some(Hit::PositionsOk) { 1 + usize::from(pointer.1) } else { 0 };
                    layer.sprites.push((&assets.ok[look], at(MOVES_OK.0), MOVES_OK.1));
                }
            }
            // Skills where they sit, the master's level up buttons, and the points left on the footer.
            SKILLS => {
                let passive = message(PASSIVE);
                for (row, skill) in guild.skills.iter().skip(self.scroll).take(SKILL_ROWS).enumerate() {
                    let y = row as f32 * SKILL_ROW;
                    let down = |place: [f32; 2]| offset(place, [0.0, y]);
                    let info = skill_info.get(&skill.id);
                    if self.skill == Some(skill.id) {
                        let band = match skill_band(skill) {
                            [0x70, 0x9F, ..] => &assets.hover,
                            [0x70, 0xD7, ..] => &assets.no_sp,
                            _ => &assets.unlearned,
                        };
                        layer.sprites.push((band, at(down(SKILL_BAND.0)), SKILL_BAND.1));
                    }
                    if let Some(Some(icon)) = info.map(skills::icon_path).and_then(|path| icons.get(&path)) {
                        layer.sprites.push((icon, at(down(SKILL_ICON)), [24.0, 24.0]));
                    }
                    let name = info.map_or_else(|| skill.id.to_string(), |info| info.name.clone());
                    let color = if skill.level == 0 { UNLEARNED_NAME } else { [0, 0, 0] };
                    text(layer, window::fit(font, &name, WIDTH - SKILL_NAME[0] - 8.0), down(SKILL_NAME), color);
                    if skill.level > 0 {
                        text(layer, format!("Lv : {}", skill.level), down(SKILL_LEVEL), [0, 0, 0]);
                        text(layer, skill_sp_text(skill, passive), down(SKILL_SP), [0, 0, 0]);
                    }
                    if self.levels_up(guild, skill) {
                        let state = if hit == Some(Hit::LevelUp(skill.id)) { 1 + usize::from(pointer.1) } else { 0 };
                        layer.sprites.push((&assets.level_up[state], at(down(SKILL_UP)), [24.0, 24.0]));
                    }
                }
                for (place, images, which) in [(SKILL_USE, &assets.use_button, Hit::SkillUse), (SKILL_CLOSE, &assets.close_button, Hit::SkillClose)] {
                    let look = if hit == Some(which) { 1 + usize::from(pointer.1) } else { 0 };
                    layer.sprites.push((&images[look], at(place), SKILL_BUTTON));
                }
                text(layer, format!("SkillPoint : {}", guild.skill_points), SKILL_POINT_AT, [0, 0, 0]);
            }
            // The expel list: the cell grid, who and why.
            EXPELLED => {
                self.draw_grid(layer, assets, &EXPEL_WIDTHS);
                for &(label, x) in &EXPEL_COLUMNS {
                    text(layer, message(label).to_owned(), [x, 49.0], [0, 0, 0]);
                }
                for (row, (name, reason)) in guild.expelled.iter().skip(self.scroll).take(LIST_ROWS).enumerate() {
                    let y = list_text_y(row);
                    if self.list_row == Some(row + self.scroll) {
                        layer.sprites.push((&assets.hover, at([5.0, y - 3.0]), [376.0, 18.0]));
                    }
                    text(layer, window::fit(font, name, EXPEL_WIDTHS[0] - 4.0), [8.0, y], [0, 0, 0]);
                    text(layer, window::fit(font, reason, EXPEL_WIDTHS[1] - 4.0), [91.0, y], [0, 0, 0]);
                }
            }
            // The Announcement tab (the master's): the subject and text being written, the count, Send.
            _ => {
                text(layer, format!("{}:", message(515).trim_end_matches(':').trim()), [5.0, 67.0], [0, 0, 0]);
                text(layer, message(516).to_owned(), [5.0, 86.0], [0, 0, 0]);
                let (subject, body) = &self.notice_draft;
                let caret = |layer: &mut Layer<'a>, at_x: f32, at_y: f32| layer.sprites.push((&chrome.border, at([at_x, at_y]), [1.0, 12.0]));
                layer.sprites.push((&assets.field, at(NOTICE_SUBJECT.0), NOTICE_SUBJECT.1));
                text(layer, window::fit(font, subject, NOTICE_SUBJECT.1[0] - 6.0), offset(NOTICE_SUBJECT.0, [3.0, 1.0]), [0, 0, 0]);
                if self.typing == Some(GuildTyping::NoticeSubject) {
                    caret(layer, NOTICE_SUBJECT.0[0] + 3.0 + font.width(subject).min(NOTICE_SUBJECT.1[0] - 6.0), NOTICE_SUBJECT.0[1] + 2.0);
                }
                layer.sprites.push((&assets.field, at(NOTICE_TEXT.0), NOTICE_TEXT.1));
                let lines = window::wrap(font, body, NOTICE_TEXT.1[0] - 8.0, 10);
                for (line, words) in lines.iter().enumerate() {
                    text(layer, words.clone(), offset(NOTICE_TEXT.0, [4.0, 3.0 + line as f32 * LINE]), [0, 0, 0]);
                }
                if self.typing == Some(GuildTyping::NoticeText) {
                    let last = lines.len().saturating_sub(1);
                    let width = lines.last().map_or(0.0, |line| font.width(line));
                    caret(layer, NOTICE_TEXT.0[0] + 4.0 + width, NOTICE_TEXT.0[1] + 4.0 + last as f32 * LINE);
                }
                let count = format!("[{}/{TEXT_MOST}]", body.chars().count());
                text(layer, count.clone(), [393.0 - font.width(&count), 266.0], [0, 0, 0]);
                if guild.master {
                    let look = if hit == Some(Hit::NoticeSend) { 1 + usize::from(pointer.1) } else { 0 };
                    layer.sprites.push((&assets.send[look], at(NOTICE_SEND), MOVES_OK.1));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downloads_an_emblem_from_the_web_server() {
        use std::io::{Read, Write};
        // A 2x1 BMP: one red pixel, one magenta (keyed out).
        let mut bmp = image::RgbImage::new(2, 1);
        bmp.put_pixel(0, 0, image::Rgb([255, 0, 0]));
        bmp.put_pixel(1, 0, image::Rgb([255, 0, 255]));
        let mut file = std::io::Cursor::new(Vec::new());
        bmp.write_to(&mut file, image::ImageFormat::Bmp).unwrap();
        let file = file.into_inner();

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let reply = file.clone();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut chunk = [0; 4096];
            while !String::from_utf8_lossy(&request).contains("--rustnarok-booking--") {
                let read = stream.read(&mut chunk).unwrap();
                request.extend(&chunk[..read]);
            }
            let mut answer = format!("HTTP/1.1 200 OK\r\nContent-Type: image/bmp\r\nContent-Length: {}\r\n\r\n", reply.len()).into_bytes();
            answer.extend(&reply);
            stream.write_all(&answer).unwrap();
            String::from_utf8_lossy(&request).into_owned()
        });
        let auth = crate::booking::Auth { account: 2000001, token: "0123456789abcdef".into(), world: "rAthena".into() };
        let got = download_emblem(&address, &auth, 7).unwrap();
        let request = server.join().unwrap();
        assert!(request.starts_with("POST /emblem/download HTTP/1.1\r\n"));
        for (name, value) in [("AID", "2000001"), ("AuthToken", "0123456789abcdef"), ("WorldName", "rAthena"), ("GDID", "7")] {
            assert!(request.contains(&format!("name=\"{name}\"\r\n\r\n{value}\r\n")), "{name}");
        }
        assert_eq!(got, file, "the BMP's bytes as sent");
        let image = decode_emblem_file(&got).unwrap();
        assert_eq!((image.get_pixel(0, 0).0, image.get_pixel(1, 0).0[3]), ([255, 0, 0, 255], 0));
        // A guild the server has no emblem for (404) is an error, and the map server's copy stays.
        assert!(crate::booking::response_body(b"HTTP/1.1 404 Not Found\r\n\r\nError").is_err());
    }

    fn member(name: &str, level: u16, online: bool, contribution: u32) -> GuildMember {
        GuildMember { name: name.into(), level, online, contribution, character: u32::from(level), ..Default::default() }
    }

    #[test]
    fn sorts_selects_and_dates_the_members() {
        let members = vec![member("Master", 50, true, 10), member("Ann", 20, false, 30), member("Bob", 90, true, 20), member("Cid", 70, false, 0)];
        assert_eq!(member_order(&members, None, false), [0, 1, 2, 3], "the packet's order");
        assert_eq!(member_order(&members, Some((0, true)), false), [1, 2, 3, 0]);
        assert_eq!(member_order(&members, Some((3, true)), false), [1, 0, 3, 2]);
        assert_eq!(member_order(&members, Some((3, false)), false), [2, 3, 0, 1]);
        assert_eq!(member_order(&members, Some((5, false)), false), [1, 2, 0, 3]);
        // Online first keeps each group in the column's order.
        assert_eq!(member_order(&members, Some((3, false)), true), [2, 0, 3, 1]);
        assert_eq!(member_order(&members, None, true), [0, 2, 1, 3]);
        // A press: header cells, rows, and the rest.
        assert_eq!(header_cell([10.0, 50.0]), Some(0));
        assert_eq!(header_cell([230.0, 60.0]), Some(3));
        assert_eq!(header_cell([310.0, 66.0]), None);
        assert_eq!((grid_row([20.0, 67.0]), grid_row([20.0, 109.0]), grid_row([20.0, 110.0]), grid_row([381.0, 285.0])), (Some(0), Some(0), Some(1), Some(4)));
        assert_eq!((grid_row([20.0, 287.0]), grid_row([2.0, 100.0])), (None, None));
        let guild = Guild { members: members.clone(), ..Guild::default() };
        let mut window = GuildWindow { tab: MEMBERS, ..GuildWindow::default() };
        window.frame.open = true;
        let corner = window.frame.position;
        let at = move |local: [f32; 2]| [corner[0] + local[0], corner[1] + local[1]];
        assert_eq!(window.hit(at([20.0, 115.0]), &guild), Some(Hit::Member(1)));
        assert_eq!(window.hit(at([230.0, 55.0]), &guild), Some(Hit::SortBy(3)));
        assert_eq!(window.hit(at([20.0, 300.0]), &guild), Some(Hit::LoginStatus));
        window.click_members(Hit::SortBy(3), &guild);
        window.click_members(Hit::SortBy(3), &guild);
        assert_eq!(window.sort, Some((3, false)), "the same column again flips it");
        assert_eq!(window.hit(at([20.0, 70.0]), &guild), Some(Hit::Member(2)), "the highest level first");
        window.click_members(Hit::Member(2), &guild);
        assert_eq!(window.member, Some(90));
        window.click_members(Hit::MemberPage, &guild);
        assert_eq!(window.member, None);
        window.click_members(Hit::SortBy(0), &guild);
        assert_eq!(window.sort, Some((0, true)), "a new column sorts up");
        // The access date and the contribution's colour.
        assert_eq!(format_date(1_792_368_000, "%y.%m.%d"), "26.10.19");
        assert_eq!(format_date(0, "%Y-%m-%d"), "1970-01-01");
        assert_eq!(format_date(951_782_400, "%y.%m.%d"), "00.02.29");
        assert_eq!((contribution_color(2_147_483_647), contribution_color(5)), (RED, [0, 0, 0]));
    }

    #[test]
    fn picks_tabs_across_the_top() {
        // x / 66 from y 17 to 41, the open tab left alone, nothing past the sixth.
        assert_eq!(tab_at([70.0, 17.0], 0), Some(1));
        assert_eq!(tab_at([395.0, 41.0], 0), Some(5));
        assert_eq!(tab_at([10.0, 30.0], 0), None);
        assert_eq!(tab_at([397.0, 30.0], 0), None);
        assert_eq!(tab_at([70.0, 42.0], 0), None);
        let mut window = GuildWindow::default();
        window.frame.open = true;
        let guild = Guild::default();
        let at = |local: [f32; 2]| window.frame.at(local);
        assert_eq!(window.hit(at([66.0 * 1.5, TITLE_HEIGHT + 5.0]), &guild), Some(Hit::Tab(1)));
        assert_eq!(window.hit(at([200.0, 200.0]), &guild), Some(Hit::Window));
        // The title bar: no minimize button, the close button at (386, 3).
        assert_eq!(window.hit(at([373.0, 5.0]), &guild), Some(Hit::Title(TitleHit::Title)));
        assert_eq!(window.hit(at([388.0, 5.0]), &guild), Some(Hit::Title(TitleHit::Close)));
        // A member (0x57) can't open Announcement; the master (0xd7) can; unknown opens all.
        window.menu = 0x57;
        assert_eq!(window.hit(at([66.0 * 5.5, 30.0]), &guild), Some(Hit::Window));
        let rights = |menu| (0..6).map(|tab| tab_allowed(tab, menu)).collect::<Vec<_>>();
        assert_eq!(rights(0x57), [true, true, true, true, true, false]);
        assert_eq!(rights(0xd7), [true; 6]);
        assert_eq!(rights(MENU_UNKNOWN), [true; 6]);
        assert_eq!(rights(0), [true, false, false, true, false, false]);
    }

    #[test]
    fn opens_where_it_was_unless_that_is_off_the_screen() {
        let mut window = GuildWindow::default();
        assert_eq!(window.frame.position, [0.0, 34.0]);
        window.frame.position = [500.0, 300.0];
        window.show_tab(POSITIONS);
        window.open([1024.0, 768.0]);
        assert_eq!((window.frame.position, window.tab), ([500.0, 300.0], POSITIONS));
        window.frame.open = false;
        // At 800 × 600 it would stick out: back to (0, 34) on Guild Info.
        window.open([800.0, 600.0]);
        assert_eq!((window.frame.position, window.tab), ([0.0, 34.0], 0));
        assert!(!on_screen([-1.0, 34.0], [WIDTH, HEIGHT], [1024.0, 768.0]));
    }

    #[test]
    fn writes_the_members_and_exp_lines() {
        assert_eq!(members_line("Members", 3, 16), "Members : 3 / 16");
        assert_eq!(online_icon_x(100.0), 116.0);
        assert_eq!([49, 50, 51].map(exp_color), [[0, 0, 0], RED, RED]);
    }

    fn crab() -> Guild {
        let member =
            |character: u32, name: &str, position: u32| GuildMember { account: character + 1000, character, position, name: name.into(), ..Default::default() };
        Guild {
            master: true,
            members: vec![member(1, "Boss", 0), member(2, "Newbie", 19)],
            positions: (0..20).map(|id| (id, format!("Position {}", id + 1))).collect(),
            position_info: (0..20).map(|id| GuildPosition { id, mode: 0, rank: id, tax: 0 }).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn edits_positions_in_the_cell_grid() {
        // The cells: Position's widths 50, 100, 60…; Expel List's 82, 292; 19-px rows from y 46.
        assert_eq!(cell(&POSITION_WIDTHS, 0, 0), [5.0, 46.0, 50.0, 18.0]);
        assert_eq!(cell(&POSITION_WIDTHS, 1, 2), [56.0, 84.0, 100.0, 18.0]);
        assert_eq!(cell(&POSITION_WIDTHS, 2, 1)[0], 157.0);
        assert_eq!(cell(&POSITION_WIDTHS, 5, 0), [340.0, 46.0, 41.0, 18.0], "kept inside the grid");
        assert_eq!(cell(&EXPEL_WIDTHS, 1, 11), [88.0, 255.0, 292.0, 18.0]);
        assert_eq!((list_text_y(0), list_text_y(10)), (68.0, 258.0));
        assert_eq!([64.0, 66.0, 83.0, 85.0, 273.0, 275.0].map(|y| list_row_at([100.0, y])), [None, Some(0), Some(0), Some(1), Some(10), None]);
        let mut window = GuildWindow::default();
        window.frame.open = true;
        window.show_tab(POSITIONS);
        let mut guild = crab();
        guild.position_info[1].mode = GuildPosition::STORAGE;
        guild.position_info[1].tax = 5;
        let corner = window.frame.position;
        let at = move |local: [f32; 2]| [corner[0] + local[0], corner[1] + local[1]];
        // The second row is position 1; the master's parts are its boxes.
        let y = list_text_y(1);
        assert_eq!(window.hit(at([60.0, y]), &guild), Some(Hit::PositionTitle(1)));
        assert_eq!(window.hit(at([165.0, y + 4.0]), &guild), Some(Hit::Permission(1, GuildPosition::INVITE)));
        assert_eq!(window.hit(at([226.0, y + 4.0]), &guild), Some(Hit::Permission(1, GuildPosition::EXPEL)));
        assert_eq!(window.hit(at([287.0, y + 4.0]), &guild), Some(Hit::Permission(1, GuildPosition::STORAGE)));
        assert_eq!(window.hit(at([350.0, y]), &guild), Some(Hit::Tax(1)));
        assert_eq!(window.hit(at([20.0, y]), &guild), Some(Hit::ListRow(1)));
        let names: Vec<String> = window.button_boxes(&guild).into_iter().map(|(name, _)| name).take(5).collect();
        assert_eq!(names, ["title:0", "invite:0", "expel:0", "storage:0", "tax:0"]);
        // Edits wait: two rights, a title, a tax of 2 digits; OK sends them all and rests 400 ms.
        window.click_position(Hit::Permission(1, GuildPosition::INVITE), &guild);
        window.click_position(Hit::Permission(2, GuildPosition::INVITE), &guild);
        window.click_position(Hit::PositionTitle(3), &guild);
        for _ in 0..12 {
            window.backspace(&guild);
        }
        window.type_text(&guild, &"Officer".repeat(6));
        window.click_position(Hit::Tax(1), &guild);
        window.type_text(&guild, "123");
        assert_eq!(window.shown_position(&guild, 1), (GuildPosition { id: 1, mode: 0x101, rank: 1, tax: 51 }, "Position 2".to_owned()));
        assert_eq!(window.shown_position(&guild, 3).1.chars().count(), 30);
        assert_eq!(window.hit(at([350.0, 300.0]), &guild), Some(Hit::PositionsOk));
        let now = std::time::Instant::now();
        let sent = window.take_position_edits(now).unwrap();
        assert_eq!(sent.iter().map(|(position, _)| position.id).collect::<Vec<_>>(), [1, 2, 3]);
        window.click_position(Hit::Permission(4, GuildPosition::EXPEL), &guild);
        assert!(window.take_position_edits(now + std::time::Duration::from_millis(300)).is_none());
        // Others see the page as it is.
        guild.master = false;
        assert_eq!(window.hit(at([165.0, y + 4.0]), &guild), Some(Hit::ListRow(1)));
        assert!(window.button_boxes(&guild).is_empty());
        // The notice being written: 60 and 119 characters at most.
        window.typing = Some(GuildTyping::NoticeSubject);
        window.type_text(&guild, &"s".repeat(70));
        window.typing = Some(GuildTyping::NoticeText);
        window.type_text(&guild, &"t".repeat(130));
        assert_eq!((window.notice_draft.0.len(), window.notice_draft.1.len()), (60, 119));
    }

    #[test]
    fn offers_moves_to_the_master_and_leaving_to_members() {
        let mut window = GuildWindow::default();
        window.frame.open = true;
        window.show_tab(MEMBERS);
        let mut guild = crab();
        assert_eq!(window.hit(window.frame.at([20.0, GRID_HEADER + MEMBER_ROW + 5.0]), &guild), Some(Hit::Member(1)));
        let names: Vec<String> = window.button_boxes(&guild).into_iter().map(|(name, _)| name).collect();
        assert_eq!(names[..2], ["member:Boss", "member:Newbie"]);
        assert!(names.contains(&"sort:level".to_owned()) && names.contains(&"login_status".to_owned()));
        use MemberChoice::*;
        // The master on another member: expel, hand over, then (online) party and 1:1 chat, then mail.
        guild.members[1].online = true;
        assert_eq!(guild.member_menu(1, 1, true), [Expel, Assign, PartyInvite, Whisper, Mail]);
        assert_eq!(guild.member_menu(1, 1, false), [Expel, Assign, Whisper, Mail]);
        guild.members[1].online = false;
        assert_eq!(guild.member_menu(1, 1, true), [Expel, Assign, Mail], "offline: no party or chat");
        assert!(guild.member_menu(1, 0, true).is_empty(), "the master doesn't leave");
        // A member: leaving on their own row; on others' only what anyone gets, expel with the right but not on
        // the master.
        guild.master = false;
        assert_eq!(guild.member_menu(2, 1, false), [Leave]);
        guild.members[0].online = true;
        assert_eq!(guild.member_menu(2, 0, false), [Whisper, Mail]);
        guild.mode = 0x10;
        assert!(guild.can_expel() && !guild.can_invite());
        assert_eq!(guild.member_menu(2, 0, false), [Whisper, Mail], "not the master's row");
        guild.members.push(GuildMember { account: 1003, character: 3, position: 5, name: "Third".into(), ..Default::default() });
        assert_eq!(guild.member_menu(2, 2, false), [Expel, Mail]);
        assert_eq!([Leave, Expel, Assign, PartyInvite, Whisper, Mail].map(MemberChoice::message), [508, 509, 2923, 1330, 2043, 2808]);
        // The invitation: with the invite right (not only the master), on a player in no guild.
        assert!(!offers_invite(Some(&guild), 0) && !offers_invite(None, 0), "a member without the invite right");
        guild.mode = 0x01;
        assert!(offers_invite(Some(&guild), 0) && !offers_invite(Some(&guild), 9), "not on one in another guild");
    }

    #[test]
    fn words_the_guild_lines() {
        assert_eq!(
            (0..=5).map(invite_result_line).collect::<Vec<_>>(),
            [Some((378, LINE_RED)), Some((379, LINE_RED)), Some((380, LINE_CYAN)), Some((381, LINE_RED)), Some((2090, LINE_RED)), None,]
        );
        assert_eq!(notice_lines("Hi", "Welcome"), ["[ Hi ]", "[ Welcome ]"]);
        assert_eq!(notice_lines("", "Welcome"), ["[ Welcome ]"]);
        assert_eq!(notice_lines("Hi", ""), ["[ Hi ]"]);
        assert!(notice_lines("", "").is_empty());
        assert_eq!(leaving_lines(true, false, None, "bye", "Nameless"), [(489, String::new(), LINE_RED)]);
        assert_eq!(leaving_lines(true, true, None, "", "Nameless"), [(490, String::new(), LINE_RED)]);
        assert_eq!(leaving_lines(false, false, Some("Ann"), "bye", "Nameless"), [(364, "Ann".to_owned(), LINE_YELLOW), (365, "bye".to_owned(), LINE_YELLOW)]);
        assert_eq!(leaving_lines(false, true, None, "", "Nameless"), [(370, "Nameless".to_owned(), LINE_YELLOW), (371, String::new(), LINE_YELLOW)]);
    }

    #[test]
    fn keeps_pending_moves_and_reasons() {
        let mut pending = Vec::new();
        pick_move(&mut pending, 1002, 2, 5);
        pick_move(&mut pending, 1003, 3, 7);
        pick_move(&mut pending, 1002, 2, 9);
        assert_eq!(pending, [(1003, 3, 7), (1002, 2, 9)], "the last pick wins");
        let guild = crab();
        let mut window = GuildWindow { tab: MEMBERS, ..GuildWindow::default() };
        window.frame.open = true;
        let corner = window.frame.position;
        let at = move |local: [f32; 2]| [corner[0] + local[0], corner[1] + local[1]];
        // The master's drop-down on the second row, then a position from its list.
        assert_eq!(window.hit(at([155.0, 67.0 + 44.0 + 15.0]), &guild), Some(Hit::Drop(1)));
        window.click_members(Hit::Drop(1), &guild);
        assert_eq!(window.hit(at([100.0, 67.0 + 44.0 + 34.0 + 16.0 * 2.0 + 4.0]), &guild), Some(Hit::PickPosition(2)));
        window.click_members(Hit::PickPosition(2), &guild);
        assert_eq!((window.pending.clone(), window.drop), (vec![(1002, 2, 2)], None));
        assert_eq!(window.hit(at([350.0, 300.0]), &guild), Some(Hit::MovesOk));
        let now = std::time::Instant::now();
        assert_eq!(window.take_moves(now), Some(vec![(1002, 2, 2)]));
        pick_move(&mut window.pending, 1001, 1, 3);
        assert_eq!(window.take_moves(now + std::time::Duration::from_millis(300)), None, "OK rests 400 ms");
        assert!(window.take_moves(now + MOVES_REST).is_some());
        // The reason box: 39 characters at most; its place.
        let mut reason = ReasonBox::new(Reasoned::Expel(1002, 2));
        reason.type_text(&"y".repeat(50));
        assert_eq!(reason.text.chars().count(), 39);
        assert_eq!(ReasonBox::at([1024.0, 768.0]), [392.0, 192.0]);
        assert_eq!(reason.hit([392.0 + 180.0, 192.0 + 50.0], [1024.0, 768.0]), Some(ReasonHit::Ok));
        assert_eq!(reason.hit([392.0 + 226.0, 192.0 + 50.0], [1024.0, 768.0]), Some(ReasonHit::Cancel));
    }

    #[test]
    fn applies_position_changes_and_moves() {
        let mut guild = Guild { members: crab().members, ..Default::default() };
        guild.change_position(GuildPosition { id: 1, mode: 1, rank: 0, tax: 10 }, "Officer".into());
        assert_eq!((guild.title(1).as_str(), guild.position(1)), ("Officer", GuildPosition { id: 1, mode: 1, rank: 0, tax: 10 }));
        guild.change_position(GuildPosition { id: 1, mode: 0, rank: 0, tax: 3 }, "Clerk".into());
        assert_eq!(guild.position_info.len(), 1);
        assert_eq!((guild.title(1).as_str(), guild.position(1).tax), ("Clerk", 3));
        guild.move_member(2, 1);
        assert_eq!(guild.members[1].position, 1);
    }

    #[test]
    fn scrolls_the_open_list() {
        let mut window = GuildWindow::default();
        let guild = Guild { position_info: vec![GuildPosition::default(); 20], ..Default::default() };
        window.show_tab(POSITIONS);
        window.scroll_by(50, &guild);
        assert_eq!(window.scroll, 20 - ROWS);
        window.show_tab(EXPELLED);
        window.scroll_by(5, &guild);
        assert_eq!(window.scroll, 0, "nobody expelled, nothing to scroll");
    }

    #[test]
    fn lists_allies_and_antagonists() {
        let relation = |antagonist: bool, id: u32| GuildRelation { antagonist, id, name: format!("G{id}") };
        let relations = vec![relation(false, 1), relation(true, 2), relation(false, 3), relation(false, 4), relation(false, 5)];
        let [allies, antagonists] = columns(&relations);
        assert_eq!(allies.iter().map(|relation| relation.id).collect::<Vec<_>>(), [1, 3, 4]);
        assert_eq!(antagonists.iter().map(|relation| relation.id).collect::<Vec<_>>(), [2]);
        let mut window = GuildWindow::default();
        window.frame.open = true;
        let guild = Guild { relations, ..Default::default() };
        // Rows stack 16 px apart in the two boxes, from x 201 to 287.
        assert_eq!([0, 1, 2].map(|row| relation_box(0, row)), [157.0, 173.0, 189.0].map(|y| ([201.0, y], [86.0, 16.0])));
        assert_eq!([0, 1, 2].map(|row| relation_box(1, row).0[1]), [231.0, 247.0, 263.0]);
        const { assert!(RELATION_BOX_TOP[1] + RELATION_SIZE[1] <= HEIGHT - FOOTER) };
        let at = |x: f32, y: f32| window.frame.at([x, y]);
        // Anyone finds a row (to select it); the menu on it is the master's, in main.
        assert_eq!(window.hit(at(201.0, 232.0), &guild), Some(Hit::Relation(2, true)));
        assert_eq!(window.hit(at(286.0, 160.0), &guild), Some(Hit::Relation(1, false)));
        assert_eq!(window.hit(at(200.0, 160.0), &guild), Some(Hit::Window));
        assert_eq!(window.hit(at(288.0, 160.0), &guild), Some(Hit::Window));
        assert_eq!(window.hit(at(250.0, 248.0), &guild), Some(Hit::Window), "one antagonist, one row");
        assert_eq!(window.relation_boxes(&guild).iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>(), ["G1", "G3", "G4", "G2"]);
        let ally = (0..=5).map(ally_result).collect::<Vec<_>>();
        assert_eq!(ally, [(394, YELLOW), (395, YELLOW), (396, CYAN), (397, YELLOW), (398, YELLOW), (1717, YELLOW)]);
        let antagonist = (0..=3).map(antagonist_result).collect::<Vec<_>>();
        assert_eq!(antagonist, [(495, CYAN), (496, RED), (497, RED), (1718, RED)]);
    }

    #[test]
    fn lists_guild_skills_in_a_column() {
        let skill = |id: u16, kind: u32, level: u16, sp: u16, upgradable: bool| Skill { id, kind, level, sp, range: 0, upgradable };
        assert_eq!(skill_band(&skill(10000, 1, 1, 10, false)), [0x70, 0x9F, 0xED, 0xFF]);
        assert_eq!(skill_band(&skill(10000, 1, 1, 0, false)), [0x70, 0xD7, 0xED, 0xFF]);
        assert_eq!(skill_band(&skill(10000, 1, 0, 10, false)), [0xB4, 0xB4, 0xB4, 0xFF]);
        assert_eq!(skill_sp_text(&skill(10000, 0, 1, 0, false), "Passive"), "Passive");
        assert_eq!(skill_sp_text(&skill(10010, 4, 1, 40, false), "Passive"), "Sp : 40");
        // Rows picked on a 32-px pitch from y 41 (drawn 34 apart from 43).
        assert_eq!(
            [30.0, 41.0, 72.0, 73.0, 41.0 + 32.0 * 6.0 + 5.0, 41.0 + 32.0 * 7.0].map(|y| skill_row_at([150.0, y])),
            [None, Some(0), Some(0), Some(1), Some(6), None]
        );
        let mut guild = Guild { skill_points: 1, skills: vec![skill(10000, 0, 1, 0, true), skill(10001, 4, 0, 30, true)], ..Default::default() };
        let mut window = GuildWindow { tab: SKILLS, ..GuildWindow::default() };
        window.frame.open = true;
        let corner = window.frame.position;
        let at = move |local: [f32; 2]| [corner[0] + local[0], corner[1] + local[1]];
        // The level up button is the master's.
        assert_eq!(window.hit(at([80.0, 45.0 + 34.0 + 5.0]), &guild), Some(Hit::Skill(10001)), "a member has none");
        guild.master = true;
        assert_eq!(window.hit(at([80.0, 45.0 + 34.0 + 5.0]), &guild), Some(Hit::LevelUp(10001)));
        assert_eq!(window.hit(at([150.0, 50.0]), &guild), Some(Hit::Skill(10000)));
        assert_eq!((window.hit(at([50.0, 300.0]), &guild), window.hit(at([100.0, 300.0]), &guild)), (Some(Hit::SkillUse), Some(Hit::SkillClose)));
    }

    #[test]
    fn decodes_emblems_keying_out_magenta() {
        use std::io::Write;
        let mut image = image::RgbImage::from_pixel(24, 24, image::Rgb([0x20, 0x40, 0x80]));
        image.put_pixel(0, 0, image::Rgb([0xFF, 0x00, 0xFF]));
        let mut bmp = std::io::Cursor::new(Vec::new());
        image.write_to(&mut bmp, image::ImageFormat::Bmp).unwrap();
        let mut zipped = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        zipped.write_all(bmp.get_ref()).unwrap();
        let emblem = decode_emblem(&zipped.finish().unwrap()).unwrap();
        assert_eq!(emblem.dimensions(), (24, 24));
        assert_eq!(emblem.get_pixel(0, 0).0, [0; 4]);
        assert_eq!(emblem.get_pixel(5, 5).0, [0x20, 0x40, 0x80, 0xFF]);
        assert!(decode_emblem(b"not zlib").is_none());
    }

    #[test]
    fn places_emblems_over_heads() {
        assert_eq!(head_emblem_box([500.0, 400.0], [1024.0, 768.0], 1.0), [484.0, 240.0, 508.0, 264.0]);
        assert_eq!(head_emblem_box([500.0, 400.0], [1024.0, 768.0], 0.8), [487.2, 272.0, 511.2, 296.0]);
        assert_eq!(head_emblem_box([500.0, 400.0], [1280.0, 720.0], 1.0), [480.0, 250.0, 504.0, 274.0]);
        assert_eq!(head_emblem_box([500.0, 400.0], [1280.0, 720.0], 0.8), [484.0, 280.0, 508.0, 304.0]);
        // At the default camera a sprite pixel is about 1.389 screen pixels at 768 high: a factor of about 1.037,
        // so the box's top is about 166 px over the feet.
        let p = perspective(1.389, 768.0);
        assert!((p - 1.0366).abs() < 1e-3 && (400.0 - head_emblem_box([500.0, 400.0], [1024.0, 768.0], p)[1] - 165.9).abs() < 0.2);
        assert!(wears_head_emblem(true, 0, 5, false, false));
        assert!(!wears_head_emblem(true, 0, 5, false, true), "not ourselves");
        assert!(!wears_head_emblem(true, 0, 0, false, false), "no guild");
        assert!(!wears_head_emblem(true, 0, 5, true, false), "hidden");
        assert!(wears_head_emblem(false, 1286, 5, false, false), "a guardian");
        assert!(!wears_head_emblem(false, 1002, 5, false, false), "another monster");
    }
}
