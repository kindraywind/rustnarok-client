//! Dev-only remote control, for test automation: with `RUSTNAROK_CLIENT_CONTROL=<path>` the client listens on that Unix socket
//! (owner only, no TCP port; on Windows, a localhost port that `path` names with a token, see `bind`) for
//! newline-delimited JSON commands and answers each with one line of JSON
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
//!   to: [x, y], button?, mods?, held?}` (`held`: the button stays down until `release` `{button?}`): the pointer and
//!   buttons, through the same code as the mouse.
//! - `key` `{key, repeat?, hold?}`: "Enter", "Escape", "Tab", "Backspace", "Space", "Insert", "Delete", "Home", "End",
//!   "PageUp", "PageDown", "Up"/"Down"/"Left"/"Right", "F1"–"F12", "PrintScreen", "ScrollLock" or one
//!   character, after modifiers like "Alt+" or "Ctrl+". `repeat` is an auto-repeat; shortcuts ignore it. `hold` is
//!   milliseconds to keep the key down, auto-repeating after the first press.
//! - `type` `{text}`: into the focused field; non-ASCII runs (Hangul) come in as the input method's commit.
//! - `say` `{text}`: a chat line or a GM command.
//! - `state`: the scene and the login server; at character select the characters, selected slot and `pin` ("ask",
//!   "new", "change" or "none"); in the game the map, the player, open windows, the NPC dialog, the aimed
//!   skill, units and ground items (with their sprites' screen boxes and the areas a click picks them in), counts and
//!   status icons. Each unit's `tint` is `[r, g, b]` while a status colours it, or null. Everything comes from what
//!   the client itself knows, whatever server it plays on.
//!   `player` has `status_points` and `skill_points` left, `stats` (`str` to `luk`: `{value, bonus, cost}`, cost being
//!   the points one more takes) and `skills` (`[{id, name, level, max_level, upgradable}]`, level 0: not learned yet).
//!   While the status window (Alt+A) is open, the arrows that raise a stat are targets named `raise_str` to
//!   `raise_luk` (only those the points pay for); while the skill window (Alt+S) is open, its job tabs are
//!   `skill_tab:<job id>` and the open tab's level up buttons `raise:<skill id>` (only with points left). Both only
//!   while a click reaches them: under another window they're left out. After `@reset`, a build is these clicks by
//!   name.
//!   `homunculus` is `{id, name, level, hunger, intimacy, hp, skill_points, vaporized, following}` or null; the skill
//!   list's level up buttons are targets named `raise:<skill id>`. `player.look` is the real look (hair, hair color,
//!   clothes color, head top, mid and bottom). The stylist's buttons are targets (`hair_style_<row>`, `hair_color_<row>`,
//!   `clothes_color_<row>`, `tab_hair`, `tab_accessory`, `buy`, `refresh`, `rotate_left`, `rotate_right`, `close`).
//! - `wait` `{ms?, packet?, event?, fresh?, pause?, do?}`: sleeps `ms`; with a `packet` id ("0x08c8") or an `event`
//!   (a `MapEvent`/`CharEvent` variant like "Act", "LoginAccepted"/"LoginRefused", or "Sound <file>" for an effect
//!   sound played, e.g. "Sound _hit_sword.wav"), waits up to `ms` (10 s by
//!   default) for one seen since the previous command, so a quick answer isn't missed between two calls. `fresh`
//!   only counts new ones; `do` is a command to run first (and implies `fresh`); `pause` stops the clock on arrival.
//! - `clock` `{action: "pause"|"resume"|"step", ms?}`: the animation clock; a step freezes it `ms` (16) further on.
//! - `trace` `{last?, events?}`: the last packets received (20), as `RUSTNAROK_CLIENT_TRACE` prints them; with `events`, the
//!   events they made in between (`==` lines, with what they say: a disconnection's reason, say).
//! - `quit`.

use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Cursor, Write};
#[cfg(windows)]
use std::net::{TcpListener as Listener, TcpStream as Stream};
#[cfg(unix)]
use std::os::unix::net::{UnixListener as Listener, UnixStream as Stream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Condvar, Mutex, mpsc};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use image::RgbaImage;
use image::imageops::FilterType;
use rustnarok_client::lua::SkillInfo;
use rustnarok_client::net;
use serde::Deserialize;
use serde_json::{Value, json};
use winit::event::{ElementState, KeyEvent};
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::keyboard::{Key, KeyCode, ModifiersState, NamedKey, NativeKeyCode, PhysicalKey, SmolStr};

use crate::actor::Look;
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
        /// Leave the button down at `to` (let go with `release`), to look at something mid-drag.
        #[serde(default)]
        held: bool,
    },
    /// Lets go of a button held by a `drag` with `held`.
    Release {
        #[serde(default)]
        button: Button,
    },
    Key {
        key: String,
        /// An auto-repeat of a held key. Shortcuts ignore it; typing still takes it.
        #[serde(default)]
        repeat: bool,
        /// Hold the key this many milliseconds, sending auto-repeats. One command, so a following `wait` still sees what the first press caused.
        #[serde(default)]
        hold: Option<u64>,
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
    /// A button goes down at a point and stays down until `mouse_up`.
    MouseDown {
        x: f32,
        y: f32,
        #[serde(default)]
        button: Button,
    },
    MouseUp {
        x: f32,
        y: f32,
        #[serde(default)]
        button: Button,
    },
    /// Where a map cell's middle is on screen: `{"point": [x, y]}`.
    CellPoint {
        x: i32,
        y: i32,
    },
    State,
    Wait {
        ms: Option<u64>,
        #[serde(default, deserialize_with = "packet_id")]
        packet: Option<u16>,
        event: Option<String>,
        /// A packet the client sends, by ID.
        #[serde(default, deserialize_with = "packet_id")]
        sent: Option<u16>,
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
#[derive(Clone)]
pub struct KeyPress {
    pub state: ElementState,
    pub logical_key: Key,
    pub physical_key: PhysicalKey,
    pub text: Option<String>,
    /// An auto-repeated key-down. Shortcuts and Esc ignore it; typing still takes it.
    pub repeat: bool,
}

impl From<&KeyEvent> for KeyPress {
    fn from(event: &KeyEvent) -> Self {
        Self {
            state: event.state,
            logical_key: event.logical_key.clone(),
            physical_key: event.physical_key,
            text: event.text.as_deref().map(str::to_owned),
            repeat: event.repeat,
        }
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
    /// A packet the client sent the map server.
    Sent(u16),
    Event(String),
}

const LOG_SIZE: usize = 2000;
static LOG: Mutex<Log> = Mutex::new(Log { next: 0, entries: VecDeque::new() });
static SEEN: Condvar = Condvar::new();
/// Effect sounds played lately, with when (for `sounds_recent`).
static RECENT_SOUNDS: Mutex<VecDeque<(Instant, String)>> = Mutex::new(VecDeque::new());
/// How long a played sound stays in `sounds_recent`.
const RECENT: Duration = Duration::from_secs(5);
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
    let path = Path::new(&path);
    let (listener, token) = bind(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let _ = net::PACKET_HOOK.set(packet_seen);
    let _ = net::SENT_HOOK.set(packet_sent);
    ON.store(true, Ordering::Relaxed);
    eprintln!("control socket at {}", path.display());
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let (proxy, token) = (proxy.clone(), token.clone());
            std::thread::spawn(move || serve(stream, &proxy, token.as_deref()));
        }
    });
}

/// Whoever can connect plays the logged-in character, so only the owner may: through a Unix socket only they can
/// open...
#[cfg(unix)]
fn bind(path: &Path) -> std::io::Result<(Listener, Option<String>)> {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::remove_file(path);
    let listener = Listener::bind(path)?;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    Ok((listener, None))
}

/// ...or, on Windows (no Unix sockets in Python there), a localhost port written to `path` as "127.0.0.1:port token": a
/// connection's first line must be the token, which only those who can read `path` know and a web page's request to
/// localhost can't send.
#[cfg(windows)]
fn bind(path: &Path) -> std::io::Result<(Listener, Option<String>)> {
    use std::hash::BuildHasher;
    let listener = Listener::bind("127.0.0.1:0")?;
    // SipHash under the keys std seeds from the OS's random source.
    let random = std::collections::hash_map::RandomState::new();
    let token = format!("{:016x}{:016x}", random.hash_one(1), random.hash_one(2));
    std::fs::write(path, format!("{} {token}", listener.local_addr()?))?;
    Ok((listener, Some(token)))
}

/// Answers one connection's commands in order. `wait` and `trace` are answered here, the rest on the event loop.
fn serve(stream: Stream, proxy: &EventLoopProxy<NetEvent>, token: Option<&str>) {
    let Ok(mut writer) = stream.try_clone() else { return };
    let mut lines = BufReader::new(stream).lines();
    if let Some(token) = token
        && lines.next().and_then(Result::ok).as_deref() != Some(token)
    {
        return;
    }
    for line in lines {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Command>(&line) {
            Ok(Command::Wait { ms, packet, event, sent, fresh, pause, first }) => wait(proxy, ms, (packet, event, sent), fresh, pause, first),
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
    (packet, event, sent): (Option<u16>, Option<String>, Option<u16>),
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
    if packet.is_none() && event.is_none() && sent.is_none() {
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
            Seen::Sent(id) => sent == Some(*id),
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
    let lines: Vec<&str> =
        log.entries.iter().filter(|entry| events || matches!(entry.seen, Seen::Packet(_) | Seen::Sent(_))).map(|entry| entry.text.as_str()).collect();
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

/// Every packet the client writes to the map server, as `->` lines.
fn packet_sent(packet: &[u8]) {
    let id = u16::from_le_bytes([packet[0], packet[1]]);
    record(Seen::Sent(id), format!("{:05} -> {id:#06x} {} {:02x?}", trace_ms(), packet.len(), &packet[..packet.len().min(32)]));
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

/// Notes an effect sound played (`wait` event `Sound <file>`, the file under data\wav\ as asked for).
pub fn sound_played(name: &str) {
    if driven() {
        record(Seen::Event(format!("Sound {name}")), format!("{:05} == Sound {name}", trace_ms()));
        RECENT_SOUNDS.lock().unwrap().push_back((now(), name.to_owned()));
    }
}

/// The effect sounds played in the last five seconds, oldest first.
fn recent_sounds() -> Vec<String> {
    let mut recent = RECENT_SOUNDS.lock().unwrap();
    let now = now();
    while recent.front().is_some_and(|(at, _)| now.saturating_duration_since(*at) > RECENT) {
        recent.pop_front();
    }
    recent.iter().map(|(_, name)| name.clone()).collect()
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
pub fn on_draw(gpu: &Gpu, world: Option<&dyn WorldPass>, sprites: &[Sprite], alphas: &[f32]) {
    if WANT_SHOT.swap(false, Ordering::Relaxed) {
        *SHOT.lock().unwrap() = Some(gpu.snapshot(world, sprites, alphas));
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
        Command::MouseDown { x, y, button } | Command::MouseUp { x, y, button } => {
            let state = if matches!(command, Command::MouseDown { .. }) { ElementState::Pressed } else { ElementState::Released };
            point(app, [x, y]);
            press(app, event_loop, button, state);
            Ok(Value::Null)
        }
        Command::CellPoint { x, y } => {
            let Scene::Game(game) = &app.scene else { return Err("not in the game".into()) };
            let view = game.camera.view(crate::screen()[0] / crate::screen()[1]);
            let point = view.project(game.world.cell_position([x as f32, y as f32])).ok_or("the cell is behind the camera")?;
            Ok(json!({"ok": true, "point": [point[0] * crate::screen()[0], point[1] * crate::screen()[1]]}))
        }
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
        Command::Drag { from, to, button, mods, held: keep_down } => {
            let held = modifiers(&mods)?;
            hold(app, held, |app| {
                point(app, from);
                press(app, event_loop, button, ElementState::Pressed);
                point(app, to);
                if !keep_down {
                    press(app, event_loop, button, ElementState::Released);
                }
            });
            Ok(Value::Null)
        }
        Command::Release { button } => {
            press(app, event_loop, button, ElementState::Released);
            Ok(Value::Null)
        }
        Command::Wheel { x, y, rows } => {
            point(app, [x, y]);
            app.wheel(rows);
            Ok(Value::Null)
        }
        Command::Key { key, repeat, hold: held_ms } => {
            let (mods, mut key) = parse_key(&key)?;
            key.repeat = repeat;
            hold(app, mods, |app| {
                app.key(&key);
                // Let go as a keyboard does, before the modifiers (Print Screen acts on its release).
                let release = KeyPress { state: ElementState::Released, repeat: false, ..key.clone() };
                let Some(ms) = held_ms.filter(|ms| *ms > 0) else {
                    app.key(&release);
                    return;
                };
                // Auto-repeats while the key stays down. Shortcuts fire on the first press only.
                key.repeat = true;
                let end = Instant::now() + Duration::from_millis(ms);
                while Instant::now() < end {
                    std::thread::sleep(Duration::from_millis(50));
                    app.key(&key);
                }
                app.key(&release);
            });
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
        "printscreen" | "print" | "snapshot" => (NamedKey::PrintScreen, KeyCode::PrintScreen),
        "scrolllock" => (NamedKey::ScrollLock, KeyCode::ScrollLock),
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
    Ok((held, KeyPress { state: ElementState::Pressed, logical_key: Key::Named(named), physical_key: PhysicalKey::Code(code), text, repeat: false }))
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
        repeat: false,
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

fn state(app: &mut App) -> Value {
    if let Scene::Game(game) = &mut app.scene {
        game.sync_esc();
    }
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
            state["pin"] = select.pin_pad.as_ref().map_or("none", |pad| pad.kind()).into();
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
    let target = |owner: Owner| game.hit_boxes.iter().find(|target| target.owner == owner);
    let screen = |owner: Owner| target(owner).map(|target| target.sprite.map(|side| side.round() as i32));
    let area = |owner: Owner| target(owner).map(|target| target.area.map(|side| side.round() as i32));
    let me = game.player.cell;
    let distance = |cell: [i32; 2]| (cell[0] - me[0]).abs().max((cell[1] - me[1]).abs());
    let status = |id: u16| game.status.get(&id).copied();
    // What circles a unit: spheres (or coins), shields, charms and their element ("water", "earth", "fire", "wind"
    // or null), souls.
    let orbs = |value: &mut Value, unit: Option<u32>| {
        let orbs = game.spirits.get(&unit).copied().unwrap_or_default();
        let (element, charms) = orbs.charms;
        value["spirits"] = orbs.spheres.into();
        value["shields"] = orbs.shields.into();
        value["charms"] = charms.into();
        value["charm"] = element.checked_sub(1).and_then(|element| crate::CHARM_NAMES.get(usize::from(element))).copied().into();
        value["souls"] = orbs.souls.into();
    };
    // The action a unit plays now, and its body's frame (null without a sprite), on the drawing's clock.
    let animation = |actor: &crate::actor::Actor| {
        let (action, timing) = actor.animation(crate::control::now());
        let frame =
            actor.sprites.as_ref().and_then(|sprites| sprites.body_frame(action * 8 + usize::from(actor.direction % 8), timing)).map(|(frame, _)| frame);
        (action, frame)
    };
    // A combo window's milliseconds left, on the drawing's clock (0: none).
    let combo_ms = |unit: Option<u32>| {
        game.combos.get(&unit).map_or(0, |(start, length)| length.saturating_sub(crate::control::now().saturating_duration_since(*start)).as_millis() as u64)
    };
    // The message box's OK and Cancel while a question is up.
    let question = game.guild_asked.is_some()
        || game.guild_invite.is_some()
        || game.cashshop_buy_asked
        || game.save_asked
        || game.trade_asked.is_some()
        || game.booking_asked.is_some()
        || game.adopt_asked.is_some()
        || game.queue_asked.is_some();
    let question_buttons = app.assets.as_ref().filter(|_| question).into_iter().flat_map(|assets| {
        let dialog = crate::dialog_position(&assets.message);
        [("ok", crate::LEFT_BUTTON, &assets.ok), ("cancel", crate::RIGHT_BUTTON, &assets.cancel)].map(|(name, place, button)| {
            let [x, y] = crate::offset(dialog, place);
            json!({"kind": "button", "name": name, "box": [x, y, button[0].size[0], button[0].size[1]]})
        })
    });
    // Hidden players are left out: the client doesn't show them.
    let mut units: Vec<_> = game.units.iter().filter(|(_, actor)| crate::actor::seen(actor.hide_option, false) == crate::actor::Seen::Whole).collect();
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
            let mut unit = json!({"id": id, "kind": kind, "name": actor.name, "job": actor.job, "cell": actor.cell, "position": actor.position(now()), "combo_ms": combo_ms(Some(id)), "held": crate::locked_with(&game.locks, id), "cart": cart_state(actor.cart.as_ref())});
            let (action, frame) = animation(actor);
            unit["action"] = action.into();
            unit["frame"] = frame.into();
            // Copies left behind it while a status blurs it (the quickens).
            unit["ghosts"] = actor.ghosts.len().into();
            orbs(&mut unit, Some(id));
            if let Some(rect) = screen(Owner::Unit(id)) {
                unit["screen"] = json!(rect);
                unit["area"] = json!(area(Owner::Unit(id)));
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
            if actor.dead {
                unit["dead"] = true.into();
            }
            if actor.sitting {
                unit["sitting"] = true.into();
            }
            // How opaque it's drawn, 0 to 255 (fading in after a spawn, out when leaving).
            unit["alpha"] = (actor.alpha(now()) * 255.0).round().into();
            unit["tint"] = json!(crate::actor::status_tint(actor.body_state, actor.health_state));
            unit["marks"] = json!(crate::head_marks::marks(actor.body_state, actor.health_state).iter().map(|mark| format!("{mark:?}")).collect::<Vec<_>>());
            unit
        })
        .collect();
    let ground: Vec<Value> = game
        .ground
        .iter()
        .map(|(&id, item)| {
            let name = inventory::item_name(&app.items, item.item, item.identified, 0);
            json!({"id": id, "item": item.item, "name": name, "amount": item.amount, "cell": item.at, "screen": screen(Owner::Item(id)), "area": area(Owner::Item(id))})
        })
        .collect();
    let jobs = crate::skills::tabs(&app.skill_trees, game.me.job, &game.skills);
    let sheet = crate::skills::Sheet { trees: &app.skill_trees, jobs: &jobs, skills: &game.skills, info: &game.skill_info, points: game.skill_points() };
    let item_of = |index: u16| game.inventory.get(index).map_or(0, |item| item.id);
    // The leader question's OK and Cancel, in the message box.
    let leader_question: Vec<(&str, [f32; 4])> = match (&app.assets, game.leader_asked.is_some() || game.apply_asked) {
        (Some(assets), true) => {
            let dialog = crate::dialog_position(&assets.message);
            [("ok", crate::LEFT_BUTTON, &assets.ok), ("cancel", crate::RIGHT_BUTTON, &assets.cancel)]
                .map(|(name, place, images)| (name, [dialog[0] + place[0], dialog[1] + place[1], images[0].size[0], images[0].size[1]]))
                .to_vec()
        }
        _ => Vec::new(),
    };
    // Whether a click in the middle of a box reaches that window, not another one over it.
    let on_top = |panel: Panel, area: &[f32; 4]| game.window_at([area[0] + area[2] / 2.0, area[1] + area[3] / 2.0]) == Some(panel);
    let targets: Vec<Value> = game
        .inventory
        .item_boxes()
        .into_iter()
        .map(|(index, area)| json!({"kind": "inventory_item", "index": index, "id": item_of(index), "box": area}))
        .chain(game.inventory.check_boxes().into_iter().map(|(name, area)| json!({"kind": "button", "name": name, "box": area})))
        .chain(
            game.item_window.iter().flat_map(|window| window.button_boxes(&app.items)).map(|(name, area)| json!({"kind": "button", "name": name, "box": area})),
        )
        .chain(game.hotkeys.slot_boxes().into_iter().map(|(slot, area)| json!({"kind": "hotkey", "slot": slot, "box": area})))
        .chain(game.skill_window.skill_boxes(&sheet).into_iter().map(|(id, area)| json!({"kind": "skill", "id": id, "box": area})))
        .chain(
            game.skill_window
                .tab_boxes(&sheet)
                .into_iter()
                .map(|(job, area)| (format!("skill_tab:{job}"), area))
                .chain(game.skill_window.arrow_boxes(&sheet))
                .chain(game.skill_window.up_boxes(&sheet))
                .chain(game.skill_window.view_box().map(|area| ("skill_view".to_owned(), area)))
                .chain(game.skill_window.button_boxes().into_iter().map(|(name, area)| (name.to_owned(), area)))
                .filter(|(_, area)| on_top(Panel::Skills, area))
                .chain(game.status_window.raise_boxes(&game.status).into_iter().filter(|(_, area)| on_top(Panel::Status, area)))
                .chain(
                    app.assets
                        .as_ref()
                        .map(|assets| game.status_window.trait_boxes(&assets.status, &game.status))
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|(_, area)| on_top(Panel::Status, area)),
                )
                .map(|(name, area)| json!({"kind": "button", "name": name, "box": area})),
        )
        .chain(game.basic_info.menu_boxes().into_iter().map(|(label, area)| json!({"kind": "menu_item", "label": label, "box": area})))
        .chain(game.basic_info.toggle_box().map(|area| json!({"kind": "button", "name": "menu", "box": area})))
        .chain(
            app.assets
                .as_ref()
                .and_then(|assets| game.cutin.as_ref()?.close_box(&assets.cutin))
                .map(|area| json!({"kind": "button", "name": "cutin_close", "box": area})),
        )
        .chain(
            game.queue_window
                .frame
                .open
                .then(|| game.queue_window.button_boxes(&app.battlegrounds, game.queued.is_some()))
                .into_iter()
                .flatten()
                .map(|(name, area)| json!({"kind": "button", "name": name, "box": area})),
        )
        .chain(game.cashshop.iter().flat_map(crate::cashshop::CashShop::item_boxes).map(|(id, area)| json!({"kind": "pick_item", "id": id, "box": area})))
        .chain(game.cashshop.iter().flat_map(crate::cashshop::CashShop::button_boxes).map(|(name, area)| json!({"kind": "button", "name": name, "box": area})))
        .chain(
            game.enchant
                .iter()
                .flat_map(|window| {
                    let index = window.item;
                    let item = index.and_then(|index| game.inventory.items.iter().find(|item| item.index == index));
                    window.row_boxes(item)
                })
                .map(|(id, area)| json!({"kind": "pick_item", "id": id, "box": area})),
        )
        .chain(game.enchant.iter().flat_map(|window| window.button_boxes()).map(|(name, area)| json!({"kind": "button", "name": name, "box": area})))
        .chain(game.pick.iter().flat_map(|(list, _)| list.row_boxes()).map(|(id, area)| json!({"kind": "pick_item", "id": id, "box": area})))
        .chain(game.pick.iter().flat_map(|(list, _)| list.button_boxes()).map(|(name, area)| json!({"kind": "button", "name": name, "box": area})))
        .chain(game.menu.iter().flat_map(|menu| menu.entry_boxes()).map(|(label, area)| json!({"kind": "menu_item", "label": label, "box": area})))
        .chain(game.pet.iter().flat_map(|pet| game.pet_window.button_boxes(pet)).map(|(name, area)| json!({"kind": "button", "name": name, "box": area})))
        .chain(
            game.buying_setup
                .iter()
                .flat_map(|setup| setup.row_boxes(&game.inventory.items))
                .map(|(id, area)| json!({"kind": "pick_item", "id": id, "box": area})),
        )
        .chain(game.shop.iter().flat_map(crate::shop::Shop::row_boxes).map(|(id, area)| json!({"kind": "pick_item", "id": id, "box": area})))
        .chain(game.storage.grip_box().map(|area| json!({"kind": "button", "name": "storage_resize", "box": area})))
        .chain(
            game.booking
                .iter()
                .flat_map(crate::booking_window::BookingWindow::button_boxes)
                .map(|(name, area)| json!({"kind": "button", "name": name, "box": area})),
        )
        .chain(game.skill_window.grip_box().map(|area| json!({"kind": "button", "name": "skills_resize", "box": area})))
        .chain(
            game.vend_setup
                .iter()
                .map(|setup| setup.grip_box())
                .chain(game.my_shop.iter().map(|shop| shop.grip_box()))
                .map(|area| json!({"kind": "button", "name": "vend_resize", "box": area})),
        )
        .chain(
            game.buying_setup
                .iter()
                .flat_map(crate::buying::Setup::button_boxes)
                .chain(game.my_buying_store.iter().flat_map(crate::buying::MyStore::button_boxes))
                .chain(game.shop.iter().flat_map(crate::shop::Shop::button_boxes))
                .map(|(name, area)| json!({"kind": "button", "name": name, "box": area})),
        )
        .chain(game.sign_rects.iter().map(|&(rect, _, kind)| {
            let name = match kind {
                crate::chatroom::BUYING_STORE => "buying_sign",
                crate::chatroom::SHOP => "shop_sign",
                _ => "room_sign",
            };
            json!({"kind": "button", "name": name, "box": [rect[0], rect[1], rect[2] - rect[0], rect[3] - rect[1]]})
        }))
        .chain(game.dialog_links.iter().chain(&game.chat_links).map(|(area, target)| match target {
            crate::links::Target::Item(item) => json!({"kind": "item_link", "id": item.id, "box": area}),
            crate::links::Target::Navi { map, .. } => json!({"kind": "navi_link", "map": map, "box": area}),
            crate::links::Target::Url(address) => json!({"kind": "url_link", "address": address, "box": area}),
            crate::links::Target::Tip => json!({"kind": "tip_link", "box": area}),
        }))
        .chain(
            game.navi_window.iter().flat_map(crate::navigation::Window::button_boxes).map(|(name, area)| json!({"kind": "button", "name": name, "box": area})),
        )
        .chain(game.chat.settings.iter().flat_map(crate::chat::Settings::button_boxes).map(|(name, area)| json!({"kind": "button", "name": name, "box": area})))
        .chain(game.chat.button_boxes().into_iter().map(|(name, area)| json!({"kind": "button", "name": name, "box": area})))
        .chain(game.room.iter().flat_map(|room| room.button_boxes(&game.me.name)).map(|(name, area)| json!({"kind": "button", "name": name, "box": area})))
        .chain(game.quest_window.button_boxes(&game.quests).into_iter().map(|(name, area)| json!({"kind": "button", "name": name, "box": area})))
        .chain(std::iter::once(json!({"kind": "button", "name": "roulette", "box": crate::roulette::icon_box()})))
        .chain(game.roulette.iter().flat_map(|window| window.button_boxes()).map(|(name, area)| json!({"kind": "button", "name": name, "box": area})))
        .chain(game.grade.iter().flat_map(|window| window.button_boxes()).map(|(name, area)| json!({"kind": "button", "name": name, "box": area})))
        .chain(game.synthesis.iter().flat_map(|window| window.button_boxes()).map(|(name, area)| json!({"kind": "button", "name": name, "box": area})))
        .chain(game.upgrade.iter().flat_map(|window| window.button_boxes()).map(|(name, area)| json!({"kind": "button", "name": name, "box": area})))
        .chain(
            game.reform.iter().flat_map(|window| window.row_boxes(&game.inventory.items)).map(|(id, area)| json!({"kind": "pick_item", "id": id, "box": area})),
        )
        .chain(game.reform.iter().flat_map(|window| window.button_boxes()).map(|(name, area)| json!({"kind": "button", "name": name, "box": area})))
        .chain(
            game.achievement_window
                .button_boxes(&app.achievement_info, &game.achievements)
                .into_iter()
                .chain(game.equipment.button_boxes())
                .map(|(name, area)| json!({"kind": "button", "name": name, "box": area})),
        )
        .chain(
            game.party_window
                .button_boxes(game.party.as_ref(), &game.friends, game.account_id)
                .into_iter()
                .chain(game.member_windows.button_boxes(game.party.as_ref()))
                .chain(game.party_window.compact.then(|| crate::party::compact_boxes(game.party.as_ref())).into_iter().flatten())
                .map(|(name, area)| json!({"kind": "button", "name": name, "box": area})),
        )
        .chain(game.inventory.tab_boxes().into_iter().map(|(name, area)| json!({"kind": "button", "name": name, "box": area})))
        .chain(game.catalog.iter().flat_map(|catalog| catalog.button_boxes()).map(|(name, area)| json!({"kind": "button", "name": name, "box": area})))
        .chain(game.catalog.iter().flat_map(|catalog| catalog.row_boxes()).map(|(id, area)| json!({"kind": "pick_item", "id": id, "box": area})))
        .chain(game.ranking.iter().flat_map(|board| board.button_boxes()).map(|(name, area)| json!({"kind": "button", "name": name, "box": area})))
        .chain(game.dressroom.iter().flat_map(|room| room.button_boxes()).map(|(name, area)| json!({"kind": "button", "name": name, "box": area})))
        .chain(game.party_setup.iter().flat_map(|setup| setup.button_boxes()).map(|(name, area)| json!({"kind": "button", "name": name, "box": area})))
        .chain(game.talks.button_boxes().into_iter().map(|(name, area)| json!({"kind": "button", "name": name, "box": area})))
        .chain(game.friend_setup.iter().flat_map(|setup| setup.button_boxes()).map(|(name, area)| json!({"kind": "button", "name": name, "box": area})))
        .chain(game.block_list.iter().flat_map(|list| list.button_boxes()).map(|(name, area)| json!({"kind": "button", "name": name, "box": area})))
        .chain(game.away_prompt.iter().flat_map(|prompt| prompt.button_boxes()).map(|(name, area)| json!({"kind": "button", "name": name, "box": area})))
        .chain(game.chat.list_boxes().into_iter().map(|(name, area)| json!({"kind": "button", "name": name, "box": area})))
        .chain(leader_question.into_iter().map(|(name, area)| json!({"kind": "button", "name": name, "box": area})))
        .chain(
            game.mercenary.iter().flat_map(|_| game.mercenary_window.button_boxes()).map(|(name, area)| json!({"kind": "button", "name": name, "box": area})),
        )
        .chain(
            game.guild
                .iter()
                .flat_map(|guild| game.guild_window.relation_boxes(guild))
                .map(|(name, area)| json!({"kind": "button", "name": name, "box": area})),
        )
        .chain(
            game.guild.iter().flat_map(|guild| game.guild_window.button_boxes(guild)).map(|(name, area)| json!({"kind": "button", "name": name, "box": area})),
        )
        .chain(game.trade.iter().flat_map(|trade| trade.button_boxes()).map(|(name, area)| json!({"kind": "button", "name": name, "box": area})))
        .chain(question_buttons)
        .chain(["plus", "minus", "mini"].map(|name| json!({"kind": "button", "name": format!("minimap_{name}"), "box": crate::minimap::button_box(name)})))
        .chain(game.minimap_window.button_boxes().into_iter().map(|(name, area)| json!({"kind": "button", "name": name, "box": area})))
        .chain(game.hotkeys.button_boxes().into_iter().map(|(name, area)| json!({"kind": "button", "name": name, "box": area})))
        .chain(game.level_ups.button_boxes().into_iter().map(|(name, area)| json!({"kind": "button", "name": name, "box": area})))
        .chain(
            game.skill_description
                .iter()
                .filter_map(|window| window.close_box())
                .map(|area| json!({"kind": "button", "name": "skill_description_close", "box": area})),
        )
        .chain(game.skill_description.iter().filter_map(|window| window.delay_box()).map(|area| json!({"kind": "button", "name": "skill_delay", "box": area})))
        .chain(
            game.mercenary
                .iter()
                .flat_map(|_| game.mercenary_skill_list.skill_boxes(&game.mercenary_skills))
                .map(|(id, area)| json!({"kind": "skill", "id": id, "box": area})),
        )
        .chain(
            game.homunculus
                .iter()
                .flat_map(|homunculus| game.homunculus_window.button_boxes(homunculus))
                .map(|(name, area)| json!({"kind": "button", "name": name, "box": area})),
        )
        .chain(
            game.homunculus
                .iter()
                .flat_map(|_| game.homunculus_skill_list.skill_boxes(&game.homunculus_skills))
                .map(|(id, area)| json!({"kind": "skill", "id": id, "box": area})),
        )
        .chain(
            game.homunculus
                .iter()
                .flat_map(|homunculus| game.homunculus_skill_list.raise_boxes(&game.homunculus_skills, homunculus.skill_points))
                .map(|(id, area)| json!({"kind": "button", "name": format!("raise:{id}"), "box": area})),
        )
        .chain(
            game.stylist
                .iter()
                .flat_map(|window| {
                    let job = game.player.look.as_ref().map(|look| look.job).unwrap_or(game.me.job);
                    window.button_boxes(&app.styling_shop, job)
                })
                .map(|(name, area)| json!({"kind": "button", "name": name, "box": area})),
        )
        .chain(
            game.escape
                .iter()
                .flat_map(|menu| menu.button_boxes(game.can_resurrect()))
                .map(|(name, area)| json!({"kind": "button", "name": name, "box": area})),
        )
        .collect();
    // Newest first: the order Esc closes them in.
    // The windows open: those Esc closes, in its order, then the ones it leaves up.
    let mut windows = game.esc_names();
    windows.extend(
        [("Hotkeys", game.hotkeys.frame.open), ("BasicInfo", game.basic_info.frame.open), ("Shop", game.shop.as_ref().is_some_and(crate::shop::Shop::plain))]
            .into_iter()
            .filter(|(_, open)| *open)
            .map(|(name, _)| name.to_owned()),
    );
    windows.extend(game.member_windows.names(game.party.as_ref()));
    if game.party_window.compact && game.party.is_some() {
        windows.push("PartyList".to_owned());
    }
    let dialog = game.dialog.as_ref().map(|dialog| {
        json!({
            "npc": dialog.npc,
            "talking": dialog.talking,
            "lines": dialog.lines,
            "button": match dialog.button {
                DialogButton::Next => "next",
                DialogButton::Close => "close",
                DialogButton::None => "none",
            },
            "menu": dialog.menu.as_ref().map(|menu| json!({"items": menu.items, "selected": menu.selected, "first": menu.first, "position": game.npc_menu.position})),
            "input": dialog.input.as_ref().map(|input| json!({"text": input.text, "typed": input.typed})),
            "position": game.say.frame.position,
            "scroll": game.say.scroll,
        })
    });
    // The NPC's picture: its window's corner and height, its type and file.
    let cutin_window = game.cutin.as_ref().map(|cutin| json!({"at": cutin.frame.position, "height": cutin.height(), "kind": cutin.kind, "name": cutin.name}));
    let now = Instant::now();
    let mut state = json!({
        "map": game.map,
        // The weather effect IDs showing.
        "weather": game.weather.ids(),
        // An NPC's picture is up.
        "cutin": game.cutin.is_some(),
        "player": {
            "name": game.me.name,
            "account_id": game.account_id,
            "char_id": game.me.id,
            "job": game.me.job,
            "cell": game.player.cell,
            // Where it's drawn, in cells: between two while walking.
            "position": game.player.position(self::now()),
            "direction": game.player.direction,
            // The head against the body: 0 straight, 1 and 2 turned to either side.
            "head_direction": game.player.head_direction,
            "moving": game.player.moving(),
            // The falcon (25 above the ground) and the warg (on the ground), or null. Riding leaves the warg null.
            "falcon": companion_state(game.player.falcon.as_ref(), crate::actor::FALCON_HEIGHT),
            // Copies left behind while a status blurs the player (the quickens).
            "ghosts": game.player.ghosts.len(),
            "warg": companion_state(game.player.warg.as_ref(), 0.0),
            "cart": cart_state(game.player.cart.as_ref()),
            "casting": game.casts.contains_key(&None),
            // The attack asked for with the mouse (null: none), kept up or only while held.
            "attack": game.attack.map(|attack| json!({"target": attack.target, "continuous": attack.continuous})),
            "no_ctrl": game.no_ctrl,
            // The skill being aimed (null: none), and the last one sent by aiming, at a cell or a unit.
            "aiming": game.targeting.map(|(skill, level)| json!({"skill": skill, "level": level})),
            "skill_sent": game.skill_sent.map(|(skill, level, target)| match target {
                crate::SkillTarget::Cell(cell) => json!({"skill": skill, "level": level, "cell": cell}),
                crate::SkillTarget::Unit(unit) => json!({"skill": skill, "level": level, "unit": unit}),
            }),
            // The last cell a mouse walk went to, and how many have gone out.
            "walk_goal": game.walk_goal,
            // Skill levels put in the skill window but not sent, in the order they'd go.
            "skill_pending": game.skill_window.pending(),
            "skill_list": game.skill_window.simple(),
            // Where the cursor is drawn (the middle of what it snapped to, else the pointer).
            "cursor_at": game.cursor_at,
            "cursor_snapped": game.cursor_at != app.cursor,
            // The cell drawn under the pointer (null past the map's edge).
            "ground_cell": game.ground_cell(app.cursor),
            "walks": game.walks,
            "combo_ms": combo_ms(None),
            // How many units Blade Stop or Cursed Circle locks the player with.
            "held": crate::locked_with(&game.locks, game.account_id),
            "action": animation(&game.player).0,
            "frame": animation(&game.player).1,
            "hp": status(net::STATUS_HP),
            "max_hp": status(net::STATUS_MAX_HP),
            "sp": status(net::STATUS_SP),
            "max_sp": status(net::STATUS_MAX_SP),
            "base_level": status(net::STATUS_BASE_LEVEL),
            "job_level": status(net::STATUS_JOB_LEVEL),
            "zeny": status(net::STATUS_ZENY),
            "status_points": status(net::STATUS_POINTS),
            "skill_points": game.skill_points(),
            "stats": stats_state(&game.status),
            // A 4th job's traits, trait points and AP.
            "traits": json!({
                "pow": status(219), "sta": status(220), "wis": status(221), "spl": status(222), "con": status(223), "crt": status(224),
                "points": status(231), "ap": status(232), "max_ap": status(233),
            }),
            "skills": skills_state(&game.skills, &app.skill_info),
            "screen": screen(Owner::Player),
            "look": player_look(game.player.look.as_ref().unwrap_or(&Look::default())),
        },
        "windows": windows,
        // An open shop's deal: what's in it and how many.
        "shop_deal": game.shop.as_ref().map(|shop| shop.deal.iter().map(|&(offer, amount)| json!({"item": shop.offers[offer].item, "amount": amount})).collect::<Vec<_>>()),
        // The windows Esc closes, in the order it closes them (newest first).
        "esc_order": game.esc_names(),
        // The game menu's buttons, when it is open (null when it isn't). Newest-first `windows` does not include it.
        "escape": game.escape.as_ref().map(|menu| menu.choices(game.can_resurrect()).into_iter().map(|choice| choice.button()).collect::<Vec<_>>()),
        "save_asked": game.save_asked,
        // What a test can click by name instead of by pixels: inventory items (index and item), hotkey slots, skill
        // boxes, pick list rows, menu choices and window buttons (by name), each with its box on screen.
        "targets": targets,
        // What's carried: index, item, how many, identified, worn where (0: not worn).
        "inventory": game.inventory.items.iter().map(|item| json!({"index": item.index, "id": item.id, "amount": item.amount, "identified": item.identified, "worn": item.worn, "refine": item.refine, "grade": item.grade, "cards": item.cards, "broken": item.broken, "bind": item.bind, "expires": item.expires, "switch": item.switch, "favorite": item.favorite})).collect::<Vec<_>>(),
        // The hotkey bar, slot by slot: null, or what it holds (a skill's count is its level).
        "hotkeys": game.hotkeys.keys.iter().map(|key| key.map(|key| json!({"skill": key.skill, "id": key.id, "count": key.count}))).collect::<Vec<_>>(),
        // The hotbar showing: 0 (hotbar 1) or 1.
        "hotkey_tab": game.hotkeys.tab(),
        // Each bar's row rotation (hotbar 1, hotbar 2), and whether the quick item row shows.
        "hotkey_rotations": game.hotkeys.rotations(),
        "hotkey_quick": game.hotkeys.quick,
        // Bound shortcuts: action name to the chord ("Alt+V", "F1"). Unbound rows are left out.
        "shortcuts": app.keybinds.labels(),
        "dialog": dialog,
        // Achievements: whether each is done and its reward taken; the title worn (0: none).
        "achievements": game.achievements.iter().map(|(id, achievement)| (id.to_string(), json!({"completed": achievement.completed, "rewarded": achievement.rewarded}))).collect::<serde_json::Map<_, _>>(),
        "title": game.equipment.title,
        // Skills blocked after use: milliseconds left by skill ID, on the drawing's clock (still while a shot pauses it).
        "cooldowns": game.cooldowns.blocked(crate::control::now()).map(|(skill, left)| (skill.to_string(), json!(left.as_millis() as u64))).collect::<serde_json::Map<_, _>>(),
        // The Lucky Roulette: points, bonus, prizes known, the wheel spinning, the prize waiting (or null).
        "roulette": game.roulette.as_ref().map(|window| {
            let now = crate::control::now();
            let [gold, silver, bronze] = window.points;
            let prize = window.shown_prize(now).map(|(row, column)| json!({"row": row, "column": column, "item": window.item_at((row, column))}));
            json!({"gold": gold, "silver": silver, "bronze": bronze, "bonus": window.bonus, "prizes": window.prizes.len(), "spinning": window.spinning(now), "prize": prize})
        }),
        // Sense's window (null when closed), and the boss the Convex Mirror shows (null when none).
        // The player's guild (null without one): its allies' and antagonists' names.
        "guild": game.guild.as_ref().map(|guild| {
            let names = |antagonist: bool| guild.relations.iter().filter(|relation| relation.antagonist == antagonist).map(|relation| relation.name.as_str()).collect::<Vec<_>>();
            // Positions by ID and members by name, so a scenario can name one in a path.
            let positions: serde_json::Map<String, Value> = guild
                .position_info
                .iter()
                .map(|position| (position.id.to_string(), json!({"title": guild.title(position.id), "mode": position.mode, "tax": position.tax})))
                .collect();
            let members: serde_json::Map<String, Value> = guild.members.iter().map(|member| (member.name.clone(), json!(member.position))).collect();
            json!({
                "id": guild.id,
                "name": guild.name,
                "master": guild.master,
                "allies": names(false),
                "antagonists": names(true),
                "positions": positions,
                "members": members,
            })
        }),
        // The tags under the names of players in view (and the player's own), where known.
        "name_tags": game
            .name_tags
            .iter()
            .filter_map(|(id, tag)| {
                let name = if *id == game.account_id { Some(&game.me.name) } else { game.units.get(id).map(|actor| &actor.name) };
                name.map(|name| (name.clone(), json!({"party": tag.party, "guild": tag.guild, "position": tag.position})))
            })
            .collect::<serde_json::Map<String, Value>>(),
        // The item merge window: how many rows, and the picked stacks' indexes.
        "merge": game.pick.as_ref().filter(|(list, _)| list.merging).map(|(list, _)| json!({"rows": list.items.len(), "picked": list.picked_indexes()})),
        "monster_info": game.monster_window.as_ref().map(|window| {
            let info = &window.info;
            json!({"class": info.class, "level": info.level, "hp": info.hp, "size": info.size, "race": info.race, "element": info.element})
        }),
        "boss": game.boss.as_ref().map(|(name, cell)| json!({"name": name, "cell": cell})),
        // What Talkie Boxes are saying now.
        "talkie": game.talkie_spots.keys().filter_map(|id| game.speech.get(&Some(*id)).map(|(text, ..)| text.clone())).collect::<Vec<_>>(),
        // A taming item waiting for its monster.
        "capturing": game.capturing,
        "aimed_skill": game.targeting.map(|(id, level)| json!({"id": id, "level": level, "name": app.skill_info.get(&id).map(|info| info.name.as_str())})),
        "units": units,
        // The player's pet as the server last told it, or null.
        "party": game.party.as_ref().map(|party| {
            let leader = party.members.iter().find(|member| member.info.leader).map(|member| member.info.name.clone());
            json!({
                "name": party.name,
                "leader": leader,
                "members": party.members.iter().map(|member| member.info.name.clone()).collect::<Vec<_>>(),
                // Members whose last party HP (0x080E) is under their max, so a scenario can see a hit land.
                "wounded": party
                    .members
                    .iter()
                    .filter(|member| member.hp.is_some_and(|[hp, max]| hp < max))
                    .map(|member| member.info.name.clone())
                    .collect::<Vec<_>>(),
                "exp_shared": party.exp_shared,
                "pickup_shared": party.pickup_shared,
                "item_shared": party.item_shared,
            })
        }),
        "refusing_party": game.refusing_party,
        // The players in sight a plain click's cursor marks as enemies.
        "enemies": game
            .units
            .iter()
            .filter(|&(&id, actor)| actor.kind == PLAYER && game.opponent(id).is_some_and(|other| game.side().attack_cursor(&other, false, false)))
            .map(|(_, actor)| actor.name.clone())
            .collect::<Vec<_>>(),
        // Seconds held at first place while the clock shows, else null.
        "first_place_seconds": game.first_since.filter(|_| game.pvp_rank.is_some_and(|(_, players)| players > 1)).map(|since| crate::control::now().saturating_duration_since(since).as_secs()),
        "top_rank_auras": game.units.iter().filter(|(id, _)| game.top_ten.contains(id)).map(|(_, actor)| actor.name.clone())
            .chain(game.top_ten.contains(&game.account_id).then(|| game.me.name.clone())).collect::<Vec<_>>(),
        "dressroom": game.dressroom.as_ref().map(|room| {
            let look = room.look(&crate::character_look(&game.me), &game.dress_lists);
            json!({"hair": look.hair, "hair_color": look.hair_color, "clothes_color": look.clothes_color, "stage": room.stage, "direction": room.direction})
        }),
        "head_emblems": game.head_emblems.iter().map(|(name, guild, area)| json!({"name": name, "guild": guild, "box": area})).collect::<Vec<_>>(),
        // The bars under units: by whose ("player", "homunculus", "mercenary", "elemental", "pet", else the unit ID), the
        // box and each row's fill (null when its maximum isn't known).
        "bars": game.bars_drawn.iter().map(|(whose, area, fills)| (whose.clone(), json!({"box": area, "fills": fills}))).collect::<serde_json::Map<_, _>>(),
        // Who has the MVP sprite over their head now.
        "mvp_sprites": game.mvps.keys().map(|unit| unit.map_or(game.me.name.clone(), |id| game.units.get(&id).map(|actor| actor.name.clone()).unwrap_or_default())).collect::<Vec<_>>(),
        "map_property": game.map_property.map(|(property, flags)| json!({"type": property, "flags": flags})),
        "map_type": game.map_type,
        // The PvP counter as it shows (rank, players), or null.
        "pvp_rank": game.pvp_rank.filter(|_| game.map_property.is_some_and(|(_, flags)| flags & net::MAP_COUNT_PK != 0)).map(|(rank, players)| [rank, players]),
        // The texts in the announcement bar, oldest first.
        "announcements": game.announcements.texts(),
        "mercenary": game.mercenary.as_ref().map(|hired| json!({"id": hired.id, "name": hired.name, "level": hired.level, "hp": hired.hp, "loyalty": hired.loyalty, "summons": hired.summons, "kills": hired.kills, "following": game.steering.following, "order": game.steering.order.map(|order| format!("{order:?}"))})),
        "homunculus": game.homunculus.as_ref().map(|homunculus| homunculus_state(homunculus, game.homunculus_id, game.homunculus_steering.following)),
        "pet": game.pet.as_ref().map(|pet| json!({"id": game.pet_id, "name": pet.name, "level": pet.level, "hunger": pet.hunger, "intimacy": pet.intimacy, "accessory": pet.accessory, "renamed": pet.renamed})),
        "ground_items": ground,
        "inventory_items": game.inventory.items.len(),
        "skills": game.skills.len(),
        "status_icons": game.status_icons.iter().map(|(id, _, ends)| json!({"id": id, "left_ms": ends.map(|end| end.saturating_duration_since(now).as_millis() as u64)})).collect::<Vec<_>>(),
        // The icons as drawn down the right: the priority passes' order.
        "status_icons_drawn": crate::status_tip::placed(&game.status_icons.iter().map(|&(id, ..)| (id, ())).collect::<Vec<_>>(), |id| app.status_icon_files.get(&id).map(|(priority, _)| *priority))
            .into_iter()
            .map(|(id, _)| id)
            .collect::<Vec<_>>(),
        // Where the camera is steered to: the turn (within a circle) and tilt in degrees, the distance in world units.
        "camera": json!({
            "yaw": game.camera_goal.yaw.rem_euclid(360.0).round(),
            "pitch": game.camera_goal.pitch.round(),
            "distance": (f64::from(game.camera_goal.distance()) * 10.0).round() / 10.0,
            "indoor": game.camera_goal.indoor,
        }),
        // What the mouse cursor shows.
        "cursor": format!("{:?}", app.cursor_look.kind()),
        "chat": game.chat.lines.iter().rev().take(5).rev().map(|(text, ..)| text.as_str()).collect::<Vec<_>>(),
        // The chat window: the tab shown, each tab's last lines, where typed lines go, the lock.
        "chat_tab": game.chat.tab,
        // The hovered player's name block: its lines and its emblem's box, or null.
        "hover_name": game.hover_block.as_ref().map(|(lines, emblem)| json!({"lines": lines, "emblem": emblem})),
        "show_emblems": game.show_emblems,
        // The name in the chat's whisper box.
        "whisper_name": game.chat.whisper,
        "chat_tabs": (0..2).map(|tab| game.chat.tab_lines(tab).iter().rev().take(5).rev().map(|(text, ..)| text.clone()).collect::<Vec<_>>()).collect::<Vec<_>>(),
        "chat_send_to": format!("{:?}", game.chat.send_to),
        "chat_locked": game.chat.locked,
        "chat_corner": game.chat.corner_moved(),
        "chat_input": game.chat.input,
        // The shown tab's log: how many rows it keeps, its first and last, how far it is scrolled back; the whisper
        // name box and its list.
        "chat_log_rows": game.chat.tab_lines(game.chat.tab).len(),
        "chat_log_first": game.chat.tab_lines(game.chat.tab).first().map(|(text, ..)| text.clone()),
        "chat_log_last": game.chat.tab_lines(game.chat.tab).iter().rev().take(2).rev().map(|(text, ..)| text.clone()).collect::<Vec<_>>(),
        "chat_scroll": game.chat.scroll(),
        "chat_whisper": game.chat.whisper,
        "chat_names": game.chat.recent,
        "block_names": game.block_list.as_ref().map(|list| list.names().to_vec()).unwrap_or_default(),
        "whisper_open": json!({"friends": game.whisper_open.friends, "strangers": game.whisper_open.strangers}),
        "chat_line_open": game.chat_mode.chat_line_open(),
        // Battle mode's icon: null out of battle mode, else whether the line is open.
        "chat_mode_icon": game.chat_mode.icon(),
        // The basic info window: where it is, and whether it's minimised.
        "basic_info": json!({"position": game.basic_info.frame.position, "minimized": game.basic_info.frame.minimized}),
        // The chat room the player is in: its title and who owns it.
        "room": game.room.as_ref().map(|room| json!({
            "title": room.sign.title,
            "owner": room.members.iter().find(|(_, owner)| *owner).map(|(name, _)| name.clone()),
            "members": room.members.len(),
        })),
        // The navigation under way: where to, and how long the trail on this map is.
        "navigation": game.navigation.as_ref().map(|active| json!({
            "map": active.target.map,
            "cell": active.target.cell,
            "monster": active.target.monster,
            "trail": active.trail.len(),
        })),
        // Reputation points by reputation type.
        "reputation": game.reputation.iter().map(|(kind, points)| (kind.to_string(), json!(points))).collect::<serde_json::Map<_, _>>(),
        // The item information window's item, if it's open.
        "item_window": game.item_window.as_ref().map(|window| window.shown.item),
        // The magic squares under ground casts: the target cell and the width in cells.
        "cast_squares": game.cast_squares.values().map(|square| json!({"cell": square.cell, "width": square.width})).collect::<Vec<_>>(),
        // How many units show each status mark (the player too).
        "marks": std::iter::once(&game.player).chain(game.units.values()).flat_map(|actor| crate::head_marks::marks(actor.body_state, actor.health_state)).fold(
            serde_json::Map::new(),
            |mut counts, mark| {
                let count = counts.get(&format!("{mark:?}")).and_then(serde_json::Value::as_u64).unwrap_or(0);
                counts.insert(format!("{mark:?}"), json!(count + 1));
                counts
            }
        ),
        // The boards over heads by kind (room, shop, buying): [x, y, width, height].
        "signs": game.sign_rects.iter().map(|&(rect, _, kind)| {
            let name = match kind {
                crate::chatroom::SHOP => "shop",
                crate::chatroom::BUYING_STORE => "buying",
                _ => "room",
            };
            (name.to_owned(), json!([rect[0], rect[1], rect[2] - rect[0], rect[3] - rect[1]]))
        }).collect::<serde_json::Map<_, _>>(),
        "chat_height": game.chat.height(),
    });
    // The party booking board: the posts found (their leaders' names), its last line, and who asks to join us.
    state["booking"] = game
        .booking
        .as_ref()
        .map(|window| json!({"posts": window.posts.iter().map(|post| post.name.clone()).collect::<Vec<_>>(), "status": window.status}))
        .into();
    state["booking_asked"] = game.booking_asked.as_ref().map(|(_, _, name, ..)| name.clone()).into();
    // The RSM 2 model instances standing on the map.
    state["rsm2_models"] = game.world.rsm_2_models.into();
    state["rsm2_moving"] = game.world.rsm_2_moving().into();
    // The way the character faced on entering the map server (it faces so again after a warp).
    state["entered_facing"] = game.entered_facing.into();
    // The skill units on the ground standing as models (their UNT_ numbers).
    state["standing_models"] = game.effects.standing_kinds().into();
    state["cutin_window"] = cutin_window.into();
    // The inventory window's size and its columns and rows of cells.
    state["inventory_window"] = json!({"size": game.inventory.window_size(), "cells": game.inventory.size()});
    // The inventory items with the new mark, by index.
    state["inventory_new"] = game.inventory.new_marked().into();
    state["opened_page"] = game.opened_page.clone().into();
    // Our milliseconds a cell.
    state["player"]["step_ms"] = game.player.step_ms.into();
    // The effect sounds played in the last five seconds (`no_sound` in a scenario checks these).
    state["sounds_recent"] = recent_sounds().into();
    // The storage's rows and an NPC shop's sizes (goods width, goods and deal rows), as the corner grips leave them.
    state["storage_rows"] = game.storage.rows().into();
    state["skill_list_rows"] = game.skill_window.list_rows().into();
    state["vend_size"] = json!({"width": game.vend_size.width, "rows": game.vend_size.rows});
    state["shop_size"] = game.shop.as_ref().and_then(crate::shop::Shop::sizes).map(|(width, rows)| json!({"width": width, "rows": rows})).into();
    // The ranking board.
    state["ranking"] = json!(game.ranking.as_ref().map(|board| {
        let rows = board.rows(&game.char_names);
        json!({
            "type": board.kind,
            "mine": board.mine,
            "listed": board.ids.contains(&game.me.id),
            "unnamed": rows.iter().filter(|row| !game.char_names.contains_key(&row.id)).count(),
            "rows": rows.iter().map(|row| json!({"char_id": row.id, "name": row.name, "points": row.points})).collect::<Vec<_>>(),
        })
    }));
    // The Universal Catalog's store search.
    state["catalog"] = json!(game.catalog.as_ref().map(|catalog| json!({
        "remote": catalog.remote,
        "uses": catalog.uses,
        "buying": catalog.buying,
        "more": catalog.more,
        "mark": catalog.mark,
        "results": catalog.rows.len(),
        "rows": catalog.rows.iter().map(|row| json!({"item": row.item, "shop": row.shop, "account": row.account, "price": row.price, "amount": row.amount})).collect::<Vec<_>>(),
    })));
    // The battleground queued for, how many wait, and whether entering is being asked.
    state["battle_queue"] = json!(
        game.queued
            .as_ref()
            .map(|(name, number)| { json!({"name": name, "number": number, "asked": matches!(game.queue_asked, Some(crate::QueueAsked::Lobby(..)))}) })
    );
    // On a battleground (its map type, or our camp known): the camp, the score, teammates with a dot and an HP bar.
    let battle = &game.battle;
    let on_battleground = game.map_type == Some(crate::battleground::MAP_TYPE) || battle.camp(game.account_id).is_some();
    state["battleground"] = json!(on_battleground.then(|| {
        let team = battle.teammates(game.account_id);
        json!({
            "joined": battle.camp(game.account_id).is_some(),
            "camp": battle.camp(game.account_id),
            "score": battle.score,
            "dots": battle.dots.len(),
            "hp": team.iter().filter(|account| game.unit_hp.contains_key(account)).count(),
            "battle_chat": battle.chat,
        })
    }));
    // The partner's name, as the last I Miss You told it.
    state["partner"] = json!(game.partner);
    // The pickup popup's line while it shows.
    state["pickup_popup"] = json!(game.pickup_popup.as_ref().map(|popup| popup.text.clone()));
    // A skill's description while it shows: which skill, and its lines.
    state["skill_description"] = json!(
        game.skill_description
            .as_ref()
            .map(|window| json!({"skill": window.skill, "lines": window.text(), "closable": window.close_box().is_some(), "delays": window.delays}))
    );
    // The quest tracker as drawn: each quest's title, summary and hunt lines (none while it's off).
    let blocks = if game.quest_window.tracker {
        crate::quest_tracker::blocks(&game.quests, &app.quest_texts, &game.quest_window.hidden, &app.messages)
    } else {
        Vec::new()
    };
    state["quest_tracker"] =
        json!(blocks.iter().map(|block| json!({"id": block.id, "title": block.title, "summary": block.summary, "hunts": block.hunts})).collect::<Vec<_>>());
    // The new-quest icon up, and the progress banners showing.
    state["quest_icon"] = json!(game.quest_icon);
    state["quest_banners"] = json!(game.banners.texts());
    // The minimap: opaque, see-through or hidden, its zoom, and whether the navigation route is on.
    state["minimap"] = json!({"mode": game.minimap_view.name(), "zoom": game.minimap_zoom, "route": game.navigation_route});
    // The big minimap and the memos recorded on this map.
    state["minimap_window"] = json!({
        "open": game.minimap_window.frame.open,
        "x": game.minimap_window.frame.position[0],
        "y": game.minimap_window.frame.position[1],
        "memos": game.minimap_window.memos.iter().map(|memo| json!({"x": memo.cell[0], "y": memo.cell[1], "name": memo.name})).collect::<Vec<_>>(),
        "quests": game.minimap_window.show_quests,
        "boss": game.minimap_window.show_boss,
        "recording": game.minimap_window.recording,
    });
    // The level up buttons up in the bottom corners.
    state["level_ups"] = json!({"base": game.level_ups.base, "job": game.level_ups.job});
    // The status icon tooltip's lines while one shows.
    state["status_tip"] = json!(game.status_tip);
    orbs(&mut state["player"], None);
    state
}

/// The player's look as the game state reports it. Zeros until the server has sent one.
/// A pushcart's spot (world X and Z), facing (degrees) and wheel frame, or null.
fn cart_state(cart: Option<&crate::actor::Cart>) -> Value {
    let Some(cart) = cart else { return Value::Null };
    let tenth = |value: f32| (f64::from(value) * 10.0).round() / 10.0;
    json!({"x": tenth(cart.at[0]), "z": tenth(cart.at[1]), "rot": tenth(cart.rot), "frame": cart.frame(cart.sprites.frames(0))})
}

fn companion_state(companion: Option<&crate::actor::Companion>, height: f32) -> Value {
    let Some(companion) = companion else { return Value::Null };
    let tenth = |value: f32| (f64::from(value) * 10.0).round() / 10.0;
    json!({"x": tenth(companion.at[0]), "z": tenth(companion.at[1]), "height": height, "moving": companion.moving})
}

fn player_look(look: &Look) -> Value {
    json!({
        "hair": look.hair,
        "hair_color": look.hair_color,
        "clothes_color": look.clothes_color,
        "head_top": look.head_top,
        "head_mid": look.head_mid,
        "head_bottom": look.head_bottom,
    })
}

/// The six stats as the status window shows them: by short name ("str" to "luk"), the value, the bonus from gear and
/// what one point more costs.
fn stats_state(status: &HashMap<u16, i64>) -> Value {
    let value = |id: u16| status.get(&id).copied().unwrap_or(0);
    let stats = crate::status::STATS.iter().zip(net::STATUS_STR..).map(|(name, stat)| {
        let cost = value(net::STATUS_STR_COST + stat - net::STATUS_STR);
        ((*name).to_owned(), json!({"value": value(stat), "bonus": value(stat + net::STATUS_BONUS), "cost": cost}))
    });
    Value::Object(stats.collect())
}

/// The player's skills as the server lists them (level 0: not learned yet), with their names and top levels.
fn skills_state(skills: &[net::Skill], info: &HashMap<u16, SkillInfo>) -> Value {
    skills
        .iter()
        .map(|skill| {
            let info = info.get(&skill.id);
            json!({"id": skill.id, "name": info.map(|info| info.name.as_str()), "level": skill.level, "max_level": info.map(|info| info.max_level), "upgradable": skill.upgradable})
        })
        .collect()
}

/// The player's homunculus as the game state reports it.
fn homunculus_state(homunculus: &net::Homunculus, id: Option<u32>, following: bool) -> Value {
    json!({
        "id": id,
        "name": homunculus.name,
        "level": homunculus.level,
        "hunger": homunculus.hunger,
        "intimacy": homunculus.intimacy,
        "hp": homunculus.hp,
        "skill_points": homunculus.skill_points,
        "vaporized": homunculus.vaporized(),
        "following": following,
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
            Command::Wait {
                ms: None,
                packet: Some(0x08c8),
                event: None,
                sent: None,
                fresh: false,
                pause: true,
                first: Some(Box::new(Command::Key { key: "F1".into(), repeat: false, hold: None }))
            }
        );
        assert!(matches!(parse(r#"{"cmd":"wait","packet":2248,"ms":500}"#), Ok(Command::Wait { packet: Some(0x08c8), ms: Some(500), .. })));
        assert!(matches!(parse(r#"{"cmd":"wait","sent":"0x0447"}"#), Ok(Command::Wait { sent: Some(0x0447), packet: None, .. })));
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

        use crate::pin::{PinPad, PinStep};
        let pin_kind = |step: PinStep| PinPad::new(0, step, false).kind();
        assert_eq!(pin_kind(PinStep::Ask), "ask");
        assert_eq!(pin_kind(PinStep::NewFirst), "new");
        assert_eq!(pin_kind(PinStep::NewConfirm { first: "1357".into() }), "new");
        assert_eq!(pin_kind(PinStep::ChangeOld), "change");
        assert_eq!(pin_kind(PinStep::ChangeNewFirst { old: "1357".into() }), "change");
        assert_eq!(pin_kind(PinStep::ChangeNewConfirm { old: "1357".into(), first: "2468".into() }), "change");
        assert_eq!(None::<PinPad>.as_ref().map_or("none", |pad| pad.kind()), "none");
    }

    #[test]
    fn reports_stats_and_skills() {
        let status = HashMap::from([
            (net::STATUS_STR, 1),
            (net::STATUS_STR + 3, 9),
            (net::STATUS_STR + 3 + net::STATUS_BONUS, 2),
            (net::STATUS_STR_COST + 3, 2),
            (net::STATUS_STR_COST + 5, 3),
        ]);
        let stats = stats_state(&status);
        assert_eq!(stats.as_object().unwrap().len(), 6);
        assert_eq!(stats["int"], json!({"value": 9, "bonus": 2, "cost": 2}));
        assert_eq!(stats["str"], json!({"value": 1, "bonus": 0, "cost": 0}));
        assert_eq!(stats["luk"]["cost"], 3);
        let info = HashMap::from([(19, SkillInfo { resource: "MG_FIREBOLT".into(), name: "Fire Bolt".into(), max_level: 10, ..Default::default() })]);
        let skills = [
            net::Skill { id: 19, kind: net::Skill::ENEMY, level: 3, sp: 16, range: 9, upgradable: true },
            net::Skill { id: 9999, kind: 0, level: 0, sp: 0, range: 0, upgradable: false },
        ];
        assert_eq!(
            skills_state(&skills, &info),
            json!([
                {"id": 19, "name": "Fire Bolt", "level": 3, "max_level": 10, "upgradable": true},
                {"id": 9999, "name": null, "level": 0, "max_level": null, "upgradable": false},
            ])
        );
    }

    #[test]
    fn reports_the_players_look() {
        let look = Look { hair: 1, hair_color: 3, clothes_color: 2, head_top: 16, head_mid: 0, head_bottom: 4, ..Look::default() };
        assert_eq!(player_look(&look), json!({"hair": 1, "hair_color": 3, "clothes_color": 2, "head_top": 16, "head_mid": 0, "head_bottom": 4}));
    }
}
