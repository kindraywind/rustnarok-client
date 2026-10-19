//! Login, char and map server protocol, as rathena speaks it for PACKETVER 20220406 (see rathena src/common/packets.hpp).

use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, TcpStream};
use std::sync::{LazyLock, RwLock};
use std::time::{Duration, Instant};

mod decode;
mod map_event;
mod map_session;

use decode::decode_map_packet;
pub use map_event::MapEvent;

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
    /// The web server's key for this login (rathena's web auth token).
    pub web_token: String,
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
/// `check_client_version` is on). A reply too short to read fails the login instead of panicking, which would leave the
/// login window waiting forever.
pub fn login(address: &str, version: u32, username: &str, password: &str) -> Result<LoginAccepted, LoginError> {
    std::panic::catch_unwind(|| request_login(address, version, username, password))
        .unwrap_or_else(|_| Err(io::Error::new(io::ErrorKind::InvalidData, "unreadable login reply").into()))
}

fn request_login(address: &str, version: u32, username: &str, password: &str) -> Result<LoginAccepted, LoginError> {
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
            web_token: c_string(&packet[47..ACCEPT_HEADER]),
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

/// A name in a 24-byte field.
fn name_field(name: &str) -> Vec<u8> {
    let mut field = encode_text(name);
    field.resize(24, 0);
    field
}

/// CZ_REG_CHANGE_GUILD_POSITIONINFO with one position: ID, permissions, rank (rathena ignores it; the ID), tax, title.
/// A room's settings as CZ_CREATE_CHATROOM and CZ_CHANGE_CHATROOM carry them: the length, the limit, public (1) or
/// private (0), the 8-byte password, the title.
pub fn encode_room(packet_type: u16, title: &str, limit: u16, public: bool, password: &str) -> Vec<u8> {
    let title = encode_text(title);
    let mut secret = password.as_bytes().to_vec();
    secret.resize(8, 0);
    let mut packet = packet_type.to_le_bytes().to_vec();
    packet.extend_from_slice(&((15 + title.len()) as u16).to_le_bytes());
    packet.extend_from_slice(&limit.to_le_bytes());
    packet.push(u8::from(public));
    packet.extend_from_slice(&secret);
    packet.extend_from_slice(&title);
    packet
}

/// CZ_ADVANCED_STATUS_CHANGE: the trait (status id) and how much, one.
pub fn encode_trait_raise(trait_id: u16) -> Vec<u8> {
    let mut packet = CZ_ADVANCED_STATUS_CHANGE.to_le_bytes().to_vec();
    packet.extend_from_slice(&trait_id.to_le_bytes());
    packet.extend_from_slice(&1u16.to_le_bytes());
    packet
}

/// CZ_SHORTCUTKEYBAR_ROTATE2: a hotkey bar (the server's tab) and the row it draws first, for the server to keep.
pub fn encode_hotkey_rotation(tab: u16, rowshift: u8) -> Vec<u8> {
    let mut packet = CZ_SHORTCUTKEYBAR_ROTATE2.to_le_bytes().to_vec();
    packet.extend_from_slice(&tab.to_le_bytes());
    packet.push(rowshift);
    packet
}

/// A name in 24 bytes, as the room packets carry it.
fn name_24(name: &str) -> Vec<u8> {
    let mut name = encode_text(name);
    name.resize(24, 0);
    name
}

/// The owner hands the room to `name` (CZ_REQ_ROLE_CHANGE: role 0, the name).
pub fn encode_room_owner(name: &str) -> Vec<u8> {
    let mut packet = CZ_REQ_ROLE_CHANGE.to_le_bytes().to_vec();
    packet.extend_from_slice(&0u32.to_le_bytes());
    packet.extend(name_24(name));
    packet
}

/// The owner kicks `name` out (CZ_REQ_EXPEL_MEMBER).
pub fn encode_room_kick(name: &str) -> Vec<u8> {
    let mut packet = CZ_REQ_EXPEL_MEMBER.to_le_bytes().to_vec();
    packet.extend(name_24(name));
    packet
}

pub fn encode_position_change(position: &GuildPosition, title: &str) -> Vec<u8> {
    let mut packet = CZ_REG_CHANGE_GUILD_POSITIONINFO.to_le_bytes().to_vec();
    packet.extend_from_slice(&44u16.to_le_bytes());
    for value in [position.id, position.mode, position.id, position.tax] {
        packet.extend_from_slice(&value.to_le_bytes());
    }
    packet.extend_from_slice(&name_field(title));
    packet
}

/// CZ_REQ_CHANGE_MEMBERPOS with every move (account, character, position): 4 + 12 bytes a member.
pub fn encode_member_moves(moves: &[(u32, u32, u32)]) -> Vec<u8> {
    let mut packet = CZ_REQ_CHANGE_MEMBERPOS.to_le_bytes().to_vec();
    packet.extend_from_slice(&((4 + 12 * moves.len()) as u16).to_le_bytes());
    for &(account, character, position) in moves {
        for value in [account, character, position] {
            packet.extend_from_slice(&value.to_le_bytes());
        }
    }
    packet
}

/// A guild's member and a reason, as CZ_REQ_LEAVE_GUILD and CZ_REQ_BAN_GUILD carry them: guild, account,
/// character, 40 bytes of reason.
pub fn encode_guild_reason(header: u16, guild: u32, account: u32, character: u32, reason: &str) -> Vec<u8> {
    let mut packet = header.to_le_bytes().to_vec();
    for value in [guild, account, character] {
        packet.extend_from_slice(&value.to_le_bytes());
    }
    let mut why = encode_text(reason);
    why.truncate(39);
    why.resize(40, 0);
    packet.extend_from_slice(&why);
    packet
}

/// A battleground's name as rathena keeps it (`name.resize(NAME_LENGTH)`): 23 bytes and a NUL in 24.
fn queue_name_field(name: &str) -> Vec<u8> {
    let mut field = encode_text(name);
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

/// CZ_MEMORIALDUNGEON_COMMAND: a u32 command. Destroy is `COMMAND_MEMORIALDUNGEON_DESTROY_FORCE`.
pub fn encode_memorial_command(command: u32) -> [u8; 6] {
    let mut packet = [0u8; 6];
    packet[..2].copy_from_slice(&CZ_MEMORIALDUNGEON_COMMAND.to_le_bytes());
    packet[2..6].copy_from_slice(&command.to_le_bytes());
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
    let text = encode_text(&format!("{name} : {message}"));
    let mut packet = CZ_BATTLEFIELD_CHAT.to_le_bytes().to_vec();
    packet.extend_from_slice(&((4 + text.len() + 1) as u16).to_le_bytes());
    packet.extend_from_slice(&text);
    packet.push(0);
    packet
}

// ponytail: one code page for the whole process, as the client talks to one server at a time; a field of each
// session if it ever talks to two.
/// The code page of text on the wire: EUC-KR until the connection's langtype picks another.
static CODEPAGE: RwLock<&'static encoding_rs::Encoding> = RwLock::new(encoding_rs::EUC_KR);

pub fn set_codepage(codepage: &'static encoding_rs::Encoding) {
    *CODEPAGE.write().unwrap() = codepage;
}

pub fn codepage() -> &'static encoding_rs::Encoding {
    *CODEPAGE.read().unwrap()
}

/// Text in the code page.
pub fn encode_text(text: &str) -> Vec<u8> {
    encode_in(codepage(), text)
}

/// Text in `codepage`; a letter it has no bytes for becomes `?`, as Windows writes it.
fn encode_in(codepage: &'static encoding_rs::Encoding, text: &str) -> Vec<u8> {
    text.chars().flat_map(|character| letter(codepage, character)).collect()
}

fn letter(codepage: &'static encoding_rs::Encoding, character: char) -> Vec<u8> {
    match codepage.encode(character.encode_utf8(&mut [0; 4])) {
        (bytes, _, false) => bytes.into_owned(),
        _ => vec![b'?'],
    }
}

/// Decodes a NUL-terminated field in the code page.
pub fn c_string(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&byte| byte == 0).unwrap_or(bytes.len());
    codepage().decode_without_bom_handling(&bytes[..end]).0.into_owned()
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
    /// Milliseconds a cell.
    pub speed: u16,
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
                    if let Some(event) = decode_or_skip(&read_packet(&mut reader)?, decode_char_packet) {
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
        let encoded = encode_text(name);
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

/// CH_REQ_CHANGE_CHARNAME: the character and its new name (24 bytes, NUL-terminated).
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

/// Decodes a packet; one the decoder can't read (shorter than its fields) is skipped. A panic would end the reader
/// thread and leave the game running with nothing more from the server. `read_packet` has taken the whole packet off
/// the stream, so the next one still starts in the right place.
fn decode_or_skip<T>(packet: &[u8], decode: fn(&[u8]) -> Option<T>) -> Option<T> {
    std::panic::catch_unwind(|| decode(packet)).unwrap_or_else(|_| {
        eprintln!("skipped an unreadable packet {:#06x} ({} bytes)", u16_at(packet, 0), packet.len());
        None
    })
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
        speed: i16_at(82),
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
const ZC_PROGRESS: u16 = 0x02F0;
const CZ_PROGRESS: u16 = 0x02F1;
const ZC_PROGRESS_CANCEL: u16 = 0x02F2;
const ZC_PROGRESS_ACTOR: u16 = 0x09D1;
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
const ZC_NPCSPRITE_CHANGE: u16 = 0x01B0;
const ZC_STATE_CHANGE: u16 = 0x0229;
const CZ_REQ_WEAR_EQUIP: u16 = 0x0998;
const ZC_REQ_WEAR_EQUIP_ACK: u16 = 0x0999;
const CZ_REQ_TAKEOFF_EQUIP: u16 = 0x00AB;
const ZC_SHORTCUT_KEY_LIST: u16 = 0x0B20;
const CZ_SHORTCUT_KEY_CHANGE: u16 = 0x0B21;
const CZ_SHORTCUTKEYBAR_ROTATE2: u16 = 0x0B22;
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
/// The server starts a navigation (rathena's clif_navigateTo).
const ZC_NAVIGATION_ACTIVE: u16 = 0x08E2;
/// Reputation points by reputation type, and the server opening the reputation window (rathena's clif_reputation_*).
const ZC_REPUTE_INFO: u16 = 0x0B8D;
const ZC_REPUTE_OPEN: u16 = 0x0B8E;
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
/// ZC instance create / wait / status / notify, and CZ memorial dungeon command.
const ZC_INSTANCE_CREATE: u16 = 0x02CB;
const ZC_INSTANCE_WAIT: u16 = 0x02CC;
const ZC_INSTANCE_STATUS: u16 = 0x02CD;
const ZC_INSTANCE_NOTIFY: u16 = 0x02CE;
const CZ_MEMORIALDUNGEON_COMMAND: u16 = 0x02CF;
/// rathena INSTANCE_NAME_LENGTH: 60 bytes plus the terminating NUL.
pub const INSTANCE_NAME_LENGTH: usize = 61;
/// CZ_MEMORIALDUNGEON_COMMAND: destroy the instance. Sent only after the player confirms.
pub const COMMAND_MEMORIALDUNGEON_DESTROY_FORCE: u32 = 3;
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
const ZC_QUEST_NOTIFY_EFFECT: u16 = 0x0446;
const ZC_SEARCH_STORE_INFO_ACK: u16 = 0x0B64;
/// One result of ZC_SEARCH_STORE_INFO_ACK.
const CATALOG_ROW_SIZE: usize = 142;
/// ZC_STARSKILL's kind for the Taekwon Mission's target.
pub const STAR_MISSION: u8 = 20;
const ZC_NOTIFY_RANKING: u16 = 0x019A;
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
const ZC_EL_INIT: u16 = 0x081D;
const ZC_EL_PAR_CHANGE: u16 = 0x081E;
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
/// CZ_CONFIG's setting for the Show Equipment box (rathena's CONFIG_OPEN_EQUIPMENT_WINDOW), and for hiding costumes
/// (CONFIG_DISABLE_SHOWCOSTUMES: non-zero hides them) with the value the View Costume box sends to hide them.
const CONFIG_OPEN_EQUIPMENT: u32 = 0;
const CONFIG_HIDE_COSTUMES: u32 = 5;
const HIDE_COSTUMES: u32 = 0x3C00;
const CZ_UNINSTALLATION: u16 = 0x0B35;
/// CZ_CONFIG's setting for a pet eating by itself (rathena's CONFIG_PET_AUTOFEED).
const CONFIG_PET_AUTOFEED: u32 = 2;
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
const CZ_REQ_BAN_GUILD: u16 = 0x015B;
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
/// Puts trait points into a 4th job's trait (rathena's clif_parse_advanced_status_change).
const CZ_ADVANCED_STATUS_CHANGE: u16 = 0x0B24;
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
/// The owner changes the room, hands it over, or kicks a member (rathena's clif_parse_ChatRoomStatusChange,
/// clif_parse_ChangeChatOwner, clif_parse_KickFromChat).
const CZ_CHANGE_CHATROOM: u16 = 0x00DE;
const CZ_REQ_ROLE_CHANGE: u16 = 0x00E0;
const CZ_REQ_EXPEL_MEMBER: u16 = 0x00E2;
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
/// Party booking (rathena's web server keeps the posts): asking a post's leader to join, the leader's question and
/// answer, and the answer to the asker.
const CZ_PARTY_REQ_MASTER_TO_JOIN: u16 = 0x0AE6;
const ZC_PARTY_REQ_MASTER_TO_JOIN: u16 = 0x0AE7;
const CZ_PARTY_REQ_ACK_MASTER_TO_JOIN: u16 = 0x0AF8;
const ZC_PARTY_JOIN_REQ_ACK_FROM_MASTER: u16 = 0x0AFA;
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
/// Milliseconds a cell (rathena's SP_SPEED).
pub const STATUS_SPEED: u16 = 0;
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
/// A 4th job's AP and its maximum.
pub const STATUS_AP: u16 = 232;
pub const STATUS_MAX_AP: u16 = 233;
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
#[derive(Debug, Clone, PartialEq, Default)]
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
    /// A stand entry's state byte (1 dead, 2 sitting; it counts for players only); none on other entries.
    pub posture: Option<u8>,
    /// Spawned in view (ZC_NOTIFY_NEWENTRY) rather than already there or walking in.
    pub spawned: bool,
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

/// What a pet does (ZC_PET_ACT, CZ_PET_ACT; rathena passes the number on as it came, so every client around reads the
/// same): an emotion (ZC_EMOTION's numbers) is emotion × 10 + 2; a line of its talk table is monster × 1000 + act × 100
/// + hunger band × 10.
#[derive(Debug, PartialEq)]
pub enum PetAct {
    Emotion(u8),
    Talk { monster: u16, act: u8, hunger: u8 },
}

impl PetAct {
    pub fn decode(value: i32) -> Self {
        if value % 10 == 2 {
            return Self::Emotion((value / 10).clamp(0, 255) as u8);
        }
        Self::Talk { monster: (value / 1000).max(0) as u16, act: (value / 100 % 10).max(0) as u8, hunger: (value / 10 % 10).max(0) as u8 }
    }

    pub fn value(&self) -> i32 {
        match *self {
            Self::Emotion(kind) => i32::from(kind) * 10 + 2,
            Self::Talk { monster, act, hunger } => i32::from(monster) * 1000 + i32::from(act) * 100 + i32::from(hunger) * 10,
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
    /// Their hair style and colour, and sex (0 female, 1 male), for their head in the member list.
    pub hair: u16,
    pub hair_color: u16,
    pub sex: u16,
    pub job: u16,
    pub level: u16,
    pub contribution: u32,
    pub online: bool,
    pub position: u32,
    /// When they last logged in (Unix time).
    pub last_login: u32,
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
    /// In the favourites (rathena's flag; listed on the favourites tab).
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

impl Drop for MapSession {
    fn drop(&mut self) {
        let _ = self.stream.0.shutdown(std::net::Shutdown::Both);
    }
}

/// CZ_SETTING_WHISPER_PC: the name (NUL-terminated, at most 23 bytes) and 0 to block, 1 to allow.
fn encode_whisper_setting(name: &str, allow: bool) -> Vec<u8> {
    let mut packet = CZ_SETTING_WHISPER_PC.to_le_bytes().to_vec();
    let mut field = [0u8; 24];
    let encoded = encode_text(name);
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
    encode_config_value(setting, u32::from(on))
}

fn encode_config_value(setting: u32, value: u32) -> [u8; 10] {
    let mut packet = [0u8; 10];
    packet[..2].copy_from_slice(&CZ_CONFIG.to_le_bytes());
    packet[2..6].copy_from_slice(&setting.to_le_bytes());
    packet[6..].copy_from_slice(&value.to_le_bytes());
    packet
}

/// CZ_UNINSTALLATION: what to take off.
fn encode_uninstall(kind: u8) -> [u8; 3] {
    let [low, high] = CZ_UNINSTALLATION.to_le_bytes();
    [low, high, kind]
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
    let bytes = text_cut(name, 23);
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

/// Text in the code page, cut to at most `most` bytes of whole characters.
fn text_cut(text: &str, most: usize) -> Vec<u8> {
    let codepage = codepage();
    let mut bytes = Vec::new();
    for character in text.chars() {
        let encoded = letter(codepage, character);
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
    let bytes = text_cut(text, 20);
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
    let mut key = encode_text(name);
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

/// CZ_REQ_OPEN_BUYING_STORE: zeny limit, 1 to open or 0 to cancel, 80-byte store name, then 10-byte (item, amount, price) rows.
fn encode_open_buying_store(title: &str, limit: u32, open: bool, items: &[(u32, u16, u32)]) -> Vec<u8> {
    let mut name = encode_text(title);
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
    let text = encode_text(&format!("{name} : {message}"));
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
        posture: layout.state.and_then(|state| packet.get(state).copied()),
        spawned: false,
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

#[cfg(test)]
mod tests;
