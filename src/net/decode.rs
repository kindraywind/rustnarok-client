//! A map server packet decoded into a `MapEvent`.

use super::*;

/// One entry of ZC_ACK_GUILDSTORAGE_LOG.
const GUILD_STORAGE_ENTRY: usize = 93;

pub(crate) fn decode_map_packet(packet: &[u8]) -> Option<MapEvent> {
    if packet.len() < 2 {
        return None;
    }
    let u32_at = |offset: usize| u32::from_le_bytes(packet[offset..offset + 4].try_into().unwrap());
    let i32_at = |offset: usize| i32::from_le_bytes(packet[offset..offset + 4].try_into().unwrap());
    let u16_at = |offset: usize| u16::from_le_bytes([packet[offset], packet[offset + 1]]);
    Some(match u16_at(0) {
        ZC_ACCEPT_ENTER => {
            let (position, direction) = decode_position(&packet[6..9]);
            MapEvent::Entered { position, direction: sprite_direction(direction), clock: (u32_at(2), Instant::now()) }
        }
        ZC_NOTIFY_STANDENTRY => MapEvent::Unit(decode_unit(packet, &STAND_LAYOUT, decode_entry_position(&packet[63..66]))),
        ZC_NOTIFY_NEWENTRY => MapEvent::Unit(Unit { spawned: true, ..decode_unit(packet, &NEW_LAYOUT, decode_entry_position(&packet[63..66])) }),
        ZC_NOTIFY_MOVEENTRY => {
            let (from, to) = decode_move(&packet[67..73]);
            let sub = [packet[72] >> 4, packet[72] & 0x0F];
            MapEvent::UnitWalk { unit: decode_unit(packet, &MOVE_LAYOUT, (from, 0)), to, start_time: u32_at(37), sub }
        }
        ZC_NOTIFY_TIME => MapEvent::ServerTime(u32_at(2), Instant::now()),
        ZC_NOTIFY_VANISH => MapEvent::Vanish { id: u32_at(2), reason: packet[6] },
        ZC_NOTIFY_MOVE => {
            let (from, to) = decode_move(&packet[6..12]);
            MapEvent::UnitMove { id: u32_at(2), from, to, start_time: u32_at(12), sub: [packet[11] >> 4, packet[11] & 0x0F] }
        }
        ZC_NOTIFY_PLAYERMOVE => {
            let (from, to) = decode_move(&packet[6..12]);
            MapEvent::PlayerMove { from, to, start_time: u32_at(2), sub: [packet[11] >> 4, packet[11] & 0x0F] }
        }
        ZC_STOPMOVE => MapEvent::Stop { id: u32_at(2), position: [u16_at(6), u16_at(8)] },
        ZC_HIGHJUMP => MapEvent::Knocked { id: u32_at(2), position: [u16_at(6), u16_at(8)] },
        ZC_FASTMOVE => MapEvent::Slide { id: u32_at(2), position: [u16_at(6), u16_at(8)] },
        ZC_COMBODELAY => MapEvent::ComboWindow { id: u32_at(2), ms: u32_at(6) },
        ZC_BLADESTOP => MapEvent::BladeStop { source: u32_at(2), target: u32_at(6), on: u32_at(10) != 0 },
        ZC_CHANGE_DIRECTION => MapEvent::Turn { id: u32_at(2), direction: sprite_direction(packet[8]), head_direction: u16_at(6) },
        ZC_NOTIFY_CHAT => MapEvent::Chat { id: Some(u32_at(4)), message: c_string(&packet[8..]), color: None },
        ZC_NOTIFY_PLAYERCHAT => MapEvent::Chat { id: None, message: c_string(&packet[4..]), color: None },
        // The color is 0xBBGGRR, little-endian.
        ZC_NPC_CHAT => MapEvent::Chat { id: Some(u32_at(4)), message: c_string(&packet[12..]), color: Some([packet[8], packet[9], packet[10]]) },
        ZC_NPCACK_MAPMOVE => MapEvent::MapChange { map: c_string(&packet[2..18]), position: [u16_at(18), u16_at(20)] },
        // The IP in network order, the port little-endian, as in HC_NOTIFY_ZONESVR; the domain after them isn't used.
        ZC_NPCACK_SERVERMOVE => MapEvent::ServerMove {
            map: c_string(&packet[2..18]),
            position: [u16_at(18), u16_at(20)],
            address: (packet[22..26].try_into().unwrap(), u16_at(26)),
        },
        ZC_SAY_DIALOG => MapEvent::NpcSay { npc: u32_at(4), text: c_string(&packet[8..]) },
        // The type byte (offset 8) isn't used: rathena defines it and never sends this packet yet.
        ZC_SAY_DIALOG2 => MapEvent::NpcSay { npc: u32_at(4), text: c_string(&packet[9..]) },
        ZC_WAIT_DIALOG => MapEvent::NpcNext { npc: u32_at(2) },
        ZC_WAIT_DIALOG2 => MapEvent::NpcNext { npc: u32_at(2) },
        // The struct lists height, then width.
        ZC_DIALOG_WINDOW_SIZE => MapEvent::NpcWindowSize { width: i32_at(6), height: i32_at(2) },
        ZC_DIALOG_WINDOW_POS => MapEvent::NpcWindowPos { x: i32_at(2), y: i32_at(6), percent: false },
        ZC_DIALOG_WINDOW_POS2 => MapEvent::NpcWindowPos { x: i32_at(2), y: i32_at(6), percent: true },
        ZC_CLOSE_DIALOG => MapEvent::NpcClose { npc: u32_at(2) },
        ZC_CLEAR_DIALOG => MapEvent::NpcClear { npc: u32_at(2) },
        ZC_PROGRESS => MapEvent::Progress { seconds: u32_at(6) },
        ZC_PROGRESS_ACTOR => MapEvent::ProgressActor { unit: u32_at(2), seconds: u32_at(10) },
        ZC_PROGRESS_CANCEL => MapEvent::ProgressCancel,
        ZC_MENU_LIST => MapEvent::NpcMenu { npc: u32_at(4), items: c_string(&packet[8..]).split(':').map(str::to_owned).collect() },
        ZC_OPEN_EDITDLG => MapEvent::NpcInput { npc: u32_at(2), text: false },
        ZC_OPEN_EDITDLGSTR => MapEvent::NpcInput { npc: u32_at(2), text: true },
        ZC_NOTIFY_ACT => MapEvent::Act(Act {
            source: u32_at(2),
            target: u32_at(6),
            source_speed: u32_at(14) as i32,
            target_speed: u32_at(18) as i32,
            damage: u32_at(22) as i32,
            sp: packet[26] != 0,
            hits: u16_at(27),
            kind: packet[29],
            left_damage: u32_at(30) as i32,
        }),
        ZC_NOTIFY_ACT_OLD => MapEvent::Act(Act {
            source: u32_at(2),
            target: u32_at(6),
            source_speed: u32_at(14) as i32,
            target_speed: u32_at(18) as i32,
            damage: i32::from(u16_at(22) as i16),
            sp: false,
            hits: u16_at(24),
            kind: packet[26],
            left_damage: i32::from(u16_at(27) as i16),
        }),
        ZC_PAR_CHANGE => MapEvent::Status { id: u16_at(2), value: i64::from(u32_at(4) as i32) },
        ZC_SKILLINFO_LIST => MapEvent::Skills(packet[4..].chunks_exact(SKILL_SIZE).map(decode_skill).collect()),
        ZC_ADD_SKILL => MapEvent::SkillChanged(decode_skill(&packet[2..])),
        ZC_SKILLINFO_UPDATE => {
            MapEvent::SkillChanged(Skill { id: u16_at(2), kind: u32::MAX, level: u16_at(4), sp: u16_at(6), range: u16_at(8), upgradable: packet[10] != 0 })
        }
        ZC_SKILLINFO_UPDATE2 => {
            MapEvent::SkillChanged(Skill { id: u16_at(2), kind: u32_at(4), level: u16_at(8), sp: u16_at(10), range: u16_at(12), upgradable: packet[14] != 0 })
        }
        ZC_SKILLINFO_DELETE => MapEvent::SkillRemoved(u16_at(2)),
        ZC_DISPEL => MapEvent::CastCancelled(u32_at(2)),
        ZC_NAVIGATION_ACTIVE => MapEvent::Navigate {
            kind: packet[2],
            services: packet[3],
            hide_window: packet[4] != 0,
            map: c_string(&packet[5..21]),
            cell: [u16_at(21), u16_at(23)],
            monster: u16_at(25),
        },
        // A success byte, then 16 bytes an entry: the type (u64) and the points (i64).
        ZC_REPUTE_INFO => MapEvent::Reputation(
            packet
                .get(5..)
                .unwrap_or_default()
                .chunks_exact(16)
                .map(|entry| (u64::from_le_bytes(entry[..8].try_into().unwrap()), i64::from_le_bytes(entry[8..].try_into().unwrap())))
                .collect(),
        ),
        ZC_REPUTE_OPEN => MapEvent::ReputationOpen { group: u64::from_le_bytes(packet[2..10].try_into().unwrap()), repute: u32_at(10) },
        ZC_USESKILL_ACK | ZC_USESKILL_ACK2 => MapEvent::Casting {
            source: u32_at(2),
            target: u32_at(6),
            cell: [u16_at(10), u16_at(12)],
            skill: u16_at(14),
            element: u32_at(16),
            cast_ms: u32_at(20),
        },
        ZC_NOTIFY_SKILL => MapEvent::SkillHit {
            skill: u16_at(2),
            level: u16_at(28),
            act: Act {
                source: u32_at(4),
                target: u32_at(8),
                source_speed: u32_at(16) as i32,
                target_speed: u32_at(20) as i32,
                damage: u32_at(24) as i32,
                sp: false,
                hits: u16_at(30),
                kind: packet[32],
                left_damage: 0,
            },
        },
        ZC_USE_SKILL => MapEvent::SkillUsed { skill: u16_at(2), amount: u32_at(4) as i32, target: u32_at(8), source: u32_at(12), ok: packet[16] != 0 },
        ZC_NOTIFY_GROUNDSKILL => MapEvent::GroundSkill { skill: u16_at(2), source: u32_at(4), level: u16_at(8), cell: [u16_at(10), u16_at(12)] },
        ZC_SKILL_ENTRY => MapEvent::SkillUnit { id: u32_at(4), kind: u32_at(16), cell: [u16_at(12), u16_at(14)], range: packet[20], level: packet[22] },
        ZC_SKILL_DISAPPEAR => MapEvent::SkillUnitGone { id: u32_at(2) },
        ZC_SET_MAPCELL => MapEvent::MapCell { cell: [u16_at(2), u16_at(4)], kind: u16_at(6), map: c_string(&packet[8..24]) },
        ZC_ACK_TOUSESKILL => MapEvent::SkillFailed { skill: u16_at(2), detail: u32_at(4) as i32, item: u32_at(8), ok: packet[12] != 0, cause: packet[13] },
        ZC_RECOVERY => MapEvent::Recovered { kind: u16_at(2), amount: u32_at(4) as i32 },
        ZC_MSG_STATE_CHANGE3 => {
            MapEvent::StatusIcon { kind: u16_at(2), id: u32_at(4), on: packet[8] != 0, total_ms: u32_at(9), left_ms: u32_at(13), value: u32_at(17) as i32 }
        }
        ZC_MSG_STATE_CHANGE2 => {
            MapEvent::StatusIcon { kind: u16_at(2), id: u32_at(4), on: packet[8] != 0, total_ms: u32_at(9), left_ms: u32_at(9), value: u32_at(13) as i32 }
        }
        ZC_MSG_STATE_CHANGE => MapEvent::StatusIcon { kind: u16_at(2), id: u32_at(4), on: packet[8] != 0, total_ms: 0, left_ms: 0, value: 0 },
        ZC_LONGLONGPAR_CHANGE => MapEvent::Status { id: u16_at(2), value: i64::from_le_bytes(packet[4..12].try_into().unwrap()) },
        ZC_HP_INFO => MapEvent::UnitHp { id: u32_at(2), hp: u32_at(6) as i32, max_hp: u32_at(10) as i32 },
        ZC_ATTACK_RANGE => MapEvent::AttackRange(u16_at(2)),
        ZC_LONGPAR_CHANGE => MapEvent::Status { id: u16_at(2), value: i64::from(u32_at(4) as i32) },
        ZC_ITEM_ENTRY => MapEvent::GroundItem(GroundItem {
            id: u32_at(2),
            item: u32_at(6),
            identified: packet[10] != 0,
            cell: [u16_at(11), u16_at(13)],
            amount: u16_at(15),
            sub: [packet[17], packet[18]],
            falling: false,
            show_effect: false,
            effect_mode: 0,
        }),
        ZC_ITEM_FALL_ENTRY => MapEvent::GroundItem(GroundItem {
            id: u32_at(2),
            item: u32_at(6),
            identified: packet[12] != 0,
            cell: [u16_at(13), u16_at(15)],
            sub: [packet[17], packet[18]],
            amount: u16_at(19),
            falling: true,
            show_effect: packet[21] != 0,
            effect_mode: u16_at(22),
        }),
        ZC_ITEM_DISAPPEAR => MapEvent::ItemGone { id: u32_at(2) },
        ZC_ITEM_PICKUP_ACK => MapEvent::PickedUp {
            result: packet[33],
            item: InventoryItem {
                index: u16_at(2),
                amount: u16_at(4),
                id: u32_at(6),
                identified: packet[10] != 0,
                cards: cards_at(packet, 12),
                location: u32_at(28),
                kind: packet[32],
                refine: packet[68],
                grade: packet[69],
                worn: 0,
                broken: packet[11] != 0,
                favorite: packet[65] != 0,
                bind: u16_at(38),
                expires: u32_at(34),
                switch: 0,
                options: options_at(packet, 40),
            },
        },
        ZC_INVENTORY_START => MapEvent::InventoryStart { kind: packet[4], name: c_string(packet.get(5..).unwrap_or_default()) },
        ZC_INVENTORY_ITEMLIST_NORMAL => {
            MapEvent::InventoryItems { kind: packet[4], items: packet[5..].chunks_exact(NORMAL_ITEM_SIZE).map(decode_normal_item).collect() }
        }
        ZC_INVENTORY_ITEMLIST_EQUIP => {
            MapEvent::InventoryItems { kind: packet[4], items: packet[5..].chunks_exact(EQUIP_ITEM_SIZE).map(decode_equip_item).collect() }
        }
        ZC_EXTEND_BODYITEM_SIZE => MapEvent::InventoryExpansion(u16_at(2)),
        ZC_ITEM_THROW_ACK => MapEvent::ItemRemoved { index: u16_at(2), amount: u16_at(4) },
        ZC_DELETE_ITEM_FROM_BODY => MapEvent::ItemRemoved { index: u16_at(4), amount: u16_at(6) },
        ZC_USE_ITEM_ACK => MapEvent::ItemUsed { index: u16_at(2), item: u32_at(4), user: u32_at(8), remaining: u16_at(12), ok: packet[14] != 0 },
        ZC_NOTIFY_EFFECT => MapEvent::MiscEffect { id: u32_at(2), kind: u32_at(6) },
        ZC_SELECT_DEALTYPE => MapEvent::DealChoice { npc: u32_at(2) },
        ZC_PC_PURCHASE_ITEMLIST => MapEvent::ShopGoods(
            packet[4..]
                .chunks_exact(19)
                .map(|goods| ShopGoods { item: self::u32_at(goods, 0), price: self::u32_at(goods, 4), discounted: self::u32_at(goods, 8), kind: goods[12] })
                .collect(),
        ),
        ZC_PC_SELL_ITEMLIST => {
            MapEvent::ShopWants(packet[4..].chunks_exact(10).map(|want| (self::u16_at(want, 0), self::u32_at(want, 2), self::u32_at(want, 6))).collect())
        }
        ZC_PC_PURCHASE_RESULT => MapEvent::Bought(packet[2]),
        ZC_PC_SELL_RESULT => MapEvent::Sold(packet[2]),
        ZC_NOTIFY_STOREITEM_COUNTINFO => MapEvent::StorageCount { amount: u16_at(2), max: u16_at(4) },
        // Index, amount, item, type, identified, damaged, cards, options, refine, grade.
        ZC_ADD_ITEM_TO_STORE => MapEvent::StorageAdded(InventoryItem {
            index: u16_at(2),
            amount: u32_at(4).min(u32::from(u16::MAX)) as u16,
            id: u32_at(8),
            kind: packet[12],
            identified: packet[13] != 0,
            cards: cards_at(packet, 15),
            options: options_at(packet, 31),
            refine: packet[56],
            ..Default::default()
        }),
        ZC_DELETE_ITEM_FROM_STORE => MapEvent::StorageRemoved { index: u16_at(2), amount: u32_at(4) },
        ZC_MOVE_ITEM_FAILED => MapEvent::MoveFailed { index: u16_at(2), amount: u16_at(4) },
        ZC_CLOSE_STORE => MapEvent::StorageClosed,
        // Sender's ID, name, admin flag, then the text; a leading "|NN" is a code page mark.
        ZC_WHISPER => {
            let text = c_string(packet.get(33..).unwrap_or_default());
            let text = match text.strip_prefix('|') {
                Some(rest) if rest.len() >= 2 && rest.is_char_boundary(2) && rest[..2].chars().all(|digit| digit.is_ascii_digit()) => rest[2..].to_owned(),
                _ => text,
            };
            MapEvent::Whisper { from: c_string(&packet[8..32]), text }
        }
        ZC_ACK_WHISPER => MapEvent::WhisperResult(packet[2]),
        ZC_USER_COUNT => MapEvent::UserCount(u32_at(2)),
        // Skill, its inf, level, SP, range, the name (24 bytes), whether it can be raised.
        ZC_AUTORUN_SKILL => {
            MapEvent::ItemSkill(Skill { id: u16_at(2), kind: u32_at(4), level: u16_at(8), sp: u16_at(10), range: u16_at(12), upgradable: packet[38] != 0 })
        }
        ZC_ITEMIDENTIFY_LIST => MapEvent::IdentifyList(
            packet[4..usize::from(u16_at(2)).min(packet.len())].chunks_exact(2).map(|pair| u16::from_le_bytes([pair[0], pair[1]])).collect(),
        ),
        ZC_ACK_ITEMIDENTIFY => MapEvent::Identified { index: u16_at(2), ok: packet[4] == 0 },
        ZC_SETTING_WHISPER_PC => MapEvent::WhisperSetting { allow: packet[2] != 0, result: packet[3] },
        ZC_SETTING_WHISPER_STATE => MapEvent::WhisperAll { allow: packet[2] != 0, result: packet[3] },
        ZC_WHISPER_LIST => MapEvent::WhisperList(packet[4..usize::from(u16_at(2)).min(packet.len())].chunks_exact(24).map(c_string).collect()),
        ZC_OPENSTORE => MapEvent::VendSetup(u16_at(2)),
        ZC_ACK_OPENSTORE2 => MapEvent::VendOpened(packet[2]),
        // Each: price, index, amount, type, item, identified, damaged, cards, options, refine, grade.
        ZC_PC_PURCHASE_MYITEMLIST => MapEvent::MyShop(
            packet[8..]
                .chunks_exact(58)
                .map(|entry| VendItem {
                    index: self::u16_at(entry, 4),
                    price: self::u32_at(entry, 0),
                    item: InventoryItem {
                        index: self::u16_at(entry, 4),
                        amount: self::u16_at(entry, 6),
                        kind: entry[8],
                        id: self::u32_at(entry, 9),
                        identified: entry[13] != 0,
                        cards: cards_at(entry, 15),
                        options: options_at(entry, 31),
                        refine: entry[56],
                        ..Default::default()
                    },
                })
                .collect(),
        ),
        ZC_DELETEITEM_FROM_MCSTORE => MapEvent::VendSold { index: u16_at(2), amount: u16_at(4), zeny: u32_at(14) as i32 },
        ZC_STORE_ENTRY => MapEvent::VendorSign { id: u32_at(2), title: c_string(&packet[6..86]) },
        ZC_DISAPPEAR_ENTRY => MapEvent::VendorGone { id: u32_at(2) },
        // The vendor, the shop's ID, then each: price, amount, index, type, item, identified, damaged, cards,
        // options, where it's worn, its look, refine, grade.
        ZC_PC_PURCHASE_ITEMLIST_FROMMC => MapEvent::VendorGoods {
            vendor: u32_at(4),
            shop: u32_at(8),
            goods: packet[12..]
                .chunks_exact(64)
                .map(|entry| VendItem {
                    index: self::u16_at(entry, 6),
                    price: self::u32_at(entry, 0),
                    item: InventoryItem {
                        index: self::u16_at(entry, 6),
                        amount: self::u16_at(entry, 4),
                        kind: entry[8],
                        id: self::u32_at(entry, 9),
                        identified: entry[13] != 0,
                        cards: cards_at(entry, 15),
                        options: options_at(entry, 31),
                        location: self::u32_at(entry, 56),
                        refine: entry[62],
                        ..Default::default()
                    },
                })
                .collect(),
        },
        ZC_PC_PURCHASE_RESULT_FROMMC => MapEvent::VendBought { index: u16_at(2), amount: u16_at(4), result: packet[6] },
        ZC_OPEN_BUYING_STORE => MapEvent::OpenBuyingStore { slots: packet[2] },
        ZC_FAILED_OPEN_BUYING_STORE_TO_BUYER => MapEvent::BuyingStoreOpenFailed { result: u16_at(2), weight: u32_at(4) },
        ZC_MYITEMLIST_BUYING_STORE => MapEvent::MyBuyingStore { account: u32_at(4), limit: u32_at(8), items: decode_buying_items(&packet[12..]) },
        ZC_BUYING_STORE_ENTRY => MapEvent::BuyingStoreSign { owner: u32_at(2), title: c_string(&packet[6..86]) },
        ZC_DISAPPEAR_BUYING_STORE_ENTRY => MapEvent::BuyingStoreSignGone { owner: u32_at(2) },
        ZC_ACK_ITEMLIST_BUYING_STORE => {
            MapEvent::BuyingStoreItems { owner: u32_at(4), store_id: u32_at(8), limit: u32_at(12), items: decode_buying_items(&packet[16..]) }
        }
        ZC_FAILED_TRADE_BUYING_STORE_TO_BUYER => MapEvent::BuyingStoreBuyerFailed { result: u16_at(2) },
        ZC_ITEM_DELETE_BUYING_STORE => MapEvent::BuyingStoreSold { index: u16_at(2), amount: u16_at(4), price: u32_at(6) },
        ZC_FAILED_TRADE_BUYING_STORE_TO_SELLER => MapEvent::BuyingStoreSellerFailed { result: u16_at(2), item_id: u32_at(4) },
        ZC_UPDATE_ITEM_FROM_BUYING_STORE2 => MapEvent::BuyingStoreUpdate {
            item_id: u32_at(2),
            amount: u16_at(6),
            zeny: u32_at(8),
            limit_left: u32_at(12),
            char_id: u32_at(16),
            time: u32_at(20),
        },
        ZC_NOTIFY_UNREADMAIL => MapEvent::MailUnread(packet[2] != 0),
        // "blue" (and "ssss" for the War of Emperium) at the start are color hints, not words.
        ZC_MAPPROPERTY_R2 => MapEvent::MapProperty { property: u16_at(2), flags: u32_at(4) },
        ZC_NOTIFY_MAPPROPERTY2 => MapEvent::MapType(u16_at(2)),
        ZC_CONGRATULATION => MapEvent::Congratulation(u32_at(2)),
        ZC_COUPLENAME => MapEvent::PartnerName(c_string(&packet[2..26])),
        ZC_DIVORCE => MapEvent::Divorced(c_string(&packet[2..26])),
        ZC_REQ_BABY => MapEvent::AdoptAsked { parent: u32_at(2), partner: u32_at(6), name: c_string(&packet[10..34]) },
        ZC_BABYMSG => MapEvent::AdoptRefused(u32_at(2)),
        ZC_BATTLEFIELD_NOTIFY_HP2 => MapEvent::BattleHp { account: u32_at(2), hp: u32_at(6) as i32, max: u32_at(10) as i32 },
        // Someone leaving comes with x = y = 0xFFFF.
        ZC_BATTLEFIELD_NOTIFY_POSITION => {
            let cell = [u16_at(32), u16_at(34)];
            MapEvent::BattlePosition { account: u32_at(2), cell: (cell != [0xFFFF; 2]).then_some(cell) }
        }
        ZC_BATTLEFIELD_NOTIFY_CAMPINFO => MapEvent::BattleCamp { account: u32_at(2), camp: u16_at(30) },
        ZC_BATTLEFIELD_NOTIFY_POINT => MapEvent::BattleScore([u16_at(2), u16_at(4)]),
        ZC_ACK_ENTRY_QUEUE_APPLY => MapEvent::BattleQueue { result: packet[2], name: c_string(&packet[3..27]) },
        ZC_NOTIFY_ENTRY_QUEUE_APPLY => MapEvent::QueueNumber { name: c_string(&packet[2..26]), number: u32_at(26) },
        ZC_ACK_ENTRY_QUEUE_CANCEL => MapEvent::QueueLeft { result: packet[2], name: c_string(&packet[3..27]) },
        ZC_NOTIFY_LOBBY_ADMISSION => MapEvent::QueueReady { name: c_string(&packet[2..26]) },
        ZC_REPLY_ACK_LOBBY_ADMISSION => MapEvent::QueueAccepted { result: packet[2], name: c_string(&packet[3..27]) },
        ZC_ENTRY_QUEUE_INIT => MapEvent::QueueReset,
        ZC_BATTLEFIELD_CHAT => MapEvent::BattleChat { account: u32_at(4), text: c_string(packet.get(32..).unwrap_or_default()) },
        // The view at 2 rathena always sends 0 and calls ignored.
        ZC_DRESSROOM_OPEN => MapEvent::DressRoom,
        ZC_ACK_RANKING2 => MapEvent::Ranking {
            kind: u16_at(2),
            ids: std::array::from_fn(|place| u32_at(4 + 4 * place)),
            points: std::array::from_fn(|place| u32_at(44 + 4 * place)),
            mine: u32_at(84),
        },
        // Flag 3: a name follows; 2: none.
        ZC_ACK_REQNAME_BYGID => MapEvent::CharName { id: u32_at(4), name: (u16_at(2) == 3).then(|| c_string(&packet[8..32])) },
        ZC_UPDATE_RANKING_POINT => MapEvent::RankingPoints { kind: u16_at(2), added: u32_at(4) as i32, total: u32_at(8) },
        ZC_OPEN_SEARCH_STORE_INFO => MapEvent::CatalogOpen { remote: u16_at(2) == 1, uses: packet[4] },
        ZC_SEARCH_STORE_INFO_ACK => MapEvent::CatalogResults {
            first: packet[4] != 0,
            more: packet[5] != 0,
            uses: packet[6],
            rows: packet[7..]
                .chunks_exact(CATALOG_ROW_SIZE)
                .map(|row| {
                    let at = |offset: usize| u32::from_le_bytes(row[offset..offset + 4].try_into().unwrap());
                    CatalogRow {
                        store: at(0),
                        account: at(4),
                        shop: c_string(&row[8..88]),
                        item: at(88),
                        kind: row[92],
                        price: at(93),
                        amount: u16::from_le_bytes([row[97], row[98]]),
                        cards: std::array::from_fn(|card| at(99 + 4 * card)),
                        options: options_at(row, 115),
                        refine: row[140],
                    }
                })
                .collect(),
        },
        ZC_SEARCH_STORE_INFO_FAILED => MapEvent::CatalogFailed(packet[2]),
        ZC_QUEST_NOTIFY_EFFECT => MapEvent::QuestMark { npc: u32_at(2), cell: [u16_at(6), u16_at(8)], effect: u16_at(10), color: u16_at(12) },
        ZC_SSILIST_ITEM_CLICK_ACK => {
            let cell = [u16_at(2) as i16, u16_at(4) as i16];
            MapEvent::CatalogMark((cell != [-1, -1]).then_some(cell))
        }
        ZC_STARSKILL => MapEvent::StarSkill { name: c_string(&packet[2..26]), monster: u32_at(26), count: packet[30], kind: packet[31] },
        ZC_NOTIFY_RANKING => MapEvent::PvpRank { account: u32_at(2), rank: u32_at(6), players: u32_at(10) },
        ZC_BROADCAST => {
            let (text, color, kind) = decode_broadcast(&packet[4..]);
            MapEvent::Broadcast { text, color, kind, size: None }
        }
        // The color is 0xRRGGBB, little-endian; the font size is at 10 (type, alignment and Y aren't used).
        ZC_BROADCAST2 => {
            MapEvent::Broadcast { text: c_string(&packet[16..]), color: [packet[6], packet[5], packet[4]], kind: BroadcastKind::Normal, size: Some(u16_at(10)) }
        }
        // The player's name and what it came from each come after their length: a box's item ID (4), a monster's
        // name (24), nothing for an NPC.
        ZC_BROADCASTING_SPECIAL_ITEM_OBTAIN | ZC_BROADCASTING_SPECIAL_ITEM_OBTAIN_ITEM => {
            let name_end = (10 + usize::from(packet[9])).min(packet.len());
            let source_length = packet.get(name_end).map_or(0, |&length| usize::from(length));
            let source = packet.get(name_end + 1..(name_end + 1 + source_length).min(packet.len())).unwrap_or_default();
            let (box_item, monster) = match packet[4] {
                0 => (source.get(..4).map(|id| u32::from_le_bytes(id.try_into().unwrap())), None),
                1 => (None, Some(c_string(source))),
                _ => (None, None),
            };
            MapEvent::ItemObtained { item: u32_at(5), player: c_string(&packet[10..name_end]), box_item, monster }
        }
        ZC_BROADCAST_ITEMREFINING_RESULT => {
            MapEvent::RefineAnnounced { player: c_string(&packet[2..26]), item: u32_at(26), level: packet[30], success: packet[31] != 0 }
        }
        ZC_MVP => MapEvent::Mvp(u32_at(2)),
        ZC_MVP_GETTING_ITEM => MapEvent::MvpItem(u32_at(2)),
        ZC_MVP_GETTING_SPECIAL_EXP => MapEvent::MvpExp(u32_at(2)),
        ZC_THROW_MVPITEM => MapEvent::MvpOverweight,
        ZC_NOTIFY_POSITION_TO_GUILDM => {
            let cell = [i32::from(u16_at(6) as i16), i32::from(u16_at(8) as i16)];
            MapEvent::GuildMemberPlace { account: u32_at(2), cell: (cell[0] >= 0 && cell[1] >= 0).then_some(cell) }
        }
        ZC_WEIGHT_INFO => MapEvent::OverweightPercent(u32_at(2)),
        ZC_ITEMCOMPOSITION_LIST => MapEvent::CardList(packet[4..].chunks_exact(2).map(|index| u16::from_le_bytes([index[0], index[1]])).collect()),
        // The result byte is rathena's "failure".
        ZC_ACK_ITEMCOMPOSITION => MapEvent::CardInserted { equip: u16_at(2), card: u16_at(4), ok: packet[6] == 0 },
        ZC_SPIRITS => MapEvent::Spirits { id: u32_at(2), count: u16_at(6) },
        // Counts are signed; rathena never sends one below 0.
        ZC_MILLENNIUMSHIELD => MapEvent::Shields { id: u32_at(2), count: (u16_at(6) as i16).max(0) as u16 },
        ZC_SPIRITS_ATTRIBUTE => MapEvent::Charms { id: u32_at(2), kind: u16_at(6), count: (u16_at(8) as i16).max(0) as u16 },
        ZC_SOULENERGY => MapEvent::Souls { id: u32_at(2), count: u16_at(6) },
        ZC_TARGET_SPIRITS => MapEvent::ShadowScars { unit: u32_at(2), amount: u16_at(10) },
        ZC_ACK_OPEN_BANKING => MapEvent::BankOpened,
        ZC_BANKING_CHECK => MapEvent::BankBalance(i64::from_le_bytes(packet[2..10].try_into().unwrap())),
        ZC_ACK_BANKING_DEPOSIT | ZC_ACK_BANKING_WITHDRAW => MapEvent::BankMoved {
            deposit: u16_at(0) == ZC_ACK_BANKING_DEPOSIT,
            reason: u16_at(2),
            balance: i64::from_le_bytes(packet[4..12].try_into().unwrap()),
            zeny: u32_at(12) as i32,
        },
        ZC_SE_CASHSHOP_OPEN => MapEvent::CashShopOpen { cash: u32_at(2), free: u32_at(6), tab: u32_at(10) },
        ZC_ACK_SCHEDULER_CASHITEM => {
            if packet.len() < 8 {
                return None;
            }
            let length = usize::from(u16_at(2));
            let count = i16::from_le_bytes([packet[4], packet[5]]);
            if count < 0 || length != 8 + count as usize * 8 || packet.len() < length {
                return None;
            }
            let tab = i16::from_le_bytes([packet[6], packet[7]]);
            let items = packet[8..length].chunks_exact(8).map(|entry| (self::u32_at(entry, 0), self::u32_at(entry, 4))).collect();
            MapEvent::CashShopList { tab, items }
        }
        ZC_SE_PC_BUY_CASHITEM_RESULT => MapEvent::CashShopBuyResult { item: u32_at(2), result: u16_at(6), cash: u32_at(8), free: u32_at(12) },
        ZC_EQUIPWIN_MICROSCOPE => MapEvent::OtherEquipment(Equipped {
            name: c_string(&packet[4..28]),
            job: u16_at(28),
            hair: u16_at(30),
            head_bottom: u16_at(32),
            head_mid: u16_at(34),
            head_top: u16_at(36),
            robe: u16_at(38),
            hair_color: u16_at(40),
            clothes_color: u16_at(42),
            sex: packet[46],
            // EQUIPITEM_INFO, with the item's look at 37.
            items: packet[47..].chunks_exact(EQUIP_ITEM_SIZE).map(|entry| (decode_equip_item(entry), u16::from_le_bytes([entry[37], entry[38]]))).collect(),
        }),
        // Other settings come in ZC_CONFIG too (calling, auto-feeding); only the box's is read.
        ZC_CONFIG if u32_at(2) == CONFIG_OPEN_EQUIPMENT => MapEvent::ShowEquipment { shown: u32_at(6) != 0, answer: true },
        ZC_CONFIG if u32_at(2) == CONFIG_HIDE_COSTUMES => MapEvent::CostumesShown(u32_at(6) == 0),
        ZC_CONFIG if u32_at(2) == CONFIG_PET_AUTOFEED => MapEvent::PetAutoFeed(u32_at(6) != 0),
        ZC_CONFIG if u32_at(2) == CONFIG_HOMUNCULUS_AUTOFEED => MapEvent::HomunculusAutoFeed(u32_at(6) != 0),
        ZC_CONFIG_NOTIFY => MapEvent::ShowEquipment { shown: packet[2] != 0, answer: false },
        ZC_ACK_OPEN_MSGBOX_EXTEND_BODYITEM_SIZE => MapEvent::ExpansionAsked { result: packet[2], voucher: u32_at(3) },
        ZC_ACK_EXTEND_BODYITEM_SIZE => MapEvent::ExpansionDone(packet[2]),
        ZC_NPC_MARKET_OPEN => MapEvent::Market { kind: MarketKind::Market, goods: packet[4..].chunks_exact(19).map(decode_market_goods).collect() },
        ZC_NPC_MARKET_PURCHASE_RESULT => MapEvent::MarketBought {
            ok: u16_at(4) == 0,
            items: packet[6..].chunks_exact(10).map(|entry| (self::u32_at(entry, 0), self::u16_at(entry, 4))).collect(),
        },
        ZC_NPC_BARTER_MARKET_ITEMINFO => MapEvent::Market { kind: MarketKind::Barter, goods: packet[4..].chunks_exact(31).map(decode_barter_goods).collect() },
        ZC_NPC_EXPANDED_BARTER_MARKET_ITEMINFO => MapEvent::Market { kind: MarketKind::Expanded, goods: decode_expanded_barter(&packet[8..]) },
        // Each entry is the item and three materials rathena leaves at 0.
        ZC_MAKABLEITEMLIST => MapEvent::MakeList(packet[4..].chunks_exact(16).map(|entry| self::u32_at(entry, 0)).collect()),
        ZC_MAKINGITEM_LIST => MapEvent::CookList { kind: u16_at(4), items: packet[6..].chunks_exact(4).map(|id| self::u32_at(id, 0)).collect() },
        ZC_MAKINGARROW_LIST => MapEvent::ArrowList(packet[4..].chunks_exact(4).map(|id| self::u32_at(id, 0)).collect()),
        ZC_ACK_REQMAKINGITEM => MapEvent::Made { result: u16_at(2), item: u32_at(4) },
        // REPAIRITEM_INFO2: index, item, cards, refine, grade.
        ZC_REPAIRITEMLIST => MapEvent::RepairList(
            packet[4..]
                .chunks_exact(24)
                .map(|entry| InventoryItem {
                    index: self::u16_at(entry, 0),
                    id: self::u32_at(entry, 2),
                    cards: cards_at(entry, 6),
                    refine: entry[22],
                    identified: true,
                    ..Default::default()
                })
                .collect(),
        ),
        ZC_ACK_ITEMREPAIR => MapEvent::Repaired { index: u16_at(2), ok: packet[4] == 0 },
        // Index, item, refine, cards.
        ZC_NOTIFY_WEAPONITEMLIST => MapEvent::UpgradeList(
            packet[4..]
                .chunks_exact(23)
                .map(|entry| InventoryItem {
                    index: self::u16_at(entry, 0),
                    id: self::u32_at(entry, 2),
                    refine: entry[6],
                    cards: cards_at(entry, 7),
                    identified: true,
                    ..Default::default()
                })
                .collect(),
        ),
        ZC_ACK_WEAPONREFINE => MapEvent::Upgraded { result: u32_at(2), item: u32_at(6) },
        ZC_AUTOSPELLLIST => MapEvent::SpellList(packet[4..].chunks_exact(4).map(|skill| self::u32_at(skill, 0)).collect()),
        ZC_SKILL_SELECT_REQUEST => MapEvent::SkillSelect { why: u32_at(4), skills: packet[8..].chunks_exact(2).map(|skill| self::u16_at(skill, 0)).collect() },
        ZC_OPEN_REFINING_UI => MapEvent::RefineOpen,
        ZC_UI_OPEN_V3 if packet[2] == ENCHANT_UI => MapEvent::EnchantOpen(u64_at(packet, 3)),
        ZC_RESPONSE_ENCHANT => MapEvent::Enchanted { message: u32_at(2), item: u32_at(6) },
        ZC_REFINING_MATERIAL_LIST => MapEvent::RefineMaterials {
            index: u16_at(4),
            blessings: packet[6],
            materials: packet[7..].chunks_exact(9).map(|ore| (self::u32_at(ore, 0), ore[4], self::u32_at(ore, 5))).collect(),
        },
        ZC_ACK_ITEMREFINING => MapEvent::Refined { result: u16_at(2), index: u16_at(4), refine: u16_at(6) },
        ZC_UI_OPEN => MapEvent::UiOpen { kind: packet[2], data: u32_at(3) },
        // Index −1: this level has no grades. Rows of 17 bytes start at 34.
        ZC_GRADE_ENCHANT_MATERIAL_LIST => {
            if packet.len() < 34 {
                return None;
            }
            MapEvent::GradeMaterials {
                index: i16::from_le_bytes([packet[4], packet[5]]),
                chance: u32_at(6),
                blessing: GradeBlessing { item: u32_at(10), per_step: u32_at(14), most: u32_at(18), chance: u32_at(22) },
                materials: packet[34..]
                    .chunks_exact(17)
                    .map(|entry| GradeMaterial {
                        item: self::u32_at(entry, 0),
                        amount: self::u32_at(entry, 4),
                        zeny: self::u32_at(entry, 8),
                        lowers_refine: self::u32_at(entry, 12) != 0,
                        can_break: entry[16] != 0,
                    })
                    .collect(),
            }
        }
        ZC_GRADE_ENCHANT_ACK => MapEvent::GradeResult { index: u16_at(2), grade: u16_at(4), result: u32_at(6) },
        ZC_GRADE_ENCHANT_BROADCAST_RESULT => {
            MapEvent::GradeAnnounced { player: c_string(&packet[2..26]), item: u32_at(26), grade: u16_at(30), success: packet[32] != 0 }
        }
        ZC_RANDOM_COMBINE_ITEM_UI_OPEN => MapEvent::SynthesisOpen(u32_at(2)),
        ZC_ACK_RANDOM_COMBINE_ITEM => MapEvent::Synthesized(u16_at(2)),
        ZC_RANDOM_UPGRADE_ITEM_UI_OPEN => MapEvent::UpgradeOpen(u32_at(2)),
        ZC_ACK_RANDOM_UPGRADE_ITEM => MapEvent::LaphineUpgraded(u16_at(2)),
        // Index, broken, four cards, five random options, refine, grade.
        ZC_CHANGE_ITEM_OPTION => MapEvent::ItemPreview {
            index: u16_at(2),
            broken: packet[4] != 0,
            cards: [u32_at(5), u32_at(9), u32_at(13), u32_at(17)],
            options: options_at(packet, 21),
            refine: packet[46],
            grade: packet[47],
        },
        ZC_OPEN_REFORM_UI => MapEvent::ReformOpen(u32_at(2)),
        ZC_ITEM_REFORM_ACK => MapEvent::Reformed { index: u16_at(2), result: packet[4] },
        ZC_PETEGG_LIST => {
            MapEvent::PetEggs(packet[4..usize::from(u16_at(2)).min(packet.len())].chunks_exact(2).map(|pair| u16::from_le_bytes([pair[0], pair[1]])).collect())
        }
        ZC_CHANGESTATE_PET => MapEvent::PetState { kind: packet[2], id: u32_at(3), value: u32_at(7) as i32 },
        // Name, renamed, level, hunger, intimacy, accessory, monster.
        ZC_MER_INIT => MapEvent::MercenaryInfo(Mercenary {
            id: u32_at(2),
            atk: u16_at(6),
            matk: u16_at(8),
            hit: u16_at(10),
            critical: u16_at(12),
            def: u16_at(14),
            mdef: u16_at(16),
            flee: u16_at(18),
            aspd: u16_at(20),
            name: c_string(&packet[22..46]),
            level: u16_at(46),
            hp: u32_at(48),
            max_hp: u32_at(52),
            sp: u32_at(56),
            max_sp: u32_at(60),
            contract_end: u32_at(64),
            loyalty: u16_at(68),
            summons: u32_at(70),
            kills: u32_at(74),
            range: u16_at(78),
        }),
        // ZC_SKILLINFO_LIST's entries.
        ZC_MER_SKILLINFO_LIST => MapEvent::MercenarySkills(packet[4..].chunks_exact(SKILL_SIZE).map(decode_skill).collect()),
        ZC_MER_PAR_CHANGE => MapEvent::MercenaryChanged { kind: u16_at(2), value: u32_at(4) },
        ZC_EL_INIT => MapEvent::ElementalInfo { id: u32_at(2), values: [u32_at(6), u32_at(10), u32_at(14), u32_at(18)] },
        ZC_EL_PAR_CHANGE => MapEvent::ElementalChanged { kind: u16_at(2), value: u32_at(4) },
        ZC_CHANGESTATE_MER => MapEvent::HomunculusState { state: packet[3], id: u32_at(4), value: u32_at(8) },
        // Name, flags, level, hunger, intimacy, the eight values, HP, SP, EXP, skill points, range.
        ZC_PROPERTY_HOMUN => MapEvent::HomunculusInfo(Homunculus {
            name: c_string(&packet[2..26]),
            flags: packet[26],
            level: u16_at(27),
            hunger: u16_at(29),
            intimacy: u16_at(31),
            atk: u16_at(33),
            matk: u16_at(35),
            hit: u16_at(37),
            critical: u16_at(39),
            def: u16_at(41),
            mdef: u16_at(43),
            flee: u16_at(45),
            aspd: u16_at(47),
            hp: u32_at(49),
            max_hp: u32_at(53),
            sp: u32_at(57),
            max_sp: u32_at(61),
            exp: self::u64_at(packet, 65) as i64,
            next_exp: self::u64_at(packet, 73) as i64,
            skill_points: u16_at(81),
            range: u16_at(83),
        }),
        ZC_HO_PAR_CHANGE2 => MapEvent::HomunculusChanged { kind: u16_at(2), value: self::u64_at(packet, 4) },
        ZC_HOSKILLINFO_LIST => MapEvent::HomunculusSkills(packet[4..].chunks_exact(SKILL_SIZE).map(decode_skill).collect()),
        ZC_HOSKILLINFO_UPDATE => {
            MapEvent::HomunculusSkill(Skill { id: u16_at(2), kind: 0, level: u16_at(4), sp: u16_at(6), range: u16_at(8), upgradable: packet[10] != 0 })
        }
        ZC_FEED_MER => MapEvent::HomunculusFed { ok: packet[2] == 1, food: u32_at(3) },
        ZC_PROPERTY_PET => MapEvent::PetInfo(Pet {
            name: c_string(&packet[2..26]),
            renamed: packet[26] != 0,
            level: u16_at(27),
            hunger: u16_at(29),
            intimacy: u16_at(31),
            accessory: u16_at(33),
            monster: u16_at(35),
        }),
        ZC_FEED_PET => MapEvent::PetFed { ok: packet[2] == 1, food: u32_at(3) },
        ZC_PET_ACT => MapEvent::PetAct { id: u32_at(2), act: PetAct::decode(u32_at(6) as i32) },
        // ID, group, name, title.
        ZC_ACK_REQNAMEALL_NPC => MapEvent::UnitName { id: u32_at(2), name: c_string(&packet[10..34]) },
        ZC_START_CAPTURE => MapEvent::CaptureStart,
        ZC_TRYCAPTURE_MONSTER => MapEvent::CaptureResult(packet[2] == 1),
        ZC_ITEMLISTWIN_OPEN => MapEvent::MaterialWindow(u32_at(2)),
        ZC_ALL_ACH_LIST => MapEvent::Achievements {
            totals: decode_totals(&packet[8..22]),
            list: packet[22..usize::from(u16_at(2)).min(packet.len())].chunks_exact(50).map(decode_achievement).collect(),
        },
        // At login once with ID 0: the totals only (rathena zeroes the rest only in part).
        ZC_ACH_UPDATE => MapEvent::AchievementUpdate {
            totals: decode_totals(&packet[2..16]),
            achievement: Some(decode_achievement(&packet[16..66])).filter(|achievement| achievement.id != 0),
        },
        ZC_REQ_ACH_REWARD_ACK => MapEvent::AchievementReward { ok: packet[2] == 1, id: u32_at(3) },
        ZC_ACK_CHANGE_TITLE => MapEvent::TitleChanged { ok: packet[2] == 0, title: u32_at(3) },
        ZC_SKILL_POSTDELAY => MapEvent::SkillCooldown { skill: u16_at(2), ms: u32_at(4) },
        // Result, serial, the waiting prize's row and column (-1: none), bonus, gold, silver, bronze.
        ZC_ACK_OPEN_ROULETTE => MapEvent::RouletteOpen {
            prize: (packet[7] as i8 >= 0 && packet[8] as i8 >= 0).then_some((packet[7], packet[8])),
            bonus: u32_at(9),
            points: [13, 17, 21].map(|offset| u32_at(offset) as i32),
        },
        // Length, serial, then each prize: row, column, item, amount, 0.
        ZC_ACK_ROULETTE_INFO => MapEvent::RoulettePrizes(
            packet[8..]
                .chunks_exact(12)
                .map(|entry| RoulettePrize { row: entry[0], column: entry[2], item: self::u32_at(entry, 4), amount: self::u16_at(entry, 8) })
                .collect(),
        ),
        ZC_ACK_GENERATE_ROULETTE => MapEvent::RouletteSpun {
            result: packet[2],
            row: u16_at(3) as u8,
            column: u16_at(5) as u8,
            bonus: u32_at(7),
            points: [11, 15, 19].map(|offset| u32_at(offset) as i32),
        },
        ZC_RECV_ROULETTE_ITEM => MapEvent::RouletteClaimed { result: packet[2], bonus: u32_at(3) },
        ZC_NOTIFY_BIND_ON_EQUIP => MapEvent::BoundOnEquip(u16_at(2)),
        ZC_ACK_TAKEOFF_EQUIP_ALL => MapEvent::TookOffAll { refused: packet[2] != 0 },
        // Length, then each item: index, places.
        ZC_EQUIPSWITCH_LIST => MapEvent::SwitchSet(packet[4..].chunks_exact(6).map(|entry| (self::u16_at(entry, 0), self::u32_at(entry, 2))).collect()),
        ZC_EQUIPSWITCH_ADDED => MapEvent::SwitchAdded { index: u16_at(2), places: u32_at(4), result: u16_at(8) },
        ZC_EQUIPSWITCH_REMOVED => MapEvent::SwitchRemoved { index: u16_at(2), places: u32_at(4) },
        ZC_EQUIPSWITCH_REPLY => MapEvent::Switched { failed: u16_at(2) != 0 },
        // rathena's byte is set when the item is out of the favourites (`clif_favorite_item`).
        ZC_INVENTORY_TAB => MapEvent::FavoriteMoved { index: u16_at(2), favorite: packet[4] == 0 },
        // Length, then the indexes.
        ZC_MERGE_ITEM_OPEN => MapEvent::MergeOpen(packet[4..].chunks_exact(2).map(|index| self::u16_at(index, 0)).collect()),
        ZC_ACK_MERGE_ITEM => MapEvent::Merged { index: u16_at(2), amount: u16_at(4), result: packet[6] },
        ZC_CASH_TIME_COUNTER => MapEvent::RentalTime { item: u32_at(2), seconds: u32_at(6) },
        ZC_CASH_ITEM_DELETE => MapEvent::RentalExpired { index: u16_at(2), item: u32_at(4) },
        // Class, level, size, HP, DEF, race, MDEF, element, then the nine elements' damage %.
        ZC_MONSTER_INFO => MapEvent::MonsterInfo(MonsterInfo {
            class: u16_at(2),
            level: u16_at(4),
            size: u16_at(6),
            hp: u32_at(8),
            def: u16_at(12) as i16,
            race: u16_at(14),
            mdef: u16_at(16) as i16,
            element: u16_at(18),
            resists: packet[20..29].try_into().unwrap(),
        }),
        // Kind, x, y, hours, minutes, two unused, the name.
        ZC_BOSS_INFO => MapEvent::BossInfo {
            kind: packet[2],
            cell: [u32_at(3) as i32, u32_at(7) as i32],
            hours: u16_at(11),
            minutes: u16_at(13),
            name: c_string(&packet[19..70]),
        },
        ZC_TALKBOX_CHATCONTENTS => MapEvent::TalkieBox { unit: u32_at(2), text: c_string(&packet[6..27]) },
        ZC_MSG => MapEvent::Message { id: usize::from(u16_at(2)), value: None, skill: None, color: None },
        ZC_MSG_VALUE => MapEvent::Message { id: usize::from(u16_at(2)), value: Some(u32_at(4) as i32), skill: None, color: None },
        ZC_MSG_SKILL => MapEvent::Message { id: u32_at(4) as usize, value: None, skill: Some(u16_at(2)), color: None },
        // 0xBBGGRR, like ZC_NPC_CHAT's.
        ZC_MSG_COLOR => MapEvent::Message { id: usize::from(u16_at(2)), value: None, skill: None, color: Some([packet[4], packet[5], packet[6]]) },
        ZC_ACTION_FAILURE => MapEvent::ActionFailure(u16_at(2)),
        ZC_ACK_REMEMBER_WARPPOINT => MapEvent::MemoResult(packet[2]),
        ZC_SOUND => MapEvent::Sound { file: c_string(&packet[2..26]), act: packet[26], unit: u32_at(31) },
        ZC_PLAY_NPC_BGM => MapEvent::NpcMusic(c_string(&packet[2..26])),
        // playType (offset 4) is 0, which loops; the player already loops whatever a script starts.
        ZC_PLAY_NPC_BGM2 => MapEvent::NpcMusic(c_string(&packet[5..])),
        ZC_SPECIALPOPUP => MapEvent::SpecialPopup(i32_at(2)),
        ZC_WARPLIST => MapEvent::WarpList { skill: u16_at(4), maps: packet[6..].chunks_exact(16).map(c_string).filter(|map| !map.is_empty()).collect() },
        ZC_NOTIFY_MAPINFO => MapEvent::MapForbids(u16_at(2)),
        // Each quest: ID, state (1 active), start and end times, hunts. Each hunt is 44 bytes as clif_quest_send_list
        // writes it (its 46 is only the buffer's estimate): ID, type, monster, level range, count, needed, name.
        ZC_ALL_QUEST_LIST3 => {
            let mut quests = Vec::new();
            let mut at = 8;
            while at + 15 <= packet.len() {
                let count = usize::from(u16_at(at + 13));
                let hunts = packet[at + 15..].chunks_exact(44).take(count).map(|hunt| Hunt {
                    id: self::u32_at(hunt, 0),
                    count: self::u16_at(hunt, 16),
                    needed: self::u16_at(hunt, 18),
                    name: c_string(&hunt[20..44]),
                    levels: [self::u16_at(hunt, 12), self::u16_at(hunt, 14)],
                });
                quests.push(Quest { id: u32_at(at), active: packet[at + 4] == 1, end: u32_at(at + 9), hunts: hunts.collect() });
                at += 15 + count * 44;
            }
            MapEvent::Quests(quests)
        }
        // A quest taken: as above, its hunts 42 bytes without how many they need (ZC_HUNTING_QUEST_INFO follows).
        ZC_ADD_QUEST3 => {
            let count = usize::from(u16_at(15)).min(3);
            let hunts = packet[17..].chunks_exact(42).take(count).map(|hunt| Hunt {
                id: self::u32_at(hunt, 0),
                count: self::u16_at(hunt, 16),
                needed: 0,
                name: c_string(&hunt[18..42]),
                levels: [self::u16_at(hunt, 12), self::u16_at(hunt, 14)],
            });
            MapEvent::QuestAdded(Quest { id: u32_at(2), active: packet[6] == 1, end: u32_at(11), hunts: hunts.collect() })
        }
        ZC_HUNTING_QUEST_INFO => {
            MapEvent::QuestHunts(packet[4..].chunks_exact(12).map(|hunt| (self::u32_at(hunt, 0), self::u16_at(hunt, 10), self::u16_at(hunt, 8))).collect())
        }
        ZC_UPDATE_MISSION_HUNT_EX => {
            MapEvent::QuestProgress(packet[6..].chunks_exact(12).map(|hunt| (self::u32_at(hunt, 4), self::u16_at(hunt, 10), self::u16_at(hunt, 8))).collect())
        }
        ZC_DEL_QUEST => MapEvent::QuestRemoved(u32_at(2)),
        ZC_ACTIVE_QUEST => MapEvent::QuestActive { id: u32_at(2), active: packet[6] != 0 },
        // Each mail: box, ID, read, flags, sender, seconds left, title (its length first).
        // The result, the count, then 93-byte entries.
        ZC_ACK_GUILDSTORAGE_LOG => {
            let entries = packet
                .get(8..)
                .unwrap_or_default()
                .chunks_exact(GUILD_STORAGE_ENTRY)
                .take(usize::from(u16_at(6)))
                .map(|entry| {
                    let at = |offset: usize| u32::from_le_bytes(entry[offset..offset + 4].try_into().unwrap());
                    GuildStorageEntry {
                        id: at(0),
                        item: at(4),
                        amount: at(8) as i32,
                        stored: entry[12] == 1,
                        refine: at(13) as i32,
                        unique_id: u64_at(entry, 17),
                        identified: entry[25] != 0,
                        kind: u16::from_le_bytes([entry[26], entry[27]]),
                        cards: [28, 32, 36, 40].map(at),
                        name: c_string(&entry[44..68]),
                        time: c_string(&entry[68..92]),
                        damaged: entry[92] == 1,
                    }
                })
                .collect();
            MapEvent::GuildStorageLog { result: u16_at(4), entries }
        }
        ZC_ACK_MAIL_LIST3 => {
            let mut mails = Vec::new();
            let mut at = 5;
            while at + 41 <= packet.len() {
                let title_length = usize::from(u16_at(at + 39));
                mails.push(MailHeader {
                    kind: packet[at],
                    id: u64_at(packet, at + 1),
                    read: packet[at + 9] != 0,
                    flags: packet[at + 10],
                    sender: c_string(&packet[at + 11..at + 35]),
                    expires: u32_at(at + 35),
                    title: c_string(&packet[at + 41..(at + 41 + title_length).min(packet.len())]),
                });
                at += 41 + title_length;
            }
            MapEvent::MailList(mails)
        }
        // The text (its length first), then 60-byte items.
        ZC_ACK_READ_RODEX => {
            let text_end = (24 + usize::from(u16_at(13))).min(packet.len());
            let items = packet[text_end..]
                .chunks_exact(60)
                .take(usize::from(packet[23]))
                .map(|entry| InventoryItem {
                    amount: self::u16_at(entry, 0),
                    id: self::u32_at(entry, 2),
                    identified: entry[6] != 0,
                    broken: entry[7] != 0,
                    cards: cards_at(entry, 8),
                    location: self::u32_at(entry, 24),
                    kind: entry[28],
                    options: options_at(entry, 33),
                    refine: entry[58],
                    grade: entry[59],
                    ..Default::default()
                })
                .collect();
            MapEvent::MailRead { kind: packet[4], id: u64_at(packet, 5), text: c_string(&packet[24..text_end]), zeny: u64_at(packet, 15), items }
        }
        ZC_ACK_ZENY_FROM_MAIL | ZC_ACK_ITEM_FROM_MAIL => {
            MapEvent::MailClaimed { id: u64_at(packet, 2), zeny: u16_at(0) == ZC_ACK_ZENY_FROM_MAIL, result: packet[11] }
        }
        ZC_ACK_DELETE_MAIL => MapEvent::MailDeleted { id: u64_at(packet, 3) },
        ZC_ACK_OPEN_WRITE_MAIL => MapEvent::MailWrite { to: c_string(&packet[2..26]), ok: packet[26] != 0 },
        ZC_CHECKNAME => MapEvent::MailReceiver { character: u32_at(2), job: u16_at(6), level: u16_at(8), name: c_string(&packet[10..34]) },
        ZC_ACK_ADD_ITEM_RODEX => MapEvent::MailItemAdded {
            result: packet[2],
            item: InventoryItem {
                index: u16_at(3),
                amount: u16_at(5),
                id: u32_at(7),
                kind: packet[11],
                identified: packet[12] != 0,
                cards: cards_at(packet, 14),
                options: options_at(packet, 30),
                location: u32_at(58),
                refine: packet[62],
                ..Default::default()
            },
            weight: u16_at(55),
        },
        ZC_ACK_REMOVE_ITEM_MAIL => MapEvent::MailItemRemoved { ok: packet[2] != 0, index: u16_at(3), amount: u16_at(5), weight: u16_at(7) },
        ZC_ACK_WRITE_MAIL => MapEvent::MailSent(packet[2]),
        ZC_NOTIFY_CARTITEM_COUNTINFO => MapEvent::CartCount { count: u16_at(2), most: u16_at(4), weight: u32_at(6) as i32, max_weight: u32_at(10) as i32 },
        ZC_DELETE_ITEM_FROM_CART => MapEvent::CartRemoved { index: u16_at(2), amount: u32_at(4) },
        // Index, amount, item, type, identified, damaged, cards, options, refine, grade.
        ZC_ADD_ITEM_TO_CART => MapEvent::CartAdded(InventoryItem {
            index: u16_at(2),
            amount: u32_at(4).min(u32::from(u16::MAX)) as u16,
            id: u32_at(8),
            kind: packet[12],
            identified: packet[13] != 0,
            cards: cards_at(packet, 15),
            options: options_at(packet, 31),
            refine: packet[56],
            ..Default::default()
        }),
        ZC_UPDATE_GDID => MapEvent::Guild { id: u32_at(2), name: c_string(&packet[19..43]), master: packet[14] != 0, emblem: u32_at(6), mode: u32_at(10) },
        // ID, level, members, most members, average level, exp, next exp, tax points, honor, virtue, emblem, name,
        // castles, zeny, master's ID and name.
        ZC_ACK_GUILD_MENUINTERFACE => MapEvent::GuildMenu(u32_at(2) as i32),
        ZC_GUILD_INFO => MapEvent::GuildInfo(GuildInfo {
            id: u32_at(2),
            level: u32_at(6),
            members: u32_at(10),
            max_members: u32_at(14),
            average_level: u32_at(18),
            exp: u32_at(22),
            next_exp: u32_at(26),
            tax_points: u32_at(30),
            emblem: u32_at(42),
            name: c_string(&packet[46..70]),
            territory: c_string(&packet[70..86]),
            master: c_string(&packet[94..118]),
        }),
        // Each member: account, character, hair, hair color, sex, job, level, contribution, online, position, last
        // login, name.
        ZC_MEMBERMGR_INFO => MapEvent::GuildMembers(
            packet[4..]
                .chunks_exact(58)
                .map(|member| GuildMember {
                    account: self::u32_at(member, 0),
                    character: self::u32_at(member, 4),
                    hair: self::u16_at(member, 8),
                    hair_color: self::u16_at(member, 10),
                    sex: self::u16_at(member, 12),
                    job: self::u16_at(member, 14),
                    level: self::u16_at(member, 16),
                    contribution: self::u32_at(member, 18),
                    online: self::u32_at(member, 22) != 0,
                    position: self::u32_at(member, 26),
                    last_login: self::u32_at(member, 30),
                    name: c_string(&member[34..58]),
                })
                .collect(),
        ),
        ZC_POSITION_ID_NAME_INFO => {
            MapEvent::GuildPositions(packet[4..].chunks_exact(28).map(|entry| (self::u32_at(entry, 0), c_string(&entry[4..28]))).collect())
        }
        // rathena sends one entry a packet: ID, permissions, rank, tax, title.
        ZC_ACK_CHANGE_GUILD_POSITIONINFO if packet.len() >= 44 => {
            MapEvent::GuildPositionChanged(GuildPosition { id: u32_at(4), mode: u32_at(8), rank: u32_at(12), tax: u32_at(16) }, c_string(&packet[20..44]))
        }
        // One member a packet: account, character, position.
        ZC_ACK_REQ_CHANGE_MEMBERS if packet.len() >= 16 => {
            MapEvent::GuildMemberPosition { account: u32_at(4), character: u32_at(8), position: (u32_at(12) as i32).max(0) as u32 }
        }
        ZC_GUILD_POSITION => MapEvent::PositionTitle { account: u32_at(4), title: c_string(packet.get(8..).unwrap_or_default()) },
        // Each position: ID, permissions, rank, tax.
        ZC_POSITION_INFO => MapEvent::GuildPositionInfo(
            packet[4..]
                .chunks_exact(16)
                .map(|entry| GuildPosition {
                    id: self::u32_at(entry, 0),
                    mode: self::u32_at(entry, 4),
                    rank: self::u32_at(entry, 8),
                    tax: self::u32_at(entry, 12),
                })
                .collect(),
        ),
        // The points left, then each skill as ZC_SKILLINFO_LIST has them.
        ZC_GUILD_SKILLINFO => MapEvent::GuildSkills { points: u16_at(4), skills: packet[6..].chunks_exact(SKILL_SIZE).map(decode_skill).collect() },
        // Each: character ID, reason, name.
        ZC_BAN_LIST => MapEvent::GuildExpelled(packet[4..].chunks_exact(68).map(|entry| (c_string(&entry[44..68]), c_string(&entry[4..44]))).collect()),
        ZC_GUILD_EMBLEM_IMG => MapEvent::GuildEmblem { guild: u32_at(4), version: u32_at(8), data: packet[12..].to_vec() },
        ZC_CHANGE_GUILD => MapEvent::GuildEmblemChanged { guild: u32_at(2), emblem: u32_at(6), account: u32_at(10) },
        // ID, name, party, guild, position, title.
        ZC_ACK_REQNAMEALL => {
            let title = (u32_at(102) as i32).max(0) as u32;
            MapEvent::NameTag(
                u32_at(2),
                NameTag { party: c_string(&packet[30..54]), guild: c_string(&packet[54..78]), position: c_string(&packet[78..102]), title },
            )
        }
        ZC_GUILD_NOTICE => MapEvent::GuildNotice { subject: c_string(&packet[2..62]), text: c_string(&packet[62..182]) },
        ZC_GUILD_CHAT => MapEvent::GuildChat(c_string(&packet[4..])),
        ZC_UPDATE_CHARSTAT => MapEvent::GuildMemberOnline { account: u32_at(2), character: u32_at(6), online: u32_at(10) != 0 },
        ZC_RESULT_MAKE_GUILD => MapEvent::GuildMade(packet[2]),
        ZC_REQ_JOIN_GUILD => MapEvent::GuildInvite { id: u32_at(2), name: c_string(&packet[6..30]) },
        ZC_ACK_REQ_JOIN_GUILD => MapEvent::GuildInviteResult(packet[2]),
        // Length, then each guild: relation, ID, name.
        ZC_MYGUILD_BASIC_INFO => MapEvent::GuildRelations(
            packet[4..]
                .chunks_exact(32)
                .map(|entry| GuildRelation { antagonist: self::u32_at(entry, 0) == 1, id: self::u32_at(entry, 4), name: c_string(&entry[8..32]) })
                .collect(),
        ),
        ZC_REQ_ALLY_GUILD => MapEvent::AllyAsked { id: u32_at(2), guild: c_string(&packet[6..30]) },
        ZC_ACK_REQ_ALLY_GUILD => MapEvent::AllyResult(packet[2]),
        ZC_ACK_REQ_HOSTILE_GUILD => MapEvent::AntagonistResult(packet[2]),
        ZC_DELETE_RELATED_GUILD => MapEvent::GuildRelationEnded { id: u32_at(2), antagonist: u32_at(6) == 1 },
        ZC_ACK_DISORGANIZE_GUILD_RESULT => MapEvent::GuildDisbanded(u32_at(2)),
        ZC_ACK_LEAVE_GUILD => MapEvent::GuildLeft { character: u32_at(2), reason: c_string(&packet[6..46]), expelled: false },
        ZC_ACK_BAN_GUILD => MapEvent::GuildLeft { character: u32_at(42), reason: c_string(&packet[2..42]), expelled: true },
        ZC_CLANINFO => {
            let allies = usize::from(packet[72]);
            let hostile = usize::from(packet[73]);
            let mut names = packet[74..].chunks_exact(24).map(c_string);
            MapEvent::ClanInfo {
                id: u32_at(4),
                name: c_string(&packet[8..32]),
                master: c_string(&packet[32..56]),
                map: c_string(&packet[56..72]),
                allies: (&mut names).take(allies).collect(),
                hostile: names.take(hostile).collect(),
            }
        }
        ZC_NOTIFY_CLAN_CONNECTINFO => MapEvent::ClanOnlineCount { online: u16_at(2) as i16, max: u16_at(4) as i16 },
        ZC_ACK_CLAN_LEAVE => MapEvent::ClanLeft,
        ZC_NOTIFY_CLAN_CHAT => MapEvent::ClanChat(c_string(&packet[28..])),
        ZC_FRIENDS_LIST => MapEvent::Friends(
            packet[4..]
                .chunks_exact(32)
                .map(|entry| Friend { account: self::u32_at(entry, 0), character: self::u32_at(entry, 4), name: c_string(&entry[8..32]), online: false })
                .collect(),
        ),
        ZC_FRIENDS_STATE => MapEvent::FriendOnline { account: u32_at(2), character: u32_at(6), online: packet[10] == 0 },
        ZC_REQ_ADD_FRIENDS => MapEvent::FriendAsked { account: u32_at(2), character: u32_at(6), name: c_string(&packet[10..34]) },
        ZC_ADD_FRIENDS_LIST => MapEvent::FriendAdded {
            result: u16_at(2),
            friend: Friend { account: u32_at(4), character: u32_at(8), name: c_string(&packet[12..36]), online: true },
        },
        ZC_DELETE_FRIENDS => MapEvent::FriendRemoved { account: u32_at(2), character: u32_at(6) },
        // Points left; each stat with the points it needs to go up; Atk, Matk (max, min), Def, Mdef, Hit, Flee,
        // Critical, ASPD with their bonuses.
        ZC_STATUS => {
            let mut values = vec![(STATUS_POINTS, i64::from(u16_at(2)))];
            for stat in 0..6u16 {
                values.push((STATUS_STR + stat, i64::from(packet[4 + usize::from(stat) * 2])));
                values.push((STATUS_STR_COST + stat, i64::from(packet[5 + usize::from(stat) * 2])));
            }
            let short = |offset: usize| i64::from(u16_at(offset) as i16);
            // SP_ATK1, ATK2, MATK1 (max), MATK2 (min), DEF1, DEF2, MDEF1, MDEF2, HIT, FLEE1, FLEE2, CRITICAL, ASPD.
            for (place, offset) in [16, 18, 20, 22, 24, 26, 28, 30, 32, 34, 36, 38, 40].into_iter().enumerate() {
                values.push((STATUS_ATK + place as u16, short(offset)));
            }
            MapEvent::Statuses(values)
        }
        ZC_STATUS_CHANGE => MapEvent::Statuses(vec![(u16_at(2), i64::from(packet[4]))]),
        ZC_STATUS_CHANGE_ACK => MapEvent::RaiseAnswer((packet[4] != 0).then(|| (u16_at(2), i64::from(packet[5])))),
        ZC_COUPLESTATUS => {
            let id = u32_at(2) as u16;
            MapEvent::Statuses(vec![(id, i64::from(u32_at(6) as i32)), (id + STATUS_BONUS, i64::from(u32_at(10) as i32))])
        }
        ZC_RESTART_ACK => MapEvent::RestartAck(packet[2]),
        ZC_EMOTION => MapEvent::Emotion { id: u32_at(2), kind: packet[6] },
        ZC_ACK_REQ_DISCONNECT => MapEvent::QuitAck(u16_at(2)),
        ZC_RESURRECTION => MapEvent::Revived { id: u32_at(2) },
        ZC_ROOM_NEWENTRY | ZC_CHANGE_CHATROOM => {
            MapEvent::ChatRoom { owner: u32_at(4), id: u32_at(8), limit: u16_at(12), users: u16_at(14), kind: packet[16], title: c_string(&packet[17..]) }
        }
        ZC_DESTROY_ROOM => MapEvent::ChatRoomGone { id: u32_at(2) },
        ZC_ACK_CREATE_CHATROOM => MapEvent::ChatRoomMade(packet[2]),
        ZC_REFUSE_ENTER_ROOM => MapEvent::ChatRoomRefused(packet[2]),
        ZC_ENTER_ROOM => MapEvent::ChatRoomEntered {
            id: u32_at(4),
            members: packet[8..].chunks_exact(28).map(|member| (c_string(&member[4..28]), self::u32_at(member, 0) == 0)).collect(),
        },
        ZC_MEMBER_NEWENTRY => MapEvent::ChatRoomJoined { users: u16_at(2), name: c_string(&packet[4..28]) },
        ZC_MEMBER_EXIT => MapEvent::ChatRoomLeft { users: u16_at(2), name: c_string(&packet[4..28]), kicked: packet[28] != 0 },
        ZC_ROLE_CHANGE => MapEvent::ChatRoomOwner { owner: u32_at(2) == 0, name: c_string(&packet[6..30]) },
        // Party name, then each member: account, character, name, map, leader (0 is), offline, job, level.
        ZC_GROUP_LIST => MapEvent::Party {
            name: c_string(&packet[4..28]),
            members: packet[28..]
                .chunks_exact(54)
                .map(|member| PartyMember {
                    account: self::u32_at(member, 0),
                    character: self::u32_at(member, 4),
                    name: c_string(&member[8..32]),
                    map: c_string(&member[32..48]),
                    leader: member[48] == 0,
                    online: member[49] == 0,
                    job: self::u16_at(member, 50),
                    level: self::u16_at(member, 52),
                })
                .collect(),
        },
        // Account, character, leader (0 is), job, level, x, y, offline, party, name, map, pickup and share rules.
        ZC_ADD_MEMBER_TO_GROUP => MapEvent::PartyMember {
            party: c_string(&packet[23..47]),
            member: PartyMember {
                account: u32_at(2),
                character: u32_at(6),
                name: c_string(&packet[47..71]),
                map: c_string(&packet[71..87]),
                leader: u32_at(10) == 0,
                online: packet[22] == 0,
                job: u16_at(14),
                level: u16_at(16),
            },
            cell: [u16_at(18), u16_at(20)],
            pickup: packet[87] != 0,
            share: packet[88] != 0,
        },
        ZC_DELETE_MEMBER_FROM_GROUP => MapEvent::PartyLeft { account: u32_at(2), name: c_string(&packet[6..30]), result: packet[30] },
        ZC_ACK_MAKE_GROUP => MapEvent::PartyMade(packet[2]),
        ZC_PARTY_REQ_MASTER_TO_JOIN => {
            MapEvent::BookingJoinAsked { character: u32_at(2), account: u32_at(6), name: c_string(&packet[10..34]), level: u16_at(34), job: u16_at(36) }
        }
        ZC_PARTY_JOIN_REQ_ACK_FROM_MASTER => {
            MapEvent::BookingJoinAnswered { leader: c_string(&packet[2..26]), party: c_string(&packet[26..50]), account: u32_at(50), refused: u32_at(54) != 0 }
        }
        ZC_PARTY_JOIN_REQ => MapEvent::PartyInvite { party: u32_at(2), name: c_string(&packet[6..30]) },
        ZC_PARTY_JOIN_REQ_ACK => MapEvent::PartyInviteResult { name: c_string(&packet[2..26]), result: u32_at(26) },
        ZC_NOTIFY_CHAT_PARTY => MapEvent::PartyChat { account: u32_at(4), text: c_string(&packet[8..]) },
        // clif_party_xy_remove sends -1, -1: the member left the map.
        ZC_NOTIFY_POSITION_TO_GROUPM => {
            let cell = [u16_at(6), u16_at(8)];
            MapEvent::PartyPosition { account: u32_at(2), cell: (cell != [u16::MAX; 2]).then_some(cell) }
        }
        ZC_ITEM_PICKUP_PARTY => MapEvent::PartyPickup { account: u32_at(2), item: u32_at(6), identified: packet[10] != 0, refine: packet[31] },
        ZC_PARTY_CONFIG => MapEvent::PartyRefusing(packet[2] != 0),
        ZC_NOTIFY_HP_TO_GROUPM => MapEvent::PartyHp { account: u32_at(2), hp: u32_at(6) as i32, max: u32_at(10) as i32 },
        ZC_REQ_GROUPINFO_CHANGE_V2 => MapEvent::PartyOptions { exp: u32_at(2), pickup: packet[6] != 0, share: packet[7] != 0 },
        ZC_CHANGE_GROUP_MASTER => MapEvent::PartyLeader { old: u32_at(2), new: u32_at(6) },
        ZC_NOTIFY_MEMBERINFO_TO_GROUPM => MapEvent::PartyMemberInfo { account: u32_at(2), job: u16_at(6), level: u16_at(8) },
        ZC_GROUP_ISALIVE => MapEvent::PartyDead { account: u32_at(2), dead: packet[6] != 0 },
        ZC_REQ_EXCHANGE_ITEM => MapEvent::TradeAsked { name: c_string(&packet[2..26]), id: u32_at(26), level: u16_at(30) },
        ZC_ACK_EXCHANGE_ITEM => MapEvent::TradeAnswer { result: packet[2], id: u32_at(3), level: u16_at(7) },
        // Item, type, amount, identified, damaged, cards, options, where it's worn, look, refine, grade; no item is zeny.
        ZC_ADD_EXCHANGE_ITEM if u32_at(2) == 0 => MapEvent::TradeZeny(u32_at(7)),
        ZC_ADD_EXCHANGE_ITEM => MapEvent::TradeItem(InventoryItem {
            id: u32_at(2),
            kind: packet[6],
            amount: u32_at(7).min(u32::from(u16::MAX)) as u16,
            identified: packet[11] != 0,
            cards: cards_at(packet, 13),
            options: options_at(packet, 29),
            location: u32_at(54),
            refine: packet[60],
            ..Default::default()
        }),
        ZC_ACK_ADD_EXCHANGE_ITEM => MapEvent::TradeAdded { index: u16_at(2), result: packet[4] },
        ZC_CONCLUDE_EXCHANGE_ITEM => MapEvent::TradeLocked { partner: packet[2] != 0 },
        ZC_CANCEL_EXCHANGE_ITEM => MapEvent::TradeCancelled,
        ZC_EXEC_EXCHANGE_ITEM => MapEvent::TradeDone { result: packet[2] },
        ZC_SHOW_IMAGE => MapEvent::Cutin { image: c_string(&packet[2..66]), place: packet[66] },
        ZC_COMPASS => MapEvent::Mark {
            npc: u32_at(2),
            kind: u32_at(6),
            cell: [u32_at(10) as u16, u32_at(14) as u16],
            id: packet[18],
            color: [packet[21], packet[20], packet[19]],
        },
        ZC_EQUIPMENT_EFFECT if packet.len() >= 9 => MapEvent::HatEffects {
            id: u32_at(4),
            on: packet[8] != 0,
            effects: packet[9..].chunks_exact(2).map(|number| u16::from_le_bytes([number[0], number[1]])).collect(),
        },
        ZC_NOTIFY_EFFECT2 => MapEvent::Effect { id: u32_at(2), effect: u32_at(6), num: None },
        ZC_NOTIFY_EFFECT3 => MapEvent::Effect { id: u32_at(2), effect: u32_at(6), num: Some(i32_at(10)) },
        ZC_SHORTCUT_KEY_LIST => MapEvent::Hotkeys {
            rotate: packet[2],
            tab: u16_at(3),
            keys: packet[5..]
                .chunks_exact(HOTKEY_SIZE)
                .map(|entry| {
                    let id = self::u32_at(entry, 1);
                    (id != 0).then(|| Hotkey { skill: entry[0] != 0, id, count: i16::from_le_bytes([entry[5], entry[6]]) })
                })
                .collect(),
        },
        ZC_SPRITE_CHANGE => MapEvent::LookChange { id: u32_at(2), kind: packet[6], value: u32_at(7), extra: u32_at(11) },
        ZC_NPCSPRITE_CHANGE => MapEvent::JobSprite { id: u32_at(2), job: u32_at(7) },
        // clif_changeoption_target: AID, bodyState, healthState, effectState, PK mode.
        ZC_STATE_CHANGE => MapEvent::OptionChange { id: u32_at(2), body: u16_at(6), health: u16_at(8), option: u32_at(10) },
        ZC_REQ_WEAR_EQUIP_ACK => MapEvent::Equipped { index: u16_at(2), location: u32_at(4), result: packet[10] },
        // Since 2011 the flag is inverted: 0 means it came off.
        ZC_REQ_TAKEOFF_EQUIP_ACK => MapEvent::Unequipped { index: u16_at(2), location: u32_at(4), failed: packet[8] != 0 },
        ZC_ATTACK_FAILURE_FOR_DISTANCE => {
            MapEvent::TooFar { target: u32_at(2), target_cell: [u16_at(6), u16_at(8)], cell: [u16_at(10), u16_at(12)], range: u16_at(14) }
        }
        ZC_STYLE_CHANGE_RES => MapEvent::StyleChanged(packet[2] != 0),
        ZC_INSTANCE_CREATE => MapEvent::InstanceCreate { name: c_string(&packet[2..2 + INSTANCE_NAME_LENGTH]), place: u16_at(2 + INSTANCE_NAME_LENGTH) },
        ZC_INSTANCE_WAIT => MapEvent::InstanceWait { place: u16_at(2) },
        ZC_INSTANCE_STATUS => MapEvent::InstanceStatus {
            name: c_string(&packet[2..2 + INSTANCE_NAME_LENGTH]),
            time_limit: u32_at(2 + INSTANCE_NAME_LENGTH),
            idle_limit: u32_at(2 + INSTANCE_NAME_LENGTH + 4),
        },
        ZC_INSTANCE_NOTIFY => MapEvent::InstanceNotify { kind: u32_at(2), time: u32_at(6) },
        _ => return None,
    })
}
