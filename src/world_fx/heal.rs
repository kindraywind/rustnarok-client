//! The heal object of effect packets and scripts (253, 312, 313, 320, 325, 1143, 1144): crowns of light round the unit,
//! built from rings and leaning walls of 21 segments each, with sparkles rising through them.

use super::{Assets, Class, FLAGS_EFFECT, Face, Fx, Seen, Tex, WorldFx, triangles};
use crate::world::Vector;
use crate::world_str::Scene;

const IDS: [u32; 7] = [253, 312, 313, 320, 325, 1143, 1144];

/// Whether an effect id is a heal object: its packets start it, and the table's part, ring and band rows stay out of it.
pub fn is_class(effect: u32) -> bool {
    IDS.contains(&effect)
}

/// Registers the heal object under its seven ids.
pub fn register(fx: &mut WorldFx) {
    for id in IDS {
        fx.register(id, || Box::new(Heal::new(pick)));
    }
}

/// One random whole number below `n`.
fn pick(n: u32) -> u32 {
    crate::effects::pick(0, n.saturating_sub(1))
}

const RING_WHITE: &str = "data\\texture\\effect\\ring_white.tga";
const RING_BLUE: &str = "data\\texture\\effect\\ring_blue.tga";
const ALPHA_DOWN: &str = "data\\texture\\effect\\alpha_down.tga";
const POK1: &str = "data\\texture\\effect\\pok1.tga";
const POK3: &str = "data\\texture\\effect\\pok3.tga";

/// How long the object lives, in ticks.
const LENGTH: u32 = 100;
/// Segments to a pass of a ring or a wall.
const SEGMENTS: usize = 21;
/// The alpha cap on ecl_fild01.
const DIM_ALPHA: i32 = 100;

fn rgb(hex: u32) -> [f32; 3] {
    [(hex >> 16) as u8, (hex >> 8) as u8, hex as u8].map(|byte| f32::from(byte) / 255.0)
}

/// What one builder call makes at an update.
#[derive(Debug, Default, PartialEq)]
struct Plan {
    rings: Vec<u8>,
    walls: Vec<u8>,
    sparkles: Vec<u8>,
}

/// The builder calls of heal object `id` at its `n`th update (0 on the first).
fn plan(id: u32, n: u32) -> Plan {
    let mut plan = Plan::default();
    let every = |step: u32, last: u32| n <= last && n.is_multiple_of(step);
    match id {
        253 if n == 0 => (plan.rings, plan.walls) = (vec![5], vec![5]),
        312 => {
            if n == 0 {
                plan.rings.push(4);
            }
            if every(6, 50) {
                plan.sparkles.push(4);
            }
        }
        313 => {
            if n == 0 {
                (plan.rings, plan.walls) = (vec![0], vec![0]);
            }
            if every(6, 50) {
                plan.sparkles.push(0);
            }
        }
        320 => {
            if n == 0 {
                plan.rings.push(3);
            }
            if every(6, 25) {
                plan.sparkles.push(3);
            }
        }
        325 => {
            if n < 2 {
                plan.rings.push(n as u8 + 1);
                plan.walls.push(n as u8 + 1);
            }
            if every(6, 50) {
                plan.sparkles.push(1);
            }
            if n <= 50 && n % 9 == 2 {
                plan.sparkles.push(2);
            }
        }
        1143 | 1144 => {
            let v = if id == 1143 { 7 } else { 8 };
            if n == 0 {
                plan.rings.push(v);
            }
            if every(6, 50) {
                plan.sparkles.push(v);
            }
        }
        _ => {}
    }
    plan
}

/// A ring's or a wall's texture, colour and fade age.
struct Look {
    texture: &'static str,
    color: [f32; 3],
    fade: u32,
}

fn ring_look(v: u8) -> Look {
    let (texture, color, fade) = match v {
        0 => (RING_WHITE, 0x8CD28C, 60),
        1 => (RING_WHITE, 0x8CD28C, 40),
        2 => (ALPHA_DOWN, 0x8CD28C, 40),
        3 => (ALPHA_DOWN, 0xFFFFFF, 30),
        4 => (ALPHA_DOWN, 0x8CD28C, 60),
        5 => (RING_BLUE, 0x6464FF, 40),
        7 => (ALPHA_DOWN, 0x00FFFF, 60),
        _ => (ALPHA_DOWN, 0xFFFF00, 60),
    };
    Look { texture, color: rgb(color), fade }
}

fn wall_look(v: u8) -> Look {
    let (texture, color, fade) = match v {
        0 => (RING_WHITE, 0x8CD28C, 60),
        1 => (RING_WHITE, 0x8CD28C, 40),
        2 => (ALPHA_DOWN, 0x8CD28C, 30),
        _ => (RING_BLUE, 0x6464FF, 40),
    };
    Look { texture, color: rgb(color), fade }
}

fn sin(degrees: u32) -> f32 {
    (degrees as f32).to_radians().sin()
}

/// A segment's alpha after its update at age `e`: it fades 2 a tick from the fade age on, else grows 5 a tick for 16 ticks.
fn alpha_after(alpha: i32, e: u32, fade: u32) -> i32 {
    if e >= fade {
        (alpha - 2).max(0)
    } else if e < 16 {
        (alpha + 5).min(180)
    } else {
        alpha
    }
}

/// The wave angle (degrees) of segment `j` of pass `pass` at age `e`.
fn wave(wall: bool, pass: u32, e: u32, j: u32) -> u32 {
    (if wall { 2 * e } else { (4 - pass) * e } + 34 * j) % 360
}

/// A segment's height: `top` scaled by the wave and, up to age 90, by the grow-in.
fn height(top: f32, d: u32, e: u32) -> f32 {
    (top * (0.75 + 0.25 * sin(d)) * if e <= 90 { sin(e) } else { 1.0 }).max(0.0)
}

/// A segment's bottom (on the ground round the unit) and its top, leaning `lean` degrees up from the ground.
fn ends(radius: f32, theta: f32, h: f32, lean: f32) -> (Vector, Vector) {
    let ((sin_t, cos_t), (sin_l, cos_l)) = (theta.to_radians().sin_cos(), lean.to_radians().sin_cos());
    let bottom = [radius * cos_t, 0.0, radius * sin_t];
    (bottom, [bottom[0] + h * cos_l * cos_t, h * sin_l, bottom[2] + h * cos_l * sin_t])
}

/// One segment of a ring or a wall.
#[derive(Debug, Clone)]
struct Segment {
    pass: u32,
    j: u32,
    /// The height the wave scales.
    tall: f32,
    start: f32,
    radius: f32,
    lean: f32,
    /// Its own age, 0 at its first update.
    e: u32,
    alpha: i32,
    bottom: Vector,
    top: Vector,
}

impl Segment {
    fn new(pass: u32, j: u32, tall: f32, start: f32, radius: f32, lean: f32) -> Self {
        Self { pass, j, tall, start, radius, lean, e: 0, alpha: 0, bottom: [0.0; 3], top: [0.0; 3] }
    }

    fn step(&mut self, wall: bool, fade: u32) {
        self.alpha = alpha_after(self.alpha, self.e, fade);
        let h = height(self.tall, wave(wall, self.pass, self.e, self.j), self.e);
        (self.bottom, self.top) = ends(self.radius, 18.0 * self.j as f32 + self.start, h, self.lean);
        self.e += 1;
    }
}

/// The 42 segments of one ring or wall.
struct Band {
    look: Look,
    wall: bool,
    segments: Vec<Segment>,
}

/// A ring of variant `v`: two passes of 21 segments standing straight up.
// ponytail: the draws of a pass come in the table's column order (height, start angle, radius), a per-segment height after them.
fn ring(v: u8, rand: &mut impl FnMut(u32) -> u32) -> Band {
    let shared = (v == 3).then(|| 40.0 + rand(20) as f32);
    let mut segments = Vec::new();
    for pass in 0..2u32 {
        let p = pass as f32;
        let start = if matches!(v, 0 | 1 | 2 | 5) { 180.0 * p } else { rand(360) as f32 };
        let radius = match v {
            0 | 1 | 5 => 4.6 + 0.4 * p,
            2 => 4.2 + 0.1 * p,
            3 => 4.5 + 0.2 * p + 0.005 * rand(100) as f32,
            _ => 4.6 + 0.2 * p,
        };
        for j in 0..SEGMENTS as u32 {
            let top = match v {
                0 | 1 | 5 => 60.0,
                2 => 90.0,
                3 => shared.unwrap_or(40.0),
                _ if pass == 0 => 40.0,
                _ => 40.0 + rand(20) as f32,
            };
            segments.push(Segment::new(pass, j, top, start, radius, 90.0));
        }
    }
    Band { look: ring_look(v), wall: false, segments }
}

/// A wall of variant `v`: two passes of 21 segments leaning outward.
fn wall(v: u8, rand: &mut impl FnMut(u32) -> u32) -> Band {
    let mut segments = Vec::new();
    for pass in 0..2u32 {
        let p = pass as f32;
        let start = rand(360) as f32;
        let (top, radius) = match v {
            0 => (12.0, 5.0 + 0.2 * p),
            2 => (10.0, 5.3 + 0.1 * p),
            _ => (18.0, 5.0 + 0.2 * p),
        };
        let lean = if v == 2 { 46.0 - 2.0 * p } else { 50.0 - 2.0 * p };
        segments.extend((0..SEGMENTS as u32).map(|j| Segment::new(pass, j, top, start, radius, lean)));
    }
    Band { look: wall_look(v), wall: true, segments }
}

/// A rising sparkle.
#[derive(Debug, Clone)]
struct Sparkle {
    v: u8,
    k: u32,
    /// Its own age, 0 at its first update.
    e: u32,
    alpha: i32,
    spin: u32,
    height: f32,
    /// The offset from the unit's feet on x and z.
    offset: [f32; 2],
    /// The half-diagonal.
    size: f32,
    rise: f32,
    pok3: bool,
}

impl Sparkle {
    fn color(&self) -> [f32; 3] {
        rgb(match self.v {
            7 => 0x00FFFF,
            8 => 0xFFFF00,
            _ => 0xDCFADC,
        })
    }

    fn step(&mut self) {
        self.spin = (self.spin + 355) % 360;
        if self.e <= 10 {
            self.alpha += 15;
        } else if self.v != 4 {
            self.pok3 = self.e + 7 * self.k == 35 || self.e + 7 * self.k == 36;
        }
        self.alpha = (self.alpha - 3).max(0);
        self.height += self.rise;
        self.e += 1;
    }
}

/// One builder call's four sparkles.
fn sparkles(v: u8, rand: &mut impl FnMut(u32) -> u32) -> Vec<Sparkle> {
    (0..4)
        .map(|k| {
            let z = rand(13) as f32 - 6.0;
            let x = rand(13) as f32 - 6.0;
            let size = if v == 2 { 0.4 + 0.1 * rand(40) as f32 } else { 2.0 + 0.1 * rand(10) as f32 };
            let rise = match v {
                1 => 0.6,
                4 => 0.3,
                _ => 0.4,
            } + 0.01 * rand(40) as f32;
            Sparkle { v, k, e: 0, alpha: 0, spin: 0, height: 0.0, offset: [x, z], size, rise, pok3: false }
        })
        .collect()
}

/// The heal object.
struct Heal {
    id: u32,
    rand: fn(u32) -> u32,
    rings: Vec<Band>,
    walls: Vec<Band>,
    sparkles: Vec<Sparkle>,
    /// One fade age for all the rings and one for all the walls: the latest builder call's.
    ring_fade: u32,
    wall_fade: u32,
}

impl Heal {
    fn new(rand: fn(u32) -> u32) -> Self {
        Self { id: 0, rand, rings: Vec::new(), walls: Vec::new(), sparkles: Vec::new(), ring_fade: 0, wall_fade: 0 }
    }
}

impl Class for Heal {
    fn begin(&mut self, fx: &mut Fx, assets: &mut dyn Assets) {
        self.id = fx.id;
        fx.length = LENGTH;
        for path in [RING_WHITE, RING_BLUE, ALPHA_DOWN, POK1, POK3] {
            assets.texture(path);
        }
    }

    fn update(&mut self, fx: &mut Fx, _: &Scene, _: &mut dyn Assets) {
        let mut rand = self.rand;
        let plan = plan(self.id, fx.age);
        for v in plan.rings {
            self.ring_fade = ring_look(v).fade;
            self.rings.push(ring(v, &mut rand));
        }
        for v in plan.walls {
            self.wall_fade = wall_look(v).fade;
            self.walls.push(wall(v, &mut rand));
        }
        for v in plan.sparkles {
            self.sparkles.extend(sparkles(v, &mut rand));
        }
        for (bands, fade) in [(&mut self.rings, self.ring_fade), (&mut self.walls, self.wall_fade)] {
            for band in bands {
                let wall = band.wall;
                band.segments.iter_mut().for_each(|segment| segment.step(wall, fade));
            }
        }
        self.sparkles.iter_mut().for_each(Sparkle::step);
    }

    fn draw<'a>(&'a self, fx: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        let at = |v: Vector| std::array::from_fn::<f32, 3, _>(|axis| v[axis] + fx.position[axis]);
        for band in self.rings.iter().chain(&self.walls) {
            for pass in 0..2 {
                for j in 1..SEGMENTS {
                    let [before, this] = [j - 1, j].map(|j| &band.segments[pass * SEGMENTS + j]);
                    if this.alpha <= 0 && before.alpha <= 0 {
                        continue;
                    }
                    let corners = [at(before.top), at(this.top), at(before.bottom), at(this.bottom)];
                    let [u0, u1] = [(j - 1) as f32 / SEGMENTS as f32, j as f32 / SEGMENTS as f32];
                    let tint = [band.look.color[0], band.look.color[1], band.look.color[2], this.alpha as f32 / 255.0];
                    out.push(Face {
                        texture: Tex::Path(band.look.texture),
                        vertices: triangles(corners, [[u0, 0.0], [u1, 0.0], [u0, 1.0], [u1, 1.0]], tint),
                        additive: true,
                        flags: FLAGS_EFFECT,
                    });
                }
            }
        }
        for sparkle in &self.sparkles {
            let alpha = if seen.dim { sparkle.alpha.min(DIM_ALPHA) } else { sparkle.alpha };
            if alpha <= 0 {
                continue;
            }
            let point = at([sparkle.offset[0], sparkle.height, sparkle.offset[1]]);
            // The corners go round the point at spin + 90 j; the picture's (0, 1), (1, 1), (1, 0), (0, 0) follow them.
            let corner: [Vector; 4] = std::array::from_fn(|j| {
                let (sin, cos) = ((sparkle.spin + 90 * j as u32) as f32).to_radians().sin_cos();
                std::array::from_fn(|axis| point[axis] + sparkle.size * (cos * seen.eye.right[axis] - sin * seen.eye.up[axis]))
            });
            let color = sparkle.color();
            let tint = [color[0], color[1], color[2], alpha as f32 / 255.0];
            out.push(Face {
                texture: Tex::Path(if sparkle.pok3 { POK3 } else { POK1 }),
                vertices: triangles([corner[0], corner[1], corner[3], corner[2]], [[0.0, 1.0], [1.0, 1.0], [0.0, 0.0], [1.0, 0.0]], tint),
                additive: true,
                flags: FLAGS_EFFECT,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world_fx::{Eye, Launch};
    use std::collections::HashMap;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    fn zero(_: u32) -> u32 {
        0
    }

    /// The updates in `ages` where a builder call makes variant `v`'s sparkles.
    fn calls(id: u32, ages: std::ops::Range<u32>, v: u8) -> Vec<u32> {
        ages.filter(|&n| plan(id, n).sparkles.contains(&v)).collect()
    }

    #[test]
    fn the_builder_table() {
        let made = |id: u32| -> Vec<(u32, Vec<u8>, Vec<u8>)> {
            (0..120).map(|n| (n, plan(id, n))).filter(|(_, p)| !p.rings.is_empty() || !p.walls.is_empty()).map(|(n, p)| (n, p.rings, p.walls)).collect()
        };
        assert_eq!(made(253), [(0, vec![5], vec![5])]);
        assert_eq!(made(312), [(0, vec![4], vec![])]);
        assert_eq!(made(313), [(0, vec![0], vec![0])]);
        assert_eq!(made(320), [(0, vec![3], vec![])]);
        assert_eq!(made(325), [(0, vec![1], vec![1]), (1, vec![2], vec![2])]);
        assert_eq!(made(1143), [(0, vec![7], vec![])]);
        assert_eq!(made(1144), [(0, vec![8], vec![])]);
        let nines: Vec<u32> = (0..=48).step_by(6).collect();
        for (id, v) in [(312, 4), (313, 0), (1143, 7), (1144, 8)] {
            assert_eq!(calls(id, 0..120, v), nines, "{id}");
            assert_eq!(calls(id, 0..120, v).len(), 9);
        }
        assert_eq!(calls(320, 0..120, 3), [0, 6, 12, 18, 24]);
        assert_eq!(calls(325, 0..120, 1), nines);
        assert_eq!(calls(325, 0..120, 2), [2, 11, 20, 29, 38, 47]);
        assert!((0..120).all(|n| plan(253, n).sparkles.is_empty()));
        assert!(!nines.contains(&54));
    }

    #[test]
    fn a_ring_segments_alpha() {
        let run = |fade: u32| {
            let mut alpha = 0;
            (0..110)
                .map(|e| {
                    alpha = alpha_after(alpha, e, fade);
                    alpha
                })
                .collect::<Vec<_>>()
        };
        let v0 = run(60);
        assert_eq!([v0[0], v0[14], v0[15], v0[16]], [5, 75, 80, 80]);
        assert_eq!([v0[59], v0[60], v0[99]], [80, 78, 0]);
        let v3 = run(30);
        assert_eq!([v3[29], v3[30], v3[69]], [80, 78, 0]);
        assert!(v3[70..].iter().all(|&alpha| alpha == 0));
    }

    #[test]
    fn the_heights_and_the_wave() {
        assert_eq!(height(60.0, wave(false, 0, 0, 0), 0), 0.0);
        for e in [90, 91] {
            let d = wave(false, 0, e, 0);
            assert!(close(height(60.0, d, e), 60.0 * (0.75 + 0.25 * sin(d))), "{e}");
        }
        // Pass p at e = 10, j = 3.
        assert_eq!([0, 1].map(|p| wave(false, p, 10, 3)), [4 * 10 + 102, 3 * 10 + 102]);
        assert_eq!(wave(true, 0, 10, 3), 2 * 10 + 102);
        assert_eq!(wave(false, 0, 100, 10), (400 + 340) % 360);
        // A wall's top for L = 50 at θ = 0.
        let (bottom, top) = ends(5.0, 0.0, 10.0, 50.0);
        assert!(close(top[0] - bottom[0], 10.0 * 50f32.to_radians().cos()) && close(top[1], 10.0 * 50f32.to_radians().sin()) && close(top[2], bottom[2]));
        // A ring stands straight up.
        let (bottom, top) = ends(4.6, 36.0, 7.0, 90.0);
        assert!(close(top[0], bottom[0]) && close(top[1], 7.0) && close(top[2], bottom[2]));
    }

    #[test]
    fn rings_and_walls_by_variant() {
        let mut rand = zero;
        for v in [0, 1, 2, 3, 4, 5, 7, 8] {
            let band = ring(v, &mut rand);
            assert_eq!(band.segments.len(), 42, "{v}");
            assert!(band.segments.iter().all(|segment| segment.lean == 90.0));
        }
        let band = ring(0, &mut rand);
        assert_eq!([band.segments[0].start, band.segments[21].start, band.segments[0].radius, band.segments[21].radius], [0.0, 180.0, 4.6, 5.0]);
        assert_eq!(ring(2, &mut rand).segments[0].tall, 90.0);
        // Variant 3: one height for both passes, a start and a radius draw each.
        let mut script = [7, 123, 40, 9, 50].into_iter();
        let band = ring(3, &mut |_| script.next().unwrap());
        assert_eq!((band.segments[0].tall, band.segments[21].tall), (47.0, 47.0));
        assert_eq!((band.segments[0].start, band.segments[21].start), (123.0, 9.0));
        assert!(close(band.segments[0].radius, 4.5 + 0.005 * 40.0) && close(band.segments[21].radius, 4.7 + 0.005 * 50.0));
        // Variant 4: pass 0 is 40, pass 1 draws a height for each segment.
        let mut draws = 0;
        let band = ring(4, &mut |n| {
            draws += 1;
            if n == 20 { 5 } else { 0 }
        });
        assert!(band.segments[..21].iter().all(|segment| segment.tall == 40.0));
        assert!(band.segments[21..].iter().all(|segment| segment.tall == 45.0));
        assert_eq!(draws, 2 + 21);
        assert!(close(band.segments[21].radius, 4.8));
        // Walls lean 50 and 48 (variant 2: 46 and 44) and start at a random angle each pass.
        for (v, leans, top, radii) in
            [(0, [50.0, 48.0], 12.0, [5.0, 5.2]), (1, [50.0, 48.0], 18.0, [5.0, 5.2]), (2, [46.0, 44.0], 10.0, [5.3, 5.4]), (5, [50.0, 48.0], 18.0, [5.0, 5.2])]
        {
            let band = wall(v, &mut |n| n - 1);
            assert_eq!(band.segments.len(), 42);
            assert_eq!([band.segments[0].lean, band.segments[21].lean], leans, "{v}");
            assert!(band.segments.iter().all(|segment| segment.tall == top && segment.start == 359.0));
            assert!(close(band.segments[0].radius, radii[0]) && close(band.segments[21].radius, radii[1]));
        }
    }

    #[test]
    fn the_fade_ages_of_325() {
        let mut object = Heal::new(zero);
        object.id = 325;
        let mut state = fx_of(325);
        for n in 0..2 {
            state.age = n;
            object.update(&mut state, &scene_of(), &mut Fake);
        }
        // The second call sets the rings' fade age to 40 again and the walls' to 30, which holds for the first walls too.
        assert_eq!((object.ring_fade, object.wall_fade), (40, 30));
        assert_eq!((object.rings.len(), object.walls.len()), (2, 2));
    }

    #[test]
    fn the_sparkles_draws_and_ranges() {
        // Every draw at its top (n − 1) and at 0.
        let high = sparkles(0, &mut |n| n - 1);
        let low = sparkles(0, &mut zero);
        assert_eq!(high.len(), 4);
        assert_eq!((low[0].offset, high[0].offset), ([-6.0, -6.0], [6.0, 6.0]));
        assert!(close(low[0].size, 2.0) && close(high[0].size, 2.9) && close(low[0].rise, 0.4) && close(high[0].rise, 0.79));
        let rises = |v: u8| (sparkles(v, &mut zero)[0].rise, sparkles(v, &mut |n| n - 1)[0].rise);
        assert!(close(rises(1).0, 0.6) && close(rises(1).1, 0.99));
        assert!(close(rises(4).0, 0.3) && close(rises(4).1, 0.69));
        assert!(close(rises(7).0, 0.4) && close(rises(8).1, 0.79) && close(rises(3).0, 0.4));
        let (low2, high2) = (sparkles(2, &mut zero).remove(0), sparkles(2, &mut |n| n - 1).remove(0));
        assert!(close(low2.size, 0.4) && close(high2.size, 0.4 + 3.9) && close(low2.rise, 0.4));
        // The z draw comes first, then x.
        let mut script = [3, 9, 0, 0].into_iter().chain(std::iter::repeat(0));
        let first = sparkles(0, &mut |_| script.next().unwrap())[0].offset;
        assert_eq!(first, [3.0, -3.0]);
        assert_eq!(sparkles(7, &mut zero)[0].color(), rgb(0x00FFFF));
        assert_eq!(sparkles(8, &mut zero)[0].color(), rgb(0xFFFF00));
        assert_eq!(sparkles(0, &mut zero)[0].color(), rgb(0xDCFADC));
    }

    #[test]
    fn a_sparkles_alpha_picture_and_spin() {
        let run = |v: u8, k: u32| {
            let mut sparkle = sparkles(v, &mut zero).remove(k as usize);
            (0..60)
                .map(|_| {
                    sparkle.step();
                    sparkle.clone()
                })
                .collect::<Vec<_>>()
        };
        let steps = run(0, 0);
        // The state after the update at age e is steps[e].
        assert_eq!([0, 10, 11, 54].map(|e| steps[e].alpha), [12, 132, 129, 0]);
        assert_eq!([0, 1, 2, 11].map(|e| steps[e].spin), [355, 350, 345, 300]);
        for k in 0..4u32 {
            for (e, sparkle) in run(0, k).iter().enumerate().skip(11) {
                assert_eq!(sparkle.pok3, matches!(e as u32 + 7 * k, 35 | 36), "k {k} e {e}");
            }
        }
        assert!(steps[35].pok3 && steps[36].pok3 && !steps[34].pok3 && !steps[37].pok3);
        assert!(run(4, 0).iter().all(|sparkle| !sparkle.pok3));
        assert!(close(steps[9].height, 10.0 * steps[0].rise));
    }

    #[test]
    fn sparkles_are_capped_on_ecl_fild01() {
        let mut object = Heal::new(zero);
        object.sparkles = sparkles(0, &mut zero);
        for _ in 0..11 {
            object.sparkles.iter_mut().for_each(Sparkle::step);
        }
        let fx = fx_of(312);
        let alpha_of = |dim: bool| {
            let mut faces = Vec::new();
            let (shows, hidden) = (|_: u32| true, |_: Option<u32>| false);
            object.draw(&fx, &Seen { eye: Eye::at(45.0, 0.0), shows: &shows, hidden: &hidden, size: &|_| 1.0, ground: &|_, _| 0.0, dim }, &mut faces);
            faces[0].vertices[0].tint[3]
        };
        assert!(close(alpha_of(false), 132.0 / 255.0) && close(alpha_of(true), 100.0 / 255.0));
    }

    #[test]
    fn a_ring_quad_has_the_segment_uv() {
        let mut object = Heal::new(zero);
        object.id = 253;
        let mut fx = fx_of(253);
        object.update(&mut fx, &scene_of(), &mut Fake);
        fx.position = [10.0, 0.0, 20.0];
        let mut faces = Vec::new();
        let (shows, hidden) = (|_: u32| true, |_: Option<u32>| false);
        let seen = Seen { eye: Eye::at(45.0, 0.0), shows: &shows, hidden: &hidden, size: &|_| 1.0, ground: &|_, _| 0.0, dim: false };
        object.draw(&fx, &seen, &mut faces);
        // Rings and walls of 2 passes × 20 quads; the first quad's uv is j = 1.
        assert_eq!(faces.len(), 80);
        let uv: Vec<[f32; 2]> = faces[0].vertices.iter().map(|vertex| vertex.uv).collect();
        let (u1, zero_u) = (1.0 / 21.0, 0.0);
        assert_eq!(uv, [[zero_u, 0.0], [u1, 0.0], [zero_u, 1.0], [zero_u, 1.0], [u1, 0.0], [u1, 1.0]]);
        assert!(faces.iter().all(|face| face.additive && face.flags == FLAGS_EFFECT));
        // Everything is built round the unit's feet.
        assert!(close(faces[0].vertices[2].position[1], 0.0) && faces[0].vertices[2].position[0] > 5.0);
    }

    #[test]
    fn the_object_lives_100_ticks_follows_its_unit_and_copies() {
        let mut fx = WorldFx::default();
        let first = fx.launch(&Launch::at(312, Some(Some(7)), [0.0; 3]), &mut Fake).unwrap();
        let second = fx.launch(&Launch::at(312, Some(Some(7)), [0.0; 3]), &mut Fake).unwrap();
        assert_ne!(first, second);
        assert_eq!(fx.get_mut(first).unwrap().fx.length, 100);
        let units = HashMap::from([(7, [4.0, 1.0, 9.0])]);
        run_ticks(&mut fx, 99, &units);
        assert_eq!(fx.get_mut(first).unwrap().fx.position, [4.0, 1.0, 9.0]);
        assert_eq!(fx.iter().count(), 2);
        run_ticks(&mut fx, 2, &units);
        assert!(fx.is_empty());
        // The sparkles of the calls made by age 50 are cut with it.
        for id in IDS {
            assert!(fx.registered(id) && is_class(id));
        }
        assert!(!is_class(314));
    }

    struct Fake;

    impl Assets for Fake {
        fn texture(&mut self, _: &str) -> Option<[f32; 2]> {
            Some([64.0; 2])
        }
        fn str(&mut self, _: &str) -> Option<std::rc::Rc<crate::effects::Loaded>> {
            None
        }
        fn sprite(&mut self, _: &str, _: usize, _: usize) -> Option<(usize, Vec<crate::actor::Placed>)> {
            None
        }
    }

    fn fx_of(id: u32) -> Fx {
        Fx {
            id,
            owner: None,
            position: [0.0; 3],
            aim: [0.0; 3],
            target: None,
            age: 0,
            length: LENGTH,
            a7: -1,
            single: false,
            tag: 0,
            life: 0,
            far: false,
            events: Vec::new(),
            updated: false,
        }
    }

    fn scene_of() -> Scene<'static> {
        Scene { player: [0.0; 3], feet: &|_| Some([0.0; 3]), body: &|_| None, longitude: 0.0, right: [1.0, 0.0, 0.0], up: [0.0, 1.0, 0.0], ground: &|_, _| 0.0 }
    }

    fn run_ticks(fx: &mut WorldFx, count: usize, units: &HashMap<u32, Vector>) {
        let feet = |unit: Option<u32>| match unit {
            None => Some([0.0; 3]),
            Some(id) => units.get(&id).copied(),
        };
        let scene = Scene { feet: &feet, ..scene_of() };
        for _ in 0..count {
            fx.tick(&scene, &mut Fake);
        }
    }
}
