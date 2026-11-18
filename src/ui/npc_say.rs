//! An NPC's say window (ZC_SAY_DIALOG): 280×180 in the sys box frame with no title bar, its text on a grey panel
//! wrapped character by character at 215 px and scrolled only by the player, and the Next or Close button.
//! ZC_DIALOG_WINDOW_SIZE grows it (never under that size) and ZC_DIALOG_WINDOW_POS moves it.

use std::time::{Duration, Instant};

use crate::render::{Gpu, Texture};
use crate::window::{BarHit, Chrome, Frame, ScrollBar, Thumb};
use crate::{Layer, contains, links, pickup};

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
/// An item's picture in the text (`^i[id]`): a box this wide and tall on the line, the item's icon at its top left.
pub const PICTURE: f32 = 25.0;
/// A pointed picture's item name shows this far from the picture's corner.
pub const PICTURE_NAME_AT: [f32; 2] = [0.0, -17.0];
/// The scroll bar at the panel's right edge, as tall as the panel, while the text is taller than the panel.
pub const SCROLLBAR: [f32; 2] = [257.0, 10.0];
/// The wheel, Up/Down, the bar's arrows and a press on its track scroll this much.
pub const SCROLL_STEP: f32 = 30.0;
/// Next and Close.
pub const BUTTON_AT: [f32; 2] = [234.0, 156.0];
pub const BUTTON: [f32; 2] = [42.0, 20.0];
/// Next, a clicked Close and the menu's OK are each ignored until this long after the last of them went through.
pub const THROTTLE: Duration = Duration::from_millis(300);

/// The server's text: the two-character `\r`, `\n` and `\"` as CR, LF and `"`.
pub fn unescape(text: &str) -> String {
    text.replace("\\r", "\r").replace("\\n", "\n").replace("\\\"", "\"")
}

/// A `^i` code at the start of `text`: how many bytes it takes (the `^i` and every `[1 to 7 digits]` group right after
/// it) and the item its picture shows, the first group's. Without a group the two characters are still taken, and
/// nothing is drawn for them.
pub fn picture_code(text: &str) -> Option<(usize, Option<u32>)> {
    let mut rest = text.strip_prefix("^i")?;
    let mut id = None;
    while let Some(group) = rest.strip_prefix('[') {
        let digits = group.bytes().take_while(u8::is_ascii_digit).count();
        if !(1..=7).contains(&digits) || group.as_bytes().get(digits) != Some(&b']') {
            break;
        }
        id = id.or(group[..digits].parse().ok());
        rest = &group[digits + 1..];
    }
    Some((text.len() - rest.len(), id))
}

/// NPC text with its `^i[id]` codes (rathena's `mesitemicon`) made into picture runs for the layout.
pub fn item_pictures(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some((at, (taken, id))) = rest.find("^i").and_then(|at| Some((at, picture_code(&rest[at..])?))) {
        out.push_str(&rest[..at]);
        if let Some(id) = id {
            out.push_str(&links::marked("", &links::Target::Picture(id)));
        }
        rest = &rest[at + taken..];
    }
    out.push_str(rest);
    out
}

/// The items whose pictures the text shows.
pub fn pictures(lines: &[String]) -> impl Iterator<Item = u32> + '_ {
    lines.iter().flat_map(|line| links::pieces(line)).filter_map(|(_, target)| match target {
        Some(links::Target::Picture(id)) => Some(id),
        _ => None,
    })
}

/// How tall a line is: its text, or a picture on it when that is taller. Text and pictures sit on the line's bottom.
pub fn line_height(text: f32, pieces: &[([u8; 3], String, Option<links::Target>)]) -> f32 {
    let picture = pieces.iter().any(|(_, _, target)| matches!(target, Some(links::Target::Picture(_))));
    if picture { text.max(PICTURE) } else { text }
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

/// One say packet's text as its lines (split at LF, CRs dropped): a `^RRGGBB` colour holds to the end of the packet,
/// so each line after the first starts with the colour in force where the one before ended.
pub fn packet_lines(text: &str) -> Vec<String> {
    let mut color = String::new();
    text.split('\n')
        .map(|line| {
            let line = format!("{color}{}", line.replace('\r', ""));
            let is_code = |code: &&str| code.len() >= 7 && code.as_bytes()[1..7].iter().all(u8::is_ascii_hexdigit);
            if let Some(code) = line.match_indices('^').map(|(at, _)| &line[at..]).rfind(is_code) {
                color = code[..7].to_owned();
            }
            line
        })
        .collect()
}

/// `^nItemID^<id>` in NPC text (say lines and menu items) as the item's `name`; without digits it stays as it is.
pub fn item_names(text: &str, name: impl Fn(u32) -> String) -> String {
    const CODE: &str = "^nItemID^";
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(CODE) {
        let after = &rest[at + CODE.len()..];
        let digits = after.bytes().take_while(u8::is_ascii_digit).count();
        out.push_str(&rest[..at]);
        match after[..digits].parse() {
            Ok(id) => out.push_str(&name(id)),
            Err(_) => out.push_str(CODE),
        }
        rest = &after[digits..];
    }
    out.push_str(rest);
    out
}

/// The part of `area` inside `to` (both x, y, width, height), if any.
pub fn cut(area: [f32; 4], to: [f32; 4]) -> Option<[f32; 4]> {
    let [left, top] = [area[0].max(to[0]), area[1].max(to[1])];
    let [right, bottom] = [(area[0] + area[2]).min(to[0] + to[2]), (area[1] + area[3]).min(to[1] + to[3])];
    (right > left && bottom > top).then_some([left, top, right - left, bottom - top])
}

/// How far text this tall can scroll: past a panel this tall.
fn scroll_range(text_height: f32, panel_height: f32) -> f32 {
    (text_height - panel_height).max(0.0)
}

/// The panel's fill (the frame is the system box's, `pickup::Assets`).
pub struct Assets {
    panel: Texture,
}

impl Assets {
    pub fn load(gpu: &Gpu) -> Self {
        Self { panel: gpu.solid(PANEL_FILL) }
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
/// how far its text is scrolled and how tall it was last drawn, the scroll bar's held thumb, the lines last drawn
/// (each one's text and its box cut to the panel), and the throttle.
pub struct SayWindow {
    pub frame: Frame,
    pub scroll: f32,
    pub text_height: f32,
    pub thumb: Thumb,
    pub drawn: Vec<(String, [f32; 4])>,
    pub throttle: Throttle,
    size: [f32; 2],
}

impl Default for SayWindow {
    fn default() -> Self {
        let mut frame = Frame::new(DEFAULT_POSITION, SIZE[0]);
        frame.open = true;
        Self { frame, scroll: 0.0, text_height: 0.0, thumb: Thumb::default(), drawn: Vec::new(), throttle: Throttle::default(), size: SIZE }
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

    /// Where a line of text breaks: 215 px, whatever size a script gives the window.
    pub fn wrap_width(&self) -> f32 {
        WRAP_WIDTH
    }

    fn button_at(&self) -> [f32; 2] {
        [self.size[0] - (SIZE[0] - BUTTON_AT[0]), self.size[1] - (SIZE[1] - BUTTON_AT[1])]
    }

    /// The scroll bar, in pixels of text: it shows the panel's height of the text's, from the scroll offset.
    fn bar(&self) -> ScrollBar {
        let at = self.at([self.size[0] - (SIZE[0] - SCROLLBAR[0]), SCROLLBAR[1]]);
        let height = self.panel_height();
        ScrollBar { at, height, shown: height as usize, rows: self.text_height.ceil() as usize, first: self.scroll as usize }
    }

    /// The text panel on screen: what the text is cut to.
    pub fn panel_box(&self) -> [f32; 4] {
        let [x, y] = self.at([PANEL[0], PANEL[1]]);
        [x, y, self.panel_width(), self.panel_height()]
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

    /// The scroll bar's up and down arrows and its thumb on screen, while there's more text than the panel shows.
    pub fn bar_boxes(&self) -> Option<[[f32; 4]; 3]> {
        self.bar().boxes()
    }

    /// Scrolls the text by `pixels` (down when positive), within its range.
    pub fn scroll_by(&mut self, pixels: f32) {
        self.scroll = (self.scroll + pixels).clamp(0.0, self.range());
    }

    /// A press on the scroll bar, whether it was on it: an arrow or the track either side of the thumb scrolls
    /// SCROLL_STEP, the thumb is held (`drag_thumb`). Holding repeats nothing.
    pub fn press_scrollbar(&mut self, cursor: [f32; 2]) -> bool {
        let bar = self.bar();
        match bar.hit(cursor) {
            None => return false,
            Some(BarHit::Up | BarHit::PageUp) => self.scroll_by(-SCROLL_STEP),
            Some(BarHit::Down | BarHit::PageDown) => self.scroll_by(SCROLL_STEP),
            Some(BarHit::Thumb) => {
                bar.press(cursor, &mut self.thumb);
            }
        }
        true
    }

    /// The pointer at height `y` with the thumb held: the offset at the press plus the pointer's way over the track,
    /// the track standing for one pixel more than the range.
    pub fn drag_thumb(&mut self, y: f32) {
        if let Some(first) = self.thumb.first(y) {
            self.scroll = first as f32;
        }
    }

    /// Where a text line goes on screen, `above` under the text's top and `line_height` tall, while any of it is
    /// inside the panel (the drawing is cut to `panel_box`).
    pub fn line_at(&self, above: f32, line_height: f32) -> Option<[f32; 2]> {
        let top = TEXT_AT[1] + above - self.scroll;
        (top < PANEL[1] + self.panel_height() && top + line_height > PANEL[1]).then(|| self.at([TEXT_AT[0], top]))
    }

    /// The frame, the panel and the scrollbar; the text and the button are the caller's.
    pub fn draw<'a>(&self, layer: &mut Layer<'a>, chrome: &'a Chrome, frame: &'a pickup::Assets, assets: &'a Assets) {
        let [x, y] = self.frame.position;
        pickup::draw_sysbox(layer, chrome, frame, [x, y, self.size[0], self.size[1]]);
        layer.sprites.push((&assets.panel, self.at([PANEL[0], PANEL[1]]), [self.panel_width(), self.panel_height()]));
        self.bar().draw(layer, chrome);
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
        assert_eq!((window.scroll, window.bar_boxes()), (0.0, None));
        // Ten lines of 20: 60 past the panel; the wheel's 30 a notch stops there.
        window.text_height = 200.0;
        window.scroll_by(SCROLL_STEP);
        window.scroll_by(SCROLL_STEP);
        window.scroll_by(SCROLL_STEP);
        assert_eq!(window.scroll, 60.0);
        window.scroll_by(-100.0);
        assert_eq!(window.scroll, 0.0);
        // The bar at (257, 10) of the window, 13 wide and as tall as the panel: arrows 13 tall at its ends.
        let [up, down, _] = window.bar_boxes().unwrap();
        assert_eq!((up, down), ([457.0, 110.0, 13.0, 13.0], [457.0, 237.0, 13.0, 13.0]));
        // The button at (234, 156) of the window, 42×20.
        assert_eq!(window.button_box(), [434.0, 256.0, 42.0, 20.0]);
        assert!(window.on_button([440.0, 260.0]) && !window.on_button([430.0, 260.0]));
        // Lines 20 apart and 14 tall: rows 0 to 6 show from the panel's top, the 8th not at all.
        window.scroll = 0.0;
        assert_eq!(window.line_at(0.0, 14.0), Some([213.0, 110.0]));
        assert_eq!(window.line_at(120.0, 14.0), Some([213.0, 230.0]));
        assert_eq!(window.line_at(140.0, 14.0), None);
        // Scrolled 30: row 1 is half out at the top and still drawn, cut to the panel.
        window.scroll_by(SCROLL_STEP);
        assert_eq!((window.line_at(20.0, 14.0), window.line_at(0.0, 14.0)), (Some([213.0, 100.0]), None));
        assert_eq!(cut([213.0, 100.0, 50.0, 14.0], window.panel_box()), Some([213.0, 110.0, 50.0, 4.0]));
        assert_eq!(cut([213.0, 80.0, 50.0, 14.0], window.panel_box()), None);
    }

    #[test]
    fn the_bars_arrows_track_and_thumb_scroll_the_text() {
        // 200 px of text in the 140 px panel: 60 to scroll.
        let mut window = SayWindow { text_height: 200.0, ..SayWindow::default() };
        let [down, up] = [[460.0, 245.0], [460.0, 115.0]];
        assert!(window.press_scrollbar(down));
        assert_eq!(window.scroll, 30.0);
        window.press_scrollbar(down);
        window.press_scrollbar(down);
        assert_eq!(window.scroll, 60.0);
        window.press_scrollbar(up);
        assert_eq!(window.scroll, 30.0);
        // The track over the thumb, then under it: 30 each.
        window.scroll = 30.0;
        let [_, _, thumb] = window.bar_boxes().unwrap();
        window.press_scrollbar([460.0, thumb[1] - 1.0]);
        assert_eq!(window.scroll, 0.0);
        let [_, _, thumb] = window.bar_boxes().unwrap();
        window.press_scrollbar([460.0, thumb[1] + thumb[3] + 1.0]);
        assert_eq!(window.scroll, 30.0);
        // The thumb, held: the offset at the press plus trunc(way × 61 / 114) (the track is 140 − 26), kept in range.
        let [_, _, thumb] = window.bar_boxes().unwrap();
        let grab = thumb[1] + 2.0;
        assert!(window.press_scrollbar([460.0, grab]));
        window.drag_thumb(grab + 10.0);
        assert_eq!(window.scroll, 30.0 + (10.0f32 * 61.0 / 114.0).trunc());
        window.drag_thumb(grab + 500.0);
        assert_eq!(window.scroll, 60.0);
        window.drag_thumb(grab - 500.0);
        assert_eq!(window.scroll, 0.0);
        assert!(window.thumb.stop());
        // Off the bar: not taken.
        assert!(!window.press_scrollbar([300.0, 150.0]));
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
        // The text still breaks at 215 px.
        assert_eq!(window.wrap_width(), WRAP_WIDTH);
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
    fn a_colour_holds_to_the_end_of_its_packet() {
        assert_eq!(
            packet_lines("^FF0000red\r\nstill red\n^0000FFblue ^00FF00green\nstill green"),
            ["^FF0000red", "^FF0000still red", "^FF0000^0000FFblue ^00FF00green", "^00FF00still green"]
        );
        // A packet starts black: the next one's lines carry nothing over.
        assert_eq!(packet_lines("black\nstill black"), ["black", "still black"]);
        // Not a colour: `^nItemID^`, too few hex digits.
        assert_eq!(packet_lines("^nItemID^501 ^12AB\nx"), ["^nItemID^501 ^12AB", "x"]);
    }

    #[test]
    fn names_items_by_their_code() {
        let name = |id: u32| if id == 501 { "Red Potion".to_owned() } else { format!("item {id}") };
        assert_eq!(item_names("Bring ^nItemID^501 and ^nItemID^909.", name), "Bring Red Potion and item 909.");
        // Without digits it stays.
        assert_eq!(item_names("^nItemID^ and ^nItemID^x", name), "^nItemID^ and ^nItemID^x");
    }

    #[test]
    fn reads_the_servers_escapes() {
        assert_eq!(unescape(r#"one\ntwo \"three\""#), "one\ntwo \"three\"");
    }

    #[test]
    fn reads_item_picture_codes() {
        assert_eq!(picture_code("^i[501] - Red Potion"), Some((7, Some(501))));
        assert_eq!(picture_code("^i[1234567]"), Some((11, Some(1234567))));
        // Every group right after it is taken; the picture is the first's.
        assert_eq!(picture_code("^i[501][502] x"), Some((12, Some(501))));
        // No group, more than 7 digits, no digits or no closing bracket: the `^i` alone is taken, and draws nothing.
        for text in ["^i", "^i 501", "^i[12345678]", "^i[]", "^i[50a]", "^i[501"] {
            assert_eq!(picture_code(text), Some((2, None)), "{text}");
        }
        // Not the code: a colour, another letter, a capital.
        for text in ["^0000FF", "^e[1]", "^I[501]", "i[501]"] {
            assert_eq!(picture_code(text), None, "{text}");
        }
    }

    #[test]
    fn makes_item_picture_codes_into_picture_runs() {
        let apple = links::Target::Item(links::ItemLink { id: 512, ..links::ItemLink::default() });
        let line = format!("^i[512] - {}^i[7]^i", links::marked("Apple", &apple));
        let pieces = links::pieces(&item_pictures(&line));
        let picture = |id| (String::new(), Some(links::Target::Picture(id)));
        assert_eq!(pieces, [picture(512), (" - ".into(), None), ("Apple".into(), Some(apple)), picture(7)]);
        assert_eq!(pictures(&[item_pictures("a ^i[501] b"), "c".into(), item_pictures("^i[99999999]^i[502]")]).collect::<Vec<_>>(), [501, 502]);
        // Text without the code, colour codes included, is left as it is.
        assert_eq!(item_pictures("^FF0000red^000000 ^e[1] i[501]"), "^FF0000red^000000 ^e[1] i[501]");
    }

    #[test]
    fn a_line_with_a_picture_is_as_tall_as_the_picture() {
        let text = ([0; 3], "Red Potion".to_owned(), None);
        let picture = ([0; 3], String::new(), Some(links::Target::Picture(501)));
        assert_eq!(line_height(15.0, std::slice::from_ref(&text)), 15.0);
        assert_eq!(line_height(30.0, std::slice::from_ref(&picture)), 30.0);
        assert_eq!(line_height(15.0, &[picture, text]), PICTURE);
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
