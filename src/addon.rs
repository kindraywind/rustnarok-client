//! A server's add-on: what a server's own DLL is to the official client. A server that needs its own client-side code
//! (a client check, say) ships a program in its client folder, at `rustnarok-addon/<os>-<arch>/addon` (`addon.exe` on
//! Windows), the OS and CPU as Rust names them: `macos-aarch64`, `macos-x86_64`, `windows-x86_64`, `linux-x86_64`. When
//! the data folder has one for this machine, the client starts it and they talk in lines of JSON:
//! - to its stdin, events. First `{"event": "hello", "protocol": 1, "client": "rustnarok-client", "version": "0.3.0",
//!   "os": "macos", "arch": "aarch64"}`; `protocol` goes up when a change would break an add-on, and new events or fields
//!   don't count, so an add-on skips what it doesn't know. Then `{"event": "npc_chat", "id": 110001, "text": "...",
//!   "color": [r, g, b]}` for each NPC's coloured chat line (0x02C1).
//! - from its stdout, actions: `{"cmd": "whisper", "to": "NPC:name", "text": "..."}`, a whisper sent as the player's
//!   (a script's hidden `NPC:<name>` runs its OnWhisperGlobal), not shown in the chat.
//!
//! Its stderr is the client's. It must exit when its stdin closes: the client has quit. It gets nothing else, no game
//! state and no input, so an add-on can't play the game. Like a server's DLL, it is that server's code running on the
//! player's machine; only the data folder the player chose can hold one.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::mpsc::{self, Sender};

use rustnarok_client::net::MapEvent;
use serde::Deserialize;
use serde_json::json;
use winit::event_loop::EventLoopProxy;

use crate::NetEvent;

/// The add-on protocol's version, in the hello line.
const PROTOCOL: u32 = 1;

/// Lines on their way to the add-on's stdin, while it runs.
static TO_ADDON: Mutex<Option<Sender<String>>> = Mutex::new(None);

#[derive(Debug, Deserialize, PartialEq)]
#[serde(tag = "cmd", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Action {
    Whisper { to: String, text: String },
}

/// Where the data folder `data` keeps the add-on for this machine.
fn program(data: &Path) -> PathBuf {
    let platform = format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH);
    data.join("rustnarok-addon").join(platform).join(format!("addon{}", std::env::consts::EXE_SUFFIX))
}

/// Starts the data folder's add-on, if it has one; its actions come back as `NetEvent::Addon`.
pub(crate) fn start(data: &Path, proxy: EventLoopProxy<NetEvent>) {
    let program = program(data);
    if !program.is_file() {
        return;
    }
    let mut child = match Command::new(&program).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn() {
        Ok(child) => child,
        Err(error) => {
            eprintln!("add-on {}: {error}", program.display());
            return;
        }
    };
    eprintln!("add-on started: {}", program.display());
    let (Some(mut stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else { return };
    // Its own writer, so an add-on that stops reading can't stall the game.
    // ponytail: unbounded; NPC chat is a few lines a minute.
    let (lines, queue) = mpsc::channel::<String>();
    let _ = lines.send(hello());
    *TO_ADDON.lock().unwrap() = Some(lines);
    std::thread::spawn(move || {
        for line in queue {
            if writeln!(stdin, "{line}").and_then(|()| stdin.flush()).is_err() {
                break;
            }
        }
    });
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            match serde_json::from_str::<Action>(&line) {
                Ok(action) => {
                    if proxy.send_event(NetEvent::Addon(action)).is_err() {
                        break;
                    }
                }
                Err(error) => eprintln!("add-on: {error}: {line}"),
            }
        }
        let _ = child.wait();
        eprintln!("add-on stopped");
        TO_ADDON.lock().unwrap().take();
    });
}

/// The first line an add-on gets: which protocol, client and machine it talks to.
fn hello() -> String {
    let (os, arch) = (std::env::consts::OS, std::env::consts::ARCH);
    json!({"event": "hello", "protocol": PROTOCOL, "client": "rustnarok-client", "version": env!("CARGO_PKG_VERSION"), "os": os, "arch": arch}).to_string()
}

/// The event line the add-on gets for `event`, if any.
fn event_line(event: &MapEvent) -> Option<String> {
    match event {
        MapEvent::Chat { id, message, color: Some(color) } => Some(json!({"event": "npc_chat", "id": id, "text": message, "color": color}).to_string()),
        _ => None,
    }
}

/// Passes the events add-ons get to the running one.
pub(crate) fn observe(event: &NetEvent) {
    let NetEvent::Map(event) = event else { return };
    if let (Some(line), Some(to_addon)) = (event_line(event), TO_ADDON.lock().unwrap().as_ref()) {
        let _ = to_addon.send(line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn talks_in_json_lines() {
        let hello: serde_json::Value = serde_json::from_str(&hello()).unwrap();
        assert_eq!((hello["event"].as_str(), hello["protocol"].as_u64(), hello["client"].as_str()), (Some("hello"), Some(1), Some("rustnarok-client")));
        assert!(hello["version"].is_string() && hello["os"].is_string() && hello["arch"].is_string());
        let npc = MapEvent::Chat { id: Some(2001257), message: "[check] 123456".into(), color: Some([181, 255, 181]) };
        let line: serde_json::Value = serde_json::from_str(&event_line(&npc).unwrap()).unwrap();
        assert_eq!(line, json!({"event": "npc_chat", "id": 2001257, "text": "[check] 123456", "color": [181, 255, 181]}));
        // Players' chat isn't an add-on's business.
        assert_eq!(event_line(&MapEvent::Chat { id: Some(150001), message: "Someone : hi".into(), color: None }), None);
        let whisper = serde_json::from_str::<Action>(r#"{"cmd":"whisper","to":"NPC:checker","text":"answer#1"}"#).unwrap();
        assert_eq!(whisper, Action::Whisper { to: "NPC:checker".into(), text: "answer#1".into() });
        assert!(serde_json::from_str::<Action>(r#"{"cmd":"say","text":"@item 501"}"#).is_err());
        assert!(program(Path::new("client")).starts_with("client/rustnarok-addon"));
    }
}
