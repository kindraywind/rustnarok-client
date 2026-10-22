//! RSM 1.x models (the format of classic map objects) and the matrix math to place them.
//!
//! Layouts and node transforms follow the public format documentation.
//! Coordinates stay in the files' native space here (Y down); the world flips Y.

use std::collections::HashMap;
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

    /// A string after its length (RSM 2).
    fn sized_string(&mut self) -> io::Result<String> {
        let length = self.count()?;
        self.string(length)
    }
}

pub struct Face {
    pub vertices: [u16; 3],
    pub uvs: [u16; 3],
    /// Index into the node's texture list (wrapped by its length).
    pub texture: u16,
    /// Drawn from both sides; and the smoothing group its corners' normals are shared in (RSM 1.2+, else 0).
    pub two_sided: bool,
    pub smoothing: i32,
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
    /// First rotation keyframe, which replaces the static rotation when the model is drawn still.
    pub keyframe_rotation: Option<[f32; 4]>,
    /// The tracks, by frame: scale, rotation, position, and the texture coordinate tracks (RSM 2); RSM 1 has its
    /// rotation keys only.
    pub tracks: Tracks,
}

/// An RSM 2 node's animation.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Tracks {
    pub scale: Vec<(i32, Vector)>,
    pub rotation: Vec<(i32, [f32; 4])>,
    pub position: Vec<(i32, Vector)>,
    pub uv: Vec<UvTrack>,
}

/// A texture of a node (by its place in the node's list), what moves on it, and its keys by frame.
pub type UvTrack = (usize, i32, Vec<(i32, f32)>);

impl Tracks {
    fn moves(&self) -> bool {
        self.scale.len() > 1 || self.rotation.len() > 1 || self.position.len() > 1 || !self.uv.is_empty()
    }
}

/// The value of a track at `frame`: the keys before the first and after the last hold, between them a blend.
fn sample<T: Copy>(keys: &[(i32, T)], frame: f32, blend: impl Fn(T, T, f32) -> T) -> Option<T> {
    let (first, last) = (keys.first()?, keys.last()?);
    if frame <= first.0 as f32 {
        return Some(first.1);
    }
    if frame >= last.0 as f32 {
        return Some(last.1);
    }
    let next = keys.iter().position(|(at, _)| *at as f32 > frame)?;
    let (from, to) = (keys[next - 1], keys[next]);
    let span = (to.0 - from.0).max(1) as f32;
    Some(blend(from.1, to.1, (frame - from.0 as f32) / span))
}

fn lerp(a: Vector, b: Vector, t: f32) -> Vector {
    std::array::from_fn(|axis| a[axis] + (b[axis] - a[axis]) * t)
}

/// The shorter arc between two rotations.
fn slerp(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    let mut dot: f32 = (0..4).map(|i| a[i] * b[i]).sum();
    let b = if dot < 0.0 {
        dot = -dot;
        b.map(|value| -value)
    } else {
        b
    };
    if dot > 0.9995 {
        let mixed: [f32; 4] = std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t);
        let length = mixed.iter().map(|value| value * value).sum::<f32>().sqrt().max(f32::EPSILON);
        return mixed.map(|value| value / length);
    }
    let angle = dot.clamp(-1.0, 1.0).acos();
    let (wa, wb) = (((1.0 - t) * angle).sin() / angle.sin(), (t * angle).sin() / angle.sin());
    std::array::from_fn(|i| a[i] * wa + b[i] * wb)
}

/// A texture coordinate under a UV track's value.
// ponytail: what each kind moves is read from the data's use (kind 1, the most common, scrolls v), not documented:
// 0 shifts u, 1 shifts v, 2 scales u, 3 scales v, 4 turns the coordinate about the texture's centre (radians).
fn moved_uv([u, v]: [f32; 2], kind: i32, value: f32) -> [f32; 2] {
    match kind {
        0 => [u + value, v],
        1 => [u, v + value],
        2 => [u * value, v],
        3 => [u, v * value],
        4 => {
            let (sin, cos) = value.sin_cos();
            let (x, y) = (u - 0.5, v - 0.5);
            [x * cos - y * sin + 0.5, x * sin + y * cos + 0.5]
        }
        _ => [u, v],
    }
}

/// A face's normal from its corners (y down): normalize((v1 − v2) × (v0 − v2)), which points down (+y) on a face
/// that looks up.
fn face_normal([v0, v1, v2]: [Vector; 3]) -> Vector {
    let (a, b) = (std::array::from_fn(|axis| v1[axis] - v2[axis]), std::array::from_fn(|axis| v0[axis] - v2[axis]));
    normalized(cross(a, b))
}

/// Each face's corner normals in the node's own space (after its offset matrix and offset translation): shade 1 the
/// face's normal on all three; otherwise (2, and any other) the sum of the normals of the node's faces sharing that
/// vertex and smoothing group, normalized. Shade 0 isn't lit, so its normals don't matter.
fn corner_normals(node: &Node, own: &Matrix, shade: i32) -> Vec<[Vector; 3]> {
    let points: Vec<Vector> = node.vertices.iter().map(|&point| transform(own, point)).collect();
    let corners = |face: &Face| face.vertices.map(|vertex| points.get(usize::from(vertex)).copied().unwrap_or_default());
    let faces: Vec<Vector> = node.faces.iter().map(|face| face_normal(corners(face))).collect();
    if shade == 1 {
        return faces.iter().map(|&normal| [normal; 3]).collect();
    }
    let mut shared: HashMap<(u16, i32), Vector> = HashMap::new();
    for (face, normal) in node.faces.iter().zip(&faces) {
        for vertex in face.vertices {
            let sum = shared.entry((vertex, face.smoothing)).or_default();
            *sum = std::array::from_fn(|axis| sum[axis] + normal[axis]);
        }
    }
    node.faces.iter().map(|face| face.vertices.map(|vertex| normalized(shared[&(vertex, face.smoothing)]))).collect()
}

/// What only moves a node's own vertices: translation1 · offset.
fn own_matrix(node: &Node) -> Matrix {
    multiply(&translation(node.translation1), &node.offset)
}

fn cross(a: Vector, b: Vector) -> Vector {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn normalized(vector: Vector) -> Vector {
    let length = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
    if length > 0.0 { vector.map(|value| value / length) } else { vector }
}

/// A direction under a matrix's 3 × 3 part (scale included, no inverse transpose).
pub fn direction(matrix: &Matrix, vector: Vector) -> Vector {
    [0, 1, 2].map(|row| matrix[0][row] * vector[0] + matrix[1][row] * vector[1] + matrix[2][row] * vector[2])
}

/// The shift that puts triangles' bounding box centre (x, z) and bottom (max y, as y points down) on the origin.
fn bottom_centre(triangles: &[Triangle]) -> Vector {
    let mut minimum = [f32::MAX; 3];
    let mut maximum = [f32::MIN; 3];
    for position in triangles.iter().flat_map(|triangle| triangle.positions) {
        for axis in 0..3 {
            minimum[axis] = minimum[axis].min(position[axis]);
            maximum[axis] = maximum[axis].max(position[axis]);
        }
    }
    [-(minimum[0] + maximum[0]) / 2.0, -maximum[1], -(minimum[2] + maximum[2]) / 2.0]
}

/// An RSM 1 node's rotation at `frame`: its one key; else between the first key i ≥ 1 later than the frame and the one
/// before it (shortest arc; before key 0 that's keys 0 and 1 with a negative fraction); past the last key, none.
pub fn rsm1_rotation(keys: &[(i32, [f32; 4])], frame: f32) -> [f32; 4] {
    if let [(_, only)] = keys {
        return *only;
    }
    let Some(next) = keys.iter().skip(1).position(|&(at, _)| at as f32 > frame).map(|index| index + 1) else { return [0.0, 0.0, 0.0, 1.0] };
    let (from, to) = (keys[next - 1], keys[next]);
    let span = (to.0 - from.0) as f32;
    slerp(from.1, to.1, if span == 0.0 { 0.0 } else { (frame - from.0 as f32) / span })
}

/// An RSM 1 instance's next frame, stepped once per rendered frame in which a 16 ms tick passed: trunc(frame +
/// speed / 3 × 100), +33 at speed 1. Type 2 starts again from 0 at the length, type 1 holds at length − 1, any other
/// stays at 0.
pub fn rsm1_step(frame: i32, speed: f32, anim_type: i32, length: i32) -> i32 {
    let next = (frame as f32 + speed / 3.0 * 100.0).trunc() as i32;
    match anim_type {
        2 if next >= length => 0,
        2 => next,
        1 => next.min((length - 1).max(0)),
        _ => 0,
    }
}

/// The frame an RSM 2 instance shows `elapsed_ms` after its clock started, and whether its loop starts over now:
/// t = elapsed × speed (whole), a length of frames × 1 000 / fps ms; type 2 starts again from 0 at the length, type 1
/// holds at length − 1, any other stays at 0. Tracks are read at t × fps / 1 000.
pub fn clock(elapsed_ms: u64, speed: f32, anim_type: i32, frames: i32, fps: f32) -> (f32, bool) {
    let t = (elapsed_ms as f64 * f64::from(speed)).trunc();
    let length = (f64::from(frames.max(0)) * 1000.0 / f64::from(fps.max(f32::EPSILON))).floor();
    let (t, again) = match anim_type {
        2 if t >= length => (0.0, true),
        2 => (t, false),
        1 => (t.min((length - 1.0).max(0.0)), false),
        _ => (0.0, false),
    };
    ((t * f64::from(fps) / 1000.0) as f32, again)
}

pub struct Rsm {
    /// The file's version: 1.x, or 2.2 / 2.3 (`.rsm2`).
    pub version: (u8, u8),
    pub textures: Vec<String>,
    pub root: String,
    pub nodes: Vec<Node>,
    /// The animation's length in frames (RSM 1's header, RSM 2's), and RSM 2's frames a second.
    pub frames: i32,
    pub fps: f32,
    /// How its light is baked: 0 none (the tint), 1 flat (a face's normal), 2 smooth (normals shared by group).
    pub shade: i32,
    /// RSM 1: the shift that puts its frame-0 pose's bottom centre on the origin, kept while it moves.
    centre: std::cell::OnceCell<Vector>,
    /// Each node's `corner_normals`, which no frame changes: found the first time it's posed.
    normals: std::cell::OnceCell<Vec<Vec<[Vector; 3]>>>,
}

/// A model triangle in model space (Y down, bottom center at the origin), with its texture name index, its corners'
/// normals (by the shade type, turned by its node, not yet normalized) and whether it's drawn from both sides.
pub struct Triangle {
    pub texture: usize,
    pub positions: [Vector; 3],
    pub uvs: [[f32; 2]; 3],
    pub normals: [Vector; 3],
    pub two_sided: bool,
}

impl Rsm {
    pub fn parse(bytes: &[u8]) -> io::Result<Self> {
        let mut reader = Reader { bytes, position: 0 };
        if reader.take(4)? != b"GRSM" {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "not an RSM model"));
        }
        let version = (reader.take(1)?[0], reader.take(1)?[0]);
        if version >= (2, 0) {
            return Self::parse_2(&mut reader, version);
        }
        let length = reader.i32()?;
        let shade = reader.i32()?;
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
                let two_sided = reader.i32()? != 0;
                let smoothing = if version >= (1, 2) { reader.i32()? } else { 0 };
                faces.push(Face { vertices, uvs, texture, two_sided, smoothing });
            }
            if version >= (1, 6) {
                let count = reader.count()?;
                reader.take(count * 20)?; // scale keyframes
            }
            let mut rotations = Vec::new();
            for _ in 0..reader.count()? {
                let frame = reader.i32()?;
                rotations.push((frame, [reader.f32()?, reader.f32()?, reader.f32()?, reader.f32()?]));
            }
            let keyframe_rotation = rotations.first().map(|&(_, rotation)| rotation);
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
                tracks: Tracks { rotation: rotations, ..Tracks::default() },
            });
        }
        Ok(Self { version, textures, root, nodes, frames: length, fps: 0.0, shade, centre: Default::default(), normals: Default::default() })
    }

    /// RSM 2.2 and 2.3 (`.rsm2`). Strings carry their length; 2.2 lists the textures once and nodes point into the
    /// list, 2.3 names each node's textures there. A node has its offset matrix and translation, vertices, UVs and
    /// faces (each with its length), then its scale, rotation and position keys and, in 2.3, texture animations. Some
    /// files end with an empty count.
    // ponytail: the first scale, rotation and position keys pose each node, composed as RSM 1's (parent, position,
    // rotation, scale, then the offset matrix for the node's own vertices); the clock that plays the rest is to come.
    fn parse_2(reader: &mut Reader, version: (u8, u8)) -> io::Result<Self> {
        if version < (2, 2) {
            return Err(io::Error::new(io::ErrorKind::Unsupported, format!("RSM {}.{} isn't supported yet", version.0, version.1)));
        }
        let frames = reader.i32()?;
        let shade = reader.i32()?;
        reader.take(1)?; // alpha
        let fps = reader.f32()?;
        let mut textures: Vec<String> = Vec::new();
        if version == (2, 2) {
            for _ in 0..reader.count()? {
                textures.push(reader.sized_string()?);
            }
        }
        let roots: Vec<String> = (0..reader.count()?).map(|_| reader.sized_string()).collect::<io::Result<_>>()?;
        let mut nodes = Vec::new();
        for _ in 0..reader.count()? {
            let name = reader.sized_string()?;
            let parent = reader.sized_string()?;
            let node_textures = if version >= (2, 3) {
                (0..reader.count()?)
                    .map(|_| {
                        let texture = reader.sized_string()?;
                        Ok(textures.iter().position(|known| *known == texture).unwrap_or_else(|| {
                            textures.push(texture);
                            textures.len() - 1
                        }))
                    })
                    .collect::<io::Result<Vec<usize>>>()?
            } else {
                (0..reader.count()?).map(|_| reader.count()).collect::<io::Result<_>>()?
            };
            let columns: Vec<f32> = (0..9).map(|_| reader.f32()).collect::<io::Result<_>>()?;
            let offset = [
                [columns[0], columns[1], columns[2], 0.0],
                [columns[3], columns[4], columns[5], 0.0],
                [columns[6], columns[7], columns[8], 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ];
            let translation = reader.vector()?;
            let vertices = (0..reader.count()?).map(|_| reader.vector()).collect::<io::Result<_>>()?;
            let uvs = (0..reader.count()?)
                .map(|_| {
                    reader.take(4)?; // color
                    Ok([reader.f32()?, reader.f32()?])
                })
                .collect::<io::Result<_>>()?;
            let mut faces = Vec::new();
            for _ in 0..reader.count()? {
                let length = reader.count()?;
                let vertices = [reader.u16()?, reader.u16()?, reader.u16()?];
                let uvs = [reader.u16()?, reader.u16()?, reader.u16()?];
                let texture = reader.u16()?;
                // Padding, two-sided, the smoothing groups.
                // ponytail: RSM 2 faces draw from both sides and smooth as one group, as before.
                reader.take(length.saturating_sub(14))?;
                faces.push(Face { vertices, uvs, texture, two_sided: true, smoothing: 0 });
            }
            // Keys: a frame, then the scale or the position and a word, or the rotation as a quaternion.
            let mut tracks = Tracks::default();
            for _ in 0..reader.count()? {
                let frame = reader.i32()?;
                tracks.scale.push((frame, reader.vector()?));
                reader.take(4)?;
            }
            for _ in 0..reader.count()? {
                let frame = reader.i32()?;
                tracks.rotation.push((frame, [reader.f32()?, reader.f32()?, reader.f32()?, reader.f32()?]));
            }
            for _ in 0..reader.count()? {
                let frame = reader.i32()?;
                tracks.position.push((frame, reader.vector()?));
                reader.take(4)?;
            }
            if version >= (2, 3) {
                for _ in 0..reader.count()? {
                    let texture = reader.count()?;
                    for _ in 0..reader.count()? {
                        let kind = reader.i32()?;
                        let keys = (0..reader.count()?).map(|_| Ok((reader.i32()?, reader.f32()?))).collect::<io::Result<_>>()?;
                        tracks.uv.push((texture, kind, keys));
                    }
                }
            }
            let scale = tracks.scale.first().map(|&(_, scale)| scale);
            let keyframe_rotation = tracks.rotation.first().map(|&(_, rotation)| rotation);
            let position = tracks.position.first().map(|&(_, position)| position);
            nodes.push(Node {
                name,
                parent,
                textures: node_textures,
                offset,
                // The node's place (its first position key, else its translation) moves its children too.
                translation1: [0.0; 3],
                translation2: position.unwrap_or(translation),
                rotation_angle: 0.0,
                rotation_axis: [0.0, 1.0, 0.0],
                scale: scale.unwrap_or([1.0; 3]),
                vertices,
                uvs,
                faces,
                keyframe_rotation,
                tracks,
            });
        }
        let root = roots.into_iter().next().unwrap_or_default();
        Ok(Self { version, textures, root, nodes, frames, fps, shade, centre: Default::default(), normals: Default::default() })
    }

    /// All triangles in model space (y down), with the model's bottom center at the origin for RSM 1; RSM 2 keeps the
    /// file's own origin and is turned the right way up (its y flipped).
    pub fn triangles(&self) -> Vec<Triangle> {
        self.posed(None)
    }

    /// Whether anything in it moves: RSM 2's tracks, or RSM 1 with a length and a node with more than one rotation key.
    pub fn animated(&self) -> bool {
        match self.version >= (2, 0) {
            true => self.nodes.iter().any(|node| node.tracks.moves()),
            false => self.frames > 0 && self.nodes.iter().any(|node| node.tracks.rotation.len() > 1),
        }
    }

    /// The triangles at a (fractional) frame of its tracks (RSM 1: of its rotation keys, centred as at frame 0).
    pub fn triangles_at(&self, frame: f32) -> Vec<Triangle> {
        self.posed(Some(frame))
    }

    fn posed(&self, frame: Option<f32>) -> Vec<Triangle> {
        let mut triangles = Vec::new();
        // RSM 1's root is the first node in the list, whatever the header names.
        let named = if self.version >= (2, 0) { self.nodes.iter().position(|node| node.name == self.root) } else { None };
        let Some(root) = named.or((!self.nodes.is_empty()).then_some(0)) else {
            return triangles;
        };
        let mut visited = vec![false; self.nodes.len()];
        self.collect(root, &IDENTITY, frame, &mut visited, &mut triangles);
        if self.version >= (2, 0) {
            for position in triangles.iter_mut().flat_map(|triangle| triangle.positions.iter_mut()) {
                position[1] = -position[1];
            }
            return triangles;
        }

        // RSM 1: center X/Z on the bounding box and put its bottom (max Y, since Y points down) at 0; a moving one keeps
        // its frame-0 pose's centring.
        let shift = match frame {
            Some(_) => *self.centre.get_or_init(|| {
                let mut still = Vec::new();
                self.collect(root, &IDENTITY, Some(0.0), &mut vec![false; self.nodes.len()], &mut still);
                bottom_centre(&still)
            }),
            None => bottom_centre(&triangles),
        };
        for position in triangles.iter_mut().flat_map(|triangle| triangle.positions.iter_mut()) {
            *position = [position[0] + shift[0], position[1] + shift[1], position[2] + shift[2]];
        }
        triangles
    }

    fn collect(&self, index: usize, parent: &Matrix, frame: Option<f32>, visited: &mut [bool], triangles: &mut Vec<Triangle>) {
        visited[index] = true;
        let node = &self.nodes[index];
        // RSM 2 at a frame: its tracks where they have keys; past its last rotation key a node holds that key.
        let tracks = frame.map(|frame| {
            let scale = sample(&node.tracks.scale, frame, lerp);
            let rotation = sample(&node.tracks.rotation, frame, slerp);
            let position = sample(&node.tracks.position, frame, lerp);
            (scale, rotation, position)
        });
        let keyframe_rotation = tracks.and_then(|(_, rotation, _)| rotation).or(node.keyframe_rotation);
        let rotation_matrix = match (self.version >= (2, 0), frame, keyframe_rotation) {
            // RSM 1 playing: its keys at the frame (identity past the last); a node without keys turns by its axis.
            (false, Some(frame), Some(_)) => quaternion(rsm1_rotation(&node.tracks.rotation, frame)),
            (_, _, Some(rotation)) => quaternion(rotation),
            (_, _, None) => rotation(node.rotation_axis, node.rotation_angle),
        };
        let node_scale = tracks.and_then(|(scale, ..)| scale).unwrap_or(node.scale);
        let node_place = tracks.and_then(|(.., position)| position).unwrap_or(node.translation2);
        // Children inherit translation2 · rotation · scale; translation1 · offset only moves this node's vertices.
        let inherited = multiply(parent, &multiply(&translation(node_place), &multiply(&rotation_matrix, &scaling(node_scale))));
        let local = multiply(&inherited, &own_matrix(node));
        let normals = &self.normals.get_or_init(|| self.nodes.iter().map(|node| corner_normals(node, &own_matrix(node), self.shade)).collect())[index];

        for (number, face) in node.faces.iter().enumerate() {
            let Some(texture) = node.textures.get(usize::from(face.texture) % node.textures.len().max(1)).copied() else { continue };
            let positions = face.vertices.map(|vertex| node.vertices.get(usize::from(vertex)).map(|&point| transform(&local, point)));
            let mut uvs = face.uvs.map(|uv| node.uvs.get(usize::from(uv)).copied().unwrap_or_default());
            if let Some(frame) = frame {
                for (track_texture, kind, keys) in &node.tracks.uv {
                    if *track_texture == usize::from(face.texture)
                        && let Some(value) = sample(keys, frame, |a, b, t| a + (b - a) * t)
                    {
                        uvs = uvs.map(|uv| moved_uv(uv, *kind, value));
                    }
                }
            }
            if let [Some(a), Some(b), Some(c)] = positions {
                let normals = normals[number].map(|normal| direction(&inherited, normal));
                triangles.push(Triangle { texture, positions: [a, b, c], uvs, normals, two_sided: face.two_sided });
            }
        }
        for child in 0..self.nodes.len() {
            if !visited[child] && self.nodes[child].parent == node.name && self.nodes[child].name != node.name {
                self.collect(child, &inherited, frame, visited, triangles);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A node of two faces sharing vertex 0: one flat on y (its normal +y), one on x (+x), in smoothing groups `groups`.
    fn two_faces(name: &str, parent: &str, groups: [i32; 2], textures: Vec<usize>, texture: u16) -> Node {
        let face = |vertices, smoothing| Face { vertices, uvs: [0; 3], texture, two_sided: false, smoothing };
        Node {
            name: name.into(),
            parent: parent.into(),
            textures,
            offset: IDENTITY,
            translation1: [0.0; 3],
            translation2: [0.0; 3],
            rotation_angle: 0.0,
            rotation_axis: [0.0, 1.0, 0.0],
            scale: [1.0; 3],
            vertices: vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]],
            uvs: vec![[0.0; 2]],
            faces: vec![face([0, 1, 2], groups[0]), face([0, 2, 3], groups[1])],
            keyframe_rotation: None,
            tracks: Tracks::default(),
        }
    }

    #[test]
    fn shades_corners_flat_or_smooth_by_group() {
        let node = two_faces("a", "", [1, 1], vec![0], 0);
        // Flat: each face's own normal on its three corners.
        let flat = corner_normals(&node, &IDENTITY, 1);
        assert_eq!((flat[0], flat[1]), ([[0.0, 1.0, 0.0]; 3], [[1.0, 0.0, 0.0]; 3]));
        // Smooth, one group: the shared corner takes both faces' normals; the others their own.
        let smooth = corner_normals(&node, &IDENTITY, 2);
        let both = [std::f32::consts::FRAC_1_SQRT_2, std::f32::consts::FRAC_1_SQRT_2, 0.0];
        assert!(smooth[0][0].iter().zip(both).all(|(a, b)| (a - b).abs() < 1e-6) && smooth[0][1] == [0.0, 1.0, 0.0]);
        // Two groups: nothing shared.
        let apart = corner_normals(&two_faces("a", "", [1, 2], vec![0], 0), &IDENTITY, 2);
        assert_eq!((apart[0][0], apart[1][0]), ([0.0, 1.0, 0.0], [1.0, 0.0, 0.0]));
    }

    #[test]
    fn roots_rsm_1_at_its_first_node_and_wraps_textures() {
        // The header names the second node, a child of the first: the first is the root all the same, so both draw.
        // A face's texture 3 of a list of two is the second.
        let nodes = vec![two_faces("first", "", [0, 0], vec![0, 1], 3), two_faces("second", "first", [0, 0], vec![0], 0)];
        let rsm = Rsm {
            version: (1, 5),
            textures: vec!["a".into(), "b".into()],
            root: "second".into(),
            nodes,
            frames: 0,
            fps: 0.0,
            shade: 1,
            centre: Default::default(),
            normals: Default::default(),
        };
        let triangles = rsm.triangles();
        assert_eq!(triangles.len(), 4);
        assert_eq!(triangles.iter().filter(|triangle| triangle.texture == 1).count(), 2);
    }

    #[test]
    fn steps_an_rsm_1_frame_by_its_type() {
        use crate::map::anim_speed_of;
        // +33 at speed 1; speeds 0 and 150 read as 1.
        assert_eq!([0.0, 150.0, 2.5].map(anim_speed_of), [1.0, 1.0, 2.5]);
        assert_eq!(rsm1_step(0, 1.0, 2, 1000), 33);
        assert_eq!(rsm1_step(33, 2.0, 2, 1000), 99);
        // Type 2 back to 0 at the length, type 1 holding at length − 1, type 0 (or any other) still.
        assert_eq!(rsm1_step(980, 1.0, 2, 1000), 0);
        assert_eq!(rsm1_step(980, 1.0, 1, 1000), 999);
        assert_eq!((rsm1_step(500, 1.0, 0, 1000), rsm1_step(500, 1.0, 7, 1000)), (0, 0));
    }

    #[test]
    fn turns_an_rsm_1_node_by_its_keys() {
        let half_turn = [0.0, 1.0, 0.0, 0.0];
        let still = [0.0, 0.0, 0.0, 1.0];
        let close = |a: [f32; 4], b: [f32; 4]| a.iter().zip(b).all(|(a, b)| (a - b).abs() < 1e-4);
        // One key: that key, at any frame.
        assert_eq!(rsm1_rotation(&[(500, half_turn)], 9_999.0), half_turn);
        let keys = [(100, still), (300, half_turn)];
        // Half-way: a quarter turn about y.
        let quarter = [0.0, std::f32::consts::FRAC_1_SQRT_2, 0.0, std::f32::consts::FRAC_1_SQRT_2];
        assert!(close(rsm1_rotation(&keys, 200.0), quarter), "{:?}", rsm1_rotation(&keys, 200.0));
        // Before key 0: keys 0 and 1 with a negative fraction (−0.5 here), so a quarter turn the other way.
        let before = rsm1_rotation(&keys, 0.0);
        assert!(close(before, [0.0, -std::f32::consts::FRAC_1_SQRT_2, 0.0, std::f32::consts::FRAC_1_SQRT_2]), "{before:?}");
        // At or past the last key: no rotation at all.
        assert_eq!((rsm1_rotation(&keys, 300.0), rsm1_rotation(&keys, 115_100.0)), (still, still));
    }
    use crate::grf::Grf;
    use crate::map::Rsw;

    /// A hand-made RSM 2 of `version`: one node with a triangle, its textures where that version keeps them, a face
    /// with an extra smoothing group, and the empty count some files end with.
    fn rsm_2(version: (u8, u8)) -> Vec<u8> {
        let text = |bytes: &mut Vec<u8>, value: &str| {
            bytes.extend((value.len() as i32).to_le_bytes());
            bytes.extend(value.as_bytes());
        };
        let numbers = |bytes: &mut Vec<u8>, values: &[f32]| values.iter().for_each(|value| bytes.extend(value.to_le_bytes()));
        let mut bytes = b"GRSM".to_vec();
        bytes.extend([version.0, version.1]);
        bytes.extend(100i32.to_le_bytes()); // animation length
        bytes.extend(2i32.to_le_bytes()); // shade
        bytes.push(255);
        numbers(&mut bytes, &[30.0]);
        if version == (2, 2) {
            bytes.extend(1i32.to_le_bytes());
            text(&mut bytes, "trap.bmp");
        }
        bytes.extend(1i32.to_le_bytes());
        text(&mut bytes, "trap");
        bytes.extend(1i32.to_le_bytes());
        text(&mut bytes, "trap");
        text(&mut bytes, "");
        bytes.extend(1i32.to_le_bytes());
        if version == (2, 2) {
            bytes.extend(0i32.to_le_bytes());
        } else {
            text(&mut bytes, "trap.bmp");
        }
        numbers(&mut bytes, &[1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0]);
        bytes.extend(3i32.to_le_bytes());
        numbers(&mut bytes, &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]);
        bytes.extend(3i32.to_le_bytes());
        for uv in [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]] {
            bytes.extend(0u32.to_le_bytes());
            numbers(&mut bytes, &uv);
        }
        bytes.extend(1i32.to_le_bytes());
        bytes.extend(28i32.to_le_bytes());
        for value in [0u16, 1, 2, 0, 1, 2, 0, 0] {
            bytes.extend(value.to_le_bytes());
        }
        for value in [0i32, 1, 2] {
            bytes.extend(value.to_le_bytes());
        }
        // No scale, rotation or position keys; in 2.3 no texture animations either.
        bytes.extend([0; 12]);
        if version >= (2, 3) {
            bytes.extend([0; 4]);
        }
        bytes.extend([0; 4]);
        bytes
    }

    #[test]
    fn plays_rsm_2_on_its_clock() {
        // 100 frames at 30 fps: 3 333 ms. Type 2 starts again at the length; type 1 holds at length - 1; 0 stays.
        assert_eq!(clock(0, 1.0, 2, 100, 30.0), (0.0, false));
        assert_eq!(clock(1000, 1.0, 2, 100, 30.0), (30.0, false));
        assert_eq!(clock(3333, 1.0, 2, 100, 30.0), (0.0, true));
        assert!(!clock(3332, 1.0, 2, 100, 30.0).1);
        assert_eq!(clock(9000, 1.0, 1, 100, 30.0), (3332.0 * 30.0 / 1000.0, false));
        assert_eq!(clock(9000, 1.0, 0, 100, 30.0), (0.0, false));
        // The speed scales the time, whole milliseconds: 250 ms at 1.5 is 375 ms, frame 11.25.
        assert_eq!(clock(250, 1.5, 2, 100, 30.0), (11.25, false));
    }

    #[test]
    fn samples_tracks_and_holds_the_last_rotation() {
        let keys = [(0, [0.0, 0.0, 0.0]), (10, [10.0, 0.0, 0.0])];
        assert_eq!(sample(&keys, 2.5, lerp), Some([2.5, 0.0, 0.0]));
        assert_eq!((sample(&keys, -5.0, lerp), sample(&keys, 50.0, lerp)), (Some([0.0; 3]), Some([10.0, 0.0, 0.0])));
        let half_turn = [0.0, 1.0, 0.0, 0.0];
        let rotations = [(0, [0.0, 0.0, 0.0, 1.0]), (10, half_turn)];
        // Past its last key a rotation holds it.
        assert_eq!(sample(&rotations, 99.0, slerp), Some(half_turn));
        let middle = sample(&rotations, 5.0, slerp).unwrap();
        let expected = (std::f32::consts::FRAC_PI_4).sin();
        assert!((middle[1] - expected).abs() < 1e-5 && (middle[3] - expected).abs() < 1e-5, "{middle:?}");
        assert_eq!(sample::<f32>(&[], 1.0, |a, _, _| a), None);
        // UV tracks: shifts, scales and a turn about the centre.
        assert_eq!([0, 1, 2, 3].map(|kind| moved_uv([0.5, 0.25], kind, 2.0)), [[2.5, 0.25], [0.5, 2.25], [1.0, 0.25], [0.5, 0.5]]);
        let [u, v] = moved_uv([1.0, 0.5], 4, std::f32::consts::FRAC_PI_2);
        assert!((u - 0.5).abs() < 1e-5 && (v - 1.0).abs() < 1e-5);
    }

    #[test]
    fn poses_rsm_2_by_its_tracks() {
        let mut rsm = Rsm::parse(&rsm_2((2, 3))).unwrap();
        assert!(!rsm.animated());
        rsm.nodes[0].tracks.position = vec![(0, [0.0; 3]), (10, [10.0, 0.0, 0.0])];
        assert!(rsm.animated());
        // Halfway the node has moved 5 along x (y flipped as always).
        assert_eq!(rsm.triangles_at(5.0)[0].positions[0], [6.0, -2.0, 3.0]);
        assert_eq!(rsm.triangles_at(0.0)[0].positions[0], [1.0, -2.0, 3.0]);
        rsm.nodes[0].tracks.uv = vec![(0, 1, vec![(0, 0.0), (10, 1.0)])];
        assert_eq!(rsm.triangles_at(5.0)[0].uvs[1], [1.0, 0.5]);
    }

    #[test]
    fn reads_rsm_2_without_centring() {
        for version in [(2, 2), (2, 3)] {
            let rsm = Rsm::parse(&rsm_2(version)).unwrap();
            assert_eq!((rsm.version, rsm.textures.as_slice(), rsm.root.as_str()), (version, ["trap.bmp".to_owned()].as_slice(), "trap"));
            let triangles = rsm.triangles();
            // The file's origin stays put; y is flipped.
            assert_eq!(triangles.len(), 1);
            assert_eq!(triangles[0].positions, [[1.0, -2.0, 3.0], [4.0, -5.0, 6.0], [7.0, -8.0, 9.0]]);
            assert_eq!(triangles[0].uvs, [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]);
        }
        assert!(Rsm::parse(&rsm_2((2, 1))).is_err(), "2.1 isn't read");
    }

    /// Needs the official data.grf, at `RUSTNAROK_CLIENT_GRF` or `./data.grf`.
    #[test]
    fn reads_every_rsm_2() {
        let Some(grf) = Grf::for_tests() else { return };
        let files: Vec<String> = grf.paths().filter(|path| path.ends_with(".rsm2")).map(str::to_owned).collect();
        assert!(!files.is_empty());
        for file in files {
            let rsm = Rsm::parse(&grf.read(&file).unwrap()).unwrap_or_else(|error| panic!("{file}: {error}"));
            assert!(rsm.triangles().iter().all(|triangle| triangle.texture < rsm.textures.len()), "{file}");
        }
        let trap = Rsm::parse(&grf.read("data\\model\\effect\\flametrap\\fla_trap.rsm2").unwrap()).unwrap();
        assert!(!trap.triangles().is_empty());
    }

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
