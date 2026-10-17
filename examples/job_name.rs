//! Dev tool: sprite name and files for job IDs. `cargo run --example job_name -- <data.grf> <id>...`

use roclient::grf::Grf;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let grf = Grf::open(&args[1]).unwrap();
    let names = roclient::lua::job_names(&grf).unwrap();
    for id in &args[2..] {
        let name = names.get(&id.parse().unwrap());
        let found: Vec<String> = ["npc", "몬스터"]
            .iter()
            .filter(|folder| name.is_some_and(|name| grf.read(&format!("data\\sprite\\{folder}\\{name}.spr")).is_ok()))
            .map(|folder| folder.to_string())
            .collect();
        println!("{id}: {name:?} found in {found:?}");
    }
}
