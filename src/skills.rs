//! The skill window (Alt+S): the job's skills where the client's skill tree puts them, a tab down the left for each
//! tier of the line (Novice with the first job, the second job with its transcendent one, third, fourth) and Etc,
//! their levels, and the points left to put in. Points go in pending first and are sent on Apply.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use rustnarok_client::grf::Grf;
use rustnarok_client::lua::{SkillInfo, SkillTrees};
use rustnarok_client::net::Skill;
use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};
use crate::window::{self, Chrome, Clicks, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI, contains, offset};

/// The box grid other skill lists (guild, homunculus, mercenary) lay their skills out on.
pub const CELL: [f32; 2] = [62.0, 58.0];
pub const BOX: f32 = 28.0;
/// Where a level up button sits by a box in those lists.
pub const UP: [f32; 2] = [BOX + 1.0, 2.0];
const GREY: [u8; 3] = [0x80, 0x80, 0x80];

const WIDTH: f32 = 560.0;
const HEIGHT: f32 = 400.0;
/// First opened at ((W − 640)/2 + 50, H·50/480) on the 1024×768 screen.
const DEFAULT_POSITION: [f32; 2] = [242.0, 80.0];
const COLUMNS: usize = 7;
const ROWS: usize = 6;
/// A skill's icon, 24×24 in a white 26×26 box, at column c, row r: (76 + 68c, 44 + 56r).
const ICON_AT: [f32; 2] = [76.0, 44.0];
const STEP: [f32; 2] = [68.0, 56.0];
const ICON: f32 = 24.0;
/// The name centred above the icon at y 30 + 56r (fitted to 48 px); the level centred below at y 70 + 56r.
const NAME_Y: f32 = 30.0;
const LEVEL_Y: f32 = 70.0;
const NAME_WIDTH: f32 = 48.0;
const LEARNED_NAME: [u8; 3] = [102, 102, 102];
const UNLEARNED_NAME: [u8; 3] = [120, 120, 120];
/// Levels put in but not sent: "+N" in blue at (102 + 68c, 44 + 56r).
const PENDING_X: f32 = 102.0;
const BLUE: [u8; 3] = [0, 0, 255];
const RED: [u8; 3] = [255, 0, 0];
// ponytail: the level up button under the pending count, right of the icon; place it against the real client.
/// The level up button by an icon, and how much around it a click takes.
const RAISE_AT: [f32; 2] = [PENDING_X - ICON_AT[0], 12.0];
const RAISE: [f32; 2] = [24.0, 24.0];
// ponytail: the level arrows at the icon's left, up over down.
const ARROWS: [[f32; 2]; 2] = [[-12.0, 0.0], [-12.0, 12.0]];
const ARROW: [f32; 2] = [11.0, 11.0];
/// Tabs down the left, 17 px thick.
// ponytail: 80 px a tab from under the title, the name a letter a line.
const TAB: [f32; 2] = [17.0, 80.0];
const TAB_LEFT: f32 = 4.0;
/// Apply and Reset (42×20) at (w − 100, h − 27) and (w − 50, h − 27); the points at (80, h − 23).
const BUTTON: [f32; 2] = [42.0, 20.0];
const APPLY_AT: [f32; 2] = [WIDTH - 100.0, HEIGHT - 27.0];
const RESET_AT: [f32; 2] = [WIDTH - 50.0, HEIGHT - 27.0];
const POINTS_AT: [f32; 2] = [80.0, HEIGHT - 23.0];
const BAR_HEIGHT: f32 = 32.0;
/// Hovering a skill: its prerequisites' boxes, and the box of one that can be raised.
const NEEDED_FILL: [u8; 3] = [204, 192, 255];
const NEEDED_LINE: [u8; 3] = [193, 182, 255];
const RAISABLE_FILL: [u8; 3] = [240, 196, 175];
/// The wheel scrolls this many rows a notch.
const WHEEL_ROWS: usize = 3;
const TITLE_MESSAGE: usize = 283;
/// "Skill Points : ", then the points.
const POINTS_MESSAGE: usize = 2343;
/// "Etc." for skills outside the trees.
const ETC_LABEL: usize = 1093;
// ponytail: the data's tab names (JobSkillTab_GetTabName) are Korean only; these stand in.
const TIER_NAMES: [&str; 4] = ["1st", "2nd", "3rd", "4th"];
/// "The skill (%s) can't be learned": a prerequisite the points can't raise.
pub const CANNOT_LEARN: usize = 3531;
/// "Skill points used can't be taken back. Apply?"
pub const APPLY_QUESTION: usize = 1377;

pub struct Assets {
    /// The dashed box other lists put a skill in.
    slot: Texture,
    /// The level up button: normal, hovered, pressed.
    pub up: [Texture; 3],
    /// The bottom bar's left end, middle and right end.
    bar: [Texture; 3],
    /// Whitens skills not learned.
    veil: Texture,
    /// The arrows that pick the level to use: up, down.
    arrows: [Texture; 2],
    /// Apply and Reset: normal, hovered, pressed.
    apply: [Texture; 3],
    reset: [Texture; 3],
    /// Box fills: plain, a prerequisite's, a raisable skill's; and a prerequisite's outline.
    fills: [Texture; 3],
    needed_line: Texture,
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |name: &str| gpu.load_texture(grf, &format!("{UI}basic_interface\\{name}.bmp"), false);
        let root = |name: &str| gpu.load_texture(grf, &format!("{UI}{name}.bmp"), false);
        let solid = |[r, g, b]: [u8; 3]| gpu.solid([r, g, b, 255]);
        Self {
            slot: image("no_skill"),
            up: ["skill_up_a", "skill_up_b", "skill_up_c"].map(image),
            bar: ["skill_botbar_left", "skill_botbar_mid", "skill_botbar_right"].map(image),
            veil: gpu.solid([255, 255, 255, 150]),
            arrows: ["arw_up", "arw_down"].map(image),
            apply: ["btn_apply", "btn_apply_a", "btn_apply_b"].map(root),
            reset: ["btn_reset", "btn_reset_a", "btn_reset_b"].map(root),
            fills: [solid([255; 3]), solid(NEEDED_FILL), solid(RAISABLE_FILL)],
            needed_line: solid(NEEDED_LINE),
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
    Apply,
    Reset,
    Window,
}

/// What a click on the window asks for.
#[derive(Debug, PartialEq)]
pub enum Action {
    /// Use the skill at its level (a double click).
    Use(u16),
    /// Put a level into the skill, with what it needs first.
    LevelUp(u16),
    /// Send the pending levels (after asking).
    Apply,
}

pub struct SkillWindow {
    pub frame: Frame,
    tab: usize,
    /// The first row shown.
    scroll: usize,
    clicks: Clicks,
    /// The level picked to use, by skill; the learned level when none is.
    chosen: HashMap<u16, u16>,
    /// Levels put in but not sent, in order (a skill once a level, what it needs before it).
    pending: Vec<u16>,
}

impl Default for SkillWindow {
    fn default() -> Self {
        Self { frame: Frame::new(DEFAULT_POSITION, WIDTH), tab: usize::MAX, scroll: 0, clicks: Clicks::default(), chosen: HashMap::new(), pending: Vec::new() }
    }
}

/// Whether a skill gets level arrows: learned past level 1, and used rather than passive.
fn steppable(skill: &Skill) -> bool {
    skill.level > 1 && skill.kind != 0
}

/// The tab for skills no tree of the line holds (a Wedding Ring's, an adopted family's, cards'), after the line's.
pub const ETC: u16 = u16::MAX;

/// The player's skills that no tree of the line holds, in ID order.
pub fn extra_skills(skills: &[Skill], trees: &SkillTrees, jobs: &[u16]) -> Vec<u16> {
    let held = |id: u16| jobs.iter().filter_map(|job| trees.trees.get(job)).any(|tree| tree.iter().any(|&(_, each)| each == id));
    let mut extra: Vec<u16> = skills.iter().map(|skill| skill.id).filter(|&id| !held(id)).collect();
    extra.sort_unstable();
    extra.dedup();
    extra
}

/// The job's line, Novice first, then `ETC` when the player has skills none of it holds.
pub fn tabs(trees: &SkillTrees, job: u16, skills: &[Skill]) -> Vec<u16> {
    let mut jobs = trees.lineage(job);
    if !extra_skills(skills, trees, &jobs).is_empty() {
        jobs.push(ETC);
    }
    jobs
}

/// The window's tabs from the line: Novice with the first job, then each job after, a transcendent second job
/// (4008–4022) with its second job; `ETC` on its own. Each tab is the jobs whose trees it shows together.
pub fn tiers(jobs: &[u16]) -> Vec<Vec<u16>> {
    let mut tiers: Vec<Vec<u16>> = Vec::new();
    for (index, &job) in jobs.iter().enumerate() {
        let joins = job != ETC && (index == 1 && jobs[0] == 0 || (4008..=4022).contains(&job));
        match tiers.last_mut() {
            Some(last) if joins => last.push(job),
            _ => tiers.push(vec![job]),
        }
    }
    tiers
}

/// Plans one more level of `id`: first every prerequisite it's still missing, up to the level it needs (and theirs
/// before them), then the skill. `level` is what each skill has (learned and pending), `max` how far each goes (0:
/// not one to learn here), `needs` what each needs. The levels to add in order, or the skill that can't get there
/// (past its maximum, not learnable, or past the `points`).
pub fn plan(id: u16, level: impl Fn(u16) -> u16, max: impl Fn(u16) -> u16, needs: impl Fn(u16) -> Vec<(u16, u16)>, points: i64) -> Result<Vec<u16>, u16> {
    fn ensure(
        id: u16,
        target: u16,
        levels: &mut HashMap<u16, u16>,
        level: &impl Fn(u16) -> u16,
        max: &impl Fn(u16) -> u16,
        needs: &impl Fn(u16) -> Vec<(u16, u16)>,
        out: &mut Vec<u16>,
        depth: usize,
    ) -> Result<(), u16> {
        let have = *levels.entry(id).or_insert_with(|| level(id));
        if have >= target {
            return Ok(());
        }
        if target > max(id) || depth > 16 {
            return Err(id);
        }
        for (need, at) in needs(id) {
            ensure(need, at, levels, level, max, needs, out, depth + 1)?;
        }
        out.extend(std::iter::repeat_n(id, usize::from(target - have)));
        levels.insert(id, target);
        Ok(())
    }
    let mut out = Vec::new();
    ensure(id, level(id) + 1, &mut HashMap::new(), &level, &max, &needs, &mut out, 0)?;
    match usize::try_from(points.max(0)).ok().and_then(|points| out.get(points)) {
        Some(&short) => Err(short),
        None => Ok(out),
    }
}

/// Where a place's icon sits in the window, `scroll` rows down.
fn icon_corner(place: usize, scroll: usize) -> Option<[f32; 2]> {
    let [column, row] = [place % COLUMNS, place / COLUMNS];
    let row = row.checked_sub(scroll).filter(|&row| row < ROWS)?;
    Some([ICON_AT[0] + column as f32 * STEP[0], ICON_AT[1] + row as f32 * STEP[1]])
}

/// Where a tab sits down the left.
fn tab_corner(tab: usize) -> [f32; 2] {
    [TAB_LEFT, TITLE_HEIGHT + 4.0 + tab as f32 * (TAB[1] + 2.0)]
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

/// What the hotkey bar and the skill window need to show cooldowns: the blocks, the fan's pictures by step, the time.
pub struct Cooling<'a> {
    pub cooldowns: &'a Cooldowns,
    pub fan: &'a [Texture],
    pub now: Instant,
}

impl<'a> Cooling<'a> {
    /// Draws the fan over a blocked skill's icon: dark where its time is still left, clearing clockwise from 12.
    pub fn draw(&self, layer: &mut Layer<'a>, skill: u16, corner: [f32; 2], size: [f32; 2]) {
        let (Some(passed), Some(last)) = (self.cooldowns.share(skill, self.now), self.fan.len().checked_sub(1)) else { return };
        layer.sprites.push((&self.fan[((passed * last as f32) as usize).min(last)], corner, size));
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

/// What the window shows from: the line's trees and jobs, the skills as the server has them, the client's skill
/// table, and the points left on the server.
#[derive(Clone, Copy)]
pub struct Sheet<'a> {
    pub trees: &'a SkillTrees,
    pub jobs: &'a [u16],
    pub skills: &'a [Skill],
    pub info: &'a HashMap<u16, SkillInfo>,
    pub points: i64,
}

impl Sheet<'_> {
    fn learned(&self, id: u16) -> u16 {
        self.skills.iter().find(|skill| skill.id == id).map_or(0, |skill| skill.level)
    }

    /// The job the player is (the line's last), whose prerequisite lists count.
    fn job(&self) -> u16 {
        self.jobs.iter().rev().copied().find(|&job| job != ETC).unwrap_or(0)
    }

    /// Whether a skill sits in a tree of the line: one to learn here.
    fn in_line(&self, id: u16) -> bool {
        self.jobs.iter().filter(|&&job| job != ETC).filter_map(|job| self.trees.trees.get(job)).any(|tree| tree.iter().any(|&(_, each)| each == id))
    }

    fn max(&self, id: u16) -> u16 {
        if self.in_line(id) { self.info.get(&id).map_or(0, |info| info.max_level) } else { 0 }
    }

    fn needs(&self, id: u16) -> Vec<(u16, u16)> {
        self.info.get(&id).map(|info| info.needs_for(self.job()).to_vec()).unwrap_or_default()
    }
}

impl SkillWindow {
    pub fn height(&self) -> f32 {
        HEIGHT
    }

    /// The level to use a skill at: the one picked, kept within what's learned, else the learned level.
    pub fn use_level(&self, id: u16, learned: u16) -> u16 {
        self.chosen.get(&id).map_or(learned, |&level| level.clamp(1, learned.max(1)))
    }

    /// Levels of a skill put in but not sent.
    pub fn pending_levels(&self, id: u16) -> u16 {
        self.pending.iter().filter(|&&each| each == id).count() as u16
    }

    /// The pending levels, in the order they go out.
    pub fn pending(&self) -> &[u16] {
        &self.pending
    }

    /// Takes the pending levels to send.
    pub fn take_pending(&mut self) -> Vec<u16> {
        std::mem::take(&mut self.pending)
    }

    /// Points left once the pending levels are counted.
    fn points_left(&self, sheet: &Sheet) -> i64 {
        sheet.points - self.pending.len() as i64
    }

    /// A skill's level counting the pending ones.
    fn level(&self, sheet: &Sheet, id: u16) -> u16 {
        sheet.learned(id) + self.pending_levels(id)
    }

    /// Whether the level up button shows by a skill: points left and the skill below its maximum.
    fn raisable(&self, sheet: &Sheet, id: u16) -> bool {
        self.points_left(sheet) > 0 && self.level(sheet, id) < sheet.max(id)
    }

    /// Puts one more level into `id`, and every level of a prerequisite it still lacks, as far as the points go.
    /// Err: the skill the points can't raise (nothing is added).
    pub fn raise(&mut self, sheet: &Sheet, id: u16) -> Result<(), u16> {
        let levels = plan(id, |skill| self.level(sheet, skill), |skill| sheet.max(skill), |skill| sheet.needs(skill), self.points_left(sheet))?;
        self.pending.extend(levels);
        Ok(())
    }

    /// The tabs: the line's tiers that have skills, then Etc.
    fn tab_jobs(&self, sheet: &Sheet) -> Vec<Vec<u16>> {
        tiers(sheet.jobs)
            .into_iter()
            .filter(|tier| tier == &[ETC] || tier.iter().any(|job| sheet.trees.trees.get(job).is_some_and(|tree| !tree.is_empty())))
            .collect()
    }

    /// The tab shown: the highest tier (the last before Etc) unless another was picked.
    fn tab(&self, tabs: &[Vec<u16>]) -> usize {
        let own = tabs.iter().rposition(|tier| tier != &[ETC]).unwrap_or(0);
        if self.tab == usize::MAX { own } else { self.tab.min(tabs.len().saturating_sub(1)) }
    }

    /// Tab: the next tab, round to the first.
    pub fn next_tab(&mut self, sheet: &Sheet) {
        let tabs = self.tab_jobs(sheet);
        self.tab = (self.tab(&tabs) + 1) % tabs.len().max(1);
        self.scroll = 0;
    }

    /// The skills in the open tab, by place; the `ETC` tab's in ID order.
    fn tree(&self, sheet: &Sheet) -> Vec<(usize, u16)> {
        let tabs = self.tab_jobs(sheet);
        match tabs.get(self.tab(&tabs)).map(Vec::as_slice) {
            Some([ETC]) => {
                let line: Vec<u16> = sheet.jobs.iter().copied().filter(|&job| job != ETC).collect();
                extra_skills(sheet.skills, sheet.trees, &line).into_iter().enumerate().collect()
            }
            Some(jobs) => {
                let mut tree: Vec<(usize, u16)> = jobs.iter().filter_map(|job| sheet.trees.trees.get(job)).flatten().copied().collect();
                tree.sort_unstable();
                tree
            }
            None => Vec::new(),
        }
    }

    /// The open tab's skills that show, with their icons' corners in the window.
    fn shown(&self, sheet: &Sheet) -> Vec<(u16, [f32; 2])> {
        self.tree(sheet).into_iter().filter_map(|(place, id)| Some((id, icon_corner(place, self.scroll)?))).collect()
    }

    /// The wheel over the window: whole rows, three a notch (`rows` > 0 down).
    pub fn scroll_by(&mut self, sheet: &Sheet, rows: i32) {
        let last = self.tree(sheet).iter().map(|(place, _)| place / COLUMNS).max().unwrap_or(0);
        let most = (last + 1).saturating_sub(ROWS);
        let step = WHEEL_ROWS as i64 * i64::from(rows.signum());
        self.scroll = (self.scroll as i64 + step).clamp(0, most as i64) as usize;
    }

    pub fn hit(&self, cursor: [f32; 2], sheet: &Sheet) -> Option<Hit> {
        let local = self.frame.local(cursor, HEIGHT)?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(Hit::Title(hit));
        }
        let tabs = self.tab_jobs(sheet);
        if let Some(tab) = (0..tabs.len()).find(|&tab| contains(tab_corner(tab), TAB, local)) {
            return Some(Hit::Tab(tab));
        }
        if !self.pending.is_empty() {
            if contains(APPLY_AT, BUTTON, local) {
                return Some(Hit::Apply);
            }
            if contains(RESET_AT, BUTTON, local) {
                return Some(Hit::Reset);
            }
        }
        for (id, corner) in self.shown(sheet) {
            if self.raisable(sheet, id) && contains(offset(corner, RAISE_AT), RAISE, local) {
                return Some(Hit::LevelUp(id));
            }
            if self.pending.is_empty()
                && let Some(skill) = sheet.skills.iter().find(|skill| skill.id == id).filter(|skill| steppable(skill))
                && let Some(arrow) = ARROWS.iter().position(|&at| contains(offset(corner, at), ARROW, local))
            {
                return Some(Hit::Step { id, up: arrow == 0, learned: skill.level });
            }
            if contains(offset(corner, [-1.0, -1.0]), [ICON + 2.0, ICON + 2.0], local) {
                return Some(Hit::Skill(id));
            }
        }
        Some(Hit::Window)
    }

    pub fn click(&mut self, hit: Hit) -> Option<Action> {
        match hit {
            Hit::Title(hit) => self.frame.click_title(hit),
            Hit::Tab(tab) => {
                self.tab = tab;
                self.scroll = 0;
            }
            Hit::Skill(id) => return self.clicks.double(id).then_some(Action::Use(id)),
            Hit::LevelUp(id) => return Some(Action::LevelUp(id)),
            Hit::Step { id, up, learned } => {
                let level = self.use_level(id, learned);
                let level = if up { level + 1 } else { level.saturating_sub(1) };
                self.chosen.insert(id, level.clamp(1, learned));
            }
            Hit::Apply => return Some(Action::Apply),
            Hit::Reset => self.pending.clear(),
            Hit::Window => {}
        }
        None
    }

    /// Whether a skill can be dragged out: not while levels are pending.
    pub fn draggable(&self) -> bool {
        self.pending.is_empty()
    }

    /// The open tab's skill boxes on screen: skill and box.
    pub fn skill_boxes(&self, sheet: &Sheet) -> Vec<(u16, [f32; 4])> {
        if !self.frame.open || self.frame.minimized {
            return Vec::new();
        }
        self.shown(sheet)
            .into_iter()
            .map(|(id, corner)| {
                let [x, y] = self.frame.at(corner);
                (id, [x, y, ICON, ICON])
            })
            .collect()
    }

    /// The open tab's level up buttons on screen (for the test driver): skill and box.
    pub fn raise_boxes(&self, sheet: &Sheet) -> Vec<(u16, [f32; 4])> {
        if !self.frame.open || self.frame.minimized {
            return Vec::new();
        }
        self.shown(sheet)
            .into_iter()
            .filter(|&(id, _)| self.raisable(sheet, id))
            .map(|(id, corner)| {
                let [x, y] = self.frame.at(offset(corner, RAISE_AT));
                (id, [x, y, RAISE[0], RAISE[1]])
            })
            .collect()
    }

    /// The tabs on screen (for the test driver): each job a tab shows (`ETC` for Etc) and the tab's box.
    pub fn tab_boxes(&self, sheet: &Sheet) -> Vec<(u16, [f32; 4])> {
        if !self.frame.open || self.frame.minimized {
            return Vec::new();
        }
        self.tab_jobs(sheet)
            .into_iter()
            .enumerate()
            .flat_map(|(tab, jobs)| {
                let [x, y] = self.frame.at(tab_corner(tab));
                jobs.into_iter().map(move |job| (job, [x, y, TAB[0], TAB[1]]))
            })
            .collect()
    }

    /// Apply and Reset on screen (for the test driver), while levels are pending.
    pub fn button_boxes(&self) -> Vec<(&'static str, [f32; 4])> {
        if !self.frame.open || self.frame.minimized || self.pending.is_empty() {
            return Vec::new();
        }
        [("skill_apply", APPLY_AT), ("skill_reset", RESET_AT)]
            .map(|(name, at)| {
                (name, {
                    let [x, y] = self.frame.at(at);
                    [x, y, BUTTON[0], BUTTON[1]]
                })
            })
            .to_vec()
    }

    /// The icons the open tab shows, to load before drawing.
    pub fn icons(&self, sheet: &Sheet) -> Vec<String> {
        if !self.frame.open {
            return Vec::new();
        }
        self.tree(sheet).iter().filter_map(|(_, id)| sheet.info.get(id).map(icon_path)).collect()
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
        sheet: &Sheet,
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
        let hit = self.hit(pointer.0, sheet);
        layer.sprites.push((&chrome.white, at([0.0, TITLE_HEIGHT]), [WIDTH, HEIGHT - TITLE_HEIGHT - BAR_HEIGHT]));

        // A tab for each tier down the left, its name a letter a line; the open one in black.
        let tabs = self.tab_jobs(sheet);
        let open = self.tab(&tabs);
        let mut tier = 0;
        for (tab, jobs) in tabs.iter().enumerate() {
            let corner = at(tab_corner(tab));
            for (place, size) in [([0.0, 0.0], [TAB[0], 1.0]), ([0.0, 0.0], [1.0, TAB[1]]), ([0.0, TAB[1] - 1.0], [TAB[0], 1.0])] {
                layer.sprites.push((&chrome.border, offset(corner, place), size));
            }
            let name = if jobs == &[ETC] {
                message(ETC_LABEL).to_owned()
            } else {
                tier += 1;
                TIER_NAMES.get(tier - 1).map_or_else(|| tier.to_string(), |name| (*name).to_owned())
            };
            let color = if tab == open { [0, 0, 0] } else { GREY };
            for (row, letter) in name.chars().enumerate() {
                let letter = letter.to_string();
                let place = offset(corner, [((TAB[0] - font.width(&letter)) / 2.0).floor(), 4.0 + row as f32 * font.line_height()]);
                layer.texts.push((letter, place, color));
            }
        }

        let hovered = match hit {
            Some(Hit::Skill(id) | Hit::LevelUp(id)) => Some(id),
            _ => None,
        };
        let needed: Vec<u16> = hovered.map(|id| sheet.needs(id).into_iter().map(|(need, _)| need).collect()).unwrap_or_default();
        let points_left = self.points_left(sheet);
        for (id, corner) in self.shown(sheet) {
            let learned = sheet.learned(id);
            let skill_info = sheet.info.get(&id);
            // The box: a prerequisite of the hovered skill in violet, the hovered one that can be raised in orange.
            let fill = if needed.contains(&id) {
                1
            } else if hovered == Some(id) && self.raisable(sheet, id) {
                2
            } else {
                0
            };
            if fill == 1 {
                layer.sprites.push((&assets.needed_line, at(offset(corner, [-2.0, -2.0])), [ICON + 4.0, ICON + 4.0]));
            }
            layer.sprites.push((&assets.fills[fill], at(offset(corner, [-1.0, -1.0])), [ICON + 2.0, ICON + 2.0]));
            if let Some(Some(icon)) = skill_info.map(icon_path).and_then(|path| icons.get(&path)) {
                layer.sprites.push((icon, at(corner), [ICON, ICON]));
                if learned == 0 {
                    layer.sprites.push((&assets.veil, at(corner), [ICON, ICON]));
                }
            }
            if learned > 0 {
                cooling.draw(layer, id, at(corner), [ICON, ICON]);
            }
            // The name above, the level below, both centred on the icon.
            let name = skill_info.map_or_else(|| id.to_string(), |info| info.name.clone());
            let shown = window::fit(font, &name, NAME_WIDTH);
            let middle = corner[0] + ICON / 2.0;
            let top = corner[1] - ICON_AT[1];
            layer.texts.push((
                shown.clone(),
                at([(middle - font.width(&shown) / 2.0).floor(), top + NAME_Y]),
                if learned > 0 { LEARNED_NAME } else { UNLEARNED_NAME },
            ));
            let level = if learned > 0 { self.use_level(id, learned) } else { 0 };
            let level = level.to_string();
            layer.texts.push((level.clone(), at([(middle - font.width(&level) / 2.0).floor(), top + LEVEL_Y]), [0, 0, 0]));
            let pending = self.pending_levels(id);
            if pending > 0 {
                layer.texts.push((format!("+{pending}"), at([corner[0] - ICON_AT[0] + PENDING_X, corner[1]]), BLUE));
            }
            if self.pending.is_empty() && sheet.skills.iter().any(|skill| skill.id == id && steppable(skill)) {
                for (arrow, place) in assets.arrows.iter().zip(ARROWS) {
                    layer.sprites.push((arrow, at(offset(corner, place)), arrow.size));
                }
            }
            if self.raisable(sheet, id) {
                let state = match hit {
                    Some(Hit::LevelUp(target)) if target == id => 1 + usize::from(pointer.1),
                    _ => 0,
                };
                layer.sprites.push((&assets.up[state], at(offset(corner, RAISE_AT)), assets.up[state].size));
            }
            if hovered == Some(id) {
                window::tooltip(tooltip, chrome, font, &name, offset(pointer.0, [-10.0, -20.0]), [255; 3]);
            }
        }

        // The points, on the bottom bar: while levels are pending, left (blue), used (red) and the total.
        let bar_top = HEIGHT - BAR_HEIGHT;
        let [left, middle, right] = &assets.bar;
        layer.sprites.push((left, at([0.0, bar_top]), left.size));
        layer.sprites.push((middle, at([left.size[0], bar_top]), [WIDTH - left.size[0] - right.size[0], BAR_HEIGHT]));
        layer.sprites.push((right, at([WIDTH - right.size[0], bar_top]), right.size));
        let mut x = POINTS_AT[0];
        let mut put = |layer: &mut Layer<'a>, text: String, color: [u8; 3]| {
            let width = font.width(&text);
            layer.texts.push((text, at([x, POINTS_AT[1]]), color));
            x += width;
        };
        put(layer, message(POINTS_MESSAGE).to_owned(), [0, 0, 0]);
        if self.pending.is_empty() {
            put(layer, sheet.points.to_string(), [0, 0, 0]);
        } else {
            put(layer, points_left.to_string(), BLUE);
            put(layer, format!("({})", self.pending.len()), RED);
            put(layer, format!(" / {}", sheet.points), [0, 0, 0]);
        }
        // Apply and Reset, live only while something is pending.
        for (images, place, button) in [(&assets.apply, APPLY_AT, Hit::Apply), (&assets.reset, RESET_AT, Hit::Reset)] {
            let state = match hit {
                Some(hit) if hit == button => 1 + usize::from(pointer.1),
                _ => 0,
            };
            layer.sprites.push((&images[state], at(place), images[state].size));
            if self.pending.is_empty() {
                layer.sprites.push((&assets.veil, at(place), images[0].size));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(max_level: u16, needs: Vec<(u16, u16)>) -> SkillInfo {
        SkillInfo { max_level, needs, ..Default::default() }
    }

    #[test]
    fn lists_skills_no_tab_of_the_line_holds() {
        // A Swordman (1) from a Novice (0): Basic Skill 1 and Bash 5 sit in their trees.
        let trees = SkillTrees { trees: HashMap::from([(0, vec![(0, 1)]), (1, vec![(1, 5), (2, 6)])]), parents: HashMap::from([(1, 0)]) };
        let skill = |id| Skill { id, kind: Skill::SELF, level: 1, sp: 0, range: 0, upgradable: false };
        let plain = [skill(1), skill(5)];
        assert!(extra_skills(&plain, &trees, &[0, 1]).is_empty());
        assert_eq!(tabs(&trees, 1, &plain), [0, 1], "no extra skills, no tab");
        // A Wedding Ring's skills (I Miss You 336, I Will Protect You 334) go in, by ID; the tree's stay out.
        let married = [skill(336), skill(1), skill(334), skill(5)];
        assert_eq!(extra_skills(&married, &trees, &[0, 1]), [334, 336]);
        let jobs = tabs(&trees, 1, &married);
        assert_eq!(jobs, [0, 1, ETC]);
        let skill_info = HashMap::new();
        let sheet = Sheet { trees: &trees, jobs: &jobs, skills: &married, info: &skill_info, points: 0 };
        // Novice and Swordman share the first tab, which opens; Etc lists the rest by ID.
        let mut window = SkillWindow::default();
        window.frame.open = true;
        let ids = |window: &SkillWindow| window.skill_boxes(&sheet).into_iter().map(|(id, _)| id).collect::<Vec<_>>();
        assert_eq!(ids(&window), [1, 5, 6]);
        window.click(Hit::Tab(1));
        assert_eq!(ids(&window), [334, 336]);
        window.next_tab(&sheet);
        assert_eq!(ids(&window), [1, 5, 6], "Tab wraps round");
    }

    #[test]
    fn groups_the_line_in_tiers() {
        // A Dragon Knight: Novice and Swordman, Knight with Lord Knight, Rune Knight, Dragon Knight, then Etc.
        assert_eq!(tiers(&[0, 1, 7, 4008, 4060, 4252, ETC]), [vec![0, 1], vec![7, 4008], vec![4060], vec![4252], vec![ETC]]);
        assert_eq!(tiers(&[0, 1, 7, 4054]), [vec![0, 1], vec![7], vec![4054]]);
        assert_eq!(tiers(&[0]), [vec![0]]);
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
    fn lays_skills_out_seven_a_row() {
        // Icons at (76 + 68c, 44 + 56r), six rows showing.
        assert_eq!(icon_corner(0, 0), Some([76.0, 44.0]));
        assert_eq!(icon_corner(8, 0), Some([76.0 + 68.0, 44.0 + 56.0]));
        assert_eq!(icon_corner(41, 0), Some([76.0 + 6.0 * 68.0, 44.0 + 5.0 * 56.0]));
        assert_eq!(icon_corner(42, 0), None);
        assert_eq!((icon_corner(42, 1), icon_corner(0, 1)), (Some([76.0, 44.0 + 5.0 * 56.0]), None));
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
    fn fills_in_what_a_skill_needs_first() {
        // Bowling Bash (62) needs Bash (5) 10 and Two-Hand Sword Mastery (3) 5; the mastery needs Sword Mastery (2) 1.
        let needs = |id: u16| match id {
            62 => vec![(5, 10), (3, 5)],
            3 => vec![(2, 1)],
            _ => Vec::new(),
        };
        let max = |id: u16| if id == 99 { 0 } else { 10 };
        let learned = |id: u16| if id == 5 { 7 } else { 0 };
        // Prerequisites first, each up to what it needs, then the skill.
        assert_eq!(plan(62, learned, max, needs, 20), Ok(vec![5, 5, 5, 2, 3, 3, 3, 3, 3, 62]));
        // Short of the whole plan: the first level the points can't reach names its skill; nothing added.
        assert_eq!(plan(62, learned, max, needs, 4), Err(3));
        // A skill already learned enough needs nothing more; one at its maximum or not learnable here can't rise.
        assert_eq!(plan(5, learned, max, needs, 1), Ok(vec![5]));
        assert_eq!(plan(5, |_| 10, max, needs, 5), Err(5));
        assert_eq!(plan(1, learned, max, |_| vec![(99, 1)], 5), Err(99));
    }

    #[test]
    fn puts_points_in_pending_until_applied() {
        // A Swordman (1): Bash (5) in place 1 needs nothing; Magnum Break (7) in place 2 needs Bash 5.
        let trees = SkillTrees { trees: HashMap::from([(0, vec![(0, 1)]), (1, vec![(1, 5), (2, 7)])]), parents: HashMap::from([(1, 0)]) };
        let jobs = [0, 1];
        let skills = [Skill { id: 1, kind: 0, level: 9, sp: 0, range: 0, upgradable: false }];
        let skill_info = HashMap::from([(1, info(9, Vec::new())), (5, info(10, Vec::new())), (7, info(10, vec![(5, 5)]))]);
        let sheet = Sheet { trees: &trees, jobs: &jobs, skills: &skills, info: &skill_info, points: 7 };
        let mut window = SkillWindow::default();
        window.frame.open = true;
        // "+" by Magnum Break: five of Bash first, then it; two points left. Nothing goes out yet.
        assert_eq!(window.raise(&sheet, 7), Ok(()));
        assert_eq!((window.pending_levels(5), window.pending_levels(7), window.points_left(&sheet)), (5, 1, 1));
        // Basic Skill is at its maximum: no "+"; Bash still has one.
        let raisable: Vec<u16> = window.raise_boxes(&sheet).into_iter().map(|(id, _)| id).collect();
        assert_eq!(raisable, [5, 7]);
        // Apply and Reset show while something is pending; the levels go out prerequisites first.
        assert_eq!(window.button_boxes().len(), 2);
        let center = |area: [f32; 4]| [area[0] + area[2] / 2.0, area[1] + area[3] / 2.0];
        assert_eq!(window.hit(center(window.button_boxes()[0].1), &sheet), Some(Hit::Apply));
        assert_eq!(window.click(Hit::Apply), Some(Action::Apply));
        assert_eq!(window.take_pending(), [5, 5, 5, 5, 5, 7]);
        // Reset drops what's pending.
        window.raise(&sheet, 5).unwrap();
        window.click(Hit::Reset);
        assert!(window.pending().is_empty() && window.button_boxes().is_empty());
        // Short of points: nothing added, the skill named.
        let poor = Sheet { points: 3, ..sheet };
        assert_eq!(window.raise(&poor, 7), Err(5));
        assert!(window.pending().is_empty());
    }

    #[test]
    fn names_the_tabs_and_level_up_buttons_a_click_takes() {
        // Novice (0) then Mage (2): Basic Skill in place 0, Fire Bolt and Cold Bolt beside it.
        let trees = SkillTrees { trees: HashMap::from([(0, vec![(0, 1)]), (2, vec![(1, 19), (3, 14)])]), parents: HashMap::new() };
        let jobs = [0, 2];
        let skill = |id: u16, level: u16| Skill { id, kind: Skill::ENEMY, level, sp: 12, range: 9, upgradable: true };
        let skills = [skill(1, 9), skill(19, 0), skill(14, 10)];
        let skill_info = HashMap::from([(1, info(9, Vec::new())), (19, info(10, Vec::new())), (14, info(10, Vec::new()))]);
        let sheet = Sheet { trees: &trees, jobs: &jobs, skills: &skills, info: &skill_info, points: 5 };
        let mut window = SkillWindow::default();
        assert!(window.raise_boxes(&sheet).is_empty() && window.tab_boxes(&sheet).is_empty());
        window.frame.open = true;
        let center = |area: [f32; 4]| [area[0] + area[2] / 2.0, area[1] + area[3] / 2.0];
        // Only Fire Bolt can still rise, and its box is the one the click test takes.
        let boxes = window.raise_boxes(&sheet);
        assert_eq!(boxes.iter().map(|(id, _)| *id).collect::<Vec<_>>(), [19]);
        assert_eq!(window.hit(center(boxes[0].1), &sheet), Some(Hit::LevelUp(19)));
        assert!(window.raise_boxes(&Sheet { points: 0, ..sheet }).is_empty());
        // One tab for Novice and Mage, named by both jobs.
        let tabs = window.tab_boxes(&sheet);
        assert_eq!(tabs.iter().map(|(job, _)| *job).collect::<Vec<_>>(), [0, 2]);
        assert_eq!(tabs[0].1, tabs[1].1);
        assert_eq!(window.hit(center(tabs[0].1), &sheet), Some(Hit::Tab(0)));
        window.frame.minimized = true;
        assert!(window.tab_boxes(&sheet).is_empty());
    }
}
