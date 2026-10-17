//! The client's right-click menu: a white box of choices by the pointer, the hovered one in blue, groups split by grey
//! lines.

use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};
use crate::{Layer, SCREEN, contains, offset};

// ponytail: the context menu's look (2px padding, 16px rows, #739CEF hover, #CECECE lines), square corners.
const PADDING: f32 = 2.0;
const ROW: f32 = 16.0;
const LINE: f32 = 5.0;

pub struct Assets {
    white: Texture,
    hover: Texture,
    line: Texture,
}

impl Assets {
    pub fn load(gpu: &Gpu) -> Self {
        Self { white: gpu.solid([255; 4]), hover: gpu.solid([0x73, 0x9C, 0xEF, 0xFF]), line: gpu.solid([0xCE, 0xCE, 0xCE, 0xFF]) }
    }
}

/// A choice, or None for a line between groups.
pub type Entry<T> = Option<(String, T)>;

pub struct Menu<T> {
    corner: [f32; 2],
    entries: Vec<Entry<T>>,
    width: f32,
}

impl<T: Copy + PartialEq> Menu<T> {
    /// Opens by the pointer, kept on screen.
    pub fn new(pointer: [f32; 2], entries: Vec<Entry<T>>, font: &Font) -> Self {
        let width = entries.iter().flatten().map(|(text, _)| font.width(text).ceil()).fold(0.0, f32::max) + 2.0 * (PADDING + 1.0);
        let height = Self::rows_height(&entries) + 2.0 * PADDING;
        let corner = [pointer[0].min(SCREEN[0] - width), pointer[1].min(SCREEN[1] - height)];
        Self { corner, entries, width }
    }

    fn rows_height(entries: &[Entry<T>]) -> f32 {
        entries.iter().map(|entry| if entry.is_some() { ROW } else { LINE }).sum()
    }

    fn size(&self) -> [f32; 2] {
        [self.width, Self::rows_height(&self.entries) + 2.0 * PADDING]
    }

    /// Over the menu: the choice under the pointer, if any.
    pub fn hit(&self, cursor: [f32; 2]) -> Option<Option<T>> {
        if !contains(self.corner, self.size(), cursor) {
            return None;
        }
        let mut top = self.corner[1] + PADDING;
        for entry in &self.entries {
            let height = if entry.is_some() { ROW } else { LINE };
            if let Some((_, choice)) = entry
                && cursor[1] >= top
                && cursor[1] < top + height
            {
                return Some(Some(*choice));
            }
            top += height;
        }
        Some(None)
    }

    /// Each choice's label and box on screen (for the test driver).
    pub fn entry_boxes(&self) -> Vec<(String, [f32; 4])> {
        let mut top = self.corner[1] + PADDING;
        let mut boxes = Vec::new();
        for entry in &self.entries {
            if let Some((text, _)) = entry {
                boxes.push((text.clone(), [self.corner[0] + PADDING, top, self.width - 2.0 * PADDING, ROW]));
            }
            top += if entry.is_some() { ROW } else { LINE };
        }
        boxes
    }

    /// `draw` inside a 1px grey line, for menus over white (the character select's cards).
    pub fn draw_outlined<'a>(&self, layer: &mut Layer<'a>, assets: &'a Assets, font: &Font, pointer: [f32; 2]) {
        let [width, height] = self.size();
        layer.sprites.push((&assets.line, offset(self.corner, [-1.0, -1.0]), [width + 2.0, height + 2.0]));
        self.draw(layer, assets, font, pointer);
    }

    pub fn draw<'a>(&self, layer: &mut Layer<'a>, assets: &'a Assets, font: &Font, pointer: [f32; 2]) {
        layer.sprites.push((&assets.white, self.corner, self.size()));
        let hovered = self.hit(pointer).flatten();
        let mut top = self.corner[1] + PADDING;
        for entry in &self.entries {
            match entry {
                Some((text, choice)) => {
                    if hovered == Some(*choice) {
                        layer.sprites.push((&assets.hover, [self.corner[0] + PADDING, top], [self.width - 2.0 * PADDING, ROW]));
                    }
                    layer.texts.push((text.clone(), offset([self.corner[0], top], [PADDING + 1.0, ((ROW - font.line_height()) / 2.0).floor()]), [0, 0, 0]));
                    top += ROW;
                }
                None => {
                    layer.sprites.push((&assets.line, [self.corner[0] + PADDING, top + 2.0], [self.width - 2.0 * PADDING, 1.0]));
                    top += LINE;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_choice_under_the_pointer() {
        let font = Font::load();
        let menu = Menu::new([100.0, 100.0], vec![Some(("Trade".into(), 1)), None, Some(("Party".into(), 2))], &font);
        assert_eq!(menu.hit([105.0, 105.0]), Some(Some(1)));
        assert_eq!(menu.hit([105.0, 100.0 + PADDING + ROW + 2.0]), Some(None));
        assert_eq!(menu.hit([105.0, 100.0 + PADDING + ROW + LINE + 3.0]), Some(Some(2)));
        assert_eq!(menu.hit([10.0, 10.0]), None);
        let [x, y, w, h] = menu.entry_boxes()[1].1;
        assert_eq!(menu.hit([x + w / 2.0, y + h / 2.0]), Some(Some(2)));
    }
}
