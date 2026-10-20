//! Walls and puffs built in code a 16 ms tick at a time: Providence's tubes (248), Reflect Shield's yellow crescents (252), Absorb
//! Spirits' curtains (253), the Fire Pillar's three funnels (138), Venom Dust's violet puffs
//! (171), Berserk Pitcher's tint, rising cylinder and streaks (220), Defender's four crescent walls (222) and Grand
//! Cross's cross of light (226).
//!
//! A shape's own age a counts from 1 at its first tick (p = a − 1 from 0). Alphas come from `burst::alpha` or their own
//! steps and are cut to whole numbers when drawn.

use std::time::Instant;

use super::sage::{Corner, ring_point, ring_quads};
use super::{burst, first, passes};
use crate::world::{MeshVertex, Vector};

pub const MAGIC_RED: &str = "data\\texture\\effect\\magic_red.tga";
pub const RING_BLACK: &str = "data\\texture\\effect\\ring_black.tga";
pub const RING_WHITE: &str = "data\\texture\\effect\\ring_white.tga";
pub const RING_YELLOW: &str = first::RING_YELLOW;
pub const RING_BLUE: &str = "data\\texture\\effect\\ring_blue.tga";
pub const ALPHA_DOWN: &str = first::ALPHA_DOWN;
pub const STREAK: &str = first::STREAK;

/// The Fire Pillar's funnels: radius at the ground, radius at the top and height.
const FUNNELS: [(f32, f32, f32); 3] = [(2.5, 4.0, 27.5), (3.5, 5.0, 15.5), (4.5, 6.0, 7.5)];
const FUNNEL_ALPHA: f32 = 240.0;
/// A Venom Dust puff comes every 5 ticks and lives 10.
const PUFF_EVERY: u32 = 5;
const PUFF_LIFE: u32 = 10;
/// Berserk's tint (#FA7878) is held to tick 150, then goes back to white by 8 a channel a tick.
const BERSERK_TINT: [u8; 3] = [0xFA, 0x78, 0x78];
const BERSERK_HELD: u32 = 150;
const CYLINDER_LIFE: u32 = 50;
const STREAK_LIFE: u32 = 40;
/// Defender plays 200 ticks (drawn on the first 199); its four bands: radius, height, start turn, turn a tick.
const DEFENDER_TICKS: u32 = 199;
const DEFENDER_BANDS: [(f32, f32, f32, f32); 4] = [(8.0, 40.0, 180.0, 1.0), (7.9, 39.0, 270.0, 2.0), (7.8, 38.0, 0.0, 1.0), (7.7, 37.0, 90.0, 2.0)];
/// Providence's tubes: drawn on ticks 0 to 75; four of them, their base radius 6 + k / 2 and lean 83 − k degrees.
const TUBE_TICKS: u32 = 76;
/// Absorb Spirits' curtains are drawn on ticks 0 to 98; four bands: radius, height, lean and fixed turn.
const CURTAIN_TICKS: u32 = 99;
const CURTAINS: [(f32, f32, f32, f32); 4] = [(4.6, 60.0, 90.0, 0.0), (5.0, 60.0, 90.0, 180.0), (5.0, 18.0, 50.0, 0.0), (5.2, 18.0, 48.0, 0.0)];
/// The curtains' tint (#6464FF).
const CURTAIN_RGB: [f32; 3] = [100.0 / 255.0, 100.0 / 255.0, 1.0];
/// What tells Steel Body's ribbons from Ganbantein's: four slowly turning crescents; the wave's height, how fast the alpha falls
/// from its 200, how fast the radius grows, the lean's start and floor, the turn a tick, and the ticks they are drawn.
pub struct RibbonLook {
    texture: &'static str,
    peak: f32,
    height: f32,
    fall: f32,
    grow: f32,
    lean_from: f32,
    lean_floor: f32,
    turn: f32,
    ticks: u32,
    forced: bool,
}

/// Steel Body's: yellow, 25 high, drawn on ticks 0 to 109 (alpha 0 at age 110), even with /effect off.
pub const STEEL_BODY_RIBBONS: RibbonLook = RibbonLook {
    texture: RING_YELLOW,
    peak: 200.0,
    height: 25.0,
    fall: 2.0,
    grow: 0.1,
    lean_from: 80.0,
    lean_floor: 30.0,
    turn: 3.0,
    ticks: 110,
    forced: true,
};
/// Ganbantein's: white, 15 high, quicker, drawn on ticks 0 to 33.
pub const GANBANTEIN: RibbonLook =
    RibbonLook { texture: RING_WHITE, peak: 200.0, height: 15.0, fall: 8.0, grow: 0.3, lean_from: 70.0, lean_floor: 40.0, turn: 5.0, ticks: 34, forced: false };
/// Storm Blast's: yellow, half as bright, 25 high, drawn on ages 1 to 59.
pub const STORM_BLAST: RibbonLook = RibbonLook {
    texture: RING_YELLOW,
    peak: 100.0,
    height: 25.0,
    fall: 2.0,
    grow: 0.3,
    lean_from: 80.0,
    lean_floor: 40.0,
    turn: 5.0,
    ticks: 59,
    forced: false,
};
/// Grand Cross's four arcs: where from the caster's spot and the turn they start at.
const ARCS: [([f32; 2], f32); 4] = [([23.0, 23.0], 180.0), ([23.0, -23.0], 90.0), ([-23.0, -23.0], 0.0), ([-23.0, 23.0], 270.0)];
/// The arcs are drawn on ages 1…98, the bars on 1…89.
const ARC_AGES: u32 = 98;
const BAR_AGES: u32 = 89;
/// The arcs' tint (#FFAFAF).
const ARC_RGB: [f32; 3] = [1.0, 175.0 / 255.0, 175.0 / 255.0];

/// A world mesh to draw: its texture, triangles, whether it adds on, and whether it draws even with /effect off.
pub struct Wall {
    pub texture: &'static str,
    pub vertices: Vec<MeshVertex>,
    pub additive: bool,
    pub forced: bool,
}

enum Kind {
    Funnels,
    Puffs,
}

/// A Fire Pillar's or Venom Dust's cell, standing with its skill unit.
struct Standing {
    kind: Kind,
    id: u32,
    at: Vector,
    start: Instant,
    /// Venom Dust's: the ticks run, and each puff's tick and first speed.
    ticks: u32,
    puffs: Vec<(u32, f32)>,
}

struct Streak {
    born: u32,
    yaw: f32,
    distance: f32,
    speed: f32,
    half: f32,
}

/// A Berserk Pitcher's effect on its owner.
struct Berserk {
    unit: Option<u32>,
    start: Instant,
    streaks: Vec<Streak>,
}

/// Four crescent ribbons round a unit (Steel Body's, Explosion Spirits', the Durian's) or over a cell (Ganbantein's).
struct Ribbons {
    look: &'static RibbonLook,
    unit: Option<u32>,
    at: Option<Vector>,
    start: Instant,
}

struct Defender {
    unit: Option<u32>,
    start: Instant,
    /// Its walls' texture: black for Defender, yellow for Reflect Shield.
    texture: &'static str,
}

/// Providence's four tubes round a unit.
struct Providence {
    unit: Option<u32>,
    start: Instant,
}

/// Absorb Spirits' two curtains and two skirts round a unit; the last two bands' turns are drawn at the start.
struct Curtains {
    unit: Option<u32>,
    start: Instant,
    turns: [f32; 4],
}

/// One Grand Cross where its caster stood.
struct Cross {
    at: Vector,
    start: Instant,
}

#[derive(Default)]
pub struct Walls {
    standing: Vec<Standing>,
    berserks: Vec<Berserk>,
    defenders: Vec<Defender>,
    ribbons: Vec<Ribbons>,
    providences: Vec<Providence>,
    curtains: Vec<Curtains>,
    crosses: Vec<Cross>,
}

/// A tick's alpha as a whole number out of 255, as a tint's last channel.
fn tint(rgb: [f32; 3], alpha: f32) -> [f32; 4] {
    [rgb[0], rgb[1], rgb[2], alpha.trunc().clamp(0.0, 255.0) / 255.0]
}

fn vertices(corners: Vec<Corner>, rgb: [f32; 3]) -> Vec<MeshVertex> {
    corners.into_iter().map(|(position, uv, alpha)| MeshVertex { position, uv, tint: tint(rgb, alpha * 255.0) }).collect()
}

/// A wall of 10 quads round `radius` from `at`, `height` high, u stepping a quarter and repeating every four.
fn cylinder(at: Vector, radius: f32, height: f32, alpha: f32) -> Vec<MeshVertex> {
    let tint = tint([1.0; 3], alpha);
    (0..10)
        .flat_map(|segment| {
            let u = (segment % 4) as f32 * 0.25;
            let point = |side: u32, y: f32, u: f32, v: f32| {
                let (sin, cos) = ((36 * side) as f32).to_radians().sin_cos();
                MeshVertex { position: [at[0] + radius * cos, at[1] + y, at[2] + radius * sin], uv: [u, v], tint }
            };
            let [foot0, top0, foot1, top1] = [
                point(segment, 0.0, u, 1.0),
                point(segment, height, u, 0.0),
                point(segment + 1, 0.0, u + 0.25, 1.0),
                point(segment + 1, height, u + 0.25, 0.0),
            ];
            [foot0, top0, foot1, top0, foot1, top1]
        })
        .collect()
}

/// The three funnels at tick p of their own: the texture runs round 6° a tick from 2° off.
fn funnels(at: Vector, p: u32) -> Vec<MeshVertex> {
    let turn = 2.0 + 6.0 * p as f32;
    let tint = tint([1.0; 3], FUNNEL_ALPHA);
    FUNNELS
        .iter()
        .flat_map(|&(inner, outer, height)| {
            (0..10).flat_map(move |segment| {
                let u = (segment % 4) as f32 * 0.25;
                let point = |side: u32, radius: f32, y: f32, u: f32, v: f32| {
                    let (sin, cos) = ((36 * side) as f32 + turn).to_radians().sin_cos();
                    MeshVertex { position: [at[0] + radius * cos, at[1] + y, at[2] + radius * sin], uv: [u, v], tint }
                };
                let [foot0, top0, foot1, top1] = [
                    point(segment, inner, 0.0, u, 1.0),
                    point(segment, outer, height, u, 0.0),
                    point(segment + 1, inner, 0.0, u + 0.25, 1.0),
                    point(segment + 1, outer, height, u + 0.25, 0.0),
                ];
                [foot0, top0, foot1, top0, foot1, top1]
            })
        })
        .collect()
}

/// A puff's height at tick p of its own: each tick the speed drops by a 20th of the first, then it rises.
fn puff_height(speed: f32, p: u32) -> f32 {
    let (mut left, mut height) = (speed, 0.0);
    for _ in 0..=p {
        left -= speed / 20.0;
        height += left;
    }
    height
}

/// The cylinder round a Berserk owner's feet at tick p: rising and widening nothing but its height.
pub fn cylinder_height(p: u32) -> f32 {
    let n = (p + 1) as f32;
    10.0 + n + 0.005 * n * (n + 1.0)
}

impl Streak {
    /// Two crossed upright blades at tick p of its own over the owner's `feet`, red.
    fn blades(&self, feet: Vector, p: u32) -> Vec<MeshVertex> {
        let (sin, cos) = self.yaw.to_radians().sin_cos();
        let centre = [feet[0] + self.distance * sin, feet[1] + self.speed * (p + 1) as f32, feet[2] + self.distance * cos];
        let tint = tint([240.0 / 255.0, 0.0, 0.0], burst::alpha(p, 0.0, 9.0, 180.0, 20, STREAK_LIFE));
        let (width, half) = (0.2, self.half);
        let blade = |along_x: bool| {
            let corner = |side: f32, end: f32| {
                let (x, z) = if along_x { (side * width, 0.0) } else { (0.0, side * width) };
                MeshVertex {
                    position: [centre[0] + x, centre[1] + end * half, centre[2] + z],
                    uv: [if end > 0.0 { 0.0 } else { 1.0 }, if side < 0.0 { 1.0 } else { 0.0 }],
                    tint,
                }
            };
            let [a, b, c, d] = [corner(-1.0, 1.0), corner(1.0, 1.0), corner(1.0, -1.0), corner(-1.0, -1.0)];
            [a, b, c, a, c, d]
        };
        [blade(true), blade(false)].concat()
    }
}

/// A ribbon's alpha at age t: 20 a tick to 200, then `fall` less a tick.
pub fn ribbon_alpha(look: &RibbonLook, t: u32) -> f32 {
    if t <= 10 { look.peak / 10.0 * t as f32 } else { (look.peak - look.fall * (t - 10) as f32).max(0.0) }
}

/// Band k's radius, lean and turn at age t.
pub fn ribbon_shape(look: &RibbonLook, k: usize, t: u32) -> (f32, f32, f32) {
    ((k + 1) as f32 + look.grow * t as f32, (look.lean_from - t as f32).max(look.lean_floor), (90.0 * k as f32 + look.turn * t as f32) % 360.0)
}

/// The height of a ribbon's top along its band at point s: a crescent, zero at both ends.
pub fn ribbon_top(look: &RibbonLook, k: usize, s: u32, t: u32) -> f32 {
    let wave = look.height * (9.0 * s as f32).to_radians().sin();
    wave + wave * 0.15 * (((t + 90 * k as u32) % 360) as f32).to_radians().sin()
}

fn ribbon(look: &RibbonLook, feet: Vector, k: usize, t: u32) -> Vec<MeshVertex> {
    let (radius, lean, turn) = ribbon_shape(look, k, t);
    let points: Vec<_> = (0..=20).map(|s| ring_point(feet, turn + 18.0 * s as f32, radius, ribbon_top(look, k, s, t), lean)).collect();
    vertices(ring_quads(&points, ribbon_alpha(look, t) / 255.0), [1.0; 3])
}

/// A Providence tube's profile at tick t: 120 sin(t + 1)° while t + 1 is up to 90.
pub fn tube_height(t: u32) -> f32 {
    120.0 * ((t + 1).min(90) as f32).to_radians().sin()
}

/// A tube's alpha at tick t: 40 to tick 49, then 1 less a tick.
pub fn tube_alpha(t: u32) -> f32 {
    if t <= 49 { 40.0 } else { (40.0 - (t - 49) as f32).max(0.0) }
}

fn tube(feet: Vector, k: usize, t: u32) -> Vec<MeshVertex> {
    let (radius, lean, turn) = (6.0 + 0.5 * k as f32, 83.0 - k as f32, 90.0 * k as f32 + t as f32);
    let points: Vec<_> = (0..=20).map(|s| ring_point(feet, turn + 18.0 * s as f32, radius, tube_height(t), lean)).collect();
    vertices(ring_quads(&points, tube_alpha(t) / 255.0), [1.0; 3])
}

/// A curtain's alpha at age a: 5 a tick to 75, held, then from age 40 2 less a tick.
pub fn curtain_alpha(a: u32) -> f32 {
    match a {
        0..=15 => 5.0 * a as f32,
        16..=39 => 75.0,
        _ => (75.0 - 2.0 * (a - 39) as f32).max(0.0),
    }
}

/// The height of band k's curtain at its point s (0 to 20) at age a: a wave round the band, the rim rising with sin a.
pub fn curtain_top(k: usize, s: u32, a: u32) -> f32 {
    let h = CURTAINS[k].1;
    let (s, a) = (s as f32, a as f32);
    let wave = match k {
        0 => (34.0 * s + 4.0 * a) % 360.0,
        1 => (34.0 * s + 3.0 * a) % 360.0,
        _ => 2.0 * (a + 17.0 * s) % 360.0,
    };
    let rise = if a <= 90.0 { a.to_radians().sin() } else { 1.0 };
    ((h * 0.25 * wave.to_radians().sin() + h * 0.75) * rise).max(0.0)
}

fn curtain(feet: Vector, k: usize, turn: f32, a: u32) -> Vec<MeshVertex> {
    let (radius, _, lean, _) = CURTAINS[k];
    let points: Vec<_> = (0..=20).map(|s| ring_point(feet, turn + 18.0 * s as f32, radius, curtain_top(k, s, a), lean)).collect();
    vertices(ring_quads(&points, curtain_alpha(a) / 255.0), CURTAIN_RGB)
}

/// Defender's alpha at age a: a up to 80, 80 to a = 179, then 5 less a tick.
pub fn defender_alpha(a: u32) -> f32 {
    if a <= 80 {
        a as f32
    } else if a <= 179 {
        80.0
    } else {
        (80.0 - 5.0 * (a - 179) as f32).max(0.0)
    }
}

/// Band k's turn at age a, in degrees.
pub fn defender_turn(k: usize, a: u32) -> f32 {
    let (_, _, start, rate) = DEFENDER_BANDS[k];
    (start + rate * a as f32) % 360.0
}

/// The height of band k's wall at its point s (0 to 20) at age a: one hump, rippling with θ.
pub fn defender_top(k: usize, s: u32, a: u32) -> f32 {
    let theta = if k < 2 { (a + 90 * k as u32) % 360 } else { (2 * (a + 45 * k as u32)) % 360 };
    DEFENDER_BANDS[k].1 * (9.0 * s as f32).to_radians().sin() * (1.0 + 0.3 * (theta as f32).to_radians().sin())
}

fn defender_band(at: Vector, k: usize, a: u32) -> Vec<MeshVertex> {
    let turn = defender_turn(k, a);
    let points: Vec<_> = (0..=20).map(|s| ring_point(at, turn + 18.0 * s as f32, DEFENDER_BANDS[k].0, defender_top(k, s, a), 90.0)).collect();
    vertices(ring_quads(&points, defender_alpha(a) / 255.0), [1.0; 3])
}

/// A Grand Cross arc's alpha at age a: 10 a tick to 90, then 1 less a tick.
pub fn arc_alpha(a: u32) -> f32 {
    if a <= 9 { 10.0 * a as f32 } else { (90.0 - (a - 9) as f32).max(0.0) }
}

/// A bar's alpha at age a: 9 a tick to 81, then 1 less a tick.
pub fn bar_alpha(a: u32) -> f32 {
    if a <= 9 { 9.0 * a as f32 } else { (81.0 - (a - 9) as f32).max(0.0) }
}

/// An arc's radius and the height of its top at age a.
pub fn arc_size(a: u32) -> (f32, f32) {
    (19.9 - 0.01 * a as f32, 120.0 * (a as f32).to_radians().sin())
}

/// A bar's half-length and half-width at age a, and the height of its walls.
pub fn bar_size(a: u32) -> (f32, f32, f32) {
    (24.0 + 0.005 * a as f32, 3.2 + 0.005 * a as f32, 60.0 * (a as f32).to_radians().sin())
}

impl Cross {
    fn walls(&self, a: u32) -> Vec<Wall> {
        let mut walls = Vec::new();
        if a <= ARC_AGES {
            let (radius, height) = arc_size(a);
            let alpha = arc_alpha(a) / 255.0;
            for &([dx, dz], turn) in &ARCS {
                let centre = [self.at[0] + dx, self.at[1], self.at[2] + dz];
                let points: Vec<_> = (0..=8).map(|m| ring_point(centre, turn + 10.0 * m as f32, radius, height, 90.0)).collect();
                walls.push(Wall { texture: RING_WHITE, vertices: vertices(ring_quads(&points, alpha), ARC_RGB), additive: true, forced: false });
            }
        }
        if a <= BAR_AGES {
            let (length, width, height) = bar_size(a);
            let alpha = bar_alpha(a);
            let y = self.at[1] + 1.0;
            for along_x in [true, false] {
                let place = |across: f32, along: f32, up: f32| {
                    let [x, z] = if along_x { [along, across] } else { [across, along] };
                    [self.at[0] + x, y + up, self.at[2] + z]
                };
                let corners = [(-width, -length), (width, -length), (width, length), (-width, length)];
                // The four walls up from the edges, v 1 at the ground.
                let mut curtain = Vec::new();
                for edge in 0..4 {
                    let ((a0, l0), (a1, l1)) = (corners[edge], corners[(edge + 1) % 4]);
                    let [foot0, top0, foot1, top1] = [
                        (place(a0, l0, 0.0), [0.0, 1.0]),
                        (place(a0, l0, height), [0.0, 0.0]),
                        (place(a1, l1, 0.0), [1.0, 1.0]),
                        (place(a1, l1, height), [1.0, 0.0]),
                    ]
                    .map(|(position, uv)| MeshVertex { position, uv, tint: tint([1.0; 3], alpha) });
                    curtain.extend([foot0, top0, foot1, top0, foot1, top1]);
                }
                walls.push(Wall { texture: RING_YELLOW, vertices: curtain, additive: false, forced: false });
                // The floor twice, each at half the alpha: v 1 at one end and 0 at the other, then the other way round.
                for flip in [false, true] {
                    let v = |end: f32| if (end > 0.0) != flip { 1.0 } else { 0.0 };
                    let [a, b, c, d] =
                        [(-width, -length, [0.0, v(-1.0)]), (width, -length, [1.0, v(-1.0)]), (width, length, [1.0, v(1.0)]), (-width, length, [0.0, v(1.0)])]
                            .map(|(across, along, uv)| MeshVertex { position: place(across, along, 0.0), uv, tint: tint([1.0; 3], alpha / 2.0) });
                    walls.push(Wall { texture: RING_YELLOW, vertices: vec![a, b, c, a, c, d], additive: true, forced: false });
                }
            }
        }
        walls
    }
}

impl Walls {
    /// A Fire Pillar's (138) or Venom Dust's (171) cell standing as skill unit `id` at `at`.
    // ponytail: the game's funnels die at about 128 s even if the unit stays; these stay as long as the unit.
    pub fn keep(&mut self, effect: u32, id: u32, at: Vector, now: Instant) {
        let kind = if effect == 138 { Kind::Funnels } else { Kind::Puffs };
        self.standing.push(Standing { kind, id, at, start: now, ticks: 0, puffs: Vec::new() });
    }

    pub fn forget(&mut self, id: u32) {
        self.standing.retain(|standing| standing.id != id);
    }

    /// A Berserk Pitcher's effect on `unit` (None = the player): its 24 streaks drawn now from `rand(n)` (0 to n − 1).
    pub fn berserk(&mut self, unit: Option<u32>, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        // One streak every 4 ticks from 16 to 108: yaw, distance, speed, half-length.
        let streaks = (16..=108)
            .step_by(4)
            .map(|born| {
                let yaw = rand(360) as f32;
                let distance = (2 + rand(7)) as f32;
                let speed = (rand(50) + 20) as f32 / 60.0;
                let half = (rand(60) + 30) as f32 / 10.0;
                Streak { born, yaw, distance, speed, half }
            })
            .collect();
        self.berserks.push(Berserk { unit, start: now, streaks });
    }

    /// Defender's walls (black) or Reflect Shield's (yellow) on `unit`.
    pub fn defender(&mut self, unit: Option<u32>, now: Instant, texture: &'static str) {
        self.defenders.push(Defender { unit, start: now, texture });
    }

    /// Four spinning crescent ribbons on `unit` (Steel Body, Explosion Spirits, the SL_SKA skill, a Durian).
    pub fn ribbons(&mut self, unit: Option<u32>, now: Instant) {
        self.ribbons.push(Ribbons { look: &STEEL_BODY_RIBBONS, unit, at: None, start: now });
    }

    /// Storm Blast's four yellow crescents on `unit`.
    pub fn storm_blast(&mut self, unit: Option<u32>, now: Instant) {
        self.ribbons.push(Ribbons { look: &STORM_BLAST, unit, at: None, start: now });
    }

    /// Ganbantein's four white crescents over the cell `at`.
    pub fn ganbantein(&mut self, at: Vector, now: Instant) {
        self.ribbons.push(Ribbons { look: &GANBANTEIN, unit: None, at: Some(at), start: now });
    }

    /// Providence's four tubes on `unit`.
    pub fn providence(&mut self, unit: Option<u32>, now: Instant) {
        self.providences.push(Providence { unit, start: now });
    }

    /// Absorb Spirits' curtains on `unit`; the turns of the two short ones come from `rand(n)` (0 to n − 1).
    // ponytail: the game plays its sound half as loud (80 units up); ours is at full volume.
    pub fn curtains(&mut self, unit: Option<u32>, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        let mut turns = CURTAINS.map(|(.., turn)| turn);
        turns[2] = rand(360) as f32;
        turns[3] = rand(360) as f32;
        self.curtains.push(Curtains { unit, start: now, turns });
    }

    /// A Grand Cross where its caster stood at `at`.
    pub fn grand_cross(&mut self, at: Vector, now: Instant) {
        self.crosses.push(Cross { at, start: now });
    }

    /// Runs Venom Dust's puffs up to `now` (a tick at a time, each puff's choices from `rand(n)`: its yaw, lost, then its
    /// speed) and drops what ended.
    pub fn advance(&mut self, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        for standing in self.standing.iter_mut().filter(|standing| matches!(standing.kind, Kind::Puffs)) {
            while standing.ticks < passes(standing.start, now) {
                if standing.ticks % PUFF_EVERY == 0 {
                    let _ = rand(360);
                    standing.puffs.push((standing.ticks, (rand(20) + 20) as f32 / 80.0));
                }
                standing.ticks += 1;
            }
            let tick = standing.ticks;
            standing.puffs.retain(|&(born, _)| tick <= born + PUFF_LIFE);
        }
        let ended = |start: Instant, ticks: u32| passes(start, now) > ticks;
        self.berserks.retain(|berserk| !ended(berserk.start, BERSERK_HELD + 20));
        self.defenders.retain(|defender| !ended(defender.start, DEFENDER_TICKS));
        self.ribbons.retain(|ribbons| !ended(ribbons.start, ribbons.look.ticks));
        self.providences.retain(|providence| !ended(providence.start, TUBE_TICKS));
        self.curtains.retain(|curtains| !ended(curtains.start, CURTAIN_TICKS));
        self.crosses.retain(|cross| !ended(cross.start, ARC_AGES));
    }

    /// Venom Dust's puffs at `now`.
    pub fn particles(&self, now: Instant) -> Vec<first::Particle> {
        let mut particles = Vec::new();
        for standing in self.standing.iter().filter(|standing| matches!(standing.kind, Kind::Puffs)) {
            let Some(tick) = passes(standing.start, now).checked_sub(1) else { continue };
            for &(born, speed) in &standing.puffs {
                let Some(p) = tick.checked_sub(born).filter(|&p| p < PUFF_LIFE) else { continue };
                let at = [standing.at[0], standing.at[1] + puff_height(speed, p), standing.at[2]];
                particles.push(first::Particle {
                    sprite: first::POISON_STAR,
                    at,
                    scale: 0.8,
                    alpha: burst::alpha(p, 70.0, 25.4, 254.0, 8, PUFF_LIFE),
                    spin: 3.0 * (p + 1) as f32,
                });
            }
        }
        particles
    }

    /// The colour a Berserk owner's body takes at `now`: #FA7878 held, then back to white.
    pub fn body_tint(&self, unit: Option<u32>, now: Instant) -> Option<[u8; 3]> {
        self.berserks.iter().filter(|berserk| berserk.unit == unit).find_map(|berserk| {
            let tick = passes(berserk.start, now).checked_sub(1)?;
            let fade = tick.saturating_sub(BERSERK_HELD) as u16 * 8;
            let tint = BERSERK_TINT.map(|channel| (u16::from(channel) + fade).min(255) as u8);
            (tint != [255; 3]).then_some(tint)
        })
    }

    /// Every wall to draw at `now`; `position_of` gives a unit's feet (None = the player).
    pub fn meshes(&self, now: Instant, position_of: impl Fn(Option<u32>) -> Option<Vector>) -> Vec<Wall> {
        let mut walls = Vec::new();
        for standing in self.standing.iter().filter(|standing| matches!(standing.kind, Kind::Funnels)) {
            if let Some(p) = passes(standing.start, now).checked_sub(1) {
                walls.push(Wall { texture: MAGIC_RED, vertices: funnels(standing.at, p), additive: false, forced: true });
            }
        }
        for berserk in &self.berserks {
            let (Some(feet), Some(tick)) = (position_of(berserk.unit), passes(berserk.start, now).checked_sub(1)) else { continue };
            if tick < CYLINDER_LIFE {
                let alpha = burst::alpha(tick, 0.0, 4.5, 90.0, 40, CYLINDER_LIFE);
                walls.push(Wall { texture: ALPHA_DOWN, vertices: cylinder(feet, 4.5, cylinder_height(tick), alpha), additive: false, forced: false });
            }
            for streak in &berserk.streaks {
                if let Some(p) = tick.checked_sub(streak.born).filter(|&p| p < STREAK_LIFE) {
                    walls.push(Wall { texture: STREAK, vertices: streak.blades(feet, p), additive: false, forced: false });
                }
            }
        }
        for defender in &self.defenders {
            let (Some(feet), Some(p)) = (position_of(defender.unit), passes(defender.start, now).checked_sub(1)) else { continue };
            if p < DEFENDER_TICKS {
                for k in 0..DEFENDER_BANDS.len() {
                    walls.push(Wall { texture: defender.texture, vertices: defender_band(feet, k, p + 1), additive: false, forced: false });
                }
            }
        }
        for ribbons in &self.ribbons {
            let look = ribbons.look;
            let place = ribbons.at.or_else(|| position_of(ribbons.unit));
            let (Some(feet), Some(p)) = (place, passes(ribbons.start, now).checked_sub(1)) else { continue };
            if p < look.ticks {
                walls.extend((0..4).map(|k| Wall { texture: look.texture, vertices: ribbon(look, feet, k, p + 1), additive: false, forced: look.forced }));
            }
        }
        for providence in &self.providences {
            let (Some(feet), Some(t)) = (position_of(providence.unit), passes(providence.start, now).checked_sub(1)) else { continue };
            if t < TUBE_TICKS {
                walls.extend((0..4).map(|k| Wall { texture: RING_YELLOW, vertices: tube(feet, k, t), additive: true, forced: false }));
            }
        }
        for curtains in &self.curtains {
            let (Some(feet), Some(t)) = (position_of(curtains.unit), passes(curtains.start, now).checked_sub(1)) else { continue };
            if t < CURTAIN_TICKS {
                walls.extend((0..4).map(|k| Wall { texture: RING_BLUE, vertices: curtain(feet, k, curtains.turns[k], t + 1), additive: true, forced: false }));
            }
        }
        for cross in &self.crosses {
            if let Some(p) = passes(cross.start, now).checked_sub(1) {
                walls.extend(cross.walls(p + 1));
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

    fn radius(vertex: &MeshVertex, at: Vector) -> f32 {
        f32::hypot(vertex.position[0] - at[0], vertex.position[2] - at[2])
    }

    #[test]
    fn fire_pillar_turns_three_funnels_six_degrees_a_tick() {
        let start = Instant::now();
        let at = [10.0, 2.0, 20.0];
        let mut walls = Walls::default();
        walls.keep(138, 7, at, start);
        let mesh = |tick: u32| walls.meshes(start + TICK * tick, |_| None);
        let wall = &mesh(0)[0];
        assert!(wall.texture == MAGIC_RED && wall.forced && !wall.additive);
        // Three funnels of 10 segments of 6 vertices; the radii and heights at the ground and the top.
        assert_eq!(wall.vertices.len(), 180);
        for (i, &(inner, outer, height)) in FUNNELS.iter().enumerate() {
            let funnel = &wall.vertices[60 * i..60 * (i + 1)];
            assert!(funnel.iter().filter(|vertex| vertex.uv[1] == 1.0).all(|vertex| near(radius(vertex, at), inner) && vertex.position[1] == at[1]));
            assert!(
                funnel.iter().filter(|vertex| vertex.uv[1] == 0.0).all(|vertex| near(radius(vertex, at), outer) && near(vertex.position[1], at[1] + height))
            );
        }
        assert!(wall.vertices.iter().all(|vertex| vertex.tint == [1.0, 1.0, 1.0, 240.0 / 255.0]));
        // 2° off at tick 0, 6° more each tick.
        let angle = |tick: u32| {
            let vertex = &mesh(tick)[0].vertices[0];
            (vertex.position[2] - at[2]).atan2(vertex.position[0] - at[0]).to_degrees().rem_euclid(360.0)
        };
        assert!(near(angle(0), 2.0) && near(angle(1), 8.0) && near(angle(10), 62.0));
        // It keeps while the unit stands, and goes with it.
        assert_eq!(walls.meshes(start + TICK * 5000, |_| None).len(), 1);
        walls.forget(7);
        assert!(walls.meshes(start, |_| None).is_empty());
    }

    #[test]
    fn venom_dust_lets_a_violet_puff_rise_every_five_ticks() {
        let start = Instant::now();
        let mut walls = Walls::default();
        walls.keep(171, 3, [1.0, 0.0, 2.0], start);
        let run = |walls: &mut Walls, tick: u32| {
            walls.advance(start + TICK * tick, |_| 0);
            walls.particles(start + TICK * tick)
        };
        // Draws all 0: speed 20/80 = 0.25. One puff at ticks 0, 5, 10…; none left after its 10.
        assert_eq!(run(&mut walls, 0).len(), 1);
        assert_eq!(run(&mut walls, 5).len(), 2);
        assert_eq!(run(&mut walls, 9).len(), 2);
        assert_eq!(run(&mut walls, 10).len(), 2, "the first ended at its tick 9, the third begins");
        // Up 7.25 × 0.25 after tick 9; alpha 95 at tick 0, 254 at 7, 127 at 8, 0 at 9; turned 3° (p + 1); scale 0.8.
        let first = |tick: u32| {
            let mut again = Walls::default();
            again.keep(171, 3, [1.0, 0.0, 2.0], start);
            again.advance(start + TICK * tick, |_| 0);
            again.particles(start + TICK * tick)[0]
        };
        assert!(near(first(9).at[1], 7.25 * 0.25) && near(first(0).at[1], 0.25 * (1.0 - 2.0 / 40.0)));
        assert_eq!([0, 7, 8, 9].map(|tick| first(tick).alpha), [95.0, 254.0, 127.0, 0.0]);
        assert_eq!([0, 9].map(|tick| first(tick).spin), [3.0, 30.0]);
        assert!(first(0).scale == 0.8 && first(0).at[0] == 1.0 && first(0).at[2] == 2.0 && first(0).sprite == first::POISON_STAR);
        // A speed from the draws: (R(20) + 20)/80.
        let mut fast = Walls::default();
        fast.keep(171, 4, [0.0; 3], start);
        fast.advance(start, |n| n - 1);
        assert!(near(fast.particles(start)[0].at[1], 0.4875 * (1.0 - 2.0 / 40.0)));
    }

    #[test]
    fn berserk_tints_then_raises_a_cylinder_and_24_streaks() {
        let start = Instant::now();
        let mut walls = Walls::default();
        walls.berserk(Some(5), start, |_| 0);
        // #FA7878 to tick 150, then 8 a channel a tick back to white.
        let tint = |tick: u32| walls.body_tint(Some(5), start + TICK * tick);
        assert_eq!([0, 150].map(tint), [Some([250, 120, 120]); 2]);
        assert_eq!((tint(151), tint(152), tint(160)), (Some([255, 128, 128]), Some([255, 136, 136]), Some([255, 200, 200])));
        assert_eq!(tint(167), None);
        assert!(walls.body_tint(Some(6), start).is_none() && walls.body_tint(None, start).is_none());
        // The cylinder: 11.01 up at p = 0, 32.1 at 19, 72.75 at 49; alpha 4 (4.5 cut), 90 at 19 and 39, 81 at 40, 0 at 49; gone at 50.
        assert!(near(cylinder_height(0), 11.01) && near(cylinder_height(19), 32.1) && near(cylinder_height(49), 72.75));
        let feet = [3.0, 1.0, 4.0];
        let cylinders = |tick: u32| walls.meshes(start + TICK * tick, |_| Some(feet)).into_iter().filter(|wall| wall.texture == ALPHA_DOWN).collect::<Vec<_>>();
        assert_eq!(cylinders(0).len(), 1);
        let alpha = |tick: u32| (cylinders(tick)[0].vertices[0].tint[3] * 255.0).round();
        assert_eq!([0, 19, 39, 40, 49].map(alpha), [4.0, 90.0, 90.0, 81.0, 0.0]);
        assert!(cylinders(50).is_empty());
        assert!(cylinders(10)[0].vertices.iter().all(|vertex| (radius(vertex, feet) - 4.5).abs() < 0.01));
        // 24 streaks, one at ticks 16, 20 … 108: none at 12, one at 16, 24 across them all.
        let streaks = |tick: u32| walls.meshes(start + TICK * tick, |_| Some(feet)).into_iter().filter(|wall| wall.texture == STREAK).count();
        assert_eq!((streaks(12), streaks(16), streaks(20)), (0, 1, 2));
        assert_eq!(walls.berserks[0].streaks.len(), 24);
        assert_eq!(walls.berserks[0].streaks.last().unwrap().born, 108);
        assert_eq!(streaks(112), 9, "the ones born from 76 to 108 still rise");
        assert_eq!(streaks(148), 0);
        // A streak: 2 out at yaw 0, 20/60 a tick up, half-length 3.0; alpha 9 at p = 0, 180 at 19, 171 at 20, 0 at 39; red.
        let blades = |tick: u32| walls.meshes(start + TICK * tick, |_| Some(feet)).into_iter().find(|wall| wall.texture == STREAK).unwrap().vertices;
        let vertices = blades(16 + 19);
        assert_eq!(vertices.len(), 12);
        assert!(near(vertices[0].position[1], feet[1] + 20.0 * (20.0 / 60.0) + 3.0) && near(vertices[0].position[2], feet[2] + 2.0));
        assert_eq!(vertices[0].tint, [240.0 / 255.0, 0.0, 0.0, 180.0 / 255.0]);
        assert_eq!([0, 20, 39].map(|p| (blades(16 + p)[0].tint[3] * 255.0).round()), [9.0, 171.0, 0.0]);
    }

    #[test]
    fn defender_turns_four_humped_walls() {
        let start = Instant::now();
        let mut walls = Walls::default();
        walls.defender(None, start, RING_BLACK);
        let at = [0.0; 3];
        let bands = |tick: u32| walls.meshes(start + TICK * tick, |_| Some(at));
        assert_eq!(bands(0).len(), 4);
        assert!(bands(0).iter().all(|wall| wall.texture == RING_BLACK && !wall.additive && wall.vertices.len() == 120));
        // Zero at the ends, h (1 + 0.3 sin θ) in the middle (θ = a + 90 k for the first two, 2 (a + 45 k) for the others).
        assert!(near(defender_top(0, 0, 5), 0.0) && near(defender_top(0, 20, 5), 0.0));
        assert!(near(defender_top(0, 10, 0), 40.0 * 1.0) && near(defender_top(0, 10, 5), 40.0 * (1.0 + 0.3 * 5.0_f32.to_radians().sin())));
        assert!(
            near(defender_top(2, 10, 5), 38.0 * (1.0 + 0.3 * 190.0_f32.to_radians().sin()))
                && near(defender_top(1, 10, 5), 39.0 * (1.0 + 0.3 * 95.0_f32.to_radians().sin()))
        );
        // The turn: band 1 after 10 ticks is 290°; band 0 starts at 180°.
        assert!(near(defender_turn(1, 10), 290.0) && near(defender_turn(0, 0), 180.0) && near(defender_turn(2, 360), 0.0));
        // Alpha a up to 80, 80 to 179, 75 at 180, 0 at 195.
        assert_eq!([1, 80, 179, 180, 194, 195, 199].map(defender_alpha), [1.0, 80.0, 80.0, 75.0, 5.0, 0.0, 0.0]);
        // Drawn ticks 0…198, gone at 199 and removed by 200.
        assert_eq!(bands(198).len(), 4);
        assert!(bands(199).is_empty());
        walls.advance(start + TICK * 200, |_| 0);
        assert!(walls.defenders.is_empty());
        // It follows its unit: off with it.
        let mut other = Walls::default();
        other.defender(Some(3), start, RING_BLACK);
        assert!(other.meshes(start, |_| None).is_empty());
    }

    #[test]
    fn grand_cross_lights_four_arcs_and_two_bars_where_the_caster_stood() {
        let start = Instant::now();
        let at = [50.0, 4.0, 60.0];
        let mut walls = Walls::default();
        walls.grand_cross(at, start);
        // Arcs: radius 19.89 and top 120 at a = 90; alpha 90 at a = 9, 89 at 10, 0 at 99.
        assert!(near(arc_size(1).0, 19.89) && near(arc_size(90).1, 120.0) && near(arc_size(90).0, 19.0));
        assert_eq!([1, 9, 10, 98, 99].map(arc_alpha), [10.0, 90.0, 89.0, 1.0, 0.0]);
        // Bars: L, W = 24.005, 3.205 at a = 1; walls 60 at a = 90; alpha 81 at a = 9, 0 at 90.
        let (length, width, wall) = bar_size(1);
        assert!(near(length, 24.005) && near(width, 3.205) && near(wall, 60.0 * 1.0_f32.to_radians().sin()) && near(bar_size(90).2, 60.0));
        assert_eq!([1, 9, 10, 89, 90].map(bar_alpha), [9.0, 81.0, 80.0, 1.0, 0.0]);
        // At tick p = a − 1: four arcs of 8 quads, then per bar a curtain and the floor twice.
        let mesh = |tick: u32| walls.meshes(start + TICK * tick, |_| None);
        let first = mesh(0);
        assert_eq!(first.len(), 4 + 2 * 3);
        assert!(first[..4].iter().all(|arc| arc.texture == RING_WHITE && arc.additive && arc.vertices.len() == 6 * 8));
        assert!(first[4..].iter().all(|bar| bar.texture == RING_YELLOW));
        let curtains: Vec<_> = first[4..].iter().filter(|bar| !bar.additive).collect();
        let floors: Vec<_> = first[4..].iter().filter(|bar| bar.additive).collect();
        assert_eq!((curtains.len(), floors.len()), (2, 4));
        assert_eq!(curtains[0].vertices.len(), 24);
        // The floor is 1 up, drawn twice with v turned over, each at half the alpha (9 → 4).
        assert!(floors[0].vertices.iter().all(|vertex| vertex.position[1] == at[1] + 1.0 && near(vertex.tint[3], 4.0 / 255.0)));
        assert_ne!(floors[0].vertices[0].uv, floors[1].vertices[0].uv);
        assert_eq!(floors[0].vertices[0].uv[1] + floors[1].vertices[0].uv[1], 1.0);
        // The bars lie along x and along z.
        let spread = |wall: &Wall, axis: usize| {
            let values: Vec<f32> = wall.vertices.iter().map(|vertex| vertex.position[axis]).collect();
            values.iter().fold(f32::MIN, |a, &b| a.max(b)) - values.iter().fold(f32::MAX, |a, &b| a.min(b))
        };
        assert!(near(spread(floors[0], 0), 2.0 * 24.005) && near(spread(floors[0], 2), 2.0 * 3.205));
        assert!(near(spread(floors[2], 2), 2.0 * 24.005) && near(spread(floors[2], 0), 2.0 * 3.205));
        // The arcs sit 23 from the spot; the bars are drawn on ages 1…89, the arcs on 1…98.
        assert!(first[0].vertices.iter().all(|vertex| (vertex.position[0] - at[0]).abs() > 3.0 || (vertex.position[2] - at[2]).abs() > 3.0));
        assert_eq!(mesh(88).len(), 4 + 6);
        assert_eq!(mesh(89).len(), 4);
        assert_eq!(mesh(97).len(), 4);
        assert!(mesh(98).is_empty());
        // A second packet adds a second cross, and neither follows the caster.
        let mut twice = Walls::default();
        twice.grand_cross(at, start);
        twice.grand_cross(at, start);
        assert_eq!(twice.meshes(start, |_| Some([0.0; 3])).len(), 2 * (4 + 6));
        assert_eq!(walls.meshes(start, |_| Some([0.0; 3])).len(), 4 + 6);
        walls.advance(start + TICK * 99, |_| 0);
        assert!(walls.crosses.is_empty());
    }

    #[test]
    fn providence_shoots_four_leaning_tubes_up() {
        let start = Instant::now();
        let mut walls = Walls::default();
        walls.providence(Some(4), start);
        // p = 120 sin(t + 1)°: 2.09 at tick 0, 60 at 29, 116.44 at 75; alpha 40 to tick 49, 39 at 50, 14 at 75.
        assert!(near(tube_height(0), 2.094) && near(tube_height(29), 60.0) && near(tube_height(75), 116.44));
        assert_eq!([0, 49, 50, 75].map(tube_alpha), [40.0, 40.0, 39.0, 14.0]);
        let feet = [5.0, 1.0, 6.0];
        let at = |tick: u32| walls.meshes(start + TICK * tick, |_| Some(feet));
        assert!(at(75).len() == 4 && at(76).is_empty());
        assert!(at(0).iter().all(|wall| wall.texture == RING_YELLOW && wall.additive && wall.vertices.len() == 120));
        // The tops of tube 0 at tick 75: radius 6 + 116.44 cos 83°, height 116.44 sin 83°; the feet at radius 6.
        let tube0 = &at(75)[0];
        let top = tube0.vertices.iter().find(|vertex| vertex.uv[1] == 0.0).unwrap();
        assert!(near(radius(top, feet), 6.0 + 116.44 * 83.0_f32.to_radians().cos()) && near(top.position[1], feet[1] + 116.44 * 83.0_f32.to_radians().sin()));
        let foot = tube0.vertices.iter().find(|vertex| vertex.uv[1] == 1.0).unwrap();
        assert!(near(radius(foot, feet), 6.0));
        assert!(near(radius(at(75)[3].vertices.iter().find(|vertex| vertex.uv[1] == 1.0).unwrap(), feet), 7.5));
        assert!((at(75)[0].vertices[0].tint[3] * 255.0).round() == 14.0);
        // It follows its unit.
        assert!(walls.meshes(start, |_| None).is_empty());
    }

    #[test]
    fn reflect_shield_is_defenders_walls_in_yellow() {
        let start = Instant::now();
        let mut walls = Walls::default();
        walls.defender(None, start, RING_YELLOW);
        let at = |tick: u32| walls.meshes(start + TICK * tick, |_| Some([0.0; 3]));
        assert_eq!(at(0).len(), 4);
        assert!(at(0).iter().all(|wall| wall.texture == RING_YELLOW && !wall.additive));
        let alpha = |tick: u32| (at(tick)[0].vertices[0].tint[3] * 255.0).round();
        assert_eq!([0, 79, 179, 194].map(alpha), [1.0, 80.0, 75.0, 0.0]);
        assert!(at(198).len() == 4 && at(199).is_empty());
    }

    #[test]
    fn absorb_spirits_waves_two_curtains_and_two_skirts() {
        let start = Instant::now();
        let mut walls = Walls::default();
        let mut draws = [200, 300].into_iter();
        walls.curtains(Some(2), start, |_| draws.next().unwrap());
        assert_eq!(walls.curtains[0].turns, [0.0, 180.0, 200.0, 300.0]);
        // Alpha 75 at ticks 14 and 38, 73 at 39, 1 at 75, 0 at 76.
        assert_eq!([15, 39, 40, 76, 77].map(curtain_alpha), [75.0, 75.0, 73.0, 1.0, 0.0]);
        assert_eq!(curtain_alpha(1), 5.0);
        // The profile: k 0 at a = 1, s = 0: 0.804; at a = 90, s = 10: 39.87; k 2 at a = 100, s = 5: 14.28.
        assert!(near(curtain_top(0, 0, 1), 0.804) && near(curtain_top(0, 10, 90), 39.87) && near(curtain_top(2, 5, 100), 14.28));
        let feet = [0.0, 3.0, 0.0];
        let at = |tick: u32| walls.meshes(start + TICK * tick, |_| Some(feet));
        let first = at(0);
        assert!(first.len() == 4 && first.iter().all(|wall| wall.texture == RING_BLUE && wall.additive && wall.vertices.len() == 120));
        assert_eq!(first[0].vertices[0].tint[..3], CURTAIN_RGB);
        assert_eq!(at(98).len(), 4);
        assert!(at(99).is_empty());
        // The top of band 2 at a = 100 sits 5 + 14.28 cos 50° out and 14.28 sin 50° up; the feet 5 out.
        let late = at(98);
        let wall = &late[2];
        let foot = wall.vertices.iter().find(|vertex| vertex.uv == [0.0, 1.0]).unwrap();
        assert!(near(radius(foot, feet), 5.0));
        // The bands never turn: the same corner at two ticks.
        assert_eq!(at(10)[0].vertices[0].uv, at(20)[0].vertices[0].uv);
        let a = at(10)[1].vertices[0].position;
        let b = at(20)[1].vertices[0].position;
        assert!(near(a[0], b[0]) && near(a[2], b[2]), "the foot of the first point stays put");
    }

    #[test]
    fn steel_body_spins_four_crescent_ribbons() {
        let start = Instant::now();
        let mut walls = Walls::default();
        walls.ribbons(None, start);
        // Alpha at t = 1, 10, 50, 110 is 20, 200, 120, 0.
        assert_eq!([1, 10, 50, 110].map(|t| ribbon_alpha(&STEEL_BODY_RIBBONS, t)), [20.0, 200.0, 120.0, 0.0]);
        // Radius (k + 1) + 0.1 t: 5.0 for k = 3 at t = 10; lean 30° at t = 50; turn 150° for k = 0 at t = 50.
        assert!(
            near(ribbon_shape(&STEEL_BODY_RIBBONS, 3, 10).0, 5.0)
                && near(ribbon_shape(&STEEL_BODY_RIBBONS, 0, 1).1, 79.0)
                && near(ribbon_shape(&STEEL_BODY_RIBBONS, 0, 50).1, 30.0)
                && near(ribbon_shape(&STEEL_BODY_RIBBONS, 0, 50).2, 150.0)
        );
        assert!(near(ribbon_shape(&STEEL_BODY_RIBBONS, 1, 10).2, 120.0) && near(ribbon_shape(&STEEL_BODY_RIBBONS, 0, 110).0, 12.0));
        // The crescent: zero at both ends, 25.07 at s = 10 for k = 0, t = 1, the most halfway round.
        assert!(
            near(ribbon_top(&STEEL_BODY_RIBBONS, 0, 0, 1), 0.0)
                && near(ribbon_top(&STEEL_BODY_RIBBONS, 0, 20, 1), 0.0)
                && near(ribbon_top(&STEEL_BODY_RIBBONS, 0, 10, 1), 25.0 * (1.0 + 0.15 * 1.0_f32.to_radians().sin()))
        );
        let feet = [2.0, 1.0, 3.0];
        let at = |tick: u32| walls.meshes(start + TICK * tick, |_| Some(feet));
        let first = at(0);
        assert_eq!(first.len(), 4);
        assert!(first.iter().all(|wall| wall.texture == RING_YELLOW && !wall.additive && wall.forced && wall.vertices.len() == 120));
        assert_eq!(at(108).len(), 4);
        assert_eq!(at(109).len(), 4, "age 110 is alpha 0 and still drawn");
        assert!(at(110).is_empty());
        walls.advance(start + TICK * 111, |_| 0);
        assert!(walls.ribbons.is_empty());
    }

    #[test]
    fn ganbantein_spins_four_white_crescents_over_the_cell() {
        let start = Instant::now();
        let mut walls = Walls::default();
        walls.ganbantein([10.0, 2.0, 20.0], start);
        // Alpha at t = 1, 10, 20, 30, 34, 35: 20, 200, 120, 40, 8, 0.
        assert_eq!([1, 10, 20, 30, 34, 35].map(|t| ribbon_alpha(&GANBANTEIN, t)), [20.0, 200.0, 120.0, 40.0, 8.0, 0.0]);
        // Radius for k = 3 at t = 10 is 7.0; lean 69 at t = 1 and 40 from t = 30; turn for k = 1 at t = 2 is 100°.
        assert!(
            near(ribbon_shape(&GANBANTEIN, 3, 10).0, 7.0) && near(ribbon_shape(&GANBANTEIN, 0, 1).1, 69.0) && near(ribbon_shape(&GANBANTEIN, 0, 30).1, 40.0)
        );
        assert!(near(ribbon_shape(&GANBANTEIN, 1, 2).2, 100.0));
        // 15 high: s = 10 reaches 15 (1 + 0.15 sin).
        assert!(near(ribbon_top(&GANBANTEIN, 0, 10, 0), 15.0));
        // Four white bands at the cell, not following anyone, normal blend; drawn on ticks 0…33, not 34.
        let at = |tick: u32| walls.meshes(start + TICK * tick, |_| None);
        assert_eq!(at(0).len(), 4);
        assert!(at(0).iter().all(|wall| wall.texture == RING_WHITE && !wall.additive && !wall.forced && wall.vertices.len() == 120));
        assert_eq!(at(33).len(), 4);
        assert!(at(34).is_empty());
        walls.advance(start + TICK * 36, |_| 0);
        assert!(walls.ribbons.is_empty());
    }

    #[test]
    fn storm_blast_ribbons_are_half_as_bright_and_open_quicker() {
        let look = &STORM_BLAST;
        // Alpha at t = 1, 10, 11, 59, 60: 10, 100, 98, 2, 0.
        assert_eq!([1, 10, 11, 59, 60].map(|t| ribbon_alpha(look, t)), [10.0, 100.0, 98.0, 2.0, 0.0]);
        // Band 3 at t = 10 is 7 out; the lean is 79, 40 and 40 at t = 1, 40, 60; the turn is 90k + 5t.
        assert!(near(ribbon_shape(look, 3, 10).0, 7.0));
        assert_eq!([1, 40, 60].map(|t| ribbon_shape(look, 0, t).1), [79.0, 40.0, 40.0]);
        assert!(near(ribbon_shape(look, 1, 4).2, 110.0));
        // Four bands on ages 1 to 59, following the unit; the Steel Body ribbons keep their 200.
        let start = Instant::now();
        let mut walls = Walls::default();
        walls.storm_blast(Some(3), start);
        assert_eq!(walls.meshes(start, |_| Some([0.0; 3])).len(), 4);
        assert_eq!(walls.meshes(start + TICK * 58, |_| Some([0.0; 3])).len(), 4);
        assert!(walls.meshes(start + TICK * 59, |_| Some([0.0; 3])).is_empty());
        assert_eq!(ribbon_alpha(&STEEL_BODY_RIBBONS, 10), 200.0);
    }
}
