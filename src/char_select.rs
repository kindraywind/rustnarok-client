//! Character select of 2018+ clients (select_character_ver3): fifteen character cards a page beside the selected
//! character's details, the deletion buttons and the Poring that starts the game.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use roclient::grf::Grf;
use roclient::messages;
use roclient::net::Character;
use roclient::text::Font;

use crate::actor::{self, Placed, SpriteSet, Timing};
use crate::render::{Gpu, Texture};
use crate::window::{self, Clicks};
use crate::{Layer, SCREEN, UI, contains, offset};

// Places measured from make_character_ver2\bg_back2.tga, the client's own (blurred) picture of this screen, by
// blurring the parts' bitmaps the same way and fitting them to it.
const FRAME: [f32; 2] = [997.0, 626.0];
const COLUMNS: usize = 5;
pub const PER_PAGE: usize = 15;
const GRID: [f32; 2] = [10.0, 21.0];
const CELL: [f32; 2] = [157.0, 195.0];
/// The 161x197 card bitmaps hang 2px left of and 1px above their 157x195 cell.
const BIG_CARD: [f32; 2] = [-2.0, -1.0];
/// The visible card inside a cell, what a click hits.
const CARD: ([f32; 2], [f32; 2]) = ([5.0, 5.0], [147.0, 183.0]);
/// Where a character stands: the middle of the shadow in the selected card (78,157, less the sprite's 2D lift).
const FEET: [f32; 2] = [78.0, 140.0];
// ponytail: job icon and countdown places are estimates; calibrate against the real client.
const JOB_ICON: [f32; 2] = [120.0, 15.0];
const COUNTDOWN: [f32; 2] = [55.0, 45.0];
/// The strip at the bottom of a card that holds the name.
const NAME_STRIP: [f32; 2] = [159.0, 187.0];
const NAME_COLOR: [u8; 3] = [0x15, 0x15, 0x4A];
const COUNTING: [u8; 3] = [0xFF, 0x00, 0x00];
const COUNTED: [u8; 3] = [0x00, 0x00, 0xFF];
const INFO: [f32; 2] = [809.0, 26.0];
/// The stats table of img_info.bmp: values start here, a row every 17px, 104px wide.
const VALUES: [f32; 2] = [60.0, 87.0];
const VALUE_ROW: f32 = 17.0;
const VALUE_HEIGHT: f32 = 16.0;
const VALUE_WIDTH: f32 = 104.0;
const BUTTONS: [[f32; 2]; 2] = [[21.0, 296.0], [21.0, 325.0]];
const START: [f32; 2] = [4.0, 381.0];
// ponytail: the label's size and color are estimated from the blurred picture.
/// "게임시작" across the Poring's belly.
const START_LABEL: [f32; 2] = [82.0, 65.0];
const START_LABEL_SIZE: f32 = 16.0;
const START_LABEL_COLOR: [u8; 3] = [0xA0, 0x40, 0x60];
/// "Character List" and the page between the arrows, centered on these heights.
const LIST_TITLE: f32 = 519.0;
const PAGE_LINE: f32 = 546.0;
const TITLE_COLOR: [u8; 3] = [0x3C, 0x4B, 0x82];
const PAGE_COLOR: [u8; 3] = [0x8E, 0x9A, 0xC0];
const ARROWS: [[f32; 2]; 2] = [[21.0, 532.0], [130.0, 532.0]];
// ponytail: the close button sits 5px in from the top right.
const CLOSE: [f32; 2] = [975.0, 5.0];
/// The selected card's border cycles through its 8 bitmaps this fast.
const SELECT_FRAME: Duration = Duration::from_millis(250);
const DEATH: usize = 8;

const COUNTDOWN_MESSAGE: usize = 3349;
const START_MESSAGE: usize = 3341;
const LIST_MESSAGE: usize = 3343;

/// The buttons under the stats; which ones show depends on the selected character's deletion.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Button {
    // ponytail: the client's picture shows "과금정보" (billing) first; rathena has no billing, so it does nothing.
    Billing,
    Reserve,
    CancelReserve,
    Delete,
}

impl Button {
    fn message(self) -> usize {
        match self {
            Button::Billing => 3338,
            Button::Reserve => 3339,
            Button::CancelReserve => 3340,
            Button::Delete => 3342,
        }
    }
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
    /// Make a character in this slot.
    Make(u8),
    Reserve(u32),
    CancelReserve(u32),
    Delete(u32),
    Close,
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
    start_label: Texture,
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
            start_label: gpu.texture(&font.render_sized(label, START_LABEL_COLOR, START_LABEL_SIZE), false),
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

pub struct Select {
    pub characters: Vec<Character>,
    figures: Vec<Option<SpriteSet>>,
    /// How many slots the account may use; the rest show locked.
    pub slots: usize,
    pub selected: usize,
    /// When each reserved deletion (by character ID) can be completed.
    deletable_at: HashMap<u32, Instant>,
    clicks: Clicks,
    /// When the selection last changed, for its animations.
    since: Instant,
}

impl Select {
    pub fn new(characters: Vec<Character>, figures: Vec<Option<SpriteSet>>, slots: usize) -> Self {
        let now = Instant::now();
        let deletable_at =
            characters.iter().filter(|character| character.deletion != 0).map(|character| (character.id, after(now, character.deletion))).collect();
        let selected = characters.iter().map(|character| usize::from(character.slot)).min().unwrap_or(0);
        Self { characters, figures, slots, selected, deletable_at, clicks: Clicks::default(), since: now }
    }

    /// The screen position of the whole window (centered).
    fn origin() -> [f32; 2] {
        [((SCREEN[0] - FRAME[0]) / 2.0).floor(), ((SCREEN[1] - FRAME[1]) / 2.0).floor()]
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

    fn pages(&self) -> usize {
        self.slots.div_ceil(PER_PAGE).max(1)
    }

    fn index_in(&self, slot: usize) -> Option<usize> {
        self.characters.iter().position(|character| usize::from(character.slot) == slot)
    }

    pub fn selected_character(&self) -> Option<&Character> {
        self.index_in(self.selected).map(|index| &self.characters[index])
    }

    fn deletion(&self, id: u32) -> Deletion {
        match self.deletable_at.get(&id) {
            None => Deletion::None,
            Some(at) => match at.checked_duration_since(Instant::now()) {
                Some(left) if left.as_secs() > 0 => Deletion::Waiting(left.as_secs()),
                _ => Deletion::Ready,
            },
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
            Some(seconds) => self.deletable_at.insert(id, after(Instant::now(), seconds)),
            None => self.deletable_at.remove(&id),
        };
        self.since = Instant::now();
    }

    fn buttons(&self) -> [Button; 2] {
        match self.selected_character().map(|character| self.deletion(character.id)) {
            Some(Deletion::Ready) => [Button::CancelReserve, Button::Delete],
            Some(Deletion::Waiting(_)) => [Button::Billing, Button::CancelReserve],
            _ => [Button::Billing, Button::Reserve],
        }
    }

    /// Whether the selected character can start (it exists and isn't being deleted).
    fn startable(&self) -> bool {
        self.selected_character().is_some_and(|character| self.deletion(character.id) == Deletion::None)
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

    pub fn click(&mut self, hit: Hit) -> Option<Action> {
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
            Hit::Card(_) | Hit::Button(Button::Billing) => {}
            Hit::Button(Button::Reserve) => return character.map(Action::Reserve),
            Hit::Button(Button::CancelReserve) => return character.map(Action::CancelReserve),
            Hit::Button(Button::Delete) => return character.map(Action::Delete),
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
        let elapsed = self.since.elapsed().as_secs_f32() * 1000.0;
        let poses: Vec<(usize, usize, Timing)> = self
            .characters
            .iter()
            .enumerate()
            .filter(|(_, character)| usize::from(character.slot) / PER_PAGE == page)
            .map(|(index, character)| {
                let (action, timing) = match self.deletion(character.id) {
                    Deletion::Waiting(_) => (actor::SIT, Timing::Loop(0.0)),
                    Deletion::Ready => (DEATH, Timing::Span(1.0)),
                    Deletion::None if usize::from(character.slot) == self.selected => (actor::READY, Timing::Loop(elapsed)),
                    Deletion::None => (actor::IDLE, Timing::Loop(0.0)),
                };
                (index, action, timing)
            })
            .collect();
        poses.into_iter().filter_map(|(index, action, timing)| Some((index, self.figures[index].as_mut()?.frame(gpu, action, 0, 0, timing)))).collect()
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
        let frame = (self.since.elapsed().as_millis() / SELECT_FRAME.as_millis()) as usize % assets.selected.len();
        for slot in first..first + PER_PAGE {
            let cell = Self::cell(slot);
            let hovered = hit == Some(Hit::Card(slot));
            let character = self.index_in(slot).map(|index| &self.characters[index]);
            let image = match (slot < self.slots, character, slot == self.selected) {
                (false, ..) => &assets.lock,
                (true, Some(_), true) => &assets.selected[frame],
                (true, Some(_), false) => &assets.cards[look(hovered)],
                (true, None, true) => &assets.empty_selected,
                (true, None, false) => &assets.empty[look(hovered)],
            };
            let corner = if image.size[0] > CELL[0] { offset(cell, BIG_CARD) } else { cell };
            layer.sprites.push((image, corner, image.size));
        }
        for (index, placements) in poses {
            let character = &self.characters[*index];
            let feet = offset(Self::cell(usize::from(character.slot)), FEET);
            let figure = self.figures[*index].as_ref().unwrap();
            layer.sprites.extend(placements.iter().map(|(key, position, size)| (figure.texture(*key), offset(feet, *position), *size)));
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
            match self.deletion(character.id) {
                Deletion::None => {}
                Deletion::Waiting(left) => {
                    let text = messages::fill(message(COUNTDOWN_MESSAGE), &[&(left / 3600), &(left / 60 % 60), &(left % 60)]);
                    layer.texts.push((text, offset(cell, COUNTDOWN), COUNTING));
                }
                Deletion::Ready => layer.texts.push((messages::fill(message(COUNTDOWN_MESSAGE), &[&0, &0, &0]), offset(cell, COUNTDOWN), COUNTED)),
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
                let top = VALUES[1] + row as f32 * VALUE_ROW + ((VALUE_HEIGHT - font.line_height()) / 2.0).floor();
                layer.texts.push((window::fit(font, &value, VALUE_WIDTH), offset(info, [VALUES[0], top]), [0, 0, 0]));
            }
        }
        for (button, place) in self.buttons().into_iter().zip(BUTTONS) {
            let image = &assets.buttons[look(hit == Some(Hit::Button(button)))];
            layer.sprites.push((image, offset(info, place), image.size));
            let label = message(button.message());
            let text =
                offset(info, [place[0] + ((image.size[0] - font.width(label)) / 2.0).floor(), place[1] + ((image.size[1] - font.line_height()) / 2.0).floor()]);
            layer.texts.push((label.to_owned(), text, [0, 0, 0]));
        }
        let start = if self.startable() { 1 + look(hit == Some(Hit::Start)) } else { 0 };
        layer.sprites.push((&assets.start[start], offset(info, START), assets.start[start].size));
        let label = &assets.start_label;
        let label_place = [START[0] + START_LABEL[0] - (label.size[0] / 2.0).floor(), START[1] + START_LABEL[1] - (label.size[1] / 2.0).floor()];
        layer.sprites.push((label, offset(info, label_place), label.size));

        // The page bar.
        let title = message(LIST_MESSAGE);
        let center = assets.info.size[0] / 2.0;
        layer.texts.push((
            title.to_owned(),
            offset(info, [(center - font.width(title) / 2.0).floor(), (LIST_TITLE - font.line_height() / 2.0).floor()]),
            TITLE_COLOR,
        ));
        let (current, rest) = ((self.page() + 1).to_string(), format!(" / {}", self.pages()));
        let left = (center - font.width(&format!("{current}{rest}")) / 2.0).floor();
        let top = (PAGE_LINE - font.line_height() / 2.0).floor();
        layer.texts.push((current.clone(), offset(info, [left, top]), [255; 3]));
        layer.texts.push((rest, offset(info, [left + font.width(&current), top]), PAGE_COLOR));
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
}

fn job_icon(job: u16) -> String {
    format!("{UI}renewalparty\\icon_jobs_{job}.bmp")
}

/// `seconds` from `now` (before it when negative).
fn after(now: Instant, seconds: i32) -> Instant {
    let span = Duration::from_secs(u64::from(seconds.unsigned_abs()));
    if seconds >= 0 { now + span } else { now.checked_sub(span).unwrap_or(now) }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn steps_only_onto_usable_slots() {
        let mut select = Select::new(Vec::new(), Vec::new(), 9);
        select.step(Select::ROW);
        assert_eq!(select.selected, 5);
        select.step(Select::ROW);
        assert_eq!(select.selected, 5, "slot 10 is locked");
        select.step(-1);
        assert_eq!((select.selected, select.enter()), (4, Some(Action::Make(4))));
    }
}
