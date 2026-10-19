//! The sky in the world (`MapSkyData`, and the clouds, clouds2 and fog weather): puffs of cloud, smog or stars round
//! the player, and cloud fields over the whole map, each fading in, living a while, fading out and starting again
//! somewhere new. A tick is 16 ms.

use rustnarok_client::grf::Grf;
use rustnarok_client::lua::{CloudField, SkyEffects};

use crate::render::{Gpu, Texture};
use crate::world::{MeshVertex, Vector};

const TICK_MS: u64 = 16;
/// At most this many ticks are caught up in one frame.
const MOST_TICKS: u64 = 64;

/// The sky types an effect id builds (on a map's load or played on the player): its effect, its sky type.
const SKY_EFFECTS: [(u32, u32); 12] =
    [(229, 0), (230, 1), (233, 2), (515, 3), (516, 4), (592, 5), (697, 7), (698, 8), (945, 9), (1045, 10), (1115, 11), (1145, 15)];

/// The sky type an effect id builds, if it builds one.
pub fn sky_type_of(effect: u32) -> Option<u32> {
    SKY_EFFECTS.iter().find(|(id, _)| *id == effect).map(|(_, sky)| *sky)
}

/// A sky type's puffs: each puff type and how many. Types 6 and 12–14 show nothing.
pub fn sky_puffs(sky: u32) -> Vec<(u32, u32)> {
    match sky {
        0 => vec![(0, 160)],
        1 => vec![(1, 240)],
        2 => vec![(2, 160)],
        3 => vec![(3, 320)],
        4 => vec![(4, 320)],
        5 => vec![(5, 320)],
        7 => vec![(7, 320)],
        8 => vec![(8, 320)],
        9 => vec![(9, 260)],
        10 => vec![(5, 120)],
        11 => vec![(11, 160), (12, 160), (13, 160), (14, 160)],
        15 => vec![(15, 260)],
        _ => Vec::new(),
    }
}

/// Which pictures a puff takes: clouds (`cloud4`, `cloud1`, `cloud2`), fog (`fog1`–`fog3`), twinkling stars
/// (`star01`–`star06`, a step every 14 ticks) or the first star only.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Look {
    Clouds,
    Fog,
    Stars,
    Star,
    /// A cloud field's: `cloud1`–`cloud4`.
    Field,
}

/// A puff type: its height (GAT units, larger is lower) from `height` + rand % 10, or 20 + rand % 5 above the ground
/// under it; square or ring place; fade-in `step` a tick for `fade` ticks up to `peak`; colour, additive; drift k
/// (x always east by 0.2·|sin a| for type 4); size `size` + rand % `spread`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Kind {
    pub height: f32,
    pub ground: bool,
    pub ring: bool,
    pub step: f32,
    pub fade: u32,
    pub peak: f32,
    pub color: [u8; 3],
    pub additive: bool,
    pub drift: f32,
    pub east: bool,
    pub size: (f32, u32),
    pub look: Look,
    /// Type 0: it fades in only while the player stands higher than −152 (GAT); lower, it starts at age 300.
    pub high_only: bool,
}

pub fn kind(puff: u32) -> Option<Kind> {
    let base = Kind {
        height: 40.0,
        ground: false,
        ring: true,
        step: 3.0,
        fade: 80,
        peak: 240.0,
        color: [255; 3],
        additive: false,
        drift: 0.05,
        east: false,
        size: (30.0, 20),
        look: Look::Clouds,
        high_only: false,
    };
    let square = Kind { ring: false, step: 2.0, peak: 160.0, ..base };
    Some(match puff {
        0 => Kind { height: -125.0, high_only: true, ..square },
        1 => base,
        2 => Kind { height: 0.0, ..square },
        3 => Kind { ground: true, step: 1.0, fade: 170, peak: 170.0, color: [0xFC, 0xAB, 0x8F], drift: 0.015, size: (35.0, 10), look: Look::Fog, ..square },
        4 => Kind { east: true, ..base },
        5 => Kind { height: 20.0, color: [0x5E, 0, 0], drift: 0.035, ..square },
        7 => Kind { color: [0, 0, 0], ..base },
        8 => Kind { color: [0xFF, 0xB4, 0xB4], ..base },
        9 => Kind { height: 80.0, additive: true, look: Look::Stars, ..base },
        11..=13 => Kind { step: 1.0, fade: 70, peak: 70.0, color: [[0x5E, 0, 0], [0, 0, 0], [0xFF, 0xB4, 0xB4]][puff as usize - 11], ..base },
        14 => Kind { step: 1.0, fade: 70, peak: 70.0, color: [0x3F, 0x48, 0xCC], additive: true, ..base },
        15 => Kind { height: -30.0, additive: true, look: Look::Star, ..base },
        _ => return None,
    })
}

/// The C library's `rand()` as the game uses it: a small, seeded generator, 0..32767.
pub struct Rng(u32);

impl Rng {
    pub fn new(seed: u32) -> Self {
        Self(seed.max(1))
    }

    pub fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(214_013).wrapping_add(2_531_011);
        (self.0 >> 16) & 0x7FFF
    }

    /// rand() % `n` (0 when `n` is 0).
    fn below(&mut self, n: u32) -> u32 {
        if n == 0 { 0 } else { self.next() % n }
    }
}

/// Where a puff starts, from the player's place on the ground plane: "square" x and z each −150…+149 of the
/// player's; "ring" x and z each `from`…`from` + `width` − 1 away on a random side, so none lies within `from` of
/// the player's x or z line.
pub fn place(rng: &mut Rng, player: [f32; 2], ring: Option<(u32, u32)>) -> [f32; 2] {
    match ring {
        None => [0, 1].map(|axis| player[axis] - 150.0 + rng.below(300) as f32),
        Some((from, width)) => [0, 1].map(|axis| {
            let away = (from + rng.below(width)) as f32;
            if rng.below(2) == 0 { player[axis] + away } else { player[axis] - away }
        }),
    }
}

/// A drifting angle: turns 1° a tick toward its target, and picks a new one (0–359) on getting there.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Drift {
    angle: f32,
    target: f32,
}

impl Drift {
    fn new(rng: &mut Rng) -> Self {
        Self { angle: rng.below(360) as f32, target: rng.below(360) as f32 }
    }

    fn tick(&mut self, rng: &mut Rng) {
        if self.angle == self.target {
            self.target = rng.below(360) as f32;
        }
        self.angle += (self.target - self.angle).clamp(-1.0, 1.0);
    }
}

/// One puff (or cloud of a field): where, how big, how old and how opaque.
#[derive(Clone, Debug, PartialEq)]
pub struct Puff {
    pub kind: Kind,
    /// Which of its look's pictures (shared by its group of 4).
    pub picture: usize,
    /// World place, y up.
    pub at: Vector,
    /// GAT height (larger is lower) kept across restarts; above the ground for a ground puff.
    height: f32,
    pub age: u32,
    pub alpha: f32,
    life: u32,
    drift: [Drift; 2],
    spin: f32,
    size: f32,
    /// A ring's distance (from, width), and the peak alpha (ecl_fild01's stars stop at 100).
    ring: Option<(u32, u32)>,
    peak: f32,
    /// A cloud field's: its entry, and the pulse.
    field: Option<CloudField>,
}

impl Puff {
    /// A puff of `kind` round `player` (world, y up): its height, place and size, at age 0 and alpha 0. Below −152 a
    /// type-0 puff starts at age 300, so it never shows.
    fn new(rng: &mut Rng, kind: Kind, picture: usize, player: Vector, ground: &impl Fn([f32; 2]) -> f32, ring: Option<(u32, u32)>) -> Self {
        let height = if kind.ground { (20 + rng.below(5)) as f32 } else { kind.height + rng.below(10) as f32 };
        let size = kind.size.0 + rng.below(kind.size.1) as f32;
        let mut puff = Self {
            kind,
            picture,
            at: [0.0; 3],
            height,
            age: 0,
            alpha: 0.0,
            life: 0,
            drift: [Drift::new(rng), Drift::new(rng)],
            spin: 0.0,
            size,
            ring,
            peak: kind.peak,
            field: None,
        };
        puff.start(rng, player, ground);
        puff
    }

    /// Starts again: age 0, alpha 0, a life of 300 + rand % 200 ticks, a new place round the player's current place, its
    /// height kept (a ground puff reads the ground again).
    fn start(&mut self, rng: &mut Rng, player: Vector, ground: &impl Fn([f32; 2]) -> f32) {
        let [x, z] = place(rng, [player[0], player[2]], self.ring.filter(|_| self.kind.ring));
        let height = if self.kind.ground { ground([x, z]) - self.height } else { self.height };
        self.at = [x, -height, z];
        self.age = if self.kind.high_only && -player[1] >= -152.0 { 300 } else { 0 };
        self.alpha = 0.0;
        self.life = 300 + rng.below(200);
    }

    /// One tick: fade in while young, out after its life, starting again at alpha 0; drift, spin.
    fn tick(&mut self, rng: &mut Rng, player: Vector, ground: &impl Fn([f32; 2]) -> f32) {
        if self.age >= self.life {
            self.alpha = (self.alpha - 1.0).max(0.0);
            if self.alpha <= 0.0 {
                self.start(rng, player, ground);
                return;
            }
        } else if self.age < self.kind.fade {
            self.alpha = (self.alpha + self.kind.step).min(self.peak);
        }
        self.age += 1;
        for drift in &mut self.drift {
            drift.tick(rng);
        }
        let [a, b] = self.drift.map(|drift| drift.angle.to_radians().sin());
        self.at[0] += if self.kind.east { 0.2 * a.abs() } else { self.kind.drift * a };
        self.at[2] += self.kind.drift * b;
        self.spin += 1.0;
    }

    /// A cloud field's tick: it gains `alpha_inc_speed` a tick while its age is below its fade-in time, loses
    /// `alpha_dec_speed` a tick from its fade-out time, and starts again elsewhere on the map at 0.
    fn tick_field(&mut self, rng: &mut Rng, map: [f32; 2]) {
        let Some(field) = self.field.clone() else { return };
        if self.age >= self.life {
            self.alpha -= field.alpha_dec_speed;
            if self.alpha <= 0.0 {
                *self = field_cloud(rng, &field, map, self.picture);
                return;
            }
        } else if self.age < self.kind.fade {
            self.alpha = (self.alpha + field.alpha_inc_speed).min(255.0);
        }
        self.age += 1;
        for drift in &mut self.drift {
            drift.tick(rng);
        }
        let [a, b] = self.drift.map(|drift| drift.angle.to_radians().sin());
        self.at[0] += 0.05 * a;
        self.at[2] += 0.05 * b;
        self.spin += 1.0;
    }

    /// Its size now: × (1 + rate · sin(spin)), 0.05 for puffs.
    fn size_now(&self) -> f32 {
        let rate = self.field.as_ref().map_or(0.05, |field| field.expand_rate);
        self.size * (1.0 + rate * self.spin.to_radians().sin())
    }

    /// The picture showing: twinkling stars step every 14 ticks.
    fn picture_now(&self) -> usize {
        if self.kind.look == Look::Stars { (self.picture + self.age as usize / 14) % 6 } else { self.picture }
    }
}

/// A cloud of a field, anywhere on the map (`map`: its width and height in world units): ±(rand % half) from the
/// centre each way, at `height` + rand % `height_extra`, fading in for `alpha_inc_time` + rand % `alpha_dec_time_extra`
/// ticks and out from `alpha_dec_time` + rand % `alpha_dec_time_extra`.
fn field_cloud(rng: &mut Rng, field: &CloudField, map: [f32; 2], picture: usize) -> Puff {
    let half = map.map(|size| (size / 2.0) as u32);
    let [x, z] = [0, 1].map(|axis| {
        let away = rng.below(half[axis]) as f32;
        let from_centre = if rng.below(2) == 0 { away } else { -away };
        map[axis] / 2.0 + from_centre
    });
    let height = field.height + rng.below(field.height_extra) as f32;
    let kind = Kind {
        height,
        ground: false,
        ring: false,
        step: field.alpha_inc_speed,
        fade: field.alpha_inc_time + rng.below(field.alpha_dec_time_extra),
        peak: 255.0,
        color: field.color,
        additive: false,
        drift: 0.05,
        east: false,
        size: (field.size, field.size_extra),
        look: Look::Field,
        high_only: false,
    };
    Puff {
        kind,
        picture,
        at: [x, -height, z],
        height,
        age: 0,
        alpha: 0.0,
        life: field.alpha_dec_time + rng.below(field.alpha_dec_time_extra),
        drift: [Drift::new(rng), Drift::new(rng)],
        spin: 0.0,
        size: field.size + rng.below(field.size_extra) as f32,
        ring: None,
        peak: 255.0,
        field: Some(field.clone()),
    }
}

/// How many pictures a look has to pick from.
fn pictures(look: Look) -> u32 {
    match look {
        Look::Clouds | Look::Fog => 3,
        Look::Stars => 6,
        Look::Star => 1,
        Look::Field => 4,
    }
}

/// A map's sky: every puff and cloud, all started together, until the next map load.
pub struct Sky {
    pub puffs: Vec<Puff>,
    rng: Rng,
    map: [f32; 2],
    last: Option<u64>,
}

impl Sky {
    /// The sky of a map, in its order: the stars, the cloud fields, then each sky type in the list's order. `map` is the
    /// map's size in world units; `name` its name (ecl_fild01's stars reach further and stay dimmer).
    pub fn new(effects: &SkyEffects, name: &str, map: [f32; 2], player: Vector, ground: &impl Fn([f32; 2]) -> f32, seed: u32) -> Self {
        let mut sky = Self { puffs: Vec::new(), rng: Rng::new(seed), map, last: None };
        if effects.stars {
            sky.add(9, name, player, ground);
        }
        for field in &effects.fields {
            for index in 0..field.num {
                let picture = if index % 4 == 0 { sky.rng.below(4) as usize } else { sky.puffs.last().map_or(0, |puff| puff.picture) };
                let cloud = field_cloud(&mut sky.rng, field, map, picture);
                sky.puffs.push(cloud);
            }
        }
        for &sky_type in &effects.types {
            sky.add(sky_type, name, player, ground);
        }
        sky
    }

    /// A sky type's puffs, in groups of 4 sharing a picture, round the player.
    pub fn add(&mut self, sky_type: u32, name: &str, player: Vector, ground: &impl Fn([f32; 2]) -> f32) {
        let far_stars = name.trim_end_matches(".gat").trim_end_matches(".rsw") == "ecl_fild01";
        for (puff_type, count) in sky_puffs(sky_type) {
            let Some(kind) = kind(puff_type) else { continue };
            let ring = Some(if far_stars && kind.look == Look::Stars { (25, 400) } else { (25, 200) });
            let mut picture = 0;
            for index in 0..count {
                if index % 4 == 0 {
                    picture = self.rng.below(pictures(kind.look)) as usize;
                }
                let mut puff = Puff::new(&mut self.rng, kind, picture, player, ground, ring);
                if far_stars && kind.look == Look::Stars {
                    puff.peak = 100.0;
                }
                self.puffs.push(puff);
            }
        }
    }

    /// Every 16 ms tick since the last frame (at most 64 caught up), with the player at `player` (world, y up).
    pub fn advance(&mut self, now_ms: u64, player: Vector, ground: &impl Fn([f32; 2]) -> f32) {
        let tick = now_ms / TICK_MS;
        let ticks = self.last.map_or(0, |last| tick.saturating_sub(last).min(MOST_TICKS));
        self.last = Some(tick);
        for _ in 0..ticks {
            for puff in &mut self.puffs {
                if puff.field.is_some() {
                    puff.tick_field(&mut self.rng, self.map);
                } else {
                    puff.tick(&mut self.rng, player, ground);
                }
            }
        }
    }

    /// The puffs to draw, as camera-facing squares of side size·√2 (`right` and `up` the camera's), each with its
    /// look's picture, colour × alpha, and whether it adds. A cloud field's beyond its `CullDist` from the player
    /// isn't drawn.
    // ponytail: a cloud field's cut is by its distance on the ground plane.
    pub fn quads(&self, right: Vector, up: Vector, player: Vector) -> Vec<(Look, usize, [MeshVertex; 6], bool)> {
        self.puffs
            .iter()
            .filter(|puff| puff.alpha > 0.0)
            .filter(|puff| puff.field.as_ref().is_none_or(|field| f32::hypot(puff.at[0] - player[0], puff.at[2] - player[2]) <= field.cull_dist))
            .map(|puff| {
                let half = puff.size_now() * std::f32::consts::SQRT_2 / 2.0;
                let corner = |x: f32, y: f32| -> Vector { [0, 1, 2].map(|axis| puff.at[axis] + right[axis] * x * half + up[axis] * y * half) };
                let color = puff.kind.color.map(|channel| f32::from(channel) / 255.0);
                let tint = [color[0], color[1], color[2], (puff.alpha / 255.0).min(1.0)];
                let vertex = |x: f32, y: f32, uv: [f32; 2]| MeshVertex { position: corner(x, y), uv, tint };
                let [a, b, c, d] = [vertex(-1.0, 1.0, [0.0, 0.0]), vertex(1.0, 1.0, [1.0, 0.0]), vertex(-1.0, -1.0, [0.0, 1.0]), vertex(1.0, -1.0, [1.0, 1.0])];
                (puff.kind.look, puff.picture_now(), [a, b, c, c, b, d], puff.kind.additive)
            })
            .collect()
    }
}

/// The sky of `map` when the player arrives on `cell`: its MapSkyData effects, round them.
pub fn for_map(grf: &Grf, map: &str, world: &crate::world::World, cell: [u16; 2]) -> Sky {
    let effects = rustnarok_client::lua::sky_effects(grf, map);
    let gat = &world.gat;
    let size = [gat.width as f32 * crate::world::CELL, gat.height as f32 * crate::world::CELL];
    let player = world.cell_position(cell.map(f32::from));
    let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |time| time.subsec_nanos());
    Sky::new(&effects, map, size, player, &ground_of(gat), seed)
}

/// The GAT height (larger is lower) under a world point on the ground plane.
pub fn ground_of(gat: &rustnarok_client::map::Gat) -> impl Fn([f32; 2]) -> f32 + '_ {
    |[x, z]| gat.height_at([x / crate::world::CELL, z / crate::world::CELL])
}

/// The sky's pictures, under data\texture\effect\.
pub struct Assets {
    clouds: Vec<Texture>,
    fogs: Vec<Texture>,
    stars: Vec<Texture>,
    fields: Vec<Texture>,
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let load =
            |names: Vec<String>| names.iter().filter_map(|name| gpu.try_load_texture(grf, &format!("data\\texture\\effect\\{name}"), true).ok()).collect();
        Self {
            clouds: load(["cloud4.tga", "cloud1.tga", "cloud2.tga"].map(String::from).to_vec()),
            fogs: load((1..=3).map(|n| format!("fog{n}.tga")).collect()),
            stars: load((1..=6).map(|n| format!("star0{n}.bmp")).collect()),
            fields: load((1..=4).map(|n| format!("cloud{n}.tga")).collect()),
        }
    }

    pub fn picture(&self, look: Look, index: usize) -> Option<&Texture> {
        let list = match look {
            Look::Clouds => &self.clouds,
            Look::Fog => &self.fogs,
            Look::Stars | Look::Star => &self.stars,
            Look::Field => &self.fields,
        };
        list.get(index).or(list.first())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FLAT: fn([f32; 2]) -> f32 = |_| 0.0;

    #[test]
    fn builds_each_sky_type_from_its_effect() {
        assert_eq!([229, 230, 233, 515, 516, 945, 1115, 1145].map(|effect| sky_type_of(effect).unwrap()), [0, 1, 2, 3, 4, 9, 11, 15]);
        assert_eq!(sky_type_of(47), None);
        assert_eq!(sky_puffs(1), [(1, 240)]);
        assert_eq!(sky_puffs(10), [(5, 120)]);
        assert_eq!(sky_puffs(11), [(11, 160), (12, 160), (13, 160), (14, 160)]);
        assert!(sky_puffs(6).is_empty() && kind(12).is_some() && kind(6).is_none());
        // Groups of 4 share a picture.
        let mut sky = Sky::new(&SkyEffects { types: vec![1], ..SkyEffects::default() }, "yuno", [1000.0; 2], [500.0, 0.0, 500.0], &FLAT, 7);
        assert_eq!(sky.puffs.len(), 240);
        assert!(sky.puffs.chunks(4).all(|group| group.iter().all(|puff| puff.picture == group[0].picture)));
        // Stars first, then the types; all at age 0 together.
        sky = Sky::new(&SkyEffects { stars: true, types: vec![2], ..SkyEffects::default() }, "dali", [1000.0; 2], [500.0, 0.0, 500.0], &FLAT, 7);
        assert_eq!((sky.puffs[0].kind.look, sky.puffs[260].kind.look, sky.puffs.len()), (Look::Stars, Look::Clouds, 420));
        assert!(sky.puffs.iter().all(|puff| puff.age == 0 && puff.alpha == 0.0));
    }

    #[test]
    fn places_puffs_in_a_square_or_a_ring() {
        let mut rng = Rng::new(3);
        for _ in 0..500 {
            let [x, z] = place(&mut rng, [1000.0, 2000.0], None);
            assert!((850.0..1150.0).contains(&x) && (1850.0..2150.0).contains(&z));
            let [x, z] = place(&mut rng, [1000.0, 2000.0], Some((25, 200)));
            assert!((x - 1000.0).abs() >= 25.0 && (x - 1000.0).abs() < 225.0 && (z - 2000.0).abs() >= 25.0 && (z - 2000.0).abs() < 225.0);
        }
    }

    #[test]
    fn fades_in_to_each_types_peak() {
        for (puff_type, step, ticks, peak) in [(0, 2.0, 80, 160.0), (1, 3.0, 80, 240.0), (3, 1.0, 170, 170.0), (11, 1.0, 70, 70.0), (14, 1.0, 70, 70.0)] {
            let kind = kind(puff_type).unwrap();
            assert_eq!((kind.step, kind.fade, kind.peak), (step, ticks, peak));
            let mut rng = Rng::new(1);
            let mut puff = Puff::new(&mut rng, kind, 0, [0.0, 200.0, 0.0], &FLAT, Some((25, 200)));
            for _ in 0..ticks {
                puff.tick(&mut rng, [0.0, 200.0, 0.0], &FLAT);
            }
            assert_eq!(puff.alpha, peak.min(step * ticks as f32), "type {puff_type}");
        }
        assert_eq!((kind(3).unwrap().color, kind(9).unwrap().additive, kind(4).unwrap().east), ([0xFC, 0xAB, 0x8F], true, true));
    }

    #[test]
    fn shows_type_0_only_from_high_ground() {
        let mut rng = Rng::new(1);
        // GAT height −160 (world y 160) is higher than −152: it fades in from age 0. On the ground (0) it starts at 300.
        let high = Puff::new(&mut rng, kind(0).unwrap(), 0, [0.0, 160.0, 0.0], &FLAT, None);
        let low = Puff::new(&mut rng, kind(0).unwrap(), 0, [0.0, 0.0, 0.0], &FLAT, None);
        assert_eq!((high.age, low.age), (0, 300));
    }

    #[test]
    fn lives_fades_out_and_starts_again_round_the_player() {
        let mut rng = Rng::new(9);
        let mut puff = Puff::new(&mut rng, kind(2).unwrap(), 0, [0.0; 3], &FLAT, None);
        let life = puff.life;
        assert!((300..500).contains(&life));
        for _ in 0..life {
            puff.tick(&mut rng, [0.0; 3], &FLAT);
        }
        assert_eq!(puff.alpha, 160.0);
        // Then 1 a tick; at 0 it starts again round where the player is now, at age 0.
        let moved = [5000.0, 0.0, 5000.0];
        for _ in 0..160 {
            puff.tick(&mut rng, moved, &FLAT);
        }
        assert_eq!((puff.age, puff.alpha), (0, 0.0));
        assert!((puff.at[0] - 5000.0).abs() <= 150.0 && (puff.at[2] - 5000.0).abs() <= 150.0);
        // A ground puff sits 20–24 above the ground under it.
        let smog = Puff::new(&mut rng, kind(3).unwrap(), 0, [0.0; 3], &|_| 30.0, None);
        assert!((-smog.at[1] - 30.0 + 24.0).abs() <= 4.0 && -smog.at[1] <= 10.0);
    }

    #[test]
    fn twinkles_stars_every_14_ticks() {
        let mut rng = Rng::new(2);
        let mut star = Puff::new(&mut rng, kind(9).unwrap(), 0, [0.0; 3], &FLAT, Some((25, 200)));
        let pictures: Vec<usize> = (0..90)
            .map(|_| {
                let picture = star.picture_now();
                star.tick(&mut rng, [0.0; 3], &FLAT);
                picture
            })
            .collect();
        assert_eq!((pictures[0], pictures[13], pictures[14], pictures[84]), (0, 0, 1, 0));
        let fixed = Puff::new(&mut rng, kind(15).unwrap(), 0, [0.0; 3], &FLAT, Some((25, 200)));
        assert_eq!(fixed.picture_now(), 0);
    }

    #[test]
    fn fades_a_cloud_field_with_the_200_extra() {
        let field = CloudField {
            num: 4,
            height: 40.0,
            height_extra: 10,
            size: 20.0,
            size_extra: 20,
            expand_rate: 0.05,
            color: [30, 30, 80],
            alpha_inc_time: 80,
            alpha_inc_speed: 1.0,
            alpha_dec_time: 300,
            alpha_dec_time_extra: 200,
            alpha_dec_speed: 0.5,
            cull_dist: 400.0,
        };
        let mut rng = Rng::new(5);
        for _ in 0..200 {
            let cloud = field_cloud(&mut rng, &field, [2000.0, 1000.0], 0);
            // Fade-in for 80 + rand % 200 (not % 50), fade-out from 300 + rand % 200; anywhere on the map.
            assert!((80..280).contains(&cloud.kind.fade) && (300..500).contains(&cloud.life));
            assert!((0.0..=2000.0).contains(&cloud.at[0]) && (0.0..=1000.0).contains(&cloud.at[2]));
            assert!((-50.0..=-40.0).contains(&cloud.at[1]));
        }
    }
}
