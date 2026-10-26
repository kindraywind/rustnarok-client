//! Character select of 2018+ clients (select_character_ver3): fifteen character cards a page beside the selected
//! character's details, the deletion buttons and the Poring that starts the game.

use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rustnarok_client::grf::Grf;
use rustnarok_client::messages;
use rustnarok_client::net::Character;
use rustnarok_client::text::Font;

use crate::actor::{self, Placed, SpriteSet, Timing};
use crate::menu::{self, Menu};
use crate::render::{Gpu, Texture};
use crate::window::{self, Clicks};
use crate::{Layer, UI, contains, offset, screen};

// Places measured from make_character_ver2\bg_back2.tga, the client's own (blurred) picture of this screen, by
// blurring the parts' bitmaps the same way and fitting them to it.
const FRAME: [f32; 2] = [997.0, 626.0];
const COLUMNS: usize = 5;
pub const PER_PAGE: usize = 15;
const GRID: [f32; 2] = [10.0, 21.0];
const CELL: [f32; 2] = [157.0, 195.0];
/// The visible card inside a cell, what a click hits.
const CARD: ([f32; 2], [f32; 2]) = ([5.0, 5.0], [147.0, 183.0]);
/// Where a character stands in its card, and its job icon.
const FEET: [f32; 2] = [80.0, 137.0];
const JOB_ICON: [f32; 2] = [118.0, 14.0];
/// A deletion date: the day over the time, centred on x 79 of the card, red while waiting and blue once it has passed.
const DATE_CENTER: f32 = 79.0;
const DATE_LINES: [f32; 2] = [40.0, 52.0];
/// The strip at the bottom of a card that holds the name.
const NAME_STRIP: [f32; 2] = [159.0, 187.0];
const NAME_COLOR: [u8; 3] = [0x15, 0x15, 0x4A];
const COUNTING: [u8; 3] = [0xFF, 0x00, 0x00];
const COUNTED: [u8; 3] = [0x00, 0x00, 0xFF];
const INFO: [f32; 2] = [809.0, 26.0];
/// The selected character's values, window-local: the map from (868, 116), then job, base level, exp, HP, SP and the
/// six stats a row every 17 px, each in a 107 × 15 box.
const VALUES: [f32; 2] = [868.0, 116.0];
const VALUE_ROW: f32 = 17.0;
const VALUE_BOX: [f32; 2] = [107.0, 15.0];
const BUTTONS: [[f32; 2]; 2] = [[21.0, 296.0], [21.0, 325.0]];
/// Game Start in the info panel, (816, 407) window-local.
const START: [f32; 2] = [7.0, 381.0];
// ponytail: the label's size is estimated from the blurred picture.
/// "게임시작" on the Poring, its top 57 down the button: white, or #DC747F on the greyed button.
const START_LABEL_TOP: f32 = 57.0;
const START_LABEL_SIZE: f32 = 16.0;
const START_LABEL_COLORS: [[u8; 3]; 2] = [[0xDC, 0x74, 0x7F], [255; 3]];
/// "Character List" and the page line, window-local, centred on x 896.
const LIST_CENTER: f32 = 896.0;
const LIST_TITLE_TOP: f32 = 538.0;
const PAGE_LINE_TOP: f32 = 564.0;
const TITLE_COLOR: [u8; 3] = [0x57, 0x66, 0x91];
const ARROWS: [[f32; 2]; 2] = [[21.0, 532.0], [130.0, 532.0]];
/// The close X, 52 in from the right.
const CLOSE: [f32; 2] = [FRAME[0] - 52.0, 5.0];
/// The selected card's border cycles through its 8 bitmaps, each for 5 frames of a 60 frame a second screen.
// ponytail: a fixed 83 ms, whatever the frame rate.
const SELECT_FRAME: Duration = Duration::from_millis(83);

/// "%d월 %d일" and "%d시 %d분 %d초": a deletion's day and time.
const DATE_MESSAGE: usize = 3104;
const TIME_MESSAGE: usize = 3349;
const START_MESSAGE: usize = 3341;
const LIST_MESSAGE: usize = 3343;
const MOVE_CHOICE: usize = 2164;
const RENAME_CHOICE: usize = 2165;
/// "Ready to change character slot in.(%d)", the moves left, shown while picking where to move.
const MOVES_LEFT_MESSAGE: usize = 2025;

/// The buttons under the stats; which ones show depends on the selected card's deletion.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Button {
    /// Reserves the deletion at once, without a question.
    DeleteCharacter,
    CancelDeletion,
    Notice,
    /// Asks, then the birthdate.
    Delete,
}

impl Button {
    fn message(self) -> usize {
        match self {
            Button::DeleteCharacter => 3339,
            Button::CancelDeletion => 3340,
            Button::Notice => 3344,
            Button::Delete => 3342,
        }
    }
}

/// The buttons' labels.
const BUTTON_LABEL: [u8; 3] = [0x31, 0x31, 0x31];
/// Notice's box.
// ponytail: the game picks one of messages 533–541 by the account's billing kind; rathena has none.
pub const NOTICE_MESSAGE: usize = 100;

/// The line HC_DELETE_CHAR3_RESERVED's result shows, or none (2, and 1: reserved).
pub fn reserve_answer(result: u32) -> Option<usize> {
    match result {
        1 | 2 => None,
        3 => Some(1817),
        4 => Some(1818),
        5 => Some(1819),
        6 => Some(2981),
        7 => Some(3092),
        _ => Some(1816),
    }
}

/// The line HC_DELETE_CHAR3's result (not 1: deleted) shows.
pub fn delete_answer(result: u32) -> usize {
    match result {
        2 => 1820,
        3 => 1817,
        4 => 1821,
        5 => 1822,
        6 => 2369,
        7 => 301,
        8 => 3044,
        _ => 1816,
    }
}

/// The card character select opens on for the last slot picked: its page (the first when that page doesn't exist)
/// and its place on it.
pub fn opening_slot(saved: usize, pages: usize) -> usize {
    if saved / PER_PAGE < pages { saved } else { saved % PER_PAGE }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Card(usize),
    Button(Button),
    Start,
    Page(isize),
    Close,
}

/// What a click or key asks the client to do.
#[derive(Debug, PartialEq)]
pub enum Action {
    Start,
    /// Notice's box.
    Notice,
    /// Make a character in this slot.
    Make(u8),
    Reserve(u32),
    CancelReserve(u32),
    Delete(u32),
    Close,
    /// Move the character in the first slot to the second (swapping with one there), with its moves left.
    Move(u8, u8, u16),
    /// Rename this character.
    Rename(u32),
}

/// A choice of a card's right-click menu.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Choice {
    /// Move the character in this slot: the next card clicked is where to.
    Move(usize),
    Rename(u32),
}

/// Normal, hovered and pressed looks; the start button and the arrows also have an idle one first.
pub struct Assets {
    backdrop: Texture,
    info: Texture,
    cards: [Texture; 3],
    empty: [Texture; 3],
    empty_selected: Texture,
    lock: Texture,
    selected: Vec<Texture>,
    buttons: [Texture; 3],
    start: [Texture; 4],
    arrows: [[Texture; 4]; 2],
    close: [Texture; 3],
    /// Game Start's label, greyed and lit.
    start_label: [Texture; 2],
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf, font: &Font, messages: &[String]) -> Self {
        let folder = format!("{UI}select_character_ver3\\");
        let image = |name: &str| gpu.load_texture(grf, &format!("{folder}{name}"), false);
        let states = |name: &str, states: [&str; 3]| states.map(|state| image(&format!("{name}_{state}.bmp")));
        let lit = |name: &str| ["off", "on", "over", "press"].map(|state| image(&format!("{name}_{state}.bmp")));
        let label = messages.get(START_MESSAGE).map_or("", String::as_str);
        Self {
            backdrop: image("bg_back1.tga"),
            info: image("img_info.bmp"),
            cards: states("img_slot", ["normal", "over", "press"]),
            empty: states("img_slot2", ["normal", "over", "press"]),
            empty_selected: image("img_slot2_select.bmp"),
            lock: image("img_slot_lock.bmp"),
            selected: (0..8).map(|frame| image(&format!("img_slot_select{frame}.bmp"))).collect(),
            buttons: states("bt_info", ["normal", "over", "press"]),
            start: lit("bt_gamestart"),
            arrows: [lit("bt_left"), lit("bt_right")],
            close: states("bt_close2", ["normal", "over", "press"]),
            start_label: START_LABEL_COLORS.map(|color| gpu.texture(&font.render_sized(label, color, START_LABEL_SIZE), false)),
        }
    }
}

/// Where a reserved deletion stands.
#[derive(Debug, PartialEq, Clone, Copy)]
enum Deletion {
    None,
    /// Seconds left before it can be completed.
    Waiting(u64),
    Ready,
}

/// Every character stands still facing front, frame 0; one with a deletion date sits.
fn pose_of(deletion: Deletion) -> usize {
    if deletion == Deletion::None { actor::IDLE } else { actor::SIT }
}

/// The selected card's border bitmap `elapsed` into the selection.
fn border_frame(elapsed: Duration, frames: usize) -> usize {
    (elapsed.as_millis() / SELECT_FRAME.as_millis()) as usize % frames.max(1)
}

/// Game Start's label: lit while the selected character can start.
fn start_label(startable: bool) -> usize {
    usize::from(startable)
}

/// The time now, in Unix seconds.
fn unix_now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |time| time.as_secs() as i64)
}

/// How far east of UTC local time is, in seconds.
// ponytail: std has no time zones; this asks `date +%z` once, and takes UTC where that isn't there.
pub(crate) fn local_offset() -> i64 {
    static OFFSET: std::sync::OnceLock<i64> = std::sync::OnceLock::new();
    *OFFSET.get_or_init(|| {
        let out = std::process::Command::new("date").arg("+%z").output().ok().and_then(|out| String::from_utf8(out.stdout).ok()).unwrap_or_default();
        parse_offset(out.trim()).unwrap_or(0)
    })
}

/// "+1100" or "-0330" as seconds.
fn parse_offset(text: &str) -> Option<i64> {
    let (sign, digits) = text.split_at_checked(1)?;
    let sign = match sign {
        "+" => 1,
        "-" => -1,
        _ => return None,
    };
    let (hours, minutes) = (digits.get(..2)?.parse::<i64>().ok()?, digits.get(2..4)?.parse::<i64>().ok()?);
    Some(sign * (hours * 3600 + minutes * 60))
}

/// A Unix time `offset` seconds east of UTC as (year, month, day, hour, minute, second).
pub(crate) fn local_date(unix: i64, offset: i64) -> [i64; 6] {
    let seconds = unix + offset;
    let (days, rest) = (seconds.div_euclid(86_400), seconds.rem_euclid(86_400));
    // Days since 1970-01-01 to a civil date (Howard Hinnant's days-to-civil).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z - era * 146_097;
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 { month_index + 3 } else { month_index - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    [year, month, day, rest / 3600, rest / 60 % 60, rest % 60]
}

/// A deletion's two lines: the day, and the time, at `unix` local time.
fn date_lines(messages: &[String], unix: i64, offset: i64) -> [String; 2] {
    let message = |id: usize| messages.get(id).map_or("", String::as_str);
    let [_, month, day, hour, minute, second] = local_date(unix, offset);
    [messages::fill(message(DATE_MESSAGE), &[&month, &day]), messages::fill(message(TIME_MESSAGE), &[&hour, &minute, &second])]
}

pub struct Select {
    pub characters: Vec<Character>,
    figures: Vec<Option<SpriteSet>>,
    /// How many slots the account may use; the rest show locked.
    pub slots: usize,
    pub selected: usize,
    /// When each reserved deletion (by character ID) can be completed, in Unix seconds.
    deletable_at: HashMap<u32, i64>,
    clicks: Clicks,
    /// When the selection last changed, for its animations.
    since: Instant,
    pub pin_verified: bool,
    pub pin_pad: Option<crate::pin::PinPad>,
    pub auto_pin_tried: bool,
    /// A card's right-click menu.
    pub menu: Option<Menu<Choice>>,
    /// The slot of a character picked to move, until a card is clicked to move it to.
    pub moving: Option<usize>,
    /// Where a move sent to the server goes, for the selection to follow once it's done.
    pub moved_to: Option<usize>,
    /// The selection last kept (to keep each change).
    kept: Option<usize>,
}

impl Select {
    pub fn new(characters: Vec<Character>, figures: Vec<Option<SpriteSet>>, slots: usize) -> Self {
        let now = Instant::now();
        let deletable_at =
            characters.iter().filter(|character| character.deletion != 0).map(|character| (character.id, unix_now() + i64::from(character.deletion))).collect();
        let selected = characters.iter().map(|character| usize::from(character.slot)).min().unwrap_or(0);
        Self {
            characters,
            figures,
            slots,
            selected,
            deletable_at,
            clicks: Clicks::default(),
            since: now,
            pin_verified: false,
            pin_pad: None,
            auto_pin_tried: false,
            menu: None,
            moving: None,
            moved_to: None,
            kept: None,
        }
    }

    /// The screen position of the whole window (centered).
    fn origin() -> [f32; 2] {
        [((screen()[0] - FRAME[0]) / 2.0).floor(), ((screen()[1] - FRAME[1]) / 2.0).floor()]
    }

    fn at(local: [f32; 2]) -> [f32; 2] {
        offset(Self::origin(), local)
    }

    fn cell(slot: usize) -> [f32; 2] {
        let place = slot % PER_PAGE;
        Self::at([GRID[0] + (place % COLUMNS) as f32 * CELL[0], GRID[1] + (place / COLUMNS) as f32 * CELL[1]])
    }

    fn page(&self) -> usize {
        self.selected / PER_PAGE
    }

    pub fn pages(&self) -> usize {
        self.slots.div_ceil(PER_PAGE).max(1)
    }

    /// The two buttons under the stats and Game Start on screen (for the test driver): "char_top", "char_bottom",
    /// "char_start".
    pub fn button_boxes(&self) -> Vec<(&'static str, [f32; 4])> {
        let info = Self::at(INFO);
        let place = |local: [f32; 2], size: [f32; 2]| {
            let [x, y] = offset(info, local);
            [x, y, size[0], size[1]]
        };
        vec![("char_top", place(BUTTONS[0], [131.0, 24.0])), ("char_bottom", place(BUTTONS[1], [131.0, 24.0])), ("char_start", place(START, [165.0, 110.0]))]
    }

    /// The selection when it changed since last asked, to keep.
    pub fn changed_selection(&mut self) -> Option<usize> {
        (self.kept != Some(self.selected)).then(|| *self.kept.insert(self.selected))
    }

    fn index_in(&self, slot: usize) -> Option<usize> {
        self.characters.iter().position(|character| usize::from(character.slot) == slot)
    }

    pub fn selected_character(&self) -> Option<&Character> {
        self.index_in(self.selected).map(|index| &self.characters[index])
    }

    fn deletion(&self, id: u32) -> Deletion {
        match self.deletable_at.get(&id).map(|at| at - unix_now()) {
            None => Deletion::None,
            Some(left) if left > 0 => Deletion::Waiting(left as u64),
            Some(_) => Deletion::Ready,
        }
    }

    /// A character just made: it joins the list, selected.
    pub fn add(&mut self, character: Character, figure: Option<SpriteSet>) {
        self.selected = usize::from(character.slot);
        self.characters.push(character);
        self.figures.push(figure);
        self.since = Instant::now();
    }

    /// A reservation the server confirmed (`seconds` until it can be completed), or one canceled (None).
    pub fn set_deletion(&mut self, id: u32, seconds: Option<i32>) {
        match seconds {
            Some(seconds) => self.deletable_at.insert(id, unix_now() + i64::from(seconds)),
            None => self.deletable_at.remove(&id),
        };
        self.since = Instant::now();
    }

    fn buttons(&self) -> [Button; 2] {
        match self.selected_character().map(|character| self.deletion(character.id)) {
            Some(Deletion::Ready) => [Button::CancelDeletion, Button::Delete],
            Some(Deletion::Waiting(_)) => [Button::CancelDeletion, Button::Notice],
            _ => [Button::DeleteCharacter, Button::Notice],
        }
    }

    /// Whether the server's PIN check has passed (state 0) and no PIN pad is open.
    pub fn unlocked(&self) -> bool {
        self.pin_verified && self.pin_pad.is_none()
    }

    /// Whether the selected character can start (it exists, isn't being deleted, and PIN state 0 has arrived).
    fn startable(&self) -> bool {
        self.unlocked() && self.selected_character().is_some_and(|character| self.deletion(character.id) == Deletion::None)
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let first = self.page() * PER_PAGE;
        if let Some(slot) = (first..first + PER_PAGE).find(|&slot| contains(offset(Self::cell(slot), CARD.0), CARD.1, cursor)) {
            return Some(Hit::Card(slot));
        }
        let info = Self::at(INFO);
        if let Some(place) = BUTTONS.iter().position(|&place| contains(offset(info, place), [131.0, 24.0], cursor)) {
            return Some(Hit::Button(self.buttons()[place]));
        }
        if contains(offset(info, START), [165.0, 110.0], cursor) {
            return Some(Hit::Start);
        }
        if let Some(arrow) = ARROWS.iter().position(|&place| contains(offset(info, place), [21.0, 28.0], cursor)) {
            return Some(Hit::Page(if arrow == 0 { -1 } else { 1 }));
        }
        contains(Self::at(CLOSE), [17.0, 18.0], cursor).then_some(Hit::Close)
    }

    // ponytail: the 2018+ screen has no bitmaps for moving and renaming (data.grf only has the older screen's Add-Ons
    // side bar), so they're a right-click menu on the card; replace it with the real client's place for them.
    /// A right click: on a character that may move or be renamed, the menu of those.
    pub fn open_menu(&mut self, cursor: [f32; 2], font: &Font, messages: &[String]) {
        self.moving = None;
        let message = |id: usize| messages.get(id).cloned().unwrap_or_default();
        let Some(Hit::Card(slot)) = self.hit(cursor) else { return self.menu = None };
        let Some(character) = self.index_in(slot).map(|index| &self.characters[index]) else { return self.menu = None };
        let choices: Vec<menu::Entry<Choice>> = [
            (character.moves > 0).then(|| (message(MOVE_CHOICE), Choice::Move(slot))),
            (character.renames > 0).then(|| (message(RENAME_CHOICE), Choice::Rename(character.id))),
        ]
        .into_iter()
        .flatten()
        .map(Some)
        .collect();
        self.menu = (!choices.is_empty()).then(|| Menu::new(cursor, choices, font));
        if self.menu.is_some() && slot != self.selected {
            self.selected = slot;
            self.since = Instant::now();
        }
    }

    /// A click while the menu is open: a choice, or anywhere else, which closes it.
    pub fn click_menu(&mut self, cursor: [f32; 2]) -> Option<Action> {
        match self.menu.take()?.hit(cursor).flatten()? {
            Choice::Move(slot) => {
                self.moving = Some(slot);
                None
            }
            Choice::Rename(id) => Some(Action::Rename(id)),
        }
    }

    /// The moves the character in `slot` has left.
    fn moves(&self, slot: usize) -> i32 {
        self.index_in(slot).map_or(0, |index| self.characters[index].moves)
    }

    pub fn click(&mut self, hit: Hit) -> Option<Action> {
        // Picking where to move: a usable card other than the character's own; anything else stops picking.
        if let Some(from) = self.moving.take() {
            return match hit {
                Hit::Card(to) if to < self.slots && to != from => Some(Action::Move(from as u8, to as u8, self.moves(from).max(0) as u16)),
                _ => None,
            };
        }
        let character = self.selected_character().map(|character| character.id);
        match hit {
            Hit::Card(slot) if slot < self.slots => {
                if slot != self.selected {
                    self.selected = slot;
                    self.since = Instant::now();
                }
                if self.clicks.double(slot as u16) {
                    return self.enter();
                }
            }
            Hit::Card(_) => {}
            Hit::Button(Button::DeleteCharacter) => return character.map(Action::Reserve),
            Hit::Button(Button::CancelDeletion) => return character.map(Action::CancelReserve),
            Hit::Button(Button::Delete) => return character.map(Action::Delete),
            Hit::Button(Button::Notice) => return Some(Action::Notice),
            // Game Start: the character, or on an empty usable card, making one there.
            Hit::Start if self.selected_character().is_none() => return (self.selected < self.slots).then_some(Action::Make(self.selected as u8)),
            Hit::Start => return self.startable().then_some(Action::Start),
            Hit::Page(step) => {
                let page = self.page() as isize + step;
                if page >= 0 && (page as usize) < self.pages() {
                    self.selected = page as usize * PER_PAGE;
                    self.since = Instant::now();
                }
            }
            Hit::Close => return Some(Action::Close),
        }
        None
    }

    /// Enter or a double click: start with the selected character, or make one in an empty slot.
    pub fn enter(&self) -> Option<Action> {
        match self.selected_character() {
            Some(_) => self.startable().then_some(Action::Start),
            None => Some(Action::Make(self.selected as u8)),
        }
    }

    /// Moves the selection by `step` cards (arrows: 1 sideways, a row up or down), staying on usable slots.
    pub fn step(&mut self, step: isize) {
        let target = self.selected as isize + step;
        if target >= 0 && (target as usize) < self.slots {
            self.selected = target as usize;
            self.since = Instant::now();
        }
    }

    pub const ROW: isize = COLUMNS as isize;

    /// Decodes the frames the page's characters show now; `draw` places them.
    pub fn pose(&mut self, gpu: &Gpu) -> Vec<(usize, Vec<Placed>)> {
        let page = self.page();
        let poses: Vec<(usize, usize)> = self
            .characters
            .iter()
            .enumerate()
            .filter(|(_, character)| usize::from(character.slot) / PER_PAGE == page)
            .map(|(index, character)| (index, pose_of(self.deletion(character.id))))
            .collect();
        poses.into_iter().filter_map(|(index, action)| Some((index, self.figures[index].as_mut()?.flat_frame(gpu, action, 0, 0, Timing::Loop(0.0))))).collect()
    }

    /// The job icons the page shows, to load before drawing.
    pub fn job_icons(&self) -> Vec<String> {
        let page = self.page();
        self.characters.iter().filter(|character| usize::from(character.slot) / PER_PAGE == page).map(|character| job_icon(character.job)).collect()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &'a self,
        layer: &mut Layer<'a>,
        assets: &'a Assets,
        font: &Font,
        messages: &[String],
        job_names: &HashMap<u16, String>,
        map_names: &HashMap<String, String>,
        icons: &'a HashMap<String, Option<Texture>>,
        pointer: ([f32; 2], bool),
        poses: &[(usize, Vec<Placed>)],
    ) {
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        let (cursor, down) = pointer;
        let look = |hovered: bool| usize::from(hovered) + usize::from(hovered && down);
        let hit = self.hit(cursor);
        layer.sprites.push((&assets.backdrop, Self::at([0.0; 2]), assets.backdrop.size));

        // The cards of this page.
        let first = self.page() * PER_PAGE;
        let frame = border_frame(self.since.elapsed(), assets.selected.len());
        for slot in first..first + PER_PAGE {
            let cell = Self::cell(slot);
            let hovered = hit == Some(Hit::Card(slot));
            let character = self.index_in(slot).map(|index| &self.characters[index]);
            // Picking where to move, the card under the pointer shows as selected.
            let target = hovered && self.moving.is_some_and(|from| from != slot);
            let image = match (slot < self.slots, character, slot == self.selected || target) {
                (false, ..) => &assets.lock,
                (true, Some(_), true) => &assets.selected[frame],
                (true, Some(_), false) => &assets.cards[look(hovered)],
                (true, None, true) => &assets.empty_selected,
                (true, None, false) => &assets.empty[look(hovered)],
            };
            layer.sprites.push((image, cell, image.size));
        }
        for (index, placements) in poses {
            let character = &self.characters[*index];
            let feet = offset(Self::cell(usize::from(character.slot)), FEET);
            let figure = self.figures[*index].as_ref().unwrap();
            layer.sprites.extend(placements.iter().map(|(key, position, size, _)| (figure.texture(*key), offset(feet, *position), *size)));
        }
        for character in self.characters.iter().filter(|character| usize::from(character.slot) / PER_PAGE == self.page()) {
            let cell = Self::cell(usize::from(character.slot));
            if let Some(Some(icon)) = icons.get(&job_icon(character.job)) {
                layer.sprites.push((icon, offset(cell, JOB_ICON), icon.size));
            }
            // The name in bold (drawn twice, a pixel apart).
            let name_top = (NAME_STRIP[0] + NAME_STRIP[1] - font.line_height()) / 2.0;
            let x = (CELL[0] - font.width(&character.name)) / 2.0;
            for nudge in [0.0, 1.0] {
                layer.texts.push((character.name.clone(), offset(cell, [(x + nudge).floor(), name_top.floor()]), NAME_COLOR));
            }
            // The moment deletion opens, in local time: red while waiting, blue once it has passed.
            let color = match self.deletion(character.id) {
                Deletion::None => continue,
                Deletion::Waiting(_) => COUNTING,
                Deletion::Ready => COUNTED,
            };
            let at = self.deletable_at.get(&character.id).copied().unwrap_or_default();
            for (line, top) in date_lines(messages, at, local_offset()).into_iter().zip(DATE_LINES) {
                let left = (DATE_CENTER - font.width(&line) / 2.0).floor();
                layer.texts.push((line, offset(cell, [left, top]), color));
            }
        }

        // The selected character's details.
        let info = Self::at(INFO);
        layer.sprites.push((&assets.info, info, assets.info.size));
        if let Some(character) = self.selected_character() {
            let map = character.map.trim_end_matches(".gat");
            let values = [
                map_names.get(&map.to_lowercase()).map_or(map, String::as_str).to_owned(),
                job_names.get(&character.job).cloned().unwrap_or_else(|| character.job.to_string()),
                character.base_level.to_string(),
                character.exp.to_string(),
                character.hp.to_string(),
                character.sp.to_string(),
            ]
            .into_iter()
            .chain(character.stats.iter().map(u8::to_string));
            for (row, value) in values.enumerate() {
                let top = VALUES[1] + row as f32 * VALUE_ROW + ((VALUE_BOX[1] - font.line_height()) / 2.0).floor();
                layer.texts.push((window::fit(font, &value, VALUE_BOX[0]), Self::at([VALUES[0], top]), [0, 0, 0]));
            }
        }
        for (button, place) in self.buttons().into_iter().zip(BUTTONS) {
            let image = &assets.buttons[look(hit == Some(Hit::Button(button)))];
            layer.sprites.push((image, offset(info, place), image.size));
            let label = message(button.message());
            let text =
                offset(info, [place[0] + ((image.size[0] - font.width(label)) / 2.0).floor(), place[1] + ((image.size[1] - font.line_height()) / 2.0).floor()]);
            layer.texts.push((label.to_owned(), text, BUTTON_LABEL));
        }
        let start = if self.startable() { 1 + look(hit == Some(Hit::Start)) } else { 0 };
        let button = &assets.start[start];
        layer.sprites.push((button, offset(info, START), button.size));
        let label = &assets.start_label[start_label(self.startable())];
        let label_place = [START[0] + ((button.size[0] - label.size[0]) / 2.0).floor(), START[1] + START_LABEL_TOP];
        layer.sprites.push((label, offset(info, label_place), label.size));

        // The page bar; picking where to move, the moves left instead of the title.
        let moves_left = self.moving.map(|from| messages::fill(message(MOVES_LEFT_MESSAGE), &[&self.moves(from)]));
        let title = moves_left.as_deref().unwrap_or(message(LIST_MESSAGE));
        layer.texts.push((title.to_owned(), Self::at([(LIST_CENTER - font.width(title) / 2.0).floor(), LIST_TITLE_TOP]), TITLE_COLOR));
        let page_line = format!("{} / {}", self.page() + 1, self.pages());
        layer.texts.push((page_line.clone(), Self::at([(LIST_CENTER - font.width(&page_line) / 2.0).floor(), PAGE_LINE_TOP]), [255; 3]));
        for (arrow, place) in ARROWS.into_iter().enumerate() {
            let step = if arrow == 0 { -1 } else { 1 };
            let page = self.page() as isize + step;
            let usable = page >= 0 && (page as usize) < self.pages();
            let image = &assets.arrows[arrow][if usable { 1 + look(hit == Some(Hit::Page(step))) } else { 0 }];
            layer.sprites.push((image, offset(info, place), image.size));
        }
        let close = &assets.close[look(hit == Some(Hit::Close))];
        layer.sprites.push((close, Self::at(CLOSE), close.size));
    }

    /// The card's menu, over the screen.
    pub fn draw_menu<'a>(&self, layer: &mut Layer<'a>, assets: &'a menu::Assets, font: &Font, pointer: [f32; 2]) {
        if let Some(menu) = &self.menu {
            menu.draw_outlined(layer, assets, font, pointer);
        }
    }

    /// Each usable card's box on screen (for the test driver).
    pub fn card_boxes(&self) -> Vec<(usize, [f32; 4])> {
        let first = self.page() * PER_PAGE;
        (first..(first + PER_PAGE).min(self.slots))
            .map(|slot| {
                let [x, y] = offset(Self::cell(slot), CARD.0);
                (slot, [x, y, CARD.1[0], CARD.1[1]])
            })
            .collect()
    }
}

fn job_icon(job: u16) -> String {
    format!("{UI}renewalparty\\icon_jobs_{job}.bmp")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offers_the_buttons_by_deletion() {
        let mut select = Select::new(vec![Character { id: 7, slot: 0, ..Character::default() }], Vec::new(), 15);
        select.pin_verified = true;
        // No deletion: Delete Character (at once) and Notice.
        assert_eq!(select.buttons(), [Button::DeleteCharacter, Button::Notice]);
        assert_eq!(select.buttons().map(Button::message), [3339, 3344]);
        assert_eq!(select.click(Hit::Button(Button::DeleteCharacter)), Some(Action::Reserve(7)));
        assert_eq!(select.click(Hit::Button(Button::Notice)), Some(Action::Notice));
        // Waiting: Cancel Deletion and Notice; past: Cancel Deletion and Delete.
        select.set_deletion(7, Some(90));
        assert_eq!(select.buttons(), [Button::CancelDeletion, Button::Notice]);
        assert_eq!(select.click(Hit::Button(Button::CancelDeletion)), Some(Action::CancelReserve(7)));
        select.set_deletion(7, Some(-5));
        assert_eq!(select.buttons(), [Button::CancelDeletion, Button::Delete]);
        assert_eq!(select.click(Hit::Button(Button::Delete)), Some(Action::Delete(7)));
        // With a deletion date Game Start does nothing; on an empty card it makes one there; Delete Character there,
        // nothing.
        assert_eq!(select.click(Hit::Start), None);
        select.click(Hit::Card(3));
        assert_eq!(select.buttons(), [Button::DeleteCharacter, Button::Notice]);
        assert_eq!(select.click(Hit::Start), Some(Action::Make(3)));
        assert_eq!(select.click(Hit::Button(Button::DeleteCharacter)), None);
    }

    #[test]
    fn keeps_the_last_slot_and_answers_deletions() {
        // Its page and place; a page that doesn't exist, the first.
        assert_eq!([opening_slot(1, 1), opening_slot(17, 2), opening_slot(17, 1), opening_slot(46, 3)], [1, 17, 2, 1]);
        let mut select = Select::new(Vec::new(), Vec::new(), 15);
        assert_eq!(select.changed_selection(), Some(0));
        assert_eq!(select.changed_selection(), None);
        select.click(Hit::Card(1));
        assert_eq!(select.changed_selection(), Some(1));
        // The answers' lines.
        assert_eq!([1, 2, 3, 4, 5, 6, 7, 9].map(reserve_answer), [None, None, Some(1817), Some(1818), Some(1819), Some(2981), Some(3092), Some(1816)]);
        assert_eq!([2, 3, 4, 5, 6, 7, 8, 9].map(delete_answer), [1820, 1817, 1821, 1822, 2369, 301, 3044, 1816]);
    }

    #[test]
    fn walks_the_deletion_states() {
        let mut select = Select::new(Vec::new(), Vec::new(), 15);
        assert_eq!(select.deletion(7), Deletion::None);
        select.set_deletion(7, Some(90));
        assert!(matches!(select.deletion(7), Deletion::Waiting(89 | 90)));
        select.set_deletion(7, Some(-5));
        assert_eq!(select.deletion(7), Deletion::Ready);
        select.set_deletion(7, None);
        assert_eq!(select.deletion(7), Deletion::None);
    }

    #[test]
    fn stands_everyone_still_and_seats_deletions() {
        // Selected or not: standing; a deletion waiting or past: sitting.
        assert_eq!([Deletion::None, Deletion::Waiting(60), Deletion::Ready].map(pose_of), [actor::IDLE, actor::SIT, actor::SIT]);
        // The border: a bitmap every 83 ms, round the eight.
        assert_eq!([0, 82, 83, 166, 8 * 83].map(|ms| border_frame(Duration::from_millis(ms), 8)), [0, 0, 1, 2, 0]);
        assert_eq!((START_LABEL_COLORS[start_label(false)], START_LABEL_COLORS[start_label(true)]), ([0xDC, 0x74, 0x7F], [255; 3]));
        // Places, window-local: Game Start at (816, 407), the close X 52 in from the right.
        assert_eq!((offset(INFO, START), CLOSE), ([816.0, 407.0], [945.0, 5.0]));
    }

    #[test]
    fn writes_the_deletion_date_in_local_time() {
        assert_eq!([parse_offset("+1100"), parse_offset("-0330"), parse_offset("x")], [Some(39_600), Some(-12_600), None]);
        // 2026-10-19 08:30:05 UTC is 19:30:05 at +11:00, and the next day's 03:30:05 at +19:00.
        let unix = 1_792_398_605;
        assert_eq!(local_date(unix, 0), [2026, 10, 19, 8, 30, 5]);
        assert_eq!(local_date(unix, 39_600), [2026, 10, 19, 19, 30, 5]);
        assert_eq!(local_date(unix, 19 * 3600), [2026, 10, 20, 3, 30, 5]);
        assert_eq!(local_date(951_782_400, 0)[..3], [2000, 2, 29], "a leap day");
        let mut messages = vec![String::new(); TIME_MESSAGE + 1];
        messages[DATE_MESSAGE] = "%d/%d".into();
        messages[TIME_MESSAGE] = "%d:%d:%d".into();
        assert_eq!(date_lines(&messages, unix, 39_600), ["10/19".to_owned(), "19:30:5".to_owned()]);
    }

    #[test]
    fn steps_only_onto_usable_slots() {
        let mut select = Select::new(Vec::new(), Vec::new(), 9);
        select.step(Select::ROW);
        assert_eq!(select.selected, 5);
        select.step(Select::ROW);
        assert_eq!(select.selected, 5, "slot 10 is locked");
        select.step(-1);
        assert_eq!((select.selected, select.enter()), (4, Some(Action::Make(4))));
    }

    #[test]
    fn moves_and_renames_from_the_card_menu() {
        let font = Font::load();
        let messages: Vec<String> = (0..=RENAME_CHOICE).map(|id| id.to_string()).collect();
        let character = |id, slot, moves, renames| Character { id, slot, moves, renames, ..Character::default() };
        let mut select = Select::new(vec![character(1, 0, 0, 0), character(2, 1, 2, 1)], vec![None, None], 15);
        let card = |slot: usize| offset(Select::cell(slot), [70.0, 90.0]);

        // Nothing granted: no menu.
        select.open_menu(card(0), &font, &messages);
        assert!(select.menu.is_none());
        select.open_menu(card(1), &font, &messages);
        let choices: Vec<String> = select.menu.as_ref().unwrap().entry_boxes().into_iter().map(|(label, _)| label).collect();
        assert_eq!((choices, select.selected), (vec!["2164".to_owned(), "2165".to_owned()], 1));

        // Move, then the card to move to.
        let [x, y, w, h] = select.menu.as_ref().unwrap().entry_boxes()[0].1;
        assert_eq!(select.click_menu([x + w / 2.0, y + h / 2.0]), None);
        assert_eq!(select.moving, Some(1));
        assert_eq!(select.click(Hit::Card(1)), None, "its own card stops picking");
        select.moving = Some(1);
        assert_eq!(select.click(Hit::Card(16)), None, "a locked slot stops picking");
        select.moving = Some(1);
        assert_eq!(select.click(Hit::Card(0)), Some(Action::Move(1, 0, 2)));
        assert_eq!(select.moving, None);

        select.open_menu(card(1), &font, &messages);
        let [x, y, w, h] = select.menu.as_ref().unwrap().entry_boxes()[1].1;
        assert_eq!(select.click_menu([x + w / 2.0, y + h / 2.0]), Some(Action::Rename(2)));
        assert!(select.menu.is_none());
    }
}
