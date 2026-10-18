//! The client's mouse cursor (data\sprite\cursors.spr): animated, and changing with what's under it.

use std::time::Instant;

use rustnarok_client::grf::Grf;

use crate::actor::{SpriteSet, Timing};
use crate::render::{Gpu, Sprite};

/// Cursor actions in cursors.act.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Default = 0,
    Talk = 1,
    /// Over a button or another control of a window.
    Clickable = 2,
    Rotate = 4,
    Attack = 5,
    Warp = 7,
    Pick = 9,
    /// Aiming a skill, nothing it can be used on under the pointer.
    Target = 10,
    /// Aiming a skill at something it can be used on.
    TargetValid = 11,
    NoWalk = 13,
}

/// How a cursor's frames play.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Play {
    Loop,
    /// Once, then the last frame.
    Once,
    /// Only while the left button is down; the first frame while it's up.
    WhilePressed,
}

impl Kind {
    /// Where the action's origin sits in its 50x50 frame, and its hot spot in that frame (the client's hard-coded
    /// table).
    // ponytail: the clickable hand and the valid-target look take the arrow's and the aiming look's places.
    fn layout(self) -> ([f32; 2], [f32; 2]) {
        match self {
            Kind::Talk => ([20.0, 40.0], [20.0, 20.0]),
            Kind::Pick => ([20.0, 40.0], [15.0, 15.0]),
            Kind::Warp => ([10.0, 32.0], [0.0, 0.0]),
            Kind::Rotate => ([18.0, 26.0], [10.0, 0.0]),
            Kind::NoWalk => ([13.0, 25.0], [14.0, 6.0]),
            Kind::Target | Kind::TargetValid => ([20.0, 50.0], [20.0, 28.0]),
            Kind::Default | Kind::Attack | Kind::Clickable => ([1.0, 19.0], [0.0, 0.0]),
        }
    }

    /// Each frame's time and how the frames play; the ACT's own delays don't count.
    fn timing(self) -> (f32, Play) {
        match self {
            Kind::Default | Kind::Rotate => (96.0, Play::Loop),
            Kind::Talk => (96.0, Play::Once),
            Kind::Clickable => (96.0, Play::WhilePressed),
            Kind::Attack => (480.0, Play::Loop),
            Kind::Warp => (240.0, Play::Loop),
            Kind::Pick => (24.0, Play::WhilePressed),
            Kind::Target => (144.0, Play::Loop),
            Kind::TargetValid => (72.0, Play::Loop),
            Kind::NoWalk => (48.0, Play::Loop),
        }
    }
}

/// The frame of `frames` a cursor shows `elapsed` ms after it was set (for the kinds that play while pressed, after
/// the press; None: the button is up).
pub fn frame(kind: Kind, elapsed: Option<f32>, frames: usize) -> usize {
    let (time, play) = kind.timing();
    let frames = frames.max(1);
    let Some(elapsed) = elapsed else { return 0 };
    let shown = (elapsed / time) as usize;
    match play {
        Play::Loop | Play::WhilePressed => shown % frames,
        Play::Once => shown.min(frames - 1),
    }
}

/// Half a cell in sprite pixels (35 per cell).
const HALF_CELL: f32 = 17.5;

pub struct Cursor {
    sprites: Option<SpriteSet>,
    kind: Kind,
    since: Instant,
    /// When the left button went down (None: it's up).
    pressed: Option<Instant>,
}

impl Cursor {
    pub fn load(grf: &Grf) -> Self {
        Self { sprites: SpriteSet::at(grf, "data\\sprite\\cursors"), kind: Kind::Default, since: Instant::now(), pressed: None }
    }

    pub fn kind(&self) -> Kind {
        self.kind
    }

    /// Switches the cursor; its animation restarts when it changes.
    pub fn set(&mut self, kind: Kind) {
        if kind != self.kind {
            self.kind = kind;
            self.since = Instant::now();
        }
    }

    /// Whether the left button is down, for the cursors that play only then.
    pub fn press(&mut self, down: bool) {
        match (down, self.pressed) {
            (true, None) => self.pressed = Some(Instant::now()),
            (false, Some(_)) => self.pressed = None,
            _ => {}
        }
    }

    /// The cursor's images with the hot spot at `position`, to draw over everything.
    pub fn draw(&mut self, gpu: &Gpu, position: [f32; 2]) -> Vec<Sprite<'_>> {
        // The pointer is outside the window.
        if position[0] < 0.0 || position[1] < 0.0 {
            return Vec::new();
        }
        let Some(sprites) = &mut self.sprites else { return Vec::new() };
        let (origin, hot_spot) = self.kind.layout();
        let index = self.kind as usize;
        let frames = sprites.frame_count(index).unwrap_or(1).max(1);
        let elapsed = match self.kind.timing().1 {
            Play::WhilePressed => self.pressed.map(|at| at.elapsed()),
            Play::Loop | Play::Once => Some(self.since.elapsed()),
        };
        let shown = frame(self.kind, elapsed.map(|elapsed| elapsed.as_secs_f32() * 1000.0), frames);
        let placed = sprites.frame_at(gpu, index, 0, Timing::Span((shown as f32 + 0.5) / frames as f32));
        // The table's origins assume 2D sprite drawing, which lifts every layer half a cell (17.5px).
        let anchor = [position[0] - hot_spot[0] + origin[0], position[1] - hot_spot[1] + origin[1] - HALF_CELL];
        let sprites = &*sprites;
        placed.into_iter().map(|(key, offset, size, _)| (sprites.texture(key), [anchor[0] + offset[0], anchor[1] + offset[1]], size)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plays_each_cursor_at_its_own_pace() {
        // The arrow loops at 96 ms a frame; the attack cursor at 480.
        assert_eq!([0.0, 95.0, 96.0, 400.0].map(|ms| frame(Kind::Default, Some(ms), 4)), [0, 0, 1, 0]);
        assert_eq!([479.0, 480.0, 1500.0].map(|ms| frame(Kind::Attack, Some(ms), 2)), [0, 1, 1]);
        // Talking plays once and holds the last frame.
        assert_eq!(frame(Kind::Talk, Some(10_000.0), 5), 4);
        // The hand plays only while pressed.
        assert_eq!([frame(Kind::Clickable, None, 3), frame(Kind::Clickable, Some(200.0), 3)], [0, 2]);
        assert_eq!([frame(Kind::Pick, None, 4), frame(Kind::Pick, Some(50.0), 4)], [0, 2]);
        assert_eq!([frame(Kind::TargetValid, Some(150.0), 6), frame(Kind::Target, Some(150.0), 6)], [2, 1]);
    }
}
