//! The guild window (Alt+G): its tabs, the guild's info with its emblem, its members, positions, skills, the expel
//! list and the notice. Also guild emblems as the server sends them.

use std::collections::HashMap;
use std::io::Read;

use image::RgbaImage;
use rustnarok_client::grf::Grf;
use rustnarok_client::lua::SkillInfo;
use rustnarok_client::messages;
use rustnarok_client::net::{GuildInfo, GuildMember, GuildPosition, GuildRelation, Skill};
use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};
use crate::skills;
use crate::window::{self, Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI, contains, offset};

// ponytail: 400x317 with six tabs over a 247px page and a 29px footer; calibrate against the real client.
const WIDTH: f32 = 400.0;
const HEIGHT: f32 = 317.0;
const TAB: [f32; 2] = [64.0, 23.0];
const PAGE_TOP: f32 = TITLE_HEIGHT + TAB[1] + 1.0;
const FOOTER: f32 = 29.0;
const LINE: f32 = 20.0;
/// Rows a list (members, positions, the expelled) shows at once; the wheel scrolls it.
const ROWS: usize = 11;
const POSITION: [f32; 2] = [180.0, 120.0];
/// The tabs' labels: info, members, positions, skills, expel list, notice.
const TABS: [usize; 6] = [340, 341, 342, 343, 344, 345];
const MEMBERS: usize = 1;
const POSITIONS: usize = 2;
const SKILLS: usize = 3;
const EXPELLED: usize = 4;
/// Info labels: name, level, master, members, average level, castles, exp, tax points.
const LABELS: [usize; 8] = [328, 329, 330, 331, 332, 333, 335, 337];
// ponytail: the emblem's frame top left of the info page, the labels right of it.
const EMBLEM: [f32; 2] = [12.0, PAGE_TOP + 8.0];
pub const EMBLEM_SIZE: [f32; 2] = [24.0, 24.0];
/// The guardians' monster IDs: they carry their castle's guild emblem too.
const GUARDIANS: [u16; 3] = [1285, 1286, 1287];

/// Whether a unit carries its guild's emblem over its head on a siege map: other players in a guild and the guardians,
/// not while out of sight.
pub fn wears_head_emblem(player: bool, job: u16, guild: u32, cloaked: bool, ours: bool) -> bool {
    !ours && guild != 0 && !cloaked && (player || GUARDIANS.contains(&job))
}

/// The emblem's box over a unit (left, top, right, bottom): `floor(10·W/640)·s` left of and `floor(100·H/480)·s`
/// above its feet on screen, `s` its sprite scale; it doesn't grow with the zoom.
pub fn head_emblem_box(feet: [f32; 2], screen: [f32; 2], scale: f32) -> [f32; 4] {
    let left = feet[0] - (10.0 * screen[0] / 640.0).floor() * scale;
    let top = feet[1] - (100.0 * screen[1] / 480.0).floor() * scale;
    [left, top, left + EMBLEM_SIZE[0], top + EMBLEM_SIZE[1]]
}
const INFO_LEFT: f32 = 52.0;
/// Member columns: name, position, job, level.
const COLUMNS: [(usize, f32); 4] = [(407, 10.0), (503, 150.0), (504, 250.0), (408, 340.0)];
/// Position columns: rank, title, invite, expel, storage, tax %.
const POSITION_COLUMNS: [(usize, f32); 6] = [(510, 10.0), (511, 50.0), (512, 190.0), (513, 240.0), (2499, 290.0), (514, 345.0)];
/// What the invite, expel and storage columns check.
const PERMISSIONS: [u32; 3] = [GuildPosition::INVITE, GuildPosition::EXPEL, GuildPosition::STORAGE];
/// Expel list columns: name, reason.
const EXPEL_COLUMNS: [(usize, f32); 2] = [(407, 10.0), (462, 150.0)];
/// Guild skills keep their place in a grid, by number (GD_APPROVAL is 10000), six to a row.
const SKILL_BASE: u16 = 10000;
const SKILL_COLUMNS: usize = 6;
const SKILL_LEFT: f32 = (WIDTH - SKILL_COLUMNS as f32 * skills::CELL[0]) / 2.0;
const SKILL_POINTS: usize = 284;
const GREY: [u8; 3] = [0x88, 0x88, 0x88];
const TAB_FILL: [u8; 4] = [0xE7, 0xEB, 0xF4, 0xFF];
/// The info page's allies (left) and antagonists (right), under their headings: three rows each, rathena's default
/// max_guild_alliance.
// ponytail: where the lists sit is an estimate.
const RELATION_HEADINGS: [usize; 2] = [338, 339];
const RELATION_TOP: f32 = PAGE_TOP + 166.0;
const RELATION_ROW: f32 = 18.0;
const RELATION_LEFT: [f32; 2] = [12.0, 206.0];
const RELATION_WIDTH: f32 = 182.0;
const RELATION_ROWS: usize = 3;
/// The menu's hover color, on a relation the master can end.
const HOVER: [u8; 4] = [0x73, 0x9C, 0xEF, 0xFF];
/// An emblem's BMP is under 2 KB; more than this is no emblem.
const EMBLEM_BYTES: u64 = 64 * 1024;

pub struct Assets {
    footer: Texture,
    tab_fill: Texture,
    /// The emblem's frame, and the position page's checkboxes (off, on).
    frame: Texture,
    check: [Texture; 2],
    hover: Texture,
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |name: &str| gpu.load_texture(grf, &format!("{UI}{name}.bmp"), false);
        Self {
            footer: image("basic_interface\\btnbar_mid2"),
            tab_fill: gpu.solid(TAB_FILL),
            frame: image("emblem_frame"),
            check: ["checkbox_0", "checkbox_1"].map(image),
            hover: gpu.solid(HOVER),
        }
    }
}

/// The player's guild.
#[derive(Default)]
pub struct Guild {
    pub id: u32,
    pub name: String,
    pub master: bool,
    /// The emblem's version (0: none).
    pub emblem: u32,
    pub info: Option<GuildInfo>,
    pub members: Vec<GuildMember>,
    pub positions: HashMap<u32, String>,
    /// Positions' permissions and tax.
    pub position_info: Vec<GuildPosition>,
    pub skills: Vec<Skill>,
    pub skill_points: u16,
    /// Who was expelled, and why.
    pub expelled: Vec<(String, String)>,
    pub notice: (String, String),
    /// Allied guilds and antagonists.
    pub relations: Vec<GuildRelation>,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(TitleHit),
    Tab(usize),
    /// A guild skill's box, or its level up button.
    Skill(u16),
    LevelUp(u16),
    /// An ally or an antagonist on the info page (the master's to end): its guild ID, and whether it's an antagonist.
    Relation(u32, bool),
    Window,
}

/// The info page's columns: allies, then antagonists, the first rows of each.
fn columns(relations: &[GuildRelation]) -> [Vec<&GuildRelation>; 2] {
    [false, true].map(|antagonist| relations.iter().filter(|relation| relation.antagonist == antagonist).take(RELATION_ROWS).collect())
}

/// A relation row's corner and size.
fn relation_box(column: usize, row: usize) -> ([f32; 2], [f32; 2]) {
    ([RELATION_LEFT[column], RELATION_TOP + (row + 1) as f32 * RELATION_ROW], [RELATION_WIDTH, RELATION_ROW])
}

/// An alliance request's result: its line, and whether it's good news. rathena's numbers are told as they come.
// ponytail: result 5 (only masters may) has no line of its own; 1717 is an estimate.
pub fn ally_result(result: u8) -> (usize, bool) {
    if result <= 4 { (394 + usize::from(result), result == 2) } else { (1717, false) }
}

/// Setting an antagonist: its line, and whether it's good news.
pub fn antagonist_result(result: u8) -> (usize, bool) {
    (495 + usize::from(result.min(2)), result == 0)
}

/// Disbanding: its line, and whether it's good news.
pub fn disband_result(result: u32) -> (usize, bool) {
    match result {
        0 => (400, true),
        1 => (401, false),
        2 => (402, false),
        _ => (366, false),
    }
}

pub struct GuildWindow {
    pub frame: Frame,
    pub tab: usize,
    /// Rows of the open list scrolled past.
    scroll: usize,
}

impl Default for GuildWindow {
    fn default() -> Self {
        Self { frame: Frame::new(POSITION, WIDTH), tab: 0, scroll: 0 }
    }
}

/// Where a guild skill's box sits.
fn skill_corner(id: u16) -> [f32; 2] {
    let place = usize::from(id.saturating_sub(SKILL_BASE));
    let [column, row] = [place % SKILL_COLUMNS, place / SKILL_COLUMNS];
    [SKILL_LEFT + column as f32 * skills::CELL[0] + (skills::CELL[0] - skills::BOX) / 2.0, PAGE_TOP + 6.0 + row as f32 * skills::CELL[1]]
}

/// A guild emblem from the server: a zlib-compressed BMP, its magenta keyed out.
pub fn decode_emblem(data: &[u8]) -> Option<RgbaImage> {
    let mut bmp = Vec::new();
    flate2::read::ZlibDecoder::new(data).take(EMBLEM_BYTES).read_to_end(&mut bmp).ok()?;
    let image = image::load_from_memory_with_format(&bmp, image::ImageFormat::Bmp).ok()?;
    Some(crate::render::key_out(image.to_rgba8()))
}

impl GuildWindow {
    pub fn height(&self) -> f32 {
        HEIGHT
    }

    pub fn show_tab(&mut self, tab: usize) {
        self.tab = tab;
        self.scroll = 0;
    }

    pub fn hit(&self, cursor: [f32; 2], guild: &Guild) -> Option<Hit> {
        let local = self.frame.local(cursor, HEIGHT)?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(Hit::Title(hit));
        }
        if let Some(tab) = (0..TABS.len()).find(|&tab| contains([tab as f32 * TAB[0], TITLE_HEIGHT], TAB, local)) {
            return Some(Hit::Tab(tab));
        }
        if self.tab == 0 && guild.master {
            for (column, relations) in columns(&guild.relations).into_iter().enumerate() {
                for (row, relation) in relations.into_iter().enumerate() {
                    let (corner, size) = relation_box(column, row);
                    if contains(corner, size, local) {
                        return Some(Hit::Relation(relation.id, relation.antagonist));
                    }
                }
            }
        }
        if self.tab == SKILLS {
            for skill in &guild.skills {
                let corner = skill_corner(skill.id);
                if guild.skill_points > 0 && skill.upgradable && contains(offset(corner, skills::UP), [24.0, 24.0], local) {
                    return Some(Hit::LevelUp(skill.id));
                }
                if contains(corner, [skills::BOX; 2], local) {
                    return Some(Hit::Skill(skill.id));
                }
            }
        }
        Some(Hit::Window)
    }

    /// The relations the master can click on the info page, named after their guilds.
    pub fn relation_boxes(&self, guild: &Guild) -> Vec<(String, [f32; 4])> {
        if !self.frame.open || self.frame.minimized || self.tab != 0 || !guild.master {
            return Vec::new();
        }
        let rows = columns(&guild.relations)
            .into_iter()
            .enumerate()
            .flat_map(|(column, relations)| relations.into_iter().enumerate().map(move |(row, relation)| (column, row, relation)));
        rows.map(|(column, row, relation)| {
            let (corner, size) = relation_box(column, row);
            let [x, y] = self.frame.at(corner);
            (relation.name.clone(), [x, y, size[0], size[1]])
        })
        .collect()
    }

    /// The wheel scrolls the open list.
    pub fn scroll_by(&mut self, rows: i32, guild: &Guild) {
        let total = match self.tab {
            MEMBERS => guild.members.len(),
            POSITIONS => guild.position_info.len(),
            EXPELLED => guild.expelled.len(),
            _ => 0,
        };
        self.scroll = (self.scroll as i32 + rows).clamp(0, total.saturating_sub(ROWS) as i32) as usize;
    }

    /// The skill icons the open page shows, to load before drawing.
    pub fn icons(&self, guild: &Guild, info: &HashMap<u16, SkillInfo>) -> Vec<String> {
        if !self.frame.open || self.tab != SKILLS {
            return Vec::new();
        }
        guild.skills.iter().filter_map(|skill| info.get(&skill.id).map(skills::icon_path)).collect()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &self,
        layer: &mut Layer<'a>,
        tooltip: &mut Layer<'a>,
        chrome: &'a Chrome,
        (assets, skill_assets): (&'a Assets, &'a skills::Assets),
        icons: &'a HashMap<String, Option<Texture>>,
        font: &Font,
        messages: &[String],
        guild: &Guild,
        (job_names, skill_info): (&HashMap<u16, String>, &HashMap<u16, SkillInfo>),
        emblem: Option<&'a Texture>,
        pointer: ([f32; 2], bool),
    ) {
        if !self.frame.open {
            return;
        }
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        let at = |local: [f32; 2]| self.frame.at(local);
        self.frame.draw_title(layer, chrome, &guild.name, pointer.0);
        if self.frame.minimized {
            return;
        }
        let hit = self.hit(pointer.0, guild);
        layer.sprites.push((&chrome.white, at([0.0, TITLE_HEIGHT]), [WIDTH, HEIGHT - TITLE_HEIGHT - FOOTER]));
        for x in [0.0, WIDTH - 1.0] {
            layer.sprites.push((&chrome.border, at([x, TITLE_HEIGHT]), [1.0, HEIGHT - TITLE_HEIGHT - FOOTER]));
        }
        // Tabs: the open one joins the page, the others sit back.
        for (tab, &label) in TABS.iter().enumerate() {
            let corner = at([tab as f32 * TAB[0], TITLE_HEIGHT]);
            if tab != self.tab {
                layer.sprites.push((&assets.tab_fill, corner, TAB));
            }
            layer.sprites.push((&chrome.border, [corner[0] + TAB[0] - 1.0, corner[1]], [1.0, TAB[1]]));
            let text = window::fit(font, message(label), TAB[0] - 6.0);
            let x = corner[0] + ((TAB[0] - font.width(&text)) / 2.0).floor();
            layer.texts.push((text, [x, corner[1] + ((TAB[1] - font.line_height()) / 2.0).floor()], if tab == self.tab { [0, 0, 0] } else { GREY }));
        }
        layer.sprites.push((&chrome.border, at([0.0, TITLE_HEIGHT + TAB[1]]), [WIDTH, 1.0]));
        layer.sprites.push((&assets.footer, at([0.0, HEIGHT - FOOTER]), [WIDTH, FOOTER]));

        let text = |layer: &mut Layer<'a>, value: String, place: [f32; 2], color: [u8; 3]| layer.texts.push((value, at(place), color));
        // A list's column labels over a rule, and the top of each row shown.
        let header = |layer: &mut Layer<'a>, columns: &[(usize, f32)]| {
            for &(label, x) in columns {
                layer.texts.push((message(label).to_owned(), at([x, PAGE_TOP + 4.0]), [0, 0, 0]));
            }
            layer.sprites.push((&chrome.border, at([4.0, PAGE_TOP + LINE + 2.0]), [WIDTH - 8.0, 1.0]));
        };
        let row_top = |row: usize| PAGE_TOP + LINE + 6.0 + row as f32 * LINE;
        match self.tab {
            // Info: the emblem, then the guild's name, level, master, members, levels, castles, exp and tax points.
            0 => {
                layer.sprites.push((&assets.frame, at(EMBLEM), assets.frame.size));
                if let Some(emblem) = emblem {
                    layer.sprites.push((emblem, at(offset(EMBLEM, [2.0, 2.0])), EMBLEM_SIZE));
                }
                let info = guild.info.clone().unwrap_or_default();
                let values = [
                    guild.name.clone(),
                    info.level.to_string(),
                    info.master,
                    format!("{} / {}", info.members, info.max_members),
                    info.average_level.to_string(),
                    info.territory,
                    format!("{} / {}", info.exp, info.next_exp),
                    info.tax_points.to_string(),
                ];
                for (row, (label, value)) in LABELS.iter().zip(values).enumerate() {
                    text(layer, format!("{} : {value}", message(*label)), [INFO_LEFT, PAGE_TOP + 8.0 + row as f32 * LINE], [0, 0, 0]);
                }
                // Allies and antagonists, each under its heading; the master's pointer marks the one it would end.
                for (column, relations) in columns(&guild.relations).into_iter().enumerate() {
                    text(layer, message(RELATION_HEADINGS[column]).to_owned(), [RELATION_LEFT[column], RELATION_TOP], [0, 0, 0]);
                    layer.sprites.push((&chrome.border, at([RELATION_LEFT[column], RELATION_TOP + RELATION_ROW - 2.0]), [RELATION_WIDTH, 1.0]));
                    for (row, relation) in relations.into_iter().enumerate() {
                        let (corner, size) = relation_box(column, row);
                        if hit == Some(Hit::Relation(relation.id, relation.antagonist)) {
                            layer.sprites.push((&assets.hover, at(corner), size));
                        }
                        text(layer, window::fit(font, &relation.name, RELATION_WIDTH - 12.0), offset(corner, [6.0, 2.0]), [0, 0, 0]);
                    }
                }
            }
            // Members: name, position, job and level; those offline in grey.
            MEMBERS => {
                header(layer, &COLUMNS);
                for (row, member) in guild.members.iter().skip(self.scroll).take(ROWS).enumerate() {
                    let color = if member.online { [0, 0, 0] } else { GREY };
                    let position = guild.positions.get(&member.position).cloned().unwrap_or_default();
                    let job = job_names.get(&member.job).cloned().unwrap_or_else(|| member.job.to_string());
                    for (value, (_, x)) in [member.name.clone(), position, job, member.level.to_string()].into_iter().zip(COLUMNS) {
                        text(layer, window::fit(font, &value, 95.0), [x, row_top(row)], color);
                    }
                }
            }
            // Positions: rank, title, what its members may do, and the tax on their exp.
            POSITIONS => {
                header(layer, &POSITION_COLUMNS);
                for (row, position) in guild.position_info.iter().skip(self.scroll).take(ROWS).enumerate() {
                    let top = row_top(row);
                    let title = guild.positions.get(&position.id).map_or("", String::as_str);
                    text(layer, (position.id + 1).to_string(), [POSITION_COLUMNS[0].1, top], [0, 0, 0]);
                    text(layer, window::fit(font, title, POSITION_COLUMNS[2].1 - POSITION_COLUMNS[1].1 - 6.0), [POSITION_COLUMNS[1].1, top], [0, 0, 0]);
                    for (permission, &(label, x)) in PERMISSIONS.iter().zip(&POSITION_COLUMNS[2..5]) {
                        let check = &assets.check[usize::from(position.mode & permission != 0)];
                        let x = x + ((font.width(message(label)) - check.size[0]) / 2.0).floor();
                        layer.sprites.push((check, at([x, top + 1.0]), check.size));
                    }
                    text(layer, position.tax.to_string(), [POSITION_COLUMNS[5].1, top], [0, 0, 0]);
                }
            }
            // Skills where they sit, the master's level up buttons, and the points left on the footer.
            SKILLS => {
                for skill in &guild.skills {
                    let corner = skill_corner(skill.id);
                    let info = skill_info.get(&skill.id);
                    let name = info.map_or_else(|| skill.id.to_string(), |info| info.name.clone());
                    skills::draw_box(layer, skill_assets, icons, font, at(corner), info, &name, skill.level);
                    if guild.skill_points > 0 && skill.upgradable {
                        let state = if hit == Some(Hit::LevelUp(skill.id)) { 1 + usize::from(pointer.1) } else { 0 };
                        let up = &skill_assets.up[state];
                        layer.sprites.push((up, at(offset(corner, skills::UP)), up.size));
                    }
                    if hit == Some(Hit::Skill(skill.id)) {
                        let max = info.map_or(0, |info| info.max_level);
                        window::tooltip(tooltip, chrome, font, &format!("{name} Lv {}/{max}", skill.level), offset(pointer.0, [12.0, 12.0]), [255; 3]);
                    }
                }
                let points = messages::fill(message(SKILL_POINTS), &[&guild.skill_points]);
                text(layer, points, [12.0, HEIGHT - FOOTER + ((FOOTER - font.line_height()) / 2.0).floor()], [0, 0, 0]);
            }
            // The expel list: who, and why.
            EXPELLED => {
                header(layer, &EXPEL_COLUMNS);
                for (row, (name, reason)) in guild.expelled.iter().skip(self.scroll).take(ROWS).enumerate() {
                    let [(_, name_x), (_, reason_x)] = EXPEL_COLUMNS;
                    text(layer, window::fit(font, name, reason_x - name_x - 6.0), [name_x, row_top(row)], [0, 0, 0]);
                    text(layer, window::fit(font, reason, WIDTH - reason_x - 8.0), [reason_x, row_top(row)], [0, 0, 0]);
                }
            }
            // The notice: its subject, then its text.
            _ => {
                text(layer, format!("{} : {}", message(515), guild.notice.0), [12.0, PAGE_TOP + 8.0], [0, 0, 0]);
                text(layer, message(516).to_owned(), [12.0, PAGE_TOP + 8.0 + LINE], [0, 0, 0]);
                for (row, line) in window::wrap(font, &guild.notice.1, WIDTH - 30.0, 8).into_iter().enumerate() {
                    text(layer, line, [20.0, PAGE_TOP + 8.0 + (row + 2) as f32 * LINE], [0, 0, 0]);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_tabs_across_the_top() {
        let mut window = GuildWindow::default();
        window.frame.open = true;
        let guild = Guild::default();
        let at = |local: [f32; 2]| window.frame.at(local);
        assert_eq!(window.hit(at([TAB[0] * 1.5, TITLE_HEIGHT + 5.0]), &guild), Some(Hit::Tab(1)));
        assert_eq!(window.hit(at([TAB[0] * 5.5, TITLE_HEIGHT + 5.0]), &guild), Some(Hit::Tab(5)));
        assert_eq!(window.hit(at([200.0, 200.0]), &guild), Some(Hit::Window));
    }

    #[test]
    fn scrolls_the_open_list() {
        let mut window = GuildWindow::default();
        let guild = Guild { position_info: vec![GuildPosition::default(); 20], ..Default::default() };
        window.show_tab(POSITIONS);
        window.scroll_by(50, &guild);
        assert_eq!(window.scroll, 20 - ROWS);
        window.show_tab(EXPELLED);
        window.scroll_by(5, &guild);
        assert_eq!(window.scroll, 0, "nobody expelled, nothing to scroll");
    }

    #[test]
    fn lists_allies_and_antagonists() {
        let relation = |antagonist: bool, id: u32| GuildRelation { antagonist, id, name: format!("G{id}") };
        let relations = vec![relation(false, 1), relation(true, 2), relation(false, 3), relation(false, 4), relation(false, 5)];
        let [allies, antagonists] = columns(&relations);
        assert_eq!(allies.iter().map(|relation| relation.id).collect::<Vec<_>>(), [1, 3, 4]);
        assert_eq!(antagonists.iter().map(|relation| relation.id).collect::<Vec<_>>(), [2]);
        let mut window = GuildWindow::default();
        window.frame.open = true;
        let mut guild = Guild { relations, ..Default::default() };
        let (corner, _) = relation_box(1, 0);
        let point = window.frame.at(offset(corner, [20.0, 5.0]));
        // Only the master may end one.
        assert_eq!(window.hit(point, &guild), Some(Hit::Window));
        assert!(window.relation_boxes(&guild).is_empty());
        guild.master = true;
        assert_eq!(window.hit(point, &guild), Some(Hit::Relation(2, true)));
        assert_eq!(window.relation_boxes(&guild).iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>(), ["G1", "G3", "G4", "G2"]);
        // The last row still fits on the page.
        assert!(relation_box(0, RELATION_ROWS - 1).0[1] + RELATION_ROW <= HEIGHT - FOOTER);
        assert_eq!([0, 2, 4, 5].map(ally_result), [(394, false), (396, true), (398, false), (1717, false)]);
        assert_eq!([0, 2, 3].map(antagonist_result), [(495, true), (497, false), (497, false)]);
        assert_eq!([0, 1, 2, 9].map(disband_result), [(400, true), (401, false), (402, false), (366, false)]);
    }

    #[test]
    fn places_guild_skills_by_number() {
        assert_eq!(skill_corner(10000), [SKILL_LEFT + 17.0, PAGE_TOP + 6.0]);
        assert_eq!(skill_corner(10007), [SKILL_LEFT + 62.0 + 17.0, PAGE_TOP + 6.0 + 58.0]);
        // The last of the 20 still fits on the page.
        assert!(skill_corner(10019)[1] + skills::CELL[1] <= HEIGHT - FOOTER);
    }

    #[test]
    fn decodes_emblems_keying_out_magenta() {
        use std::io::Write;
        let mut image = image::RgbImage::from_pixel(24, 24, image::Rgb([0x20, 0x40, 0x80]));
        image.put_pixel(0, 0, image::Rgb([0xFF, 0x00, 0xFF]));
        let mut bmp = std::io::Cursor::new(Vec::new());
        image.write_to(&mut bmp, image::ImageFormat::Bmp).unwrap();
        let mut zipped = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        zipped.write_all(bmp.get_ref()).unwrap();
        let emblem = decode_emblem(&zipped.finish().unwrap()).unwrap();
        assert_eq!(emblem.dimensions(), (24, 24));
        assert_eq!(emblem.get_pixel(0, 0).0, [0; 4]);
        assert_eq!(emblem.get_pixel(5, 5).0, [0x20, 0x40, 0x80, 0xFF]);
        assert!(decode_emblem(b"not zlib").is_none());
    }

    #[test]
    fn places_emblems_over_heads() {
        assert_eq!(head_emblem_box([500.0, 400.0], [1024.0, 768.0], 1.0), [484.0, 240.0, 508.0, 264.0]);
        assert_eq!(head_emblem_box([500.0, 400.0], [1024.0, 768.0], 0.8), [487.2, 272.0, 511.2, 296.0]);
        assert_eq!(head_emblem_box([500.0, 400.0], [1280.0, 720.0], 1.0), [480.0, 250.0, 504.0, 274.0]);
        assert_eq!(head_emblem_box([500.0, 400.0], [1280.0, 720.0], 0.8), [484.0, 280.0, 508.0, 304.0]);
        assert!(wears_head_emblem(true, 0, 5, false, false));
        assert!(!wears_head_emblem(true, 0, 5, false, true), "not ourselves");
        assert!(!wears_head_emblem(true, 0, 0, false, false), "no guild");
        assert!(!wears_head_emblem(true, 0, 5, true, false), "hidden");
        assert!(wears_head_emblem(false, 1286, 5, false, false), "a guardian");
        assert!(!wears_head_emblem(false, 1002, 5, false, false), "another monster");
    }
}
