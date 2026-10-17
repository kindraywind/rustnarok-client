//! The quest window (Alt+U): quests in progress and those put aside under the client's own tabbed background, each
//! with a lock that moves it between them, and beside it the picked quest's description, hunts and time limit.

use std::collections::HashMap;

use roclient::grf::Grf;
use roclient::net::Quest;
use roclient::text::Font;

use crate::render::{Gpu, Texture};
use crate::window::{self, Chrome, Frame, TitleHit};
use crate::{Layer, UI, contains, offset};

// ponytail: rows, the lock and the detail window's text places are guesses over renew_questui's backgrounds;
// calibrate against the real client. The recommended tab stays empty.
/// The background is 381×466 with the tabs down its left 30px; the frame (title bar) starts after them.
const TABS_WIDTH: f32 = 30.0;
const WIDTH: f32 = 351.0;
const HEIGHT: f32 = 466.0;
/// Tabs' tops and height, across the background.
const TAB_TOPS: [f32; 3] = [17.0, 132.0, 247.0];
const TAB_SIZE: [f32; 2] = [28.0, 113.0];
const ROWS_AT: [f32; 2] = [6.0, 21.0];
const ROW_STEP: f32 = 47.0;
const ROWS: usize = 8;
const LOCK_AT: [f32; 2] = [300.0, 14.0];
const LOCK: [f32; 2] = [20.0, 17.0];
const POSITION: [f32; 2] = [200.0, 120.0];
const DETAIL_WIDTH: f32 = 342.0;
const DETAIL_HEIGHT: f32 = 412.0;
const DETAIL_TEXT_TOP: f32 = 70.0;
const DETAIL_LINE: f32 = 16.0;
const GREY: [u8; 3] = [0x70, 0x70, 0x70];
/// "Quest List", "Description", "Target monsters", "Time Limit".
const TITLE: usize = 1317;
const DESCRIPTION: usize = 3401;
const HUNTS: usize = 2191;
const TIME_LIMIT: usize = 2194;

/// A quest's words, from questid2display.txt.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct QuestText {
    pub title: String,
    pub description: String,
    pub summary: String,
}

pub fn texts(grf: &Grf) -> HashMap<u32, QuestText> {
    grf.read("data\\questid2display.txt").map(|bytes| parse_texts(&encoding_rs::EUC_KR.decode(&bytes).0)).unwrap_or_default()
}

/// questid2display.txt: per quest `id#title#icon#image#`, then its description and summary, each ending in `#`.
fn parse_texts(text: &str) -> HashMap<u32, QuestText> {
    let fields: Vec<&str> = text.split('#').map(str::trim).collect();
    fields
        .chunks_exact(6)
        .filter_map(|entry| {
            Some((entry[0].parse().ok()?, QuestText { title: entry[1].to_owned(), description: entry[4].to_owned(), summary: entry[5].to_owned() }))
        })
        .collect()
}

pub struct Assets {
    /// The background with each tab open: in progress, recommended, put aside.
    backgrounds: [Texture; 3],
    row: [Texture; 2],
    icon: Texture,
    /// Put aside, in progress.
    lock: [Texture; 2],
    detail: Texture,
    detail_icon: Texture,
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |name: &str| gpu.load_texture(grf, &format!("{UI}renew_questui\\{name}.bmp"), false);
        Self {
            backgrounds: ["bg_quest1", "bg_quest2", "bg_quest3"].map(image),
            row: ["bg_questlist", "bg_questlist_press"].map(image),
            icon: image("ico_nq"),
            lock: ["bt_lock", "bt_lock_open"].map(image),
            detail: image("bg_questsub"),
            detail_icon: image("img_questiocn"),
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(TitleHit),
    Tab(usize),
    /// A quest, by its place among those the tab shows.
    Row(usize),
    Lock(usize),
    DetailClose,
    Window,
}

pub struct QuestWindow {
    pub frame: Frame,
    pub tab: usize,
    /// The quest whose details show.
    pub picked: Option<u32>,
}

impl Default for QuestWindow {
    fn default() -> Self {
        Self { frame: Frame::new(POSITION, WIDTH), tab: 0, picked: None }
    }
}

impl QuestWindow {
    /// The background's corner (left of the frame, by the tabs).
    fn corner(&self) -> [f32; 2] {
        [self.frame.position[0] - TABS_WIDTH, self.frame.position[1]]
    }

    fn detail_corner(&self) -> [f32; 2] {
        offset(self.frame.position, [WIDTH + 2.0, 24.0])
    }

    /// The quests the open tab shows: in progress, or put aside.
    fn shown<'q>(&self, quests: &'q [Quest]) -> Vec<&'q Quest> {
        quests
            .iter()
            .filter(|quest| match self.tab {
                0 => quest.active,
                2 => !quest.active,
                _ => false,
            })
            .collect()
    }

    pub fn hit(&self, cursor: [f32; 2], quests: &[Quest]) -> Option<Hit> {
        if !self.frame.open {
            return None;
        }
        if self.picked.is_some() && !self.frame.minimized {
            let detail = self.detail_corner();
            if contains(offset(detail, [DETAIL_WIDTH - 14.0, 3.0]), [11.0, 11.0], cursor) {
                return Some(Hit::DetailClose);
            }
            if contains(detail, [DETAIL_WIDTH, DETAIL_HEIGHT], cursor) {
                return Some(Hit::Window);
            }
        }
        let corner = self.corner();
        if !self.frame.minimized
            && let Some(tab) = TAB_TOPS.iter().position(|&top| contains(offset(corner, [2.0, top]), TAB_SIZE, cursor))
        {
            return Some(Hit::Tab(tab));
        }
        let local = self.frame.local(cursor, HEIGHT)?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(Hit::Title(hit));
        }
        let place = ((local[1] - ROWS_AT[1]) / ROW_STEP).floor();
        if local[1] < ROWS_AT[1] || place >= ROWS as f32 || place as usize >= self.shown(quests).len() {
            return Some(Hit::Window);
        }
        let place = place as usize;
        let top = ROWS_AT[1] + place as f32 * ROW_STEP;
        Some(if contains([ROWS_AT[0] + LOCK_AT[0], top + LOCK_AT[1]], LOCK, local) { Hit::Lock(place) } else { Hit::Row(place) })
    }

    /// A click; returns the quest to take up again or put aside, with its new state.
    pub fn click(&mut self, hit: Hit, quests: &[Quest]) -> Option<(u32, bool)> {
        match hit {
            Hit::Title(TitleHit::Close) => {
                self.frame.open = false;
                self.picked = None;
            }
            Hit::Title(title) => self.frame.click_title(title),
            Hit::Tab(tab) => self.tab = tab,
            Hit::Row(place) => {
                let id = self.shown(quests).get(place)?.id;
                self.picked = (self.picked != Some(id)).then_some(id);
            }
            Hit::Lock(place) => {
                let quest = *self.shown(quests).get(place)?;
                return Some((quest.id, !quest.active));
            }
            Hit::DetailClose => self.picked = None,
            Hit::Window => {}
        }
        None
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &self,
        layer: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        font: &Font,
        messages: &[String],
        quests: &[Quest],
        texts: &HashMap<u32, QuestText>,
        pointer: [f32; 2],
    ) {
        if !self.frame.open {
            return;
        }
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        let title = |id: u32| texts.get(&id).map_or_else(|| id.to_string(), |text| text.title.clone());
        if self.frame.minimized {
            self.frame.draw_title(layer, chrome, message(TITLE), pointer);
            return;
        }
        let background = &assets.backgrounds[self.tab.min(2)];
        layer.sprites.push((background, self.corner(), background.size));
        self.frame.draw_bare_title(layer, chrome, message(TITLE), pointer);
        let at = |local: [f32; 2]| self.frame.at(local);
        for (place, quest) in self.shown(quests).into_iter().take(ROWS).enumerate() {
            let top = ROWS_AT[1] + place as f32 * ROW_STEP;
            let row = &assets.row[usize::from(self.picked == Some(quest.id))];
            layer.sprites.push((row, at([ROWS_AT[0], top]), row.size));
            layer.sprites.push((&assets.icon, at([ROWS_AT[0] + 8.0, top + 8.0]), assets.icon.size));
            layer.texts.push((window::fit(font, &title(quest.id), 240.0), at([ROWS_AT[0] + 42.0, top + 7.0]), [0, 0, 0]));
            let summary = texts.get(&quest.id).map_or("", |text| text.summary.as_str());
            layer.texts.push((window::fit(font, summary, 240.0), at([ROWS_AT[0] + 42.0, top + 24.0]), GREY));
            let lock = &assets.lock[usize::from(quest.active)];
            layer.sprites.push((lock, at([ROWS_AT[0] + LOCK_AT[0], top + LOCK_AT[1]]), lock.size));
        }
        // The picked quest beside the list: its title and summary, description, hunts and time limit.
        let Some(quest) = self.picked.and_then(|id| quests.iter().find(|quest| quest.id == id)) else { return };
        let corner = self.detail_corner();
        layer.sprites.push((&assets.detail, corner, assets.detail.size));
        let detail = Frame::new(corner, DETAIL_WIDTH);
        detail.draw_bare_title(layer, chrome, message(DESCRIPTION), pointer);
        layer.sprites.push((&assets.detail_icon, offset(corner, [8.0, 20.0]), assets.detail_icon.size));
        layer.texts.push((window::fit(font, &title(quest.id), 270.0), offset(corner, [52.0, 25.0]), [0, 0, 0]));
        let text = texts.get(&quest.id).cloned().unwrap_or_default();
        layer.texts.push((window::fit(font, &text.summary, 270.0), offset(corner, [52.0, 42.0]), GREY));
        let mut lines: Vec<(String, [u8; 3])> =
            window::wrap(font, &text.description, DETAIL_WIDTH - 30.0, 12).into_iter().map(|line| (line, [0, 0, 0])).collect();
        if !quest.hunts.is_empty() {
            lines.push((String::new(), [0, 0, 0]));
            lines.push((message(HUNTS).to_owned(), GREY));
            lines.extend(quest.hunts.iter().map(|hunt| (format!("{} ({}/{})", hunt.name, hunt.count, hunt.needed), [0, 0, 0])));
        }
        if quest.end > 0 {
            let left =
                u64::from(quest.end).saturating_sub(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.as_secs()));
            lines.push((String::new(), [0, 0, 0]));
            lines.push((format!("{} : {}:{:02}:{:02}", message(TIME_LIMIT), left / 3600, left / 60 % 60, left % 60), GREY));
        }
        for (row, (line, color)) in lines.into_iter().take(19).enumerate() {
            layer.texts.push((line, offset(corner, [14.0, DETAIL_TEXT_TOP + row as f32 * DETAIL_LINE]), color));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shows_quests_by_tab_and_toggles_them() {
        let quests = [Quest { id: 1, active: true, ..Default::default() }, Quest { id: 2, active: false, ..Default::default() }];
        let mut window = QuestWindow::default();
        window.frame.open = true;
        let at = |local: [f32; 2]| [window.frame.position[0] + local[0], window.frame.position[1] + local[1]];
        let row = |place: f32| at([ROWS_AT[0] + 100.0, ROWS_AT[1] + place * ROW_STEP + 20.0]);
        assert_eq!(window.hit(row(0.0), &quests), Some(Hit::Row(0)));
        assert_eq!(window.hit(row(1.0), &quests), Some(Hit::Window), "one quest in progress");
        assert_eq!(window.click(Hit::Row(0), &quests), None);
        assert_eq!(window.picked, Some(1));
        assert_eq!(window.hit([window.frame.position[0] - 15.0, window.frame.position[1] + 300.0], &quests), Some(Hit::Tab(2)));
        window.click(Hit::Tab(2), &quests);
        assert_eq!(window.click(Hit::Lock(0), &quests), Some((2, true)));
    }

    #[test]
    fn reads_quest_texts() {
        let texts = parse_texts("1#Ransom#SG_FEEL#QUE_NOIMAGE#\nTurtle#\nCarapace#\n\n101#Professor#SG_FEEL#QUE_NOIMAGE#\nGo up.#\nTalk#\n");
        assert_eq!(texts[&101], QuestText { title: "Professor".into(), description: "Go up.".into(), summary: "Talk".into() });
        assert_eq!(texts[&1].summary, "Carapace");
    }
}
