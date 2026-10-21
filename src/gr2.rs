//! Granny 2 model files (`.gr2`, version 6, 32-bit little endian): the container with its compressed sections and
//! relocations, the self-describing type tree, and the meshes of a model (positions, normals, texture coordinates,
//! triangles and the texture each uses). Animations, skeletons and morph targets are not read: a mesh is its bind pose.

mod bink;
mod oodle0;

use std::collections::HashMap;
use std::io;

pub use bink::Image;
pub use oodle0::decompress;

const MAGIC: [u8; 16] = [0xb8, 0x67, 0xb0, 0xca, 0xf8, 0x6d, 0xb1, 0x0f, 0x84, 0x72, 0x8c, 0x7e, 0x5e, 0x19, 0x00, 0x1e];
/// The size of a section header and of a relocation in the file, and of a member definition in the type tree.
const SECTION_HEADER: usize = 44;
const RELOCATION: usize = 12;
const MEMBER: usize = 32;

fn bad(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("gr2: {what}"))
}

/// A place in the file's data: a section and an offset in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Ptr {
    section: u32,
    offset: u32,
}

impl Ptr {
    fn plus(self, bytes: usize) -> Self {
        Self { section: self.section, offset: self.offset + bytes as u32 }
    }
}

/// The member types of the type tree (granny_member_type).
mod kind {
    pub const END: u32 = 0;
    pub const INLINE: u32 = 1;
    pub const REFERENCE: u32 = 2;
    pub const REFERENCE_TO_ARRAY: u32 = 3;
    pub const ARRAY_OF_REFERENCES: u32 = 4;
    pub const VARIANT_REFERENCE: u32 = 5;
    pub const REFERENCE_TO_VARIANT_ARRAY: u32 = 7;
    pub const STRING: u32 = 8;
    pub const TRANSFORM: u32 = 9;
    pub const REAL32: u32 = 10;
    pub const INT8: u32 = 11;
    pub const UINT8: u32 = 12;
    pub const BINORMAL_INT8: u32 = 13;
    pub const NORMAL_UINT8: u32 = 14;
    pub const INT16: u32 = 15;
    pub const UINT16: u32 = 16;
    pub const BINORMAL_INT16: u32 = 17;
    pub const NORMAL_UINT16: u32 = 18;
    pub const INT32: u32 = 19;
    pub const UINT32: u32 = 20;
    pub const REAL16: u32 = 21;
    pub const EMPTY_REFERENCE: u32 = 22;
}

/// One member of a struct in the type tree.
#[derive(Clone, Debug)]
pub struct Member {
    pub kind: u32,
    pub name: String,
    /// The struct type of an inline, a reference or an array of them.
    reference: Option<Ptr>,
    /// How many of a scalar (0: one).
    pub width: u32,
    /// Where it starts in the struct, and how many bytes it takes.
    pub offset: usize,
    pub size: usize,
}

/// A parsed file: its sections unpacked, its pointers resolved.
pub struct Gr2 {
    sections: Vec<Vec<u8>>,
    pointers: HashMap<Ptr, Ptr>,
    root_type: Ptr,
    root: Ptr,
}

/// Names the type tree's element types bring.
pub struct Struct {
    members: Vec<Member>,
    pub size: usize,
}

impl Struct {
    pub fn member(&self, name: &str) -> Option<&Member> {
        self.members.iter().find(|member| member.name == name)
    }
}

/// An object in a file: where it is and the struct that describes it.
#[derive(Clone)]
pub struct Object<'a> {
    file: &'a Gr2,
    at: Ptr,
    layout: std::rc::Rc<Struct>,
}

/// One vertex of a mesh.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

/// A mesh in its bind pose: the triangles are index triples, each group using one texture.
#[derive(Clone, Debug, Default)]
pub struct Mesh {
    pub name: String,
    pub vertices: Vec<Vertex>,
    pub triangles: Vec<[u32; 3]>,
    /// The groups of triangles that share a material: (first triangle, triangle count, texture file name or empty).
    pub groups: Vec<(usize, usize, String)>,
}

/// A model: its meshes and the axes of the tool that made it.
#[derive(Clone, Debug, Default)]
pub struct Model {
    pub meshes: Vec<Mesh>,
    /// The textures the file carries, by the file name the exporter wrote (what `Mesh::groups` name), decoded.
    pub textures: Vec<(String, Image)>,
    /// The meter in the model's units, and the file's up, right and back axes (as the exporter wrote them).
    pub units_per_meter: f32,
    pub axes: [[f32; 3]; 3],
}

impl Gr2 {
    pub fn parse(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() < 0x80 || bytes[..16] != MAGIC {
            return Err(bad("not a granny file"));
        }
        let u32_at =
            |at: usize| -> io::Result<u32> { bytes.get(at..at + 4).map(|word| u32::from_le_bytes(word.try_into().unwrap())).ok_or_else(|| bad("truncated")) };
        // The header: its size, the format (0: plain), two reserved words; then the file info.
        if u32_at(20)? != 0 {
            return Err(bad("unsupported header format"));
        }
        let version = u32_at(32)?;
        if version != 6 {
            return Err(bad("unsupported version"));
        }
        let (sections_at, sections) = (32 + u32_at(44)? as usize, u32_at(48)? as usize);
        let root_type = Ptr { section: u32_at(52)?, offset: u32_at(56)? };
        let root = Ptr { section: u32_at(60)?, offset: u32_at(64)? };
        if sections == 0 || sections > 32 {
            return Err(bad("sections"));
        }
        let mut data = Vec::with_capacity(sections);
        let mut relocations = Vec::new();
        for index in 0..sections {
            let header = sections_at + index * SECTION_HEADER;
            let field = |n: usize| u32_at(header + n * 4);
            let (compression, offset, size, expanded) = (field(0)?, field(1)? as usize, field(2)? as usize, field(3)? as usize);
            let (first_16, first_8) = (field(5)? as usize, field(6)? as usize);
            let (reloc_at, reloc_count) = (field(7)? as usize, field(8)? as usize);
            let packed = bytes.get(offset..offset + size).ok_or_else(|| bad("section outside the file"))?;
            data.push(match compression {
                0 => packed.to_vec(),
                1 if expanded == 0 => Vec::new(),
                1 => decompress(packed, [first_16, first_8, expanded])?,
                _ => return Err(bad("unsupported compression")),
            });
            for entry in 0..reloc_count {
                let at = reloc_at + entry * RELOCATION;
                relocations.push((index as u32, u32_at(at)?, Ptr { section: u32_at(at + 4)?, offset: u32_at(at + 8)? }));
            }
        }
        let mut pointers = HashMap::new();
        for (section, offset, target) in relocations {
            if target.section as usize >= data.len() || offset as usize + 4 > data[section as usize].len() {
                return Err(bad("relocation outside its section"));
            }
            pointers.insert(Ptr { section, offset }, target);
        }
        Ok(Self { sections: data, pointers, root_type, root })
    }

    fn u32_at(&self, at: Ptr) -> Option<u32> {
        let bytes = self.sections.get(at.section as usize)?.get(at.offset as usize..at.offset as usize + 4)?;
        Some(u32::from_le_bytes(bytes.try_into().unwrap()))
    }

    fn f32_at(&self, at: Ptr) -> Option<f32> {
        self.u32_at(at).map(f32::from_bits)
    }

    /// The pointer stored at `at`, if it holds one.
    fn pointer_at(&self, at: Ptr) -> Option<Ptr> {
        self.pointers.get(&at).copied()
    }

    fn string_at(&self, at: Ptr) -> Option<String> {
        let target = self.pointer_at(at)?;
        let section = self.sections.get(target.section as usize)?;
        let tail = section.get(target.offset as usize..)?;
        let end = tail.iter().position(|&byte| byte == 0)?;
        Some(String::from_utf8_lossy(&tail[..end]).into_owned())
    }

    /// The members of a struct type: the definitions up to the end marker, with their offsets.
    fn read_struct(&self, at: Ptr) -> Option<Struct> {
        let mut members = Vec::new();
        let mut offset = 0;
        for index in 0..256 {
            let member = at.plus(index * MEMBER);
            let kind = self.u32_at(member)?;
            if kind == kind::END {
                return Some(Struct { members, size: offset });
            }
            let reference = self.pointer_at(member.plus(8));
            let width = self.u32_at(member.plus(12))?;
            let size = self.member_size(kind, reference, width)?;
            members.push(Member { kind, name: self.string_at(member.plus(4)).unwrap_or_default(), reference, width, offset, size });
            offset += size;
        }
        None
    }

    fn member_size(&self, kind: u32, reference: Option<Ptr>, width: u32) -> Option<usize> {
        let count = width.max(1) as usize;
        Some(match kind {
            kind::INLINE => self.read_struct(reference?)?.size * count,
            kind::REFERENCE | kind::STRING | kind::EMPTY_REFERENCE => 4,
            kind::REFERENCE_TO_ARRAY | kind::ARRAY_OF_REFERENCES | kind::VARIANT_REFERENCE => 8,
            kind::REFERENCE_TO_VARIANT_ARRAY => 12,
            kind::TRANSFORM => 68 * count,
            kind::REAL32 | kind::INT32 | kind::UINT32 => 4 * count,
            kind::INT8 | kind::UINT8 | kind::BINORMAL_INT8 | kind::NORMAL_UINT8 => count,
            kind::INT16 | kind::UINT16 | kind::BINORMAL_INT16 | kind::NORMAL_UINT16 | kind::REAL16 => 2 * count,
            _ => return None,
        })
    }

    /// The file info at the root of the object tree.
    pub fn root(&self) -> Option<Object<'_>> {
        let layout = std::rc::Rc::new(self.read_struct(self.root_type)?);
        Some(Object { file: self, at: self.root, layout })
    }
}

impl<'a> Object<'a> {
    fn at(&self, member: &str) -> Option<(&Member, Ptr)> {
        let member = self.layout.member(member)?;
        Some((member, self.at.plus(member.offset)))
    }

    /// The names of the members of this object's struct, with their type numbers.
    pub fn members(&self) -> Vec<(String, u32)> {
        self.layout.members.iter().map(|member| (member.name.clone(), member.kind)).collect()
    }

    /// The object a reference member points to.
    pub fn reference(&self, name: &str) -> Option<Object<'a>> {
        let (member, at) = self.at(name)?;
        if member.kind != kind::REFERENCE {
            return None;
        }
        let layout = std::rc::Rc::new(self.file.read_struct(member.reference?)?);
        Some(Object { file: self.file, at: self.file.pointer_at(at)?, layout })
    }

    pub fn string(&self, name: &str) -> Option<String> {
        let (member, at) = self.at(name)?;
        (member.kind == kind::STRING).then(|| self.file.string_at(at)).flatten()
    }

    /// The elements of an array member: objects for a reference to an array of structs, an array of references or a
    /// plain array of inline structs.
    pub fn array(&self, name: &str) -> Vec<Object<'a>> {
        self.try_array(name).unwrap_or_default()
    }

    fn try_array(&self, name: &str) -> Option<Vec<Object<'a>>> {
        let (member, at) = self.at(name)?;
        let count = self.file.u32_at(at)? as usize;
        if count > 1 << 20 {
            return None;
        }
        let first = self.file.pointer_at(at.plus(4));
        let layout = std::rc::Rc::new(self.file.read_struct(member.reference?)?);
        let mut objects = Vec::with_capacity(count);
        match member.kind {
            kind::REFERENCE_TO_ARRAY => {
                for index in 0..count {
                    objects.push(Object { file: self.file, at: first?.plus(index * layout.size), layout: layout.clone() });
                }
            }
            kind::ARRAY_OF_REFERENCES => {
                for index in 0..count {
                    objects.push(Object { file: self.file, at: self.file.pointer_at(first?.plus(index * 4))?, layout: layout.clone() });
                }
            }
            _ => return None,
        }
        Some(objects)
    }

    /// An int32 member (or an int member narrower than that).
    pub fn int(&self, name: &str) -> Option<i32> {
        let (member, at) = self.at(name)?;
        match member.kind {
            kind::INT32 | kind::UINT32 => self.file.u32_at(at).map(|value| value as i32),
            _ => None,
        }
    }

    pub fn real(&self, name: &str) -> Option<f32> {
        let (member, at) = self.at(name)?;
        (member.kind == kind::REAL32).then(|| self.file.f32_at(at)).flatten()
    }

    pub fn reals<const N: usize>(&self, name: &str) -> Option<[f32; N]> {
        let (member, at) = self.at(name)?;
        if member.kind != kind::REAL32 || (member.width as usize) < N {
            return None;
        }
        let mut out = [0.0; N];
        for (index, slot) in out.iter_mut().enumerate() {
            *slot = self.file.f32_at(at.plus(index * 4))?;
        }
        Some(out)
    }

    /// The vertices of a variant array member (`Vertices`): the layout's members by name, and the data to read them from.
    fn variant_array(&self, name: &str) -> Option<(Struct, usize, Ptr)> {
        let (member, at) = self.at(name)?;
        if member.kind != kind::REFERENCE_TO_VARIANT_ARRAY {
            return None;
        }
        let layout = self.file.read_struct(self.file.pointer_at(at)?)?;
        let count = self.file.u32_at(at.plus(4))? as usize;
        Some((layout, count, self.file.pointer_at(at.plus(8))?))
    }

    /// The bytes of an array member of bytes (`Pixels`).
    fn bytes(&self, name: &str) -> Option<&'a [u8]> {
        let (member, at) = self.at(name)?;
        if member.kind != kind::REFERENCE_TO_ARRAY {
            return None;
        }
        let count = self.file.u32_at(at)? as usize;
        let first = self.file.pointer_at(at.plus(4))?;
        self.file.sections.get(first.section as usize)?.get(first.offset as usize..(first.offset as usize).checked_add(count)?)
    }

    /// An int array member (`Indices`) as numbers; the elements are int32 or int16 by the member's type.
    fn numbers(&self, name: &str) -> Option<Vec<u32>> {
        let (member, at) = self.at(name)?;
        if member.kind != kind::REFERENCE_TO_ARRAY {
            return None;
        }
        let count = self.file.u32_at(at)? as usize;
        if count == 0 {
            return Some(Vec::new());
        }
        let first = self.file.pointer_at(at.plus(4))?;
        let width = match self.file.read_struct(member.reference?).as_ref().and_then(|layout| layout.members.first()).map(|first| first.kind) {
            Some(kind::INT16 | kind::UINT16) => 2,
            _ => 4,
        };
        let data = self.file.sections.get(first.section as usize)?;
        let bytes = data.get(first.offset as usize..first.offset as usize + count * width)?;
        Some(
            bytes
                .chunks_exact(width)
                .map(|chunk| if width == 2 { u32::from(u16::from_le_bytes([chunk[0], chunk[1]])) } else { u32::from_le_bytes(chunk.try_into().unwrap()) })
                .collect(),
        )
    }
}

/// The base name (no folders, no extension) of a file name an exporter wrote, whichever slash it used.
pub fn base_name(path: &str) -> &str {
    let name = path.rsplit(['\\', '/']).next().unwrap_or(path);
    name.rsplit_once('.').map_or(name, |(stem, _)| stem)
}

impl Model {
    /// The meshes of `bytes`, a `.gr2` file.
    pub fn parse(bytes: &[u8]) -> io::Result<Self> {
        let file = Gr2::parse(bytes)?;
        let root = file.root().ok_or_else(|| bad("no file info"))?;
        let mut model = Model { units_per_meter: 1.0, ..Self::default() };
        if let Some(art) = root.reference("ArtToolInfo") {
            model.units_per_meter = art.real("UnitsPerMeter").unwrap_or(1.0);
            model.axes = [
                art.reals("RightVector").unwrap_or([1.0, 0.0, 0.0]),
                art.reals("UpVector").unwrap_or([0.0, 1.0, 0.0]),
                art.reals("BackVector").unwrap_or([0.0, 0.0, 1.0]),
            ];
        }
        for mesh in root.array("Meshes") {
            if let Some(read) = read_mesh(&mesh) {
                model.meshes.push(read);
            }
        }
        for texture in root.array("Textures") {
            let (Some(name), Some(width), Some(height)) = (texture.string("FromFileName"), texture.int("Width"), texture.int("Height")) else { continue };
            if texture.int("Encoding") != Some(3) {
                continue;
            }
            let pixels = texture.array("Images").first().and_then(|image| image.array("MIPLevels").first().and_then(|level| level.bytes("Pixels")));
            if let Some(image) = pixels.and_then(|pixels| bink::decode(pixels, width.max(0) as usize, height.max(0) as usize).ok()) {
                model.textures.push((name, image));
            }
        }
        if model.meshes.is_empty() {
            return Err(bad("no meshes"));
        }
        Ok(model)
    }

    /// The box all the vertices fit in: smallest corner, then largest.
    pub fn bounds(&self) -> Option<([f32; 3], [f32; 3])> {
        let mut vertices = self.meshes.iter().flat_map(|mesh| mesh.vertices.iter());
        let first = vertices.next()?.position;
        Some(vertices.fold((first, first), |(low, high), vertex| {
            (std::array::from_fn(|axis| low[axis].min(vertex.position[axis])), std::array::from_fn(|axis| high[axis].max(vertex.position[axis])))
        }))
    }
}

/// The file name of the texture a material uses: its own, or the first of its maps'.
fn texture_name(material: &Object) -> Option<String> {
    if let Some(name) = material.reference("Texture").and_then(|texture| texture.string("FromFileName")) {
        return Some(name);
    }
    material.array("Maps").iter().find_map(|map| map.reference("Map").and_then(|inner| texture_name(&inner)))
}

fn read_mesh(mesh: &Object) -> Option<Mesh> {
    let data = mesh.reference("PrimaryVertexData")?;
    let (layout, count, first) = data.variant_array("Vertices")?;
    let file = mesh.file;
    let section = file.sections.get(first.section as usize)?;
    let stride = layout.size;
    let bytes = section.get(first.offset as usize..first.offset as usize + count * stride)?;
    let offset_of = |name: &str| layout.member(name).filter(|member| member.kind == kind::REAL32).map(|member| member.offset);
    let (position, normal, uv) = (offset_of("Position")?, offset_of("Normal"), offset_of("TextureCoordinates0"));
    let float = |vertex: &[u8], at: usize| f32::from_le_bytes(vertex[at..at + 4].try_into().unwrap());
    let vertices: Vec<Vertex> = bytes
        .chunks_exact(stride)
        .map(|vertex| Vertex {
            position: std::array::from_fn(|axis| float(vertex, position + 4 * axis)),
            normal: normal.map_or([0.0, 1.0, 0.0], |at| std::array::from_fn(|axis| float(vertex, at + 4 * axis))),
            uv: uv.map_or([0.0; 2], |at| std::array::from_fn(|axis| float(vertex, at + 4 * axis))),
        })
        .collect();
    let topology = mesh.reference("PrimaryTopology")?;
    let indices = topology.numbers("Indices").filter(|indices| !indices.is_empty()).or_else(|| topology.numbers("Indices16"))?;
    if indices.len() % 3 != 0 || indices.iter().any(|&index| index as usize >= vertices.len()) {
        return None;
    }
    let triangles: Vec<[u32; 3]> = indices.chunks_exact(3).map(|corner| [corner[0], corner[1], corner[2]]).collect();
    let materials: Vec<String> = mesh
        .array("MaterialBindings")
        .iter()
        .map(|binding| binding.reference("Material").and_then(|material| texture_name(&material)).unwrap_or_default())
        .collect();
    let mut groups: Vec<(usize, usize, String)> = topology
        .array("Groups")
        .iter()
        .filter_map(|group| {
            let (first, count) = (usize::try_from(group.int("TriFirst")?).ok()?, usize::try_from(group.int("TriCount")?).ok()?);
            let material = usize::try_from(group.int("MaterialIndex")?).ok()?;
            (first + count <= triangles.len()).then(|| (first, count, materials.get(material).cloned().unwrap_or_default()))
        })
        .collect();
    if groups.is_empty() {
        groups.push((0, triangles.len(), materials.first().cloned().unwrap_or_default()));
    }
    Some(Mesh { name: mesh.string("Name").unwrap_or_default(), vertices, triangles, groups })
}

#[cfg(test)]
mod tests;
