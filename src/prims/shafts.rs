//! Renovatio's four slanting green light shafts (719), a 16 ms tick at a time.
//!
//! A shaft's own age a counts from 1 at its first drawn tick; p = a − 1.

use std::time::Instant;

use super::passes;
use super::walls::Wall;
use crate::world::{MeshVertex, Vector};

pub const CLOUD: &str = super::sniper::CLOUD;
const TINT: [f32; 3] = [230.0 / 255.0, 1.0, 230.0 / 255.0];
const SHAFTS: usize = 4;
/// The top ring's place against the bottom's: 70 to −x and −z, 300 up.
const TOP: Vector = [-70.0, 300.0, -70.0];
/// The first three are drawn for p = 0 to 208, the fourth (born at age 180) for 0 to 28.
const LONG: u32 = 209;
const SHORT: u32 = 29;

struct Renovatio {
    at: Vector,
    start: Instant,
}

/// Dimension Door's cell: the same four shafts, blue, that never end while the unit stands.
struct Door {
    id: u32,
    at: Vector,
    start: Instant,
}

#[derive(Default)]
pub struct Shafts {
    list: Vec<Renovatio>,
    doors: Vec<Door>,
}

const DOOR_TINT: [f32; 3] = [80.0 / 255.0, 80.0 / 255.0, 1.0];

/// Dimension Door's shaft k at its age a: 30 + a to 70 for the first three, 30 for the fourth; none ever ends.
pub fn door_alpha(k: usize, a: u32) -> f32 {
    if k == 3 { 30.0 } else { (30 + a).min(70) as f32 }
}

/// Dimension Door's radius of shaft k at its age a (the fourth starts at age 180).
pub fn door_radius(k: usize, a: u32) -> f32 {
    let breath = |age: u32| (((age % 720) / 2) as f32).to_radians().sin();
    match k {
        0 => 1.0,
        2 => 2.0,
        1 => 1.5 + 0.5 * breath(a),
        _ => 1.0 + 0.5 * breath(a + 180),
    }
}

/// Shaft k's alpha at p, or none once it is not drawn: the first three open by 1 a tick from 30 to 70, hold from age 40 to 140 and
/// close by 1 a tick; the fourth falls from 29.
pub fn alpha(k: usize, p: u32) -> Option<f32> {
    let a = p + 1;
    if k == 3 {
        return (p < SHORT).then(|| (SHORT - p) as f32);
    }
    if p >= LONG {
        return None;
    }
    Some(match a {
        1..=40 => 30.0 + a as f32,
        41..=140 => 70.0,
        _ => 70.0 - (a - 140) as f32,
    })
}

/// Shaft k's radius at p.
pub fn radius(k: usize, p: u32) -> f32 {
    match k {
        0 => 1.0,
        2 => 2.0,
        1 => 1.5 + 0.5 * (p.div_ceil(2) as f32).to_radians().sin(),
        _ => 1.0 + 0.5 * (((p + 181) / 2) as f32).to_radians().sin(),
    }
}

impl Shafts {
    /// Dimension Door's four shafts over the cell `at` of the skill unit `id`.
    pub fn keep(&mut self, id: u32, at: Vector, now: Instant) {
        self.doors.push(Door { id, at, start: now });
    }

    pub fn forget(&mut self, id: u32) {
        self.doors.retain(|door| door.id != id);
    }

    /// Four shafts over the target's feet `at`; they stay there.
    pub fn play(&mut self, at: Vector, now: Instant) {
        self.list.push(Renovatio { at, start: now });
    }

    pub fn advance(&mut self, now: Instant) {
        self.list.retain(|renovatio| passes(renovatio.start, now) <= LONG + 1);
    }

    /// One shaft: a closed pentagonal tube, five quads from the ground ring to the top one.
    fn tube(at: Vector, k: usize, r: f32, alpha: f32, rgb: [f32; 3]) -> Vec<MeshVertex> {
        let tint = [rgb[0], rgb[1], rgb[2], alpha / 255.0];
        let ring = |j: usize, up: Vector| {
            let (sin, cos) = ((72 * j) as f32 + 25.0 * k as f32).to_radians().sin_cos();
            [at[0] + up[0] + r * cos, at[1] + up[1], at[2] + up[2] + r * sin]
        };
        let mut vertices = Vec::new();
        for j in 1..=5 {
            let [a, b, c, d] = [
                MeshVertex { position: ring(j - 1, [0.0; 3]), uv: [0.0, 1.0], tint },
                MeshVertex { position: ring(j, [0.0; 3]), uv: [1.0, 1.0], tint },
                MeshVertex { position: ring(j, TOP), uv: [1.0, 0.0], tint },
                MeshVertex { position: ring(j - 1, TOP), uv: [0.0, 0.0], tint },
            ];
            vertices.extend([a, b, c, a, c, d]);
        }
        vertices
    }

    /// The shafts at `now`, added on.
    pub fn meshes(&self, now: Instant) -> Vec<Wall> {
        let mut walls = Vec::new();
        for renovatio in &self.list {
            let Some(p) = passes(renovatio.start, now).checked_sub(1) else { continue };
            for k in 0..SHAFTS {
                if let Some(alpha) = alpha(k, p) {
                    walls.push(Wall { texture: CLOUD, vertices: Self::tube(renovatio.at, k, radius(k, p), alpha, TINT), additive: true, forced: false });
                }
            }
        }
        for door in &self.doors {
            let a = passes(door.start, now);
            if a == 0 {
                continue;
            }
            for k in 0..SHAFTS {
                walls.push(Wall {
                    texture: CLOUD,
                    vertices: Self::tube(door.at, k, door_radius(k, a), door_alpha(k, a), DOOR_TINT),
                    additive: true,
                    forced: false,
                });
            }
        }
        walls
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prims::TICK;

    #[test]
    fn renovatio_slants_four_shafts_onto_the_target() {
        let start = Instant::now();
        let mut shafts = Shafts::default();
        shafts.play([10.0, 1.0, 10.0], start);
        // Four elements at tick 0, five quads each, added on, never following anyone.
        let walls = shafts.meshes(start);
        assert_eq!(walls.len(), 4);
        assert!(walls.iter().all(|wall| wall.additive && wall.texture == CLOUD && wall.vertices.len() == 30));
        // k = 0: alpha at p = 0, 39, 139, 140, 208, 209: 31, 70, 70, 69, 1, hidden.
        assert_eq!([0, 39, 139, 140, 208, 209].map(|p| alpha(0, p)), [Some(31.0), Some(70.0), Some(70.0), Some(69.0), Some(1.0), None]);
        // k = 3 is drawn for p = 0 to 28 only: 29 at the first, 1 at the last.
        assert_eq!([0, 28, 29].map(|p| alpha(3, p)), [Some(29.0), Some(1.0), None]);
        // Radius: 1.0, 1.5 → 2.0, 2.0 and about 1.5 for the fourth.
        assert_eq!([radius(0, 50), radius(2, 50)], [1.0, 2.0]);
        assert!((radius(1, 0) - 1.5).abs() < 1e-4 && (radius(1, 179) - 2.0).abs() < 1e-4);
        assert!((radius(3, 0) - 1.5).abs() < 0.01);
        // The ring: the top is 70 to −x and −z and 300 up; the corners go 72° apart from 25k.
        let first = &walls[0].vertices;
        assert_eq!(first[1].position[1], 1.0);
        assert_eq!(first[2].position[1], 301.0);
        assert!((first[2].position[0] - (first[1].position[0] - 70.0)).abs() < 1e-3 && (first[2].position[2] - (first[1].position[2] - 70.0)).abs() < 1e-3);
        assert_eq!([first[0].uv, first[1].uv, first[2].uv, first[5].uv], [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]]);
        // The fourth is gone at p = 29, the others at 209.
        assert_eq!(shafts.meshes(start + TICK * 29).len(), 3);
        assert_eq!(shafts.meshes(start + TICK * 208).len(), 3);
        assert!(shafts.meshes(start + TICK * 209).is_empty());
        shafts.advance(start + TICK * 400);
        assert!(shafts.list.is_empty());
    }

    #[test]
    fn dimension_door_keeps_four_blue_shafts() {
        let start = Instant::now();
        let mut shafts = Shafts::default();
        shafts.keep(5, [10.0, 1.0, 10.0], start);
        // k = 0 … 2: 31 at a = 1, 70 at 40, 41 and 100; k = 3: 30 for good.
        assert_eq!([1, 40, 41, 100].map(|a| door_alpha(0, a)), [31.0, 70.0, 70.0, 70.0]);
        assert_eq!([1, 500].map(|a| door_alpha(3, a)), [30.0, 30.0]);
        // Radius: k = 1 1.5 at a = 1 and 2.0 at 180; k = 3 1.5 after its first tick (a = 181).
        assert!((door_radius(1, 1) - 1.5).abs() < 1e-4 && (door_radius(1, 180) - 2.0).abs() < 1e-4 && (door_radius(3, 1) - 1.5).abs() < 1e-4);
        assert_eq!([door_radius(0, 7), door_radius(2, 7)], [1.0, 2.0]);
        // #5050FF, the top ring 70 to −x and −z and 300 up; none ends while the unit stands, and it goes with it.
        let walls = shafts.meshes(start);
        assert_eq!(walls.len(), 4);
        assert_eq!(walls[0].vertices[0].tint[..3], [80.0 / 255.0, 80.0 / 255.0, 1.0]);
        assert_eq!(walls[0].vertices[2].position[1], 301.0);
        assert_eq!(shafts.meshes(start + TICK * 30_000).len(), 4);
        shafts.forget(5);
        assert!(shafts.meshes(start).is_empty());
    }
}
