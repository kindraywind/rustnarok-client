//! A file built by hand: one uncompressed section holding a type tree and one mesh of two triangles.

use super::*;

/// Lays out objects in the one section and records where its pointers go.
#[derive(Default)]
struct Section {
    data: Vec<u8>,
    pointers: Vec<(u32, u32)>,
}

impl Section {
    fn put(&mut self, bytes: &[u8]) -> u32 {
        while !self.data.len().is_multiple_of(4) {
            self.data.push(0);
        }
        self.data.extend(bytes);
        (self.data.len() - bytes.len()) as u32
    }

    fn words(&mut self, words: &[u32]) -> u32 {
        self.put(&words.iter().flat_map(|word| word.to_le_bytes()).collect::<Vec<u8>>())
    }

    fn string(&mut self, text: &str) -> u32 {
        self.put(&[text.as_bytes(), &[0]].concat())
    }

    /// Makes the word at `at` a pointer to `target`.
    fn point(&mut self, at: u32, target: u32) {
        self.pointers.push((at, target));
    }

    /// A struct type: (kind, name, referenced type, width) per member.
    fn type_of(&mut self, members: &[(u32, &str, Option<u32>, u32)]) -> u32 {
        let start = self.put(&vec![0; (members.len() + 1) * MEMBER]);
        for (index, (kind, name, reference, width)) in members.iter().enumerate() {
            let at = start + (index * MEMBER) as u32;
            self.data[at as usize..at as usize + 4].copy_from_slice(&kind.to_le_bytes());
            self.data[at as usize + 12..at as usize + 16].copy_from_slice(&width.to_le_bytes());
            let name = self.string(name);
            self.point(at + 4, name);
            if let Some(reference) = reference {
                self.point(at + 8, *reference);
            }
        }
        start
    }

    fn file(&self, root_type: u32, root: u32) -> Vec<u8> {
        let header = 0x80usize;
        let relocations = header + 44;
        let data_at = relocations + self.pointers.len() * RELOCATION;
        let mut file = vec![0u8; data_at];
        file[..16].copy_from_slice(&MAGIC);
        let put = |file: &mut Vec<u8>, at: usize, word: u32| file[at..at + 4].copy_from_slice(&word.to_le_bytes());
        put(&mut file, 16, header as u32);
        put(&mut file, 32, 6);
        put(&mut file, 44, (header - 32) as u32);
        put(&mut file, 48, 1);
        put(&mut file, 56, root_type);
        put(&mut file, 64, root);
        // The section: not compressed.
        put(&mut file, header + 4, data_at as u32);
        put(&mut file, header + 8, self.data.len() as u32);
        put(&mut file, header + 12, self.data.len() as u32);
        put(&mut file, header + 28, relocations as u32);
        put(&mut file, header + 32, self.pointers.len() as u32);
        for (index, (at, target)) in self.pointers.iter().enumerate() {
            put(&mut file, relocations + index * RELOCATION, *at);
            put(&mut file, relocations + index * RELOCATION + 8, *target);
        }
        file.extend(&self.data);
        file
    }
}

fn floats(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|value| value.to_le_bytes()).collect()
}

/// A mesh "m" of a square (4 vertices, 2 triangles) in the x/z plane, 3 high in z, textured by "C:\art\wall.tif".
fn square_file() -> Vec<u8> {
    let mut s = Section::default();
    // Types.
    let texture_type = s.type_of(&[(kind::STRING, "FromFileName", None, 0)]);
    let material_type = s.type_of(&[(kind::REFERENCE, "Texture", Some(texture_type), 0)]);
    let binding_type = s.type_of(&[(kind::REFERENCE, "Material", Some(material_type), 0)]);
    let group_type = s.type_of(&[(kind::INT32, "MaterialIndex", None, 0), (kind::INT32, "TriFirst", None, 0), (kind::INT32, "TriCount", None, 0)]);
    let index_type = s.type_of(&[(kind::INT32, "Int32", None, 0)]);
    let vertex_type = s.type_of(&[
        (kind::REAL32, "Position", None, 3),
        (kind::UINT8, "BoneIndices", None, 4),
        (kind::REAL32, "Normal", None, 3),
        (kind::REAL32, "TextureCoordinates0", None, 2),
    ]);
    let data_type = s.type_of(&[(kind::REFERENCE_TO_VARIANT_ARRAY, "Vertices", Some(0), 0)]);
    let topology_type = s.type_of(&[(kind::REFERENCE_TO_ARRAY, "Groups", Some(group_type), 0), (kind::REFERENCE_TO_ARRAY, "Indices", Some(index_type), 0)]);
    let mesh_type = s.type_of(&[
        (kind::STRING, "Name", None, 0),
        (kind::REFERENCE, "PrimaryVertexData", Some(data_type), 0),
        (kind::REFERENCE, "PrimaryTopology", Some(topology_type), 0),
        (kind::REFERENCE_TO_ARRAY, "MaterialBindings", Some(binding_type), 0),
    ]);
    let root_type = s.type_of(&[(kind::ARRAY_OF_REFERENCES, "Meshes", Some(mesh_type), 0)]);
    // Objects: the texture, its material, the vertices (32 bytes each: position, 4 bone bytes, normal, uv).
    let texture = s.words(&[0]);
    let name = s.string("C:\\art\\wall.tif");
    s.point(texture, name);
    let material = s.words(&[0]);
    s.point(material, texture);
    let binding = s.words(&[0]);
    s.point(binding, material);
    let corners = [([0.0, 0.0, 0.0], [0.0, 0.0]), ([2.0, 0.0, 0.0], [1.0, 0.0]), ([2.0, 0.0, 3.0], [1.0, 1.0]), ([0.0, 0.0, 3.0], [0.0, 1.0])];
    let mut vertices = Vec::new();
    for (position, uv) in corners {
        vertices.extend(floats(&position));
        vertices.extend([0u8; 4]);
        vertices.extend(floats(&[0.0, -1.0, 0.0]));
        vertices.extend(floats(&uv));
    }
    let vertices = s.put(&vertices);
    let data = s.words(&[0, 4, 0]);
    s.point(data, vertex_type);
    s.point(data + 8, vertices);
    let indices = s.words(&[0, 1, 2, 0, 2, 3]);
    let groups = s.words(&[0, 0, 2]);
    let topology = s.words(&[2, 0, 6, 0]);
    s.point(topology + 4, groups);
    s.point(topology + 12, indices);
    let mesh_name = s.string("m");
    let mesh = s.words(&[0, 0, 0, 1, 0]);
    s.point(mesh, mesh_name);
    s.point(mesh + 4, data);
    s.point(mesh + 8, topology);
    s.point(mesh + 16, binding);
    let list = s.words(&[0]);
    s.point(list, mesh);
    let root = s.words(&[1, 0]);
    s.point(root + 4, list);
    s.file(root_type, root)
}

#[test]
fn reads_the_mesh_of_a_built_file() {
    let model = Model::parse(&square_file()).unwrap();
    assert_eq!(model.meshes.len(), 1);
    let mesh = &model.meshes[0];
    assert_eq!(mesh.name, "m");
    assert_eq!(mesh.vertices.len(), 4);
    assert_eq!(mesh.vertices[2], Vertex { position: [2.0, 0.0, 3.0], normal: [0.0, -1.0, 0.0], uv: [1.0, 1.0] });
    assert_eq!(mesh.triangles, vec![[0, 1, 2], [0, 2, 3]]);
    assert_eq!(mesh.groups, vec![(0, 2, "C:\\art\\wall.tif".to_string())]);
    assert_eq!(model.bounds(), Some(([0.0, 0.0, 0.0], [2.0, 0.0, 3.0])));
}

#[test]
fn refuses_what_is_not_a_granny_file() {
    assert!(Gr2::parse(b"not a model file").is_err());
    let mut file = square_file();
    file[0] ^= 1;
    assert!(Gr2::parse(&file).is_err());
    // Cut off in the middle of the data.
    let file = square_file();
    assert!(Gr2::parse(&file[..file.len() - 40]).is_err() || Model::parse(&file[..file.len() - 40]).is_err());
}

#[test]
fn names_are_the_file_without_folders_or_extension() {
    assert_eq!(base_name("D:\\ChalesWORK_2003\\0124HG\\guardian copy.tif"), "guardian copy");
    assert_eq!(base_name("a/b/c.d.png"), "c.d");
    assert_eq!(base_name("plain"), "plain");
}

#[test]
fn reads_the_castle_models_of_the_data() {
    let Some(grf) = crate::grf::Grf::for_tests() else { return };
    for (name, meshes) in [("empelium90_0", 1), ("aguardian90_8", 6), ("kguardian90_7", 5), ("sguardian90_9", 4), ("guildflag90_1", 2)] {
        let model = Model::parse(&grf.read(&format!("data\\model\\3dmob\\{name}.gr2")).unwrap()).unwrap();
        assert_eq!(model.meshes.len(), meshes, "{name}");
        assert!(model.meshes.iter().all(|mesh| !mesh.triangles.is_empty()), "{name}");
    }
}
