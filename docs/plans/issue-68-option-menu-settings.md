# Issue #68 — Game option menu: Settings and BM/Shortcut Settings do nothing

## Problem

Esc → Game option: **Character Select**, **Sound**, **Exit**, **Return to game** work.
**Settings** (`Choice::Graphics`) and **BM/Shortcut Settings** (`Choice::Hotkeys`) ignore clicks
(`main.rs`: ponytail — windows aren't made yet). Control-socket `windows` stays unchanged.

## Goal

Match the client's own windows as far as we support them:

1. **Settings** → graphics/game settings window (at least Effects, hanging off `/effect`).
2. **BM/Shortcut Settings** → shortcut-key window (Skill Bar bindings that drive F-key / remapped keys).

Not in this PR: full Interface/Emotions rebinding tabs, every official graphics toggle
(shadows, mist, NoCtrl, …) we don't already have a hook for. Those stay `ponytail:`.

## Protocol / facts

- UI only; no new map packets.
- Audio settings (`options::AudioWindow`) is the pattern: Esc closes the menu, opens a framed window,
  control reports it, unit tests cover hit/click without data.grf.
- Message ids (ponytail until checked against msgstringtable): graphics title near audio's 1485;
  shortcuts title next to it. Prefer numbers over hard-coded English when a nearby id exists.

## Subtasks (TDD)

Each subtask is self-contained: **write the failing test first**, then the code.

### T1 — Graphics settings window (unit)

**Key tests**

- `GraphicsWindow::new` is open; `hit` on the Effects checkbox returns `Hit::Check(Effects)`.
- `click(Check(Effects))` flips a `GraphicsSettings { effects_on }` value.
- `click(Title(Close))` clears `frame.open`.
- `GraphicsSettings::load` / `save` round-trip via a temp `settings_path`-style file (or inject path in test).

**Code**

- Extend `src/options.rs` (same module as audio): `GraphicsSettings`, `GraphicsWindow`, assets reuse
  checkboxes from audio assets where possible.
- Persist under `settings_path("graphics")` (same layout as `sound`).

### T2 — Shortcut (BM) settings window (unit)

**Key tests**

- Window opens; Skill Bar lists slots `0..8` (row 1) with default keys F1–F9.
- `click` on a row selects it for rebinding; `bind(key)` assigns that key to the slot and clears
  selection; assigning the same key to another slot clears the previous slot.
- `ShortcutSettings::slot_for(key)` returns the bound slot; defaults match today's F1–F9 → 0..8.
- Load/save round-trip of bindings.

**Code**

- `ShortcutSettings` + `ShortcutsWindow` in `src/options.rs` (or `src/shortcuts.rs` if it grows).
- Persist under `settings_path("shortcuts")`.
- Battle Mode / Interface / Emotions tabs: not drawn yet (`ponytail`); Skill Bar row 1 is enough to
  make the Esc button open a real window players can use.

### T3 — Wire Esc menu + App state (integration-style unit where possible)

**Key tests**

- Prefer pure helpers if extractable: e.g. `fn open_graphics_from_escape(...)` is overkill;
  instead cover through existing patterns:
  - control socket lists `GraphicsSettings` / `ShortcutSettings` when the matching
    `Option<…Window>` is `Some` (mirror `AudioSettings` in `control.rs` — test via a small
    helper that builds the windows list, or document that control coverage is manual/scenario).
- Unit: clicking Graphics/Hotkeys must not be a no-op in the match arms — assert by testing that
  constructing the windows from those choices is what Sound already does (code review + the
  window tests above). Add a focused test only if a pure function is extracted.

**Code**

- `Game`: `graphics_window: Option<…>`, `shortcuts_window: Option<…>`.
- `Client`/`App`: keep `effects_on` in sync with `GraphicsSettings` (load at start, save on toggle;
  `/effect` updates the same flag and persists).
- Esc `Choice::Graphics` / `Choice::Hotkeys`: close escape, open the window (same as Sound).
- Pointer routing, draw, drag — copy the audio_window paths.
- F1–F9 (and remapped keys): use `ShortcutSettings::slot_for` instead of a hard-coded F-key list.
- `control.rs` windows: `"GraphicsSettings"`, `"ShortcutSettings"`.
- Remove the ponytail comment; update README “Not done yet” / Windows row.

### T4 — Scenario (when data + rathena available)

**Key tests / expect**

- Scenario JSON: Esc → click Settings → `windows_open: ["GraphicsSettings"]`, Escape menu closed;
  Esc → BM/Shortcut → `windows_open: ["ShortcutSettings"]`.
- Skip gracefully if this environment has no data.grf / login (CI has no game data).

## Order of work

1. Plan (this file).
2. Subagent: **tests only** for T1 + T2 (must fail / not compile until stubs exist — prefer compile with
   `todo!()` stubs only if needed for the test module to typecheck; otherwise write tests against the
   intended API and let them fail to compile, then stubs in the code pass).
3. Subagent: **implementation** for T1–T3 until `cargo test --release` and clippy are green.
4. Parent: review, fix, `verify.sh` as far as the environment allows, commit, PR.

## Out of scope

- Greying or removing the buttons (fallback in the issue) — we implement windows instead.
- Full official graphics option list; Interface/Emotions shortcut tabs; Battle Mode chat hijack.
