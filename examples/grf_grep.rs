//! Dev tool: `cargo run --example grf_grep -- <data.grf> <bytes> [path filter]` lists files whose contents contain the
//! text (as raw bytes).

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let grf = roclient::grf::Grf::open(&args[1]).unwrap();
    let needle = args[2].as_bytes();
    let filter = args.get(3).map(|filter| filter.to_lowercase());
    for path in grf.paths().filter(|path| filter.as_ref().is_none_or(|filter| path.contains(filter.as_str()))) {
        if let Ok(bytes) = grf.read(path)
            && bytes.windows(needle.len()).any(|window| window == needle)
        {
            println!("{path}");
        }
    }
}
