//! What a map's own effect objects show: the glows (EF_GLOW1, 2, 11, 12), the coloured torches and the dust
//! (EF_DUST, EF_TORCH_RED, EF_TORCH_GREEN and the violet one), the yellow flies (EF_YELLOWFLY1 to 3), the firefly
//! (EF_FIREFLY) and the chimney smoke (EF_SMOKE). Squares and sprites that face the camera, moved in 16 ms ticks.
//! Offsets are in the map's axes (y down); places in the world's (y up).

use std::time::Instant;

use super::TICK;
use super::snow::flake_corners;
use crate::world::Vector;

pub const FIREFLY: u32 = 45;
pub const SMOKE: u32 = 44;
pub const FLY_TEXTURE: &str = "data\\texture\\effect\\whitelight.tga";
pub const FIREFLY_SPRITE: &str = "data\\sprite\\이팩트\\particle1";
pub const SMOKE_SPRITE: &str = "data\\sprite\\이팩트\\굴뚝연기";
/// The smoke runs and shows only with the player under 130 units away.
const SMOKE_RANGE: f32 = 130.0;
/// At most this many ticks are caught up in one frame.
const MOST_TICKS: u32 = 64;
// ponytail: nothing further than this from the player is drawn (the game draws whatever is on screen); far past the
// fog on every map, and it keeps a map of hundreds of fly swarms cheap.
const SEEN: f32 = 300.0;

/// A glow's picture, alpha, colour and turn (degrees every second tick); its size is the object's first parameter, or 30.
const GLOWS: [(u32, &str, u8, [u8; 3], i32); 4] = [
    (693, "glow01.bmp", 50, [255; 3], 1),
    (694, "glow02.bmp", 50, [255; 3], 1),
    (701, "glow11.bmp", 250, [51, 153, 153], 1),
    (702, "glow12b .bmp", 50, [255; 3], 1),
];
const GLOW_SIZE: f32 = 30.0;

/// A torch's pictures (`<name>01.bmp` on), how many, the ticks each shows, and its alpha; its size is the object's
/// first parameter, or 20.
const TORCHES: [(u32, &str, u32, u32, u8); 4] =
    [(689, "dust", 9, 8, 100), (690, "torch_red", 13, 6, 130), (691, "torch_green", 13, 6, 130), (696, "torch_violet", 13, 6, 130)];
const TORCH_SIZE: f32 = 20.0;
/// A torch stands this far over its object.
const TORCH_RISE: f32 = 10.0;

/// The fly swarms: 713 and 714 yellow, 946 white; 714 is one fly that only rises and sinks.
const SWARMS: [(u32, [u8; 3]); 3] = [(713, [250, 250, 10]), (714, [250, 250, 10]), (946, [250; 3])];
const LONE: u32 = 714;
const FLY_SIZE: f32 = 2.4;
/// Each fly's x and z steps' signs, and whether it may sit on the edge (30) before turning back.
const FLY_STEPS: [([f32; 2], bool); 4] = [([1.0, 1.0], false), ([-1.0, -1.0], false), ([1.0, -1.0], true), ([-1.0, 1.0], true)];

const FIREFLY_TICKS: u32 = 140;
const SMOKE_TICKS: u32 = 50;
/// A puff shows this far over its object.
const SMOKE_RISE: f32 = 9.0;
const SMOKE_SCALE: f32 = 1.5;

/// Whether `id` is one of these looks when anything plays it (the smoke is a map object's only).
pub fn has(id: u32) -> bool {
    id == FIREFLY || GLOWS.iter().any(|glow| glow.0 == id) || TORCHES.iter().any(|torch| torch.0 == id) || SWARMS.iter().any(|swarm| swarm.0 == id)
}

fn sin(degrees: f32) -> f32 {
    degrees.trunc().to_radians().sin()
}

fn cos(degrees: f32) -> f32 {
    degrees.trunc().to_radians().cos()
}

/// A place in the world `offset` (map axes, y down) from `at`.
fn place(at: Vector, offset: [f32; 3]) -> Vector {
    [at[0] + offset[0], at[1] - offset[1], at[2] + offset[2]]
}

#[derive(Debug, Clone, PartialEq)]
pub struct Glow {
    pub picture: String,
    pub at: Vector,
    pub size: f32,
    alpha: u8,
    rgb: [u8; 3],
    turn: i32,
    /// Degrees, whole.
    pub angle: i32,
    phase: i32,
    pub ticks: u32,
}

impl Glow {
    fn step(&mut self) {
        self.ticks += 1;
        if self.ticks.is_multiple_of(2) {
            self.angle = (self.angle + self.turn).rem_euclid(360);
        }
        self.phase = (self.phase + 2) % 360;
    }

    /// Its half-diagonal now: it breathes by a twentieth, once every 180 ticks.
    pub fn reach(&self) -> f32 {
        self.size * (1.0 + 0.05 * sin(self.phase as f32))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Torch {
    name: &'static str,
    count: u32,
    every: u32,
    alpha: u8,
    pub at: Vector,
    pub size: f32,
    pub index: u32,
    pub ticks: u32,
}

impl Torch {
    fn step(&mut self) {
        self.ticks += 1;
        if self.ticks.is_multiple_of(self.every) {
            self.index = (self.index + 1) % self.count;
        }
    }

    pub fn picture(&self) -> String {
        format!("data\\texture\\effect\\{}{:02}.bmp", self.name, self.index + 1)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Fly {
    /// From the swarm's object, map axes.
    pub offset: [f32; 3],
    /// 0 to 120; drawn at twice that.
    pub alpha: i32,
    blink: u32,
    /// For x and for z: where the angle is and where it heads (degrees).
    angles: [[f32; 2]; 2],
}

/// An angle on its way to its target: 3° a tick up or 2° down, then a new target on the other side.
fn wander(angle: &mut [f32; 2], rand: &mut dyn FnMut(u32) -> u32) {
    let below = |at: f32, rand: &mut dyn FnMut(u32) -> u32| if at as u32 == 0 { 0 } else { rand(at as u32) };
    if angle[1] > angle[0] {
        angle[0] = (angle[0] + 3.0).min(359.0);
        if angle[0] > angle[1] {
            angle[1] = below(angle[0], rand) as f32;
        }
    } else {
        angle[0] = (angle[0] - 2.0).max(0.0);
        if angle[1] >= angle[0] {
            angle[1] = (359 - below(angle[0], rand)) as f32;
        }
    }
}

impl Fly {
    fn new(lone: bool, rand: &mut dyn FnMut(u32) -> u32) -> Self {
        let offset = if lone { [0.0; 3] } else { [rand(41) as f32 - 20.0, 0.0, rand(41) as f32 - 20.0] };
        Self { offset, alpha: 0, blink: 0, angles: [[rand(360) as f32, rand(360) as f32], [rand(360) as f32, rand(360) as f32]] }
    }

    /// Fades out past the edge (`out`) and starts again; inside, now and then it goes dark for a while. True when it
    /// has faded out and must start again.
    fn fade(&mut self, out: bool, blink_in: u32, rand: &mut dyn FnMut(u32) -> u32) -> bool {
        if out {
            self.alpha -= 2;
            return self.alpha <= 1;
        }
        if rand(blink_in) == 0 && self.alpha > 100 && self.blink == 0 {
            self.blink = 1;
        }
        if self.blink >= 1 {
            self.blink += 1;
            match self.blink {
                ..=90 => self.alpha = (self.alpha - 6).max(0),
                91..=119 => self.alpha = (self.alpha + 4).min(120),
                _ => self.blink = 0,
            }
        }
        false
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Swarm {
    pub at: Vector,
    rgb: [u8; 3],
    /// One fly that rises and sinks, in place of sixteen that roam.
    lone: bool,
    pub flies: Vec<Fly>,
    /// The lone fly's ticks since it last started.
    ticks: u32,
}

impl Swarm {
    fn new(at: Vector, rgb: [u8; 3], lone: bool, rand: &mut dyn FnMut(u32) -> u32) -> Self {
        let flies = (0..if lone { 1 } else { 16 }).map(|_| Fly::new(lone, rand)).collect();
        Self { at, rgb, lone, flies, ticks: 0 }
    }

    fn step(&mut self, rand: &mut dyn FnMut(u32) -> u32) {
        self.ticks += 1;
        for (number, fly) in self.flies.iter_mut().enumerate() {
            fly.alpha = (fly.alpha + 1).min(120);
            for angle in &mut fly.angles {
                wander(angle, rand);
            }
            if self.lone {
                fly.offset[1] += 0.03 * sin(fly.angles[0][0]);
                if fly.fade(self.ticks > 3000, 20, rand) {
                    (self.ticks, fly.offset) = (0, [0.0; 3]);
                }
                continue;
            }
            let (signs, edge) = FLY_STEPS[number % 4];
            fly.offset[0] += signs[0] * 0.05 * sin(fly.angles[0][0]);
            fly.offset[2] += signs[1] * 0.05 * sin(fly.angles[1][0]);
            // By whole units: x first, z only while x is inside.
            let far = match fly.offset[0].trunc().abs() {
                x if x >= 30.0 => x,
                _ => fly.offset[2].trunc().abs(),
            };
            if fly.fade(if edge { far > 30.0 } else { far >= 30.0 }, 125, rand) {
                fly.offset = [rand(31) as f32 - 15.0, 0.0, rand(31) as f32 - 15.0];
            }
        }
    }
}

/// The way (0, 0, 1) points turned about y by `yaw`, then about x by `pitch` (whole degrees), in the map's axes.
fn heading(yaw: f32, pitch: f32) -> [f32; 3] {
    [sin(yaw), -cos(yaw) * sin(pitch), cos(yaw) * cos(pitch)]
}

#[derive(Debug, Clone, PartialEq)]
pub struct Firefly {
    at: Vector,
    offset: [f32; 3],
    yaw: f32,
    pitch: f32,
    speed: f32,
    /// It turns on the ticks its age divides by this.
    turn_every: u32,
    pub scale: f32,
    pub age: u32,
    pub alpha: f32,
    most: f32,
    step: f32,
}

impl Firefly {
    fn new(at: Vector, rand: &mut dyn FnMut(u32) -> u32) -> Self {
        let speed = (rand(35) + 15) as f32 / 100.0;
        let (yaw, pitch) = (rand(360) as f32, rand(360) as f32);
        let out = (rand(13) + 2) as f32;
        let offset = heading(yaw, pitch).map(|axis| axis * out);
        let turn_every = rand(25) + 5;
        let scale = (rand(70) + 30) as f32 / 100.0;
        Self { at, offset: [offset[0], offset[1] - 10.0, offset[2]], yaw, pitch, speed, turn_every, scale, age: 0, alpha: 0.0, most: 80.0, step: 80.0 / 1.5 }
    }

    fn step(&mut self, rand: &mut dyn FnMut(u32) -> u32) {
        let heading = heading(self.yaw, self.pitch);
        self.offset = [0, 1, 2].map(|axis| self.offset[axis] + self.speed * heading[axis]);
        if self.age.is_multiple_of(self.turn_every) {
            self.speed = (rand(35) + 15) as f32 / 100.0;
            self.yaw += rand(80) as f32 - 40.0;
            self.pitch += rand(80) as f32 - 40.0;
            self.turn_every = rand(10) + 5;
        }
        // Dim for 40 ticks, bright for 60, dim again; the flicker turns back at each end.
        match self.age {
            40 => (self.alpha, self.most) = (120.0, 200.0),
            100 => (self.alpha, self.most) = (0.0, 80.0),
            _ => {}
        }
        if if self.step < 0.0 { self.alpha <= 0.0 } else { self.alpha >= self.most } {
            self.step = -self.step;
        }
        self.alpha = (self.alpha + self.step).clamp(0.0, self.most);
        self.age += 1;
    }

    pub fn place(&self) -> Vector {
        place(self.at, self.offset)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Puff {
    heading: [f32; 3],
    speed: f32,
    pub offset: [f32; 3],
    pub alpha: i32,
    pub age: u32,
    /// Degrees it has turned in its own plane.
    pub turn: f32,
    /// It has a sprite: a new puff waits its first 50 ticks without one.
    pub out: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Smoke {
    pub at: Vector,
    pub puffs: [Puff; 4],
    /// How many of the four show.
    pub shown: usize,
}

impl Smoke {
    fn step(&mut self, rand: &mut dyn FnMut(u32) -> u32) {
        for puff in &mut self.puffs {
            if puff.age < SMOKE_TICKS {
                if puff.out {
                    puff.offset = [0, 1, 2].map(|axis| puff.offset[axis] + puff.speed * puff.heading[axis]);
                    puff.turn += 0.75;
                    if puff.age > 34 {
                        puff.alpha = (puff.alpha - 15).max(0);
                    }
                }
            } else {
                let speed = (rand(3) + 3) as f32 / 10.0;
                let (tilt, round) = ((rand(30) + 75) as f32, rand(360) as f32);
                let heading = [cos(tilt) * sin(round), -sin(tilt), cos(tilt) * cos(round)];
                *puff = Puff { heading, speed, offset: [0.0; 3], alpha: 255, age: 0, turn: puff.turn, out: true };
                // One to four, in tenths: four only on the last of the 31 draws.
                self.shown = ((10 + rand(31)) / 10) as usize;
            }
            puff.age += 1;
        }
    }
}

/// A square to draw: its picture, middle, corners in sprite pixels, colour and alpha (0 to 255).
#[derive(Debug, Clone, PartialEq)]
pub struct Square {
    pub picture: String,
    pub at: Vector,
    pub corners: [[f32; 2]; 4],
    pub rgb: [u8; 3],
    pub alpha: f32,
}

/// A sprite to draw: where, how big, turned how far in its plane, and its alpha (0 to 255).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sprite {
    pub at: Vector,
    pub scale: f32,
    pub turn: f32,
    pub alpha: f32,
}

#[derive(Default)]
pub struct MapLooks {
    pub glows: Vec<Glow>,
    pub torches: Vec<Torch>,
    pub swarms: Vec<Swarm>,
    pub fireflies: Vec<Firefly>,
    pub smokes: Vec<Smoke>,
    /// The start of the tick under way.
    last: Option<Instant>,
    /// Ticks run; the smoke's sprite shows one frame for each.
    pub ticks: u32,
    /// Looks started since the map's load.
    pub started: usize,
}

impl MapLooks {
    /// Starts `id`'s look at `at`; `size` is the map object's first parameter (0 or less: the look's own).
    // ponytail: a glow, a torch or a swarm lasts 18 days, as in the game, and stays where it started: one played on a
    // unit doesn't follow it, and a second one played at a place adds to the first (the maps' own come once in hours).
    pub fn play(&mut self, id: u32, at: Vector, size: f32, rand: &mut dyn FnMut(u32) -> u32) {
        let sized = |own: f32| if size > 0.0 { size } else { own };
        self.started += usize::from(has(id) || id == SMOKE);
        if let Some(&(_, picture, alpha, rgb, turn)) = GLOWS.iter().find(|glow| glow.0 == id) {
            let picture = format!("data\\texture\\effect\\{picture}");
            self.glows.push(Glow { picture, at, size: sized(GLOW_SIZE), alpha, rgb, turn, angle: rand(360) as i32, phase: 0, ticks: 0 });
        } else if let Some(&(_, name, count, every, alpha)) = TORCHES.iter().find(|torch| torch.0 == id) {
            let at = [at[0], at[1] + TORCH_RISE, at[2]];
            self.torches.push(Torch { name, count, every, alpha, at, size: sized(TORCH_SIZE), index: 0, ticks: 0 });
        } else if let Some(&(_, rgb)) = SWARMS.iter().find(|swarm| swarm.0 == id) {
            self.swarms.push(Swarm::new(at, rgb, id == LONE, rand));
        } else if id == FIREFLY {
            self.fireflies.push(Firefly::new(at, rand));
        } else if id == SMOKE {
            self.smokes.push(Smoke { at, puffs: Default::default(), shown: 1 });
        }
    }

    /// The pictures of a look to load before it shows.
    pub fn pictures(id: u32) -> Vec<String> {
        let path = |name: &str| format!("data\\texture\\effect\\{name}");
        if let Some(glow) = GLOWS.iter().find(|glow| glow.0 == id) {
            vec![path(glow.1)]
        } else if let Some(&(_, name, count, ..)) = TORCHES.iter().find(|torch| torch.0 == id) {
            (1..=count).map(|number| path(&format!("{name}{number:02}.bmp"))).collect()
        } else if SWARMS.iter().any(|swarm| swarm.0 == id) {
            vec![FLY_TEXTURE.to_owned()]
        } else {
            Vec::new()
        }
    }

    /// Runs the 16 ms ticks passed since the last call. The smoke waits while the player (at `player`) is far.
    pub fn advance(&mut self, now: Instant, player: Vector, rand: &mut dyn FnMut(u32) -> u32) {
        let Some(last) = self.last else {
            self.last = Some(now);
            return;
        };
        let mut due = (now.saturating_duration_since(last).as_millis() / TICK.as_millis()) as u32;
        self.last = Some(last + TICK * due);
        if due > MOST_TICKS {
            (due, self.last) = (MOST_TICKS, Some(now));
        }
        for _ in 0..due {
            self.ticks += 1;
            self.glows.iter_mut().for_each(Glow::step);
            self.torches.iter_mut().for_each(Torch::step);
            self.swarms.iter_mut().for_each(|swarm| swarm.step(rand));
            self.fireflies.iter_mut().for_each(|firefly| firefly.step(rand));
            self.smokes.iter_mut().filter(|smoke| distance(smoke.at, player) < SMOKE_RANGE).for_each(|smoke| smoke.step(rand));
        }
        self.fireflies.retain(|firefly| firefly.age < FIREFLY_TICKS);
    }

    /// The squares to draw, all added onto what's behind: glows, torches and flies, in that order.
    pub fn squares(&self, player: Vector) -> Vec<Square> {
        let seen = |at: Vector| distance(at, player) < SEEN;
        let mut squares = Vec::new();
        for glow in self.glows.iter().filter(|glow| glow.ticks > 0 && seen(glow.at)) {
            let corners = flake_corners(glow.reach(), glow.angle as f32);
            squares.push(Square { picture: glow.picture.clone(), at: glow.at, corners, rgb: glow.rgb, alpha: f32::from(glow.alpha) });
        }
        for torch in self.torches.iter().filter(|torch| seen(torch.at)) {
            let corners = flake_corners(torch.size, 135.0);
            squares.push(Square { picture: torch.picture(), at: torch.at, corners, rgb: [255; 3], alpha: f32::from(torch.alpha) });
        }
        for swarm in self.swarms.iter().filter(|swarm| seen(swarm.at)) {
            for fly in swarm.flies.iter().filter(|fly| fly.alpha > 0) {
                let corners = flake_corners(FLY_SIZE, 45.0);
                squares.push(Square {
                    picture: FLY_TEXTURE.to_owned(),
                    at: place(swarm.at, fly.offset),
                    corners,
                    rgb: swarm.rgb,
                    alpha: (2 * fly.alpha) as f32,
                });
            }
        }
        squares
    }

    /// The fireflies to draw.
    pub fn firefly_sprites(&self, player: Vector) -> Vec<Sprite> {
        let lit = self.fireflies.iter().filter(|firefly| firefly.alpha > 0.0 && distance(firefly.at, player) < SEEN);
        lit.map(|firefly| Sprite { at: firefly.place(), scale: firefly.scale, turn: 0.0, alpha: firefly.alpha }).collect()
    }

    /// The smoke puffs to draw: the first few of each chimney near the player.
    pub fn smoke_sprites(&self, player: Vector) -> Vec<Sprite> {
        let mut sprites = Vec::new();
        for smoke in self.smokes.iter().filter(|smoke| distance(smoke.at, player) < SMOKE_RANGE) {
            for puff in smoke.puffs.iter().take(smoke.shown).filter(|puff| puff.out && puff.alpha > 0) {
                let offset = [puff.offset[0], puff.offset[1] - SMOKE_RISE, puff.offset[2]];
                sprites.push(Sprite { at: place(smoke.at, offset), scale: SMOKE_SCALE, turn: puff.turn, alpha: puff.alpha as f32 });
            }
        }
        sprites
    }

    /// How many of each look are running, for the test driver.
    pub fn counts(&self) -> [(&'static str, usize); 6] {
        [
            ("started", self.started),
            ("glows", self.glows.len()),
            ("torches", self.torches.len()),
            ("swarms", self.swarms.len()),
            ("fireflies", self.fireflies.len()),
            ("smokes", self.smokes.len()),
        ]
    }
}

fn distance(a: Vector, b: Vector) -> f32 {
    [0, 1, 2].map(|axis| (a[axis] - b[axis]).powi(2)).iter().sum::<f32>().sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    const HERE: Vector = [100.0, 5.0, 200.0];

    /// A look started with draws that are all `draw` (capped under each range), run for `ticks`.
    fn run(id: u32, size: f32, draw: u32, ticks: u32) -> MapLooks {
        let mut rand = |range: u32| draw.min(range - 1);
        let start = Instant::now();
        let mut looks = MapLooks::default();
        looks.play(id, HERE, size, &mut rand);
        (0..=ticks).for_each(|tick| looks.advance(start + TICK * tick, HERE, &mut rand));
        looks
    }

    #[test]
    fn knows_its_ids() {
        for id in [45, 689, 690, 691, 693, 694, 696, 701, 702, 713, 714, 946] {
            assert!(has(id), "{id}");
        }
        // The smoke is the map's own; the other glows and the green light aren't made yet.
        assert!([44, 47, 695, 703, 710, 974].iter().all(|&id| !has(id)));
    }

    #[test]
    fn a_glow_turns_and_breathes() {
        // Sized by the object's parameter, or 30; unseen until its first tick.
        assert_eq!(run(693, 0.0, 10, 0).glows[0].size, 30.0);
        assert!(run(693, 0.0, 10, 0).squares(HERE).is_empty());
        let looks = run(701, 12.0, 10, 45);
        let glow = &looks.glows[0];
        // A degree every second tick from its start angle; the breath is at its widest at tick 45 (phase 90°).
        assert_eq!((glow.size, glow.angle, glow.ticks), (12.0, 32, 45));
        assert!((glow.reach() - 12.6).abs() < 1e-4);
        let squares = looks.squares(HERE);
        assert_eq!((squares[0].picture.as_str(), squares[0].rgb, squares[0].alpha), ("data\\texture\\effect\\glow11.bmp", [51, 153, 153], 250.0));
        // The square's corners are its half-diagonal away, in sprite pixels.
        assert!(squares[0].corners.iter().all(|corner| (f32::hypot(corner[0], corner[1]) - 12.6 * 7.0).abs() < 1e-3));
        assert_eq!(run(693, 0.0, 0, 5).squares(HERE)[0].alpha, 50.0);
        assert_eq!(MapLooks::pictures(702), ["data\\texture\\effect\\glow12b .bmp"]);
    }

    #[test]
    fn a_torch_shows_its_pictures_in_turn() {
        // 10 over its object, 20 big unless the object says; a picture every 6 ticks, 13 of them, then again.
        let torch = |ticks| run(690, 0.0, 0, ticks).torches[0].clone();
        assert_eq!((torch(0).at, torch(0).size), ([100.0, 15.0, 200.0], 20.0));
        assert_eq!([0, 5, 6, 77, 78].map(|ticks| torch(ticks).index), [0, 0, 1, 12, 0]);
        assert_eq!(torch(6).picture(), "data\\texture\\effect\\torch_red02.bmp");
        // The dust has 9, one every 8 ticks, fainter.
        let dust = run(689, 7.0, 0, 8 * 9 - 1);
        assert_eq!((dust.torches[0].index, dust.torches[0].size, dust.squares(HERE)[0].alpha), (8, 7.0, 100.0));
        assert_eq!(MapLooks::pictures(691).len(), 13);
        assert_eq!(MapLooks::pictures(696)[12], "data\\texture\\effect\\torch_violet13.bmp");
    }

    #[test]
    fn an_angle_wanders_up_and_down() {
        // Up 3° a tick; past its target, a new one below (here the highest: one under where it is).
        let mut top = |range: u32| range - 1;
        let mut angle = [10.0, 14.0];
        wander(&mut angle, &mut top);
        assert_eq!(angle, [13.0, 14.0]);
        wander(&mut angle, &mut top);
        assert_eq!(angle, [16.0, 15.0]);
        // Down 2° a tick; at its target, a new one above: 359 less a draw under where it is.
        wander(&mut angle, &mut top);
        assert_eq!(angle, [14.0, 359.0 - 13.0]);
        // Never over 359 or under 0; at 0 the new target is 359.
        let mut ends = [[358.0, 359.0], [1.0, 0.0]];
        ends.iter_mut().for_each(|angle| wander(angle, &mut top));
        assert_eq!(ends, [[359.0, 359.0], [0.0, 359.0]]);
    }

    #[test]
    fn flies_roam_fade_at_the_edge_and_blink() {
        // Sixteen flies, yellow (946: white), each within 20 of the object, brightening by 2 of 255 a tick.
        let looks = run(713, 0.0, 5, 30);
        let swarm = &looks.swarms[0];
        assert_eq!((swarm.flies.len(), swarm.rgb), (16, [250, 250, 10]));
        assert!(swarm.flies.iter().all(|fly| fly.alpha == 30 && fly.offset[1] == 0.0));
        assert!(swarm.flies.iter().all(|fly| (fly.offset[0] + 15.0).abs() < 1.6 && (fly.offset[2] + 15.0).abs() < 1.6));
        let squares = looks.squares(HERE);
        assert_eq!((squares.len(), squares[0].alpha, squares[0].picture.as_str()), (16, 60.0, FLY_TEXTURE));
        assert_eq!(run(946, 0.0, 5, 1).swarms[0].rgb, [250; 3]);
        // Past the edge a fly dims by 2 a tick and starts again within 15 of the object once it's out.
        let mut none = |range: u32| range - 1;
        let mut fly = Fly { alpha: 5, ..Fly::default() };
        assert!(!fly.fade(true, 125, &mut none));
        assert!(fly.fade(true, 125, &mut none));
        // A blink: 6 darker a tick to tick 90, 4 brighter to 119, over at 120. It starts only from over 100.
        let mut first = |_: u32| 0;
        let mut fly = Fly { alpha: 100, ..Fly::default() };
        fly.fade(false, 125, &mut first);
        assert_eq!((fly.alpha, fly.blink), (100, 0));
        fly.alpha = 120;
        fly.fade(false, 125, &mut first);
        assert_eq!((fly.alpha, fly.blink), (114, 2));
        for _ in 0..88 {
            fly.fade(false, 125, &mut none);
        }
        assert_eq!((fly.alpha, fly.blink), (0, 90));
        for _ in 0..29 {
            fly.fade(false, 125, &mut none);
        }
        assert_eq!((fly.alpha, fly.blink), (116, 119));
        fly.fade(false, 125, &mut none);
        assert_eq!(fly.blink, 0);
    }

    #[test]
    fn the_lone_fly_only_rises_and_sinks() {
        let looks = run(714, 0.0, 90, 20);
        let swarm = &looks.swarms[0];
        assert_eq!(swarm.flies.len(), 1);
        assert_eq!((swarm.flies[0].offset[0], swarm.flies[0].offset[2]), (0.0, 0.0));
        assert!(swarm.flies[0].offset[1] != 0.0 && swarm.flies[0].offset[1].abs() <= 0.03 * 20.0);
    }

    #[test]
    fn a_firefly_flickers_brightens_and_ends() {
        // Starts 2 to 14 from a point 10 over the object.
        let firefly = Firefly::new(HERE, &mut |_| 0);
        assert_eq!((firefly.offset, firefly.scale, firefly.turn_every), ([0.0, -10.0, 2.0], 0.3, 5));
        // Alpha 53, 80, 27, 0 and round again; at tick 40 it jumps to 120 on its way down, then swings up to 200; from
        // 100 dim again.
        let mut firefly = Firefly::new(HERE, &mut |_| 0);
        let mut alphas = Vec::new();
        for _ in 0..FIREFLY_TICKS {
            firefly.step(&mut |range| range / 2);
            alphas.push(firefly.alpha.round() as i32);
        }
        assert_eq!(alphas[..6], [53, 80, 27, 0, 53, 80]);
        assert_eq!(alphas[40..47], [67, 13, 0, 53, 107, 160, 200]);
        assert!(alphas[..40].iter().all(|&alpha| alpha <= 80) && alphas[100..].iter().all(|&alpha| alpha <= 80));
        assert_eq!(alphas[40..100].iter().max(), Some(&200));
        // It flies at 0.15 to 0.49 a tick, and is gone after 140 ticks.
        assert!(distance(firefly.place(), HERE) < 14.0 + 10.0 + 0.5 * 140.0);
        assert_eq!(run(45, 0.0, 3, 139).fireflies.len(), 1);
        let gone = run(45, 0.0, 3, 140);
        assert!(gone.fireflies.is_empty() && gone.started == 1);
        // Up in the map's axes is down: straight ahead with no pitch is +z; a quarter pitch points up.
        assert_eq!(heading(0.0, 0.0), [0.0, -0.0, 1.0]);
        assert!((heading(0.0, 90.0)[1] + 1.0).abs() < 1e-6);
    }

    #[test]
    fn smoke_puffs_rise_fade_and_start_again_together() {
        // Nothing for the first 50 ticks; then the puffs start, 9 over the chimney.
        assert!(run(SMOKE, 0.0, 0, 50).smoke_sprites(HERE).is_empty());
        let looks = run(SMOKE, 0.0, 15, 51);
        let smoke = &looks.smokes[0];
        // A draw of 15 of 31: 2.5, so two show; of 30: tilt 90°, straight up at 0.5 a tick (the draw is capped at 2 of 3).
        assert_eq!(smoke.shown, 2);
        assert!(smoke.puffs.iter().all(|puff| puff.out && puff.alpha == 255 && puff.age == 1));
        let sprites = looks.smoke_sprites(HERE);
        assert_eq!(sprites.len(), 2);
        assert_eq!((sprites[0].at, sprites[0].scale), ([100.0, 14.0, 200.0], 1.5));
        // 49 moves and 15 fades of 15 by the end of its 50 ticks; it has turned 0.75° a tick.
        let old = run(SMOKE, 0.0, 15, 100).smokes[0].puffs[0].clone();
        assert_eq!((old.age, old.alpha, old.turn), (50, 30, 49.0 * 0.75));
        assert!((old.offset[1] + 49.0 * 0.5).abs() < 1e-3 && old.offset[0].abs() < 1e-3);
        // Then it starts again from the chimney.
        assert_eq!(run(SMOKE, 0.0, 15, 101).smokes[0].puffs[0].offset, [0.0; 3]);
        // Far from the player it neither runs nor shows.
        let mut far = MapLooks::default();
        let start = Instant::now();
        far.play(SMOKE, HERE, 0.0, &mut |_| 0);
        far.advance(start, [300.0, 5.0, 200.0], &mut |_| 0);
        far.advance(start + TICK * 60, [300.0, 5.0, 200.0], &mut |_| 0);
        assert_eq!(far.smokes[0].puffs[0].age, 0);
        assert!(far.smoke_sprites([300.0, 5.0, 200.0]).is_empty());
    }

    #[test]
    fn catches_up_in_whole_ticks() {
        let start = Instant::now();
        let mut looks = MapLooks::default();
        looks.play(690, HERE, 0.0, &mut |_| 0);
        looks.advance(start, HERE, &mut |_| 0);
        // 40 ms: two ticks, 8 ms kept for the next frame.
        looks.advance(start + std::time::Duration::from_millis(40), HERE, &mut |_| 0);
        assert_eq!(looks.torches[0].ticks, 2);
        looks.advance(start + std::time::Duration::from_millis(48), HERE, &mut |_| 0);
        assert_eq!(looks.torches[0].ticks, 3);
        // A long pause runs 64 ticks at most.
        looks.advance(start + std::time::Duration::from_secs(60), HERE, &mut |_| 0);
        assert_eq!(looks.torches[0].ticks, 3 + MOST_TICKS);
        // Nothing far from the player is drawn.
        assert!(looks.squares([500.0, 5.0, 200.0]).is_empty());
        assert_eq!(looks.squares(HERE).len(), 1);
    }
}
