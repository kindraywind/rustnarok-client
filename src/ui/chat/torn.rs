//! A tab torn off the chat window: its log in a window of its own, with buttons to close it, put it back and pick
//! what it shows, a grip to resize it, and its name on a tab at its top left.

use rustnarok_client::text::Font;

use super::{ARROW, Assets, Hit, LogBox, Tab};
use crate::links::{ItemLink, Target};
use crate::{Layer, contains};

/// A new one's size; the n-th (from 0) opens at (100 + 10n, 100 + 20n).
pub const SIZE: [f32; 2] = [300.0, 95.0];
const FIRST_AT: [f32; 2] = [100.0, 100.0];
const STAGGER: [f32; 2] = [10.0, 20.0];
/// The grip keeps it between these sizes, in 32-px steps from the least.
const LEAST: [f32; 2] = [280.0, 74.0];
const MOST: [f32; 2] = [512.0, 384.0];
const SIZE_STEP: f32 = 32.0;
/// The name's tab: its label's width and 10, at least 25; 16 tall.
const TITLE_PADDING: f32 = 10.0;
const TITLE_LEAST: f32 = 25.0;
const TITLE_HEIGHT: f32 = 16.0;
/// The log: from (5, 20), the window's width less 8 and its height less 35.
const LOG_AT: [f32; 2] = [5.0, 20.0];
const LOG_LESS: [f32; 2] = [8.0, 35.0];
/// The buttons' row and their sizes: close (and remove), put back, tab settings. The grip, at the bottom right.
const BUTTON_Y: f32 = 4.0;
const CLOSE: [f32; 2] = [8.0, 8.0];
const ATTACH: [f32; 2] = [14.0, 9.0];
const OPTIONS: [f32; 2] = [9.0, 9.0];
const GRIP: f32 = 13.0;

pub struct Torn {
    pub tab: Tab,
    /// Its top left on screen, and its size.
    pub position: [f32; 2],
    pub size: [f32; 2],
    /// The name's width and its tab's, as last measured.
    name_width: f32,
    title_width: f32,
    /// The pointer's offset while it's dragged, and when it was grabbed or let go (it's half see-through held).
    drag: Option<[f32; 2]>,
    /// Whether the held window moved: a press let go where it was is still a click (on its name, a double click).
    moved: bool,
    held: Option<std::time::Instant>,
    let_go: Option<(std::time::Instant, f32)>,
    /// While the grip is held: the pointer and the size when it started.
    grip: Option<([f32; 2], [f32; 2])>,
    /// While the thumb is held: the pointer's height and the scroll when it started.
    thumb_drag: Option<(f32, usize)>,
}

impl Torn {
    /// Tab `tab` as the `nth` torn-off window (from 0), at its place, or at `rect` as it was kept.
    pub fn new(tab: Tab, nth: usize, rect: Option<[f32; 4]>) -> Self {
        let [x, y, w, h] = rect.unwrap_or([FIRST_AT[0] + STAGGER[0] * nth as f32, FIRST_AT[1] + STAGGER[1] * nth as f32, SIZE[0], SIZE[1]]);
        Self {
            tab,
            position: [x, y],
            size: [w, h],
            name_width: 0.0,
            title_width: TITLE_LEAST,
            drag: None,
            moved: false,
            held: None,
            let_go: None,
            grip: None,
            thumb_drag: None,
        }
    }

    pub fn rect(&self) -> [f32; 4] {
        [self.position[0], self.position[1], self.size[0], self.size[1]]
    }

    fn at(&self, local: [f32; 2]) -> [f32; 2] {
        [self.position[0] + local[0], self.position[1] + local[1]]
    }

    pub fn contains(&self, cursor: [f32; 2]) -> bool {
        contains(self.position, self.size, cursor)
    }

    pub fn log_box(&self) -> LogBox {
        LogBox { corner: self.at(LOG_AT), size: [self.size[0] - LOG_LESS[0], self.size[1] - LOG_LESS[1]] }
    }

    /// Measures the name's tab: the label's width and 10, at least 25.
    pub fn measure(&mut self, messages: &[String], measure: &dyn Fn(&str) -> f32) {
        self.name_width = measure(self.tab.label.text(messages));
        self.title_width = (self.name_width + TITLE_PADDING).max(TITLE_LEAST);
    }

    pub fn name_width(&self) -> f32 {
        self.name_width
    }

    /// The buttons, right to left at y 4: unlocked close, put back and settings; locked only settings.
    fn buttons(&self, locked: bool) -> Vec<(Hit, [f32; 2], [f32; 2])> {
        let w = self.size[0];
        if locked {
            vec![(Hit::Options, [w - 11.0, BUTTON_Y], OPTIONS)]
        } else {
            vec![(Hit::RemoveTab, [w - 10.0, BUTTON_Y], CLOSE), (Hit::Attach, [w - 26.0, BUTTON_Y], ATTACH), (Hit::Options, [w - 37.0, BUTTON_Y], OPTIONS)]
        }
    }

    fn grip_at(&self) -> [f32; 2] {
        [self.size[0] - 15.0, self.size[1] - 15.0]
    }

    /// What `cursor` is on: a button, the grip (unlocked), the log or its scrollbar, the name's tab (`Hit::Title`), or
    /// the rest of the window (`Hit::Strip`).
    pub fn hit(&self, cursor: [f32; 2], locked: bool) -> Option<Hit> {
        if !self.contains(cursor) {
            return None;
        }
        let local = [cursor[0] - self.position[0], cursor[1] - self.position[1]];
        if let Some((hit, ..)) = self.buttons(locked).into_iter().find(|&(_, at, size)| contains(at, size, local)) {
            return Some(hit);
        }
        if !locked && contains(self.grip_at(), [GRIP; 2], local) {
            return Some(Hit::Resize);
        }
        if let Some(hit) = self.tab.log_hit(self.log_box(), cursor) {
            return Some(hit);
        }
        Some(if local[0] > 0.0 && local[0] < self.title_width && local[1] > 0.0 && local[1] < TITLE_HEIGHT { Hit::Title } else { Hit::Strip })
    }

    /// A press: unlocked, the bare window and the name's tab grab it (half see-through while held) and the grip
    /// resizes it; locked, they do nothing. The thumb drags the log either way.
    pub fn press(&mut self, cursor: [f32; 2], locked: bool) {
        match self.hit(cursor, locked) {
            Some(Hit::Strip | Hit::Title) if !locked => self.grab(cursor),
            Some(Hit::Resize) => self.grip = Some((cursor, self.size)),
            Some(Hit::Thumb) => self.thumb_drag = Some((cursor[1], self.tab.scroll)),
            _ => {}
        }
    }

    /// Held from `cursor`: it follows the pointer from here.
    pub fn grab(&mut self, cursor: [f32; 2]) {
        self.drag = Some([cursor[0] - self.position[0], cursor[1] - self.position[1]]);
        self.held = Some(crate::control::now());
        self.let_go = None;
    }

    pub fn dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// The pointer moved: held, it follows (edges snapping to the screen's); by the grip it takes the size the pointer
    /// asks, kept between 280×74 and 512×384 in 32-px steps; by the thumb the log scrolls.
    pub fn drag_to(&mut self, cursor: [f32; 2]) {
        if let Some(grab) = self.drag {
            let position = crate::window::dragged_to([cursor[0] - grab[0], cursor[1] - grab[1]], self.size);
            self.moved |= position != self.position;
            self.position = position;
        }
        if let Some((from, size)) = self.grip {
            self.size = [0, 1].map(|axis| sized(size[axis] + cursor[axis] - from[axis], LEAST[axis], MOST[axis]));
        }
        if let Some(held) = self.thumb_drag {
            let log = self.log_box();
            self.tab.drag_thumb(log, held, cursor[1]);
        }
    }

    /// Let go; whether it moved, or was resized or scrolled by the thumb.
    pub fn drop_drag(&mut self) -> bool {
        if let Some(held) = self.held.take() {
            let now = crate::control::now();
            self.let_go = Some((now, crate::window::hold_fade(Some(held), None, now)));
        }
        self.drag = None;
        std::mem::take(&mut self.moved) | self.grip.take().is_some() | self.thumb_drag.take().is_some()
    }

    /// How see-through to draw it now: half while it's held.
    pub fn faded(&self) -> f32 {
        crate::window::hold_fade(self.held, self.let_go, crate::control::now())
    }

    /// Its bottom left corner, where dropping it onto the chat window puts it back.
    pub fn bottom_left(&self) -> [f32; 2] {
        [self.position[0], self.position[1] + self.size[1]]
    }

    /// The buttons and the grip on screen, for the test driver: "torn_<n>_close", "_attach", "_options", "_grip",
    /// "_title" and the scrollbar's.
    pub fn button_boxes(&self, nth: usize, locked: bool) -> Vec<(String, [f32; 4])> {
        let boxed = |local: [f32; 2], size: [f32; 2]| {
            let [x, y] = self.at(local);
            [x, y, size[0], size[1]]
        };
        let name = |hit: Hit| match hit {
            Hit::RemoveTab => "close",
            Hit::Attach => "attach",
            _ => "options",
        };
        let mut boxes: Vec<_> = self.buttons(locked).into_iter().map(|(hit, at, size)| (format!("torn_{nth}_{}", name(hit)), boxed(at, size))).collect();
        boxes.push((format!("torn_{nth}_title"), boxed([1.0, 1.0], [self.title_width - 2.0, TITLE_HEIGHT - 2.0])));
        if !locked {
            boxes.push((format!("torn_{nth}_grip"), boxed(self.grip_at(), [GRIP; 2])));
        }
        if let Some([up, down, thumb]) = self.tab.scrollbar_boxes(self.log_box()) {
            boxes.extend([(format!("torn_{nth}_scroll_up"), up), (format!("torn_{nth}_scroll_down"), down), (format!("torn_{nth}_thumb"), thumb)]);
        }
        boxes
    }

    /// The window: black at 40 % behind the name's tab and the body, a folder's outline (white top and left, grey
    /// right and bottom), the name, the log, the buttons and the grip.
    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &self,
        layer: &mut Layer<'a>,
        assets: &'a Assets,
        font: &Font,
        messages: &[String],
        locked: bool,
        name_of: &dyn Fn(&ItemLink) -> String,
        link_boxes: &mut Vec<([f32; 4], Target)>,
    ) {
        let [w, h] = self.size;
        let tab = self.title_width;
        let line = |layer: &mut Layer<'a>, texture: &'a crate::render::Texture, at: [f32; 2], size: [f32; 2]| layer.sprites.push((texture, self.at(at), size));
        line(layer, &assets.dark, [1.0, 1.0], [tab - 2.0, TITLE_HEIGHT]);
        line(layer, &assets.dark, [1.0, 17.0], [w - 2.0, h - 18.0]);
        // White: the name's tab's top, its corner, the left edge, the body's top right of the tab.
        let white = &assets.white;
        line(layer, white, [2.0, 0.0], [tab - 6.0, 1.0]);
        line(layer, white, [1.0, 1.0], [1.0, 1.0]);
        line(layer, white, [0.0, 2.0], [1.0, h - 4.0]);
        line(layer, white, [tab, 16.0], [w - 4.0 - tab, 1.0]);
        // Grey: the tab's right corner and edge, the body's right corner and edge, the bottom.
        let grey = &assets.frame_line;
        for (at, size) in [
            ([tab - 3.0, 0.0], [1.0, 1.0]),
            ([tab - 2.0, 1.0], [1.0, 1.0]),
            ([tab - 1.0, 2.0], [1.0, 15.0]),
            ([w - 3.0, 16.0], [1.0, 1.0]),
            ([w - 2.0, 17.0], [1.0, 1.0]),
            ([w - 1.0, 18.0], [1.0, h - 20.0]),
            ([1.0, h - 2.0], [1.0, 1.0]),
            ([w - 2.0, h - 2.0], [1.0, 1.0]),
            ([2.0, h - 1.0], [w - 4.0, 1.0]),
        ] {
            line(layer, grey, at, size);
        }
        let name = self.tab.label.text(messages);
        layer.texts.push((name.to_owned(), self.at([5.0, 4.0]), [0, 0, 0]));
        layer.texts.push((name.to_owned(), self.at([4.0, 3.0]), [255; 3]));
        self.tab.draw(layer, assets, font, self.log_box(), name_of, link_boxes);
        for (hit, at, _) in self.buttons(locked) {
            let image = match hit {
                Hit::RemoveTab => &assets.remove_tab[0],
                Hit::Attach => &assets.attach[0],
                _ => &assets.tab_settings[0],
            };
            layer.sprites.push((image, self.at(at), image.size));
        }
        if !locked {
            layer.sprites.push((&assets.grip, self.at(self.grip_at()), assets.grip.size));
        }
    }

    /// A rename's box over the name's tab: (tab − 10) × 14 at (5, 2).
    pub fn rename_box(&self) -> [f32; 4] {
        let [x, y] = self.at([5.0, 2.0]);
        [x, y, (self.title_width - TITLE_PADDING).max(ARROW), 14.0]
    }

    /// Whether a name this wide fits: its width and 10 under the window's less 50.
    pub fn fits(&self, width: f32) -> bool {
        width + TITLE_PADDING < self.size[0] - 50.0
    }

    /// Its log's rows, oldest first (lines not yet wrapped at the end).
    pub fn tab_lines(&self) -> Vec<&super::Row> {
        self.tab.rows.iter().chain(&self.tab.fresh).collect()
    }
}

/// A grip's asked length kept between `least` and `most`, floored to 32-px steps from `least`.
fn sized(asked: f32, least: f32, most: f32) -> f32 {
    least + ((asked.clamp(least, most) - least) / SIZE_STEP).floor() * SIZE_STEP
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_in_steps() {
        assert_eq!(
            [sized(300.0, 280.0, 512.0), sized(100.0, 280.0, 512.0), sized(600.0, 280.0, 512.0), sized(330.0, 280.0, 512.0)],
            [280.0, 280.0, 504.0, 312.0]
        );
        assert_eq!([sized(95.0, 74.0, 384.0), sized(400.0, 74.0, 384.0)], [74.0, 362.0]);
    }
}
