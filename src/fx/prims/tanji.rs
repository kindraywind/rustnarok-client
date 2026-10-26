//! The spirit sphere (the game's prim kind 42 in mode 1) of Finger Offensive's and Duple Light's effect 265, and of 412
//! with its rings (416, kind 98): a square facing the camera, three ghosts behind it, waiting 15 ticks, flying out,
//! bouncing off the target and fading (265); flying straight on, dropping a ring every 4 ticks (412).

use std::time::Instant;

use super::{TICK, passes};
use crate::world::{EffectMesh, EffectQuad, MeshVertex, Vector};

pub const SPHERE: &str = "data\\texture\\effect\\blue_ivy.bmp";
pub const RING: &str = "data\\texture\\effect\\emp shock.tga";
/// The effect's length in ticks; the sphere waits this many ticks before it flies.
const LENGTH: u32 = 300;
const WAIT: u32 = 15;
/// How high over the caster's feet it starts, and its speed out and after the bounce.
const RISE: f32 = 8.0;
const SPEED: f32 = 2.0;
const BOUNCE_SPEED: f32 = 6.0;
/// Its size by hit count: 2.0 + 0.4 n.
const SIZE_FIRST: f32 = 2.0;
const SIZE_EACH: f32 = 0.4;
/// The sphere's alpha at the start, the ramp a tick for 10 ticks, the fade a tick after the bounce, and the ghosts'
/// alphas: the first copy's is the sphere's less 150, each later copy's 25 less.
const ALPHA_START: f32 = 5.0;
const RAMP: f32 = 25.0;
const FADE: f32 = 12.0;
const COPY_BELOW: [f32; 2] = [150.0, 25.0];
const SPIN: f32 = -15.0;
/// On ecl_fild01 every alpha over 100 is cut to 100.
const DIM_ALPHA: f32 = 100.0;
/// A world unit is 7 sprite pixels (as Jupitel Thunder's ball).
const PIXELS: f32 = 35.0 / crate::world::CELL;
/// 412's rings: one every 4th counter up to 284; a ring lives 100 ticks.
const RING_EVERY: u32 = 4;
const RING_LIFE: u32 = 100;

/// The bearing A(dx, dz) in whole degrees (x = cos h, z = sin h; 0 = +x, 90 = +z): the first whole degree from 0 whose
/// cosine is no more than the direction's x (180 if none, also for a zero length), then 360 less that when dz < 0. So
/// straight along +z is 91.
pub fn bearing(dx: f32, dz: f32) -> i32 {
    let length = f64::hypot(f64::from(dx), f64::from(dz));
    if length == 0.0 {
        return 180;
    }
    let x = f64::from(dx) / length;
    let degrees = (0..=180).find(|&degrees| f64::from(degrees).to_radians().cos() <= x).unwrap_or(180);
    if dz < 0.0 { 360 - degrees } else { degrees }
}

/// The hit count the game makes of a packet's `count`: 0 or over 36 is 1, and only damage types 8, 9 and 13 keep it;
/// every other type is 1.
pub fn hit_count(count: u16, kind: u8) -> u16 {
    if !matches!(kind, 8 | 9 | 13) || count == 0 || count > 36 { 1 } else { count }
}

/// One square: its place, spin (degrees), and alpha (0 to 255).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Element {
    at: Vector,
    spin: f32,
    alpha: f32,
}

/// What a sphere does.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Mode {
    /// 265: out at 2 a tick, bounces off the target at 6, fades.
    Bounce,
    /// 412: straight on at 2 a tick to the effect's end, with a ring every 4 ticks.
    Straight,
}

/// A 416 ring.
struct Ring {
    start: Instant,
    at: Vector,
    /// Its facing, set once: the bearing from the ring to the local player, plus 90.
    facing: i32,
}

pub struct Sphere {
    start: Instant,
    mode: Mode,
    /// Its size (the radius its corners lie on, world units).
    size: f32,
    aim: [f32; 2],
    /// Across the ground from the start to the aim, and the climb a tick toward the aim's height.
    reach: f32,
    climb: f32,
    /// Ticks run, the heading (whole degrees) and the speed.
    ticks: u32,
    heading: i32,
    speed: f32,
    bounced: bool,
    dim: bool,
    /// The sphere, then its three ghosts.
    squares: [Element; 4],
    rings: Vec<Ring>,
}

impl Sphere {
    /// A sphere from the caster's feet `from` at the aim `to` (feet), for hit count `hits`; `dim` on ecl_fild01.
    pub fn new(mode_straight: bool, from: Vector, to: Vector, hits: u16, now: Instant, dim: bool) -> Self {
        let reach = f32::hypot(to[0] - from[0], to[2] - from[2]);
        // ponytail: a target under the caster has no finite climb; there is none.
        let climb = if reach > 0.0 { SPEED * (to[1] - from[1]) / reach } else { 0.0 };
        let at = [from[0], from[1] + RISE, from[2]];
        let first = Element { at, spin: 0.0, alpha: ALPHA_START };
        let ghost = Element { alpha: 0.0, ..first };
        Sphere {
            start: now,
            mode: if mode_straight { Mode::Straight } else { Mode::Bounce },
            size: SIZE_FIRST + SIZE_EACH * f32::from(hits),
            aim: [to[0], to[2]],
            reach,
            climb,
            ticks: 0,
            heading: 0,
            speed: SPEED,
            bounced: false,
            dim,
            squares: [first, ghost, ghost, ghost],
            rings: Vec::new(),
        }
    }

    /// The tick (c = k − 15) the sphere bounces on: the first with twice c over the distance to the aim.
    #[cfg(test)]
    fn bounce_counter(&self) -> u32 {
        (self.reach / SPEED) as u32 + 1
    }

    /// One tick of the sphere, `player` the local player's feet for 412's rings; `rand(n)` makes the bounce's choices.
    fn step(&mut self, player: Option<Vector>, mut rand: impl FnMut(u32) -> u32) {
        self.ticks += 1;
        if self.ticks <= WAIT {
            return self.cut();
        }
        let c = self.ticks - WAIT;
        let [e0, e1, e2, e3] = self.squares;
        let mut e0 = e0;
        if c <= 10 {
            e0.alpha = (e0.alpha + RAMP).min(255.0);
        }
        // The copies, oldest first, from the values before this step (e0 with this tick's ramp).
        let copy = |from: Element, below: f32| Element { alpha: (from.alpha - below).max(0.0), ..from };
        let ghosts = [
            if c > 1 { copy(e0, COPY_BELOW[0]) } else { e1 },
            if c > 2 { copy(e1, COPY_BELOW[1]) } else { e2 },
            if c > 3 { copy(e2, COPY_BELOW[1]) } else { e3 },
        ];
        e0.spin = (e0.spin + SPIN).rem_euclid(360.0);
        if c == 1 {
            self.heading = bearing(self.aim[0] - e0.at[0], self.aim[1] - e0.at[2]);
        }
        if self.mode == Mode::Bounce {
            if !self.bounced && SPEED * c as f32 > self.reach {
                self.bounced = true;
                self.speed = BOUNCE_SPEED;
                // R(2) first: 0 turns it one way, else the other; then R(45) degrees.
                let side = rand(2);
                let turn = rand(45) as i32;
                self.heading = (self.heading + if side == 0 { turn } else { -turn }).rem_euclid(360);
                e0.at[1] += self.climb;
            } else if self.bounced {
                e0.alpha = (e0.alpha - FADE).max(0.0);
                e0.at[1] -= self.climb;
            } else {
                e0.at[1] += self.climb;
            }
        }
        let (sin, cos) = f64::from(self.heading).to_radians().sin_cos();
        e0.at[0] += self.speed * cos as f32;
        e0.at[2] += self.speed * sin as f32;
        self.squares = [e0, ghosts[0], ghosts[1], ghosts[2]];
        // 412: a ring every 4th counter after that tick's move, at the sphere's place, facing the local player.
        if self.mode == Mode::Straight
            && c.is_multiple_of(RING_EVERY)
            && let Some(player) = player
        {
            let at = self.squares[0].at;
            let facing = (bearing(at[0] - player[0], at[2] - player[2]) + 90).rem_euclid(360);
            self.rings.push(Ring { start: self.start + TICK * (self.ticks - 1), at, facing });
        }
        self.cut();
    }

    /// On ecl_fild01 every alpha over 100 is cut to 100, and stays cut.
    fn cut(&mut self) {
        if self.dim {
            for square in &mut self.squares {
                square.alpha = square.alpha.min(DIM_ALPHA);
            }
        }
    }

    /// The ring `ring` at `now`: its size, alpha (0 to 255), and tick; None once it's gone.
    fn ring_state(ring: &Ring, now: Instant) -> Option<(f32, f32)> {
        let tick = passes(ring.start, now);
        if tick == 0 || tick >= RING_LIFE {
            return None;
        }
        let size = (1..=tick.min(49)).fold(5.0_f32, |size, _| size + 0.1);
        let alpha = if tick <= 5 { 50.0 * tick as f32 } else { (250.0 - 3.0 * (tick - 5) as f32).max(0.0) };
        Some((size, alpha))
    }
}

/// The spheres in flight.
#[derive(Default)]
pub struct Spheres {
    list: Vec<Sphere>,
}

impl Spheres {
    pub fn throw(&mut self, sphere: Sphere) {
        self.list.push(sphere);
    }

    /// Runs the spheres up to `now`, a tick at a time; `player` is the local player's feet, `rand(n)` (0 to n − 1).
    pub fn advance(&mut self, now: Instant, player: Option<Vector>, mut rand: impl FnMut(u32) -> u32) {
        for sphere in &mut self.list {
            while sphere.ticks < passes(sphere.start, now).min(LENGTH - 1) {
                sphere.step(player, &mut rand);
            }
        }
        self.list.retain(|sphere| passes(sphere.start, now) <= LENGTH || sphere.rings.iter().any(|ring| Sphere::ring_state(ring, now).is_some()));
    }

    /// The squares to draw, added on: each sphere's four with alpha, the sphere first.
    pub fn quads<'a>(&self, texture: Option<&'a crate::render::Texture>) -> Vec<EffectQuad<'a>> {
        let Some(texture) = texture else { return Vec::new() };
        let mut quads = Vec::new();
        for sphere in self.list.iter().filter(|sphere| sphere.ticks > 0 && sphere.ticks < LENGTH) {
            for square in sphere.squares.iter().filter(|square| square.alpha > 0.0) {
                // Corner j at the spin + 90 j, r·cos s right and r·sin s down (so −15° turns counter-clockwise); its
                // texture coordinates (0, 1), (1, 1), (1, 0), (0, 0) land on the quad's corners 3, 2, 1, 0.
                // ponytail: the sense of the spin is worked out, not seen.
                let corner = |j: u32| {
                    let (sin, cos) = (square.spin + 90.0 * j as f32).to_radians().sin_cos();
                    [sphere.size * PIXELS * cos, sphere.size * PIXELS * sin]
                };
                let corners = [corner(3), corner(2), corner(1), corner(0)];
                quads.push((texture, square.at, corners, [150.0 / 255.0, 150.0 / 255.0, 250.0 / 255.0, square.alpha / 255.0], true, false));
            }
        }
        quads
    }

    /// The rings' diamonds at `now`: a vertical square facing the local player, blue, added on.
    pub fn rings<'a>(&self, texture: Option<&'a crate::render::Texture>, now: Instant) -> Vec<EffectMesh<'a>> {
        let Some(texture) = texture else { return Vec::new() };
        self.list
            .iter()
            .flat_map(|sphere| sphere.rings.iter())
            .filter_map(|ring| {
                let (size, alpha) = Sphere::ring_state(ring, now)?;
                if alpha <= 0.0 {
                    return None;
                }
                let (sin, cos) = f64::from(ring.facing).to_radians().sin_cos();
                let across = [cos as f32, 0.0, sin as f32];
                let at = ring.at;
                let corner = |x: f32, y: f32| -> Vector { [at[0] + across[0] * x, at[1] + y, at[2] + across[2] * x] };
                let tint = [60.0 / 255.0, 60.0 / 255.0, 1.0, alpha / 255.0];
                let vertex = |position: Vector, uv: [f32; 2]| MeshVertex { position, uv, tint };
                let [c0, c1, c2, c3] = [
                    vertex(corner(0.0, -size), [0.0, 1.0]),
                    vertex(corner(-size, 0.0), [1.0, 1.0]),
                    vertex(corner(0.0, size), [1.0, 0.0]),
                    vertex(corner(size, 0.0), [0.0, 0.0]),
                ];
                Some((texture, vec![c0, c1, c2, c0, c2, c3], true))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sphere from the origin at the aim `d0` along +x (level), run `ticks` ticks with every draw 0 (no turn).
    fn fly(d0: f32, ticks: u32, hits: u16) -> Sphere {
        let mut sphere = Sphere::new(false, [0.0; 3], [d0, 0.0, 0.0], hits, Instant::now(), false);
        for _ in 0..ticks {
            sphere.step(None, |_| 0);
        }
        sphere
    }

    #[test]
    fn counts_hits_as_the_game_does() {
        assert_eq!([hit_count(0, 8), hit_count(37, 8), hit_count(36, 8), hit_count(5, 9), hit_count(5, 13)], [1, 1, 36, 5, 5]);
        assert_eq!([hit_count(5, 4), hit_count(36, 6)], [1, 1]);
        // Straight along +z is 91, a zero length 180, along −z 269.
        assert_eq!([bearing(0.0, 1.0), bearing(0.0, 0.0), bearing(0.0, -1.0), bearing(1.0, 0.0)], [91, 180, 269, 0]);
    }

    #[test]
    fn a_sphere_is_sized_by_its_hits() {
        let size = |hits: u16| Sphere::new(false, [0.0; 3], [10.0, 0.0, 0.0], hits, Instant::now(), false).size;
        let near = |got: f32, want: f32| (got - want).abs() < 1e-4;
        assert!([1, 3, 5, 36].iter().zip([2.4, 3.2, 4.0, 16.4]).all(|(&hits, want)| near(size(hits), want)));
        assert!(near(size(0), 2.0));
    }

    #[test]
    fn waits_flies_bounces_and_fades() {
        // 15 ticks waiting, alpha 5 at the start; the heading is set at tick 16.
        let waiting = fly(20.0, 15, 1);
        assert_eq!((waiting.squares[0].alpha, waiting.squares[0].at), (5.0, [0.0, 8.0, 0.0]));
        assert_eq!(fly(20.0, 16, 1).heading, 0);
        // Bounce counter N = ⌊d0/2⌋ + 1 and the last visible tick N + 36, by d0.
        for (d0, n, last) in [(5.0, 3, 39), (10.0, 6, 42), (15.0, 8, 44), (20.0, 11, 47), (25.0, 13, 49), (45.0, 23, 59)] {
            let bounce = fly(d0, 15 + n, 1);
            assert_eq!((bounce.bounce_counter(), bounce.bounced, bounce.speed), (n, true, 6.0), "d0 {d0}");
            // Drawn at 2(N − 1) + 6 from the start on the bounce tick.
            assert!((bounce.squares[0].at[0] - (2.0 * (n - 1) as f32 + 6.0)).abs() < 1e-3, "d0 {d0}: {}", bounce.squares[0].at[0]);
            assert!(fly(d0, last, 1).squares[0].alpha > 0.0, "visible at {last} for d0 {d0}");
            assert_eq!(fly(d0, last + 1, 1).squares[0].alpha, 0.0, "gone after {last} for d0 {d0}");
        }
        // The alpha at the bounce: 80, 155, 205, 255.
        assert_eq!([5.0, 10.0, 15.0, 20.0].map(|d0| fly(d0, 15 + (d0 / 2.0) as u32 + 1, 1).squares[0].alpha), [80.0, 155.0, 205.0, 255.0]);
        // d0 = 20: the alphas (sphere, ghosts) by c.
        let by_c = |c: u32| fly(20.0, 15 + c, 1).squares.map(|square| square.alpha);
        assert_eq!(
            [1, 6, 8, 10, 11, 12, 13, 20, 21].map(by_c),
            [
                [30.0, 0.0, 0.0, 0.0],
                [155.0, 5.0, 0.0, 0.0],
                [205.0, 55.0, 5.0, 0.0],
                [255.0, 105.0, 55.0, 5.0],
                [255.0, 105.0, 80.0, 30.0],
                [243.0, 105.0, 80.0, 55.0],
                [231.0, 93.0, 80.0, 55.0],
                [147.0, 9.0, 0.0, 0.0],
                [135.0, 0.0, 0.0, 0.0]
            ]
        );
        // The target 10 higher at d0 = 25: the height 8 → 18.4 at the bounce (tick 28) → 1.6 at tick 49.
        let high = |ticks: u32| {
            let mut sphere = Sphere::new(false, [0.0; 3], [25.0, 10.0, 0.0], 1, Instant::now(), false);
            (0..ticks).for_each(|_| sphere.step(None, |_| 0));
            sphere.squares[0].at[1]
        };
        assert!((high(28) - 18.4).abs() < 1e-3 && (high(49) - 1.6).abs() < 1e-3, "{} {}", high(28), high(49));
        // The bounce turns the heading by R(45) one way or the other, by R(2): 0 + , else −.
        let turned = |side: u32| {
            let mut sphere = Sphere::new(false, [0.0; 3], [20.0, 0.0, 0.0], 1, Instant::now(), false);
            (0..26).for_each(|_| sphere.step(None, |n| if n == 45 { 30 } else { side }));
            sphere.heading
        };
        assert_eq!([turned(0), turned(1)], [30, 330]);
        // A target under the caster: heading 180, bounces at c = 1, no climb.
        let under = {
            let mut sphere = Sphere::new(false, [0.0; 3], [0.0, 5.0, 0.0], 1, Instant::now(), false);
            (0..16).for_each(|_| sphere.step(None, |_| 0));
            sphere
        };
        assert_eq!((under.heading, under.bounced, under.climb), (180, true, 0.0));
    }

    #[test]
    fn has_no_ghosts_on_ecl_fild01() {
        let mut sphere = Sphere::new(false, [0.0; 3], [40.0, 0.0, 0.0], 1, Instant::now(), true);
        let mut alphas = Vec::new();
        for _ in 0..(15 + 21 + 16) {
            sphere.step(None, |_| 0);
            assert!(sphere.squares[1..].iter().all(|ghost| ghost.alpha == 0.0));
            alphas.push(sphere.squares[0].alpha);
        }
        // 100 at the bounce draw (c = 21), then 88, 76 … 28, 16, 4, 0.
        assert_eq!(alphas[15 + 20], 100.0);
        assert_eq!(alphas[15 + 21..15 + 21 + 8], [88.0, 76.0, 64.0, 52.0, 40.0, 28.0, 16.0, 4.0]);
        assert_eq!(alphas[15 + 21 + 8], 0.0);
    }

    #[test]
    fn a_412_sphere_flies_straight_on_dropping_rings() {
        let start = Instant::now();
        let mut sphere = Sphere::new(true, [0.0; 3], [0.0, 0.0, 30.0], 0, start, false);
        assert_eq!(sphere.size, 2.0);
        // Ghosts as the 265's (105, 80, 55) with no fade; no bounce; level; 2 a tick on.
        for _ in 0..(15 + 12) {
            sphere.step(Some([100.0, 0.0, 0.0]), |_| 0);
        }
        assert_eq!(sphere.heading, 91);
        assert_eq!(sphere.squares.map(|square| square.alpha), [255.0, 105.0, 80.0, 55.0]);
        assert!(!sphere.bounced && sphere.squares[0].at[1] == 8.0);
        // A ring on every 4th counter: 71 by the effect's end (tick 299), 284 moves of 2.
        for _ in (15 + 12)..299 {
            sphere.step(Some([100.0, 0.0, 0.0]), |_| 0);
        }
        assert_eq!(sphere.rings.len(), 71);
        assert_eq!(sphere.ticks, 299);
        assert!((sphere.squares[0].at[2] - 284.0 * 2.0 * 91.0_f32.to_radians().sin()).abs() < 0.01);
        // A ring faces the player as the bearing from the ring plus 90, set once.
        let first = &sphere.rings[0];
        assert_eq!(first.facing, (bearing(first.at[0] - 100.0, first.at[2]) + 90).rem_euclid(360));
    }

    #[test]
    fn a_ring_grows_and_fades_for_100_ticks() {
        let start = Instant::now();
        let ring = Ring { start, at: [0.0; 3], facing: 0 };
        let state = |tick: u32| Sphere::ring_state(&ring, start + TICK * (tick - 1));
        let near = |got: Option<(f32, f32)>, want: (f32, f32)| got.is_some_and(|(size, alpha)| (size - want.0).abs() < 1e-3 && alpha == want.1);
        assert!(near(state(1), (5.1, 50.0)) && near(state(5), (5.5, 250.0)) && near(state(6), (5.6, 247.0)));
        assert!(near(state(49), (9.9, 250.0 - 3.0 * 44.0)) && near(state(88), (9.9, 1.0)) && near(state(89), (9.9, 0.0)));
        assert!(state(99).is_some() && state(100).is_none());
    }
}
