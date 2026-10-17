//! Dev tool: `cargo run --example grf_extract -- <data.grf> <path in grf> <output file>`.

use roclient::grf;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let [_, grf_path, inner, output] = &args[..] else {
        panic!("usage: grf_extract <data.grf> <path in grf> <output file>");
    };
    let data = grf::Grf::open(grf_path).unwrap().read(inner).unwrap();
    std::fs::write(output, data).unwrap();
}
