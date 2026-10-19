//! A client shared with an agent: `RUSTNAROK_CLIENT_CONTROL_SHARED` together with the control socket (control.rs) lets
//! the player play the same client. The real mouse and keyboard work, the window is drawn and kept like a player's,
//! settings are saved, and the chat has an MCP tab that lists what the agent does.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::chat::Kind;
use crate::control::{Button, Command};
use crate::{Game, Scene};

/// The colours of the MCP tab's lines: what the agent does, what it answers, what the player typed.
const MCP_LINE: [u8; 3] = [200, 170, 255];
const REPLY_LINE: [u8; 3] = [255, 255, 255];
const TYPED_LINE: [u8; 3] = [150, 255, 255];

/// What the player typed in the MCP tab and the agent hasn't read yet.
static INBOX: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());

/// Whether `RUSTNAROK_CLIENT_CONTROL_SHARED` is set.
pub fn shared() -> bool {
    static SHARED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *SHARED.get_or_init(|| std::env::var_os("RUSTNAROK_CLIENT_CONTROL_SHARED").is_some())
}

/// Whether a test driver has the client to itself: the socket is named (before it opens) and the player isn't sharing it.
pub fn scripted() -> bool {
    std::env::var_os("RUSTNAROK_CLIENT_CONTROL").is_some() && !shared()
}

/// What a command does, for the chat's MCP tab; nothing for those that only look.
fn describe(command: &Command) -> Option<String> {
    let at = |x: &f32, y: &f32| format!("({}, {})", x.round(), y.round());
    Some(match command {
        Command::Move { x, y } => format!("move {}", at(x, y)),
        Command::Click { x, y, button, mods, count } => {
            let side = if *button == Button::Right { " right" } else { "" };
            let times = count.filter(|&count| count > 1).map_or(String::new(), |count| format!(" x{count}"));
            let mods = if mods.is_empty() { String::new() } else { format!(" +{mods}") };
            format!("click {}{side}{times}{mods}", at(x, y))
        }
        Command::Drag { from, to, .. } => format!("drag {} -> {}", at(&from[0], &from[1]), at(&to[0], &to[1])),
        Command::Wheel { x, y, rows } => format!("wheel {} {rows}", at(x, y)),
        Command::Key { key, .. } => format!("key {key}"),
        Command::Type { text } => format!("type {text}"),
        Command::Say { text } => format!("say {text}"),
        Command::MouseDown { x, y, .. } => format!("mouse down {}", at(x, y)),
        Command::MouseUp { x, y, .. } => format!("mouse up {}", at(x, y)),
        Command::Release { .. } => "release".to_owned(),
        Command::Quit => "quit".to_owned(),
        Command::Screenshot { .. }
        | Command::CellPoint { .. }
        | Command::State
        | Command::Wait { .. }
        | Command::Clock { .. }
        | Command::Trace { .. }
        | Command::Inbox { .. }
        | Command::Reply { .. } => return None,
    })
}

/// Adds what `command` does to the chat's MCP tab, when the client is shared and in the game.
pub fn note(scene: &mut Scene, command: &Command) {
    if shared()
        && let (Scene::Game(game), Some(line)) = (scene, describe(command))
    {
        game.chat_line_kind(line, MCP_LINE, Kind::Mcp);
    }
}

/// Whether a line typed in the MCP tab is for the agent: all but the game's own / commands.
pub fn for_agent(text: &str) -> bool {
    !text.starts_with('/')
}

/// A line the player typed in the MCP tab: shown there, kept for the agent's `inbox`, and not sent to the server.
pub fn typed(game: &mut Game, text: &str) {
    game.chat_line_kind(format!("> {text}"), TYPED_LINE, Kind::Mcp);
    INBOX.lock().unwrap().push_back(text.to_owned());
}

/// The agent's `inbox`: what was typed since the last one, waiting up to `wait` for the first line.
pub fn inbox(wait: Duration) -> Value {
    let end = Instant::now() + wait;
    loop {
        let lines: Vec<String> = INBOX.lock().unwrap().drain(..).collect();
        if !lines.is_empty() || Instant::now() >= end {
            return json!({"ok": true, "messages": lines});
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// The agent's `reply`: a line in the MCP tab.
pub fn reply(scene: &mut Scene, text: String) -> Result<Value, String> {
    let Scene::Game(game) = scene else { return Err("not in the game".into()) };
    game.chat_line_kind(text, REPLY_LINE, Kind::Mcp);
    Ok(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describes_what_the_driver_does_for_the_mcp_tab() {
        let describe = |line: &str| describe(&serde_json::from_str::<Command>(line).unwrap());
        assert_eq!(
            describe(r#"{"cmd":"click","x":512.4,"y":300.5,"button":"right","count":2,"mods":"Alt"}"#).as_deref(),
            Some("click (512, 301) right x2 +Alt")
        );
        assert_eq!(describe(r#"{"cmd":"click","x":1,"y":2}"#).as_deref(), Some("click (1, 2)"));
        assert_eq!(describe(r#"{"cmd":"drag","from":[1,2],"to":[3,4]}"#).as_deref(), Some("drag (1, 2) -> (3, 4)"));
        assert_eq!(describe(r#"{"cmd":"key","key":"Ctrl+F"}"#).as_deref(), Some("key Ctrl+F"));
        assert_eq!(describe(r#"{"cmd":"say","text":"@heal"}"#).as_deref(), Some("say @heal"));
        // What only looks isn't listed.
        for looking in [r#"{"cmd":"state"}"#, r#"{"cmd":"screenshot"}"#, r#"{"cmd":"clock","action":"pause"}"#, r#"{"cmd":"cell_point","x":1,"y":2}"#] {
            assert_eq!(describe(looking), None, "{looking}");
        }
    }

    #[test]
    fn leaves_slash_commands_to_the_game() {
        assert!(for_agent("go to prontera") && for_agent(" ") && for_agent("ไปพรอน"));
        assert!(!for_agent("/navi") && !for_agent("/sit"));
    }

    #[test]
    fn hands_the_agent_what_the_player_typed() {
        // Not through `typed`, which needs a game: the queue it feeds.
        INBOX.lock().unwrap().extend(["hello".to_owned(), "go north".to_owned()]);
        assert_eq!(inbox(Duration::ZERO), json!({"ok": true, "messages": ["hello", "go north"]}));
        assert_eq!(inbox(Duration::ZERO), json!({"ok": true, "messages": []}));
        // It waits for a line that comes in time.
        let late = std::thread::spawn(|| {
            std::thread::sleep(Duration::from_millis(150));
            INBOX.lock().unwrap().push_back("late".to_owned());
        });
        assert_eq!(inbox(Duration::from_secs(5)), json!({"ok": true, "messages": ["late"]}));
        late.join().unwrap();
        let parse = |line: &str| serde_json::from_str::<Command>(line).unwrap();
        assert_eq!(parse(r#"{"cmd":"inbox","ms":500}"#), Command::Inbox { ms: Some(500) });
        assert_eq!(parse(r#"{"cmd":"reply","text":"hi"}"#), Command::Reply { text: "hi".into() });
        // Neither shows in the tab as something the agent did.
        assert_eq!(describe(&Command::Inbox { ms: None }), None);
        assert_eq!(describe(&Command::Reply { text: "hi".into() }), None);
    }
}
