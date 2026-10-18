//! A connected map server's requests: a `MapSession` method per packet the client sends.

use super::*;

impl MapSession {
    pub fn connect(ticket: &MapTicket, client_tick: u32, mut on_event: impl FnMut(MapEvent) + Send + 'static) -> io::Result<Self> {
        let mut stream = TcpStream::connect((Ipv4Addr::from(ticket.address.0), ticket.address.1))?;
        let mut enter = Vec::with_capacity(23);
        enter.extend_from_slice(&CZ_ENTER.to_le_bytes());
        for value in [ticket.account_id, ticket.character_id, ticket.login_id1, client_tick, 0] {
            enter.extend_from_slice(&value.to_le_bytes());
        }
        enter.push(ticket.sex);
        stream.write_all(&enter)?;

        let mut reader = stream.try_clone()?;
        std::thread::spawn(move || {
            let error = loop {
                match read_packet(&mut reader) {
                    Ok(packet) => {
                        if let Some(event) = decode_or_skip(&packet, decode_map_packet) {
                            on_event(event);
                        }
                    }
                    Err(error) => break error,
                }
            };
            on_event(MapEvent::Disconnected(error));
        });
        Ok(Self { stream: Wire(stream) })
    }

    /// Tells the server the map is loaded; it then sends everything in view.
    pub fn loaded(&mut self) -> io::Result<()> {
        self.stream.write_all(&CZ_NOTIFY_ACTORINIT.to_le_bytes())
    }

    pub fn walk(&mut self, to: [u16; 2]) -> io::Result<()> {
        let mut packet = CZ_REQUEST_MOVE.to_le_bytes().to_vec();
        packet.extend_from_slice(&encode_position(to, 0));
        self.stream.write_all(&packet)
    }

    /// Moves an item into the favourites, or (when it's there now: `favorite`) out of them.
    pub fn move_favorite(&mut self, index: u16, favorite: bool) -> io::Result<()> {
        self.stream.write_all(&encode_favorite(index, favorite))
    }

    /// Identifies an item with a Magnifier from the inventory (rathena uses one up).
    pub fn identify_one(&mut self, index: u16) -> io::Result<()> {
        self.stream.write_all(&encode_index(CZ_REQ_ONECLICK_ITEMIDENTIFY, index))
    }

    /// Stops attacking (CZ_CANCEL_LOCKON).
    pub fn stop_attack(&mut self) -> io::Result<()> {
        self.stream.write_all(&CZ_CANCEL_LOCKON.to_le_bytes())
    }

    /// Turns the character (rathena's direction: 0 north, counterclockwise), the head straight (0) or to a side.
    pub fn change_direction(&mut self, direction: u8, head: u8) -> io::Result<()> {
        self.stream.write_all(&encode_direction(direction, head))
    }

    /// Keeps the connection alive; the client sends this every few seconds.
    pub fn tick(&mut self, client_tick: u32) -> io::Result<()> {
        let mut packet = CZ_REQUEST_TIME.to_le_bytes().to_vec();
        packet.extend_from_slice(&client_tick.to_le_bytes());
        self.stream.write_all(&packet)
    }

    /// Clicking an NPC starts its script.
    pub fn talk(&mut self, npc: u32) -> io::Result<()> {
        let mut packet = CZ_CONTACTNPC.to_le_bytes().to_vec();
        packet.extend_from_slice(&npc.to_le_bytes());
        packet.push(1);
        self.stream.write_all(&packet)
    }

    pub fn next(&mut self, npc: u32) -> io::Result<()> {
        self.send_id(CZ_REQ_NEXT_SCRIPT, npc)
    }

    pub fn close_dialog(&mut self, npc: u32) -> io::Result<()> {
        self.send_id(CZ_CLOSE_DIALOG, npc)
    }

    /// Picks menu item `choice` (1-based); 255 cancels.
    pub fn choose(&mut self, npc: u32, choice: u8) -> io::Result<()> {
        let mut packet = CZ_CHOOSE_MENU.to_le_bytes().to_vec();
        packet.extend_from_slice(&npc.to_le_bytes());
        packet.push(choice);
        self.stream.write_all(&packet)
    }

    pub fn input_number(&mut self, npc: u32, value: i32) -> io::Result<()> {
        let mut packet = CZ_INPUT_EDITDLG.to_le_bytes().to_vec();
        packet.extend_from_slice(&npc.to_le_bytes());
        packet.extend_from_slice(&value.to_le_bytes());
        self.stream.write_all(&packet)
    }

    pub fn input_text(&mut self, npc: u32, text: &str) -> io::Result<()> {
        let text = encode_text(text);
        let mut packet = CZ_INPUT_EDITDLGSTR.to_le_bytes().to_vec();
        packet.extend_from_slice(&((8 + text.len() + 1) as u16).to_le_bytes());
        packet.extend_from_slice(&npc.to_le_bytes());
        packet.extend_from_slice(&text);
        packet.push(0);
        self.stream.write_all(&packet)
    }

    /// Attacks (`ACT_ATTACK`, `ACT_ATTACK_REPEAT`) or otherwise acts on a unit.
    pub fn act(&mut self, target: u32, action: u8) -> io::Result<()> {
        let mut packet = CZ_REQUEST_ACT.to_le_bytes().to_vec();
        packet.extend_from_slice(&target.to_le_bytes());
        packet.push(action);
        self.stream.write_all(&packet)
    }

    /// The player's progress bar ended: its time is up, or the server took it away.
    pub fn progress_done(&mut self) -> io::Result<()> {
        self.stream.write_all(&CZ_PROGRESS.to_le_bytes())
    }

    pub(crate) fn send_id(&mut self, header: u16, id: u32) -> io::Result<()> {
        let mut packet = header.to_le_bytes().to_vec();
        packet.extend_from_slice(&id.to_le_bytes());
        self.stream.write_all(&packet)
    }

    /// Picks up an item lying within two cells.
    pub fn pick_up(&mut self, item: u32) -> io::Result<()> {
        self.send_id(CZ_ITEM_PICKUP, item)
    }

    pub fn wear(&mut self, index: u16, location: u32) -> io::Result<()> {
        let mut packet = CZ_REQ_WEAR_EQUIP.to_le_bytes().to_vec();
        packet.extend_from_slice(&index.to_le_bytes());
        packet.extend_from_slice(&location.to_le_bytes());
        self.stream.write_all(&packet)
    }

    /// Answers a shop's question: buy (false) or sell (true).
    pub fn choose_deal(&mut self, npc: u32, sell: bool) -> io::Result<()> {
        let mut packet = CZ_ACK_SELECT_DEALTYPE.to_le_bytes().to_vec();
        packet.extend_from_slice(&npc.to_le_bytes());
        packet.push(u8::from(sell));
        self.stream.write_all(&packet)
    }

    /// Buys from the open shop: (item, amount) pairs.
    pub fn buy(&mut self, items: &[(u32, u16)]) -> io::Result<()> {
        let mut packet = CZ_PC_PURCHASE_ITEMLIST.to_le_bytes().to_vec();
        packet.extend_from_slice(&((4 + items.len() * 6) as u16).to_le_bytes());
        for &(item, amount) in items {
            packet.extend_from_slice(&amount.to_le_bytes());
            packet.extend_from_slice(&item.to_le_bytes());
        }
        self.stream.write_all(&packet)
    }

    /// Sells to the open shop: (inventory index, amount) pairs.
    pub fn sell(&mut self, items: &[(u16, u16)]) -> io::Result<()> {
        let mut packet = CZ_PC_SELL_ITEMLIST.to_le_bytes().to_vec();
        packet.extend_from_slice(&((4 + items.len() * 4) as u16).to_le_bytes());
        for &(index, amount) in items {
            packet.extend_from_slice(&index.to_le_bytes());
            packet.extend_from_slice(&amount.to_le_bytes());
        }
        self.stream.write_all(&packet)
    }

    /// Moves an inventory item into the open storage.
    pub fn put_in_storage(&mut self, index: u16, amount: u32) -> io::Result<()> {
        self.move_stored(CZ_MOVE_ITEM_FROM_BODY_TO_STORE, index, amount)
    }

    /// Takes a stored item back into the inventory.
    pub fn take_from_storage(&mut self, index: u16, amount: u32) -> io::Result<()> {
        self.move_stored(CZ_MOVE_ITEM_FROM_STORE_TO_BODY, index, amount)
    }

    pub(crate) fn move_stored(&mut self, header: u16, index: u16, amount: u32) -> io::Result<()> {
        let mut packet = header.to_le_bytes().to_vec();
        packet.extend_from_slice(&index.to_le_bytes());
        packet.extend_from_slice(&amount.to_le_bytes());
        self.stream.write_all(&packet)
    }

    pub fn close_storage(&mut self) -> io::Result<()> {
        self.stream.write_all(&CZ_CLOSE_STORE.to_le_bytes())
    }

    /// Puts a skill point into a skill.
    pub fn skill_up(&mut self, skill: u16) -> io::Result<()> {
        let mut packet = CZ_UPGRADE_SKILLLEVEL.to_le_bytes().to_vec();
        packet.extend_from_slice(&skill.to_le_bytes());
        self.stream.write_all(&packet)
    }

    /// Uses a skill on a unit (oneself included).
    pub fn use_skill(&mut self, skill: u16, level: u16, target: u32) -> io::Result<()> {
        let mut packet = CZ_USE_SKILL.to_le_bytes().to_vec();
        packet.extend_from_slice(&level.to_le_bytes());
        packet.extend_from_slice(&skill.to_le_bytes());
        packet.extend_from_slice(&target.to_le_bytes());
        self.stream.write_all(&packet)
    }

    /// Uses a skill on a cell.
    /// A ground skill with a message: a Talkie Box.
    pub fn use_skill_at_with_text(&mut self, skill: u16, level: u16, cell: [u16; 2], text: &str) -> io::Result<()> {
        self.stream.write_all(&encode_skill_at_with_text(skill, level, cell, text))
    }

    pub fn use_skill_at(&mut self, skill: u16, level: u16, cell: [u16; 2]) -> io::Result<()> {
        let mut packet = CZ_USE_SKILL_TOGROUND.to_le_bytes().to_vec();
        packet.extend_from_slice(&level.to_le_bytes());
        packet.extend_from_slice(&skill.to_le_bytes());
        packet.extend_from_slice(&cell[0].to_le_bytes());
        packet.extend_from_slice(&cell[1].to_le_bytes());
        self.stream.write_all(&packet)
    }

    /// Keeps a hotkey bar's rotation on the server.
    pub fn rotate_hotkeys(&mut self, tab: u16, rowshift: u8) -> io::Result<()> {
        self.stream.write_all(&encode_hotkey_rotation(tab, rowshift))
    }

    /// Puts something on a hotkey slot (or clears it).
    pub fn set_hotkey(&mut self, tab: u16, index: u16, key: Option<Hotkey>) -> io::Result<()> {
        let key = key.unwrap_or(Hotkey { skill: false, id: 0, count: 0 });
        let mut packet = CZ_SHORTCUT_KEY_CHANGE.to_le_bytes().to_vec();
        packet.extend_from_slice(&tab.to_le_bytes());
        packet.extend_from_slice(&index.to_le_bytes());
        packet.push(u8::from(key.skill));
        packet.extend_from_slice(&key.id.to_le_bytes());
        packet.extend_from_slice(&key.count.to_le_bytes());
        self.stream.write_all(&packet)
    }

    pub fn take_off(&mut self, index: u16) -> io::Result<()> {
        let mut packet = CZ_REQ_TAKEOFF_EQUIP.to_le_bytes().to_vec();
        packet.extend_from_slice(&index.to_le_bytes());
        self.stream.write_all(&packet)
    }

    pub fn use_item(&mut self, index: u16, account_id: u32) -> io::Result<()> {
        let mut packet = CZ_USE_ITEM.to_le_bytes().to_vec();
        packet.extend_from_slice(&index.to_le_bytes());
        packet.extend_from_slice(&account_id.to_le_bytes());
        self.stream.write_all(&packet)
    }

    /// Public chat; the client sends "name : message".
    /// Shows an emotion over the player's head.
    /// The bank's requests carry the player's own account: opening, the balance, closing.
    pub fn open_bank(&mut self, account: u32) -> io::Result<()> {
        self.send_id(CZ_REQ_OPEN_BANKING, account)
    }

    pub fn check_bank(&mut self, account: u32) -> io::Result<()> {
        self.send_id(CZ_REQ_BANKING_CHECK, account)
    }

    pub fn close_bank(&mut self, account: u32) -> io::Result<()> {
        self.send_id(CZ_REQ_CLOSE_BANKING, account)
    }

    pub fn open_cashshop(&mut self, tab: u32) -> io::Result<()> {
        self.stream.write_all(&encode_cashshop_open(tab))
    }

    pub fn request_cashshop_list(&mut self) -> io::Result<()> {
        self.stream.write_all(&encode_cashshop_list_request())
    }

    pub fn buy_cashshop(&mut self, free: u32, items: &[(u32, u32, u16)]) -> io::Result<()> {
        self.stream.write_all(&encode_cashshop_buy(free, items))
    }

    pub fn close_cashshop(&mut self) -> io::Result<()> {
        self.stream.write_all(&encode_cashshop_close())
    }

    /// Puts zeny in the bank, or takes it out.
    pub fn move_zeny(&mut self, deposit: bool, account: u32, zeny: i32) -> io::Result<()> {
        self.stream.write_all(&encode_bank_move(deposit, account, zeny))
    }

    /// Asks to see a player's equipment (by account ID); they must be on the same map.
    pub fn view_equipment(&mut self, account: u32) -> io::Result<()> {
        self.send_id(CZ_EQUIPWIN_MICROSCOPE, account)
    }

    /// The Show Equipment box: whether others may see what the player wears.
    /// The pet window's automatic feeding box.
    pub fn pet_auto_feed(&mut self, on: bool) -> io::Result<()> {
        self.stream.write_all(&encode_config(CONFIG_PET_AUTOFEED, on))
    }

    pub fn show_equipment(&mut self, shown: bool) -> io::Result<()> {
        self.stream.write_all(&encode_config(CONFIG_OPEN_EQUIPMENT, shown))
    }

    /// The inventory's expansion: may it grow, yes, no (the last needs no answer).
    pub fn ask_expansion(&mut self) -> io::Result<()> {
        self.stream.write_all(&CZ_REQ_OPEN_MSGBOX_EXTEND_BODYITEM_SIZE.to_le_bytes())
    }

    pub fn answer_expansion(&mut self, yes: bool) -> io::Result<()> {
        self.stream.write_all(&if yes { CZ_REQ_EXTEND_BODYITEM_SIZE } else { CZ_CLOSE_MSGBOX_EXTEND_BODYITEM_SIZE }.to_le_bytes())
    }

    pub fn market_buy(&mut self, items: &[(u32, u32)]) -> io::Result<()> {
        self.stream.write_all(&encode_market_buy(items))
    }

    pub fn barter_buy(&mut self, items: &[(u32, u32, u16, u32)]) -> io::Result<()> {
        self.stream.write_all(&encode_barter_buy(items))
    }

    pub fn expanded_barter_buy(&mut self, items: &[(u32, u32, u32)]) -> io::Result<()> {
        self.stream.write_all(&encode_expanded_barter_buy(items))
    }

    /// A market's or barter's window closed: until the server hears it, it keeps the player busy.
    pub fn close_market(&mut self, kind: MarketKind) -> io::Result<()> {
        let header = match kind {
            MarketKind::Market => CZ_NPC_MARKET_CLOSE,
            MarketKind::Barter => CZ_NPC_BARTER_MARKET_CLOSE,
            MarketKind::Expanded => CZ_NPC_EXPANDED_BARTER_MARKET_CLOSE,
        };
        self.stream.write_all(&header.to_le_bytes())
    }

    /// Answers a making list: the item, or 0 to give it up (rathena then forgets the list).
    pub fn make(&mut self, item: u32) -> io::Result<()> {
        self.stream.write_all(&encode_make(item))
    }

    pub fn cook(&mut self, kind: u16, item: u32) -> io::Result<()> {
        self.stream.write_all(&encode_cook(kind, item))
    }

    pub fn make_arrow(&mut self, item: u32) -> io::Result<()> {
        self.stream.write_all(&encode_make_arrow(item))
    }

    /// Answers Repair Weapon's list with an entry, or none.
    pub fn repair(&mut self, item: Option<&InventoryItem>) -> io::Result<()> {
        self.stream.write_all(&encode_repair(item))
    }

    /// Answers Upgrade Weapon's list with a weapon's inventory index, or 0 for none.
    pub fn upgrade_weapon(&mut self, index: u32) -> io::Result<()> {
        self.stream.write_all(&encode_upgrade(index))
    }

    pub fn select_autospell(&mut self, skill: u32) -> io::Result<()> {
        self.stream.write_all(&encode_autospell(skill))
    }

    pub fn select_skill(&mut self, why: u32, skill: u16) -> io::Result<()> {
        self.stream.write_all(&encode_skill_select(why, skill))
    }

    /// The refine window: put an item in (its ores come back), refine it, close the window.
    pub fn refine_select(&mut self, index: u16) -> io::Result<()> {
        self.stream.write_all(&[CZ_REFINING_SELECT_ITEM.to_le_bytes(), index.to_le_bytes()].concat())
    }

    pub fn refine(&mut self, index: u16, ore: u32, blessings: bool) -> io::Result<()> {
        self.stream.write_all(&encode_refine(index, ore, blessings))
    }

    pub fn close_refine(&mut self) -> io::Result<()> {
        self.stream.write_all(&CZ_CLOSE_REFINING_UI.to_le_bytes())
    }

    /// Puts an item in the grade window. Its materials come back, or index −1 when that level has no grades.
    pub fn grade_select(&mut self, index: u16) -> io::Result<()> {
        self.stream.write_all(&encode_grade_select(index))
    }

    /// Starts a grade try. `steps` 0 sends blessings off; more than 0 sends them on with that many steps.
    pub fn grade_enchant(&mut self, index: u16, material: u32, steps: u32) -> io::Result<()> {
        self.stream.write_all(&encode_grade_enchant(index, material, steps))
    }

    /// Closes the grade window. Until this, the server holds the player still.
    pub fn close_grade(&mut self) -> io::Result<()> {
        self.stream.write_all(&encode_close_grade())
    }

    /// Sends the synthesis window's materials. Each entry is an inventory index and the amount the list wants.
    pub fn synthesize(&mut self, box_id: u32, entries: &[(u16, u16)]) -> io::Result<()> {
        self.stream.write_all(&encode_synthesis(box_id, entries))
    }

    pub fn close_synthesis(&mut self) -> io::Result<()> {
        self.stream.write_all(&encode_synthesis_close())
    }

    /// Sends the item in the upgrade window.
    pub fn upgrade_item(&mut self, box_id: u32, index: u16) -> io::Result<()> {
        self.stream.write_all(&encode_upgrade_item(box_id, index))
    }

    pub fn close_upgrade(&mut self) -> io::Result<()> {
        self.stream.write_all(&encode_upgrade_close())
    }

    /// Asks to reform the item at `index` with the reform item the window was opened for.
    pub fn reform(&mut self, item: u32, index: u16) -> io::Result<()> {
        self.stream.write_all(&encode_item_reform(item, index))
    }

    /// Tells the server the reform window closed. Until it hears this, it holds the player.
    pub fn close_reform(&mut self) -> io::Result<()> {
        self.stream.write_all(&encode_close_reform())
    }

    /// The enchant window: a random enchant, a chosen perfect one, an upgrade, a reset, or closing it.
    pub fn enchant_random(&mut self, table: u64, index: u16) -> io::Result<()> {
        self.stream.write_all(&encode_enchant_random(table, index))
    }

    pub fn enchant_perfect(&mut self, table: u64, index: u16, item: u32) -> io::Result<()> {
        self.stream.write_all(&encode_enchant_perfect(table, index, item))
    }

    pub fn enchant_upgrade(&mut self, table: u64, index: u16, slot: u16) -> io::Result<()> {
        self.stream.write_all(&encode_enchant_upgrade(table, index, slot))
    }

    pub fn enchant_reset(&mut self, table: u64, index: u16) -> io::Result<()> {
        self.stream.write_all(&encode_enchant_reset(table, index))
    }

    pub fn close_enchant(&mut self) -> io::Result<()> {
        self.stream.write_all(&encode_enchant_close())
    }

    /// Buys the stylist's rows (0 leaves a part as it is). Always the 16-byte request.
    pub fn buy_style(&mut self, picks: StylePicks) -> io::Result<()> {
        self.stream.write_all(&encode_style_change(picks))
    }

    /// Closes the stylist. Until this, the server holds the player and refuses another one.
    pub fn close_stylist(&mut self) -> io::Result<()> {
        self.stream.write_all(&encode_style_close())
    }

    /// Hatches the egg at this inventory index (from `PetEggs`).
    pub fn hatch(&mut self, index: u16) -> io::Result<()> {
        self.stream.write_all(&encode_hatch(index))
    }

    /// Dismisses the player's mercenary (MERCENARY_DISMISS); the server answers nothing but the unit vanishing.
    pub fn mercenary_command(&mut self, command: u8) -> io::Result<()> {
        let [low, high] = CZ_MER_COMMAND.to_le_bytes();
        self.stream.write_all(&[low, high, command])
    }

    /// One of the HOMUNCULUS_* commands (feed) to the player's homunculus.
    pub fn homunculus_command(&mut self, command: u8) -> io::Result<()> {
        self.stream.write_all(&encode_homunculus_command(command))
    }

    /// Names the player's homunculus (once).
    pub fn rename_homunculus(&mut self, name: &str) -> io::Result<()> {
        self.stream.write_all(&encode_rename_homunculus(name))
    }

    /// Sends the player's mercenary (or homunculus) to a cell.
    pub fn move_npc(&mut self, id: u32, cell: [u16; 2]) -> io::Result<()> {
        self.stream.write_all(&encode_move_npc(id, cell))
    }

    /// Sends the player's mercenary (or homunculus) at a target; `action` other than 0 keeps attacking.
    pub fn act_npc(&mut self, id: u32, target: u32, action: u8) -> io::Result<()> {
        self.stream.write_all(&encode_act_npc(id, target, action))
    }

    /// Brings the player's mercenary (or homunculus) back next to the player.
    pub fn move_to_owner(&mut self, id: u32) -> io::Result<()> {
        self.send_id(CZ_REQUEST_MOVETOOWNER, id)
    }

    /// One of the PET_* commands (PET_INFO and on) to the player's pet.
    pub fn pet_command(&mut self, command: u8) -> io::Result<()> {
        self.stream.write_all(&encode_pet_command(command))
    }

    pub fn rename_pet(&mut self, name: &str) -> io::Result<()> {
        self.stream.write_all(&encode_rename_pet(name))
    }

    /// Has the player's pet emote or talk; the server shows it to everyone around.
    pub fn pet_act(&mut self, act: &PetAct) -> io::Result<()> {
        self.stream.write_all(&encode_pet_act(act))
    }

    /// Tries to tame this monster (after `CaptureStart`).
    pub fn try_capture(&mut self, monster: u32) -> io::Result<()> {
        self.stream.write_all(&encode_try_capture(monster))
    }

    /// Answers the material window: what's in with OK, or a cancel.
    pub fn send_materials(&mut self, kind: u32, ok: bool, entries: &[(u16, u16)]) -> io::Result<()> {
        self.stream.write_all(&encode_materials(kind, ok, entries))
    }

    /// Claims a completed achievement's reward.
    pub fn claim_achievement(&mut self, id: u32) -> io::Result<()> {
        self.stream.write_all(&encode_id(CZ_REQ_ACH_REWARD, id))
    }

    /// Wears an achievement's title (0 takes it off).
    pub fn change_title(&mut self, title: u32) -> io::Result<()> {
        self.stream.write_all(&encode_id(CZ_REQ_CHANGE_TITLE, title))
    }

    pub fn roulette(&mut self, request: RouletteRequest) -> io::Result<()> {
        self.stream.write_all(&encode_roulette(request))
    }

    /// Puts an item in the Swap Equipment set, in its places; ZC_EQUIPSWITCH_ADDED answers.
    pub fn switch_add(&mut self, index: u16, places: u32) -> io::Result<()> {
        self.stream.write_all(&encode_switch_add(index, places))
    }

    /// Takes an item out of the Swap Equipment set; ZC_EQUIPSWITCH_REMOVED answers.
    pub fn switch_remove(&mut self, index: u16) -> io::Result<()> {
        self.stream.write_all(&encode_index(CZ_REQ_EQUIPSWITCH_REMOVE, index))
    }

    /// Switches the Swap Equipment set with what's worn.
    /// Merges stacks of one item into the first.
    pub fn merge_items(&mut self, indexes: &[u16]) -> io::Result<()> {
        self.stream.write_all(&encode_merge_items(indexes))
    }

    /// The item merge window closed without merging.
    pub fn cancel_merge(&mut self) -> io::Result<()> {
        self.stream.write_all(&CZ_CANCEL_MERGE_ITEM.to_le_bytes())
    }

    pub fn switch_all(&mut self) -> io::Result<()> {
        self.stream.write_all(&CZ_REQ_EQUIPSWITCH.to_le_bytes())
    }

    /// Takes off the general equipment (costumes, shadow gear and ammunition stay on).
    pub fn take_off_all(&mut self) -> io::Result<()> {
        self.stream.write_all(&CZ_REQ_TAKEOFF_EQUIP_ALL.to_le_bytes())
    }

    /// Asks what a card can go into; ZC_ITEMCOMPOSITION_LIST answers (nothing when nothing can).
    pub fn card_list(&mut self, card: u16) -> io::Result<()> {
        self.stream.write_all(&encode_card_list(card))
    }

    pub fn insert_card(&mut self, card: u16, equip: u16) -> io::Result<()> {
        self.stream.write_all(&encode_insert_card(card, equip))
    }

    /// Where Warp Portal or Teleport goes: a map from the list as it came, or "cancel".
    pub fn select_warp_point(&mut self, skill: u16, map: &str) -> io::Result<()> {
        self.stream.write_all(&encode_select_warp_point(skill, map))
    }

    /// /memo: saves where the player stands as a Warp Portal destination.
    pub fn remember_warp_point(&mut self) -> io::Result<()> {
        self.stream.write_all(&CZ_REMEMBER_WARPPOINT.to_le_bytes())
    }

    pub fn emote(&mut self, kind: u8) -> io::Result<()> {
        let mut packet = CZ_REQ_EMOTION.to_le_bytes().to_vec();
        packet.push(kind);
        self.stream.write_all(&packet)
    }

    /// Puts a status point into a stat (STATUS_STR and on).
    pub fn raise_stat(&mut self, stat: u16) -> io::Result<()> {
        let mut packet = CZ_STATUS_CHANGE.to_le_bytes().to_vec();
        packet.extend_from_slice(&stat.to_le_bytes());
        packet.push(1);
        self.stream.write_all(&packet)
    }

    /// Puts one trait point into a 4th job's trait (rathena's SP_POW and on).
    pub fn raise_trait(&mut self, trait_id: u16) -> io::Result<()> {
        self.stream.write_all(&encode_trait_raise(trait_id))
    }

    /// Back to the save point (after dying), or to character select.
    pub fn restart(&mut self, char_select: bool) -> io::Result<()> {
        let mut packet = CZ_RESTART.to_le_bytes().to_vec();
        packet.push(u8::from(char_select));
        self.stream.write_all(&packet)
    }

    /// Stand up where the player died (Token of Siegfried and the like). CZ_STANDING_RESURRECTION, header only.
    pub fn standing_resurrection(&mut self) -> io::Result<()> {
        self.stream.write_all(&encode_standing_resurrection())
    }

    /// Asks to leave the game.
    pub fn quit(&mut self) -> io::Result<()> {
        let mut packet = CZ_REQUEST_QUIT.to_le_bytes().to_vec();
        packet.extend_from_slice(&0u16.to_le_bytes());
        self.stream.write_all(&packet)
    }

    /// Opens a chat room over the player's head.
    pub fn make_chat_room(&mut self, title: &str, limit: u16, public: bool, password: &str) -> io::Result<()> {
        self.stream.write_all(&encode_room(CZ_CREATE_CHATROOM, title, limit, public, password))
    }

    /// The owner changes the room's title, size, password and public setting.
    pub fn change_chat_room(&mut self, title: &str, limit: u16, public: bool, password: &str) -> io::Result<()> {
        self.stream.write_all(&encode_room(CZ_CHANGE_CHATROOM, title, limit, public, password))
    }

    pub fn hand_over_chat_room(&mut self, name: &str) -> io::Result<()> {
        self.stream.write_all(&encode_room_owner(name))
    }

    pub fn kick_from_chat_room(&mut self, name: &str) -> io::Result<()> {
        self.stream.write_all(&encode_room_kick(name))
    }

    pub fn enter_chat_room(&mut self, id: u32, password: &str) -> io::Result<()> {
        let mut secret = password.as_bytes().to_vec();
        secret.resize(8, 0);
        let mut packet = CZ_REQ_ENTER_ROOM.to_le_bytes().to_vec();
        packet.extend_from_slice(&id.to_le_bytes());
        packet.extend_from_slice(&secret);
        self.stream.write_all(&packet)
    }

    pub fn leave_chat_room(&mut self) -> io::Result<()> {
        self.stream.write_all(&CZ_EXIT_ROOM.to_le_bytes())
    }

    /// Opens our shop (or, with `open` false, gives up setting one up): its title and (cart index, amount, price).
    pub fn open_shop(&mut self, title: &str, open: bool, items: &[(u16, u16, u32)]) -> io::Result<()> {
        let mut name = encode_text(title);
        name.resize(80, 0);
        let mut packet = CZ_REQ_OPENSTORE2.to_le_bytes().to_vec();
        packet.extend_from_slice(&((85 + items.len() * 8) as u16).to_le_bytes());
        packet.extend_from_slice(&name);
        packet.push(u8::from(open));
        for &(index, amount, price) in items {
            packet.extend_from_slice(&index.to_le_bytes());
            packet.extend_from_slice(&amount.to_le_bytes());
            packet.extend_from_slice(&price.to_le_bytes());
        }
        self.stream.write_all(&packet)
    }

    pub fn close_shop(&mut self) -> io::Result<()> {
        self.stream.write_all(&CZ_REQ_CLOSESTORE.to_le_bytes())
    }

    /// Asks what a vendor sells.
    pub fn browse_vendor(&mut self, vendor: u32) -> io::Result<()> {
        let mut packet = CZ_REQ_BUY_FROMMC.to_le_bytes().to_vec();
        packet.extend_from_slice(&vendor.to_le_bytes());
        self.stream.write_all(&packet)
    }

    /// Buys from a vendor's shop: (amount, index) pairs.
    pub fn buy_from_vendor(&mut self, vendor: u32, shop: u32, items: &[(u16, u16)]) -> io::Result<()> {
        let mut packet = CZ_PC_PURCHASE_ITEMLIST_FROMMC2.to_le_bytes().to_vec();
        packet.extend_from_slice(&((12 + items.len() * 4) as u16).to_le_bytes());
        packet.extend_from_slice(&vendor.to_le_bytes());
        packet.extend_from_slice(&shop.to_le_bytes());
        for &(amount, index) in items {
            packet.extend_from_slice(&amount.to_le_bytes());
            packet.extend_from_slice(&index.to_le_bytes());
        }
        self.stream.write_all(&packet)
    }

    /// Opens our buying store (or, with `open` false, cancels setting one up): zeny limit, title, and (item ID, amount, price).
    pub fn open_buying_store(&mut self, title: &str, limit: u32, open: bool, items: &[(u32, u16, u32)]) -> io::Result<()> {
        self.stream.write_all(&encode_open_buying_store(title, limit, open, items))
    }

    pub fn close_buying_store(&mut self) -> io::Result<()> {
        self.stream.write_all(&encode_close_buying_store())
    }

    /// Asks what a buying store wants to buy.
    pub fn click_buying_store(&mut self, owner: u32) -> io::Result<()> {
        self.stream.write_all(&encode_click_buying_store(owner))
    }

    /// Sells to a buying store: (inventory index, item ID, amount) triples.
    pub fn sell_to_buying_store(&mut self, owner: u32, store_id: u32, items: &[(u16, u32, u16)]) -> io::Result<()> {
        self.stream.write_all(&encode_sell_to_buying_store(owner, store_id, items))
    }

    /// Takes a quest up again, or puts it aside.
    pub fn set_quest_active(&mut self, id: u32, active: bool) -> io::Result<()> {
        let mut packet = CZ_ACTIVE_QUEST.to_le_bytes().to_vec();
        packet.extend_from_slice(&id.to_le_bytes());
        packet.push(u8::from(active));
        self.stream.write_all(&packet)
    }

    /// Opens the mailbox (or, with `refresh`, has the server read it again); the list comes back.
    pub fn open_mailbox(&mut self, refresh: bool) -> io::Result<()> {
        let mut packet = if refresh { CZ_REQ_REFRESH_MAIL_LIST2 } else { CZ_OPEN_MAILBOX2 }.to_le_bytes().to_vec();
        packet.resize(26, 0);
        self.stream.write_all(&packet)
    }

    pub fn close_mailbox(&mut self) -> io::Result<()> {
        self.stream.write_all(&CZ_CLOSE_MAILBOX.to_le_bytes())
    }

    /// The opener of the mail packets that name a mail by its box and ID.
    pub(crate) fn mail_request(&mut self, header: u16, kind: u8, id: u64) -> io::Result<()> {
        let mut packet = header.to_le_bytes().to_vec();
        packet.push(kind);
        packet.extend_from_slice(&id.to_le_bytes());
        self.stream.write_all(&packet)
    }

    pub fn read_mail(&mut self, kind: u8, id: u64) -> io::Result<()> {
        self.mail_request(CZ_REQ_READ_MAIL, kind, id)
    }

    pub fn delete_mail(&mut self, kind: u8, id: u64) -> io::Result<()> {
        self.mail_request(CZ_REQ_DELETE_MAIL, kind, id)
    }

    /// Takes a mail's zeny (or its items) into the inventory.
    pub fn claim_mail(&mut self, kind: u8, id: u64, zeny: bool) -> io::Result<()> {
        let mut packet = if zeny { CZ_REQ_ZENY_FROM_MAIL } else { CZ_REQ_ITEM_FROM_MAIL }.to_le_bytes().to_vec();
        packet.extend_from_slice(&id.to_le_bytes());
        packet.push(kind);
        self.stream.write_all(&packet)
    }

    /// Asks to start writing a mail (to that name, or nobody yet).
    pub fn start_mail(&mut self, to: &str) -> io::Result<()> {
        let mut packet = CZ_REQ_OPEN_WRITE_MAIL.to_le_bytes().to_vec();
        packet.extend_from_slice(&name_field(to));
        self.stream.write_all(&packet)
    }

    pub fn cancel_mail(&mut self) -> io::Result<()> {
        self.stream.write_all(&CZ_REQ_CANCEL_WRITE_MAIL.to_le_bytes())
    }

    /// Asks whose a name is (ZC_CHECKNAME answers).
    pub fn check_receiver(&mut self, name: &str) -> io::Result<()> {
        let mut packet = CZ_CHECKNAME2.to_le_bytes().to_vec();
        packet.extend_from_slice(&name_field(name));
        packet.push(0);
        self.stream.write_all(&packet)
    }

    /// Puts an inventory item in (or takes it back out of) the mail being written.
    pub fn attach_to_mail(&mut self, index: u16, amount: u16, attach: bool) -> io::Result<()> {
        let mut packet = if attach { CZ_REQ_ADD_ITEM_TO_MAIL } else { CZ_REQ_REMOVE_ITEM_MAIL }.to_le_bytes().to_vec();
        packet.extend_from_slice(&index.to_le_bytes());
        packet.extend_from_slice(&amount.to_le_bytes());
        self.stream.write_all(&packet)
    }

    /// Sends the mail being written, with the items attached and `zeny`; `receiver` is the character ID a name
    /// check found (0 if none was made).
    pub fn send_mail(&mut self, to: &str, from: &str, title: &str, text: &str, zeny: u64, receiver: u32) -> io::Result<()> {
        let field = |text: &str| {
            let mut bytes = encode_text(text);
            bytes.push(0);
            bytes
        };
        let (title, text) = (field(title), field(text));
        let mut packet = CZ_REQ_WRITE_MAIL2.to_le_bytes().to_vec();
        packet.extend_from_slice(&((68 + title.len() + text.len()) as u16).to_le_bytes());
        packet.extend_from_slice(&name_field(to));
        packet.extend_from_slice(&name_field(from));
        packet.extend_from_slice(&zeny.to_le_bytes());
        packet.extend_from_slice(&(title.len() as u16).to_le_bytes());
        packet.extend_from_slice(&(text.len() as u16).to_le_bytes());
        packet.extend_from_slice(&receiver.to_le_bytes());
        packet.extend_from_slice(&title);
        packet.extend_from_slice(&text);
        self.stream.write_all(&packet)
    }

    /// Moves an inventory item into the cart.
    pub fn put_in_cart(&mut self, index: u16, amount: u32) -> io::Result<()> {
        self.move_stored(CZ_MOVE_ITEM_FROM_BODY_TO_CART, index, amount)
    }

    /// Takes a cart item back into the inventory.
    pub fn take_from_cart(&mut self, index: u16, amount: u32) -> io::Result<()> {
        self.move_stored(CZ_MOVE_ITEM_FROM_CART_TO_BODY, index, amount)
    }

    /// Makes a guild (an Emperium is needed).
    pub fn make_guild(&mut self, character: u32, name: &str) -> io::Result<()> {
        let mut packet = CZ_REQ_MAKE_GUILD.to_le_bytes().to_vec();
        packet.extend_from_slice(&character.to_le_bytes());
        packet.extend_from_slice(&name_field(name));
        self.stream.write_all(&packet)
    }

    /// Asks for a page of the guild window: 0 its info, 1 its members, 2 positions, 3 skills, 4 the expel list.
    pub fn guild_page(&mut self, page: u32) -> io::Result<()> {
        let mut packet = CZ_REQ_GUILD_MENU.to_le_bytes().to_vec();
        packet.extend_from_slice(&page.to_le_bytes());
        self.stream.write_all(&packet)
    }

    /// Asks for a guild's emblem; a guild without one doesn't answer.
    pub fn guild_emblem(&mut self, guild: u32) -> io::Result<()> {
        let mut packet = CZ_REQ_GUILD_EMBLEM_IMG.to_le_bytes().to_vec();
        packet.extend_from_slice(&guild.to_le_bytes());
        self.stream.write_all(&packet)
    }

    /// Asks for a unit's name; for a player, the server adds their party, guild and position.
    pub fn ask_name(&mut self, id: u32) -> io::Result<()> {
        let mut packet = CZ_REQNAME.to_le_bytes().to_vec();
        packet.extend_from_slice(&id.to_le_bytes());
        self.stream.write_all(&packet)
    }

    /// Invites a character, by name, into the guild.
    pub fn invite_to_guild(&mut self, name: &str) -> io::Result<()> {
        let mut packet = CZ_REQ_JOIN_GUILD2.to_le_bytes().to_vec();
        packet.extend_from_slice(&name_field(name));
        self.stream.write_all(&packet)
    }

    /// Asks the guild of the player (account ID) for an alliance; ours are the player's account and character.
    pub fn ask_alliance(&mut self, account: u32, me: u32, character: u32) -> io::Result<()> {
        self.stream.write_all(&encode_ally_request(account, me, character))
    }

    /// Answers an alliance request, to the player who asked (account ID).
    pub fn answer_alliance(&mut self, account: u32, yes: bool) -> io::Result<()> {
        self.stream.write_all(&encode_pair(CZ_ALLY_GUILD, account, u32::from(yes)))
    }

    /// Sets the guild of the player (account ID) as an antagonist.
    pub fn set_antagonist(&mut self, account: u32) -> io::Result<()> {
        self.stream.write_all(&encode_id(CZ_REQ_HOSTILE_GUILD, account))
    }

    /// Ends the alliance with a guild, or no longer holds it as an antagonist.
    pub fn end_relation(&mut self, guild: u32, antagonist: bool) -> io::Result<()> {
        self.stream.write_all(&encode_pair(CZ_REQ_DELETE_RELATED_GUILD, guild, u32::from(antagonist)))
    }

    /// Disbands the guild; the key is its name.
    pub fn disband_guild(&mut self, name: &str) -> io::Result<()> {
        self.stream.write_all(&encode_disband(name))
    }

    pub fn answer_guild(&mut self, guild: u32, accept: bool) -> io::Result<()> {
        let mut packet = CZ_JOIN_GUILD.to_le_bytes().to_vec();
        packet.extend_from_slice(&guild.to_le_bytes());
        packet.extend_from_slice(&u32::from(accept).to_le_bytes());
        self.stream.write_all(&packet)
    }

    /// The master changes a position: its permissions, tax and title.
    pub fn change_position(&mut self, position: &GuildPosition, title: &str) -> io::Result<()> {
        self.stream.write_all(&encode_position_change(position, title))
    }

    /// The master moves members to other positions (account, character, position each), in one packet.
    pub fn move_members(&mut self, moves: &[(u32, u32, u32)]) -> io::Result<()> {
        self.stream.write_all(&encode_member_moves(moves))
    }

    /// Expels a member (CZ_REQ_BAN_GUILD), with a reason.
    pub fn expel_from_guild(&mut self, guild: u32, account: u32, character: u32, reason: &str) -> io::Result<()> {
        self.stream.write_all(&encode_guild_reason(CZ_REQ_BAN_GUILD, guild, account, character, reason))
    }

    pub fn leave_guild(&mut self, guild: u32, account: u32, character: u32, reason: &str) -> io::Result<()> {
        let packet = encode_guild_reason(CZ_REQ_LEAVE_GUILD, guild, account, character, reason);
        self.stream.write_all(&packet)
    }

    /// Applies to a battleground's queue (`QUEUE_ALONE`, `QUEUE_PARTY`, `QUEUE_GUILD`).
    pub fn apply_to_queue(&mut self, kind: u16, name: &str) -> io::Result<()> {
        self.stream.write_all(&encode_queue_apply(kind, name))
    }

    pub fn leave_queue(&mut self, name: &str) -> io::Result<()> {
        self.stream.write_all(&encode_queue_name(CZ_REQ_ENTRY_QUEUE_CANCEL, name))
    }

    /// How many are queued for it (answered with ZC_NOTIFY_ENTRY_QUEUE_APPLY).
    pub fn ask_queue_number(&mut self, name: &str) -> io::Result<()> {
        self.stream.write_all(&encode_queue_name(CZ_REQ_ENTRY_QUEUE_RANKING, name))
    }

    /// Enters the battle, or not.
    pub fn answer_lobby(&mut self, accept: bool, name: &str) -> io::Result<()> {
        self.stream.write_all(&encode_lobby_reply(accept, name))
    }

    /// CZ_MEMORIALDUNGEON_COMMAND. Destroy (command 3) goes only after the player confirms.
    pub fn memorial_command(&mut self, command: u32) -> io::Result<()> {
        self.stream.write_all(&encode_memorial_command(command))
    }

    /// Asks a party member (account) to be adopted by the player and their partner.
    pub fn ask_adoption(&mut self, account: u32) -> io::Result<()> {
        self.stream.write_all(&encode_adopt_request(account))
    }

    /// Answers a request to be adopted, with the two accounts it gave.
    pub fn answer_adoption(&mut self, parent: u32, partner: u32, accept: bool) -> io::Result<()> {
        self.stream.write_all(&encode_adopt_answer(parent, partner, accept))
    }

    /// Says something to the battleground team.
    pub fn battle_chat(&mut self, name: &str, message: &str) -> io::Result<()> {
        self.stream.write_all(&encode_battle_chat(name, message))
    }

    /// Says something to the guild.
    pub fn guild_chat(&mut self, name: &str, message: &str) -> io::Result<()> {
        let text = encode_text(&format!("{name} : {message}"));
        let mut packet = CZ_GUILD_CHAT.to_le_bytes().to_vec();
        packet.extend_from_slice(&((4 + text.len() + 1) as u16).to_le_bytes());
        packet.extend_from_slice(&text);
        packet.push(0);
        self.stream.write_all(&packet)
    }

    /// Says something to the clan.
    pub fn clan_chat(&mut self, name: &str, message: &str) -> io::Result<()> {
        self.stream.write_all(&encode_clan_chat(name, message))
    }

    /// Asks a character, by name, to be friends.
    pub fn add_friend(&mut self, name: &str) -> io::Result<()> {
        let mut packet = CZ_ADD_FRIENDS.to_le_bytes().to_vec();
        packet.extend_from_slice(&name_field(name));
        self.stream.write_all(&packet)
    }

    pub fn answer_friend(&mut self, account: u32, character: u32, accept: bool) -> io::Result<()> {
        let mut packet = CZ_ACK_REQ_ADD_FRIENDS.to_le_bytes().to_vec();
        packet.extend_from_slice(&account.to_le_bytes());
        packet.extend_from_slice(&character.to_le_bytes());
        packet.extend_from_slice(&u32::from(accept).to_le_bytes());
        self.stream.write_all(&packet)
    }

    pub fn remove_friend(&mut self, account: u32, character: u32) -> io::Result<()> {
        let mut packet = CZ_DELETE_FRIENDS.to_le_bytes().to_vec();
        packet.extend_from_slice(&account.to_le_bytes());
        packet.extend_from_slice(&character.to_le_bytes());
        self.stream.write_all(&packet)
    }

    /// Makes a party: its name, and whether items go to whoever picks them up (false) or are shared.
    pub fn make_party(&mut self, name: &str, share_pickup: bool, share_loot: bool) -> io::Result<()> {
        let mut packet = CZ_MAKE_GROUP2.to_le_bytes().to_vec();
        packet.extend_from_slice(&name_field(name));
        packet.extend_from_slice(&[u8::from(share_pickup), u8::from(share_loot)]);
        self.stream.write_all(&packet)
    }

    /// Searches the stores on the map: vending (`buying` false) or buying stores, the items by ID, prices between
    /// `min` and `max` (0: no limit); the answer is `CatalogResults` or `CatalogFailed`.
    pub fn search_stores(&mut self, buying: bool, max: u32, min: u32, items: &[u32], cards: &[u32]) -> io::Result<()> {
        self.stream.write_all(&encode_store_search(buying, max, min, items, cards))
    }

    pub fn next_store_page(&mut self) -> io::Result<()> {
        self.stream.write_all(&CZ_SEARCH_STORE_INFO_NEXT_PAGE.to_le_bytes())
    }

    pub fn close_store_search(&mut self) -> io::Result<()> {
        self.stream.write_all(&CZ_CLOSE_SEARCH_STORE_INFO.to_le_bytes())
    }

    /// A result picked: where it is (`CatalogMark`), or its store opened.
    pub fn pick_store(&mut self, account: u32, store: u32, item: u32) -> io::Result<()> {
        self.stream.write_all(&encode_store_pick(account, store, item))
    }

    /// Asks for a ranking's top ten (0 Blacksmith, 1 Alchemist, 2 Taekwon, 3 Killer); the answer is `Ranking`.
    pub fn ask_ranking(&mut self, kind: u16) -> io::Result<()> {
        self.stream.write_all(&encode_ranking_request(kind))
    }

    /// Asks for a character's name by ID; the answer is `CharName`.
    pub fn ask_char_name(&mut self, id: u32) -> io::Result<()> {
        self.stream.write_all(&encode_name_request(id))
    }

    /// Tells the server the client is ready after a map property (CZ_BLOCKING_PLAY_CANCEL).
    pub fn ready_to_play(&mut self) -> io::Result<()> {
        self.stream.write_all(&encode_blocking_play_cancel())
    }

    /// Refuses (or accepts again) party invitations; the answer is `PartyRefusing`.
    pub fn refuse_parties(&mut self, refuse: bool) -> io::Result<()> {
        let [low, high] = CZ_PARTY_CONFIG.to_le_bytes();
        self.stream.write_all(&[low, high, u8::from(refuse)])
    }

    /// The leader's party settings: even EXP share, items to the party on pickup, items shared out.
    pub fn party_options(&mut self, exp_even: bool, pickup: bool, share: bool) -> io::Result<()> {
        self.stream.write_all(&encode_party_options(exp_even, pickup, share))
    }

    /// Hands the party to another member (the leader only).
    pub fn change_party_leader(&mut self, account: u32) -> io::Result<()> {
        self.send_id(CZ_CHANGE_GROUP_MASTER, account)
    }

    /// Invites a character, by name, into the party.
    pub fn invite_to_party(&mut self, name: &str) -> io::Result<()> {
        let mut packet = CZ_PARTY_JOIN_REQ.to_le_bytes().to_vec();
        packet.extend_from_slice(&name_field(name));
        self.stream.write_all(&packet)
    }

    /// Asks a booking post's leader (by character and account) to let the player join.
    pub fn ask_to_join(&mut self, character: u32, account: u32) -> io::Result<()> {
        self.stream.write_all(&encode_pair(CZ_PARTY_REQ_MASTER_TO_JOIN, character, account))
    }

    /// The leader's answer to a request through the booking post.
    pub fn answer_join(&mut self, character: u32, account: u32, accept: bool) -> io::Result<()> {
        let mut packet = encode_pair(CZ_PARTY_REQ_ACK_MASTER_TO_JOIN, character, account);
        packet.push(u8::from(accept));
        self.stream.write_all(&packet)
    }

    pub fn answer_party(&mut self, party: u32, accept: bool) -> io::Result<()> {
        let mut packet = CZ_PARTY_JOIN_REQ_ACK.to_le_bytes().to_vec();
        packet.extend_from_slice(&party.to_le_bytes());
        packet.push(u8::from(accept));
        self.stream.write_all(&packet)
    }

    pub fn leave_party(&mut self) -> io::Result<()> {
        self.stream.write_all(&CZ_REQ_LEAVE_GROUP.to_le_bytes())
    }

    pub fn expel_from_party(&mut self, account: u32, name: &str) -> io::Result<()> {
        let mut packet = CZ_REQ_EXPEL_GROUP_MEMBER.to_le_bytes().to_vec();
        packet.extend_from_slice(&account.to_le_bytes());
        packet.extend_from_slice(&name_field(name));
        self.stream.write_all(&packet)
    }

    /// Says something to the party.
    pub fn party_chat(&mut self, name: &str, message: &str) -> io::Result<()> {
        let text = encode_text(&format!("{name} : {message}"));
        let mut packet = CZ_REQUEST_CHAT_PARTY.to_le_bytes().to_vec();
        packet.extend_from_slice(&((4 + text.len() + 1) as u16).to_le_bytes());
        packet.extend_from_slice(&text);
        packet.push(0);
        self.stream.write_all(&packet)
    }

    /// Asks a player (by account ID) for a deal.
    pub fn request_trade(&mut self, account: u32) -> io::Result<()> {
        let mut packet = CZ_REQ_EXCHANGE_ITEM.to_le_bytes().to_vec();
        packet.extend_from_slice(&account.to_le_bytes());
        self.stream.write_all(&packet)
    }

    pub fn answer_trade(&mut self, accept: bool) -> io::Result<()> {
        let mut packet = CZ_ACK_EXCHANGE_ITEM.to_le_bytes().to_vec();
        packet.push(if accept { 3 } else { 4 });
        self.stream.write_all(&packet)
    }

    /// Puts an inventory item into the deal; index 0 is zeny.
    pub fn add_to_trade(&mut self, index: u16, amount: u32) -> io::Result<()> {
        let mut packet = CZ_ADD_EXCHANGE_ITEM.to_le_bytes().to_vec();
        packet.extend_from_slice(&index.to_le_bytes());
        packet.extend_from_slice(&amount.to_le_bytes());
        self.stream.write_all(&packet)
    }

    pub fn lock_trade(&mut self) -> io::Result<()> {
        self.stream.write_all(&CZ_CONCLUDE_EXCHANGE_ITEM.to_le_bytes())
    }

    pub fn cancel_trade(&mut self) -> io::Result<()> {
        self.stream.write_all(&CZ_CANCEL_EXCHANGE_ITEM.to_le_bytes())
    }

    pub fn finish_trade(&mut self) -> io::Result<()> {
        self.stream.write_all(&CZ_EXEC_EXCHANGE_ITEM.to_le_bytes())
    }

    /// Whispers to the character `to`.
    pub fn whisper(&mut self, to: &str, message: &str) -> io::Result<()> {
        let mut name = encode_text(to);
        name.resize(24, 0);
        let text = encode_text(message);
        let mut packet = CZ_WHISPER.to_le_bytes().to_vec();
        packet.extend_from_slice(&((4 + 24 + text.len() + 1) as u16).to_le_bytes());
        packet.extend_from_slice(&name);
        packet.extend_from_slice(&text);
        packet.push(0);
        self.stream.write_all(&packet)
    }

    /// Drops `amount` of an inventory item at the player's feet; the answer is `ItemRemoved`, and the item shows up
    /// on the ground.
    pub fn drop_item(&mut self, index: u16, amount: u16) -> io::Result<()> {
        self.stream.write_all(&encode_drop_item(index, amount))
    }

    /// Answers an `IdentifyList`: the item to identify, or none to cancel; the answer is `Identified`.
    pub fn identify(&mut self, index: Option<u16>) -> io::Result<()> {
        self.stream.write_all(&encode_identify(index))
    }

    /// Asks how many players are online; the answer is `UserCount`.
    pub fn user_count(&mut self) -> io::Result<()> {
        self.stream.write_all(&CZ_REQ_USER_COUNT.to_le_bytes())
    }

    /// Blocks (`allow` false) or allows whispers from `name`; the answer is `WhisperSetting`.
    pub fn set_whisper(&mut self, name: &str, allow: bool) -> io::Result<()> {
        self.stream.write_all(&encode_whisper_setting(name, allow))
    }

    /// Asks for the names whose whispers are blocked; the answer is `WhisperList`.
    pub fn whisper_list(&mut self) -> io::Result<()> {
        self.stream.write_all(&CZ_REQ_WHISPER_LIST.to_le_bytes())
    }

    /// Blocks (`allow` false) or allows everyone's whispers; the answer is `WhisperAll`.
    pub fn set_whisper_all(&mut self, allow: bool) -> io::Result<()> {
        self.stream.write_all(&encode_whisper_all(allow))
    }

    pub fn chat(&mut self, name: &str, message: &str) -> io::Result<()> {
        let text = encode_text(&format!("{name} : {message}"));
        let mut packet = CZ_REQUEST_CHAT.to_le_bytes().to_vec();
        packet.extend_from_slice(&((4 + text.len() + 1) as u16).to_le_bytes());
        packet.extend_from_slice(&text);
        packet.push(0);
        self.stream.write_all(&packet)
    }
}
