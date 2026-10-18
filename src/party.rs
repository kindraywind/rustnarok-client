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
pub const MEMBER_SIZE: [f32; 2] = [120.0, 32.0];
pub const MEMBER_HP: [f32; 2] = [100.0, 5.0];
const MEMBER_HP_AT: [f32; 2] = [8.0, 20.0];
const CLOSE_AT: [f32; 2] = [106.0, 3.0];
const MOST_FRIENDS: usize = 40;
const FRIEND_ROW: f32 = 23.0;
const FRIEND: [u8; 3] = [0x10, 0x25, 0x50];
const FRIENDS_MESSAGE: usize = 102;
const POSITION: [f32; 2] = [300.0, 200.0];
/// "Party", "Cap", and what the buttons do.
const TITLE_MESSAGE: usize = 103;
const CAP_MESSAGE: usize = 3231;
const NAME: [u8; 3] = [0x33, 0x33, 0x33];
const LEADER: [u8; 3] = [0x08, 0x31, 0x7B];
const MAP: [u8; 3] = [0x77, 0x77, 0x77];
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

    /// Whether the player (`me`) may hand the party to the member `account`: the leader, to another online member.
    pub fn offers_leader(&self, me: u32, account: u32) -> bool {
        self.leader() == Some(me) && account != me && self.members.iter().any(|member| member.info.account == account && member.info.online)
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

/// What the window's buttons do.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Button {
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
}

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
    hp: [Texture; 4],
    grey: Texture,
    picked: Texture,
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
            buttons: [Button::Create, Button::Invite, Button::Leave, Button::Setup, Button::Whisper, Button::Delete, Button::FriendSetup]
                .into_iter()
                .map(|button| (button, ["", "_a", "_b"].map(|look| image(format!("{basic}{}{look}.bmp", button.image())))))
                .collect(),
            radio: ["radiobtn_off", "radiobtn_on"].map(|name| image(format!("{UI}{name}.bmp"))),
            // Border, background, healthy and low.
            hp: [[0x11, 0x11, 0x11, 0xFF], [0x33, 0x33, 0x33, 0xFF], [0x10, 0xEF, 0x21, 0xFF], [0xFF, 0x00, 0x00, 0xFF]].map(|color| gpu.solid(color)),
            // ponytail: an offline small window fills its bar with the offline grey. A client bitmap would replace it.
            grey: gpu.solid([OFFLINE[0], OFFLINE[1], OFFLINE[2], 0xFF]),
            picked: gpu.solid([0x73, 0x9C, 0xEF, 0xFF]),
        }
    }

    fn button(&self, button: Button) -> &[Texture; 3] {
        &self.buttons.iter().find(|(which, _)| *which == button).unwrap().1
    }
}

// ponytail: the setup window's size and place (by the party window) are estimates.
const SETUP_WIDTH: f32 = 230.0;
const SETUP_ROW: f32 = 40.0;
const SETUP_TOP: f32 = TITLE_HEIGHT + 8.0;
const SETUP_NOTE: f32 = SETUP_TOP + 3.0 * SETUP_ROW;
const SETUP_HEIGHT: f32 = SETUP_NOTE + 46.0;
/// The two choices' left edges.
const CHOICES: [f32; 2] = [20.0, 120.0];
const SETUP_BUTTON: [f32; 2] = [42.0, 20.0];
const SETUP_TITLE: usize = 291;
/// Each row's label, its first choice (on) and its second (off).
const SETUP_ROWS: [(usize, usize, usize); 3] = [(292, 288, 287), (293, 290, 289), (738, 740, 739)];
/// The choices' names for the test driver, in the same order.
const SETUP_NAMES: [[&str; 2]; 3] = [["exp_even", "exp_each"], ["pickup_party", "pickup_each"], ["share_shared", "share_individual"]];
const LEADER_ONLY: usize = 294;

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum SetupHit {
    Title(TitleHit),
    /// A row (0 EXP, 1 pickup, 2 distribution) and which choice: true the first (on).
    Choice(usize, bool),
    Ok,
    Cancel,
    Window,
}

/// What a click in the setup window comes to.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum SetupAction {
    Close,
    /// Send these settings, and close.
    Send(Settings),
}

/// The party setup window (the Setup button): the three settings, changeable by the leader only.
pub struct Setup {
    pub frame: Frame,
    pub settings: Settings,
    start: Settings,
    leader: bool,
}

impl Setup {
    pub fn new(party: &Party, leader: bool, beside: [f32; 2]) -> Self {
        let mut frame = Frame::new([(beside[0] - SETUP_WIDTH - 4.0).max(0.0), beside[1]], SETUP_WIDTH);
        frame.open = true;
        Self { frame, settings: party.settings(), start: party.settings(), leader }
    }

    fn choice_at(row: usize, first: bool) -> [f32; 2] {
        [CHOICES[usize::from(!first)], SETUP_TOP + row as f32 * SETUP_ROW + 18.0]
    }

    fn ok_at() -> [f32; 2] {
        [SETUP_WIDTH - 2.0 * SETUP_BUTTON[0] - 14.0, SETUP_HEIGHT - SETUP_BUTTON[1] - 4.0]
    }

    fn cancel_at() -> [f32; 2] {
        [SETUP_WIDTH - SETUP_BUTTON[0] - 8.0, SETUP_HEIGHT - SETUP_BUTTON[1] - 4.0]
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<SetupHit> {
        let local = self.frame.local(cursor, SETUP_HEIGHT)?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(SetupHit::Title(hit));
        }
        for row in 0..3 {
            for first in [true, false] {
                if contains(Self::choice_at(row, first), [90.0, 16.0], local) {
                    return Some(SetupHit::Choice(row, first));
                }
            }
        }
        if contains(Self::ok_at(), SETUP_BUTTON, local) {
            return Some(SetupHit::Ok);
        }
        if contains(Self::cancel_at(), SETUP_BUTTON, local) {
            return Some(SetupHit::Cancel);
        }
        Some(SetupHit::Window)
    }

    /// A choice moves only for the leader; OK sends what changed.
    pub fn click(&mut self, hit: SetupHit) -> Option<SetupAction> {
        match hit {
            SetupHit::Choice(row, first) if self.leader => self.settings.set(row, first),
            SetupHit::Title(TitleHit::Close) | SetupHit::Cancel => return Some(SetupAction::Close),
            SetupHit::Ok if self.leader && self.settings != self.start => return Some(SetupAction::Send(self.settings)),
            SetupHit::Ok => return Some(SetupAction::Close),
            _ => {}
        }
        None
    }

    /// The choices and buttons on screen (for the test driver).
    pub fn button_boxes(&self) -> Vec<(&'static str, [f32; 4])> {
        let place = |at: [f32; 2], size: [f32; 2]| {
            let [x, y] = self.frame.at(at);
            [x, y, size[0], size[1]]
        };
        let mut boxes: Vec<_> = (0..3)
            .flat_map(|row| [true, false].map(|first| (SETUP_NAMES[row][usize::from(!first)], place(Self::choice_at(row, first), [90.0, 16.0]))))
            .collect();
        boxes.push(("ok", place(Self::ok_at(), SETUP_BUTTON)));
        boxes.push(("cancel", place(Self::cancel_at(), SETUP_BUTTON)));
        boxes
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &self,
        layer: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        (ok, cancel): (&'a [Texture; 3], &'a [Texture; 3]),
        messages: &[String],
        pointer: ([f32; 2], bool),
    ) {
        let message = |id: usize| messages.get(id).map_or("", String::as_str).trim_end();
        let at = |local: [f32; 2]| self.frame.at(local);
        self.frame.draw_title(layer, chrome, message(SETUP_TITLE), pointer.0);
        layer.sprites.push((&chrome.white, at([0.0, TITLE_HEIGHT]), [SETUP_WIDTH, SETUP_HEIGHT - TITLE_HEIGHT]));
        for x in [0.0, SETUP_WIDTH - 1.0] {
            layer.sprites.push((&chrome.border, at([x, TITLE_HEIGHT]), [1.0, SETUP_HEIGHT - TITLE_HEIGHT]));
        }
        layer.sprites.push((&chrome.border, at([0.0, SETUP_HEIGHT - 1.0]), [SETUP_WIDTH, 1.0]));
        for (row, &(label, on, off)) in SETUP_ROWS.iter().enumerate() {
            layer.texts.push((message(label).to_owned(), at([10.0, SETUP_TOP + row as f32 * SETUP_ROW]), [0, 0, 0]));
            for (first, words) in [(true, on), (false, off)] {
                let corner = Self::choice_at(row, first);
                let radio = &assets.radio[usize::from(self.settings.get(row) == first)];
                layer.sprites.push((radio, at([corner[0], corner[1] + 2.0]), radio.size));
                layer.texts.push((message(words).to_owned(), at([corner[0] + radio.size[0] + 4.0, corner[1]]), [0, 0, 0]));
            }
        }
        if !self.leader {
            layer.texts.push((message(LEADER_ONLY).to_owned(), at([10.0, SETUP_NOTE]), [0xC0, 0x30, 0x30]));
        }
        let hit = self.hit(pointer.0);
        let look = |target: SetupHit| usize::from(hit == Some(target)) + usize::from(hit == Some(target) && pointer.1);
        layer.sprites.push((&ok[look(SetupHit::Ok)], at(Self::ok_at()), SETUP_BUTTON));
        layer.sprites.push((&cancel[look(SetupHit::Cancel)], at(Self::cancel_at()), SETUP_BUTTON));
    }
}

/// How wide the filled part of an HP bar `width` wide is. The party window and the small windows share it.
fn hp_fill(hp: i32, max: i32, width: f32) -> f32 {
    let share = if max > 0 { (hp as f32 / max as f32).clamp(0.0, 1.0) } else { 0.0 };
    (width * share).round()
}

/// The party window draws the low (red) fill under a quarter of the bar.
fn hp_low(hp: i32, max: i32) -> bool {
    max > 0 && (hp as f32 / max as f32) < 0.25
}

/// What a small window's bar shows. Offline is grey, whatever the last HP was.
#[derive(Debug, PartialEq)]
enum MemberBar {
    Empty,
    Hp(f32),
    Offline,
}

fn member_bar(online: bool, hp: Option<[i32; 2]>) -> MemberBar {
    if !online {
        // ponytail: the whole bar goes grey, not the last fill in grey. A client bitmap would say which.
        return MemberBar::Offline;
    }
    match hp {
        Some([hp, max]) => MemberBar::Hp(hp_fill(hp, max, MEMBER_HP[0])),
        None => MemberBar::Empty,
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
}

impl Default for PartyWindow {
    fn default() -> Self {
        Self { frame: Frame::new(POSITION, MODES[0].0[0]), friends: false, picked: None, mode: 0, scroll: 0 }
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
    pub fn size(&self) -> [f32; 2] {
        MODES[self.mode].0
    }

    fn row_height(&self) -> f32 {
        if self.friends { FRIEND_ROW } else { MODES[self.mode].1 }
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

    /// The buttons shown: for friends, whispering and dropping; making a party without one; inviting (the leader),
    /// leaving and setting it up within one.
    fn buttons(&self, party: Option<&Party>, me: u32) -> Vec<Button> {
        if self.friends {
            return vec![Button::Whisper, Button::Delete, Button::FriendSetup];
        }
        match party {
            None => vec![Button::Create],
            Some(party) if party.leader() == Some(me) => vec![Button::Setup, Button::Invite, Button::Leave],
            Some(_) => vec![Button::Setup, Button::Leave],
        }
    }

    /// A button's place on the bar. ponytail: the buttons keep their order from the bar's left end; their places are
    /// another issue.
    fn button_at(&self, place: usize) -> [f32; 2] {
        [7.0 + place as f32 * BUTTON[0], self.size()[1] - BAR_ABOVE]
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
    pub fn button_boxes(&self, party: Option<&Party>, me: u32) -> Vec<(String, [f32; 4])> {
        if !self.frame.open || self.frame.minimized {
            return Vec::new();
        }
        let place = |local: [f32; 2], size: [f32; 2]| {
            let [x, y] = self.frame.at(local);
            [x, y, size[0], size[1]]
        };
        let mut boxes: Vec<(String, [f32; 4])> = self
            .buttons(party, me)
            .into_iter()
            .enumerate()
            .map(|(index, button)| (format!("{button:?}").to_lowercase(), place(self.button_at(index), BUTTON)))
            .collect();
        let members = party.filter(|_| !self.friends).map_or(&[][..], |party| &party.members);
        let width = self.size()[0];
        boxes.extend(
            self.shown(members).map(|(row, member)| (format!("member:{}", member.info.name), place([0.0, self.row_top(row)], [width, self.row_height()]))),
        );
        boxes.push(("party_mode".to_owned(), place(self.frame.minimize_button(), window::BUTTON_SIZE)));
        boxes
    }

    pub fn hit(&self, cursor: [f32; 2], party: Option<&Party>, friends: &[Friend], me: u32) -> Option<Hit> {
        let [width, height] = self.size();
        let local = self.frame.local(cursor, height)?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(Hit::Title(hit));
        }
        for (place, button) in self.buttons(party, me).into_iter().enumerate() {
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
        let hit = self.hit(pointer.0, party, friends, me);
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
        for (place, button) in self.buttons(party, me).into_iter().enumerate() {
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
    }
}

/// A click on a small member window.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum MemberHit {
    Close,
    Body,
}

/// One member's small HP window, dragged out of the party list.
#[derive(Debug, Clone, PartialEq)]
pub struct MemberWindow {
    pub account: u32,
    /// Top left. ponytail: the drop point, then snapped like any other window near the screen edge.
    pub position: [f32; 2],
}

impl MemberWindow {
    fn hit(&self, cursor: [f32; 2]) -> Option<MemberHit> {
        let local = [cursor[0] - self.position[0], cursor[1] - self.position[1]];
        if !contains([0.0, 0.0], MEMBER_SIZE, local) {
            return None;
        }
        if contains(CLOSE_AT, window::BUTTON_SIZE, local) {
            return Some(MemberHit::Close);
        }
        Some(MemberHit::Body)
    }
}

/// The small windows dragged out of the party window.
/// ponytail: forgotten at logout (the game is dropped). Saving each account and place would keep them.
#[derive(Debug, Default)]
pub struct MemberWindows {
    pub open: Vec<MemberWindow>,
    /// A party row held until the button is let go.
    held: Option<u32>,
    /// A small window being moved, and where in it the pointer grabbed.
    moving: Option<(u32, [f32; 2])>,
}

impl MemberWindows {
    /// A press on a member's row in the party window.
    pub fn press_row(&mut self, account: u32) {
        self.held = Some(account);
    }

    /// A press on a small window's body grabs it. The close button does not.
    pub fn press_window(&mut self, account: u32, cursor: [f32; 2]) {
        let Some(index) = self.open.iter().position(|window| window.account == account) else { return };
        let window = self.open.remove(index);
        let grab = [cursor[0] - window.position[0], cursor[1] - window.position[1]];
        self.open.push(window);
        self.moving = Some((account, grab));
    }

    /// Follows the pointer while a small window is held.
    pub fn drag_to(&mut self, cursor: [f32; 2]) {
        let Some((account, grab)) = self.moving else { return };
        let Some(window) = self.open.iter_mut().find(|window| window.account == account) else { return };
        window.position = window::dragged_to([cursor[0] - grab[0], cursor[1] - grab[1]], MEMBER_SIZE);
    }

    /// Lets go. A party row let go outside the party window (`outside`) opens that member's window at `at`, or moves
    /// the one they already have. Returns whether a row or a window was held, so the release is not also a click.
    pub fn release(&mut self, outside: bool, at: [f32; 2]) -> bool {
        let held = self.held.take();
        let moving = self.moving.take().is_some();
        if let Some(account) = held {
            if outside {
                self.place(account, at);
            }
            return true;
        }
        moving
    }

    fn place(&mut self, account: u32, at: [f32; 2]) {
        let position = window::dragged_to(at, MEMBER_SIZE);
        if let Some(window) = self.open.iter_mut().find(|window| window.account == account) {
            window.position = position;
        } else {
            self.open.push(MemberWindow { account, position });
        }
    }

    pub fn close(&mut self, account: u32) {
        self.open.retain(|window| window.account != account);
        if self.moving.is_some_and(|(moving, _)| moving == account) {
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
        self.moving = self.moving.filter(|&(account, _)| still(account));
    }

    /// The topmost small window under the pointer.
    pub fn hit(&self, cursor: [f32; 2]) -> Option<(u32, MemberHit)> {
        self.open.iter().rev().find_map(|window| window.hit(cursor).map(|hit| (window.account, hit)))
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

    /// The body and close button of each small window, for the test driver.
    pub fn button_boxes(&self, party: Option<&Party>) -> Vec<(String, [f32; 4])> {
        let Some(party) = party else { return Vec::new() };
        let mut boxes = Vec::new();
        for window in &self.open {
            let Some(member) = party.members.iter().find(|member| member.info.account == window.account) else { continue };
            let [x, y] = window.position;
            boxes.push((format!("party_hp:{}", member.info.name), [x, y, MEMBER_SIZE[0], MEMBER_SIZE[1]]));
            let close = offset(window.position, CLOSE_AT);
            boxes.push((format!("party_hp_close:{}", member.info.name), [close[0], close[1], window::BUTTON_SIZE[0], window::BUTTON_SIZE[1]]));
        }
        boxes
    }

    /// Each dragged-out window. The bar reads `Member::hp`, which `MapEvent::PartyHp` (ZC_NOTIFY_HP_TO_GROUPM,
    /// 0x080E) fills for the party window too.
    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(&self, layer: &mut Layer<'a>, chrome: &'a Chrome, assets: &'a Assets, font: &Font, party: Option<&Party>, here: &str, pointer: [f32; 2]) {
        let Some(party) = party else { return };
        for window in &self.open {
            if let Some(member) = party.members.iter().find(|member| member.info.account == window.account) {
                draw_member(layer, chrome, assets, font, member, window.position, here, true, pointer);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_member<'a>(
    layer: &mut Layer<'a>,
    chrome: &'a Chrome,
    assets: &'a Assets,
    font: &Font,
    member: &Member,
    at: [f32; 2],
    here: &str,
    close: bool,
    pointer: [f32; 2],
) {
    let place = |local: [f32; 2]| offset(at, local);
    layer.sprites.push((&chrome.white, at, MEMBER_SIZE));
    for x in [0.0, MEMBER_SIZE[0] - 1.0] {
        layer.sprites.push((&chrome.border, place([x, 0.0]), [1.0, MEMBER_SIZE[1]]));
    }
    layer.sprites.push((&chrome.border, place([0.0, 0.0]), [MEMBER_SIZE[0], 1.0]));
    layer.sprites.push((&chrome.border, place([0.0, MEMBER_SIZE[1] - 1.0]), [MEMBER_SIZE[0], 1.0]));
    let info = &member.info;
    let color = if !info.online {
        OFFLINE
    } else if info.leader {
        LEADER
    } else {
        NAME
    };
    let map = info.map.trim_end_matches(".gat");
    // ponytail: the map shares the name's line when it isn't this map. A second line would need a taller window.
    let room = if close { CLOSE_AT[0] - 8.0 } else { MEMBER_SIZE[0] - 8.0 };
    let (name, elsewhere) = if !map.is_empty() && map != here {
        let name = window::fit(font, &info.name, room * 0.55);
        let used = font.width(&name) + 4.0;
        (name, Some(window::fit(font, map, (room - used).max(0.0))))
    } else {
        (window::fit(font, &info.name, room), None)
    };
    layer.texts.push((name.clone(), place([4.0, 3.0]), color));
    if let Some(elsewhere) = elsewhere {
        layer.texts.push((elsewhere, place([4.0 + font.width(&name) + 4.0, 3.0]), if info.online { MAP } else { OFFLINE }));
    }
    if close {
        let image = chrome.button(2, contains(place(CLOSE_AT), window::BUTTON_SIZE, pointer));
        layer.sprites.push((image, place(CLOSE_AT), image.size));
    }
    let [border, background, healthy, low] = &assets.hp;
    let bar = place(MEMBER_HP_AT);
    layer.sprites.push((border, bar, [MEMBER_HP[0] + 2.0, MEMBER_HP[1] + 2.0]));
    layer.sprites.push((background, offset(bar, [1.0, 1.0]), MEMBER_HP));
    match member_bar(info.online, member.hp) {
        MemberBar::Offline => layer.sprites.push((&assets.grey, offset(bar, [1.0, 1.0]), MEMBER_HP)),
        MemberBar::Hp(width) => {
            let [hp, max] = member.hp.unwrap_or([0, 0]);
            let fill = if hp_low(hp, max) { low } else { healthy };
            layer.sprites.push((fill, offset(bar, [1.0, 1.0]), [width, MEMBER_HP[1]]));
        }
        MemberBar::Empty => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(account: u32, leader: bool) -> PartyMember {
        PartyMember { account, name: format!("M{account}"), leader, online: true, ..Default::default() }
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
        assert_eq!(window.buttons(Some(&party), 1), [Button::Setup, Button::Invite, Button::Leave]);
        assert_eq!(window.buttons(Some(&party), 2), [Button::Setup, Button::Leave]);
        assert_eq!(window.buttons(None, 2), [Button::Create]);
        window.friends = true;
        assert_eq!(window.buttons(None, 2), [Button::Whisper, Button::Delete, Button::FriendSetup]);
    }

    #[test]
    fn sets_up_the_party() {
        let mut party = Party { name: "P".into(), pickup_shared: true, ..Default::default() };
        party.upsert(member(1, true));
        party.upsert(member(2, false));
        let mut setup = Setup::new(&party, true, [770.0, 180.0]);
        let at = |setup: &Setup, name: &str| {
            let [x, y, w, h] = setup.button_boxes().into_iter().find(|(found, _)| *found == name).unwrap().1;
            [x + w / 2.0, y + h / 2.0]
        };
        // Each choice, OK and Cancel by their boxes.
        for (name, hit) in [
            ("exp_even", SetupHit::Choice(0, true)),
            ("exp_each", SetupHit::Choice(0, false)),
            ("pickup_party", SetupHit::Choice(1, true)),
            ("pickup_each", SetupHit::Choice(1, false)),
            ("share_shared", SetupHit::Choice(2, true)),
            ("share_individual", SetupHit::Choice(2, false)),
            ("ok", SetupHit::Ok),
            ("cancel", SetupHit::Cancel),
        ] {
            assert_eq!(setup.hit(at(&setup, name)), Some(hit), "{name}");
        }
        // Nothing changed: OK only closes.
        assert_eq!(setup.click(SetupHit::Ok), Some(SetupAction::Close));
        assert_eq!(setup.click(SetupHit::Choice(2, true)), None);
        assert_eq!(setup.click(SetupHit::Ok), Some(SetupAction::Send(Settings { exp_even: false, pickup: true, share: true })));
        assert_eq!(setup.click(SetupHit::Cancel), Some(SetupAction::Close));

        // A member can't move them.
        let mut member_setup = Setup::new(&party, false, [770.0, 180.0]);
        member_setup.click(SetupHit::Choice(0, true));
        assert_eq!(member_setup.settings, party.settings());
        assert_eq!(member_setup.click(SetupHit::Ok), Some(SetupAction::Close));
    }

    #[test]
    fn offers_the_leader_menu_to_the_leader_only() {
        let mut party = Party::default();
        party.upsert(member(1, true));
        party.upsert(member(2, false));
        party.upsert(PartyMember { online: false, ..member(3, false) });
        assert!(party.offers_leader(1, 2));
        assert!(!party.offers_leader(1, 1), "not to the leader's own row");
        assert!(!party.offers_leader(2, 1), "not from a member");
        assert!(!party.offers_leader(1, 3), "not to someone offline");
        assert!(!party.offers_leader(1, 9), "not to a stranger");
        let mut window = PartyWindow::default();
        window.frame.open = true;
        let names: Vec<String> = window.button_boxes(Some(&party), 1).into_iter().map(|(name, _)| name).collect();
        assert_eq!(names, ["setup", "invite", "leave", "member:M1", "member:M2", "member:M3", "party_mode"]);
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
    fn drag_out_opens_outside_the_party_window_only() {
        let mut windows = MemberWindows::default();
        windows.press_row(2);
        // Let go still inside the party window: no small window.
        assert!(windows.release(false, [400.0, 300.0]));
        assert!(windows.open.is_empty());
        windows.press_row(2);
        assert!(windows.release(true, [400.0, 300.0]));
        assert_eq!(windows.open, vec![MemberWindow { account: 2, position: [400.0, 300.0] }]);
        // Dragging the same member out again moves that window, at the new drop point.
        windows.press_row(2);
        assert!(windows.release(true, [300.0, 240.0]));
        assert_eq!(windows.open, vec![MemberWindow { account: 2, position: [300.0, 240.0] }]);
        // A release back inside leaves it where it was.
        windows.press_row(2);
        assert!(windows.release(false, [10.0, 10.0]));
        assert_eq!(windows.open[0].position, [300.0, 240.0]);
        // Dragging the small window itself moves it by the grab.
        windows.press_window(2, [310.0, 250.0]);
        windows.drag_to([500.0, 320.0]);
        assert!(windows.release(true, [500.0, 320.0]));
        assert_eq!(windows.open[0].position, [490.0, 310.0]);
        assert_eq!(windows.open[0].hit([490.0 + 8.0, 310.0 + 8.0]), Some(MemberHit::Body));
        assert_eq!(windows.open[0].hit([490.0 + CLOSE_AT[0] + 2.0, 310.0 + CLOSE_AT[1] + 2.0]), Some(MemberHit::Close));
        windows.close(2);
        assert!(windows.open.is_empty());
    }

    #[test]
    fn leaving_removes_the_window() {
        let mut party = Party::default();
        party.upsert(member(1, true));
        party.upsert(member(2, false));
        let mut windows = MemberWindows::default();
        windows.press_row(2);
        windows.release(true, [400.0, 300.0]);
        windows.press_row(1);
        windows.release(true, [100.0, 120.0]);
        assert_eq!(windows.names(Some(&party)), ["PartyHp:M2", "PartyHp:M1"]);
        party.members.retain(|member| member.info.account != 2);
        windows.retain(Some(&party));
        assert_eq!(windows.open.iter().map(|window| window.account).collect::<Vec<_>>(), vec![1]);
        // A row held while they leave is not opened on release.
        windows.press_row(2);
        windows.retain(Some(&party));
        assert!(!windows.release(true, [20.0, 20.0]));
        assert!(windows.open.iter().all(|window| window.account != 2));
        // Leaving the party, or logging out, drops the rest.
        windows.retain(None);
        assert!(windows.open.is_empty());
        assert!(windows.names(None).is_empty());
    }

    #[test]
    fn hp_fill_width_matches_the_party_bar() {
        assert_eq!(hp_fill(50, 100, MEMBER_HP[0]), 50.0);
        assert_eq!(hp_fill(5, 10, 60.0), 30.0);
        assert_eq!(hp_fill(1, 100, MEMBER_HP[0]), 1.0);
        assert_eq!(hp_fill(0, 100, MEMBER_HP[0]), 0.0);
        assert_eq!(hp_fill(10, 0, MEMBER_HP[0]), 0.0);
        assert_eq!(hp_fill(-5, 100, MEMBER_HP[0]), 0.0);
        assert_eq!(hp_fill(150, 100, MEMBER_HP[0]), MEMBER_HP[0]);
        // The small window's bar is that width; offline is grey rather than a fill.
        assert_eq!(member_bar(true, Some([50, 100])), MemberBar::Hp(50.0));
        assert_eq!(member_bar(true, Some([10, 100])), MemberBar::Hp(10.0));
        assert_eq!(member_bar(true, None), MemberBar::Empty);
        assert_eq!(member_bar(false, Some([50, 100])), MemberBar::Offline);
        assert!(hp_low(10, 100));
        assert!(!hp_low(50, 100));
    }
}
