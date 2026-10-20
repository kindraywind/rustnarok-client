//! The knight and crusader skills' code-made shapes, a 16 ms tick at a time: Spiral Pierce's ten pairs of cones (339), Soul
//! Breaker's four slashes (361), Pressure's falling cross (365) and Martyr's Reckoning's 20 rays (366).
//!
//! A shape's own age a counts from 1 at its first tick.

use std::time::Instant;

use super::ground::{Potion, flash_at};
use super::passes;
use super::sage::{ring_point, ring_quads};
use super::walls::{RING_YELLOW, Wall};
use crate::world::{MeshVertex, Vector};

pub const PURPLE_SLASH: &str = "data\\texture\\effect\\purpleslash.tga";
pub const CROSS: &str = "data\\texture\\effect\\cross_old.bmp";
pub const CROSS_FLASH: &str = "data\\texture\\effect\\explosive_1_128.bmp";
pub const RAY: &str = "data\\texture\\effect\\alpha_center.tga";

/// Spiral Pierce's pairs are made on ticks 27, 30 … 54 and each is seen on 10 ticks.
const SPIRAL_FIRST: u32 = 27;
const SPIRAL_LAST: u32 = 54;
/// Soul Breaker: four slashes, a square 20 from its middle to a corner, speed 0.8, the colour #C8C8FF.
const SLASH_HALF: f32 = 20.0;
const SLASH_TINT: [f32; 3] = [200.0 / 255.0, 200.0 / 255.0, 1.0];
/// Pressure's cross starts 115 up and lands 19 up on tick 32; its flash waits until tick 27.
const CROSS_START: f32 = 115.0;
const CROSS_LAND: u32 = 32;
const FLASH_WAIT: u32 = 27;
/// Martyr's Reckoning: 5 prims of 4 rays, an age behind by 24, the rays' tips 12 up.
const RAYS: usize = 20;

struct Spiral {
    unit: Option<u32>,
    start: Instant,
}

/// One of Soul Breaker's slashes: how high over the feet and its heading.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Slash {
    height: f32,
    heading: f32,
}

struct Breaker {
    caster: Option<u32>,
    start: Instant,
    from: Vector,
    slashes: [Slash; 4],
}

struct Pressure {
    unit: Option<u32>,
    start: Instant,
    spin: f32,
    /// Where it stands once landed (the target's feet), once known.
    landed: Option<Vector>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Ray {
    plane: f32,
    angle: f32,
}

struct Martyr {
    unit: Option<u32>,
    start: Instant,
    rays: Vec<Ray>,
}

#[derive(Default)]
pub struct Knight {
    spirals: Vec<Spiral>,
    breakers: Vec<Breaker>,
    pressures: Vec<Pressure>,
    martyrs: Vec<Martyr>,
}

/// Cone k of Spiral Pierce's pair j: the rim's height, the radius, the start turn and the lean.
pub fn spiral_cone(j: u32, k: usize) -> (f32, f32, f32, f32) {
    let (j, k) = (j as f32, k as f32);
    (20.0 - 0.7 * j, 5.0 + 0.2 * j, 6.0 * j + 9.0 * k, (2.0 * (35.0 - 4.0 * k - j)).max(0.0))
}

/// A cone's alpha at its age a: cone 0 gains 10 a tick to 80, cone 1 gains 5 to 40, both lose 30 from a = 9.
pub fn spiral_alpha(k: usize, a: u32) -> f32 {
    let (gain, top) = if k == 0 { (10.0, 80.0) } else { (5.0, 40.0) };
    if a < 9 { (gain * a as f32).min(top) } else { (top - 30.0 * (a - 8) as f32).max(0.0) }
}

/// How far a slash is from where it started at its age a: 0.8 at 1, then its speed grows 2 % a tick to a = 10, 4 % after.
pub fn slash_distance(a: u32) -> f32 {
    let (mut speed, mut distance) = (0.8_f32, 0.8_f32);
    for age in 2..=a {
        speed *= if age <= 10 { 1.02 } else { 1.04 };
        distance += speed;
    }
    distance
}

/// A slash's alpha at its age a (seen from 2 to 27): 10 a tick to 90, then 5 less a tick.
pub fn slash_alpha(a: i32) -> f32 {
    match a {
        2..=10 => 10.0 * (a - 1) as f32,
        11..=28 => (90.0 - 5.0 * (a - 10) as f32).max(0.0),
        _ => 0.0,
    }
}

/// The cross's height over the target at count n: 3 a tick down from 115 to 19.
pub fn cross_height(n: u32) -> f32 {
    CROSS_START - 3.0 * n.min(CROSS_LAND) as f32
}

/// The cross's alpha at n: 20 a tick to 200, held to 32, then 5 less a tick; drawn for n = 1 to 71.
pub fn cross_alpha(n: u32) -> f32 {
    match n {
        1..=10 => 20.0 * n as f32,
        11..=32 => 200.0,
        33..=72 => (200.0 - 5.0 * (n - 32) as f32).max(0.0),
        _ => 0.0,
    }
}

/// The cross's spin at n: down 15° a tick until it lands.
pub fn cross_spin(spin: f32, n: u32) -> f32 {
    (spin - 15.0 * n.min(CROSS_LAND) as f32).rem_euclid(360.0)
}

/// A ray's alpha at age a: 20 a tick to 200 at 10, held to 12, then 15 less a tick; seen for a = 1 to 25.
pub fn ray_alpha(a: i32) -> f32 {
    match a {
        1..=10 => 20.0 * a as f32,
        11..=12 => 200.0,
        13..=26 => (200.0 - 15.0 * (a - 12) as f32).max(0.0),
        _ => 0.0,
    }
}

/// How far a ray's tip is from the middle at age a: 2 grown 15 % a tick.
pub fn ray_length(a: i32) -> f32 {
    2.0 * 1.15_f32.powi(a)
}

/// A point of the ray's plane at `angle` degrees: the plane contains the z axis turned by `plane`, 12 up.
fn ray_point(plane: f32, length: f32, angle: f32) -> Vector {
    let (plane_sin, plane_cos) = plane.to_radians().sin_cos();
    let (sin, cos) = angle.to_radians().sin_cos();
    [plane_cos * length * cos, 12.0 - plane_sin * length * cos, length * sin]
}

impl Knight {
    /// Spiral Pierce on `unit`: the cones come as the effect goes on.
    pub fn spiral(&mut self, unit: Option<u32>, now: Instant) {
        self.spirals.push(Spiral { unit, start: now });
    }

    /// Soul Breaker from `from` (the caster's place) toward `aim`: four slashes, their choices from `rand(n)` (0 to n − 1).
    pub fn breaker(&mut self, caster: Option<u32>, from: Vector, aim: Vector, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        // Straight at the aim, a whole number of degrees; 180 on the same spot.
        let (dx, dz) = (aim[0] - from[0], aim[2] - from[2]);
        let toward = if dx == 0.0 && dz == 0.0 { 180.0 } else { dz.atan2(dx).to_degrees().trunc() };
        let heights = [(); 4].map(|()| 10.0 - rand(5) as f32);
        // Each slash takes its heading on its first tick: band 3 first, then 2, 1 and 0.
        let mut headings = [0.0; 4];
        for k in (0..4).rev() {
            headings[k] = (toward + rand(31) as f32 - 15.0).rem_euclid(360.0);
        }
        let slashes = std::array::from_fn(|k| Slash { height: heights[k], heading: headings[k] });
        self.breakers.push(Breaker { caster, start: now, from, slashes });
    }

    /// Pressure on `unit`: a cross falls onto it, its spin from `rand(n)`.
    pub fn pressure(&mut self, unit: Option<u32>, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        self.pressures.push(Pressure { unit, start: now, spin: rand(360) as f32, landed: None });
    }

    /// Martyr's Reckoning on `unit`: 20 rays, per ray the plane's turn from `rand(n)`.
    pub fn martyr(&mut self, unit: Option<u32>, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        let mut rays = Vec::with_capacity(RAYS);
        for i in 0..5 {
            for k in 0..4 {
                rays.push(Ray { plane: rand(360) as f32, angle: (90 * k + 22 * i) as f32 });
            }
        }
        self.martyrs.push(Martyr { unit, start: now, rays });
    }

    /// A cross that has landed stays where the target stood; what ended goes.
    pub fn advance(&mut self, now: Instant, position_of: impl Fn(Option<u32>) -> Option<Vector>) {
        for pressure in &mut self.pressures {
            if pressure.landed.is_none() && passes(pressure.start, now) > CROSS_LAND {
                pressure.landed = position_of(pressure.unit);
            }
        }
        self.spirals.retain(|spiral| passes(spiral.start, now) <= SPIRAL_LAST + 10);
        self.breakers.retain(|breaker| passes(breaker.start, now) <= 55);
        self.pressures.retain(|pressure| passes(pressure.start, now) <= 75);
        self.martyrs.retain(|martyr| passes(martyr.start, now) <= 50);
    }

    /// Every wall to draw at `now`; `position_of` gives a unit's feet.
    pub fn meshes(&self, now: Instant, position_of: impl Fn(Option<u32>) -> Option<Vector>) -> Vec<Wall> {
        let mut walls = Vec::new();
        for spiral in &self.spirals {
            let (Some(feet), Some(t)) = (position_of(spiral.unit), passes(spiral.start, now).checked_sub(1)) else { continue };
            for made in (SPIRAL_FIRST..=SPIRAL_LAST).step_by(3) {
                let Some(a) = t.checked_sub(made).map(|since| since + 1).filter(|&a| a <= 10) else { continue };
                for k in 0..2 {
                    let alpha = spiral_alpha(k, a);
                    if alpha <= 0.0 {
                        continue;
                    }
                    let (height, radius, turn, lean) = spiral_cone(made - 25, k);
                    let base = [feet[0], feet[1] + 3.0, feet[2]];
                    let points: Vec<_> = (0..=20).map(|s| ring_point(base, (turn + 18.0 * s as f32) % 360.0, radius, height, lean)).collect();
                    let vertices = ring_quads(&points, alpha / 255.0)
                        .into_iter()
                        .map(|(position, uv, alpha)| MeshVertex { position, uv, tint: [1.0, 1.0, 1.0, alpha] })
                        .collect();
                    walls.push(Wall { texture: RING_YELLOW, vertices, additive: true, forced: false });
                }
            }
        }
        for breaker in &self.breakers {
            let Some(t) = passes(breaker.start, now).checked_sub(1) else { continue };
            for (k, slash) in breaker.slashes.iter().enumerate() {
                let a = k as i32 - 27 + t as i32;
                let alpha = slash_alpha(a);
                if alpha <= 0.0 {
                    continue;
                }
                let (sin, cos) = slash.heading.to_radians().sin_cos();
                let distance = slash_distance(a as u32);
                let centre = [breaker.from[0] + distance * cos, breaker.from[1] + slash.height, breaker.from[2] + distance * sin];
                let tint = [SLASH_TINT[0], SLASH_TINT[1], SLASH_TINT[2], alpha / 255.0];
                let first = (slash.heading + 45.0) % 360.0;
                let corner = |i: u32, uv: [f32; 2]| {
                    let (sin, cos) = (first + 90.0 * i as f32).to_radians().sin_cos();
                    MeshVertex { position: [centre[0] + SLASH_HALF * cos, centre[1], centre[2] + SLASH_HALF * sin], uv, tint }
                };
                let [c0, c1, c2, c3] = [corner(0, [0.0, 1.0]), corner(1, [1.0, 1.0]), corner(2, [1.0, 0.0]), corner(3, [0.0, 0.0])];
                walls.push(Wall { texture: PURPLE_SLASH, vertices: vec![c0, c1, c2, c0, c2, c3], additive: false, forced: false });
            }
        }
        for pressure in &self.pressures {
            let (Some(feet), Some(t)) = (position_of(pressure.unit).or(pressure.landed), passes(pressure.start, now).checked_sub(1)) else { continue };
            let (half, alpha) = flash_at(t.saturating_sub(FLASH_WAIT));
            if alpha > 0.0 {
                let tint = [1.0, 1.0, 1.0, alpha / 255.0];
                let corner = |x: f32, z: f32, uv: [f32; 2]| MeshVertex { position: [feet[0] + x, feet[1] + 5.0, feet[2] + z], uv, tint };
                let [a, b, c, d] =
                    [corner(half, 0.0, [0.0, 1.0]), corner(0.0, half, [1.0, 1.0]), corner(-half, 0.0, [1.0, 0.0]), corner(0.0, -half, [0.0, 0.0])];
                walls.push(Wall { texture: CROSS_FLASH, vertices: vec![a, b, c, a, c, d], additive: true, forced: false });
            }
        }
        for martyr in &self.martyrs {
            let (Some(feet), Some(t)) = (position_of(martyr.unit), passes(martyr.start, now).checked_sub(1)) else { continue };
            let a = t as i32 - 23;
            let alpha = ray_alpha(a);
            if alpha <= 0.0 {
                continue;
            }
            let length = ray_length(a);
            let (mut core, mut edge) = (Vec::new(), Vec::new());
            for ray in &martyr.rays {
                let at = |angle: f32| {
                    let point = ray_point(ray.plane, length, angle);
                    [feet[0] + point[0], feet[1] + point[1], feet[2] + point[2]]
                };
                let middle = [feet[0], feet[1] + 12.0, feet[2]];
                let wedge = |width: f32, rgb: [f32; 3], vertices: &mut Vec<MeshVertex>| {
                    let tint = [rgb[0], rgb[1], rgb[2], alpha / 255.0];
                    vertices.extend([
                        MeshVertex { position: at(ray.angle - width), uv: [0.0, 1.0], tint },
                        MeshVertex { position: at(ray.angle + width), uv: [1.0, 1.0], tint },
                        MeshVertex { position: middle, uv: [1.0, 0.0], tint },
                    ]);
                };
                wedge(2.0, [0.0, 250.0 / 255.0, 250.0 / 255.0], &mut core);
                wedge(5.0, [250.0 / 255.0, 0.0, 0.0], &mut edge);
            }
            walls.push(Wall { texture: RAY, vertices: edge, additive: true, forced: false });
            walls.push(Wall { texture: RAY, vertices: core, additive: true, forced: false });
        }
        walls
    }

    /// The falling crosses and their ghosts at `now`: three ghosts a cross, where it was 1, 2 and 3 ticks earlier.
    pub fn crosses(&self, now: Instant, position_of: impl Fn(Option<u32>) -> Option<Vector>) -> Vec<Potion> {
        let mut crosses = Vec::new();
        for pressure in &self.pressures {
            let (Some(feet), Some(t)) = (pressure.landed.or_else(|| position_of(pressure.unit)), passes(pressure.start, now).checked_sub(1)) else { continue };
            let n = t + 1;
            let at = |n: u32| [feet[0], feet[1] + cross_height(n), feet[2]];
            if (1..=71).contains(&n) {
                crosses.push(Potion { icon: CROSS, at: at(n), spin: cross_spin(pressure.spin, n), alpha: cross_alpha(n) });
            }
            for k in 1..=3 {
                let Some(earlier) = n.checked_sub(k).filter(|&earlier| (1..=71).contains(&earlier)) else { continue };
                let alpha = (cross_alpha(earlier) - (75 + 25 * k) as f32).max(0.0);
                if alpha > 0.0 {
                    crosses.push(Potion { icon: CROSS, at: at(earlier), spin: cross_spin(pressure.spin, n), alpha });
                }
            }
        }
        crosses
    }

    /// The body flags and colour Soul Breaker (magenta, additive, ticks 0 to 10) and Martyr's Reckoning (pale red outline copies,
    /// ticks 20 to 40) give `unit` at `now`.
    pub fn glow(&self, unit: Option<u32>, now: Instant) -> Option<(u32, Option<[u8; 3]>)> {
        let breaking = self.breakers.iter().filter(|breaker| breaker.caster == unit).find_map(|breaker| {
            passes(breaker.start, now).checked_sub(1).filter(|&t| t <= 10)?;
            Some((crate::actor::GLOW_ADDITIVE, Some([0xFF, 0x00, 0xFF])))
        });
        breaking.or_else(|| {
            self.martyrs.iter().filter(|martyr| martyr.unit == unit).find_map(|martyr| {
                passes(martyr.start, now).checked_sub(1).filter(|t| (20..=40).contains(t))?;
                Some((crate::actor::GLOW_OUTLINE, Some([0xFF, 0xC8, 0xC8])))
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prims::TICK;

    fn near(got: f32, want: f32) -> bool {
        (got - want).abs() < 0.01
    }

    #[test]
    fn spiral_pierce_flashes_ten_pairs_of_cones_and_sinks_them() {
        // Pair j = 2: h 18.6, r 5.4, lean 66/58; j = 29: h −0.3, r 10.8, lean 12/4.
        let (h, r, _, lean0) = spiral_cone(2, 0);
        assert!(near(h, 18.6) && near(r, 5.4) && near(lean0, 66.0) && near(spiral_cone(2, 1).3, 58.0));
        let (h, r, turn, lean0) = spiral_cone(29, 0);
        assert!(
            near(h, -0.3) && near(r, 10.8) && near(lean0, 12.0) && near(spiral_cone(29, 1).3, 4.0) && near(turn, 174.0) && near(spiral_cone(29, 1).2, 183.0)
        );
        // Cone 0 shows 10 … 80 (a = 1…8), 50, 20, gone at 11; cone 1 shows 5 … 40, 10, gone at 10.
        assert_eq!([1, 8, 9, 10, 11].map(|a| spiral_alpha(0, a)), [10.0, 80.0, 50.0, 20.0, 0.0]);
        assert_eq!([1, 8, 9, 10].map(|a| spiral_alpha(1, a)), [5.0, 40.0, 10.0, 0.0]);
        // Ten pairs made on ticks 27, 30 … 54; a pair made at 27 is seen on ticks 27…36; the last ends at tick 63.
        let start = Instant::now();
        let mut knight = Knight::default();
        knight.spiral(Some(1), start);
        let feet = [10.0, 2.0, 10.0];
        let count = |tick: u32| knight.meshes(start + TICK * tick, |_| Some(feet)).len();
        assert_eq!((count(26), count(27), count(28)), (0, 2, 2));
        assert_eq!(count(35), 3 + 3, "ages 9 (two cones with alpha), 6 and 3 … ");
        assert!(count(36) > 0 && count(63) > 0 && count(64) == 0);
        let walls = knight.meshes(start + TICK * 27, |_| Some(feet));
        assert!(walls.iter().all(|wall| wall.texture == RING_YELLOW && wall.additive && wall.vertices.len() == 120));
        // 3 over the feet at the rim's base; the cone follows its target.
        assert!(walls[0].vertices.iter().any(|vertex| near(vertex.position[1], 5.0)));
        assert!(knight.meshes(start, |_| None).is_empty());
    }

    #[test]
    fn soul_breaker_throws_four_slashes_toward_the_aim() {
        let start = Instant::now();
        let mut knight = Knight::default();
        // Every draw 0: 10 − 0 high, heading east − 15 = 345 (R(31) − 15).
        knight.breaker(Some(2), [10.0, 1.0, 10.0], [30.0, 1.0, 10.0], start, |_| 0);
        assert!(knight.breakers[0].slashes.iter().all(|slash| slash.height == 10.0 && slash.heading == 345.0));
        // Alpha at a = 2, 10, 11, 27, 28: 10, 90, 85, 5, 0; distance 0.8, 1.62, 8.76 at a = 1, 2, 10.
        assert_eq!([2, 10, 11, 27, 28].map(slash_alpha), [10.0, 90.0, 85.0, 5.0, 0.0]);
        assert!(
            near(slash_distance(1), 0.8)
                && (slash_distance(2) - 1.616).abs() < 0.01
                && (slash_distance(10) - 8.76).abs() < 0.01
                && (slash_distance(11) - 9.75).abs() < 0.01
        );
        assert!((slash_distance(20) - 20.7).abs() < 0.1 && (slash_distance(27) - 32.3).abs() < 0.1);
        // The first live ticks: band 3 at tick 25 (a = 1 shows nothing), a = 2 at 26 … seen on ticks 29 − k … 54 − k.
        let slashes = |tick: u32| knight.meshes(start + TICK * tick, |_| None).len();
        assert_eq!((slashes(25), slashes(26), slashes(27), slashes(28), slashes(29)), (0, 1, 2, 3, 4));
        assert_eq!((slashes(51), slashes(52), slashes(54), slashes(55)), (4, 3, 1, 0));
        // A flat square 20 from its middle, 10 over the feet, the colour #C8C8FF, normal blend, texture purpleslash.
        let wall = &knight.meshes(start + TICK * 29, |_| None)[0];
        assert!(wall.texture == PURPLE_SLASH && !wall.additive && wall.vertices.len() == 6);
        assert!(wall.vertices.iter().all(|vertex| near(vertex.position[1], 11.0)));
        assert_eq!(wall.vertices[0].tint[..3], [200.0 / 255.0, 200.0 / 255.0, 1.0]);
        let corner0 = wall.vertices[0].position;
        let middle = [10.0 + slash_distance(2) * 345.0_f32.to_radians().cos(), 10.0 + slash_distance(2) * 345.0_f32.to_radians().sin()];
        assert!(near(f32::hypot(corner0[0] - middle[0], corner0[2] - middle[1]), 20.0));
        assert_eq!(wall.vertices[0].uv, [0.0, 1.0]);
        // The caster's body is magenta and added on, ticks 0…10 only.
        let glow = |tick: u32| knight.glow(Some(2), start + TICK * tick);
        assert_eq!(glow(0), Some((crate::actor::GLOW_ADDITIVE, Some([0xFF, 0, 0xFF]))));
        assert!(glow(10).is_some() && glow(11).is_none() && knight.glow(Some(3), start).is_none());
        // On the same spot the aim direction is 180°.
        let mut here = Knight::default();
        here.breaker(None, [0.0; 3], [0.0; 3], start, |_| 15);
        assert_eq!(here.breakers[0].slashes[0].heading, 180.0);
    }

    #[test]
    fn pressure_drops_a_cross_with_three_ghosts_and_lands_a_flash() {
        let start = Instant::now();
        let mut knight = Knight::default();
        knight.pressure(Some(1), start, |_| 0);
        // n: height, alpha — 1: 112, 20; 10: 85, 200; 32: 19, 200; 33: 19, 195; 71: 19, 5; 72: gone.
        assert_eq!([1, 10, 32, 33, 71].map(cross_height), [112.0, 85.0, 19.0, 19.0, 19.0]);
        assert_eq!([1, 10, 32, 33, 71, 72].map(cross_alpha), [20.0, 200.0, 200.0, 195.0, 5.0, 0.0]);
        assert_eq!(cross_alpha(52), 100.0);
        let feet = [4.0, 1.0, 4.0];
        let crosses = |tick: u32| knight.crosses(start + TICK * tick, |_| Some(feet));
        assert_eq!(crosses(0).iter().map(|cross| cross.alpha).collect::<Vec<_>>(), [20.0]);
        // At tick 12 (n = 13): the cross 200, then the ghosts 100, 75, 50.
        assert_eq!(crosses(12).iter().map(|cross| cross.alpha).collect::<Vec<_>>(), [200.0, 100.0, 75.0, 50.0]);
        assert!(crosses(0)[0].icon == CROSS && near(crosses(0)[0].at[1], 1.0 + 112.0));
        // Spin 15° less a tick until it lands.
        assert_eq!([1, 2, 32, 33, 50].map(|n| cross_spin(100.0, n)), [85.0, 70.0, 340.0, 340.0, 340.0]);
        // It stays where it landed even if the target walks away.
        knight.advance(start + TICK * 33, |_| Some(feet));
        let elsewhere = knight.crosses(start + TICK * 40, |_| Some([100.0, 1.0, 100.0]));
        assert!(elsewhere.iter().all(|cross| cross.at[0] == 4.0));
        // The ground flash: 0.9 size 14.445 at its tick 1 (tick 28), alpha 20 at its tick 10, 19 at 11, 0 at 30 (tick 57).
        assert!(near(flash_at(1).0, 14.445) && flash_at(10).1 == 20.0 && flash_at(11).1 == 19.0 && flash_at(30).1 == 0.0);
        let flash = |tick: u32| knight.meshes(start + TICK * tick, |_| Some(feet)).into_iter().filter(|wall| wall.texture == CROSS_FLASH).count();
        assert_eq!((flash(27), flash(28), flash(56), flash(57)), (0, 1, 1, 0));
        let wall = &knight.meshes(start + TICK * 28, |_| Some(feet))[0];
        assert!(wall.additive && wall.vertices.len() == 6 && wall.vertices.iter().all(|vertex| near(vertex.position[1], 6.0)));
        assert!(near((wall.vertices[0].position[0] - 4.0).abs().max((wall.vertices[1].position[2] - 4.0).abs()), 14.445));
        knight.advance(start + TICK * 80, |_| None);
        assert!(knight.pressures.is_empty());
    }

    #[test]
    fn martyrs_reckoning_shoots_twenty_rays() {
        let start = Instant::now();
        let mut knight = Knight::default();
        knight.martyr(Some(1), start, |_| 0);
        assert_eq!(knight.martyrs[0].rays.len(), 20);
        // The angles 90k + 22i, the largest 358.
        let mut angles: Vec<f32> = knight.martyrs[0].rays.iter().map(|ray| ray.angle).collect();
        angles.sort_by(f32::total_cmp);
        assert_eq!((angles[0], *angles.last().unwrap()), (0.0, 358.0));
        // Alpha at a = 1, 10, 12, 13, 20, 25, 26: 20, 200, 200, 185, 80, 5, 0; r at a = 1, 13: 2.30, 12.30.
        assert_eq!([1, 10, 12, 13, 20, 25, 26].map(ray_alpha), [20.0, 200.0, 200.0, 185.0, 80.0, 5.0, 0.0]);
        assert!((ray_length(1) - 2.30).abs() < 0.01 && (ray_length(13) - 12.30).abs() < 0.05 && (ray_length(20) - 32.7).abs() < 0.1);
        // Seen on ticks 24…48 (a = 1…25), following the caster.
        let feet = [0.0, 0.0, 0.0];
        let rays = |tick: u32| knight.meshes(start + TICK * tick, |_| Some(feet)).len();
        assert_eq!((rays(23), rays(24), rays(48), rays(49)), (0, 2, 2, 0));
        // Two wedges of 20 triangles: red at ±5°, cyan at ±2°, additive; the tip points for L = 100: 98 and 102, 95 and 105.
        let walls = knight.meshes(start + TICK * 24, |_| Some(feet));
        assert!(walls.iter().all(|wall| wall.texture == RAY && wall.additive && wall.vertices.len() == 60));
        assert_eq!(walls[0].vertices[0].tint[..3], [250.0 / 255.0, 0.0, 0.0]);
        assert_eq!(walls[1].vertices[0].tint[..3], [0.0, 250.0 / 255.0, 250.0 / 255.0]);
        let tip = |plane: f32, length: f32, angle: f32| ray_point(plane, length, angle);
        assert!(near(tip(0.0, 10.0, 98.0)[2], 10.0 * 98.0_f32.to_radians().sin()) && near(tip(0.0, 10.0, 0.0)[1], 12.0));
        assert_eq!(walls[1].vertices[2].position, [0.0, 12.0, 0.0]);
        assert_eq!(walls[1].vertices[0].uv, [0.0, 1.0]);
        assert_eq!(walls[1].vertices[2].uv, [1.0, 0.0]);
        // The caster has the outline copies in #FFC8C8 on ticks 20…40.
        let glow = |tick: u32| knight.glow(Some(1), start + TICK * tick);
        assert_eq!(glow(20), Some((crate::actor::GLOW_OUTLINE, Some([0xFF, 0xC8, 0xC8]))));
        assert!(glow(19).is_none() && glow(40).is_some() && glow(41).is_none());
    }
}
