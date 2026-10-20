//! The game's hand-made effects: a builder per effect id that runs once a 16 ms tick and lets out primitives, each
//! moved once a tick by its own rule, drawn as a sprite with trail copies, a shape in the world or flat on the screen.
//! Soul Strike's ghosts and the cold hit here; what flies to a target in `balls`, spikes (Frost Diver's ice and the
//! stone ones) in `stone`, the bolts' arrows in `bolts`, Bash's burst in `burst`.

pub mod balls;
pub mod blitz;
pub mod bolts;
pub mod boomerang;
pub mod brawl;
pub mod burst;
pub mod circle;
pub mod dancer;
pub mod first;
pub mod ground;
pub mod guard;
pub mod huujin;
pub mod knight;
pub mod monk;
pub mod napalm;
pub mod sage;
pub mod shafts;
pub mod shards;
pub mod sight;
pub mod sniper;
pub mod songs;
pub mod sparkle;
pub mod spray;
pub mod star;
pub mod stone;
pub mod tanji;
pub mod throw;
pub mod transcend;
pub mod walls;
pub mod waterball;

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::world::{EffectQuad, Vector};

/// The game's world tick: effects move in steps of it, whatever the frame rate.
pub const TICK: Duration = Duration::from_millis(16);

/// How many ticks something whose first tick is at `start` has run by `now`, that first one included.
fn passes(start: Instant, now: Instant) -> u32 {
    if now < start { 0 } else { (now.duration_since(start).as_millis() / TICK.as_millis()) as u32 + 1 }
}

/// Soul Strike's ghost sprite (under data\sprite\이팩트\).
pub const GHOST_SPRITE: &str = "data\\sprite\\이팩트\\particle1";
/// Dark Strike's ghost: the same one-frame glow, red.
pub const RED_GHOST_SPRITE: &str = "data\\sprite\\이팩트\\particle5";
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
    /// Dark Strike's red glow instead of Soul Strike's pale yellow.
    red: bool,
}

/// A ghost (the game's prim kind 8): Soul Strike's flies from over the caster to the target in a straight line on the ground's
/// plane, slow at first and faster at the end, bulging out to one side of that line (its own way round, a fan) and back; Soul
/// Drain's and Indulge's are the same with other numbers.
struct Ghost {
    start: Instant,
    /// Ticks it is ahead of its time: 1 for a ghost whose first step is its first tick.
    lead: u32,
    ticks: u32,
    life: u32,
    base: Vector,
    /// Along the ground toward the target, and how far a tick on average.
    forward: [f32; 2],
    speed: f32,
    /// Taken off the speed: falls from its first value to the same below zero over the flight.
    slow: f32,
    slow_fall: f32,
    /// How far out of the line (world units), and how fast that changes.
    bulge: f32,
    bulge_speed: f32,
    /// Which way round the line it bulges (degrees: 0 up).
    turn: f32,
    along: f32,
    /// Where it was drawn on the last ticks, newest first, and how many.
    trail: VecDeque<Vector>,
    trail_len: usize,
    /// Its size and colour.
    scale: f32,
    tint: [f32; 3],
    /// Dark Strike's red glow.
    red: bool,
}

/// What makes a ghost: when it starts, where it flies, how fast, and how it looks.
struct GhostSpec {
    start: Instant,
    lead: u32,
    life: u32,
    /// Where its trail begins (the caster's feet) and where the ghost does.
    trail_from: Vector,
    base: Vector,
    forward: [f32; 2],
    speed: f32,
    slow: f32,
    turn: f32,
    trail_len: usize,
    scale: f32,
    tint: [f32; 3],
    red: bool,
}

/// One copy of a ghost to draw: where, how opaque (0 to 1), how big, its turn, its colour and whether it is a red one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GhostDraw {
    pub at: Vector,
    pub alpha: f32,
    pub scale: f32,
    pub turn: f32,
    pub tint: [f32; 3],
    pub red: bool,
}

/// The ghosts in flight and the Soul Strikes letting them out; the cold hits bursting; Fire Ball's and Jupitel
/// Thunder's balls; the spikes.
#[derive(Default)]
pub struct Prims {
    builders: Vec<Builder>,
    ghosts: Vec<Ghost>,
    cold_hits: Vec<ColdHit>,
    pub balls: balls::Balls,
    pub stone: stone::Stone,
    pub volleys: Vec<bolts::Volley>,
    pub sage: sage::Sage,
    pub bursts: Vec<burst::Burst>,
    pub boomerangs: boomerang::Boomerangs,
    pub napalms: Vec<napalm::Napalm>,
    pub sparkles: Vec<sparkle::Hallutime>,
    pub blues: Vec<sparkle::Blues>,
    pub blitzes: blitz::Blitzes,
    pub spheres: tanji::Spheres,
    pub huujin: huujin::Huujin,
    pub throws: throw::Throws,
    pub first: first::First,
    pub sights: sight::Sights,
    pub waterballs: waterball::Waterballs,
    pub walls: walls::Walls,
    pub shards: shards::Shards,
    pub monk: monk::Monk,
    pub stars: star::Stars,
    pub ground: ground::Ground,
    pub songs: songs::Songs,
    pub knight: knight::Knight,
    pub transcend: transcend::Transcend,
    pub sniper: sniper::Sniper,
    pub dancer: dancer::Dancer,
    pub guard: guard::Guard,
    pub spray: spray::Spray,
    pub brawl: brawl::Brawl,
    pub shafts: shafts::Shafts,
    pub circles: circle::Circles,
}

/// COLDHIT's shard and smoke textures.
pub const ICE_TEXTURE: &str = "data\\texture\\effect\\ice.tga";
pub const SMOKE_TEXTURE: &str = "data\\texture\\effect\\smoke.tga";
/// How long COLDHIT's shards and puffs live, in ticks; the age (in frames) the second puff comes at; and how many frames
/// the effect runs (ages 0–49, its parts last drawn at 48).
const SHARD_LIFE: u32 = 15;
const SMOKE_LIFE: u32 = 25;
const SECOND_SMOKE: u32 = 7;
const COLD_FRAMES: u32 = 50;

/// A negative whole angle looked up one degree up (−15 is −14), as the game's tables do.
fn lookup(angle: f32) -> f32 {
    if angle < 0.0 { angle + 1.0 } else { angle }
}

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

/// A shard's or puff's fade over what its effect has left: from its own tick `from` its alpha (0–255) drops `step` a tick.
#[derive(Clone, Copy)]
struct Fade {
    from: u32,
    alpha: f32,
    step: f32,
}

/// COLDHIT (51) on a unit: nine shards and a puff at once, a second puff when its age reaches 7. Its age E counts frames:
/// each by the ticks due in it, or by 1 when none is; its shards and puffs move on ticks only.
struct ColdHit {
    start: Instant,
    unit: Option<u32>,
    shards: [Shard; 9],
    smokes: [Smoke; 2],
    /// Its age E (None before its first frame), the ticks run by the last frame, and the tick the second puff came on.
    age: Option<u32>,
    ticks: u32,
    second: Option<u32>,
    /// The nine shards' fades, then the two puffs'.
    fades: [Option<Fade>; 11],
}

/// A shape drawn in screen pixels from a unit's feet: its texture, then each corner's place (y down), texture
/// coordinate and color.
pub type ScreenShape = (&'static str, Option<u32>, Vec<([f32; 2], [f32; 2], [f32; 4])>);

impl ColdHit {
    fn tick_at(&self, now: Instant) -> u32 {
        (now.saturating_duration_since(self.start).as_millis() / TICK.as_millis()) as u32
    }

    /// The tick a shard (0–8) or puff (9, 10) came on, once it has.
    fn born(&self, index: usize) -> Option<u32> {
        if index == 10 { self.second } else { Some(0) }
    }

    /// Its life in ticks and its alpha (0–255) at its own tick `own`, fade or not.
    fn life(index: usize) -> u32 {
        if index < 9 { SHARD_LIFE } else { SMOKE_LIFE }
    }

    fn alpha(&self, index: usize, own: u32) -> f32 {
        let plain = if index < 9 { Shard::alpha(own) } else { Smoke::alpha(own) } * 255.0;
        self.fades[index].map_or(plain, |fade| (fade.alpha - fade.step * own.saturating_sub(fade.from) as f32).max(0.0))
    }

    /// Runs a frame at `now`: its age goes up by the ticks due, or by 1; the second puff comes at age 7; a part that would
    /// outlive the effect starts to fade, on the first of its ticks where its life left is over 51 − E, over the 50 − E
    /// frames the effect has left.
    fn advance(&mut self, now: Instant) {
        let ticks = self.tick_at(now);
        let first = self.age.is_none();
        let age = self.age.map_or(0, |age| age + ticks.saturating_sub(self.ticks).max(1));
        self.age = Some(age);
        if self.second.is_none() && age >= SECOND_SMOKE {
            self.second = Some(ticks);
        }
        for tick in (if first { 0 } else { self.ticks + 1 })..=ticks {
            for index in 0..11 {
                let Some(own) = self.born(index).and_then(|born| tick.checked_sub(born)) else { continue };
                let life = Self::life(index);
                if own < life && self.fades[index].is_none() && age < COLD_FRAMES && i64::from(life - own) > 51 - i64::from(age) {
                    let alpha = self.alpha(index, own);
                    self.fades[index] = Some(Fade { from: own, alpha, step: alpha / (COLD_FRAMES - age) as f32 });
                }
            }
        }
        self.ticks = ticks;
    }

    /// Its shards and puffs at `now`, in the effect's last frame gone with it.
    fn shapes(&self, now: Instant) -> Vec<ScreenShape> {
        let tick = self.tick_at(now);
        let mut shapes = Vec::new();
        if self.age.is_some_and(|age| age >= COLD_FRAMES - 1) {
            return shapes;
        }
        if tick < SHARD_LIFE {
            for (index, shard) in self.shards.iter().enumerate() {
                shapes.push((ICE_TEXTURE, self.unit, shard.shape(tick, self.alpha(index, tick) / 255.0)));
            }
        }
        for (smoke, index) in self.smokes.iter().zip([9, 10]) {
            let Some(own) = self.born(index).and_then(|born| tick.checked_sub(born)).filter(|&own| own < SMOKE_LIFE) else { continue };
            shapes.push((SMOKE_TEXTURE, self.unit, smoke.shape(own, self.alpha(index, own) / 255.0)));
        }
        shapes
    }
}

/// COLDHIT's nine shards, from `rand(n)` (0 to n − 1): every 40° from up the screen, turned −15…+44°; the 0°, 120°
/// and 240° ones long, the rest short; each starting up to 4 px from 20 px over the feet.
pub fn shards(mut rand: impl FnMut(u32) -> u32) -> [Shard; 9] {
    std::array::from_fn(|index| {
        let slot = index as f32 * 40.0;
        let yaw = lookup(rand(60) as f32 + slot - 15.0);
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

    /// Its two triangles at a tick, `alpha` (0 to 1) opaque: head, sides and tail turned by its heading; the tail always
    /// opaque white.
    fn shape(&self, tick: u32, alpha: f32) -> Vec<([f32; 2], [f32; 2], [f32; 4])> {
        let middle = self.place(tick);
        let (sin, cos) = self.yaw.to_radians().sin_cos();
        let [half, side] = [self.length / 2.0, self.width / 2.0];
        let point = |[x, y]: [f32; 2]| [middle[0] + x * cos - y * sin, middle[1] + x * sin + y * cos];
        let color = [1.0, 1.0, 1.0, alpha];
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

    fn shape(&self, tick: u32, alpha: f32) -> Vec<([f32; 2], [f32; 2], [f32; 4])> {
        let (sin, cos) = self.yaw.to_radians().sin_cos();
        let drift = 0.85 * (tick + 1) as f32;
        let middle = [sin * drift - 0.5, -cos * drift - 0.5];
        let half = Self::half(tick);
        let color = [1.0, 1.0, 1.0, alpha];
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
    // A negative fan angle is looked up a degree up (−45 is −44).
    lookup(((index + 1) as i32 * step + first) as f32)
}

/// When a Soul Strike with `hits` hits lets its ghosts out, from its start.
pub fn ghost_times(hits: u16) -> Vec<Duration> {
    // 1 to 7 balls; 8 and more (up to 36) give none.
    if hits > 7 {
        return Vec::new();
    }
    (0..u32::from(hits.max(1))).map(|index| TICK * ghost_tick(index)).collect()
}

impl Ghost {
    /// Soul Strike's (or Dark Strike's) ghost from `from` to `to`, bulging round `turn`.
    fn new(start: Instant, from: Vector, to: Vector, turn: f32, red: bool) -> Self {
        let [dx, dz] = [to[0] - from[0], to[2] - from[2]];
        let distance = f32::hypot(dx, dz);
        let forward = if distance > 0.0 { [dx / distance, dz / distance] } else { [0.0, 1.0] };
        Self::from_spec(GhostSpec {
            start,
            lead: 0,
            life: GHOST_LIFE,
            trail_from: from,
            base: [from[0], from[1] + GHOST_RISE, from[2]],
            forward,
            speed: distance / GHOST_LIFE as f32,
            slow: 3.5,
            turn,
            trail_len: GHOST_TRAIL,
            scale: GHOST_SCALE,
            tint: [1.0; 3],
            red,
        })
    }

    fn from_spec(spec: GhostSpec) -> Self {
        Self {
            start: spec.start,
            lead: spec.lead,
            ticks: 0,
            life: spec.life,
            base: spec.base,
            forward: spec.forward,
            speed: spec.speed,
            slow: spec.slow,
            slow_fall: 2.0 * spec.slow / spec.life as f32,
            bulge: 0.0,
            bulge_speed: -2.0,
            turn: spec.turn,
            along: 0.0,
            trail: std::iter::repeat_n(spec.trail_from, spec.trail_len).collect(),
            trail_len: spec.trail_len,
            scale: spec.scale,
            tint: spec.tint,
            red: spec.red,
        }
    }

    /// One tick: the bulge moves by its speed as it was, the speed of the bulge and the slowing change, and the ghost
    /// goes on along the line.
    fn step(&mut self) {
        self.bulge += self.bulge_speed;
        self.bulge_speed += 4.0 / self.life as f32;
        self.slow -= self.slow_fall;
        self.along += self.speed - self.slow;
        self.ticks += 1;
        let (sin, cos) = self.turn.to_radians().sin_cos();
        // The bulge is the game's y (down) turned about the line: up at 0°, to the line's right (a quarter turn clockwise
        // seen from above: toward +x when flying toward −z) at 90°.
        let [fx, fz] = self.forward;
        let right = [-fz, fx];
        let out = -self.bulge;
        let at = [self.base[0] + fx * self.along + right[0] * out * sin, self.base[1] + out * cos, self.base[2] + fz * self.along + right[1] * out * sin];
        self.trail.push_front(at);
        self.trail.truncate(self.trail_len);
    }
}

/// The ground direction a flight takes for the heading of (dx, dz): the whole degree before the heading's own (0 below 1).
fn flight_direction(dx: f32, dz: f32) -> [f32; 2] {
    let heading = crate::actor::step_rot(dx, dz).trunc();
    let (sin, cos) = (if heading < 1.0 { 0.0 } else { heading - 1.0 }).to_radians().sin_cos();
    [sin, -cos]
}

impl Prims {
    /// Soul Strike (EF_SOULSTRIKE) from `from` at `to` with `count` hits: a ghost a hit, 6 ticks in and every 11 after.
    pub fn soul_strike(&mut self, from: Vector, to: Vector, count: u16, now: Instant, red: bool) {
        if count > 7 {
            return;
        }
        self.builders.push(Builder { start: now, ticks: 0, from, to, count: u32::from(count.max(1)), red });
    }

    /// Soul Drain (EF_ENERGYDRAIN2) from the caster's feet `from` to the target's `to`: three sparks fanned −90°, 0° and 90°,
    /// each flying from over the target toward the caster, their choices from `rand(n)` (0 to n − 1).
    pub fn soul_drain(&mut self, from: Vector, to: Vector, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        let distance = f32::hypot(from[0] - to[0], from[2] - to[2]);
        let forward = flight_direction(from[0] - to[0], from[2] - to[2]);
        for angle in [-90.0, 0.0, 90.0] {
            let (red, blue) = (rand(81) as f32, rand(81) as f32);
            let scale = 1.5 + 0.1 * rand(6) as f32;
            self.ghosts.push(Ghost::from_spec(GhostSpec {
                start: now,
                lead: 1,
                life: 60,
                trail_from: from,
                base: [to[0], from[1] + GHOST_RISE, to[2]],
                forward,
                speed: distance / 60.0,
                slow: 2.0,
                turn: lookup(angle),
                trail_len: 7,
                scale,
                tint: [(160.0 + red) / 255.0, (160.0 + blue) / 255.0, 1.0],
                red: false,
            }));
        }
    }

    /// Blood Drain (EF_BLOODDRAIN) from the feet `from` toward `aim`: three red drops fanned −90°, 0° and 90°, each flying out past
    /// the aim and back (a drop's forward speed 2 · D / 60, D the distance), every one at once.
    pub fn blood_drain(&mut self, from: Vector, aim: Vector, now: Instant) {
        let (dx, dz) = (aim[0] - from[0], aim[2] - from[2]);
        let distance = f32::hypot(dx, dz);
        let forward = flight_direction(dx, dz);
        for angle in [-90.0, 0.0, 90.0] {
            self.ghosts.push(Ghost::from_spec(GhostSpec {
                start: now,
                lead: 1,
                life: 60,
                trail_from: from,
                base: [from[0], from[1] + GHOST_RISE, from[2]],
                forward,
                speed: distance / 30.0,
                slow: 2.0,
                turn: lookup(angle),
                trail_len: 7,
                scale: 1.7,
                tint: [250.0 / 255.0, 100.0 / 255.0, 100.0 / 255.0],
                red: false,
            }));
        }
    }

    /// Indulge (EF_ENERGYDRAIN3) round the caster's feet `at`: three sparks on each of five ticks, each flying out against a
    /// heading of its own and back; their choices from `rand(n)` in the order the game draws them.
    pub fn indulge(&mut self, at: Vector, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        for wave in 0..5 {
            for angle in [-90.0, 0.0, 90.0] {
                let (first, second) = (rand(7) as f32, rand(7) as f32);
                let (red, blue) = (rand(81) as f32, rand(81) as f32);
                let scale = 1.0 + 0.1 * rand(6) as f32;
                self.ghosts.push(Ghost::from_spec(GhostSpec {
                    start: now + TICK * wave,
                    lead: 1,
                    life: 60,
                    trail_from: at,
                    base: [at[0], at[1] + GHOST_RISE, at[2]],
                    forward: flight_direction(second - 3.0, first - 3.0),
                    speed: 0.0,
                    slow: 2.0,
                    turn: lookup(angle),
                    trail_len: 7,
                    scale,
                    tint: [(160.0 + red) / 255.0, 1.0, (160.0 + blue) / 255.0],
                    red: false,
                }));
            }
        }
    }

    /// COLDHIT (EF_COLDHIT) on a unit (None = the player) from `now`, its random choices from `rand(n)`.
    pub fn cold_hit(&mut self, unit: Option<u32>, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        let shards = shards(&mut rand);
        let smokes = [0, SECOND_SMOKE].map(|born| Smoke { yaw: lookup(rand(40) as f32 - 20.0), born });
        self.cold_hits.push(ColdHit { start: now, unit, shards, smokes, age: None, ticks: 0, second: None, fades: [None; 11] });
    }

    /// Napalm Beat (EF_NAPALMBEAT 32) on a unit (None = the player) from `now`, its random choices from `rand(n)`.
    pub fn napalm_beat(&mut self, unit: Option<u32>, now: Instant, rand: impl FnMut(u32) -> u32) {
        self.napalms.push(napalm::Napalm::new(unit, now, rand));
    }

    /// EF_HALLUTIME (793) round an Escape copy's feet.
    /// Hovering's blue sparkles round `unit` (None = the player).
    pub fn blue_sparkles(&mut self, unit: Option<u32>, now: Instant) {
        self.blues.push(sparkle::Blues::new(unit, now));
    }

    pub fn sparkle(&mut self, unit: Option<u32>, at: Vector, now: Instant, rand: impl FnMut(u32) -> u32) {
        self.sparkles.push(sparkle::Hallutime::new(unit, at, now, rand));
    }

    /// Drops any prims attached to `unit`.
    pub fn forget(&mut self, unit: u32) {
        self.sparkles.retain(|s| s.unit != Some(unit));
        self.walls.forget(unit);
        self.ground.forget(unit);
        self.transcend.forget(unit);
        self.dancer.forget(unit);
        self.guard.forget(unit);
        self.brawl.forget(unit);
        self.circles.forget(Some(unit));
    }

    /// The camera-facing squares for effect 793 at `now`.
    pub fn sparkle_quads<'a>(
        &self,
        now: Instant,
        dim: bool,
        texture_of: impl Fn(&str) -> Option<&'a crate::render::Texture>,
        position_of: &impl Fn(Option<u32>) -> Option<Vector>,
    ) -> Vec<EffectQuad<'a>> {
        self.sparkles
            .iter()
            .flat_map(|s| s.quads(now, dim, &texture_of, position_of))
            .chain(self.blues.iter().flat_map(|blues| blues.quads(now, dim, &texture_of)))
            .collect()
    }

    /// What the cold hits and bursts draw at `now`, in screen pixels from their units' feet.
    pub fn screen_shapes(&self, now: Instant) -> Vec<ScreenShape> {
        let mut shapes = Vec::new();
        for hit in self.cold_hits.iter().filter(|hit| now >= hit.start) {
            shapes.extend(hit.shapes(now));
        }
        shapes.extend(self.bursts.iter().filter(|burst| burst.fixed.is_none()).flat_map(|burst| burst.shapes(now)));
        shapes.extend(self.napalms.iter().flat_map(|napalm| napalm.shapes(now)));
        shapes.extend(self.first.shapes(now));
        shapes.extend(self.brawl.shapes(now));
        shapes
    }

    /// What the bursts over a place in the world (Flasher's) draw at `now`: the place, and the shapes in screen pixels from it.
    pub fn fixed_shapes(&self, now: Instant) -> Vec<(Vector, ScreenShape)> {
        self.bursts
            .iter()
            .filter_map(|burst| Some((burst.fixed?, burst)))
            .flat_map(|(at, burst)| burst.shapes(now).into_iter().map(move |shape| (at, shape)))
            .collect()
    }

    /// Runs the builders and the ghosts up to `now`, a tick at a time.
    pub fn advance(&mut self, now: Instant) {
        let due = |start: Instant| (now.saturating_duration_since(start).as_millis() / TICK.as_millis()) as u32;
        for builder in &mut self.builders {
            while builder.ticks < due(builder.start) {
                builder.ticks += 1;
                for index in (0..builder.count).filter(|&index| ghost_tick(index) == builder.ticks) {
                    let born = builder.start + TICK * builder.ticks;
                    self.ghosts.push(Ghost::new(born, builder.from, builder.to, ghost_turn(index, builder.count), builder.red));
                }
            }
        }
        self.builders.retain(|builder| builder.ticks < ghost_tick(builder.count));
        for ghost in &mut self.ghosts {
            while now >= ghost.start && ghost.ticks < (due(ghost.start) + ghost.lead).min(ghost.life) {
                ghost.step();
            }
        }
        self.ghosts.retain(|ghost| now < ghost.start || due(ghost.start) < ghost.life);
        for hit in &mut self.cold_hits {
            hit.advance(now);
        }
        self.cold_hits.retain(|hit| due(hit.start) < SECOND_SMOKE + SMOKE_LIFE && hit.age.is_none_or(|age| age < COLD_FRAMES));
        self.balls.advance(now);
        self.waterballs.advance(now);
        self.shards.advance(now);
        self.stars.advance(now);
        self.dancer.advance(now);
        self.guard.advance(now);
        self.ground.advance(now);
        self.walls.advance(now, |n| crate::effects::pick(0, n.saturating_sub(1)));
        self.stone.advance(now);
        self.volleys.retain(|volley| volley.alive(now));
        self.sage.advance(now);
        self.bursts.retain(|burst| burst.alive(now));
        self.napalms.retain(|napalm| napalm.alive(now));
        self.sparkles.retain(|sparkle| sparkle.alive(now));
        self.blues.retain(|blues| blues.alive(now));
        self.shafts.advance(now);
        self.blitzes.advance(now, |n| crate::effects::pick(0, n.saturating_sub(1)));
    }

    /// Every ghost copy to draw: where, how opaque and how big. Down a trail of n copies the i-th older is
    /// 1 − i/n as opaque and 1 − i/2n as big.
    /// Each copy is also turned by its ghost's fan angle (degrees, clockwise on the screen).
    pub fn ghosts(&self) -> Vec<GhostDraw> {
        self.ghosts
            .iter()
            .filter(|ghost| ghost.ticks > 0)
            .flat_map(|ghost| {
                let copies = ghost.trail_len as f32;
                ghost.trail.iter().enumerate().map(move |(age, &at)| {
                    let share = age as f32 / copies;
                    GhostDraw {
                        at,
                        alpha: 254.0 / 255.0 * (1.0 - share),
                        scale: ghost.scale * (1.0 - share / 2.0),
                        turn: ghost.turn,
                        tint: ghost.tint,
                        red: ghost.red,
                    }
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cold_hit_bursts_shards_and_smoke() {
        // With every random draw 0: shards at −15°, 25°, … ; the 0°, 120°, 240° ones long (100 × 10 px, 12.8 px a
        // tick at first), the rest short (45 × 4, 4.8); all from 20 px over the feet.
        let shards = shards(|_| 0);
        assert_eq!((shards[0].yaw, shards[1].yaw, shards[3].yaw), (-14.0, 25.0, 105.0), "the first looked up a degree up");
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
        // Nine shards and a puff at first, the second puff from tick 7, nothing after tick 31 (a frame a tick).
        let start = Instant::now();
        let mut prims = Prims::default();
        prims.cold_hit(None, start, |_| 0);
        let counts: Vec<usize> = (0..=20)
            .map(|tick| {
                prims.advance(start + TICK * tick);
                prims.screen_shapes(start + TICK * tick).len()
            })
            .collect();
        assert_eq!((counts[0], counts[6], counts[7], counts[20]), (10, 10, 11, 2));
        prims.advance(start + TICK * 40);
        assert!(prims.cold_hits.is_empty());
    }

    #[test]
    fn cold_hit_ages_by_frames_above_62_frames_a_second() {
        let start = Instant::now();
        let at = |frame: u32, rate: f64| start + std::time::Duration::from_secs_f64(f64::from(frame) / rate);
        // At 144 frames a second: the second puff 7 frames in (tick 3), drawn up to frame 48, gone from 49.
        let mut prims = Prims::default();
        prims.cold_hit(None, start, |_| 0);
        for frame in 0..=7 {
            prims.advance(at(frame, 144.0));
        }
        assert_eq!((prims.cold_hits[0].age, prims.cold_hits[0].second), (Some(7), Some(3)));
        for frame in 8..=48 {
            prims.advance(at(frame, 144.0));
        }
        assert!(!prims.screen_shapes(at(48, 144.0)).is_empty());
        prims.advance(at(49, 144.0));
        assert!(prims.screen_shapes(at(49, 144.0)).is_empty());
        prims.advance(at(50, 144.0));
        assert!(prims.cold_hits.is_empty(), "the effect ran its 50 frames");
        // At 60 frames a second the age is the tick, as before: the second puff on tick 7.
        let mut prims = Prims::default();
        prims.cold_hit(None, start, |_| 0);
        for frame in 0..=30 {
            prims.advance(at(frame, 60.0));
            let tick = (u128::from(frame) * 1000 / 60 / 16) as u32;
            assert_eq!(prims.cold_hits[0].age, Some(tick), "frame {frame}");
        }
        assert_eq!(prims.cold_hits[0].second, Some(7));
        // A part that would outlive the effect fades over what it has left: at 144 frames a second the first puff (25 ticks)
        // outlives the effect's 21 ticks, so near its end it takes a fade over the frames left; not at 60.
        let mut prims = Prims::default();
        prims.cold_hit(None, start, |_| 0);
        for frame in 0..=48 {
            prims.advance(at(frame, 144.0));
        }
        let fade = prims.cold_hits[0].fades[9].expect("the puff fades");
        let age = fade.from + 3;
        assert!(fade.from >= 18 && fade.alpha > 0.0 && fade.step > 0.0 && age > 0);
        let mut steady = Prims::default();
        steady.cold_hit(None, start, |_| 0);
        for frame in 0..=30 {
            steady.advance(at(frame, 60.0));
        }
        assert!(steady.cold_hits[0].fades.iter().all(Option::is_none));
        // A shard heading −15 moves along −14°.
        let shard = shards(|_| 0)[0];
        assert_eq!(shard.yaw, -14.0);
        let (sin, cos) = (-14.0_f32).to_radians().sin_cos();
        let moved = shard.place(0);
        let step = shard.speed - shard.speed / 22.5;
        assert!((moved[0] - shard.start[0] - sin * step).abs() < 1e-4 && (moved[1] - shard.start[1] + cos * step).abs() < 1e-4);
    }

    #[test]
    fn soul_strike_lets_a_ghost_out_a_hit() {
        // Five hits: ghosts at ticks 6, 17, 28, 39 and 50, fanned −45° to 135°.
        assert_eq!((0..5).map(ghost_tick).collect::<Vec<_>>(), [6, 17, 28, 39, 50]);
        assert_eq!((0..5).map(|index| ghost_turn(index, 5)).collect::<Vec<_>>(), [-44.0, 0.0, 45.0, 90.0, 135.0]);
        let start = Instant::now();
        let mut prims = Prims::default();
        prims.soul_strike([0.0; 3], [0.0, 0.0, 40.0], 5, start, false);
        prims.advance(start + TICK * 18);
        assert_eq!(prims.ghosts.len(), 2);
        // The first ghost flies 40 ticks: half way out it bulges about 21 units, and it ends on the target.
        let mut ghost = Ghost::new(start, [0.0; 3], [0.0, 0.0, 40.0], 0.0, false);
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
        let oldest = flying.ghosts()[GHOST_TRAIL - 1];
        assert!((oldest.alpha - 254.0 / 255.0 / 12.0).abs() < 1e-4 && (oldest.scale - GHOST_SCALE * 13.0 / 24.0).abs() < 1e-4);
        flying.ghosts.clear();
        prims.advance(start + TICK * 200);
        assert!(prims.ghosts.is_empty() && prims.builders.is_empty());
    }

    #[test]
    fn soul_drain_throws_three_blue_sparks_back_to_the_caster() {
        let start = Instant::now();
        let mut prims = Prims::default();
        let (caster, target) = ([0.0, 3.0, 0.0], [25.0, 0.0, 0.0]);
        // Every draw 0: colour (160, 160, 255), scale 1.5, three sparks fanned −90, 0, 90 (−90 looked up as −89).
        prims.soul_drain(caster, target, start, |_| 0);
        assert_eq!(prims.ghosts.len(), 3);
        assert_eq!(prims.ghosts.iter().map(|ghost| ghost.turn).collect::<Vec<_>>(), [-89.0, 0.0, 90.0]);
        assert!(
            prims
                .ghosts
                .iter()
                .all(|ghost| ghost.tint == [160.0 / 255.0, 160.0 / 255.0, 1.0] && ghost.scale == 1.5 && ghost.trail_len == 7 && ghost.life == 60)
        );
        // It starts over the target, 10 up at the caster's height; and flies toward the caster along the heading before its own.
        assert_eq!(prims.ghosts[1].base, [25.0, 13.0, 0.0]);
        // F_t = t (d/60 − 2) + t (t + 1)/30 for d = 25: −18.02 at t = 23, d + 2 = 27 at t = 60; A_t = −2t + t (t − 1)/30.
        let mut spark = Ghost::from_spec(GhostSpec {
            start,
            lead: 1,
            life: 60,
            trail_from: caster,
            base: [25.0, 13.0, 0.0],
            forward: [-1.0, 0.0],
            speed: 25.0 / 60.0,
            slow: 2.0,
            turn: 0.0,
            trail_len: 7,
            scale: 1.5,
            tint: [1.0; 3],
            red: false,
        });
        let (mut along, mut bulge) = (vec![], vec![]);
        for _ in 0..60 {
            spark.step();
            along.push(spark.along);
            bulge.push(spark.bulge);
        }
        assert!((along[22] - -18.017).abs() < 0.01 && (along[59] - 27.0).abs() < 0.01);
        assert!((bulge[29] - -31.0).abs() < 0.01 && (bulge[59] - -2.0).abs() < 0.01 && (bulge[0] - -2.0).abs() < 0.01);
        // A spark turned 90° flying toward −z arcs out to +x.
        let mut side = Ghost::new(start, [0.0; 3], [0.0, 0.0, -40.0], 90.0, false);
        for _ in 0..20 {
            side.step();
        }
        assert!(side.trail[0][0] > 10.0, "{:?}", side.trail[0]);
        // The trail starts filled with the caster's feet: on the first draw the older copies are there, copy 3 at 254 − 3 · 254/7.
        prims.advance(start);
        let draws = prims.ghosts();
        assert_eq!(draws.len(), 21);
        assert_eq!(draws[1].at, caster);
        assert!((draws[3].alpha - (254.0 - 3.0 * 254.0 / 7.0) / 255.0).abs() < 1e-4 && (draws[3].scale - 1.5 * (1.0 - 3.0 / 14.0)).abs() < 1e-4);
        // Drawn on ticks 0…59, not 60.
        prims.advance(start + TICK * 59);
        assert_eq!(prims.ghosts().len(), 21);
        prims.advance(start + TICK * 60);
        assert!(prims.ghosts.is_empty());
    }

    #[test]
    fn indulge_loops_fifteen_sparks_out_and_back() {
        let start = Instant::now();
        let mut prims = Prims::default();
        let caster = [10.0, 1.0, 10.0];
        prims.indulge(caster, start, |_| 0);
        assert_eq!(prims.ghosts.len(), 15);
        // Three on each of ticks 0…4; the colour (160, 255, 160), scale 1.0, speed 0, starting 10 up.
        for wave in 0..5 {
            assert_eq!(prims.ghosts.iter().filter(|ghost| ghost.start == start + TICK * wave).count(), 3);
        }
        assert!(
            prims
                .ghosts
                .iter()
                .all(|ghost| ghost.tint == [160.0 / 255.0, 1.0, 160.0 / 255.0] && ghost.scale == 1.0 && ghost.speed == 0.0 && ghost.base == [10.0, 11.0, 10.0])
        );
        // The heading of (−3, −3) is 315°: the flight takes the degree before, 314°.
        let [fx, fz] = prims.ghosts[0].forward;
        assert!((fx - 314.0_f32.to_radians().sin()).abs() < 1e-4 && (fz + 314.0_f32.to_radians().cos()).abs() < 1e-4);
        // F_t = −(2t − t (t + 1)/30): −29 at t = 29, 0 at 59, +2 at 60.
        let mut spark = Ghost::from_spec(GhostSpec {
            start,
            lead: 1,
            life: 60,
            trail_from: caster,
            base: [10.0, 11.0, 10.0],
            forward: [0.0, -1.0],
            speed: 0.0,
            slow: 2.0,
            turn: 0.0,
            trail_len: 7,
            scale: 1.0,
            tint: [1.0; 3],
            red: false,
        });
        let mut along = vec![];
        for _ in 0..60 {
            spark.step();
            along.push(spark.along);
        }
        assert!((along[28] - -29.0).abs() < 0.01 && along[58].abs() < 0.01 && (along[59] - 2.0).abs() < 0.01);
        // Nothing before a wave's tick; a spark made at tick 4 is last drawn at tick 63.
        prims.advance(start);
        assert_eq!(prims.ghosts().len(), 3 * 7);
        prims.advance(start + TICK * 63);
        assert!(prims.ghosts().iter().any(|draw| draw.alpha > 0.0));
        let last = prims.ghosts.iter().filter(|ghost| ghost.start == start + TICK * 4).count();
        assert_eq!(last, 3);
        prims.advance(start + TICK * 64);
        assert!(prims.ghosts.is_empty());
    }

    #[test]
    fn dark_strike_throws_red_balls_one_a_hit() {
        // Balls 1 to 7 at ticks 6, 17, 28 …; 8 to 36 none; 0 is one.
        let times = |hits: u16| ghost_times(hits).iter().map(|at| (at.as_millis() / TICK.as_millis()) as u32).collect::<Vec<_>>();
        assert_eq!(times(5), [6, 17, 28, 39, 50]);
        assert_eq!([times(7).len(), times(8).len(), times(36).len(), times(0).len()], [7, 0, 0, 1]);
        // The fans for n = 1…5 (a negative angle a degree up).
        let fan = |n: u32| (0..n).map(|index| ghost_turn(index, n)).collect::<Vec<_>>();
        assert_eq!(fan(1), [1.0]);
        assert_eq!(fan(2), [90.0, 270.0]);
        assert_eq!(fan(3), [0.0, 90.0, 180.0]);
        assert_eq!(fan(4), [-29.0, 30.0, 90.0, 150.0]);
        assert_eq!(fan(5), [-44.0, 0.0, 45.0, 90.0, 135.0]);
        // Dark Strike's are red (particle5), Soul Strike's not; none for 8.
        let start = Instant::now();
        let mut prims = Prims::default();
        prims.soul_strike([0.0; 3], [0.0, 0.0, 40.0], 3, start, true);
        prims.soul_strike([0.0; 3], [0.0, 0.0, 40.0], 3, start, false);
        prims.soul_strike([0.0; 3], [0.0, 0.0, 40.0], 8, start, true);
        assert_eq!(prims.builders.len(), 2);
        prims.advance(start + TICK * 7);
        assert_eq!(prims.ghosts.iter().map(|ghost| ghost.red).collect::<Vec<_>>(), [true, false]);
        assert!(RED_GHOST_SPRITE.ends_with("particle5") && GHOST_SPRITE.ends_with("particle1"));
    }

    #[test]
    fn napalm_beat_bursts_puffs() {
        let start = Instant::now();
        let mut prims = Prims::default();
        prims.napalm_beat(None, start, |_| 0);
        assert_eq!(prims.screen_shapes(start).len(), 1);
        assert_eq!(prims.screen_shapes(start + TICK * 1).len(), 2);
        assert_eq!(prims.screen_shapes(start + TICK * 14).len(), 14);
        assert_eq!(prims.screen_shapes(start + TICK * 27).len(), 1);
        assert_eq!(prims.screen_shapes(start + TICK * 28).len(), 0);
        prims.advance(start + TICK * 27);
        assert_eq!(prims.napalms.len(), 1);
        prims.advance(start + TICK * 28);
        assert!(prims.napalms.is_empty());
    }

    #[test]
    fn sparkle_advances_and_forgets() {
        let start = Instant::now();
        let mut prims = Prims::default();
        prims.sparkle(Some(55), [0.0; 3], start, |_| 0);
        assert_eq!(prims.sparkles.len(), 1);
        prims.advance(start + TICK * 99);
        assert_eq!(prims.sparkles.len(), 1);
        prims.advance(start + TICK * 100);
        assert!(prims.sparkles.is_empty());

        prims.sparkle(Some(55), [0.0; 3], start, |_| 0);
        prims.forget(55);
        assert!(prims.sparkles.is_empty());
    }

    #[test]
    fn blood_drain_throws_three_drops_out_past_the_aim_and_back() {
        let start = Instant::now();
        let at = |ticks: u32| {
            let mut prims = Prims::default();
            prims.blood_drain([0.0, 0.0, 0.0], [0.0, 0.0, -15.0], start);
            prims.advance(start + TICK * (ticks - 1));
            (prims.ghosts.len(), prims.ghosts[0].along, prims.ghosts[0].bulge, prims.ghosts[0].trail_len)
        };
        // D = 15: the drops are 3 at once with 7 trail copies; forward −14 at t = 30 and 32 at t = 60 (2D + 2); the arc −31 at 30.
        let (count, along, bulge, trail) = at(30);
        assert!(count == 3 && trail == 7 && (along + 14.0).abs() < 0.01 && (bulge + 31.0).abs() < 0.01);
        assert!((at(60).1 - 32.0).abs() < 0.01);
        // Round the caster itself (D = 0): out to −29 and back to 2.
        let mut near_self = Prims::default();
        near_self.blood_drain([5.0, 0.0, 5.0], [5.0, 0.0, 5.0], start);
        near_self.advance(start + TICK * 59);
        assert!((near_self.ghosts[0].along - 2.0).abs() < 0.01);
        // The fan: the drops turn −89, 0 and 90 degrees round the line; in #FA6464 at scale 1.7.
        assert_eq!(near_self.ghosts.iter().map(|ghost| ghost.turn).collect::<Vec<_>>(), [-89.0, 0.0, 90.0]);
        assert!(near_self.ghosts.iter().all(|ghost| ghost.scale == 1.7 && ghost.tint == [250.0 / 255.0, 100.0 / 255.0, 100.0 / 255.0]));
    }
}
