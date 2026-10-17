//! The client's mouse cursor (data\sprite\cursors.spr): animated, and changing with what's under it.

use std::time::Instant;

use roclient::grf::Grf;

use crate::actor::{SpriteSet, Timing};
use crate::render::{Gpu, Sprite};

/// Cursor actions in cursors.act.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Default = 0,
    Talk = 1,
    Rotate = 4,
    Attack = 5,
    Warp = 7,
    Pick = 9,
    /// Aiming a skill.
    Target = 10,
    NoWalk = 13,
}

impl Kind {
    /// Where the action's origin sits in its 50x50 frame, its hot spot in that frame, how much slower than the
    /// ACT's delay it plays, and whether it loops (the client's hard-coded table).
    fn layout(self) -> ([f32; 2], [f32; 2], f32, bool) {
        match self {
            Kind::Talk => ([20.0, 40.0], [20.0, 20.0], 1.0, false),
            Kind::Warp => ([10.0, 32.0], [0.0, 0.0], 1.0, true),
            Kind::Rotate => ([18.0, 26.0], [10.0, 0.0], 1.0, true),
            Kind::Pick => ([20.0, 40.0], [15.0, 15.0], 1.0, false),
            Kind::NoWalk => ([13.0, 25.0], [14.0, 6.0], 1.0, true),
            Kind::Target => ([20.0, 50.0], [20.0, 28.0], 0.5, true),
            Kind::Default | Kind::Attack => ([1.0, 19.0], [0.0, 0.0], 2.0, true),
        }
    }
}

/// Half a cell in sprite pixels (35 per cell).
const HALF_CELL: f32 = 17.5;

pub struct Cursor {
    sprites: Option<SpriteSet>,
    kind: Kind,
    since: Instant,
}

impl Cursor {
    pub fn load(grf: &Grf) -> Self {
        Self { sprites: SpriteSet::at(grf, "data\\sprite\\cursors"), kind: Kind::Default, since: Instant::now() }
    }

    /// Switches the cursor; its animation restarts when it changes.
    pub fn set(&mut self, kind: Kind) {
        if kind != self.kind {
            self.kind = kind;
            self.since = Instant::now();
        }
    }

    /// The cursor's images with the hot spot at `position`, to draw over everything.
    pub fn draw(&mut self, gpu: &Gpu, position: [f32; 2]) -> Vec<Sprite<'_>> {
        // The pointer is outside the window.
        if position[0] < 0.0 || position[1] < 0.0 {
            return Vec::new();
        }
        let Some(sprites) = &mut self.sprites else { return Vec::new() };
        let (origin, hot_spot, slowness, looping) = self.kind.layout();
        let index = self.kind as usize;
        let elapsed = self.since.elapsed().as_secs_f32() * 1000.0 / slowness;
        let timing = if looping { Timing::Loop(elapsed) } else { Timing::Span(elapsed / sprites.index_ms(index).unwrap_or(1.0).max(1.0)) };
        let placed = sprites.frame_at(gpu, index, 0, timing);
        // The table's origins assume 2D sprite drawing, which lifts every layer half a cell (17.5px).
        let anchor = [position[0] - hot_spot[0] + origin[0], position[1] - hot_spot[1] + origin[1] - HALF_CELL];
        let sprites = &*sprites;
        placed.into_iter().map(|(key, offset, size)| (sprites.texture(key), [anchor[0] + offset[0], anchor[1] + offset[1]], size)).collect()
    }
}
