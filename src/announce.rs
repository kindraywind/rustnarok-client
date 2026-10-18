//! The announcement bar: server broadcasts in boxes across the top of the screen, as well as in the chat; three at
//! most, each for 20 seconds.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};
use crate::{Layer, screen};

/// The bar's top, below the top of the screen.
const TOP: f32 = 51.0;
const LASTS: Duration = Duration::from_secs(20);
const MOST: usize = 3;
/// Wrapped at this many bytes a line (EUC-KR, as the client counts).
const LINE_BYTES: usize = 72;
/// Text starts this far inside its box; lines are the text's height plus `LINE_GAP` apart; messages `MESSAGE_GAP`.
const INSET: f32 = 6.0;
const LINE_GAP: f32 = 4.0;
const MESSAGE_GAP: f32 = 10.0;
pub const SIZE: u16 = 12;
const BOX: [u8; 4] = [0x5C, 0x5C, 0x5C, 0xFF];

struct Announcement {
    text: String,
    color: [u8; 3],
    size: u16,
    at: Instant,
    /// Its lines, rendered once.
    lines: Option<Vec<Texture>>,
}

#[derive(Default)]
pub struct Bar {
    messages: VecDeque<Announcement>,
    background: Option<Texture>,
}

/// `text` cut into lines of at most `LINE_BYTES` bytes as EUC-KR counts them (2 for a Hangul letter), whole letters.
pub fn wrap(text: &str) -> Vec<String> {
    let mut lines = vec![String::new()];
    let mut bytes = 0;
    for character in text.chars() {
        let width = if character.is_ascii() { 1 } else { 2 };
        if bytes + width > LINE_BYTES {
            lines.push(String::new());
            bytes = 0;
        }
        lines.last_mut().unwrap().push(character);
        bytes += width;
    }
    lines
}

impl Bar {
    /// A broadcast: the oldest goes when there are already three.
    pub fn push(&mut self, text: String, color: [u8; 3], size: u16, now: Instant) {
        if self.messages.len() == MOST {
            self.messages.pop_front();
        }
        self.messages.push_back(Announcement { text, color, size, at: now, lines: None });
    }

    /// Takes off the messages whose time is up.
    pub fn expire(&mut self, now: Instant) {
        self.messages.retain(|message| now.saturating_duration_since(message.at) < LASTS);
    }

    /// The texts showing, oldest first.
    pub fn texts(&self) -> Vec<String> {
        self.messages.iter().map(|message| message.text.clone()).collect()
    }

    /// Renders the lines not drawn yet.
    pub fn prepare(&mut self, gpu: &Gpu, font: &Font) {
        self.background.get_or_insert_with(|| gpu.solid(BOX));
        for message in self.messages.iter_mut().filter(|message| message.lines.is_none()) {
            let lines = wrap(&message.text).iter().map(|line| gpu.texture(&font.render_sized(line, message.color, f32::from(message.size)), false)).collect();
            message.lines = Some(lines);
        }
    }

    pub fn draw<'a>(&'a self, layer: &mut Layer<'a>) {
        let Some(background) = &self.background else { return };
        let mut top = TOP;
        for message in &self.messages {
            let Some(lines) = &message.lines else { continue };
            let width = lines.iter().map(|line| line.size[0]).fold(0.0, f32::max) + 2.0 * INSET;
            let height: f32 = lines.iter().map(|line| line.size[1] + LINE_GAP).sum::<f32>() + 2.0 * INSET - LINE_GAP;
            let left = ((screen()[0] - width) / 2.0).floor();
            layer.sprites.push((background, [left, top], [width, height]));
            let mut y = top + INSET;
            for line in lines {
                layer.sprites.push((line, [left + INSET, y], line.size));
                y += line.size[1] + LINE_GAP;
            }
            top += height + MESSAGE_GAP;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_at_72_bytes() {
        let lines = wrap(&"x".repeat(150));
        assert_eq!(lines.iter().map(String::len).collect::<Vec<_>>(), [72, 72, 6]);
        // Hangul counts two bytes a letter.
        assert_eq!(wrap(&"가".repeat(40)).iter().map(|line| line.chars().count()).collect::<Vec<_>>(), [36, 4]);
        assert_eq!(wrap("Hello"), ["Hello"]);
    }

    #[test]
    fn keeps_the_newest_three_for_20_seconds() {
        let mut bar = Bar::default();
        let now = Instant::now();
        for (n, text) in ["One", "Two", "Three", "Four"].into_iter().enumerate() {
            bar.push(text.into(), [255, 255, 0], SIZE, now + Duration::from_secs(n as u64));
        }
        assert_eq!(bar.texts(), ["Two", "Three", "Four"]);
        bar.expire(now + Duration::from_millis(21_500));
        assert_eq!(bar.texts(), ["Three", "Four"], "Two came at 1 s");
        bar.expire(now + Duration::from_secs(24));
        assert!(bar.texts().is_empty());
    }
}
