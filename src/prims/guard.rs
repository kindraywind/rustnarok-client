//! The guard and poison skills' code-made shapes, a 16 ms tick at a time: Fog Wall's grey boxes over each cell (405), Full Strip's
//! flying shields and swords (495), Preserve's green dome of sparkle tiles (496), and the body flickers of Enchant Deadly Poison,
//! Preserve and Spiral Pierce's cast (493, 496, 502).
//!
//! A shape's own age n counts from 1 at its first tick; t is the ticks since the effect started.

use std::time::Instant;

use super::ground::Potion;
use super::passes;
use super::shards::coin_alpha;
use super::walls::Wall;
use crate::world::{MeshVertex, Vector};

pub const FOG: &str = "data\\texture\\effect\\ring_white.tga";
pub const SHIELD: &str = "data\\texture\\effect\\shield.bmp";
pub const SWORD: &str = "data\\texture\\effect\\sword.bmp";
/// Weapon Crush's eight weapon icons, one a prim.
pub const WEAPONS: [&str; 8] = [
    "data\\texture\\유저인터페이스\\item\\루드라의활.bmp",
    "data\\texture\\유저인터페이스\\item\\아트로스의흉기.bmp",
    "data\\texture\\유저인터페이스\\item\\모닝스타.bmp",
    "data\\texture\\유저인터페이스\\item\\바스타드소드.bmp",
    "data\\texture\\유저인터페이스\\item\\글레이브.bmp",
    "data\\texture\\유저인터페이스\\item\\블레이드휩.bmp",
    "data\\texture\\유저인터페이스\\item\\드래곤슬레이어.bmp",
    "data\\texture\\유저인터페이스\\item\\배틀액스.bmp",
];
/// Strip Accessory's eight icons, one a prim.
pub const ACCESSORIES: [&str; 8] = [
    "data\\texture\\유저인터페이스\\item\\갈색반지.bmp",
    "data\\texture\\유저인터페이스\\item\\금반지.bmp",
    "data\\texture\\유저인터페이스\\item\\금방울.bmp",
    "data\\texture\\유저인터페이스\\item\\꽃반지.bmp",
    "data\\texture\\유저인터페이스\\item\\낡은펜던트.bmp",
    "data\\texture\\유저인터페이스\\item\\눈구슬반지.bmp",
    "data\\texture\\유저인터페이스\\item\\상자.bmp",
    "data\\texture\\유저인터페이스\\item\\수제초콜렛.bmp",
];
/// Detonator's flush (#9B9BFF).
const LILAC: [u8; 3] = [0x9B, 0x9B, 0xFF];
pub const TILE: &str = "data\\texture\\effect\\a01.bmp";
pub const HOLY_TILE: &str = "data\\texture\\effect\\성체강복.tga";
/// Fog Wall's box colour (#505050), the dome's three shells (#CDFFCD, #9BFF9B, #E1FFE1).
const FOG_TINT: f32 = 80.0 / 255.0;
const SHELLS: [([f32; 3], bool); 3] =
    [([205.0 / 255.0, 1.0, 205.0 / 255.0], true), ([155.0 / 255.0, 1.0, 155.0 / 255.0], true), ([225.0 / 255.0, 1.0, 225.0 / 255.0], false)];
/// The dome's ρ and k offsets of each shell from its radius h = 10, and the rows' half-sizes, latitudes and tilts.
const DOME_HEIGHT: f32 = 10.0;
const ROWS: [(f32, f32, f32); 3] = [(315.0, 45.0, 2.0), (0.0, 90.0, 3.0), (45.0, 135.0, 2.0)];
const COLUMNS: [f32; 5] = [0.0, 45.0, 90.0, 135.0, 180.0];
/// Full Strip: 8 prims of 4 elements, alternating shield and sword.
const STRIP_ELEMENTS: usize = 32;
pub const STRIP_TINT: [f32; 3] = [250.0 / 255.0, 100.0 / 255.0, 100.0 / 255.0];
const STRIP_TICKS: u32 = 300;

/// One of Fog Wall's boxes over a cell.
struct Fog {
    id: u32,
    at: Vector,
    start: Instant,
    angle: f32,
}

/// One of Full Strip's pieces: a shield (half-diagonal 5) or a sword (9) flying out from where the target stood.
struct Piece {
    half: f32,
    icon: &'static str,
    wait: i32,
    azimuth: f32,
    elevation: f32,
    spin: f32,
    /// Strip Accessory's: the turn of the upright plane it flies in.
    turn: f32,
}

struct Strip {
    at: Vector,
    start: Instant,
    pieces: Vec<Piece>,
    /// Strip Accessory's: upright squares in turning vertical planes, seen through the ground, not squares facing the camera.
    upright: bool,
}

/// Preserve's dome round the caster.
struct Dome {
    unit: Option<u32>,
    start: Instant,
    /// The facing at tick 0 the dome's first turn comes from.
    facing: f32,
    /// Neutral Barrier's: its additive shells use a holier tile.
    holy: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Flicker {
    /// Enchant Deadly Poison's magenta (ticks 0 to 80).
    Poison,
    /// A white or yellow strobe, added on every other tick from `first` to `last` (Preserve's 10 to 30, Analyze's 10 to 90).
    Strobe([u8; 3], u32, u32),
    /// Spiral Pierce's cast: outline copies and yellow for its length, at most 141 ticks.
    Pierce(u32),
    /// Memorize's yellow (ticks 10 to 35).
    Memorize,
    /// Double Casting's red strobe in a white glow (ticks 10 to 50).
    DoubleCast,
    /// Refresh's and Abundance's green flicker, the poisons' magenta, in outline copies (ticks 0 to 30).
    Outlined([u8; 3], u32),
    /// Detonator's lilac flush (ticks 10 to 33).
    Concentration,
    /// Sonic Wave's blue strobe: outline copies added on in #0505FF on even ticks 4 to 52, white on the odd ones.
    Sonic,
    /// Deadly Infect's black and white blink: white on the odd ticks, black on the even, ticks 6 to 60.
    Blink,
}

/// Chase Walk's cast turning its caster.
struct Walk {
    unit: Option<u32>,
    start: Instant,
    length: u32,
    done: u32,
}

/// What Chase Walk's cast turns its caster on tick t (from 0): faster as the cast goes on.
pub fn spin_step(t: u32) -> f32 {
    match t {
        0..=15 => 12.0,
        16..=30 => 18.0,
        31..=40 => 24.0,
        _ => 48.0,
    }
}

struct Body {
    unit: Option<u32>,
    start: Instant,
    flicker: Flicker,
}

#[derive(Default)]
pub struct Guard {
    fogs: Vec<Fog>,
    strips: Vec<Strip>,
    domes: Vec<Dome>,
    bodies: Vec<Body>,
    walks: Vec<Walk>,
}

/// A fog box's height at its age n: it breathes between 9.6 and 32 and rises over the first 89 ticks.
pub fn fog_height(start_angle: f32, n: u32) -> f32 {
    let angle = (start_angle + n as f32) % 360.0;
    let height = 32.0 * angle.to_radians().sin() * 0.35 + 32.0 * 0.65;
    if n < 90 { height * (n as f32).to_radians().sin() } else { height }
}

/// The dome's alpha at its age n: 20 a tick to 100, held to 26, then 6 less a tick from 27.
pub fn dome_alpha(n: u32) -> f32 {
    match n {
        1..=5 => 20.0 * n as f32,
        6..=26 => 100.0,
        _ => (100.0 - 6.0 * (n - 26) as f32).max(0.0),
    }
}

/// How far the dome's outer shell has swollen at n: 0.1 a tick from 27.
pub fn dome_grow(n: u32) -> f32 {
    0.1 * n.saturating_sub(26) as f32
}

/// The dome's turn at n from the facing at tick 0: facing + 270 on the first tick, then 10 more a tick.
pub fn dome_turn(facing: f32, n: u32) -> f32 {
    ((facing.trunc() + 270.0) + 10.0 * (n - 1) as f32).rem_euclid(360.0)
}

/// A body's flags and colour at tick e for a flicker, or None while it does nothing.
fn flicker_at(flicker: Flicker, e: u32) -> Option<(u32, Option<[u8; 3]>)> {
    use crate::actor::{GLOW_ADDITIVE, GLOW_OUTLINE};
    let even = e.is_multiple_of(2);
    match flicker {
        Flicker::Poison => (e <= 80).then_some(if even { (GLOW_ADDITIVE, Some([0xFF, 0x00, 0xFF])) } else { (0, Some([255; 3])) }),
        Flicker::Strobe(rgb, first, last) => (first..=last).contains(&e).then_some(if even { (GLOW_ADDITIVE, Some(rgb)) } else { (0, Some([255; 3])) }),
        Flicker::Memorize => (10..=35).contains(&e).then_some(if even { (GLOW_ADDITIVE, Some([0xFA, 0xFA, 0x64])) } else { (0, Some([255; 3])) }),
        Flicker::DoubleCast => {
            (10..=50).contains(&e).then_some(if even { (GLOW_OUTLINE | GLOW_ADDITIVE, Some([0xFF, 0, 0])) } else { (GLOW_OUTLINE, Some([255; 3])) })
        }
        Flicker::Outlined(rgb, last) => (e <= last).then_some(if even { (GLOW_OUTLINE | GLOW_ADDITIVE, Some(rgb)) } else { (GLOW_OUTLINE, Some([255; 3])) }),
        Flicker::Sonic => (4..=52).contains(&e).then_some(if even { (GLOW_OUTLINE | GLOW_ADDITIVE, Some([5, 5, 0xFF])) } else { (0, Some([255; 3])) }),
        Flicker::Blink => (6..=60).contains(&e).then_some((0, Some(if even { [0; 3] } else { [255; 3] }))),
        Flicker::Concentration => (10..=33).contains(&e).then_some((if even { GLOW_ADDITIVE } else { 0 }, Some(LILAC))),
        Flicker::Pierce(length) => {
            (e < length.min(141)).then_some(if even { (GLOW_OUTLINE | GLOW_ADDITIVE, Some([0xFA, 0xFA, 0x64])) } else { (GLOW_OUTLINE, Some([255; 3])) })
        }
    }
}

/// What a flash left on a body fades back from, by 8 a channel a tick.
fn fade_back(from: [u8; 3], ticks: u32) -> Option<[u8; 3]> {
    let drift = (ticks * 8) as u16;
    let tint = from.map(|channel| (u16::from(channel) + drift).min(255) as u8);
    (tint != [255; 3]).then_some(tint)
}

impl Fog {
    fn walls(&self, n: u32) -> Wall {
        let height = fog_height(self.angle, n);
        let (half, up) = (2.5, 1.0);
        let corners = [[half, half], [half, -half], [-half, -half], [-half, half]];
        let mut vertices = Vec::new();
        // The four sides at alpha 30, then the two diagonals at 60.
        let walls = [(0, 1, 30.0), (1, 2, 30.0), (2, 3, 30.0), (3, 0, 30.0), (0, 2, 60.0), (1, 3, 60.0)];
        for (first, second, alpha) in walls {
            let tint = [FOG_TINT, FOG_TINT, FOG_TINT, alpha / 255.0];
            let point = |corner: [f32; 2], rise: f32, uv: [f32; 2]| MeshVertex {
                position: [self.at[0] + corner[0], self.at[1] + rise, self.at[2] + corner[1]],
                uv,
                tint,
            };
            let [a, b, c, d] = [
                point(corners[first], up, [0.0, 1.0]),
                point(corners[second], up, [1.0, 1.0]),
                point(corners[second], up + height, [1.0, 0.0]),
                point(corners[first], up + height, [0.0, 0.0]),
            ];
            vertices.extend([a, b, c, a, c, d]);
        }
        Wall { texture: FOG, vertices, additive: false, forced: false }
    }
}

impl Dome {
    /// The dome's 45 quads at its age n: 15 tiles in three shells, their corners round the caster's feet.
    fn walls(&self, feet: Vector, n: u32) -> Vec<Wall> {
        let alpha = dome_alpha(n) / 255.0;
        let turn = dome_turn(self.facing, n);
        let grow = dome_grow(n);
        let mut walls = Vec::new();
        for (shell, &(rgb, additive)) in SHELLS.iter().enumerate() {
            let (radius, lift) = match shell {
                0 => (DOME_HEIGHT, DOME_HEIGHT + 2.0),
                1 => (DOME_HEIGHT + 0.2, DOME_HEIGHT),
                _ => (DOME_HEIGHT + 0.4 + grow, DOME_HEIGHT),
            };
            let tint = [rgb[0], rgb[1], rgb[2], alpha];
            let mut vertices = Vec::new();
            for &(latitude, tilt, half) in &ROWS {
                let (lat_sin, lat_cos) = latitude.to_radians().sin_cos();
                let (tilt_sin, tilt_cos) = tilt.to_radians().sin_cos();
                for &column in &COLUMNS {
                    let (a_sin, a_cos) = ((column + turn - 90.0).rem_euclid(360.0)).to_radians().sin_cos();
                    let (b_sin, b_cos) = ((column + turn - 180.0).rem_euclid(360.0)).to_radians().sin_cos();
                    // The tile's centre; the game's y points down, so the client's up is the negative of it.
                    let centre = [radius * lat_cos * a_cos, -(radius * lat_sin - lift), radius * lat_cos * a_sin];
                    let corner = |x: f32, y: f32, z: f32, uv: [f32; 2]| MeshVertex {
                        position: [feet[0] + centre[0] + x * b_cos - z * b_sin, feet[1] + centre[1] - y, feet[2] + centre[2] + z * b_cos + x * b_sin],
                        uv,
                        tint,
                    };
                    let (up, deep) = (half * tilt_sin, half * tilt_cos);
                    let [a, b, c, d] = [
                        corner(half, up, deep, [0.0, 1.0]),
                        corner(-half, up, deep, [1.0, 1.0]),
                        corner(-half, -up, -deep, [1.0, 0.0]),
                        corner(half, -up, -deep, [0.0, 0.0]),
                    ];
                    vertices.extend([a, b, c, a, c, d]);
                }
            }
            let texture = if self.holy && additive { HOLY_TILE } else { TILE };
            walls.push(Wall { texture, vertices, additive, forced: false });
        }
        walls
    }
}

impl Guard {
    /// Fog Wall's box over the cell `at` of skill unit `id`; its start angle from `rand(n)` (0 to n − 1).
    pub fn keep_fog(&mut self, id: u32, at: Vector, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        self.fogs.push(Fog { id, at, start: now, angle: rand(360) as f32 });
    }

    pub fn forget(&mut self, id: u32) {
        self.fogs.retain(|fog| fog.id != id);
    }

    /// Full Strip's pieces flying out from the target's feet `at`: 8 prims of 4, alternating shield and sword; each piece's
    /// choices from `rand(n)` in the order the game draws them (its wait, azimuth, elevation, spin, and one that is not used).
    pub fn strip(&mut self, at: Vector, now: Instant, rand: impl FnMut(u32) -> u32) {
        self.burst(at, now, false, |prim| if prim % 2 == 0 { (SHIELD, 5.0) } else { (SWORD, 9.0) }, rand);
    }

    /// Strip Accessory's 32 red icons in upright planes, eight prims of four, their fifth draw each the plane's turn.
    pub fn accessory(&mut self, at: Vector, now: Instant, rand: impl FnMut(u32) -> u32) {
        self.burst(at, now, true, |prim| (ACCESSORIES[prim], if prim == 6 { 4.0 } else { 5.0 }), rand);
    }

    /// Weapon Crush's 32 weapon icons, four of each of eight, the same way, all with a half-diagonal of 5.
    pub fn weapon_crush(&mut self, at: Vector, now: Instant, rand: impl FnMut(u32) -> u32) {
        self.burst(at, now, false, |prim| (WEAPONS[prim], 5.0), rand);
    }

    fn burst(&mut self, at: Vector, now: Instant, upright: bool, icon_of: impl Fn(usize) -> (&'static str, f32), mut rand: impl FnMut(u32) -> u32) {
        let mut pieces = Vec::with_capacity(STRIP_ELEMENTS);
        for prim in 0..8 {
            for _ in 0..4 {
                let wait = -18 - rand(30) as i32;
                let (azimuth, elevation, spin) = (rand(360) as f32, rand(360) as f32, rand(360) as f32);
                let turn = rand(360) as f32;
                let (icon, half) = icon_of(prim);
                pieces.push(Piece { half, icon, wait, azimuth, elevation, spin, turn });
            }
        }
        self.strips.push(Strip { at, start: now, pieces, upright });
    }

    /// Preserve's dome on `unit`, facing `facing` degrees.
    pub fn preserve(&mut self, unit: Option<u32>, facing: f32, now: Instant) {
        self.domes.push(Dome { unit, start: now, facing, holy: false });
        self.bodies.push(Body { unit, start: now, flicker: Flicker::Strobe([0xFA; 3], 10, 30) });
    }

    /// Neutral Barrier's dome on `unit`: Preserve's with another tile, and a yellow strobe.
    pub fn neutral_barrier(&mut self, unit: Option<u32>, facing: f32, now: Instant) {
        self.domes.push(Dome { unit, start: now, facing, holy: true });
        self.bodies.push(Body { unit, start: now, flicker: Flicker::Strobe([0xFA, 0xFA, 0], 10, 30) });
    }

    /// Analyze's white strobe of `unit` (ticks 10 to 90).
    pub fn analyze(&mut self, unit: Option<u32>, now: Instant) {
        self.bodies.push(Body { unit, start: now, flicker: Flicker::Strobe([0xFA; 3], 10, 90) });
    }

    /// Enchant Deadly Poison's flicker on `unit`.
    pub fn poison(&mut self, unit: Option<u32>, now: Instant) {
        self.bodies.push(Body { unit, start: now, flicker: Flicker::Poison });
    }

    /// Memorize's flicker on `unit`.
    pub fn memorize(&mut self, unit: Option<u32>, now: Instant) {
        self.bodies.push(Body { unit, start: now, flicker: Flicker::Memorize });
    }

    /// Refresh's, Abundance's and Phantom Thrust's green flicker on `unit`.
    pub fn green(&mut self, unit: Option<u32>, now: Instant) {
        self.bodies.push(Body { unit, start: now, flicker: Flicker::Outlined([0, 0xFF, 0], 30) });
    }

    /// The Guillotine Cross poisons' magenta flicker on `unit`.
    pub fn purple(&mut self, unit: Option<u32>, now: Instant) {
        self.bodies.push(Body { unit, start: now, flicker: Flicker::Outlined([0xFF, 0, 0xFF], 30) });
    }

    /// Acceleration's and Shape Shift's bluish-white pulse of `unit` (ticks 0 to 60).
    pub fn pulse(&mut self, unit: Option<u32>, now: Instant) {
        self.bodies.push(Body { unit, start: now, flicker: Flicker::Outlined([0x9B, 0x9B, 0xFF], 60) });
    }

    /// Sonic Wave's blue strobe of `unit`.
    pub fn sonic(&mut self, unit: Option<u32>, now: Instant) {
        self.bodies.push(Body { unit, start: now, flicker: Flicker::Sonic });
    }

    /// Deadly Infect's black and white blink of `unit`.
    pub fn blink(&mut self, unit: Option<u32>, now: Instant) {
        self.bodies.push(Body { unit, start: now, flicker: Flicker::Blink });
    }

    /// Detonator's lilac flush of `unit`.
    pub fn concentration(&mut self, unit: Option<u32>, now: Instant) {
        self.bodies.push(Body { unit, start: now, flicker: Flicker::Concentration });
    }

    /// Double Casting's strobe on `unit`.
    pub fn double_cast(&mut self, unit: Option<u32>, now: Instant) {
        self.bodies.push(Body { unit, start: now, flicker: Flicker::DoubleCast });
    }

    /// Chase Walk's cast: `unit` spins faster and faster while it stands, for `length` ticks.
    pub fn chase_walk(&mut self, unit: Option<u32>, length: u32, now: Instant) {
        self.walks.retain(|walk| walk.unit != unit);
        self.walks.push(Walk { unit, start: now, length, done: 0 });
    }

    /// How many degrees `unit` turns since this was last asked: each tick of the cast it stands for adds its share.
    pub fn turned(&mut self, unit: Option<u32>, now: Instant, standing: bool) -> f32 {
        let mut degrees = 0.0;
        for walk in self.walks.iter_mut().filter(|walk| walk.unit == unit) {
            let due = passes(walk.start, now).min(walk.length);
            if standing {
                degrees += (walk.done..due).map(spin_step).sum::<f32>();
            }
            walk.done = walk.done.max(due);
        }
        degrees
    }

    /// Spiral Pierce's cast on `unit` for `length` ticks: it replaces the last.
    pub fn pierce(&mut self, unit: Option<u32>, length: u32, now: Instant) {
        self.bodies.retain(|body| !(body.unit == unit && matches!(body.flicker, Flicker::Pierce(_))));
        self.bodies.push(Body { unit, start: now, flicker: Flicker::Pierce(length) });
    }

    /// A cast cut off: Spiral Pierce's flicker goes.
    pub fn cut_pierce(&mut self) {
        self.bodies.retain(|body| !matches!(body.flicker, Flicker::Pierce(_)));
    }

    pub fn advance(&mut self, now: Instant) {
        self.strips.retain(|strip| passes(strip.start, now) <= STRIP_TICKS);
        self.domes.retain(|dome| passes(dome.start, now) <= 200);
        self.bodies.retain(|body| passes(body.start, now) <= 300);
        self.walks.retain(|walk| passes(walk.start, now) <= walk.length);
    }

    /// Every wall at `now`; `position_of` gives a unit's feet.
    pub fn meshes(&self, now: Instant, position_of: impl Fn(Option<u32>) -> Option<Vector>) -> Vec<Wall> {
        let mut walls = Vec::new();
        for fog in &self.fogs {
            let n = passes(fog.start, now);
            if n >= 1 {
                walls.push(fog.walls(n));
            }
        }
        for dome in &self.domes {
            let (Some(feet), n) = (position_of(dome.unit), passes(dome.start, now)) else { continue };
            if (1..43).contains(&n) {
                walls.extend(dome.walls(feet, n));
            }
        }
        walls
    }

    /// Full Strip's pieces at `now`: where, spin, half-diagonal and alpha.
    pub fn pieces(&self, now: Instant) -> Vec<Piece3> {
        let mut pieces = Vec::new();
        for strip in self.strips.iter().filter(|strip| !strip.upright) {
            let Some(t) = passes(strip.start, now).checked_sub(1) else { continue };
            for piece in &strip.pieces {
                let n = piece.wait + t as i32 + 1;
                if !(1..=60).contains(&n) {
                    continue;
                }
                let radius = 12.0 + n as f32;
                let (az_sin, az_cos) = piece.azimuth.to_radians().sin_cos();
                let (el_sin, el_cos) = piece.elevation.to_radians().sin_cos();
                let at = [strip.at[0] + radius * el_cos * az_cos, strip.at[1] - radius * el_sin, strip.at[2] + radius * el_cos * az_sin];
                let spin = (piece.spin + 3.0 * (t + 1) as f32) % 360.0;
                pieces.push(Piece3 { potion: Potion { icon: piece.icon, at, spin, alpha: coin_alpha(n as u32) }, half: piece.half });
            }
        }
        pieces
    }

    /// Strip Accessory's upright icons at `now`, added on in #FA6464 and drawn over everything with no depth test: a wall each icon.
    pub fn upright_pieces(&self, now: Instant) -> Vec<Wall> {
        let mut walls: Vec<Wall> = Vec::new();
        for strip in self.strips.iter().filter(|strip| strip.upright) {
            let Some(t) = passes(strip.start, now).checked_sub(1) else { continue };
            for piece in &strip.pieces {
                let n = piece.wait + t as i32 + 1;
                if !(1..=60).contains(&n) {
                    continue;
                }
                let radius = 12.0 + n as f32;
                let (az_sin, az_cos) = piece.azimuth.to_radians().sin_cos();
                let (el_sin, el_cos) = piece.elevation.to_radians().sin_cos();
                let centre = [strip.at[0] + radius * el_cos * az_cos, strip.at[1] + radius * el_sin, strip.at[2] + radius * el_cos * az_sin];
                let (turn_sin, turn_cos) = ((piece.turn + 5.0 * (t + 1) as f32) % 360.0).to_radians().sin_cos();
                let tint = [STRIP_TINT[0], STRIP_TINT[1], STRIP_TINT[2], coin_alpha(n as u32).trunc().clamp(0.0, 255.0) / 255.0];
                let corner = |j: usize, uv: [f32; 2]| {
                    let (sin, cos) = ((piece.spin + 90.0 * j as f32) % 360.0).to_radians().sin_cos();
                    MeshVertex {
                        position: [centre[0] + piece.half * cos * turn_cos, centre[1] - piece.half * sin, centre[2] + piece.half * cos * turn_sin],
                        uv,
                        tint,
                    }
                };
                let [a, b, c, d] = [corner(0, [0.0, 1.0]), corner(1, [1.0, 1.0]), corner(2, [1.0, 0.0]), corner(3, [0.0, 0.0])];
                let vertices = [a, b, c, a, c, d];
                match walls.iter_mut().find(|wall| wall.texture == piece.icon) {
                    Some(wall) => wall.vertices.extend(vertices),
                    None => walls.push(Wall { texture: piece.icon, vertices: vertices.to_vec(), additive: true, forced: false }),
                }
            }
        }
        walls
    }

    /// The body flags and colour an effect gives `unit` at `now`.
    pub fn glow(&self, unit: Option<u32>, now: Instant) -> Option<(u32, Option<[u8; 3]>)> {
        self.bodies.iter().filter(|body| body.unit == unit).find_map(|body| flicker_at(body.flicker, passes(body.start, now).checked_sub(1)?))
    }

    /// The colour a flash left on `unit`'s body, fading back to white.
    pub fn body_tint(&self, unit: Option<u32>, now: Instant) -> Option<[u8; 3]> {
        self.bodies.iter().filter(|body| body.unit == unit).find_map(|body| {
            let e = passes(body.start, now).checked_sub(1)?;
            match body.flicker {
                Flicker::Poison if e > 80 => fade_back([0xFF, 0x00, 0xFF], e - 80),
                Flicker::Strobe(rgb, _, last) if e > last => fade_back(rgb, e - last),
                Flicker::Outlined(rgb, last) if e > last => fade_back(rgb, e - last),
                Flicker::Concentration if e > 33 => fade_back(LILAC, e - 33),
                Flicker::Blink if e > 60 => fade_back([0; 3], e - 60),
                Flicker::Sonic if e > 52 => fade_back([5, 5, 0xFF], e - 52),
                _ => None,
            }
        })
    }
}

/// A Full Strip piece to draw: a picture at a place (see `Potion`) and its half-diagonal in world units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Piece3 {
    pub potion: Potion,
    pub half: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prims::TICK;

    fn near(got: f32, want: f32) -> bool {
        (got - want).abs() < 0.01
    }

    #[test]
    fn fog_wall_rises_and_breathes() {
        let start = Instant::now();
        let mut guard = Guard::default();
        guard.keep_fog(4, [10.0, 2.0, 10.0], start, |_| 0);
        // With the angle 0: p at n = 1, 89, 90: 0.3664, 31.993, 32.0; with 180 it is 9.6 at n = 90.
        assert!(near(fog_height(0.0, 1), 0.3664) && near(fog_height(0.0, 89), 31.993) && near(fog_height(0.0, 90), 32.0));
        assert!(near(fog_height(180.0, 90), 9.6));
        // Twelve triangles: four sides at alpha 30 and two diagonals at 60, the base 1 up, in the box's grey.
        let walls = guard.meshes(start, |_| None);
        assert_eq!(walls.len(), 1);
        assert!(walls[0].texture == FOG && !walls[0].additive && walls[0].vertices.len() == 36);
        let alpha = |wall: &Wall, wall_index: usize| (wall.vertices[wall_index * 6].tint[3] * 255.0).round();
        assert_eq!([0, 3, 4, 5].map(|k| alpha(&walls[0], k)), [30.0, 30.0, 60.0, 60.0]);
        assert_eq!(walls[0].vertices[0].tint[..3], [80.0 / 255.0; 3]);
        assert!(near(walls[0].vertices[0].position[1], 3.0));
        // It goes with its unit.
        guard.forget(4);
        assert!(guard.meshes(start, |_| None).is_empty());
    }

    #[test]
    fn full_strip_throws_16_shields_and_16_swords() {
        let start = Instant::now();
        let mut guard = Guard::default();
        guard.strip([5.0, 1.0, 5.0], start, |_| 0);
        let pieces = &guard.strips[0].pieces;
        assert_eq!(pieces.len(), 32);
        assert_eq!((pieces.iter().filter(|piece| piece.half == 5.0).count(), pieces.iter().filter(|piece| piece.half == 9.0).count()), (16, 16));
        assert!(pieces[..4].iter().all(|piece| piece.icon == SHIELD) && pieces[4..8].iter().all(|piece| piece.icon == SWORD));
        // Every draw 0: first shown on tick 18 (n = 1: alpha 25, r 13), alpha at n = 10, 25, 26, 60, 61: 250, 250, 243, 5, 0.
        let at = |tick: u32| guard.pieces(start + TICK * tick);
        assert!(at(17).is_empty());
        let first = at(18);
        assert_eq!(first.len(), 32);
        assert_eq!(first[0].potion.alpha, 25.0);
        assert!(near(f32::hypot(first[0].potion.at[0] - 5.0, first[0].potion.at[2] - 5.0), 13.0));
        assert_eq!([10, 25, 26, 60, 61].map(coin_alpha), [250.0, 250.0, 243.0, 5.0, 0.0]);
        assert!(at(77).len() == 32 && at(78).is_empty());
        // r at n = 60 is 72; the spin goes up 3 a tick.
        assert!(near(f32::hypot(at(77)[0].potion.at[0] - 5.0, at(77)[0].potion.at[2] - 5.0), 72.0));
        assert!(near(at(19)[0].potion.spin - at(18)[0].potion.spin, 3.0));
        // Elevation 0, azimuth 0: straight out along +x at the target's height.
        assert!(near(first[0].potion.at[1], 1.0));
        guard.advance(start + TICK * 301);
        assert!(guard.strips.is_empty());
    }

    #[test]
    fn preserve_stands_a_green_dome_of_45_tiles() {
        let start = Instant::now();
        let mut guard = Guard::default();
        guard.preserve(Some(1), 90.4, start);
        // Alpha at n = 1, 5, 26, 27, 42, 43: 20, 100, 100, 94, 4, 0; grow at 27 and 42: 0.1, 1.6.
        assert_eq!([1, 5, 26, 27, 42, 43].map(dome_alpha), [20.0, 100.0, 100.0, 94.0, 4.0, 0.0]);
        assert!(near(dome_grow(27), 0.1) && near(dome_grow(42), 1.6));
        // The first turn is the facing (truncated) + 270, then 10 a tick.
        assert_eq!((dome_turn(90.4, 1), dome_turn(90.4, 2), dome_turn(90.4, 4)), (0.0, 10.0, 30.0));
        let feet = [0.0; 3];
        let walls = guard.meshes(start, |_| Some(feet));
        assert_eq!(walls.len(), 3);
        assert!(walls.iter().all(|wall| wall.texture == TILE && wall.vertices.len() == 15 * 6));
        assert_eq!(walls.iter().map(|wall| wall.additive).collect::<Vec<_>>(), [true, true, false]);
        assert_eq!(walls[0].vertices[0].tint[..3], [205.0 / 255.0, 1.0, 205.0 / 255.0]);
        assert_eq!((walls[0].vertices[0].tint[3] * 255.0).round(), 20.0);
        // The tiles are 2, 3 and 2 wide by row: the equator row's first tile has corners 3·√2 apart… across its diagonal.
        let tile = |row: usize, column: usize| &walls[0].vertices[(row * 5 + column) * 6..(row * 5 + column + 1) * 6];
        let width = |tile: &[MeshVertex]| f32::hypot(tile[0].position[0] - tile[1].position[0], tile[0].position[2] - tile[1].position[2]);
        assert!(near(width(tile(0, 0)), 4.0) && near(width(tile(1, 0)), 6.0) && near(width(tile(2, 0)), 4.0));
        // Shell 3 swells with the grow; drawn on ticks 0…41, gone at 42.
        let radius = |tick: u32| {
            let walls = guard.meshes(start + TICK * tick, |_| Some(feet));
            walls[2].vertices.iter().map(|vertex| f32::hypot(vertex.position[0], vertex.position[2])).fold(0.0, f32::max)
        };
        assert!(radius(41) > radius(25) + 1.0);
        assert!(guard.meshes(start + TICK * 42, |_| Some(feet)).is_empty());
        // The body: additive #FAFAFA on the even ticks of 10…30, white on the odd; nothing at 9 and the fade after.
        let glow = |tick: u32| guard.glow(Some(1), start + TICK * tick);
        assert_eq!(glow(10), Some((crate::actor::GLOW_ADDITIVE, Some([0xFA; 3]))));
        assert_eq!(glow(11), Some((0, Some([255; 3]))));
        assert!(glow(9).is_none() && glow(30).is_some() && glow(31).is_none());
    }

    #[test]
    fn enchant_deadly_poison_and_spiral_pierces_cast_flicker_the_body() {
        let start = Instant::now();
        let mut guard = Guard::default();
        guard.poison(Some(1), start);
        guard.pierce(Some(2), 200, start);
        // Poison: additive #FF00FF on even ticks to 80, white on odd, nothing at 81.
        let poison = |tick: u32| guard.glow(Some(1), start + TICK * tick);
        assert_eq!(poison(0), Some((crate::actor::GLOW_ADDITIVE, Some([0xFF, 0, 0xFF]))));
        assert_eq!(poison(1), Some((0, Some([255; 3]))));
        assert!(poison(80).is_some() && poison(81).is_none());
        assert!(guard.body_tint(Some(1), start + TICK * 81).is_some(), "the magenta fades back");
        // Pierce: outline copies on ticks 0…140, additive #FAFA64 on even ticks, #FFFFFF on odd.
        let pierce = |guard: &Guard, tick: u32| guard.glow(Some(2), start + TICK * tick);
        assert_eq!(pierce(&guard, 0), Some((crate::actor::GLOW_OUTLINE | crate::actor::GLOW_ADDITIVE, Some([0xFA, 0xFA, 0x64]))));
        assert_eq!(pierce(&guard, 1), Some((crate::actor::GLOW_OUTLINE, Some([255; 3]))));
        assert!(pierce(&guard, 140).is_some() && pierce(&guard, 141).is_none());
        // It stops with a shorter cast, and a second cast replaces the first.
        guard.pierce(Some(2), 10, start);
        assert_eq!(guard.bodies.iter().filter(|body| body.unit == Some(2)).count(), 1);
        assert!(pierce(&guard, 9).is_some() && pierce(&guard, 10).is_none());
        guard.cut_pierce();
        assert!(pierce(&guard, 0).is_none());
    }

    #[test]
    fn memorize_double_casting_and_chase_walk_work_on_the_body() {
        let start = Instant::now();
        let mut guard = Guard::default();
        guard.memorize(Some(1), start);
        guard.double_cast(Some(2), start);
        // Memorize: additive #FAFA64 on ticks 10, 12 … 34 (13), white normal on 11, 13 … 35; nothing on 9 and 36.
        let memorize = |tick: u32| guard.glow(Some(1), start + TICK * tick);
        assert_eq!(memorize(10), Some((crate::actor::GLOW_ADDITIVE, Some([0xFA, 0xFA, 0x64]))));
        assert_eq!(memorize(11), Some((0, Some([255; 3]))));
        assert_eq!((0..40).filter(|&tick| memorize(tick).is_some_and(|(flags, _)| flags != 0)).count(), 13);
        assert!(memorize(9).is_none() && memorize(35).is_some() && memorize(36).is_none());
        // Double Casting: outline copies on 10…50, additive #FF0000 on the even ticks, #FFFFFF on the odd.
        let double = |tick: u32| guard.glow(Some(2), start + TICK * tick);
        assert_eq!(double(10), Some((crate::actor::GLOW_OUTLINE | crate::actor::GLOW_ADDITIVE, Some([0xFF, 0, 0]))));
        assert_eq!(double(11), Some((crate::actor::GLOW_OUTLINE, Some([255; 3]))));
        assert!(double(9).is_none() && double(50).is_some() && double(51).is_none());
        // Chase Walk: the turn after t ticks 180, 192, 444, 462, 678, 702, 1662 for t = 15, 16, 30, 31, 40, 41, 61.
        let total = |ticks: u32| (0..ticks).map(spin_step).sum::<f32>();
        assert_eq!([15, 16, 30, 31, 40, 41, 61].map(total), [180.0, 192.0, 444.0, 462.0, 678.0, 702.0, 1662.0]);
        // It turns the caster as the ticks come, only while it stands, and not past the cast.
        guard.chase_walk(Some(3), 62, start);
        assert_eq!(guard.turned(Some(3), start, true), 12.0);
        assert_eq!(guard.turned(Some(3), start, true), 0.0);
        assert_eq!(guard.turned(Some(3), start + TICK * 4, true), 48.0);
        assert_eq!(guard.turned(Some(3), start + TICK * 9, false), 0.0, "walking: those ticks are lost");
        assert_eq!(guard.turned(Some(3), start + TICK * 10, true), 12.0);
        let rest = guard.turned(Some(3), start + TICK * 100, true);
        assert_eq!(rest, total(62) - total(11));
        assert_eq!(guard.turned(Some(4), start + TICK * 5, true), 0.0, "another unit");
        guard.advance(start + TICK * 70);
        assert!(guard.walks.is_empty());
    }

    #[test]
    fn refresh_flickers_green_in_outline_copies() {
        let start = Instant::now();
        let mut guard = Guard::default();
        guard.green(Some(1), start);
        use crate::actor::{GLOW_ADDITIVE, GLOW_OUTLINE};
        let glow = |tick: u32| guard.glow(Some(1), start + TICK * tick);
        // Outline copies on ticks 0 to 30: additive #00FF00 on the even ticks, #FFFFFF on the odd ones; nothing on 31.
        assert_eq!(glow(0), Some((GLOW_OUTLINE | GLOW_ADDITIVE, Some([0, 255, 0]))));
        assert_eq!(glow(1), Some((GLOW_OUTLINE, Some([255; 3]))));
        assert_eq!(glow(30), Some((GLOW_OUTLINE | GLOW_ADDITIVE, Some([0, 255, 0]))));
        assert!(glow(31).is_none() && guard.glow(Some(2), start).is_none());
        // The colour fades back to white by 8 a channel a tick after the last green.
        assert_eq!(guard.body_tint(Some(1), start + TICK * 31), Some([8, 255, 8]));
        assert_eq!(guard.body_tint(Some(1), start + TICK * 62), None);
    }

    #[test]
    fn detonator_flushes_lilac_the_poisons_flicker_magenta_and_weapon_crush_bursts_weapons() {
        use crate::actor::{GLOW_ADDITIVE, GLOW_OUTLINE};
        let start = Instant::now();
        let mut guard = Guard::default();
        guard.concentration(Some(1), start);
        guard.purple(Some(2), start);
        // Detonator: #9B9BFF on ticks 10 to 33, added on only on the even ones; nothing on 9 and 34; back to white in 13 ticks.
        let lilac = |tick: u32| guard.glow(Some(1), start + TICK * tick);
        assert!(lilac(9).is_none() && lilac(34).is_none());
        assert_eq!(lilac(10), Some((GLOW_ADDITIVE, Some([0x9B, 0x9B, 0xFF]))));
        assert_eq!(lilac(11), Some((0, Some([0x9B, 0x9B, 0xFF]))));
        assert_eq!((lilac(32).unwrap().0, lilac(33).unwrap().0), (GLOW_ADDITIVE, 0));
        assert_eq!(guard.body_tint(Some(1), start + TICK * 34), Some([0xA3, 0xA3, 0xFF]));
        assert_eq!(guard.body_tint(Some(1), start + TICK * 47), None);
        // The poisons: outline copies on ticks 0 to 30, #FF00FF added on the even ticks, #FFFFFF on the odd.
        let purple = |tick: u32| guard.glow(Some(2), start + TICK * tick);
        assert_eq!(purple(0), Some((GLOW_OUTLINE | GLOW_ADDITIVE, Some([0xFF, 0, 0xFF]))));
        assert_eq!(purple(1), Some((GLOW_OUTLINE, Some([255; 3]))));
        assert!(purple(30).is_some() && purple(31).is_none());
        // Weapon Crush: 32 icons from 8 prims, in the issue's order, all with a half-diagonal of 5, the first shown on tick 18.
        guard.weapon_crush([5.0, 1.0, 5.0], start, |_| 0);
        let pieces = &guard.strips[0].pieces;
        assert_eq!(pieces.len(), 32);
        assert!(pieces.iter().all(|piece| piece.half == 5.0));
        assert!((0..8).all(|prim| pieces[4 * prim..4 * prim + 4].iter().all(|piece| piece.icon == WEAPONS[prim])));
        assert!(WEAPONS[0].ends_with("루드라의활.bmp") && WEAPONS[7].ends_with("배틀액스.bmp"));
        assert!(guard.pieces(start + TICK * 17).is_empty() && guard.pieces(start + TICK * 18).len() == 32);
    }

    #[test]
    fn acceleration_pulses_bluish_white_for_61_ticks() {
        use crate::actor::{GLOW_ADDITIVE, GLOW_OUTLINE};
        let start = Instant::now();
        let mut guard = Guard::default();
        guard.pulse(Some(1), start);
        let glow = |tick: u32| guard.glow(Some(1), start + TICK * tick);
        assert_eq!(glow(0), Some((GLOW_OUTLINE | GLOW_ADDITIVE, Some([0x9B, 0x9B, 0xFF]))));
        assert_eq!(glow(1), Some((GLOW_OUTLINE, Some([255; 3]))));
        assert!(glow(60).is_some() && glow(61).is_none());
        assert_eq!(guard.body_tint(Some(1), start + TICK * 61), Some([0xA3, 0xA3, 0xFF]));
    }

    #[test]
    fn neutral_barrier_uses_a_holier_tile_and_a_yellow_strobe_and_analyze_strobes_white() {
        use crate::actor::GLOW_ADDITIVE;
        let start = Instant::now();
        let mut guard = Guard::default();
        guard.neutral_barrier(Some(1), 90.4, start);
        guard.analyze(Some(2), start);
        // 45 quads: the additive shells (1 and 2) on the holy tile, shell 3 on a01.
        let walls = guard.meshes(start, |_| Some([0.0; 3]));
        assert_eq!(walls.iter().map(|wall| wall.texture).collect::<Vec<_>>(), [HOLY_TILE, HOLY_TILE, TILE]);
        assert!(walls.iter().all(|wall| wall.vertices.len() == 15 * 6));
        // Yellow on ticks 10 to 30: additive #FAFA00 on the even ones, #FFFFFF on the odd.
        let yellow = |tick: u32| guard.glow(Some(1), start + TICK * tick);
        assert_eq!(yellow(10), Some((GLOW_ADDITIVE, Some([0xFA, 0xFA, 0]))));
        assert_eq!(yellow(11), Some((0, Some([255; 3]))));
        assert!(yellow(9).is_none() && yellow(30).is_some() && yellow(31).is_none());
        // Analyze: ticks 10 to 90 in #FAFAFA; preserve's own white.
        let white = |tick: u32| guard.glow(Some(2), start + TICK * tick);
        assert!(white(9).is_none() && white(10) == Some((GLOW_ADDITIVE, Some([0xFA; 3]))) && white(90).is_some() && white(91).is_none());
        let mut plain = Guard::default();
        plain.preserve(Some(1), 0.0, start);
        assert!(plain.meshes(start, |_| Some([0.0; 3])).iter().all(|wall| wall.texture == TILE));
    }

    #[test]
    fn deadly_infect_blinks_the_caster_black_and_white() {
        let start = Instant::now();
        let mut guard = Guard::default();
        guard.blink(Some(1), start);
        let glow = |tick: u32| guard.glow(Some(1), start + TICK * tick);
        // White on the odd ticks 6 to 60 would be 7, 9 …: black on the even ones, no flag; nothing on 5 and 61.
        assert!(glow(5).is_none() && glow(61).is_none());
        assert_eq!((glow(6), glow(7)), (Some((0, Some([0; 3]))), Some((0, Some([255; 3])))));
        assert_eq!(glow(60), Some((0, Some([0; 3]))));
        // It fades back to white by 8 a channel a tick.
        assert_eq!(guard.body_tint(Some(1), start + TICK * 61), Some([8; 3]));
    }

    #[test]
    fn strip_accessory_bursts_32_icons_in_upright_planes() {
        let start = Instant::now();
        let mut guard = Guard::default();
        guard.accessory([5.0, 1.0, 5.0], start, |_| 0);
        let pieces = &guard.strips[0].pieces;
        // 8 prims of 4: f 5.0 but 4.0 for the seventh, the icons in the issue's order; not squares facing the camera.
        assert_eq!(pieces.len(), 32);
        assert!(pieces.iter().enumerate().all(|(index, piece)| piece.icon == ACCESSORIES[index / 4] && piece.half == if index / 4 == 6 { 4.0 } else { 5.0 }));
        assert!(ACCESSORIES[0].ends_with("갈색반지.bmp") && ACCESSORIES[7].ends_with("수제초콜렛.bmp"));
        assert!(guard.pieces(start + TICK * 30).is_empty());
        // Shown for ages 1 to 60: with every draw 0 on ticks 18 to 77; one wall an icon.
        assert!(guard.upright_pieces(start + TICK * 17).is_empty());
        let walls = guard.upright_pieces(start + TICK * 18);
        assert_eq!(walls.len(), 8);
        assert!(walls.iter().all(|wall| wall.additive && wall.vertices.len() == 4 * 6));
        assert!(!guard.upright_pieces(start + TICK * 77).is_empty() && guard.upright_pieces(start + TICK * 78).is_empty());
        // Alpha at n = 1, 10, 11, 25, 26, 60, 61: 25, 250, 250, 250, 243, 5, 0. r = 12 + n: 13 and 72.
        assert_eq!([1, 10, 11, 25, 26, 60, 61].map(coin_alpha), [25.0, 250.0, 250.0, 250.0, 243.0, 5.0, 0.0]);
        assert_eq!((walls[0].vertices[0].tint[3] * 255.0).round(), 25.0);
        // With σ = 0 … the plane turns 5° a tick from tick 1 (σ = 5 · (t + 1)): corner offsets at b = 0 are along the plane (5, 0).
        let first = |tick: u32| guard.upright_pieces(start + TICK * tick)[0].vertices[0].position;
        let centre = [5.0 + 13.0, 1.0, 5.0];
        assert!((first(18)[1] - centre[1]).abs() < 1e-4);
        let (sin, cos) = (5.0 * 19.0_f32).to_radians().sin_cos();
        assert!((first(18)[0] - (centre[0] + 5.0 * cos)).abs() < 1e-3 && (first(18)[2] - (centre[2] + 5.0 * sin)).abs() < 1e-3);
        guard.advance(start + TICK * 301);
        assert!(guard.strips.is_empty());
    }

    #[test]
    fn sonic_wave_strobes_the_rune_knight_blue() {
        use crate::actor::{GLOW_ADDITIVE, GLOW_OUTLINE};
        let start = Instant::now();
        let mut guard = Guard::default();
        guard.sonic(Some(1), start);
        let glow = |tick: u32| guard.glow(Some(1), start + TICK * tick);
        // #0505FF added on with the outline copies on ticks 4, 6 … 52, white on 5, 7 … 51; nothing on 3 and 53.
        assert!(glow(3).is_none() && glow(53).is_none());
        assert_eq!(glow(4), Some((GLOW_OUTLINE | GLOW_ADDITIVE, Some([5, 5, 0xFF]))));
        assert_eq!(glow(5), Some((0, Some([255; 3]))));
        assert!(glow(52).is_some() && glow(51).unwrap().0 == 0);
        assert_eq!(guard.body_tint(Some(1), start + TICK * 53), Some([13, 13, 255]));
    }
}
