//! The quest window (Alt+U): four tabs down its left (in progress, recommended, on hold, waiting), a scrolling list
//! of their quests by id with a tracker check and a lock each, and beside it the picked quest's description, hunts
//! and time limit.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use rustnarok_client::grf::Grf;
use rustnarok_client::lua::ItemInfo;
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
/// The search strip: six 26×22 filter buttons from (35, 414), 31 apart; the box (at most 20 characters) and its button.
const FILTER_NAMES: [&str; 6] = ["all", "normal", "sub", "daily", "event", "etc"];
const FILTER_AT: [f32; 2] = [35.0, 414.0];
const FILTER_STEP: f32 = 31.0;
const FILTER_SIZE: [f32; 2] = [26.0, 22.0];
const SEARCH_BOX: [f32; 4] = [223.0, 417.0, 119.0, 16.0];
const SEARCH_RIM: [u8; 4] = [0xC6, 0xC6, 0xC6, 0xFF];
const SEARCH_BUTTON: [f32; 2] = [342.0, 416.0];
const SEARCH_BUTTON_SIZE: [f32; 2] = [23.0, 18.0];
const SEARCH_CHARACTERS: usize = 20;
/// The client's filter table's ids for the buttons, left to right (ALL, NORMAL, SUB, DAILY, EVENT, ETC).
pub const FILTER_IDS: [i64; 6] = [0, 2, 3, 4, 5, 1];

/// The filters: each button's quest icons (lower case, no ".bmp") and tooltip, from the client's QuestFilterTable.
#[derive(Clone, Debug, PartialEq)]
pub struct Filters {
    pub icons: [Vec<String>; 6],
    pub tooltips: [String; 6],
}

impl Default for Filters {
    /// What the client's table holds.
    fn default() -> Self {
        let icons = |names: &[&str]| names.iter().map(|name| (*name).to_owned()).collect();
        Self {
            icons: [icons(&[]), icons(&["ico_ep", "ico_gq", "ico_lq"]), icons(&["ico_sub"]), icons(&["ico_dq"]), icons(&["ico_ev"]), icons(&[])],
            // ponytail: the tooltips are only in the client's table; without it the buttons have none.
            tooltips: Default::default(),
        }
    }
}

/// An icon's name as the filters compare it: lower case, without ".bmp".
fn icon_key(name: &str) -> String {
    name.trim().to_ascii_lowercase().trim_end_matches(".bmp").to_owned()
}

impl Filters {
    /// The client's table where the data has it (by its ids), else the built-in one.
    pub fn load(grf: &Grf) -> Self {
        let Ok(table) = rustnarok_client::lua::quest_filters(grf) else { return Self::default() };
        let mut filters = Self::default();
        for (button, id) in FILTER_IDS.iter().enumerate() {
            if let Some((tooltip, icons)) = table.get(id) {
                filters.tooltips[button] = tooltip.clone();
                filters.icons[button] = icons.iter().map(|icon| icon_key(icon)).collect();
            }
        }
        filters
    }

    /// Whether a filter keeps a quest with this icon (its IconName): All every quest, Etc one whose icon is in no other
    /// filter (or none), the rest their icons.
    fn keeps(&self, button: usize, icon: Option<&str>) -> bool {
        let icon = icon.map(icon_key);
        match button {
            0 => true,
            5 => icon.is_none_or(|icon| !self.icons[1..5].iter().any(|icons| icons.contains(&icon))),
            _ => icon.is_some_and(|icon| self.icons[button].contains(&icon)),
        }
    }
}

/// Whether a quest passes the search: the text in its title or summary, as written; an empty search passes all.
fn found(text: Option<&QuestText>, search: &str) -> bool {
    search.is_empty() || text.is_some_and(|text| text.title.contains(search) || text.summary.contains(search))
}

/// The details window, right of the list: places from its corner.
const DETAIL_WIDTH: f32 = 342.0;
pub const DETAIL_HEIGHT: f32 = 412.0;
const DETAIL_BACKGROUND: [f32; 2] = [0.0, 17.0];
const DETAIL_BASE: [f32; 2] = [3.0, 3.0];
const DETAIL_CLOSE: [f32; 2] = [288.0, 388.0];
const DETAIL_TITLE_TOP: f32 = 39.0;
const DETAIL_TITLE_COLOR: [u8; 3] = [0x1D, 0x32, 0x55];
/// The scroll area and its bar; three panels in it, 335 wide.
const SCROLL_AREA: [f32; 4] = [0.0, 62.0, 342.0, 322.0];
const DETAIL_SCROLLBAR_X: f32 = 335.0;
const PANEL_WIDTH: f32 = 335.0;
/// In a panel: the bullet, the header, the text box; a panel is as tall as its lowest part and 9.
const BULLET: [f32; 2] = [9.0, 9.0];
const HEADER: [f32; 2] = [25.0, 9.0];
const TEXT_BOX: [f32; 2] = [32.0, 30.0];
const TEXT_BOX_WIDTH: f32 = 290.0;
const PANEL_BELOW: f32 = 9.0;
// ponytail: the details' lines are 16 px apart.
const DETAIL_LINE: f32 = 16.0;
const DETAIL_TEXT_COLOR: [u8; 3] = [0x2E, 0x30, 0x38];
/// The Reward panel: 122 tall; its labels and values, the item list, the NPC's picture.
const REWARD_HEIGHT: f32 = 122.0;
const REWARD_ROWS: [f32; 3] = [30.0, 51.0, 72.0];
const REWARD_LABEL: f32 = 32.0;
const REWARD_VALUE: f32 = 110.0;
const REWARD_COLOR: [u8; 3] = [0x1D, 0x55, 0xA0];
const ITEM_LIST: [f32; 4] = [110.0, 72.0, 225.0, 41.0];
const ITEM_LIST_BAR: f32 = 218.0;
const ITEM_ICON: [f32; 2] = [7.0, 7.0];
const ITEM_NAME: [f32; 2] = [46.0, 5.0];
const ITEM_COUNT: [f32; 2] = [46.0, 20.0];
const NPC_BOX: [f32; 4] = [237.0, 9.0, 75.0, 145.0];
const DEADLINE: [f32; 2] = [13.0, 393.0];
const DEADLINE_COLOR: [u8; 3] = [0xD2, 0x1B, 0x16];
/// "Quest List", "Description", "Monster", "Reward", "Base EXP", "Job EXP", "Item", "%d ea.", "Deadline", "Show Quest".
const TITLE: usize = 3400;
const DESCRIPTION: usize = 3401;
const MONSTERS: usize = 3402;
const REWARD: usize = 3403;
const REWARD_EXP: usize = 3404;
const REWARD_JOB_EXP: usize = 3405;
const REWARD_ITEMS: usize = 3406;
const EACH: usize = 182;
const DEADLINE_MESSAGE: usize = 2195;
const SHOW_TRACKER: usize = 2019;

/// A quest's words, from questid2display.txt.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct QuestText {
    pub title: String,
    /// The description, a line an entry.
    pub description: Vec<String>,
    pub summary: String,
    /// The quest NPC's cell on the big minimap, when the quest info lists one.
    pub npc: Option<[i32; 2]>,
    /// Only a cooldown timer: never on the tracker, and listed under Waiting.
    pub cooldown: bool,
    /// Its type icon's file under renew_questui, when the quest info names one.
    pub icon: Option<String>,
    /// Its NPC's sprite and the map to find it on; what it rewards (base and job exp as written, items and counts).
    pub npc_sprite: String,
    pub npc_map: String,
    pub reward_exp: String,
    pub reward_job_exp: String,
    pub reward_items: Vec<(u32, u32)>,
}

/// The quests' words: the translation's quest info Lua where there is one, else questid2display.txt.
pub fn texts(grf: &Grf) -> HashMap<u32, QuestText> {
    let mut texts: HashMap<u32, QuestText> =
        grf.read("data\\questid2display.txt").map(|bytes| parse_texts(&encoding_rs::EUC_KR.decode(&bytes).0)).unwrap_or_default();
    for (id, info) in rustnarok_client::lua::quest_info(grf).unwrap_or_default() {
        let icon = (!info.icon.is_empty()).then_some(info.icon);
        let text = QuestText {
            title: info.title,
            description: info.description,
            summary: info.summary,
            npc: info.npc,
            cooldown: info.cooldown,
            icon,
            npc_sprite: info.npc_sprite,
            npc_map: info.npc_map,
            reward_exp: info.reward_exp,
            reward_job_exp: info.reward_job_exp,
            reward_items: info.reward_items,
        };
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
            let text = QuestText { title: entry[1].to_owned(), description: vec![entry[4].to_owned()], summary: entry[5].to_owned(), ..QuestText::default() };
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
    /// The details: background, the bullet, the line between panels, an item's frame.
    /// The filters, off and on; the search button: normal, over, press; the box's rim.
    filters: [[Texture; 2]; 6],
    search_button: [Texture; 3],
    rim: Texture,
    detail: Texture,
    bullet: Texture,
    line: Texture,
    item_frame: Texture,
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
            filters: FILTER_NAMES.map(|name| ["off", "on"].map(|look| image(&format!("btn_{name}_{look}")))),
            search_button: ["normal", "over", "press"].map(|look| image(&format!("btn_search_{look}"))),
            rim: gpu.solid(SEARCH_RIM),
            detail: image("bg_detail_quest"),
            bullet: image("img_poring"),
            line: image("img_line"),
            item_frame: image("img_questiocn"),
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
    /// A filter button, the search box, the search button.
    Filter(usize),
    SearchBox,
    Search,
    /// The details: Close, the NPC's picture, anywhere else (it drags).
    DetailClose,
    DetailNpc,
    Detail,
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

/// A filter button's corner on the search strip.
fn filter_at(filter: usize) -> [f32; 2] {
    [FILTER_AT[0] + filter as f32 * FILTER_STEP, FILTER_AT[1]]
}

/// The details' place: right of the list with the tops aligned, or with the list shut, the middle of the screen.
pub fn details_place(list: Option<[f32; 2]>, screen: [f32; 2]) -> [f32; 2] {
    match list {
        Some(list) => offset(list, [WIDTH, 0.0]),
        None => [((screen[0] - DETAIL_WIDTH) / 2.0).floor(), ((screen[1] - DETAIL_HEIGHT) / 2.0).floor()],
    }
}

/// The panels' heights in the scroll area: Description and Monster by their lines (the header alone when there are
/// none), then Reward.
fn panel_heights(description: usize, hunts: usize) -> [f32; 3] {
    let text = |lines: usize| if lines == 0 { HEADER[1] + DETAIL_LINE } else { TEXT_BOX[1] + lines as f32 * DETAIL_LINE } + PANEL_BELOW;
    [text(description), text(hunts), REWARD_HEIGHT]
}

/// The Deadline line for an end time, local, or none for 0.
fn deadline_line(message: &str, end: u32, offset: i64) -> Option<String> {
    (end > 0).then(|| {
        let [year, month, day, hour, minute, _] = crate::char_select::local_date(i64::from(end), offset);
        format!("{message} [{year:04}/{month:02}/{day:02} {hour:02}:{minute:02}]")
    })
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
    /// The details' window (open while a quest is picked), how far its panels are scrolled (px), the reward items scrolled
    /// past, and the description's lines wrapped as last drawn.
    pub detail: Frame,
    detail_scroll: f32,
    reward_scroll: usize,
    description: Vec<String>,
    /// The filters, the one on, the search typed and the one applied (to every tab), and whether typing goes to the box.
    pub filters: Filters,
    filter: usize,
    search: String,
    applied: String,
    pub typing: bool,
    /// The quest tracker shows on screen.
    pub tracker: bool,
    /// Quests whose switch is off: the tracker leaves them out (saved, as the game's OngoingSimpleViewList).
    pub hidden: HashSet<u32>,
}

impl Default for QuestWindow {
    fn default() -> Self {
        Self {
            frame: Frame::new(centred(crate::screen()), WIDTH),
            tab: 0,
            scroll: 0,
            picked: None,
            detail: Frame::new([0.0; 2], DETAIL_WIDTH),
            detail_scroll: 0.0,
            reward_scroll: 0,
            description: Vec::new(),
            filters: Filters::default(),
            filter: 0,
            search: String::new(),
            applied: String::new(),
            typing: false,
            tracker: true,
            hidden: HashSet::new(),
        }
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
    /// Shows a quest's details: right of the list, or in the middle of the screen with the list shut.
    pub fn pick(&mut self, id: u32, screen: [f32; 2]) {
        self.picked = Some(id);
        self.detail.position = details_place(self.frame.open.then_some(self.frame.position), screen);
        self.detail_scroll = 0.0;
        self.reward_scroll = 0;
    }

    /// Wraps the picked quest's description to the text box, for the panels' heights.
    pub fn lay_out(&mut self, font: &Font, texts: &HashMap<u32, QuestText>) {
        let lines = self.picked.and_then(|id| texts.get(&id)).map_or(&[][..], |text| &text.description[..]);
        self.description = lines.iter().flat_map(|line| window::wrap(font, line, TEXT_BOX_WIDTH, 99)).collect();
    }

    /// The hunts' lines of the picked quest.
    fn hunt_lines(&self, quests: &[Quest], messages: &[String]) -> Vec<String> {
        let quest = self.picked.and_then(|id| quests.iter().find(|quest| quest.id == id));
        quest.map_or(Vec::new(), |quest| quest.hunts.iter().filter_map(|hunt| crate::quest_tracker::hunt_line(hunt, messages)).collect())
    }

    /// The panels' tops in the scroll area, scrolled.
    fn panel_tops(&self, hunts: usize) -> [f32; 3] {
        let [description, monsters, _] = panel_heights(self.description.len(), hunts);
        let top = SCROLL_AREA[1] - self.detail_scroll;
        [top, top + description, top + description + monsters]
    }

    /// The wheel on the details: over the reward items an item a notch, elsewhere the panels a line a notch, as far as
    /// they go.
    pub fn scroll_details(&mut self, rows: i32, cursor: [f32; 2], quests: &[Quest], texts: &HashMap<u32, QuestText>) {
        let hunts = self.hunt_count(quests);
        let [x, y, w, h] = ITEM_LIST;
        let [_, _, reward] = self.panel_tops(hunts);
        if contains(self.detail.at([x, reward + y]), [w, h], cursor) {
            let count = self.picked.and_then(|id| texts.get(&id)).map_or(0, |text| text.reward_items.len());
            self.reward_scroll = (self.reward_scroll as i32 + rows).clamp(0, count.saturating_sub(1) as i32) as usize;
            return;
        }
        let total: f32 = panel_heights(self.description.len(), hunts).iter().sum();
        let most = (total - SCROLL_AREA[3]).max(0.0);
        self.detail_scroll = (self.detail_scroll + rows as f32 * DETAIL_LINE).clamp(0.0, most);
    }

    /// Whether the pointer is on the details.
    pub fn on_details(&self, cursor: [f32; 2]) -> bool {
        self.picked.is_some() && contains(self.detail.position, [DETAIL_WIDTH, DETAIL_HEIGHT], cursor)
    }

    /// The NPC's picture on screen, when the picked quest has one.
    fn npc_box(&self, texts: &HashMap<u32, QuestText>, hunts: usize) -> Option<[f32; 4]> {
        let text = texts.get(&self.picked?)?;
        if text.npc_sprite.is_empty() {
            return None;
        }
        let [x, y, w, h] = NPC_BOX;
        let [_, _, reward] = self.panel_tops(hunts);
        let [left, top] = self.detail.at([x, reward + y]);
        Some([left, top, w, h])
    }

    /// How many hunt lines the picked quest has (those with a count to reach).
    fn hunt_count(&self, quests: &[Quest]) -> usize {
        let quest = self.picked.and_then(|id| quests.iter().find(|quest| quest.id == id));
        quest.map_or(0, |quest| quest.hunts.iter().filter(|hunt| hunt.needed > 0).count())
    }

    /// The NPC's sprite to show and where its feet go (the picture's box, bottom middle), when it's in the scroll area.
    pub fn npc_sprite(&self, texts: &HashMap<u32, QuestText>, quests: &[Quest]) -> Option<(String, [f32; 2])> {
        let [x, y, w, h] = self.npc_box(texts, self.hunt_count(quests))?;
        let area_top = self.detail.at([0.0, SCROLL_AREA[1]])[1];
        let sprite = texts.get(&self.picked?)?.npc_sprite.clone();
        // ponytail: drawn while its box's top is in the scroll area, feet 8 px above the box's bottom.
        (y >= area_top && y < area_top + SCROLL_AREA[3]).then_some((sprite, [x + w / 2.0, y + h - 8.0]))
    }

    /// The picked quest's NPC: its map and cell, to navigate to.
    pub fn npc_place(&self, texts: &HashMap<u32, QuestText>) -> Option<(String, [i32; 2])> {
        let text = texts.get(&self.picked?)?;
        (!text.npc_map.is_empty()).then(|| (text.npc_map.clone(), text.npc.unwrap_or_default()))
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
        // On All with an empty box.
        self.filter = 0;
        self.search.clear();
        self.applied.clear();
        self.typing = false;
    }

    /// Typed into the search box, up to 20 characters.
    pub fn type_text(&mut self, text: &str) {
        for character in text.chars().filter(|character| !character.is_control()) {
            if self.search.chars().count() < SEARCH_CHARACTERS {
                self.search.push(character);
            }
        }
    }

    pub fn backspace(&mut self) {
        self.search.pop();
    }

    /// Enter in the box, or the search button: the text applies to every tab.
    pub fn apply_search(&mut self) {
        self.applied = self.search.clone();
        self.scroll = 0;
        self.typing = false;
    }

    /// A `<QUEST>` link: the window open on the quest's tab, its row in view and picked; false when no list has it.
    pub fn show_quest(&mut self, id: u32, quests: &[Quest], texts: &HashMap<u32, QuestText>) -> bool {
        if !self.frame.open {
            self.open();
        }
        let tab = self.tab;
        for candidate in 0..TABS.len() {
            self.tab = candidate;
            if let Some(place) = self.shown(quests, texts).iter().position(|quest| quest.id == id) {
                self.scroll = self.scroll.clamp((place + 1).saturating_sub(ROWS), place);
                self.pick(id, crate::screen());
                return true;
            }
        }
        self.tab = tab;
        false
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
    /// The listed quests' ids on the tab, filtered and searched (for the test driver).
    pub fn listed(&self, quests: &[Quest], texts: &HashMap<u32, QuestText>) -> Vec<u32> {
        self.shown(quests, texts).iter().map(|quest| quest.id).collect()
    }

    fn shown<'q>(&self, quests: &'q [Quest], texts: &HashMap<u32, QuestText>) -> Vec<&'q Quest> {
        let mut shown: Vec<&Quest> = quests
            .iter()
            .filter(|quest| tab_of(quest, texts) == self.tab)
            .filter(|quest| self.filters.keeps(self.filter, texts.get(&quest.id).and_then(|text| text.icon.as_deref())))
            .filter(|quest| found(texts.get(&quest.id), &self.applied))
            .collect();
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
        self.on_details(cursor) || self.hit(cursor, &[], &HashMap::new()).is_some()
    }

    pub fn hit(&self, cursor: [f32; 2], quests: &[Quest], texts: &HashMap<u32, QuestText>) -> Option<Hit> {
        if self.on_details(cursor) {
            if contains(self.detail.at(DETAIL_CLOSE), CLOSE_SIZE, cursor) {
                return Some(Hit::DetailClose);
            }
            if self.npc_box(texts, self.hunt_count(quests)).is_some_and(|[x, y, w, h]| contains([x, y], [w, h], cursor)) {
                return Some(Hit::DetailNpc);
            }
            return Some(Hit::Detail);
        }
        if !self.frame.open {
            return None;
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
        if let Some(filter) = (0..FILTER_NAMES.len()).find(|&filter| contains(filter_at(filter), FILTER_SIZE, local)) {
            return Some(Hit::Filter(filter));
        }
        let [x, y, w, h] = SEARCH_BOX;
        if contains([x, y], [w, h], local) {
            return Some(Hit::SearchBox);
        }
        if contains(SEARCH_BUTTON, SEARCH_BUTTON_SIZE, local) {
            return Some(Hit::Search);
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
    /// "quest_tab:<n>", "quest_close", "quest_filter:<name>", "quest_search_box", "quest_search", "quest_row_<id>",
    /// "quest_switch_<id>", "quest_lock_<id>".
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
        boxes.extend(FILTER_NAMES.iter().enumerate().map(|(filter, name)| (format!("quest_filter:{name}"), place(filter_at(filter), FILTER_SIZE))));
        let [x, y, w, h] = SEARCH_BOX;
        boxes.push(("quest_search_box".into(), place([x, y], [w, h])));
        boxes.push(("quest_search".into(), place(SEARCH_BUTTON, SEARCH_BUTTON_SIZE)));
        if self.picked.is_some() {
            let [x, y] = self.detail.at(DETAIL_CLOSE);
            boxes.push(("quest_detail_close".into(), [x, y, CLOSE_SIZE[0], CLOSE_SIZE[1]]));
            if let Some(area) = self.npc_box(texts, self.hunt_count(quests)) {
                boxes.push(("quest_detail_npc".into(), area));
            }
        }
        for (row, quest) in self.shown(quests, texts).into_iter().skip(self.scroll).take(ROWS).enumerate() {
            let top = offset(LIST, [0.0, row as f32 * ROW[1]]);
            boxes.push((format!("quest_row_{}", quest.id), place(top, ROW)));
            if self.tab != WAITING {
                boxes.push((format!("quest_switch_{}", quest.id), place(offset(top, CHECK), CHECK_SIZE)));
                boxes.push((format!("quest_lock_{}", quest.id), place(offset(top, LOCK), LOCK_SIZE)));
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
            Hit::Row(place) => {
                let id = self.shown(quests, texts).get(place)?.id;
                self.pick(id, crate::screen());
            }
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
            // A filter, Enter or the search button: the filter and the text apply to every tab.
            Hit::Filter(filter) => {
                self.filter = filter;
                self.apply_search();
            }
            Hit::SearchBox => self.typing = true,
            Hit::Search => self.apply_search(),
            Hit::DetailClose => self.picked = None,
            Hit::DetailNpc | Hit::Detail | Hit::Window => {}
        }
        None
    }

    /// The listed quests' type icons and the picked one's reward items', to load before drawing.
    pub fn icons(&self, quests: &[Quest], texts: &HashMap<u32, QuestText>, items: &HashMap<u32, ItemInfo>) -> Vec<String> {
        let rewards = self.picked.and_then(|id| texts.get(&id)).map_or(&[][..], |text| &text.reward_items[..]);
        let mut icons: Vec<String> = rewards.iter().filter_map(|(item, _)| items.get(item).map(|info| crate::inventory::icon_path(info, true))).collect();
        if self.frame.open {
            icons.extend(self.shown(quests, texts).into_iter().filter_map(|quest| texts.get(&quest.id)?.icon.as_deref().map(icon_path)));
        }
        icons
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &self,
        layer: &mut Layer<'a>,
        tooltip: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        icons: &'a HashMap<String, Option<Texture>>,
        items: &HashMap<u32, ItemInfo>,
        font: &Font,
        messages: &[String],
        quests: &[Quest],
        texts: &HashMap<u32, QuestText>,
        pointer: ([f32; 2], bool),
    ) {
        if self.frame.open {
            self.draw_list(layer, tooltip, chrome, assets, icons, font, messages, quests, texts, pointer);
        }
        self.draw_details(layer.window(), chrome, assets, icons, items, font, messages, quests, texts, pointer);
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_list<'a>(
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
        // The filters (one on), with their tooltips; the see-through box, its rim and text; the search button.
        for (filter, images) in assets.filters.iter().enumerate() {
            let image = &images[usize::from(filter == self.filter)];
            layer.sprites.push((image, at(filter_at(filter)), FILTER_SIZE));
            let tip = &self.filters.tooltips[filter];
            if hit == Some(Hit::Filter(filter)) && !tip.is_empty() {
                window::tooltip(tooltip, chrome, font, tip, offset(pointer.0, [12.0, 12.0]), [255; 3]);
            }
        }
        let [x, y, w, h] = SEARCH_BOX;
        for (corner, size) in [([x, y], [w, 1.0]), ([x, y + h - 1.0], [w, 1.0]), ([x, y], [1.0, h]), ([x + w - 1.0, y], [1.0, h])] {
            layer.sprites.push((&assets.rim, at(corner), size));
        }
        let cursor = if self.typing { "|" } else { "" };
        let box_text = window::fit(font, &format!("{}{cursor}", self.search), w - 4.0);
        layer.texts.push((box_text, at([x + 2.0, y + ((h - font.line_height()) / 2.0).floor()]), [0, 0, 0]));
        let look = match hit {
            Some(Hit::Search) => 1 + usize::from(pointer.1),
            _ => 0,
        };
        layer.sprites.push((&assets.search_button[look], at(SEARCH_BUTTON), SEARCH_BUTTON_SIZE));
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
    }

    /// The details: the title, the Description, Monster and Reward panels (scrolled), the Deadline and Close. The NPC's
    /// picture is a sprite: the caller draws it (`npc_sprite`).
    #[allow(clippy::too_many_arguments)]
    fn draw_details<'a>(
        &self,
        layer: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        icons: &'a HashMap<String, Option<Texture>>,
        items: &HashMap<u32, ItemInfo>,
        font: &Font,
        messages: &[String],
        quests: &[Quest],
        texts: &HashMap<u32, QuestText>,
        pointer: ([f32; 2], bool),
    ) {
        let Some(id) = self.picked else { return };
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        let at = |local: [f32; 2]| self.detail.at(local);
        let hit = self.hit(pointer.0, quests, texts);
        let text = texts.get(&id).cloned().unwrap_or_default();
        // The title bar (the base button alone), the background, the quest's title in the middle.
        chrome.draw_title_bar(layer, at([0.0, 0.0]), DETAIL_WIDTH);
        let base = chrome.button(0, contains(at(DETAIL_BASE), window::BUTTON_SIZE, pointer.0));
        layer.sprites.push((base, at(DETAIL_BASE), base.size));
        let title_top = ((window::TITLE_HEIGHT - font.line_height()) / 2.0).floor();
        layer.texts.push((message(TITLE).to_owned(), at([BAR_TITLE, title_top]), [0, 0, 0]));
        layer.sprites.push((&assets.detail, at(DETAIL_BACKGROUND), assets.detail.size));
        let title = if text.title.is_empty() { id.to_string() } else { text.title.clone() };
        let title = window::fit(font, &title, DETAIL_WIDTH - 8.0);
        layer.texts.push((title.clone(), at([((DETAIL_WIDTH - font.width(&title)) / 2.0).floor(), DETAIL_TITLE_TOP]), DETAIL_TITLE_COLOR));
        // ponytail: what doesn't fit whole in the scroll area isn't drawn.
        let inside = |top: f32, height: f32| top >= SCROLL_AREA[1] && top + height <= SCROLL_AREA[1] + SCROLL_AREA[3];
        let hunts = self.hunt_lines(quests, messages);
        let tops = self.panel_tops(hunts.len());
        let heading = |layer: &mut Layer<'a>, top: f32, header: usize| {
            if inside(top + BULLET[1], assets.bullet.size[1]) {
                layer.sprites.push((&assets.bullet, at([BULLET[0], top + BULLET[1]]), assets.bullet.size));
            }
            if inside(top + HEADER[1], DETAIL_LINE) {
                layer.texts.push((message(header).to_owned(), at([HEADER[0], top + HEADER[1]]), [0, 0, 0]));
            }
        };
        for (panel, (header, lines)) in [(DESCRIPTION, &self.description), (MONSTERS, &hunts)].into_iter().enumerate() {
            heading(layer, tops[panel], header);
            for (row, line) in lines.iter().enumerate() {
                let top = tops[panel] + TEXT_BOX[1] + row as f32 * DETAIL_LINE;
                if inside(top, DETAIL_LINE) {
                    layer.texts.push((line.clone(), at([TEXT_BOX[0], top]), DETAIL_TEXT_COLOR));
                }
            }
            if inside(tops[panel + 1], 1.0) {
                layer.sprites.push((&assets.line, at([1.0, tops[panel + 1]]), [PANEL_WIDTH - 1.0, 1.0]));
            }
        }
        // Reward: base and job exp in blue, the items one at a time with their bar.
        let top = tops[2];
        heading(layer, top, REWARD);
        for (row, (label, value)) in
            [(REWARD_EXP, text.reward_exp.as_str()), (REWARD_JOB_EXP, text.reward_job_exp.as_str()), (REWARD_ITEMS, "")].into_iter().enumerate()
        {
            let row_top = top + REWARD_ROWS[row];
            if inside(row_top, DETAIL_LINE) {
                layer.texts.push((message(label).to_owned(), at([REWARD_LABEL, row_top]), [0, 0, 0]));
                layer.texts.push((value.to_owned(), at([REWARD_VALUE, row_top]), REWARD_COLOR));
            }
        }
        let [x, y, w, h] = ITEM_LIST;
        if let Some(&(item, count)) = text.reward_items.get(self.reward_scroll)
            && inside(top + y, h)
        {
            let corner = [x, top + y];
            layer.sprites.push((&assets.item_frame, at(corner), assets.item_frame.size));
            let icon = items.get(&item).and_then(|info| icons.get(&crate::inventory::icon_path(info, true))).and_then(Option::as_ref);
            if let Some(icon) = icon {
                layer.sprites.push((icon, at(offset(corner, ITEM_ICON)), icon.size));
            }
            let name = crate::inventory::item_name(items, item, true, 0);
            layer.texts.push((window::fit(font, &name, w - ITEM_NAME[0] - SCROLL_WIDTH), at(offset(corner, ITEM_NAME)), [0, 0, 0]));
            let each = rustnarok_client::messages::fill(message(EACH), &[&count]);
            layer.texts.push((each, at(offset(corner, ITEM_COUNT)), REWARD_COLOR));
            self.scrollbar(layer, assets, at([x + ITEM_LIST_BAR, top + y]), h, text.reward_items.len(), 1, self.reward_scroll);
        }
        // The panels' bar, the Deadline as a local date, Close.
        let total: f32 = panel_heights(self.description.len(), hunts.len()).iter().sum();
        let lines = (total / DETAIL_LINE).ceil() as usize;
        let shown = (SCROLL_AREA[3] / DETAIL_LINE) as usize;
        self.scrollbar(layer, assets, at([DETAIL_SCROLLBAR_X, SCROLL_AREA[1]]), SCROLL_AREA[3], lines, shown, (self.detail_scroll / DETAIL_LINE) as usize);
        let end = quests.iter().find(|quest| quest.id == id).map_or(0, |quest| quest.end);
        if let Some(line) = deadline_line(message(DEADLINE_MESSAGE), end, crate::char_select::local_offset()) {
            layer.texts.push((line, at(DEADLINE), DEADLINE_COLOR));
        }
        let close = if hit == Some(Hit::DetailClose) { 1 + usize::from(pointer.1) } else { 0 };
        layer.sprites.push((&assets.close[close], at(DETAIL_CLOSE), CLOSE_SIZE));
    }

    /// A 7-wide scrollbar `height` tall at `corner`: its arrows, and a thumb for `shown` of `count` rows from `first`.
    #[allow(clippy::too_many_arguments)]
    fn scrollbar<'a>(&self, layer: &mut Layer<'a>, assets: &'a Assets, corner: [f32; 2], height: f32, count: usize, shown: usize, first: usize) {
        layer.sprites.push((&assets.arrows[0], corner, [SCROLL_WIDTH, ARROW_HEIGHT]));
        layer.sprites.push((&assets.arrows[1], offset(corner, [0.0, height - ARROW_HEIGHT]), [SCROLL_WIDTH, ARROW_HEIGHT]));
        let track = height - 2.0 * ARROW_HEIGHT;
        let most = count.saturating_sub(shown);
        let thumb = (track * shown as f32 / count.max(shown).max(1) as f32).max(3.0 * THUMB_PIECE).min(track);
        let top = corner[1] + ARROW_HEIGHT + if most == 0 { 0.0 } else { (track - thumb) * first.min(most) as f32 / most as f32 };
        let [up, middle, down] = &assets.thumb;
        layer.sprites.push((up, [corner[0], top], [SCROLL_WIDTH, THUMB_PIECE]));
        layer.sprites.push((middle, [corner[0], top + THUMB_PIECE], [SCROLL_WIDTH, thumb - 2.0 * THUMB_PIECE]));
        layer.sprites.push((down, [corner[0], top + thumb - THUMB_PIECE], [SCROLL_WIDTH, THUMB_PIECE]));
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
    fn filters_by_icon_and_searches_titles_and_summaries() {
        let filters = Filters::default();
        // All, Normal, Sub, Daily, Event, Etc on each icon, an unknown one and none.
        let kept = |icon: Option<&str>| (0..6).filter(|&button| filters.keeps(button, icon)).collect::<Vec<_>>();
        assert_eq!(kept(Some("ico_ep.bmp")), [0, 1]);
        assert_eq!(kept(Some("ICO_GQ.BMP")), [0, 1]);
        assert_eq!(kept(Some("ico_lq")), [0, 1]);
        assert_eq!(kept(Some("ico_sub.bmp")), [0, 2]);
        assert_eq!(kept(Some("ico_dq.bmp")), [0, 3]);
        assert_eq!(kept(Some("ico_ev.bmp")), [0, 4]);
        assert_eq!(kept(Some("ico_other.bmp")), [0, 5]);
        assert_eq!(kept(None), [0, 5]);
        // A plain case-sensitive substring of the title or summary; an empty search passes all.
        let text = QuestText { title: "Hunting Porings".into(), summary: "Poring x10".into(), ..QuestText::default() };
        assert!(found(Some(&text), "Hunt") && found(Some(&text), "x10") && found(Some(&text), ""));
        assert!(!found(Some(&text), "hunt") && !found(None, "Hunt") && found(None, ""));
    }

    #[test]
    fn narrows_every_tab_and_takes_twenty_characters() {
        let texts: HashMap<u32, QuestText> = [
            (1, QuestText { title: "Daily hunt".into(), icon: Some("ico_dq.bmp".into()), ..Default::default() }),
            (2, QuestText { title: "Story".into(), icon: Some("ico_ep.bmp".into()), ..Default::default() }),
        ]
        .into();
        let quests = [quest(1, true), quest(2, true), quest(3, false)];
        let mut window = QuestWindow::default();
        window.open();
        let ids = |window: &QuestWindow| window.shown(&quests, &texts).iter().map(|quest| quest.id).collect::<Vec<_>>();
        window.click(Hit::Filter(3), &quests, &texts);
        assert_eq!(ids(&window), [1]);
        window.click(Hit::Filter(1), &quests, &texts);
        assert_eq!(ids(&window), [2]);
        window.click(Hit::Filter(0), &quests, &texts);
        assert_eq!(ids(&window), [1, 2]);
        // Typed text applies on Enter (or the button), on every tab.
        window.type_text("Story");
        assert_eq!(ids(&window), [1, 2]);
        window.apply_search();
        assert_eq!(ids(&window), [2]);
        window.tab = 2;
        assert!(ids(&window).is_empty());
        window.type_text("abcdefghijklmnopqrstuvwxyz");
        assert_eq!(window.search.chars().count(), 20);
        // Opening again: All with an empty box.
        window.open();
        assert_eq!((window.filter, window.search.as_str(), window.applied.as_str()), (0, "", ""));
    }

    #[test]
    fn shows_a_linked_quest_on_its_tab() {
        let quests: Vec<Quest> = (1..=12).map(|id| quest(id, true)).chain([quest(20, false)]).collect();
        let texts = HashMap::new();
        let mut window = QuestWindow::default();
        // On hold, in view and picked; then the twelfth row of In progress, scrolled to.
        assert!(window.show_quest(20, &quests, &texts));
        assert_eq!((window.frame.open, window.tab, window.picked), (true, 2, Some(20)));
        assert!(window.show_quest(12, &quests, &texts));
        assert_eq!((window.tab, window.picked), (0, Some(12)));
        assert!((window.scroll..window.scroll + ROWS).contains(&11));
        // A quest no list has.
        assert!(!window.show_quest(99, &quests, &texts));
        assert_eq!(window.tab, 0);
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
    fn lays_the_details_out() {
        // Right of the list with the tops aligned; with the list shut, the middle of the screen.
        assert_eq!(details_place(Some([322.0, 151.0]), [1024.0, 768.0]), [703.0, 151.0]);
        assert_eq!(details_place(None, [1024.0, 768.0]), [341.0, 178.0]);
        // Description and Monster by their lines (the header alone when empty), each with 9 below; Reward 122.
        assert_eq!(panel_heights(3, 0), [30.0 + 48.0 + 9.0, 9.0 + 16.0 + 9.0, 122.0]);
        // The Deadline: the end time local, none for 0.
        let end = 1_792_398_605; // 2026-10-19 08:30:05 UTC
        assert_eq!(deadline_line("Deadline", end, 39_600).as_deref(), Some("Deadline [2026/10/19 19:30]"));
        assert_eq!(deadline_line("Deadline", 0, 39_600), None);
    }

    #[test]
    fn reads_quest_texts() {
        let texts = parse_texts("1#Ransom#SG_FEEL#QUE_NOIMAGE#\nTurtle#\nCarapace#\n\n101#Professor#SG_FEEL#QUE_NOIMAGE#\nGo up.#\nTalk#\n");
        let professor = QuestText { title: "Professor".into(), description: vec!["Go up.".into()], summary: "Talk".into(), ..QuestText::default() };
        assert_eq!(texts[&101], professor);
        assert_eq!(texts[&1].summary, "Carapace");
    }
}
