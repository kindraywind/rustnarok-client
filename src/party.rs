//! The party and friends: who's in them, and their window (Alt+Z): each member's level, name, map and HP, or each
//! friend's name; the buttons to make a party, invite, leave or set it up (or to talk to or drop a friend); and the
//! count at the bottom, with the tab picked.

use rustnarok_client::grf::Grf;
use rustnarok_client::net::{Friend, PartyMember};
use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};
use crate::window::{self, Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI, contains, offset};

// ponytail: a 240px list of 38px rows, mesbtn buttons and a btnbar footer; calibrate against
// the real client.
const WIDTH: f32 = 240.0;
const ROW: f32 = 38.0;
const LIST_HEIGHT: f32 = 120.0;
const BAR_HEIGHT: f32 = 20.0;
const FOOTER_HEIGHT: f32 = 21.0;
const BUTTON: [f32; 2] = [26.0, 20.0];
const HP_BAR: [f32; 2] = [75.0, 5.0];
const MOST: usize = 12;
const MOST_FRIENDS: usize = 40;
const FRIEND_ROW: f32 = 18.0;
const FRIEND: [u8; 3] = [0x10, 0x25, 0x50];
const FRIENDS_MESSAGE: usize = 102;
const POSITION: [f32; 2] = [770.0, 180.0];
/// "Party", "Cap", and what the buttons do.
const TITLE_MESSAGE: usize = 103;
const CAP_MESSAGE: usize = 3231;
const LEVEL: [u8; 3] = [0xE3, 0x75, 0x22];
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

    /// A member joined or changed: the entry is replaced, keeping what's known of their HP and place.
    pub fn upsert(&mut self, info: PartyMember) {
        match self.member(info.account) {
            Some(member) => member.info = info,
            None => self.members.push(Member { info, ..Member::default() }),
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
    bar: Texture,
    footer: Texture,
    /// Each button's normal, hovered and pressed images.
    buttons: Vec<(Button, [Texture; 3])>,
    radio: [Texture; 2],
    hp: [Texture; 4],
    picked: Texture,
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let basic = format!("{UI}basic_interface\\");
        let image = |name: String| gpu.load_texture(grf, &name, false);
        Self {
            bar: image(format!("{basic}mesbtn_mid.bmp")),
            footer: image(format!("{basic}btnbar_mid.bmp")),
            buttons: [Button::Create, Button::Invite, Button::Leave, Button::Setup, Button::Whisper, Button::Delete, Button::FriendSetup]
                .into_iter()
                .map(|button| (button, ["", "_a", "_b"].map(|look| image(format!("{basic}{}{look}.bmp", button.image())))))
                .collect(),
            radio: ["radiobtn_off", "radiobtn_on"].map(|name| image(format!("{UI}{name}.bmp"))),
            // Border, background, healthy and low.
            hp: [[0x11, 0x11, 0x11, 0xFF], [0x33, 0x33, 0x33, 0xFF], [0x10, 0xEF, 0x21, 0xFF], [0xFF, 0x00, 0x00, 0xFF]].map(|color| gpu.solid(color)),
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

pub struct PartyWindow {
    pub frame: Frame,
    /// The friends tab is open (else the party's); the friend picked.
    pub friends: bool,
    pub picked: Option<usize>,
}

impl Default for PartyWindow {
    fn default() -> Self {
        Self { frame: Frame::new(POSITION, WIDTH), friends: false, picked: None }
    }
}

impl PartyWindow {
    fn list_height(&self, party: Option<&Party>, friends: &[Friend]) -> f32 {
        let rows = if self.friends { friends.len() as f32 * FRIEND_ROW } else { party.map_or(0.0, |party| party.members.len() as f32 * ROW) };
        rows.max(LIST_HEIGHT)
    }

    pub fn height(&self, party: Option<&Party>, friends: &[Friend]) -> f32 {
        TITLE_HEIGHT + self.list_height(party, friends) + BAR_HEIGHT + FOOTER_HEIGHT
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

    /// The buttons (`setup`, `invite`, `leave`, ...) and each member's row (`member:<name>`) on screen, for the test
    /// driver.
    pub fn button_boxes(&self, party: Option<&Party>, friends: &[Friend], me: u32) -> Vec<(String, [f32; 4])> {
        if !self.frame.open || self.frame.minimized {
            return Vec::new();
        }
        let bar_top = TITLE_HEIGHT + self.list_height(party, friends);
        let place = |local: [f32; 2], size: [f32; 2]| {
            let [x, y] = self.frame.at(local);
            [x, y, size[0], size[1]]
        };
        let buttons = self
            .buttons(party, me)
            .into_iter()
            .enumerate()
            .map(|(index, button)| (format!("{button:?}").to_lowercase(), place([index as f32 * BUTTON[0], bar_top], BUTTON)));
        let members = party.filter(|_| !self.friends).map_or(&[][..], |party| &party.members);
        let rows = members
            .iter()
            .enumerate()
            .map(|(row, member)| (format!("member:{}", member.info.name), place([0.0, TITLE_HEIGHT + row as f32 * ROW], [WIDTH, ROW])));
        buttons.chain(rows).collect()
    }

    pub fn hit(&self, cursor: [f32; 2], party: Option<&Party>, friends: &[Friend], me: u32) -> Option<Hit> {
        let local = self.frame.local(cursor, self.height(party, friends))?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(Hit::Title(hit));
        }
        let bar_top = TITLE_HEIGHT + self.list_height(party, friends);
        for (place, button) in self.buttons(party, me).into_iter().enumerate() {
            if contains([place as f32 * BUTTON[0], bar_top], BUTTON, local) {
                return Some(Hit::Button(button));
            }
        }
        // The footer's two tabs, friends first.
        let footer_top = bar_top + BAR_HEIGHT;
        if local[1] >= footer_top && local[0] < 140.0 {
            return Some(Hit::Tab(local[0] < 70.0));
        }
        if local[1] < bar_top {
            let row = local[1] - TITLE_HEIGHT;
            if self.friends && ((row / FRIEND_ROW) as usize) < friends.len() {
                return Some(Hit::Friend((row / FRIEND_ROW) as usize));
            }
            if let Some(member) = party.filter(|_| !self.friends).and_then(|party| party.members.get((row / ROW) as usize)) {
                return Some(Hit::Member(member.info.account));
            }
        }
        Some(Hit::Window)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &self,
        layer: &mut Layer<'a>,
        tooltip: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        font: &Font,
        messages: &[String],
        party: Option<&Party>,
        friends: &[Friend],
        (me, map_names): (u32, &dyn Fn(&str) -> String),
        pointer: ([f32; 2], bool),
    ) {
        if !self.frame.open {
            return;
        }
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        let at = |local: [f32; 2]| self.frame.at(local);
        let title = if self.friends {
            format!("{} ({}/{MOST_FRIENDS})", message(FRIENDS_MESSAGE), friends.len())
        } else {
            party.map_or_else(|| message(TITLE_MESSAGE).to_owned(), |party| format!("{} ({})", message(TITLE_MESSAGE), party.name))
        };
        self.frame.draw_title(layer, chrome, &title, pointer.0);
        if self.frame.minimized {
            return;
        }
        let hit = self.hit(pointer.0, party, friends, me);
        let list_height = self.list_height(party, friends);
        layer.sprites.push((&chrome.white, at([0.0, TITLE_HEIGHT]), [WIDTH, list_height]));
        for x in [0.0, WIDTH - 1.0] {
            layer.sprites.push((&chrome.border, at([x, TITLE_HEIGHT]), [1.0, list_height]));
        }

        // Friends: each name, the picked one in blue.
        if self.friends {
            for (row, friend) in friends.iter().enumerate() {
                let top = TITLE_HEIGHT + row as f32 * FRIEND_ROW;
                if self.picked == Some(row) {
                    layer.sprites.push((&assets.picked, at([1.0, top]), [WIDTH - 2.0, FRIEND_ROW]));
                }
                layer.texts.push((friend.name.clone(), at([18.0, top + 2.0]), if friend.online { FRIEND } else { OFFLINE }));
            }
        }
        // Each member: level, name and map, then the HP bar and numbers while they're online.
        let members = if self.friends { &[][..] } else { party.map_or(&[][..], |party| &party.members) };
        for (row, member) in members.iter().enumerate() {
            let top = TITLE_HEIGHT + row as f32 * ROW;
            let info = &member.info;
            let mut x = 8.0;
            let level = format!("Lv {}", info.level);
            layer.texts.push((level.clone(), at([x, top + 4.0]), if info.online { LEVEL } else { OFFLINE }));
            x += font.width(&level) + 5.0;
            let name = window::fit(font, &info.name, 100.0);
            layer.texts.push((
                name.clone(),
                at([x, top + 4.0]),
                if !info.online {
                    OFFLINE
                } else if info.leader {
                    LEADER
                } else {
                    NAME
                },
            ));
            x += font.width(&name) + 5.0;
            let map = window::fit(font, &map_names(&info.map), WIDTH - x - 4.0);
            layer.texts.push((map, at([x, top + 4.0]), MAP));
            if info.online
                && let Some([hp, max]) = member.hp
            {
                let share = if max > 0 { (hp as f32 / max as f32).clamp(0.0, 1.0) } else { 0.0 };
                let bar = [8.0, top + 22.0];
                let [border, background, healthy, low] = &assets.hp;
                layer.sprites.push((border, at(bar), [HP_BAR[0] + 2.0, HP_BAR[1] + 2.0]));
                layer.sprites.push((background, at(offset(bar, [1.0, 1.0])), HP_BAR));
                layer.sprites.push((if share < 0.25 { low } else { healthy }, at(offset(bar, [1.0, 1.0])), [(HP_BAR[0] * share).round(), HP_BAR[1]]));
                layer.texts.push((format!("{hp}/{max}"), at([8.0 + HP_BAR[0] + 8.0, top + 18.0]), NAME));
            }
        }

        // The button bar, then the footer with the party tab picked and the count.
        let bar_top = TITLE_HEIGHT + list_height;
        layer.sprites.push((&assets.bar, at([0.0, bar_top]), [WIDTH, BAR_HEIGHT]));
        for (place, button) in self.buttons(party, me).into_iter().enumerate() {
            let look = match hit {
                Some(Hit::Button(hovered)) if hovered == button => 1 + usize::from(pointer.1),
                _ => 0,
            };
            let corner = at([place as f32 * BUTTON[0], bar_top]);
            layer.sprites.push((&assets.button(button)[look], corner, BUTTON));
            if look > 0 {
                window::tooltip(tooltip, chrome, font, message(button.message()), offset(pointer.0, [12.0, 12.0]), [255; 3]);
            }
        }
        let footer_top = bar_top + BAR_HEIGHT;
        layer.sprites.push((&assets.footer, at([0.0, footer_top]), [WIDTH, FOOTER_HEIGHT]));
        let text_top = footer_top + ((FOOTER_HEIGHT - font.line_height()) / 2.0).floor();
        let mut x = 6.0;
        for (label, on) in [(FRIENDS_MESSAGE, self.friends), (TITLE_MESSAGE, !self.friends)] {
            let radio = &assets.radio[usize::from(on)];
            layer.sprites.push((radio, at([x, footer_top + ((FOOTER_HEIGHT - radio.size[1]) / 2.0).floor()]), radio.size));
            x += radio.size[0] + 3.0;
            layer.texts.push((message(label).to_owned(), at([x, text_top]), [0, 0, 0]));
            x += font.width(message(label)) + 10.0;
        }
        if !self.friends {
            let count = format!("{} {}/{MOST}", message(CAP_MESSAGE), party.map_or(0, |party| party.members.len()));
            layer.texts.push((count.clone(), at([WIDTH - 10.0 - font.width(&count), text_top]), [0, 0, 0]));
        }
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
        let names: Vec<String> = window.button_boxes(Some(&party), &[], 1).into_iter().map(|(name, _)| name).collect();
        assert_eq!(names, ["setup", "invite", "leave", "member:M1", "member:M2", "member:M3"]);
    }
}
