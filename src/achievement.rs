//! The achievement window (the basic info menu's button, /achievement): the score and rank along the top and tabs down
//! the left; the summary (the rank's bar, how much of each tab is done, the latest done) or a tab's achievements, with
//! the picked one's details and its reward to claim.

use std::collections::HashMap;

use rustnarok_client::grf::Grf;
use rustnarok_client::lua::{AchievementInfo, ItemInfo};
use rustnarok_client::messages;
use rustnarok_client::net::{Achievement, AchievementTotals};
use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};
use crate::window::{self, Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, SCREEN, UI, contains, inventory, offset};

const TITLE: usize = 2646;
const SCORE: usize = 2647;
const RANK: usize = 2648;
const TO_NEXT: usize = 2649;
const PROGRESS: usize = 2650;
const RECENT: usize = 2651;
const SUMMARY: usize = 2656;
/// "< %s > achieved.": an achievement just done.
pub const ACHIEVED: usize = 2681;
/// The tabs after the summary: each one's name, and its sub-tabs' first name and how many (minor 0 first).
const MAJORS: [(usize, usize, usize); 6] = [(2657, 2658, 3), (2661, 2662, 6), (2668, 2669, 2), (2671, 2672, 2), (2674, 2675, 3), (2678, 0, 0)];

// ponytail: the bars and boxes are measured from achievement_re's backgrounds; everything no picture shows (the texts,
// the tabs' and rows' contents, the rewards, the claim button's place) is an estimate. Calibrate against the real
// client.
const WIDTH: f32 = 750.0;
const UPPER: f32 = 46.0;
const BODY_TOP: f32 = TITLE_HEIGHT + UPPER;
pub const HEIGHT: f32 = BODY_TOP + 405.0;
const TABS_WIDTH: f32 = 102.0;
const TAB: [f32; 2] = [100.0, 40.0];
const SUB_TAB: [f32; 2] = [100.0, 26.0];
/// Tabs overlap a little so the longest tab's sub-tabs fit.
const TAB_STEP: f32 = 38.0;
const SUB_STEP: f32 = 23.0;
const ROW: [f32; 2] = [309.0, 81.0];
const ROWS: usize = 5;
const DETAIL_LEFT: f32 = TABS_WIDTH + ROW[0];
const DETAIL_TEXT: f32 = 300.0;
const CLAIM: [f32; 2] = [148.0, 20.0];
const CLAIM_AT: [f32; 2] = [DETAIL_LEFT + 90.0, BODY_TOP + 378.0];
/// The summary's bars over its background: the rank's (inside its border), then each tab's in two columns of three.
const RANK_BAR: [f32; 4] = [47.0, 51.0, 550.0, 6.0];
const BAR_LEFTS: [f32; 2] = [47.0, 351.0];
const BAR_TOPS: [f32; 3] = [125.0, 175.0, 225.0];
const BAR: [f32; 2] = [246.0, 6.0];
const RECENT_BOXES: [f32; 2] = [9.0, 325.0];
const RECENT_TOP: f32 = 310.0;
const TEXT: [u8; 3] = [0, 0, 0];
const GREY: [u8; 3] = [0x70, 0x70, 0x70];
const NAVY: [u8; 4] = [0x73, 0x8A, 0xC6, 0xFF];

pub struct Assets {
    upper: Texture,
    tabs: Texture,
    summary: Texture,
    list: Texture,
    detail: Texture,
    /// A row, not done and done; each picked.
    rows: [[Texture; 2]; 2],
    /// Tabs and sub-tabs: out, over, pressed.
    tab: [Texture; 3],
    sub_tab: [Texture; 3],
    badge: Texture,
    /// The reward box: nothing waiting, a reward waiting.
    reward_box: [Texture; 2],
    /// The rewards: an item, a title, a buff.
    rewards: [Texture; 3],
    /// Claim: out, over, pressed, disabled.
    claim: [Texture; 4],
    bullet: Texture,
    fill: Texture,
    white: Texture,
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |name: &str| gpu.load_texture(grf, &format!("{UI}achievement_re\\{name}.bmp"), false);
        Self {
            upper: image("bg_upper"),
            tabs: image("bg_tab"),
            summary: image("bg_summary"),
            list: image("bg_list"),
            detail: image("bg_detail"),
            rows: [["list_out", "list_press"].map(image), ["list_complete_out", "list_complete_press"].map(image)],
            tab: ["tab_out", "tab_over", "tab_press"].map(image),
            sub_tab: ["tab_sub_out", "tab_sub_over", "tab_sub_press"].map(image),
            badge: image("badge_complete"),
            // ponytail: the faded box read as nothing to take, the bright one as a reward waiting.
            reward_box: ["list_rewardbox_not_receive", "list_rewardbox_default"].map(image),
            rewards: ["reward_item", "reward_title", "reward_buff"].map(image),
            claim: ["btn_receive_out", "btn_receive_over", "btn_receive_press", "btn_receive_disable"].map(image),
            bullet: image("detail_bullet"),
            fill: gpu.solid(NAVY),
            white: gpu.solid([255; 4]),
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(TitleHit),
    /// 0 the summary, 1–6 the tabs.
    Tab(usize),
    Sub(usize),
    /// An achievement's row, by ID.
    Row(u32),
    Claim,
    Window,
}

pub struct AchievementWindow {
    pub frame: Frame,
    /// 0 the summary, 1–6 the tabs (an achievement's `major`), and the sub-tab (its `minor`).
    pub tab: usize,
    pub sub: usize,
    scroll: usize,
    /// The achievement whose details show.
    pub picked: Option<u32>,
}

impl Default for AchievementWindow {
    fn default() -> Self {
        Self { frame: Frame::new([((SCREEN[0] - WIDTH) / 2.0).floor(), 100.0], WIDTH), tab: 0, sub: 0, scroll: 0, picked: None }
    }
}

/// How many of an achievement's objectives are done, of how many.
fn objectives_done(info: &AchievementInfo, progress: Option<&Achievement>) -> (usize, usize) {
    let total = info.objectives.len().max(1);
    let Some(progress) = progress else { return (0, total) };
    if progress.completed {
        return (total, total);
    }
    // ponytail: a checklist's objective counts as done once its count is above 0.
    let done = info.objectives.iter().zip(progress.counts).filter(|((_, target), count)| *count >= (*target).max(1)).count();
    (done, total)
}

/// A completed achievement with its reward still waiting.
pub fn claimable(progress: Option<&Achievement>) -> bool {
    progress.is_some_and(|progress| progress.completed && !progress.rewarded)
}

impl AchievementWindow {
    /// The open tab's achievements, by ID.
    fn shown(&self, table: &HashMap<u32, AchievementInfo>) -> Vec<u32> {
        let mut shown: Vec<u32> =
            table.iter().filter(|(_, info)| usize::from(info.major) == self.tab && usize::from(info.minor) == self.sub).map(|(&id, _)| id).collect();
        shown.sort_unstable();
        shown
    }

    /// The tabs down the left, with the open one's sub-tabs under it: what each is and where.
    fn tab_places(&self) -> Vec<(Hit, [f32; 2])> {
        let mut places = Vec::new();
        let mut top = BODY_TOP + 2.0;
        for tab in 0..=MAJORS.len() {
            places.push((Hit::Tab(tab), [1.0, top]));
            top += TAB_STEP;
            if tab == self.tab && tab > 0 {
                for sub in 0..MAJORS[tab - 1].2 {
                    places.push((Hit::Sub(sub), [1.0, top]));
                    top += SUB_STEP;
                }
            }
        }
        places
    }

    pub fn covers(&self, cursor: [f32; 2]) -> bool {
        self.frame.local(cursor, HEIGHT).is_some()
    }

    pub fn hit(&self, cursor: [f32; 2], table: &HashMap<u32, AchievementInfo>, progress: &HashMap<u32, Achievement>) -> Option<Hit> {
        let local = self.frame.local(cursor, HEIGHT)?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(Hit::Title(hit));
        }
        if self.frame.minimized {
            return Some(Hit::Window);
        }
        // The last tab drawn is on top.
        let tab = self.tab_places().into_iter().rev().find(|(hit, place)| contains(*place, if matches!(hit, Hit::Tab(_)) { TAB } else { SUB_TAB }, local));
        if let Some((hit, _)) = tab {
            return Some(hit);
        }
        if self.tab > 0 && contains([TABS_WIDTH, BODY_TOP], [ROW[0], ROW[1] * ROWS as f32], local) {
            let row = ((local[1] - BODY_TOP) / ROW[1]) as usize + self.scroll;
            return Some(self.shown(table).get(row).map_or(Hit::Window, |&id| Hit::Row(id)));
        }
        if self.tab > 0 && claimable(self.picked.and_then(|id| progress.get(&id))) && contains(CLAIM_AT, CLAIM, local) {
            return Some(Hit::Claim);
        }
        Some(Hit::Window)
    }

    /// Reacts to a click; returns the achievement whose reward to claim.
    pub fn click(&mut self, hit: Hit) -> Option<u32> {
        match hit {
            Hit::Title(title) => self.frame.click_title(title),
            Hit::Tab(tab) => (self.tab, self.sub, self.scroll, self.picked) = (tab, 0, 0, None),
            Hit::Sub(sub) => (self.sub, self.scroll, self.picked) = (sub, 0, None),
            Hit::Row(id) => self.picked = Some(id),
            Hit::Claim => return self.picked,
            Hit::Window => {}
        }
        None
    }

    pub fn scroll_by(&mut self, rows: i32, table: &HashMap<u32, AchievementInfo>) {
        let most = self.shown(table).len().saturating_sub(ROWS);
        self.scroll = (self.scroll as i32 + rows).clamp(0, most as i32) as usize;
    }

    /// The tabs, rows and claim button on screen, named for scripted clicks: `tab:<n>`, `sub:<n>`,
    /// `achievement:<id>`, `claim`.
    pub fn button_boxes(&self, table: &HashMap<u32, AchievementInfo>, progress: &HashMap<u32, Achievement>) -> Vec<(String, [f32; 4])> {
        if !self.frame.open || self.frame.minimized {
            return Vec::new();
        }
        let place = |local: [f32; 2], size: [f32; 2]| {
            let [x, y] = self.frame.at(local);
            [x, y, size[0], size[1]]
        };
        let mut boxes: Vec<(String, [f32; 4])> = self
            .tab_places()
            .into_iter()
            .map(|(hit, at)| match hit {
                Hit::Tab(tab) => (format!("tab:{tab}"), place(at, TAB)),
                Hit::Sub(sub) => (format!("sub:{sub}"), place(at, SUB_TAB)),
                _ => unreachable!(),
            })
            .collect();
        if self.tab > 0 {
            for (row, id) in self.shown(table).into_iter().enumerate().skip(self.scroll).take(ROWS) {
                boxes.push((format!("achievement:{id}"), place([TABS_WIDTH, BODY_TOP + (row - self.scroll) as f32 * ROW[1]], ROW)));
            }
            if claimable(self.picked.and_then(|id| progress.get(&id))) {
                boxes.push(("claim".to_owned(), place(CLAIM_AT, CLAIM)));
            }
        }
        boxes
    }

    /// The group icons the open tab shows.
    pub fn icons(&self, table: &HashMap<u32, AchievementInfo>) -> Vec<String> {
        if !self.frame.open || self.tab == 0 {
            return Vec::new();
        }
        self.shown(table).iter().filter_map(|id| table.get(id)).map(|info| icon_path(&info.group)).collect()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &self,
        layer: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        icons: &'a HashMap<String, Option<Texture>>,
        font: &Font,
        messages: &[String],
        table: &HashMap<u32, AchievementInfo>,
        progress: &HashMap<u32, Achievement>,
        totals: &AchievementTotals,
        titles: &HashMap<u32, String>,
        items: &HashMap<u32, ItemInfo>,
        pointer: ([f32; 2], bool),
    ) {
        if !self.frame.open {
            return;
        }
        let frame = &self.frame;
        let at = |local: [f32; 2]| frame.at(local);
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        frame.draw_title(layer, chrome, message(TITLE), pointer.0);
        if frame.minimized {
            return;
        }
        let hit = self.hit(pointer.0, table, progress);
        let look = |target: Hit| usize::from(hit == Some(target)) + usize::from(hit == Some(target) && pointer.1);

        // The score and rank along the top, the rank on the gold circle.
        layer.sprites.push((&assets.upper, at([0.0, TITLE_HEIGHT]), assets.upper.size));
        let level = totals.level.to_string();
        layer.texts.push((level.clone(), at([23.0 - font.width(&level) / 2.0, TITLE_HEIGHT + 17.0]), TEXT));
        layer.texts.push((format!("{} {}", message(SCORE), totals.score), at([48.0, TITLE_HEIGHT + 8.0]), TEXT));
        layer.texts.push((format!("{} {}", message(RANK), totals.level), at([48.0, TITLE_HEIGHT + 26.0]), TEXT));

        // The tabs; the open one pressed, with its sub-tabs under it.
        layer.sprites.push((&assets.tabs, at([0.0, BODY_TOP]), assets.tabs.size));
        for (place, at_tab) in self.tab_places() {
            let (images, label, open, text_at) = match place {
                Hit::Tab(tab) => (&assets.tab, if tab == 0 { SUMMARY } else { MAJORS[tab - 1].0 }, tab == self.tab, [10.0, 13.0]),
                Hit::Sub(sub) => (&assets.sub_tab, MAJORS[self.tab - 1].1 + sub, sub == self.sub, [16.0, 6.0]),
                _ => continue,
            };
            let image = &images[if open { 2 } else { look(place).min(1) }];
            layer.sprites.push((image, at(at_tab), image.size));
            layer.texts.push((window::fit(font, message(label), 84.0), at(offset(at_tab, text_at)), TEXT));
        }

        if self.tab == 0 {
            self.draw_summary(layer, assets, font, messages, table, progress, totals);
        } else {
            self.draw_list(layer, assets, icons, font, messages, table, progress, titles, items, look(Hit::Claim));
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_summary<'a>(
        &self,
        layer: &mut Layer<'a>,
        assets: &'a Assets,
        font: &Font,
        messages: &[String],
        table: &HashMap<u32, AchievementInfo>,
        progress: &HashMap<u32, Achievement>,
        totals: &AchievementTotals,
    ) {
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        let corner = [TABS_WIDTH, BODY_TOP];
        let at = |local: [f32; 2]| self.frame.at(offset(corner, local));
        let bar = |layer: &mut Layer<'a>, place: [f32; 4], share: f32| {
            layer.sprites.push((&assets.fill, at([place[0], place[1]]), [(place[2] * share.clamp(0.0, 1.0)).round(), place[3]]));
        };
        layer.sprites.push((&assets.summary, self.frame.at(corner), assets.summary.size));
        // The background's bars come part filled: white first.
        for place in std::iter::once(RANK_BAR).chain(BAR_TOPS.iter().flat_map(|&top| BAR_LEFTS.map(|left| [left, top, BAR[0], BAR[1]]))) {
            layer.sprites.push((&assets.white, at([place[0], place[1]]), [place[2], place[3]]));
        }

        // The rank: its bar, and what the next one needs.
        layer.texts.push((format!("{} {}", message(RANK), totals.level), at([80.0, 26.0]), TEXT));
        let next = messages::fill(message(TO_NEXT), &[&totals.needed.saturating_sub(totals.points)]);
        layer.texts.push((next.clone(), at([RANK_BAR[0] + RANK_BAR[2] - font.width(&next), 34.0]), GREY));
        bar(layer, RANK_BAR, totals.points as f32 / totals.needed.max(1) as f32);

        // Each tab: how many of its achievements are done.
        for (tab, &(label, ..)) in MAJORS.iter().enumerate() {
            let (left, top) = (BAR_LEFTS[tab / 3], BAR_TOPS[tab % 3]);
            let ids = table.iter().filter(|(_, info)| usize::from(info.major) == tab + 1).map(|(id, _)| id);
            let (done, total) =
                ids.fold((0, 0), |(done, total), id| (done + usize::from(progress.get(id).is_some_and(|achievement| achievement.completed)), total + 1));
            layer.texts.push((message(label).to_owned(), at([left + 33.0, top - 26.0]), TEXT));
            let count = format!("{done} / {total}");
            layer.texts.push((count.clone(), at([left + BAR[0] - font.width(&count), top - 14.0]), GREY));
            bar(layer, [left, top, BAR[0], BAR[1]], done as f32 / total.max(1) as f32);
        }

        // The two latest done.
        layer.texts.push((message(RECENT).to_owned(), at([12.0, RECENT_TOP - 20.0]), TEXT));
        let mut recent: Vec<&Achievement> = progress.values().filter(|achievement| achievement.completed).collect();
        recent.sort_by_key(|achievement| std::cmp::Reverse((achievement.completed_at, achievement.id)));
        for (left, achievement) in RECENT_BOXES.into_iter().zip(recent) {
            let info = table.get(&achievement.id);
            let title = info.map_or_else(|| achievement.id.to_string(), |info| info.title.clone());
            layer.texts.push((window::fit(font, &title, 290.0), at([left + 10.0, RECENT_TOP + 13.0]), TEXT));
            let summary = info.map_or("", |info| info.summary.as_str());
            for (line, text) in window::wrap(font, summary, 290.0, 2).into_iter().enumerate() {
                layer.texts.push((text, at([left + 10.0, RECENT_TOP + 46.0 + line as f32 * 14.0]), GREY));
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_list<'a>(
        &self,
        layer: &mut Layer<'a>,
        assets: &'a Assets,
        icons: &'a HashMap<String, Option<Texture>>,
        font: &Font,
        messages: &[String],
        table: &HashMap<u32, AchievementInfo>,
        progress: &HashMap<u32, Achievement>,
        titles: &HashMap<u32, String>,
        items: &HashMap<u32, ItemInfo>,
        claim_look: usize,
    ) {
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        let at = |local: [f32; 2]| self.frame.at(local);
        layer.sprites.push((&assets.list, at([TABS_WIDTH, BODY_TOP]), assets.list.size));
        layer.sprites.push((&assets.detail, at([DETAIL_LEFT, BODY_TOP]), assets.detail.size));

        // The tab's achievements: icon, title, summary, progress; done ones badged, a reward box when there's one.
        for (row, id) in self.shown(table).into_iter().enumerate().skip(self.scroll).take(ROWS) {
            let Some(info) = table.get(&id) else { continue };
            let achievement = progress.get(&id);
            let done = achievement.is_some_and(|achievement| achievement.completed);
            let top = BODY_TOP + (row - self.scroll) as f32 * ROW[1];
            let image = &assets.rows[usize::from(done)][usize::from(self.picked == Some(id))];
            layer.sprites.push((image, at([TABS_WIDTH, top]), image.size));
            if let Some(Some(icon)) = icons.get(&icon_path(&info.group)) {
                layer.sprites.push((icon, at([TABS_WIDTH + 8.0, top + 10.0]), icon.size));
            }
            layer.texts.push((window::fit(font, &info.title, 210.0), at([TABS_WIDTH + 54.0, top + 10.0]), TEXT));
            layer.texts.push((window::fit(font, &info.summary, 240.0), at([TABS_WIDTH + 54.0, top + 30.0]), GREY));
            let (objectives, total) = objectives_done(info, achievement);
            layer.texts.push((format!("{} {objectives} / {total}", message(PROGRESS)), at([TABS_WIDTH + 54.0, top + 54.0]), GREY));
            if done {
                layer.sprites.push((&assets.badge, at([TABS_WIDTH + 280.0, top + 6.0]), assets.badge.size));
            }
            if info.reward_item != 0 || info.reward_title != 0 || info.reward_buff != 0 {
                let reward = &assets.reward_box[usize::from(claimable(achievement))];
                layer.sprites.push((reward, at([TABS_WIDTH + 270.0, top + 50.0]), reward.size));
            }
        }

        // The picked one: title, details, objectives, rewards, and the claim button.
        let Some((id, info)) = self.picked.and_then(|id| Some((id, table.get(&id)?))) else { return };
        let achievement = progress.get(&id);
        let left = DETAIL_LEFT + 12.0;
        layer.texts.push((window::fit(font, &info.title, DETAIL_TEXT), at([left, BODY_TOP + 12.0]), TEXT));
        for (line, text) in window::wrap(font, &info.details, DETAIL_TEXT, 3).into_iter().enumerate() {
            layer.texts.push((text, at([left, BODY_TOP + 44.0 + line as f32 * 14.0]), GREY));
        }
        for (objective, (text, target)) in info.objectives.iter().enumerate().take(10) {
            let top = BODY_TOP + 100.0 + objective as f32 * 18.0;
            layer.sprites.push((&assets.bullet, at([left, top + 5.0]), assets.bullet.size));
            layer.texts.push((window::fit(font, text, DETAIL_TEXT - 70.0), at([left + 10.0, top]), TEXT));
            let count = achievement.map_or(0, |achievement| achievement.counts[objective]);
            let state = if *target > 0 { format!("{} / {target}", count.min(*target)) } else { String::new() };
            layer.texts.push((state.clone(), at([left + DETAIL_TEXT - font.width(&state), top]), GREY));
        }
        let rewards = [
            (info.reward_item, inventory::item_name(items, info.reward_item, true, 0)),
            (info.reward_title, titles.get(&info.reward_title).cloned().unwrap_or_default()),
            (info.reward_buff, String::new()),
        ];
        let shown = rewards.iter().enumerate().filter(|(_, (id, _))| *id != 0);
        for (place, (kind, (_, name))) in shown.enumerate() {
            let corner = [left + place as f32 * 104.0, BODY_TOP + 296.0];
            layer.sprites.push((&assets.rewards[kind], at(corner), assets.rewards[kind].size));
            layer.texts.push((window::fit(font, name, 100.0), at(offset(corner, [0.0, 64.0])), TEXT));
        }
        let claim = &assets.claim[if claimable(achievement) { claim_look } else { 3 }];
        layer.sprites.push((claim, at(CLAIM_AT), CLAIM));
    }
}

/// A group's icon.
fn icon_path(group: &str) -> String {
    format!("{UI}achievement_re\\icon_{group}.bmp")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabs_lists_and_claims() {
        let info = |major: u8, minor: u8| AchievementInfo { major, minor, objectives: vec![("x".into(), 3)], ..Default::default() };
        let table = HashMap::from([(200005, info(1, 0)), (200000, info(1, 0)), (200100, info(1, 1)), (120001, info(2, 0))]);
        let mut window = AchievementWindow::default();
        window.click(Hit::Tab(1));
        assert_eq!(window.shown(&table), [200000, 200005]);
        window.click(Hit::Sub(1));
        assert_eq!(window.shown(&table), [200100]);
        // Opening a tab shows its sub-tabs under it, and the next tabs move down past them.
        let places = window.tab_places();
        assert_eq!(places.iter().filter(|(hit, _)| matches!(hit, Hit::Sub(_))).count(), 3);
        assert_eq!(places[5], (Hit::Tab(2), [1.0, BODY_TOP + 2.0 + 2.0 * TAB_STEP + 3.0 * SUB_STEP]));

        // Only a completed achievement whose reward waits can be claimed.
        let done = |rewarded: bool| Achievement { id: 200005, completed: true, rewarded, ..Default::default() };
        assert!(claimable(Some(&done(false))));
        assert!(!claimable(Some(&done(true))));
        assert!(!claimable(None));
        window.click(Hit::Row(200005));
        assert_eq!(window.click(Hit::Claim), Some(200005));
        assert_eq!(objectives_done(&info(1, 0), Some(&Achievement { counts: [2, 0, 0, 0, 0, 0, 0, 0, 0, 0], ..Default::default() })), (0, 1));
        assert_eq!(objectives_done(&info(1, 0), Some(&Achievement { counts: [3, 0, 0, 0, 0, 0, 0, 0, 0, 0], ..Default::default() })), (1, 1));
    }
}
