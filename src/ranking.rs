//! The ranking board (/blacksmith, /alchemist, /taekwon, /pk): a ranking's top ten with names and points, the
//! player's own points, and a choice of ranking.

use std::collections::HashMap;

use rustnarok_client::text::Font;

use crate::window::{Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, contains};

const TITLE: usize = 2382;
const RANK: usize = 2383;
const NAME: usize = 2384;
const POINTS: usize = 2385;
const KIND_LABEL: usize = 2407;
const FETCHING: usize = 3481;
/// The rankings' names, by type: Blacksmith, Alchemist, Taekwon, Killer.
const KINDS: [usize; 4] = [2386, 2387, 2388, 2389];
/// Their names for the test driver.
const KIND_NAMES: [&str; 4] = ["blacksmith", "alchemist", "taekwon", "killer"];

// ponytail: data.grf has no ranking board; the client's own window chrome, 280 wide, the type choice on top, ten rows,
// the player's points under them; which label goes where is an estimate.
const WIDTH: f32 = 280.0;
const KIND_TOP: f32 = TITLE_HEIGHT + 6.0;
const KIND_ROW: f32 = 18.0;
const KIND_LEFT: f32 = 8.0;
const KIND_WIDTH: f32 = 66.0;
const HEADER_TOP: f32 = KIND_TOP + 2.0 * KIND_ROW + 4.0;
const ROW: f32 = 17.0;
const ROWS_TOP: f32 = HEADER_TOP + ROW;
const HEIGHT: f32 = ROWS_TOP + 10.0 * ROW + 26.0;
const RANK_X: f32 = 10.0;
const NAME_X: f32 = 50.0;
const POINTS_RIGHT: f32 = WIDTH - 12.0;
const TEXT: [u8; 3] = [0, 0, 0];
const PICKED: [u8; 3] = [0x30, 0x50, 0xC0];
const GREY: [u8; 3] = [0x90, 0x90, 0x90];

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(TitleHit),
    /// A ranking in the type choice.
    Kind(u16),
    Window,
}

/// A ranking's place on the board: the character, its name once known (None: still coming, or the server has none),
/// its points.
#[derive(Debug, PartialEq, Clone)]
pub struct Row {
    pub id: u32,
    pub name: Option<String>,
    pub points: u32,
}

/// The board as the server last sent it.
pub struct Board {
    pub frame: Frame,
    pub kind: u16,
    pub ids: [u32; 10],
    pub points: [u32; 10],
    pub mine: u32,
}

/// The listed characters whose names aren't known yet, each once.
pub fn missing_names(ids: &[u32; 10], names: &HashMap<u32, Option<String>>) -> Vec<u32> {
    let mut missing: Vec<u32> = ids.iter().copied().filter(|id| *id != 0 && !names.contains_key(id)).collect();
    missing.dedup();
    missing
}

impl Board {
    pub fn new(kind: u16, ids: [u32; 10], points: [u32; 10], mine: u32) -> Self {
        let mut frame = Frame::new([380.0, 140.0], WIDTH);
        frame.open = true;
        Self { frame, kind, ids, points, mine }
    }

    /// The filled places, in order.
    pub fn rows(&self, names: &HashMap<u32, Option<String>>) -> Vec<Row> {
        self.ids.iter().zip(self.points).filter(|(id, _)| **id != 0).map(|(&id, points)| Row { id, name: names.get(&id).cloned().flatten(), points }).collect()
    }

    fn kind_at(kind: usize) -> [f32; 2] {
        [KIND_LEFT + (kind % 2) as f32 * (KIND_WIDTH + 70.0), KIND_TOP + (kind / 2) as f32 * KIND_ROW]
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let local = self.frame.local(cursor, HEIGHT)?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(Hit::Title(hit));
        }
        let kind = (0..KINDS.len()).find(|&kind| contains(Self::kind_at(kind), [KIND_WIDTH + 60.0, KIND_ROW], local));
        Some(kind.map_or(Hit::Window, |kind| Hit::Kind(kind as u16)))
    }

    /// The type choices on screen (for the test driver).
    pub fn button_boxes(&self) -> Vec<(&'static str, [f32; 4])> {
        if !self.frame.open || self.frame.minimized {
            return Vec::new();
        }
        (0..KINDS.len())
            .map(|kind| {
                let [x, y] = self.frame.at(Self::kind_at(kind));
                (KIND_NAMES[kind], [x, y, KIND_WIDTH + 60.0, KIND_ROW])
            })
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &self,
        layer: &mut Layer<'a>,
        chrome: &'a Chrome,
        font: &Font,
        messages: &[String],
        names: &HashMap<u32, Option<String>>,
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
        layer.sprites.push((&chrome.white, at([0.0, TITLE_HEIGHT]), [WIDTH, HEIGHT - TITLE_HEIGHT]));
        for x in [0.0, WIDTH - 1.0] {
            layer.sprites.push((&chrome.border, at([x, TITLE_HEIGHT]), [1.0, HEIGHT - TITLE_HEIGHT]));
        }
        layer.sprites.push((&chrome.border, at([0.0, HEIGHT - 1.0]), [WIDTH, 1.0]));
        // The type choice: the one shown in blue.
        for (kind, &label) in KINDS.iter().enumerate() {
            let color = if kind as u16 == self.kind { PICKED } else { TEXT };
            let marker = if kind as u16 == self.kind { "● " } else { "○ " };
            layer.texts.push((format!("{marker}{}", message(label)), at(Self::kind_at(kind)), color));
        }
        let header = [(RANK, RANK_X), (NAME, NAME_X)];
        for (label, x) in header {
            layer.texts.push((message(label).to_owned(), at([x, HEADER_TOP]), GREY));
        }
        let points = message(POINTS);
        layer.texts.push((points.to_owned(), at([POINTS_RIGHT - font.width(points), HEADER_TOP]), GREY));
        for (place, row) in self.rows(names).into_iter().enumerate() {
            let y = ROWS_TOP + place as f32 * ROW;
            layer.texts.push(((place + 1).to_string(), at([RANK_X, y]), TEXT));
            // Still waiting for the name: the client's "Fetching rank..."; none from the server: nothing.
            let name = match (&row.name, names.contains_key(&row.id)) {
                (Some(name), _) => name.clone(),
                (None, false) => message(FETCHING).to_owned(),
                (None, true) => String::new(),
            };
            layer.texts.push((crate::window::fit(font, &name, POINTS_RIGHT - NAME_X - 50.0), at([NAME_X, y]), TEXT));
            let value = row.points.to_string();
            layer.texts.push((value.clone(), at([POINTS_RIGHT - font.width(&value), y]), TEXT));
        }
        let mine = format!("{} : {}", message(POINTS), self.mine);
        layer.texts.push((mine, at([RANK_X, HEIGHT - 20.0]), TEXT));
        let kind_label = message(KIND_LABEL);
        layer.texts.push((kind_label.to_owned(), at([WIDTH - 10.0 - font.width(kind_label), HEIGHT - 20.0]), GREY));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asks_for_the_names_it_lacks_and_lists_the_places() {
        let mut ids = [0u32; 10];
        ids[..3].copy_from_slice(&[150_000, 150_002, 150_005]);
        let mut points = [0u32; 10];
        points[..3].copy_from_slice(&[50, 10, 0]);
        let mut names = HashMap::new();
        names.insert(150_000, Some("Rndev".to_owned()));
        names.insert(150_005, None);
        assert_eq!(missing_names(&ids, &names), [150_002]);
        let board = Board::new(0, ids, points, 50);
        assert_eq!(
            board.rows(&names),
            [
                Row { id: 150_000, name: Some("Rndev".into()), points: 50 },
                Row { id: 150_002, name: None, points: 10 },
                Row { id: 150_005, name: None, points: 0 }
            ]
        );
        assert!(Board::new(3, [0; 10], [0; 10], 0).rows(&names).is_empty(), "Killer: ten empty places");
        let [x, y, w, h] = board.button_boxes()[1].1;
        assert_eq!(board.hit([x + w / 2.0, y + h / 2.0]), Some(Hit::Kind(1)));
    }
}
