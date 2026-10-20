//! The Monk line's and the Mechanic's code-made shapes, a 16 ms tick at a time: Self-Destruction's eight streaks (183), Storm
//! Blast's hopping lines (330), Rage Burst's 32 rays (377), Sound of Destruction's sliding copies (425, in `actor`), Windmill's
//! tornado and spin (435) and Dark Illusion's 32 pale lines (439).
//!
//! A shape's own age a counts from 1 at its first tick; t is the ticks since the effect started (a = t + 1 for what is made at
//! tick 0).

use std::time::Instant;

use super::sniper::{self, arc_ribbon};
use super::tanji::bearing;
use super::walls::Wall;
use super::{ScreenShape, passes};
use crate::world::{MeshVertex, Vector};

pub const LENS: [&str; 2] = ["data\\texture\\effect\\lens1.tga", "data\\texture\\effect\\lens2.tga"];
pub const WHITE: &str = "data\\texture\\effect\\white02.bmp";
pub const STORM: &str = "data\\texture\\effect\\storm2.tga";
pub const STRIP: &str = "data\\texture\\effect\\line3.tga";
/// Hesperus Lit's disc: a ring blast (128 px) that grows 7 % a tick for 69 ticks.
pub const BLAST: &str = "data\\texture\\effect\\explosive_1_128.bmp";
const DISC_LAST: u32 = 69;
pub const GOLD_STRIP: &str = "data\\texture\\effect\\line3y.tga";
pub const RAY: &str = sniper::RAY;
pub const CLOUD: &str = sniper::CLOUD;

type Corner = ([f32; 2], [f32; 2], [f32; 4]);

/// Self-Destruction: eight streaks 100 px over the feet, #7878FF.
const STREAKS: u32 = 8;
const STREAK_TINT: [f32; 3] = [120.0 / 255.0, 120.0 / 255.0, 1.0];
/// The STR's anchor: 80 px over the projected feet.
const ANCHOR_UP: f32 = 80.0;
/// Storm Blast: a prim of four bands on each of ticks 0 to 3.
const HOP_PRIMS: u32 = 4;
const HOP_SEGMENTS: usize = 20;
const HOP_TUBE: f32 = 0.1;
/// Rage Burst: 8 prims of 4 rays, drawn for ages 1 to 44, and the body added on ticks 5 to 35.
const RAY_LAST: u32 = 44;
/// Windmill: the tornado's ten squares (radius, drawn for ages 1 to 99), the wisps (ages 1 to 60), the spin on ticks 0 to 47.
const SQUARES: [f32; 10] = [40.0, 36.0, 32.4, 29.2, 26.4, 24.0, 22.0, 20.4, 19.2, 18.4];
const TORNADO_LAST: u32 = 99;
const WISP_LAST: u32 = 60;
const SPIN_TICKS: u32 = 48;
/// Dark Illusion: 8 prims of 4 pale lines, drawn for ages 1 to 49.
const STRIP_PRIMS: usize = 8;
const STRIP_LAST: u32 = 49;
/// Hundred Spear's lines: 120 ticks, in pale yellows.
const GOLD_LAST: u32 = 119;
/// Knuckle Arrow's lines' tints: #6969FF, #4141FF, #1919FF.
const KNUCKLE_TINTS: [[f32; 3]; 3] = [[105.0 / 255.0, 105.0 / 255.0, 1.0], [65.0 / 255.0, 65.0 / 255.0, 1.0], [25.0 / 255.0, 25.0 / 255.0, 1.0]];
const GOLD_TINTS: [[f32; 3]; 3] = [[1.0, 1.0, 225.0 / 255.0], [1.0, 1.0, 205.0 / 255.0], [1.0, 1.0, 185.0 / 255.0]];
const STRIP_TINTS: [[f32; 3]; 3] = [[225.0 / 255.0, 225.0 / 255.0, 1.0], [205.0 / 255.0, 205.0 / 255.0, 1.0], [185.0 / 255.0, 185.0 / 255.0, 1.0]];

/// A whole angle as the game's tables read it: a negative one a degree up, one past 360 as a − 181.
fn table(angle: i32) -> f32 {
    (if angle < 0 {
        angle + 1
    } else if angle > 360 {
        angle - 181
    } else {
        angle
    }) as f32
}

/// One of Self-Destruction's streaks: which lens picture, its life, the way it points, how fast it flies, its half-size and
/// where it starts (px from the anchor, y down).
pub struct Flare {
    lens: usize,
    life: u32,
    yaw: f32,
    speed: f32,
    half: f32,
    tall: f32,
    offset: [f32; 2],
}

struct Flares {
    unit: Option<u32>,
    start: Instant,
    flares: Vec<Flare>,
}

/// A streak's alpha after its tick k: 31.75 a tick to 254.
pub fn flare_alpha(k: u32) -> f32 {
    (31.75 * (k + 1) as f32).min(254.0)
}

impl Flare {
    /// Its half-height after tick k: it grows faster and faster.
    pub fn half_height(&self, k: u32) -> f32 {
        let n = (k + 1) as f32;
        self.tall + 1.5 * n + 0.125 * n * (n + 1.0)
    }

    /// Its half-width after tick k: it thins to nothing at its life.
    pub fn half_width(&self, k: u32) -> f32 {
        self.half * (1.0 - (k + 1) as f32 / self.life as f32)
    }

    /// How far it has flown after tick k.
    pub fn flown(&self, k: u32) -> f32 {
        let n = (k + 1) as f32;
        n * self.speed - self.speed * n * (n + 1.0) / (4.0 * self.life as f32)
    }

    /// The quad after tick k, turned clockwise by its yaw so it points the way it flies.
    fn shape(&self, k: u32) -> Vec<Corner> {
        let (sin, cos) = self.yaw.to_radians().sin_cos();
        let flown = self.flown(k);
        let centre = [self.offset[0] + flown * sin, self.offset[1] - ANCHOR_UP - flown * cos];
        let (wide, high) = (self.half_width(k), self.half_height(k));
        let tint = [STREAK_TINT[0], STREAK_TINT[1], STREAK_TINT[2], flare_alpha(k) / 255.0];
        let corner = |x: f32, y: f32, uv: [f32; 2]| ([centre[0] + x * cos - y * sin, centre[1] + x * sin + y * cos], uv, tint);
        let [a, b, c, d] = [corner(-wide, -high, [0.0, 0.0]), corner(wide, -high, [1.0, 0.0]), corner(wide, high, [1.0, 1.0]), corner(-wide, high, [0.0, 1.0])];
        vec![a, b, c, a, c, d]
    }
}

/// One of Storm Blast's lines: when its prim was made, its wait, alpha, heading, hop radius and centre.
pub struct Hop {
    made: u32,
    delay: i32,
    alpha: f32,
    heading: f32,
    radius: f32,
    centre: Vector,
}

struct Hops {
    start: Instant,
    bands: Vec<Hop>,
}

/// The head phase f (0 to 180) and hop count c of a line at its age e: 8 a tick, a hop at 180 (23, 22, 23 … ticks).
pub fn hop_state(e: i32) -> (f32, u32) {
    let (mut phase, mut hops) = (0.0_f32, 0);
    for _ in 0..e.max(0) {
        phase += 8.0;
        if phase >= 180.0 {
            phase -= 180.0;
            hops += 1;
        }
    }
    (phase, hops)
}

impl Hop {
    /// Its age after effect tick t.
    fn age(&self, t: u32) -> i32 {
        self.delay + t as i32 - self.made as i32 + 1
    }

    /// Its alpha at age e: 5 less a tick from age 36.
    pub fn alpha_at(&self, e: i32) -> f32 {
        (self.alpha - 5.0 * (e - 35).max(0) as f32).max(0.0)
    }

    /// Where it is for phase f of hop c: shorter hops, 0.7 as long and high each time.
    fn point(&self, f: f32, hops: u32) -> Vector {
        let r = self.radius;
        let mut d: f32 = (0..hops).map(|i| 2.0 * r * 0.7_f32.powi(i as i32)).sum();
        let scale = r * 0.7_f32.powi(hops as i32);
        d += scale * f / 90.0;
        let up = 1.5 * scale * f.to_radians().sin();
        let (sin, cos) = self.heading.to_radians().sin_cos();
        [self.centre[0] + cos * d, self.centre[1] + up, self.centre[2] + sin * d]
    }

    /// The line at age e as segments: number j, its two ends and its alpha; segment 0 has no length and a segment with none is left out.
    pub fn segments(&self, e: i32) -> Vec<(usize, Vector, Vector, f32)> {
        let alpha = self.alpha_at(e);
        let history: Vec<Vector> = (0..=HOP_SEGMENTS)
            .map(|j| {
                let (f, hops) = hop_state(e - j.saturating_sub(1) as i32);
                self.point(f, hops)
            })
            .collect();
        (0..HOP_SEGMENTS)
            .filter(|&j| alpha - 5.0 * j as f32 > 0.0 && history[j] != history[j + 1])
            .map(|j| (j, history[j], history[j + 1], alpha - 5.0 * j as f32))
            .collect()
    }

    /// A 3-sided tube of radius 0.1 round the segment from `a` to `b`, in the colour of segment `j`.
    fn tube(a: Vector, b: Vector, j: usize, alpha: f32) -> Vec<MeshVertex> {
        let tint = [1.0, 200.0 / 255.0, (7 * j + 10) as f32 / 255.0, alpha / 255.0];
        let (sin, cos) = ((bearing(a[0] - b[0], a[2] - b[2]) as f32 + 90.0) % 360.0).to_radians().sin_cos();
        let offset = |angle: f32| {
            let (s, c) = angle.to_radians().sin_cos();
            [HOP_TUBE * c * cos, HOP_TUBE * s, HOP_TUBE * c * sin]
        };
        let at = |end: Vector, angle: f32| {
            let off = offset(angle);
            [end[0] + off[0], end[1] + off[1], end[2] + off[2]]
        };
        let mut vertices = Vec::new();
        for angle in [120.0, 240.0, 360.0] {
            let [p, q, r, s] = [
                MeshVertex { position: at(a, angle - 120.0), uv: [0.0, 1.0], tint },
                MeshVertex { position: at(a, angle), uv: [1.0, 1.0], tint },
                MeshVertex { position: at(b, angle), uv: [1.0, 0.0], tint },
                MeshVertex { position: at(b, angle - 120.0), uv: [0.0, 0.0], tint },
            ];
            vertices.extend([p, q, r, p, r, s]);
        }
        vertices
    }
}

/// One of Rage Burst's rays: the turn of its plane and its angle.
pub struct Ray {
    plane: f32,
    angle: i32,
}

struct Rays {
    unit: Option<u32>,
    start: Instant,
    rays: Vec<Ray>,
    /// The Dragon Combo's: thinner and bluer wedges drawn over everything, and no body change.
    dragon: bool,
}

/// A ray's alpha at its age a: 10 a tick to 100, held one more tick, then 3 less a tick.
pub fn ray_alpha(a: u32) -> f32 {
    match a {
        1..=10 => 10.0 * a as f32,
        11 => 100.0,
        _ => (100.0 - 3.0 * (a - 11) as f32).max(0.0),
    }
}

/// How far a ray reaches at its age a.
pub fn ray_reach(a: u32) -> f32 {
    2.0 + 3.0 * a as f32
}

impl Ray {
    /// The angles of its two wedges' rim points: the near ones (L − 1, L + 1) and the far ones (L − 2, L + 2). L + δ wraps at
    /// 360; L − δ does not, so a ray past 360 reads its low side from the table's far end.
    #[cfg(test)]
    pub fn rim(&self) -> [[f32; 2]; 2] {
        self.rim_by([1, 2])
    }

    /// The same for wedges `deltas` degrees to either side.
    pub fn rim_by(&self, deltas: [i32; 2]) -> [[f32; 2]; 2] {
        let high = |delta: i32| table(if self.angle + delta >= 360 { self.angle + delta - 360 } else { self.angle + delta });
        deltas.map(|delta| [table(self.angle - delta), high(delta)])
    }

    /// The two wedges at age a round `feet`: the near one blue, the far one yellow, both added on.
    fn wedges(&self, feet: Vector, a: u32, dragon: bool) -> Vec<MeshVertex> {
        let alpha = ray_alpha(a) / 255.0;
        let reach = ray_reach(a);
        let (plane_sin, plane_cos) = self.plane.to_radians().sin_cos();
        let at = |angle: f32| {
            let (sin, cos) = angle.to_radians().sin_cos();
            [feet[0] + plane_cos * reach * cos, feet[1] + 12.0 - plane_sin * reach * cos, feet[2] + reach * sin]
        };
        let middle = [feet[0], feet[1] + 12.0, feet[2]];
        let mut vertices = Vec::new();
        let (deltas, colours) = if dragon {
            ([2, 5], [[0.0, 0.0, 1.0], [50.0 / 255.0, 50.0 / 255.0, 1.0]])
        } else {
            ([1, 2], [[0.0, 0.0, 250.0 / 255.0], [250.0 / 255.0, 250.0 / 255.0, 0.0]])
        };
        for (rim, rgb) in self.rim_by(deltas).into_iter().zip(colours) {
            let tint = [rgb[0], rgb[1], rgb[2], alpha];
            vertices.extend([
                MeshVertex { position: at(rim[0]), uv: [0.0, 1.0], tint },
                MeshVertex { position: at(rim[1]), uv: [1.0, 1.0], tint },
                MeshVertex { position: middle, uv: [1.0, 0.0], tint },
            ]);
        }
        vertices
    }
}

/// A Windmill: its tornado and wisps stay where the caster stood.
struct Tornado {
    at: Vector,
    start: Instant,
    /// The start turn, then the turn after each tick run.
    first: f32,
    turns: Vec<f32>,
    wisps: Vec<Wisp>,
    /// Reverberation's: red, each square drawn twice, and gone with the trap `kept`.
    red: bool,
    kept: Option<u32>,
}

pub struct Wisp {
    height: f32,
    turn: f32,
    radius: f32,
    lean: f32,
}

/// A tornado square's alpha at its age a: 1 a tick to 40, held to 60, then 1 less a tick.
pub fn tornado_alpha(a: u32) -> f32 {
    match a {
        1..=40 => a as f32,
        41..=60 => 40.0,
        _ => (40.0 - (a - 60) as f32).max(0.0),
    }
}

/// A wisp's alpha at its age a: 1 a tick to 11, held to 50, then 1 less a tick.
pub fn wisp_alpha(a: u32) -> f32 {
    match a {
        1..=11 => a as f32,
        12..=50 => 11.0,
        _ => (11.0 - (a - 50) as f32).max(0.0),
    }
}

impl Tornado {
    /// Square i at the turn `turn`: a slightly tilted quad standing i·5 under the top.
    fn square(&self, i: usize, turn: f32, rgb: [f32; 3], alpha: f32) -> Vec<MeshVertex> {
        let tint = [rgb[0], rgb[1], rgb[2], alpha / 255.0];
        let height = 50.0 - 5.0 * i as f32;
        let corner = |quarter: f32, rise: f32, uv: [f32; 2]| {
            let (sin, cos) = (45.0 * i as f32 + turn + 90.0 * quarter).to_radians().sin_cos();
            MeshVertex { position: [self.at[0] + SQUARES[i] * cos, self.at[1] + height + rise, self.at[2] + SQUARES[i] * sin], uv, tint }
        };
        let [a, b, c, d] = [corner(0.0, 1.0, [0.0, 1.0]), corner(1.0, 1.0, [1.0, 1.0]), corner(2.0, 0.0, [1.0, 0.0]), corner(3.0, 0.0, [0.0, 0.0])];
        vec![a, b, c, a, c, d]
    }

    fn turn_at(&self, a: u32) -> f32 {
        a.checked_sub(1).and_then(|n| self.turns.get(n as usize)).or(self.turns.last()).copied().unwrap_or(self.first)
    }
}

/// One of Dark Illusion's lines.
pub struct Strip {
    side: f32,
    phase: f32,
    half: f32,
    length: f32,
    colour: usize,
    speed: f32,
    /// How much faster it gets each tick.
    growth: f32,
}

/// Which lines: Dark Illusion's pale ones, Hundred Spear's gold ones or Knuckle Arrow's blue ones.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LineKind {
    Pale,
    Gold,
    Knuckle,
}

impl LineKind {
    /// Whether it is one of the quicker, longer-lived kinds (gold and Knuckle Arrow's): 120 ticks, growing speed.
    fn fast(self) -> bool {
        self != LineKind::Pale
    }
}

struct Lines {
    kind: LineKind,
    start: Instant,
    from: Vector,
    heading: f32,
    strips: Vec<Strip>,
}

/// The disc's half-diagonal after n ticks: 0.9 of a size of 15 grown 7 % a tick.
pub fn disc_half(n: u32) -> f32 {
    0.9 * 15.0 * 1.07_f32.powi(n as i32)
}

/// The disc's alpha at n: 6 a tick to 60, then 1 less a tick.
pub fn disc_alpha(n: u32) -> f32 {
    if n <= 10 { 6.0 * n as f32 } else { (60.0 - (n - 10) as f32).max(0.0) }
}

/// Hundred Spear's line alpha at its age a: 20 a tick to 200, held to 20, then 2 less a tick.
pub fn gold_alpha(a: u32) -> f32 {
    match a {
        1..=10 => 20.0 * a as f32,
        11..=20 => 200.0,
        _ => (200.0 - 2.0 * (a - 20) as f32).max(0.0),
    }
}

/// A line's alpha at its age a: 12 a tick to 120, held to 20, then 4 less a tick.
pub fn strip_alpha(a: u32) -> f32 {
    match a {
        1..=10 => 12.0 * a as f32,
        11..=20 => 120.0,
        _ => (120.0 - 4.0 * (a - 20) as f32).max(0.0),
    }
}

impl Strip {
    /// How far along its run it is at age a: 100 units back to 100 on, then round again.
    pub fn at(&self, a: u32) -> f32 {
        (self.phase + self.speed * a as f32 + self.growth * (a * (a + 1)) as f32 / 2.0) % 200.0
    }
}

struct Spread {
    unit: Option<u32>,
    start: Instant,
    /// The ticks its flag is set on: Sound of Destruction's 30 to 99, the chi burst's 0 to 29.
    first: u32,
    last: u32,
}

/// Hesperus Lit's and Chilly Air's faint blue disc spreading flat under a unit.
struct Disc {
    unit: Option<u32>,
    start: Instant,
}

/// Camouflage's translucent unit (744): alpha 50 while the status lasts, then 8 more a tick back to 255.
struct Cloak {
    unit: Option<u32>,
    start: Instant,
    ended: Option<Instant>,
    /// Shadow Form's: black as well, its colour climbing back too.
    black: bool,
}

/// Tetra Vortex's cast: a fresh colour for the caster each tick for the cast, added on two ticks in five.
struct Tetra {
    unit: Option<u32>,
    start: Instant,
    length: u32,
    colours: Vec<[u8; 3]>,
}

/// A unit whose parts shake on ticks `first` to `last` (458, 550).
struct Shake {
    unit: Option<u32>,
    start: Instant,
    first: u32,
    last: u32,
}

/// A unit whose parts swing its head (466), glow blue (543) or red (548) from `start`.
struct Marked {
    unit: Option<u32>,
    start: Instant,
}
type Swing = Marked;
type Blue = Marked;

/// A target spinning (435): +30° a tick on ticks 0 to 47.
struct Spin {
    unit: Option<u32>,
    start: Instant,
    done: u32,
    /// Moon Slasher's: a step a tick from a table instead of 30.
    table: bool,
}

/// Moon Slasher's turn on tick e: 12, 18, 24, 48, 24, 18, 12 over 46 ticks (954° in all).
pub fn spin_step(e: u32) -> f32 {
    match e {
        0..=7 => 12.0,
        8..=15 => 18.0,
        16..=20 => 24.0,
        21..=25 => 48.0,
        26..=32 => 24.0,
        33..=37 => 18.0,
        38..=45 => 12.0,
        _ => 0.0,
    }
}

#[derive(Default)]
pub struct Brawl {
    flares: Vec<Flares>,
    hops: Vec<Hops>,
    rays: Vec<Rays>,
    tornados: Vec<Tornado>,
    lines: Vec<Lines>,
    spreads: Vec<Spread>,
    spins: Vec<Spin>,
    shakes: Vec<Shake>,
    swings: Vec<Swing>,
    blues: Vec<Blue>,
    reds: Vec<Marked>,
    cloaks: Vec<Cloak>,
    tetras: Vec<Tetra>,
    discs: Vec<Disc>,
    manholes: Vec<Option<u32>>,
}

impl Brawl {
    /// Self-Destruction's eight streaks on `unit`, the choices of each from `rand(n)` (0 to n − 1) in the order the game draws them.
    pub fn flares(&mut self, unit: Option<u32>, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        let flares = (0..STREAKS)
            .map(|i| {
                let around = (45 * i) as f32;
                let life = 10 + rand(20);
                let yaw = rand(30) as f32 + around - 15.0;
                let speed = (5 + rand(45)) as f32 / 10.0;
                let half = (5 + rand(15)) as f32;
                let tall = (20 + rand(20)) as f32;
                let away = rand(5) as f32;
                let (sin, cos) = around.to_radians().sin_cos();
                Flare { lens: (i % 2) as usize, life, yaw, speed, half, tall, offset: [sin * away, -cos * away - 20.0] }
            })
            .collect();
        self.flares.push(Flares { unit, start: now, flares });
    }

    /// Storm Blast's 16 lines over the unit's feet `at`: a prim of four bands on each of ticks 0 to 3, their choices from `rand(n)`.
    // ponytail: all four prims take the feet at tick 0 (the game takes them at each prim's tick; 48 ms apart).
    pub fn hops(&mut self, at: Vector, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        let mut bands = Vec::new();
        for made in 0..HOP_PRIMS {
            for _ in 0..4 {
                let delay = -(rand(26) as i32);
                let alpha = (125 + rand(51)) as f32;
                let heading = rand(360) as f32;
                let radius = (6 + rand(8)) as f32;
                bands.push(Hop { made, delay, alpha, heading, radius, centre: at });
            }
        }
        self.hops.push(Hops { start: now, bands });
    }

    /// Rage Burst's 32 rays on `unit`, each plane's turn from `rand(n)`.
    pub fn rays(&mut self, unit: Option<u32>, now: Instant, rand: impl FnMut(u32) -> u32) {
        self.make_rays(unit, now, false, rand);
    }

    /// The Dragon Combo's 32 blue rays over everything, the same way.
    pub fn dragon_rays(&mut self, unit: Option<u32>, now: Instant, rand: impl FnMut(u32) -> u32) {
        self.make_rays(unit, now, true, rand);
    }

    fn make_rays(&mut self, unit: Option<u32>, now: Instant, dragon: bool, mut rand: impl FnMut(u32) -> u32) {
        let mut rays = Vec::new();
        for i in 0..8 {
            for k in 0..4 {
                rays.push(Ray { plane: rand(360) as f32, angle: 90 * k + 22 * i });
            }
        }
        self.rays.push(Rays { unit, start: now, rays, dragon });
    }

    /// Windmill's tornado and wisps where the caster stands (`at`), and its spin, on `unit`.
    pub fn windmill(&mut self, unit: Option<u32>, at: Vector, now: Instant, rand: impl FnMut(u32) -> u32) {
        self.tornado(at, now, None, rand);
        self.spins.push(Spin { unit, start: now, done: 0, table: false });
    }

    /// Moon Slasher's and Sky Net Blow's spin of `unit`.
    pub fn moon_spin(&mut self, unit: Option<u32>, now: Instant) {
        self.spins.push(Spin { unit, start: now, done: 0, table: true });
    }

    /// Reverberation's red tornado and wisps at a trap's cell `at`, gone with the trap `id`.
    pub fn trap_tornado(&mut self, id: u32, at: Vector, now: Instant, rand: impl FnMut(u32) -> u32) {
        self.tornado(at, now, Some(id), rand);
    }

    /// A trap went: its tornado with it; a unit went: its cloak.
    pub fn forget(&mut self, id: u32) {
        self.tornados.retain(|tornado| tornado.kept != Some(id));
        self.cloaks.retain(|cloak| cloak.unit != Some(id));
        self.manholes.retain(|unit| *unit != Some(id));
    }

    /// A status of Camouflage's on `unit` (replacing the last).
    pub fn cloak(&mut self, unit: Option<u32>, now: Instant) {
        self.cloaks.retain(|cloak| cloak.unit != unit);
        self.cloaks.push(Cloak { unit, start: now, ended: None, black: false });
    }

    /// Shadow Form's status on `unit`: a faint black silhouette (replacing the last).
    pub fn shadow(&mut self, unit: Option<u32>, now: Instant) {
        self.cloaks.retain(|cloak| cloak.unit != unit);
        self.cloaks.push(Cloak { unit, start: now, ended: None, black: true });
    }

    /// Manhole's status on `unit`: it sinks and turns to a ghost until the status goes (one to a unit).
    pub fn manhole(&mut self, unit: Option<u32>) {
        self.manholes.retain(|each| *each != unit);
        self.manholes.push(unit);
    }

    /// A status of `unit`'s went off: Manhole's body with it.
    pub fn forget_status(&mut self, unit: Option<u32>, status: u16) {
        if status == 422 {
            self.manholes.retain(|each| *each != unit);
        }
    }

    /// The disc of Hesperus Lit and Chilly Air under `unit`.
    pub fn disc(&mut self, unit: Option<u32>, now: Instant) {
        self.discs.push(Disc { unit, start: now });
    }

    /// Tetra Vortex's cast on `unit` for `length` ticks, replacing the last.
    pub fn tetra(&mut self, unit: Option<u32>, length: u32, now: Instant) {
        self.tetras.retain(|tetra| tetra.unit != unit);
        self.tetras.push(Tetra { unit, start: now, length, colours: Vec::new() });
    }

    /// Camouflage's status went off `unit`.
    pub fn uncloak(&mut self, unit: Option<u32>, now: Instant) {
        for cloak in self.cloaks.iter_mut().filter(|cloak| cloak.unit == unit && cloak.ended.is_none()) {
            cloak.ended = Some(now);
        }
    }

    /// How opaque `unit` is drawn now (0 to 1) under Camouflage: 50 / 255, climbing back by 8 / 255 a tick once it is over.
    #[cfg(test)]
    pub fn cloak_alpha(&self, unit: Option<u32>, now: Instant) -> Option<f32> {
        self.cloak_tint(unit, now).map(|tint| tint[3])
    }

    /// What `unit`'s parts are multiplied by under Camouflage or Shadow Form: white or black at alpha 50 / 255, both climbing back
    /// by 8 / 255 a tick once it is over.
    pub fn cloak_tint(&self, unit: Option<u32>, now: Instant) -> Option<[f32; 4]> {
        self.cloaks.iter().filter(|cloak| cloak.unit == unit).find_map(|cloak| {
            passes(cloak.start, now).checked_sub(1)?;
            let climbed = cloak.ended.map_or(0, |ended| 8 * passes(ended, now));
            let alpha = 50 + climbed;
            let rgb = if cloak.black { climbed.min(255) as f32 / 255.0 } else { 1.0 };
            (alpha < 255).then_some([rgb, rgb, rgb, alpha as f32 / 255.0])
        })
    }

    /// The colour a finished Tetra Vortex cast left on `unit`, fading back to white by 8 a channel a tick.
    pub fn body_tint(&self, unit: Option<u32>, now: Instant) -> Option<[u8; 3]> {
        self.tetras.iter().filter(|tetra| tetra.unit == unit).find_map(|tetra| {
            let e = passes(tetra.start, now).checked_sub(1)?;
            let after = e.checked_sub(tetra.length.saturating_sub(1)).filter(|&after| after > 0)?;
            let from = tetra.colours.last().copied()?;
            let tint = from.map(|channel| (u16::from(channel) + (8 * after) as u16).min(255) as u8);
            (tint != [255; 3]).then_some(tint)
        })
    }

    fn tornado(&mut self, at: Vector, now: Instant, kept: Option<u32>, mut rand: impl FnMut(u32) -> u32) {
        let first = rand(360) as f32;
        let mut wisps = Vec::new();
        for (height, radius, drop) in [(48.0, 14.0, 5.0), (28.0, 6.0, 5.0)] {
            for k in 0..4 {
                let step = if radius > 10.0 { 2.0 } else { 1.0 };
                let lean = 80.0 + rand(21) as f32;
                wisps.push(Wisp { height: height - drop * k as f32, turn: 90.0 * k as f32, radius: radius - step * k as f32, lean });
            }
        }
        self.tornados.push(Tornado { at, start: now, first, turns: Vec::new(), wisps, red: kept.is_some(), kept });
    }

    /// Dark Illusion's 32 lines from the caster's feet `from` toward `to`, each streak's choices from `rand(n)`.
    pub fn lines(&mut self, from: Vector, to: Vector, now: Instant, rand: impl FnMut(u32) -> u32) {
        self.make_lines(LineKind::Pale, from, to, now, rand);
    }

    /// Hundred Spear's 32 gold lines, quicker and longer, the same way.
    pub fn spear_lines(&mut self, from: Vector, to: Vector, now: Instant, rand: impl FnMut(u32) -> u32) {
        self.make_lines(LineKind::Gold, from, to, now, rand);
    }

    /// Knuckle Arrow's 32 blue lines over everything, the same way.
    pub fn knuckle_lines(&mut self, from: Vector, to: Vector, now: Instant, rand: impl FnMut(u32) -> u32) {
        self.make_lines(LineKind::Knuckle, from, to, now, rand);
    }

    fn make_lines(&mut self, kind: LineKind, from: Vector, to: Vector, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        let mut strips = Vec::new();
        for prim in 0..STRIP_PRIMS {
            for _ in 0..4 {
                let side = (15 + rand(11)) as f32 * if prim % 2 == 0 { -1.0 } else { 1.0 };
                let phase = rand(200) as f32;
                let half = if kind == LineKind::Gold { 0.5 + 0.1 * rand(6) as f32 } else { 0.3 + 0.1 * rand(4) as f32 };
                let length = if kind == LineKind::Gold { (40 + rand(31)) as f32 } else { (20 + rand(31)) as f32 };
                let colour = rand(3) as usize;
                let (speed, growth) = match kind {
                    LineKind::Gold => (1.0 + 0.1 * rand(11) as f32, 0.05),
                    LineKind::Knuckle => (1.5 + 0.1 * rand(21) as f32, 0.05),
                    LineKind::Pale => (1.5 + 0.1 * rand(21) as f32, 0.0),
                };
                strips.push(Strip { side, phase, half, length, colour, speed, growth });
            }
        }
        let heading = bearing(to[0] - from[0], to[2] - from[2]) as f32;
        self.lines.push(Lines { kind, start: now, from, heading, strips });
    }

    /// A shake of `unit`'s parts on ticks `first` to `last` (Dark Illusion's 0 to 34, Sky Net Blow's 31 to 50).
    pub fn shake(&mut self, unit: Option<u32>, now: Instant, first: u32, last: u32) {
        self.shakes.push(Shake { unit, start: now, first, last });
    }

    /// Whether `unit`'s parts shake now.
    pub fn shaking(&self, unit: Option<u32>, now: Instant) -> bool {
        self.shakes
            .iter()
            .filter(|shake| shake.unit == unit)
            .any(|shake| passes(shake.start, now).checked_sub(1).is_some_and(|t| (shake.first..=shake.last).contains(&t)))
    }

    /// Sky Net Blow's and Infrared Scan's red flash of `unit`.
    pub fn red(&mut self, unit: Option<u32>, now: Instant) {
        self.reds.push(Marked { unit, start: now });
    }

    /// Marsh of Abyss' and Great Echo's head swing on `unit`.
    pub fn swing(&mut self, unit: Option<u32>, now: Instant) {
        self.swings.push(Marked { unit, start: now });
    }

    /// Hovering's and Emergency Cool's blue copies on `unit`.
    pub fn blue(&mut self, unit: Option<u32>, now: Instant) {
        self.blues.push(Marked { unit, start: now });
    }

    /// Sound of Destruction's sliding copies on `unit`.
    pub fn spread(&mut self, unit: Option<u32>, now: Instant) {
        self.spreads.push(Spread { unit, start: now, first: 30, last: 99 });
    }

    /// The chi burst's four sliding copies of `unit`, from its first tick.
    pub fn chi_spread(&mut self, unit: Option<u32>, now: Instant) {
        self.spreads.push(Spread { unit, start: now, first: 0, last: 29 });
    }

    /// Runs the tornados' turns up to `now`, a tick at a time (3 to 5 degrees each from `rand(n)`), and drops what ended.
    pub fn advance(&mut self, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        for tornado in &mut self.tornados {
            let due = passes(tornado.start, now).min(TORNADO_LAST) as usize;
            while tornado.turns.len() < due {
                let turn = tornado.turns.last().copied().unwrap_or(tornado.first);
                tornado.turns.push(turn + 3.0 + rand(3) as f32);
            }
        }
        for tetra in &mut self.tetras {
            let due = passes(tetra.start, now).min(tetra.length) as usize;
            while tetra.colours.len() < due {
                // The first draw is the blue, the second the green, the third the red.
                let (blue, green, red) = (rand(256) as u8, rand(256) as u8, rand(256) as u8);
                tetra.colours.push([red, green, blue]);
            }
        }
        self.tetras.retain(|tetra| passes(tetra.start, now) <= tetra.length + 33);
        self.discs.retain(|disc| passes(disc.start, now) <= DISC_LAST + 1);
        self.flares.retain(|flares| passes(flares.start, now) <= 30);
        self.hops.retain(|hops| passes(hops.start, now) <= 100);
        self.rays.retain(|rays| passes(rays.start, now) <= RAY_LAST + 1);
        self.tornados.retain(|tornado| passes(tornado.start, now) <= TORNADO_LAST + 1);
        self.lines.retain(|lines| passes(lines.start, now) <= if lines.kind.fast() { GOLD_LAST } else { STRIP_LAST } + 1);
        self.spreads.retain(|spread| passes(spread.start, now) <= 100);
        self.spins.retain(|spin| passes(spin.start, now) <= SPIN_TICKS + 1);
        self.shakes.retain(|shake| passes(shake.start, now) <= shake.last + 1);
        self.reds.retain(|red| passes(red.start, now) <= 17);
        self.cloaks.retain(|cloak| cloak.ended.is_none_or(|ended| passes(ended, now) <= 26));
        self.swings.retain(|swing| passes(swing.start, now) <= 50);
        self.blues.retain(|blue| passes(blue.start, now) <= 200);
    }

    /// How many degrees `unit` has turned since this was last asked (30 a tick on ticks 0 to 47).
    pub fn turned(&mut self, unit: Option<u32>, now: Instant) -> f32 {
        let mut degrees = 0.0;
        for spin in self.spins.iter_mut().filter(|spin| spin.unit == unit) {
            let due = passes(spin.start, now).min(if spin.table { 46 } else { SPIN_TICKS });
            degrees += if spin.table { (spin.done..due).map(spin_step).sum() } else { 30.0 * due.saturating_sub(spin.done) as f32 };
            spin.done = spin.done.max(due);
        }
        degrees
    }

    /// The body flags and colour an effect gives `unit` now: Rage Burst adds the body on ticks 5 to 35, Sound of Destruction's copies
    /// come with ticks 30 to 99 (the age n = t − 30 in the flags' upper half).
    pub fn glow(&self, unit: Option<u32>, now: Instant) -> Option<(u32, Option<[u8; 3]>)> {
        let added = self
            .rays
            .iter()
            .filter(|rays| rays.unit == unit && !rays.dragon)
            .any(|rays| passes(rays.start, now).checked_sub(1).is_some_and(|t| (5..=35).contains(&t)));
        let spread = self.spreads.iter().filter(|spread| spread.unit == unit).find_map(|spread| {
            let t = passes(spread.start, now).checked_sub(1).filter(|t| (spread.first..=spread.last).contains(t))?;
            Some(crate::actor::GLOW_SPREAD | (t - spread.first) << crate::actor::PARAM_SHIFT)
        });
        // Marsh of Abyss' swing on ticks 14 to 49 (the age in the upper half) and Hovering's blue copies on ticks 0 to 199 (the alpha).
        let swing = self.swings.iter().filter(|swing| swing.unit == unit).find_map(|swing| {
            let t = passes(swing.start, now).checked_sub(1).filter(|t| (14..50).contains(t))?;
            Some(crate::actor::GLOW_SWING | (t - 14) << crate::actor::PARAM_SHIFT)
        });
        let blue = self.blues.iter().filter(|blue| blue.unit == unit).find_map(|blue| {
            let t = passes(blue.start, now).checked_sub(1).filter(|&t| t < 200)?;
            Some(crate::actor::GLOW_BLUE | (crate::actor::blue_alpha(t) as u32) << crate::actor::PARAM_SHIFT)
        });
        // Sky Net Blow's red copies on ticks 0 to 16, three phases a tick.
        let red = self.reds.iter().filter(|red| red.unit == unit).find_map(|red| {
            let t = passes(red.start, now).checked_sub(1).filter(|&t| t <= 16)?;
            Some(crate::actor::GLOW_RED | (crate::actor::copies_alpha(3 * t) as u32) << crate::actor::PARAM_SHIFT)
        });
        // Tetra Vortex's cast: its colour each tick, added on when the tick's number mod 5 is 0 or 1.
        let tetra = self.tetras.iter().filter(|tetra| tetra.unit == unit).find_map(|tetra| {
            let e = passes(tetra.start, now).checked_sub(1).filter(|&e| e < tetra.length)?;
            let colour = tetra.colours.get(e as usize).or(tetra.colours.last()).copied()?;
            Some((if e % 5 <= 1 { crate::actor::GLOW_ADDITIVE } else { 0 }, colour))
        });
        let manhole = self.manholes.contains(&unit);
        match (added, spread.or(swing).or(blue).or(red), tetra) {
            (false, None, None) if !manhole => None,
            (added, extra, tetra) => {
                let flags = if added { crate::actor::GLOW_ADDITIVE } else { 0 }
                    | extra.unwrap_or(0)
                    | tetra.map_or(0, |(flags, _)| flags)
                    | if manhole { crate::actor::GLOW_MANHOLE } else { 0 };
                Some((flags, tetra.map(|(_, colour)| colour)))
            }
        }
    }

    /// The streaks flat on the screen over their units now.
    pub fn shapes(&self, now: Instant) -> Vec<ScreenShape> {
        let mut shapes = Vec::new();
        for flares in &self.flares {
            let Some(t) = passes(flares.start, now).checked_sub(1) else { continue };
            shapes.extend(flares.flares.iter().filter(|flare| t < flare.life).map(|flare| (LENS[flare.lens], flares.unit, flare.shape(t))));
        }
        shapes
    }

    /// Every mesh drawn in the world at `now`; `position_of` gives a unit's feet.
    pub fn meshes(&self, now: Instant, position_of: impl Fn(Option<u32>) -> Option<Vector>) -> Vec<Wall> {
        let mut walls = Vec::new();
        for hops in &self.hops {
            let Some(t) = passes(hops.start, now).checked_sub(1) else { continue };
            let mut vertices = Vec::new();
            for hop in &hops.bands {
                let e = hop.age(t);
                if e >= 1 {
                    for (j, a, b, alpha) in hop.segments(e) {
                        vertices.extend(Hop::tube(a, b, j, alpha));
                    }
                }
            }
            if !vertices.is_empty() {
                walls.push(Wall { texture: WHITE, vertices, additive: true, forced: false });
            }
        }
        for rays in self.rays.iter().filter(|rays| !rays.dragon) {
            let (Some(feet), Some(t)) = (position_of(rays.unit), passes(rays.start, now).checked_sub(1)) else { continue };
            let a = t + 1;
            if a <= RAY_LAST {
                let vertices = rays.rays.iter().flat_map(|ray| ray.wedges(feet, a, false)).collect();
                walls.push(Wall { texture: RAY, vertices, additive: true, forced: false });
            }
        }
        for tornado in &self.tornados {
            let Some(t) = passes(tornado.start, now).checked_sub(1) else { continue };
            let a = t + 1;
            if a <= TORNADO_LAST {
                let turn = tornado.turn_at(a);
                let alpha = tornado_alpha(a);
                if tornado.red {
                    // Each square twice: a white veil of a third of the alpha, then red added on.
                    let veil = (0..SQUARES.len()).flat_map(|i| tornado.square(i, turn, [1.0; 3], (alpha / 3.0).floor())).collect();
                    walls.push(Wall { texture: STORM, vertices: veil, additive: false, forced: false });
                }
                let rgb = if tornado.red { [1.0, 55.0 / 255.0, 55.0 / 255.0] } else { [50.0 / 255.0, 50.0 / 255.0, 1.0] };
                let vertices = (0..SQUARES.len()).flat_map(|i| tornado.square(i, turn, rgb, alpha)).collect();
                walls.push(Wall { texture: STORM, vertices, additive: true, forced: false });
            }
            if a <= WISP_LAST {
                for wisp in &tornado.wisps {
                    let turn = (wisp.turn + 5.0 * a as f32) % 360.0;
                    let radius = wisp.radius + 0.1 * a.saturating_sub(12) as f32;
                    let span = (30.0 + 3.0 * a as f32).min(180.0);
                    let vertices = arc_ribbon(tornado.at, wisp.height, turn, radius, wisp.lean, span, wisp_alpha(a));
                    walls.push(Wall { texture: CLOUD, vertices, additive: true, forced: false });
                }
            }
        }
        walls
    }

    /// Dark Illusion's lines and the Dragon Combo's rays, drawn over everything with no depth test.
    pub fn top_meshes(&self, now: Instant, position_of: impl Fn(Option<u32>) -> Option<Vector>) -> Vec<Wall> {
        let mut walls = Vec::new();
        for disc in &self.discs {
            let (Some(feet), n) = (position_of(disc.unit), passes(disc.start, now)) else { continue };
            if n == 0 || n > DISC_LAST {
                continue;
            }
            let half = disc_half(n);
            let tint = [5.0 / 255.0, 5.0 / 255.0, 1.0, disc_alpha(n) / 255.0];
            let corner = |x: f32, z: f32, uv: [f32; 2]| MeshVertex { position: [feet[0] + x * half, feet[1] + 5.0, feet[2] + z * half], uv, tint };
            let [a, b, c, d] = [corner(1.0, 0.0, [0.0, 1.0]), corner(0.0, 1.0, [1.0, 1.0]), corner(-1.0, 0.0, [1.0, 0.0]), corner(0.0, -1.0, [0.0, 0.0])];
            walls.push(Wall { texture: BLAST, vertices: vec![a, b, c, a, c, d], additive: true, forced: false });
        }
        for rays in self.rays.iter().filter(|rays| rays.dragon) {
            let (Some(feet), Some(t)) = (position_of(rays.unit), passes(rays.start, now).checked_sub(1)) else { continue };
            let a = t + 1;
            if a <= RAY_LAST {
                let vertices = rays.rays.iter().flat_map(|ray| ray.wedges(feet, a, true)).collect();
                walls.push(Wall { texture: RAY, vertices, additive: true, forced: false });
            }
        }
        for lines in &self.lines {
            let Some(t) = passes(lines.start, now).checked_sub(1) else { continue };
            let a = t + 1;
            if a > if lines.kind.fast() { GOLD_LAST } else { STRIP_LAST } {
                continue;
            }
            let (sin, cos) = lines.heading.to_radians().sin_cos();
            let (side_sin, side_cos) = (lines.heading - 90.0).to_radians().sin_cos();
            let mut vertices = Vec::new();
            for strip in &lines.strips {
                let middle = [lines.from[0] + strip.side * side_cos, lines.from[1] + 6.0, lines.from[2] + strip.side * side_sin];
                let (head, tail) = (strip.at(a) + strip.length - 100.0, strip.at(a) - 100.0);
                let place = |along: f32, across: f32| [middle[0] + cos * along - sin * across, middle[1], middle[2] + sin * along + cos * across];
                let rgb = match lines.kind {
                    LineKind::Gold => GOLD_TINTS[strip.colour],
                    LineKind::Knuckle => KNUCKLE_TINTS[strip.colour],
                    LineKind::Pale => STRIP_TINTS[strip.colour],
                };
                let alpha = if lines.kind.fast() { gold_alpha(a) } else { strip_alpha(a) };
                let tint = [rgb[0], rgb[1], rgb[2], alpha / 255.0];
                let [p, q, r, s] = [
                    MeshVertex { position: place(head, -strip.half), uv: [0.0, 0.0], tint },
                    MeshVertex { position: place(head, strip.half), uv: [0.0, 1.0], tint },
                    MeshVertex { position: place(tail, strip.half), uv: [1.0, 1.0], tint },
                    MeshVertex { position: place(tail, -strip.half), uv: [1.0, 0.0], tint },
                ];
                vertices.extend([p, q, r, p, r, s]);
            }
            walls.push(Wall {
                texture: if lines.kind == LineKind::Gold { GOLD_STRIP } else { STRIP },
                vertices,
                additive: lines.kind == LineKind::Knuckle,
                forced: false,
            });
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

    #[test]
    fn self_destruction_bursts_eight_streaks() {
        let start = Instant::now();
        let mut brawl = Brawl::default();
        brawl.flares(Some(1), start, |_| 0);
        let flares = &brawl.flares[0].flares;
        assert_eq!(flares.len(), 8);
        // Every draw 0: life 10, yaw a − 15, speed 0.5, half-width 5, half-height 20, the start (0, −20); the lens picture by prim.
        assert!(flares.iter().all(|flare| flare.life == 10 && flare.speed == 0.5 && flare.half == 5.0 && flare.tall == 20.0 && flare.offset == [0.0, -20.0]));
        assert_eq!(flares.iter().map(|flare| flare.yaw).collect::<Vec<_>>(), [-15.0, 30.0, 75.0, 120.0, 165.0, 210.0, 255.0, 300.0]);
        assert_eq!(flares.iter().map(|flare| flare.lens).collect::<Vec<_>>(), [0, 1, 0, 1, 0, 1, 0, 1]);
        // Alpha at k = 0 and 7: 31.75 and 254; the half-height at k = 9: 20 + 28.75.
        assert!(near(flare_alpha(0), 31.75) && flare_alpha(7) == 254.0);
        assert!(near(flares[0].half_height(9), 48.75));
        // The width thins to nothing at the life; the flight is 0.5 · n less a bit.
        assert!(near(flares[0].half_width(9), 0.0) && near(flares[0].half_width(4), 2.5));
        assert!(near(flares[0].flown(0), 0.5 - 0.5 * 2.0 / 40.0));
        // Drawn on ticks 0 to 9 only, over the unit, in screen px 80 over its feet.
        assert_eq!(brawl.shapes(start).len(), 8);
        assert_eq!(brawl.shapes(start + TICK * 9).len(), 8);
        assert!(brawl.shapes(start + TICK * 10).is_empty());
        let shape = &brawl.shapes(start)[0];
        assert!(shape.0 == LENS[0] && shape.1 == Some(1) && shape.2.len() == 6);
        // Prim a = 0 points straight up: the quad's far edge (v 0) is above its near one.
        let corners = &shape.2;
        assert!(corners[0].1 == [0.0, 0.0] && corners[0].0[1] < corners[2].0[1]);
        assert!(corners.iter().all(|corner| near(corner.2[0], 120.0 / 255.0) && corner.2[2] == 1.0));
    }

    #[test]
    fn storm_blast_hops_16_lines_away() {
        let start = Instant::now();
        let mut brawl = Brawl::default();
        brawl.hops([10.0, 1.0, 10.0], start, |_| 0);
        let bands = &brawl.hops[0].bands;
        // Four prims of four, made on ticks 0 to 3; every draw 0: no wait, alpha 125, heading 0, radius 6.
        assert_eq!(bands.len(), 16);
        assert_eq!((0..4).map(|made| bands.iter().filter(|band| band.made == made).count()).collect::<Vec<_>>(), [4, 4, 4, 4]);
        assert!(bands.iter().all(|band| band.delay == 0 && band.alpha == 125.0 && band.heading == 0.0 && band.radius == 6.0));
        // The head phase after 23 ticks is 4 with 1 hop; hops last 23, 22, 23 ticks.
        assert_eq!(hop_state(23), (4.0, 1));
        assert_eq!(hop_state(45), (0.0, 2));
        assert_eq!(hop_state(68), (4.0, 3));
        // The alpha is 125 to age 35, 5 less a tick after, and gone at age 60.
        assert_eq!([35, 36, 59, 60].map(|e| bands[0].alpha_at(e)), [125.0, 120.0, 5.0, 0.0]);
        // Segment 3 has alpha 110 and the colour (255, 200, 31); segment 0 has no length.
        let segments = bands[0].segments(30);
        assert!(segments.iter().all(|segment| segment.0 != 0));
        let third = segments.iter().find(|segment| segment.0 == 3).unwrap();
        assert!(near(third.3, 110.0));
        let tube = Hop::tube(third.1, third.2, 3, third.3);
        assert_eq!(tube.len(), 18);
        assert!(near(tube[0].tint[0] * 255.0, 255.0) && near(tube[0].tint[1] * 255.0, 200.0) && near(tube[0].tint[2] * 255.0, 31.0));
        // Its first hop at heading 0 runs along +x, 2R = 12 long, 9 high at the middle (1.5 R).
        let top = bands[0].point(90.0, 0);
        assert!(near(top[0], 10.0 + 6.0) && near(top[1], 1.0 + 9.0) && near(top[2], 10.0));
        assert!(near(bands[0].point(0.0, 1)[0], 10.0 + 12.0));
        // A line that has not begun draws nothing; a prim made on tick 3 waits three ticks longer than one made on tick 0.
        let mut lazy = Brawl::default();
        lazy.hops([0.0; 3], start, |_| 5);
        assert_eq!(lazy.hops[0].bands[0].age(5), 1);
        assert_eq!(lazy.hops[0].bands[12].age(5), -2);
        assert!(brawl.meshes(start, |_| None).iter().all(|wall| wall.texture == WHITE && wall.additive) && !brawl.meshes(start, |_| None).is_empty());
        // The last prim (made on tick 3) is gone at age 60, which is tick 62; nothing follows the unit.
        assert!(!brawl.meshes(start + TICK * 58, |_| None).is_empty());
        assert!(brawl.meshes(start + TICK * 62, |_| None).is_empty());
    }

    #[test]
    fn rage_burst_shoots_32_rays() {
        let start = Instant::now();
        let mut brawl = Brawl::default();
        brawl.rays(Some(1), start, |_| 0);
        let rays = &brawl.rays[0].rays;
        assert_eq!(rays.len(), 32);
        // L = 90k + 22i for (i, k) = (0, 0), (1, 0), (7, 3).
        assert_eq!([rays[0].angle, rays[4].angle, rays[31].angle], [0, 22, 424]);
        assert_eq!([1, 10, 11, 12, 44, 45].map(ray_alpha), [10.0, 100.0, 100.0, 97.0, 1.0, 0.0]);
        assert_eq!([1, 10, 44].map(ray_reach), [5.0, 32.0, 134.0]);
        // The sliver ray (i = 5, k = 3, L = 380): wedge 1 at 198° and 21°, wedge 2 at 197° and 22°.
        let sliver = Ray { plane: 0.0, angle: 380 };
        assert_eq!(sliver.rim(), [[198.0, 21.0], [197.0, 22.0]]);
        assert_eq!(Ray { plane: 0.0, angle: 424 }.rim(), [[242.0, 65.0], [241.0, 66.0]]);
        // A negative angle reads a degree up.
        assert_eq!(rays[0].rim(), [[0.0, 1.0], [-1.0, 2.0]]);
        // 32 rays × 2 wedges × 3 corners, added on, drawn for ages 1 to 44 and following the unit.
        let feet = [0.0; 3];
        let walls = brawl.meshes(start, |_| Some(feet));
        assert_eq!(walls.len(), 1);
        assert!(walls[0].additive && walls[0].vertices.len() == 192 && walls[0].texture == RAY);
        assert_eq!(walls[0].vertices[2].position, [0.0, 12.0, 0.0]);
        assert!(brawl.meshes(start + TICK * 43, |_| Some(feet)).len() == 1 && brawl.meshes(start + TICK * 44, |_| Some(feet)).is_empty());
        assert!(brawl.meshes(start, |_| None).is_empty());
        // The body is added on ticks 5 to 35 only.
        let glow = |tick: u32| brawl.glow(Some(1), start + TICK * tick);
        assert!(glow(4).is_none() && glow(5) == Some((crate::actor::GLOW_ADDITIVE, None)) && glow(35).is_some() && glow(36).is_none());
        assert!(brawl.glow(Some(2), start + TICK * 5).is_none());
    }

    #[test]
    fn sound_of_destruction_slides_four_copies() {
        let start = Instant::now();
        let mut brawl = Brawl::default();
        brawl.spread(Some(1), start);
        let glow = |tick: u32| brawl.glow(Some(1), start + TICK * tick);
        assert!(glow(29).is_none() && glow(100).is_none());
        for (tick, n) in [(30, 0), (31, 1), (59, 29), (99, 69)] {
            assert_eq!(glow(tick), Some((crate::actor::GLOW_SPREAD | n << 24, None)));
        }
        // The copies: the age n → 4 · n px · unit right, left, down, up, alpha 5 · (30 − n).
        let (_, copies) = crate::actor::glow_copies(crate::actor::GLOW_SPREAD | 29 << 24, None, crate::actor::PartGroup::Body, 0.0, 0.0, 400.0 / 300.0, true);
        assert_eq!(copies.len(), 4);
        assert!(
            near(copies[0].offset[0], 154.67) && near(copies[1].offset[0], -154.67) && near(copies[2].offset[1], 154.67) && near(copies[3].offset[1], -154.67)
        );
        assert!(copies.iter().all(|copy| !copy.additive && near(copy.color[3] * 255.0, 5.0)));
        let (_, first) = crate::actor::glow_copies(crate::actor::GLOW_SPREAD, None, crate::actor::PartGroup::Gear, 0.0, 0.0, 400.0 / 300.0, false);
        assert!(first.len() == 4 && first.iter().all(|copy| copy.offset == [0.0; 2]) && near(first[0].color[3] * 255.0, 150.0));
        let (_, gone) = crate::actor::glow_copies(crate::actor::GLOW_SPREAD | 30 << 24, None, crate::actor::PartGroup::Body, 0.0, 0.0, 1.0, true);
        assert!(gone.is_empty());
    }

    #[test]
    fn windmill_spins_the_caster_in_a_tornado() {
        let start = Instant::now();
        let mut brawl = Brawl::default();
        let mut draws = 0;
        brawl.windmill(Some(1), [10.0, 1.0, 10.0], start, |_| {
            draws += 1;
            0
        });
        // One draw for the start turn and one for each of the eight wisps' leans.
        assert_eq!(draws, 9);
        // The caster turns 30° a tick on ticks 0 to 47 only: 1 440° in all.
        let mut total = 0.0;
        for tick in 0..120 {
            total += brawl.turned(Some(1), start + TICK * tick);
        }
        assert_eq!(total, 1440.0);
        assert_eq!(brawl.turned(Some(2), start + TICK * 10), 0.0);
        // The ten squares; alpha at a = 1, 40, 60, 61, 99: 1, 40, 40, 39, 1.
        assert_eq!([1, 40, 60, 61, 99].map(tornado_alpha), [1.0, 40.0, 40.0, 39.0, 1.0]);
        assert_eq!(SQUARES.len(), 10);
        for tick in 0..100 {
            brawl.advance(start + TICK * tick, |_| 1);
        }
        let walls = brawl.meshes(start, |_| None);
        // The first tick: the tornado's 10 squares and 8 wisp bands.
        assert_eq!(walls.iter().filter(|wall| wall.texture == STORM).count(), 1);
        assert_eq!(walls.iter().filter(|wall| wall.texture == STORM).map(|wall| wall.vertices.len()).sum::<usize>(), 60);
        assert_eq!(walls.iter().filter(|wall| wall.texture == CLOUD).count(), 8);
        assert!(walls.iter().all(|wall| wall.additive));
        // The wisps' heights 48, 43, 38, 33 and 28, 23, 18, 13; radii 14 … 8 and 6 … 3; alpha at a = 11, 50, 51, 61: 11, 11, 10, 0.
        assert_eq!(brawl.tornados[0].wisps.iter().map(|wisp| wisp.height).collect::<Vec<_>>(), [48.0, 43.0, 38.0, 33.0, 28.0, 23.0, 18.0, 13.0]);
        assert_eq!(brawl.tornados[0].wisps.iter().map(|wisp| wisp.radius).collect::<Vec<_>>(), [14.0, 12.0, 10.0, 8.0, 6.0, 5.0, 4.0, 3.0]);
        assert_eq!([11, 50, 51, 61].map(wisp_alpha), [11.0, 11.0, 10.0, 0.0]);
        // The turn grows 3 + R(3) a tick (here 4); the wisps are gone after 60 ticks and the tornado after 99.
        assert_eq!(brawl.tornados[0].turns[0], 4.0);
        assert!(!brawl.meshes(start + TICK * 60, |_| None).iter().any(|wall| wall.texture == CLOUD));
        assert!(brawl.meshes(start + TICK * 98, |_| None).iter().any(|wall| wall.texture == STORM));
        assert!(brawl.meshes(start + TICK * 99, |_| None).is_empty());
    }

    #[test]
    fn dark_illusion_races_32_pale_lines() {
        let start = Instant::now();
        let mut brawl = Brawl::default();
        brawl.lines([10.0, 1.0, 10.0], [50.0, 1.0, 10.0], start, |_| 0);
        let strips = &brawl.lines[0].strips;
        assert_eq!(strips.len(), 32);
        // 16 on each side; every draw 0: s ∓ 15, phase 0, half-width 0.3, length 20, speed 1.5, colour #E1E1FF.
        assert_eq!(strips.iter().filter(|strip| strip.side == -15.0).count(), 16);
        assert_eq!(strips.iter().filter(|strip| strip.side == 15.0).count(), 16);
        assert!(strips.iter().all(|strip| strip.phase == 0.0 && near(strip.half, 0.3) && strip.length == 20.0 && strip.speed == 1.5 && strip.colour == 0));
        assert_eq!([10, 20, 21, 50].map(strip_alpha), [120.0, 120.0, 116.0, 0.0]);
        // It wraps by 200: 199.5 at a = 133 and 1.0 at 134.
        assert!(near(strips[0].at(133), 199.5) && near(strips[0].at(134), 1.0));
        // Due east (bearing 0 along +x): a strip lies flat 6 up, 100 units back at first.
        let walls = brawl.top_meshes(start, |_| Some([0.0; 3]));
        assert_eq!(walls.len(), 1);
        assert!(!walls[0].additive && walls[0].texture == STRIP && walls[0].vertices.len() == 192);
        let first = &walls[0].vertices;
        assert!(near(first[0].position[1], 7.0));
        assert!(first.iter().all(|vertex| near(vertex.tint[3] * 255.0, 12.0)));
        // The first strip (side −15, heading 0) lies 15 to the +z side, its head 78.5 back along x, a half-width off its middle.
        assert!(near(first[0].position[0], 10.0 - 78.5) && near(first[0].position[2], 25.0 - 0.3));
        assert!(brawl.top_meshes(start + TICK * 48, |_| Some([0.0; 3])).len() == 1 && brawl.top_meshes(start + TICK * 49, |_| Some([0.0; 3])).is_empty());
    }

    #[test]
    fn the_game_reads_angles_from_its_table() {
        assert_eq!([-2, -1, 0, 360, 361, 379, 424].map(table), [-1.0, 0.0, 0.0, 360.0, 180.0, 198.0, 243.0]);
    }

    #[test]
    fn dark_illusion_shakes_marsh_of_abyss_swings_and_hovering_glows() {
        let start = Instant::now();
        let mut brawl = Brawl::default();
        brawl.shake(Some(1), start, 0, 34);
        brawl.swing(Some(2), start);
        brawl.blue(Some(3), start);
        // The shake is on ticks 0 to 34 only, for its unit.
        assert!(brawl.shaking(Some(1), start) && brawl.shaking(Some(1), start + TICK * 34));
        assert!(!brawl.shaking(Some(1), start + TICK * 35) && !brawl.shaking(Some(2), start));
        // The swing is set on ticks 14 to 49 with its age n = t − 14.
        let swing = |tick: u32| brawl.glow(Some(2), start + TICK * tick);
        assert!(swing(13).is_none() && swing(50).is_none());
        assert_eq!(swing(14), Some((crate::actor::GLOW_SWING, None)));
        assert_eq!(swing(49), Some((crate::actor::GLOW_SWING | 35 << 24, None)));
        // The blue copies: ticks 0 to 199, the alpha in the upper half.
        let blue = |tick: u32| brawl.glow(Some(3), start + TICK * tick);
        assert_eq!(blue(0), Some((crate::actor::GLOW_BLUE, None)));
        assert_eq!(blue(11), Some((crate::actor::GLOW_BLUE | 160 << 24, None)));
        assert!(blue(199).is_some() && blue(200).is_none());
    }

    #[test]
    fn reverberation_draws_each_square_twice_and_goes_with_its_trap() {
        let start = Instant::now();
        let mut brawl = Brawl::default();
        brawl.trap_tornado(7, [10.0, 1.0, 10.0], start, |_| 0);
        for tick in 0..=60 {
            brawl.advance(start + TICK * tick, |_| 0);
        }
        // At age 60: a white veil at a third of the alpha, then red added on; the wisps beside them (8 bands).
        let walls = brawl.meshes(start + TICK * 59, |_| None);
        let storm: Vec<_> = walls.iter().filter(|wall| wall.texture == STORM).collect();
        assert_eq!(storm.len(), 2);
        assert!(!storm[0].additive && storm[0].vertices[0].tint == [1.0, 1.0, 1.0, 13.0 / 255.0]);
        assert!(storm[1].additive && storm[1].vertices[0].tint == [1.0, 55.0 / 255.0, 55.0 / 255.0, 40.0 / 255.0]);
        assert_eq!(walls.iter().filter(|wall| wall.texture == CLOUD).count(), 8);
        // Nothing is made later, and the trap going takes it away.
        brawl.forget(8);
        assert_eq!(brawl.meshes(start + TICK * 59, |_| None).len(), 10);
        brawl.forget(7);
        assert!(brawl.meshes(start + TICK * 59, |_| None).is_empty());
    }

    #[test]
    fn sky_net_blow_flashes_red_and_shakes_late_and_camouflage_goes_translucent() {
        use crate::actor::{GLOW_RED, PARAM_SHIFT, PartGroup, glow_copies};
        let start = Instant::now();
        let mut brawl = Brawl::default();
        brawl.red(Some(1), start);
        brawl.shake(Some(1), start, 31, 50);
        // The red copies' alpha at t = 0, 1, 3, 4, 6, 7, 10, 16, 17: 0, 45, 135, 160, 160, 150, 105, 15, none.
        let alpha = |tick: u32| brawl.glow(Some(1), start + TICK * tick).map(|(flags, _)| flags >> PARAM_SHIFT);
        assert_eq!([0, 1, 3, 4, 6, 7, 10, 16].map(alpha), [0, 45, 135, 160, 160, 150, 105, 15].map(Some));
        assert_eq!(alpha(17), None);
        assert_eq!(brawl.glow(Some(1), start + TICK * 3).unwrap().0 & GLOW_RED, GLOW_RED);
        let (_, copies) = glow_copies(GLOW_RED | 160 << PARAM_SHIFT, None, PartGroup::Body, 0.0, 0.0, 1.0, true);
        assert_eq!(copies.len(), 2);
        assert!(copies.iter().all(|copy| copy.additive && copy.color == [1.0, 5.0 / 255.0, 5.0 / 255.0, 160.0 / 255.0]));
        // The jitter is on ticks 31 to 50 and not on 30 or 51.
        assert!(!brawl.shaking(Some(1), start + TICK * 30) && brawl.shaking(Some(1), start + TICK * 31));
        assert!(brawl.shaking(Some(1), start + TICK * 50) && !brawl.shaking(Some(1), start + TICK * 51));
        // Camouflage: alpha 50 every tick while on, then 58, 66 … back to opaque in 26 ticks; one to a unit.
        let mut cloak = Brawl::default();
        cloak.cloak(Some(2), start);
        cloak.cloak(Some(2), start);
        assert_eq!(cloak.cloaks.len(), 1);
        assert!(cloak.cloak_alpha(Some(3), start).is_none());
        assert!([0, 1, 500].iter().all(|&tick| cloak.cloak_alpha(Some(2), start + TICK * tick) == Some(50.0 / 255.0)));
        let ended = start + TICK * 600;
        cloak.uncloak(Some(2), ended);
        assert_eq!(cloak.cloak_alpha(Some(2), ended), Some(58.0 / 255.0));
        assert_eq!(cloak.cloak_alpha(Some(2), ended + TICK * 1), Some(66.0 / 255.0));
        assert_eq!(cloak.cloak_alpha(Some(2), ended + TICK * 24), Some(250.0 / 255.0));
        assert_eq!(cloak.cloak_alpha(Some(2), ended + TICK * 25), None);
        cloak.advance(ended + TICK * 40, |_| 0);
        assert!(cloak.cloaks.is_empty());
        cloak.cloak(Some(4), start);
        cloak.forget(4);
        assert!(cloak.cloaks.is_empty());
    }

    #[test]
    fn tetra_vortex_flickers_a_fresh_colour_and_shadow_form_goes_black() {
        use crate::actor::GLOW_ADDITIVE;
        let start = Instant::now();
        let mut brawl = Brawl::default();
        brawl.tetra(Some(1), 56, start);
        // The three draws go to blue, green, red: 1, 2, 3 make the colour (3, 2, 1).
        let mut draws = [1, 2, 3].into_iter().cycle();
        for tick in 0..60 {
            brawl.advance(start + TICK * tick, |_| draws.next().unwrap());
        }
        assert_eq!(brawl.tetras[0].colours[0], [3, 2, 1]);
        // Added on when the tick mod 5 is 0 or 1; the colour every tick; nothing after the cast.
        let glow = |tick: u32| brawl.glow(Some(1), start + TICK * tick);
        assert_eq!(glow(0), Some((GLOW_ADDITIVE, Some([3, 2, 1]))));
        assert_eq!(glow(1).unwrap().0, GLOW_ADDITIVE);
        assert_eq!(
            (glow(2).unwrap().0, glow(4).unwrap().0, glow(5).unwrap().0, glow(6).unwrap().0, glow(7).unwrap().0),
            (0, 0, GLOW_ADDITIVE, GLOW_ADDITIVE, 0)
        );
        assert!(glow(55).is_some() && glow(56).is_none());
        // The last colour fades back to white by 8 a channel a tick.
        let last = brawl.tetras[0].colours[55];
        assert_eq!(brawl.body_tint(Some(1), start + TICK * 56), Some(last.map(|channel| channel + 8)));
        // Shadow Form: black at alpha 50 / 255, both climbing back by 8 a tick once it is over.
        let mut shadow = Brawl::default();
        shadow.shadow(Some(2), start);
        assert_eq!(shadow.cloak_tint(Some(2), start + TICK * 3), Some([0.0, 0.0, 0.0, 50.0 / 255.0]));
        shadow.uncloak(Some(2), start + TICK * 10);
        assert_eq!(shadow.cloak_tint(Some(2), start + TICK * 10), Some([8.0 / 255.0, 8.0 / 255.0, 8.0 / 255.0, 58.0 / 255.0]));
        assert_eq!(shadow.cloak_tint(Some(2), start + TICK * 36), None);
    }

    #[test]
    fn manhole_sinks_a_unit_until_its_status_goes() {
        let mut brawl = Brawl::default();
        let start = Instant::now();
        brawl.manhole(Some(1));
        brawl.manhole(Some(1));
        assert_eq!(brawl.manholes.len(), 1);
        assert_eq!(brawl.glow(Some(1), start), Some((crate::actor::GLOW_MANHOLE, None)));
        assert!(brawl.glow(Some(2), start).is_none());
        brawl.forget_status(Some(1), 421);
        assert!(brawl.glow(Some(1), start).is_some());
        brawl.forget_status(Some(1), 422);
        assert!(brawl.glow(Some(1), start).is_none());
        brawl.manhole(None);
        brawl.forget(3);
        assert!(brawl.glow(None, start).is_some());
    }

    #[test]
    fn hundred_spear_races_32_gold_lines_and_lights_the_caster() {
        let start = Instant::now();
        let mut brawl = Brawl::default();
        brawl.spear_lines([10.0, 1.0, 10.0], [50.0, 1.0, 10.0], start, |_| 0);
        let strips = &brawl.lines[0].strips;
        // 16 each side; every draw 0: s ∓ 15, phase 0, half-width 0.5, length 40, speed 1.0, colour #FFFFE1.
        assert_eq!(strips.len(), 32);
        assert_eq!(strips.iter().filter(|strip| strip.side == -15.0).count(), 16);
        assert!(strips.iter().all(|strip| strip.phase == 0.0
            && strip.half == 0.5
            && strip.length == 40.0
            && strip.speed == 1.0
            && strip.colour == 0
            && strip.growth == 0.05));
        // Alpha at n = 1, 10, 20, 21, 120: 20, 200, 200, 198, 0; the speed 6.0 above its start after 120 ticks.
        assert_eq!([1, 10, 20, 21, 120].map(gold_alpha), [20.0, 200.0, 200.0, 198.0, 0.0]);
        assert!((strips[0].at(120) - (1.0 * 120.0 + 0.05 * 120.0 * 121.0 / 2.0) % 200.0).abs() < 1e-3);
        assert_eq!(strips[0].growth * 120.0, 6.0);
        // Drawn over everything in the gold picture, blended; the tints are #FFFFE1 …; gone after tick 118.
        let walls = brawl.top_meshes(start, |_| Some([0.0; 3]));
        assert_eq!(walls.len(), 1);
        assert!(!walls[0].additive && walls[0].texture == GOLD_STRIP && walls[0].vertices[0].tint[..3] == [1.0, 1.0, 225.0 / 255.0]);
        assert!(!brawl.top_meshes(start + TICK * 118, |_| Some([0.0; 3])).is_empty() && brawl.top_meshes(start + TICK * 119, |_| Some([0.0; 3])).is_empty());
        // Dark Illusion's pale lines are unchanged.
        brawl.lines([0.0; 3], [40.0, 0.0, 0.0], start, |_| 0);
        assert!(brawl.lines[1].strips.iter().all(|strip| strip.growth == 0.0 && strip.speed == 1.5));
    }

    #[test]
    fn the_dragon_combo_shoots_blue_rays_over_everything_and_moon_slasher_spins() {
        let start = Instant::now();
        let mut brawl = Brawl::default();
        brawl.dragon_rays(Some(1), start, |_| 0);
        // 32 rays, no body change; the sliver ray L = 380 reads 197 and 194 low, 22 and 25 wrapped high.
        assert_eq!(brawl.rays[0].rays.len(), 32);
        assert!(brawl.glow(Some(1), start + TICK * 10).is_none());
        let sliver = Ray { plane: 0.0, angle: 380 };
        assert_eq!(sliver.rim_by([2, 5]), [[197.0, 22.0], [194.0, 25.0]]);
        // Two wedges a ray in #0000FF and #3232FF, added on, over everything, none in the world's depth-tested pass.
        let feet = [0.0; 3];
        assert!(brawl.meshes(start, |_| Some(feet)).iter().all(|wall| wall.texture != RAY));
        let walls = brawl.top_meshes(start, |_| Some(feet));
        let rays = walls.iter().find(|wall| wall.texture == RAY).unwrap();
        assert!(rays.additive && rays.vertices.len() == 32 * 2 * 3);
        assert_eq!(rays.vertices[0].tint[..3], [0.0, 0.0, 1.0]);
        assert_eq!(rays.vertices[3].tint[..3], [50.0 / 255.0, 50.0 / 255.0, 1.0]);
        assert!(brawl.top_meshes(start + TICK * 44, |_| Some(feet)).iter().all(|wall| wall.texture != RAY));
        // Moon Slasher: the facing after e = 7, 15, 20, 25, 32, 37, 45: 96, 240, 360, 600, 768, 858, 954; nothing from 46.
        let mut spin = Brawl::default();
        spin.moon_spin(Some(1), start);
        let total = |spin: &mut Brawl, tick: u32| spin.turned(Some(1), start + TICK * tick);
        let mut turned = 0.0;
        let mut at = Vec::new();
        for tick in 0..60 {
            turned += total(&mut spin, tick);
            if [7, 15, 20, 25, 32, 37, 45, 59].contains(&tick) {
                at.push(turned);
            }
        }
        assert_eq!(at, [96.0, 240.0, 360.0, 600.0, 768.0, 858.0, 954.0, 954.0]);
        // The chi burst's copies: flagged on ticks 0 to 29 with the age in the flags, none from 30.
        let mut chi = Brawl::default();
        chi.chi_spread(Some(2), start);
        let glow = |tick: u32| chi.glow(Some(2), start + TICK * tick);
        assert_eq!(glow(0), Some((crate::actor::GLOW_SPREAD, None)));
        assert_eq!(glow(29), Some((crate::actor::GLOW_SPREAD | 29 << crate::actor::PARAM_SHIFT, None)));
        assert!(glow(30).is_none());
    }

    #[test]
    fn knuckle_arrow_races_32_blue_lines_and_hesperus_lit_spreads_a_disc() {
        let start = Instant::now();
        let mut brawl = Brawl::default();
        brawl.knuckle_lines([10.0, 1.0, 10.0], [50.0, 1.0, 10.0], start, |_| 0);
        let strips = &brawl.lines[0].strips;
        assert_eq!(strips.len(), 32);
        assert!(strips.iter().all(|strip| strip.speed == 1.5 && strip.growth == 0.05 && strip.half == 0.3 && strip.length == 20.0));
        // Over everything, added on, in #6969FF, #4141FF and #1919FF.
        let walls = brawl.top_meshes(start, |_| Some([0.0; 3]));
        assert!(walls.iter().all(|wall| wall.additive && wall.texture == STRIP));
        let tints: Vec<[f32; 3]> = walls.iter().flat_map(|wall| wall.vertices.iter().map(|vertex| [vertex.tint[0], vertex.tint[1], vertex.tint[2]])).collect();
        assert!(tints.iter().all(|tint| *tint == KNUCKLE_TINTS[0]));
        let mut last = Brawl::default();
        last.knuckle_lines([10.0, 1.0, 10.0], [50.0, 1.0, 10.0], start, |n| n - 1);
        let walls = last.top_meshes(start, |_| Some([0.0; 3]));
        assert!(walls.iter().all(|wall| wall.vertices.iter().all(|vertex| vertex.tint[..3] == KNUCKLE_TINTS[2])));
        // Hesperus Lit: the half-diagonal 0.9 × 15 × 1.07ⁿ, alpha 6, 60, 59, 1, 0 at n = 1, 10, 11, 69, 70.
        assert!((disc_half(1) - 0.9 * 15.0 * 1.07).abs() < 1e-3);
        assert_eq!([1, 10, 11, 69, 70].map(disc_alpha), [6.0, 60.0, 59.0, 1.0, 0.0]);
        let mut disc = Brawl::default();
        disc.disc(Some(1), start);
        let feet = [3.0, 2.0, 4.0];
        let walls = disc.top_meshes(start, |_| Some(feet));
        assert_eq!(walls.len(), 1);
        let wall = &walls[0];
        assert!(wall.additive && wall.texture == BLAST && wall.vertices.len() == 6);
        assert_eq!(wall.vertices[0].tint[..3], [5.0 / 255.0, 5.0 / 255.0, 1.0]);
        assert!(wall.vertices.iter().all(|vertex| vertex.position[1] == feet[1] + 5.0));
        assert!((wall.vertices[0].position[0] - (feet[0] + disc_half(1))).abs() < 1e-3);
        assert!(!disc.top_meshes(start + TICK * 68, |_| Some(feet)).is_empty() && disc.top_meshes(start + TICK * 69, |_| Some(feet)).is_empty());
        assert!(disc.top_meshes(start, |_| None).is_empty(), "it follows the unit");
    }
}
