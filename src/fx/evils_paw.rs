//! The map's Evil's Paws (RSW type-4 objects 1039): one square picture over the object, facing the camera, that runs
//! through twenty pictures over and over. It steps in 16 ms ticks while the player is near.

use rustnarok_client::lua::PawInfo;

use crate::emitters::SQUARE;
use crate::world::{MeshVertex, Vector};
use crate::world_fx::Eye;

pub const ID: u32 = 1039;
/// The pictures in their order, under `data\texture\`.
pub const PICTURES: [&str; 20] = [
    "effect\\1-1.tga",
    "effect\\1-2.tga",
    "effect\\1-3.tga",
    "effect\\1-4.tga",
    "effect\\1-5.tga",
    "effect\\1-6.tga",
    "effect\\1-7.tga",
    "effect\\1-8.tga",
    "effect\\1-9.tga",
    "effect\\1-10.tga",
    "effect\\2-1.tga",
    "effect\\2-2.tga",
    "effect\\2-3.tga",
    "effect\\2-4.tga",
    "effect\\2-5.tga",
    "effect\\2-6.tga",
    "effect\\2-7.tga",
    "effect\\2-8.tga",
    "effect\\2-9.tga",
    "effect\\2-10.tga",
];
/// The blend numbers: the picture's own alpha over what is behind.
pub const BLEND: [u8; 2] = [5, 6];
/// The square sits this far behind its place, along its own facing, as an emitter's particle.
const BACK: f32 = 1.0;

pub struct Paw {
    pub info: PawInfo,
    /// The map's object, in the world (y up).
    pub at: Vector,
    /// Its place in `Emitters::looks`.
    pub look: usize,
    /// The picture drawn: the one the last tick started from.
    pub shown: usize,
    /// The picture the next tick shows, and the ticks it still stays after that one.
    next: usize,
    wait: i32,
    /// The player is in range: it steps and is drawn.
    pub near: bool,
}

impl Paw {
    pub fn new(info: PawInfo, at: Vector) -> Self {
        Self { info, at, look: 0, shown: 0, next: 0, wait: info.speed, near: false }
    }

    /// One 16 ms tick with the player in range: a picture stays the row's speed in ticks and one more, and the first
    /// follows the last.
    pub fn tick(&mut self) {
        self.shown = self.next;
        self.wait -= 1;
        if self.wait < 0 {
            self.next = (self.next + 1) % PICTURES.len();
            self.wait = self.info.speed;
        }
    }

    /// The square's four corners in the world: its middle the row's offset from the object, its half side the row's
    /// size across the view, the picture upright (a half turn from an emitter particle's square).
    pub fn corners(&self, eye: &Eye) -> [Vector; 4] {
        let [dx, dy, dz] = self.info.offset;
        let middle = [self.at[0] + dx, self.at[1] - dy, self.at[2] + dz];
        SQUARE.map(|([x, y], _)| {
            let [x, y] = [-x * self.info.size, -y * self.info.size];
            [0, 1, 2].map(|axis| middle[axis] + eye.right[axis] * x - eye.up[axis] * y + eye.forward[axis] * BACK)
        })
    }

    /// The square as two triangles, the whole picture on it, white and solid.
    pub fn vertices(&self, eye: &Eye) -> Vec<MeshVertex> {
        let corners = self.corners(eye);
        [0, 1, 2, 2, 1, 3].map(|corner| MeshVertex { position: corners[corner], uv: SQUARE[corner].1, tint: [1.0; 4] }).to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paw(speed: i32) -> Paw {
        Paw::new(PawInfo { offset: [0.0, -40.0, 3.0], size: 14.0, speed }, [100.0, 5.0, 200.0])
    }

    #[test]
    fn steps_through_its_twenty_pictures() {
        // Speed 2: each picture for three ticks.
        let mut paw = paw(2);
        let shown: Vec<usize> = (0..8).map(|_| (paw.tick(), paw.shown).1).collect();
        assert_eq!(shown, [0, 0, 0, 1, 1, 1, 2, 2]);
        // After the twentieth comes the first again.
        (8..60).for_each(|_| paw.tick());
        assert_eq!((paw.shown, PICTURES[paw.shown]), (19, "effect\\2-10.tga"));
        paw.tick();
        assert_eq!((paw.shown, PICTURES[paw.shown]), (0, "effect\\1-1.tga"));
        assert_eq!((PICTURES[9], PICTURES[10]), ("effect\\1-10.tga", "effect\\2-1.tga"));
        // Speed 0: a picture a tick; under 0 the same.
        let mut fast = self::paw(0);
        assert_eq!((0..4).map(|_| (fast.tick(), fast.shown).1).collect::<Vec<_>>(), [0, 1, 2, 3]);
        let mut faster = self::paw(-3);
        assert_eq!((0..4).map(|_| (faster.tick(), faster.shown).1).collect::<Vec<_>>(), [0, 1, 2, 3]);
    }

    #[test]
    fn stands_upright_over_its_object() {
        let close = |a: Vector, b: Vector| a.iter().zip(b).all(|(a, b)| (a - b).abs() < 1e-3);
        let paw = paw(5);
        // Looking level along +z: the middle 40 over the object and 3 along z (and 1 back); the picture's top left corner
        // is up and, turned half round from a particle's square, on the camera's right.
        let corners = paw.corners(&Eye::at(0.0, 0.0));
        assert!(close(corners[0], [114.0, 59.0, 204.0]) && close(corners[1], [86.0, 59.0, 204.0]), "{corners:?}");
        assert!(close(corners[2], [114.0, 31.0, 204.0]) && close(corners[3], [86.0, 31.0, 204.0]), "{corners:?}");
        // It turns with the camera: a quarter round, its width lies along z.
        let turned = paw.corners(&Eye::at(0.0, 90.0));
        assert!(close(turned[0], [101.0, 59.0, 189.0]) && close(turned[1], [101.0, 59.0, 217.0]), "{turned:?}");
        // Two triangles with the whole picture, white and solid.
        let vertices = paw.vertices(&Eye::at(45.0, 0.0));
        assert_eq!((vertices.len(), vertices[0].uv, vertices[5].uv, vertices[0].tint), (6, [0.0, 0.0], [1.0, 1.0], [1.0; 4]));
    }
}
