//! The emotion list (Alt+L, /emo, /emotion): every emotion the data lists, its picture and its command; a click shows it.

use rustnarok_client::text::Font;

use crate::actor::{Placed, SpriteSet};
use crate::emotion;
use crate::window::{Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, SCREEN, contains, offset};

// ponytail: an 8 by 8 grid on the window chrome (data.grf has no bitmaps for this window); place it against the real
// client.
const COLUMNS: usize = 8;
const CELL: [f32; 2] = [42.0, 40.0];
const ICON: f32 = 26.0;
const SIDE: f32 = 6.0;
const WIDTH: f32 = SIDE * 2.0 + COLUMNS as f32 * CELL[0];
const TITLE: usize = 542;

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(TitleHit),
    /// An emotion, by ET_* number.
    Emotion(u8),
    Window,
}

pub struct List {
    entries: Vec<(u8, usize)>,
}

impl Default for List {
    fn default() -> Self {
        Self { entries: emotion::list() }
    }
}

impl List {
    fn height(&self) -> f32 {
        TITLE_HEIGHT + self.entries.len().div_ceil(COLUMNS) as f32 * CELL[1] + SIDE
    }

    fn corner(&self) -> [f32; 2] {
        [((SCREEN[0] - WIDTH) / 2.0).floor(), ((SCREEN[1] - self.height()) / 2.0).floor()]
    }

    fn frame(&self) -> Frame {
        let mut frame = Frame::new(self.corner(), WIDTH);
        frame.open = true;
        frame
    }

    fn cell_corner(index: usize) -> [f32; 2] {
        [SIDE + (index % COLUMNS) as f32 * CELL[0], TITLE_HEIGHT + (index / COLUMNS) as f32 * CELL[1]]
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let frame = self.frame();
        let local = frame.local(cursor, self.height())?;
        if let Some(hit) = frame.title_hit(local) {
            return Some(Hit::Title(hit));
        }
        Some(
            (0..self.entries.len())
                .find(|&index| contains(Self::cell_corner(index), CELL, local))
                .map_or(Hit::Window, |index| Hit::Emotion(self.entries[index].0)),
        )
    }

    // ponytail: the issue asked for each action's first frame, but emotions grow in from a dot, so their first frames
    // are nearly empty; the largest frame stands in until the list's own icons (emotion_index*.act?) are worked out.
    /// Each entry's picture: the largest frame of its emotion's action.
    pub fn icons(&self, gpu: &crate::render::Gpu, sprite: &mut SpriteSet) -> Vec<Vec<Placed>> {
        let area = |placed: &[Placed]| placed.iter().map(|(_, _, size, _)| (size[0] * size[1]).abs()).sum::<f32>();
        self.entries
            .iter()
            .map(|&(kind, _)| {
                let Some(action) = emotion::sprite_action(kind) else { return Vec::new() };
                let count = sprite.frames(action).max(1);
                // The first of the largest: later ones of the same size are often fading out.
                (0..count)
                    .map(|frame| sprite.frame_at(gpu, action, 0, crate::actor::Timing::Span((frame as f32 + 0.5) / count as f32)))
                    .fold(Vec::new(), |best, placed| if area(&placed) > area(&best) { placed } else { best })
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
        sprite: Option<&'a SpriteSet>,
        icons: &[Vec<Placed>],
        pointer: [f32; 2],
    ) {
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        let frame = self.frame();
        frame.draw_title(layer, chrome, message(TITLE), pointer);
        layer.sprites.push((&chrome.white, frame.at([0.0, TITLE_HEIGHT]), [WIDTH, self.height() - TITLE_HEIGHT]));
        let hovered = self.hit(pointer);
        for (index, &(kind, label)) in self.entries.iter().enumerate() {
            let cell = Self::cell_corner(index);
            if hovered == Some(Hit::Emotion(kind)) {
                layer.sprites.push((&chrome.border, frame.at(cell), CELL));
                layer.sprites.push((&chrome.white, frame.at(offset(cell, [1.0, 1.0])), [CELL[0] - 2.0, CELL[1] - 2.0]));
            }
            // The picture on a grey tile (the bubbles are white), fitted into its square and centred over the label.
            layer.sprites.push((&chrome.border, frame.at(offset(cell, [4.0, 0.0])), [CELL[0] - 8.0, ICON + 1.0]));
            if let (Some(sprite), Some(placed)) = (sprite, icons.get(index))
                && !placed.is_empty()
            {
                let (low, high) = placed.iter().fold(([f32::MAX; 2], [f32::MIN; 2]), |(low, high), (_, position, size, _)| {
                    ([low[0].min(position[0]), low[1].min(position[1])], [high[0].max(position[0] + size[0]), high[1].max(position[1] + size[1])])
                });
                let scale = (ICON / (high[0] - low[0]).max(high[1] - low[1]).max(1.0)).min(1.0);
                let size = [(high[0] - low[0]) * scale, (high[1] - low[1]) * scale];
                let origin = frame.at(offset(cell, [((CELL[0] - size[0]) / 2.0).floor(), ((ICON - size[1]) / 2.0).floor() + 1.0]));
                for (key, position, part, _) in placed {
                    let at = [origin[0] + (position[0] - low[0]) * scale, origin[1] + (position[1] - low[1]) * scale];
                    layer.sprites.push((sprite.texture(*key), at, [part[0] * scale, part[1] * scale]));
                }
            }
            let text = crate::window::fit(font, message(label), CELL[0] - 2.0);
            let place = offset(cell, [((CELL[0] - font.width(&text)) / 2.0).floor(), ICON]);
            layer.texts.push((text, frame.at(place), [0, 0, 0]));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_an_emotion_by_its_cell() {
        let list = List::default();
        let corner = list.corner();
        let at = |index: usize| offset(corner, offset(List::cell_corner(index), [CELL[0] / 2.0, CELL[1] / 2.0]));
        assert_eq!(list.hit(at(0)), Some(Hit::Emotion(0)));
        // The fifth entry is ET 14 (/lv2), as the data orders them.
        assert_eq!(list.hit(at(4)), Some(Hit::Emotion(14)));
        assert_eq!(list.hit(at(63)), Some(Hit::Emotion(80)));
        assert_eq!(list.hit([0.0, 0.0]), None);
    }
}
