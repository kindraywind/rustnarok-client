//! Dev tool: `cargo run --example messages -- <data.grf> <from> [to]` prints the client's messages in that ID range;
//! `... -- <data.grf> find <text>` prints every message containing the text.

use roclient::{grf, messages};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let grf = grf::Grf::open(&args[1]).unwrap();
    let messages = messages::load(&grf).unwrap();
    let show = |id: usize| println!("{id:5} {}", messages[id].replace('\n', "\\n"));
    if args[2] == "find" {
        (0..messages.len()).filter(|&id| messages[id].contains(&args[3])).for_each(show);
    } else {
        let from: usize = args[2].parse().unwrap();
        let to: usize = args.get(3).map_or(from, |to| to.parse().unwrap());
        (from..=to.min(messages.len() - 1)).for_each(show);
    }
}
