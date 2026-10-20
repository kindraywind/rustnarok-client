//! Rising sparkles (the game's prim kind 59): camera-facing squares that rise and fade, with twinkles.
//! Effect 793 (EF_HALLUTIME) round an Escape copy's feet.

use std::time::Instant;

use super::TICK;
use crate::render::Texture;
use crate::world::{EffectQuad, Vector};

pub const POK1: &str = "data\\texture\\effect\\pok1.tga";
pub const POK3: &str = "data\\texture\\effect\\pok3.tga";
pub const TEXTURES: [&str; 2] = [POK1, POK3];

pub const EF_HALLUTIME: u32 = 793;
pub const LIFE: u32 = 100;
pub const WAVES: usize = 6;
pub const SQUARES_PER_WAVE: usize = 4;
pub const TOTAL_SQUARES: usize = WAVES * SQUARES_PER_WAVE;

const PIXELS_PER_UNIT: f32 = 7.0;

/// Alpha (0..=135) of a square at age `n` ticks (1 on its first tick).
pub fn alpha_at(n: u32) -> u32 {
    match n {
        0 | 1 => 0,
        2..=10 => (n - 1) * 15,
        11..=54 => 135 - (n - 10) * 3,
        _ => 0,
    }
}

/// Drawn alpha (0..=67, or 0..=50 on ecl_fild01) of a square at age `n`.
pub fn drawn_alpha(n: u32, dim: bool) -> u32 {
    let a = alpha_at(n);
    let capped = if dim { a.min(100) } else { a };
    capped / 2
}

/// Spin in whole degrees (0..359) at age `n` ticks.
pub fn spin_at(n: u32) -> u32 {
    (360 - (5 * n as i32).rem_euclid(360)) as u32 % 360
}

/// Texture for square `k` (0..=3) at age `n`.
pub fn texture_at(k: usize, n: u32) -> &'static str {
    let target = 36 - 7 * k as i32;
    let age = n as i32;
    if age == target - 1 || age == target { POK3 } else { POK1 }
}

/// Corners of a camera-facing square of half-diagonal `r` at spin `spin` in sprite pixels (y down).
pub fn corners(r: f32, spin: u32) -> [[f32; 2]; 4] {
    std::array::from_fn(|j| {
        let angle = (spin + 90 * j as u32) % 360;
        let (sin, cos) = (angle as f32).to_radians().sin_cos();
        [r * PIXELS_PER_UNIT * cos, r * PIXELS_PER_UNIT * sin]
    })
}

/// One square of kind 59 round a copy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Square {
    pub k: usize,
    pub born: u32,
    pub half_diagonal: f32,
    pub dx: f32,
    pub dz: f32,
    pub rise: f32,
}

impl Square {
    pub fn new(k: usize, born: u32, mut rand: impl FnMut(u32) -> u32) -> Self {
        let half_diagonal = 1.5 + 0.1 * rand(10) as f32;
        let dx = rand(7) as f32 - 3.0;
        let dz = rand(7) as f32 - 3.0;
        let rise = 0.20 + 0.01 * rand(20) as f32;
        Self { k, born, half_diagonal, dx, dz, rise }
    }
}

/// Effect 793 (EF_HALLUTIME): white glints rising round a copy.
pub struct Hallutime {
    pub unit: Option<u32>,
    pub at: Vector,
    pub start: Instant,
    pub squares: [Square; TOTAL_SQUARES],
}

impl Hallutime {
    pub fn new(unit: Option<u32>, at: Vector, start: Instant, mut rand: impl FnMut(u32) -> u32) -> Self {
        let mut squares = Vec::with_capacity(TOTAL_SQUARES);
        for w in 0..WAVES {
            let born = (w * 4) as u32;
            for k in 0..SQUARES_PER_WAVE {
                squares.push(Square::new(k, born, &mut rand));
            }
        }
        Self { unit, at, start, squares: squares.try_into().unwrap() }
    }

    /// Ticks since the effect started (16 ms ticks).
    pub fn tick(&self, now: Instant) -> Option<u32> {
        let age = now.checked_duration_since(self.start)?;
        Some((age.as_millis() / TICK.as_millis()) as u32)
    }

    /// Whether the effect is still running (ends at 100 ticks).
    pub fn alive(&self, now: Instant) -> bool {
        self.tick(now).is_some_and(|tick| tick < LIFE)
    }

    /// Returns the squares that have been born by effect tick `e`.
    pub fn squares_born_by(&self, e: u32) -> &[Square] {
        let count = match e {
            0..=3 => 4,
            4..=7 => 8,
            8..=11 => 12,
            12..=15 => 16,
            16..=19 => 20,
            _ => 24,
        };
        &self.squares[..count]
    }

    /// Draws the camera-facing squares for this effect at `now`.
    pub fn quads<'a>(
        &self,
        now: Instant,
        dim: bool,
        texture_of: impl Fn(&str) -> Option<&'a Texture>,
        position_of: &impl Fn(Option<u32>) -> Option<Vector>,
    ) -> Vec<EffectQuad<'a>> {
        let Some(e) = self.tick(now).filter(|&t| t < LIFE) else { return Vec::new() };
        let base = match self.unit {
            Some(unit) => match position_of(Some(unit)) {
                Some(pos) => pos,
                None => return Vec::new(),
            },
            None => position_of(None).unwrap_or(self.at),
        };
        let mut quads = Vec::new();
        for square in self.squares_born_by(e) {
            let n = e - square.born;
            if !(2..=54).contains(&n) {
                continue;
            }
            let alpha = drawn_alpha(n, dim);
            if alpha == 0 {
                continue;
            }
            let Some(texture) = texture_of(texture_at(square.k, n)) else { continue };
            let quad_corners = corners(square.half_diagonal, spin_at(n));
            let place = [base[0] + square.dx, base[1] + 6.0 + n as f32 * square.rise, base[2] + square.dz];
            quads.push((texture, place, quad_corners, [1.0, 1.0, 1.0, alpha as f32 / 255.0], true, false));
        }
        quads
    }
}

/// Hovering's and Emergency Cool's (EF_HATED 543) blue sparkles: a set of four squares on each of ticks 0, 4 … 80, each rising from
/// where the unit stood, in #9696FA.
pub struct Blues {
    pub unit: Option<u32>,
    pub start: Instant,
    /// The sets made so far, and where the unit last stood.
    pub made: u32,
    last: Option<Vector>,
    pub squares: Vec<Rise>,
}

pub const BLUE_SETS: u32 = 21;
const BLUE_TINT: [f32; 3] = [150.0 / 255.0, 150.0 / 255.0, 250.0 / 255.0];

/// One blue square: where it starts (6 up), its size and how fast it rises.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rise {
    pub k: usize,
    pub born: u32,
    pub half_diagonal: f32,
    pub at: Vector,
    pub rise: f32,
}

impl Blues {
    pub fn new(unit: Option<u32>, start: Instant) -> Self {
        Self { unit, start, made: 0, last: None, squares: Vec::new() }
    }

    fn tick(&self, now: Instant) -> Option<u32> {
        let age = now.checked_duration_since(self.start)?;
        Some((age.as_millis() / TICK.as_millis()) as u32)
    }

    /// Whether anything of it is still to show: the last set is gone 54 ticks after tick 80.
    pub fn alive(&self, now: Instant) -> bool {
        self.tick(now).is_some_and(|tick| tick < 4 * (BLUE_SETS - 1) + 55)
    }

    /// Makes the sets whose ticks have come, where `position_of` has the unit's feet then; each square's choices come from `rand(n)`.
    pub fn advance(&mut self, now: Instant, position_of: &impl Fn(Option<u32>) -> Option<Vector>, mut rand: impl FnMut(u32) -> u32) {
        self.last = position_of(self.unit).or(self.last);
        let Some(tick) = self.tick(now) else { return };
        while self.made < BLUE_SETS && tick >= 4 * self.made {
            if let Some(feet) = self.last {
                for k in 0..SQUARES_PER_WAVE {
                    let half_diagonal = 2.0 + 0.1 * rand(10) as f32;
                    let x = feet[0] + rand(11) as f32 - 5.0;
                    let z = feet[2] + rand(11) as f32 - 5.0;
                    let rise = 0.10 + 0.01 * rand(20) as f32;
                    // The game's one more draw, at the square's first tick, is thrown away.
                    let _ = rand(360);
                    self.squares.push(Rise { k, born: 4 * self.made, half_diagonal, at: [x, feet[1] + 6.0, z], rise });
                }
            }
            self.made += 1;
        }
    }

    /// The camera-facing squares at `now`, added on and not following the unit.
    pub fn quads<'a>(&self, now: Instant, dim: bool, texture_of: impl Fn(&str) -> Option<&'a Texture>) -> Vec<EffectQuad<'a>> {
        let Some(tick) = self.tick(now) else { return Vec::new() };
        let mut quads = Vec::new();
        for square in &self.squares {
            let n = tick.saturating_sub(square.born);
            if tick < square.born || !(2..=54).contains(&n) {
                continue;
            }
            let alpha = drawn_alpha(n, dim);
            let Some(texture) = texture_of(texture_at(square.k, n)) else { continue };
            let place = [square.at[0], square.at[1] + n as f32 * square.rise, square.at[2]];
            let tint = [BLUE_TINT[0], BLUE_TINT[1], BLUE_TINT[2], alpha as f32 / 255.0];
            quads.push((texture, place, corners(square.half_diagonal, spin_at(n)), tint, true, false));
        }
        quads
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wave_tick(e: u32) -> bool {
        e <= 20 && e.is_multiple_of(4)
    }

    #[derive(Clone, Copy, Debug, PartialEq)]
    struct SteppedSquare {
        k: usize,
        half_diagonal: f32,
        rise: f32,
        x: f32,
        y: f32,
        z: f32,
        spin: u32,
        age: u32,
    }

    impl SteppedSquare {
        fn from_square(square: &Square, copy: Vector) -> Self {
            Self {
                k: square.k,
                half_diagonal: square.half_diagonal,
                rise: square.rise,
                x: copy[0] + square.dx,
                y: copy[1] + 6.0,
                z: copy[2] + square.dz,
                spin: 0,
                age: 0,
            }
        }

        fn step(&mut self) {
            self.age += 1;
            self.spin = (self.spin as i32 - 5).rem_euclid(360) as u32;
            self.y += self.rise;
        }

        fn alpha(&self) -> u32 {
            alpha_at(self.age)
        }

        fn texture(&self) -> &'static str {
            texture_at(self.k, self.age)
        }

        fn corners(&self) -> [[f32; 2]; 4] {
            corners(self.half_diagonal, self.spin)
        }
    }

    #[test]
    fn sets_of_squares_and_timing() {
        let start = Instant::now();
        let hallutime = Hallutime::new(None, [10.0, 0.0, 20.0], start, |_| 0);
        // Sets of 4 squares on e = 0, 4, 8, 12, 16, 20 (24 in all) and none on e = 1 … 3 or 24.
        assert_eq!(hallutime.squares_born_by(0).len(), 4);
        assert_eq!(hallutime.squares_born_by(1).len(), 4);
        assert_eq!(hallutime.squares_born_by(2).len(), 4);
        assert_eq!(hallutime.squares_born_by(3).len(), 4);
        assert_eq!(hallutime.squares_born_by(4).len(), 8);
        assert_eq!(hallutime.squares_born_by(7).len(), 8);
        assert_eq!(hallutime.squares_born_by(8).len(), 12);
        assert_eq!(hallutime.squares_born_by(12).len(), 16);
        assert_eq!(hallutime.squares_born_by(16).len(), 20);
        assert_eq!(hallutime.squares_born_by(20).len(), 24);
        assert_eq!(hallutime.squares_born_by(24).len(), 24);
        assert_eq!(hallutime.squares.len(), 24);
        assert_eq!((0..=24).filter(|&e| wave_tick(e)).collect::<Vec<_>>(), [0, 4, 8, 12, 16, 20]);
    }

    #[test]
    fn fixed_draws_and_draw_order() {
        let copy = [10.0, 5.0, 20.0];
        let start = Instant::now();
        // With every draw 0: half-diagonal 1.5, x and z = the copy's − 3, rise 0.20, y 6 up.
        let hallutime = Hallutime::new(None, copy, start, |_| 0);
        let square = hallutime.squares[0];
        let stepped = SteppedSquare::from_square(&square, copy);
        assert_eq!(square.half_diagonal, 1.5);
        assert_eq!(stepped.x, copy[0] - 3.0);
        assert_eq!(stepped.z, copy[2] - 3.0);
        assert_eq!(square.rise, 0.20);
        assert_eq!(stepped.y, copy[1] + 6.0);

        // With the largest draws (9, 6, 6, 19): 2.4, + 3, +3, 0.39.
        let mut largest_draws = [9, 6, 6, 19].into_iter();
        let max_square = Square::new(0, 0, |_| largest_draws.next().unwrap());
        let max_stepped = SteppedSquare::from_square(&max_square, copy);
        assert_eq!(max_square.half_diagonal, 2.4);
        assert_eq!(max_stepped.x, copy[0] + 3.0);
        assert_eq!(max_stepped.z, copy[2] + 3.0);
        assert_eq!(max_square.rise, 0.39);

        // The draw order R(10), R(7), R(7), R(20) per square, square after square.
        let mut limits = Vec::new();
        let _ = Hallutime::new(None, copy, start, |limit| {
            limits.push(limit);
            0
        });
        assert_eq!(limits.len(), 24 * 4);
        for chunk in limits.chunks(4) {
            assert_eq!(chunk, [10, 7, 7, 20]);
        }
    }

    #[test]
    fn alpha_drawn_alpha_y_and_spin() {
        // Alpha at n = 1, 2, 10, 11, 54, 55 (0, 15, 135, 132, 3, 0).
        assert_eq!([1, 2, 10, 11, 54, 55].map(alpha_at), [0, 15, 135, 132, 3, 0]);

        // The drawn alpha at n = 10 is 67, and 50 on ecl_fild01.
        assert_eq!(drawn_alpha(10, false), 67);
        assert_eq!(drawn_alpha(10, true), 50);

        // y at n = 55 is 6 + 55·rise.
        let copy = [0.0, 0.0, 0.0];
        let square = Square::new(0, 0, |_| 0);
        let mut stepped = SteppedSquare::from_square(&square, copy);
        for _ in 0..55 {
            stepped.step();
        }
        assert_eq!(stepped.age, 55);
        assert!((stepped.y - (6.0 + 55.0 * square.rise)).abs() < 1e-4);

        // The spin at n = 1, 2, 72, 73 (355, 350, 0, 355).
        assert_eq!([1, 2, 72, 73].map(spin_at), [355, 350, 0, 355]);
        let mut spin_check = SteppedSquare::from_square(&square, copy);
        let mut spins = Vec::new();
        for n in 1..=73 {
            spin_check.step();
            if [1, 2, 72, 73].contains(&n) {
                spins.push(spin_check.spin);
            }
        }
        assert_eq!(spins, [355, 350, 0, 355]);
        assert_eq!(stepped.alpha(), alpha_at(55));
        assert_eq!(stepped.corners(), corners(stepped.half_diagonal, stepped.spin));

        // Drawn at n = 2 … 54 only.
        for n in 0..100 {
            assert_eq!(alpha_at(n) > 0, (2..=54).contains(&n), "failed at age {n}");
        }
    }

    #[test]
    fn pok3_twinkle_ages() {
        // The pok3 ages: k = 0: 35, 36; k = 1: 28, 29; k = 2: 21, 22; k = 3: 14, 15, and pok1 at every other age.
        for (k, target_ages) in [(0, [35, 36]), (1, [28, 29]), (2, [21, 22]), (3, [14, 15])] {
            for n in 0..=60 {
                let expected = if target_ages.contains(&n) { POK3 } else { POK1 };
                assert_eq!(texture_at(k, n), expected, "failed for k={k}, n={n}");
                let stepped = SteppedSquare { k, half_diagonal: 1.5, rise: 0.2, x: 0.0, y: 6.0, z: 0.0, spin: 0, age: n };
                assert_eq!(stepped.texture(), expected);
            }
        }
    }

    #[test]
    fn follows_copy_ends_when_gone_and_lifetime() {
        let start = Instant::now();
        let copy = [15.0, 2.0, 30.0];
        let hallutime = Hallutime::new(Some(42), copy, start, |_| 0);

        // Life 100 ticks (alive at 99, dead at 100).
        assert!(hallutime.alive(start));
        assert!(hallutime.alive(start + TICK * 99));
        assert!(!hallutime.alive(start + TICK * 100));

        // When copy is gone (position_of returns None), quads returns empty.
        let quads_dead = hallutime.quads(start + TICK * 2, false, |_| None, &|_| None);
        assert!(quads_dead.is_empty());

        // Effects.txt has length 793 100 and no wav row.
        let effects_txt = include_str!("../effects.txt");
        assert!(effects_txt.lines().any(|line| line == "length\t793\t100"));
        assert!(!effects_txt.lines().any(|line| line.starts_with("wav\t793")));
    }

    #[test]
    fn hovering_lets_blue_sparkles_rise_in_21_sets() {
        let start = Instant::now();
        let feet = [10.0, 5.0, 20.0];
        let mut blues = Blues::new(Some(1), start);
        let run = |blues: &mut Blues, tick: u32| blues.advance(start + TICK * tick, &|_| Some(feet), |_| 0);
        for tick in 0..=82 {
            run(&mut blues, tick);
            // A set of four on ticks 0, 4 … 80 and none between.
            assert_eq!(blues.squares.len() as u32, 4 * (tick / 4 + 1).min(BLUE_SETS), "tick {tick}");
        }
        assert_eq!(blues.made, 21);
        // Every draw 0: half-diagonal 2.0, x and z the feet − 5, rise 0.10, 6 up; absolute: the unit moving changes nothing.
        let first = blues.squares[0];
        assert_eq!((first.half_diagonal, first.at, first.rise), (2.0, [5.0, 11.0, 15.0], 0.10));
        // Alpha at ages 2, 10, 11, 54, 55: 15, 135, 132, 3, 0; the twinkle at ages 35 and 36 for the first square.
        assert_eq!([2, 10, 11, 54, 55].map(alpha_at), [15, 135, 132, 3, 0]);
        assert_eq!([texture_at(0, 34), texture_at(0, 35), texture_at(0, 36), texture_at(0, 37)], [POK1, POK3, POK3, POK1]);
        assert!(blues.alive(start + TICK * 134) && !blues.alive(start + TICK * 135));
    }
}
