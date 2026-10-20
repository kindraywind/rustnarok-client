//! Manhole's and Dimension Door's ground (822, 825), a 16 ms tick at a time: a blue swirl square turning on the cell and, every 100
//! ticks, three breathing flat ring bands growing out from its centre.
//!
//! A shape's own age n counts from 1 at its first tick; e is the ticks since the effect started.

use std::time::Instant;

use super::sage::{Corner, ring_point, ring_quads};
use super::walls::{RING_WHITE, Wall};
use super::{TICK, passes};
use crate::world::{MeshVertex, Vector};

pub const TWIRL: &str = "data\\texture\\effect\\twirl.bmp";
pub const SPHERE: &str = "data\\texture\\effect\\구체15.bmp";
pub const SURA_CIRCLE: &str = "data\\texture\\effect\\cir0002.bmp";
/// The swirl lies 2 over the ground; a hole lives 29 999 ticks.
const RISE: f32 = 2.0;
const LIFE: u32 = 29_999;
/// A ring triple is seen for prim ages 0 to 135.
const RING_AGES: u32 = 136;
const LEANS: [f32; 3] = [2.0, 3.0, 4.0];
const TURNS: [f32; 3] = [0.0, 25.0, 50.0];

/// What a ground unit's cell looks like: the squares it turns, the colour of its ring triples and whether it throws them.
pub enum Look {
    Manhole,
    DimensionDoor,
    ChaosPanic,
    Maelstrom,
    BloodyLust,
}

/// Chaos Panic's pictures (dust01 to dust09) and Bloody Lust's (torch_red01 to torch_red13), one every 4 ticks.
pub const DUST: [&str; 9] = [
    "data\\texture\\effect\\dust01.bmp",
    "data\\texture\\effect\\dust02.bmp",
    "data\\texture\\effect\\dust03.bmp",
    "data\\texture\\effect\\dust04.bmp",
    "data\\texture\\effect\\dust05.bmp",
    "data\\texture\\effect\\dust06.bmp",
    "data\\texture\\effect\\dust07.bmp",
    "data\\texture\\effect\\dust08.bmp",
    "data\\texture\\effect\\dust09.bmp",
];
pub const TORCH: [&str; 13] = [
    "data\\texture\\effect\\torch_red01.bmp",
    "data\\texture\\effect\\torch_red02.bmp",
    "data\\texture\\effect\\torch_red03.bmp",
    "data\\texture\\effect\\torch_red04.bmp",
    "data\\texture\\effect\\torch_red05.bmp",
    "data\\texture\\effect\\torch_red06.bmp",
    "data\\texture\\effect\\torch_red07.bmp",
    "data\\texture\\effect\\torch_red08.bmp",
    "data\\texture\\effect\\torch_red09.bmp",
    "data\\texture\\effect\\torch_red10.bmp",
    "data\\texture\\effect\\torch_red11.bmp",
    "data\\texture\\effect\\torch_red12.bmp",
    "data\\texture\\effect\\torch_red13.bmp",
];
/// Maelstrom's three squares' radii: flare, ring 1, ring 2.
const MAELSTROM_RADII: [f32; 3] = [30.0, 28.0, 20.0];

/// The picture of a square at its tick a.
#[derive(Clone, Copy)]
enum Picture {
    One(&'static str),
    /// A new one every 4 ticks from a = 3 among these.
    Moving(&'static [&'static str]),
    /// Maelstrom's: index into `MAELSTROM`, the folder by /mineffect.
    Maelstrom(usize, bool),
}

/// A square of kind 39 or 143 on the ground: its picture, start turn and spin, base radius, whether it breathes and how fast it turns.
struct Square {
    picture: Picture,
    turn: f32,
    spin: f32,
    base: f32,
    breathes: bool,
    step: f32,
}

/// A cell of a ground skill unit.
struct Hole {
    id: u32,
    at: Vector,
    start: Instant,
    squares: Vec<Square>,
    /// The colour of the ring triples thrown every 100 ticks, if it throws any.
    rings: Option<[f32; 3]>,
    /// How many ticks have started a ring triple's check.
    ticks: u32,
}

/// A ring triple made at `start`.
struct Triple {
    start: Instant,
    at: Vector,
    rgb: [f32; 3],
}

/// Aura 831's two flat squares on a Sura's feet.
struct Squares {
    unit: Option<u32>,
    start: Instant,
    length: u32,
}

#[derive(Default)]
pub struct Holes {
    holes: Vec<Hole>,
    triples: Vec<Triple>,
    squares: Vec<Squares>,
}

/// One ring band at a prim age: whether it still runs, its own age, radius, alpha (whole) and profile height.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Band {
    pub active: bool,
    pub age: i32,
    pub radius: f32,
    pub alpha: i32,
    pub height: f32,
}

/// A square's half-diagonal at spin `spin` for a base radius `base`: 0.8 to 1.0 of it, a 120-tick breath (9.6 to 12.0 for 12).
pub fn swirl_half(spin: f32, base: f32) -> f32 {
    (spin.to_radians().sin() + 1.0) * 0.1 * base + 0.8 * base
}

/// The three bands after the prim's tick `a` (a + 1 ticks run): each starts at its own age (0, −10, −20), grows 0.5 a tick, fades
/// from radius 7, and starts again until band 0 has run 120 ticks.
pub fn bands(a: u32) -> [Band; 3] {
    let mut bands = [0, -10, -20].map(|age| Band { active: true, age, radius: 0.0, alpha: 0, height: 0.0 });
    let mut counter = 0;
    for _ in 0..=a {
        for (k, band) in bands.iter_mut().enumerate() {
            if !band.active {
                continue;
            }
            band.age += 1;
            if k == 0 {
                counter += 1;
            }
            if band.age <= 0 {
                continue;
            }
            band.radius += 0.5;
            if band.radius > 7.0 {
                band.alpha -= 5;
                if band.alpha < 0 {
                    band.alpha = 0;
                    if counter > 120 {
                        band.active = false;
                        continue;
                    }
                    band.age = 0;
                    band.radius = 0.0;
                }
            }
            if band.age < 10 {
                band.alpha = (band.alpha + 4).min(40);
            }
            band.height = 6.001 * if band.age <= 10 { (9.0 * band.age as f32).to_radians().sin() } else { 1.0 };
        }
    }
    bands
}

/// Every picture a `look` can show, for loading (`min` is /mineffect).
pub fn textures(look: &Look, min: bool) -> Vec<&'static str> {
    match look {
        Look::Manhole => vec![TWIRL],
        Look::DimensionDoor => vec![SPHERE],
        Look::ChaosPanic => DUST.to_vec(),
        Look::Maelstrom => (0..3).map(|index| maelstrom_path(index, min)).collect(),
        Look::BloodyLust => TORCH.to_vec(),
    }
}

/// The picture a moving square shows at its tick a.
fn picture_at(picture: Picture, a: u32) -> &'static str {
    match picture {
        Picture::One(path) => path,
        Picture::Moving(list) => list[((a + 1) / 4) as usize % list.len()],
        Picture::Maelstrom(index, min) => maelstrom_path(index, min),
    }
}

/// A Maelstrom square's file.
fn maelstrom_path(index: usize, min: bool) -> &'static str {
    // ponytail: the four paths are written out once so they stay 'static.
    const NORMAL: [&str; 3] = [
        "data\\texture\\effect\\new_maelstrom\\new_maelstrom\\mae_flare_00.bmp",
        "data\\texture\\effect\\new_maelstrom\\new_maelstrom\\mae_ring_00.tga",
        "data\\texture\\effect\\new_maelstrom\\new_maelstrom\\mae_ring_01.tga",
    ];
    const MIN: [&str; 3] = [
        "data\\texture\\effect\\mineffect\\new_maelstrom\\new_maelstrom\\mae_flare_00.bmp",
        "data\\texture\\effect\\mineffect\\new_maelstrom\\new_maelstrom\\mae_ring_00.tga",
        "data\\texture\\effect\\mineffect\\new_maelstrom\\new_maelstrom\\mae_ring_01.tga",
    ];
    if min { MIN[index] } else { NORMAL[index] }
}

/// The flare's radius after n ticks: 0.3 less while the turn (3 a tick) is under 180 or at 0, 0.3 more over it.
pub fn flare_radius(n: u32) -> f32 {
    let (periods, m) = (n / 120, n % 120);
    let shrinking = m.min(60) as f32;
    let growing = m.saturating_sub(60).min(59) as f32;
    MAELSTROM_RADII[0] - 0.6 * periods as f32 - 0.3 * shrinking + 0.3 * growing
}

impl Square {
    fn new(picture: Picture, turn: f32, spin: f32, base: f32, breathes: bool, step: f32) -> Self {
        Self { picture, turn, spin, base, breathes, step }
    }

    /// Its half-diagonal after n ticks.
    fn half(&self, n: u32) -> f32 {
        if self.breathes { swirl_half((self.spin + 3.0 * n as f32) % 360.0, self.base) } else { 0.9 * self.base }
    }
}

impl Holes {
    /// A ground unit's cell `look` at `at`, standing as skill unit `id`; its random choices from `rand(n)`, `min` is /mineffect.
    pub fn keep(&mut self, id: u32, look: Look, at: Vector, now: Instant, min: bool, mut rand: impl FnMut(u32) -> u32) {
        let blue = [100.0 / 255.0, 100.0 / 255.0, 1.0];
        let (squares, rings) = match look {
            Look::Manhole | Look::DimensionDoor => {
                let spin = rand(360) as f32;
                let _ = rand(360);
                let path = if matches!(look, Look::Manhole) { TWIRL } else { SPHERE };
                (vec![Square::new(Picture::One(path), 0.0, spin, 12.0, true, 6.0)], Some(blue))
            }
            Look::ChaosPanic => {
                let spin = rand(360) as f32;
                (vec![Square::new(Picture::Moving(&DUST), 0.0, spin, 18.0, true, 6.0)], Some([1.0, 1.0, 170.0 / 255.0]))
            }
            Look::Maelstrom => {
                let squares = (0..3)
                    .map(|index| {
                        let base = MAELSTROM_RADII[index];
                        Square::new(Picture::Maelstrom(index, min), 0.0, 0.0, base, false, 3.0)
                    })
                    .collect();
                (squares, None)
            }
            Look::BloodyLust => {
                let squares = [0.0, 120.0, 240.0].map(|turn| Square::new(Picture::Moving(&TORCH), turn, rand(360) as f32, 18.0, true, 6.0)).into();
                (squares, Some([1.0, 100.0 / 255.0, 100.0 / 255.0]))
            }
        };
        self.holes.push(Hole { id, at, start: now, squares, rings, ticks: 0 });
    }

    /// Aura 831's two flat squares on `unit` for a cast of `length` ticks.
    pub fn squares(&mut self, unit: Option<u32>, length: u32, now: Instant) {
        self.squares.retain(|squares| squares.unit != unit);
        self.squares.push(Squares { unit, start: now, length: length.max(120) });
    }

    pub fn forget(&mut self, id: u32) {
        self.holes.retain(|hole| hole.id != id);
    }

    /// Starts a ring triple on every 100th tick of each hole that throws them and drops what ended.
    pub fn advance(&mut self, now: Instant) {
        for hole in &mut self.holes {
            while hole.ticks < passes(hole.start, now) {
                if let (0, Some(rgb)) = (hole.ticks % 100, hole.rings) {
                    self.triples.push(Triple { start: hole.start + TICK * hole.ticks, at: hole.at, rgb });
                }
                hole.ticks += 1;
            }
        }
        self.holes.retain(|hole| passes(hole.start, now) <= LIFE);
        self.triples.retain(|triple| passes(triple.start, now) <= RING_AGES);
        self.squares.retain(|squares| passes(squares.start, now) <= squares.length);
    }

    /// The squares, ring bands and Sura squares at `now`, added on; `position_of` gives a unit's feet.
    pub fn meshes(&self, now: Instant, position_of: impl Fn(Option<u32>) -> Option<Vector>) -> Vec<Wall> {
        let mut walls = Vec::new();
        for hole in &self.holes {
            let n = passes(hole.start, now);
            if n == 0 {
                continue;
            }
            let a = n - 1;
            for square in &hole.squares {
                let turn = (square.turn + square.step * n as f32) % 360.0;
                let half = if matches!(square.picture, Picture::Maelstrom(0, _)) { 0.9 * flare_radius(n) } else { square.half(n) };
                let tint = [1.0, 1.0, 1.0, 200.0 / 255.0];
                let corner = |j: u32, uv: [f32; 2]| {
                    let (sin, cos) = (turn + 90.0 * j as f32).to_radians().sin_cos();
                    MeshVertex { position: [hole.at[0] + half * cos, hole.at[1] + RISE, hole.at[2] + half * sin], uv, tint }
                };
                let [p, q, r, s] = [corner(0, [0.0, 1.0]), corner(1, [1.0, 1.0]), corner(2, [1.0, 0.0]), corner(3, [0.0, 0.0])];
                walls.push(Wall { texture: picture_at(square.picture, a), vertices: vec![p, q, r, p, r, s], additive: true, forced: false });
            }
        }
        for triple in &self.triples {
            let Some(a) = passes(triple.start, now).checked_sub(1).filter(|&a| a < RING_AGES) else { continue };
            let base = [triple.at[0], triple.at[1] + RISE, triple.at[2]];
            let mut vertices = Vec::new();
            for (k, band) in bands(a).iter().enumerate() {
                if !band.active || band.age <= 0 || band.radius <= 0.0 {
                    continue;
                }
                let points: Vec<_> = (0..=20).map(|s| ring_point(base, TURNS[k] + 18.0 * s as f32, band.radius, band.height, LEANS[k])).collect();
                let tint = [triple.rgb[0], triple.rgb[1], triple.rgb[2], band.alpha as f32 / 255.0];
                vertices.extend(corners_to_vertices(ring_quads(&points, 1.0), tint));
            }
            if !vertices.is_empty() {
                walls.push(Wall { texture: RING_WHITE, vertices, additive: true, forced: false });
            }
        }
        for squares in &self.squares {
            let (Some(feet), n) = (position_of(squares.unit), passes(squares.start, now)) else { continue };
            if n == 0 || n > squares.length {
                continue;
            }
            let alpha = square_alpha(squares.length, n);
            for (spin, corners) in [(0.0, [0.0, 90.0, 180.0, 270.0]), (180.0, [23.0, 113.0, 203.0, 293.0])] {
                let half = sura_half((spin + 3.0 * n as f32) % 360.0);
                let tint = [35.0 / 255.0, 35.0 / 255.0, 1.0, alpha / 255.0];
                let corner = |angle: f32, uv: [f32; 2]| {
                    let (sin, cos) = angle.to_radians().sin_cos();
                    MeshVertex { position: [feet[0] + half * cos, feet[1] + 1.0, feet[2] + half * sin], uv, tint }
                };
                let [p, q, r, s] =
                    [corner(corners[0], [0.0, 1.0]), corner(corners[1], [1.0, 1.0]), corner(corners[2], [1.0, 0.0]), corner(corners[3], [0.0, 0.0])];
                walls.push(Wall { texture: SURA_CIRCLE, vertices: vec![p, q, r, p, r, s], additive: true, forced: false });
            }
        }
        walls
    }
}

/// A Sura cast's square half-diagonal at spin `spin`: 19.2 to 24.0.
pub fn sura_half(spin: f32) -> f32 {
    (spin.to_radians().sin() + 1.0) * 2.4 + 19.2
}

/// A Sura cast's squares' alpha at age A of a `length`-tick effect: 200, then 8 less a tick over the last 25.
pub fn square_alpha(length: u32, a: u32) -> f32 {
    (200.0 - 8.0 * a.saturating_sub(length.saturating_sub(25)) as f32).max(0.0)
}

/// A ring's corners as vertices tinted `tint` (its own opacity taken as given).
fn corners_to_vertices(corners: Vec<Corner>, tint: [f32; 4]) -> Vec<MeshVertex> {
    corners.into_iter().map(|(position, uv, _)| MeshVertex { position, uv, tint }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(got: f32, want: f32) -> bool {
        (got - want).abs() < 0.01
    }

    #[test]
    fn the_ring_triple_breathes_out_and_ends_in_136_ticks() {
        // Band 0 at n = 1, 9, 10, 14, 15, 21, 22: r 0.5, 4.5, 5.0, 7.0, 7.5, 10.5, 0; alpha 4, 36, 36, 36, 31, 1, 4; p 0.94 … 0.
        let first = |n: u32| bands(n - 1)[0];
        assert_eq!([1, 9, 10, 14, 15, 21, 22].map(|n| first(n).radius), [0.5, 4.5, 5.0, 7.0, 7.5, 10.5, 0.0]);
        assert_eq!([1, 9, 10, 14, 15, 21, 22].map(|n| first(n).alpha), [4, 36, 36, 36, 31, 1, 4]);
        assert!(
            near(first(1).height, 0.94) && near(first(9).height, 5.93) && near(first(10).height, 6.0) && near(first(21).height, 6.0) && first(22).height == 0.0
        );
        // The second cycle: alpha at n = 9, 14, 15, 22, 23 (of its own age): 40, 40, 35, 0, 4.
        let second = |n: u32| bands(22 + n - 1)[0];
        assert_eq!([9, 14, 15, 22, 23].map(|n| second(n).alpha), [40, 40, 35, 0, 4]);
        // Bands 1 and 2 show nothing before prim ages 10 and 20; band 0 ends at prim age 136, band 1 at 123, band 2 at 133.
        assert!(bands(9)[1].age <= 0 && bands(10)[1].age == 1 && bands(19)[2].age <= 0 && bands(20)[2].age == 1);
        assert!(bands(135)[0].active && !bands(136)[0].active);
        assert!(bands(122)[1].active && !bands(123)[1].active);
        assert!(bands(132)[2].active && !bands(133)[2].active);
    }

    #[test]
    fn a_hole_turns_a_breathing_swirl_and_throws_ring_triples_every_100_ticks() {
        let start = Instant::now();
        let mut holes = Holes::default();
        holes.keep(4, Look::Manhole, [10.0, 1.0, 10.0], start, false, |_| 0);
        holes.keep(5, Look::DimensionDoor, [20.0, 1.0, 20.0], start, false, |_| 0);
        // The swirl: 9.6 … 12.0; one ring prim at e = 0, 100, 200 and none between.
        assert!(near(swirl_half(90.0, 12.0), 12.0) && near(swirl_half(270.0, 12.0), 9.6));
        for tick in 0..=205 {
            holes.advance(start + TICK * tick);
            let triples = holes.triples.len();
            assert!(triples <= 4 * 2, "tick {tick}");
        }
        assert_eq!(holes.triples.iter().filter(|triple| triple.at[0] == 10.0).count(), 2);
        // The swirls: twirl for Manhole, sphere for the door; white at alpha 200, added on, 2 up; turned 6° a tick, a turn in 60.
        let walls = holes.meshes(start + TICK * 3, |_| None);
        assert_eq!(walls[0].texture, TWIRL);
        assert_eq!(walls[1].texture, SPHERE);
        assert!(walls[..2].iter().all(|wall| wall.additive && wall.vertices.len() == 6 && wall.vertices[0].tint == [1.0, 1.0, 1.0, 200.0 / 255.0]));
        assert!(near(walls[0].vertices[0].position[1], 3.0));
        let turned = |tick: u32| {
            let wall = &holes.meshes(start + TICK * tick, |_| None)[0];
            let vertex = &wall.vertices[0];
            (vertex.position[2] - 10.0).atan2(vertex.position[0] - 10.0).to_degrees().rem_euclid(360.0)
        };
        assert!(near(turned(0), 6.0) && near((turned(1) - turned(0)).rem_euclid(360.0), 6.0) && near(turned(59), 0.0));
        // They go with the unit, and after 29 999 ticks.
        holes.forget(4);
        assert_eq!(holes.holes.len(), 1);
        holes.advance(start + TICK * 30_005);
        assert!(holes.holes.is_empty());
    }

    #[test]
    fn chaos_panic_maelstrom_and_bloody_lust_turn_their_squares() {
        let start = Instant::now();
        let mut holes = Holes::default();
        let mut draws = [10, 20, 30].into_iter();
        holes.keep(1, Look::ChaosPanic, [10.0, 1.0, 10.0], start, false, |_| 0);
        holes.keep(2, Look::Maelstrom, [20.0, 1.0, 20.0], start, false, |_| 0);
        holes.keep(3, Look::BloodyLust, [30.0, 1.0, 30.0], start, false, |_| draws.next().unwrap());
        // Chaos Panic: base 18 breathing 14.4 to 18.0; the picture a new one every 4 ticks from a = 3, a 36-tick loop.
        assert!(near(swirl_half(90.0, 18.0), 18.0) && near(swirl_half(270.0, 18.0), 14.4));
        let picture = |a: u32| DUST.iter().position(|path| *path == picture_at(Picture::Moving(&DUST), a)).unwrap();
        assert_eq!([2, 3, 31, 35, 39].map(picture), [0, 1, 8, 0, 1]);
        let ring_ticks: Vec<usize> = (0..=205)
            .map(|tick| {
                holes.advance(start + TICK * tick);
                holes.triples.len()
            })
            .collect();
        // Rings only for Chaos Panic and Bloody Lust: two prims at e = 0, 100, 200, none between (each lasts 136 ticks), in #FFFFAA and #FF6464.
        assert_eq!((ring_ticks[0], ring_ticks[99], ring_ticks[100], ring_ticks[199], ring_ticks[200]), (2, 2, 4, 2, 4));
        assert!(
            holes.triples.iter().any(|triple| triple.rgb == [1.0, 1.0, 170.0 / 255.0])
                && holes.triples.iter().any(|triple| triple.rgb == [1.0, 100.0 / 255.0, 100.0 / 255.0])
        );
        // Maelstrom: three squares in order, half-diagonal 27.0, 25.2, 18.0; the flare breathes (29.7, 12.0, 29.7, 29.4).
        let walls = holes.meshes(start, |_| None);
        let maelstrom: Vec<&Wall> = walls.iter().filter(|wall| wall.texture.contains("mae_")).collect();
        assert_eq!(maelstrom.len(), 3);
        assert!(
            maelstrom[0].texture.ends_with("mae_flare_00.bmp")
                && maelstrom[1].texture.ends_with("mae_ring_00.tga")
                && maelstrom[2].texture.ends_with("mae_ring_01.tga")
        );
        assert!(!maelstrom[0].texture.contains("mineffect"));
        assert!(near(f32::hypot(maelstrom[1].vertices[0].position[0] - 20.0, maelstrom[1].vertices[0].position[2] - 20.0), 25.2));
        assert!(near(f32::hypot(maelstrom[2].vertices[0].position[0] - 20.0, maelstrom[2].vertices[0].position[2] - 20.0), 18.0));
        let radii = [1, 60, 119, 120].map(flare_radius);
        assert!(near(radii[0], 29.7) && near(radii[1], 12.0) && near(radii[2], 29.7) && near(radii[3], 29.4));
        // Bloody Lust: three prims with the start turns 0, 120, 240 and the three spin draws in order, base 18.
        let lust = &holes.holes[2];
        assert_eq!(lust.squares.iter().map(|square| square.turn).collect::<Vec<_>>(), [0.0, 120.0, 240.0]);
        assert_eq!(lust.squares.iter().map(|square| square.spin).collect::<Vec<_>>(), [10.0, 20.0, 30.0]);
        assert!(lust.squares.iter().all(|square| square.base == 18.0 && square.breathes && square.step == 6.0));
        assert_eq!(picture_at(Picture::Moving(&TORCH), 47), TORCH[12]);
        assert_eq!((picture_at(Picture::Moving(&TORCH), 3), picture_at(Picture::Moving(&TORCH), 51)), (TORCH[1], TORCH[0]));
        // With /mineffect the Maelstrom's squares come from the mineffect folder.
        let mut minimal = Holes::default();
        minimal.keep(9, Look::Maelstrom, [0.0; 3], start, true, |_| 0);
        assert!(minimal.meshes(start, |_| None).iter().all(|wall| wall.texture.contains("mineffect")));
        assert!(textures(&Look::Maelstrom, true)[0].contains("mineffect") && !textures(&Look::Maelstrom, false)[0].contains("mineffect"));
    }

    #[test]
    fn a_sura_cast_breathes_two_blue_squares_apart_and_fades_them() {
        let start = Instant::now();
        let mut holes = Holes::default();
        holes.squares(Some(1), 40, start);
        // At least 120 ticks; R at spin 0, 90, 270: 21.6, 24.0, 19.2; alpha at A = 95, 96, 119, 120 for 120: 200, 192, 8, 0.
        assert_eq!(holes.squares[0].length, 120);
        assert!(near(sura_half(0.0), 21.6) && near(sura_half(90.0), 24.0) && near(sura_half(270.0), 19.2));
        assert_eq!([95, 96, 119, 120].map(|a| square_alpha(120, a)), [200.0, 192.0, 8.0, 0.0]);
        assert_eq!(square_alpha(200, 175), 200.0);
        assert_eq!(square_alpha(200, 176), 192.0);
        // Two flat squares 1 up, #2323FF added on; band 1 starts 180° after band 0 and its corners are at 23, 113, 203, 293.
        let feet = [10.0, 1.0, 10.0];
        let walls = holes.meshes(start, |_| Some(feet));
        assert_eq!(walls.len(), 2);
        assert!(walls.iter().all(|wall| wall.texture == SURA_CIRCLE && wall.additive && wall.vertices.len() == 6));
        assert_eq!(walls[0].vertices[0].tint[..3], [35.0 / 255.0, 35.0 / 255.0, 1.0]);
        assert!(near(walls[0].vertices[0].position[1], 2.0));
        let angle = |vertex: &MeshVertex| (vertex.position[2] - 10.0).atan2(vertex.position[0] - 10.0).to_degrees().rem_euclid(360.0);
        assert!(near(angle(&walls[0].vertices[0]), 0.0) && near(angle(&walls[1].vertices[0]), 23.0) && near(angle(&walls[1].vertices[1]), 113.0));
        // They follow the caster, end with the cast.
        assert!(holes.meshes(start, |_| None).is_empty());
        holes.advance(start + TICK * 130);
        assert!(holes.squares.is_empty());
    }
}
