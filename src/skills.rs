//! The skill window (Alt+S): the job's skills where the client's skill tree puts them, a tab for each job of the
//! line, their levels, and the points left to put in.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use rustnarok_client::grf::Grf;
use rustnarok_client::lua::{SkillInfo, SkillTrees};
use rustnarok_client::messages;
use rustnarok_client::net::Skill;
use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};
use crate::window::{self, Chrome, Clicks, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI, contains, offset};

// ponytail: a grid of 28px dashed boxes 62x58 apart; calibrate against the real client.
const COLUMNS: usize = 7;
const ROWS: usize = 6;
pub const CELL: [f32; 2] = [62.0, 58.0];
pub const BOX: f32 = 28.0;
/// Where the level up button sits by a skill's box.
pub const UP: [f32; 2] = [BOX + 1.0, 2.0];
/// How much around it a click takes.
const LEVEL_UP: [f32; 2] = [24.0, 24.0];
const SIDE: f32 = 13.0;
const WIDTH: f32 = SIDE * 2.0 + COLUMNS as f32 * CELL[0];
const TAB_HEIGHT: f32 = 20.0;
const TAB_WIDTH: f32 = 90.0;
const GRID_TOP: f32 = TITLE_HEIGHT + TAB_HEIGHT + 6.0;
const BAR_HEIGHT: f32 = 32.0;
const HEIGHT: f32 = GRID_TOP + ROWS as f32 * CELL[1] + BAR_HEIGHT;
const DEFAULT_POSITION: [f32; 2] = [200.0, 120.0];
const TITLE_MESSAGE: usize = 283;
const POINTS_MESSAGE: usize = 284;
const GREY: [u8; 3] = [0x80, 0x80, 0x80];

pub struct Assets {
    /// The dashed box a skill sits in.
    slot: Texture,
    /// The level up button: normal, hovered, pressed.
    pub up: [Texture; 3],
    /// The bottom bar's left end, middle and right end.
    bar: [Texture; 3],
    /// Whitens skills not learned.
    veil: Texture,
    /// The arrows that pick the level to use: up, down.
    arrows: [Texture; 2],
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |name: &str| gpu.load_texture(grf, &format!("{UI}basic_interface\\{name}.bmp"), false);
        Self {
            slot: image("no_skill"),
            up: ["skill_up_a", "skill_up_b", "skill_up_c"].map(image),
            bar: ["skill_botbar_left", "skill_botbar_mid", "skill_botbar_right"].map(image),
            veil: gpu.solid([255, 255, 255, 150]),
            arrows: ["arw_up", "arw_down"].map(image),
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(TitleHit),
    Tab(usize),
    Skill(u16),
    LevelUp(u16),
    /// An arrow at a skill's box: one level more (`up`) or less to use, up to `learned`.
    Step {
        id: u16,
        up: bool,
        learned: u16,
    },
    Window,
}

/// What a click on the window asks for.
#[derive(Debug, PartialEq)]
pub enum Action {
    /// Use the skill at its level (a double click).
    Use(u16),
    LevelUp(u16),
}

pub struct SkillWindow {
    pub frame: Frame,
    tab: usize,
    clicks: Clicks,
    /// The level picked to use, by skill; the learned level when none is.
    chosen: HashMap<u16, u16>,
}

impl Default for SkillWindow {
    fn default() -> Self {
        Self { frame: Frame::new(DEFAULT_POSITION, WIDTH), tab: usize::MAX, clicks: Clicks::default(), chosen: HashMap::new() }
    }
}

// ponytail: the level arrows at the box's left, up over down; place them against the real client.
/// Where a skill's level arrows sit, from its box's corner: up, then down.
const ARROWS: [[f32; 2]; 2] = [[-12.0, 4.0], [-12.0, 16.0]];
const ARROW: [f32; 2] = [11.0, 11.0];

/// Whether a skill gets level arrows: learned past level 1, and used rather than passive.
fn steppable(skill: &Skill) -> bool {
    skill.level > 1 && skill.kind != 0
}

/// The tab for skills no tab of the line holds (a Wedding Ring's, an adopted family's, cards'), after the line's.
pub const ETC: u16 = u16::MAX;
// ponytail: 1471 "Etc" as its label is an estimate.
const ETC_LABEL: usize = 1471;

/// The player's skills that no tree of the line holds, in ID order.
pub fn extra_skills(skills: &[Skill], trees: &SkillTrees, jobs: &[u16]) -> Vec<u16> {
    let held = |id: u16| jobs.iter().filter_map(|job| trees.trees.get(job)).any(|tree| tree.iter().any(|&(_, each)| each == id));
    let mut extra: Vec<u16> = skills.iter().map(|skill| skill.id).filter(|&id| !held(id)).collect();
    extra.sort_unstable();
    extra.dedup();
    extra
}

/// The skill window's tabs: the job's line, then `ETC` when the player has skills none of it holds.
pub fn tabs(trees: &SkillTrees, job: u16, skills: &[Skill]) -> Vec<u16> {
    let mut jobs = trees.lineage(job);
    if !extra_skills(skills, trees, &jobs).is_empty() {
        jobs.push(ETC);
    }
    jobs
}

/// Where a job's tab sits in the window.
fn tab_corner(tab: usize) -> [f32; 2] {
    [SIDE + tab as f32 * TAB_WIDTH, TITLE_HEIGHT]
}

/// Whether a skill's level up button shows: points left, and the server says it can rise.
fn upgradable(skills: &[Skill], id: u16, points: i64) -> bool {
    points > 0 && skills.iter().any(|skill| skill.id == id && skill.upgradable)
}

/// Where a skill's box sits in the window.
fn box_corner(place: usize) -> [f32; 2] {
    let [column, row] = [place % COLUMNS, place / COLUMNS];
    [SIDE + column as f32 * CELL[0] + (CELL[0] - BOX) / 2.0, GRID_TOP + row as f32 * CELL[1] + 6.0]
}

/// Skills blocked for a while after use (ZC_SKILL_POSTDELAY): when each block began and how long it lasts. A block
/// past its end is gone: the server says nothing then.
#[derive(Default)]
pub struct Cooldowns(HashMap<u16, (Instant, Duration)>);

impl Cooldowns {
    /// A block of `ms` starts at `now`; 0 lifts it.
    pub fn set(&mut self, skill: u16, ms: u32, now: Instant) {
        if ms == 0 {
            self.0.remove(&skill);
        } else {
            self.0.insert(skill, (now, Duration::from_millis(u64::from(ms))));
        }
    }

    /// How long the skill stays blocked (None: it isn't).
    pub fn left(&self, skill: u16, now: Instant) -> Option<Duration> {
        let &(start, length) = self.0.get(&skill)?;
        Some((start + length).saturating_duration_since(now)).filter(|left| !left.is_zero())
    }

    /// How much of the block has passed, 0 to 1 (None: not blocked).
    pub fn share(&self, skill: u16, now: Instant) -> Option<f32> {
        let left = self.left(skill, now)?;
        let length = self.0[&skill].1.as_secs_f32().max(0.001);
        Some((1.0 - left.as_secs_f32() / length).clamp(0.0, 1.0))
    }

    /// The blocked skills and how long each has left.
    pub fn blocked(&self, now: Instant) -> impl Iterator<Item = (u16, Duration)> + '_ {
        self.0.keys().filter_map(move |&skill| Some((skill, self.left(skill, now)?)))
    }
}

/// What the hotkey bar and the skill window need to show cooldowns: the blocks, the clock's pictures by step, the
/// time, and the words after the seconds left (message 3230, " seconds left before it become usable again.").
pub struct Cooling<'a> {
    pub cooldowns: &'a Cooldowns,
    pub clocks: &'a [Texture],
    pub now: Instant,
    pub note: &'a str,
}

impl<'a> Cooling<'a> {
    /// Draws the clock over a blocked skill's icon, veiling the part of its time still left; returns the line saying
    /// how long that is.
    // ponytail: the status icons' clock over the icon and the line under the name; how the real client shows a
    // cooldown isn't known.
    pub fn draw(&self, layer: &mut Layer<'a>, skill: u16, corner: [f32; 2], size: [f32; 2]) -> Option<String> {
        let left = 1.0 - self.cooldowns.share(skill, self.now)?;
        let last = self.clocks.len().checked_sub(1)?;
        layer.sprites.push((&self.clocks[((left * last as f32).ceil() as usize).min(last)], corner, size));
        let seconds = self.cooldowns.left(skill, self.now)?.as_secs_f32().ceil() as u32;
        Some(format!("{seconds}{}", self.note))
    }
}

pub fn icon_path(info: &SkillInfo) -> String {
    format!("{UI}item\\{}.bmp", info.resource.to_lowercase())
}

/// A skill's box with its corner at `corner` on screen: the icon, the level in its corner and the name under it; a
/// level of 0 is a skill not learned, veiled and greyed.
#[allow(clippy::too_many_arguments)]
pub fn draw_box<'a>(
    layer: &mut Layer<'a>,
    assets: &'a Assets,
    icons: &'a HashMap<String, Option<Texture>>,
    font: &Font,
    corner: [f32; 2],
    info: Option<&SkillInfo>,
    name: &str,
    level: u16,
) {
    layer.sprites.push((&assets.slot, offset(corner, [2.0, 2.0]), assets.slot.size));
    if let Some(Some(icon)) = info.map(icon_path).and_then(|path| icons.get(&path)) {
        layer.sprites.push((icon, offset(corner, [2.0, 2.0]), icon.size));
        if level == 0 {
            layer.sprites.push((&assets.veil, offset(corner, [2.0, 2.0]), icon.size));
        }
    }
    if level > 0 {
        // The level in the box's corner, white on a dark outline.
        let text = level.to_string();
        let place = offset(corner, [BOX - font.width(&text) - 1.0, BOX - font.line_height() + 2.0]);
        for nudge in [[-1.0, 0.0], [1.0, 0.0], [0.0, -1.0], [0.0, 1.0]] {
            layer.texts.push((text.clone(), offset(place, nudge), [0x2F, 0x2F, 0x2F]));
        }
        layer.texts.push((text, place, [255; 3]));
    }
    let shown = window::fit(font, name, CELL[0] - 2.0);
    let place = [corner[0] + (BOX - font.width(&shown)) / 2.0, corner[1] + BOX + 2.0].map(f32::floor);
    layer.texts.push((shown, place, if level > 0 { [0, 0, 0] } else { GREY }));
}

impl SkillWindow {
    pub fn height(&self) -> f32 {
        HEIGHT
    }

    /// The level to use a skill at: the one picked, kept within what's learned, else the learned level.
    pub fn use_level(&self, id: u16, learned: u16) -> u16 {
        self.chosen.get(&id).map_or(learned, |&level| level.clamp(1, learned.max(1)))
    }

    /// The tab shown: the player's own job's (the line's last, before Etc) unless another was picked.
    fn tab(&self, jobs: &[u16]) -> usize {
        let own = jobs.iter().rposition(|&job| job != ETC).unwrap_or(0);
        if self.tab == usize::MAX { own } else { self.tab.min(jobs.len().saturating_sub(1)) }
    }

    /// The skills in the open tab, by place; the `ETC` tab's in ID order.
    fn tree(&self, trees: &SkillTrees, jobs: &[u16], skills: &[Skill]) -> Vec<(usize, u16)> {
        match jobs.get(self.tab(jobs)) {
            Some(&ETC) => {
                let line: Vec<u16> = jobs.iter().copied().filter(|&job| job != ETC).collect();
                extra_skills(skills, trees, &line).into_iter().enumerate().collect()
            }
            Some(job) => trees.trees.get(job).cloned().unwrap_or_default(),
            None => Vec::new(),
        }
    }

    pub fn hit(&self, cursor: [f32; 2], trees: &SkillTrees, jobs: &[u16], skills: &[Skill], points: i64) -> Option<Hit> {
        let local = self.frame.local(cursor, HEIGHT)?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(Hit::Title(hit));
        }
        if local[1] < TITLE_HEIGHT + TAB_HEIGHT {
            return Some((0..jobs.len()).find(|&tab| contains(tab_corner(tab), [TAB_WIDTH, TAB_HEIGHT], local)).map_or(Hit::Window, Hit::Tab));
        }
        for (place, id) in self.tree(trees, jobs, skills) {
            let corner = box_corner(place);
            if upgradable(skills, id, points) && contains(offset(corner, UP), LEVEL_UP, local) {
                return Some(Hit::LevelUp(id));
            }
            if let Some(skill) = skills.iter().find(|skill| skill.id == id).filter(|skill| steppable(skill))
                && let Some(arrow) = ARROWS.iter().position(|&at| contains(offset(corner, at), ARROW, local))
            {
                return Some(Hit::Step { id, up: arrow == 0, learned: skill.level });
            }
            if contains(corner, [BOX, BOX], local) {
                return Some(Hit::Skill(id));
            }
        }
        Some(Hit::Window)
    }

    pub fn click(&mut self, hit: Hit) -> Option<Action> {
        match hit {
            Hit::Title(hit) => self.frame.click_title(hit),
            Hit::Tab(tab) => self.tab = tab,
            Hit::Skill(id) => return self.clicks.double(id).then_some(Action::Use(id)),
            Hit::LevelUp(id) => return Some(Action::LevelUp(id)),
            Hit::Step { id, up, learned } => {
                let level = self.use_level(id, learned);
                let level = if up { level + 1 } else { level.saturating_sub(1) };
                self.chosen.insert(id, level.clamp(1, learned));
            }
            Hit::Window => {}
        }
        None
    }

    /// The open tab's skill boxes on screen: skill and box.
    pub fn skill_boxes(&self, trees: &SkillTrees, jobs: &[u16], skills: &[Skill]) -> Vec<(u16, [f32; 4])> {
        if !self.frame.open {
            return Vec::new();
        }
        self.tree(trees, jobs, skills)
            .into_iter()
            .map(|(place, id)| {
                let corner = self.frame.at(box_corner(place));
                (id, [corner[0], corner[1], BOX, BOX])
            })
            .collect()
    }

    /// The open tab's level up buttons on screen (for the test driver): skill and box.
    pub fn raise_boxes(&self, trees: &SkillTrees, jobs: &[u16], skills: &[Skill], points: i64) -> Vec<(u16, [f32; 4])> {
        if !self.frame.open || self.frame.minimized {
            return Vec::new();
        }
        self.tree(trees, jobs, skills)
            .into_iter()
            .filter(|&(_, id)| upgradable(skills, id, points))
            .map(|(place, id)| {
                let [x, y] = self.frame.at(offset(box_corner(place), UP));
                (id, [x, y, LEVEL_UP[0], LEVEL_UP[1]])
            })
            .collect()
    }

    /// The job tabs on screen (for the test driver): job and box.
    pub fn tab_boxes(&self, jobs: &[u16]) -> Vec<(u16, [f32; 4])> {
        if !self.frame.open || self.frame.minimized {
            return Vec::new();
        }
        jobs.iter()
            .enumerate()
            .map(|(tab, &job)| {
                let [x, y] = self.frame.at(tab_corner(tab));
                (job, [x, y, TAB_WIDTH, TAB_HEIGHT])
            })
            .collect()
    }

    /// The icons the open tab shows, to load before drawing.
    pub fn icons(&self, trees: &SkillTrees, jobs: &[u16], skills: &[Skill], info: &HashMap<u16, SkillInfo>) -> Vec<String> {
        if !self.frame.open {
            return Vec::new();
        }
        self.tree(trees, jobs, skills).iter().filter_map(|(_, id)| info.get(id).map(icon_path)).collect()
    }

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
        (trees, jobs, info, job_names): (&SkillTrees, &[u16], &HashMap<u16, SkillInfo>, &HashMap<u16, String>),
        skills: &[Skill],
        points: i64,
        pointer: ([f32; 2], bool),
        cooling: &Cooling<'a>,
    ) {
        if !self.frame.open {
            return;
        }
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        let at = |local: [f32; 2]| self.frame.at(local);
        self.frame.draw_title(layer, chrome, message(TITLE_MESSAGE), pointer.0);
        if self.frame.minimized {
            return;
        }
        let hit = self.hit(pointer.0, trees, jobs, skills, points);
        layer.sprites.push((&chrome.white, at([0.0, TITLE_HEIGHT]), [WIDTH, HEIGHT - TITLE_HEIGHT - BAR_HEIGHT]));

        // A tab for each job of the line, the open one joined to the page.
        let open = self.tab(jobs);
        for (tab, job) in jobs.iter().enumerate() {
            let corner = at([SIDE + tab as f32 * TAB_WIDTH, TITLE_HEIGHT + 2.0]);
            let size = [TAB_WIDTH - 2.0, TAB_HEIGHT - 2.0];
            layer.sprites.push((&chrome.border, corner, [size[0], 1.0]));
            layer.sprites.push((&chrome.border, corner, [1.0, size[1]]));
            layer.sprites.push((&chrome.border, offset(corner, [size[0] - 1.0, 0.0]), [1.0, size[1]]));
            let name = match *job {
                ETC => message(ETC_LABEL).to_owned(),
                job => job_names.get(&job).map_or_else(|| job.to_string(), Clone::clone),
            };
            let place = offset(corner, [((size[0] - font.width(&name)) / 2.0).floor(), 3.0]);
            layer.texts.push((name, place, if tab == open { [0, 0, 0] } else { GREY }));
        }
        layer.sprites.push((&chrome.border, at([0.0, TITLE_HEIGHT + TAB_HEIGHT]), [WIDTH, 1.0]));

        for (place, id) in self.tree(trees, jobs, skills) {
            let corner = box_corner(place);
            let known = skills.iter().find(|skill| skill.id == id);
            let level = known.map_or(0, |skill| skill.level);
            let skill_info = info.get(&id);
            let name = skill_info.map_or_else(|| id.to_string(), |info| info.name.clone());
            // A learned skill's box shows the level it's used at.
            let shown = if level > 0 { self.use_level(id, level) } else { 0 };
            draw_box(layer, assets, icons, font, at(corner), skill_info, &name, shown);
            let cooldown = if level > 0 { cooling.draw(layer, id, at(offset(corner, [2.0, 2.0])), [24.0, 24.0]) } else { None };
            if known.is_some_and(steppable) {
                for (arrow, at) in assets.arrows.iter().zip(ARROWS) {
                    layer.sprites.push((arrow, self.frame.at(offset(corner, at)), arrow.size));
                }
            }
            if points > 0 && known.is_some_and(|skill| skill.upgradable) {
                let state = match hit {
                    Some(Hit::LevelUp(target)) if target == id => 1 + usize::from(pointer.1),
                    _ => 0,
                };
                layer.sprites.push((&assets.up[state], at(offset(corner, UP)), assets.up[state].size));
            }
            if hit == Some(Hit::Skill(id)) {
                let max = skill_info.map_or(0, |info| info.max_level);
                window::tooltip(tooltip, chrome, font, &format!("{name} Lv {level}/{max}"), offset(pointer.0, [12.0, 12.0]), [255; 3]);
                if let Some(cooldown) = &cooldown {
                    window::tooltip(tooltip, chrome, font, cooldown, offset(pointer.0, [12.0, 35.0]), [255; 3]);
                }
            }
        }

        // The points left, on the bottom bar.
        let bar_top = HEIGHT - BAR_HEIGHT;
        let [left, middle, right] = &assets.bar;
        layer.sprites.push((left, at([0.0, bar_top]), left.size));
        layer.sprites.push((middle, at([left.size[0], bar_top]), [WIDTH - left.size[0] - right.size[0], BAR_HEIGHT]));
        layer.sprites.push((right, at([WIDTH - right.size[0], bar_top]), right.size));
        let text = messages::fill(message(POINTS_MESSAGE), &[&points]);
        layer.texts.push((text, at([12.0, bar_top + ((BAR_HEIGHT - font.line_height()) / 2.0).floor()]), [0, 0, 0]));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_skills_no_tab_of_the_line_holds() {
        // A Swordman (1) from a Novice (0): Basic Skill 1 and Bash 5 sit in their trees.
        let trees = SkillTrees { trees: HashMap::from([(0, vec![(0, 1)]), (1, vec![(0, 5), (1, 6)])]), parents: HashMap::from([(1, 0)]) };
        let skill = |id| Skill { id, kind: Skill::SELF, level: 1, sp: 0, range: 0, upgradable: false };
        let plain = [skill(1), skill(5)];
        assert!(extra_skills(&plain, &trees, &[0, 1]).is_empty());
        assert_eq!(tabs(&trees, 1, &plain), [0, 1], "no extra skills, no tab");
        // A Wedding Ring's skills (I Miss You 336, I Will Protect You 334) go in, by ID; the tree's stay out.
        let married = [skill(336), skill(1), skill(334), skill(5)];
        assert_eq!(extra_skills(&married, &trees, &[0, 1]), [334, 336]);
        assert_eq!(tabs(&trees, 1, &married), [0, 1, ETC]);
        // Unpicked, the window opens on the job's own tab, not Etc (#245).
        let mut window = SkillWindow::default();
        assert_eq!(window.tab(&[0, 1, ETC]), 1);
        assert_eq!(window.tab(&[0, 1]), 1);
        // The tab lays them out in ID order.
        window.frame.open = true;
        window.tab = 2;
        let jobs = tabs(&trees, 1, &married);
        assert_eq!(window.skill_boxes(&trees, &jobs, &married).into_iter().map(|(id, _)| id).collect::<Vec<_>>(), [334, 336]);
    }

    #[test]
    fn counts_cooldowns_down() {
        let start = Instant::now();
        let at = |ms: u64| start + Duration::from_millis(ms);
        let mut cooldowns = Cooldowns::default();
        cooldowns.set(8, 10_000, start);
        assert_eq!(cooldowns.left(8, at(2_500)), Some(Duration::from_millis(7_500)));
        assert_eq!(cooldowns.share(8, at(2_500)), Some(0.25));
        assert_eq!(cooldowns.blocked(at(2_500)).collect::<Vec<_>>(), [(8, Duration::from_millis(7_500))]);
        // Past its end it's gone, as is one lifted with 0; one never set isn't blocked.
        assert_eq!(cooldowns.left(8, at(10_000)), None);
        assert_eq!(cooldowns.share(8, at(12_000)), None);
        cooldowns.set(8, 10_000, start);
        cooldowns.set(8, 0, at(1_000));
        assert_eq!(cooldowns.left(8, at(1_000)), None);
        assert_eq!(cooldowns.left(7, start), None);
    }

    #[test]
    fn places_skills_seven_a_row() {
        assert_eq!(box_corner(0), [SIDE + 17.0, GRID_TOP + 6.0]);
        assert_eq!(box_corner(8), [SIDE + 62.0 + 17.0, GRID_TOP + 58.0 + 6.0]);
    }

    #[test]
    fn picks_the_level_to_use_within_what_is_learned() {
        let mut window = SkillWindow::default();
        assert_eq!(window.use_level(28, 10), 10);
        for _ in 0..3 {
            window.click(Hit::Step { id: 28, up: false, learned: 10 });
        }
        assert_eq!(window.use_level(28, 10), 7);
        // Not past the learned level, nor under 1.
        for _ in 0..5 {
            window.click(Hit::Step { id: 28, up: true, learned: 10 });
        }
        assert_eq!(window.use_level(28, 10), 10);
        for _ in 0..12 {
            window.click(Hit::Step { id: 28, up: false, learned: 10 });
        }
        assert_eq!(window.use_level(28, 10), 1);
        // A pick above a level later lost comes down to it; other skills keep their learned level.
        window.click(Hit::Step { id: 34, up: false, learned: 5 });
        assert_eq!(window.use_level(34, 2), 2);
        assert_eq!(window.use_level(29, 3), 3);
        let passive = Skill { id: 1, kind: 0, level: 9, sp: 0, range: 0, upgradable: false };
        let heal = Skill { id: 28, kind: Skill::FRIEND, level: 10, sp: 13, range: 9, upgradable: false };
        assert!(!steppable(&passive) && steppable(&heal));
        assert!(!steppable(&Skill { level: 1, ..heal }));
    }

    #[test]
    fn names_the_level_up_buttons_a_click_takes() {
        // Novice (0) then Mage (2): Basic Skill in the first tab; Fire Bolt and Cold Bolt in the second.
        let trees = SkillTrees { trees: HashMap::from([(0, vec![(0, 1)]), (2, vec![(0, 19), (2, 14)])]), parents: HashMap::new() };
        let jobs = [0, 2];
        let skill = |id: u16, level: u16, upgradable: bool| Skill { id, kind: Skill::ENEMY, level, sp: 12, range: 9, upgradable };
        let skills = [skill(1, 9, false), skill(19, 0, true), skill(14, 10, false)];
        let mut window = SkillWindow::default();
        assert!(window.raise_boxes(&trees, &jobs, &skills, 5).is_empty() && window.tab_boxes(&jobs).is_empty());
        window.frame.open = true;
        let center = |area: [f32; 4]| [area[0] + area[2] / 2.0, area[1] + area[3] / 2.0];
        // The Mage tab opens by default; only Fire Bolt can still rise, and its box is the one the click test takes.
        let boxes = window.raise_boxes(&trees, &jobs, &skills, 5);
        assert_eq!(boxes.iter().map(|(id, _)| *id).collect::<Vec<_>>(), [19]);
        assert_eq!(window.hit(center(boxes[0].1), &trees, &jobs, &skills, 5), Some(Hit::LevelUp(19)));
        assert!(window.raise_boxes(&trees, &jobs, &skills, 0).is_empty());
        // The tabs, by job: the Novice tab's box switches to it.
        let tabs = window.tab_boxes(&jobs);
        assert_eq!(tabs.iter().map(|(job, _)| *job).collect::<Vec<_>>(), [0, 2]);
        assert_eq!(window.hit(center(tabs[0].1), &trees, &jobs, &skills, 5), Some(Hit::Tab(0)));
        window.click(Hit::Tab(0));
        assert!(window.raise_boxes(&trees, &jobs, &skills, 5).is_empty());
        window.frame.minimized = true;
        assert!(window.tab_boxes(&jobs).is_empty());
    }
}
