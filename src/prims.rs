//! The game's hand-made effects: a builder per effect id that runs once a 16 ms tick and lets out primitives, each
//! moved once a tick by its own rule and drawn as a sprite with trail copies. So far Soul Strike's ghosts.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::world::Vector;

/// The game's world tick: effects move in steps of it, whatever the frame rate.
pub const TICK: Duration = Duration::from_millis(16);

/// Soul Strike's ghost sprite (under data\sprite\이팩트\).
pub const GHOST_SPRITE: &str = "data\\sprite\\이팩트\\particle1";
/// How big a ghost is drawn, and how many copies trail it.
pub const GHOST_SCALE: f32 = 2.75;
const GHOST_TRAIL: usize = 12;
/// A ghost's flight in ticks, and how high over the caster's feet it starts (world units).
const GHOST_LIFE: u32 = 40;
const GHOST_RISE: f32 = 10.0;

/// A builder: what an effect lets out, tick by tick.
struct Builder {
    start: Instant,
    /// Ticks run so far.
    ticks: u32,
    /// Where the caster stood, and the target (world units).
    from: Vector,
    to: Vector,
    /// The skill's hit count (the game passes it with the effect).
    count: u32,
}

/// Soul Strike's ghost (the game's prim kind 8): it flies from over the caster to the target in a straight line on
/// the ground's plane, slow at first and faster at the end, bulging out to one side of that line (its own way round,
/// a fan) and back.
struct Ghost {
    start: Instant,
    ticks: u32,
    base: Vector,
    /// Along the ground toward the target, and how far a tick on average.
    forward: [f32; 2],
    speed: f32,
    /// Taken off the speed: falls from 3.5 to −3.5 over the flight.
    slow: f32,
    /// How far out of the line (world units), and how fast that changes.
    bulge: f32,
    bulge_speed: f32,
    /// Which way round the line it bulges (degrees: 0 up).
    turn: f32,
    along: f32,
    /// Where it was drawn on the last ticks, newest first.
    trail: VecDeque<Vector>,
}

/// The ghosts in flight and the Soul Strikes letting them out.
#[derive(Default)]
pub struct Prims {
    builders: Vec<Builder>,
    ghosts: Vec<Ghost>,
}

/// When the `index`th of a Soul Strike's ghosts (from 0) leaves, in ticks: 6, then every 11.
fn ghost_tick(index: u32) -> u32 {
    6 + 11 * index
}

/// Which way round the line the `index`th of `count` ghosts bulges (degrees): spread over half a turn, from −90.
fn ghost_turn(index: u32, count: u32) -> f32 {
    let (step, first) = match count {
        2 => (180, -90),
        3 => (90, -90),
        4 => (60, -90),
        5 => (45, -90),
        6 => (30, -90),
        7 => (25, -90),
        _ => (1, 0),
    };
    ((index + 1) as i32 * step + first) as f32
}

/// When a Soul Strike with `hits` hits lets its ghosts out, from its start.
pub fn ghost_times(hits: u16) -> Vec<Duration> {
    (0..u32::from(hits.clamp(1, 7))).map(|index| TICK * ghost_tick(index)).collect()
}

impl Ghost {
    fn new(start: Instant, from: Vector, to: Vector, turn: f32) -> Self {
        let [dx, dz] = [to[0] - from[0], to[2] - from[2]];
        let distance = f32::hypot(dx, dz);
        let forward = if distance > 0.0 { [dx / distance, dz / distance] } else { [0.0, 1.0] };
        Self {
            start,
            ticks: 0,
            base: [from[0], from[1] + GHOST_RISE, from[2]],
            forward,
            speed: distance / GHOST_LIFE as f32,
            slow: 3.5,
            bulge: 0.0,
            bulge_speed: -2.0,
            turn,
            along: 0.0,
            trail: VecDeque::new(),
        }
    }

    /// One tick: the bulge moves by its speed as it was, the speed of the bulge and the slowing change, and the ghost
    /// goes on along the line.
    fn step(&mut self) {
        self.bulge += self.bulge_speed;
        self.bulge_speed += 4.0 / GHOST_LIFE as f32;
        self.slow -= 7.0 / GHOST_LIFE as f32;
        self.along += self.speed - self.slow;
        self.ticks += 1;
        let (sin, cos) = self.turn.to_radians().sin_cos();
        // The bulge is the game's y (down) turned about the line: up at 0°, to the line's right at 90°.
        let [fx, fz] = self.forward;
        let right = [fz, -fx];
        let out = -self.bulge;
        let at = [
            self.base[0] + fx * self.along + right[0] * out * sin,
            self.base[1] + out * cos,
            self.base[2] + fz * self.along + right[1] * out * sin,
        ];
        self.trail.push_front(at);
        self.trail.truncate(GHOST_TRAIL);
    }
}

impl Prims {
    /// Soul Strike (EF_SOULSTRIKE) from `from` at `to` with `count` hits: a ghost a hit, 6 ticks in and every 11 after.
    pub fn soul_strike(&mut self, from: Vector, to: Vector, count: u16, now: Instant) {
        self.builders.push(Builder { start: now, ticks: 0, from, to, count: u32::from(count.clamp(1, 7)) });
    }

    /// Runs the builders and the ghosts up to `now`, a tick at a time.
    pub fn advance(&mut self, now: Instant) {
        let due = |start: Instant| (now.saturating_duration_since(start).as_millis() / TICK.as_millis()) as u32;
        for builder in &mut self.builders {
            while builder.ticks < due(builder.start) {
                builder.ticks += 1;
                for index in (0..builder.count).filter(|&index| ghost_tick(index) == builder.ticks) {
                    let born = builder.start + TICK * builder.ticks;
                    self.ghosts.push(Ghost::new(born, builder.from, builder.to, ghost_turn(index, builder.count)));
                }
            }
        }
        self.builders.retain(|builder| builder.ticks < ghost_tick(builder.count));
        for ghost in &mut self.ghosts {
            while ghost.ticks < due(ghost.start).min(GHOST_LIFE) {
                ghost.step();
            }
        }
        self.ghosts.retain(|ghost| due(ghost.start) < GHOST_LIFE);
    }

    /// Every ghost copy to draw: where, how opaque and how big. Down a trail of n copies the i-th older is
    /// 1 − i/n as opaque and 1 − i/2n as big.
    pub fn ghosts(&self) -> Vec<(Vector, f32, f32)> {
        let copies = GHOST_TRAIL as f32;
        self.ghosts
            .iter()
            .flat_map(|ghost| {
                ghost.trail.iter().enumerate().map(move |(age, &at)| {
                    let share = age as f32 / copies;
                    (at, 254.0 / 255.0 * (1.0 - share), GHOST_SCALE * (1.0 - share / 2.0))
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soul_strike_lets_a_ghost_out_a_hit() {
        // Five hits: ghosts at ticks 6, 17, 28, 39 and 50, fanned −45° to 135°.
        assert_eq!((0..5).map(ghost_tick).collect::<Vec<_>>(), [6, 17, 28, 39, 50]);
        assert_eq!((0..5).map(|index| ghost_turn(index, 5)).collect::<Vec<_>>(), [-45.0, 0.0, 45.0, 90.0, 135.0]);
        let start = Instant::now();
        let mut prims = Prims::default();
        prims.soul_strike([0.0; 3], [0.0, 0.0, 40.0], 5, start);
        prims.advance(start + TICK * 18);
        assert_eq!(prims.ghosts.len(), 2);
        // The first ghost flies 40 ticks: half way out it bulges about 21 units, and it ends on the target.
        let mut ghost = Ghost::new(start, [0.0; 3], [0.0, 0.0, 40.0], 0.0);
        for _ in 0..20 {
            ghost.step();
        }
        assert!((ghost.trail[0][1] - (GHOST_RISE + 21.0)).abs() < 0.01, "{:?}", ghost.trail[0]);
        for _ in 20..GHOST_LIFE {
            ghost.step();
        }
        assert!((ghost.trail[0][2] - 43.5).abs() < 0.01 && (ghost.trail[0][1] - (GHOST_RISE + 2.0)).abs() < 0.01, "{:?}", ghost.trail[0]);
        assert_eq!(ghost.trail.len(), GHOST_TRAIL);
        // The oldest copy is a twelfth as opaque and 13/24 as big.
        let mut flying = Prims { ghosts: vec![ghost], ..Prims::default() };
        let (_, alpha, scale) = flying.ghosts()[GHOST_TRAIL - 1];
        assert!((alpha - 254.0 / 255.0 / 12.0).abs() < 1e-4 && (scale - GHOST_SCALE * 13.0 / 24.0).abs() < 1e-4);
        flying.ghosts.clear();
        prims.advance(start + TICK * 200);
        assert!(prims.ghosts.is_empty() && prims.builders.is_empty());
    }
}
