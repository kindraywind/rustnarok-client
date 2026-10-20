//! The Mechanic's code-made shapes, a 16 ms tick at a time: Magnetic Field's 32 jittering lightning arcs (781) and the breathing
//! ground bands of Neutral Barrier's and Stealth Field's cells (783, 784).
//!
//! An element's own age n counts from 1 at its first tick; t is the ticks since the effect started.

use std::time::Instant;

use super::passes;
use super::tanji::bearing;
use super::walls::Wall;
use crate::world::{MeshVertex, Vector};

pub const ELEC: &str = "data\\texture\\effect\\elec1.tga";
pub const BARRIER_BAND: &str = "data\\texture\\effect\\slash01.tga";
pub const STEALTH_BAND: &str = "data\\texture\\effect\\썬더스톰파티클.tga";
/// Magnetic Field: two prims on each of ticks 0 to 3, four arcs each; the effect lasts 300 ticks.
const ARC_TICKS: u32 = 4;
const ARC_LIFE: u32 = 300;
const POINTS: usize = 21;
/// A band lives 29 999 ticks even if its unit stands longer; #FFFFAA.
const BAND_LIFE: u32 = 29_999;
const BAND_TINT: [f32; 3] = [1.0, 1.0, 170.0 / 255.0];

/// One lightning arc: when its prim was made, the way it points, its height factor, where it stands and its alpha after each tick.
pub struct Arc {
    made: u32,
    pub dir: f32,
    pub h: f32,
    origin: Vector,
    alpha: Vec<f32>,
}

struct Field {
    start: Instant,
    caster: Option<u32>,
    /// The prims made so far, and where the caster last stood.
    prims: u32,
    last: Option<Vector>,
    arcs: Vec<Arc>,
}

/// A breathing band on a ground unit's cell.
struct Band {
    id: u32,
    texture: &'static str,
    at: Vector,
    start: Instant,
    h: f32,
    turn: f32,
    alpha: f32,
    phase: f32,
}

#[derive(Default)]
pub struct Mechanic {
    fields: Vec<Field>,
    bands: Vec<Band>,
}

/// The angle of profile point s: 180 at both the first two, then 9 less each.
pub fn profile(s: usize) -> f32 {
    if s == 0 { 180.0 } else { 9.0 * (21 - s) as f32 }
}

/// How far an arc reaches at its age n.
pub fn reach(n: u32) -> f32 {
    0.4 * n as f32
}

impl Arc {
    /// Its alpha at age n (0 before it and once it has gone).
    pub fn alpha_at(&self, n: u32) -> f32 {
        n.checked_sub(1).and_then(|index| self.alpha.get(index as usize)).copied().unwrap_or(0.0)
    }

    /// The age it is drawn at after effect tick `t`, if it is.
    fn age(&self, t: u32) -> Option<u32> {
        let n = (t + 1).checked_sub(self.made)?;
        (n >= 1 && self.alpha_at(n) > 0.0).then_some(n)
    }

    /// Its points at age n with the frame's jitter: the base shape plus (A, B, C) from `jitter()` for every point to the 19th.
    pub fn points(&self, n: u32, mut jitter: impl FnMut() -> [f32; 3]) -> Vec<Vector> {
        let r = reach(n);
        let (sin, cos) = self.dir.to_radians().sin_cos();
        (0..POINTS)
            .map(|s| {
                let rho = profile(s) / 90.0 * r;
                let [a, b, c] = if s < POINTS - 1 { jitter() } else { [0.0; 3] };
                [self.origin[0] + cos * rho + b, self.origin[1] + 2.0 + r * self.h * profile(s).to_radians().sin() - a, self.origin[2] + sin * rho + c]
            })
            .collect()
    }
}

/// A 3-sided tube of radius 0.1 round the segment from `a` to `b`, three quads, in `rgb` at `alpha`.
fn tube(a: Vector, b: Vector, rgb: [f32; 3], alpha: f32) -> Vec<MeshVertex> {
    let tint = [rgb[0], rgb[1], rgb[2], alpha / 255.0];
    let (sin, cos) = ((bearing(a[0] - b[0], a[2] - b[2]) as f32 + 90.0) % 360.0).to_radians().sin_cos();
    let offset = |theta: f32| {
        let (s, c) = theta.to_radians().sin_cos();
        [0.1 * c * cos, 0.1 * s, 0.1 * c * sin]
    };
    let at = |end: Vector, off: [f32; 3]| [end[0] + off[0], end[1] + off[1], end[2] + off[2]];
    let mut vertices = Vec::new();
    for j in 1..=3 {
        let (near, far) = (offset(120.0 * j as f32), offset(120.0 * (j - 1) as f32));
        let [p, q, r, s] = [
            MeshVertex { position: at(a, near), uv: [0.0, 1.0], tint },
            MeshVertex { position: at(b, near), uv: [1.0, 1.0], tint },
            MeshVertex { position: at(b, far), uv: [1.0, 0.0], tint },
            MeshVertex { position: at(a, far), uv: [0.0, 0.0], tint },
        ];
        vertices.extend([p, q, r, p, r, s]);
    }
    vertices
}

impl Mechanic {
    /// Magnetic Field from the caster `caster` (None = the player).
    pub fn magnetic_field(&mut self, caster: Option<u32>, now: Instant) {
        self.fields.push(Field { start: now, caster, prims: 0, last: None, arcs: Vec::new() });
    }

    /// A band on the cell `at` of the ground unit `id`: its choices from `rand(n)` in the order the game draws them.
    pub fn keep_band(&mut self, id: u32, stealth: bool, at: Vector, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        let h = (25 + rand(10)) as f32;
        let turn = rand(360) as f32;
        let alpha = (50 - rand(10) as i32) as f32;
        let phase = rand(360) as f32;
        let texture = if stealth { STEALTH_BAND } else { BARRIER_BAND };
        self.bands.push(Band { id, texture, at, start: now, h, turn, alpha, phase });
    }

    /// A ground unit went: its band with it.
    pub fn forget(&mut self, id: u32) {
        self.bands.retain(|band| band.id != id);
    }

    /// Makes the arcs of the ticks that have come (where the caster stands then, `h` from `rand(n)`) and runs their alpha a tick
    /// at a time (5 + R(4) more a tick to the 25th, 2 less a tick from the 38th).
    pub fn advance(&mut self, now: Instant, position_of: impl Fn(Option<u32>) -> Option<Vector>, mut rand: impl FnMut(u32) -> u32) {
        for field in &mut self.fields {
            field.last = position_of(field.caster).or(field.last);
            let due = passes(field.start, now);
            while field.prims < ARC_TICKS && due > field.prims {
                if let Some(feet) = field.last {
                    for base in [0.0, 180.0] {
                        for k in 0..4 {
                            let h = 0.01 * rand(11) as f32;
                            field.arcs.push(Arc {
                                made: field.prims,
                                dir: 45.0 * k as f32 + base,
                                h,
                                origin: [feet[0], feet[1] + 5.0, feet[2]],
                                alpha: Vec::new(),
                            });
                        }
                    }
                }
                field.prims += 1;
            }
            for arc in &mut field.arcs {
                let ticks = due.saturating_sub(arc.made).min(ARC_LIFE - 1 - arc.made);
                while (arc.alpha.len() as u32) < ticks {
                    let n = arc.alpha.len() as u32 + 1;
                    let before = arc.alpha.last().copied().unwrap_or(0.0);
                    let alpha = match n {
                        1..=25 => before + 5.0 + rand(4) as f32,
                        26..=37 => before,
                        _ => (before - 2.0).max(0.0),
                    };
                    arc.alpha.push(alpha);
                }
            }
        }
        self.fields.retain(|field| passes(field.start, now) <= ARC_LIFE);
        self.bands.retain(|band| passes(band.start, now) <= BAND_LIFE);
    }

    /// Every mesh at `now`, added on. The arcs jitter afresh each frame from `rand(n)`.
    pub fn meshes(&self, now: Instant, rand: impl Fn(u32) -> u32) -> Vec<Wall> {
        let mut walls = Vec::new();
        for field in &self.fields {
            let Some(t) = passes(field.start, now).checked_sub(1) else { continue };
            let mut vertices = Vec::new();
            for arc in &field.arcs {
                let Some(n) = arc.age(t) else { continue };
                let jitter = || [0.1 * rand(7) as f32 - 0.3, 0.1 * rand(7) as f32 - 0.3, 0.1 * rand(7) as f32 - 0.3];
                let points = arc.points(n, jitter);
                for s in 1..=18 {
                    let alpha = arc.alpha_at(n) - 5.0 * s as f32;
                    if alpha <= 0.0 {
                        break;
                    }
                    // A colour for each quad: the green channel is drawn first, then the red.
                    let mut segment = tube(points[s], points[s + 1], [1.0; 3], alpha);
                    for quad in segment.chunks_mut(6) {
                        let green = rand(100) as f32;
                        let red = (200 + rand(50)) as f32;
                        for vertex in quad {
                            vertex.tint[..3].copy_from_slice(&[red / 255.0, green / 255.0, 1.0]);
                        }
                    }
                    vertices.extend(segment);
                }
            }
            if !vertices.is_empty() {
                walls.push(Wall { texture: ELEC, vertices, additive: true, forced: false });
            }
        }
        for band in &self.bands {
            let n = passes(band.start, now);
            if n == 0 {
                continue;
            }
            walls.push(Wall { texture: band.texture, vertices: band.walls(n), additive: true, forced: false });
        }
        walls
    }
}

impl Band {
    /// The breathing radius at age n: 2 to 4 over 360 ticks.
    pub fn radius(&self, n: u32) -> f32 {
        ((self.phase + n as f32) % 360.0).to_radians().sin() + 3.0
    }

    /// The top of point s at age n: zero at both ends, the height wobbling ±30 % in the middle.
    pub fn top(&self, s: usize, n: u32) -> f32 {
        let q = self.h * (9.0 * s as f32).to_radians().sin();
        0.3 * q * ((n % 360) as f32).to_radians().sin() + q
    }

    /// A lens-shaped wall of 20 quads over 300 degrees round the cell's centre.
    fn walls(&self, n: u32) -> Vec<MeshVertex> {
        let turn = (self.turn + 3.0 * n as f32) % 360.0;
        let r = self.radius(n);
        let tint = [BAND_TINT[0], BAND_TINT[1], BAND_TINT[2], self.alpha / 255.0];
        let point = |s: usize| {
            let (sin, cos) = ((15.0 * s as f32 + turn) % 360.0).to_radians().sin_cos();
            let foot = [self.at[0] + r * cos, self.at[1], self.at[2] + r * sin];
            (foot, [foot[0], foot[1] + self.top(s, n), foot[2]])
        };
        let mut vertices = Vec::new();
        for s in 1..POINTS {
            let ((foot0, top0), (foot1, top1)) = (point(s - 1), point(s));
            let j = (s - 1) as f32;
            let [a, b, c, d] = [
                MeshVertex { position: foot0, uv: [j / 21.0, 1.0], tint },
                MeshVertex { position: foot1, uv: [(j + 1.0) / 21.0, 1.0], tint },
                MeshVertex { position: top1, uv: [(j + 1.0) / 21.0, 0.0], tint },
                MeshVertex { position: top0, uv: [j / 21.0, 0.0], tint },
            ];
            vertices.extend([a, b, c, a, c, d]);
        }
        vertices
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
    fn magnetic_field_throws_32_jittering_arcs() {
        let start = Instant::now();
        let feet = [10.0, 1.0, 10.0];
        let mut mechanic = Mechanic::default();
        mechanic.magnetic_field(Some(1), start);
        for tick in 0..8 {
            mechanic.advance(start + TICK * tick, |_| Some(feet), |_| 0);
            // Two prims of four a tick on ticks 0 to 3, none after.
            assert_eq!(mechanic.fields[0].arcs.len() as u32, 8 * (tick + 1).min(4), "tick {tick}");
        }
        let arcs = &mechanic.fields[0].arcs;
        // Directions 0, 45, 90, 135 and 180, 225, 270, 315; with every draw 0: h 0, origin 5 up.
        assert_eq!(arcs[..8].iter().map(|arc| arc.dir).collect::<Vec<_>>(), [0.0, 45.0, 90.0, 135.0, 180.0, 225.0, 270.0, 315.0]);
        assert!(arcs.iter().all(|arc| arc.h == 0.0 && arc.origin == [10.0, 6.0, 10.0]));
        // The profile 180, 180, 171 … 18, 9 and the reach 0.4 n.
        assert_eq!((0..21).map(profile).collect::<Vec<_>>()[..4], [180.0, 180.0, 171.0, 162.0]);
        assert_eq!((profile(19), profile(20)), (18.0, 9.0));
        assert!(near(reach(38), 15.2));
        // Run on: alpha at n = 1, 25, 37, 38, 100: 5, 125, 125, 123, 0.
        for tick in 8..140 {
            mechanic.advance(start + TICK * tick, |_| Some(feet), |_| 0);
        }
        let arc = &mechanic.fields[0].arcs[0];
        assert_eq!([1, 25, 37, 38, 100].map(|n| arc.alpha_at(n)), [5.0, 125.0, 125.0, 123.0, 0.0]);
        // Segments 1 to 18 only, three quads each: while the alpha is over 90 all show, 32 arcs × 18 × 3 quads × 6 vertices.
        let early = mechanic.meshes(start + TICK * 25, |_| 0);
        assert!(early[0].additive && early[0].texture == ELEC);
        assert_eq!(early[0].vertices.len(), 32 * 18 * 3 * 6);
        // Gone once the alpha is: nothing at the end of the field's 300 ticks.
        assert!(mechanic.meshes(start + TICK * 139, |_| 0).is_empty());
        // The arcs stay where the caster stood and jitter by ±0.3: a draw of 6 on all three is +0.3.
        let points = arc.points(10, || [0.0; 3]);
        assert_eq!(points.len(), 21);
        assert!(near(points[0][0], 10.0 + 4.0 * 2.0) && near(points[0][1], 6.0 + 2.0));
        let moved = arc.points(10, || [0.3, 0.3, 0.3]);
        assert!(near(moved[1][0] - points[1][0], 0.3) && near(moved[1][1] - points[1][1], -0.3) && near(moved[1][2] - points[1][2], 0.3));
        assert_eq!(moved[20], points[20]);
    }

    #[test]
    fn neutral_barrier_and_stealth_field_cells_breathe_a_ground_band() {
        let start = Instant::now();
        let mut mechanic = Mechanic::default();
        mechanic.keep_band(4, false, [10.0, 1.0, 10.0], start, |_| 0);
        mechanic.keep_band(5, true, [20.0, 1.0, 20.0], start, |_| 0);
        // Every draw 0: h 25, alpha 50; the textures differ; 20 quads covering 300 degrees; additive, #FFFFAA.
        let walls = mechanic.meshes(start, |_| 0);
        assert_eq!(walls.len(), 2);
        assert_eq!((walls[0].texture, walls[1].texture), (BARRIER_BAND, STEALTH_BAND));
        assert!(walls.iter().all(|wall| wall.additive && wall.vertices.len() == 20 * 6));
        assert_eq!(walls[0].vertices[0].tint, [1.0, 1.0, 170.0 / 255.0, 50.0 / 255.0]);
        let band = &mechanic.bands[0];
        // r = 3 + sin φ: 3.0175 at n = 1; p_s at s = 10 for θ = 90 is 1.3 · 25 = 32.5; zero at both ends.
        assert!(near(band.radius(1), 3.0 + 1.0_f32.to_radians().sin()));
        assert!(near(band.top(10, 90), 32.5) && band.top(0, 5) == 0.0 && band.top(20, 5).abs() < 1e-3);
        // The top is p_s above the foot; it turns 3° a tick.
        let first = &walls[0].vertices;
        assert!(near(first[2].position[1] - first[1].position[1], band.top(1, 1)));
        // It goes with its unit.
        mechanic.forget(4);
        assert_eq!(mechanic.bands.len(), 1);
        mechanic.advance(start + TICK * 30_000, |_| None, |_| 0);
        assert!(mechanic.bands.is_empty());
    }
}
