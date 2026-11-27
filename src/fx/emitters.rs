//! The map's particle emitters (RSW type-4 objects 974, and 1073 with moving pictures): each throws the particles its
//! row of the map's emitter table describes, in 16 ms ticks, while the player is near. The particles move in the map's
//! own axes (y down); only the drawn corners are in the world's (y up). The map's winds (`wind`), magic floors
//! (`magic_floor`) and Evil's Paws (`evils_paw`) take their rows, tick and draw along with them; its STR objects
//! (1412) only take their rows here, and play as STR world effects.

use rustnarok_client::lua::{EmitterInfo, MapEmitters, StrInfo};
use rustnarok_client::map::EffectSource;
use wgpu::BlendFactor;

use crate::evils_paw::{self, Paw};
use crate::magic_floor::{self, Floor};
use crate::wind::{self, Wind};
use crate::world::{MeshVertex, Vector};
use crate::world_fx::Eye;

pub const PLAIN: u32 = 974;
pub const ANIMATED: u32 = 1073;
/// The map's STR objects: each plays the file its row names.
pub const STR: u32 = 1412;
/// An emitter throws while the player is under this many world units away.
const RANGE: f32 = 130.0;
/// A tick as the rates, velocities and lives count it: a 60th of a second.
const STEP: f32 = 1.0 / 60.0;
const TICK_MS: u64 = 16;
/// At most this many ticks are caught up in one frame.
const MOST_TICKS: u64 = 64;
/// How high over its object a particle starts.
const RISE: f32 = 10.0;
/// A particle sits this far behind its place, along its own facing.
const BACK: f32 = 1.0;
/// The two picture sets of an animated emitter's particles, each picture shown for `SET_TICKS` ticks.
const SETS: [[&str; 5]; 2] = [
    ["effect\\shockwave_a.bmp", "effect\\shockwave_b.bmp", "effect\\shockwave_c.bmp", "effect\\shockwave_d.bmp", "effect\\shockwave_e.bmp"],
    ["effect\\plazma_a.bmp", "effect\\plazma_b.bmp", "effect\\plazma_c.bmp", "effect\\plazma_a.bmp", "effect\\plazma_b.bmp"],
];
const SET_TICKS: u32 = 8;
/// A square's corners v0 … v3 and their texture corners; drawn as v0 v1 v2, v2 v1 v3.
pub const SQUARE: [([f32; 2], [f32; 2]); 4] = [([-1.0, 1.0], [0.0, 0.0]), ([1.0, 1.0], [1.0, 0.0]), ([-1.0, -1.0], [0.0, 1.0]), ([1.0, -1.0], [1.0, 1.0])];

/// A draw of chance, 0 to 32 767.
pub fn chance() -> u32 {
    crate::effects::pick(0, 0x7fff)
}

/// A number from `a` to `b` in tenths: `a` when they are equal, else trunc(10a) plus up to their distance in tenths,
/// over 10. With `a` over `b` it runs up from `a`.
pub fn between(a: f32, b: f32, rand: &mut dyn FnMut() -> u32) -> f32 {
    if a == b {
        return a;
    }
    let low = (a * 10.0) as i32;
    let span = (low - (b * 10.0) as i32).unsigned_abs() + 1;
    (low + (rand() % span) as i32) as f32 * 0.1
}

/// How a blend number pair draws: the source's and the picture's factors. An unknown number keeps the default (5 and 2).
// ponytail: the picture behind is taken to have no alpha of its own, so its alpha counts as 1 (7 one, 8 zero, 11 zero);
// reading the frame's own alpha would replace these three.
pub fn factors(blend: [u8; 2]) -> [BlendFactor; 2] {
    let factor = |number: u8, default: BlendFactor| match number {
        1 | 8 | 11 => BlendFactor::Zero,
        2 | 7 => BlendFactor::One,
        3 => BlendFactor::Src,
        4 => BlendFactor::OneMinusSrc,
        5 => BlendFactor::SrcAlpha,
        6 => BlendFactor::OneMinusSrcAlpha,
        9 => BlendFactor::Dst,
        10 => BlendFactor::OneMinusDst,
        _ => default,
    };
    [factor(blend[0], BlendFactor::SrcAlpha), factor(blend[1], BlendFactor::One)]
}

/// How an emitter's particles are put on the picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Look {
    pub factors: [BlendFactor; 2],
    /// Hidden by what stands in front; without it, over everything.
    pub depth_test: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Particle {
    pub active: bool,
    /// Seconds.
    pub life: f32,
    pub size: f32,
    pub velocity: [f32; 3],
    /// From the emitter, in the map's axes.
    pub offset: [f32; 3],
    pub alpha: i32,
    /// Ticks since its start.
    pub age: u32,
    /// An animated emitter's picture set.
    pub set: usize,
}

impl Particle {
    /// A particle at its start: its set (animated only), then life, size, velocity and offset, in that order of draws.
    fn start(info: &EmitterInfo, animated: bool, rand: &mut dyn FnMut() -> u32) -> Self {
        let set = if animated { (rand() % 2) as usize } else { 0 };
        let life = between(info.life[0], info.life[1], rand);
        let size = between(info.size[0], info.size[1], rand);
        let velocity = [0, 1, 2].map(|axis| between(info.dir1[axis], info.dir2[axis], rand));
        let mut offset = [0, 1, 2].map(|axis| between(-info.radius[axis], info.radius[axis], rand));
        offset[1] -= RISE;
        Self { active: false, life, size, velocity, offset, alpha: 0, age: 0, set }
    }

    /// Its life in ticks is over (a life of 0 never is).
    fn expired(&self) -> bool {
        self.life != 0.0 && self.life * 60.0 <= self.age as f32
    }
}

pub struct Emitter {
    pub info: EmitterInfo,
    /// In the world (y up).
    pub at: Vector,
    pub animated: bool,
    /// Its place in `Emitters::looks`.
    pub look: usize,
    pub particles: Vec<Particle>,
    /// The part of a particle owed by the rate so far.
    owed: f32,
    /// The player is out of range: nothing new is thrown.
    pub far: bool,
    /// Far with no particle left: nothing moves or draws.
    pub asleep: bool,
}

impl Emitter {
    /// An emitter with its `max_count` particles, none thrown yet; an eternal one with no rate has them all out.
    pub fn new(info: EmitterInfo, at: Vector, animated: bool, rand: &mut dyn FnMut() -> u32) -> Self {
        let all_out = info.eternal && info.rate == [0.0, 0.0];
        let particles = (0..info.max_count)
            .map(|_| Particle { active: all_out, alpha: if all_out { i32::from(info.color[3]) } else { 0 }, ..Particle::start(&info, animated, rand) })
            .collect();
        Self { info, at, animated, look: 0, particles, owed: 0.0, far: false, asleep: false }
    }

    /// One 16 ms tick, the player `distance` away.
    pub fn tick(&mut self, distance: f32, rand: &mut dyn FnMut() -> u32) {
        self.far = distance >= RANGE;
        self.asleep = self.far && (self.asleep || !self.particles.iter().any(|particle| particle.active));
        if self.asleep {
            return;
        }
        if !self.far {
            // The rate's whole particles this tick, plus one for each whole particle its fractions add up to.
            let share = between(self.info.rate[0], self.info.rate[1], rand).trunc() * STEP;
            let mut count = share as i32;
            self.owed += share - count as f32;
            if self.owed > 1.0 {
                count += self.owed as i32;
                self.owed = self.owed.fract();
            }
            for _ in 0..count {
                let Some(free) = self.particles.iter_mut().find(|particle| !particle.active) else { break };
                *free = Particle {
                    active: true,
                    alpha: if self.info.eternal { i32::from(self.info.color[3]) } else { 0 },
                    ..Particle::start(&self.info, self.animated, rand)
                };
            }
        }
        let (info, far) = (&self.info, self.far);
        for particle in &mut self.particles {
            if particle.active {
                // An eternal emitter's particles outlast their lives while the player is near.
                if particle.expired() && (far || !info.eternal) {
                    particle.active = false;
                } else {
                    if !info.eternal {
                        // +4 a tick up to 255, then −5 a tick through its last second.
                        let left = particle.life * 60.0 - particle.age as f32;
                        let step = STEP * 255.0;
                        particle.alpha =
                            if left < 60.0 { ((particle.alpha as f32 - step) as i32).max(0) } else { ((particle.alpha as f32 + step) as i32).min(255) };
                    }
                    for axis in 0..3 {
                        particle.offset[axis] += particle.velocity[axis] * info.speed * STEP;
                        particle.velocity[axis] += info.gravity[axis] * STEP;
                    }
                }
            }
            particle.age += 1;
        }
    }

    /// The picture a particle shows now, under `data\texture\`.
    pub fn picture(&self, particle: &Particle) -> &str {
        if self.animated { SETS[particle.set % 2][(particle.age.saturating_sub(1) / SET_TICKS % 5) as usize] } else { &self.info.texture }
    }

    /// A particle's four corners in the world. Turned to the camera: its square of half sides size × scale across the
    /// view. Fixed: the square turned about x, y, then z by whole degrees in the map's axes.
    // ponytail: turned to the camera, the picture's top row is at the bottom, as the square's own axes give it; flip
    // SQUARE's texture corners if a picture with an up and a down shows otherwise.
    pub fn corners(&self, particle: &Particle, eye: &Eye) -> [Vector; 4] {
        let half = [particle.size * self.info.scale[0], particle.size * self.info.scale[1]];
        let center: Vector = [self.at[0] + particle.offset[0], self.at[1] - particle.offset[1], self.at[2] + particle.offset[2]];
        SQUARE.map(|([x, y], _)| {
            let [x, y] = [x * half[0], y * half[1]];
            let from_center = if self.info.fixed {
                let [a, b, c] = self.info.rotate.map(|degrees| degrees.trunc().to_radians().sin_cos());
                let [x, y, z] = [x, y * a.1 - BACK * a.0, y * a.0 + BACK * a.1];
                let [x, z] = [x * b.1 + z * b.0, z * b.1 - x * b.0];
                [x * c.1 - y * c.0, -(x * c.0 + y * c.1), z]
            } else {
                [0, 1, 2].map(|axis| eye.right[axis] * x - eye.up[axis] * y + eye.forward[axis] * BACK)
            };
            [0, 1, 2].map(|axis| center[axis] + from_center[axis])
        })
    }
}

/// What one emitter draws with one picture: two triangles a particle.
pub struct Shape<'a> {
    pub picture: &'a str,
    pub look: usize,
    pub vertices: Vec<MeshVertex>,
}

/// A map's emitters, winds, magic floors, Evil's Paws and STR objects.
#[derive(Default)]
pub struct Emitters {
    pub list: Vec<Emitter>,
    pub winds: Vec<Wind>,
    pub floors: Vec<Floor>,
    pub paws: Vec<Paw>,
    /// The STR objects' rows and where each plays, in the world (y up).
    pub strs: Vec<(StrInfo, Vector)>,
    /// The different looks of them all.
    pub looks: Vec<Look>,
    /// The last 16 ms tick run.
    last: Option<u64>,
}

impl Emitters {
    /// The map's emitter, wind, magic floor, Evil's Paw and STR objects, each with the table row of its number among
    /// those of its kind, counted from 0 in the map's order; placed in the world (x and z moved by `half` from the
    /// map's centre, y up). A wind stands where its row says (the same place, in the data).
    // ponytail: an object without a row draws nothing; no map in the data has one.
    pub fn new(sources: &[EffectSource], half: [f32; 2], tables: &MapEmitters, rand: &mut dyn FnMut() -> u32) -> Self {
        let mut emitters = Self::default();
        let mut numbers = [0u32; 6];
        for source in sources {
            let kind = match u32::try_from(source.id) {
                Ok(PLAIN) => 0,
                Ok(ANIMATED) => 1,
                Ok(wind::ID) => 2,
                Ok(magic_floor::ID) => 3,
                Ok(evils_paw::ID) => 4,
                Ok(STR) => 5,
                _ => continue,
            };
            let number = numbers[kind];
            numbers[kind] += 1;
            let at = [source.position[0] + half[0], -source.position[1], source.position[2] + half[1]];
            // The wind, the floor's pictures and rings and the paw are hidden by what stands in front; the floor's
            // blends come to the default.
            let (blend, depth_test) = match kind {
                0 | 1 => {
                    let Some(info) = [&tables.plain, &tables.animated][kind].get(&number) else { continue };
                    emitters.list.push(Emitter::new(info.clone(), at, kind == 1, rand));
                    (info.blend, info.depth_test)
                }
                2 => {
                    let Some(info) = tables.winds.get(&number) else { continue };
                    emitters.winds.push(Wind::new(info.clone(), half, rand));
                    (info.blend, true)
                }
                3 => {
                    let Some(info) = tables.floors.get(&number) else { continue };
                    emitters.floors.push(Floor::new(*info, at, rand));
                    ([0, 0], true)
                }
                4 => {
                    let Some(info) = tables.paws.get(&number) else { continue };
                    emitters.paws.push(Paw::new(*info, at));
                    (evils_paw::BLEND, true)
                }
                _ => {
                    emitters.strs.extend(tables.strs.get(&number).map(|info| (info.clone(), at)));
                    continue;
                }
            };
            let look = Look { factors: factors(blend), depth_test };
            let look = emitters.looks.iter().position(|known| *known == look).unwrap_or_else(|| {
                emitters.looks.push(look);
                emitters.looks.len() - 1
            });
            match kind {
                0 | 1 => emitters.list.last_mut().expect("just pushed").look = look,
                2 => emitters.winds.last_mut().expect("just pushed").look = look,
                3 => emitters.floors.last_mut().expect("just pushed").look = look,
                _ => emitters.paws.last_mut().expect("just pushed").look = look,
            }
        }
        emitters
    }

    /// Every picture the emitters, winds, magic floors and paws may show, under `data\texture\`.
    pub fn pictures(&self) -> impl Iterator<Item = &str> {
        let sets: &[&str] = if self.list.iter().any(|emitter| emitter.animated) { SETS.as_flattened() } else { &[] };
        let floors: &[&str] = if self.floors.is_empty() { &[] } else { &magic_floor::DISCS };
        let ring = (!self.floors.is_empty()).then_some(magic_floor::RING);
        let paws: &[&str] = if self.paws.is_empty() { &[] } else { &evils_paw::PICTURES };
        let plain = self.list.iter().filter(|emitter| !emitter.animated).map(|emitter| emitter.info.texture.as_str());
        let winds = self.winds.iter().map(|wind| wind.info.texture.as_str());
        plain.chain(sets.iter().copied()).chain(winds).chain(floors.iter().copied()).chain(ring).chain(paws.iter().copied())
    }

    /// The 16 ms ticks passed since the last call (`now_ms` in whole ms), the player standing at `player`.
    pub fn tick(&mut self, now_ms: u64, player: Vector, rand: &mut dyn FnMut() -> u32) {
        let tick_now = now_ms / TICK_MS;
        let ticks = self.last.map_or(1, |last| tick_now.saturating_sub(last).min(MOST_TICKS));
        self.last = Some(tick_now);
        let distance = |at: Vector| [0, 1, 2].map(|axis| (at[axis] - player[axis]).powi(2)).iter().sum::<f32>().sqrt();
        for emitter in &mut self.list {
            let distance = distance(emitter.at);
            for _ in 0..ticks {
                emitter.tick(distance, rand);
            }
        }
        // A wind, a floor or a paw out of range stops as it is, and isn't drawn.
        for wind in &mut self.winds {
            wind.near = distance(wind.at()) < RANGE;
            (0..if wind.near { ticks } else { 0 }).for_each(|_| wind.tick());
        }
        for floor in &mut self.floors {
            floor.near = distance(floor.at) < RANGE;
            (0..if floor.near { ticks } else { 0 }).for_each(|_| floor.tick());
        }
        for paw in &mut self.paws {
            paw.near = distance(paw.at) < RANGE;
            (0..if paw.near { ticks } else { 0 }).for_each(|_| paw.tick());
        }
    }

    /// The winds, the magic floors and the paws moving now: those the player is near.
    pub fn turning(&self) -> [usize; 3] {
        [
            self.winds.iter().filter(|wind| wind.near).count(),
            self.floors.iter().filter(|floor| floor.near).count(),
            self.paws.iter().filter(|paw| paw.near).count(),
        ]
    }

    /// The particles out now.
    pub fn out(&self) -> usize {
        self.list.iter().filter(|emitter| !emitter.asleep).flat_map(|emitter| &emitter.particles).filter(|particle| particle.active).count()
    }

    /// What the waking emitters draw, in their order: the particles that are out and not fully faded.
    // ponytail: an eternal particle that ended while the player was far isn't drawn until it's thrown again (within its
    // rate); drawing the ended ones where they stopped would show it meanwhile.
    pub fn shapes(&self, eye: &Eye) -> Vec<Shape<'_>> {
        let mut shapes: Vec<Shape> = Vec::new();
        for emitter in self.list.iter().filter(|emitter| !emitter.asleep) {
            let first = shapes.len();
            let [red, green, blue, _] = emitter.info.color.map(|channel| f32::from(channel) / 255.0);
            for particle in emitter.particles.iter().filter(|particle| particle.active && particle.alpha > 0) {
                let picture = emitter.picture(particle);
                let shape = match shapes[first..].iter().position(|shape| shape.picture == picture) {
                    Some(index) => &mut shapes[first + index],
                    None => {
                        shapes.push(Shape { picture, look: emitter.look, vertices: Vec::new() });
                        shapes.last_mut().expect("just pushed")
                    }
                };
                let corners = emitter.corners(particle, eye);
                let tint = [red, green, blue, particle.alpha.min(255) as f32 / 255.0];
                shape.vertices.extend([0, 1, 2, 2, 1, 3].map(|corner| MeshVertex { position: corners[corner], uv: SQUARE[corner].1, tint }));
            }
        }
        // The winds' strands, then each floor's six pictures and its rings.
        for wind in self.winds.iter().filter(|wind| wind.near) {
            shapes.push(Shape { picture: &wind.info.texture, look: wind.look, vertices: wind.vertices() });
        }
        for floor in self.floors.iter().filter(|floor| floor.near) {
            shapes.extend(magic_floor::DISCS.into_iter().zip(floor.discs(eye)).map(|(picture, vertices)| Shape { picture, look: floor.look, vertices }));
            shapes.push(Shape { picture: magic_floor::RING, look: floor.look, vertices: floor.rings(eye) });
        }
        for paw in self.paws.iter().filter(|paw| paw.near) {
            shapes.push(Shape { picture: evils_paw::PICTURES[paw.shown], look: paw.look, vertices: paw.vertices(eye) });
        }
        shapes.retain(|shape| !shape.vertices.is_empty());
        shapes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn info() -> EmitterInfo {
        EmitterInfo {
            dir1: [0.0; 3],
            dir2: [0.0; 3],
            gravity: [0.0; 3],
            radius: [0.0; 3],
            color: [255, 128, 0, 200],
            rate: [60.0, 60.0],
            size: [5.0, 5.0],
            life: [3.0, 3.0],
            texture: "effect\\a.bmp".into(),
            speed: 1.0,
            blend: [5, 2],
            max_count: 4,
            depth_test: true,
            fixed: false,
            rotate: [0.0; 3],
            eternal: false,
            scale: [1.0; 2],
        }
    }

    /// An emitter at the origin that never draws a number by chance.
    fn emitter(info: EmitterInfo) -> Emitter {
        Emitter::new(info, [0.0; 3], false, &mut || 0)
    }

    fn close(a: Vector, b: Vector) -> bool {
        a.iter().zip(b).all(|(a, b)| (a - b).abs() < 1e-4)
    }

    #[test]
    fn draws_numbers_in_tenths() {
        let mut draws = [7u32, 23, 5, 0, 20].into_iter();
        let mut rand = || draws.next().unwrap();
        // Equal ends: that number, without a draw.
        assert_eq!(between(2.5, 2.5, &mut rand), 2.5);
        // 1 to 5 is 41 steps of a tenth from 1.0: draws 7 and 23 + 41.
        assert!((between(1.0, 5.0, &mut rand) - 1.7).abs() < 1e-5);
        assert!((between(1.0, 5.0, &mut || 64) - 3.3).abs() < 1e-5);
        assert!((between(1.0, 5.0, &mut rand) - 3.3).abs() < 1e-5);
        // Either way from 0: −2 to 2.
        assert!((between(-2.0, 2.0, &mut rand) + 1.5).abs() < 1e-5);
        assert!((between(-2.0, 2.0, &mut || 40) - 2.0).abs() < 1e-5);
        // The first end over the second: up from the first, as far as they are apart.
        assert!((between(15.0, 5.0, &mut rand) - 15.0).abs() < 1e-5);
        assert!((between(15.0, 5.0, &mut rand) - 17.0).abs() < 1e-5);
    }

    #[test]
    fn starts_a_particle_from_eight_draws() {
        let row = EmitterInfo { life: [1.0, 5.0], size: [3.0, 8.0], dir1: [-1.0, -2.0, 0.0], dir2: [1.0, -1.0, 0.5], radius: [2.0, 0.0, 1.0], ..info() };
        let mut draws = [10u32, 20, 5, 3, 2, 40, 7].into_iter();
        let particle = Particle::start(&row, false, &mut || draws.next().unwrap());
        assert!((particle.life - 2.0).abs() < 1e-5 && (particle.size - 5.0).abs() < 1e-5);
        assert!(close(particle.velocity, [-0.5, -1.7, 0.2]));
        // Either way within the radius, and 10 over the object (y is down); the radius of 0 takes no draw.
        assert!(close(particle.offset, [2.0, -10.0, -0.3]));
        assert_eq!((particle.alpha, particle.age, particle.active, draws.next()), (0, 0, false, None));
        // An animated emitter's particle draws its picture set first.
        let mut draws = [3u32, 10, 20, 5, 3, 2, 40, 7].into_iter();
        let particle = Particle::start(&row, true, &mut || draws.next().unwrap());
        assert_eq!(particle.set, 1);
        assert!((particle.life - 2.0).abs() < 1e-5);
    }

    #[test]
    fn throws_at_its_rate_up_to_its_count() {
        let out = |emitter: &Emitter| emitter.particles.iter().filter(|particle| particle.active).count();
        // 30 a second is half a particle a tick; one is thrown each time the halves pass a whole one.
        let mut slow = emitter(EmitterInfo { rate: [30.0, 30.0], max_count: 40, ..info() });
        let thrown: Vec<usize> = (0..7)
            .map(|_| {
                slow.tick(0.0, &mut || 0);
                out(&slow)
            })
            .collect();
        assert_eq!(thrown, [0, 0, 1, 1, 2, 2, 3]);
        // 120 a second is two a tick, until all four are out.
        let mut fast = emitter(EmitterInfo { rate: [120.0, 120.0], ..info() });
        let thrown: Vec<usize> = (0..3)
            .map(|_| {
                fast.tick(0.0, &mut || 0);
                out(&fast)
            })
            .collect();
        assert_eq!(thrown, [2, 4, 4]);
        // The rate is a whole number of particles: 0.9 a second throws nothing.
        let mut none = emitter(EmitterInfo { rate: [0.9, 0.9], ..info() });
        (0..600).for_each(|_| none.tick(0.0, &mut || 0));
        assert_eq!(out(&none), 0);
    }

    #[test]
    fn fades_in_and_out_over_its_life() {
        // One particle of 3 s: 180 ticks. It comes out on the first tick (60 a second).
        let mut emitter = emitter(EmitterInfo { max_count: 1, ..info() });
        let mut alphas = Vec::new();
        for _ in 0..181 {
            emitter.tick(0.0, &mut || 0);
            alphas.push((emitter.particles[0].active, emitter.particles[0].alpha));
        }
        // +4 a tick to 255 at the 64th, held, −5 a tick from 59 ticks before its end, gone at the end.
        assert_eq!(alphas[..3], [(true, 4), (true, 8), (true, 12)]);
        assert_eq!((alphas[62], alphas[63], alphas[120]), ((true, 252), (true, 255), (true, 255)));
        assert_eq!((alphas[121], alphas[122], alphas[171], alphas[172], alphas[179]), ((true, 250), (true, 245), (true, 0), (true, 0), (true, 0)));
        assert_eq!(alphas[180], (false, 0));
        // A particle of a second or less only fades out, from next to nothing.
        let mut short = self::emitter(EmitterInfo { max_count: 1, life: [1.0, 1.0], ..info() });
        for _ in 0..30 {
            short.tick(0.0, &mut || 0);
        }
        assert_eq!((short.particles[0].active, short.particles[0].alpha), (true, 0));
        assert!(Emitters { list: vec![short], ..Default::default() }.shapes(&Eye::at(45.0, 0.0)).is_empty());
    }

    #[test]
    fn moves_by_its_velocity_and_the_gravity() {
        let mut emitter =
            emitter(EmitterInfo { dir1: [6.0, -12.0, 0.0], dir2: [6.0, -12.0, 0.0], gravity: [0.0, 60.0, 30.0], speed: 0.5, max_count: 1, ..info() });
        emitter.tick(0.0, &mut || 0);
        // A tick: velocity × speed / 60 from 10 over the object, then the velocity takes gravity / 60.
        assert!(close(emitter.particles[0].offset, [0.05, -10.1, 0.0]));
        assert!(close(emitter.particles[0].velocity, [6.0, -11.0, 0.5]));
        emitter.tick(0.0, &mut || 0);
        assert!(close(emitter.particles[0].offset, [0.1, -10.1 - 11.0 / 120.0, 0.5 / 120.0]));
        assert_eq!(emitter.particles[0].age, 2);
    }

    #[test]
    fn stops_throwing_when_the_player_is_far() {
        let out = |emitter: &Emitter| emitter.particles.iter().filter(|particle| particle.active).count();
        let mut emitter = emitter(EmitterInfo { life: [2.0, 2.0], ..info() });
        (0..10).for_each(|_| emitter.tick(129.9, &mut || 0));
        assert_eq!((out(&emitter), emitter.far, emitter.asleep), (4, false, false));
        // At 130 or more nothing new comes out; the ones out finish their lives, then it sleeps.
        (0..100).for_each(|_| emitter.tick(130.0, &mut || 0));
        assert_eq!((out(&emitter), emitter.far, emitter.asleep), (4, true, false));
        (0..20).for_each(|_| emitter.tick(130.0, &mut || 0));
        assert_eq!((out(&emitter), emitter.asleep), (0, true));
        let age = emitter.particles[0].age;
        emitter.tick(500.0, &mut || 0);
        assert_eq!(emitter.particles[0].age, age);
        // Near again it wakes and throws.
        emitter.tick(10.0, &mut || 0);
        assert_eq!((out(&emitter), emitter.asleep), (1, false));
    }

    #[test]
    fn keeps_eternal_particles_while_near() {
        let out = |emitter: &Emitter| emitter.particles.iter().filter(|particle| particle.active).count();
        // One a second, a life of 1 s: out at the row's alpha at once, and still there long after its life.
        let mut emitter = emitter(EmitterInfo { eternal: true, rate: [1.0, 1.0], life: [1.0, 1.0], max_count: 1, ..info() });
        (0..61).for_each(|_| emitter.tick(0.0, &mut || 0));
        assert_eq!((out(&emitter), emitter.particles[0].alpha), (1, 200));
        (0..600).for_each(|_| emitter.tick(0.0, &mut || 0));
        assert_eq!((out(&emitter), emitter.particles[0].alpha), (1, 200));
        // Far, it goes at once (its life is long over); near again, it comes back within its rate.
        emitter.tick(300.0, &mut || 0);
        assert_eq!(out(&emitter), 0);
        (0..61).for_each(|_| emitter.tick(0.0, &mut || 0));
        assert_eq!(out(&emitter), 1);
        // No rate at all: every particle is out from the start.
        let all = self::emitter(EmitterInfo { eternal: true, rate: [0.0, 0.0], ..info() });
        assert_eq!((out(&all), all.particles[3].alpha), (4, 200));
    }

    #[test]
    fn turns_a_particle_to_the_camera_or_by_its_angles() {
        let eye = Eye::at(45.0, 30.0);
        let particle = Particle { active: true, size: 5.0, offset: [1.0, -10.0, 2.0], alpha: 255, ..Default::default() };
        // To the camera: half sides size × scale along the view's right and up, one unit behind its place, the
        // picture's top (v 0, the first two corners) at the bottom.
        let mut facing = emitter(EmitterInfo { scale: [2.0, 1.0], ..info() });
        facing.at = [100.0, 20.0, 50.0];
        let corners = facing.corners(&particle, &eye);
        let center: Vector = std::array::from_fn(|axis| [101.0, 30.0, 52.0][axis] + eye.forward[axis]);
        let expect = |right: f32, up: f32| -> Vector { std::array::from_fn(|axis| center[axis] + eye.right[axis] * right + eye.up[axis] * up) };
        assert!(close(corners[0], expect(-10.0, -5.0)) && close(corners[1], expect(10.0, -5.0)));
        assert!(close(corners[2], expect(-10.0, 5.0)) && close(corners[3], expect(10.0, 5.0)));
        // Fixed, a quarter turn about x: flat, one unit over its place, whatever the camera.
        let mut flat = emitter(EmitterInfo { fixed: true, rotate: [90.9, 0.0, 0.0], ..info() });
        flat.at = [100.0, 20.0, 50.0];
        let corners = flat.corners(&particle, &eye);
        assert!(close(corners[0], [96.0, 31.0, 57.0]) && close(corners[1], [106.0, 31.0, 57.0]));
        assert!(close(corners[2], [96.0, 31.0, 47.0]) && close(corners[3], [106.0, 31.0, 47.0]));
        // Then about y and z: a quarter turn about y swings its right to the map's −z.
        let turned = emitter(EmitterInfo { fixed: true, rotate: [0.0, 90.0, 0.0], ..info() });
        let corners = turned.corners(&Particle { size: 1.0, ..Default::default() }, &eye);
        assert!(close(corners[1], [1.0, -1.0, -1.0]), "{corners:?}");
        let rolled = emitter(EmitterInfo { fixed: true, rotate: [0.0, 0.0, 90.0], ..info() });
        let corners = rolled.corners(&Particle { size: 1.0, ..Default::default() }, &eye);
        assert!(close(corners[1], [-1.0, -1.0, 1.0]), "{corners:?}");
    }

    #[test]
    fn maps_blend_numbers_to_factors() {
        use BlendFactor::*;
        assert_eq!(factors([5, 2]), [SrcAlpha, One]);
        assert_eq!(factors([2, 2]), [One, One]);
        assert_eq!(factors([10, 4]), [OneMinusDst, OneMinusSrc]);
        assert_eq!(factors([9, 6]), [Dst, OneMinusSrcAlpha]);
        assert_eq!(factors([3, 1]), [Src, Zero]);
        // The picture's own alpha counts as 1.
        assert_eq!(factors([7, 7]), [One, One]);
        assert_eq!(factors([8, 4]), [Zero, OneMinusSrc]);
        assert_eq!(factors([11, 4]), [Zero, OneMinusSrc]);
        assert_eq!(factors([0, 99]), [SrcAlpha, One]);
    }

    fn source(id: i32, x: f32) -> EffectSource {
        EffectSource { name: String::new(), position: [x, -5.0, 20.0], id, speed: 1.0, params: [0.0; 4] }
    }

    #[test]
    fn gives_each_object_its_row() {
        let row = |texture: &str, blend: [u8; 2]| EmitterInfo { texture: texture.into(), blend, ..info() };
        let tables = MapEmitters {
            plain: HashMap::from([(0, row("a.bmp", [5, 2])), (1, row("b.bmp", [2, 2])), (2, row("c.bmp", [7, 7]))]),
            animated: HashMap::from([(0, row("unused.bmp", [5, 2]))]),
            ..Default::default()
        };
        // Rows go by each kind's own count, in the map's order; other effects don't count; the fourth emitter has no row.
        let sources = [source(974, 1.0), source(47, 2.0), source(1073, 3.0), source(974, 4.0), source(974, 5.0), source(974, 6.0), source(1073, 7.0)];
        let emitters = Emitters::new(&sources, [100.0, 200.0], &tables, &mut || 0);
        let got: Vec<(&str, f32, bool, usize)> =
            emitters.list.iter().map(|emitter| (emitter.info.texture.as_str(), emitter.at[0], emitter.animated, emitter.look)).collect();
        assert_eq!(got, [("a.bmp", 101.0, false, 0), ("unused.bmp", 103.0, true, 0), ("b.bmp", 104.0, false, 1), ("c.bmp", 105.0, false, 1)]);
        assert_eq!(emitters.list[0].at, [101.0, 5.0, 220.0]);
        assert_eq!(emitters.looks.len(), 2);
        // The pictures to load: the plain rows' own, and the animated sets.
        let pictures: Vec<&str> = emitters.pictures().collect();
        assert_eq!(pictures[..3], ["a.bmp", "b.bmp", "c.bmp"]);
        assert_eq!((pictures.len(), pictures[3], pictures[12]), (13, "effect\\shockwave_a.bmp", "effect\\plazma_b.bmp"));
        assert_eq!(Emitters::new(&sources[..1], [0.0; 2], &tables, &mut || 0).pictures().count(), 1);
    }

    #[test]
    fn ticks_by_the_clock_and_draws_what_is_out() {
        let tables = MapEmitters { plain: HashMap::from([(0, info()), (1, info())]), animated: HashMap::from([(0, info())]), ..Default::default() };
        let mut emitters = Emitters::new(&[source(974, 0.0), source(974, 500.0), source(1073, 0.0)], [0.0; 2], &tables, &mut || 0);
        let player = [0.0, 5.0, 20.0];
        // One tick on the first call, then one per 16 ms passed; the far emitter throws nothing.
        emitters.tick(1000, player, &mut || 0);
        assert_eq!(emitters.out(), 2);
        emitters.tick(1007, player, &mut || 0);
        assert_eq!(emitters.out(), 2);
        emitters.tick(1030, player, &mut || 0);
        assert_eq!((emitters.out(), emitters.list[1].asleep), (6, true));
        // A shape per emitter and picture, six corners a particle, tinted the row's colour at the particle's alpha.
        let shapes = emitters.shapes(&Eye::at(45.0, 0.0));
        let got: Vec<(&str, usize)> = shapes.iter().map(|shape| (shape.picture, shape.vertices.len())).collect();
        assert_eq!(got, [("effect\\a.bmp", 18), ("effect\\shockwave_a.bmp", 18)]);
        let vertex = shapes[0].vertices[0];
        assert_eq!((vertex.uv, vertex.tint), ([0.0, 0.0], [1.0, 128.0 / 255.0, 0.0, 12.0 / 255.0]));
        assert_eq!(shapes[0].vertices[5].uv, [1.0, 1.0]);
        // An animated particle's picture moves on every 8 ticks, through its set's five.
        let animated = &emitters.list[2];
        let at = |age: u32, set: usize| animated.picture(&Particle { age, set, ..Default::default() });
        assert_eq!(
            (at(8, 0), at(9, 0), at(40, 0), at(41, 0)),
            ("effect\\shockwave_a.bmp", "effect\\shockwave_b.bmp", "effect\\shockwave_e.bmp", "effect\\shockwave_a.bmp")
        );
        assert_eq!((at(17, 1), at(25, 1)), ("effect\\plazma_c.bmp", "effect\\plazma_a.bmp"));
    }

    #[test]
    fn gives_winds_and_floors_their_rows_too() {
        use rustnarok_client::lua::{FloorInfo, WindInfo};
        let wind = WindInfo {
            position: [50.0, -5.0, 20.0],
            strands: 2,
            alpha: 200,
            color: [255; 3],
            radius: 10.0,
            thickness: 1.0,
            height: 4.0,
            speed: 1.0,
            span: 200,
            tilt: [0.0; 2],
            blend: [6, 2],
            texture: "effect\\w.tga".into(),
        };
        let tables = MapEmitters {
            plain: HashMap::from([(0, info())]),
            winds: HashMap::from([(1, wind)]),
            floors: HashMap::from([(0, FloorInfo { size: 10.0, angle: 0, alpha: 255 })]),
            ..Default::default()
        };
        // Each kind counts its own objects: the second wind has the row; the first, and the second floor, have none.
        let sources = [source(2343, 1.0), source(974, 2.0), source(1025, 300.0), source(2343, 4.0), source(1025, 5.0)];
        let mut emitters = Emitters::new(&sources, [100.0, 200.0], &tables, &mut || 7);
        assert_eq!((emitters.list.len(), emitters.winds.len(), emitters.floors.len()), (1, 1, 1));
        // The wind stands where its row says, the floor where its object does; the floor's look is the emitter's.
        assert_eq!((emitters.winds[0].at(), emitters.floors[0].at), ([150.0, 5.0, 220.0], [400.0, 5.0, 220.0]));
        assert_eq!((emitters.list[0].look, emitters.winds[0].look, emitters.floors[0].look, emitters.looks.len()), (0, 1, 0, 2));
        let pictures: Vec<&str> = emitters.pictures().collect();
        assert_eq!((pictures.len(), pictures[1], pictures[2], pictures[8]), (9, "effect\\w.tga", "effect\\hero_011.tga", "effect\\cloud11.tga"));
        // Near the wind only: it turns and draws its strands; the floor, 250 away, stays as it was and draws nothing.
        let eye = Eye::at(45.0, 0.0);
        emitters.tick(1000, [150.0, 5.0, 220.0], &mut || 0);
        assert_eq!((emitters.turning(), emitters.winds[0].strands[0].alpha, emitters.floors[0].age), ([1, 0, 0], 8, 0));
        let got: Vec<(&str, usize, usize)> = emitters.shapes(&eye).iter().map(|shape| (shape.picture, shape.look, shape.vertices.len())).collect();
        assert_eq!(got, [("effect\\a.bmp", 0, 6), ("effect\\w.tga", 1, 240)]);
        // Near the floor: its six pictures and its rings after the emitter's particle (still out, though far), and the
        // wind stops where it was.
        emitters.tick(1016, [400.0, 5.0, 220.0], &mut || 0);
        assert_eq!((emitters.turning(), emitters.winds[0].strands[0].alpha, emitters.floors[0].age), ([0, 1, 0], 8, 1));
        let got: Vec<(&str, usize)> = emitters.shapes(&eye).iter().map(|shape| (shape.picture, shape.vertices.len())).collect();
        assert_eq!(got[..3], [("effect\\a.bmp", 6), ("effect\\hero_011.tga", 6), ("effect\\hero_001.tga", 6)]);
        assert_eq!((got.len(), got[7]), (8, ("effect\\cloud11.tga", 8 * 20 * 6)));
    }

    #[test]
    fn gives_paws_and_str_objects_their_rows_too() {
        use rustnarok_client::lua::{PawInfo, StrInfo};
        let splash = StrInfo { file: "water_splash\\water_splash.str".into(), scale: 1.3, alpha: 1.0 };
        let tables = MapEmitters {
            paws: HashMap::from([(1, PawInfo { offset: [0.0, -40.0, 0.0], size: 14.0, speed: 1 })]),
            strs: HashMap::from([(0, splash.clone())]),
            ..Default::default()
        };
        // Each kind counts its own objects: the second paw and the first STR object have rows.
        let sources = [source(1039, 1.0), source(1412, 2.0), source(1039, 3.0), source(1412, 4.0)];
        let mut emitters = Emitters::new(&sources, [100.0, 200.0], &tables, &mut || 7);
        assert_eq!((emitters.paws.len(), emitters.paws[0].at, &emitters.strs), (1, [103.0, 5.0, 220.0], &vec![(splash, [102.0, 5.0, 220.0])]));
        // The paw's own alpha blends it over what is behind, and what stands in front hides it.
        let over = [BlendFactor::SrcAlpha, BlendFactor::OneMinusSrcAlpha];
        assert_eq!(emitters.looks, [Look { factors: over, depth_test: true }]);
        assert_eq!(emitters.pictures().collect::<Vec<_>>(), evils_paw::PICTURES);
        // Far from it: it neither steps nor draws. Near (by the object, not the picture 40 over it): a picture every two
        // ticks.
        let eye = Eye::at(45.0, 0.0);
        emitters.tick(1000, [103.0, 5.0, 400.0], &mut || 0);
        assert_eq!((emitters.turning(), emitters.shapes(&eye).len()), ([0, 0, 0], 0));
        emitters.tick(1016, [103.0, -124.0, 220.0], &mut || 0);
        assert_eq!((emitters.turning(), emitters.paws[0].shown), ([0, 0, 1], 0));
        emitters.tick(1016 + 32, [103.0, -124.0, 220.0], &mut || 0);
        let got: Vec<(&str, usize, usize)> = emitters.shapes(&eye).iter().map(|shape| (shape.picture, shape.look, shape.vertices.len())).collect();
        assert_eq!(got, [("effect\\1-2.tga", 0, 6)]);
    }
}
