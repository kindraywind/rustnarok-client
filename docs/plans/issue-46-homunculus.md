# Issue #46 — Homunculus: its windows, commands and status

## Problem

An Alchemist's homunculus doesn't show: its sprite isn't found (`data\sprite\homun\` isn't searched), none of its
packets are decoded, there is no window (Alt+R) or skill list, and nothing feeds, renames, steers or follows it.

## Already there (reuse, don't redo)

- `net.rs`: `encode_move_npc`, `encode_act_npc`, `MapSession::{move_npc, act_npc, move_to_owner}` (CZ_REQUEST_*NPC,
  MOVETOOWNER), `ZC_ACK_REQNAMEALL_NPC` → `MapEvent::UnitName`, `decode_skill` / `SKILL_SIZE`, `skill_up`,
  `use_skill`, `use_skill_at`, `encode_rename_pet` (the name field rule to copy).
- `mercenary.rs`: `Steering` / `Order` (follow > 3 cells, twice a second, orders pause following) — the homunculus
  gets its own `Steering` value; the logic stays in one place.
- `mercenary.rs` window: `homuninfo_bg.bmp` layout constants (values, bars, right half) — the homunculus window uses
  the same background, so the same measurements.
- `pet.rs`: rename/feed buttons, words lookups, `PetAsked` rename question flow in `main.rs`.
- `control.rs`: unit kinds 7/8/9 are named; `visual.py` has `state` dotted paths and `click_unit` with `mods`.

## Protocol (rathena `rustnarok`, PACKETVER 20220406)

From the server: ZC_CHANGESTATE_MER 0x0230 (12), ZC_PROPERTY_HOMUN 0x0BA4 (85), ZC_HO_PAR_CHANGE2 0x0BA5 (12),
ZC_HOSKILLINFO_LIST 0x0235 (var), ZC_HOSKILLINFO_UPDATE 0x0239 (11), ZC_FEED_MER 0x022F (7). Requests:
CZ_COMMAND_MER 0x022D (5; u16 0 at 2, command u8 at 4; 1 feed), CZ_RENAME_MER 0x0231 (26). Layouts as in the issue.

## Subtasks (TDD)

Each subtask: tests first (they fail / don't compile), then the code.

### T1 — Packets (`src/net.rs`)

**Key tests** (`decodes_homunculi`, in the style of `decodes_mercenaries`)

- 0x0230 state 0/1/2 → `MapEvent::HomunculusState { state, id, value }`.
- 0x0BA4 → `MapEvent::HomunculusInfo(Homunculus { .. })`: every field at its offset; `renamed()`, `vaporized()`,
  `alive()` from flag bits 0, 1, 2; EXP as i64.
- 0x0BA5 types 1/5/7 → `MapEvent::HomunculusChanged { kind, value: u64 }`; `Homunculus::set` applies EXP, HP, SP.
- 0x0235 → `MapEvent::HomunculusSkills(Vec<Skill>)`; 0x0239 → `MapEvent::HomunculusSkill(Skill)` (kind kept).
- 0x022F → `MapEvent::HomunculusFed { ok, food }`.
- `encode_homunculus_command(HOMUNCULUS_FEED)` = `[0x2D, 0x02, 0, 0, 1]`; `encode_rename_homunculus` = 26 bytes,
  NUL-padded, never past 23 bytes nor half a character.
- `packet_length` of every ID above matches.

**Code**: constants, `Homunculus` struct + `set`, `MapEvent` variants, decode arms, encoders,
`MapSession::{homunculus_command, rename_homunculus}`.

### T2 — Window, words, skill list (`src/homunculus.rs`, new)

**Key tests**

- `hunger_word`: >90 671, >75 670, >25 669, >10 668, else 667 (bounds 91/90, 76/75, 26/25, 11/10).
- `intimacy_word`: ≥911 675, ≥751 674, ≥251 669, ≥101 673, ≥11 672, ≥4 1021, else 1022.
- `Window::hit` / `button_boxes`: feed and skill always; rename only while not renamed.
- `SkillList::hit` returns `Skill(id)` on a box and `Raise(id)` on its level-up button only with points and an
  upgradable skill; `raise_boxes` lists those buttons.
- Starving: `starving_now(previous, hunger)` true once when hunger crosses to ≤ 10.

**Code**: `Assets`, `Window` (title 936, homuninfo_bg, eight values, bars, name/level/EXP/hunger/intimacy rows,
"vaporized"/"dead" text instead of bars — `ponytail:`), `SkillList` (title 937, 284 "Skill Point: %d", grid of
`skills::Assets` boxes with level-up buttons, like `guild.rs`). Layout off the background is `ponytail:`.

### T3 — Game wiring (`src/main.rs`, `src/actor.rs`, `src/control.rs`)

- `SpriteSet::single` searches `homun` for unit kind 8 (`single_in` helper; unit test of the folder order without
  data: `folders(kind)` returns the list).
- `Game`: `homunculus`, `homunculus_id`, `homunculus_skills`, window, skill list, `homunculus_steering`,
  `homunculus_asked` (rename question). Events update them; state 0 sets the ID; map change keeps the window but
  forgets data (resent).
- Alt+R toggles the window while there's a homunculus; Alt+T toggles following.
- Alt + right click (not a drag, not over a window): attack a monster / move to a cell; shares the mercenary code path.
- Follow tick: same as the mercenary's, for the homunculus.
- `use_skill` / `aim_skill` find homunculus skills: self skills at once on the homunculus, others aimed and sent
  without walking the player. Skill list raise → `skill_up`.
- Feed button → CZ_COMMAND_MER 1; `HomunculusFed { ok: false }` → 591 with the item's name (`ponytail:`).
  Hunger update ≤ 10 → 1018 once.
- Rename button → `Amount::name` (938, 23 bytes) → question 939 → `rename_homunculus`.
- `control.rs`: windows `Homunculus`, `HomunculusSkills`; button/skill targets; `state.game.homunculus` =
  `{id, name, level, hunger, intimacy, hp, skill_points, vaporized, following}` or null.

**Key tests**: control `state` builder covered by unit test where a pure helper exists (`homunculus_state`).

### T4 — Scenario

`.claude/skills/rustnarok-dev/scenarios/homunculus.json` exactly as the issue's "Done when" lists, and the
`SKILL.md` reset note adds the `homunculus`, `skill_homunculus` tables and `char.homun_id`. This environment has no
data.grf or rathena: the scenario is written and JSON-checked but can't be played here.

## Order of work

1. Plan (this file).
2. Subagent: tests for T1 + T2 (+ T3's pure helpers).
3. Subagent: code for T1–T3 until `cargo test`, `cargo clippy --release --all-targets -D warnings`, `cargo fmt` pass.
4. Parent: review, fix, scenario, verify, PR.
