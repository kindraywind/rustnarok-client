//! Dev tool: a mesh's vertices by height with their texture coordinates. `cargo run --example gr2_uv -- <data.grf> <path in grf> <mesh name>`

use rustnarok_client::gr2::Model;
use rustnarok_client::grf::Grf;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let [_, grf_path, inner, name] = &args[..] else {
        panic!("usage: gr2_uv <data.grf> <path in grf> <mesh name>");
    };
    let model = Model::parse(&Grf::open(grf_path).unwrap().read(inner).unwrap()).unwrap();
    let mesh = model.meshes.iter().find(|mesh| &mesh.name == name).unwrap();
    let mut vertices = mesh.vertices.clone();
    vertices.sort_by(|a, b| a.position[2].total_cmp(&b.position[2]));
    for vertex in vertices.iter().step_by((vertices.len() / 40).max(1)) {
        println!("pos {:7.2} {:7.2} {:7.2}  uv {:.3} {:.3}", vertex.position[0], vertex.position[1], vertex.position[2], vertex.uv[0], vertex.uv[1]);
    }
}
