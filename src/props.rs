//! Units the data draws as Granny models instead of sprites: the Emperium, the guardians and the guild flags. Each is a
//! still mesh in its bind pose, standing on its cell, turned by its facing, lit by the map's light and depth-tested like
//! the map's own models.
//!
//! ponytail: no animation (the flag doesn't wave), no skeleton; the guild flag's emblem plate isn't the guild's emblem (the
//! file's own picture of it is blank).

use rustnarok_client::gr2::{self, Model};
use rustnarok_client::grf::Grf;

use crate::render::{Gpu, Texture};
use crate::world::{MeshVertex, Vector};

/// World units per model unit. ponytail: a guess (the models are in inches and nothing in the data says how big a cell
/// is to them); the Emperium comes out about a cell and a half wide.
pub const SCALE: f32 = 0.7;
/// The folder the models' textures are in.
const TEXTURES: &str = "data\\texture\\기타마을\\";
const GREY: [u8; 4] = [150, 150, 150, 255];

/// Whether a job's name from the data is a Granny model rather than a sprite.
pub fn is_model(name: &str) -> bool {
    name.len() > 4 && name[name.len() - 4..].eq_ignore_ascii_case(".gr2")
}

/// A model's file in the data.
pub fn model_path(name: &str) -> String {
    format!("data\\model\\3dmob\\{name}")
}

/// The texture files a mesh's exporter file name can mean, best first: the same name as a bitmap, and with the `_01`
/// the Emperium's has.
pub fn texture_paths(exporter_name: &str) -> Vec<String> {
    let base = gr2::base_name(exporter_name);
    vec![format!("{TEXTURES}{base}.bmp"), format!("{TEXTURES}{base}_01.bmp")]
}

/// A vertex in world units, the model standing on the origin facing south (+z is north, y up).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Corner {
    pub position: Vector,
    pub normal: Vector,
    pub uv: [f32; 2],
}

/// The model's axes as the world has them: its right is west (the model is seen from its front), its up is up and its
/// front is south.
fn to_world([x, y, z]: Vector) -> Vector {
    [-x, z, -y]
}

/// Turns about the vertical so that what faced south faces `rot` degrees (0 south, 90 east).
pub fn turn([x, y, z]: Vector, rot: f32) -> Vector {
    let (sin, cos) = rot.to_radians().sin_cos();
    [x * cos - z * sin, y, x * sin + z * cos]
}

/// The pieces of a model by texture file name: the corners of each triangle, in world units.
pub fn parts(model: &Model) -> Vec<(String, Vec<Corner>)> {
    let mut parts: Vec<(String, Vec<Corner>)> = Vec::new();
    for mesh in &model.meshes {
        for (first, count, texture) in &mesh.groups {
            let index = parts.iter().position(|(name, _)| name == texture).unwrap_or_else(|| {
                parts.push((texture.clone(), Vec::new()));
                parts.len() - 1
            });
            for triangle in &mesh.triangles[*first..first + count] {
                for &corner in triangle {
                    let vertex = &mesh.vertices[corner as usize];
                    parts[index].1.push(Corner {
                        position: to_world(vertex.position).map(|value| value * SCALE),
                        normal: to_world(vertex.normal),
                        // The exporter counts v up from the bottom.
                        uv: [vertex.uv[0], 1.0 - vertex.uv[1]],
                    });
                }
            }
        }
    }
    parts.retain(|(_, corners)| !corners.is_empty());
    parts
}

/// min(1, d · diffuse + ambient) per channel, d the normal's light (the map's light: the direction toward it, then
/// its ambient and diffuse colours).
pub fn lit(normal: Vector, (direction, ambient, diffuse): (Vector, [f32; 3], [f32; 3])) -> [f32; 3] {
    let length = normal.iter().map(|value| value * value).sum::<f32>().sqrt().max(1e-6);
    let d = (normal.iter().zip(direction).map(|(a, b)| a * b).sum::<f32>() / length).max(0.0);
    std::array::from_fn(|channel| (d * diffuse[channel] + ambient[channel]).min(1.0))
}

/// A model ready to draw.
pub struct Prop {
    parts: Vec<(Texture, Vec<Corner>)>,
    low: Vector,
    high: Vector,
}

impl Prop {
    pub fn load(gpu: &Gpu, grf: &Grf, name: &str) -> Option<Self> {
        let model = Model::parse(&grf.read(&model_path(name)).ok()?).ok()?;
        let parts = parts(&model)
            .into_iter()
            .map(|(file, corners)| {
                // The picture the file carries first, then a bitmap of the same name in the data, then grey.
                let embedded = model
                    .textures
                    .iter()
                    .find(|(name, _)| *name == file)
                    .and_then(|(_, image)| image::RgbaImage::from_raw(image.width as u32, image.height as u32, image.rgba.clone()));
                let texture = embedded
                    .map(|image| gpu.texture(&image, true))
                    .or_else(|| texture_paths(&file).iter().find_map(|path| gpu.try_load_texture(grf, path, true).ok()))
                    .unwrap_or_else(|| gpu.solid(GREY));
                (texture, corners)
            })
            .collect::<Vec<_>>();
        let mut every = parts.iter().flat_map(|(_, corners)| corners.iter().map(|corner| corner.position));
        let first = every.next()?;
        let (low, high) = every.fold((first, first), |(low, high), point| {
            (std::array::from_fn(|axis| low[axis].min(point[axis])), std::array::from_fn(|axis| high[axis].max(point[axis])))
        });
        Some(Self { parts, low, high })
    }

    /// The triangles, per texture, standing at `feet` turned to `rot`, lit by `light`.
    pub fn meshes(&self, feet: Vector, rot: f32, light: (Vector, [f32; 3], [f32; 3])) -> Vec<(&Texture, Vec<MeshVertex>, bool)> {
        self.parts
            .iter()
            .map(|(texture, corners)| {
                let vertices = corners
                    .iter()
                    .map(|corner| {
                        let at = turn(corner.position, rot);
                        let color = lit(turn(corner.normal, rot), light);
                        MeshVertex { position: [feet[0] + at[0], feet[1] + at[1], feet[2] + at[2]], uv: corner.uv, tint: [color[0], color[1], color[2], 1.0] }
                    })
                    .collect();
                (texture, vertices, false)
            })
            .collect()
    }

    /// The corners of the box the model fits in, standing at `feet` turned to `rot`.
    pub fn box_corners(&self, feet: Vector, rot: f32) -> [Vector; 8] {
        std::array::from_fn(|corner| {
            let pick = |axis: usize| if corner >> axis & 1 == 1 { self.high[axis] } else { self.low[axis] };
            let at = turn([pick(0), pick(1), pick(2)], rot);
            [feet[0] + at[0], feet[1] + at[1], feet[2] + at[2]]
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: Vector, b: Vector) {
        assert!(a.iter().zip(b).all(|(a, b)| (a - b).abs() < 1e-5), "{a:?} != {b:?}");
    }

    #[test]
    fn a_model_faces_south_standing_up() {
        // Granny's front is +y, its right +x and its up +z; the world's south is -z, west -x and up +y.
        near(to_world([0.0, 1.0, 0.0]), [0.0, 0.0, -1.0]);
        near(to_world([1.0, 0.0, 0.0]), [-1.0, 0.0, 0.0]);
        near(to_world([0.0, 0.0, 1.0]), [0.0, 1.0, 0.0]);
    }

    #[test]
    fn turning_takes_south_to_the_facing() {
        let south = [0.0, 0.0, -1.0];
        near(turn(south, 0.0), south);
        near(turn(south, 90.0), [1.0, 0.0, 0.0]);
        near(turn(south, 180.0), [0.0, 0.0, 1.0]);
        near(turn([0.0, 2.0, 0.0], 45.0), [0.0, 2.0, 0.0]);
    }

    #[test]
    fn light_is_the_ambient_turned_away_and_capped() {
        let light = ([0.0, 1.0, 0.0], [0.3, 0.3, 0.3], [0.5, 0.5, 0.5]);
        near(lit([0.0, -1.0, 0.0], light), [0.3; 3]);
        near(lit([0.0, 1.0, 0.0], light), [0.8; 3]);
        near(lit([0.0, 50.0, 0.0], ([0.0, 1.0, 0.0], [0.9; 3], [0.5; 3])), [1.0; 3]);
    }

    #[test]
    fn a_job_name_is_a_model_by_its_extension() {
        assert!(is_model("Empelium90_0.gr2"));
        assert!(is_model("a.GR2"));
        assert!(!is_model("poring"));
        assert!(!is_model(".gr2"));
        assert_eq!(model_path("Kguardian90_7.gr2"), "data\\model\\3dmob\\Kguardian90_7.gr2");
    }

    #[test]
    fn textures_are_the_exporters_name_as_a_bitmap() {
        assert_eq!(
            texture_paths("D:\\ChalesWORK_2003\\0124HG\\guardian copy.tif"),
            vec![format!("{TEXTURES}guardian copy.bmp"), format!("{TEXTURES}guardian copy_01.bmp")]
        );
    }
}
