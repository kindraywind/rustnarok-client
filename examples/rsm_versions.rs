//! Dev tool: RSM versions used by a map's models. `cargo run --example rsm_versions -- <data.grf> <map>`

use std::collections::BTreeMap;

use roclient::grf::Grf;
use roclient::map::Rsw;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let grf = Grf::open(&args[1]).unwrap();
    let rsw = Rsw::parse(&grf.read(&format!("data\\{}.rsw", args[2])).unwrap()).unwrap();
    let mut versions: BTreeMap<(u8, u8), usize> = BTreeMap::new();
    let mut files: Vec<&str> = rsw.models.iter().map(|model| model.file.as_str()).collect();
    files.sort();
    files.dedup();
    for file in &files {
        match grf.read(&format!("data\\model\\{file}")) {
            Ok(bytes) => *versions.entry((bytes[4], bytes[5])).or_default() += 1,
            Err(error) => println!("missing {file}: {error}"),
        }
    }
    println!("{} model instances, {} files, versions {versions:?}", rsw.models.len(), files.len());
}
