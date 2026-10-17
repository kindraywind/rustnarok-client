//! The option menu's audio settings (Esc, then Sound): music (BGM) and effects (SFX), each on or off, with a slider
//! for its volume.

use roclient::grf::Grf;
use roclient::text::Font;

use crate::render::{Gpu, Texture};
use crate::sound::{MAX_VOLUME, Settings};
use crate::window::{Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI};

// ponytail: a 280px window with a row per setting; calibrate against the real client.
const WIDTH: f32 = 280.0;
const ROW: f32 = 22.0;
const TOP: f32 = TITLE_HEIGHT + 8.0;
const HEIGHT: f32 = TOP + 2.0 * ROW + 4.0;
const CHECK_X: f32 = 12.0;
const SLIDER_X: f32 = 80.0;
const ARROW: f32 = 13.0;
/// The track between the arrows, and the thumb on it.
const TRACK: f32 = 174.0;
const THUMB: f32 = 24.0;
const STEP: u8 = 8;
const POSITION: [f32; 2] = [372.0, 300.0];
/// "Audio Settings", then "    BGM" and "    SFX" (their spaces leave room for the checkbox).
const TITLE: usize = 1485;
const LABELS: [usize; 2] = [1495, 1496];

pub struct Assets {
    check: [Texture; 2],
    arrows: [Texture; 2],
    track: Texture,
    /// The thumb's left end, middle and right end.
    thumb: [Texture; 3],
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |name: &str| gpu.load_texture(grf, &format!("{UI}{name}.bmp"), false);
        Self {
            check: ["checkbox_0", "checkbox_1"].map(image),
            arrows: ["scroll1left", "scroll1right"].map(image),
            track: image("scroll1mid"),
            thumb: ["scroll1bar_left", "scroll1bar_mid", "scroll1bar_right"].map(image),
        }
    }
}

/// What's under the pointer; settings by number, 0 music and 1 effects.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(TitleHit),
    Check(usize),
    Less(usize),
    More(usize),
    /// The track, and the volume at that point.
    Track(usize, u8),
    Window,
}

pub struct AudioWindow {
    pub frame: Frame,
    /// The slider whose thumb is held.
    dragging: Option<usize>,
}

impl AudioWindow {
    pub fn new() -> Self {
        let mut frame = Frame::new(POSITION, WIDTH);
        frame.open = true;
        Self { frame, dragging: None }
    }

    fn row_top(kind: usize) -> f32 {
        TOP + kind as f32 * ROW
    }

    /// The volume at `x` across the window (the thumb's middle there).
    fn volume_at(x: f32) -> u8 {
        let along = ((x - SLIDER_X - ARROW - THUMB / 2.0) / (TRACK - THUMB)).clamp(0.0, 1.0);
        (along * f32::from(MAX_VOLUME)).round() as u8
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let local = self.frame.local(cursor, HEIGHT)?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(Hit::Title(hit));
        }
        let row = ((local[1] - TOP) / ROW).floor();
        if !(0.0..2.0).contains(&row) {
            return Some(Hit::Window);
        }
        let (kind, x) = (row as usize, local[0]);
        Some(if x < SLIDER_X {
            Hit::Check(kind)
        } else if x < SLIDER_X + ARROW {
            Hit::Less(kind)
        } else if x < SLIDER_X + ARROW + TRACK {
            Hit::Track(kind, Self::volume_at(x))
        } else if x < SLIDER_X + 2.0 * ARROW + TRACK {
            Hit::More(kind)
        } else {
            Hit::Window
        })
    }

    /// A press: the track grabs the thumb there, the title bar the window. Returns whether the settings changed.
    pub fn press(&mut self, hit: Hit, cursor: [f32; 2], settings: &mut Settings) -> bool {
        match hit {
            Hit::Title(TitleHit::Title) => self.frame.start_drag(cursor),
            Hit::Track(kind, volume) => {
                self.dragging = Some(kind);
                settings.volume[kind] = volume;
                return true;
            }
            _ => {}
        }
        false
    }

    /// A click: the checkboxes turn a setting on or off, the arrows step its volume. Returns whether the settings
    /// changed.
    pub fn click(&mut self, hit: Hit, settings: &mut Settings) -> bool {
        match hit {
            Hit::Title(TitleHit::Close) => self.frame.open = false,
            Hit::Check(kind) => settings.on[kind] = !settings.on[kind],
            Hit::Less(kind) => settings.volume[kind] = settings.volume[kind].saturating_sub(STEP),
            Hit::More(kind) => settings.volume[kind] = (settings.volume[kind] + STEP).min(MAX_VOLUME),
            Hit::Title(_) | Hit::Track(..) | Hit::Window => return false,
        }
        true
    }

    /// The held thumb (or the window) follows the pointer; returns whether the settings changed.
    pub fn drag_to(&mut self, cursor: [f32; 2], settings: &mut Settings) -> bool {
        self.frame.drag_to(cursor, HEIGHT);
        let Some(kind) = self.dragging else { return false };
        let volume = Self::volume_at(cursor[0] - self.frame.position[0]);
        std::mem::replace(&mut settings.volume[kind], volume) != volume
    }

    /// Lets go of the thumb or the window; returns whether one was held.
    pub fn drop_drag(&mut self) -> bool {
        self.dragging.take().is_some() | self.frame.drop_drag()
    }

    pub fn draw<'a>(
        &self,
        layer: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        font: &Font,
        messages: &[String],
        settings: &Settings,
        pointer: [f32; 2],
    ) {
        let at = |local: [f32; 2]| self.frame.at(local);
        self.frame.draw_title(layer, chrome, messages.get(TITLE).map_or("", String::as_str), pointer);
        if self.frame.minimized {
            return;
        }
        layer.sprites.push((&chrome.border, at([0.0, TITLE_HEIGHT]), [WIDTH, HEIGHT - TITLE_HEIGHT]));
        layer.sprites.push((&chrome.white, at([1.0, TITLE_HEIGHT]), [WIDTH - 2.0, HEIGHT - TITLE_HEIGHT - 1.0]));
        for (kind, &label) in LABELS.iter().enumerate() {
            let top = Self::row_top(kind);
            let middle = |height: f32| top + ((ROW - height) / 2.0).floor();
            let check = &assets.check[usize::from(settings.on[kind])];
            layer.sprites.push((check, at([CHECK_X, middle(check.size[1])]), check.size));
            layer.texts.push((messages.get(label).cloned().unwrap_or_default(), at([CHECK_X, middle(font.line_height())]), [0, 0, 0]));
            let [less, more] = &assets.arrows;
            layer.sprites.push((less, at([SLIDER_X, middle(ARROW)]), less.size));
            layer.sprites.push((&assets.track, at([SLIDER_X + ARROW, middle(ARROW)]), [TRACK, ARROW]));
            layer.sprites.push((more, at([SLIDER_X + ARROW + TRACK, middle(ARROW)]), more.size));
            // The thumb: its ends around a stretched middle, placed by the volume.
            let left = SLIDER_X + ARROW + (TRACK - THUMB) * f32::from(settings.volume[kind]) / f32::from(MAX_VOLUME);
            let [start, body, end] = &assets.thumb;
            let top = middle(start.size[1]);
            layer.sprites.push((start, at([left, top]), start.size));
            layer.sprites.push((body, at([left + start.size[0], top]), [THUMB - start.size[0] - end.size[0], body.size[1]]));
            layer.sprites.push((end, at([left + THUMB - end.size[0], top]), end.size));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_steps_and_slides() {
        let mut window = AudioWindow::new();
        let mut settings = Settings::default();
        let corner = window.frame.position;
        let at = |local: [f32; 2]| [corner[0] + local[0], corner[1] + local[1]];
        let row = |kind: usize| AudioWindow::row_top(kind) + ROW / 2.0;
        assert_eq!(window.hit(at([20.0, row(1)])), Some(Hit::Check(1)));
        assert!(window.click(Hit::Check(1), &mut settings));
        assert_eq!(window.hit(at([SLIDER_X + 2.0, row(0)])), Some(Hit::Less(0)));
        assert_eq!(window.hit(at([SLIDER_X + ARROW + 1.0, row(0)])), Some(Hit::Track(0, 0)), "the thumb's middle can't go past the track's end");
        assert_eq!(window.hit(at([SLIDER_X + ARROW + TRACK - 1.0, row(0)])), Some(Hit::Track(0, MAX_VOLUME)));
        window.click(Hit::More(0), &mut settings);
        assert_eq!(settings, Settings { on: [true, false], volume: [108, 100] });
        assert!(window.press(Hit::Track(0, 20), at([0.0, 0.0]), &mut settings));
        let middle = window.frame.position[0] + SLIDER_X + ARROW + TRACK / 2.0;
        assert!(window.drag_to([middle, 0.0], &mut settings));
        assert_eq!(settings.volume[0], 64);
        assert!(window.drop_drag());
    }
}
