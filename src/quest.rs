//! The quest window (Alt+U): four tabs down its left (in progress, recommended, on hold, waiting), a scrolling list
//! of their quests by id with a tracker check and a lock each, and beside it the picked quest's description, hunts
//! and time limit.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use rustnarok_client::grf::Grf;
use rustnarok_client::net::Quest;
use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};
use crate::window::{self, Chrome, Frame};
use crate::{Layer, UI, contains, offset};

/// 381 × 466 from the tabs' left edge: the tabs (30 wide), the title bar and background from x 29.
const WIDTH: f32 = 381.0;
const HEIGHT: f32 = 466.0;
const BAR: [f32; 2] = [29.0, 0.0];
const BAR_WIDTH: f32 = 352.0;
/// The title 18 into the bar; the bar's base button (it does nothing) 3 in; no close or minimise button.
const BAR_TITLE: f32 = 18.0;
const BASE_BUTTON: [f32; 2] = [32.0, 3.0];
const BACKGROUND: [f32; 2] = [29.0, 17.0];
const SEARCH_BAR: [f32; 2] = [29.0, 411.0];
/// Close, and "Show Quest" with its box.
const CLOSE: [f32; 2] = [327.0, 442.0];
const CLOSE_SIZE: [f32; 2] = [42.0, 20.0];
const SHOW_BOX: [f32; 2] = [38.0, 448.0];
const SHOW_BOX_SIZE: [f32; 2] = [10.0, 10.0];
const SHOW_LABEL: [f32; 2] = [52.0, 447.0];
// ponytail: the box and its label take clicks over 120 px.
const SHOW_AREA: [f32; 2] = [120.0, 12.0];
/// The tabs: in progress, recommended (empty here), on hold, waiting (cooldowns).
const TABS: [&str; 4] = ["process", "recommended", "hold", "wait"];
const TAB_TOPS: [f32; 4] = [17.0, 118.0, 219.0, 320.0];
const TAB_SIZE: [f32; 2] = [30.0, 100.0];
const WAITING: usize = 3;
/// The list: rows 330 × 46 from (38, 17), 8 whole ones shown; its scrollbar at x 374.
const LIST: [f32; 2] = [38.0, 17.0];
const LIST_HEIGHT: f32 = 394.0;
const ROW: [f32; 2] = [330.0, 46.0];
const ROWS: usize = 8;
const SCROLLBAR: [f32; 2] = [374.0, 17.0];
const SCROLL_WIDTH: f32 = 7.0;
const ARROW_HEIGHT: f32 = 3.0;
const THUMB_PIECE: f32 = 4.0;
/// A row's parts: the type icon, title and summary (cut to 260 px), the tracker check, the lock.
const ICON: [f32; 2] = [8.0, 7.0];
const ICON_SIZE: [f32; 2] = [26.0, 29.0];
const ROW_TITLE: [f32; 2] = [42.0, 6.0];
const ROW_SUMMARY: [f32; 2] = [42.0, 24.0];
const TEXT_WIDTH: f32 = 260.0;
const TITLE_COLOR: [u8; 3] = [0x22, 0x53, 0x95];
const PICKED_COLOR: [u8; 3] = [0x2E, 0x7B, 0xE4];
const SUMMARY_COLOR: [u8; 3] = [0x2E, 0x30, 0x38];
const CHECK: [f32; 2] = [312.0, 3.0];
const CHECK_SIZE: [f32; 2] = [14.0, 14.0];
const LOCK: [f32; 2] = [306.0, 22.0];
const LOCK_SIZE: [f32; 2] = [20.0, 17.0];
const CHECK_TIP: usize = 3409;
const LOCK_TIP: usize = 3407;
/// The details beside the window.
const DETAIL_WIDTH: f32 = 342.0;
const DETAIL_HEIGHT: f32 = 412.0;
const DETAIL_TEXT_TOP: f32 = 70.0;
const DETAIL_LINE: f32 = 16.0;
const GREY: [u8; 3] = [0x70, 0x70, 0x70];
/// "Quest List", "Description", "Target monsters", "Show Quest", "Time Limit".
const TITLE: usize = 3400;
const DESCRIPTION: usize = 3401;
const HUNTS: usize = 2191;
const SHOW_TRACKER: usize = 2019;
const TIME_LIMIT: usize = 2194;

/// A quest's words, from questid2display.txt.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct QuestText {
    pub title: String,
    pub description: String,
    pub summary: String,
    /// The quest NPC's cell on the big minimap, when the quest info lists one.
    pub npc: Option<[i32; 2]>,
    /// Only a cooldown timer: never on the tracker, and listed under Waiting.
    pub cooldown: bool,
    /// Its type icon's file under renew_questui, when the quest info names one.
    pub icon: Option<String>,
}

/// The quests' words: the translation's quest info Lua where there is one, else questid2display.txt.
pub fn texts(grf: &Grf) -> HashMap<u32, QuestText> {
    let mut texts: HashMap<u32, QuestText> =
        grf.read("data\\questid2display.txt").map(|bytes| parse_texts(&encoding_rs::EUC_KR.decode(&bytes).0)).unwrap_or_default();
    for (id, info) in rustnarok_client::lua::quest_info(grf).unwrap_or_default() {
        let icon = (!info.icon.is_empty()).then_some(info.icon);
        let text = QuestText { title: info.title, description: info.description, summary: info.summary, npc: info.npc, cooldown: info.cooldown, icon };
        texts.insert(id, text);
    }
    texts
}

/// questid2display.txt: per quest `id#title#icon#image#`, then its description and summary, each ending in `#`.
fn parse_texts(text: &str) -> HashMap<u32, QuestText> {
    let fields: Vec<&str> = text.split('#').map(str::trim).collect();
    fields
        .chunks_exact(6)
        .filter_map(|entry| {
            let text = QuestText {
                title: entry[1].to_owned(),
                description: entry[4].to_owned(),
                summary: entry[5].to_owned(),
                npc: None,
                cooldown: false,
                icon: None,
            };
            Some((entry[0].parse().ok()?, text))
        })
        .collect()
}

pub struct Assets {
    background: Texture,
    search: Texture,
    /// Close: normal, pointed at, pressed.
    close: [Texture; 3],
    /// The "Show Quest" box, off and on.
    show: [Texture; 2],
    /// Each tab, shut and open.
    tabs: [[Texture; 2]; 4],
    /// A row: plain, picked, pointed at.
    rows: [Texture; 3],
    icon: Texture,
    /// The tracker check, off and on; the lock, on hold and in progress.
    check: [Texture; 2],
    lock: [Texture; 2],
    /// The scrollbar's arrows (up, down) and thumb (top, middle, bottom).
    arrows: [Texture; 2],
    thumb: [Texture; 3],
    detail: Texture,
    detail_icon: Texture,
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        // Data without these images still starts: a missing one is left blank.
        let load = |path: String| gpu.try_load_texture(grf, &path, false).unwrap_or_else(|_| gpu.solid([0, 0, 0, 0]));
        let image = |name: &str| load(format!("{UI}renew_questui\\{name}.bmp"));
        let basic = |name: &str| load(format!("{UI}basic_interface\\{name}.bmp"));
        Self {
            background: image("bg_quest"),
            search: image("bg_search"),
            close: ["btn_close", "btn_close_a", "btn_close_b"].map(basic),
            show: ["checkbox_off", "checkbox_on"].map(image),
            tabs: TABS.map(|name| ["off", "on"].map(|look| image(&format!("tab_{name}_{look}")))),
            rows: ["bg_questlist", "bg_questlist_check", "bg_questlist_press"].map(image),
            icon: image("ico_nq"),
            check: ["bt_check_off", "bt_check_on"].map(image),
            lock: ["bt_lock", "bt_lock_open"].map(image),
            arrows: ["bt_scrollbar_arrow_upper", "bt_scrollbar_arrow_down"].map(image),
            thumb: ["bt_scrollbar_up", "bt_scrollbar_mid", "bt_scrollbar_down"].map(image),
            detail: image("bg_questsub"),
            detail_icon: image("img_questiocn"),
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Tab(usize),
    /// A quest, by its place among those the tab lists.
    Row(usize),
    /// Its tracker check, and its lock.
    Check(usize),
    Lock(usize),
    /// The "Show Quest" box.
    ShowTracker,
    Close,
    ScrollUp,
    ScrollDown,
    DetailClose,
    /// Anywhere else: the window drags from it.
    Window,
}

/// The tab a quest is listed under: waiting for a cooldown, else in progress or on hold.
fn tab_of(quest: &Quest, texts: &HashMap<u32, QuestText>) -> usize {
    if texts.get(&quest.id).is_some_and(|text| text.cooldown) {
        WAITING
    } else if quest.active {
        0
    } else {
        2
    }
}

/// Where the window opens: the middle of the screen, (W/2 − 190, H/2 − 233).
pub fn centred(screen: [f32; 2]) -> [f32; 2] {
    [(screen[0] / 2.0 - 190.0).floor(), (screen[1] / 2.0 - 233.0).floor()]
}

/// A saved place, when the whole window fits on the screen there; else the middle.
pub fn restored_place(saved: [f32; 2], screen: [f32; 2]) -> [f32; 2] {
    let fits = saved[0] >= 0.0 && saved[1] >= 0.0 && saved[0] + WIDTH <= screen[0] && saved[1] + HEIGHT <= screen[1];
    if fits { saved } else { centred(screen) }
}

/// A quest type icon's file.
fn icon_path(name: &str) -> String {
    format!("{UI}renew_questui\\{name}")
}

pub struct QuestWindow {
    pub frame: Frame,
    pub tab: usize,
    /// Rows scrolled past.
    scroll: usize,
    /// The quest whose details show.
    pub picked: Option<u32>,
    /// The quest tracker shows on screen.
    pub tracker: bool,
    /// Quests whose switch is off: the tracker leaves them out (saved, as the game's OngoingSimpleViewList).
    pub hidden: HashSet<u32>,
}

impl Default for QuestWindow {
    fn default() -> Self {
        Self { frame: Frame::new(centred(crate::screen()), WIDTH), tab: 0, scroll: 0, picked: None, tracker: true, hidden: HashSet::new() }
    }
}

fn hidden_path() -> Option<PathBuf> {
    crate::settings_path("quest_hidden")
}

/// The hidden quests' ids, one a line.
pub fn hidden_from(text: &str) -> HashSet<u32> {
    text.lines().filter_map(|line| line.trim().parse().ok()).collect()
}

pub fn hidden_text(hidden: &HashSet<u32>) -> String {
    let mut ids: Vec<u32> = hidden.iter().copied().collect();
    ids.sort_unstable();
    ids.iter().map(u32::to_string).collect::<Vec<_>>().join("\n")
}

impl QuestWindow {
    fn detail_corner(&self) -> [f32; 2] {
        offset(self.frame.position, [WIDTH + 2.0, 24.0])
    }

    /// The window with the hidden quests as saved.
    pub fn load() -> Self {
        let hidden = hidden_path().and_then(|path| std::fs::read_to_string(path).ok()).map(|text| hidden_from(&text)).unwrap_or_default();
        Self { hidden, ..Self::default() }
    }

    pub fn save_hidden_to(&self, path: &Path) {
        let _ = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(path, hidden_text(&self.hidden)));
    }

    /// Opens on In progress, at the top, with no quest picked.
    pub fn open(&mut self) {
        self.frame.open = true;
        self.tab = 0;
        self.scroll = 0;
        self.picked = None;
    }

    /// Shuts, with the details.
    pub fn close(&mut self) {
        self.frame.open = false;
        self.picked = None;
    }

    /// Alt+U and the menu's button.
    pub fn toggle(&mut self) {
        if self.frame.open { self.close() } else { self.open() }
    }

    /// Turns a quest's tracker switch, and saves.
    pub fn toggle_shown(&mut self, id: u32) {
        if !self.hidden.remove(&id) {
            self.hidden.insert(id);
        }
        if let Some(path) = hidden_path() {
            self.save_hidden_to(&path);
        }
    }

    /// The quests the open tab lists, by id.
    fn shown<'q>(&self, quests: &'q [Quest], texts: &HashMap<u32, QuestText>) -> Vec<&'q Quest> {
        let mut shown: Vec<&Quest> = quests.iter().filter(|quest| tab_of(quest, texts) == self.tab).collect();
        shown.sort_by_key(|quest| quest.id);
        shown
    }

    /// The furthest the list scrolls: its last row at the bottom of the eight shown.
    fn most_scroll(&self, quests: &[Quest], texts: &HashMap<u32, QuestText>) -> usize {
        self.shown(quests, texts).len().saturating_sub(ROWS)
    }

    /// The wheel (and the arrows): a row a notch.
    pub fn scroll_by(&mut self, rows: i32, quests: &[Quest], texts: &HashMap<u32, QuestText>) {
        let most = self.most_scroll(quests, texts) as i32;
        self.scroll = (self.scroll as i32 + rows).clamp(0, most) as usize;
    }

    /// The row's place under a window-local point, and the part of it: the row, its check or its lock (neither on
    /// Waiting).
    fn row_at(&self, local: [f32; 2]) -> Option<Hit> {
        if !contains(LIST, [ROW[0], ROWS as f32 * ROW[1]], local) {
            return None;
        }
        let row = ((local[1] - LIST[1]) / ROW[1]) as usize;
        let inside = [local[0] - LIST[0], local[1] - LIST[1] - row as f32 * ROW[1]];
        let place = self.scroll + row;
        Some(if self.tab != WAITING && contains(CHECK, CHECK_SIZE, inside) {
            Hit::Check(place)
        } else if self.tab != WAITING && contains(LOCK, LOCK_SIZE, inside) {
            Hit::Lock(place)
        } else {
            Hit::Row(place)
        })
    }

    /// Whether the window (or its details) is under the pointer.
    pub fn covers(&self, cursor: [f32; 2]) -> bool {
        self.hit(cursor, &[], &HashMap::new()).is_some()
    }

    pub fn hit(&self, cursor: [f32; 2], quests: &[Quest], texts: &HashMap<u32, QuestText>) -> Option<Hit> {
        if !self.frame.open {
            return None;
        }
        if self.picked.is_some() {
            let detail = self.detail_corner();
            if contains(offset(detail, [DETAIL_WIDTH - 14.0, 3.0]), [11.0, 11.0], cursor) {
                return Some(Hit::DetailClose);
            }
            if contains(detail, [DETAIL_WIDTH, DETAIL_HEIGHT], cursor) {
                return Some(Hit::Window);
            }
        }
        let local = self.frame.local(cursor, HEIGHT)?;
        if let Some(tab) = TAB_TOPS.iter().position(|&top| contains([0.0, top], TAB_SIZE, local)) {
            return Some(Hit::Tab(tab));
        }
        if contains(CLOSE, CLOSE_SIZE, local) {
            return Some(Hit::Close);
        }
        if contains(SHOW_BOX, SHOW_AREA, local) {
            return Some(Hit::ShowTracker);
        }
        if contains(SCROLLBAR, [SCROLL_WIDTH, ARROW_HEIGHT], local) {
            return Some(Hit::ScrollUp);
        }
        if contains(offset(SCROLLBAR, [0.0, LIST_HEIGHT - ARROW_HEIGHT]), [SCROLL_WIDTH, ARROW_HEIGHT], local) {
            return Some(Hit::ScrollDown);
        }
        let row = self.row_at(local).filter(|hit| match hit {
            Hit::Row(place) | Hit::Check(place) | Hit::Lock(place) => *place < self.shown(quests, texts).len(),
            _ => false,
        });
        Some(row.unwrap_or(Hit::Window))
    }

    /// The tabs, Close, the rows and the listed quests' tracker checks on screen (for the test driver):
    /// "quest_tab:<n>", "quest_close", "quest_row_<id>", "quest_switch_<id>".
    pub fn button_boxes(&self, quests: &[Quest], texts: &HashMap<u32, QuestText>) -> Vec<(String, [f32; 4])> {
        if !self.frame.open {
            return Vec::new();
        }
        let place = |local: [f32; 2], size: [f32; 2]| {
            let [x, y] = self.frame.at(local);
            [x, y, size[0], size[1]]
        };
        let mut boxes: Vec<(String, [f32; 4])> =
            TAB_TOPS.iter().enumerate().map(|(tab, &top)| (format!("quest_tab:{tab}"), place([0.0, top], TAB_SIZE))).collect();
        boxes.push(("quest_close".into(), place(CLOSE, CLOSE_SIZE)));
        for (row, quest) in self.shown(quests, texts).into_iter().skip(self.scroll).take(ROWS).enumerate() {
            let top = offset(LIST, [0.0, row as f32 * ROW[1]]);
            boxes.push((format!("quest_row_{}", quest.id), place(top, ROW)));
            if self.tab != WAITING {
                boxes.push((format!("quest_switch_{}", quest.id), place(offset(top, CHECK), CHECK_SIZE)));
            }
        }
        boxes
    }

    /// A click; returns the quest to take up again or put aside, with its new state.
    pub fn click(&mut self, hit: Hit, quests: &[Quest], texts: &HashMap<u32, QuestText>) -> Option<(u32, bool)> {
        match hit {
            Hit::Tab(tab) => {
                self.tab = tab;
                self.scroll = 0;
            }
            // A row always opens its details.
            Hit::Row(place) => self.picked = Some(self.shown(quests, texts).get(place)?.id),
            Hit::Lock(place) => {
                let quest = *self.shown(quests, texts).get(place)?;
                return Some((quest.id, !quest.active));
            }
            Hit::Check(place) => {
                let id = self.shown(quests, texts).get(place)?.id;
                self.toggle_shown(id);
            }
            Hit::ShowTracker => self.tracker = !self.tracker,
            Hit::Close => self.close(),
            Hit::ScrollUp => self.scroll_by(-1, quests, texts),
            Hit::ScrollDown => self.scroll_by(1, quests, texts),
            Hit::DetailClose => self.picked = None,
            Hit::Window => {}
        }
        None
    }

    /// The listed quests' type icons, to load before drawing.
    pub fn icons(&self, quests: &[Quest], texts: &HashMap<u32, QuestText>) -> Vec<String> {
        if !self.frame.open {
            return Vec::new();
        }
        self.shown(quests, texts).into_iter().filter_map(|quest| texts.get(&quest.id)?.icon.as_deref().map(icon_path)).collect()
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
        quests: &[Quest],
        texts: &HashMap<u32, QuestText>,
        pointer: ([f32; 2], bool),
    ) {
        if !self.frame.open {
            return;
        }
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        let title = |id: u32| texts.get(&id).map_or_else(|| id.to_string(), |text| text.title.clone());
        let at = |local: [f32; 2]| self.frame.at(local);
        let hit = self.hit(pointer.0, quests, texts);
        // The title bar (the base button alone), the background, the search strip and the tabs.
        chrome.draw_title_bar(layer, at(BAR), BAR_WIDTH);
        let base = chrome.button(0, contains(at(BASE_BUTTON), window::BUTTON_SIZE, pointer.0));
        layer.sprites.push((base, at(BASE_BUTTON), base.size));
        let title_top = ((window::TITLE_HEIGHT - font.line_height()) / 2.0).floor();
        layer.texts.push((message(TITLE).to_owned(), at([BAR[0] + BAR_TITLE, title_top]), [0, 0, 0]));
        layer.sprites.push((&assets.background, at(BACKGROUND), assets.background.size));
        layer.sprites.push((&assets.search, at(SEARCH_BAR), assets.search.size));
        for (tab, &top) in TAB_TOPS.iter().enumerate() {
            let image = &assets.tabs[tab][usize::from(tab == self.tab)];
            layer.sprites.push((image, at([0.0, top]), TAB_SIZE));
        }
        // The rows: plain, picked or pointed at; the type icon, title and summary; the check and lock but on Waiting.
        let shown = self.shown(quests, texts);
        for (row, quest) in shown.iter().skip(self.scroll).take(ROWS).enumerate() {
            let place = self.scroll + row;
            let corner = offset(LIST, [0.0, row as f32 * ROW[1]]);
            let pointed = matches!(hit, Some(Hit::Row(at) | Hit::Check(at) | Hit::Lock(at)) if at == place);
            let picked = self.picked == Some(quest.id);
            let (background, height) = if pointed {
                (&assets.rows[2], ROW[1] + 1.0)
            } else if picked {
                (&assets.rows[1], ROW[1])
            } else {
                (&assets.rows[0], ROW[1])
            };
            layer.sprites.push((background, at(corner), [ROW[0], height]));
            let text = texts.get(&quest.id);
            let icon = text.and_then(|text| text.icon.as_deref()).and_then(|name| icons.get(&icon_path(name))).and_then(Option::as_ref);
            layer.sprites.push((icon.unwrap_or(&assets.icon), at(offset(corner, ICON)), ICON_SIZE));
            let color = if picked { PICKED_COLOR } else { TITLE_COLOR };
            layer.texts.push((window::fit(font, &title(quest.id), TEXT_WIDTH), at(offset(corner, ROW_TITLE)), color));
            let summary = text.map_or("", |text| text.summary.as_str());
            layer.texts.push((window::fit(font, summary, TEXT_WIDTH), at(offset(corner, ROW_SUMMARY)), SUMMARY_COLOR));
            if self.tab == WAITING {
                continue;
            }
            let check = &assets.check[usize::from(!self.hidden.contains(&quest.id))];
            layer.sprites.push((check, at(offset(corner, CHECK)), CHECK_SIZE));
            let lock = &assets.lock[usize::from(quest.active)];
            layer.sprites.push((lock, at(offset(corner, LOCK)), LOCK_SIZE));
            let tip = match hit {
                Some(Hit::Check(at)) if at == place => Some(CHECK_TIP),
                Some(Hit::Lock(at)) if at == place => Some(LOCK_TIP),
                _ => None,
            };
            if let Some(tip) = tip {
                window::tooltip(tooltip, chrome, font, message(tip), offset(pointer.0, [12.0, 12.0]), [255; 3]);
            }
        }
        // The scrollbar: its arrows, and a thumb for the rows shown.
        layer.sprites.push((&assets.arrows[0], at(SCROLLBAR), [SCROLL_WIDTH, ARROW_HEIGHT]));
        layer.sprites.push((&assets.arrows[1], at(offset(SCROLLBAR, [0.0, LIST_HEIGHT - ARROW_HEIGHT])), [SCROLL_WIDTH, ARROW_HEIGHT]));
        let track = LIST_HEIGHT - 2.0 * ARROW_HEIGHT;
        let most = self.most_scroll(quests, texts);
        let thumb = (track * ROWS as f32 / shown.len().max(ROWS) as f32).max(3.0 * THUMB_PIECE);
        let top = SCROLLBAR[1] + ARROW_HEIGHT + if most == 0 { 0.0 } else { (track - thumb) * self.scroll as f32 / most as f32 };
        let [up, middle, down] = &assets.thumb;
        layer.sprites.push((up, at([SCROLLBAR[0], top]), [SCROLL_WIDTH, THUMB_PIECE]));
        layer.sprites.push((middle, at([SCROLLBAR[0], top + THUMB_PIECE]), [SCROLL_WIDTH, thumb - 2.0 * THUMB_PIECE]));
        layer.sprites.push((down, at([SCROLLBAR[0], top + thumb - THUMB_PIECE]), [SCROLL_WIDTH, THUMB_PIECE]));
        // Close, and whether the tracker shows.
        let close = if hit == Some(Hit::Close) { 1 + usize::from(pointer.1) } else { 0 };
        layer.sprites.push((&assets.close[close], at(CLOSE), CLOSE_SIZE));
        let show = &assets.show[usize::from(self.tracker)];
        layer.sprites.push((show, at(SHOW_BOX), SHOW_BOX_SIZE));
        layer.texts.push((message(SHOW_TRACKER).to_owned(), at(SHOW_LABEL), [0, 0, 0]));
        // The picked quest beside the list: its title and summary, description, hunts and time limit.
        let Some(quest) = self.picked.and_then(|id| quests.iter().find(|quest| quest.id == id)) else { return };
        let corner = self.detail_corner();
        layer.sprites.push((&assets.detail, corner, assets.detail.size));
        let detail = Frame::new(corner, DETAIL_WIDTH);
        detail.draw_bare_title(layer, chrome, message(DESCRIPTION), pointer.0);
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

    fn quest(id: u32, active: bool) -> Quest {
        Quest { id, active, ..Default::default() }
    }

    #[test]
    fn lists_quests_by_tab_in_id_order() {
        let texts: HashMap<u32, QuestText> =
            [(3, QuestText { cooldown: true, ..Default::default() }), (4, QuestText { cooldown: true, ..Default::default() })].into();
        // Active or not, cooldown or not.
        assert_eq!([quest(1, true), quest(2, false), quest(3, true), quest(4, false)].map(|quest| tab_of(&quest, &texts)), [0, 2, WAITING, WAITING]);
        let quests = [quest(9, true), quest(5, true), quest(7, true)];
        let window = QuestWindow::default();
        assert_eq!(window.shown(&quests, &texts).iter().map(|quest| quest.id).collect::<Vec<_>>(), [5, 7, 9]);
    }

    #[test]
    fn hits_rows_checks_and_locks_but_not_on_waiting() {
        let quests: Vec<Quest> = (1..=10).map(|id| quest(id, true)).collect();
        let texts = HashMap::new();
        let mut window = QuestWindow::default();
        window.open();
        let corner = window.frame.position;
        let at = |local: [f32; 2]| offset(corner, local);
        let row = |place: f32, inside: [f32; 2]| at([LIST[0] + inside[0], LIST[1] + place * ROW[1] + inside[1]]);
        assert_eq!(window.hit(row(0.0, [100.0, 20.0]), &quests, &texts), Some(Hit::Row(0)));
        assert_eq!(window.hit(row(1.0, [313.0, 4.0]), &quests, &texts), Some(Hit::Check(1)));
        assert_eq!(window.hit(row(2.0, [307.0, 23.0]), &quests, &texts), Some(Hit::Lock(2)));
        assert_eq!(window.hit(at([5.0, 120.0]), &quests, &texts), Some(Hit::Tab(1)));
        assert_eq!(window.hit(at([330.0, 445.0]), &quests, &texts), Some(Hit::Close));
        assert_eq!(window.hit(at([200.0, 440.0]), &quests, &texts), Some(Hit::Window), "it drags from here");
        // Ten quests: 8 show, the list scrolls 2.
        assert_eq!(window.most_scroll(&quests, &texts), 2);
        window.scroll_by(5, &quests, &texts);
        assert_eq!(window.hit(row(7.0, [100.0, 20.0]), &quests, &texts), Some(Hit::Row(9)));
        assert_eq!(window.click(Hit::Row(9), &quests, &texts), None);
        window.click(Hit::Row(9), &quests, &texts);
        assert_eq!(window.picked, Some(10), "a row's details stay open on a second click");
        // Waiting: no check, no lock.
        let cooldowns: HashMap<u32, QuestText> = (1..=10).map(|id| (id, QuestText { cooldown: true, ..Default::default() })).collect();
        window.click(Hit::Tab(WAITING), &quests, &cooldowns);
        assert_eq!(window.hit(row(1.0, [313.0, 4.0]), &quests, &cooldowns), Some(Hit::Row(1)));
        assert!(window.button_boxes(&quests, &cooldowns).iter().all(|(name, _)| !name.starts_with("quest_switch")));
        // Alt+U shuts it with its details, and opens it again on In progress at the top.
        window.toggle();
        assert!(!window.frame.open && window.picked.is_none());
        window.toggle();
        assert_eq!((window.tab, window.scroll, window.picked), (0, 0, None));
    }

    #[test]
    fn opens_in_the_middle_or_where_it_was_left() {
        assert_eq!(centred([1024.0, 768.0]), [322.0, 151.0]);
        assert_eq!(restored_place([10.0, 20.0], [1024.0, 768.0]), [10.0, 20.0]);
        assert_eq!(restored_place([700.0, 20.0], [1024.0, 768.0]), [322.0, 151.0], "it would stick out on the right");
        assert_eq!(restored_place([10.0, 400.0], [1024.0, 768.0]), [322.0, 151.0]);
    }

    #[test]
    fn switches_quests_on_the_tracker() {
        let quests = [quest(1, true)];
        let mut window = QuestWindow::default();
        window.open();
        let texts = HashMap::new();
        let [x, y, w, h] = window.button_boxes(&quests, &texts).iter().find(|(name, _)| name == "quest_switch_1").unwrap().1;
        assert_eq!(window.hit([x + w / 2.0, y + h / 2.0], &quests, &texts), Some(Hit::Check(0)));
        // The hidden ids, saved a line each.
        let hidden: HashSet<u32> = [7001, 12].into_iter().collect();
        assert_eq!(hidden_text(&hidden), "12\n7001");
        assert_eq!(hidden_from("12\n7001\nnot a number\n"), hidden);
    }

    #[test]
    fn reads_quest_texts() {
        let texts = parse_texts("1#Ransom#SG_FEEL#QUE_NOIMAGE#\nTurtle#\nCarapace#\n\n101#Professor#SG_FEEL#QUE_NOIMAGE#\nGo up.#\nTalk#\n");
        let professor = QuestText { title: "Professor".into(), description: "Go up.".into(), summary: "Talk".into(), npc: None, cooldown: false, icon: None };
        assert_eq!(texts[&101], professor);
        assert_eq!(texts[&1].summary, "Carapace");
    }
}
