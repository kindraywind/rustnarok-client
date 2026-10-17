//! Login, char and map server protocol, as rathena speaks it for PACKETVER 20220406 (see rathena src/common/packets.hpp).

use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, TcpStream};
use std::sync::LazyLock;
use std::time::Duration;

/// rathena's NAME_LENGTH: 23 characters plus the terminating NUL.
pub const NAME_LENGTH: usize = 24;

const CA_LOGIN: u16 = 0x0064;
const AC_ACCEPT_LOGIN: u16 = 0x0AC4;
const AC_REFUSE_LOGIN: u16 = 0x083E;
const SC_NOTIFY_BAN: u16 = 0x0081;
/// Fixed part of AC_ACCEPT_LOGIN before the char server list.
const ACCEPT_HEADER: usize = 64;
const CHAR_SERVER_SIZE: usize = 160;

#[derive(Debug, Clone, PartialEq)]
pub struct CharServer {
    pub ip: [u8; 4],
    pub port: u16,
    pub name: String,
    pub users: u16,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoginAccepted {
    pub login_id1: u32,
    pub account_id: u32,
    pub login_id2: u32,
    pub sex: u8,
    pub char_servers: Vec<CharServer>,
}

#[derive(Debug)]
pub enum LoginError {
    /// AC_REFUSE_LOGIN error code (0 unregistered ID, 1 wrong password, 6 banned, ...).
    Refused(u32),
    /// SC_NOTIFY_BAN result (1 server closed, 8 already online, ...).
    Notified(u8),
    Io(io::Error),
}

impl From<io::Error> for LoginError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Logs in at `address`, saying the client is `version` (clientinfo.xml's; rathena only checks it when
/// `check_client_version` is on).
pub fn login(address: &str, version: u32, username: &str, password: &str) -> Result<LoginAccepted, LoginError> {
    let mut stream = TcpStream::connect(address)?;
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    stream.write_all(&encode_login(version, username, password))?;

    let mut header = [0u8; 2];
    stream.read_exact(&mut header)?;
    let mut packet = header.to_vec();
    let total = match u16::from_le_bytes(header) {
        AC_ACCEPT_LOGIN => {
            let mut length = [0u8; 2];
            stream.read_exact(&mut length)?;
            packet.extend_from_slice(&length);
            usize::from(u16::from_le_bytes(length))
        }
        AC_REFUSE_LOGIN => 26,
        SC_NOTIFY_BAN => 3,
        other => return Err(io::Error::new(io::ErrorKind::InvalidData, format!("unexpected packet {other:#06x}")).into()),
    };
    let received = packet.len();
    packet.resize(total, 0);
    stream.read_exact(&mut packet[received..])?;
    decode_reply(&packet)
}

fn encode_login(version: u32, username: &str, password: &str) -> Vec<u8> {
    let mut packet = Vec::with_capacity(55);
    packet.extend_from_slice(&CA_LOGIN.to_le_bytes());
    packet.extend_from_slice(&version.to_le_bytes());
    for field in [username, password] {
        let mut bytes = [0u8; NAME_LENGTH];
        let length = field.len().min(NAME_LENGTH - 1);
        bytes[..length].copy_from_slice(&field.as_bytes()[..length]);
        packet.extend_from_slice(&bytes);
    }
    packet.push(0); // client type
    packet
}

fn decode_reply(packet: &[u8]) -> Result<LoginAccepted, LoginError> {
    let u16_at = |offset: usize| u16::from_le_bytes([packet[offset], packet[offset + 1]]);
    let u32_at = |offset: usize| u32::from_le_bytes(packet[offset..offset + 4].try_into().unwrap());
    match u16_at(0) {
        AC_REFUSE_LOGIN => Err(LoginError::Refused(u32_at(2))),
        SC_NOTIFY_BAN => Err(LoginError::Notified(packet[2])),
        _ => Ok(LoginAccepted {
            login_id1: u32_at(4),
            account_id: u32_at(8),
            login_id2: u32_at(12),
            sex: packet[46],
            char_servers: packet[ACCEPT_HEADER..]
                .chunks_exact(CHAR_SERVER_SIZE)
                .map(|server| CharServer {
                    // The IP is in network order, the port little-endian.
                    ip: server[..4].try_into().unwrap(),
                    port: u16::from_le_bytes([server[4], server[5]]),
                    name: c_string(&server[6..26]),
                    users: u16::from_le_bytes([server[26], server[27]]),
                })
                .collect(),
        }),
    }
}

/// A name in a 24-byte EUC-KR field.
fn name_field(name: &str) -> Vec<u8> {
    let mut field = encoding_rs::EUC_KR.encode(name).0.into_owned();
    field.resize(24, 0);
    field
}

/// Decodes a NUL-terminated EUC-KR field.
pub fn c_string(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&byte| byte == 0).unwrap_or(bytes.len());
    encoding_rs::EUC_KR.decode_without_bom_handling(&bytes[..end]).0.into_owned()
}

const CH_ENTER: u16 = 0x0065;
const CH_SELECT_CHAR: u16 = 0x0066;
const HC_ACCEPT_ENTER: u16 = 0x006B;
const HC_REFUSE_ENTER: u16 = 0x006C;
const HC_NOTIFY_ZONESVR: u16 = 0x0AC5;
const CH_MAKE_CHAR: u16 = 0x0A39;
const HC_ACCEPT_MAKECHAR: u16 = 0x0B6F;
const HC_REFUSE_MAKECHAR: u16 = 0x006E;
const CH_DELETE_CHAR3_RESERVED: u16 = 0x0827;
const HC_DELETE_CHAR3_RESERVED: u16 = 0x0828;
const CH_DELETE_CHAR3: u16 = 0x0829;
const HC_DELETE_CHAR3: u16 = 0x082A;
const CH_DELETE_CHAR3_CANCEL: u16 = 0x082B;
const HC_DELETE_CHAR3_CANCEL: u16 = 0x082C;
/// HC_ACCEPT_ENTER before its character list.
const ACCEPT_ENTER_HEADER: usize = 27;
/// CHARACTER_INFO for PACKETVER_MAIN_NUM >= 20220330 (64-bit exp, HP and SP).
const CHARACTER_INFO_SIZE: usize = 175;

#[derive(Debug, Clone, PartialEq)]
pub struct Character {
    pub id: u32,
    pub exp: i64,
    pub zeny: i32,
    pub job_exp: i64,
    pub job_level: i32,
    pub hp: i64,
    pub max_hp: i64,
    pub sp: i64,
    pub max_sp: i64,
    pub job: u16,
    pub hair: u16,
    pub weapon: u16,
    pub base_level: u16,
    pub head_bottom: u16,
    pub shield: u16,
    pub head_top: u16,
    pub head_mid: u16,
    pub hair_color: u16,
    pub clothes_color: u16,
    pub robe: u16,
    pub name: String,
    /// STR, AGI, VIT, INT, DEX, LUK.
    pub stats: [u8; 6],
    pub slot: u8,
    pub map: String,
    /// Seconds until a reserved deletion can be completed (0: none reserved, below 0: it can be now).
    pub deletion: i32,
    pub sex: u8,
}

#[derive(Debug)]
pub enum CharEvent {
    Characters {
        slots: u8,
        characters: Vec<Character>,
    },
    /// HC_REFUSE_ENTER or SC_NOTIFY_BAN code.
    Refused(u8),
    ZoneServer {
        character_id: u32,
        map: String,
        ip: [u8; 4],
        port: u16,
    },
    /// HC_DELETE_CHAR3_RESERVED: 1 reserved (completable in `seconds`), else the reason (0 already, 3 database,
    /// 4 in a guild, 5 in a party).
    DeleteReserved {
        id: u32,
        result: u32,
        seconds: i32,
    },
    /// HC_DELETE_CHAR3: 1 deleted (the list follows), else the reason (2 settings, 3 database, 4 too soon,
    /// 5 wrong birthdate).
    Deleted {
        id: u32,
        result: u32,
    },
    /// HC_DELETE_CHAR3_CANCEL: 1 canceled, else a database error.
    DeleteCanceled {
        id: u32,
        result: u32,
    },
    /// HC_ACCEPT_MAKECHAR: the new character.
    Made(Character),
    /// HC_REFUSE_MAKECHAR: 0 name taken, 1 underage, 2 symbols, 3 slot not available, 0xFF denied.
    MakeRefused(u8),
    Disconnected(io::Error),
}

/// A char server connection; events arrive on a reader thread until the connection closes.
pub struct CharSession {
    stream: TcpStream,
}

impl CharSession {
    pub fn connect(server: &CharServer, login: &LoginAccepted, mut on_event: impl FnMut(CharEvent) + Send + 'static) -> io::Result<Self> {
        let mut stream = TcpStream::connect((Ipv4Addr::from(server.ip), server.port))?;
        let mut enter = Vec::with_capacity(17);
        enter.extend_from_slice(&CH_ENTER.to_le_bytes());
        for value in [login.account_id, login.login_id1, login.login_id2] {
            enter.extend_from_slice(&value.to_le_bytes());
        }
        enter.extend_from_slice(&[0, 0, login.sex]);
        stream.write_all(&enter)?;

        let mut reader = stream.try_clone()?;
        std::thread::spawn(move || {
            let error = (|| -> io::Result<()> {
                // The char server answers with the bare account ID before any packet.
                reader.read_exact(&mut [0u8; 4])?;
                loop {
                    if let Some(event) = decode_char_packet(&read_packet(&mut reader)?) {
                        on_event(event);
                    }
                }
            })()
            .unwrap_err();
            on_event(CharEvent::Disconnected(error));
        });
        Ok(Self { stream })
    }

    /// Asks to play the character in `slot`; the answer is a `ZoneServer` or `Refused` event.
    pub fn select(&mut self, slot: u8) -> io::Result<()> {
        let [low, high] = CH_SELECT_CHAR.to_le_bytes();
        self.stream.write_all(&[low, high, slot])
    }

    /// Reserves a character's deletion; the answer is a `DeleteReserved` event.
    pub fn reserve_delete(&mut self, id: u32) -> io::Result<()> {
        self.send_id(CH_DELETE_CHAR3_RESERVED, id, &[])
    }

    pub fn cancel_delete(&mut self, id: u32) -> io::Result<()> {
        self.send_id(CH_DELETE_CHAR3_CANCEL, id, &[])
    }

    /// Deletes a character whose reservation is over, proven by the account's birthdate (YYMMDD).
    pub fn delete(&mut self, id: u32, birthdate: &str) -> io::Result<()> {
        let mut digits = [0u8; 6];
        for (digit, byte) in digits.iter_mut().zip(birthdate.bytes()) {
            *digit = byte;
        }
        self.send_id(CH_DELETE_CHAR3, id, &digits)
    }

    /// Creates a character; the answer is `Made` or `MakeRefused`.
    pub fn make(&mut self, name: &str, slot: u8, hair_color: u16, hair_style: u16, job: u32, sex: u8) -> io::Result<()> {
        let mut packet = CH_MAKE_CHAR.to_le_bytes().to_vec();
        let mut field = [0u8; 24];
        let encoded = encoding_rs::EUC_KR.encode(name).0;
        // NUL-terminated: at most 23 bytes of name.
        let length = encoded.len().min(field.len() - 1);
        field[..length].copy_from_slice(&encoded[..length]);
        packet.extend_from_slice(&field);
        packet.push(slot);
        packet.extend_from_slice(&hair_color.to_le_bytes());
        packet.extend_from_slice(&hair_style.to_le_bytes());
        packet.extend_from_slice(&job.to_le_bytes());
        packet.push(sex);
        self.stream.write_all(&packet)
    }

    fn send_id(&mut self, header: u16, id: u32, rest: &[u8]) -> io::Result<()> {
        let mut packet = header.to_le_bytes().to_vec();
        packet.extend_from_slice(&id.to_le_bytes());
        packet.extend_from_slice(rest);
        self.stream.write_all(&packet)
    }
}

impl Drop for CharSession {
    fn drop(&mut self) {
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
    }
}

/// Packet lengths for PACKETVER 20220406, generated from rathena (see tools/packet-lengths); -1 is variable.
fn packet_length(id: u16) -> Option<i32> {
    static TABLE: LazyLock<HashMap<u16, i32>> = LazyLock::new(|| {
        include_str!("packet_lengths.txt")
            .lines()
            .filter(|line| !line.starts_with('#'))
            .filter_map(|line| {
                let (id, length) = line.split_once(' ')?;
                Some((u16::from_str_radix(id.trim_start_matches("0x"), 16).ok()?, length.parse().ok()?))
            })
            .collect()
    });
    TABLE.get(&id).copied()
}

/// Dev hook (the client's control socket): sees every packet read from the char and map servers.
pub static PACKET_HOOK: std::sync::OnceLock<fn(&[u8])> = std::sync::OnceLock::new();

/// Reads one packet: fixed lengths come from the table, variable ones carry their length at offset 2.
fn read_packet(reader: &mut impl Read) -> io::Result<Vec<u8>> {
    let mut packet = vec![0u8; 2];
    reader.read_exact(&mut packet)?;
    let id = u16::from_le_bytes([packet[0], packet[1]]);
    let size = match packet_length(id) {
        Some(-1) => {
            packet.resize(4, 0);
            reader.read_exact(&mut packet[2..])?;
            usize::from(u16::from_le_bytes([packet[2], packet[3]]))
        }
        Some(length) => length as usize,
        None => return Err(io::Error::new(io::ErrorKind::InvalidData, format!("unknown packet {id:#06x}"))),
    };
    let received = packet.len();
    packet.resize(size.max(received), 0);
    reader.read_exact(&mut packet[received..])?;
    if std::env::var_os("RUSTNAROK_CLIENT_TRACE").is_some() {
        let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() % 100_000;
        let shown = if std::env::var("RUSTNAROK_CLIENT_TRACE").is_ok_and(|trace| trace == "full") { packet.len() } else { packet.len().min(32) };
        eprintln!("{ms:05} <- {id:#06x} {} {:02x?}", packet.len(), &packet[..shown]);
    }
    if let Some(hook) = PACKET_HOOK.get() {
        hook(&packet);
    }
    Ok(packet)
}

fn decode_char_packet(packet: &[u8]) -> Option<CharEvent> {
    match u16::from_le_bytes([packet[0], packet[1]]) {
        HC_ACCEPT_ENTER => Some(CharEvent::Characters {
            slots: packet[4],
            characters: packet[ACCEPT_ENTER_HEADER..].chunks_exact(CHARACTER_INFO_SIZE).map(decode_character).collect(),
        }),
        HC_REFUSE_ENTER | SC_NOTIFY_BAN => Some(CharEvent::Refused(packet[2])),
        HC_NOTIFY_ZONESVR => Some(CharEvent::ZoneServer {
            character_id: u32::from_le_bytes(packet[2..6].try_into().unwrap()),
            map: c_string(&packet[6..22]),
            ip: packet[22..26].try_into().unwrap(),
            port: u16::from_le_bytes([packet[26], packet[27]]),
        }),
        HC_DELETE_CHAR3_RESERVED => Some(CharEvent::DeleteReserved { id: u32_at(packet, 2), result: u32_at(packet, 6), seconds: u32_at(packet, 10) as i32 }),
        HC_DELETE_CHAR3 => Some(CharEvent::Deleted { id: u32_at(packet, 2), result: u32_at(packet, 6) }),
        HC_DELETE_CHAR3_CANCEL => Some(CharEvent::DeleteCanceled { id: u32_at(packet, 2), result: u32_at(packet, 6) }),
        HC_ACCEPT_MAKECHAR => Some(CharEvent::Made(decode_character(&packet[2..]))),
        HC_REFUSE_MAKECHAR => Some(CharEvent::MakeRefused(packet[2])),
        // Slot info, page count, PIN state and blocked characters aren't needed yet.
        _ => None,
    }
}

fn decode_skill(entry: &[u8]) -> Skill {
    Skill { id: u16_at(entry, 0), kind: u32_at(entry, 2), level: u16_at(entry, 6), sp: u16_at(entry, 8), range: u16_at(entry, 10), upgradable: entry[36] != 0 }
}

fn decode_character(info: &[u8]) -> Character {
    let i16_at = |offset: usize| u16::from_le_bytes([info[offset], info[offset + 1]]);
    let i32_at = |offset: usize| i32::from_le_bytes(info[offset..offset + 4].try_into().unwrap());
    let i64_at = |offset: usize| i64::from_le_bytes(info[offset..offset + 8].try_into().unwrap());
    Character {
        id: i32_at(0) as u32,
        exp: i64_at(4),
        zeny: i32_at(12),
        job_exp: i64_at(16),
        job_level: i32_at(24),
        hp: i64_at(50),
        max_hp: i64_at(58),
        sp: i64_at(66),
        max_sp: i64_at(74),
        job: i16_at(84),
        hair: i16_at(86),
        weapon: i16_at(90),
        base_level: i16_at(92),
        head_bottom: i16_at(96),
        shield: i16_at(98),
        head_top: i16_at(100),
        head_mid: i16_at(102),
        hair_color: i16_at(104),
        clothes_color: i16_at(106),
        robe: i32_at(162) as u16,
        name: c_string(&info[108..132]),
        stats: info[132..138].try_into().unwrap(),
        slot: info[138],
        map: c_string(&info[142..158]),
        deletion: i32_at(158),
        sex: info[174],
    }
}

const CZ_ENTER: u16 = 0x0436;
const CZ_NOTIFY_ACTORINIT: u16 = 0x007D;
const CZ_REQUEST_MOVE: u16 = 0x035F;
const CZ_REQUEST_TIME: u16 = 0x0360;
const CZ_REQUEST_CHAT: u16 = 0x00F3;
const CZ_WHISPER: u16 = 0x0096;
const ZC_WHISPER: u16 = 0x09DE;
const ZC_ACK_WHISPER: u16 = 0x09DF;
const ZC_ACCEPT_ENTER: u16 = 0x02EB;
const ZC_NOTIFY_VANISH: u16 = 0x0080;
const ZC_NOTIFY_PLAYERMOVE: u16 = 0x0087;
const ZC_STOPMOVE: u16 = 0x0088;
const ZC_CHANGE_DIRECTION: u16 = 0x009C;
const ZC_NOTIFY_CHAT: u16 = 0x008D;
const ZC_NOTIFY_PLAYERCHAT: u16 = 0x008E;
const ZC_NPCACK_MAPMOVE: u16 = 0x0091;
const ZC_NOTIFY_MOVEENTRY: u16 = 0x09FD;
const CZ_CONTACTNPC: u16 = 0x0090;
const CZ_CHOOSE_MENU: u16 = 0x00B8;
const CZ_REQ_NEXT_SCRIPT: u16 = 0x00B9;
const CZ_CLOSE_DIALOG: u16 = 0x0146;
const CZ_INPUT_EDITDLG: u16 = 0x0143;
const CZ_INPUT_EDITDLGSTR: u16 = 0x01D5;
const CZ_REQUEST_ACT: u16 = 0x0437;
const ZC_SAY_DIALOG: u16 = 0x00B4;
const ZC_WAIT_DIALOG: u16 = 0x00B5;
const ZC_CLOSE_DIALOG: u16 = 0x00B6;
const ZC_MENU_LIST: u16 = 0x00B7;
const ZC_OPEN_EDITDLG: u16 = 0x0142;
const ZC_OPEN_EDITDLGSTR: u16 = 0x01D4;
const ZC_CLEAR_DIALOG: u16 = 0x08D6;
const ZC_NOTIFY_ACT: u16 = 0x08C8;
const ZC_PAR_CHANGE: u16 = 0x00B0;
const ZC_LONGLONGPAR_CHANGE: u16 = 0x0ACB;
const ZC_HP_INFO: u16 = 0x0977;
const ZC_ATTACK_FAILURE_FOR_DISTANCE: u16 = 0x0139;
const ZC_ATTACK_RANGE: u16 = 0x013A;
const ZC_LONGPAR_CHANGE: u16 = 0x00B1;
const ZC_ITEM_ENTRY: u16 = 0x009D;
const ZC_ITEM_FALL_ENTRY: u16 = 0x0ADD;
const ZC_ITEM_DISAPPEAR: u16 = 0x00A1;
const CZ_ITEM_PICKUP: u16 = 0x0362;
const ZC_ITEM_PICKUP_ACK: u16 = 0x0B41;
const ZC_INVENTORY_START: u16 = 0x0B08;
const ZC_INVENTORY_ITEMLIST_NORMAL: u16 = 0x0B09;
const ZC_INVENTORY_ITEMLIST_EQUIP: u16 = 0x0B39;
const ZC_EXTEND_BODYITEM_SIZE: u16 = 0x0B18;
const ZC_ITEM_THROW_ACK: u16 = 0x00AF;
const ZC_DELETE_ITEM_FROM_BODY: u16 = 0x07FA;
const CZ_USE_ITEM: u16 = 0x0439;
const ZC_USE_ITEM_ACK: u16 = 0x01C8;
const ZC_SPRITE_CHANGE: u16 = 0x01D7;
const CZ_REQ_WEAR_EQUIP: u16 = 0x0998;
const ZC_REQ_WEAR_EQUIP_ACK: u16 = 0x0999;
const CZ_REQ_TAKEOFF_EQUIP: u16 = 0x00AB;
const ZC_SHORTCUT_KEY_LIST: u16 = 0x0B20;
const CZ_SHORTCUT_KEY_CHANGE: u16 = 0x0B21;
/// Hotkey entries: skill flag, item or skill ID, amount or level.
const HOTKEY_SIZE: usize = 7;
const ZC_REQ_TAKEOFF_EQUIP_ACK: u16 = 0x099A;
const ZC_SKILLINFO_LIST: u16 = 0x010F;
const ZC_ADD_SKILL: u16 = 0x0111;
const ZC_SKILLINFO_UPDATE: u16 = 0x010E;
const ZC_SKILLINFO_UPDATE2: u16 = 0x07E1;
const ZC_SKILLINFO_DELETE: u16 = 0x0441;
const CZ_UPGRADE_SKILLLEVEL: u16 = 0x0112;
const CZ_USE_SKILL: u16 = 0x0438;
const CZ_USE_SKILL_TOGROUND: u16 = 0x0366;
const ZC_USESKILL_ACK: u16 = 0x0B1A;
const ZC_NOTIFY_SKILL: u16 = 0x01DE;
const ZC_USE_SKILL: u16 = 0x09CB;
const ZC_NOTIFY_GROUNDSKILL: u16 = 0x0117;
const ZC_ACK_TOUSESKILL: u16 = 0x0110;
const ZC_RECOVERY: u16 = 0x0A27;
const ZC_MSG_STATE_CHANGE: u16 = 0x0196;
const ZC_NOTIFY_EFFECT: u16 = 0x019B;
const ZC_SELECT_DEALTYPE: u16 = 0x00C4;
const CZ_ACK_SELECT_DEALTYPE: u16 = 0x00C5;
const ZC_PC_PURCHASE_ITEMLIST: u16 = 0x0B77;
const ZC_PC_SELL_ITEMLIST: u16 = 0x00C7;
const CZ_PC_PURCHASE_ITEMLIST: u16 = 0x00C8;
const CZ_PC_SELL_ITEMLIST: u16 = 0x00C9;
const ZC_PC_PURCHASE_RESULT: u16 = 0x00CA;
const ZC_PC_SELL_RESULT: u16 = 0x00CB;
const ZC_NOTIFY_EFFECT2: u16 = 0x01F3;
const ZC_NOTIFY_EFFECT3: u16 = 0x0284;
const ZC_MSG_STATE_CHANGE2: u16 = 0x043F;
const ZC_MSG_STATE_CHANGE3: u16 = 0x0983;
const ZC_NOTIFY_STOREITEM_COUNTINFO: u16 = 0x00F2;
const ZC_SHOW_IMAGE: u16 = 0x01B3;
const CZ_REQ_EXCHANGE_ITEM: u16 = 0x00E4;
const CZ_ACK_EXCHANGE_ITEM: u16 = 0x00E6;
const CZ_ADD_EXCHANGE_ITEM: u16 = 0x00E8;
const CZ_CONCLUDE_EXCHANGE_ITEM: u16 = 0x00EB;
const CZ_CANCEL_EXCHANGE_ITEM: u16 = 0x00ED;
const CZ_EXEC_EXCHANGE_ITEM: u16 = 0x00EF;
const ZC_REQ_EXCHANGE_ITEM: u16 = 0x01F4;
const ZC_ACK_EXCHANGE_ITEM: u16 = 0x01F5;
const ZC_ADD_EXCHANGE_ITEM: u16 = 0x0B42;
const ZC_ACK_ADD_EXCHANGE_ITEM: u16 = 0x00EA;
const ZC_CONCLUDE_EXCHANGE_ITEM: u16 = 0x00EC;
const ZC_CANCEL_EXCHANGE_ITEM: u16 = 0x00EE;
const ZC_EXEC_EXCHANGE_ITEM: u16 = 0x00F0;
const CZ_MAKE_GROUP2: u16 = 0x01E8;
const ZC_FRIENDS_LIST: u16 = 0x0201;
const ZC_NOTIFY_CARTITEM_COUNTINFO: u16 = 0x0121;
const ZC_OPENSTORE: u16 = 0x012D;
const CZ_REQ_CLOSESTORE: u16 = 0x012E;
const CZ_REQ_BUY_FROMMC: u16 = 0x0130;
const ZC_STORE_ENTRY: u16 = 0x0131;
const ZC_DISAPPEAR_ENTRY: u16 = 0x0132;
const ZC_PC_PURCHASE_RESULT_FROMMC: u16 = 0x0135;
const CZ_REQ_OPENSTORE2: u16 = 0x01B2;
const CZ_PC_PURCHASE_ITEMLIST_FROMMC2: u16 = 0x0801;
const ZC_DELETEITEM_FROM_MCSTORE: u16 = 0x09E5;
const ZC_ACK_OPENSTORE2: u16 = 0x0A28;
const ZC_PC_PURCHASE_ITEMLIST_FROMMC: u16 = 0x0B3D;
const ZC_PC_PURCHASE_MYITEMLIST: u16 = 0x0B40;
const ZC_DELETE_ITEM_FROM_CART: u16 = 0x0125;
const CZ_MOVE_ITEM_FROM_BODY_TO_CART: u16 = 0x0126;
const CZ_MOVE_ITEM_FROM_CART_TO_BODY: u16 = 0x0127;
const ZC_ADD_ITEM_TO_CART: u16 = 0x0B45;
const ZC_NOTIFY_UNREADMAIL: u16 = 0x09E7;
const CZ_OPEN_MAILBOX2: u16 = 0x0AC0;
const CZ_REQ_REFRESH_MAIL_LIST2: u16 = 0x0AC1;
const CZ_CLOSE_MAILBOX: u16 = 0x09E9;
const ZC_ACK_MAIL_LIST3: u16 = 0x0AC2;
const CZ_REQ_READ_MAIL: u16 = 0x09EA;
const ZC_ACK_READ_RODEX: u16 = 0x0B63;
const CZ_REQ_ZENY_FROM_MAIL: u16 = 0x09F1;
const ZC_ACK_ZENY_FROM_MAIL: u16 = 0x09F2;
const CZ_REQ_ITEM_FROM_MAIL: u16 = 0x09F3;
const ZC_ACK_ITEM_FROM_MAIL: u16 = 0x09F4;
const CZ_REQ_DELETE_MAIL: u16 = 0x09F5;
const ZC_ACK_DELETE_MAIL: u16 = 0x09F6;
const CZ_REQ_OPEN_WRITE_MAIL: u16 = 0x0A08;
const ZC_ACK_OPEN_WRITE_MAIL: u16 = 0x0A12;
const CZ_REQ_CANCEL_WRITE_MAIL: u16 = 0x0A03;
const CZ_CHECKNAME2: u16 = 0x0B97;
const ZC_CHECKNAME: u16 = 0x0A51;
const CZ_REQ_ADD_ITEM_TO_MAIL: u16 = 0x0A04;
const ZC_ACK_ADD_ITEM_RODEX: u16 = 0x0B3F;
const CZ_REQ_REMOVE_ITEM_MAIL: u16 = 0x0A06;
const ZC_ACK_REMOVE_ITEM_MAIL: u16 = 0x0A07;
const CZ_REQ_WRITE_MAIL2: u16 = 0x0A6E;
const ZC_ACK_WRITE_MAIL: u16 = 0x09ED;
const ZC_ALL_QUEST_LIST3: u16 = 0x09F8;
const ZC_ADD_QUEST3: u16 = 0x09F9;
const ZC_UPDATE_MISSION_HUNT_EX: u16 = 0x09FA;
const ZC_HUNTING_QUEST_INFO: u16 = 0x08FE;
const ZC_DEL_QUEST: u16 = 0x02B4;
const ZC_ACTIVE_QUEST: u16 = 0x02B7;
const CZ_ACTIVE_QUEST: u16 = 0x02B6;
const CZ_REQ_GUILD_MENU: u16 = 0x014F;
const CZ_REQ_LEAVE_GUILD: u16 = 0x0159;
const CZ_REQ_MAKE_GUILD: u16 = 0x0165;
const CZ_JOIN_GUILD: u16 = 0x016B;
const CZ_GUILD_CHAT: u16 = 0x017E;
const CZ_REQ_JOIN_GUILD2: u16 = 0x0916;
const ZC_POSITION_ID_NAME_INFO: u16 = 0x0166;
const ZC_RESULT_MAKE_GUILD: u16 = 0x0167;
const ZC_ACK_REQ_JOIN_GUILD: u16 = 0x0169;
const ZC_REQ_JOIN_GUILD: u16 = 0x016A;
const ZC_GUILD_NOTICE: u16 = 0x016F;
const ZC_GUILD_CHAT: u16 = 0x017F;
const ZC_UPDATE_CHARSTAT: u16 = 0x01F2;
const ZC_UPDATE_GDID: u16 = 0x02F7;
const ZC_ACK_BAN_GUILD: u16 = 0x0A82;
const ZC_ACK_LEAVE_GUILD: u16 = 0x0A83;
const ZC_GUILD_INFO: u16 = 0x0B7B;
const ZC_MEMBERMGR_INFO: u16 = 0x0B7D;
const CZ_ADD_FRIENDS: u16 = 0x0202;
const CZ_DELETE_FRIENDS: u16 = 0x0203;
const ZC_FRIENDS_STATE: u16 = 0x0206;
const ZC_REQ_ADD_FRIENDS: u16 = 0x0207;
const CZ_ACK_REQ_ADD_FRIENDS: u16 = 0x0208;
const ZC_ADD_FRIENDS_LIST: u16 = 0x0209;
const ZC_DELETE_FRIENDS: u16 = 0x020A;
const CZ_RESTART: u16 = 0x00B2;
const CZ_REQ_EMOTION: u16 = 0x00BF;
const ZC_EMOTION: u16 = 0x00C0;
const CZ_STATUS_CHANGE: u16 = 0x00BB;
const ZC_STATUS_CHANGE_ACK: u16 = 0x00BC;
const ZC_STATUS: u16 = 0x00BD;
const ZC_STATUS_CHANGE: u16 = 0x00BE;
const ZC_COUPLESTATUS: u16 = 0x0141;
const CZ_REQUEST_QUIT: u16 = 0x018A;
const ZC_RESTART_ACK: u16 = 0x00B3;
const ZC_ACK_REQ_DISCONNECT: u16 = 0x018B;
const ZC_RESURRECTION: u16 = 0x0148;
const CZ_CREATE_CHATROOM: u16 = 0x00D5;
const CZ_REQ_ENTER_ROOM: u16 = 0x00D9;
const CZ_EXIT_ROOM: u16 = 0x00E3;
const ZC_ROOM_NEWENTRY: u16 = 0x00D7;
const ZC_DESTROY_ROOM: u16 = 0x00D8;
const ZC_ACK_CREATE_CHATROOM: u16 = 0x00D6;
const ZC_REFUSE_ENTER_ROOM: u16 = 0x00DA;
const ZC_ENTER_ROOM: u16 = 0x00DB;
const ZC_MEMBER_NEWENTRY: u16 = 0x00DC;
const ZC_MEMBER_EXIT: u16 = 0x00DD;
const ZC_CHANGE_CHATROOM: u16 = 0x00DF;
const ZC_ROLE_CHANGE: u16 = 0x00E1;
const CZ_PARTY_JOIN_REQ: u16 = 0x02C4;
const CZ_PARTY_JOIN_REQ_ACK: u16 = 0x02C7;
const CZ_REQ_LEAVE_GROUP: u16 = 0x0100;
const CZ_REQ_EXPEL_GROUP_MEMBER: u16 = 0x0103;
const CZ_REQUEST_CHAT_PARTY: u16 = 0x0108;
const ZC_ACK_MAKE_GROUP: u16 = 0x00FA;
const ZC_PARTY_JOIN_REQ: u16 = 0x02C6;
const ZC_PARTY_JOIN_REQ_ACK: u16 = 0x02C5;
const ZC_ADD_MEMBER_TO_GROUP: u16 = 0x0AE4;
const ZC_GROUP_LIST: u16 = 0x0AE5;
const ZC_DELETE_MEMBER_FROM_GROUP: u16 = 0x0105;
const ZC_NOTIFY_CHAT_PARTY: u16 = 0x0109;
const ZC_NOTIFY_POSITION_TO_GROUPM: u16 = 0x0107;
const ZC_NOTIFY_HP_TO_GROUPM: u16 = 0x080E;
const ZC_REQ_GROUPINFO_CHANGE_V2: u16 = 0x07D8;
const ZC_CHANGE_GROUP_MASTER: u16 = 0x07FC;
const ZC_GROUP_ISALIVE: u16 = 0x0AB2;
const ZC_NOTIFY_MEMBERINFO_TO_GROUPM: u16 = 0x0ABD;
const ZC_COMPASS: u16 = 0x0144;
const ZC_ADD_ITEM_TO_STORE: u16 = 0x0B44;
const ZC_DELETE_ITEM_FROM_STORE: u16 = 0x00F6;
const ZC_CLOSE_STORE: u16 = 0x00F8;
const CZ_MOVE_ITEM_FROM_BODY_TO_STORE: u16 = 0x0364;
const CZ_MOVE_ITEM_FROM_STORE_TO_BODY: u16 = 0x0365;
const CZ_CLOSE_STORE: u16 = 0x0193;
/// A ZC_SKILLINFO_LIST entry (with the server's skill name, for clients before the 2019 renewal of the packet).
const SKILL_SIZE: usize = 37;
/// Inventory list entries: ordinary items and equipment.
const NORMAL_ITEM_SIZE: usize = 34;
const EQUIP_ITEM_SIZE: usize = 68;

/// Status IDs in ZC_PAR_CHANGE (rathena's SP_*).
pub const STATUS_HP: u16 = 5;
pub const STATUS_MAX_HP: u16 = 6;
pub const STATUS_SP: u16 = 7;
pub const STATUS_MAX_SP: u16 = 8;
pub const STATUS_BASE_EXP: u16 = 1;
pub const STATUS_JOB_EXP: u16 = 2;
pub const STATUS_BASE_LEVEL: u16 = 11;
pub const STATUS_ZENY: u16 = 20;
pub const STATUS_NEXT_BASE_EXP: u16 = 22;
pub const STATUS_NEXT_JOB_EXP: u16 = 23;
pub const STATUS_JOB_LEVEL: u16 = 55;
pub const STATUS_SKILL_POINT: u16 = 12;
/// Tenths of a weight unit.
pub const STATUS_WEIGHT: u16 = 24;
pub const STATUS_MAX_WEIGHT: u16 = 25;
/// Status points left; Str to Luk (13-18) and the points each needs to go up (32-37).
pub const STATUS_POINTS: u16 = 9;
pub const STATUS_STR: u16 = 13;
pub const STATUS_STR_COST: u16 = 32;
/// Atk, Atk bonus, Matk, Matk bonus, Def, Def bonus, Mdef, Mdef bonus, Hit, Flee, Flee bonus, Critical, ASPD (41-53).
pub const STATUS_ATK: u16 = 41;
/// Our own key for a stat's bonus from gear (Str + this is Str's bonus); the server never sends it.
pub const STATUS_BONUS: u16 = 0x8000;

/// What a ZC_SPRITE_CHANGE changes (rathena's _look).
pub const LOOK_JOB: u8 = 0;
pub const LOOK_HAIR: u8 = 1;
pub const LOOK_WEAPON: u8 = 2;
pub const LOOK_HEAD_BOTTOM: u8 = 3;
pub const LOOK_HEAD_TOP: u8 = 4;
pub const LOOK_HEAD_MID: u8 = 5;
pub const LOOK_HAIR_COLOR: u8 = 6;
pub const LOOK_CLOTHES_COLOR: u8 = 7;
pub const LOOK_SHIELD: u8 = 8;
pub const LOOK_ROBE: u8 = 12;

/// Item types as the server sends them (rathena's item_types).
pub const ITEM_HEALING: u8 = 0;
pub const ITEM_USABLE: u8 = 2;
pub const ITEM_ARMOR: u8 = 4;
pub const ITEM_WEAPON: u8 = 5;
pub const ITEM_CARD: u8 = 6;
pub const ITEM_PET_EGG: u8 = 7;
pub const ITEM_PET_ARMOR: u8 = 8;
pub const ITEM_DELAY_CONSUME: u8 = 11;
pub const ITEM_SHADOW_GEAR: u8 = 12;
pub const ITEM_AMMO: u8 = 10;
pub const ITEM_CASH: u8 = 18;

/// CZ_REQUEST_ACT actions.
pub const ACT_ATTACK: u8 = 0;
pub const ACT_SIT: u8 = 2;
pub const ACT_STAND: u8 = 3;
pub const ACT_ATTACK_REPEAT: u8 = 7;
const ZC_NOTIFY_NEWENTRY: u16 = 0x09FE;
const ZC_NOTIFY_STANDENTRY: u16 = 0x09FF;

/// A player, NPC or monster as the map server describes it (ZC_NOTIFY_*ENTRY11).
#[derive(Debug, Clone, PartialEq)]
pub struct Unit {
    pub id: u32,
    /// Object type: 0 player, 5 monster, 6 NPC, ... (rathena's clif_bl_type).
    pub kind: u8,
    pub speed: u16,
    pub job: u16,
    pub hair: u16,
    pub head_bottom: u16,
    pub head_top: u16,
    pub head_mid: u16,
    /// Weapon and shield view IDs (the item's look), and the garment's.
    pub weapon: u32,
    pub shield: u32,
    pub robe: u16,
    pub hair_color: u16,
    pub clothes_color: u16,
    pub head_direction: u16,
    pub sex: u8,
    pub position: [u16; 2],
    pub direction: u8,
    pub level: u16,
    pub name: String,
    /// Sitting when it came into view.
    pub sitting: bool,
}

#[derive(Debug)]
pub enum MapEvent {
    Entered {
        position: [u16; 2],
        direction: u8,
    },
    /// A unit is standing in view (appeared or was already there).
    Unit(Unit),
    /// A unit walking in view; its `position` is where the walk starts.
    UnitWalk {
        unit: Unit,
        to: [u16; 2],
        start_time: u32,
    },
    Vanish {
        id: u32,
        reason: u8,
    },
    PlayerMove {
        from: [u16; 2],
        to: [u16; 2],
        start_time: u32,
    },
    Stop {
        id: u32,
        position: [u16; 2],
    },
    /// A unit turned its body or (players) its head.
    Turn {
        id: u32,
        direction: u8,
        head_direction: u16,
    },
    /// Chat from another unit, or our own (id None).
    Chat {
        id: Option<u32>,
        message: String,
    },
    MapChange {
        map: String,
        position: [u16; 2],
    },
    /// NPC dialog text (one line; the box keeps appending until cleared).
    NpcSay {
        npc: u32,
        text: String,
    },
    /// Show the dialog's "next" button.
    NpcNext {
        npc: u32,
    },
    /// Show the dialog's "close" button.
    NpcClose {
        npc: u32,
    },
    /// Close every dialog window of this NPC.
    NpcClear {
        npc: u32,
    },
    NpcMenu {
        npc: u32,
        items: Vec<String>,
    },
    /// Ask for a number, or text when `text` is set.
    NpcInput {
        npc: u32,
        text: bool,
    },
    /// An attack (or other action) between units (ZC_NOTIFY_ACT).
    Act(Act),
    /// One of the player's own status values changed (HP, SP, ...).
    Status {
        id: u16,
        value: i64,
    },
    /// A monster's HP, sent while it's being fought.
    UnitHp {
        id: u32,
        hp: i32,
        max_hp: i32,
    },
    /// The player's attack reach in cells.
    AttackRange(u16),
    /// An attack request was out of reach: walk closer first.
    TooFar {
        target: u32,
        target_cell: [u16; 2],
        cell: [u16; 2],
        range: u16,
    },
    GroundItem(GroundItem),
    /// An item on the ground was picked up or went away.
    ItemGone {
        id: u32,
    },
    /// The result of picking something up: the inventory entry it went into, unless `result` is nonzero.
    PickedUp {
        result: u8,
        item: InventoryItem,
    },
    /// The server starts listing an inventory (`kind` 0 is the player's own).
    InventoryStart {
        kind: u8,
    },
    InventoryItems {
        kind: u8,
        items: Vec<InventoryItem>,
    },
    /// Inventory slots beyond the base 100.
    InventoryExpansion(u16),
    /// Fewer of an inventory item: dropped, used up or taken.
    ItemRemoved {
        index: u16,
        amount: u16,
    },
    /// Someone used an item; for the player, how many are left.
    ItemUsed {
        index: u16,
        item: u32,
        user: u32,
        remaining: u16,
        ok: bool,
    },
    /// A unit shows an effect by its ID (ZC_NOTIFY_EFFECT2/3).
    Effect {
        id: u32,
        effect: u32,
    },
    /// A level up, refining result and the like on a unit (ZC_NOTIFY_EFFECT): which kind.
    MiscEffect {
        id: u32,
        kind: u32,
    },
    /// An NPC shop asks whether to buy or sell (ZC_SELECT_DEALTYPE).
    DealChoice {
        npc: u32,
    },
    /// What the shop sells (ZC_PC_PURCHASE_ITEMLIST).
    ShopGoods(Vec<ShopGoods>),
    /// What the shop would buy from the inventory: index, price and the price it pays (ZC_PC_SELL_ITEMLIST).
    ShopWants(Vec<(u16, u32, u32)>),
    /// How buying or selling went: 0 done, else why not (ZC_PC_PURCHASE_RESULT, ZC_PC_SELL_RESULT).
    Bought(u8),
    Sold(u8),
    /// How full the storage is (ZC_NOTIFY_STOREITEM_COUNTINFO).
    StorageCount {
        amount: u16,
        max: u16,
    },
    /// An item went into storage (ZC_ADD_ITEM_TO_STORE); a stack already there grows.
    StorageAdded(InventoryItem),
    /// Fewer of a stored item (ZC_DELETE_ITEM_FROM_STORE).
    StorageRemoved {
        index: u16,
        amount: u32,
    },
    StorageClosed,
    /// A unit shows an emotion (the client's ET_* number).
    Emotion {
        id: u32,
        kind: u8,
    },
    /// Several status values at once (rathena's SP_* IDs, bonuses under STATUS_BONUS).
    Statuses(Vec<(u16, i64)>),
    /// Going back to character select: 1 yes, 0 not yet (wait 10 seconds).
    RestartAck(u8),
    /// Leaving the game: 0 go, 1 not yet.
    QuitAck(u16),
    /// A unit came back to life.
    Revived {
        id: u32,
    },
    /// A chat room over its owner's head, new or changed (ZC_ROOM_NEWENTRY, ZC_CHANGE_CHATROOM): its ID, size, how
    /// many are in, kind (0 password, 1 open, 2 an NPC's waiting room) and title.
    ChatRoom {
        owner: u32,
        id: u32,
        limit: u16,
        users: u16,
        kind: u8,
        title: String,
    },
    ChatRoomGone {
        id: u32,
    },
    /// Making a room: 0 made, 1 too many rooms, 2 the same room exists.
    ChatRoomMade(u8),
    /// Why a room can't be entered (0 full, 1 wrong password, 2 kicked out before, 4 not enough zeny, 5-7 level or job).
    ChatRoomRefused(u8),
    /// In a room: its ID, and who's there (and whether they own it).
    ChatRoomEntered {
        id: u32,
        members: Vec<(String, bool)>,
    },
    ChatRoomJoined {
        users: u16,
        name: String,
    },
    ChatRoomLeft {
        users: u16,
        name: String,
        kicked: bool,
    },
    /// Someone becomes (or stops being) the room's owner.
    ChatRoomOwner {
        owner: bool,
        name: String,
    },
    /// The vending skill went off: set up a shop of up to this many items (ZC_OPENSTORE).
    VendSetup(u16),
    /// Our shop opened (0) or didn't, and what it sells.
    VendOpened(u8),
    MyShop(Vec<VendItem>),
    /// Someone bought from our shop: which item, how many, for how much in all.
    VendSold {
        index: u16,
        amount: u16,
        zeny: i32,
    },
    /// A vendor's sign over their head, and its going away.
    VendorSign {
        id: u32,
        title: String,
    },
    VendorGone {
        id: u32,
    },
    /// What a vendor sells (their ID, the shop's ID to buy with, the goods).
    VendorGoods {
        vendor: u32,
        shop: u32,
        goods: Vec<VendItem>,
    },
    /// How buying an item from a vendor went: 0 bought, 1 not enough zeny, 2 too heavy, 4 out of stock, ...
    VendBought {
        index: u16,
        amount: u16,
        result: u8,
    },
    /// Whether unread mail waits (ZC_NOTIFY_UNREADMAIL).
    MailUnread(bool),
    /// The mailbox, newest first.
    MailList(Vec<MailHeader>),
    /// A mail opened: its text, and the zeny and items still in it.
    MailRead {
        kind: u8,
        id: u64,
        text: String,
        zeny: u64,
        items: Vec<InventoryItem>,
    },
    /// Taking a mail's zeny (or its items): 0 done, 1 failed (or too much zeny), 2 too heavy or no room.
    MailClaimed {
        id: u64,
        zeny: bool,
        result: u8,
    },
    MailDeleted {
        id: u64,
    },
    /// Whether writing a mail (to that name) may start.
    MailWrite {
        to: String,
        ok: bool,
    },
    /// Whose name it is: their character ID (0 for nobody), job and level.
    MailReceiver {
        character: u32,
        job: u16,
        level: u16,
        name: String,
    },
    /// An item went in the mail being written (result 0; 1 too heavy, 2 failed, 3 no room, 4 can't be mailed),
    /// with what the attachments weigh now.
    MailItemAdded {
        result: u8,
        item: InventoryItem,
        weight: u16,
    },
    MailItemRemoved {
        ok: bool,
        index: u16,
        amount: u16,
        weight: u16,
    },
    /// How sending went: 0 sent, 1 failed, 2 too many today, 3 an item can't go, 4 no such receiver.
    MailSent(u8),
    /// The quest log at login, a quest taken, and quests leaving it.
    Quests(Vec<Quest>),
    QuestAdded(Quest),
    QuestRemoved(u32),
    /// A quest put aside or taken up again.
    QuestActive {
        id: u32,
        active: bool,
    },
    /// Hunts' counts: objective ID, how many so far, how many in all.
    QuestHunts(Vec<(u32, u16, u16)>),
    /// The cart: how many kinds it holds and can, its weight and most weight.
    CartCount {
        count: u16,
        most: u16,
        weight: i32,
        max_weight: i32,
    },
    CartAdded(InventoryItem),
    CartRemoved {
        index: u16,
        amount: u32,
    },
    /// The player's guild: its ID, name, whether we lead it (ZC_UPDATE_GDID).
    Guild {
        id: u32,
        name: String,
        master: bool,
    },
    GuildInfo(GuildInfo),
    GuildMembers(Vec<GuildMember>),
    /// Position names, by position ID.
    GuildPositions(Vec<(u32, String)>),
    GuildNotice {
        subject: String,
        text: String,
    },
    GuildChat(String),
    /// A member logged in or out.
    GuildMemberOnline {
        account: u32,
        character: u32,
        online: bool,
    },
    /// Making a guild: 0 made, 1 already in one, 2 the name's taken, 3 no Emperium.
    GuildMade(u8),
    /// A guild invites the player.
    GuildInvite {
        id: u32,
        name: String,
    },
    /// How an invitation went: 0 they're in a guild, 1 refused, 2 joined, 3 the guild is full.
    GuildInviteResult(u8),
    /// Someone (by character ID) left or was expelled, and why.
    GuildLeft {
        character: u32,
        reason: String,
        expelled: bool,
    },
    /// The friend list, at login (everyone offline until told otherwise).
    Friends(Vec<Friend>),
    /// A friend logged in or out.
    FriendOnline {
        account: u32,
        character: u32,
        online: bool,
    },
    /// Someone asks to be friends.
    FriendAsked {
        account: u32,
        character: u32,
        name: String,
    },
    /// How asking went: 0 friends now, 1 refused, 2 our list is full, 3 theirs is.
    FriendAdded {
        result: u16,
        friend: Friend,
    },
    FriendRemoved {
        account: u32,
        character: u32,
    },
    /// The whole party (ZC_GROUP_LIST), at login and when it changes.
    Party {
        name: String,
        members: Vec<PartyMember>,
    },
    /// Someone joined or logged in or out (ZC_ADD_MEMBER_TO_GROUP); where they are, and the item rules.
    PartyMember {
        party: String,
        member: PartyMember,
        cell: [u16; 2],
        pickup: bool,
        share: bool,
    },
    /// Someone left (0) or was expelled (1); 2 and 3 mean leaving isn't allowed here.
    PartyLeft {
        account: u32,
        name: String,
        result: u8,
    },
    /// How making a party went: 0 made, 1 the name is taken, 2 already in one, 3 not on this map.
    PartyMade(u8),
    /// A party asks the player to join.
    PartyInvite {
        party: u32,
        name: String,
    },
    /// How an invitation went (`name` is who was asked); see the client's messages 80-83 and on.
    PartyInviteResult {
        name: String,
        result: u32,
    },
    PartyChat {
        account: u32,
        text: String,
    },
    /// A member's place on the minimap.
    PartyPosition {
        account: u32,
        cell: [u16; 2],
    },
    PartyHp {
        account: u32,
        hp: i32,
        max: i32,
    },
    /// Shared experience (0 no, 1 yes, 2 can't change) and the item rules.
    PartyOptions {
        exp: u32,
        pickup: bool,
        share: bool,
    },
    PartyLeader {
        old: u32,
        new: u32,
    },
    PartyMemberInfo {
        account: u32,
        job: u16,
        level: u16,
    },
    PartyDead {
        account: u32,
        dead: bool,
    },
    /// Someone asks the player for a deal: their name, character ID and level.
    TradeAsked {
        name: String,
        id: u32,
        level: u16,
    },
    /// How asking for a deal went: 0 too far, 1 no such character, 2 they're in another deal, 3 it's on, 4 refused,
    /// 5 busy.
    TradeAnswer {
        result: u8,
        id: u32,
        level: u16,
    },
    /// The partner put in an item, or zeny.
    TradeItem(InventoryItem),
    TradeZeny(u32),
    /// Whether an item went in: 0 yes, 1 too heavy for them, 2 the deal was cancelled, 3 their inventory is full,
    /// 4 too many of it.
    TradeAdded {
        index: u16,
        result: u8,
    },
    /// One side locked in (OK).
    TradeLocked {
        partner: bool,
    },
    TradeCancelled,
    TradeDone {
        ok: bool,
    },
    /// Someone whispered to the player.
    Whisper {
        from: String,
        text: String,
    },
    /// How a whisper went: 0 sent, 1 nobody of that name online, 2 ignored by them, 3 they ignore everyone.
    WhisperResult(u8),
    /// An NPC's illustration (`illust\<image>`): where (0 bottom left, 1 bottom middle, 2 bottom right, 3 and 4
    /// the middle of the screen; anything else, or no image, takes it away).
    Cutin {
        image: String,
        place: u8,
    },
    /// An NPC marks a cell on the minimap: `kind` 0 for 15 seconds, 1 until taken away, 2 takes it away.
    Mark {
        npc: u32,
        kind: u32,
        cell: [u16; 2],
        id: u8,
        color: [u8; 3],
    },
    /// A unit's looks changed (`kind` is a `LOOK_*`); for weapons `extra` is the shield.
    LookChange {
        id: u32,
        kind: u8,
        value: u32,
        extra: u32,
    },
    /// Putting on equipment: where it went, or a nonzero `result` (1 level too low, 2 can't).
    Equipped {
        index: u16,
        location: u32,
        result: u8,
    },
    /// Taking off equipment, unless `failed`.
    Unequipped {
        index: u16,
        location: u32,
        failed: bool,
    },
    /// The hotkeys of one tab (`rotate`: the row shown first).
    Hotkeys {
        tab: u16,
        rotate: u8,
        keys: Vec<Option<Hotkey>>,
    },
    /// The player's skills (ZC_SKILLINFO_LIST).
    Skills(Vec<Skill>),
    /// A skill learned or changed (level, SP, range, whether it can go up).
    SkillChanged(Skill),
    SkillRemoved(u16),
    /// A unit starts a skill (ZC_USESKILL_ACK) at a unit or a cell, casting for `cast_ms` first.
    Casting {
        source: u32,
        target: u32,
        cell: [u16; 2],
        skill: u16,
        element: u32,
        cast_ms: u32,
    },
    /// A skill hit (ZC_NOTIFY_SKILL); `act.kind` 8 splits the damage into hits.
    SkillHit {
        skill: u16,
        level: u16,
        act: Act,
    },
    /// A skill without damage, like a heal or a buff (ZC_USE_SKILL); `amount` is how much a heal healed.
    SkillUsed {
        skill: u16,
        amount: i32,
        target: u32,
        source: u32,
        ok: bool,
    },
    /// A skill aimed at a cell (ZC_NOTIFY_GROUNDSKILL).
    GroundSkill {
        skill: u16,
        source: u32,
        level: u16,
        cell: [u16; 2],
    },
    /// The player's skill didn't go off (ZC_ACK_TOUSESKILL) unless `ok`: rathena's `useskill_fail_cause`, and a
    /// skill-specific detail (`btype`).
    SkillFailed {
        skill: u16,
        detail: i32,
        ok: bool,
        cause: u8,
    },
    /// HP (5) or SP (7) restored (ZC_RECOVERY).
    Recovered {
        kind: u16,
        amount: i32,
    },
    /// A status effect began or ended on a unit (ZC_MSG_STATE_CHANGE): the client's EFST id and the time left.
    StatusIcon {
        kind: u16,
        id: u32,
        on: bool,
        total_ms: u32,
        left_ms: u32,
    },
    Disconnected(io::Error),
}

/// An item an NPC shop sells.
#[derive(Debug, Clone, PartialEq)]
pub struct ShopGoods {
    pub item: u32,
    pub price: u32,
    /// The price with the player's discount.
    pub discounted: u32,
    pub kind: u8,
}

/// A skill the player has (ZC_SKILLINFO_LIST entry).
#[derive(Debug, Clone, PartialEq)]
pub struct Skill {
    pub id: u16,
    /// rathena's `inf`: 1 targets an enemy, 2 the ground, 4 oneself, 16 a friend; 0 is passive.
    pub kind: u32,
    pub level: u16,
    pub sp: u16,
    pub range: u16,
    pub upgradable: bool,
}

impl Skill {
    pub const ENEMY: u32 = 1;
    pub const GROUND: u32 = 2;
    pub const SELF: u32 = 4;
    pub const FRIEND: u32 = 16;
}

#[derive(Debug, Clone, PartialEq)]
pub struct Act {
    pub source: u32,
    pub target: u32,
    /// Milliseconds the attacker's motion takes, then the target's flinch.
    pub source_speed: i32,
    pub target_speed: i32,
    pub damage: i32,
    /// Number of hits the damage is split into.
    pub hits: u16,
    /// 0 normal hit, 1 pick up, 2 sit, 3 stand, 8 multi hit, 10 critical, ... (rathena's e_damage_type).
    pub kind: u8,
    pub left_damage: i32,
}

/// An item lying on the map.
#[derive(Debug, Clone, PartialEq)]
pub struct GroundItem {
    pub id: u32,
    pub item: u32,
    pub identified: bool,
    pub cell: [u16; 2],
    /// Where in the cell, in twelfths.
    pub sub: [u8; 2],
    pub amount: u16,
    /// Just dropped: it falls into place.
    pub falling: bool,
}

/// An item or skill on the hotkey bar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hotkey {
    pub skill: bool,
    pub id: u32,
    /// Item amount or skill level.
    pub count: i16,
}

/// An item in a vendor's shop: what, how many, at what price, where it sits in their cart.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VendItem {
    pub index: u16,
    pub item: InventoryItem,
    pub price: u32,
}

/// A monster a quest wants hunted: its objective's ID (the quest's × 1000 + its place), the monster's name, how many
/// so far and how many in all.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Hunt {
    pub id: u32,
    pub name: String,
    pub count: u16,
    pub needed: u16,
}

/// A quest in the player's log: active (or put aside), when it runs out (Unix time, 0 never) and its hunts.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Quest {
    pub id: u32,
    pub active: bool,
    pub end: u32,
    pub hunts: Vec<Hunt>,
}

/// What a mail holds besides text (`MailHeader::flags`), and whether an NPC sent it.
pub const MAIL_ZENY: u8 = 0x2;
pub const MAIL_ITEM: u8 = 0x4;
pub const MAIL_NPC: u8 = 0x8;

/// A mail in the mailbox list.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MailHeader {
    pub id: u64,
    /// The box it's in: 0 the character's, 1 the account's, 2 returned.
    pub kind: u8,
    pub read: bool,
    /// `MAIL_*` bits.
    pub flags: u8,
    pub sender: String,
    /// Seconds until it's deleted.
    pub expires: u32,
    pub title: String,
}

/// The player's guild as its info tab shows it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GuildInfo {
    pub id: u32,
    pub level: u32,
    pub members: u32,
    pub max_members: u32,
    pub average_level: u32,
    pub exp: u32,
    pub next_exp: u32,
    pub tax_points: u32,
    pub emblem: u32,
    pub name: String,
    /// Castles owned.
    pub territory: String,
    pub master: String,
}

/// Someone in the player's guild.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GuildMember {
    pub account: u32,
    pub character: u32,
    pub job: u16,
    pub level: u16,
    pub contribution: u32,
    pub online: bool,
    pub position: u32,
    pub name: String,
}

/// Someone on the player's friend list.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Friend {
    pub account: u32,
    pub character: u32,
    pub name: String,
    pub online: bool,
}

/// Someone in the player's party.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PartyMember {
    pub account: u32,
    pub character: u32,
    pub name: String,
    /// The map they're on (`prontera.gat`).
    pub map: String,
    pub leader: bool,
    pub online: bool,
    pub job: u16,
    pub level: u16,
}

/// One entry of the player's inventory.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InventoryItem {
    /// The server's slot number, used in requests.
    pub index: u16,
    pub id: u32,
    /// Item type (`ITEM_*`).
    pub kind: u8,
    pub amount: u16,
    /// Where it can be worn; 0 for anything that isn't equipment.
    pub location: u32,
    /// Where it is worn now; 0 when it isn't.
    pub worn: u32,
    pub identified: bool,
    pub refine: u8,
    pub cards: [u32; 4],
}

fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn u64_at(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}

fn cards_at(bytes: &[u8], offset: usize) -> [u32; 4] {
    std::array::from_fn(|card| u32_at(bytes, offset + card * 4))
}

/// NORMALITEM_INFO: index, item, type, amount, worn, cards, rental end, flags (bit 0: identified).
fn decode_normal_item(entry: &[u8]) -> InventoryItem {
    InventoryItem {
        index: u16_at(entry, 0),
        id: u32_at(entry, 2),
        kind: entry[6],
        amount: u16_at(entry, 7),
        worn: u32_at(entry, 9),
        cards: cards_at(entry, 13),
        identified: entry[33] & 1 != 0,
        ..Default::default()
    }
}

/// EQUIPITEM_INFO: index, item, type, location, worn, cards, rental end, bind, look, options, refine, grade, flags.
fn decode_equip_item(entry: &[u8]) -> InventoryItem {
    InventoryItem {
        index: u16_at(entry, 0),
        id: u32_at(entry, 2),
        kind: entry[6],
        amount: 1,
        location: u32_at(entry, 7),
        worn: u32_at(entry, 11),
        cards: cards_at(entry, 15),
        refine: entry[65],
        identified: entry[67] & 1 != 0,
    }
}

/// What CZ_ENTER needs from the login and char servers.
pub struct MapTicket {
    pub address: ([u8; 4], u16),
    pub account_id: u32,
    pub character_id: u32,
    pub login_id1: u32,
    pub sex: u8,
}

pub struct MapSession {
    stream: TcpStream,
}

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
                        if let Some(event) = decode_map_packet(&packet) {
                            on_event(event);
                        }
                    }
                    Err(error) => break error,
                }
            };
            on_event(MapEvent::Disconnected(error));
        });
        Ok(Self { stream })
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
        let text = encoding_rs::EUC_KR.encode(text).0.into_owned();
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

    fn send_id(&mut self, header: u16, id: u32) -> io::Result<()> {
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

    fn move_stored(&mut self, header: u16, index: u16, amount: u32) -> io::Result<()> {
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
    pub fn use_skill_at(&mut self, skill: u16, level: u16, cell: [u16; 2]) -> io::Result<()> {
        let mut packet = CZ_USE_SKILL_TOGROUND.to_le_bytes().to_vec();
        packet.extend_from_slice(&level.to_le_bytes());
        packet.extend_from_slice(&skill.to_le_bytes());
        packet.extend_from_slice(&cell[0].to_le_bytes());
        packet.extend_from_slice(&cell[1].to_le_bytes());
        self.stream.write_all(&packet)
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

    /// Back to the save point (after dying), or to character select.
    pub fn restart(&mut self, char_select: bool) -> io::Result<()> {
        let mut packet = CZ_RESTART.to_le_bytes().to_vec();
        packet.push(u8::from(char_select));
        self.stream.write_all(&packet)
    }

    /// Asks to leave the game.
    pub fn quit(&mut self) -> io::Result<()> {
        let mut packet = CZ_REQUEST_QUIT.to_le_bytes().to_vec();
        packet.extend_from_slice(&0u16.to_le_bytes());
        self.stream.write_all(&packet)
    }

    /// Opens a chat room over the player's head.
    pub fn make_chat_room(&mut self, title: &str, limit: u16, public: bool, password: &str) -> io::Result<()> {
        let title = encoding_rs::EUC_KR.encode(title).0.into_owned();
        let mut secret = password.as_bytes().to_vec();
        secret.resize(8, 0);
        let mut packet = CZ_CREATE_CHATROOM.to_le_bytes().to_vec();
        packet.extend_from_slice(&((15 + title.len()) as u16).to_le_bytes());
        packet.extend_from_slice(&limit.to_le_bytes());
        packet.push(u8::from(public));
        packet.extend_from_slice(&secret);
        packet.extend_from_slice(&title);
        self.stream.write_all(&packet)
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
        let mut name = encoding_rs::EUC_KR.encode(title).0.into_owned();
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
    fn mail_request(&mut self, header: u16, kind: u8, id: u64) -> io::Result<()> {
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
            let mut bytes = encoding_rs::EUC_KR.encode(text).0.into_owned();
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

    /// Asks for a page of the guild window: 0 its info, 1 its members.
    pub fn guild_page(&mut self, page: u32) -> io::Result<()> {
        let mut packet = CZ_REQ_GUILD_MENU.to_le_bytes().to_vec();
        packet.extend_from_slice(&page.to_le_bytes());
        self.stream.write_all(&packet)
    }

    /// Invites a character, by name, into the guild.
    pub fn invite_to_guild(&mut self, name: &str) -> io::Result<()> {
        let mut packet = CZ_REQ_JOIN_GUILD2.to_le_bytes().to_vec();
        packet.extend_from_slice(&name_field(name));
        self.stream.write_all(&packet)
    }

    pub fn answer_guild(&mut self, guild: u32, accept: bool) -> io::Result<()> {
        let mut packet = CZ_JOIN_GUILD.to_le_bytes().to_vec();
        packet.extend_from_slice(&guild.to_le_bytes());
        packet.extend_from_slice(&u32::from(accept).to_le_bytes());
        self.stream.write_all(&packet)
    }

    pub fn leave_guild(&mut self, guild: u32, account: u32, character: u32, reason: &str) -> io::Result<()> {
        let mut packet = CZ_REQ_LEAVE_GUILD.to_le_bytes().to_vec();
        for value in [guild, account, character] {
            packet.extend_from_slice(&value.to_le_bytes());
        }
        let mut why = encoding_rs::EUC_KR.encode(reason).0.into_owned();
        why.resize(40, 0);
        packet.extend_from_slice(&why);
        self.stream.write_all(&packet)
    }

    /// Says something to the guild.
    pub fn guild_chat(&mut self, name: &str, message: &str) -> io::Result<()> {
        let text = encoding_rs::EUC_KR.encode(&format!("{name} : {message}")).0.into_owned();
        let mut packet = CZ_GUILD_CHAT.to_le_bytes().to_vec();
        packet.extend_from_slice(&((4 + text.len() + 1) as u16).to_le_bytes());
        packet.extend_from_slice(&text);
        packet.push(0);
        self.stream.write_all(&packet)
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

    /// Invites a character, by name, into the party.
    pub fn invite_to_party(&mut self, name: &str) -> io::Result<()> {
        let mut packet = CZ_PARTY_JOIN_REQ.to_le_bytes().to_vec();
        packet.extend_from_slice(&name_field(name));
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
        let text = encoding_rs::EUC_KR.encode(&format!("{name} : {message}")).0.into_owned();
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
        let mut name = encoding_rs::EUC_KR.encode(to).0.into_owned();
        name.resize(24, 0);
        let text = encoding_rs::EUC_KR.encode(message).0.into_owned();
        let mut packet = CZ_WHISPER.to_le_bytes().to_vec();
        packet.extend_from_slice(&((4 + 24 + text.len() + 1) as u16).to_le_bytes());
        packet.extend_from_slice(&name);
        packet.extend_from_slice(&text);
        packet.push(0);
        self.stream.write_all(&packet)
    }

    pub fn chat(&mut self, name: &str, message: &str) -> io::Result<()> {
        let text = encoding_rs::EUC_KR.encode(&format!("{name} : {message}")).0.into_owned();
        let mut packet = CZ_REQUEST_CHAT.to_le_bytes().to_vec();
        packet.extend_from_slice(&((4 + text.len() + 1) as u16).to_le_bytes());
        packet.extend_from_slice(&text);
        packet.push(0);
        self.stream.write_all(&packet)
    }
}

impl Drop for MapSession {
    fn drop(&mut self) {
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
    }
}

/// Packed cell position and direction (3 bytes), as rathena's WBUFPOS.
pub fn encode_position([x, y]: [u16; 2], direction: u8) -> [u8; 3] {
    [(x >> 2) as u8, ((x << 6) | ((y >> 4) & 0x3F)) as u8, ((y << 4) | u16::from(direction & 0x0F)) as u8]
}

pub fn decode_position(bytes: &[u8]) -> ([u16; 2], u8) {
    let [a, b, c] = [bytes[0], bytes[1], bytes[2]].map(u16::from);
    ([(a << 2) | (b >> 6), ((b & 0x3F) << 4) | (c >> 4)], (c & 0x0F) as u8)
}

/// Packed walk (6 bytes): start and destination cells, as rathena's WBUFPOS2.
pub fn decode_move(bytes: &[u8]) -> ([u16; 2], [u16; 2]) {
    let [a, b, c, d, e] = [bytes[0], bytes[1], bytes[2], bytes[3], bytes[4]].map(u16::from);
    ([(a << 2) | (b >> 6), ((b & 0x3F) << 4) | (c >> 4)], [((c & 0x0F) << 6) | (d >> 2), ((d & 0x03) << 8) | e])
}

/// Field offsets of ZC_NOTIFY_STANDENTRY11 / NEWENTRY11 / MOVEENTRY11 (from rathena's packets_struct.hpp).
struct UnitLayout {
    /// Where the unit's dead-or-sitting state is, if the packet has it.
    state: Option<usize>,
    hair: usize,
    accessories: [usize; 3],
    robe: usize,
    palettes: [usize; 2],
    head_direction: usize,
    sex: usize,
    level: usize,
    name: usize,
}

const STAND_LAYOUT: UnitLayout =
    UnitLayout { state: Some(68), hair: 25, accessories: [35, 37, 39], robe: 47, palettes: [41, 43], head_direction: 45, sex: 62, level: 69, name: 84 };
const NEW_LAYOUT: UnitLayout =
    UnitLayout { state: None, hair: 25, accessories: [35, 37, 39], robe: 47, palettes: [41, 43], head_direction: 45, sex: 62, level: 68, name: 83 };
const MOVE_LAYOUT: UnitLayout =
    UnitLayout { state: None, hair: 25, accessories: [35, 41, 43], robe: 51, palettes: [45, 47], head_direction: 49, sex: 66, level: 75, name: 90 };

fn decode_unit(packet: &[u8], layout: &UnitLayout, position: ([u16; 2], u8)) -> Unit {
    let u16_at = |offset: usize| u16::from_le_bytes([packet[offset], packet[offset + 1]]);
    Unit {
        // AID: account ID for players, the unit ID for everything else (GID at 9 is only set for players).
        id: u32::from_le_bytes(packet[5..9].try_into().unwrap()),
        kind: packet[4],
        speed: u16_at(13),
        job: u16_at(23),
        hair: u16_at(layout.hair),
        head_bottom: u16_at(layout.accessories[0]),
        head_top: u16_at(layout.accessories[1]),
        head_mid: u16_at(layout.accessories[2]),
        weapon: u32_at(packet, 27),
        shield: u32_at(packet, 31),
        robe: u16_at(layout.robe),
        hair_color: u16_at(layout.palettes[0]),
        clothes_color: u16_at(layout.palettes[1]),
        head_direction: u16_at(layout.head_direction),
        sex: packet[layout.sex],
        position: position.0,
        direction: position.1,
        level: u16_at(layout.level),
        name: c_string(&packet[layout.name..(layout.name + 24).min(packet.len())]),
        sitting: layout.state.is_some_and(|state| packet.get(state) == Some(&2)),
    }
}

fn decode_map_packet(packet: &[u8]) -> Option<MapEvent> {
    let u32_at = |offset: usize| u32::from_le_bytes(packet[offset..offset + 4].try_into().unwrap());
    let u16_at = |offset: usize| u16::from_le_bytes([packet[offset], packet[offset + 1]]);
    Some(match u16_at(0) {
        ZC_ACCEPT_ENTER => {
            let (position, direction) = decode_position(&packet[6..9]);
            MapEvent::Entered { position, direction }
        }
        ZC_NOTIFY_STANDENTRY => MapEvent::Unit(decode_unit(packet, &STAND_LAYOUT, decode_position(&packet[63..66]))),
        ZC_NOTIFY_NEWENTRY => MapEvent::Unit(decode_unit(packet, &NEW_LAYOUT, decode_position(&packet[63..66]))),
        ZC_NOTIFY_MOVEENTRY => {
            let (from, to) = decode_move(&packet[67..73]);
            MapEvent::UnitWalk { unit: decode_unit(packet, &MOVE_LAYOUT, (from, 0)), to, start_time: u32_at(37) }
        }
        ZC_NOTIFY_VANISH => MapEvent::Vanish { id: u32_at(2), reason: packet[6] },
        ZC_NOTIFY_PLAYERMOVE => {
            let (from, to) = decode_move(&packet[6..12]);
            MapEvent::PlayerMove { from, to, start_time: u32_at(2) }
        }
        ZC_STOPMOVE => MapEvent::Stop { id: u32_at(2), position: [u16_at(6), u16_at(8)] },
        ZC_CHANGE_DIRECTION => MapEvent::Turn { id: u32_at(2), direction: packet[8], head_direction: u16_at(6) },
        ZC_NOTIFY_CHAT => MapEvent::Chat { id: Some(u32_at(4)), message: c_string(&packet[8..]) },
        ZC_NOTIFY_PLAYERCHAT => MapEvent::Chat { id: None, message: c_string(&packet[4..]) },
        ZC_NPCACK_MAPMOVE => MapEvent::MapChange { map: c_string(&packet[2..18]), position: [u16_at(18), u16_at(20)] },
        ZC_SAY_DIALOG => MapEvent::NpcSay { npc: u32_at(4), text: c_string(&packet[8..]) },
        ZC_WAIT_DIALOG => MapEvent::NpcNext { npc: u32_at(2) },
        ZC_CLOSE_DIALOG => MapEvent::NpcClose { npc: u32_at(2) },
        ZC_CLEAR_DIALOG => MapEvent::NpcClear { npc: u32_at(2) },
        ZC_MENU_LIST => MapEvent::NpcMenu { npc: u32_at(4), items: c_string(&packet[8..]).split(':').map(str::to_owned).collect() },
        ZC_OPEN_EDITDLG => MapEvent::NpcInput { npc: u32_at(2), text: false },
        ZC_OPEN_EDITDLGSTR => MapEvent::NpcInput { npc: u32_at(2), text: true },
        ZC_NOTIFY_ACT => MapEvent::Act(Act {
            source: u32_at(2),
            target: u32_at(6),
            source_speed: u32_at(14) as i32,
            target_speed: u32_at(18) as i32,
            damage: u32_at(22) as i32,
            hits: u16_at(27),
            kind: packet[29],
            left_damage: u32_at(30) as i32,
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
        ZC_USESKILL_ACK => MapEvent::Casting {
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
                hits: u16_at(30),
                kind: packet[32],
                left_damage: 0,
            },
        },
        ZC_USE_SKILL => MapEvent::SkillUsed { skill: u16_at(2), amount: u32_at(4) as i32, target: u32_at(8), source: u32_at(12), ok: packet[16] != 0 },
        ZC_NOTIFY_GROUNDSKILL => MapEvent::GroundSkill { skill: u16_at(2), source: u32_at(4), level: u16_at(8), cell: [u16_at(10), u16_at(12)] },
        ZC_ACK_TOUSESKILL => MapEvent::SkillFailed { skill: u16_at(2), detail: u32_at(4) as i32, ok: packet[12] != 0, cause: packet[13] },
        ZC_RECOVERY => MapEvent::Recovered { kind: u16_at(2), amount: u32_at(4) as i32 },
        ZC_MSG_STATE_CHANGE3 => MapEvent::StatusIcon { kind: u16_at(2), id: u32_at(4), on: packet[8] != 0, total_ms: u32_at(9), left_ms: u32_at(13) },
        ZC_MSG_STATE_CHANGE2 => MapEvent::StatusIcon { kind: u16_at(2), id: u32_at(4), on: packet[8] != 0, total_ms: u32_at(9), left_ms: u32_at(9) },
        ZC_MSG_STATE_CHANGE => MapEvent::StatusIcon { kind: u16_at(2), id: u32_at(4), on: packet[8] != 0, total_ms: 0, left_ms: 0 },
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
        }),
        ZC_ITEM_FALL_ENTRY => MapEvent::GroundItem(GroundItem {
            id: u32_at(2),
            item: u32_at(6),
            identified: packet[12] != 0,
            cell: [u16_at(13), u16_at(15)],
            sub: [packet[17], packet[18]],
            amount: u16_at(19),
            falling: true,
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
                worn: 0,
            },
        },
        ZC_INVENTORY_START => MapEvent::InventoryStart { kind: packet[4] },
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
            refine: packet[56],
            ..Default::default()
        }),
        ZC_DELETE_ITEM_FROM_STORE => MapEvent::StorageRemoved { index: u16_at(2), amount: u32_at(4) },
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
                        location: self::u32_at(entry, 56),
                        refine: entry[62],
                        ..Default::default()
                    },
                })
                .collect(),
        },
        ZC_PC_PURCHASE_RESULT_FROMMC => MapEvent::VendBought { index: u16_at(2), amount: u16_at(4), result: packet[6] },
        ZC_NOTIFY_UNREADMAIL => MapEvent::MailUnread(packet[2] != 0),
        // Each quest: ID, state (1 active), start and end times, hunts; each hunt 46 bytes, with how many it needs.
        ZC_ALL_QUEST_LIST3 => {
            let mut quests = Vec::new();
            let mut at = 8;
            while at + 15 <= packet.len() {
                let count = usize::from(u16_at(at + 13));
                let hunts = packet[at + 15..].chunks_exact(46).take(count).map(|hunt| Hunt {
                    id: self::u32_at(hunt, 0),
                    count: self::u16_at(hunt, 16),
                    needed: self::u16_at(hunt, 18),
                    name: c_string(&hunt[20..44]),
                });
                quests.push(Quest { id: u32_at(at), active: packet[at + 4] == 1, end: u32_at(at + 9), hunts: hunts.collect() });
                at += 15 + count * 46;
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
            });
            MapEvent::QuestAdded(Quest { id: u32_at(2), active: packet[6] == 1, end: u32_at(11), hunts: hunts.collect() })
        }
        ZC_HUNTING_QUEST_INFO => {
            MapEvent::QuestHunts(packet[4..].chunks_exact(12).map(|hunt| (self::u32_at(hunt, 0), self::u16_at(hunt, 10), self::u16_at(hunt, 8))).collect())
        }
        ZC_UPDATE_MISSION_HUNT_EX => {
            MapEvent::QuestHunts(packet[6..].chunks_exact(12).map(|hunt| (self::u32_at(hunt, 4), self::u16_at(hunt, 10), self::u16_at(hunt, 8))).collect())
        }
        ZC_DEL_QUEST => MapEvent::QuestRemoved(u32_at(2)),
        ZC_ACTIVE_QUEST => MapEvent::QuestActive { id: u32_at(2), active: packet[6] != 0 },
        // Each mail: box, ID, read, flags, sender, seconds left, title (its length first).
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
                    cards: cards_at(entry, 8),
                    location: self::u32_at(entry, 24),
                    kind: entry[28],
                    refine: entry[58],
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
            refine: packet[56],
            ..Default::default()
        }),
        ZC_UPDATE_GDID => MapEvent::Guild { id: u32_at(2), name: c_string(&packet[19..43]), master: packet[14] != 0 },
        // ID, level, members, most members, average level, exp, next exp, tax points, honor, virtue, emblem, name,
        // castles, zeny, master's ID and name.
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
                    job: self::u16_at(member, 14),
                    level: self::u16_at(member, 16),
                    contribution: self::u32_at(member, 18),
                    online: self::u32_at(member, 22) != 0,
                    position: self::u32_at(member, 26),
                    name: c_string(&member[34..58]),
                })
                .collect(),
        ),
        ZC_POSITION_ID_NAME_INFO => {
            MapEvent::GuildPositions(packet[4..].chunks_exact(28).map(|entry| (self::u32_at(entry, 0), c_string(&entry[4..28]))).collect())
        }
        ZC_GUILD_NOTICE => MapEvent::GuildNotice { subject: c_string(&packet[2..62]), text: c_string(&packet[62..182]) },
        ZC_GUILD_CHAT => MapEvent::GuildChat(c_string(&packet[4..])),
        ZC_UPDATE_CHARSTAT => MapEvent::GuildMemberOnline { account: u32_at(2), character: u32_at(6), online: u32_at(10) != 0 },
        ZC_RESULT_MAKE_GUILD => MapEvent::GuildMade(packet[2]),
        ZC_REQ_JOIN_GUILD => MapEvent::GuildInvite { id: u32_at(2), name: c_string(&packet[6..30]) },
        ZC_ACK_REQ_JOIN_GUILD => MapEvent::GuildInviteResult(packet[2]),
        ZC_ACK_LEAVE_GUILD => MapEvent::GuildLeft { character: u32_at(2), reason: c_string(&packet[6..46]), expelled: false },
        ZC_ACK_BAN_GUILD => MapEvent::GuildLeft { character: u32_at(42), reason: c_string(&packet[2..42]), expelled: true },
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
        ZC_STATUS_CHANGE_ACK if packet[4] != 0 => MapEvent::Statuses(vec![(u16_at(2), i64::from(packet[5]))]),
        ZC_STATUS_CHANGE_ACK => MapEvent::Statuses(Vec::new()),
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
        ZC_PARTY_JOIN_REQ => MapEvent::PartyInvite { party: u32_at(2), name: c_string(&packet[6..30]) },
        ZC_PARTY_JOIN_REQ_ACK => MapEvent::PartyInviteResult { name: c_string(&packet[2..26]), result: u32_at(26) },
        ZC_NOTIFY_CHAT_PARTY => MapEvent::PartyChat { account: u32_at(4), text: c_string(&packet[8..]) },
        ZC_NOTIFY_POSITION_TO_GROUPM => MapEvent::PartyPosition { account: u32_at(2), cell: [u16_at(6), u16_at(8)] },
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
            location: u32_at(54),
            refine: packet[60],
            ..Default::default()
        }),
        ZC_ACK_ADD_EXCHANGE_ITEM => MapEvent::TradeAdded { index: u16_at(2), result: packet[4] },
        ZC_CONCLUDE_EXCHANGE_ITEM => MapEvent::TradeLocked { partner: packet[2] != 0 },
        ZC_CANCEL_EXCHANGE_ITEM => MapEvent::TradeCancelled,
        ZC_EXEC_EXCHANGE_ITEM => MapEvent::TradeDone { ok: packet[2] == 0 },
        ZC_SHOW_IMAGE => MapEvent::Cutin { image: c_string(&packet[2..66]), place: packet[66] },
        ZC_COMPASS => MapEvent::Mark {
            npc: u32_at(2),
            kind: u32_at(6),
            cell: [u32_at(10) as u16, u32_at(14) as u16],
            id: packet[18],
            color: [packet[21], packet[20], packet[19]],
        },
        ZC_NOTIFY_EFFECT2 | ZC_NOTIFY_EFFECT3 => MapEvent::Effect { id: u32_at(2), effect: u32_at(6) },
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
        ZC_REQ_WEAR_EQUIP_ACK => MapEvent::Equipped { index: u16_at(2), location: u32_at(4), result: packet[10] },
        // Since 2011 the flag is inverted: 0 means it came off.
        ZC_REQ_TAKEOFF_EQUIP_ACK => MapEvent::Unequipped { index: u16_at(2), location: u32_at(4), failed: packet[8] != 0 },
        ZC_ATTACK_FAILURE_FOR_DISTANCE => {
            MapEvent::TooFar { target: u32_at(2), target_cell: [u16_at(6), u16_at(8)], cell: [u16_at(10), u16_at(12)], range: u16_at(14) }
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn decodes_accept_and_refuse() {
        let mut packet = vec![0u8; ACCEPT_HEADER + CHAR_SERVER_SIZE];
        packet[..2].copy_from_slice(&AC_ACCEPT_LOGIN.to_le_bytes());
        packet[2..4].copy_from_slice(&((ACCEPT_HEADER + CHAR_SERVER_SIZE) as u16).to_le_bytes());
        packet[8..12].copy_from_slice(&2_000_000u32.to_le_bytes());
        packet[46] = 1;
        let server = &mut packet[ACCEPT_HEADER..];
        server[..4].copy_from_slice(&[127, 0, 0, 1]);
        server[4..6].copy_from_slice(&6121u16.to_le_bytes());
        server[6..13].copy_from_slice(b"rAthena");
        server[26..28].copy_from_slice(&3u16.to_le_bytes());

        let accepted = decode_reply(&packet).unwrap();
        assert_eq!(accepted.account_id, 2_000_000);
        assert_eq!(accepted.sex, 1);
        assert_eq!(accepted.char_servers, vec![CharServer { ip: [127, 0, 0, 1], port: 6121, name: "rAthena".into(), users: 3 }]);

        let mut refuse = vec![0u8; 26];
        refuse[..2].copy_from_slice(&AC_REFUSE_LOGIN.to_le_bytes());
        refuse[2] = 1;
        assert!(matches!(decode_reply(&refuse), Err(LoginError::Refused(1))));
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
    fn decodes_ground_items_and_inventory() {
        // Captured from rathena: a Poring dropping a Jellopy (909) at 252,88.
        let jellopy = [0xdd, 0x0a, 0x60, 0x04, 0, 0, 0x8d, 0x03, 0, 0, 0x03, 0, 0x01, 0xfc, 0, 0x58, 0, 0x06, 0x0c, 0x01, 0, 0, 0, 0];
        let Some(MapEvent::GroundItem(item)) = decode_map_packet(&jellopy) else { panic!() };
        assert_eq!(item, GroundItem { id: 0x460, item: 909, identified: true, cell: [252, 88], sub: [6, 12], amount: 1, falling: true });
        assert_eq!(packet_length(ZC_ITEM_FALL_ENTRY), Some(jellopy.len() as i32));

        let mut list = vec![0u8; 5 + 2 * NORMAL_ITEM_SIZE];
        list[..2].copy_from_slice(&ZC_INVENTORY_ITEMLIST_NORMAL.to_le_bytes());
        for (slot, (index, id, amount)) in [(2u16, 501u32, 7u16), (3, 909, 12)].into_iter().enumerate() {
            let entry = &mut list[5 + slot * NORMAL_ITEM_SIZE..][..NORMAL_ITEM_SIZE];
            entry[..2].copy_from_slice(&index.to_le_bytes());
            entry[2..6].copy_from_slice(&id.to_le_bytes());
            entry[7..9].copy_from_slice(&amount.to_le_bytes());
            entry[33] = 1;
        }
        let Some(MapEvent::InventoryItems { kind: 0, items }) = decode_map_packet(&list) else { panic!() };
        assert_eq!(items.iter().map(|item| (item.index, item.id, item.amount, item.identified)).collect::<Vec<_>>(), [(2, 501, 7, true), (3, 909, 12, true)]);

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
        assert_eq!(packet_length(ZC_REQ_WEAR_EQUIP_ACK), Some(11));
        assert_eq!(packet_length(ZC_REQ_TAKEOFF_EQUIP_ACK), Some(9));
    }

    #[test]
    fn decodes_quests() {
        let mut list = ZC_ALL_QUEST_LIST3.to_le_bytes().to_vec();
        list.extend_from_slice(&[0, 0, 1, 0, 0, 0]);
        list.extend_from_slice(&7001u32.to_le_bytes());
        list.push(1);
        list.extend_from_slice(&[0; 8]);
        list.extend_from_slice(&1u16.to_le_bytes());
        let mut hunt = [0u8; 46];
        hunt[..4].copy_from_slice(&7_001_000u32.to_le_bytes());
        hunt[16..18].copy_from_slice(&2u16.to_le_bytes());
        hunt[18..20].copy_from_slice(&10u16.to_le_bytes());
        hunt[20..26].copy_from_slice(b"Poring");
        list.extend_from_slice(&hunt);
        let length = list.len() as u16;
        list[2..4].copy_from_slice(&length.to_le_bytes());
        let Some(MapEvent::Quests(quests)) = decode_map_packet(&list) else { panic!() };
        assert_eq!(quests, [Quest { id: 7001, active: true, end: 0, hunts: vec![Hunt { id: 7_001_000, name: "Poring".into(), count: 2, needed: 10 }] }]);

        let mut update = ZC_UPDATE_MISSION_HUNT_EX.to_le_bytes().to_vec();
        update.extend_from_slice(&[18, 0, 1, 0]);
        for value in [7001u32, 7_001_000] {
            update.extend_from_slice(&value.to_le_bytes());
        }
        update.extend_from_slice(&10u16.to_le_bytes());
        update.extend_from_slice(&3u16.to_le_bytes());
        let Some(MapEvent::QuestHunts(hunts)) = decode_map_packet(&update) else { panic!() };
        assert_eq!(hunts, [(7_001_000, 3, 10)]);
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
                CharEvent::Characters { .. } => continue,
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
        match receiver.recv_timeout(Duration::from_secs(10)).unwrap() {
            MapEvent::Entered { position, .. } => assert!(position[0] > 0 && position[1] > 0),
            other => panic!("{other:?}"),
        }
        map.loaded().unwrap();
        // The server answers with what is in view; at least some packet must decode without breaking the stream.
        let first = receiver.recv_timeout(Duration::from_secs(10)).unwrap();
        assert!(!matches!(first, MapEvent::Disconnected(_)), "{first:?}");
        drop(map);

        assert!(matches!(login("127.0.0.1:6900", 0, user, "wrong password"), Err(LoginError::Refused(1))));
    }
}
