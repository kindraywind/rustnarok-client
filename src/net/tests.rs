use super::*;

#[test]
fn reads_guild_member_records() {
    // 0b7d <length>.W, then 58 bytes a member: AID, GID, hair, hair colour, sex, job, level, contribution, state,
    // position, last login, name.
    let mut record = Vec::new();
    for value in [7u32, 150_000] {
        record.extend(value.to_le_bytes());
    }
    for value in [12u16, 3, 1, 4001, 99] {
        record.extend(value.to_le_bytes());
    }
    for value in [2_147_483_647u32, 1, 5, 1_792_368_000] {
        record.extend(value.to_le_bytes());
    }
    let mut name = b"Crab".to_vec();
    name.resize(24, 0);
    record.extend(name);
    let packet = [&ZC_MEMBERMGR_INFO.to_le_bytes()[..], &(4u16 + 58).to_le_bytes(), &record].concat();
    let Some(MapEvent::GuildMembers(members)) = decode_map_packet(&packet) else { panic!("not decoded") };
    let member = &members[0];
    assert_eq!((member.account, member.character, member.hair, member.hair_color, member.sex), (7, 150_000, 12, 3, 1));
    assert_eq!((member.job, member.level, member.contribution, member.online, member.position), (4001, 99, 2_147_483_647, true, 5));
    assert_eq!((member.last_login, member.name.as_str()), (1_792_368_000, "Crab"));
}

#[test]
fn encodes_login_request() {
    let packet = encode_login(55, "tester", "a-password-longer-than-twenty-three");
    assert_eq!(packet.len(), 55);
    assert_eq!(&packet[..2], &[0x64, 0x00]);
    assert_eq!(&packet[2..6], &55u32.to_le_bytes(), "clientinfo.xml's version");
    assert_eq!(&packet[6..13], b"tester\0");
    // Truncated to 23 bytes, keeping the terminating NUL.
    assert_eq!(packet[30 + 22], b'-');
    assert_eq!(packet[30 + 23], 0);
}

#[test]
fn asks_and_answers_through_booking_posts() {
    assert_eq!(encode_pair(CZ_PARTY_REQ_MASTER_TO_JOIN, 150004, 2000001), [0xE6, 0x0A, 0xF4, 0x49, 2, 0, 0x81, 0x84, 0x1E, 0]);
    let mut asked = vec![0u8; 38];
    asked[..2].copy_from_slice(&ZC_PARTY_REQ_MASTER_TO_JOIN.to_le_bytes());
    asked[2..6].copy_from_slice(&150005u32.to_le_bytes());
    asked[6..10].copy_from_slice(&2000002u32.to_le_bytes());
    asked[10..16].copy_from_slice(b"LaneA2");
    asked[34..36].copy_from_slice(&60u16.to_le_bytes());
    asked[36..38].copy_from_slice(&4257u16.to_le_bytes());
    let Some(MapEvent::BookingJoinAsked { character, account, name, level, job }) = decode_map_packet(&asked) else { panic!() };
    assert_eq!((character, account, name.as_str(), level, job), (150005, 2000002, "LaneA2", 60, 4257));
    let mut answered = vec![0u8; 58];
    answered[..2].copy_from_slice(&ZC_PARTY_JOIN_REQ_ACK_FROM_MASTER.to_le_bytes());
    answered[2..8].copy_from_slice(b"LaneA1");
    answered[26..31].copy_from_slice(b"Crabs");
    answered[50..54].copy_from_slice(&2000001u32.to_le_bytes());
    let Some(MapEvent::BookingJoinAnswered { leader, party, account, refused }) = decode_map_packet(&answered) else { panic!() };
    assert_eq!((leader.as_str(), party.as_str(), account, refused), ("LaneA1", "Crabs", 2000001, false));
}

#[test]
fn decodes_accept_and_refuse() {
    let mut packet = vec![0u8; ACCEPT_HEADER + CHAR_SERVER_SIZE];
    packet[..2].copy_from_slice(&AC_ACCEPT_LOGIN.to_le_bytes());
    packet[2..4].copy_from_slice(&((ACCEPT_HEADER + CHAR_SERVER_SIZE) as u16).to_le_bytes());
    packet[8..12].copy_from_slice(&2_000_000u32.to_le_bytes());
    packet[46] = 1;
    packet[47..63].copy_from_slice(b"0123456789abcdef");
    let server = &mut packet[ACCEPT_HEADER..];
    server[..4].copy_from_slice(&[127, 0, 0, 1]);
    server[4..6].copy_from_slice(&6121u16.to_le_bytes());
    server[6..13].copy_from_slice(b"rAthena");
    server[26..28].copy_from_slice(&3u16.to_le_bytes());
    server[28..30].copy_from_slice(&2u16.to_le_bytes());
    server[30..32].copy_from_slice(&1u16.to_le_bytes());

    let accepted = decode_reply(&packet).unwrap();
    assert_eq!(accepted.account_id, 2_000_000);
    assert_eq!(accepted.sex, 1);
    assert_eq!(accepted.web_token, "0123456789abcdef");
    assert_eq!(accepted.char_servers, vec![CharServer { ip: [127, 0, 0, 1], port: 6121, name: "rAthena".into(), users: 3, kind: 2, new: 1 }]);

    let mut refuse = vec![0u8; 26];
    refuse[..2].copy_from_slice(&AC_REFUSE_LOGIN.to_le_bytes());
    refuse[2] = 1;
    assert!(matches!(decode_reply(&refuse), Err(LoginError::Refused(1))));
}

#[test]
fn fails_a_login_reply_too_short_to_read() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.read_exact(&mut [0u8; 55]).unwrap();
        // AC_ACCEPT_LOGIN that ends after its length.
        let [low, high] = AC_ACCEPT_LOGIN.to_le_bytes();
        stream.write_all(&[low, high, 4, 0]).unwrap();
    });
    assert!(matches!(login(&address, 0, "user", "password"), Err(LoginError::Io(_))));
}

#[test]
fn skips_a_packet_too_short_to_read() {
    let [low, high] = ZC_NOTIFY_CHAT.to_le_bytes();
    assert!(decode_or_skip(&[low, high, 4, 0], decode_map_packet).is_none());
    let chat = [low, high, 11, 0, 7, 0, 0, 0, b'h', b'i', 0];
    assert!(matches!(decode_or_skip(&chat, decode_map_packet), Some(MapEvent::Chat { id: Some(7), .. })));
}

#[test]
fn decodes_character_info() {
    let mut info = vec![0u8; CHARACTER_INFO_SIZE];
    info[..4].copy_from_slice(&150_001u32.to_le_bytes());
    info[50..58].copy_from_slice(&40i64.to_le_bytes());
    info[84..86].copy_from_slice(&4001u16.to_le_bytes());
    info[92..94].copy_from_slice(&99u16.to_le_bytes());
    info[108..114].copy_from_slice(b"Tester");
    info[132..138].copy_from_slice(&[1, 2, 3, 4, 5, 6]);
    info[138] = 1;
    info[142..150].copy_from_slice(b"prontera");
    info[174] = 1;
    let mut packet = vec![0u8; ACCEPT_ENTER_HEADER];
    packet[..2].copy_from_slice(&HC_ACCEPT_ENTER.to_le_bytes());
    packet[4] = 15;
    packet.extend_from_slice(&info);

    let Some(CharEvent::Characters { slots: 15, characters }) = decode_char_packet(&packet) else { panic!() };
    let character = &characters[0];
    assert_eq!((character.id, character.hp, character.job, character.base_level), (150_001, 40, 4001, 99));
    assert_eq!((character.name.as_str(), character.map.as_str()), ("Tester", "prontera"));
    assert_eq!((character.stats, character.slot, character.sex), ([1, 2, 3, 4, 5, 6], 1, 1));
    assert_eq!((character.moves, character.renames, character.name_changed), (0, 0, false));

    // char_mmo_char_tobuf with two moves left and a rename granted.
    info[166..170].copy_from_slice(&2i32.to_le_bytes());
    info[170..174].copy_from_slice(&1i32.to_le_bytes());
    info[140..142].copy_from_slice(&1u16.to_le_bytes());
    let character = decode_character(&info);
    assert_eq!((character.moves, character.renames, character.name_changed), (2, 1, true));

    // A knight on a Peco: effectstate is the option.
    info[36..40].copy_from_slice(&0x20u32.to_le_bytes());
    assert_eq!(decode_character(&info).option, 0x20);
}

#[test]
fn reads_reputation() {
    let mut packet = ZC_REPUTE_INFO.to_le_bytes().to_vec();
    packet.extend(0u16.to_le_bytes());
    packet.push(1);
    for (kind, points) in [(1u64, 700i64), (3, -250)] {
        packet.extend(kind.to_le_bytes());
        packet.extend(points.to_le_bytes());
    }
    let length = packet.len() as u16;
    packet[2..4].copy_from_slice(&length.to_le_bytes());
    assert!(matches!(decode_map_packet(&packet), Some(MapEvent::Reputation(entries)) if entries == [(1, 700), (3, -250)]));
    let mut open = ZC_REPUTE_OPEN.to_le_bytes().to_vec();
    open.extend(2u64.to_le_bytes());
    open.extend(3u32.to_le_bytes());
    open.extend(0u32.to_le_bytes());
    assert_eq!(open.len(), packet_length(ZC_REPUTE_OPEN).unwrap() as usize);
    assert!(matches!(decode_map_packet(&open), Some(MapEvent::ReputationOpen { group: 2, repute: 3 })));
}

#[test]
fn reads_a_navigation() {
    let mut packet = ZC_NAVIGATION_ACTIVE.to_le_bytes().to_vec();
    packet.extend([0, 111, 1]);
    let mut map = *b"prontera.gat\0\0\0\0";
    map[12..].fill(0);
    packet.extend(map);
    for value in [150u16, 160, 0] {
        packet.extend(value.to_le_bytes());
    }
    assert_eq!(packet.len(), packet_length(ZC_NAVIGATION_ACTIVE).unwrap() as usize);
    let event = decode_map_packet(&packet);
    assert!(
        matches!(&event, Some(MapEvent::Navigate { kind: 0, services: 111, hide_window: true, map, cell: [150, 160], monster: 0 }) if map == "prontera.gat"),
        "{event:?}"
    );
}

#[test]
fn writes_a_hotkey_bar_rotation() {
    let packet = encode_hotkey_rotation(1, 3);
    assert_eq!(packet, [0x22, 0x0B, 1, 0, 3]);
    assert_eq!(packet.len(), packet_length(CZ_SHORTCUTKEYBAR_ROTATE2).unwrap() as usize);
}

#[test]
fn writes_a_trait_raise() {
    let packet = encode_trait_raise(219);
    assert_eq!(packet, [0x24, 0x0B, 219, 0, 1, 0]);
    assert_eq!(packet.len(), packet_length(CZ_ADVANCED_STATUS_CHANGE).unwrap() as usize);
}

#[test]
fn writes_the_room_owners_requests() {
    let change = encode_room(CZ_CHANGE_CHATROOM, "QA", 12, false, "pw");
    assert_eq!(&change[..7], [0xDE, 0x00, 17, 0, 12, 0, 0]);
    assert_eq!((&change[7..15], &change[15..]), (&b"pw\0\0\0\0\0\0"[..], &b"QA"[..]));
    let owner = encode_room_owner("LaneA2");
    assert_eq!(owner.len(), packet_length(CZ_REQ_ROLE_CHANGE).unwrap() as usize);
    assert_eq!((&owner[..6], &owner[6..12]), (&[0xE0, 0x00, 0, 0, 0, 0][..], &b"LaneA2"[..]));
    let kick = encode_room_kick("LaneA1");
    assert_eq!(kick.len(), packet_length(CZ_REQ_EXPEL_MEMBER).unwrap() as usize);
    assert_eq!((&kick[..2], &kick[2..8], kick[8]), (&[0xE2, 0x00][..], &b"LaneA1"[..], 0));
}

#[test]
fn reads_a_cancelled_cast() {
    let mut packet = ZC_DISPEL.to_le_bytes().to_vec();
    packet.extend(150_004u32.to_le_bytes());
    assert_eq!(packet.len(), packet_length(ZC_DISPEL).unwrap() as usize);
    assert!(matches!(decode_map_packet(&packet), Some(MapEvent::CastCancelled(150_004))));
}

#[test]
fn decodes_option_changes() {
    // clif_changeoption_target: AID, bodyState, healthState, effectState, PK mode.
    let mut packet = ZC_STATE_CHANGE.to_le_bytes().to_vec();
    packet.extend_from_slice(&2_000_002u32.to_le_bytes());
    packet.extend_from_slice(&1u16.to_le_bytes());
    packet.extend_from_slice(&0x0081u16.to_le_bytes());
    packet.extend_from_slice(&0x80_0020u32.to_le_bytes());
    packet.push(0);
    assert_eq!(packet.len(), packet_length(ZC_STATE_CHANGE).unwrap() as usize);
    let Some(MapEvent::OptionChange { id: 2_000_002, body: 1, health: 0x0081, option: 0x80_0020 }) = decode_map_packet(&packet) else { panic!() };
    // The state ending comes as zeros.
    packet[6..10].fill(0);
    let Some(MapEvent::OptionChange { body: 0, health: 0, .. }) = decode_map_packet(&packet) else { panic!() };
}

#[test]
fn moves_and_renames_characters() {
    assert_eq!(encode_move_slot(1, 2, 2), [0xD4, 0x08, 1, 0, 2, 0, 2, 0]);
    assert_eq!(packet_length(CH_REQ_CHANGE_CHARACTER_SLOT), Some(8));
    let rename = encode_rename(150_002, "RenameTestA");
    assert_eq!((rename.len(), &rename[..6]), (30, &[0xFC, 0x08, 0xF2, 0x49, 0x02, 0x00][..]));
    assert_eq!(c_string(&rename[6..]), "RenameTestA");
    assert_eq!(packet_length(CH_REQ_CHANGE_CHARNAME), Some(30));
    // A 24-letter name keeps its terminator.
    assert_eq!(encode_rename(1, &"x".repeat(30))[29], 0);

    let mut moved = HC_ACK_CHANGE_CHARACTER_SLOT.to_le_bytes().to_vec();
    moved.extend_from_slice(&[8, 0, 0, 0, 1, 0]);
    assert!(matches!(decode_char_packet(&moved), Some(CharEvent::Moved { result: 0, moves: 1 })));
    assert_eq!(packet_length(HC_ACK_CHANGE_CHARACTER_SLOT), Some(-1));
    let mut renamed = HC_ACK_CHANGE_CHARNAME.to_le_bytes().to_vec();
    renamed.extend_from_slice(&4u32.to_le_bytes());
    assert!(matches!(decode_char_packet(&renamed), Some(CharEvent::Renamed(4))));
    assert_eq!(packet_length(HC_ACK_CHANGE_CHARNAME), Some(6));
}

#[test]
fn turns_server_directions_into_sprite_directions() {
    // North shows the back, west and east stay, south faces the camera.
    assert_eq!((0..8).map(sprite_direction).collect::<Vec<_>>(), [4, 3, 2, 1, 0, 7, 6, 5]);
    assert!((0..8).all(|direction| sprite_direction(sprite_direction(direction)) == direction));
    // A standing unit's entry packet carries rathena's direction; the unit faces the sprite's way.
    assert_eq!(decode_entry_position(&encode_position([156, 191], 4)), ([156, 191], 0));
}

#[test]
fn packs_positions_like_rathena() {
    for (position, direction) in [([0, 0], 0), ([156, 191], 4), ([399, 399], 7), ([1023, 1023], 15)] {
        assert_eq!(decode_position(&encode_position(position, direction)), (position, direction));
    }
    // WBUFPOS2 for (156,191) -> (160,185), sub-cell 8/8.
    let (x0, y0, x1, y1) = (156u16, 191u16, 160u16, 185u16);
    let packed = [
        (x0 >> 2) as u8,
        ((x0 << 6) | ((y0 >> 4) & 0x3F)) as u8,
        ((y0 << 4) | ((x1 >> 6) & 0x0F)) as u8,
        ((x1 << 2) | ((y1 >> 8) & 0x03)) as u8,
        y1 as u8,
        0x88,
    ];
    assert_eq!(decode_move(&packed), ([156, 191], [160, 185]));
}

#[test]
fn knows_packet_lengths() {
    assert_eq!(packet_length(CZ_ENTER), Some(23));
    assert_eq!(packet_length(ZC_ACCEPT_ENTER), Some(13));
    assert_eq!(packet_length(ZC_NOTIFY_STANDENTRY), Some(-1));
    assert_eq!(packet_length(HC_NOTIFY_ZONESVR), Some(156));
    assert_eq!(packet_length(0x09A0), Some(6));
    // Written by hand in rathena (clif_navigateTo, the sale tool): the tutorial maps' navigation used to drop us.
    assert_eq!([0x08E2, 0x09B5, 0x09BD].map(packet_length), [Some(27), Some(2), Some(2)]);
}

#[test]
fn decodes_guild_pages() {
    // A raise's answer: the new value when taken, nothing when refused.
    let Some(MapEvent::RaiseAnswer(Some((13, 5)))) = decode_map_packet(&[0xBC, 0x00, 13, 0, 1, 5]) else { panic!() };
    let Some(MapEvent::RaiseAnswer(None)) = decode_map_packet(&[0xBC, 0x00, 13, 0, 0, 4]) else { panic!() };
    let packet = |id: u16, body: &[u8]| {
        let mut packet = id.to_le_bytes().to_vec();
        packet.extend_from_slice(&((4 + body.len()) as u16).to_le_bytes());
        packet.extend_from_slice(body);
        packet
    };
    let mut position = [0u8; 16];
    position[..4].copy_from_slice(&19u32.to_le_bytes());
    position[4..8].copy_from_slice(&0x111u32.to_le_bytes());
    position[8..12].copy_from_slice(&7u32.to_le_bytes());
    position[12..16].copy_from_slice(&50u32.to_le_bytes());
    let Some(MapEvent::GuildPositionInfo(positions)) = decode_map_packet(&packet(ZC_POSITION_INFO, &position)) else { panic!() };
    assert_eq!(positions, [GuildPosition { id: 19, mode: 0x111, rank: 7, tax: 50 }], "the ranking at 8");

    let mut skills = 3u16.to_le_bytes().to_vec();
    let mut skill = [0u8; SKILL_SIZE];
    skill[..2].copy_from_slice(&10004u16.to_le_bytes());
    skill[6..8].copy_from_slice(&2u16.to_le_bytes());
    skill[36] = 1;
    skills.extend_from_slice(&skill);
    let Some(MapEvent::GuildSkills { points: 3, skills }) = decode_map_packet(&packet(ZC_GUILD_SKILLINFO, &skills)) else { panic!() };
    assert_eq!((skills[0].id, skills[0].level, skills[0].upgradable), (10004, 2, true));

    let mut banned = [0u8; 68];
    banned[4..11].copy_from_slice(b"spammer");
    banned[44..49].copy_from_slice(b"Ghost");
    let Some(MapEvent::GuildExpelled(list)) = decode_map_packet(&packet(ZC_BAN_LIST, &banned)) else { panic!() };
    assert_eq!(list, [("Ghost".to_owned(), "spammer".to_owned())]);

    // Players carry their guild and emblem version after the robe.
    let mut stand = vec![0u8; 108];
    stand[..2].copy_from_slice(&ZC_NOTIFY_STANDENTRY.to_le_bytes());
    stand[15..17].copy_from_slice(&2u16.to_le_bytes());
    stand[17..19].copy_from_slice(&2u16.to_le_bytes());
    stand[49..53].copy_from_slice(&7u32.to_le_bytes());
    stand[53..55].copy_from_slice(&3u16.to_le_bytes());
    let Some(MapEvent::Unit(unit)) = decode_map_packet(&stand) else { panic!() };
    assert_eq!((unit.guild, unit.emblem), (7, 3));
    assert_eq!((unit.body_state, unit.health_state), (2, 2));
    assert_eq!(unit.hp, None);
    // Its state byte; a stand entry isn't a spawn, a new entry is and has no state byte.
    stand[68] = 1;
    let Some(MapEvent::Unit(unit)) = decode_map_packet(&stand) else { panic!() };
    assert_eq!((unit.posture, unit.spawned), (Some(1), false));
    let mut new = stand.clone();
    new[..2].copy_from_slice(&ZC_NOTIFY_NEWENTRY.to_le_bytes());
    let Some(MapEvent::Unit(unit)) = decode_map_packet(&new) else { panic!() };
    assert_eq!((unit.posture, unit.spawned), (None, true));

    // A hurt monster comes with its HP after the level and font; -1 when not told.
    stand[73..77].copy_from_slice(&50u32.to_le_bytes());
    stand[77..81].copy_from_slice(&20u32.to_le_bytes());
    let Some(MapEvent::Unit(unit)) = decode_map_packet(&stand) else { panic!() };
    assert_eq!(unit.hp, Some([20, 50]));
    stand[73..81].fill(0xFF);
    let Some(MapEvent::Unit(unit)) = decode_map_packet(&stand) else { panic!() };
    assert_eq!(unit.hp, None);
}

#[test]
fn decodes_npc_dialog_and_attacks() {
    let mut say = vec![0u8; 8];
    say[..2].copy_from_slice(&ZC_SAY_DIALOG.to_le_bytes());
    say[4..8].copy_from_slice(&110_012_460u32.to_le_bytes());
    say.extend_from_slice(b"[Guide]\0");
    let length = say.len() as u16;
    say[2..4].copy_from_slice(&length.to_le_bytes());
    assert!(matches!(decode_map_packet(&say), Some(MapEvent::NpcSay { npc: 110_012_460, text }) if text == "[Guide]"));

    let mut menu = vec![0u8; 8];
    menu[..2].copy_from_slice(&ZC_MENU_LIST.to_le_bytes());
    menu.extend_from_slice(b"Yes:No:\0");
    let Some(MapEvent::NpcMenu { items, .. }) = decode_map_packet(&menu) else { panic!() };
    assert_eq!(items, ["Yes", "No", ""]);

    let mut act = vec![0u8; 34];
    act[..2].copy_from_slice(&ZC_NOTIFY_ACT.to_le_bytes());
    act[2..6].copy_from_slice(&2_000_000u32.to_le_bytes());
    act[6..10].copy_from_slice(&110_000_001u32.to_le_bytes());
    act[14..18].copy_from_slice(&1_288u32.to_le_bytes());
    act[22..26].copy_from_slice(&42u32.to_le_bytes());
    act[27..29].copy_from_slice(&1u16.to_le_bytes());
    act[29] = 10;
    let Some(MapEvent::Act(act)) = decode_map_packet(&act) else { panic!() };
    assert_eq!((act.source, act.target, act.source_speed, act.damage, act.hits, act.kind), (2_000_000, 110_000_001, 1_288, 42, 1, 10));
    assert_eq!(packet_length(ZC_NOTIFY_ACT), Some(34));
    assert_eq!(packet_length(ZC_HP_INFO), Some(14));
}

#[test]
fn decodes_progress_bars() {
    // progressbar "ffff00", 3: the colour, then the seconds.
    let bar = [0xf0, 0x02, 0, 0xff, 0xff, 0, 3, 0, 0, 0];
    assert!(matches!(decode_map_packet(&bar), Some(MapEvent::Progress { seconds: 3 })));
    let mut actor = vec![0xd1, 0x09];
    actor.extend_from_slice(&110_000_007u32.to_le_bytes());
    actor.extend_from_slice(&0x00ff_ff00u32.to_le_bytes());
    actor.extend_from_slice(&5u32.to_le_bytes());
    assert!(matches!(decode_map_packet(&actor), Some(MapEvent::ProgressActor { unit: 110_000_007, seconds: 5 })));
    assert!(matches!(decode_map_packet(&[0xf2, 0x02]), Some(MapEvent::ProgressCancel)));
    assert_eq!([ZC_PROGRESS, ZC_PROGRESS_ACTOR, ZC_PROGRESS_CANCEL].map(packet_length), [Some(10), Some(14), Some(2)]);
}

#[test]
fn decodes_skill_units() {
    // Captured from rathena: a Fire Wall cell (UNT_FIREWALL) at 156,190, and its going.
    let entry = [0xca, 0x09, 0x17, 0, 0x1a, 0x0d, 0, 0, 0x82, 0x84, 0x1e, 0, 0x9c, 0, 0xbe, 0, 0x7f, 0, 0, 0, 0, 0x01, 0x0a];
    assert!(matches!(decode_map_packet(&entry), Some(MapEvent::SkillUnit { id: 3354, kind: 127, cell: [156, 190] })));
    assert!(matches!(decode_map_packet(&[0x20, 0x01, 0x1a, 0x0d, 0, 0]), Some(MapEvent::SkillUnitGone { id: 3354 })));
    assert_eq!((packet_length(ZC_SKILL_ENTRY), packet_length(ZC_SKILL_DISAPPEAR)), (Some(23), Some(6)));
}

#[test]
fn decodes_ground_items_and_inventory() {
    // Captured from rathena: a Poring dropping a Jellopy (909) at 252,88.
    let jellopy = [0xdd, 0x0a, 0x60, 0x04, 0, 0, 0x8d, 0x03, 0, 0, 0x03, 0, 0x01, 0xfc, 0, 0x58, 0, 0x06, 0x0c, 0x01, 0, 0, 0, 0];
    let Some(MapEvent::GroundItem(item)) = decode_map_packet(&jellopy) else { panic!() };
    assert_eq!(item, GroundItem { id: 0x460, item: 909, identified: true, cell: [252, 88], sub: [6, 12], amount: 1, falling: true });
    assert_eq!(packet_length(ZC_ITEM_FALL_ENTRY), Some(jellopy.len() as i32));

    // A weapon card (4035) comes with its place, the weapon (2), where a worn item's would be.
    let mut list = vec![0u8; 5 + 3 * NORMAL_ITEM_SIZE];
    list[..2].copy_from_slice(&ZC_INVENTORY_ITEMLIST_NORMAL.to_le_bytes());
    for (slot, (index, id, amount, places)) in [(2u16, 501u32, 7u16, 0u32), (3, 909, 12, 0), (4, 4035, 1, 2)].into_iter().enumerate() {
        let entry = &mut list[5 + slot * NORMAL_ITEM_SIZE..][..NORMAL_ITEM_SIZE];
        entry[..2].copy_from_slice(&index.to_le_bytes());
        entry[2..6].copy_from_slice(&id.to_le_bytes());
        entry[7..9].copy_from_slice(&amount.to_le_bytes());
        entry[9..13].copy_from_slice(&places.to_le_bytes());
        entry[33] = 1;
    }
    let Some(MapEvent::InventoryItems { kind: 0, items }) = decode_map_packet(&list) else { panic!() };
    assert_eq!(
        items.iter().map(|item| (item.index, item.id, item.amount, item.identified)).collect::<Vec<_>>(),
        [(2, 501, 7, true), (3, 909, 12, true), (4, 4035, 1, true)]
    );
    assert_eq!((items[2].worn, items[2].location), (0, 2));

    let mut pickup = vec![0u8; 70];
    pickup[..2].copy_from_slice(&ZC_ITEM_PICKUP_ACK.to_le_bytes());
    pickup[2..4].copy_from_slice(&4u16.to_le_bytes());
    pickup[4..6].copy_from_slice(&1u16.to_le_bytes());
    pickup[6..10].copy_from_slice(&1201u32.to_le_bytes());
    pickup[10] = 1;
    pickup[32] = ITEM_WEAPON;
    pickup[68] = 4;
    let Some(MapEvent::PickedUp { result: 0, item }) = decode_map_packet(&pickup) else { panic!() };
    assert_eq!((item.index, item.id, item.amount, item.kind, item.refine), (4, 1201, 1, ITEM_WEAPON, 4));
    assert_eq!(packet_length(ZC_ITEM_PICKUP_ACK), Some(70));
    assert_eq!(packet_length(ZC_USE_ITEM_ACK), Some(15));
    assert_eq!(packet_length(ZC_SPRITE_CHANGE), Some(15));
    // ZC_NPCSPRITE_CHANGE: the unit, a byte, the new class.
    let mut npc = vec![0xb0, 0x01];
    npc.extend_from_slice(&110_000_009u32.to_le_bytes());
    npc.push(0);
    npc.extend_from_slice(&6009u32.to_le_bytes());
    assert!(matches!(decode_map_packet(&npc), Some(MapEvent::JobSprite { id: 110_000_009, job: 6009 })));
    assert_eq!(packet_length(ZC_NPCSPRITE_CHANGE), Some(11));
    assert_eq!(packet_length(ZC_REQ_WEAR_EQUIP_ACK), Some(11));
    assert_eq!(packet_length(ZC_REQ_TAKEOFF_EQUIP_ACK), Some(9));
}

#[test]
fn decodes_quests() {
    let mut list = ZC_ALL_QUEST_LIST3.to_le_bytes().to_vec();
    list.extend_from_slice(&[0, 0, 2, 0, 0, 0]);
    // Two quests, the first with two hunts (one with a level range): 44 bytes each, as rathena writes them.
    let hunt = |id: u32, count: u16, needed: u16, levels: [u16; 2], name: &[u8]| {
        let mut hunt = [0u8; 44];
        hunt[..4].copy_from_slice(&id.to_le_bytes());
        hunt[12..14].copy_from_slice(&levels[0].to_le_bytes());
        hunt[14..16].copy_from_slice(&levels[1].to_le_bytes());
        hunt[16..18].copy_from_slice(&count.to_le_bytes());
        hunt[18..20].copy_from_slice(&needed.to_le_bytes());
        hunt[20..20 + name.len()].copy_from_slice(name);
        hunt
    };
    for (id, hunts) in [(7001u32, vec![hunt(7_001_000, 2, 10, [0, 0], b"Poring"), hunt(7_001_001, 0, 5, [10, 20], b"Lunatic")]), (7002, vec![])] {
        list.extend_from_slice(&id.to_le_bytes());
        list.push(1);
        list.extend_from_slice(&[0; 8]);
        list.extend_from_slice(&(hunts.len() as u16).to_le_bytes());
        for hunt in hunts {
            list.extend_from_slice(&hunt);
        }
    }
    let length = list.len() as u16;
    list[2..4].copy_from_slice(&length.to_le_bytes());
    let Some(MapEvent::Quests(quests)) = decode_map_packet(&list) else { panic!() };
    let poring = Hunt { id: 7_001_000, name: "Poring".into(), count: 2, needed: 10, levels: [0, 0] };
    let lunatic = Hunt { id: 7_001_001, name: "Lunatic".into(), count: 0, needed: 5, levels: [10, 20] };
    assert_eq!(quests, [Quest { id: 7001, active: true, end: 0, hunts: vec![poring, lunatic] }, Quest { id: 7002, active: true, end: 0, hunts: vec![] }]);

    let mut update = ZC_UPDATE_MISSION_HUNT_EX.to_le_bytes().to_vec();
    update.extend_from_slice(&[18, 0, 1, 0]);
    for value in [7001u32, 7_001_000] {
        update.extend_from_slice(&value.to_le_bytes());
    }
    update.extend_from_slice(&10u16.to_le_bytes());
    update.extend_from_slice(&3u16.to_le_bytes());
    let Some(MapEvent::QuestProgress(hunts)) = decode_map_packet(&update) else { panic!() };
    assert_eq!(hunts, [(7_001_000, 3, 10)]);
}

#[test]
fn decodes_announcements_and_messages() {
    let mut blue = ZC_BROADCAST.to_le_bytes().to_vec();
    blue.extend_from_slice(&[0, 0]);
    blue.extend_from_slice(b"blueHello all\0");
    let length = blue.len() as u16;
    blue[2..4].copy_from_slice(&length.to_le_bytes());
    let Some(MapEvent::Broadcast { text, color: BROADCAST_BLUE, kind: BroadcastKind::Normal, size: None }) = decode_map_packet(&blue) else { panic!() };
    assert_eq!(text, "Hello all");

    // Every prefix, in either case.
    for (message, want, color, kind) in [
        (&b"Hello\0"[..], "Hello", BROADCAST_YELLOW, BroadcastKind::Normal),
        (b"BLUEShout\0", "Shout", BROADCAST_BLUE, BroadcastKind::Normal),
        (b"ssssThe War of Emperium has begun.\0", "The War of Emperium has begun.", BROADCAST_YELLOW, BroadcastKind::Woe),
        (b"toolFF8800Orange\0", "Orange", [0xFF, 0x88, 0x00], BroadcastKind::Normal),
        (b"TOOLzzzzzzBad hex\0", "Bad hex", BROADCAST_YELLOW, BroadcastKind::Normal),
        // A name's #tag up to the first colon goes.
        (b"GM#1234 : Server restart\0", "GM: Server restart", BROADCAST_YELLOW, BroadcastKind::Normal),
        (b"#notag at the start\0", "#notag at the start", BROADCAST_YELLOW, BroadcastKind::Normal),
    ] {
        assert_eq!(decode_broadcast(message), (want.to_owned(), color, kind), "{}", String::from_utf8_lossy(message));
    }
    let mut speaker = b"micc".to_vec();
    speaker.extend_from_slice(&name_field("Loud"));
    speaker.extend_from_slice(b"00FF00Buy potions\0");
    assert_eq!(decode_broadcast(&speaker), ("Buy potions".to_owned(), [0, 0xFF, 0], BroadcastKind::Speaker("Loud".into())));

    let mut colored = ZC_BROADCAST2.to_le_bytes().to_vec();
    colored.extend_from_slice(&[0, 0]);
    colored.extend_from_slice(&0x00FF_8000u32.to_le_bytes());
    colored.extend_from_slice(&[0; 8]);
    colored.extend_from_slice(b"Event!\0");
    let length = colored.len() as u16;
    colored[2..4].copy_from_slice(&length.to_le_bytes());
    colored[10..12].copy_from_slice(&16u16.to_le_bytes());
    let Some(MapEvent::Broadcast { text, color, size, .. }) = decode_map_packet(&colored) else { panic!() };
    assert_eq!((text.as_str(), color, size), ("Event!", [0xFF, 0x80, 0x00], Some(16)));

    let mut value = ZC_MSG_VALUE.to_le_bytes().to_vec();
    value.extend_from_slice(&178u16.to_le_bytes());
    value.extend_from_slice(&42i32.to_le_bytes());
    let Some(MapEvent::Message { id: 178, value: Some(42), skill: None, color: None }) = decode_map_packet(&value) else { panic!() };

    // rathena's red, 0xFF0000 turned to 0x0000FF.
    let mut red = ZC_MSG_COLOR.to_le_bytes().to_vec();
    red.extend_from_slice(&1923u16.to_le_bytes());
    red.extend_from_slice(&0x0000_00FFu32.to_le_bytes());
    let Some(MapEvent::Message { id: 1923, color: Some([0xFF, 0, 0]), .. }) = decode_map_packet(&red) else { panic!() };

    let mut talk = ZC_NPC_CHAT.to_le_bytes().to_vec();
    talk.extend_from_slice(&[0, 0]);
    talk.extend_from_slice(&110_000_007u32.to_le_bytes());
    talk.extend_from_slice(&0x0000_FF00u32.to_le_bytes());
    talk.extend_from_slice(b"Grr\0");
    let length = talk.len() as u16;
    talk[2..4].copy_from_slice(&length.to_le_bytes());
    let Some(MapEvent::Chat { id: Some(110_000_007), message, color: Some([0, 0xFF, 0]) }) = decode_map_packet(&talk) else { panic!() };
    assert_eq!(message, "Grr");
    for (id, length) in [(ZC_BROADCAST, -1), (ZC_BROADCAST2, -1), (ZC_MSG, 4), (ZC_MSG_VALUE, 8), (ZC_MSG_SKILL, 8), (ZC_MSG_COLOR, 8), (ZC_NPC_CHAT, -1)] {
        assert_eq!(packet_length(id), Some(length), "{id:#06x}");
    }
    assert_eq!([ZC_ACTION_FAILURE, ZC_ACK_REMEMBER_WARPPOINT, ZC_NOTIFY_MAPINFO].map(packet_length), [Some(4), Some(3), Some(4)]);
}

#[test]
fn decodes_item_announcements() {
    let head = |kind: u8, item: u32| {
        let mut packet = ZC_BROADCASTING_SPECIAL_ITEM_OBTAIN.to_le_bytes().to_vec();
        packet.extend_from_slice(&[0, 0, kind]);
        packet.extend_from_slice(&item.to_le_bytes());
        packet.push(24);
        let mut name = b"Pori**".to_vec();
        name.resize(24, 0);
        packet.extend_from_slice(&name);
        packet
    };
    // Out of a box: the box's item ID after its length.
    let mut boxed = head(0, 4407);
    boxed.push(4);
    boxed.extend_from_slice(&22829u32.to_le_bytes());
    assert_eq!(boxed.len(), 39);
    let Some(MapEvent::ItemObtained { item: 4407, player, box_item: Some(22829), monster: None }) = decode_map_packet(&boxed) else { panic!() };
    assert_eq!(player, "Pori**");
    // From a monster: its name; rathena's length counts one byte more than it sends.
    let mut dropped = head(1, 4407);
    dropped.push(24);
    let mut monster = b"Poring".to_vec();
    monster.resize(25, 0);
    dropped.extend_from_slice(&monster);
    assert_eq!(dropped.len(), 60);
    let Some(MapEvent::ItemObtained { box_item: None, monster: Some(monster), .. }) = decode_map_packet(&dropped) else { panic!() };
    assert_eq!(monster, "Poring");
    let mut npc = head(2, 4407);
    npc.extend_from_slice(&[0, 0]);
    assert_eq!(npc.len(), 36);
    let Some(MapEvent::ItemObtained { box_item: None, monster: None, .. }) = decode_map_packet(&npc) else { panic!() };

    let mut refine = ZC_BROADCAST_ITEMREFINING_RESULT.to_le_bytes().to_vec();
    let mut name = b"Smi**".to_vec();
    name.resize(24, 0);
    refine.extend_from_slice(&name);
    refine.extend_from_slice(&1201u32.to_le_bytes());
    refine.extend_from_slice(&[13, 1]);
    assert_eq!(Some(refine.len() as i32), packet_length(ZC_BROADCAST_ITEMREFINING_RESULT));
    let Some(MapEvent::RefineAnnounced { player, item: 1201, level: 13, success: true }) = decode_map_packet(&refine) else { panic!() };
    assert_eq!(player, "Smi**");
}

#[test]
fn decodes_mvp() {
    let packet = |id: u16, value: u32| [id.to_le_bytes().as_slice(), &value.to_le_bytes()].concat();
    let Some(MapEvent::Mvp(2_000_002)) = decode_map_packet(&packet(ZC_MVP, 2_000_002)) else { panic!() };
    let Some(MapEvent::MvpItem(6010)) = decode_map_packet(&packet(ZC_MVP_GETTING_ITEM, 6010)) else { panic!() };
    let Some(MapEvent::MvpExp(90_000)) = decode_map_packet(&packet(ZC_MVP_GETTING_SPECIAL_EXP, 90_000)) else { panic!() };
    let Some(MapEvent::MvpOverweight) = decode_map_packet(&ZC_THROW_MVPITEM.to_le_bytes()) else { panic!() };
    assert!(matches!(decode_map_packet(&[0xEB, 0x01, 1, 0, 0, 0, 156, 0, 180, 0]), Some(MapEvent::GuildMemberPlace { account: 1, cell: Some([156, 180]) })));
    assert!(matches!(decode_map_packet(&[0xEB, 0x01, 1, 0, 0, 0, 0xFF, 0xFF, 0xFF, 0xFF]), Some(MapEvent::GuildMemberPlace { cell: None, .. })));
    assert!(matches!(decode_map_packet(&[0xDE, 0x0A, 50, 0, 0, 0]), Some(MapEvent::OverweightPercent(50))));
    assert_eq!([ZC_MVP, ZC_MVP_GETTING_ITEM, ZC_MVP_GETTING_SPECIAL_EXP, ZC_THROW_MVPITEM].map(packet_length), [Some(6), Some(6), Some(6), Some(2)]);
}

#[test]
fn turns() {
    assert_eq!(encode_direction(6, 0), [0x61, 0x03, 0, 0, 6]);
    assert_eq!(packet_length(CZ_CHANGE_DIRECTION), Some(5));
    assert_eq!((CZ_CANCEL_LOCKON.to_le_bytes(), packet_length(CZ_CANCEL_LOCKON)), ([0x18, 0x01], Some(2)));
}

#[test]
fn moves_favourites_and_identifies() {
    assert_eq!(encode_favorite(7, false), [0x07, 0x09, 7, 0, 0]);
    assert_eq!(encode_favorite(7, true)[4], 1);
    let Some(MapEvent::FavoriteMoved { index: 7, favorite: true }) = decode_map_packet(&[0x08, 0x09, 7, 0, 0]) else { panic!() };
    let Some(MapEvent::FavoriteMoved { favorite: false, .. }) = decode_map_packet(&[0x08, 0x09, 7, 0, 1]) else { panic!() };
    assert_eq!(encode_index(CZ_REQ_ONECLICK_ITEMIDENTIFY, 7), [0x35, 0x0A, 7, 0]);
    assert_eq!([CZ_INVENTORY_TAB, ZC_INVENTORY_TAB, CZ_REQ_ONECLICK_ITEMIDENTIFY].map(packet_length), [Some(5), Some(5), Some(4)]);
    // The favourite flag: bit 2 of an equipment's flags, bit 1 of another item's.
    let mut entry = vec![0u8; EQUIP_ITEM_SIZE];
    entry[67] = 5;
    assert!(decode_equip_item(&entry).favorite);
    let mut normal = vec![0u8; NORMAL_ITEM_SIZE];
    normal[33] = 3;
    assert!(decode_normal_item(&normal).favorite && decode_normal_item(&normal).identified);
}

#[test]
fn decodes_spirits() {
    let mut packet = ZC_SPIRITS.to_le_bytes().to_vec();
    packet.extend_from_slice(&2_000_002u32.to_le_bytes());
    packet.extend_from_slice(&5u16.to_le_bytes());
    assert_eq!(Some(packet.len() as i32), packet_length(ZC_SPIRITS));
    let Some(MapEvent::Spirits { id: 2_000_002, count: 5 }) = decode_map_packet(&packet) else { panic!() };
    let shields = [&ZC_MILLENNIUMSHIELD.to_le_bytes()[..], &2_000_002u32.to_le_bytes(), &3u16.to_le_bytes(), &0u16.to_le_bytes()].concat();
    let Some(MapEvent::Shields { id: 2_000_002, count: 3 }) = decode_map_packet(&shields) else { panic!() };
    let charms =
        |kind: u16, count: u16| [&ZC_SPIRITS_ATTRIBUTE.to_le_bytes()[..], &2_000_002u32.to_le_bytes(), &kind.to_le_bytes(), &count.to_le_bytes()].concat();
    let Some(MapEvent::Charms { kind: 3, count: 2, .. }) = decode_map_packet(&charms(3, 2)) else { panic!() };
    let Some(MapEvent::Charms { kind: 0, count: 0, .. }) = decode_map_packet(&charms(0, 0)) else { panic!() };
    let souls = [&ZC_SOULENERGY.to_le_bytes()[..], &2_000_002u32.to_le_bytes(), &5u16.to_le_bytes()].concat();
    let Some(MapEvent::Souls { id: 2_000_002, count: 5 }) = decode_map_packet(&souls) else { panic!() };
    assert_eq!([ZC_MILLENNIUMSHIELD, ZC_SPIRITS_ATTRIBUTE, ZC_SOULENERGY].map(packet_length), [Some(10), Some(10), Some(8)]);
}

#[test]
fn speaks_to_the_bank() {
    assert_eq!(encode_bank_move(true, 2_000_002, 1000)[..2], CZ_REQ_BANKING_DEPOSIT.to_le_bytes());
    assert_eq!(encode_bank_move(false, 2_000_002, 1000), [0xA9, 0x09, 0x82, 0x84, 0x1E, 0x00, 0xE8, 0x03, 0, 0]);
    let Some(MapEvent::BankOpened) = decode_map_packet(&[0xB7, 0x09, 0, 0]) else { panic!() };
    let mut check = ZC_BANKING_CHECK.to_le_bytes().to_vec();
    check.extend_from_slice(&5_000_000_000i64.to_le_bytes());
    check.extend_from_slice(&[0, 0]);
    let Some(MapEvent::BankBalance(5_000_000_000)) = decode_map_packet(&check) else { panic!() };
    let mut moved = ZC_ACK_BANKING_WITHDRAW.to_le_bytes().to_vec();
    moved.extend_from_slice(&1u16.to_le_bytes());
    moved.extend_from_slice(&1000i64.to_le_bytes());
    moved.extend_from_slice(&4000i32.to_le_bytes());
    let Some(MapEvent::BankMoved { deposit: false, reason: 1, balance: 1000, zeny: 4000 }) = decode_map_packet(&moved) else { panic!() };
    let ids = [CZ_REQ_OPEN_BANKING, ZC_ACK_OPEN_BANKING, CZ_REQ_BANKING_CHECK, ZC_BANKING_CHECK, CZ_REQ_BANKING_DEPOSIT, ZC_ACK_BANKING_DEPOSIT];
    assert_eq!(ids.map(packet_length), [Some(6), Some(4), Some(6), Some(12), Some(10), Some(16)]);
    assert_eq!([CZ_REQ_BANKING_WITHDRAW, ZC_ACK_BANKING_WITHDRAW, CZ_REQ_CLOSE_BANKING].map(packet_length), [Some(10), Some(16), Some(6)]);
}

#[test]
fn decodes_cash_shop() {
    assert_eq!(encode_cashshop_open(0), [0x6D, 0x0B, 0, 0, 0, 0]);
    assert_eq!(&encode_cashshop_open(3)[2..6], &3u32.to_le_bytes());
    assert_eq!(encode_cashshop_list_request(), [0xC9, 0x08]);

    let buy = encode_cashshop_buy(25, &[(501, 2, 3), (1201, 99, 8)]);
    assert_eq!(buy, [0x48, 0x08, 30, 0, 2, 0, 25, 0, 0, 0, 0xF5, 0x01, 0, 0, 2, 0, 0, 0, 3, 0, 0xB1, 0x04, 0, 0, 99, 0, 0, 0, 8, 0,]);
    assert_eq!(encode_cashshop_buy(0, &[]), [0x48, 0x08, 10, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(encode_cashshop_close(), [0x4A, 0x08]);

    let open = [&ZC_SE_CASHSHOP_OPEN.to_le_bytes()[..], &1000u32.to_le_bytes(), &75u32.to_le_bytes(), &3u32.to_le_bytes()].concat();
    let Some(MapEvent::CashShopOpen { cash: 1000, free: 75, tab: 3 }) = decode_map_packet(&open) else { panic!() };

    let mut list = ZC_ACK_SCHEDULER_CASHITEM.to_le_bytes().to_vec();
    list.extend_from_slice(&24u16.to_le_bytes());
    list.extend_from_slice(&2i16.to_le_bytes());
    list.extend_from_slice(&4i16.to_le_bytes());
    list.extend_from_slice(&501u32.to_le_bytes());
    list.extend_from_slice(&10u32.to_le_bytes());
    list.extend_from_slice(&1201u32.to_le_bytes());
    list.extend_from_slice(&250u32.to_le_bytes());
    let Some(MapEvent::CashShopList { tab: 4, items }) = decode_map_packet(&list) else { panic!() };
    assert_eq!(items, [(501, 10), (1201, 250)]);
    assert!(decode_map_packet(&list[..list.len() - 1]).is_none());

    let bought =
        [&ZC_SE_PC_BUY_CASHITEM_RESULT.to_le_bytes()[..], &501u32.to_le_bytes(), &0u16.to_le_bytes(), &990u32.to_le_bytes(), &65u32.to_le_bytes()].concat();
    let Some(MapEvent::CashShopBuyResult { item: 501, result: 0, cash: 990, free: 65 }) = decode_map_packet(&bought) else { panic!() };

    let ids = [
        CZ_SE_CASHSHOP_OPEN2,
        ZC_SE_CASHSHOP_OPEN,
        CZ_CASHSHOP_LIST,
        ZC_ACK_SCHEDULER_CASHITEM,
        CZ_SE_PC_BUY_CASHITEM_LIST,
        ZC_SE_PC_BUY_CASHITEM_RESULT,
        CZ_CASHSHOP_CLOSE,
    ];
    assert_eq!(ids.map(packet_length), [Some(6), Some(14), Some(2), Some(-1), Some(-1), Some(16), Some(2)]);
}

#[test]
fn decodes_others_equipment() {
    let mut packet = ZC_EQUIPWIN_MICROSCOPE.to_le_bytes().to_vec();
    packet.extend_from_slice(&[0, 0]);
    let mut name = b"McpTester2".to_vec();
    name.resize(24, 0);
    packet.extend_from_slice(&name);
    for look in [4u16, 13, 0, 0, 5, 0, 2, 1, 0] {
        packet.extend_from_slice(&look.to_le_bytes());
    }
    packet.push(1);
    // A Knife worn in the right hand (2, look 1), a Cotton Shirt on the body (16).
    for (index, item, worn, look) in [(4u16, 1201u32, 2u32, 1u16), (5, 2301, 16, 0)] {
        let mut entry = vec![0u8; EQUIP_ITEM_SIZE];
        entry[..2].copy_from_slice(&index.to_le_bytes());
        entry[2..6].copy_from_slice(&item.to_le_bytes());
        entry[11..15].copy_from_slice(&worn.to_le_bytes());
        entry[37..39].copy_from_slice(&look.to_le_bytes());
        entry[67] = 1;
        packet.extend_from_slice(&entry);
    }
    let length = packet.len() as u16;
    packet[2..4].copy_from_slice(&length.to_le_bytes());
    let Some(MapEvent::OtherEquipment(equipped)) = decode_map_packet(&packet) else { panic!() };
    assert_eq!((equipped.name.as_str(), equipped.job, equipped.hair, equipped.head_top, equipped.hair_color, equipped.sex), ("McpTester2", 4, 13, 5, 2, 1));
    let worn: Vec<_> = equipped.items.iter().map(|(item, look)| (item.id, item.worn, *look)).collect();
    assert_eq!(worn, [(1201, 2, 1), (2301, 16, 0)]);

    assert_eq!(encode_config(CONFIG_OPEN_EQUIPMENT, true), [0xD8, 0x02, 0, 0, 0, 0, 1, 0, 0, 0]);
    // ZC_INVENTORY_START: the storage's name from byte 5, up to its NUL; an empty one.
    let start = |name: &[u8]| decode_map_packet(&[&[0x08, 0x0B, 6 + name.len() as u8, 0, 2], name].concat());
    let Some(MapEvent::InventoryStart { kind: 2, name }) = start(b"Guild Storage\0") else { panic!() };
    assert_eq!(name, "Guild Storage");
    let Some(MapEvent::InventoryStart { kind: 2, name }) = start(b"\0") else { panic!() };
    assert!(name.is_empty());
    // The View Costume box: {5, 0x3C00} hides costumes, {5, 0} shows them; the answer's non-zero means hidden.
    assert_eq!(encode_config_value(CONFIG_HIDE_COSTUMES, HIDE_COSTUMES), [0xD8, 0x02, 5, 0, 0, 0, 0x00, 0x3C, 0, 0]);
    assert_eq!(encode_config_value(CONFIG_HIDE_COSTUMES, 0), [0xD8, 0x02, 5, 0, 0, 0, 0, 0, 0, 0]);
    let Some(MapEvent::CostumesShown(false)) = decode_map_packet(&[0xD9, 0x02, 5, 0, 0, 0, 1, 0, 0, 0]) else { panic!() };
    assert_eq!(encode_uninstall(6), [0x35, 0x0B, 6]);
    let Some(MapEvent::ShowEquipment { shown: true, answer: true }) = decode_map_packet(&[0xD9, 0x02, 0, 0, 0, 0, 1, 0, 0, 0]) else { panic!() };
    assert!(decode_map_packet(&[0xD9, 0x02, 1, 0, 0, 0, 1, 0, 0, 0]).is_none());
    let Some(MapEvent::ShowEquipment { shown: false, answer: false }) = decode_map_packet(&[0xDA, 0x02, 0]) else { panic!() };
    let ids = [CZ_EQUIPWIN_MICROSCOPE, ZC_EQUIPWIN_MICROSCOPE, CZ_CONFIG, ZC_CONFIG, ZC_CONFIG_NOTIFY];
    assert_eq!(ids.map(packet_length), [Some(6), Some(-1), Some(10), Some(10), Some(3)]);
}

#[test]
fn decodes_inventory_expansion() {
    let mut asked = ZC_ACK_OPEN_MSGBOX_EXTEND_BODYITEM_SIZE.to_le_bytes().to_vec();
    asked.push(0);
    asked.extend_from_slice(&25793u32.to_le_bytes());
    let Some(MapEvent::ExpansionAsked { result: 0, voucher: 25793 }) = decode_map_packet(&asked) else { panic!() };
    let Some(MapEvent::ExpansionDone(4)) = decode_map_packet(&[0x17, 0x0B, 4]) else { panic!() };
    let ids = [CZ_REQ_OPEN_MSGBOX_EXTEND_BODYITEM_SIZE, ZC_ACK_OPEN_MSGBOX_EXTEND_BODYITEM_SIZE, CZ_REQ_EXTEND_BODYITEM_SIZE, ZC_ACK_EXTEND_BODYITEM_SIZE];
    assert_eq!(ids.map(packet_length), [Some(2), Some(7), Some(2), Some(3)]);
    assert_eq!(packet_length(CZ_CLOSE_MSGBOX_EXTEND_BODYITEM_SIZE), Some(2));
}

#[test]
fn decodes_markets_and_barters() {
    let header = |id: u16, body: &[u8]| {
        let mut packet = id.to_le_bytes().to_vec();
        packet.extend_from_slice(&((4 + body.len()) as u16).to_le_bytes());
        packet.extend_from_slice(body);
        packet
    };
    let words = |values: &[(u32, usize)]| values.iter().flat_map(|&(value, size)| value.to_le_bytes()[..size].to_vec()).collect::<Vec<u8>>();
    // A market: Phracon, 200 z, 999,999 left; Emveretarcon, 1,000 z, no end.
    let mut market = words(&[(1010, 4), (3, 1), (200, 4), (999_999, 4), (20, 2), (0, 4)]);
    market.extend(words(&[(1011, 4), (3, 1), (1000, 4), (UNLIMITED, 4), (20, 2), (0, 4)]));
    let Some(MapEvent::Market { kind: MarketKind::Market, goods }) = decode_map_packet(&header(ZC_NPC_MARKET_OPEN, &market)) else { panic!() };
    assert_eq!(goods.iter().map(|goods| (goods.item, goods.zeny, goods.stock)).collect::<Vec<_>>(), [(1010, 200, Some(999_999)), (1011, 1000, None)]);
    let mut result = words(&[(0, 2)]);
    result.extend(words(&[(1010, 4), (2, 2), (200, 4)]));
    let Some(MapEvent::MarketBought { ok: true, items }) = decode_map_packet(&header(ZC_NPC_MARKET_PURCHASE_RESULT, &result)) else { panic!() };
    assert_eq!(items, [(1010, 2)]);
    // A barter: a Shadowdecon for 20 Shadowdecon Ore.
    let barter = words(&[(25729, 4), (3, 1), (UNLIMITED, 4), (25728, 4), (20, 4), (10, 4), (1, 4), (0, 2), (0, 4)]);
    let Some(MapEvent::Market { kind: MarketKind::Barter, goods }) = decode_map_packet(&header(ZC_NPC_BARTER_MARKET_ITEMINFO, &barter)) else { panic!() };
    assert_eq!((goods[0].item, goods[0].costs.clone(), goods[0].index, goods[0].stock), (25729, vec![(25728, 20)], 1, None));
    // An expanded barter: Oridecon for 5 Oridecon Stones; Bradium for 3 Oridecon and 50,000 z.
    let mut expanded = words(&[(2, 4)]);
    expanded.extend(words(&[(984, 4), (3, 2), (UNLIMITED, 4), (10, 4), (1, 4), (0, 4), (0, 2), (0, 4), (1, 4)]));
    expanded.extend(words(&[(756, 4), (0, 2), (5, 4), (3, 2)]));
    expanded.extend(words(&[(6224, 4), (3, 2), (UNLIMITED, 4), (10, 4), (2, 4), (50_000, 4), (0, 2), (0, 4), (1, 4)]));
    expanded.extend(words(&[(984, 4), (0, 2), (3, 4), (3, 2)]));
    let Some(MapEvent::Market { kind: MarketKind::Expanded, goods }) = decode_map_packet(&header(ZC_NPC_EXPANDED_BARTER_MARKET_ITEMINFO, &expanded)) else {
        panic!()
    };
    let read: Vec<_> = goods.iter().map(|goods| (goods.item, goods.zeny, goods.costs.clone(), goods.index)).collect();
    assert_eq!(read, [(984, 0, vec![(756, 5)], 1), (6224, 50_000, vec![(984, 3)], 2)]);

    assert_eq!(encode_market_buy(&[(1010, 2)]), [0xD6, 0x09, 12, 0, 0xF2, 0x03, 0, 0, 2, 0, 0, 0]);
    assert_eq!(encode_barter_buy(&[(25729, 1, 5, 1)]).len(), 18);
    assert_eq!(encode_expanded_barter_buy(&[(984, 1, 1)])[4..], [0xD8, 0x03, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0]);
    let opens = [ZC_NPC_MARKET_OPEN, ZC_NPC_MARKET_PURCHASE_RESULT, ZC_NPC_BARTER_MARKET_ITEMINFO, ZC_NPC_EXPANDED_BARTER_MARKET_ITEMINFO].map(packet_length);
    assert_eq!(opens, [Some(-1); 4]);
    let closes = [CZ_NPC_MARKET_CLOSE, CZ_NPC_BARTER_MARKET_CLOSE, CZ_NPC_EXPANDED_BARTER_MARKET_CLOSE, CZ_NPC_TRADE_QUIT].map(packet_length);
    assert_eq!(closes, [Some(2); 4]);
}

#[test]
fn decodes_making_lists() {
    let mut make = ZC_MAKABLEITEMLIST.to_le_bytes().to_vec();
    make.extend_from_slice(&36u16.to_le_bytes());
    for item in [501u32, 502] {
        make.extend_from_slice(&item.to_le_bytes());
        make.extend_from_slice(&[0; 12]);
    }
    let Some(MapEvent::MakeList(items)) = decode_map_packet(&make) else { panic!() };
    assert_eq!(items, [501, 502]);
    let cook = [&ZC_MAKINGITEM_LIST.to_le_bytes()[..], &10u16.to_le_bytes(), &1u16.to_le_bytes(), &12046u32.to_le_bytes()].concat();
    let Some(MapEvent::CookList { kind: 1, items }) = decode_map_packet(&cook) else { panic!() };
    assert_eq!(items, [12046]);
    let arrows = [&ZC_MAKINGARROW_LIST.to_le_bytes()[..], &8u16.to_le_bytes(), &713u32.to_le_bytes()].concat();
    let Some(MapEvent::ArrowList(items)) = decode_map_packet(&arrows) else { panic!() };
    assert_eq!(items, [713]);
    let made = [&ZC_ACK_REQMAKINGITEM.to_le_bytes()[..], &2u16.to_le_bytes(), &501u32.to_le_bytes()].concat();
    let Some(MapEvent::Made { result: 2, item: 501 }) = decode_map_packet(&made) else { panic!() };
    assert_eq!(encode_make(501)[..6], [0x8E, 0x01, 0xF5, 0x01, 0, 0]);
    assert_eq!(encode_make(0), [0x8E, 0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(encode_cook(1, 12046), [0x5B, 0x02, 1, 0, 0x0E, 0x2F, 0, 0]);
    assert_eq!(encode_make_arrow(713), [0xAE, 0x01, 0xC9, 0x02, 0, 0]);
    let ids = [ZC_MAKABLEITEMLIST, CZ_REQMAKINGITEM, ZC_ACK_REQMAKINGITEM, ZC_MAKINGITEM_LIST, CZ_REQ_MAKINGITEM, ZC_MAKINGARROW_LIST, CZ_REQ_MAKINGARROW];
    assert_eq!(ids.map(packet_length), [Some(-1), Some(18), Some(8), Some(-1), Some(8), Some(-1), Some(6)]);
}

#[test]
fn decodes_blacksmith_lists() {
    let mut repair = ZC_REPAIRITEMLIST.to_le_bytes().to_vec();
    repair.extend_from_slice(&28u16.to_le_bytes());
    let mut entry = vec![0u8; 24];
    entry[..2].copy_from_slice(&3u16.to_le_bytes());
    entry[2..6].copy_from_slice(&1201u32.to_le_bytes());
    entry[6..10].copy_from_slice(&4035u32.to_le_bytes());
    entry[22] = 4;
    repair.extend_from_slice(&entry);
    let Some(MapEvent::RepairList(items)) = decode_map_packet(&repair) else { panic!() };
    assert_eq!((items[0].index, items[0].id, items[0].cards[0], items[0].refine), (3, 1201, 4035, 4));
    // The pick goes back as it came.
    assert_eq!(encode_repair(Some(&items[0]))[2..], entry[..]);
    assert_eq!(encode_repair(None)[..4], [0x66, 0x0B, 0xFF, 0xFF]);
    let Some(MapEvent::Repaired { index: 5, ok: true }) = decode_map_packet(&[0xFE, 0x01, 5, 0, 0]) else { panic!() };

    let mut upgrade = ZC_NOTIFY_WEAPONITEMLIST.to_le_bytes().to_vec();
    upgrade.extend_from_slice(&27u16.to_le_bytes());
    upgrade.extend_from_slice(&4u16.to_le_bytes());
    upgrade.extend_from_slice(&1201u32.to_le_bytes());
    upgrade.push(0);
    upgrade.extend_from_slice(&[0; 16]);
    let Some(MapEvent::UpgradeList(items)) = decode_map_packet(&upgrade) else { panic!() };
    assert_eq!((items[0].index, items[0].id), (4, 1201));
    assert_eq!(encode_upgrade(4), [0x22, 0x02, 4, 0, 0, 0]);
    let upgraded = [&ZC_ACK_WEAPONREFINE.to_le_bytes()[..], &0u32.to_le_bytes(), &1201u32.to_le_bytes()].concat();
    let Some(MapEvent::Upgraded { result: 0, item: 1201 }) = decode_map_packet(&upgraded) else { panic!() };
    let ids = [ZC_REPAIRITEMLIST, CZ_REQ_ITEMREPAIR2, ZC_ACK_ITEMREPAIR, ZC_NOTIFY_WEAPONITEMLIST, CZ_REQ_WEAPONREFINE, ZC_ACK_WEAPONREFINE];
    assert_eq!(ids.map(packet_length), [Some(-1), Some(26), Some(5), Some(-1), Some(6), Some(10)]);
}

#[test]
fn decodes_skill_pick_lists() {
    let spells = [&ZC_AUTOSPELLLIST.to_le_bytes()[..], &12u16.to_le_bytes(), &19u32.to_le_bytes(), &14u32.to_le_bytes()].concat();
    let Some(MapEvent::SpellList(skills)) = decode_map_packet(&spells) else { panic!() };
    assert_eq!(skills, [19, 14]);
    let select = [&ZC_SKILL_SELECT_REQUEST.to_le_bytes()[..], &12u16.to_le_bytes(), &1u32.to_le_bytes(), &19u16.to_le_bytes(), &14u16.to_le_bytes()].concat();
    let Some(MapEvent::SkillSelect { why: 1, skills }) = decode_map_packet(&select) else { panic!() };
    assert_eq!(skills, [19, 14]);
    assert_eq!(encode_autospell(19), [0xCE, 0x01, 19, 0, 0, 0]);
    assert_eq!(encode_autospell(0), [0xCE, 0x01, 0, 0, 0, 0]);
    assert_eq!(encode_skill_select(1, 19), [0x43, 0x04, 1, 0, 0, 0, 19, 0]);
    assert_eq!(encode_skill_select(1, 0), [0x43, 0x04, 1, 0, 0, 0, 0, 0]);
    let ids = [ZC_AUTOSPELLLIST, CZ_SELECTAUTOSPELL, ZC_SKILL_SELECT_REQUEST, CZ_SKILL_SELECT_RESPONSE];
    assert_eq!(ids.map(packet_length), [Some(-1), Some(6), Some(-1), Some(8)]);
}

#[test]
fn decodes_the_refine_window() {
    let Some(MapEvent::RefineOpen) = decode_map_packet(&[0xA0, 0x0A]) else { panic!() };
    let mut list = ZC_REFINING_MATERIAL_LIST.to_le_bytes().to_vec();
    list.extend_from_slice(&25u16.to_le_bytes());
    list.extend_from_slice(&4u16.to_le_bytes());
    list.push(0);
    for (ore, chance, zeny) in [(1010u32, 100u8, 50u32), (6240, 100, 5000)] {
        list.extend_from_slice(&ore.to_le_bytes());
        list.push(chance);
        list.extend_from_slice(&zeny.to_le_bytes());
    }
    let Some(MapEvent::RefineMaterials { index: 4, blessings: 0, materials }) = decode_map_packet(&list) else { panic!() };
    assert_eq!(materials, [(1010, 100, 50), (6240, 100, 5000)]);
    let Some(MapEvent::Refined { result: 0, index: 4, refine: 1 }) = decode_map_packet(&[0x88, 0x01, 0, 0, 4, 0, 1, 0]) else { panic!() };
    assert_eq!(encode_refine(4, 1010, true), [0xA3, 0x0A, 4, 0, 0xF2, 0x03, 0, 0, 1]);
    let ids = [ZC_OPEN_REFINING_UI, CZ_REFINING_SELECT_ITEM, ZC_REFINING_MATERIAL_LIST, CZ_REQ_REFINING, CZ_CLOSE_REFINING_UI, ZC_ACK_ITEMREFINING];
    assert_eq!(ids.map(packet_length), [Some(2), Some(4), Some(-1), Some(9), Some(2), Some(8)]);
}

#[test]
fn decodes_grade_enchant() {
    assert!(matches!(decode_map_packet(&[0xE2, 0x0A, 8, 0, 0, 0, 0]), Some(MapEvent::UiOpen { kind: 8, data: 0 })));
    assert!(matches!(decode_map_packet(&[0xE2, 0x0A, 1, 7, 0, 0, 0]), Some(MapEvent::UiOpen { kind: 1, data: 7 })));

    // Two rows: 1 Etel Aquamarine for 175,000 z (can break), 5 for 875,000 z. Blessings: 1 a step, 10 steps, +1%.
    let mut list = ZC_GRADE_ENCHANT_MATERIAL_LIST.to_le_bytes().to_vec();
    list.extend_from_slice(&68u16.to_le_bytes());
    list.extend_from_slice(&4i16.to_le_bytes());
    list.extend_from_slice(&80u32.to_le_bytes());
    for value in [1_000_337u32, 1, 10, 1, 0, 0] {
        list.extend_from_slice(&value.to_le_bytes());
    }
    for (item, amount, zeny, lowers, breaks) in [(1_000_325u32, 1u32, 175_000u32, 0u32, 1u8), (1_000_325, 5, 875_000, 1, 0)] {
        list.extend_from_slice(&item.to_le_bytes());
        list.extend_from_slice(&amount.to_le_bytes());
        list.extend_from_slice(&zeny.to_le_bytes());
        list.extend_from_slice(&lowers.to_le_bytes());
        list.push(breaks);
    }
    assert_eq!(list.len(), 68);
    let Some(MapEvent::GradeMaterials { index: 4, chance: 80, blessing, materials }) = decode_map_packet(&list) else { panic!() };
    assert_eq!(blessing, GradeBlessing { item: 1_000_337, per_step: 1, most: 10, chance: 1 });
    assert_eq!(
        materials,
        [
            GradeMaterial { item: 1_000_325, amount: 1, zeny: 175_000, lowers_refine: false, can_break: true },
            GradeMaterial { item: 1_000_325, amount: 5, zeny: 875_000, lowers_refine: true, can_break: false },
        ]
    );
    let mut none = vec![0u8; 34];
    none[..2].copy_from_slice(&ZC_GRADE_ENCHANT_MATERIAL_LIST.to_le_bytes());
    none[2..4].copy_from_slice(&34u16.to_le_bytes());
    none[4..6].copy_from_slice(&(-1i16).to_le_bytes());
    let Some(MapEvent::GradeMaterials { index: -1, chance: 0, materials, .. }) = decode_map_packet(&none) else { panic!() };
    assert!(materials.is_empty());

    let result = [&ZC_GRADE_ENCHANT_ACK.to_le_bytes()[..], &4u16.to_le_bytes(), &1u16.to_le_bytes(), &0u32.to_le_bytes()].concat();
    let Some(MapEvent::GradeResult { index: 4, grade: 1, result: 0 }) = decode_map_packet(&result) else { panic!() };

    let mut announced = ZC_GRADE_ENCHANT_BROADCAST_RESULT.to_le_bytes().to_vec();
    let mut name = b"Pori**".to_vec();
    name.resize(24, 0);
    announced.extend_from_slice(&name);
    announced.extend_from_slice(&15282u32.to_le_bytes());
    announced.extend_from_slice(&1u16.to_le_bytes());
    announced.push(1);
    assert_eq!(Some(announced.len() as i32), packet_length(ZC_GRADE_ENCHANT_BROADCAST_RESULT));
    let Some(MapEvent::GradeAnnounced { player, item: 15282, grade: 1, success: true }) = decode_map_packet(&announced) else { panic!() };
    assert_eq!(player, "Pori**");
    announced[32] = 0;
    let Some(MapEvent::GradeAnnounced { success: false, .. }) = decode_map_packet(&announced) else { panic!() };

    let mut equip = vec![0u8; 5 + EQUIP_ITEM_SIZE];
    equip[..2].copy_from_slice(&ZC_INVENTORY_ITEMLIST_EQUIP.to_le_bytes());
    let entry = &mut equip[5..];
    entry[..2].copy_from_slice(&7u16.to_le_bytes());
    entry[2..6].copy_from_slice(&15282u32.to_le_bytes());
    entry[65] = 20;
    entry[66] = 2;
    entry[67] = 1;
    let Some(MapEvent::InventoryItems { items, .. }) = decode_map_packet(&equip) else { panic!() };
    assert_eq!((items[0].id, items[0].refine, items[0].grade), (15282, 20, 2));

    let mut pickup = vec![0u8; 70];
    pickup[..2].copy_from_slice(&ZC_ITEM_PICKUP_ACK.to_le_bytes());
    pickup[2..4].copy_from_slice(&7u16.to_le_bytes());
    pickup[6..10].copy_from_slice(&15282u32.to_le_bytes());
    pickup[68] = 20;
    pickup[69] = 1;
    let Some(MapEvent::PickedUp { item, .. }) = decode_map_packet(&pickup) else { panic!() };
    assert_eq!((item.refine, item.grade), (20, 1));

    assert_eq!(encode_grade_select(4), [0x59, 0x0B, 4, 0]);
    assert_eq!(encode_grade_enchant(4, 0, 2), [0x5B, 0x0B, 4, 0, 0, 0, 0, 0, 1, 2, 0, 0, 0, 0]);
    assert_eq!(encode_grade_enchant(4, 1, 0), [0x5B, 0x0B, 4, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(encode_close_grade(), [0x5C, 0x0B]);
    let ids = [
        ZC_UI_OPEN,
        CZ_GRADE_ENCHANT_SELECT_EQUIPMENT,
        ZC_GRADE_ENCHANT_MATERIAL_LIST,
        CZ_GRADE_ENCHANT_REQUEST,
        CZ_GRADE_ENCHANT_CLOSE_UI,
        ZC_GRADE_ENCHANT_ACK,
        ZC_GRADE_ENCHANT_BROADCAST_RESULT,
    ];
    assert_eq!(ids.map(packet_length), [Some(7), Some(4), Some(-1), Some(14), Some(2), Some(10), Some(33)]);
}

#[test]
fn decodes_laphine_boxes() {
    let open = [&ZC_RANDOM_COMBINE_ITEM_UI_OPEN.to_le_bytes()[..], &100_000u32.to_le_bytes()].concat();
    assert!(matches!(decode_map_packet(&open), Some(MapEvent::SynthesisOpen(100_000))));
    assert!(matches!(decode_map_packet(&[&ZC_ACK_RANDOM_COMBINE_ITEM.to_le_bytes()[..], &5u16.to_le_bytes()].concat()), Some(MapEvent::Synthesized(5))));
    let entries = [(2u16, 1u16), (4, 1), (5, 2)];
    let made = encode_synthesis(100_000, &entries);
    assert_eq!(made.len(), 20);
    assert_eq!(&made[..8], &[&CZ_REQ_RANDOM_COMBINE_ITEM.to_le_bytes()[..], &20u16.to_le_bytes(), &100_000u32.to_le_bytes()].concat());
    assert_eq!(&made[8..], &entries.into_iter().flat_map(|(index, amount)| [index.to_le_bytes(), amount.to_le_bytes()].concat()).collect::<Vec<_>>());
    assert_eq!(encode_synthesis_close(), CZ_RANDOM_COMBINE_ITEM_UI_CLOSE.to_le_bytes());

    let upgrade = [&ZC_RANDOM_UPGRADE_ITEM_UI_OPEN.to_le_bytes()[..], &100_626u32.to_le_bytes()].concat();
    assert!(matches!(decode_map_packet(&upgrade), Some(MapEvent::UpgradeOpen(100_626))));
    assert!(matches!(decode_map_packet(&[&ZC_ACK_RANDOM_UPGRADE_ITEM.to_le_bytes()[..], &1u16.to_le_bytes()].concat()), Some(MapEvent::LaphineUpgraded(1))));
    let upgrade_request: [u8; 8] = [&CZ_REQ_RANDOM_UPGRADE_ITEM.to_le_bytes()[..], &100_626u32.to_le_bytes(), &7u16.to_le_bytes()].concat().try_into().unwrap();
    assert_eq!(encode_upgrade_item(100_626, 7), upgrade_request);
    assert_eq!(encode_upgrade_close(), CZ_RANDOM_UPGRADE_ITEM_UI_CLOSE.to_le_bytes());

    let mut preview = vec![0u8; 48];
    preview[..2].copy_from_slice(&ZC_CHANGE_ITEM_OPTION.to_le_bytes());
    preview[2..4].copy_from_slice(&7u16.to_le_bytes());
    preview[4] = 1;
    preview[5..9].copy_from_slice(&4001u32.to_le_bytes());
    preview[46] = 7;
    preview[47] = 1;
    match decode_map_packet(&preview) {
        Some(MapEvent::ItemPreview { index: 7, cards, refine: 7, grade: 1, broken: true }) => assert_eq!(cards, [4001, 0, 0, 0]),
        other => panic!("{other:?}"),
    }
    let ids = [
        ZC_RANDOM_COMBINE_ITEM_UI_OPEN,
        CZ_REQ_RANDOM_COMBINE_ITEM,
        ZC_ACK_RANDOM_COMBINE_ITEM,
        CZ_RANDOM_COMBINE_ITEM_UI_CLOSE,
        ZC_RANDOM_UPGRADE_ITEM_UI_OPEN,
        CZ_RANDOM_UPGRADE_ITEM_UI_CLOSE,
        CZ_REQ_RANDOM_UPGRADE_ITEM,
        ZC_ACK_RANDOM_UPGRADE_ITEM,
        ZC_CHANGE_ITEM_OPTION,
    ];
    assert_eq!(ids.map(packet_length), [Some(6), Some(-1), Some(4), Some(2), Some(6), Some(2), Some(8), Some(4), Some(48)]);
}

#[test]
fn decodes_reform() {
    // ZC_OPEN_REFORM_UI: the reform item (Thanos Upgrade Box).
    let mut open = ZC_OPEN_REFORM_UI.to_le_bytes().to_vec();
    open.extend_from_slice(&100817u32.to_le_bytes());
    assert_eq!(open.len(), 6);
    let Some(MapEvent::ReformOpen(100817)) = decode_map_packet(&open) else { panic!() };
    // ZC_ITEM_REFORM_ACK: index 3, result 0 (done).
    let Some(MapEvent::Reformed { index: 3, result: 0 }) = decode_map_packet(&[0x92, 0x0B, 3, 0, 0]) else { panic!() };
    // 100817 is D1 89 01 00.
    assert_eq!(encode_item_reform(100817, 4), [0x91, 0x0B, 0xD1, 0x89, 0x01, 0x00, 4, 0]);
    assert_eq!(encode_close_reform(), [0x90, 0x0B]);
    let ids = [ZC_OPEN_REFORM_UI, CZ_CLOSE_REFORM_UI, CZ_ITEM_REFORM, ZC_ITEM_REFORM_ACK];
    assert_eq!(ids.map(packet_length), [Some(6), Some(2), Some(8), Some(5)]);
}

#[test]
fn decodes_the_map_property() {
    let packet = [&ZC_MAPPROPERTY_R2.to_le_bytes()[..], &3u16.to_le_bytes(), &(MAP_NO_COSTUME | 1).to_le_bytes()].concat();
    assert!(matches!(decode_map_packet(&packet), Some(MapEvent::MapProperty { property: 3, flags }) if flags == MAP_NO_COSTUME | 1));
    assert_eq!(packet_length(ZC_MAPPROPERTY_R2), Some(8));
    assert_eq!(encode_blocking_play_cancel(), [0x47, 0x04]);
    assert!(matches!(decode_map_packet(&[0xD6, 0x01, 19, 0]), Some(MapEvent::MapType(19))));
    assert!(matches!(decode_map_packet(&[0x02, 0x0A, 0, 0]), Some(MapEvent::DressRoom)));
}

#[test]
fn decodes_store_searches() {
    assert!(matches!(decode_map_packet(&[0x3A, 0x08, 1, 0, 10]), Some(MapEvent::CatalogOpen { remote: true, uses: 10 })));
    assert!(matches!(decode_map_packet(&[0x37, 0x08, 3]), Some(MapEvent::CatalogFailed(3))));
    // Two results, the second with cards and a refine.
    let mut results = ZC_SEARCH_STORE_INFO_ACK.to_le_bytes().to_vec();
    results.extend_from_slice(&[0, 0, 1, 1, 9]);
    for (store, item, refine, card) in [(11u32, 909u32, 0u8, 0u32), (12, 1201, 7, 4035)] {
        let mut row = vec![0u8; CATALOG_ROW_SIZE];
        row[..4].copy_from_slice(&store.to_le_bytes());
        row[4..8].copy_from_slice(&2_000_001u32.to_le_bytes());
        row[8..20].copy_from_slice(b"Catalog test");
        row[88..92].copy_from_slice(&item.to_le_bytes());
        row[93..97].copy_from_slice(&1500u32.to_le_bytes());
        row[97..99].copy_from_slice(&10u16.to_le_bytes());
        row[99..103].copy_from_slice(&card.to_le_bytes());
        row[140] = refine;
        results.extend_from_slice(&row);
    }
    let length = results.len() as u16;
    results[2..4].copy_from_slice(&length.to_le_bytes());
    let Some(MapEvent::CatalogResults { first: true, more: true, uses: 9, rows }) = decode_map_packet(&results) else { panic!() };
    assert_eq!(
        (rows[0].store, rows[0].account, rows[0].shop.as_str(), rows[0].item, rows[0].price, rows[0].amount),
        (11, 2_000_001, "Catalog test", 909, 1500, 10)
    );
    assert_eq!((rows[1].item, rows[1].refine, rows[1].cards), (1201, 7, [4035, 0, 0, 0]));
    let empty = [&ZC_SEARCH_STORE_INFO_ACK.to_le_bytes()[..], &7u16.to_le_bytes(), &[1, 0, 8]].concat();
    assert!(matches!(decode_map_packet(&empty), Some(MapEvent::CatalogResults { rows, uses: 8, .. }) if rows.is_empty()));
    let mark = |x: i16, y: i16| [&ZC_SSILIST_ITEM_CLICK_ACK.to_le_bytes()[..], &x.to_le_bytes(), &y.to_le_bytes()].concat();
    assert!(matches!(decode_map_packet(&mark(170, 330)), Some(MapEvent::CatalogMark(Some([170, 330])))));
    assert!(matches!(decode_map_packet(&mark(-1, -1)), Some(MapEvent::CatalogMark(None))));
    // 0446 <npc id>.L <x>.W <y>.W <effect>.W <color>.W
    let quest = [
        &ZC_QUEST_NOTIFY_EFFECT.to_le_bytes()[..],
        &110_00042u32.to_le_bytes(),
        &150u16.to_le_bytes(),
        &170u16.to_le_bytes(),
        &9999u16.to_le_bytes(),
        &2u16.to_le_bytes(),
    ]
    .concat();
    assert!(matches!(decode_map_packet(&quest), Some(MapEvent::QuestMark { npc: 110_00042, cell: [150, 170], effect: 9999, color: 2 })));
    // Max price before min, two items, no cards: 23 bytes.
    let search = encode_store_search(false, 2000, 100, &[909, 910], &[]);
    assert_eq!(search.len(), 23);
    assert_eq!(&search[..15], &[0x35, 0x08, 23, 0, 0, 0xD0, 0x07, 0, 0, 100, 0, 0, 0, 2, 0]);
    assert_eq!(&search[15..], &[0x8D, 0x03, 0, 0, 0x8E, 0x03, 0, 0]);
    assert_eq!(encode_store_pick(2_000_001, 11, 909), [0x3C, 0x08, 0x81, 0x84, 0x1E, 0, 11, 0, 0, 0, 0x8D, 0x03, 0, 0]);
    assert_eq!(packet_length(CZ_SEARCH_STORE_INFO_NEXT_PAGE), Some(2));
    assert_eq!(packet_length(CZ_CLOSE_SEARCH_STORE_INFO), Some(2));
    assert_eq!(packet_length(ZC_SEARCH_STORE_INFO_ACK), Some(-1));
}

#[test]
fn decodes_rankings() {
    // clif_ranklist: two filled places, the rest empty, and our 50 points.
    let mut ranking = vec![0u8; 88];
    ranking[..2].copy_from_slice(&ZC_ACK_RANKING2.to_le_bytes());
    ranking[4..8].copy_from_slice(&150_000u32.to_le_bytes());
    ranking[8..12].copy_from_slice(&150_002u32.to_le_bytes());
    ranking[44..48].copy_from_slice(&50u32.to_le_bytes());
    ranking[48..52].copy_from_slice(&10u32.to_le_bytes());
    ranking[84..88].copy_from_slice(&50u32.to_le_bytes());
    let Some(MapEvent::Ranking { kind: 0, ids, points, mine: 50 }) = decode_map_packet(&ranking) else { panic!() };
    assert_eq!((&ids[..3], &points[..3]), (&[150_000, 150_002, 0][..], &[50, 10, 0][..]));
    let name = [&ZC_ACK_REQNAME_BYGID.to_le_bytes()[..], &3u16.to_le_bytes(), &150_000u32.to_le_bytes(), &name_field("Rndev")].concat();
    assert!(matches!(decode_map_packet(&name), Some(MapEvent::CharName { id: 150_000, name: Some(name) }) if name == "Rndev"));
    let nameless = [&ZC_ACK_REQNAME_BYGID.to_le_bytes()[..], &2u16.to_le_bytes(), &7u32.to_le_bytes(), &[0u8; 24]].concat();
    assert!(matches!(decode_map_packet(&nameless), Some(MapEvent::CharName { id: 7, name: None })));
    // @addfame -50 arrives as a negative amount.
    let points = [&ZC_UPDATE_RANKING_POINT.to_le_bytes()[..], &0u16.to_le_bytes(), &(-50i32).to_le_bytes(), &0u32.to_le_bytes()].concat();
    assert!(matches!(decode_map_packet(&points), Some(MapEvent::RankingPoints { kind: 0, added: -50, total: 0 })));
    let mut star = vec![0u8; 32];
    star[..2].copy_from_slice(&ZC_STARSKILL.to_le_bytes());
    star[2..8].copy_from_slice(b"Poring");
    star[26..30].copy_from_slice(&1002u32.to_le_bytes());
    star[30] = 7;
    star[31] = STAR_MISSION;
    assert!(matches!(decode_map_packet(&star), Some(MapEvent::StarSkill { monster: 1002, count: 7, kind: STAR_MISSION, .. })));
    assert_eq!(encode_ranking_request(3), [0x7C, 0x09, 3, 0]);
    assert_eq!(encode_name_request(150_000), [0x69, 0x03, 0xF0, 0x49, 0x02, 0]);
    for (header, length) in
        [(ZC_ACK_RANKING2, 88), (ZC_ACK_REQNAME_BYGID, 32), (ZC_UPDATE_RANKING_POINT, 12), (ZC_STARSKILL, 32), (CZ_REQ_RANKING, 4), (CZ_REQNAME_BYGID, 6)]
    {
        assert_eq!(packet_length(header), Some(length), "{header:#06x}");
    }
    assert_eq!(packet_length(ZC_DRESSROOM_OPEN), Some(4));
    // clif_pvpset: ours and another player's.
    let rank = |account: u32, rank: u32, players: u32| {
        [&ZC_NOTIFY_RANKING.to_le_bytes()[..], &account.to_le_bytes(), &rank.to_le_bytes(), &players.to_le_bytes()].concat()
    };
    assert!(matches!(decode_map_packet(&rank(2_000_000, 3, 12)), Some(MapEvent::PvpRank { account: 2_000_000, rank: 3, players: 12 })));
    assert!(matches!(decode_map_packet(&rank(2_000_001, u32::MAX, 12)), Some(MapEvent::PvpRank { account: 2_000_001, rank: u32::MAX, .. })));
    // The PvP record's answer (ZC_ACK_PVPPOINT) shows nothing: it's skipped by its length.
    assert!(decode_map_packet(&[&0x0210u16.to_le_bytes()[..], &[0; 20]].concat()).is_none());
    for (header, length) in [(ZC_NOTIFY_MAPPROPERTY2, 4), (ZC_NOTIFY_RANKING, 14), (0x020F, 10), (0x0210, 22)] {
        assert_eq!(packet_length(header), Some(length), "{header:#06x}");
    }
    assert_eq!(packet_length(CZ_BLOCKING_PLAY_CANCEL), Some(2));
}

#[test]
fn decodes_party_extras() {
    // clif_party_show_picker: a Knife +4, identified, picked up by account 2000001.
    let mut pickup = vec![0u8; 33];
    pickup[..2].copy_from_slice(&ZC_ITEM_PICKUP_PARTY.to_le_bytes());
    pickup[2..6].copy_from_slice(&2_000_001u32.to_le_bytes());
    pickup[6..10].copy_from_slice(&1201u32.to_le_bytes());
    pickup[10] = 1;
    pickup[30] = 5;
    pickup[31] = 4;
    assert!(matches!(decode_map_packet(&pickup), Some(MapEvent::PartyPickup { account: 2_000_001, item: 1201, identified: true, refine: 4 })));
    assert_eq!(packet_length(ZC_ITEM_PICKUP_PARTY), Some(33));
    assert!(matches!(decode_map_packet(&[0xC9, 0x02, 1]), Some(MapEvent::PartyRefusing(true))));
    assert!(matches!(decode_map_packet(&[0xC9, 0x02, 0]), Some(MapEvent::PartyRefusing(false))));

    // clif_party_option: all three, and the refused EXP share (2).
    let options = |exp: u32, pickup: u8, share: u8| [&ZC_REQ_GROUPINFO_CHANGE_V2.to_le_bytes()[..], &exp.to_le_bytes(), &[pickup, share]].concat();
    assert!(matches!(decode_map_packet(&options(1, 1, 0)), Some(MapEvent::PartyOptions { exp: 1, pickup: true, share: false })));
    assert!(matches!(decode_map_packet(&options(2, 0, 1)), Some(MapEvent::PartyOptions { exp: 2, pickup: false, share: true })));

    // clif_party_xy_remove: -1, -1.
    let position = |x: i16, y: i16| [&ZC_NOTIFY_POSITION_TO_GROUPM.to_le_bytes()[..], &7u32.to_le_bytes(), &x.to_le_bytes(), &y.to_le_bytes()].concat();
    assert!(matches!(decode_map_packet(&position(-1, -1)), Some(MapEvent::PartyPosition { account: 7, cell: None })));
    assert!(matches!(decode_map_packet(&position(156, 180)), Some(MapEvent::PartyPosition { account: 7, cell: Some([156, 180]) })));

    assert_eq!(encode_party_options(true, false, true), [0xD7, 0x07, 1, 0, 0, 0, 0, 1]);
    assert_eq!(encode_party_options(false, true, false), [0xD7, 0x07, 0, 0, 0, 0, 1, 0]);
    for (header, length) in [(CZ_PARTY_CONFIG, 3), (ZC_PARTY_CONFIG, 3), (CZ_GROUPINFO_CHANGE_V2, 8), (CZ_CHANGE_GROUP_MASTER, 6)] {
        assert_eq!(packet_length(header), Some(length), "{header:#06x}");
    }
}

#[test]
fn decodes_mercenaries() {
    // clif_mercenary_info for Mina.
    let mut init = vec![0u8; 80];
    init[..2].copy_from_slice(&ZC_MER_INIT.to_le_bytes());
    init[2..6].copy_from_slice(&110_038_127u32.to_le_bytes());
    init[6..8].copy_from_slice(&300u16.to_le_bytes());
    init[20..22].copy_from_slice(&700u16.to_le_bytes());
    init[22..26].copy_from_slice(b"Mina");
    init[46..48].copy_from_slice(&20u16.to_le_bytes());
    init[48..52].copy_from_slice(&256u32.to_le_bytes());
    init[52..56].copy_from_slice(&256u32.to_le_bytes());
    init[64..68].copy_from_slice(&1_800_000_000u32.to_le_bytes());
    init[68..70].copy_from_slice(&3u16.to_le_bytes());
    init[70..74].copy_from_slice(&4u32.to_le_bytes());
    init[74..78].copy_from_slice(&5u32.to_le_bytes());
    init[78..80].copy_from_slice(&9u16.to_le_bytes());
    let Some(MapEvent::MercenaryInfo(mut mercenary)) = decode_map_packet(&init) else { panic!() };
    assert_eq!((mercenary.id, mercenary.name.as_str(), mercenary.level, mercenary.atk, mercenary.aspd), (110_038_127, "Mina", 20, 300, 700));
    assert_eq!((mercenary.hp, mercenary.max_hp, mercenary.contract_end), (256, 256, 1_800_000_000));
    assert_eq!((mercenary.loyalty, mercenary.summons, mercenary.kills, mercenary.range), (3, 4, 5, 9));
    assert_eq!(packet_length(ZC_MER_INIT), Some(80));

    let mut skills = ZC_MER_SKILLINFO_LIST.to_le_bytes().to_vec();
    skills.extend_from_slice(&41u16.to_le_bytes());
    for id in [8207u16, 8221] {
        let mut entry = vec![0u8; SKILL_SIZE];
        entry[..2].copy_from_slice(&id.to_le_bytes());
        entry[6..8].copy_from_slice(&2u16.to_le_bytes());
        skills.extend_from_slice(&entry);
    }
    let length = skills.len() as u16;
    skills[2..4].copy_from_slice(&length.to_le_bytes());
    let Some(MapEvent::MercenarySkills(list)) = decode_map_packet(&skills) else { panic!() };
    assert_eq!(list.iter().map(|skill| (skill.id, skill.level)).collect::<Vec<_>>(), [(8207, 2), (8221, 2)]);
    assert_eq!(packet_length(ZC_MER_SKILLINFO_LIST), Some(-1));

    let change = [&ZC_MER_PAR_CHANGE.to_le_bytes()[..], &MERCENARY_HP.to_le_bytes(), &99u32.to_le_bytes()].concat();
    let Some(MapEvent::MercenaryChanged { kind, value }) = decode_map_packet(&change) else { panic!() };
    mercenary.set(kind, value);
    mercenary.set(MERCENARY_KILLS, 6);
    mercenary.set(MERCENARY_LOYALTY, 70_000);
    assert_eq!((mercenary.hp, mercenary.kills, mercenary.loyalty), (99, 6, u16::MAX));

    assert_eq!(encode_move_npc(7, [170, 360]), [0x32, 0x02, 7, 0, 0, 0, 170 >> 2, ((170 << 6) as u8) | (360 >> 4) as u8, (360 << 4) as u8]);
    assert_eq!(encode_act_npc(7, 9, 1), [0x33, 0x02, 7, 0, 0, 0, 9, 0, 0, 0, 1]);
    for (header, length) in [(CZ_MER_COMMAND, 3), (CZ_REQUEST_MOVENPC, 9), (CZ_REQUEST_ACTNPC, 11), (CZ_REQUEST_MOVETOOWNER, 6), (ZC_MER_PAR_CHANGE, 8)] {
        assert_eq!(packet_length(header), Some(length), "{header:#06x}");
    }
}

#[test]
fn decodes_elementals() {
    // clif_elemental_info, then clif_elemental_updatestatus with its SP.
    let init = [&ZC_EL_INIT.to_le_bytes()[..], &[7u32, 300, 400, 50, 60].map(u32::to_le_bytes).concat()].concat();
    let Some(MapEvent::ElementalInfo { id: 7, values: [300, 400, 50, 60] }) = decode_map_packet(&init) else { panic!() };
    let change = [&ZC_EL_PAR_CHANGE.to_le_bytes()[..], &MERCENARY_SP.to_le_bytes(), &10u32.to_le_bytes()].concat();
    let Some(MapEvent::ElementalChanged { kind: MERCENARY_SP, value: 10 }) = decode_map_packet(&change) else { panic!() };
    assert_eq!([init.len(), change.len()].map(|length| Some(length as i32)), [ZC_EL_INIT, ZC_EL_PAR_CHANGE].map(packet_length));
}

#[test]
fn decodes_homunculi() {
    let state = |state: u8, value: u32| [&ZC_CHANGESTATE_MER.to_le_bytes()[..], &[0, state], &7u32.to_le_bytes(), &value.to_le_bytes()].concat();
    let Some(MapEvent::HomunculusState { state: 0, id: 7, value: 0 }) = decode_map_packet(&state(0, 0)) else { panic!() };
    let Some(MapEvent::HomunculusState { state: 2, id: 7, value: 60 }) = decode_map_packet(&state(2, 60)) else { panic!() };

    // clif_hominfo, PACKETVER 20220406.
    let property = |flags: u8| {
        let mut packet = ZC_PROPERTY_HOMUN.to_le_bytes().to_vec();
        packet.extend_from_slice(&name_field("Lif"));
        packet.push(flags);
        for value in [99u16, 60, 250, 300, 310, 320, 330, 340, 350, 360, 370] {
            packet.extend_from_slice(&value.to_le_bytes());
        }
        for value in [4000u32, 5000, 600, 700] {
            packet.extend_from_slice(&value.to_le_bytes());
        }
        packet.extend_from_slice(&5_000_000_000i64.to_le_bytes());
        packet.extend_from_slice(&9_000_000_000i64.to_le_bytes());
        packet.extend_from_slice(&8u16.to_le_bytes());
        packet.extend_from_slice(&2u16.to_le_bytes());
        packet
    };
    assert_eq!(property(0b101).len(), 85);
    let Some(MapEvent::HomunculusInfo(mut homunculus)) = decode_map_packet(&property(0b101)) else { panic!() };
    assert_eq!(
        homunculus,
        Homunculus {
            name: "Lif".into(),
            flags: 0b101,
            level: 99,
            hunger: 60,
            intimacy: 250,
            atk: 300,
            matk: 310,
            hit: 320,
            critical: 330,
            def: 340,
            mdef: 350,
            flee: 360,
            aspd: 370,
            hp: 4000,
            max_hp: 5000,
            sp: 600,
            max_sp: 700,
            exp: 5_000_000_000,
            next_exp: 9_000_000_000,
            skill_points: 8,
            range: 2,
        }
    );
    assert!(homunculus.renamed() && homunculus.alive() && !homunculus.vaporized());
    let Some(MapEvent::HomunculusInfo(vaporized)) = decode_map_packet(&property(0b010)) else { panic!() };
    assert!(!vaporized.renamed() && vaporized.vaporized() && !vaporized.alive());

    let change = |kind: u16, value: u64| [&ZC_HO_PAR_CHANGE2.to_le_bytes()[..], &kind.to_le_bytes(), &value.to_le_bytes()].concat();
    let Some(MapEvent::HomunculusChanged { kind, value }) = decode_map_packet(&change(HOMUNCULUS_HP, 99)) else { panic!() };
    assert_eq!((kind, value), (HOMUNCULUS_HP, 99));
    homunculus.set(kind, value);
    homunculus.set(HOMUNCULUS_EXP, 6_000_000_000);
    homunculus.set(HOMUNCULUS_SP, u64::MAX);
    assert_eq!((homunculus.hp, homunculus.exp, homunculus.sp), (99, 6_000_000_000, u32::MAX));
    let before = homunculus.clone();
    homunculus.set(HOMUNCULUS_HP + 1, 5);
    homunculus.set(0x7FFF, 5);
    assert_eq!(homunculus, before);

    let mut skills = ZC_HOSKILLINFO_LIST.to_le_bytes().to_vec();
    skills.extend_from_slice(&41u16.to_le_bytes());
    for (id, level, upgradable) in [(8001u16, 3u16, 1u8), (8002, 1, 0)] {
        let mut entry = vec![0u8; SKILL_SIZE];
        entry[..2].copy_from_slice(&id.to_le_bytes());
        entry[6..8].copy_from_slice(&level.to_le_bytes());
        entry[36] = upgradable;
        skills.extend_from_slice(&entry);
    }
    let length = skills.len() as u16;
    skills[2..4].copy_from_slice(&length.to_le_bytes());
    let Some(MapEvent::HomunculusSkills(list)) = decode_map_packet(&skills) else { panic!() };
    assert_eq!(list.iter().map(|skill| (skill.id, skill.level, skill.upgradable)).collect::<Vec<_>>(), [(8001, 3, true), (8002, 1, false)]);

    let update =
        [&ZC_HOSKILLINFO_UPDATE.to_le_bytes()[..], &8001u16.to_le_bytes(), &4u16.to_le_bytes(), &30u16.to_le_bytes(), &9u16.to_le_bytes(), &[1]].concat();
    let Some(MapEvent::HomunculusSkill(skill)) = decode_map_packet(&update) else { panic!() };
    assert_eq!(skill, Skill { id: 8001, kind: 0, level: 4, sp: 30, range: 9, upgradable: true });

    let fed = |ok: u8| [&ZC_FEED_MER.to_le_bytes()[..], &[ok], &531u32.to_le_bytes()].concat();
    let Some(MapEvent::HomunculusFed { ok: true, food: 531 }) = decode_map_packet(&fed(1)) else { panic!() };
    let Some(MapEvent::HomunculusFed { ok: false, food: 531 }) = decode_map_packet(&fed(0)) else { panic!() };

    assert_eq!(encode_homunculus_command(HOMUNCULUS_FEED), [0x2D, 0x02, 0, 0, 1]);
    let rename = encode_rename_homunculus("Lif");
    assert_eq!((&rename[..2], &rename[2..6]), (&[0x31, 0x02][..], &b"Lif\0"[..]));
    // Never past 23 bytes, nor half a character.
    assert_eq!(encode_rename_homunculus(&"a".repeat(30))[25], 0);
    assert_eq!(&encode_rename_homunculus(&format!("{}가", "a".repeat(22)))[24..], [0, 0]);

    let ids =
        [ZC_CHANGESTATE_MER, ZC_PROPERTY_HOMUN, ZC_HO_PAR_CHANGE2, ZC_HOSKILLINFO_LIST, ZC_HOSKILLINFO_UPDATE, ZC_FEED_MER, CZ_COMMAND_MER, CZ_RENAME_MER];
    assert_eq!(ids.map(packet_length), [Some(12), Some(85), Some(12), Some(-1), Some(11), Some(7), Some(5), Some(26)]);
}

#[test]
fn decodes_pets() {
    let Some(MapEvent::PetEggs(eggs)) = decode_map_packet(&[0xA6, 0x01, 8, 0, 3, 0, 9, 0]) else { panic!() };
    assert_eq!(eggs, [3, 9]);
    let state = [&ZC_CHANGESTATE_PET.to_le_bytes()[..], &[PET_ACCESSORY], &7u32.to_le_bytes(), &10013i32.to_le_bytes()].concat();
    let Some(MapEvent::PetState { kind: PET_ACCESSORY, id: 7, value: 10013 }) = decode_map_packet(&state) else { panic!() };
    let mut property = ZC_PROPERTY_PET.to_le_bytes().to_vec();
    property.extend_from_slice(&name_field("Poring"));
    property.push(1);
    for value in [1u16, 100, 250, 10013, 1002] {
        property.extend_from_slice(&value.to_le_bytes());
    }
    let Some(MapEvent::PetInfo(pet)) = decode_map_packet(&property) else { panic!() };
    assert_eq!(pet, Pet { name: "Poring".into(), renamed: true, level: 1, hunger: 100, intimacy: 250, accessory: 10013, monster: 1002 });
    let fed = [&ZC_FEED_PET.to_le_bytes()[..], &[1], &531u32.to_le_bytes()].concat();
    let Some(MapEvent::PetFed { ok: true, food: 531 }) = decode_map_packet(&fed) else { panic!() };
    let mut named = ZC_ACK_REQNAMEALL_NPC.to_le_bytes().to_vec();
    named.extend_from_slice(&7u32.to_le_bytes());
    named.extend_from_slice(&0u32.to_le_bytes());
    named.extend_from_slice(&name_field("Pori"));
    named.extend_from_slice(&[0; 24]);
    let Some(MapEvent::UnitName { id: 7, name }) = decode_map_packet(&named) else { panic!() };
    assert_eq!(name, "Pori");

    // Below 100 an emotion; else the monster, the act and the hunger band.
    let act = [&ZC_PET_ACT.to_le_bytes()[..], &7u32.to_le_bytes(), &1_002_440i32.to_le_bytes()].concat();
    let Some(MapEvent::PetAct { id: 7, act }) = decode_map_packet(&act) else { panic!() };
    assert_eq!(act, PetAct::Talk { monster: 1002, act: 4, hunger: 4 });
    // The last digit: 2 an emotion, 0 or 1 a talk.
    assert_eq!(PetAct::decode(52), PetAct::Emotion(5));
    assert_eq!(PetAct::decode(1_002_441), PetAct::Talk { monster: 1002, act: 4, hunger: 4 });
    assert_eq!(PetAct::Talk { monster: 1002, act: 9, hunger: 4 }.value(), 1_002_940);
    assert_eq!(PetAct::Emotion(5).value(), 52);

    assert_eq!(encode_hatch(3), [0xA7, 0x01, 3, 0]);
    assert_eq!(encode_pet_command(PET_RETURN), [0xA1, 0x01, 3]);
    assert_eq!(encode_pet_act(&act), [0xA9, 0x01, 0xC8, 0x4B, 0x0F, 0]);
    let rename = encode_rename_pet("Pori");
    assert_eq!((&rename[..2], &rename[2..7]), (&[0xA5, 0x01][..], &b"Pori\0"[..]));
    // Never past 23 bytes, nor half a character.
    assert_eq!(encode_rename_pet(&"a".repeat(30))[25], 0);
    assert_eq!(&encode_rename_pet(&format!("{}가", "a".repeat(22)))[24..], [0, 0]);
    let ids = [
        ZC_PETEGG_LIST,
        CZ_SELECT_PETEGG,
        ZC_CHANGESTATE_PET,
        ZC_PROPERTY_PET,
        ZC_FEED_PET,
        CZ_COMMAND_PET,
        CZ_RENAME_PET,
        ZC_PET_ACT,
        CZ_PET_ACT,
        ZC_ACK_REQNAMEALL_NPC,
    ];
    assert_eq!(ids.map(packet_length), [Some(-1), Some(4), Some(11), Some(37), Some(7), Some(3), Some(26), Some(10), Some(6), Some(58)]);
}

#[test]
fn decodes_taming() {
    let Some(MapEvent::CaptureStart) = decode_map_packet(&[0x9E, 0x01]) else { panic!() };
    let Some(MapEvent::CaptureResult(true)) = decode_map_packet(&[0xA0, 0x01, 1]) else { panic!() };
    let Some(MapEvent::CaptureResult(false)) = decode_map_packet(&[0xA0, 0x01, 0]) else { panic!() };
    assert_eq!(encode_try_capture(110_000_123), [0x9F, 0x01, 0xFB, 0x77, 0x8E, 0x06]);
    assert_eq!([ZC_START_CAPTURE, CZ_TRYCAPTURE_MONSTER, ZC_TRYCAPTURE_MONSTER].map(packet_length), [Some(2), Some(6), Some(3)]);
}

#[test]
fn decodes_material_window() {
    let Some(MapEvent::MaterialWindow(1)) = decode_map_packet(&[0xE3, 0x07, 1, 0, 0, 0]) else { panic!() };
    assert_eq!(encode_materials(1, true, &[(5, 10), (7, 3)]), [0xE4, 0x07, 20, 0, 1, 0, 0, 0, 1, 0, 0, 0, 5, 0, 10, 0, 7, 0, 3, 0]);
    assert_eq!(encode_materials(0, false, &[]), [0xE4, 0x07, 12, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!([ZC_ITEMLISTWIN_OPEN, CZ_ITEMLISTWIN_RES].map(packet_length), [Some(6), Some(-1)]);
}

#[test]
fn decodes_achievements() {
    let entry = |id: u32, completed: bool, count: u32, at: u32, rewarded: bool| {
        let mut entry = id.to_le_bytes().to_vec();
        entry.push(u8::from(completed));
        entry.extend_from_slice(&count.to_le_bytes());
        entry.extend_from_slice(&[0; 36]);
        entry.extend_from_slice(&at.to_le_bytes());
        entry.push(u8::from(rewarded));
        entry
    };
    let totals = [&120u32.to_le_bytes()[..], &3u16.to_le_bytes(), &20u32.to_le_bytes(), &100u32.to_le_bytes()].concat();
    let mut list = ZC_ALL_ACH_LIST.to_le_bytes().to_vec();
    list.extend_from_slice(&122u16.to_le_bytes());
    list.extend_from_slice(&2u32.to_le_bytes());
    list.extend_from_slice(&totals);
    list.extend_from_slice(&entry(200005, true, 1, 1_700_000_000, false));
    list.extend_from_slice(&entry(120001, false, 7, 0, false));
    let Some(MapEvent::Achievements { totals: listed, list }) = decode_map_packet(&list) else { panic!() };
    assert_eq!(listed, AchievementTotals { score: 120, level: 3, points: 20, needed: 100 });
    assert_eq!((list[0].id, list[0].completed, list[0].counts[0], list[0].completed_at, list[0].rewarded), (200005, true, 1, 1_700_000_000, false));
    assert_eq!((list[1].id, list[1].completed, list[1].counts[0]), (120001, false, 7));

    let update = [&ZC_ACH_UPDATE.to_le_bytes()[..], &totals, &entry(200005, true, 1, 5, true)].concat();
    let Some(MapEvent::AchievementUpdate { achievement: Some(achievement), .. }) = decode_map_packet(&update) else { panic!() };
    assert_eq!((achievement.id, achievement.rewarded), (200005, true));
    // ID 0: the totals only.
    let totals_only = [&ZC_ACH_UPDATE.to_le_bytes()[..], &totals, &entry(0, false, 0, 0, false)].concat();
    let Some(MapEvent::AchievementUpdate { totals: updated, achievement: None }) = decode_map_packet(&totals_only) else { panic!() };
    assert_eq!(updated.needed, 100);

    let reward = [&ZC_REQ_ACH_REWARD_ACK.to_le_bytes()[..], &[1], &200005u32.to_le_bytes()].concat();
    let Some(MapEvent::AchievementReward { ok: true, id: 200005 }) = decode_map_packet(&reward) else { panic!() };
    let title = [&ZC_ACK_CHANGE_TITLE.to_le_bytes()[..], &[0], &1000u32.to_le_bytes()].concat();
    let Some(MapEvent::TitleChanged { ok: true, title: 1000 }) = decode_map_packet(&title) else { panic!() };
    let mut name = ZC_ACK_REQNAMEALL.to_le_bytes().to_vec();
    name.extend_from_slice(&7u32.to_le_bytes());
    name.extend_from_slice(&[0; 96]);
    name.extend_from_slice(&1000i32.to_le_bytes());
    let Some(MapEvent::NameTag(7, tag)) = decode_map_packet(&name) else { panic!() };
    assert_eq!(tag.title, 1000);

    assert_eq!(encode_id(CZ_REQ_ACH_REWARD, 200005), [0x25, 0x0A, 0x45, 0x0D, 0x03, 0]);
    assert_eq!(encode_id(CZ_REQ_CHANGE_TITLE, 0), [0x2E, 0x0A, 0, 0, 0, 0]);
    let ids = [ZC_ALL_ACH_LIST, ZC_ACH_UPDATE, CZ_REQ_ACH_REWARD, ZC_REQ_ACH_REWARD_ACK, CZ_REQ_CHANGE_TITLE, ZC_ACK_CHANGE_TITLE];
    assert_eq!(ids.map(packet_length), [Some(-1), Some(66), Some(6), Some(7), Some(6), Some(7)]);
}

#[test]
fn decodes_cooldowns() {
    let Some(MapEvent::SkillCooldown { skill: 8, ms: 10_000 }) = decode_map_packet(&[0x3D, 0x04, 8, 0, 0x10, 0x27, 0, 0]) else { panic!() };
    let Some(MapEvent::SkillCooldown { skill: 8, ms: 0 }) = decode_map_packet(&[0x3D, 0x04, 8, 0, 0, 0, 0, 0]) else { panic!() };
    assert_eq!(packet_length(ZC_SKILL_POSTDELAY), Some(8));
}

#[test]
fn decodes_the_roulette() {
    let points = [&3i32.to_le_bytes()[..], &2i32.to_le_bytes(), &1i32.to_le_bytes()].concat();
    let open = |row: i8, column: i8| {
        [&ZC_ACK_OPEN_ROULETTE.to_le_bytes()[..], &[0], &0u32.to_le_bytes(), &[row as u8, column as u8], &5u32.to_le_bytes(), &points].concat()
    };
    let Some(MapEvent::RouletteOpen { prize: Some((2, 4)), bonus: 5, points: [3, 2, 1] }) = decode_map_packet(&open(2, 4)) else { panic!() };
    let Some(MapEvent::RouletteOpen { prize: None, .. }) = decode_map_packet(&open(0, -1)) else { panic!() };
    let Some(MapEvent::RouletteOpen { prize: None, .. }) = decode_map_packet(&open(-1, 0)) else { panic!() };

    // Rows of 9 down to 3: 42 prizes.
    let mut info = ZC_ACK_ROULETTE_INFO.to_le_bytes().to_vec();
    info.extend_from_slice(&512u16.to_le_bytes());
    info.extend_from_slice(&1u32.to_le_bytes());
    for row in 0..7u16 {
        for column in 0..9 - row {
            let item = 500 + u32::from(row * 10 + column);
            info.extend_from_slice(&[&row.to_le_bytes()[..], &column.to_le_bytes(), &item.to_le_bytes(), &1u16.to_le_bytes(), &0u16.to_le_bytes()].concat());
        }
    }
    let Some(MapEvent::RoulettePrizes(prizes)) = decode_map_packet(&info) else { panic!() };
    assert_eq!(prizes.len(), 42);
    assert_eq!(prizes[0], RoulettePrize { row: 0, column: 0, item: 500, amount: 1 });
    assert_eq!((prizes[41].row, prizes[41].column, prizes[41].item), (6, 2, 562));

    let spun =
        |result: u8| [&ZC_ACK_GENERATE_ROULETTE.to_le_bytes()[..], &[result], &0u16.to_le_bytes(), &4u16.to_le_bytes(), &5u32.to_le_bytes(), &points].concat();
    let Some(MapEvent::RouletteSpun { result: 0, row: 0, column: 4, bonus: 5, points: [3, 2, 1] }) = decode_map_packet(&spun(0)) else { panic!() };
    let Some(MapEvent::RouletteSpun { result: 2, .. }) = decode_map_packet(&spun(2)) else { panic!() };
    let Some(MapEvent::RouletteSpun { result: 3, .. }) = decode_map_packet(&spun(3)) else { panic!() };
    let claimed = [&ZC_RECV_ROULETTE_ITEM.to_le_bytes()[..], &[0], &7u32.to_le_bytes()].concat();
    let Some(MapEvent::RouletteClaimed { result: 0, bonus: 7 }) = decode_map_packet(&claimed) else { panic!() };

    assert_eq!(encode_roulette(RouletteRequest::Open), [0x19, 0x0A]);
    assert_eq!(encode_roulette(RouletteRequest::Prizes), [0x1B, 0x0A]);
    assert_eq!(encode_roulette(RouletteRequest::Close), [0x1D, 0x0A]);
    assert_eq!(encode_roulette(RouletteRequest::Spin), [0x1F, 0x0A]);
    assert_eq!(encode_roulette(RouletteRequest::Claim), [0x21, 0x0A, 0]);
    // The open answer is 25 bytes at this version (the bonus item is 4 bytes), not packet_db's 23.
    let ids = [
        CZ_REQ_OPEN_ROULETTE,
        ZC_ACK_OPEN_ROULETTE,
        CZ_REQ_ROULETTE_INFO,
        ZC_ACK_ROULETTE_INFO,
        CZ_REQ_CLOSE_ROULETTE,
        CZ_REQ_GENERATE_ROULETTE,
        ZC_ACK_GENERATE_ROULETTE,
        CZ_RECV_ROULETTE_ITEM,
        ZC_RECV_ROULETTE_ITEM,
    ];
    assert_eq!(ids.map(packet_length), [Some(2), Some(25), Some(2), Some(-1), Some(2), Some(2), Some(23), Some(3), Some(7)]);
    assert_eq!(open(0, -1).len(), 25);
}

#[test]
fn decodes_equipment_states() {
    // EQUIPITEM_INFO: bind at 35, flags at 67 (1 identified, 2 broken).
    let mut entry = vec![0u8; EQUIP_ITEM_SIZE];
    entry[..2].copy_from_slice(&5u16.to_le_bytes());
    entry[2..6].copy_from_slice(&1201u32.to_le_bytes());
    entry[35..37].copy_from_slice(&2u16.to_le_bytes());
    entry[65] = 9;
    entry[66] = 4;
    entry[67] = 3;
    let item = decode_equip_item(&entry);
    assert_eq!((item.index, item.id, item.identified, item.broken, item.bind, item.refine, item.grade), (5, 1201, true, true, 2, 9, 4));
    entry[67] = 1;
    assert!(!decode_equip_item(&entry).broken);
    // ZC_ITEM_PICKUP_ACK: broken at 11, bind at 38.
    let mut pickup = vec![0u8; 70];
    pickup[..2].copy_from_slice(&ZC_ITEM_PICKUP_ACK.to_le_bytes());
    pickup[6..10].copy_from_slice(&2301u32.to_le_bytes());
    pickup[11] = 1;
    pickup[38..40].copy_from_slice(&1u16.to_le_bytes());
    pickup[68] = 7;
    pickup[69] = 2;
    let Some(MapEvent::PickedUp { item, .. }) = decode_map_packet(&pickup) else { panic!() };
    assert_eq!((item.id, item.broken, item.bind, item.refine, item.grade), (2301, true, 1, 7, 2));
    let Some(MapEvent::BoundOnEquip(7)) = decode_map_packet(&[0xD3, 0x02, 7, 0]) else { panic!() };
    let Some(MapEvent::TookOffAll { refused: false }) = decode_map_packet(&[0xAE, 0x0B, 0]) else { panic!() };
    let Some(MapEvent::TookOffAll { refused: true }) = decode_map_packet(&[0xAE, 0x0B, 1]) else { panic!() };
    assert_eq!(CZ_REQ_TAKEOFF_EQUIP_ALL.to_le_bytes(), [0xAD, 0x0B]);
    let ids = [ZC_NOTIFY_BIND_ON_EQUIP, CZ_REQ_TAKEOFF_EQUIP_ALL, ZC_ACK_TAKEOFF_EQUIP_ALL, ZC_ITEM_PICKUP_ACK];
    assert_eq!(ids.map(packet_length), [Some(4), Some(2), Some(3), Some(70)]);
}

#[test]
fn decodes_combos_locks_and_slides() {
    let combo = [&ZC_COMBODELAY.to_le_bytes()[..], &2000010u32.to_le_bytes(), &700u32.to_le_bytes()].concat();
    let Some(MapEvent::ComboWindow { id: 2000010, ms: 700 }) = decode_map_packet(&combo) else { panic!() };
    let lock = |on: u32| [&ZC_BLADESTOP.to_le_bytes()[..], &2000010u32.to_le_bytes(), &110000u32.to_le_bytes(), &on.to_le_bytes()].concat();
    let Some(MapEvent::BladeStop { source: 2000010, target: 110000, on: true }) = decode_map_packet(&lock(1)) else { panic!() };
    let Some(MapEvent::BladeStop { on: false, .. }) = decode_map_packet(&lock(0)) else { panic!() };
    let moved = |header: u16| [&header.to_le_bytes()[..], &2000010u32.to_le_bytes(), &170u16.to_le_bytes(), &370u16.to_le_bytes()].concat();
    let Some(MapEvent::Knocked { id: 2000010, position: [170, 370] }) = decode_map_packet(&moved(ZC_HIGHJUMP)) else { panic!() };
    let Some(MapEvent::Slide { id: 2000010, position: [170, 370] }) = decode_map_packet(&moved(ZC_FASTMOVE)) else { panic!() };
    assert_eq!([ZC_COMBODELAY, ZC_BLADESTOP, ZC_HIGHJUMP, ZC_FASTMOVE].map(packet_length), [Some(10), Some(14), Some(10), Some(10)]);
}

#[test]
fn speaks_of_allies_and_antagonists() {
    let entry = |relation: u32, id: u32, name: &str| [&relation.to_le_bytes()[..], &id.to_le_bytes(), &name_field(name)].concat();
    let list = [&ZC_MYGUILD_BASIC_INFO.to_le_bytes()[..], &68u16.to_le_bytes(), &entry(0, 7, "RelTestB"), &entry(1, 9, "Crabs")].concat();
    let Some(MapEvent::GuildRelations(relations)) = decode_map_packet(&list) else { panic!() };
    assert_eq!(
        relations,
        [GuildRelation { antagonist: false, id: 7, name: "RelTestB".into() }, GuildRelation { antagonist: true, id: 9, name: "Crabs".into() }]
    );
    let Some(MapEvent::GuildRelations(none)) = decode_map_packet(&[0x4C, 0x01, 4, 0]) else { panic!() };
    assert!(none.is_empty());
    let asked = [&ZC_REQ_ALLY_GUILD.to_le_bytes()[..], &2_000_011u32.to_le_bytes(), &name_field("RelTestA")].concat();
    let Some(MapEvent::AllyAsked { id: 2_000_011, guild }) = decode_map_packet(&asked) else { panic!() };
    assert_eq!(guild, "RelTestA");
    let Some(MapEvent::AllyResult(2)) = decode_map_packet(&[0x73, 0x01, 2]) else { panic!() };
    let Some(MapEvent::AntagonistResult(0)) = decode_map_packet(&[0x81, 0x01, 0]) else { panic!() };
    let Some(MapEvent::GuildRelationEnded { id: 7, antagonist: true }) = decode_map_packet(&[0x84, 0x01, 7, 0, 0, 0, 1, 0, 0, 0]) else { panic!() };
    let Some(MapEvent::GuildDisbanded(2)) = decode_map_packet(&[0x5E, 0x01, 2, 0, 0, 0]) else { panic!() };
    assert_eq!(encode_ally_request(5, 6, 7), [0x70, 0x01, 5, 0, 0, 0, 6, 0, 0, 0, 7, 0, 0, 0]);
    assert_eq!(encode_pair(CZ_ALLY_GUILD, 5, 1), [0x72, 0x01, 5, 0, 0, 0, 1, 0, 0, 0]);
    assert_eq!(encode_pair(CZ_ALLY_GUILD, 5, 0)[6..], [0, 0, 0, 0]);
    assert_eq!(encode_id(CZ_REQ_HOSTILE_GUILD, 5), [0x80, 0x01, 5, 0, 0, 0]);
    assert_eq!(encode_pair(CZ_REQ_DELETE_RELATED_GUILD, 7, 1), [0x83, 0x01, 7, 0, 0, 0, 1, 0, 0, 0]);
    let disband = encode_disband("RelTestA");
    assert_eq!((disband.len(), &disband[..10], disband[10]), (42, &[0x5D, 0x01, b'R', b'e', b'l', b'T', b'e', b's', b't', b'A'][..], 0));
    let ids =
        [ZC_MYGUILD_BASIC_INFO, ZC_REQ_ALLY_GUILD, ZC_ACK_REQ_ALLY_GUILD, ZC_ACK_REQ_HOSTILE_GUILD, ZC_DELETE_RELATED_GUILD, ZC_ACK_DISORGANIZE_GUILD_RESULT];
    assert_eq!(ids.map(packet_length), [Some(-1), Some(30), Some(3), Some(3), Some(10), Some(6)]);
    let ids = [CZ_REQ_ALLY_GUILD, CZ_ALLY_GUILD, CZ_REQ_HOSTILE_GUILD, CZ_REQ_DELETE_RELATED_GUILD, CZ_REQ_DISORGANIZE_GUILD];
    assert_eq!(ids.map(packet_length), [Some(14), Some(10), Some(6), Some(10), Some(42)]);
}

#[test]
fn merges_items() {
    let open = [&ZC_MERGE_ITEM_OPEN.to_le_bytes()[..], &10u16.to_le_bytes(), &[2, 0, 5, 0, 9, 0]].concat();
    let Some(MapEvent::MergeOpen(indexes)) = decode_map_packet(&open) else { panic!() };
    assert_eq!(indexes, [2, 5, 9]);
    let Some(MapEvent::Merged { index: 5, amount: 7, result: 0 }) = decode_map_packet(&[0x6F, 0x09, 5, 0, 7, 0, 0]) else { panic!() };
    let Some(MapEvent::Merged { index: 0, amount: 0, result: 2 }) = decode_map_packet(&[0x6F, 0x09, 0, 0, 0, 0, 2]) else { panic!() };
    assert_eq!(encode_merge_items(&[5, 2]), [0x6E, 0x09, 8, 0, 5, 0, 2, 0]);
    assert_eq!(CZ_CANCEL_MERGE_ITEM.to_le_bytes(), [0x74, 0x09]);
    let ids = [ZC_MERGE_ITEM_OPEN, CZ_REQ_MERGE_ITEM, ZC_ACK_MERGE_ITEM, CZ_CANCEL_MERGE_ITEM];
    assert_eq!(ids.map(packet_length), [Some(-1), Some(-1), Some(7), Some(2)]);
}

#[test]
fn decodes_swap_equipment() {
    let list = [&ZC_EQUIPSWITCH_LIST.to_le_bytes()[..], &16u16.to_le_bytes(), &[7, 0, 2, 0, 0, 0], &[9, 0, 0x10, 0, 0, 0]].concat();
    let Some(MapEvent::SwitchSet(set)) = decode_map_packet(&list) else { panic!() };
    assert_eq!(set, [(7, 2), (9, 0x10)]);
    let added = [&ZC_EQUIPSWITCH_ADDED.to_le_bytes()[..], &7u16.to_le_bytes(), &2u32.to_le_bytes(), &1u16.to_le_bytes()].concat();
    let Some(MapEvent::SwitchAdded { index: 7, places: 2, result: 1 }) = decode_map_packet(&added) else { panic!() };
    let removed = [&ZC_EQUIPSWITCH_REMOVED.to_le_bytes()[..], &7u16.to_le_bytes(), &2u32.to_le_bytes(), &0u16.to_le_bytes()].concat();
    let Some(MapEvent::SwitchRemoved { index: 7, places: 2 }) = decode_map_packet(&removed) else { panic!() };
    let Some(MapEvent::Switched { failed: false }) = decode_map_packet(&[0x9D, 0x0A, 0, 0]) else { panic!() };
    assert_eq!(encode_switch_add(7, 2), [0x97, 0x0A, 7, 0, 2, 0, 0, 0]);
    assert_eq!(encode_index(CZ_REQ_EQUIPSWITCH_REMOVE, 7), [0x99, 0x0A, 7, 0]);
    assert_eq!(CZ_REQ_EQUIPSWITCH.to_le_bytes(), [0x9C, 0x0A]);
    let ids = [
        CZ_REQ_EQUIPSWITCH_ADD,
        ZC_EQUIPSWITCH_ADDED,
        CZ_REQ_EQUIPSWITCH_REMOVE,
        ZC_EQUIPSWITCH_REMOVED,
        ZC_EQUIPSWITCH_LIST,
        CZ_REQ_EQUIPSWITCH,
        ZC_EQUIPSWITCH_REPLY,
    ];
    assert_eq!(ids.map(packet_length), [Some(8), Some(10), Some(4), Some(10), Some(-1), Some(2), Some(4)]);
}

#[test]
fn decodes_rentals() {
    let time = [&ZC_CASH_TIME_COUNTER.to_le_bytes()[..], &5856u32.to_le_bytes(), &3599u32.to_le_bytes()].concat();
    let Some(MapEvent::RentalTime { item: 5856, seconds: 3599 }) = decode_map_packet(&time) else { panic!() };
    let gone = [&ZC_CASH_ITEM_DELETE.to_le_bytes()[..], &4u16.to_le_bytes(), &5856u32.to_le_bytes()].concat();
    let Some(MapEvent::RentalExpired { index: 4, item: 5856 }) = decode_map_packet(&gone) else { panic!() };
    // The end time: at 29 of a NORMALITEM_INFO entry, at 31 of an EQUIPITEM_INFO entry, at 34 of a pickup.
    let mut normal = vec![0u8; NORMAL_ITEM_SIZE];
    normal[29..33].copy_from_slice(&1_800_000_000u32.to_le_bytes());
    assert_eq!(decode_normal_item(&normal).expires, 1_800_000_000);
    let mut equip = vec![0u8; EQUIP_ITEM_SIZE];
    equip[31..35].copy_from_slice(&1_800_000_001u32.to_le_bytes());
    assert_eq!(decode_equip_item(&equip).expires, 1_800_000_001);
    let mut pickup = vec![0u8; 70];
    pickup[..2].copy_from_slice(&ZC_ITEM_PICKUP_ACK.to_le_bytes());
    pickup[34..38].copy_from_slice(&1_800_000_002u32.to_le_bytes());
    let Some(MapEvent::PickedUp { item, .. }) = decode_map_packet(&pickup) else { panic!() };
    assert_eq!(item.expires, 1_800_000_002);
    assert_eq!([ZC_CASH_TIME_COUNTER, ZC_CASH_ITEM_DELETE].map(packet_length), [Some(10), Some(8)]);
}

#[test]
fn decodes_sense_and_the_mirror() {
    let mut sense = ZC_MONSTER_INFO.to_le_bytes().to_vec();
    for value in [1002u16, 1, 1] {
        sense.extend_from_slice(&value.to_le_bytes());
    }
    sense.extend_from_slice(&55u32.to_le_bytes());
    for value in [2u16, 3, 5, 1] {
        sense.extend_from_slice(&value.to_le_bytes());
    }
    sense.extend_from_slice(&[25, 100, 90, 175, 100, 100, 100, 100, 100]);
    let Some(MapEvent::MonsterInfo(info)) = decode_map_packet(&sense) else { panic!() };
    assert_eq!(
        info,
        MonsterInfo { class: 1002, level: 1, size: 1, hp: 55, def: 2, race: 3, mdef: 5, element: 1, resists: [25, 100, 90, 175, 100, 100, 100, 100, 100] }
    );
    let boss = |kind: u8, name: &str| {
        let mut packet =
            [&ZC_BOSS_INFO.to_le_bytes()[..], &[kind], &120u32.to_le_bytes(), &80u32.to_le_bytes(), &1u16.to_le_bytes(), &5u16.to_le_bytes(), &[0; 4]].concat();
        let mut field = name.as_bytes().to_vec();
        field.resize(51, 0);
        packet.extend_from_slice(&field);
        packet
    };
    let Some(MapEvent::BossInfo { kind: 2, cell: [120, 80], name, .. }) = decode_map_packet(&boss(2, "Orc Hero")) else { panic!() };
    assert_eq!(name, "Orc Hero");
    let Some(MapEvent::BossInfo { kind: 3, hours: 1, minutes: 5, .. }) = decode_map_packet(&boss(3, "Orc Hero")) else { panic!() };
    let Some(MapEvent::BossInfo { kind: 0, name, .. }) = decode_map_packet(&boss(0, "")) else { panic!() };
    assert!(name.is_empty());
    assert_eq!([ZC_MONSTER_INFO, ZC_BOSS_INFO].map(packet_length), [Some(29), Some(70)]);
    assert_eq!(boss(1, "").len(), 70);
}

#[test]
fn encodes_text_in_the_code_page() {
    assert_eq!(encode_in(encoding_rs::WINDOWS_874, "hi สวัสดี"), [b'h', b'i', b' ', 0xCA, 0xC7, 0xD1, 0xCA, 0xB4, 0xD5]);
    // What a code page has no bytes for is a `?` each, not an HTML character reference.
    assert_eq!(encode_in(encoding_rs::EUC_KR, "가ก気"), [0xB0, 0xA1, b'?', b'?']);
    assert_eq!(encode_in(encoding_rs::UTF_8, "ก気"), "ก気".as_bytes());
}

#[test]
fn encodes_a_talkie_box() {
    let packet = encode_skill_at_with_text(125, 1, [171, 360], "hello");
    assert_eq!(packet[..10], [0x67, 0x03, 1, 0, 125, 0, 171, 0, 0x68, 0x01]);
    assert_eq!(&packet[10..16], b"hello\0");
    // At most 20 bytes, cut at a whole character: "ab" and nine Korean characters (2 bytes each), no half of the tenth.
    let korean = encode_skill_at_with_text(125, 1, [0, 0], &format!("ab{}", "가".repeat(10)));
    assert_eq!(korean[10..12], *b"ab");
    assert_eq!(encoding_rs::EUC_KR.decode(&korean[12..30]).0, "가".repeat(9));
    assert_eq!(korean[30], 0);
    let mut said = [&ZC_TALKBOX_CHATCONTENTS.to_le_bytes()[..], &77u32.to_le_bytes()].concat();
    said.extend_from_slice(b"hello");
    said.resize(27, 0);
    let Some(MapEvent::TalkieBox { unit: 77, text }) = decode_map_packet(&said) else { panic!() };
    assert_eq!(text, "hello");
    assert_eq!([CZ_USE_SKILL_TOGROUND_WITHTALKBOX2, ZC_TALKBOX_CHATCONTENTS].map(packet_length), [Some(31), Some(27)]);
}

#[test]
fn decodes_sounds_and_music() {
    let mut sound = ZC_SOUND.to_le_bytes().to_vec();
    let mut file = b"humming.wav".to_vec();
    file.resize(24, 0);
    sound.extend_from_slice(&file);
    sound.push(0);
    sound.extend_from_slice(&0u32.to_le_bytes());
    sound.extend_from_slice(&110_000_123u32.to_le_bytes());
    assert_eq!(Some(sound.len() as i32), packet_length(ZC_SOUND));
    let Some(MapEvent::Sound { file, act: 0, unit: 110_000_123 }) = decode_map_packet(&sound) else { panic!() };
    assert_eq!(file, "humming.wav");

    let mut music = ZC_PLAY_NPC_BGM.to_le_bytes().to_vec();
    let mut name = b"99".to_vec();
    name.resize(24, 0);
    music.extend_from_slice(&name);
    assert_eq!(Some(music.len() as i32), packet_length(ZC_PLAY_NPC_BGM));
    let Some(MapEvent::NpcMusic(name)) = decode_map_packet(&music) else { panic!() };
    assert_eq!(name, "99");
}

#[test]
fn decodes_warp_list() {
    // Teleport level 2: "Random" and the save point, as rathena names them (16 bytes each, ".gat" added).
    let mut list = ZC_WARPLIST.to_le_bytes().to_vec();
    list.extend_from_slice(&38u16.to_le_bytes());
    list.extend_from_slice(&26u16.to_le_bytes());
    for map in ["Random.gat", "prontera.gat"] {
        let mut field = map.as_bytes().to_vec();
        field.resize(16, 0);
        list.extend_from_slice(&field);
    }
    assert_eq!(list.len(), 38);
    let Some(MapEvent::WarpList { skill: 26, maps }) = decode_map_packet(&list) else { panic!() };
    assert_eq!(maps, ["Random.gat", "prontera.gat"]);
    assert_eq!([ZC_WARPLIST, CZ_SELECT_WARPPOINT, CZ_REMEMBER_WARPPOINT].map(packet_length), [Some(-1), Some(20), Some(2)]);
}

#[test]
fn decodes_guild_position_changes() {
    // One position as clif_guild_positionchanged sends it.
    let mut changed = ZC_ACK_CHANGE_GUILD_POSITIONINFO.to_le_bytes().to_vec();
    changed.extend_from_slice(&44u16.to_le_bytes());
    for value in [1u32, 0x11, 1, 10] {
        changed.extend_from_slice(&value.to_le_bytes());
    }
    changed.extend_from_slice(&name_field("Officer"));
    let Some(MapEvent::GuildPositionChanged(position, title)) = decode_map_packet(&changed) else { panic!() };
    assert_eq!((position, title.as_str()), (GuildPosition { id: 1, mode: 0x11, rank: 1, tax: 10 }, "Officer"));

    let mut moved = ZC_ACK_REQ_CHANGE_MEMBERS.to_le_bytes().to_vec();
    moved.extend_from_slice(&16u16.to_le_bytes());
    for value in [2_000_001u32, 150_001, 1] {
        moved.extend_from_slice(&value.to_le_bytes());
    }
    assert!(matches!(decode_map_packet(&moved), Some(MapEvent::GuildMemberPosition { account: 2_000_001, character: 150_001, position: 1 })));

    // A title under the name, and none (no guild).
    let mut titled = ZC_GUILD_POSITION.to_le_bytes().to_vec();
    titled.extend_from_slice(&32u16.to_le_bytes());
    titled.extend_from_slice(&2_000_001u32.to_le_bytes());
    titled.extend_from_slice(&name_field("Officer"));
    assert!(matches!(decode_map_packet(&titled), Some(MapEvent::PositionTitle { account: 2_000_001, title }) if title == "Officer"));
    let mut none = ZC_GUILD_POSITION.to_le_bytes().to_vec();
    none.extend_from_slice(&8u16.to_le_bytes());
    none.extend_from_slice(&2_000_001u32.to_le_bytes());
    assert!(matches!(decode_map_packet(&none), Some(MapEvent::PositionTitle { account: 2_000_001, title }) if title.is_empty()));

    // The master's requests: one position, the title padded to 24 bytes; one member.
    let request = encode_position_changes(&[(GuildPosition { id: 1, mode: 0x01, rank: 1, tax: 10 }, "Officer".into())]);
    assert_eq!(request.len(), 44);
    assert_eq!(&request[..20], &[0x61, 0x01, 44, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 10, 0, 0, 0]);
    assert_eq!(&request[20..], &name_field("Officer")[..]);
    // Three positions in one packet, each with its ranking as received.
    let three = encode_position_changes(&[
        (GuildPosition { id: 1, mode: 0x01, rank: 1, tax: 0 }, "A".into()),
        (GuildPosition { id: 2, mode: 0x11, rank: 2, tax: 0 }, "B".into()),
        (GuildPosition { id: 5, mode: 0, rank: 9, tax: 7 }, "Renamed".into()),
    ]);
    assert_eq!((three.len(), &three[2..4], &three[84..100]), (124, &[124u8, 0][..], &[5u8, 0, 0, 0, 0, 0, 0, 0, 9, 0, 0, 0, 7, 0, 0, 0][..]));
    // The notice: a 60-character subject loses its last character to the NUL.
    let notice = encode_guild_notice(3, &"s".repeat(60), "hello");
    assert_eq!(
        (notice.len(), &notice[..6], notice[6 + 58], notice[6 + 59], &notice[66..71], notice[71]),
        (186, &[0x6E, 0x01, 3, 0, 0, 0][..], b's', 0, &b"hello"[..], 0)
    );
    assert_eq!(encode_member_moves(&[(7, 8, 19)]), [0x55, 0x01, 16, 0, 7, 0, 0, 0, 8, 0, 0, 0, 19, 0, 0, 0]);
    // Two moves in one packet: 4 + 12 · 2 bytes.
    let two = encode_member_moves(&[(7, 8, 19), (9, 10, 0)]);
    assert_eq!((two.len(), &two[2..4], &two[16..28]), (28, &[28u8, 0][..], &[9u8, 0, 0, 0, 10, 0, 0, 0, 0, 0, 0, 0][..]));
    // CZ_REQ_BAN_GUILD: guild, account, character, a reason in 40 bytes (empty is fine).
    let ban = encode_guild_reason(CZ_REQ_BAN_GUILD, 3, 7, 8, "");
    assert_eq!((ban.len(), &ban[..6], ban[14]), (54, &[0x5B, 0x01, 3, 0, 0, 0][..], 0));
    let long = encode_guild_reason(CZ_REQ_LEAVE_GUILD, 3, 7, 8, &"x".repeat(60));
    assert_eq!((long.len(), long[14 + 38], long[14 + 39]), (54, b'x', 0));
}

#[test]
fn decodes_the_battleground_queue() {
    let named = |command: u16, result: Option<u8>, name: &str, tail: &[u8]| {
        let mut packet = command.to_le_bytes().to_vec();
        packet.extend(result);
        packet.extend_from_slice(&name_field(name));
        packet.extend_from_slice(tail);
        packet
    };
    let kvm = "KVM (Level 80 and up)";
    assert!(matches!(decode_map_packet(&named(ZC_ACK_ENTRY_QUEUE_APPLY, Some(1), kvm, &[])), Some(MapEvent::BattleQueue { result: 1, name }) if name == kvm));
    assert!(matches!(
        decode_map_packet(&named(ZC_NOTIFY_ENTRY_QUEUE_APPLY, None, kvm, &3u32.to_le_bytes())),
        Some(MapEvent::QueueNumber { name, number: 3 }) if name == kvm
    ));
    assert!(matches!(decode_map_packet(&named(ZC_ACK_ENTRY_QUEUE_CANCEL, Some(1), kvm, &[])), Some(MapEvent::QueueLeft { result: 1, .. })));
    assert!(matches!(decode_map_packet(&named(ZC_NOTIFY_LOBBY_ADMISSION, None, kvm, &name_field(kvm))), Some(MapEvent::QueueReady { name }) if name == kvm));
    assert!(matches!(
        decode_map_packet(&named(ZC_REPLY_ACK_LOBBY_ADMISSION, Some(1), "bat_c01", &name_field("bat_c01"))),
        Some(MapEvent::QueueAccepted { result: 1, name }) if name == "bat_c01"
    ));
    assert!(matches!(decode_map_packet(&[0x0E, 0x09]), Some(MapEvent::QueueReset)));
    for (command, length) in
        [(ZC_ACK_ENTRY_QUEUE_APPLY, 27), (ZC_NOTIFY_ENTRY_QUEUE_APPLY, 30), (ZC_NOTIFY_LOBBY_ADMISSION, 50), (ZC_REPLY_ACK_LOBBY_ADMISSION, 51)]
    {
        assert_eq!(packet_length(command), Some(length));
    }

    // A 24-character name keeps 23 bytes and a NUL, as rathena cuts it.
    let apply = encode_queue_apply(QUEUE_ALONE, "KVM (Level 59 and below)");
    assert_eq!((apply.len(), &apply[..4]), (28, &[0xD7, 0x08, 1, 0][..]));
    assert_eq!(&apply[4..], &name_field("KVM (Level 59 and below")[..]);
    assert_eq!(queue_name("KVM (Level 59 and below)"), "KVM (Level 59 and below");
    assert_eq!(encode_queue_name(CZ_REQ_ENTRY_QUEUE_CANCEL, kvm)[..2], [0xDA, 0x08]);
    assert_eq!(encode_queue_name(CZ_REQ_ENTRY_QUEUE_RANKING, kvm).len(), 26);
    let reply = encode_lobby_reply(false, kvm);
    assert_eq!((reply.len(), reply[2]), (51, 2));
}

#[test]
fn decodes_marriage_and_adoption() {
    assert!(matches!(decode_map_packet(&[0xEA, 0x01, 0x81, 0x84, 0x1E, 0x00]), Some(MapEvent::Congratulation(2_000_001))));
    let named = |command: u16, name: &str| {
        let mut packet = command.to_le_bytes().to_vec();
        packet.extend_from_slice(&name_field(name));
        packet
    };
    assert!(matches!(decode_map_packet(&named(ZC_COUPLENAME, "LaneA2")), Some(MapEvent::PartnerName(name)) if name == "LaneA2"));
    assert!(matches!(decode_map_packet(&named(ZC_COUPLENAME, "")), Some(MapEvent::PartnerName(name)) if name.is_empty()));
    assert!(matches!(decode_map_packet(&named(ZC_DIVORCE, "LaneA2")), Some(MapEvent::Divorced(name)) if name == "LaneA2"));
    let mut asked = ZC_REQ_BABY.to_le_bytes().to_vec();
    asked.extend_from_slice(&2_000_003u32.to_le_bytes());
    asked.extend_from_slice(&2_000_004u32.to_le_bytes());
    asked.extend_from_slice(&name_field("LaneA1"));
    assert!(matches!(
        decode_map_packet(&asked),
        Some(MapEvent::AdoptAsked { parent: 2_000_003, partner: 2_000_004, name }) if name == "LaneA1"
    ));
    assert!(matches!(decode_map_packet(&[0x16, 0x02, 1, 0, 0, 0]), Some(MapEvent::AdoptRefused(1))));
    assert_eq!(packet_length(ZC_BABYMSG), Some(6));

    assert_eq!(encode_adopt_request(2_000_000), [0xF9, 0x01, 0x80, 0x84, 0x1E, 0x00]);
    assert_eq!(encode_adopt_answer(7, 8, true), [0xF7, 0x01, 7, 0, 0, 0, 8, 0, 0, 0, 1, 0, 0, 0]);
    assert_eq!(encode_adopt_answer(7, 8, false)[10..], [0, 0, 0, 0]);
}

#[test]
fn decodes_battlegrounds() {
    let mut hp = ZC_BATTLEFIELD_NOTIFY_HP2.to_le_bytes().to_vec();
    for value in [2_000_001u32, 30, 40] {
        hp.extend_from_slice(&value.to_le_bytes());
    }
    assert!(matches!(decode_map_packet(&hp), Some(MapEvent::BattleHp { account: 2_000_001, hp: 30, max: 40 })));

    // A teammate's place, and the same packet when they leave (empty name, job 0, 0xFFFF).
    let position = |name: &str, job: u16, cell: [u16; 2]| {
        let mut packet = ZC_BATTLEFIELD_NOTIFY_POSITION.to_le_bytes().to_vec();
        packet.extend_from_slice(&2_000_001u32.to_le_bytes());
        packet.extend_from_slice(&name_field(name));
        for value in [job, cell[0], cell[1]] {
            packet.extend_from_slice(&value.to_le_bytes());
        }
        packet
    };
    assert!(matches!(decode_map_packet(&position("LaneA2", 4001, [62, 119])), Some(MapEvent::BattlePosition { account: 2_000_001, cell: Some([62, 119]) })));
    assert!(matches!(decode_map_packet(&position("", 0, [0xFFFF; 2])), Some(MapEvent::BattlePosition { cell: None, .. })));

    let mut camp = ZC_BATTLEFIELD_NOTIFY_CAMPINFO.to_le_bytes().to_vec();
    camp.extend_from_slice(&2_000_001u32.to_le_bytes());
    camp.extend_from_slice(&name_field("LaneA2"));
    camp.extend_from_slice(&3u16.to_le_bytes());
    assert!(matches!(decode_map_packet(&camp), Some(MapEvent::BattleCamp { account: 2_000_001, camp: 3 })));
    assert!(matches!(decode_map_packet(&[0xDE, 0x02, 3, 0, 1, 0]), Some(MapEvent::BattleScore([3, 1]))));

    // A team line as clif_bg_message writes it: length, account, name, the whole line.
    let mut chat = ZC_BATTLEFIELD_CHAT.to_le_bytes().to_vec();
    chat.extend_from_slice(&[0, 0]);
    chat.extend_from_slice(&2_000_001u32.to_le_bytes());
    chat.extend_from_slice(&name_field("LaneA2"));
    chat.extend_from_slice(b"LaneA2 : hello team\0");
    let length = chat.len() as u16;
    chat[2..4].copy_from_slice(&length.to_le_bytes());
    assert!(matches!(decode_map_packet(&chat), Some(MapEvent::BattleChat { account: 2_000_001, text }) if text == "LaneA2 : hello team"));

    assert_eq!(encode_battle_chat("Me", "hi"), [0xDB, 0x02, 12, 0, b'M', b'e', b' ', b':', b' ', b'h', b'i', 0]);
}

#[test]
fn decodes_mail() {
    // The list as clif_Mail_refreshinbox writes it: length, a byte, then per mail its box, ID, read, flags,
    // sender, seconds left, title length and title.
    let mut list = ZC_ACK_MAIL_LIST3.to_le_bytes().to_vec();
    list.extend_from_slice(&[0, 0, 1]);
    for (id, title, flags) in [(7u64, "Hello\0", MAIL_ZENY), (3, "Hi\0", 0)] {
        list.push(0);
        list.extend_from_slice(&id.to_le_bytes());
        list.extend_from_slice(&[u8::from(id == 3), flags]);
        list.extend_from_slice(&name_field("Tester2"));
        list.extend_from_slice(&86_400u32.to_le_bytes());
        list.extend_from_slice(&(title.len() as u16).to_le_bytes());
        list.extend_from_slice(title.as_bytes());
    }
    let length = list.len() as u16;
    list[2..4].copy_from_slice(&length.to_le_bytes());
    let Some(MapEvent::MailList(mails)) = decode_map_packet(&list) else { panic!() };
    assert_eq!(
        mails.iter().map(|mail| (mail.id, mail.read, mail.flags, mail.title.as_str(), mail.sender.as_str())).collect::<Vec<_>>(),
        [(7, false, MAIL_ZENY, "Hello", "Tester2"), (3, true, 0, "Hi", "Tester2")]
    );
    assert_eq!(packet_length(ZC_ACK_MAIL_LIST3), Some(-1));

    // A mail read: box, ID, text length, zeny, item count, the text, then 60-byte items.
    let mut read = ZC_ACK_READ_RODEX.to_le_bytes().to_vec();
    read.extend_from_slice(&[0, 0, 0]);
    read.extend_from_slice(&7u64.to_le_bytes());
    read.extend_from_slice(&4u16.to_le_bytes());
    read.extend_from_slice(&500u64.to_le_bytes());
    read.push(1);
    read.extend_from_slice(b"Hey\0");
    let mut item = [0u8; 60];
    item[..2].copy_from_slice(&3u16.to_le_bytes());
    item[2..6].copy_from_slice(&512u32.to_le_bytes());
    item[6] = 1;
    read.extend_from_slice(&item);
    let length = read.len() as u16;
    read[2..4].copy_from_slice(&length.to_le_bytes());
    let Some(MapEvent::MailRead { kind: 0, id: 7, text, zeny: 500, items }) = decode_map_packet(&read) else { panic!() };
    assert_eq!((text.as_str(), items.len(), items[0].id, items[0].amount, items[0].identified), ("Hey", 1, 512, 3, true));
    assert_eq!(packet_length(ZC_ACK_READ_RODEX), Some(-1));
    assert_eq!(packet_length(ZC_ACK_ADD_ITEM_RODEX), Some(64));
    assert_eq!(packet_length(ZC_CHECKNAME), Some(34));
}

#[test]
fn decodes_memorial_dungeon() {
    let name_field = |name: &str| {
        let mut field = [0u8; INSTANCE_NAME_LENGTH];
        let bytes = encode_text(name);
        let length = bytes.len().min(INSTANCE_NAME_LENGTH - 1);
        field[..length].copy_from_slice(&bytes[..length]);
        field
    };
    let mut create = ZC_INSTANCE_CREATE.to_le_bytes().to_vec();
    create.extend_from_slice(&name_field("Endless Tower"));
    create.extend_from_slice(&4u16.to_le_bytes());
    assert_eq!(create.len(), 65);
    let Some(MapEvent::InstanceCreate { name, place }) = decode_map_packet(&create) else { panic!() };
    assert_eq!((name.as_str(), place), ("Endless Tower", 4));

    let mut waiting = ZC_INSTANCE_WAIT.to_le_bytes().to_vec();
    waiting.extend_from_slice(&2u16.to_le_bytes());
    assert!(matches!(decode_map_packet(&waiting), Some(MapEvent::InstanceWait { place: 2 })));

    let mut status = ZC_INSTANCE_STATUS.to_le_bytes().to_vec();
    status.extend_from_slice(&name_field("Orc's Memory"));
    status.extend_from_slice(&1_700_000_100u32.to_le_bytes());
    status.extend_from_slice(&1_700_000_000u32.to_le_bytes());
    assert_eq!(status.len(), 71);
    let Some(MapEvent::InstanceStatus { name, time_limit, idle_limit }) = decode_map_packet(&status) else { panic!() };
    assert_eq!((name.as_str(), time_limit, idle_limit), ("Orc's Memory", 1_700_000_100, 1_700_000_000));

    let notify = |kind: u32, time: u32| {
        let mut packet = ZC_INSTANCE_NOTIFY.to_le_bytes().to_vec();
        packet.extend_from_slice(&kind.to_le_bytes());
        packet.extend_from_slice(&time.to_le_bytes());
        packet
    };
    assert!(matches!(decode_map_packet(&notify(0, 1_700_000_050)), Some(MapEvent::InstanceNotify { kind: 0, time: 1_700_000_050 })));
    assert!(matches!(decode_map_packet(&notify(4, 0)), Some(MapEvent::InstanceNotify { kind: 4, time: 0 })));

    assert_eq!(encode_memorial_command(COMMAND_MEMORIALDUNGEON_DESTROY_FORCE), [0xCF, 0x02, 3, 0, 0, 0]);
    assert_eq!(
        [ZC_INSTANCE_CREATE, ZC_INSTANCE_WAIT, ZC_INSTANCE_STATUS, ZC_INSTANCE_NOTIFY, CZ_MEMORIALDUNGEON_COMMAND].map(packet_length),
        [Some(65), Some(4), Some(71), Some(10), Some(6)]
    );
}

#[test]
fn decodes_the_stylist() {
    // ZC_UI_OPEN: kind at 2, data u32 at 3. Kind 1 is the stylist; 8 is grade enchant, decoded the same way.
    assert!(matches!(decode_map_packet(&[0xE2, 0x0A, 1, 0, 0, 0, 0]), Some(MapEvent::UiOpen { kind: 1, data: 0 })));
    assert!(matches!(decode_map_packet(&[0xE2, 0x0A, 8, 4, 0, 0, 0]), Some(MapEvent::UiOpen { kind: 8, data: 4 })));
    // ZC_STYLE_CHANGE_RES: 0 done, 1 failed.
    assert!(matches!(decode_map_packet(&[0x47, 0x0A, 0]), Some(MapEvent::StyleChanged(false))));
    assert!(matches!(decode_map_packet(&[0x47, 0x0A, 1]), Some(MapEvent::StyleChanged(true))));

    let picks = StylePicks { hair_color: 3, hair_style: 5, clothes_color: 2, head_top: 1, head_mid: 0, head_bottom: 0, body_style: 0 };
    let packet = encode_style_change(picks);
    assert_eq!(packet.len(), 16);
    // 0x0AFC, never the 14-byte 0x0A46.
    assert_eq!(&packet[..2], &[0xFC, 0x0A]);
    assert_ne!(&packet[..2], &[0x46, 0x0A]);
    let row = |offset: usize| u16::from_le_bytes([packet[offset], packet[offset + 1]]);
    assert_eq!((row(2), row(4), row(6), row(8), row(10), row(12), row(14)), (3, 5, 2, 1, 0, 0, 0));
    let body = encode_style_change(StylePicks { body_style: 2, ..StylePicks::default() });
    assert_eq!(u16::from_le_bytes([body[14], body[15]]), 2);
    assert_eq!(&body[2..14], &[0; 12]);
    assert_eq!(encode_style_close(), [0x48, 0x0A]);
    assert_eq!([ZC_UI_OPEN, ZC_STYLE_CHANGE_RES, CZ_REQ_STYLE_CHANGE2, CZ_REQ_STYLE_CLOSE].map(packet_length), [Some(7), Some(3), Some(16), Some(2)]);
}

#[test]
fn reads_a_status_value() {
    // ZC_MSG_STATE_CHANGE3: EFST_ON_PUSH_CART (673) on account 7, cart type 3.
    let mut packet = vec![0x83, 0x09, 0xA1, 0x02, 7, 0, 0, 0, 1];
    packet.extend_from_slice(&[0; 8]);
    packet.extend_from_slice(&3u32.to_le_bytes());
    packet.extend_from_slice(&[0; 8]);
    assert_eq!(packet.len(), 29);
    let Some(MapEvent::StatusIcon { kind: 673, id: 7, on: true, value: 3, .. }) = decode_map_packet(&packet) else { panic!() };
}

#[test]
fn identifies_with_a_magnifier() {
    let mut autorun = vec![0x47, 0x01, 40, 0, 4, 0, 0, 0, 1, 0, 10, 0, 1, 0];
    autorun.extend_from_slice(&[0; 24]);
    autorun.push(0);
    let Some(MapEvent::ItemSkill(skill)) = decode_map_packet(&autorun) else { panic!() };
    assert_eq!((skill.id, skill.kind, skill.level, skill.sp, skill.range, skill.upgradable), (40, Skill::SELF, 1, 10, 1, false));
    let Some(MapEvent::IdentifyList(indexes)) = decode_map_packet(&[0x77, 0x01, 8, 0, 3, 0, 9, 0]) else { panic!() };
    assert_eq!(indexes, [3, 9]);
    let Some(MapEvent::Identified { index: 9, ok: true }) = decode_map_packet(&[0x79, 0x01, 9, 0, 0]) else { panic!() };
    let Some(MapEvent::Identified { ok: false, .. }) = decode_map_packet(&[0x79, 0x01, 9, 0, 1]) else { panic!() };
    assert_eq!(encode_identify(Some(9)), [0x78, 0x01, 9, 0]);
    assert_eq!(encode_identify(None), [0x78, 0x01, 0xFF, 0xFF]);
    assert_eq!(encode_card_list(5), [0x7A, 0x01, 5, 0]);
    assert_eq!(encode_insert_card(5, 9), [0x7C, 0x01, 5, 0, 9, 0]);
    let list = [0x7B, 0x01, 8, 0, 9, 0, 12, 0];
    let Some(MapEvent::CardList(indexes)) = decode_map_packet(&list) else { panic!() };
    assert_eq!(indexes, [9, 12]);
    let Some(MapEvent::CardInserted { equip: 9, card: 5, ok: true }) = decode_map_packet(&[0x7D, 0x01, 9, 0, 5, 0, 0]) else { panic!() };
    let Some(MapEvent::CardInserted { ok: false, .. }) = decode_map_packet(&[0x7D, 0x01, 9, 0, 5, 0, 1]) else { panic!() };
    let lengths = [CZ_REQ_ITEMCOMPOSITION_LIST, ZC_ITEMCOMPOSITION_LIST, CZ_REQ_ITEMCOMPOSITION, ZC_ACK_ITEMCOMPOSITION].map(packet_length);
    assert_eq!(lengths, [Some(4), Some(-1), Some(6), Some(7)]);
    let warp = encode_select_warp_point(27, "prontera.gat");
    assert_eq!((&warp[..4], &warp[4..16], &warp[16..]), (&[0x1B, 0x01, 27, 0][..], &b"prontera.gat"[..], &[0u8; 4][..]));
    assert_eq!(&encode_select_warp_point(26, "cancel")[4..11], b"cancel\0");
    for (header, length) in [(ZC_AUTORUN_SKILL, 39), (ZC_ITEMIDENTIFY_LIST, -1), (CZ_REQ_ITEMIDENTIFY, 4), (ZC_ACK_ITEMIDENTIFY, 5)] {
        assert_eq!(packet_length(header), Some(length));
    }
}

#[test]
fn encodes_dropping_an_item() {
    assert_eq!(encode_drop_item(7, 300), [0x63, 0x03, 7, 0, 0x2C, 0x01]);
    assert_eq!(packet_length(CZ_ITEM_THROW), Some(6));
}

#[test]
fn encodes_whisper_settings_and_who() {
    assert_eq!(CZ_REQ_USER_COUNT.to_le_bytes(), [0xC1, 0x00]);
    assert_eq!(CZ_REQ_WHISPER_LIST.to_le_bytes(), [0xD3, 0x00]);
    let block = encode_whisper_setting("Poring", false);
    assert_eq!(block.len(), 27);
    assert_eq!(&block[..2], &[0xCF, 0x00]);
    assert_eq!(&block[2..9], b"Poring\0");
    assert_eq!(block[26], 0);
    let allow = encode_whisper_setting("a-name-longer-than-twenty-three", true);
    // Truncated to 23 bytes, keeping the terminating NUL.
    assert_eq!((allow[2 + 22], allow[2 + 23], allow[26]), (b'n', 0, 1));
    assert_eq!(encode_whisper_all(false), [0xD0, 0x00, 0]);
    assert_eq!(encode_whisper_all(true), [0xD0, 0x00, 1]);
    for (header, length) in [(CZ_REQ_USER_COUNT, 2), (CZ_SETTING_WHISPER_PC, 27), (CZ_SETTING_WHISPER_STATE, 3), (CZ_REQ_WHISPER_LIST, 2)] {
        assert_eq!(packet_length(header), Some(length));
    }
}

#[test]
fn decodes_who_and_whisper_settings() {
    let Some(MapEvent::UserCount(1234)) = decode_map_packet(&[0xC2, 0x00, 0xD2, 0x04, 0, 0]) else { panic!() };
    let Some(MapEvent::WhisperSetting { allow: false, result: 2 }) = decode_map_packet(&[0xD1, 0x00, 0, 2]) else { panic!() };
    let Some(MapEvent::WhisperSetting { allow: true, result: 0 }) = decode_map_packet(&[0xD1, 0x00, 1, 0]) else { panic!() };
    let Some(MapEvent::WhisperAll { allow: true, result: 1 }) = decode_map_packet(&[0xD2, 0x00, 1, 1]) else { panic!() };

    let mut list = vec![0xD4, 0x00, 0, 0];
    for name in ["Poring", "Lunatic"] {
        let mut field = [0u8; 24];
        field[..name.len()].copy_from_slice(name.as_bytes());
        list.extend_from_slice(&field);
    }
    let length = list.len() as u16;
    list[2..4].copy_from_slice(&length.to_le_bytes());
    let Some(MapEvent::WhisperList(names)) = decode_map_packet(&list) else { panic!() };
    assert_eq!(names, ["Poring", "Lunatic"]);
    let Some(MapEvent::WhisperList(names)) = decode_map_packet(&[0xD4, 0x00, 4, 0]) else { panic!() };
    assert!(names.is_empty());
    for (header, length) in [(ZC_USER_COUNT, 6), (ZC_SETTING_WHISPER_PC, 4), (ZC_SETTING_WHISPER_STATE, 4), (ZC_WHISPER_LIST, -1)] {
        assert_eq!(packet_length(header), Some(length));
    }
}

#[test]
fn encodes_buying_store_requests() {
    let open = encode_open_buying_store("Buying Jellopy", 1000, true, &[(909, 5, 10)]);
    assert_eq!(open.len(), 99);
    assert_eq!(&open[..2], &[0x11, 0x08]);
    assert_eq!(u16::from_le_bytes([open[2], open[3]]), 99);
    assert_eq!(u32::from_le_bytes(open[4..8].try_into().unwrap()), 1000);
    assert_eq!(open[8], 1);
    assert_eq!(&open[9..24], b"Buying Jellopy\0");
    assert_eq!(u32::from_le_bytes(open[89..93].try_into().unwrap()), 909);
    assert_eq!(u16::from_le_bytes([open[93], open[94]]), 5);
    assert_eq!(u32::from_le_bytes(open[95..99].try_into().unwrap()), 10);

    let cancel = encode_open_buying_store("", 0, false, &[]);
    assert_eq!(cancel.len(), 89);
    assert_eq!(u16::from_le_bytes([cancel[2], cancel[3]]), 89);
    assert_eq!(cancel[8], 0);

    assert_eq!(encode_close_buying_store(), [0x15, 0x08]);
    assert_eq!(encode_click_buying_store(2000001), [0x17, 0x08, 0x81, 0x84, 0x1E, 0x00]);

    let sell = encode_sell_to_buying_store(2000001, 42, &[(4, 909, 2)]);
    assert_eq!(sell.len(), 20);
    assert_eq!(&sell[..4], &[0x19, 0x08, 20, 0]);
    assert_eq!(u32::from_le_bytes(sell[4..8].try_into().unwrap()), 2000001);
    assert_eq!(u32::from_le_bytes(sell[8..12].try_into().unwrap()), 42);
    assert_eq!(u16::from_le_bytes([sell[12], sell[13]]), 4);
    assert_eq!(u32::from_le_bytes(sell[14..18].try_into().unwrap()), 909);
    assert_eq!(u16::from_le_bytes([sell[18], sell[19]]), 2);

    for (header, length) in [(CZ_REQ_OPEN_BUYING_STORE, -1), (CZ_REQ_CLOSE_BUYING_STORE, 2), (CZ_REQ_CLICK_TO_BUYING_STORE, 6), (CZ_REQ_TRADE_BUYING_STORE, -1)]
    {
        assert_eq!(packet_length(header), Some(length));
    }
}

#[test]
fn decodes_buying_store_packets() {
    let Some(MapEvent::OpenBuyingStore { slots: 2 }) = decode_map_packet(&[0x10, 0x08, 2]) else { panic!() };
    let Some(MapEvent::BuyingStoreOpenFailed { result: 1, weight: 500 }) = decode_map_packet(&[0x12, 0x08, 1, 0, 0xF4, 0x01, 0, 0]) else { panic!() };

    let mut my_list = vec![0x13, 0x08, 23, 0];
    my_list.extend_from_slice(&2000001u32.to_le_bytes());
    my_list.extend_from_slice(&1000u32.to_le_bytes());
    my_list.extend_from_slice(&10u32.to_le_bytes());
    my_list.extend_from_slice(&5u16.to_le_bytes());
    my_list.push(3);
    my_list.extend_from_slice(&909u32.to_le_bytes());
    let Some(MapEvent::MyBuyingStore { account: 2000001, limit: 1000, items }) = decode_map_packet(&my_list) else { panic!() };
    assert_eq!(items, [BuyingItem { price: 10, amount: 5, kind: 3, item_id: 909 }]);

    let mut entry = vec![0x14, 0x08];
    entry.extend_from_slice(&2000001u32.to_le_bytes());
    let mut title = b"Buying Jellopy".to_vec();
    title.resize(80, 0);
    entry.extend_from_slice(&title);
    let Some(MapEvent::BuyingStoreSign { owner: 2000001, title }) = decode_map_packet(&entry) else { panic!() };
    assert_eq!(title, "Buying Jellopy");

    let mut gone = vec![0x16, 0x08];
    gone.extend_from_slice(&2000001u32.to_le_bytes());
    let Some(MapEvent::BuyingStoreSignGone { owner: 2000001 }) = decode_map_packet(&gone) else { panic!() };

    let mut seller_list = vec![0x18, 0x08, 27, 0];
    seller_list.extend_from_slice(&2000001u32.to_le_bytes());
    seller_list.extend_from_slice(&42u32.to_le_bytes());
    seller_list.extend_from_slice(&1000u32.to_le_bytes());
    seller_list.extend_from_slice(&10u32.to_le_bytes());
    seller_list.extend_from_slice(&5u16.to_le_bytes());
    seller_list.push(3);
    seller_list.extend_from_slice(&909u32.to_le_bytes());
    let Some(MapEvent::BuyingStoreItems { owner: 2000001, store_id: 42, limit: 1000, items }) = decode_map_packet(&seller_list) else { panic!() };
    assert_eq!(items, [BuyingItem { price: 10, amount: 5, kind: 3, item_id: 909 }]);

    let Some(MapEvent::BuyingStoreBuyerFailed { result: 3 }) = decode_map_packet(&[0x1A, 0x08, 3, 0]) else { panic!() };

    let mut sold = vec![0x1C, 0x08];
    sold.extend_from_slice(&4u16.to_le_bytes());
    sold.extend_from_slice(&2u16.to_le_bytes());
    sold.extend_from_slice(&10u32.to_le_bytes());
    let Some(MapEvent::BuyingStoreSold { index: 4, amount: 2, price: 10 }) = decode_map_packet(&sold) else { panic!() };

    let mut seller_failed = vec![0x24, 0x08, 6, 0];
    seller_failed.extend_from_slice(&909u32.to_le_bytes());
    let Some(MapEvent::BuyingStoreSellerFailed { result: 6, item_id: 909 }) = decode_map_packet(&seller_failed) else { panic!() };

    let mut update = vec![0xE6, 0x09];
    update.extend_from_slice(&909u32.to_le_bytes());
    update.extend_from_slice(&2u16.to_le_bytes());
    update.extend_from_slice(&20u32.to_le_bytes());
    update.extend_from_slice(&980u32.to_le_bytes());
    update.extend_from_slice(&150001u32.to_le_bytes());
    update.extend_from_slice(&1_700_000_000u32.to_le_bytes());
    let Some(MapEvent::BuyingStoreUpdate { item_id: 909, amount: 2, zeny: 20, limit_left: 980, char_id: 150001, time: 1_700_000_000 }) =
        decode_map_packet(&update)
    else {
        panic!()
    };

    for (header, length) in [
        (ZC_OPEN_BUYING_STORE, 3),
        (ZC_FAILED_OPEN_BUYING_STORE_TO_BUYER, 8),
        (ZC_MYITEMLIST_BUYING_STORE, -1),
        (ZC_BUYING_STORE_ENTRY, 86),
        (ZC_DISAPPEAR_BUYING_STORE_ENTRY, 6),
        (ZC_ACK_ITEMLIST_BUYING_STORE, -1),
        (ZC_FAILED_TRADE_BUYING_STORE_TO_BUYER, 4),
        (ZC_ITEM_DELETE_BUYING_STORE, 10),
        (ZC_FAILED_TRADE_BUYING_STORE_TO_SELLER, 8),
        (ZC_UPDATE_ITEM_FROM_BUYING_STORE2, 24),
    ] {
        assert_eq!(packet_length(header), Some(length));
    }
}

#[test]
fn encodes_and_decrypts_pin() {
    // rathena's char_pincode_decrypt (src/char/char.cpp): builds the same multiplier/seed table and replaces each
    // sent button index '0' + k with '0' + tab[k].
    fn rathena_decrypt(mut seed: u32, encoded: [u8; 4]) -> [u8; 4] {
        let mut tab = [0u8, 1, 2, 3, 4, 5, 6, 7, 8, 9];
        for i in 1..10u32 {
            seed = 0x0088_1234u32.wrapping_add(seed.wrapping_mul(0x3498));
            let pos = (seed % (i + 1)) as usize;
            tab.swap(i as usize, pos);
        }
        encoded.map(|byte| b'0' + tab[usize::from(byte - b'0')])
    }

    for seed in [0, 1, 0x1234_5678, 0xDEAD_BEEF, u32::MAX] {
        for pin in ["0000", "1357", "9876", "2468", "0192"] {
            let encoded = encode_pin(seed, pin);
            assert_eq!(&rathena_decrypt(seed, encoded), pin.as_bytes());
        }
    }

    let seed = 0x1234_5678;
    let check = encode_pin_check(2_000_001, seed, "1357");
    assert_eq!(u16_at(&check, 0), 0x08B8);
    assert_eq!(u32_at(&check, 2), 2_000_001);
    assert_eq!(&rathena_decrypt(seed, check[6..10].try_into().unwrap()), b"1357");

    let make = encode_pin_new(2_000_001, seed, "2468");
    assert_eq!(u16_at(&make, 0), 0x08BA);
    assert_eq!(u32_at(&make, 2), 2_000_001);
    assert_eq!(&rathena_decrypt(seed, make[6..10].try_into().unwrap()), b"2468");

    let change = encode_pin_change(2_000_001, seed, "1357", "2468");
    assert_eq!(u16_at(&change, 0), 0x08BE);
    assert_eq!(u32_at(&change, 2), 2_000_001);
    assert_eq!(&rathena_decrypt(seed, change[6..10].try_into().unwrap()), b"1357");
    assert_eq!(&rathena_decrypt(seed, change[10..14].try_into().unwrap()), b"2468");

    for (header, length) in [(CH_SECOND_PASSWD_ACK, 10), (HC_SECOND_PASSWD_LOGIN, 12), (CH_MAKE_SECOND_PASSWD, 10), (CH_EDIT_SECOND_PASSWD, 14)] {
        assert_eq!(packet_length(header), Some(length));
    }

    let mut login_pin = [0u8; 12];
    login_pin[..2].copy_from_slice(&HC_SECOND_PASSWD_LOGIN.to_le_bytes());
    login_pin[2..6].copy_from_slice(&0xCAFE_BABEu32.to_le_bytes());
    login_pin[6..10].copy_from_slice(&2_000_001u32.to_le_bytes());
    login_pin[10..12].copy_from_slice(&1u16.to_le_bytes());
    let Some(CharEvent::Pin { seed: 0xCAFE_BABE, state: 1 }) = decode_char_packet(&login_pin) else { panic!() };
}

#[test]
fn decodes_clan() {
    let chat = encode_clan_chat("Tester", "hello clan");
    assert_eq!(&chat[..2], &CZ_CLAN_CHAT.to_le_bytes());
    assert_eq!(u16::from_le_bytes([chat[2], chat[3]]) as usize, chat.len());
    assert_eq!(&chat[4..], b"Tester : hello clan\0");

    // ZC_CLANINFO: length, clan ID, name, master, map, ally count, hostile count, then 24-byte clan names.
    let mut info = ZC_CLANINFO.to_le_bytes().to_vec();
    info.extend_from_slice(&[0, 0]);
    info.extend_from_slice(&2u32.to_le_bytes());
    info.extend_from_slice(&name_field("Arcwand Clan"));
    info.extend_from_slice(&name_field("Devon Aire"));
    let mut map = [0u8; 16];
    map[..10].copy_from_slice(b"geffen.gat");
    info.extend_from_slice(&map);
    info.extend_from_slice(&[1, 1]);
    info.extend_from_slice(&name_field("Golden Mace Clan"));
    info.extend_from_slice(&name_field("Crossbow Clan"));
    let length = info.len() as u16;
    info[2..4].copy_from_slice(&length.to_le_bytes());
    let Some(MapEvent::ClanInfo { id, name, master, map, allies, hostile }) = decode_map_packet(&info) else { panic!() };
    assert_eq!((id, name.as_str(), master.as_str(), map.as_str()), (2, "Arcwand Clan", "Devon Aire", "geffen.gat"));
    assert_eq!(allies, ["Golden Mace Clan"]);
    assert_eq!(hostile, ["Crossbow Clan"]);

    let mut count = ZC_NOTIFY_CLAN_CONNECTINFO.to_le_bytes().to_vec();
    count.extend_from_slice(&3i16.to_le_bytes());
    count.extend_from_slice(&500i16.to_le_bytes());
    let Some(MapEvent::ClanOnlineCount { online: 3, max: 500 }) = decode_map_packet(&count) else { panic!() };

    let Some(MapEvent::ClanLeft) = decode_map_packet(&ZC_ACK_CLAN_LEAVE.to_le_bytes()) else { panic!() };

    let mut incoming_chat = ZC_NOTIFY_CLAN_CHAT.to_le_bytes().to_vec();
    incoming_chat.extend_from_slice(&[0, 0]);
    incoming_chat.extend_from_slice(&[0u8; 24]);
    incoming_chat.extend_from_slice(b"Tester : hello clan\0");
    let length = incoming_chat.len() as u16;
    incoming_chat[2..4].copy_from_slice(&length.to_le_bytes());
    let Some(MapEvent::ClanChat(text)) = decode_map_packet(&incoming_chat) else { panic!() };
    assert_eq!(text, "Tester : hello clan");

    for (header, length) in [(ZC_NOTIFY_CLAN_CONNECTINFO, 6), (ZC_ACK_CLAN_LEAVE, 2), (ZC_CLANINFO, -1), (CZ_CLAN_CHAT, -1), (ZC_NOTIFY_CLAN_CHAT, -1)] {
        assert_eq!(packet_length(header), Some(length));
    }
}

#[test]
fn decodes_enchant() {
    let mut open = ZC_UI_OPEN_V3.to_le_bytes().to_vec();
    open.resize(11, 0);
    open[2] = ENCHANT_UI;
    open[3..11].copy_from_slice(&33u64.to_le_bytes());
    let Some(MapEvent::EnchantOpen(33)) = decode_map_packet(&open) else { panic!() };
    open[2] = 1;
    assert!(decode_map_packet(&open).is_none());
    let answer = [&ZC_RESPONSE_ENCHANT.to_le_bytes()[..], &3857u32.to_le_bytes(), &4818u32.to_le_bytes()].concat();
    let Some(MapEvent::Enchanted { message: 3857, item: 4818 }) = decode_map_packet(&answer) else { panic!() };
    let failed = [&ZC_RESPONSE_ENCHANT.to_le_bytes()[..], &3858u32.to_le_bytes(), &0u32.to_le_bytes()].concat();
    let Some(MapEvent::Enchanted { message: 3858, item: 0 }) = decode_map_packet(&failed) else { panic!() };
    assert_eq!(encode_enchant_random(33, 4)[0..2], CZ_REQUEST_RANDOM_ENCHANT.to_le_bytes());
    assert_eq!(u64::from_le_bytes(encode_enchant_random(33, 4)[2..10].try_into().unwrap()), 33);
    assert_eq!(u16::from_le_bytes(encode_enchant_random(33, 4)[10..12].try_into().unwrap()), 4);
    let perfect = encode_enchant_perfect(33, 4, 4818);
    assert_eq!((u16::from_le_bytes(perfect[10..12].try_into().unwrap()), u32::from_le_bytes(perfect[12..16].try_into().unwrap())), (4, 4818));
    let upgrade = encode_enchant_upgrade(33, 4, 3);
    assert_eq!(u16::from_le_bytes(upgrade[12..14].try_into().unwrap()), 3);
    assert_eq!(encode_enchant_reset(33, 4)[10..12], 4u16.to_le_bytes());
    assert_eq!(encode_enchant_close(), CZ_CLOSE_UI_ENCHANT.to_le_bytes());
    let ids = [
        ZC_UI_OPEN_V3,
        CZ_REQUEST_RANDOM_ENCHANT,
        CZ_REQUEST_PERFECT_ENCHANT,
        CZ_REQUEST_UPGRADE_ENCHANT,
        CZ_REQUEST_RESET_ENCHANT,
        ZC_RESPONSE_ENCHANT,
        CZ_CLOSE_UI_ENCHANT,
    ];
    assert_eq!(ids.map(packet_length), [Some(11), Some(12), Some(16), Some(14), Some(12), Some(10), Some(2)]);
}

/// Against a running rathena: `RUSTNAROK_CLIENT_TEST_ACCOUNT=user:password cargo test -- --ignored`.
#[test]
#[ignore]
fn logs_into_local_rathena() {
    let account = std::env::var("RUSTNAROK_CLIENT_TEST_ACCOUNT").unwrap();
    let (user, password) = account.split_once(':').unwrap();
    let accepted = login("127.0.0.1:6900", 0, user, password).unwrap();
    assert!(!accepted.char_servers.is_empty());

    let (sender, receiver) = std::sync::mpsc::channel();
    let _session = CharSession::connect(&accepted.char_servers[0], &accepted, move |event| {
        let _ = sender.send(event);
    })
    .unwrap();
    let characters = match receiver.recv_timeout(Duration::from_secs(10)).unwrap() {
        CharEvent::Characters { characters, .. } => characters,
        other => panic!("{other:?}"),
    };
    assert!(characters.iter().all(|character| !character.name.is_empty()));

    // Play the test character (RUSTNAROK_CLIENT_TEST_SLOT, default 1) and enter its map.
    let slot = std::env::var("RUSTNAROK_CLIENT_TEST_SLOT").map_or(1, |slot| slot.parse().unwrap());
    let mut session = _session;
    session.select(slot).unwrap();
    let (character_id, address) = loop {
        match receiver.recv_timeout(Duration::from_secs(10)).unwrap() {
            CharEvent::ZoneServer { character_id, ip, port, .. } => break (character_id, (ip, port)),
            CharEvent::Characters { .. } | CharEvent::Pin { .. } => continue,
            other => panic!("{other:?}"),
        }
    };
    drop(session);
    let ticket = MapTicket { address, account_id: accepted.account_id, character_id, login_id1: accepted.login_id1, sex: accepted.sex };
    let (sender, receiver) = std::sync::mpsc::channel();
    let mut map = MapSession::connect(&ticket, 0, move |event| {
        let _ = sender.send(event);
    })
    .unwrap();
    // rathena may send the inventory size and such first.
    loop {
        match receiver.recv_timeout(Duration::from_secs(10)).unwrap() {
            MapEvent::Entered { position, .. } => break assert!(position[0] > 0 && position[1] > 0),
            MapEvent::Disconnected(error) => panic!("{error}"),
            _ => continue,
        }
    }
    map.loaded().unwrap();
    // The server answers with what is in view; at least some packet must decode without breaking the stream.
    let first = receiver.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(!matches!(first, MapEvent::Disconnected(_)), "{first:?}");
    drop(map);

    assert!(matches!(login("127.0.0.1:6900", 0, user, "wrong password"), Err(LoginError::Refused(1))));
}

#[test]
fn encodes_standing_resurrection() {
    // rathena parseable_packet(0x0292, 2, clif_parse_AutoRevive, 0): header only.
    assert_eq!(encode_standing_resurrection(), [0x92, 0x02]);
    assert_eq!(packet_length(0x0292), Some(2));
}
