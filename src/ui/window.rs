//! What the client's windows share: the title bar with its buttons, dragging, minimizing and tooltips.

use rustnarok_client::grf::Grf;
use rustnarok_client::text::Font;

use std::time::{Duration, Instant};

use crate::render::{Gpu, Texture};
use crate::{Layer, UI, contains, offset, screen};

pub const TITLE_HEIGHT: f32 = 17.0;
pub const BUTTON_SIZE: [f32; 2] = [11.0, 11.0];
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
    /// A cut text's whole-words balloon: the speech balloon's border, a darker fill.
    whole_fill: Texture,
    balloon_border: Texture,
    /// Laid over a button the client can't do yet, so it reads as switched off.
    pub dimmed: Texture,
}

impl Chrome {
    /// The title bar alone (its rounded caps around a stretched middle), `width` wide at `corner`.
    pub fn draw_title_bar<'a>(&'a self, layer: &mut Layer<'a>, corner: [f32; 2], width: f32) {
        let [left, middle, right] = &self.title;
        layer.sprites.push((left, corner, left.size));
        layer.sprites.push((middle, offset(corner, [left.size[0], 0.0]), [width - left.size[0] - right.size[0], TITLE_HEIGHT]));
        layer.sprites.push((right, offset(corner, [width - right.size[0], 0.0]), right.size));
    }

    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let basic = format!("{UI}basic_interface\\");
        let image = |path: String| gpu.load_texture(grf, &path, false);
        Self {
            title: ["left", "mid", "right"].map(|part| image(format!("{basic}titlebar_{part}.bmp"))),
            buttons: ["base", "mini", "close"].map(|name| ["off", "on"].map(|state| image(format!("{basic}sys_{name}_{state}.bmp")))),
            white: gpu.solid([255; 4]),
            border: gpu.solid(BORDER),
            tooltip: gpu.solid(TOOLTIP),
            whole_fill: gpu.solid([0, 0, 0, 0x88]),
            balloon_border: gpu.solid([0x5C, 0x5C, 0x5C, 0xFF]),
            dimmed: gpu.solid([0x90, 0x90, 0x90, 0xB0]),
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

/// A dragged window's edge this close to the screen's snaps to it.
const MAGNET: f32 = 10.0;
/// Dragged off screen, a window keeps this much of its width showing (and its title bar always), to grab it again.
const KEEP_SHOWING: f32 = 40.0;
/// Held by the pointer, a window is drawn this see-through (128 of 255 opaque), easing there and back over HOLD_EASE.
const HELD_FADED: f32 = 1.0 - 128.0 / 255.0;
const HOLD_EASE: Duration = Duration::from_millis(267);
/// A second press on the title bar this soon collapses or opens a window that does that.
const TITLE_DOUBLE_CLICK: Duration = Duration::from_millis(500);

/// Where a window `size` big goes when dragged to `to`: an edge within MAGNET of the screen's snaps flush, and it
/// stays where it can be grabbed again (the title bar on screen, KEEP_SHOWING of its width).
pub fn dragged_to(to: [f32; 2], size: [f32; 2]) -> [f32; 2] {
    let snap = |at: f32, length: f32, screen: f32| {
        if at.abs() <= MAGNET {
            0.0
        } else if (at + length - screen).abs() <= MAGNET {
            screen - length
        } else {
            at
        }
    };
    let x = snap(to[0], size[0], screen()[0]).clamp(KEEP_SHOWING - size[0], screen()[0] - KEEP_SHOWING);
    let y = snap(to[1], size[1], screen()[1]).clamp(0.0, screen()[1] - TITLE_HEIGHT);
    [x, y]
}

/// How see-through a window is `now`: easing to HELD_FADED while held (since `held`), and back once let go (at
/// `let_go`, from how faded it was then).
pub(crate) fn hold_fade(held: Option<Instant>, let_go: Option<(Instant, f32)>, now: Instant) -> f32 {
    let eased = |since: Instant| (now.saturating_duration_since(since).as_secs_f32() / HOLD_EASE.as_secs_f32()).min(1.0);
    match (held, let_go) {
        (Some(held), _) => HELD_FADED * eased(held),
        (None, Some((at, from))) => from * (1.0 - eased(at)),
        (None, None) => 0.0,
    }
}

/// A window's place on screen and its title bar's state.
pub struct Frame {
    pub open: bool,
    pub minimized: bool,
    pub position: [f32; 2],
    width: f32,
    /// Pointer offset into the title bar while the window is dragged.
    drag: Option<[f32; 2]>,
    /// When it was grabbed, or let go (and how faded it was then), for the see-through while held.
    held: Option<Instant>,
    let_go: Option<(Instant, f32)>,
    /// A double click on the title bar collapses or opens it; the last title press.
    collapses: bool,
    last_title_press: Option<Instant>,
}

impl Frame {
    pub fn new(position: [f32; 2], width: f32) -> Self {
        Self { open: false, minimized: false, position, width, drag: None, held: None, let_go: None, collapses: false, last_title_press: None }
    }

    /// The window collapses to its title bar on a double click there (inventory, equipment, status, basic info).
    pub fn collapsing(mut self) -> Self {
        self.collapses = true;
        self
    }

    /// The pointer is holding it.
    pub fn dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// How see-through to draw it now (0 opaque): half while it's held.
    pub fn faded(&self) -> f32 {
        hold_fade(self.held, self.let_go, crate::control::now())
    }

    pub fn toggle(&mut self) {
        self.open = !self.open;
    }

    /// The close button's corner inside the window.
    pub fn close_button(&self) -> [f32; 2] {
        [self.width - 14.0, 3.0]
    }

    /// The minimize button's corner inside the window.
    pub fn minimize_button(&self) -> [f32; 2] {
        [self.width - 29.0, 3.0]
    }

    /// Where `cursor` is inside the window (`height` tall when not minimized), if it is.
    pub fn local(&self, cursor: [f32; 2], height: f32) -> Option<[f32; 2]> {
        let height = if self.minimized { TITLE_HEIGHT } else { height };
        (self.open && contains(self.position, [self.width, height], cursor)).then(|| [cursor[0] - self.position[0], cursor[1] - self.position[1]])
    }

    /// A window whose width changes (the skill window's two views).
    pub fn set_width(&mut self, width: f32) {
        self.width = width;
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

    /// A press on the title bar grabs the window; a second one soon after collapses or opens one that does that.
    pub fn start_drag(&mut self, cursor: [f32; 2]) {
        let now = crate::control::now();
        if self.collapses && self.last_title_press.is_some_and(|at| now.saturating_duration_since(at) < TITLE_DOUBLE_CLICK) {
            self.minimized = !self.minimized;
            self.last_title_press = None;
        } else {
            self.last_title_press = Some(now);
        }
        self.drag = Some([cursor[0] - self.position[0], cursor[1] - self.position[1]]);
        self.held = Some(now);
        self.let_go = None;
    }

    /// Follows the pointer while dragged (see `dragged_to`); returns whether it is.
    pub fn drag_to(&mut self, cursor: [f32; 2], height: f32) -> bool {
        let Some(grab) = self.drag else { return false };
        let height = if self.minimized { TITLE_HEIGHT } else { height };
        self.position = dragged_to([cursor[0] - grab[0], cursor[1] - grab[1]], [self.width, height]);
        true
    }

    /// Ends a drag; returns whether one was going on.
    pub fn drop_drag(&mut self) -> bool {
        let now = crate::control::now();
        if let Some(held) = self.held.take() {
            self.let_go = Some((now, hold_fade(Some(held), None, now)));
        }
        self.drag.take().is_some()
    }

    /// A point inside the window, on screen.
    pub fn at(&self, local: [f32; 2]) -> [f32; 2] {
        offset(self.position, local)
    }

    /// The title bar: rounded caps around a stretched middle, the three buttons and the title with its white shadow.
    pub fn draw_title<'a>(&self, layer: &mut Layer<'a>, chrome: &'a Chrome, title: &str, cursor: [f32; 2]) {
        self.draw_bar(layer, chrome);
        self.draw_buttons(layer, chrome, cursor);
        layer.texts.push((title.to_owned(), self.at(offset(TITLE_TEXT, [1.0, 1.0])), [255, 255, 255]));
        layer.texts.push((title.to_owned(), self.at(TITLE_TEXT), [0, 0, 0]));
    }

    /// The title bar from one piece (`titlebar_fix.bmp`), the three buttons, and the title black at (16, 3) over white
    /// at (17, 4).
    pub fn draw_fixed_title<'a>(&self, layer: &mut Layer<'a>, chrome: &'a Chrome, bar: &'a Texture, title: &str, cursor: [f32; 2]) {
        layer.sprites.push((bar, self.at([0.0, 0.0]), bar.size));
        self.draw_buttons(layer, chrome, cursor);
        layer.texts.push((title.to_owned(), self.at([17.0, 4.0]), [255, 255, 255]));
        layer.texts.push((title.to_owned(), self.at([16.0, 3.0]), [0, 0, 0]));
    }

    /// The base, minimize and close buttons, lit when pointed at.
    fn draw_buttons<'a>(&self, layer: &mut Layer<'a>, chrome: &'a Chrome, cursor: [f32; 2]) {
        for (button, place) in [BASE_BUTTON, self.minimize_button(), self.close_button()].into_iter().enumerate() {
            let hovering = contains(self.at(place), BUTTON_SIZE, cursor);
            let image = &chrome.buttons[button][usize::from(hovering)];
            layer.sprites.push((image, self.at(place), image.size));
        }
    }

    /// The bar alone: rounded caps around a stretched middle.
    pub fn draw_bar<'a>(&self, layer: &mut Layer<'a>, chrome: &'a Chrome) {
        let [left, middle, right] = &chrome.title;
        layer.sprites.push((left, self.at([0.0, 0.0]), left.size));
        layer.sprites.push((middle, self.at([left.size[0], 0.0]), [self.width - left.size[0] - right.size[0], TITLE_HEIGHT]));
        layer.sprites.push((right, self.at([self.width - right.size[0], 0.0]), right.size));
    }

    /// The bar and the title, with no buttons.
    pub fn draw_title_only<'a>(&self, layer: &mut Layer<'a>, chrome: &'a Chrome, title: &str) {
        self.draw_bar(layer, chrome);
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

/// A cut text shown whole while the pointer is on it: the speech balloon's look with a darker fill, its words starting
/// on the cut text's own `spot`, kept on screen.
pub fn whole_text<'a>(layer: &mut Layer<'a>, chrome: &'a Chrome, font: &Font, text: &str, spot: [f32; 2]) {
    let size = crate::overhead::size(font.width(text), 1);
    let corner = crate::overhead::over_text_corner(spot, size, screen());
    layer.sprites.push((&chrome.whole_fill, corner, size));
    for (edge, piece) in crate::overhead::border(size) {
        layer.sprites.push((&chrome.balloon_border, offset(corner, edge), piece));
    }
    let place = offset(corner, crate::overhead::TEXT_AT);
    layer.texts.push((text.to_owned(), offset(place, [1.0, 1.0]), [0, 0, 0]));
    layer.texts.push((text.to_owned(), place, [255; 3]));
}

/// The client's tooltip: white text with a black shadow on a translucent black box.
pub fn tooltip<'a>(layer: &mut Layer<'a>, chrome: &'a Chrome, font: &Font, text: &str, corner: [f32; 2], color: [u8; 3]) {
    layer.sprites.push((&chrome.tooltip, corner, [font.width(text).ceil() + 8.0, 23.0]));
    layer.texts.push((text.to_owned(), offset(corner, [5.0, 5.0]), [0, 0, 0]));
    layer.texts.push((text.to_owned(), offset(corner, [4.0, 4.0]), color));
}

/// The tooltip box with lines of coloured pieces, each line a line height under the last.
pub fn tooltip_lines<'a>(layer: &mut Layer<'a>, chrome: &'a Chrome, font: &Font, lines: &[Vec<([u8; 3], String)>], corner: [f32; 2]) {
    let width = lines.iter().map(|line| line.iter().map(|(_, piece)| font.width(piece)).sum::<f32>()).fold(0.0, f32::max);
    layer.sprites.push((&chrome.tooltip, corner, [width.ceil() + 8.0, 8.0 + lines.len() as f32 * font.line_height()]));
    for (row, line) in lines.iter().enumerate() {
        let mut x = 4.0;
        for (color, piece) in line {
            let place = offset(corner, [x, 4.0 + row as f32 * font.line_height()]);
            layer.texts.push((piece.clone(), offset(place, [1.0, 1.0]), [0, 0, 0]));
            layer.texts.push((piece.clone(), place, *color));
            x += font.width(piece);
        }
    }
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

/// A window's corner grip: the 15×15 corner inside the window takes the press (its 16×16 bitmap hangs 1 px out).
pub const GRIP: f32 = 15.0;

/// Whether `local` (window-local) is on the corner grip of a window `size` big.
pub fn on_grip(size: [f32; 2], local: [f32; 2]) -> bool {
    contains([size[0] - GRIP, size[1] - GRIP], [GRIP; 2], local)
}

/// A corner grip being held: where the press was and the window's size then. The corner follows the pointer from
/// the press (moving back undoes it); each window clamps and snaps what it's asked for.
#[derive(Debug, Default, Clone, Copy)]
pub struct Grip(Option<([f32; 2], [f32; 2])>);

impl Grip {
    pub fn start(&mut self, cursor: [f32; 2], size: [f32; 2]) {
        self.0 = Some((cursor, size));
    }

    /// The size the pointer asks for while held.
    pub fn asked(&self, cursor: [f32; 2]) -> Option<[f32; 2]> {
        let (from, size) = self.0?;
        Some([size[0] + cursor[0] - from[0], size[1] + cursor[1] - from[1]])
    }

    /// Lets go; whether it was held.
    pub fn stop(&mut self) -> bool {
        self.0.take().is_some()
    }
}

/// The steps (0, 1, 2...) a size `asked` makes past `least`, `step` apart, clamped to `most` steps: the game's
/// least + step·trunc((clamp(asked) − least)/step), rounding toward the least.
pub fn steps(asked: f32, least: f32, step: f32, most: usize) -> usize {
    (((asked - least) / step).max(0.0) as usize).min(most)
}

/// Notices double clicks on the same thing (an item index).
#[derive(Default)]
pub struct Clicks(Option<(u16, std::time::Instant)>);

impl Clicks {
    const DOUBLE_CLICK: std::time::Duration = std::time::Duration::from_millis(500);

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
    /// The question (a message number).
    message: usize,
    /// A custom label (the item's name for a shop).
    label: Option<String>,
    /// OK only (no Cancel).
    ok_only: bool,
    /// How many digits go in; 0 takes a name instead.
    max_digits: usize,
}

impl<T> Amount<T> {
    /// A name's 23 bytes, about as many characters.
    const NAME: usize = 23;

    /// Asks how many ("Input Number"), with `start` already typed in.
    pub fn new(what: T, start: Option<u32>) -> Self {
        Self { what, typed: start.map(|amount| amount.to_string()).unwrap_or_default(), message: 1259, label: None, ok_only: false, max_digits: 5 }
    }

    /// Asks how many of a shop item to buy or sell: the item's name as its label, with OK only.
    pub fn shop(what: T, label: String, start: Option<u32>) -> Self {
        Self { what, typed: start.map(|amount| amount.to_string()).unwrap_or_default(), message: 1259, label: Some(label), ok_only: true, max_digits: 5 }
    }

    /// Asks for a buying-store price ("Price: "), up to 8 digits (the most is 99,990,000).
    pub fn price(what: T) -> Self {
        Self { what, typed: String::new(), message: 1721, label: None, ok_only: false, max_digits: 8 }
    }

    /// Asks for a name, under that message.
    pub fn name(what: T, message: usize) -> Self {
        Self { what, typed: String::new(), message, label: None, ok_only: false, max_digits: 0 }
    }

    /// The number typed; one too big for 32 bits is the most there is.
    pub fn value(&self) -> u32 {
        self.typed.parse::<u64>().map_or(0, |value| value.min(u64::from(u32::MAX)) as u32)
    }

    pub fn type_text(&mut self, text: &str) {
        let digits = self.max_digits > 0;
        let (limit, typed) = if digits { (self.max_digits, self.typed.len()) } else { (Self::NAME, self.typed.chars().count()) };
        self.typed.extend(
            text.chars().filter(|character| if digits { character.is_ascii_digit() } else { !character.is_control() }).take(limit.saturating_sub(typed)),
        );
    }

    /// The box's buttons and whether each answers yes (true for OK, false for Cancel):
    /// OK and Cancel, or OK alone where Cancel would be.
    pub fn buttons(&self) -> Vec<([f32; 2], bool)> {
        if self.ok_only { vec![(crate::RIGHT_BUTTON, true)] } else { vec![(crate::LEFT_BUTTON, true), (crate::RIGHT_BUTTON, false)] }
    }

    /// The question's label: the item's name when set, else the message.
    pub fn label<'a>(&'a self, messages: &'a [String]) -> &'a str {
        self.label.as_deref().unwrap_or_else(|| messages.get(self.message).map_or("", String::as_str))
    }

    pub fn corner(message_box: [f32; 2]) -> [f32; 2] {
        [((screen()[0] - message_box[0]) / 2.0).floor(), ((screen()[1] - message_box[1]) / 2.0).floor()]
    }

    /// Over the box: Some(Some(ok)) on its buttons, Some(None) elsewhere on it.
    pub fn hit(&self, cursor: [f32; 2], message_box: [f32; 2], button: [f32; 2]) -> Option<Option<bool>> {
        let corner = Self::corner(message_box);
        for (place, ok) in self.buttons() {
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
        let hit = self.hit(pointer.0, message_box.size, ok_cancel[0][0].size);
        for (place, ok) in self.buttons() {
            let images = &ok_cancel[usize::from(!ok)];
            let look = if hit == Some(Some(ok)) { 1 + usize::from(pointer.1) } else { 0 };
            layer.sprites.push((&images[look], offset(corner, place), images[look].size));
        }
        let text = self.label(messages);
        layer.texts.push((text.to_owned(), offset(corner, [20.0, 30.0]), [0, 0, 0]));
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
    fn grips_follow_the_pointer_from_the_press() {
        let size = [280.0, 300.0];
        assert!(on_grip(size, [266.0, 286.0]) && on_grip(size, [279.0, 299.0]));
        assert!(!on_grip(size, [264.0, 290.0]) && !on_grip(size, [280.0, 290.0]));
        let mut grip = Grip::default();
        assert_eq!(grip.asked([0.0, 0.0]), None);
        grip.start([500.0, 400.0], size);
        assert_eq!(grip.asked([540.0, 380.0]), Some([320.0, 280.0]));
        assert_eq!(grip.asked([500.0, 400.0]), Some(size), "moving back undoes it");
        assert!(grip.stop() && !grip.stop());
        // Steps round toward the least, and stop at the most.
        assert_eq!([100.0, 120.0, 151.0, 152.0, 1000.0].map(|asked| steps(asked, 120.0, 32.0, 6)), [0, 0, 0, 1, 6]);
    }

    #[test]
    fn dragged_windows_snap_to_the_edges_and_stay_in_reach() {
        let size = [280.0, 300.0];
        // Within 10 px of an edge: flush. Farther: where it was put.
        assert_eq!(dragged_to([7.0, 9.0], size), [0.0, 0.0]);
        assert_eq!(dragged_to([1024.0 - 280.0 - 10.0, 768.0 - 300.0 + 4.0], size), [744.0, 468.0]);
        assert_eq!(dragged_to([11.0, 12.0], size), [11.0, 12.0]);
        // Off screen it keeps 40 px showing and its title bar.
        assert_eq!(dragged_to([-500.0, -50.0], size), [-240.0, 0.0]);
        assert_eq!(dragged_to([2000.0, 2000.0], size), [984.0, 768.0 - TITLE_HEIGHT]);
    }

    #[test]
    fn held_windows_fade_to_half_and_back() {
        let start = Instant::now();
        let at = |ms: u64| start + Duration::from_millis(ms);
        assert_eq!(hold_fade(None, None, start), 0.0);
        assert!((hold_fade(Some(start), None, at(267)) - HELD_FADED).abs() < 1e-4);
        assert!((hold_fade(Some(start), None, at(133)) - HELD_FADED / 2.0).abs() < 0.01);
        // Let go halfway faded: back to opaque over the same time.
        assert!((hold_fade(None, Some((at(0), 0.25)), at(133)) - 0.125).abs() < 0.01);
        assert_eq!(hold_fade(None, Some((at(0), 0.25)), at(400)), 0.0);
    }

    #[test]
    fn a_double_click_on_the_title_collapses_a_window_that_does_that() {
        let mut inventory = Frame::new([0.0, 0.0], 280.0).collapsing();
        inventory.start_drag([5.0, 5.0]);
        inventory.drop_drag();
        assert!(!inventory.minimized);
        inventory.start_drag([5.0, 5.0]);
        inventory.drop_drag();
        assert!(inventory.minimized);
        let mut storage = Frame::new([0.0, 0.0], 280.0);
        storage.start_drag([5.0, 5.0]);
        storage.start_drag([5.0, 5.0]);
        assert!(!storage.minimized);
    }

    #[test]
    fn wraps_whole_words() {
        let font = Font::load();
        let width = font.width("Remove the target at");
        assert_eq!(wrap(&font, "Remove the target at the back space", width, 9), ["Remove the target at", "the back space"]);
        assert_eq!(wrap(&font, "Remove the targetted", font.width("Remove the tar"), 9), ["Remove the", "targetted"]);
        let korean = "타나토스타워하층부";
        assert!(wrap(&font, korean, font.width("타나토스"), 9).len() > 1, "no spaces: anywhere");
    }

    #[test]
    fn limits_what_goes_in_each_prompt() {
        let mut amount = Amount::new((), None);
        amount.type_text("12a345678");
        assert_eq!(amount.typed, "12345");
        let mut price = Amount::price(());
        price.type_text("999999999");
        assert_eq!(price.typed, "99999999");
        let mut name = Amount::name((), 0);
        name.type_text("A name longer than twenty-three");
        assert_eq!(name.typed, "A name longer than twen");
    }

    #[test]
    fn amount_box_shows_item_name_and_ok_only() {
        let messages = vec!["".to_owned(); 2000];
        let generic = Amount::new((), Some(5));
        assert_eq!(generic.label(&messages), "");
        assert_eq!(generic.buttons(), vec![(crate::LEFT_BUTTON, true), (crate::RIGHT_BUTTON, false)]);

        let shop_box = Amount::shop((), "Red Potion".to_owned(), Some(1));
        assert_eq!(shop_box.label(&messages), "Red Potion");
        assert_eq!(shop_box.buttons(), vec![(crate::RIGHT_BUTTON, true)]);

        let msg_size = [280.0, 120.0];
        let btn_size = [42.0, 20.0];
        let corner = Amount::<()>::corner(msg_size);
        let right_btn = offset(corner, crate::RIGHT_BUTTON);
        let left_btn = offset(corner, crate::LEFT_BUTTON);
        // Generic has Cancel on right, OK on left:
        assert_eq!(generic.hit(right_btn, msg_size, btn_size), Some(Some(false)));
        assert_eq!(generic.hit(left_btn, msg_size, btn_size), Some(Some(true)));
        // Shop box has OK on right, body on left:
        assert_eq!(shop_box.hit(right_btn, msg_size, btn_size), Some(Some(true)));
        assert_eq!(shop_box.hit(left_btn, msg_size, btn_size), Some(None));
    }
}
