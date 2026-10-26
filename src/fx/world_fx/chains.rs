//! Two objects that live as long as their unit: the map chain (1024), a cage of five chain rings turning round the unit,
//! and the common floor disc (1056), a pale blue rune disc spinning under it.

use std::any::Any;

use crate::world::Vector;
use crate::world_str::Scene;

use super::{Assets, Class, FLAGS_EFFECT, Face, Fx, SQUARE, Seen, Tex, WorldFx, add, scaled, triangles};

/// EF_MAPCHAIN.
pub const MAP_CHAIN: u32 = 1024;
/// EF_RA_UNLIMIT.
pub const FLOOR_DISC: u32 = 1056;

pub fn register(fx: &mut WorldFx) {
    fx.register(MAP_CHAIN, || Box::new(Chains::default()));
    fx.register(FLOOR_DISC, || Box::new(Disc::default()));
}

// The map chain (1024)

const CHAIN_TEXTURE: &str = "data\\texture\\effect\\hero_005.tga";
/// The five bands' tilts in degrees.
const TILTS: [i32; 5] = [25, 50, 95, 120, 165];
const CHAINS: usize = 12;
const LINKS: usize = 21;
/// Half the band's width.
const WIDTH: f32 = 10.0;
/// The ring's radius, and how high its centre is over the feet.
const RADIUS: f32 = 14.0;

/// One point of a chain, as it turns round its ring.
#[derive(Debug, Clone, Default)]
struct Link {
    /// Degrees, +1 a tick.
    angle: i32,
    alpha: i32,
    /// Its own counter, +1 an update.
    e: u32,
    /// The band's two edges here, from the feet.
    inner: Vector,
    outer: Vector,
}

/// 5 bands × 12 chains × 21 links, in that order. Chains 6 – 11 are chains 0 – 5 again, drawn adding and pulsing.
struct Chains {
    links: Vec<Link>,
}

impl Default for Chains {
    fn default() -> Self {
        let link = |index: usize| Link { angle: 60 * ((index / LINKS % CHAINS) % 6) as i32, alpha: 255, ..Link::default() };
        Self { links: (0..TILTS.len() * CHAINS * LINKS).map(link).collect() }
    }
}

/// The sine and cosine of a whole degree.
fn sin_cos(degrees: i32) -> (f32, f32) {
    (degrees as f32).to_radians().sin_cos()
}

/// A band of tilt `p`'s inner and outer edges at `theta` on its ring, from the feet in world axes.
fn edges(p: i32, theta: i32) -> (Vector, Vector) {
    let (sin_p, cos_p) = sin_cos(p);
    let (sin_t, cos_t) = sin_cos(theta);
    let y = RADIUS - RADIUS * sin_t;
    let along = RADIUS * cos_t;
    ([along * sin_p - WIDTH * cos_p, y, along * cos_p + WIDTH * sin_p], [along * sin_p + WIDTH * cos_p, y, along * cos_p - WIDTH * sin_p])
}

/// A copy's alpha after the update with counter `e`: up 3 a tick for the first 251 of every 600, down 3 for the rest.
fn pulse(alpha: i32, e: u32) -> i32 {
    (alpha + if e % 600 <= 250 { 3 } else { -3 }).clamp(0, 255)
}

impl Class for Chains {
    fn begin(&mut self, _: &mut Fx, assets: &mut dyn Assets) {
        assets.texture(CHAIN_TEXTURE);
    }

    fn update(&mut self, _: &mut Fx, _: &Scene, _: &mut dyn Assets) {
        for (index, link) in self.links.iter_mut().enumerate() {
            let (p, chain, i) = (TILTS[index / (CHAINS * LINKS)], index / LINKS % CHAINS, index % LINKS);
            link.angle = (link.angle + 1) % 360;
            if chain >= 6 {
                link.alpha = pulse(link.alpha, link.e);
            }
            (link.inner, link.outer) = edges(p, (3 * i as i32 + link.angle) % 360);
            link.e += 1;
        }
    }

    /// Per chain, a quad between each two links; chains 6 – 11 add.
    // ponytail: the copies blend source alpha onto destination alpha in the game; drawn adding here.
    fn draw<'a>(&'a self, fx: &Fx, _: &Seen, out: &mut Vec<Face<'a>>) {
        for (index, chain) in self.links.chunks(LINKS).enumerate() {
            for (i, pair) in chain.windows(2).enumerate() {
                let (u0, u1) = (i as f32 / LINKS as f32, (i + 1) as f32 / LINKS as f32);
                let corners = [pair[0].inner, pair[1].inner, pair[0].outer, pair[1].outer].map(|offset| add(fx.position, offset));
                out.push(Face {
                    texture: Tex::Path(CHAIN_TEXTURE),
                    vertices: triangles(corners, [[u0, 0.0], [u1, 0.0], [u0, 1.0], [u1, 1.0]], [1.0, 1.0, 1.0, pair[1].alpha as f32 / 255.0]),
                    additive: index % CHAINS >= 6,
                    flags: FLAGS_EFFECT,
                });
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

// The common floor disc (1056)

const DISC_TEXTURE: &str = "data\\texture\\effect\\ra_unlimit.tga";
const DISC_HALF: f32 = 15.0;
const DISC_ALPHA: f32 = 90.0;
/// How far over the feet it lies.
const DISC_LIFT: f32 = 1.0;

/// Two copies of the disc, half a turn apart, each turning 6° a tick.
struct Disc {
    angles: [i32; 2],
}

impl Default for Disc {
    fn default() -> Self {
        Self { angles: [0, 180] }
    }
}

impl Class for Disc {
    fn begin(&mut self, _: &mut Fx, assets: &mut dyn Assets) {
        assets.texture(DISC_TEXTURE);
    }

    fn update(&mut self, _: &mut Fx, _: &Scene, _: &mut dyn Assets) {
        self.angles = self.angles.map(|angle| (angle + 6) % 360);
    }

    /// Flat with its edges along the screen, turned by its angle in the ground's plane.
    // ponytail: the turn's sign is unchecked; it spins one way or the other.
    fn draw<'a>(&'a self, fx: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        let (right, forward) = (seen.eye.ground_right(), seen.eye.ground_forward());
        let point = add(fx.position, [0.0, DISC_LIFT, 0.0]);
        for angle in self.angles {
            let (sin, cos) = sin_cos(angle);
            let x = add(scaled(right, cos), scaled(forward, sin));
            let y = add(scaled(forward, cos), scaled(right, -sin));
            let corners = SQUARE.map(|[cx, cy]| add(point, add(scaled(x, cx * DISC_HALF), scaled(y, cy * DISC_HALF))));
            out.push(Face {
                texture: Tex::Path(DISC_TEXTURE),
                vertices: triangles(corners, [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]], [1.0, 1.0, 1.0, DISC_ALPHA / 255.0]),
                additive: true,
                flags: FLAGS_EFFECT,
            });
        }
    }

    fn alive(&self, fx: &Fx, scene: &Scene) -> bool {
        fx.owned(scene)
    }

    fn any(&mut self) -> Option<&mut dyn Any> {
        Some(self)
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Eye, Launch, Object};
    use super::*;
    use crate::actor::Placed;
    use crate::effects::Loaded;
    use std::rc::Rc;

    const UNIT: u32 = 0x700;
    const FEET: Vector = [30.0, 4.0, 50.0];

    #[derive(Default)]
    struct Fake;

    impl Assets for Fake {
        fn texture(&mut self, _: &str) -> Option<[f32; 2]> {
            Some([256.0; 2])
        }
        fn str(&mut self, _: &str) -> Option<Rc<Loaded>> {
            None
        }
        fn sprite(&mut self, _: &str, _: usize, _: usize) -> Option<(usize, Vec<Placed>)> {
            None
        }
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    fn close3(a: Vector, b: Vector) -> bool {
        (0..3).all(|axis| close(a[axis], b[axis]))
    }

    /// `count` ticks with the unit at `at` (None: gone).
    fn ticks(fx: &mut WorldFx, count: usize, at: Option<Vector>) {
        let feet = |unit: Option<u32>| match unit {
            Some(UNIT) => at,
            Some(_) => None,
            None => Some(FEET),
        };
        let body = |_: Option<u32>| None;
        let eye = Eye::at(45.0, 0.0);
        let scene = Scene { player: FEET, feet: &feet, body: &body, longitude: 0.0, right: eye.right, up: eye.up, ground: &|_, _| 0.0 };
        for _ in 0..count {
            fx.tick(&scene, &mut Fake);
        }
    }

    fn launched(id: u32) -> WorldFx {
        let mut fx = WorldFx::default();
        fx.launch(&Launch::at(id, Some(Some(UNIT)), FEET), &mut Fake).unwrap();
        fx
    }

    fn faces_at(fx: &WorldFx, longitude: f32) -> Vec<(Vec<Vector>, bool, f32)> {
        let seen = Seen { eye: Eye::at(45.0, longitude), shows: &|_| true, hidden: &|_| false, size: &|_| 1.0, ground: &|_, _| 0.0, dim: false };
        fx.faces(&seen)
            .into_iter()
            .map(|face| (face.vertices.iter().map(|vertex| vertex.position).collect(), face.additive, face.vertices[0].tint[3]))
            .collect()
    }

    fn chains(fx: &mut WorldFx) -> &mut Chains {
        fx.class_mut::<Chains>(Some(Some(UNIT)), &[MAP_CHAIN]).unwrap().1
    }

    fn disc(fx: &mut WorldFx) -> &mut Disc {
        fx.class_mut::<Disc>(Some(Some(UNIT)), &[FLOOR_DISC]).unwrap().1
    }

    #[test]
    fn the_cage_makes_1260_links_in_60_chains_and_draws_1200_quads() {
        let mut fx = launched(MAP_CHAIN);
        let links = &chains(&mut fx).links;
        assert_eq!(links.len(), 1260);
        for band in 0..5 {
            for chain in 0..12 {
                let start = (band * 12 + chain) * 21;
                assert!(links[start..start + 21].iter().all(|link| link.angle == 60 * (chain as i32 % 6) && link.alpha == 255 && link.e == 0));
            }
        }
        ticks(&mut fx, 3, Some(FEET));
        assert_eq!(chains(&mut fx).links.len(), 1260);
        let faces = faces_at(&fx, 0.0);
        assert_eq!(faces.len(), 1200);
        // Per band, chains 0 – 5 blend and 6 – 11 add: 120 quads each.
        for (index, (_, additive, _)) in faces.iter().enumerate() {
            assert_eq!(*additive, index / 20 % 12 >= 6, "{index}");
        }
    }

    #[test]
    fn a_link_turns_a_degree_a_tick() {
        let mut fx = launched(MAP_CHAIN);
        ticks(&mut fx, 365, Some(FEET));
        let links = &chains(&mut fx).links;
        // Band 0, chain 1 (a = 60), and band 4, chain 11 (a = 300).
        assert_eq!(links[21].angle, (60 + 365) % 360);
        assert_eq!(links[4 * 252 + 11 * 21].angle, (300 + 365) % 360);
    }

    #[test]
    fn a_band_is_a_vertical_ring_of_radius_14_resting_on_the_feet() {
        // θ for link 5 after 2 updates with a = 0 is 17.
        let mut fx = launched(MAP_CHAIN);
        ticks(&mut fx, 2, Some(FEET));
        let link = chains(&mut fx).links[5].clone();
        assert_eq!(link.angle, 2);
        assert_eq!((link.inner, link.outer), edges(25, 17));
        // p = 0, θ = 0.
        assert_eq!(edges(0, 0), ([-10.0, 14.0, 14.0], [10.0, 14.0, 14.0]));
        let (top, _) = edges(0, 270);
        assert!(close3(top, [-10.0, 28.0, 0.0]));
        let (bottom, _) = edges(90, 90);
        assert!(close3(bottom, [0.0, 0.0, 10.0]));
    }

    #[test]
    fn the_copies_fade_out_and_in_every_600_ticks() {
        let mut alpha = 255;
        let mut seen = Vec::new();
        for e in 0..=851 {
            alpha = pulse(alpha, e);
            if [0, 250, 251, 335, 599, 600, 684, 851].contains(&e) {
                seen.push(alpha);
            }
        }
        assert_eq!(seen, [255, 255, 252, 0, 0, 3, 255, 252]);
        // In the object: chain 6 pulses, chains 0 – 5 stay.
        let mut fx = launched(MAP_CHAIN);
        ticks(&mut fx, 336, Some(FEET));
        let links = &chains(&mut fx).links;
        assert_eq!(links[6 * 21].alpha, 0);
        assert!(links.iter().enumerate().filter(|(index, _)| index / 21 % 12 < 6).all(|(_, link)| link.alpha == 255));
        ticks(&mut fx, 849 - 336, Some(FEET));
        assert_eq!(chains(&mut fx).links[6 * 21].alpha, 255);
        ticks(&mut fx, 3, Some(FEET));
        assert_eq!(chains(&mut fx).links[6 * 21].alpha, 252);
    }

    #[test]
    fn the_cage_ignores_the_camera_follows_its_unit_and_dies_with_it() {
        let mut fx = launched(MAP_CHAIN);
        ticks(&mut fx, 1, Some(FEET));
        assert_eq!(faces_at(&fx, 0.0), faces_at(&fx, 90.0));
        let before = faces_at(&fx, 0.0);
        let moved = [40.0, 6.0, 55.0];
        // Same tick's shape, moved: compare after the same number of updates.
        let mut there = launched(MAP_CHAIN);
        ticks(&mut there, 1, Some(moved));
        let after = faces_at(&there, 0.0);
        for ((a, _, _), (b, _, _)) in before.iter().zip(&after) {
            assert!(a.iter().zip(b).all(|(a, b)| close3([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [10.0, 2.0, 5.0])));
        }
        ticks(&mut fx, 1, None);
        ticks(&mut fx, 1, None);
        assert!(fx.is_empty());
    }

    #[test]
    fn two_packets_make_two_cages() {
        let mut fx = launched(MAP_CHAIN);
        fx.launch(&Launch::at(MAP_CHAIN, Some(Some(UNIT)), FEET), &mut Fake).unwrap();
        assert_eq!(fx.iter().filter(|Object { fx, .. }| fx.id == MAP_CHAIN).count(), 2);
        let mut disc = launched(FLOOR_DISC);
        disc.launch(&Launch::at(FLOOR_DISC, Some(Some(UNIT)), FEET), &mut Fake).unwrap();
        assert_eq!(disc.iter().count(), 2);
    }

    #[test]
    fn the_disc_is_two_faint_adding_quads_spinning_6_a_tick() {
        let mut fx = launched(FLOOR_DISC);
        assert_eq!(disc(&mut fx).angles, [0, 180]);
        let mut seen = Vec::new();
        for (count, total) in [(1, 1), (59, 60), (1, 61)] {
            ticks(&mut fx, count, Some(FEET));
            assert_eq!(fx.iter().next().unwrap().fx.age, total);
            seen.push(disc(&mut fx).angles);
        }
        assert_eq!(seen, [[6, 186], [0, 180], [6, 186]]);
        let faces = faces_at(&fx, 30.0);
        assert_eq!(faces.len(), 2);
        for (corners, additive, alpha) in &faces {
            assert!(*additive && close(*alpha, 90.0 / 255.0));
            // 1 over the feet, 15 from the centre to each edge's middle: corners √2·15 out.
            assert!(corners.iter().all(|corner| close(corner[1], FEET[1] + 1.0)));
            assert!(corners.iter().all(|corner| close((corner[0] - FEET[0]).hypot(corner[2] - FEET[2]), 15.0 * 2f32.sqrt())));
        }
        // The two copies are half a turn apart: each corner of one is the opposite corner of the other.
        assert!(close3(faces[0].0[0], faces[1].0[5]));
    }

    #[test]
    fn the_disc_follows_its_unit_and_dies_with_it() {
        let mut fx = launched(FLOOR_DISC);
        let moved = [40.0, 6.0, 55.0];
        ticks(&mut fx, 1, Some(moved));
        let faces = faces_at(&fx, 0.0);
        let centre = faces[0].0.iter().fold([0.0; 3], |sum, corner| add(sum, scaled(*corner, 1.0 / 6.0)));
        assert!(close3(centre, [40.0, 7.0, 55.0]), "{centre:?}");
        ticks(&mut fx, 1, None);
        assert!(fx.is_empty());
    }
}
