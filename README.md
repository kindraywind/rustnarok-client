# rustnarok-client

A native Ragnarok Online client for macOS, written from scratch in Rust. The aim is to match the official client or
improve on it: its windows, layouts, fonts, sounds and behavior at least as good as the original, drawn from the
player's own game data at runtime. It speaks the protocol of the 2022-04-06 main client (PACKETVER 20220406) and is
developed against a local rathena server (see [Server](#server)).

The crate and binary are `rustnarok-client` (the library `rustnarok_client`), and the dev switches start with
`RUSTNAROK_CLIENT_`.

Nothing from the game is in this repository: no data.grf, no images, sounds or translations. The client reads them
from the player's files when it runs.

## What works

- **Login to map**: the 2018+ login window (saved ID), server select, character select with sprites, character
  creation (doram included) and deletion with its birthdate check, the loading screen.
- **The world**: GND ground with lightmaps, RSW models (RSM 1.x), water, GAT walkability, the camera (right drag
  turns, wheel zooms), cursors, minimap with NPC marks and party members, the world map.
- **Characters**: players with heads, headgear, weapons, shields and garments; monsters, NPCs, warp portals; walking,
  sitting, emotions; HP/SP bars, names, chat bubbles, cast bars.
- **Combat and skills**: attacking with reach checks, damage numbers of every kind, death; the skill window and
  hotkeys, casting with targets, status icons, effects (STR animations, moving textures, cylinders).
- **Windows**: basic info with its menu, inventory, equipment, item descriptions with card slots, status (raising
  stats), skills, hotkey bar, chat with whispers, party, guild (info, members, notice), friends, storage, cart, NPC
  dialogs (menus, number and text input), NPC shops, trading, chat rooms, vending (own shop and others'), RODEX
  mail, quests, the option menu (Esc) and audio settings, the right-click menu.
- **Sound**: each map's music, sprites' own sounds (attacks, flinches, deaths), effect sounds, volume settings.
- **macOS**: Retina-sharp text, resizable window, fullscreen (Ctrl+Cmd+F), an `.app` bundle.
- **English** through a translation pack's loose files placed over data.grf, as the official client does.

Window places are measured over the client's own bitmaps where they show them; where they don't, they are estimates
marked `ponytail:` in the code, to calibrate against screenshots of the real client.

## Setup

1. **Rust**: install rustup; `rust-toolchain.toml` pins the nightly the code builds with.
2. **Game data**: put an official kRO `data.grf` (2024 data is what's tested) in the repository folder, or point
   `RUSTNAROK_CLIENT_GRF` at one.
3. **English** (optional): a translation pack's `data` and `System` folders next to data.grf. Their loose files go
   before the archive. `RUSTNAROK_CLIENT_DATA=<folder>` says where they are, `none` turns them off.
4. **Music** (optional): the official `BGM` folder (numbered mp3s) next to data.grf, or `RUSTNAROK_CLIENT_BGM=<folder>`.
5. **Server**: as the official client does, it reads the server list from `data/clientinfo.xml` (else
   `sclientinfo.xml`) in that data folder. Only a loose file counts, never the copy inside data.grf. With several
   connections, a list to pick from comes before the login window. Each connection's `<version>` and `<loading>`
   pictures are used. Without the file it logs into `127.0.0.1:6900`, and `RUSTNAROK_CLIENT_SERVER=host:port`
   overrides both. To join a server on another machine, put its address in clientinfo.xml; the server must announce
   an address the player can reach.

`data.grf`, `data`, `System`, `SystemEN` and `BGM` are gitignored. In development they are symlinks to wherever the
files live.

## Build and run

```bash
cargo build --release
```

```bash
cargo run --release
```

`tools/bundle.sh` builds `Ragnarok.app`. Like the official client, the app goes next to data.grf (and the data,
System and BGM folders). Started from Finder, it works in its own folder.

## Server

The client needs a server running PACKETVER 20220406 without packet obfuscation. The rathena it is developed against
is a fork with those settings and a setup guide for macOS: https://github.com/kindraywind/rathena (branch
`rustnarok`, see its RUSTNAROK.md). Its changes from upstream are:

- `src/custom/defines_pre.hpp`: `#define PACKETVER 20220406`.
- `src/config/packets.hpp`: packet obfuscation off.
- `conf/import-tmpl/char_conf.txt`: `pincode_enabled: no`; `conf/import-tmpl/login_conf.txt`: `new_account: yes`.

Test accounts live in that server's database; their passwords never go in this repository. Keep them in a file
outside it with `username:`/`password:` lines (`username2:`/`password2:` and so on for more), which the dev tools
read.

## Development

### Layout

The library (`src/lib.rs`) holds what doesn't draw: `grf` (archives, with the mixcrypt decryption), `lua` (the
client's Lua tables run in a sandboxed Lua 5.1: item, skill, job, look and world-map data), `net` (login, char and
map server protocols, every packet the client reads or sends), `map` (GND, GAT, RSW), `model` (RSM), `sprite`
(SPR, ACT), `effect` (STR), `text` (font rasterizing), `messages` (msgstringtable and its `%s`/`%d` filling).

The binary (`src/main.rs` and its modules) is the game: `render` (wgpu: sprites, text, the canvas viewport), `world`
(the 3D world pass and camera), `actor` (units, their animation and sounds), `effects` (the effect player and its
table `effects.txt`), `sound` (music and effects over cpal), `control` (the test-driver socket), and one module per
window: `basic_info`, `inventory`, `equipment`, `iteminfo`, `status`, `skills`, `hotkeys`, `chat`, `party`, `guild`,
`storage`, `shop`, `trade`, `chatroom`, `vend`, `mail`, `quest`, `escape`, `options`, `menu`, `minimap`, `worldmap`,
`char_select`, `char_make`.

`main.rs` holds the scenes (login, character select, game), the game state, input, the server's events and
drawing. The UI draws in a fixed 1024×768 canvas, scaled to the window.

### Packet lengths

An unknown packet ID ends the connection, so the client knows every packet length the server uses:
`src/packet_lengths.txt`, generated from rathena's headers by `tools/packet-lengths/run.sh <rathena> <output>`. It
compiles the server's packet tables and structs at this PACKETVER, and finds the packets rathena writes by hand.
Regenerate it whenever the server changes.

### Tests

```bash
cargo test --release
```

Some tests read the official data.grf (`RUSTNAROK_CLIENT_GRF`, or `./data.grf`). One test logs into a running server and is
ignored by default; it runs with `RUSTNAROK_CLIENT_TEST_ACCOUNT=user:password cargo test -- --ignored`.

### Seeing what the client draws

- `RUSTNAROK_CLIENT_SCREENSHOT=out.png` renders headlessly, saves a settled frame and quits; in the game after
  `RUSTNAROK_CLIENT_SHOT_DELAY` seconds.
- `RUSTNAROK_CLIENT_CONTROL=<socket>` lets a test driver play the client over a Unix socket: screenshots, clicks, keys,
  typing, the game state with units' screen boxes, waiting for packets, pausing the clock. The commands are listed
  at the top of `src/control.rs`. A driven client ignores the real mouse and keyboard and stays silent.
- `src/bin/rustnarok-client-mcp.rs` offers those commands as MCP tools, so an AI assistant can launch and play the client
  (`claude mcp add rustnarok-client-dev -- <repo>/target/release/rustnarok-client-mcp`).
- `RUSTNAROK_CLIENT_TRACE=1` prints received packets; `=full` dumps them whole.

The top of `src/main.rs` lists the other dev switches: logging in and picking a slot (`RUSTNAROK_CLIENT_LOGIN`,
`RUSTNAROK_CLIENT_AUTOSTART`), scripted chat and GM commands (`RUSTNAROK_CLIENT_SAY`), clicks (`RUSTNAROK_CLIENT_CLICKS`), walking, talking,
fighting, shops, storage, trades, parties, guilds and more.

The examples are tools for reading the game data: `grf_list`, `grf_extract`, `grf_grep`, `grf_sheet` (images to one
PNG), `lua_dump`, `messages`, `job_name`, `act_sounds`, `str_dump`, `map_info`, `rsm_versions`, `map_probe`.

### Conventions

- **Clean room**: no decompiled code from the official client. The protocol comes from the server's source, file
  formats from public documentation, and the client's look from its own data files.
- **No game data in git**: no .grf, nothing extracted from one, no screenshots. Fact tables written by hand
  (`effects.txt`, `status_icons.txt`, `packet_lengths.txt`) are code.
- **`ponytail:` comments** mark deliberate shortcuts and estimates, with what would replace them.
- `cargo fmt` (`rustfmt.toml`, 160 columns) and `cargo clippy` stay clean.

## Not done yet

- Calibration of window places against screenshots of the real client.
- Effects: SPR, FUNC and RSM effect types; the graphics and hotkey settings windows behind the option menu.
- Guild positions, skills, expulsion list and emblems; the status window's trait stats.
- World map: dungeons, monster info, the other worlds. Quests: the recommended tab.
- Mail: returning mail, selecting several; the fee shown before sending.
- Weapon hit sounds; RSM2 models; lightmap edge smoothing.

## Licenses

See [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) for the code in `src/grf/mixcrypt.rs` that comes from elsewhere.
