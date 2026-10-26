//! Blitz Beat's effect on its target (EF_BLITZBEAT 115), as the game builds it a 16 ms tick at a time, following the
//! target: ten streaks sweeping back through it at tick 0, and a green funnel ring that opens at tick 36 (and at every
//! third tick after it for as long as a shared counter lets it).

use std::time::Instant;

use super::{TICK, passes};
use crate::world::Vector;

pub const STREAK: &str = "data\\texture\\effect\\ac_center2.tga";
pub const RING: &str = "data\\texture\\effect\\magic_green.tga";
/// The effect's length in ticks.
const LENGTH: u32 = 250;
const STREAKS: usize = 10;
/// A streak lives 20 ticks, a ring 12.
const STREAK_LIFE: u32 = 20;
const RING_LIFE: u32 = 12;
/// Rings come on the ticks above 35 that are multiples of 3.
const RING_AFTER: u32 = 35;
const RING_EVERY: u32 = 3;
/// A streak: where it starts out round the feet and how high; how far along its axis (15, sweeping through the target);
/// its half-width.
const STREAK_OUT: f32 = 3.5;
const STREAK_RISE: f32 = 7.0;
const STREAK_START: f32 = 15.0;
const STREAK_WIDTH: f32 = 0.2;
/// The streaks' alpha on drawn ticks 0, 1…17, 18, 19 (of 255).
const STREAK_ALPHAS: [f32; 4] = [125.0, 250.0, 125.0, 0.0];
/// The ring's alpha on its drawn ticks (of 255), and its inner and outer radius and the funnel's length.
const RING_ALPHAS: [f32; 12] = [254.0, 254.0, 254.0, 254.0, 254.0, 254.0, 211.0, 169.0, 127.0, 84.0, 42.0, 0.0];
const RING_RADII: [f32; 2] = [0.5, 1.5];
const RING_FUNNEL: f32 = 3.5;
const RING_SIDES: usize = 10;
/// How far over the feet the ring starts, before its own rise.
const RING_LOWER: f32 = 0.965;

/// A corner: where (world units, from the feet) and its texture coordinate.
pub type Corner = (Vector, [f32; 2]);

#[derive(Clone, Copy, Debug, PartialEq)]
struct Streak {
    /// Its start round the feet.
    around: [f32; 2],
    /// Its half-length at tick 0.
    length: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Ring {
    /// The effect's tick it opened on, and where it starts from the feet before its flight along the axis.
    born: u32,
    start: Vector,
}

/// One 115 on a unit (None = the player).
pub struct Blitz {
    start: Instant,
    pub unit: Option<u32>,
    /// Ticks run (the next to run), and how many rings it may make.
    ticks: u32,
    rings_left: u32,
    /// The target's facing, whole degrees: the streaks' and the ring's axis is (sin, 0, cos) of it.
    facing: f32,
    streaks: Vec<Streak>,
    rings: Vec<Ring>,
}

/// The 115s playing, and the counter they share: each sets it to 0 at its tick 0, and a ring takes one of its count.
#[derive(Default)]
pub struct Blitzes {
    list: Vec<Blitz>,
    counter: u32,
}

/// The axis of a streak or ring, and the one across it on the ground, for a facing of `degrees`: the facing's mirror
/// across x, not the facing.
fn axes(degrees: f32) -> (Vector, Vector) {
    let (sin, cos) = degrees.trunc().to_radians().sin_cos();
    ([sin, 0.0, cos], [cos, 0.0, -sin])
}

/// A streak's place along its axis after drawn tick `tick`: from 15, its speed −1.2 falling 0.1 a tick, set to −0.3
/// rising 0.001 a tick at tick 10; each tick the speed changes, then it moves.
fn along(tick: u32) -> f32 {
    let (mut s, mut speed, mut change) = (STREAK_START, -1.2_f32, -0.1_f32);
    for age in 0..=tick {
        if age == 10 {
            (speed, change) = (-0.3, 0.001);
        }
        speed += change;
        s += speed;
    }
    s
}

/// A streak's half-length after drawn tick `tick`: its first grown by 0.1 a tick less 0.01 more each tick.
fn half_length(first: f32, tick: u32) -> f32 {
    (0..=tick).fold((first, 0.1_f32), |(length, growth), _| (length + growth - 0.01, growth - 0.01)).0
}

/// The streaks' alpha on drawn tick `tick`.
fn streak_alpha(tick: u32) -> f32 {
    match tick {
        0 => STREAK_ALPHAS[0],
        1..=17 => STREAK_ALPHAS[1],
        18 => STREAK_ALPHAS[2],
        _ => STREAK_ALPHAS[3],
    }
}

/// How far a ring has flown along its axis after drawn tick `tick`: speed 0.4 falling 1/60 a tick, changed first.
fn ring_flight(tick: u32) -> f32 {
    let k = (tick + 1) as f32;
    0.4 * k - k * (k + 1.0) / 120.0
}

impl Blitz {
    /// A 115 on `unit` from `now`, the target facing `facing` degrees, with `rings` rings to make; every random choice
    /// from `rand(n)` (0 to n − 1).
    pub fn new(unit: Option<u32>, facing: f32, rings: u32, now: Instant, mut rand: impl FnMut(u32) -> u32) -> Self {
        let streaks = (0..STREAKS)
            .map(|_| {
                let turn = (rand(360) as f32).to_radians();
                let length = (rand(40) + 20) as f32 / 10.0;
                Streak { around: [STREAK_OUT * turn.sin(), -STREAK_OUT * turn.cos()], length }
            })
            .collect();
        Blitz { start: now, unit, ticks: 0, rings_left: rings, facing, streaks, rings: Vec::new() }
    }

    fn tick_of(&self, now: Instant) -> Option<u32> {
        (now >= self.start).then(|| passes(self.start, now) - 1)
    }

    pub fn alive(&self, now: Instant) -> bool {
        passes(self.start, now) <= LENGTH
    }

    /// The streaks at `now` on feet `feet`: each two crossed quads (two triangles each), and the alpha (0 to 1).
    pub fn streaks(&self, now: Instant, feet: Vector) -> Vec<([[Corner; 6]; 2], f32)> {
        let Some(tick) = self.tick_of(now).filter(|&tick| tick < STREAK_LIFE) else { return Vec::new() };
        let alpha = streak_alpha(tick) / 255.0;
        if alpha <= 0.0 {
            return Vec::new();
        }
        let (axis, across) = axes(self.facing);
        let (centre_along, length) = (along(tick), |first| half_length(first, tick));
        self.streaks
            .iter()
            .map(|streak| {
                let middle: Vector = std::array::from_fn(|k| feet[k] + [streak.around[0], STREAK_RISE, streak.around[1]][k] + axis[k] * centre_along);
                let half = length(streak.length);
                let quad = |side: Vector| -> [Corner; 6] {
                    let point = |along: f32, across: f32| -> Vector { std::array::from_fn(|k| middle[k] + axis[k] * along + side[k] * across) };
                    // The texture's u runs from the +axis end (0), the leading one, to the −axis end (1); v across.
                    let corners = [
                        (point(half, -STREAK_WIDTH), [0.0, 1.0]),
                        (point(-half, -STREAK_WIDTH), [1.0, 1.0]),
                        (point(half, STREAK_WIDTH), [0.0, 0.0]),
                        (point(-half, STREAK_WIDTH), [1.0, 0.0]),
                    ];
                    [corners[0], corners[1], corners[2], corners[3], corners[2], corners[1]]
                };
                ([quad(across), quad([0.0, 1.0, 0.0])], alpha)
            })
            .collect()
    }

    /// The rings at `now` on feet `feet`: their triangles and alpha (0 to 1).
    pub fn rings(&self, now: Instant, feet: Vector) -> Vec<(Vec<Corner>, f32)> {
        let Some(tick) = self.tick_of(now) else { return Vec::new() };
        let (axis, across) = axes(self.facing);
        self.rings
            .iter()
            .filter_map(|ring| {
                let age = tick.checked_sub(ring.born).filter(|&age| age < RING_LIFE)?;
                let alpha = RING_ALPHAS[age as usize] / 255.0;
                if alpha <= 0.0 {
                    return None;
                }
                let flown = ring_flight(age);
                let centre: Vector = std::array::from_fn(|k| {
                    let drift = [0.0, 0.017_452 * flown, 0.0][k] + axis[k] * (2.017 + 0.999_85 * flown);
                    feet[k] + ring.start[k] + drift
                });
                // Round in the upright plane across the axis, the side at 90° pointing down; the outer edge further on.
                let point = |side: usize, edge: usize| -> Vector {
                    let (sin, cos) = (side as f32 / RING_SIDES as f32 * std::f32::consts::TAU).sin_cos();
                    let radius = RING_RADII[edge];
                    std::array::from_fn(|k| centre[k] + across[k] * radius * cos + [0.0, -radius * sin, 0.0][k] + axis[k] * edge as f32 * RING_FUNNEL)
                };
                let triangles = (0..RING_SIDES)
                    .flat_map(|side| {
                        let u = (side % 4) as f32 * 0.25;
                        let [inner, outer, next_inner, next_outer] = [
                            (point(side, 0), [u, 1.0]),
                            (point(side, 1), [u, 0.0]),
                            (point(side + 1, 0), [u + 0.25, 1.0]),
                            (point(side + 1, 1), [u + 0.25, 0.0]),
                        ];
                        [inner, outer, next_inner, outer, next_inner, next_outer]
                    })
                    .collect();
                Some((triangles, alpha))
            })
            .collect()
    }
}

impl Blitzes {
    /// A 115 from `now`.
    pub fn push(&mut self, blitz: Blitz) {
        self.list.push(blitz);
    }

    /// Runs every 115's ticks up to `now`, in the order they came due and, at one moment, the order they started: a
    /// tick 0 sets the counter to 0; a ring comes on a tick above 35 that is a multiple of 3 while the counter is
    /// below the 115's count, taking one of it. `rand(n)` makes the ring's choices (0 to n − 1).
    pub fn advance(&mut self, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        let mut due: Vec<(Instant, usize, u32)> = Vec::new();
        for (index, blitz) in self.list.iter().enumerate() {
            let last = passes(blitz.start, now).min(LENGTH);
            due.extend((blitz.ticks..last).map(|tick| (blitz.start + TICK * tick, index, tick)));
        }
        due.sort_by_key(|&(at, index, _)| (at, index));
        for (_, index, tick) in due {
            let blitz = &mut self.list[index];
            blitz.ticks = tick + 1;
            if tick == 0 {
                self.counter = 0;
            }
            if tick > RING_AFTER && tick % RING_EVERY == 0 && self.counter < blitz.rings_left {
                self.counter += 1;
                // The draws: where along x (0 to 2) and how high (2 to 12), and one more unused.
                let (a, b) = (rand(10), rand(20));
                let _unused = rand(20);
                let start = [((a + 1) / 5) as f32, (12 - b.div_ceil(2)) as f32 - RING_LOWER, 0.0];
                blitz.rings.push(Ring { born: tick, start });
            }
        }
        self.list.retain(|blitz| blitz.alive(now));
    }

    pub fn iter(&self) -> impl Iterator<Item = &Blitz> {
        self.list.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blitz(facing: f32, rings: u32, start: Instant) -> Blitz {
        Blitz::new(None, facing, rings, start, |_| 0)
    }

    #[test]
    fn ten_streaks_sweep_through_the_target() {
        // Ten at tick 0, each half-length 2 + (R(40))/10 (every draw 0: 2.0).
        let start = Instant::now();
        let one = blitz(0.0, 1, start);
        assert_eq!(one.streaks.len(), 10);
        assert_eq!(one.streaks[0].length, 2.0);
        // 15 out at first; −2.5 after tick 9; −5.445 after tick 19, 5.4 out on the far side... of 15.
        assert!((along(0) - 13.7).abs() < 1e-4 && (along(9) + 2.5).abs() < 1e-4 && (along(19) + 5.445).abs() < 1e-3, "{} {}", along(9), along(19));
        // Half-length L0 + 0.45 at tick 9, L0 − 0.1 at tick 19.
        assert!((half_length(2.0, 9) - 2.45).abs() < 1e-4 && (half_length(2.0, 19) - 1.9).abs() < 1e-4);
        // Alpha 125, then 250 for ticks 1–17, 125, 0.
        let alphas: Vec<f32> = (0..20).map(streak_alpha).collect();
        assert_eq!(alphas[0], 125.0);
        assert!(alphas[1..18].iter().all(|&alpha| alpha == 250.0));
        assert_eq!((alphas[18], alphas[19]), (125.0, 0.0));
        // Drawn 20 ticks, two quads each; the axis mirrors the facing across x.
        assert_eq!(one.streaks(start, [0.0; 3]).len(), 10);
        assert!(one.streaks(start + TICK * 19, [0.0; 3]).is_empty() && one.streaks(start + TICK * 20, [0.0; 3]).is_empty());
        assert_eq!(axes(0.0).0, [0.0, 0.0, 1.0]);
        assert!((axes(90.0).0[0] - 1.0).abs() < 1e-6 && axes(180.0).0[2] < -0.99);
    }

    #[test]
    fn the_ring_flies_along_its_axis_and_fades() {
        // d after tick k = 0.4(k + 1) − (k + 1)(k + 2)/120: 3.5 at tick 11.
        assert!((ring_flight(11) - 3.5).abs() < 1e-5);
        assert!((ring_flight(0) - (0.4 - 2.0 / 120.0)).abs() < 1e-6);
        assert_eq!(RING_ALPHAS.iter().filter(|&&alpha| alpha == 254.0).count(), 6);
        assert_eq!(RING_ALPHAS[6..], [211.0, 169.0, 127.0, 84.0, 42.0, 0.0]);
        // One ring, at tick 36, with a count of 1; none with 0.
        let start = Instant::now();
        let mut ones = Blitzes::default();
        ones.push(blitz(0.0, 1, start));
        ones.advance(start + TICK * 35, |_| 0);
        assert!(ones.list[0].rings.is_empty());
        ones.advance(start + TICK * 36, |_| 0);
        assert_eq!(ones.list[0].rings.len(), 1);
        ones.advance(start + TICK * 60, |_| 0);
        assert_eq!(ones.list[0].rings.len(), 1, "one ring only");
        let mut none = Blitzes::default();
        none.push(blitz(0.0, 0, start));
        none.advance(start + TICK * 60, |_| 0);
        assert!(none.list[0].rings.is_empty());
        // Its start: 0 to 2 along x, 12 less up to 6 up, 0.965 lower; 10 sides, two triangles each.
        assert_eq!(ones.list[0].rings[0].start, [0.0, 12.0 - RING_LOWER, 0.0]);
        assert_eq!(ones.list[0].rings(start + TICK * 36, [0.0; 3]).len(), 1);
        assert_eq!(ones.list[0].rings(start + TICK * 36, [0.0; 3])[0].0.len(), RING_SIDES * 6);
        assert!(ones.list[0].rings(start + TICK * (36 + 11), [0.0; 3]).is_empty(), "alpha 0 at its 12th tick");
    }

    #[test]
    fn rings_share_a_counter() {
        let start = Instant::now();
        // Two started together ring once between them.
        let mut together = Blitzes::default();
        together.push(blitz(0.0, 1, start));
        together.push(blitz(0.0, 1, start));
        together.advance(start + TICK * 40, |_| 0);
        assert_eq!(together.list.iter().map(|blitz| blitz.rings.len()).collect::<Vec<_>>(), [1, 0]);
        // One at tick 0 and one at 100: the first rings at 36 and again at 102, the second never.
        let mut later = Blitzes::default();
        later.push(blitz(0.0, 1, start));
        later.advance(start + TICK * 99, |_| 0);
        later.push(blitz(0.0, 1, start + TICK * 100));
        later.advance(start + TICK * 200, |_| 0);
        assert_eq!(later.list[0].rings.iter().map(|ring| ring.born).collect::<Vec<_>>(), [36, 102]);
        assert!(later.list[1].rings.is_empty());
    }
}
