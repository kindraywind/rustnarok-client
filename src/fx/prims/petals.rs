//! Lope's 30 glowing motes and 64 petals (411) and Kouenka's 20 floating petals (618), a 16 ms tick at a time.
//!
//! A shape's own age n counts from 1 on its first tick after its wait; e is the ticks since the effect started.

use std::time::Instant;

use super::crowds::CrowdDraw;
use super::passes;
use crate::render::Texture;
use crate::world::{EffectQuad, Vector};

pub const MOTE: &str = super::spray::DOT;
/// The petal pictures, one prim of four petals for each of 16 prims: peong1, peong2, peong3, peong2 for each quarter.
pub const PETALS: [&str; 3] = ["data\\texture\\effect\\peong1.tga", "data\\texture\\effect\\peong2.tga", "data\\texture\\effect\\peong3.tga"];
const PETAL_OF_QUARTER: [usize; 4] = [0, 1, 2, 1];
/// Kouenka's petal sprite: 3 actions of 10 frames.
pub const SAKURA: &str = "data\\sprite\\이팩트\\sakura01";
const PIXELS_PER_UNIT: f32 = 7.0;
/// Kouenka lasts 300 ticks and makes a petal on each of the first 20.
const KOUENKA_LIFE: u32 = 300;
const KOUENKA_PETALS: u32 = 20;
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

/// One of Kouenka's floating petals.
struct Fall {
    /// The tick of the effect it was made on.
    made: u32,
    rise: f32,
    scale: f32,
    at: Vector,
    action: usize,
    age: u32,
    alpha: f32,
    /// The constant step the effect's own end adds to the alpha, taken on the first tick of a petal made after the first.
    step: f32,
    angles: [i32; 2],
    targets: [i32; 2],
}

struct Kouenka {
    unit: Option<u32>,
    start: Instant,
    done: u32,
    last: Vector,
    falls: Vec<Fall>,
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
    kouenkas: Vec<Kouenka>,
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

impl Fall {
    /// A petal made on the tick `made` round `feet`, its choices from `rand(n)` in the game's order.
    fn new(made: u32, feet: Vector, rand: &mut impl FnMut(u32) -> u32) -> Self {
        let rise = 0.15 * (2 + rand(3)) as f32;
        let scale = (22 + rand(6)) as f32 / 70.0;
        let (sin, cos) = (rand(360) as f32).to_radians().sin_cos();
        let (c0, t0, c1, t1) = (rand(360) as i32, rand(360) as i32, rand(360) as i32, rand(360) as i32);
        let action = rand(3) as usize;
        Self {
            made,
            rise,
            scale,
            at: [feet[0] + 8.0 * cos, feet[1], feet[2] + 8.0 * sin],
            action,
            age: 0,
            alpha: 250.0,
            step: 0.0,
            angles: [c0, c1],
            targets: [t0, t1],
        }
    }

    fn step(&mut self, rand: &mut impl FnMut(u32) -> u32) {
        self.age += 1;
        for k in 0..2 {
            (self.angles[k], self.targets[k]) = wander(self.angles[k], self.targets[k], rand);
        }
        self.at[1] += self.rise;
        self.alpha = (self.alpha - 3.0).max(0.0);
        self.at[0] += 0.15 * (self.angles[0] as f32).to_radians().sin();
        self.at[2] += 0.15 * (self.angles[1] as f32).to_radians().sin();
        // A petal made after the first fades out with the effect: its alpha runs down to nothing by the effect's last tick.
        if self.made >= 1 {
            if self.age == 1 {
                self.step = -self.alpha / (KOUENKA_LIFE - 1 - self.made) as f32;
            }
            self.alpha = (self.alpha + self.step).clamp(0.0, 254.0);
        }
    }
}

impl Petals {
    /// Kouenka's petals round `unit`, whose feet are at `at` when it cannot be found.
    pub fn kouenka(&mut self, unit: Option<u32>, at: Vector, now: Instant) {
        self.kouenkas.push(Kouenka { unit, start: now, done: 0, last: at, falls: Vec::new() });
    }

    /// Makes Kouenka's petals (one a tick on the first 20, round the target's feet then) and runs them a tick at a time.
    pub fn drift(&mut self, now: Instant, position_of: impl Fn(Option<u32>) -> Option<Vector>, mut rand: impl FnMut(u32) -> u32) {
        for kouenka in &mut self.kouenkas {
            kouenka.last = position_of(kouenka.unit).unwrap_or(kouenka.last);
            while kouenka.done < passes(kouenka.start, now).min(KOUENKA_LIFE) {
                let e = kouenka.done;
                kouenka.done += 1;
                if e < KOUENKA_PETALS {
                    kouenka.falls.push(Fall::new(e, kouenka.last, &mut rand));
                }
                for fall in &mut kouenka.falls {
                    fall.step(&mut rand);
                }
            }
        }
        self.kouenkas.retain(|kouenka| passes(kouenka.start, now) <= KOUENKA_LIFE);
    }

    /// Kouenka's petals as sprites: the frame steps every 4 ticks and loops.
    pub fn falls(&self) -> Vec<CrowdDraw> {
        let mut draws = Vec::new();
        for kouenka in &self.kouenkas {
            for fall in kouenka.falls.iter().filter(|fall| fall.alpha >= 1.0) {
                draws.push(CrowdDraw {
                    sprite: SAKURA,
                    at: fall.at,
                    scale: fall.scale,
                    action: fall.action,
                    steps: (fall.age - 1) / 4 + 1,
                    alpha: fall.alpha,
                    tint: [1.0; 3],
                });
            }
        }
        draws
    }

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

    #[test]
    fn kouenka_makes_a_petal_a_tick_for_20_ticks_with_the_games_draws() {
        let start = Instant::now();
        let feet = [10.0, 1.0, 20.0];
        let mut petals = Petals::default();
        petals.kouenka(Some(1), feet, start);
        let mut draws = Vec::new();
        petals.drift(
            start,
            |_| Some(feet),
            |n| {
                draws.push(n);
                0
            },
        );
        // One petal on e = 0: R(3), R(6), R(360) five times, R(3); the tick's own wander draws follow.
        assert_eq!(draws[..8], [3, 6, 360, 360, 360, 360, 360, 3]);
        let fall = &petals.kouenkas[0].falls[0];
        assert_eq!(petals.kouenkas[0].falls.len(), 1);
        // Every draw 0: rise 0.30, scale 22/70, x +8 (cos 0), z 0, action 0; after its first tick it has risen once.
        assert!(near(fall.rise, 0.30) && near(fall.scale, 22.0 / 70.0) && fall.action == 0);
        assert!(near(fall.at[0], 18.0) && near(fall.at[2], 20.0) && near(fall.at[1], 1.0 + 0.30));
        // The largest draws: rise 0.60, scale 27/70.
        let big = Fall::new(0, feet, &mut |n| n - 1);
        assert!(near(big.rise, 0.60) && near(big.scale, 27.0 / 70.0) && big.action == 2);
        // 20 petals on e = 0 … 19 and none after.
        petals.drift(start + TICK * 19, |_| Some(feet), |_| 0);
        assert_eq!(petals.kouenkas[0].falls.len(), 20);
        petals.drift(start + TICK * 40, |_| Some(feet), |_| 0);
        assert_eq!(petals.kouenkas[0].falls.len(), 20);
    }

    #[test]
    fn a_kouenka_petal_fades_three_a_tick_and_ends_with_the_effect() {
        let mut first = Fall::new(0, [0.0; 3], &mut |_| 0);
        let mut alphas = Vec::new();
        for _ in 0..84 {
            first.step(&mut |_| 0);
            alphas.push(first.alpha);
        }
        // n = 1, 83, 84: 247, 1, 0.
        assert_eq!([alphas[0], alphas[82], alphas[83]], [247.0, 1.0, 0.0]);
        // A petal made on e = 10 adds 247 / 289 less each tick from its first, and is gone by n = 65.
        let mut late = Fall::new(10, [0.0; 3], &mut |_| 0);
        late.step(&mut |_| 0);
        assert!(near(late.alpha, 247.0 - 247.0 / 289.0) && near(late.step, -247.0 / 289.0));
        for _ in 1..64 {
            late.step(&mut |_| 0);
        }
        assert!(late.alpha > 0.0);
        late.step(&mut |_| 0);
        assert_eq!((late.age, late.alpha), (65, 0.0));
        // The frames: 1, 1, 2, 0, 1 at n = 1, 4, 5, 40, 41.
        let frame = |n: u32| ((n - 1) / 4 + 1) % 10;
        assert_eq!([1, 4, 5, 40, 41].map(frame), [1, 1, 2, 0, 1]);
    }

    #[test]
    fn a_kouenka_petal_wobbles_rises_and_stays_where_the_target_was() {
        let start = Instant::now();
        let mut petals = Petals::default();
        petals.kouenka(Some(1), [0.0; 3], start);
        petals.drift(start + TICK * 2, |_| Some([100.0, 0.0, 100.0]), |_| 0);
        // The petal of e = 0 was made at the feet (100, 100) and rises 0.30 a tick for 3 ticks; its drawing is the sakura sprite.
        let draws = petals.falls();
        assert_eq!(draws.len(), 3);
        assert!(draws.iter().all(|draw| draw.sprite == SAKURA && draw.tint == [1.0; 3]));
        // The sideways wobble is at most 0.15 a tick.
        assert!(near(draws[0].at[1], 0.30 * 3.0) && (draws[0].at[0] - 108.0).abs() < 0.05 && (draws[0].at[2] - 100.0).abs() < 0.05);
        petals.drift(start + TICK * 5, |_| Some([0.0; 3]), |_| 0);
        assert!((petals.falls()[0].at[0] - 108.0).abs() < 0.2, "it does not follow the unit");
        petals.drift(start + TICK * 301, |_| None, |_| 0);
        assert!(petals.falls().is_empty() && petals.kouenkas.is_empty());
    }
}
