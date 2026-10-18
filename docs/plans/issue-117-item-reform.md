# Issue #117 — Item reform window

## Problem

Reform boxes and NPCs open with `ZC_OPEN_REFORM_UI` (`0x0B8F`). `decode_map_packet` has no arm for it, so the packet hits the wildcard and is dropped. A reform box is also `ITEM_DELAY_CONSUME`, which `Game::use_item` does not send, so a double-click never asks the server to open the window.

## Protocol (rathena `packets_struct.hpp`, PACKETVER 20220406, little-endian)

| Packet | Bytes | Fields |
|---|---|---|
| `ZC_OPEN_REFORM_UI` `0x0B8F` | 6 | item id `u32` at 2 → `MapEvent::ReformOpen(u32)` |
| `CZ_ITEM_REFORM` `0x0B91` | 8 | item id `u32` at 2, index `u16` at 6. `encode_item_reform` + `MapSession::reform` |
| `ZC_ITEM_REFORM_ACK` `0x0B92` | 5 | index `u16` at 2, result `u8` at 4 (0 = done) → `MapEvent::Reformed { index, result }` |
| `CZ_CLOSE_REFORM_UI` `0x0B90` | 2 | `encode_close_reform` + `MapSession::close_reform` |

Indexes are `InventoryItem::index`. Result 0 closes the window. A map change closes it with no packet, like refine on `MapEvent::MapChange`. A reform item with no entries sends the close and opens nothing.

## Data (`src/lua.rs`)

`item_db_names(bytes)` reads `ItemDBNameTbl` into `HashMap<Vec<u8>, u32>` keyed by the Lua string's raw bytes (cp949 names are not UTF-8). The enchant window reuses this function and `ITEM_DB_NAMES`. A table stored as id → name is read the other way into the same map.

`reform_table(bytes, names)` reads `ReformItemList` / `ReformInfo` the way `achievement_table` reads a file's bytes. Names resolve through that map. `InformationString` is left out. Fields: base, result, materials, refine min/max, option count (stored; the client does not track random options), empty socket, preserve socket, preserve grade, change refine, random option code.

## Window (`src/reform.rs`)

`0x0B8F` opens the window and the inventory. Rows are carried items that are a base item, identified and not worn. The picked row shows materials (carried / needed, stacks summed), the refine range when it is not 0..=20, and the refine change. Lines, `ponytail:` which shows when: 3849 if sockets are not kept, 3871 if they are, 3646 when the item has a card and sockets must be empty.

Button 3847 "Reform" is on only when refine is inside the range, the item has no cards if sockets must be empty, and every material is carried. It sends `0x0B91`. Success inventory updates already exist; result 0 closes the window. No extra success string (`ponytail:`). Title is 3847. Layout and text colors are `ponytail:`. Pictures, if present: `itemreform/bg_reform.bmp`, `btn_reform_item.bmp` and `_over` / `_press`.

`control.rs`: `("ItemReform", ...)` in `overlays`. Rows are `pick_item` with the base item's id. Buttons `reform` and `close` (the title close), like the roulette window's `button_boxes`.

## Subtasks

Tests first, then the code. They share no file except `main.rs` and `control.rs`, which only T3 touches.

### T1 — Packets (`src/net.rs`)

**Test** `decodes_reform` (style of `decodes_mail`):

- `0x0B8F` with item `100817` → `ReformOpen(100817)`. Proves the open packet is no longer dropped.
- `0x0B92` index 3 result 0 → `Reformed { index: 3, result: 0 }`.
- `encode_item_reform(100817, 4)` is `[0x91, 0x0B, 0xD1, 0x89, 0x01, 0x00, 4, 0]`.
- `encode_close_reform()` is `[0x90, 0x0B]`.
- `packet_length` is 6, 2, 8, 5.

### T2 — List and button (`src/lua.rs`, `src/reform.rs`)

**Tests** (a small Lua source, no data.grf):

- Names resolve, including a cp949 key (Lua `\196\247`, bytes `C4 F7`, not UTF-8) to its item id, and that id's `ReformItemList` entry loads. An unknown name is skipped. Materials come out sorted by name.
- Listed rows are identified, unworn base items only.
- The button is on inside the refine range and off one step outside; off when `IsEmptySocket` and any card slot is set, on when sockets may hold cards; on when material stacks sum to the need, off when they fall short.

### T3 — Wiring (`src/main.rs`, `src/control.rs`, scenario)

- `use_item` sends `ITEM_DELAY_CONSUME` (the box).
- `ReformOpen` opens the window and the inventory, or sends `close_reform` when the item has no entries.
- `Reformed { result: 0 }` closes the window. `MapChange` clears it with no packet. Close sends `0x0B90`.
- Overlays, `pick_item` rows, `reform` / `close` buttons.
- Scenario `.claude/skills/rustnarok-dev/scenarios/item-reform.json` as the issue describes (open the inventory with Alt+E first, so the box can be double-clicked).
