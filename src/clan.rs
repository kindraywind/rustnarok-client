//! The clan window (Ctrl+G): the clan's picture, emblem, name, master, members online, territory, allies and
//! hostile clans.

use rustnarok_client::grf::Grf;

use crate::guild::EMBLEM_SIZE;
use crate::render::{Gpu, Texture};
use crate::window::{Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI, offset};

const TITLE: usize = 2355;
const EMBLEM_LABEL: usize = 2358;
/// Info labels: name, master, members, territory, ally, hostile.
// ponytail: which label sits where is an estimate; 2356 "Clan Level" and 2364 "Castles Owned" have nothing from
// rathena to show, and 333 names the territory row like the guild window.
const LABELS: [usize; 6] = [2357, 2362, 2363, 333, 2359, 2360];

// ponytail: 380x225 modeled on the guild window's info page, with the clan illustration on the left and the emblem
// and rows to its right; calibrate against the real client.
const WIDTH: f32 = 380.0;
const HEIGHT: f32 = 225.0;
const FOOTER: f32 = 29.0;
const LINE: f32 = 20.0;
const POSITION: [f32; 2] = [200.0, 140.0];
const ILLUST: [f32; 2] = [10.0, TITLE_HEIGHT + 10.0];
const INFO_LEFT: f32 = 156.0;
const EMBLEM: [f32; 2] = [INFO_LEFT + 90.0, TITLE_HEIGHT + 8.0];

pub struct Assets {
    footer: Texture,
    frame: Texture,
    // ponytail: by their numbers, emblem and picture N are clan N's; nothing in the data says so.
    emblems: [Texture; 9],
    illusts: [Texture; 8],
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |name: &str| gpu.load_texture(grf, &format!("{UI}{name}.bmp"), false);
        Self {
            footer: image("basic_interface\\btnbar_mid2"),
            frame: image("emblem_frame"),
            emblems: std::array::from_fn(|i| image(&format!("clan_system\\clan_emblem{i:02}"))),
            illusts: std::array::from_fn(|i| image(&format!("clan_system\\clan_illust{:02}", i + 1))),
        }
    }
}

/// The player's clan.
#[derive(Default)]
pub struct Clan {
    pub id: u32,
    pub name: String,
    pub master: String,
    pub map: String,
    pub online: i16,
    pub max: i16,
    pub allies: Vec<String>,
    pub hostile: Vec<String>,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(TitleHit),
    Window,
}

pub struct ClanWindow {
    pub frame: Frame,
}

impl Default for ClanWindow {
    fn default() -> Self {
        Self { frame: Frame::new(POSITION, WIDTH) }
    }
}

/// The six text values shown beside `LABELS`.
pub fn info_values(clan: &Clan) -> [String; 6] {
    [
        clan.name.clone(),
        clan.master.clone(),
        format!("{} / {}", clan.online, clan.max),
        clan.map.trim_end_matches(".gat").to_owned(),
        clan.allies.join(", "),
        clan.hostile.join(", "),
    ]
}

impl ClanWindow {
    pub fn height(&self) -> f32 {
        HEIGHT
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let local = self.frame.local(cursor, HEIGHT)?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(Hit::Title(hit));
        }
        Some(Hit::Window)
    }

    pub fn draw<'a>(
        &self,
        layer: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        messages: &[String],
        clan: &Clan,
        pointer: [f32; 2],
    ) {
        if !self.frame.open {
            return;
        }
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        let at = |local: [f32; 2]| self.frame.at(local);
        self.frame.draw_title(layer, chrome, message(TITLE), pointer);
        if self.frame.minimized {
            return;
        }
        layer.sprites.push((&chrome.white, at([0.0, TITLE_HEIGHT]), [WIDTH, HEIGHT - TITLE_HEIGHT - FOOTER]));
        for x in [0.0, WIDTH - 1.0] {
            layer.sprites.push((&chrome.border, at([x, TITLE_HEIGHT]), [1.0, HEIGHT - TITLE_HEIGHT - FOOTER]));
        }
        layer.sprites.push((&assets.footer, at([0.0, HEIGHT - FOOTER]), [WIDTH, FOOTER]));

        if let Some(illust) = clan.id.checked_sub(1).and_then(|i| assets.illusts.get(i as usize)) {
            layer.sprites.push((illust, at(ILLUST), illust.size));
        }
        layer.texts.push((message(EMBLEM_LABEL).to_owned(), at([INFO_LEFT, TITLE_HEIGHT + 12.0]), [0, 0, 0]));
        layer.sprites.push((&assets.frame, at(EMBLEM), assets.frame.size));
        if let Some(emblem) = assets.emblems.get(clan.id as usize) {
            layer.sprites.push((emblem, at(offset(EMBLEM, [2.0, 2.0])), EMBLEM_SIZE));
        }
        for (row, (label, value)) in LABELS.iter().zip(info_values(clan)).enumerate() {
            let y = TITLE_HEIGHT + 40.0 + row as f32 * LINE;
            layer.texts.push((format!("{} : {value}", message(*label)), at([INFO_LEFT, y]), [0, 0, 0]));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_clan_rows() {
        let clan = Clan {
            id: 2,
            name: "Arcwand Clan".into(),
            master: "Devon Aire".into(),
            map: "geffen.gat".into(),
            online: 1,
            max: 500,
            allies: vec!["Golden Mace Clan".into()],
            hostile: vec!["Crossbow Clan".into()],
        };
        assert_eq!(
            info_values(&clan),
            ["Arcwand Clan", "Devon Aire", "1 / 500", "geffen", "Golden Mace Clan", "Crossbow Clan"]
        );
    }
}
