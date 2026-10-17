//! Dev tool: `cargo run --example grf_list -- <data.grf> <text>` lists the files whose path contains `text`.

use roclient::grf;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let [_, grf_path, text] = &args[..] else {
        panic!("usage: grf_list <data.grf> <text>");
    };
    let grf = grf::Grf::open(grf_path).unwrap();
    let text = text.replace('/', "\\").to_lowercase();
    let mut paths: Vec<&str> = grf.paths().filter(|path| path.contains(&text)).collect();
    paths.sort_unstable();
    for path in paths {
        println!("{path}");
    }
}
