//! An NPC's number or text input (ZC_OPEN_EDITDLG, ZC_OPEN_EDITDLGSTR): a box of its own in the sys box frame with an
//! edit field and OK, at a place that depends only on the screen size.

use crate::render::{Gpu, Texture};
use crate::window::Chrome;
use crate::{Layer, offset, pickup};

const NUMBER_SIZE: [f32; 2] = [176.0, 46.0];
const TEXT_SIZE: [f32; 2] = [346.0, 46.0];
/// The number box's label, "Input number ".
const LABEL: &str = "Input number ";
const LABEL_AT: [f32; 2] = [15.0, 6.0];
const FIELD_AT: [f32; 2] = [15.0, 22.0];
const FIELD_HEIGHT: f32 = 16.0;
const NUMBER_FIELD: f32 = 99.0;
const TEXT_FIELD: f32 = 269.0;
const FIELD_FILL: [u8; 4] = [0xE8, 0xE8, 0xE8, 0xFF];
/// Where the typed text starts in the field.
const TYPED_AT: [f32; 2] = [2.0, 1.0];
const NUMBER_OK: [f32; 2] = [121.0, 14.0];
const TEXT_OK: [f32; 2] = [291.0, 14.0];
// ponytail: 12 and 70 characters until the game's limits are checked.
const NUMBER_LENGTH: usize = 12;
const TEXT_LENGTH: usize = 70;

/// Where both boxes go at this screen size: (392, 320) at 1024×768.
pub fn corner(screen: [f32; 2]) -> [f32; 2] {
    [((screen[0] - 640.0) / 2.0 + 200.0).floor(), (200.0 * screen[1] / 480.0).floor()]
}

/// A number as C's `atoi` reads it: an optional `-` and the digits up to the first other character, wrapping
/// rather than checking the range ("12abc" is 12, "abc" is 0).
pub fn atoi(text: &str) -> i32 {
    let text = text.trim_start();
    let (negative, digits) = text.strip_prefix('-').map_or((false, text.strip_prefix('+').unwrap_or(text)), |rest| (true, rest));
    let value = digits.chars().map_while(|character| character.to_digit(10)).fold(0i32, |value, digit| value.wrapping_mul(10).wrapping_add(digit as i32));
    if negative { value.wrapping_neg() } else { value }
}

/// What OK sends.
#[derive(Debug, PartialEq)]
pub enum Answer {
    Number(i32),
    Text(String),
}

/// The field's fill.
pub struct Assets {
    field: Texture,
}

impl Assets {
    pub fn load(gpu: &Gpu) -> Self {
        Self { field: gpu.solid(FIELD_FILL) }
    }
}

/// The box: a text or number field and what's typed in it.
pub struct Input {
    pub text: bool,
    pub typed: String,
}

impl Input {
    /// A number field starts with "0", a text field empty.
    pub fn new(text: bool) -> Self {
        Self { text, typed: if text { String::new() } else { "0".into() } }
    }

    fn size(&self) -> [f32; 2] {
        if self.text { TEXT_SIZE } else { NUMBER_SIZE }
    }

    /// OK's corner inside the box.
    fn ok_at(&self) -> [f32; 2] {
        if self.text { TEXT_OK } else { NUMBER_OK }
    }

    /// OK's corner on a screen this big.
    pub fn ok_on(&self, screen: [f32; 2]) -> [f32; 2] {
        offset(corner(screen), self.ok_at())
    }

    /// Typed characters, up to the field's length.
    pub fn type_text(&mut self, text: &str) {
        let room = if self.text { TEXT_LENGTH } else { NUMBER_LENGTH }.saturating_sub(self.typed.chars().count());
        self.typed.extend(text.chars().filter(|character| !character.is_control()).take(room));
    }

    pub fn backspace(&mut self) {
        self.typed.pop();
    }

    /// What OK sends: nothing for an empty field (the box stays); a number read like `atoi`; text up to its first LF.
    pub fn answer(&self) -> Option<Answer> {
        if self.typed.is_empty() {
            return None;
        }
        Some(if self.text { Answer::Text(self.typed.split('\n').next().unwrap_or_default().to_owned()) } else { Answer::Number(atoi(&self.typed)) })
    }

    /// The frame, the number box's label and the field with what's typed and the caret; OK is the caller's.
    pub fn draw<'a>(&self, layer: &mut Layer<'a>, chrome: &'a Chrome, frame: &'a pickup::Assets, assets: &'a Assets, screen: [f32; 2]) {
        let corner = corner(screen);
        let [width, height] = self.size();
        pickup::draw_sysbox(layer, chrome, frame, [corner[0], corner[1], width, height]);
        if !self.text {
            layer.texts.push((LABEL.to_owned(), offset(corner, LABEL_AT), [0, 0, 0]));
        }
        let field = offset(corner, FIELD_AT);
        layer.sprites.push((&assets.field, field, [if self.text { TEXT_FIELD } else { NUMBER_FIELD }, FIELD_HEIGHT]));
        layer.texts.push((format!("{}_", self.typed), offset(field, TYPED_AT), [0, 0, 0]));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_numbers_like_atoi() {
        assert_eq!([atoi("-5"), atoi("12abc"), atoi("abc"), atoi("025")], [-5, 12, 0, 25]);
        // An empty field sends nothing; "0" as it opens sends 0.
        let mut input = Input::new(false);
        assert_eq!(input.answer(), Some(Answer::Number(0)));
        input.backspace();
        assert_eq!(input.answer(), None);
        input.type_text("12345678901234567890");
        assert_eq!(input.typed, "123456789012");
    }

    #[test]
    fn sends_text_up_to_its_first_line_break() {
        let mut input = Input::new(true);
        assert_eq!(input.answer(), None);
        input.typed = "one\ntwo".into();
        assert_eq!(input.answer(), Some(Answer::Text("one".into())));
    }

    #[test]
    fn opens_at_the_same_place_for_both() {
        assert_eq!(corner([1024.0, 768.0]), [392.0, 320.0]);
        assert_eq!(corner([800.0, 600.0]), [280.0, 250.0]);
        assert_eq!(Input::new(false).ok_on([1024.0, 768.0]), [513.0, 334.0]);
        assert_eq!(Input::new(true).ok_on([800.0, 600.0]), [571.0, 264.0]);
    }
}
