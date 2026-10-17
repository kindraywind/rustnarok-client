//! Dev-only remote control, for test automation: with `RUSTNAROK_CLIENT_CONTROL=<path>` the client listens on that Unix socket
//! (owner only, no TCP port) for newline-delimited JSON commands and answers each with one line of JSON
//! (`{"ok": true, ...}` or `{"ok": false, "error": ...}`). Without the variable none of this runs. A driven client
//! ignores the real mouse and keyboard, opens behind the window in use and draws on screen only while it has the focus
//! (click it to watch); screenshots don't need that.
//!
//! The `rustnarok-client-mcp` binary (src/bin/rustnarok-client-mcp.rs) offers these commands to Claude Code as MCP tools. Build with
//! `cargo build --release`, then register it once with
//! `claude mcp add rustnarok-client-dev -- <repo>/target/release/rustnarok-client-mcp`.
//!
//! Commands by `cmd`, in the client's logical 1024x768 pixels:
//! - `screenshot` `{crop?: [x, y, width, height], scale?}`: the next frame through the `RUSTNAROK_CLIENT_SCREENSHOT` capture,
//!   as base64 PNG (`png`).
//! - `move` `{x, y}`, `click` `{x, y, button?: "left"|"right", mods?: "Alt+Shift", count?}`, `drag` `{from: [x, y],
//!   to: [x, y], button?, mods?}`: the pointer and buttons, through the same code as the mouse.
//! - `key` `{key}`: "Enter", "Escape", "Tab", "Backspace", "Space", "Insert", "Delete", "Home", "End", "PageUp",
//!   "PageDown", "Up"/"Down"/"Left"/"Right", "F1"–"F12" or one
//!   character, after modifiers like "Alt+" or "Ctrl+".
//! - `type` `{text}`: into the focused field; non-ASCII runs (Hangul) come in as the input method's commit.
//! - `say` `{text}`: a chat line or a GM command.
//! - `state`: the scene and the login server; in the game the map, the player, open windows, the NPC dialog, the aimed
//!   skill, units and ground items (with their screen boxes), counts and status icons. Everything comes from what the
//!   client itself knows, whatever server it plays on.
//! - `wait` `{ms?, packet?, event?, fresh?, pause?, do?}`: sleeps `ms`; with a `packet` id ("0x08c8") or an `event`
//!   (a `MapEvent`/`CharEvent` variant like "Act", or "LoginAccepted"/"LoginRefused"), waits up to `ms` (10 s by
//!   default) for one seen since the previous command, so a quick answer isn't missed between two calls. `fresh`
//!   only counts new ones; `do` is a command to run first (and implies `fresh`); `pause` stops the clock on arrival.
//! - `clock` `{action: "pause"|"resume"|"step", ms?}`: the animation clock; a step freezes it `ms` (16) further on.
//! - `trace` `{last?, events?}`: the last packets received (20), as `RUSTNAROK_CLIENT_TRACE` prints them; with `events`, the
//!   events they made in between (`==` lines, with what they say: a disconnection's reason, say).
//! - `quit`.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Cursor, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Condvar, Mutex, mpsc};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use image::RgbaImage;
use image::imageops::FilterType;
use rustnarok_client::net;
use serde::Deserialize;
use serde_json::{Value, json};
use winit::event::{ElementState, KeyEvent};
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::keyboard::{Key, KeyCode, ModifiersState, NamedKey, NativeKeyCode, PhysicalKey, SmolStr};

use crate::render::{Gpu, Sprite, WorldPass};
use crate::{App, DialogButton, Game, HOMUNCULUS, MERCENARY, MONSTER, NPC, NetEvent, Owner, PET, PLAYER, Panel, Scene, inventory};

#[derive(Debug, Deserialize, PartialEq)]
#[serde(tag = "cmd", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Screenshot {
        crop: Option<[u32; 4]>,
        scale: Option<f32>,
    },
    Move {
        x: f32,
        y: f32,
    },
    Click {
        x: f32,
        y: f32,
        #[serde(default)]
        button: Button,
        #[serde(default)]
        mods: String,
        count: Option<u8>,
    },
    Drag {
        from: [f32; 2],
        to: [f32; 2],
        #[serde(default)]
        button: Button,
        #[serde(default)]
        mods: String,
    },
    Key {
        key: String,
    },
    /// The wheel at a point: `rows` notches, positive toward the player (scrolls down).
    Wheel {
        x: f32,
        y: f32,
        rows: i32,
    },
    Type {
        text: String,
    },
    Say {
        text: String,
    },
    State,
    Wait {
        ms: Option<u64>,
        #[serde(default, deserialize_with = "packet_id")]
        packet: Option<u16>,
        event: Option<String>,
        #[serde(default)]
        fresh: bool,
        #[serde(default)]
        pause: bool,
        #[serde(rename = "do")]
        first: Option<Box<Command>>,
    },
    Clock {
        action: ClockAction,
        ms: Option<u64>,
    },
    Trace {
        last: Option<usize>,
        #[serde(default)]
        events: bool,
    },
    Quit,
}

#[derive(Debug, Default, Deserialize, PartialEq, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum Button {
    #[default]
    Left,
    Right,
}

#[derive(Debug, Deserialize, PartialEq, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum ClockAction {
    Pause,
    Resume,
    Step,
}

/// A packet ID as a number or in hex ("0x08c8").
fn packet_id<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Option<u16>, D::Error> {
    let id = match Value::deserialize(deserializer)? {
        Value::Number(number) => number.as_u64().and_then(|id| u16::try_from(id).ok()),
        Value::String(text) => u16::from_str_radix(text.trim_start_matches("0x"), 16).ok(),
        _ => None,
    };
    id.map(Some).ok_or_else(|| serde::de::Error::custom("packet: a number or hex like \"0x08c8\""))
}

/// A command on its way to the event loop, and where its answer goes.
pub struct Request {
    command: Command,
    reply: mpsc::Sender<Value>,
}

/// What `App::key` reads of a key press: winit's `KeyEvent` can't be made outside winit, so commands make these.
pub struct KeyPress {
    pub state: ElementState,
    pub logical_key: Key,
    pub physical_key: PhysicalKey,
    pub text: Option<String>,
}

impl From<&KeyEvent> for KeyPress {
    fn from(event: &KeyEvent) -> Self {
        Self { state: event.state, logical_key: event.logical_key.clone(), physical_key: event.physical_key, text: event.text.as_deref().map(str::to_owned) }
    }
}

/// Packets and events seen, newest last, for `wait` and `trace`.
struct Log {
    next: u64,
    entries: VecDeque<Entry>,
}

struct Entry {
    seq: u64,
    at: Instant,
    /// A packet's ID, or an event's variant name.
    seen: Seen,
    /// The packet's trace line, or the event's: as it debug-prints (shortened), after `==`.
    text: String,
}

enum Seen {
    Packet(u16),
    Event(String),
}

const LOG_SIZE: usize = 2000;
static LOG: Mutex<Log> = Mutex::new(Log { next: 0, entries: VecDeque::new() });
static SEEN: Condvar = Condvar::new();
/// Where the log stood at the last command other than `wait`: what a `wait` looks at by default.
// ponytail: one mark for every connection (one driver at a time); keep one per connection if drivers ever share a client.
static MARK: AtomicU64 = AtomicU64::new(0);
static ON: AtomicBool = AtomicBool::new(false);
/// The animation clock while paused.
static FROZEN: Mutex<Option<Instant>> = Mutex::new(None);
/// A screenshot asked of the next frame drawn, and the frame.
static WANT_SHOT: AtomicBool = AtomicBool::new(false);
static SHOT: Mutex<Option<RgbaImage>> = Mutex::new(None);

/// Opens the control socket when `RUSTNAROK_CLIENT_CONTROL` names one.
pub fn listen(proxy: EventLoopProxy<NetEvent>) {
    let Some(path) = std::env::var_os("RUSTNAROK_CLIENT_CONTROL") else { return };
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    // Whoever can connect plays the logged-in character.
    let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    let _ = net::PACKET_HOOK.set(packet_seen);
    ON.store(true, Ordering::Relaxed);
    eprintln!("control socket at {}", path.display());
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let proxy = proxy.clone();
            std::thread::spawn(move || serve(stream, &proxy));
        }
    });
}

/// Answers one connection's commands in order. `wait` and `trace` are answered here, the rest on the event loop.
fn serve(stream: UnixStream, proxy: &EventLoopProxy<NetEvent>) {
    let Ok(mut writer) = stream.try_clone() else { return };
    for line in BufReader::new(stream).lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Command>(&line) {
            Ok(Command::Wait { ms, packet, event, fresh, pause, first }) => wait(proxy, ms, packet, event, fresh, pause, first),
            Ok(Command::Trace { last, events }) => trace(last.unwrap_or(20), events),
            Ok(command) => {
                MARK.store(LOG.lock().unwrap().next, Ordering::Relaxed);
                forward(proxy, command)
            }
            Err(error) => json!({"ok": false, "error": error.to_string()}),
        };
        if writeln!(writer, "{reply}").is_err() {
            break;
        }
    }
}

fn forward(proxy: &EventLoopProxy<NetEvent>, command: Command) -> Value {
    let (reply, answer) = mpsc::channel();
    if proxy.send_event(NetEvent::Control(Request { command, reply })).is_err() {
        return json!({"ok": false, "error": "the client is closing"});
    }
    answer.recv_timeout(Duration::from_secs(30)).unwrap_or_else(|_| json!({"ok": false, "error": "no answer from the client"}))
}

fn wait(
    proxy: &EventLoopProxy<NetEvent>,
    ms: Option<u64>,
    packet: Option<u16>,
    event: Option<String>,
    fresh: bool,
    pause: bool,
    first: Option<Box<Command>>,
) -> Value {
    if fresh || first.is_some() {
        MARK.store(LOG.lock().unwrap().next, Ordering::Relaxed);
    }
    if let Some(command) = first {
        let done = forward(proxy, *command);
        if done["ok"] != true {
            return done;
        }
    }
    if packet.is_none() && event.is_none() {
        std::thread::sleep(Duration::from_millis(ms.unwrap_or(0)));
        return json!({"ok": true});
    }
    let limit = ms.unwrap_or(10_000);
    let deadline = Instant::now() + Duration::from_millis(limit);
    let mut log = LOG.lock().unwrap();
    loop {
        let mark = MARK.load(Ordering::Relaxed);
        let wanted = |entry: &&Entry| match &entry.seen {
            Seen::Packet(id) => packet == Some(*id),
            Seen::Event(name) => event.as_deref() == Some(name.as_str()),
        };
        if let Some(entry) = log.entries.iter().filter(|entry| entry.seq >= mark).find(wanted) {
            let (seen, ago) = (entry.text.clone(), entry.at.elapsed().as_millis() as u64);
            // The next wait looks past this one.
            MARK.store(entry.seq + 1, Ordering::Relaxed);
            drop(log);
            let paused = pause && forward(proxy, Command::Clock { action: ClockAction::Pause, ms: None })["ok"] == true;
            return json!({"ok": true, "seen": seen, "ago_ms": ago, "paused": paused});
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return json!({"ok": false, "error": format!("nothing came in {limit} ms")});
        }
        log = SEEN.wait_timeout(log, left).unwrap().0;
    }
}

fn trace(last: usize, events: bool) -> Value {
    let log = LOG.lock().unwrap();
    let lines: Vec<&str> = log.entries.iter().filter(|entry| events || matches!(entry.seen, Seen::Packet(_))).map(|entry| entry.text.as_str()).collect();
    json!({"ok": true, "trace": lines[lines.len().saturating_sub(last)..].join("\n")})
}

fn record(seen: Seen, text: String) {
    let mut log = LOG.lock().unwrap();
    let seq = log.next;
    log.next += 1;
    log.entries.push_back(Entry { seq, at: Instant::now(), seen, text });
    if log.entries.len() > LOG_SIZE {
        log.entries.pop_front();
    }
    SEEN.notify_all();
}

/// Every packet from the char and map servers (on their reader threads), in the `RUSTNAROK_CLIENT_TRACE` format.
fn packet_seen(packet: &[u8]) {
    let id = u16::from_le_bytes([packet[0], packet[1]]);
    record(Seen::Packet(id), format!("{:05} <- {id:#06x} {} {:02x?}", trace_ms(), packet.len(), &packet[..packet.len().min(32)]));
}

/// The `RUSTNAROK_CLIENT_TRACE` timestamp: wall-clock milliseconds, the last five digits.
fn trace_ms() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() % 100_000
}

/// Whether the client takes commands from the socket. Then the real mouse and keyboard are ignored: between commands
/// they'd move the pointer, type into the chat line or hold modifiers down.
pub fn driven() -> bool {
    ON.load(Ordering::Relaxed)
}

/// Notes an event from the servers for `wait`: its variant's name, and what it says (a disconnection's reason, say).
pub fn observe(event: &NetEvent) {
    if !driven() {
        return;
    }
    let (name, debug) = match event {
        // Not its session keys.
        NetEvent::Login(Ok(_)) => ("LoginAccepted".to_owned(), "LoginAccepted".to_owned()),
        NetEvent::Login(Err(error)) => ("LoginRefused".to_owned(), format!("LoginRefused({error:?})")),
        NetEvent::Char(event) => (String::new(), format!("{event:?}")),
        NetEvent::Map(event) => (String::new(), format!("{event:?}")),
        NetEvent::Control(_) => return,
    };
    let name = if name.is_empty() { debug.split(|character: char| !character.is_alphanumeric()).next().unwrap_or_default().to_owned() } else { name };
    let end = debug.char_indices().nth(300).map_or(debug.len(), |(at, _)| at);
    record(Seen::Event(name), format!("{:05} == {}", trace_ms(), &debug[..end]));
}

/// The animation clock: real time unless the `clock` command froze it.
// ponytail: only the frame clock freezes; what arrives while paused keeps its real time, so it shows once steps reach
// it, or at once on resume.
pub fn now() -> Instant {
    FROZEN.lock().unwrap().unwrap_or_else(Instant::now)
}

/// Every drawn frame passes here: a screenshot asked for is taken of it, the way `RUSTNAROK_CLIENT_SCREENSHOT` captures.
pub fn on_draw(gpu: &Gpu, world: Option<&dyn WorldPass>, sprites: &[Sprite]) {
    if WANT_SHOT.swap(false, Ordering::Relaxed) {
        *SHOT.lock().unwrap() = Some(gpu.snapshot(world, sprites));
    }
}

/// Runs a command on the event loop, where the client's state lives, and answers it.
pub fn run(app: &mut App, event_loop: &ActiveEventLoop, Request { command, reply }: Request) {
    let answer = match execute(app, event_loop, command) {
        Ok(Value::Null) => json!({"ok": true}),
        Ok(value) => value,
        Err(error) => json!({"ok": false, "error": error}),
    };
    let _ = reply.send(answer);
}

fn execute(app: &mut App, event_loop: &ActiveEventLoop, command: Command) -> Result<Value, String> {
    match command {
        Command::Screenshot { crop, scale } => screenshot(app, event_loop, crop, scale),
        Command::Move { x, y } => {
            point(app, [x, y]);
            Ok(Value::Null)
        }
        Command::Click { x, y, button, mods, count } => {
            let held = modifiers(&mods)?;
            point(app, [x, y]);
            hold(app, held, |app| {
                for _ in 0..count.unwrap_or(1) {
                    press(app, event_loop, button, ElementState::Pressed);
                    press(app, event_loop, button, ElementState::Released);
                }
            });
            Ok(Value::Null)
        }
        Command::Drag { from, to, button, mods } => {
            let held = modifiers(&mods)?;
            hold(app, held, |app| {
                point(app, from);
                press(app, event_loop, button, ElementState::Pressed);
                point(app, to);
                press(app, event_loop, button, ElementState::Released);
            });
            Ok(Value::Null)
        }
        Command::Wheel { x, y, rows } => {
            point(app, [x, y]);
            app.wheel(rows);
            Ok(Value::Null)
        }
        Command::Key { key } => {
            let (held, key) = parse_key(&key)?;
            hold(app, held, |app| app.key(&key));
            Ok(Value::Null)
        }
        Command::Type { text } => {
            let mut rest = text.as_str();
            while let Some(first) = rest.chars().next() {
                if first.is_ascii() {
                    app.key(&typed(first, true));
                    rest = &rest[1..];
                } else {
                    let end = rest.find(|character: char| character.is_ascii()).unwrap_or(rest.len());
                    app.compose(String::new());
                    app.commit(&rest[..end]);
                    rest = &rest[end..];
                }
            }
            Ok(Value::Null)
        }
        Command::Say { text } => {
            let Scene::Game(game) = &mut app.scene else { return Err("not in the game".into()) };
            game.session.chat(&game.me.name, &text).map_err(|error| error.to_string())?;
            Ok(Value::Null)
        }
        Command::State => Ok(state(app)),
        Command::Clock { action, ms } => {
            let mut frozen = FROZEN.lock().unwrap();
            *frozen = match action {
                ClockAction::Pause => Some(frozen.unwrap_or_else(Instant::now)),
                ClockAction::Resume => None,
                ClockAction::Step => Some(frozen.unwrap_or_else(Instant::now) + Duration::from_millis(ms.unwrap_or(16))),
            };
            Ok(json!({"ok": true, "paused": frozen.is_some()}))
        }
        Command::Quit => {
            event_loop.exit();
            Ok(Value::Null)
        }
        Command::Wait { .. } | Command::Trace { .. } => Err("wait and trace can't be run by wait".into()),
    }
}

fn point(app: &mut App, at: [f32; 2]) {
    app.cursor = at;
    app.pointer_moved();
}

fn press(app: &mut App, event_loop: &ActiveEventLoop, button: Button, state: ElementState) {
    match button {
        Button::Left => app.left_button(event_loop, state),
        Button::Right => app.right_button(state),
    }
}

/// Holds modifier keys down while `act` runs.
fn hold(app: &mut App, held: ModifiersState, act: impl FnOnce(&mut App)) {
    let before = std::mem::replace(&mut app.modifiers, held);
    act(app);
    app.modifiers = before;
}

fn modifiers(names: &str) -> Result<ModifiersState, String> {
    names.split('+').map(str::trim).filter(|name| !name.is_empty()).try_fold(ModifiersState::empty(), |held, name| {
        Ok(held
            | match name.to_ascii_lowercase().as_str() {
                "alt" | "option" => ModifiersState::ALT,
                "ctrl" | "control" => ModifiersState::CONTROL,
                "shift" => ModifiersState::SHIFT,
                "cmd" | "super" | "meta" => ModifiersState::SUPER,
                _ => return Err(format!("unknown modifier {name:?}")),
            })
    })
}

/// A key by name ("Enter", "F5", one character), after modifiers ("Alt+E").
fn parse_key(name: &str) -> Result<(ModifiersState, KeyPress), String> {
    let (held, key) = match name.rsplit_once('+') {
        Some((held, key)) if !key.is_empty() => (modifiers(held)?, key),
        _ => (ModifiersState::empty(), name),
    };
    let mut characters = key.chars();
    if let (Some(character), None) = (characters.next(), characters.next()) {
        // Alt and Ctrl make shortcuts, not text.
        return Ok((held, typed(character, !held.alt_key() && !held.control_key() && !held.super_key())));
    }
    const FUNCTION: [(NamedKey, KeyCode); 12] = [
        (NamedKey::F1, KeyCode::F1),
        (NamedKey::F2, KeyCode::F2),
        (NamedKey::F3, KeyCode::F3),
        (NamedKey::F4, KeyCode::F4),
        (NamedKey::F5, KeyCode::F5),
        (NamedKey::F6, KeyCode::F6),
        (NamedKey::F7, KeyCode::F7),
        (NamedKey::F8, KeyCode::F8),
        (NamedKey::F9, KeyCode::F9),
        (NamedKey::F10, KeyCode::F10),
        (NamedKey::F11, KeyCode::F11),
        (NamedKey::F12, KeyCode::F12),
    ];
    let lower = key.to_ascii_lowercase();
    let (named, code) = match lower.as_str() {
        "enter" | "return" => (NamedKey::Enter, KeyCode::Enter),
        "escape" | "esc" => (NamedKey::Escape, KeyCode::Escape),
        "tab" => (NamedKey::Tab, KeyCode::Tab),
        "insert" => (NamedKey::Insert, KeyCode::Insert),
        "delete" => (NamedKey::Delete, KeyCode::Delete),
        "home" => (NamedKey::Home, KeyCode::Home),
        "end" => (NamedKey::End, KeyCode::End),
        "pageup" => (NamedKey::PageUp, KeyCode::PageUp),
        "pagedown" => (NamedKey::PageDown, KeyCode::PageDown),
        "backspace" => (NamedKey::Backspace, KeyCode::Backspace),
        "space" => (NamedKey::Space, KeyCode::Space),
        "up" | "arrowup" => (NamedKey::ArrowUp, KeyCode::ArrowUp),
        "down" | "arrowdown" => (NamedKey::ArrowDown, KeyCode::ArrowDown),
        "left" | "arrowleft" => (NamedKey::ArrowLeft, KeyCode::ArrowLeft),
        "right" | "arrowright" => (NamedKey::ArrowRight, KeyCode::ArrowRight),
        function => match function.strip_prefix('f').and_then(|number| number.parse::<usize>().ok()) {
            Some(number @ 1..=12) => FUNCTION[number - 1],
            _ => return Err(format!("unknown key {key:?}")),
        },
    };
    let text = (named == NamedKey::Space && !held.alt_key() && !held.control_key()).then(|| " ".to_owned());
    Ok((held, KeyPress { state: ElementState::Pressed, logical_key: Key::Named(named), physical_key: PhysicalKey::Code(code), text }))
}

/// A character key's press; letters and digits carry their physical key, for shortcuts like Alt+E.
fn typed(character: char, text: bool) -> KeyPress {
    const LETTERS: [KeyCode; 26] = [
        KeyCode::KeyA,
        KeyCode::KeyB,
        KeyCode::KeyC,
        KeyCode::KeyD,
        KeyCode::KeyE,
        KeyCode::KeyF,
        KeyCode::KeyG,
        KeyCode::KeyH,
        KeyCode::KeyI,
        KeyCode::KeyJ,
        KeyCode::KeyK,
        KeyCode::KeyL,
        KeyCode::KeyM,
        KeyCode::KeyN,
        KeyCode::KeyO,
        KeyCode::KeyP,
        KeyCode::KeyQ,
        KeyCode::KeyR,
        KeyCode::KeyS,
        KeyCode::KeyT,
        KeyCode::KeyU,
        KeyCode::KeyV,
        KeyCode::KeyW,
        KeyCode::KeyX,
        KeyCode::KeyY,
        KeyCode::KeyZ,
    ];
    const DIGITS: [KeyCode; 10] = [
        KeyCode::Digit0,
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
        KeyCode::Digit8,
        KeyCode::Digit9,
    ];
    let physical_key = match character.to_ascii_lowercase() {
        letter @ 'a'..='z' => PhysicalKey::Code(LETTERS[usize::from(letter as u8 - b'a')]),
        digit @ '0'..='9' => PhysicalKey::Code(DIGITS[usize::from(digit as u8 - b'0')]),
        _ => PhysicalKey::Unidentified(NativeKeyCode::Unidentified),
    };
    KeyPress {
        state: ElementState::Pressed,
        logical_key: Key::Character(SmolStr::new(character.to_string())),
        physical_key,
        text: text.then(|| character.to_string()),
    }
}

fn screenshot(app: &mut App, event_loop: &ActiveEventLoop, crop: Option<[u32; 4]>, scale: Option<f32>) -> Result<Value, String> {
    WANT_SHOT.store(true, Ordering::Relaxed);
    app.redraw(event_loop);
    WANT_SHOT.store(false, Ordering::Relaxed);
    let mut image = SHOT.lock().unwrap().take().ok_or("no frame was drawn")?;
    if let Some([x, y, width, height]) = crop {
        image = image::imageops::crop_imm(&image, x, y, width, height).to_image();
    }
    if let Some(scale) = scale.filter(|&scale| scale > 0.0 && scale != 1.0) {
        let [width, height] = [image.width(), image.height()].map(|side| ((side as f32 * scale).round() as u32).max(1));
        // Enlarged pixels stay sharp.
        image = image::imageops::resize(&image, width, height, if scale < 1.0 { FilterType::Triangle } else { FilterType::Nearest });
    }
    let (width, height) = image.dimensions();
    let mut png = Vec::new();
    image::DynamicImage::ImageRgba8(image).into_rgb8().write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png).map_err(|error| error.to_string())?;
    Ok(json!({"ok": true, "width": width, "height": height, "png": base64(&png)}))
}

fn base64(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut text = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let bits = chunk.iter().enumerate().fold(0u32, |bits, (index, &byte)| bits | u32::from(byte) << (16 - 8 * index));
        for index in 0..4 {
            text.push(if index <= chunk.len() { char::from(DIGITS[(bits >> (18 - 6 * index) & 63) as usize]) } else { '=' });
        }
    }
    text
}

fn scene_name(scene: &Scene) -> &'static str {
    match scene {
        Scene::Connections { .. } => "connections",
        Scene::Login => "login",
        Scene::Waiting(_) => "waiting",
        Scene::ServerSelect { .. } => "server_select",
        Scene::CharSelect(_) => "char_select",
        Scene::CharMake(..) => "char_make",
        Scene::Game(_) => "game",
        Scene::Message(..) => "message",
        Scene::Confirm(..) => "confirm",
        Scene::Prompt { .. } => "prompt",
    }
}

fn state(app: &App) -> Value {
    let mut state = json!({
        "ok": true,
        "scene": scene_name(&app.scene),
        // The music file playing, with RUSTNAROK_CLIENT_SOUND=1 (a driven client is silent otherwise).
        "music": app.audio.as_ref().and_then(|audio| audio.music()),
        "server": app.login_server().0,
        "loading": app.loading.is_some(),
        "cursor": app.cursor,
        "paused": FROZEN.lock().unwrap().is_some(),
    });
    if let Scene::Message(text, _) | Scene::Confirm(text, ..) | Scene::Prompt { text, .. } = &app.scene {
        state["text"] = text.as_str().into();
    }
    let base = app.scene.base();
    if !std::ptr::eq(base, &app.scene) {
        state["under"] = scene_name(base).into();
    }
    match base {
        Scene::Login => state["login_id"] = app.fields[0].as_str().into(),
        Scene::ServerSelect { accepted, selected } => {
            state["servers"] = accepted.char_servers.iter().map(|server| server.name.as_str()).collect();
            state["selected"] = (*selected).into();
        }
        Scene::CharSelect(select) => {
            state["characters"] = select
                .characters
                .iter()
                .map(|character| json!({"slot": character.slot, "name": character.name, "job": character.job, "level": character.base_level, "map": character.map}))
                .collect();
            state["selected"] = select.selected.into();
            state["moving"] = select.moving.into();
            state["targets"] = select
                .card_boxes()
                .into_iter()
                .map(|(slot, area)| json!({"kind": "char_slot", "slot": slot, "box": area}))
                .chain(select.menu.iter().flat_map(|menu| menu.entry_boxes()).map(|(label, area)| json!({"kind": "menu_item", "label": label, "box": area})))
                .collect();
        }
        Scene::Game(game) => state["game"] = game_state(app, game),
        _ => {}
    }
    state
}

fn game_state(app: &App, game: &Game) -> Value {
    let screen = |owner: Owner| game.hit_boxes.iter().find(|(seen, ..)| *seen == owner).map(|(_, rect, _)| rect.map(|side| side.round() as i32));
    let me = game.player.cell;
    let distance = |cell: [i32; 2]| (cell[0] - me[0]).abs().max((cell[1] - me[1]).abs());
    let status = |id: u16| game.status.get(&id).copied();
    let mut units: Vec<_> = game.units.iter().collect();
    units.sort_by_key(|(_, actor)| distance(actor.cell));
    let units: Vec<Value> = units
        .into_iter()
        .map(|(&id, actor)| {
            let kind = match actor.kind {
                PLAYER => json!("player"),
                MONSTER => json!("monster"),
                NPC => json!("npc"),
                PET => json!("pet"),
                HOMUNCULUS => json!("homunculus"),
                MERCENARY => json!("mercenary"),
                other => json!(other),
            };
            let spirits = game.spirits.get(&Some(id)).copied().unwrap_or(0);
            let mut unit = json!({"id": id, "kind": kind, "name": actor.name, "job": actor.job, "cell": actor.cell, "spirits": spirits});
            if let Some(rect) = screen(Owner::Unit(id)) {
                unit["screen"] = json!(rect);
            }
            if let Some(hp) = game.unit_hp.get(&id) {
                unit["hp"] = json!(hp);
            }
            if actor.hidden {
                unit["hidden"] = true.into();
            }
            if actor.remove_at.is_some() {
                unit["dying"] = true.into();
            }
            unit
        })
        .collect();
    let ground: Vec<Value> = game
        .ground
        .iter()
        .map(|(&id, item)| {
            let name = inventory::item_name(&app.items, item.item, item.identified, 0);
            json!({"id": id, "item": item.item, "name": name, "amount": item.amount, "cell": item.at, "screen": screen(Owner::Item(id))})
        })
        .collect();
    let open = |panel: &Panel| match panel {
        Panel::Hotkeys => game.hotkeys.frame.open,
        Panel::BasicInfo => game.basic_info.frame.open,
        Panel::Inventory => game.inventory.frame.open,
        Panel::Equipment => game.equipment.frame.open,
        Panel::Skills => game.skill_window.frame.open,
        Panel::Storage => game.storage.frame.open,
        Panel::Party => game.party_window.frame.open,
        Panel::Status => game.status_window.frame.open,
        Panel::Guild => game.guild.is_some() && game.guild_window.frame.open,
        Panel::Clan => game.clan.is_some() && game.clan_window.frame.open,
        Panel::Cart => game.cart.cart.is_some() && game.cart.frame.open,
        Panel::Pet => game.pet.is_some() && game.pet_window.frame.open,
    };
    let jobs = app.skill_trees.lineage(game.me.job);
    let item_of = |index: u16| game.inventory.get(index).map_or(0, |item| item.id);
    let targets: Vec<Value> = game
        .inventory
        .item_boxes()
        .into_iter()
        .map(|(index, area)| json!({"kind": "inventory_item", "index": index, "id": item_of(index), "box": area}))
        .chain(game.hotkeys.slot_boxes().into_iter().map(|(slot, area)| json!({"kind": "hotkey", "slot": slot, "box": area})))
        .chain(game.skill_window.skill_boxes(&app.skill_trees, &jobs).into_iter().map(|(id, area)| json!({"kind": "skill", "id": id, "box": area})))
        .chain(game.pick.iter().flat_map(|(list, _)| list.row_boxes()).map(|(id, area)| json!({"kind": "pick_item", "id": id, "box": area})))
        .chain(game.menu.iter().flat_map(|menu| menu.entry_boxes()).map(|(label, area)| json!({"kind": "menu_item", "label": label, "box": area})))
        .chain(game.pet.iter().flat_map(|pet| game.pet_window.button_boxes(pet)).map(|(name, area)| json!({"kind": "button", "name": name, "box": area})))
        .chain(
            game.achievement_window
                .button_boxes(&app.achievement_info, &game.achievements)
                .into_iter()
                .chain(game.equipment.button_boxes())
                .map(|(name, area)| json!({"kind": "button", "name": name, "box": area})),
        )
        .collect();
    let mut windows: Vec<String> = game.windows.iter().filter(|panel| open(panel)).map(|panel| format!("{panel:?}")).collect();
    let overlays = [
        ("NpcDialog", game.dialog.as_ref().is_some_and(|dialog| dialog.warp.is_none())),
        ("WarpList", game.dialog.as_ref().is_some_and(|dialog| dialog.warp.is_some())),
        ("Shop", game.shop.is_some()),
        ("Refine", game.refine.is_some()),
        ("Materials", game.materials.is_some()),
        ("Achievements", game.achievement_window.frame.open),
        ("InventoryExpansion", game.expansion_asked.is_some()),
        ("PetAsked", game.pet_asked.is_some()),
        ("OtherEquipment", game.viewing.is_some()),
        ("Bank", game.bank.is_some()),
        ("PickList", game.pick.is_some()),
        ("DealChoice", game.deal_choice.is_some()),
        ("Trade", game.trade.is_some()),
        ("TradeAsked", game.trade_asked.is_some()),
        ("Amount", game.amount.is_some()),
        ("Menu", game.menu.is_some()),
        ("Escape", game.escape.is_some()),
        ("ItemInfo", game.item_window.is_some()),
        ("ChatRoom", game.room.is_some()),
        ("RoomMake", game.room_make.is_some()),
        ("EmotionList", game.emotion_list.is_some()),
        ("Macros", game.macro_window.is_some()),
        ("PartyInvite", game.party_invite.is_some()),
        ("FriendAsked", game.friend_asked.is_some()),
        ("GuildInvite", game.guild_invite.is_some()),
        ("VendSetup", game.vend_setup.is_some()),
        ("MyShop", game.my_shop.is_some()),
        ("Mailbox", game.mail.list.frame.open),
        ("MailRead", game.mail.letter.is_some()),
        ("MailWrite", game.mail.compose.is_some()),
        ("NewMail", game.mail.unread),
        ("AudioSettings", game.audio_window.is_some()),
        ("WorldMap", game.world_map.is_some()),
        ("Quests", game.quest_window.frame.open),
    ];
    windows.extend(overlays.into_iter().filter(|(_, shown)| *shown).map(|(name, _)| name.to_owned()));
    let dialog = game.dialog.as_ref().map(|dialog| {
        json!({
            "npc": dialog.npc,
            "lines": dialog.lines,
            "button": match dialog.button {
                DialogButton::Next => "next",
                DialogButton::Close => "close",
                DialogButton::None => "none",
            },
            "menu": dialog.menu.as_ref().map(|(items, selected)| json!({"items": items, "selected": selected})),
            "input": dialog.input.as_ref().map(|(text, typed)| json!({"text": text, "typed": typed})),
        })
    });
    let now = Instant::now();
    json!({
        "map": game.map,
        // The weather effect IDs showing.
        "weather": game.weather.ids(),
        "player": {
            "name": game.me.name,
            "account_id": game.account_id,
            "char_id": game.me.id,
            "job": game.me.job,
            "cell": game.player.cell,
            "spirits": game.spirits.get(&None).copied().unwrap_or(0),
            "direction": game.player.direction,
            "moving": game.player.moving(),
            "casting": game.casts.contains_key(&None),
            "hp": status(net::STATUS_HP),
            "max_hp": status(net::STATUS_MAX_HP),
            "sp": status(net::STATUS_SP),
            "max_sp": status(net::STATUS_MAX_SP),
            "base_level": status(net::STATUS_BASE_LEVEL),
            "job_level": status(net::STATUS_JOB_LEVEL),
            "zeny": status(net::STATUS_ZENY),
            "screen": screen(Owner::Player),
        },
        "windows": windows,
        // What a test can click by name instead of by pixels: inventory items (index and item), hotkey slots, skill
        // boxes, pick list rows, menu choices and window buttons (by name), each with its box on screen.
        "targets": targets,
        // What's carried: index, item, how many, identified, worn where (0: not worn).
        "inventory": game.inventory.items.iter().map(|item| json!({"index": item.index, "id": item.id, "amount": item.amount, "identified": item.identified, "worn": item.worn, "refine": item.refine, "cards": item.cards})).collect::<Vec<_>>(),
        // The hotkey bar, slot by slot: null, or what it holds (a skill's count is its level).
        "hotkeys": game.hotkeys.keys.iter().map(|key| key.map(|key| json!({"skill": key.skill, "id": key.id, "count": key.count}))).collect::<Vec<_>>(),
        "dialog": dialog,
        // Achievements: whether each is done and its reward taken; the title worn (0: none).
        "achievements": game.achievements.iter().map(|(id, achievement)| (id.to_string(), json!({"completed": achievement.completed, "rewarded": achievement.rewarded}))).collect::<serde_json::Map<_, _>>(),
        "title": game.equipment.title,
        // A taming item waiting for its monster.
        "capturing": game.capturing,
        "aimed_skill": game.targeting.map(|(id, level)| json!({"id": id, "level": level, "name": app.skill_info.get(&id).map(|info| info.name.as_str())})),
        "units": units,
        // The player's pet as the server last told it, or null.
        "pet": game.pet.as_ref().map(|pet| json!({"id": game.pet_id, "name": pet.name, "level": pet.level, "hunger": pet.hunger, "intimacy": pet.intimacy, "accessory": pet.accessory, "renamed": pet.renamed})),
        "ground_items": ground,
        "inventory_items": game.inventory.items.len(),
        "skills": game.skills.len(),
        "status_icons": game.status_icons.iter().map(|(id, _, ends)| json!({"id": id, "left_ms": ends.map(|end| end.saturating_duration_since(now).as_millis() as u64)})).collect::<Vec<_>>(),
        "chat": game.chat.lines.iter().rev().take(5).rev().map(|(text, _)| text.as_str()).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_commands() {
        let parse = |line: &str| serde_json::from_str::<Command>(line);
        assert_eq!(parse(r#"{"cmd":"wheel","x":390,"y":360,"rows":-2}"#).unwrap(), Command::Wheel { x: 390.0, y: 360.0, rows: -2 });
        assert_eq!(
            parse(r#"{"cmd":"click","x":512,"y":300.5,"button":"right"}"#).unwrap(),
            Command::Click { x: 512.0, y: 300.5, button: Button::Right, mods: String::new(), count: None }
        );
        assert_eq!(
            parse(r#"{"cmd":"wait","packet":"0x08c8","pause":true,"do":{"cmd":"key","key":"F1"}}"#).unwrap(),
            Command::Wait { ms: None, packet: Some(0x08c8), event: None, fresh: false, pause: true, first: Some(Box::new(Command::Key { key: "F1".into() })) }
        );
        assert!(matches!(parse(r#"{"cmd":"wait","packet":2248,"ms":500}"#), Ok(Command::Wait { packet: Some(0x08c8), ms: Some(500), .. })));
        assert_eq!(parse(r#"{"cmd":"clock","action":"step","ms":50}"#).unwrap(), Command::Clock { action: ClockAction::Step, ms: Some(50) });
        assert_eq!(parse(r#"{"cmd":"state"}"#).unwrap(), Command::State);
        // A missing coordinate, a misspelt field, an unknown command, a bad packet ID.
        for wrong in [r#"{"cmd":"click","x":1}"#, r#"{"cmd":"click","x":1,"y":2,"buton":"left"}"#, r#"{"cmd":"fly"}"#, r#"{"cmd":"wait","packet":"zz"}"#] {
            assert!(parse(wrong).is_err(), "{wrong}");
        }

        let (held, alt_e) = parse_key("Alt+E").unwrap();
        assert_eq!((held, alt_e.physical_key, alt_e.text), (ModifiersState::ALT, PhysicalKey::Code(KeyCode::KeyE), None));
        assert_eq!(parse_key("f12").unwrap().1.logical_key, Key::Named(NamedKey::F12));
        assert_eq!(parse_key("a").unwrap().1.text.as_deref(), Some("a"));
        assert_eq!(parse_key("+").unwrap().1.text.as_deref(), Some("+"));
        assert!(parse_key("Hyper+Q").is_err() && parse_key("F13").is_err());

        assert_eq!([base64(b"Man"), base64(b"Ma"), base64(b"M")], ["TWFu", "TWE=", "TQ=="]);
    }
}
