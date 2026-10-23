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
cargo fmt && .claude/skills/rustnarok-dev/verify.sh
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
python3 -I $D state                       # scene, map, player, windows, units with their screen boxes: a big JSON
python3 -I $D state game.ground_items     # one part of it, by dotted path (also game.targets, game.windows, game.chat)
python3 -I $D send '{"cmd":"say","text":"@item 501 5"}'
python3 -I $D send '{"cmd":"click","x":512,"y":384,"count":2}'
python3 -I $D shot inventory-open         # -> docs/personal/shots/inventory-open.png
python3 -I $D quit
```

Coordinates are the client's logical 1024×768; a screenshot can be wider (1224 px on a retina display), so take click
points from `state`'s `targets` boxes, not from the image. A driven client ignores the real mouse and keyboard. A GM account
(`group_id` 99 in rathena's `login` table) can spawn what a test needs (`@monster`, `@item`, `@warp`). A scenario sets the level it needs with
`@set BaseLevel <n>` (absolute, unlike `@blvl`): the ones that spend SP (skill-hits, skill-motions, status-loops) and
aura-high leave the first character at base 99.

### Playing it as a player: play modes

An agent playthrough uses only what a player has. Every MCP tool and `state` field has a grade (`src/play.rs`):

- **fair**: what a player has: the same input, and what the screen shows.
- **assist**: nothing a player couldn't see or do, in an easier form, or making up for a mouse driven over a socket.
  Examples: `click_unit`, `cell_point` for on-screen cells, the cells of units on screen, aggro cues, a monster's HP
  bar as a fraction.
- **test**: beyond a player, for tests and debugging. `trace`, `clock`, `wait` on `packet`/`sent`/`pause`, `@` GM
  commands, units and items off screen, invisible script NPCs, `moving_to`, a monster's HP in numbers, animation
  internals.

`RUSTNAROK_CLIENT_PLAY=fair|assist|all` (or MCP `launch {"play": ...}`) is the highest grade a session may use. The
client refuses commands above it ("test only (play mode: ...)") and leaves those fields out of `state`, which then says
`"play"`. The MCP lists only the tools the mode allows. Unset means `all`, so tests and scenarios are unchanged.

A playthrough states its mode, and logs any switch to `all` and why. `@` lines typed into the chat box still reach the
server, as they do for a player, so play on an account that isn't a GM.

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
rathena: delete the characters' `hotkey`, `skillcooldown` and `sc_data` rows (running cooldowns and statuses such as Millennium Shield are saved at logout) and their `achievement` rows for 200000 and 200005 (achievements.json
completes them again), set `char.inventory_slots` back to 100, `char.pet_id` to 0 (a pet a stopped run left out of its
egg), `char.title_id` to 0, `char.homun_id` to 0 and delete their `homunculus` and `skill_homunculus` rows (`@makehomun` refuses a character that has one), `char.clan_id` to 0 (a clan a stopped run left the character in; e.g. `UPDATE char SET clan_id=0`), `char.class` to 0 and their `inventory` rows' `equip`, `attribute` and `equip_switch` to 0 (gear a stopped run
left worn, broken or in the Swap Equipment set), and delete their rentals (`inventory` rows with an `expire_time`: they warn in every scenario) (a job left behind completes job achievements at login). A
scenario that changes something saved on the character (`@job`, `@vit`) changes it back at its end. `mask` keeps animated rectangles (water, portals) out of the diff; `threshold` sets how much change a shot may take
before the report flags it. Each scenario starts a fresh client, with fresh client settings (`RUSTNAROK_CLIENT_SETTINGS`, a temporary folder its
clients and relaunches share, so saved options never leak into the next scenario or your own client), and pauses the
clock for every shot. Add one for the
feature you touch, and keep it: the set grows into the game's visual regression suite.

`python3 -I $V suite` checks every scenario and writes `report.md` beside the sheets, but it takes about half an hour
now: **don't run it per PR** (see "Merge bar"). `check` and `suite` play `origin/main` as it is *now*: rebase onto it
first, or main's own newer fixes show up as your branch's regressions.

Animated things (water, warp portals, effects) differ between two runs of the same code: about 0.6% on an interior
map. Crop shots to what the change touches when that noise gets in the way. Read the sheets: an unexpected change elsewhere is a regression; no change where you meant one is a fix that didn't
land. Put the sheets for the changed shots in the PR.

## Things only the server can start

The server sends some things only for its own events, so a GM command or a player's own action can't show them: an
item's drop pillar (the fall packet's show drop effect is set for monster drops and the script command `makeitem`, never
for a player's drop). To see one, run a throwaway script on your rathena (`@loadnpc`, `makeitem … ,1`, `@unloadnpcfile`
at the end), then remove the file.

## Several tickets at once: lanes

A lane is a git worktree of its own for one ticket, so several people or sessions can each build, run clients and play
scenarios at the same time against one rathena:

```bash
L=.claude/skills/rustnarok-dev/lane.sh
$L new issue-123 --accounts ~/.config/rustnarok/lanes/a.accounts --reset ~/.config/rustnarok/lanes/a.reset
cd ../rustnarok-client-lanes/issue-123      # work, build, verify.sh, visual.py check here as usual
$L list                                     # each lane: branch, merged or open, clients running
$L rm issue-123                             # when its PR is merged: quits its clients, removes it, deletes the branch
$L prune                                    # every merged lane with no client running
```

- `new` branches off a fresh `origin/main` (`--branch` to name it) into `../rustnarok-client-lanes/<name>`
  (`RUSTNAROK_LANES` to move them) and links in the game data and your `local-*` skills from the main checkout. The lane
  builds into its own `target/` (the first build takes a few minutes).
- **Each lane needs its own test accounts**: two clients on one account log each other out. Give every lane a pair
  (GM, each with a character in slot 0), an accounts file like the main one, and a reset script for its characters.
  `--accounts` and `--reset` copy them to the lane's `docs/personal/accounts.txt` and `docs/personal/scenario-reset.sh`,
  which `drive.py` and `visual.py` use when the variables aren't set. Sockets, logs, shots and `visual.py`'s baseline
  are per checkout already. With rathena, a pair is two rows in `login` (`group_id` 99) and a character each
  (`RUSTNAROK_CLIENT_MAKE=0:<name>` on a launch makes it).
- **The server is shared**: map-wide commands (`@killmonster2`, weather flags, `@broadcast`) reach every lane on that
  map. Concurrent scenarios on different maps don't meet; two on one map may.
- `rm` refuses a lane with uncommitted work unless `--force`, and keeps a branch that isn't merged.

## Merge bar

Merge when `verify.sh` ends `VERIFY: PASS` and `visual.py check` passes on **the feature's own scenario** (plus a
scenario the change directly touches, e.g. `party.json` for party work), on a branch rebased onto current
`origin/main`. No full suite per PR: fixing what breaks later is cheaper than waiting on regression runs (the project
lead's call, 2026-10-18).

## Issue to PR

0. **Pick it up visibly**: check it's free **right before** claiming, as its own step (no assignee, no "Taking this"
   comment): another developer may have taken it minutes ago. Then assign yourself, react 👀 on the issue
   (`gh api -X POST repos/kindraywind/rustnarok-client/issues/<n>/reactions -f content=eyes`) and comment that you've
   taken it. A contributor without write access can't assign (`gh issue edit` is refused): the 👀 and the comment are
   the claim. Comment again as it moves — what you found, the plan, the PR — in the issue's language.
1. Reproduce it in the running client and screenshot it. **A UI bug report needs a screenshot.** Attach it to the
   issue on GitHub (drag it into the comment box); never commit it.
2. Check the protocol facts against rathena before planning; plan before writing code.
3. Branch off a fresh `origin/main` (or `lane.sh new`), build, run `verify.sh` until `VERIFY: PASS`.
   A rebase or merge that stops on a conflict in a file whose code has moved to other files (as main.rs's methods
   did to `input.rs`, `events.rs`, `draw.rs` and `game.rs`): `python3 -I tools/port-moved.py <that file>` applies
   your hunks wherever that code is now.
4. QA in the client: the fix, then try to break it (edges, Esc, resizing, other windows open, the second account).
   Screenshot before and after.
5. Open the PR answering the three questions, each with evidence:
   1. **Was there a real issue, and did we solve it?** It reproduced on main (a failing scenario expectation or a
      before-screenshot) and the root cause is named. If it can't be reproduced, say so; don't claim a fix.
   2. **How was the fix verified?** `verify.sh` and the issue's scenario passing on the branch, after-screenshots.
   3. **How do we know nothing regressed?** The merge bar: unit tests, and the feature's own scenario (plus any it
      directly touches) passing with `check` against current main. Say what else you checked.
   A contributor's PR comes from a fork, so `gh pr list --head issue-<n>` finds none: look with
   `gh pr list --repo kindraywind/rustnarok-client --state all --search "<n>"`.
6. Merge it (`gh pr merge --merge --delete-branch`), then `lane.sh rm` its lane.
