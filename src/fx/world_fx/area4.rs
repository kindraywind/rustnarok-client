//! Fourth-job ground and area objects: Violent Quake's (1760) and All Bloom's (1798) bursts, Astral Strike's meteors
//! (1768), Floral Flare Road's roses (1804), Competentia's sparkles (1851), Petitio's hammer (1911), Pneumaticus
//! Procella's spikes (1957), Gale Storm's arrows (1973) and Nipelheim Requiem's funnel (2077).

use std::time::{Duration, Instant};

use super::{Assets, Class, Event, FLAGS_EFFECT, FLAGS_FLOOR, Face, Fx, Launch, STR_OFFSET, Seen, StrPart, Tex, WorldFx, add, triangles, yawed};
use crate::world::Vector;
use crate::world_str::Scene;

pub const VIOLENT_QUAKE: u32 = 1760;
pub const ASTRAL_STRIKE: u32 = 1768;
pub const ALL_BLOOM: u32 = 1798;
pub const FLORAL_FLARE_ROAD: u32 = 1804;
pub const COMPETENTIA: u32 = 1851;
pub const PETITIO: u32 = 1911;
pub const PNEUMATICUS: u32 = 1957;
pub const GALE_STORM: u32 = 1973;
/// Nipelheim Requiem's cast STR (a world STR row) and its funnel.
pub const NIPELHEIM_CAST: u32 = 2076;
pub const NIPELHEIM_FUNNEL: u32 = 2077;

/// CD_COMPETENTIA, CD_PETITIO, WH_GALESTORM and TR_NIPELHEIM_REQUIEM.
pub const COMPETENTIA_SKILL: u16 = 5278;
pub const PETITIO_SKILL: u16 = 5283;
pub const GALE_STORM_SKILL: u16 = 5330;
pub const NIPELHEIM_SKILL: u16 = 5363;

/// UNT_FLORAL_FLARE_ROAD, UNT_ASTRAL_STRIKE and UNT_PNEUMATICUS_PROCELLA.
pub const FLORAL_UNIT: u32 = 273;
pub const ASTRAL_UNIT: u32 = 274;
pub const PNEUMATICUS_UNIT: u32 = 276;

const FUNNEL_TEXTURE: &str = "data\\texture\\effect\\nipelheim\\nipelheim_cast\\nip_cast.tga";

/// Registers the fourth-job ground and area objects.
pub fn register(fx: &mut WorldFx) {
    for id in [VIOLENT_QUAKE, ALL_BLOOM] {
        fx.register(id, || Box::new(Burst::new(Box::new(pick))));
    }
    fx.register(ASTRAL_STRIKE, || Box::new(Astral::new(Box::new(pick))));
    fx.register(FLORAL_FLARE_ROAD, || Box::new(Floral::new(Box::new(pick))));
    fx.register(COMPETENTIA, || Box::new(Sparkles::new(Box::new(pick))));
    fx.register(PETITIO, || Box::new(Hammer::default()));
    fx.register(PNEUMATICUS, || Box::new(Procella::new(Box::new(pick))));
    fx.register(GALE_STORM, || Box::new(Gale::new(Box::new(pick))));
    fx.register(NIPELHEIM_FUNNEL, || Box::new(Funnel::default()));
}

/// Draws one random whole number below its argument.
type Rand = Box<dyn FnMut(u32) -> u32>;

/// One random whole number below `n`.
fn pick(n: u32) -> u32 {
    crate::effects::pick(0, n.saturating_sub(1))
}

fn sound(file: &str) -> Event {
    Event::Sound { file: file.to_owned(), listener: false }
}

/// An STR part of `file` at `point`: on the floor list or the effect list, with a step offset.
fn part(file: &str, min: bool, floor: bool, offset: [f32; 2], point: Vector, assets: &mut dyn Assets) -> StrPart {
    let mut part = StrPart::new(file, min, assets);
    part.flags = if floor { FLAGS_FLOOR } else { FLAGS_EFFECT };
    part.offset = offset;
    part.point = point;
    part
}

/// Steps every part and frees the ended ones.
fn step_all(parts: &mut Vec<StrPart>) {
    parts.retain_mut(StrPart::step);
}

fn draw_all<'a>(parts: &'a [StrPart], seen: &Seen, out: &mut Vec<Face<'a>>) {
    for part in parts {
        part.faces(&seen.eye, out);
    }
}

/// Events already fired (at or before the age `e`) are dropped, so a long-lived object's queue stays short.
fn drop_fired(fx: &mut Fx, e: u32) {
    fx.events.retain(|(age, _)| *age > e);
}

// Violent Quake (1760) and All Bloom (1798)

/// Violent Quake's rocks heaving out of a dust cloud, or All Bloom's bud bursting over a cracked ring, once at a cell.
struct Burst {
    rand: Rand,
    parts: Vec<StrPart>,
    /// Drawn 8 units up the screen (Violent Quake).
    lift: bool,
}

impl Burst {
    fn new(rand: Rand) -> Self {
        Self { rand, parts: Vec::new(), lift: false }
    }

    /// The STRs by the draw: Violent Quake's variant 01 – 03, or All Bloom's floor and burst 01 or 02.
    fn files(effect: u32, rand: &mut Rand) -> Vec<(String, bool)> {
        if effect == VIOLENT_QUAKE {
            let n = rand(3) + 1;
            vec![(format!("violentquake\\violentquake\\violentquake_{n:02}"), false)]
        } else {
            let n = rand(2) + 1;
            vec![(format!("allbloom\\allbloom_bomb\\allbloom_bomb{n:02}_bottom"), true), (format!("allbloom\\allbloom_bomb\\allbloom_bomb{n:02}"), false)]
        }
    }
}

impl Class for Burst {
    fn begin(&mut self, fx: &mut Fx, assets: &mut dyn Assets) {
        self.lift = fx.id == VIOLENT_QUAKE;
        self.parts = Self::files(fx.id, &mut self.rand).iter().map(|(file, floor)| part(file, fx.min, *floor, [0.0; 2], fx.position, assets)).collect();
        let wav = if self.lift { "effect\\AG_VIOLENT_QUAKE.wav" } else { "effect\\AG_ALL_BLOOM.wav" };
        fx.events.push((0, sound(wav)));
    }

    fn update(&mut self, fx: &mut Fx, scene: &Scene, _: &mut dyn Assets) {
        let lift = if self.lift { 8.0 } else { 0.0 };
        for part in &mut self.parts {
            part.point = add(fx.position, scene.up.map(|axis| axis * lift));
        }
        step_all(&mut self.parts);
    }

    fn draw<'a>(&'a self, _: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        draw_all(&self.parts, seen, out);
    }

    fn alive(&self, _: &Fx, _: &Scene) -> bool {
        !self.parts.is_empty()
    }
}

// Astral Strike (1768)

/// Astral Strike's comet slamming into its cell once, then small meteors raining round it while the unit stands.
struct Astral {
    rand: Rand,
    main: Vec<StrPart>,
    small: Vec<StrPart>,
}

/// The level as 1 … 10: −1 counts as 10.
// ponytail: level 0 counts as 1.
fn astral_n(level: i32) -> usize {
    if level < 0 { 10 } else { level.clamp(1, 10) as usize }
}

/// The ticks between small meteors and their spread factor r (±2.5·r units) by level.
const ASTRAL_EVERY: [u32; 10] = [11, 11, 11, 9, 9, 9, 7, 7, 7, 5];
const ASTRAL_SPREAD: [i32; 10] = [5, 5, 5, 7, 7, 7, 9, 9, 9, 11];

impl Astral {
    fn new(rand: Rand) -> Self {
        Self { rand, main: Vec::new(), small: Vec::new() }
    }

    /// A small meteor's point round `at` with spread factor `r`: the draws x, then z.
    fn spot(at: Vector, r: i32, rand: &mut Rand) -> Vector {
        let m = (50 * r) as u32;
        let mut off = || (rand(m + 1) as i32 - 25 * r) as f32 * 0.1;
        let x = at[0] + off();
        let z = at[2] + off();
        [x, at[1], z]
    }
}

impl Class for Astral {
    fn begin(&mut self, fx: &mut Fx, assets: &mut dyn Assets) {
        self.main = vec![
            part("astralstrike\\astralstrike_bottom\\astralstrike_bottom", fx.min, true, [0.0; 2], fx.position, assets),
            part("astralstrike\\astralstrike\\astralstrike", fx.min, false, [0.0; 2], fx.position, assets),
        ];
        fx.events.push((0, sound("effect\\AG_ASTRAL_STRIKE1.wav")));
    }

    fn update(&mut self, fx: &mut Fx, _: &Scene, assets: &mut dyn Assets) {
        let e = fx.age;
        drop_fired(fx, e);
        step_all(&mut self.main);
        if !fx.released && self.main.is_empty() {
            let n = astral_n(fx.a7);
            if e.is_multiple_of(ASTRAL_EVERY[n - 1]) {
                let at = Self::spot(fx.position, ASTRAL_SPREAD[n - 1], &mut self.rand);
                let v = (self.rand)(3) + 1;
                let floor = format!("astralstrike\\astralstrike_sub_{v}_bottom\\astralstrike_sub_{v}_bottom");
                let top = format!("astralstrike\\astralstrike_sub_{v}\\astralstrike_sub_{v}");
                self.small.push(part(&floor, fx.min, true, [0.0; 2], at, assets));
                self.small.push(part(&top, fx.min, false, [0.0; 2], at, assets));
                fx.events.push((e + 1, sound("effect\\AG_ASTRAL_STRIKE2.wav")));
            }
        }
        step_all(&mut self.small);
    }

    fn draw<'a>(&'a self, _: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        draw_all(&self.main, seen, out);
        draw_all(&self.small, seen, out);
    }

    fn alive(&self, fx: &Fx, _: &Scene) -> bool {
        !fx.released
    }
}

// Floral Flare Road (1804)

/// Floral Flare Road's scorched floor with 15 fire roses blooming one after another, 0.3 s apart, while the unit stands.
struct Floral {
    rand: Rand,
    parts: Vec<StrPart>,
}

const ROSES: u32 = 15;
const ROSE_GAP_MS: u32 = 300;

impl Floral {
    fn new(rand: Rand) -> Self {
        Self { rand, parts: Vec::new() }
    }

    /// A rose's point round `at`: the draws r1, a1, r2, a2; two independent angles.
    fn spot(at: Vector, rand: &mut Rand) -> Vector {
        let r1 = rand(20) as f32;
        let a1 = (rand(360) as f32).to_radians();
        let r2 = rand(20) as f32;
        let a2 = (rand(360) as f32).to_radians();
        [at[0] + a1.sin() * r1, at[1], at[2] + a2.cos() * r2]
    }
}

impl Class for Floral {
    fn begin(&mut self, fx: &mut Fx, assets: &mut dyn Assets) {
        self.parts.push(part("froral_flareroad\\froral_flareroad_bottom\\froral_flareroad_bottom", false, true, STR_OFFSET, fx.position, assets));
        for i in 0..ROSES {
            let at = Self::spot(fx.position, &mut self.rand);
            let mut rose = part("froral_flareroad\\froral_flareroad\\froral_flareroad", fx.min, false, STR_OFFSET, at, assets);
            rose.timeline.delay = (ROSE_GAP_MS * i) as i32;
            self.parts.push(rose);
            fx.events.push((ROSE_GAP_MS * i / 16, sound("effect\\AG_ALL_BLOOM.wav")));
        }
    }

    fn update(&mut self, _: &mut Fx, _: &Scene, _: &mut dyn Assets) {
        step_all(&mut self.parts);
    }

    fn draw<'a>(&'a self, _: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        draw_all(&self.parts, seen, out);
    }

    fn alive(&self, fx: &Fx, _: &Scene) -> bool {
        !fx.released
    }
}

// Competentia (1851)

/// Competentia's ten gold sparkles blinking at random spots within 10 cells of the caster for the cast; they never move.
struct Sparkles {
    rand: Rand,
    parts: Vec<StrPart>,
    /// Set on the ground once, at the first update.
    grounded: bool,
}

const SPARKLES: usize = 10;
const SPARKLE_RADIUS: f32 = 50.0;

impl Sparkles {
    fn new(rand: Rand) -> Self {
        Self { rand, parts: Vec::new(), grounded: false }
    }

    /// A point in the disc round `at`: the draws the angle, then the radius.
    // ponytail: a uniform point (the square root of a uniform draw); the game's two-draw method is not known.
    fn spot(at: Vector, rand: &mut Rand) -> Vector {
        let angle = (rand(360) as f32).to_radians();
        let radius = SPARKLE_RADIUS * (rand(1001) as f32 / 1000.0).sqrt();
        [at[0] + radius * angle.cos(), at[1], at[2] + radius * angle.sin()]
    }
}

/// Competentia's cast by `owner` standing at `at`: the sparkles for a cast of `cast_ms`, none under /mineffect or without a
/// cast time.
pub fn competentia(owner: Option<u32>, at: Vector, cast_ms: u32, min: bool) -> Option<Launch> {
    (cast_ms > 0 && !min).then(|| Launch { a7: Some(cast_ms as i32), ..Launch::at(COMPETENTIA, Some(owner), at) })
}

impl Class for Sparkles {
    fn begin(&mut self, fx: &mut Fx, assets: &mut dyn Assets) {
        for _ in 0..SPARKLES {
            let at = Self::spot(fx.position, &mut self.rand);
            let mut sparkle = part("competentia\\competentia_cast_particle\\competentia_cast_particle", false, false, STR_OFFSET, at, assets);
            sparkle.timeline.looped = true;
            sparkle.timeline.cut = fx.a7.max(0);
            self.parts.push(sparkle);
        }
        fx.events.push((0, sound("effect\\EF_BeginSpell.wav")));
    }

    fn update(&mut self, _: &mut Fx, scene: &Scene, _: &mut dyn Assets) {
        if !self.grounded {
            for part in &mut self.parts {
                part.point[1] = (scene.ground)(part.point[0], part.point[2]);
            }
            self.grounded = true;
        }
        step_all(&mut self.parts);
    }

    fn draw<'a>(&'a self, _: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        draw_all(&self.parts, seen, out);
    }

    fn alive(&self, _: &Fx, _: &Scene) -> bool {
        !self.parts.is_empty()
    }
}

// Petitio (1911)

/// Petitio's golden hammer swinging down onto its target over a cracking, glowing floor; it follows the target.
#[derive(Default)]
struct Hammer {
    parts: Vec<StrPart>,
}

impl Class for Hammer {
    fn begin(&mut self, fx: &mut Fx, assets: &mut dyn Assets) {
        self.parts = vec![
            part("petitio\\petitio_bottom\\petitio_bottom", fx.min, true, [0.0; 2], fx.position, assets),
            part("petitio\\petitio\\petitio", fx.min, false, [0.0; 2], fx.position, assets),
        ];
        fx.events.push((0, sound("effect\\CD_PETITIO.wav")));
        fx.events.push((6, Event::Quake));
    }

    fn update(&mut self, fx: &mut Fx, _: &Scene, _: &mut dyn Assets) {
        for part in &mut self.parts {
            part.point = add(fx.position, [0.0, 12.0, 0.0]);
        }
        step_all(&mut self.parts);
    }

    fn draw<'a>(&'a self, _: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        draw_all(&self.parts, seen, out);
    }

    fn alive(&self, _: &Fx, _: &Scene) -> bool {
        !self.parts.is_empty()
    }
}

/// A use or damage packet of Petitio on `target` (None = the player) standing at `at`: one hammer while it falls, whichever
/// packet comes first.
pub fn petitio(target: Option<u32>, at: Vector, min: bool) -> Launch {
    Launch { min, unique: true, ..Launch::at(PETITIO, Some(target), at) }
}

// Pneumaticus Procella (1957)

/// Pneumaticus Procella's breathing ring and four waves of ten gold wind spikes slamming down round it, 3 s apart.
struct Procella {
    rand: Rand,
    ring: Option<StrPart>,
    drops: Vec<StrPart>,
}

const DROPS: u32 = 40;
/// A drop's STR plays 120 frames but is drawn for the first 48.
const DROP_SHOWN: i32 = 48;
const DROP_X: [f32; 6] = [-12.0, -6.0, 0.0, 0.0, 6.0, 12.0];
const DROP_Z: [f32; 9] = [-12.0, -10.0, -6.0, -4.0, 0.0, 4.0, 6.0, 10.0, 12.0];

/// The ring's scale by level.
fn procella_scale(level: i32) -> f32 {
    match level {
        ..=3 => 0.6,
        4..=6 => 0.8,
        7..=9 => 1.2,
        _ => 1.0,
    }
}

impl Procella {
    fn new(rand: Rand) -> Self {
        Self { rand, ring: None, drops: Vec::new() }
    }

    /// Drop `i`'s offset from the unit, start delay (ms) and variant: the draws dx, dz, the delay's jitter, the variant.
    fn drop(i: u32, s: f32, rand: &mut Rand) -> (Vector, i32, u32) {
        let spread = s + 0.6;
        let dx = spread * DROP_X[rand(6) as usize];
        let dz = spread * DROP_Z[rand(9) as usize];
        let delay = rand(200) + 100 * (i + 20 * (i / 10));
        let n = rand(4).max(1);
        ([dx, 25.0, dz], delay as i32, n)
    }
}

impl Class for Procella {
    fn begin(&mut self, fx: &mut Fx, assets: &mut dyn Assets) {
        let s = procella_scale(fx.a7);
        let mut ring = part("pneumaticusprocella\\pneumaticusprocella_target\\pneumaticusprocella_target", false, true, STR_OFFSET, fx.position, assets);
        ring.timeline.looped = true;
        ring.controls.scale = s;
        self.ring = Some(ring);
        for i in 0..DROPS {
            let (offset, delay, n) = Self::drop(i, s, &mut self.rand);
            let file = format!("pneumaticusprocella\\pneumaticusprocella\\pneumaticusprocella_{n}");
            let mut drop = part(&file, false, false, STR_OFFSET, add(fx.position, offset), assets);
            drop.timeline.delay = delay;
            self.drops.push(drop);
        }
        fx.events.push((0, sound("effect\\CD_PNEUMATICUS_PROCELLA.wav")));
    }

    fn update(&mut self, _: &mut Fx, _: &Scene, _: &mut dyn Assets) {
        if let Some(ring) = &mut self.ring {
            ring.step();
        }
        step_all(&mut self.drops);
    }

    fn draw<'a>(&'a self, _: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        if let Some(ring) = &self.ring {
            ring.faces(&seen.eye, out);
        }
        // The frame after the one shown.
        let shown = self.drops.iter().filter(|drop| drop.timeline.frame <= DROP_SHOWN);
        for drop in shown {
            drop.faces(&seen.eye, out);
        }
    }

    fn alive(&self, fx: &Fx, _: &Scene) -> bool {
        !fx.released
    }
}

// Gale Storm (1973)

/// Gale Storm's arrows raining for a second onto random cells of an N × N square round the unit, with small whirlwinds.
struct Gale {
    rand: Rand,
    parts: Vec<StrPart>,
}

/// Spawns run while the age is at most this.
const GALE_TICKS: u32 = 60;

/// The square's side in cells by level.
fn gale_side(level: i32) -> u32 {
    match level {
        ..=4 => 5,
        5..=7 => 7,
        8 | 9 => 9,
        _ => 11,
    }
}

impl Gale {
    fn new(rand: Rand) -> Self {
        Self { rand, parts: Vec::new() }
    }

    /// A spawn's point over the N × N square round `at`: the draws x, then z.
    fn spot(at: Vector, n: u32, rand: &mut Rand) -> Vector {
        let half = 2.5 * n as f32;
        let x = at[0] + rand(5 * n + 1) as f32 - half;
        let z = at[2] + rand(5 * n + 1) as f32 - half;
        [x, at[1], z]
    }
}

impl Class for Gale {
    fn update(&mut self, fx: &mut Fx, _: &Scene, assets: &mut dyn Assets) {
        let e = fx.age;
        drop_fired(fx, e);
        if e <= GALE_TICKS {
            let n = gale_side(fx.a7);
            if e.is_multiple_of(100 / n) {
                let at = Self::spot(fx.position, n, &mut self.rand);
                self.parts.push(part("galestorm\\galestorm\\galestorm", fx.min, false, [0.0; 2], at, assets));
            }
            if e.is_multiple_of(30 / n) {
                let at = Self::spot(fx.position, n, &mut self.rand);
                self.parts.push(part("galestorm\\galestorm_arrow\\galestorm_arrow", fx.min, false, [0.0; 2], at, assets));
                if (self.rand)(7) == 0 {
                    fx.events.push((e + 1, sound("effect\\WH_GALESTORM.wav")));
                }
            }
        }
        step_all(&mut self.parts);
    }

    fn draw<'a>(&'a self, _: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        draw_all(&self.parts, seen, out);
    }

    fn alive(&self, fx: &Fx, scene: &Scene) -> bool {
        fx.owned(scene) && (fx.age <= GALE_TICKS || !self.parts.is_empty())
    }
}

/// A use packet of Gale Storm at `level` on `target` (None = the player) standing at `at`.
pub fn gale_storm(target: Option<u32>, at: Vector, level: i32, min: bool) -> Launch {
    Launch { a7: Some(level), min, ..Launch::at(GALE_STORM, Some(target), at) }
}

// Nipelheim Requiem's funnel (2077)

/// A dark smoky funnel of 21 quads spinning round the caster from the ground to over the head for the cast.
#[derive(Default)]
struct Funnel {
    /// The phase in degrees, +3 a tick.
    phase: f32,
    /// Each quad's corners v0 … v3 from the feet, set by the last update.
    quads: Vec<[Vector; 4]>,
    /// The real time it ends at.
    until: Option<Instant>,
}

const FUNNEL_QUADS: u32 = 21;
const FUNNEL_RADIUS: f32 = 4.5;
const FUNNEL_RIM: f32 = 25.0;
const FUNNEL_LEAN: f32 = 70.0;

impl Funnel {
    /// The 21 quads' corners at phase `p` (degrees): the rim, leaning out 70°, over a ring of radius 4.5 at the feet.
    fn corners(p: f32) -> Vec<[Vector; 4]> {
        let (sin, cos) = FUNNEL_LEAN.to_radians().sin_cos();
        let out = FUNNEL_RADIUS + FUNNEL_RIM * cos;
        let ring = |theta: f32, r: f32, y: f32| {
            let (s, c) = theta.to_radians().sin_cos();
            [r * c, y, r * s]
        };
        let step = 360.0 / FUNNEL_QUADS as f32;
        (0..FUNNEL_QUADS)
            .map(|j| {
                let t0 = j as f32 * step + p;
                let t1 = t0 + step;
                [ring(t0, out, FUNNEL_RIM * sin), ring(t1, out, FUNNEL_RIM * sin), ring(t0, FUNNEL_RADIUS, 0.0), ring(t1, FUNNEL_RADIUS, 0.0)]
            })
            .collect()
    }
}

/// Nipelheim Requiem's funnel on `owner` (None = the player) standing at `at`, for a cast of `cast_ms` real milliseconds.
pub fn funnel(owner: Option<u32>, at: Vector, cast_ms: u32) -> Launch {
    Launch { a7: Some(cast_ms as i32), ..Launch::at(NIPELHEIM_FUNNEL, Some(owner), at) }
}

impl Class for Funnel {
    fn begin(&mut self, fx: &mut Fx, assets: &mut dyn Assets) {
        assets.texture(FUNNEL_TEXTURE);
        self.until = Some(crate::control::now() + Duration::from_millis(fx.a7.max(0) as u64));
    }

    fn update(&mut self, _: &mut Fx, _: &Scene, _: &mut dyn Assets) {
        self.quads = Self::corners(self.phase);
        self.phase += 3.0;
    }

    fn draw<'a>(&'a self, fx: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        let step = 1.0 / FUNNEL_QUADS as f32;
        for (j, corners) in self.quads.iter().enumerate() {
            let (u0, u1) = (j as f32 * step, (j + 1) as f32 * step);
            let corners = corners.map(|offset| yawed(fx.position, offset, &seen.eye));
            out.push(Face {
                texture: Tex::Path(FUNNEL_TEXTURE),
                vertices: triangles(corners, [[u0, 0.0], [u1, 0.0], [u0, 1.0], [u1, 1.0]], [1.0; 4]),
                additive: false,
                flags: FLAGS_EFFECT,
            });
        }
    }

    /// Until the cast's real time is up, near or far.
    fn alive(&self, _: &Fx, _: &Scene) -> bool {
        self.until.is_some_and(|until| crate::control::now() < until)
    }
}

#[cfg(test)]
mod tests {
    use super::super::Eye;
    use super::*;
    use crate::actor::Placed;
    use crate::effects::Loaded;
    use rustnarok_client::effect::{Key, Layer, Str};
    use std::cell::RefCell;
    use std::rc::Rc;

    const UNIT: u32 = 0x700;
    const FEET: Vector = [30.0, 4.0, 50.0];

    /// The frame count of each fake STR, by its file.
    fn frames_of(file: &str) -> i32 {
        let name = file.rsplit('\\').next().unwrap_or(file);
        match name.trim_start_matches("min_") {
            name if name.starts_with("violentquake_") => 90,
            "allbloom_bomb01_bottom" | "allbloom_bomb02_bottom" => 70,
            "astralstrike" | "astralstrike_bottom" => 52,
            name if name.starts_with("astralstrike_sub_") => 30,
            "froral_flareroad" => 93,
            "froral_flareroad_bottom" => 400,
            "competentia_cast_particle" => 60,
            "petitio_bottom" => 81,
            "pneumaticusprocella_target" => 40,
            name if name.starts_with("pneumaticusprocella_") => 120,
            "galestorm_arrow" => 20,
            _ => 50,
        }
    }

    /// STRs of one layer with keys at 0 and their last frame; the files asked for kept.
    #[derive(Default)]
    struct Fake {
        strs: Vec<String>,
        textures: Vec<String>,
    }

    impl Assets for Fake {
        fn texture(&mut self, path: &str) -> Option<[f32; 2]> {
            self.textures.push(path.to_owned());
            Some([128.0; 2])
        }
        fn str(&mut self, file: &str) -> Option<Rc<Loaded>> {
            self.strs.push(file.to_owned());
            let frames = frames_of(file);
            let key = |frame| Key {
                frame,
                kind: 0,
                position: [320.0, 240.0],
                corners: [-30.0, 30.0, 30.0, -30.0, -20.0, -20.0, 20.0, 20.0],
                texture: 0.0,
                texture_motion: 0,
                delay: 0.0,
                angle: 0.0,
                color: [1.0; 4],
                source_blend: 5,
                destination_blend: 2,
            };
            let layer = || Layer { textures: vec!["a.bmp".into()], keys: vec![key(0), key(frames - 1)] };
            Some(Rc::new(Loaded { animation: Str { fps: 60, frames: (frames - 1) as u32, layers: vec![layer(), layer()] }, textures: vec![Vec::new(); 2] }))
        }
        fn sprite(&mut self, _: &str, _: usize, _: usize) -> Option<(usize, Vec<Placed>)> {
            None
        }
    }

    /// A random source that answers from `values` in turn (then 0) and logs what it was asked.
    fn scripted(values: &[u32]) -> (Rand, Rc<RefCell<Vec<u32>>>) {
        let asked = Rc::new(RefCell::new(Vec::new()));
        let log = asked.clone();
        let values = values.to_vec();
        (
            Box::new(move |n| {
                let mut log = log.borrow_mut();
                log.push(n);
                values.get(log.len() - 1).copied().unwrap_or(0)
            }),
            asked,
        )
    }

    /// Always the highest answer.
    fn top() -> Rand {
        Box::new(|n: u32| n.saturating_sub(1))
    }

    /// The ground is 2 under x = 0 and rises 0.1 a unit of x.
    fn ground(x: f32, _: f32) -> f32 {
        2.0 + 0.1 * x
    }

    /// Runs `f` on a scene with the units in `here` standing at their feet (None = the player, at the origin).
    fn scene<R>(here: &[(u32, Vector)], f: impl FnOnce(&Scene) -> R) -> R {
        let feet = |unit: Option<u32>| match unit {
            Some(id) => here.iter().find(|(each, _)| *each == id).map(|(_, at)| *at),
            None => Some([0.0; 3]),
        };
        let body = |_: Option<u32>| None;
        let eye = Eye::at(45.0, 0.0);
        f(&Scene { player: FEET, feet: &feet, body: &body, longitude: 0.0, right: eye.right, up: eye.up, ground: &ground })
    }

    /// One object run on its own as the registry runs it: it follows its owner, updates and ages.
    struct Solo<C> {
        fx: Fx,
        class: C,
    }

    impl<C: Class> Solo<C> {
        fn new(mut class: C, launch: Launch, assets: &mut Fake) -> Self {
            let mut fx = Fx::of(&launch);
            class.begin(&mut fx, assets);
            Self { fx, class }
        }

        fn tick(&mut self, here: &[(u32, Vector)], assets: &mut Fake) {
            scene(here, |scene| {
                if let Some(feet) = self.fx.owner.and_then(|owner| (scene.feet)(owner)) {
                    self.fx.position = feet;
                }
                self.class.update(&mut self.fx, scene, assets);
            });
            self.fx.updated = true;
            self.fx.age += 1;
        }

        fn alive(&self, here: &[(u32, Vector)]) -> bool {
            scene(here, |scene| self.class.alive(&self.fx, scene))
        }

        /// Ticks while alive, at most `limit` times; the updates run.
        fn run(&mut self, here: &[(u32, Vector)], assets: &mut Fake, limit: u32) -> u32 {
            let mut updates = 0;
            while updates < limit && self.alive(here) {
                self.tick(here, assets);
                updates += 1;
            }
            updates
        }

        /// The ages of its sound events, and of its shakes.
        fn sounds(&self) -> Vec<(u32, String)> {
            self.fx
                .events
                .iter()
                .filter_map(|(age, event)| match event {
                    Event::Sound { file, .. } => Some((*age, file.clone())),
                    Event::Quake | Event::Afterimage => None,
                })
                .collect()
        }
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    fn close3(a: Vector, b: Vector) -> bool {
        (0..3).all(|axis| close(a[axis], b[axis]))
    }

    const HERE: [(u32, Vector); 1] = [(UNIT, FEET)];

    #[test]
    fn violent_quake_picks_one_of_three_and_stands_8_units_up_the_screen() {
        for (draw, file) in [(0, "violentquake_01"), (2, "violentquake_03")] {
            for min in [false, true] {
                let mut assets = Fake::default();
                let (rand, asked) = scripted(&[draw]);
                let mut solo = Solo::new(Burst::new(rand), Launch { min, ..Launch::at(VIOLENT_QUAKE, None, FEET) }, &mut assets);
                let name = if min { format!("min_{file}") } else { file.to_owned() };
                assert_eq!(assets.strs, [format!("violentquake\\violentquake\\{name}")]);
                assert_eq!(*asked.borrow(), [3]);
                assert_eq!(solo.sounds(), [(0, "effect\\AG_VIOLENT_QUAKE.wav".to_owned())]);
                solo.tick(&[], &mut assets);
                let part = &solo.class.parts[0];
                let up = Eye::at(45.0, 0.0).up;
                assert!(close3(part.point, add(FEET, up.map(|axis| axis * 8.0))));
                assert_eq!((part.offset, part.flags), ([0.0; 2], FLAGS_EFFECT));
            }
        }
        // It ends with its part: 90 frames.
        let mut solo = Solo::new(Burst::new(top()), Launch::at(VIOLENT_QUAKE, None, FEET), &mut Fake::default());
        assert_eq!(solo.run(&[], &mut Fake::default(), 1000), 91);
    }

    #[test]
    fn all_bloom_picks_a_pair_and_ends_with_the_longer() {
        for (draw, n) in [(0, "01"), (1, "02")] {
            let mut assets = Fake::default();
            let (rand, _) = scripted(&[draw]);
            let mut solo = Solo::new(Burst::new(rand), Launch { min: true, ..Launch::at(ALL_BLOOM, None, FEET) }, &mut assets);
            let floor = format!("allbloom\\allbloom_bomb\\min_allbloom_bomb{n}_bottom");
            assert_eq!(assets.strs, [floor, format!("allbloom\\allbloom_bomb\\min_allbloom_bomb{n}")]);
            assert_eq!(solo.sounds(), [(0, "effect\\AG_ALL_BLOOM.wav".to_owned())]);
            solo.tick(&[], &mut assets);
            let flags: Vec<_> = solo.class.parts.iter().map(|part| (part.flags, part.offset, part.point)).collect();
            assert_eq!(flags, [(FLAGS_FLOOR, [0.0; 2], FEET), (FLAGS_EFFECT, [0.0; 2], FEET)]);
            // The floor plays 70 frames, the burst 50.
            assert_eq!(1 + solo.run(&[], &mut assets, 1000), 71);
        }
    }

    #[test]
    fn astral_strike_plays_its_comet_then_rains_meteors_by_level() {
        for (level, every, spread) in [(1, 11, 12.5), (4, 9, 17.5), (7, 7, 22.5), (10, 5, 27.5), (-1, 5, 27.5)] {
            let mut assets = Fake::default();
            let mut solo = Solo::new(Astral::new(top()), Launch { a7: Some(level), ..Launch::at(ASTRAL_STRIKE, Some(Some(UNIT)), FEET) }, &mut assets);
            assert_eq!(assets.strs, ["astralstrike\\astralstrike_bottom\\astralstrike_bottom", "astralstrike\\astralstrike\\astralstrike"]);
            assert_eq!(solo.sounds(), [(0, "effect\\AG_ASTRAL_STRIKE1.wav".to_owned())]);
            // Its 52-frame STRs are freed at e = 52; nothing small comes before.
            for e in 0..=52 {
                solo.tick(&HERE, &mut assets);
                assert!(solo.class.small.is_empty(), "{e}");
                assert_eq!(solo.class.main.is_empty(), e == 52, "{e}");
            }
            let first = (53..).find(|e: &u32| e.is_multiple_of(every)).unwrap();
            for e in 53..=first {
                solo.tick(&HERE, &mut assets);
                assert_eq!(solo.class.small.len(), if e == first { 2 } else { 0 }, "level {level} e {e}");
            }
            // The highest draws put it +2.5·r off on both axes, variant 3; its sound comes the next tick.
            let small = &solo.class.small[1];
            assert!(close3(small.point, [FEET[0] + spread, FEET[1], FEET[2] + spread]), "{level}");
            assert_eq!(
                assets.strs[2..],
                ["astralstrike\\astralstrike_sub_3_bottom\\astralstrike_sub_3_bottom", "astralstrike\\astralstrike_sub_3\\astralstrike_sub_3"]
            );
            assert_eq!(solo.sounds(), [(first + 1, "effect\\AG_ASTRAL_STRIKE2.wav".to_owned())]);
        }
        // The lowest draws: −2.5·r; the draws x, z, variant; the unit gone ends it.
        let (rand, asked) = scripted(&[]);
        let mut assets = Fake::default();
        let mut solo = Solo::new(Astral::new(rand), Launch { a7: Some(1), ..Launch::at(ASTRAL_STRIKE, Some(Some(UNIT)), FEET) }, &mut assets);
        solo.run(&HERE, &mut assets, 56);
        assert_eq!(*asked.borrow(), [251, 251, 3]);
        assert!(close3(solo.class.small[0].point, [FEET[0] - 12.5, FEET[1], FEET[2] - 12.5]));
        assert!(solo.alive(&HERE));
        solo.fx.released = true;
        assert!(!solo.alive(&HERE));
    }

    #[test]
    fn floral_flare_road_blooms_fifteen_roses_three_tenths_apart() {
        let (rand, asked) = scripted(&[]);
        let mut assets = Fake::default();
        let mut solo = Solo::new(Floral::new(rand), Launch { min: true, a7: Some(3), ..Launch::at(FLORAL_FLARE_ROAD, Some(Some(UNIT)), FEET) }, &mut assets);
        assert_eq!(*asked.borrow(), [20, 360, 20, 360].repeat(15));
        assert_eq!(assets.strs[0], "froral_flareroad\\froral_flareroad_bottom\\froral_flareroad_bottom");
        assert!(assets.strs[1..].iter().all(|file| file == "froral_flareroad\\froral_flareroad\\min_froral_flareroad") && assets.strs.len() == 16);
        let ages: Vec<u32> = solo
            .sounds()
            .into_iter()
            .map(|(age, file)| {
                assert_eq!(file, "effect\\AG_ALL_BLOOM.wav");
                age
            })
            .collect();
        assert_eq!(ages, [0, 18, 37, 56, 75, 93, 112, 131, 150, 168, 187, 206, 225, 243, 262]);
        let floor = &solo.class.parts[0];
        assert_eq!((floor.flags, floor.offset, floor.point, floor.timeline.looped), (FLAGS_FLOOR, STR_OFFSET, FEET, false));
        // Draws of 0: sin 0 · 0 and cos 0 · 0, the unit's point.
        assert!(solo.class.parts[1..].iter().all(|rose| rose.point == FEET && rose.offset == STR_OFFSET));
        // Rose i's first frame comes ⌈300·i/16⌉ ticks on (before rose 0 ends at 93).
        let mut first = [None; 15];
        for tick in 0..93 {
            solo.tick(&HERE, &mut assets);
            for (i, rose) in solo.class.parts[1..].iter().enumerate() {
                if rose.timeline.frame == 1 && first[i].is_none() {
                    first[i] = Some(tick);
                }
            }
        }
        assert_eq!(first[..6], [Some(0), Some(19), Some(38), Some(57), Some(75), None]);
        assert!(solo.alive(&HERE));
        solo.fx.released = true;
        assert!(!solo.alive(&HERE));
    }

    #[test]
    fn floral_flare_road_scatters_on_two_angles() {
        let (rand, _) = scripted(&[10, 90, 10, 0]);
        let solo = Solo::new(Floral::new(rand), Launch::at(FLORAL_FLARE_ROAD, Some(Some(UNIT)), FEET), &mut Fake::default());
        assert!(close3(solo.class.parts[1].point, [FEET[0] + 10.0, FEET[1], FEET[2] + 10.0]));
    }

    #[test]
    fn competentia_sparkles_last_the_cast_on_the_ground_where_they_fell() {
        assert_eq!(competentia(Some(UNIT), FEET, 1000, true), None);
        assert_eq!(competentia(Some(UNIT), FEET, 0, false), None);
        let launch = competentia(Some(UNIT), FEET, 1000, false).unwrap();
        assert_eq!((launch.id, launch.owner, launch.a7), (COMPETENTIA, Some(Some(UNIT)), Some(1000)));
        let mut assets = Fake::default();
        let (rand, asked) = scripted(&[0, 1000, 90, 1000, 180, 250]);
        let mut solo = Solo::new(Sparkles::new(rand), launch.clone(), &mut assets);
        assert_eq!(*asked.borrow(), [360, 1001].repeat(10));
        assert_eq!(assets.strs, vec!["competentia\\competentia_cast_particle\\competentia_cast_particle"; 10]);
        assert_eq!(solo.sounds(), [(0, "effect\\EF_BeginSpell.wav".to_owned())]);
        assert!(solo.class.parts.iter().all(|part| part.timeline.looped && part.offset == STR_OFFSET && part.flags == FLAGS_EFFECT));
        solo.tick(&HERE, &mut assets);
        // Angle 0 at radius 50, angle 90 at 50, angle 180 at 25; each on the ground under it.
        let points: Vec<Vector> = solo.class.parts.iter().map(|part| part.point).collect();
        for (point, want) in points.iter().zip([[80.0, 0.0, 50.0], [30.0, 0.0, 100.0], [5.0, 0.0, 50.0]]) {
            assert!(close3(*point, [want[0], ground(want[0], 0.0), want[2]]), "{point:?}");
        }
        assert!(points.iter().all(|point| (point[0] - FEET[0]).hypot(point[2] - FEET[2]) <= 50.0 + 1e-3));
        // The caster walks off: the sparkles stay.
        let moved = [(UNIT, [90.0, 4.0, 90.0])];
        let mut shown = 1;
        while solo.alive(&moved) {
            solo.tick(&moved, &mut assets);
            shown += 1;
        }
        assert_eq!(solo.class.parts.len(), 0);
        // A 1 s cast: the 60-frame loop is cut at frame 59.
        assert_eq!(shown, 60);
        let mut solo = Solo::new(Sparkles::new(top()), launch, &mut assets);
        solo.tick(&moved, &mut assets);
        assert!(solo.class.parts.iter().all(|part| part.point[1] == ground(part.point[0], 0.0)));
    }

    #[test]
    fn petitios_hammer_falls_12_up_its_target_and_shakes_at_tick_6() {
        let launch = petitio(Some(UNIT), FEET, true);
        // Its use and its damage packet start one hammer.
        assert!(launch.unique && launch.owner == Some(Some(UNIT)));
        let mut assets = Fake::default();
        let mut solo = Solo::new(Hammer::default(), launch, &mut assets);
        assert_eq!(assets.strs, ["petitio\\petitio_bottom\\min_petitio_bottom", "petitio\\petitio\\min_petitio"]);
        assert_eq!(solo.sounds(), [(0, "effect\\CD_PETITIO.wav".to_owned())]);
        assert!(solo.fx.events.contains(&(6, Event::Quake)));
        let moved = [(UNIT, [40.0, 6.0, 60.0])];
        solo.tick(&moved, &mut assets);
        let parts: Vec<_> = solo.class.parts.iter().map(|part| (part.flags, part.offset, part.point)).collect();
        assert_eq!(parts, [(FLAGS_FLOOR, [0.0; 2], [40.0, 18.0, 60.0]), (FLAGS_EFFECT, [0.0; 2], [40.0, 18.0, 60.0])]);
        // The 81-frame floor ends it at tick 82.
        assert_eq!(1 + solo.run(&moved, &mut assets, 1000), 82);
    }

    #[test]
    fn pneumaticus_procella_drops_four_waves_of_ten_round_its_ring() {
        let scales: Vec<f32> = (1..=10).map(procella_scale).collect();
        assert_eq!(scales, [0.6, 0.6, 0.6, 0.8, 0.8, 0.8, 1.2, 1.2, 1.2, 1.0]);
        let mut assets = Fake::default();
        let (rand, asked) = scripted(&[0, 0, 0, 0, 5, 8, 199, 3, 1, 1, 0, 2]);
        let mut solo = Solo::new(Procella::new(rand), Launch { a7: Some(1), ..Launch::at(PNEUMATICUS, Some(Some(UNIT)), FEET) }, &mut assets);
        assert_eq!(*asked.borrow(), [6, 9, 200, 4].repeat(40));
        assert_eq!(solo.sounds(), [(0, "effect\\CD_PNEUMATICUS_PROCELLA.wav".to_owned())]);
        let ring = solo.class.ring.as_ref().unwrap();
        assert_eq!((ring.flags, ring.offset, ring.point, ring.timeline.looped, ring.controls.scale), (FLAGS_FLOOR, STR_OFFSET, FEET, true, 0.6));
        // s' = 1.2: draws (0, 0) → (−14.4, −14.4), (5, 8) → (14.4, 14.4), (1, 1) → (−7.2, −12).
        let drops = &solo.class.drops;
        let offsets = drops.iter().take(3).map(|drop| [drop.point[0] - FEET[0], drop.point[1] - FEET[1], drop.point[2] - FEET[2]]);
        for (got, want) in offsets.zip([[-14.4, 25.0, -14.4], [14.4, 25.0, 14.4], [-7.2, 25.0, -12.0]]) {
            assert!(close3(got, want), "{got:?}");
        }
        // Variants: draw 0 → 1, 3 → 3, 2 → 2.
        assert_eq!(
            assets.strs[1..4],
            [
                "pneumaticusprocella\\pneumaticusprocella\\pneumaticusprocella_1",
                "pneumaticusprocella\\pneumaticusprocella\\pneumaticusprocella_3",
                "pneumaticusprocella\\pneumaticusprocella\\pneumaticusprocella_2"
            ]
        );
        // Wave 0 at R(200) + 100·i, wave 1 3 s on.
        let delays: Vec<i32> = drops.iter().map(|drop| drop.timeline.delay).collect();
        assert_eq!(delays[..3], [0, 299, 200]);
        assert_eq!(delays[10..13], [3000, 3100, 3200]);
        assert_eq!((delays[20], delays[39]), (6000, 9900));
        assert!(drops.iter().all(|drop| drop.offset == STR_OFFSET && drop.flags == FLAGS_EFFECT));
        // A drop is drawn for its first 48 frames.
        for _ in 0..60 {
            solo.tick(&HERE, &mut assets);
        }
        let mut faces = Vec::new();
        solo.class.draw(
            &solo.fx,
            &Seen { eye: Eye::at(45.0, 0.0), shows: &|_| true, hidden: &|_| false, size: &|_| 1.0, ground: &ground, dim: false },
            &mut faces,
        );
        let shown = solo.class.drops.iter().filter(|drop| drop.timeline.frame <= DROP_SHOWN && drop.timeline.frame > 0).count();
        // One drawn layer a part.
        assert_eq!(faces.len(), 1 + shown);
        assert!(solo.class.drops[0].timeline.frame > DROP_SHOWN);
        assert!(solo.alive(&HERE));
        solo.fx.released = true;
        assert!(!solo.alive(&HERE));
    }

    #[test]
    fn gale_storm_rains_arrows_and_whirlwinds_for_a_second() {
        assert_eq!([1, 5, 8, 10, 0].map(gale_side), [5, 7, 9, 11, 5]);
        let mut assets = Fake::default();
        let (rand, asked) = scripted(&[]);
        let launch = gale_storm(Some(UNIT), FEET, 1, false);
        assert_eq!((launch.owner, launch.a7), (Some(Some(UNIT)), Some(1)));
        let mut solo = Solo::new(Gale::new(rand), launch, &mut assets);
        let mut whirls = Vec::new();
        let mut arrows = Vec::new();
        for e in 0..=60 {
            let before = assets.strs.len();
            solo.tick(&HERE, &mut assets);
            for file in &assets.strs[before..] {
                if file.ends_with("galestorm_arrow") { arrows.push(e) } else { whirls.push(e) }
            }
        }
        assert_eq!(whirls, [0, 20, 40, 60]);
        assert_eq!(arrows, (0..=60).step_by(6).collect::<Vec<_>>());
        // Tick 0: whirlwind x, z, then arrow x, z, R(7); the 0 draws queue the sound a tick on.
        assert_eq!(asked.borrow()[..5], [26, 26, 26, 26, 7]);
        assert!(solo.class.parts.iter().all(|part| part.offset == [0.0; 2]));
        assert!(solo.sounds().iter().all(|(_, file)| file == "effect\\WH_GALESTORM.wav"));
        // Draws of 0: −12.5 on both axes.
        let mut solo = Solo::new(Gale::new(scripted(&[]).0), gale_storm(Some(UNIT), FEET, 1, false), &mut Fake::default());
        solo.tick(&HERE, &mut assets);
        assert!(close3(solo.class.parts[0].point, [FEET[0] - 12.5, FEET[1], FEET[2] - 12.5]));
        assert_eq!(solo.sounds(), [(1, "effect\\WH_GALESTORM.wav".to_owned())]);
        // The top draws: no sound; it ends past 60 once its parts are done, or with its target.
        let mut solo = Solo::new(Gale::new(top()), gale_storm(Some(UNIT), FEET, 10, true), &mut assets);
        solo.tick(&HERE, &mut assets);
        assert!(solo.sounds().is_empty());
        assert!(assets.strs.last().unwrap().ends_with("min_galestorm_arrow"));
        assert!(!solo.alive(&[]));
        // Level 10 (N = 11): the last whirlwind at 54 plays 50 frames.
        let last = 54 + frames_of("galestorm") as u32;
        assert_eq!(1 + solo.run(&HERE, &mut assets, 1000), last + 1);
    }

    #[test]
    fn nipelheims_funnel_spins_21_quads_for_the_cast_in_real_time() {
        let mut assets = Fake::default();
        let mut solo = Solo::new(Funnel::default(), funnel(Some(UNIT), FEET, 10_000), &mut assets);
        assert_eq!(assets.textures, [FUNNEL_TEXTURE]);
        solo.tick(&HERE, &mut assets);
        let quads = &solo.class.quads;
        assert_eq!(quads.len(), 21);
        // At p = 0, quad 0's v1 is the rim at 17.14°: radius 13.05, 23.5 up; v3 the foot at radius 4.5.
        let theta = (360.0f32 / 21.0).to_radians();
        assert!(close3(quads[0][1], [13.0505 * theta.cos(), 23.4923, 13.0505 * theta.sin()]), "{:?}", quads[0][1]);
        assert!(close3(quads[0][3], [4.5 * theta.cos(), 0.0, 4.5 * theta.sin()]));
        assert!(close3(quads[0][0], [13.0505, 23.4923, 0.0]) && close3(quads[0][2], [4.5, 0.0, 0.0]));
        // The phase gains 3° a tick.
        solo.tick(&HERE, &mut assets);
        let turned = 3.0f32.to_radians();
        assert!(close3(solo.class.quads[0][2], [4.5 * turned.cos(), 0.0, 4.5 * turned.sin()]));
        let mut faces = Vec::new();
        solo.class.draw(
            &solo.fx,
            &Seen { eye: Eye::at(45.0, 0.0), shows: &|_| true, hidden: &|_| false, size: &|_| 1.0, ground: &ground, dim: false },
            &mut faces,
        );
        assert_eq!(faces.len(), 21);
        assert!(faces.iter().all(|face| !face.additive && face.flags == FLAGS_EFFECT && face.vertices[0].tint == [1.0; 4]));
        assert!(solo.alive(&HERE));
        // A far one still ends on time; a cast of 0 ends at once.
        solo.fx.far = true;
        solo.class.until = Some(crate::control::now() - Duration::from_millis(1));
        assert!(!solo.alive(&HERE));
        let solo = Solo::new(Funnel::default(), funnel(None, FEET, 0), &mut assets);
        assert!(!solo.alive(&HERE));
    }
}
