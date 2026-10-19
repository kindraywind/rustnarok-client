//! The guild storage's usage log (ZC_ACK_GUILDSTORAGE_LOG): a 400 × 250 window of who stored or took what and when,
//! ten rows a page, with All, In and Out tabs. A press anywhere on it drags it; only its Close button shuts it.

use std::collections::HashMap;

use rustnarok_client::grf::Grf;
use rustnarok_client::lua::ItemInfo;
use rustnarok_client::net::{GuildStorageEntry, InventoryItem};
use rustnarok_client::text::Font;

use crate::inventory::{self, CardNames};
use crate::render::{Gpu, Texture};
use crate::window::{self, Chrome, TITLE_HEIGHT};
use crate::{Layer, UI, contains, offset};

const SIZE: [f32; 2] = [400.0, 250.0];
/// "Guild Storage Logs", the tabs' All, In and Out, "Last 100 entries will be displayed.", and the heads.
const TITLE: usize = 2547;
const TABS: [usize; 3] = [2544, 2545, 2546];
const NOTE: usize = 2548;
const HEADS: [(usize, f32); 5] = [(2542, 6.0), (2538, 70.0), (2539, 150.0), (2540, 200.0), (2541, 320.0)];
/// "Log does not exist." and the line for any other refusal.
pub const NO_LOG: usize = 2543;
pub const LOG_FAILED: usize = 1816;
/// The tabs side by side at (0, 17), the picked one's label in #2396C8.
// ponytail: plain labels on white, 60 × 20 each; the game's tab images aren't settled.
const TABS_AT: [f32; 2] = [0.0, 17.0];
const TAB_SIZE: [f32; 2] = [60.0, 20.0];
const PICKED: [u8; 3] = [0x23, 0x96, 0xC8];
const NOTE_AT: [f32; 2] = [234.0, 23.0];
const RED: [u8; 3] = [0xFF, 0x00, 0x00];
const HEADS_Y: f32 = 47.0;
/// Ten rows from y 60, 15 apart: the in/out mark at x 6, the item at 50 (100 px), the amount at 150 (20), the
/// character at 190 (80), the time at 280 (130).
pub const ROWS: usize = 10;
const ROWS_TOP: f32 = 60.0;
const ROW_HEIGHT: f32 = 15.0;
const MARK_X: f32 = 6.0;
const COLUMNS: [(f32, f32); 4] = [(50.0, 100.0), (150.0, 20.0), (190.0, 80.0), (280.0, 130.0)];
/// The page at (175, 228); « at (160, 229) and » at (215, 229), 14 × 9; Close at (350, 225); a 29 px footer.
const PAGE_AT: [f32; 2] = [175.0, 228.0];
const BACK_AT: [f32; 2] = [160.0, 229.0];
const FORWARD_AT: [f32; 2] = [215.0, 229.0];
const ARROW_SIZE: [f32; 2] = [14.0, 9.0];
const CLOSE_AT: [f32; 2] = [350.0, 225.0];
const CLOSE_SIZE: [f32; 2] = [42.0, 20.0];
const FOOTER: f32 = 29.0;

/// Where the log opens on a screen this big: (W/2 − 300, H/2 − 225).
pub fn place(screen: [f32; 2]) -> [f32; 2] {
    [screen[0] / 2.0 - 300.0, screen[1] / 2.0 - 225.0]
}

/// How many pages `count` entries take, ten a page, at least one.
pub fn pages(count: usize) -> usize {
    count.div_ceil(ROWS).max(1)
}

/// "%02d / %02d": the page (from 1) of the pages.
pub fn page_text(page: usize, pages: usize) -> String {
    format!("{:02} / {:02}", page + 1, pages)
}

/// A row's texts: red for a damaged item, else black.
pub fn row_color(entry: &GuildStorageEntry) -> [u8; 3] {
    if entry.damaged { RED } else { [0, 0, 0] }
}

/// What the tabs list: everything, what was stored, what was taken.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    All,
    In,
    Out,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Hit {
    Tab(Tab),
    Back,
    Forward,
    Close,
    Window,
}

pub struct Assets {
    marks: [Option<Texture>; 2],
    back: [Option<Texture>; 3],
    forward: [Option<Texture>; 3],
    close: [Option<Texture>; 3],
    footer: [Option<Texture>; 3],
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let basic = format!("{UI}basic_interface\\");
        let image = |name: &str| gpu.try_load_texture(grf, &format!("{basic}{name}.bmp"), false).ok();
        let looks = |name: &str| ["", "_a", "_b"].map(|look| image(&format!("{name}{look}")));
        Self {
            marks: [image("in"), image("out")],
            back: looks("stickon"),
            forward: looks("stickoff"),
            close: looks("btn_close"),
            footer: ["left2", "mid2", "right2"].map(|part| image(&format!("btnbar_{part}"))),
        }
    }
}

pub struct Log {
    /// Newest first.
    pub entries: Vec<GuildStorageEntry>,
    pub position: [f32; 2],
    pub tab: Tab,
    pub page: usize,
    drag: Option<[f32; 2]>,
}

impl Log {
    /// The log with everything heard, newest first (by the time text), at its place for this screen.
    pub fn new(mut entries: Vec<GuildStorageEntry>, screen: [f32; 2]) -> Self {
        entries.sort_by(|a, b| b.time.cmp(&a.time));
        Self { entries, position: place(screen), tab: Tab::All, page: 0, drag: None }
    }

    /// The open tab's entries.
    pub fn listed(&self) -> Vec<&GuildStorageEntry> {
        self.entries
            .iter()
            .filter(|entry| match self.tab {
                Tab::All => true,
                Tab::In => entry.stored,
                Tab::Out => !entry.stored,
            })
            .collect()
    }

    pub fn pages(&self) -> usize {
        pages(self.listed().len())
    }

    pub fn page_text(&self) -> String {
        page_text(self.page, self.pages())
    }

    pub fn covers(&self, cursor: [f32; 2]) -> bool {
        contains(self.position, SIZE, cursor)
    }

    fn at(&self, local: [f32; 2]) -> [f32; 2] {
        offset(self.position, local)
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        if !self.covers(cursor) {
            return None;
        }
        let local = [cursor[0] - self.position[0], cursor[1] - self.position[1]];
        let tab = (0..3).find(|&tab| contains([TABS_AT[0] + tab as f32 * TAB_SIZE[0], TABS_AT[1]], TAB_SIZE, local));
        if let Some(tab) = tab {
            return Some(Hit::Tab([Tab::All, Tab::In, Tab::Out][tab]));
        }
        Some(match () {
            _ if contains(BACK_AT, ARROW_SIZE, local) => Hit::Back,
            _ if contains(FORWARD_AT, ARROW_SIZE, local) => Hit::Forward,
            _ if contains(CLOSE_AT, CLOSE_SIZE, local) => Hit::Close,
            _ => Hit::Window,
        })
    }

    /// A press: a tab (back to page 1), « and », or anywhere else to drag it. Whether it asks to close.
    pub fn press(&mut self, hit: Hit, cursor: [f32; 2]) -> bool {
        match hit {
            Hit::Tab(tab) => {
                self.tab = tab;
                self.page = 0;
            }
            Hit::Back => self.page = self.page.saturating_sub(1),
            Hit::Forward => self.page = (self.page + 1).min(self.pages() - 1),
            Hit::Close => return true,
            Hit::Window => self.drag = Some([cursor[0] - self.position[0], cursor[1] - self.position[1]]),
        }
        false
    }

    pub fn drag_to(&mut self, cursor: [f32; 2]) {
        if let Some(grab) = self.drag {
            self.position = window::dragged_to(cursor, grab);
        }
    }

    pub fn drop_drag(&mut self) -> bool {
        self.drag.take().is_some()
    }

    /// The tabs, arrows and Close on screen (for the test driver).
    pub fn button_boxes(&self) -> Vec<(String, [f32; 4])> {
        let place = |name: String, local: [f32; 2], size: [f32; 2]| {
            let [x, y] = self.at(local);
            (name, [x, y, size[0], size[1]])
        };
        let mut boxes: Vec<_> =
            (0..3).map(|tab| place(format!("storage_log_tab:{tab}"), [TABS_AT[0] + tab as f32 * TAB_SIZE[0], TABS_AT[1]], TAB_SIZE)).collect();
        boxes.push(place("storage_log_back".into(), BACK_AT, ARROW_SIZE));
        boxes.push(place("storage_log_forward".into(), FORWARD_AT, ARROW_SIZE));
        boxes.push(place("storage_log_close".into(), CLOSE_AT, CLOSE_SIZE));
        boxes
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &self,
        layer: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        items: &HashMap<u32, ItemInfo>,
        (messages, cards, makers): (&[String], &CardNames, &HashMap<u32, Option<String>>),
        font: &Font,
        pointer: ([f32; 2], bool),
    ) {
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        let [w, h] = SIZE;
        chrome.draw_title_bar(layer, self.position, w);
        layer.texts.push((message(TITLE).to_owned(), self.at([18.0, 2.0]), [255; 3]));
        layer.texts.push((message(TITLE).to_owned(), self.at([17.0, 1.0]), [0, 0, 0]));
        layer.sprites.push((&chrome.white, self.at([0.0, TITLE_HEIGHT]), [w, h - TITLE_HEIGHT - FOOTER]));
        for (number, &label) in TABS.iter().enumerate() {
            let picked = [Tab::All, Tab::In, Tab::Out][number] == self.tab;
            let at = [TABS_AT[0] + number as f32 * TAB_SIZE[0] + 6.0, TABS_AT[1] + 4.0];
            let label = window::fit(font, message(label), TAB_SIZE[0] - 8.0);
            layer.texts.push((label, self.at(at), if picked { PICKED } else { [0, 0, 0] }));
        }
        layer.texts.push((message(NOTE).to_owned(), self.at(NOTE_AT), RED));
        for (id, x) in HEADS {
            layer.texts.push((message(id).to_owned(), self.at([x, HEADS_Y]), [0, 0, 0]));
        }
        for (row, entry) in self.listed().into_iter().skip(self.page * ROWS).take(ROWS).enumerate() {
            let y = ROWS_TOP + row as f32 * ROW_HEIGHT;
            if let Some(mark) = &assets.marks[usize::from(!entry.stored)] {
                layer.sprites.push((mark, self.at([MARK_X, y]), mark.size));
            }
            let item = InventoryItem {
                id: entry.item,
                amount: entry.amount.unsigned_abs().min(u32::from(u16::MAX)) as u16,
                refine: entry.refine.clamp(0, 255) as u8,
                cards: entry.cards,
                identified: entry.identified,
                kind: u8::try_from(entry.kind).unwrap_or(u8::MAX),
                ..Default::default()
            };
            let (name, _) = inventory::full_name_maker(items, cards, messages, makers, &item, false);
            let color = row_color(entry);
            for ((x, width), text) in COLUMNS.into_iter().zip([name, entry.amount.to_string(), entry.name.clone(), entry.time.clone()]) {
                layer.texts.push((window::fit(font, &text, width), self.at([x, y]), color));
            }
        }
        let [left, middle, right] = &assets.footer;
        let footer_top = h - FOOTER;
        if let (Some(left), Some(middle), Some(right)) = (left, middle, right) {
            layer.sprites.push((left, self.at([0.0, footer_top]), left.size));
            layer.sprites.push((middle, self.at([left.size[0], footer_top]), [w - left.size[0] - right.size[0], FOOTER]));
            layer.sprites.push((right, self.at([w - right.size[0], footer_top]), right.size));
        }
        layer.texts.push((self.page_text(), self.at(PAGE_AT), [0, 0, 0]));
        let hit = self.hit(pointer.0);
        let look = |target: Hit| {
            let hovered = hit == Some(target);
            usize::from(hovered) + usize::from(hovered && pointer.1)
        };
        for (images, place, target, size) in [
            (&assets.back, BACK_AT, Hit::Back, ARROW_SIZE),
            (&assets.forward, FORWARD_AT, Hit::Forward, ARROW_SIZE),
            (&assets.close, CLOSE_AT, Hit::Close, CLOSE_SIZE),
        ] {
            if let Some(image) = images[look(target)].as_ref().or(images[0].as_ref()) {
                layer.sprites.push((image, self.at(place), size));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(stored: bool, time: &str) -> GuildStorageEntry {
        GuildStorageEntry { stored, time: time.into(), item: 501, amount: 1, ..Default::default() }
    }

    #[test]
    fn splits_in_and_out_and_pages_them() {
        let mut log = Log::new(vec![entry(true, "10:00"), entry(false, "12:00"), entry(true, "11:00")], [1024.0, 768.0]);
        assert_eq!(log.position, [212.0, 159.0]);
        // Newest first by the time text.
        assert_eq!(log.entries.iter().map(|entry| entry.time.as_str()).collect::<Vec<_>>(), ["12:00", "11:00", "10:00"]);
        log.press(Hit::Tab(Tab::In), [0.0; 2]);
        assert!(log.listed().iter().all(|entry| entry.stored) && log.listed().len() == 2);
        log.press(Hit::Tab(Tab::Out), [0.0; 2]);
        assert_eq!(log.listed().len(), 1);
        // Pages: 0 and 10 entries one, 11 two; no cap.
        assert_eq!([0, 10, 11, 101].map(pages), [1, 1, 2, 11]);
        assert_eq!((page_text(0, 1), page_text(1, 2)), ("01 / 01".to_owned(), "02 / 02".to_owned()));
        let mut many = Log::new((0..11).map(|minute| entry(minute % 2 == 0, &format!("10:{minute:02}"))).collect(), [1024.0, 768.0]);
        assert_eq!(many.page_text(), "01 / 02");
        many.press(Hit::Forward, [0.0; 2]);
        many.press(Hit::Forward, [0.0; 2]);
        assert_eq!(many.page_text(), "02 / 02");
        // A tab change goes back to page 1.
        many.press(Hit::Tab(Tab::In), [0.0; 2]);
        assert_eq!(many.page_text(), "01 / 01");
        many.press(Hit::Back, [0.0; 2]);
        assert_eq!(many.page, 0);
    }

    #[test]
    fn reds_a_damaged_row_and_hits_its_buttons() {
        assert_eq!((row_color(&entry(true, "")), row_color(&GuildStorageEntry { damaged: true, ..entry(true, "") })), ([0, 0, 0], RED));
        let log = Log::new(Vec::new(), [1024.0, 768.0]);
        let at = |local: [f32; 2]| [212.0 + local[0], 159.0 + local[1]];
        assert_eq!(log.hit(at([10.0, 20.0])), Some(Hit::Tab(Tab::All)));
        assert_eq!(log.hit(at([70.0, 20.0])), Some(Hit::Tab(Tab::In)));
        assert_eq!(log.hit(at([165.0, 232.0])), Some(Hit::Back));
        assert_eq!(log.hit(at([220.0, 232.0])), Some(Hit::Forward));
        assert_eq!(log.hit(at([360.0, 230.0])), Some(Hit::Close));
        assert_eq!(log.hit(at([200.0, 120.0])), Some(Hit::Window));
        assert_eq!(log.hit(at([401.0, 120.0])), None);
    }
}
