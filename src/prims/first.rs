//! Six first-job skill effects, built tick by tick from a few exact parts: the small heal's glowing column and
//! sparkles (7), Endure's crest and closing streaks (11), Magnum Break's two rings and turning dome (17), Steal's ten
//! sparks (18), Detoxify's rising sparkles (21) and Decrease AGI's sinking sign and falling streaks (38).
//!
//! An effect has an age e (0 at its launch) and a length N; a part made at age e has its own age a. A part with life L
//! made at e is drawn min(L, N − e − 1) times: the effect's end cuts it. Alphas are cut to whole numbers when drawn.

use std::time::Instant;

use super::passes;
use crate::world::{MeshVertex, Vector};

pub const ALPHA_DOWN: &str = "data\\texture\\effect\\alpha_down.tga";
pub const ENDURE: &str = "data\\texture\\effect\\endure.tga";
pub const RING_YELLOW: &str = "data\\texture\\effect\\ring_yellow.tga";
pub const DOME: &str = "data\\texture\\effect\\대폭발.tga";
pub const SLOW: &str = "data\\texture\\effect\\slow.bmp";
pub const STREAK: &str = "data\\texture\\effect\\ac_center2.tga";
pub const STAR: &str = "data\\sprite\\이팩트\\particle1";
pub const GOLD_STAR: &str = "data\\sprite\\이팩트\\particle7";
pub const CYAN_STAR: &str = "data\\sprite\\이팩트\\particle2";
pub const DEX_AGI_TEXT: &str = "data\\texture\\effect\\dex_agi_up.bmp";
/// Holy Water's bottle (a sprite of 42 frames, a frame every 2 ticks).
pub const HOLY_WATER_SPRITE: &str = "data\\sprite\\이팩트\\성수뜨기";

/// The effects this makes.
pub const SMALL_HEAL: u32 = 7;
pub const ENDURE_FX: u32 = 11;
pub const MAGNUM_BREAK: u32 = 17;
pub const STEAL: u32 = 18;
pub const DETOXIFY: u32 = 21;
pub const DECREASE_AGI: u32 = 38;
pub const HOLY_WATER: u32 = 39;
pub const DEX_AGI_UP: u32 = 43;

pub fn makes(effect: u32) -> bool {
    matches!(effect, SMALL_HEAL | ENDURE_FX | MAGNUM_BREAK | STEAL | DETOXIFY | DECREASE_AGI | HOLY_WATER | DEX_AGI_UP)
}

/// The effects' lengths N in ticks.
fn length(effect: u32) -> u32 {
    match effect {
        SMALL_HEAL | DETOXIFY | DECREASE_AGI | HOLY_WATER | DEX_AGI_UP => 100,
        ENDURE_FX => 80,
        MAGNUM_BREAK => 30,
        _ => 50,
    }
}

/// A screen quad's corners and a world triangle's corners: where, texture coordinate and colour.
pub type ScreenCorner = ([f32; 2], [f32; 2], [f32; 4]);
pub type ScreenShape = (&'static str, Option<u32>, Vec<ScreenCorner>);
/// A world mesh: its texture and triangles.
pub type Mesh = (&'static str, Vec<MeshVertex>);

/// A sprite particle: which sprite, where, how big (a multiplier of its pixels), how opaque (0 to 255) and turned.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Particle {
    pub sprite: &'static str,
    pub at: Vector,
    pub scale: f32,
    pub alpha: f32,
    pub spin: f32,
}

/// The random choices of a part made at age `born`.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Seed {
    born: u32,
    /// Yaw in degrees.
    y: f32,
    /// What else it drew: its distance, half-width, half-height and the like (per effect).
    r: [f32; 4],
}

/// One of Steal's sparks, run tick by tick over the ground.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Spark {
    at: Vector,
    yaw: f32,
    run: f32,
    run_fall: f32,
    rise: f32,
    rise_fall: f32,
    scale: f32,
    ticks: u32,
}

struct Effect {
    id: u32,
    start: Instant,
    /// The unit it follows (None = the player), or where it stands.
    unit: Option<u32>,
    follows: bool,
    at: Vector,
    seeds: Vec<Seed>,
    sparks: Vec<Spark>,
}

/// The effects playing.
#[derive(Default)]
pub struct First {
    list: Vec<Effect>,
}

/// Sine and cosine of whole degrees.
fn trig(degrees: f32) -> (f32, f32) {
    degrees.to_radians().sin_cos()
}

/// A negative whole angle looked up a degree up.
fn lookup(angle: f32) -> f32 {
    if angle < 0.0 { angle + 1.0 } else { angle }
}

impl First {
    /// Effect `id` from `now`, on `unit` (None = the player) standing at `at`, following it where the effect does;
    /// `min` is /mineffect; every random choice from `rand(n)` (0 to n − 1), in the order the parts are made.
    pub fn play(&mut self, id: u32, unit: Option<u32>, at: Vector, now: Instant, min: bool, mut rand: impl FnMut(u32) -> u32) {
        let mut seeds = Vec::new();
        let mut seed = |born: u32, y: f32, r: [f32; 4]| seeds.push(Seed { born, y, r });
        match id {
            SMALL_HEAL if !min => {
                // A sparkle every 9 ticks (the one made at 99 is never drawn): one draw thrown away, then its yaw.
                for born in (0..=90).step_by(9) {
                    let _ = rand(2);
                    seed(born, rand(360) as f32, [0.0; 4]);
                }
            }
            ENDURE_FX => {
                // A streak every tick to 40: yaw, half-width, half-height, distance.
                for born in 0..=40 {
                    let y = rand(360) as f32;
                    let width = (rand(100) + 60) as f32 / 100.0;
                    let height = (rand(20) + 30) as f32;
                    let d = (rand(40) + 100) as f32;
                    seed(born, y, [width, height, d, 0.0]);
                }
            }
            STEAL => {
                for _ in 0..10 {
                    let y = rand(360) as f32;
                    let p = (rand(100) as i32 - 50) as f32;
                    let d = (4 + rand(4)) as f32;
                    let v0 = (rand(50) + 50) as f32 / 100.0;
                    let u0 = (rand(90) + 60) as f32 / 100.0;
                    let s = (rand(100) + 100) as f32 / 100.0;
                    seed(0, y, [p, d, v0, u0 + s * 1000.0]);
                }
            }
            DETOXIFY => {
                // A sparkle every 5 ticks up to 60 (every 15 under /mineffect).
                for born in (0..=60).step_by(if min { 15 } else { 5 }) {
                    let y = rand(360) as f32;
                    let d = (2 + rand(6)) as f32;
                    let v0 = (rand(30) + 30) as f32 / 100.0;
                    let s = (rand(50) + 40) as f32 / 100.0;
                    seed(born, y, [d, v0, s, 0.0]);
                }
            }
            DEX_AGI_UP => {
                // A streak every second tick to 50: yaw, distance, speed, half-length.
                for born in (0..=50).step_by(2) {
                    let y = rand(360) as f32;
                    let d = (2 + rand(7)) as f32;
                    let speed = (rand(50) + 20) as f32 / 100.0;
                    let half = (rand(60) + 30) as f32 / 10.0;
                    seed(born, y, [d, speed, half, 0.0]);
                }
            }
            DECREASE_AGI => {
                // A streak every second tick to 50: yaw, distance, half-length.
                for born in (0..=50).step_by(2) {
                    let y = rand(360) as f32;
                    let d = (2 + rand(7)) as f32;
                    let half = (rand(60) + 30) as f32 / 10.0;
                    seed(born, y, [d, half, 0.0, 0.0]);
                }
            }
            _ => {}
        }
        let sparks = if id == STEAL {
            seeds
                .iter()
                .map(|seed| {
                    let [p, d, v0, packed] = seed.r;
                    let (u0, s) = (packed % 1000.0, (packed / 1000.0).floor());
                    let p = lookup(p);
                    let ((sin_y, cos_y), (sin_p, cos_p)) = (trig(seed.y), trig(p));
                    let start = [at[0] + d * sin_y, at[1] + d * cos_y * sin_p + 10.0, at[2] + d * cos_y * cos_p];
                    Spark { at: start, yaw: seed.y, run: v0, run_fall: v0 / (2.0 * 50.0), rise: u0, rise_fall: 2.0 * u0 / 50.0, scale: s, ticks: 0 }
                })
                .collect()
        } else {
            Vec::new()
        };
        self.list.push(Effect { id, start: now, unit, follows: !matches!(id, MAGNUM_BREAK | STEAL | DETOXIFY), at, seeds, sparks });
    }

    /// Runs Steal's sparks up to `now`, a tick at a time over the ground (`ground_at(x, z)`: its height in the world), and
    /// drops what has ended.
    pub fn advance(&mut self, now: Instant, ground_at: impl Fn(f32, f32) -> f32) {
        for effect in self.list.iter_mut().filter(|effect| effect.id == STEAL) {
            while effect.sparks[0].ticks < passes(effect.start, now).min(length(STEAL) - 1) {
                for spark in &mut effect.sparks {
                    spark.run = (spark.run - spark.run_fall).max(0.0);
                    let (sin, cos) = trig(spark.yaw);
                    spark.at[0] += spark.run * sin;
                    spark.at[2] += spark.run * cos;
                    spark.rise -= spark.rise_fall;
                    spark.at[1] += spark.rise;
                    // A spark moving down below the ground bounces: its vertical speed divided by −1.5.
                    if spark.rise < 0.0 && spark.at[1] < ground_at(spark.at[0], spark.at[2]) {
                        spark.rise /= -1.5;
                    }
                    spark.ticks += 1;
                }
            }
        }
        self.list.retain(|effect| passes(effect.start, now) < length(effect.id));
    }

    /// The effect's tick e at `now` (None before it starts).
    fn tick(effect: &Effect, now: Instant) -> Option<u32> {
        passes(effect.start, now).checked_sub(1)
    }

    /// A part made at `born` with life `life` is at age a at `now`: drawn min(life, N − born − 1) times.
    fn age(effect: &Effect, now: Instant, born: u32, life: u32) -> Option<u32> {
        let tick = Self::tick(effect, now)?;
        let a = tick.checked_sub(born)?;
        (a < life.min(length(effect.id).saturating_sub(born + 1))).then_some(a)
    }

    fn place(effect: &Effect, position_of: &impl Fn(Option<u32>) -> Option<Vector>) -> Option<Vector> {
        if effect.follows { position_of(effect.unit) } else { Some(effect.at) }
    }

    /// The parts drawn flat on the screen from their unit's feet: Endure's crest and streaks, Decrease AGI's sign.
    pub fn shapes(&self, now: Instant) -> Vec<ScreenShape> {
        let mut shapes = Vec::new();
        for effect in &self.list {
            match effect.id {
                ENDURE_FX => {
                    if let Some(a) = Self::age(effect, now, 0, 80) {
                        let n = (a + 1).min(45) as f32;
                        let half = 150.0 - 5.0 * n + n * (n + 1.0) / 18.0;
                        let alpha = if a < 20 {
                            12.7 * (a + 1) as f32
                        } else if a <= 53 {
                            254.0
                        } else {
                            254.0 - 254.0 * (a - 53) as f32 / 26.0
                        };
                        shapes.push((ENDURE, effect.unit, square([0.0, -20.0], [half, half], alpha)));
                    }
                    for seed in &effect.seeds {
                        let Some(a) = Self::age(effect, now, seed.born, 40) else { continue };
                        let n = (a + 1) as f32;
                        let [width, height, d, _] = seed.r;
                        let radius = d - 4.0 * n + 0.05 * n * (n + 1.0);
                        let (sin, cos) = trig(seed.y);
                        let centre = [radius * sin, -20.0 - radius * cos];
                        let alpha = if a < 20 {
                            12.7 * (a + 1) as f32
                        } else if a <= 26 {
                            254.0
                        } else {
                            254.0 - 19.54 * (a - 26) as f32
                        };
                        shapes.push((ALPHA_DOWN, effect.unit, turned(centre, [width, height], seed.y, alpha)));
                    }
                }
                DEX_AGI_UP => {
                    if let Some(a) = Self::age(effect, now, 0, 100) {
                        let k = (a + 1) as f32;
                        let rise = 1.5 * k - 0.009 * k * (k + 1.0);
                        let alpha = super::burst::alpha(a, 0.0, 200.0 / 15.0, 200.0, 85, 100);
                        shapes.push((DEX_AGI_TEXT, effect.unit, square([0.0, -rise], [40.0, 20.0], alpha)));
                    }
                }
                DECREASE_AGI => {
                    if let Some(a) = Self::age(effect, now, 0, 100) {
                        let n = (a + 1) as f32;
                        let alpha = if a < 14 {
                            13.33 * (a + 1) as f32
                        } else if a <= 84 {
                            200.0
                        } else {
                            200.0 - 13.33 * (a - 84) as f32
                        };
                        shapes.push((SLOW, effect.unit, square([0.0, -80.0 + n - 0.005 * n * (n + 1.0)], [40.0, 10.0], alpha.min(200.0))));
                    }
                }
                _ => {}
            }
        }
        shapes
    }

    /// The parts drawn in the world, depth-tested: the heal's column, Magnum Break's dome, Decrease AGI's streaks.
    pub fn meshes(&self, now: Instant, position_of: impl Fn(Option<u32>) -> Option<Vector>) -> Vec<Mesh> {
        let mut meshes = Vec::new();
        for effect in &self.list {
            let Some(at) = Self::place(effect, &position_of) else { continue };
            match effect.id {
                SMALL_HEAL => {
                    if let Some(a) = Self::age(effect, now, 0, 100) {
                        let alpha = if a < 30 {
                            3.0 * (a + 1) as f32
                        } else if a <= 79 {
                            90.0
                        } else {
                            90.0 - 4.5 * (a - 79) as f32
                        };
                        meshes.push((ALPHA_DOWN, column(at, alpha)));
                    }
                }
                MAGNUM_BREAK => {
                    if let Some(a) = Self::age(effect, now, 0, 30) {
                        meshes.push((DOME, dome(at, a, magnum_alpha(a))));
                    }
                }
                DECREASE_AGI => {
                    for seed in &effect.seeds {
                        let Some(a) = Self::age(effect, now, seed.born, 50) else { continue };
                        meshes.push((STREAK, blades(at, seed, a)));
                    }
                }
                DEX_AGI_UP => {
                    for seed in &effect.seeds {
                        let Some(a) = Self::age(effect, now, seed.born, 50) else { continue };
                        meshes.push((STREAK, rising_blades(at, seed, a)));
                    }
                }
                _ => {}
            }
        }
        meshes
    }

    /// Magnum Break's two rings, flat on the ground and drawn with the depth test off, over everything: the same ring twice.
    pub fn top_meshes(&self, now: Instant) -> Vec<Mesh> {
        let mut meshes = Vec::new();
        for effect in self.list.iter().filter(|effect| effect.id == MAGNUM_BREAK) {
            if let Some(a) = Self::age(effect, now, 0, 30) {
                let ring = ring(effect.at, a, magnum_alpha(a));
                meshes.push((RING_YELLOW, ring.clone()));
                meshes.push((RING_YELLOW, ring));
            }
        }
        meshes
    }

    /// Holy Water's bottle sprites playing at `now`: where (20 over the unit's feet) and which frame (1 to 41): a frame every
    /// two ticks, frame 0 never shown, the sprite over at age 82.
    pub fn holy_waters(&self, now: Instant, position_of: impl Fn(Option<u32>) -> Option<Vector>) -> Vec<(Vector, usize)> {
        self.list
            .iter()
            .filter(|effect| effect.id == HOLY_WATER)
            .filter_map(|effect| {
                let a = Self::age(effect, now, 0, 82)?;
                let at = Self::place(effect, &position_of)?;
                Some(([at[0], at[1] + 20.0, at[2]], (a / 2 + 1) as usize))
            })
            .collect()
    }

    /// The sprite particles: the heal's sparkles, Steal's sparks, Detoxify's sparkles.
    pub fn particles(&self, now: Instant, position_of: impl Fn(Option<u32>) -> Option<Vector>) -> Vec<Particle> {
        let mut particles = Vec::new();
        for effect in &self.list {
            let Some(at) = Self::place(effect, &position_of) else { continue };
            match effect.id {
                SMALL_HEAL => {
                    for seed in &effect.seeds {
                        let Some(a) = Self::age(effect, now, seed.born, 50) else { continue };
                        let k = (a + 1) as f32;
                        let rise = 1.2 * k - 0.008 * k * (k + 1.0);
                        let (sin, cos) = trig(seed.y);
                        let place = [at[0] + 6.0 * sin, at[1] + rise, at[2] + 6.0 * cos];
                        particles.push(Particle { sprite: STAR, at: place, scale: 0.55, alpha: heal_sparkle_alpha(seed.born, a), spin: 0.0 });
                    }
                }
                STEAL => {
                    for spark in &effect.sparks {
                        let Some(a) = Self::age(effect, now, 0, 50).filter(|&a| a < 49 && a < spark.ticks) else { continue };
                        let alpha = if a <= 33 { 254.0 } else { 254.0 - 15.875 * (a - 33) as f32 };
                        particles.push(Particle { sprite: GOLD_STAR, at: spark.at, scale: spark.scale * (1.0 - spark.ticks as f32 / 50.0), alpha, spin: 0.0 });
                    }
                }
                DETOXIFY => {
                    for seed in &effect.seeds {
                        let Some(a) = Self::age(effect, now, seed.born, 40) else { continue };
                        let n = (a + 1) as f32;
                        let [d, v0, s, _] = seed.r;
                        let (sin, cos) = trig(seed.y);
                        let place = [at[0] + d * sin, at[1] + v0 * (n - n * (n + 1.0) / 160.0), at[2] + d * cos];
                        let alpha = if a < 10 {
                            25.4 * n
                        } else if a <= 31 {
                            254.0
                        } else {
                            254.0 - 31.75 * (a - 31) as f32
                        };
                        particles.push(Particle { sprite: CYAN_STAR, at: place, scale: s, alpha, spin: 3.0 * n });
                    }
                }
                _ => {}
            }
        }
        particles
    }
}

/// The heal's sparkle's alpha at age `a` for one made at effect age `born`: from 254, a fade from the start when its life
/// reaches past the effect's end, and one over 16 ticks from age 34.
pub fn heal_sparkle_alpha(born: u32, a: u32) -> f32 {
    let (mut alpha, mut step) = (254.0_f32, 0.0_f32);
    for age in 0..=a {
        if age == 0 && born > 50 {
            step = -254.0 / (99 - born) as f32;
        }
        if age == 34 {
            step = -alpha / 16.0;
        }
        alpha = (alpha + step).max(0.0);
    }
    alpha
}

/// Magnum Break's alpha at age `a`: up to 254 over 15 ticks, then down by the same.
pub fn magnum_alpha(a: u32) -> f32 {
    if a <= 14 { (16.93 * (a + 1) as f32).min(254.0) } else { (254.0 - 16.93 * (a - 14) as f32).max(0.0) }
}

/// A screen square of half-size `half` at `centre` (y down), `alpha` 0 to 255: two triangles.
fn square(centre: [f32; 2], half: [f32; 2], alpha: f32) -> Vec<ScreenCorner> {
    let color = [1.0, 1.0, 1.0, alpha.trunc().clamp(0.0, 255.0) / 255.0];
    let corner = |x: f32, y: f32, uv: [f32; 2]| ([centre[0] + x * half[0], centre[1] + y * half[1]], uv, color);
    let [a, b, c, d] = [corner(-1.0, -1.0, [0.0, 0.0]), corner(1.0, -1.0, [1.0, 0.0]), corner(1.0, 1.0, [1.0, 1.0]), corner(-1.0, 1.0, [0.0, 1.0])];
    vec![a, b, c, a, c, d]
}

/// A thin screen quad turned clockwise by `degrees`, its length (the second half) radial with the top outward.
fn turned(centre: [f32; 2], half: [f32; 2], degrees: f32, alpha: f32) -> Vec<ScreenCorner> {
    let (sin, cos) = trig(degrees);
    let color = [1.0, 1.0, 1.0, alpha.trunc().clamp(0.0, 255.0) / 255.0];
    // Local x across, y along (down); clockwise by Y: x → (cos, sin), y → (−sin, cos).
    let corner = |x: f32, y: f32, uv: [f32; 2]| {
        let (x, y) = (x * half[0], y * half[1]);
        ([centre[0] + x * cos - y * sin, centre[1] + x * sin + y * cos], uv, color)
    };
    let [a, b, c, d] = [corner(-1.0, -1.0, [0.0, 0.0]), corner(1.0, -1.0, [1.0, 0.0]), corner(1.0, 1.0, [1.0, 1.0]), corner(-1.0, 1.0, [0.0, 1.0])];
    vec![a, b, c, a, c, d]
}

/// The heal's column: an upright cylinder of 10 segments, radius 4.5, 35 tall, on the feet `at`.
fn column(at: Vector, alpha: f32) -> Vec<MeshVertex> {
    let tint = [1.0, 1.0, 1.0, alpha.trunc().clamp(0.0, 255.0) / 255.0];
    (0..10)
        .flat_map(|segment| {
            let u = (segment % 4) as f32 * 0.25;
            let point = |side: u32, top: bool, u: f32| {
                let (sin, cos) = ((36 * side) as f32).to_radians().sin_cos();
                MeshVertex {
                    position: [at[0] + 4.5 * cos, at[1] + if top { 35.0 } else { 0.0 }, at[2] + 4.5 * sin],
                    uv: [u, if top { 0.0 } else { 1.0 }],
                    tint,
                }
            };
            let [bottom0, top0, bottom1, top1] =
                [point(segment, false, u), point(segment, true, u), point(segment + 1, false, u + 0.25), point(segment + 1, true, u + 0.25)];
            [bottom0, top0, bottom1, bottom1, top0, top1]
        })
        .collect()
}

/// Magnum Break's ring at age `a`: flat on the ground, outer radius 1.75 n − 0.0145833 n (n + 1), 12 wide at most.
fn ring(at: Vector, a: u32, alpha: f32) -> Vec<MeshVertex> {
    let n = (a + 1) as f32;
    let outer = 1.75 * n - 0.014_583_3 * n * (n + 1.0);
    let inner = outer - outer.min(12.0);
    let tint = [1.0, 1.0, 1.0, alpha.trunc().clamp(0.0, 255.0) / 255.0];
    (0..10)
        .flat_map(|segment| {
            let u = (segment % 4) as f32 * 0.25;
            let point = |side: u32, radius: f32, u: f32, v: f32| {
                let (sin, cos) = ((36 * side) as f32).to_radians().sin_cos();
                MeshVertex { position: [at[0] + radius * cos, at[1], at[2] + radius * sin], uv: [u, v], tint }
            };
            let [inner0, outer0, inner1, outer1] = [
                point(segment, inner, u, 1.0),
                point(segment, outer, u, 0.0),
                point(segment + 1, inner, u + 0.25, 1.0),
                point(segment + 1, outer, u + 0.25, 0.0),
            ];
            [inner0, outer1, outer0, inner0, inner1, outer1]
        })
        .collect()
}

/// Magnum Break's dome at age `a`: a sphere of radius 1.15 n − 0.0095833 n (n + 1) centred on the feet, turning 3° a tick, 5
/// rows of 10 cells; a negative angle is used as that angle + 1 (so the bottom pole is a 1° hole).
fn dome(at: Vector, a: u32, alpha: f32) -> Vec<MeshVertex> {
    let n = (a + 1) as f32;
    let radius = 1.15 * n - 0.009_583_3 * n * (n + 1.0);
    let yaw = 3.0 * n;
    let tint = [1.0, 1.0, 1.0, alpha.trunc().clamp(0.0, 255.0) / 255.0];
    let point = |theta: f32, phi: f32, u: f32, v: f32| {
        let (theta, phi) = (lookup(theta), lookup(phi));
        let ((sin_t, cos_t), (sin_p, cos_p)) = (trig(theta), trig(phi));
        MeshVertex { position: [at[0] + radius * cos_p * sin_t, at[1] + radius * sin_p, at[2] + radius * cos_p * cos_t], uv: [u, v], tint }
    };
    let mut vertices = Vec::new();
    for row in 0..5 {
        let phi = -90.0 + 36.0 * row as f32;
        let v = 0.2 * row as f32;
        for column in 0..10 {
            let theta = 36.0 * column as f32 + yaw;
            let u = 0.1 * column as f32;
            let [p0, p1, p2, p3] = [
                point(theta, phi, u, v),
                point(theta, phi + 36.0, u, v + 0.2),
                point(theta + 36.0, phi, u + 0.1, v),
                point(theta + 36.0, phi + 36.0, u + 0.1, v + 0.2),
            ];
            vertices.extend([p0, p1, p2, p3, p2, p1]);
        }
    }
    vertices
}

/// DEX AGI UP's streak made at `seed.born`, at age `a`: two crossed vertical blades standing on the ground and rising by their
/// speed a tick, centred on the streak's centre.
fn rising_blades(at: Vector, seed: &Seed, a: u32) -> Vec<MeshVertex> {
    let n = (a + 1) as f32;
    let [d, speed, half, _] = seed.r;
    let (sin, cos) = trig(seed.y);
    let alpha = if a < 20 {
        10.0 * n
    } else if a <= 29 {
        200.0
    } else {
        (200.0 - 10.0 * (a - 29) as f32).max(0.0)
    };
    let tint = [1.0, 1.0, 1.0, alpha.trunc().clamp(0.0, 200.0) / 255.0];
    let centre = [at[0] + d * sin, at[1] + speed * n, at[2] + d * cos];
    let w = 0.18;
    let point = |across: [f32; 3], side: f32, top: bool| {
        let uv = [if top { 0.0 } else { 1.0 }, if side < 0.0 { 1.0 } else { 0.0 }];
        MeshVertex { position: [centre[0] + across[0] * side * w, centre[1] + if top { half } else { -half }, centre[2] + across[2] * side * w], uv, tint }
    };
    [[1.0, 0.0, 0.0], [0.0, 0.0, 1.0]]
        .into_iter()
        .flat_map(|across| {
            let [a, b, c, d] = [point(across, -1.0, true), point(across, 1.0, true), point(across, 1.0, false), point(across, -1.0, false)];
            [a, b, c, a, c, d]
        })
        .collect()
}

/// Decrease AGI's streak made at `seed.born`, at age `a`: two crossed vertical blades, falling.
fn blades(at: Vector, seed: &Seed, a: u32) -> Vec<MeshVertex> {
    let n = (a + 1) as f32;
    let [d, half, _, _] = seed.r;
    let (sin, cos) = trig(seed.y);
    let alpha = if a < 20 {
        10.0 * n
    } else if a <= 29 {
        200.0
    } else {
        200.0 - 10.0 * (a - 29) as f32
    };
    let tint = [1.0, 1.0, 1.0, alpha.trunc().clamp(0.0, 200.0) / 255.0];
    let fall = 0.0075 * n * (n + 1.0);
    let start = [at[0] + d * sin, at[1] + 20.0 - fall * 0.999_85, at[2] + d * cos + fall * 0.017_452];
    // A local point (x, y, z): x along world x, y mostly along −z, z mostly down (the blades' length).
    let point = |x: f32, y: f32, z: f32, uv: [f32; 2]| MeshVertex {
        position: [start[0] + x, start[1] + y * -0.017_452 + z * -0.999_85, start[2] + y * -0.999_85 + z * 0.017_452],
        uv,
        tint,
    };
    let w = 0.18;
    let blade = |across: fn(f32) -> (f32, f32)| {
        let corner = |side: f32, end: f32| {
            let (x, y) = across(side * w);
            point(x, y, end * half, [if end > 0.0 { 0.0 } else { 1.0 }, if side < 0.0 { 1.0 } else { 0.0 }])
        };
        let [a, b, c, d] = [corner(-1.0, 1.0), corner(1.0, 1.0), corner(1.0, -1.0), corner(-1.0, -1.0)];
        [a, b, c, a, c, d]
    };
    [blade(|width| (width, 0.0)), blade(|width| (0.0, width))].concat()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn near(got: f32, want: f32) -> bool {
        (got - want).abs() < 0.01
    }

    fn effect_at(id: u32, rand: impl FnMut(u32) -> u32, min: bool) -> (First, Instant) {
        let start = Instant::now();
        let mut first = First::default();
        first.play(id, None, [0.0; 3], start, min, rand);
        (first, start)
    }

    fn tick(start: Instant, e: u32) -> Instant {
        start + Duration::from_millis(16 * u64::from(e))
    }

    #[test]
    fn the_small_heal_glows_and_sparkles() {
        let (first, start) = effect_at(SMALL_HEAL, |_| 0, false);
        // 11 sparkles, every 9 ticks to 90.
        assert_eq!(first.list[0].seeds.iter().map(|seed| seed.born).collect::<Vec<_>>(), (0..=90).step_by(9).collect::<Vec<_>>());
        // The column: 3, 90 at 29 and 79, 85.5 at 80, 4.5 at 98, 99 draws.
        let alpha = |e: u32| first.meshes(tick(start, e), |_| Some([0.0; 3])).first().map(|(_, mesh)| mesh[0].tint[3] * 255.0);
        assert!(near(alpha(0).unwrap(), 3.0) && near(alpha(29).unwrap(), 90.0) && near(alpha(79).unwrap(), 90.0) && near(alpha(80).unwrap(), 85.0));
        assert!(near(alpha(98).unwrap(), 4.0) && alpha(99).is_none(), "99 draws: ages 0…98");
        // The sparkle rises 1.18 at a = 0, 24.0 at 23, 39.6 at 49; alpha 254 to 33, 238.1 at 34, 0 at 49.
        let particles = |e: u32| first.particles(tick(start, e), |_| Some([0.0; 3]));
        assert!(near(particles(0)[0].at[1], 1.18) && near(particles(23)[0].at[1], 24.0));
        assert!(near(particles(49)[0].at[1], 39.6));
        assert_eq!([0, 33, 34, 49].map(|a| heal_sparkle_alpha(0, a).trunc()), [254.0, 254.0, 238.0, 0.0]);
        // Fades from the start when it lives past the effect: e = 54, 63, 72, 90.
        assert!(near(heal_sparkle_alpha(54, 0), 248.36) && near(heal_sparkle_alpha(54, 8), 203.2) && near(heal_sparkle_alpha(54, 33), 62.09));
        assert!(near(heal_sparkle_alpha(54, 34), 58.21) && near(heal_sparkle_alpha(54, 44), 19.4));
        assert!(near(heal_sparkle_alpha(63, 0), 246.94) && near(heal_sparkle_alpha(72, 0), 244.59) && near(heal_sparkle_alpha(72, 26), 0.0));
        assert!(near(heal_sparkle_alpha(90, 0), 225.78) && near(heal_sparkle_alpha(90, 8), 0.0));
        // Not under /mineffect.
        assert!(effect_at(SMALL_HEAL, |_| 0, true).0.list[0].seeds.is_empty());
        // The sparkle is placed 6 round from the feet at its yaw.
        let at = particles(0)[0].at;
        assert!(near(f32::hypot(at[0], at[2]), 6.0));
    }

    #[test]
    fn endure_closes_a_crest_and_streaks() {
        let (first, start) = effect_at(ENDURE_FX, |_| 0, false);
        assert_eq!(first.list[0].seeds.len(), 41);
        assert_eq!(first.list[0].seeds.last().unwrap().born, 40);
        let crest =
            |e: u32| first.shapes(tick(start, e)).iter().find(|(path, ..)| *path == ENDURE).map(|(_, _, corners)| (corners[2].0[0], corners[0].2[3] * 255.0));
        // Half-size 145.1, 140.3 … 40 from a = 44; alpha 12.7, 254 at 19, 254 at 53, 244.2 at 54, 9.8 at 78; 79 draws.
        assert!(near(crest(0).unwrap().0, 145.11) && near(crest(1).unwrap().0, 140.33) && near(crest(44).unwrap().0, 40.0) && near(crest(45).unwrap().0, 40.0));
        assert!(near(crest(0).unwrap().1, 12.0) && near(crest(19).unwrap().1, 254.0) && near(crest(53).unwrap().1, 254.0) && near(crest(54).unwrap().1, 244.0));
        assert!(near(crest(78).unwrap().1, 9.0) && crest(79).is_none());
        // A streak made at e = 0: radius d − 4n + 0.05n(n+1) with d = 100: 96.1 at a = 0, 22 at a = 39; alpha 254 at 19 and 26, 234 at 27.
        let streak = |e: u32| first.shapes(tick(start, e)).into_iter().filter(|(path, ..)| *path == ALPHA_DOWN).map(|(_, _, corners)| corners).next();
        assert!(streak(0).is_some() && streak(39).is_some());
        let alpha = |e: u32| streak(e).map(|corners| corners[0].2[3] * 255.0).unwrap();
        assert!(near(alpha(19), 254.0) && near(alpha(26), 254.0) && near(alpha(27), 234.0) && near(alpha(39), 0.0));
    }

    #[test]
    fn magnum_break_draws_two_rings_and_a_dome() {
        let (first, start) = effect_at(MAGNUM_BREAK, |_| 0, false);
        // The alpha 16.93 a tick to 254 at a = 14, 237.1 at 15, 16.9 at 28; 29 draws.
        let close = |got: f32, want: f32| (got - want).abs() < 0.06;
        assert!(close(magnum_alpha(0), 16.93) && close(magnum_alpha(14), 254.0) && close(magnum_alpha(15), 237.07) && close(magnum_alpha(28), 16.98));
        assert!(first.top_meshes(tick(start, 28)).len() == 2 && first.top_meshes(tick(start, 29)).is_empty());
        // The ring: R 1.721 at a = 0, 11.4 at 6, 25.29 at 16, 38.06 at 28; a disc to 6, 12 wide from 7.
        let outer = |a: u32| {
            let mesh = ring([0.0; 3], a, 255.0);
            f32::hypot(mesh[1].position[0], mesh[1].position[2])
        };
        assert!(near(outer(0), 1.721) && near(outer(6), 11.433) && near(outer(16), 25.29) && near(outer(28), 38.06));
        let inner = |a: u32| f32::hypot(ring([0.0; 3], a, 255.0)[0].position[0], ring([0.0; 3], a, 255.0)[0].position[2]);
        assert!(inner(6) < 0.01 && near(inner(7), outer(7) - 12.0));
        // The ring twice, depth test off (top meshes); the dome depth-tested: radius 1.131 at a = 0, 25.01 at 28; 5 × 10 cells of 6 vertices.
        let mesh = dome([0.0; 3], 0, 255.0);
        assert_eq!(mesh.len(), 5 * 10 * 6);
        let radius = |a: u32| dome([0.0; 3], a, 255.0).iter().map(|vertex| (vertex.position.iter().map(|v| v * v).sum::<f32>()).sqrt()).fold(0.0, f32::max);
        assert!(near(radius(0), 1.131) && near(radius(28), 25.01), "{} {}", radius(0), radius(28));
        // The bottom pole is a 1° hole: the lowest point is at −89.
        let lowest = dome([0.0; 3], 28, 255.0).iter().map(|vertex| vertex.position[1]).fold(f32::MAX, f32::min);
        assert!(near(lowest, -25.01 * 89.0_f32.to_radians().sin()));
        // It stays where the caster stood: placed from `at`, not following.
        assert!(!first.list[0].follows);
        assert_eq!(first.meshes(tick(start, 3), |_| Some([100.0, 0.0, 0.0])).len(), 1);
        assert!(first.meshes(tick(start, 3), |_| Some([100.0, 0.0, 0.0]))[0].1.iter().all(|vertex| vertex.position[0].abs() < 30.0));
    }

    #[test]
    fn steal_throws_ten_sparks() {
        let (mut first, start) = effect_at(STEAL, |_| 0, false);
        assert_eq!(first.list[0].sparks.len(), 10);
        // With every draw 0: y 0, p = −50 + 1, d = 4, v0 = 0.5, u0 = 0.6, S = 1.0.
        let spark = first.list[0].sparks[0];
        assert!(near(spark.run, 0.5) && near(spark.rise, 0.6) && near(spark.scale, 1.0));
        first.advance(tick(start, 24), |_, _| -1000.0);
        let spark = first.list[0].sparks[0];
        let n = 25.0;
        let rise = 0.6 * n * (1.0 - (n + 1.0) / 50.0);
        assert!(near(spark.at[1], 10.0 + 4.0 * (-49.0_f32).to_radians().sin() + rise), "{:?}", spark.at);
        // The horizontal distance v0·(n − n(n+1)/200) along the yaw.
        let run = 0.5 * (n - n * (n + 1.0) / 200.0);
        assert!(near(spark.at[2], 4.0 * (-49.0_f32).to_radians().cos() + run), "{:?}", spark.at);
        // Scale S(1 − n/50); alpha 254 to 33, 254 − 15.875 (a − 33) after; drawn ages 0…48.
        first.advance(tick(start, 48), |_, _| -1000.0);
        let at = |e: u32| first.particles(tick(start, e), |_| None);
        assert_eq!(at(48).len(), 10);
        assert!(near(at(48)[0].scale, 1.0 - 49.0 / 50.0) && near(at(48)[0].alpha, 254.0 - 15.875 * 15.0));
        assert!(near(at(33)[0].alpha, 254.0) && near(at(34)[0].alpha, 238.125));
        assert!(at(49).is_empty());
        // A spark moving down below the ground bounces (its speed divided by −1.5).
        let (mut bounce, start) = effect_at(STEAL, |_| 0, false);
        bounce.list[0].sparks[0].rise = -1.0;
        bounce.list[0].sparks[0].rise_fall = 0.0;
        bounce.advance(tick(start, 0), |_, _| 1000.0);
        assert!(near(bounce.list[0].sparks[0].rise, 1.0 / 1.5));
    }

    #[test]
    fn detoxify_pops_sparkles_every_80_ms() {
        let (first, start) = effect_at(DETOXIFY, |_| 0, false);
        assert_eq!(first.list[0].seeds.iter().map(|seed| seed.born).collect::<Vec<_>>(), (0..=60).step_by(5).collect::<Vec<_>>());
        assert_eq!(effect_at(DETOXIFY, |_| 0, true).0.list[0].seeds.iter().map(|seed| seed.born).collect::<Vec<_>>(), [0, 15, 30, 45, 60]);
        // The rise at its end: v0 (n − n(n+1)/160) with n = 40, v0 0.3: 8.9; alpha 25.4 a tick to 254 at a = 9, 254 to 31, 222.25 at 32, 0 at 39.
        let particle = |e: u32| first.particles(tick(start, e), |_| None)[0];
        assert!(near(particle(39).at[1], 0.3 * (40.0 - 40.0 * 41.0 / 160.0)), "{}", particle(39).at[1]);
        assert!(near(particle(9).alpha, 254.0) && near(particle(31).alpha, 254.0) && near(particle(32).alpha, 222.25) && near(particle(39).alpha, 0.0));
        assert!(near(particle(0).alpha, 25.4) && near(particle(2).spin, 9.0));
        assert!(first.particles(tick(start, 40), |_| None).iter().all(|particle| particle.at[1] > 0.0));
    }

    #[test]
    fn dex_agi_up_raises_its_text_and_streaks() {
        let (first, start) = effect_at(DEX_AGI_UP, |_| 0, false);
        assert_eq!(first.list[0].seeds.iter().map(|seed| seed.born).collect::<Vec<_>>(), (0..=50).step_by(2).collect::<Vec<_>>());
        // The text: y −1.48 at a = 0, −14.01 at 9, −52.05 at 49, −61.75 at 82, −59.4 at 98; half-size 40 × 20; 99 draws.
        let text = |e: u32| {
            first
                .shapes(tick(start, e))
                .into_iter()
                .next()
                .map(|(_, _, corners)| ((corners[0].0[1] + corners[2].0[1]) / 2.0, corners[0].2[3] * 255.0, corners[2].0[0]))
        };
        let y = |e: u32| text(e).unwrap().0;
        assert!(
            near(y(0), -1.482) && near(y(9), -14.01) && near(y(49), -52.05) && near(y(82), -61.75) && near(y(98), -59.4),
            "{} {} {} {} {}",
            y(0),
            y(9),
            y(49),
            y(82),
            y(98)
        );
        assert!(near(text(0).unwrap().2, 40.0) && text(99).is_none());
        // The alpha: 13, 26, 40 … 199 at a = 14, 200 at 15 to 84, 186, 173 … 13 at 98.
        let alpha = |e: u32| text(e).unwrap().1.floor();
        assert_eq!([0, 1, 2, 14, 15, 84, 85, 86, 98].map(alpha), [13.0, 26.0, 40.0, 199.0, 200.0, 200.0, 186.0, 173.0, 13.0]);
        // Streaks at e = 0, 2 … 50 (26), none at 52; alpha 10 a tick to 200 at 19, held to 29, then 190 … 0 at 49; it rises by its speed.
        assert_eq!(first.list[0].seeds.len(), 26);
        let seed = Seed { born: 0, y: 0.0, r: [2.0, 0.5, 3.0, 0.0] };
        let streak_alpha = |a: u32| rising_blades([0.0; 3], &seed, a)[0].tint[3] * 255.0;
        assert!(near(streak_alpha(19), 200.0) && near(streak_alpha(29), 200.0) && near(streak_alpha(30), 190.0) && near(streak_alpha(49), 0.0));
        let middle = |a: u32| rising_blades([0.0; 3], &seed, a).iter().map(|vertex| vertex.position[1]).sum::<f32>() / 12.0;
        assert!(near(middle(49), 0.5 * 50.0));
        // The blades are the same whatever y is.
        let turned = Seed { y: 120.0, ..seed };
        assert!(rising_blades([0.0; 3], &seed, 5).iter().zip(rising_blades([0.0; 3], &turned, 5)).all(|(p, q)| near(p.position[1], q.position[1])));
    }

    #[test]
    fn holy_water_shows_frames_1_to_41_for_82_ticks() {
        let (first, start) = effect_at(HOLY_WATER, |_| 0, false);
        let at = |e: u32| first.holy_waters(tick(start, e), |_| Some([1.0, 2.0, 3.0]));
        // Frame 1 at ages 0 and 1, frame 41 at 80 and 81, over at 82; 20 over the feet.
        assert_eq!([0, 1, 2, 80, 81].map(|e| at(e)[0].1), [1, 1, 2, 41, 41]);
        assert!(at(82).is_empty() && at(99).is_empty());
        assert_eq!(at(0)[0].0, [1.0, 22.0, 3.0]);
        assert_eq!((0..100).filter(|&e| !at(e).is_empty()).count(), 82);
    }

    #[test]
    fn decrease_agi_lowers_a_sign_and_drops_streaks() {
        let (first, start) = effect_at(DECREASE_AGI, |_| 0, false);
        assert_eq!(first.list[0].seeds.len(), 26);
        assert_eq!(first.list[0].seeds.last().unwrap().born, 50);
        let sign = |e: u32| {
            first.shapes(tick(start, e)).into_iter().next().map(|(_, _, corners)| ((corners[0].0[1] + corners[2].0[1]) / 2.0, corners[0].2[3] * 255.0))
        };
        // Centre y −79.01 at a = 0, −30.5 at 98 (80 px up, sinking 49.5); alpha 13.3 at 0, 200 at 14, 200 at 84, 13.3 at 98.
        assert!(near(sign(0).unwrap().0, -79.01) && near(sign(98).unwrap().0, -80.0 + 99.0 - 0.005 * 99.0 * 100.0));
        assert!(near(sign(0).unwrap().1, 13.0) && near(sign(14).unwrap().1, 200.0) && near(sign(84).unwrap().1, 200.0) && near(sign(98).unwrap().1, 13.0));
        assert!(sign(99).is_none());
        // A streak falls 0.0075 n (n + 1) · 0.99985 after n ticks: 19.1 at n = 50.
        let seed = Seed { born: 0, y: 0.0, r: [2.0, 3.0, 0.0, 0.0] };
        let top = |a: u32| blades([0.0; 3], &seed, a).iter().map(|vertex| vertex.position[1]).fold(f32::MIN, f32::max);
        assert!(near(top(49), 20.0 + 3.0 * 0.99985 - 0.0075 * 50.0 * 51.0 * 0.99985), "{}", top(49));
        // The blades are the same whatever y is (only the start moves).
        let turned_seed = Seed { y: 90.0, ..seed };
        let a = blades([0.0; 3], &seed, 5);
        let b = blades([0.0; 3], &turned_seed, 5);
        assert!(a.iter().zip(&b).all(|(p, q)| near(p.position[1], q.position[1])));
        // Alpha 10 a tick to 200 at a = 19, held to 29, 0 at 49.
        let alpha = |a: u32| blades([0.0; 3], &seed, a)[0].tint[3] * 255.0;
        assert!(near(alpha(0), 10.0) && near(alpha(19), 200.0) && near(alpha(29), 200.0) && near(alpha(49), 0.0));
    }
}
