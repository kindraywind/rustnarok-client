//! Dev tool: the textures a Granny model carries. `cargo run --example gr2_texture -- <data.grf> <path in grf> <output folder>` writes each as a PNG.

use rustnarok_client::gr2::Model;
use rustnarok_client::grf::Grf;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let [_, grf_path, inner, folder] = &args[..] else {
        panic!("usage: gr2_texture <data.grf> <path in grf> <output folder>");
    };
    let model = Model::parse(&Grf::open(grf_path).unwrap().read(inner).unwrap()).unwrap();
    std::fs::create_dir_all(folder).unwrap();
    for (index, (name, image)) in model.textures.iter().enumerate() {
        let path = format!("{folder}/{index}.png");
        image::RgbaImage::from_raw(image.width as u32, image.height as u32, image.rgba.clone()).unwrap().save(&path).unwrap();
        println!("{path}: {name} {}x{}", image.width, image.height);
    }
}
