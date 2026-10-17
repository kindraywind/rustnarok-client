//! Dev tool: a map's RSW water and light settings, and the walkable cell nearest its center.
//! `cargo run --example map_info -- <data.grf> <map>`

use roclient::grf::Grf;
use roclient::map::{Gat, Rsw};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let grf = Grf::open(&args[1]).unwrap();
    let rsw = Rsw::parse(&grf.read(&format!("data\\{}.rsw", args[2])).unwrap()).unwrap();
    println!("water {:?}\nlight {:?}", rsw.water, rsw.light);
    let gat = Gat::parse(&grf.read(&format!("data\\{}", rsw.altitude_file)).unwrap()).unwrap();
    let center = [gat.width as i32 / 2, gat.height as i32 / 2];
    let nearest = (0..gat.height as i32)
        .flat_map(|y| (0..gat.width as i32).map(move |x| [x, y]))
        .filter(|&cell| gat.walkable(cell))
        .min_by_key(|cell| (cell[0] - center[0]).pow(2) + (cell[1] - center[1]).pow(2));
    println!("size {}x{}, walkable near center {nearest:?}", gat.width, gat.height);
}
