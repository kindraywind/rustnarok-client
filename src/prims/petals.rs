//! Lope's 30 glowing motes and 64 petals (411), a 16 ms tick at a time.
//!
//! A shape's own age n counts from 1 on its first tick after its wait; e is the ticks since the effect started.

use std::time::Instant;

use super::passes;
use crate::render::Texture;
use crate::world::{EffectQuad, Vector};

pub const MOTE: &str = super::spray::DOT;
/// The petal pictures, one prim of four petals for each of 16 prims: peong1, peong2, peong3, peong2 for each quarter.
pub const PETALS: [&str; 3] = ["data\\texture\\effect\\peong1.tga", "data\\texture\\effect\\peong2.tga", "data\\texture\\effect\\peong3.tga"];
const PETAL_OF_QUARTER: [usize; 4] = [0, 1, 2, 1];
const PIXELS_PER_UNIT: f32 = 7.0;
/// Everything is made at tick 35 of the effect, which lasts 300 ticks.
const MADE_AT: u32 = 35;
const LIFE: u32 = 300;
const MOTES: usize = 30;
const PRIMS_PER_QUARTER: usize = 4;
const PETALS_PER_PRIM: usize = 4;

/// One wandering mote: its rise a tick, size, wait, offset from the spot and height, and the two wander angles with their targets.
struct Mote {
    rise: f32,
    half: f32,
    age: i32,
    x: f32,
    z: f32,
    height: f32,
    alpha: i32,
    angles: [i32; 2],
    targets: [i32; 2],
}

/// One bobbing petal.
struct Petal {
    texture: usize,
    half: f32,
    phi: f32,
    age: i32,
    x: f32,
    z: f32,
    height: f32,
    alpha: i32,
}

struct Lope {
    at: Vector,
    start: Instant,
    done: u32,
    made: bool,
    motes: Vec<Mote>,
    petals: Vec<Petal>,
}

#[derive(Default)]
pub struct Petals {
    lopes: Vec<Lope>,
}

/// A mote's alpha at its age n: 5 a tick to 145 at 29, then 2 less a tick (0 from 102).
#[cfg(test)]
pub fn mote_alpha(n: u32) -> i32 {
    let mut alpha = 0;
    for each in 1..=n {
        alpha = if each < 30 { alpha + 5 } else { (alpha - 2).max(0) };
    }
    alpha
}

/// A petal's alpha at its age n: 2 a tick to 50 at 25, held to 70, then 1 less a tick (0 at 120).
pub fn petal_alpha(n: u32) -> i32 {
    match n {
        0 => 0,
        1..=25 => 2 * n as i32,
        26..=70 => 50,
        _ => (50 - (n as i32 - 70)).max(0),
    }
}

/// A petal's height at its age n: 4 + 1.5 sin 2n° up to 90, then 4.
pub fn petal_height(n: u32) -> f32 {
    if n <= 90 { 4.0 + 1.5 * (2.0 * n as f32).to_radians().sin() } else { 4.0 }
}

/// One wander angle toward its target (3° up, 2° down); a target passed is redrawn from `rand(n)` as the game does.
pub fn wander(angle: i32, target: i32, rand: &mut impl FnMut(u32) -> u32) -> (i32, i32) {
    if target > angle {
        let angle = (angle + 3).min(359);
        let target = if angle > target { if angle == 0 { 0 } else { rand(angle as u32) as i32 } } else { target };
        (angle, target)
    } else {
        let angle = (angle - 2).max(0);
        let target = if angle <= target { 359 - if angle == 0 { 0 } else { rand(angle as u32) as i32 } } else { target };
        (angle, target)
    }
}

impl Mote {
    fn new(rand: &mut impl FnMut(u32) -> u32) -> Self {
        let rise = 0.1 + 0.01 * rand(16) as f32;
        let half = 3.0 + 0.1 * rand(11) as f32;
        let age = -(rand(26) as i32);
        let height = rand(8) as f32;
        let theta = rand(360) as f32;
        let (a, a_target, b, b_target) = (rand(360) as i32, rand(360) as i32, rand(360) as i32, rand(360) as i32);
        let (sin, cos) = theta.to_radians().sin_cos();
        Self { rise, half, age, x: 2.5 * cos, z: 2.5 * sin, height, alpha: 0, angles: [a, b], targets: [a_target, b_target] }
    }

    fn step(&mut self, rand: &mut impl FnMut(u32) -> u32) {
        self.age += 1;
        if self.age <= 0 {
            return;
        }
        let n = self.age;
        self.alpha = if n < 30 { self.alpha + 5 } else { (self.alpha - 2).max(0) };
        for k in 0..2 {
            (self.angles[k], self.targets[k]) = wander(self.angles[k], self.targets[k], rand);
        }
        if self.height > 0.0 {
            self.x += (self.angles[0] as f32).to_radians().sin() * 0.03;
            self.z += (self.angles[1] as f32).to_radians().sin() * 0.03;
        }
        self.height += self.rise;
    }
}

impl Petal {
    fn new(texture: usize, base: f32, rand: &mut impl FnMut(u32) -> u32) -> Self {
        let half = 2.5 + 0.2 * rand(11) as f32;
        let phi = (base + rand(90) as f32) % 360.0;
        let age = -(rand(11) as i32);
        for _ in 0..4 {
            let _ = rand(360);
        }
        let (sin, cos) = phi.to_radians().sin_cos();
        Self { texture, half, phi, age, x: 4.0 * cos, z: 4.0 * sin, height: 4.0, alpha: 0 }
    }

    fn step(&mut self) {
        self.age += 1;
        if self.age <= 0 {
            return;
        }
        let n = self.age as u32;
        if n <= 90 {
            let (sin, cos) = self.phi.to_radians().sin_cos();
            self.x += 0.03 * cos;
            self.z += 0.03 * sin;
            self.height = petal_height(n);
        }
        self.alpha = petal_alpha(n);
    }
}

impl Petals {
    /// Lope's motes and petals at the spot `at`, made 35 ticks on.
    pub fn lope(&mut self, at: Vector, now: Instant) {
        self.lopes.push(Lope { at, start: now, done: 0, made: false, motes: Vec::new(), petals: Vec::new() });
    }

    /// Makes the shapes at tick 35 (their choices from `rand(n)` in the game's order), runs them a tick at a time and drops what ended.
    pub fn advance(&mut self, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        for lope in &mut self.lopes {
            let ticks = passes(lope.start, now).min(LIFE);
            if !lope.made && ticks > MADE_AT {
                lope.made = true;
                lope.done = ticks;
                lope.motes = (0..MOTES).map(|_| Mote::new(&mut rand)).collect();
                for (quarter, &texture) in PETAL_OF_QUARTER.iter().enumerate() {
                    for _ in 0..PRIMS_PER_QUARTER {
                        for _ in 0..PETALS_PER_PRIM {
                            lope.petals.push(Petal::new(texture, 90.0 * quarter as f32, &mut rand));
                        }
                    }
                }
            }
            while lope.made && lope.done < ticks {
                lope.done += 1;
                for mote in &mut lope.motes {
                    mote.step(&mut rand);
                }
                for petal in &mut lope.petals {
                    petal.step();
                }
            }
        }
        self.lopes.retain(|lope| passes(lope.start, now) <= LIFE);
    }

    /// The squares at `now`, camera-facing: the motes added on, the petals blended; each as its picture's name, place, corners,
    /// colour and whether it adds on.
    pub fn draws(&self) -> Vec<PetalDraw> {
        let mut draws = Vec::new();
        for lope in &self.lopes {
            for mote in lope.motes.iter().filter(|mote| mote.alpha > 0) {
                // Skipped while the square's lowest corner is under the ground at the spot.
                if mote.height < mote.half * std::f32::consts::FRAC_1_SQRT_2 {
                    continue;
                }
                let at = [lope.at[0] + mote.x, lope.at[1] + mote.height, lope.at[2] + mote.z];
                draws.push(PetalDraw { texture: MOTE, at, half: mote.half, alpha: mote.alpha as f32, additive: true });
            }
            for petal in lope.petals.iter().filter(|petal| petal.alpha > 0) {
                let at = [lope.at[0] + petal.x, lope.at[1] + petal.height, lope.at[2] + petal.z];
                draws.push(PetalDraw { texture: PETALS[petal.texture], at, half: petal.half, alpha: petal.alpha as f32, additive: false });
            }
        }
        draws
    }

    /// The draws as quads with their textures.
    pub fn quads<'a>(&self, texture_of: impl Fn(&str) -> Option<&'a Texture>) -> Vec<EffectQuad<'a>> {
        self.draws()
            .into_iter()
            .filter_map(|draw| {
                let side = draw.half * PIXELS_PER_UNIT;
                // ponytail: the picture stands upright; the corner angles' own uv order would turn it over.
                let corners = [225.0_f32, 315.0, 45.0, 135.0].map(|angle| {
                    let (sin, cos) = angle.to_radians().sin_cos();
                    [side * cos, side * sin]
                });
                Some((texture_of(draw.texture)?, draw.at, corners, [1.0, 1.0, 1.0, draw.alpha / 255.0], draw.additive, false))
            })
            .collect()
    }
}

/// A square to draw: its picture, place, half-diagonal, alpha (0 to 255) and whether it adds on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PetalDraw {
    pub texture: &'static str,
    pub at: Vector,
    pub half: f32,
    pub alpha: f32,
    pub additive: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prims::TICK;

    fn near(got: f32, want: f32) -> bool {
        (got - want).abs() < 1e-3
    }

    #[test]
    fn lope_makes_30_motes_and_64_petals_at_tick_35() {
        let start = Instant::now();
        let at = [10.0, 1.0, 20.0];
        let mut petals = Petals::default();
        petals.lope(at, start);
        let mut draws = Vec::new();
        petals.advance(start + TICK * 34, |_| 0);
        assert!(petals.lopes[0].motes.is_empty() && petals.draws().is_empty(), "nothing before tick 35");
        petals.advance(start + TICK * 35, |n| {
            draws.push(n);
            0
        });
        let lope = &petals.lopes[0];
        assert_eq!((lope.motes.len(), lope.petals.len()), (30, 64));
        // A mote's draws: R(16), R(11), R(26), R(8), R(360) five times; a petal's: R(11), R(90), R(11), four R(360).
        assert_eq!(draws[..9], [16, 11, 26, 8, 360, 360, 360, 360, 360]);
        assert_eq!(draws.len(), 30 * 9 + 64 * 7);
        // Every draw 0: mote rise 0.10, half-diagonal 3.0, delay 0, height 0, place (2.5, 0, 0); petal 2.5 across, φ = base, no wait.
        let mote = &lope.motes[0];
        assert!(near(mote.rise, 0.1) && near(mote.half, 3.0) && mote.age == 0 && near(mote.x, 2.5) && near(mote.z, 0.0));
        // The textures in the order peong1, peong2, peong3, peong2 for each quarter, bases 0, 90, 180, 270.
        let textures: Vec<usize> = (0..4).map(|quarter| lope.petals[16 * quarter].texture).collect();
        assert_eq!(textures, [0, 1, 2, 1]);
        assert_eq!([0, 16, 32, 48].map(|index| lope.petals[index].phi), [0.0, 90.0, 180.0, 270.0]);
        assert!(near(lope.petals[0].half, 2.5) && lope.petals[0].age == 0);
    }

    #[test]
    fn the_motes_brighten_wander_and_rise_and_the_petals_sway() {
        // Mote alpha at n = 1, 29, 30, 101, 102.
        assert_eq!([1, 29, 30, 101, 102].map(mote_alpha), [5, 145, 143, 1, 0]);
        // A mote with a delay of 3 stays at 0 until n = 1.
        let mut mote = Mote::new(&mut |n| if n == 26 { 3 } else { 0 });
        for _ in 0..3 {
            mote.step(&mut |_| 0);
        }
        assert_eq!((mote.age, mote.alpha), (0, 0));
        mote.step(&mut |_| 0);
        assert_eq!((mote.age, mote.alpha), (1, 5));
        // The wander rule: from 0 toward 5 up 3; past the target, redrawn below the angle; from 359 toward 10 down 2.
        assert_eq!(wander(0, 5, &mut |_| 0), (3, 5));
        assert_eq!(wander(3, 5, &mut |n| n - 1), (6, 5));
        assert_eq!(wander(359, 10, &mut |_| 0), (357, 10));
        assert_eq!(wander(11, 10, &mut |n| n - 1), (9, 359 - 8));
        assert_eq!(wander(1, 5, &mut |_| 0), (4, 5));
        assert_eq!(wander(0, 0, &mut |_| 0), (0, 359));
        // No sideways step while the height is 0 before the tick: the angles move but x does not (height 0, rise 0.1).
        let mut flat = Mote::new(&mut |_| 0);
        flat.angles = [90, 90];
        flat.targets = [359, 359];
        flat.step(&mut |_| 0);
        assert!(near(flat.x, 2.5) && near(flat.height, 0.1));
        flat.step(&mut |_| 0);
        assert!(flat.x > 2.5, "now it is above the ground");
        // Petal height 4.05, 5.5, 4.0, 4.0 at n = 1, 45, 90, 91; alpha 2, 50, 50, 49, 0 at 1, 25, 70, 71, 120.
        assert!(
            near(petal_height(1), 4.0 + 1.5 * 2.0_f32.to_radians().sin())
                && near(petal_height(45), 5.5)
                && near(petal_height(90), 4.0)
                && petal_height(91) == 4.0
        );
        assert_eq!([1, 25, 70, 71, 120].map(petal_alpha), [2, 50, 50, 49, 0]);
        // Radius 4 + 0.03n to n = 90 (6.7), then it stays.
        let mut petal = Petal::new(0, 0.0, &mut |_| 0);
        for _ in 0..95 {
            petal.step();
        }
        assert!(near(petal.x, 6.7) && near(petal.height, 4.0));
    }

    #[test]
    fn the_squares_are_additive_motes_and_blended_petals_that_stay_put() {
        let start = Instant::now();
        let mut petals = Petals::default();
        petals.lope([10.0, 1.0, 20.0], start);
        for tick in 0..=80 {
            petals.advance(start + TICK * tick, |_| 0);
        }
        let draws = petals.draws();
        assert!(draws.iter().any(|draw| draw.texture == MOTE && draw.additive) && draws.iter().any(|draw| draw.texture == PETALS[0] && !draw.additive));
        // A mote is skipped while its lowest corner is under the ground; the petals are not (they are 4 up).
        assert!(draws.iter().filter(|draw| draw.texture == MOTE).all(|draw| draw.at[1] - 1.0 >= draw.half * std::f32::consts::FRAC_1_SQRT_2 - 1e-3));
        assert!(draws.iter().filter(|draw| draw.texture != MOTE).all(|draw| draw.at[1] > 4.9 && draw.half >= 2.5));
        // Nothing follows: the squares stand round the spot whoever moves; it ends at tick 300.
        assert!(draws.iter().all(|draw| (draw.at[0] - 10.0).abs() < 12.0));
        petals.advance(start + TICK * 301, |_| 0);
        assert!(petals.lopes.is_empty());
    }
}
