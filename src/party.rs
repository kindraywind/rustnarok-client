//! The party and friends: who's in them, and their window (Alt+Z): each member's job icon, level, name and map, HP
//! and whether they're online, or each friend's name; the buttons to make a party, invite, leave or set it up (or to
//! talk to or drop a friend); the tabs and the count at the bottom. The window has two sizes. A member dragged out of
//! the list keeps a small HP window.

use rustnarok_client::grf::Grf;
use rustnarok_client::net::{Friend, PartyMember};
use rustnarok_client::text::Font;

use std::collections::HashMap;

use image::RgbaImage;

use crate::render::{Gpu, Texture};
use crate::window::{self, Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI, contains, offset};

/// The window's two size modes: its size, its rows' height, and where a row's content starts in the row.
const MODES: [([f32; 2], f32, [f32; 2]); 2] = [([273.0, 326.0], 34.0, [5.0, 5.0]), ([243.0, 179.0], 27.0, [4.0, 1.0])];
/// The rows start under the title; they fit in h − 58, the white body ends 38 above the bottom.
const LIST_UNDER: f32 = 58.0;
/// The corner grip sets the height to 58 + 23·k: k rows of 23 px, 2 to 12.
// ponytail: the grip's rows are the friends' 23 px whatever the mode, from 2 (one member row in either mode) to 12; a
// mode keeps its own height until its grip is used.
const GRIP_ROW: f32 = 23.0;
const GRIP_ROWS: std::ops::RangeInclusive<usize> = 2..=12;
const BODY_UNDER: f32 = 38.0;
/// The button bar 40 above the bottom, 20 tall; the footer 16 above it, 12 tall.
const BAR_ABOVE: f32 = 40.0;
const BAR_HEIGHT: f32 = 20.0;
const FOOTER_ABOVE: f32 = 16.0;
const FOOTER_HEIGHT: f32 = 12.0;
/// The scrollbar along the rows' right edge.
const SCROLLBAR: f32 = 15.0;
const BUTTON: [f32; 2] = [26.0, 20.0];
const ICON: f32 = 25.0;
const STATUS: [f32; 2] = [26.0, 11.0];
const LEVEL_TEXT: [u8; 3] = [0x37, 0x3A, 0x49];
const OFFLINE_TEXT: [u8; 3] = [0x86, 0x8A, 0xA3];
const BODY_BORDER: [u8; 4] = [0xC0, 0xC0, 0xC0, 0xFF];
// ponytail: the hovered row's band colour is a choice.
const HOVER: [u8; 4] = [0x70, 0x9F, 0xED, 0xFF];
/// The party's most (rathena's MAX_PARTY).
const MOST_IN_PARTY: usize = 12;
const PARTY_ART: &str = "renewalparty\\";
/// A small member window. ponytail: 120×32 with a 100×5 bar in the party window's colours; the client's bitmaps
/// would replace the size and the flat fill.
/// A small party window, its grid's cell, and its parts: the background, the job icon (the leader's lower), the crown,
/// the line and how wide it may be, the HP bar.
pub const MINI_SIZE: [f32; 2] = [174.0, 52.0];
const CELL: [f32; 2] = [177.0, 55.0];
const MINI_ICON: [f32; 2] = [7.0, 6.0];
const MINI_LEADER_ICON: [f32; 2] = [7.0, 8.0];
const MINI_CROWN: [f32; 2] = [9.0, 0.0];
const MINI_TEXT: [f32; 2] = [35.0, 9.0];
const MINI_TEXT_WIDTH: f32 = 104.0;
const MINI_BAR: [f32; 2] = [35.0, 23.0];
const MINI_BAR_SIZE: [f32; 2] = [66.0, 5.0];
/// "In the last row", for a small window let go outside the used cells.
pub const IN_THE_LAST_ROW: usize = 3145;
const MOST_FRIENDS: usize = 40;
const FRIEND_ROW: f32 = 23.0;
const FRIEND: [u8; 3] = [0x10, 0x25, 0x50];
const FRIENDS_MESSAGE: usize = 102;
const POSITION: [f32; 2] = [300.0, 200.0];
/// "Party", "Cap", and what the buttons do.
const TITLE_MESSAGE: usize = 103;
const CAP_MESSAGE: usize = 3231;
const OFFLINE: [u8; 3] = [0xAA, 0xAA, 0xAA];

/// Someone in the party, as the window shows them.
#[derive(Debug, Clone, Default)]
pub struct Member {
    pub info: PartyMember,
    pub hp: Option<[i32; 2]>,
    /// Where they are on the minimap, when on the player's map.
    pub cell: Option<[i32; 2]>,
    pub dead: bool,
    /// Their colour on the minimap and in the window: their place in the list when they were added.
    pub color: usize,
}

#[derive(Default)]
pub struct Party {
    pub name: String,
    pub members: Vec<Member>,
    /// EXP shared evenly (else each takes their own).
    pub exp_shared: bool,
    /// Items picked up go to the party (else to whoever picks them up).
    pub pickup_shared: bool,
    /// Items are shared out among the members (else the one who gets them keeps them).
    pub item_shared: bool,
}

impl Party {
    pub fn member(&mut self, account: u32) -> Option<&mut Member> {
        self.members.iter_mut().find(|member| member.info.account == account)
    }

    /// A member joined or changed: the entry is replaced, keeping what's known of their HP, place and colour; someone
    /// new takes the colour of their place in the list now.
    pub fn upsert(&mut self, info: PartyMember) {
        let place = self.members.len();
        match self.member(info.account) {
            Some(member) => member.info = info,
            None => self.members.push(Member { info, color: place, ..Member::default() }),
        }
    }

    pub fn leader(&self) -> Option<u32> {
        self.members.iter().find(|member| member.info.leader).map(|member| member.info.account)
    }

    pub fn settings(&self) -> Settings {
        Settings { exp_even: self.exp_shared, pickup: self.pickup_shared, share: self.item_shared }
    }
}

/// The party's three settings, as the setup window shows them.
#[derive(Debug, PartialEq, Clone, Copy, Default)]
pub struct Settings {
    pub exp_even: bool,
    pub pickup: bool,
    pub share: bool,
}

impl Settings {
    fn get(self, row: usize) -> bool {
        [self.exp_even, self.pickup, self.share][row]
    }

    fn set(&mut self, row: usize, on: bool) {
        *[&mut self.exp_even, &mut self.pickup, &mut self.share][row] = on;
    }
}

/// How a line about the party reads: good news in cyan, refusals in red.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Tone {
    Good,
    Bad,
}

/// The line for an invitation's answer (ZC_PARTY_JOIN_REQ_ACK, with the invited character's name), if any.
pub fn invite_result_line(result: u32) -> Option<(usize, Tone)> {
    Some(match result {
        0 => (80, Tone::Bad),
        1 => (2060, Tone::Bad),
        2 => (2061, Tone::Good),
        3 => (83, Tone::Bad),
        4 => (608, Tone::Bad),
        5 => (1324, Tone::Bad),
        7 => (71, Tone::Bad),
        8 => (1388, Tone::Bad),
        9 => (1871, Tone::Bad),
        10 => (3027, Tone::Bad),
        11 => (3226, Tone::Bad),
        18 => (3634, Tone::Bad),
        _ => return None,
    })
}

/// The line for making a party (ZC_ACK_MAKE_GROUP), if any; 0 made it.
pub fn made_line(result: u8) -> Option<(usize, Tone)> {
    Some(match result {
        0 => (77, Tone::Good),
        1 => (78, Tone::Bad),
        2 => (79, Tone::Bad),
        3 => (1387, Tone::Bad),
        _ => return None,
    })
}

/// "Party Setup", then each line's label, value and a note in brackets after it (ZC_REQ_GROUPINFO_CHANGE_V2): how EXP
/// is shared (EXP 2: each takes, as a level gap keeps it), how items are picked up, how they're shared.
pub fn setup_lines(exp: u32, pickup: bool, share: bool) -> [(usize, usize, usize, Option<usize>); 3] {
    const SETUP: usize = 291;
    const EACH: usize = 287;
    let (exp_value, note) = match exp {
        1 => (288, None),
        2 => (EACH, Some(1535)),
        _ => (EACH, None),
    };
    [(SETUP, 292, exp_value, note), (SETUP, 293, if pickup { 290 } else { EACH }, None), (SETUP, 738, if share { 288 } else { EACH }, None)]
}

/// An invitation answer's line: the template's `%s` takes the name; the accepted and rejected lines have none, the
/// name goes in front.
pub fn invite_result_text(template: &str, name: &str) -> String {
    if template.contains('%') {
        rustnarok_client::messages::fill(template, &[&name])
    } else if template.starts_with(' ') {
        format!("{name}{template}")
    } else {
        template.to_owned()
    }
}

/// The invitation box's question: "(" + the party's name + ")" + message 94.
pub fn invitation_text(party: &str, asks: &str) -> String {
    format!("({party}){asks}")
}

/// The invitation box's corner on a screen this size: ((W − 640) / 2 + 200, 200·H / 480).
pub fn invitation_at(screen: [f32; 2]) -> [f32; 2] {
    [(screen[0] - 640.0) / 2.0 + 200.0, 200.0 * screen[1] / 480.0]
}

/// What the window's buttons do.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Button {
    /// The adventurer's agency; the lock (on or off).
    Agency,
    Lock(bool),
    /// Sort Mini Party Window.
    Sort,
    Create,
    Invite,
    Leave,
    Setup,
    /// Friends: whisper the one picked, drop them, or open the 1:1 setup.
    Whisper,
    Delete,
    FriendSetup,
}

impl Button {
    fn image(self) -> &'static str {
        match self {
            Self::Agency => "mesbtn_01",
            Self::Lock(false) => "mesbtn_07",
            Self::Lock(true) => "mesbtn_06",
            Self::Sort => "mesbtn_011",
            Self::Create => "mesbtn_08",
            Self::Invite => "mesbtn_010",
            Self::Leave => "mesbtn_09",
            Self::Setup => "mesbtn_04",
            Self::Whisper => "mesbtn_02",
            Self::Delete => "mesbtn_05",
            Self::FriendSetup => "mesbtn_04",
        }
    }

    /// The client's words for it, in a tooltip.
    fn message(self) -> usize {
        match self {
            Self::Agency => 3514,
            Self::Lock(false) => 1627,
            Self::Lock(true) => 1626,
            Self::Sort => 3983,
            Self::Create => 2054,
            Self::Invite => 2056,
            Self::Leave => 2055,
            Self::Setup => 101,
            Self::Whisper => 99,
            Self::Delete => 351,
            // ponytail: the same tooltip as the party setup button; the client's own words for this one aren't pinned down.
            Self::FriendSetup => 101,
        }
    }

    /// Its name for the test driver.
    pub fn name(self) -> &'static str {
        match self {
            Self::Agency => "agency",
            Self::Lock(_) => "lock",
            Self::Sort => "sort",
            Self::Create => "create",
            Self::Invite => "invite",
            Self::Leave => "leave",
            Self::Setup => "setup",
            Self::Whisper => "whisper",
            Self::Delete => "delete",
            Self::FriendSetup => "friendsetup",
        }
    }
}

/// What Leave does once asked (`leave_decision`).
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Leaving {
    /// Leave now.
    Leave,
    /// Hand the party to this member first, then leave.
    HandOver(u32),
    /// Others are online, none on our map: "You can leave a party after delegation." (3273), nothing sent.
    Delegate,
    /// Nobody else is online: ask whether to disband instead (3257).
    AskDisband,
}

pub const LEAVE_AFTER_DELEGATION: usize = 3273;
pub const ASK_DISBAND: usize = 3257;

/// Whether two map names are the same map (`prontera.gat` and `prontera`).
fn same_map(a: &str, b: &str) -> bool {
    let key = |map: &str| map.to_ascii_lowercase().trim_end_matches(".gat").trim_end_matches(".rsw").to_owned();
    key(a) == key(b)
}

/// What Leave does: a leader with others hands the party to the first other member, in list order, who is online
/// and on our map; others online elsewhere keep them in; nobody else online asks to disband.
pub fn leave_decision(party: &Party, me: u32, our_map: &str) -> Leaving {
    let others: Vec<&PartyMember> = party.members.iter().map(|member| &member.info).filter(|info| info.account != me).collect();
    if party.leader() != Some(me) || others.is_empty() {
        return Leaving::Leave;
    }
    if let Some(next) = others.iter().find(|info| info.online && same_map(&info.map, our_map)) {
        return Leaving::HandOver(next.account);
    }
    if others.iter().any(|info| info.online) { Leaving::Delegate } else { Leaving::AskDisband }
}

/// A member's right-click menu entries.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum MemberChoice {
    Mail,
    Leave,
    Whisper,
    Expel,
    Assign,
    /// Give them a small party window, or close theirs.
    AddSmall,
    RemoveSmall,
}

impl MemberChoice {
    pub fn message(self) -> usize {
        match self {
            Self::Mail => 2808,
            Self::Leave => 96,
            Self::Whisper => 360,
            Self::Expel => 97,
            Self::Assign => 1532,
            Self::AddSmall => 3101,
            Self::RemoveSmall => 3102,
        }
    }
}

/// The menu on a member's row (or small party window, `small`): mail; on our own row leaving, on another whispering
/// and, when we lead, expelling and handing over; then on an online member's row adding and removing a small window,
/// on a small window removing it.
pub fn member_menu(me: u32, account: u32, leader: bool, small: bool, online: bool) -> Vec<MemberChoice> {
    let mut choices = vec![MemberChoice::Mail];
    if account == me {
        choices.push(MemberChoice::Leave);
    } else {
        choices.push(MemberChoice::Whisper);
        if leader {
            choices.extend([MemberChoice::Expel, MemberChoice::Assign]);
        }
    }
    if small {
        choices.push(MemberChoice::RemoveSmall);
    } else if online {
        choices.extend([MemberChoice::AddSmall, MemberChoice::RemoveSmall]);
    }
    choices
}

/// Skills that go on a member's row with Shift held (or /noshift on).
const SUPPORT_SKILLS: [u16; 60] = [
    28, 29, 30, 34, 35, 53, 54, 66, 67, 68, 71, 72, 73, 77, 78, 138, 139, 231, 234, 235, 236, 237, 255, 280, 281, 282, 283, 286, 289, 374, 402, 463, 464, 465,
    466, 467, 468, 469, 470, 471, 472, 479, 1015, 2001, 2020, 2026, 2027, 2050, 2051, 2052, 2053, 2341, 2344, 2345, 2346, 2347, 2348, 2451, 2515, 8221,
];

/// Whether a press on a member's row (or small window) casts the skill being aimed on them: a Heal-like skill
/// (`friend`), or with Shift (`shift`, /noshift counts) one on the support list; our own row takes support skills only,
/// an offline one nothing.
pub fn row_takes_skill(skill: u16, friend: bool, shift: bool, own: bool, online: bool) -> bool {
    let support = SUPPORT_SKILLS.contains(&skill);
    if !online {
        return false;
    }
    // ponytail: our own row keeps the same Heal-like or Shift rule, limited to the support list.
    (friend || (shift && support)) && (!own || support)
}

/// The invite button pressed by someone not leading: "Only the leader can invite." (2070).
pub const ONLY_LEADER_INVITES: usize = 2070;

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(TitleHit),
    Button(Button),
    /// A member, by account.
    Member(u32),
    /// A friend, by place in the list.
    Friend(usize),
    /// The footer's tabs: friends or party.
    Tab(bool),
    /// The corner grip.
    Resize,
    Window,
}

pub struct Assets {
    /// The button bar's left end, middle and right end.
    bar: [Texture; 3],
    /// The window's body edge, the hovered row, the party count's box, and the size button in mode 1 (normal,
    /// hovered, pressed).
    body_border: Texture,
    hover: Texture,
    member_count: Texture,
    maximum: [Texture; 3],
    /// The crown and the status icons (us, online, offline); the HP bar's border, background, fill and low fill.
    crown: Texture,
    status: [Texture; 3],
    bar_colors: [Texture; 4],
    /// Each button's normal, hovered and pressed images.
    buttons: Vec<(Button, [Texture; 3])>,
    radio: [Texture; 2],
    /// The small windows' background and the offline crown.
    mini_bg: Texture,
    crown_grey: Texture,
    has_small: Texture,
    /// The docked windows' edit box.
    edit: Texture,
    picked: Texture,
    /// The corner grip.
    resize: Texture,
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let basic = format!("{UI}basic_interface\\");
        let image = |name: String| gpu.load_texture(grf, &name, false);
        Self {
            bar: ["mesbtn_left", "mesbtn_mid", "mesbtn_right"].map(|name| image(format!("{basic}{name}.bmp"))),
            body_border: gpu.solid(BODY_BORDER),
            hover: gpu.solid(HOVER),
            member_count: image(format!("{UI}{PARTY_ART}bg_partymember.bmp")),
            maximum: ["out", "over", "press"].map(|look| image(format!("{UI}{PARTY_ART}btn_titlebar_maximum_{look}.bmp"))),
            crown: image(format!("{UI}{PARTY_ART}ico_partycrown.bmp")),
            status: ["me", "on", "off"].map(|name| image(format!("{UI}{PARTY_ART}icon_party_{name}.bmp"))),
            bar_colors: [[0x14, 0x19, 0x9B, 255], [0x44, 0x44, 0x44, 255], [0x16, 0xEA, 0x25, 255], [0xEA, 0x16, 0x16, 255]].map(|color| gpu.solid(color)),
            buttons: [
                Button::Agency,
                Button::Lock(false),
                Button::Lock(true),
                Button::Sort,
                Button::Create,
                Button::Invite,
                Button::Leave,
                Button::Setup,
                Button::Whisper,
                Button::Delete,
                Button::FriendSetup,
            ]
            .into_iter()
            .map(|button| (button, ["", "_a", "_b"].map(|look| image(format!("{basic}{}{look}.bmp", button.image())))))
            .collect(),
            radio: ["radiobtn_off", "radiobtn_on"].map(|name| image(format!("{UI}{name}.bmp"))),
            mini_bg: image(format!("{UI}{PARTY_ART}img_partymini_bg.tga")),
            has_small: gpu.solid([0xDD, 0xE8, 0xFF, 0xFF]),
            edit: gpu.solid(EDIT_BACKGROUND),
            crown_grey: crate::render::load_image(grf, &format!("{UI}{PARTY_ART}ico_partycrown.bmp"))
                .map_or_else(|_| gpu.solid([0, 0, 0, 0]), |crown| gpu.texture(&greyed(crown), false)),
            picked: gpu.solid([0x73, 0x9C, 0xEF, 0xFF]),
            resize: image(format!("{UI}btn_resize.bmp")),
        }
    }

    fn button(&self, button: Button) -> &[Texture; 3] {
        &self.buttons.iter().find(|(which, _)| *which == button).unwrap().1
    }
}

/// The choices' names for the test driver: [setting][0 the "on" choice, 1 the "off"].
const SETUP_NAMES: [[&str; 2]; 3] = [["exp_even", "exp_each"], ["pickup_party", "pickup_each"], ["share_shared", "share_individual"]];

/// A group of choices: the setting (0 EXP, 1 pickup, 2 distribution), the label and its y, and each choice's words, y
/// and whether it is the "on" one.
type ChoiceGroup = (usize, usize, f32, [(usize, f32, bool); 2]);
/// An edit box: its place and width, and the label over it (words and place).
type EditBox = ([f32; 2], f32, Option<(usize, [f32; 2])>);

/// The three windows docked right of the party window: setup (the Setup button), create (Create Party) and invite
/// (Party Invitation).
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum DialogKind {
    Setup,
    Create,
    Invite,
}

impl DialogKind {
    fn size(self) -> [f32; 2] {
        match self {
            Self::Setup => [130.0, 245.0],
            Self::Create => [130.0, 220.0],
            Self::Invite => [150.0, 100.0],
        }
    }

    fn title(self) -> usize {
        match self {
            Self::Setup => 291,
            Self::Create => 2054,
            Self::Invite => 2056,
        }
    }

    /// Its groups of choices: the setting (0 EXP, 1 pickup, 2 distribution), the label and its y, and each choice's
    /// words, y and whether it is the "on" one. Top to bottom: the "off" choice, then the "on".
    fn groups(self) -> &'static [ChoiceGroup] {
        const SETUP: [ChoiceGroup; 3] = [
            (0, 292, 22.0, [(287, 42.0, false), (288, 62.0, true)]),
            (1, 293, 87.0, [(289, 107.0, false), (290, 127.0, true)]),
            (2, 738, 152.0, [(739, 172.0, false), (740, 192.0, true)]),
        ];
        const CREATE: [ChoiceGroup; 2] = [(1, 293, 67.0, [(289, 87.0, false), (290, 107.0, true)]), (2, 738, 132.0, [(739, 152.0, false), (740, 172.0, true)])];
        match self {
            Self::Setup => &SETUP,
            Self::Create => &CREATE,
            Self::Invite => &[],
        }
    }

    /// The edit box's place and width, and the label over it.
    fn edit(self) -> Option<EditBox> {
        match self {
            Self::Setup => None,
            Self::Create => Some(([20.0, 38.0], 100.0, Some((2057, [10.0, 22.0])))),
            // ponytail: no label is given for the invite window's box.
            Self::Invite => Some(([20.0, 42.0], 120.0, None)),
        }
    }
}

const RADIO_HIT: [f32; 2] = [160.0, 12.0];
const EDIT_HEIGHT: f32 = 20.0;
const EDIT_BACKGROUND: [u8; 4] = [0xE6, 0xE6, 0xE6, 0xFF];
/// A name: at most 23 bytes.
const NAME_BYTES: usize = 23;
const DIALOG_BUTTON: [f32; 2] = [42.0, 20.0];
/// The red lines for an empty name: the party's, the character's.
pub const PARTY_NAME_EMPTY: usize = 2073;
pub const CHARACTER_NAME_EMPTY: usize = 2072;
/// "<name> has received an invitation to join your party." and "Only the Party Leader can change this setting."
pub const INVITATION_SENT: usize = 2059;
pub const LEADER_ONLY: usize = 294;

/// Where a docked window opens: right of the party window when it's open (its corner and width), else at
/// ((W − 640)/2 + 200, 120·H/480).
pub fn docked_at(party: Option<([f32; 2], f32)>, screen: [f32; 2]) -> [f32; 2] {
    match party {
        Some((at, width)) => [at[0] + width + 2.0, at[1]],
        None => [(screen[0] - 640.0) / 2.0 + 200.0, 120.0 * screen[1] / 480.0],
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum DialogHit {
    Title(TitleHit),
    /// A setting (0 EXP, 1 pickup, 2 distribution) and which choice: true the "on" one.
    Choice(usize, bool),
    Edit,
    Ok,
    Cancel,
    Window,
}

/// What a click in a docked window comes to.
#[derive(Debug, PartialEq, Clone)]
pub enum DialogAction {
    Close,
    /// Setup: send these settings, and close.
    Send(Settings),
    /// A member clicked a setting: "Only the Party Leader can change this setting." (294) in the chat.
    LeaderOnly,
    /// OK with an empty name: this red line, and the window stays.
    EmptyName(usize),
    /// Create: CZ_MAKE_GROUP2 with the name and the item choices (pickup shared, items shared), and close.
    Make(String, bool, bool),
    /// Invite: CZ_PARTY_JOIN_REQ with the name, and close.
    Invite(String),
}

/// A window docked right of the party window.
pub struct PartyDialog {
    pub kind: DialogKind,
    pub frame: Frame,
    pub settings: Settings,
    start: Settings,
    leader: bool,
    /// The name typed, and whether the edit box has the caret.
    pub name: String,
    pub typing: bool,
}

impl PartyDialog {
    /// The setup window on the party's settings; create with both item choices off; invite with an empty name. EXP 2
    /// shows as Each Take.
    pub fn new(kind: DialogKind, party: Option<&Party>, leader: bool, at: [f32; 2]) -> Self {
        let mut frame = Frame::new(at, kind.size()[0]);
        frame.open = true;
        let settings = if kind == DialogKind::Setup { party.map(Party::settings).unwrap_or_default() } else { Settings::default() };
        Self { kind, frame, settings, start: settings, leader, name: String::new(), typing: kind != DialogKind::Setup }
    }

    fn ok_at(&self) -> [f32; 2] {
        let [w, h] = self.kind.size();
        [w - 92.0, h - 24.0]
    }

    fn cancel_at(&self) -> [f32; 2] {
        let [w, h] = self.kind.size();
        [w - 46.0, h - 24.0]
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<DialogHit> {
        let local = self.frame.local(cursor, self.kind.size()[1])?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(DialogHit::Title(hit));
        }
        for &(setting, _, _, choices) in self.kind.groups() {
            for (_, y, on) in choices {
                if contains([20.0, y], RADIO_HIT, local) {
                    return Some(DialogHit::Choice(setting, on));
                }
            }
        }
        if let Some((at, width, _)) = self.kind.edit()
            && contains(at, [width, EDIT_HEIGHT], local)
        {
            return Some(DialogHit::Edit);
        }
        if contains(self.ok_at(), DIALOG_BUTTON, local) {
            return Some(DialogHit::Ok);
        }
        if contains(self.cancel_at(), DIALOG_BUTTON, local) {
            return Some(DialogHit::Cancel);
        }
        Some(DialogHit::Window)
    }

    /// A choice moves for the leader (and when creating); a member's click is refused in the chat. OK sends.
    pub fn click(&mut self, hit: DialogHit) -> Option<DialogAction> {
        match hit {
            DialogHit::Choice(..) if self.kind == DialogKind::Setup && !self.leader => return Some(DialogAction::LeaderOnly),
            DialogHit::Choice(setting, on) => self.settings.set(setting, on),
            DialogHit::Edit => self.typing = true,
            DialogHit::Title(TitleHit::Close) | DialogHit::Cancel => return Some(DialogAction::Close),
            DialogHit::Ok => return Some(self.ok()),
            _ => {}
        }
        None
    }

    /// OK (or Enter in the edit box).
    pub fn ok(&self) -> DialogAction {
        let name = self.name.trim();
        match self.kind {
            DialogKind::Setup if self.leader && self.settings != self.start => DialogAction::Send(self.settings),
            DialogKind::Setup => DialogAction::Close,
            DialogKind::Create if name.is_empty() => DialogAction::EmptyName(PARTY_NAME_EMPTY),
            DialogKind::Create => DialogAction::Make(name.to_owned(), self.settings.pickup, self.settings.share),
            DialogKind::Invite if name.is_empty() => DialogAction::EmptyName(CHARACTER_NAME_EMPTY),
            DialogKind::Invite => DialogAction::Invite(name.to_owned()),
        }
    }

    /// Typing into the edit box: up to 23 bytes.
    pub fn type_text(&mut self, text: &str) {
        for character in text.chars().filter(|character| !character.is_control()) {
            if self.name.len() + character.len_utf8() > NAME_BYTES {
                break;
            }
            self.name.push(character);
        }
    }

    /// The choices, the edit box and the buttons on screen (for the test driver).
    pub fn button_boxes(&self) -> Vec<(&'static str, [f32; 4])> {
        let place = |at: [f32; 2], size: [f32; 2]| {
            let [x, y] = self.frame.at(at);
            [x, y, size[0], size[1]]
        };
        let mut boxes: Vec<_> = self
            .kind
            .groups()
            .iter()
            .flat_map(|&(setting, _, _, choices)| choices.map(|(_, y, on)| (SETUP_NAMES[setting][usize::from(!on)], place([20.0, y], RADIO_HIT))))
            .collect();
        if let Some((at, width, _)) = self.kind.edit() {
            boxes.push(("party_name_box", place(at, [width, EDIT_HEIGHT])));
        }
        boxes.push(("ok", place(self.ok_at(), DIALOG_BUTTON)));
        boxes.push(("cancel", place(self.cancel_at(), DIALOG_BUTTON)));
        boxes
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
        caret: bool,
        pointer: ([f32; 2], bool),
    ) {
        let message = |id: usize| messages.get(id).map_or("", String::as_str).trim_end();
        let at = |local: [f32; 2]| self.frame.at(local);
        let [width, height] = self.kind.size();
        self.frame.draw_title(layer, chrome, message(self.kind.title()), pointer.0);
        layer.sprites.push((&chrome.white, at([0.0, TITLE_HEIGHT]), [width, height - TITLE_HEIGHT]));
        for x in [0.0, width - 1.0] {
            layer.sprites.push((&chrome.border, at([x, TITLE_HEIGHT]), [1.0, height - TITLE_HEIGHT]));
        }
        layer.sprites.push((&chrome.border, at([0.0, height - 1.0]), [width, 1.0]));
        for &(setting, label, y, choices) in self.kind.groups() {
            layer.texts.push((message(label).to_owned(), at([10.0, y]), [0, 0, 0]));
            for (words, y, on) in choices {
                let radio = &assets.radio[usize::from(self.settings.get(setting) == on)];
                layer.sprites.push((radio, at([20.0, y]), radio.size));
                layer.texts.push((message(words).to_owned(), at([20.0 + radio.size[0] + 4.0, y - 1.0]), [0, 0, 0]));
            }
        }
        if let Some((box_at, box_width, label)) = self.kind.edit() {
            if let Some((label, label_at)) = label {
                layer.texts.push((message(label).to_owned(), at(label_at), [0, 0, 0]));
            }
            layer.sprites.push((&assets.edit, at(box_at), [box_width, EDIT_HEIGHT]));
            let shown = window::fit(font, &self.name, box_width - 8.0);
            let caret_x = font.width(&shown);
            layer.texts.push((shown, at([box_at[0] + 4.0, box_at[1] + 3.0]), [0, 0, 0]));
            if self.typing && caret {
                layer.sprites.push((&chrome.border, at([box_at[0] + 4.0 + caret_x, box_at[1] + 3.0]), [1.0, 14.0]));
            }
        }
        let hit = self.hit(pointer.0);
        let look = |target: DialogHit| usize::from(hit == Some(target)) + usize::from(hit == Some(target) && pointer.1);
        layer.sprites.push((&ok[look(DialogHit::Ok)], at(self.ok_at()), DIALOG_BUTTON));
        layer.sprites.push((&cancel[look(DialogHit::Cancel)], at(self.cancel_at()), DIALOG_BUTTON));
    }
}

/// A job's icon in the party window, alive or dead: its path (an offline member's is that path with "|grey").
pub fn job_icon(job: u16, dead: bool) -> String {
    format!("{UI}{PARTY_ART}icon_jobs_{job}{}.bmp", if dead { "_die" } else { "" })
}

/// An offline member's icon: each pixel the mean of its channels, at least 120; the magenta key kept.
pub fn greyed(mut image: RgbaImage) -> RgbaImage {
    for pixel in image.pixels_mut() {
        let [r, g, b, a] = pixel.0;
        if a == 0 || [r, g, b] == [255, 0, 255] {
            continue;
        }
        let grey = ((u16::from(r) + u16::from(g) + u16::from(b)) / 3).max(120) as u8;
        pixel.0 = [grey, grey, grey, a];
    }
    image
}

/// Loads the job icons the window shows into `icons` (an offline member's greyed).
pub fn load_icons(icons: &mut HashMap<String, Option<Texture>>, gpu: &Gpu, grf: &Grf, party: Option<&Party>) {
    for member in party.map_or(&[][..], |party| &party.members) {
        let path = job_icon(member.info.job, member.dead);
        if member.info.online {
            icons.entry(path.clone()).or_insert_with(|| gpu.try_load_texture(grf, &path, false).ok());
        } else {
            icons.entry(format!("{path}|grey")).or_insert_with(|| crate::render::load_image(grf, &path).ok().map(|image| gpu.texture(&greyed(image), false)));
        }
    }
}

/// The window's title: "Party(name)", "Party" before a name is known, or "Friend(n/40)".
pub fn title(messages: &[String], friends: Option<usize>, party: Option<&str>) -> String {
    let message = |id: usize| messages.get(id).map_or("", String::as_str);
    match (friends, party) {
        (Some(count), _) => format!("{}({count}/{MOST_FRIENDS})", message(FRIENDS_MESSAGE)),
        (None, Some(name)) if !name.is_empty() => format!("{}({name})", message(TITLE_MESSAGE)),
        _ => message(TITLE_MESSAGE).to_owned(),
    }
}

/// Whether a member's row shows the HP bar: ours always; another's only online and on screen.
pub fn shows_bar(account: u32, me: u32, online: bool, on_screen: bool) -> bool {
    account == me || (online && on_screen)
}

/// Who's looking at the party window: our account, our HP and max, whether a member is on screen, and the maps'
/// display names.
pub type Whose<'f> = (u32, [i32; 2], &'f dyn Fn(u32) -> bool, &'f dyn Fn(&str) -> String);

pub struct PartyWindow {
    pub frame: Frame,
    /// The friends tab is open (else the party's); the friend picked.
    pub friends: bool,
    pub picked: Option<usize>,
    /// The size mode (0: 273×326, 1: 243×179), and rows scrolled past.
    pub mode: usize,
    scroll: usize,
    /// Each mode's grip rows once its corner grip has been used, and the grip while held.
    grip_rows: [Option<usize>; 2],
    grip: window::Grip,
    /// The lock: only it and the agency take a press while it's on.
    pub locked: bool,
}

impl Default for PartyWindow {
    fn default() -> Self {
        Self {
            frame: Frame::new(POSITION, MODES[0].0[0]),
            friends: false,
            picked: None,
            mode: 0,
            scroll: 0,
            grip_rows: [None; 2],
            grip: window::Grip::default(),
            locked: false,
        }
    }
}

/// Where a row's parts go, window-local, for a window `width` wide with rows `height` tall and the content offset
/// `[mx, my]`, its top at `top`: the icon, the crown, the level, the HP bar, the status icon; and how wide the name
/// may be.
struct RowPlaces {
    icon: [f32; 2],
    crown: [f32; 2],
    level: [f32; 2],
    bar: [f32; 2],
    status: [f32; 2],
    name_width: f32,
}

fn row_places(width: f32, height: f32, [mx, my]: [f32; 2], top: f32) -> RowPlaces {
    RowPlaces {
        icon: [mx, top + my],
        crown: [mx + 2.0, top + my - 8.0],
        level: [mx + 28.0, top + my + 3.0],
        bar: [47.0, top + (0.76 * height).trunc()],
        status: [(0.8 * width).trunc(), top + (0.65 * height).trunc()],
        name_width: (0.8 * width - 80.0).trunc(),
    }
}

impl PartyWindow {
    /// The mode's width, and its height: the mode's own, or 58 + 23·k once its grip has been used.
    pub fn size(&self) -> [f32; 2] {
        let [width, height] = MODES[self.mode].0;
        [width, self.grip_rows[self.mode].map_or(height, |rows| LIST_UNDER + rows as f32 * GRIP_ROW)]
    }

    /// Each mode's grip rows, 0 where its grip hasn't been used (kept with the window places).
    pub fn grip_rows(&self) -> [usize; 2] {
        self.grip_rows.map(|rows| rows.unwrap_or(0))
    }

    /// Takes saved grip rows (0: the mode's own height), clamped to 2–12.
    pub fn set_grip_rows(&mut self, rows: [usize; 2]) {
        self.grip_rows = rows.map(|rows| (rows > 0).then(|| rows.clamp(*GRIP_ROWS.start(), *GRIP_ROWS.end())));
    }

    /// The corner grip on screen while the window is open (for the test driver).
    pub fn grip_box(&self) -> Option<[f32; 4]> {
        let [width, height] = self.size();
        let [x, y] = self.frame.at([width - window::GRIP, height - window::GRIP]);
        (self.frame.open && !self.frame.minimized).then_some([x, y, window::GRIP, window::GRIP])
    }

    /// Whether `cursor` is on the corner grip.
    pub fn on_grip(&self, cursor: [f32; 2]) -> bool {
        let size = self.size();
        self.frame.open && !self.frame.minimized && self.frame.local(cursor, size[1]).is_some_and(|local| window::on_grip(size, local))
    }

    /// Held by the corner grip at `cursor`.
    pub fn start_resize(&mut self, cursor: [f32; 2]) {
        self.grip.start(cursor, self.size());
    }

    /// While held, the bottom follows the pointer from the press, a 23 px row at a time (rounding toward the least);
    /// returns whether it's held.
    pub fn resize_to(&mut self, cursor: [f32; 2]) -> bool {
        let Some([_, asked]) = self.grip.asked(cursor) else { return false };
        let least = *GRIP_ROWS.start();
        let rows = least + window::steps(asked, LIST_UNDER + least as f32 * GRIP_ROW, GRIP_ROW, GRIP_ROWS.end() - least);
        if self.grip_rows[self.mode] != Some(rows) {
            self.grip_rows[self.mode] = Some(rows);
            self.scroll = 0;
        }
        true
    }

    pub fn stop_resize(&mut self) -> bool {
        self.grip.stop()
    }

    fn row_height(&self) -> f32 {
        if self.friends { FRIEND_ROW } else { MODES[self.mode].1 }
    }

    /// Rows that fit, for the test driver.
    pub fn shown_rows(&self) -> usize {
        self.visible_rows()
    }

    /// Rows that fit: (h − 58) / H.
    fn visible_rows(&self) -> usize {
        ((self.size()[1] - LIST_UNDER) / self.row_height()) as usize
    }

    pub fn height(&self) -> f32 {
        self.size()[1]
    }

    /// The title bar's minimize button switches the size mode instead of folding the window.
    pub fn click_title(&mut self, hit: TitleHit) {
        if hit == TitleHit::Minimize {
            self.set_mode(1 - self.mode);
        } else {
            self.frame.click_title(hit);
        }
    }

    pub fn set_mode(&mut self, mode: usize) {
        self.mode = mode.min(1);
        self.frame.set_width(self.size()[0]);
        self.scroll = 0;
    }

    /// The wheel over the rows.
    pub fn scroll_by(&mut self, rows: i32, count: usize) {
        let most = count.saturating_sub(self.visible_rows());
        self.scroll = (self.scroll as i32 + rows).clamp(0, most as i32) as usize;
    }

    fn row_top(&self, row: usize) -> f32 {
        TITLE_HEIGHT + row as f32 * self.row_height()
    }

    /// The buttons shown: for friends, whispering and dropping; for the party the agency, setup and the lock, then
    /// making a party without one, or inviting and leaving within one.
    fn buttons(&self, party: Option<&Party>) -> Vec<Button> {
        if self.friends {
            return vec![Button::Whisper, Button::Delete, Button::FriendSetup];
        }
        let mut buttons = vec![Button::Agency, Button::Setup, Button::Lock(self.locked)];
        match party {
            None => buttons.push(Button::Create),
            Some(_) => buttons.extend([Button::Invite, Button::Leave, Button::Sort]),
        }
        buttons
    }

    /// Whether a press on `button` does anything: with the lock on, only the lock and the agency do.
    pub fn takes(&self, button: Button) -> bool {
        !self.locked || matches!(button, Button::Lock(_) | Button::Agency)
    }

    /// A button's place on the bar: the party's from x 5, 26 apart; the friends' as before.
    fn button_at(&self, place: usize) -> [f32; 2] {
        let left = if self.friends { 7.0 } else { 5.0 };
        [left + place as f32 * BUTTON[0], self.size()[1] - BAR_ABOVE]
    }

    /// The footer's tabs: friends at x 4, 48 wide; party at x 64.
    // ponytail: the party tab's hit is 48 wide; drawn, it's its text's width + 13.
    fn tab_at(friends: bool) -> [f32; 2] {
        [if friends { 4.0 } else { 64.0 }, 48.0]
    }

    /// The members or friends shown, with their rows.
    fn shown<'p, T>(&self, all: &'p [T]) -> impl Iterator<Item = (usize, &'p T)> {
        all.iter().skip(self.scroll).take(self.visible_rows()).enumerate()
    }

    /// The buttons (`setup`, `invite`, `leave`, ...) and each member's row (`member:<name>`) on screen, for the test
    /// driver.
    pub fn button_boxes(&self, party: Option<&Party>) -> Vec<(String, [f32; 4])> {
        if !self.frame.open || self.frame.minimized {
            return Vec::new();
        }
        let place = |local: [f32; 2], size: [f32; 2]| {
            let [x, y] = self.frame.at(local);
            [x, y, size[0], size[1]]
        };
        let mut boxes: Vec<(String, [f32; 4])> =
            self.buttons(party).into_iter().enumerate().map(|(index, button)| (button.name().to_owned(), place(self.button_at(index), BUTTON))).collect();
        let members = party.filter(|_| !self.friends).map_or(&[][..], |party| &party.members);
        let width = self.size()[0];
        boxes.extend(
            self.shown(members).map(|(row, member)| (format!("member:{}", member.info.name), place([0.0, self.row_top(row)], [width, self.row_height()]))),
        );
        boxes.push(("party_mode".to_owned(), place(self.frame.minimize_button(), window::BUTTON_SIZE)));
        boxes
    }

    pub fn hit(&self, cursor: [f32; 2], party: Option<&Party>, friends: &[Friend]) -> Option<Hit> {
        let [width, height] = self.size();
        let local = self.frame.local(cursor, height)?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(Hit::Title(hit));
        }
        if window::on_grip([width, height], local) {
            return Some(Hit::Resize);
        }
        for (place, button) in self.buttons(party).into_iter().enumerate() {
            if contains(self.button_at(place), BUTTON, local) {
                return Some(Hit::Button(button));
            }
        }
        let footer = height - FOOTER_ABOVE;
        for friends_tab in [true, false] {
            let [x, w] = Self::tab_at(friends_tab);
            if contains([x, footer], [w, FOOTER_HEIGHT], local) {
                return Some(Hit::Tab(friends_tab));
            }
        }
        let list_bottom = TITLE_HEIGHT + self.visible_rows() as f32 * self.row_height();
        if local[1] >= TITLE_HEIGHT && local[1] < list_bottom && local[0] < width - SCROLLBAR {
            let row = ((local[1] - TITLE_HEIGHT) / self.row_height()) as usize + self.scroll;
            if self.friends && row < friends.len() {
                return Some(Hit::Friend(row));
            }
            if let Some(member) = party.filter(|_| !self.friends).and_then(|party| party.members.get(row)) {
                return Some(Hit::Member(member.info.account));
            }
        }
        Some(Hit::Window)
    }

    /// The window. `on_screen` tells whether a member's character is on screen; `our_hp` is the player's HP and max.
    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &self,
        layer: &mut Layer<'a>,
        tooltip: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        icons: &'a HashMap<String, Option<Texture>>,
        font: &Font,
        messages: &[String],
        party: Option<&Party>,
        friends: &[Friend],
        (me, our_hp, on_screen, map_names): Whose<'_>,
        small: &[u32],
        pointer: ([f32; 2], bool),
    ) {
        if !self.frame.open {
            return;
        }
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        let at = |local: [f32; 2]| self.frame.at(local);
        let [width, height] = self.size();
        let title = title(messages, self.friends.then_some(friends.len()), party.map(|party| party.name.as_str()));
        self.frame.draw_title(layer, chrome, &title, pointer.0);
        // Mode 1's size button turns it back.
        if self.mode == 1 {
            let place = self.frame.minimize_button();
            let hovered = contains(at(place), window::BUTTON_SIZE, pointer.0);
            let image = &assets.maximum[usize::from(hovered) + usize::from(hovered && pointer.1)];
            layer.sprites.push((image, at(place), image.size));
        }
        if self.frame.minimized {
            return;
        }
        let hit = self.hit(pointer.0, party, friends);
        // The white body, outlined, from under the title to 38 above the bottom.
        let body = height - BODY_UNDER - TITLE_HEIGHT;
        layer.sprites.push((&assets.body_border, at([0.0, TITLE_HEIGHT]), [width, body]));
        layer.sprites.push((&chrome.white, at([1.0, TITLE_HEIGHT]), [width - 2.0, body - 1.0]));
        let row_height = self.row_height();

        // Friends: each name, the picked one in blue.
        if self.friends {
            for (row, friend) in self.shown(friends) {
                let top = self.row_top(row);
                if self.picked == Some(row + self.scroll) {
                    layer.sprites.push((&assets.picked, at([1.0, top]), [width - 2.0, row_height]));
                }
                layer.texts.push((friend.name.clone(), at([18.0, top + 5.0]), if friend.online { FRIEND } else { OFFLINE_TEXT }));
            }
        }
        // Each member: the job icon, the crown, "Lv.N" and "Name(Map)", the HP bar where it shows, the status icon.
        let members = if self.friends { &[][..] } else { party.map_or(&[][..], |party| &party.members) };
        for (row, member) in self.shown(members) {
            let top = self.row_top(row);
            let info = &member.info;
            let places = row_places(width, row_height, MODES[self.mode].2, top);
            // A member with a small window has a band on their row.
            if small.contains(&info.account) {
                layer.sprites.push((&assets.has_small, at([0.0, top + 2.0]), [width, row_height]));
            }
            if hit == Some(Hit::Member(info.account)) {
                layer.sprites.push((&assets.hover, at([0.0, top + 2.0]), [width, row_height]));
            }
            let icon = job_icon(info.job, member.dead);
            let key = if info.online { icon } else { format!("{icon}|grey") };
            if let Some(Some(icon)) = icons.get(&key) {
                layer.sprites.push((icon, at(places.icon), [ICON, ICON]));
            }
            if info.leader {
                layer.sprites.push((&assets.crown, at(places.crown), assets.crown.size));
            }
            let text = if info.online { LEVEL_TEXT } else { OFFLINE_TEXT };
            let level = format!("Lv.{}", info.level);
            layer.texts.push((level.clone(), at(places.level), text));
            let map = map_names(&info.map);
            let whole = if info.online && !map.is_empty() { format!("{}({map})", info.name) } else { info.name.clone() };
            let name = window::fit(font, &whole, places.name_width);
            let color = if info.online {
                let [r, g, b, _] = crate::minimap::party_color(member.color);
                [r, g, b]
            } else {
                OFFLINE_TEXT
            };
            let name_at = [places.level[0] + font.width(&level) + 5.0, places.level[1]];
            layer.texts.push((name.clone(), at(name_at), color));
            if name != whole && hit == Some(Hit::Member(info.account)) {
                window::tooltip(tooltip, chrome, font, &format!("{level} {whole}"), offset(pointer.0, [12.0, 12.0]), [255; 3]);
            }
            let hp = if info.account == me { Some(our_hp) } else { member.hp };
            if shows_bar(info.account, me, info.online, on_screen(info.account))
                && let Some([hp, max]) = hp
            {
                let [border, background, fill, low] = &assets.bar_colors;
                let rows = [(i64::from(hp), i64::from(max), fill, low)];
                for (texture, [x, y, w, h]) in crate::overhead::gage(&rows, border, background, crate::overhead::party_fill) {
                    layer.sprites.push((texture, at([places.bar[0] + x, places.bar[1] + y]), [w, h]));
                }
                layer.texts.push((format!("{hp}/{max}"), at([places.bar[0] + 66.0, places.bar[1] - 2.0]), [0, 0, 0]));
            }
            let status = &assets.status[if info.account == me {
                0
            } else if info.online {
                1
            } else {
                2
            }];
            layer.sprites.push((status, at(places.status), STATUS));
        }
        // The scrollbar when the rows don't all fit. ponytail: a plain track and thumb in the window's colours.
        let count = if self.friends { friends.len() } else { members.len() };
        let shown = self.visible_rows();
        if count > shown {
            let track = height - LIST_UNDER;
            layer.sprites.push((&assets.body_border, at([width - SCROLLBAR, TITLE_HEIGHT]), [SCROLLBAR, track]));
            let thumb = (track * shown as f32 / count as f32).max(10.0);
            let top = TITLE_HEIGHT + (track - thumb) * self.scroll as f32 / (count - shown) as f32;
            layer.sprites.push((&assets.picked, at([width - SCROLLBAR + 2.0, top]), [SCROLLBAR - 4.0, thumb]));
        }

        // The button bar: its ends and the middle tiled between, then the buttons.
        let bar_top = height - BAR_ABOVE;
        let [left, middle, right] = &assets.bar;
        layer.sprites.push((left, at([0.0, bar_top]), [left.size[0], BAR_HEIGHT]));
        layer.sprites.push((middle, at([left.size[0], bar_top]), [width - left.size[0] - right.size[0], BAR_HEIGHT]));
        layer.sprites.push((right, at([width - right.size[0], bar_top]), [right.size[0], BAR_HEIGHT]));
        for (place, button) in self.buttons(party).into_iter().enumerate() {
            let look = match hit {
                Some(Hit::Button(hovered)) if hovered == button => 1 + usize::from(pointer.1),
                _ => 0,
            };
            layer.sprites.push((&assets.button(button)[look], at(self.button_at(place)), BUTTON));
            if look > 0 {
                window::tooltip(tooltip, chrome, font, message(button.message()), offset(pointer.0, [12.0, 12.0]), [255; 3]);
            }
        }
        // The footer: the two tabs (the open one in black), and the party's count in its box.
        // ponytail: under the button bar the window's white body, outlined, behind the tabs; the tabs are words only,
        // the open one black, the other grey.
        let under = bar_top + BAR_HEIGHT;
        layer.sprites.push((&assets.body_border, at([0.0, under]), [width, height - under]));
        layer.sprites.push((&chrome.white, at([1.0, under]), [width - 2.0, height - under - 1.0]));
        let footer = height - FOOTER_ABOVE;
        for (friends_tab, label) in [(true, FRIENDS_MESSAGE), (false, TITLE_MESSAGE)] {
            let color = if friends_tab == self.friends { [0, 0, 0] } else { OFFLINE_TEXT };
            layer.texts.push((message(label).to_owned(), at([Self::tab_at(friends_tab)[0] + 6.0, footer - 1.0]), color));
        }
        if !self.friends && !members.is_empty() {
            layer.texts.push((message(CAP_MESSAGE).to_owned(), at([width - 96.0, footer]), [0, 0, 0]));
            layer.sprites.push((&assets.member_count, at([width - 58.0, footer - 2.0]), assets.member_count.size));
            layer.texts.push((format!("{}/{MOST_IN_PARTY}", members.len()), at([width - 51.0, footer]), [255; 3]));
        }
        layer.sprites.push((&assets.resize, at([width - window::GRIP, height - window::GRIP]), assets.resize.size));
    }
}

/// A click on a small member window.
/// The small party windows' grid: the screen in 177×55 cells from (0, 0), column by column; only the cells wholly
/// inside x 0..W and y trunc(0.3·H)..H − 165 are used.
#[derive(Debug, Clone, PartialEq)]
pub struct Grid {
    rows: usize,
    columns: usize,
    /// The used cells, in cell order.
    pub used: Vec<usize>,
}

impl Grid {
    pub fn new([width, height]: [f32; 2]) -> Self {
        let rows = (height / CELL[1]).floor().max(1.0) as usize;
        let columns = (width / CELL[0]).floor().max(1.0) as usize;
        let (top, bottom) = ((0.3 * height).trunc(), height - 165.0);
        let used = (0..rows * columns)
            .filter(|&cell| {
                let [x, y] = Self::corner(rows, cell);
                x + CELL[0] <= width && y >= top && y + CELL[1] <= bottom
            })
            .collect();
        Self { rows, columns, used }
    }

    fn corner(rows: usize, cell: usize) -> [f32; 2] {
        [CELL[0] * (cell / rows) as f32, CELL[1] * (cell % rows) as f32]
    }

    /// A cell's top left on screen.
    pub fn at(&self, cell: usize) -> [f32; 2] {
        Self::corner(self.rows, cell)
    }

    /// The cell nearest a window's top left (rounding at half a cell, clamped to the grid), or `None` when that cell
    /// isn't a used one.
    pub fn nearest(&self, [x, y]: [f32; 2]) -> Option<usize> {
        let column = ((x / CELL[0]).round().max(0.0) as usize).min(self.columns - 1);
        let row = ((y / CELL[1]).round().max(0.0) as usize).min(self.rows - 1);
        let cell = column * self.rows + row;
        self.used.contains(&cell).then_some(cell)
    }

    /// The first used cell no window holds.
    // ponytail: in cell order.
    pub fn first_free(&self, taken: &[usize]) -> Option<usize> {
        self.used.iter().copied().find(|cell| !taken.contains(cell))
    }
}

/// One member's small window, in its grid cell.
#[derive(Debug, Clone, PartialEq)]
pub struct MemberWindow {
    pub account: u32,
    pub cell: usize,
}

/// The small party windows.
#[derive(Debug, Default)]
pub struct MemberWindows {
    pub open: Vec<MemberWindow>,
    /// A party row held until the button is let go.
    held: Option<u32>,
    /// A small window being moved: whose, where in it the pointer grabbed, and where it is now.
    moving: Option<(u32, [f32; 2], [f32; 2])>,
    /// A window let go outside the used cells: "In the last row" is to be shown.
    pub last_row: bool,
}

impl MemberWindows {
    /// A press on a member's row in the party window.
    pub fn press_row(&mut self, account: u32) {
        self.held = Some(account);
    }

    /// A press anywhere on a small window grabs it.
    pub fn press_window(&mut self, account: u32, cursor: [f32; 2], grid: &Grid) {
        let Some(index) = self.open.iter().position(|window| window.account == account) else { return };
        let window = self.open.remove(index);
        let at = grid.at(window.cell);
        self.open.push(window);
        self.moving = Some((account, [cursor[0] - at[0], cursor[1] - at[1]], at));
    }

    /// Follows the pointer while a small window is held, with no snapping.
    pub fn drag_to(&mut self, cursor: [f32; 2]) {
        if let Some((_, grab, at)) = &mut self.moving {
            *at = [cursor[0] - grab[0], cursor[1] - grab[1]];
        }
    }

    /// Lets go. A party row let go outside the party window (`outside`) gives that member a window in the first free
    /// cell; a small window snaps to the nearest cell (swapping with a window there), or goes back to its own when
    /// that cell isn't used. Returns whether a row or a window was held, so the release is not also a click.
    pub fn release(&mut self, outside: bool, grid: &Grid) -> bool {
        if let Some(account) = self.held.take() {
            if outside {
                self.add(account, grid);
            }
            return true;
        }
        let Some((account, _, at)) = self.moving.take() else { return false };
        match grid.nearest(at) {
            Some(cell) => self.move_to(account, cell),
            None => self.last_row = true,
        }
        true
    }

    /// `account`'s window to `cell`; a window already there takes the moved one's cell.
    fn move_to(&mut self, account: u32, cell: usize) {
        let Some(from) = self.open.iter().find(|window| window.account == account).map(|window| window.cell) else { return };
        for window in &mut self.open {
            if window.account == account {
                window.cell = cell;
            } else if window.cell == cell {
                window.cell = from;
            }
        }
    }

    /// A window for `account` in the first free cell; nothing when they have one or no cell is free.
    pub fn add(&mut self, account: u32, grid: &Grid) {
        if self.has(account) {
            return;
        }
        let taken: Vec<usize> = self.open.iter().map(|window| window.cell).collect();
        if let Some(cell) = grid.first_free(&taken) {
            self.open.push(MemberWindow { account, cell });
        }
    }

    pub fn has(&self, account: u32) -> bool {
        self.open.iter().any(|window| window.account == account)
    }

    /// Every window back in the used cells from the first, in their cell order.
    pub fn sort(&mut self, grid: &Grid) {
        let mut order: Vec<usize> = (0..self.open.len()).collect();
        order.sort_by_key(|&index| self.open[index].cell);
        for (index, cell) in order.into_iter().zip(&grid.used) {
            self.open[index].cell = *cell;
        }
    }

    pub fn close(&mut self, account: u32) {
        self.open.retain(|window| window.account != account);
        if self.moving.is_some_and(|(moving, ..)| moving == account) {
            self.moving = None;
        }
    }

    /// Drops windows whose member is gone. `None` is leaving the party or logging out: all of them go.
    pub fn retain(&mut self, party: Option<&Party>) {
        let Some(party) = party else {
            self.open.clear();
            self.held = None;
            self.moving = None;
            return;
        };
        let still = |account: u32| party.members.iter().any(|member| member.info.account == account);
        self.open.retain(|window| still(window.account));
        self.held = self.held.filter(|&account| still(account));
        self.moving = self.moving.filter(|&(account, ..)| still(account));
    }

    /// The saved windows of members in the party come back in their cells (the first free one when theirs is taken or
    /// no longer used).
    pub fn restore(&mut self, saved: &[(u32, usize)], party: &Party, grid: &Grid) {
        for &(account, cell) in saved {
            if self.has(account) || !party.members.iter().any(|member| member.info.account == account) {
                continue;
            }
            if grid.used.contains(&cell) && !self.open.iter().any(|window| window.cell == cell) {
                self.open.push(MemberWindow { account, cell });
            } else {
                self.add(account, grid);
            }
        }
    }

    /// Each window's member and cell, a line each, as saved for the character.
    pub fn saved(&self) -> String {
        self.open.iter().map(|window| format!("{} {}\n", window.account, window.cell)).collect()
    }

    /// Where a window is drawn: its cell, or under the pointer while moved.
    fn position(&self, window: &MemberWindow, grid: &Grid) -> [f32; 2] {
        match self.moving {
            Some((account, _, at)) if account == window.account => at,
            _ => grid.at(window.cell),
        }
    }

    /// The topmost small window under the pointer.
    pub fn hit(&self, cursor: [f32; 2], grid: &Grid) -> Option<u32> {
        self.open.iter().rev().find(|window| contains(self.position(window, grid), MINI_SIZE, cursor)).map(|window| window.account)
    }

    /// `PartyHp:<name>` for each open window, in list order.
    pub fn names(&self, party: Option<&Party>) -> Vec<String> {
        let Some(party) = party else { return Vec::new() };
        self.open
            .iter()
            .filter_map(|window| {
                party.members.iter().find(|member| member.info.account == window.account).map(|member| format!("PartyHp:{}", member.info.name))
            })
            .collect()
    }

    /// Each small window, for the test driver.
    pub fn button_boxes(&self, party: Option<&Party>, grid: &Grid) -> Vec<(String, [f32; 4])> {
        let Some(party) = party else { return Vec::new() };
        self.open
            .iter()
            .filter_map(|window| {
                let member = party.members.iter().find(|member| member.info.account == window.account)?;
                let [x, y] = self.position(window, grid);
                Some((format!("party_hp:{}", member.info.name), [x, y, MINI_SIZE[0], MINI_SIZE[1]]))
            })
            .collect()
    }

    /// Each small window: the background (three times over online, once offline), the job icon and crown, "Lv.N
    /// Name(Map)" cut to 104 px (whole in a tooltip), and the HP bar where the party window shows one.
    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &self,
        layer: &mut Layer<'a>,
        tooltip: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        icons: &'a HashMap<String, Option<Texture>>,
        font: &Font,
        party: Option<&Party>,
        (me, our_hp, on_screen, map_names): Whose<'_>,
        grid: &Grid,
        pointer: [f32; 2],
    ) {
        let Some(party) = party else { return };
        for window in &self.open {
            let Some(member) = party.members.iter().find(|member| member.info.account == window.account) else { continue };
            let info = &member.info;
            let at = self.position(window, grid);
            let place = |local: [f32; 2]| offset(at, local);
            for _ in 0..if info.online { 3 } else { 1 } {
                layer.sprites.push((&assets.mini_bg, at, assets.mini_bg.size));
            }
            let icon = job_icon(info.job, member.dead);
            let key = if info.online { icon } else { format!("{icon}|grey") };
            if let Some(Some(icon)) = icons.get(&key) {
                layer.sprites.push((icon, place(if info.leader { MINI_LEADER_ICON } else { MINI_ICON }), [ICON, ICON]));
            }
            if info.leader {
                let crown = if info.online { &assets.crown } else { &assets.crown_grey };
                layer.sprites.push((crown, place(MINI_CROWN), crown.size));
            }
            let whole = mini_line(info.level, &info.name, &map_names(&info.map));
            let line = window::fit(font, &whole, MINI_TEXT_WIDTH);
            layer.texts.push((line.clone(), place(MINI_TEXT), if info.online { [255; 3] } else { OFFLINE }));
            if line != whole && contains(at, MINI_SIZE, pointer) {
                window::tooltip(tooltip, chrome, font, &whole, offset(pointer, [12.0, 12.0]), [255; 3]);
            }
            let hp = if info.account == me { Some(our_hp) } else { member.hp };
            if shows_bar(info.account, me, info.online, on_screen(info.account))
                && let Some([hp, max]) = hp
            {
                let [border, background, fill, low] = &assets.bar_colors;
                let bar = place(MINI_BAR);
                layer.sprites.push((border, bar, MINI_BAR_SIZE));
                layer.sprites.push((background, offset(bar, [1.0, 1.0]), [MINI_BAR_SIZE[0] - 2.0, MINI_BAR_SIZE[1] - 2.0]));
                if let Some(width) = crate::overhead::party_fill_width(i64::from(hp), i64::from(max), MINI_BAR_SIZE[0]) {
                    let color = if hp * 4 < max { low } else { fill };
                    layer.sprites.push((color, offset(bar, [1.0, 1.0]), [width, MINI_BAR_SIZE[1] - 2.0]));
                }
            }
        }
    }
}

/// A small window's line: "Lv.N Name(Map)", the map left out when not known.
fn mini_line(level: u16, name: &str, map: &str) -> String {
    if map.is_empty() { format!("Lv.{level} {name}") } else { format!("Lv.{level} {name}({map})") }
}

/// The saved list read back: a member's account and cell a line; a line it can't read is skipped.
pub fn parse_saved(text: &str) -> Vec<(u32, usize)> {
    text.lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            Some((words.next()?.parse().ok()?, words.next()?.parse().ok()?))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(account: u32, leader: bool) -> PartyMember {
        PartyMember { account, name: format!("M{account}"), leader, online: true, ..Default::default() }
    }

    #[test]
    fn words_the_party_lines() {
        assert_eq!(
            (0..=19).map(invite_result_line).collect::<Vec<_>>(),
            [
                Some((80, Tone::Bad)),
                Some((2060, Tone::Bad)),
                Some((2061, Tone::Good)),
                Some((83, Tone::Bad)),
                Some((608, Tone::Bad)),
                Some((1324, Tone::Bad)),
                None,
                Some((71, Tone::Bad)),
                Some((1388, Tone::Bad)),
                Some((1871, Tone::Bad)),
                Some((3027, Tone::Bad)),
                Some((3226, Tone::Bad)),
                None,
                None,
                None,
                None,
                None,
                None,
                Some((3634, Tone::Bad)),
                None,
            ]
        );
        assert_eq!([0, 1, 2, 3, 4].map(made_line), [Some((77, Tone::Good)), Some((78, Tone::Bad)), Some((79, Tone::Bad)), Some((1387, Tone::Bad)), None]);
        assert_eq!(setup_lines(0, false, false), [(291, 292, 287, None), (291, 293, 287, None), (291, 738, 287, None)]);
        assert_eq!(setup_lines(1, true, true), [(291, 292, 288, None), (291, 293, 290, None), (291, 738, 288, None)]);
        assert_eq!(setup_lines(2, false, true)[0], (291, 292, 287, Some(1535)));
        assert_eq!(invite_result_text(" accepted your party invitation.", "Second"), "Second accepted your party invitation.");
        assert_eq!(
            invite_result_text("(%s) are currently in restricted map to join a party.", "Second"),
            "(Second) are currently in restricted map to join a party."
        );
        assert_eq!(invite_result_text("Character with this level cannot join the party", "Second"), "Character with this level cannot join the party");
        assert_eq!(invitation_text("QAParty", " Party has sent you an invitation."), "(QAParty) Party has sent you an invitation.");
        assert_eq!(invitation_at([1024.0, 768.0]), [392.0, 320.0]);
    }

    #[test]
    fn keeps_members_and_offers_the_leaders_buttons() {
        let mut party = Party { name: "P".into(), ..Default::default() };
        party.upsert(member(1, true));
        party.upsert(member(2, false));
        party.member(2).unwrap().hp = Some([5, 10]);
        party.upsert(PartyMember { level: 9, ..member(2, false) });
        assert_eq!(party.members.len(), 2);
        assert_eq!(party.member(2).unwrap().hp, Some([5, 10]));
        assert_eq!(party.leader(), Some(1));
        let mut window = PartyWindow::default();
        let party_bar = [Button::Agency, Button::Setup, Button::Lock(false), Button::Invite, Button::Leave, Button::Sort];
        assert_eq!(window.buttons(Some(&party)), party_bar);
        assert_eq!(window.buttons(None), [Button::Agency, Button::Setup, Button::Lock(false), Button::Create]);
        let xs = |window: &PartyWindow, party| (0..window.buttons(party).len()).map(|place| window.button_at(place)[0]).collect::<Vec<_>>();
        assert_eq!(xs(&window, Some(&party)), [5.0, 31.0, 57.0, 83.0, 109.0, 135.0]);
        assert_eq!(xs(&window, None), [5.0, 31.0, 57.0, 83.0]);
        assert!(party_bar.iter().all(|&button| window.takes(button)));
        window.locked = true;
        assert_eq!(window.buttons(Some(&party))[2], Button::Lock(true));
        assert_eq!((Button::Lock(false).image(), Button::Lock(true).image()), ("mesbtn_07", "mesbtn_06"));
        assert_eq!((Button::Lock(false).message(), Button::Lock(true).message(), Button::Agency.message()), (1627, 1626, 3514));
        let taken: Vec<Button> = window.buttons(Some(&party)).into_iter().filter(|&button| window.takes(button)).collect();
        assert_eq!(taken, [Button::Agency, Button::Lock(true)]);
        window.friends = true;
        assert_eq!(window.buttons(None), [Button::Whisper, Button::Delete, Button::FriendSetup]);
    }

    #[test]
    fn decides_how_to_leave() {
        let on = |account: u32, leader: bool, map: &str| PartyMember { map: map.into(), ..member(account, leader) };
        let off = |account: u32| PartyMember { online: false, ..member(account, false) };
        let party_of = |members: Vec<PartyMember>| {
            let mut party = Party::default();
            members.into_iter().for_each(|info| party.upsert(info));
            party
        };
        // Leading: the first other online member on our map, in list order.
        let party = party_of(vec![on(1, true, "prontera.gat"), off(2), on(3, false, "geffen.gat"), on(4, false, "prontera.gat"), on(5, false, "prontera.gat")]);
        assert_eq!(leave_decision(&party, 1, "prontera"), Leaving::HandOver(4));
        assert_eq!(leave_decision(&party, 1, "payon"), Leaving::Delegate);
        assert_eq!(leave_decision(&party, 4, "prontera"), Leaving::Leave, "a member just leaves");
        assert_eq!(leave_decision(&party_of(vec![on(1, true, "prontera.gat"), off(2)]), 1, "prontera"), Leaving::AskDisband);
        assert_eq!(leave_decision(&party_of(vec![on(1, true, "prontera.gat")]), 1, "prontera"), Leaving::Leave, "alone");
    }

    #[test]
    fn offers_each_rows_menu() {
        use MemberChoice::*;
        assert_eq!(member_menu(1, 1, true, false, false), [Mail, Leave]);
        assert_eq!(member_menu(1, 2, true, false, false), [Mail, Whisper, Expel, Assign]);
        assert_eq!(member_menu(2, 1, false, false, false), [Mail, Whisper]);
        assert_eq!(member_menu(1, 2, true, false, true), [Mail, Whisper, Expel, Assign, AddSmall, RemoveSmall]);
        assert_eq!(member_menu(1, 2, true, true, true), [Mail, Whisper, Expel, Assign, RemoveSmall]);
        assert_eq!(member_menu(1, 1, true, true, true), [Mail, Leave, RemoveSmall]);
        assert_eq!([Mail, Leave, Whisper, Expel, Assign, AddSmall, RemoveSmall].map(MemberChoice::message), [2808, 96, 360, 97, 1532, 3101, 3102]);
    }

    #[test]
    fn casts_on_rows_by_the_rule() {
        const HEAL: u16 = 28;
        const BLESSING: u16 = 34;
        const BASH: u16 = 5;
        // Another member: a Heal-like skill always, a support one with Shift, an enemy one never.
        assert!(row_takes_skill(HEAL, true, false, false, true));
        assert!(!row_takes_skill(BLESSING, false, false, false, true));
        assert!(row_takes_skill(BLESSING, false, true, false, true));
        assert!(!row_takes_skill(BASH, false, true, false, true));
        // Our own row: support skills only; an offline row nothing.
        assert!(row_takes_skill(HEAL, true, false, true, true));
        assert!(!row_takes_skill(BASH, true, false, true, true));
        assert!(!row_takes_skill(HEAL, true, true, false, false));
    }

    #[test]
    fn sets_up_the_party() {
        let mut party = Party { name: "P".into(), pickup_shared: true, ..Default::default() };
        party.upsert(member(1, true));
        party.upsert(member(2, false));
        let mut setup = PartyDialog::new(DialogKind::Setup, Some(&party), true, [276.0, 200.0]);
        let at = |dialog: &PartyDialog, name: &str| {
            let [x, y, _, h] = dialog.button_boxes().into_iter().find(|(found, _)| *found == name).unwrap().1;
            // The radio's box runs past the window's edge: aim inside it.
            [x + 10.0, y + h / 2.0]
        };
        // Each choice, stacked, OK and Cancel by their boxes.
        for (name, hit) in [
            ("exp_each", DialogHit::Choice(0, false)),
            ("exp_even", DialogHit::Choice(0, true)),
            ("pickup_each", DialogHit::Choice(1, false)),
            ("pickup_party", DialogHit::Choice(1, true)),
            ("share_individual", DialogHit::Choice(2, false)),
            ("share_shared", DialogHit::Choice(2, true)),
            ("ok", DialogHit::Ok),
            ("cancel", DialogHit::Cancel),
        ] {
            assert_eq!(setup.hit(at(&setup, name)), Some(hit), "{name}");
        }
        let corner = |dialog: &PartyDialog, name: &str| {
            let [x, y, ..] = dialog.button_boxes().into_iter().find(|(found, _)| *found == name).unwrap().1;
            [x - 276.0, y - 200.0]
        };
        assert_eq!(corner(&setup, "exp_each"), [20.0, 42.0]);
        assert_eq!(corner(&setup, "share_shared"), [20.0, 192.0]);
        assert_eq!((corner(&setup, "ok"), corner(&setup, "cancel")), ([38.0, 221.0], [84.0, 221.0]));
        // Nothing changed: OK only closes.
        assert_eq!(setup.click(DialogHit::Ok), Some(DialogAction::Close));
        assert_eq!(setup.click(DialogHit::Choice(2, true)), None);
        assert_eq!(setup.click(DialogHit::Ok), Some(DialogAction::Send(Settings { exp_even: false, pickup: true, share: true })));
        assert_eq!(setup.click(DialogHit::Cancel), Some(DialogAction::Close));
        // A member's click is refused in the chat, and nothing moves.
        let mut member_setup = PartyDialog::new(DialogKind::Setup, Some(&party), false, [0.0, 0.0]);
        assert_eq!(member_setup.click(DialogHit::Choice(0, true)), Some(DialogAction::LeaderOnly));
        assert_eq!(member_setup.settings, party.settings());
        assert_eq!(member_setup.click(DialogHit::Ok), Some(DialogAction::Close));
    }

    #[test]
    fn creates_and_invites() {
        let mut create = PartyDialog::new(DialogKind::Create, None, false, [0.0, 0.0]);
        let at = |dialog: &PartyDialog, name: &str| {
            let [x, y, _, h] = dialog.button_boxes().into_iter().find(|(found, _)| *found == name).unwrap().1;
            [x + 10.0, y + h / 2.0]
        };
        assert_eq!(create.hit(at(&create, "party_name_box")), Some(DialogHit::Edit));
        assert_eq!(create.hit(at(&create, "pickup_party")), Some(DialogHit::Choice(1, true)));
        assert_eq!(create.hit(at(&create, "share_individual")), Some(DialogHit::Choice(2, false)));
        assert!(create.button_boxes().iter().all(|(name, _)| !name.starts_with("exp")));
        assert_eq!(create.click(DialogHit::Ok), Some(DialogAction::EmptyName(2073)));
        create.type_text("Crabs and a name far too long");
        assert_eq!(create.name, "Crabs and a name far to");
        create.name = "Crabs".into();
        assert_eq!(create.click(DialogHit::Ok), Some(DialogAction::Make("Crabs".into(), false, false)));
        create.click(DialogHit::Choice(1, true));
        create.click(DialogHit::Choice(2, true));
        assert_eq!(create.click(DialogHit::Ok), Some(DialogAction::Make("Crabs".into(), true, true)));
        let mut invite = PartyDialog::new(DialogKind::Invite, None, true, [0.0, 0.0]);
        assert_eq!(invite.hit(at(&invite, "party_name_box")), Some(DialogHit::Edit));
        assert_eq!(invite.hit(at(&invite, "ok")), Some(DialogHit::Ok));
        assert_eq!(invite.click(DialogHit::Ok), Some(DialogAction::EmptyName(2072)));
        invite.type_text("LaneA2");
        assert_eq!(invite.click(DialogHit::Ok), Some(DialogAction::Invite("LaneA2".into())));
        // Docked right of the party window, or at the game's place without it.
        assert_eq!(docked_at(Some(([300.0, 200.0], 273.0)), [1024.0, 768.0]), [575.0, 200.0]);
        assert_eq!(docked_at(None, [1024.0, 768.0]), [392.0, 192.0]);
    }

    #[test]
    fn names_the_buttons_and_rows_for_the_driver() {
        let mut party = Party::default();
        party.upsert(member(1, true));
        party.upsert(member(2, false));
        party.upsert(PartyMember { online: false, ..member(3, false) });
        let mut window = PartyWindow::default();
        window.frame.open = true;
        let names: Vec<String> = window.button_boxes(Some(&party)).into_iter().map(|(name, _)| name).collect();
        assert_eq!(names, ["agency", "setup", "lock", "invite", "leave", "sort", "member:M1", "member:M2", "member:M3", "party_mode"]);
    }

    #[test]
    fn lays_out_rows_in_both_modes() {
        // Mode 0: 273×326, 34 px rows from y 17, content at (5, 5).
        let row = row_places(273.0, 34.0, MODES[0].2, 17.0);
        assert_eq!((row.icon, row.crown, row.level), ([5.0, 22.0], [7.0, 14.0], [33.0, 25.0]));
        assert_eq!((row.bar, row.status, row.name_width), ([47.0, 42.0], [218.0, 39.0], 138.0));
        // Mode 1: 243×179, 27 px rows, content at (4, 1).
        let row = row_places(243.0, 27.0, MODES[1].2, 17.0);
        assert_eq!((row.icon, row.crown, row.level), ([4.0, 18.0], [6.0, 10.0], [32.0, 21.0]));
        assert_eq!((row.bar, row.status, row.name_width), ([47.0, 37.0], [194.0, 34.0], 114.0));
        // Seven rows fit at 326, the title button switches the mode and back.
        let mut window = PartyWindow::default();
        assert_eq!((window.visible_rows(), window.height()), (7, 326.0));
        window.click_title(TitleHit::Minimize);
        assert_eq!((window.mode, window.visible_rows(), window.height(), window.frame.minimized), (1, 4, 179.0, false));
        window.click_title(TitleHit::Minimize);
        assert_eq!(window.mode, 0);
        // The friends tab's rows are 23 px.
        window.friends = true;
        assert_eq!(window.visible_rows(), 11);
        // More rows than fit scroll, no further than the last.
        window.scroll_by(5, 15);
        assert_eq!(window.scroll, 4);
        window.scroll_by(-9, 15);
        assert_eq!(window.scroll, 0);
    }

    #[test]
    fn resizes_by_the_corner_grip_in_23_px_rows() {
        let mut window = PartyWindow::default();
        window.frame.open = true;
        let corner = window.frame.at([273.0 - 5.0, 326.0 - 5.0]);
        assert!(window.on_grip(corner));
        assert_eq!(window.hit(corner, None, &[]), Some(Hit::Resize));
        // 326 asks 11.6 rows: 11 (311); 23 px down, 12 (334), no further; far up, 2 (104).
        window.start_resize(corner);
        assert!(window.resize_to(corner));
        assert_eq!((window.size(), window.grip_rows()), ([273.0, 311.0], [11, 0]));
        window.resize_to([corner[0], corner[1] + 40.0]);
        assert_eq!(window.size()[1], 334.0);
        window.resize_to([corner[0], corner[1] + 500.0]);
        assert_eq!(window.size()[1], 334.0);
        window.resize_to([corner[0] + 90.0, corner[1] - 500.0]);
        assert_eq!((window.size(), window.visible_rows()), ([273.0, 104.0], 1));
        assert!(window.stop_resize() && !window.resize_to(corner));
        // Mode 1 keeps its own height until its grip is used; the rows come back as saved, clamped.
        window.set_mode(1);
        assert_eq!(window.size(), [243.0, 179.0]);
        window.set_grip_rows([40, 0]);
        assert_eq!(window.grip_rows(), [12, 0]);
        window.set_grip_rows([0, 5]);
        assert_eq!((window.size(), window.visible_rows()), ([243.0, 173.0], 4));
        window.set_mode(0);
        assert_eq!(window.size(), [273.0, 326.0]);
    }

    #[test]
    fn shows_bars_colours_titles_and_grey_icons() {
        // The bar: ours always; another's online and on screen only.
        assert!(shows_bar(1, 1, true, false));
        assert!(shows_bar(2, 1, true, true));
        assert!(!shows_bar(2, 1, true, false), "off screen");
        assert!(!shows_bar(2, 1, false, true), "offline");
        // Colours come from the place at the add and stay when someone leaves.
        let mut party = Party::default();
        for account in 1..=3 {
            party.upsert(member(account, account == 1));
        }
        party.members.retain(|member| member.info.account != 2);
        party.upsert(member(4, false));
        party.upsert(PartyMember { level: 50, ..member(3, false) });
        assert_eq!(party.members.iter().map(|member| (member.info.account, member.color)).collect::<Vec<_>>(), [(1, 0), (3, 2), (4, 2)]);
        // The title.
        let messages: Vec<String> = (0..=103)
            .map(|id| match id {
                102 => "Friend".to_owned(),
                103 => "Party".to_owned(),
                _ => String::new(),
            })
            .collect();
        assert_eq!(title(&messages, None, Some("Crabs")), "Party(Crabs)");
        assert_eq!(title(&messages, None, Some("")), "Party");
        assert_eq!(title(&messages, None, None), "Party");
        assert_eq!(title(&messages, Some(3), None), "Friend(3/40)");
        // Offline icons: each pixel the mean, at least 120; magenta and clear ones kept.
        let image = RgbaImage::from_fn(4, 1, |x, _| image::Rgba([[30, 60, 90, 255], [200, 210, 220, 255], [255, 0, 255, 255], [9, 9, 9, 0]][x as usize]));
        let grey = greyed(image);
        assert_eq!(grey.pixels().map(|pixel| pixel.0).collect::<Vec<_>>(), [[120, 120, 120, 255], [210, 210, 210, 255], [255, 0, 255, 255], [9, 9, 9, 0]]);
        assert!(job_icon(4257, true).ends_with("renewalparty\\icon_jobs_4257_die.bmp"));
    }

    #[test]
    fn lays_out_the_small_windows_grid() {
        let grid = Grid::new([1024.0, 768.0]);
        assert_eq!(grid.used.len(), 25);
        let first: Vec<[f32; 2]> = grid.used.iter().take(6).map(|&cell| grid.at(cell)).collect();
        assert_eq!(first, [[0.0, 275.0], [0.0, 330.0], [0.0, 385.0], [0.0, 440.0], [0.0, 495.0], [177.0, 275.0]]);
        assert_eq!(grid.at(*grid.used.last().unwrap()), [708.0, 495.0]);
        let small = Grid::new([800.0, 600.0]);
        let ys: Vec<f32> = small.used.iter().take(3).map(|&cell| small.at(cell)[1]).collect();
        assert_eq!((small.used.len(), ys), (12, vec![220.0, 275.0, 330.0]));
        assert_eq!(small.at(small.used[11]), [531.0, 330.0]);
        // The nearest cell rounds at half a cell and clamps to the grid; a cell outside the used rows is none.
        assert_eq!(grid.nearest([180.0, 330.0]).map(|cell| grid.at(cell)), Some([177.0, 330.0]));
        assert_eq!(grid.nearest([88.0, 302.0]).map(|cell| grid.at(cell)), Some([0.0, 275.0]));
        assert_eq!(grid.nearest([89.0, 303.0]).map(|cell| grid.at(cell)), Some([177.0, 330.0]));
        assert_eq!(grid.nearest([5000.0, 400.0]).map(|cell| grid.at(cell)), Some([708.0, 385.0]));
        assert_eq!(grid.nearest([0.0, 10.0]), None);
        assert_eq!(grid.nearest([-50.0, 700.0]), None);
        assert_eq!(grid.first_free(&[grid.used[0], grid.used[2]]), Some(grid.used[1]));
    }

    #[test]
    fn small_windows_snap_swap_and_sort() {
        let grid = Grid::new([1024.0, 768.0]);
        let mut windows = MemberWindows::default();
        windows.press_row(2);
        // Let go still inside the party window: no small window.
        assert!(windows.release(false, &grid));
        assert!(windows.open.is_empty());
        windows.press_row(2);
        assert!(windows.release(true, &grid));
        assert_eq!(windows.open, [MemberWindow { account: 2, cell: grid.used[0] }]);
        assert_eq!(windows.hit([10.0, 280.0], &grid), Some(2));
        // One a member; Add takes the next free cell.
        windows.add(2, &grid);
        windows.add(3, &grid);
        assert_eq!(windows.open.iter().map(|window| (window.account, window.cell)).collect::<Vec<_>>(), [(2, grid.used[0]), (3, grid.used[1])]);
        // Dragged to about (180, 330): it snaps to (177, 330).
        windows.press_window(2, [10.0, 280.0], &grid);
        windows.drag_to([190.0, 335.0]);
        assert_eq!(windows.hit([185.0, 340.0], &grid), Some(2));
        assert!(windows.release(false, &grid));
        assert_eq!(grid.at(windows.open.iter().find(|window| window.account == 2).unwrap().cell), [177.0, 330.0]);
        // Onto the other's cell: they swap.
        windows.press_window(2, [180.0, 335.0], &grid);
        windows.drag_to([5.0, 335.0]);
        windows.release(false, &grid);
        let cell_of = |windows: &MemberWindows, account: u32| windows.open.iter().find(|window| window.account == account).unwrap().cell;
        assert_eq!((grid.at(cell_of(&windows, 2)), grid.at(cell_of(&windows, 3))), ([0.0, 330.0], [177.0, 330.0]));
        // Let go at the top of the screen: "In the last row", and it goes back.
        windows.press_window(2, [5.0, 335.0], &grid);
        windows.drag_to([300.0, 10.0]);
        windows.release(false, &grid);
        assert!(windows.last_row);
        assert_eq!(grid.at(cell_of(&windows, 2)), [0.0, 330.0]);
        // Sort packs them from the first cell, in their cell order.
        windows.sort(&grid);
        assert_eq!((cell_of(&windows, 2), cell_of(&windows, 3)), (grid.used[0], grid.used[1]));
        windows.close(2);
        assert_eq!(windows.open.len(), 1);
    }

    #[test]
    fn keeps_the_small_windows_for_next_time() {
        let grid = Grid::new([1024.0, 768.0]);
        let mut party = Party::default();
        [member(1, true), member(2, false), member(3, false)].into_iter().for_each(|info| party.upsert(info));
        let mut windows = MemberWindows::default();
        windows.add(2, &grid);
        windows.add(3, &grid);
        windows.sort(&grid);
        windows.press_window(3, [10.0, 335.0], &grid);
        windows.drag_to([360.0, 440.0]);
        windows.release(false, &grid);
        let saved = windows.saved();
        assert_eq!(parse_saved(&saved), [(2, grid.used[0]), (3, 2 * 13 + 8)]);
        assert_eq!(parse_saved("broken\n9 x\n4 7\n"), [(4, 7)]);
        // Read back on the party list: in their cells, members only.
        let mut again = MemberWindows::default();
        again.restore(&[(2, grid.used[0]), (3, 2 * 13 + 8), (9, grid.used[3])], &party, &grid);
        assert_eq!(again.open, windows.open);
        again.restore(&parse_saved(&saved), &party, &grid);
        assert_eq!(again.open.len(), 2, "no second window for a member");
    }

    #[test]
    fn leaving_removes_the_window() {
        let grid = Grid::new([1024.0, 768.0]);
        let mut party = Party::default();
        party.upsert(member(1, true));
        party.upsert(member(2, false));
        let mut windows = MemberWindows::default();
        windows.add(2, &grid);
        windows.add(1, &grid);
        assert_eq!(windows.names(Some(&party)), ["PartyHp:M2", "PartyHp:M1"]);
        party.members.retain(|member| member.info.account != 2);
        windows.retain(Some(&party));
        assert_eq!(windows.open.iter().map(|window| window.account).collect::<Vec<_>>(), vec![1]);
        // A row held while they leave is not opened on release.
        windows.press_row(2);
        windows.retain(Some(&party));
        assert!(!windows.release(true, &grid));
        assert!(windows.open.iter().all(|window| window.account != 2));
        // Leaving the party, or logging out, drops the rest.
        windows.retain(None);
        assert!(windows.open.is_empty());
        assert!(windows.names(None).is_empty());
        assert_eq!(mini_line(9, "M1", "Prontera"), "Lv.9 M1(Prontera)");
        assert_eq!(mini_line(9, "M1", ""), "Lv.9 M1");
    }
}
