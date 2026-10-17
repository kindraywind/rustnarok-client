//! The skill window (Alt+S): the job's skills where the client's skill tree puts them, a tab for each job of the
//! line, their levels, and the points left to put in.

use std::collections::HashMap;

use roclient::grf::Grf;
use roclient::lua::{SkillInfo, SkillTrees};
use roclient::messages;
use roclient::net::Skill;
use roclient::text::Font;

use crate::render::{Gpu, Texture};
use crate::window::{self, Chrome, Clicks, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI, contains, offset};

// ponytail: a grid of 28px dashed boxes 62x58 apart; calibrate against the real client.
const COLUMNS: usize = 7;
const ROWS: usize = 6;
const CELL: [f32; 2] = [62.0, 58.0];
const BOX: f32 = 28.0;
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
    up: [Texture; 3],
    /// The bottom bar's left end, middle and right end.
    bar: [Texture; 3],
    /// Whitens skills not learned.
    veil: Texture,
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |name: &str| gpu.load_texture(grf, &format!("{UI}basic_interface\\{name}.bmp"), false);
        Self {
            slot: image("no_skill"),
            up: ["skill_up_a", "skill_up_b", "skill_up_c"].map(image),
            bar: ["skill_botbar_left", "skill_botbar_mid", "skill_botbar_right"].map(image),
            veil: gpu.solid([255, 255, 255, 150]),
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(TitleHit),
    Tab(usize),
    Skill(u16),
    LevelUp(u16),
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
}

impl Default for SkillWindow {
    fn default() -> Self {
        Self { frame: Frame::new(DEFAULT_POSITION, WIDTH), tab: usize::MAX, clicks: Clicks::default() }
    }
}

/// Where a skill's box sits in the window.
fn box_corner(place: usize) -> [f32; 2] {
    let [column, row] = [place % COLUMNS, place / COLUMNS];
    [SIDE + column as f32 * CELL[0] + (CELL[0] - BOX) / 2.0, GRID_TOP + row as f32 * CELL[1] + 6.0]
}

pub fn icon_path(info: &SkillInfo) -> String {
    format!("{UI}item\\{}.bmp", info.resource.to_lowercase())
}

impl SkillWindow {
    pub fn height(&self) -> f32 {
        HEIGHT
    }

    /// The tab shown: the player's own job's unless another was picked.
    fn tab(&self, jobs: &[u16]) -> usize {
        self.tab.min(jobs.len().saturating_sub(1))
    }

    /// The skills in the open tab, by place.
    fn tree<'t>(&self, trees: &'t SkillTrees, jobs: &[u16]) -> &'t [(usize, u16)] {
        jobs.get(self.tab(jobs)).and_then(|job| trees.trees.get(job)).map_or(&[], Vec::as_slice)
    }

    pub fn hit(&self, cursor: [f32; 2], trees: &SkillTrees, jobs: &[u16], skills: &[Skill], points: i64) -> Option<Hit> {
        let local = self.frame.local(cursor, HEIGHT)?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(Hit::Title(hit));
        }
        if local[1] < TITLE_HEIGHT + TAB_HEIGHT {
            return Some(
                (0..jobs.len())
                    .find(|&tab| contains([SIDE + tab as f32 * TAB_WIDTH, TITLE_HEIGHT], [TAB_WIDTH, TAB_HEIGHT], local))
                    .map_or(Hit::Window, Hit::Tab),
            );
        }
        for &(place, id) in self.tree(trees, jobs) {
            let corner = box_corner(place);
            let upgradable = points > 0 && skills.iter().any(|skill| skill.id == id && skill.upgradable);
            if upgradable && contains(offset(corner, [BOX + 1.0, 2.0]), [24.0, 24.0], local) {
                return Some(Hit::LevelUp(id));
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
            Hit::Window => {}
        }
        None
    }

    /// The icons the open tab shows, to load before drawing.
    pub fn icons(&self, trees: &SkillTrees, jobs: &[u16], info: &HashMap<u16, SkillInfo>) -> Vec<String> {
        if !self.frame.open {
            return Vec::new();
        }
        self.tree(trees, jobs).iter().filter_map(|(_, id)| info.get(id).map(icon_path)).collect()
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
            let name = job_names.get(job).map_or_else(|| job.to_string(), Clone::clone);
            let place = offset(corner, [((size[0] - font.width(&name)) / 2.0).floor(), 3.0]);
            layer.texts.push((name, place, if tab == open { [0, 0, 0] } else { GREY }));
        }
        layer.sprites.push((&chrome.border, at([0.0, TITLE_HEIGHT + TAB_HEIGHT]), [WIDTH, 1.0]));

        for &(place, id) in self.tree(trees, jobs) {
            let corner = box_corner(place);
            let known = skills.iter().find(|skill| skill.id == id);
            let level = known.map_or(0, |skill| skill.level);
            let skill_info = info.get(&id);
            layer.sprites.push((&assets.slot, at(offset(corner, [2.0, 2.0])), assets.slot.size));
            if let Some(Some(icon)) = skill_info.map(icon_path).and_then(|path| icons.get(&path)) {
                layer.sprites.push((icon, at(offset(corner, [2.0, 2.0])), icon.size));
                if level == 0 {
                    layer.sprites.push((&assets.veil, at(offset(corner, [2.0, 2.0])), icon.size));
                }
            }
            if level > 0 {
                // The level in the box's corner, white on a dark outline.
                let text = level.to_string();
                let place = at(offset(corner, [BOX - font.width(&text) - 1.0, BOX - font.line_height() + 2.0]));
                for nudge in [[-1.0, 0.0], [1.0, 0.0], [0.0, -1.0], [0.0, 1.0]] {
                    layer.texts.push((text.clone(), offset(place, nudge), [0x2F, 0x2F, 0x2F]));
                }
                layer.texts.push((text, place, [255; 3]));
            }
            let name = skill_info.map_or_else(|| id.to_string(), |info| info.name.clone());
            let shown = window::fit(font, &name, CELL[0] - 2.0);
            let name_place = at([corner[0] + (BOX - font.width(&shown)) / 2.0, corner[1] + BOX + 2.0].map(f32::floor));
            layer.texts.push((shown, name_place, if level > 0 { [0, 0, 0] } else { GREY }));
            if points > 0 && known.is_some_and(|skill| skill.upgradable) {
                let state = match hit {
                    Some(Hit::LevelUp(target)) if target == id => 1 + usize::from(pointer.1),
                    _ => 0,
                };
                layer.sprites.push((&assets.up[state], at(offset(corner, [BOX + 1.0, 2.0])), assets.up[state].size));
            }
            if hit == Some(Hit::Skill(id)) {
                let max = skill_info.map_or(0, |info| info.max_level);
                window::tooltip(tooltip, chrome, font, &format!("{name} Lv {level}/{max}"), offset(pointer.0, [12.0, 12.0]), [255; 3]);
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
    fn places_skills_seven_a_row() {
        assert_eq!(box_corner(0), [SIDE + 17.0, GRID_TOP + 6.0]);
        assert_eq!(box_corner(8), [SIDE + 62.0 + 17.0, GRID_TOP + 58.0 + 6.0]);
    }
}
