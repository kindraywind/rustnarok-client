//! Third-job objects: Flower Leaf's rising glints (1013), Frigg's Song's light columns (1059), Bind Trap's rune circle
//! (1069 – 1071), Cloud Kill's haze (1080), the Rebellion's coins (1135), Fallen Angel's feathers (1140), the Warlock's
//! spheres (1262 – 1265), the shots of Triangle Shot (1324) and Cart Cannon (1357), and the boomerangs of Axe Boomerang
//! (1657) and Shield Shooting (2183).

use std::any::Any;

use super::{Assets, Card, Class, End, FLAGS_EFFECT, Face, Fx, Launch, Seen, StrPart, Tex, WorldFx, padded_ratio, triangles, yawed};
use crate::world::Vector;
use crate::world_str::{Scene, Timeline};

pub const FLOWER_LEAF: u32 = 1013;
pub const FRIGG: u32 = 1059;
pub const RUNES: u32 = 1069;
pub const CLOUD_KILL: u32 = 1080;
pub const COINS: u32 = 1135;
pub const FALLEN_ANGEL: u32 = 1140;
/// The spheres: fire, water, wind, earth.
pub const SPHERES: [u32; 4] = [1262, 1263, 1264, 1265];
pub const TRIANGLE_SHOT: u32 = 1324;
pub const CART_CANNON: u32 = 1357;
pub const AXE_BOOMERANG: u32 = 1657;
pub const SHIELD_SHOOTING: u32 = 2183;
/// Triangle Shot's STRs on its target, and Axe Boomerang's hit.
pub const TRIANGLE_HIT: u32 = 1296;
pub const TRIANGLE_HIT_2: u32 = 1297;

/// WM_FRIGG_SONG, RL_FALLEN_ANGEL, SC_TRIANGLESHOT, GN_CARTCANNON, NC_AXEBOOMERANG and IG_SHIELD_SHOOTING.
pub const FRIGG_SKILL: u16 = 5007;
pub const FALLEN_ANGEL_SKILL: u16 = 2564;
pub const TRIANGLE_SKILL: u16 = 2288;
pub const CART_CANNON_SKILL: u16 = 2477;
pub const AXE_SKILL: u16 = 2278;
pub const SHIELD_SKILL: u16 = 5265;

/// UNT_B_TRAP.
pub const BIND_TRAP_UNIT: u32 = 260;
/// EFST_SPHERE_1 … 5.
const SPHERE_STATUSES: std::ops::RangeInclusive<u16> = 363..=367;
/// Cart Cannon's ball leaves this long after its packet.
pub const CART_CANNON_DELAY_MS: i32 = 200;
const CART_CANNON_SOUND: &str = "effect\\mon_침투경.wav";

const CHERRY: &str = "data\\texture\\effect\\cherry_circle.bmp";
const RING_BLUE: &str = "data\\texture\\effect\\ring_blue.tga";
const RUNE: &str = "data\\texture\\effect\\hero_011.tga";
const COIN_FACES: [&str; 5] = [
    "data\\texture\\effect\\rl_richs_coin_b\\rcoin1.bmp",
    "data\\texture\\effect\\rl_richs_coin_b\\rcoin2.bmp",
    "data\\texture\\effect\\rl_richs_coin_b\\rcoin3.bmp",
    "data\\texture\\effect\\rl_richs_coin_b\\rcoin4.bmp",
    "data\\texture\\effect\\rl_richs_coin_b\\rcoin5.bmp",
];
const COIN_LIGHT: &str = "data\\texture\\effect\\rl_richs_coin_b\\light.bmp";
const PUFF: &str = "data\\sprite\\이팩트\\particle3";

/// Registers the third-job objects.
pub fn register(fx: &mut WorldFx) {
    fx.register(FLOWER_LEAF, || Box::new(Motes::new(pick)));
    fx.register(FRIGG, || Box::new(Frigg::default()));
    for id in RUNES..=RUNES + 2 {
        fx.register(id, || Box::new(Runes::default()));
    }
    fx.register(CLOUD_KILL, || Box::new(Haze::new(pick)));
    fx.register(COINS, || Box::new(Coins::default()));
    fx.register(FALLEN_ANGEL, || Box::new(Feathers::default()));
    for id in SPHERES {
        fx.register(id, || Box::new(Spheres::default()));
    }
    for id in [TRIANGLE_SHOT, CART_CANNON] {
        fx.register(id, || Box::new(Line::default()));
    }
    for id in [AXE_BOOMERANG, SHIELD_SHOOTING] {
        fx.register(id, || Box::new(Boomerang::default()));
    }
}

/// One random whole number below `n`.
fn pick(n: u32) -> u32 {
    crate::effects::pick(0, n.saturating_sub(1))
}

fn add(a: Vector, b: Vector) -> Vector {
    std::array::from_fn(|axis| a[axis] + b[axis])
}

fn sub(a: Vector, b: Vector) -> Vector {
    std::array::from_fn(|axis| a[axis] - b[axis])
}

fn scaled(v: Vector, by: f32) -> Vector {
    v.map(|value| value * by)
}

fn length(v: Vector) -> f32 {
    v.iter().map(|value| value * value).sum::<f32>().sqrt()
}

/// A square of half-side `half` on the axes `x` and `y` through `centre`.
fn square(texture: &'static str, centre: Vector, (x, y): (Vector, Vector), half: f32, uv: [f32; 4], tint: [f32; 4]) -> Face<'static> {
    let corner = |cx: f32, cy: f32| add(centre, add(scaled(x, cx * half), scaled(y, cy * half)));
    let [u0, u1, v0, v1] = uv;
    Face {
        texture: Tex::Path(texture),
        vertices: triangles([corner(-1.0, 1.0), corner(1.0, 1.0), corner(-1.0, -1.0), corner(1.0, -1.0)], [[u0, v0], [u1, v0], [u0, v1], [u1, v1]], tint),
        additive: true,
        flags: FLAGS_EFFECT,
    }
}

/// A status object's launch: Flower Leaf's glints start afresh each time its status comes on.
pub fn restarts_on_status(effect: u32) -> bool {
    effect == FLOWER_LEAF
}

/// What a skill unit of `effect` shows at `level`: Bind Trap's circle by its level (none past 5), the others their own.
pub fn unit_effect(effect: u32, level: u8) -> Option<u32> {
    if effect != RUNES {
        return Some(effect);
    }
    match level {
        1 => Some(RUNES),
        2 | 3 => Some(RUNES + 1),
        4 | 5 => Some(RUNES + 2),
        _ => None,
    }
}

/// Frigg's Song's nine columns at level `level` round `at`: the rows 1, 0, −1 by the columns −1, 0, 1, (level + 2) · 5
/// apart.
pub fn frigg_points(at: Vector, level: i32) -> Vec<Vector> {
    let apart = ((level + 2) * 5) as f32;
    [1.0, 0.0, -1.0].into_iter().flat_map(|k: f32| [-1.0, 0.0, 1.0].map(|i: f32| [at[0] + i * apart, at[1], at[2] + k * apart])).collect()
}

/// Whose spirits the coins (1135) show instead of the coin sprite: Rebellion, Baby Rebellion, Night Watch.
pub fn object_coins(job: u16) -> bool {
    matches!(job, 4215 | 4229 | 4306)
}

/// A sphere status's slot, 0 … 4.
pub fn sphere_slot(status: u16) -> Option<usize> {
    SPHERE_STATUSES.contains(&status).then(|| usize::from(status - SPHERE_STATUSES.start()))
}

/// A sphere status's value (the element code 0x44 – 0x47) to its object: fire, wind, water, earth by its low two bits.
pub fn sphere_effect(value: i32) -> u32 {
    [1262, 1264, 1263, 1265][(value & 3) as usize]
}

// Flower Leaf (1013)

/// One of Flower Leaf's glints: it waits under the feet, pops up, rises with a wobble and fades from 20 up to 30.
#[derive(Debug, Clone, PartialEq)]
struct Mote {
    /// Its height over the feet; at or under 0 it waits.
    y: f32,
    /// The two angles and the angles they chase, whole degrees.
    a: [i32; 2],
    t: [i32; 2],
    off: [f32; 2],
    alpha: i32,
    shown: bool,
}

impl Mote {
    fn new(rand: &mut impl FnMut(u32) -> u32) -> Self {
        let y = -(rand(100) as f32);
        let [a1, a2, t1, t2] = [(); 4].map(|_| rand(360) as i32);
        Self { y, a: [a1, a2], t: [t1, t2], off: [0.0; 2], alpha: 0, shown: false }
    }

    fn update(&mut self, rand: &mut impl FnMut(u32) -> u32) {
        self.shown = self.y > 0.0 && self.y <= 30.0;
        if self.shown {
            for k in 0..2 {
                (self.a[k], self.t[k]) = chase(self.a[k], self.t[k], rand);
            }
            self.off[0] += 0.15 * (self.a[0] as f32).to_radians().sin();
            self.off[1] += 0.15 * (self.a[1] as f32).to_radians().sin();
            self.alpha = if self.y > 20.0 { (self.alpha - 4).max(0) } else { 255 };
        }
        if self.y > 30.0 {
            *self = Self::new(rand);
        }
        self.y += 0.15;
    }
}

/// One step of an angle `a` chasing `t`: 3 degrees up toward a larger one, 2 down toward a smaller; within 3 of it, a new
/// one to chase.
// ponytail: an angle can reach −1, −2 or 361 for a tick and the game's table read jumps there (0.94 units); not reproduced.
fn chase(a: i32, t: i32, rand: &mut impl FnMut(u32) -> u32) -> (i32, i32) {
    let d = t - a;
    let next = a + if d > 0 { 3 } else { -2 };
    if d.abs() >= 3 {
        return (next, t);
    }
    let r = if a != 0 { rand(a as u32) as i32 } else { 0 };
    (next, if d > 0 { r } else { 359 - r })
}

/// Flower Leaf's 16 glints rising from the feet while the status lasts.
struct Motes {
    rand: fn(u32) -> u32,
    motes: Vec<Mote>,
}

impl Motes {
    fn new(rand: fn(u32) -> u32) -> Self {
        Self { rand, motes: Vec::new() }
    }
}

impl Class for Motes {
    fn begin(&mut self, _: &mut Fx, assets: &mut dyn Assets) {
        assets.texture(CHERRY);
        self.motes = (0..16).map(|_| Mote::new(&mut self.rand)).collect();
    }

    fn update(&mut self, _: &mut Fx, _: &Scene, _: &mut dyn Assets) {
        for mote in &mut self.motes {
            mote.update(&mut self.rand);
        }
    }

    fn draw<'a>(&'a self, fx: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        let eye = &seen.eye;
        for mote in self.motes.iter().filter(|mote| mote.shown) {
            // The height drawn is the one the update showed.
            let centre = yawed(fx.position, [mote.off[0], mote.y - 0.15, mote.off[1]], eye);
            let tint = [1.0, 1.0, 1.0, f32::from((mote.alpha & 0xff) as u8) / 255.0];
            // Drawn twice: twice as bright.
            for _ in 0..2 {
                out.push(square(CHERRY, centre, (eye.right, eye.up), 0.55, [0.0, 1.0, 0.0, 1.0], tint));
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

// Frigg's Song (1059)

/// One of Frigg's Song's columns: a cylinder of 20 sides, radius 2.5, that flashes low, shoots up and sinks while it
/// turns.
#[derive(Default)]
struct Frigg {
    /// Its age and the height it sinks from.
    n: u32,
    sink: Option<f32>,
    wall: f32,
}

/// A column's length in ticks.
const FRIGG_LENGTH: u32 = 350;

impl Frigg {
    /// The wall's height at age `n`, the sinking height carried from tick to tick.
    fn wall_at(n: u32, sink: &mut Option<f32>) -> f32 {
        let h = if n < 20 {
            1.0
        } else if n <= 140 {
            (((n - 20) as f32).to_radians().sin() * 120.0).max(0.0)
        } else {
            let c0 = sink.get_or_insert(120.0);
            *c0 = (*c0 - 0.5 - 0.01 * *c0).max(0.0);
            *c0
        };
        120.0 * h / 100.0 * 1.1
    }
}

impl Class for Frigg {
    fn begin(&mut self, fx: &mut Fx, assets: &mut dyn Assets) {
        assets.texture(RING_BLUE);
        fx.length = FRIGG_LENGTH;
        fx.events.push((0, super::Event::Sound { file: "effect\\wm_frigg_song.wav".to_owned(), listener: false }));
    }

    fn update(&mut self, _: &mut Fx, _: &Scene, _: &mut dyn Assets) {
        self.wall = Self::wall_at(self.n, &mut self.sink);
        self.n += 1;
    }

    fn draw<'a>(&'a self, fx: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        // The age the update drew, turned −n degrees.
        let turn = (self.n.saturating_sub(1)) as f32;
        let foot = |degrees: f32| {
            let (sin, cos) = (degrees - turn).to_radians().sin_cos();
            yawed(fx.position, [2.5 * cos, 0.0, 2.5 * sin], &seen.eye)
        };
        let tint = [50.0 / 255.0, 200.0 / 255.0, 50.0 / 255.0, 50.0 / 255.0];
        // Sides 1 … 20; the game's sides 0 and 21 are not drawn.
        // ponytail: the game leaves two small stray squares at the centre.
        for j in 1..=20u32 {
            let (from, to) = (foot(18.0 * (j - 1) as f32), foot((18 * j % 360) as f32));
            let up = |at: Vector| [at[0], at[1] + self.wall, at[2]];
            let (u0, u1) = ((j - 1) as f32 / 21.0, j as f32 / 21.0);
            out.push(Face {
                texture: Tex::Path(RING_BLUE),
                vertices: triangles([up(from), up(to), from, to], [[u0, 0.0], [u1, 0.0], [u0, 1.0], [u1, 1.0]], tint),
                additive: true,
                flags: FLAGS_EFFECT,
            });
        }
    }

    fn any(&mut self) -> Option<&mut dyn Any> {
        Some(self)
    }
}

// Bind Trap (1069 – 1071)

/// Bind Trap's orange rune circle lying flat on its cell, 1 unit up, while the trap stands.
#[derive(Default)]
struct Runes {
    half: f32,
}

/// The circle's half-side by its id: 9, 15 or 21.
fn rune_half(effect: u32) -> f32 {
    (6 * (effect - RUNES) + 9) as f32
}

impl Class for Runes {
    fn begin(&mut self, fx: &mut Fx, assets: &mut dyn Assets) {
        assets.texture(RUNE);
        self.half = rune_half(fx.id);
        fx.length = 8000;
    }

    fn update(&mut self, _: &mut Fx, _: &Scene, _: &mut dyn Assets) {}

    fn draw<'a>(&'a self, fx: &Fx, _: &Seen, out: &mut Vec<Face<'a>>) {
        let centre = [fx.position[0], fx.position[1] + 1.0, fx.position[2]];
        out.push(square(RUNE, centre, ([1.0, 0.0, 0.0], [0.0, 0.0, 1.0]), self.half, [0.0, 1.0, 0.0, 1.0], [1.0; 4]));
    }

    fn alive(&self, fx: &Fx, _: &Scene) -> bool {
        !fx.released && fx.timed()
    }
}

// Cloud Kill (1080)

/// One of Cloud Kill's puffs: it rises slower and slower while it spins, fading in and then out.
struct Puff {
    card: Card,
    offset: Vector,
    rise: f32,
    speed: f32,
    decel: f32,
    alpha: f32,
    da: f32,
    fade: u32,
    life: u32,
    age: u32,
}

impl Puff {
    fn new(offset: Vector, scale: f32, (speed, decel): (f32, f32), (alpha, fade, life): (f32, u32, u32)) -> Self {
        let mut card = Card::new(PUFF);
        card.flags = 1;
        card.layer_colours = false;
        card.end = End::Still;
        card.scale = scale;
        Self { card, offset, rise: 0.0, speed, decel, alpha, da: 25.4, fade, life, age: 0 }
    }

    /// A puff at the feet: 0.25 – 0.49 a tick up, from alpha 70.
    fn centre(rand: &mut impl FnMut(u32) -> u32) -> Self {
        rand(360);
        let speed = (rand(20) + 20) as f32 / 80.0;
        Self::new([0.0; 3], 0.8, (speed, -speed / 20.0), (70.0, 8, 10))
    }

    /// The grid's 36 puffs, each drawn about its point of a 6 × 6 grid 4 units apart.
    fn grid(rand: &mut impl FnMut(u32) -> u32) -> Vec<Self> {
        let mut puffs = Vec::new();
        for z in (-10..=10).step_by(4) {
            for x in (-10..=10).step_by(4) {
                let theta = (rand(360) as f32).to_radians();
                let k = (rand(6) + 2) as f32;
                let speed = (rand(30) + 30) as f32 / 100.0;
                let scale = (rand(50) + 40) as f32 / 170.0;
                let offset = [x as f32 + k * theta.sin(), 0.0, z as f32 + k * theta.cos()];
                puffs.push(Self::new(offset, scale, (speed, -speed / 80.0), (0.0, 32, 40)));
            }
        }
        puffs
    }

    fn step(&mut self) {
        self.speed += self.decel;
        self.rise += self.speed;
        self.card.spin += 3.0;
        if self.age == self.fade {
            self.da = -self.alpha / (self.life - self.age) as f32;
        }
        self.alpha = (self.alpha + self.da).clamp(0.0, 254.0).trunc();
        self.card.alpha = self.alpha;
        self.age += 1;
    }
}

/// Cloud Kill's violet haze while its unit stands: a puff at the centre every 5 ticks and a grid of 36 every 9.
struct Haze {
    rand: fn(u32) -> u32,
    centre: Vec<Puff>,
    grid: Vec<Puff>,
}

impl Haze {
    fn new(rand: fn(u32) -> u32) -> Self {
        Self { rand, centre: Vec::new(), grid: Vec::new() }
    }
}

impl Class for Haze {
    fn begin(&mut self, fx: &mut Fx, _: &mut dyn Assets) {
        fx.length = 9999;
    }

    fn update(&mut self, fx: &mut Fx, _: &Scene, assets: &mut dyn Assets) {
        let e = fx.age;
        let standing = !fx.released;
        if standing && e.is_multiple_of(5) {
            self.centre.push(Puff::centre(&mut self.rand));
        }
        for puff in &mut self.centre {
            puff.step();
            puff.card.step(assets);
        }
        self.centre.retain(|puff| puff.age < puff.life);
        if standing && e.is_multiple_of(9) {
            self.grid.extend(Puff::grid(&mut self.rand));
        }
        for puff in &mut self.grid {
            puff.step();
            puff.card.step(assets);
        }
        self.grid.retain(|puff| puff.age < puff.life);
        // The unit gone: the puffs finish within 50 ticks.
        if !standing && fx.length == 9999 {
            fx.length = e + 50;
        }
    }

    fn draw<'a>(&'a self, fx: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        for puff in self.centre.iter().chain(&self.grid) {
            let at = add(add(fx.position, puff.offset), [0.0, puff.rise, 0.0]);
            puff.card.faces(at, 1.0, false, &seen.eye, out);
        }
    }

    fn any(&mut self) -> Option<&mut dyn Any> {
        Some(self)
    }
}

// The Rebellion's coins (1135)

/// Up to five gold coins circling the chest, flipping as they go; a glow rides each spirit past the fifth.
#[derive(Default)]
struct Coins {
    /// Each coin's angle (radians) and face, then each light's angle.
    coins: Vec<(f32, usize)>,
    lights: Vec<f32>,
    uv: [f32; 4],
}

impl Coins {
    /// The coins' and the lights' start angles for `count` spirits.
    fn angles(count: i32) -> (Vec<f32>, Vec<f32>) {
        let n1 = count.clamp(0, 5);
        let coins = (0..n1).map(|i| ((360 / n1 * i) as f32).to_radians() - 10.0).collect();
        let lights = (0..(count - 5).max(0)).map(|k| ((72 * k) as f32).to_radians() - 10.0).collect();
        (coins, lights)
    }

    /// The coin's face at angle `theta` (radians) under a camera at `longitude`: one of five a 36 degrees.
    fn face(theta: f32, longitude: f32) -> usize {
        let a = (theta.to_degrees() - longitude).rem_euclid(360.0);
        (a / 36.0).trunc() as usize % 5
    }

    /// A coin's centre: 7 out and 10 up from the feet.
    fn centre(feet: Vector, theta: f32) -> Vector {
        [feet[0] + 7.0 * theta.cos(), feet[1] + 10.0, feet[2] + 7.0 * theta.sin()]
    }
}

impl Class for Coins {
    fn begin(&mut self, fx: &mut Fx, assets: &mut dyn Assets) {
        let size = assets.texture(COIN_FACES[0]).unwrap_or([32.0; 2]);
        for face in &COIN_FACES[1..] {
            assets.texture(face);
        }
        assets.texture(COIN_LIGHT);
        let [u, v] = padded_ratio(size);
        self.uv = [0.0, u, 0.0, v];
        let (coins, lights) = Self::angles(fx.a7);
        self.coins = coins.into_iter().map(|theta| (theta, 0)).collect();
        self.lights = lights;
    }

    fn update(&mut self, _: &mut Fx, scene: &Scene, _: &mut dyn Assets) {
        for (theta, face) in &mut self.coins {
            *theta += 0.015;
            *face = Self::face(*theta, scene.longitude);
        }
        for theta in &mut self.lights {
            *theta += 0.015;
        }
    }

    fn draw<'a>(&'a self, fx: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        let axes = (seen.eye.ground_right(), [0.0, 1.0, 0.0]);
        for &(theta, face) in &self.coins {
            out.push(square(COIN_FACES[face], Self::centre(fx.position, theta), axes, 1.0, self.uv, [1.0; 4]));
        }
        for &theta in &self.lights {
            out.push(square(COIN_LIGHT, Self::centre(fx.position, theta), axes, 1.5, [0.0, 1.0, 0.0, 1.0], [1.0, 1.0, 1.0, 70.0 / 255.0]));
        }
    }

    fn alive(&self, fx: &Fx, scene: &Scene) -> bool {
        fx.owned(scene)
    }

    fn any(&mut self) -> Option<&mut dyn Any> {
        Some(self)
    }
}

// Fallen Angel (1140)

/// Fallen Angel's feathers fluttering down round the Rebellion's new spot, drifting back toward where it jumped from.
#[derive(Default)]
struct Feathers {
    part: Option<StrPart>,
    from: Vector,
    /// The jump has landed: the picture hangs `FEATHERS_DROP` lower.
    landed: bool,
    over: bool,
}

/// How far under the feet the feathers' point sits after the jump: 10 + (camera distance − 230) / 16.
// ponytail: the game's default camera distance of 300; the client's own distance isn't mapped to the game's.
pub const FEATHERS_DROP: f32 = 10.0 + (300.0 - 230.0) / 16.0;

impl Feathers {
    /// The unit snapped to `cell` (its position there): the feathers' point moves to it.
    pub fn snap(&mut self, cell: Vector) {
        if let Some(part) = &mut self.part {
            part.point = cell;
            self.landed = true;
        }
    }

    /// The step offset (STR px) that draws the picture `FEATHERS_DROP` units under the point once landed, under a camera
    /// whose up axis is `up`: a shift on the screen, so the ground in front of a lowered point doesn't hide it.
    fn offset(&self, up: Vector) -> [f32; 2] {
        let drop = if self.landed { FEATHERS_DROP * up[1] / super::PIXEL } else { 0.0 };
        [super::STR_OFFSET[0], super::STR_OFFSET[1] + drop]
    }
}

impl Class for Feathers {
    fn begin(&mut self, fx: &mut Fx, assets: &mut dyn Assets) {
        let mut part = StrPart::new("RL_fallen_angel\\Penne", false, assets);
        part.point = fx.position;
        self.part = Some(part);
        self.from = fx.position;
    }

    fn update(&mut self, fx: &mut Fx, scene: &Scene, _: &mut dyn Assets) {
        let offset = self.offset(scene.up);
        let Some(part) = &mut self.part else { return };
        part.offset = offset;
        // The point drifts 0.09 a tick toward where the jump started, along the ground; not before the jump.
        let d = [self.from[0] - fx.position[0], 0.0, self.from[2] - fx.position[2]];
        let length = length(d);
        if length > 0.0 {
            part.point = add(part.point, scaled(d, 0.09 / length));
        }
        self.over = !part.step();
    }

    fn draw<'a>(&'a self, _: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        if let Some(part) = &self.part {
            part.faces(&seen.eye, out);
        }
    }

    fn alive(&self, _: &Fx, _: &Scene) -> bool {
        !self.over && self.part.as_ref().is_some_and(|part| part.loaded.is_some())
    }

    fn any(&mut self) -> Option<&mut dyn Any> {
        Some(self)
    }
}

/// Fallen Angel's cast on `owner` standing at `at`: the feathers, started again if they are falling already.
pub fn fallen_angel(fx: &mut WorldFx, owner: Option<Option<u32>>, at: Vector, assets: &mut dyn Assets) {
    fx.remove(owner, FALLEN_ANGEL);
    fx.launch(&Launch::at(FALLEN_ANGEL, owner, at), assets);
}

/// `owner` snapped to the cell at `cell`: its feathers, if falling, fall there.
pub fn snap(fx: &mut WorldFx, owner: Option<Option<u32>>, cell: Vector) {
    if let Some((_, feathers)) = fx.class_mut::<Feathers>(owner, &[FALLEN_ANGEL]) {
        feathers.snap(cell);
    }
}

// The Warlock's spheres (1262 – 1265)

/// The element spheres circling over the Warlock's head, one a slot, evenly spread in phase.
#[derive(Default)]
struct Spheres {
    balls: [Option<(u32, StrPart)>; 5],
    /// Each slot's status value.
    held: [i32; 5],
}

/// A sphere's STR.
fn sphere_file(effect: u32) -> &'static str {
    match effect {
        1262 => "tetravortex\\tetra_ball_01_fire\\tetra_01_fire",
        1263 => "tetravortex\\tetra_ball_04_water\\tetra_04_water",
        1264 => "tetravortex\\tetra_ball_02_wind\\tetra_02_wind",
        _ => "tetravortex\\tetra_ball_03_earth\\tetra_03_earth",
    }
}

impl Spheres {
    fn ball(effect: u32, assets: &mut dyn Assets) -> StrPart {
        let mut part = StrPart::new(sphere_file(effect), false, assets);
        part.timeline.looped = true;
        part.offset = [0.0; 2];
        part
    }

    fn count(&self) -> usize {
        self.balls.iter().flatten().count()
    }

    /// With two or more balls, each in slot i starts again, `i / n` of its loop on.
    fn rebalance(&mut self) {
        let n = self.count();
        if n < 2 {
            return;
        }
        for (i, ball) in self.balls.iter_mut().enumerate() {
            let Some((_, part)) = ball else { continue };
            part.timeline = Timeline::default();
            part.timeline.looped = true;
            let frames = part.loaded.as_ref().map_or(0, |loaded| loaded.animation.last_key() + 1);
            for _ in 0..(i as f32 / n as f32 * frames as f32) as u32 {
                part.step();
            }
        }
    }
}

impl Class for Spheres {
    fn begin(&mut self, fx: &mut Fx, assets: &mut dyn Assets) {
        fx.single = true;
        self.balls[0] = Some((fx.id, Self::ball(fx.id, assets)));
    }

    fn update(&mut self, fx: &mut Fx, _: &Scene, _: &mut dyn Assets) {
        for (_, part) in self.balls.iter_mut().flatten() {
            part.point = fx.position;
            part.step();
        }
    }

    fn draw<'a>(&'a self, _: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        for (_, part) in self.balls.iter().flatten() {
            part.faces(&seen.eye, out);
        }
    }

    fn alive(&self, fx: &Fx, scene: &Scene) -> bool {
        fx.owned(scene) && self.count() > 0
    }

    fn any(&mut self) -> Option<&mut dyn Any> {
        Some(self)
    }
}

/// A sphere status of `slot` turning on (with its value) or off on `owner`.
pub fn sphere(fx: &mut WorldFx, owner: Option<Option<u32>>, slot: usize, on: bool, value: i32, assets: &mut dyn Assets) {
    if !on {
        let Some((_, spheres)) = fx.class_mut::<Spheres>(owner, &SPHERES) else { return };
        if spheres.count() > 1 {
            spheres.balls[slot] = None;
            spheres.held[slot] = 0;
            spheres.rebalance();
        } else {
            for id in SPHERES {
                fx.remove(owner, id);
            }
        }
        return;
    }
    let effect = sphere_effect(value);
    match fx.class_mut::<Spheres>(owner, &SPHERES) {
        Some((_, spheres)) => {
            if value == 0 || spheres.held[slot] == value {
                return;
            }
            spheres.balls[slot] = Some((effect, Spheres::ball(effect, assets)));
            spheres.held[slot] = value;
            spheres.rebalance();
        }
        None if value != 0 => {
            fx.launch(&Launch::at(effect, owner, [0.0; 3]), assets);
            if let Some((_, spheres)) = fx.class_mut::<Spheres>(owner, &SPHERES) {
                spheres.balls.swap(0, slot);
                spheres.held[slot] = value;
            }
        }
        None => {}
    }
}

// Triangle Shot (1324) and Cart Cannon (1357)

/// An arrow or a cannonball flying a straight line 10 units up from the start to the aim, after its start delay.
#[derive(Default)]
struct Line {
    part: Option<StrPart>,
    /// The first point and the step a tick.
    start: Vector,
    step: Vector,
    n: u32,
    /// It moves while n is at most this.
    last: f32,
    /// The screen turn's offset from the camera-relative heading: 360 or 270.
    base: f32,
    /// The line across the ground.
    across: (f32, f32),
    turned: bool,
}

impl Line {
    /// The step a tick and the last n for a line `toward` long, in `steps` steps: 5 units a tick at least.
    fn steps(toward: Vector, steps: f32) -> (Vector, f32) {
        let l = length(toward);
        if l == 0.0 {
            return ([0.0; 3], 0.0);
        }
        let f = if l < 40.0 { 40.0 / l } else { 1.0 };
        (scaled(toward, f / steps), steps / f)
    }

    /// The screen turn for a line along (dx, dz) under a camera at `longitude`.
    fn turn(base: f32, (dx, dz): (f32, f32), longitude: f32) -> f32 {
        let mut a = longitude + dz.atan2(dx).to_degrees();
        if a < 0.0 {
            a += 360.0;
        }
        let turn = base - a;
        if turn < 0.0 { turn + 360.0 } else { turn }
    }

    fn file(effect: u32, min: bool) -> &'static str {
        match (effect, min) {
            (TRIANGLE_SHOT, false) => "new_triangleshot\\new_triangleshot_arrow\\new_triangleshot_arrow",
            (TRIANGLE_SHOT, true) => "mineffect\\new_triangleshot\\new_triangleshot_arrow\\new_triangleshot_arrow",
            (_, false) => "new_cart_cannon\\new_cart_cannon\\new_cart_cannon",
            (_, true) => "mineffect\\new_cart_cannon\\new_cart_cannon\\new_cart_cannon",
        }
    }
}

impl Class for Line {
    fn begin(&mut self, fx: &mut Fx, assets: &mut dyn Assets) {
        let mut part = StrPart::new(Self::file(fx.id, fx.min), false, assets);
        part.timeline.looped = true;
        part.timeline.delay = fx.delay;
        part.offset = [0.0; 2];
        let lift = |at: Vector| [at[0], at[1] + 10.0, at[2]];
        let (from, to) = (lift(fx.position), lift(fx.aim));
        let toward = sub(to, from);
        let l = length(toward);
        (self.step, self.last) = Self::steps(toward, if fx.id == TRIANGLE_SHOT { 8.0 } else { 10.0 });
        self.start = if l > 0.0 { add(from, scaled(toward, 4.0 / l)) } else { from };
        self.base = if fx.id == TRIANGLE_SHOT { 360.0 } else { 270.0 };
        // Cart Cannon's sound comes at once, at the listener.
        if fx.id == CART_CANNON {
            fx.events.push((0, super::Event::Sound { file: CART_CANNON_SOUND.to_owned(), listener: true }));
        }
        self.across = (toward[0], toward[2]);
        part.point = self.start;
        self.part = Some(part);
    }

    fn update(&mut self, _: &mut Fx, scene: &Scene, _: &mut dyn Assets) {
        let Some(part) = &mut self.part else { return };
        if !self.turned {
            part.controls.turn = Self::turn(self.base, self.across, scene.longitude);
            self.turned = true;
        }
        // The start delay holds the ball and its flight.
        if part.timeline.delay > 0 {
            part.step();
            return;
        }
        part.point = add(self.start, scaled(self.step, self.n as f32));
        part.step();
        self.n += 1;
    }

    fn draw<'a>(&'a self, _: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        if let Some(part) = &self.part {
            part.faces(&seen.eye, out);
        }
    }

    fn alive(&self, _: &Fx, _: &Scene) -> bool {
        self.part.as_ref().is_some_and(|part| part.loaded.is_some()) && self.n as f32 <= self.last
    }

    fn any(&mut self) -> Option<&mut dyn Any> {
        Some(self)
    }
}

// Axe Boomerang (1657) and Shield Shooting (2183)

/// A spinning axe or a shield flying out past the target on one curve and back to the caster on the other, at the feet.
struct Boomerang {
    part: Option<StrPart>,
    t: f32,
    back: bool,
    over: bool,
}

impl Default for Boomerang {
    fn default() -> Self {
        Self { part: None, t: 1.0, back: false, over: false }
    }
}

impl Boomerang {
    /// The point at `t` on the curve from the target `to` (t = 0) to the caster `from` (t = 1), bent through the target
    /// pushed `k` units on along the line and turned `degrees` about the vertical.
    // ponytail: the turn's sign is unchecked: look at one throw.
    pub fn point(from: Vector, to: Vector, t: f32, degrees: f32, k: f32) -> Vector {
        let along = [to[0] - from[0], 0.0, to[2] - from[2]];
        let l = length(along);
        let u = if l > 0.0 { scaled(along, k / l) } else { [0.0; 3] };
        let (sin, cos) = degrees.to_radians().sin_cos();
        let q = [to[0] + u[0] * cos - u[2] * sin, to[1], to[2] + u[0] * sin + u[2] * cos];
        let s = 1.0 - t;
        std::array::from_fn(|axis| s * s * to[axis] + 2.0 * t * s * q[axis] + t * t * from[axis])
    }

    fn file(effect: u32, min: bool) -> String {
        match (effect, min) {
            (AXE_BOOMERANG, false) => "new_axeboomerang\\new_axeboomerang\\new_axeboomerang".to_owned(),
            (AXE_BOOMERANG, true) => "mineffect\\new_axeboomerang\\new_axeboomerang\\new_axeboomerang".to_owned(),
            (_, min) => {
                let file = "shield_shooting\\shield_shooting\\shield_shooting";
                if min { super::min_name(file) } else { file.to_owned() }
            }
        }
    }
}

impl Class for Boomerang {
    fn begin(&mut self, fx: &mut Fx, assets: &mut dyn Assets) {
        let mut part = StrPart::new(&Self::file(fx.id, fx.min), false, assets);
        part.timeline.looped = true;
        part.offset = [0.0; 2];
        part.point = fx.position;
        self.part = Some(part);
    }

    fn update(&mut self, fx: &mut Fx, scene: &Scene, _: &mut dyn Assets) {
        fx.aim_at(scene);
        let (degrees, k) = if self.back {
            self.t += 0.02;
            (260.0, 40.0)
        } else {
            self.t -= 0.05;
            if self.t <= 0.0 {
                self.back = true;
            }
            (100.0, 20.0)
        };
        let Some(part) = &mut self.part else { return };
        part.point = Self::point(fx.position, fx.aim, self.t, degrees, k);
        part.step();
        // Back at the caster: this update is its last drawn.
        self.over = self.back && self.t >= 1.0;
    }

    fn draw<'a>(&'a self, _: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        if let Some(part) = &self.part {
            part.faces(&seen.eye, out);
        }
    }

    fn alive(&self, fx: &Fx, scene: &Scene) -> bool {
        !self.over && fx.owned(scene) && self.part.as_ref().is_some_and(|part| part.loaded.is_some())
    }

    fn any(&mut self) -> Option<&mut dyn Any> {
        Some(self)
    }
}

/// A shot's launch: `effect` from `from` (owned by `owner`) at `target` standing at `aim`.
pub fn shot(effect: u32, owner: Option<Option<u32>>, from: Vector, (target, aim): (Option<u32>, Vector), min: bool, delay: i32) -> Launch {
    Launch { target: Some(target), aim, min, delay, ..Launch::at(effect, owner, from) }
}

#[cfg(test)]
mod tests {
    use super::super::Eye;
    use super::*;
    use crate::actor::Placed;
    use crate::effects::Loaded;
    use rustnarok_client::effect::{Key, Layer, Str};
    use std::rc::Rc;

    const OWNER: u32 = 0x800;
    const TARGET: u32 = 0x900;
    const FEET: Vector = [30.0, 4.0, 50.0];

    /// Sprites of one 32 px layer; STRs of one layer with keys at 0 and their last frame (Penne 160 frames, the spheres
    /// 72, the rest 50), the files asked for kept.
    #[derive(Default)]
    struct Fake {
        strs: Vec<String>,
    }

    impl Assets for Fake {
        fn texture(&mut self, _: &str) -> Option<[f32; 2]> {
            Some([18.0; 2])
        }
        fn str(&mut self, file: &str) -> Option<Rc<Loaded>> {
            self.strs.push(file.to_owned());
            let frames = if file.ends_with("Penne") {
                160
            } else if file.starts_with("tetravortex") {
                72
            } else {
                50
            };
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
        fn sprite(&mut self, _: &str, _: usize, frame: usize) -> Option<(usize, Vec<Placed>)> {
            Some((1, vec![((0, frame as u32, 0, [255; 4], 0), [-16.0, -16.0], [32.0, 32.0], 0.0)]))
        }
    }

    /// Runs `f` on a scene with the units in `here` standing at their feet (None = the player, at the origin).
    fn scene<R>(here: &[(u32, Vector)], longitude: f32, f: impl FnOnce(&Scene) -> R) -> R {
        let feet = |unit: Option<u32>| match unit {
            Some(id) => here.iter().find(|(each, _)| *each == id).map(|(_, at)| *at),
            None => Some([0.0; 3]),
        };
        let body = |_: Option<u32>| None;
        let eye = Eye::at(45.0, longitude);
        f(&Scene { player: FEET, feet: &feet, body: &body, longitude, right: eye.right, up: eye.up, ground: &|_, _| 0.0 })
    }

    fn seen<'a>() -> Seen<'a> {
        Seen { eye: Eye::at(45.0, 0.0), shows: &|_| true, hidden: &|_| false, size: &|_| 1.0, ground: &|_, _| 0.0, dim: false }
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
            scene(here, 0.0, |scene| {
                if let Some(feet) = self.fx.owner.and_then(|owner| (scene.feet)(owner)) {
                    self.fx.position = feet;
                }
                self.class.update(&mut self.fx, scene, assets);
            });
            self.fx.updated = true;
            self.fx.age += 1;
        }

        fn alive(&self, here: &[(u32, Vector)]) -> bool {
            scene(here, 0.0, |scene| self.class.alive(&self.fx, scene))
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

        fn faces(&self) -> Vec<Face<'_>> {
            let mut out = Vec::new();
            self.class.draw(&self.fx, &seen(), &mut out);
            out
        }
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    fn close3(a: Vector, b: Vector) -> bool {
        (0..3).all(|axis| close(a[axis], b[axis]))
    }

    fn top(n: u32) -> u32 {
        n.saturating_sub(1)
    }

    const HERE: [(u32, Vector); 2] = [(OWNER, FEET), (TARGET, [130.0, 4.0, 50.0])];

    #[test]
    fn flower_leafs_motes_draw_their_numbers_in_order() {
        let mut asked = Vec::new();
        let motes: Vec<Mote> = (0..16)
            .map(|_| {
                Mote::new(&mut |n| {
                    asked.push(n);
                    n - 1
                })
            })
            .collect();
        assert_eq!(asked, [100, 360, 360, 360, 360].repeat(16));
        assert_eq!((motes[0].y, motes[0].a, motes[0].t), (-99.0, [359, 359], [359, 359]));
        let solo = Solo::new(Motes::new(top), Launch::at(FLOWER_LEAF, Some(Some(OWNER)), FEET), &mut Fake::default());
        assert_eq!(solo.class.motes.len(), 16);
        assert!(solo.alive(&HERE) && !solo.alive(&[]));
    }

    #[test]
    fn a_flower_leaf_mote_waits_rises_fades_and_starts_again() {
        let mut rand = |n: u32| n / 2;
        let mut mote = Mote { y: -30.0, a: [10, 10], t: [100, 100], off: [0.0; 2], alpha: 0, shown: false };
        let mut waited = 0;
        while !mote.shown {
            mote.update(&mut rand);
            waited += 1;
        }
        // Up from −30 at 0.15 a tick: 200 updates, and one more where it is 0 give or take the float's rounding.
        assert!((200..=201).contains(&(waited - 1)), "{waited}");
        let mut alphas = vec![mote.alpha];
        while mote.shown {
            mote.update(&mut rand);
            if mote.shown {
                alphas.push(mote.alpha);
            }
        }
        // Full until 20 up, then 4 less a tick up to 30.
        let full = alphas.iter().filter(|&&alpha| alpha == 255).count();
        assert!((132..=134).contains(&full), "{full}");
        assert!(alphas.windows(2).skip(full).all(|pair| pair[1] == (pair[0] - 4).max(0)));
        // Past 30 it takes a new wait under the feet.
        mote.update(&mut rand);
        assert!(mote.y <= 0.15 && !mote.shown && mote.alpha == 0 && mote.off == [0.0; 2]);
    }

    #[test]
    fn the_angles_chase() {
        let mut rand = |n: u32| n - 1;
        assert_eq!(chase(10, 100, &mut rand), (13, 100));
        assert_eq!(chase(10, 5, &mut rand), (8, 5));
        // Within 3: a new angle to chase, below the old one going up, 359 less it going down.
        assert_eq!(chase(10, 11, &mut rand), (13, 9));
        assert_eq!(chase(10, 9, &mut rand), (8, 350));
        assert_eq!(chase(0, 1, &mut rand), (3, 0));
    }

    #[test]
    fn a_shown_mote_draws_twice() {
        let mut solo = Solo::new(Motes::new(top), Launch::at(FLOWER_LEAF, Some(Some(OWNER)), FEET), &mut Fake::default());
        for mote in &mut solo.class.motes {
            mote.y = -1.0;
        }
        solo.class.motes[0].y = 5.0;
        solo.tick(&HERE, &mut Fake::default());
        let faces = solo.faces();
        assert_eq!(faces.len(), 2);
        assert!(faces.iter().all(|face| matches!(face.texture, Tex::Path(CHERRY)) && face.additive && face.vertices[0].tint[3] == 1.0));
    }

    #[test]
    fn frigg_songs_nine_columns() {
        let points = frigg_points([100.0, 0.0, 200.0], 1);
        let offsets: Vec<[f32; 2]> = points.iter().map(|at| [at[0] - 100.0, at[2] - 200.0]).collect();
        assert_eq!(offsets, [[-15.0, 15.0], [0.0, 15.0], [15.0, 15.0], [-15.0, 0.0], [0.0, 0.0], [15.0, 0.0], [-15.0, -15.0], [0.0, -15.0], [15.0, -15.0]]);
        assert_eq!(frigg_points([0.0; 3], 5)[0], [-35.0, 0.0, 35.0]);
    }

    #[test]
    fn a_frigg_column_flashes_shoots_up_and_sinks() {
        let mut sink = None;
        let walls: Vec<f32> = (0..300).map(|n| Frigg::wall_at(n, &mut sink)).collect();
        for (n, wall) in [(0, 1.32), (19, 1.32), (20, 0.0), (110, 158.4), (140, 137.17), (141, 156.16)] {
            assert!((walls[n] - wall).abs() < 0.01, "{n}: {}", walls[n]);
        }
        assert!(walls[262] < 1.0 && walls[299] == 0.0);
        let mut assets = Fake::default();
        let mut solo = Solo::new(Frigg::default(), Launch::at(FRIGG, None, FEET), &mut assets);
        assert_eq!(solo.fx.length, 350);
        assert_eq!(solo.fx.events, [(0, super::super::Event::Sound { file: "effect\\wm_frigg_song.wav".into(), listener: false })]);
        solo.tick(&HERE, &mut assets);
        let faces = solo.faces();
        assert_eq!(faces.len(), 20);
        let tint = faces[0].vertices[0].tint;
        assert!(close(tint[1], 200.0 / 255.0) && close(tint[3], 50.0 / 255.0));
        // The first side: 1.32 high, radius 2.5.
        let [top_left, .., foot_left] = [0, 1, 2].map(|corner| faces[0].vertices[corner].position);
        assert!(close(top_left[1] - foot_left[1], 1.32));
        assert!(close((foot_left[0] - FEET[0]).hypot(foot_left[2] - FEET[2]), 2.5));
    }

    #[test]
    fn bind_traps_circle_by_level() {
        let ids: Vec<Option<u32>> = (1..=6).map(|level| unit_effect(RUNES, level)).collect();
        assert_eq!(ids, [Some(1069), Some(1070), Some(1070), Some(1071), Some(1071), None]);
        assert_eq!(unit_effect(CLOUD_KILL, 0), Some(CLOUD_KILL));
        for (id, across) in [(1069, 18.0), (1070, 30.0), (1071, 42.0)] {
            let mut assets = Fake::default();
            let mut solo = Solo::new(Runes::default(), Launch::at(id, Some(Some(7)), FEET), &mut assets);
            solo.tick(&HERE, &mut assets);
            let faces = solo.faces();
            let xs: Vec<f32> = faces[0].vertices.iter().map(|vertex| vertex.position[0]).collect();
            let span = xs.iter().copied().fold(f32::MIN, f32::max) - xs.iter().copied().fold(f32::MAX, f32::min);
            assert!(close(span, across));
            assert!(faces[0].vertices.iter().all(|vertex| close(vertex.position[1], FEET[1] + 1.0)));
            assert!(solo.alive(&HERE));
            solo.fx.released = true;
            assert!(!solo.alive(&HERE));
        }
    }

    #[test]
    fn cloud_kills_puffs() {
        let mut assets = Fake::default();
        let mut solo = Solo::new(Haze::new(top), Launch::at(CLOUD_KILL, Some(Some(7)), FEET), &mut assets);
        let mut centre = Vec::new();
        let mut grids = Vec::new();
        for e in 0..20 {
            solo.tick(&HERE, &mut assets);
            // A new A puff is the one at age 1, a new grid's at age 1.
            if solo.class.centre.iter().any(|puff| puff.age == 1) {
                centre.push(e);
            }
            if solo.class.grid.iter().filter(|puff| puff.age == 1).count() == 36 {
                grids.push(e);
            }
        }
        assert_eq!(centre, [0, 5, 10, 15]);
        assert_eq!(grids, [0, 9, 18]);
        // An A puff from 70: 25.4 more a tick to 254, then out over its last 2.
        let mut a = Puff::centre(&mut top);
        let mut a_alphas = Vec::new();
        for _ in 0..10 {
            a.step();
            a_alphas.push(a.alpha);
        }
        assert_eq!(a_alphas, [95.0, 120.0, 145.0, 170.0, 195.0, 220.0, 245.0, 254.0, 127.0, 0.0]);
        // A grid puff: in to 254 by age 10, out over its last 8.
        let mut b = Puff::grid(&mut top).remove(0);
        let alphas: Vec<f32> = (0..40)
            .map(|_| {
                b.step();
                b.alpha
            })
            .collect();
        assert_eq!((alphas[9], alphas[10], alphas[31], alphas[32], alphas[39]), (250.0, 254.0, 254.0, 222.0, 0.0));
        // The unit gone: no new puffs, and the end 50 ticks on.
        solo.fx.released = true;
        let e = solo.fx.age;
        solo.tick(&HERE, &mut assets);
        assert_eq!(solo.fx.length, e + 50);
        for _ in 0..60 {
            solo.tick(&HERE, &mut assets);
        }
        assert!(solo.class.centre.is_empty() && solo.class.grid.is_empty() && !solo.alive(&HERE));
    }

    #[test]
    fn a_cloud_kill_puff_draws_in_order() {
        let mut asked = Vec::new();
        let puff = Puff::centre(&mut |n| {
            asked.push(n);
            n - 1
        });
        assert_eq!(asked, [360, 20]);
        assert!(close(puff.speed, 39.0 / 80.0) && close(puff.decel, -39.0 / 1600.0) && puff.alpha == 70.0);
        asked.clear();
        let grid = Puff::grid(&mut |n| {
            asked.push(n);
            0
        });
        assert_eq!(asked, [360, 6, 30, 50].repeat(36));
        assert_eq!(grid.len(), 36);
        // θ = 0, k = 2: 2 units along z from (−10, −10).
        assert_eq!(grid[0].offset, [-10.0, 0.0, -8.0]);
        assert_eq!(grid[1].offset, [-6.0, 0.0, -8.0]);
        assert!(close(grid[0].card.scale, 40.0 / 170.0));
    }

    #[test]
    fn the_rebellions_coins() {
        let (coins, lights) = Coins::angles(3);
        let start: Vec<f32> = coins.iter().map(|theta| (theta + 10.0).to_degrees()).collect();
        assert!(close(start[0], 0.0) && close(start[1], 120.0) && close(start[2], 240.0));
        assert!(lights.is_empty());
        let (coins, lights) = Coins::angles(7);
        assert_eq!((coins.len(), lights.len()), (5, 2));
        assert!(close((lights[1] + 10.0).to_degrees(), 72.0));
        assert_eq!(Coins::face(10.0_f32.to_radians(), 0.0), 0);
        assert_eq!(Coins::face(40.0_f32.to_radians(), 0.0), 1);
        assert_eq!(Coins::face(200.0_f32.to_radians(), 0.0), 0);
        assert_eq!(Coins::face(10.0_f32.to_radians(), 90.0), 2);
        assert!([4215, 4229, 4306].into_iter().all(object_coins) && ![24, 4228, 4016].into_iter().any(object_coins));
        let mut assets = Fake::default();
        let launch = Launch { a7: Some(7), ..Launch::at(COINS, Some(Some(OWNER)), FEET) };
        let mut solo = Solo::new(Coins::default(), launch, &mut assets);
        solo.tick(&HERE, &mut assets);
        assert_eq!(solo.faces().len(), 7);
        // 18 of a 32 px surface.
        assert_eq!(solo.class.uv, [0.0, 18.0 / 32.0, 0.0, 18.0 / 32.0]);
        // One turn in 419 ticks, 7 out, 10 up.
        let centre = Coins::centre(FEET, 0.0);
        assert_eq!(centre, [37.0, 14.0, 50.0]);
        assert!(close(solo.class.coins[0].0, -10.0 + 0.015));
        assert!(!solo.alive(&[]));
    }

    #[test]
    fn the_coin_packet_takes_the_old_ring_first() {
        let mut fx = WorldFx::default();
        let mut assets = Fake::default();
        let owner = Some(Some(OWNER));
        for count in [3, 7] {
            fx.remove(owner, COINS);
            fx.launch(&Launch { a7: Some(count), ..Launch::at(COINS, owner, FEET) }, &mut assets);
        }
        assert_eq!(fx.iter().count(), 1);
        assert_eq!(fx.iter().next().unwrap().fx.a7, 7);
    }

    #[test]
    fn fallen_angels_feathers() {
        let mut fx = WorldFx::default();
        let mut assets = Fake::default();
        let owner = Some(Some(OWNER));
        fallen_angel(&mut fx, owner, FEET, &mut assets);
        assert_eq!(assets.strs, ["RL_fallen_angel\\Penne"]);
        scene(&HERE, 0.0, |scene| fx.tick(scene, &mut assets));
        // A second cast starts them again.
        fallen_angel(&mut fx, owner, FEET, &mut assets);
        assert_eq!((fx.iter().count(), fx.iter().next().unwrap().fx.age), (1, 0));
        // The snap: the point at the new cell, drawn 14.4 lower; then 0.09 a tick toward where the jump began.
        let landed = [50.0, 6.0, 50.0];
        let feathers = |fx: &mut WorldFx| {
            let part = fx.class_mut::<Feathers>(owner, &[FALLEN_ANGEL]).unwrap().1.part.as_ref().unwrap();
            (part.point, part.offset)
        };
        assert_eq!(feathers(&mut fx).1, super::super::STR_OFFSET);
        snap(&mut fx, owner, landed);
        assert!(close3(feathers(&mut fx).0, landed));
        scene(&[(OWNER, landed)], 0.0, |scene| fx.tick(scene, &mut assets));
        let (point, offset) = feathers(&mut fx);
        assert!(close3(point, [50.0 - 0.09, 6.0, 50.0]));
        // 14.375 units straight down at 45 degrees: 14.375 · cos 45° · 7 px lower on the screen.
        assert!(close(offset[1], -80.0 + 14.375 * 45.0_f32.to_radians().cos() * 7.0));
        // Standing where it began: no drift.
        let mut solo = Solo::new(Feathers::default(), Launch::at(FALLEN_ANGEL, owner, FEET), &mut assets);
        solo.tick(&HERE, &mut assets);
        assert_eq!(solo.class.part.as_ref().unwrap().point, FEET);
        // The STR's 160 frames, then it ends.
        assert_eq!(1 + solo.run(&HERE, &mut assets, 500), 161);
    }

    #[test]
    fn the_spheres_by_value() {
        assert_eq!([0x44, 0x45, 0x46, 0x47].map(sphere_effect), [1262, 1264, 1263, 1265]);
        assert_eq!((sphere_slot(362), sphere_slot(363), sphere_slot(367), sphere_slot(368)), (None, Some(0), Some(4), None));
        for id in SPHERES {
            assert!(crate::effects::force_drawn(id));
        }
    }

    #[test]
    fn spheres_launch_add_rebalance_and_go() {
        let mut fx = WorldFx::default();
        let mut assets = Fake::default();
        let owner = Some(Some(OWNER));
        // The first launches the object, its ball in the status's slot.
        sphere(&mut fx, owner, 1, true, 0x44, &mut assets);
        assert_eq!(fx.iter().map(|object| object.fx.id).collect::<Vec<_>>(), [1262]);
        let balls = |fx: &mut WorldFx| fx.class_mut::<Spheres>(owner, &SPHERES).unwrap().1.balls.each_ref().map(|ball| ball.as_ref().map(|(id, _)| *id));
        assert_eq!(balls(&mut fx), [None, Some(1262), None, None, None]);
        // The next adds a ball; the same value again changes nothing.
        sphere(&mut fx, owner, 4, true, 0x46, &mut assets);
        sphere(&mut fx, owner, 4, true, 0x46, &mut assets);
        assert_eq!(fx.iter().count(), 1);
        assert_eq!(balls(&mut fx), [None, Some(1262), None, None, Some(1263)]);
        // Rebalanced: slot i is i / n of the 72-frame loop on (slot 4: twice round).
        let frames = |fx: &mut WorldFx| {
            fx.class_mut::<Spheres>(owner, &SPHERES).unwrap().1.balls.each_ref().map(|ball| ball.as_ref().map(|(_, part)| part.timeline.frame))
        };
        assert_eq!(frames(&mut fx), [None, Some(36), None, None, Some(72)]);
        // Off with two balls takes one; off with one takes the object.
        sphere(&mut fx, owner, 1, false, 0, &mut assets);
        assert_eq!(balls(&mut fx), [None, None, None, None, Some(1263)]);
        sphere(&mut fx, owner, 4, false, 0, &mut assets);
        assert!(fx.is_empty());
        // A zero value starts nothing.
        sphere(&mut fx, owner, 0, true, 0, &mut assets);
        assert!(fx.is_empty());
    }

    #[test]
    fn the_shot_lines() {
        let mut assets = Fake::default();
        let far = [130.0, 4.0, 50.0];
        // L = 100: an eighth a tick and 9 updates; a tenth and 11.
        for (id, step, updates) in [(TRIANGLE_SHOT, 12.5, 9), (CART_CANNON, 10.0, 11)] {
            let mut solo = Solo::new(Line::default(), shot(id, None, FEET, (Some(TARGET), far), false, 0), &mut assets);
            assert!(close3(solo.class.start, [34.0, 14.0, 50.0]));
            assert!(close(solo.class.step[0], step));
            assert_eq!(solo.run(&HERE, &mut assets, 100), updates);
        }
        // L = 20: 5 units a tick, 20 / 5 + 1 updates.
        let near = [50.0, 4.0, 50.0];
        let mut solo = Solo::new(Line::default(), shot(TRIANGLE_SHOT, None, FEET, (Some(TARGET), near), false, 0), &mut assets);
        assert!(close(solo.class.step[0], 5.0));
        assert_eq!(solo.run(&HERE, &mut assets, 100), 5);
        // Cart Cannon's 200 ms: 13 ticks held, then its 11.
        let mut solo = Solo::new(Line::default(), shot(CART_CANNON, Some(Some(TARGET)), FEET, (Some(TARGET), far), false, 200), &mut assets);
        assert_eq!(solo.run(&HERE, &mut assets, 100), 13 + 11);
        assert_eq!(solo.fx.events, [(0, super::super::Event::Sound { file: CART_CANNON_SOUND.into(), listener: true })]);
    }

    #[test]
    fn the_shot_turns_and_files() {
        // Along +x at longitude 0: a = 0.
        assert!(close(Line::turn(360.0, (1.0, 0.0), 0.0), 0.0) || close(Line::turn(360.0, (1.0, 0.0), 0.0), 360.0));
        assert!(close(Line::turn(270.0, (1.0, 0.0), 0.0), 270.0));
        // Along +z at longitude 30: a = 120.
        assert!(close(Line::turn(360.0, (0.0, 1.0), 30.0), 240.0));
        assert!(close(Line::turn(270.0, (0.0, 1.0), 30.0), 150.0));
        // Along −z: a = −90 + 360 = 270.
        assert!(close(Line::turn(270.0, (0.0, -1.0), 0.0), 0.0));
        let mut assets = Fake::default();
        for (id, min) in [(TRIANGLE_SHOT, false), (TRIANGLE_SHOT, true), (CART_CANNON, false), (CART_CANNON, true)] {
            Solo::new(Line::default(), shot(id, None, FEET, (Some(TARGET), FEET), min, 0), &mut assets);
        }
        assert_eq!(
            assets.strs,
            [
                "new_triangleshot\\new_triangleshot_arrow\\new_triangleshot_arrow",
                "mineffect\\new_triangleshot\\new_triangleshot_arrow\\new_triangleshot_arrow",
                "new_cart_cannon\\new_cart_cannon\\new_cart_cannon",
                "mineffect\\new_cart_cannon\\new_cart_cannon\\new_cart_cannon",
            ]
        );
    }

    #[test]
    fn the_boomerangs_curve_out_and_back() {
        let mut assets = Fake::default();
        let target = [130.0, 4.0, 50.0];
        let mut solo = Solo::new(Boomerang::default(), shot(AXE_BOOMERANG, Some(Some(OWNER)), FEET, (Some(TARGET), target), false, 0), &mut assets);
        solo.tick(&HERE, &mut assets);
        // The first point at t = 0.95.
        assert!(close3(solo.class.part.as_ref().unwrap().point, Boomerang::point(FEET, target, 0.95, 100.0, 20.0)));
        let mut outbound = 1;
        while !solo.class.back {
            solo.tick(&HERE, &mut assets);
            outbound += 1;
        }
        assert_eq!(outbound, 20);
        let back = solo.run(&HERE, &mut assets, 200);
        assert_eq!((back, outbound + back), (51, 71));
        // Its ends: the caster at t = 1, the target at t = 0.
        assert!(close3(Boomerang::point(FEET, target, 1.0, 100.0, 20.0), FEET));
        assert!(close3(Boomerang::point(FEET, target, 0.0, 260.0, 40.0), target));
        // The caster gone ends it.
        let solo = Solo::new(Boomerang::default(), shot(SHIELD_SHOOTING, Some(Some(OWNER)), FEET, (Some(TARGET), target), true, 0), &mut assets);
        assert!(solo.alive(&HERE) && !solo.alive(&[(TARGET, target)]));
        assert_eq!(assets.strs.last().unwrap(), "shield_shooting\\shield_shooting\\min_shield_shooting");
    }
}
