//! Oratio's teal magic circle over the unit (755), Analyze's turned target mark (786) and Venom Impress' icon (788), a 16 ms tick
//! at a time: squares facing the camera that breathe.
//!
//!
//! A circle's own age n counts from 1 at its first tick.

use std::time::Instant;

use super::passes;
use crate::render::Texture;
use crate::world::{EffectQuad, Vector};

pub const CIRCLE: &str = "data\\texture\\effect\\마법진1-3.bmp";
pub const TARGET: &str = "data\\texture\\effect\\target3.bmp";
pub const VENOM: &str = "data\\texture\\effect\\베놈_상태.bmp";
/// Analyze's mark lasts 99 ticks; Venom Impress' icon floats this high when the unit's sprite says nothing.
const MARK_TICKS: u32 = 99;
// ponytail: the sprite's own head offset isn't read; every unit's icon floats 30 up.
const VENOM_HEAD: f32 = 30.0;
/// The icon lifts a further 1/16 of the screen's height (768) above that.
const VENOM_LIFT: f32 = 768.0 / 16.0;
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

/// A unit with Analyze's mark or Venom Impress' icon from `start`.
struct Marked {
    unit: Option<u32>,
    start: Instant,
}

#[derive(Default)]
pub struct Circles {
    list: Vec<Circle>,
    marks: Vec<Marked>,
    venoms: Vec<Marked>,
}

/// The circle's turn at its age n: the start turn and a degree every second tick.
pub fn turn(first: f32, n: u32) -> f32 {
    (first + (n / 2) as f32) % 360.0
}

/// Half its diagonal at n, in world units: 8 breathing by 0.4 over 180 ticks.
pub fn half(n: u32) -> f32 {
    8.0 + 0.4 * ((2 * n % 360) as f32).to_radians().sin()
}

/// The venom icon's alpha at n: 10 a tick to 220.
pub fn venom_alpha(n: u32) -> f32 {
    (10 * n).min(220) as f32
}

/// Its half-diagonal at n, in world units.
pub fn venom_half(n: u32) -> f32 {
    4.0 + 4.0 * ((2 * n % 360) as f32).to_radians().sin() * 0.05
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

    /// Analyze's turned target mark over `unit`.
    pub fn mark(&mut self, unit: Option<u32>, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        let _ = rand(360);
        self.marks.push(Marked { unit, start: now });
    }

    /// Venom Impress' icon over `unit`, replacing the last.
    pub fn venom(&mut self, unit: Option<u32>, now: Instant) {
        self.venoms.retain(|venom| venom.unit != unit);
        self.venoms.push(Marked { unit, start: now });
    }

    /// Venom Impress went off `unit`.
    pub fn forget_venom(&mut self, unit: Option<u32>) {
        self.venoms.retain(|venom| venom.unit != unit);
    }

    pub fn advance(&mut self, now: Instant) {
        self.marks.retain(|mark| passes(mark.start, now) <= MARK_TICKS + 1);
    }

    /// Analyze's marks at `now`: diamonds in the camera's plane 10 over the feet, added on.
    pub fn mark_quads<'a>(
        &self,
        now: Instant,
        texture_of: impl Fn(&str) -> Option<&'a Texture>,
        position_of: &impl Fn(Option<u32>) -> Option<Vector>,
    ) -> Vec<EffectQuad<'a>> {
        let Some(texture) = texture_of(TARGET) else { return Vec::new() };
        self.marks
            .iter()
            .filter_map(|mark| {
                let n = passes(mark.start, now);
                let feet = position_of(mark.unit).filter(|_| (1..=MARK_TICKS).contains(&n))?;
                let r = (24.0 + 24.0 * ((2 * n % 360) as f32).to_radians().sin() * 0.05) * PIXELS_PER_UNIT;
                let corners = [[r, 0.0], [0.0, r], [-r, 0.0], [0.0, -r]];
                Some((texture, [feet[0], feet[1] + 10.0, feet[2]], corners, [1.0, 1.0, 1.0, 250.0 / 255.0], true, false))
            })
            .collect()
    }

    /// Venom Impress' icons at `now`: upright and mirrored, over everything; `str_rise` is the compensation for that.
    pub fn venom_quads<'a>(
        &self,
        now: Instant,
        str_rise: f32,
        texture_of: impl Fn(&str) -> Option<&'a Texture>,
        position_of: &impl Fn(Option<u32>) -> Option<Vector>,
    ) -> Vec<EffectQuad<'a>> {
        let Some(texture) = texture_of(VENOM) else { return Vec::new() };
        self.venoms
            .iter()
            .filter_map(|venom| {
                let n = passes(venom.start, now);
                let feet = position_of(venom.unit).filter(|_| n > 0)?;
                let h = venom_half(n) * std::f32::consts::FRAC_1_SQRT_2 * PIXELS_PER_UNIT;
                let up = str_rise - VENOM_LIFT;
                // Right-bottom, left-bottom, left-top, right-top for the mirrored picture.
                let corners = [[h, h + up], [-h, h + up], [-h, -h + up], [h, -h + up]];
                Some((texture, [feet[0], feet[1] + VENOM_HEAD, feet[2]], corners, [1.0, 1.0, 1.0, venom_alpha(n) / 255.0], false, true))
            })
            .collect()
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

    #[test]
    fn analyze_turns_a_mark_and_venom_impress_floats_an_icon() {
        let start = Instant::now();
        let mut circles = Circles::default();
        circles.mark(Some(1), start, |_| 0);
        circles.venom(Some(2), start);
        circles.venom(Some(2), start);
        assert_eq!((circles.marks.len(), circles.venoms.len()), (1, 1));
        // The mark: half-diagonal 24 ± 5 % breathing: 24.04 at n = 1 and 25.2 at 45; alpha 250; 99 ticks.
        let r = |n: u32| 24.0 + 24.0 * ((2 * n % 360) as f32).to_radians().sin() * 0.05;
        assert!((r(1) - 24.04).abs() < 0.01 && (r(45) - 25.2).abs() < 0.01);
        // The icon: alpha 10, 220, 220 at n = 1, 22, 23; r = 4 + 4 · sin(2n) · 0.05.
        assert_eq!([1, 22, 23].map(venom_alpha), [10.0, 220.0, 220.0]);
        assert!((venom_half(45) - 4.2).abs() < 1e-4);
        // Mirrored upright corners (right-bottom first) and drawn over everything; one icon to a unit; it goes with the status.
        circles.forget_venom(Some(2));
        assert!(circles.venoms.is_empty());
        circles.advance(start + crate::prims::TICK * 200);
        assert!(circles.marks.is_empty());
    }
}
