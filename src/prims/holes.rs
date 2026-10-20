//! Manhole's and Dimension Door's ground (822, 825), a 16 ms tick at a time: a blue swirl square turning on the cell and, every 100
//! ticks, three breathing flat ring bands growing out from its centre.
//!
//! A shape's own age n counts from 1 at its first tick; e is the ticks since the effect started.

use std::time::Instant;

use super::sage::{Corner, ring_point, ring_quads};
use super::walls::{RING_WHITE, Wall};
use super::{TICK, passes};
use crate::world::{MeshVertex, Vector};

pub const TWIRL: &str = "data\\texture\\effect\\twirl.bmp";
pub const SPHERE: &str = "data\\texture\\effect\\구체15.bmp";
const RING_TINT: [f32; 3] = [100.0 / 255.0, 100.0 / 255.0, 1.0];
/// The swirl lies 2 over the ground; a hole lives 29 999 ticks.
const RISE: f32 = 2.0;
const LIFE: u32 = 29_999;
/// A ring triple is seen for prim ages 0 to 135.
const RING_AGES: u32 = 136;
const LEANS: [f32; 3] = [2.0, 3.0, 4.0];
const TURNS: [f32; 3] = [0.0, 25.0, 50.0];

/// A cell of Manhole's or Dimension Door's skill unit.
struct Hole {
    id: u32,
    door: bool,
    at: Vector,
    start: Instant,
    /// The swirl's start spin, and how many ticks have started a ring triple's check.
    spin: f32,
    ticks: u32,
}

/// A ring triple made at `start`.
struct Triple {
    start: Instant,
    at: Vector,
}

#[derive(Default)]
pub struct Holes {
    holes: Vec<Hole>,
    triples: Vec<Triple>,
}

/// One ring band at a prim age: whether it still runs, its own age, radius, alpha (whole) and profile height.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Band {
    pub active: bool,
    pub age: i32,
    pub radius: f32,
    pub alpha: i32,
    pub height: f32,
}

/// The swirl's half-diagonal at spin `spin`: 9.6 to 12.0, a 120-tick breath.
pub fn swirl_half(spin: f32) -> f32 {
    (spin.to_radians().sin() + 1.0) * 1.2 + 9.6
}

/// The three bands after the prim's tick `a` (a + 1 ticks run): each starts at its own age (0, −10, −20), grows 0.5 a tick, fades
/// from radius 7, and starts again until band 0 has run 120 ticks.
pub fn bands(a: u32) -> [Band; 3] {
    let mut bands = [0, -10, -20].map(|age| Band { active: true, age, radius: 0.0, alpha: 0, height: 0.0 });
    let mut counter = 0;
    for _ in 0..=a {
        for (k, band) in bands.iter_mut().enumerate() {
            if !band.active {
                continue;
            }
            band.age += 1;
            if k == 0 {
                counter += 1;
            }
            if band.age <= 0 {
                continue;
            }
            band.radius += 0.5;
            if band.radius > 7.0 {
                band.alpha -= 5;
                if band.alpha < 0 {
                    band.alpha = 0;
                    if counter > 120 {
                        band.active = false;
                        continue;
                    }
                    band.age = 0;
                    band.radius = 0.0;
                }
            }
            if band.age < 10 {
                band.alpha = (band.alpha + 4).min(40);
            }
            band.height = 6.001 * if band.age <= 10 { (9.0 * band.age as f32).to_radians().sin() } else { 1.0 };
        }
    }
    bands
}

impl Holes {
    /// A hole's cell at `at`, standing as skill unit `id` (Dimension Door's if `door`); the swirl's start spin from `rand(n)`.
    pub fn keep(&mut self, id: u32, door: bool, at: Vector, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        let spin = rand(360) as f32;
        let _ = rand(360);
        self.holes.push(Hole { id, door, at, start: now, spin, ticks: 0 });
    }

    pub fn forget(&mut self, id: u32) {
        self.holes.retain(|hole| hole.id != id);
    }

    /// Starts a ring triple on every 100th tick of each hole and drops what ended.
    pub fn advance(&mut self, now: Instant) {
        for hole in &mut self.holes {
            while hole.ticks < passes(hole.start, now) {
                if hole.ticks % 100 == 0 {
                    self.triples.push(Triple { start: hole.start + TICK * hole.ticks, at: hole.at });
                }
                hole.ticks += 1;
            }
        }
        self.holes.retain(|hole| passes(hole.start, now) <= LIFE);
        self.triples.retain(|triple| passes(triple.start, now) <= RING_AGES);
    }

    /// The swirls and ring bands at `now`, added on.
    pub fn meshes(&self, now: Instant) -> Vec<Wall> {
        let mut walls = Vec::new();
        for hole in &self.holes {
            let n = passes(hole.start, now);
            if n == 0 {
                continue;
            }
            let spin = (hole.spin + 3.0 * n as f32) % 360.0;
            let turn = (6 * n % 360) as f32;
            let half = swirl_half(spin);
            let tint = [1.0, 1.0, 1.0, 200.0 / 255.0];
            let corner = |j: u32, uv: [f32; 2]| {
                let (sin, cos) = (turn + 90.0 * j as f32).to_radians().sin_cos();
                MeshVertex { position: [hole.at[0] + half * cos, hole.at[1] + RISE, hole.at[2] + half * sin], uv, tint }
            };
            let [a, b, c, d] = [corner(0, [0.0, 1.0]), corner(1, [1.0, 1.0]), corner(2, [1.0, 0.0]), corner(3, [0.0, 0.0])];
            walls.push(Wall { texture: if hole.door { SPHERE } else { TWIRL }, vertices: vec![a, b, c, a, c, d], additive: true, forced: false });
        }
        for triple in &self.triples {
            let Some(a) = passes(triple.start, now).checked_sub(1).filter(|&a| a < RING_AGES) else { continue };
            let base = [triple.at[0], triple.at[1] + RISE, triple.at[2]];
            let mut vertices = Vec::new();
            for (k, band) in bands(a).iter().enumerate() {
                if !band.active || band.age <= 0 || band.radius <= 0.0 {
                    continue;
                }
                let points: Vec<_> = (0..=20).map(|s| ring_point(base, TURNS[k] + 18.0 * s as f32, band.radius, band.height, LEANS[k])).collect();
                let tint = [RING_TINT[0], RING_TINT[1], RING_TINT[2], band.alpha as f32 / 255.0];
                vertices.extend(corners_to_vertices(ring_quads(&points, 1.0), tint));
            }
            if !vertices.is_empty() {
                walls.push(Wall { texture: RING_WHITE, vertices, additive: true, forced: false });
            }
        }
        walls
    }
}

/// A ring's corners as vertices tinted `tint` (its own opacity taken as given).
fn corners_to_vertices(corners: Vec<Corner>, tint: [f32; 4]) -> Vec<MeshVertex> {
    corners.into_iter().map(|(position, uv, _)| MeshVertex { position, uv, tint }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(got: f32, want: f32) -> bool {
        (got - want).abs() < 0.01
    }

    #[test]
    fn the_ring_triple_breathes_out_and_ends_in_136_ticks() {
        // Band 0 at n = 1, 9, 10, 14, 15, 21, 22: r 0.5, 4.5, 5.0, 7.0, 7.5, 10.5, 0; alpha 4, 36, 36, 36, 31, 1, 4; p 0.94 … 0.
        let first = |n: u32| bands(n - 1)[0];
        assert_eq!([1, 9, 10, 14, 15, 21, 22].map(|n| first(n).radius), [0.5, 4.5, 5.0, 7.0, 7.5, 10.5, 0.0]);
        assert_eq!([1, 9, 10, 14, 15, 21, 22].map(|n| first(n).alpha), [4, 36, 36, 36, 31, 1, 4]);
        assert!(
            near(first(1).height, 0.94) && near(first(9).height, 5.93) && near(first(10).height, 6.0) && near(first(21).height, 6.0) && first(22).height == 0.0
        );
        // The second cycle: alpha at n = 9, 14, 15, 22, 23 (of its own age): 40, 40, 35, 0, 4.
        let second = |n: u32| bands(22 + n - 1)[0];
        assert_eq!([9, 14, 15, 22, 23].map(|n| second(n).alpha), [40, 40, 35, 0, 4]);
        // Bands 1 and 2 show nothing before prim ages 10 and 20; band 0 ends at prim age 136, band 1 at 123, band 2 at 133.
        assert!(bands(9)[1].age <= 0 && bands(10)[1].age == 1 && bands(19)[2].age <= 0 && bands(20)[2].age == 1);
        assert!(bands(135)[0].active && !bands(136)[0].active);
        assert!(bands(122)[1].active && !bands(123)[1].active);
        assert!(bands(132)[2].active && !bands(133)[2].active);
    }

    #[test]
    fn a_hole_turns_a_breathing_swirl_and_throws_ring_triples_every_100_ticks() {
        let start = Instant::now();
        let mut holes = Holes::default();
        holes.keep(4, false, [10.0, 1.0, 10.0], start, |_| 0);
        holes.keep(5, true, [20.0, 1.0, 20.0], start, |_| 0);
        // The swirl: 9.6 … 12.0; one ring prim at e = 0, 100, 200 and none between.
        assert!(near(swirl_half(90.0), 12.0) && near(swirl_half(270.0), 9.6));
        for tick in 0..=205 {
            holes.advance(start + TICK * tick);
            let triples = holes.triples.len();
            assert!(triples <= 4 * 2, "tick {tick}");
        }
        assert_eq!(holes.triples.iter().filter(|triple| triple.at[0] == 10.0).count(), 2);
        // The swirls: twirl for Manhole, sphere for the door; white at alpha 200, added on, 2 up; turned 6° a tick, a turn in 60.
        let walls = holes.meshes(start + TICK * 3);
        assert_eq!(walls[0].texture, TWIRL);
        assert_eq!(walls[1].texture, SPHERE);
        assert!(walls[..2].iter().all(|wall| wall.additive && wall.vertices.len() == 6 && wall.vertices[0].tint == [1.0, 1.0, 1.0, 200.0 / 255.0]));
        assert!(near(walls[0].vertices[0].position[1], 3.0));
        let turned = |tick: u32| {
            let wall = &holes.meshes(start + TICK * tick)[0];
            let vertex = &wall.vertices[0];
            (vertex.position[2] - 10.0).atan2(vertex.position[0] - 10.0).to_degrees().rem_euclid(360.0)
        };
        assert!(near(turned(0), 6.0) && near((turned(1) - turned(0)).rem_euclid(360.0), 6.0) && near(turned(59), 0.0));
        // They go with the unit, and after 29 999 ticks.
        holes.forget(4);
        assert_eq!(holes.holes.len(), 1);
        holes.advance(start + TICK * 30_005);
        assert!(holes.holes.is_empty());
    }
}
