# Issue #51 — Cash shop plan

Branch: `jpasuksmit/issue-51-cash-shop`

Add the cash shop: open/close and list/buy packets, a window with tabs + cart + points, menu button, unit tests, and `cash-shop.json`.

## Done when

- Encode each request and decode each answer (style of `decodes_mail` / bank tests in `src/net.rs`).
- Cart logic: amount 1–99, free points capped at cart total; tests need no data.grf.
- Window opens from basic-info menu `"shop"`, overlays report `CashShop`.
- Scenario `cash-shop.json` + `verify.sh` → `VERIFY: PASS`.

## Packet facts (PACKETVER 20220406)

| Packet | ID | Len | Role |
|---|---|---|---|
| CZ_SE_CASHSHOP_OPEN2 | `0x0B6D` | 6 | open, tab u32 @2 |
| ZC_SE_CASHSHOP_OPEN | `0x0B6E` | 14 | cash u32@2, free u32@6, tab u32@10 |
| (list request) | `0x08C9` | 2 | first open per session |
| ZC_ACK_SCHEDULER_CASHITEM | `0x08CA` | var | count i16@4, tab i16@6, items 8B from 8: id u32, price u32 |
| CZ_SE_PC_BUY_CASHITEM_LIST | `0x0848` | var | count u16@4, free u32@6, items 10B from 10: id u32, amount u32, tab u16 |
| ZC_SE_PC_BUY_CASHITEM_RESULT | `0x0849` | 16 | item u32@2, result u16@6, cash u32@8, free u32@12 |
| (close) | `0x084A` | 2 | window |

Tabs (`e_cash_shop_tab`): 0 New … 8 Sale. Messages: result→line map is `ponytail:` (issue table). Images under `cashshop\`; layout estimates marked `ponytail:`.

---

## Subtasks (TDD)

Each subtask lists **Key tests** first. Write those failing tests, then the code that makes them pass. No data.grf in new tests.

### A — Net packets (`src/net.rs`, `packet_lengths.txt` already has the IDs)

**Key tests** (`decodes_cash_shop` in `net.rs` tests):

1. `encode_cashshop_open(0)` → `[0x6D, 0x0B, 0,0,0,0]`; open with tab 3 encodes tab at offset 2.
2. `encode_cashshop_list_request()` → `[0xC9, 0x08]`.
3. `encode_cashshop_buy(free, &[(id, amount, tab), …])` length and fields match layout; empty list still has header.
4. `encode_cashshop_close()` → `[0x4A, 0x08]`.
5. Decode `0x0B6E` → `MapEvent::CashShopOpen { cash, free, tab }`.
6. Decode `0x08CA` with two items → `MapEvent::CashShopList { tab, items: Vec<(u32,u32)> }`; truncated / short packet → None.
7. Decode `0x0849` → `MapEvent::CashShopBuyResult { item, result, cash, free }`.
8. `packet_length` for the seven IDs matches the table (`-1` for var).

**Code after tests:**

- Constants + pure `encode_*` + `MapSession::{open_cashshop, request_cashshop_list, buy_cashshop, close_cashshop}`.
- `MapEvent` variants + `decode_map_packet` arms.

### B — Cart / points logic (`src/cashshop.rs` unit tests, no GPU)

**Key tests** (`cashshop::tests`):

1. Add to cart: amount clamped to 1..=99; reject amount 0 / 100+.
2. Same item+tab updates amount (or stacks per design); remove clears line.
3. `cart_total_price()` = sum(price * amount).
4. Free points to spend: typed/chosen value capped at `min(free_points, cart_total)`; cannot exceed total (would add cash on server).
5. Buy action only when cart non-empty and confirmation OK path returns `(free_to_spend, items)`.
6. On success result (0): cart cleared; points updated from packet.
7. Result→message mapping covers 0..12 (ponytail table).

**Code after tests:** pure state helpers on `CashShop` (cart, free spend, clear-on-success) before UI.

### C — Window UI (`src/cashshop.rs`)

**Key tests** (logic-level hits, still no GRF):

1. Tab with no items is disabled (click ignored / not selectable).
2. Tab with items selects and shows that list.
3. Close hit → `Action::Close`.

**Code after tests:** Assets (cashshop bitmaps), Hit/Action, draw, scrolling grid (pattern: `storage.rs` / `shop.rs`). Confirmation is a message box like `friend_asked` (message 1312). Labels from messages 3288–3298, 1854–1858, 1306.

### D — Wire-up (`basic_info.rs`, `main.rs`, `control.rs`)

**Key tests:** none new beyond existing compile/unit; scenario covers integration.

**Code:**

- Append `"shop"` to `MENU` (ponytail: official order unknown). Prefer `bt_shop` / `bt_shop_new` like other menu icons.
- On menu click: `open_cashshop(0)`; on `CashShopOpen` show window + points; first open in session also `request_cashshop_list`; accumulate `CashShopList` per tab; buy → confirm → `buy_cashshop`; close → `close_cashshop`.
- Result: print mapped message, update points, clear cart on success.
- `control.rs` overlays: `("CashShop", …)`.

### E — Scenario + verify

**Key checks** (scenario, not unit):

File: `.claude/skills/rustnarok-dev/scenarios/cash-shop.json`

- GM: `@itemreset`, `@cash -1000000`, `@cash 100` (items already in server `item_cash.yml`).
- Open shop → `wait_for` `0x0B6E` and `0x08CA` → `expect` `windows_open: ["CashShop"]` → shot.
- Put one Red Potion (501) in cart, buy, OK → `wait_for` `0x0849` → `expect` chat "Item purchase successful", `identified: [501]`, shot (90 cash left).
- Close → `expect` `windows_closed: ["CashShop"]`.

Then `.claude/skills/rustnarok-dev/verify.sh` → `VERIFY: PASS`. Visual check vs main when rathena + data present.

---

## Agent split

| Step | Who | Model |
|---|---|---|
| Plan (this file) | parent | — |
| A+B tests only (failing) | coding subagent | GPT Sol 6 medium (`gpt-5.6-sol-medium`; Luna 6 not in allowed list) |
| A+B+C+D implementation to pass those tests | coding subagent | same |
| Review / fix / scenario / verify | parent | — |
| PR → wait 15m → CI → merge if green | parent | — |

## Rules

Clean room (rathena only). No game data / screenshots in git. Plain short comments; `ponytail:` on estimates. `Closes #51` in PR body.
