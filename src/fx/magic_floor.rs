//! The map's magic floors (RSW type-4 objects 1025): six pictures lying over the object, turning at their own speeds,
//! and eight thin rings of cloud at their rim, high over them, that come and go. They turn in 16 ms ticks while the
//! player is near, and with the camera: the whole keeps its face to the view as the camera goes round.

use rustnarok_client::lua::FloorInfo;

use crate::emitters::SQUARE;
use crate::wind::{POINTS, band};
use crate::world::{MeshVertex, Vector};
use crate::world_fx::{Eye, yawed};

pub const ID: u32 = 1025;
/// The six pictures, under `data\texture\`, and the degrees each turns a tick.
pub const DISCS: [&str; 6] =
    ["effect\\hero_011.tga", "effect\\hero_001.tga", "effect\\hero_021.tga", "effect\\hero_002.tga", "effect\\hero_031.tga", "effect\\hero_003.tga"];
const TURNS: [i32; 6] = [1, 1, 2, 2, -1, -1];
/// The pictures lie this far over the object.
const OVER: f32 = 1.0;
pub const RING: &str = "effect\\cloud11.tga";
const RING_COLOR: [f32; 3] = [1.0, 100.0 / 255.0, 100.0 / 255.0];
/// The rings: two sets of four, the first of each set this high over the object and each next one 5 lower.
const RING_TOPS: [f32; 2] = [48.0, 28.0];
const RING_DROP: f32 = 5.0;
/// A ring turns this many degrees a tick; its arc starts this long and grows by `RING_GROWS` a tick up to `RING_SPAN`.
const RING_TURN: i32 = 5;
const RING_STARTS: i32 = 30;
const RING_GROWS: i32 = 3;
const RING_SPAN: i32 = 180;
/// A ring's alpha runs over this many ticks: from `RING_ALPHA` one up a tick for `RING_RISE` ticks, held, one down a
/// tick after `RING_HELD`, and nothing from then to the round's end.
const RING_ROUND: u32 = 300;
const RING_ALPHA: i32 = 50;
const RING_RISE: u32 = 12;
const RING_HELD: u32 = 100;

/// One ring of cloud.
#[derive(Debug, Clone, PartialEq)]
pub struct Ring {
    /// Where its tail is on the circle, and how long its arc is, in whole degrees.
    pub angle: i32,
    pub span: i32,
    pub alpha: i32,
    /// How high over the object it turns.
    pub lift: f32,
    /// How its band stands at each point, in degrees from lying flat outwards (80 to 100: about upright).
    pub leans: [u32; POINTS],
}

pub struct Floor {
    pub info: FloorInfo,
    /// In the world (y up).
    pub at: Vector,
    /// Its place in `Emitters::looks`.
    pub look: usize,
    /// The six pictures' angles, in whole degrees.
    pub angles: [i32; 6],
    pub rings: Vec<Ring>,
    /// Ticks run so far.
    pub age: u32,
    /// The player is in range: it turns and is drawn.
    pub near: bool,
}

impl Floor {
    /// A floor with its pictures at the row's angle and its eight rings, each starting somewhere on the circle (a
    /// draw) with a lean for each of its points (21 draws).
    pub fn new(info: FloorInfo, at: Vector, rand: &mut dyn FnMut() -> u32) -> Self {
        let mut rings = Vec::new();
        for top in RING_TOPS {
            for ring in 0..4 {
                let angle = (rand() % 360) as i32;
                let leans = std::array::from_fn(|_| 80 + rand() % 21);
                rings.push(Ring { angle, span: RING_STARTS, alpha: 0, lift: top - RING_DROP * ring as f32, leans });
            }
        }
        Self { info, at, look: 0, angles: [info.angle; 6], rings, age: 0, near: false }
    }

    /// One 16 ms tick with the player in range.
    pub fn tick(&mut self) {
        for (angle, turn) in self.angles.iter_mut().zip(TURNS) {
            *angle = (*angle + turn).rem_euclid(360);
        }
        let time = self.age % RING_ROUND;
        for ring in &mut self.rings {
            ring.angle = (ring.angle + RING_TURN) % 360;
            ring.span = (ring.span + RING_GROWS).min(RING_SPAN);
            if time == 0 {
                ring.alpha = RING_ALPHA;
            } else if time > RING_HELD {
                ring.alpha = (ring.alpha - 1).max(0);
            }
            if time < RING_RISE {
                ring.alpha += 1;
            }
        }
        self.age += 1;
    }

    /// A picture's four corners in the world: a square of the row's half side, turned by its angle, lying over the
    /// object.
    pub fn disc(&self, disc: usize, eye: &Eye) -> [Vector; 4] {
        let (sin, cos) = (self.angles[disc] as f32).to_radians().sin_cos();
        SQUARE.map(|([x, y], _)| {
            let [x, y] = [x * self.info.size, y * self.info.size];
            yawed(self.at, [x * cos - y * sin, OVER, -(x * sin + y * cos)], eye)
        })
    }

    /// A ring's point `index` in the world, its outer (upper) end then its inner: on the circle of the row's size, each
    /// point a 20th of the arc (in whole degrees) on from the one before; the band one unit long, leaning by the point's
    /// own lean.
    pub fn ring_point(&self, ring: &Ring, index: usize, eye: &Eye) -> [Vector; 2] {
        let (sin, cos) = (((ring.span / 20 * index as i32 + ring.angle) % 360) as f32).to_radians().sin_cos();
        let (up, out) = (ring.leans[index] as f32).to_radians().sin_cos();
        let size = self.info.size;
        [[(size + out) * cos, ring.lift + up, -(size + out) * sin], [size * cos, ring.lift, -size * sin]].map(|offset| yawed(self.at, offset, eye))
    }

    /// The pictures' squares, in their order: two triangles each, at the row's alpha.
    pub fn discs(&self, eye: &Eye) -> [Vec<MeshVertex>; 6] {
        let tint = [1.0, 1.0, 1.0, f32::from(self.info.alpha) / 255.0];
        std::array::from_fn(|disc| {
            let corners = self.disc(disc, eye);
            [0, 1, 2, 2, 1, 3].map(|corner| MeshVertex { position: corners[corner], uv: SQUARE[corner].1, tint }).to_vec()
        })
    }

    /// The rings' squares: every ring that isn't faded to nothing.
    pub fn rings(&self, eye: &Eye) -> Vec<MeshVertex> {
        let mut vertices = Vec::new();
        for ring in self.rings.iter().filter(|ring| ring.alpha > 0) {
            let points: Vec<[Vector; 2]> = (0..POINTS).map(|index| self.ring_point(ring, index, eye)).collect();
            vertices.extend(band(&points, [RING_COLOR[0], RING_COLOR[1], RING_COLOR[2], ring.alpha as f32 / 255.0]));
        }
        vertices
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn floor(rand: &mut dyn FnMut() -> u32) -> Floor {
        Floor::new(FloorInfo { size: 10.0, angle: 358, alpha: 255 }, [100.0, 5.0, 200.0], rand)
    }

    fn close(a: Vector, b: Vector) -> bool {
        a.iter().zip(b).all(|(a, b)| (a - b).abs() < 1e-3)
    }

    #[test]
    fn starts_its_rings_from_draws() {
        // A ring: its place on the circle, then a lean for each of its 21 points; eight rings.
        let mut drawn = 0;
        let floor = floor(&mut || {
            drawn += 1;
            [725, 20, 41][(drawn - 1) % 3]
        });
        assert_eq!(drawn, 8 * 22);
        assert_eq!((floor.rings[0].angle, floor.rings[0].leans[..3].to_vec(), floor.rings[0].span), (5, vec![100, 100, 91], 30));
        assert_eq!(floor.rings[1].angle, 20);
        let lifts: Vec<f32> = floor.rings.iter().map(|ring| ring.lift).collect();
        assert_eq!(lifts, [48.0, 43.0, 38.0, 33.0, 28.0, 23.0, 18.0, 13.0]);
        assert_eq!(floor.angles, [358; 6]);
    }

    #[test]
    fn turns_its_pictures_at_their_own_speeds() {
        let mut floor = floor(&mut || 0);
        floor.tick();
        assert_eq!(floor.angles, [359, 359, 0, 0, 357, 357]);
        floor.tick();
        assert_eq!(floor.angles, [0, 0, 2, 2, 356, 356]);
        (0..357).for_each(|_| floor.tick());
        assert_eq!((floor.angles[4], floor.age), (359, 359));
    }

    #[test]
    fn brings_its_rings_in_and_out() {
        let mut floor = floor(&mut || 0);
        let mut seen = Vec::new();
        for _ in 0..302 {
            floor.tick();
            seen.push((floor.rings[0].alpha, floor.rings[0].span, floor.rings[0].angle));
        }
        // 51 on the first tick, one up a tick to 62, held to the 101st, one down a tick to nothing, then again.
        let alphas: Vec<i32> = seen.iter().map(|(alpha, ..)| *alpha).collect();
        assert_eq!((alphas[0], alphas[1], alphas[11], alphas[12], alphas[100]), (51, 52, 62, 62, 62));
        assert_eq!((alphas[101], alphas[161], alphas[162], alphas[163], alphas[299]), (61, 1, 0, 0, 0));
        assert_eq!((alphas[300], alphas[301]), (51, 52));
        // The arc grows 3 degrees a tick from 30 to 180 and stays; the ring turns 5 degrees a tick.
        assert_eq!((seen[0].1, seen[49].1, seen[50].1, seen[301].1), (33, 180, 180, 180));
        assert_eq!((seen[0].2, seen[71].2, seen[72].2), (5, 0, 5));
        // Faded to nothing, a ring draws nothing.
        assert_eq!(floor.rings(&Eye::at(45.0, 0.0)).len(), 8 * 20 * 6);
        (0..170).for_each(|_| floor.tick());
        assert!(floor.rings(&Eye::at(45.0, 0.0)).is_empty());
    }

    #[test]
    fn lays_its_pictures_over_the_object() {
        let mut floor = floor(&mut || 0);
        floor.angles = [0, 90, 0, 0, 0, 0];
        // Flat, one unit over the object, its half side the row's size; the picture's top edge away from the camera.
        let corners = floor.disc(0, &Eye::at(45.0, 0.0));
        assert!(close(corners[0], [90.0, 6.0, 210.0]) && close(corners[1], [110.0, 6.0, 210.0]), "{corners:?}");
        assert!(close(corners[2], [90.0, 6.0, 190.0]) && close(corners[3], [110.0, 6.0, 190.0]), "{corners:?}");
        // Turned a quarter: its first corner goes where the third was.
        assert!(close(floor.disc(1, &Eye::at(45.0, 0.0))[0], [90.0, 6.0, 190.0]));
        // With the camera a quarter round, the whole goes round with it.
        let turned = floor.disc(0, &Eye::at(45.0, 90.0));
        assert!(close(turned[0], [110.0, 6.0, 210.0]) && close(turned[1], [110.0, 6.0, 190.0]), "{turned:?}");
        // Six squares at the row's alpha, the whole picture on each.
        let discs = floor.discs(&Eye::at(45.0, 0.0));
        assert_eq!((discs[0].len(), discs[0][0].uv, discs[0][5].uv, discs[0][0].tint), (6, [0.0, 0.0], [1.0, 1.0], [1.0; 4]));
    }

    #[test]
    fn places_a_rings_points_at_the_rim() {
        let floor = floor(&mut || 0);
        let eye = Eye::at(45.0, 0.0);
        let mut leans = [90; POINTS];
        leans[1] = 0;
        let ring = Ring { angle: 0, span: 185, alpha: 60, lift: 48.0, leans };
        // The tail at angle 0: at the rim along x, 48 over the object; upright, its upper end one unit higher.
        let [outer, inner] = floor.ring_point(&ring, 0, &eye);
        assert!(close(inner, [110.0, 53.0, 200.0]) && close(outer, [110.0, 54.0, 200.0]), "{inner:?} {outer:?}");
        // Each point a 20th of the arc on, in whole degrees (9 for an arc of 185); lying flat it reaches one unit out.
        let (sin, cos) = 9f32.to_radians().sin_cos();
        let [outer, inner] = floor.ring_point(&ring, 1, &eye);
        assert!(close(inner, [100.0 + 10.0 * cos, 53.0, 200.0 + 10.0 * sin]), "{inner:?}");
        assert!(close(outer, [100.0 + 11.0 * cos, 53.0, 200.0 + 11.0 * sin]), "{outer:?}");
        // Ten points on, a quarter round.
        assert!(close(floor.ring_point(&ring, 10, &eye)[1], [100.0, 53.0, 210.0]));
    }
}
