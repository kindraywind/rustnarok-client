//! Cold Slower's flying snowflakes (791, kind 134) and Stasis's electric rings (799, kind 88),
//! a 16 ms tick at a time.

use std::time::Instant;

use crate::prims::{tanji::bearing, walls::Wall};
use crate::render::Texture;
use crate::world::{EffectQuad, MeshVertex, Vector};

pub const PIXELS_PER_UNIT: f32 = 7.0;

pub const SNOW_TEXTURE: &str = "data\\texture\\effect\\E9_snow03.tga";

pub const THUNDER_BALLS: [&str; 6] = [
    "data\\texture\\effect\\thunder_ball_0001.bmp",
    "data\\texture\\effect\\thunder_ball_0002.bmp",
    "data\\texture\\effect\\thunder_ball_0003.bmp",
    "data\\texture\\effect\\thunder_ball_0004.bmp",
    "data\\texture\\effect\\thunder_ball_0005.bmp",
    "data\\texture\\effect\\thunder_ball_0006.bmp",
];

pub const FLAKE_COLORS: [[f32; 3]; 4] = [
    [55.0 / 255.0, 55.0 / 255.0, 1.0],   // 0 -> #3737FF
    [105.0 / 255.0, 105.0 / 255.0, 1.0], // 1 -> #6969FF
    [75.0 / 255.0, 75.0 / 255.0, 1.0],   // 2 -> #4B4BFF
    [85.0 / 255.0, 85.0 / 255.0, 1.0],   // 3 -> #5555FF
];

pub const STASIS_RING_COLOR: [f32; 3] = [55.0 / 255.0, 55.0 / 255.0, 1.0]; // #3737FF

/// What a flash left on a body fades back from, by 8 a channel a tick.
fn fade_back(from: [u8; 3], ticks: u32) -> Option<[u8; 3]> {
    let drift = (ticks * 8) as u16;
    let tint = from.map(|channel| (u16::from(channel) + drift).min(255) as u8);
    (tint != [255; 3]).then_some(tint)
}

/// The corners of a camera-facing square of half-diagonal `r` at spin `spin` in the camera's plane.
/// Corners c₀ … c₃ are at spin, spin − 90, spin − 180, spin − 270 (each wrapped into 0 … 359).
/// In `EffectQuad` order: uv (0, 0) at c₂, (1, 0) at c₃, (1, 1) at c₀, (0, 1) at c₁.
pub fn flake_corners(r: f32, spin: f32) -> [[f32; 2]; 4] {
    let corner = |angle: f32| {
        let (sin, cos) = angle.rem_euclid(360.0).to_radians().sin_cos();
        [r * PIXELS_PER_UNIT * cos, r * PIXELS_PER_UNIT * sin]
    };
    let c0 = corner(spin);
    let c1 = corner(spin - 90.0);
    let c2 = corner(spin - 180.0);
    let c3 = corner(spin - 270.0);
    [c2, c3, c0, c1]
}

/// One flying snowflake of kind 134: size, spin, heading in degrees, span T, age n, and alpha.
#[derive(Clone, Debug)]
pub struct Flake {
    pub size: f32,
    pub spin: f32,
    pub heading: i32,
    pub span: u32,
    pub age: u32,
    pub alpha: i32,
}

impl Flake {
    pub fn new(size: f32, spin: f32, heading: i32, span: u32) -> Self {
        Self { size, spin, heading, span, age: 0, alpha: 0 }
    }

    pub fn step(&mut self) {
        self.age += 1;
        self.spin = (self.spin + 1.0) % 360.0;
        let n = self.age;
        if n < 10 {
            self.alpha = (self.alpha + 25).min(255);
        } else if n > 40 {
            self.alpha = (self.alpha - 5).max(0);
        }
    }
}

/// One prim of four flakes sharing a colour.
#[derive(Clone, Debug)]
pub struct FlakePrim {
    pub color: [f32; 3],
    pub flakes: Vec<Flake>,
}

/// One Cold Slower effect (791): caster unit, place P at e = 0, aim A, distance D, and 3 prims of 4 flakes.
#[derive(Clone, Debug)]
pub struct ColdSlower {
    pub start: Instant,
    pub unit: Option<u32>,
    pub place: Vector,
    #[allow(dead_code)]
    pub aim: Vector,
    pub distance: f32,
    pub ticks: u32,
    pub prims: Vec<FlakePrim>,
}

impl ColdSlower {
    pub fn step(&mut self) {
        for prim in &mut self.prims {
            for flake in &mut prim.flakes {
                flake.step();
            }
        }
    }

    /// The body flags and colour of the caster's flicker at tick `e`.
    pub fn glow(&self, e: u32) -> Option<(u32, Option<[u8; 3]>)> {
        use crate::actor::{GLOW_ADDITIVE, GLOW_OUTLINE};
        if !(6..=40).contains(&e) {
            return None;
        }
        if e.is_multiple_of(2) { Some((GLOW_OUTLINE | GLOW_ADDITIVE, Some([0, 0, 0xFF]))) } else { Some((GLOW_OUTLINE, Some([0xFF, 0xFF, 0xFF]))) }
    }

    /// The colour left on the caster fading back to white after e = 40.
    pub fn body_tint(&self, e: u32) -> Option<[u8; 3]> {
        if e > 40 { fade_back([0, 0, 0xFF], e - 40) } else { None }
    }
}

/// One of Stasis's six electric rings (kind 88): age, size, alpha, height over feet, and texture index k (1..=6).
#[derive(Clone, Debug)]
pub struct StasisRing {
    pub k: usize,
    pub age: i32,
    pub size: f32,
    pub alpha: f32,
    pub height: f32,
}

impl StasisRing {
    pub fn step(&mut self) {
        self.age += 1;
        let n = self.age;
        if n <= 0 {
            return;
        }
        self.size *= 1.07;
        if n <= 10 {
            self.alpha = (self.alpha + 2.0).min(20.0);
        } else {
            self.alpha = (self.alpha - 1.0).max(0.0);
        }
    }

    /// The flat diamond's radius R = 0.9 × size at the ring's current age.
    pub fn radius(&self) -> f32 {
        0.9 * self.size
    }
}

/// One Stasis effect (799): following `unit`, with six rings and a flicker on ticks 6..=40.
#[derive(Clone, Debug)]
pub struct Stasis {
    pub start: Instant,
    pub unit: Option<u32>,
    pub ticks: u32,
    pub rings: Vec<StasisRing>,
}

impl Stasis {
    pub fn step(&mut self) {
        for ring in &mut self.rings {
            ring.step();
        }
    }

    /// The body flags and colour of the caster's flicker at tick `e`.
    pub fn glow(&self, e: u32) -> Option<(u32, Option<[u8; 3]>)> {
        use crate::actor::GLOW_ADDITIVE;
        if !(6..=40).contains(&e) {
            return None;
        }
        if e.is_multiple_of(2) { Some((GLOW_ADDITIVE, Some([0, 0, 0xFF]))) } else { Some((0, Some([0xFF, 0xFF, 0xFF]))) }
    }

    /// The colour left on the unit fading back to white after e = 40.
    pub fn body_tint(&self, e: u32) -> Option<[u8; 3]> {
        if e > 40 { fade_back([0, 0, 0xFF], e - 40) } else { None }
    }
}

#[derive(Default)]
pub struct Snow {
    pub cold_slowers: Vec<ColdSlower>,
    pub stases: Vec<Stasis>,
}

impl Snow {
    /// Launches Cold Slower (791) on `unit`: three prims of four flakes thrown from `feet` toward `aim`.
    pub fn cold_slower(&mut self, unit: Option<u32>, feet: Vector, aim: Vector, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        let distance = f32::hypot(feet[0] - aim[0], feet[2] - aim[2]);
        let mut prims = Vec::with_capacity(3);
        let b = bearing(feet[0] - aim[0], feet[2] - aim[2]);
        for _ in 0..3 {
            let mut flakes = Vec::with_capacity(4);
            let mut color = FLAKE_COLORS[0];
            for k in 0..4 {
                let r_draw = rand(21);
                let size = 2.5 + 0.08 * r_draw as f32;
                let m_draw = rand(4);
                if k == 0 {
                    color = FLAKE_COLORS[m_draw.min(3) as usize];
                }
                let spin = rand(360) as f32;
                let mut h = b + 160 + rand(41) as i32;
                if h >= 360 {
                    h -= 360;
                }
                let _unused = rand(360);
                let span = 60 - rand(21);
                flakes.push(Flake::new(size, spin, h, span));
            }
            prims.push(FlakePrim { color, flakes });
        }
        let mut slower = ColdSlower { start: now, unit, place: feet, aim, distance, ticks: 1, prims };
        // Made and stepped on the same tick (n = 1 on the first).
        slower.step();
        self.cold_slowers.push(slower);
    }

    /// Starts Stasis (799) on `unit`: six rings spreading flat round feet with heights from 5 to 10.
    pub fn stasis(&mut self, unit: Option<u32>, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        let mut rings = Vec::with_capacity(6);
        for k in 1..=6 {
            let height = 5.0 + rand(6) as f32;
            let age = -10 - 4 * (k as i32 - 1);
            rings.push(StasisRing { k, age, size: 13.0, alpha: 0.0, height });
        }
        let mut stasis = Stasis { start: now, unit, ticks: 1, rings };
        // Stepped once on tick 0.
        stasis.step();
        self.stases.push(stasis);
    }

    /// Removes running Stasis for `unit` when EFST 453 status changes.
    pub fn forget_status(&mut self, unit: Option<u32>, status: u16) {
        if status == 453 {
            self.stases.retain(|s| s.unit != unit);
        }
    }

    /// Advances Cold Slower and Stasis by 16 ms ticks.
    pub fn advance(&mut self, now: Instant) {
        for slower in &mut self.cold_slowers {
            let e = (now.saturating_duration_since(slower.start).as_millis() / crate::prims::TICK.as_millis()) as u32;
            while slower.ticks <= e {
                slower.ticks += 1;
                slower.step();
            }
        }
        self.cold_slowers.retain(|slower| slower.ticks <= 200);

        for stasis in &mut self.stases {
            let e = (now.saturating_duration_since(stasis.start).as_millis() / crate::prims::TICK.as_millis()) as u32;
            while stasis.ticks <= e {
                stasis.ticks += 1;
                stasis.step();
            }
        }
        self.stases.retain(|stasis| stasis.ticks <= 300);
    }

    /// The body flags and colour an active Cold Slower or Stasis gives `unit` at `now`.
    pub fn glow(&self, unit: Option<u32>, now: Instant) -> Option<(u32, Option<[u8; 3]>)> {
        self.cold_slowers
            .iter()
            .filter(|s| s.unit == unit)
            .find_map(|s| s.glow((now.saturating_duration_since(s.start).as_millis() / crate::prims::TICK.as_millis()) as u32))
            .or_else(|| {
                self.stases
                    .iter()
                    .filter(|s| s.unit == unit)
                    .find_map(|s| s.glow((now.saturating_duration_since(s.start).as_millis() / crate::prims::TICK.as_millis()) as u32))
            })
    }

    /// The colour fading back to white after the flicker ends.
    pub fn body_tint(&self, unit: Option<u32>, now: Instant) -> Option<[u8; 3]> {
        self.cold_slowers
            .iter()
            .filter(|s| s.unit == unit)
            .find_map(|s| s.body_tint((now.saturating_duration_since(s.start).as_millis() / crate::prims::TICK.as_millis()) as u32))
            .or_else(|| {
                self.stases
                    .iter()
                    .filter(|s| s.unit == unit)
                    .find_map(|s| s.body_tint((now.saturating_duration_since(s.start).as_millis() / crate::prims::TICK.as_millis()) as u32))
            })
    }

    /// Cold Slower's snowflake quads to draw at `now`: camera-facing, additive, five trailing copies per flake.
    pub fn quads<'a>(&self, now: Instant, texture_of: impl Fn(&str) -> Option<&'a Texture>) -> Vec<EffectQuad<'a>> {
        let _ = now;
        let Some(texture) = texture_of(SNOW_TEXTURE) else {
            return Vec::new();
        };
        let mut quads = Vec::new();
        for slower in &self.cold_slowers {
            let p = slower.place;
            let d = slower.distance;
            for prim in &slower.prims {
                let [cr, cg, cb] = prim.color;
                for flake in &prim.flakes {
                    if flake.alpha <= 0 {
                        continue;
                    }
                    let (sin_h, cos_h) = (flake.heading as f32).to_radians().sin_cos();
                    let corners = flake_corners(flake.size, flake.spin);
                    for j in 0..5 {
                        let copy_alpha = if j == 0 { flake.alpha } else { flake.alpha - 100 - 20 * j };
                        if copy_alpha <= 0 {
                            continue;
                        }
                        let s = (d * flake.age as f32 / flake.span as f32 - 2.0 * j as f32).max(0.0);
                        let center = [p[0] + s * cos_h, p[1] + 12.0, p[2] + s * sin_h];
                        let tint = [cr, cg, cb, copy_alpha as f32 / 255.0];
                        quads.push((texture, center, corners, tint, true, false));
                    }
                }
            }
        }
        quads
    }

    /// Stasis's flat diamond rings at `now`: horizontal meshes following the unit.
    pub fn meshes(&self, position_of: impl Fn(Option<u32>) -> Option<Vector>) -> Vec<Wall> {
        let mut walls = Vec::new();
        for stasis in &self.stases {
            let Some(feet) = position_of(stasis.unit) else { continue };
            for ring in &stasis.rings {
                if ring.alpha <= 0.0 || ring.age <= 0 || ring.age >= 30 {
                    continue;
                }
                let r = ring.radius();
                let tint = [STASIS_RING_COLOR[0], STASIS_RING_COLOR[1], STASIS_RING_COLOR[2], ring.alpha / 255.0];
                let corner = |dx: f32, dz: f32, uv: [f32; 2]| MeshVertex { position: [feet[0] + dx, feet[1] + ring.height, feet[2] + dz], uv, tint };
                let [a, b, c, d] = [corner(r, 0.0, [0.0, 1.0]), corner(0.0, r, [1.0, 1.0]), corner(-r, 0.0, [1.0, 0.0]), corner(0.0, -r, [0.0, 0.0])];
                walls.push(Wall { texture: THUNDER_BALLS[ring.k - 1], vertices: vec![a, b, c, a, c, d], additive: true, forced: false });
            }
        }
        walls
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actor::{GLOW_ADDITIVE, GLOW_OUTLINE};
    use crate::prims::TICK;

    fn near(got: f32, want: f32) -> bool {
        (got - want).abs() < 0.05
    }

    #[test]
    fn cold_slower_prims_timing_and_draws() {
        let start = Instant::now();
        let mut snow = Snow::default();
        let feet = [10.0, 0.0, 20.0];
        let aim = [30.0, 0.0, 40.0];

        // 3 prims of 4 flakes made at e = 0 and none later; 6 draws a flake (72 in all)
        let mut draws = Vec::new();
        snow.cold_slower(Some(1), feet, aim, start, |cap| {
            draws.push(cap);
            0
        });
        assert_eq!(draws.len(), 72);
        for chunk in draws.chunks(6) {
            assert_eq!(chunk, [21, 4, 360, 41, 360, 21]);
        }
        assert_eq!(snow.cold_slowers[0].prims.len(), 3);
        assert!(snow.cold_slowers[0].prims.iter().all(|prim| prim.flakes.len() == 4));

        // None made on later ticks
        let count_before = snow.cold_slowers[0].prims.len();
        snow.advance(start + TICK * 5);
        assert_eq!(snow.cold_slowers[0].prims.len(), count_before);

        // With every draw 0: size 2.5, T = 60, h = A(P - A) + 160
        let b = bearing(feet[0] - aim[0], feet[2] - aim[2]);
        let mut zero_snow = Snow::default();
        zero_snow.cold_slower(Some(1), feet, aim, start, |_| 0);
        let flake0 = &zero_snow.cold_slowers[0].prims[0].flakes[0];
        assert_eq!(flake0.size, 2.5);
        assert_eq!(flake0.span, 60);
        let mut expected_h0 = b + 160;
        if expected_h0 >= 360 {
            expected_h0 -= 360;
        }
        assert_eq!(flake0.heading, expected_h0);

        // With every draw at its maximum: size 4.1, T = 40, h = A + 200 wrapped
        let mut max_snow = Snow::default();
        max_snow.cold_slower(Some(1), feet, aim, start, |cap| cap - 1);
        let flake_max = &max_snow.cold_slowers[0].prims[0].flakes[0];
        assert!(near(flake_max.size, 4.1));
        assert_eq!(flake_max.span, 40);
        let mut expected_h = b + 160 + 40;
        if expected_h >= 360 {
            expected_h -= 360;
        }
        assert_eq!(flake_max.heading, expected_h);

        // The colour from element 0's m for all four flakes of a prim
        let mut color_snow = Snow::default();
        // Give element 0 m = 2 (#4B4BFF), and elements 1..3 m = 0
        let mut call = 0;
        color_snow.cold_slower(Some(1), feet, aim, start, |_| {
            call += 1;
            if call == 2 { 2 } else { 0 }
        });
        assert_eq!(color_snow.cold_slowers[0].prims[0].color, FLAKE_COLORS[2]);

        // Alpha at n = 1, 9, 10, 40, 41, 84, 85 (25, 225, 225, 225, 220, 5, 0)
        let mut flake = Flake::new(3.0, 0.0, 0, 50);
        let mut alpha_map = Vec::new();
        for _ in 1..=85 {
            flake.step();
            if [1, 9, 10, 40, 41, 84, 85].contains(&flake.age) {
                alpha_map.push((flake.age, flake.alpha));
            }
        }
        assert_eq!(alpha_map, [(1, 25), (9, 225), (10, 225), (40, 225), (41, 220), (84, 5), (85, 0)]);

        // Copy distance s at n = T for j = 0 (D), and at j = 4 (D - 8); s never below 0
        let d = 50.0_f32;
        let t = 50_u32;
        let s0 = (d * t as f32 / t as f32 - 0.0).max(0.0);
        let s4 = (d * t as f32 / t as f32 - 8.0).max(0.0);
        assert_eq!(s0, d);
        assert_eq!(s4, d - 8.0);
        let s_neg = (10.0_f32 * 1.0 / 50.0 - 8.0_f32).max(0.0);
        assert_eq!(s_neg, 0.0);

        // Copy alphas for flake alpha 225 (225, 105, 85, 65, 45)
        let copy_alphas: Vec<i32> = (0..5).map(|j| if j == 0 { 225 } else { (225 - 100 - 20 * j).max(0) }).collect();
        assert_eq!(copy_alphas, [225, 105, 85, 65, 45]);

        // With D = 0 every copy stays at P
        let d_zero = 0.0_f32;
        for j in 0..5 {
            let s = (d_zero * 20.0 / 50.0 - 2.0 * j as f32).max(0.0);
            assert_eq!(s, 0.0);
        }

        // Spin + 1 a tick
        let mut spin_flake = Flake::new(3.0, 45.0, 0, 50);
        spin_flake.step();
        assert_eq!(spin_flake.spin, 46.0);

        // The flicker on e = 6 (additive #0000FF with glow copies), 7 (#FFFFFF), 40 (additive #0000FF) and nothing on e = 5 and 41
        let slower = &snow.cold_slowers[0];
        assert_eq!(slower.glow(5), None);
        assert_eq!(slower.glow(6), Some((GLOW_OUTLINE | GLOW_ADDITIVE, Some([0, 0, 0xFF]))));
        assert_eq!(slower.glow(7), Some((GLOW_OUTLINE, Some([0xFF, 0xFF, 0xFF]))));
        assert_eq!(slower.glow(40), Some((GLOW_OUTLINE | GLOW_ADDITIVE, Some([0, 0, 0xFF]))));
        assert_eq!(slower.glow(41), None);
        assert_eq!(slower.body_tint(41), Some([8, 8, 255]));
    }

    #[test]
    fn stasis_rings_timing_and_draws() {
        let start = Instant::now();
        let mut snow = Snow::default();

        // 6 prims made at e = 0 and none later; height between 5 and 10
        let mut height_draws = Vec::new();
        snow.stasis(Some(2), start, |cap| {
            height_draws.push(cap);
            3
        });
        assert_eq!(height_draws.len(), 6);
        assert_eq!(height_draws, [6; 6]);
        assert_eq!(snow.stases[0].rings.len(), 6);
        for ring in &snow.stases[0].rings {
            assert!(ring.height >= 5.0 && ring.height <= 10.0);
        }

        // Ring k first drawn at e = 10, 14, 18, 22, 26, 30
        let mut first_drawn = [0; 6];
        for e in 0..=35 {
            let now = start + TICK * e;
            snow.advance(now);
            for (idx, ring) in snow.stases[0].rings.iter().enumerate() {
                if first_drawn[idx] == 0 && ring.alpha > 0.0 && ring.age > 0 {
                    first_drawn[idx] = e;
                }
            }
        }
        assert_eq!(first_drawn, [10, 14, 18, 22, 26, 30]);

        // Alpha and R at n = 1, 5, 10, 15, 20, 25, 29 as in the table; none from n = 30
        let mut ring = StasisRing { k: 1, age: 0, size: 13.0, alpha: 0.0, height: 5.0 };
        let mut vals = Vec::new();
        for _ in 1..=35 {
            ring.step();
            if [1, 5, 10, 15, 20, 25, 29, 30].contains(&ring.age) {
                vals.push((ring.age, ring.alpha, ring.radius()));
            }
        }
        assert_eq!(vals[0].0, 1);
        assert_eq!(vals[0].1, 2.0);
        assert!(near(vals[0].2, 12.5));

        assert_eq!(vals[1].0, 5);
        assert_eq!(vals[1].1, 10.0);
        assert!(near(vals[1].2, 16.4));

        assert_eq!(vals[2].0, 10);
        assert_eq!(vals[2].1, 20.0);
        assert!(near(vals[2].2, 23.0));

        assert_eq!(vals[3].0, 15);
        assert_eq!(vals[3].1, 15.0);
        assert!(near(vals[3].2, 32.3));

        assert_eq!(vals[4].0, 20);
        assert_eq!(vals[4].1, 10.0);
        assert!(near(vals[4].2, 45.3));

        assert_eq!(vals[5].0, 25);
        assert_eq!(vals[5].1, 5.0);
        assert!(near(vals[5].2, 63.5));

        assert_eq!(vals[6].0, 29);
        assert_eq!(vals[6].1, 1.0);
        assert!(near(vals[6].2, 83.2));

        assert_eq!(vals[7].0, 30);
        assert_eq!(vals[7].1, 0.0);

        // The flicker without glow copies on e = 6 … 40
        let stasis = &snow.stases[0];
        assert_eq!(stasis.glow(5), None);
        assert_eq!(stasis.glow(6), Some((GLOW_ADDITIVE, Some([0, 0, 0xFF]))));
        assert_eq!(stasis.glow(7), Some((0, Some([0xFF, 0xFF, 0xFF]))));
        assert_eq!(stasis.glow(40), Some((GLOW_ADDITIVE, Some([0, 0, 0xFF]))));
        assert_eq!(stasis.glow(41), None);
        assert_eq!(stasis.body_tint(41), Some([8, 8, 255]));

        // A second status packet removes the first 799 before starting one; status off only removes
        let mut status_snow = Snow::default();
        status_snow.stasis(Some(3), start, |_| 0);
        assert_eq!(status_snow.stases.len(), 1);
        // Status off removes
        status_snow.forget_status(Some(3), 453);
        assert!(status_snow.stases.is_empty());
        // Status on restarts
        status_snow.stasis(Some(3), start, |_| 0);
        assert_eq!(status_snow.stases.len(), 1);
        status_snow.forget_status(Some(3), 453);
        status_snow.stasis(Some(3), start, |_| 0);
        assert_eq!(status_snow.stases.len(), 1);

        // No repeat after 300 ticks
        status_snow.advance(start + TICK * 301);
        assert!(status_snow.stases.is_empty());
    }
}
