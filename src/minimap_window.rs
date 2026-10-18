//! The big minimap: the current map's overview, quest and boss marks, and memos the player records.
//!
//! ponytail: the pane, the bottom row and the icon looks are laid out here; the client's own window bitmap would
//! replace them. Quest NPCs are a gold cross (the quest info's NpcPosX/NpcPosY); the NPC's sprite would replace it.

use std::path::Path;

use image::RgbaImage;

use crate::minimap;
use crate::render::{Gpu, Texture};
use crate::window::{Chrome, Frame, TitleHit};
use crate::{Layer, contains, offset, screen};

pub const WIDTH: f32 = 516.0;
pub const HEIGHT: f32 = 559.0;
/// "Show Quest icon", "Record", "Boss Monster", "Store", and the line when the map has no overview.
pub const SHOW_QUESTS: usize = 2743;
pub const RECORD: usize = 2748;
pub const SHOW_BOSS: usize = 2749;
pub const STORE: usize = 2754;
pub const NO_PICTURE: usize = 2767;

/// ponytail: the map pane and the bottom row's places.
const MAP_AT: [f32; 2] = [8.0, 22.0];
const MAP_SIDE: f32 = 500.0;
const CHECK_AT: [[f32; 2]; 2] = [[12.0, 532.0], [230.0, 532.0]];
const CHECK: [f32; 2] = [13.0, 13.0];
const RECORD_AT: [f32; 2] = [400.0, 528.0];
const RECORD_SIZE: [f32; 2] = [100.0, 22.0];
const FIELD_AT: [f32; 2] = [12.0, 528.0];
const FIELD_SIZE: [f32; 2] = [280.0, 22.0];
const STORE_AT: [f32; 2] = [300.0, 528.0];
const STORE_SIZE: [f32; 2] = [80.0, 22.0];
const NAME_MOST: usize = 40;

/// Where the window first opens: 536 left of the screen's right edge, 190 down.
pub fn default_position() -> [f32; 2] {
    [screen()[0] - 536.0, 190.0]
}

/// A cell on the map pane. The overview's longer side fills the pane; the shorter is centred (`minimap::share`).
pub fn point(cell: [i32; 2], size: [u32; 2]) -> [f32; 2] {
    let [x, y] = minimap::share(cell, size);
    [MAP_AT[0] + x * MAP_SIDE, MAP_AT[1] + y * MAP_SIDE]
}

/// The cell under a point in the window, or none outside the picture (the letterbox around a map that isn't square).
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

/// Where the overview picture sits in the pane: its longer side fills the pane, the other is centred.
pub fn picture_rect(size: [u32; 2]) -> ([f32; 2], [f32; 2]) {
    let longest = size[0].max(size[1]).max(1) as f32;
    let picture = [size[0] as f32 / longest * MAP_SIDE, size[1] as f32 / longest * MAP_SIDE];
    ([MAP_AT[0] + (MAP_SIDE - picture[0]) / 2.0, MAP_AT[1] + (MAP_SIDE - picture[1]) / 2.0], picture)
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
    Quests,
    Boss,
    Record,
    Map([i32; 2]),
    Store,
    Field,
    Window,
}

/// Marks drawn on the overview, placed as on the small minimap.
pub struct Pins<'a, 'b> {
    pub player: [i32; 2],
    pub yaw: f32,
    pub marks: &'b [([i32; 2], &'a Texture)],
    pub party: &'b [([i32; 2], &'a Texture)],
    pub guild: &'b [[i32; 2]],
    pub boss: Option<[i32; 2]>,
    pub quests: &'b [[i32; 2]],
}

pub struct Window {
    pub frame: Frame,
    pub show_quests: bool,
    pub show_boss: bool,
    /// A click on the map will start a memo.
    pub recording: bool,
    draft: Option<[i32; 2]>,
    draft_name: String,
    pub memos: Vec<Memo>,
    map: String,
    picture: Option<Texture>,
    body: Texture,
    map_back: Texture,
    /// ponytail: a gold cross stands in for the quest NPC's sprite.
    quest_mark: Texture,
    memo_mark: Texture,
}

impl Window {
    pub fn new(gpu: &Gpu, map: &str) -> Self {
        Self {
            frame: Frame::new(default_position(), WIDTH),
            show_quests: false,
            show_boss: false,
            recording: false,
            draft: None,
            draft_name: String::new(),
            memos: saved_memos(map),
            map: map.to_owned(),
            picture: None,
            body: gpu.solid([24, 28, 36, 255]),
            map_back: gpu.solid([0, 0, 0, 255]),
            quest_mark: gpu.solid([255, 210, 40, 255]),
            memo_mark: gpu.solid([255, 64, 64, 255]),
        }
    }

    pub fn naming(&self) -> bool {
        self.draft.is_some()
    }

    pub fn covers(&self, cursor: [f32; 2]) -> bool {
        self.frame.open && self.frame.local(cursor, HEIGHT).is_some()
    }

    /// Opens on `image`, or closes when it is already open. False when there is no picture to open.
    pub fn toggle(&mut self, gpu: &Gpu, image: Option<&RgbaImage>, map: &str) -> bool {
        if self.frame.open {
            self.close();
            return true;
        }
        let Some(image) = image else { return false };
        self.open_with(gpu, image, map);
        true
    }

    pub fn open_with(&mut self, gpu: &Gpu, image: &RgbaImage, map: &str) {
        if self.map != map {
            self.map = map.to_owned();
            self.memos = saved_memos(map);
            self.cancel_name();
        }
        self.picture = Some(gpu.texture(image, true));
        self.frame.open = true;
        self.frame.minimized = false;
    }

    pub fn close(&mut self) {
        self.frame.open = false;
        self.recording = false;
        self.cancel_name();
    }

    /// Another map. True when an open window had to close because the new map has no overview.
    pub fn change_map(&mut self, gpu: &Gpu, image: Option<&RgbaImage>, map: &str) -> bool {
        self.map = map.to_owned();
        self.memos = saved_memos(map);
        self.recording = false;
        self.cancel_name();
        self.picture = image.map(|image| gpu.texture(image, true));
        let lost = self.frame.open && self.picture.is_none();
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

    pub fn button_boxes(&self) -> Vec<(&'static str, [f32; 4])> {
        if !self.frame.open || self.frame.minimized {
            return Vec::new();
        }
        let at = |local: [f32; 2], size: [f32; 2]| {
            let place = self.frame.at(local);
            [place[0], place[1], size[0], size[1]]
        };
        let mut boxes =
            vec![("minimap_quests", at(CHECK_AT[0], CHECK)), ("minimap_boss", at(CHECK_AT[1], CHECK)), ("minimap_map", at(MAP_AT, [MAP_SIDE, MAP_SIDE]))];
        if self.draft.is_some() {
            boxes.push(("minimap_store", at(STORE_AT, STORE_SIZE)));
        } else {
            boxes.push(("minimap_record", at(RECORD_AT, RECORD_SIZE)));
        }
        boxes
    }

    pub fn hit(&self, cursor: [f32; 2], map_size: [u32; 2]) -> Option<Hit> {
        let local = self.frame.local(cursor, HEIGHT)?;
        if let Some(title) = self.frame.title_hit(local) {
            return Some(Hit::Title(title));
        }
        if self.frame.minimized {
            return Some(Hit::Window);
        }
        if contains(CHECK_AT[0], CHECK, local) {
            return Some(Hit::Quests);
        }
        if contains(CHECK_AT[1], CHECK, local) {
            return Some(Hit::Boss);
        }
        if self.draft.is_some() {
            if contains(FIELD_AT, FIELD_SIZE, local) {
                return Some(Hit::Field);
            }
            if contains(STORE_AT, STORE_SIZE, local) {
                return Some(Hit::Store);
            }
        } else if contains(RECORD_AT, RECORD_SIZE, local) {
            return Some(Hit::Record);
        }
        if contains(MAP_AT, [MAP_SIDE, MAP_SIDE], local) {
            if (self.recording || self.draft.is_some())
                && let Some(cell) = cell_at(local, map_size)
            {
                return Some(Hit::Map(cell));
            }
            return Some(Hit::Window);
        }
        Some(Hit::Window)
    }

    pub fn click(&mut self, hit: Hit) {
        match hit {
            Hit::Title(title) => {
                self.frame.click_title(title);
                if !self.frame.open {
                    self.recording = false;
                    self.cancel_name();
                }
            }
            Hit::Quests => self.show_quests = !self.show_quests,
            Hit::Boss => self.show_boss = !self.show_boss,
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
            Hit::Field | Hit::Window => {}
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a, 'b>(
        &'a self,
        layer: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a minimap::Assets,
        messages: &[String],
        title: &str,
        map_size: [u32; 2],
        pins: &Pins<'a, 'b>,
        cursor: [f32; 2],
        caret: bool,
    ) {
        if !self.frame.open {
            return;
        }
        let message = |id: usize| messages.get(id).cloned().unwrap_or_default();
        if !self.frame.minimized {
            layer.sprites.push((&self.body, self.frame.position, [WIDTH, HEIGHT]));
            if let Some(picture) = &self.picture {
                layer.sprites.push((&self.map_back, self.frame.at(MAP_AT), [MAP_SIDE, MAP_SIDE]));
                let (origin, size) = picture_rect(map_size);
                layer.sprites.push((picture, self.frame.at(origin), size));
            }
            let at = |cell: [i32; 2]| self.frame.at(point(cell, map_size)).map(f32::round);
            for &(cell, color) in pins.marks {
                let [x, y] = at(cell);
                layer.sprites.push((color, [x - 1.0, y - 4.0], [2.0, 8.0]));
                layer.sprites.push((color, [x - 4.0, y - 1.0], [8.0, 2.0]));
            }
            for &cell in pins.quests {
                let [x, y] = at(cell);
                layer.sprites.push((&self.quest_mark, [x - 1.0, y - 4.0], [2.0, 8.0]));
                layer.sprites.push((&self.quest_mark, [x - 4.0, y - 1.0], [8.0, 2.0]));
            }
            for &cell in pins.guild {
                let [x, y] = at(cell);
                layer.sprites.push((assets.guild_ring(), [x - 3.0, y - 3.0], assets.guild_ring().size));
                layer.sprites.push((assets.guild_dot(), [x - 2.0, y - 2.0], assets.guild_dot().size));
            }
            for &(cell, color) in pins.party {
                let [x, y] = at(cell);
                layer.sprites.push((assets.party_back(), [x - 3.0, y - 3.0], [6.0, 6.0]));
                layer.sprites.push((color, [x - 2.0, y - 2.0], [4.0, 4.0]));
            }
            if let Some(boss) = pins.boss {
                let [x, y] = at(boss);
                layer.sprites.push((assets.boss_mark(), [x - 6.0, y - 6.0], [12.0, 12.0]));
            }
            let mut named = self.memos.iter().map(|memo| (memo.cell, memo.name.as_str())).collect::<Vec<_>>();
            if let Some(cell) = self.draft {
                named.push((cell, self.draft_name.as_str()));
            }
            for (cell, name) in named {
                let [x, y] = at(cell);
                layer.sprites.push((&self.memo_mark, [x - 3.0, y - 3.0], [6.0, 6.0]));
                if !name.is_empty() {
                    layer.texts.push((name.to_owned(), [x + 6.0, y - 6.0], [255, 255, 255]));
                }
            }
            let point = at(pins.player);
            let (shadow_at, shadow_size) = minimap::shadow_box(point);
            layer.sprites.push((assets.arrow_shadow(pins.yaw), shadow_at, shadow_size));
            let (arrow_at, arrow_size) = minimap::arrow_box(point);
            layer.sprites.push((assets.player_arrow(pins.yaw), arrow_at, arrow_size));
            for (index, place) in CHECK_AT.into_iter().enumerate() {
                let on = if index == 0 { self.show_quests } else { self.show_boss };
                layer.sprites.push((&chrome.border, self.frame.at(place), CHECK));
                if on {
                    layer.sprites.push((&chrome.white, self.frame.at(offset(place, [3.0, 3.0])), [CHECK[0] - 6.0, CHECK[1] - 6.0]));
                }
                let label = message(if index == 0 { SHOW_QUESTS } else { SHOW_BOSS });
                layer.texts.push((label, self.frame.at(offset(place, [16.0, 0.0])), [255, 255, 255]));
            }
            if self.draft.is_some() {
                layer.sprites.push((&chrome.white, self.frame.at(FIELD_AT), FIELD_SIZE));
                let shown = if caret { format!("{}|", self.draft_name) } else { self.draft_name.clone() };
                layer.texts.push((shown, self.frame.at(offset(FIELD_AT, [4.0, 4.0])), [0, 0, 0]));
                self.button(layer, chrome, STORE_AT, STORE_SIZE, &message(STORE), cursor);
            } else {
                self.button(layer, chrome, RECORD_AT, RECORD_SIZE, &message(RECORD), cursor);
            }
        }
        self.frame.draw_title(layer, chrome, title, cursor);
    }

    fn button<'a>(&'a self, layer: &mut Layer<'a>, chrome: &'a Chrome, place: [f32; 2], size: [f32; 2], text: &str, cursor: [f32; 2]) {
        let at = self.frame.at(place);
        let fill = if contains(at, size, cursor) { &chrome.white } else { &chrome.border };
        layer.sprites.push((fill, at, size));
        layer.texts.push((text.to_owned(), offset(at, [6.0, 4.0]), [0, 0, 0]));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_a_point_on_a_400_by_300_map() {
        assert_eq!((WIDTH, HEIGHT), (516.0, 559.0));
        assert_eq!(default_position(), [screen()[0] - 536.0, 190.0]);
        let size = [400, 300];
        // The 300 side is centred in the 400 square, so the middle cell is the middle of the pane.
        assert_eq!(point([200, 150], size), [MAP_AT[0] + 250.0, MAP_AT[1] + 250.0]);
        assert_eq!(point([0, 0], size), [MAP_AT[0], MAP_AT[1] + 437.5]);
        assert_eq!(cell_at([MAP_AT[0] + 250.0, MAP_AT[1] + 250.0], size), Some([200, 150]));
        assert_eq!(cell_at([MAP_AT[0], MAP_AT[1] + 437.5], size), Some([0, 0]));
        // The top of the pane is the letterbox above a map that's wider than it is tall.
        assert_eq!(cell_at(MAP_AT, size), None);
        let (origin, picture) = picture_rect(size);
        assert_eq!((origin, picture), ([MAP_AT[0], MAP_AT[1] + 62.5], [500.0, 375.0]));
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
