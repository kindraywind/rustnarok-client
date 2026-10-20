//! Chemical Protection's star (EF_CHEMICALPROTECTION): eight rays of light, 100 units long, flashing from the target's feet
//! in a plane that faces the camera, held, and gone by tick 31.

use std::time::Instant;

use super::passes;
use crate::world::Vector;

pub const TEXTURE: &str = "data\\texture\\effect\\shockwave_c.bmp";
/// A ray's length in sprite pixels: 100 world units of 7 pixels.
const LENGTH: f32 = 700.0;
/// Two prims of four rays, 90° apart, the second 30° on from the first.
const ANGLES: [f32; 8] = [30.0, 120.0, 210.0, 300.0, 60.0, 150.0, 240.0, 330.0];
/// Drawn on ticks 0 to 30.
const TICKS: u32 = 31;
/// The ray's second triangle, wider and blue-white (#8282FF).
const BLUE: [f32; 3] = [130.0 / 255.0, 130.0 / 255.0, 1.0];

/// A triangle of a ray, as a degenerate quad over `at`: its corners in sprite pixels (y down) and colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Triangle {
    pub at: Vector,
    pub corners: [[f32; 2]; 4],
    pub tint: [f32; 4],
}

#[derive(Default)]
pub struct Stars {
    list: Vec<(Vector, Instant)>,
}

/// The rays' alpha at n (from 1): 18 a tick to 180, held, then 15 less a tick from n = 21, 0 from n = 32.
pub fn alpha(n: u32) -> f32 {
    match n {
        1..=10 => 18.0 * n as f32,
        11..=20 => 180.0,
        21..=31 => 180.0 - 15.0 * (n - 20) as f32,
        _ => 0.0,
    }
}

/// The point `LENGTH` out at `degrees` in the screen's plane.
fn point(degrees: f32) -> [f32; 2] {
    let (sin, cos) = degrees.to_radians().sin_cos();
    [LENGTH * cos, -LENGTH * sin]
}

impl Stars {
    /// A star at the target's feet `at`.
    pub fn play(&mut self, at: Vector, now: Instant) {
        self.list.push((at, now));
    }

    pub fn advance(&mut self, now: Instant) {
        self.list.retain(|&(_, start)| passes(start, now) <= TICKS);
    }

    /// The triangles at `now`: per ray a white one at half the alpha, ±3° wide, then a blue one, ±5° wide.
    pub fn triangles(&self, now: Instant) -> Vec<Triangle> {
        let mut triangles = Vec::new();
        for &(at, start) in &self.list {
            let n = passes(start, now);
            let alpha = alpha(n);
            if n == 0 || n > TICKS || alpha <= 0.0 {
                continue;
            }
            for phi in ANGLES {
                for (width, rgb, share) in [(3.0, [1.0; 3], 0.5), (5.0, BLUE, 1.0)] {
                    // Corner 0 the centre (uv 0, 1), 1 the ray at φ − width (1, 1), 2 and 3 the one at φ + width (1, 0).
                    let (behind, ahead) = (point(phi - width), point(phi + width));
                    let tint = [rgb[0], rgb[1], rgb[2], (alpha * share).trunc() / 255.0];
                    triangles.push(Triangle { at, corners: [[0.0, 0.0], behind, ahead, ahead], tint });
                }
            }
        }
        triangles
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prims::TICK;

    #[test]
    fn eight_rays_flash_hold_and_fade_by_tick_31() {
        let start = Instant::now();
        let mut stars = Stars::default();
        stars.play([3.0, 0.0, 4.0], start);
        // Alpha at n = 1, 10, 20, 21, 31, 32: 18, 180, 180, 165, 15, 0.
        assert_eq!([1, 10, 20, 21, 31, 32].map(alpha), [18.0, 180.0, 180.0, 165.0, 15.0, 0.0]);
        // Eight rays of two triangles each; the angles of the two prims.
        let at = |tick: u32| stars.triangles(start + TICK * tick);
        assert_eq!(at(0).len(), 16);
        let mut angles: Vec<f32> = ANGLES.to_vec();
        angles.sort_by(f32::total_cmp);
        assert_eq!(angles, [30.0, 60.0, 120.0, 150.0, 210.0, 240.0, 300.0, 330.0]);
        // The white triangle's alpha is half (n = 10: 90), the blue one's whole; ±3° and ±5° wide, 700 px out.
        let tick9 = at(9);
        assert_eq!((tick9[0].tint[3] * 255.0).round(), 90.0);
        assert_eq!((tick9[1].tint[3] * 255.0).round(), 180.0);
        assert_eq!(tick9[0].tint[..3], [1.0; 3]);
        assert_eq!(tick9[1].tint[..3], BLUE);
        let corners = tick9[0].corners;
        assert_eq!(corners[0], [0.0, 0.0]);
        assert!((f32::hypot(corners[1][0], corners[1][1]) - 700.0).abs() < 0.01 && corners[2] == corners[3]);
        let angle = |corner: [f32; 2]| (-corner[1]).atan2(corner[0]).to_degrees();
        assert!((angle(corners[1]) - 27.0).abs() < 0.01 && (angle(corners[2]) - 33.0).abs() < 0.01);
        assert!((angle(tick9[1].corners[1]) - 25.0).abs() < 0.01 && (angle(tick9[1].corners[2]) - 35.0).abs() < 0.01);
        // At the target's feet, drawn on ticks 0…30, gone at 31.
        assert!(at(0).iter().all(|triangle| triangle.at == [3.0, 0.0, 4.0]));
        assert_eq!(at(30).len(), 16);
        assert!(at(31).is_empty());
        stars.advance(start + TICK * 32);
        assert!(stars.list.is_empty());
    }
}
