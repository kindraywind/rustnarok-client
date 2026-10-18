//! RSM 1.x models (the format of classic map objects) and the matrix math to place them.
//!
//! Layouts and node transforms follow the public format documentation.
//! Coordinates stay in the files' native space here (Y down); the world flips Y.

use std::io;

/// Column-major 4x4 matrix.
pub type Matrix = [[f32; 4]; 4];
pub type Vector = [f32; 3];

pub const IDENTITY: Matrix = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];

pub fn multiply(a: &Matrix, b: &Matrix) -> Matrix {
    let mut result = [[0.0; 4]; 4];
    for column in 0..4 {
        for row in 0..4 {
            result[column][row] = (0..4).map(|k| a[k][row] * b[column][k]).sum();
        }
    }
    result
}

pub fn transform(matrix: &Matrix, point: Vector) -> Vector {
    [0, 1, 2].map(|row| matrix[0][row] * point[0] + matrix[1][row] * point[1] + matrix[2][row] * point[2] + matrix[3][row])
}

pub fn translation(offset: Vector) -> Matrix {
    let mut matrix = IDENTITY;
    matrix[3] = [offset[0], offset[1], offset[2], 1.0];
    matrix
}

pub fn scaling(scale: Vector) -> Matrix {
    [[scale[0], 0.0, 0.0, 0.0], [0.0, scale[1], 0.0, 0.0], [0.0, 0.0, scale[2], 0.0], [0.0, 0.0, 0.0, 1.0]]
}

/// Rotation by `angle` radians around `axis` (identity for a zero axis).
pub fn rotation(axis: Vector, angle: f32) -> Matrix {
    let length = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
    if length < 1e-6 || angle == 0.0 {
        return IDENTITY;
    }
    let [x, y, z] = axis.map(|value| value / length);
    let (sin, cos) = angle.sin_cos();
    let t = 1.0 - cos;
    [
        [t * x * x + cos, t * x * y + sin * z, t * x * z - sin * y, 0.0],
        [t * x * y - sin * z, t * y * y + cos, t * y * z + sin * x, 0.0],
        [t * x * z + sin * y, t * y * z - sin * x, t * z * z + cos, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

/// Rotation from a unit quaternion `[x, y, z, w]`.
pub fn quaternion([x, y, z, w]: [f32; 4]) -> Matrix {
    [
        [1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y + z * w), 2.0 * (x * z - y * w), 0.0],
        [2.0 * (x * y - z * w), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z + x * w), 0.0],
        [2.0 * (x * z + y * w), 2.0 * (y * z - x * w), 1.0 - 2.0 * (x * x + y * y), 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> io::Result<&'a [u8]> {
        let slice =
            self.bytes.get(self.position..self.position + count).ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "model file is truncated"))?;
        self.position += count;
        Ok(slice)
    }

    fn u16(&mut self) -> io::Result<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }

    fn i32(&mut self) -> io::Result<i32> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn count(&mut self) -> io::Result<usize> {
        Ok(self.i32()?.max(0) as usize)
    }

    fn f32(&mut self) -> io::Result<f32> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn vector(&mut self) -> io::Result<Vector> {
        Ok([self.f32()?, self.f32()?, self.f32()?])
    }

    fn string(&mut self, length: usize) -> io::Result<String> {
        let bytes = self.take(length)?;
        let end = bytes.iter().position(|&byte| byte == 0).unwrap_or(length);
        Ok(encoding_rs::EUC_KR.decode_without_bom_handling(&bytes[..end]).0.into_owned())
    }
}

pub struct Face {
    pub vertices: [u16; 3],
    pub uvs: [u16; 3],
    /// Index into the node's texture list.
    pub texture: u16,
}

pub struct Node {
    pub name: String,
    pub parent: String,
    /// Indices into the model's texture names.
    pub textures: Vec<usize>,
    pub offset: Matrix,
    pub translation1: Vector,
    pub translation2: Vector,
    pub rotation_angle: f32,
    pub rotation_axis: Vector,
    pub scale: Vector,
    pub vertices: Vec<Vector>,
    pub uvs: Vec<[f32; 2]>,
    pub faces: Vec<Face>,
    /// First rotation keyframe, which replaces the static rotation (animation isn't played yet).
    pub keyframe_rotation: Option<[f32; 4]>,
}

pub struct Rsm {
    pub textures: Vec<String>,
    pub root: String,
    pub nodes: Vec<Node>,
}

/// A model triangle in model space (Y down, bottom center at the origin), with its texture name index.
pub struct Triangle {
    pub texture: usize,
    pub positions: [Vector; 3],
    pub uvs: [[f32; 2]; 3],
}

impl Rsm {
    pub fn parse(bytes: &[u8]) -> io::Result<Self> {
        let mut reader = Reader { bytes, position: 0 };
        if reader.take(4)? != b"GRSM" {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "not an RSM model"));
        }
        let version = (reader.take(1)?[0], reader.take(1)?[0]);
        if version >= (2, 0) {
            return Err(io::Error::new(io::ErrorKind::Unsupported, format!("RSM {}.{} isn't supported yet", version.0, version.1)));
        }
        reader.i32()?; // animation length
        reader.i32()?; // shade type
        if version >= (1, 4) {
            reader.take(1)?; // alpha
        }
        reader.take(16)?;
        let textures = (0..reader.count()?).map(|_| reader.string(40)).collect::<io::Result<_>>()?;
        let root = reader.string(40)?;

        let mut nodes = Vec::new();
        for _ in 0..reader.count()? {
            let name = reader.string(40)?;
            let parent = reader.string(40)?;
            let textures = (0..reader.count()?).map(|_| reader.count()).collect::<io::Result<_>>()?;
            let columns: Vec<f32> = (0..9).map(|_| reader.f32()).collect::<io::Result<_>>()?;
            let offset = [
                [columns[0], columns[1], columns[2], 0.0],
                [columns[3], columns[4], columns[5], 0.0],
                [columns[6], columns[7], columns[8], 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ];
            let translation1 = reader.vector()?;
            let translation2 = reader.vector()?;
            let rotation_angle = reader.f32()?;
            let rotation_axis = reader.vector()?;
            let scale = reader.vector()?;
            let vertices = (0..reader.count()?).map(|_| reader.vector()).collect::<io::Result<_>>()?;
            let uvs = (0..reader.count()?)
                .map(|_| {
                    if version >= (1, 2) {
                        reader.take(4)?; // color
                    }
                    Ok([reader.f32()?, reader.f32()?])
                })
                .collect::<io::Result<_>>()?;
            let mut faces = Vec::new();
            for _ in 0..reader.count()? {
                let vertices = [reader.u16()?, reader.u16()?, reader.u16()?];
                let uvs = [reader.u16()?, reader.u16()?, reader.u16()?];
                let texture = reader.u16()?;
                reader.u16()?; // padding
                reader.i32()?; // two-sided
                if version >= (1, 2) {
                    reader.i32()?; // smoothing group
                }
                faces.push(Face { vertices, uvs, texture });
            }
            if version >= (1, 6) {
                let count = reader.count()?;
                reader.take(count * 20)?; // scale keyframes
            }
            let mut keyframe_rotation = None;
            for _ in 0..reader.count()? {
                reader.i32()?; // frame
                let rotation = [reader.f32()?, reader.f32()?, reader.f32()?, reader.f32()?];
                keyframe_rotation.get_or_insert(rotation);
            }
            nodes.push(Node {
                name,
                parent,
                textures,
                offset,
                translation1,
                translation2,
                rotation_angle,
                rotation_axis,
                scale,
                vertices,
                uvs,
                faces,
                keyframe_rotation,
            });
        }
        Ok(Self { textures, root, nodes })
    }

    /// All triangles in model space, with the model's bottom center at the origin.
    pub fn triangles(&self) -> Vec<Triangle> {
        let mut triangles = Vec::new();
        let Some(root) = self.nodes.iter().position(|node| node.name == self.root).or((!self.nodes.is_empty()).then_some(0)) else {
            return triangles;
        };
        let mut visited = vec![false; self.nodes.len()];
        self.collect(root, &IDENTITY, &mut visited, &mut triangles);

        // RSM 1: center X/Z on the bounding box and put its bottom (max Y, since Y points down) at 0.
        let mut minimum = [f32::MAX; 3];
        let mut maximum = [f32::MIN; 3];
        for position in triangles.iter().flat_map(|triangle| triangle.positions) {
            for axis in 0..3 {
                minimum[axis] = minimum[axis].min(position[axis]);
                maximum[axis] = maximum[axis].max(position[axis]);
            }
        }
        let shift = [-(minimum[0] + maximum[0]) / 2.0, -maximum[1], -(minimum[2] + maximum[2]) / 2.0];
        for position in triangles.iter_mut().flat_map(|triangle| triangle.positions.iter_mut()) {
            *position = [position[0] + shift[0], position[1] + shift[1], position[2] + shift[2]];
        }
        triangles
    }

    fn collect(&self, index: usize, parent: &Matrix, visited: &mut [bool], triangles: &mut Vec<Triangle>) {
        visited[index] = true;
        let node = &self.nodes[index];
        let rotation_matrix = match node.keyframe_rotation {
            Some(rotation) => quaternion(rotation),
            None => rotation(node.rotation_axis, node.rotation_angle),
        };
        // Children inherit translation2 · rotation · scale; translation1 · offset only moves this node's vertices.
        let inherited = multiply(parent, &multiply(&translation(node.translation2), &multiply(&rotation_matrix, &scaling(node.scale))));
        let local = multiply(&inherited, &multiply(&translation(node.translation1), &node.offset));

        for face in &node.faces {
            let Some(texture) = node.textures.get(usize::from(face.texture)).copied() else { continue };
            let positions = face.vertices.map(|vertex| node.vertices.get(usize::from(vertex)).map(|&point| transform(&local, point)));
            let uvs = face.uvs.map(|uv| node.uvs.get(usize::from(uv)).copied().unwrap_or_default());
            if let [Some(a), Some(b), Some(c)] = positions {
                triangles.push(Triangle { texture, positions: [a, b, c], uvs });
            }
        }
        for child in 0..self.nodes.len() {
            if !visited[child] && self.nodes[child].parent == node.name && self.nodes[child].name != node.name {
                self.collect(child, &inherited, visited, triangles);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grf::Grf;
    use crate::map::Rsw;

    #[test]
    fn rotations_compose_like_matrices() {
        let quarter = rotation([0.0, 1.0, 0.0], std::f32::consts::FRAC_PI_2);
        let rotated = transform(&quarter, [1.0, 0.0, 0.0]);
        assert!((rotated[2] + 1.0).abs() < 1e-5 && rotated[0].abs() < 1e-5, "{rotated:?}");
        let same = quaternion([0.0, (std::f32::consts::FRAC_PI_4).sin(), 0.0, (std::f32::consts::FRAC_PI_4).cos()]);
        let rotated_too = transform(&same, [1.0, 0.0, 0.0]);
        assert!((rotated_too[2] + 1.0).abs() < 1e-5, "{rotated_too:?}");
        let moved = transform(&multiply(&translation([1.0, 2.0, 3.0]), &scaling([2.0; 3])), [1.0, 1.0, 1.0]);
        assert_eq!(moved, [3.0, 4.0, 5.0]);
    }

    /// Needs the official data.grf, at `RUSTNAROK_CLIENT_GRF` or `./data.grf`.
    #[test]
    fn bakes_prontera_models() {
        let Some(grf) = Grf::for_tests() else { return };
        let rsw = Rsw::parse(&grf.read("data\\prontera.rsw").unwrap()).unwrap();
        let mut files: Vec<&str> = rsw.models.iter().map(|model| model.file.as_str()).collect();
        files.sort();
        files.dedup();
        for file in files {
            let rsm = Rsm::parse(&grf.read(&format!("data\\model\\{file}")).unwrap()).unwrap();
            let triangles = rsm.triangles();
            assert!(!triangles.is_empty(), "{file} has no triangles");
            assert!(triangles.iter().all(|triangle| triangle.texture < rsm.textures.len()), "{file}");
            // Bottom at 0 with Y pointing down: nothing below the ground.
            let lowest = triangles.iter().flat_map(|triangle| triangle.positions).map(|position| position[1]).fold(f32::MIN, f32::max);
            assert!(lowest.abs() < 1e-3, "{file}: {lowest}");
        }
    }
}
