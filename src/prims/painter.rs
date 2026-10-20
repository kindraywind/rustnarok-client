//! Body Painting's four blue strokes (811), a 16 ms tick at a time: strips in the camera's plane, each painted out along a diagonal
//! and then faded.
//!
//! A stroke's own age n counts from 1 at its first drawn tick.

use std::time::Instant;

use super::passes;
use crate::render::Texture;
use crate::world::{EffectQuad, Vector};

pub const STICK: &str = "data\\texture\\effect\\freezing_stick.bmp";
const SIZE: f32 = 20.0;
/// The strip's angle on the screen: 215° points up and to the left.
const ANGLE: f32 = 215.0;
const TINT: [f32; 3] = [100.0 / 255.0, 100.0 / 255.0, 1.0];
const PIXELS_PER_UNIT: f32 = 7.0;
/// Each stroke starts 10 ticks after the last and is drawn for ages 1 to 147; the heights they stand at over the source's feet.
const START_GAP: u32 = 10;
const LAST_AGE: u32 = 147;
const HEIGHTS: [f32; 4] = [30.0, 20.0, 10.0, 0.0];

/// A source painting from the place `at`: the strokes follow it sideways, not in height.
struct Painting {
    unit: Option<u32>,
    start: Instant,
    height: f32,
}

#[derive(Default)]
pub struct Painter {
    list: Vec<Painting>,
}

/// How far a stroke is painted at its age n (percent): 5 a tick to 100.
pub fn painted(n: u32) -> u32 {
    (5 * n).min(100)
}

/// The inner edge's distance at n: from 20 through the centre to −20.
pub fn inner(n: u32) -> f32 {
    SIZE * (1.0 - painted(n) as f32 / 50.0)
}

/// A stroke's alpha at n: 255 to age 20, then 2 less a tick.
pub fn alpha(n: u32) -> f32 {
    (255.0 - 2.0 * n.saturating_sub(20) as f32).max(0.0)
}

impl Painter {
    /// Body Painting by the source `unit` standing at `at`.
    pub fn paint(&mut self, unit: Option<u32>, at: Vector, now: Instant) {
        let _ = at;
        self.list.push(Painting { unit, start: now, height: at[1] });
    }

    pub fn advance(&mut self, now: Instant) {
        self.list.retain(|painting| passes(painting.start, now) <= 3 * START_GAP + LAST_AGE + 1);
    }

    /// The strokes at `now`, over everything: each a strip from the inner edge to the outer one, 10 wide each side of its axis.
    /// `str_rise` is the compensation for being drawn over everything.
    pub fn quads<'a>(
        &self,
        now: Instant,
        str_rise: f32,
        texture_of: impl Fn(&str) -> Option<&'a Texture>,
        position_of: &impl Fn(Option<u32>) -> Option<Vector>,
    ) -> Vec<EffectQuad<'a>> {
        let Some(texture) = texture_of(STICK) else { return Vec::new() };
        let (sin, cos) = ANGLE.to_radians().sin_cos();
        let mut quads = Vec::new();
        for painting in &self.list {
            let (Some(feet), Some(t)) = (position_of(painting.unit), passes(painting.start, now).checked_sub(1)) else { continue };
            for (k, height) in HEIGHTS.iter().enumerate() {
                let n = (t + 1).saturating_sub(START_GAP * k as u32);
                if n == 0 || n > LAST_AGE {
                    continue;
                }
                let (near, far, half) = (inner(n) * PIXELS_PER_UNIT, SIZE * PIXELS_PER_UNIT, 0.5 * SIZE * PIXELS_PER_UNIT);
                // The axis points (cos, sin) on the screen, y down; the strip's sides lie along its perpendicular.
                let point = |along: f32, side: f32| [cos * along - sin * side, sin * along + cos * side + str_rise];
                let corners = [point(near, -half), point(near, half), point(far, half), point(far, -half)];
                let place = [feet[0], painting.height + height, feet[2]];
                quads.push((texture, place, corners, [TINT[0], TINT[1], TINT[2], alpha(n) / 255.0], true, true));
            }
        }
        quads
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prims::TICK;

    #[test]
    fn body_painting_paints_four_strokes_down_from_over_the_head() {
        let start = Instant::now();
        let mut painter = Painter::default();
        painter.paint(Some(1), [10.0, 2.0, 10.0], start);
        // For the first stroke: painted % at n = 1, 10, 20, 21: 5, 50, 100, 100; the inner edge 18, 0, −20; alpha 255 … 253, 175, 1, 0.
        assert_eq!([1, 10, 20, 21, 60, 147, 148].map(painted), [5, 50, 100, 100, 100, 100, 100]);
        assert_eq!([0, 1, 10, 20].map(inner), [20.0, 18.0, 0.0, -20.0]);
        assert_eq!([1, 20, 21, 60, 100, 147, 148].map(alpha), [255.0, 255.0, 253.0, 175.0, 95.0, 1.0, 0.0]);
        // The length 20 − inner: 2 at n = 1, 20 at 10, 40 from 20.
        assert_eq!([1, 10, 20].map(|n| SIZE - inner(n)), [2.0, 20.0, 40.0]);
        // The strokes stand 30, 20, 10 and 0 over the feet at tick 0, one more every 10 ticks, the last drawn at tick 10k + 146.
        assert_eq!(HEIGHTS, [30.0, 20.0, 10.0, 0.0]);
        assert_eq!(painter.list[0].height, 2.0);
        painter.advance(start + TICK * 176);
        assert_eq!(painter.list.len(), 1);
        painter.advance(start + TICK * 400);
        assert!(painter.list.is_empty());
    }
}
