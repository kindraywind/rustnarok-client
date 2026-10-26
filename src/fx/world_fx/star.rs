//! The Star Emperor, Soul Reaper and Rebellion kick objects: Round Trip's funnel and Full Moon Kick's ribbons (435, 1223), the
//! light spheres (1197 - 1199), the plasma ball (1200), the gravity lift (1201), the nova globes (1202, 1203), the stance
//! glyphs (1214 - 1217), the soul emblem (1222), the soul orbs (1225) and the soul sprites (1231 - 1237, 1242).
//!
//! An object's age e is 0 at its first update; a quad's own counter c counts from its first update too.

use std::any::Any;

use crate::world::Vector;
use crate::world_str::Scene;

use super::{
    Assets, Card, Class, End, Event, Eye, FLAGS_EFFECT, FLAGS_TRANSLUCENT, Face, Fx, Launch, Seen, StrPart, Tex, WorldFx, add, padded_ratio, scaled, triangles,
    view_offset, yawed,
};

/// EF_R_TRIP's funnel, launched by Round Trip's use packet.
pub const ROUND_TRIP: u32 = 435;
/// RL_R_TRIP.
pub const ROUND_TRIP_SKILL: u16 = 2565;
/// Full Moon Kick's ribbons.
pub const MOON_KICK: u32 = 1223;
/// Light of Sun, Moon and Star.
pub const LIGHTS: [u32; 3] = [1197, 1198, 1199];
/// The Book of Creating Star's plasma ball.
pub const PLASMA: u32 = 1200;
/// Gravity Control's lift.
pub const GRAVITY: u32 = 1201;
/// Gravity Control's status.
pub const GRAVITY_STATUS: u16 = 1050;
/// Nova Explosion's globe, and Star Emperor Advent's.
pub const NOVA: u32 = 1202;
pub const ADVENT: u32 = 1203;
/// The Sun, Lunar, Star and Universe stances.
pub const STANCES: [u32; 4] = [1214, 1215, 1216, 1217];
/// Soul Unity's emblem.
pub const EMBLEM: u32 = 1222;
/// The Soul Reaper's orbs, launched by the soul-energy packet.
pub const SOULS: u32 = 1225;
/// The soul sprites: (effect, sprite name, depth bias).
pub const SPIRITS: [(u32, &str, f32); 8] = [
    (1231, "book_of_dimension", 0.0),
    (1232, "book_of_creatingstar", 0.0),
    (1233, "curse_explosion", 0.0),
    (1234, "soul_reaper", 0.0),
    (1235, "soul_shadow", 3e-4),
    (1236, "soul_fairy", 3e-4),
    (1237, "soul_falcon", 3e-4),
    (1242, "soul_explosion", 3e-4),
];

const FUNNEL_TEXTURE: &str = "data\\texture\\effect\\storm2.tga";
const CLOUD_TEXTURE: &str = "data\\texture\\effect\\cloud11.tga";
const CORE_TEXTURE: &str = "data\\texture\\effect\\thunder_center.bmp";
const RAY_TEXTURE: &str = "data\\texture\\effect\\white02.bmp";
const PLASMA_TEXTURES: [&str; 5] = [
    "data\\texture\\effect\\thunder_plazma_blast_a.bmp",
    "data\\texture\\effect\\thunder_plazma_blast_b.bmp",
    "data\\texture\\effect\\thunder_ball_d.bmp",
    "data\\texture\\effect\\thunder_ball_e.bmp",
    "data\\texture\\effect\\thunder_ball_f.bmp",
];
const GRAVITY_STR: &str = "WL_TELEKINESIS_INTENSE";
const GLOBE_TEXTURE: &str = "data\\texture\\effect\\star_emperor\\boom.tga";
const RING_TEXTURE: &str = "data\\texture\\effect\\star_emperor\\ring_white.tga";
const STANCE_TEXTURES: [&str; 3] = [
    "data\\texture\\effect\\sun_stance\\sun_stance.tga",
    "data\\texture\\effect\\sun_stance\\lunar_stance.tga",
    "data\\texture\\effect\\sun_stance\\star_stance.tga",
];
const EMBLEM_TEXTURE: &str = "data\\texture\\effect\\soul_unity\\soul_unity.bmp";
const ORB_TEXTURE: &str = "data\\texture\\effect\\soul_collect\\soul_collect.bmp";

/// Draws one random whole number below its argument.
type Rand = Box<dyn FnMut(u32) -> u32>;

/// One random whole number below `n`.
fn pick(n: u32) -> u32 {
    crate::effects::pick(0, n.saturating_sub(1))
}

const fn rgb(hex: u32) -> [f32; 3] {
    [(hex >> 16 & 0xff) as f32 / 255.0, (hex >> 8 & 0xff) as f32 / 255.0, (hex & 0xff) as f32 / 255.0]
}

fn sound(file: &str) -> Event {
    Event::Sound { file: file.to_owned(), listener: false }
}

/// The sine and cosine of an angle in whole degrees.
fn sin_cos(degrees: i32) -> (f32, f32) {
    (degrees as f32).to_radians().sin_cos()
}

const UV: [[f32; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]];

/// A textured quad of the given flags; only the low byte of the alpha is drawn.
fn face(texture: &'static str, corners: [Vector; 4], uv: [[f32; 2]; 4], (color, alpha): ([f32; 3], i32), (additive, flags): (bool, u32)) -> Face<'static> {
    let tint = [color[0], color[1], color[2], f32::from((alpha & 0xff) as u8) / 255.0];
    Face { texture: Tex::Path(texture), vertices: triangles(corners, uv, tint), additive, flags }
}

/// A square of half-side `half` facing the camera, corners v0 (-1, 1), v1 (1, 1), v2 (-1, -1), v3 (1, -1).
fn billboard(point: Vector, half: f32, eye: &Eye) -> [Vector; 4] {
    [[-1.0, 1.0], [1.0, 1.0], [-1.0, -1.0], [1.0, -1.0]].map(|[x, y]| add(point, add(scaled(eye.right, x * half), scaled(eye.up, y * half))))
}

/// The texture rectangle that fills a texture of this size with a half-texel inset.
fn exact(size: [f32; 2]) -> [[f32; 2]; 4] {
    let [u, v] = size.map(|side| 0.5 / side.max(1.0));
    [[u, v], [1.0 - u, v], [u, 1.0 - v], [1.0 - u, 1.0 - v]]
}

pub fn register(fx: &mut WorldFx) {
    fx.register(ROUND_TRIP, || Box::new(Kick::new(true, Box::new(pick))));
    fx.register(MOON_KICK, || Box::new(Kick::new(false, Box::new(pick))));
    for id in LIGHTS {
        fx.register(id, || Box::new(Light::new(Box::new(pick))));
    }
    fx.register(PLASMA, || Box::new(Plasma::default()));
    fx.register(GRAVITY, || Box::new(Lift::default()));
    fx.register(NOVA, || Box::new(Nova::default()));
    fx.register(ADVENT, || Box::new(Nova::default()));
    for id in STANCES {
        fx.register(id, || Box::new(Glyphs::default()));
    }
    fx.register(EMBLEM, || Box::new(Emblem::default()));
    fx.register(SOULS, || Box::new(Souls::default()));
    for (id, ..) in SPIRITS {
        fx.register(id, || Box::new(Spirit::default()));
    }
}

/// The objects only their own launchers start: Round Trip's use packet and the soul-energy packet.
pub fn is_class(effect: u32) -> bool {
    matches!(effect, ROUND_TRIP | SOULS)
}

/// A status object's launch: the lift, the glyphs and the emblem start afresh each time their status comes on.
pub fn restarts_on_status(effect: u32) -> bool {
    effect == GRAVITY || effect == EMBLEM || STANCES.contains(&effect)
}

/// Round Trip's funnel: no owner, fixed where the caster stood.
pub fn round_trip(at: Vector, min: bool) -> Launch {
    Launch { min, ..Launch::at(ROUND_TRIP, None, at) }
}

/// The jobs whose soul energy is the orbs object: Soul Reaper, Baby Soul Reaper and Soul Ascetic.
pub fn object_souls(job: u16) -> bool {
    matches!(job, 4240 | 4242 | 4303)
}

/// The soul-energy packet for `owner` of a soul job: the orbs object, re-synced to `count`, or gone for a hidden unit.
pub fn souls(fx: &mut WorldFx, owner: Option<Option<u32>>, count: u16, seen: bool, at: Vector, min: bool, assets: &mut dyn Assets) {
    if !seen {
        fx.remove(owner, SOULS);
        return;
    }
    fx.launch(&Launch { min, ..Launch::at(SOULS, owner, at) }, assets);
    if let Some((_, orbs)) = fx.class_mut::<Souls>(owner, &[SOULS]) {
        orbs.set_count(usize::from(count));
    }
}

// Round Trip's funnel (435) and Full Moon Kick's ribbons (1223)

/// The funnel's squares: half-diagonals from the top down, and the heights.
const FUNNEL: [f32; 10] = [40.0, 36.0, 32.4, 29.2, 26.4, 24.0, 22.0, 20.4, 19.2, 18.4];
const FUNNEL_BLUE: [f32; 3] = rgb(0x3232FF);

/// One flat square of the funnel, turning.
#[derive(Debug, Clone, PartialEq)]
struct Square {
    angle: i32,
    alpha: i32,
}

/// One quad of a ribbon row, which seeds or strips the row's points.
#[derive(Debug, Clone, PartialEq)]
struct Strip {
    lean: i32,
    turn: i32,
    arc: i32,
    r: f32,
    h: f32,
    alpha: i32,
    /// The base and the top points, from the feet before the camera's yaw.
    base: Vector,
    top: Vector,
}

/// The funnel (435 only) and 168 ribbon quads: two calls of 4 rows of 21.
struct Kick {
    round: bool,
    squares: Vec<Square>,
    strips: Vec<Strip>,
    draw: Rand,
}

impl Kick {
    fn new(round: bool, mut draw: Rand) -> Self {
        let squares = if round { (0..FUNNEL.len()).map(|j| Square { angle: 45 * j as i32, alpha: 0 }).collect() } else { Vec::new() };
        let mut strips = Vec::new();
        for call in 0..2 {
            for k in 0..4 {
                for _ in 0..21 {
                    let (h, r) = match (round, call) {
                        (true, 0) => (48.0 - 5.0 * k as f32, 14.0 - 2.0 * k as f32),
                        (true, _) => (28.0 - 5.0 * k as f32, 6.0 - k as f32),
                        (false, 0) => (28.0 - 5.0 * k as f32, 10.0 - 2.0 * k as f32),
                        (false, _) => (18.0 - 5.0 * k as f32, 5.0 - k as f32),
                    };
                    let lean = 80 + draw(21) as i32;
                    strips.push(Strip { lean, turn: 90 * k, arc: 30, r, h, alpha: 0, base: [0.0; 3], top: [0.0; 3] });
                }
            }
        }
        Self { round, squares, strips, draw }
    }

    /// The funnel's alpha after the update with counter `c`: up to c = 40, held to 60, then down.
    fn funnel_alpha(alpha: i32, c: u32) -> i32 {
        match c {
            0..=40 => alpha + 1,
            41..=60 => alpha,
            _ => (alpha - 1).max(0),
        }
    }

    /// A ribbon's alpha after the update at age `e`.
    fn ribbon_alpha(round: bool, alpha: i32, e: u32) -> i32 {
        match (round, e) {
            (true, 0..=11) => alpha + 1,
            (true, 12..=50) => alpha,
            (false, 0..=30) => alpha + 3,
            (true, _) => (alpha - 1).max(0),
            (false, _) => (alpha - 3).max(0),
        }
    }
}

impl Class for Kick {
    fn begin(&mut self, fx: &mut Fx, assets: &mut dyn Assets) {
        fx.length = if self.round { 100 } else { 60 };
        assets.texture(CLOUD_TEXTURE);
        if self.round {
            assets.texture(FUNNEL_TEXTURE);
            fx.events.push((5, sound("effect\\T_회오리차기.wav")));
        }
    }

    fn update(&mut self, fx: &mut Fx, scene: &Scene, _: &mut dyn Assets) {
        let e = fx.age;
        for square in &mut self.squares {
            square.angle = (square.angle + 3 + (self.draw)(3) as i32) % 360;
            square.alpha = Self::funnel_alpha(square.alpha, e);
        }
        for (index, strip) in self.strips.iter_mut().enumerate() {
            strip.turn = (strip.turn + 5) % 360;
            strip.arc = (strip.arc + 3).min(180);
            strip.alpha = Self::ribbon_alpha(self.round, strip.alpha, e);
            if e > 12 {
                strip.r += 0.1;
            }
            let phi = (index % 21) as i32 * (strip.arc / 20) + strip.turn;
            let (sin, cos) = sin_cos(phi % 360);
            let (lean_sin, lean_cos) = sin_cos(strip.lean);
            strip.base = [strip.r * cos, strip.h, strip.r * sin];
            strip.top = [strip.base[0] + cos * lean_cos, strip.base[1] + lean_sin, strip.base[2] + sin * lean_cos];
        }
        // The owner spins four times in 48 ticks.
        if fx.age <= 47 && fx.owner.is_some_and(|owner| (scene.feet)(owner).is_some()) {
            fx.turn += 30.0;
        }
    }

    fn draw<'a>(&'a self, fx: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        for (j, square) in self.squares.iter().enumerate() {
            let height = 51.0 - 5.0 * j as f32;
            let corner = |turn: i32| {
                let (sin, cos) = sin_cos(square.angle + turn);
                yawed(fx.position, [FUNNEL[j] * cos, height, FUNNEL[j] * sin], &seen.eye)
            };
            let corners = [corner(270), corner(180), corner(0), corner(90)];
            out.push(face(FUNNEL_TEXTURE, corners, UV, (FUNNEL_BLUE, square.alpha), (true, FLAGS_EFFECT)));
        }
        for row in self.strips.chunks(21) {
            for j in 1..21 {
                let (before, strip) = (&row[j - 1], &row[j]);
                let point = |offset: Vector| yawed(fx.position, offset, &seen.eye);
                let corners = [point(before.top), point(strip.top), point(before.base), point(strip.base)];
                let (u0, u1) = ((j - 1) as f32 / 21.0, j as f32 / 21.0);
                out.push(face(CLOUD_TEXTURE, corners, [[u0, 0.0], [u1, 0.0], [u0, 1.0], [u1, 1.0]], ([1.0; 3], strip.alpha), (true, FLAGS_EFFECT)));
            }
        }
    }
}

// The light spheres (1197 - 1199)

/// A light sphere's core colour and its two ray colours.
fn light_colours(id: u32) -> ([f32; 3], [[f32; 3]; 2]) {
    match id {
        1197 => (rgb(0xFF7C80), [rgb(0xFF7C80), rgb(0xFF4A4E)]),
        1198 => (rgb(0xFFFF37), [rgb(0xFFFF9B), rgb(0xFFFF37)]),
        _ => (rgb(0x3BCCFF), [rgb(0x3BCCFF), rgb(0x6D9AFF)]),
    }
}

/// One of 640 rays: a thin triangle from its base on the sphere's centre to an apex that wanders.
#[derive(Debug, Clone, PartialEq)]
struct Ray {
    set: usize,
    /// The apex's two angles and the one that sets its distance, whole degrees.
    a: i32,
    b: i32,
    theta: i32,
    colour: usize,
    scale: f32,
    alpha: i32,
}

/// A glow orb of 4 core quads at chest height inside a starburst of 640 rays.
struct Light {
    colours: ([f32; 3], [[f32; 3]; 2]),
    core_alpha: i32,
    core_phi: i32,
    core_scale: f32,
    rays: Vec<Ray>,
    draw: Rand,
}

impl Light {
    fn new(draw: Rand) -> Self {
        Self { colours: light_colours(0), core_alpha: 1, core_phi: 0, core_scale: 3.5, rays: Vec::new(), draw }
    }
}

impl Class for Light {
    /// Makes the rays: per ray in order the draws a, b, theta and the colour.
    fn begin(&mut self, fx: &mut Fx, assets: &mut dyn Assets) {
        self.colours = light_colours(fx.id);
        for set in 0..2 {
            for _ in 0..320 {
                let (a, b, theta) = ((self.draw)(360) as i32, (self.draw)(360) as i32, (self.draw)(360) as i32);
                let colour = (self.draw)(2) as usize;
                self.rays.push(Ray { set, a, b, theta, colour, scale: 1.0, alpha: 40 });
            }
        }
        assets.texture(CORE_TEXTURE);
        assets.texture(RAY_TEXTURE);
    }

    fn update(&mut self, fx: &mut Fx, _: &Scene, _: &mut dyn Assets) {
        let c = fx.age;
        self.core_alpha = if c < 300 { (self.core_alpha + 1).min(255) } else { (self.core_alpha - 1).max(0) };
        self.core_phi = (self.core_phi + 2) % 360;
        self.core_scale = 3.5 + 3.5 * 0.05 * sin_cos(self.core_phi).0;
        for ray in &mut self.rays {
            if ray.scale < 10.0 {
                ray.scale *= 1.02;
            }
            if c > 500 {
                ray.alpha = (ray.alpha - 1).max(0);
            }
            ray.theta = (ray.theta + 1 + (self.draw)(2) as i32) % 360;
        }
    }

    fn draw<'a>(&'a self, fx: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        let centre = add(fx.position, [0.0, 10.0, 0.0]);
        for _ in 0..4 {
            out.push(face(CORE_TEXTURE, billboard(centre, self.core_scale, &seen.eye), UV, (self.colours.0, self.core_alpha), (true, FLAGS_EFFECT)));
        }
        for ray in self.rays.iter().filter(|ray| ray.alpha & 0xff != 0) {
            let r = ray.scale * (1.0 + sin_cos(ray.theta).0);
            let ((sin_a, cos_a), (sin_b, cos_b)) = (sin_cos(ray.a), sin_cos(ray.b));
            let apex = yawed(fx.position, [r * cos_a * cos_b, 10.0 - r * sin_a, r * cos_a * sin_b], &seen.eye);
            let ends = if ray.set == 0 { [[-0.5, 10.0, 0.0], [0.5, 10.0, 0.0]] } else { [[0.0, 10.0, -0.5], [0.0, 10.0, 0.5]] };
            let [from, to] = ends.map(|end| yawed(fx.position, end, &seen.eye));
            out.push(face(RAY_TEXTURE, [apex, apex, from, to], UV, (self.colours.1[ray.colour], ray.alpha), (true, FLAGS_EFFECT)));
        }
    }

    /// While any core or ray quad can still be seen.
    fn alive(&self, _: &Fx, _: &Scene) -> bool {
        self.core_alpha > 0 || self.rays.iter().any(|ray| ray.alpha > 0)
    }
}

// The plasma ball (1200)

/// A 15 × 15 blue lightning ball 5 up, over everything, flickering through five pictures.
struct Plasma {
    c: i32,
    frame: usize,
}

impl Default for Plasma {
    fn default() -> Self {
        Self { c: -10, frame: 0 }
    }
}

impl Class for Plasma {
    fn begin(&mut self, _: &mut Fx, assets: &mut dyn Assets) {
        for path in PLASMA_TEXTURES {
            assets.texture(path);
        }
    }

    fn update(&mut self, _: &mut Fx, _: &Scene, _: &mut dyn Assets) {
        self.c += 1;
        if self.c >= 0 {
            self.frame = (self.frame + 1) % PLASMA_TEXTURES.len();
        }
    }

    fn draw<'a>(&'a self, fx: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        if self.c >= 0 {
            let corners = billboard(add(fx.position, [0.0, 5.0, 0.0]), 7.5, &seen.eye);
            out.push(face(PLASMA_TEXTURES[self.frame], corners, UV, ([1.0; 3], 255), (true, 0x203)));
        }
    }

    fn alive(&self, fx: &Fx, scene: &Scene) -> bool {
        fx.owned(scene)
    }
}

// The gravity lift (1201)

/// A looping swirl under the unit, which raises the unit 20 units over 40 ticks.
#[derive(Default)]
struct Lift {
    d: f32,
    part: Option<StrPart>,
}

impl Class for Lift {
    fn begin(&mut self, fx: &mut Fx, assets: &mut dyn Assets) {
        fx.single = true;
        let mut part = StrPart::new(GRAVITY_STR, fx.min, assets);
        part.offset = [0.0; 2];
        part.timeline.looped = true;
        self.part = Some(part);
    }

    fn update(&mut self, fx: &mut Fx, _: &Scene, _: &mut dyn Assets) {
        self.d = (self.d - 0.5).max(-20.0);
        fx.lift = (-self.d).trunc();
        if let Some(part) = &mut self.part {
            part.point = fx.position;
            part.step();
        }
    }

    fn draw<'a>(&'a self, _: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        if let Some(part) = &self.part {
            part.faces(&seen.eye, out);
        }
    }

    fn alive(&self, fx: &Fx, scene: &Scene) -> bool {
        fx.owned(scene)
    }

    fn any(&mut self) -> Option<&mut dyn Any> {
        Some(self)
    }
}

// The nova globes (1202, 1203)

/// A pale globe of 72 quads and a ring of 10 on the ground, swelling out of the target or collapsing into the caster.
#[derive(Default)]
struct Nova {
    advent: bool,
    sphere: f32,
    turn: i32,
    outer: f32,
    inner: f32,
    alpha: i32,
}

impl Nova {
    /// The numbers at counter `c` of an effect `length` ticks long: sphere radius, turn, ring radii and alpha.
    fn at(advent: bool, c: u32, length: u32) -> (f32, i32, [f32; 2], i32) {
        let n = if advent { (c as i32 - 30).abs() } else { c as i32 + 1 };
        let tri = (n * (n + 1) / 2) as f32;
        let length = length as f32;
        let sphere = 1.15 * n as f32 - (1.15 / length) * 0.5 * tri;
        let outer = 1.75 * n as f32 - (1.75 / length) * 0.5 * tri;
        let inner = if outer > 12.0 { outer - 12.0 } else { 0.0 };
        let alpha = match (advent, c < 15, n >= 15) {
            (false, true, _) => n * 17,
            (false, false, _) => (30 - n) * 17,
            (true, _, true) => (60 - n) * 17,
            (true, _, false) => n * 17,
        };
        (sphere, 3 * n, [outer, inner], alpha)
    }
}

impl Class for Nova {
    fn begin(&mut self, fx: &mut Fx, assets: &mut dyn Assets) {
        assets.texture(GLOBE_TEXTURE);
        assets.texture(RING_TEXTURE);
        self.advent = fx.id == ADVENT;
        fx.length = if self.advent { 60 } else { 30 };
        if self.advent {
            fx.events.push((1, sound("effect\\wizard_fire_pillar_a.wav")));
            fx.events.push((31, sound("effect\\wizard_fire_pillar_b.wav")));
        }
    }

    fn update(&mut self, fx: &mut Fx, _: &Scene, _: &mut dyn Assets) {
        let (sphere, turn, [outer, inner], alpha) = Self::at(self.advent, fx.age, fx.length);
        (self.sphere, self.turn, self.outer, self.inner, self.alpha) = (sphere, turn, outer, inner, alpha);
    }

    fn draw<'a>(&'a self, fx: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        // The globe: 6 bands of 12 segments on a unit sphere, y up, band 0 the top.
        let point = |latitude: i32, longitude: i32| {
            let ((sin_p, cos_p), (sin_s, cos_s)) = (sin_cos(latitude), sin_cos(longitude + self.turn));
            yawed(fx.position, [-cos_p * sin_s * self.sphere, -sin_p * self.sphere, -cos_p * cos_s * self.sphere], &seen.eye)
        };
        for i in 0..6 {
            for j in 0..12 {
                let (p0, p1) = (30 * i - 90, 30 * (i + 1) - 90);
                let (s0, s1) = (-30 * j, -30 * (j + 1));
                let corners = [point(p0, s0), point(p0, s1), point(p1, s0), point(p1, s1)];
                let (u0, u1, v0, v1) = (j as f32 / 12.0, (j + 1) as f32 / 12.0, i as f32 / 6.0, (i + 1) as f32 / 6.0);
                out.push(face(GLOBE_TEXTURE, corners, [[u0, v0], [u1, v0], [u0, v1], [u1, v1]], ([1.0; 3], self.alpha), (false, FLAGS_TRANSLUCENT)));
            }
        }
        // The ring: a flat annulus on the ground at the feet, over everything.
        let ring = |radius: f32, segment: i32| {
            let (sin, cos) = sin_cos(36 * segment);
            add(fx.position, [radius * cos, 0.0, radius * sin])
        };
        for j in 0..10 {
            let corners = [ring(self.outer, j), ring(self.outer, j + 1), ring(self.inner, j), ring(self.inner, j + 1)];
            out.push(face(RING_TEXTURE, corners, [[0.0, 0.0], [0.25, 0.0], [0.0, 1.0], [0.25, 1.0]], ([1.0; 3], self.alpha), (false, 0x201)));
        }
    }
}

// The stance glyphs (1214 - 1217)

/// Three small glyphs circling the unit at chest height, a turn every 5 s.
#[derive(Default)]
struct Glyphs {
    textures: [&'static str; 3],
    /// How much of its surface each picture fills (a padded texture).
    fill: [[f32; 2]; 3],
    phi: [f32; 3],
}

impl Glyphs {
    /// The three pictures of a stance object.
    fn textures(id: u32) -> [&'static str; 3] {
        match id {
            1214 => [STANCE_TEXTURES[0]; 3],
            1215 => [STANCE_TEXTURES[1]; 3],
            1216 => [STANCE_TEXTURES[2]; 3],
            _ => STANCE_TEXTURES,
        }
    }

    /// Glyph `i`'s centre over the feet, in world axes.
    fn centre(&self, i: usize, feet: Vector) -> Vector {
        add(feet, [8.0 * self.phi[i].sin(), 10.0, 8.0 * self.phi[i].cos()])
    }
}

impl Class for Glyphs {
    fn begin(&mut self, fx: &mut Fx, assets: &mut dyn Assets) {
        self.textures = Self::textures(fx.id);
        self.phi = [0.0, 120f32.to_radians(), 240f32.to_radians()];
        for (fill, path) in self.fill.iter_mut().zip(self.textures) {
            *fill = assets.texture(path).map_or([1.0; 2], padded_ratio);
        }
    }

    fn update(&mut self, _: &mut Fx, _: &Scene, _: &mut dyn Assets) {
        self.phi = self.phi.map(|phi| phi + 0.02);
    }

    /// A 6 × 6 card whose picture fills its top left part (the rest of the game's surface is clear).
    fn draw<'a>(&'a self, fx: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        for i in 0..3 {
            let [fill_x, fill_y] = self.fill[i];
            let centre = self.centre(i, fx.position);
            let corner = |x: f32, y: f32| add(centre, add(scaled(seen.eye.right, x), scaled(seen.eye.up, y)));
            let (left, top) = (-3.0, 3.0);
            let corners =
                [corner(left, top), corner(left + 6.0 * fill_x, top), corner(left, top - 6.0 * fill_y), corner(left + 6.0 * fill_x, top - 6.0 * fill_y)];
            out.push(face(self.textures[i], corners, UV, ([1.0; 3], 255), (true, FLAGS_EFFECT)));
        }
    }

    fn alive(&self, fx: &Fx, scene: &Scene) -> bool {
        fx.owned(scene)
    }
}

// Soul Unity's emblem (1222)

/// A small solid emblem over the head, 15 units up the camera's up axis (20 when the unit rides).
#[derive(Default)]
struct Emblem {
    uv: [[f32; 2]; 4],
    rides: bool,
}

impl Class for Emblem {
    fn begin(&mut self, _: &mut Fx, assets: &mut dyn Assets) {
        self.uv = exact(assets.texture(EMBLEM_TEXTURE).unwrap_or([64.0; 2]));
    }

    fn update(&mut self, fx: &mut Fx, scene: &Scene, _: &mut dyn Assets) {
        self.rides = fx.owner.and_then(|owner| (scene.body)(owner)).is_some_and(|body| body.rides);
    }

    /// Upright, in the opaque list, pulled 0.035 toward the camera so the head does not hide it.
    fn draw<'a>(&'a self, fx: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        let centre = view_offset(fx.position, 0.0, if self.rides { 20.0 } else { 15.0 }, &seen.eye);
        let right = seen.eye.ground_right();
        let corners = [[-1.0, 1.0], [1.0, 1.0], [-1.0, -1.0], [1.0, -1.0]]
            .map(|[x, y]| seen.eye.nearer(add(centre, add(scaled(right, x * 2.5), [0.0, y * 2.5, 0.0])), 0.035));
        out.push(face(EMBLEM_TEXTURE, corners, self.uv, ([1.0; 3], 255), (false, 0)));
    }

    fn alive(&self, fx: &Fx, scene: &Scene) -> bool {
        fx.owned(scene)
    }
}

// The soul orbs (1225)

const GREEN: [f32; 3] = rgb(0x00FF00);
const RED: [f32; 3] = rgb(0xFF0000);

/// An orb's outer quad: its scale (half-side), colour and highest alpha, by its number.
fn orb_look(i: usize) -> (f32, [f32; 3], i32) {
    match i / 5 {
        0 => (2.0, GREEN, 150),
        1 => (3.5, GREEN, 150),
        2 => (3.5, RED, 255),
        _ => (5.5, RED, 255),
    }
}

/// The soul orbs: a pair of quads (outer, core) a soul, the newest five showing, circling at chest height.
#[derive(Default)]
struct Souls {
    /// Each orb's two alphas.
    alphas: Vec<[i32; 2]>,
    /// Each orb's angle round the unit, degrees.
    angles: Vec<f32>,
    phase: f32,
    uv: [[f32; 2]; 4],
}

impl Souls {
    /// Adds pairs while there are fewer, removes the newest pairs while there are more.
    fn set_count(&mut self, count: usize) {
        self.alphas.resize(count, [0; 2]);
        self.angles.resize(count, 0.0);
    }

    fn count(&self) -> usize {
        self.alphas.len()
    }
}

impl Class for Souls {
    fn begin(&mut self, fx: &mut Fx, assets: &mut dyn Assets) {
        fx.single = true;
        self.uv = exact(assets.texture(ORB_TEXTURE).unwrap_or([100.0, 107.0]));
    }

    fn update(&mut self, _: &mut Fx, _: &Scene, _: &mut dyn Assets) {
        let n = self.count();
        for i in 0..n {
            let [outer, core] = &mut self.alphas[i];
            if i < n.saturating_sub(5) {
                (*outer, *core) = (0, 0);
                continue;
            }
            let (_, _, most) = orb_look(i);
            *outer = (*outer + 2).min(most);
            *core = (*core + 2).min(255);
            self.angles[i] = 360.0 / n.min(5) as f32 * (i - 5 * (i / 5)) as f32 + self.phase;
        }
        self.phase += 0.5;
        if self.phase > 360.0 {
            self.phase -= 360.0;
        }
    }

    fn draw<'a>(&'a self, fx: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        for (i, (alphas, angle)) in self.alphas.iter().zip(&self.angles).enumerate() {
            let (sin, cos) = angle.to_radians().sin_cos();
            let centre = add(fx.position, [10.0 * sin, 10.0, 10.0 * cos]);
            let (scale, colour, _) = orb_look(i);
            for (layer, (half, color, alpha)) in [(scale, colour, alphas[0]), (scale * 0.5, [1.0; 3], alphas[1])].into_iter().enumerate() {
                if alpha & 0xff == 0 {
                    continue;
                }
                let bias = 1e-5 * (layer + 1) as f32;
                let corners = billboard(centre, half, &seen.eye).map(|corner| seen.eye.nearer(corner, bias));
                out.push(face(ORB_TEXTURE, corners, self.uv, (color, alpha), (true, FLAGS_EFFECT)));
            }
        }
    }

    fn alive(&self, fx: &Fx, scene: &Scene) -> bool {
        fx.owned(scene)
    }

    fn any(&mut self) -> Option<&mut dyn Any> {
        Some(self)
    }
}

// The soul sprites (1231 - 1237, 1242)

/// A sprite card at the owner, 4 ticks a frame, ending with its animation.
#[derive(Default)]
struct Spirit {
    card: Option<Card>,
    /// The tick the card last stepped at: a faster frame rate than 62.5 a second steps it once a tick, not once a frame.
    stepped: Option<u32>,
}

impl Class for Spirit {
    fn begin(&mut self, fx: &mut Fx, _: &mut dyn Assets) {
        let Some(&(_, name, bias)) = SPIRITS.iter().find(|(id, ..)| *id == fx.id) else { return };
        let mut card = Card::new(&format!("data\\sprite\\이팩트\\{name}\\{name}"));
        card.delay = 4.0;
        card.end = End::Stop;
        card.flags = 0x101;
        card.bias = bias;
        self.card = Some(card);
    }

    fn update(&mut self, _: &mut Fx, _: &Scene, _: &mut dyn Assets) {
        if let Some(card) = &mut self.card {
            card.e8 += 1;
        }
    }

    fn frame(&mut self, _: &Fx, assets: &mut dyn Assets) {
        if let Some(card) = self.card.as_mut().filter(|card| self.stepped != Some(card.e8)) {
            self.stepped = Some(card.e8);
            card.step(assets);
        }
    }

    fn draw<'a>(&'a self, fx: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        if let Some(card) = &self.card {
            card.faces(fx.position, 1.0, false, &seen.eye, out);
        }
    }

    fn alive(&self, _: &Fx, _: &Scene) -> bool {
        self.card.as_ref().is_some_and(|card| !card.ended)
    }

    fn any(&mut self) -> Option<&mut dyn Any> {
        Some(self)
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Eye, List};
    use super::*;
    use crate::actor::Placed;
    use crate::effects::Loaded;
    use crate::world_str::{Body, Fired};
    use rustnarok_client::effect::{Key, Layer, Str};
    use std::cell::RefCell;
    use std::rc::Rc;

    const UNIT: u32 = 0x700;
    const FEET: Vector = [30.0, 4.0, 50.0];

    /// Textures of 64 px (the stances' 50), STRs of 120 frames, sprites of the soul rows' frame counts.
    #[derive(Default)]
    struct Fake;

    fn frames_of(path: &str) -> usize {
        SPIRITS
            .iter()
            .zip([36, 20, 20, 18, 18, 18, 16, 32])
            .find(|((_, name, _), _)| path.ends_with(&format!("{name}\\{name}")))
            .map_or(1, |(_, frames)| frames)
    }

    impl Assets for Fake {
        fn texture(&mut self, path: &str) -> Option<[f32; 2]> {
            Some(if path.contains("stance") { [50.0; 2] } else { [64.0; 2] })
        }
        fn str(&mut self, _: &str) -> Option<Rc<Loaded>> {
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
            let layer = || Layer { textures: vec!["a.bmp".into()], keys: vec![key(0), key(119)] };
            Some(Rc::new(Loaded { animation: Str { fps: 60, frames: 119, layers: vec![layer(), layer()] }, textures: vec![Vec::new(); 2] }))
        }
        fn sprite(&mut self, path: &str, _: usize, frame: usize) -> Option<(usize, Vec<Placed>)> {
            Some((frames_of(path), vec![((0, frame as u32, 0, [255; 4], 0), [-16.0, -16.0], [32.0, 32.0], 0.0)]))
        }
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.06
    }

    fn close3(a: Vector, b: Vector) -> bool {
        (0..3).all(|axis| (a[axis] - b[axis]).abs() < 1e-3)
    }

    fn sub(a: Vector, b: Vector) -> Vector {
        std::array::from_fn(|axis| a[axis] - b[axis])
    }

    fn dot(a: Vector, b: Vector) -> f32 {
        a.iter().zip(b).map(|(a, b)| a * b).sum()
    }

    /// A random source that answers from `values` in turn (then 0) and logs what it was asked.
    fn scripted(values: Vec<u32>) -> (Rand, Rc<RefCell<Vec<u32>>>) {
        let asked = Rc::new(RefCell::new(Vec::new()));
        let log = asked.clone();
        (
            Box::new(move |n| {
                let mut log = log.borrow_mut();
                log.push(n);
                values.get(log.len() - 1).copied().unwrap_or(0)
            }),
            asked,
        )
    }

    fn seen<'a>(shows: &'a dyn Fn(u32) -> bool, hidden: &'a dyn Fn(Option<u32>) -> bool) -> Seen<'a> {
        Seen { eye: Eye::at(45.0, 0.0), shows, hidden, size: &|_| 1.0, ground: &|_, _| 0.0, dim: false }
    }

    /// Ticks `count` times with the unit at `at` (None: gone) and the player at `player`.
    fn ticks_from(fx: &mut WorldFx, count: usize, at: Option<Vector>, player: Vector, rides: bool) -> Vec<Fired> {
        let feet = |unit: Option<u32>| match unit {
            Some(UNIT) => at,
            Some(_) => None,
            None => Some(player),
        };
        let body = move |_: Option<u32>| Some(Body { height: 27.2, scale: 1.0, rides, doram: false, rot: 0.0 });
        let eye = Eye::at(45.0, 0.0);
        let scene = Scene { player, feet: &feet, body: &body, longitude: 0.0, right: eye.right, up: eye.up, ground: &|_, _| 0.0 };
        (0..count).flat_map(|_| fx.tick(&scene, &mut Fake)).collect()
    }

    fn ticks(fx: &mut WorldFx, count: usize, at: Option<Vector>) -> Vec<Fired> {
        ticks_from(fx, count, at, FEET, false)
    }

    fn launch(id: u32, owner: Option<Option<u32>>) -> WorldFx {
        let mut fx = WorldFx::default();
        fx.launch(&Launch::at(id, owner, FEET), &mut Fake).unwrap();
        fx
    }

    fn drawn(fx: &WorldFx) -> Vec<(Vec<Vector>, bool, u32, f32)> {
        let (shows, hidden) = (|_: u32| true, |_: Option<u32>| false);
        fx.faces(&seen(&shows, &hidden))
            .into_iter()
            .map(|face| (face.vertices.iter().map(|v| v.position).collect(), face.additive, face.flags, face.vertices[0].tint[3]))
            .collect()
    }

    fn texture_names(fx: &WorldFx) -> Vec<String> {
        let (shows, hidden) = (|_: u32| true, |_: Option<u32>| false);
        fx.faces(&seen(&shows, &hidden)).into_iter().filter_map(|face| if let Tex::Path(path) = face.texture { Some(path.to_owned()) } else { None }).collect()
    }

    /// An object run on its own: it follows nobody, updates and ages as the registry does.
    struct Solo<C> {
        fx: Fx,
        class: C,
    }

    impl<C: Class> Solo<C> {
        fn new(id: u32, owner: Option<Option<u32>>, mut class: C) -> Self {
            let mut fx = Fx::of(&Launch::at(id, owner, FEET));
            class.begin(&mut fx, &mut Fake);
            Self { fx, class }
        }

        fn tick(&mut self) {
            let feet = |_: Option<u32>| Some(FEET);
            let body = |_: Option<u32>| None;
            let eye = Eye::at(45.0, 0.0);
            let scene = Scene { player: FEET, feet: &feet, body: &body, longitude: 0.0, right: eye.right, up: eye.up, ground: &|_, _| 0.0 };
            self.class.update(&mut self.fx, &scene, &mut Fake);
            self.fx.age += 1;
        }

        fn faces(&self) -> Vec<Face<'_>> {
            let (shows, hidden) = (|_: u32| true, |_: Option<u32>| false);
            let mut out = Vec::new();
            self.class.draw(&self.fx, &seen(&shows, &hidden), &mut out);
            out
        }
    }

    fn horizontal(a: Vector, b: Vector) -> f32 {
        (a[0] - b[0]).hypot(a[2] - b[2])
    }

    // Launches

    #[test]
    fn round_trip_stands_where_the_caster_stood_and_turns_nobody() {
        let mut fx = WorldFx::default();
        let launch = round_trip(FEET, false);
        assert_eq!((launch.id, launch.owner, launch.point), (435, None, FEET));
        fx.launch(&launch, &mut Fake).unwrap();
        // The caster walks off: the funnel stays, nothing spins.
        ticks(&mut fx, 20, Some([90.0, 4.0, 90.0]));
        assert_eq!(fx.iter().next().unwrap().fx.position, FEET);
        assert_eq!(fx.turned(Some(UNIT)) + fx.turned(None), 0.0);
        // Sounds once, at e = 5; length 100.
        let mut fx = WorldFx::default();
        fx.launch(&launch, &mut Fake).unwrap();
        let fired = ticks(&mut fx, 100, None);
        assert_eq!(fired, [Fired::Sound("effect\\T_회오리차기.wav".to_owned(), FEET)]);
        assert!(!fx.is_empty());
        ticks(&mut fx, 1, None);
        assert!(fx.is_empty());
    }

    #[test]
    fn full_moon_kick_follows_the_caster_who_spins_four_times_in_48_ticks() {
        let mut fx = launch(1223, Some(Some(UNIT)));
        let moved = [40.0, 4.0, 60.0];
        let fired = ticks(&mut fx, 1, Some(moved));
        assert_eq!(fx.iter().next().unwrap().fx.position, moved);
        assert!(fired.is_empty());
        ticks(&mut fx, 47, Some(moved));
        assert_eq!(fx.turned(Some(UNIT)), 48.0 * 30.0);
        assert_eq!(fx.turned(Some(UNIT)), 0.0);
        let fired = ticks(&mut fx, 12, Some(moved));
        assert_eq!(fx.turned(Some(UNIT)), 0.0);
        // No sound, and gone once its age reaches 60.
        assert!(fired.is_empty() && !fx.is_empty());
        ticks(&mut fx, 1, Some(moved));
        assert!(fx.is_empty());
    }

    #[test]
    fn the_statuses_replace_their_copy_and_the_objects_end_with_the_unit() {
        for id in [1201, 1214, 1215, 1216, 1217, 1222] {
            assert!(restarts_on_status(id), "{id}");
            let mut fx = WorldFx::default();
            let launch = Launch::at(id, Some(Some(UNIT)), FEET);
            let first = fx.status(&launch, true, &mut Fake).unwrap();
            let again = fx.status(&launch, true, &mut Fake).unwrap();
            assert_ne!(first, again);
            assert_eq!(fx.iter().count(), 1);
            ticks(&mut fx, 2, Some(FEET));
            ticks(&mut fx, 2, None);
            assert!(fx.is_empty(), "{id}");
            fx.status(&launch, true, &mut Fake).unwrap();
            assert_eq!(fx.status(&launch, false, &mut Fake), None);
            assert!(fx.is_empty());
        }
    }

    #[test]
    fn the_soul_packet_ignores_other_jobs_and_resyncs_the_count() {
        assert!(object_souls(4240) && object_souls(4242) && object_souls(4303) && !object_souls(4001));
        let mut fx = WorldFx::default();
        let owner = Some(Some(UNIT));
        souls(&mut fx, owner, 3, true, FEET, false, &mut Fake);
        assert_eq!(fx.iter().count(), 1);
        let count = |fx: &mut WorldFx| fx.class_mut::<Souls>(owner, &[SOULS]).unwrap().1.count();
        assert_eq!(count(&mut fx), 3);
        // A running copy is kept; 3 -> 5 adds two pairs, 5 -> 3 removes the newest two.
        souls(&mut fx, owner, 5, true, FEET, false, &mut Fake);
        assert_eq!((fx.iter().count(), count(&mut fx)), (1, 5));
        ticks(&mut fx, 1, Some(FEET));
        let alphas = fx.class_mut::<Souls>(owner, &[SOULS]).unwrap().1.alphas.clone();
        souls(&mut fx, owner, 3, true, FEET, false, &mut Fake);
        assert_eq!(fx.class_mut::<Souls>(owner, &[SOULS]).unwrap().1.alphas, alphas[..3]);
        // A hidden unit loses it; 0 leaves it with no quads.
        souls(&mut fx, owner, 0, true, FEET, false, &mut Fake);
        assert_eq!(count(&mut fx), 0);
        assert!(drawn(&fx).is_empty());
        souls(&mut fx, owner, 2, false, FEET, false, &mut Fake);
        assert!(fx.is_empty());
        // The funnel and the orbs come only from their own launchers.
        assert!(is_class(435) && is_class(1225) && !is_class(1197));
    }

    // 435 and 1223

    /// A source that answers 0 for the 168 lean draws, then 1, 0, 2 for the three ticks (every quad alike).
    fn funnel_draws() -> Rand {
        let mut asked = 0;
        Box::new(move |_| {
            asked += 1;
            if asked <= 168 { 0 } else { [1, 0, 2][(asked - 169) / 10] }
        })
    }

    #[test]
    fn the_funnel_is_ten_flat_squares_turning_by_their_draws() {
        let mut solo = Solo::new(435, None, Kick::new(true, funnel_draws()));
        assert_eq!((solo.class.squares.len(), solo.class.strips.len(), solo.fx.length), (10, 168, 100));
        for _ in 0..3 {
            solo.tick();
        }
        // 45 j + 4 + 3 + 5 after the draws 1, 0, 2.
        for (j, square) in solo.class.squares.iter().enumerate() {
            assert_eq!(square.angle, (45 * j as i32 + 12) % 360, "{j}");
        }
        let faces = solo.faces();
        assert_eq!(faces.len(), 10 + 8 * 20);
        for (j, face) in faces.iter().take(10).enumerate() {
            let corners: Vec<Vector> = [0, 1, 2, 5].iter().map(|&at| face.vertices[at].position).collect();
            // Flat at 51 - 5 j over the feet; the half-diagonals 40 ... 18.4.
            assert!(corners.iter().all(|c| close(c[1], FEET[1] + 51.0 - 5.0 * j as f32)), "{j}");
            assert!(corners.iter().all(|c| close(horizontal(*c, FEET), FUNNEL[j])), "{j}");
            // The corners lie a quarter turn apart: v0 and v1 a quarter, v0 and v3 half a turn.
            assert!(close(horizontal(corners[0], corners[1]), FUNNEL[j] * 2.0f32.sqrt()), "{j}");
            assert!(close(horizontal(corners[0], corners[3]), 2.0 * FUNNEL[j]), "{j}");
            assert_eq!(face.vertices.iter().map(|v| v.uv).collect::<Vec<_>>(), [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [0.0, 1.0], [1.0, 0.0], [1.0, 1.0]]);
            assert!(face.additive && face.flags == FLAGS_EFFECT);
            assert_eq!(&face.vertices[0].tint[..3], &FUNNEL_BLUE);
        }
    }

    #[test]
    fn the_funnel_fades_in_for_40_ticks_holds_to_60_and_out() {
        let mut solo = Solo::new(435, None, Kick::new(true, Box::new(|_| 0)));
        let mut alphas = Vec::new();
        for _ in 0..102 {
            solo.tick();
            alphas.push(solo.class.squares[0].alpha);
        }
        // After the update at e: 1 at e = 0, 41 at e = 40, held to 60, 21 at 80, 0 at 101.
        let after = |e: usize| alphas[e];
        assert_eq!([0, 40, 60, 80, 101].map(after), [1, 41, 41, 21, 0]);
    }

    #[test]
    fn the_ribbons_are_168_quads_with_their_lean_draws_in_order_and_four_variants() {
        let (draws, asked) = scripted((0..168).map(|i| (i * 5) % 21).collect());
        let solo = Solo::new(435, None, Kick::new(true, draws));
        assert!(asked.borrow().iter().all(|&n| n == 21) && asked.borrow().len() == 168);
        assert!(solo.class.strips.iter().enumerate().all(|(i, strip)| strip.lean == 80 + (i as i32 * 5) % 21));
        // Rows k of 21 quads; the second call after the first's 84.
        let variant = |round: bool, call: usize, k: usize| {
            let solo = Solo::new(if round { 435 } else { 1223 }, None, Kick::new(round, Box::new(|_| 0)));
            let strip = &solo.class.strips[(call * 4 + k) * 21 + 7];
            (strip.h, strip.r, strip.turn, strip.arc)
        };
        assert_eq!(variant(true, 0, 0), (48.0, 14.0, 0, 30));
        assert_eq!(variant(true, 0, 3), (33.0, 8.0, 270, 30));
        assert_eq!(variant(true, 1, 0), (28.0, 6.0, 0, 30));
        assert_eq!(variant(true, 1, 3), (13.0, 3.0, 270, 30));
        assert_eq!(variant(false, 0, 0), (28.0, 10.0, 0, 30));
        assert_eq!(variant(false, 0, 3), (13.0, 4.0, 270, 30));
        assert_eq!(variant(false, 1, 0), (18.0, 5.0, 0, 30));
        assert_eq!(variant(false, 1, 3), (3.0, 2.0, 270, 30));
        // 1223 has no funnel and 168 quads, 160 of them strips.
        let moon = Solo::new(1223, None, Kick::new(false, Box::new(|_| 0)));
        assert_eq!((moon.class.squares.len(), moon.class.strips.len(), moon.fx.length), (0, 168, 60));
    }

    #[test]
    fn a_ribbon_sweeps_from_20_to_180_degrees_a_unit_tall() {
        let mut solo = Solo::new(435, None, Kick::new(true, Box::new(|_| 0)));
        solo.tick();
        // e = 0: arc 33 (1 step of j), turn 5: phi_j = j + 5, lean 80 -> the top edge 1 unit up the lean.
        let strip = &solo.class.strips[2];
        let (sin, cos) = (7f32.to_radians().sin(), 7f32.to_radians().cos());
        assert!(close3(strip.base, [14.0 * cos, 48.0, 14.0 * sin]));
        let (lean_sin, lean_cos) = (80f32.to_radians().sin(), 80f32.to_radians().cos());
        assert!(close3(strip.top, [14.0 * cos + cos * lean_cos, 48.0 + lean_sin, 14.0 * sin + sin * lean_cos]));
        // The top edge is 1 unit from the base.
        assert!((dot(sub(strip.top, strip.base), sub(strip.top, strip.base)).sqrt() - 1.0).abs() < 1e-4);
        for _ in 0..50 {
            solo.tick();
        }
        // e = 50: arc 180 (9 steps), turn 255, r grown by 0.1 for each of e = 13 ... 50.
        let strip = &solo.class.strips[3];
        assert_eq!((strip.arc, strip.turn), (180, 255));
        let phi = (27 + 255) as f32;
        assert!((strip.r - 17.8).abs() < 1e-3);
        assert!(close3(strip.base, [17.8 * phi.to_radians().cos(), 48.0, 17.8 * phi.to_radians().sin()]));
        // Quad 3 of the row is the strip between the points 2 and 3, u from 2/21 to 3/21, the top edge at v = 0.
        let faces = solo.faces();
        let strip_face = &faces[10 + 2];
        assert_eq!(strip_face.vertices[0].uv, [2.0 / 21.0, 0.0]);
        assert_eq!(strip_face.vertices[5].uv, [3.0 / 21.0, 1.0]);
    }

    #[test]
    fn the_ribbons_alpha_follows_each_variant() {
        let alpha_after = |round: bool, ticks: &[usize]| {
            let mut solo = Solo::new(if round { 435 } else { 1223 }, None, Kick::new(round, Box::new(|_| 0)));
            let mut seen = Vec::new();
            for e in 0..=62 {
                solo.tick();
                if ticks.contains(&e) {
                    seen.push(solo.class.strips[5].alpha);
                }
            }
            seen
        };
        assert_eq!(alpha_after(true, &[0, 11, 12, 50, 62]), [1, 12, 12, 12, 0]);
        assert_eq!(alpha_after(false, &[0, 30, 59]), [3, 93, 6]);
    }

    #[test]
    fn the_owner_spins_only_with_an_owner_and_only_to_e_47() {
        let mut owned = Solo::new(1223, Some(Some(UNIT)), Kick::new(false, Box::new(|_| 0)));
        let mut turns = Vec::new();
        for _ in 0..50 {
            owned.tick();
            turns.push(owned.fx.turn);
        }
        assert_eq!((turns[0], turns[47], turns[49]), (30.0, 1440.0, 1440.0));
        let mut loose = Solo::new(435, None, Kick::new(true, Box::new(|_| 0)));
        loose.tick();
        assert_eq!(loose.fx.turn, 0.0);
    }

    // Light spheres

    #[test]
    fn a_light_sphere_has_four_core_and_640_ray_quads_and_draws_2560_numbers() {
        let asked = Rc::new(RefCell::new(Vec::new()));
        let log = asked.clone();
        // The answer to call i is 3 i below the range.
        let draws: Rand = Box::new(move |n| {
            let mut log = log.borrow_mut();
            log.push(n);
            (3 * (log.len() as u32 - 1)) % n
        });
        let solo = Solo::new(1197, None, Light::new(draws));
        assert_eq!(solo.class.rays.len(), 640);
        // Per ray in order: a = R(360), b = R(360), theta = R(360), then R(2).
        let log = asked.borrow().clone();
        assert_eq!(log.len(), 2560);
        assert!(log.chunks(4).all(|ray| ray == [360, 360, 360, 2]));
        let ray = &solo.class.rays[1];
        assert_eq!((ray.a, ray.b, ray.theta, ray.colour), (12, 15, 18, 1));
        assert_eq!((solo.class.rays[319].set, solo.class.rays[320].set), (0, 1));
        assert_eq!(solo.faces().len(), 4 + 640);
        // The colours of each id.
        assert_eq!(light_colours(1197), (rgb(0xFF7C80), [rgb(0xFF7C80), rgb(0xFF4A4E)]));
        assert_eq!(light_colours(1198), (rgb(0xFFFF37), [rgb(0xFFFF9B), rgb(0xFFFF37)]));
        assert_eq!(light_colours(1199), (rgb(0x3BCCFF), [rgb(0x3BCCFF), rgb(0x6D9AFF)]));
    }

    #[test]
    fn the_core_fades_in_pulses_and_the_rays_grow_and_fade() {
        let mut solo = Solo::new(1198, None, Light::new(Box::new(|_| 0)));
        let mut core = Vec::new();
        let mut scale = Vec::new();
        for _ in 0..=555 {
            solo.tick();
            core.push(solo.class.core_alpha);
            scale.push((solo.class.rays[0].scale, solo.class.rays[0].alpha, solo.class.core_scale));
        }
        assert_eq!([0, 253, 299, 300, 554].map(|c| core[c]), [2, 255, 255, 254, 0]);
        // The ray's size 1 -> 10.13 at update 117 (index 116); the alpha 40 to c = 500, 0 at 540.
        assert!(scale[115].0 < 10.0 && (scale[116].0 - 10.13).abs() < 0.02 && scale[117].0 == scale[116].0);
        assert_eq!([500, 501, 540].map(|c| scale[c].1), [40, 39, 0]);
        // The core pulses +-5 percent: 3.5 + 3.5 * 0.05 * sin(2 (c + 1)).
        assert!(close(scale[44].2, 3.5 * 1.05) && close(scale[134].2, 3.5 * 0.95));
        let scene = Scene { player: FEET, feet: &|_| None, body: &|_| None, longitude: 0.0, right: [1.0, 0.0, 0.0], up: [0.0, 1.0, 0.0], ground: &|_, _| 0.0 };
        assert!(!solo.class.alive(&solo.fx, &scene));
    }

    #[test]
    fn a_light_sphere_goes_after_e_554_and_does_not_need_its_caster() {
        let mut fx = launch(1199, Some(Some(UNIT)));
        // The caster leaves at once: the sphere stays where it stood.
        ticks(&mut fx, 555, None);
        assert_eq!(fx.iter().count(), 1);
        assert_eq!(fx.iter().next().unwrap().fx.position, FEET);
        ticks(&mut fx, 1, None);
        assert!(fx.is_empty());
        // Every cast makes a new copy.
        let mut fx = launch(1199, Some(Some(UNIT)));
        fx.launch(&Launch::at(1199, Some(Some(UNIT)), FEET), &mut Fake).unwrap();
        assert_eq!(fx.iter().count(), 2);
    }

    #[test]
    fn a_rays_apex_wanders_and_its_base_lies_along_x_or_the_view_depth() {
        let (draws, _) = scripted(vec![0, 0, 90, 0]);
        let mut solo = Solo::new(1197, None, Light::new(draws));
        // Ray 0: a = 0, b = 0, theta = 90 -> 91 after one tick (R(2) = 0 later): r = scale (1 + sin theta).
        solo.tick();
        let faces = solo.faces();
        let ray = &faces[4];
        let r = solo.class.rays[0].scale * (1.0 + 91f32.to_radians().sin());
        let eye = Eye::at(45.0, 0.0);
        let apex = yawed(FEET, [r, 10.0, 0.0], &eye);
        assert!(close3(ray.vertices[0].position, apex));
        let base0 = [ray.vertices[2].position, ray.vertices[5].position];
        assert!(close3(sub(base0[1], base0[0]), scaled(eye.ground_right(), 1.0)));
        // Set 1's base runs along the view depth.
        let ray1 = &faces[4 + 320];
        let depth = sub(ray1.vertices[5].position, ray1.vertices[2].position);
        assert!(close3(depth, scaled(eye.ground_forward(), -1.0)) || close3(depth, scaled(eye.ground_forward(), 1.0)));
        assert!(ray.additive && ray.flags == FLAGS_EFFECT);
    }

    // The plasma ball

    #[test]
    fn the_plasma_ball_shows_after_nine_updates_and_cycles_b_d_e_f_a() {
        let mut fx = launch(1200, Some(Some(UNIT)));
        let mut shown = Vec::new();
        for _ in 0..16 {
            ticks(&mut fx, 1, Some(FEET));
            shown.push(texture_names(&fx).first().map(|path| path.rsplit('\\').next().unwrap().to_owned()));
        }
        assert!(shown[..9].iter().all(Option::is_none));
        let names: Vec<_> = shown[9..].iter().map(|name| name.clone().unwrap()).collect();
        assert_eq!(
            names,
            [
                "thunder_plazma_blast_b.bmp",
                "thunder_ball_d.bmp",
                "thunder_ball_e.bmp",
                "thunder_ball_f.bmp",
                "thunder_plazma_blast_a.bmp",
                "thunder_plazma_blast_b.bmp",
                "thunder_ball_d.bmp"
            ]
        );
        // A 15 x 15 billboard, 5 up, over everything adding.
        let faces = drawn(&fx);
        assert_eq!((faces.len(), faces[0].1, List::of(faces[0].2)), (1, true, List::TopAdd));
        assert!(close(dot(sub(faces[0].0[0], faces[0].0[1]), sub(faces[0].0[0], faces[0].0[1])).sqrt(), 15.0));
        // It stands with its unit.
        ticks(&mut fx, 1, None);
        assert!(fx.is_empty());
    }

    // The gravity lift

    #[test]
    fn the_lift_rises_20_units_in_40_ticks_and_the_swirl_is_unshifted() {
        let mut fx = launch(1201, Some(Some(UNIT)));
        let mut lifts = Vec::new();
        for _ in 0..45 {
            ticks(&mut fx, 1, Some(FEET));
            lifts.push(fx.lifted(Some(UNIT)));
        }
        assert_eq!(&lifts[..8], [0.0, 1.0, 1.0, 2.0, 2.0, 3.0, 3.0, 4.0]);
        assert_eq!((lifts[38], lifts[39], lifts[44]), (19.0, 20.0, 20.0));
        let part = fx.class_mut::<Lift>(Some(Some(UNIT)), &[GRAVITY]).unwrap().1.part.as_ref().unwrap();
        assert_eq!(part.offset, [0.0, 0.0]);
        assert!(part.timeline.looped);
        // Its swirl follows the lifted feet; removing it drops the unit at once.
        assert_eq!(part.point, FEET);
        fx.remove(Some(Some(UNIT)), GRAVITY);
        assert_eq!(fx.lifted(Some(UNIT)), 0.0);
    }

    #[test]
    fn a_far_lift_stops_where_it_was() {
        let mut fx = launch(1201, Some(Some(UNIT)));
        ticks(&mut fx, 10, Some(FEET));
        let lift = fx.lifted(Some(UNIT));
        assert_eq!(lift, 5.0);
        ticks_from(&mut fx, 10, Some(FEET), [FEET[0] + 200.0, FEET[1], FEET[2]], false);
        assert_eq!(fx.lifted(Some(UNIT)), lift);
        ticks(&mut fx, 2, Some(FEET));
        assert_eq!(fx.lifted(Some(UNIT)), 6.0);
        assert_eq!(fx.lifted(None), 0.0);
    }

    // The nova globes

    #[test]
    fn the_nova_numbers_follow_the_table() {
        let row = |advent: bool, c: u32| {
            let (sphere, _, [outer, inner], alpha) = Nova::at(advent, c, if advent { 60 } else { 30 });
            (sphere, outer, inner, alpha)
        };
        for (c, sphere, outer, inner, alpha) in
            [(0, 1.13, 1.7, 0.0, 17), (7, 8.51, 13.0, 1.0, 136), (14, 14.95, 22.8, 10.8, 255), (21, 20.45, 31.1, 19.1, 136), (29, 25.59, 38.9, 26.9, 0)]
        {
            let got = row(false, c);
            assert!(close(got.0, sphere) && close(got.1, outer) && close(got.2, inner) && got.3 == alpha, "{c}: {got:?}");
        }
        let (start, middle, end) = (row(true, 0), row(true, 30), row(true, 59));
        assert!(close(start.0, 30.0) && close(start.1, 45.7) && close(middle.0, 0.0) && close(middle.1, 0.0) && close(end.0, 29.2) && close(end.1, 44.4));
        // The big one's alpha runs over 255: its low byte is a sawtooth.
        let low = |c: u32| (row(true, c).3 & 0xff) as u8;
        assert_eq!([0, 1, 15, 16, 30, 31, 44, 45].map(low), [254, 15, 253, 238, 0, 17, 238, 253]);
        // The sphere turns 3 degrees a step.
        assert_eq!(Nova::at(false, 4, 30).1, 15);
    }

    #[test]
    fn a_nova_is_72_globe_and_10_ring_quads_in_their_lists_with_their_sounds() {
        let mut small = launch(1202, Some(Some(UNIT)));
        assert!(ticks(&mut small, 5, Some(FEET)).is_empty());
        let faces = drawn(&small);
        assert_eq!(faces.len(), 82);
        assert!(faces[..72].iter().all(|(_, additive, flags, _)| !additive && List::of(*flags) == List::Translucent));
        assert!(faces[72..].iter().all(|(_, additive, flags, _)| !additive && List::of(*flags) == List::Top));
        // The ring lies flat at the feet, outer radius at the first corners.
        let c = 4;
        let (_, _, [outer, inner], _) = Nova::at(false, c, 30);
        assert!(faces[72..].iter().all(|(corners, ..)| corners[0][1] == FEET[1] && close(horizontal(corners[0], FEET), outer)));
        assert!(faces[72..].iter().all(|(corners, ..)| close(horizontal(corners[2], FEET), inner)));
        // Lengths 30 and 60; the big one sounds at e = 1 and e = 31.
        ticks(&mut small, 25, Some(FEET));
        assert!(!small.is_empty());
        ticks(&mut small, 1, Some(FEET));
        assert!(small.is_empty());
        let mut big = launch(1203, Some(Some(UNIT)));
        let fired = ticks(&mut big, 60, Some(FEET));
        assert_eq!(
            fired,
            [Fired::Sound("effect\\wizard_fire_pillar_a.wav".to_owned(), FEET), Fired::Sound("effect\\wizard_fire_pillar_b.wav".to_owned(), FEET)]
        );
        ticks(&mut big, 1, Some(FEET));
        assert!(big.is_empty());
        // Not tied to the owner: the caster gone, it runs its length.
        let mut orphan = launch(1202, Some(Some(UNIT)));
        ticks(&mut orphan, 20, None);
        assert!(!orphan.is_empty());
    }

    #[test]
    fn the_globe_is_a_unit_sphere_band_by_band() {
        let mut solo = Solo::new(1202, None, Nova::default());
        solo.tick();
        let faces = solo.faces();
        let radius = solo.class.sphere;
        // Band 0 is the top: its first latitude is the pole, one unit radius up.
        assert!(close(faces[0].vertices[0].position[1], FEET[1] + radius));
        // Band 5 ends at the bottom pole.
        assert!(close(faces[71].vertices[5].position[1], FEET[1] - radius));
        assert_eq!(faces[13].vertices[0].uv, [1.0 / 12.0, 1.0 / 6.0]);
        assert_eq!(faces[13].vertices[5].uv, [2.0 / 12.0, 2.0 / 6.0]);
    }

    // Glyphs, emblem

    #[test]
    fn a_glyph_circles_at_radius_8_chest_high_and_1217_has_three_pictures() {
        let mut solo = Solo::new(1217, Some(Some(UNIT)), Glyphs::default());
        assert!(solo.class.textures.iter().zip(STANCE_TEXTURES).all(|(got, want)| *got == want));
        let [x, y] = solo.class.fill[0];
        assert_eq!([x, y], [0.78125, 0.78125]);
        assert!(close3(solo.class.centre(0, FEET), [FEET[0], FEET[1] + 10.0, FEET[2] + 8.0]));
        solo.class.phi[0] = std::f32::consts::FRAC_PI_2;
        assert!(close3(solo.class.centre(0, FEET), [FEET[0] + 8.0, FEET[1] + 10.0, FEET[2]]));
        // The phases are 120 degrees apart and a turn takes 314 ticks.
        let mut solo = Solo::new(1214, Some(Some(UNIT)), Glyphs::default());
        assert!(solo.class.textures.iter().all(|path| path.ends_with("sun_stance.tga")));
        assert!((solo.class.phi[1] - solo.class.phi[0] - 120f32.to_radians()).abs() < 1e-5);
        solo.tick();
        assert!((solo.class.phi[0] - 0.02).abs() < 1e-6);
        // A 6 x 6 card whose picture fills the top left 0.78 of it.
        let faces = solo.faces();
        assert_eq!(faces.len(), 3);
        let eye = Eye::at(45.0, 0.0);
        let across = sub(faces[0].vertices[1].position, faces[0].vertices[0].position);
        assert!(close(dot(across, eye.right), 6.0 * 0.78125));
    }

    #[test]
    fn the_emblem_floats_15_up_or_20_when_the_unit_rides_in_the_opaque_list_pulled_forward() {
        let eye = Eye::at(45.0, 0.0);
        for (rides, height) in [(false, 15.0), (true, 20.0)] {
            let mut fx = launch(1222, Some(Some(UNIT)));
            ticks_from(&mut fx, 1, Some(FEET), FEET, rides);
            let faces = drawn(&fx);
            assert_eq!((faces.len(), faces[0].1, List::of(faces[0].2)), (1, false, List::Opaque));
            let corners = &faces[0].0;
            let centre: Vector = std::array::from_fn(|axis| (corners[0][axis] + corners[5][axis]) / 2.0);
            let moved = sub(centre, FEET);
            assert!(close(dot(moved, eye.up), height), "{rides}");
            // 0.035 toward the camera is 35 units of depth.
            assert!(close(dot(moved, eye.forward), -35.0), "{rides}");
            // 5 x 5 and upright.
            assert!(close(horizontal(corners[0], corners[1]), 5.0 * eye.ground_right()[0].hypot(eye.ground_right()[2])));
            assert!(corners[0][1] > corners[2][1]);
        }
    }

    // Orbs

    fn orbs_of(count: usize) -> WorldFx {
        let mut fx = WorldFx::default();
        let owner = Some(Some(UNIT));
        souls(&mut fx, owner, count as u16, true, FEET, false, &mut Fake);
        fx
    }

    #[test]
    fn seven_souls_show_orbs_2_to_6_in_72_degree_steps() {
        let mut fx = orbs_of(7);
        ticks(&mut fx, 1, Some(FEET));
        let orbs = fx.class_mut::<Souls>(Some(Some(UNIT)), &[SOULS]).unwrap().1;
        assert!(orbs.alphas[..2].iter().all(|alpha| *alpha == [0, 0]));
        assert!(orbs.alphas[2..].iter().all(|alpha| *alpha == [2, 2]));
        let groups: Vec<_> = (2..7).map(|i| orb_look(i).0).collect();
        assert_eq!(groups, [2.0, 2.0, 2.0, 3.5, 3.5]);
        // theta = 72 (i - 5 g) + phase, phase 0 at the first update.
        let angles: Vec<_> = (2..7).map(|i| orbs.angles[i]).collect();
        assert_eq!(angles, [144.0, 216.0, 288.0, 0.0, 72.0]);
        // Green orbs fade to 150 at 2 a tick, the cores to 255.
        ticks(&mut fx, 140, Some(FEET));
        let orbs = fx.class_mut::<Souls>(Some(Some(UNIT)), &[SOULS]).unwrap().1;
        assert!(orbs.alphas[2..].iter().all(|alpha| *alpha == [150, 255]));
        assert_eq!(orbs.alphas[0], [0, 0]);
        assert!(close(orbs.phase, 70.5));
    }

    #[test]
    fn red_orbs_fade_to_255_and_grow_with_the_energy() {
        assert_eq!([0, 5, 10, 15, 20, 30].map(|i| orb_look(i).0), [2.0, 3.5, 3.5, 5.5, 5.5, 5.5]);
        assert_eq!([0, 5, 10, 15, 20].map(|i| orb_look(i).1 == RED), [false, false, true, true, true]);
        assert_eq!([0, 5, 10, 15, 20].map(|i| orb_look(i).2), [150, 150, 255, 255, 255]);
        let mut fx = orbs_of(12);
        ticks(&mut fx, 140, Some(FEET));
        let orbs = fx.class_mut::<Souls>(Some(Some(UNIT)), &[SOULS]).unwrap().1;
        // Orbs 7 - 11 show: 7, 8, 9 green (150), 10, 11 red (255).
        assert_eq!([7, 9, 10, 11].map(|i| orbs.alphas[i][0]), [150, 150, 255, 255]);
        // Each orb is two quads, the core half the size and white; the newest five only.
        let faces = drawn(&fx);
        assert_eq!(faces.len(), 10);
        let side = |face: usize| horizontal(faces[face].0[0], faces[face].0[1]);
        assert!(close(side(1) * 2.0, side(0)));
        let (shows, hidden) = (|_: u32| true, |_: Option<u32>| false);
        let all = fx.faces(&seen(&shows, &hidden));
        assert_eq!(&all[0].vertices[0].tint[..3], &GREEN);
        assert_eq!(&all[1].vertices[0].tint[..3], &[1.0; 3]);
        assert_eq!(&all[8].vertices[0].tint[..3], &RED);
        assert!(all.iter().all(|face| List::of(face.flags) == List::Effect && face.additive));
    }

    // Sprites

    #[test]
    fn each_soul_sprite_has_its_frames_delay_mode_and_bias() {
        for (id, name, bias) in SPIRITS {
            let mut fx = launch(id, Some(Some(UNIT)));
            let spirit = fx.class_mut::<Spirit>(Some(Some(UNIT)), &[id]).unwrap().1;
            let card = spirit.card.as_ref().unwrap();
            assert_eq!(card.sprite, format!("data\\sprite\\이팩트\\{name}\\{name}"));
            assert_eq!(
                (card.action, card.delay, card.end, card.flags, card.bias, card.scale, card.alpha, card.layer_colours),
                (0, 4.0, End::Stop, 0x101, bias, 1.0, 255.0, true)
            );
            assert_eq!(List::of(card.flags), List::WithSprites);
            // It ends with its animation: about 4 ticks a frame, one drawn frame a tick.
            let frames = frames_of(&card.sprite);
            let (shows, hidden) = (|_: u32| true, |_: Option<u32>| false);
            let mut ended = 0;
            for tick in 1..=frames * 4 + 4 {
                ticks(&mut fx, 1, Some(FEET));
                if fx.is_empty() {
                    ended = tick;
                    break;
                }
                // Two drawn frames a tick step it once.
                fx.frame(&seen(&shows, &hidden), &mut Fake);
                fx.frame(&seen(&shows, &hidden), &mut Fake);
            }
            assert!((frames * 4 - 1..=frames * 4 + 1).contains(&ended), "{id}: {ended}");
        }
    }

    #[test]
    fn a_card_that_is_not_drawn_never_ends() {
        let mut fx = launch(1234, Some(Some(UNIT)));
        ticks(&mut fx, 500, Some(FEET));
        assert_eq!(fx.iter().count(), 1);
    }
}
