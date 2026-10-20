//! Exceed Break's cast (882): 160 needles converge on the Royal Guard, a 16 ms tick at a time.
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
    unit: Option<u32>,
    start: Instant,
    length: u32,
    ticks: u32,
    needles: Vec<Needle>,
}

#[derive(Default)]
pub struct Chi {
    charges: Vec<Charge>,
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

    /// The narrow cyan needle and the wide red one round it, as corner angles; the flat widths 0.28 and 1.1 at size 4.
    fn quads(&self) -> [([f32; 4], [f32; 3]); 2] {
        let e = self.elevation;
        [([e - 2.0, e + 2.0, e + 178.0, e + 182.0], [0.0, 250.0 / 255.0, 250.0 / 255.0]), ([e - 8.0, e + 8.0, e + 172.0, e + 188.0], [250.0 / 255.0, 0.0, 0.0])]
    }
}

impl Chi {
    /// Exceed Break's cast on `unit` for `length` ticks: 40 prims of 4 needles, each one's wait, elevation and plane from `rand(n)`.
    pub fn charge(&mut self, unit: Option<u32>, length: u32, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        let mut needles = Vec::with_capacity(PRIMS * PER_PRIM);
        for _ in 0..PRIMS * PER_PRIM {
            let wait = rand(101);
            let elevation = rand(360) as f32;
            let plane = rand(360) as f32;
            needles.push(Needle::new(wait, elevation, plane));
        }
        self.charges.retain(|charge| charge.unit != unit);
        self.charges.push(Charge { unit, start: now, length, ticks: 0, needles });
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
    }

    /// The needles at `now` round their units' feet, added on.
    pub fn meshes(&self, now: Instant, position_of: impl Fn(Option<u32>) -> Option<Vector>) -> Vec<Wall> {
        let mut walls = Vec::new();
        for charge in &self.charges {
            let (Some(feet), true) = (position_of(charge.unit), passes(charge.start, now) > 0) else { continue };
            let mut vertices = Vec::new();
            for needle in charge.needles.iter().filter(|needle| needle.age > 0 && needle.alpha > 0.0) {
                for (angles, rgb) in needle.quads() {
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
        let quads = Needle::new(0, 0.0, 0.0).quads();
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
}
