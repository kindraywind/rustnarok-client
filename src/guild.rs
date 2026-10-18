//! The guild window (Alt+G): its tabs, the guild's info with its emblem, its members, positions, skills, the expel
//! list and the notice. Also guild emblems as the server sends them.

use std::collections::HashMap;
use std::io::Read;

use image::RgbaImage;
use rustnarok_client::grf::Grf;
use rustnarok_client::lua::SkillInfo;
use rustnarok_client::messages;
use rustnarok_client::net::{GuildInfo, GuildMember, GuildPosition, GuildRelation, Skill};
use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};
use crate::skills;
use crate::window::{self, Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI, contains, offset};

// ponytail: 400x317 with six tabs over a 247px page and a 29px footer; calibrate against the real client.
const WIDTH: f32 = 400.0;
const HEIGHT: f32 = 317.0;
const TAB: [f32; 2] = [64.0, 23.0];
const PAGE_TOP: f32 = TITLE_HEIGHT + TAB[1] + 1.0;
const FOOTER: f32 = 29.0;
const LINE: f32 = 20.0;
/// Rows a list (members, positions, the expelled) shows at once; the wheel scrolls it.
const ROWS: usize = 11;
const POSITION: [f32; 2] = [180.0, 120.0];
/// The tabs' labels: info, members, positions, skills, expel list, notice.
const TABS: [usize; 6] = [340, 341, 342, 343, 344, 345];
pub const MEMBERS: usize = 1;
const POSITIONS: usize = 2;
const SKILLS: usize = 3;
const EXPELLED: usize = 4;
/// Info labels: name, level, master, members, average level, castles, exp, tax points.
const LABELS: [usize; 8] = [328, 329, 330, 331, 332, 333, 335, 337];
// ponytail: the emblem's frame top left of the info page, the labels right of it.
const EMBLEM: [f32; 2] = [12.0, PAGE_TOP + 8.0];
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

const INFO_LEFT: f32 = 52.0;
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
const POSITION_COLUMNS: [(usize, f32); 6] = [(510, 10.0), (511, 50.0), (512, 190.0), (513, 240.0), (2499, 290.0), (514, 345.0)];
/// What the invite, expel and storage columns check.
const PERMISSIONS: [u32; 3] = [GuildPosition::INVITE, GuildPosition::EXPEL, GuildPosition::STORAGE];
/// Expel list columns: name, reason.
const EXPEL_COLUMNS: [(usize, f32); 2] = [(407, 10.0), (462, 150.0)];
/// Guild skills keep their place in a grid, by number (GD_APPROVAL is 10000), six to a row.
const SKILL_BASE: u16 = 10000;
const SKILL_COLUMNS: usize = 6;
const SKILL_LEFT: f32 = (WIDTH - SKILL_COLUMNS as f32 * skills::CELL[0]) / 2.0;
const SKILL_POINTS: usize = 284;
const GREY: [u8; 3] = [0x88, 0x88, 0x88];
const TAB_FILL: [u8; 4] = [0xE7, 0xEB, 0xF4, 0xFF];
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
    footer: Texture,
    tab_fill: Texture,
    /// The emblem's frame, and the position page's checkboxes (off, on).
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
}

impl Assets {
    /// The frame drawn behind an emblem.
    pub fn emblem_frame(&self) -> &Texture {
        &self.frame
    }

    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |name: &str| gpu.load_texture(grf, &format!("{UI}{name}.bmp"), false);
        Self {
            footer: image("basic_interface\\btnbar_mid2"),
            tab_fill: gpu.solid(TAB_FILL),
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
    /// A guild skill's box, or its level up button.
    Skill(u16),
    LevelUp(u16),
    /// An ally or an antagonist on the info page (a click selects it, the master's right click ends it): its guild ID,
    /// and whether it's an antagonist.
    Relation(u32, bool),
    /// On the positions page, the master's: a permission box (position, bit), a title, a tax.
    Permission(u32, u32),
    PositionTitle(u32),
    Tax(u32),
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

    /// A position's values with one permission switched, as the master's request carries them.
    pub fn switched(&self, id: u32, permission: u32) -> GuildPosition {
        let mut position = self.position(id);
        position.mode ^= permission;
        position
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

/// Disbanding: its line, and whether it's good news.
pub fn disband_result(result: u32) -> (usize, bool) {
    match result {
        0 => (400, true),
        1 => (401, false),
        2 => (402, false),
        _ => (366, false),
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

/// Where a guild skill's box sits.
fn skill_corner(id: u16) -> [f32; 2] {
    let place = usize::from(id.saturating_sub(SKILL_BASE));
    let [column, row] = [place % SKILL_COLUMNS, place / SKILL_COLUMNS];
    [SKILL_LEFT + column as f32 * skills::CELL[0] + (skills::CELL[0] - skills::BOX) / 2.0, PAGE_TOP + 6.0 + row as f32 * skills::CELL[1]]
}

/// A guild emblem from the server: a zlib-compressed BMP, its magenta keyed out.
pub fn decode_emblem(data: &[u8]) -> Option<RgbaImage> {
    let mut bmp = Vec::new();
    flate2::read::ZlibDecoder::new(data).take(EMBLEM_BYTES).read_to_end(&mut bmp).ok()?;
    let image = image::load_from_memory_with_format(&bmp, image::ImageFormat::Bmp).ok()?;
    Some(crate::render::key_out(image.to_rgba8()))
}

/// The top of a list's row.
fn row_top(row: usize) -> f32 {
    PAGE_TOP + LINE + 6.0 + row as f32 * LINE
}

/// A positions page row's clickable parts (the master's): the title, the three boxes, the tax; corners and sizes.
// ponytail: each part takes its column's width over the whole row.
fn position_parts(row: usize) -> [(PositionPart, [f32; 2], [f32; 2]); 5] {
    let top = row_top(row) - 2.0;
    let column = |index: usize| POSITION_COLUMNS[index].1 - 2.0;
    let width = |index: usize| POSITION_COLUMNS.get(index + 1).map_or(WIDTH - 6.0, |next| next.1 - 2.0) - column(index);
    [
        (PositionPart::Title, [column(1), top], [width(1), LINE]),
        (PositionPart::Permission(PERMISSIONS[0]), [column(2), top], [width(2), LINE]),
        (PositionPart::Permission(PERMISSIONS[1]), [column(3), top], [width(3), LINE]),
        (PositionPart::Permission(PERMISSIONS[2]), [column(4), top], [width(4), LINE]),
        (PositionPart::Tax, [column(5), top], [width(5), LINE]),
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

    pub fn show_tab(&mut self, tab: usize) {
        self.tab = tab;
        self.scroll = 0;
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
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(Hit::Title(hit));
        }
        if let Some(tab) = (0..TABS.len()).find(|&tab| contains([tab as f32 * TAB[0], TITLE_HEIGHT], TAB, local)) {
            return Some(Hit::Tab(tab));
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
            for (row, position) in guild.position_info.iter().skip(self.scroll).take(ROWS).enumerate() {
                if let Some((part, ..)) = position_parts(row).into_iter().find(|(_, corner, size)| contains(*corner, *size, local)) {
                    return Some(part.hit(position.id));
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
            for skill in &guild.skills {
                let corner = skill_corner(skill.id);
                if guild.skill_points > 0 && skill.upgradable && contains(offset(corner, skills::UP), [24.0, 24.0], local) {
                    return Some(Hit::LevelUp(skill.id));
                }
                if contains(corner, [skills::BOX; 2], local) {
                    return Some(Hit::Skill(skill.id));
                }
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
            POSITIONS if guild.master => guild
                .position_info
                .iter()
                .skip(self.scroll)
                .take(ROWS)
                .enumerate()
                .flat_map(|(row, position)| position_parts(row).map(|(part, corner, size)| (part.name(position.id), place(corner, size))))
                .collect(),
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
            POSITIONS => guild.position_info.len(),
            EXPELLED => guild.expelled.len(),
            _ => 0,
        };
        self.scroll = (self.scroll as i32 + rows).clamp(0, total.saturating_sub(ROWS) as i32) as usize;
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
        tooltip: &mut Layer<'a>,
        chrome: &'a Chrome,
        (assets, skill_assets): (&'a Assets, &'a skills::Assets),
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
        self.frame.draw_title(layer, chrome, &guild.name, pointer.0);
        if self.frame.minimized {
            return;
        }
        let hit = self.hit(pointer.0, guild);
        layer.sprites.push((&chrome.white, at([0.0, TITLE_HEIGHT]), [WIDTH, HEIGHT - TITLE_HEIGHT - FOOTER]));
        for x in [0.0, WIDTH - 1.0] {
            layer.sprites.push((&chrome.border, at([x, TITLE_HEIGHT]), [1.0, HEIGHT - TITLE_HEIGHT - FOOTER]));
        }
        // Tabs: the open one joins the page, the others sit back.
        for (tab, &label) in TABS.iter().enumerate() {
            let corner = at([tab as f32 * TAB[0], TITLE_HEIGHT]);
            if tab != self.tab {
                layer.sprites.push((&assets.tab_fill, corner, TAB));
            }
            layer.sprites.push((&chrome.border, [corner[0] + TAB[0] - 1.0, corner[1]], [1.0, TAB[1]]));
            let text = window::fit(font, message(label), TAB[0] - 6.0);
            let x = corner[0] + ((TAB[0] - font.width(&text)) / 2.0).floor();
            layer.texts.push((text, [x, corner[1] + ((TAB[1] - font.line_height()) / 2.0).floor()], if tab == self.tab { [0, 0, 0] } else { GREY }));
        }
        layer.sprites.push((&chrome.border, at([0.0, TITLE_HEIGHT + TAB[1]]), [WIDTH, 1.0]));
        layer.sprites.push((&assets.footer, at([0.0, HEIGHT - FOOTER]), [WIDTH, FOOTER]));

        let text = |layer: &mut Layer<'a>, value: String, place: [f32; 2], color: [u8; 3]| layer.texts.push((value, at(place), color));
        // A list's column labels over a rule, and the top of each row shown.
        let header = |layer: &mut Layer<'a>, columns: &[(usize, f32)]| {
            for &(label, x) in columns {
                layer.texts.push((message(label).to_owned(), at([x, PAGE_TOP + 4.0]), [0, 0, 0]));
            }
            layer.sprites.push((&chrome.border, at([4.0, PAGE_TOP + LINE + 2.0]), [WIDTH - 8.0, 1.0]));
        };
        match self.tab {
            // Info: the emblem, then the guild's name, level, master, members, levels, castles, exp and tax points.
            0 => {
                layer.sprites.push((&assets.frame, at(EMBLEM), assets.frame.size));
                if let Some(emblem) = emblem {
                    layer.sprites.push((emblem, at(offset(EMBLEM, [2.0, 2.0])), EMBLEM_SIZE));
                }
                let info = guild.info.clone().unwrap_or_default();
                let values = [
                    guild.name.clone(),
                    info.level.to_string(),
                    info.master,
                    format!("{} / {}", info.members, info.max_members),
                    info.average_level.to_string(),
                    info.territory,
                    format!("{} / {}", info.exp, info.next_exp),
                    info.tax_points.to_string(),
                ];
                for (row, (label, value)) in LABELS.iter().zip(values).enumerate() {
                    let line = window::fit(font, &format!("{} : {value}", message(*label)), INFO_RIGHT - INFO_LEFT);
                    text(layer, line, [INFO_LEFT, PAGE_TOP + 8.0 + row as f32 * LINE], [0, 0, 0]);
                }
                // Allies, then antagonists: each heading over a white box with its rows; the selected one marked.
                // ponytail: the boxes' look (the window's border colour around white) is a choice.
                for (column, relations) in columns(&guild.relations).into_iter().enumerate() {
                    text(layer, message(RELATION_HEADINGS[column]).to_owned(), [RELATION_LEFT, RELATION_HEADING_TOP[column]], [0, 0, 0]);
                    let [w, h] = RELATION_SIZE;
                    let (left, top) = (RELATION_LEFT, RELATION_BOX_TOP[column]);
                    layer.sprites.push((&chrome.border, at([left, top]), RELATION_SIZE));
                    layer.sprites.push((&assets.white, at([left + 1.0, top + 1.0]), [w - 2.0, h - 2.0]));
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
            // Positions: rank, title, what its members may do, and the tax on their exp.
            POSITIONS => {
                header(layer, &POSITION_COLUMNS);
                for (row, position) in guild.position_info.iter().skip(self.scroll).take(ROWS).enumerate() {
                    let top = row_top(row);
                    // The part under the master's pointer can be clicked.
                    if let Some((_, corner, size)) = position_parts(row).into_iter().find(|(part, ..)| hit == Some(part.hit(position.id))) {
                        layer.sprites.push((&assets.hover, at(corner), size));
                    }
                    let title = guild.positions.get(&position.id).map_or("", String::as_str);
                    text(layer, (position.id + 1).to_string(), [POSITION_COLUMNS[0].1, top], [0, 0, 0]);
                    text(layer, window::fit(font, title, POSITION_COLUMNS[2].1 - POSITION_COLUMNS[1].1 - 6.0), [POSITION_COLUMNS[1].1, top], [0, 0, 0]);
                    for (permission, &(label, x)) in PERMISSIONS.iter().zip(&POSITION_COLUMNS[2..5]) {
                        let check = &assets.check[usize::from(position.mode & permission != 0)];
                        let x = x + ((font.width(message(label)) - check.size[0]) / 2.0).floor();
                        layer.sprites.push((check, at([x, top + 1.0]), check.size));
                    }
                    text(layer, position.tax.to_string(), [POSITION_COLUMNS[5].1, top], [0, 0, 0]);
                }
            }
            // Skills where they sit, the master's level up buttons, and the points left on the footer.
            SKILLS => {
                for skill in &guild.skills {
                    let corner = skill_corner(skill.id);
                    let info = skill_info.get(&skill.id);
                    let name = info.map_or_else(|| skill.id.to_string(), |info| info.name.clone());
                    skills::draw_box(layer, skill_assets, icons, font, at(corner), info, &name, skill.level);
                    if guild.skill_points > 0 && skill.upgradable {
                        let state = if hit == Some(Hit::LevelUp(skill.id)) { 1 + usize::from(pointer.1) } else { 0 };
                        let up = &skill_assets.up[state];
                        layer.sprites.push((up, at(offset(corner, skills::UP)), up.size));
                    }
                    if hit == Some(Hit::Skill(skill.id)) {
                        let max = info.map_or(0, |info| info.max_level);
                        window::tooltip(tooltip, chrome, font, &format!("{name} Lv {}/{max}", skill.level), offset(pointer.0, [12.0, 12.0]), [255; 3]);
                    }
                }
                let points = messages::fill(message(SKILL_POINTS), &[&guild.skill_points]);
                text(layer, points, [12.0, HEIGHT - FOOTER + ((FOOTER - font.line_height()) / 2.0).floor()], [0, 0, 0]);
            }
            // The expel list: who, and why.
            EXPELLED => {
                header(layer, &EXPEL_COLUMNS);
                for (row, (name, reason)) in guild.expelled.iter().skip(self.scroll).take(ROWS).enumerate() {
                    let [(_, name_x), (_, reason_x)] = EXPEL_COLUMNS;
                    text(layer, window::fit(font, name, reason_x - name_x - 6.0), [name_x, row_top(row)], [0, 0, 0]);
                    text(layer, window::fit(font, reason, WIDTH - reason_x - 8.0), [reason_x, row_top(row)], [0, 0, 0]);
                }
            }
            // The notice: its subject, then its text.
            _ => {
                text(layer, format!("{} : {}", message(515), guild.notice.0), [12.0, PAGE_TOP + 8.0], [0, 0, 0]);
                text(layer, message(516).to_owned(), [12.0, PAGE_TOP + 8.0 + LINE], [0, 0, 0]);
                for (row, line) in window::wrap(font, &guild.notice.1, WIDTH - 30.0, 8).into_iter().enumerate() {
                    text(layer, line, [20.0, PAGE_TOP + 8.0 + (row + 2) as f32 * LINE], [0, 0, 0]);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let mut window = GuildWindow::default();
        window.frame.open = true;
        let guild = Guild::default();
        let at = |local: [f32; 2]| window.frame.at(local);
        assert_eq!(window.hit(at([TAB[0] * 1.5, TITLE_HEIGHT + 5.0]), &guild), Some(Hit::Tab(1)));
        assert_eq!(window.hit(at([TAB[0] * 5.5, TITLE_HEIGHT + 5.0]), &guild), Some(Hit::Tab(5)));
        assert_eq!(window.hit(at([200.0, 200.0]), &guild), Some(Hit::Window));
    }

    fn crab() -> Guild {
        let member =
            |character: u32, name: &str, position: u32| GuildMember { account: character + 1000, character, position, name: name.into(), ..Default::default() };
        Guild {
            master: true,
            members: vec![member(1, "Boss", 0), member(2, "Newbie", 19)],
            positions: (0..20).map(|id| (id, format!("Position {}", id + 1))).collect(),
            position_info: (0..20).map(|id| GuildPosition { id, mode: 0, tax: 0 }).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn hits_the_masters_position_parts() {
        let mut window = GuildWindow::default();
        window.frame.open = true;
        window.show_tab(POSITIONS);
        let mut guild = crab();
        let at = |local: [f32; 2]| window.frame.at(local);
        // The second row is position 1.
        let middle = row_top(1) + 5.0;
        assert_eq!(window.hit(at([60.0, middle]), &guild), Some(Hit::PositionTitle(1)));
        assert_eq!(window.hit(at([200.0, middle]), &guild), Some(Hit::Permission(1, GuildPosition::INVITE)));
        assert_eq!(window.hit(at([250.0, middle]), &guild), Some(Hit::Permission(1, GuildPosition::EXPEL)));
        assert_eq!(window.hit(at([300.0, middle]), &guild), Some(Hit::Permission(1, GuildPosition::STORAGE)));
        assert_eq!(window.hit(at([350.0, middle]), &guild), Some(Hit::Tax(1)));
        let names: Vec<String> = window.button_boxes(&guild).into_iter().map(|(name, _)| name).take(5).collect();
        assert_eq!(names, ["title:0", "invite:0", "expel:0", "storage:0", "tax:0"]);
        // A box click switches that one bit and keeps the rest.
        guild.position_info[1].mode = GuildPosition::STORAGE;
        guild.position_info[1].tax = 5;
        assert_eq!(guild.switched(1, GuildPosition::INVITE), GuildPosition { id: 1, mode: 0x101, tax: 5 });
        assert_eq!(guild.switched(1, GuildPosition::STORAGE), GuildPosition { id: 1, mode: 0, tax: 5 });
        // Others see the page as it is.
        guild.master = false;
        assert_eq!(window.hit(at([200.0, middle]), &guild), Some(Hit::Window));
        assert!(window.button_boxes(&guild).is_empty());
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
        guild.change_position(GuildPosition { id: 1, mode: 1, tax: 10 }, "Officer".into());
        assert_eq!((guild.title(1).as_str(), guild.position(1)), ("Officer", GuildPosition { id: 1, mode: 1, tax: 10 }));
        guild.change_position(GuildPosition { id: 1, mode: 0, tax: 3 }, "Clerk".into());
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
        assert_eq!([0, 1, 2, 9].map(disband_result), [(400, true), (401, false), (402, false), (366, false)]);
    }

    #[test]
    fn places_guild_skills_by_number() {
        assert_eq!(skill_corner(10000), [SKILL_LEFT + 17.0, PAGE_TOP + 6.0]);
        assert_eq!(skill_corner(10007), [SKILL_LEFT + 62.0 + 17.0, PAGE_TOP + 6.0 + 58.0]);
        // The last of the 20 still fits on the page.
        assert!(skill_corner(10019)[1] + skills::CELL[1] <= HEIGHT - FOOTER);
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
