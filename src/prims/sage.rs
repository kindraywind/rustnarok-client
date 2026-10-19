//! The Sage's ground skills, as the game draws them a 16 ms tick at a time: Volcano's, Deluge's, Violent Gale's and
//! Land Protector's look at the cell as they're cast (225, 236, 237, 238: four spinning crescent ribbons opening out
//! and fading), and what each cell of them keeps showing while it lasts (239–241: a ring opening like a flower every
//! second; 242: a flat square breathing).

use std::time::Instant;

use super::TICK;
use crate::world::Vector;

/// The ground looks' effect ids and textures.
pub const LOOKS: [(u32, &str); 4] = [
    (225, "data\\texture\\effect\\ring_red.tga"),
    (236, "data\\texture\\effect\\ring_blue.tga"),
    (237, "data\\texture\\effect\\magic_green.tga"),
    (238, "data\\texture\\effect\\ring_white.tga"),
];
/// The loops' effect ids and textures; Land Protector's square is additive.
pub const LOOPS: [(u32, &str); 4] = [
    (239, "data\\texture\\effect\\ring_red.tga"),
    (240, "data\\texture\\effect\\ring_blue.tga"),
    (241, "data\\texture\\effect\\magic_green.tga"),
    (LAND_PROTECTOR, "data\\texture\\effect\\aaa copy.bmp"),
];
pub const LAND_PROTECTOR: u32 = 242;

/// A ground look shows for 110 ticks (its ribbons faded by then), though the effect lives on unseen.
const LOOK_TICKS: u32 = 110;
const RIBBONS: usize = 4;
const RIBBON_QUADS: usize = 20;
const RIBBON_HEIGHT: f32 = 25.0;
const PETAL_QUADS: usize = 10;
const PETAL_HEIGHT: f32 = 15.0;

/// A corner of a shape: where (world units), texture coordinate and opacity.
pub type Corner = (Vector, [f32; 2], f32);

/// One ground look at a cell.
struct Look {
    start: Instant,
    texture: &'static str,
    at: Vector,
}

/// One cell's loop, kept until the cell's unit goes.
struct Loop {
    start: Instant,
    unit: u32,
    texture: &'static str,
    at: Vector,
    /// Ticks run.
    ticks: u32,
    /// Volcano's, Deluge's and Violent Gale's ring (kind 36): its turn (never changes), opacity, radius and lean.
    turn: f32,
    alpha: i32,
    radius: f32,
    lean: i32,
    /// Land Protector's square (kind 39): where in its breath it is (degrees).
    spin: u32,
}

/// The Sage's ground effects showing.
#[derive(Default)]
pub struct Sage {
    looks: Vec<Look>,
    loops: Vec<Loop>,
}

/// A point `radius` out at `angle` (degrees: 0 east, 90 north) with a top `height` along a `lean` from the ground.
pub(super) fn ring_point(at: Vector, angle: f32, radius: f32, height: f32, lean: f32) -> (Vector, Vector) {
    let (sin, cos) = angle.to_radians().sin_cos();
    let (lean_sin, lean_cos) = lean.to_radians().sin_cos();
    let out = radius + height * lean_cos;
    ([at[0] + radius * cos, at[1], at[2] + radius * sin], [at[0] + out * cos, at[1] + height * lean_sin, at[2] + out * sin])
}

/// Quads round a ring, `u` stepping a 21st each, `v` 1 at the foot and 0 at the top.
pub(super) fn ring_quads(points: &[(Vector, Vector)], alpha: f32) -> Vec<Corner> {
    points
        .windows(2)
        .enumerate()
        .flat_map(|(quad, pair)| {
            let ((foot, top), (next_foot, next_top)) = (pair[0], pair[1]);
            let u = [quad as f32 / 21.0, (quad + 1) as f32 / 21.0];
            let [a, b, c, d] = [(foot, [u[0], 1.0], alpha), (top, [u[0], 0.0], alpha), (next_foot, [u[1], 1.0], alpha), (next_top, [u[1], 0.0], alpha)];
            [a, b, c, b, c, d]
        })
        .collect()
}

/// A ground look's ribbons at age `age` (from 1): each fades in to 200 over 10 ticks and out by 2 a tick, opens 0.3 a
/// tick from radius 1–4, leans out from 80° to 40°, turns 5° a tick 90° apart, and is a crescent 25 high at most.
fn ribbons(at: Vector, age: u32) -> Vec<Corner> {
    let t = age as f32;
    let alpha = if age <= 10 { 20.0 * t } else { (200.0 - 2.0 * (t - 10.0)).max(0.0) } / 255.0;
    let lean = (80.0 - t).max(40.0);
    (0..RIBBONS)
        .flat_map(|ribbon| {
            let radius = (ribbon + 1) as f32 + 0.3 * t;
            let turn = (90 * ribbon as u32 + 5 * age) % 360;
            let swing = 1.0 + 0.15 * (((age + 90 * ribbon as u32) % 360) as f32).to_radians().sin();
            let points: Vec<_> = (0..=RIBBON_QUADS)
                .map(|s| {
                    let height = RIBBON_HEIGHT * (9.0 * s as f32).to_radians().sin() * swing;
                    ring_point(at, (turn + 18 * s as u32) as f32, radius, height, lean)
                })
                .collect();
            ring_quads(&points, alpha)
        })
        .collect()
}

impl Loop {
    /// One tick of a Volcano, Deluge or Violent Gale ring: it opens 0.04 and leans 1° more a tick; up to radius 2.5 it
    /// brightens 20 a tick, past it it fades 2, and at 0 it starts again (radius 2.14, lean 74): a 65-tick cycle.
    fn step(&mut self) {
        self.ticks += 1;
        self.spin = (self.spin + 3) % 360;
        self.radius += 0.04;
        self.lean -= 1;
        if self.lean < 10 {
            self.lean = 10;
            self.alpha = 0;
        }
        if self.radius >= 2.5 {
            self.alpha -= 2;
            if self.alpha <= 0 {
                (self.alpha, self.radius, self.lean) = (0, 2.14, 74);
            }
        } else {
            self.alpha += 20;
        }
    }

    /// What it draws now.
    fn shape(&self) -> Vec<Corner> {
        if self.texture == LOOPS[3].1 {
            // Land Protector: a flat square 2 over the cell, corners on the axes, breathing between 5.6 and 7 out.
            let reach = 5.6 + 0.7 * (1.0 + (self.spin as f32).to_radians().sin());
            let alpha = 100.0 / 255.0;
            let corner = |x: f32, z: f32, uv: [f32; 2]| ([self.at[0] + x, self.at[1] + 2.0, self.at[2] + z], uv, alpha);
            let [c0, c1, c2, c3] =
                [corner(reach, 0.0, [0.0, 1.0]), corner(0.0, reach, [1.0, 1.0]), corner(-reach, 0.0, [1.0, 0.0]), corner(0.0, -reach, [0.0, 0.0])];
            return vec![c0, c1, c2, c3, c2, c0];
        }
        // The ring: ten quads from its turn, 15 high with a 30% swing on the far side as the ticks go.
        let swing = (self.ticks % 360) as f32;
        let points: Vec<_> = (0..=PETAL_QUADS)
            .map(|s| {
                let height = PETAL_HEIGHT + (9.0 * s as f32).to_radians().sin() * PETAL_HEIGHT * 0.3 * swing.to_radians().sin();
                let angle = if s == PETAL_QUADS { self.turn } else { (36.0 * s as f32 + self.turn) % 360.0 };
                ring_point(self.at, angle, self.radius, height, self.lean as f32)
            })
            .collect();
        ring_quads(&points, self.alpha.max(0) as f32 / 255.0)
    }
}

impl Sage {
    /// A ground look (225, 236, 237 or 238) at `at`, cast `now`.
    pub fn look(&mut self, id: u32, at: Vector, now: Instant) {
        if let Some(&(_, texture)) = LOOKS.iter().find(|(look, _)| *look == id) {
            self.looks.push(Look { start: now, texture, at });
        }
    }

    /// A cell's loop (239–242) for skill unit `unit` at `at`, from `rand(n)`.
    pub fn keep(&mut self, id: u32, unit: u32, at: Vector, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        if let Some(&(_, texture)) = LOOPS.iter().find(|(kept, _)| *kept == id) {
            self.loops.retain(|each| each.unit != unit);
            let (turn, alpha) = (rand(360) as f32, rand(100) as i32);
            self.loops.push(Loop { start: now, unit, texture, at, ticks: 0, turn, alpha, radius: 2.6, lean: 80, spin: rand(360) });
        }
    }

    /// The unit went: its loop goes with it.
    pub fn forget(&mut self, unit: u32) {
        self.loops.retain(|each| each.unit != unit);
    }

    pub fn advance(&mut self, now: Instant) {
        let due = |start: Instant| (now.saturating_duration_since(start).as_millis() / TICK.as_millis()) as u32;
        self.looks.retain(|look| due(look.start) < LOOK_TICKS);
        for each in &mut self.loops {
            let due = due(each.start) + 1;
            while each.ticks < due {
                each.step();
            }
        }
    }

    /// Everything to draw at `now`: its texture, whether it adds on, and its triangles.
    pub fn shapes(&self, now: Instant) -> Vec<(&'static str, bool, Vec<Corner>)> {
        let looks = self.looks.iter().filter_map(|look| {
            let age = (now.checked_duration_since(look.start)?.as_millis() / TICK.as_millis()) as u32 + 1;
            (age <= LOOK_TICKS).then(|| (look.texture, false, ribbons(look.at, age)))
        });
        let loops = self.loops.iter().map(|each| (each.texture, each.texture == LOOPS[3].1, each.shape()));
        looks.chain(loops).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sage_looks_and_loops() {
        let start = Instant::now();
        let mut sage = Sage::default();
        // Volcano's look: four ribbons of 20 quads, faded out after 110 ticks.
        sage.look(225, [0.0; 3], start);
        assert_eq!(sage.shapes(start)[0].2.len(), RIBBONS * RIBBON_QUADS * 6);
        let first = ribbons([0.0; 3], 10);
        assert_eq!(first[0].2, 200.0 / 255.0);
        assert_eq!(ribbons([0.0; 3], 110)[0].2, 0.0);
        sage.advance(start + TICK * 110);
        assert!(sage.looks.is_empty());
        // A Deluge cell: starting from alpha 40 at radius 2.6 it fades 2 a tick, resets after 20, then runs 65-tick
        // cycles: 9 ticks brightening to 180, then fading.
        sage.keep(240, 7, [0.0; 3], start, |n| if n == 100 { 40 } else { 0 });
        let mut cell = sage.loops.pop().unwrap();
        (0..20).for_each(|_| cell.step());
        assert_eq!((cell.alpha, cell.radius, cell.lean), (0, 2.14, 74));
        (0..9).for_each(|_| cell.step());
        assert_eq!(cell.alpha, 180);
        (0..56).for_each(|_| cell.step());
        assert_eq!((cell.alpha, cell.lean), (0, 74));
        assert_eq!(cell.shape().len(), PETAL_QUADS * 6);
        // Land Protector's square breathes between 5.6 and 7 out, additive at alpha 100.
        sage.keep(LAND_PROTECTOR, 8, [0.0; 3], start, |_| 90);
        let shapes = sage.shapes(start);
        let (_, additive, square) = &shapes[0];
        assert!(*additive && square.len() == 6 && (square[0].0[0] - 7.0).abs() < 1e-4);
        sage.forget(8);
        assert!(sage.loops.is_empty());
    }
}
