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
/// Dark Illusion's shake of its target: ticks 0 to 34.
const SHAKE_TICKS: u32 = 35;
/// Dark Illusion: 8 prims of 4 pale lines, drawn for ages 1 to 49.
const STRIP_PRIMS: usize = 8;
const STRIP_LAST: u32 = 49;
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
    pub fn rim(&self) -> [[f32; 2]; 2] {
        let high = |delta: i32| table(if self.angle + delta >= 360 { self.angle + delta - 360 } else { self.angle + delta });
        [[table(self.angle - 1), high(1)], [table(self.angle - 2), high(2)]]
    }

    /// The two wedges at age a round `feet`: the near one blue, the far one yellow, both added on.
    fn wedges(&self, feet: Vector, a: u32) -> Vec<MeshVertex> {
        let alpha = ray_alpha(a) / 255.0;
        let reach = ray_reach(a);
        let (plane_sin, plane_cos) = self.plane.to_radians().sin_cos();
        let at = |angle: f32| {
            let (sin, cos) = angle.to_radians().sin_cos();
            [feet[0] + plane_cos * reach * cos, feet[1] + 12.0 - plane_sin * reach * cos, feet[2] + reach * sin]
        };
        let middle = [feet[0], feet[1] + 12.0, feet[2]];
        let mut vertices = Vec::new();
        for (rim, rgb) in self.rim().into_iter().zip([[0.0, 0.0, 250.0 / 255.0], [250.0 / 255.0, 250.0 / 255.0, 0.0]]) {
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
}

struct Lines {
    start: Instant,
    from: Vector,
    heading: f32,
    strips: Vec<Strip>,
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
        (self.phase + self.speed * a as f32) % 200.0
    }
}

struct Spread {
    unit: Option<u32>,
    start: Instant,
}

/// A unit whose parts shake (458), swing its head (466) or glow blue (543) from `start`.
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
    shakes: Vec<Marked>,
    swings: Vec<Swing>,
    blues: Vec<Blue>,
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
    pub fn rays(&mut self, unit: Option<u32>, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        let mut rays = Vec::new();
        for i in 0..8 {
            for k in 0..4 {
                rays.push(Ray { plane: rand(360) as f32, angle: 90 * k + 22 * i });
            }
        }
        self.rays.push(Rays { unit, start: now, rays });
    }

    /// Windmill's tornado and wisps where the caster stands (`at`), and its spin, on `unit`.
    pub fn windmill(&mut self, unit: Option<u32>, at: Vector, now: Instant, rand: impl FnMut(u32) -> u32) {
        self.tornado(at, now, None, rand);
        self.spins.push(Spin { unit, start: now, done: 0 });
    }

    /// Reverberation's red tornado and wisps at a trap's cell `at`, gone with the trap `id`.
    pub fn trap_tornado(&mut self, id: u32, at: Vector, now: Instant, rand: impl FnMut(u32) -> u32) {
        self.tornado(at, now, Some(id), rand);
    }

    /// A trap went: its tornado with it.
    pub fn forget(&mut self, id: u32) {
        self.tornados.retain(|tornado| tornado.kept != Some(id));
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
    pub fn lines(&mut self, from: Vector, to: Vector, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        let mut strips = Vec::new();
        for prim in 0..STRIP_PRIMS {
            for _ in 0..4 {
                let side = (15 + rand(11)) as f32 * if prim % 2 == 0 { -1.0 } else { 1.0 };
                let phase = rand(200) as f32;
                let half = 0.3 + 0.1 * rand(4) as f32;
                let length = (20 + rand(31)) as f32;
                let colour = rand(3) as usize;
                let speed = 1.5 + 0.1 * rand(21) as f32;
                strips.push(Strip { side, phase, half, length, colour, speed });
            }
        }
        let heading = bearing(to[0] - from[0], to[2] - from[2]) as f32;
        self.lines.push(Lines { start: now, from, heading, strips });
    }

    /// Dark Illusion's and Knuckle Arrow's shake of `unit` (35 ticks).
    pub fn shake(&mut self, unit: Option<u32>, now: Instant) {
        self.shakes.push(Marked { unit, start: now });
    }

    /// Whether `unit`'s parts shake now.
    pub fn shaking(&self, unit: Option<u32>, now: Instant) -> bool {
        self.shakes.iter().filter(|shake| shake.unit == unit).any(|shake| passes(shake.start, now).checked_sub(1).is_some_and(|t| t < SHAKE_TICKS))
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
        self.spreads.push(Spread { unit, start: now });
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
        self.flares.retain(|flares| passes(flares.start, now) <= 30);
        self.hops.retain(|hops| passes(hops.start, now) <= 100);
        self.rays.retain(|rays| passes(rays.start, now) <= RAY_LAST + 1);
        self.tornados.retain(|tornado| passes(tornado.start, now) <= TORNADO_LAST + 1);
        self.lines.retain(|lines| passes(lines.start, now) <= STRIP_LAST + 1);
        self.spreads.retain(|spread| passes(spread.start, now) <= 100);
        self.spins.retain(|spin| passes(spin.start, now) <= SPIN_TICKS + 1);
        self.shakes.retain(|shake| passes(shake.start, now) <= SHAKE_TICKS);
        self.swings.retain(|swing| passes(swing.start, now) <= 50);
        self.blues.retain(|blue| passes(blue.start, now) <= 200);
    }

    /// How many degrees `unit` has turned since this was last asked (30 a tick on ticks 0 to 47).
    pub fn turned(&mut self, unit: Option<u32>, now: Instant) -> f32 {
        let mut degrees = 0.0;
        for spin in self.spins.iter_mut().filter(|spin| spin.unit == unit) {
            let due = passes(spin.start, now).min(SPIN_TICKS);
            degrees += 30.0 * due.saturating_sub(spin.done) as f32;
            spin.done = spin.done.max(due);
        }
        degrees
    }

    /// The body flags and colour an effect gives `unit` now: Rage Burst adds the body on ticks 5 to 35, Sound of Destruction's copies
    /// come with ticks 30 to 99 (the age n = t − 30 in the flags' upper half).
    pub fn glow(&self, unit: Option<u32>, now: Instant) -> Option<(u32, Option<[u8; 3]>)> {
        let added = self.rays.iter().filter(|rays| rays.unit == unit).any(|rays| passes(rays.start, now).checked_sub(1).is_some_and(|t| (5..=35).contains(&t)));
        let spread = self.spreads.iter().filter(|spread| spread.unit == unit).find_map(|spread| {
            let t = passes(spread.start, now).checked_sub(1).filter(|t| (30..100).contains(t))?;
            Some(crate::actor::GLOW_SPREAD | (t - 30) << 16)
        });
        // Marsh of Abyss' swing on ticks 14 to 49 (the age in the upper half) and Hovering's blue copies on ticks 0 to 199 (the alpha).
        let swing = self.swings.iter().filter(|swing| swing.unit == unit).find_map(|swing| {
            let t = passes(swing.start, now).checked_sub(1).filter(|t| (14..50).contains(t))?;
            Some(crate::actor::GLOW_SWING | (t - 14) << 16)
        });
        let blue = self.blues.iter().filter(|blue| blue.unit == unit).find_map(|blue| {
            let t = passes(blue.start, now).checked_sub(1).filter(|&t| t < 200)?;
            Some(crate::actor::GLOW_BLUE | (crate::actor::blue_alpha(t) as u32) << 16)
        });
        match (added, spread.or(swing).or(blue)) {
            (false, None) => None,
            (added, extra) => Some((if added { crate::actor::GLOW_ADDITIVE } else { 0 } | extra.unwrap_or(0), None)),
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
        for rays in &self.rays {
            let (Some(feet), Some(t)) = (position_of(rays.unit), passes(rays.start, now).checked_sub(1)) else { continue };
            let a = t + 1;
            if a <= RAY_LAST {
                let vertices = rays.rays.iter().flat_map(|ray| ray.wedges(feet, a)).collect();
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

    /// Dark Illusion's lines, drawn over everything with no depth test.
    pub fn top_meshes(&self, now: Instant) -> Vec<Wall> {
        let mut walls = Vec::new();
        for lines in &self.lines {
            let Some(t) = passes(lines.start, now).checked_sub(1) else { continue };
            let a = t + 1;
            if a > STRIP_LAST {
                continue;
            }
            let (sin, cos) = lines.heading.to_radians().sin_cos();
            let (side_sin, side_cos) = (lines.heading - 90.0).to_radians().sin_cos();
            let mut vertices = Vec::new();
            for strip in &lines.strips {
                let middle = [lines.from[0] + strip.side * side_cos, lines.from[1] + 6.0, lines.from[2] + strip.side * side_sin];
                let (head, tail) = (strip.at(a) + strip.length - 100.0, strip.at(a) - 100.0);
                let place = |along: f32, across: f32| [middle[0] + cos * along - sin * across, middle[1], middle[2] + sin * along + cos * across];
                let rgb = STRIP_TINTS[strip.colour];
                let tint = [rgb[0], rgb[1], rgb[2], strip_alpha(a) / 255.0];
                let [p, q, r, s] = [
                    MeshVertex { position: place(head, -strip.half), uv: [0.0, 0.0], tint },
                    MeshVertex { position: place(head, strip.half), uv: [0.0, 1.0], tint },
                    MeshVertex { position: place(tail, strip.half), uv: [1.0, 1.0], tint },
                    MeshVertex { position: place(tail, -strip.half), uv: [1.0, 0.0], tint },
                ];
                vertices.extend([p, q, r, p, r, s]);
            }
            walls.push(Wall { texture: STRIP, vertices, additive: false, forced: false });
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
            assert_eq!(glow(tick), Some((crate::actor::GLOW_SPREAD | n << 16, None)));
        }
        // The copies: the age n → 4 · n px · unit right, left, down, up, alpha 5 · (30 − n).
        let (_, copies) = crate::actor::glow_copies(crate::actor::GLOW_SPREAD | 29 << 16, None, crate::actor::PartGroup::Body, 0.0, 0.0, 400.0 / 300.0, true);
        assert_eq!(copies.len(), 4);
        assert!(
            near(copies[0].offset[0], 154.67) && near(copies[1].offset[0], -154.67) && near(copies[2].offset[1], 154.67) && near(copies[3].offset[1], -154.67)
        );
        assert!(copies.iter().all(|copy| !copy.additive && near(copy.color[3] * 255.0, 5.0)));
        let (_, first) = crate::actor::glow_copies(crate::actor::GLOW_SPREAD, None, crate::actor::PartGroup::Gear, 0.0, 0.0, 400.0 / 300.0, false);
        assert!(first.len() == 4 && first.iter().all(|copy| copy.offset == [0.0; 2]) && near(first[0].color[3] * 255.0, 150.0));
        let (_, gone) = crate::actor::glow_copies(crate::actor::GLOW_SPREAD | 30 << 16, None, crate::actor::PartGroup::Body, 0.0, 0.0, 1.0, true);
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
        let walls = brawl.top_meshes(start);
        assert_eq!(walls.len(), 1);
        assert!(!walls[0].additive && walls[0].texture == STRIP && walls[0].vertices.len() == 192);
        let first = &walls[0].vertices;
        assert!(near(first[0].position[1], 7.0));
        assert!(first.iter().all(|vertex| near(vertex.tint[3] * 255.0, 12.0)));
        // The first strip (side −15, heading 0) lies 15 to the +z side, its head 78.5 back along x, a half-width off its middle.
        assert!(near(first[0].position[0], 10.0 - 78.5) && near(first[0].position[2], 25.0 - 0.3));
        assert!(brawl.top_meshes(start + TICK * 48).len() == 1 && brawl.top_meshes(start + TICK * 49).is_empty());
    }

    #[test]
    fn the_game_reads_angles_from_its_table() {
        assert_eq!([-2, -1, 0, 360, 361, 379, 424].map(table), [-1.0, 0.0, 0.0, 360.0, 180.0, 198.0, 243.0]);
    }

    #[test]
    fn dark_illusion_shakes_marsh_of_abyss_swings_and_hovering_glows() {
        let start = Instant::now();
        let mut brawl = Brawl::default();
        brawl.shake(Some(1), start);
        brawl.swing(Some(2), start);
        brawl.blue(Some(3), start);
        // The shake is on ticks 0 to 34 only, for its unit.
        assert!(brawl.shaking(Some(1), start) && brawl.shaking(Some(1), start + TICK * 34));
        assert!(!brawl.shaking(Some(1), start + TICK * 35) && !brawl.shaking(Some(2), start));
        // The swing is set on ticks 14 to 49 with its age n = t − 14.
        let swing = |tick: u32| brawl.glow(Some(2), start + TICK * tick);
        assert!(swing(13).is_none() && swing(50).is_none());
        assert_eq!(swing(14), Some((crate::actor::GLOW_SWING, None)));
        assert_eq!(swing(49), Some((crate::actor::GLOW_SWING | 35 << 16, None)));
        // The blue copies: ticks 0 to 199, the alpha in the upper half.
        let blue = |tick: u32| brawl.glow(Some(3), start + TICK * tick);
        assert_eq!(blue(0), Some((crate::actor::GLOW_BLUE, None)));
        assert_eq!(blue(11), Some((crate::actor::GLOW_BLUE | 160 << 16, None)));
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
}
