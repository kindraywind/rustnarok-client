//! Login, char and map server protocol, as rathena speaks it for PACKETVER 20220406 (see rathena src/common/packets.hpp).

use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, TcpStream};
use std::sync::LazyLock;
use std::time::{Duration, Instant};

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

/// CZ_REG_CHANGE_GUILD_POSITIONINFO with one position: ID, permissions, rank (rathena ignores it; the ID), tax, title.
pub fn encode_position_change(position: &GuildPosition, title: &str) -> Vec<u8> {
    let mut packet = CZ_REG_CHANGE_GUILD_POSITIONINFO.to_le_bytes().to_vec();
    packet.extend_from_slice(&44u16.to_le_bytes());
    for value in [position.id, position.mode, position.id, position.tax] {
        packet.extend_from_slice(&value.to_le_bytes());
    }
    packet.extend_from_slice(&name_field(title));
    packet
}

/// CZ_REQ_CHANGE_MEMBERPOS with one member.
pub fn encode_member_move(account: u32, character: u32, position: u32) -> Vec<u8> {
    let mut packet = CZ_REQ_CHANGE_MEMBERPOS.to_le_bytes().to_vec();
    packet.extend_from_slice(&16u16.to_le_bytes());
    for value in [account, character, position] {
        packet.extend_from_slice(&value.to_le_bytes());
    }
    packet
}

/// A battleground's name as rathena keeps it (`name.resize(NAME_LENGTH)`): 23 bytes and a NUL in 24.
fn queue_name_field(name: &str) -> Vec<u8> {
    let mut field = encoding_rs::EUC_KR.encode(name).0.into_owned();
    field.truncate(NAME_LENGTH - 1);
    field.resize(NAME_LENGTH, 0);
    field
}

/// The name as rathena's replies carry it, to match them against the list.
pub fn queue_name(name: &str) -> String {
    c_string(&queue_name_field(name))
}

/// CZ_REQ_ENTRY_QUEUE_APPLY: how, and the battleground.
pub fn encode_queue_apply(kind: u16, name: &str) -> Vec<u8> {
    let mut packet = CZ_REQ_ENTRY_QUEUE_APPLY.to_le_bytes().to_vec();
    packet.extend_from_slice(&kind.to_le_bytes());
    packet.extend_from_slice(&queue_name_field(name));
    packet
}

/// CZ_REQ_ENTRY_QUEUE_CANCEL and CZ_REQ_ENTRY_QUEUE_RANKING: the battleground's name.
fn encode_queue_name(command: u16, name: &str) -> Vec<u8> {
    let mut packet = command.to_le_bytes().to_vec();
    packet.extend_from_slice(&queue_name_field(name));
    packet
}

/// CZ_REPLY_LOBBY_ADMISSION: 1 enters, 2 declines; the names (rathena reads only the answer).
pub fn encode_lobby_reply(accept: bool, name: &str) -> Vec<u8> {
    let mut packet = CZ_REPLY_LOBBY_ADMISSION.to_le_bytes().to_vec();
    packet.push(if accept { 1 } else { 2 });
    packet.extend_from_slice(&queue_name_field(name));
    packet.extend_from_slice(&queue_name_field(name));
    packet
}

/// CZ_REQ_JOIN_BABY: the player to adopt.
pub fn encode_adopt_request(account: u32) -> Vec<u8> {
    let mut packet = CZ_REQ_JOIN_BABY.to_le_bytes().to_vec();
    packet.extend_from_slice(&account.to_le_bytes());
    packet
}

/// CZ_JOIN_BABY: the parents' accounts as ZC_REQ_BABY gave them, and 1 yes or 0 no.
pub fn encode_adopt_answer(parent: u32, partner: u32, accept: bool) -> Vec<u8> {
    let mut packet = CZ_JOIN_BABY.to_le_bytes().to_vec();
    for value in [parent, partner, u32::from(accept)] {
        packet.extend_from_slice(&value.to_le_bytes());
    }
    packet
}

/// CZ_BATTLEFIELD_CHAT: the line as the guild chat writes it, "Name : message" and a NUL.
pub fn encode_battle_chat(name: &str, message: &str) -> Vec<u8> {
    let text = encoding_rs::EUC_KR.encode(&format!("{name} : {message}")).0.into_owned();
    let mut packet = CZ_BATTLEFIELD_CHAT.to_le_bytes().to_vec();
    packet.extend_from_slice(&((4 + text.len() + 1) as u16).to_le_bytes());
    packet.extend_from_slice(&text);
    packet.push(0);
    packet
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
const CH_SECOND_PASSWD_ACK: u16 = 0x08B8;
const HC_SECOND_PASSWD_LOGIN: u16 = 0x08B9;
const CH_MAKE_SECOND_PASSWD: u16 = 0x08BA;
const CH_EDIT_SECOND_PASSWD: u16 = 0x08BE;
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
    /// Option bits (rathena's OPTION_*: mounts, outfits, ...).
    pub option: u32,
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
    /// HC_SECOND_PASSWD_LOGIN (0x08B9): PIN state (0 OK/passed, 1 ask, 2/4 make, 3 expired/change, 5 disallowed, 8 wrong)
    /// and the shuffle seed for the next reply.
    Pin {
        seed: u32,
        state: u16,
    },
    Disconnected(io::Error),
}

/// A char server connection; events arrive on a reader thread until the connection closes.
pub struct CharSession {
    stream: TcpStream,
    account_id: u32,
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
        Ok(Self { stream, account_id: login.account_id })
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

    /// Sends the 4-digit PIN for state 1 (CH_SECOND_PASSWD_ACK 0x08B8); the answer is a `Pin` event.
    pub fn check_pin(&mut self, seed: u32, pin: &str) -> io::Result<()> {
        self.stream.write_all(&encode_pin_check(self.account_id, seed, pin))
    }

    /// Sets the account's first 4-digit PIN for states 2 and 4 (CH_MAKE_SECOND_PASSWD 0x08BA).
    pub fn make_pin(&mut self, seed: u32, pin: &str) -> io::Result<()> {
        self.stream.write_all(&encode_pin_new(self.account_id, seed, pin))
    }

    /// Changes the 4-digit PIN (CH_EDIT_SECOND_PASSWD 0x08BE).
    pub fn change_pin(&mut self, seed: u32, old_pin: &str, new_pin: &str) -> io::Result<()> {
        self.stream.write_all(&encode_pin_change(self.account_id, seed, old_pin, new_pin))
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

/// Builds rathena's 10-digit PIN pad permutation for `seed` (`char_pincode_decrypt` in `src/char/char.cpp`):
/// button `k` shows digit `tab[k]`, and pressing it sends `'0' + k`.
pub fn pin_table(mut seed: u32) -> [u8; 10] {
    let mut tab = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
    for i in 1..10u32 {
        seed = 0x0088_1234u32.wrapping_add(seed.wrapping_mul(0x3498));
        let pos = (seed % (i + 1)) as usize;
        tab.swap(i as usize, pos);
    }
    tab
}

/// Encodes a 4-digit ASCII PIN with `seed` into the 4 button-index ASCII bytes sent to the char server.
pub fn encode_pin(seed: u32, digits: &str) -> [u8; 4] {
    let tab = pin_table(seed);
    let mut out = [0u8; 4];
    for (slot, byte) in out.iter_mut().zip(digits.bytes()) {
        let index = tab.iter().position(|&digit| digit == byte.wrapping_sub(b'0')).unwrap_or(0);
        *slot = b'0' + index as u8;
    }
    out
}

fn encode_pin_check(account_id: u32, seed: u32, pin: &str) -> [u8; 10] {
    let mut packet = [0u8; 10];
    packet[..2].copy_from_slice(&CH_SECOND_PASSWD_ACK.to_le_bytes());
    packet[2..6].copy_from_slice(&account_id.to_le_bytes());
    packet[6..10].copy_from_slice(&encode_pin(seed, pin));
    packet
}

fn encode_pin_new(account_id: u32, seed: u32, pin: &str) -> [u8; 10] {
    let mut packet = [0u8; 10];
    packet[..2].copy_from_slice(&CH_MAKE_SECOND_PASSWD.to_le_bytes());
    packet[2..6].copy_from_slice(&account_id.to_le_bytes());
    packet[6..10].copy_from_slice(&encode_pin(seed, pin));
    packet
}

fn encode_pin_change(account_id: u32, seed: u32, old_pin: &str, new_pin: &str) -> [u8; 14] {
    let mut packet = [0u8; 14];
    packet[..2].copy_from_slice(&CH_EDIT_SECOND_PASSWD.to_le_bytes());
    packet[2..6].copy_from_slice(&account_id.to_le_bytes());
    packet[6..10].copy_from_slice(&encode_pin(seed, old_pin));
    packet[10..14].copy_from_slice(&encode_pin(seed, new_pin));
    packet
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
/// Dev hook (the control socket): sees every packet written to the map server.
pub static SENT_HOOK: std::sync::OnceLock<fn(&[u8])> = std::sync::OnceLock::new();

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
        HC_SECOND_PASSWD_LOGIN => Some(CharEvent::Pin { seed: u32_at(packet, 2), state: u16_at(packet, 10) }),
        HC_ACK_CHANGE_CHARACTER_SLOT => Some(CharEvent::Moved { result: u16_at(packet, 4), moves: u16_at(packet, 6) }),
        HC_ACK_CHANGE_CHARNAME => Some(CharEvent::Renamed(u32_at(packet, 2))),
        // Slot info, page count and blocked characters aren't needed yet.
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
        option: i32_at(36) as u32,
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
const CZ_CHANGE_DIRECTION: u16 = 0x0361;
const CZ_CANCEL_LOCKON: u16 = 0x0118;
const CZ_INVENTORY_TAB: u16 = 0x0907;
const ZC_INVENTORY_TAB: u16 = 0x0908;
const CZ_REQ_ONECLICK_ITEMIDENTIFY: u16 = 0x0A35;
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
const ZC_NOTIFY_TIME: u16 = 0x007F;
const ZC_STOPMOVE: u16 = 0x0088;
const ZC_HIGHJUMP: u16 = 0x01FF;
const ZC_FASTMOVE: u16 = 0x08D2;
const ZC_COMBODELAY: u16 = 0x01D2;
const ZC_BLADESTOP: u16 = 0x01D1;
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
const ZC_STATE_CHANGE: u16 = 0x0229;
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
/// A unit's cast was cut off (rathena's clif_skillcastcancel).
const ZC_DISPEL: u16 = 0x01B9;
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
const CZ_REQ_OPEN_BUYING_STORE: u16 = 0x0811;
const CZ_REQ_CLOSE_BUYING_STORE: u16 = 0x0815;
const CZ_REQ_CLICK_TO_BUYING_STORE: u16 = 0x0817;
const CZ_REQ_TRADE_BUYING_STORE: u16 = 0x0819;
const ZC_OPEN_BUYING_STORE: u16 = 0x0810;
const ZC_FAILED_OPEN_BUYING_STORE_TO_BUYER: u16 = 0x0812;
const ZC_MYITEMLIST_BUYING_STORE: u16 = 0x0813;
const ZC_BUYING_STORE_ENTRY: u16 = 0x0814;
const ZC_DISAPPEAR_BUYING_STORE_ENTRY: u16 = 0x0816;
const ZC_ACK_ITEMLIST_BUYING_STORE: u16 = 0x0818;
const ZC_FAILED_TRADE_BUYING_STORE_TO_BUYER: u16 = 0x081A;
const ZC_ITEM_DELETE_BUYING_STORE: u16 = 0x081C;
const ZC_FAILED_TRADE_BUYING_STORE_TO_SELLER: u16 = 0x0824;
const ZC_UPDATE_ITEM_FROM_BUYING_STORE2: u16 = 0x09E6;
const ZC_DELETE_ITEM_FROM_CART: u16 = 0x0125;
const CZ_MOVE_ITEM_FROM_BODY_TO_CART: u16 = 0x0126;
const CZ_MOVE_ITEM_FROM_CART_TO_BODY: u16 = 0x0127;
const ZC_ADD_ITEM_TO_CART: u16 = 0x0B45;
const ZC_NOTIFY_UNREADMAIL: u16 = 0x09E7;
const ZC_BROADCAST: u16 = 0x009A;
const ZC_MAPPROPERTY_R2: u16 = 0x099B;
const CZ_BLOCKING_PLAY_CANCEL: u16 = 0x0447;
const ZC_NOTIFY_MAPPROPERTY2: u16 = 0x01D6;
const ZC_NOTIFY_POSITION_TO_GUILDM: u16 = 0x01EB;
const ZC_WEIGHT_INFO: u16 = 0x0ADE;
const ZC_CONGRATULATION: u16 = 0x01EA;
const ZC_COUPLENAME: u16 = 0x01E6;
const ZC_DIVORCE: u16 = 0x0205;
const ZC_REQ_BABY: u16 = 0x01F6;
const ZC_BABYMSG: u16 = 0x0216;
const CZ_REQ_JOIN_BABY: u16 = 0x01F9;
const CZ_JOIN_BABY: u16 = 0x01F7;
const ZC_BATTLEFIELD_NOTIFY_HP2: u16 = 0x0A0E;
const ZC_BATTLEFIELD_NOTIFY_POSITION: u16 = 0x02DF;
const ZC_BATTLEFIELD_NOTIFY_CAMPINFO: u16 = 0x02DD;
const ZC_BATTLEFIELD_NOTIFY_POINT: u16 = 0x02DE;
const ZC_BATTLEFIELD_CHAT: u16 = 0x02DC;
const CZ_BATTLEFIELD_CHAT: u16 = 0x02DB;
const CZ_REQ_ENTRY_QUEUE_APPLY: u16 = 0x08D7;
const ZC_ACK_ENTRY_QUEUE_APPLY: u16 = 0x08D8;
const ZC_NOTIFY_ENTRY_QUEUE_APPLY: u16 = 0x08D9;
const CZ_REQ_ENTRY_QUEUE_CANCEL: u16 = 0x08DA;
const ZC_ACK_ENTRY_QUEUE_CANCEL: u16 = 0x08DB;
const ZC_NOTIFY_LOBBY_ADMISSION: u16 = 0x08DF;
const CZ_REPLY_LOBBY_ADMISSION: u16 = 0x08E0;
const ZC_REPLY_ACK_LOBBY_ADMISSION: u16 = 0x08E1;
const CZ_REQ_ENTRY_QUEUE_RANKING: u16 = 0x090A;
const ZC_ENTRY_QUEUE_INIT: u16 = 0x090E;
/// How one applies to a battleground's queue: alone, as a party, as a guild.
pub const QUEUE_ALONE: u16 = 1;
pub const QUEUE_PARTY: u16 = 2;
pub const QUEUE_GUILD: u16 = 4;
const ZC_DRESSROOM_OPEN: u16 = 0x0A02;
const CZ_REQ_RANKING: u16 = 0x097C;
const ZC_UPDATE_RANKING_POINT: u16 = 0x097E;
const ZC_ACK_RANKING2: u16 = 0x0AF6;
const CZ_REQNAME_BYGID: u16 = 0x0369;
const ZC_ACK_REQNAME_BYGID: u16 = 0x0AF7;
const ZC_STARSKILL: u16 = 0x020E;
const CZ_SEARCH_STORE_INFO: u16 = 0x0835;
const ZC_SEARCH_STORE_INFO_FAILED: u16 = 0x0837;
const CZ_SEARCH_STORE_INFO_NEXT_PAGE: u16 = 0x0838;
const ZC_OPEN_SEARCH_STORE_INFO: u16 = 0x083A;
const CZ_CLOSE_SEARCH_STORE_INFO: u16 = 0x083B;
const CZ_SSILIST_ITEM_CLICK: u16 = 0x083C;
const ZC_SSILIST_ITEM_CLICK_ACK: u16 = 0x083D;
const ZC_SEARCH_STORE_INFO_ACK: u16 = 0x0B64;
/// One result of ZC_SEARCH_STORE_INFO_ACK.
const CATALOG_ROW_SIZE: usize = 142;
/// ZC_STARSKILL's kind for the Taekwon Mission's target.
pub const STAR_MISSION: u8 = 20;
const ZC_NOTIFY_RANKING: u16 = 0x019A;
const CZ_REQ_PVPPOINT: u16 = 0x020F;
const ZC_ACK_PVPPOINT: u16 = 0x0210;
/// ZC_MAPPROPERTY_R2's flags (rathena's `clif_map_property`).
pub const MAP_PVP: u32 = 1;
pub const MAP_GVG: u32 = 1 << 1;
/// Guild emblems over players' heads (a GvG map).
pub const MAP_SIEGE: u32 = 1 << 2;
pub const MAP_NO_LOCK_ON: u32 = 1 << 4;
/// Show the PvP rank counter.
pub const MAP_COUNT_PK: u32 = 1 << 5;
pub const MAP_BATTLEFIELD: u32 = 1 << 7;
pub const MAP_NO_COSTUME: u32 = 1 << 8;
pub const MAP_SUN_MOON_STAR_MIRACLE: u32 = 1 << 10;
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
const CZ_USE_SKILL_TOGROUND_WITHTALKBOX2: u16 = 0x0367;
const ZC_TALKBOX_CHATCONTENTS: u16 = 0x0191;
const ZC_MONSTER_INFO: u16 = 0x018C;
const ZC_BOSS_INFO: u16 = 0x0293;
const ZC_CASH_TIME_COUNTER: u16 = 0x0298;
const ZC_CASH_ITEM_DELETE: u16 = 0x0299;
const CZ_SE_CASHSHOP_OPEN2: u16 = 0x0B6D;
const ZC_SE_CASHSHOP_OPEN: u16 = 0x0B6E;
const CZ_CASHSHOP_LIST: u16 = 0x08C9;
const ZC_ACK_SCHEDULER_CASHITEM: u16 = 0x08CA;
const CZ_SE_PC_BUY_CASHITEM_LIST: u16 = 0x0848;
const ZC_SE_PC_BUY_CASHITEM_RESULT: u16 = 0x0849;
const CZ_CASHSHOP_CLOSE: u16 = 0x084A;
const ZC_NOTIFY_BIND_ON_EQUIP: u16 = 0x02D3;
const CZ_REQ_TAKEOFF_EQUIP_ALL: u16 = 0x0BAD;
const ZC_ACK_TAKEOFF_EQUIP_ALL: u16 = 0x0BAE;
const ZC_MERGE_ITEM_OPEN: u16 = 0x096D;
const CZ_REQ_MERGE_ITEM: u16 = 0x096E;
const ZC_ACK_MERGE_ITEM: u16 = 0x096F;
const CZ_CANCEL_MERGE_ITEM: u16 = 0x0974;
const CZ_REQ_EQUIPSWITCH_ADD: u16 = 0x0A97;
const ZC_EQUIPSWITCH_ADDED: u16 = 0x0A98;
const CZ_REQ_EQUIPSWITCH_REMOVE: u16 = 0x0A99;
const ZC_EQUIPSWITCH_REMOVED: u16 = 0x0A9A;
const ZC_EQUIPSWITCH_LIST: u16 = 0x0A9B;
const CZ_REQ_EQUIPSWITCH: u16 = 0x0A9C;
const ZC_EQUIPSWITCH_REPLY: u16 = 0x0A9D;
const CZ_REQ_OPEN_ROULETTE: u16 = 0x0A19;
const ZC_ACK_OPEN_ROULETTE: u16 = 0x0A1A;
const CZ_REQ_ROULETTE_INFO: u16 = 0x0A1B;
const ZC_ACK_ROULETTE_INFO: u16 = 0x0A1C;
const CZ_REQ_CLOSE_ROULETTE: u16 = 0x0A1D;
const CZ_REQ_GENERATE_ROULETTE: u16 = 0x0A1F;
const ZC_ACK_GENERATE_ROULETTE: u16 = 0x0A20;
const CZ_RECV_ROULETTE_ITEM: u16 = 0x0A21;
const ZC_RECV_ROULETTE_ITEM: u16 = 0x0A22;
const ZC_SKILL_POSTDELAY: u16 = 0x043D;
const ZC_ALL_ACH_LIST: u16 = 0x0A23;
const ZC_ACH_UPDATE: u16 = 0x0A24;
const CZ_REQ_ACH_REWARD: u16 = 0x0A25;
const ZC_REQ_ACH_REWARD_ACK: u16 = 0x0A26;
const CZ_REQ_CHANGE_TITLE: u16 = 0x0A2E;
const ZC_ACK_CHANGE_TITLE: u16 = 0x0A2F;
const ZC_ITEMLISTWIN_OPEN: u16 = 0x07E3;
const CZ_ITEMLISTWIN_RES: u16 = 0x07E4;
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
const ZC_MER_INIT: u16 = 0x029B;
const ZC_MER_SKILLINFO_LIST: u16 = 0x029D;
const ZC_MER_PAR_CHANGE: u16 = 0x02A2;
const CZ_MER_COMMAND: u16 = 0x029F;
const CZ_REQUEST_MOVENPC: u16 = 0x0232;
const CZ_REQUEST_ACTNPC: u16 = 0x0233;
const CZ_REQUEST_MOVETOOWNER: u16 = 0x0234;
/// CZ_MER_COMMAND's dismiss.
pub const MERCENARY_DISMISS: u8 = 2;
/// ZC_MER_PAR_CHANGE's kinds (rathena's SP_ numbers).
pub const MERCENARY_HP: u16 = 5;
pub const MERCENARY_MAX_HP: u16 = 6;
pub const MERCENARY_SP: u16 = 7;
pub const MERCENARY_MAX_SP: u16 = 8;
pub const MERCENARY_ATK: u16 = 41;
pub const MERCENARY_MATK: u16 = 43;
pub const MERCENARY_DEF: u16 = 45;
pub const MERCENARY_MDEF: u16 = 47;
pub const MERCENARY_HIT: u16 = 49;
pub const MERCENARY_CRITICAL: u16 = 52;
pub const MERCENARY_ASPD: u16 = 53;
pub const MERCENARY_FLEE: u16 = 165;
pub const MERCENARY_KILLS: u16 = 189;
pub const MERCENARY_LOYALTY: u16 = 190;
const ZC_CHANGESTATE_MER: u16 = 0x0230;
const ZC_PROPERTY_HOMUN: u16 = 0x0BA4;
const ZC_HO_PAR_CHANGE2: u16 = 0x0BA5;
const ZC_HOSKILLINFO_LIST: u16 = 0x0235;
const ZC_HOSKILLINFO_UPDATE: u16 = 0x0239;
const ZC_FEED_MER: u16 = 0x022F;
const CZ_COMMAND_MER: u16 = 0x022D;
const CZ_RENAME_MER: u16 = 0x0231;
/// ZC_CHANGESTATE_MER's states: the unit is the player's homunculus, its intimacy (0-1000), its hunger (0-100).
pub const HOMUNCULUS_OWNED: u8 = 0;
pub const HOMUNCULUS_INTIMACY: u8 = 1;
pub const HOMUNCULUS_HUNGER: u8 = 2;
/// CZ_COMMAND_MER's feed.
pub const HOMUNCULUS_FEED: u8 = 1;
/// ZC_HO_PAR_CHANGE2's kinds.
pub const HOMUNCULUS_EXP: u16 = 1;
pub const HOMUNCULUS_HP: u16 = 5;
pub const HOMUNCULUS_SP: u16 = 7;
const ZC_PET_ACT: u16 = 0x01AA;
const CZ_PET_ACT: u16 = 0x01A9;
const ZC_ACK_REQNAMEALL_NPC: u16 = 0x0ADF;
/// ZC_UI_OPEN: a window by kind. Kind 1 is the stylist, kind 8 is grade enchanting.
const ZC_UI_OPEN: u16 = 0x0AE2;
const ZC_STYLE_CHANGE_RES: u16 = 0x0A47;
/// The 16-byte style request. The 14-byte CZ_REQ_STYLE_CHANGE (0x0A46) is not what this version reads.
const CZ_REQ_STYLE_CHANGE2: u16 = 0x0AFC;
const CZ_REQ_STYLE_CLOSE: u16 = 0x0A48;
const ZC_OPEN_REFINING_UI: u16 = 0x0AA0;
const CZ_REFINING_SELECT_ITEM: u16 = 0x0AA1;
const ZC_REFINING_MATERIAL_LIST: u16 = 0x0AA2;
const CZ_REQ_REFINING: u16 = 0x0AA3;
const CZ_CLOSE_REFINING_UI: u16 = 0x0AA4;
const ZC_ACK_ITEMREFINING: u16 = 0x0188;
const CZ_GRADE_ENCHANT_SELECT_EQUIPMENT: u16 = 0x0B59;
const ZC_GRADE_ENCHANT_MATERIAL_LIST: u16 = 0x0B5A;
const CZ_GRADE_ENCHANT_REQUEST: u16 = 0x0B5B;
const CZ_GRADE_ENCHANT_CLOSE_UI: u16 = 0x0B5C;
const ZC_GRADE_ENCHANT_ACK: u16 = 0x0B5D;
const ZC_GRADE_ENCHANT_BROADCAST_RESULT: u16 = 0x0B5E;
const ZC_RANDOM_COMBINE_ITEM_UI_OPEN: u16 = 0x0A4E;
const CZ_REQ_RANDOM_COMBINE_ITEM: u16 = 0x0A4F;
const ZC_ACK_RANDOM_COMBINE_ITEM: u16 = 0x0A50;
const CZ_RANDOM_COMBINE_ITEM_UI_CLOSE: u16 = 0x0A70;
const ZC_RANDOM_UPGRADE_ITEM_UI_OPEN: u16 = 0x0AB4;
const CZ_RANDOM_UPGRADE_ITEM_UI_CLOSE: u16 = 0x0AB5;
const CZ_REQ_RANDOM_UPGRADE_ITEM: u16 = 0x0AB6;
const ZC_ACK_RANDOM_UPGRADE_ITEM: u16 = 0x0AB7;
const ZC_CHANGE_ITEM_OPTION: u16 = 0x0B43;
const ZC_OPEN_REFORM_UI: u16 = 0x0B8F;
const CZ_CLOSE_REFORM_UI: u16 = 0x0B90;
const CZ_ITEM_REFORM: u16 = 0x0B91;
const ZC_ITEM_REFORM_ACK: u16 = 0x0B92;

/// ZC_UI_OPEN_V3's enchant window.
const ENCHANT_UI: u8 = 10;
const ZC_UI_OPEN_V3: u16 = 0x0B9A;
const CZ_REQUEST_RANDOM_ENCHANT: u16 = 0x0B9B;
const CZ_REQUEST_PERFECT_ENCHANT: u16 = 0x0B9C;
const CZ_REQUEST_UPGRADE_ENCHANT: u16 = 0x0B9D;
const CZ_REQUEST_RESET_ENCHANT: u16 = 0x0B9E;
const ZC_RESPONSE_ENCHANT: u16 = 0x0B9F;
const CZ_CLOSE_UI_ENCHANT: u16 = 0x0BA0;
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
const ZC_MILLENNIUMSHIELD: u16 = 0x0440;
const ZC_SPIRITS_ATTRIBUTE: u16 = 0x08CF;
const ZC_SOULENERGY: u16 = 0x0B73;
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
const CZ_REG_CHANGE_GUILD_POSITIONINFO: u16 = 0x0161;
const ZC_ACK_CHANGE_GUILD_POSITIONINFO: u16 = 0x0174;
const CZ_REQ_CHANGE_MEMBERPOS: u16 = 0x0155;
const ZC_ACK_REQ_CHANGE_MEMBERS: u16 = 0x0156;
const ZC_GUILD_POSITION: u16 = 0x0AFD;
const CZ_REQ_MAKE_GUILD: u16 = 0x0165;
const CZ_JOIN_GUILD: u16 = 0x016B;
const CZ_GUILD_CHAT: u16 = 0x017E;
const CZ_REQ_JOIN_GUILD2: u16 = 0x0916;
const ZC_MYGUILD_BASIC_INFO: u16 = 0x014C;
const CZ_REQ_ALLY_GUILD: u16 = 0x0170;
const ZC_REQ_ALLY_GUILD: u16 = 0x0171;
const CZ_ALLY_GUILD: u16 = 0x0172;
const ZC_ACK_REQ_ALLY_GUILD: u16 = 0x0173;
const CZ_REQ_HOSTILE_GUILD: u16 = 0x0180;
const ZC_ACK_REQ_HOSTILE_GUILD: u16 = 0x0181;
const CZ_REQ_DELETE_RELATED_GUILD: u16 = 0x0183;
const ZC_DELETE_RELATED_GUILD: u16 = 0x0184;
const CZ_REQ_DISORGANIZE_GUILD: u16 = 0x015D;
const ZC_ACK_DISORGANIZE_GUILD_RESULT: u16 = 0x015E;
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
const ZC_NOTIFY_CLAN_CONNECTINFO: u16 = 0x0988;
const ZC_ACK_CLAN_LEAVE: u16 = 0x0989;
const ZC_CLANINFO: u16 = 0x098A;
const CZ_CLAN_CHAT: u16 = 0x098D;
const ZC_NOTIFY_CLAN_CHAT: u16 = 0x098E;
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
/// CZ_STANDING_RESURRECTION. rathena `clif_parse_AutoRevive` reads packet 0x0292, length 2, and calls `pc_revive_item`.
const CZ_STANDING_RESURRECTION: u16 = 0x0292;

/// CZ_STANDING_RESURRECTION: the 2-byte header 0x0292 and nothing after it.
pub fn encode_standing_resurrection() -> [u8; 2] {
    CZ_STANDING_RESURRECTION.to_le_bytes()
}
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
const CZ_GROUPINFO_CHANGE_V2: u16 = 0x07D7;
const CZ_CHANGE_GROUP_MASTER: u16 = 0x07DA;
const ZC_ITEM_PICKUP_PARTY: u16 = 0x0B67;
const CZ_PARTY_CONFIG: u16 = 0x02C8;
const ZC_PARTY_CONFIG: u16 = 0x02C9;
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
    /// A sprite direction (`sprite_direction`).
    pub direction: u8,
    pub level: u16,
    pub name: String,
    /// Sitting when it came into view.
    pub sitting: bool,
    /// rathena's opt1 (OPT1_*): stone, freeze, and the rest.
    pub body_state: u16,
    /// rathena's opt2 (OPT2_*): poison, curse, and the rest.
    pub health_state: u16,
    /// Option bits (rathena's OPTION_*: mounts, outfits, hiding, ...).
    pub option: u32,
    /// A player's guild and its emblem's version (0 for none).
    pub guild: u32,
    pub emblem: u32,
    /// HP and max HP, when the server tells them (rathena does for a hurt monster).
    pub hp: Option<[i32; 2]>,
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

/// What kind of announcement a ZC_BROADCAST is, by its prefix.
#[derive(Debug, Clone, PartialEq)]
pub enum BroadcastKind {
    Normal,
    /// "ssss": War of Emperium notices.
    Woe,
    /// "micc": a speaker's, by name (dropped when the player blocks that name).
    Speaker(String),
}

const BROADCAST_YELLOW: [u8; 3] = [0xFF, 0xFF, 0x00];
const BROADCAST_BLUE: [u8; 3] = [0x00, 0xFF, 0xFF];

/// "RRGGBB" as a color; yellow when it isn't one.
fn hex_color(text: &[u8]) -> [u8; 3] {
    let hex = std::str::from_utf8(text).ok().and_then(|text| u32::from_str_radix(text, 16).ok()).filter(|_| text.len() == 6);
    hex.map_or(BROADCAST_YELLOW, |value| [(value >> 16) as u8, (value >> 8) as u8, value as u8])
}

/// ZC_BROADCAST's text (from byte 4): its prefix ("tool", "blue", "micc", "ssss", any case) decides the color and
/// where the words start; without one, a `#…` part up to the first `:` is cut out.
fn decode_broadcast(message: &[u8]) -> (String, [u8; 3], BroadcastKind) {
    let prefix = message.get(..4).map(<[u8]>::to_ascii_lowercase).unwrap_or_default();
    let text = |from: usize| c_string(message.get(from..).unwrap_or_default());
    match prefix.as_slice() {
        b"tool" => (text(10), hex_color(message.get(4..10).unwrap_or_default()), BroadcastKind::Normal),
        b"blue" => (text(4), BROADCAST_BLUE, BroadcastKind::Normal),
        b"micc" => (text(34), hex_color(message.get(28..34).unwrap_or_default()), BroadcastKind::Speaker(c_string(message.get(4..28).unwrap_or_default()))),
        b"ssss" => (text(4), BROADCAST_YELLOW, BroadcastKind::Woe),
        _ => {
            let mut text = text(0);
            if let Some(hash) = text.char_indices().skip(1).find(|&(_, character)| character == '#').map(|(at, _)| at)
                && let Some(colon) = text[hash..].find(':')
            {
                text.replace_range(hash..hash + colon, "");
            }
            (text, BROADCAST_YELLOW, BroadcastKind::Normal)
        }
    }
}

/// A store the Universal Catalog found (ZC_SEARCH_STORE_INFO_ACK).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CatalogRow {
    pub store: u32,
    /// The owner's account.
    pub account: u32,
    pub shop: String,
    pub item: u32,
    pub kind: u8,
    pub price: u32,
    pub amount: u16,
    pub cards: [u32; 4],
    pub refine: u8,
}

/// The player's mercenary as ZC_MER_INIT has it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mercenary {
    /// Its unit's ID.
    pub id: u32,
    pub atk: u16,
    pub matk: u16,
    pub hit: u16,
    pub critical: u16,
    pub def: u16,
    pub mdef: u16,
    pub flee: u16,
    /// Attack motion, as the status window's ASPD value.
    pub aspd: u16,
    pub name: String,
    pub level: u16,
    pub hp: u32,
    pub max_hp: u32,
    pub sp: u32,
    pub max_sp: u32,
    /// When the contract ends, in Unix seconds.
    pub contract_end: u32,
    pub loyalty: u16,
    pub summons: u32,
    pub kills: u32,
    pub range: u16,
}

impl Mercenary {
    /// A ZC_MER_PAR_CHANGE value.
    pub fn set(&mut self, kind: u16, value: u32) {
        let short = value.min(u32::from(u16::MAX)) as u16;
        match kind {
            MERCENARY_HP => self.hp = value,
            MERCENARY_MAX_HP => self.max_hp = value,
            MERCENARY_SP => self.sp = value,
            MERCENARY_MAX_SP => self.max_sp = value,
            MERCENARY_ATK => self.atk = short,
            MERCENARY_MATK => self.matk = short,
            MERCENARY_DEF => self.def = short,
            MERCENARY_MDEF => self.mdef = short,
            MERCENARY_HIT => self.hit = short,
            MERCENARY_CRITICAL => self.critical = short,
            MERCENARY_ASPD => self.aspd = short,
            MERCENARY_FLEE => self.flee = short,
            MERCENARY_KILLS => self.kills = value,
            MERCENARY_LOYALTY => self.loyalty = short,
            _ => {}
        }
    }
}

/// The player's homunculus as ZC_PROPERTY_HOMUN has it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Homunculus {
    pub name: String,
    /// Bit 0: renamed, bit 1: vaporized, bit 2: alive.
    pub flags: u8,
    pub level: u16,
    /// 0-100.
    pub hunger: u16,
    /// 0-1000.
    pub intimacy: u16,
    pub atk: u16,
    pub matk: u16,
    pub hit: u16,
    pub critical: u16,
    pub def: u16,
    pub mdef: u16,
    pub flee: u16,
    /// Attack motion, as the status window's ASPD value.
    pub aspd: u16,
    pub hp: u32,
    pub max_hp: u32,
    pub sp: u32,
    pub max_sp: u32,
    pub exp: i64,
    /// 0 at the top level.
    pub next_exp: i64,
    pub skill_points: u16,
    pub range: u16,
}

impl Homunculus {
    /// It has a name of the player's choosing and can't be renamed again.
    pub fn renamed(&self) -> bool {
        self.flags & 1 != 0
    }

    pub fn vaporized(&self) -> bool {
        self.flags & 2 != 0
    }

    pub fn alive(&self) -> bool {
        self.flags & 4 != 0
    }

    /// A ZC_HO_PAR_CHANGE2 value.
    pub fn set(&mut self, kind: u16, value: u64) {
        let short = value.min(u64::from(u32::MAX)) as u32;
        match kind {
            HOMUNCULUS_EXP => self.exp = value.min(i64::MAX as u64) as i64,
            HOMUNCULUS_HP => self.hp = short,
            HOMUNCULUS_SP => self.sp = short,
            _ => {}
        }
    }
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

/// What Sense tells of a monster: its class, level, size (0 small, 1 medium, 2 large), HP, DEF, race (0 formless to
/// 9 dragon), MDEF, element (0 neutral to 9 undead) and the damage % water to undead attacks do to it.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MonsterInfo {
    pub class: u16,
    pub level: u16,
    pub size: u16,
    pub hp: u32,
    pub def: i16,
    pub race: u16,
    pub mdef: i16,
    pub element: u16,
    pub resists: [u8; 9],
}

/// What the client asks of the Lucky Roulette.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RouletteRequest {
    Open,
    Prizes,
    Close,
    Spin,
    Claim,
}

/// A Lucky Roulette prize: its place (row 0 at the bottom, 9 wide, up to row 6, 3 wide), the item and how many.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoulettePrize {
    pub row: u8,
    pub column: u8,
    pub item: u32,
    pub amount: u16,
}

/// Blessings on a grade try: the item, how many one step takes, the most steps, and the chance each step adds (percent).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GradeBlessing {
    pub item: u32,
    pub per_step: u32,
    pub most: u32,
    pub chance: u32,
}

/// One material row of a grade try. `lowers_refine` and `can_break` are the option's flags (0 or 1).
#[derive(Debug, Clone, PartialEq)]
pub struct GradeMaterial {
    pub item: u32,
    pub amount: u32,
    pub zeny: u32,
    pub lowers_refine: bool,
    pub can_break: bool,
}

#[derive(Debug)]
pub enum MapEvent {
    Entered {
        position: [u16; 2],
        direction: u8,
        /// The server's tick, and when it came.
        clock: (u32, Instant),
    },
    /// The server's tick, answering ours, and when it came.
    ServerTime(u32, Instant),
    /// A unit is standing in view (appeared or was already there).
    Unit(Unit),
    /// A unit walking in view; its `position` is where the walk starts.
    UnitWalk {
        unit: Unit,
        to: [u16; 2],
        start_time: u32,
        /// Where in its start cell it is, in sixteenths (8: the centre).
        sub: [u8; 2],
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
    /// A unit is at a cell now, moved there at once (a knockback, Taekwon Jump, Snap, a pet put by its owner).
    Slide {
        id: u32,
        position: [u16; 2],
    },
    /// A unit may use its next combo skill for this long.
    ComboWindow {
        id: u32,
        ms: u32,
    },
    /// Blade Stop or Cursed Circle locks two units together, or lets go.
    BladeStop {
        source: u32,
        target: u32,
        on: bool,
    },
    /// A unit turned its body (a sprite direction) or (players) its head.
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
    /// A buying store license or skill went off: set up a buying store of up to this many items (ZC_OPEN_BUYING_STORE).
    OpenBuyingStore {
        slots: u8,
    },
    /// Opening our buying store failed (ZC_FAILED_OPEN_BUYING_STORE_TO_BUYER).
    BuyingStoreOpenFailed {
        result: u16,
        weight: u32,
    },
    /// Our buying store opened: our account, the zeny limit, and the wanted items (ZC_MYITEMLIST_BUYING_STORE).
    MyBuyingStore {
        account: u32,
        limit: u32,
        items: Vec<BuyingItem>,
    },
    /// A buying store's sign over its owner's head, and its going away (ZC_BUYING_STORE_ENTRY, ZC_DISAPPEAR_BUYING_STORE_ENTRY).
    BuyingStoreSign {
        owner: u32,
        title: String,
    },
    BuyingStoreSignGone {
        owner: u32,
    },
    /// What another player's buying store wants to buy (ZC_ACK_ITEMLIST_BUYING_STORE).
    BuyingStoreItems {
        owner: u32,
        store_id: u32,
        limit: u32,
        items: Vec<BuyingItem>,
    },
    /// The server closed our buying store after a trade or limit check (ZC_FAILED_TRADE_BUYING_STORE_TO_BUYER).
    BuyingStoreBuyerFailed {
        result: u16,
    },
    /// An item we sold into a buying store left our inventory (ZC_ITEM_DELETE_BUYING_STORE).
    BuyingStoreSold {
        index: u16,
        amount: u16,
        price: u32,
    },
    /// Selling into a buying store failed (ZC_FAILED_TRADE_BUYING_STORE_TO_SELLER).
    BuyingStoreSellerFailed {
        result: u16,
        item_id: u32,
    },
    /// Someone sold an item into our buying store (ZC_UPDATE_ITEM_FROM_BUYING_STORE2).
    BuyingStoreUpdate {
        item_id: u32,
        amount: u16,
        zeny: u32,
        limit_left: u32,
        char_id: u32,
        time: u32,
    },
    /// An announcement: plain ones yellow (or blue when the server asks), ZC_BROADCAST2 in its own color.
    /// The map's property (rathena's `map_property`) and its flags (MAP_*), at every load and flag change.
    MapProperty {
        property: u16,
        flags: u32,
    },
    /// A script opens the dress room (`opendressroom`).
    DressRoom,
    /// A Universal Catalog opens the store search: `remote` opens a store found (effect 1), else it shows where it
    /// is; the searches left.
    CatalogOpen {
        remote: bool,
        uses: u8,
    },
    /// A page of results: the first replaces the list, later ones add to it; `more`: a next page exists.
    CatalogResults {
        first: bool,
        more: bool,
        uses: u8,
        rows: Vec<CatalogRow>,
    },
    /// A search failed: 0 nothing found, 1 too many, 2 no searches left, 3 too soon, 4 the store's gone.
    CatalogFailed(u8),
    /// Where a store found stands on this map; none when it's on another.
    CatalogMark(Option<[i16; 2]>),
    /// A ranking's top ten (0 Blacksmith, 1 Alchemist, 2 Taekwon, 3 Killer): character IDs (0: an empty place) and
    /// points, and the player's own points.
    Ranking {
        kind: u16,
        ids: [u32; 10],
        points: [u32; 10],
        mine: u32,
    },
    /// A character's name by ID (none when the server has none).
    CharName {
        id: u32,
        name: Option<String>,
    },
    /// Rank points the player earned (or lost), and the new total.
    RankingPoints {
        kind: u16,
        added: i32,
        total: u32,
    },
    /// A Star Gladiator's or Taekwon's target (STAR_MISSION: the Taekwon Mission, `count` kills of 100 done).
    StarSkill {
        name: String,
        monster: u32,
        count: u8,
        kind: u8,
    },
    /// The map's type (rathena's `e_map_type`; it only sends 19, a battleground).
    MapType(u16),
    /// The wedding effect on a unit (ZC_CONGRATULATION).
    Congratulation(u32),
    /// Someone nearby called their partner: the partner's name, empty when offline (ZC_COUPLENAME).
    PartnerName(String),
    /// Divorced from this partner (ZC_DIVORCE).
    Divorced(String),
    /// A parent (account) asks, with their partner (account), to adopt the player (ZC_REQ_BABY).
    AdoptAsked {
        parent: u32,
        partner: u32,
        name: String,
    },
    /// Asking to adopt, refused (ZC_BABYMSG): 0 a child already, 1 under level 70, 2 the one asked is married.
    AdoptRefused(u32),
    /// A teammate's HP on a battleground (ZC_BATTLEFIELD_NOTIFY_HP2).
    BattleHp {
        account: u32,
        hp: i32,
        max: i32,
    },
    /// A teammate's cell on the minimap (ZC_BATTLEFIELD_NOTIFY_POSITION); none when they left.
    BattlePosition {
        account: u32,
        cell: Option<[u16; 2]>,
    },
    /// A player's camp, rathena's team ID (ZC_BATTLEFIELD_NOTIFY_CAMPINFO).
    BattleCamp {
        account: u32,
        camp: u16,
    },
    /// Applying to a battleground's queue, answered (ZC_ACK_ENTRY_QUEUE_APPLY): rathena's `e_bg_queue_apply_ack`.
    BattleQueue {
        result: u8,
        name: String,
    },
    /// How many are queued for it (ZC_NOTIFY_ENTRY_QUEUE_APPLY).
    QueueNumber {
        name: String,
        number: u32,
    },
    /// Leaving the queue, answered (ZC_ACK_ENTRY_QUEUE_CANCEL): 1 left, 0 wasn't in it.
    QueueLeft {
        result: u8,
        name: String,
    },
    /// Both teams are full: enter? (ZC_NOTIFY_LOBBY_ADMISSION)
    QueueReady {
        name: String,
    },
    /// Entering, answered (ZC_REPLY_ACK_LOBBY_ADMISSION).
    QueueAccepted {
        result: u8,
        name: String,
    },
    /// Forget the queue: declined, expired, or gone in (ZC_ENTRY_QUEUE_INIT).
    QueueReset,
    /// Camp A's points and camp B's (ZC_BATTLEFIELD_NOTIFY_POINT).
    BattleScore([u16; 2]),
    /// A line of the team's chat, whole ("Name : hello"), and who said it (account 0: the server).
    BattleChat {
        account: u32,
        text: String,
    },
    /// A player's PvP rank among the players on the map (rank 0 or 0 players: no counter).
    PvpRank {
        account: u32,
        rank: u32,
        players: u32,
    },
    /// The player's PvP record (/pvpinfo).
    PvpInfo {
        wins: u32,
        losses: u32,
        points: i32,
    },
    /// A server announcement, its color resolved; `size` is ZC_BROADCAST2's font size.
    Broadcast {
        text: String,
        color: [u8; 3],
        kind: BroadcastKind,
        size: Option<u16>,
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
    /// A guild member's cell on this map, for the minimap; none when they left it (ZC_NOTIFY_POSITION_TO_GUILDM).
    GuildMemberPlace {
        account: u32,
        cell: Option<[i32; 2]>,
    },
    /// The weight at which the player counts as overweight, in percent of the limit (ZC_WEIGHT_INFO).
    OverweightPercent(u32),
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
    /// How many Millennium Shield shields (a Royal Guard's: rage spheres) a unit carries.
    Shields {
        id: u32,
        count: u16,
    },
    /// A unit's charms: their element (0 none, 1 water, 2 earth, 3 fire, 4 wind) and how many.
    Charms {
        id: u32,
        kind: u16,
        count: u16,
    },
    /// How many souls a unit carries.
    Souls {
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
    CashShopOpen {
        cash: u32,
        free: u32,
        tab: u32,
    },
    CashShopList {
        tab: i16,
        items: Vec<(u32, u32)>,
    },
    CashShopBuyResult {
        item: u32,
        result: u16,
        cash: u32,
        free: u32,
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
    /// The stylist's answer (ZC_STYLE_CHANGE_RES): true when the change failed.
    StyleChanged(bool),
    /// The refine window opens.
    RefineOpen,
    /// The enchant window opens on this enchant table.
    EnchantOpen(u64),
    /// An enchant's answer: a message number, and the enchant put in (0 on a failure and after a reset).
    Enchanted {
        message: u32,
        item: u32,
    },
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
    /// ZC_UI_OPEN: a window by kind (1 is the stylist, 8 is grade enchanting) and the data it carries.
    UiOpen {
        kind: u8,
        data: u32,
    },
    /// What grades an item, or index −1 when its level has none. Chance is a percent. Blessing steps add `chance`
    /// each, up to `most`. Each material row is one option, in the order the request numbers them.
    GradeMaterials {
        index: i16,
        chance: u32,
        blessing: GradeBlessing,
        materials: Vec<GradeMaterial>,
    },
    /// A grade try: the item's grade after it, and the result (0 raised, 1 failed, 2 refine down, 3 destroyed, 4 protected).
    GradeResult {
        index: u16,
        grade: u16,
        result: u32,
    },
    /// Someone's grade try, told to every player: success raised it, failure did not.
    GradeAnnounced {
        player: String,
        item: u32,
        grade: u16,
        success: bool,
    },
    /// A Laphine synthesis box opened (the box's item ID).
    SynthesisOpen(u32),
    /// Synthesis result: 0 done, 5 an amount is wrong, 7 an item isn't wanted.
    Synthesized(u16),
    /// A Laphine upgrade box opened (the box's item ID).
    UpgradeOpen(u32),
    /// Upgrade result: 0 done, 1 refused. `Upgraded` is already Upgrade Weapon.
    LaphineUpgraded(u16),
    /// The item an upgrade just rewrote: its cards, refine and grade. Random options aren't tracked.
    ItemPreview {
        index: u16,
        cards: [u32; 4],
        refine: u8,
        grade: u8,
        broken: bool,
    },
    /// The item reform window opens for this reform item (the box, or the item an NPC named).
    ReformOpen(u32),
    /// A reform finished: result 0 means done, and the client closes the window.
    Reformed {
        index: u16,
        result: u8,
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
    /// The player's mercenary: hired, or on a map load.
    MercenaryInfo(Mercenary),
    /// The mercenary's skills.
    MercenarySkills(Vec<Skill>),
    /// One of the mercenary's values changed (a MERCENARY_* kind).
    MercenaryChanged {
        kind: u16,
        value: u32,
    },
    /// ZC_CHANGESTATE_MER: a HOMUNCULUS_* state of the homunculus `id`.
    HomunculusState {
        state: u8,
        id: u32,
        value: u32,
    },
    /// What the player's homunculus is like now.
    HomunculusInfo(Homunculus),
    /// One of the homunculus's values changed (a HOMUNCULUS_* kind).
    HomunculusChanged {
        kind: u16,
        value: u64,
    },
    /// The homunculus's skills.
    HomunculusSkills(Vec<Skill>),
    /// One of the homunculus's skills changed; its kind isn't in the packet (0).
    HomunculusSkill(Skill),
    /// The homunculus was fed (or there was no food), and its food.
    HomunculusFed {
        ok: bool,
        food: u32,
    },
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
    /// The material window opens: 0 for Change Material, 1 or 2 for Four Spirit Analysis at that level.
    MaterialWindow(u32),
    /// All the player's achievements, with the totals (at login, and after a reward that gives a title).
    Achievements {
        totals: AchievementTotals,
        list: Vec<Achievement>,
    },
    /// One achievement added or changed (none: the totals only), with the totals.
    AchievementUpdate {
        totals: AchievementTotals,
        achievement: Option<Achievement>,
    },
    /// A reward claim taken, or refused.
    AchievementReward {
        ok: bool,
        id: u32,
    },
    /// The title put on (0: taken off), or refused.
    TitleChanged {
        ok: bool,
        title: u32,
    },
    /// A skill can't be used again for this long from now (0: it can again).
    SkillCooldown {
        skill: u16,
        ms: u32,
    },
    /// The roulette window opens: a prize waiting to be claimed (row, column), the bonus item, and the gold, silver
    /// and bronze points.
    RouletteOpen {
        prize: Option<(u8, u8)>,
        bonus: u32,
        points: [i32; 3],
    },
    /// The roulette's prizes.
    RoulettePrizes(Vec<RoulettePrize>),
    /// A spin's result (0 won, 1 failed, 2 not enough points, 3 lost) with where it stopped, the bonus and the points.
    RouletteSpun {
        result: u8,
        row: u8,
        column: u8,
        bonus: u32,
        points: [i32; 3],
    },
    /// A claim's result (0 claimed, 1 failed, 2 too many items, 3 too heavy) and the next bonus item.
    RouletteClaimed {
        result: u8,
        bonus: u32,
    },
    /// A bind-on-equip item (inventory index) was put on: it's bound to the character now.
    BoundOnEquip(u16),
    /// Take Off All was done, or refused (the player can't act).
    TookOffAll {
        refused: bool,
    },
    /// The Swap Equipment set: inventory index and the places it goes in.
    SwitchSet(Vec<(u16, u32)>),
    /// An item went into the set (result 0), or didn't: 1 level too low, 2 can't.
    SwitchAdded {
        index: u16,
        places: u32,
        result: u16,
    },
    /// An item left the set.
    SwitchRemoved {
        index: u16,
        places: u32,
    },
    /// The item merge window opens with the stacks (inventory indexes) that can merge.
    MergeOpen(Vec<u16>),
    /// Stacks merged into the one at `index`, now `amount` (result 0), or not: 2 the total is too big.
    Merged {
        index: u16,
        amount: u16,
        result: u8,
    },
    /// An item went into the favourites, or out.
    FavoriteMoved {
        index: u16,
        favorite: bool,
    },
    /// The set is switched with what's worn now.
    Switched {
        failed: bool,
    },
    /// A rental (by item ID) has this many seconds left.
    RentalTime {
        item: u32,
        seconds: u32,
    },
    /// The rental at an inventory index has ended.
    RentalExpired {
        index: u16,
        item: u32,
    },
    /// Sense's report on a monster.
    MonsterInfo(MonsterInfo),
    /// The Convex Mirror on the map's boss: 0 none here, 2 alive (first), 1 alive (moved), 3 dead (back in hours and
    /// minutes).
    BossInfo {
        kind: u8,
        cell: [i32; 2],
        hours: u16,
        minutes: u16,
        name: String,
    },
    /// A Talkie Box went off: its ground unit, and the message it held.
    TalkieBox {
        unit: u32,
        text: String,
    },
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
    /// Hunts that progressed with a kill (ZC_UPDATE_MISSION_HUNT_EX), as `QuestHunts`.
    QuestProgress(Vec<(u32, u16, u16)>),
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
    /// A position changed (ZC_ACK_CHANGE_GUILD_POSITIONINFO): its permissions and tax, and its title.
    GuildPositionChanged(GuildPosition, String),
    /// A member moved to another position (ZC_ACK_REQ_CHANGE_MEMBERS).
    GuildMemberPosition {
        account: u32,
        character: u32,
        position: u32,
    },
    /// The position title under a player's name (ZC_GUILD_POSITION); empty without a guild.
    PositionTitle {
        account: u32,
        title: String,
    },
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
    /// The guild's allies and antagonists.
    GuildRelations(Vec<GuildRelation>),
    /// Another guild (its name) asks ours for an alliance, through the player who asked (account ID).
    AllyAsked {
        id: u32,
        guild: String,
    },
    /// How an alliance request went (0 already allied, 1 they're answering another, 2 allied, 3 refused or they
    /// have too many, 4 we have too many, 5 not allowed).
    AllyResult(u8),
    /// How setting an antagonist went: 0 set, 1 too many, 2 already one.
    AntagonistResult(u8),
    /// A relation with a guild ended.
    GuildRelationEnded {
        id: u32,
        antagonist: bool,
    },
    /// Disbanding the guild: 0 done, 1 wrong name, 2 members are left.
    GuildDisbanded(u32),
    /// Someone (by character ID) left or was expelled, and why.
    GuildLeft {
        character: u32,
        reason: String,
        expelled: bool,
    },
    /// The character's clan, at login or upon joining (`ZC_CLANINFO`).
    ClanInfo {
        id: u32,
        name: String,
        master: String,
        map: String,
        allies: Vec<String>,
        hostile: Vec<String>,
    },
    /// Members online and the clan's capacity (`ZC_NOTIFY_CLAN_CONNECTINFO`).
    ClanOnlineCount {
        online: i16,
        max: i16,
    },
    /// The character left their clan (`ZC_ACK_CLAN_LEAVE`).
    ClanLeft,
    /// A line of clan chat (`ZC_NOTIFY_CLAN_CHAT`).
    ClanChat(String),
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
    /// A member's place on the minimap; none when they left the map.
    PartyPosition {
        account: u32,
        cell: Option<[u16; 2]>,
    },
    /// A member (by account) picked up an item.
    PartyPickup {
        account: u32,
        item: u32,
        identified: bool,
        refine: u8,
    },
    /// Whether the player refuses party invitations.
    PartyRefusing(bool),
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
    /// A unit's body state, health state and option bits changed (ZC_STATE_CHANGE).
    OptionChange {
        id: u32,
        /// rathena's opt1.
        body: u16,
        /// rathena's opt2.
        health: u16,
        option: u32,
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
    /// A unit's cast was cut off before the skill went (ZC_DISPEL).
    CastCancelled(u32),
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
    /// The damage is to SP (shown in blue).
    pub sp: bool,
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
    /// Where in the cell, in sixteenths from its low corner.
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

/// A wanted item in a buying store: price each, amount still wanted, item type, and item ID.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BuyingItem {
    pub price: u32,
    pub amount: u16,
    pub kind: u8,
    pub item_id: u32,
}

/// A monster a quest wants hunted: its objective's ID (the quest's × 1000 + its place), the monster's name, how many
/// so far and how many in all.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Hunt {
    pub id: u32,
    pub name: String,
    pub count: u16,
    pub needed: u16,
    /// The monsters' level range (0: none that way).
    pub levels: [u16; 2],
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
    /// The title worn (0: none).
    pub title: u32,
}

/// The player's achievement totals: score, rank (level), and the points into this rank and the points it needs.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct AchievementTotals {
    pub score: u32,
    pub level: u16,
    pub points: u32,
    pub needed: u32,
}

/// One of the player's achievements: done or not, each objective's count, when it was done (Unix time, 0 not yet),
/// and whether its reward was taken.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Achievement {
    pub id: u32,
    pub completed: bool,
    pub counts: [u32; 10],
    pub completed_at: u32,
    pub rewarded: bool,
}

/// A 50-byte achievement entry: ID, completed, ten counts, the time, rewarded.
fn decode_achievement(entry: &[u8]) -> Achievement {
    Achievement {
        id: u32_at(entry, 0),
        completed: entry[4] != 0,
        counts: std::array::from_fn(|objective| u32_at(entry, 5 + objective * 4)),
        completed_at: u32_at(entry, 45),
        rewarded: entry[49] != 0,
    }
}

/// Score, rank, points into it, points it needs.
fn decode_totals(bytes: &[u8]) -> AchievementTotals {
    AchievementTotals { score: u32_at(bytes, 0), level: u16_at(bytes, 4), points: u32_at(bytes, 6), needed: u32_at(bytes, 10) }
}

/// A guild allied with the player's, or held as an antagonist.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GuildRelation {
    pub antagonist: bool,
    pub id: u32,
    pub name: String,
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
    /// Enchant grade: 0 none, 1 D, 2 C, 3 B, 4 A.
    pub grade: u8,
    pub cards: [u32; 4],
    /// Broken: it can't be worn until it's repaired.
    pub broken: bool,
    /// 0 not bound, 1 binds to the character once worn, 2 bound.
    pub bind: u16,
    /// A rental's end, Unix time in seconds (0: not a rental).
    pub expires: u32,
    /// Where it goes in the Swap Equipment set; 0 when it isn't in it.
    pub switch: u32,
    /// In the favourites (rathena's flag; the client shows them under the etc tab).
    pub favorite: bool,
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
        expires: u32_at(entry, 29),
        identified: entry[33] & 1 != 0,
        favorite: entry[33] & 2 != 0,
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
        grade: entry[66],
        identified: entry[67] & 1 != 0,
        broken: entry[67] & 2 != 0,
        favorite: entry[67] & 4 != 0,
        bind: u16_at(entry, 35),
        expires: u32_at(entry, 31),
        switch: 0,
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
    stream: Wire,
}

/// The map server's socket: every packet written is shown to `SENT_HOOK` first.
struct Wire(TcpStream);

impl Wire {
    fn write_all(&mut self, packet: &[u8]) -> io::Result<()> {
        if let Some(hook) = SENT_HOOK.get() {
            hook(packet);
        }
        self.0.write_all(packet)
    }
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

    /// The master moves a member to another position.
    pub fn move_member(&mut self, account: u32, character: u32, position: u32) -> io::Result<()> {
        self.stream.write_all(&encode_member_move(account, character, position))
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
        let text = encoding_rs::EUC_KR.encode(&format!("{name} : {message}")).0.into_owned();
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

    /// Asks for the player's PvP record; the answer is `PvpInfo`.
    pub fn pvp_info(&mut self, character: u32, account: u32) -> io::Result<()> {
        self.stream.write_all(&encode_pvp_info(character, account))
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
        let _ = self.stream.0.shutdown(std::net::Shutdown::Both);
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
    encode_index(CZ_REQ_ITEMCOMPOSITION_LIST, card)
}

/// A header and an inventory index.
fn encode_index(header: u16, index: u16) -> [u8; 4] {
    let [[h0, h1], [i0, i1]] = [header, index].map(u16::to_le_bytes);
    [h0, h1, i0, i1]
}

/// CZ_REQ_EQUIPSWITCH_ADD: the inventory index, then the places.
/// CZ_REQ_MERGE_ITEM: the length, then the indexes; the first keeps the total.
fn encode_merge_items(indexes: &[u16]) -> Vec<u8> {
    let length = 4 + 2 * indexes.len() as u16;
    [CZ_REQ_MERGE_ITEM, length].iter().chain(indexes).flat_map(|value| value.to_le_bytes()).collect()
}

fn encode_switch_add(index: u16, places: u32) -> Vec<u8> {
    [&CZ_REQ_EQUIPSWITCH_ADD.to_le_bytes()[..], &index.to_le_bytes(), &places.to_le_bytes()].concat()
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

fn encode_cashshop_open(tab: u32) -> Vec<u8> {
    [&CZ_SE_CASHSHOP_OPEN2.to_le_bytes()[..], &tab.to_le_bytes()].concat()
}

fn encode_cashshop_list_request() -> Vec<u8> {
    CZ_CASHSHOP_LIST.to_le_bytes().to_vec()
}

fn encode_cashshop_buy(free: u32, items: &[(u32, u32, u16)]) -> Vec<u8> {
    let mut packet = CZ_SE_PC_BUY_CASHITEM_LIST.to_le_bytes().to_vec();
    packet.extend_from_slice(&((10 + items.len() * 10) as u16).to_le_bytes());
    packet.extend_from_slice(&(items.len() as u16).to_le_bytes());
    packet.extend_from_slice(&free.to_le_bytes());
    for &(item, amount, tab) in items {
        packet.extend_from_slice(&item.to_le_bytes());
        packet.extend_from_slice(&amount.to_le_bytes());
        packet.extend_from_slice(&tab.to_le_bytes());
    }
    packet
}

fn encode_cashshop_close() -> Vec<u8> {
    CZ_CASHSHOP_CLOSE.to_le_bytes().to_vec()
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

/// CZ_GRADE_ENCHANT_SELECT_EQUIPMENT: the inventory index.
fn encode_grade_select(index: u16) -> [u8; 4] {
    let mut packet = [0u8; 4];
    packet[..2].copy_from_slice(&CZ_GRADE_ENCHANT_SELECT_EQUIPMENT.to_le_bytes());
    packet[2..].copy_from_slice(&index.to_le_bytes());
    packet
}

/// CZ_GRADE_ENCHANT_REQUEST: the item, the material row, and blessing steps (0 sends the flag off). Protect stays 0.
fn encode_grade_enchant(index: u16, material: u32, steps: u32) -> [u8; 14] {
    let mut packet = [0u8; 14];
    packet[..2].copy_from_slice(&CZ_GRADE_ENCHANT_REQUEST.to_le_bytes());
    packet[2..4].copy_from_slice(&index.to_le_bytes());
    packet[4..8].copy_from_slice(&material.to_le_bytes());
    if steps > 0 {
        packet[8] = 1;
        packet[9..13].copy_from_slice(&steps.to_le_bytes());
    }
    packet
}

/// CZ_REQ_RANDOM_COMBINE_ITEM: length, the box's item ID as it came, then index and amount for each material.
fn encode_synthesis(box_id: u32, entries: &[(u16, u16)]) -> Vec<u8> {
    let mut packet = CZ_REQ_RANDOM_COMBINE_ITEM.to_le_bytes().to_vec();
    packet.extend_from_slice(&((8 + entries.len() * 4) as u16).to_le_bytes());
    packet.extend_from_slice(&box_id.to_le_bytes());
    for (index, amount) in entries {
        packet.extend_from_slice(&index.to_le_bytes());
        packet.extend_from_slice(&amount.to_le_bytes());
    }
    packet
}

/// CZ_GRADE_ENCHANT_CLOSE_UI.
fn encode_close_grade() -> [u8; 2] {
    CZ_GRADE_ENCHANT_CLOSE_UI.to_le_bytes()
}

/// Rows of the styling shop to buy. 0 leaves that part unchanged. Body style is sent, and this version keeps it 0.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StylePicks {
    pub hair_color: u16,
    pub hair_style: u16,
    pub clothes_color: u16,
    pub head_top: u16,
    pub head_mid: u16,
    pub head_bottom: u16,
    pub body_style: u16,
}

/// CZ_REQ_STYLE_CHANGE2: 16 bytes, one u16 row at offsets 2, 4, 6, 8, 10, 12 and 14.
fn encode_style_change(picks: StylePicks) -> [u8; 16] {
    let mut packet = [0u8; 16];
    packet[..2].copy_from_slice(&CZ_REQ_STYLE_CHANGE2.to_le_bytes());
    let rows = [picks.hair_color, picks.hair_style, picks.clothes_color, picks.head_top, picks.head_mid, picks.head_bottom, picks.body_style];
    for (index, row) in rows.into_iter().enumerate() {
        packet[2 + index * 2..4 + index * 2].copy_from_slice(&row.to_le_bytes());
    }
    packet
}

fn encode_synthesis_close() -> [u8; 2] {
    CZ_RANDOM_COMBINE_ITEM_UI_CLOSE.to_le_bytes()
}

/// CZ_REQ_RANDOM_UPGRADE_ITEM: the box's item ID, then the inventory index.
fn encode_upgrade_item(box_id: u32, index: u16) -> [u8; 8] {
    let mut packet = [0u8; 8];
    packet[..2].copy_from_slice(&CZ_REQ_RANDOM_UPGRADE_ITEM.to_le_bytes());
    packet[2..6].copy_from_slice(&box_id.to_le_bytes());
    packet[6..].copy_from_slice(&index.to_le_bytes());
    packet
}

fn encode_upgrade_close() -> [u8; 2] {
    CZ_RANDOM_UPGRADE_ITEM_UI_CLOSE.to_le_bytes()
}

/// CZ_ITEM_REFORM: the reform item, then the inventory index of the item to reform.
fn encode_item_reform(item: u32, index: u16) -> [u8; 8] {
    let mut packet = [0u8; 8];
    packet[..2].copy_from_slice(&CZ_ITEM_REFORM.to_le_bytes());
    packet[2..6].copy_from_slice(&item.to_le_bytes());
    packet[6..].copy_from_slice(&index.to_le_bytes());
    packet
}

/// CZ_CLOSE_REFORM_UI: the header only.
fn encode_close_reform() -> [u8; 2] {
    CZ_CLOSE_REFORM_UI.to_le_bytes()
}

/// CZ_REQUEST_RANDOM_ENCHANT: the enchant table and the item's inventory index.
fn encode_enchant_random(table: u64, index: u16) -> [u8; 12] {
    let mut packet = [0u8; 12];
    packet[..2].copy_from_slice(&CZ_REQUEST_RANDOM_ENCHANT.to_le_bytes());
    packet[2..10].copy_from_slice(&table.to_le_bytes());
    packet[10..].copy_from_slice(&index.to_le_bytes());
    packet
}

/// CZ_REQUEST_PERFECT_ENCHANT: the table, the item, and the enchant's item ID.
fn encode_enchant_perfect(table: u64, index: u16, item: u32) -> [u8; 16] {
    let mut packet = [0u8; 16];
    packet[..2].copy_from_slice(&CZ_REQUEST_PERFECT_ENCHANT.to_le_bytes());
    packet[2..10].copy_from_slice(&table.to_le_bytes());
    packet[10..12].copy_from_slice(&index.to_le_bytes());
    packet[12..].copy_from_slice(&item.to_le_bytes());
    packet
}

/// CZ_REQUEST_UPGRADE_ENCHANT: the table, the item, and the card slot (0-3).
fn encode_enchant_upgrade(table: u64, index: u16, slot: u16) -> [u8; 14] {
    let mut packet = [0u8; 14];
    packet[..2].copy_from_slice(&CZ_REQUEST_UPGRADE_ENCHANT.to_le_bytes());
    packet[2..10].copy_from_slice(&table.to_le_bytes());
    packet[10..12].copy_from_slice(&index.to_le_bytes());
    packet[12..].copy_from_slice(&slot.to_le_bytes());
    packet
}

/// CZ_REQUEST_RESET_ENCHANT: the table and the item.
fn encode_enchant_reset(table: u64, index: u16) -> [u8; 12] {
    let mut packet = [0u8; 12];
    packet[..2].copy_from_slice(&CZ_REQUEST_RESET_ENCHANT.to_le_bytes());
    packet[2..10].copy_from_slice(&table.to_le_bytes());
    packet[10..].copy_from_slice(&index.to_le_bytes());
    packet
}

/// CZ_CLOSE_UI_ENCHANT: the header alone.
fn encode_enchant_close() -> [u8; 2] {
    CZ_CLOSE_UI_ENCHANT.to_le_bytes()
}

fn encode_style_close() -> [u8; 2] {
    CZ_REQ_STYLE_CLOSE.to_le_bytes()
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

/// CZ_SEARCH_STORE_INFO: kind (0 vending, 1 buying), max then min price, the item and card counts, the IDs.
fn encode_store_search(buying: bool, max: u32, min: u32, items: &[u32], cards: &[u32]) -> Vec<u8> {
    let mut packet = CZ_SEARCH_STORE_INFO.to_le_bytes().to_vec();
    packet.extend_from_slice(&[0, 0, u8::from(buying)]);
    packet.extend_from_slice(&max.to_le_bytes());
    packet.extend_from_slice(&min.to_le_bytes());
    packet.extend_from_slice(&[items.len().min(255) as u8, cards.len().min(255) as u8]);
    for id in items.iter().take(255).chain(cards.iter().take(255)) {
        packet.extend_from_slice(&id.to_le_bytes());
    }
    let length = packet.len() as u16;
    packet[2..4].copy_from_slice(&length.to_le_bytes());
    packet
}

/// CZ_SSILIST_ITEM_CLICK: the owner's account, the store, the item, as the result gave them.
fn encode_store_pick(account: u32, store: u32, item: u32) -> [u8; 14] {
    let mut packet = [0u8; 14];
    packet[..2].copy_from_slice(&CZ_SSILIST_ITEM_CLICK.to_le_bytes());
    packet[2..6].copy_from_slice(&account.to_le_bytes());
    packet[6..10].copy_from_slice(&store.to_le_bytes());
    packet[10..].copy_from_slice(&item.to_le_bytes());
    packet
}

/// CZ_REQ_RANKING: the ranking's type.
fn encode_ranking_request(kind: u16) -> [u8; 4] {
    let [[h0, h1], [k0, k1]] = [CZ_REQ_RANKING, kind].map(u16::to_le_bytes);
    [h0, h1, k0, k1]
}

/// CZ_REQNAME_BYGID: the character ID.
fn encode_name_request(id: u32) -> [u8; 6] {
    let mut packet = [0u8; 6];
    packet[..2].copy_from_slice(&CZ_REQNAME_BYGID.to_le_bytes());
    packet[2..].copy_from_slice(&id.to_le_bytes());
    packet
}

/// CZ_REQ_PVPPOINT: the character and account (rathena reads neither).
fn encode_pvp_info(character: u32, account: u32) -> [u8; 10] {
    let mut packet = [0u8; 10];
    packet[..2].copy_from_slice(&CZ_REQ_PVPPOINT.to_le_bytes());
    packet[2..6].copy_from_slice(&character.to_le_bytes());
    packet[6..].copy_from_slice(&account.to_le_bytes());
    packet
}

/// CZ_BLOCKING_PLAY_CANCEL: the header alone.
fn encode_blocking_play_cancel() -> [u8; 2] {
    CZ_BLOCKING_PLAY_CANCEL.to_le_bytes()
}

/// CZ_GROUPINFO_CHANGE_V2: EXP (0 each takes, 1 even) u32, then the pickup and distribution flags.
fn encode_party_options(exp_even: bool, pickup: bool, share: bool) -> [u8; 8] {
    let mut packet = [0u8; 8];
    packet[..2].copy_from_slice(&CZ_GROUPINFO_CHANGE_V2.to_le_bytes());
    packet[2..6].copy_from_slice(&u32::from(exp_even).to_le_bytes());
    packet[6] = u8::from(pickup);
    packet[7] = u8::from(share);
    packet
}

/// CZ_REQUEST_MOVENPC: the unit and the cell (rathena ignores the direction).
fn encode_move_npc(id: u32, cell: [u16; 2]) -> [u8; 9] {
    let mut packet = [0u8; 9];
    packet[..2].copy_from_slice(&CZ_REQUEST_MOVENPC.to_le_bytes());
    packet[2..6].copy_from_slice(&id.to_le_bytes());
    packet[6..].copy_from_slice(&encode_position(cell, 0));
    packet
}

/// CZ_REQUEST_ACTNPC: the unit, its target and the action.
fn encode_act_npc(id: u32, target: u32, action: u8) -> [u8; 11] {
    let mut packet = [0u8; 11];
    packet[..2].copy_from_slice(&CZ_REQUEST_ACTNPC.to_le_bytes());
    packet[2..6].copy_from_slice(&id.to_le_bytes());
    packet[6..10].copy_from_slice(&target.to_le_bytes());
    packet[10] = action;
    packet
}

fn encode_pet_command(command: u8) -> [u8; 3] {
    let [low, high] = CZ_COMMAND_PET.to_le_bytes();
    [low, high, command]
}

/// CZ_COMMAND_MER: a homunculus command.
fn encode_homunculus_command(command: u8) -> [u8; 5] {
    let [low, high] = CZ_COMMAND_MER.to_le_bytes();
    [low, high, 0, 0, command]
}

/// A rename request: the header and at most 23 bytes of whole characters, NUL-padded.
fn encode_named(header: u16, name: &str) -> [u8; 26] {
    let mut packet = [0u8; 26];
    packet[..2].copy_from_slice(&header.to_le_bytes());
    let bytes = euc_kr_cut(name, 23);
    packet[2..2 + bytes.len()].copy_from_slice(&bytes);
    packet
}

/// CZ_RENAME_PET.
fn encode_rename_pet(name: &str) -> [u8; 26] {
    encode_named(CZ_RENAME_PET, name)
}

/// CZ_RENAME_MER.
fn encode_rename_homunculus(name: &str) -> [u8; 26] {
    encode_named(CZ_RENAME_MER, name)
}

/// Text in EUC-KR, cut to at most `most` bytes of whole characters.
fn euc_kr_cut(text: &str, most: usize) -> Vec<u8> {
    let mut bytes = Vec::new();
    for character in text.chars() {
        let encoded = encoding_rs::EUC_KR.encode(character.encode_utf8(&mut [0; 4])).0.into_owned();
        if bytes.len() + encoded.len() > most {
            break;
        }
        bytes.extend_from_slice(&encoded);
    }
    bytes
}

/// CZ_USE_SKILL_TOGROUND_WITHTALKBOX2: a ground skill with a message (a Talkie Box's): level, skill, cell, then at
/// most 20 bytes of text, NUL-padded to 21.
fn encode_skill_at_with_text(skill: u16, level: u16, cell: [u16; 2], text: &str) -> [u8; 31] {
    let mut packet = [0u8; 31];
    packet[..2].copy_from_slice(&CZ_USE_SKILL_TOGROUND_WITHTALKBOX2.to_le_bytes());
    packet[2..4].copy_from_slice(&level.to_le_bytes());
    packet[4..6].copy_from_slice(&skill.to_le_bytes());
    packet[6..8].copy_from_slice(&cell[0].to_le_bytes());
    packet[8..10].copy_from_slice(&cell[1].to_le_bytes());
    let bytes = euc_kr_cut(text, 20);
    packet[10..10 + bytes.len()].copy_from_slice(&bytes);
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

/// CZ_ITEMLISTWIN_RES: the window's kind as it came, OK (1) or cancel (0), and each item's inventory index and amount.
fn encode_materials(kind: u32, ok: bool, entries: &[(u16, u16)]) -> Vec<u8> {
    let mut packet = CZ_ITEMLISTWIN_RES.to_le_bytes().to_vec();
    packet.extend_from_slice(&((12 + entries.len() * 4) as u16).to_le_bytes());
    packet.extend_from_slice(&kind.to_le_bytes());
    packet.extend_from_slice(&u32::from(ok).to_le_bytes());
    for (index, amount) in entries {
        packet.extend_from_slice(&index.to_le_bytes());
        packet.extend_from_slice(&amount.to_le_bytes());
    }
    packet
}

/// CZ_REQ_ACH_REWARD and CZ_REQ_CHANGE_TITLE: a header and an ID.
/// CZ_INVENTORY_TAB: the index, and whether the item is in the favourites now (rathena moves it the other way).
fn encode_favorite(index: u16, favorite: bool) -> [u8; 5] {
    let [[h0, h1], [i0, i1]] = [CZ_INVENTORY_TAB, index].map(u16::to_le_bytes);
    [h0, h1, i0, i1, u8::from(favorite)]
}

/// CZ_REQ_ALLY_GUILD: the player asked, then our account and character.
fn encode_ally_request(account: u32, me: u32, character: u32) -> Vec<u8> {
    [&CZ_REQ_ALLY_GUILD.to_le_bytes()[..], &account.to_le_bytes(), &me.to_le_bytes(), &character.to_le_bytes()].concat()
}

/// A header and two u32s.
fn encode_pair(header: u16, first: u32, second: u32) -> Vec<u8> {
    [&header.to_le_bytes()[..], &first.to_le_bytes(), &second.to_le_bytes()].concat()
}

/// CZ_REQ_DISORGANIZE_GUILD: the key (the guild's name) in 40 bytes.
fn encode_disband(name: &str) -> Vec<u8> {
    let mut key = encoding_rs::EUC_KR.encode(name).0.into_owned();
    key.resize(40, 0);
    [&CZ_REQ_DISORGANIZE_GUILD.to_le_bytes()[..], &key].concat()
}

/// CZ_CHANGE_DIRECTION: the head's direction (a u16), then the body's.
fn encode_direction(direction: u8, head: u8) -> [u8; 5] {
    let [h0, h1] = CZ_CHANGE_DIRECTION.to_le_bytes();
    [h0, h1, head, 0, direction]
}

fn encode_id(header: u16, id: u32) -> [u8; 6] {
    let mut packet = [0u8; 6];
    packet[..2].copy_from_slice(&header.to_le_bytes());
    packet[2..].copy_from_slice(&id.to_le_bytes());
    packet
}

/// The roulette's requests: open, the prizes, close, spin (just the header), and claim (a byte rathena doesn't read).
fn encode_roulette(request: RouletteRequest) -> Vec<u8> {
    let header = match request {
        RouletteRequest::Open => CZ_REQ_OPEN_ROULETTE,
        RouletteRequest::Prizes => CZ_REQ_ROULETTE_INFO,
        RouletteRequest::Close => CZ_REQ_CLOSE_ROULETTE,
        RouletteRequest::Spin => CZ_REQ_GENERATE_ROULETTE,
        // ponytail: what the official client puts in the byte isn't knowable.
        RouletteRequest::Claim => return [&CZ_RECV_ROULETTE_ITEM.to_le_bytes()[..], &[0]].concat(),
    };
    header.to_le_bytes().to_vec()
}

/// CZ_SETTING_WHISPER_STATE: 0 to block everyone, 1 to allow everyone.
fn encode_whisper_all(allow: bool) -> [u8; 3] {
    let [low, high] = CZ_SETTING_WHISPER_STATE.to_le_bytes();
    [low, high, u8::from(allow)]
}

/// CZ_REQ_OPEN_BUYING_STORE: zeny limit, 1 to open or 0 to cancel, 80-byte EUC-KR store name, then 10-byte (item, amount, price) rows.
fn encode_open_buying_store(title: &str, limit: u32, open: bool, items: &[(u32, u16, u32)]) -> Vec<u8> {
    let mut name = encoding_rs::EUC_KR.encode(title).0.into_owned();
    name.resize(80, 0);
    let mut packet = CZ_REQ_OPEN_BUYING_STORE.to_le_bytes().to_vec();
    packet.extend_from_slice(&((89 + items.len() * 10) as u16).to_le_bytes());
    packet.extend_from_slice(&limit.to_le_bytes());
    packet.push(u8::from(open));
    packet.extend_from_slice(&name);
    for &(item_id, amount, price) in items {
        packet.extend_from_slice(&item_id.to_le_bytes());
        packet.extend_from_slice(&amount.to_le_bytes());
        packet.extend_from_slice(&price.to_le_bytes());
    }
    packet
}

/// CZ_REQ_CLOSE_BUYING_STORE.
fn encode_close_buying_store() -> [u8; 2] {
    CZ_REQ_CLOSE_BUYING_STORE.to_le_bytes()
}

/// CZ_REQ_CLICK_TO_BUYING_STORE: owner's account ID.
fn encode_click_buying_store(owner: u32) -> [u8; 6] {
    let mut packet = [0u8; 6];
    packet[..2].copy_from_slice(&CZ_REQ_CLICK_TO_BUYING_STORE.to_le_bytes());
    packet[2..].copy_from_slice(&owner.to_le_bytes());
    packet
}

/// CZ_REQ_TRADE_BUYING_STORE: owner's account ID, store ID, then 8-byte (inventory index, item ID, amount) rows.
fn encode_sell_to_buying_store(owner: u32, store_id: u32, items: &[(u16, u32, u16)]) -> Vec<u8> {
    let mut packet = CZ_REQ_TRADE_BUYING_STORE.to_le_bytes().to_vec();
    packet.extend_from_slice(&((12 + items.len() * 8) as u16).to_le_bytes());
    packet.extend_from_slice(&owner.to_le_bytes());
    packet.extend_from_slice(&store_id.to_le_bytes());
    for &(index, item_id, amount) in items {
        packet.extend_from_slice(&index.to_le_bytes());
        packet.extend_from_slice(&item_id.to_le_bytes());
        packet.extend_from_slice(&amount.to_le_bytes());
    }
    packet
}

/// CZ_CLAN_CHAT: length, then `"<name> : <message>\0"`.
fn encode_clan_chat(name: &str, message: &str) -> Vec<u8> {
    let text = encoding_rs::EUC_KR.encode(&format!("{name} : {message}")).0.into_owned();
    let mut packet = CZ_CLAN_CHAT.to_le_bytes().to_vec();
    packet.extend_from_slice(&((4 + text.len() + 1) as u16).to_le_bytes());
    packet.extend_from_slice(&text);
    packet.push(0);
    packet
}

/// Packed cell position and direction (3 bytes), as rathena's WBUFPOS.
pub fn encode_position([x, y]: [u16; 2], direction: u8) -> [u8; 3] {
    [(x >> 2) as u8, ((x << 6) | ((y >> 4) & 0x3F)) as u8, ((y << 4) | u16::from(direction & 0x0F)) as u8]
}

/// A server direction (0 north, counter-clockwise: 2 west, 4 south) as a sprite direction (0 facing the camera,
/// clockwise: 2 west, 4 away). The same sum turns a sprite direction back into a server one.
pub fn sprite_direction(direction: u8) -> u8 {
    4u8.wrapping_sub(direction) & 7
}

pub fn decode_position(bytes: &[u8]) -> ([u16; 2], u8) {
    let [a, b, c] = [bytes[0], bytes[1], bytes[2]].map(u16::from);
    ([(a << 2) | (b >> 6), ((b & 0x3F) << 4) | (c >> 4)], (c & 0x0F) as u8)
}

/// A standing unit's cell and its sprite direction.
fn decode_entry_position(bytes: &[u8]) -> ([u16; 2], u8) {
    let (position, direction) = decode_position(bytes);
    (position, sprite_direction(direction))
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
        // bodyState and healthState sit in front of effectState on every entry packet.
        body_state: u16_at(15),
        health_state: u16_at(17),
        option: u32_at(packet, 19),
        // GUID and GEmblemVer follow the robe.
        guild: u32_at(packet, layout.robe + 2),
        emblem: u32::from(u16_at(layout.robe + 6)),
        // maxHP and HP follow the level and the font; -1 when not told.
        hp: Some([u32_at(packet, layout.level + 8) as i32, u32_at(packet, layout.level + 4) as i32]).filter(|[hp, max]| *hp >= 0 && *max > 0),
    }
}

fn decode_buying_items(bytes: &[u8]) -> Vec<BuyingItem> {
    bytes.chunks_exact(11).map(|entry| BuyingItem { price: u32_at(entry, 0), amount: u16_at(entry, 4), kind: entry[6], item_id: u32_at(entry, 7) }).collect()
}

fn decode_map_packet(packet: &[u8]) -> Option<MapEvent> {
    if packet.len() < 2 {
        return None;
    }
    let u32_at = |offset: usize| u32::from_le_bytes(packet[offset..offset + 4].try_into().unwrap());
    let u16_at = |offset: usize| u16::from_le_bytes([packet[offset], packet[offset + 1]]);
    Some(match u16_at(0) {
        ZC_ACCEPT_ENTER => {
            let (position, direction) = decode_position(&packet[6..9]);
            MapEvent::Entered { position, direction: sprite_direction(direction), clock: (u32_at(2), Instant::now()) }
        }
        ZC_NOTIFY_STANDENTRY => MapEvent::Unit(decode_unit(packet, &STAND_LAYOUT, decode_entry_position(&packet[63..66]))),
        ZC_NOTIFY_NEWENTRY => MapEvent::Unit(decode_unit(packet, &NEW_LAYOUT, decode_entry_position(&packet[63..66]))),
        ZC_NOTIFY_MOVEENTRY => {
            let (from, to) = decode_move(&packet[67..73]);
            let sub = [packet[72] >> 4, packet[72] & 0x0F];
            MapEvent::UnitWalk { unit: decode_unit(packet, &MOVE_LAYOUT, (from, 0)), to, start_time: u32_at(37), sub }
        }
        ZC_NOTIFY_TIME => MapEvent::ServerTime(u32_at(2), Instant::now()),
        ZC_NOTIFY_VANISH => MapEvent::Vanish { id: u32_at(2), reason: packet[6] },
        ZC_NOTIFY_PLAYERMOVE => {
            let (from, to) = decode_move(&packet[6..12]);
            MapEvent::PlayerMove { from, to, start_time: u32_at(2) }
        }
        ZC_STOPMOVE => MapEvent::Stop { id: u32_at(2), position: [u16_at(6), u16_at(8)] },
        ZC_HIGHJUMP | ZC_FASTMOVE => MapEvent::Slide { id: u32_at(2), position: [u16_at(6), u16_at(8)] },
        ZC_COMBODELAY => MapEvent::ComboWindow { id: u32_at(2), ms: u32_at(6) },
        ZC_BLADESTOP => MapEvent::BladeStop { source: u32_at(2), target: u32_at(6), on: u32_at(10) != 0 },
        ZC_CHANGE_DIRECTION => MapEvent::Turn { id: u32_at(2), direction: sprite_direction(packet[8]), head_direction: u16_at(6) },
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
            sp: packet[26] != 0,
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
        ZC_DISPEL => MapEvent::CastCancelled(u32_at(2)),
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
                sp: false,
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
                grade: packet[69],
                worn: 0,
                broken: packet[11] != 0,
                favorite: packet[65] != 0,
                bind: u16_at(38),
                expires: u32_at(34),
                switch: 0,
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
                        refine: row[140],
                    }
                })
                .collect(),
        },
        ZC_SEARCH_STORE_INFO_FAILED => MapEvent::CatalogFailed(packet[2]),
        ZC_SSILIST_ITEM_CLICK_ACK => {
            let cell = [u16_at(2) as i16, u16_at(4) as i16];
            MapEvent::CatalogMark((cell != [-1, -1]).then_some(cell))
        }
        ZC_STARSKILL => MapEvent::StarSkill { name: c_string(&packet[2..26]), monster: u32_at(26), count: packet[30], kind: packet[31] },
        ZC_NOTIFY_RANKING => MapEvent::PvpRank { account: u32_at(2), rank: u32_at(6), players: u32_at(10) },
        ZC_ACK_PVPPOINT => MapEvent::PvpInfo { wins: u32_at(10), losses: u32_at(14), points: u32_at(18) as i32 },
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
        // Index, broken, four cards, five random options (unused), refine, grade.
        ZC_CHANGE_ITEM_OPTION => MapEvent::ItemPreview {
            index: u16_at(2),
            broken: packet[4] != 0,
            cards: [u32_at(5), u32_at(9), u32_at(13), u32_at(17)],
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
        // rathena sends one entry a packet: ID, permissions, rank, tax, title.
        ZC_ACK_CHANGE_GUILD_POSITIONINFO if packet.len() >= 44 => {
            MapEvent::GuildPositionChanged(GuildPosition { id: u32_at(4), mode: u32_at(8), tax: u32_at(16) }, c_string(&packet[20..44]))
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
        // clif_changeoption_target: AID, bodyState, healthState, effectState, PK mode.
        ZC_STATE_CHANGE => MapEvent::OptionChange { id: u32_at(2), body: u16_at(6), health: u16_at(8), option: u32_at(10) },
        ZC_REQ_WEAR_EQUIP_ACK => MapEvent::Equipped { index: u16_at(2), location: u32_at(4), result: packet[10] },
        // Since 2011 the flag is inverted: 0 means it came off.
        ZC_REQ_TAKEOFF_EQUIP_ACK => MapEvent::Unequipped { index: u16_at(2), location: u32_at(4), failed: packet[8] != 0 },
        ZC_ATTACK_FAILURE_FOR_DISTANCE => {
            MapEvent::TooFar { target: u32_at(2), target_cell: [u16_at(6), u16_at(8)], cell: [u16_at(10), u16_at(12)], range: u16_at(14) }
        }
        ZC_STYLE_CHANGE_RES => MapEvent::StyleChanged(packet[2] != 0),
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

        // A knight on a Peco: effectstate is the option.
        info[36..40].copy_from_slice(&0x20u32.to_le_bytes());
        assert_eq!(decode_character(&info).option, 0x20);
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
        stand[15..17].copy_from_slice(&2u16.to_le_bytes());
        stand[17..19].copy_from_slice(&2u16.to_le_bytes());
        stand[49..53].copy_from_slice(&7u32.to_le_bytes());
        stand[53..55].copy_from_slice(&3u16.to_le_bytes());
        let Some(MapEvent::Unit(unit)) = decode_map_packet(&stand) else { panic!() };
        assert_eq!((unit.guild, unit.emblem), (7, 3));
        assert_eq!((unit.body_state, unit.health_state), (2, 2));
        assert_eq!(unit.hp, None);

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
        assert!(matches!(
            decode_map_packet(&[0xEB, 0x01, 1, 0, 0, 0, 156, 0, 180, 0]),
            Some(MapEvent::GuildMemberPlace { account: 1, cell: Some([156, 180]) })
        ));
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
        assert!(matches!(
            decode_map_packet(&[&ZC_ACK_RANDOM_UPGRADE_ITEM.to_le_bytes()[..], &1u16.to_le_bytes()].concat()),
            Some(MapEvent::LaphineUpgraded(1))
        ));
        let upgrade_request: [u8; 8] =
            [&CZ_REQ_RANDOM_UPGRADE_ITEM.to_le_bytes()[..], &100_626u32.to_le_bytes(), &7u16.to_le_bytes()].concat().try_into().unwrap();
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
        // clif_PVPInfo: pvp_point is an int16 that can go below 0.
        let info = [
            &ZC_ACK_PVPPOINT.to_le_bytes()[..],
            &150_000u32.to_le_bytes(),
            &2_000_000u32.to_le_bytes(),
            &0u32.to_le_bytes(),
            &1u32.to_le_bytes(),
            &(-3i32).to_le_bytes(),
        ]
        .concat();
        assert!(matches!(decode_map_packet(&info), Some(MapEvent::PvpInfo { wins: 0, losses: 1, points: -3 })));
        assert_eq!(encode_pvp_info(150_000, 2_000_000), [0x0F, 0x02, 0xF0, 0x49, 0x02, 0, 0x80, 0x84, 0x1E, 0]);
        for (header, length) in [(ZC_NOTIFY_MAPPROPERTY2, 4), (ZC_NOTIFY_RANKING, 14), (CZ_REQ_PVPPOINT, 10), (ZC_ACK_PVPPOINT, 22)] {
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
                info.extend_from_slice(
                    &[&row.to_le_bytes()[..], &column.to_le_bytes(), &item.to_le_bytes(), &1u16.to_le_bytes(), &0u16.to_le_bytes()].concat(),
                );
            }
        }
        let Some(MapEvent::RoulettePrizes(prizes)) = decode_map_packet(&info) else { panic!() };
        assert_eq!(prizes.len(), 42);
        assert_eq!(prizes[0], RoulettePrize { row: 0, column: 0, item: 500, amount: 1 });
        assert_eq!((prizes[41].row, prizes[41].column, prizes[41].item), (6, 2, 562));

        let spun = |result: u8| {
            [&ZC_ACK_GENERATE_ROULETTE.to_le_bytes()[..], &[result], &0u16.to_le_bytes(), &4u16.to_le_bytes(), &5u32.to_le_bytes(), &points].concat()
        };
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
        for header in [ZC_HIGHJUMP, ZC_FASTMOVE] {
            let slide = [&header.to_le_bytes()[..], &2000010u32.to_le_bytes(), &170u16.to_le_bytes(), &370u16.to_le_bytes()].concat();
            let Some(MapEvent::Slide { id: 2000010, position: [170, 370] }) = decode_map_packet(&slide) else { panic!() };
        }
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
        let ids = [
            ZC_MYGUILD_BASIC_INFO,
            ZC_REQ_ALLY_GUILD,
            ZC_ACK_REQ_ALLY_GUILD,
            ZC_ACK_REQ_HOSTILE_GUILD,
            ZC_DELETE_RELATED_GUILD,
            ZC_ACK_DISORGANIZE_GUILD_RESULT,
        ];
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
                [&ZC_BOSS_INFO.to_le_bytes()[..], &[kind], &120u32.to_le_bytes(), &80u32.to_le_bytes(), &1u16.to_le_bytes(), &5u16.to_le_bytes(), &[0; 4]]
                    .concat();
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
        assert_eq!((position, title.as_str()), (GuildPosition { id: 1, mode: 0x11, tax: 10 }, "Officer"));

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
        let request = encode_position_change(&GuildPosition { id: 1, mode: 0x01, tax: 10 }, "Officer");
        assert_eq!(request.len(), 44);
        assert_eq!(&request[..20], &[0x61, 0x01, 44, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 10, 0, 0, 0]);
        assert_eq!(&request[20..], &name_field("Officer")[..]);
        assert_eq!(encode_member_move(7, 8, 19), [0x55, 0x01, 16, 0, 7, 0, 0, 0, 8, 0, 0, 0, 19, 0, 0, 0]);
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
        assert!(
            matches!(decode_map_packet(&named(ZC_ACK_ENTRY_QUEUE_APPLY, Some(1), kvm, &[])), Some(MapEvent::BattleQueue { result: 1, name }) if name == kvm)
        );
        assert!(matches!(
            decode_map_packet(&named(ZC_NOTIFY_ENTRY_QUEUE_APPLY, None, kvm, &3u32.to_le_bytes())),
            Some(MapEvent::QueueNumber { name, number: 3 }) if name == kvm
        ));
        assert!(matches!(decode_map_packet(&named(ZC_ACK_ENTRY_QUEUE_CANCEL, Some(1), kvm, &[])), Some(MapEvent::QueueLeft { result: 1, .. })));
        assert!(
            matches!(decode_map_packet(&named(ZC_NOTIFY_LOBBY_ADMISSION, None, kvm, &name_field(kvm))), Some(MapEvent::QueueReady { name }) if name == kvm)
        );
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
        assert!(matches!(
            decode_map_packet(&position("LaneA2", 4001, [62, 119])),
            Some(MapEvent::BattlePosition { account: 2_000_001, cell: Some([62, 119]) })
        ));
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

        for (header, length) in
            [(CZ_REQ_OPEN_BUYING_STORE, -1), (CZ_REQ_CLOSE_BUYING_STORE, 2), (CZ_REQ_CLICK_TO_BUYING_STORE, 6), (CZ_REQ_TRADE_BUYING_STORE, -1)]
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

    #[test]
    fn encodes_standing_resurrection() {
        // rathena parseable_packet(0x0292, 2, clif_parse_AutoRevive, 0): header only.
        assert_eq!(encode_standing_resurrection(), [0x92, 0x02]);
        assert_eq!(packet_length(0x0292), Some(2));
    }
}
