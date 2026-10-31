//! The big minimap: the current map's overview 1:1 on a grey pane, the player's head, party, guild, quest NPCs and the
//! boss, the Quest / Facility / Guild/Party boxes, and memos the player records.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use image::RgbaImage;
use rustnarok_client::grf::Grf;
use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};
use crate::window::{self, Chrome, Frame, TitleHit};
use crate::{Layer, UI, contains, offset, screen};

pub const WIDTH: f32 = 516.0;
pub const HEIGHT: f32 = 559.0;
/// The boxes' labels and tooltips (Quest, Facility, Guild/Party), Record's and the icons' tooltips, Store, and the
/// line for a map whose big minimap isn't shown.
const CHECK_LABELS: [usize; 3] = [2742, 2744, 2746];
const CHECK_TIPS: [usize; 3] = [2743, 2745, 2747];
pub const RECORD: usize = 2748;
const BOSS_TIP: usize = 2749;
const PLAYER_TIP: usize = 2750;
const QUEST_TIP: usize = 2752;
pub const STORE: usize = 2754;
pub const NO_PICTURE: usize = 2767;

/// The pane: #AAAAAA at (2, 19), 512 across, the picture 1:1 over it.
const MAP_AT: [f32; 2] = [2.0, 19.0];
const MAP_SIDE: f32 = 512.0;
const PANE: [u8; 4] = [0xAA, 0xAA, 0xAA, 0xFF];
/// The bottom row: the three boxes (100×12 each, the 10×10 checkbox at their left), Record (100×20).
const CHECK_AT: [[f32; 2]; 3] = [[20.0, 540.0], [120.0, 540.0], [220.0, 540.0]];
const CHECK: [f32; 2] = [100.0, 12.0];
const RECORD_AT: [f32; 2] = [396.0, 536.0];
const RECORD_SIZE: [f32; 2] = [100.0, 20.0];
/// ponytail: naming a memo takes the boxes' row for its field and Store.
const FIELD_AT: [f32; 2] = [20.0, 536.0];
const FIELD_SIZE: [f32; 2] = [270.0, 20.0];
const STORE_AT: [f32; 2] = [300.0, 536.0];
const STORE_SIZE: [f32; 2] = [80.0, 20.0];
const NAME_MOST: usize = 40;
/// The icons: the player's head 26×26, the boss 24×24, party and guild and quest marks around 12.
const HEAD: f32 = 26.0;
const BOSS: f32 = 24.0;
/// The quest effects with a picture of their own (`quest_<effect>_<colour>.bmp`).
const QUEST_EFFECTS: std::ops::RangeInclusive<u16> = 0..=12;

/// Where the window first opens: 536 left of the screen's right edge, 190 down.
pub fn default_position() -> [f32; 2] {
    [screen()[0] - 536.0, 190.0]
}

/// The title: "%s ( %s ) %d %d", the map's display name, its name and the player's cell.
pub fn title(display: &str, map: &str, cell: [i32; 2]) -> String {
    format!("{display} ( {map} ) {} {}", cell[0], cell[1])
}

/// A cell on the pane: s = 512 / max(w, h); x = 2 + cx·s, y = 531 − cy·s, the shorter side centred.
pub fn point(cell: [i32; 2], size: [u32; 2]) -> [f32; 2] {
    let [x, y] = crate::minimap::share(cell, size);
    [MAP_AT[0] + x * MAP_SIDE, MAP_AT[1] + y * MAP_SIDE]
}

/// The cell under a point in the window, or none outside the map (the letterbox around a map that isn't square).
pub fn cell_at(local: [f32; 2], size: [u32; 2]) -> Option<[i32; 2]> {
    let share = [(local[0] - MAP_AT[0]) / MAP_SIDE, (local[1] - MAP_AT[1]) / MAP_SIDE];
    if share.iter().any(|value| !(0.0..1.0).contains(value)) {
        return None;
    }
    let longest = size[0].max(size[1]).max(1) as f32;
    let start = [(longest - size[0] as f32) / 2.0, (longest - size[1] as f32) / 2.0];
    let cell = [share[0] * longest - start[0], (1.0 - share[1]) * longest - start[1]].map(|value| value.round() as i32);
    (cell[0] >= 0 && cell[1] >= 0 && (cell[0] as u32) < size[0] && (cell[1] as u32) < size[1]).then_some(cell)
}

/// The maps with no big minimap (`data\ExceptionMinimapNameTable.txt`): a name a line, each ending in `#`.
pub fn exception_maps(text: &str) -> HashSet<String> {
    text.lines().filter_map(|line| line.trim().strip_suffix('#')).map(|name| name.trim().to_ascii_lowercase()).filter(|name| !name.is_empty()).collect()
}

pub fn load_exceptions(grf: &Grf) -> HashSet<String> {
    grf.read("data\\ExceptionMinimapNameTable.txt").map(|bytes| exception_maps(&String::from_utf8_lossy(&bytes))).unwrap_or_default()
}

/// The three boxes as kept with the window places: "minimap_checks <quest> <facility> <guild/party>", 1 on.
pub const CHECKS_LINE: &str = "minimap_checks";

pub fn checks_line(checks: [bool; 3]) -> String {
    format!("{CHECKS_LINE} {} {} {}\n", u8::from(checks[0]), u8::from(checks[1]), u8::from(checks[2]))
}

/// The boxes from a saved line's three numbers (as `parse_window_places` reads them).
pub fn checks_from(([quest, facility], party): ([f32; 2], bool)) -> [bool; 3] {
    [quest != 0.0, facility != 0.0, party]
}

/// A memo the player stored on a map.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Memo {
    pub cell: [i32; 2],
    pub name: String,
}

/// `map x y name` per line. A name may contain spaces. Lines that don't parse are skipped.
pub fn decode_memos(text: &str) -> Vec<(String, Memo)> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }
            let mut parts = line.splitn(4, ' ');
            let map = parts.next()?.to_owned();
            let x = parts.next()?.parse().ok()?;
            let y = parts.next()?.parse().ok()?;
            Some((map, Memo { cell: [x, y], name: parts.next().unwrap_or("").to_owned() }))
        })
        .collect()
}

pub fn encode_memos(rows: &[(String, Memo)]) -> String {
    let mut text = String::new();
    for (map, memo) in rows {
        text.push_str(&format!("{map} {} {} {}\n", memo.cell[0], memo.cell[1], memo.name));
    }
    text
}

/// `text` with `map`'s memos replaced by `memos`. Other maps stay.
pub fn replace_map(text: &str, map: &str, memos: &[Memo]) -> String {
    let mut rows: Vec<_> = decode_memos(text).into_iter().filter(|(name, _)| name != map).collect();
    rows.extend(memos.iter().cloned().map(|memo| (map.to_owned(), memo)));
    encode_memos(&rows)
}

pub fn load_memos(path: &Path, map: &str) -> Vec<Memo> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    decode_memos(&text).into_iter().filter(|(name, _)| name == map).map(|(_, memo)| memo).collect()
}

pub fn save_memos(path: &Path, map: &str, memos: &[Memo]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = std::fs::read_to_string(path).unwrap_or_default();
    std::fs::write(path, replace_map(&text, map, memos))
}

fn saved_memos(map: &str) -> Vec<Memo> {
    crate::settings_path("minimap_memos").map(|path| load_memos(&path, map)).unwrap_or_default()
}

fn write_saved(map: &str, memos: &[Memo]) {
    if let Some(path) = crate::settings_path("minimap_memos") {
        let _ = save_memos(&path, map, memos);
    }
}

/// What the player is pointing at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hit {
    Title(TitleHit),
    /// A box: 0 Quest, 1 Facility, 2 Guild/Party.
    Check(usize),
    Record,
    /// A quest NPC's mark: a click navigates to its cell.
    Quest([i32; 2]),
    Map([i32; 2]),
    Store,
    Field,
    Window,
}

/// What the window shows besides the picture and the memos.
pub struct Pins<'b> {
    pub player: [i32; 2],
    pub party: &'b [[i32; 2]],
    pub guild: &'b [[i32; 2]],
    pub boss: Option<[i32; 2]>,
    /// Quest NPCs (0x0446 with a colour): cell, effect and colour.
    pub quests: &'b [([i32; 2], u16, u16)],
}

pub struct Window {
    pub frame: Frame,
    /// Quest, Facility and Guild/Party: all on at first. ponytail: Facility only keeps its flag; this data has no
    /// town facility icons.
    pub checks: [bool; 3],
    /// A click on the map will start a memo.
    pub recording: bool,
    draft: Option<[i32; 2]>,
    draft_name: String,
    pub memos: Vec<Memo>,
    map: String,
    picture: Option<Texture>,
    /// The player's head, as their character looks.
    head: Option<Texture>,
    pane: Texture,
    memo_mark: Texture,
    checkbox: [Option<Texture>; 2],
    record: [Option<Texture>; 3],
    boss: [Option<Texture>; 3],
    party: Option<Texture>,
    guild: Option<Texture>,
    quests: HashMap<(u16, u16), Texture>,
}

impl Window {
    pub fn new(gpu: &Gpu, grf: &Grf, map: &str) -> Self {
        let image = |path: String| gpu.try_load_texture(grf, &path, false).ok();
        let quests = QUEST_EFFECTS
            .flat_map(|effect| (1..=3).map(move |color| (effect, color)))
            .filter_map(|(effect, color)| Some(((effect, color), image(format!("{UI}minimap\\quest_{effect}_{color}.bmp"))?)))
            .collect();
        Self {
            frame: Frame::new(default_position(), WIDTH),
            checks: [true; 3],
            recording: false,
            draft: None,
            draft_name: String::new(),
            memos: saved_memos(map),
            map: map.to_owned(),
            picture: None,
            head: None,
            pane: gpu.solid(PANE),
            memo_mark: gpu.solid([255, 64, 64, 255]),
            checkbox: [0, 1].map(|on| image(format!("{UI}checkbox_{on}.bmp"))),
            record: [1, 2, 3].map(|state| image(format!("{UI}minimap\\memo_ready_{state}.bmp"))),
            boss: [1, 2, 3].map(|state| image(format!("{UI}minimap\\boss_{state}.bmp"))),
            party: image(format!("{UI}minimap\\party.bmp")),
            guild: image(format!("{UI}minimap\\guild.bmp")),
            quests,
        }
    }

    pub fn naming(&self) -> bool {
        self.draft.is_some()
    }

    pub fn covers(&self, cursor: [f32; 2]) -> bool {
        self.frame.open && self.frame.local(cursor, HEIGHT).is_some()
    }

    /// The player's head as drawn on the map.
    pub fn set_head(&mut self, gpu: &Gpu, head: Option<&RgbaImage>) {
        self.head = head.map(|image| gpu.texture(image, false));
    }

    /// Opens (the pane grey when the map has no picture), or closes when it is already open.
    pub fn toggle(&mut self, gpu: &Gpu, image: Option<&RgbaImage>, map: &str) {
        if self.frame.open {
            self.close();
        } else {
            self.open_with(gpu, image, map);
        }
    }

    pub fn open_with(&mut self, gpu: &Gpu, image: Option<&RgbaImage>, map: &str) {
        if self.map != map {
            self.map = map.to_owned();
            self.memos = saved_memos(map);
            self.cancel_name();
        }
        self.picture = image.map(|image| gpu.texture(image, true));
        self.frame.open = true;
        self.frame.minimized = false;
    }

    pub fn close(&mut self) {
        self.frame.open = false;
        self.recording = false;
        self.cancel_name();
    }

    /// Another map. An open window closes on a map that has no big minimap (`shown` false), and says so (true).
    pub fn change_map(&mut self, gpu: &Gpu, image: Option<&RgbaImage>, map: &str, shown: bool) -> bool {
        self.map = map.to_owned();
        self.memos = saved_memos(map);
        self.recording = false;
        self.cancel_name();
        self.picture = image.map(|image| gpu.texture(image, true));
        let lost = self.frame.open && !shown;
        if lost {
            self.frame.open = false;
        }
        lost
    }

    pub fn cancel_name(&mut self) {
        self.draft = None;
        self.draft_name.clear();
    }

    pub fn type_text(&mut self, text: &str) {
        if self.draft.is_none() {
            return;
        }
        let room = NAME_MOST.saturating_sub(self.draft_name.chars().count());
        self.draft_name.extend(text.chars().filter(|character| !character.is_control()).take(room));
    }

    pub fn backspace(&mut self) {
        self.draft_name.pop();
    }

    /// Stores the memo being named and writes this map's memos. An empty name stays in the box.
    pub fn store(&mut self) -> bool {
        let Some(cell) = self.draft else { return false };
        let name = self.draft_name.trim().to_owned();
        if name.is_empty() {
            return false;
        }
        self.cancel_name();
        self.memos.push(Memo { cell, name });
        write_saved(&self.map, &self.memos);
        true
    }

    /// A quest mark's picture and its box in the window, placed by its top left at − 6 from the cell's point.
    fn quest_icon(&self, cell: [i32; 2], effect: u16, color: u16, map_size: [u32; 2]) -> Option<(&Texture, [f32; 2])> {
        let icon = self.quests.get(&(effect, color)).or_else(|| self.quests.get(&(0, color)))?;
        Some((icon, offset(point(cell, map_size), [-6.0, -6.0])))
    }

    pub fn button_boxes(&self, map_size: [u32; 2], pins: &Pins) -> Vec<(String, [f32; 4])> {
        if !self.frame.open || self.frame.minimized {
            return Vec::new();
        }
        let at = |local: [f32; 2], size: [f32; 2]| {
            let place = self.frame.at(local);
            [place[0], place[1], size[0], size[1]]
        };
        let mut boxes: Vec<(String, [f32; 4])> = vec![("minimap_map".to_owned(), at(MAP_AT, [MAP_SIDE, MAP_SIDE]))];
        if self.draft.is_some() {
            boxes.push(("minimap_store".to_owned(), at(STORE_AT, STORE_SIZE)));
        } else {
            for (name, place) in ["minimap_check_quest", "minimap_check_facility", "minimap_check_party"].into_iter().zip(CHECK_AT) {
                boxes.push((name.to_owned(), at(place, CHECK)));
            }
            boxes.push(("minimap_record".to_owned(), at(RECORD_AT, RECORD_SIZE)));
        }
        if self.checks[0] {
            for &(cell, effect, color) in pins.quests {
                if let Some((icon, corner)) = self.quest_icon(cell, effect, color, map_size) {
                    boxes.push((format!("minimap_quest:{} {}", cell[0], cell[1]), at(corner, icon.size)));
                }
            }
        }
        boxes
    }

    pub fn hit(&self, cursor: [f32; 2], map_size: [u32; 2], pins: &Pins) -> Option<Hit> {
        let local = self.frame.local(cursor, HEIGHT)?;
        if let Some(title) = self.frame.title_hit(local) {
            return Some(Hit::Title(title));
        }
        if self.frame.minimized {
            return Some(Hit::Window);
        }
        if self.draft.is_some() {
            if contains(FIELD_AT, FIELD_SIZE, local) {
                return Some(Hit::Field);
            }
            if contains(STORE_AT, STORE_SIZE, local) {
                return Some(Hit::Store);
            }
        } else {
            if let Some(index) = CHECK_AT.iter().position(|&place| contains(place, CHECK, local)) {
                return Some(Hit::Check(index));
            }
            if contains(RECORD_AT, RECORD_SIZE, local) {
                return Some(Hit::Record);
            }
        }
        if contains(MAP_AT, [MAP_SIDE, MAP_SIDE], local) {
            if (self.recording || self.draft.is_some())
                && let Some(cell) = cell_at(local, map_size)
            {
                return Some(Hit::Map(cell));
            }
            if self.checks[0]
                && let Some(&(cell, ..)) = pins.quests.iter().rev().find(|&&(cell, effect, color)| {
                    self.quest_icon(cell, effect, color, map_size).is_some_and(|(icon, corner)| contains(corner, icon.size, local))
                })
            {
                return Some(Hit::Quest(cell));
            }
        }
        Some(Hit::Window)
    }

    /// A click; a quest mark's (navigating to it) is the caller's.
    pub fn click(&mut self, hit: Hit) {
        match hit {
            Hit::Title(title) => {
                self.frame.click_title(title);
                if !self.frame.open {
                    self.recording = false;
                    self.cancel_name();
                }
            }
            Hit::Check(index) => self.checks[index] = !self.checks[index],
            Hit::Record => {
                self.recording = !self.recording;
                if self.recording {
                    self.cancel_name();
                }
            }
            Hit::Map(cell) => {
                self.draft = Some(cell);
                self.draft_name.clear();
                self.recording = false;
            }
            Hit::Store => {
                self.store();
            }
            Hit::Quest(_) | Hit::Field | Hit::Window => {}
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &'a self,
        layer: &mut Layer<'a>,
        tooltip: &mut Layer<'a>,
        chrome: &'a Chrome,
        font: &Font,
        messages: &[String],
        title: &str,
        map_size: [u32; 2],
        pins: &Pins,
        pointer: ([f32; 2], bool),
        caret: bool,
    ) {
        if !self.frame.open {
            return;
        }
        let message = |id: usize| messages.get(id).cloned().unwrap_or_default();
        let cursor = pointer.0;
        // A tooltip and the local corner of what it names: 20 px above it.
        let mut tip: Option<(usize, [f32; 2])> = None;
        let hovered = |corner: [f32; 2], size: [f32; 2]| contains(self.frame.at(corner), size, cursor);
        if !self.frame.minimized {
            layer.sprites.push((&chrome.white, self.frame.position, [WIDTH, HEIGHT]));
            layer.sprites.push((&self.pane, self.frame.at(MAP_AT), [MAP_SIDE, MAP_SIDE]));
            if let Some(picture) = &self.picture {
                layer.sprites.push((picture, self.frame.at(MAP_AT), picture.size));
            }
            let at = |cell: [i32; 2], back: f32| self.frame.at(offset(point(cell, map_size), [-back, -back])).map(f32::round);
            // Memos.
            let mut named = self.memos.iter().map(|memo| (memo.cell, memo.name.as_str())).collect::<Vec<_>>();
            if let Some(cell) = self.draft {
                named.push((cell, self.draft_name.as_str()));
            }
            for (cell, name) in named {
                let [x, y] = at(cell, 3.0);
                layer.sprites.push((&self.memo_mark, [x, y], [6.0, 6.0]));
                if !name.is_empty() {
                    layer.texts.push((name.to_owned(), [x + 9.0, y - 3.0], [0, 0, 0]));
                }
            }
            // Party and guild members.
            if self.checks[2] {
                for (cells, icon) in [(pins.party, &self.party), (pins.guild, &self.guild)] {
                    let Some(icon) = icon else { continue };
                    for &cell in cells {
                        layer.sprites.push((icon, at(cell, 6.0), icon.size));
                    }
                }
            }
            // Quest NPCs.
            if self.checks[0] {
                for &(cell, effect, color) in pins.quests {
                    if let Some((icon, corner)) = self.quest_icon(cell, effect, color, map_size) {
                        layer.sprites.push((icon, self.frame.at(corner).map(f32::round), icon.size));
                        if hovered(corner, icon.size) {
                            tip = Some((QUEST_TIP, corner));
                        }
                    }
                }
            }
            // The boss, while the Convex Mirror shows it.
            if let Some(cell) = pins.boss {
                let corner = offset(point(cell, map_size), [-BOSS / 2.0, -BOSS / 2.0]);
                let over = hovered(corner, [BOSS; 2]);
                if let Some(image) = &self.boss[usize::from(over) + usize::from(over && pointer.1)] {
                    layer.sprites.push((image, self.frame.at(corner).map(f32::round), [BOSS; 2]));
                }
                if over {
                    tip = Some((BOSS_TIP, corner));
                }
            }
            // The player: their own head.
            if let Some(head) = &self.head {
                let corner = offset(point(pins.player, map_size), [-HEAD / 2.0, -HEAD / 2.0]);
                let middle = self.frame.at(offset(corner, [HEAD / 2.0, HEAD / 2.0]));
                layer.sprites.push((head, [(middle[0] - head.size[0] / 2.0).round(), (middle[1] - head.size[1] / 2.0).round()], head.size));
                if hovered(corner, [HEAD; 2]) {
                    tip = Some((PLAYER_TIP, corner));
                }
            }
            // The bottom row.
            if self.draft.is_some() {
                layer.sprites.push((&self.pane, self.frame.at(FIELD_AT), FIELD_SIZE));
                let shown = if caret { format!("{}|", self.draft_name) } else { self.draft_name.clone() };
                layer.texts.push((shown, self.frame.at(offset(FIELD_AT, [4.0, 3.0])), [0, 0, 0]));
                let store = self.frame.at(STORE_AT);
                layer.sprites.push((if contains(store, STORE_SIZE, cursor) { &chrome.border } else { &self.pane }, store, STORE_SIZE));
                layer.texts.push((message(STORE), offset(store, [6.0, 3.0]), [0, 0, 0]));
            } else {
                for (index, place) in CHECK_AT.into_iter().enumerate() {
                    if let Some(image) = &self.checkbox[usize::from(self.checks[index])] {
                        layer.sprites.push((image, self.frame.at(offset(place, [0.0, 1.0])), image.size));
                    }
                    layer.texts.push((message(CHECK_LABELS[index]), self.frame.at(offset(place, [14.0, -1.0])), [0, 0, 0]));
                    if hovered(place, CHECK) {
                        tip = Some((CHECK_TIPS[index], place));
                    }
                }
                let over = hovered(RECORD_AT, RECORD_SIZE);
                if let Some(image) = &self.record[usize::from(over || self.recording) + usize::from(over && pointer.1)] {
                    layer.sprites.push((image, self.frame.at(RECORD_AT), RECORD_SIZE));
                }
                if over {
                    tip = Some((RECORD, RECORD_AT));
                }
            }
        }
        self.frame.draw_title(layer, chrome, title, cursor);
        if let Some((tip, corner)) = tip {
            window::tooltip(tooltip, chrome, font, &message(tip), self.frame.at(offset(corner, [0.0, -20.0])), [255; 3]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_and_maps_cells_on_the_pane() {
        assert_eq!((WIDTH, HEIGHT), (516.0, 559.0));
        assert_eq!(default_position(), [screen()[0] - 536.0, 190.0]);
        assert_eq!(title("Prontera", "prontera", [156, 180]), "Prontera ( prontera ) 156 180");
        // Square: s = 512 / 256 = 2; x = 2 + 2·cx, y = 531 − 2·cy.
        assert_eq!(point([0, 0], [256, 256]), [2.0, 531.0]);
        assert_eq!(point([100, 50], [256, 256]), [202.0, 431.0]);
        // Wider (400×300): s = 1.28, y less (400 − 300)·s/2 = 64.
        assert_eq!(point([0, 0], [400, 300]), [2.0, 531.0 - 64.0]);
        assert_eq!(point([200, 150], [400, 300]), [2.0 + 256.0, 19.0 + 256.0]);
        // Taller (300×400): x plus 64.
        assert_eq!(point([0, 0], [300, 400]), [2.0 + 64.0, 531.0]);
        assert_eq!(cell_at([2.0 + 256.0, 19.0 + 256.0], [400, 300]), Some([200, 150]));
        assert_eq!(cell_at(MAP_AT, [400, 300]), None, "the letterbox above a wide map");
    }

    #[test]
    fn reads_the_exception_table_and_the_boxes() {
        let maps = exception_maps("1@air1#\r\nprtg_cas01#\n\n#\nnot a map\n");
        assert!(maps.contains("1@air1") && maps.contains("prtg_cas01") && !maps.contains("not a map"));
        assert_eq!(maps.len(), 2);
        assert_eq!(checks_line([true, false, true]), "minimap_checks 1 0 1\n");
        assert_eq!(checks_from(([1.0, 0.0], true)), [true, false, true]);
        assert_eq!(checks_from(([0.0, 1.0], false)), [false, true, false]);
    }

    #[test]
    fn memos_save_and_load_per_map() {
        let dir = std::env::temp_dir().join(format!("rustnarok-memos-{}-{}", std::process::id(), MAP_SIDE as u32));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("memos");
        let prontera = vec![Memo { cell: [156, 180], name: "Kafra".into() }];
        save_memos(&path, "prontera", &prontera).unwrap();
        save_memos(&path, "gef_fild03", &[Memo { cell: [1, 2], name: "Hill top".into() }]).unwrap();
        let loaded = load_memos(&path, "prontera");
        assert_eq!(loaded, prontera);
        assert_eq!(load_memos(&path, "gef_fild03")[0].name, "Hill top");
        // Writing one map again leaves the other map's memos in the file.
        save_memos(&path, "prontera", &[]).unwrap();
        assert!(load_memos(&path, "prontera").is_empty());
        assert_eq!(load_memos(&path, "gef_fild03").len(), 1);
        assert!(load_memos(&path, "payon").is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
