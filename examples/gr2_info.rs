//! Dev tool: what a Granny model holds. `cargo run --example gr2_info -- <data.grf> <path in grf, e.g. data\model\3dmob\empelium90_0.gr2>`

use rustnarok_client::gr2::{Gr2, Model};
use rustnarok_client::grf::Grf;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let [_, grf_path, inner] = &args[..] else {
        panic!("usage: gr2_info <data.grf> <path in grf>");
    };
    let bytes = Grf::open(grf_path).unwrap().read(inner).unwrap();
    let model = Model::parse(&bytes).unwrap();
    let file = Gr2::parse(&bytes).unwrap();
    let root = file.root().unwrap();
    println!("file info: {:?}", root.members());
    for material in root.array("Materials") {
        println!("material {:?}: {:?}", material.string("Name"), material.members());
        for map in material.array("Maps") {
            println!("  map {:?}: {:?}", map.string("Usage"), map.members());
        }
    }
    for texture in root.array("Textures") {
        println!("texture {:?}: {:?}", texture.string("FromFileName"), texture.members());
        println!(
            "  {}x{} type {:?} encoding {:?} subformat {:?}",
            texture.int("Width").unwrap_or(0),
            texture.int("Height").unwrap_or(0),
            texture.int("TextureType"),
            texture.int("Encoding"),
            texture.int("SubFormat")
        );
        for image in texture.array("Images") {
            println!("  image: {:?} mips {:?} pixels {:?}", image.members(), image.int("MIPLevels"), image.array("MIPLevels").len());
        }
    }
    println!("{} meshes, {} units per meter, axes (right, up, back) {:?}", model.meshes.len(), model.units_per_meter, model.axes);
    println!("bounds {:?}", model.bounds());
    for mesh in &model.meshes {
        println!("mesh {:?}: {} vertices, {} triangles", mesh.name, mesh.vertices.len(), mesh.triangles.len());
        for (first, count, texture) in &mesh.groups {
            println!("  triangles {first}..{}: texture {texture:?}", first + count);
        }
    }
}
