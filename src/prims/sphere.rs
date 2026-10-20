//! I'm All Yours' heart of pink flame flowers (343) and Baby's and Cheer Up's tumbling fire ribbons (408), a 16 ms tick at a time.
//!
//! A band's own age n counts from 1 on its first tick; e is the ticks since the effect started.

use std::time::Instant;

use super::passes;
use super::walls::Wall;
use crate::world::{MeshVertex, Vector};

pub const RING_RED: &str = "data\\texture\\effect\\ring_red.tga";
pub const FIRE_IVY: &str = "data\\texture\\effect\\fire_ivy.bmp";
/// The pink of both (#FF59B6).
const PINK: [f32; 3] = [1.0, 89.0 / 255.0, 182.0 / 255.0];
/// A cast without a time lasts this many ticks; the hearts fade over their last 25.
const HEART_DEFAULT: u32 = 1000;
const HEART_FADE: u32 = 25;
const RIBBON_LIFE: u32 = 200;

/// The heart outline's twenty points (x, z) from the caster's feet, in the game's axes: the cleft at +z, the point at −z.
pub const HEART: [[f32; 2]; 20] = [
    [0.0, 9.99],
    [-3.51, 11.88],
    [-7.83, 12.96],
    [-12.42, 11.34],
    [-15.12, 7.02],
    [-15.39, 1.62],
    [-13.77, -3.24],
    [-10.8, -7.02],
    [-6.75, -10.26],
    [-2.7, -12.69],
    [0.0, -16.2],
    [2.97, -12.69],
    [7.02, -10.26],
    [11.07, -7.02],
    [14.04, -3.24],
    [15.66, 1.62],
    [15.39, 7.02],
    [12.69, 11.34],
    [8.1, 12.96],
    [3.78, 11.88],
];

/// One of a flame flower's two rings: its height, start turn, radius and lean.
struct Ring {
    height: f32,
    turn: f32,
    radius: f32,
    lean: f32,
}

/// A flower on one point of the heart.
struct Flower {
    point: [f32; 2],
    rings: [Ring; 2],
}

struct Hearts {
    unit: Option<u32>,
    at: Vector,
    start: Instant,
    length: u32,
    flowers: Vec<Flower>,
}

/// One tumbling ribbon: its wait and half-width, turn and arc, turn speed, radius, tilt and alpha.
struct Ribbon {
    age: i32,
    half: f32,
    turn: f32,
    arc: f32,
    speed: f32,
    radius: f32,
    tilt: i32,
    alpha: f32,
}

/// Twenty ribbons round a unit, run `done` ticks.
struct Cloud {
    unit: Option<u32>,
    start: Instant,
    done: u32,
    ribbons: Vec<Ribbon>,
}

#[derive(Default)]
pub struct Sphere {
    hearts: Vec<Hearts>,
    clouds: Vec<Cloud>,
}

/// A flower ring's alpha at its age n for a cast of `length` ticks: 5 a tick to 75, held to the last 25 ticks, then 2 less a tick.
pub fn heart_alpha(n: u32, length: u32) -> f32 {
    let hold = length.saturating_sub(HEART_FADE);
    if n < 16 {
        5.0 * n as f32
    } else if n <= hold {
        75.0
    } else {
        (75.0 - 2.0 * (n - hold) as f32).max(0.0)
    }
}

/// A flower ring's rim at point s and age n: a ripple on three quarters of its height, rising over the first 90 ticks.
pub fn heart_profile(ring_one: bool, height: f32, s: u32, n: u32) -> f32 {
    let i = (34 * s + if ring_one { 3 } else { 4 } * n) % 360;
    let grow = if n <= 90 { (n as f32).to_radians().sin() } else { 1.0 };
    ((height * 0.25) * (i as f32).to_radians().sin() + height * 0.75) * grow
}

/// A heart point as it lies round the caster seen by a camera turned `yaw` degrees: the same way up on the screen.
// ponytail: the client's +z runs toward the bottom of the screen, so the game's z is flipped, then turned back by the camera.
pub fn heart_offset(point: [f32; 2], yaw: f32) -> [f32; 2] {
    let (sin, cos) = yaw.to_radians().sin_cos();
    let (u, v) = (point[0], -point[1]);
    [u * cos - v * sin, u * sin + v * cos]
}

/// A ribbon's alpha at its age n: 2 a tick to 50 (at 25), held to 70, then 2 less a tick (0 at 95).
pub fn ribbon_alpha(n: u32) -> f32 {
    match n {
        0 => 0.0,
        1..=25 => 2.0 * n as f32,
        26..=70 => 50.0,
        _ => (50.0 - 2.0 * (n - 70) as f32).max(0.0),
    }
}

impl Ribbon {
    /// The ribbon of prim `p` (0, 9 … 36) and band `k`, its choices from `rand(n)` in the game's order.
    fn new(p: i32, k: i32, rand: &mut impl FnMut(u32) -> u32) -> Self {
        let age = -(rand(20) as i32);
        let half = 2.0 + 0.2 * rand(11) as f32;
        let turn = rand(360) as f32;
        let arc = 180.0 + rand(91) as f32;
        let speed = 3.0 + rand(4) as f32;
        Self { age, half, turn, arc, speed, radius: 5.0, tilt: 45 * k + p, alpha: 0.0 }
    }

    /// One tick: nothing while it waits; then it fades, swells, creeps round its tilt and turns.
    fn step(&mut self, rand: &mut impl FnMut(u32) -> u32) {
        self.age += 1;
        if self.age <= 0 {
            return;
        }
        let n = self.age as u32;
        self.alpha = ribbon_alpha(n);
        self.radius *= 1.01;
        self.tilt += 1;
        if rand(2) == 0 {
            self.tilt += 1;
        }
        self.tilt = self.tilt.rem_euclid(360);
        self.turn = (self.turn + self.speed) % 360.0;
    }

    /// The strip on a cylinder of radius r round an axis tilted from the vertical, 2h wide, round the feet `at`.
    fn mesh(&self, at: Vector) -> Vec<MeshVertex> {
        let tint = [PINK[0], PINK[1], PINK[2], self.alpha / 255.0];
        let (tilt_sin, tilt_cos) = (self.tilt as f32).to_radians().sin_cos();
        let step = (self.arc / 20.0).floor().max(1.0);
        let point = |s: u32, side: f32| {
            let (sin, cos) = (self.turn + s as f32 * step).to_radians().sin_cos();
            let centre = [self.radius * tilt_cos * cos, 10.0 - self.radius * tilt_sin * cos, self.radius * sin];
            let axis = [-tilt_sin, -tilt_cos, 0.0];
            [at[0] + centre[0] + side * self.half * axis[0], at[1] + centre[1] + side * self.half * axis[1], at[2] + centre[2]]
        };
        let last = ((self.arc / step).floor() as u32).min(20);
        let mut vertices = Vec::new();
        for s in 1..=last {
            let (u0, u1) = ((s - 1) as f32 / 21.0, s as f32 / 21.0);
            let at_edge = |s: u32, side: f32, uv: [f32; 2]| MeshVertex { position: point(s, side), uv, tint };
            let [a, b, c, d] = [at_edge(s - 1, 1.0, [u0, 1.0]), at_edge(s, 1.0, [u1, 1.0]), at_edge(s, -1.0, [u1, 0.0]), at_edge(s - 1, -1.0, [u0, 0.0])];
            vertices.extend([a, b, c, a, c, d]);
        }
        vertices
    }
}

impl Sphere {
    /// I'm All Yours' cast on `unit`, standing at `feet`, for `cast_ticks` ticks (1 000 with no cast time), replacing its last: twenty
    /// flowers, per flower the start turns of its two rings from `rand(n)`.
    pub fn heart(&mut self, unit: Option<u32>, feet: Vector, cast_ticks: u32, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        let length = if cast_ticks == 0 { HEART_DEFAULT } else { cast_ticks };
        let flowers = HEART
            .iter()
            .map(|&point| {
                let first = rand(360) as f32;
                let second = rand(360) as f32;
                Flower {
                    point,
                    rings: [Ring { height: 12.0, turn: first, radius: 2.3, lean: 86.0 }, Ring { height: 25.0, turn: second, radius: 2.0, lean: 90.0 }],
                }
            })
            .collect();
        self.hearts.retain(|hearts| hearts.unit != unit);
        self.hearts.push(Hearts { unit, at: feet, start: now, length, flowers });
    }

    /// A cast cut off: the hearts go.
    pub fn cut_hearts(&mut self) {
        self.hearts.clear();
    }

    /// Baby's or Cheer Up's ribbons round `unit`: five prims of four bands, each from `rand(n)`.
    pub fn ribbons(&mut self, unit: Option<u32>, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        let mut ribbons = Vec::with_capacity(20);
        for p in [0, 9, 18, 27, 36] {
            for k in 0..4 {
                ribbons.push(Ribbon::new(p, k, &mut rand));
            }
        }
        self.clouds.push(Cloud { unit, start: now, done: 0, ribbons });
    }

    /// Runs the ribbons up to `now`, a tick at a time (the tilt's creep from `rand(n)`), and drops what ended.
    pub fn advance(&mut self, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        for cloud in &mut self.clouds {
            while cloud.done < passes(cloud.start, now).min(RIBBON_LIFE) {
                cloud.done += 1;
                for ribbon in &mut cloud.ribbons {
                    ribbon.step(&mut rand);
                }
            }
        }
        self.clouds.retain(|cloud| passes(cloud.start, now) <= RIBBON_LIFE);
        self.hearts.retain(|hearts| passes(hearts.start, now) <= hearts.length);
    }

    /// The meshes at `now`: the hearts' flower rings where the casters stood, turned for a camera at `yaw` degrees; the ribbons round
    /// their units' feet (`position_of`). All pink and added on.
    pub fn meshes(&self, now: Instant, yaw: f32, position_of: impl Fn(Option<u32>) -> Option<Vector>) -> Vec<Wall> {
        let mut walls = Vec::new();
        for hearts in &self.hearts {
            let Some(t) = passes(hearts.start, now).checked_sub(1) else { continue };
            let n = t + 1;
            let mut vertices = Vec::new();
            for flower in &hearts.flowers {
                let [dx, dz] = heart_offset(flower.point, yaw);
                for (index, ring) in flower.rings.iter().enumerate() {
                    let alpha = heart_alpha(n, hearts.length);
                    if alpha <= 0.0 {
                        continue;
                    }
                    let tint = [PINK[0], PINK[1], PINK[2], alpha / 255.0];
                    let (lean_sin, lean_cos) = ring.lean.to_radians().sin_cos();
                    let corner = |s: u32, top: bool, uv: [f32; 2]| {
                        let (sin, cos) = ((ring.turn + 36.0 * s as f32) % 360.0).to_radians().sin_cos();
                        let (reach, up) = if top {
                            let p = heart_profile(index == 1, ring.height, s, n);
                            (ring.radius + p * lean_cos, p * lean_sin)
                        } else {
                            (ring.radius, 0.0)
                        };
                        MeshVertex { position: [hearts.at[0] + dx + reach * cos, hearts.at[1] + up, hearts.at[2] + dz + reach * sin], uv, tint }
                    };
                    for s in 1..=10 {
                        let (u0, u1) = ((s - 1) as f32 / 21.0, s as f32 / 21.0);
                        let [a, b, c, d] =
                            [corner(s - 1, false, [u0, 1.0]), corner(s, false, [u1, 1.0]), corner(s, true, [u1, 0.0]), corner(s - 1, true, [u0, 0.0])];
                        vertices.extend([a, b, c, a, c, d]);
                    }
                }
            }
            walls.push(Wall { texture: RING_RED, vertices, additive: true, forced: false });
        }
        for cloud in &self.clouds {
            let Some(feet) = position_of(cloud.unit) else { continue };
            let vertices: Vec<MeshVertex> = cloud.ribbons.iter().filter(|ribbon| ribbon.alpha > 0.0).flat_map(|ribbon| ribbon.mesh(feet)).collect();
            walls.push(Wall { texture: FIRE_IVY, vertices, additive: true, forced: false });
        }
        walls.retain(|wall| !wall.vertices.is_empty());
        walls
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
    fn im_all_yours_puts_twenty_flame_flowers_on_a_heart() {
        let start = Instant::now();
        let feet = [10.0, 1.0, 20.0];
        let mut sphere = Sphere::default();
        let mut draws = Vec::new();
        sphere.heart(Some(1), feet, 0, start, |n| {
            draws.push(n);
            0
        });
        // Twenty flowers, two draws of R(360) each, two rings: 12 and 25 tall, radius 2.3 and 2.0, lean 86 and 90.
        assert_eq!(draws.len(), 40);
        let hearts = &sphere.hearts[0];
        assert_eq!(hearts.length, 1000, "no cast time");
        assert_eq!(hearts.flowers.len(), 20);
        assert_eq!((hearts.flowers[0].point, hearts.flowers[19].point), ([0.0, 9.99], [3.78, 11.88]));
        let rings = &hearts.flowers[0].rings;
        assert_eq!((rings[0].height, rings[0].radius, rings[0].lean, rings[1].height, rings[1].radius, rings[1].lean), (12.0, 2.3, 86.0, 25.0, 2.0, 90.0));
        // Alpha at n = 1, 15, 16: 5, 75, 75; held to 975 and 73 at 976 for a length of 1 000.
        assert_eq!([1, 15, 16, 975, 976, 1000].map(|n| heart_alpha(n, 1000)), [5.0, 75.0, 75.0, 75.0, 73.0, 25.0]);
        // The profile at n = 1 (sin 1° scaled) and 91 (unscaled): for s = 0, ring 0 of height 12 at n = 91.
        assert!(near(heart_profile(false, 12.0, 0, 1), (3.0 * (4.0_f32).to_radians().sin() + 9.0) * 1.0_f32.to_radians().sin()));
        assert!(near(heart_profile(false, 12.0, 0, 91), 3.0 * (364.0_f32).to_radians().sin() + 9.0));
        // Ten quads a ring, twenty rings × two, all pink and added on, and they stay where the caster stood.
        let walls = sphere.meshes(start, 0.0, |_| Some([99.0; 3]));
        assert_eq!(walls.len(), 1);
        assert!(walls[0].additive && walls[0].vertices.len() == 20 * 2 * 10 * 6 && walls[0].vertices[0].tint[..3] == PINK);
        assert!(walls[0].vertices.iter().all(|vertex| vertex.position[0] < 40.0));
        // The heart keeps its orientation on the screen: (0, 9.99) is at the top, (0, −9.99) in the client's axes, at longitude 0;
        // at 90° it is (9.99, 0).
        let [x, z] = heart_offset([0.0, 9.99], 0.0);
        assert!(near(x, 0.0) && near(z, -9.99));
        let [x, z] = heart_offset([0.0, 9.99], 90.0);
        assert!(near(x, 9.99) && near(z, 0.0));
        // It ends with the cast (here 100 ticks), is replaced by the caster's next, and is cut with the cast.
        let mut cast = Sphere::default();
        cast.heart(Some(1), feet, 100, start, |_| 0);
        cast.heart(Some(1), feet, 100, start, |_| 0);
        assert_eq!(cast.hearts.len(), 1);
        assert!(
            cast.meshes(start + TICK * 99, 0.0, |_| None).len() == 1 && {
                cast.advance(start + TICK * 100, |_| 0);
                cast.hearts.is_empty()
            }
        );
        cast.heart(Some(2), feet, 100, start, |_| 0);
        cast.cut_hearts();
        assert!(cast.hearts.is_empty());
    }

    #[test]
    fn baby_swells_a_sphere_of_twenty_tumbling_ribbons() {
        let start = Instant::now();
        let feet = [10.0, 1.0, 20.0];
        let mut sphere = Sphere::default();
        let mut draws = Vec::new();
        sphere.ribbons(Some(1), start, |n| {
            draws.push(n);
            0
        });
        // Five prims of four bands, five draws each in order: wait, half-width, turn, arc, speed.
        let ribbons = &sphere.clouds[0].ribbons;
        assert_eq!(ribbons.len(), 20);
        assert_eq!(draws[..5], [20, 11, 360, 91, 4]);
        // Every draw 0: no wait, half-width 2.0, arc 180, speed 3; the tilts 0, 9 … 171.
        assert!(ribbons.iter().all(|ribbon| ribbon.age == 0 && ribbon.half == 2.0 && ribbon.arc == 180.0 && ribbon.speed == 3.0 && ribbon.radius == 5.0));
        let mut tilts: Vec<i32> = ribbons.iter().map(|ribbon| ribbon.tilt).collect();
        tilts.sort_unstable();
        assert_eq!(tilts, (0..20).map(|k| 9 * k).collect::<Vec<_>>());
        // The largest draws: wait 19, half-width 4.0, arc 270, speed 6.
        let mut high = Sphere::default();
        high.ribbons(None, start, |n| n - 1);
        let ribbon = &high.clouds[0].ribbons[0];
        assert!(ribbon.age == -19 && near(ribbon.half, 4.0) && ribbon.arc == 270.0 && ribbon.speed == 6.0);
        // Alpha at n = 1, 25, 26, 70, 71, 95; the radius at 1, 25, 70 (5.05, 6.41, 10.03).
        assert_eq!([1, 25, 26, 70, 71, 95].map(ribbon_alpha), [2.0, 50.0, 50.0, 50.0, 48.0, 0.0]);
        let mut ribbon = Ribbon::new(0, 0, &mut |_| 0);
        let mut radii = Vec::new();
        for n in 1..=70 {
            ribbon.step(&mut |_| 1);
            if [1, 25, 70].contains(&n) {
                radii.push((ribbon.radius * 100.0).round() / 100.0);
            }
        }
        assert_eq!(radii, [5.05, 6.41, 10.03]);
        // The tilt creeps 1 degree with R(2) = 1 and 2 with R(2) = 0, once a band each tick; the turn goes round at its speed.
        let mut creep = Ribbon::new(0, 0, &mut |_| 0);
        creep.step(&mut |_| 1);
        creep.step(&mut |_| 0);
        assert_eq!((creep.tilt, creep.turn), (3, 6.0));
        // A waiting band does nothing and draws nothing.
        let mut waiting = Ribbon::new(0, 0, &mut |n| if n == 20 { 5 } else { 0 });
        let mut asked = 0;
        for _ in 0..5 {
            waiting.step(&mut |_| {
                asked += 1;
                0
            });
        }
        assert_eq!((waiting.age, waiting.alpha, asked, waiting.radius), (0, 0.0, 0, 5.0));
        // Follows the target; gone at tick 200.
        for tick in 0..60 {
            sphere.advance(start + TICK * tick, |_| 1);
        }
        let walls = sphere.meshes(start + TICK * 59, 0.0, |_| Some(feet));
        assert_eq!(walls.len(), 1);
        assert!(
            walls[0].texture == FIRE_IVY && walls[0].additive && walls[0].vertices.iter().all(|vertex| vertex.position[0] > 0.0 && vertex.position[0] < 30.0)
        );
        assert!(sphere.meshes(start + TICK * 59, 0.0, |_| Some([100.0, 1.0, 100.0]))[0].vertices.iter().all(|vertex| vertex.position[0] > 80.0));
        assert!(sphere.meshes(start, 0.0, |_| None).is_empty());
        sphere.advance(start + TICK * 201, |_| 1);
        assert!(sphere.clouds.is_empty());
    }
}
