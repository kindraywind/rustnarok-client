//! The guild window (Alt+G): its tabs, the guild's info, its members and its notice.

use std::collections::HashMap;

use roclient::grf::Grf;
use roclient::net::{GuildInfo, GuildMember};
use roclient::text::Font;

use crate::render::{Gpu, Texture};
use crate::window::{self, Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI, contains};

// ponytail: 400x317 with six tabs over a 247px page and a 29px footer; calibrate against the real client.
const WIDTH: f32 = 400.0;
const HEIGHT: f32 = 317.0;
const TAB: [f32; 2] = [64.0, 23.0];
const PAGE_TOP: f32 = TITLE_HEIGHT + TAB[1] + 1.0;
const FOOTER: f32 = 29.0;
const LINE: f32 = 20.0;
const POSITION: [f32; 2] = [180.0, 120.0];
/// The tabs' labels: info, members, positions, skills, expel list, notice.
const TABS: [usize; 6] = [340, 341, 342, 343, 344, 345];
/// Info labels: name, level, master, members, average level, castles, exp, tax points.
const LABELS: [usize; 8] = [328, 329, 330, 331, 332, 333, 335, 337];
/// Member columns: name, position, job, level.
const COLUMNS: [(usize, f32); 4] = [(407, 10.0), (503, 150.0), (504, 250.0), (408, 340.0)];
const GREY: [u8; 3] = [0x88, 0x88, 0x88];
const TAB_FILL: [u8; 4] = [0xE7, 0xEB, 0xF4, 0xFF];

pub struct Assets {
    footer: Texture,
    tab_fill: Texture,
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        Self { footer: gpu.load_texture(grf, &format!("{UI}basic_interface\\btnbar_mid2.bmp"), false), tab_fill: gpu.solid(TAB_FILL) }
    }
}

/// The player's guild.
#[derive(Default)]
pub struct Guild {
    pub id: u32,
    pub name: String,
    pub master: bool,
    pub info: Option<GuildInfo>,
    pub members: Vec<GuildMember>,
    pub positions: HashMap<u32, String>,
    pub notice: (String, String),
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(TitleHit),
    Tab(usize),
    Window,
}

pub struct GuildWindow {
    pub frame: Frame,
    pub tab: usize,
}

impl Default for GuildWindow {
    fn default() -> Self {
        Self { frame: Frame::new(POSITION, WIDTH), tab: 0 }
    }
}

impl GuildWindow {
    pub fn height(&self) -> f32 {
        HEIGHT
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let local = self.frame.local(cursor, HEIGHT)?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(Hit::Title(hit));
        }
        Some((0..TABS.len()).find(|&tab| contains([tab as f32 * TAB[0], TITLE_HEIGHT], TAB, local)).map_or(Hit::Window, Hit::Tab))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &self,
        layer: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        font: &Font,
        messages: &[String],
        guild: &Guild,
        job_names: &HashMap<u16, String>,
        pointer: [f32; 2],
    ) {
        if !self.frame.open {
            return;
        }
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        let at = |local: [f32; 2]| self.frame.at(local);
        self.frame.draw_title(layer, chrome, &guild.name, pointer);
        if self.frame.minimized {
            return;
        }
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

        let text = |layer: &mut Layer<'a>, value: String, place: [f32; 2], color: [u8; 3]| layer.texts.push((value, at(place), color));
        match self.tab {
            // Info: the guild's name, level, master, members, levels, castles, exp and tax points.
            0 => {
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
                    text(layer, format!("{} : {value}", message(*label)), [12.0, PAGE_TOP + 8.0 + row as f32 * LINE], [0, 0, 0]);
                }
            }
            // Members: name, position, job and level; those offline in grey.
            1 => {
                for (label, x) in COLUMNS {
                    text(layer, message(label).to_owned(), [x, PAGE_TOP + 4.0], [0, 0, 0]);
                }
                layer.sprites.push((&chrome.border, at([4.0, PAGE_TOP + LINE + 2.0]), [WIDTH - 8.0, 1.0]));
                for (row, member) in guild.members.iter().enumerate().take(10) {
                    let top = PAGE_TOP + LINE + 6.0 + row as f32 * LINE;
                    let color = if member.online { [0, 0, 0] } else { GREY };
                    let position = guild.positions.get(&member.position).cloned().unwrap_or_default();
                    let job = job_names.get(&member.job).cloned().unwrap_or_else(|| member.job.to_string());
                    for (value, (_, x)) in [member.name.clone(), position, job, member.level.to_string()].into_iter().zip(COLUMNS) {
                        text(layer, window::fit(font, &value, 95.0), [x, top], color);
                    }
                }
            }
            // The notice: its subject, then its text.
            5 => {
                text(layer, format!("{} : {}", message(515), guild.notice.0), [12.0, PAGE_TOP + 8.0], [0, 0, 0]);
                text(layer, message(516).to_owned(), [12.0, PAGE_TOP + 8.0 + LINE], [0, 0, 0]);
                for (row, line) in window::wrap(font, &guild.notice.1, WIDTH - 30.0, 8).into_iter().enumerate() {
                    text(layer, line, [20.0, PAGE_TOP + 8.0 + (row + 2) as f32 * LINE], [0, 0, 0]);
                }
            }
            // ponytail: positions, guild skills and the expel list aren't made yet.
            _ => {}
        }
        layer.sprites.push((&assets.footer, at([0.0, HEIGHT - FOOTER]), [WIDTH, FOOTER]));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_tabs_across_the_top() {
        let mut window = GuildWindow::default();
        window.frame.open = true;
        let at = |local: [f32; 2]| window.frame.at(local);
        assert_eq!(window.hit(at([TAB[0] * 1.5, TITLE_HEIGHT + 5.0])), Some(Hit::Tab(1)));
        assert_eq!(window.hit(at([TAB[0] * 5.5, TITLE_HEIGHT + 5.0])), Some(Hit::Tab(5)));
        assert_eq!(window.hit(at([200.0, 200.0])), Some(Hit::Window));
    }
}
