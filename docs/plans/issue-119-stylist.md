# Issue #119 — Stylist shop

## Problem

`@stylist` and `openstylist()` send ZC_UI_OPEN `0x0AE2`. The client never decodes it, so the talk ends and no window opens. The packet carries no catalogue: styles, colors and prices are the client's Lua tables, and the buy request names rows of those tables.

## Protocol (rathena `rustnarok`, PACKETVER 20220406)

From `src/map/packets_struct.hpp`, `clif.cpp` (`clif_ui_open`, `clif_parse_stylist_buy`, `clif_stylist_response`, `clif_parse_stylist_close`) and `clif_packetdb.hpp`.

| Packet | Bytes | Layout |
|---|---|---|
| ZC_UI_OPEN `0x0AE2` | 7 | kind u8 at 2 (1 stylist, 8 grade enchant — out of scope), data u32 at 3 |
| ZC_STYLE_CHANGE_RES `0x0A47` | 3 | failed u8 at 2 (0 done, 1 failed) |
| CZ_REQ_STYLE_CHANGE2 `0x0AFC` | 16 | u16 row numbers, 0 = no change: hair color 2, hair style 4, clothes color 6, upper 8, middle 10, lower 12, body style 14 |
| CZ_REQ_STYLE_CLOSE `0x0A48` | 2 | header only |

`clif_parse_stylist_buy` reads every style request as the 16-byte packet on this PACKETVER. Send `0x0AFC` only, never the 14-byte `0x0A46`.

rathena applies hair color, hair style, clothes color, upper, middle, lower, then body style. Each part is paid before it is applied. A failure stops there: earlier parts stay done and paid, and the answer is failed=1 with the window still open. Success (`failed` 0) clears `stylist_open` and the client closes the window. The close packet clears `stylist_open` without an answer. A map change forgets it in `pc_setpos` with no packet, the same way the refine window goes on `MapEvent::MapChange`.

Body style is taken only from third jobs (not fourth) and changes LOOK_BODY2, which this client does not draw. The tab stays off and the request sends 0.

Headgear is mailed (RODEX from "Styling Shop"). Mail tag expansion is not this issue. The scenario buys no headgear.

## Lua

`stylingshopinfo_f.lub` then `stylingshopinfo.lub` run in a Lua VM and fill `StylingShop`. Read the tables; do not call into an official client. Row fields: `goods`, `payment` (`zeny`, or `itid` plus `boxitid`), optional `isDoram` and `isColoring`. The row's number (the table key the append functions assign) is what the request sends. `goods` is the look: a color, a style number, or a headgear item id. Clothes-color row 1 has goods 0.

Tables: HeadPalette, HeadStyle, DoramHeadStyle, BodyPalette, DoramBodyPalette, TopAccessory, MidAccessory, BottomAccessory, BodyStyle. Leave CostumeChange out.

Doram jobs 4218, 4220 and 4308 (`MAPID_SUMMONER`) use DoramHeadStyle and DoramBodyPalette. Hair colors stay HeadPalette. A hair style with `isColoring` false cannot take a hair color (message 2834).

## Window

Kind 1 of `MapEvent::UiOpen` opens it. Preview is `Look` with the picks put in (headgear view is iteminfo `ClassNum`), drawn with `SpriteSet::player` the way character creation draws its figure. The real look does not change until `MapEvent::LookChange`. Rotate (2174, 2175), per-row undo (click the row again) and refresh (clear every pick). Tabs: hair (2764, 2763, 2765) and accessory (2831). Favorites left out. Second costume drawn from `tab_2ndcostume_disable` and not clickable.

Buy checks, in order, so a refusal sends nothing: a hair color on a style that cannot be dyed (2834), zeny of the picks (2968), each coupon or its box (2814). Then `MapSession::buy_style`. Success `0x0A47` failed=0 closes the window and says message 2836. failed=1 leaves it open. ponytail: no line fits a failure, so the result is printed plainly. The close button sends `0x0A48`.

Where things sit and which image goes where is not in the sources. ponytail: estimated against `bg_styling.bmp` (480×330) and the named bitmaps under `styleshop\`.

## Control socket

`player.look`: hair, hair_color, clothes_color, head_top, head_mid, head_bottom (the real look). `Stylist` in the window list. Targets: `hair_style_<row>`, `hair_color_<row>`, `clothes_color_<row>`, `tab_hair`, `tab_accessory`, `buy`, `refresh`, `rotate_left`, `rotate_right`, `close`.

## Scenario

`.claude/skills/rustnarok-dev/scenarios/stylist.json` as the issue specifies. No headgear purchase, so the scenario reset needs nothing new. This VM has no data.grf and no login server: the scenario is written and not played here.

## Subtasks (TDD)

Each subtask: tests first, then the code that makes them pass.

### T1 — Packets (`src/net.rs`)

**Key tests** (`decodes_the_stylist`, in the style of `decodes_mail`)

- `0x0AE2` kind 1 data 0, and kind 8 data 4, → `MapEvent::UiOpen { kind, data }`.
- `0x0A47` byte 0 → `StyleChanged(false)`; byte 1 → `StyleChanged(true)`.
- `encode_style_change` is 16 bytes, header `0x0AFC`, rows at 2, 4, 6, 8, 10, 12, 14, zeros elsewhere. Not `0x0A46`.
- `encode_style_close` is `[0x48, 0x0A]`.
- `packet_length` of the four ids is 7, 3, 16, 2.

**Code**: constants, `StylePicks`, events, decode arms, encoders, `MapSession::{buy_style, close_stylist}`.

### T2 — Lua (`src/lua.rs`)

**Key tests**

- A small source assigning `StylingShop` tables: goods, zeny, itid, boxitid, isDoram, isColoring; the key is the row; CostumeChange is not a loaded table; BodyPalette row 1 can have goods 0.
- Two sources (Add* functions, then calls) fill the same shape, so the real files can be executed.

**Code**: `StyleRow`, `StylingShop`, `styling_shop(&[Vec<u8>])`, `load_styling_shop(grf)` running the framework then the lists.

### T3 — Preview, price, refusals (`src/stylist.rs`)

**Key tests** (no data.grf)

- Preview look: picks replace hair, hair color, clothes color (the row's goods, so clothes row 2 can be color 2 and row 1 color 0) and headgear view from `ClassNum`. Unpicked parts stay. The real `Look` is not written.
- Doram jobs 4218, 4220, 4308 use DoramHeadStyle and DoramBodyPalette; other jobs use HeadStyle and BodyPalette. Hair colors stay HeadPalette.
- Price is the picks' zeny; coupons are `(itid, boxitid)` of the item rows.
- Refusals: isColoring false with a hair color → 2834 (the picked style, or the current hair's row when no style is picked); zeny short → 2968; missing coupon and its box → 2814; a box alone is enough; two picks that need the same coupon need two of them. A style that cannot be dyed is fine when no hair color is picked.
- `Picks::request` sends body style 0.
- Button names for the hair tab include `hair_style_<row>`, `hair_color_<row>`, `clothes_color_<row>`, `tab_hair`, `tab_accessory`, `buy`, `refresh`, `rotate_left`, `rotate_right`, `close`. Clicking a row again clears it. Refresh clears every pick. Buy with a refusal does not produce a request.

### T4 — Session flow and drawing (`src/main.rs`, `src/stylist.rs`, `src/control.rs`)

- Kind 1 opens the window and dresses the preview from the real look. Other kinds do not.
- Buy sends `0x0AFC` only after the checks. Close sends `0x0A48` and drops the window.
- `StyleChanged(false)` closes it and says message 2836. `StyleChanged(true)` leaves it open and prints the result plainly.
- Map change drops it with no packet.
- The preview sprite is `SpriteSet::player` and turns with the rotate buttons. The player's actor is not rebuilt from a pick.
- `game_state`: `player.look`, `Stylist` in the windows, the button targets.
- Second-costume tab is drawn disabled and is not a target.

**Key test**: `player_look` in `control.rs`.

### T5 — Scenario

`.claude/skills/rustnarok-dev/scenarios/stylist.json` exactly as the issue's "Done when" lists. No new scenario-reset SQL.
