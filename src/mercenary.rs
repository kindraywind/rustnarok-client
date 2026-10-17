//! The player's mercenary: its window (Ctrl+R) with its values, contract and buttons, its skill list, and how the
//! client steers it (rathena runs no AI for it: following the player is the client's).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use rustnarok_client::grf::Grf;
use rustnarok_client::lua::SkillInfo;
use rustnarok_client::net::{Mercenary, Skill};
use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};
use crate::skills;
use crate::window::{Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, SCREEN, UI, contains, offset};

const TITLE_ARCHER: usize = 1181;
const TITLE_SWORDMAN: usize = 1182;
const TITLE_SPEARMAN: usize = 1183;
const TITLE_MONSTER: usize = 1367;
const LEVEL: usize = 408;
const EXPIRATION: usize = 1184;
const LOYALTY: usize = 1185;
const SUMMONS: usize = 1186;
const KILLS: usize = 1187;
const SKILLS_TITLE: usize = 1262;
pub const EXPIRED: usize = 1266;
pub const DIED: usize = 1267;
pub const RELEASED: usize = 1268;
pub const DISMISS_QUESTION: usize = 1370;

/// The window's title, by the mercenary's job.
pub fn title(job: u16) -> usize {
    match job {
        6017..=6026 => TITLE_ARCHER,
        6027..=6036 => TITLE_SPEARMAN,
        6037..=6046 => TITLE_SWORDMAN,
        _ => TITLE_MONSTER,
    }
}

// ponytail: data.grf has no mercenary window; the homunculus's background (same labels and bar frames) under the
// window's title, the values beside its labels, the rest in its empty right half and the buttons on a bar below.
const WIDTH: f32 = 280.0;
/// The background's own title strip hides under the window's.
const BACKGROUND_TOP: f32 = TITLE_HEIGHT - 16.0;
const BACKGROUND_HEIGHT: f32 = 180.0;
const FOOTER: f32 = 27.0;
/// The eight values' right edge and their first row's top; a row every 18 px (the background's underlines).
const VALUE_RIGHT: f32 = 79.0;
const FIRST_ROW: f32 = 23.0;
const ROW: f32 = 18.0;
/// The HP and SP bars' insides.
const BARS: [[f32; 2]; 2] = [[118.0, 74.0], [118.0, 89.0]];
const BAR_FULL: f32 = 75.0;
const BAR_NUMBERS: f32 = 206.0;
/// The right half: labels, values' right edge, and the rows above and below the bars.
const RIGHT_LABEL: f32 = 100.0;
const RIGHT_VALUE: f32 = WIDTH - 8.0;
const UPPER_ROWS: [f32; 2] = [21.0, 39.0];
const LOWER_ROWS: [f32; 4] = [107.0, 125.0, 143.0, 161.0];
const BUTTON: [f32; 2] = [42.0, 20.0];
const TEXT: [u8; 3] = [0, 0, 0];

pub struct Assets {
    background: Option<Texture>,
    /// Blue and red bars' left, middle and right.
    bars: [[Texture; 3]; 2],
    footer: Texture,
    /// Normal, hovered, pressed.
    skill: [Texture; 3],
    dismiss: [Texture; 3],
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let basic = format!("{UI}basic_interface\\");
        let button = |name: &str| ["", "_a", "_b"].map(|suffix| gpu.load_texture(grf, &format!("{UI}btn_{name}{suffix}.bmp"), false));
        Self {
            background: gpu.try_load_texture(grf, &format!("{basic}homuninfo_bg.bmp"), false).ok(),
            bars: ["blue", "red"].map(|color| ["left", "mid", "right"].map(|part| gpu.load_texture(grf, &format!("{basic}gze{color}_{part}.bmp"), false))),
            footer: gpu.load_texture(grf, &format!("{basic}btnbar_mid2.bmp"), false),
            skill: button("skill"),
            dismiss: button("fired"),
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(TitleHit),
    Skills,
    Dismiss,
    Window,
}

pub struct Window {
    pub frame: Frame,
}

impl Default for Window {
    fn default() -> Self {
        Self { frame: Frame::new([((SCREEN[0] - WIDTH) / 2.0).floor() + 160.0, 150.0], WIDTH) }
    }
}

/// The time left on a contract ending at `end` (Unix seconds) as minutes and seconds.
// ponytail: how the client writes the expiration isn't knowable; minutes and seconds left.
pub fn time_left(end: u32, now: u64) -> String {
    let left = u64::from(end).saturating_sub(now);
    format!("{}:{:02}", left / 60, left % 60)
}

impl Window {
    pub fn height() -> f32 {
        BACKGROUND_TOP + BACKGROUND_HEIGHT + FOOTER
    }

    fn skills_at() -> [f32; 2] {
        [WIDTH - 2.0 * BUTTON[0] - 14.0, BACKGROUND_TOP + BACKGROUND_HEIGHT + 4.0]
    }

    fn dismiss_at() -> [f32; 2] {
        [WIDTH - BUTTON[0] - 8.0, BACKGROUND_TOP + BACKGROUND_HEIGHT + 4.0]
    }

    /// The buttons on screen (for the test driver).
    pub fn button_boxes(&self) -> Vec<(&'static str, [f32; 4])> {
        if !self.frame.open || self.frame.minimized {
            return Vec::new();
        }
        let place = |at: [f32; 2]| {
            let [x, y] = self.frame.at(at);
            [x, y, BUTTON[0], BUTTON[1]]
        };
        vec![("skill", place(Self::skills_at())), ("dismiss", place(Self::dismiss_at()))]
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let local = self.frame.local(cursor, Self::height())?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(Hit::Title(hit));
        }
        if contains(Self::skills_at(), BUTTON, local) {
            return Some(Hit::Skills);
        }
        if contains(Self::dismiss_at(), BUTTON, local) {
            return Some(Hit::Dismiss);
        }
        Some(Hit::Window)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &'a self,
        layer: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        font: &Font,
        messages: &[String],
        mercenary: &Mercenary,
        job: u16,
        now: u64,
        pointer: ([f32; 2], bool),
    ) {
        if !self.frame.open {
            return;
        }
        let frame = &self.frame;
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        if !frame.minimized {
            match &assets.background {
                Some(background) => layer.sprites.push((background, frame.at([0.0, BACKGROUND_TOP]), background.size)),
                None => layer.sprites.push((&chrome.white, frame.at([0.0, TITLE_HEIGHT]), [WIDTH, BACKGROUND_HEIGHT])),
            }
        }
        frame.draw_title(layer, chrome, message(title(job)), pointer.0);
        if frame.minimized {
            return;
        }
        let right = |text: String, right: f32, top: f32| {
            let x = (right - font.width(&text)).floor();
            (text, frame.at([x, BACKGROUND_TOP + top]), TEXT)
        };
        // Atk, Matk, Hit, Critical, Def, Mdef, Flee and Aspd, as the status window shows ASPD.
        let aspd = 200 - i32::from(mercenary.aspd) / 10;
        let values = [mercenary.atk, mercenary.matk, mercenary.hit, mercenary.critical, mercenary.def, mercenary.mdef, mercenary.flee]
            .map(|value| value.to_string())
            .into_iter()
            .chain([aspd.to_string()]);
        for (row, value) in values.enumerate() {
            layer.texts.push(right(value, VALUE_RIGHT, FIRST_ROW + row as f32 * ROW));
        }
        for (bar, (now, most)) in [(mercenary.hp, mercenary.max_hp), (mercenary.sp, mercenary.max_sp)].into_iter().enumerate() {
            let percent = if most > 0 { (u64::from(now) * 100 / u64::from(most)).min(100) } else { 0 };
            if percent > 0 {
                // Blue, red when low: SP is always blue.
                let [left, middle, end] = &assets.bars[usize::from(bar == 0 && percent < 25)];
                let width = (percent as f32 * BAR_FULL / 100.0).floor();
                let at = [BARS[bar][0], BACKGROUND_TOP + BARS[bar][1]];
                layer.sprites.push((left, frame.at(at), left.size));
                layer.sprites.push((middle, frame.at(offset(at, [left.size[0], 0.0])), [width, middle.size[1]]));
                layer.sprites.push((end, frame.at(offset(at, [left.size[0] + width, 0.0])), end.size));
            }
            layer.texts.push((format!("{now}/{most}"), frame.at([BAR_NUMBERS, BACKGROUND_TOP + BARS[bar][1] - 4.0]), TEXT));
        }
        let rows = [
            (UPPER_ROWS[0], "", crate::window::fit(font, &mercenary.name, RIGHT_VALUE - RIGHT_LABEL)),
            (UPPER_ROWS[1], message(LEVEL), mercenary.level.to_string()),
            (LOWER_ROWS[0], message(EXPIRATION), time_left(mercenary.contract_end, now)),
            (LOWER_ROWS[1], message(LOYALTY), mercenary.loyalty.to_string()),
            (LOWER_ROWS[2], message(SUMMONS), mercenary.summons.to_string()),
            (LOWER_ROWS[3], message(KILLS), mercenary.kills.to_string()),
        ];
        for (top, label, value) in rows {
            if label.is_empty() {
                layer.texts.push((value, frame.at([RIGHT_LABEL, BACKGROUND_TOP + top]), TEXT));
            } else {
                layer.texts.push((label.to_owned(), frame.at([RIGHT_LABEL, BACKGROUND_TOP + top]), TEXT));
                layer.texts.push(right(value, RIGHT_VALUE, top));
            }
        }
        layer.sprites.push((&assets.footer, frame.at([0.0, BACKGROUND_TOP + BACKGROUND_HEIGHT]), [WIDTH, FOOTER]));
        let hit = self.hit(pointer.0);
        let look = |target: Hit| {
            let hovered = hit == Some(target);
            usize::from(hovered) + usize::from(hovered && pointer.1)
        };
        layer.sprites.push((&assets.skill[look(Hit::Skills)], frame.at(Self::skills_at()), BUTTON));
        layer.sprites.push((&assets.dismiss[look(Hit::Dismiss)], frame.at(Self::dismiss_at()), BUTTON));
    }
}

// ponytail: the skill list as the guild window's skill page (a grid of boxes, no raising), four to a row.
const SKILL_COLUMNS: usize = 4;
const SKILL_SIDE: f32 = 8.0;
const SKILL_WIDTH: f32 = SKILL_SIDE * 2.0 + SKILL_COLUMNS as f32 * skills::CELL[0];

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum SkillHit {
    Title(TitleHit),
    Skill(u16),
    Window,
}

pub struct SkillList {
    pub frame: Frame,
}

impl Default for SkillList {
    fn default() -> Self {
        Self { frame: Frame::new([((SCREEN[0] - SKILL_WIDTH) / 2.0).floor() - 160.0, 150.0], SKILL_WIDTH) }
    }
}

impl SkillList {
    pub fn height(count: usize) -> f32 {
        TITLE_HEIGHT + 6.0 + count.div_ceil(SKILL_COLUMNS).max(1) as f32 * skills::CELL[1]
    }

    fn corner(index: usize) -> [f32; 2] {
        let [column, row] = [index % SKILL_COLUMNS, index / SKILL_COLUMNS];
        [SKILL_SIDE + column as f32 * skills::CELL[0] + (skills::CELL[0] - skills::BOX) / 2.0, TITLE_HEIGHT + 6.0 + row as f32 * skills::CELL[1]]
    }

    pub fn hit(&self, cursor: [f32; 2], skills: &[Skill]) -> Option<SkillHit> {
        let local = self.frame.local(cursor, Self::height(skills.len()))?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(SkillHit::Title(hit));
        }
        let index = (0..skills.len()).find(|&index| contains(Self::corner(index), [skills::BOX; 2], local));
        Some(index.map_or(SkillHit::Window, |index| SkillHit::Skill(skills[index].id)))
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
        info: &HashMap<u16, SkillInfo>,
        pointer: [f32; 2],
    ) {
        if !self.frame.open {
            return;
        }
        let frame = &self.frame;
        frame.draw_title(layer, chrome, messages.get(SKILLS_TITLE).map_or("", String::as_str), pointer);
        if frame.minimized {
            return;
        }
        let height = Self::height(skills.len()) - TITLE_HEIGHT;
        layer.sprites.push((&chrome.white, frame.at([0.0, TITLE_HEIGHT]), [SKILL_WIDTH, height]));
        layer.sprites.push((&chrome.border, frame.at([0.0, TITLE_HEIGHT]), [1.0, height]));
        layer.sprites.push((&chrome.border, frame.at([SKILL_WIDTH - 1.0, TITLE_HEIGHT]), [1.0, height]));
        layer.sprites.push((&chrome.border, frame.at([0.0, TITLE_HEIGHT + height - 1.0]), [SKILL_WIDTH, 1.0]));
        for (index, skill) in skills.iter().enumerate() {
            let info = info.get(&skill.id);
            let name = info.map_or_else(|| skill.id.to_string(), |info| info.name.clone());
            skills::draw_box(layer, skill_assets, icons, font, frame.at(Self::corner(index)), info, &name, skill.level);
        }
    }
}

// ponytail: the official client's own follow rule isn't in data.grf: more than 3 cells away, at most twice a second.
const FOLLOW_DISTANCE: i32 = 3;
const FOLLOW_EVERY: Duration = Duration::from_millis(500);

/// What the player last told the mercenary to do; following waits while it's at it.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Order {
    /// Attacking this unit, until it's gone.
    Attack(u32),
    /// Going to a cell, until the player (who stood here) moves.
    Move([i32; 2]),
}

/// How the client steers the mercenary.
pub struct Steering {
    pub following: bool,
    pub order: Option<Order>,
    last_call: Option<Instant>,
}

impl Default for Steering {
    fn default() -> Self {
        Self { following: true, order: None, last_call: None }
    }
}

impl Steering {
    /// Whether to call the mercenary back to the player now, it being at `mercenary` and the player at `player`.
    pub fn should_call(&mut self, mercenary: [i32; 2], player: [i32; 2], now: Instant) -> bool {
        if let Some(Order::Move(from)) = self.order
            && from != player
        {
            self.order = None;
        }
        let far = (mercenary[0] - player[0]).abs().max((mercenary[1] - player[1]).abs()) > FOLLOW_DISTANCE;
        let rested = self.last_call.is_none_or(|last| now.duration_since(last) >= FOLLOW_EVERY);
        let call = self.following && self.order.is_none() && far && rested;
        if call {
            self.last_call = Some(now);
        }
        call
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_by_job() {
        assert_eq!([6017, 6026, 6027, 6036, 6037, 6046, 6047, 1002].map(title), [1181, 1181, 1183, 1183, 1182, 1182, 1367, 1367]);
        assert_eq!(time_left(1_000, 1_000 - 125), "2:05");
        assert_eq!(time_left(1_000, 2_000), "0:00");
    }

    #[test]
    fn follows_unless_told_otherwise() {
        let mut steering = Steering::default();
        let now = Instant::now();
        assert!(!steering.should_call([10, 10], [12, 13], now), "within 3 cells");
        assert!(steering.should_call([10, 10], [14, 10], now));
        assert!(!steering.should_call([10, 10], [14, 10], now + Duration::from_millis(100)), "not twice in half a second");
        assert!(steering.should_call([10, 10], [14, 10], now + Duration::from_millis(600)));
        steering.order = Some(Order::Attack(5));
        assert!(!steering.should_call([10, 10], [20, 10], now + Duration::from_secs(2)), "attacking");
        steering.order = Some(Order::Move([14, 10]));
        assert!(!steering.should_call([30, 10], [14, 10], now + Duration::from_secs(3)), "sent somewhere");
        assert!(steering.should_call([30, 10], [15, 10], now + Duration::from_secs(3)), "the player walked on");
        assert_eq!(steering.order, None);
        steering.following = false;
        assert!(!steering.should_call([10, 10], [20, 10], now + Duration::from_secs(4)), "Ctrl+T");
    }

    #[test]
    fn finds_a_skill_by_its_box() {
        let mut list = SkillList::default();
        list.frame.open = true;
        let skills = [8207, 8221, 8222, 8223, 8224].map(|id| Skill { id, kind: 0, level: 1, sp: 0, range: 0, upgradable: false });
        let boxes = list.skill_boxes(&skills);
        let [x, y, w, h] = boxes[4].1;
        assert_eq!(list.hit([x + w / 2.0, y + h / 2.0], &skills), Some(SkillHit::Skill(8224)));
        assert_eq!(boxes[4].1[1] - boxes[0].1[1], skills::CELL[1], "the fifth starts the second row");
    }
}
