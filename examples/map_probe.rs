//! Dev tool: logs in, plays a character and prints what the map server sends.
//! `ROCLIENT_TEST_ACCOUNT=user:password cargo run --example map_probe -- [slot] [seconds] [walk x,y]`

use std::sync::mpsc;
use std::time::{Duration, Instant};

use roclient::net::{self, CharEvent, CharSession, MapEvent, MapSession, MapTicket};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let slot: u8 = args.get(1).map_or(1, |slot| slot.parse().unwrap());
    let seconds: u64 = args.get(2).map_or(3, |seconds| seconds.parse().unwrap());
    let walk: Option<[u16; 2]> = args.get(3).map(|cell| {
        let (x, y) = cell.split_once(',').unwrap();
        [x.parse().unwrap(), y.parse().unwrap()]
    });
    let account = std::env::var("ROCLIENT_TEST_ACCOUNT").expect("ROCLIENT_TEST_ACCOUNT=user:password");
    let (user, password) = account.split_once(':').unwrap();

    let accepted = net::login("127.0.0.1:6900", user, password).unwrap();
    let (sender, receiver) = mpsc::channel();
    let mut session = CharSession::connect(&accepted.char_servers[0], &accepted, move |event| {
        let _ = sender.send(event);
    })
    .unwrap();
    let (character_id, map, address) = loop {
        match receiver.recv_timeout(Duration::from_secs(10)).unwrap() {
            CharEvent::Characters { .. } => session.select(slot).unwrap(),
            CharEvent::ZoneServer { character_id, map, ip, port } => break (character_id, map, (ip, port)),
            other => panic!("{other:?}"),
        }
    };
    drop(session);
    println!("map {map} at {address:?}");

    let ticket = MapTicket { address, account_id: accepted.account_id, character_id, login_id1: accepted.login_id1, sex: accepted.sex };
    let (sender, receiver) = mpsc::channel();
    let mut map = MapSession::connect(&ticket, 0, move |event| {
        let _ = sender.send(event);
    })
    .unwrap();
    let start = Instant::now();
    let mut walk = walk;
    while start.elapsed() < Duration::from_secs(seconds) {
        // Walk a second in, once everything in view has arrived.
        if start.elapsed() > Duration::from_secs(1)
            && let Some(cell) = walk.take()
        {
            println!("walk to {cell:?}");
            map.walk(cell).unwrap();
        }
        let Ok(event) = receiver.recv_timeout(Duration::from_millis(100)) else { continue };
        if matches!(event, MapEvent::Entered { .. }) {
            map.loaded().unwrap();
        }
        println!("{event:?}");
    }
}
