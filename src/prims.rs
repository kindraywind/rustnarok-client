//! The game's hand-made effects: a builder per effect id that runs once a 16 ms tick and lets out primitives, each
//! moved once a tick by its own rule, drawn as a sprite with trail copies, a shape in the world or flat on the screen.
//! Soul Strike's ghosts, the cold hit and Frost Diver's ice spikes here; what flies to a target in `balls`, stone
//! spikes in `stone`, the bolts' arrows in `bolts`, Bash's burst in `burst`.

pub mod balls;
pub mod bolts;
pub mod burst;
pub mod sage;
pub mod stone;

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::world::Vector;

/// The game's world tick: effects move in steps of it, whatever the frame rate.
pub const TICK: Duration = Duration::from_millis(16);

/// How many ticks something whose first tick is at `start` has run by `now`, that first one included.
fn passes(start: Instant, now: Instant) -> u32 {
    if now < start { 0 } else { (now.duration_since(start).as_millis() / TICK.as_millis()) as u32 + 1 }
}

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

/// The ghosts in flight and the Soul Strikes letting them out; the cold hits bursting; Frost Diver's ice spikes; Fire
/// Ball's and Jupitel Thunder's balls; stone spikes.
#[derive(Default)]
pub struct Prims {
    builders: Vec<Builder>,
    ghosts: Vec<Ghost>,
    cold_hits: Vec<ColdHit>,
    spikes: Vec<Spike>,
    pub balls: balls::Balls,
    pub stone: stone::Stone,
    pub volleys: Vec<bolts::Volley>,
    pub sage: sage::Sage,
    pub bursts: Vec<burst::Burst>,
}

/// An ice spike (the game's prim kind 9): an open four-sided pyramid, its base `half` units out each way and its tip
/// `height` up its own axis. Born 30 units under where it stands, it rises up its axis 3 units the first tick, slowing
/// by 0.15 a tick, and stops after 21 ticks (28.35 up); it lives 40 ticks, fading over the last 10.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spike {
    start: Instant,
    /// Where its base starts (world units), and its axes: across, across, and up to the tip.
    base: Vector,
    axes: [Vector; 3],
    half: f32,
    height: f32,
    /// Its opacity before it fades, 0 to 1.
    alpha: f32,
}

/// Frost Diver's spikes start 5 units out toward the target and come 2 units further each tick, one a tick, the last
/// within 2.5 of the target (or for the effect's 150 ticks if it never gets there).
const FROST_FIRST: f32 = 5.0;
const FROST_STEP: f32 = 2.0;
const FROST_REACH: f32 = 2.5;
const FROST_TICKS: u32 = 150;
const SPIKE_LIFE: u32 = 40;
const SPIKE_DEPTH: f32 = 30.0;

/// How far up its axis a spike has risen after `tick` ticks (from 0): 3, slowing by 0.15 a tick, still after tick 20.
fn spike_rise(tick: u32) -> f32 {
    let moving = tick.min(20) + 1;
    (1..=moving).map(|step| 3.0 - 0.15 * step as f32).sum()
}

/// A spike's axes for its pitch and yaw (degrees): the tip leans `90 − pitch` from straight up toward `yaw`.
fn spike_axes(pitch: f32, yaw: f32) -> [Vector; 3] {
    let ((sin_p, cos_p), (sin_y, cos_y)) = (pitch.to_radians().sin_cos(), yaw.to_radians().sin_cos());
    [[cos_y, 0.0, -sin_y], [-sin_p * sin_y, cos_p, -sin_p * cos_y], [cos_p * sin_y, sin_p, cos_p * cos_y]]
}

impl Spike {
    /// A spike at `at` (where its base settles), `around` units off it the way `turn` points, from `rand(n)`.
    #[allow(clippy::too_many_arguments)]
    fn new(start: Instant, at: Vector, around: f32, turn: f32, half: f32, height: f32, pitch: f32, yaw: f32, alpha: f32) -> Self {
        let (sin, cos) = turn.to_radians().sin_cos();
        let base = [at[0] + around * sin, at[1] - SPIKE_DEPTH, at[2] - around * cos];
        Spike { start, base, axes: spike_axes(pitch, yaw), half, height, alpha }
    }

    fn opacity(&self, tick: u32) -> f32 {
        self.alpha * (1.0 - tick.saturating_sub(SPIKE_LIFE - 10) as f32 / 10.0).max(0.0)
    }

    /// Its four faces at a tick, each (corner, tip, corner), the texture's u stepping 0.2 a face.
    pub fn faces(&self, tick: u32) -> Vec<(Vector, [f32; 2], f32)> {
        let rise = spike_rise(tick);
        let [across, along, up] = self.axes;
        let point = |x: f32, y: f32, z: f32| std::array::from_fn(|axis| self.base[axis] + up[axis] * (rise + z) + across[axis] * x + along[axis] * y);
        let (s, alpha) = (self.half, self.opacity(tick));
        let tip = point(0.0, 0.0, self.height);
        [((-s, -s), (s, -s)), ((-s, -s), (-s, s)), ((-s, s), (s, s)), ((s, s), (s, -s))]
            .into_iter()
            .enumerate()
            .flat_map(|(face, ((ax, ay), (cx, cy)))| {
                let u = 0.2 * face as f32;
                [(point(ax, ay, 0.0), [u, 1.0], alpha), (tip, [u, 0.0], alpha), (point(cx, cy, 0.0), [u + 0.2, 1.0], alpha)]
            })
            .collect()
    }
}

/// COLDHIT's shard and smoke textures.
pub const ICE_TEXTURE: &str = "data\\texture\\effect\\ice.tga";
pub const SMOKE_TEXTURE: &str = "data\\texture\\effect\\smoke.tga";
/// How long COLDHIT's shards and puffs live, in ticks; when the second puff comes.
const SHARD_LIFE: u32 = 15;
const SMOKE_LIFE: u32 = 25;
const SECOND_SMOKE: u32 = 7;

/// One of COLDHIT's ice shards (the game's prim kind 2): a thin four-point quad on the screen, flying out from just over
/// the target's feet the way its head points and slowing to a third.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shard {
    /// Heading in degrees, 0 up the screen, turning clockwise; also how the quad is turned.
    yaw: f32,
    /// Length and width in screen pixels.
    length: f32,
    width: f32,
    /// Speed at first, screen pixels a tick.
    speed: f32,
    /// Where it starts, screen pixels from the target's feet (y down).
    start: [f32; 2],
}

/// One of COLDHIT's smoke puffs (the game's prim kind 0): a soft upright square growing fast, then slowly, drifting up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Smoke {
    yaw: f32,
    /// The tick it comes on.
    born: u32,
}

/// COLDHIT (51) on a unit: nine shards and a puff at once, a second puff 7 ticks later.
struct ColdHit {
    start: Instant,
    unit: Option<u32>,
    shards: [Shard; 9],
    smokes: [Smoke; 2],
}

/// A shape drawn in screen pixels from a unit's feet: its texture, then each corner's place (y down), texture
/// coordinate and color.
pub type ScreenShape = (&'static str, Option<u32>, Vec<([f32; 2], [f32; 2], [f32; 4])>);

/// COLDHIT's nine shards, from `rand(n)` (0 to n − 1): every 40° from up the screen, turned −15…+44°; the 0°, 120°
/// and 240° ones long, the rest short; each starting up to 4 px from 20 px over the feet.
pub fn shards(mut rand: impl FnMut(u32) -> u32) -> [Shard; 9] {
    std::array::from_fn(|index| {
        let slot = index as f32 * 40.0;
        let yaw = rand(60) as f32 + slot - 15.0;
        let long = index % 3 == 0;
        let (length, width) = if long { (rand(10) as f32 + 100.0, 10.0) } else { (rand(5) as f32 + 45.0, 4.0) };
        let speed = (rand(45) / 10) as f32 + if long { 12.8 } else { 4.8 };
        let out = rand(5) as f32;
        let (sin, cos) = slot.to_radians().sin_cos();
        Shard { yaw, length, width, speed, start: [sin * out, -cos * out - 20.0] }
    })
}

impl Shard {
    /// Where its middle is after `tick` ticks (from 0): each tick the speed drops by a 22.5th of the first, then it moves.
    fn place(&self, tick: u32) -> [f32; 2] {
        let (sin, cos) = self.yaw.to_radians().sin_cos();
        let moved: f32 = (0..=tick).map(|step| self.speed - (step + 1) as f32 * self.speed / 22.5).sum();
        [self.start[0] + sin * moved, self.start[1] - cos * moved]
    }

    /// Its opacity at a tick: 200 for five ticks, then 20 less a tick.
    fn alpha(tick: u32) -> f32 {
        (200.0 - 20.0 * tick.saturating_sub(4) as f32).max(0.0) / 255.0
    }

    /// Its two triangles at a tick: head, sides and tail turned by its heading; the tail always opaque white.
    fn shape(&self, tick: u32) -> Vec<([f32; 2], [f32; 2], [f32; 4])> {
        let middle = self.place(tick);
        let (sin, cos) = self.yaw.to_radians().sin_cos();
        let [half, side] = [self.length / 2.0, self.width / 2.0];
        let point = |[x, y]: [f32; 2]| [middle[0] + x * cos - y * sin, middle[1] + x * sin + y * cos];
        let color = [1.0, 1.0, 1.0, Self::alpha(tick)];
        let (head, tail) = ((point([0.0, -half]), [0.5, 0.0], color), (point([0.0, half]), [0.5, 1.0], [1.0; 4]));
        let (left, right) = ((point([-side, -half / 6.0]), [0.0, 0.5], color), (point([side, half / 6.0]), [1.0, 0.5], color));
        vec![left, tail, head, right, tail, head]
    }
}

impl Smoke {
    /// Its half-size after `tick` ticks of its own: 6 px more a tick up to 61, then half a pixel.
    fn half(tick: u32) -> f32 {
        if tick <= 9 { 1.0 + 6.0 * (tick + 1) as f32 } else { 61.0 + 0.5 * (tick - 9) as f32 }
    }

    /// Its opacity: 254 to tick 16, then down 31.75 a tick.
    fn alpha(tick: u32) -> f32 {
        (254.0 - 31.75 * tick.saturating_sub(16) as f32).max(0.0) / 255.0
    }

    fn shape(&self, tick: u32) -> Vec<([f32; 2], [f32; 2], [f32; 4])> {
        let (sin, cos) = self.yaw.to_radians().sin_cos();
        let drift = 0.85 * (tick + 1) as f32;
        let middle = [sin * drift - 0.5, -cos * drift - 0.5];
        let half = Self::half(tick);
        let color = [1.0, 1.0, 1.0, Self::alpha(tick)];
        let corner = |x: f32, y: f32, uv: [f32; 2]| ([middle[0] + x, middle[1] + y], uv, color);
        let [bottom_left, top_left, bottom_right, top_right] =
            [corner(-half, half, [0.0, 1.0]), corner(-half, -half, [0.0, 0.0]), corner(half, half, [1.0, 1.0]), corner(half, -half, [1.0, 0.0])];
        vec![bottom_left, top_left, bottom_right, bottom_right, top_left, top_right]
    }
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
        let at = [self.base[0] + fx * self.along + right[0] * out * sin, self.base[1] + out * cos, self.base[2] + fz * self.along + right[1] * out * sin];
        self.trail.push_front(at);
        self.trail.truncate(GHOST_TRAIL);
    }
}

impl Prims {
    /// Soul Strike (EF_SOULSTRIKE) from `from` at `to` with `count` hits: a ghost a hit, 6 ticks in and every 11 after.
    pub fn soul_strike(&mut self, from: Vector, to: Vector, count: u16, now: Instant) {
        self.builders.push(Builder { start: now, ticks: 0, from, to, count: u32::from(count.clamp(1, 7)) });
    }

    /// COLDHIT (EF_COLDHIT) on a unit (None = the player) from `now`, its random choices from `rand(n)`.
    pub fn cold_hit(&mut self, unit: Option<u32>, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        let shards = shards(&mut rand);
        let smokes = [0, SECOND_SMOKE].map(|born| Smoke { yaw: rand(40) as f32 - 20.0, born });
        self.cold_hits.push(ColdHit { start: now, unit, shards, smokes });
    }

    /// Frost Diver (EF_FROSTDIVER) from `from` at `to`: one spike a tick in a line toward the target.
    pub fn frost_diver(&mut self, from: Vector, to: Vector, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        let [dx, dz] = [to[0] - from[0], to[2] - from[2]];
        let distance = f32::hypot(dx, dz);
        let way = if distance > 0.0 { [dx / distance, dz / distance] } else { [0.0, 1.0] };
        for tick in 0..FROST_TICKS {
            let out = FROST_FIRST + FROST_STEP * tick as f32;
            let at = [from[0] + way[0] * out, from[1], from[2] + way[1] * out];
            let [turn, around, half, height, yaw, pitch] = [rand(360), rand(10), rand(40), rand(30), rand(360), rand(30)];
            let (around, half, height) = ((around + 5) as f32 / 10.0, (half + 60) as f32 / 100.0, (height + 150) as f32 / 10.0);
            let pitch = (pitch + 75) as f32;
            self.spikes.push(Spike::new(now + TICK * tick, at, around, turn as f32, half, height, pitch, yaw as f32, 254.0 / 255.0));
            if f32::hypot(to[0] - at[0], to[2] - at[2]) <= FROST_REACH {
                break;
            }
        }
    }

    /// Frost Diver's second hit effect (EF_FROSTDIVER2) at `at`: eight bigger spikes round the target at once.
    pub fn frost_hit(&mut self, at: Vector, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        for _ in 0..8 {
            let [turn, around, half, height, yaw, pitch] = [rand(360), rand(4), rand(250), rand(100), rand(360), rand(20)];
            let (around, half, height) = ((around + 1) as f32 / 10.0, (half + 100) as f32 / 100.0, (height + 200) as f32 / 10.0);
            self.spikes.push(Spike::new(now, at, around, turn as f32, half, height, (pitch + 80) as f32, yaw as f32, 200.0 / 255.0));
        }
    }

    /// The spikes showing at `now` and their ticks.
    pub fn spikes(&self, now: Instant) -> impl Iterator<Item = (&Spike, u32)> {
        self.spikes.iter().filter_map(move |spike| {
            let tick = (now.checked_duration_since(spike.start)?.as_millis() / TICK.as_millis()) as u32;
            (tick < SPIKE_LIFE).then_some((spike, tick))
        })
    }

    /// What the cold hits and bursts draw at `now`, in screen pixels from their units' feet.
    pub fn screen_shapes(&self, now: Instant) -> Vec<ScreenShape> {
        let mut shapes = Vec::new();
        for hit in &self.cold_hits {
            let Some(tick) = now.checked_duration_since(hit.start).map(|age| (age.as_millis() / TICK.as_millis()) as u32) else { continue };
            if tick < SHARD_LIFE {
                for shard in &hit.shards {
                    shapes.push((ICE_TEXTURE, hit.unit, shard.shape(tick)));
                }
            }
            for smoke in hit.smokes.iter().filter(|smoke| tick >= smoke.born && tick - smoke.born < SMOKE_LIFE) {
                shapes.push((SMOKE_TEXTURE, hit.unit, smoke.shape(tick - smoke.born)));
            }
        }
        shapes.extend(self.bursts.iter().flat_map(|burst| burst.shapes(now)));
        shapes
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
        self.cold_hits.retain(|hit| due(hit.start) < SECOND_SMOKE + SMOKE_LIFE);
        self.spikes.retain(|spike| now < spike.start || due(spike.start) < SPIKE_LIFE);
        self.balls.advance(now);
        self.stone.advance(now);
        self.volleys.retain(|volley| volley.alive(now));
        self.sage.advance(now);
        self.bursts.retain(|burst| burst.alive(now));
    }

    /// Every ghost copy to draw: where, how opaque and how big. Down a trail of n copies the i-th older is
    /// 1 − i/n as opaque and 1 − i/2n as big.
    /// Each copy is also turned by its ghost's fan angle (degrees, clockwise on the screen).
    pub fn ghosts(&self) -> Vec<(Vector, f32, f32, f32)> {
        let copies = GHOST_TRAIL as f32;
        self.ghosts
            .iter()
            .flat_map(|ghost| {
                ghost.trail.iter().enumerate().map(move |(age, &at)| {
                    let share = age as f32 / copies;
                    (at, 254.0 / 255.0 * (1.0 - share), GHOST_SCALE * (1.0 - share / 2.0), ghost.turn)
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frost_diver_lays_spikes_to_the_target() {
        // A target 45 units off: spikes 5, 7, … 43 units out, 20 of them, one a tick.
        let start = Instant::now();
        let mut prims = Prims::default();
        prims.frost_diver([0.0; 3], [0.0, 0.0, 45.0], start, |_| 0);
        assert_eq!(prims.spikes.len(), 20);
        // (each a little off its point: half a unit the way its random turn points)
        assert_eq!((prims.spikes[1].start - start, prims.spikes[19].base[2]), (TICK, 42.5));
        assert_eq!(prims.spikes(start + TICK * 5).count(), 6);
        // A spike rises 2.85 the first tick and settles 28.35 up after tick 20, 30 under where it stands at first.
        assert!((spike_rise(0) - 2.85).abs() < 1e-4 && (spike_rise(20) - 28.35).abs() < 1e-4 && spike_rise(39) == spike_rise(20));
        assert_eq!(prims.spikes[0].base[1], -SPIKE_DEPTH);
        // Straight up at pitch 90; four faces of three corners; fading over its last 10 ticks.
        let up = spike_axes(90.0, 0.0)[2];
        assert!((up[1] - 1.0).abs() < 1e-6 && up[0].abs() < 1e-6 && up[2].abs() < 1e-6);
        assert_eq!(prims.spikes[0].faces(0).len(), 12);
        assert_eq!((prims.spikes[0].opacity(29), prims.spikes[0].opacity(35)), (254.0 / 255.0, 254.0 / 255.0 * 0.5));
        // Its hit: eight spikes at once, 200 alpha.
        prims.frost_hit([0.0; 3], start, |_| 0);
        assert_eq!(prims.spikes.len(), 28);
        assert_eq!(prims.spikes[27].alpha, 200.0 / 255.0);
        prims.advance(start + TICK * 100);
        assert!(prims.spikes.is_empty());
    }

    #[test]
    fn cold_hit_bursts_shards_and_smoke() {
        // With every random draw 0: shards at −15°, 25°, … ; the 0°, 120°, 240° ones long (100 × 10 px, 12.8 px a
        // tick at first), the rest short (45 × 4, 4.8); all from 20 px over the feet.
        let shards = shards(|_| 0);
        assert_eq!((shards[0].yaw, shards[1].yaw, shards[3].yaw), (-15.0, 25.0, 105.0));
        assert_eq!((shards[0].length, shards[0].width, shards[0].speed), (100.0, 10.0, 12.8));
        assert_eq!((shards[1].length, shards[1].width, shards[1].speed), (45.0, 4.0, 4.8));
        assert_eq!(shards[4].start, [0.0, -20.0]);
        // A shard flies 9.67 times its first speed over its 15 ticks, slowing to a third.
        let straight = Shard { yaw: 90.0, ..shards[0] };
        let flown = straight.place(SHARD_LIFE - 1)[0] - straight.start[0];
        assert!((flown - 12.8 * (15.0 - 15.0 * 16.0 / 45.0)).abs() < 0.01, "{flown}");
        // Alpha 200 for 5 ticks, then down to 0 at tick 14; the smoke 254 to tick 16, 0 at 24; its half-size 61 at
        // tick 9, 68.5 at 24.
        assert_eq!([0, 4, 5, 14].map(Shard::alpha), [200.0 / 255.0, 200.0 / 255.0, 180.0 / 255.0, 0.0]);
        assert_eq!([16, 24].map(Smoke::alpha), [254.0 / 255.0, 0.0]);
        assert_eq!([0, 9, 24].map(Smoke::half), [7.0, 61.0, 68.5]);
        // Nine shards and a puff at first, the second puff from tick 7, nothing after tick 31.
        let start = Instant::now();
        let mut prims = Prims::default();
        prims.cold_hit(None, start, |_| 0);
        assert_eq!(prims.screen_shapes(start).len(), 10);
        assert_eq!(prims.screen_shapes(start + TICK * 7).len(), 11);
        assert_eq!(prims.screen_shapes(start + TICK * 20).len(), 2);
        prims.advance(start + TICK * 40);
        assert!(prims.cold_hits.is_empty());
    }

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
        let (_, alpha, scale, _) = flying.ghosts()[GHOST_TRAIL - 1];
        assert!((alpha - 254.0 / 255.0 / 12.0).abs() < 1e-4 && (scale - GHOST_SCALE * 13.0 / 24.0).abs() < 1e-4);
        flying.ghosts.clear();
        prims.advance(start + TICK * 200);
        assert!(prims.ghosts.is_empty() && prims.builders.is_empty());
    }
}
