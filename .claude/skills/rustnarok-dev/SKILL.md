---
name: rustnarok-dev
description: Development loop for rustnarok-client shared by everyone working on it — the repo's rules, issue to PR, building and verifying (fmt, clippy, tests, a live rathena), and playing the client over its control socket for QA and screenshots. Use when working an issue or bug here, before opening or updating a PR, or when asked to build, verify, run, QA or screenshot the client.
---

# rustnarok-client: develop, verify, QA

Personal or machine-specific notes (your server's ports, data paths, task lists) go in a skill of your own under
`.claude/skills/local-<you>/`, which git ignores. Keep this one true for every machine.

## The repo's rules

- **Clean room**: protocol facts from rathena's source (`src/map/clif.cpp`, `packets.hpp`, `clif_packetdb.hpp` on
  kindraywind/rathena, branch `rustnarok`), file formats from public documentation, the look from the game's own
  data files. No code from the official client.
- **No game data in git**: no .grf, nothing extracted from one, **no screenshots**. Hand-written fact tables
  (`effects.txt`, `status_icons.txt`, `packet_lengths.txt`) are code.
- Comments say what the code does, plain and short; `ponytail:` marks shortcuts and estimates, with what would
  replace them. Dev switches start with `RUSTNAROK_CLIENT_`.
- `cargo fmt` (160 columns) and `cargo clippy --release --all-targets` stay clean.
- Requests are pure `encode_*` functions plus a `MapSession` method; replies decode in `decode_map_packet` into
  `MapEvent`s. Both get unit tests in the style of `decodes_mail`. New tests must not need data.grf.
- Commit messages and PR text: plain and short like the history; `Closes #<n>` in the PR body.

## What you need

- The game data: see the README's Setup. `data.grf`, `data`, `System`, `BGM` are gitignored (symlinks are fine).
- A rathena from kindraywind/rathena `rustnarok` (its RUSTNAROK.md), login on 127.0.0.1:6900.
- An accounts file outside the repo — `RUSTNAROK_CLIENT_ACCOUNTS`, default `~/.config/rustnarok/accounts.txt` —
  with `username:`/`password:` lines (`username2:`/`password2:` for a second account), each with a character in
  slot 0. Never print the passwords.

## Build and verify

```bash
.claude/skills/rustnarok-dev/verify.sh
```

fmt, clippy (`-D warnings`), every test target, and when a login server is up, the ignored live test. Failures that
come only from a missing data.grf (`NotFound`) are counted apart; anything else fails the run. It ends with
`VERIFY: PASS` or `VERIFY: FAIL`. "Already online" (`Notified(8)`) after a previous run is retried for a minute.

## Play it: QA and screenshots

`drive.py` starts the client on a control socket (`qa.sock`, gitignored) and sends it commands (the list is at the
top of `src/control.rs`):

```bash
D=.claude/skills/rustnarok-dev/drive.py
python3 -I $D launch "" 0                 # first account, slot 0: logs in and enters the map
python3 -I $D state                       # scene, map, player, windows, units with their screen boxes
python3 -I $D send '{"cmd":"say","text":"@item 501 5"}'
python3 -I $D send '{"cmd":"click","x":512,"y":384,"count":2}'
python3 -I $D shot inventory-open         # -> docs/personal/shots/inventory-open.png
python3 -I $D quit
```

Coordinates are the client's logical 1024×768. A driven client ignores the real mouse and keyboard. A GM account
(`group_id` 99 in rathena's `login` table) can spawn what a test needs (`@monster`, `@item`, `@warp`).

## Issue to PR

1. Reproduce it in the running client and screenshot it. **A UI bug report needs a screenshot.** Attach it to the
   issue on GitHub (drag it into the comment box); never commit it.
2. Check the protocol facts against rathena before planning; plan before writing code.
3. Branch off a fresh `origin/main`, build, run `verify.sh` until `VERIFY: PASS`.
4. QA in the client: the fix, then try to break it (edges, Esc, resizing, other windows open, the second account).
   Screenshot before and after.
5. Open the PR with what changed, what was verified and how, and the screenshots attached in the description.
