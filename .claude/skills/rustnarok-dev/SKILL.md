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

## Visual verification: before and after

This is a game: what the player sees is the product. Unit tests and packet checks don't show a window drawn in the
wrong place, so every visible change is checked in the running client against `origin/main`.

```bash
V=.claude/skills/rustnarok-dev/visual.py
python3 -I $V check .claude/skills/rustnarok-dev/scenarios/inventory-hotkeys.json
```

`check` builds `origin/main` in a git worktree (`docs/personal/baseline-tree`), plays the scenarios with it, plays
them again with this tree, and writes per shot the share of pixels changed and a before | after | diff sheet (changed
pixels in red). `run`, `baseline` and `diff` do the steps one at a time. Output goes to
`docs/personal/visual/<date>/<HHMMSS>/{main,branch,diff}/`, gitignored, with each shot's game state beside it.

Scenarios are JSON in `scenarios/` (the format is at the top of `visual.py`): GM commands, waits (`settle` after a
warp: commands sent while a map loads are lost), keys, clicks, drags, and named shots, optionally cropped.
**Click by name, not by pixels**: `click_on` / `drag_on` take `{"inventory_item": <id>}`, `{"pick_item": <id>}`, `{"menu_item": "<part of a label>"}`, `{"hotkey": <slot>}`,
`{"skill": <id>}` from the control socket's `targets`, so a scenario survives windows moving and inventories
reordering; keep raw coordinates for the world and for things not yet in `targets`.
**Two players**: `"second": {"account": "2", ...}` starts a second client (on `qa2.sock`); `"as": "second"` sends a
step or a shot to it. Whispers, trades, parties and chat rooms can be scripted and seen from both sides.
Name the characters `{first}` and `{second}`, not by their names on your server: `visual.py` puts in each
client's own, so the scenario runs for everyone.
**Make each scenario prove itself**: `expect` steps check the game state (chat lines, HP, map, open windows, units)
and `wait_for` waits on a packet or event (`ItemUsed`, `PickedUp`) instead of a sleep. A failed one fails the
scenario and `visual.py` exits non-zero — otherwise a scenario that silently did nothing still makes screenshots.
Set `RUSTNAROK_CLIENT_SCENARIO_RESET` to a shell command that puts your server's test characters back in a known
state (it runs before every scenario): what one scenario saves on the server otherwise leaks into the next. With
rathena: delete the characters' `hotkey` and `skillcooldown` rows (running cooldowns are saved at logout) and their `achievement` rows for 200000 and 200005 (achievements.json
completes them again), set `char.inventory_slots` back to 100, `char.pet_id` to 0 (a pet a stopped run left out of its
egg), `char.title_id` to 0, `char.class` to 0 and their `inventory` rows' `equip` and `attribute` to 0 (gear a stopped run
left worn or broken) (a job left behind completes job achievements at login). A
scenario that changes something saved on the character (`@job`, `@vit`) changes it back at its end. `mask` keeps animated rectangles (water, portals) out of the diff; `threshold` sets how much change a shot may take
before the report flags it. `python3 -I $V suite` checks every scenario and writes `report.md` beside the sheets. Each starts
a fresh client and pauses the clock for every shot. Add one for the feature you touch, and keep it: the set grows into
the game's visual regression suite.

Animated things (water, warp portals, effects) differ between two runs of the same code: about 0.6% on an interior
map. Crop shots to what the change touches when that noise gets in the way. Read the sheets: an unexpected change elsewhere is a regression; no change where you meant one is a fix that didn't
land. Put the sheets for the changed shots in the PR.

## Issue to PR

0. **Pick it up visibly**: assign yourself, react 👀 on the issue
   (`gh api -X POST repos/kindraywind/rustnarok-client/issues/<n>/reactions -f content=eyes`) and comment that you've
   taken it. Comment again as it moves — what you found, the plan, the PR — in the issue's language.
1. Reproduce it in the running client and screenshot it. **A UI bug report needs a screenshot.** Attach it to the
   issue on GitHub (drag it into the comment box); never commit it.
2. Check the protocol facts against rathena before planning; plan before writing code.
3. Branch off a fresh `origin/main`, build, run `verify.sh` until `VERIFY: PASS`.
4. QA in the client: the fix, then try to break it (edges, Esc, resizing, other windows open, the second account).
   Screenshot before and after.
5. Open the PR answering the three questions, each with evidence:
   1. **Was there a real issue, and did we solve it?** It reproduced on main (a failing scenario expectation or a
      before-screenshot) and the root cause is named. If it can't be reproduced, say so; don't claim a fix.
   2. **How was the fix verified?** `verify.sh` and the issue's scenario passing on the branch, after-screenshots.
   3. **How do we know nothing regressed?** `visual.py suite` on the branch against main: every scenario's
      expectations pass, and any changed shot outside the fix is explained.
