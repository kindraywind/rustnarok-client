//! Infrared Scan's radar rays (794): two jagged red rays that sweep round the Mechanic in opposite directions, each dragging a wedge,
//! a 16 ms tick at a time.
//!
//! An element's own age n counts from 1 at its first tick; the effect lives 300 ticks.

use std::time::Instant;

use super::mechanic::{ELEC, profile};
use super::passes;
use super::tanji::bearing;
use super::walls::Wall;
use crate::world::{MeshVertex, Vector};

const LIFE: u32 = 300;
/// Two prims (one turns each way) of four elements, each lagging 10 ticks behind the one before.
const PRIMS: usize = 2;
const ELEMENTS: usize = 4;
const POINTS: usize = 20;
/// The ray's start alpha by element, and the radius of its tube.
const ALPHA: [f32; ELEMENTS] = [250.0, 110.0, 80.0, 50.0];
const TUBE: f32 = 0.1;
/// How high the ray stands over the feet, and how far its first points reach.
const HEIGHT: f32 = 12.0;
const REACH: f32 = 20.0;

/// One ray: the way it points, its arch, its age and its alpha.
#[derive(Clone, Copy, Debug)]
pub struct Element {
    arch: f32,
    heading: i32,
    alpha: f32,
    age: u32,
}

/// One cast: its prims, each of four elements, on the caster `caster` (None = the player).
struct Scan {
    start: Instant,
    caster: Option<u32>,
    ticks: u32,
    prims: Vec<[Element; ELEMENTS]>,
}

#[derive(Default)]
pub struct Radar {
    scans: Vec<Scan>,
}

impl Element {
    /// An element for a caster facing `facing` degrees (as `Actor::rot`: 0 toward −z, 90 toward +x) with arch `arch`.
    fn new(k: usize, facing: f32, arch: f32) -> Self {
        Self { arch, heading: heading(facing), alpha: ALPHA[k], age: 0 }
    }

    /// One tick: element `k` of the prim turning `turn` degrees a tick (−2 or +2) turns after its wait and fades after 60.
    fn step(&mut self, k: usize, turn: i32) {
        self.age += 1;
        if self.age > 10 * k as u32 {
            self.heading = (self.heading + turn).rem_euclid(360);
        }
        if self.age > 60 {
            self.alpha = (self.alpha - 6.0).max(0.0);
        }
    }

    /// The ray's points over `feet` with the frame's three shifts from `jitter()` for each.
    fn points(&self, feet: Vector, mut jitter: impl FnMut() -> [f32; 3]) -> Vec<Vector> {
        let (sin, cos) = (self.heading as f32).to_radians().sin_cos();
        (0..POINTS)
            .map(|i| {
                let r = profile(i) / 90.0 * REACH;
                let [a, b, c] = jitter();
                [feet[0] + cos * r + b, feet[1] + HEIGHT + REACH * self.arch * profile(i).to_radians().sin() + a, feet[2] + sin * r + c]
            })
            .collect()
    }
}

/// The caster's heading in whole degrees: his facing less 90, wrapped into 0 … 359.
pub fn heading(facing: f32) -> i32 {
    (facing.floor() as i32 - 90).rem_euclid(360)
}

/// The four corners round the segment `a` to `b`: offset j is a tube corner 120° j round it, and the first (and the last, 360°) is the one the
/// sheets use.
fn corner(a: Vector, b: Vector, j: u32) -> (Vector, Vector) {
    let (sin, cos) = ((bearing(a[0] - b[0], a[2] - b[2]) as f32 + 90.0) % 360.0).to_radians().sin_cos();
    let (s, c) = (120.0 * j as f32).to_radians().sin_cos();
    let offset = [TUBE * c * cos, TUBE * s, TUBE * c * sin];
    ([a[0] + offset[0], a[1] + offset[1], a[2] + offset[2]], [b[0] + offset[0], b[1] + offset[1], b[2] + offset[2]])
}

/// A quad `p`, `q`, `r`, `s` at uv (0, 1), (1, 1), (1, 0), (0, 0) in `rgb` at `alpha`.
fn quad(p: Vector, q: Vector, r: Vector, s: Vector, rgb: [f32; 3], alpha: f32) -> [MeshVertex; 6] {
    let tint = [rgb[0], rgb[1], rgb[2], alpha / 255.0];
    let [a, b, c, d] = [
        MeshVertex { position: p, uv: [0.0, 1.0], tint },
        MeshVertex { position: q, uv: [1.0, 1.0], tint },
        MeshVertex { position: r, uv: [1.0, 0.0], tint },
        MeshVertex { position: s, uv: [0.0, 0.0], tint },
    ];
    [a, b, c, a, c, d]
}

impl Radar {
    /// Infrared Scan from the caster `caster` facing `facing` degrees: each of the 8 elements draws its arch from `rand(n)`.
    pub fn scan(&mut self, caster: Option<u32>, facing: f32, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        let prims = (0..PRIMS).map(|_| std::array::from_fn(|k| Element::new(k, facing, 0.01 * rand(11) as f32))).collect();
        self.scans.push(Scan { start: now, caster, ticks: 0, prims });
    }

    /// Runs the rays up to `now`, a tick at a time, and drops the scans that ended.
    pub fn advance(&mut self, now: Instant) {
        for scan in &mut self.scans {
            while scan.ticks < passes(scan.start, now).min(LIFE) {
                scan.ticks += 1;
                for (v, elements) in scan.prims.iter_mut().enumerate() {
                    for (k, element) in elements.iter_mut().enumerate() {
                        element.step(k, if v == 0 { -2 } else { 2 });
                    }
                }
            }
        }
        self.scans.retain(|scan| passes(scan.start, now) <= LIFE);
    }

    /// Every ray at `now` over its caster's feet, added on; the points shake afresh each frame from `rand(n)`.
    pub fn meshes(&self, now: Instant, position_of: impl Fn(Option<u32>) -> Option<Vector>, mut rand: impl FnMut(u32) -> u32) -> Vec<Wall> {
        let mut walls = Vec::new();
        for scan in &self.scans {
            let (Some(feet), true) = (position_of(scan.caster), passes(scan.start, now) > 0) else { continue };
            let mut vertices = Vec::new();
            for elements in &scan.prims {
                // Element 0's corners of each segment, which the other elements' sheets reach for.
                let mut first: Vec<(Vector, Vector)> = Vec::new();
                for (k, element) in elements.iter().enumerate() {
                    if element.age == 0 || element.alpha <= 0.0 {
                        continue;
                    }
                    let mut shake = || [0.1 * rand(7) as f32 - 0.3, 0.1 * rand(7) as f32 - 0.3, 0.1 * rand(7) as f32 - 0.3];
                    let points = element.points(feet, &mut shake);
                    for i in 1..=POINTS - 2 {
                        let (a, b) = (points[i], points[i + 1]);
                        let rim = corner(a, b, 0);
                        if k == 0 {
                            first.push(rim);
                            let alpha = element.alpha - 5.0 * i as f32;
                            if alpha <= 0.0 {
                                continue;
                            }
                            let rgb = [1.0, 3.0 * i as f32 / 255.0, 3.0 * i as f32 / 255.0];
                            for j in 1..=3 {
                                let (near, far) = (corner(a, b, j), corner(a, b, j - 1));
                                vertices.extend(quad(near.0, near.1, far.1, far.0, rgb, alpha));
                            }
                        } else if let Some(zero) = first.get(i - 1) {
                            let alpha = (element.alpha / [0.0, 3.0, 2.0, 1.0][k]).floor();
                            if alpha > 0.0 {
                                vertices.extend(quad(rim.0, rim.1, zero.1, zero.0, [1.0, 0.0, 0.0], alpha));
                            }
                        }
                    }
                }
            }
            if !vertices.is_empty() {
                walls.push(Wall { texture: ELEC, vertices, additive: true, forced: false });
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

    fn ticks(radar: &mut Radar, start: Instant, from: u32, to: u32) {
        for tick in from..to {
            radar.advance(start + TICK * tick);
        }
    }

    #[test]
    fn infrared_scan_makes_two_prims_of_four_elements_with_an_arch_each() {
        let start = Instant::now();
        let mut radar = Radar::default();
        let mut draws = Vec::new();
        radar.scan(Some(1), 90.0, start, |n| {
            draws.push(n);
            3
        });
        // One draw an element, R(11), and none later.
        assert_eq!(draws, [11; 8]);
        ticks(&mut radar, start, 0, 100);
        assert_eq!(draws.len(), 8);
        let scan = &radar.scans[0];
        assert!(scan.prims.len() == 2 && scan.prims.iter().all(|elements| elements.len() == 4));
        assert!(scan.prims.iter().flatten().all(|element| near(element.arch, 0.03)));
        // The heading is the facing less 90, wrapped: facing 90 → 0, facing 0 → 270, facing 45 → 315.
        assert_eq!((heading(90.0), heading(0.0), heading(45.0)), (0, 270, 315));
    }

    #[test]
    fn the_alpha_holds_to_tick_60_and_fades_6_a_tick() {
        let start = Instant::now();
        let mut radar = Radar::default();
        radar.scan(None, 90.0, start, |_| 0);
        let mut by_age = vec![[0.0; 4]; 103];
        for tick in 0..103 {
            radar.advance(start + TICK * tick);
            let element = radar.scans.first().map(|scan| scan.prims[0]);
            if let Some(elements) = element {
                by_age[tick as usize] = [0, 1, 2, 3].map(|k| elements[k].alpha);
            }
        }
        // Advance made n = tick + 1 ticks by `tick`; k = 0 at n = 1, 60, 61, 101, 102.
        assert_eq!([0, 59, 60, 100, 101].map(|tick| by_age[tick][0]), [250.0, 250.0, 244.0, 4.0, 0.0]);
        // The lagging copies reach 0 at n = 79, 74 and 69.
        assert_eq!((by_age[78][1], by_age[77][1]), (0.0, 2.0));
        assert_eq!((by_age[73][2], by_age[72][2]), (0.0, 2.0));
        assert_eq!((by_age[68][3], by_age[67][3]), (0.0, 2.0));
    }

    #[test]
    fn the_two_prims_turn_opposite_ways_and_element_k_waits_10_k_ticks() {
        let start = Instant::now();
        let mut radar = Radar::default();
        radar.scan(None, 90.0, start, |_| 0);
        // Element 1 of the left prim (v = 0) after n = 10, 11, 12: H, H − 2, H − 4 (wrapped); of the right prim: H, H + 2, H + 4.
        let mut seen = Vec::new();
        for tick in 0..12 {
            radar.advance(start + TICK * tick);
            let scan = &radar.scans[0];
            seen.push((scan.prims[0][1].heading, scan.prims[1][1].heading, scan.prims[0][0].heading));
        }
        assert_eq!((seen[9].0, seen[10].0, seen[11].0), (0, 358, 356));
        assert_eq!((seen[9].1, seen[10].1, seen[11].1), (0, 2, 4));
        // Element 0 turns at once: −2 on its first tick.
        assert_eq!(seen[0].2, 358);
    }

    #[test]
    fn the_ray_runs_from_40_to_4_units_out_and_follows_the_feet() {
        let element = Element::new(0, 90.0, 0.0);
        let feet = [10.0, 1.0, 10.0];
        let points = element.points(feet, || [0.0; 3]);
        // The profile 180, 180, 171 … 18 and the reach 20 / 90 of it: 40, 40 … 4.
        assert_eq!(points.len(), 20);
        assert!(near(points[1][0] - feet[0], 40.0) && near(points[19][0] - feet[0], 4.0) && near(points[0][0] - feet[0], 40.0));
        // Heading 0 (x = cos H, z = sin H) and 12 up with no arch.
        assert!(points.iter().all(|point| near(point[1], 13.0) && near(point[2], 10.0)));
        // An arch of 0.1 bows the middle up by 20 f at most; the shifts add on.
        let arched = Element::new(0, 90.0, 0.1).points(feet, || [0.3, -0.3, 0.3]);
        assert!(near(arched[10][1], 13.0 + 2.0 * (9.0 * 11.0_f32).to_radians().sin() + 0.3));
        assert!(near(arched[1][0], 10.0 + 40.0 - 0.3) && near(arched[1][2], 10.0 + 0.3));
        // Another place for the feet moves it, the heading stays.
        let moved = element.points([20.0, 1.0, 30.0], || [0.0; 3]);
        assert!(near(moved[5][0] - points[5][0], 10.0) && near(moved[5][2] - points[5][2], 20.0));
    }

    #[test]
    fn a_frame_draws_a_tube_for_element_0_and_a_sheet_for_each_other_element() {
        let start = Instant::now();
        let mut radar = Radar::default();
        radar.scan(Some(1), 90.0, start, |_| 0);
        ticks(&mut radar, start, 0, 25);
        let now = start + TICK * 24;
        let walls = radar.meshes(now, |_| Some([0.0; 3]), |_| 0);
        assert!(walls.len() == 1 && walls[0].additive && walls[0].texture == ELEC && !walls[0].forced);
        // Per prim: 18 segments × 3 tube quads for element 0 and 18 sheet quads for each of the other three; two prims.
        assert_eq!(walls[0].vertices.len(), 2 * (18 * 3 + 3 * 18) * 6);
        // The tube colour is (255, 3i, 3i) with the alpha less 5 i; segment 1 first, its three quads.
        let tube = &walls[0].vertices[..18];
        assert_eq!(tube[0].tint, [1.0, 3.0 / 255.0, 3.0 / 255.0, (250.0 - 5.0) / 255.0]);
        let last = &walls[0].vertices[17 * 18..18 * 18];
        assert_eq!(last[0].tint, [1.0, 54.0 / 255.0, 54.0 / 255.0, (250.0 - 90.0) / 255.0]);
        // The sheets are red at a third, a half and all of their alpha (element 1: 110 → 36 at n = 25).
        let sheet = &walls[0].vertices[18 * 18..18 * 18 + 6];
        assert_eq!(sheet[0].tint, [1.0, 0.0, 0.0, 36.0 / 255.0]);
        // No caster in sight, no scan.
        assert!(radar.meshes(now, |_| None, |_| 0).is_empty());
        // Nothing before the first tick, and gone with the 300 ticks.
        assert!(radar.meshes(start - TICK, |_| Some([0.0; 3]), |_| 0).is_empty());
        radar.advance(start + TICK * 310);
        assert!(radar.scans.is_empty());
    }
}
