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
const CH_REQ_CHANGE_CHARACTER_SLOT: u16 = 0x08D4;
const HC_ACK_CHANGE_CHARACTER_SLOT: u16 = 0x0B70;
const CH_REQ_CHANGE_CHARNAME: u16 = 0x08FC;
const HC_ACK_CHANGE_CHARNAME: u16 = 0x08FD;
/// HC_ACCEPT_ENTER before its character list.
const ACCEPT_ENTER_HEADER: usize = 27;
/// CHARACTER_INFO for PACKETVER_MAIN_NUM >= 20220330 (64-bit exp, HP and SP).
const CHARACTER_INFO_SIZE: usize = 175;

#[derive(Debug, Clone, PartialEq, Default)]
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
    /// Slot moves left (0 when moving is off, 1 when unlimited).
    pub moves: i32,
    /// 1 when a rename is granted.
    pub renames: i32,
    /// The opposite of `renames`: no rename granted (or it was used).
    pub name_changed: bool,
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
    /// HC_ACK_CHANGE_CHARACTER_SLOT: 0 moved (the list follows), 1 failed; the moves the character has left.
    Moved {
        result: u16,
        moves: u16,
    },
    /// HC_ACK_CHANGE_CHARNAME: 0 renamed (the list follows), else the reason (1 renamed before, 2 bad account,
    /// 3 failed, 4 name taken, 5 in a guild, 6 in a party, 7 too long, 8 bad characters, 9 prohibited, 10 unknown).
    Renamed(u32),
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

    /// Moves the character in slot `from` to `to` (swapping with one there); the answer is a `Moved` event.
    pub fn move_slot(&mut self, from: u16, to: u16, moves: u16) -> io::Result<()> {
        self.stream.write_all(&encode_move_slot(from, to, moves))
    }

    /// Renames a character; the answer is a `Renamed` event.
    pub fn rename(&mut self, id: u32, name: &str) -> io::Result<()> {
        self.stream.write_all(&encode_rename(id, name))
    }

    fn send_id(&mut self, header: u16, id: u32, rest: &[u8]) -> io::Result<()> {
        let mut packet = header.to_le_bytes().to_vec();
        packet.extend_from_slice(&id.to_le_bytes());
        packet.extend_from_slice(rest);
        self.stream.write_all(&packet)
    }
}

/// CH_REQ_CHANGE_CHARACTER_SLOT: from, to and the moves left (rathena ignores them).
fn encode_move_slot(from: u16, to: u16, moves: u16) -> [u8; 8] {
    let [[h0, h1], [f0, f1], [t0, t1], [m0, m1]] = [CH_REQ_CHANGE_CHARACTER_SLOT, from, to, moves].map(u16::to_le_bytes);
    [h0, h1, f0, f1, t0, t1, m0, m1]
}

/// CH_REQ_CHANGE_CHARNAME: the character and its new name (24 bytes EUC-KR, NUL-terminated).
fn encode_rename(id: u32, name: &str) -> Vec<u8> {
    let mut field = name_field(name);
    field[23] = 0;
    let mut packet = CH_REQ_CHANGE_CHARNAME.to_le_bytes().to_vec();
    packet.extend_from_slice(&id.to_le_bytes());
    packet.extend_from_slice(&field);
    packet
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
        HC_ACK_CHANGE_CHARACTER_SLOT => Some(CharEvent::Moved { result: u16_at(packet, 4), moves: u16_at(packet, 6) }),
        HC_ACK_CHANGE_CHARNAME => Some(CharEvent::Renamed(u32_at(packet, 2))),
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
        moves: i32_at(166),
        renames: i32_at(170),
        name_changed: i16_at(140) != 0,
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
const CZ_REQ_USER_COUNT: u16 = 0x00C1;
const ZC_USER_COUNT: u16 = 0x00C2;
const CZ_SETTING_WHISPER_PC: u16 = 0x00CF;
const CZ_SETTING_WHISPER_STATE: u16 = 0x00D0;
const ZC_SETTING_WHISPER_PC: u16 = 0x00D1;
const ZC_SETTING_WHISPER_STATE: u16 = 0x00D2;
const CZ_REQ_WHISPER_LIST: u16 = 0x00D3;
const ZC_WHISPER_LIST: u16 = 0x00D4;
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
const CZ_ITEM_THROW: u16 = 0x0363;
const ZC_AUTORUN_SKILL: u16 = 0x0147;
const ZC_ITEMIDENTIFY_LIST: u16 = 0x0177;
const CZ_REQ_ITEMIDENTIFY: u16 = 0x0178;
const ZC_ACK_ITEMIDENTIFY: u16 = 0x0179;
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
const ZC_SKILL_ENTRY: u16 = 0x09CA;
const ZC_SKILL_DISAPPEAR: u16 = 0x0120;
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
const ZC_BROADCAST: u16 = 0x009A;
const ZC_BROADCAST2: u16 = 0x01C3;
const ZC_MSG: u16 = 0x0291;
const ZC_MSG_VALUE: u16 = 0x07E2;
const ZC_MSG_SKILL: u16 = 0x07E6;
const ZC_MSG_COLOR: u16 = 0x09CD;
const ZC_ACTION_FAILURE: u16 = 0x013B;
const ZC_ACK_REMEMBER_WARPPOINT: u16 = 0x011E;
const ZC_WARPLIST: u16 = 0x0ABE;
const ZC_SOUND: u16 = 0x01D3;
const ZC_PLAY_NPC_BGM: u16 = 0x07FE;
const CZ_SELECT_WARPPOINT: u16 = 0x011B;
const CZ_REMEMBER_WARPPOINT: u16 = 0x011D;
const ZC_NOTIFY_MAPINFO: u16 = 0x0189;
const ZC_NPC_CHAT: u16 = 0x02C1;
const ZC_START_CAPTURE: u16 = 0x019E;
const CZ_TRYCAPTURE_MONSTER: u16 = 0x019F;
const ZC_TRYCAPTURE_MONSTER: u16 = 0x01A0;
const ZC_PETEGG_LIST: u16 = 0x01A6;
const CZ_SELECT_PETEGG: u16 = 0x01A7;
const ZC_CHANGESTATE_PET: u16 = 0x01A4;
const ZC_PROPERTY_PET: u16 = 0x01A2;
const ZC_FEED_PET: u16 = 0x01A3;
const CZ_COMMAND_PET: u16 = 0x01A1;
const CZ_RENAME_PET: u16 = 0x01A5;
const ZC_PET_ACT: u16 = 0x01AA;
const CZ_PET_ACT: u16 = 0x01A9;
const ZC_ACK_REQNAMEALL_NPC: u16 = 0x0ADF;
const ZC_OPEN_REFINING_UI: u16 = 0x0AA0;
const CZ_REFINING_SELECT_ITEM: u16 = 0x0AA1;
const ZC_REFINING_MATERIAL_LIST: u16 = 0x0AA2;
const CZ_REQ_REFINING: u16 = 0x0AA3;
const CZ_CLOSE_REFINING_UI: u16 = 0x0AA4;
const ZC_ACK_ITEMREFINING: u16 = 0x0188;
const ZC_AUTOSPELLLIST: u16 = 0x0AFB;
const CZ_SELECTAUTOSPELL: u16 = 0x01CE;
const ZC_SKILL_SELECT_REQUEST: u16 = 0x0442;
const CZ_SKILL_SELECT_RESPONSE: u16 = 0x0443;
const ZC_REPAIRITEMLIST: u16 = 0x0B65;
const CZ_REQ_ITEMREPAIR2: u16 = 0x0B66;
const ZC_ACK_ITEMREPAIR: u16 = 0x01FE;
const ZC_NOTIFY_WEAPONITEMLIST: u16 = 0x0221;
const CZ_REQ_WEAPONREFINE: u16 = 0x0222;
const ZC_ACK_WEAPONREFINE: u16 = 0x0223;
const ZC_MAKABLEITEMLIST: u16 = 0x018D;
const CZ_REQMAKINGITEM: u16 = 0x018E;
const ZC_ACK_REQMAKINGITEM: u16 = 0x018F;
const ZC_MAKINGITEM_LIST: u16 = 0x025A;
const CZ_REQ_MAKINGITEM: u16 = 0x025B;
const ZC_MAKINGARROW_LIST: u16 = 0x01AD;
const CZ_REQ_MAKINGARROW: u16 = 0x01AE;
const ZC_NPC_MARKET_OPEN: u16 = 0x0B7A;
const CZ_NPC_MARKET_PURCHASE: u16 = 0x09D6;
const ZC_NPC_MARKET_PURCHASE_RESULT: u16 = 0x0B4E;
/// The market window closed (rathena gives it no name).
const CZ_NPC_MARKET_CLOSE: u16 = 0x09D8;
const ZC_NPC_BARTER_MARKET_ITEMINFO: u16 = 0x0B78;
const CZ_NPC_BARTER_MARKET_PURCHASE: u16 = 0x0B0F;
const CZ_NPC_BARTER_MARKET_CLOSE: u16 = 0x0B12;
const ZC_NPC_EXPANDED_BARTER_MARKET_ITEMINFO: u16 = 0x0B79;
const CZ_NPC_EXPANDED_BARTER_MARKET_PURCHASE: u16 = 0x0B57;
const CZ_NPC_EXPANDED_BARTER_MARKET_CLOSE: u16 = 0x0B58;
/// A market's or barter's stock that doesn't run out.
const UNLIMITED: u32 = u32::MAX;
const CZ_REQ_OPEN_MSGBOX_EXTEND_BODYITEM_SIZE: u16 = 0x0B14;
const ZC_ACK_OPEN_MSGBOX_EXTEND_BODYITEM_SIZE: u16 = 0x0B15;
const CZ_REQ_EXTEND_BODYITEM_SIZE: u16 = 0x0B16;
const ZC_ACK_EXTEND_BODYITEM_SIZE: u16 = 0x0B17;
const CZ_CLOSE_MSGBOX_EXTEND_BODYITEM_SIZE: u16 = 0x0B19;
const CZ_EQUIPWIN_MICROSCOPE: u16 = 0x02D6;
const ZC_EQUIPWIN_MICROSCOPE: u16 = 0x0B37;
const CZ_CONFIG: u16 = 0x02D8;
const ZC_CONFIG: u16 = 0x02D9;
const ZC_CONFIG_NOTIFY: u16 = 0x02DA;
/// CZ_CONFIG's setting for the Show Equipment box (rathena's CONFIG_OPEN_EQUIPMENT_WINDOW).
const CONFIG_OPEN_EQUIPMENT: u32 = 0;
const CZ_REQ_OPEN_BANKING: u16 = 0x09B6;
const ZC_ACK_OPEN_BANKING: u16 = 0x09B7;
const CZ_REQ_BANKING_CHECK: u16 = 0x09AB;
const ZC_BANKING_CHECK: u16 = 0x09A6;
const CZ_REQ_BANKING_DEPOSIT: u16 = 0x09A7;
const ZC_ACK_BANKING_DEPOSIT: u16 = 0x09A8;
const CZ_REQ_BANKING_WITHDRAW: u16 = 0x09A9;
const ZC_ACK_BANKING_WITHDRAW: u16 = 0x09AA;
const CZ_REQ_CLOSE_BANKING: u16 = 0x09B8;
const ZC_SPIRITS: u16 = 0x01D0;
const CZ_REQ_ITEMCOMPOSITION_LIST: u16 = 0x017A;
const ZC_ITEMCOMPOSITION_LIST: u16 = 0x017B;
const CZ_REQ_ITEMCOMPOSITION: u16 = 0x017C;
const ZC_ACK_ITEMCOMPOSITION: u16 = 0x017D;
const ZC_MVP_GETTING_ITEM: u16 = 0x010A;
const ZC_MVP_GETTING_SPECIAL_EXP: u16 = 0x010B;
const ZC_MVP: u16 = 0x010C;
const ZC_THROW_MVPITEM: u16 = 0x010D;
const ZC_BROADCASTING_SPECIAL_ITEM_OBTAIN: u16 = 0x07FD;
const ZC_BROADCAST_ITEMREFINING_RESULT: u16 = 0x0ADA;
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
const ZC_POSITION_INFO: u16 = 0x0160;
const ZC_GUILD_SKILLINFO: u16 = 0x0162;
const ZC_BAN_LIST: u16 = 0x0B7C;
const CZ_REQ_GUILD_EMBLEM_IMG: u16 = 0x0151;
const ZC_GUILD_EMBLEM_IMG: u16 = 0x0152;
const ZC_CHANGE_GUILD: u16 = 0x0B47;
const CZ_REQNAME: u16 = 0x0368;
const ZC_ACK_REQNAMEALL: u16 = 0x0A30;
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
    /// A player's guild and its emblem's version (0 for none).
    pub guild: u32,
    pub emblem: u32,
}

/// What ZC_CHANGESTATE_PET says: the pet is ours, its intimacy (0–1000), hunger (0–100), accessory (item ID),
/// a performance (1–3, 4 special), or the egg list is done.
pub const PET_OWNED: u8 = 0;
pub const PET_INTIMACY: u8 = 1;
pub const PET_HUNGER: u8 = 2;
pub const PET_ACCESSORY: u8 = 3;
pub const PET_PERFORMANCE: u8 = 4;
pub const PET_EGGS_DONE: u8 = 6;
/// CZ_COMMAND_PET's commands.
pub const PET_INFO: u8 = 0;
pub const PET_FEED: u8 = 1;
pub const PET_PERFORM: u8 = 2;
pub const PET_RETURN: u8 = 3;
pub const PET_UNEQUIP: u8 = 4;

/// The player's pet as ZC_PROPERTY_PET has it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Pet {
    pub name: String,
    /// Renamed once already: it can't be again.
    pub renamed: bool,
    pub level: u16,
    /// 0–100.
    pub hunger: u16,
    /// 0–1000.
    pub intimacy: u16,
    /// The accessory's item ID (0: none).
    pub accessory: u16,
    /// The monster it is.
    pub monster: u16,
}

/// What a pet does (ZC_PET_ACT, CZ_PET_ACT): below 100 an emotion (ZC_EMOTION's numbers), else a line of its
/// talk table, sent as (monster − 100) × 100 + act × 10 + hunger band.
#[derive(Debug, PartialEq)]
pub enum PetAct {
    Emotion(u8),
    Talk { monster: u16, act: u8, hunger: u8 },
}

impl PetAct {
    pub fn decode(value: i32) -> Self {
        if value < 100 {
            return Self::Emotion(value.max(0) as u8);
        }
        Self::Talk { monster: (value / 100 + 100) as u16, act: (value / 10 % 10) as u8, hunger: (value % 10) as u8 }
    }

    pub fn value(&self) -> i32 {
        match *self {
            Self::Emotion(kind) => i32::from(kind),
            Self::Talk { monster, act, hunger } => (i32::from(monster) - 100) * 100 + i32::from(act) * 10 + i32::from(hunger),
        }
    }
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
    /// Chat from another unit, or our own (id None); ZC_NPC_CHAT's comes in a color.
    Chat {
        id: Option<u32>,
        message: String,
        color: Option<[u8; 3]>,
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
    /// An announcement: plain ones yellow (or blue when the server asks), ZC_BROADCAST2 in its own color.
    Broadcast {
        text: String,
        color: Option<[u8; 3]>,
        blue: bool,
    },
    /// Someone got a special item: out of a box (its item), from a monster (its name), or from an NPC (neither).
    ItemObtained {
        item: u32,
        player: String,
        box_item: Option<u32>,
        monster: Option<String>,
    },
    /// Someone's refine at a high level: the new level, or the one before a failure.
    RefineAnnounced {
        player: String,
        item: u32,
        level: u8,
        success: bool,
    },
    /// A boss's MVP: the player (account ID) who did the most against it.
    Mvp(u32),
    /// The MVP's reward: an item, or EXP; or a reward too heavy to take.
    MvpItem(u32),
    MvpExp(u32),
    MvpOverweight,
    /// The carried equipment a card can go into (inventory indexes).
    CardList(Vec<u16>),
    /// A card put into equipment, or not; rathena takes the card away without saying so.
    CardInserted {
        equip: u16,
        card: u16,
        ok: bool,
    },
    /// How many spirit spheres (or coins) a unit carries; 0 takes them away.
    Spirits {
        id: u32,
        count: u16,
    },
    /// The bank opens (rathena answers in its own words instead where banking is off).
    BankOpened,
    /// What the account keeps.
    BankBalance(i64),
    /// A deposit's or withdrawal's answer: rathena's reason (0 done), the balance and the zeny carried after.
    BankMoved {
        deposit: bool,
        reason: u16,
        balance: i64,
        zeny: i32,
    },
    /// Another player's equipment, as asked for.
    OtherEquipment(Equipped),
    /// The Show Equipment box: the server's answer to a click, or the saved state at a map change.
    ShowEquipment {
        shown: bool,
        answer: bool,
    },
    /// May the inventory grow: 0 ask (with the voucher it would use), 1 failed, 2 busy, 3 no voucher, 4 at the most.
    ExpansionAsked {
        result: u8,
        voucher: u32,
    },
    /// The inventory grew (0), or why not (as above).
    ExpansionDone(u8),
    /// A market or barter opens with what it offers.
    Market {
        kind: MarketKind,
        goods: Vec<Bartered>,
    },
    /// A market purchase: done, with what was bought (item, amount), or failed.
    MarketBought {
        ok: bool,
        items: Vec<(u32, u16)>,
    },
    /// What can be made (Pharmacy, forging, rune stones...): the items' IDs.
    MakeList(Vec<u32>),
    /// A cooking kit's (or another) making list: its type and the items.
    CookList {
        kind: u16,
        items: Vec<u32>,
    },
    /// Arrow Crafting's (or a converter's, a poison's) list: the items it can use.
    ArrowList(Vec<u32>),
    /// Something made, or not: even results succeeded, odd ones failed.
    Made {
        result: u16,
        item: u32,
    },
    /// Repair Weapon's list: the target's broken items, indexed by rathena's own slots in the target's inventory.
    RepairList(Vec<InventoryItem>),
    /// An item repaired, or not.
    Repaired {
        index: u16,
        ok: bool,
    },
    /// Upgrade Weapon's list: the caster's weapons it can refine.
    UpgradeList(Vec<InventoryItem>),
    /// Upgrade Weapon's result (0 refined, 1 failed, 2 skill too low, 3 ore missing) with the item it names.
    Upgraded {
        result: u32,
        item: u32,
    },
    /// Hindsight's spells to pick from.
    SpellList(Vec<u32>),
    /// Skills to pick from for a reason (1: Shadow Spell), which goes back with the answer.
    SkillSelect {
        why: u32,
        skills: Vec<u16>,
    },
    /// The refine window opens.
    RefineOpen,
    /// What an item in the refine window can be refined with: ores (item, chance %, zeny), and how many Blacksmith's
    /// Blessings protect it (0: they can't).
    RefineMaterials {
        index: u16,
        blessings: u8,
        materials: Vec<(u32, u8, u32)>,
    },
    /// A refine: 0 done, 1 failed and broke, 2 failed and went down, 3 failed; the item's level after.
    Refined {
        result: u16,
        index: u16,
        refine: u16,
    },
    /// Eggs that can hatch, by inventory index.
    PetEggs(Vec<u16>),
    /// A pet's state changed: `kind` is one of the PET_* states above.
    PetState {
        kind: u8,
        id: u32,
        value: i32,
    },
    /// What the player's pet is like now.
    PetInfo(Pet),
    /// The pet was fed (or there was no food), and its food.
    PetFed {
        ok: bool,
        food: u32,
    },
    /// A pet emotes or talks.
    PetAct {
        id: u32,
        act: PetAct,
    },
    /// A unit's new name (a renamed pet).
    UnitName {
        id: u32,
        name: String,
    },
    /// A taming item was used: pick the monster to tame.
    CaptureStart,
    /// The monster was tamed (its egg comes next), or not.
    CaptureResult(bool),
    /// One of the client's messages, by number: with a number for its `%d`, a skill for its `%s`, in a color.
    Message {
        id: usize,
        value: Option<i32>,
        skill: Option<u16>,
        color: Option<[u8; 3]>,
    },
    /// Ammunition or weight trouble: 0 no ammunition, 1 and 2 too heavy, 3 ammunition equipped.
    ActionFailure(u16),
    /// A sound a script plays (`data\wav\<file>`) from where a unit stands: act 0 once, 1 on repeat, 2 stops it.
    Sound {
        file: String,
        act: u8,
        unit: u32,
    },
    /// Music a script plays instead of the map's (a BGM file, maybe without ".mp3").
    NpcMusic(String),
    /// Where Warp Portal or Teleport may take the player: map names with ".gat" (Teleport's first is "Random").
    WarpList {
        skill: u16,
        maps: Vec<String>,
    },
    /// Saving a memo point: 0 saved, 1 Warp Portal's level is too low, 2 it isn't learned.
    MemoResult(u8),
    /// What this map doesn't allow: 0 teleporting, 1 memo points, 2 skills, 3 items.
    MapForbids(u16),
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
    /// The player's guild: its ID, name, whether we lead it, its emblem's version (ZC_UPDATE_GDID).
    Guild {
        id: u32,
        name: String,
        master: bool,
        emblem: u32,
    },
    GuildInfo(GuildInfo),
    GuildMembers(Vec<GuildMember>),
    /// Position names, by position ID.
    GuildPositions(Vec<(u32, String)>),
    /// Positions' permissions and tax (ZC_POSITION_INFO).
    GuildPositionInfo(Vec<GuildPosition>),
    /// The guild's skills and the points left to put in them.
    GuildSkills {
        points: u16,
        skills: Vec<Skill>,
    },
    /// Who was expelled from the guild (name), and why.
    GuildExpelled(Vec<(String, String)>),
    /// A guild's emblem: a zlib-compressed 24x24 BMP.
    GuildEmblem {
        guild: u32,
        version: u32,
        data: Vec<u8>,
    },
    /// A player's guild or its emblem changed (ZC_CHANGE_GUILD).
    GuildEmblemChanged {
        account: u32,
        guild: u32,
        emblem: u32,
    },
    /// A player's party, guild and position, shown with their name (ZC_ACK_REQNAMEALL), by account ID.
    NameTag(u32, NameTag),
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
    /// An item casts a skill the player may not know (a Magnifier's Item Appraisal, a scroll's bolt): cast it now.
    ItemSkill(Skill),
    /// The inventory indexes a Magnifier can identify; the server waits for a pick (or a cancel) before anything else.
    IdentifyList(Vec<u16>),
    /// How identifying went.
    Identified {
        index: u16,
        ok: bool,
    },
    /// How many players are online (/who).
    UserCount(u32),
    /// How blocking (`allow` false) or allowing one name's whispers went: 0 done, 1 failed, 2 the list is full.
    WhisperSetting {
        allow: bool,
        result: u8,
    },
    /// The names whose whispers are blocked.
    WhisperList(Vec<String>),
    /// How blocking (`allow` false) or allowing everyone's whispers went: 0 done, else failed.
    WhisperAll {
        allow: bool,
        result: u8,
    },
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
    /// A skill's unit on a cell comes into sight (ZC_SKILL_ENTRY): a Fire Wall, a warp portal, a trap. `kind` is
    /// rathena's UNT_ number.
    SkillUnit {
        id: u32,
        kind: u32,
        cell: [u16; 2],
    },
    /// A skill's unit is gone or out of sight (ZC_SKILL_DISAPPEAR).
    SkillUnitGone {
        id: u32,
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
        /// The status's first value where the server sends one (a pushcart's type), else 0.
        value: i32,
    },
    Disconnected(io::Error),
}

/// NPC shops beyond the plain one: a market (its stock runs out), a barter (items for items), an expanded barter
/// (several items and zeny).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MarketKind {
    Market,
    Barter,
    Expanded,
}

/// What a market or barter offers: the item, what it costs (zeny; items and how many), how many are left (None: no
/// end), and the shop's own index for it (barters).
#[derive(Debug, Clone, PartialEq)]
pub struct Bartered {
    pub item: u32,
    pub kind: u8,
    pub zeny: u32,
    pub costs: Vec<(u32, u32)>,
    pub stock: Option<u32>,
    pub index: u32,
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

/// A guild position's permissions (`GUILD_PERM_*` bits) and the share of its members' exp that goes to the guild.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GuildPosition {
    pub id: u32,
    pub mode: u32,
    pub tax: u32,
}

impl GuildPosition {
    pub const INVITE: u32 = 0x01;
    pub const EXPEL: u32 = 0x10;
    pub const STORAGE: u32 = 0x100;
}

/// What shows under a player's name: their party, and their guild with their position in it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NameTag {
    pub party: String,
    pub guild: String,
    pub position: String,
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

/// Another player's looks and what they wear (ZC_EQUIPWIN_MICROSCOPE).
#[derive(Debug, Clone, PartialEq)]
pub struct Equipped {
    pub name: String,
    pub job: u16,
    pub hair: u16,
    pub head_bottom: u16,
    pub head_mid: u16,
    pub head_top: u16,
    pub robe: u16,
    pub hair_color: u16,
    pub clothes_color: u16,
    pub sex: u8,
    /// The worn items, each with its look (a weapon's or shield's sprite).
    pub items: Vec<(InventoryItem, u16)>,
}

/// NORMALITEM_INFO: index, item, type, amount, where it can go, cards, rental end, flags (bit 0: identified).
/// rathena fills "WearState" with the item's places (`id->equip`): a card's is what it goes into, ammunition's
/// the ammunition place, worn or not. None of these is worn as equipment is.
// ponytail: worn ammunition (ZC_EQUIP_ARROW) stays in the inventory; the 2018 equipment window has no place for it.
fn decode_normal_item(entry: &[u8]) -> InventoryItem {
    InventoryItem {
        index: u16_at(entry, 0),
        id: u32_at(entry, 2),
        kind: entry[6],
        amount: u16_at(entry, 7),
        location: u32_at(entry, 9),
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

    /// Puts zeny in the bank, or takes it out.
    pub fn move_zeny(&mut self, deposit: bool, account: u32, zeny: i32) -> io::Result<()> {
        self.stream.write_all(&encode_bank_move(deposit, account, zeny))
    }

    /// Asks to see a player's equipment (by account ID); they must be on the same map.
    pub fn view_equipment(&mut self, account: u32) -> io::Result<()> {
        self.send_id(CZ_EQUIPWIN_MICROSCOPE, account)
    }

    /// The Show Equipment box: whether others may see what the player wears.
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

    /// Hatches the egg at this inventory index (from `PetEggs`).
    pub fn hatch(&mut self, index: u16) -> io::Result<()> {
        self.stream.write_all(&encode_hatch(index))
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

/// CZ_SETTING_WHISPER_PC: the name (NUL-terminated, at most 23 bytes) and 0 to block, 1 to allow.
fn encode_whisper_setting(name: &str, allow: bool) -> Vec<u8> {
    let mut packet = CZ_SETTING_WHISPER_PC.to_le_bytes().to_vec();
    let mut field = [0u8; 24];
    let encoded = encoding_rs::EUC_KR.encode(name).0;
    let length = encoded.len().min(field.len() - 1);
    field[..length].copy_from_slice(&encoded[..length]);
    packet.extend_from_slice(&field);
    packet.push(u8::from(allow));
    packet
}

/// CZ_ITEM_THROW: the inventory index and how many to drop.
fn encode_drop_item(index: u16, amount: u16) -> [u8; 6] {
    let [[h0, h1], [i0, i1], [a0, a1]] = [CZ_ITEM_THROW, index, amount].map(u16::to_le_bytes);
    [h0, h1, i0, i1, a0, a1]
}

/// CZ_REQ_ITEMIDENTIFY: the inventory index to identify; none cancels (an index rathena doesn't have).
fn encode_identify(index: Option<u16>) -> [u8; 4] {
    let [[h0, h1], [i0, i1]] = [CZ_REQ_ITEMIDENTIFY, index.unwrap_or(u16::MAX)].map(u16::to_le_bytes);
    [h0, h1, i0, i1]
}

/// CZ_SELECT_WARPPOINT: the skill that asked and a map (16 bytes, NUL-padded).
fn encode_select_warp_point(skill: u16, map: &str) -> [u8; 20] {
    let mut packet = [0u8; 20];
    packet[..2].copy_from_slice(&CZ_SELECT_WARPPOINT.to_le_bytes());
    packet[2..4].copy_from_slice(&skill.to_le_bytes());
    let length = map.len().min(15);
    packet[4..4 + length].copy_from_slice(&map.as_bytes()[..length]);
    packet
}

/// CZ_REQ_ITEMCOMPOSITION_LIST: a card's inventory index, asking what it can go into.
fn encode_card_list(card: u16) -> [u8; 4] {
    let [[h0, h1], [c0, c1]] = [CZ_REQ_ITEMCOMPOSITION_LIST, card].map(u16::to_le_bytes);
    [h0, h1, c0, c1]
}

/// CZ_REQ_ITEMCOMPOSITION: the card's inventory index, then the equipment's.
fn encode_insert_card(card: u16, equip: u16) -> [u8; 6] {
    let [[h0, h1], [c0, c1], [e0, e1]] = [CZ_REQ_ITEMCOMPOSITION, card, equip].map(u16::to_le_bytes);
    [h0, h1, c0, c1, e0, e1]
}

/// CZ_REQ_BANKING_DEPOSIT or CZ_REQ_BANKING_WITHDRAW: the player's own account and the zeny.
fn encode_bank_move(deposit: bool, account: u32, zeny: i32) -> [u8; 10] {
    let mut packet = [0u8; 10];
    packet[..2].copy_from_slice(&if deposit { CZ_REQ_BANKING_DEPOSIT } else { CZ_REQ_BANKING_WITHDRAW }.to_le_bytes());
    packet[2..6].copy_from_slice(&account.to_le_bytes());
    packet[6..].copy_from_slice(&zeny.to_le_bytes());
    packet
}

/// CZ_CONFIG: a setting (rathena's e_config_type) on or off.
fn encode_config(setting: u32, on: bool) -> [u8; 10] {
    let mut packet = [0u8; 10];
    packet[..2].copy_from_slice(&CZ_CONFIG.to_le_bytes());
    packet[2..6].copy_from_slice(&setting.to_le_bytes());
    packet[6..].copy_from_slice(&u32::from(on).to_le_bytes());
    packet
}

/// A market's entry: item, type, price, stock, weight, where it's worn.
fn decode_market_goods(entry: &[u8]) -> Bartered {
    let stock = u32_at(entry, 9);
    Bartered { item: u32_at(entry, 0), kind: entry[4], zeny: u32_at(entry, 5), costs: Vec::new(), stock: (stock != UNLIMITED).then_some(stock), index: 0 }
}

/// A barter's entry: item, type, stock, the item it costs and how many, weight, the shop's index, look, where worn.
fn decode_barter_goods(entry: &[u8]) -> Bartered {
    let stock = u32_at(entry, 5);
    Bartered {
        item: u32_at(entry, 0),
        kind: entry[4],
        zeny: 0,
        costs: vec![(u32_at(entry, 9), u32_at(entry, 13))],
        stock: (stock != UNLIMITED).then_some(stock),
        index: u32_at(entry, 21),
    }
}

/// An expanded barter's entries: each 32 bytes (item, type, stock, weight, the shop's index, zeny, look, where
/// worn, how many costs), then its costs, 12 bytes each (item, refine, amount, type).
fn decode_expanded_barter(mut rest: &[u8]) -> Vec<Bartered> {
    let mut goods = Vec::new();
    while rest.len() >= 32 {
        let count = u32_at(rest, 28) as usize;
        let Some(costs) = rest.get(32..32 + count * 12) else { break };
        let stock = u32_at(rest, 6);
        goods.push(Bartered {
            item: u32_at(rest, 0),
            kind: rest[4],
            zeny: u32_at(rest, 18),
            costs: costs.chunks_exact(12).map(|cost| (u32_at(cost, 0), u32_at(cost, 6))).collect(),
            stock: (stock != UNLIMITED).then_some(stock),
            index: u32_at(rest, 14),
        });
        rest = &rest[32 + count * 12..];
    }
    goods
}

/// CZ_NPC_MARKET_PURCHASE: (item, amount) pairs.
fn encode_market_buy(items: &[(u32, u32)]) -> Vec<u8> {
    let mut packet = CZ_NPC_MARKET_PURCHASE.to_le_bytes().to_vec();
    packet.extend_from_slice(&((4 + items.len() * 8) as u16).to_le_bytes());
    for &(item, amount) in items {
        packet.extend_from_slice(&item.to_le_bytes());
        packet.extend_from_slice(&amount.to_le_bytes());
    }
    packet
}

/// CZ_NPC_BARTER_MARKET_PURCHASE: (item, amount, inventory index of what it costs, the shop's index).
fn encode_barter_buy(items: &[(u32, u32, u16, u32)]) -> Vec<u8> {
    let mut packet = CZ_NPC_BARTER_MARKET_PURCHASE.to_le_bytes().to_vec();
    packet.extend_from_slice(&((4 + items.len() * 14) as u16).to_le_bytes());
    for &(item, amount, cost_index, shop_index) in items {
        packet.extend_from_slice(&item.to_le_bytes());
        packet.extend_from_slice(&amount.to_le_bytes());
        packet.extend_from_slice(&cost_index.to_le_bytes());
        packet.extend_from_slice(&shop_index.to_le_bytes());
    }
    packet
}

/// CZ_NPC_EXPANDED_BARTER_MARKET_PURCHASE: (item, the shop's index, amount).
fn encode_expanded_barter_buy(items: &[(u32, u32, u32)]) -> Vec<u8> {
    let mut packet = CZ_NPC_EXPANDED_BARTER_MARKET_PURCHASE.to_le_bytes().to_vec();
    packet.extend_from_slice(&((4 + items.len() * 12) as u16).to_le_bytes());
    for &(item, shop_index, amount) in items {
        packet.extend_from_slice(&item.to_le_bytes());
        packet.extend_from_slice(&shop_index.to_le_bytes());
        packet.extend_from_slice(&amount.to_le_bytes());
    }
    packet
}

/// CZ_REQMAKINGITEM: the item to make (0: none, which ends the list) and three extra ingredients (none here).
fn encode_make(item: u32) -> [u8; 18] {
    let mut packet = [0u8; 18];
    packet[..2].copy_from_slice(&CZ_REQMAKINGITEM.to_le_bytes());
    packet[2..6].copy_from_slice(&item.to_le_bytes());
    packet
}

/// CZ_REQ_MAKINGITEM: the list's type and the item (0: none).
fn encode_cook(kind: u16, item: u32) -> [u8; 8] {
    let mut packet = [0u8; 8];
    packet[..2].copy_from_slice(&CZ_REQ_MAKINGITEM.to_le_bytes());
    packet[2..4].copy_from_slice(&kind.to_le_bytes());
    packet[4..].copy_from_slice(&item.to_le_bytes());
    packet
}

/// CZ_REQ_MAKINGARROW: the item picked.
fn encode_make_arrow(item: u32) -> [u8; 6] {
    let mut packet = [0u8; 6];
    packet[..2].copy_from_slice(&CZ_REQ_MAKINGARROW.to_le_bytes());
    packet[2..].copy_from_slice(&item.to_le_bytes());
    packet
}

/// CZ_REQ_ITEMREPAIR2: the picked entry as it came (rathena reads its index); none sends index 0xFFFF, which
/// repairs nothing and ends the list.
fn encode_repair(item: Option<&InventoryItem>) -> [u8; 26] {
    let mut packet = [0u8; 26];
    packet[..2].copy_from_slice(&CZ_REQ_ITEMREPAIR2.to_le_bytes());
    let Some(item) = item else {
        packet[2..4].copy_from_slice(&u16::MAX.to_le_bytes());
        return packet;
    };
    packet[2..4].copy_from_slice(&item.index.to_le_bytes());
    packet[4..8].copy_from_slice(&item.id.to_le_bytes());
    for (slot, card) in item.cards.iter().enumerate() {
        packet[8 + slot * 4..12 + slot * 4].copy_from_slice(&card.to_le_bytes());
    }
    packet[24] = item.refine;
    packet
}

/// CZ_REQ_WEAPONREFINE: the weapon's inventory index (0: none, which ends the list).
fn encode_upgrade(index: u32) -> [u8; 6] {
    let mut packet = [0u8; 6];
    packet[..2].copy_from_slice(&CZ_REQ_WEAPONREFINE.to_le_bytes());
    packet[2..].copy_from_slice(&index.to_le_bytes());
    packet
}

/// CZ_SELECTAUTOSPELL: the spell Hindsight keeps ready (0: none, which lets the player go).
fn encode_autospell(skill: u32) -> [u8; 6] {
    let mut packet = [0u8; 6];
    packet[..2].copy_from_slice(&CZ_SELECTAUTOSPELL.to_le_bytes());
    packet[2..].copy_from_slice(&skill.to_le_bytes());
    packet
}

/// CZ_SKILL_SELECT_RESPONSE: the reason as it came, and the skill (0: none).
fn encode_skill_select(why: u32, skill: u16) -> [u8; 8] {
    let mut packet = [0u8; 8];
    packet[..2].copy_from_slice(&CZ_SKILL_SELECT_RESPONSE.to_le_bytes());
    packet[2..6].copy_from_slice(&why.to_le_bytes());
    packet[6..].copy_from_slice(&skill.to_le_bytes());
    packet
}

/// CZ_REQ_REFINING: the item, the ore, and whether Blacksmith's Blessings go with it.
fn encode_refine(index: u16, ore: u32, blessings: bool) -> [u8; 9] {
    let mut packet = [0u8; 9];
    packet[..2].copy_from_slice(&CZ_REQ_REFINING.to_le_bytes());
    packet[2..4].copy_from_slice(&index.to_le_bytes());
    packet[4..8].copy_from_slice(&ore.to_le_bytes());
    packet[8] = u8::from(blessings);
    packet
}

/// CZ_SELECT_PETEGG: the egg's inventory index, as the list gave it.
fn encode_hatch(index: u16) -> [u8; 4] {
    let mut packet = [0u8; 4];
    packet[..2].copy_from_slice(&CZ_SELECT_PETEGG.to_le_bytes());
    packet[2..].copy_from_slice(&index.to_le_bytes());
    packet
}

fn encode_pet_command(command: u8) -> [u8; 3] {
    let [low, high] = CZ_COMMAND_PET.to_le_bytes();
    [low, high, command]
}

/// CZ_RENAME_PET: at most 23 bytes of whole characters, NUL-padded.
fn encode_rename_pet(name: &str) -> [u8; 26] {
    let mut packet = [0u8; 26];
    packet[..2].copy_from_slice(&CZ_RENAME_PET.to_le_bytes());
    let mut bytes = Vec::new();
    for character in name.chars() {
        let encoded = encoding_rs::EUC_KR.encode(character.encode_utf8(&mut [0; 4])).0.into_owned();
        if bytes.len() + encoded.len() > 23 {
            break;
        }
        bytes.extend_from_slice(&encoded);
    }
    packet[2..2 + bytes.len()].copy_from_slice(&bytes);
    packet
}

fn encode_pet_act(act: &PetAct) -> [u8; 6] {
    let mut packet = [0u8; 6];
    packet[..2].copy_from_slice(&CZ_PET_ACT.to_le_bytes());
    packet[2..].copy_from_slice(&act.value().to_le_bytes());
    packet
}

/// CZ_TRYCAPTURE_MONSTER: the monster to tame.
fn encode_try_capture(monster: u32) -> [u8; 6] {
    let mut packet = [0u8; 6];
    packet[..2].copy_from_slice(&CZ_TRYCAPTURE_MONSTER.to_le_bytes());
    packet[2..].copy_from_slice(&monster.to_le_bytes());
    packet
}

/// CZ_SETTING_WHISPER_STATE: 0 to block everyone, 1 to allow everyone.
fn encode_whisper_all(allow: bool) -> [u8; 3] {
    let [low, high] = CZ_SETTING_WHISPER_STATE.to_le_bytes();
    [low, high, u8::from(allow)]
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
        // GUID and GEmblemVer follow the robe.
        guild: u32_at(packet, layout.robe + 2),
        emblem: u32::from(u16_at(layout.robe + 6)),
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
        ZC_NOTIFY_CHAT => MapEvent::Chat { id: Some(u32_at(4)), message: c_string(&packet[8..]), color: None },
        ZC_NOTIFY_PLAYERCHAT => MapEvent::Chat { id: None, message: c_string(&packet[4..]), color: None },
        // The color is 0xBBGGRR, little-endian.
        ZC_NPC_CHAT => MapEvent::Chat { id: Some(u32_at(4)), message: c_string(&packet[12..]), color: Some([packet[8], packet[9], packet[10]]) },
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
        ZC_SKILL_ENTRY => MapEvent::SkillUnit { id: u32_at(4), kind: u32_at(16), cell: [u16_at(12), u16_at(14)] },
        ZC_SKILL_DISAPPEAR => MapEvent::SkillUnitGone { id: u32_at(2) },
        ZC_ACK_TOUSESKILL => MapEvent::SkillFailed { skill: u16_at(2), detail: u32_at(4) as i32, ok: packet[12] != 0, cause: packet[13] },
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
        // "blue" (and "ssss" for the War of Emperium) at the start are color hints, not words.
        ZC_BROADCAST => {
            let text = c_string(&packet[4..]);
            let blue = text.starts_with("blue");
            let text = text.strip_prefix("blue").or_else(|| text.strip_prefix("ssss")).unwrap_or(&text).to_owned();
            MapEvent::Broadcast { text, color: None, blue }
        }
        // The color is 0xRRGGBB, little-endian.
        ZC_BROADCAST2 => MapEvent::Broadcast { text: c_string(&packet[16..]), color: Some([packet[6], packet[5], packet[4]]), blue: false },
        // The player's name and what it came from each come after their length: a box's item ID (4), a monster's
        // name (24), nothing for an NPC.
        ZC_BROADCASTING_SPECIAL_ITEM_OBTAIN => {
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
        ZC_ITEMCOMPOSITION_LIST => MapEvent::CardList(packet[4..].chunks_exact(2).map(|index| u16::from_le_bytes([index[0], index[1]])).collect()),
        // The result byte is rathena's "failure".
        ZC_ACK_ITEMCOMPOSITION => MapEvent::CardInserted { equip: u16_at(2), card: u16_at(4), ok: packet[6] == 0 },
        ZC_SPIRITS => MapEvent::Spirits { id: u32_at(2), count: u16_at(6) },
        ZC_ACK_OPEN_BANKING => MapEvent::BankOpened,
        ZC_BANKING_CHECK => MapEvent::BankBalance(i64::from_le_bytes(packet[2..10].try_into().unwrap())),
        ZC_ACK_BANKING_DEPOSIT | ZC_ACK_BANKING_WITHDRAW => MapEvent::BankMoved {
            deposit: u16_at(0) == ZC_ACK_BANKING_DEPOSIT,
            reason: u16_at(2),
            balance: i64::from_le_bytes(packet[4..12].try_into().unwrap()),
            zeny: u32_at(12) as i32,
        },
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
        ZC_REFINING_MATERIAL_LIST => MapEvent::RefineMaterials {
            index: u16_at(4),
            blessings: packet[6],
            materials: packet[7..].chunks_exact(9).map(|ore| (self::u32_at(ore, 0), ore[4], self::u32_at(ore, 5))).collect(),
        },
        ZC_ACK_ITEMREFINING => MapEvent::Refined { result: u16_at(2), index: u16_at(4), refine: u16_at(6) },
        ZC_PETEGG_LIST => {
            MapEvent::PetEggs(packet[4..usize::from(u16_at(2)).min(packet.len())].chunks_exact(2).map(|pair| u16::from_le_bytes([pair[0], pair[1]])).collect())
        }
        ZC_CHANGESTATE_PET => MapEvent::PetState { kind: packet[2], id: u32_at(3), value: u32_at(7) as i32 },
        // Name, renamed, level, hunger, intimacy, accessory, monster.
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
        ZC_MSG => MapEvent::Message { id: usize::from(u16_at(2)), value: None, skill: None, color: None },
        ZC_MSG_VALUE => MapEvent::Message { id: usize::from(u16_at(2)), value: Some(u32_at(4) as i32), skill: None, color: None },
        ZC_MSG_SKILL => MapEvent::Message { id: u32_at(4) as usize, value: None, skill: Some(u16_at(2)), color: None },
        // 0xBBGGRR, like ZC_NPC_CHAT's.
        ZC_MSG_COLOR => MapEvent::Message { id: usize::from(u16_at(2)), value: None, skill: None, color: Some([packet[4], packet[5], packet[6]]) },
        ZC_ACTION_FAILURE => MapEvent::ActionFailure(u16_at(2)),
        ZC_ACK_REMEMBER_WARPPOINT => MapEvent::MemoResult(packet[2]),
        ZC_SOUND => MapEvent::Sound { file: c_string(&packet[2..26]), act: packet[26], unit: u32_at(31) },
        ZC_PLAY_NPC_BGM => MapEvent::NpcMusic(c_string(&packet[2..26])),
        ZC_WARPLIST => MapEvent::WarpList { skill: u16_at(4), maps: packet[6..].chunks_exact(16).map(c_string).filter(|map| !map.is_empty()).collect() },
        ZC_NOTIFY_MAPINFO => MapEvent::MapForbids(u16_at(2)),
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
        ZC_UPDATE_GDID => MapEvent::Guild { id: u32_at(2), name: c_string(&packet[19..43]), master: packet[14] != 0, emblem: u32_at(6) },
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
        // Each position: ID, permissions, rank, tax.
        ZC_POSITION_INFO => MapEvent::GuildPositionInfo(
            packet[4..]
                .chunks_exact(16)
                .map(|entry| GuildPosition { id: self::u32_at(entry, 0), mode: self::u32_at(entry, 4), tax: self::u32_at(entry, 12) })
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
            MapEvent::NameTag(u32_at(2), NameTag { party: c_string(&packet[30..54]), guild: c_string(&packet[54..78]), position: c_string(&packet[78..102]) })
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
        assert_eq!((character.moves, character.renames, character.name_changed), (0, 0, false));

        // char_mmo_char_tobuf with two moves left and a rename granted.
        info[166..170].copy_from_slice(&2i32.to_le_bytes());
        info[170..174].copy_from_slice(&1i32.to_le_bytes());
        info[140..142].copy_from_slice(&1u16.to_le_bytes());
        let character = decode_character(&info);
        assert_eq!((character.moves, character.renames, character.name_changed), (2, 1, true));
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
        let packet = |id: u16, body: &[u8]| {
            let mut packet = id.to_le_bytes().to_vec();
            packet.extend_from_slice(&((4 + body.len()) as u16).to_le_bytes());
            packet.extend_from_slice(body);
            packet
        };
        let mut position = [0u8; 16];
        position[..4].copy_from_slice(&19u32.to_le_bytes());
        position[4..8].copy_from_slice(&0x111u32.to_le_bytes());
        position[12..16].copy_from_slice(&50u32.to_le_bytes());
        let Some(MapEvent::GuildPositionInfo(positions)) = decode_map_packet(&packet(ZC_POSITION_INFO, &position)) else { panic!() };
        assert_eq!(positions, [GuildPosition { id: 19, mode: 0x111, tax: 50 }]);

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
        stand[49..53].copy_from_slice(&7u32.to_le_bytes());
        stand[53..55].copy_from_slice(&3u16.to_le_bytes());
        let Some(MapEvent::Unit(unit)) = decode_map_packet(&stand) else { panic!() };
        assert_eq!((unit.guild, unit.emblem), (7, 3));
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
    fn decodes_announcements_and_messages() {
        let mut blue = ZC_BROADCAST.to_le_bytes().to_vec();
        blue.extend_from_slice(&[0, 0]);
        blue.extend_from_slice(b"blueHello all\0");
        let length = blue.len() as u16;
        blue[2..4].copy_from_slice(&length.to_le_bytes());
        let Some(MapEvent::Broadcast { text, color: None, blue: true }) = decode_map_packet(&blue) else { panic!() };
        assert_eq!(text, "Hello all");

        let mut colored = ZC_BROADCAST2.to_le_bytes().to_vec();
        colored.extend_from_slice(&[0, 0]);
        colored.extend_from_slice(&0x00FF_8000u32.to_le_bytes());
        colored.extend_from_slice(&[0; 8]);
        colored.extend_from_slice(b"Event!\0");
        let length = colored.len() as u16;
        colored[2..4].copy_from_slice(&length.to_le_bytes());
        let Some(MapEvent::Broadcast { text, color: Some(color), .. }) = decode_map_packet(&colored) else { panic!() };
        assert_eq!((text.as_str(), color), ("Event!", [0xFF, 0x80, 0x00]));

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
        assert_eq!([ZC_MVP, ZC_MVP_GETTING_ITEM, ZC_MVP_GETTING_SPECIAL_EXP, ZC_THROW_MVPITEM].map(packet_length), [Some(6), Some(6), Some(6), Some(2)]);
    }

    #[test]
    fn decodes_spirits() {
        let mut packet = ZC_SPIRITS.to_le_bytes().to_vec();
        packet.extend_from_slice(&2_000_002u32.to_le_bytes());
        packet.extend_from_slice(&5u16.to_le_bytes());
        assert_eq!(Some(packet.len() as i32), packet_length(ZC_SPIRITS));
        let Some(MapEvent::Spirits { id: 2_000_002, count: 5 }) = decode_map_packet(&packet) else { panic!() };
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
        let opens =
            [ZC_NPC_MARKET_OPEN, ZC_NPC_MARKET_PURCHASE_RESULT, ZC_NPC_BARTER_MARKET_ITEMINFO, ZC_NPC_EXPANDED_BARTER_MARKET_ITEMINFO].map(packet_length);
        assert_eq!(opens, [Some(-1); 4]);
        let closes = [CZ_NPC_MARKET_CLOSE, CZ_NPC_BARTER_MARKET_CLOSE, CZ_NPC_EXPANDED_BARTER_MARKET_CLOSE].map(packet_length);
        assert_eq!(closes, [Some(2); 3]);
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
        let select =
            [&ZC_SKILL_SELECT_REQUEST.to_le_bytes()[..], &12u16.to_le_bytes(), &1u32.to_le_bytes(), &19u16.to_le_bytes(), &14u16.to_le_bytes()].concat();
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
        let act = [&ZC_PET_ACT.to_le_bytes()[..], &7u32.to_le_bytes(), &90203i32.to_le_bytes()].concat();
        let Some(MapEvent::PetAct { id: 7, act }) = decode_map_packet(&act) else { panic!() };
        assert_eq!(act, PetAct::Talk { monster: 1002, act: 0, hunger: 3 });
        assert_eq!(PetAct::decode(5), PetAct::Emotion(5));
        assert_eq!(PetAct::Talk { monster: 1002, act: 9, hunger: 4 }.value(), 90294);

        assert_eq!(encode_hatch(3), [0xA7, 0x01, 3, 0]);
        assert_eq!(encode_pet_command(PET_RETURN), [0xA1, 0x01, 3]);
        assert_eq!(encode_pet_act(&act), [0xA9, 0x01, 0x5B, 0x60, 0x01, 0]);
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
