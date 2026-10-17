//! What the client's windows share: the title bar with its buttons, dragging, minimizing and tooltips.

use rustnarok_client::grf::Grf;
use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};
use crate::{Layer, SCREEN, UI, contains, offset};

pub const TITLE_HEIGHT: f32 = 17.0;
const BUTTON_SIZE: [f32; 2] = [11.0, 11.0];
const BASE_BUTTON: [f32; 2] = [3.0, 3.0];
const TITLE_TEXT: [f32; 2] = [17.0, 1.0];
const BORDER: [u8; 4] = [0xCC, 0xCC, 0xCC, 0xFF];
const TOOLTIP: [u8; 4] = [0, 0, 0, 0xB3];

/// Bitmaps every window uses, all from data.grf.
pub struct Chrome {
    title: [Texture; 3],
    /// Normal and hover images of the base, minimize and close buttons.
    buttons: [[Texture; 2]; 3],
    pub white: Texture,
    pub border: Texture,
    tooltip: Texture,
}

impl Chrome {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let basic = format!("{UI}basic_interface\\");
        let image = |path: String| gpu.load_texture(grf, &path, false);
        Self {
            title: ["left", "mid", "right"].map(|part| image(format!("{basic}titlebar_{part}.bmp"))),
            buttons: ["base", "mini", "close"].map(|name| ["off", "on"].map(|state| image(format!("{basic}sys_{name}_{state}.bmp")))),
            white: gpu.solid([255; 4]),
            border: gpu.solid(BORDER),
            tooltip: gpu.solid(TOOLTIP),
        }
    }
}

impl Chrome {
    /// The base (0), minimize (1) or close (2) button, lit when hovered.
    pub fn button(&self, button: usize, hovered: bool) -> &Texture {
        &self.buttons[button][usize::from(hovered)]
    }
}

/// A click on the title bar.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum TitleHit {
    Title,
    Minimize,
    Close,
}

/// A window's place on screen and its title bar's state.
pub struct Frame {
    pub open: bool,
    pub minimized: bool,
    pub position: [f32; 2],
    width: f32,
    /// Pointer offset into the title bar while the window is dragged.
    drag: Option<[f32; 2]>,
}

impl Frame {
    pub fn new(position: [f32; 2], width: f32) -> Self {
        Self { open: false, minimized: false, position, width, drag: None }
    }

    pub fn toggle(&mut self) {
        self.open = !self.open;
    }

    fn close_button(&self) -> [f32; 2] {
        [self.width - 14.0, 3.0]
    }

    fn minimize_button(&self) -> [f32; 2] {
        [self.width - 29.0, 3.0]
    }

    /// Where `cursor` is inside the window (`height` tall when not minimized), if it is.
    pub fn local(&self, cursor: [f32; 2], height: f32) -> Option<[f32; 2]> {
        let height = if self.minimized { TITLE_HEIGHT } else { height };
        (self.open && contains(self.position, [self.width, height], cursor)).then(|| [cursor[0] - self.position[0], cursor[1] - self.position[1]])
    }

    pub fn title_hit(&self, local: [f32; 2]) -> Option<TitleHit> {
        if contains(self.close_button(), BUTTON_SIZE, local) {
            Some(TitleHit::Close)
        } else if contains(self.minimize_button(), BUTTON_SIZE, local) {
            Some(TitleHit::Minimize)
        } else {
            (local[1] < TITLE_HEIGHT).then_some(TitleHit::Title)
        }
    }

    pub fn click_title(&mut self, hit: TitleHit) {
        match hit {
            TitleHit::Close => self.open = false,
            TitleHit::Minimize => self.minimized = !self.minimized,
            TitleHit::Title => {}
        }
    }

    pub fn start_drag(&mut self, cursor: [f32; 2]) {
        self.drag = Some([cursor[0] - self.position[0], cursor[1] - self.position[1]]);
    }

    /// Follows the pointer while dragged, keeping the window (`height` tall) on screen; returns whether it is.
    pub fn drag_to(&mut self, cursor: [f32; 2], height: f32) -> bool {
        let Some(grab) = self.drag else { return false };
        let height = if self.minimized { TITLE_HEIGHT } else { height };
        self.position = [(cursor[0] - grab[0]).clamp(0.0, SCREEN[0] - self.width), (cursor[1] - grab[1]).clamp(0.0, SCREEN[1] - height)];
        true
    }

    /// Ends a drag; returns whether one was going on.
    pub fn drop_drag(&mut self) -> bool {
        self.drag.take().is_some()
    }

    /// A point inside the window, on screen.
    pub fn at(&self, local: [f32; 2]) -> [f32; 2] {
        offset(self.position, local)
    }

    /// The title bar: rounded caps around a stretched middle, the three buttons and the title with its white shadow.
    pub fn draw_title<'a>(&self, layer: &mut Layer<'a>, chrome: &'a Chrome, title: &str, cursor: [f32; 2]) {
        let [left, middle, right] = &chrome.title;
        layer.sprites.push((left, self.at([0.0, 0.0]), left.size));
        layer.sprites.push((middle, self.at([left.size[0], 0.0]), [self.width - left.size[0] - right.size[0], TITLE_HEIGHT]));
        layer.sprites.push((right, self.at([self.width - right.size[0], 0.0]), right.size));
        for (button, place) in [BASE_BUTTON, self.minimize_button(), self.close_button()].into_iter().enumerate() {
            let hovering = contains(self.at(place), BUTTON_SIZE, cursor);
            let image = &chrome.buttons[button][usize::from(hovering)];
            layer.sprites.push((image, self.at(place), image.size));
        }
        layer.texts.push((title.to_owned(), self.at(offset(TITLE_TEXT, [1.0, 1.0])), [255, 255, 255]));
        layer.texts.push((title.to_owned(), self.at(TITLE_TEXT), [0, 0, 0]));
    }

    /// The title over a background that brings its own bar: the close button and the title only.
    pub fn draw_bare_title<'a>(&self, layer: &mut Layer<'a>, chrome: &'a Chrome, title: &str, cursor: [f32; 2]) {
        let place = self.close_button();
        let image = chrome.button(2, contains(self.at(place), BUTTON_SIZE, cursor));
        layer.sprites.push((image, self.at(place), image.size));
        layer.texts.push((title.to_owned(), self.at(offset(TITLE_TEXT, [1.0, 1.0])), [255, 255, 255]));
        layer.texts.push((title.to_owned(), self.at(TITLE_TEXT), [0, 0, 0]));
    }
}

/// The client's tooltip: white text with a black shadow on a translucent black box.
pub fn tooltip<'a>(layer: &mut Layer<'a>, chrome: &'a Chrome, font: &Font, text: &str, corner: [f32; 2], color: [u8; 3]) {
    layer.sprites.push((&chrome.tooltip, corner, [font.width(text).ceil() + 8.0, 23.0]));
    layer.texts.push((text.to_owned(), offset(corner, [5.0, 5.0]), [0, 0, 0]));
    layer.texts.push((text.to_owned(), offset(corner, [4.0, 4.0]), color));
}

/// The text cut short with "..." to fit `width`.
pub fn fit(font: &Font, text: &str, width: f32) -> String {
    if font.width(text) <= width {
        return text.to_owned();
    }
    let mut cut = text.to_owned();
    while !cut.is_empty() && font.width(&format!("{cut}...")) > width {
        cut.pop();
    }
    format!("{cut}...")
}

/// Splits text into lines no wider than `width`, keeping at most `lines` of them. Lines break after their last space
/// so words stay whole; text without spaces (Korean) breaks anywhere.
pub fn wrap(font: &Font, text: &str, width: f32, lines: usize) -> Vec<String> {
    let mut wrapped = vec![String::new()];
    for character in text.chars() {
        let line = wrapped.last_mut().unwrap();
        if !line.is_empty() && font.width(&format!("{line}{character}")) > width {
            let carried = match line.rfind(' ') {
                Some(space) if character != ' ' => line.split_off(space + 1),
                _ => String::new(),
            };
            line.truncate(line.trim_end().len());
            wrapped.push(carried);
            if character == ' ' {
                continue;
            }
        }
        wrapped.last_mut().unwrap().push(character);
    }
    wrapped.truncate(lines);
    wrapped
}

/// Notices double clicks on the same thing (an item index).
#[derive(Default)]
pub struct Clicks(Option<(u16, std::time::Instant)>);

impl Clicks {
    const DOUBLE_CLICK: std::time::Duration = std::time::Duration::from_millis(400);

    /// Records a click; returns whether it completes a double click.
    pub fn double(&mut self, index: u16) -> bool {
        let now = std::time::Instant::now();
        let double = self.0.is_some_and(|(last, at)| last == index && now.duration_since(at) < Self::DOUBLE_CLICK);
        self.0 = (!double).then_some((index, now));
        double
    }
}

/// The client's box asking how many (or for a name), in the middle of the screen: what's typed, OK and cancel.
pub struct Amount<T> {
    /// What the answer is for.
    pub what: T,
    pub typed: String,
    /// The question (a message number), and whether only digits go in.
    message: usize,
    digits: bool,
}

impl<T> Amount<T> {
    const DIGITS: usize = 5;
    /// A name's 23 bytes, about as many characters.
    const NAME: usize = 23;

    /// Asks how many ("Input Number"), with `start` already typed in.
    pub fn new(what: T, start: Option<u32>) -> Self {
        Self { what, typed: start.map(|amount| amount.to_string()).unwrap_or_default(), message: 1259, digits: true }
    }

    /// Asks for a name, under that message.
    pub fn name(what: T, message: usize) -> Self {
        Self { what, typed: String::new(), message, digits: false }
    }

    pub fn value(&self) -> u32 {
        self.typed.parse().unwrap_or(0)
    }

    pub fn type_text(&mut self, text: &str) {
        let (limit, typed) = if self.digits { (Self::DIGITS, self.typed.len()) } else { (Self::NAME, self.typed.chars().count()) };
        let digits = self.digits;
        self.typed.extend(
            text.chars().filter(|character| if digits { character.is_ascii_digit() } else { !character.is_control() }).take(limit.saturating_sub(typed)),
        );
    }

    fn corner(message_box: [f32; 2]) -> [f32; 2] {
        [((SCREEN[0] - message_box[0]) / 2.0).floor(), ((SCREEN[1] - message_box[1]) / 2.0).floor()]
    }

    /// Over the box: Some(Some(ok)) on its buttons, Some(None) elsewhere on it.
    pub fn hit(cursor: [f32; 2], message_box: [f32; 2], button: [f32; 2]) -> Option<Option<bool>> {
        let corner = Self::corner(message_box);
        for (place, ok) in [(crate::LEFT_BUTTON, true), (crate::RIGHT_BUTTON, false)] {
            if contains(offset(corner, place), button, cursor) {
                return Some(Some(ok));
            }
        }
        contains(corner, message_box, cursor).then_some(None)
    }

    pub fn draw<'a>(
        &self,
        layer: &mut Layer<'a>,
        chrome: &'a Chrome,
        message_box: &'a Texture,
        ok_cancel: [&'a [Texture; 3]; 2],
        messages: &[String],
        pointer: ([f32; 2], bool),
    ) {
        let corner = Self::corner(message_box.size);
        layer.sprites.push((message_box, corner, message_box.size));
        let hit = Self::hit(pointer.0, message_box.size, ok_cancel[0][0].size);
        for (images, (place, ok)) in ok_cancel.into_iter().zip([(crate::LEFT_BUTTON, true), (crate::RIGHT_BUTTON, false)]) {
            let look = if hit == Some(Some(ok)) { 1 + usize::from(pointer.1) } else { 0 };
            layer.sprites.push((&images[look], offset(corner, place), images[look].size));
        }
        layer.texts.push((messages.get(self.message).cloned().unwrap_or_default(), offset(corner, [20.0, 30.0]), [0, 0, 0]));
        let field = offset(corner, [20.0, 55.0]);
        layer.sprites.push((&chrome.border, offset(field, [-1.0, -1.0]), [242.0, 20.0]));
        layer.sprites.push((&chrome.white, field, [240.0, 18.0]));
        layer.texts.push((format!("{}_", self.typed), offset(field, [3.0, 2.0]), [0, 0, 0]));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_whole_words() {
        let font = Font::load();
        let width = font.width("Remove the target at");
        assert_eq!(wrap(&font, "Remove the target at the back space", width, 9), ["Remove the target at", "the back space"]);
        assert_eq!(wrap(&font, "Remove the targetted", font.width("Remove the tar"), 9), ["Remove the", "targetted"]);
        let korean = "타나토스타워하층부";
        assert!(wrap(&font, korean, font.width("타나토스"), 9).len() > 1, "no spaces: anywhere");
    }
}
