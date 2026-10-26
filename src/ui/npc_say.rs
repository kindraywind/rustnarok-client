//! An NPC's say window (ZC_SAY_DIALOG): 280×180 in the sys box frame with no title bar, its text on a grey panel
//! wrapped character by character at 215 px and scrolled only by the player, and the Next or Close button.
//! ZC_DIALOG_WINDOW_SIZE grows it (never under that size) and ZC_DIALOG_WINDOW_POS moves it.

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

/// The game's font tags in NPC text as the `^RRGGBB` codes the layout already reads: `<FONT COLOR=RRGGBB>` sets the
/// colour (any spacing round the `=`, six hex digits or the tag's colour is ignored), `</FONT>` goes back to the
/// enclosing tag's or black, and `<B>`, `<I>` and their closers (and a `SIZE`) are read but not drawn. Any other `<` is text.
// ponytail: bold, italic and size aren't drawn (the text renderer has one face); a `^RRGGBB` between tags isn't
// tracked, so `</FONT>` restores the enclosing tag's colour, as the game's own stack does.
pub fn font_tags(text: &str) -> String {
    fn code([red, green, blue]: [u8; 3]) -> String {
        format!("^{red:02X}{green:02X}{blue:02X}")
    }
    let mut out = String::with_capacity(text.len());
    let mut stack: Vec<[u8; 3]> = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find('<') {
        out.push_str(&rest[..open]);
        rest = &rest[open..];
        let Some(close) = rest.find('>') else { break };
        let body = rest[1..close].to_ascii_uppercase();
        let current = stack.last().copied().unwrap_or([0; 3]);
        if body.starts_with("FONT") {
            let color = body.find("COLOR").and_then(|at| hex_color(&body[at..]));
            if color.is_some() || body.contains("SIZE") {
                let color = color.unwrap_or(current);
                stack.push(color);
                out.push_str(&code(color));
            }
        } else if body == "/FONT" {
            if stack.pop().is_some() {
                out.push_str(&code(stack.last().copied().unwrap_or([0; 3])));
            }
        } else if !matches!(body.as_str(), "B" | "/B" | "I" | "/I") {
            out.push('<');
            rest = &rest[1..];
            continue;
        }
        rest = &rest[close + 1..];
    }
    out.push_str(rest);
    out
}

/// What follows the `=` in `COLOR = 2ECCFA`: the first six characters after any spaces, all hex digits.
fn hex_color(from: &str) -> Option<[u8; 3]> {
    let value: String = from.split_once('=')?.1.trim_start().chars().take_while(|character| !character.is_whitespace()).take(6).collect();
    let value = u32::from_str_radix(&value, 16).ok().filter(|_| value.len() == 6)?;
    Some([(value >> 16) as u8, (value >> 8) as u8, value as u8])
}

/// How far text this tall can scroll: past a panel this tall.
fn scroll_range(text_height: f32, panel_height: f32) -> f32 {
    (text_height - panel_height).max(0.0)
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

/// What a key pressed inside the throttle's wait presses once the wait is over.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Queued {
    Next,
    MenuOk,
}

/// The timer Next, a clicked Close and the menu's OK share, and a key press that came too soon for it.
#[derive(Default)]
pub struct Throttle {
    last: Option<Instant>,
    queued: Option<Queued>,
}

impl Throttle {
    /// Whether a press goes through now; one that does starts the wait.
    pub fn pass(&mut self, now: Instant) -> bool {
        if self.last.is_some_and(|last| now.saturating_duration_since(last) < THROTTLE) {
            return false;
        }
        self.last = Some(now);
        true
    }

    /// A key's press (Enter, Space): as `pass`, but one that comes too soon isn't lost: it waits for the end of the
    /// wait (`due`). Several too soon are one.
    pub fn press(&mut self, now: Instant, what: Queued) -> bool {
        let through = self.pass(now);
        if !through {
            self.queued = Some(what);
        }
        through
    }

    /// The key press waiting, if any.
    pub fn queued(&self) -> Option<Queued> {
        self.queued
    }

    /// The waiting key press, once the wait is over: it goes through then, once.
    pub fn due(&mut self, now: Instant) -> Option<Queued> {
        if self.queued.is_some() && self.pass(now) { self.queued.take() } else { None }
    }

    /// Drops the waiting key press (what it would press is gone).
    pub fn forget(&mut self) {
        self.queued = None;
    }
}

/// What Enter or Space does while an NPC dialog is open.
#[derive(Debug, PartialEq)]
pub enum DialogKey {
    /// The input box's OK.
    Submit,
    /// A space typed into the input box.
    TypeInput,
    /// A space typed into the chat line.
    TypeChat,
    /// The menu's OK on the highlighted item.
    MenuOk,
    /// Next or Close.
    Button,
}

/// Enter (or Space with `space`), with the input box open, a menu open, and text in an open chat line: the input box
/// takes both, Enter submitting. Space types into a chat line that has text in it; otherwise both confirm the menu,
/// or press Next or Close.
pub fn dialog_key(space: bool, input: bool, menu: bool, chat_text: bool) -> DialogKey {
    match (space, input, menu) {
        (false, true, _) => DialogKey::Submit,
        (true, true, _) => DialogKey::TypeInput,
        (true, false, _) if chat_text => DialogKey::TypeChat,
        (_, false, true) => DialogKey::MenuOk,
        (_, false, false) => DialogKey::Button,
    }
}

/// Where the window is (dragged by anything but its button, links and scrollbar, and kept with the other windows),
/// how far its text is scrolled and how tall it was last drawn, and the throttle.
pub struct SayWindow {
    pub frame: Frame,
    pub scroll: f32,
    pub text_height: f32,
    pub throttle: Throttle,
    size: [f32; 2],
}

impl Default for SayWindow {
    fn default() -> Self {
        let mut frame = Frame::new(DEFAULT_POSITION, SIZE[0]);
        frame.open = true;
        Self { frame, scroll: 0.0, text_height: 0.0, throttle: Throttle::default(), size: SIZE }
    }
}

impl SayWindow {
    pub fn size(&self) -> [f32; 2] {
        self.size
    }

    fn panel_height(&self) -> f32 {
        (self.size[1] - (SIZE[1] - PANEL[3])).max(40.0)
    }

    fn panel_width(&self) -> f32 {
        self.size[0] - (SIZE[0] - PANEL[2])
    }

    /// Where a line of text breaks: the default 215, wider as the window grows.
    pub fn wrap_width(&self) -> f32 {
        (self.size[0] - (SIZE[0] - WRAP_WIDTH)).max(40.0)
    }

    fn button_at(&self) -> [f32; 2] {
        [self.size[0] - (SIZE[0] - BUTTON_AT[0]), self.size[1] - (SIZE[1] - BUTTON_AT[1])]
    }

    fn scrollbar_rect(&self) -> [f32; 4] {
        [self.size[0] - (SIZE[0] - SCROLLBAR[0]), SCROLLBAR[1], SCROLLBAR[2], self.panel_height()]
    }

    fn range(&self) -> f32 {
        scroll_range(self.text_height, self.panel_height())
    }

    fn at(&self, local: [f32; 2]) -> [f32; 2] {
        self.frame.at(local)
    }

    pub fn contains(&self, cursor: [f32; 2]) -> bool {
        contains(self.frame.position, self.size, cursor)
    }

    /// The button's box on screen.
    pub fn button_box(&self) -> [f32; 4] {
        let [x, y] = self.at(self.button_at());
        [x, y, BUTTON[0], BUTTON[1]]
    }

    pub fn on_button(&self, cursor: [f32; 2]) -> bool {
        let [x, y, width, height] = self.button_box();
        contains([x, y], [width, height], cursor)
    }

    /// The scrollbar's box on screen, while there's more text than the panel shows.
    pub fn scrollbar_box(&self) -> Option<[f32; 4]> {
        let [x, y, width, height] = self.scrollbar_rect();
        let [x, y] = self.at([x, y]);
        (self.range() > 0.0).then_some([x, y, width, height])
    }

    /// Scrolls the text by `pixels` (down when positive), within its range.
    pub fn scroll_by(&mut self, pixels: f32) {
        self.scroll = (self.scroll + pixels).clamp(0.0, self.range());
    }

    /// A press on the scrollbar scrolls to that point of it.
    pub fn press_scrollbar(&mut self, cursor: [f32; 2]) {
        let track = self.scrollbar_rect();
        let top = self.at([track[0], track[1]])[1];
        self.scroll = self.range() * ((cursor[1] - top) / track[3]).clamp(0.0, 1.0);
    }

    /// Where text line `row` goes on screen, lines `pitch` apart and `line_height` tall: only lines wholly inside
    /// the panel show.
    // ponytail: whole lines only, as the renderer has no clipping; a line half out of the panel is hidden.
    pub fn line_at(&self, row: usize, pitch: f32, line_height: f32) -> Option<[f32; 2]> {
        let top = TEXT_AT[1] + row as f32 * pitch - self.scroll;
        (top >= PANEL[1] && top + line_height <= PANEL[1] + self.panel_height()).then(|| self.at([TEXT_AT[0], top]))
    }

    /// The frame, the panel and the scrollbar; the text and the button are the caller's.
    pub fn draw<'a>(&self, layer: &mut Layer<'a>, chrome: &'a Chrome, frame: &'a pickup::Assets, assets: &'a Assets) {
        let [x, y] = self.frame.position;
        pickup::draw_sysbox(layer, chrome, frame, [x, y, self.size[0], self.size[1]]);
        layer.sprites.push((&assets.panel, self.at([PANEL[0], PANEL[1]]), [self.panel_width(), self.panel_height()]));
        if let Some([x, y, width, height]) = self.scrollbar_box() {
            layer.sprites.push((&assets.track, [x, y], [width, height]));
            let thumb = (height * height / self.text_height).max(THUMB_MIN);
            let top = (height - thumb) * self.scroll / self.range();
            layer.sprites.push((&assets.thumb, [x, y + top], [width, thumb]));
        }
    }

    /// New text replacing the old: back to the top.
    pub fn restart(&mut self) {
        self.scroll = 0.0;
    }

    /// ZC_DIALOG_WINDOW_SIZE. A non-positive axis is the default, and the window never shrinks under 280×180.
    pub fn resize(&mut self, width: i32, height: i32) {
        let width = if width > 0 { width as f32 } else { SIZE[0] };
        let height = if height > 0 { height as f32 } else { SIZE[1] };
        self.size = [width.max(SIZE[0]), height.max(SIZE[1])];
        self.frame.set_width(self.size[0]);
        self.scroll_by(0.0);
    }

    /// ZC_DIALOG_WINDOW_POS, or the percent form. Kept on the screen.
    pub fn move_to(&mut self, x: i32, y: i32, percent: bool) {
        let screen = crate::screen();
        let [w, h] = self.size;
        let (x, y) = if percent {
            let share = |value: i32, span: f32, length: f32| (span - length).max(0.0) * (value.clamp(0, 100) as f32) / 100.0;
            (share(x, screen[0], w), share(y, screen[1], h))
        } else {
            (x as f32, y as f32)
        };
        self.frame.position = [x.clamp(0.0, (screen[0] - w).max(0.0)), y.clamp(0.0, (screen[1] - h).max(0.0))];
    }

    /// The talk ended: back to 280×180. Where the player left it stays.
    pub fn end_talk(&mut self) {
        self.throttle.forget();
        self.size = SIZE;
        self.frame.set_width(SIZE[0]);
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
    fn keeps_a_key_pressed_too_soon_for_the_end_of_the_wait() {
        let start = Instant::now();
        let at = |ms| start + Duration::from_millis(ms);
        let mut throttle = Throttle::default();
        assert!(throttle.press(start, Queued::Next));
        assert_eq!(throttle.due(at(50)), None);
        // Two presses too soon: one waits, and goes through once the 300 ms are over.
        assert!(!throttle.press(at(100), Queued::Next));
        assert!(!throttle.press(at(150), Queued::Next));
        assert_eq!(throttle.queued(), Some(Queued::Next));
        assert_eq!(throttle.due(at(299)), None);
        assert_eq!(throttle.due(at(300)), Some(Queued::Next));
        assert_eq!(throttle.due(at(700)), None);
        // It started a wait of its own.
        assert!(!throttle.pass(at(599)));
        // A click too soon is dropped, as before.
        assert!(!throttle.pass(at(450)));
        assert_eq!(throttle.due(at(1000)), None);
        // The talk ending drops a waiting press.
        assert!(throttle.press(at(1000), Queued::MenuOk));
        assert!(!throttle.press(at(1100), Queued::MenuOk));
        throttle.forget();
        assert_eq!(throttle.due(at(2000)), None);
    }

    #[test]
    fn space_and_enter_in_a_dialog() {
        // Space with an empty chat line: Next or Close, and the menu's OK, as Enter.
        assert_eq!(dialog_key(true, false, false, false), DialogKey::Button);
        assert_eq!(dialog_key(false, false, false, false), DialogKey::Button);
        assert_eq!(dialog_key(true, false, true, false), DialogKey::MenuOk);
        assert_eq!(dialog_key(false, false, true, false), DialogKey::MenuOk);
        // With text in the chat line Space types there; Enter still answers the dialog.
        assert_eq!(dialog_key(true, false, false, true), DialogKey::TypeChat);
        assert_eq!(dialog_key(true, false, true, true), DialogKey::TypeChat);
        assert_eq!(dialog_key(false, false, true, true), DialogKey::MenuOk);
        // The input box takes Space as text and Enter as OK.
        assert_eq!(dialog_key(true, true, false, false), DialogKey::TypeInput);
        assert_eq!(dialog_key(true, true, false, true), DialogKey::TypeInput);
        assert_eq!(dialog_key(false, true, false, false), DialogKey::Submit);
    }

    #[test]
    fn the_buttons_target_is_where_a_click_presses_it() {
        let mut window = SayWindow::default();
        for size in [(0, 0), (400, 240)] {
            window.resize(size.0, size.1);
            let [x, y, width, height] = window.button_box();
            assert!(window.on_button([x + width / 2.0, y + height / 2.0]));
        }
    }

    #[test]
    fn grows_from_the_default_and_moves() {
        let mut window = SayWindow::default();
        window.resize(400, 240);
        assert_eq!(window.size(), [400.0, 240.0]);
        assert!(window.wrap_width() > WRAP_WIDTH);
        // The button stays 4 px in from the right and the bottom.
        assert_eq!(window.button_box(), [window.frame.position[0] + 400.0 - 46.0, window.frame.position[1] + 240.0 - 24.0, 42.0, 20.0]);
        // A 0 axis is the default, and nothing shrinks under 280×180.
        window.resize(0, 10);
        assert_eq!(window.size(), SIZE);
        window.move_to(12, 34, false);
        assert_eq!(window.frame.position, [12.0, 34.0]);
        window.move_to(100, 0, true);
        let screen = crate::screen();
        assert_eq!(window.frame.position, [screen[0] - SIZE[0], 0.0]);
        window.end_talk();
        assert_eq!((window.size(), window.frame.position), (SIZE, [screen[0] - SIZE[0], 0.0]));
    }

    #[test]
    fn reads_the_servers_escapes() {
        assert_eq!(unescape(r#"one\ntwo \"three\""#), "one\ntwo \"three\"");
    }

    #[test]
    fn turns_font_tags_into_colour_codes() {
        // A vending machine's line, as a script writes it (spaces round the `=`).
        assert_eq!(
            font_tags("<font color = 2ECCFA>Clean</font> through your <font color = DF0101>cramped</font> stomach with <i>refreshing</i> <b>Soda</b>!"),
            "^2ECCFAClean^000000 through your ^DF0101cramped^000000 stomach with refreshing Soda!"
        );
        // Nested tags go back to the enclosing one's colour; a closer with nothing open is dropped.
        assert_eq!(font_tags("<FONT COLOR=FF0000>a<FONT COLOR=00ff00>b</FONT>c</FONT></FONT>d"), "^FF0000a^00FF00b^FF0000c^000000d");
        // A size alone keeps the colour; a bad colour is ignored; other angle brackets stay text.
        assert_eq!(font_tags("<FONT SIZE=14>x</FONT> <font color=red>y</font> a < b <ITEM>i</ITEM> c > d"), "^000000x^000000 y a < b <ITEM>i</ITEM> c > d");
    }
}
