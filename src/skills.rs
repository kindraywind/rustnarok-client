//! The skill window (Alt+S): the job's skills where the client's skill tree puts them, a tab down the left for each
//! tier of the line (Novice with the first job, the second job with its transcendent one, third, fourth) and Etc,
//! their levels, and the points left to put in. Points go in pending first and are sent on Apply. The title bar's
//! minimize button switches to a simple list, one skill a row with a level up button, and back; the choice is kept.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use rustnarok_client::grf::Grf;
use rustnarok_client::lua::{SkillInfo, SkillTrees};
use rustnarok_client::net::Skill;
use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};
use crate::window::{self, Chrome, Clicks, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI, contains, offset};

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
/// Levels put in but not sent: "+N" in red at (102 + 68c, 44 + 56r).
const PENDING_X: f32 = 102.0;
const BLUE: [u8; 3] = [0, 0, 255];
const RED: [u8; 3] = [255, 0, 0];
/// The arrows that pick the level to use, on the level line: left at x 56 + 68c, right at 111 + 68c.
const ARROWS: [[f32; 2]; 2] = [[56.0 - ICON_AT[0], LEVEL_Y - ICON_AT[1]], [111.0 - ICON_AT[0], LEVEL_Y - ICON_AT[1]]];
const ARROW: [f32; 2] = [11.0, 11.0];
/// A use level other than the learned one.
const USE_LEVEL: [u8; 3] = [255, 60, 127];
/// The level a prerequisite needs, beside its icon, black outlined in white.
const NEED_AT: [f32; 2] = [26.0, 14.0];
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
/// The list view: 330 wide, one skill a row 32 tall from y 32, ten rows a page.
const LIST_WIDTH: f32 = 330.0;
const LIST_TOP: f32 = 32.0;
const LIST_ROW: f32 = 32.0;
const LIST_ROWS: usize = 10;
/// The list view's corner grip changes only its height: 400 at most, a row less per 32 px up.
// ponytail: the least (the game's tab strip + 12) is taken as 3 rows.
const LIST_LEAST_ROWS: usize = 3;
/// In a row, from the icon at (44, top + 2): the level up button at (78, +2), the selection band 164×28 at (104, +0),
/// the name at (112, +3), "Lv :" at (113, +15) or (124, +15) beside the arrows at (112, +16) and (192, +16), the SP at
/// (212, +15).
const LIST_ICON_X: f32 = 44.0;
const LIST_UP: [f32; 2] = [78.0 - LIST_ICON_X, 0.0];
const UP_SIZE: [f32; 2] = [24.0, 24.0];
const LIST_BAND: [f32; 4] = [104.0 - LIST_ICON_X, -2.0, 164.0, 28.0];
const LIST_NAME: [f32; 2] = [112.0 - LIST_ICON_X, 1.0];
const LIST_LEVEL: [f32; 2] = [113.0 - LIST_ICON_X, 13.0];
const LIST_LEVEL_ARROWS: [f32; 2] = [124.0 - LIST_ICON_X, 13.0];
const LIST_ARROWS: [[f32; 2]; 2] = [[112.0 - LIST_ICON_X, 14.0], [192.0 - LIST_ICON_X, 14.0]];
const LIST_SP: [f32; 2] = [212.0 - LIST_ICON_X, 13.0];
// ponytail: the band's colour comes from the skin's table, which isn't read; a light blue stands in.
const BAND: [u8; 3] = [222, 232, 250];
/// Apply and Reset at (w − 115, h − 27) and (w − 65, h − 27); "Skill Point: %d" at (30, h − 23).
const LIST_APPLY_AT: [f32; 2] = [LIST_WIDTH - 115.0, HEIGHT - 27.0];
const LIST_RESET_AT: [f32; 2] = [LIST_WIDTH - 65.0, HEIGHT - 27.0];
const LIST_POINTS_AT: [f32; 2] = [30.0, HEIGHT - 23.0];
const LIST_POINTS_MESSAGE: usize = 284;
/// "Passive" in place of the SP.
const PASSIVE_MESSAGE: usize = 286;
/// The boxes of the hovered (or pinned) skill's prerequisites, pink; of a skill that can be raised, light blue.
const NEEDED_FILL: [u8; 3] = [255, 192, 204];
const NEEDED_LINE: [u8; 3] = [255, 182, 193];
const RAISABLE_FILL: [u8; 3] = [175, 196, 240];
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
    /// The list view's corner grip.
    resize: Texture,
    /// Whitens skills not learned.
    veil: Texture,
    /// The arrows that pick the level to use: less, more.
    arrows: [Texture; 2],
    /// Apply and Reset: normal, hovered, pressed.
    apply: [Texture; 3],
    reset: [Texture; 3],
    /// Box fills: plain, a prerequisite's, a raisable skill's; and a prerequisite's outline.
    fills: [Texture; 3],
    needed_line: Texture,
    /// The list view's selection band.
    band: Texture,
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
            resize: gpu.load_texture(grf, &format!("{UI}btn_resize.bmp"), false),
            veil: gpu.solid([255, 255, 255, 150]),
            arrows: ["arw_left", "arw_right"].map(image),
            apply: ["btn_apply", "btn_apply_a", "btn_apply_b"].map(root),
            reset: ["btn_reset", "btn_reset_a", "btn_reset_b"].map(root),
            fills: [solid([255; 3]), solid(NEEDED_FILL), solid(RAISABLE_FILL)],
            needed_line: solid(NEEDED_LINE),
            band: solid(BAND),
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(TitleHit),
    Tab(usize),
    Skill(u16),
    /// The list view's level up button by a skill.
    LevelUp(u16),
    /// An arrow on a skill's level line: one level more (`up`) or less to use, up to `learned`.
    Step {
        id: u16,
        up: bool,
        learned: u16,
    },
    Apply,
    Reset,
    /// The list view's corner grip.
    Resize,
    Window,
}

/// What a click on the window asks for.
#[derive(Debug, PartialEq)]
pub enum Action {
    /// Use the skill at its level (a double click).
    Use(u16),
    /// Put a level into the skill, with what it needs first (a click, let go on it).
    LevelUp(u16),
    /// Send the pending levels (after asking).
    Apply,
    /// The view was switched: the list (`true`) or the tree, to keep.
    View(bool),
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
    /// The skill whose prerequisites stay lit (a right click), instead of the hovered one's.
    pinned: Option<u16>,
    /// The simple list rather than the tree.
    simple: bool,
    /// The list view's rows, and its corner grip while held.
    list_rows: usize,
    grip: window::Grip,
}

impl Default for SkillWindow {
    fn default() -> Self {
        Self {
            frame: Frame::new(DEFAULT_POSITION, WIDTH),
            tab: usize::MAX,
            scroll: 0,
            clicks: Clicks::default(),
            chosen: HashMap::new(),
            pending: Vec::new(),
            pinned: None,
            simple: false,
            list_rows: LIST_ROWS,
            grip: window::Grip::default(),
        }
    }
}

fn view_path() -> Option<std::path::PathBuf> {
    crate::settings_path("skilllist")
}

/// Whether the skill window was last left in its list view.
pub fn load_view() -> bool {
    view_path().and_then(|path| std::fs::read_to_string(path).ok()).is_some_and(|text| text.trim() == "1")
}

pub fn save_view(simple: bool) {
    let Some(path) = view_path() else { return };
    let _ = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(path, if simple { "1" } else { "0" }));
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

/// The job's line, the first job first, then `ETC` when the player has skills none of it holds.
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

    /// Whether a skill sits in a tree of the line: one to learn here.
    fn in_line(&self, id: u16) -> bool {
        self.jobs.iter().filter(|&&job| job != ETC).filter_map(|job| self.trees.trees.get(job)).any(|tree| tree.iter().any(|&(_, each)| each == id))
    }

    /// How far points can take a skill here: 0 outside the line; the learned level when the server lists it as not
    /// upgradable; 0 when the server leaves it out though nothing it needs is missing (a quest skill).
    fn max(&self, id: u16) -> u16 {
        if !self.in_line(id) {
            return 0;
        }
        match self.skills.iter().find(|skill| skill.id == id) {
            Some(skill) if !skill.upgradable => skill.level,
            None if self.needs(id).iter().all(|&(need, at)| self.learned(need) >= at) => 0,
            _ => self.info.get(&id).map_or(0, |info| info.max_level),
        }
    }

    fn needs(&self, id: u16) -> Vec<(u16, u16)> {
        self.info.get(&id).map(|info| info.needs_for(self.jobs).to_vec()).unwrap_or_default()
    }

    /// Every skill `id` needs, however far back, each with the level it needs (the highest asked).
    fn chain(&self, id: u16) -> HashMap<u16, u16> {
        let mut chain = HashMap::new();
        let mut todo = vec![(id, 0)];
        while let Some((skill, depth)) = todo.pop() {
            for (need, at) in self.needs(skill) {
                let level = chain.entry(need).or_insert(0);
                *level = at.max(*level);
                if depth < 16 {
                    todo.push((need, depth + 1));
                }
            }
        }
        chain
    }

    /// The learned level when the skill's level to use can be picked (the level arrows show).
    fn steppable(&self, id: u16) -> Option<u16> {
        let learned = self.learned(id);
        (learned > 0 && self.info.get(&id).is_some_and(|info| info.separate_level)).then_some(learned)
    }
}

impl SkillWindow {
    pub fn height(&self) -> f32 {
        if self.simple { HEIGHT - (LIST_ROWS - self.list_rows) as f32 * LIST_ROW } else { HEIGHT }
    }

    /// The list view's rows.
    pub fn list_rows(&self) -> usize {
        self.list_rows
    }

    pub fn set_list_rows(&mut self, rows: usize) {
        self.list_rows = rows.clamp(LIST_LEAST_ROWS, LIST_ROWS);
    }

    pub fn start_resize(&mut self, cursor: [f32; 2]) {
        self.grip.start(cursor, [self.width(), self.height()]);
    }

    /// While held by its grip, the list view's bottom follows the pointer from the press, a row per 32 px (rounding
    /// toward the full 400); returns whether it's held.
    pub fn resize_to(&mut self, cursor: [f32; 2]) -> bool {
        let Some([_, asked]) = self.grip.asked(cursor) else { return false };
        let fewer = window::steps(HEIGHT - asked, 0.0, LIST_ROW, LIST_ROWS - LIST_LEAST_ROWS);
        self.set_list_rows(LIST_ROWS - fewer);
        true
    }

    pub fn stop_resize(&mut self) -> bool {
        self.grip.stop()
    }

    /// The list view's corner grip on screen (for the test driver).
    pub fn grip_box(&self) -> Option<[f32; 4]> {
        let [x, y] = self.frame.at([self.width() - window::GRIP, self.height() - window::GRIP]);
        (self.simple && self.frame.open && !self.frame.minimized).then_some([x, y, window::GRIP, window::GRIP])
    }

    fn width(&self) -> f32 {
        if self.simple { LIST_WIDTH } else { WIDTH }
    }

    /// The simple list (`true`) or the tree.
    pub fn set_simple(&mut self, simple: bool) {
        self.simple = simple;
        self.frame.set_width(self.width());
        self.scroll = 0;
    }

    /// Where the level arrows sit from a skill's icon.
    fn arrows(&self) -> [[f32; 2]; 2] {
        if self.simple { LIST_ARROWS } else { ARROWS }
    }

    /// Apply and Reset.
    fn buttons(&self) -> [[f32; 2]; 2] {
        let lift = HEIGHT - self.height();
        if self.simple { [LIST_APPLY_AT, LIST_RESET_AT].map(|[x, y]| [x, y - lift]) } else { [APPLY_AT, RESET_AT] }
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

    /// Whether a skill can be raised now: points left, below its maximum, and what it needs in place.
    fn raisable(&self, sheet: &Sheet, id: u16) -> bool {
        self.points_left(sheet) > 0 && self.level(sheet, id) < sheet.max(id) && sheet.needs(id).iter().all(|&(need, at)| self.level(sheet, need) >= at)
    }

    /// Puts one more level into `id`, and every level of a prerequisite it still lacks, as far as the points go.
    /// Nothing without points or past the maximum. Err: the skill the points can't raise (nothing is added).
    pub fn raise(&mut self, sheet: &Sheet, id: u16) -> Result<(), u16> {
        if self.points_left(sheet) <= 0 || self.level(sheet, id) >= sheet.max(id) {
            return Ok(());
        }
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
        let tree = self.tree(sheet).into_iter();
        if self.simple {
            let row = |row: usize| [LIST_ICON_X, LIST_TOP + 2.0 + row as f32 * LIST_ROW];
            return tree.skip(self.scroll).take(self.list_rows).enumerate().map(|(index, (_, id))| (id, row(index))).collect();
        }
        tree.filter_map(|(place, id)| Some((id, icon_corner(place, self.scroll)?))).collect()
    }

    /// The wheel over the window: whole rows, three a notch (`rows` > 0 down).
    pub fn scroll_by(&mut self, sheet: &Sheet, rows: i32) {
        let tree = self.tree(sheet);
        let most = if self.simple {
            tree.len().saturating_sub(self.list_rows)
        } else {
            let last = tree.iter().map(|(place, _)| place / COLUMNS).max().unwrap_or(0);
            (last + 1).saturating_sub(ROWS)
        };
        let step = WHEEL_ROWS as i64 * i64::from(rows.signum());
        self.scroll = (self.scroll as i64 + step).clamp(0, most as i64) as usize;
    }

    pub fn hit(&self, cursor: [f32; 2], sheet: &Sheet) -> Option<Hit> {
        let local = self.frame.local(cursor, self.height())?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(Hit::Title(hit));
        }
        if self.simple && !self.frame.minimized && window::on_grip([self.width(), self.height()], local) {
            return Some(Hit::Resize);
        }
        let tabs = self.tab_jobs(sheet);
        if let Some(tab) = (0..tabs.len()).find(|&tab| contains(tab_corner(tab), TAB, local)) {
            return Some(Hit::Tab(tab));
        }
        let [apply, reset] = self.buttons();
        if !self.pending.is_empty() {
            if contains(apply, BUTTON, local) {
                return Some(Hit::Apply);
            }
            if contains(reset, BUTTON, local) {
                return Some(Hit::Reset);
            }
        }
        for (id, corner) in self.shown(sheet) {
            if self.simple && self.raisable(sheet, id) && contains(offset(corner, LIST_UP), UP_SIZE, local) {
                return Some(Hit::LevelUp(id));
            }
            if self.pending.is_empty()
                && let Some(learned) = sheet.steppable(id)
                && let Some(arrow) = self.arrows().iter().position(|&at| contains(offset(corner, at), ARROW, local))
            {
                return Some(Hit::Step { id, up: arrow == 1, learned });
            }
            if contains(offset(corner, [-1.0, -1.0]), [ICON + 2.0, ICON + 2.0], local) {
                return Some(Hit::Skill(id));
            }
        }
        Some(Hit::Window)
    }

    pub fn click(&mut self, hit: Hit) -> Option<Action> {
        // A left press lets go of a chain a right click pinned; the point is still added below.
        self.pinned = None;
        match hit {
            // The minimize button switches the view instead of collapsing the window.
            Hit::Title(TitleHit::Minimize) => {
                self.set_simple(!self.simple);
                return Some(Action::View(self.simple));
            }
            Hit::Title(hit) => self.frame.click_title(hit),
            Hit::Tab(tab) => {
                self.tab = tab;
                self.scroll = 0;
            }
            // A double click uses it; one click raises it in the tree (the list has its buttons).
            Hit::Skill(id) => {
                return match self.clicks.double(id) {
                    true => Some(Action::Use(id)),
                    false => (!self.simple).then_some(Action::LevelUp(id)),
                };
            }
            Hit::LevelUp(id) => return Some(Action::LevelUp(id)),
            Hit::Step { id, up, learned } => {
                let level = self.use_level(id, learned);
                let level = if up { level + 1 } else { level.saturating_sub(1) };
                self.chosen.insert(id, level.clamp(1, learned));
            }
            Hit::Apply => return Some(Action::Apply),
            Hit::Reset => self.pending.clear(),
            Hit::Resize | Hit::Window => {}
        }
        None
    }

    /// A left press on the window: the pinned chain lets go, and the pointer's chain follows again.
    pub fn unpin(&mut self) {
        self.pinned = None;
    }

    /// A right click: on a skill, keeps its prerequisites lit (again on it: lets go); elsewhere, lets go.
    pub fn pin(&mut self, skill: Option<u16>) {
        self.pinned = if skill == self.pinned { None } else { skill };
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

    /// The level arrows on screen (for the test driver): `skill_less:<id>` and `skill_more:<id>` with their boxes.
    pub fn arrow_boxes(&self, sheet: &Sheet) -> Vec<(String, [f32; 4])> {
        if !self.frame.open || self.frame.minimized || !self.pending.is_empty() {
            return Vec::new();
        }
        self.shown(sheet)
            .into_iter()
            .filter(|&(id, _)| sheet.steppable(id).is_some())
            .flat_map(|(id, corner)| {
                ["less", "more"].into_iter().zip(self.arrows()).map(move |(name, place)| {
                    let [x, y] = self.frame.at(offset(corner, place));
                    (format!("skill_{name}:{id}"), [x, y, ARROW[0], ARROW[1]])
                })
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

    /// The list view's level up buttons on screen (for the test driver): `skill_up:<id>` and its box.
    pub fn up_boxes(&self, sheet: &Sheet) -> Vec<(String, [f32; 4])> {
        if !self.frame.open || !self.simple {
            return Vec::new();
        }
        self.shown(sheet)
            .into_iter()
            .filter(|&(id, _)| self.raisable(sheet, id))
            .map(|(id, corner)| {
                let [x, y] = self.frame.at(offset(corner, LIST_UP));
                (format!("skill_up:{id}"), [x, y, UP_SIZE[0], UP_SIZE[1]])
            })
            .collect()
    }

    /// The title bar's minimize button, which switches the view, on screen (for the test driver).
    pub fn view_box(&self) -> Option<[f32; 4]> {
        let [x, y] = self.frame.at(self.frame.minimize_button());
        self.frame.open.then_some([x, y, window::BUTTON_SIZE[0], window::BUTTON_SIZE[1]])
    }

    /// Whether the simple list shows rather than the tree.
    pub fn simple(&self) -> bool {
        self.simple
    }

    /// Apply and Reset on screen (for the test driver), while levels are pending.
    pub fn button_boxes(&self) -> Vec<(&'static str, [f32; 4])> {
        if !self.frame.open || self.frame.minimized || self.pending.is_empty() {
            return Vec::new();
        }
        let [apply, reset] = self.buttons();
        [("skill_apply", apply), ("skill_reset", reset)]
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
        let width = self.width();
        layer.sprites.push((&chrome.white, at([0.0, TITLE_HEIGHT]), [width, self.height() - TITLE_HEIGHT - BAR_HEIGHT]));

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

        if self.simple {
            self.draw_rows(layer, assets, icons, font, messages, sheet, hit, pointer.1, cooling);
        } else {
            // Every cell's empty box, under the skills.
            for cell in 0..COLUMNS * ROWS {
                if let Some(corner) = icon_corner(cell, 0) {
                    layer.sprites.push((&assets.slot, at(corner), assets.slot.size));
                }
            }
            let hovered = match hit {
                Some(Hit::Skill(id)) => Some(id),
                _ => None,
            };
            // The pinned skill's prerequisites, else the hovered one's: each with the level it needs.
            let chain = self.pinned.or(hovered).map(|id| sheet.chain(id)).unwrap_or_default();
            for (id, corner) in self.shown(sheet) {
                let learned = sheet.learned(id);
                let skill_info = sheet.info.get(&id);
                let needed = chain.get(&id).copied();
                let raisable = self.raisable(sheet, id);
                // The box: pink in the chain, light blue when it can be raised, else white.
                let (line, fill) = match (needed, raisable) {
                    (Some(_), _) => (Some(&assets.needed_line), 1),
                    (None, true) => (Some(&assets.fills[2]), 2),
                    (None, false) => (None, 0),
                };
                if let Some(line) = line {
                    layer.sprites.push((line, at(offset(corner, [-2.0, -2.0])), [ICON + 4.0, ICON + 4.0]));
                }
                layer.sprites.push((&assets.fills[fill], at(offset(corner, [-1.0, -1.0])), [ICON + 2.0, ICON + 2.0]));
                if let Some(Some(icon)) = skill_info.map(icon_path).and_then(|path| icons.get(&path)) {
                    layer.sprites.push((icon, at(corner), [ICON, ICON]));
                    // ponytail: whitened rather than greyed; the renderer has no grey mode.
                    if learned == 0 && fill == 0 {
                        layer.sprites.push((&assets.veil, at(corner), [ICON, ICON]));
                    }
                }
                if learned > 0 {
                    cooling.draw(layer, id, at(corner), [ICON, ICON]);
                }
                // The name above, the level below (learned skills only), both centred on the icon.
                let name = skill_info.map_or_else(|| id.to_string(), |info| info.name.clone());
                let shown = window::fit(font, &name, NAME_WIDTH);
                let middle = corner[0] + ICON / 2.0;
                let top = corner[1] - ICON_AT[1];
                layer.texts.push((
                    shown.clone(),
                    at([(middle - font.width(&shown) / 2.0).floor(), top + NAME_Y]),
                    if learned > 0 { LEARNED_NAME } else { UNLEARNED_NAME },
                ));
                if learned > 0 {
                    // A level to pick: "use / learned", the use level coloured when it isn't the learned one.
                    let (first, rest) = match sheet.steppable(id) {
                        Some(learned) => (self.use_level(id, learned), format!(" / {learned}")),
                        None => (learned, String::new()),
                    };
                    let first_text = first.to_string();
                    let left = (middle - (font.width(&first_text) + font.width(&rest)) / 2.0).floor();
                    let color = if first == learned { [0, 0, 0] } else { USE_LEVEL };
                    layer.texts.push((rest, at([left + font.width(&first_text), top + LEVEL_Y]), [0, 0, 0]));
                    layer.texts.push((first_text, at([left, top + LEVEL_Y]), color));
                }
                let pending = self.pending_levels(id);
                if pending > 0 {
                    layer.texts.push((format!("+{pending}"), at([corner[0] - ICON_AT[0] + PENDING_X, corner[1]]), RED));
                }
                if self.pending.is_empty() && sheet.steppable(id).is_some() {
                    for (arrow, place) in assets.arrows.iter().zip(ARROWS) {
                        layer.sprites.push((arrow, at(offset(corner, place)), arrow.size));
                    }
                }
                if let Some(level) = needed {
                    let text = level.to_string();
                    let place = at(offset(corner, NEED_AT));
                    for nudge in [[-1.0, 0.0], [1.0, 0.0], [0.0, -1.0], [0.0, 1.0]] {
                        layer.texts.push((text.clone(), offset(place, nudge), [255; 3]));
                    }
                    layer.texts.push((text, place, [0, 0, 0]));
                }
                if hovered == Some(id) {
                    window::tooltip(tooltip, chrome, font, &name, offset(pointer.0, [-10.0, -20.0]), [255; 3]);
                }
            }
        }

        // The points, on the bottom bar: while levels are pending, left (blue), used (red) and the total. The list
        // says the points left.
        let bar_top = self.height() - BAR_HEIGHT;
        let [left, middle, right] = &assets.bar;
        layer.sprites.push((left, at([0.0, bar_top]), left.size));
        layer.sprites.push((middle, at([left.size[0], bar_top]), [width - left.size[0] - right.size[0], BAR_HEIGHT]));
        layer.sprites.push((right, at([width - right.size[0], bar_top]), right.size));
        if self.simple {
            let line = message(LIST_POINTS_MESSAGE).replace("%d", &self.points_left(sheet).to_string());
            layer.texts.push((line, at([LIST_POINTS_AT[0], LIST_POINTS_AT[1] - (HEIGHT - self.height())]), [0, 0, 0]));
            layer.sprites.push((&assets.resize, at([width - window::GRIP, self.height() - window::GRIP]), assets.resize.size));
        } else {
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
                put(layer, self.points_left(sheet).to_string(), BLUE);
                put(layer, format!("({})", self.pending.len()), RED);
                put(layer, format!(" / {}", sheet.points), [0, 0, 0]);
            }
        }
        // Apply and Reset, live only while something is pending.
        let [apply, reset] = self.buttons();
        for (images, place, button) in [(&assets.apply, apply, Hit::Apply), (&assets.reset, reset, Hit::Reset)] {
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
    /// The list view's rows: the hovered one's band, the icon, the level up button, the name, "Lv :" with the level
    /// (red while pending; "use / learned" between the arrows for a level to pick) and the SP, or "Passive".
    #[allow(clippy::too_many_arguments)]
    fn draw_rows<'a>(
        &self,
        layer: &mut Layer<'a>,
        assets: &'a Assets,
        icons: &'a HashMap<String, Option<Texture>>,
        font: &Font,
        messages: &[String],
        sheet: &Sheet,
        hit: Option<Hit>,
        pressed: bool,
        cooling: &Cooling<'a>,
    ) {
        let at = |local: [f32; 2]| self.frame.at(local);
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        let hovered = match hit {
            Some(Hit::Skill(id) | Hit::LevelUp(id) | Hit::Step { id, .. }) => Some(id),
            _ => None,
        };
        for (id, corner) in self.shown(sheet) {
            let place = |local: [f32; 2]| at(offset(corner, local));
            let learned = sheet.learned(id);
            let raisable = self.raisable(sheet, id);
            if hovered == Some(id) {
                let [x, y, w, h] = LIST_BAND;
                layer.sprites.push((&assets.band, place([x, y]), [w, h]));
            }
            let skill_info = sheet.info.get(&id);
            if let Some(Some(icon)) = skill_info.map(icon_path).and_then(|path| icons.get(&path)) {
                layer.sprites.push((icon, at(corner), [ICON, ICON]));
                if learned == 0 && !raisable {
                    layer.sprites.push((&assets.veil, at(corner), [ICON, ICON]));
                }
            }
            if learned > 0 {
                cooling.draw(layer, id, at(corner), [ICON, ICON]);
            }
            if raisable {
                let state = if hit == Some(Hit::LevelUp(id)) { 1 + usize::from(pressed) } else { 0 };
                layer.sprites.push((&assets.up[state], place(LIST_UP), UP_SIZE));
            }
            let name = skill_info.map_or_else(|| id.to_string(), |info| info.name.clone());
            let name = window::fit(font, &name, LIST_WIDTH - 20.0 - LIST_ICON_X - LIST_NAME[0]);
            layer.texts.push((name, place(LIST_NAME), if learned > 0 { [0, 0, 0] } else { UNLEARNED_NAME }));
            // "Lv :" and the level; with the arrows, the level to use over the learned one.
            let steppable = sheet.steppable(id).filter(|_| self.pending.is_empty());
            let mut x = if steppable.is_some() { LIST_LEVEL_ARROWS[0] } else { LIST_LEVEL[0] };
            let mut put = |text: String, color: [u8; 3]| {
                let width = font.width(&text);
                layer.texts.push((text, place([x, LIST_LEVEL[1]]), color));
                x += width;
            };
            put("Lv : ".to_owned(), [0, 0, 0]);
            let pending = self.pending_levels(id);
            match steppable {
                Some(learned) => {
                    let level = self.use_level(id, learned);
                    put(format!("{level:2}"), if level == learned { [0, 0, 0] } else { USE_LEVEL });
                    put(format!(" / {learned}"), [0, 0, 0]);
                }
                None => put(format!("{:2}", learned + pending), if pending > 0 { RED } else { [0, 0, 0] }),
            }
            if steppable.is_some() {
                for (arrow, at) in assets.arrows.iter().zip(LIST_ARROWS) {
                    layer.sprites.push((arrow, place(at), arrow.size));
                }
            }
            if let Some(skill) = sheet.skills.iter().find(|skill| skill.id == id && skill.level > 0) {
                let sp = if skill.kind == 0 { message(PASSIVE_MESSAGE).to_owned() } else { format!("Sp : {}", skill.sp) };
                layer.texts.push((sp, place(LIST_SP), [0, 0, 0]));
            }
        }
    }
}

/// What fills a refusal line's blanks.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Fill {
    Nothing,
    /// The skill's name.
    Skill,
    /// The item's name, and for "[%s] required '%d' amount." the number.
    Item,
    ItemCount(i32),
    Number(i32),
}

/// The red lines for a refused skill (ZC_ACK_TOUSESKILL): message ids and what fills each, by cause and the skill's
/// detail (`btype`). Some only show with `/skillfail` on (`skill_fail`).
pub fn refusal_lines(skill: u16, btype: i32, cause: u8, skill_fail: bool) -> Vec<(usize, Fill)> {
    let line = |id: usize| vec![(id, Fill::Nothing)];
    let by = |ids: &[usize]| usize::try_from(btype).ok().and_then(|index| ids.get(index)).map_or(Vec::new(), |&id| line(id));
    let quiet = |id: usize| if skill_fail { line(id) } else { Vec::new() };
    match cause {
        // Failed: the skill's own reason (Basic Skill's by what it was for), else the general line.
        0 => match (skill, btype) {
            (1, 0..=6) => line(159 + btype as usize),
            (1, 7) => vec![(383, Fill::Nothing), (1304, Fill::Nothing)],
            (1, 8) => line(1304),
            (27, _) => line(214),
            (50, _) => line(205),
            (52, _) => line(207),
            _ => quiet(204),
        },
        1 => line(202),
        2 => line(203),
        3 => line(808),
        // The cooldown, by name; none for two skills that show their own.
        4 if skill_fail && ![6552, 6601].contains(&skill) => vec![(1460, Fill::Skill)],
        4 => Vec::new(),
        5 => line(233),
        6 => line(239),
        7 => line(246),
        8 => line(247),
        9 => line(580),
        // ponytail: the game also drops the four Tetra Vortex states here for skills 2217–2221.
        10 => quiet(285),
        11..=25 => line([1396, 1397, 1398, 1399, 1400, 1401, 1411, 1412, 1413, 1414, 1415, 1416, 1417, 1425, 1864][usize::from(cause - 11)]),
        26..=37 => line([1427, 1428, 1929, 1932, 1937, 1433, 1434, 1435, 1948, 1952, 1951, 1439][usize::from(cause - 26)]),
        40 => line(1450),
        43 => line(1453),
        50..=54 => line([1462, 1474, 1475, 1476, 1477][usize::from(cause - 50)]),
        57 => line(1519),
        60 => line(1522),
        71 => vec![(1536, Fill::ItemCount(btype))],
        72 => vec![(1538, Fill::Item)],
        // ponytail: the detail picks between each pair, 0 the first.
        73 => line(if btype == 0 { 1559 } else { 1560 }),
        74 => line(if btype == 0 { 1561 } else { 1562 }),
        85 => line(if btype == 0 { 2552 } else { 2553 }),
        // ponytail: the detail indexes the weapon lines.
        78 => by(&[1691, 1692, 1863, 1994, 2554, 2555, 2556, 2557, 2561]),
        79..=84 => line([1748, 1823, 1824, 1825, 1954, 2466][usize::from(cause - 79)]),
        87 => line(2563),
        // The detail is the message.
        86 => usize::try_from(btype).map_or(Vec::new(), line),
        88 => line(2626),
        89 => line(2982),
        90 | 91 | 92 | 98 | 101 => {
            let id = match cause {
                90 => 2997,
                91 => 3006,
                92 => 3458,
                98 => 3584,
                _ => 3773,
            };
            vec![(id, Fill::Number(btype))]
        }
        93..=97 => line([3482, 3483, 3529, 3530, 3533][usize::from(cause - 93)]),
        99 => line(3604),
        100 => line(3643),
        102..=110 => line(3957 + usize::from(cause - 102)),
        111..=115 => line([3968, 3987, 4004, 4005, 4193][usize::from(cause - 111)]),
        116 | 117 => vec![(4377, Fill::Number(btype))],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_view_resizes_its_height() {
        let mut window = SkillWindow::default();
        window.frame.open = true;
        assert_eq!((window.grip_box(), window.height()), (None, 400.0), "the tree view has no grip");
        window.set_simple(true);
        let [x, y, w, h] = window.grip_box().unwrap();
        let corner = [x + w / 2.0, y + h / 2.0];
        window.start_resize(corner);
        // 93 px up is two rows fewer (rounding toward 400); down never grows it past 400.
        assert!(window.resize_to([corner[0], corner[1] - 93.0]));
        assert_eq!((window.list_rows(), window.height()), (8, 336.0));
        window.resize_to([corner[0], corner[1] + 200.0]);
        assert_eq!(window.list_rows(), LIST_ROWS);
        window.resize_to([corner[0], corner[1] - 1000.0]);
        assert_eq!(window.list_rows(), LIST_LEAST_ROWS);
        assert!(window.stop_resize());
        // Apply and Reset follow the bottom.
        assert_eq!(window.buttons()[0][1], window.height() - 27.0);
    }

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
        // A Doram's Summoner tab has no Novice in it.
        assert_eq!(tiers(&[4218, 4308]), [vec![4218], vec![4308]]);
    }

    #[test]
    fn shows_each_job_of_the_line_in_its_tab() {
        // A Lord Knight: Basic Skill (1) and Bash (5) in the first tab, Pierce (55) with Berserk (355) in the second.
        let trees = SkillTrees {
            trees: HashMap::from([(0, vec![(0, 1)]), (1, vec![(1, 5)]), (7, vec![(10, 55)]), (4008, vec![(4, 355)])]),
            parents: HashMap::from([(1, 0), (7, 1), (4008, 7)]),
        };
        let skill = |id| Skill { id, kind: Skill::SELF, level: 1, sp: 0, range: 0, upgradable: false };
        let skills = [skill(355), skill(5), skill(55), skill(1)];
        let jobs = tabs(&trees, 4008, &skills);
        assert_eq!(jobs, [0, 1, 7, 4008], "every skill sits in a tree of the line: no Etc");
        let skill_info = HashMap::new();
        let sheet = Sheet { trees: &trees, jobs: &jobs, skills: &skills, info: &skill_info, points: 0 };
        let mut window = SkillWindow::default();
        window.frame.open = true;
        let ids = |window: &SkillWindow| window.skill_boxes(&sheet).into_iter().map(|(id, _)| id).collect::<Vec<_>>();
        // It opens on the highest tab, by place.
        assert_eq!(ids(&window), [355, 55]);
        window.click(Hit::Tab(0));
        assert_eq!(ids(&window), [1, 5]);
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
        // Arrows for a learned skill whose level can be picked, even at level 1.
        let trees = SkillTrees::default();
        let skill = |id, level| Skill { id, kind: Skill::FRIEND, level, sp: 13, range: 9, upgradable: false };
        let skills = [skill(1, 9), skill(28, 1)];
        let skill_info = HashMap::from([(1, info(9, Vec::new())), (28, SkillInfo { separate_level: true, ..info(10, Vec::new()) })]);
        let sheet = Sheet { trees: &trees, jobs: &[], skills: &skills, info: &skill_info, points: 0 };
        assert_eq!((sheet.steppable(1), sheet.steppable(28), sheet.steppable(29)), (None, Some(1), None));
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
        // The server lists Bash, not yet learned; Magnum Break waits on it.
        let skills = [
            Skill { id: 1, kind: 0, level: 9, sp: 0, range: 0, upgradable: false },
            Skill { id: 5, kind: Skill::ENEMY, level: 0, sp: 8, range: 1, upgradable: true },
        ];
        let skill_info = HashMap::from([(1, info(9, Vec::new())), (5, info(10, Vec::new())), (7, info(10, vec![(5, 5)]))]);
        let sheet = Sheet { trees: &trees, jobs: &jobs, skills: &skills, info: &skill_info, points: 7 };
        let mut window = SkillWindow::default();
        window.frame.open = true;
        // Magnum Break needs Bash first: only Bash can be raised on its own.
        assert!(window.raisable(&sheet, 5) && !window.raisable(&sheet, 7));
        // Raising Magnum Break: five of Bash first, then it; one point left. Nothing goes out yet.
        assert_eq!(window.raise(&sheet, 7), Ok(()));
        assert_eq!((window.pending_levels(5), window.pending_levels(7), window.points_left(&sheet)), (5, 1, 1));
        // Basic Skill is at its maximum; Bash and Magnum Break can still go up.
        assert!(!window.raisable(&sheet, 1) && window.raisable(&sheet, 5) && window.raisable(&sheet, 7));
        assert_eq!(window.raise(&sheet, 1), Ok(()), "past the maximum: nothing, and no complaint");
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
    fn raises_a_skill_a_click_is_on_and_uses_it_on_a_double_click() {
        let mut window = SkillWindow::default();
        assert_eq!(window.click(Hit::Skill(19)), Some(Action::LevelUp(19)));
        assert_eq!(window.click(Hit::Skill(19)), Some(Action::Use(19)));
        assert_eq!(window.click(Hit::Skill(14)), Some(Action::LevelUp(14)));
    }

    #[test]
    fn leaves_out_skills_points_dont_learn() {
        // A Novice: the server lists Basic Skill (1) but not First Aid (142), a quest skill that needs nothing.
        let trees = SkillTrees { trees: HashMap::from([(0, vec![(0, 1), (1, 142)])]), parents: HashMap::new() };
        let skills = [Skill { id: 1, kind: 0, level: 0, sp: 0, range: 0, upgradable: true }];
        let skill_info = HashMap::from([(1, info(9, Vec::new())), (142, info(1, Vec::new()))]);
        let sheet = Sheet { trees: &trees, jobs: &[0], skills: &skills, info: &skill_info, points: 3 };
        let window = SkillWindow::default();
        assert!(window.raisable(&sheet, 1) && !window.raisable(&sheet, 142));
        // Listed but not upgradable (a skill the job can't take further): it stays where it is.
        let held = [Skill { level: 4, upgradable: false, ..skills[0] }];
        assert_eq!(Sheet { skills: &held, ..sheet }.max(1), 4);
    }

    #[test]
    fn lights_the_whole_chain_a_skill_needs() {
        // Comet (2213) needs Hell Inferno (2212) 3, which needs Crimson Rock (2211) 2, which needs Summon Fire Ball (2208) 1.
        let trees = SkillTrees::default();
        let skill_info = HashMap::from([(2213, info(5, vec![(2212, 3)])), (2212, info(5, vec![(2211, 2)])), (2211, info(5, vec![(2208, 1)]))]);
        let sheet = Sheet { trees: &trees, jobs: &[], skills: &[], info: &skill_info, points: 0 };
        assert_eq!(sheet.chain(2213), HashMap::from([(2212, 3), (2211, 2), (2208, 1)]));
        assert!(sheet.chain(2208).is_empty());
        // A right click keeps a chain lit; again on it, or off any skill, lets go.
        let mut window = SkillWindow::default();
        window.pin(Some(2213));
        assert_eq!(window.pinned, Some(2213));
        window.pin(Some(2213));
        assert_eq!(window.pinned, None);
        window.pin(Some(2212));
        window.pin(None);
        assert_eq!(window.pinned, None);
        // A left press lets go, and still raises the skill.
        window.pin(Some(2213));
        assert_eq!(window.click(Hit::Skill(2212)), Some(Action::LevelUp(2212)));
        assert_eq!(window.pinned, None);
    }

    #[test]
    fn switches_to_a_list_with_level_up_buttons() {
        // A Mage (2) with twelve skills in a row of places; Fire Bolt (19) can rise, Basic Skill (1) is at its maximum.
        let places: Vec<(usize, u16)> = (0..12).map(|place| (place, 100 + place as u16)).collect();
        let trees = SkillTrees { trees: HashMap::from([(0, vec![(20, 1)]), (2, [places, vec![(21, 19)]].concat())]), parents: HashMap::new() };
        let skill = |id: u16, level: u16| Skill { id, kind: Skill::ENEMY, level, sp: 12, range: 9, upgradable: true };
        let skills = [skill(1, 9), skill(19, 2)];
        let skill_info = HashMap::from([(1, info(9, Vec::new())), (19, info(10, Vec::new()))]);
        let sheet = Sheet { trees: &trees, jobs: &[0, 2], skills: &skills, info: &skill_info, points: 3 };
        let mut window = SkillWindow::default();
        window.frame.open = true;
        let center = |area: [f32; 4]| [area[0] + area[2] / 2.0, area[1] + area[3] / 2.0];
        // The minimize button switches the view (the window doesn't collapse) and says which to keep.
        assert_eq!(window.hit(center(window.view_box().unwrap()), &sheet), Some(Hit::Title(TitleHit::Minimize)));
        assert_eq!(window.click(Hit::Title(TitleHit::Minimize)), Some(Action::View(true)));
        assert!(window.simple() && !window.frame.minimized);
        assert_eq!(window.view_box().unwrap()[0], DEFAULT_POSITION[0] + LIST_WIDTH - 29.0);
        // One skill a row, 32 apart from y 34, in place order; ten rows a page.
        let boxes = window.skill_boxes(&sheet);
        assert_eq!(boxes.len(), LIST_ROWS);
        assert_eq!(boxes[0].1, [DEFAULT_POSITION[0] + 44.0, DEFAULT_POSITION[1] + 34.0, ICON, ICON]);
        assert_eq!(boxes[1].1[1], DEFAULT_POSITION[1] + 66.0);
        // Fourteen rows: the wheel stops at the fifth.
        window.scroll_by(&sheet, 1);
        window.scroll_by(&sheet, 1);
        assert_eq!(window.skill_boxes(&sheet)[0].0, 104);
        // Only Fire Bolt gets a level up button; it raises, a click on the skill doesn't.
        let ups = window.up_boxes(&sheet);
        assert_eq!(ups.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>(), ["skill_up:19"]);
        assert_eq!(window.hit(center(ups[0].1), &sheet), Some(Hit::LevelUp(19)));
        assert_eq!(window.click(Hit::LevelUp(19)), Some(Action::LevelUp(19)));
        assert_eq!(window.click(Hit::Skill(19)), None);
        assert_eq!(window.click(Hit::Skill(19)), Some(Action::Use(19)));
        // Apply and Reset sit at (w − 115, h − 27) and (w − 65, h − 27).
        window.raise(&sheet, 19).unwrap();
        let buttons = window.button_boxes();
        assert_eq!(buttons[0].1[0] - DEFAULT_POSITION[0], LIST_WIDTH - 115.0);
        assert_eq!(buttons[1].1[0] - DEFAULT_POSITION[0], LIST_WIDTH - 65.0);
        window.click(Hit::Title(TitleHit::Minimize));
        assert!(!window.simple() && window.up_boxes(&sheet).is_empty());
    }

    #[test]
    fn names_the_tabs_a_click_takes() {
        // Novice (0) then Mage (2): Basic Skill in place 0, Fire Bolt and Cold Bolt beside it.
        let trees = SkillTrees { trees: HashMap::from([(0, vec![(0, 1)]), (2, vec![(1, 19), (3, 14)])]), parents: HashMap::new() };
        let jobs = [0, 2];
        let skill = |id: u16, level: u16| Skill { id, kind: Skill::ENEMY, level, sp: 12, range: 9, upgradable: true };
        let skills = [skill(1, 9), skill(19, 0), skill(14, 10)];
        let skill_info = HashMap::from([(1, info(9, Vec::new())), (19, info(10, Vec::new())), (14, info(10, Vec::new()))]);
        let sheet = Sheet { trees: &trees, jobs: &jobs, skills: &skills, info: &skill_info, points: 5 };
        let mut window = SkillWindow::default();
        assert!(window.skill_boxes(&sheet).is_empty() && window.tab_boxes(&sheet).is_empty());
        window.frame.open = true;
        let center = |area: [f32; 4]| [area[0] + area[2] / 2.0, area[1] + area[3] / 2.0];
        // Only Fire Bolt can still rise, and its box is the one the click test takes.
        assert_eq!([1, 19, 14].map(|id| window.raisable(&sheet, id)), [false, true, false]);
        assert!(!window.raisable(&Sheet { points: 0, ..sheet }, 19));
        let fire_bolt = window.skill_boxes(&sheet).into_iter().find(|(id, _)| *id == 19).unwrap();
        assert_eq!(window.hit(center(fire_bolt.1), &sheet), Some(Hit::Skill(19)));
        // One tab for Novice and Mage, named by both jobs.
        let tabs = window.tab_boxes(&sheet);
        assert_eq!(tabs.iter().map(|(job, _)| *job).collect::<Vec<_>>(), [0, 2]);
        assert_eq!(tabs[0].1, tabs[1].1);
        assert_eq!(window.hit(center(tabs[0].1), &sheet), Some(Hit::Tab(0)));
        window.frame.minimized = true;
        assert!(window.tab_boxes(&sheet).is_empty());
    }

    #[test]
    fn says_why_a_skill_was_refused() {
        let ids = |skill, btype, cause, skill_fail| refusal_lines(skill, btype, cause, skill_fail).into_iter().map(|(id, _)| id).collect::<Vec<_>>();
        // Failed: Basic Skill by what it was for, Warp Portal's own, else the general line only with /skillfail.
        assert_eq!((ids(1, 0, 0, true), ids(1, 7, 0, true), ids(1, 8, 0, true)), (vec![159], vec![383, 1304], vec![1304]));
        assert_eq!((ids(27, 0, 0, true), ids(5, 0, 0, true), ids(5, 0, 0, false)), (vec![214], vec![204], vec![]));
        // The cooldown names the skill, only with /skillfail, never for 6552 or 6601.
        assert_eq!(refusal_lines(19, 0, 4, true), [(1460, Fill::Skill)]);
        assert!(refusal_lines(19, 0, 4, false).is_empty() && refusal_lines(6552, 0, 4, true).is_empty() && refusal_lines(6601, 0, 4, true).is_empty());
        // "Skill has failed." only with /skillfail; the rest always.
        assert_eq!((ids(19, 0, 10, true), ids(19, 0, 10, false)), (vec![285], vec![]));
        assert_eq!((ids(19, 0, 1, false), ids(19, 0, 13, false), ids(19, 0, 83, false)), (vec![202], vec![1398], vec![1954]));
        assert_eq!(
            (ids(19, 0, 25, true), ids(19, 0, 37, true), ids(19, 0, 102, true), ids(19, 0, 110, true)),
            (vec![1864], vec![1439], vec![3957], vec![3965])
        );
        assert_eq!((ids(19, 0, 111, true), ids(19, 0, 115, true)), (vec![3968], vec![4193]));
        assert_eq!(refusal_lines(19, 7, 116, true), [(4377, Fill::Number(7))]);
        assert_eq!(refusal_lines(19, 7, 117, true), [(4377, Fill::Number(7))]);
        // The detail fills some, names the item for others, or is the message itself; unknown causes say nothing.
        assert_eq!(refusal_lines(19, 5, 71, true), [(1536, Fill::ItemCount(5))]);
        assert_eq!(refusal_lines(19, 3, 90, true), [(2997, Fill::Number(3))]);
        assert_eq!((ids(19, 731, 86, true), ids(19, 0, 38, true), ids(19, 0, 200, true)), (vec![731], vec![], vec![]));
    }
}
