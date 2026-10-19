//! A client shared with an agent: `RUSTNAROK_CLIENT_CONTROL_SHARED` together with the control socket (control.rs) lets
//! the player play the same client. The real mouse and keyboard work, the window is drawn and kept like a player's,
//! settings are saved, and the chat has an MCP tab that lists what the agent does.

use crate::Scene;
use crate::chat::Kind;
use crate::control::{Button, Command};

/// The colour of the MCP tab's lines.
const MCP_LINE: [u8; 3] = [200, 170, 255];

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
        Command::Screenshot { .. } | Command::CellPoint { .. } | Command::State | Command::Wait { .. } | Command::Clock { .. } | Command::Trace { .. } => {
            return None;
        }
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
}
