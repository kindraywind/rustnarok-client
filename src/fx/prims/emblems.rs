//! King's Grace's four pulsing emblems (1051), a 16 ms tick at a time.
//!
//! An emblem's own age n counts from 1 on its first tick.

use std::time::Instant;

use super::passes;
use crate::world::Vector;

pub const EMBLEM: &str = "data\\texture\\effect\\lg_kings_grace.tga";
const PIXELS_PER_UNIT: f32 = 7.0;
/// A copy lasts 998 ticks (the length less one); the emblems stand 15 out, 8 up, and are 8 across to a corner.
const PERIOD: u32 = 998;
const OUT: f32 = 15.0;
const UP: f32 = 8.0;
const RADIUS: f32 = 8.0;
/// The glow's colour (#B4B4B4).
const GLOW: [f32; 3] = [180.0 / 255.0, 180.0 / 255.0, 180.0 / 255.0];

/// The four emblems round the cell `at` of skill unit `id`, this copy made at `start`.
struct Cell {
    id: u32,
    at: Vector,
    start: Instant,
}

#[derive(Default)]
pub struct Emblems {
    cells: Vec<Cell>,
}

/// A square to draw: where, its corners in screen pixels, its colour and whether it adds on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EmblemDraw {
    pub at: Vector,
    pub corners: [[f32; 2]; 4],
    pub tint: [f32; 4],
    pub additive: bool,
}

/// An emblem's alpha at its age n: 4 a tick up to 255 (at 64), then 2 a tick down to 0 (at 192), and again.
pub fn emblem_alpha(n: u32) -> f32 {
    match n % 192 {
        0 => 0.0,
        m @ 1..=64 => (4 * m).min(255) as f32,
        m => (255 - 2 * (m - 64)) as f32,
    }
}

/// The glow's radius for an emblem of `alpha`: 8 swelling to 9.6.
pub fn glow_radius(alpha: f32) -> f32 {
    (alpha / 255.0 * 0.2 + 1.0) * RADIUS
}

/// The emblems' offsets from the cell centre: 15 out north, east, south and west, 8 up.
pub fn offsets() -> [Vector; 4] {
    [0.0, 90.0, 180.0, 270.0_f32].map(|theta| {
        let (sin, cos) = theta.to_radians().sin_cos();
        [OUT * sin, UP, OUT * cos]
    })
}

/// A square of corner radius `radius` world units, upright, as screen-pixel offsets.
// ponytail: the picture stands upright; the corner angles' own uv order would turn it over.
fn square(radius: f32) -> [[f32; 2]; 4] {
    [225.0_f32, 315.0, 45.0, 135.0].map(|angle| {
        let (sin, cos) = angle.to_radians().sin_cos();
        [radius * PIXELS_PER_UNIT * cos, radius * PIXELS_PER_UNIT * sin]
    })
}

impl Emblems {
    /// The emblems of skill unit `id` over its cell `at`.
    pub fn keep(&mut self, id: u32, at: Vector, now: Instant) {
        self.cells.push(Cell { id, at, start: now });
    }

    pub fn forget(&mut self, id: u32) {
        self.cells.retain(|cell| cell.id != id);
    }

    /// Starts the next copy of what has run its length.
    pub fn advance(&mut self, now: Instant) {
        for cell in &mut self.cells {
            while passes(cell.start, now) > PERIOD {
                cell.start += super::TICK * PERIOD;
            }
        }
    }

    /// The squares at `now`: per emblem a grey glow added on, then the emblem itself, blended.
    pub fn draws(&self, now: Instant) -> Vec<EmblemDraw> {
        let mut draws = Vec::new();
        for cell in &self.cells {
            let n = passes(cell.start, now);
            let alpha = emblem_alpha(n);
            for offset in offsets() {
                let at = [cell.at[0] + offset[0], cell.at[1] + offset[1], cell.at[2] + offset[2]];
                let [red, green, blue] = GLOW;
                draws.push(EmblemDraw { at, corners: square(glow_radius(alpha)), tint: [red, green, blue, alpha / 255.0], additive: true });
                draws.push(EmblemDraw { at, corners: square(RADIUS), tint: [1.0; 4], additive: false });
            }
        }
        draws
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prims::TICK;

    fn near(got: f32, want: f32) -> bool {
        (got - want).abs() < 1e-3
    }

    #[test]
    fn kings_grace_pulses_four_emblems_round_the_cell() {
        let start = Instant::now();
        let mut emblems = Emblems::default();
        emblems.keep(7, [10.0, 0.0, 20.0], start);
        // Offsets (0, 8, 15), (15, 8, 0), (0, 8, −15), (−15, 8, 0) from the cell centre.
        let want = [[0.0, 8.0, 15.0], [15.0, 8.0, 0.0], [0.0, 8.0, -15.0], [-15.0, 8.0, 0.0]];
        assert!(offsets().iter().zip(want).all(|(got, want)| (0..3).all(|axis| near(got[axis], want[axis]))));
        // Alpha at n = 1, 63, 64, 65, 192, 193: 4, 252, 255, 253, 0, 4.
        assert_eq!([1, 63, 64, 65, 192, 193].map(emblem_alpha), [4.0, 252.0, 255.0, 253.0, 0.0, 4.0]);
        // The glow's radius at alpha 255 and 0: 9.6 and 8.
        assert!(near(glow_radius(255.0), 9.6) && near(glow_radius(0.0), 8.0));
        // Per frame two squares an emblem: the glow #B4B4B4 added on at alpha, the emblem #FFFFFF at 255 blended.
        let draws = emblems.draws(start + TICK * 63);
        assert_eq!(draws.len(), 8);
        assert!(draws[0].additive && !draws[1].additive);
        assert_eq!(draws[0].tint[..3], GLOW);
        assert!(near(draws[0].tint[3] * 255.0, emblem_alpha(64)));
        assert_eq!(draws[1].tint, [1.0; 4]);
        assert!(near(draws[0].at[0], 10.0) && near(draws[0].at[1], 8.0) && near(draws[0].at[2], 35.0));
        // The glow swells past the emblem: 9.6 against 8 a side at the top of the pulse.
        let across = |draw: &EmblemDraw| draw.corners[1][0] - draw.corners[0][0];
        assert!(across(&draws[0]) > across(&draws[1]));
        // A copy again at the 998th tick: the pulse starts over.
        emblems.advance(start + TICK * 998);
        assert!(near(emblems.draws(start + TICK * 998)[0].tint[3] * 255.0, 4.0));
        // It goes with its unit.
        emblems.forget(7);
        assert!(emblems.draws(start).is_empty());
    }
}
