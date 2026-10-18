//! The player's homunculus: its window (Alt+R), its skill list, the words for its hunger and intimacy.

use std::collections::HashMap;

use rustnarok_client::grf::Grf;
use rustnarok_client::lua::SkillInfo;
use rustnarok_client::net::{Homunculus, Skill};
use rustnarok_client::text::Font;

use crate::mercenary::{
    BACKGROUND_HEIGHT, BACKGROUND_TOP, BARS, BUTTON, FOOTER, LOWER_ROWS, RIGHT_LABEL, RIGHT_VALUE, TEXT, UPPER_ROWS, WIDTH, draw_background, draw_bars,
    draw_values, load_bars, right_text,
};
use crate::render::{Gpu, Texture};
use crate::window::{Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI, contains, messages, offset, screen, skills};

const TITLE: usize = 936;
const SKILLS_TITLE: usize = 937;
const NAME: usize = 282;
const SKILL_POINTS: usize = 284;
const LEVEL: usize = 408;
const HUNGER: usize = 585;
const INTIMACY: usize = 586;
const EXP: usize = 1019;
/// " Very Hungry" to " Stuffed", by hunger band.
const HUNGER_WORDS: [usize; 5] = [667, 668, 669, 670, 671];
/// The message of the starving line.
pub const STARVING: usize = 1018;
/// The question before a rename, and the name box's text.
pub const RENAME_QUESTION: usize = 939;
pub const RENAME_PROMPT: usize = 938;

/// The message naming how full it is.
pub fn hunger_word(hunger: u16) -> usize {
    match hunger {
        91.. => HUNGER_WORDS[4],
        76..=90 => HUNGER_WORDS[3],
        26..=75 => HUNGER_WORDS[2],
        11..=25 => HUNGER_WORDS[1],
        _ => HUNGER_WORDS[0],
    }
}

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

/// Whether a hunger update says "starving" now: it's at 10 or less and wasn't before.
// ponytail: when the official client says it isn't knowable; once per fall to 10 or less.
pub fn starving_now(previous: Option<u16>, hunger: u16) -> bool {
    hunger <= 10 && previous.is_none_or(|previous| previous > 10)
}

// ponytail: the layout off the background (the buttons on a footer bar, the words in place of the bars) is an estimate.
const VAPORIZED: &str = "Vaporized";
const DEAD: &str = "Dead";
const BUTTON_GAP: f32 = 4.0;

pub struct Assets {
    background: Option<Texture>,
    /// Blue and red bars' left, middle and right.
    bars: [[Texture; 3]; 2],
    footer: Texture,
    /// Normal, hovered, pressed.
    feed: [Texture; 3],
    skill: [Texture; 3],
    rename: [Texture; 3],
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let basic = format!("{UI}basic_interface\\");
        let button = |name: &str| ["", "_a", "_b"].map(|suffix| gpu.load_texture(grf, &format!("{UI}btn_{name}{suffix}.bmp"), false));
        Self {
            background: gpu.try_load_texture(grf, &format!("{basic}homuninfo_bg.bmp"), false).ok(),
            bars: load_bars(gpu, grf),
            footer: gpu.load_texture(grf, &format!("{basic}btnbar_mid2.bmp"), false),
            feed: button("feed"),
            skill: button("skill"),
            rename: button("rewrite"),
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(TitleHit),
    Feed,
    Skills,
    Rename,
    Window,
}

pub struct Window {
    pub frame: Frame,
}

impl Default for Window {
    fn default() -> Self {
        Self { frame: Frame::new([((screen()[0] - WIDTH) / 2.0).floor() + 120.0, 170.0], WIDTH) }
    }
}

impl Window {
    pub fn height() -> f32 {
        BACKGROUND_TOP + BACKGROUND_HEIGHT + FOOTER
    }

    /// A button's corner: feed, skill and rename from the left.
    fn button_at(slot: usize) -> [f32; 2] {
        [8.0 + slot as f32 * (BUTTON[0] + BUTTON_GAP), BACKGROUND_TOP + BACKGROUND_HEIGHT + 4.0]
    }

    /// The buttons on screen (for the test driver): rename only until it has been renamed.
    pub fn button_boxes(&self, homunculus: &Homunculus) -> Vec<(&'static str, [f32; 4])> {
        if !self.frame.open || self.frame.minimized {
            return Vec::new();
        }
        let place = |slot: usize| {
            let [x, y] = self.frame.at(Self::button_at(slot));
            [x, y, BUTTON[0], BUTTON[1]]
        };
        let mut boxes = vec![("feed", place(0)), ("skill", place(1))];
        if !homunculus.renamed() {
            boxes.push(("rename", place(2)));
        }
        boxes
    }

    pub fn hit(&self, cursor: [f32; 2], homunculus: &Homunculus) -> Option<Hit> {
        let local = self.frame.local(cursor, Self::height())?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(Hit::Title(hit));
        }
        if contains(Self::button_at(0), BUTTON, local) {
            return Some(Hit::Feed);
        }
        if contains(Self::button_at(1), BUTTON, local) {
            return Some(Hit::Skills);
        }
        if !homunculus.renamed() && contains(Self::button_at(2), BUTTON, local) {
            return Some(Hit::Rename);
        }
        Some(Hit::Window)
    }

    pub fn draw<'a>(
        &'a self,
        layer: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        font: &Font,
        messages: &[String],
        homunculus: &Homunculus,
        pointer: ([f32; 2], bool),
    ) {
        if !self.frame.open {
            return;
        }
        let frame = &self.frame;
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        if !frame.minimized {
            draw_background(layer, frame, chrome, assets.background.as_ref());
        }
        frame.draw_title(layer, chrome, message(TITLE), pointer.0);
        if frame.minimized {
            return;
        }
        let h = homunculus;
        draw_values(layer, frame, font, [h.atk, h.matk, h.hit, h.critical, h.def, h.mdef, h.flee], h.aspd);
        if h.vaporized() || !h.alive() {
            let word = if h.vaporized() { VAPORIZED } else { DEAD };
            layer.texts.push((word.to_owned(), frame.at([BARS[0][0], BACKGROUND_TOP + BARS[0][1]]), TEXT));
        } else {
            draw_bars(layer, frame, &assets.bars, (h.hp, h.max_hp), (h.sp, h.max_sp));
        }
        let exp = format!("{}/{}", h.exp, h.next_exp);
        let rows = [
            (UPPER_ROWS[0], NAME, crate::window::fit(font, &h.name, RIGHT_VALUE - RIGHT_LABEL - font.width(message(NAME)) - 4.0)),
            (UPPER_ROWS[1], LEVEL, h.level.to_string()),
            (LOWER_ROWS[0], EXP, crate::window::fit(font, &exp, RIGHT_VALUE - RIGHT_LABEL - font.width(message(EXP)) - 4.0)),
            (LOWER_ROWS[1], HUNGER, message(hunger_word(h.hunger)).trim().to_owned()),
            (LOWER_ROWS[2], INTIMACY, message(intimacy_word(h.intimacy)).trim().to_owned()),
        ];
        for (top, label, value) in rows {
            layer.texts.push((message(label).to_owned(), frame.at([RIGHT_LABEL, BACKGROUND_TOP + top]), TEXT));
            layer.texts.push(right_text(font, frame, value, RIGHT_VALUE, top));
        }
        layer.sprites.push((&assets.footer, frame.at([0.0, BACKGROUND_TOP + BACKGROUND_HEIGHT]), [WIDTH, FOOTER]));
        let hit = self.hit(pointer.0, homunculus);
        let look = |target: Hit| {
            let hovered = hit == Some(target);
            usize::from(hovered) + usize::from(hovered && pointer.1)
        };
        layer.sprites.push((&assets.feed[look(Hit::Feed)], frame.at(Self::button_at(0)), BUTTON));
        layer.sprites.push((&assets.skill[look(Hit::Skills)], frame.at(Self::button_at(1)), BUTTON));
        if !homunculus.renamed() {
            layer.sprites.push((&assets.rename[look(Hit::Rename)], frame.at(Self::button_at(2)), BUTTON));
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
    fn names_hunger_and_intimacy() {
        assert_eq!([0, 10, 11, 25, 26, 75, 76, 90, 91, 100].map(hunger_word), [667, 667, 668, 668, 669, 669, 670, 670, 671, 671]);
        assert_eq!(
            [0, 3, 4, 10, 11, 100, 101, 250, 251, 750, 751, 910, 911, 1000].map(intimacy_word),
            [1022, 1022, 1021, 1021, 672, 672, 673, 673, 669, 669, 674, 674, 675, 675]
        );
    }

    #[test]
    fn says_starving_once_per_crossing() {
        assert!(starving_now(Some(11), 10));
        assert!(!starving_now(Some(10), 9));
        assert!(starving_now(None, 5));
        assert!(!starving_now(Some(5), 50));
        assert!(!starving_now(None, 11));
    }

    #[test]
    fn hits_the_window_buttons() {
        let window = open_window();
        let homunculus = Homunculus::default();
        let boxes = window.button_boxes(&homunculus);
        assert_eq!(boxes.iter().map(|(name, _)| *name).collect::<Vec<_>>(), ["feed", "skill", "rename"]);
        for (name, rect) in &boxes {
            let expected = match *name {
                "feed" => Hit::Feed,
                "skill" => Hit::Skills,
                _ => Hit::Rename,
            };
            assert_eq!(window.hit(centre(*rect), &homunculus), Some(expected), "{name}");
        }
    }

    #[test]
    fn renaming_is_once() {
        let window = open_window();
        let renamed = Homunculus { flags: 0b001, ..Default::default() };
        let boxes = window.button_boxes(&renamed);
        assert_eq!(boxes.iter().map(|(name, _)| *name).collect::<Vec<_>>(), ["feed", "skill"]);
        let unnamed = window.button_boxes(&Homunculus::default());
        let rename = unnamed.iter().find(|(name, _)| *name == "rename").unwrap().1;
        assert_ne!(window.hit(centre(rename), &renamed), Some(Hit::Rename));
    }

    #[test]
    fn a_closed_window_has_no_boxes() {
        let window = Window::default();
        assert!(window.button_boxes(&Homunculus::default()).is_empty());
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
