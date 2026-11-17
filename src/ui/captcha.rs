//! A server's captcha check (rathena's macro detector): the picture comes in chunks (ZC_APPLY_MACRO_DETECTOR_CAPTCHA),
//! then a box shows it with the tries and the time left and a field for the answer. Nothing closes it but the server
//! (ZC_CLOSE_MACRO_DETECTOR): it takes every key and click while it's up.

use std::io::Read;
use std::time::{Duration, Instant};

use image::RgbaImage;
use rustnarok_client::text::Font;

use crate::messages;
use crate::render::{Gpu, Texture};
use crate::window::{self, Chrome};
use crate::{Layer, offset};

// ponytail: the layout is ours (the game's box wasn't measured): the picture with its lines under it, the field and OK.
const SIZE: [f32; 2] = [260.0, 222.0];
const PICTURE_AT: [f32; 2] = [20.0, 26.0];
const PICTURE_SIZE: [f32; 2] = [220.0, 90.0];
const TRIES_AT: [f32; 2] = [20.0, 124.0];
const TIME_AT: [f32; 2] = [20.0, 140.0];
const FIELD_AT: [f32; 2] = [20.0, 164.0];
const FIELD_SIZE: [f32; 2] = [160.0, 18.0];
const FIELD_FILL: [u8; 4] = [0xE8, 0xE8, 0xE8, 0xFF];
const TYPED_AT: [f32; 2] = [3.0, 2.0];
const OK_AT: [f32; 2] = [196.0, 163.0];
/// "Enter security code", "Remaining chance : %d", "Remaining Time %02d: %02d: %02d".
const TITLE: usize = 4022;
const TRIES: usize = 2886;
const TIME: usize = 4019;
/// The answer's most letters: 16 bytes with the NUL that ends it.
const ANSWER_LETTERS: usize = 15;
/// The picture's most bytes once inflated: rathena's 220 x 90, 24-bit BMP.
const BMP_BYTES: u64 = 54 + 3 * 220 * 90;

pub struct Assets {
    field: Texture,
}

impl Assets {
    pub fn load(gpu: &Gpu) -> Self {
        Self { field: gpu.solid(FIELD_FILL) }
    }
}

/// A check in progress: the picture as it arrives, then the box.
pub struct Captcha {
    /// The compressed picture's size and the bytes so far.
    size: usize,
    data: Vec<u8>,
    pub picture: Option<Texture>,
    /// Once asked: the tries left and when the time runs out.
    pub asked: Option<(u8, Instant)>,
    pub typed: String,
}

impl Captcha {
    pub fn new(size: u16) -> Self {
        Self { size: usize::from(size), data: Vec::new(), picture: None, asked: None, typed: String::new() }
    }

    /// A chunk of the picture; the whole compressed picture with the one that completes it.
    pub fn add(&mut self, chunk: &[u8]) -> Option<&[u8]> {
        if self.data.len() >= self.size {
            return None;
        }
        let room = self.size - self.data.len();
        self.data.extend_from_slice(&chunk[..chunk.len().min(room)]);
        (self.data.len() == self.size).then_some(&self.data[..])
    }

    /// The server asks (again): the field empties.
    pub fn ask(&mut self, tries: u8, ms: i32, now: Instant) {
        self.asked = Some((tries, now + Duration::from_millis(ms.max(0) as u64)));
        self.typed.clear();
    }

    pub fn type_text(&mut self, text: &str) {
        let room = ANSWER_LETTERS.saturating_sub(self.typed.chars().count());
        self.typed.extend(text.chars().filter(|character| !character.is_control()).take(room));
    }

    /// What Enter or OK sends: the typed answer, unless nothing is typed or nothing was asked; the field empties.
    pub fn answer(&mut self) -> Option<String> {
        self.asked?;
        (!self.typed.is_empty()).then(|| std::mem::take(&mut self.typed))
    }

    /// Whole seconds left, rounded up.
    pub fn seconds_left(&self, now: Instant) -> u64 {
        self.asked.map_or(0, |(_, until)| until.saturating_duration_since(now).as_millis().div_ceil(1000) as u64)
    }
}

/// The picture as rathena sends it: a zlib-compressed BMP.
pub fn decode_picture(data: &[u8]) -> Option<RgbaImage> {
    let mut bmp = Vec::new();
    flate2::read::ZlibDecoder::new(data).take(BMP_BYTES).read_to_end(&mut bmp).ok()?;
    Some(image::load_from_memory_with_format(&bmp, image::ImageFormat::Bmp).ok()?.to_rgba8())
}

/// The box's corner: centred on the screen.
pub fn corner(screen: [f32; 2]) -> [f32; 2] {
    [((screen[0] - SIZE[0]) / 2.0).floor(), ((screen[1] - SIZE[1]) / 2.0).floor()]
}

/// OK's corner on the screen.
pub fn ok_on(screen: [f32; 2]) -> [f32; 2] {
    offset(corner(screen), OK_AT)
}

impl Captcha {
    /// The box once asked: the title bar, the picture, the tries and time left, the field with the caret; OK is the
    /// caller's.
    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(&'a self, layer: &mut Layer<'a>, chrome: &'a Chrome, assets: &'a Assets, font: &Font, messages: &[String], screen: [f32; 2], now: Instant) {
        let Some((tries, _)) = self.asked else { return };
        let corner = corner(screen);
        let at = |local: [f32; 2]| offset(corner, local);
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        chrome.draw_title_bar(layer, corner, SIZE[0]);
        chrome.draw_body(layer, corner, SIZE);
        chrome.draw_footer(layer, at([0.0, SIZE[1]]), SIZE[0], 21.0);
        layer.texts.push((window::fit(font, message(TITLE), SIZE[0] - 20.0), at([16.0, 2.0]), [0, 0, 0]));
        if let Some(picture) = &self.picture {
            layer.sprites.push((picture, at(PICTURE_AT), PICTURE_SIZE));
        }
        layer.texts.push((messages::fill(message(TRIES), &[&tries]), at(TRIES_AT), [0, 0, 0]));
        let left = self.seconds_left(now);
        layer.texts.push((messages::fill(message(TIME), &[&(left / 3600), &(left / 60 % 60), &(left % 60)]), at(TIME_AT), [0, 0, 0]));
        layer.sprites.push((&assets.field, at(FIELD_AT), FIELD_SIZE));
        layer.texts.push((format!("{}_", self.typed), at(offset(FIELD_AT, TYPED_AT)), [0, 0, 0]));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn puts_the_picture_together_from_its_chunks() {
        // A small 24-bit BMP, compressed as rathena does, sent in chunks of 10.
        let mut bmp = std::io::Cursor::new(Vec::new());
        image::RgbImage::from_fn(4, 2, |x, y| image::Rgb([x as u8 * 60, y as u8 * 100, 7])).write_to(&mut bmp, image::ImageFormat::Bmp).unwrap();
        let mut zipped = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        zipped.write_all(bmp.get_ref()).unwrap();
        let zipped = zipped.finish().unwrap();
        let mut captcha = Captcha::new(zipped.len() as u16);
        let chunks: Vec<&[u8]> = zipped.chunks(10).collect();
        for chunk in &chunks[..chunks.len() - 1] {
            assert!(captcha.add(chunk).is_none());
        }
        let whole = captcha.add(chunks[chunks.len() - 1]).unwrap().to_vec();
        assert_eq!(whole, zipped);
        let picture = decode_picture(&whole).unwrap();
        assert_eq!((picture.dimensions(), picture.get_pixel(3, 1).0), ((4, 2), [180, 100, 7, 255]));
        // More after it's whole changes nothing; a broken picture decodes to nothing.
        assert!(captcha.add(&[1, 2]).is_none());
        assert!(decode_picture(&whole[..whole.len() / 2]).is_none());
    }

    #[test]
    fn answers_only_when_asked() {
        let now = Instant::now();
        let mut captcha = Captcha::new(10);
        captcha.type_text("abc");
        assert_eq!(captcha.answer(), None);
        captcha.ask(3, 60_000, now);
        assert!(captcha.typed.is_empty());
        assert_eq!(captcha.seconds_left(now), 60);
        assert_eq!(captcha.seconds_left(now + Duration::from_millis(500)), 60);
        assert_eq!(captcha.answer(), None);
        // At most 15 letters, no control characters; Enter sends and empties the field.
        captcha.type_text("0123456789abcdef\r");
        assert_eq!(captcha.answer().as_deref(), Some("0123456789abcde"));
        assert!(captcha.typed.is_empty());
        assert_eq!(captcha.seconds_left(now + Duration::from_secs(61)), 0);
    }
}
