//! The pickup popup: what was just picked up, with its icon, near the top of the screen for five seconds.

use std::time::{Duration, Instant};

use rustnarok_client::grf::Grf;
use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};
use crate::window::Chrome;
use crate::{Layer, UI, screen};

/// " - %d obtained."
pub const OBTAINED: usize = 696;
const LIFE: Duration = Duration::from_secs(5);
/// Its left edge never moves: it grows to the right.
fn corner() -> [f32; 2] {
    [(screen()[0] - 640.0) / 2.0 + 220.0, 75.0]
}
const HEIGHT: f32 = 32.0;
const PIECE: f32 = 14.0;
const ICON: [f32; 2] = [13.0, 5.0];
const ICON_SIZE: [f32; 2] = [24.0, 24.0];
const TEXT: [f32; 2] = [41.0, 10.0];
/// Two lines when it doesn't fit on one.
const LINES: [f32; 2] = [4.0, 17.0];
const INK: [u8; 3] = [0, 0, 0];
const SHADOW: [u8; 3] = [224, 224, 224];

/// How many came in a pickup: result 0 brings the amount picked up; 3 the stack's new total (less what was held).
/// Anything else is a refusal.
// ponytail: rathena only ever sends 0 with the item (a refusal comes with nothing); 3 is the game's.
pub fn obtained(result: u8, count: u16, held: u16) -> Option<u16> {
    match result {
        0 => Some(count),
        3 => Some(count.saturating_sub(held)),
        _ => None,
    }
}

/// A refused pickup's line (results other than 0 and 3); 8 has none.
pub fn failure_line(result: u8) -> Option<usize> {
    match result {
        2 => Some(52),
        4 => Some(220),
        5 => Some(279),
        7 => Some(1418),
        8 => None,
        9 => Some(3984),
        _ => Some(53),
    }
}

/// The popup's width for a chat line this wide: 54 more, in steps of 28, at least 34.
pub fn width_for(line_width: f32) -> f32 {
    (((line_width + 54.0) / 28.0).ceil() * 28.0).max(34.0)
}

pub struct Assets {
    /// The system box: corners, edges (lu, mu, ru, lm, rm, ld, md, rd).
    pieces: [Texture; 8],
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |name: &str| gpu.try_load_texture(grf, &format!("{UI}sysbox_{name}.bmp"), false).unwrap_or_else(|_| gpu.solid([0, 0, 0, 0]));
        Self { pieces: ["lu", "mu", "ru", "lm", "rm", "ld", "md", "rd"].map(image) }
    }
}

pub struct Popup {
    /// The line it shows ("<name> - N obtained.") and the item's icon file.
    pub text: String,
    pub icon: String,
    /// The maker's name piece of the text, coloured.
    pub maker: Option<crate::inventory::MakerRun>,
    width: f32,
    until: Instant,
}

impl Popup {
    /// A pickup: `line_width` is the chat line's width, which sizes the popup.
    pub fn new(text: String, icon: String, line_width: f32, now: Instant) -> Self {
        Self { text, icon, maker: None, width: width_for(line_width), until: now + LIFE }
    }

    /// The pieces of a line that starts `start` bytes into the text: plain (black) and the maker's (its colour).
    fn runs(&self, line: &str, start: usize) -> Vec<(String, [u8; 3])> {
        let Some((range, color)) = &self.maker else { return vec![(line.to_owned(), INK)] };
        let end = start + line.len();
        let (from, to) = (range.start.clamp(start, end) - start, range.end.clamp(start, end) - start);
        let cut = |mut at: usize| {
            while !line.is_char_boundary(at) {
                at -= 1;
            }
            at
        };
        let (from, to) = (cut(from), cut(to));
        [(&line[..from], INK), (&line[from..to], *color), (&line[to..], INK)]
            .into_iter()
            .filter(|(piece, _)| !piece.is_empty())
            .map(|(piece, color)| (piece.to_owned(), color))
            .collect()
    }

    pub fn alive(&self, now: Instant) -> bool {
        now < self.until
    }

    /// The text on one line, or two when it's too wide (the second cut with "..." if it still is), each with where it
    /// starts in the text.
    fn lines(&self, font: &Font) -> Vec<(String, usize)> {
        let room = self.width - 49.0;
        if font.width(&self.text) <= room {
            return vec![(self.text.clone(), 0)];
        }
        let mut first = String::new();
        let mut rest = self.text.as_str();
        for (at, character) in self.text.char_indices() {
            if font.width(&format!("{first}{character}")) > room {
                rest = &self.text[at..];
                break;
            }
            first.push(character);
        }
        let second_start = self.text.len() - rest.trim_start().len();
        let mut second = rest.trim_start().to_owned();
        if font.width(&second) > room {
            while !second.is_empty() && font.width(&format!("{second}...")) > room {
                second.pop();
            }
            second.push_str("...");
        }
        vec![(first, 0), (second, second_start)]
    }

    pub fn draw<'a>(&self, layer: &mut Layer<'a>, chrome: &'a Chrome, assets: &'a Assets, icon: Option<&'a Texture>, font: &Font) {
        let [x, y] = corner();
        let [w, h] = [self.width, HEIGHT];
        let [lu, mu, ru, lm, rm, ld, md, rd] = &assets.pieces;
        let middle = [w - 2.0 * PIECE, h - 2.0 * PIECE];
        layer.sprites.push((&chrome.white, [x + PIECE, y + PIECE], middle));
        layer.sprites.push((lu, [x, y], [PIECE; 2]));
        layer.sprites.push((mu, [x + PIECE, y], [middle[0], PIECE]));
        layer.sprites.push((ru, [x + w - PIECE, y], [PIECE; 2]));
        layer.sprites.push((lm, [x, y + PIECE], [PIECE, middle[1]]));
        layer.sprites.push((rm, [x + w - PIECE, y + PIECE], [PIECE, middle[1]]));
        layer.sprites.push((ld, [x, y + h - PIECE], [PIECE; 2]));
        layer.sprites.push((md, [x + PIECE, y + h - PIECE], [middle[0], PIECE]));
        layer.sprites.push((rd, [x + w - PIECE, y + h - PIECE], [PIECE; 2]));
        if let Some(icon) = icon {
            layer.sprites.push((icon, [x + ICON[0], y + ICON[1]], ICON_SIZE));
        }
        let lines = self.lines(font);
        let tops: Vec<f32> = if lines.len() == 1 { vec![TEXT[1]] } else { LINES.to_vec() };
        // Black with a light shadow, the maker's name in its colour.
        for ((line, start), top) in lines.into_iter().zip(tops) {
            let mut left = x + TEXT[0];
            for (piece, color) in self.runs(&line, start) {
                let width = font.width(&piece);
                layer.texts.push((piece.clone(), [left + 1.0, y + top + 1.0], SHADOW));
                layer.texts.push((piece, [left, y + top], color));
                left += width;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_the_makers_name() {
        let now = Instant::now();
        let mut popup = Popup::new("Smith's Knife - 1 obtained.".into(), String::new(), 200.0, now);
        assert_eq!(popup.runs("Smith's Knife", 0), [("Smith's Knife".to_owned(), INK)]);
        popup.maker = Some((0..5, crate::inventory::MAKER_COLOR));
        assert_eq!(popup.runs("Smith's Knife", 0), [("Smith".to_owned(), crate::inventory::MAKER_COLOR), ("'s Knife".to_owned(), INK)]);
        // "Nameless" in red; a second line starting past the maker is plain.
        popup.maker = Some((2..10, crate::inventory::NAMELESS_COLOR));
        assert_eq!(popup.runs("+7Nameless's", 0)[1], ("Nameless".to_owned(), crate::inventory::NAMELESS_COLOR));
        assert_eq!(popup.runs("obtained.", 20), [("obtained.".to_owned(), INK)]);
    }

    #[test]
    fn sizes_lives_and_counts() {
        // 54 more, rounded up to 28s, never under 34.
        assert_eq!(width_for(0.0), 56.0);
        assert_eq!(width_for(100.0), 168.0);
        assert_eq!(width_for(114.0), 168.0);
        assert_eq!(width_for(115.0), 196.0);
        assert_eq!(width_for(-60.0), 34.0);
        // Five seconds from the pickup; a new one starts over.
        let start = Instant::now();
        let popup = Popup::new("Red Potion - 3 obtained.".into(), String::new(), 120.0, start);
        assert!(popup.alive(start + Duration::from_millis(4_999)));
        assert!(!popup.alive(start + LIFE));
        // Result 3 carries the new total: what came is that less what was held.
        assert_eq!((obtained(0, 3, 7), obtained(3, 10, 7), obtained(3, 5, 7), obtained(2, 3, 0)), (Some(3), Some(3), Some(0), None));
        assert_eq!(corner(), [412.0, 75.0]);
        // Each refusal's line: 1 and 6 can't get it, 2 overweight, 4 the weight limit, 5 30,000 of a kind, 7 another
        // item's limit, 9 its own; 8 nothing.
        assert_eq!([1, 2, 4, 5, 6, 7, 8, 9].map(failure_line), [Some(53), Some(52), Some(220), Some(279), Some(53), Some(1418), None, Some(3984)]);
    }
}
