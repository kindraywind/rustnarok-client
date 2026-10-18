//! An NPC's say window (ZC_SAY_DIALOG): 280×180 in the sys box frame with no title bar, its text on a grey panel
//! wrapped character by character at 215 px and scrolled only by the player, and the Next or Close button.

use std::time::{Duration, Instant};

use crate::render::{Gpu, Texture};
use crate::window::{Chrome, Frame};
use crate::{Layer, contains, pickup};

pub const SIZE: [f32; 2] = [280.0, 180.0];
/// Where it first opens, at every screen size; then where it was left.
pub const DEFAULT_POSITION: [f32; 2] = [200.0, 100.0];
/// The text panel, filled #F2F2F2.
pub const PANEL: [f32; 4] = [10.0, 10.0, 260.0, 140.0];
pub const PANEL_FILL: [u8; 4] = [0xF2, 0xF2, 0xF2, 0xFF];
/// Where the text starts, and where its lines break.
pub const TEXT_AT: [f32; 2] = [13.0, 10.0];
pub const WRAP_WIDTH: f32 = 215.0;
/// Lines are the font's line height plus this apart.
pub const LINE_GAP: f32 = 6.0;
/// The scrollbar, while the text is taller than the panel.
pub const SCROLLBAR: [f32; 4] = [257.0, 10.0, 13.0, 140.0];
/// The wheel and Up/Down scroll this much.
pub const SCROLL_STEP: f32 = 30.0;
// ponytail: plain fills for the scrollbar; the scroll bitmaps when someone wants them.
const TRACK: [u8; 4] = [0xE0, 0xE0, 0xE0, 0xFF];
const THUMB: [u8; 4] = [0xA8, 0xA8, 0xA8, 0xFF];
const THUMB_MIN: f32 = 14.0;
/// Next and Close.
pub const BUTTON_AT: [f32; 2] = [234.0, 156.0];
pub const BUTTON: [f32; 2] = [42.0, 20.0];
/// Next, a clicked Close and the menu's OK are each ignored until this long after the last of them went through.
pub const THROTTLE: Duration = Duration::from_millis(300);

/// The server's text: the two-character `\r`, `\n` and `\"` as CR, LF and `"`.
pub fn unescape(text: &str) -> String {
    text.replace("\\r", "\r").replace("\\n", "\n").replace("\\\"", "\"")
}

/// How far text this tall can scroll: past the panel's height.
pub fn scroll_range(text_height: f32) -> f32 {
    (text_height - PANEL[3]).max(0.0)
}

/// The panel's fill and the scrollbar's track and thumb (the frame is the system box's, `pickup::Assets`).
pub struct Assets {
    panel: Texture,
    track: Texture,
    thumb: Texture,
}

impl Assets {
    pub fn load(gpu: &Gpu) -> Self {
        Self { panel: gpu.solid(PANEL_FILL), track: gpu.solid(TRACK), thumb: gpu.solid(THUMB) }
    }
}

/// The timer Next, a clicked Close and the menu's OK share.
#[derive(Default)]
pub struct Throttle(Option<Instant>);

impl Throttle {
    /// Whether a press goes through now; one that does starts the wait.
    pub fn pass(&mut self, now: Instant) -> bool {
        if self.0.is_some_and(|last| now.saturating_duration_since(last) < THROTTLE) {
            return false;
        }
        self.0 = Some(now);
        true
    }
}

/// Where the window is (dragged by anything but its button, links and scrollbar, and kept with the other windows),
/// how far its text is scrolled and how tall it was last drawn, and the throttle.
pub struct SayWindow {
    pub frame: Frame,
    pub scroll: f32,
    pub text_height: f32,
    pub throttle: Throttle,
}

impl Default for SayWindow {
    fn default() -> Self {
        let mut frame = Frame::new(DEFAULT_POSITION, SIZE[0]);
        frame.open = true;
        Self { frame, scroll: 0.0, text_height: 0.0, throttle: Throttle::default() }
    }
}

impl SayWindow {
    fn at(&self, local: [f32; 2]) -> [f32; 2] {
        self.frame.at(local)
    }

    pub fn contains(&self, cursor: [f32; 2]) -> bool {
        contains(self.frame.position, SIZE, cursor)
    }

    /// The button's box on screen.
    pub fn button_box(&self) -> [f32; 4] {
        let [x, y] = self.at(BUTTON_AT);
        [x, y, BUTTON[0], BUTTON[1]]
    }

    pub fn on_button(&self, cursor: [f32; 2]) -> bool {
        let [x, y, width, height] = self.button_box();
        contains([x, y], [width, height], cursor)
    }

    /// The scrollbar's box on screen, while there's more text than the panel shows.
    pub fn scrollbar_box(&self) -> Option<[f32; 4]> {
        let [x, y] = self.at([SCROLLBAR[0], SCROLLBAR[1]]);
        (scroll_range(self.text_height) > 0.0).then_some([x, y, SCROLLBAR[2], SCROLLBAR[3]])
    }

    /// Scrolls the text by `pixels` (down when positive), within its range.
    pub fn scroll_by(&mut self, pixels: f32) {
        self.scroll = (self.scroll + pixels).clamp(0.0, scroll_range(self.text_height));
    }

    /// A press on the scrollbar scrolls to that point of it.
    pub fn press_scrollbar(&mut self, cursor: [f32; 2]) {
        let top = self.at([SCROLLBAR[0], SCROLLBAR[1]])[1];
        self.scroll = scroll_range(self.text_height) * ((cursor[1] - top) / SCROLLBAR[3]).clamp(0.0, 1.0);
    }

    /// Where text line `row` goes on screen, lines `pitch` apart and `line_height` tall: only lines wholly inside
    /// the panel show.
    // ponytail: whole lines only, as the renderer has no clipping; a line half out of the panel is hidden.
    pub fn line_at(&self, row: usize, pitch: f32, line_height: f32) -> Option<[f32; 2]> {
        let top = TEXT_AT[1] + row as f32 * pitch - self.scroll;
        (top >= PANEL[1] && top + line_height <= PANEL[1] + PANEL[3]).then(|| self.at([TEXT_AT[0], top]))
    }

    /// The frame, the panel and the scrollbar; the text and the button are the caller's.
    pub fn draw<'a>(&self, layer: &mut Layer<'a>, chrome: &'a Chrome, frame: &'a pickup::Assets, assets: &'a Assets) {
        let [x, y] = self.frame.position;
        pickup::draw_sysbox(layer, chrome, frame, [x, y, SIZE[0], SIZE[1]]);
        layer.sprites.push((&assets.panel, self.at([PANEL[0], PANEL[1]]), [PANEL[2], PANEL[3]]));
        if let Some([x, y, width, height]) = self.scrollbar_box() {
            layer.sprites.push((&assets.track, [x, y], [width, height]));
            let thumb = (height * height / self.text_height).max(THUMB_MIN);
            let top = (height - thumb) * self.scroll / scroll_range(self.text_height);
            layer.sprites.push((&assets.thumb, [x, y + top], [width, thumb]));
        }
    }

    /// New text replacing the old: back to the top.
    pub fn restart(&mut self) {
        self.scroll = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrolls_only_as_far_as_the_text_goes() {
        // A page shorter than the panel: no scrolling, no scrollbar.
        let mut window = SayWindow { text_height: 120.0, ..SayWindow::default() };
        window.scroll_by(SCROLL_STEP);
        assert_eq!((window.scroll, window.scrollbar_box()), (0.0, None));
        // Ten lines of 20: 60 past the panel; the wheel's 30 a notch stops there.
        window.text_height = 200.0;
        window.scroll_by(SCROLL_STEP);
        window.scroll_by(SCROLL_STEP);
        window.scroll_by(SCROLL_STEP);
        assert_eq!(window.scroll, 60.0);
        window.scroll_by(-100.0);
        assert_eq!(window.scroll, 0.0);
        assert_eq!(window.scrollbar_box(), Some([200.0 + 257.0, 100.0 + 10.0, 13.0, 140.0]));
        // The button at (234, 156) of the window, 42×20.
        assert_eq!(window.button_box(), [434.0, 256.0, 42.0, 20.0]);
        assert!(window.on_button([440.0, 260.0]) && !window.on_button([430.0, 260.0]));
        // Lines 20 apart and 14 tall: rows 0 to 6 show from the panel's top; scrolled 30, row 1 is half out.
        window.scroll = 0.0;
        assert_eq!(window.line_at(0, 20.0, 14.0), Some([213.0, 110.0]));
        assert_eq!(window.line_at(6, 20.0, 14.0), Some([213.0, 230.0]));
        assert_eq!(window.line_at(7, 20.0, 14.0), None);
        window.scroll_by(SCROLL_STEP);
        assert_eq!((window.line_at(1, 20.0, 14.0), window.line_at(2, 20.0, 14.0)), (None, Some([213.0, 120.0])));
        // A press on the scrollbar's middle scrolls halfway.
        window.press_scrollbar([460.0, 180.0]);
        assert_eq!(window.scroll, 30.0);
    }

    #[test]
    fn lets_one_press_through_each_300_ms() {
        let start = Instant::now();
        let mut throttle = Throttle::default();
        assert!(throttle.pass(start));
        assert!(!throttle.pass(start + Duration::from_millis(299)));
        assert!(throttle.pass(start + Duration::from_millis(300)));
        assert!(!throttle.pass(start + Duration::from_millis(400)));
    }

    #[test]
    fn reads_the_servers_escapes() {
        assert_eq!(unescape(r#"one\ntwo \"three\""#), "one\ntwo \"three\"");
    }
}
