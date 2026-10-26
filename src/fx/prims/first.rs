//! Six first-job skill effects, built tick by tick from a few exact parts: the small heal's glowing column and
//! sparkles (7), Endure's crest and closing streaks (11), Magnum Break's two rings and turning dome (17), Steal's ten
//! sparks (18), Detoxify's rising sparkles (21) and Decrease AGI's sinking sign and falling streaks (38); Tidal Weapon's
//! blue rings round an icy dome (942) is Magnum Break's.
//!
//! An effect has an age e (0 at its launch) and a length N; a part made at age e has its own age a. A part with life L
//! made at e is drawn min(L, N − e − 1) times: the effect's end cuts it. Alphas are cut to whole numbers when drawn.

use std::time::Instant;

use super::burst;
use super::passes;
use crate::world::{MeshVertex, Vector};

pub const ALPHA_DOWN: &str = "data\\texture\\effect\\alpha_down.tga";
pub const ENDURE: &str = "data\\texture\\effect\\endure.tga";
pub const RING_YELLOW: &str = "data\\texture\\effect\\ring_yellow.tga";
pub const RING_BLUE: &str = "data\\texture\\effect\\ring_blue.tga";
/// Tidal Weapon's icy dome.
pub const ICE: &str = crate::prims::ICE_TEXTURE;
pub const DOME: &str = "data\\texture\\effect\\대폭발.tga";
pub const SLOW: &str = "data\\texture\\effect\\slow.bmp";
pub const STREAK: &str = "data\\texture\\effect\\ac_center2.tga";
pub const STAR: &str = "data\\sprite\\이팩트\\particle1";
/// Enchant Poison's purple sparkle.
pub const POISON_STAR: &str = "data\\sprite\\이팩트\\particle3";
/// Detecting's soft ring.
pub const FASHASHA: &str = "data\\texture\\effect\\fashasha.tga";
/// Slow Cast's clock face.
pub const CLOCK: &str = "data\\texture\\effect\\blast_mine##clock.bmp";
pub const GOLD_STAR: &str = "data\\sprite\\이팩트\\particle7";
pub const CYAN_STAR: &str = "data\\sprite\\이팩트\\particle2";
pub const PIKAPIKA: &str = "data\\texture\\effect\\pikapika2.bmp";
pub const THUNDER_CENTER: &str = "data\\texture\\effect\\thunder_center.bmp";
pub const POK1: &str = "data\\texture\\effect\\pok1.tga";
pub const POK3: &str = "data\\texture\\effect\\pok3.tga";
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
/// The 4th-job stat-up signs: DEX AGI UP's lettering and rising streaks, the streaks cyan, the lettering sized by its picture.
pub const C_RATE_UP: u32 = 2288;
pub const P_ATK_UP: u32 = 2294;
pub const RES_UP: u32 = 2296;
pub const S_MATK_UP: u32 = 2298;
pub const P_ATK_S_MATK_UP: u32 = 2300;
pub const CART_REVOLUTION: u32 = 170;
pub const FIRST_AID: u32 = 309;
pub const SPRINKLE_SAND: u32 = 310;
pub const HEAL_UNDEAD: u32 = 320;
pub const ASPERSIO: u32 = 86;
pub const ADRENALINE_RUSH: u32 = 98;
pub const FLASHER_FX: u32 = 99;
pub const REMOVE_TRAP: u32 = 100;
pub const DETECTING: u32 = 119;
/// Hovering's and the Side Slides' pale ring: Detecting's, smaller and earlier fading.
pub const SIDE_SLIDE: u32 = 778;
/// Infrared Scan's red ring and Analyze's green one: Detecting's, tinted; the green follows its unit.
pub const RED_RING: u32 = 780;
pub const GREEN_RING: u32 = 786;
/// Deadly Infect's dark ring: Hovering's, in near black on the ground.
pub const DARK_RING: u32 = 812;
/// The monsters' Ignition Break: Detecting's ring in pale orange, following the monster.
pub const IGNITION_RING: u32 = 722;
/// The monsters' Slow Cast: a clock over the target's head.
pub const SLOW_CAST: u32 = 676;
/// The water elemental's Tidal Weapon: Magnum Break's rings in blue, twice, round its dome in ice.
pub const TIDAL_WEAPON: u32 = 942;
/// Slow Cast's clock is drawn this many ticks.
const CLOCK_LIFE: u32 = 242;
pub const ENCHANT_POISON: u32 = 125;
/// Enchant Deadly Poison: the same sparkles round the caster's tick-0 place, 40 of them, those after tick 80 never drawn.
pub const ENCHANT_DEADLY_POISON: u32 = 493;
const EDP_LAST: u32 = 80;
/// Enchant Poison's sparkles made after this tick are never drawn.
const POISON_LAST: u32 = 210;

/// A stat-up sign's picture.
pub fn sign_picture(effect: u32) -> Option<&'static str> {
    Some(match effect {
        C_RATE_UP => "data\\texture\\effect\\status_effect\\C_rate_Up.bmp",
        P_ATK_UP => "data\\texture\\effect\\status_effect\\P_atk_Up.bmp",
        RES_UP => "data\\texture\\effect\\status_effect\\Res_Up.bmp",
        S_MATK_UP => "data\\texture\\effect\\status_effect\\S_matk_Up.bmp",
        P_ATK_S_MATK_UP => "data\\texture\\effect\\status_effect\\P_atk_S_matk_Up.bmp",
        _ => return None,
    })
}

/// The signs' streaks are cyan.
const SIGN_STREAK: [f32; 3] = [0.0, 1.0, 1.0];

pub fn makes(effect: u32) -> bool {
    matches!(
        effect,
        SMALL_HEAL
            | ENDURE_FX
            | MAGNUM_BREAK
            | STEAL
            | DETOXIFY
            | DECREASE_AGI
            | HOLY_WATER
            | DEX_AGI_UP
            | CART_REVOLUTION
            | FIRST_AID
            | SPRINKLE_SAND
            | HEAL_UNDEAD
            | ASPERSIO
            | ADRENALINE_RUSH
            | FLASHER_FX
            | REMOVE_TRAP
            | DETECTING
            | SIDE_SLIDE
            | RED_RING
            | GREEN_RING
            | DARK_RING
            | IGNITION_RING
            | SLOW_CAST
            | ENCHANT_POISON
            | ENCHANT_DEADLY_POISON
            | TIDAL_WEAPON
    )
}

/// The effects' lengths N in ticks.
fn length(effect: u32) -> u32 {
    match effect {
        SMALL_HEAL | DETOXIFY | DECREASE_AGI | HOLY_WATER | DEX_AGI_UP | C_RATE_UP | P_ATK_UP | RES_UP | S_MATK_UP | P_ATK_S_MATK_UP => 100,
        ENDURE_FX => 80,
        MAGNUM_BREAK => 30,
        CART_REVOLUTION => 50,
        FIRST_AID => 299,
        SPRINKLE_SAND => 300,
        HEAL_UNDEAD => 100,
        ASPERSIO => 175,
        ADRENALINE_RUSH => 300,
        FLASHER_FX => 250,
        REMOVE_TRAP => 70,
        DETECTING | SIDE_SLIDE | RED_RING | DARK_RING | ENCHANT_POISON => 250,
        GREEN_RING | IGNITION_RING => 100,
        SLOW_CAST => 300,
        ENCHANT_DEADLY_POISON => 120,
        TIDAL_WEAPON => 100,
        _ => 50,
    }
}

/// A screen quad's corners and a world triangle's corners: where, texture coordinate and colour.
pub type ScreenCorner = ([f32; 2], [f32; 2], [f32; 4]);
pub type ScreenShape = (&'static str, Option<u32>, Vec<ScreenCorner>);
/// A world mesh: its texture and triangles.
pub type Mesh = (&'static str, Vec<MeshVertex>);

/// A camera-facing square: its texture, centre, half-diagonal (world units), spin (degrees), alpha (0 to 255), whether its
/// texture is mirrored, the colour it's multiplied by, and whether it's added on (or blended over).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Square {
    pub texture: &'static str,
    pub at: Vector,
    pub radius: f32,
    pub spin: f32,
    pub alpha: f32,
    pub mirrored: bool,
    pub tint: [u8; 3],
    pub additive: bool,
}

/// A sparkle's alpha at its age `a` (from 1): 0 at 1, +15 at ages 2…10, then −3 a tick.
pub fn sparkle_alpha(a: u32) -> f32 {
    if a <= 1 {
        0.0
    } else if a <= 10 {
        15.0 * (a - 1) as f32
    } else {
        (135.0 - 3.0 * (a - 10) as f32).max(0.0)
    }
}

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
    /// Two more numbers (a speck's second ground coordinate, a sparkle's index).
    more: [f32; 2],
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
    /// A sign's picture size in pixels.
    picture: [f32; 2],
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
        let mut seeds_more: Vec<Seed> = Vec::new();
        let mut seed = |born: u32, y: f32, r: [f32; 4]| seeds.push(Seed { born, y, r, more: [0.0; 2] });
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
            ADRENALINE_RUSH => {
                // Four sparks starting at yaws 0, 90, 180 and 270.
                for yaw in [0.0, 90.0, 180.0, 270.0] {
                    seed(0, yaw, [0.0; 4]);
                }
            }
            FLASHER_FX => seed(0, 0.0, [0.0; 4]),
            REMOVE_TRAP => {
                // 12 sparks at ticks 0 and 7: one draw thrown away, the pull of gravity, the yaw.
                for born in [0, 7] {
                    for _ in 0..12 {
                        let _ = rand(2);
                        let gravity = 0.08 + rand(10) as f32 / 1000.0;
                        let yaw = rand(360) as f32;
                        seed(born, yaw, [gravity, 0.0, 0.0, 0.0]);
                    }
                }
            }
            DETECTING | SIDE_SLIDE | RED_RING | GREEN_RING | DARK_RING | IGNITION_RING | SLOW_CAST => seed(0, 0.0, [0.0; 4]),
            ENCHANT_POISON => {
                // A sparkle every 5 ticks: yaw, distance, speed, size.
                for born in (0..=245).step_by(5) {
                    let y = rand(360) as f32;
                    let d = (2 + rand(6)) as f32;
                    let speed = (30 + rand(30)) as f32 / 100.0;
                    let scale = (40 + rand(50)) as f32 / 100.0;
                    seed(born, y, [d, speed, scale, 0.0]);
                }
            }
            ENCHANT_DEADLY_POISON => {
                // A sparkle every 3 ticks: yaw, distance, speed, size.
                for born in (0..=117).step_by(3) {
                    let y = rand(360) as f32;
                    let d = (2 + rand(6)) as f32;
                    let speed = (30 + rand(50)) as f32 / 100.0;
                    let scale = (20 + rand(20)) as f32 / 100.0;
                    seed(born, y, [d, speed, scale, 0.0]);
                }
            }
            ASPERSIO => {
                // A sparkle every 5 ticks from 65 to 135, each with a bearing.
                for born in (65..=135).step_by(5) {
                    seed(born, rand(360) as f32, [0.0; 4]);
                }
            }
            CART_REVOLUTION => {
                // A ring and a sphere at ticks 7 and 20.
                seed(7, 0.0, [0.0; 4]);
                seed(20, 0.0, [0.0; 4]);
            }
            FIRST_AID => {
                // One glow: its spin, then where in its wobble it starts.
                let spin = rand(360) as f32;
                let wobble = rand(360) as f32;
                seed(0, spin, [wobble, 0.0, 0.0, 0.0]);
            }
            SPRINKLE_SAND => {
                // 80 specks: delay, half-diagonal, place round the feet, speed, rise; each then a direction at its first step
                // (in tick order, then the order made).
                let mut specks = Vec::new();
                for _ in 0..80 {
                    let delay = rand(10) as i32 - 10;
                    let radius = 0.1 + 0.03 * rand(16) as f32;
                    let x = at[0] + rand(9) as f32 - 4.0;
                    let z = at[2] + rand(9) as f32 - 4.0;
                    let speed = 0.01 * rand(10) as f32;
                    let rise = 0.2 + 0.01 * rand(40) as f32;
                    specks.push(((-delay) as u32, radius, x, z, speed, rise));
                }
                let mut order: Vec<usize> = (0..specks.len()).collect();
                order.sort_by_key(|&index| (specks[index].0, index));
                let mut directions = vec![0.0; specks.len()];
                for index in order {
                    directions[index] = rand(360) as f32;
                }
                for (index, (born, radius, x, z, speed, rise)) in specks.into_iter().enumerate() {
                    seeds_more.push(Seed { born, y: directions[index], r: [radius, x, speed, rise], more: [z, 0.0] });
                }
            }
            HEAL_UNDEAD => {
                // Four sets of four sparkles at ticks 6, 12, 18, 24: half-diagonal, place round the feet, rise.
                for born in [6, 12, 18, 24] {
                    for k in 0..4 {
                        let radius = 2.0 + 0.1 * rand(10) as f32;
                        let x = rand(13) as f32 - 6.0;
                        let z = rand(13) as f32 - 6.0;
                        let rise = 0.4 + 0.01 * rand(40) as f32;
                        seeds_more.push(Seed { born, y: 0.0, r: [radius, x, z, rise], more: [k as f32, 0.0] });
                    }
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
        seeds.extend(seeds_more);
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
        self.list.push(Effect {
            id,
            start: now,
            unit,
            follows: !matches!(
                id,
                MAGNUM_BREAK
                    | STEAL
                    | DETOXIFY
                    | CART_REVOLUTION
                    | FIRST_AID
                    | SPRINKLE_SAND
                    | HEAL_UNDEAD
                    | ASPERSIO
                    | ADRENALINE_RUSH
                    | FLASHER_FX
                    | REMOVE_TRAP
                    | DETECTING
                    | SIDE_SLIDE
                    | RED_RING
                    | DARK_RING
                    | ENCHANT_POISON
                    | ENCHANT_DEADLY_POISON
                    | TIDAL_WEAPON
            ),
            at,
            seeds,
            sparks,
            picture: [0.0; 2],
        });
    }

    /// Stat-up sign `id` from `now` on `unit`, following it, its picture `picture` pixels: DEX AGI UP's lettering and rising
    /// streaks (26, made at ticks 0, 2 … 50) with the streak's draws in the order half-length, yaw, distance, speed. Every
    /// sign made stays; a repeat makes another.
    pub fn sign(&mut self, id: u32, unit: Option<u32>, picture: [f32; 2], now: Instant, mut rand: impl FnMut(u32) -> u32) {
        let seeds = (0..=50)
            .step_by(2)
            .map(|born| {
                let half = (rand(60) + 30) as f32 / 10.0;
                let y = rand(360) as f32;
                let d = (2 + rand(7)) as f32;
                let speed = (rand(50) + 20) as f32 / 100.0;
                Seed { born, y, r: [d, speed, half, 0.0], more: [0.0; 2] }
            })
            .collect();
        self.list.push(Effect { id, start: now, unit, follows: true, at: [0.0; 3], seeds, sparks: Vec::new(), picture });
        self.list.retain(|effect| passes(effect.start, now) < length(effect.id));
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
                id if sign_picture(id).is_some() => {
                    if let (Some(a), Some(picture)) = (Self::age(effect, now, 0, 100), sign_picture(id)) {
                        let k = (a + 1) as f32;
                        let rise = 1.5 * k - 0.009 * k * (k + 1.0);
                        let alpha = super::burst::alpha(a, 0.0, 200.0 / 15.0, 200.0, 85, 100);
                        shapes.push((picture, effect.unit, square([0.0, -rise], [effect.picture[0] * 0.625, effect.picture[1] * 0.625], alpha)));
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
                        meshes.push((DOME, dome(at, a, magnum_alpha(a), 1.15, 0.009_583_3)));
                    }
                }
                TIDAL_WEAPON => {
                    if let Some(a) = Self::age(effect, now, 0, 30) {
                        meshes.push((ICE, dome(at, a, tidal_alpha(a, 30), 1.15, 0.009_583_3)));
                    }
                }
                CART_REVOLUTION => {
                    for seed in &effect.seeds {
                        if let Some(a) = Self::age(effect, now, seed.born, 20) {
                            meshes.push((DOME, dome(effect.at, a, cart_dome_alpha(a), 1.35, 0.016_875)));
                        }
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
                        meshes.push((STREAK, rising_blades(at, seed, a, [1.0; 3])));
                    }
                }
                id if sign_picture(id).is_some() => {
                    for seed in &effect.seeds {
                        let Some(a) = Self::age(effect, now, seed.born, 50) else { continue };
                        meshes.push((STREAK, rising_blades(at, seed, a, SIGN_STREAK)));
                    }
                }
                DETECTING => {
                    if let Some(a) = Self::age(effect, now, 0, 57) {
                        meshes.push((FASHASHA, ring_square(at, detecting_half(a), burst::alpha(a, 254.0, 0.0, 254.0, 40, 57), [1.0; 3])));
                    }
                }
                DARK_RING => {
                    if let Some(a) = Self::age(effect, now, 0, 57) {
                        meshes.push((FASHASHA, ring_square(at, ring_half(a, 10), burst::alpha(a, 254.0, 0.0, 254.0, 30, 57), [10.0 / 255.0; 3])));
                    }
                }
                SIDE_SLIDE => {
                    if let Some(a) = Self::age(effect, now, 0, 57) {
                        meshes.push((FASHASHA, ring_square(at, ring_half(a, 10), burst::alpha(a, 254.0, 0.0, 254.0, 30, 57), [1.0; 3])));
                    }
                }
                RED_RING | GREEN_RING | IGNITION_RING => {
                    if let Some(a) = Self::age(effect, now, 0, 57) {
                        let rgb = match effect.id {
                            RED_RING => [1.0, 140.0 / 255.0, 140.0 / 255.0],
                            GREEN_RING => [210.0 / 255.0, 250.0 / 255.0, 210.0 / 255.0],
                            _ => [250.0 / 255.0, 210.0 / 255.0, 140.0 / 255.0],
                        };
                        meshes.push((FASHASHA, ring_square(at, detecting_half(a), burst::alpha(a, 254.0, 0.0, 254.0, 40, 57), rgb)));
                    }
                }
                _ => {}
            }
        }
        meshes
    }

    /// Magnum Break's two rings, flat on the ground and drawn with the depth test off, over everything: the same ring twice;
    /// Tidal Weapon's two rings, each twice.
    pub fn top_meshes(&self, now: Instant) -> Vec<Mesh> {
        let mut meshes = Vec::new();
        for effect in self.list.iter().filter(|effect| effect.id == CART_REVOLUTION) {
            for seed in &effect.seeds {
                if let Some(a) = Self::age(effect, now, seed.born, 20) {
                    meshes.push((RING_YELLOW, ring(effect.at, a, cart_ring_alpha(a), 0.021_875, 5.0)));
                }
            }
        }
        for effect in self.list.iter().filter(|effect| effect.id == MAGNUM_BREAK) {
            if let Some(a) = Self::age(effect, now, 0, 30) {
                let ring = ring(effect.at, a, magnum_alpha(a), 0.014_583_3, 12.0);
                meshes.push((RING_YELLOW, ring.clone()));
                meshes.push((RING_YELLOW, ring));
            }
        }
        for effect in self.list.iter().filter(|effect| effect.id == TIDAL_WEAPON) {
            for (life, swell) in [(30, 0.014_583_3), (20, 0.021_875)] {
                if let Some(a) = Self::age(effect, now, 0, life) {
                    let ring = ring(effect.at, a, tidal_alpha(a, life), swell, 12.0);
                    meshes.push((RING_BLUE, ring.clone()));
                    meshes.push((RING_BLUE, ring));
                }
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

    /// The camera-facing squares: First Aid's glow, Sprinkle Sand's specks, the undead heal's sparkles, Slow Cast's clock.
    pub fn squares(&self, now: Instant, position_of: impl Fn(Option<u32>) -> Option<Vector>) -> Vec<Square> {
        let mut squares = Vec::new();
        for effect in &self.list {
            match effect.id {
                SLOW_CAST => {
                    let (Some(a), Some(feet)) = (Self::age(effect, now, 0, CLOCK_LIFE), Self::place(effect, &position_of)) else { continue };
                    let n = a + 1;
                    // Corners at 135, 45, 315, 225 (y up) at uv (0, 1), (1, 1), (1, 0), (0, 0).
                    // ponytail: flip v if the clock comes out upside down.
                    squares.push(Square {
                        texture: CLOCK,
                        at: [feet[0], feet[1] + 30.0, feet[2]],
                        radius: 6.0 * (1.0 + 0.05 * trig((2 * n) as f32).0),
                        spin: 225.0,
                        alpha: slow_cast_alpha(n),
                        mirrored: false,
                        tint: [255; 3],
                        additive: false,
                    });
                }
                FLASHER_FX => {
                    // The flash: a 40 by 40 square (half-diagonal 20·√2, turned 45°) 10 up, at full strength for ages 0…68.
                    if let Some(a) = Self::age(effect, now, 0, 69) {
                        let _ = a;
                        let at = [effect.at[0], effect.at[1] + 10.0, effect.at[2]];
                        squares.push(Square {
                            texture: THUNDER_CENTER,
                            at,
                            radius: 20.0 * std::f32::consts::SQRT_2,
                            spin: 45.0,
                            alpha: 255.0,
                            mirrored: false,
                            tint: [255; 3],
                            additive: true,
                        });
                    }
                }
                FIRST_AID => {
                    for seed in &effect.seeds {
                        let Some(a) = Self::age(effect, now, seed.born, 299) else { continue };
                        let n = (a + 1) as f32;
                        let wobble = (seed.r[0] + 3.0 * n).rem_euclid(360.0);
                        let alpha = if n <= 20.0 {
                            4.0 * n
                        } else if n <= 100.0 {
                            80.0
                        } else {
                            (80.0 - (n - 100.0)).max(0.0)
                        };
                        let radius = 10.0 + 10.0 * trig(wobble.floor()).0 * 0.05;
                        let at = [effect.at[0], effect.at[1] + 12.0, effect.at[2]];
                        squares.push(Square { texture: PIKAPIKA, at, radius, spin: seed.y, alpha, mirrored: false, tint: [0x96, 0x96, 0xFA], additive: true });
                    }
                }
                SPRINKLE_SAND => {
                    for seed in &effect.seeds {
                        let Some(tick) = Self::tick(effect, now) else { continue };
                        let Some(a) = tick.checked_sub(seed.born).map(|since| since + 1).filter(|a| (2..=54).contains(a)) else { continue };
                        let [radius, x, speed, rise] = seed.r;
                        let (sin, cos) = trig(seed.y);
                        let steps = a as f32;
                        let at = [x + speed * cos * steps, effect.at[1] + rise * steps, seed.more[0] + speed * sin * steps];
                        squares.push(Square {
                            texture: THUNDER_CENTER,
                            at,
                            radius,
                            spin: (-5.0 * steps).rem_euclid(360.0),
                            alpha: sparkle_alpha(a),
                            mirrored: true,
                            tint: [0xFA, 0xFA, 0x96],
                            additive: true,
                        });
                    }
                }
                HEAL_UNDEAD => {
                    for seed in &effect.seeds {
                        let Some(tick) = Self::tick(effect, now) else { continue };
                        let Some(a) = tick.checked_sub(seed.born).map(|since| since + 1).filter(|a| (2..=54).contains(a)) else { continue };
                        let [radius, x, z, rise] = seed.r;
                        let k = seed.more[0] as u32;
                        // Element k shows the round glow at ages 35 − 7k and 36 − 7k.
                        let twinkle = a == 35 - 7 * k || a == 36 - 7 * k;
                        let at = [effect.at[0] + x, effect.at[1] + rise * a as f32, effect.at[2] + z];
                        squares.push(Square {
                            texture: if twinkle { POK3 } else { POK1 },
                            at,
                            radius,
                            spin: (-5.0 * a as f32).rem_euclid(360.0),
                            alpha: sparkle_alpha(a),
                            mirrored: true,
                            tint: [0xDC, 0xFA, 0xDC],
                            additive: true,
                        });
                    }
                }
                _ => {}
            }
        }
        squares
    }

    /// The sprite particles: the heal's sparkles, Steal's sparks, Detoxify's sparkles.
    pub fn particles(&self, now: Instant, position_of: impl Fn(Option<u32>) -> Option<Vector>) -> Vec<Particle> {
        let mut particles = Vec::new();
        for effect in &self.list {
            let Some(at) = Self::place(effect, &position_of) else { continue };
            match effect.id {
                ADRENALINE_RUSH => {
                    for seed in &effect.seeds {
                        let Some(a) = Self::age(effect, now, seed.born, 100) else { continue };
                        let n = (a + 1) as f32;
                        let (sin, cos) = trig(seed.y + 0.3 * n + 0.1 * n * (n + 1.0));
                        let place = [at[0] + 7.0 * sin, at[1] + 0.1 * n + 0.001 * n * (n + 1.0), at[2] + 7.0 * cos];
                        let alpha = if a <= 89 { 254.0 } else { (254.0 - 25.4 * (a - 89) as f32).max(0.0) };
                        particles.push(Particle { sprite: STAR, at: place, scale: 1.2, alpha, spin: 0.0 });
                    }
                }
                REMOVE_TRAP => {
                    for seed in &effect.seeds {
                        let Some(a) = Self::age(effect, now, seed.born, 50) else { continue };
                        let n = (a + 1) as f32;
                        let gravity = seed.r[0];
                        let (sin, cos) = trig(seed.y);
                        let place = [at[0] + 0.2 * n * sin, at[1] + 2.0 * n - gravity * n * (n + 1.0) / 2.0, at[2] + 0.2 * n * cos];
                        let alpha = if a <= 34 { 254.0 } else { (254.0 - 254.0 / 15.0 * (a - 34) as f32).max(0.0) };
                        particles.push(Particle { sprite: STAR, at: place, scale: 0.8, alpha, spin: 0.0 });
                    }
                }
                ENCHANT_POISON | ENCHANT_DEADLY_POISON => {
                    let last = if effect.id == ENCHANT_POISON { POISON_LAST } else { EDP_LAST };
                    for seed in effect.seeds.iter().filter(|seed| seed.born <= last) {
                        let Some(a) = Self::age(effect, now, seed.born, 40) else { continue };
                        let [d, speed, scale, _] = seed.r;
                        let (sin, cos) = trig(seed.y);
                        // Each tick the speed drops by an 80th of the first, then it rises.
                        let (mut left, mut height) = (speed, 0.0);
                        for _ in 0..=a {
                            left -= speed / 80.0;
                            height += left;
                        }
                        let place = [at[0] + d * sin, at[1] + height, at[2] + d * cos];
                        particles.push(Particle {
                            sprite: POISON_STAR,
                            at: place,
                            scale,
                            alpha: burst::alpha(a, 0.0, 25.4, 254.0, 32, 40),
                            spin: 3.0 * (a + 1) as f32,
                        });
                    }
                }
                ASPERSIO => {
                    for seed in &effect.seeds {
                        let Some(a) = Self::age(effect, now, seed.born, 40) else { continue };
                        let k = (a + 1) as f32;
                        let (sin, cos) = trig(seed.y);
                        let place = [at[0] + 3.0 * sin, at[1] + 30.0 - 0.19 * k - 0.005 * k * (k + 1.0), at[2] + 3.0 * cos];
                        let alpha = if a <= 29 { 254.0 } else { (254.0 - 25.4 * (a - 29) as f32).max(0.0) };
                        particles.push(Particle { sprite: STAR, at: place, scale: 1.2, alpha, spin: 0.0 });
                    }
                }
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

/// Slow Cast's clock alpha after its tick `n` (from 1): +10 a tick while n < 200, up to 220, then −5 a tick.
pub fn slow_cast_alpha(n: u32) -> f32 {
    if n < 200 { (10 * n).min(220) as f32 } else { 220u32.saturating_sub(5 * (n - 199)) as f32 }
}

/// Cart Revolution's ring alpha at age `a`: 12 a tick to 60, then 4 less a tick.
pub fn cart_ring_alpha(a: u32) -> f32 {
    if a <= 4 { 12.0 * (a + 1) as f32 } else { (60.0 - 4.0 * (a - 4) as f32).max(0.0) }
}

/// Cart Revolution's sphere alpha at age `a`: 240/7 a tick to 171, fading over the rest from age 5.
pub fn cart_dome_alpha(a: u32) -> f32 {
    super::burst::alpha(a, 0.0, 240.0 / 7.0, 171.0, 5, 20)
}

/// Magnum Break's alpha at age `a`: up to 254 over 15 ticks, then down by the same.
pub fn magnum_alpha(a: u32) -> f32 {
    if a <= 14 { (16.93 * (a + 1) as f32).min(254.0) } else { (254.0 - 16.93 * (a - 14) as f32).max(0.0) }
}

/// Tidal Weapon's alpha at age `a` for a part of life `life`: 16.93 a tick to 169.3 at a = 9, then down to 0 at the
/// last age.
pub fn tidal_alpha(a: u32, life: u32) -> f32 {
    let top = 16.93 * 10.0;
    if a <= 9 { 16.93 * (a + 1) as f32 } else { (top - top / (life - 10) as f32 * (a - 9) as f32).max(0.0) }
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
fn ring(at: Vector, a: u32, alpha: f32, swell: f32, widest: f32) -> Vec<MeshVertex> {
    let n = (a + 1) as f32;
    let outer = 1.75 * n - swell * n * (n + 1.0);
    let inner = outer - outer.min(widest);
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

/// A ring: a square 5 up from the ground, `half` wide, at `alpha` (0 to 255).
fn ring_square(at: Vector, half: f32, alpha: f32, rgb: [f32; 3]) -> Vec<MeshVertex> {
    let tint = [rgb[0], rgb[1], rgb[2], alpha / 255.0];
    let corner = |across: f32, along: f32, uv| MeshVertex { position: [at[0] + across * half, at[1] + 5.0, at[2] + along * half], uv, tint };
    let [a, b, c, d] = [corner(1.0, 1.0, [0.0, 0.0]), corner(1.0, -1.0, [1.0, 0.0]), corner(-1.0, 1.0, [0.0, 1.0]), corner(-1.0, -1.0, [1.0, 1.0])];
    vec![a, b, c, d, c, b]
}

/// Detecting's half-width at age `a`: each tick the rate gains a 14th of 1.5 (from 1.5), then the width grows by it; from
/// age 17 the rate is 0.1 and gains a 30th of that.
fn detecting_half(a: u32) -> f32 {
    ring_half(a, 17)
}

/// The same with the slower rate from age `slow`.
fn ring_half(a: u32, slow: u32) -> f32 {
    let (mut half, mut rate, mut gain) = (0.0_f32, 1.5_f32, 1.5_f32 / 14.0);
    for age in 0..=a {
        if age == slow {
            (rate, gain) = (0.1, 0.1 / 30.0);
        }
        rate += gain;
        half += rate;
    }
    half
}

/// Magnum Break's dome at age `a`: a sphere of radius 1.15 n − 0.0095833 n (n + 1) centred on the feet, turning 3° a tick, 5
/// rows of 10 cells; a negative angle is used as that angle + 1 (so the bottom pole is a 1° hole).
fn dome(at: Vector, a: u32, alpha: f32, grow: f32, slow: f32) -> Vec<MeshVertex> {
    let n = (a + 1) as f32;
    let radius = grow * n - slow * n * (n + 1.0);
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
fn rising_blades(at: Vector, seed: &Seed, a: u32, colour: [f32; 3]) -> Vec<MeshVertex> {
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
    let tint = [colour[0], colour[1], colour[2], alpha.trunc().clamp(0.0, 200.0) / 255.0];
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
            let mesh = ring([0.0; 3], a, 255.0, 0.014_583_3, 12.0);
            f32::hypot(mesh[1].position[0], mesh[1].position[2])
        };
        assert!(near(outer(0), 1.721) && near(outer(6), 11.433) && near(outer(16), 25.29) && near(outer(28), 38.06));
        let inner = |a: u32| f32::hypot(ring([0.0; 3], a, 255.0, 0.014_583_3, 12.0)[0].position[0], ring([0.0; 3], a, 255.0, 0.014_583_3, 12.0)[0].position[2]);
        assert!(inner(6) < 0.01 && near(inner(7), outer(7) - 12.0));
        // The ring twice, depth test off (top meshes); the dome depth-tested: radius 1.131 at a = 0, 25.01 at 28; 5 × 10 cells of 6 vertices.
        let mesh = dome([0.0; 3], 0, 255.0, 1.15, 0.009_583_3);
        assert_eq!(mesh.len(), 5 * 10 * 6);
        let radius = |a: u32| {
            dome([0.0; 3], a, 255.0, 1.15, 0.009_583_3).iter().map(|vertex| (vertex.position.iter().map(|v| v * v).sum::<f32>()).sqrt()).fold(0.0, f32::max)
        };
        assert!(near(radius(0), 1.131) && near(radius(28), 25.01), "{} {}", radius(0), radius(28));
        // The bottom pole is a 1° hole: the lowest point is at −89.
        let lowest = dome([0.0; 3], 28, 255.0, 1.15, 0.009_583_3).iter().map(|vertex| vertex.position[1]).fold(f32::MAX, f32::min);
        assert!(near(lowest, -25.01 * 89.0_f32.to_radians().sin()));
        // It stays where the caster stood: placed from `at`, not following.
        assert!(!first.list[0].follows);
        assert_eq!(first.meshes(tick(start, 3), |_| Some([100.0, 0.0, 0.0])).len(), 1);
        assert!(first.meshes(tick(start, 3), |_| Some([100.0, 0.0, 0.0]))[0].1.iter().all(|vertex| vertex.position[0].abs() < 30.0));
    }

    #[test]
    fn tidal_weapon_draws_two_blue_rings_round_an_icy_dome() {
        let (first, start) = effect_at(TIDAL_WEAPON, |_| 0, false);
        assert!(makes(TIDAL_WEAPON) && !first.list[0].follows);
        // The alpha: 16 at a = 0, 169 at 9, then the ring A 160, 118, 0 at 10, 15, 29, and C 152, 67, 0 at 10, 15, 19.
        let cut = |a: u32, life: u32| tidal_alpha(a, life).trunc();
        assert_eq!([0, 9, 10, 15, 29].map(|a| cut(a, 30)), [16.0, 169.0, 160.0, 118.0, 0.0]);
        assert_eq!([10, 12, 15, 19].map(|a| cut(a, 20)), [152.0, 118.0, 67.0, 0.0]);
        assert_eq!(cut(12, 30), 143.0);
        // The radii: A 1.72, 14.44, 28.87, 38.94 at 0, 8, 19, 29; B 1.13, 9.49, 18.97, 25.59; C 1.71, 13.78, 25.81.
        let outer = |a: u32, swell: f32| {
            let mesh = ring([0.0; 3], a, 255.0, swell, 12.0);
            f32::hypot(mesh[1].position[0], mesh[1].position[2])
        };
        let close = |got: f32, want: f32| (got - want).abs() < 0.01;
        assert!([(0, 1.72), (8, 14.44), (19, 28.87), (29, 38.94)].iter().all(|&(a, want)| close(outer(a, 0.014_583_3), want)));
        assert!([(0, 1.71), (8, 13.78), (19, 25.81)].iter().all(|&(a, want)| close(outer(a, 0.021_875), want)));
        let radius = |a: u32| {
            dome([0.0; 3], a, 255.0, 1.15, 0.009_583_3).iter().map(|vertex| (vertex.position.iter().map(|v| v * v).sum::<f32>()).sqrt()).fold(0.0, f32::max)
        };
        assert!([(0, 1.13), (8, 9.49), (19, 18.97), (29, 25.59)].iter().all(|&(a, want)| close(radius(a), want)));
        // The inner radius: a full disc while the outer is under 12, then 12 wide.
        let inner = |a: u32| {
            let mesh = ring([0.0; 3], a, 255.0, 0.014_583_3, 12.0);
            f32::hypot(mesh[0].position[0], mesh[0].position[2])
        };
        assert!(inner(5) < 0.01 && close(inner(19), outer(19, 0.014_583_3) - 12.0));
        // A and C twice each, over everything, to a = 19; A alone (twice) to 29; nothing after. The dome depth-tested to 29.
        let rings = |e: u32| first.top_meshes(tick(start, e)).into_iter().map(|(path, _)| path).collect::<Vec<_>>();
        assert_eq!(rings(19), [RING_BLUE; 4]);
        assert_eq!((rings(20).len(), rings(29).len(), rings(30).len()), (2, 2, 0));
        let domes = |e: u32| first.meshes(tick(start, e), |_| Some([100.0, 0.0, 0.0]));
        assert!(domes(29).len() == 1 && domes(29)[0].0 == ICE && domes(30).is_empty());
        // It stays at the target's feet at tick 0.
        assert!(domes(3)[0].1.iter().all(|vertex| vertex.position[0].abs() < 30.0));
        // The dome turns 3 (a + 1)°: yaw 3 at a = 0, 30 at 9 (the first column's first point).
        let yaw = |a: u32| {
            let point = dome([0.0; 3], a, 255.0, 1.15, 0.009_583_3)[60].position;
            point[0].atan2(point[2]).to_degrees()
        };
        assert!(close(yaw(0), 3.0) && close(yaw(9), 30.0), "{} {}", yaw(0), yaw(9));
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
    fn adrenaline_rush_spirals_four_sparks() {
        let (first, start) = effect_at(ADRENALINE_RUSH, |_| 0, false);
        assert_eq!(first.list[0].seeds.iter().map(|seed| seed.y).collect::<Vec<_>>(), [0.0, 90.0, 180.0, 270.0]);
        let at = |e: u32| first.particles(tick(start, e), |_| Some([0.0; 3]));
        assert_eq!((at(0).len(), at(99).len(), at(100).len()), (4, 4, 0));
        // 7 out from the feet; up 0.1 n + 0.001 n (n + 1) with n = age + 1: 0.102 at age 0, 20.1 at 99.
        assert!(near(f32::hypot(at(0)[0].at[0], at(0)[0].at[2]), 7.0) && near(at(0)[0].at[1], 0.102) && near(at(99)[0].at[1], 20.1));
        // Alpha 254 to age 89, 228.6 at 90, 25.4 at 98, 0 at 99; scale 1.2.
        let alpha = |e: u32| at(e)[0].alpha;
        assert!(near(alpha(89), 254.0) && near(alpha(90), 228.6) && near(alpha(98), 25.4) && near(alpha(99), 0.0) && at(0)[0].scale == 1.2);
    }

    #[test]
    fn flasher_flashes_a_40_unit_square_10_up() {
        let (first, start) = effect_at(FLASHER_FX, |_| 0, false);
        let at = |e: u32| first.squares(tick(start, e), |_| None);
        // 40 by 40 (half-diagonal 20·√2, turned 45°), 10 up, at full strength for ages 0…68.
        let flash = &at(0)[0];
        assert!(near(flash.radius, 28.284) && flash.spin == 45.0 && flash.at == [0.0, 10.0, 0.0] && flash.texture == THUNDER_CENTER && flash.alpha == 255.0);
        assert!(at(68).len() == 1 && at(69).is_empty());
    }

    #[test]
    fn remove_trap_throws_a_fountain() {
        let (first, start) = effect_at(REMOVE_TRAP, |_| 0, false);
        // 12 sparks at ticks 0 and 7, each living ages 0…49.
        assert_eq!(first.list[0].seeds.len(), 24);
        let at = |e: u32| first.particles(tick(start, e), |_| Some([0.0; 3]));
        assert_eq!((at(0).len(), at(6).len(), at(7).len(), at(49).len(), at(50).len(), at(56).len(), at(57).len()), (12, 12, 24, 24, 12, 12, 0));
        // A pull of 0.08: up 2n − g n (n + 1)/2, n = age + 1: 24.0 at age 23, −2.0 at 49.
        assert!(near(at(23)[0].at[1], 24.0) && near(at(49)[0].at[1], -2.0));
        // A pull of 0.089 (draws 0, 9, 0 a spark) drops it 13.475.
        let mut draws = [0, 9, 0].into_iter().cycle();
        let (heavy, heavy_start) = effect_at(REMOVE_TRAP, |_| draws.next().unwrap(), false);
        let got = heavy.particles(tick(heavy_start, 49), |_| Some([0.0; 3]))[0].at[1];
        assert!(near(got, -13.475), "{got}");
        // Alpha 254 to age 34, 237.07 at 35, 0 at 49; scale 0.8.
        assert!(near(at(34)[0].alpha, 254.0) && near(at(35)[0].alpha, 237.07) && near(at(49)[0].alpha, 0.0) && at(0)[0].scale == 0.8);
    }

    #[test]
    fn detecting_bursts_a_soft_ring_on_the_ground() {
        let (first, start) = effect_at(DETECTING, |_| 0, false);
        // Half-width: 1.607 after age 0, 41.893 at 16, 42.0 at 17 (the rate drops to 0.1), 48.63 at 56.
        assert!(near(detecting_half(0), 1.607) && near(detecting_half(1), 3.321) && near(detecting_half(16), 41.893));
        assert!(near(detecting_half(17), 42.0) && near(detecting_half(26), 43.07) && near(detecting_half(39), 45.11) && near(detecting_half(56), 48.63));
        // One square for ages 0…56, 5 up from the ground, alpha 254 to 39, 239 at 40, 0 at 56; none at 57.
        let mesh = |e: u32| first.meshes(tick(start, e), |_| None);
        assert!((0..57).all(|e| mesh(e).len() == 1) && mesh(57).is_empty());
        let (path, vertices) = &mesh(0)[0];
        assert!(*path == FASHASHA && vertices.len() == 6 && vertices.iter().all(|vertex| vertex.position[1] == 5.0));
        let alpha = |e: u32| (mesh(e)[0].1[0].tint[3] * 255.0).round();
        assert_eq!([0, 39, 40, 48, 56].map(alpha), [254.0, 254.0, 239.0, 119.0, 0.0]);
        // The corners as Turn Undead's ring: (+W, +W) at uv (0, 0), (+W, −W) at (1, 0).
        assert!(near(vertices[0].position[0], 1.607) && near(vertices[0].position[2], 1.607) && vertices[0].uv == [0.0, 0.0]);
        assert!(near(vertices[1].position[2], -1.607) && vertices[1].uv == [1.0, 0.0] && vertices[3].uv == [1.0, 1.0]);
        // It stays on its cell.
        let (other, other_start) = effect_at(DETECTING, |_| 0, false);
        assert!(near(other.meshes(other_start, |_| Some([9.0; 3]))[0].1[0].position[0], 1.607));
    }

    #[test]
    fn ignition_break_bursts_a_pale_orange_ring_under_the_monster() {
        let (first, start) = effect_at(IGNITION_RING, |_| 0, false);
        // Detecting's ring: 1.607 after age 0, 41.893 at 16, 42.0 at 17, 48.63 at 56; 57 draws.
        let mesh = |e: u32| first.meshes(tick(start, e), |_| Some([0.0; 3]));
        assert!((0..57).all(|e| mesh(e).len() == 1) && mesh(57).is_empty());
        let half = |e: u32| mesh(e)[0].1[0].position[0];
        assert!(near(half(0), 1.607) && near(half(16), 41.893) && near(half(17), 42.0) && near(half(56), 48.63));
        let alpha = |e: u32| (mesh(e)[0].1[0].tint[3] * 255.0).round();
        assert_eq!([39, 40, 55, 56].map(alpha), [254.0, 239.0, 14.0, 0.0]);
        // #FAD28C, on the fashasha ring, 5 up.
        let (path, vertices) = &mesh(0)[0];
        assert!(*path == FASHASHA && vertices.iter().all(|vertex| vertex.position[1] == 5.0));
        assert_eq!(vertices[0].tint[..3], [250.0 / 255.0, 210.0 / 255.0, 140.0 / 255.0]);
        // It follows the monster.
        let moved = first.meshes(tick(start, 3), |_| Some([20.0, 1.0, 0.0]));
        assert!(near(moved[0].1[0].position[0], 20.0 + half(3)) && moved[0].1[0].position[1] == 6.0);
    }

    #[test]
    fn slow_cast_floats_a_clock_over_the_target() {
        let (first, start) = effect_at(SLOW_CAST, |_| 0, false);
        let at = |e: u32| first.squares(tick(start, e), |_| Some([0.0; 3]));
        // One square, drawn for n = 1 … 242.
        assert_eq!(first.list[0].seeds.len(), 1);
        assert!((0..242).all(|e| at(e).len() == 1) && at(242).is_empty());
        let clock = at(0)[0];
        assert!(clock.texture == CLOCK && !clock.additive && clock.tint == [255; 3] && !clock.mirrored);
        // Corners at 135, 45, 315, 225 (y up): 225, 315, 45, 135 a quarter turn apart on the screen (y down).
        assert_eq!(clock.spin, 225.0);
        // Alpha: +10 a tick to 220 at n = 22, held, −5 a tick from n = 200 (215), 0 at 243.
        let alpha = |n: u32| at(n - 1).first().map_or(0.0, |square| square.alpha);
        assert_eq!([1, 22, 199, 200, 242].map(alpha), [10.0, 220.0, 220.0, 215.0, 5.0]);
        assert_eq!(slow_cast_alpha(243), 0.0);
        // Half-diagonal 6 (1 + 0.05 sin 2n°): 6.3 at n = 45.
        assert!(near(at(0)[0].radius, 6.0 * (1.0 + 0.05 * 2.0_f32.to_radians().sin())) && near(at(44)[0].radius, 6.3));
        // 30 over the feet, read again every tick.
        assert_eq!(at(0)[0].at, [0.0, 30.0, 0.0]);
        assert_eq!(first.squares(tick(start, 5), |_| Some([10.0, 2.0, -4.0]))[0].at, [10.0, 32.0, -4.0]);
    }

    #[test]
    fn enchant_poison_lets_43_purple_sparkles_rise() {
        let (first, start) = effect_at(ENCHANT_POISON, |_| 0, false);
        // A sparkle every 5 ticks to 245: 50; those from 215 on are never drawn.
        assert_eq!(first.list[0].seeds.iter().map(|seed| seed.born).collect::<Vec<_>>(), (0..=245).step_by(5).collect::<Vec<_>>());
        let at = |e: u32| first.particles(tick(start, e), |_| Some([0.0; 3]));
        // Drawn first at alpha 25: 43 times over the 250 ticks.
        assert_eq!((0..250).map(|e| at(e).iter().filter(|particle| particle.alpha == 25.0).count()).sum::<usize>(), 43);
        assert_eq!((at(210).len(), at(215).len(), at(245).len()), (8, 7, 1));
        // The one born at 210 is drawn 39 times (ages 0…38); the one born at 0, 40.
        assert!(at(248).len() == 1 && at(249).is_empty());
        assert!(at(39).len() == 8 && at(40).len() == 8, "one ends and one begins every 5 ticks");
        // Draws all 0: 2 out at yaw 0, speed 0.30, scale 0.4; up 29.75 × 0.30 = 8.925 after age 39.
        let sparkle = |e: u32| at(e)[0];
        assert!(near(sparkle(0).at[2], 2.0) && near(sparkle(0).at[1], 0.30 * (1.0 - 2.0 / 160.0)) && near(sparkle(39).at[1], 8.925));
        assert!(sparkle(39).scale == 0.4 && sparkle(0).sprite == POISON_STAR);
        // Alpha 25, 253 at age 9, 254 at 10, 222 at 32, 0 at 39; turned 3° (a + 1) clockwise.
        let alpha = |e: u32| sparkle(e).alpha;
        assert_eq!([0, 9, 10, 31, 32, 39].map(alpha), [25.0, 253.0, 254.0, 254.0, 222.0, 0.0]);
        assert_eq!([0, 9, 39].map(|e| sparkle(e).spin), [3.0, 30.0, 120.0]);
        // Out 2 to 7 from the feet at the yaw, even after the unit walks away.
        let mut draws = [359, 5, 29, 49].into_iter().cycle();
        let (far, far_start) = effect_at(ENCHANT_POISON, |_| draws.next().unwrap(), false);
        let particle = far.particles(tick(far_start, 0), |_| Some([100.0, 0.0, 100.0]))[0];
        assert!(near(f32::hypot(particle.at[0], particle.at[2]), 7.0) && particle.at[0] < 0.0 && near(particle.scale, 0.89));
        assert!(particle.at[0].abs() < 8.0, "{:?}", particle.at);
    }

    #[test]
    fn enchant_deadly_poison_lifts_27_violet_sparkles() {
        let (first, start) = effect_at(ENCHANT_DEADLY_POISON, |_| 0, false);
        // A sparkle every 3 ticks: 40 made, those after tick 80 never drawn, so 27 show.
        assert_eq!(first.list[0].seeds.iter().map(|seed| seed.born).collect::<Vec<_>>(), (0..=117).step_by(3).collect::<Vec<_>>());
        let at = |e: u32| first.particles(tick(start, e), |_| Some([0.0; 3]));
        assert_eq!((0..120).map(|e| at(e).iter().filter(|particle| particle.alpha == 25.0).count()).sum::<usize>(), 27);
        assert!(at(81).iter().all(|particle| particle.alpha != 25.0) && !at(78).is_empty());
        // Every draw 0: 2 out, speed 0.30, scale 0.20; each is drawn 40 times (the last of the first at tick 39).
        let sparkle = at(0)[0];
        assert!(near(f32::hypot(sparkle.at[0], sparkle.at[2]), 2.0) && near(sparkle.scale, 0.2) && sparkle.sprite == POISON_STAR);
        assert!(!at(39).is_empty() && near(at(39)[0].at[1], 0.30 * 29.75));
        assert!(near(at(39)[0].alpha, 0.0));
        // It stays where the caster stood at tick 0.
        let (moved, moved_start) = effect_at(ENCHANT_DEADLY_POISON, |_| 0, false);
        assert!(near(moved.particles(tick(moved_start, 0), |_| Some([50.0; 3]))[0].at[0], 0.0));
    }

    #[test]
    fn aspersio_lets_fifteen_sparkles_fall() {
        let (first, start) = effect_at(ASPERSIO, |_| 0, false);
        assert_eq!(first.list[0].seeds.iter().map(|seed| seed.born).collect::<Vec<_>>(), (65..=135).step_by(5).collect::<Vec<_>>());
        assert_eq!(first.list[0].seeds.len(), 15);
        let at = |e: u32| first.particles(tick(start, e), |_| Some([100.0, 0.0, 0.0]));
        // None at 60, one at 65 (from age 0), the height 29.80, 27.55 at 9, 24.10 at 19, 19.65 at 29, 14.20 at 39 (on 3 round the feet).
        assert!(at(60).is_empty() && at(65).len() == 1);
        let height = |k: u32| at(65 + k)[0].at[1];
        assert!(near(height(0), 29.80) && near(height(9), 27.55) && near(height(19), 24.10) && near(height(29), 19.65) && near(height(39), 14.20));
        assert!(near(f32::hypot(at(65)[0].at[0], at(65)[0].at[2]), 3.0) && at(65)[0].scale == 1.2);
        // Alpha 254 to age 29, 228.6 at 30, 0 at 39; 40 draws; fixed when the target walks off.
        let alpha = |k: u32| at(65 + k)[0].alpha;
        assert!(near(alpha(29), 254.0) && near(alpha(30), 228.6) && near(alpha(39), 0.0));
        assert_eq!((0..175).filter(|&e| at(e).iter().any(|_| true) && e < 105).count(), 40);
        assert!(at(100).iter().all(|particle| particle.at[0].abs() < 4.0));
        assert!(at(175).is_empty());
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
        let seed = Seed { born: 0, y: 0.0, r: [2.0, 0.5, 3.0, 0.0], more: [0.0; 2] };
        let streak_alpha = |a: u32| rising_blades([0.0; 3], &seed, a, [1.0; 3])[0].tint[3] * 255.0;
        assert!(near(streak_alpha(19), 200.0) && near(streak_alpha(29), 200.0) && near(streak_alpha(30), 190.0) && near(streak_alpha(49), 0.0));
        let middle = |a: u32| rising_blades([0.0; 3], &seed, a, [1.0; 3]).iter().map(|vertex| vertex.position[1]).sum::<f32>() / 12.0;
        assert!(near(middle(49), 0.5 * 50.0));
        // The blades are the same whatever y is.
        let turned = Seed { y: 120.0, ..seed };
        assert!(
            rising_blades([0.0; 3], &seed, 5, [1.0; 3])
                .iter()
                .zip(rising_blades([0.0; 3], &turned, 5, [1.0; 3]))
                .all(|(p, q)| near(p.position[1], q.position[1]))
        );
    }

    #[test]
    fn stat_up_signs_float_their_picture_over_cyan_streaks() {
        let signs = [(C_RATE_UP, [128.0, 16.0], 160.0, 20.0), (P_ATK_UP, [64.0, 16.0], 80.0, 20.0), (P_ATK_S_MATK_UP, [128.0, 32.0], 160.0, 40.0)];
        for (id, picture, width, height) in signs {
            let start = Instant::now();
            let mut first = First::default();
            first.sign(id, Some(7), picture, start, |_| 0);
            let text = |e: u32| {
                first.shapes(tick(start, e)).into_iter().next().map(|(path, unit, corners)| {
                    (
                        path,
                        unit,
                        (corners[2].0[0] - corners[0].0[0], corners[2].0[1] - corners[0].0[1]),
                        (corners[0].0[1] + corners[2].0[1]) / 2.0,
                        corners[0].2[3] * 255.0,
                    )
                })
            };
            // The quad is 1.25 times the picture; the picture is the sign's own.
            let (path, unit, size, _, _) = text(0).unwrap();
            assert_eq!((path, unit, size), (sign_picture(id).unwrap(), Some(7), (width, height)), "{id}");
            // y −1.48 at a = 0, −14.0 at 9, −52.1 at 49, −61.75 at 81, −59.4 at 98; 99 draws.
            let y = |e: u32| text(e).unwrap().3;
            assert!(near(y(0), -1.482) && near(y(9), -14.01) && near(y(49), -52.05) && near(y(81), -61.75) && near(y(98), -59.4));
            assert!(text(99).is_none() && (0..100).filter(|&e| text(e).is_some()).count() == 99);
            // 13, 26, 199 at 14, 200 at 15 and 84, 186 at 85, 13 at 98.
            let alpha = |e: u32| text(e).unwrap().4.floor();
            assert_eq!([0, 1, 14, 15, 84, 85, 98].map(alpha), [13.0, 26.0, 199.0, 200.0, 200.0, 186.0, 13.0]);
        }
        // 26 streaks, at e = 0, 2 … 50, none at 52.
        let (mut first, start) = (First::default(), Instant::now());
        first.sign(RES_UP, None, [64.0, 16.0], start, |_| 0);
        assert_eq!(first.list[0].seeds.iter().map(|seed| seed.born).collect::<Vec<_>>(), (0..=50).step_by(2).collect::<Vec<_>>());
        let streaks = |e: u32| first.meshes(tick(start, e), |_| Some([0.0; 3])).len();
        // Seeds 0 … 48 show at 49 (25); the one made at 0 is over at 50, the one made at 50 shows; 24 at 52.
        assert_eq!((streaks(0), streaks(49), streaks(50), streaks(52)), (1, 25, 25, 24));
        // Every draw 0: half-length 3.0, yaw 0, distance 2, speed 0.20; the largest: 8.9, 359, 8, 0.69.
        let seed = |draw: fn(u32) -> u32| {
            let mut first = First::default();
            first.sign(RES_UP, None, [64.0, 16.0], start, draw);
            let seed = first.list[0].seeds[0];
            (seed.r[2], seed.y, seed.r[0], seed.r[1])
        };
        assert_eq!(seed(|_| 0), (3.0, 0.0, 2.0, 0.2));
        assert_eq!(seed(|n| n - 1), (8.9, 359.0, 8.0, 0.69));
        // The draws go R(60), R(360), R(7), R(50), a streak at a time.
        let mut asked = Vec::new();
        let mut order = First::default();
        order.sign(RES_UP, None, [64.0, 16.0], start, |n| {
            asked.push(n);
            0
        });
        assert_eq!(asked[..8], [60, 360, 7, 50, 60, 360, 7, 50]);
        assert_eq!(asked.len(), 26 * 4);
        // Cyan, alpha 10 at a = 0, 200 at 19 and 29, 190 at 30, 0 at 49.
        let seed = Seed { born: 0, y: 0.0, r: [2.0, 0.5, 3.0, 0.0], more: [0.0; 2] };
        let blades = |a: u32| rising_blades([0.0; 3], &seed, a, SIGN_STREAK)[0].tint;
        assert_eq!(&blades(0)[..3], &[0.0, 1.0, 1.0]);
        assert_eq!([0, 19, 29, 30, 49].map(|a| (blades(a)[3] * 255.0).round()), [10.0, 200.0, 200.0, 190.0, 0.0]);
        // A second packet makes a second copy; both play on.
        let mut twice = First::default();
        twice.sign(RES_UP, None, [64.0, 16.0], start, |_| 0);
        twice.sign(RES_UP, None, [64.0, 16.0], tick(start, 10), |_| 0);
        assert_eq!(twice.shapes(tick(start, 20)).len(), 2);
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
    fn cart_revolution_swells_a_fireball_under_a_ring_twice() {
        let (first, start) = effect_at(CART_REVOLUTION, |_| 0, false);
        // The parts come at ticks 7 and 20 (20 draws each), the ring at the depth test off, the sphere depth-tested.
        assert_eq!(first.list[0].seeds.iter().map(|seed| seed.born).collect::<Vec<_>>(), [7, 20]);
        assert!(first.top_meshes(tick(start, 6)).is_empty() && first.top_meshes(tick(start, 7)).len() == 1);
        assert_eq!(first.top_meshes(tick(start, 20)).len(), 2);
        assert_eq!(first.meshes(tick(start, 7), |_| None).len(), 1);
        assert!(first.top_meshes(tick(start, 27)).len() == 1 && first.top_meshes(tick(start, 40)).is_empty());
        // The ring: R 1.71, 3.37 … 25.81 at a = 19; a disc to a = 2 and 5 wide after; alpha 12 to 60 at 4, 56, 8 at 17, 0 at 19.
        let outer = |a: u32| {
            let mesh = ring([0.0; 3], a, 255.0, 0.021_875, 5.0);
            f32::hypot(mesh[1].position[0], mesh[1].position[2])
        };
        assert!(near(outer(0), 1.706) && near(outer(1), 3.369) && near(outer(19), 25.8125), "{} {} {}", outer(0), outer(1), outer(19));
        let inner = |a: u32| f32::hypot(ring([0.0; 3], a, 255.0, 0.021_875, 5.0)[0].position[0], ring([0.0; 3], a, 255.0, 0.021_875, 5.0)[0].position[2]);
        assert!(inner(2) < 0.01 && near(inner(3), outer(3) - 5.0));
        assert_eq!([0, 4, 5, 8, 17, 19].map(cart_ring_alpha), [12.0, 60.0, 56.0, 44.0, 8.0, 0.0]);
        // The sphere: alpha 34, 68, 102, 137, 171, 159, 125 at a = 8, 0 at 19.
        assert_eq!([0, 1, 2, 3, 4].map(|a| cart_dome_alpha(a).floor()), [34.0, 68.0, 102.0, 137.0, 171.0]);
        assert_eq!(cart_dome_alpha(5).floor(), 159.0);
        assert_eq!(cart_dome_alpha(8).floor(), 125.0);
        assert!(near(cart_dome_alpha(19), 0.0));
        let radius = |a: u32| {
            dome([0.0; 3], a, 255.0, 1.35, 0.016_875).iter().map(|vertex| (vertex.position.iter().map(|v| v * v).sum::<f32>()).sqrt()).fold(0.0, f32::max)
        };
        assert!(near(radius(0), 1.316) && (radius(19) - 19.91).abs() < 0.05, "{} {}", radius(0), radius(19));
    }

    #[test]
    fn first_aid_glows_for_179_ticks() {
        let (first, start) = effect_at(FIRST_AID, |_| 0, false);
        assert_eq!(first.list[0].seeds.len(), 1);
        let square = |e: u32| first.squares(tick(start, e), |_| Some([100.0, 0.0, 0.0]));
        // r = 10 ± 0.5, 12 up, not following; alpha 4 a tick to 80 at n = 20, held to 100, 0 at n = 180.
        let first_square = square(0)[0];
        assert!(near(first_square.radius, 10.0 + 10.0 * 3.0_f32.to_radians().sin() * 0.05) && first_square.at == [0.0, 12.0, 0.0]);
        assert_eq!(first_square.tint, [0x96, 0x96, 0xFA]);
        let alpha = |n: u32| square(n - 1)[0].alpha;
        assert_eq!([1, 20, 100, 101, 179].map(alpha), [4.0, 80.0, 80.0, 79.0, 1.0]);
        assert_eq!(alpha(180), 0.0);
        assert_eq!((0..299).filter(|&e| square(e).first().is_some_and(|square| square.alpha > 0.0)).count(), 179);
    }

    #[test]
    fn sprinkle_sand_scatters_80_specks() {
        let (first, start) = effect_at(SPRINKLE_SAND, |_| 0, false);
        assert_eq!(first.list[0].seeds.len(), 80);
        // Every draw 0: delay −10, so a first step on tick 10; half-diagonal 0.1; 4 round the feet; speed 0; rise 0.2.
        assert!(first.list[0].seeds.iter().all(|seed| seed.born == 10));
        let at = |e: u32| first.squares(tick(start, e), |_| None);
        assert!(at(10).is_empty(), "age 1: alpha 0, not drawn");
        // Ages 2 … 54: alpha 15, then up to 135 at age 10, down 3 a tick to 3 at 54; gone at 55.
        assert_eq!((at(11)[0].alpha, at(19)[0].alpha, at(20)[0].alpha, at(63)[0].alpha), (15.0, 135.0, 132.0, 3.0));
        assert!(at(64).is_empty());
        // Height 0.2 · age; spin −5 a tick; nobody moves sideways at speed 0.
        assert!(near(at(19)[0].at[1], 0.2 * 10.0) && near(at(19)[0].spin, 310.0));
        assert_eq!(at(11).len(), 80);
        assert!(at(11)[0].mirrored && at(11)[0].tint == [0xFA, 0xFA, 0x96]);
        // Delays vary: with the draw 9 the first step is on tick 1.
        let mut draws = [9, 0, 0, 0, 0, 0].into_iter().cycle();
        let (late, _) = effect_at(SPRINKLE_SAND, |_| draws.next().unwrap(), false);
        assert!(late.list[0].seeds.iter().all(|seed| seed.born == 1));
    }

    #[test]
    fn the_undead_heals_sparkles_twinkle() {
        let (first, start) = effect_at(HEAL_UNDEAD, |_| 0, false);
        // Four sets of four at ticks 6, 12, 18, 24: 16 squares.
        assert_eq!(first.list[0].seeds.len(), 16);
        assert_eq!(first.list[0].seeds.iter().map(|seed| seed.born).collect::<Vec<_>>().iter().filter(|&&born| born == 6).count(), 4);
        let at = |e: u32| first.squares(tick(start, e), |_| None);
        // From age 2 at tick born + 1: alpha 15 … 135 at age 10, drawn to 54.
        assert!(at(6).is_empty() && at(7).len() == 4 && at(7)[0].alpha == 15.0);
        // Element k shows pok3 at ages 35 − 7k and 36 − 7k (tick = born + age − 1): k = 0 at 35 and 36, 1 at 28 and 29, 3 at 14 and 15.
        let texture = |k: usize, age: u32| {
            first.squares(tick(start, 6 + age - 1), |_| None).iter().filter(|square| square.tint == [0xDC, 0xFA, 0xDC]).nth(k).map(|square| square.texture)
        };
        assert_eq!(
            [texture(0, 35), texture(0, 36), texture(0, 34), texture(1, 28), texture(1, 29), texture(3, 14), texture(3, 15), texture(3, 16)],
            [Some(POK3), Some(POK3), Some(POK1), Some(POK3), Some(POK3), Some(POK3), Some(POK3), Some(POK1)]
        );
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
        let seed = Seed { born: 0, y: 0.0, r: [2.0, 3.0, 0.0, 0.0], more: [0.0; 2] };
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

    #[test]
    fn the_side_slide_ring_is_a_smaller_earlier_fading_detecting() {
        let (first, start) = effect_at(SIDE_SLIDE, |_| 0, false);
        // W after ages 9, 10, 15, 30, 56: 20.893, 20.996, 21.563, 23.763, 29.353.
        assert!(near(ring_half(0, 10), 1.607) && near(ring_half(9, 10), 20.893) && near(ring_half(10, 10), 20.996));
        assert!(near(ring_half(15, 10), 21.563) && near(ring_half(30, 10), 23.763) && near(ring_half(56, 10), 29.353));
        // Alpha 254 to age 29, 244 at 30, 0 at 56; 57 draws; it stays on its cell.
        let mesh = |e: u32| first.meshes(tick(start, e), |_| None);
        assert!((0..57).all(|e| mesh(e).len() == 1) && mesh(57).is_empty());
        let alpha = |e: u32| (mesh(e)[0].1[0].tint[3] * 255.0).trunc();
        assert_eq!([0, 29, 30, 56].map(alpha), [254.0, 254.0, 244.0, 0.0]);
        assert!(makes(SIDE_SLIDE));
    }

    #[test]
    fn infrared_scan_rings_red_and_analyze_rings_green_following_its_target() {
        let (red, start) = effect_at(RED_RING, |_| 0, false);
        // Detecting's ring in #FF8C8C: W 41.893 at age 16 and 48.63 at 56, 57 draws.
        assert!(near(detecting_half(16), 41.893) && near(detecting_half(56), 48.63));
        let mesh = |first: &First, e: u32| first.meshes(tick(start, e), |_| Some([5.0; 3]));
        assert!((0..57).all(|e| mesh(&red, e).len() == 1) && mesh(&red, 57).is_empty());
        let vertex = &mesh(&red, 0)[0].1[0];
        assert_eq!(vertex.tint[..3], [1.0, 140.0 / 255.0, 140.0 / 255.0]);
        // The red ring stays on its cell; the green one follows the target's feet every tick and is #D2FAD2.
        assert!(near(vertex.position[0], 1.607));
        let (green, _) = effect_at(GREEN_RING, |_| 0, false);
        let moved = green.meshes(tick(start, 3), |_| Some([40.0, 0.0, 0.0]));
        assert!(moved[0].1[0].position[0] > 40.0);
        assert_eq!(moved[0].1[0].tint[..3], [210.0 / 255.0, 250.0 / 255.0, 210.0 / 255.0]);
        assert!(makes(RED_RING) && makes(GREEN_RING));
    }

    #[test]
    fn deadly_infect_rings_the_ground_dark() {
        let (first, start) = effect_at(DARK_RING, |_| 0, false);
        // Hovering's ring in #0A0A0A: W after age 9, 10, 56 20.893, 20.996, 29.353; alpha 254 to 29, 244 at 30, 0 at 56.
        let mesh = |e: u32| first.meshes(tick(start, e), |_| None);
        let vertex = |e: u32| mesh(e)[0].1[0];
        assert!(near(ring_half(9, 10), 20.893) && near(ring_half(56, 10), 29.353));
        assert_eq!(vertex(0).tint[..3], [10.0 / 255.0; 3]);
        assert_eq!([29, 30, 56].map(|e| (vertex(e).tint[3] * 255.0).trunc()), [254.0, 244.0, 0.0]);
        assert!((0..57).all(|e| mesh(e).len() == 1) && mesh(57).is_empty());
    }
}
