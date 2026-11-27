//! Frost Misty's cloud (the game's prim kind 131, effect 726): ten squares of one picture facing the camera, added on,
//! 30 over the cell. They grow 1 % a tick; nine flash and fade, the smallest holds for 100 ticks.

use std::time::Instant;

use super::passes;
use crate::render::Texture;
use crate::world::{EffectQuad, Vector};

pub const PICTURE: &str = "data\\texture\\effect\\freeze_a.bmp";
/// The effect that makes it, and how many ticks its snow goes on for.
pub const EF_FROSTMISTY: u32 = 726;
const RISE: f32 = 30.0;
const PIXELS_PER_UNIT: f32 = 7.0;
/// The last tick anything of it shows: the smallest square's alpha is gone at 150.
const LAST: u32 = 149;

/// A cloud over the point `at` from `start`.
pub struct Mist {
    pub at: Vector,
    pub start: Instant,
}

/// Square `k`'s alpha at age `n` (1 on its first tick): 13 a tick up to 70 + 20 k for 19 ticks, then 5 a tick down; the
/// last one (k = 9) only from tick 101.
pub fn alpha(k: u32, n: u32) -> u32 {
    let top = (13 * n.min(19)).min(70 + 20 * k);
    let falling = if k == 9 { n.saturating_sub(100) } else { n.saturating_sub(19) };
    top.saturating_sub(5 * falling)
}

/// The size at age `n`: 24 growing 1 % a tick, twice a tick once the last square fades.
pub fn size(n: u32) -> f32 {
    24.0 * 1.01_f32.powi((n + n.saturating_sub(100)) as i32)
}

impl Mist {
    pub fn alive(&self, now: Instant) -> bool {
        passes(self.start, now) <= LAST
    }

    /// Its squares at `now`: square k has the half-diagonal size + 3 (9 − k); the last is a lighter blue.
    pub fn quads<'a>(&self, now: Instant, texture: &'a Texture) -> Vec<EffectQuad<'a>> {
        let n = passes(self.start, now);
        if !(1..=LAST).contains(&n) {
            return Vec::new();
        }
        let place = [self.at[0], self.at[1] + RISE, self.at[2]];
        (0..10)
            .filter(|&k| alpha(k, n) > 0)
            .map(|k| {
                let half = (size(n) + 3.0 * (9 - k) as f32) * std::f32::consts::FRAC_1_SQRT_2 * PIXELS_PER_UNIT;
                let other = if k == 9 { 55.0 } else { 5.0 } / 255.0;
                let corners = [[-half, -half], [half, -half], [half, half], [-half, half]];
                (texture, place, corners, [other, other, 1.0, alpha(k, n) as f32 / 255.0], true, false)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_squares_flash_and_the_last_one_holds() {
        // Rising 13 a tick to each cap; the widest (k = 0) stops at 70, the last at 247 (19 ticks of 13).
        assert_eq!([alpha(0, 1), alpha(0, 5), alpha(0, 6), alpha(0, 19)], [13, 65, 70, 70]);
        assert_eq!([alpha(8, 19), alpha(9, 19)], [230, 247]);
        // From tick 20 the nine fall 5 a tick; the last holds to 100, then falls and is gone at 150.
        assert_eq!([alpha(0, 20), alpha(0, 33), alpha(0, 34), alpha(8, 65), alpha(8, 66)], [65, 0, 0, 0, 0]);
        assert_eq!([alpha(9, 100), alpha(9, 101), alpha(9, 149), alpha(9, 150)], [247, 242, 2, 0]);
        // 24 at the start, 1 % a tick, 2 % once the last one fades.
        assert!((size(0) - 24.0).abs() < 1e-4 && (size(100) - 24.0 * 1.01_f32.powi(100)).abs() < 1e-3);
        assert!((size(110) / size(100) - 1.01_f32.powi(20)).abs() < 1e-4);
    }
}
