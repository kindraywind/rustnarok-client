//! Resurrection's rising golden funnel rings (140) and the Call Partner portal's pink warp (341), a 16 ms tick at a time.
//!
//! A ring's own age a counts from 0 on its first tick; a band's age n from 1 on its first; e is the ticks since the effect started.

use std::time::Instant;

use super::passes;
use super::walls::{RING_YELLOW, Wall};
use crate::world::{MeshVertex, Vector};

pub const RING_RED: &str = "data\\texture\\effect\\ring_red.tga";
pub const CLOUD: &str = "data\\texture\\effect\\cloud11.tga";
/// The resurrection lasts 250 ticks; a ring is made every 25.
const REVIVE_LENGTH: u32 = 250;
const REVIVE_EVERY: u32 = 25;
const REVIVE_ALPHA: f32 = 240.0;
/// The portal starts again 8 ticks before its 9 999.
const PORTAL_PERIOD: u32 = 9991;
/// The warp's colour (#FF59B6).
const PINK: [f32; 3] = [1.0, 89.0 / 255.0, 182.0 / 255.0];
/// The flat rings are held back after this many ticks of the prim.
const FLAT_STOP: u32 = 1400;

/// One golden funnel ring: the inner and outer radius and their speeds and changes, the yaw, the alpha and its step.
#[derive(Clone, Debug)]
pub struct Ring {
    life: u32,
    fade_at: u32,
    age: u32,
    pub inner: f32,
    pub outer: f32,
    inner_speed: f32,
    inner_change: f32,
    outer_speed: f32,
    outer_change: f32,
    pub yaw: f32,
    yaw_speed: f32,
    yaw_change: f32,
    pub alpha: f32,
    alpha_step: f32,
}

impl Ring {
    /// The ring made at tick `e` of the effect: the first stands still turning 3° a tick; the others shrink and turn inside out, and
    /// one the effect's end would cut fades from its first tick.
    pub fn new(e: u32) -> Self {
        if e == 0 {
            return Self {
                life: REVIVE_LENGTH,
                fade_at: 225,
                age: 0,
                inner: 7.5,
                outer: 11.0,
                inner_speed: 0.0,
                inner_change: 0.0,
                outer_speed: 0.0,
                outer_change: 0.0,
                yaw: 0.0,
                yaw_speed: 1.5,
                yaw_change: 0.0,
                alpha: REVIVE_ALPHA,
                alpha_step: 0.0,
            };
        }
        let cut = e + 60 > REVIVE_LENGTH;
        let (life, fade_at) = if cut { (REVIVE_LENGTH - e - 1, 0) } else { (60, 30) };
        Self {
            life,
            fade_at,
            age: 0,
            inner: 7.5,
            outer: 10.5,
            inner_speed: -0.12,
            inner_change: 0.003,
            outer_speed: -0.12,
            outer_change: 0.003,
            yaw: 0.0,
            yaw_speed: 1.5,
            yaw_change: 0.03,
            alpha: REVIVE_ALPHA,
            alpha_step: 0.0,
        }
    }

    /// One tick: the speeds change, the yaw grows twice by its speed, the radii by theirs; at its fade tick the outer rim starts to
    /// close and the alpha to fall to nothing.
    pub fn step(&mut self) {
        let a = self.age;
        self.inner_speed += self.inner_change;
        self.outer_speed += self.outer_change;
        self.yaw_speed += self.yaw_change;
        self.yaw += 2.0 * self.yaw_speed;
        self.inner += self.inner_speed;
        self.outer += self.outer_speed;
        if a == self.fade_at {
            self.outer_speed = -self.outer / (self.life - a + 1) as f32;
            self.alpha_step = -REVIVE_ALPHA / (self.life - a) as f32;
        }
        self.alpha = (self.alpha + self.alpha_step).max(0.0);
        self.age += 1;
    }

    /// The ring's 10 sides as triangles round the feet `at`: the inner point at the feet, the outer 7.5 above it.
    fn mesh(&self, at: Vector) -> Vec<MeshVertex> {
        let tint = [1.0, 1.0, 1.0, self.alpha / 255.0];
        let point = |side: u32, outer: bool, uv: [f32; 2]| {
            let (sin, cos) = (36.0 * side as f32 + self.yaw).to_radians().sin_cos();
            let (reach, up) = if outer { (self.outer, 7.5) } else { (self.inner, 0.0) };
            MeshVertex { position: [at[0] + reach * cos, at[1] + up, at[2] + reach * sin], uv, tint }
        };
        let mut vertices = Vec::with_capacity(60);
        for side in 0..10 {
            let (u0, u1) = ((side % 4) as f32 * 0.25, (side % 4) as f32 * 0.25 + 0.25);
            let (inner0, outer0, inner1, outer1) =
                (point(side, false, [u0, 1.0]), point(side, true, [u0, 0.0]), point(side + 1, false, [u1, 1.0]), point(side + 1, true, [u1, 0.0]));
            vertices.extend([inner0, outer0, inner1, inner1, outer0, outer1]);
        }
        vertices
    }
}

/// Resurrection's rings over the target's feet `at`, made at tick 0.
struct Revive {
    at: Vector,
    start: Instant,
    ticks: u32,
    rings: Vec<Ring>,
}

/// One band of the upright flame cylinders (prim 1): its radius and the start of its turn.
struct Cylinder {
    radius: f32,
    turn: f32,
    step: f32,
}

/// One flat ring closing in on the centre (prim 2).
struct Flat {
    age: i32,
    radius: f32,
    alpha: f32,
    lean: f32,
    ended: bool,
}

/// One arc ribbon (prim 3).
struct Arc {
    height: f32,
    turn: f32,
    radius: f32,
}

/// The pink warp on the cell `at` of skill unit `id`, this copy made at `start`.
struct Warp {
    id: u32,
    at: Vector,
    start: Instant,
    cylinders: [Cylinder; 3],
    arcs: [Arc; 4],
    flats: [Flat; 3],
    /// The flat rings' prim: its ticks run.
    flat_ticks: u32,
    done: u32,
}

#[derive(Default)]
pub struct Portal {
    revives: Vec<Revive>,
    warps: Vec<Warp>,
}

/// A cylinder band's alpha at its age n: 5 a tick to 75 (at 15), held to 1 400, then 2 less a tick.
pub fn cylinder_alpha(n: u32) -> f32 {
    match n {
        0 => 0.0,
        1..=15 => 5.0 * n as f32,
        16..=1400 => 75.0,
        _ => (75.0 - 2.0 * (n - 1400) as f32).max(0.0),
    }
}

/// A cylinder band's height at its age n: 50 sin n° up to 90, then 50.
pub fn cylinder_height(n: u32) -> f32 {
    if n <= 90 { 50.0 * (n as f32).to_radians().sin() } else { 50.0 }
}

/// An arc band's alpha at its age n: 10 a tick to 110 (at 11), held to 1 400, then 3 less a tick.
pub fn arc_alpha(n: u32) -> f32 {
    match n {
        0 => 0.0,
        1..=11 => 10.0 * n as f32,
        12..=1400 => 110.0,
        _ => (110.0 - 3.0 * (n - 1400) as f32).max(0.0),
    }
}

/// An arc band's span in degrees at its age n: 30 and 3 a tick to 120.
pub fn arc_span(n: u32) -> f32 {
    (30.0 + 3.0 * n as f32).min(120.0)
}

impl Flat {
    fn new(age: i32, lean: f32) -> Self {
        Self { age, radius: 0.0, alpha: 0.0, lean, ended: false }
    }

    /// The ring's rim height at its age: 6.001 sin 9° a tick up to 10, then 6.
    pub fn profile(&self) -> f32 {
        if self.age <= 10 { 6.001 * (9.0 * self.age.max(0) as f32).to_radians().sin() } else { 6.0 }
    }

    /// One tick of a band, `ticks` being its prim's tick count: it closes in by 0.25 and restarts at 14 until the prim is old.
    fn step(&mut self, ticks: u32) {
        if self.ended {
            return;
        }
        self.age += 1;
        if self.age <= 0 {
            return;
        }
        self.radius -= 0.25;
        if self.radius < 7.0 {
            self.radius = self.radius.max(0.0);
            self.alpha -= 7.0;
            if self.alpha < 0.0 {
                self.alpha = 0.0;
                if ticks > FLAT_STOP {
                    self.ended = true;
                    return;
                }
                self.age = 0;
                self.radius = 14.0;
            }
        }
        if self.age < 20 {
            self.alpha = (self.alpha + 12.0).min(240.0);
        }
    }
}

impl Warp {
    fn new(id: u32, at: Vector, start: Instant, rand: &mut impl FnMut(u32) -> u32) -> Self {
        let cylinders = [(4.0, 8.0), (3.0, 9.0), (2.0, 10.0)].map(|(radius, step)| Cylinder { radius, turn: rand(360) as f32, step });
        let arcs = [(1.5, 0.0, 9.0), (3.5, 90.0, 7.5), (5.5, 180.0, 6.0), (7.5, 270.0, 4.5)].map(|(height, turn, radius)| Arc { height, turn, radius });
        let flats = [Flat::new(0, 2.0), Flat::new(-20, 3.0), Flat::new(-40, 4.0)];
        Self { id, at, start, cylinders, arcs, flats, flat_ticks: 0, done: 0 }
    }
}

/// A closed band standing on the ground: 20 quads at 18°, the top straight above the foot at `height`.
fn upright_band(at: Vector, radius: f32, turn: f32, height: f32, alpha: f32) -> Vec<MeshVertex> {
    let tint = [PINK[0], PINK[1], PINK[2], alpha / 255.0];
    let corner = |s: u32, top: bool, uv: [f32; 2]| {
        let (sin, cos) = (turn + 18.0 * s as f32).to_radians().sin_cos();
        MeshVertex { position: [at[0] + radius * cos, at[1] + if top { height } else { 0.0 }, at[2] + radius * sin], uv, tint }
    };
    ribbon(&corner)
}

/// 20 quads between points s and s + 1: the foot, then the top, with u = j / 21 across and v 1 at the foot and 0 at the top.
fn ribbon(corner: &impl Fn(u32, bool, [f32; 2]) -> MeshVertex) -> Vec<MeshVertex> {
    let mut vertices = Vec::with_capacity(20 * 6);
    for j in 0..20 {
        let (u0, u1) = (j as f32 / 21.0, (j + 1) as f32 / 21.0);
        let [a, b, c, d] = [corner(j, false, [u0, 1.0]), corner(j + 1, false, [u1, 1.0]), corner(j + 1, true, [u1, 0.0]), corner(j, true, [u0, 0.0])];
        vertices.extend([a, b, c, a, c, d]);
    }
    vertices
}

impl Portal {
    /// Resurrection's rings over the target's feet `at`.
    pub fn revive(&mut self, at: Vector, now: Instant) {
        self.revives.push(Revive { at, start: now, ticks: 0, rings: Vec::new() });
    }

    /// The warp of skill unit `id` over the cell `at`, its cylinders' start turns from `rand(n)`.
    pub fn keep(&mut self, id: u32, at: Vector, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        self.warps.push(Warp::new(id, at, now, &mut rand));
    }

    pub fn forget(&mut self, id: u32) {
        self.warps.retain(|warp| warp.id != id);
    }

    /// Runs the rings and the flat rings up to `now`, a tick at a time, starts the portal's next copy and drops what ended.
    pub fn advance(&mut self, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        for revive in &mut self.revives {
            while revive.ticks < passes(revive.start, now).min(REVIVE_LENGTH) {
                let e = revive.ticks;
                if e.is_multiple_of(REVIVE_EVERY) {
                    revive.rings.push(Ring::new(e));
                }
                for ring in &mut revive.rings {
                    ring.step();
                }
                revive.ticks += 1;
            }
        }
        self.revives.retain(|revive| passes(revive.start, now) <= REVIVE_LENGTH);
        for warp in &mut self.warps {
            while passes(warp.start, now) > PORTAL_PERIOD {
                let (id, at, start) = (warp.id, warp.at, warp.start + super::TICK * PORTAL_PERIOD);
                *warp = Warp::new(id, at, start, &mut rand);
            }
            // The flat rings' prim is made at tick 1: its ticks count from there.
            while warp.done < passes(warp.start, now) {
                warp.done += 1;
                if warp.done >= 2 {
                    warp.flat_ticks += 1;
                    for flat in &mut warp.flats {
                        flat.step(warp.flat_ticks);
                    }
                }
            }
        }
    }

    /// The meshes at `now`: the golden rings, white and blended; the warp's three prims, pink and added on.
    pub fn meshes(&self, now: Instant) -> Vec<Wall> {
        let mut walls = Vec::new();
        for revive in &self.revives {
            for ring in revive.rings.iter().filter(|ring| ring.alpha > 0.0) {
                walls.push(Wall { texture: RING_YELLOW, vertices: ring.mesh(revive.at), additive: false, forced: false });
            }
        }
        for warp in &self.warps {
            let e = passes(warp.start, now).saturating_sub(1);
            let n = e + 1;
            let at = warp.at;
            let alpha = cylinder_alpha(n);
            let mut cylinders = Vec::new();
            if alpha > 0.0 {
                for cylinder in &warp.cylinders {
                    let turn = cylinder.turn + cylinder.step * n as f32;
                    cylinders.extend(upright_band(at, cylinder.radius, turn, cylinder_height(n), alpha));
                }
            }
            walls.push(Wall { texture: RING_RED, vertices: cylinders, additive: true, forced: false });
            let mut flats = Vec::new();
            for flat in warp.flats.iter().filter(|flat| flat.alpha > 0.0 && !flat.ended) {
                let tint = [PINK[0], PINK[1], PINK[2], flat.alpha / 255.0];
                let (lean_sin, lean_cos) = flat.lean.to_radians().sin_cos();
                let rim = flat.profile();
                let corner = |s: u32, top: bool, uv: [f32; 2]| {
                    let (sin, cos) = (18.0 * s as f32).to_radians().sin_cos();
                    let (reach, up) = if top { (flat.radius + rim * lean_cos, 2.0 + rim * lean_sin) } else { (flat.radius, 2.0) };
                    MeshVertex { position: [at[0] + reach * cos, at[1] + up, at[2] + reach * sin], uv, tint }
                };
                flats.extend(ribbon(&corner));
            }
            walls.push(Wall { texture: RING_RED, vertices: flats, additive: true, forced: false });
            let mut arcs = Vec::new();
            let alpha = arc_alpha(n.saturating_sub(2));
            if alpha > 0.0 {
                let age = n.saturating_sub(2);
                let step = (arc_span(age) / 21.0).floor();
                for arc in &warp.arcs {
                    let turn = arc.turn + 5.0 * age as f32;
                    let tint = [PINK[0], PINK[1], PINK[2], alpha / 255.0];
                    let corner = |s: u32, top: bool, uv: [f32; 2]| {
                        let (sin, cos) = (turn + step * s as f32).to_radians().sin_cos();
                        MeshVertex {
                            position: [at[0] + arc.radius * cos, at[1] + arc.height + if top { 1.5 } else { 0.0 }, at[2] + arc.radius * sin],
                            uv,
                            tint,
                        }
                    };
                    arcs.extend(ribbon(&corner));
                }
            }
            walls.push(Wall { texture: CLOUD, vertices: arcs, additive: true, forced: false });
        }
        walls.retain(|wall| !wall.vertices.is_empty());
        walls
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prims::TICK;

    fn near(got: f32, want: f32, tolerance: f32) -> bool {
        (got - want).abs() < tolerance
    }

    /// A ring made at tick `e`, run for `ticks` ticks; it shows the state after the tick of age `ticks - 1`.
    fn ring(e: u32, ticks: u32) -> Ring {
        let mut ring = Ring::new(e);
        for _ in 0..ticks {
            ring.step();
        }
        ring
    }

    #[test]
    fn the_golden_rings_narrow_turn_and_fade() {
        // The e = 25 … 175 rings after the tick of age a = 0, 10, 20, 30, 40, 50, 59: inner / outer radius.
        let radii = [0, 10, 20, 30, 40, 50, 59].map(|a| {
            let ring = ring(25, a + 1);
            ((ring.inner * 100.0).round() / 100.0, (ring.outer * 100.0).round() / 100.0)
        });
        let want = [(7.38, 10.38), (6.38, 9.38), (5.67, 8.67), (5.27, 8.27), (5.16, 5.77), (5.36, 3.56), (5.79, 1.84)];
        for (got, want) in radii.iter().zip(want) {
            assert!(near(got.0, want.0, 0.015) && near(got.1, want.1, 0.015), "{got:?} against {want:?}");
        }
        // Alpha 240 up to a = 29, then 232 at 30, 152 at 40, 72 at 50, 0 at 59; yaw 3.1° at a = 0, 37° at 10, 123° at 30.
        assert_eq!([0, 29, 30, 40, 50, 59].map(|a| ring(25, a + 1).alpha.round()), [240.0, 240.0, 232.0, 152.0, 72.0, 0.0]);
        assert!(near(ring(25, 1).yaw, 3.06, 0.01) && near(ring(25, 11).yaw, 36.96, 0.01) && near(ring(25, 31).yaw, 122.76, 0.01));
        // The first ring stays 7.5 / 11.0 at 240 turning 3° a tick until a = 225, then its rim closes and its alpha falls 9.6 a tick.
        let steady = ring(0, 200);
        assert_eq!((steady.inner, steady.outer, steady.alpha, steady.yaw), (7.5, 11.0, 240.0, 600.0));
        let falling = ring(0, 226);
        assert!(near(falling.alpha, 230.4, 0.01));
        let last = ring(0, 249);
        assert!(last.alpha > 0.0 && near(last.outer, 1.27, 0.05));
        assert!(near(ring(0, 250).alpha, 0.0, 0.01), "drawn 249 times");
        // The late rings fade from their first tick: 240 / 49 = 4.9 for e = 200 and 10 for e = 225, gone after 48 and 23 draws.
        assert!(near(ring(200, 1).alpha, 240.0 - 240.0 / 49.0, 0.01) && near(ring(225, 1).alpha, 230.0, 0.01));
        assert!(ring(200, 48).alpha > 0.0 && ring(200, 49).alpha < 0.01);
        assert!(ring(225, 23).alpha > 0.0 && ring(225, 24).alpha < 0.01);
    }

    #[test]
    fn ten_rings_are_made_every_25_ticks_and_nothing_after() {
        let start = Instant::now();
        let at = [10.0, 1.0, 20.0];
        let mut portal = Portal::default();
        portal.revive(at, start);
        for tick in 0..=260 {
            portal.advance(start + TICK * tick, |_| 0);
            if tick == 0 {
                assert_eq!(portal.revives[0].rings.len(), 1);
            }
        }
        assert!(portal.revives.is_empty(), "gone with the effect at 250");
        let mut again = Portal::default();
        again.revive(at, start);
        again.advance(start + TICK * 249, |_| 0);
        assert_eq!(again.revives[0].rings.len(), 10);
        let mut again = Portal::default();
        again.revive(at, start);
        // Ten sides a ring, two triangles each, white × alpha, u = 0, .25, .5, .75, 0 …; the inner edge at the feet, the outer 7.5 up.
        again.advance(start + TICK * 100, |_| 0);
        let walls = again.meshes(start + TICK * 100);
        assert!(walls.iter().all(|wall| wall.texture == RING_YELLOW && !wall.additive && wall.vertices.len() == 60));
        let first = &walls[0].vertices;
        assert_eq!([0, 6, 12, 18, 24].map(|index| first[index].uv[0]), [0.0, 0.25, 0.5, 0.75, 0.0]);
        assert!(near(first[0].position[1], 1.0, 1e-4) && near(first[1].position[1], 8.5, 1e-4) && first[0].uv[1] == 1.0 && first[1].uv[1] == 0.0);
        assert!(near((first[0].position[0] - at[0]).hypot(first[0].position[2] - at[2]), 7.5, 1e-3));
    }

    #[test]
    fn the_cylinders_and_arcs_rise_hold_and_fade() {
        // Cylinder alpha at n = 1, 15, 16, 1400, 1401, 1438: 5, 75, 75, 75, 73, 0; the height 50 sin 1° at 1, then 50 from 90.
        assert_eq!([1, 15, 16, 1400, 1401, 1438].map(cylinder_alpha), [5.0, 75.0, 75.0, 75.0, 73.0, 0.0]);
        assert!(near(cylinder_height(1), 50.0 * 1.0_f32.to_radians().sin(), 1e-4) && cylinder_height(90) > 49.99 && cylinder_height(91) == 50.0);
        // Arc alpha at n = 11, 12, 1400, 1401, 1437: 110, 110, 110, 107, 0; the span 30 → 120 at n = 30.
        assert_eq!([11, 12, 1400, 1401, 1437].map(arc_alpha), [110.0, 110.0, 110.0, 107.0, 0.0]);
        assert_eq!([0, 29, 30, 31].map(arc_span), [30.0, 117.0, 120.0, 120.0]);
    }

    #[test]
    fn the_flat_rings_close_in_on_the_centre_in_a_63_tick_cycle() {
        let mut band = Flat::new(0, 2.0);
        let mut at = Vec::new();
        for _ in 0..=64 {
            band.step(1);
            at.push((band.age, band.radius, band.alpha));
        }
        // The first tick is the restart (n = 0); then n = 1, 19, 28, 29, 62, and the restart again at 63.
        let state = |n: usize| at[n];
        assert_eq!(state(0), (0, 14.0, 12.0));
        assert_eq!((state(1).1, state(1).2), (13.75, 24.0));
        assert_eq!((state(19).1, state(19).2), (9.25, 240.0));
        assert_eq!((state(28).1, state(28).2), (7.0, 240.0));
        assert_eq!((state(29).1, state(29).2), (6.75, 233.0));
        assert_eq!((state(62).1, state(62).2), (0.0, 2.0));
        assert_eq!(state(63), (0, 14.0, 12.0), "the restart at n = 63");
        // The rim: 0.94 at age 1, 6.0 from age 11.
        let mut rim = Flat::new(1, 2.0);
        assert!(near(rim.profile(), 6.001 * 9.0_f32.to_radians().sin(), 1e-4) && near(rim.profile(), 0.94, 0.01));
        rim.age = 11;
        assert_eq!(rim.profile(), 6.0);
        // Past tick 1 400 the band ends at its next restart, for good.
        let mut old = Flat::new(0, 2.0);
        for _ in 0..70 {
            old.step(1401);
        }
        assert!(old.ended);
        // Bands 1 and 2 wait 20 and 40 ticks.
        let mut late = Flat::new(-20, 3.0);
        for _ in 0..20 {
            late.step(1);
        }
        assert!(late.alpha == 0.0 && late.age == 0);
        late.step(1);
        assert_eq!((late.age, late.alpha), (0, 12.0));
    }

    #[test]
    fn the_warp_is_three_pink_added_on_prims_that_start_again_at_9991() {
        let start = Instant::now();
        let at = [10.0, 1.0, 20.0];
        let mut portal = Portal::default();
        let mut draws = Vec::new();
        portal.keep(9, at, start, |n| {
            draws.push(n);
            0
        });
        assert_eq!(draws, [360, 360, 360]);
        assert_eq!([portal.warps[0].cylinders[0].radius, portal.warps[0].cylinders[1].radius, portal.warps[0].cylinders[2].radius], [4.0, 3.0, 2.0]);
        assert_eq!([0, 1, 2].map(|k| portal.warps[0].cylinders[k].step), [8.0, 9.0, 10.0]);
        assert_eq!([0, 1, 2].map(|k| portal.warps[0].flats[k].age), [0, -20, -40]);
        assert_eq!(portal.warps[0].arcs.iter().map(|arc| arc.radius).collect::<Vec<_>>(), [9.0, 7.5, 6.0, 4.5]);
        // The cylinders show from tick 0, the flat rings from tick 1 on (alpha 12 after the first), the arcs from tick 2.
        let textures = |tick: u32, portal: &mut Portal| {
            portal.advance(start + TICK * tick, |_| 0);
            portal.meshes(start + TICK * tick).iter().map(|wall| wall.texture).collect::<Vec<_>>()
        };
        assert_eq!(textures(0, &mut portal), [RING_RED]);
        assert_eq!(textures(1, &mut portal), [RING_RED, RING_RED]);
        assert_eq!(textures(2, &mut portal), [RING_RED, RING_RED, CLOUD]);
        assert_eq!(textures(3, &mut portal), [RING_RED, RING_RED, CLOUD]);
        let walls = portal.meshes(start + TICK * 3);
        assert!(walls.iter().all(|wall| wall.additive && wall.vertices.iter().all(|vertex| vertex.tint[..3] == PINK)));
        // All faded by about tick 1 450; a copy again at the 9991st tick, and it goes with its unit.
        portal.advance(start + TICK * 1500, |_| 0);
        assert!(portal.meshes(start + TICK * 1500).is_empty());
        portal.advance(start + TICK * 9991, |_| 0);
        assert_eq!(portal.warps[0].done, 1, "a fresh copy");
        portal.forget(9);
        assert!(portal.warps.is_empty());
    }

    #[test]
    fn the_flat_rings_end_at_the_issues_ticks() {
        let mut bands = [Flat::new(0, 2.0), Flat::new(-20, 3.0), Flat::new(-40, 4.0)];
        let mut ended = [None; 3];
        for tick in 1..=1500 {
            for (k, band) in bands.iter_mut().enumerate() {
                let was = band.ended;
                band.step(tick);
                if band.ended && !was {
                    ended[k] = Some(tick);
                }
            }
        }
        // The prim counts its ticks from 1 on its first; the issue counts from 0.
        assert_eq!(ended.map(|tick| tick.map(|tick| tick - 1)), [Some(1449), Some(1406), Some(1426)]);
    }
}
