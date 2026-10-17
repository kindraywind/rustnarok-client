//! Dev tool: `cargo run --example act_sounds -- <data.grf> <act path>...` prints each ACT's sounds and which action
//! frames (action number, frame) play them.

use roclient::grf::Grf;
use roclient::sprite::Act;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (grf_path, acts) = args.split_first().expect("usage: act_sounds <data.grf> <act path>...");
    let grf = Grf::open(grf_path).unwrap();
    for path in acts {
        let act = Act::parse(&grf.read(path).unwrap()).unwrap();
        let played: Vec<(usize, usize, &str)> = act
            .actions
            .iter()
            .enumerate()
            .flat_map(|(action, frames)| frames.iter().enumerate().map(move |(frame, at)| (action, frame, at.event)))
            .filter_map(|(action, frame, event)| Some((action, frame, act.events.get(usize::try_from(event).ok()?)?.as_str())))
            .collect();
        println!("{path}: {:?}\n  {played:?}", act.events);
    }
}
