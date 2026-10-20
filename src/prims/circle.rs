//! Oratio's teal magic circle over the unit (755), a 16 ms tick at a time: a square facing the camera, turning half a degree a
//! tick and breathing, as long as the status lasts.
//!
//! A circle's own age n counts from 1 at its first tick.

use std::time::Instant;

use super::passes;
use crate::render::Texture;
use crate::world::{EffectQuad, Vector};

pub const CIRCLE: &str = "data\\texture\\effect\\마법진1-3.bmp";
/// #339999 at alpha 250, added on, 30 over the unit's feet.
const TINT: [f32; 4] = [51.0 / 255.0, 153.0 / 255.0, 153.0 / 255.0, 250.0 / 255.0];
const RISE: f32 = 30.0;
const PIXELS_PER_UNIT: f32 = 7.0;

struct Circle {
    unit: Option<u32>,
    start: Instant,
    /// Its start turn.
    first: f32,
}

#[derive(Default)]
pub struct Circles {
    list: Vec<Circle>,
}

/// The circle's turn at its age n: the start turn and a degree every second tick.
pub fn turn(first: f32, n: u32) -> f32 {
    (first + (n / 2) as f32) % 360.0
}

/// Half its diagonal at n, in world units: 8 breathing by 0.4 over 180 ticks.
pub fn half(n: u32) -> f32 {
    8.0 + 0.4 * ((2 * n % 360) as f32).to_radians().sin()
}

/// The corners V1, V0, V3, V2 (V_j at the angle T − 90 j) of a square of half-diagonal `half` px turned `turn` degrees, for the
/// texture coordinates (0, 1), (1, 1), (1, 0), (0, 0).
pub fn corners(turn: f32, half: f32) -> [[f32; 2]; 4] {
    let v = |j: f32| {
        let (sin, cos) = (turn - 90.0 * j).to_radians().sin_cos();
        [half * cos, half * sin]
    };
    [v(1.0), v(0.0), v(3.0), v(2.0)]
}

impl Circles {
    /// A circle on `unit` (None = the player), replacing its last; its start turn from `rand(n)` (0 to n − 1), and the second
    /// draw the game makes after it thrown away.
    pub fn oratio(&mut self, unit: Option<u32>, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        self.list.retain(|circle| circle.unit != unit);
        let first = rand(360) as f32;
        let _ = rand(360);
        self.list.push(Circle { unit, start: now, first });
    }

    /// Oratio's status went off `unit`.
    pub fn forget(&mut self, unit: Option<u32>) {
        self.list.retain(|circle| circle.unit != unit);
    }

    /// The circles at `now`, over their units' feet (`position_of`).
    pub fn quads<'a>(
        &self,
        now: Instant,
        texture_of: impl Fn(&str) -> Option<&'a Texture>,
        position_of: &impl Fn(Option<u32>) -> Option<Vector>,
    ) -> Vec<EffectQuad<'a>> {
        let Some(texture) = texture_of(CIRCLE) else { return Vec::new() };
        self.list
            .iter()
            .filter_map(|circle| {
                let n = passes(circle.start, now);
                let feet = position_of(circle.unit).filter(|_| n > 0)?;
                let place = [feet[0], feet[1] + RISE, feet[2]];
                Some((texture, place, corners(turn(circle.first, n), half(n) * PIXELS_PER_UNIT), TINT, true, false))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(got: f32, want: f32) -> bool {
        (got - want).abs() < 0.001
    }

    #[test]
    fn oratio_turns_a_breathing_square_over_the_unit() {
        let start = Instant::now();
        let mut circles = Circles::default();
        let mut draws = 0;
        circles.oratio(Some(1), start, |_| {
            draws += 1;
            0
        });
        // The start turn and one more draw; one circle to a unit.
        assert_eq!(draws, 2);
        circles.oratio(Some(1), start, |_| 0);
        assert_eq!(circles.list.len(), 1);
        // With a start turn 0: the turn at n = 1, 2, 3, 4 is 0, 1, 1, 2; half-diagonal at n = 1, 45, 135: 8.014, 8.4, 7.6.
        assert_eq!([1, 2, 3, 4].map(|n| turn(0.0, n)), [0.0, 1.0, 1.0, 2.0]);
        assert!(near(half(1), 8.014) && near(half(45), 8.4) && near(half(135), 7.6));
        assert!(near(turn(359.0, 4), 1.0));
        // V1 (0, 1), V0 (1, 1), V3 (1, 0), V2 (0, 0): at turn 0 V0 is out along +x, V1 along −y (the angle −90).
        let square = corners(0.0, 8.0);
        assert!(near(square[1][0], 8.0) && near(square[1][1], 0.0));
        assert!(near(square[0][0], 0.0) && near(square[0][1], -8.0));
        assert!(near(square[3][0], -8.0) && near(square[2][1], 8.0));
        // Alpha 250 from the first tick for good; 30 up; following the unit.
        assert_eq!(TINT[3] * 255.0, 250.0);
        let feet = [10.0, 2.0, 10.0];
        let _ = feet;
        assert!(circles.quads(start, |_| None, &|_| Some(feet)).is_empty());
        circles.forget(Some(1));
        assert!(circles.list.is_empty());
    }
}
