//! The map's winds (RSW type-4 objects 2343): strands of a picture turning round a point, each an arc of squares at
//! its own height, fading in and out, as the wind's row of the map's table describes. They turn in 16 ms ticks while
//! the player is near, in the map's own axes (y down); only the drawn corners are in the world's (y up).

use rustnarok_client::lua::WindInfo;

use crate::world::{MeshVertex, Vector};

pub const ID: u32 = 2343;
/// A strand's points along its arc; a square is drawn from each to the next.
pub const POINTS: usize = 21;

/// One arc of a wind.
#[derive(Debug, Clone, PartialEq)]
pub struct Strand {
    /// Where its tail is on the circle, in degrees.
    pub angle: f32,
    pub alpha: i32,
    /// The alpha is on its way down.
    pub fading: bool,
    /// How far over the wind's place it turns.
    pub lift: f32,
}

pub struct Wind {
    pub info: WindInfo,
    /// The map's half width and depth: the map's axes start at its centre.
    half: [f32; 2],
    /// Its place in `Emitters::looks`.
    pub look: usize,
    pub strands: Vec<Strand>,
    /// The player is in range: it turns and is drawn.
    pub near: bool,
}

/// The squares between an arc's points (each its outer and its inner end): the picture runs once along the arc, its
/// top on the outer edge, its last 21st unused.
pub fn band(points: &[[Vector; 2]], tint: [f32; 4]) -> impl Iterator<Item = MeshVertex> + '_ {
    points.windows(2).enumerate().flat_map(move |(index, pair)| {
        let (from, to) = (index as f32 / POINTS as f32, (index + 1) as f32 / POINTS as f32);
        let corners = [(pair[0][0], [from, 0.0]), (pair[1][0], [to, 0.0]), (pair[0][1], [from, 1.0]), (pair[1][1], [to, 1.0])];
        [0, 1, 2, 2, 1, 3].map(|corner| MeshVertex { position: corners[corner].0, uv: corners[corner].1, tint })
    })
}

impl Wind {
    /// A wind with its strands spread over its height, each starting somewhere on the circle (a draw) at some alpha
    /// under the row's (a second draw).
    pub fn new(info: WindInfo, half: [f32; 2], rand: &mut dyn FnMut() -> u32) -> Self {
        let strands = (0..info.strands)
            .map(|strand| {
                let angle = (rand() % 360) as f32;
                let alpha = if info.alpha == 0 { 0 } else { (rand() % u32::from(info.alpha)) as i32 };
                let lift = if info.strands > 1 { info.height / (info.strands - 1) as f32 * strand as f32 } else { 0.0 };
                Strand { angle, alpha, fading: false, lift }
            })
            .collect();
        Self { info, half, look: 0, strands, near: false }
    }

    /// A point of the map's axes in the world.
    fn world(&self, [x, y, z]: Vector) -> Vector {
        [x + self.half[0], -y, z + self.half[1]]
    }

    /// Where it turns, in the world.
    pub fn at(&self) -> Vector {
        self.world(self.info.position)
    }

    /// One 16 ms tick with the player in range: each strand's alpha goes a step up to the row's, then down to nothing,
    /// and the strand turns by the row's speed.
    pub fn tick(&mut self) {
        let top = i32::from(self.info.alpha);
        for strand in &mut self.strands {
            if strand.fading {
                strand.alpha -= 1;
                if strand.alpha <= 0 {
                    (strand.alpha, strand.fading) = (0, false);
                }
            } else {
                strand.alpha += 1;
                if strand.alpha >= top {
                    (strand.alpha, strand.fading) = (top, true);
                }
            }
            strand.angle += self.info.speed;
            if strand.angle > 360.0 {
                strand.angle -= 360.0;
            }
        }
    }

    /// A strand's point `index` in the world, its outer end then its inner: on the circle of the row's radius, the
    /// outer end the thickness further out and as much lower; the whole turned about x, then z, round the wind's place.
    pub fn point(&self, strand: &Strand, index: usize) -> [Vector; 2] {
        let (sin, cos) = (strand.angle + index as f32 * self.info.span as f32 / (POINTS - 1) as f32).to_radians().sin_cos();
        let (radius, thickness) = (self.info.radius, self.info.thickness);
        let [about_x, about_z] = self.info.tilt.map(|degrees| degrees.to_radians().sin_cos());
        [[(radius + thickness) * cos, thickness - strand.lift, (radius + thickness) * sin], [radius * cos, -strand.lift, radius * sin]].map(|[x, y, z]| {
            let [y, z] = [y * about_x.1 - z * about_x.0, y * about_x.0 + z * about_x.1];
            let [x, y] = [x * about_z.1 - y * about_z.0, x * about_z.0 + y * about_z.1];
            self.world([0, 1, 2].map(|axis| [x, y, z][axis] + self.info.position[axis]))
        })
    }

    /// What it draws now: the squares of every strand that isn't faded to nothing, tinted the row's colour.
    pub fn vertices(&self) -> Vec<MeshVertex> {
        let [red, green, blue] = self.info.color.map(|channel| f32::from(channel) / 255.0);
        let mut vertices = Vec::new();
        for strand in self.strands.iter().filter(|strand| strand.alpha > 0) {
            let points: Vec<[Vector; 2]> = (0..POINTS).map(|index| self.point(strand, index)).collect();
            vertices.extend(band(&points, [red, green, blue, strand.alpha as f32 / 255.0]));
        }
        vertices
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info() -> WindInfo {
        WindInfo {
            position: [10.0, -20.0, 30.0],
            strands: 3,
            alpha: 200,
            color: [255, 102, 0],
            radius: 12.0,
            thickness: 2.0,
            height: 10.0,
            speed: 4.0,
            span: 200,
            tilt: [0.0, 0.0],
            blend: [5, 7],
            texture: "effect\\cloud11.tga".into(),
        }
    }

    fn close(a: Vector, b: Vector) -> bool {
        a.iter().zip(b).all(|(a, b)| (a - b).abs() < 1e-3)
    }

    #[test]
    fn starts_each_strand_from_two_draws() {
        let mut draws = [370u32, 205, 90, 199, 0, 400].into_iter();
        let wind = Wind::new(info(), [100.0, 200.0], &mut || draws.next().unwrap());
        // Somewhere on the circle, at an alpha under the row's; spread from the wind's place up to its height.
        let got: Vec<(f32, i32, f32)> = wind.strands.iter().map(|strand| (strand.angle, strand.alpha, strand.lift)).collect();
        assert_eq!(got, [(10.0, 5, 0.0), (90.0, 199, 5.0), (0.0, 0, 10.0)]);
        assert_eq!(wind.at(), [110.0, 20.0, 230.0]);
        // One strand turns at the wind's own height; a row's alpha of 0 takes no draw.
        let mut draws = [45u32].into_iter();
        let one = Wind::new(WindInfo { strands: 1, alpha: 0, ..info() }, [0.0; 2], &mut || draws.next().unwrap());
        assert_eq!(one.strands, [Strand { angle: 45.0, alpha: 0, fading: false, lift: 0.0 }]);
    }

    #[test]
    fn turns_and_breathes_by_the_tick() {
        let mut wind = Wind::new(WindInfo { strands: 1, alpha: 3, speed: 100.0, ..info() }, [0.0; 2], &mut || 0);
        let mut seen = Vec::new();
        for _ in 0..8 {
            wind.tick();
            seen.push((wind.strands[0].alpha, wind.strands[0].angle));
        }
        // A step a tick up to the row's alpha, then down to nothing, and up again; past a full turn the angle starts over.
        let alphas: Vec<i32> = seen.iter().map(|(alpha, _)| *alpha).collect();
        assert_eq!(alphas, [1, 2, 3, 2, 1, 0, 1, 2]);
        let angles: Vec<f32> = seen.iter().map(|(_, angle)| *angle).collect();
        assert_eq!(angles[..5], [100.0, 200.0, 300.0, 40.0, 140.0]);
        // A wind turning the other way goes on under 0.
        let mut back = Wind::new(WindInfo { strands: 1, speed: -0.5, ..info() }, [0.0; 2], &mut || 0);
        back.tick();
        assert_eq!(back.strands[0].angle, -0.5);
    }

    #[test]
    fn places_a_strands_points_on_its_circle() {
        let wind = Wind::new(info(), [100.0, 200.0], &mut || 0);
        let strand = Strand { angle: 0.0, alpha: 100, fading: false, lift: 5.0 };
        // The tail at angle 0: 12 along x from the wind's place, 5 over it (y is down in the map); the outer end 2
        // further out and 2 lower.
        let [outer, inner] = wind.point(&strand, 0);
        assert!(close(inner, [122.0, 25.0, 230.0]) && close(outer, [124.0, 23.0, 230.0]), "{inner:?} {outer:?}");
        // Each point a 20th of the span on: the ninth is a quarter turn round, the last the whole 200 degrees.
        let [outer, inner] = wind.point(&strand, 9);
        assert!(close(inner, [110.0, 25.0, 242.0]) && close(outer, [110.0, 23.0, 244.0]), "{inner:?} {outer:?}");
        let (sin, cos) = 200f32.to_radians().sin_cos();
        assert!(close(wind.point(&strand, 20)[1], [110.0 + 12.0 * cos, 25.0, 230.0 + 12.0 * sin]));
        // Tilted a quarter turn about x, the circle stands up: what was 12 along z is 12 up, the 5 over it 5 back along z.
        let tilted = Wind { info: WindInfo { tilt: [90.0, 0.0], ..info() }, ..Wind::new(info(), [100.0, 200.0], &mut || 0) };
        assert!(close(tilted.point(&strand, 9)[1], [110.0, 32.0, 225.0]), "{:?}", tilted.point(&strand, 9));
        // Then about z: x swings to the map's y (down).
        let rolled = Wind { info: WindInfo { tilt: [0.0, 90.0], ..info() }, ..Wind::new(info(), [100.0, 200.0], &mut || 0) };
        assert!(close(rolled.point(&strand, 0)[1], [115.0, 8.0, 230.0]), "{:?}", rolled.point(&strand, 0));
    }

    #[test]
    fn draws_twenty_squares_a_strand() {
        let mut draws = [0u32, 100, 0, 0, 0, 50].into_iter();
        let wind = Wind::new(info(), [0.0; 2], &mut || draws.next().unwrap());
        // The strand at alpha 0 draws nothing; the others 20 squares of two triangles each.
        let vertices = wind.vertices();
        assert_eq!(vertices.len(), 2 * 20 * 6);
        assert_eq!(vertices[0].tint, [1.0, 0.4, 0.0, 100.0 / 255.0]);
        assert_eq!(vertices[240 - 1].tint[3], 50.0 / 255.0);
        // A square: the point before and its own, outer ends on the picture's top row; a 21st of the picture each.
        let strand = &wind.strands[0];
        let (before, own) = (wind.point(strand, 0), wind.point(strand, 1));
        let square: Vec<(Vector, [f32; 2])> = vertices[..6].iter().map(|vertex| (vertex.position, vertex.uv)).collect();
        let (right, corners) = (1.0 / 21.0, [(before[0], [0.0, 0.0]), (own[0], [1.0 / 21.0, 0.0]), (before[1], [0.0, 1.0])]);
        assert_eq!(square[..3], corners);
        assert_eq!(square[3..], [(before[1], [0.0, 1.0]), (own[0], [right, 0.0]), (own[1], [right, 1.0])]);
        assert_eq!((vertices[119].position, vertices[119].uv), (wind.point(strand, 20)[1], [20.0 / 21.0, 1.0]));
    }
}
