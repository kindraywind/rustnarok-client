//! The player's homunculus: its window (Alt+R), its skill list, the words for its intimacy, when it says it's
//! starving.

use std::collections::HashMap;

use rustnarok_client::grf::Grf;
use rustnarok_client::lua::SkillInfo;
use rustnarok_client::net::{Homunculus, Skill};
use rustnarok_client::text::Font;

use crate::mercenary::draw_values;
use crate::render::{Gpu, Texture};
use crate::window::{self, Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI, contains, messages, offset, screen, skills};

const TITLE: usize = 936;
const SKILLS_TITLE: usize = 937;
const NAME: usize = 407;
const SKILL_POINTS: usize = 284;
const LEVEL: usize = 408;
const HUNGER: usize = 585;
const INTIMACY: usize = 586;
const EXP: usize = 1019;
/// "receives 10% of master's experience", red in the white band.
const NOTE: usize = 3284;
const NOTE_COLOR: [u8; 3] = [0xFF, 0, 0];
/// The automatic feeding box's two-line label.
const AUTO_FEED: usize = 3279;
/// The starving line.
pub const STARVING: usize = 1018;
/// A new name: too long, or holding an item link (boxes); else asked with the name.
pub const NAME_TOO_LONG: usize = 938;
pub const RENAME_ASKS: usize = 2977;
pub const FEED_ASKS: usize = 601;
pub const DELETE_ASKS: usize = 955;
/// Feeding refused while trading.
pub const FEED_WHILE_TRADING: usize = 3551;
/// Fed: "[food] - n remaining", cyan; not fed, red.
pub const FED: usize = 3281;
pub const FED_COLOR: [u8; 3] = [0x64, 0xFF, 0xFF];
pub const NOT_FED: usize = 591;
pub const NOT_FED_COLOR: [u8; 3] = [0xF5, 0x1E, 0x1E];
/// The server's word on automatic feeding: on, off.
pub const AUTO_FEED_ON: usize = 3282;
pub const AUTO_FEED_OFF: usize = 3283;
pub const AUTO_FEED_COLOR: [u8; 3] = [0xFF, 0xFF, 0x64];

/// The message naming how fond it is (rathena's `intimacy_grades`, divided by 100).
pub fn intimacy_word(intimacy: u16) -> usize {
    match intimacy {
        911.. => 675,
        751..=910 => 674,
        251..=750 => 669,
        101..=250 => 673,
        11..=100 => 672,
        4..=10 => 1021,
        _ => 1022,
    }
}

/// A message's lines: the table writes its breaks as "\n".
fn text_lines(text: &str) -> impl Iterator<Item = &str> {
    text.split("\\n").flat_map(str::lines)
}

/// Whether the homunculus's full status says "starving": hunger at exactly 1, 5 or 10, and not the hunger last
/// warned about.
pub fn starving_now(warned: Option<u16>, hunger: u16) -> bool {
    matches!(hunger, 1 | 5 | 10) && warned != Some(hunger)
}

/// Places from the window's corner (its title bar included).
const WIDTH: f32 = 280.0;
const HEIGHT: f32 = 208.0;
const POSITION: [f32; 2] = [150.0, 100.0];
/// The white band under the background, and the note in it.
const BAND_TOP: f32 = 168.0;
const NOTE_AT: [f32; 2] = [8.0, 180.0];
const LABEL: f32 = 100.0;
const VALUE: f32 = 135.0;
const NAME_TOP: f32 = 24.0;
const NAME_BOX: [f32; 4] = [135.0, 22.0, 80.0, 16.0];
const LEVEL_TOP: f32 = 47.0;
const BUTTON: [f32; 2] = [42.0, 20.0];
const RENAME: [f32; 2] = [232.0, 21.0];
const DELETE: [f32; 2] = [187.0, 44.0];
const SKILLS: [f32; 2] = [232.0, 44.0];
const FEED: [f32; 2] = [210.0, 105.0];
const FEED_SIZE: [f32; 2] = [64.0, 20.0];
/// HP and SP: 157 × 9 each; the fill's left end is 4 px, its middle starts there.
const HP_BAR: [f32; 2] = [117.0, 73.0];
const SP_BAR: [f32; 2] = [117.0, 88.0];
const BAR: [f32; 2] = [157.0, 9.0];
const FILL_END: f32 = 4.0;
const EXP_TOP: f32 = 105.0;
const EXP_RIGHT: f32 = 205.0;
const EXP_BAR: [f32; 4] = [101.0, 119.0, 102.0, 6.0];
const HUNGER_TOP: f32 = 131.0;
const HUNGER_RIGHT: f32 = 200.0;
const HUNGER_BAR: [f32; 4] = [101.0, 145.0, 102.0, 6.0];
const AUTO_BOX: [f32; 2] = [210.0, 142.0];
const CHECK: [f32; 2] = [12.0, 12.0];
const AUTO_LABEL: [f32; 2] = [225.0, 136.0];
const INTIMACY_TOP: f32 = 159.0;
const COLON: f32 = 140.0;
const WORD: f32 = 150.0;
const TEXT: [u8; 3] = [0, 0, 0];
const BAR_FRAME: [u8; 4] = [0xAF, 0xAF, 0xAF, 0xFF];
const BAR_BLUE: [u8; 4] = [0x56, 0x7F, 0xCF, 0xFF];
const BAR_RED: [u8; 4] = [0xFF, 0x00, 0x00, 0xFF];
/// The exp a level with no next one asks for.
const NO_NEXT: i64 = 99_999_999;

pub struct Assets {
    background: Option<Texture>,
    /// The HP and SP bars' track, blue and red fills: left, middle, right.
    track: [Texture; 3],
    fills: [[Texture; 3]; 2],
    /// Normal, hovered, pressed.
    rename: [Texture; 3],
    delete: [Texture; 3],
    skill: [Texture; 3],
    feed: [Texture; 3],
    check: [Texture; 2],
    bar_frame: Texture,
    bar_blue: Texture,
    bar_red: Texture,
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let basic = |name: &str| gpu.load_texture(grf, &format!("{UI}basic_interface\\{name}.bmp"), false);
        let button = |name: &str| ["", "_a", "_b"].map(|suffix| gpu.load_texture(grf, &format!("{UI}btn_{name}{suffix}.bmp"), false));
        let pieces = |color: &str| ["left", "mid", "right"].map(|part| basic(&format!("gze{color}_{part}")));
        Self {
            background: gpu.try_load_texture(grf, &format!("{UI}basic_interface\\homuninfo_bg.bmp"), false).ok(),
            track: ["gze_bg_left", "gze_bg_mid", "gze_bg_right"].map(basic),
            fills: [pieces("blue"), pieces("red")],
            rename: button("rewrite"),
            delete: button("del"),
            skill: button("skill"),
            feed: button("feed"),
            check: ["checkbox_0", "checkbox_1"].map(|name| gpu.load_texture(grf, &format!("{UI}{name}.bmp"), false)),
            bar_frame: gpu.solid(BAR_FRAME),
            bar_blue: gpu.solid(BAR_BLUE),
            bar_red: gpu.solid(BAR_RED),
        }
    }
}

/// An HP or SP bar: nothing past its maximum; else the track, and unless the maximum or the value is 0, the fill's
/// middle width and whether it's red (under a quarter).
#[derive(Debug, PartialEq)]
pub enum Gauge {
    Hidden,
    Empty,
    Filled(f32, bool),
}

pub fn gauge(value: u32, most: u32, width: f32) -> Gauge {
    if value > most {
        Gauge::Hidden
    } else if value == 0 || most == 0 {
        Gauge::Empty
    } else {
        let middle = ((width - 2.0 * FILL_END) * value as f32 / most as f32).trunc();
        Gauge::Filled(middle, u64::from(value) * 4 < u64::from(most))
    }
}

/// The numbers over an HP or SP bar: three wide, two spaces each side of the slash.
pub fn gauge_text(value: u32, most: u32) -> String {
    format!("{value:>3}  /  {most:>3}")
}

/// The exp bar's fill width, or None when there's no bar: exp past the next level's, or no next level.
pub fn exp_fill(exp: i64, next: i64) -> Option<f32> {
    (exp <= next && next != 0 && next != NO_NEXT).then(|| ((EXP_BAR[2] - 2.0) * exp.max(0) as f32 / next as f32).trunc())
}

/// The hunger bar's fill width and whether it's red (under 25), or None past 100.
pub fn hunger_fill(hunger: u16) -> Option<(f32, bool)> {
    (hunger <= 100).then(|| (((HUNGER_BAR[2] - 2.0) * f32::from(hunger) / 100.0).trunc(), hunger < 25))
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Close,
    Rename,
    /// The name box: typing goes to it.
    NameBox,
    Delete,
    Skills,
    Feed,
    AutoFeed,
    /// Anywhere else: the window drags from it.
    Window,
}

pub struct Window {
    pub frame: Frame,
    /// The name box's text once it's been typed in (None: the homunculus's name), and whether typing goes to it.
    pub name_text: Option<String>,
    pub typing: bool,
    /// The automatic feeding box, as the server last said.
    pub auto_feed: bool,
}

impl Default for Window {
    fn default() -> Self {
        Self { frame: Frame::new(POSITION, WIDTH), name_text: None, typing: false, auto_feed: false }
    }
}

impl Window {
    pub fn height() -> f32 {
        HEIGHT
    }

    /// The name the box shows: what's typed, else the homunculus's.
    pub fn name<'a>(&'a self, homunculus: &'a Homunculus) -> &'a str {
        self.name_text.as_deref().unwrap_or(&homunculus.name)
    }

    const PARTS: [(Hit, [f32; 2], [f32; 2]); 6] = [
        (Hit::Rename, RENAME, BUTTON),
        (Hit::NameBox, [NAME_BOX[0], NAME_BOX[1]], [NAME_BOX[2], NAME_BOX[3]]),
        (Hit::Delete, DELETE, BUTTON),
        (Hit::Skills, SKILLS, BUTTON),
        (Hit::Feed, FEED, FEED_SIZE),
        (Hit::AutoFeed, AUTO_BOX, CHECK),
    ];

    /// The buttons and boxes on screen (for the test driver).
    pub fn button_boxes(&self) -> Vec<(&'static str, [f32; 4])> {
        if !self.frame.open {
            return Vec::new();
        }
        let names = ["rename", "homunculus_name", "delete", "skill", "feed", "homunculus_auto_feed"];
        names
            .into_iter()
            .zip(Self::PARTS)
            .map(|(name, (_, at, size))| {
                let [x, y] = self.frame.at(at);
                (name, [x, y, size[0], size[1]])
            })
            .collect()
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let local = self.frame.local(cursor, HEIGHT)?;
        if self.frame.title_hit(local) == Some(TitleHit::Close) {
            return Some(Hit::Close);
        }
        Some(Self::PARTS.iter().find(|(_, at, size)| contains(*at, *size, local)).map_or(Hit::Window, |&(hit, ..)| hit))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &'a self,
        layer: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        font: &Font,
        messages: &[String],
        homunculus: &Homunculus,
        pointer: ([f32; 2], bool),
        blink: bool,
    ) {
        if !self.frame.open {
            return;
        }
        let frame = &self.frame;
        let at = |local: [f32; 2]| frame.at(local);
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        let text = |layer: &mut Layer<'a>, text: String, place: [f32; 2]| layer.texts.push((text, at(place), TEXT));
        let right = |layer: &mut Layer<'a>, value: String, edge: f32, top: f32| {
            let x = (edge - font.width(&value)).floor();
            layer.texts.push((value, at([x, top]), TEXT));
        };
        let hit = self.hit(pointer.0);
        let look = |target: Hit| {
            let hovered = hit == Some(target);
            usize::from(hovered) + usize::from(hovered && pointer.1)
        };
        // The white band with its red note, the background (bringing its title strip) over it.
        layer.sprites.push((&chrome.white, at([0.0, BAND_TOP]), [WIDTH, HEIGHT - BAND_TOP]));
        match &assets.background {
            Some(background) => layer.sprites.push((background, at([0.0, 0.0]), background.size)),
            None => chrome.draw_title_bar(layer, at([0.0, 0.0]), WIDTH),
        }
        for (row, line) in text_lines(message(NOTE)).enumerate() {
            layer.texts.push((line.to_owned(), at([NOTE_AT[0], NOTE_AT[1] + row as f32 * font.line_height()]), NOTE_COLOR));
        }
        frame.draw_bare_title(layer, chrome, message(TITLE), pointer.0);
        let h = homunculus;
        draw_values(layer, frame, font, [h.atk, h.matk, h.hit, h.critical, h.def, h.mdef, h.flee], h.aspd);
        // Name (edited in place) and level, the buttons.
        text(layer, message(NAME).to_owned(), [LABEL, NAME_TOP]);
        let [x, y, w, h_box] = NAME_BOX;
        let shown = if self.typing && blink { format!("{}|", self.name(h)) } else { self.name(h).to_owned() };
        text(layer, window::fit(font, &shown, w - 4.0), [x + 2.0, y + ((h_box - font.line_height()) / 2.0).floor()]);
        text(layer, message(LEVEL).to_owned(), [LABEL, LEVEL_TOP]);
        text(layer, h.level.to_string(), [VALUE, LEVEL_TOP]);
        for (images, target, place, size) in [
            (&assets.rename, Hit::Rename, RENAME, BUTTON),
            (&assets.delete, Hit::Delete, DELETE, BUTTON),
            (&assets.skill, Hit::Skills, SKILLS, BUTTON),
            (&assets.feed, Hit::Feed, FEED, FEED_SIZE),
        ] {
            layer.sprites.push((&images[look(target)], at(place), size));
        }
        // HP and SP.
        for (place, (value, most)) in [(HP_BAR, (h.hp, h.max_hp)), (SP_BAR, (h.sp, h.max_sp))] {
            let gauge = gauge(value, most, BAR[0]);
            if gauge == Gauge::Hidden {
                continue;
            }
            let [left, middle, end] = &assets.track;
            layer.sprites.push((left, at(place), [left.size[0], BAR[1]]));
            layer.sprites.push((middle, at([place[0] + left.size[0], place[1]]), [BAR[0] - left.size[0] - end.size[0], BAR[1]]));
            layer.sprites.push((end, at([place[0] + BAR[0] - end.size[0], place[1]]), [end.size[0], BAR[1]]));
            if let Gauge::Filled(width, red) = gauge {
                let [left, middle, end] = &assets.fills[usize::from(red)];
                layer.sprites.push((left, at(place), [FILL_END, BAR[1]]));
                layer.sprites.push((middle, at([place[0] + FILL_END, place[1]]), [width, BAR[1]]));
                layer.sprites.push((end, at([place[0] + FILL_END + width, place[1]]), [end.size[0], BAR[1]]));
                // ponytail: centred on the bar.
                let numbers = gauge_text(value, most);
                let x = place[0] + ((BAR[0] - font.width(&numbers)) / 2.0).floor();
                text(layer, numbers, [x, place[1] - 2.0]);
            }
        }
        // Exp: the next level's total, its bar.
        text(layer, message(EXP).to_owned(), [LABEL, EXP_TOP]);
        right(layer, h.next_exp.to_string(), EXP_RIGHT, EXP_TOP - 1.0);
        self.small_bar(layer, assets, chrome, EXP_BAR, exp_fill(h.exp, h.next_exp).map(|width| (width, false)));
        // Hunger: "n / 100", its bar.
        text(layer, message(HUNGER).to_owned(), [LABEL, HUNGER_TOP]);
        right(layer, format!("{} / 100", h.hunger), HUNGER_RIGHT, HUNGER_TOP - 1.0);
        if let Some(fill) = hunger_fill(h.hunger) {
            self.small_bar(layer, assets, chrome, HUNGER_BAR, Some(fill));
        }
        // Automatic feeding.
        let check = &assets.check[usize::from(self.auto_feed)];
        layer.sprites.push((check, at(AUTO_BOX), CHECK));
        for (row, line) in text_lines(message(AUTO_FEED)).enumerate() {
            text(layer, line.to_owned(), [AUTO_LABEL[0], AUTO_LABEL[1] + row as f32 * font.line_height()]);
        }
        // Intimacy (the word keeps its leading space).
        text(layer, message(INTIMACY).to_owned(), [LABEL, INTIMACY_TOP]);
        text(layer, ":".to_owned(), [COLON, INTIMACY_TOP]);
        text(layer, message(intimacy_word(h.intimacy)).trim().to_owned(), [WORD, INTIMACY_TOP]);
    }

    /// A thin bar: the grey frame, white inside, the fill (`None`: no bar).
    fn small_bar<'a>(&self, layer: &mut Layer<'a>, assets: &'a Assets, chrome: &'a Chrome, [x, y, w, h]: [f32; 4], fill: Option<(f32, bool)>) {
        let Some((width, red)) = fill else { return };
        layer.sprites.push((&assets.bar_frame, self.frame.at([x, y]), [w, h]));
        layer.sprites.push((&chrome.white, self.frame.at([x + 1.0, y + 1.0]), [w - 2.0, h - 2.0]));
        if width > 0.0 {
            layer.sprites.push((if red { &assets.bar_red } else { &assets.bar_blue }, self.frame.at([x + 1.0, y + 1.0]), [width, h - 2.0]));
        }
    }
}

// ponytail: the skill list as the guild window's skill page: a grid of boxes, four to a row, level up buttons on
// the boxes and the points on a line below.
const SKILL_COLUMNS: usize = 4;
const SKILL_SIDE: f32 = 8.0;
const SKILL_WIDTH: f32 = SKILL_SIDE * 2.0 + SKILL_COLUMNS as f32 * skills::CELL[0];
const POINTS_LINE: f32 = 22.0;
const RAISE: [f32; 2] = [24.0, 24.0];

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum SkillHit {
    Title(TitleHit),
    Skill(u16),
    Raise(u16),
    Window,
}

pub struct SkillList {
    pub frame: Frame,
}

impl Default for SkillList {
    fn default() -> Self {
        Self { frame: Frame::new([((screen()[0] - SKILL_WIDTH) / 2.0).floor() - 120.0, 190.0], SKILL_WIDTH) }
    }
}

impl SkillList {
    pub fn height(count: usize) -> f32 {
        TITLE_HEIGHT + 6.0 + count.div_ceil(SKILL_COLUMNS).max(1) as f32 * skills::CELL[1] + POINTS_LINE
    }

    fn corner(index: usize) -> [f32; 2] {
        let [column, row] = [index % SKILL_COLUMNS, index / SKILL_COLUMNS];
        [SKILL_SIDE + column as f32 * skills::CELL[0] + (skills::CELL[0] - skills::BOX) / 2.0, TITLE_HEIGHT + 6.0 + row as f32 * skills::CELL[1]]
    }

    pub fn hit(&self, cursor: [f32; 2], skills: &[Skill], points: u16) -> Option<SkillHit> {
        let local = self.frame.local(cursor, Self::height(skills.len()))?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(SkillHit::Title(hit));
        }
        for (index, skill) in skills.iter().enumerate() {
            let corner = Self::corner(index);
            if points > 0 && skill.upgradable && contains(offset(corner, skills::UP), RAISE, local) {
                return Some(SkillHit::Raise(skill.id));
            }
            if contains(corner, [skills::BOX; 2], local) {
                return Some(SkillHit::Skill(skill.id));
            }
        }
        Some(SkillHit::Window)
    }

    /// Each skill's box on screen (for the test driver).
    pub fn skill_boxes(&self, skills: &[Skill]) -> Vec<(u16, [f32; 4])> {
        if !self.frame.open || self.frame.minimized {
            return Vec::new();
        }
        skills
            .iter()
            .enumerate()
            .map(|(index, skill)| {
                let [x, y] = self.frame.at(Self::corner(index));
                (skill.id, [x, y, skills::BOX, skills::BOX])
            })
            .collect()
    }

    /// The level up buttons on screen: only with points to spend, on skills that can rise.
    pub fn raise_boxes(&self, skills: &[Skill], points: u16) -> Vec<(u16, [f32; 4])> {
        if !self.frame.open || self.frame.minimized || points == 0 {
            return Vec::new();
        }
        skills
            .iter()
            .enumerate()
            .filter(|(_, skill)| skill.upgradable)
            .map(|(index, skill)| {
                let [x, y] = self.frame.at(offset(Self::corner(index), skills::UP));
                (skill.id, [x, y, RAISE[0], RAISE[1]])
            })
            .collect()
    }

    pub fn icons(skills: &[Skill], info: &HashMap<u16, SkillInfo>) -> Vec<String> {
        skills.iter().filter_map(|skill| info.get(&skill.id).map(skills::icon_path)).collect()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &'a self,
        layer: &mut Layer<'a>,
        chrome: &'a Chrome,
        skill_assets: &'a skills::Assets,
        icons: &'a HashMap<String, Option<Texture>>,
        font: &Font,
        messages: &[String],
        skills: &[Skill],
        points: u16,
        info: &HashMap<u16, SkillInfo>,
        pointer: ([f32; 2], bool),
    ) {
        if !self.frame.open {
            return;
        }
        let frame = &self.frame;
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        frame.draw_title(layer, chrome, message(SKILLS_TITLE), pointer.0);
        if frame.minimized {
            return;
        }
        let height = Self::height(skills.len()) - TITLE_HEIGHT;
        layer.sprites.push((&chrome.white, frame.at([0.0, TITLE_HEIGHT]), [SKILL_WIDTH, height]));
        layer.sprites.push((&chrome.border, frame.at([0.0, TITLE_HEIGHT]), [1.0, height]));
        layer.sprites.push((&chrome.border, frame.at([SKILL_WIDTH - 1.0, TITLE_HEIGHT]), [1.0, height]));
        layer.sprites.push((&chrome.border, frame.at([0.0, TITLE_HEIGHT + height - 1.0]), [SKILL_WIDTH, 1.0]));
        let hit = self.hit(pointer.0, skills, points);
        for (index, skill) in skills.iter().enumerate() {
            let corner = Self::corner(index);
            let info = info.get(&skill.id);
            let name = info.map_or_else(|| skill.id.to_string(), |info| info.name.clone());
            skills::draw_box(layer, skill_assets, icons, font, frame.at(corner), info, &name, skill.level);
            if points > 0 && skill.upgradable {
                let state = if hit == Some(SkillHit::Raise(skill.id)) { 1 + usize::from(pointer.1) } else { 0 };
                let up = &skill_assets.up[state];
                layer.sprites.push((up, frame.at(offset(corner, skills::UP)), up.size));
            }
        }
        let line = messages::fill(message(SKILL_POINTS), &[&points]);
        let top = Self::height(skills.len()) - POINTS_LINE + ((POINTS_LINE - font.line_height()) / 2.0).floor();
        layer.texts.push((line, frame.at([SKILL_SIDE, top]), TEXT));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skill(id: u16, level: u16, upgradable: bool) -> Skill {
        Skill { id, kind: 0, level, sp: 0, range: 0, upgradable }
    }

    fn centre(rect: [f32; 4]) -> [f32; 2] {
        [rect[0] + rect[2] / 2.0, rect[1] + rect[3] / 2.0]
    }

    fn open_window() -> Window {
        let mut window = Window::default();
        window.frame.open = true;
        window
    }

    fn open_list() -> SkillList {
        let mut list = SkillList::default();
        list.frame.open = true;
        list
    }

    #[test]
    fn names_intimacy_and_says_starving() {
        assert_eq!(
            [0, 3, 4, 10, 11, 100, 101, 250, 251, 750, 751, 910, 911, 1000].map(intimacy_word),
            [1022, 1022, 1021, 1021, 672, 672, 673, 673, 669, 669, 674, 674, 675, 675]
        );
        // At exactly 1, 5 or 10, and not the hunger last warned about.
        assert_eq!([0, 1, 2, 5, 9, 10, 11].map(|hunger| starving_now(None, hunger)), [false, true, false, true, false, true, false]);
        assert!(!starving_now(Some(10), 10));
        assert!(starving_now(Some(10), 5));
        assert_eq!(text_lines("Auto \\nFeeding").collect::<Vec<_>>(), ["Auto ", "Feeding"]);
    }

    #[test]
    fn hits_the_window_buttons() {
        let window = open_window();
        let boxes = window.button_boxes();
        let expected = [Hit::Rename, Hit::NameBox, Hit::Delete, Hit::Skills, Hit::Feed, Hit::AutoFeed];
        for ((name, rect), hit) in boxes.iter().zip(expected) {
            assert_eq!(window.hit(centre(*rect)), Some(hit), "{name}");
        }
        // At (150, 100): Rename (382, 121), Delete (337, 144), Skills (382, 144), Feed (360, 205).
        let corner = |name: &str| boxes.iter().find(|(found, _)| *found == name).map(|(_, [x, y, ..])| [*x, *y]).unwrap();
        assert_eq!([corner("rename"), corner("delete"), corner("skill"), corner("feed")], [[382.0, 121.0], [337.0, 144.0], [382.0, 144.0], [360.0, 205.0]]);
        // Only the close button on the title bar; the rest drags.
        let [x, y] = window.frame.position;
        assert_eq!(window.hit([x + 270.0, y + 8.0]), Some(Hit::Close));
        assert_eq!(window.hit([x + 255.0, y + 8.0]), Some(Hit::Window));
        assert_eq!(window.hit([x + 20.0, y + 190.0]), Some(Hit::Window));
        assert_eq!(window.hit([x + 20.0, y + 208.0]), None);
        assert!(Window::default().button_boxes().is_empty());
        // A new name: too long, then an item link, then asked.
        assert_eq!(crate::pet::rename_refusal(&"a".repeat(24), NAME_TOO_LONG), Some(938));
        assert_eq!(crate::pet::rename_refusal("<ITEML>", NAME_TOO_LONG), Some(2812));
    }

    #[test]
    fn fills_the_bars() {
        // HP and SP: the middle over 149 px, red under a quarter, nothing past the maximum, no fill at 0.
        assert_eq!(gauge(100, 100, BAR[0]), Gauge::Filled(149.0, false));
        assert_eq!(gauge(25, 100, BAR[0]), Gauge::Filled(37.0, false));
        assert_eq!(gauge(24, 100, BAR[0]), Gauge::Filled(35.0, true));
        assert_eq!(gauge(101, 100, BAR[0]), Gauge::Hidden);
        assert_eq!([gauge(0, 100, BAR[0]), gauge(0, 0, BAR[0])], [Gauge::Empty, Gauge::Empty]);
        assert_eq!(gauge_text(20, 150), " 20  /  150");
        // Exp: none past the next level's or without one.
        assert_eq!(exp_fill(50, 100), Some(50.0));
        assert_eq!([exp_fill(101, 100), exp_fill(0, 0), exp_fill(5, NO_NEXT)], [None, None, None]);
        // Hunger: red under 25, nothing past 100, only the frame at 0.
        assert_eq!([hunger_fill(20), hunger_fill(25), hunger_fill(0)], [Some((20.0, true)), Some((25.0, false)), Some((0.0, true))]);
        assert_eq!(hunger_fill(101), None);
    }

    #[test]
    fn hits_the_skill_list() {
        let list = open_list();
        let skills = [skill(8001, 3, true), skill(8002, 1, false), skill(8003, 1, true)];
        let boxes = list.skill_boxes(&skills);
        assert_eq!(boxes.iter().map(|(id, _)| *id).collect::<Vec<_>>(), [8001, 8002, 8003]);
        for (id, rect) in &boxes {
            assert_eq!(list.hit(centre(*rect), &skills, 0), Some(SkillHit::Skill(*id)));
        }
        let raises = list.raise_boxes(&skills, 2);
        assert_eq!(raises.iter().map(|(id, _)| *id).collect::<Vec<_>>(), [8001, 8003]);
        for (id, rect) in &raises {
            assert_eq!(list.hit(centre(*rect), &skills, 2), Some(SkillHit::Raise(*id)));
        }
    }

    #[test]
    fn raises_need_points() {
        let list = open_list();
        let skills = [skill(8001, 3, true), skill(8002, 1, false)];
        assert!(list.raise_boxes(&skills, 0).is_empty());
        assert!(list.raise_boxes(&[skill(8002, 1, false)], 5).is_empty());
    }

    #[test]
    fn a_click_outside_the_list_hits_nothing() {
        let list = open_list();
        let skills = [skill(8001, 3, true)];
        assert_eq!(list.hit([-100.0, -100.0], &skills, 1), None);
    }
}
