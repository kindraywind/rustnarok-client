//! Dev tool: `cargo run --example str_dump -- <data.grf> <effect name>` lists an STR effect's layers and keyframes.

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let grf = roclient::grf::Grf::open(&args[1]).unwrap();
    let bytes = grf.read(&format!("data\\texture\\effect\\{}.str", args[2])).unwrap();
    let effect = roclient::effect::Str::parse(&bytes).unwrap();
    println!("{} fps, {} frames, {} layers", effect.fps, effect.frames, effect.layers.len());
    for (index, layer) in effect.layers.iter().enumerate() {
        println!("layer {index}: {:?}", layer.textures);
        for key in &layer.keys {
            println!(
                "  frame {:3} kind {} pos {:?} corners {:?} tex {} angle {:.1} color {:?} blend {}/{}",
                key.frame,
                key.kind,
                key.position,
                key.corners,
                key.texture,
                key.angle,
                key.color.map(|c| (c * 255.0).round()),
                key.source_blend,
                key.destination_blend
            );
        }
    }
}
