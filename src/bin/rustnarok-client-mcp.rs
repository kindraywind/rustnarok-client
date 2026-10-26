//! Dev-only MCP server that lets Claude Code drive a live rustnarok-client through its control socket (src/control.rs).
//! Build with `cargo build --release`, then register it once with
//! `claude mcp add rustnarok-client-dev -- <repo>/target/release/rustnarok-client-mcp`.
//!
//! It speaks JSON-RPC 2.0 over stdio (one message per line; logs go to stderr): `initialize`, `tools/list` and
//! `tools/call`. `launch` starts the client next to this binary, in the repository it was built from; every other
//! tool is a control command, sent with its arguments as they are.

use std::io::{BufRead, BufReader, Write};
#[cfg(unix)]
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use rustnarok_client::play::{self, Grade};
use serde_json::{Value, json};

const INSTRUCTIONS: &str = "Drives a live rustnarok-client (the Ragnarok Online client in this repository) for testing. Call launch \
first; it stays logged in across calls until quit. Coordinates are the client's 1024x768 pixels, the same as a screenshot \
at scale 1. state lists units and ground items with their screen boxes to click. To catch a short effect, run the action \
through wait's `do` with `pause: true`, then alternate clock step and screenshot.";

fn tools() -> Value {
    let point = json!({"type": "array", "items": {"type": "number"}, "minItems": 2, "maxItems": 2});
    let button = json!({"type": "string", "enum": ["left", "right"], "description": "left by default"});
    let mods = json!({"type": "string", "description": "modifier keys held, e.g. \"Alt\" or \"Shift+Ctrl\""});
    // Each description starts with its grade (play.rs), which tools/list and tools/call go by.
    let tool = |grade: Grade, name: &str, description: &str, properties: Value, required: &[&str]| json!({"name": name, "description": format!("{} {description}", grade.tag()), "inputSchema": {"type": "object", "properties": properties, "required": required}});
    json!([
        tool(
            Grade::Fair,
            "launch",
            "Start the client with its control socket and wait until it's ready: in the game with a slot, at character select with an account only, else at the login screen. Retries a refused login (servers keep an account online for a few seconds after a client leaves). If a client already answers, reports its state instead.",
            json!({
                "server": {"type": "string", "description": "the login server, host:port (RUSTNAROK_CLIENT_SERVER, else 127.0.0.1:6900, by default)"},
                "account_file": {"type": "string", "description": "a file with `username:` and `password:` lines; the password goes to the client only"},
                "account": {"type": "string", "description": "which pair of lines: \"2\" for `username2:` and `password2:` (the first by default)"},
                "slot": {"type": "integer", "description": "character slot to play"},
                "play": {"type": "string", "enum": ["fair", "assist", "all"], "description": "the play mode: what the client allows, by grade (RUSTNAROK_CLIENT_PLAY, else all)"},
                "timeout_s": {"type": "integer", "description": "how long to wait for the client (90 by default)"},
                "shared": {"type": "boolean", "description": "true (the default): the player plays the same client with the real mouse and keyboard, and its chat has an MCP tab listing what you do. false: the client is yours alone and ignores the real mouse and keyboard"},
            }),
            &[],
        ),
        tool(Grade::Fair, "quit", "Close the client.", json!({}), &[]),
        tool(
            Grade::Fair,
            "screenshot",
            "The client's next frame as a PNG.",
            json!({
                "crop": {"type": "array", "items": {"type": "integer"}, "minItems": 4, "maxItems": 4, "description": "[x, y, width, height]"},
                "scale": {"type": "number", "description": "resize factor, e.g. 0.5, or 2 to look closely"},
            }),
            &[],
        ),
        tool(Grade::Fair, "move", "Move the pointer there (hover).", json!({"x": {"type": "number"}, "y": {"type": "number"}}), &["x", "y"]),
        tool(
            Grade::Fair,
            "click",
            "Click there, through the client's own mouse handling. Answers what the click landed on, as the cursor shows it: {\"hit\": {\"kind\": \"ui\", \"window\": name or null} | {\"kind\": \"unit\"|\"item\"|\"skill_unit\", \"id\": id} | {\"kind\": \"ground\", \"cell\": [x, y]} | null}; the play mode fair leaves the ground's cell out.",
            json!({"x": {"type": "number"}, "y": {"type": "number"}, "button": button, "mods": mods, "count": {"type": "integer", "description": "2 for a double click"}}),
            &["x", "y"],
        ),
        tool(
            Grade::Assist,
            "click_unit",
            "Click a unit where it is drawn now, through the client's own mouse handling, at a point that picks it (answers {\"point\": [x, y]}). Use the unit's id from state. Refused for a unit that's hidden, off the screen or covered.",
            json!({"id": {"type": "integer", "description": "the unit's id from state"}, "button": button, "mods": mods, "count": {"type": "integer", "description": "2 for a double click"}}),
            &["id"],
        ),
        tool(
            Grade::Assist,
            "cell_point",
            "Where map cell (x, y)'s middle is on the screen, as {\"point\": [x, y]} in the same 1024x768 pixels as click. Use it to click the ground at a cell. Errors outside the game and for a cell behind the camera. With `cells` instead of x and y, many cells in one call: {\"points\": [...]} in their order, null for a cell a single call would refuse.",
            json!({
                "x": {"type": "integer"},
                "y": {"type": "integer"},
                "cells": {"type": "array", "items": {"type": "array", "items": {"type": "integer"}, "minItems": 2, "maxItems": 2}, "description": "[[x, y], ...] instead of x and y"},
            }),
            &[],
        ),
        tool(
            Grade::Fair,
            "wheel",
            "Turn the mouse wheel over a point: `rows` notches, positive toward the player. Over the world it zooms the camera (positive zooms out); over a list it scrolls.",
            json!({"x": {"type": "number"}, "y": {"type": "number"}, "rows": {"type": "integer"}}),
            &["x", "y", "rows"],
        ),
        tool(
            Grade::Fair,
            "drag",
            "Press at `from`, move to `to` and let go (windows, items onto hotkeys, the camera with the right button).",
            json!({"from": point, "to": point, "button": button, "mods": mods}),
            &["from", "to"]
        ),
        tool(
            Grade::Fair,
            "key",
            "Press a key: Enter, Escape, Tab, Backspace, Space, Up/Down/Left/Right, F1-F12 or one character, after modifiers like \"Alt+E\" (inventory), \"Alt+S\" (skills), \"Alt+Q\" (equipment).",
            json!({"key": {"type": "string"}}),
            &["key"],
        ),
        tool(
            Grade::Fair,
            "type",
            "Type text into the focused field (login, chat line, NPC input); Hangul comes in through the input method. Send with the Enter key.",
            json!({"text": {"type": "string"}}),
            &["text"]
        ),
        tool(
            Grade::Fair,
            "say",
            "Say a chat line as if typed in the chat box and sent: the game's / commands (\"/sit\", emotions like \"/!\", \"/where\"), % party lines and plain chat all work; a GM command like \"@monster poring\" or \"@heal\" goes to the server as chat (GM commands are test only).",
            json!({"text": {"type": "string"}}),
            &["text"]
        ),
        tool(
            Grade::Fair,
            "inbox",
            "What the player typed in the chat's MCP tab since the last call (a client launched shared). Waits up to ms for a line, 0 by default; answer with reply.",
            json!({"ms": {"type": "integer", "description": "how long to wait for a line, at most 100000"}}),
            &[],
        ),
        tool(Grade::Fair, "reply", "Answer the player in the chat's MCP tab, which shows what you do there.", json!({"text": {"type": "string"}}), &["text"]),
        tool(
            Grade::Fair,
            "state",
            "The scene; in the game the map, the player (cell, HP/SP, levels, zeny, screen box), open windows, `escape` (the Esc or death menu's buttons while it is open, null otherwise; it is not in `windows` and takes every click until it is closed), the NPC dialog, the aimed skill, units and ground items nearest first (id, kind, name, cell, screen box [left, top, right, bottom]), counts, status icons and the last chat lines. cell_point turns player.cell or a unit's cell into a point to click. Under a play mode, what is above it is left out (src/play.rs).",
            json!({"only": {"type": "array", "items": {"type": "string"}, "description": "just these fields of the game's state, e.g. [\"player\", \"units\", \"dialog\"]: a smaller, faster answer"}}),
            &[],
        ),
        tool(
            Grade::Fair,
            "wait",
            "Sleep `ms`, or wait up to `ms` (10000 by default) for a packet or an event seen since the previous command, so a fast answer to that command isn't missed. Events are the client's MapEvent/CharEvent names (Act, SkillHit, SkillUsed, Casting, Chat, Unit, Vanish, Status, MapChange, Entered, Characters, ...) or LoginAccepted/LoginRefused.",
            json!({
                "ms": {"type": "integer"},
                "packet": {"type": "string", "description": "packet ID in hex, e.g. \"0x08c8\" (test only)"},
                "event": {"type": "string"},
                "fresh": {"type": "boolean", "description": "only count what comes after this call"},
                "pause": {"type": "boolean", "description": "pause the animation clock the moment it comes (test only)"},
                "do": {"type": "object", "description": "a command to run first, e.g. {\"cmd\": \"key\", \"key\": \"F1\"} or {\"cmd\": \"click\", \"x\": 500, \"y\": 400}"},
            }),
            &[],
        ),
        tool(
            Grade::Test,
            "clock",
            "Pause or resume the animation clock, or step it `ms` (16 by default) further while paused, to screenshot short effects frame by frame. Resuming catches up with real time.",
            json!({"action": {"type": "string", "enum": ["pause", "resume", "step"]}, "ms": {"type": "integer"}}),
            &["action"],
        ),
        tool(
            Grade::Test,
            "trace",
            "The last packets received from the char and map servers, in the RUSTNAROK_CLIENT_TRACE format (time ms, id, length, first bytes).",
            json!({"last": {"type": "integer", "description": "lines, 20 by default"}, "events": {"type": "boolean", "description": "also the client events they made (== lines), e.g. why a connection dropped"}}),
            &[],
        ),
    ])
}

struct Server {
    socket: PathBuf,
    /// The play mode: the highest grade of tool listed and answered.
    play: Grade,
    /// The client this server started.
    child: Option<Child>,
}

impl Server {
    /// One command, on a connection of its own.
    fn request(&self, command: &Value) -> Result<Value, String> {
        let stream = connect(&self.socket).map_err(|_| "no client is running; call launch first".to_owned())?;
        let _ = stream.set_read_timeout(Some(Duration::from_secs(120)));
        (&stream).write_all(format!("{command}\n").as_bytes()).map_err(|error| error.to_string())?;
        let mut line = String::new();
        BufReader::new(&stream).read_line(&mut line).map_err(|error| format!("no answer: {error}"))?;
        serde_json::from_str(&line).map_err(|error| format!("bad answer {line:?}: {error}"))
    }

    fn call(&mut self, name: &str, arguments: Value) -> Result<Value, String> {
        if grade(name).is_some_and(|grade| grade > self.play) {
            return Err(format!("test only (play mode: {})", self.play.mode()));
        }
        match name {
            "launch" => self.launch(&arguments).map(text),
            "quit" => Ok(text(self.quit())),
            _ => {
                let mut command = if arguments.is_object() { arguments } else { json!({}) };
                command["cmd"] = name.into();
                let mut reply = self.request(&command)?;
                if reply["ok"] != true {
                    return Err(reply["error"].as_str().map_or_else(|| reply.to_string(), str::to_owned));
                }
                if let Some(png) = reply["png"].as_str() {
                    return Ok(json!([{"type": "image", "data": png, "mimeType": "image/png"}]));
                }
                if let Some(trace) = reply["trace"].as_str() {
                    return Ok(text(trace.to_owned()));
                }
                if let Some(fields) = reply.as_object_mut() {
                    fields.remove("ok");
                }
                Ok(text(if reply == json!({}) { "ok".to_owned() } else { reply.to_string() }))
            }
        }
    }

    fn launch(&mut self, arguments: &Value) -> Result<String, String> {
        if let Ok(state) = self.request(&json!({"cmd": "state"})) {
            return Ok(format!("a client is already running: {state}"));
        }
        let account_file = arguments["account_file"].as_str();
        let slot = arguments["slot"].as_u64();
        let game = std::env::current_exe().map_err(|error| error.to_string())?.with_file_name(format!("rustnarok-client{}", std::env::consts::EXE_SUFFIX));
        let log_path = self.socket.with_extension("log");
        let log = std::fs::File::create(&log_path).map_err(|error| format!("{}: {error}", log_path.display()))?;
        let mut command = Command::new(&game);
        // The client finds data.grf and the data folders in the repository.
        command.current_dir(env!("CARGO_MANIFEST_DIR")).env("RUSTNAROK_CLIENT_CONTROL", &self.socket);
        if let Some(mode) = arguments["play"].as_str() {
            self.play = Grade::parse(mode).ok_or_else(|| format!("no play mode {mode}"))?;
        }
        command.env("RUSTNAROK_CLIENT_PLAY", self.play.mode());
        if arguments["shared"].as_bool().unwrap_or(true) {
            command.env("RUSTNAROK_CLIENT_CONTROL_SHARED", "1");
        }
        if let Some(server) = arguments["server"].as_str() {
            command.env("RUSTNAROK_CLIENT_SERVER", server);
        }
        command.stdin(Stdio::null()).stdout(log.try_clone().map_err(|error| error.to_string())?).stderr(log);
        if let Some(path) = account_file {
            let which = arguments["account"].as_str().unwrap_or_default();
            let contents = std::fs::read_to_string(path).map_err(|error| format!("{path}: {error}"))?;
            let (user, password) = account(&contents, which).ok_or_else(|| format!("{path} has no `username{which}:` and `password{which}:` lines"))?;
            command.env("RUSTNAROK_CLIENT_LOGIN", format!("{user}:{password}"));
            command.env("RUSTNAROK_CLIENT_AUTOSTART", slot.map_or_else(|| "select".to_owned(), |slot| slot.to_string()));
        }
        self.child = Some(command.spawn().map_err(|error| format!("{}: {error}", game.display()))?);
        let goal = match (account_file, slot) {
            (None, _) => "login",
            (Some(_), None) => "char_select",
            (Some(_), Some(_)) => "game",
        };
        let deadline = Instant::now() + Duration::from_secs(arguments["timeout_s"].as_u64().unwrap_or(90));
        let (mut state, mut retries) = (Value::Null, 0);
        while Instant::now() < deadline {
            if let Some(status) = self.child.as_mut().and_then(|child| child.try_wait().ok().flatten()) {
                return Err(format!("the client exited ({status}); its output is in {}", log_path.display()));
            }
            std::thread::sleep(Duration::from_millis(300));
            let Ok(now) = self.request(&json!({"cmd": "state"})) else { continue };
            state = now;
            match state["scene"].as_str() {
                Some(scene) if scene == goal && state["loading"] == false => {
                    let pid = self.child.as_ref().map_or(0, Child::id);
                    return Ok(format!("client {pid} is up (output in {}): {state}", log_path.display()));
                }
                Some("message") => {
                    // A login server that can't be reached won't be later.
                    let trace = self.request(&json!({"cmd": "trace", "events": true, "last": 50})).unwrap_or_default();
                    if let Some(refusal) = trace["trace"].as_str().and_then(|lines| lines.lines().rev().find(|line| line.contains("== LoginRefused(Io("))) {
                        return Err(format!("can't reach the login server: {refusal}"));
                    }
                    // Refused, most likely because the server still has the account online: close the message and log
                    // in again a little later.
                    if account_file.is_none() || retries == 6 {
                        return Err(format!("stopped at a message: {state}"));
                    }
                    retries += 1;
                    eprintln!("rustnarok-client-mcp: login refused ({}), retrying", state["text"]);
                    std::thread::sleep(Duration::from_secs(5));
                    for key in ["Enter", "Enter"] {
                        let _ = self.request(&json!({"cmd": "key", "key": key}));
                    }
                }
                _ => {}
            }
        }
        Err(format!("not at {goal} in time; last state: {state}"))
    }

    fn quit(&mut self) -> String {
        let answered = self.request(&json!({"cmd": "quit"})).is_ok();
        if let Some(mut child) = self.child.take() {
            let deadline = Instant::now() + Duration::from_secs(5);
            while matches!(child.try_wait(), Ok(None)) && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(100));
            }
            if matches!(child.try_wait(), Ok(None)) {
                let _ = child.kill();
                let _ = child.wait();
                return "killed the client".into();
            }
        }
        if answered { "the client quit".into() } else { "no client was running".into() }
    }
}

#[cfg(unix)]
fn connect(socket: &Path) -> std::io::Result<UnixStream> {
    UnixStream::connect(socket)
}

/// On Windows the control "socket" is a file naming a localhost port and the token to open with (src/control.rs).
#[cfg(windows)]
fn connect(socket: &Path) -> std::io::Result<std::net::TcpStream> {
    let contents = std::fs::read_to_string(socket)?;
    let (address, token) = contents.split_once(' ').ok_or(std::io::ErrorKind::InvalidData)?;
    let mut stream = std::net::TcpStream::connect(address)?;
    writeln!(stream, "{token}")?;
    Ok(stream)
}

/// The `username<which>:` and `password<which>:` lines of an account file.
fn account(contents: &str, which: &str) -> Option<(String, String)> {
    let field = |name: &str| contents.lines().find_map(|line| Some(line.trim().strip_prefix(name)?.strip_prefix(which)?.strip_prefix(':')?.trim().to_owned()));
    Some((field("username")?, field("password")?)).filter(|(user, password)| !user.is_empty() && !password.is_empty())
}

/// A tool's grade, from the start of its description.
fn grade(name: &str) -> Option<Grade> {
    let tools = tools();
    let description = tools.as_array()?.iter().find(|tool| tool["name"] == name)?["description"].as_str()?.to_owned();
    [Grade::Fair, Grade::Assist, Grade::Test].into_iter().find(|grade| description.starts_with(grade.tag()))
}

/// The tools a play mode lists.
fn listed(mode: Grade) -> Vec<Value> {
    tools().as_array().into_iter().flatten().filter(|tool| tool["name"].as_str().and_then(grade).is_some_and(|grade| grade <= mode)).cloned().collect()
}

fn text(text: String) -> Value {
    json!([{"type": "text", "text": text}])
}

#[cfg(test)]
#[test]
fn picks_an_account_from_the_file() {
    let file = "# note: not a username\nusername: one\npassword: p1\n\nusername2: two\npassword2: p2\n";
    assert_eq!(account(file, ""), Some(("one".into(), "p1".into())));
    assert_eq!(account(file, "2"), Some(("two".into(), "p2".into())));
    assert_eq!(account(file, "3"), None);
}

#[cfg(test)]
#[test]
fn offers_click_unit() {
    let tools = tools();
    let tool = tools.as_array().unwrap().iter().find(|tool| tool["name"] == "click_unit").expect("a click_unit tool");
    assert_eq!(tool["inputSchema"]["required"], json!(["id"]));
    let properties = tool["inputSchema"]["properties"].as_object().unwrap();
    let mut names: Vec<_> = properties.keys().map(String::as_str).collect();
    names.sort_unstable();
    assert_eq!(names, ["button", "count", "id", "mods"]);
}

#[cfg(test)]
#[test]
fn offers_cell_point() {
    let tools = tools();
    let tool = tools.as_array().unwrap().iter().find(|tool| tool["name"] == "cell_point").expect("a cell_point tool");
    // x and y, or cells.
    assert_eq!(tool["inputSchema"]["required"], json!([]));
    let properties = &tool["inputSchema"]["properties"];
    assert_eq!((&properties["x"]["type"], &properties["y"]["type"]), (&json!("integer"), &json!("integer")));
    assert_eq!(properties["cells"]["items"]["maxItems"], 2);
    assert_eq!(properties.as_object().unwrap().len(), 3);
}

#[cfg(test)]
#[test]
fn grades_every_tool() {
    let tools = tools();
    let names: Vec<_> = tools.as_array().unwrap().iter().filter_map(|tool| tool["name"].as_str()).collect();
    assert!(names.iter().all(|name| grade(name).is_some()), "a tool without a grade");
    let listed = |mode| listed(mode).iter().filter_map(|tool| tool["name"].as_str().map(str::to_owned)).collect::<Vec<_>>();
    assert_eq!(listed(Grade::Test).len(), names.len());
    assert!(listed(Grade::Assist).contains(&"click_unit".to_owned()) && !listed(Grade::Assist).contains(&"trace".to_owned()));
    assert!(!listed(Grade::Fair).iter().any(|name| ["click_unit", "cell_point", "clock", "trace"].contains(&name.as_str())));
    assert!(listed(Grade::Fair).contains(&"click".to_owned()));
}

fn main() {
    // In the repository's target folder: a short, private path (socket paths have a length limit) that's ignored by git.
    let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target");
    let _ = std::fs::create_dir_all(&target);
    let mut server = Server { socket: target.join("rustnarok-client-dev.sock"), play: play::mode(), child: None };
    let mut stdout = std::io::stdout();
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let message: Value = match serde_json::from_str(&line) {
            Ok(message) => message,
            Err(error) => {
                eprintln!("rustnarok-client-mcp: not JSON ({error}): {line}");
                continue;
            }
        };
        // Notifications (no ID) need no answer.
        let Some(id) = message.get("id").cloned() else { continue };
        let params = &message["params"];
        let result = match message["method"].as_str().unwrap_or_default() {
            "initialize" => json!({
                "protocolVersion": params["protocolVersion"].as_str().unwrap_or("2025-06-18"),
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "rustnarok-client-dev", "version": env!("CARGO_PKG_VERSION")},
                "instructions": INSTRUCTIONS,
            }),
            "tools/list" => json!({"tools": listed(server.play)}),
            "tools/call" => match server.call(params["name"].as_str().unwrap_or_default(), params["arguments"].clone()) {
                Ok(content) => json!({"content": content, "isError": false}),
                Err(error) => json!({"content": text(error), "isError": true}),
            },
            "ping" => json!({}),
            method => {
                let error = json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": format!("no method {method}")}});
                let _ = writeln!(stdout, "{error}").and_then(|()| stdout.flush());
                continue;
            }
        };
        if writeln!(stdout, "{}", json!({"jsonrpc": "2.0", "id": id, "result": result})).and_then(|()| stdout.flush()).is_err() {
            break;
        }
    }
    // Claude Code went away: don't leave a logged-in client behind.
    if server.child.is_some() {
        eprintln!("rustnarok-client-mcp: {}", server.quit());
    }
}
