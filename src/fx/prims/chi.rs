//! Exceed Break's cast (882) and the Royal Guard's buffs (908 to 910): 160 needles converge on the unit, and Hesperus Lit's and Ride
//! in Lightning's thin light pillars (899), a 16 ms tick at a time.
//!
//! A particle's own age n counts from 1 on its first tick after its wait; e is the ticks since the effect started.

use std::time::Instant;

use super::passes;
use super::walls::Wall;
use crate::world::{MeshVertex, Vector};

pub const NEEDLE: &str = "data\\texture\\effect\\alpha_center.tga";
const PRIMS: usize = 40;
const PER_PRIM: usize = 4;
/// Where a particle starts: on a sphere of radius 70, centred 5 up.
const START: f32 = 70.0;
const BASE: f32 = 5.0;
pub const RING_BLUE: &str = "data\\texture\\effect\\ring_blue.tga";
/// The pillars: 6 prims of 4 columns, and the ticks they last at most.
const PILLAR_PRIMS: usize = 6;
const PILLAR_BLUES: [[f32; 3]; 3] = [rgb(100, 100, 255), rgb(70, 70, 255), rgb(30, 30, 255)];
const PILLAR_RED: [f32; 3] = rgb(255, 10, 10);
const PILLAR_TICKS: u32 = 100;

/// The colours of a needle pair, the narrow one and the wide one.
#[derive(Clone, Copy)]
pub struct Look {
    narrow: [f32; 3],
    wide: [f32; 3],
}

const fn rgb(red: u8, green: u8, blue: u8) -> [f32; 3] {
    [red as f32 / 255.0, green as f32 / 255.0, blue as f32 / 255.0]
}

pub const EXCEED: Look = Look { narrow: rgb(0, 250, 250), wide: rgb(250, 0, 0) };
pub const PRESTIGE: Look = Look { narrow: rgb(250, 250, 250), wide: rgb(250, 250, 0) };
pub const BANDING: Look = Look { narrow: rgb(250, 100, 100), wide: rgb(255, 166, 210) };
pub const INSPIRATION: Look = Look { narrow: rgb(200, 200, 255), wide: rgb(5, 5, 255) };

/// One needle: its wait (age), elevation and plane (degrees), position in the game's y-down frame, size, alpha, phase and counter.
#[derive(Clone, Copy, Debug)]
pub struct Needle {
    age: i32,
    elevation: f32,
    plane: f32,
    at: [f32; 3],
    size: f32,
    alpha: f32,
    phase: u32,
    counter: u32,
}

struct Charge {
    /// The effect (882, 908 to 910) and its needles' colours.
    id: u32,
    look: Look,
    unit: Option<u32>,
    start: Instant,
    length: u32,
    ticks: u32,
    needles: Vec<Needle>,
}

/// One of a pillar's columns: its own age, turn and height, and where it stands round the centre.
struct Column {
    age: u32,
    turn: f32,
    height: f32,
    radius: f32,
    offset: [f32; 2],
    blue: [f32; 3],
}

struct Pillars {
    at: Vector,
    start: Instant,
    columns: Vec<Column>,
}

#[derive(Default)]
pub struct Chi {
    charges: Vec<Charge>,
    pillars: Vec<Pillars>,
}

impl Needle {
    /// A needle made with its wait `wait`, elevation and plane.
    pub fn new(wait: u32, elevation: f32, plane: f32) -> Self {
        let (sin_e, cos_e) = elevation.to_radians().sin_cos();
        let (sin_z, cos_z) = plane.to_radians().sin_cos();
        let at = [START * cos_e * cos_z, START * cos_e * sin_z - BASE, START * sin_e];
        Self { age: -(wait as i32), elevation, plane, at, size: 4.0, alpha: 0.0, phase: 0, counter: 0 }
    }

    /// How far it is from the Royal Guard's feet.
    pub fn distance(&self) -> f32 {
        self.at.iter().map(|value| value * value).sum::<f32>().sqrt()
    }

    /// One tick: it fades in, flies straight in, whips round and keeps curving.
    pub fn step(&mut self) {
        self.age += 1;
        if self.age <= 0 {
            return;
        }
        let n = self.age as u32;
        if n < 20 {
            self.alpha = (self.alpha + 5.0).min(255.0);
        }
        if self.phase == 0 && self.distance() < 12.0 {
            self.phase = 1;
            self.counter = 0;
        }
        if self.phase == 1 {
            self.elevation = (self.elevation + 9.0) % 360.0;
            self.counter += 1;
            if self.counter >= 10 {
                self.phase = 2;
            }
            self.alpha = (self.alpha - 2.0).max(0.0);
            self.size *= 0.99;
        }
        if self.phase == 2 {
            self.elevation = (self.elevation - 3.0).rem_euclid(360.0);
            self.plane = (self.plane + 1.0) % 360.0;
            if n > 300 {
                self.alpha = (self.alpha - 1.0).max(0.0);
                self.size *= 0.99;
            }
        }
        let (sin_e, cos_e) = self.elevation.to_radians().sin_cos();
        let (sin_z, cos_z) = self.plane.to_radians().sin_cos();
        self.at[0] -= 0.5 * cos_e * cos_z;
        self.at[1] -= 0.5 * cos_e * sin_z;
        self.at[2] -= 0.5 * sin_e;
    }

    /// Point `angle` on its needle's circle, y down: `size` out in the plane of its motion.
    fn point(&self, angle: f32) -> Vector {
        let (sin, cos) = angle.rem_euclid(360.0).to_radians().sin_cos();
        let (sin_z, cos_z) = self.plane.to_radians().sin_cos();
        [self.size * cos * cos_z + self.at[0], self.size * cos * sin_z + self.at[1], self.size * sin + self.at[2]]
    }

    /// The narrow needle and the wide one round it, as corner angles; the flat widths 0.28 and 1.1 at size 4.
    fn quads(&self, look: &Look) -> [([f32; 4], [f32; 3]); 2] {
        let e = self.elevation;
        [([e - 2.0, e + 2.0, e + 178.0, e + 182.0], look.narrow), ([e - 8.0, e + 8.0, e + 172.0, e + 188.0], look.wide)]
    }
}

/// A pillar column's alpha at its age n: 130 until n = 124, then 15 less a tick.
pub fn pillar_alpha(n: u32) -> f32 {
    (130.0 - 15.0 * n.saturating_sub(124) as f32).max(0.0)
}

/// How far up a column of start height `start` stands at age n: flat to 99, a sine rise over 15 ticks, held, then sinking 5 a tick
/// from n = 125.
pub fn pillar_height(start: f32, n: u32) -> f32 {
    let held = (start - 5.0 * n.saturating_sub(124) as f32).max(0.0);
    match n {
        0..=99 => 0.0,
        100..=115 => held * (6.0 * (n - 100) as f32).to_radians().sin(),
        _ => held,
    }
}

impl Column {
    /// Its age, turn and alpha `ticks` after the pillars were made.
    fn at(&self, ticks: u32) -> (u32, f32, f32) {
        let n = self.age + ticks;
        (n, (self.turn + ticks as f32) % 360.0, pillar_alpha(n))
    }

    /// The square tube round the column's line from its foot `foot` to `up` above it: `angle` turns its corners, `rho` is its
    /// radius and `steps` splits the picture across the four sides (the whole picture on each when None).
    pub fn tube(foot: Vector, up: f32, angle: f32, rho: f32, tint: [f32; 4], steps: Option<f32>) -> Vec<MeshVertex> {
        tube(foot, up, angle, rho, tint, steps)
    }
}

/// The square tube round the column's line from its foot `foot` to `up` above it: `angle` turns its corners, `rho` is its
/// radius and `steps` splits the picture across the four sides (the whole picture on each when None).
pub fn tube(foot: Vector, up: f32, angle: f32, rho: f32, tint: [f32; 4], steps: Option<f32>) -> Vec<MeshVertex> {
    let corner = |s: u32, up: f32| {
        let (sin, cos) = ((90.0 * s as f32 + angle) % 360.0).to_radians().sin_cos();
        [foot[0] + rho * cos, foot[1] + up, foot[2] + rho * sin]
    };
    let mut vertices = Vec::with_capacity(24);
    for s in 1..=4 {
        let (u0, u1) = match steps {
            Some(count) => ((s - 1) as f32 / count, s as f32 / count),
            None => (0.0, 1.0),
        };
        let [a, b, c, d] = [
            MeshVertex { position: corner(s - 1, 0.0), uv: [u0, 1.0], tint },
            MeshVertex { position: corner(s, 0.0), uv: [u1, 1.0], tint },
            MeshVertex { position: corner(s, up), uv: [u1, 0.0], tint },
            MeshVertex { position: corner(s - 1, up), uv: [u0, 0.0], tint },
        ];
        vertices.extend([a, b, c, a, c, d]);
    }
    vertices
}

impl Chi {
    /// Hesperus Lit's and Ride in Lightning's 6 prims of 4 pillars standing at `at`: per prim its blue, per column its age, offset
    /// radius and angle, and radius from `rand(n)`.
    pub fn pillars(&mut self, at: Vector, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        let mut columns = Vec::with_capacity(PILLAR_PRIMS * 4);
        for _ in 0..PILLAR_PRIMS {
            let blue = PILLAR_BLUES[rand(3) as usize];
            for k in 0..4 {
                let age = 50 + rand(50);
                let (distance, angle) = (rand(17) as f32, rand(360) as f32);
                let radius = 0.15 + 0.015 * rand(11) as f32;
                let (sin, cos) = angle.to_radians().sin_cos();
                columns.push(Column { age, turn: 90.0 * k as f32, height: 30.0 * (k + 1) as f32, radius, offset: [distance * cos, distance * sin], blue });
            }
        }
        self.pillars.push(Pillars { at, start: now, columns });
    }

    /// Exceed Break's cast on `unit` for `length` ticks, the last one replaced.
    pub fn charge(&mut self, unit: Option<u32>, length: u32, now: Instant, rand: impl FnMut(u32) -> u32) {
        self.charges.retain(|charge| !(charge.unit == unit && charge.id == 882));
        self.needles(882, EXCEED, unit, length, now, rand);
    }

    /// Prestige's, Banding's or Inspiration's `id` (908 to 910) on `unit` for `length` ticks, beside any others.
    pub fn buff(&mut self, id: u32, unit: Option<u32>, length: u32, now: Instant, rand: impl FnMut(u32) -> u32) {
        let look = match id {
            908 => PRESTIGE,
            909 => BANDING,
            _ => INSPIRATION,
        };
        self.needles(id, look, unit, length, now, rand);
    }

    /// Banding's status going off takes its needles with it.
    pub fn forget_status(&mut self, unit: Option<u32>, status: u16) {
        if status == 405 {
            self.charges.retain(|charge| !(charge.unit == unit && charge.id == 909));
        }
    }

    /// 40 prims of 4 needles, each one's wait, elevation and plane from `rand(n)`.
    fn needles(&mut self, id: u32, look: Look, unit: Option<u32>, length: u32, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        let mut needles = Vec::with_capacity(PRIMS * PER_PRIM);
        for _ in 0..PRIMS * PER_PRIM {
            let wait = rand(101);
            let elevation = rand(360) as f32;
            let plane = rand(360) as f32;
            needles.push(Needle::new(wait, elevation, plane));
        }
        self.charges.push(Charge { id, look, unit, start: now, length, ticks: 0, needles });
    }

    /// Runs the needles up to `now`, a tick at a time, and drops what ended.
    pub fn advance(&mut self, now: Instant) {
        for charge in &mut self.charges {
            while charge.ticks < passes(charge.start, now).min(charge.length) {
                charge.ticks += 1;
                for needle in &mut charge.needles {
                    needle.step();
                }
            }
        }
        self.charges.retain(|charge| passes(charge.start, now) <= charge.length);
        self.pillars.retain(|pillars| passes(pillars.start, now) <= PILLAR_TICKS);
    }

    /// The needles at `now` round their units' feet, added on.
    pub fn meshes(&self, now: Instant, position_of: impl Fn(Option<u32>) -> Option<Vector>) -> Vec<Wall> {
        let mut walls = Vec::new();
        for charge in &self.charges {
            let (Some(feet), true) = (position_of(charge.unit), passes(charge.start, now) > 0) else { continue };
            let mut vertices = Vec::new();
            for needle in charge.needles.iter().filter(|needle| needle.age > 0 && needle.alpha > 0.0) {
                for (angles, rgb) in needle.quads(&charge.look) {
                    let tint = [rgb[0], rgb[1], rgb[2], needle.alpha / 255.0];
                    let corner = |angle: f32, uv: [f32; 2]| {
                        let point = needle.point(angle);
                        MeshVertex { position: [feet[0] + point[0], feet[1] - point[1], feet[2] + point[2]], uv, tint }
                    };
                    let [a, b, c, d] =
                        [corner(angles[0], [0.0, 1.0]), corner(angles[1], [1.0, 1.0]), corner(angles[2], [1.0, 0.0]), corner(angles[3], [0.0, 0.0])];
                    vertices.extend([a, b, c, a, c, d]);
                }
            }
            if !vertices.is_empty() {
                walls.push(Wall { texture: NEEDLE, vertices, additive: true, forced: false });
            }
        }
        // The pillars: a faint red tube blended, then a fatter blue one turned 45° and added on, depth tested.
        for pillars in &self.pillars {
            let ticks = passes(pillars.start, now);
            let (mut core, mut glow) = (Vec::new(), Vec::new());
            for column in &pillars.columns {
                let (n, turn, alpha) = column.at(ticks);
                let up = pillar_height(column.height, n);
                if alpha <= 0.0 || up <= 0.0 {
                    continue;
                }
                let foot = [pillars.at[0] + column.offset[0], pillars.at[1], pillars.at[2] + column.offset[1]];
                let [red, green, blue] = PILLAR_RED;
                core.extend(Column::tube(foot, up, turn, column.radius, [red, green, blue, (alpha / 5.0).floor() / 255.0], None));
                let [red, green, blue] = column.blue;
                glow.extend(Column::tube(foot, up, turn + 45.0, 1.5 * column.radius, [red, green, blue, alpha / 255.0], Some(21.0)));
            }
            walls.push(Wall { texture: RING_BLUE, vertices: core, additive: false, forced: false });
            walls.push(Wall { texture: RING_BLUE, vertices: glow, additive: true, forced: false });
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
        (got - want).abs() < 0.01
    }

    #[test]
    fn exceed_break_converges_160_needles_on_the_royal_guard() {
        let start = Instant::now();
        let mut chi = Chi::default();
        let mut draws = 0;
        chi.charge(Some(1), 500, start, |_| {
            draws += 1;
            0
        });
        // 40 prims of 4, three draws each; the waits are R(101).
        assert_eq!(draws, 480);
        assert_eq!(chi.charges[0].needles.len(), 160);
        // A needle at e = 0, z = 0 with no wait: alpha 5 a tick to 95 at n = 19, held at n = 20.
        let mut needle = Needle::new(0, 0.0, 0.0);
        assert!(near(needle.distance(), (70.0_f32 * 70.0 + 25.0).sqrt()));
        let mut alphas = Vec::new();
        for _ in 0..20 {
            needle.step();
            alphas.push(needle.alpha);
        }
        assert_eq!((alphas[0], alphas[18], alphas[19]), (5.0, 95.0, 95.0));
        // No particle before its wait: a wait of 3 does nothing for 3 ticks.
        let mut waiting = Needle::new(3, 0.0, 0.0);
        for _ in 0..3 {
            waiting.step();
        }
        assert_eq!((waiting.alpha, waiting.at[0]), (0.0, 70.0));
        // It flies 0.5 a tick to within 12, then turns 9° for 10 ticks (alpha down to 75, size × 0.99 each), then −3° and +1°.
        let mut flight = Needle::new(0, 0.0, 0.0);
        let mut ticks = 0;
        while flight.phase == 0 {
            flight.step();
            ticks += 1;
        }
        assert!((112..=122).contains(&ticks), "{ticks}");
        for _ in 0..9 {
            flight.step();
        }
        assert_eq!(flight.phase, 2);
        assert!(near(flight.alpha, 75.0) && near(flight.size, 4.0 * 0.99_f32.powi(10)));
        let (e, z) = (flight.elevation, flight.plane);
        flight.step();
        assert!(near(flight.elevation, e - 3.0) && near(flight.plane, z + 1.0));
        // At n = 300 the alpha is 75 and at 301 it is 74.
        while flight.age < 300 {
            flight.step();
        }
        assert_eq!(flight.alpha, 75.0);
        flight.step();
        assert_eq!(flight.alpha, 74.0);
        // The needle's widths at size 4: 0.28 narrow, 1.1 wide; the colours #00FAFA and #FA0000.
        let narrow = (4.0_f32 * 4.0_f32.to_radians().sin()).abs();
        let wide = (4.0_f32 * 16.0_f32.to_radians().sin()).abs();
        assert!(near(narrow, 0.28) && (wide - 1.1).abs() < 0.01);
        let quads = Needle::new(0, 0.0, 0.0).quads(&EXCEED);
        assert_eq!((quads[0].1, quads[1].1), ([0.0, 250.0 / 255.0, 250.0 / 255.0], [250.0 / 255.0, 0.0, 0.0]));
        // Drawn while alpha > 0, follows the caster, ends with the cast.
        for tick in 0..30 {
            chi.advance(start + TICK * tick);
        }
        let walls = chi.meshes(start + TICK * 29, |_| Some([0.0; 3]));
        assert!(walls.len() == 1 && walls[0].additive && walls[0].texture == NEEDLE && !walls[0].vertices.is_empty());
        assert!(chi.meshes(start, |_| None).is_empty());
        chi.advance(start + TICK * 510);
        assert!(chi.charges.is_empty());
    }

    #[test]
    fn hesperus_lit_shoots_24_thin_blue_pillars() {
        let start = Instant::now();
        let at = [10.0, 1.0, 20.0];
        let mut chi = Chi::default();
        let mut draws = 0;
        chi.pillars(at, start, |_| {
            draws += 1;
            0
        });
        // 6 prims of 4: one draw for the colour and four a column, 17 a prim.
        assert_eq!(draws, 6 * 17);
        let columns = &chi.pillars[0].columns;
        assert_eq!(columns.len(), 24);
        // Every draw 0: age 50, heights 30 to 120, turns 0 to 270, no offset, radius 0.15, #6464FF.
        assert!(columns.iter().all(|column| column.age == 50 && column.offset == [0.0, 0.0] && near(column.radius, 0.15) && column.blue == PILLAR_BLUES[0]));
        assert_eq!([0, 1, 2, 3].map(|k| (columns[k].height, columns[k].turn)), [(30.0, 0.0), (60.0, 90.0), (90.0, 180.0), (120.0, 270.0)]);
        // The height at n = 51, 100, 101, 115, 124 for a column of 30, and the sinking from 125.
        let heights = [51, 100, 101, 115, 124, 125, 129].map(|n| pillar_height(30.0, n));
        assert_eq!(heights[..2], [0.0, 0.0]);
        assert!(near(heights[2], 30.0 * 6.0_f32.to_radians().sin()) && near(heights[2], 3.136));
        assert_eq!((heights[3], heights[4], heights[5], heights[6]), (30.0, 30.0, 25.0, 5.0));
        assert_eq!([124, 125, 132, 133].map(pillar_alpha), [130.0, 115.0, 10.0, 0.0]);
        // The turn grows 1 a tick; nothing is drawn while the column lies flat (ticks to n = 100).
        assert_eq!(columns[0].at(1), (51, 1.0, 130.0));
        assert!(chi.meshes(start + TICK * 49, |_| None).is_empty());
        // At the first tick with n = 101 (k = 51): the height 3.14 for the 30-tall column, tubes of 4 quads, two passes.
        let walls = chi.meshes(start + TICK * 50, |_| None);
        assert_eq!(walls.len(), 2);
        assert!(!walls[0].additive && walls[1].additive && walls.iter().all(|wall| wall.texture == RING_BLUE));
        // Four sides of two triangles each for each of the 24 columns (all but the 30-tall ones are higher than 0 as well).
        assert_eq!(walls[0].vertices.len(), 24 * 24);
        // Pass A is #FF0A0A with alpha ⌊130/5⌋ = 26; pass B the prim's blue with alpha 130.
        assert_eq!(walls[0].vertices[0].tint, [1.0, 10.0 / 255.0, 10.0 / 255.0, 26.0 / 255.0]);
        assert_eq!(walls[1].vertices[0].tint, [100.0 / 255.0, 100.0 / 255.0, 1.0, 130.0 / 255.0]);
        // The foot stands at the place and the top p up; pass B's corners are 45° on from pass A's and 1.5 times as wide.
        let first = &walls[0].vertices;
        assert!(near(first[0].position[1], at[1]) && near(first[2].position[1], at[1] + 3.136));
        let angle = |vertex: &MeshVertex| (vertex.position[2] - at[2]).atan2(vertex.position[0] - at[0]).to_degrees().rem_euclid(360.0);
        // k = 51: turn 51.
        assert!(near(angle(&first[0]), 51.0) && near(angle(&walls[1].vertices[0]), 96.0));
        assert!(near((walls[1].vertices[0].position[0] - at[0]).hypot(walls[1].vertices[0].position[2] - at[2]), 0.225));
        // Pass B's picture runs across the sides in 21 steps; pass A's shows the whole picture on each.
        assert_eq!([walls[1].vertices[0].uv, walls[1].vertices[1].uv, walls[1].vertices[2].uv], [[0.0, 1.0], [1.0 / 21.0, 1.0], [1.0 / 21.0, 0.0]]);
        assert_eq!([first[0].uv, first[1].uv, first[2].uv, first[5].uv], [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]]);
        // Gone once faded: at n = 134 for the age-50 columns (k = 84), and dropped after 100 ticks.
        assert!(chi.meshes(start + TICK * 84, |_| None).iter().all(|wall| wall.vertices.is_empty()));
        chi.advance(start + TICK * 101);
        assert!(chi.pillars.is_empty());
    }

    #[test]
    fn prestige_banding_and_inspiration_converge_160_needles_in_their_colours() {
        let start = Instant::now();
        let mut chi = Chi::default();
        for id in [908, 909, 910] {
            chi.buff(id, Some(1), 500, start, |_| 0);
        }
        // Beside each other and Exceed Break's cast, 160 each, 500 ticks long.
        chi.charge(Some(1), 300, start, |_| 0);
        assert_eq!(chi.charges.len(), 4);
        assert!(chi.charges.iter().all(|charge| charge.needles.len() == 160));
        assert!(chi.charges[..3].iter().all(|charge| charge.length == 500));
        let colours = |index: usize| {
            let charge = &chi.charges[index];
            let quads = Needle::new(0, 0.0, 0.0).quads(&charge.look);
            [quads[0].1.map(|channel| (channel * 255.0).round() as u8), quads[1].1.map(|channel| (channel * 255.0).round() as u8)]
        };
        assert_eq!(colours(0), [[250, 250, 250], [250, 250, 0]]);
        assert_eq!(colours(1), [[250, 100, 100], [255, 166, 210]]);
        assert_eq!(colours(2), [[200, 200, 255], [5, 5, 255]]);
        assert_eq!(colours(3), [[0, 250, 250], [250, 0, 0]]);
        // A second cast replaces only Exceed Break's; Banding's status going off takes only Banding's, only for its unit.
        chi.charge(Some(1), 300, start, |_| 0);
        assert_eq!(chi.charges.len(), 4);
        chi.forget_status(Some(2), 405);
        chi.forget_status(Some(1), 406);
        assert_eq!(chi.charges.len(), 4);
        chi.forget_status(Some(1), 405);
        assert!(chi.charges.iter().all(|charge| charge.id != 909) && chi.charges.len() == 3);
    }
}
