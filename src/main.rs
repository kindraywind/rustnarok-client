//! Clean-room native Ragnarok Online client. Assets come from the user's own data.grf at runtime.
//!
//! README.md says what it does so far and how to set it up.
//!
//! Dev switches: `RUSTNAROK_CLIENT_GRF` (data.grf path), `RUSTNAROK_CLIENT_SERVER` (login server, over the data folder's
//! clientinfo.xml and the default 127.0.0.1:6900),
//! `RUSTNAROK_CLIENT_LOGIN=user:password` (fill in and submit on start), `RUSTNAROK_CLIENT_AUTOSTART=<slot>` (then pick the first
//! server and play that slot; any other value stops at character select), `RUSTNAROK_CLIENT_PIN=<4 digits>` (enter that PIN when
//! the char server asks), `RUSTNAROK_CLIENT_WALK=x,y` (walk there a second after entering the map),
//! `RUSTNAROK_CLIENT_TALK=<name>[:<n>]` (talk to that NPC, pressing "next" or "close" up to n times), `RUSTNAROK_CLIENT_ATTACK=1` (attack the
//! nearest monster in view), `RUSTNAROK_CLIENT_SAY=<text>[|<text>...]` (type each into the chat a second after entering, e.g.
//! GM commands or /organize; `RUSTNAROK_CLIENT_SAY_AT` seconds in instead),
//! `RUSTNAROK_CLIENT_PICKUP=1` (pick up the nearest item once one lies around), `RUSTNAROK_CLIENT_INVENTORY=1` (open the inventory),
//! `RUSTNAROK_CLIENT_CURSOR=x,y` (where the pointer starts, to see hover labels in screenshots),
//! `RUSTNAROK_CLIENT_CAMERA=yaw,pitch,zoom` (the camera turned, in degrees and zoom units, on every map),
//! `RUSTNAROK_CLIENT_WEAR=1` (put on every piece of equipment carried, once the inventory is in), `RUSTNAROK_CLIENT_EQUIPMENT=1` (open
//! the equipment window), `RUSTNAROK_CLIENT_SHOT_LOADING=1` (screenshot the loading screen instead of the game),
//! `RUSTNAROK_CLIENT_WINDOW=<width>x<height>` (the window's size at start, e.g. 1365x768 for 16:9),
//! `RUSTNAROK_CLIENT_SKILLS=1` (open the skill window), `RUSTNAROK_CLIENT_SKILL=<id>:<level>` (use a skill 3 seconds in, on oneself
//! if it needs a target), `RUSTNAROK_CLIENT_HOTKEYS=<slot>[,<slot>...]` (press those hotkeys, one a second from 3 seconds in),
//! `RUSTNAROK_CLIENT_DEAL=buy|sell[:<row>:<amount>]` (answer a shop's question at once, and make that deal),
//! `RUSTNAROK_CLIENT_MENU=<n>[,<n>...]` (pick those NPC menu items, counting from 1, as menus come up),
//! `RUSTNAROK_CLIENT_STORE=put|take:<item>:<amount>` (once storage opens, move that much of an item in or out),
//! `RUSTNAROK_CLIENT_TRADE=ask:<name>|accept` (ask that player for a deal, or take the first one asked),
//! `RUSTNAROK_CLIENT_TRADE_GIVE=<item>:<amount>[,...][,z<zeny>]` (once a deal opens, put those in, lock and exchange),
//! `RUSTNAROK_CLIENT_ROOM=window|make:<title>` (open the chat room maker with that title, or make the room),
//! `RUSTNAROK_CLIENT_KEEP_WINDOWS=1` (under the test driver, keep the basic info and chat windows between runs as a player's are),
//! `RUSTNAROK_CLIENT_STATUS=1` (open the status window), `RUSTNAROK_CLIENT_CART=1` (open the cart),
//! `RUSTNAROK_CLIENT_VEND=<name>:<price>` (when the vending skill goes off, sell the whole cart at that price),
//! `RUSTNAROK_CLIENT_FRIEND=accept` (befriend whoever asks), `RUSTNAROK_CLIENT_GUILD=accept,window` (join the first guild that invites, open the guild window),
//! `RUSTNAROK_CLIENT_PARTY=accept,window,friends` (join the first party that invites, open the party window),
//! `RUSTNAROK_CLIENT_WHISPER=<name>|<text>[;...]` (whisper those a second in, through the chat's name box),
//! `RUSTNAROK_CLIENT_CONTROL=<socket>` (take commands from a test driver on that Unix socket; see control.rs),
//! `RUSTNAROK_CLIENT_CLICKS=[r]x,y[>x,y][;...]` (click, or drag, one a second from `RUSTNAROK_CLIENT_CLICKS_AT` seconds in, 4 by
//! default; `r` for the right button),
//! `RUSTNAROK_CLIENT_MAKE=<slot>[:<name>]` (open character creation on that slot; with a name, make it),
//! `RUSTNAROK_CLIENT_DELETE=<slot>:<reserve|cancel|ask|prompt|birthdate>` (reserve, cancel, show the questions of, or complete
//! that slot's deletion; `-` is an empty birthdate),
//! `RUSTNAROK_CLIENT_DATA=<folder>` (where the loose `data` and `System` folders that go before data.grf are; `.` by default,
//! `none` for the archive alone),
//! `RUSTNAROK_CLIENT_SCREENSHOT=out.png` (save a settled frame and quit; in the game after `RUSTNAROK_CLIENT_SHOT_DELAY` seconds, 2 by default).

// The control socket's game state is one json! literal deeper than the macro's default limit.
#![recursion_limit = "256"]

mod achievement;
mod actor;
mod announce;
mod bank;
mod basic_info;
mod battle_mode;
mod battle_queue;
mod battleground;
mod buying;
mod cashshop;
mod cast_square;
mod catalog;
mod char_make;
mod char_select;
mod chat;
mod chatroom;
mod clan;
mod control;
mod cursor;
mod dressroom;
mod effects;
mod emotion;
mod emotion_list;
mod enchant;
mod equipment;
mod escape;
mod grade;
mod guild;
mod head_marks;
mod homunculus;
mod hotkeys;
mod inventory;
mod iteminfo;
mod laphine;
mod level_up;
mod links;
mod macros;
mod mail;
mod material;
mod menu;
mod mercenary;
mod minimap;
mod minimap_window;
mod monster_info;
mod navigation;
mod options;
mod overhead;
mod party;
mod pet;
mod pick_list;
mod picking;
mod pickup;
mod pin;
mod quest;
mod quest_notice;
mod quest_tracker;
mod ranking;
mod refine;
mod reform;
mod render;
mod reputation;
mod roulette;
mod screenshot;
mod shop;
mod shortcuts;
mod skill_descript;
mod skill_motions;
mod skills;
mod sound;
mod status;
mod status_tip;
mod storage;
mod stylist;
mod trade;
mod vend;
mod versus;
mod weather;
mod whisper;
mod window;
mod world;
mod worldmap;

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use actor::{Actor, Digits, Look, SpriteSet, Timing};
use basic_info::BasicInfo;
use equipment::Equipment;
use image::{Rgba, RgbaImage};
use inventory::Inventory;
use render::{Gpu, Sprite, Texture};
use rustnarok_client::grf::Grf;
use rustnarok_client::lua::{self, ItemInfo, LookTables, SkillInfo, SkillTrees};
use rustnarok_client::messages;
use rustnarok_client::net::{self, CharEvent, CharSession, Character, LoginAccepted, LoginError, MapEvent, MapSession, MapTicket, NAME_LENGTH};
use rustnarok_client::text;
use rustnarok_client::text::Font;
use window::TitleHit;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, Ime, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, KeyCode, ModifiersState, NamedKey, PhysicalKey};
use winit::window::{Window, WindowId};
use world::{Billboard, Camera, DepthLayer, World, WorldFrame};

/// The canvas the client lays out and renders in, in logical pixels (Retina scaling happens in the projection): 768
/// high and as wide as the window's shape, at least the classic 1024, so a wide window shows more instead of black bars.
pub fn screen() -> [f32; 2] {
    [f32::from_bits(SCREEN_WIDTH.load(Ordering::Relaxed)), 768.0]
}

// ponytail: one global for the one window, read in over a hundred places that would otherwise all take it as an argument.
static SCREEN_WIDTH: AtomicU32 = AtomicU32::new(1024f32.to_bits());

/// The canvas width for a window of `size` pixels.
fn canvas_width([width, height]: [u32; 2]) -> f32 {
    (768.0 * width as f32 / height.max(1) as f32).round().max(1024.0)
}

/// Where a full-screen 4:3 picture (the login background, a loading screen) goes: in its own shape, centered.
fn picture_place() -> ([f32; 2], [f32; 2]) {
    let [width, height] = screen();
    ([((width - 1024.0) / 2.0).floor(), 0.0], [1024.0, height])
}

/// Fits the canvas to a window of `size` pixels.
pub fn fit_screen(size: [u32; 2]) {
    SCREEN_WIDTH.store(canvas_width(size).to_bits(), Ordering::Relaxed);
}

const UI: &str = "data\\texture\\유저인터페이스\\";
const DEFAULT_SERVER: &str = "127.0.0.1:6900";
const CARET_BLINK: Duration = Duration::from_millis(500);
const TEXT_COLOR: [u8; 3] = [0, 0, 0];
const FRAME_TIME: Duration = Duration::from_millis(16);
/// The client pings the map server so idle connections aren't dropped.
const KEEPALIVE: Duration = Duration::from_secs(10);
/// Chat colors: others, our own, item messages, errors.
const CHAT_WHITE: [u8; 3] = [255, 255, 255];
const CHAT_SELF: [u8; 3] = [0x00, 0xFF, 0x00];
const CHAT_ITEM: [u8; 3] = [0x00, 0xFF, 0xFF];
const CHAT_ERROR: [u8; 3] = [0xFF, 0x00, 0x00];
// ponytail: the two announcement shades are guesses; calibrate them against the real client.
const CHAT_BROADCAST: [u8; 3] = [0xFF, 0xFF, 0x00];
/// Messages that come with a skill's name, "[Name] message" (rathena's notes give 0xCDCDFF).
const CHAT_SKILL_MESSAGE: [u8; 3] = [0xCD, 0xCD, 0xFF];
/// Out of ammunition, too heavy, ammunition equipped (ZC_ACTION_FAILURE).
const ACTION_FAILURES: [usize; 4] = [242, 243, 244, 245];
/// Saving a memo point: saved, Warp Portal's level too low, not learned.
const MEMO_RESULTS: [usize; 3] = [217, 214, 216];
/// Item news for everyone: a box's or a monster's special item, an NPC's, a high refine that took or didn't. rathena's
/// notes name the lines after the first two; these are the ones whose words fit.
const ITEM_FROM_BOX: usize = 1628;
const ITEM_FROM_NPC: usize = 1868;
const REFINE_SUCCEEDED: usize = 3271;
const REFINE_FAILED: usize = 3272;
/// The MVP's reward lines: before and after the item's name, before and after the EXP, and the too-heavy one.
const MVP_ITEM: [usize; 2] = [143, 144];
const MVP_EXP: [usize; 2] = [145, 146];
const MVP_OVERWEIGHT: usize = 147;
/// The bank's refusals: unavailable, not enough zeny carried, too much to hold, not enough in the bank.
const BANK_DOWN: usize = 2454;
const BANK_NO_ZENY: usize = 2456;
const BANK_OVERFLOW: usize = 2459;
const BANK_SHORT: usize = 2455;
/// Pick lists' titles: a Magnifier's, a card's.
const ITEM_APPRAISAL: usize = 521;
const INSERT_CARD: usize = 522;
/// A card put in, or not.
const CARD_DONE: usize = 493;
const CARD_FAILED: usize = 494;
/// The refine window's lines: nothing to refine with; the results (done, broken, lowered, failed).
const CANT_REFINE: usize = 2970;
const REFINE_RESULTS: [(usize, [u8; 3]); 4] = [(2971, CHAT_ITEM), (3246, CHAT_ERROR), (1537, CHAT_ERROR), (2972, CHAT_ERROR)];
/// Making lists' titles (things to make, Arrow Crafting's), nothing to make, and the results: made, not made.
// ponytail: the titles per list and the result lines (428/429 exist too) are estimates.
const MAKING_TITLE: usize = 425;
const ARROW_TITLE: usize = 658;
const NOTHING_TO_MAKE: usize = 424;
const MADE: usize = 431;
const NOT_MADE: usize = 430;
/// Blacksmith services: the lists' titles, a repair done or not, and Upgrade Weapon's results (refined, failed,
/// skill too low, ore missing) in rathena's colors for them.
/// The spell lists' title.
// ponytail: one title for both; the real client's isn't known.
const SPELL_TITLE: usize = 697;
const REPAIR_TITLE: usize = 812;
const REPAIRED: usize = 813;
const NOT_REPAIRED: usize = 814;
const UPGRADE_TITLE: usize = 910;
const UPGRADE_RESULTS: [(usize, [u8; 3]); 4] = [(911, [0, 255, 255]), (912, [0, 205, 205]), (913, [255, 200, 200]), (914, [255, 200, 200])];
/// Warp Portal's and Teleport's list: Teleport's random spot, and the last row.
const RANDOM_AREA: usize = 212;
const WARP_CANCEL: usize = 218;
/// What a map doesn't allow: teleporting, memo points, the skill, the item.
const MAP_FORBIDS: [usize; 4] = [500, 501, 1334, 1335];
/// How far (in cells) others' sounds carry, fading on the way.
/// The sound of a landed hit (under data\wav\). On a player, its body by its job's line (`lineage`): metal for the
/// Swordman, Monk and Star Gladiator lines, wood for the Archer, Thief, Gunslinger and Ninja lines, cloth for the rest.
/// On anything else, the attacker's weapon type: unknown (a monster) one of four plain hits, bare hands one of four
/// fists (`roll` picks).
fn hit_sound(lineage: Option<&[u16]>, weapon: Option<u32>, roll: u32) -> String {
    const METAL: [u16; 7] = [1, 4002, 4024, 15, 4016, 4038, 4047];
    const WOOD: [u16; 8] = [3, 4003, 4026, 6, 4005, 4029, 24, 25];
    let pick = roll % 4 + 1;
    let name = match (lineage, weapon) {
        (Some(jobs), _) if jobs.iter().any(|job| METAL.contains(job)) => "player_metal".to_owned(),
        (Some(jobs), _) if jobs.iter().any(|job| WOOD.contains(job)) => "player_wooden_male".to_owned(),
        (Some(_), _) => "player_clothes".to_owned(),
        (None, None) => format!("_enemy_hit_normal{pick}"),
        (None, Some(0)) => format!("_hit_fist{pick}"),
        (None, Some(1..=3)) => "_hit_sword".to_owned(),
        (None, Some(4 | 5)) => "_hit_spear".to_owned(),
        (None, Some(6 | 7)) => "_hit_axe".to_owned(),
        (None, Some(10 | 23)) => "_hit_rod".to_owned(),
        (None, Some(11)) => "_hit_arrow".to_owned(),
        (None, Some(17)) => "_hit_권총".to_owned(),
        (None, Some(18)) => "_hit_라이플".to_owned(),
        (None, Some(19)) => "_hit_개틀링한발".to_owned(),
        (None, Some(20)) => "_hit_샷건".to_owned(),
        (None, Some(21)) => "_hit_그레네이드런쳐".to_owned(),
        (None, Some(_)) => "_hit_mace".to_owned(),
    };
    format!("{name}.wav")
}

/// The sounds of a unit's state changing (body state OPT1, health bits OPT2): stone setting in and breaking, a freeze
/// breaking, stun; poison, curse, silence and blindness starting.
// ponytail: which state change plays which sound is partly guessed from the files' names.
fn state_sounds(before: (u16, u16), after: (u16, u16)) -> Vec<&'static str> {
    let (stone, stone_wait, freeze, stun) = (1, 6, 2, 3);
    let mut sounds = Vec::new();
    match (before.0, after.0) {
        (from, to) if from == to => {}
        (from, to) if (to == stone || to == stone_wait) && from != stone && from != stone_wait => sounds.push("_stonecurse.wav"),
        (from, _) if from == stone => sounds.push("_stone_explosion.wav"),
        (from, _) if from == freeze => sounds.push("_frozen_explosion.wav"),
        (_, to) if to == stun => sounds.push("_stun.wav"),
        _ => {}
    }
    for (bit, sound) in [(0x01, "_poison.wav"), (0x02, "_curse.wav"), (0x04, "_silence.wav"), (0x10, "_blind.wav")] {
        if after.1 & bit != 0 && before.1 & bit == 0 {
            sounds.push(sound);
        }
    }
    sounds
}

/// How loud an effect sound is `cells` away: full within 8 cells (40 world units), then 40 / distance, none from 50
/// cells (250 units) on.
fn heard(cells: f32) -> f32 {
    let distance = cells * world::CELL;
    if distance >= 250.0 { 0.0 } else { (40.0 / distance).min(1.0) }
}
const WHISPER_COLOR: [u8; 3] = [0xFF, 0xFF, 0x00];
/// "No one of that name is online", then the two ways of being ignored (147 + the result).
const WHISPER_FAILED: usize = 148;
/// "Please wait 10 seconds before trying to log out."
const LOGOUT_WAIT: usize = 502;
/// /help: its heading, then the help lines.
const HELP_TITLE: usize = 22;
const HELP_LINES: std::ops::RangeInclusive<usize> = 33..=46;
/// /who: how many are online (%d).
const USER_COUNT: usize = 178;
/// /ex alone: no one blocked. After a name, how blocking it went (193 + the result) or allowing it (196 + the result).
const BLOCKED_NONE: usize = 192;
const BLOCK_RESULTS: usize = 193;
const ALLOW_RESULTS: usize = 196;
/// /exall done or failed (58, 59), /inall (60, 61).
const BLOCK_ALL_RESULTS: usize = 58;
const ALLOW_ALL_RESULTS: usize = 60;
/// /effect: effects on, effects off.
const EFFECTS_ON: usize = 23;
const EFFECTS_OFF: usize = 24;
/// Deals: "requests a deal", no one of that name, "can't overlap items", "no more than 10 kinds", how asking went
/// (70 + the result), how it ended (75 done, 76 failed, 74 cancelled), too heavy.
const TRADE_ASKS: usize = 93;
const TRADE_NO_ONE: usize = 71;
const TRADE_OVERLAP: usize = 51;
const TRADE_FULL: usize = 297;
const TRADE_ANSWERS: usize = 70;
const TRADE_DONE: usize = 75;
const TRADE_FAILED: usize = 76;
const TRADE_CANCELLED: usize = 74;
const TRADE_HEAVY: usize = 73;
/// "Request a deal with %s".
const TRADE_CHOICE: usize = 87;
/// "Check %s's Equipment Info"; the Show Equipment box's answers, on and off.
const VIEW_CHOICE: usize = 1360;
/// "Send an adoption request to %s"; the questions to the one asked: "(name)" + " wishes to adopt you. Do you
/// accept?", then what a baby class gives up.
const ADOPT_CHOICE: usize = 887;
const ADOPT_ASKS: usize = 794;
const ADOPT_ASKS_AGAIN: usize = 888;
/// Asking to adopt, refused: a child already, under level 70, the one asked is married.
const ADOPT_REFUSALS: [usize; 3] = [899, 900, 916];
/// "%s and %s have divorced from each other."
const DIVORCED: usize = 836;
/// The Wedding Rings (a man's, a woman's) and I Miss You, which the ring gives.
const WEDDING_RINGS: [u32; 2] = [2634, 2635];
const CALL_PARTNER: u16 = 336;
/// The wedding effect.
// ponytail: the effect ZC_CONGRATULATION plays isn't knowable; 347 (EF_COLORPAPER) is the one whose sound the
// client's table names effect\wedding.
const WEDDING_EFFECT: u32 = 347;
/// Pets: the egg list's title, the menu (status, feed, performance, back to the egg, accessory off, by command), the
/// questions (feeding; the name, then renaming once), no food, and neglect.
// ponytail: the menu's order is a guess; the strings sit together like one.
const PET_EGGS: usize = 599;
const PET_CHOICES: [(usize, u8); 5] = [(596, net::PET_INFO), (592, net::PET_FEED), (593, net::PET_PERFORM), (594, net::PET_RETURN), (595, net::PET_UNEQUIP)];
const PET_FEED_ASKS: usize = 601;
const PET_NAME_RULE: usize = 682;
const PET_RENAME_ASKS: usize = 683;
const PET_NO_FOOD: usize = 591;
const PET_NEGLECTED: usize = 1188;
const EQUIPMENT_SHOWN: usize = 1359;
const EQUIPMENT_HIDDEN: usize = 1358;
/// Vending: "%s %d sold.", "Items are sold out.", and "There are items priced 0..." (a shop refused for it).
const VEND_SOLD: usize = 231;
const VEND_SOLD_OUT: usize = 311;
const VEND_FAILED: usize = 2720;
/// The cart window's title ("Cart Items") and where it first opens.
const CART_TITLE: usize = 222;
const CART_POSITION: [f32; 2] = [300.0, 172.0];
/// Guilds: "Send (%s) a Guild Invitation", " Guild has sent you an invitation...", made (374, then 375, 376, 405),
/// how an invitation went (378 + the result), "Guild Member %s has connected/disconnected", guild chat's color.
const GUILD_CHOICE: usize = 382;
/// "Send an Guild Alliance Request", "Set this guild as an Antagonist".
const ALLY_CHOICE: usize = 399;
const ANTAGONIST_CHOICE: usize = 403;
/// "(guild) Guild is asking for an Alliance with them. Do you accept?", "Are you sure that you want to delete it?"
const ALLY_ASKS: usize = 393;
const DELETE_ASKS: usize = 356;
/// "Are you sure that you want to leave?", and the lines when someone leaves: "You left the guild.", "%s has left the
/// guild.", "Withdrawal Reason: %s". The member menu's "Leave Guild".
const LEAVE_ASKS: usize = 357;
const LEFT_GUILD: usize = 489;
const MEMBER_LEFT: usize = 364;
const LEFT_BECAUSE: usize = 365;
const LEAVE_CHOICE: usize = 508;
/// A tracked quest's menu: "Quest information", "Delete".
const QUEST_INFO_CHOICE: usize = 1622;
const QUEST_DELETE_CHOICE: usize = 351;
/// "Position Title", over the question for a new one.
const POSITION_TITLE: usize = 511;
const GUILD_INVITED: usize = 377;
const GUILD_ONLINE: usize = 485;
const GUILD_CHAT: [u8; 3] = [180, 255, 180];
// ponytail: clan chat's color isn't in the data files; calibrate against the official client.
const CLAN_CHAT: [u8; 3] = [180, 220, 255];
/// Friends: "Register as a Friend", "(%s) wishes to be friends with you...", how adding went (821 + the result, but
/// 819 and 820 for full lists), "%s has logged in/out".
const FRIEND_CHOICE: usize = 358;
const FRIEND_ASKS: usize = 818;
/// Inventory expansion vouchers: [Event], [Limited], and the coupon.
const VOUCHERS: [u32; 3] = [25791, 25792, 25793];
/// What a voucher adds.
const VOUCHER_SLOTS: usize = 10;
/// The question (the voucher, the size now, the size after), and the results: done, failed, busy, no voucher, at
/// the most.
// ponytail: rathena sends numbers only; the lines are matched to them by their words.
const EXPANSION_ASKS: usize = 3561;
const EXPANSION_RESULTS: [usize; 5] = [3566, 3562, 3563, 3564, 3565];
const FRIEND_ONLINE: usize = 1041;
/// Parties: "Ask %s to join your party", made, " Party has sent you an invitation...", "You left the party", party
/// chat colors (ours, others'), the prompts' titles.
const PARTY_ASK: usize = 88;
const PARTY_MADE: usize = 77;
const PARTY_INVITED: usize = 94;
const PARTY_LEFT: usize = 84;
const PARTY_CHAT_OURS: [u8; 3] = chat::PARTY_OURS;
const PARTY_CHAT: [u8; 3] = chat::PARTY;
/// "%s has acquired %s.", a member's pickup.
const PARTY_PICKUP: usize = 1276;
/// "Costume equipments are not visible on this map."
const NO_COSTUME_NOTICE: usize = 1772;
/// "In this Map,Effect of Mirace of Sun and Moon is nullified."
const NO_MIRACLE_NOTICE: usize = 2381;
const MAP_NOTICE: [u8; 3] = [0x64, 0xFF, 0xFF];
/// "[Point] You have been rewarded with %d … rank points. Your point total is %d.", by ranking.
const RANK_POINTS_BLACKSMITH: usize = 901;
const RANK_POINTS_ALCHEMIST: usize = 902;
const RANK_POINTS_TAEKWON: usize = 926;
const RANK_POINTS_SLAYER: usize = 953;
/// "[Taekwon Mission] Target Monster: %s (%d%%)"
const MISSION_TARGET: usize = 927;
/// rankfont's and timefont's action for the slash or colon (0 to 9 are the digits).
const GLYPH_SEPARATOR: usize = 10;
/// "Congratulations! %s's rank has gone up to %d." / "What a pity! %s's rank has gone down to %d."
const RANK_UP: usize = 582;
const RANK_DOWN: usize = 583;
const RANK_LINE: [u8; 3] = [0xFF, 0xFF, 0x00];
/// EF_TOPRANK, on every player ranked 1 to 10.
const TOP_RANK_EFFECT: u32 = 159;
/// How long the "time attack" banner shows after reaching first place.
const FIRST_PLACE_BANNER: Duration = Duration::from_secs(3);

fn digits(number: u64) -> Vec<usize> {
    number.to_string().bytes().map(|digit| usize::from(digit - b'0')).collect()
}

/// The PvP counter's glyphs (rankfont actions) and where each is anchored: the rank right-aligned with its last digit
/// at W − 105, H − 50, 25 px apart; the slash at W − 80, H − 40; the player count (at most 9999) from W − 55, H − 30.
fn counter_glyphs(rank: u32, players: u32, screen: [f32; 2]) -> Vec<(usize, [f32; 2])> {
    let [width, height] = screen;
    let rank = digits(u64::from(rank));
    let count = rank.len() as f32;
    let mut glyphs: Vec<_> = rank.into_iter().enumerate().map(|(n, digit)| (digit, [width - 105.0 - (count - 1.0 - n as f32) * 25.0, height - 50.0])).collect();
    glyphs.push((GLYPH_SEPARATOR, [width - 80.0, height - 40.0]));
    glyphs.extend(digits(u64::from(players.min(9999))).into_iter().enumerate().map(|(n, digit)| (digit, [width - 55.0 + n as f32 * 25.0, height - 30.0])));
    glyphs
}

/// The first-place clock's glyphs (timefont actions): mm:ss, hh:mm:ss or dd:hh:mm:ss, every part two digits.
fn clock_glyphs(seconds: u64) -> Vec<usize> {
    let parts = [seconds / 86_400, seconds / 3600 % 24, seconds / 60 % 60, seconds % 60];
    // Minutes and seconds at the least.
    let first = parts.iter().position(|&part| part > 0).unwrap_or(2).min(2);
    let mut glyphs = Vec::new();
    for (n, part) in parts[first..].iter().enumerate() {
        if n > 0 {
            glyphs.push(GLYPH_SEPARATOR);
        }
        glyphs.extend(digits(*part / 10).into_iter().chain(digits(*part % 10)));
    }
    glyphs
}

/// The chat line when our rank moves into or out of the top ten (the first rank compares with 0).
fn rank_line(before: u32, now: u32) -> Option<usize> {
    let top = |rank: u32| (1..=10).contains(&rank);
    match (top(before), top(now)) {
        (false, true) => Some(RANK_UP),
        (true, false) => Some(RANK_DOWN),
        (false, false) if before == 0 && now > 10 => Some(RANK_DOWN),
        _ => None,
    }
}
/// The bow's weapon type (rathena's W_BOW): its attack pace isn't capped.
const BOW: u32 = 11;
/// The Star Gladiator line, whose Miracle of Sun, Moon and Stars a map can turn off (rathena's `enum e_job`).
const STAR_GLADIATORS: [u16; 10] = [4047, 4048, 4226, 4238, 4239, 4243, 4241, 4244, 4302, 4316];

/// A unit that died falls when the killing hit lands, playing its death at the sprite's pace. A player lies there until
/// revived (ZC_RESURRECTION) or gone; any other body then lies on its last frame as a corpse and fades out.
fn fall(actor: &mut Actor, lands: Instant) {
    let (.., die) = actor::combat_actions(actor.kind);
    actor.play_at_pace(die, lands, 1.0, true);
    if actor.kind == PLAYER {
        actor.dead = true;
    } else {
        let dying_ms = actor.sprites.as_ref().and_then(|sprites| sprites.action_ms(die, actor.direction)).unwrap_or(0.0);
        actor.fade_out(lands + Duration::from_micros((f64::from(dying_ms) * 1000.0).round() as u64), CORPSE_FADE_MS);
    }
}

/// What a map's flags tell a player of this job on arrival: the messages to print.
fn arrival_notices(flags: u32, job: u16) -> Vec<usize> {
    let mut notices = Vec::new();
    if flags & net::MAP_NO_COSTUME != 0 {
        notices.push(NO_COSTUME_NOTICE);
    }
    if flags & net::MAP_SUN_MOON_STAR_MIRACLE == 0 && STAR_GLADIATORS.contains(&job) {
        notices.push(NO_MIRACLE_NOTICE);
    }
    notices
}
const PARTY_REFUSING: usize = 1325;
const PARTY_ALLOWING: usize = 1326;
/// "You can't modify Party Setup."
const PARTY_SETUP_REFUSED: usize = 373;
const LEADER_CHOICE: usize = 1532;
/// "Item cannot be dropped from the hotkey bar."
const HOTKEY_ITEM_STAYS: usize = 1696;
/// Dropping on the ground with the equipment window open, the mail window open, or the ammunition worn.
const DROP_EQUIPMENT_OPEN: usize = 189;
const DROP_MAIL_OPEN: usize = 2736;
const DROP_AMMO_WORN: usize = 657;
/// Nothing is dropped on the ground this soon after asking to wear something.
const WEAR_PAUSE: Duration = Duration::from_secs(2);
const LEADER_QUESTION: usize = 1533;
const PARTY_CREATE: usize = 2054;
/// What a locked chat room asks ("Please enter Password.").
const ROOM_PASSWORD: usize = 15;
const PARTY_INVITE: usize = 2056;
/// An item without a picture of its own shows the apple's.
const FALLBACK_ITEM_SPRITE: &str = "사과";

/// How high a just-dropped item is over the ground, `ticks` (24 ms each) after it appears: it starts 15 units up, hops
/// a little, then falls and lands after about 0.42 s.
fn item_fall_height(ticks: f32) -> f32 {
    (15.0 + 0.6 * ticks - 0.083 * ticks * ticks).max(0.0)
}

/// An item on the ground glints yellow for 2 ticks every 92 (48 ms every 2.208 s), counted from when it appeared.
fn item_glints(ticks: f32) -> bool {
    (ticks as i32).rem_euclid(92) >= 90
}

/// The sprite an item lies on the ground as (identified or not), the apple when the data has none.
fn ground_sprite(items: &HashMap<u32, ItemInfo>, item: u32, identified: bool) -> &str {
    items.get(&item).map(|info| info.resource(identified)).filter(|resource| !resource.is_empty()).unwrap_or(FALLBACK_ITEM_SPRITE)
}
/// Items cast 0.4 of a character's shadow.
const ITEM_SHADOW: f32 = 0.4;

/// The size a unit is drawn at. The size table applies to monsters (1 when the job isn't listed).
/// ChangeSize's percent multiplies that. 100 leaves it. None leaves the table factor.
fn unit_draw_scale(sizes: &HashMap<u16, f32>, kind: u8, job: u16, change_percent: Option<i32>) -> f32 {
    let factor = if kind == MONSTER { sizes.get(&job).copied().unwrap_or(1.0) } else { 1.0 };
    lua::sprite_scale(factor, change_percent)
}

/// Grows a frame around the feet. 1 leaves it.
fn scale_frame(placed: &mut [actor::Placed], scale: f32) {
    if scale == 1.0 {
        return;
    }
    for (_, position, size, _) in placed {
        *position = position.map(|value| value * scale);
        *size = size.map(|value| value * scale);
    }
}

// ponytail: dialog layout eyeballed from the classic client; calibrate against a screenshot of the real one.
/// Offsets inside the 280x120 dialog bitmaps (win_login, win_service, win_msgbox).
const LEFT_BUTTON: [f32; 2] = [189.0, 96.0];
/// A shop's question: buy, sell or cancel, in a message box.
const DEAL_BUTTONS: [[f32; 2]; 3] = [[143.0, 96.0], [189.0, 96.0], [235.0, 96.0]];
const DEAL_MESSAGE: usize = 92;
const RIGHT_BUTTON: [f32; 2] = [235.0, 96.0];
const FIELD_PADDING: f32 = 3.0;
// ponytail: the login window's places (2018+ clients) are estimates; calibrate against the real client.
/// The login window (bg_login.tga): centered, 60% down; its ID and password fields, the save box, the round
/// start button and the close mark drawn into the bitmap.
const LOGIN_TOP: f32 = 460.0;
const LOGIN_FIELDS: [[f32; 2]; 2] = [[17.0, 39.0], [17.0, 61.0]];
const LOGIN_FIELD_SIZE: [f32; 2] = [127.0, 18.0];
const LOGIN_SAVE: ([f32; 2], [f32; 2]) = ([57.0, 85.0], [34.0, 10.0]);
const LOGIN_START: [f32; 2] = [192.0, 33.0];
const LOGIN_EXIT: ([f32; 2], [f32; 2]) = ([270.0, 13.0], [24.0, 23.0]);
const SERVER_LIST: [f32; 2] = [10.0, 22.0];
const SERVER_ROW: [f32; 2] = [260.0, 18.0];
const SELECTION_COLOR: [u8; 4] = [0xC6, 0xDE, 0xF7, 0xFF];
const MESSAGE_TEXT: [f32; 2] = [20.0, 40.0];

/// The message box's text area and its input field (the birthdate prompt).
const MESSAGE_WIDTH: f32 = 240.0;
const MESSAGE_LINE: f32 = 16.0;
// ponytail: the prompt's field is placed like the login fields; calibrate against the real client.
const PROMPT_FIELD: [f32; 2] = [20.0, 70.0];
const PROMPT_FIELD_SIZE: [f32; 2] = [240.0, 18.0];
const PROMPT_LENGTH: usize = 6;
/// The most letters a new name takes (NAME_LENGTH, less its terminator).
const NAME_MOST: usize = NAME_LENGTH - 1;
const MOVE_QUESTION: usize = 2016;
const MOVE_FAILED: usize = 2017;
const RENAME_QUESTION: usize = 1213;
/// "Ready to change character name in.(%d)": the name prompt, with the renames left.
const RENAME_PROMPT: usize = 2026;
const QUIT_MESSAGE: usize = 17;
const DELETE_QUESTION: usize = 19;
const BIRTHDATE_MESSAGE: usize = 1815;

/// NPC dialogs are drawn as shapes, like the client draws them.
const DIALOG_SIZE: [u32; 2] = [276, 176];
const DIALOG_CONTENT: [u32; 4] = [8, 8, 272, 148];
const DIALOG_TEXT: [f32; 2] = [13.0, 13.0];
const DIALOG_TEXT_WIDTH: f32 = 254.0;
const DIALOG_LINE: f32 = 18.0;
const DIALOG_LINES: usize = 7;
const DIALOG_BUTTON: [f32; 2] = [226.0, 151.0];
const MENU_SIZE: [u32; 2] = [276, 116];
const MENU_CONTENT: [u32; 4] = [8, 10, 268, 90];
const MENU_ROW: [f32; 2] = [260.0, 20.0];
const MENU_ROWS: usize = 4;
const MENU_OK: [f32; 2] = [184.0, 91.0];
const MENU_CANCEL: [f32; 2] = [230.0, 91.0];
const MENU_SELECTION: [u8; 4] = [0xCD, 0xE0, 0xFF, 0xFF];
/// A player's action while casting or using a skill.
const SKILL_ACTION: usize = 12;
/// How far over the ground the magic square under a ground cast floats, so the ground doesn't cover it.
const CAST_SQUARE_LIFT: f32 = 0.2;
/// The navigation arrow sits this far over the head; routes the player starts may use every service.
// ponytail: the client's arrow height and its services for a route picked by hand are estimates.
const NAVI_ARROW_RISE: f32 = 12.0;
const NAVI_SERVICES: u8 = 111;
/// How many cells toward where it heads the arrow looks.
const NAVI_ARROW_LOOK: f32 = 3.0;
/// The room owner's menu: "Give Master Authority", "Kick Character Out".
const ROOM_HAND_OVER: usize = 128;
const ROOM_KICK: usize = 127;
/// Words over a head stay this long.
const SPEECH_TIME: Duration = Duration::from_secs(5);
/// Spirit spheres and coins: how far from the body they circle, how high, how fast (radians a second), and how
/// much flatter the circle looks than it is.
// ponytail: estimates; calibrate against the real client.
const SPIRIT_RADIUS: f32 = 28.0;
const SPIRIT_HEIGHT: f32 = 45.0;
const SPIRIT_TURN: f32 = 1.5;
const SPIRIT_TILT: f32 = 0.4;
/// The sphere picture's ball is about 22px across in a 94px square.
const SPHERE_SCALE: f32 = 0.5;
/// Shields, souls and charms circle at their own heights, so two kinds on one unit don't overlap; the shield and
/// charm pictures are scaled down to about a sphere's size, and souls are spheres tinted pale violet.
// ponytail: heights, sizes and tints are estimates; calibrate against the real client.
const SHIELD_HEIGHT: f32 = 22.0;
const SHIELD_SCALE: f32 = 0.22;
const SOUL_HEIGHT: f32 = 64.0;
const SOUL_TINT: [f32; 4] = [0.85, 0.7, 1.0, 1.0];
const CHARM_HEIGHT: f32 = 84.0;
const CHARM_SCALE: f32 = 0.4;
/// Charm tints by element: water, earth, fire, wind.
const CHARM_TINTS: [[f32; 4]; 4] = [[0.5, 0.7, 1.0, 1.0], [0.75, 0.55, 0.35, 1.0], [1.0, 0.45, 0.35, 1.0], [0.5, 1.0, 0.55, 1.0]];
const CHARM_NAMES: [&str; 4] = ["water", "earth", "fire", "wind"];
/// Heal, Highness Heal and Coluceo Heal report how much they healed.
const HEAL_SKILLS: [u16; 3] = [28, 2043, 2044];
const STATUS_ICON: f32 = status_tip::ICON;
/// Steps of the clock drawn over a status icon.
const CLOCK_STEPS: usize = 32;
/// A skill still blocked: "There is a Delay after using a Skill.", and the words after a cooldown's seconds left.
const COOLDOWN_REFUSED: usize = 1460;
/// A skill goes once less than this of its cooldown is left.
const COOLDOWN_GRACE: Duration = Duration::from_millis(100);
/// The Talkie Box (a Hunter's trap holding a message), and the box asking for its message.
// ponytail: 436 "Please type a message for the Talkie Box"; 435 may be the official one.
const TALKIE_BOX: u16 = 125;
/// The aim balloons' text.
const AIM_TEXT: [u8; 3] = [50, 255, 50];
/// The one skill that takes a homunculus as its target.
const POTION_PITCHER: u16 = 231;
const TALKIE_ASKS: usize = 436;
/// Equipment: a break ("[%s]'s %s has been damaged.", Weapon, Armor), the item's damaged line, and binding (the
/// question before, the line after).
const DAMAGED: usize = 1301;
const DAMAGED_WEAPON: usize = 1302;
const DAMAGED_ARMOR: usize = 1303;
const ITEM_DAMAGED: usize = 699;
const BIND_ASKS: usize = 1327;
const BOUND: usize = 1328;
/// Rentals: " The [%s] item will disappear in %d minutes.", the same in seconds, and " [%s] item has been deleted
/// from the Inventory.".
const RENTAL_MINUTES: usize = 1256;
const RENTAL_SECONDS: usize = 1270;
const RENTAL_ENDED: usize = 1258;
/// Item merge: done, and the total too big.
const MERGED: usize = 2171;
const MERGE_TOO_MANY: usize = 2173;
/// EFST_BLADESTOP, EFST_CURSEDCIRCLE_ATKER and EFST_CURSEDCIRCLE_TARGET: a unit locked with another.
const LOCK_ICONS: [u16; 3] = [85, 416, 417];
/// "<item> have been placed on / removed from the Swap Equipment window."
const SWITCH_ADDED: usize = 3143;
const SWITCH_REMOVED: usize = 3144;
/// The Convex Mirror: no boss here, the boss will show on the minimap; its status icon (EFST_CASH_BOSS_ALARM).
const NO_BOSS: usize = 1223;
const BOSS_SHOWN: usize = 1225;
const BOSS_ALARM: u16 = 253;
/// The spark of a landed hit, and a warp portal's rings.
const EF_HIT1: u32 = 0;
const EF_WARPZONE2: u32 = 321;
/// The MVP's banner (mvp.str).
const EF_MVP: u32 = 68;
/// The cast aura by the skill's element: neutral, water, earth, fire, wind, poison, holy, shadow, ghost, undead.
const CAST_AURAS: [u32; 10] = [12, 54, 56, 55, 57, 59, 58, 12, 12, 12];
/// "Select a target", under an aimed skill's name.
const TARGET_MESSAGE: usize = 234;
/// Taming's result: the slot machine's success and failure actions (they play those sounds), how long the last
/// frame stays, and the chat lines.
// ponytail: the slot machine at the screen's middle and the lines in the chat; how and where the real client shows
// the result isn't known.
const SLOT_CAUGHT: usize = 2;
const SLOT_MISSED: usize = 3;
const SLOT_HOLD_MS: f32 = 1500.0;
const TAMED: usize = 1574;
const NOT_TAMED: usize = 1575;
/// HP/SP bars under characters: border, background, HP, low HP (under a quarter), SP, hunger, a 4th job's third row;
/// and monsters' HP bars.
const BAR_COLORS: [[u8; 4]; 7] = [
    [0x14, 0x19, 0x9B, 255],
    [0x44, 0x44, 0x44, 255],
    [0x16, 0xEA, 0x25, 255],
    [0xEA, 0x16, 0x16, 255],
    [0x1D, 0x65, 0xDB, 255],
    [0xFF, 0xE4, 0xE1, 255],
    [0xFF, 0xC0, 0x60, 255],
];
const MONSTER_BAR_COLORS: [[u8; 4]; 4] = [[0x10, 0x18, 0x9C, 255], [0x42, 0x41, 0x42, 255], [0xFF, 0x00, 0xE7, 255], [0xFF, 0xFF, 0x00, 255]];
/// A multi-hit's hits come this far apart.
const MULTI_HIT_DELAY: Duration = Duration::from_millis(200);
/// A corpse fades out over this long (one step of alpha every 24 ms); a unit walking out of sight over this.
const CORPSE_FADE_MS: f32 = 255.0 * 24.0;
const OUT_OF_SIGHT_FADE_MS: f32 = 510.0;

/// Unit object types (rathena's clif_bl_type).
const PLAYER: u8 = 0;
const MONSTER: u8 = 5;
const NPC: u8 = 6;
const PET: u8 = 7;
const HOMUNCULUS: u8 = 8;
/// rathena's NPC_ELEMENTAL_TYPE.
const ELEMENTAL: u8 = 10;
const MERCENARY: u8 = 9;
/// The effect launcher (104), JT_HIDDEN_NPC and JT_HIDDEN_WARP_NPC: invisible script NPCs.
const HIDDEN_JOBS: [u16; 3] = [104, 111, 139];
/// Names over the world: NPCs', monsters' (red well above our level, grey well under), our summons', ground items'.
const NPC_NAME_COLOR: [u8; 3] = [0x95, 0xB8, 0xF7];
const MONSTER_NAME_COLOR: [u8; 3] = [0xFF, 0xC3, 0xC3];
const STRONG_NAME_COLOR: [u8; 3] = [0xFF, 0x00, 0x00];
const WEAK_NAME_COLOR: [u8; 3] = [0x9B, 0x9B, 0x9B];
const SUMMON_NAME_COLOR: [u8; 3] = [0xC8, 0xC8, 0xC8];
const ITEM_NAME_COLOR: [u8; 3] = [0xFF, 0xEF, 0x95];

/// The colour of a unit's name over the world, by its object type and job: NPC-like units blue, monsters pink (red at
/// 11 or more levels above ours, grey at more than 15 below), our own summons grey, players and the rest white.
fn name_color(kind: u8, job: u16, level: u16, ours: bool, own_level: i64) -> [u8; 3] {
    if ours {
        return SUMMON_NAME_COLOR;
    }
    if matches!(kind, 1 | NPC | 12) || matches!(job, 45..=999 | 10001..=20000) {
        return NPC_NAME_COLOR;
    }
    if !matches!(job, 1001..=3998 | 20021..=31999) {
        return CHAT_WHITE;
    }
    match i64::from(level) - own_level {
        11.. => STRONG_NAME_COLOR,
        ..-15 => WEAK_NAME_COLOR,
        _ => MONSTER_NAME_COLOR,
    }
}

/// The effect a unit leaves where it vanished, by rathena's `clr_type`: 3 teleported (EF_TELEPORTATION), 2 logged out
/// (its fainter rings); nothing for a GM hidden from sight (OPTION_INVISIBLE), nor for going out of sight or dying.
// ponytail: the 300 and 600 the game passes with these aren't used.
fn vanish_effect(reason: u8, option: u32) -> Option<u32> {
    if option & actor::OPTION_INVISIBLE != 0 {
        return None;
    }
    match reason {
        3 => Some(304),
        2 => Some(314),
        _ => None,
    }
}

/// Over a monster: the attack cursor when a click would attack it (not with Shift held, unless /noshift is on).
fn monster_cursor(shift: bool, no_shift: bool) -> cursor::Kind {
    if shift && !no_shift { cursor::Kind::Default } else { cursor::Kind::Attack }
}

/// The turning cursor only while the right button turns the camera: not when Shift tilts it, nor with Alt.
fn turning_cursor(dragging: bool, shift: bool, alt: bool) -> bool {
    dragging && !shift && !alt
}

/// A warp portal's pick box around its feet, in sprite pixels: 50/640 of the screen's width and 50/730 of its height
/// each way at the default camera, so it grows and shrinks with the zoom like a sprite.
// ponytail: worked out for the 1024-wide screen; a wider canvas keeps the same box.
const WARP_PICK: [f32; 2] = [59.7, 39.3];

/// A warp portal's pick box on screen (`0..1`, y down: left, top, right, bottom).
fn warp_pick_rect(view: &world::View, feet: world::Vector) -> Option<[f32; 4]> {
    view.sprite_rect(feet, [-WARP_PICK[0], -WARP_PICK[1]], WARP_PICK)
}

/// /snap, /itemsnap and /skillsnap.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Snaps {
    snap: bool,
    item: bool,
    skill: bool,
}

impl Snaps {
    /// As saved; a first run has only /skillsnap on.
    fn load() -> Self {
        let saved =
            |name: &str, default: bool| settings_path(name).and_then(|path| std::fs::read_to_string(path).ok()).map_or(default, |text| text.trim() == "1");
        Self { snap: saved("snap", false), item: saved("itemsnap", false), skill: saved("skillsnap", true) }
    }
}

/// What is under the pointer, for snapping.
#[derive(Clone, Copy, Debug, PartialEq)]
enum SnapTo {
    Monster,
    Item,
    OurHomunculus,
    Other,
}

/// Where the cursor is drawn when it snaps: the middle of the pick box under the pointer (a monster with /skillsnap
/// while a skill waits for a unit, with /snap otherwise; an item with /itemsnap; our homunculus while aiming Potion
/// Pitcher). None: it stays at the pointer.
// ponytail: Potion Pitcher's snap to our homunculus goes by /skillsnap too.
fn snap_point(under: Option<(SnapTo, picking::Rect)>, aiming_unit: bool, pitcher: bool, snaps: Snaps) -> Option<[f32; 2]> {
    let (kind, [left, top, right, bottom]) = under?;
    let snaps_here = match kind {
        SnapTo::Monster => {
            if aiming_unit {
                snaps.skill
            } else {
                snaps.snap
            }
        }
        SnapTo::Item => snaps.item,
        SnapTo::OurHomunculus => pitcher && snaps.skill,
        SnapTo::Other => false,
    };
    snaps_here.then_some([(left + right) / 2.0, (top + bottom) / 2.0])
}

/// JT_WARPNPC: warp portals, drawn as an effect without a name.
const WARP_JOB: u16 = 45;

// The camera by default: 300 world units (60 cells) from the player, 45 degrees up, with world.rs's 15 degree field
// of view. Distances are in world units (a cell is 5); the goal keeps the client's zoom units (× 2.5).
const ZOOM: f32 = 120.0;
/// World units per zoom unit.
const UNITS_PER_ZOOM: f32 = 2.5;
/// How near and far the camera goes, outdoors and indoors; a wheel step, and how often the wheel counts.
const DISTANCE_RANGE: [f32; 2] = [230.0, 400.0];
const DISTANCE_RANGE_INDOOR: [f32; 2] = [230.0, 300.0];
/// With /zoom on, outdoors the camera goes out to this; its lines: on, off.
const ZOOM_OUT_DISTANCE: f32 = 730.0;
const ZOOM_OUT_ON: usize = 3511;
const ZOOM_OUT_OFF: usize = 3512;
const WHEEL_STEP: f32 = 24.3;
const WHEEL_GAP: Duration = Duration::from_millis(15);
/// Shift + wheel tilts this many degrees a step.
const WHEEL_TILT: f32 = 5.0;
const PITCH: f32 = 45.0;
/// A double right-click with Shift tilts back to this.
const PITCH_RESET: f32 = 50.0;
/// Indoors the camera keeps a fixed slant you can only turn a little.
const YAW_INDOOR: f32 = 45.0;
const YAW_RANGE_INDOOR: [f32; 2] = [25.0, 65.0];
const CAMERA_DOUBLE_CLICK: Duration = Duration::from_millis(500);
/// /camera's lines: on, off.
const CAMERA_ON: usize = 319;
const CAMERA_OFF: usize = 320;

/// How far the camera tilts: outdoors 50 to 65 degrees (25 to 65 with Ctrl), indoors fixed at 60 (35 to 55 with
/// Ctrl).
fn pitch_range(indoor: bool, ctrl: bool) -> [f32; 2] {
    match (indoor, ctrl) {
        (false, false) => [50.0, 65.0],
        (false, true) => [25.0, 65.0],
        (true, false) => [60.0, 60.0],
        (true, true) => [35.0, 55.0],
    }
}
/// "Disconnected from the server."
const DISCONNECTED_MESSAGE: usize = 3;
/// The loading screen's progress bar.
const LOADING_BAR: [f32; 2] = [240.0, 15.0];

enum NetEvent {
    Login(Result<LoginAccepted, LoginError>),
    Char(CharEvent),
    Map(MapEvent),
    /// A command from the control socket (dev).
    Control(control::Request),
}

/// On a map: the connection, the world and everyone in view.
struct Game {
    /// The windows' places as last saved (see `save_places`).
    saved_places: String,
    session: MapSession,
    world: World,
    map: String,
    me: Character,
    account_id: u32,
    player: Actor,
    units: HashMap<u32, Actor>,
    camera: Camera,
    camera_goal: CameraGoal,
    /// The map's overview picture, for the minimap, and the part of it showing (with what it was cut for).
    minimap: Option<image::RgbaImage>,
    minimap_crop: Option<((u32, u32, u32, minimap::View), Texture)>,
    /// Ctrl+Tab walks this: opaque, see-through, hidden. Kept with the settings (`minimap_view`).
    minimap_view: minimap::View,
    /// The minimap's zoom (1 to 4), kept with the settings.
    minimap_zoom: f32,
    /// ponytail: the navigation route (#276) reads this. The route itself is not drawn here.
    navigation_route: bool,
    /// The big minimap of the current map.
    minimap_window: minimap_window::Window,
    /// Guild members' cells on this map; party members' minimap colours by their place in the party.
    guild_places: HashMap<u32, [i32; 2]>,
    party_colors: Vec<Texture>,
    /// Alt+End hides the player's HP and SP bars. Monsters keep theirs.
    show_bars: bool,
    last_frame: Instant,
    chat: chat::Chat,
    entered: Instant,
    last_keepalive: Instant,
    /// Dev switch: a cell to walk to shortly after entering.
    auto_walk: Option<[u16; 2]>,
    /// Walking by the mouse: when the last walk was sent and where, how many were sent, whether the held button
    /// keeps walking (and when it last tried), and the last press (when, and whether the pointer moved since).
    last_move: Option<Instant>,
    walk_goal: Option<[i32; 2]>,
    walks: u32,
    walk_held: bool,
    last_walk_try: Option<Instant>,
    /// A walk asked for during the player's own swing, sent when the swing ends.
    walk_after_swing: Option<[i32; 2]>,
    /// The client's skill table: names, levels to pick, ranges by level.
    skill_info: HashMap<u16, SkillInfo>,
    press_mark: Option<(Instant, [f32; 2], bool)>,
    auto_say: Option<String>,
    /// The blob under every character.
    shadow: Option<SpriteSet>,
    /// The emotion sprite, and the emotions showing: by unit (None = the player), the sprite action and when.
    emotion_sprite: Option<SpriteSet>,
    emotions: HashMap<Option<u32>, (usize, Instant)>,
    /// The MVP sprite, and over which MVP winners it shows (None = the player) since when.
    mvp_sprite: Option<SpriteSet>,
    mvps: HashMap<Option<u32>, Instant>,
    /// The status marks' sprites (stun, sleep, curse, the ice block) by name.
    mark_sprites: HashMap<&'static str, SpriteSet>,
    /// What circles units (None = the player): spheres or coins, shields, charms, souls; the sphere's, the shield's
    /// and the charm's pictures, the coin's sprite.
    spirits: HashMap<Option<u32>, Orbs>,
    sphere: Option<Texture>,
    shield: Option<Texture>,
    charm: Option<Texture>,
    coin_sprite: Option<SpriteSet>,
    digits: Option<Digits>,
    /// The message sprite: a critical hit's burst and Lucky (`actor::BURST_ACTION`, `LUCKY_ACTION`).
    message_sprite: Option<SpriteSet>,
    /// The NPC conversation on screen.
    dialog: Option<Dialog>,
    /// Last frame's clickable units and items.
    hit_boxes: Vec<picking::Target<Owner>>,
    /// Items lying around, by ground ID.
    ground: HashMap<u32, FloorItem>,
    /// Ground item sprites by resource name.
    item_sprites: HashMap<String, Option<SpriteSet>>,
    inventory: Inventory,
    equipment: Equipment,
    basic_info: BasicInfo,
    hotkeys: hotkeys::Hotkeys,
    /// An item or skill being dragged: what, where the drag began, and whether it has moved yet.
    dragging: Option<(Dragged, [f32; 2], bool)>,
    /// A right press let go of the drag: the left button's release does nothing.
    drag_cancelled: bool,
    skills: Vec<net::Skill>,
    skill_window: skills::SkillWindow,
    /// A Talkie Box's message waiting to be placed, and the boxes saying theirs (where each stands).
    talkie_text: Option<String>,
    talkie_spots: HashMap<u32, world::Vector>,
    /// Skills blocked after use, and the line one still blocked prints when used.
    cooldowns: skills::Cooldowns,
    /// "Cannot use [%s] due to cooldown delay."
    cooldown_line: String,
    /// A skill waiting for its target: ID and level.
    targeting: Option<(u16, u16)>,
    /// A taming item waiting for its monster; the slot machine, and the result it plays (action, since when).
    capturing: bool,
    slot_sprite: Option<SpriteSet>,
    slot: Option<(usize, Instant)>,
    /// Units casting (None = the player): when it began and how long it takes.
    casts: HashMap<Option<u32>, (Instant, Duration)>,
    /// The magic squares under ground skills being cast, by caster (None = the player); each goes with its cast.
    cast_squares: HashMap<Option<u32>, cast_square::Square>,
    /// Units that may use their next combo skill (None = the player): when the window opened and how long it lasts.
    combos: HashMap<Option<u32>, (Instant, Duration)>,
    /// Units locked together by Blade Stop or Cursed Circle: source and target.
    locks: Vec<(u32, u32)>,
    /// Words over heads (chat, skill names), by unit (None = the player): text, color, when said.
    speech: HashMap<Option<u32>, (String, [u8; 3], Instant)>,
    /// Status effects on the player with an icon: the client's EFST id, when it began and ends (None: no end).
    status_icons: Vec<(u16, Instant, Option<Instant>)>,
    /// The lines of the status icon tooltip showing, if one is.
    status_tip: Option<Vec<String>>,
    effects: effects::Effects,
    /// An NPC shop asking whether to buy or sell, and the open shop.
    deal_choice: Option<u32>,
    shop: Option<shop::Shop>,
    storage: storage::Storage,
    /// An NPC's illustration and where it goes.
    cutin: Option<(Texture, u8)>,
    /// NPC marks on the minimap: whose (NPC and mark), the cell, its color and until when (None: until taken away).
    marks: Vec<Mark>,
    /// The box asking how many, and what for.
    amount: Option<window::Amount<Asked>>,
    /// The option menu (Esc, or on dying).
    escape: Option<escape::Menu>,
    /// The option menu's audio settings, while open.
    audio_window: Option<options::AudioWindow>,
    graphics_window: Option<options::GraphicsWindow>,
    shortcuts_window: Option<options::ShortcutsWindow>,
    world_map: Option<worldmap::WorldMap>,
    /// The quest log and its window.
    quests: Vec<net::Quest>,
    quest_window: quest::QuestWindow,
    /// The achievements: the totals, the player's by ID, and their window.
    achievement_totals: net::AchievementTotals,
    achievements: HashMap<u32, net::Achievement>,
    achievement_window: achievement::AchievementWindow,
    /// The item description window, and how many lines its text wraps to.
    item_window: Option<iteminfo::ItemWindow>,
    item_lines: usize,
    /// A card's picture, opened from its info.
    card_view: Option<iteminfo::CardView>,
    /// Whispers sent and not yet answered: to whom, and what.
    whispered: VecDeque<(String, String)>,
    /// Names whose whispers we asked to block or allow, not yet answered (the answer doesn't name them).
    whisper_asked: VecDeque<String>,
    /// A list of carried items to pick from, and what the pick is for.
    pick: Option<(pick_list::PickList, Picking)>,
    /// The card whose list was asked for (the list doesn't name it).
    card_asked: Option<u16>,
    /// A skill an item cast that needs aiming (the player may not know it).
    item_skill: Option<net::Skill>,
    /// The deal under way; someone asking for one (name, character ID, level); whom we asked.
    trade: Option<trade::Trade>,
    refine: Option<refine::Refine>,
    /// The grade enchant window.
    grade: Option<grade::GradeEnchant>,
    /// Laphine synthesis and upgrade, open until their close packet or a map change.
    synthesis: Option<laphine::Synthesis>,
    upgrade: Option<laphine::Upgrade>,
    /// The item reform window (a reform box or an NPC).
    reform: Option<reform::Reform>,
    /// The enchant window (perfect, random, upgrade, reset).
    enchant: Option<enchant::Enchant>,
    /// The stylist (`@stylist`). The preview is its own figure; the player's look waits for the server.
    stylist: Option<stylist::Window>,
    /// The material window (Change Material, Four Spirit Analysis).
    materials: Option<material::Materials>,
    /// The Lucky Roulette's window.
    roulette: Option<roulette::Roulette>,
    /// Sense's window, and the map's boss as the Convex Mirror shows it (name and cell).
    monster_window: Option<monster_info::MonsterWindow>,
    boss: Option<(String, [i32; 2])>,
    viewing: Option<Viewed>,
    bank: Option<bank::Bank>,
    cashshop: Option<cashshop::CashShop>,
    cashshop_lists: Vec<Vec<(u32, u32)>>,
    cashshop_list_requested: bool,
    cashshop_buy_asked: bool,
    trade_asked: Option<(String, u32, u16)>,
    trade_with: String,
    /// The status window, and its height (its background's).
    status_window: status::StatusWindow,
    status_height: f32,
    /// The party, its window, and an invitation waiting for an answer (party ID and name).
    party: Option<party::Party>,
    party_window: party::PartyWindow,
    /// Members dragged out of the party window: a small HP window each, until it's closed, they leave, or logout.
    member_windows: party::MemberWindows,
    party_invite: Option<(u32, String)>,
    /// The partner's name, as the last I Miss You told it (while the player has the ring's skill).
    partner: Option<String>,
    /// Being asked to be adopted: by whom (parent, partner accounts; the name), and whether it's the second question.
    adopt_asked: Option<(u32, u32, String, bool)>,
    /// Setting up our shop, and our open shop.
    vend_setup: Option<vend::Setup>,
    my_shop: Option<vend::MyShop>,
    /// Setting up our buying store, and our open buying store.
    buying_setup: Option<buying::Setup>,
    my_buying_store: Option<buying::MyStore>,
    /// RODEX: the mailbox, a mail read and one written.
    mail: mail::Mail,
    /// The cart (shown like the inventory, once the server says there is one).
    cart: inventory::Inventory,
    /// The guild we're in, its window, and a guild inviting us (ID and name).
    guild: Option<guild::Guild>,
    guild_window: guild::GuildWindow,
    guild_invite: Option<(u32, String)>,
    /// A guild question: another guild asks for an alliance, or the master is about to end a relation.
    guild_asked: Option<GuildAsked>,
    /// Dev switch RUSTNAROK_CLIENT_GUILD=window ran.
    guild_shown: bool,
    /// The clan we're in, and its window.
    clan: Option<clan::Clan>,
    clan_window: clan::ClanWindow,
    /// Guild emblems by guild ID: the version last asked for, and the image that came.
    emblems: HashMap<u32, (u32, Option<Texture>)>,
    /// What shows with players' names (party, guild, position) by account ID, and the unit last pointed at: a
    /// player's tag is asked for each time the pointer comes onto them, as what the server sends unasked can be stale.
    name_tags: HashMap<u32, net::NameTag>,
    pointed: Option<u32>,
    /// The friend list, and someone asking to be friends (account, character, name).
    friends: Vec<net::Friend>,
    friend_asked: Option<(u32, u32, String)>,
    /// The server asks whether to use this voucher to grow the inventory.
    expansion_asked: Option<u32>,
    /// The item the player last put in or took out of the Swap Equipment set: its answer gets a line.
    switch_asked: Option<u16>,
    /// When something was last asked to be worn.
    last_wear: Option<Instant>,
    /// A bind-on-equip item (inventory index) waiting for OK to be worn; the broken items' indexes before the
    /// inventory came again (None: there was none yet, at login).
    bind_asked: Option<u16>,
    broken_before: Option<Vec<u16>>,
    /// The player's pet: its unit (once the server says it's ours), what it's like, its window, and a question about
    /// it waiting for OK.
    pet_id: Option<u32>,
    pet: Option<net::Pet>,
    pet_window: pet::PetWindow,
    pet_asked: Option<PetAsked>,
    /// The player's mercenary, its skills, its window and skill list, how it's steered, and whether dismissing it is
    /// being asked.
    mercenary: Option<net::Mercenary>,
    /// The ranking board while it's open, characters' names by ID as the server told them (None: it has none), and
    /// the names asked for.
    ranking: Option<ranking::Board>,
    /// Reputation points by reputation type, as the server last sent them, and the reputation window while it's open.
    reputation: HashMap<u64, i64>,
    reputation_window: Option<reputation::Window>,
    /// The navigation under way, and the navigation window while it's open.
    navigation: Option<navigation::Active>,
    navi_window: Option<navigation::Window>,
    /// What was just picked up, for five seconds; and whether anything was since the inventory was last open.
    pickup_popup: Option<pickup::Popup>,
    new_items: bool,
    /// The level up buttons in the bottom corners.
    level_ups: level_up::LevelUps,
    /// The server's overweight point, in percent of the weight limit.
    overweight: u32,
    /// A new quest's icon (until the quest window opens), and hunts' progress banners.
    quest_icon: bool,
    banners: quest_notice::Banners,
    /// A skill's description, while it shows.
    skill_description: Option<skill_descript::SkillDescription>,
    /// The Universal Catalog's store search while it's open.
    catalog: Option<catalog::Catalog>,
    char_names: HashMap<u32, Option<String>>,
    names_asked: HashSet<u32>,
    /// The dress room's lists, the room while it's open, and its figure.
    dress_lists: dressroom::Lists,
    dressroom: Option<dressroom::Room>,
    dress_figure: Option<SpriteSet>,
    /// The emblems drawn over heads this frame: whose, their guild, the box (for the test driver).
    head_emblems: Vec<(String, u32, [f32; 4])>,
    /// The bars drawn under units this frame: whose ("player", a summon's kind, else the unit ID), the box, each row's
    /// fill (for the test driver).
    bars_drawn: Vec<(String, [f32; 4], Vec<Option<f32>>)>,
    /// /noshift: a click on a player attacks without Shift (saved).
    no_shift: bool,
    /// /noctrl: a click on a monster keeps attacking without Ctrl (saved; on unless turned off).
    no_ctrl: bool,
    /// /snap, /itemsnap, /skillsnap (saved; only /skillsnap on unless changed).
    snaps: Snaps,
    /// Where the cursor is drawn: the pointer, or the middle of what it snapped to.
    cursor_at: [f32; 2],
    /// /camera: the camera sticks to the player (saved; off unless turned on); the earthquake's shake this frame.
    camera_snap: bool,
    quake_flip: bool,
    /// The last skill sent by aiming: ID, level and target.
    skill_sent: Option<(u16, u16, SkillTarget)>,
    /// The attack asked for with the mouse, and when the last attack packet went out; riding a mount that can't
    /// attack.
    attack: Option<Attack>,
    last_attack: Option<Instant>,
    riding: bool,
    /// ChangeChatMode (/bm): the chat line stays closed and letter keys are shortcuts.
    chat_mode: battle_mode::Mode,
    /// The map's property and flags (net::MAP_*), and its type, as the server last sent them.
    map_property: Option<(u16, u32)>,
    map_type: Option<u16>,
    /// The battleground queue's window, the battleground queued for (and how many wait), and its question up.
    queue_window: battle_queue::QueueWindow,
    queued: Option<(String, Option<u32>)>,
    queue_asked: Option<QueueAsked>,
    /// The battleground's camps, score, teammates' dots and battle chat; forgotten on a map change.
    battle: battleground::Battle,
    /// The minimap dot for teammates, and the marks over heads: our camp's, the other's.
    camp_looks: Option<(Texture, [Texture; 2])>,
    /// The player's PvP rank and the players on the map, while the server counts them.
    pvp_rank: Option<(u32, u32)>,
    /// The counter's digits (rankfont), the clock's (timefont) and the first-place banner (timeattack), loaded when first
    /// shown.
    rank_font: Option<Option<SpriteSet>>,
    time_font: Option<Option<SpriteSet>>,
    time_attack: Option<Option<SpriteSet>>,
    /// Our last rank (0 before any), and since when we've held first place.
    last_rank: u32,
    first_since: Option<Instant>,
    /// Players ranked 1 to 10 (by account): they carry the top-ten aura.
    top_ten: HashSet<u32>,
    /// Server announcements in the bar across the top.
    announcements: announce::Bar,
    /// The names whose whispers (and speaker announcements) the player blocks, as far as the server has said.
    blocked: HashSet<String>,
    /// Whether the player refuses party invitations (None until the server says).
    refusing_party: Option<bool>,
    /// The party setup window, and the member whose leadership is being asked about.
    party_setup: Option<party::Setup>,
    leader_asked: Option<u32>,
    /// 1:1 whisper windows, when an incoming whisper opens one, the friend setup, the block list and the away message.
    talks: whisper::Talks,
    whisper_open: whisper::OpenSettings,
    friend_setup: Option<whisper::FriendSetup>,
    block_list: Option<whisper::BlockList>,
    away: whisper::Away,
    away_prompt: Option<whisper::AwayPrompt>,
    mercenary_skills: Vec<net::Skill>,
    mercenary_window: mercenary::Window,
    mercenary_skill_list: mercenary::SkillList,
    steering: mercenary::Steering,
    dismiss_asked: bool,
    /// The player's homunculus: its unit (once the server says it's ours), what it's like, its skills, its windows,
    /// how it's steered, the name being asked about, and its hunger before the last update.
    homunculus: Option<net::Homunculus>,
    homunculus_id: Option<u32>,
    homunculus_skills: Vec<net::Skill>,
    homunculus_window: homunculus::Window,
    homunculus_skill_list: homunculus::SkillList,
    homunculus_steering: mercenary::Steering,
    homunculus_rename: Option<String>,
    homunculus_hunger: Option<u16>,
    /// The player's elemental: its unit, HP, max HP, SP and max SP.
    elemental: Option<(u32, [u32; 4])>,
    /// Chat rooms over their owners' heads (by owner), and where their signs were last drawn (room ID, password).
    signs: HashMap<u32, chatroom::Sign>,
    sign_rects: Vec<([f32; 4], u32, u8)>,
    /// Item links as last drawn, their boxes and items: in the chat log (under the windows) and in the NPC window
    /// (over them).
    chat_links: Vec<([f32; 4], links::Target)>,
    dialog_links: Vec<([f32; 4], links::Target)>,
    /// The room we're in, and the window making one.
    room: Option<chatroom::Room>,
    room_make: Option<chatroom::Make>,
    /// The emotion list (Alt+L), when open.
    emotion_list: Option<emotion_list::List>,
    /// The macro window (Alt+M), when open, and the ten lines Alt+1 to Alt+0 send.
    macro_window: Option<macros::Window>,
    macro_lines: [String; macros::COUNT],
    /// Dev switch RUSTNAROK_CLIENT_ROOM ran.
    room_tried: bool,
    /// The map's weather, from the server's weather effects until the map changes.
    weather: weather::Weather,
    /// The right-click menu, and where a right press began (and on whom).
    menu: Option<menu::Menu<Choice>>,
    right_press: Option<([f32; 2], Option<u32>)>,
    /// The pointer moved since the right button went down: no menu on release.
    right_moved: bool,
    /// A monster marked for a companion (the companion, the monster): a second Alt press on it sends the attack.
    companion_mark: Option<(u32, u32)>,
    /// Open windows from back to front.
    windows: Vec<Panel>,
    /// Windows Esc closes, newest first. Pushed on open and on a press, dropped on close.
    esc_order: Vec<escape::WindowId>,
    /// The save-point question (messages[1548]) is up.
    save_asked: bool,
    /// The skill window's Apply asks first (`skills::APPLY_QUESTION`).
    apply_asked: bool,
    damages: Vec<Damage>,
    /// The player's own status values (HP, SP, ...) by rathena SP_* ID.
    status: HashMap<u16, i64>,
    /// Monsters' HP, when the server shares it: (hp, max hp).
    unit_hp: HashMap<u32, (i32, i32)>,
    /// Dev switches: an NPC name to talk to, "next" presses left, and whether to attack the nearest monster.
    auto_talk: Option<String>,
    auto_next: u32,
    auto_menu: Vec<usize>,
    auto_store: Option<String>,
    auto_attack: bool,
    auto_pick_up: bool,
    auto_wear: bool,
    /// Dev switch: hotkey slots to press and when (after entering).
    auto_keys: Vec<(Duration, usize)>,
    /// Dev switch: a skill and level to use 3 seconds in (aimed at oneself if it needs a target).
    auto_skill: Option<(u16, u16)>,
    /// The player's attack reach in cells.
    attack_range: i32,
    /// What to do once the walk toward it ends (and when it ended), and where its walk went and when (None: none yet).
    pending: Option<(Pending, Option<Instant>)>,
    pending_walk: Option<([i32; 2], Instant)>,
}

/// Where the camera eases toward, steered with the right mouse button and the wheel.
struct CameraGoal {
    /// Degrees, clockwise.
    yaw: f32,
    /// Degrees above the horizon.
    pitch: f32,
    /// Zoom units: 125 shows 62.5 cells.
    zoom: f32,
    indoor: bool,
    /// The right button is down: where the pointer was last.
    dragging: Option<[f32; 2]>,
    /// The last right press: when, where, and whether the pointer moved off since.
    last_press: Option<(Instant, [f32; 2], bool)>,
    /// The pointer's vertical movement this frame with the right button down, and whether Ctrl and Shift were held:
    /// tilt and zoom are applied once a frame (`drag_frame`).
    dragged: (f32, bool, bool),
    last_wheel: Option<Instant>,
    /// The map's own camera table, which then sets the limits (and no double click resets).
    limits: Option<rustnarok_client::map::Viewpoint>,
    /// /zoom: outdoors the camera goes out to ZOOM_OUT_DISTANCE.
    zoom_out: bool,
}

impl CameraGoal {
    #[cfg(test)]
    fn new(indoor: bool) -> Self {
        Self::arrive(indoor, None, [PITCH, ZOOM * UNITS_PER_ZOOM], false)
    }

    /// The goal on arriving on a map: the map's own table's start, or the tilt and distance the player left on this
    /// kind of map (within its limits); turned 0 outdoors, YAW_INDOOR indoors.
    fn arrive(indoor: bool, limits: Option<rustnarok_client::map::Viewpoint>, saved: [f32; 2], zoom_out: bool) -> Self {
        let mut goal = Self {
            yaw: if indoor { YAW_INDOOR } else { 0.0 },
            pitch: PITCH,
            zoom: ZOOM,
            indoor,
            dragging: None,
            last_press: None,
            dragged: (0.0, false, false),
            last_wheel: None,
            limits,
            zoom_out,
        };
        match limits {
            Some(table) => {
                goal.yaw = table.yaw_in.clamp(table.yaw[0], table.yaw[1]);
                goal.pitch = table.pitch_in.clamp(table.pitch[0], table.pitch[1]);
                goal.set_distance(table.distance_in);
            }
            None => {
                // The widest tilt each kind of map allows: with Ctrl outdoors, 35 to 55 indoors.
                let [low, high] = if indoor { [35.0, 55.0] } else { [25.0, 65.0] };
                goal.pitch = saved[0].clamp(low, high);
                goal.set_distance(saved[1]);
            }
        }
        // RUSTNAROK_CLIENT_CAMERA holds on every map, warps included.
        if let Some([yaw, pitch, zoom]) = std::env::var("RUSTNAROK_CLIENT_CAMERA").ok().and_then(|camera| {
            let values: Vec<f32> = camera.split(',').filter_map(|value| value.parse().ok()).collect();
            <[f32; 3]>::try_from(values).ok()
        }) {
            (goal.yaw, goal.pitch, goal.zoom) = (yaw, pitch, zoom);
        }
        goal
    }

    /// The camera already in place, for a fresh map.
    fn camera(&self, target: world::Vector) -> Camera {
        Camera { target, distance: self.zoom * UNITS_PER_ZOOM, pitch: self.pitch.to_radians(), yaw: self.yaw.to_radians(), ground: f32::MIN }
    }

    /// The distance to the player in world units.
    fn distance(&self) -> f32 {
        self.zoom * UNITS_PER_ZOOM
    }

    /// Moves the camera to `distance` world units, within the map's range: its own table's, or the kind of map's
    /// (further out outdoors with /zoom).
    fn set_distance(&mut self, distance: f32) {
        let [near, far] = match self.limits {
            Some(table) => table.distance,
            None if self.indoor => DISTANCE_RANGE_INDOOR,
            None if self.zoom_out => [DISTANCE_RANGE[0], ZOOM_OUT_DISTANCE],
            None => DISTANCE_RANGE,
        };
        self.zoom = distance.clamp(near, far) / UNITS_PER_ZOOM;
    }

    /// How far the camera tilts now: the map's table, or `pitch_range`.
    fn pitch_limits(&self, ctrl: bool) -> [f32; 2] {
        self.limits.map_or(pitch_range(self.indoor, ctrl), |table| table.pitch)
    }

    /// Right button down. A second press within CAMERA_DOUBLE_CLICK with the pointer not moved at all resets: the
    /// distance with Ctrl, the tilt with Shift, otherwise (outdoors only) the turn, to the nearest whole circle.
    fn press(&mut self, cursor: [f32; 2], now: Instant, ctrl: bool, shift: bool) {
        let double = self.last_press.is_some_and(|(at, _, moved)| !moved && now.saturating_duration_since(at) < CAMERA_DOUBLE_CLICK);
        if self.limits.is_none() && double {
            if ctrl {
                self.zoom = ZOOM;
            }
            if shift {
                self.pitch = PITCH_RESET;
            }
            if !ctrl && !shift && !self.indoor {
                self.yaw = (self.yaw / 360.0).round() * 360.0;
            }
        }
        // After a double click the next press starts a new pair.
        self.last_press = (!double).then_some((now, cursor, false));
        self.dragging = Some(cursor);
    }

    /// The pointer moved to `cursor`: off the last press, the next right press isn't a double click.
    fn pointer_moved(&mut self, cursor: [f32; 2]) {
        if let Some((_, from, moved)) = &mut self.last_press {
            *moved |= moved_off(*from, cursor);
        }
    }

    /// Dragging with the right button down (see `steer`).
    fn drag(&mut self, cursor: [f32; 2], modifiers: ModifiersState) {
        // Only while the right button is down: `replace` would start a drag on any move.
        let Some(last) = self.dragging else { return };
        self.dragging = Some(cursor);
        self.steer(cursor[0] - last[0], cursor[1] - last[1], modifiers.control_key(), modifiers.shift_key(), modifiers.alt_key());
    }

    /// A drag of `dx`, `dy` pixels: turns a degree a pixel (indoors within YAW_RANGE_INDOOR), except with Shift. With
    /// Shift it tilts and with Ctrl it zooms, once a frame (`drag_frame`). Ctrl turns too; Alt does nothing.
    fn steer(&mut self, dx: f32, dy: f32, ctrl: bool, shift: bool, alt: bool) {
        if alt {
            return;
        }
        if shift || ctrl {
            self.dragged = (self.dragged.0 + dy, ctrl, shift);
        }
        if !shift {
            self.yaw += dx;
            if let Some(table) = self.limits {
                self.yaw = self.yaw.clamp(table.yaw[0], table.yaw[1]);
            } else if self.indoor {
                self.yaw = self.yaw.clamp(YAW_RANGE_INDOOR[0], YAW_RANGE_INDOOR[1]);
            }
        }
    }

    /// The frame's tilt or zoom drag: half a degree or unit a pixel of the frame's vertical movement, whole steps only
    /// (the rest is dropped, so a very slow drag does nothing). Dragging down tilts down and comes closer.
    fn drag_frame(&mut self) {
        let (dy, ctrl, shift) = std::mem::replace(&mut self.dragged, (0.0, false, false));
        let steps = (dy / 2.0).trunc();
        if shift {
            let [low, high] = self.pitch_limits(ctrl);
            self.pitch = (self.pitch + steps).clamp(low, high);
        }
        if ctrl {
            self.set_distance(self.distance() - steps);
        }
    }

    /// A wheel turn (`rows` > 0: toward the user), counted at most once per WHEEL_GAP: one WHEEL_STEP however far
    /// it turned, away from the user farther; with Shift it tilts instead (away from the user flatter).
    fn wheel(&mut self, rows: i32, now: Instant, ctrl: bool, shift: bool) {
        if rows == 0 || self.last_wheel.is_some_and(|last| now.saturating_duration_since(last) < WHEEL_GAP) {
            return;
        }
        self.last_wheel = Some(now);
        let toward = rows.signum() as f32;
        if shift {
            let [low, high] = self.pitch_limits(ctrl);
            self.pitch = (self.pitch + toward * WHEEL_TILT).clamp(low, high);
        } else {
            self.set_distance(self.distance() - toward * WHEEL_STEP);
        }
    }

    /// Moves the camera FOLLOW of the way per 60th of a second to the goal and its look-at point to the player's feet
    /// (`snap`: at once); it turns the short way round.
    fn ease(&self, camera: &mut Camera, elapsed: Duration, feet: world::Vector, snap: bool) {
        let step = follow_step(elapsed);
        camera.target = if snap { feet } else { [0, 1, 2].map(|axis| camera.target[axis] + (feet[axis] - camera.target[axis]) * step) };
        let yaw = camera.yaw.to_degrees();
        let turn = (self.yaw - yaw + 180.0).rem_euclid(360.0) - 180.0;
        camera.yaw = (yaw + turn * step).to_radians();
        camera.pitch += (self.pitch.to_radians() - camera.pitch) * step;
        camera.distance += (self.distance() - camera.distance) * step;
    }
}

/// The camera moves this share of the way each 60th of a second.
const FOLLOW: f32 = 0.1;
/// A view changed is written this long after it last changed.
const VIEW_SAVE_AFTER: Duration = Duration::from_millis(500);

/// How much of the way the camera moves in `elapsed`: FOLLOW per 60th of a second, the same at any frame rate.
fn follow_step(elapsed: Duration) -> f32 {
    1.0 - (1.0 - FOLLOW).powf(elapsed.as_secs_f32() * 60.0)
}

/// The tilt and distance the player left the camera at, outdoors and indoors: kept between maps, and (for a client
/// the harness doesn't drive) saved.
#[derive(Clone, Copy, Debug, PartialEq)]
struct SavedView {
    views: [[f32; 2]; 2],
    changed: Option<Instant>,
}

/// A first run's view: outdoors 400 units away, indoors 300, tilted 45.
const FIRST_DISTANCE: [f32; 2] = [400.0, 300.0];

impl Default for SavedView {
    fn default() -> Self {
        Self { views: FIRST_DISTANCE.map(|distance| [PITCH, distance]), changed: None }
    }
}

impl SavedView {
    /// The view a client the harness drives starts from, every run: 300 units both ways.
    // ponytail: the scenarios' fixed pixel clicks were placed at 300; a driven client keeps it outdoors too.
    fn harness() -> Self {
        Self { views: [[PITCH, ZOOM * UNITS_PER_ZOOM]; 2], changed: None }
    }

    /// From the saved text: outdoor tilt and distance, then indoor; anything missing stays the default.
    fn parse(text: &str) -> Self {
        let mut saved = Self::default();
        let values: Vec<f32> = text.split_whitespace().filter_map(|value| value.parse().ok()).collect();
        if let [outdoor_pitch, outdoor_distance, indoor_pitch, indoor_distance] = values[..] {
            saved.views = [[outdoor_pitch, outdoor_distance], [indoor_pitch, indoor_distance]];
        }
        saved
    }

    fn text(&self) -> String {
        let [[a, b], [c, d]] = self.views;
        format!("{a} {b} {c} {d}")
    }

    fn of(&self, indoor: bool) -> [f32; 2] {
        self.views[usize::from(indoor)]
    }

    /// Follows the goal: a changed tilt or distance is kept (not on a map with its own table), and written once it
    /// has held still VIEW_SAVE_AFTER, when `save`.
    fn follow(&mut self, goal: &CameraGoal, now: Instant, save: bool) {
        if goal.limits.is_none() {
            let view = [goal.pitch, goal.distance()];
            let slot = &mut self.views[usize::from(goal.indoor)];
            if *slot != view {
                *slot = view;
                self.changed = Some(now);
            }
        }
        if self.changed.is_some_and(|at| now.saturating_duration_since(at) >= VIEW_SAVE_AFTER) {
            self.changed = None;
            if save && let Some(path) = settings_path("camera") {
                let _ = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(path, self.text()));
            }
        }
    }
}

/// The sprite direction to draw for a unit facing `direction`, seen through a camera turned `yaw` radians.
fn seen_direction(rot: f32, yaw: f32, round: bool) -> u8 {
    actor::view_direction(rot, yaw.to_degrees(), round)
}

/// Whether a unit rounds the camera's angle (to the nearest direction) rather than flooring it: players standing,
/// walking or sitting; NPCs (jobs 46 to 999) standing; jobs 6018 to 6052 standing, walking or sitting. Monsters,
/// and players in any other action, floor.
// ponytail: the battle stance counts as standing.
fn rounds_facing(kind: u8, job: u16, action: usize) -> bool {
    let still_or_walking = matches!(action, actor::IDLE | actor::WALK | actor::SIT | actor::READY);
    match kind {
        PLAYER => still_or_walking,
        _ if (6018..=6052).contains(&job) => still_or_walking,
        NPC => (46..=999).contains(&job) && action == actor::IDLE,
        _ => false,
    }
}

/// Ground position of a companion at world XZ, lifted by `lift` (the falcon's 25).
fn companion_feet(world: &World, at: [f32; 2], lift: f32) -> [f32; 3] {
    let mut feet = world.cell_position([at[0] / world::CELL - 0.5, at[1] / world::CELL - 0.5]);
    feet[1] += lift;
    feet
}

/// What shows in front of its owner (spirit spheres on the near side, emotions) steps nearer than any of its images.
const IN_FRONT: f32 = 100.0;

/// The depth layer of a sprite's `index`th image: a player's all share the body's plane, each a step nearer; other
/// sprites' each sit a layer nearer.
fn sprite_layer(index: usize, player: bool, flat: bool) -> DepthLayer {
    if player { DepthLayer { index: 0.0, nudge: index as f32, flat } } else { DepthLayer { index: index as f32, nudge: 0.0, flat } }
}

struct CompanionDraw {
    owner: Owner,
    warg: bool,
    feet: [f32; 3],
    placed: Vec<actor::Placed>,
}

fn push_companion(companion: &mut actor::Companion, warg: bool, owner: Owner, gpu: &Gpu, world: &World, yaw: f32, now: Instant, out: &mut Vec<CompanionDraw>) {
    let lift = if warg { 0.0 } else { actor::FALCON_HEIGHT };
    let placed = companion.frame(gpu, seen_direction(actor::rot_of(companion.direction), yaw, false), now, !warg);
    if placed.is_empty() {
        return;
    }
    out.push(CompanionDraw { owner, warg, feet: companion_feet(world, companion.at, lift), placed });
}

/// A pet: its monster's sprite, moving by its accessory's ACT when it wears one the data has.
fn pet_sprites(grf: &Grf, name: &str, act: Option<&String>) -> Option<SpriteSet> {
    let worn = act.and_then(|act| SpriteSet::with_act(grf, &format!("data\\sprite\\몬스터\\{name}"), &format!("data\\sprite\\몬스터\\{act}")));
    worn.or_else(|| SpriteSet::single(grf, name, MONSTER))
}

/// The titles of the achievements done whose rewards were taken, by ID, with their names.
fn earned_titles(achievements: &HashMap<u32, net::Achievement>, info: &HashMap<u32, lua::AchievementInfo>, names: &HashMap<u32, String>) -> Vec<(u32, String)> {
    let mut titles: Vec<u32> = achievements
        .values()
        .filter(|achievement| achievement.completed && achievement.rewarded)
        .filter_map(|achievement| info.get(&achievement.id).map(|info| info.reward_title).filter(|&title| title != 0))
        .collect();
    titles.sort_unstable();
    titles.dedup();
    titles.into_iter().map(|title| (title, names.get(&title).cloned().unwrap_or_else(|| title.to_string()))).collect()
}

fn character_look(character: &Character) -> Look {
    Look {
        job: character.job,
        sex: character.sex,
        hair: character.hair,
        hair_color: character.hair_color,
        clothes_color: character.clothes_color,
        weapon: u32::from(character.weapon),
        shield: u32::from(character.shield),
        head_top: character.head_top,
        head_mid: character.head_mid,
        head_bottom: character.head_bottom,
        robe: character.robe,
        option: character.option & actor::BODY_OPTIONS,
    }
}

fn unit_look(unit: &net::Unit) -> Look {
    Look {
        job: unit.job,
        sex: unit.sex,
        hair: unit.hair,
        hair_color: unit.hair_color,
        clothes_color: unit.clothes_color,
        weapon: unit.weapon,
        shield: unit.shield,
        head_top: unit.head_top,
        head_mid: unit.head_mid,
        head_bottom: unit.head_bottom,
        robe: unit.robe,
        option: unit.option & actor::BODY_OPTIONS,
    }
}

/// Applies a ZC_SPRITE_CHANGE to a look; returns whether it changed anything drawn.
fn change_look(look: &mut Look, kind: u8, value: u32, extra: u32) -> bool {
    let before = look.clone();
    let short = value as u16;
    match kind {
        net::LOOK_JOB => look.job = short,
        net::LOOK_HAIR => look.hair = short,
        net::LOOK_WEAPON => (look.weapon, look.shield) = (value, extra),
        net::LOOK_HEAD_BOTTOM => look.head_bottom = short,
        net::LOOK_HEAD_TOP => look.head_top = short,
        net::LOOK_HEAD_MID => look.head_mid = short,
        net::LOOK_HAIR_COLOR => look.hair_color = short,
        net::LOOK_CLOTHES_COLOR => look.clothes_color = short,
        net::LOOK_SHIELD => look.shield = value,
        net::LOOK_ROBE => look.robe = short,
        _ => {}
    }
    *look != before
}

/// A map name as the indoor table lists it.
/// An NPC's mark on the minimap: (NPC, mark ID), cell, image, until when (None while the NPC keeps it), and when it was set.
type Mark = ((u32, u8), [i32; 2], Texture, Option<Instant>, Instant);
/// The Universal Catalog's mark, apart from the NPCs'.
const CATALOG_MARK: (u32, u8) = (u32::MAX, 0);

/// A scripted input (RUSTNAROK_CLIENT_CLICKS): when, right button, where, and where to drag to.
type ScriptedInput = (Duration, bool, [f32; 2], Option<[f32; 2]>);

fn map_key(map: &str) -> String {
    map.trim_end_matches(".gat").trim_end_matches(".rsw").to_lowercase()
}

/// A panel Esc closes. The hotbar and the basic info window stay up.
fn esc_panel(id: Panel) -> Option<escape::WindowId> {
    use escape::WindowId::*;
    Some(match id {
        Panel::Hotkeys | Panel::BasicInfo => return None,
        Panel::Inventory => Inventory,
        Panel::Equipment => Equipment,
        Panel::Skills => Skills,
        Panel::Storage => Storage,
        Panel::Party => Party,
        Panel::Status => Status,
        Panel::Guild => Guild,
        Panel::Clan => Clan,
        Panel::Cart => Cart,
        Panel::Pet => Pet,
        Panel::Mercenary => Mercenary,
        Panel::MercenarySkills => MercenarySkills,
        Panel::Homunculus => Homunculus,
        Panel::HomunculusSkills => HomunculusSkills,
    })
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Panel {
    Hotkeys,
    BasicInfo,
    Inventory,
    Equipment,
    Skills,
    Storage,
    Party,
    Status,
    Guild,
    Clan,
    Cart,
    Pet,
    Mercenary,
    MercenarySkills,
    Homunculus,
    HomunculusSkills,
}

/// What the box asking how many is for.
#[derive(Clone, Copy)]
enum Asked {
    /// A row of the shop, into the deal.
    Shop(usize),
    /// A row of the shop's deal, out of it.
    ShopOut(usize),
    /// An inventory item, into storage.
    Store(u16),
    /// An inventory item, onto the ground.
    Drop(u16),
    /// A stored item, out.
    Take(u16),
    /// An inventory item, into the deal.
    Trade(u16),
    /// The price of a cart item going on sale.
    VendPrice(u16),
    /// The price, then the quantity, of an inventory item for a buying store: `(index, item_id)`.
    BuyingPrice(u16, u32),
    BuyingAmount(u16),
    /// An inventory item into the cart, and a cart item out.
    ToCart(u16),
    FromCart(u16),
    /// The new party's name.
    PartyName,
    /// Whom to invite into the party.
    Invite,
    /// The password of a chat room, by ID.
    RoomPassword(u32),
    /// An inventory item, into the mail being written.
    Mail(u16),
    /// An inventory item, into the material window.
    Material(u16),
    /// The pet's new name.
    PetName,
    /// A guild position's new title, and its tax (the master's).
    PositionTitle(u32),
    PositionTax(u32),
    /// The homunculus's new name.
    HomunculusName,
    /// A Talkie Box's message, before it's placed at the cell.
    TalkieBox {
        level: u16,
        cell: [i32; 2],
    },
}

impl Asked {
    /// A count of items to move, not a name or a price.
    fn moves_items(&self) -> bool {
        matches!(
            self,
            Asked::Shop(_)
                | Asked::ShopOut(_)
                | Asked::Store(_)
                | Asked::Drop(_)
                | Asked::Take(_)
                | Asked::Trade(_)
                | Asked::ToCart(_)
                | Asked::FromCart(_)
                | Asked::Mail(_)
                | Asked::Material(_)
        )
    }
}

/// What the right-click menu offers.
#[derive(Clone, Copy, PartialEq)]
enum Choice {
    /// Ask this player (by account ID) for a deal.
    Trade(u32),
    /// Invite them into the party.
    Invite(u32),
    /// Ask them to be friends.
    Befriend(u32),
    /// Invite them into the guild.
    GuildInvite(u32),
    /// Ask their guild for an alliance, or set it as an antagonist.
    Ally(u32),
    Antagonist(u32),
    /// See what they wear.
    ViewEquipment(u32),
    /// A command to the player's pet (net::PET_INFO and on).
    Pet(u8),
    /// Hand the party to this member (by account).
    Leader(u32),
    /// The room's owner hands the room to this member, or kicks them out (by their row in the member list).
    RoomOwner(usize),
    RoomKick(usize),
    /// A tracked quest: its details in the quest window, or put it aside.
    QuestInfo(u32),
    QuestAside(u32),
    /// Ask this party member (by account) to be adopted.
    Adopt(u32),
    /// The guild master moves a member (by its place in the member list) to a position.
    GuildMove(usize, u32),
    /// Leave the guild (asks first).
    GuildLeave,
}

/// A battleground queue question waiting for OK: applying (how, for which), leaving the queue, or entering the
/// battle (until the time to answer runs out).
enum QueueAsked {
    Apply(u16, String),
    Cancel(String),
    Lobby(String, Instant),
}

/// A pet question waiting for OK: feed it, or rename it (to this).
enum PetAsked {
    Feed,
    Rename(String),
}

/// Another player's equipment window: the window, what they wear, and their figure for its preview.
struct Viewed {
    window: equipment::Equipment,
    items: Vec<net::InventoryItem>,
    sprites: Option<SpriteSet>,
}

/// A map about to load behind the loading screen.
enum LoadTarget {
    Enter { position: [u16; 2], direction: u8 },
    Change { map: String, position: [u16; 2] },
}

struct Loading {
    image: Option<Texture>,
    /// Drawn once: now the map can load.
    shown: bool,
    target: LoadTarget,
}

/// An action waiting for the player to arrive.
#[derive(Clone, Copy, PartialEq)]
enum Pending {
    PickUp(u32),
    Skill { id: u16, level: u16, target: SkillTarget },
}

/// What a skill was aimed at.
#[derive(Clone, Copy, PartialEq)]
enum SkillTarget {
    Unit(u32),
    Cell([i32; 2]),
}

/// What's being dragged out of a window.
#[derive(Clone, Copy, PartialEq)]
enum Dragged {
    Item(u16),
    Skill(u16),
    /// A stored item, by storage index.
    Stored(u16),
    /// A cart item, by cart index.
    Carted(u16),
    /// What a hotkey slot holds, by slot.
    Hotkey(usize),
    /// A worn item, by inventory index, out of the equipment window.
    Worn(u16),
    /// A shop's offer (by row), or a row of its deal.
    ShopOffer(usize),
    ShopDeal(usize),
}

/// What a sprite on the map belongs to.
#[derive(Clone, Copy, PartialEq)]
enum Owner {
    Player,
    Unit(u32),
    Item(u32),
}

struct FloorItem {
    item: u32,
    identified: bool,
    amount: u16,
    /// The server's cell, and where in it the item lies (fractional cells).
    at: [i32; 2],
    cell: [f32; 2],
    /// When it appeared: it glints from then on.
    seen: Instant,
    /// Set while it falls after being dropped.
    dropped: Option<Instant>,
}

impl Game {
    fn dialog_for(&mut self, npc: u32) -> &mut Dialog {
        if self.dialog.as_ref().is_none_or(|dialog| dialog.npc != npc) {
            self.dialog = Some(Dialog { npc, lines: Vec::new(), button: DialogButton::None, menu: None, input: None, warp: None });
        }
        self.dialog.as_mut().unwrap()
    }

    /// The dialog's next/close button.
    fn press_dialog_button(&mut self) {
        let Some(dialog) = &mut self.dialog else { return };
        match dialog.button {
            DialogButton::Next => {
                let _ = self.session.next(dialog.npc);
                // The next page replaces the text.
                dialog.lines.clear();
                dialog.button = DialogButton::None;
            }
            DialogButton::Close => {
                let _ = self.session.close_dialog(dialog.npc);
                self.dialog = None;
            }
            DialogButton::None => {}
        }
    }

    /// Picks the highlighted menu item, or cancels the conversation.
    fn choose(&mut self, cancel: bool) {
        let Some(dialog) = &mut self.dialog else { return };
        let Some((_, selected)) = dialog.menu.take() else { return };
        // Warp Portal and Teleport: the map as it came; the row after the maps (Cancel) and Esc cancel.
        if let Some((skill, maps)) = dialog.warp.take() {
            let _ = self.session.select_warp_point(skill, maps.get(selected).filter(|_| !cancel).map_or("cancel", String::as_str));
            self.dialog = None;
            return;
        }
        let _ = self.session.choose(dialog.npc, if cancel { 255 } else { selected as u8 + 1 });
        if cancel {
            self.dialog = None;
        }
    }

    fn submit_input(&mut self) {
        let Some(dialog) = &mut self.dialog else { return };
        let Some((text, value)) = dialog.input.take() else { return };
        let _ = if text { self.session.input_text(dialog.npc, &value) } else { self.session.input_number(dialog.npc, value.trim().parse().unwrap_or(0)) };
    }

    /// Whether players win over monsters under the pointer: Shift held, or aiming a skill for players, but not both.
    // ponytail: "a skill for players" is one the server flags as for friends (Heal, Blessing); a skill table may list
    // more.
    /// Opens the dress room on the player's own look.
    fn open_dressroom(&mut self) {
        self.dressroom = Some(dressroom::Room::new(&character_look(&self.me), &self.dress_lists));
    }

    /// A click in the store search window.
    fn click_catalog(&mut self, hit: catalog::Hit, items: &HashMap<u32, ItemInfo>, messages: &[String]) {
        let Some(catalog) = &mut self.catalog else { return };
        match hit {
            catalog::Hit::Title(TitleHit::Close) | catalog::Hit::Close => self.close_catalog(),
            catalog::Hit::Title(title) => catalog.frame.click_title(title),
            catalog::Hit::Kind(buying) => catalog.buying = buying,
            catalog::Hit::Field(field) => catalog.focus = Some(field),
            catalog::Hit::Search => match catalog::search(&catalog.name, &catalog.min, &catalog.max, items) {
                Ok((ids, max, min)) => {
                    let _ = self.session.search_stores(catalog.buying, max, min, &ids, &[]);
                }
                Err(id) => self.chat_line(messages.get(id).cloned().unwrap_or_default(), CHAT_ERROR),
            },
            catalog::Hit::Next => {
                let _ = self.session.next_store_page();
            }
            catalog::Hit::Row(index) => {
                if catalog.click_row(index) {
                    let row = &catalog.rows[index];
                    let _ = self.session.pick_store(row.account, row.store, row.item);
                }
            }
            catalog::Hit::Window => catalog.focus = None,
        }
    }

    /// Closes the store search, telling the server (CZ_CLOSE_SEARCH_STORE_INFO).
    fn close_catalog(&mut self) {
        if self.catalog.take().is_some() {
            let _ = self.session.close_store_search();
            self.marks.retain(|(key, ..)| *key != CATALOG_MARK);
        }
    }

    /// Who the player sides with on this map, for `versus`: the map's flags, our party's online members, our guild and
    /// its allies.
    fn side(&self) -> versus::Side {
        let party = self.party.iter().flat_map(|party| &party.members).filter(|member| member.info.online).map(|member| member.info.account).collect();
        let flags = self.map_property.map_or(0, |(_, flags)| flags);
        let allies = self.guild.iter().flat_map(|guild| &guild.relations).filter(|relation| !relation.antagonist).map(|relation| relation.id).collect();
        versus::Side { flags, party, guild: self.guild.as_ref().map_or(0, |guild| guild.id), allies, camp: self.battle.camp(self.account_id) }
    }

    /// Another player as `versus` sees them.
    fn opponent(&self, id: u32) -> Option<versus::Other> {
        let actor = self.units.get(&id).filter(|actor| actor.kind == PLAYER)?;
        Some(versus::Other { account: id, guild: actor.guild, dead: actor.dead || actor.remove_at.is_some(), camp: self.battle.camps.get(&id).copied() })
    }

    fn players_first(&self, shift: bool) -> bool {
        let support =
            self.targeting.and_then(|(id, _)| self.skills.iter().find(|skill| skill.id == id)).is_some_and(|skill| skill.kind & net::Skill::FRIEND != 0);
        shift != support
    }

    /// A monster coming into view brings its HP bar if it's hurt, and loses it if not.
    fn monster_hp(&mut self, unit: &net::Unit) {
        if unit.kind != MONSTER {
            return;
        }
        match unit.hp {
            Some([hp, max]) => {
                self.unit_hp.insert(unit.id, (hp, max));
            }
            None => {
                self.unit_hp.remove(&unit.id);
            }
        }
    }

    /// Attacks `target` and keeps at it, walking into reach first.
    fn attack(&mut self, target: u32) {
        self.attack = Some(Attack::new(target, true));
        self.attack_step(Instant::now());
    }

    /// Moves the attack along: once in reach (or once a walk toward it has ended) the one attack packet goes out, no
    /// sooner than ATTACK_GAP after the last; out of reach, every CHASE_GAP a cell in reach is looked for, and walked
    /// to when it changed. A target gone (or dying) ends it; after the packet the server keeps at it (or swings once).
    fn attack_step(&mut self, now: Instant) {
        let Some(mut attack) = self.attack else { return };
        let Some(goal) = self.units.get(&attack.target).filter(|actor| actor.remove_at.is_none()).map(|actor| actor.cell) else {
            self.attack = None;
            return;
        };
        if attack.sent || self.store_open() {
            return;
        }
        let from = self.player.cell;
        if in_attack_reach(from, goal, self.attack_range) || (attack.chased && !self.player.moving()) {
            if may_attack(self.last_attack, now) {
                let _ = self.session.act(attack.target, attack.action());
                self.last_attack = Some(now);
                attack.sent = true;
            }
        } else if attack.last_chase.is_none_or(|last| now.saturating_duration_since(last) >= CHASE_GAP) {
            attack.last_chase = Some(now);
            let path = world::find_path(&self.world.gat, from, goal);
            let stop = path.and_then(|path| path.into_iter().find(|&cell| in_attack_reach(cell, goal, self.attack_range)));
            if let Some(stop) = stop.filter(|&stop| attack.chase_cell != Some(stop))
                && may_move(self.last_move, now)
            {
                self.last_move = Some(now);
                attack.chase_cell = Some(stop);
                attack.chased = true;
                let _ = self.session.walk(stop.map(|value| value as u16));
            }
        }
        // A one swing that went out is over; so is one let go of.
        self.attack = Some(attack).filter(|attack| !(attack.sent && !attack.continuous && !attack.held));
    }

    /// The cell under a point on screen, on the plane at the player's height.
    fn ground_cell(&self, cursor: [f32; 2]) -> Option<[i32; 2]> {
        let view = self.camera.view(screen()[0] / screen()[1]);
        view.pick_ground([cursor[0] / screen()[0], cursor[1] / screen()[1]], &self.world.gat)
    }

    /// Walks to a cell, or as close to it as can be walked (`world::walk_goal`: not blocked, nobody standing there, a
    /// path to it); nothing when the last walk went out less than MOVE_GAP ago. Whether a walk was sent.
    fn walk_toward(&mut self, cell: [i32; 2], now: Instant) -> bool {
        if self.player.swinging(control::now()) {
            self.walk_after_swing = Some(cell);
            return false;
        }
        if !may_move(self.last_move, now) {
            return false;
        }
        let from = self.player.cell;
        let taken: std::collections::HashSet<[i32; 2]> = self.units.values().filter(|actor| !actor.hidden).map(|actor| actor.cell).collect();
        let gat = &self.world.gat;
        let open = |cell: [i32; 2]| gat.walkable(cell) && !taken.contains(&cell) && world::find_path(gat, from, cell).is_some();
        let Some(goal) = world::walk_goal(cell, from, open) else { return false };
        self.pending = None;
        self.last_move = Some(now);
        self.walk_goal = Some(goal);
        self.walks += 1;
        let _ = self.session.walk(goal.map(|value| value as u16));
        true
    }

    /// Turns to face a cell, the head straight. rathena counts directions the other way along y from the sprites.
    /// Only standing, sitting or flinching; see `turn_toward`.
    fn turn_to(&mut self, cell: [i32; 2]) {
        let step = [cell[0] - self.player.cell[0], cell[1] - self.player.cell[1]];
        if step == [0, 0] || self.player.moving() || self.player.swinging(control::now()) {
            return;
        }
        // The sprites count directions the other way round from rathena: body = (4 - ours) mod 8 and back.
        let body = (12 - self.player.direction % 8) % 8;
        let (body, head) = turn_toward(body, self.player.head_direction as u8, world::direction([step[0], -step[1]]));
        let _ = self.session.change_direction(body, head);
        self.player.set_direction((12 - body) % 8);
        self.player.head_direction = u16::from(head);
    }

    /// What a left click on the world does: talk to an NPC, attack, swing once at a player, pick up an item, or
    /// (the ground, anything else) walk.
    fn world_click(&self, cursor: [f32; 2], shift: bool) -> WorldClick {
        match hovered(&self.hit_boxes, cursor, shift) {
            Some(Owner::Unit(id)) => match self.units.get(&id).map(|actor| actor.kind) {
                // A warp portal: walk onto it.
                Some(NPC) if self.units[&id].job == WARP_JOB => WorldClick::Warp(self.units[&id].cell),
                Some(NPC) => WorldClick::Talk(id),
                Some(MONSTER) => WorldClick::Attack(id),
                // Players: an attack where the map (or Shift, or /noshift) says so; one swing a click on a siege map.
                Some(PLAYER) => match versus::click(self.side().flags, self.players_first(shift), self.no_shift, self.account_id, id) {
                    versus::Click::Attack => WorldClick::Attack(id),
                    versus::Click::Swing => WorldClick::Swing(id),
                    versus::Click::Walk => WorldClick::Ground,
                },
                // Someone else's homunculus, mercenary or elemental where the map lets them be fought.
                Some(kind @ (HOMUNCULUS | MERCENARY | ELEMENTAL)) => {
                    let ours = self.homunculus_id == Some(id) || self.mercenary.as_ref().is_some_and(|mercenary| mercenary.id == id);
                    match versus::companion_click(self.side().flags, kind, self.players_first(shift), self.no_shift, ours) {
                        versus::Click::Attack => WorldClick::Attack(id),
                        versus::Click::Swing => WorldClick::Swing(id),
                        versus::Click::Walk => WorldClick::Ground,
                    }
                }
                _ => WorldClick::Ground,
            },
            Some(Owner::Item(id)) => WorldClick::PickUp(id),
            _ => WorldClick::Ground,
        }
    }

    fn store_open(&self) -> bool {
        self.my_shop.is_some() || self.my_buying_store.is_some()
    }

    /// What a press would aim the skill being aimed at: the cell under the pointer for ground skills (units there
    /// don't count), the unit under it (or the player) for the others.
    fn aim_target(&self, cursor: [f32; 2], players_first: bool) -> Option<SkillTarget> {
        let (id, _) = self.targeting?;
        let ground = self.skills.iter().chain(&self.item_skill).find(|skill| skill.id == id).is_some_and(|skill| skill.kind & net::Skill::GROUND != 0);
        let unit = match hovered_unit(&self.hit_boxes, cursor, players_first) {
            Some(Owner::Unit(unit)) => Some(unit),
            Some(Owner::Player) => Some(self.account_id),
            _ => None,
        };
        aim_use(ground, unit, if ground { self.ground_cell(cursor) } else { None })
    }

    /// A party member's small window (or a row of the compact list) under the pointer, for aiming a skill.
    /// The close button is left out: that still closes the window.
    fn party_member_at(&self, cursor: [f32; 2]) -> Option<u32> {
        match self.member_windows.hit(cursor) {
            Some((account, party::MemberHit::Body)) => Some(account),
            Some(_) => None,
            None if self.party_window.compact => party::compact_hit(cursor, self.party.as_ref()),
            None => None,
        }
    }

    /// Talks to an NPC, turned to face it; only standing still or sitting (walking or attacking, nothing).
    /// rathena answers when it's too far.
    fn talk_to(&mut self, id: u32) {
        if !may_talk(self.player.moving(), self.attack.is_some(), self.player.sitting) {
            return;
        }
        if let Some(cell) = self.units.get(&id).map(|actor| actor.cell) {
            self.turn_to(cell);
        }
        let _ = self.session.talk(id);
    }

    /// Picks up an item, facing it: at once from the next cell, else walking there with the pickup pending.
    fn pick_up(&mut self, id: u32, now: Instant) {
        if self.store_open() {
            return;
        }
        let Some(at) = self.ground.get(&id).map(|item| item.at) else { return };
        self.player.face(at);
        // Another press on the item while walking to it keeps that walk.
        if !(self.player.moving() && self.pending.is_some_and(|(pending, _)| pending == Pending::PickUp(id))) {
            self.pending_walk = None;
        }
        self.pending = Some((Pending::PickUp(id), None));
        self.pending_step(now);
    }

    /// A pickup or skill waiting on a walk, each frame. A pickup goes out as soon as the player's cell is next to the
    /// item, still walking; a walk the move limit held back is asked for again.
    fn pending_step(&mut self, now: Instant) {
        match self.pending {
            Some((Pending::PickUp(id), arrived)) => {
                let Some(at) = self.ground.get(&id).map(|item| item.at) else {
                    self.pending = None;
                    return;
                };
                match pickup_step(self.player.cell, at, self.pending_walk.map(|(cell, _)| cell)) {
                    PickStep::PickUp => {
                        self.pending = None;
                        self.player.face(at);
                        let _ = self.session.pick_up(id);
                    }
                    // The walk goes to the cell next to the item on the player's side (`pending_walk` notes the item).
                    PickStep::Walk if may_move(self.last_move, now) => {
                        let spot = pickup_spot(self.player.cell, at, |cell| self.world.gat.walkable(cell));
                        self.last_move = Some(now);
                        self.pending_walk = Some((at, now));
                        let _ = self.session.walk(spot.map(|value| value as u16));
                    }
                    // Stopped out of reach after walking there: it can't be reached.
                    PickStep::Wait if arrived.zip(self.pending_walk).is_some_and(|(stopped, (_, sent))| stopped > sent) => self.pending = None,
                    _ => {}
                }
            }
            // A skill: sent the moment it's in reach (walking or not), or once the walk there ended; out of reach, a walk
            // toward a cell in range on the way to where the target is going, again whenever that changes. It waits
            // while the player swings, casts or sits; a target out of sight ends it.
            Some((Pending::Skill { id, level, target }, arrived)) => {
                let clock = control::now();
                if self.player.swinging(clock) || self.casts.contains_key(&None) || self.player.sitting {
                    return;
                }
                let toward = match target {
                    SkillTarget::Unit(unit) if unit == self.account_id => Some((self.player.position(clock), self.player.cell)),
                    SkillTarget::Unit(unit) => {
                        self.units.get(&unit).filter(|actor| actor.remove_at.is_none()).map(|actor| (actor.position(clock), actor.destination()))
                    }
                    SkillTarget::Cell(cell) => Some((cell.map(|value| value as f32), cell)),
                };
                let Some((at, goal)) = toward else {
                    self.pending = None;
                    return;
                };
                let range = self.skill_range(id, level);
                let walked = arrived.is_some() && !self.player.moving();
                if skill_in_reach(self.player.position(clock), at, range) || walked {
                    self.pending = None;
                    self.pending_walk = None;
                    self.send_skill(id, level, target);
                } else if self.pending_walk.is_none_or(|(toward, _)| toward != goal) && may_move(self.last_move, now) {
                    let reach = range.max(1.0) as i32;
                    match world::find_path(&self.world.gat, self.player.cell, goal)
                        .and_then(|path| path.into_iter().find(|&cell| world::in_reach(cell, goal, reach)))
                    {
                        Some(stop) => {
                            self.last_move = Some(now);
                            self.pending_walk = Some((goal, now));
                            self.pending = Some((Pending::Skill { id, level, target }, None));
                            let _ = self.session.walk(stop.map(|value| value as u16));
                        }
                        None => self.pending = None,
                    }
                }
            }
            _ => {}
        }
    }

    /// The topmost window under the pointer.
    fn window_at(&self, cursor: [f32; 2]) -> Option<Panel> {
        self.windows.iter().rev().copied().find(|&id| match id {
            Panel::Hotkeys => self.hotkeys.hit(cursor).is_some(),
            Panel::BasicInfo => self.basic_info.hit(cursor).is_some(),
            Panel::Inventory => self.inventory.hit(cursor).is_some(),
            Panel::Equipment => self.equipment.hit(cursor, &self.inventory.items).is_some(),
            Panel::Skills => self.skill_window.frame.local(cursor, self.skill_window.height()).is_some(),
            Panel::Storage => self.storage.frame.open && self.storage.frame.local(cursor, self.storage.height()).is_some(),
            Panel::Party => self.party_window.frame.open && self.party_window.hit(cursor, self.party.as_ref(), &self.friends, self.account_id).is_some(),
            Panel::Status => self.status_window.frame.open && self.status_window.frame.local(cursor, self.status_height).is_some(),
            Panel::Guild => self.guild.is_some() && self.guild_window.frame.local(cursor, self.guild_window.height()).is_some(),
            Panel::Clan => self.clan.is_some() && self.clan_window.frame.local(cursor, self.clan_window.height()).is_some(),
            Panel::Cart => self.cart.cart.is_some() && self.cart.hit(cursor).is_some(),
            Panel::Pet => self.pet.as_ref().is_some_and(|pet| self.pet_window.hit(cursor, pet).is_some()),
            Panel::Mercenary => self.mercenary.is_some() && self.mercenary_window.hit(cursor).is_some(),
            Panel::MercenarySkills => self.mercenary.is_some() && self.mercenary_skill_list.hit(cursor, &self.mercenary_skills).is_some(),
            Panel::Homunculus => self.homunculus.as_ref().is_some_and(|homunculus| self.homunculus_window.hit(cursor, homunculus).is_some()),
            Panel::HomunculusSkills => self
                .homunculus
                .as_ref()
                .is_some_and(|homunculus| self.homunculus_skill_list.hit(cursor, &self.homunculus_skills, homunculus.skill_points).is_some()),
        })
    }

    /// How see-through a window is drawn: half while it's held.
    fn panel_faded(&self, panel: Panel) -> f32 {
        match panel {
            Panel::Hotkeys => self.hotkeys.frame.faded(),
            Panel::BasicInfo => self.basic_info.frame.faded(),
            _ => self.titled_frame(panel).map_or(0.0, window::Frame::faded),
        }
    }

    /// Every window's frame, by name, for saving where each was left. `minimap` is the game's MINIMAPWNDINFO.
    fn panel_frames(&mut self) -> [(&'static str, &mut window::Frame); 17] {
        [
            ("hotkeys", &mut self.hotkeys.frame),
            ("basic_info", &mut self.basic_info.frame),
            ("inventory", &mut self.inventory.frame),
            ("equipment", &mut self.equipment.frame),
            ("skills", &mut self.skill_window.frame),
            ("storage", &mut self.storage.frame),
            ("party", &mut self.party_window.frame),
            ("status", &mut self.status_window.frame),
            ("guild", &mut self.guild_window.frame),
            ("clan", &mut self.clan_window.frame),
            ("cart", &mut self.cart.frame),
            ("pet", &mut self.pet_window.frame),
            ("mercenary", &mut self.mercenary_window.frame),
            ("mercenary_skills", &mut self.mercenary_skill_list.frame),
            ("homunculus", &mut self.homunculus_window.frame),
            ("homunculus_skills", &mut self.homunculus_skill_list.frame),
            ("minimap", &mut self.minimap_window.frame),
        ]
    }

    /// Writes where the windows are and which are collapsed, when that changed and none is being dragged. A client
    /// driven over the control socket (scenarios) neither saves nor restores them, so each run starts the same.
    fn save_places(&mut self) {
        if control::driven() {
            return;
        }
        let frames = self.panel_frames();
        if frames.iter().any(|(_, frame)| frame.dragging()) {
            return;
        }
        let places = window_places(frames.iter().map(|(name, frame)| (*name, frame.position, frame.minimized)));
        if places != self.saved_places {
            if let Some(path) = settings_path("windows") {
                let _ = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(path, &places));
            }
            self.saved_places = places;
        }
    }

    /// Puts the windows where they were left last time.
    fn restore_places(&mut self) {
        if control::driven() {
            return;
        }
        let Some(text) = settings_path("windows").and_then(|path| std::fs::read_to_string(path).ok()) else { return };
        let saved = parse_window_places(&text);
        for (name, frame) in self.panel_frames() {
            if let Some(&(position, minimized)) = saved.get(name) {
                frame.position = window::dragged_to(position, [0.0, 0.0]);
                frame.minimized = minimized;
            }
        }
        self.saved_places = text;
    }

    /// The frame of a window with a title bar.
    fn titled_frame(&self, panel: Panel) -> Option<&window::Frame> {
        Some(match panel {
            Panel::Hotkeys | Panel::BasicInfo => return None,
            Panel::Inventory => &self.inventory.frame,
            Panel::Equipment => &self.equipment.frame,
            Panel::Skills => &self.skill_window.frame,
            Panel::Storage => &self.storage.frame,
            Panel::Party => &self.party_window.frame,
            Panel::Status => &self.status_window.frame,
            Panel::Guild => &self.guild_window.frame,
            Panel::Clan => &self.clan_window.frame,
            Panel::Cart => &self.cart.frame,
            Panel::Pet => &self.pet_window.frame,
            Panel::Mercenary => &self.mercenary_window.frame,
            Panel::MercenarySkills => &self.mercenary_skill_list.frame,
            Panel::Homunculus => &self.homunculus_window.frame,
            Panel::HomunculusSkills => &self.homunculus_skill_list.frame,
        })
    }

    /// Whether the pointer is on a window's control: a title bar's close or minimize button, the equipment window's
    /// buttons, a pick list's OK or Cancel.
    // ponytail: tabs, scroll arrows, resize corners and the minimap aren't told apart yet.
    /// Opens the navigation window, or closes it.
    fn toggle_navigation(&mut self) {
        self.navi_window = if self.navi_window.is_some() { None } else { Some(navigation::Window::new()) };
    }

    /// Opens the reputation window, or closes it.
    fn toggle_reputation(&mut self, tables: &reputation::Tables) {
        self.reputation_window = match self.reputation_window.take() {
            Some(_) => None,
            None => Some(reputation::Window::new(tables, &self.reputation)),
        };
    }

    /// The item link under the pointer: the NPC window's, else the chat log's where no window covers it.
    fn link_at(&self, cursor: [f32; 2]) -> Option<&links::Target> {
        if self.minimap_window.covers(cursor) {
            return None;
        }
        links::at(&self.dialog_links, cursor).or_else(|| self.window_at(cursor).is_none().then(|| links::at(&self.chat_links, cursor)).flatten())
    }

    fn on_control(&self, cursor: [f32; 2]) -> bool {
        let inside = |area: [f32; 4]| contains([area[0], area[1]], [area[2], area[3]], cursor);
        let title = self
            .window_at(cursor)
            .and_then(|panel| self.titled_frame(panel))
            .and_then(|frame| frame.title_hit([cursor[0] - frame.position[0], cursor[1] - frame.position[1]]));
        matches!(title, Some(TitleHit::Close | TitleHit::Minimize))
            || self.equipment.button_boxes().into_iter().any(|(_, area)| inside(area))
            || self.pick.as_ref().is_some_and(|(list, _)| list.button_boxes().into_iter().any(|(_, area)| inside(area)))
    }

    /// Whether the aim cursor shows "valid" here: over a unit a press would take; never for a ground skill.
    fn aim_valid(&self, cursor: [f32; 2], shift: bool) -> bool {
        matches!(self.aim_target(cursor, self.players_first(shift)), Some(SkillTarget::Unit(unit)) if self.aim_takes_unit(unit, shift))
    }

    /// Whether a press on `unit` takes the skill being aimed (`aim_takes`). A homunculus takes only Potion Pitcher;
    /// NPCs and the like none.
    fn aim_takes_unit(&self, unit: u32, shift: bool) -> bool {
        let Some((id, _)) = self.targeting else { return false };
        let support = self.skills.iter().chain(&self.item_skill).find(|skill| skill.id == id).is_some_and(|skill| skill.kind == net::Skill::FRIEND);
        let pvp = self.side().flags & net::MAP_NO_LOCK_ON != 0;
        let at = match self.units.get(&unit).map(|actor| actor.kind) {
            _ if unit == self.account_id => AimAt::Me,
            Some(MONSTER) => AimAt::Monster,
            Some(HOMUNCULUS) => return id == POTION_PITCHER,
            Some(NPC) | None => return false,
            Some(_) => AimAt::Player,
        };
        aim_takes(at, support, shift, self.no_shift, pvp)
    }

    fn skill_points(&self) -> i64 {
        self.status.get(&net::STATUS_SKILL_POINT).copied().unwrap_or(0)
    }

    /// Starts using a skill, ending any aim first: by its exact type a self skill goes off at once, an enemy, ground or
    /// support skill is aimed, anything else (passives, traps, mixed types) does nothing. A level above the learned one
    /// does nothing; an aimed skill without a level to pick goes at the learned level.
    fn use_skill(&mut self, id: u16, level: u16) {
        self.targeting = None;
        let Some(&net::Skill { kind, level: learned, .. }) = self.skills.iter().find(|skill| skill.id == id) else {
            return self.use_companion_skill(id, level);
        };
        // With a store open only the mercenary's skills go.
        if self.store_open() {
            return;
        }
        if level == 0 || level > learned {
            return;
        }
        match kind {
            net::Skill::SELF if self.cooled_down(id) => {
                let _ = self.session.use_skill(id, level, self.account_id);
            }
            net::Skill::SELF => {}
            net::Skill::ENEMY | net::Skill::GROUND | net::Skill::FRIEND => {
                self.targeting = Some((id, if self.skill_info.get(&id).is_some_and(|info| info.separate_level) { level } else { learned }));
            }
            // ponytail: trap skills (32) aim at ground skill units in the game; not made yet.
            _ => {}
        }
    }

    /// An item casts a skill: on oneself at once, otherwise aimed as a known one would be.
    fn cast_item_skill(&mut self, skill: net::Skill) {
        if self.store_open() {
            return;
        }
        if skill.kind & net::Skill::SELF != 0 {
            let _ = self.session.use_skill(skill.id, skill.level, self.account_id);
        } else if skill.kind & (net::Skill::ENEMY | net::Skill::GROUND | net::Skill::FRIEND) != 0 {
            self.targeting = Some((skill.id, skill.level));
            self.item_skill = Some(skill);
        }
    }

    /// Uses the aimed skill on what was clicked, walking into its range first.
    fn aim_skill(&mut self, target: SkillTarget) {
        // A Talkie Box asks for its message first.
        if let (Some((TALKIE_BOX, level)), SkillTarget::Cell(cell), None) = (self.targeting, target, &self.talkie_text) {
            self.targeting = None;
            self.amount = Some(window::Amount::name(Asked::TalkieBox { level, cell }, TALKIE_ASKS));
            return;
        }
        let Some((id, level)) = self.targeting.take() else { return };
        // The mercenary and the homunculus cast their own skills from where they stand; out of its range the server does nothing.
        // With a store open the server still takes them on a unit but refuses every skill on the ground.
        if self.mercenary_skills.iter().chain(&self.homunculus_skills).any(|skill| skill.id == id) {
            let _ = match target {
                SkillTarget::Unit(unit) => self.session.use_skill(id, level, unit),
                SkillTarget::Cell(_) if self.store_open() => return,
                SkillTarget::Cell(cell) => self.session.use_skill_at(id, level, cell.map(|value| value.max(0) as u16)),
            };
            return;
        }
        if self.store_open() {
            return;
        }
        // Out of reach it walks there first: `pending_step` checks every frame.
        self.pending = Some((Pending::Skill { id, level, target }, None));
        self.pending_walk = None;
        self.pending_step(Instant::now());
    }

    /// Whether a skill's cooldown is over (or under 100 ms is left); if not, says so.
    fn cooled_down(&mut self, id: u16) -> bool {
        if self.cooldowns.left(id, control::now()).is_none_or(|left| left < COOLDOWN_GRACE) {
            return true;
        }
        let name = self.skill_info.get(&id).map_or_else(|| id.to_string(), |info| info.name.clone());
        self.chat_line(rustnarok_client::messages::fill(&self.cooldown_line, &[&name]), CHAT_ERROR);
        false
    }

    /// The range in cells of skill `id` at `level`: its range for that level, plus what the skill list adds at the
    /// learned level.
    fn skill_range(&self, id: u16, level: u16) -> f32 {
        let known = self.skills.iter().chain(&self.item_skill).find(|skill| skill.id == id);
        let per_level = self.skill_info.get(&id).map_or(&[][..], |info| &info.attack_range);
        skill_range(per_level, level, known.map_or(level, |skill| skill.level), known.map_or(1, |skill| skill.range))
    }

    /// Sends a skill walked into range (refused while its cooldown runs).
    fn send_skill(&mut self, id: u16, level: u16, target: SkillTarget) {
        if !self.cooled_down(id) {
            return;
        }
        self.skill_sent = Some((id, level, target));
        let _ = match target {
            SkillTarget::Unit(unit) => self.session.use_skill(id, level, unit),
            SkillTarget::Cell(cell) => self.send_skill_at(id, level, cell.map(|value| value as u16)),
        };
    }

    /// Plays an effect on a unit (by server ID; the player's own works too) where it stands.
    fn play_effect(&mut self, gpu: &Gpu, grf: &Grf, table: &effects::Table, effect: u32, unit: u32) {
        self.play_effect_from(gpu, grf, table, effect, unit, Instant::now());
    }

    /// Plays an effect on a unit from a moment on (a hit when it lands).
    fn play_effect_from(&mut self, gpu: &Gpu, grf: &Grf, table: &effects::Table, effect: u32, unit: u32, start: Instant) {
        let key = (unit != self.account_id).then_some(unit);
        let cell = match key {
            None => Some(self.player.cell),
            Some(id) => self.units.get(&id).map(|actor| actor.cell),
        };
        if let Some(cell) = cell {
            let at = self.world.cell_position(cell.map(|value| value as f32));
            self.effects.play(gpu, grf, table, effect, effects::Target::Unit(key), at, start);
        }
    }

    /// Plays an effect at a cell.
    fn play_effect_at(&mut self, gpu: &Gpu, grf: &Grf, table: &effects::Table, effect: u32, cell: [u16; 2]) {
        let at = self.world.cell_position(cell.map(f32::from));
        self.effects.play(gpu, grf, table, effect, effects::Target::Ground, at, Instant::now());
    }

    /// The guild window's pages: the guild's info and its members, and the open tab's (positions, skills, the expel
    /// list; the notice comes by itself).
    fn ask_guild_pages(&mut self) {
        if self.guild.is_some() {
            let tab = self.guild_window.tab as u32;
            for page in [0, 1].into_iter().chain((2..=4).contains(&tab).then_some(tab)) {
                let _ = self.session.guild_page(page);
            }
        }
    }

    /// Asks for a guild's emblem unless that version is here or asked for already.
    fn want_emblem(&mut self, guild: u32, version: u32) {
        if guild == 0 || version == 0 {
            return;
        }
        let known = self.emblems.entry(guild).or_insert((0, None));
        if known.0 != version {
            known.0 = version;
            let _ = self.session.guild_emblem(guild);
        }
    }

    /// Sits down, or stands up.
    fn toggle_sit(&mut self) {
        let _ = self.session.act(0, if self.player.sitting { net::ACT_STAND } else { net::ACT_SIT });
    }

    /// Sends what's typed: whispered when the name box has a name, to the party with a leading % (or `party`, Ctrl
    /// held). Returns a message to show when it can't.
    fn send_chat(&mut self, party: bool, guild: bool, messages: &[String], effects_on: &mut bool) -> Option<usize> {
        let text = std::mem::take(&mut self.chat.input);
        let (text, to) = (text.trim(), self.chat.whisper.trim().to_owned());
        if text.is_empty() {
            return None;
        }
        if let Some(command) = chat::command(text) {
            self.client_command(command, messages, effects_on);
            return None;
        }
        // /sit sits down or stands up; /! /? /ho and the rest show an emotion.
        if text == "/sit" {
            self.toggle_sit();
            return None;
        }
        if let Some(kind) = emotion::by_command(text) {
            let _ = self.session.emote(kind);
            return None;
        }
        // The client's party commands (messages 138-140).
        if let Some(name) = text.strip_prefix("/organize ") {
            let _ = self.session.make_party(name.trim().trim_matches('"'), false, false);
            return None;
        }
        if text == "/leave" {
            let _ = self.session.leave_party();
            return None;
        }
        // ponytail: /refuse and /accept are rathena's names for them (clif_parse_PartyTick).
        if text == "/refuse" || text == "/accept" {
            let _ = self.session.refuse_parties(text == "/refuse");
            return None;
        }
        if let Some(name) = text.strip_prefix("/expel ") {
            let member = self.party.as_ref().and_then(|party| party.members.iter().find(|member| member.info.name == name.trim()));
            if let Some(member) = member {
                let _ = self.session.expel_from_party(member.info.account, &member.info.name);
            }
            return None;
        }
        if let Some(name) = text.strip_prefix("/guild ") {
            let _ = self.session.make_guild(self.me.id, name.trim().trim_matches('"'));
            return None;
        }
        // The guild's name is the key; rathena does nothing for a wrong one.
        if let Some(name) = text.strip_prefix("/breakguild ") {
            let _ = self.session.disband_guild(name.trim().trim_matches('"'));
            return None;
        }
        // ponytail: /cl sends to the clan until the chat window has the send-to choice (messages 85, 86, 437, 2361).
        if text == "/cl" {
            return None;
        }
        if let Some(message) = text.strip_prefix("/cl ") {
            let message = message.trim();
            if !message.is_empty() {
                let _ = self.session.clan_chat(&self.me.name, message);
            }
            return None;
        }
        // With /battlechat on, a line goes to the battleground team (not to the party, the guild or a whisper).
        if self.battle.chat {
            let _ = self.session.battle_chat(&self.me.name, text);
            return None;
        }
        if to.is_empty() && (guild || text.starts_with('$')) {
            let _ = self.session.guild_chat(&self.me.name, text.trim_start_matches('$'));
            return None;
        }
        if to.is_empty() && (party || text.starts_with('%')) {
            let _ = self.session.party_chat(&self.me.name, text.trim_start_matches('%'));
            return None;
        }
        // The client's own commands (message 136): /deal, /거래 or /교환 and a name ask for a deal.
        if let Some(name) = ["/deal ", "/거래 ", "/교환 "].iter().find_map(|command| text.strip_prefix(command)) {
            return (!self.ask_trade(name.trim())).then_some(TRADE_NO_ONE);
        }
        if to.is_empty() {
            let _ = self.session.chat(&self.me.name, text);
        } else {
            let _ = self.session.whisper(&to, text);
            self.whispered.push_back((to.clone(), text.to_owned()));
            self.chat.note_recent(&to);
            self.talks.push_line(&to, format!("(To {to}) : {text}"));
        }
        None
    }

    /// An incoming whisper: always a main-chat line, a 1:1 window when that sender's box is on, and one away reply.
    fn receive_whisper(&mut self, from: String, text: String) {
        let line = format!("(From {from}) : {text}");
        self.chat_line_kind(line.clone(), WHISPER_COLOR, chat::Kind::Whisper);
        self.chat.note_recent(&from);
        let friend = self.friends.iter().any(|friend| friend.name.eq_ignore_ascii_case(&from));
        if self.whisper_open.opens(friend) {
            self.talks.ensure(&from);
            self.sync_esc();
        }
        self.talks.push_line(&from, line);
        if let Some(reply) = self.away.reply(&from) {
            let _ = self.session.whisper(&from, &reply);
            self.whispered.push_back((from.clone(), reply.clone()));
            self.chat.note_recent(&from);
            self.talks.push_line(&from, format!("(To {from}) : {reply}"));
            // ponytail: a plain line; the client's wording for an automatic answer isn't identified.
            self.chat_line_kind(format!("Away: answered {from}."), WHISPER_COLOR, chat::Kind::Whisper);
        }
    }

    /// Typing in a 1:1 window whispers that partner, the same packet as the chat's name box.
    fn send_focused_talk(&mut self) {
        let Some((partner, text)) = self.talks.take_input() else { return };
        let _ = self.session.whisper(&partner, &text);
        self.whispered.push_back((partner.clone(), text.clone()));
        self.chat.note_recent(&partner);
        self.talks.push_line(&partner, format!("(To {partner}) : {text}"));
    }

    fn open_friend_setup(&mut self) {
        if self.friend_setup.take().is_some() {
            self.sync_esc();
            return;
        }
        self.friend_setup = Some(whisper::FriendSetup::new(self.whisper_open, self.party_window.frame.position));
        self.sync_esc();
    }

    fn apply_whisper_open(&mut self, settings: whisper::OpenSettings) {
        self.whisper_open = settings;
        // A driven client (a scenario) doesn't keep the boxes, so the next run still starts with both off.
        if !control::driven() {
            settings.save();
        }
    }

    /// OK on the away box sets the message (empty clears it). Cancel leaves the old one.
    fn finish_away(&mut self, text: &str) {
        self.away_prompt = None;
        self.away.set(text);
        // ponytail: the client's line for setting an away message isn't identified.
        let line = match self.away.text() {
            Some(message) => format!("Away message: {message}"),
            None => "Away message off.".to_owned(),
        };
        self.chat_line(line, CHAT_ITEM);
        self.sync_esc();
    }

    /// The pointer is on a whisper window, the friend setup, the block list or the away box.
    fn whisper_at(&self, cursor: [f32; 2]) -> bool {
        self.away_prompt.as_ref().is_some_and(|prompt| prompt.hit(cursor).is_some())
            || self.talks.hit(cursor).is_some()
            || self.block_list.as_ref().is_some_and(|list| list.hit(cursor).is_some())
            || self.friend_setup.as_ref().is_some_and(|setup| setup.hit(cursor).is_some())
    }

    /// A click on a whisper window, the friend setup, the block list or the away box.
    fn whisper_click(&mut self, cursor: [f32; 2]) -> bool {
        if let Some(hit) = self.away_prompt.as_ref().and_then(|prompt| prompt.hit(cursor)) {
            match hit {
                whisper::AwayHit::Ok => {
                    let text = self.away_prompt.take().map(|prompt| prompt.typed).unwrap_or_default();
                    self.finish_away(&text);
                }
                whisper::AwayHit::Cancel => {
                    self.away_prompt = None;
                    self.sync_esc();
                }
                whisper::AwayHit::Box => {}
            }
            return true;
        }
        if let Some((index, hit)) = self.talks.hit(cursor) {
            match self.talks.click(index, hit, cursor) {
                whisper::TalkClick::Save(partner, lines) => match whisper::save_log(std::path::Path::new(screenshot::FOLDER), &partner, &lines) {
                    Ok(path) => self.chat_line(format!("Chat saved: {}", path.display()), CHAT_ITEM),
                    Err(_) => self.chat_line("Chat was not saved.".into(), CHAT_ERROR),
                },
                whisper::TalkClick::Friend(partner) => {
                    let _ = self.session.add_friend(&partner);
                }
                whisper::TalkClick::Closed => self.sync_esc(),
                whisper::TalkClick::None => {}
            }
            return true;
        }
        if let Some(hit) = self.block_list.as_ref().and_then(|list| list.hit(cursor)) {
            match hit {
                whisper::BlockHit::Allow(name) => {
                    if self.session.set_whisper(&name, true).is_ok() {
                        self.whisper_asked.push_back(name);
                    }
                }
                whisper::BlockHit::Title(TitleHit::Close) => {
                    self.block_list = None;
                    self.sync_esc();
                }
                whisper::BlockHit::Title(TitleHit::Title) => {
                    if let Some(list) = &mut self.block_list {
                        list.frame.start_drag(cursor);
                    }
                }
                whisper::BlockHit::Title(title) => {
                    if let Some(list) = &mut self.block_list {
                        list.frame.click_title(title);
                        if !list.frame.open {
                            self.block_list = None;
                            self.sync_esc();
                        }
                    }
                }
                whisper::BlockHit::Window => {}
            }
            return true;
        }
        if let Some(hit) = self.friend_setup.as_ref().and_then(|setup| setup.hit(cursor)) {
            match hit {
                whisper::SetupHit::Title(TitleHit::Title) => {
                    if let Some(setup) = &mut self.friend_setup {
                        setup.frame.start_drag(cursor);
                    }
                }
                whisper::SetupHit::Title(title) => {
                    let close = title == TitleHit::Close;
                    if let Some(setup) = &mut self.friend_setup {
                        setup.frame.click_title(title);
                        if close || !setup.frame.open {
                            self.friend_setup = None;
                            self.sync_esc();
                        }
                    }
                }
                other => {
                    let changed = self.friend_setup.as_mut().and_then(|setup| setup.click(other));
                    if let Some(whisper::SetupClick::Changed(settings)) = changed {
                        self.apply_whisper_open(settings);
                    }
                }
            }
            return true;
        }
        false
    }

    /// Answers a chat command the client handles itself; replies from the server come back as map events.
    fn client_command(&mut self, command: chat::Command, messages: &[String], effects_on: &mut bool) {
        let message = |id: usize| messages.get(id).cloned().unwrap_or_default();
        let _ = match command {
            chat::Command::Help => {
                for id in std::iter::once(HELP_TITLE).chain(HELP_LINES) {
                    self.chat_line(message(id), CHAT_ITEM);
                }
                Ok(())
            }
            chat::Command::Who => self.session.user_count(),
            chat::Command::Memo => self.session.remember_warp_point(),
            chat::Command::Ranking(kind) => self.session.ask_ranking(kind),
            chat::Command::BattleChat => {
                let id = self.battle.toggle_chat();
                self.chat_line(message(id), CHAT_ITEM);
                Ok(())
            }
            chat::Command::BattleMode => {
                let id = self.chat_mode.toggle();
                self.chat_mode.save();
                self.chat_line(message(id), CHAT_ITEM);
                Ok(())
            }
            chat::Command::DressRoom => {
                self.open_dressroom();
                Ok(())
            }
            chat::Command::CashShop => {
                let _ = self.session.open_cashshop(0);
                Ok(())
            }
            // The battleground queue; while queued, how many wait is asked again.
            chat::Command::BattleQueue => {
                self.queue_window.frame.toggle();
                if self.queue_window.frame.open
                    && let Some((name, _)) = &self.queued
                {
                    let _ = self.session.ask_queue_number(name);
                }
                Ok(())
            }
            chat::Command::Navigation => {
                self.toggle_navigation();
                self.sync_esc();
                Ok(())
            }
            // ponytail: the client's lines for /camera speak of zooming; they're used for on and off.
            chat::Command::Camera => {
                self.camera_snap = !self.camera_snap;
                if let Some(path) = settings_path("camerasnap") {
                    let _ = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(path, if self.camera_snap { "1" } else { "0" }));
                }
                let line = messages.get(if self.camera_snap { CAMERA_ON } else { CAMERA_OFF }).cloned().unwrap_or_default();
                self.chat_line(line, CHAT_ITEM);
                Ok(())
            }
            chat::Command::Zoom => {
                let on = !self.camera_goal.zoom_out;
                if let Some(path) = settings_path("zoomout") {
                    let _ = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(path, if on { "1" } else { "0" }));
                }
                self.camera_goal.zoom_out = on;
                let distance = self.camera_goal.distance();
                self.camera_goal.set_distance(distance);
                let line = messages.get(if on { ZOOM_OUT_ON } else { ZOOM_OUT_OFF }).cloned().unwrap_or_default();
                self.chat_line(line, CHAT_ITEM);
                Ok(())
            }
            chat::Command::NoCtrl => {
                self.no_ctrl = !self.no_ctrl;
                if let Some(path) = settings_path("noctrl") {
                    let _ = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(path, if self.no_ctrl { "1" } else { "0" }));
                }
                let line = messages.get(if self.no_ctrl { NO_CTRL_ON } else { NO_CTRL_OFF }).cloned().unwrap_or_default();
                self.chat_line(line, CHAT_ITEM);
                Ok(())
            }
            // ponytail: the client's own line for /noshift isn't in this data; a plain one.
            chat::Command::NoShift => {
                self.no_shift = !self.no_shift;
                if let Some(path) = settings_path("noshift") {
                    let _ = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(path, if self.no_shift { "1" } else { "0" }));
                }
                self.chat_line(format!("noshift: {}", if self.no_shift { "On" } else { "Off" }), CHAT_ITEM);
                Ok(())
            }
            // ponytail: plain on/off lines, as /noshift prints.
            chat::Command::Snap | chat::Command::ItemSnap | chat::Command::SkillSnap => {
                let (name, on) = match command {
                    chat::Command::Snap => ("snap", &mut self.snaps.snap),
                    chat::Command::ItemSnap => ("itemsnap", &mut self.snaps.item),
                    _ => ("skillsnap", &mut self.snaps.skill),
                };
                *on = !*on;
                let on = *on;
                if let Some(path) = settings_path(name) {
                    let _ = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(path, if on { "1" } else { "0" }));
                }
                self.chat_line(format!("{name}: {}", if on { "On" } else { "Off" }), CHAT_ITEM);
                Ok(())
            }
            chat::Command::Where => {
                // ponytail: the map and the cell, plainly; match the official client's wording from a screenshot.
                let [x, y] = self.player.cell;
                self.chat_line(format!("{} {x} {y}", map_key(&self.map)), CHAT_ITEM);
                Ok(())
            }
            chat::Command::Whispers { name, allow } => {
                let sent = self.session.set_whisper(&name, allow);
                if sent.is_ok() {
                    self.whisper_asked.push_back(name);
                }
                sent
            }
            chat::Command::BlockList => self.session.whisper_list(),
            chat::Command::Away => {
                let typed = self.away.text().unwrap_or("").to_owned();
                self.away_prompt = Some(whisper::AwayPrompt::new(typed));
                self.sync_esc();
                Ok(())
            }
            chat::Command::AllWhispers { allow } => self.session.set_whisper_all(allow),
            chat::Command::Effect => {
                *effects_on = !*effects_on;
                self.chat_line(message(if *effects_on { EFFECTS_ON } else { EFFECTS_OFF }), CHAT_ITEM);
                Ok(())
            }
            chat::Command::Achievements => {
                self.achievement_window.frame.toggle();
                Ok(())
            }
            chat::Command::EmotionList => {
                self.toggle_emotion_list();
                Ok(())
            }
        };
    }

    fn toggle_emotion_list(&mut self) {
        self.emotion_list = if self.emotion_list.is_some() { None } else { Some(emotion_list::List::default()) };
        if self.emotion_list.is_some() {
            escape::touch(&mut self.esc_order, escape::WindowId::EmotionList);
        } else {
            escape::remove(&mut self.esc_order, escape::WindowId::EmotionList);
        }
    }

    /// Opens the macro window, or closes it keeping what was typed.
    fn toggle_macro_window(&mut self) {
        if self.macro_window.take().is_none() {
            self.macro_window = Some(macros::Window::default());
            escape::touch(&mut self.esc_order, escape::WindowId::Macros);
        } else {
            macros::save(macros::path().as_deref(), &self.macro_lines);
            escape::remove(&mut self.esc_order, escape::WindowId::Macros);
        }
    }

    /// A text box has the caret. The chat line does while it is open, and so does a window's field while it is typed in.
    fn text_box_focused(&self) -> bool {
        self.dialog.as_ref().is_some_and(|dialog| dialog.input.is_some())
            || self.amount.is_some()
            || self.vend_setup.as_ref().is_some_and(|setup| setup.typing)
            || self.buying_setup.as_ref().is_some_and(|setup| setup.focus.is_some())
            || self.mail.typing()
            || self.macro_window.as_ref().is_some_and(|window| window.editing.is_some())
            || self.room_make.is_some()
            || self.room.is_some()
            || self.cashshop.as_ref().is_some_and(|shop| shop.typing)
            || self.bank.as_ref().is_some_and(|bank| bank.typing)
            || self.trade.as_ref().is_some_and(|trade| trade.typing)
            || self.storage.typing
            || self.catalog.as_ref().is_some_and(|catalog| catalog.focus.is_some())
            || self.talks.typing()
            || self.away_prompt.is_some()
            || self.minimap_window.naming()
            // Battle mode off: the line is always open, so letters stay out of the shortcut layer.
            || self.chat_mode.chat_line_open()
    }

    /// F11 closes the player's windows. A talk with an NPC stays until the server ends it.
    fn close_windows(&mut self) {
        for frame in [
            &mut self.hotkeys.frame,
            &mut self.basic_info.frame,
            &mut self.inventory.frame,
            &mut self.equipment.frame,
            &mut self.skill_window.frame,
            &mut self.storage.frame,
            &mut self.party_window.frame,
            &mut self.status_window.frame,
            &mut self.guild_window.frame,
            &mut self.clan_window.frame,
            &mut self.cart.frame,
            &mut self.pet_window.frame,
            &mut self.mercenary_window.frame,
            &mut self.mercenary_skill_list.frame,
            &mut self.homunculus_window.frame,
            &mut self.homunculus_skill_list.frame,
            &mut self.quest_window.frame,
            &mut self.achievement_window.frame,
            &mut self.mail.list.frame,
            &mut self.queue_window.frame,
        ] {
            frame.open = false;
        }
        self.basic_info.menu_open = false;
        self.equipment.swapping = false;
        if self.macro_window.take().is_some() {
            macros::save(macros::path().as_deref(), &self.macro_lines);
        }
        self.emotion_list = None;
        self.world_map = None;
        self.minimap_window.close();
        self.audio_window = None;
        self.graphics_window = None;
        self.shortcuts_window = None;
        self.item_window = None;
        self.card_view = None;
        self.monster_window = None;
        self.ranking = None;
        self.dressroom = None;
        self.dress_figure = None;
        self.party_setup = None;
        self.talks = whisper::Talks::default();
        self.friend_setup = None;
        self.block_list = None;
        self.away_prompt = None;
        self.escape = None;
        self.room_make = None;
        self.viewing = None;
        self.mail.letter = None;
        self.mail.compose = None;
        if self.bank.is_some() {
            let _ = self.session.close_bank(self.account_id);
            self.bank = None;
        }
        self.close_catalog();
    }

    /// Whether the pointer is over the interface (a window, the chat, a dialog) rather than the world.
    /// Whether the pointer is on a window, or something holds the clicks (a dialog, a menu, a question in the message
    /// box of this size).
    fn on_ui(&self, cursor: [f32; 2], items: &HashMap<u32, ItemInfo>, message: [f32; 2]) -> bool {
        let asking = self.guild_invite.is_some()
            || self.guild_asked.is_some()
            || self.bind_asked.is_some()
            || self.expansion_asked.is_some()
            || self.leader_asked.is_some()
            || self.dismiss_asked
            || self.homunculus_rename.is_some()
            || self.pet_asked.is_some()
            || self.friend_asked.is_some()
            || self.party_invite.is_some()
            || self.trade_asked.is_some()
            || self.queue_asked.is_some()
            || self.adopt_asked.is_some();
        self.window_at(cursor).is_some()
            || self.catalog.as_ref().and_then(|catalog| catalog.hit(cursor)).is_some()
            || self.queue_window.covers(cursor)
            || self.skill_description.as_ref().is_some_and(|window| window.contains(cursor))
            || self.level_ups.hit(cursor).is_some()
            || (self.quest_icon && quest_notice::on_icon(cursor))
            || minimap::button_at(cursor).is_some()
            || self.minimap_window.covers(cursor)
            || (asking && contains(dialog_position_of(message), message, cursor))
            || self.dialog.is_some()
            || self.amount.is_some()
            || self.escape.is_some()
            || self.world_map.is_some()
            || self.deal_choice.is_some()
            || self.menu.is_some()
            || self.pick.as_ref().is_some_and(|(list, _)| list.covers(cursor))
            || self.shop.as_ref().is_some_and(|shop| shop.covers(cursor))
            || self.refine.as_ref().is_some_and(|window| window.hit(cursor).is_some())
            || self.enchant.as_ref().is_some_and(|window| window.hit(cursor).is_some())
            || self.materials.as_ref().is_some_and(|window| window.hit(cursor).is_some())
            || self.bank.as_ref().is_some_and(|bank| bank.hit(cursor).is_some())
            || self.viewing.as_ref().is_some_and(|viewed| viewed.window.hit(cursor, &viewed.items).is_some())
            || self.ranking.as_ref().and_then(|board| board.hit(cursor)).is_some()
            || self.reputation_window.as_ref().and_then(|window| window.hit(cursor)).is_some()
            || self.navi_window.as_ref().and_then(|window| window.hit(cursor)).is_some()
            || self.dressroom.as_ref().and_then(|room| room.hit(cursor)).is_some()
            || self.party_setup.as_ref().and_then(|setup| setup.hit(cursor)).is_some()
            || self.talks.hit(cursor).is_some()
            || self.friend_setup.as_ref().is_some_and(|setup| setup.hit(cursor).is_some())
            || self.block_list.as_ref().is_some_and(|list| list.hit(cursor).is_some())
            || self.away_prompt.as_ref().is_some_and(|prompt| prompt.hit(cursor).is_some())
            || self.chat.on_list(cursor)
            || self.member_windows.hit(cursor).is_some()
            || self.party_window.compact && party::compact_hit(cursor, self.party.as_ref()).is_some()
            || (self.room.is_none() && self.sign_rects.iter().any(|(rect, ..)| contains([rect[0], rect[1]], [rect[2] - rect[0], rect[3] - rect[1]], cursor)))
            || self.audio_window.as_ref().is_some_and(|window| window.hit(cursor).is_some())
            || self.cashshop.as_ref().is_some_and(|shop| shop.hit(cursor).is_some())
            || self.cashshop_buy_asked
            || self.graphics_window.as_ref().is_some_and(|window| window.hit(cursor).is_some())
            || self.shortcuts_window.as_ref().is_some_and(|window| window.hit(cursor).is_some())
            || self.card_view.as_ref().is_some_and(|view| view.hit(cursor).is_some())
            || self.item_window.as_ref().is_some_and(|window| window.hit(cursor, items).is_some())
            || self.menu.as_ref().is_some_and(|menu| menu.hit(cursor).is_some())
            || self.trade.as_ref().is_some_and(|trade| trade.hit(cursor).is_some())
            || self.room.as_ref().is_some_and(|room| room.hit(cursor, &self.me.name).is_some())
            || self.room_make.as_ref().is_some_and(|make| make.hit(cursor).is_some())
            || self.vend_setup.as_ref().is_some_and(|setup| setup.hit(cursor, &self.cart.items).is_some())
            || self.my_shop.as_ref().is_some_and(|shop| shop.hit(cursor).is_some())
            || self.buying_setup.as_ref().is_some_and(|setup| setup.hit(cursor, &self.inventory.items).is_some())
            || self.my_buying_store.as_ref().is_some_and(|store| store.hit(cursor).is_some())
            || self.mail.hit(cursor).is_some()
            || self.quest_window.hit(cursor, &self.quests).is_some()
            || self.achievement_window.covers(cursor)
            || self.synthesis.as_ref().is_some_and(|window| window.hit(cursor).is_some())
            || self.upgrade.as_ref().is_some_and(|window| window.hit(cursor).is_some())
            || self.roulette.as_ref().is_some_and(|window| window.hit(cursor).is_some())
            || self.reform.as_ref().is_some_and(|window| window.hit(cursor, &self.inventory.items).is_some())
            || self.grade.as_ref().is_some_and(|window| window.hit(cursor).is_some())
            || roulette::icon_hit(cursor)
            || self.monster_window.as_ref().is_some_and(|window| window.hit(cursor).is_some())
            || self.stylist.as_ref().is_some_and(|window| window.covers(cursor))
            || self.chat.blocks(cursor)
            || self.chat.settings.as_ref().and_then(|settings| settings.hit(cursor)).is_some()
            || self.link_at(cursor).is_some()
            || self.emotion_list.as_ref().is_some_and(|list| list.hit(cursor).is_some())
            || self.macro_window.as_ref().is_some_and(|window| window.hit(cursor).is_some())
    }

    /// Moves an item between the inventory and storage, asking how many when there's more than one.
    fn move_item(&mut self, asked: Asked) {
        let amount = match asked {
            Asked::Store(index) | Asked::Drop(index) | Asked::Trade(index) | Asked::ToCart(index) | Asked::Mail(index) | Asked::Material(index) => {
                self.inventory.get(index).map(|item| item.amount)
            }
            Asked::FromCart(index) => self.cart.get(index).map(|item| item.amount),
            Asked::Take(index) => self.storage.get(index).map(|item| item.amount),
            Asked::Shop(_)
            | Asked::ShopOut(_)
            | Asked::PartyName
            | Asked::Invite
            | Asked::RoomPassword(_)
            | Asked::VendPrice(_)
            | Asked::BuyingPrice(..)
            | Asked::BuyingAmount(_)
            | Asked::PetName
            | Asked::HomunculusName
            | Asked::PositionTitle(_)
            | Asked::PositionTax(_)
            | Asked::TalkieBox { .. } => None,
        };
        match amount {
            Some(1) => self.give(asked, 1),
            Some(amount) => self.amount = Some(window::Amount::new(asked, Some(u32::from(amount)))),
            None => {}
        }
    }

    /// The box asking how many (or for a name) is answered.
    fn answer_amount(&mut self, ok: bool, messages: &[String]) {
        let Some(amount) = self.amount.take().filter(|_| ok) else { return };
        let name = amount.typed.trim();
        let _ = match amount.what {
            Asked::PartyName if !name.is_empty() => self.session.make_party(name, false, false),
            Asked::Invite if !name.is_empty() => self.session.invite_to_party(name),
            Asked::RoomPassword(id) => self.session.enter_chat_room(id, name),
            // A name for the pet asks once more: it can be renamed only once.
            Asked::PetName if !name.is_empty() => {
                self.pet_asked = Some(PetAsked::Rename(name.to_owned()));
                Ok(())
            }
            // The message is in: the box goes where it was aimed (walking there first if need be).
            Asked::TalkieBox { level, cell } => {
                self.talkie_text = Some(name.to_owned());
                self.targeting = Some((TALKIE_BOX, level));
                self.aim_skill(SkillTarget::Cell(cell));
                Ok(())
            }
            // The same once-only question for the homunculus.
            Asked::HomunculusName if !name.is_empty() => {
                self.homunculus_rename = Some(name.to_owned());
                Ok(())
            }
            Asked::PartyName | Asked::Invite | Asked::PetName | Asked::HomunculusName => Ok(()),
            // A position's title or tax: the request carries its other values as they are.
            Asked::PositionTitle(id) if !name.is_empty() => match &self.guild {
                Some(guild) => self.session.change_position(&guild.position(id), name),
                None => Ok(()),
            },
            Asked::PositionTax(id) if !name.is_empty() => match &self.guild {
                Some(guild) => self.session.change_position(&net::GuildPosition { tax: amount.value(), ..guild.position(id) }, &guild.title(id)),
                None => Ok(()),
            },
            Asked::PositionTitle(_) | Asked::PositionTax(_) => Ok(()),
            Asked::BuyingPrice(index, item_id) => {
                if let Some(setup) = &mut self.buying_setup {
                    setup.set_price(index, item_id, amount.value());
                    self.amount = Some(window::Amount::new(Asked::BuyingAmount(index), None));
                }
                Ok(())
            }
            Asked::BuyingAmount(index) => {
                if let Some(setup) = &mut self.buying_setup {
                    setup.set_amount(index, amount.value().min(u32::from(u16::MAX)) as u16);
                }
                Ok(())
            }
            Asked::ShopOut(row) => {
                if let Some(shop) = &mut self.shop
                    && amount.value() > 0
                {
                    shop.take_out(row, amount.value());
                }
                Ok(())
            }
            Asked::Shop(row) if amount.value() > 0 => {
                let zeny = self.status.get(&net::STATUS_ZENY).copied().unwrap_or(0).max(0) as u64;
                if let Some(shop) = &mut self.shop
                    && let Some((msg_id, arg)) = {
                        shop.zeny = zeny;
                        shop.add_checked(row, amount.value())
                    }
                {
                    let text = messages.get(msg_id).map_or("", String::as_str);
                    let line = match arg {
                        Some(limit) => rustnarok_client::messages::fill(text, &[&limit]),
                        None => text.to_owned(),
                    };
                    self.chat_line(line, CHAT_ERROR);
                }
                Ok(())
            }
            // None or less into storage or the cart is one.
            what @ (Asked::Store(_) | Asked::ToCart(_)) => {
                self.give(what, amount.value().max(1));
                Ok(())
            }
            what if amount.value() > 0 => {
                self.give(what, amount.value());
                Ok(())
            }
            _ => Ok(()),
        };
    }

    /// Does what was asked with `amount` (no more than there is).
    fn give(&mut self, asked: Asked, amount: u32) {
        let _ = match asked {
            Asked::Shop(row) => {
                if let Some(shop) = &mut self.shop {
                    shop.add(row, amount);
                }
                Ok(())
            }
            Asked::ShopOut(row) => {
                if let Some(shop) = &mut self.shop {
                    shop.take_out(row, amount);
                }
                Ok(())
            }
            Asked::Store(index) => {
                let have = self.inventory.get(index).map_or(0, |item| u32::from(item.amount));
                self.session.put_in_storage(index, amount.min(have))
            }
            Asked::Take(index) => {
                let have = self.storage.get(index).map_or(0, |item| u32::from(item.amount));
                self.session.take_from_storage(index, amount.min(have))
            }
            // More than there is drops nothing.
            Asked::Drop(index) => match self.inventory.get(index).map_or(0, |item| u32::from(item.amount)) {
                have if amount > have => Ok(()),
                _ => self.session.drop_item(index, amount as u16),
            },
            Asked::PartyName
            | Asked::Invite
            | Asked::RoomPassword(_)
            | Asked::PetName
            | Asked::HomunculusName
            | Asked::PositionTitle(_)
            | Asked::PositionTax(_)
            | Asked::BuyingPrice(..)
            | Asked::BuyingAmount(_)
            | Asked::TalkieBox { .. } => Ok(()),
            Asked::VendPrice(index) => {
                if let Some(setup) = &mut self.vend_setup {
                    setup.prices.insert(index, amount);
                }
                Ok(())
            }
            Asked::ToCart(index) => {
                let have = self.inventory.get(index).map_or(0, |item| u32::from(item.amount));
                self.session.put_in_cart(index, amount.min(have))
            }
            Asked::FromCart(index) => {
                let have = self.cart.get(index).map_or(0, |item| u32::from(item.amount));
                self.session.take_from_cart(index, amount.min(have))
            }
            // Nothing goes to the server until the window's OK.
            Asked::Material(index) => {
                let have = self.inventory.get(index).map_or(0, |item| item.amount);
                if let Some(window) = &mut self.materials {
                    window.add(index, amount.min(u32::from(have)) as u16, have);
                }
                Ok(())
            }
            Asked::Mail(index) => {
                let have = self.inventory.get(index).map_or(0, |item| item.amount);
                self.session.attach_to_mail(index, (amount.min(u32::from(have))) as u16, true)
            }
            Asked::Trade(index) => {
                let have = self.inventory.get(index).map_or(0, |item| u32::from(item.amount));
                match &mut self.trade {
                    Some(trade) => {
                        trade.pending.insert(index, amount.min(have));
                        self.session.add_to_trade(index, amount.min(have))
                    }
                    None => Ok(()),
                }
            }
        };
    }

    /// Carries out what a click on the mail windows asks; returns a message (by number) for the chat.
    fn mail_action(&mut self, action: mail::Action) -> Option<usize> {
        let _ = match action {
            mail::Action::Open => self.session.open_mailbox(false),
            mail::Action::Close => self.session.close_mailbox(),
            mail::Action::Refresh => self.session.open_mailbox(true),
            mail::Action::Read(kind, id) => self.session.read_mail(kind, id),
            mail::Action::Write(to) => self.session.start_mail(&to),
            mail::Action::Claim { kind, id, zeny } => self.session.claim_mail(kind, id, zeny),
            mail::Action::Delete(kind, id) => self.session.delete_mail(kind, id),
            mail::Action::Check(name) => self.session.check_receiver(&name),
            mail::Action::Detach(index, amount) => self.session.attach_to_mail(index, amount, false),
            mail::Action::Send => {
                let Some(compose) = &self.mail.compose else { return None };
                let receiver = compose.receiver.map_or(0, |(character, ..)| character);
                self.session.send_mail(&compose.to, &self.me.name, &compose.title, &compose.text, compose.zeny(), receiver)
            }
            mail::Action::Cancel => self.session.cancel_mail(),
            mail::Action::Say(id) => return Some(id),
        };
        None
    }

    /// An inventory item let go on the deal: it goes in unless the deal is locked, holds it already or is full.
    fn offer(&mut self, index: u16, all: bool) -> Option<usize> {
        let trade = self.trade.as_ref()?;
        if !trade.open_for_items() {
            return None;
        }
        if trade.pending.contains_key(&index) {
            return Some(TRADE_OVERLAP);
        }
        if trade.items[0].len() + trade.pending.len() >= trade::MOST_ITEMS {
            return Some(TRADE_FULL);
        }
        match self.inventory.get(index).map(|item| u32::from(item.amount)) {
            Some(amount) if all => self.give(Asked::Trade(index), amount),
            _ => self.move_item(Asked::Trade(index)),
        }
        None
    }

    /// Alt + right click on an inventory item: the whole stack goes to the first that's open of the storage, a trade
    /// and the cart. A refused trade says why.
    fn quick_move(&mut self, index: u16) -> Option<usize> {
        let amount = u32::from(self.inventory.get(index)?.amount);
        match quick_target(self.storage.frame.open, self.trade.is_some(), self.cart.cart.is_some() && self.cart.frame.open)? {
            QuickTo::Storage => self.give(Asked::Store(index), amount),
            QuickTo::Trade => return self.offer(index, true),
            QuickTo::Cart => self.give(Asked::ToCart(index), amount),
        }
        None
    }

    /// The room maker's OK: opens the room (the window stays until the server says).
    fn make_room(&mut self) {
        if let Some(make) = &self.room_make
            && !make.title.trim().is_empty()
        {
            if make.editing {
                // The server's ZC_CHANGE_CHATROOM brings the changed sign.
                let _ = self.session.change_chat_room(make.title.trim(), make.limit(), make.public, &make.password);
                self.room_make = None;
            } else {
                let _ = self.session.make_chat_room(make.title.trim(), make.limit(), make.public, &make.password);
            }
        }
    }

    /// Locks our side of the deal in, with the zeny typed (no more than carried).
    fn lock_trade(&mut self, zeny: u32) {
        let carried = self.status.get(&net::STATUS_ZENY).copied().unwrap_or(0).max(0) as u32;
        let zeny = zeny.min(carried);
        if let Some(trade) = &mut self.trade {
            trade.typed = zeny.to_string();
        }
        if zeny > 0 {
            let _ = self.session.add_to_trade(0, zeny);
        }
        let _ = self.session.lock_trade();
    }

    /// Dev switch `RUSTNAROK_CLIENT_TRADE_GIVE`: once a deal opens, puts those in and locks.
    fn auto_trade(&mut self) {
        let Ok(give) = std::env::var("RUSTNAROK_CLIENT_TRADE_GIVE") else { return };
        let mut zeny = 0;
        for part in give.split(',') {
            if let Some(amount) = part.strip_prefix('z') {
                zeny = amount.parse().unwrap_or(0);
            } else if let Some((id, amount)) = part.split_once(':')
                && let (Ok(id), Ok(amount)) = (id.parse::<u32>(), amount.parse::<u32>())
                && let Some(index) = self.inventory.items.iter().find(|item| item.id == id && item.worn == 0).map(|item| item.index)
            {
                self.give(Asked::Trade(index), amount);
            }
        }
        self.lock_trade(zeny);
    }

    /// Asks a player seen by name for a deal (`/deal name`), or says there's no one of that name.
    fn ask_trade(&mut self, name: &str) -> bool {
        let found = self.units.iter().find(|(_, actor)| actor.kind == PLAYER && actor.name == name).map(|(&id, _)| id);
        if let Some(id) = found {
            self.trade_with = name.to_owned();
            let _ = self.session.request_trade(id);
        }
        found.is_some()
    }

    /// Dev switch `RUSTNAROK_CLIENT_STORE=put|take:<item>:<amount>`, once storage has opened.
    fn auto_store(&mut self) {
        let Some(order) = self.auto_store.take() else { return };
        let mut parts = order.split(':');
        let (Some(way), Some(Ok(id)), Some(Ok(amount))) = (parts.next(), parts.next().map(str::parse::<u32>), parts.next().map(str::parse::<u32>)) else {
            return;
        };
        let asked = if way == "take" {
            self.storage.items.iter().find(|item| item.id == id).map(|item| Asked::Take(item.index))
        } else {
            self.inventory.items.iter().find(|item| item.id == id && item.worn == 0).map(|item| Asked::Store(item.index))
        };
        if let Some(asked) = asked {
            self.give(asked, amount);
        }
    }

    /// Dev switch `RUSTNAROK_CLIENT_DEAL=buy|sell:<row>:<amount>`: puts a row of the shop in the deal and makes it.
    fn auto_deal(&mut self) {
        let Ok(deal) = std::env::var("RUSTNAROK_CLIENT_DEAL") else { return };
        let mut parts = deal.split(':').skip(1).map(str::parse::<u32>);
        let (Some(Ok(row)), Some(Ok(amount)), Some(shop)) = (parts.next(), parts.next(), self.shop.as_mut()) else { return };
        if (row as usize) < shop.offers.len() {
            shop.add(row as usize, amount);
            let _ = shop.send(&mut self.session);
        }
    }

    /// Words over a unit's head (None = the player); an item link shows as its words.
    fn say(&mut self, unit: Option<u32>, text: String, color: [u8; 3]) {
        let text = links::split(&links::mark(&text, &|_| String::new())).0;
        self.speech.insert(unit, (text, color, Instant::now()));
    }

    /// A unit starts or uses a skill: players strike the skill pose, and units call its name (see `overhead::shouts`).
    /// A player caster's motion for a skill packet (`skill_motions`) and its name shouted. A cast holds the cast pose
    /// for `pace` ms (none for skills cast standing); the others play their motion at `pace` times its ACT pace.
    /// Returns when the hit lands, in ms from now: the swing's "atk" frame, the skill pose's frame 6.
    #[allow(clippy::too_many_arguments)]
    fn skill_motion(
        &mut self,
        source: u32,
        skill: u16,
        level: i32,
        packet: overhead::Shout,
        pace: f32,
        info: &HashMap<u16, SkillInfo>,
        tables: &LookTables,
    ) -> Option<f32> {
        let me = self.account_id;
        let actor = (if source == me { Some(&mut self.player) } else { self.units.get_mut(&source) })?;
        if actor.remove_at.is_some() {
            return None;
        }
        let monster = actor.kind == MONSTER;
        let mut hit_ms = None;
        if actor.kind == PLAYER {
            let now = Instant::now();
            let action = match (packet, skill_motions::motion(skill)) {
                (overhead::Shout::Cast, _) if skill_motions::cast_standing(skill) => None,
                (overhead::Shout::Cast, _) => {
                    actor.hold_for(SKILL_ACTION, now, pace);
                    None
                }
                (_, skill_motions::Motion::Keep) => None,
                (_, skill_motions::Motion::Attack) => Some(
                    actor
                        .look
                        .as_ref()
                        .map_or(actor::combat_actions(actor.kind).0, |look| actor::attack_action(look.job, look.sex, tables.weapon_type(look.weapon))),
                ),
                (_, skill_motions::Motion::Throw) => Some(5),
                (_, skill_motions::Motion::ThirdAttack) => Some(11),
                (_, skill_motions::Motion::PickUp) => Some(actor::PICKUP),
                (_, skill_motions::Motion::Skill) if skill_motions::ready_instead(actor.job) => Some(actor::READY),
                (_, skill_motions::Motion::Skill) => Some(SKILL_ACTION),
            };
            if let Some(action) = action {
                actor.play_at_pace(action, now, pace, false);
                let index = action * 8 + usize::from(actor.direction % 8);
                hit_ms = actor.sprites.as_ref().and_then(|sprites| match action {
                    SKILL_ACTION => sprites.frame_ms(index, 6, pace),
                    _ => sprites.hit_ms(index, pace),
                });
            }
            actor.stand_ready();
        }
        if overhead::shouts(skill, level, monster, packet)
            && let Some(info) = info.get(&skill)
        {
            let unit = (source != me).then_some(source);
            self.say(unit, format!("{} !!", info.name), CHAT_WHITE);
        }
        hit_ms.map(|ms| ms + skill_motions::hit_extra_ms(skill))
    }

    /// Puts a window in front of the others, and at the front of the Esc list when it is open.
    fn raise(&mut self, id: Panel) {
        self.windows.retain(|&window| window != id);
        self.windows.push(id);
        if let Some(esc) = esc_panel(id) {
            if self.panel_open(id) {
                escape::touch(&mut self.esc_order, esc);
            } else {
                escape::remove(&mut self.esc_order, esc);
            }
        }
    }

    fn panel_open(&self, id: Panel) -> bool {
        match id {
            Panel::Hotkeys => self.hotkeys.frame.open,
            Panel::BasicInfo => self.basic_info.frame.open,
            Panel::Inventory => self.inventory.frame.open,
            Panel::Equipment => self.equipment.frame.open,
            Panel::Skills => self.skill_window.frame.open,
            Panel::Storage => self.storage.frame.open,
            Panel::Party => self.party_window.frame.open,
            Panel::Status => self.status_window.frame.open,
            Panel::Guild => self.guild.is_some() && self.guild_window.frame.open,
            Panel::Clan => self.clan.is_some() && self.clan_window.frame.open,
            Panel::Cart => self.cart.cart.is_some() && self.cart.frame.open,
            Panel::Pet => self.pet.is_some() && self.pet_window.frame.open,
            Panel::Mercenary => self.mercenary.is_some() && self.mercenary_window.frame.open,
            Panel::MercenarySkills => self.mercenary.is_some() && self.mercenary_skill_list.frame.open,
            Panel::Homunculus => self.homunculus.is_some() && self.homunculus_window.frame.open,
            Panel::HomunculusSkills => self.homunculus.is_some() && self.homunculus_skill_list.frame.open,
        }
    }

    /// Token of Siegfried (or Light of Regene): the dead menu offers standing up here.
    fn can_resurrect(&self) -> bool {
        escape::can_stand_up(self.inventory.items.iter().map(|item| item.id), self.status_icons.iter().map(|(id, ..)| *id))
    }

    /// Every window Esc may close that is open now, questions included.
    fn esc_open(&self) -> Vec<escape::WindowId> {
        use escape::WindowId::*;
        let mut open = Vec::new();
        let mut push = |id: escape::WindowId, shown: bool| {
            if shown {
                open.push(id);
            }
        };
        for panel in [
            Panel::Inventory,
            Panel::Equipment,
            Panel::Skills,
            Panel::Storage,
            Panel::Party,
            Panel::Status,
            Panel::Guild,
            Panel::Clan,
            Panel::Cart,
            Panel::Pet,
            Panel::Mercenary,
            Panel::MercenarySkills,
            Panel::Homunculus,
            Panel::HomunculusSkills,
        ] {
            if let Some(id) = esc_panel(panel) {
                push(id, self.panel_open(panel));
            }
        }
        // An NPC's window and the warp list aren't Esc's to close: they end with their own buttons.
        // An NPC shop isn't Esc's to close.
        push(Shop, self.shop.as_ref().is_some_and(|shop| !shop.plain()));
        push(Refine, self.refine.is_some());
        push(GradeEnchant, self.grade.is_some());
        push(LaphineSynthesis, self.synthesis.is_some());
        push(LaphineUpgrade, self.upgrade.is_some());
        push(ItemReform, self.reform.is_some());
        push(Enchant, self.enchant.is_some());
        push(Stylist, self.stylist.is_some());
        push(Materials, self.materials.is_some());
        push(PartySetup, self.party_setup.is_some());
        push(Whisper, !self.talks.is_empty());
        push(FriendSetup, self.friend_setup.is_some());
        push(BlockList, self.block_list.is_some());
        push(AwayMessage, self.away_prompt.is_some());
        push(DressRoom, self.dressroom.is_some());
        push(Ranking, self.ranking.is_some());
        push(Reputation, self.reputation_window.is_some());
        push(Navigation, self.navi_window.is_some());
        push(Catalog, self.catalog.is_some());
        push(SkillDescription, self.skill_description.is_some());
        push(Roulette, self.roulette.is_some());
        push(MonsterInfo, self.monster_window.is_some());
        push(Achievements, self.achievement_window.frame.open);
        push(OtherEquipment, self.viewing.is_some());
        push(Bank, self.bank.is_some());
        push(CashShop, self.cashshop.is_some());
        push(PickList, self.pick.is_some());
        push(Trade, self.trade.is_some());
        push(CardPicture, self.card_view.is_some());
        push(ItemInfo, self.item_window.is_some());
        push(ChatRoom, self.room.is_some());
        push(RoomMake, self.room_make.is_some());
        push(EmotionList, self.emotion_list.is_some());
        push(Macros, self.macro_window.is_some());
        push(VendSetup, self.vend_setup.is_some());
        push(MyShop, self.my_shop.is_some());
        push(BuyingStoreSetup, self.buying_setup.is_some());
        push(MyBuyingStore, self.my_buying_store.is_some());
        push(Mailbox, self.mail.list.frame.open);
        push(MailRead, self.mail.letter.is_some());
        push(MailWrite, self.mail.compose.is_some());
        push(AudioSettings, self.audio_window.is_some());
        push(GraphicsSettings, self.graphics_window.is_some());
        push(ShortcutSettings, self.shortcuts_window.is_some());
        push(WorldMap, self.world_map.is_some());
        push(Minimap, self.minimap_window.frame.open);
        push(Quests, self.quest_window.frame.open);
        push(Menu, self.menu.is_some());
        push(Capturing, self.capturing);
        push(Amount, self.amount.is_some());
        push(SaveAsked, self.save_asked);
        push(SkillApplyAsked, self.apply_asked);
        push(BindAsked, self.bind_asked.is_some());
        push(ExpansionAsked, self.expansion_asked.is_some());
        push(LeaderAsked, self.leader_asked.is_some());
        push(DismissAsked, self.dismiss_asked);
        push(HomunculusRename, self.homunculus_rename.is_some());
        push(PetAsked, self.pet_asked.is_some());
        push(CashShopBuy, self.cashshop_buy_asked);
        push(GuildAsked, self.guild_asked.is_some());
        push(GuildInvite, self.guild_invite.is_some());
        push(PartyInvite, self.party_invite.is_some());
        push(FriendAsked, self.friend_asked.is_some());
        push(TradeAsked, self.trade_asked.is_some());
        push(DealChoice, self.deal_choice.is_some());
        push(BattleQueue, self.queue_window.frame.open);
        push(AdoptAsked, self.adopt_asked.is_some());
        push(QueueAsked, self.queue_asked.is_some());
        open
    }

    /// Brings the Esc list in line with what's open.
    fn sync_esc(&mut self) {
        let open = self.esc_open();
        escape::sync(&mut self.esc_order, &open);
    }

    /// `windows` in the control state: the Esc order, newest first, plus a few second names for the same window.
    fn esc_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.esc_order.iter().map(|id| id.name().to_owned()).collect();
        if self.equipment.frame.open && self.equipment.swapping {
            names.push("SwapEquipment".into());
        }
        if self.pick.as_ref().is_some_and(|(list, _)| list.merging) {
            names.push("ItemMerge".into());
        }
        if matches!(self.guild_asked, Some(GuildAsked::Leave)) {
            names.push("LeaveAsked".into());
        }
        if matches!(self.queue_asked, Some(QueueAsked::Lobby(..))) {
            names.push("BattleLobby".into());
        }
        names.extend(self.talks.window_names());
        if let Some(dialog) = &self.dialog {
            names.push(if dialog.warp.is_some() { "WarpList" } else { "NpcDialog" }.into());
        }
        names
    }

    /// A text box inside a window has the caret. The always-on chat line is not one of these.
    fn focused_text(&self) -> Option<escape::WindowId> {
        use escape::WindowId::*;
        if self.away_prompt.is_some() {
            return Some(AwayMessage);
        }
        if self.talks.typing() {
            return Some(Whisper);
        }
        if self.dialog.as_ref().is_some_and(|dialog| dialog.input.is_some()) {
            return Some(if self.dialog.as_ref().unwrap().warp.is_some() { WarpList } else { NpcDialog });
        }
        if self.vend_setup.as_ref().is_some_and(|setup| setup.typing) {
            return Some(VendSetup);
        }
        if self.buying_setup.as_ref().is_some_and(|setup| setup.focus.is_some()) {
            return Some(BuyingStoreSetup);
        }
        if self.mail.compose.as_ref().is_some_and(|compose| compose.field.is_some()) {
            return Some(MailWrite);
        }
        if self.mail.list.typing {
            return Some(Mailbox);
        }
        if self.macro_window.as_ref().is_some_and(|window| window.editing.is_some()) {
            return Some(Macros);
        }
        if self.room_make.is_some() {
            return Some(RoomMake);
        }
        if self.room.is_some() {
            return Some(ChatRoom);
        }
        if self.cashshop.as_ref().is_some_and(|shop| shop.typing) {
            return Some(CashShop);
        }
        if self.bank.as_ref().is_some_and(|bank| bank.typing) {
            return Some(Bank);
        }
        if self.trade.as_ref().is_some_and(|trade| trade.typing) {
            return Some(Trade);
        }
        if self.storage.typing {
            return Some(Storage);
        }
        if self.catalog.as_ref().is_some_and(|catalog| catalog.focus.is_some()) {
            return Some(Catalog);
        }
        if self.minimap_window.naming() {
            return Some(Minimap);
        }
        None
    }

    /// A question is up. An NPC's menu (and the warp list) isn't one: Esc leaves those alone.
    fn question_open(&self) -> bool {
        self.save_asked
            || self.apply_asked
            || self.bind_asked.is_some()
            || self.expansion_asked.is_some()
            || self.leader_asked.is_some()
            || self.dismiss_asked
            || self.homunculus_rename.is_some()
            || self.pet_asked.is_some()
            || self.cashshop_buy_asked
            || self.guild_asked.is_some()
            || self.guild_invite.is_some()
            || self.party_invite.is_some()
            || self.friend_asked.is_some()
            || self.trade_asked.is_some()
            || self.deal_choice.is_some()
            || self.amount.is_some()
            || self.adopt_asked.is_some()
            || self.queue_asked.is_some()
    }

    /// The Apply question's answer: yes sends each pending level, what it needs first; no keeps them pending.
    fn answer_apply(&mut self, yes: bool) {
        self.apply_asked = false;
        if yes {
            for skill in self.skill_window.take_pending() {
                let _ = self.session.skill_up(skill);
            }
        }
    }

    /// Answers the open question with No.
    fn answer_no(&mut self, messages: &[String]) {
        if self.save_asked {
            self.save_asked = false;
            return;
        }
        if self.apply_asked {
            self.apply_asked = false;
            return;
        }
        if self.amount.is_some() {
            self.answer_amount(false, messages);
            return;
        }
        if self.bind_asked.is_some() {
            self.answer_bind(false);
            return;
        }
        if self.expansion_asked.is_some() {
            self.expansion_asked = None;
            let _ = self.session.answer_expansion(false);
            return;
        }
        if self.leader_asked.take().is_some() {
            return;
        }
        if self.dismiss_asked {
            self.answer_dismiss(false, messages);
            return;
        }
        if self.homunculus_rename.is_some() {
            self.answer_homunculus_name(false);
            return;
        }
        if self.pet_asked.is_some() {
            self.answer_pet(false);
            return;
        }
        if self.cashshop_buy_asked {
            self.cashshop_buy_asked = false;
            if let Some((free, items)) = self.cashshop.as_ref().and_then(|shop| shop.buy_request(false)) {
                let _ = self.session.buy_cashshop(free, &items);
            }
            return;
        }
        if let Some(asked) = self.guild_asked.take() {
            if let crate::GuildAsked::Ally { id, .. } = asked {
                let _ = self.session.answer_alliance(id, false);
            }
            return;
        }
        if let Some((guild, _)) = self.guild_invite.take() {
            let _ = self.session.answer_guild(guild, false);
            return;
        }
        if let Some((party, _)) = self.party_invite.take() {
            let _ = self.session.answer_party(party, false);
            return;
        }
        if let Some((account, character, _)) = self.friend_asked.take() {
            let _ = self.session.answer_friend(account, character, false);
            return;
        }
        if self.trade_asked.take().is_some() {
            let _ = self.session.answer_trade(false);
            return;
        }
        if let Some((parent, partner, _, _)) = self.adopt_asked.take() {
            let _ = self.session.answer_adoption(parent, partner, false);
            return;
        }
        if let Some(asked) = self.queue_asked.take() {
            if let QueueAsked::Lobby(name, _) = asked {
                let _ = self.session.answer_lobby(false, &name);
            }
            return;
        }
        self.deal_choice = None;
    }

    /// Closes one window the way its Cancel or close button does, and drops it from the Esc list.
    fn close_esc(&mut self, id: escape::WindowId) {
        use escape::WindowId::*;
        match id {
            Inventory => self.inventory.frame.open = false,
            Equipment => self.equipment.frame.open = false,
            Skills => self.skill_window.frame.open = false,
            Storage => {
                self.storage.close();
                let _ = self.session.close_storage();
            }
            Party => self.party_window.frame.open = false,
            Status => self.status_window.frame.open = false,
            Guild => self.guild_window.frame.open = false,
            Clan => self.clan_window.frame.open = false,
            Cart => self.cart.frame.open = false,
            Pet => self.pet_window.frame.open = false,
            Mercenary => self.mercenary_window.frame.open = false,
            MercenarySkills => self.mercenary_skill_list.frame.open = false,
            Homunculus => self.homunculus_window.frame.open = false,
            HomunculusSkills => self.homunculus_skill_list.frame.open = false,
            NpcDialog | WarpList => {
                if self.dialog.as_ref().is_some_and(|dialog| dialog.menu.is_some()) {
                    self.choose(true);
                } else if let Some(dialog) = self.dialog.take() {
                    let _ = self.session.close_dialog(dialog.npc);
                }
            }
            Shop => self.close_shop(),
            Refine => {
                let _ = self.session.close_refine();
                self.refine = None;
            }
            GradeEnchant => {
                let _ = self.session.close_grade();
                self.grade = None;
            }
            LaphineSynthesis => {
                let _ = self.session.close_synthesis();
                self.synthesis = None;
            }
            LaphineUpgrade => {
                let _ = self.session.close_upgrade();
                self.upgrade = None;
            }
            ItemReform => {
                let _ = self.session.close_reform();
                self.reform = None;
            }
            Enchant => {
                let _ = self.session.close_enchant();
                self.enchant = None;
            }
            Stylist => {
                let _ = self.session.close_stylist();
                self.stylist = None;
            }
            Materials => self.materials = None,
            PartySetup => self.party_setup = None,
            DressRoom => {
                self.dressroom = None;
                self.dress_figure = None;
            }
            Ranking => self.ranking = None,
            Reputation => self.reputation_window = None,
            Navigation => self.navi_window = None,
            Catalog => self.close_catalog(),
            SkillDescription => self.skill_description = None,
            Roulette => self.close_roulette(),
            MonsterInfo => self.monster_window = None,
            Achievements => self.achievement_window.frame.open = false,
            OtherEquipment => self.viewing = None,
            Bank => {
                let _ = self.session.close_bank(self.account_id);
                self.bank = None;
            }
            CashShop => self.close_cashshop(),
            PickList => self.answer_pick(None),
            Trade => {
                let _ = self.session.cancel_trade();
                self.trade = None;
            }
            ItemInfo => {
                self.item_window = None;
                self.card_view = None;
            }
            CardPicture => self.card_view = None,
            ChatRoom => {
                let _ = self.session.leave_chat_room();
            }
            RoomMake => self.room_make = None,
            EmotionList => self.emotion_list = None,
            Macros => {
                if self.macro_window.take().is_some() {
                    macros::save(macros::path().as_deref(), &self.macro_lines);
                }
            }
            VendSetup => {
                let _ = self.session.open_shop("", false, &[]);
                self.vend_setup = None;
            }
            MyShop => {
                let _ = self.session.close_shop();
                self.my_shop = None;
                self.signs.remove(&self.account_id);
            }
            BuyingStoreSetup => {
                let _ = self.session.open_buying_store("", 0, false, &[]);
                self.buying_setup = None;
            }
            MyBuyingStore => {
                let _ = self.session.close_buying_store();
                self.my_buying_store = None;
                self.signs.remove(&self.account_id);
            }
            Mailbox => {
                self.mail.list.frame.open = false;
                let _ = self.session.close_mailbox();
            }
            MailRead => self.mail.letter = None,
            MailWrite => self.mail.compose = None,
            AudioSettings => self.audio_window = None,
            GraphicsSettings => self.graphics_window = None,
            ShortcutSettings => self.shortcuts_window = None,
            WorldMap => self.world_map = None,
            Minimap => {
                if self.minimap_window.naming() {
                    self.minimap_window.cancel_name();
                } else {
                    self.minimap_window.close();
                }
            }
            Quests => self.quest_window.frame.open = false,
            Menu => self.menu = None,
            Capturing => self.capturing = false,
            Amount => self.answer_amount(false, &[]),
            SaveAsked => self.save_asked = false,
            SkillApplyAsked => self.apply_asked = false,
            BindAsked => self.answer_bind(false),
            ExpansionAsked => {
                self.expansion_asked = None;
                let _ = self.session.answer_expansion(false);
            }
            LeaderAsked => {
                self.leader_asked = None;
            }
            DismissAsked => self.answer_dismiss(false, &[]),
            HomunculusRename => self.answer_homunculus_name(false),
            PetAsked => self.answer_pet(false),
            CashShopBuy => self.cashshop_buy_asked = false,
            GuildAsked => {
                self.guild_asked = None;
            }
            GuildInvite => {
                if let Some((guild, _)) = self.guild_invite.take() {
                    let _ = self.session.answer_guild(guild, false);
                }
            }
            PartyInvite => {
                if let Some((party, _)) = self.party_invite.take() {
                    let _ = self.session.answer_party(party, false);
                }
            }
            FriendAsked => {
                if let Some((account, character, _)) = self.friend_asked.take() {
                    let _ = self.session.answer_friend(account, character, false);
                }
            }
            TradeAsked => {
                self.trade_asked = None;
                let _ = self.session.answer_trade(false);
            }
            DealChoice => self.deal_choice = None,
            BattleQueue => self.queue_window.frame.open = false,
            AdoptAsked => {
                if let Some((parent, partner, _, _)) = self.adopt_asked.take() {
                    let _ = self.session.answer_adoption(parent, partner, false);
                }
            }
            QueueAsked => {
                if let Some(crate::QueueAsked::Lobby(name, _)) = self.queue_asked.take() {
                    let _ = self.session.answer_lobby(false, &name);
                }
            }
            Whisper => {
                if self.talks.typing() {
                    self.talks.blur();
                } else {
                    self.talks.close_front();
                }
            }
            FriendSetup => self.friend_setup = None,
            BlockList => self.block_list = None,
            AwayMessage => self.away_prompt = None,
        }
        escape::remove(&mut self.esc_order, id);
        if id == escape::WindowId::Whisper && !self.talks.is_empty() {
            escape::touch(&mut self.esc_order, escape::WindowId::Whisper);
        }
    }

    /// The window under a press, so Esc closes the one the player just touched.
    fn press_esc(&mut self, cursor: [f32; 2]) {
        if let Some(id) = self.esc_at(cursor) {
            escape::touch(&mut self.esc_order, id);
        }
    }

    fn esc_at(&self, cursor: [f32; 2]) -> Option<escape::WindowId> {
        use escape::WindowId::*;
        if self.menu.is_some() {
            return Some(Menu);
        }
        // An NPC shop isn't Esc's to close.
        if self.shop.as_ref().is_some_and(|shop| shop.covers(cursor)) {
            return self.shop.as_ref().filter(|shop| !shop.plain()).map(|_| Shop);
        }
        if self.world_map.is_some() {
            return Some(WorldMap);
        }
        if self.away_prompt.as_ref().is_some_and(|prompt| prompt.hit(cursor).is_some()) {
            return Some(AwayMessage);
        }
        if self.talks.hit(cursor).is_some() {
            return Some(Whisper);
        }
        if self.block_list.as_ref().is_some_and(|list| list.hit(cursor).is_some()) {
            return Some(BlockList);
        }
        if self.friend_setup.as_ref().is_some_and(|setup| setup.hit(cursor).is_some()) {
            return Some(FriendSetup);
        }
        if self.minimap_window.covers(cursor) {
            return Some(Minimap);
        }
        self.window_at(cursor).and_then(esc_panel)
    }

    /// Asks to wear an item.
    fn wear(&mut self, index: u16, location: u32) {
        self.last_wear = Some(control::now());
        let _ = self.session.wear(index, location);
    }

    /// An inventory item let go over the world: dropped, unless something open or a fresh wear holds it back.
    fn drop_on_ground(&mut self, index: u16, messages: &[String]) {
        let since_wear = self.last_wear.map(|at| control::now().saturating_duration_since(at));
        let busy = self.trade.is_some() || self.dialog.is_some() || self.shop.is_some();
        let mail = self.mail.list.frame.open || self.mail.compose.is_some() || self.mail.letter.is_some();
        let ammo = self.inventory.get(index).is_some_and(|item| item.kind == net::ITEM_AMMO && item.worn != 0);
        match ground_drop_refused(since_wear, busy, self.equipment.frame.open, self.storage.frame.open, mail, ammo) {
            None => self.move_item(Asked::Drop(index)),
            Some(Some(id)) => self.chat_line(messages.get(id).cloned().unwrap_or_default(), CHAT_ERROR),
            Some(None) => {}
        }
    }

    /// Uses an inventory item: consumables are used, equipment is put on.
    fn use_item(&mut self, index: u16) {
        if self.store_open() {
            return;
        }
        let Some(item) = self.inventory.get(index) else { return };
        // ponytail: a voucher asks to grow the inventory; how the real client lets the player ask isn't known.
        if VOUCHERS.contains(&item.id) {
            let _ = self.session.ask_expansion();
            return;
        }
        match item.kind {
            // A reform box is a delayed consume: using it only opens the window.
            net::ITEM_HEALING | net::ITEM_USABLE | net::ITEM_DELAY_CONSUME | net::ITEM_CASH => {
                let _ = self.session.use_item(index, self.account_id);
            }
            // An egg (armor worn nowhere) hatches from the egg list only.
            net::ITEM_ARMOR if item.location == 0 => {}
            // An item that binds once worn asks first.
            net::ITEM_WEAPON | net::ITEM_ARMOR | net::ITEM_SHADOW_GEAR if item.identified && item.bind == 1 => self.bind_asked = Some(index),
            net::ITEM_WEAPON | net::ITEM_ARMOR | net::ITEM_SHADOW_GEAR | net::ITEM_PET_ARMOR | net::ITEM_AMMO if item.identified => {
                let location = item.location;
                self.wear(index, location);
            }
            // A card asks what it can go into.
            net::ITEM_CARD => {
                self.card_asked = Some(index);
                let _ = self.session.card_list(index);
            }
            _ => {}
        }
    }

    /// Closes the shop's windows; a market or barter tells the server, which keeps the player busy until it hears.
    fn close_shop(&mut self) {
        if let Some(kind) = self.shop.take().and_then(|shop| shop.market) {
            let _ = self.session.close_market(kind);
        }
    }

    fn close_cashshop(&mut self) {
        if self.cashshop.take().is_some() {
            let _ = self.session.close_cashshop();
        }
        self.cashshop_buy_asked = false;
    }

    /// The answer to a pick list: the row picked, or None to cancel.
    fn answer_pick(&mut self, answer: Option<usize>) {
        let Some((list, picking)) = self.pick.take() else { return };
        let picked = answer.and_then(|row| list.items.get(row));
        let _ = match (picking, picked) {
            (Picking::Card(card), Some(equip)) => self.session.insert_card(card, equip.index),
            // rathena keeps nothing for a card's list, nor for Arrow Crafting's.
            (Picking::Card(_) | Picking::Arrow, None) => Ok(()),
            // The server waits for the Magnifier's answer, a cancel too.
            (Picking::Identify, item) => self.session.identify(item.map(|item| item.index)),
            // A making list unanswered isn't sent again: a cancel answers item 0.
            (Picking::Make, item) => self.session.make(item.map_or(0, |item| item.id)),
            (Picking::Cook(kind), item) => self.session.cook(kind, item.map_or(0, |item| item.id)),
            (Picking::Arrow, Some(item)) => self.session.make_arrow(item.id),
            // Both lists end with an answer, a cancel too.
            (Picking::Repair, item) => self.session.repair(item),
            // Both hold the player until answered; 0 starts nothing.
            (Picking::Spell, skill) => self.session.select_autospell(skill.map_or(0, |skill| skill.id)),
            (Picking::SkillSelect(why), skill) => self.session.select_skill(why, skill.map_or(0, |skill| skill.id as u16)),
            (Picking::Upgrade, item) => self.session.upgrade_weapon(item.map_or(0, |item| u32::from(item.index))),
            // An egg list left unanswered is let go (rathena drops it on the next skill).
            (Picking::Hatch, Some(egg)) => self.session.hatch(egg.index),
            (Picking::Hatch, None) => Ok(()),
            (Picking::Merge, _) => self.session.cancel_merge(),
        };
    }

    /// Answers the bind question: OK wears the item.
    fn answer_bind(&mut self, yes: bool) {
        let Some(index) = self.bind_asked.take().filter(|_| yes) else { return };
        if let Some(location) = self.inventory.get(index).map(|item| item.location) {
            self.wear(index, location);
        }
    }

    /// A ground skill goes out; a Talkie Box carries the message typed for it.
    fn send_skill_at(&mut self, id: u16, level: u16, cell: [u16; 2]) -> std::io::Result<()> {
        match self.talkie_text.take().filter(|_| id == TALKIE_BOX) {
            Some(text) => self.session.use_skill_at_with_text(id, level, cell, &text),
            None => self.session.use_skill_at(id, level, cell),
        }
    }

    /// Closes the roulette's window; the server answers nothing.
    fn close_roulette(&mut self) {
        let _ = self.session.roulette(net::RouletteRequest::Close);
        self.roulette = None;
    }

    /// Opens the pet window (or brings it up) with fresh news of the pet.
    fn open_pet_window(&mut self) {
        self.pet_window.frame.open = true;
        self.raise(Panel::Pet);
        let _ = self.session.pet_command(net::PET_INFO);
    }

    /// A command to the pet; sent back to its egg, it's gone and its window closes.
    fn pet_command(&mut self, command: u8) {
        let _ = self.session.pet_command(command);
        if command == net::PET_RETURN {
            (self.pet_id, self.pet) = (None, None);
            self.pet_window.frame.open = false;
        }
    }

    /// Answers the pet question: OK feeds it, or renames it.
    fn answer_pet(&mut self, yes: bool) {
        let _ = match self.pet_asked.take() {
            Some(PetAsked::Feed) if yes => self.session.pet_command(net::PET_FEED),
            Some(PetAsked::Rename(name)) if yes => self.session.rename_pet(&name),
            _ => Ok(()),
        };
    }

    /// The mercenary is gone (dismissed, its contract over, dead): its windows close. On a map change the server sends
    /// it again, so they stay as they were.
    fn forget_mercenary(&mut self, close: bool) {
        self.mercenary = None;
        self.mercenary_skills.clear();
        self.steering = mercenary::Steering::default();
        self.dismiss_asked = false;
        if close {
            self.mercenary_window.frame.open = false;
            self.mercenary_skill_list.frame.open = false;
        }
    }

    /// Answers the dismiss question: OK lets the mercenary go.
    fn answer_dismiss(&mut self, yes: bool, messages: &[String]) {
        self.dismiss_asked = false;
        if yes && self.mercenary.is_some() {
            let _ = self.session.mercenary_command(net::MERCENARY_DISMISS);
            self.forget_mercenary(true);
            // ponytail: rathena answers nothing; when the official client prints this isn't knowable.
            self.chat_line(messages.get(mercenary::RELEASED).cloned().unwrap_or_default(), CHAT_ITEM);
        }
    }

    /// Sends the mercenary or homunculus `id` at the monster under the cursor, or to the cell there; the order it now
    /// follows, if any.
    /// An Alt press for a companion (the mercenary, or the homunculus): on a monster it attacks it, at once with
    /// Shift, otherwise the first press marks it and a second on the same one sends the attack; on the ground it goes
    /// there. False when that companion isn't out (Alt does nothing special then).
    fn companion_press(&mut self, homunculus: bool, cursor: [f32; 2], shift: bool) -> bool {
        let id = if homunculus { self.homunculus_id.filter(|id| self.units.contains_key(id)) } else { self.mercenary.as_ref().map(|hired| hired.id) };
        let Some(id) = id else { return false };
        let monster = match hovered(&self.hit_boxes, cursor, false) {
            Some(Owner::Unit(target)) if self.units.get(&target).is_some_and(|actor| actor.kind == MONSTER) => Some(target),
            _ => None,
        };
        let order = match monster {
            Some(target) => {
                let (mark, attack) = companion_mark(self.companion_mark, id, target, shift);
                self.companion_mark = mark;
                // ponytail: rathena reads non-zero as "keep attacking" and says the client sends 0; 1 keeps it at it.
                attack.then(|| {
                    let _ = self.session.act_npc(id, target, 1);
                    mercenary::Order::Attack(target)
                })
            }
            None => {
                self.companion_mark = None;
                self.ground_cell(cursor).filter(|&cell| self.world.gat.walkable(cell)).map(|cell| {
                    let _ = self.session.move_npc(id, cell.map(|value| value as u16));
                    mercenary::Order::Move(self.player.cell)
                })
            }
        };
        if let Some(order) = order {
            let steering = if homunculus { &mut self.homunculus_steering } else { &mut self.steering };
            steering.order = Some(order);
        }
        true
    }

    /// Calls the mercenary or homunculus `id` back to the player when `steering` says so; an attack order lasts while
    /// its target is there.
    fn follow_player(&mut self, id: u32, homunculus: bool, now: Instant) {
        let Some(cell) = self.units.get(&id).map(|actor| actor.cell) else { return };
        let steering = if homunculus { &mut self.homunculus_steering } else { &mut self.steering };
        if let Some(mercenary::Order::Attack(target)) = steering.order
            && self.units.get(&target).is_none_or(|actor| actor.remove_at.is_some())
        {
            steering.order = None;
        }
        if steering.should_call(cell, self.player.cell, now) {
            let _ = self.session.move_to_owner(id);
        }
    }

    /// Uses a skill of the mercenary's or the homunculus's.
    fn use_companion_skill(&mut self, id: u16, level: u16) {
        if self.homunculus_skills.iter().any(|skill| skill.id == id) {
            self.use_homunculus_skill(id, level);
        } else {
            self.use_mercenary_skill(id, level);
        }
    }

    /// Uses one of the homunculus's skills: on the homunculus at once when it's a self skill, otherwise aimed.
    fn use_homunculus_skill(&mut self, id: u16, level: u16) {
        let (Some(unit), Some(skill)) = (self.homunculus_id, self.homunculus_skills.iter().find(|skill| skill.id == id)) else { return };
        let level = level.clamp(1, skill.level.max(1));
        if skill.kind & net::Skill::SELF != 0 {
            let _ = self.session.use_skill(id, level, unit);
        } else if skill.kind & (net::Skill::ENEMY | net::Skill::GROUND | net::Skill::FRIEND) != 0 {
            self.targeting = Some((id, level));
        }
    }

    /// The homunculus is gone: its windows close when asked to. On a map change the server sends it again, so they
    /// stay as they were.
    fn forget_homunculus(&mut self, close: bool) {
        self.homunculus = None;
        self.homunculus_id = None;
        self.homunculus_skills.clear();
        self.homunculus_steering = mercenary::Steering::default();
        self.homunculus_rename = None;
        self.homunculus_hunger = None;
        if close {
            self.homunculus_window.frame.open = false;
            self.homunculus_skill_list.frame.open = false;
        }
    }

    /// Answers the rename question: OK names the homunculus.
    fn answer_homunculus_name(&mut self, yes: bool) {
        if let Some(name) = self.homunculus_rename.take().filter(|_| yes) {
            let _ = self.session.rename_homunculus(&name);
        }
    }

    /// Uses one of the mercenary's skills: on the mercenary at once when it's a self skill, otherwise aimed.
    fn use_mercenary_skill(&mut self, id: u16, level: u16) {
        let (Some(mercenary), Some(skill)) = (&self.mercenary, self.mercenary_skills.iter().find(|skill| skill.id == id)) else { return };
        let level = level.clamp(1, skill.level.max(1));
        if skill.kind & net::Skill::SELF != 0 {
            let _ = self.session.use_skill(id, level, mercenary.id);
        } else if skill.kind & (net::Skill::ENEMY | net::Skill::GROUND | net::Skill::FRIEND) != 0 {
            self.targeting = Some((id, level));
        }
    }

    /// Presses a hotkey slot: uses the item it holds (the first stack of it carried).
    fn press_hotkey(&mut self, slot: usize) {
        if let Some(Some(net::Hotkey { skill: true, id, count })) = self.hotkeys.keys.get(slot).copied() {
            return self.use_skill(id as u16, count.max(1) as u16);
        }
        if let Some(Some(net::Hotkey { skill: false, id, .. })) = self.hotkeys.keys.get(slot).copied()
            && let Some(index) = self.inventory.items.iter().find(|item| item.id == id && item.worn == 0).map(|item| item.index)
        {
            self.use_item(index);
        }
    }

    /// Units locked together, turned to stone or frozen hold still on the frame they had when caught; the others move
    /// again.
    fn hold_locked(&mut self) {
        let now = control::now();
        let me = self.account_id;
        let locks = &self.locks;
        let actors = std::iter::once((me, &mut self.player)).chain(self.units.iter_mut().map(|(id, actor)| (*id, actor)));
        for (id, actor) in actors {
            actor.held = (locked_with(locks, id) > 0 || actor::stilled(actor.body_state)).then(|| actor.held.unwrap_or(now));
        }
    }

    fn chat_line(&mut self, text: String, color: [u8; 3]) {
        self.chat.push(text, color);
    }

    /// A chat line of a kind its colour doesn't tell (whispers share the broadcasts' yellow).
    fn chat_line_kind(&mut self, text: String, color: [u8; 3], kind: chat::Kind) {
        self.chat.push_kind(text, color, kind);
    }

    /// Starts the attacker's swing now; the number and the flinch come when the hit lands.
    fn act(&mut self, act: net::Act, tables: &LookTables) {
        // Sitting down and standing up.
        if matches!(act.kind, 2 | 3) {
            let who = if act.source == self.account_id { Some(&mut self.player) } else { self.units.get_mut(&act.source) };
            if let Some(actor) = who {
                actor.sitting = act.kind == 2;
            }
            return;
        }
        // Picking an item up: bend toward it.
        if act.kind == 1 {
            let item = self.ground.get(&act.target).map(|item| item.at);
            let picker = if act.source == self.account_id { Some(&mut self.player) } else { self.units.get_mut(&act.source) };
            if let Some(picker) = picker.filter(|actor| actor.kind == PLAYER) {
                if let Some(cell) = item {
                    picker.face(cell);
                }
                let duration = picker.sprites.as_ref().and_then(|sprites| sprites.action_ms(actor::PICKUP, picker.direction)).unwrap_or(500.0);
                picker.play(actor::PICKUP, Instant::now(), duration, false);
            }
            return;
        }
        // Only hits animate (normal, endure, multi-hit, critical, lucky dodge, ...).
        if !matches!(act.kind, 0 | 4 | 8 | 9 | 10 | 11 | 13) {
            return;
        }
        let me = self.account_id;
        let target_cell = if act.target == me { Some(self.player.cell) } else { self.units.get(&act.target).map(|actor| actor.cell) };
        let source = if act.source == me { Some(&mut self.player) } else { self.units.get_mut(&act.source) };
        let mut hit_ms = None;
        if let Some(source) = source {
            if let Some(cell) = target_cell {
                source.face(cell);
            }
            let attack = match &source.look {
                Some(look) => actor::attack_action(look.job, look.sex, tables.weapon_type(look.weapon)),
                None => actor::combat_actions(source.kind).0,
            };
            let bow = source.look.as_ref().is_some_and(|look| tables.weapon_type(look.weapon) == BOW);
            let scale = actor::attack_scale(act.source_speed, bow);
            source.play_at_pace(attack, Instant::now(), scale, false);
            source.stand_ready();
            hit_ms = source.sprites.as_ref().and_then(|sprites| sprites.hit_ms(attack * 8 + usize::from(source.direction % 8), scale));
        }
        self.show_hit(&act, hit_ms, MULTI_HIT_DELAY);
    }

    /// The numbers of a hit as it lands: split over a multi-hit's hits (with the running total over monsters),
    /// yellow for a critical, "Lucky" for a lucky dodge.
    /// The numbers (and the flinch) of a hit, landing `hit_ms` after now (the attacker's "atk" frame), or after its
    /// attack motion when its sprite doesn't say.
    fn show_hit(&mut self, act: &net::Act, hit_ms: Option<f32>, gap: Duration) {
        let me = self.account_id;
        let target = (act.target != me).then_some(act.target);
        let hit = match target {
            None => Some(&self.player),
            Some(id) => self.units.get(&id),
        };
        // Red on anything but monsters and NPCs: players (us too), homunculi, mercenaries.
        let on_player = hit.is_none_or(|actor| !(actor::is_monster_job(actor.job) || actor::is_npc_job(actor.job)));
        // Hits drift diagonally away from where the target faces.
        let drift = hit.map_or([1.0, 1.0], |actor| actor::number_drift(actor.rot, actor.facing_from_server));
        let amount = act.damage + act.left_damage;
        let lands = Instant::now() + Duration::from_secs_f32(hit_ms.unwrap_or(act.source_speed.max(0) as f32) / 1000.0);
        // Misses, endured hits and lucky dodges don't flinch.
        let flinch = (amount > 0 && !matches!(act.kind, 4 | 9 | 11)).then_some(act.target_speed as f32);
        let kind = match act.kind {
            11 => DamageKind::Lucky,
            10 | 13 => DamageKind::Critical,
            _ if amount <= 0 => {
                // MISS shows over the attacker: white when it's a monster or an NPC, red otherwise.
                let attacker = if act.source == me { Some(&self.player) } else { self.units.get(&act.source) };
                let red = attacker.is_none_or(|actor| !(actor::is_monster_job(actor.job) || actor::is_npc_job(actor.job)));
                let over = (act.source != me).then_some(act.source);
                self.damages.push(Damage { target: over, amount, start: lands, kind: DamageKind::Miss { red }, flinch: None, drift, source: None });
                return;
            }
            _ => DamageKind::Hit { taken: on_player, sp: act.sp },
        };
        let hits = if amount > 0 { i32::from(act.hits.max(1)) } else { 1 };
        for hit in 0..hits {
            let start = lands + gap * hit as u32;
            self.damages.push(Damage { target, amount: amount / hits, start, kind, flinch, drift, source: (amount > 0).then_some(act.source) });
            if hits > 1 && !on_player {
                self.damages.push(Damage { target, amount: amount / hits * (hit + 1), start, kind: DamageKind::Combo, flinch: None, drift, source: None });
            }
        }
    }
}

#[derive(PartialEq)]
enum DialogButton {
    None,
    Next,
    Close,
}

/// A guild question, answered with OK or Cancel.
enum GuildAsked {
    /// Another guild (its name) asks for an alliance, through the player who asked (account ID).
    Ally { id: u32, guild: String },
    /// End the relation with this guild.
    End { id: u32, antagonist: bool },
    /// Leave the guild.
    Leave,
}

/// What a pick list's answer is for.
#[derive(Debug, Clone, Copy)]
enum Picking {
    /// The equipment this card (its inventory index) goes into.
    Card(u16),
    /// What a Magnifier appraises.
    Identify,
    /// What to make (CZ_REQMAKINGITEM), what to cook (CZ_REQ_MAKINGITEM with the list's type), what Arrow
    /// Crafting uses (CZ_REQ_MAKINGARROW).
    Make,
    Cook(u16),
    Arrow,
    /// What Repair Weapon mends, what Upgrade Weapon refines.
    Repair,
    Upgrade,
    /// Hindsight's spell; a skill picked for a reason (Shadow Spell's), which goes back with it.
    Spell,
    SkillSelect(u32),
    /// The egg to hatch.
    Hatch,
    /// Stacks of one item to merge (several picked; OK and Cancel are handled apart).
    Merge,
}

struct Dialog {
    npc: u32,
    lines: Vec<String>,
    button: DialogButton,
    /// Menu items and the highlighted one.
    menu: Option<(Vec<String>, usize)>,
    /// Waiting for a number (or text when the flag is set).
    input: Option<(bool, String)>,
    /// Warp Portal's or Teleport's list rather than an NPC's: the skill and the maps as the server named them.
    warp: Option<(u16, Vec<String>)>,
}

/// A number over a unit: who (None = the player), how much, when it shows, how it looks, and the flinch it causes.
struct Damage {
    target: Option<u32>,
    /// Who hit, until the hit's sound has played (none for misses, heals and totals).
    source: Option<u32>,
    amount: i32,
    start: Instant,
    kind: DamageKind,
    flinch: Option<f32>,
    /// The ground axes' signs (x east, z north) a hit's number drifts along.
    drift: [f32; 2],
}

/// How a number looks and moves.
#[derive(Clone, Copy, PartialEq)]
enum DamageKind {
    /// White, or red on a player, homunculus or mercenary (`taken`); blue when it's SP.
    Hit { taken: bool, sp: bool },
    /// Yellow on the critical burst.
    Critical,
    /// "Lucky": the hit was dodged.
    Lucky,
    /// MISS over the attacker: white from a monster or NPC, red from anyone else.
    Miss { red: bool },
    /// Green HP or blue SP, rising slowly.
    Heal { sp: bool },
    /// The running total of a multi-hit, high above in yellow; each new one replaces the last.
    Combo,
}

impl DamageKind {
    fn duration(self, _amount: i32) -> f32 {
        let ticks = match self {
            DamageKind::Lucky => return actor::LUCKY_MS / 1000.0,
            DamageKind::Miss { .. } => actor::MISS_TICKS,
            DamageKind::Combo => actor::TOTAL_NUMBER_TICKS,
            DamageKind::Heal { .. } => actor::HEAL_NUMBER_TICKS,
            DamageKind::Hit { .. } | DamageKind::Critical => actor::HIT_NUMBER_TICKS,
        };
        ticks * 0.024
    }
}

enum Scene {
    /// The servers clientinfo.xml lists, before the login window when it lists more than one.
    Connections {
        selected: usize,
    },
    Login,
    /// A request is in flight; the inner scene stays on screen without reacting.
    Waiting(Box<Scene>),
    ServerSelect {
        accepted: LoginAccepted,
        selected: usize,
    },
    CharSelect(Box<char_select::Select>),
    /// Making a character, with the select screen to go back to.
    CharMake(Box<char_make::Make>, Box<char_select::Select>),
    Game(Box<Game>),
    /// A message box over the scene it returns to.
    Message(String, Box<Scene>),
    /// A question with OK and Cancel over the scene it returns to; OK goes on with `Then`.
    Confirm(String, Then, Box<Scene>),
    /// Asks for a line of text over the scene it returns to; OK goes on with `Then`.
    Prompt {
        text: String,
        input: String,
        then: Then,
        back: Box<Scene>,
    },
}

/// What a confirmed question or a prompt goes on to do.
enum Then {
    BackToLogin,
    /// Ask for the birthdate that deletes this character.
    AskBirthdate(u32),
    /// Delete this character with the birthdate typed in.
    Delete(u32),
    /// Move a character from one slot to another, with its moves left.
    Move(u8, u8, u16),
    /// Ask whether to rename this character to the name typed in.
    ConfirmName(u32),
    /// Rename this character.
    Rename(u32, String),
}

impl Scene {
    /// The scene drawn as the base layer, under any message box or wait.
    fn base(&self) -> &Scene {
        match self {
            Scene::Waiting(inner) | Scene::Message(_, inner) | Scene::Confirm(_, _, inner) | Scene::Prompt { back: inner, .. } => inner.base(),
            scene => scene,
        }
    }

    fn base_mut(&mut self) -> &mut Scene {
        match self {
            Scene::Waiting(inner) | Scene::Message(_, inner) | Scene::Confirm(_, _, inner) | Scene::Prompt { back: inner, .. } => inner.base_mut(),
            scene => scene,
        }
    }
}

/// Normal, hover and pressed images of a button.
type ButtonImages = [Texture; 3];

struct Assets {
    background: Texture,
    login: Texture,
    /// The save-ID box, off and on.
    save: [Texture; 2],
    service: Texture,
    message: Texture,
    char_select: char_select::Assets,
    char_make: char_make::Assets,
    pin: pin::Assets,
    skills: skills::Assets,
    shop: shop::Assets,
    pick_list: pick_list::Assets,
    storage: storage::Assets,
    trade: trade::Assets,
    refine: refine::Assets,
    grade: grade::Assets,
    laphine: laphine::Assets,
    reform: reform::Assets,
    enchant: enchant::Assets,
    stylist: stylist::Assets,
    bank: bank::Assets,
    cashshop: cashshop::Assets,
    context_menu: menu::Assets,
    weather: weather::Assets,
    dressroom: dressroom::Assets,
    catalog: catalog::Assets,
    skill_descript: skill_descript::Assets,
    pickup: pickup::Assets,
    level_up: level_up::Assets,
    quest_notice: quest_notice::Assets,
    battle_queue: battle_queue::Assets,
    mercenary: mercenary::Assets,
    homunculus: homunculus::Assets,
    party: party::Assets,
    chatroom: chatroom::Assets,
    reputation: reputation::Assets,
    /// The navigation arrows, by direction (south, south-west, … south-east).
    navi_arrows: Vec<Texture>,
    /// Item links' underline in the NPC window.
    link_line: Texture,
    navi_line: Texture,
    /// The magic square under a ground cast.
    magic_target: Option<Texture>,
    escape: escape::Assets,
    options: options::Assets,
    quest: quest::Assets,
    achievement: achievement::Assets,
    status: status::Assets,
    pet: pet::Assets,
    roulette: roulette::Assets,
    item_window: iteminfo::Assets,
    guild: guild::Assets,
    clan: clan::Assets,
    vend: vend::Assets,
    /// The cast bar's fill, the speech bubble's box and border, the aimed skill's box, and status icons' clocks
    /// (white, then orange in the last minute) by step.
    cast_fill: Texture,
    combo_fill: Texture,
    speech_fill: Texture,
    speech_border: Texture,
    clocks: [Vec<Texture>; 2],
    /// The cooldown fan by step, `cooldown_fan`.
    fan: Vec<Texture>,
    connect: ButtonImages,
    ok: ButtonImages,
    cancel: ButtonImages,
    black: Texture,
    white: Texture,
    grey: Texture,
    selection: Texture,
    dialog: Texture,
    menu: Texture,
    next: ButtonImages,
    close: ButtonImages,
    menu_selection: Texture,
    bars: [Texture; 7],
    monster_bars: [Texture; 4],
    chrome: window::Chrome,
    inventory: inventory::Assets,
    equipment: equipment::Assets,
    basic_info: basic_info::Assets,
    mail: mail::Assets,
    chat: chat::Assets,
    minimap: minimap::Assets,
    hotkeys: hotkeys::Assets,
    /// The loading bar's border and trough.
    loading_bar: [Texture; 2],
}

/// A status icon's clock: the part of the 32px square swept clockwise from 12 o'clock by `share` of a turn.
/// A cooldown's fan over a 32 px icon after `passed` of it (0 to 1): black at alpha 170 from that angle clockwise round
/// to 12 o'clock, so it clears clockwise from 12.
fn cooldown_fan(passed: f32) -> RgbaImage {
    RgbaImage::from_fn(32, 32, |x, y| {
        let [dx, dy] = [x as f32 + 0.5 - 16.0, y as f32 + 0.5 - 16.0];
        let turned = (dx.atan2(-dy) / std::f32::consts::TAU).rem_euclid(1.0);
        if turned >= passed { Rgba([0, 0, 0, 170]) } else { Rgba([0; 4]) }
    })
}

fn clock(share: f32, color: [u8; 4]) -> RgbaImage {
    RgbaImage::from_fn(32, 32, |x, y| {
        let [dx, dy] = [x as f32 + 0.5 - 16.0, y as f32 + 0.5 - 16.0];
        let turned = (dx.atan2(-dy) / std::f32::consts::TAU).rem_euclid(1.0);
        if turned < share { Rgba(color) } else { Rgba([0; 4]) }
    })
}

/// The client's dialog frame: a white rounded box with a 1px grey border inset by 2, and a filled content area.
fn dialog_frame([width, height]: [u32; 2], content: [u32; 4], fill: [u8; 3]) -> RgbaImage {
    let rounded = |x: u32, y: u32, [left, top, right, bottom]: [u32; 4], radius: f32| {
        let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
        if x < left || y < top || x >= right || y >= bottom {
            return false;
        }
        let cx = px.clamp(left as f32 + radius, right as f32 - radius);
        let cy = py.clamp(top as f32 + radius, bottom as f32 - radius);
        (px - cx).powi(2) + (py - cy).powi(2) <= radius * radius
    };
    RgbaImage::from_fn(width, height, |x, y| {
        if !rounded(x, y, [0, 0, width, height], 5.0) {
            Rgba([0; 4])
        } else if rounded(x, y, [2, 2, width - 2, height - 2], 5.0) && !rounded(x, y, [3, 3, width - 3, height - 3], 4.0) {
            Rgba([0xC1, 0xC6, 0xC2, 255])
        } else if x >= content[0] && y >= content[1] && x < content[2] && y < content[3] {
            Rgba([fill[0], fill[1], fill[2], 255])
        } else {
            Rgba([255; 4])
        }
    })
}

impl Assets {
    fn load(gpu: &Gpu, grf: &Grf, font: &Font, messages: &[String]) -> Self {
        let login = format!("{UI}login_interface\\");
        let image = |path: String| gpu.load_texture(grf, &path, false);
        let button = |folder: &str, name: &str| ["", "_a", "_b"].map(|suffix| image(format!("{folder}{name}{suffix}.bmp")));
        Self {
            background: gpu.load_texture(grf, &format!("{UI}bgi_temp.bmp"), true),
            login: image(format!("{login}bg_login.tga")),
            save: ["chk_saveoff", "chk_saveon"].map(|name| image(format!("{login}{name}.bmp"))),
            service: image(format!("{login}win_service.bmp")),
            message: image(format!("{UI}win_msgbox.bmp")),
            char_select: char_select::Assets::load(gpu, grf, font, messages),
            char_make: char_make::Assets::load(gpu, grf),
            pin: pin::Assets::load(gpu, grf),
            skills: skills::Assets::load(gpu, grf),
            shop: shop::Assets::load(gpu, grf),
            pick_list: pick_list::Assets::load(gpu, grf),
            storage: storage::Assets::load(gpu, grf),
            trade: trade::Assets::load(gpu, grf),
            refine: refine::Assets::load(gpu, grf),
            grade: grade::Assets::load(gpu, grf),
            laphine: laphine::Assets::load(gpu, grf),
            reform: reform::Assets::load(gpu, grf),
            enchant: enchant::Assets::load(gpu, grf),
            stylist: stylist::Assets::load(gpu, grf),
            bank: bank::Assets::load(gpu, grf),
            cashshop: cashshop::Assets::load(gpu, grf),
            context_menu: menu::Assets::load(gpu),
            weather: weather::Assets::load(gpu, grf),
            dressroom: dressroom::Assets::load(gpu, grf),
            catalog: catalog::Assets::load(gpu, grf),
            skill_descript: skill_descript::Assets::load(gpu),
            pickup: pickup::Assets::load(gpu, grf),
            level_up: level_up::Assets::load(gpu, grf),
            quest_notice: quest_notice::Assets::load(gpu, grf),
            battle_queue: battle_queue::Assets::load(gpu, grf, &lua::entry_queue(grf).unwrap_or_default()),
            mercenary: mercenary::Assets::load(gpu, grf),
            homunculus: homunculus::Assets::load(gpu, grf),
            party: party::Assets::load(gpu, grf),
            chatroom: chatroom::Assets::load(gpu, grf),
            reputation: reputation::Assets::load(gpu, grf),
            navi_arrows: (0..8).map(|turn| image(format!("{UI}navigation_interface\\navi_grid_{turn}.tga"))).collect(),
            link_line: gpu.solid([links::COLOR[0], links::COLOR[1], links::COLOR[2], 0xFF]),
            navi_line: gpu.solid([links::NAVI_COLOR[0], links::NAVI_COLOR[1], links::NAVI_COLOR[2], 0xFF]),
            magic_target: gpu.try_load_texture(grf, "data\\texture\\effect\\magic_target.tga", false).ok(),
            escape: escape::Assets::load(gpu, grf),
            options: options::Assets::load(gpu, grf),
            quest: quest::Assets::load(gpu, grf),
            achievement: achievement::Assets::load(gpu, grf),
            status: status::Assets::load(gpu, grf),
            pet: pet::Assets::load(gpu, grf),
            roulette: roulette::Assets::load(gpu, grf),
            item_window: iteminfo::Assets::load(gpu, grf),
            guild: guild::Assets::load(gpu, grf),
            clan: clan::Assets::load(gpu, grf),
            vend: vend::Assets::load(gpu, grf),
            cast_fill: gpu.solid([0x16, 0xEA, 0x25, 0xFF]),
            combo_fill: gpu.solid([0xFF, 0xA0, 0x00, 0xFF]),
            speech_fill: gpu.solid([0, 0, 0, 102]),
            speech_border: gpu.solid([0x5C, 0x5C, 0x5C, 0xFF]),
            clocks: [[255, 255, 255, 166], [255, 150, 50, 166]]
                .map(|color| (0..=CLOCK_STEPS).map(|step| gpu.texture(&clock(step as f32 / CLOCK_STEPS as f32, color), false)).collect()),
            fan: (0..=CLOCK_STEPS).map(|step| gpu.texture(&cooldown_fan(step as f32 / CLOCK_STEPS as f32), false)).collect(),
            connect: ["normal", "over", "press"].map(|state| image(format!("{login}bt_start_{state}.bmp"))),
            ok: button(UI, "btn_ok"),
            cancel: button(UI, "btn_cancel"),
            black: gpu.solid([0, 0, 0, 255]),
            white: gpu.solid([255; 4]),
            grey: gpu.solid([0x9C, 0x9C, 0x9C, 0xFF]),
            selection: gpu.solid(SELECTION_COLOR),
            dialog: gpu.texture(&dialog_frame(DIALOG_SIZE, DIALOG_CONTENT, [0xEF, 0xF4, 0xF0]), false),
            menu: gpu.texture(&dialog_frame(MENU_SIZE, MENU_CONTENT, [0xF9, 0xF9, 0xF9]), false),
            next: button(UI, "btn_next"),
            close: button(UI, "btn_close"),
            menu_selection: gpu.solid(MENU_SELECTION),
            bars: BAR_COLORS.map(|color| gpu.solid(color)),
            monster_bars: MONSTER_BAR_COLORS.map(|color| gpu.solid(color)),
            chrome: window::Chrome::load(gpu, grf),
            inventory: inventory::Assets::load(gpu, grf),
            equipment: equipment::Assets::load(gpu, grf),
            basic_info: basic_info::Assets::load(gpu, grf),
            mail: mail::Assets::load(gpu, grf),
            chat: chat::Assets::load(gpu, grf),
            minimap: minimap::Assets::load(gpu, grf),
            hotkeys: hotkeys::Assets::load(gpu, grf),
            loading_bar: [gpu.solid([0, 255, 255, 255]), gpu.solid([140, 140, 140, 255])],
        }
    }
}

impl Assets {
    /// Buy, sell and cancel buttons (the shop's).
    fn shop_buttons(&self) -> [&ButtonImages; 3] {
        self.shop.buttons()
    }
}

fn login_position(window: &Texture) -> [f32; 2] {
    [((screen()[0] - window.size[0]) / 2.0).floor(), LOGIN_TOP]
}

fn shortcut_to_bind(key: options::ShortcutKey) -> Option<shortcuts::ShortcutKey> {
    match key {
        options::ShortcutKey::NamedF(number) => shortcuts::function_key(number),
        options::ShortcutKey::Char(character) => Some(shortcuts::ShortcutKey::Char(character.to_ascii_lowercase())),
    }
}

fn battle_key(key: &Key) -> battle_mode::KeyKind {
    match key {
        Key::Named(NamedKey::Enter) => battle_mode::KeyKind::Enter,
        Key::Named(NamedKey::Escape) => battle_mode::KeyKind::Escape,
        Key::Character(text) => {
            let mut characters = text.chars();
            match (characters.next(), characters.next()) {
                (Some(character), None) if character.is_ascii_alphanumeric() => battle_mode::KeyKind::Char(character.to_ascii_lowercase()),
                _ => battle_mode::KeyKind::Other,
            }
        }
        _ => battle_mode::KeyKind::Other,
    }
}

fn shortcut_key(key: &Key) -> Option<options::ShortcutKey> {
    let number = match key {
        Key::Named(NamedKey::F1) => 1,
        Key::Named(NamedKey::F2) => 2,
        Key::Named(NamedKey::F3) => 3,
        Key::Named(NamedKey::F4) => 4,
        Key::Named(NamedKey::F5) => 5,
        Key::Named(NamedKey::F6) => 6,
        Key::Named(NamedKey::F7) => 7,
        Key::Named(NamedKey::F8) => 8,
        Key::Named(NamedKey::F9) => 9,
        Key::Named(NamedKey::F10) => 10,
        Key::Named(NamedKey::F11) => 11,
        Key::Named(NamedKey::F12) => 12,
        Key::Character(text) => {
            let mut characters = text.chars();
            return characters
                .next()
                .filter(|character| !character.is_control() && characters.next().is_none())
                .map(|character| options::ShortcutKey::Char(character.to_ascii_lowercase()));
        }
        _ => return None,
    };
    Some(options::ShortcutKey::NamedF(number))
}

/// Where the player's settings file `name` is kept: Application Support on a Mac, %APPDATA% on Windows, the XDG config
/// folder elsewhere.
/// The windows' places as saved: a line per window, its name, x, y and 1 when collapsed.
fn window_places<'a>(frames: impl Iterator<Item = (&'a str, [f32; 2], bool)>) -> String {
    frames.map(|(name, [x, y], minimized)| format!("{name} {x} {y} {}\n", u8::from(minimized))).collect()
}

/// Reads `window_places`' lines back; a line it can't read is skipped.
fn parse_window_places(text: &str) -> HashMap<&str, ([f32; 2], bool)> {
    text.lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            let name = words.next()?;
            let [x, y] = [words.next()?.parse().ok()?, words.next()?.parse().ok()?];
            Some((name, ([x, y], words.next()? == "1")))
        })
        .collect()
}

/// Whether /zoom is on (saved; off unless turned on).
fn zoom_out_saved() -> bool {
    settings_path("zoomout").and_then(|path| std::fs::read_to_string(path).ok()).is_some_and(|text| text.trim() == "1")
}

/// Opens or closes the big minimap. With no overview picture, says `no_picture` and leaves it closed.
/// ponytail: said when opening is asked and the map has no overview, and when a warp closes an open window
/// because the new map has none. A picture that fails to decode counts as none.
fn toggle_big_minimap(game: &mut Game, gpu: &render::Gpu, no_picture: String) {
    let map = map_key(&game.map);
    let image = game.minimap.clone();
    if !game.minimap_window.toggle(gpu, image.as_ref(), &map) {
        game.chat_line(no_picture, CHAT_ERROR);
    }
    game.sync_esc();
}

/// Whether the basic info and chat windows are kept between runs: always for a player, and for a client the test
/// driver runs (`RUSTNAROK_CLIENT_CONTROL`) only with the dev switch `RUSTNAROK_CLIENT_KEEP_WINDOWS`, so one
/// scenario's windows don't move the next one's.
pub(crate) fn keeps_windows() -> bool {
    std::env::var_os("RUSTNAROK_CLIENT_CONTROL").is_none() || std::env::var_os("RUSTNAROK_CLIENT_KEEP_WINDOWS").is_some()
}

pub(crate) fn settings_path(name: &str) -> Option<std::path::PathBuf> {
    let home = || Some(std::path::PathBuf::from(std::env::var_os("HOME")?));
    let folder = if cfg!(target_os = "macos") {
        home()?.join("Library/Application Support")
    } else if cfg!(windows) {
        std::env::var_os("APPDATA")?.into()
    } else {
        std::env::var_os("XDG_CONFIG_HOME").filter(|folder| !folder.is_empty()).map(Into::into).or_else(|| Some(home()?.join(".config")))?
    };
    Some(folder.join("rustnarok-client").join(name))
}

/// Where the saved login ID is kept.
fn saved_id_path() -> Option<std::path::PathBuf> {
    settings_path("saved_id")
}

fn saved_id() -> Option<String> {
    let id = std::fs::read_to_string(saved_id_path()?).ok()?;
    let id = id.trim();
    (!id.is_empty()).then(|| id.to_owned())
}

/// Keeps the ID for next time, or forgets it.
fn remember_id(id: Option<&str>) {
    let Some(path) = saved_id_path() else { return };
    let _ = match id {
        Some(id) => path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(&path, id)),
        None => std::fs::remove_file(&path),
    };
}

/// A message's last sentence (after its last ". "), or all of it.
fn last_sentence(text: &str) -> &str {
    text.rsplit_once(". ").map_or(text, |(_, last)| last)
}

fn dialog_position(dialog: &Texture) -> [f32; 2] {
    dialog_position_of(dialog.size)
}

fn dialog_position_of(size: [f32; 2]) -> [f32; 2] {
    [((screen()[0] - size[0]) / 2.0).floor(), ((screen()[1] - size[1]) / 1.5).floor()]
}

/// An attack asked for with the mouse.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Attack {
    target: u32,
    /// Kept up (Ctrl or /noctrl); otherwise only while the button is held.
    continuous: bool,
    held: bool,
    /// The attack packet went out.
    sent: bool,
    /// A walk toward it went out (to where), and when a cell in reach was last looked for.
    chased: bool,
    chase_cell: Option<[i32; 2]>,
    last_chase: Option<Instant>,
}

/// What letting go of the button does to an attack.
#[derive(Debug, PartialEq)]
enum Release {
    Keep,
    /// It went out and was only while held: CZ_CANCEL_LOCKON.
    Cancel,
}

impl Attack {
    fn new(target: u32, continuous: bool) -> Self {
        Self { target, continuous, held: true, sent: false, chased: false, chase_cell: None, last_chase: None }
    }

    fn release(&mut self) -> Release {
        if self.continuous {
            return Release::Keep;
        }
        self.held = false;
        if self.sent { Release::Cancel } else { Release::Keep }
    }

    /// Keeps attacking while kept up or held; let go before it reached, one swing.
    fn action(&self) -> u8 {
        if self.continuous || self.held { net::ACT_ATTACK_REPEAT } else { net::ACT_ATTACK }
    }
}

/// An attack packet goes no sooner than this after the last; out of reach, a cell in reach is looked for this often.
const ATTACK_GAP: Duration = Duration::from_millis(1200);
const CHASE_GAP: Duration = Duration::from_millis(450);
/// "Attacks and skills cannot be used when your Weight is over 90%.", "Unable to attack while riding."
const OVERWEIGHT: usize = 307;
const RIDING_NO_ATTACK: usize = 1885;
/// EFST_ALL_RIDING: a mount that can't attack (rathena's All_Riding has NoAttack).
const ALL_RIDING: u16 = 613;
/// /noctrl's lines: on, off.
const NO_CTRL_ON: usize = 717;
const NO_CTRL_OFF: usize = 718;

/// In reach to attack: at most range × 5 + 5.5 world units away (a cell is 5).
fn in_attack_reach(from: [i32; 2], to: [i32; 2], range: i32) -> bool {
    let [dx, dy] = [(from[0] - to[0]) as f32, (from[1] - to[1]) as f32];
    dx.hypot(dy) * 5.0 <= range as f32 * 5.0 + 5.5
}

fn may_attack(last: Option<Instant>, now: Instant) -> bool {
    last.is_none_or(|last| now.saturating_duration_since(last) >= ATTACK_GAP)
}

/// Why an attack can't start: weight at 90% or more, riding a mount that can't attack.
fn attack_refused(weight: i64, max_weight: i64, riding: bool) -> Option<usize> {
    if max_weight > 0 && weight * 10 >= max_weight * 9 {
        Some(OVERWEIGHT)
    } else if riding {
        Some(RIDING_NO_ATTACK)
    } else {
        None
    }
}

/// The player menu opens on the right release only for a still click: the pointer not moved at all, no Shift, the
/// left button up, off the windows.
fn menu_opens(moved: bool, shift: bool, left_down: bool, on_window: bool) -> bool {
    !(moved || shift || left_down || on_window)
}

/// An Alt press for a companion on a monster: the mark left (companion, monster), and whether the attack goes now
/// (with Shift, or a second press on the marked one).
fn companion_mark(mark: Option<(u32, u32)>, companion: u32, target: u32, shift: bool) -> (Option<(u32, u32)>, bool) {
    if shift || mark == Some((companion, target)) { (None, true) } else { (Some((companion, target)), false) }
}

/// What an aimed skill could land on: the player, a monster, another player.
#[derive(Clone, Copy)]
enum AimAt {
    Me,
    Monster,
    Player,
}

/// A skill's range in cells at `level`: its range by level (1 where the table has none) plus what the skill list adds
/// to it at the learned level; the list's range when the table has nothing.
fn skill_range(per_level: &[u16], level: u16, learned: u16, listed: u16) -> f32 {
    if per_level.is_empty() {
        return f32::from(listed.max(1));
    }
    let at = |level: u16| per_level.get(usize::from(level.max(1)) - 1).map_or(1.0, |&range| f32::from(range));
    at(level) + f32::from(listed) - at(learned)
}

/// Whether a skill reaches from `from` to `to` (cells, drawn positions): within range × 5 + 5.5 world units.
fn skill_in_reach(from: [f32; 2], to: [f32; 2], range: f32) -> bool {
    f32::hypot(to[0] - from[0], to[1] - from[1]) * world::CELL <= range * world::CELL + 5.5
}

/// Whether a press while aiming takes the unit: a monster with Shift up and another player with Shift down, the other
/// way round for a support skill (standing in for the game's `IsPlayerSkill`). `/noshift` drops the Shift rule and a
/// PvP map lets players in. Yourself only for a support skill, by the players' rule.
fn aim_takes(at: AimAt, support: bool, shift: bool, no_shift: bool, pvp: bool) -> bool {
    let players = no_shift || shift != support;
    match at {
        AimAt::Monster => no_shift || shift == support,
        AimAt::Player => players || pvp,
        AimAt::Me => support && players,
    }
}

/// What a press while aiming uses the skill on: for a ground skill the cell under the pointer (a unit there doesn't
/// count), for the others the unit under it. None: the press lets go of the aim.
fn aim_use(ground: bool, unit: Option<u32>, cell: Option<[i32; 2]>) -> Option<SkillTarget> {
    if ground { cell.map(SkillTarget::Cell) } else { unit.map(SkillTarget::Unit) }
}

/// The level a wheel turn picks while aiming (rows > 0 is down, lowering it): 1 to the learned level, and only for
/// skills whose level can be picked.
fn aim_level(level: u16, learned: u16, rows: i32, separate: bool) -> u16 {
    if !separate {
        return level;
    }
    (i32::from(level) - rows).clamp(1, i32::from(learned.max(1))) as u16
}

/// Talking needs the player standing still (or sitting): not walking, not attacking.
fn may_talk(moving: bool, attacking: bool, sitting: bool) -> bool {
    sitting || !(moving || attacking)
}

/// An item can be picked up from the next cell, diagonals too.
fn within_pickup(from: [i32; 2], at: [i32; 2]) -> bool {
    (at[0] - from[0]).abs().max((at[1] - from[1]).abs()) <= 1
}

/// Where a walk to pick up an item at `item` goes: the cell next to it one step toward the player (the item's own when
/// the player is on it). When that can't be walked on, the first of its neighbours that can: right, right and down,
/// right and up, left, left and down, left and up, down, up (+y up).
fn pickup_spot(player: [i32; 2], item: [i32; 2], walkable: impl Fn([i32; 2]) -> bool) -> [i32; 2] {
    const AROUND: [[i32; 2]; 8] = [[1, 0], [1, 1], [1, -1], [-1, 0], [-1, 1], [-1, -1], [0, 1], [0, -1]];
    let toward = [item[0] + (player[0] - item[0]).signum(), item[1] + (player[1] - item[1]).signum()];
    if walkable(toward) {
        return toward;
    }
    AROUND.iter().map(|step| [item[0] + step[0], item[1] + step[1]]).find(|&cell| walkable(cell)).unwrap_or(toward)
}

/// What a pending pickup does this frame.
#[derive(Debug, PartialEq)]
enum PickStep {
    PickUp,
    Walk,
    Wait,
}

/// A pending pickup: picked up once the player's cell is next to the item, walking or not; out of reach, a walk
/// there unless the last one already went there.
fn pickup_step(player: [i32; 2], item: [i32; 2], walked_to: Option<[i32; 2]>) -> PickStep {
    if within_pickup(player, item) {
        PickStep::PickUp
    } else if walked_to != Some(item) {
        PickStep::Walk
    } else {
        PickStep::Wait
    }
}

/// The cursor over an NPC: a warp portal's, or talking.
fn npc_cursor(job: u16) -> cursor::Kind {
    if job == WARP_JOB { cursor::Kind::Warp } else { cursor::Kind::Talk }
}

/// A right click on an inventory item with a modifier: Alt sends the stack on, Shift moves it into the favourites or
/// out, Ctrl identifies it. None: no modifier (the description).
#[derive(Clone, Copy, Debug, PartialEq)]
enum InventoryRight {
    QuickMove,
    Favorite,
    Identify,
}

fn inventory_right(alt: bool, shift: bool, ctrl: bool) -> Option<InventoryRight> {
    if alt {
        Some(InventoryRight::QuickMove)
    } else if shift {
        Some(InventoryRight::Favorite)
    } else if ctrl {
        Some(InventoryRight::Identify)
    } else {
        None
    }
}

/// With compare on, the worn items that share a slot with the inventory item under the pointer.
fn compared_windows(inventory: &Inventory, items: &HashMap<u32, ItemInfo>, messages: &[String], cursor: [f32; 2]) -> Vec<iteminfo::ItemWindow> {
    if !inventory.compare || !inventory.frame.open {
        return Vec::new();
    }
    let Some((index, area)) = inventory.item_boxes().into_iter().find(|(_, area)| contains([area[0], area[1]], [area[2], area[3]], cursor)) else {
        return Vec::new();
    };
    let Some(hovered) = inventory.get(index) else { return Vec::new() };
    inventory::compared_worn(hovered, &inventory.items)
        .into_iter()
        .enumerate()
        .map(|(place, item)| {
            let damaged = item.broken.then(|| messages.get(ITEM_DAMAGED).cloned().unwrap_or_default());
            let mut window = iteminfo::ItemWindow::new(iteminfo::Shown::of(item, damaged));
            window.worn = true;
            window.place_beside(area, place, items);
            window
        })
        .collect()
}

/// Where Alt + right click sends a stack: the first that's open of the storage, a trade, the cart.
#[derive(Clone, Copy, Debug, PartialEq)]
enum QuickTo {
    Storage,
    Trade,
    Cart,
}

fn quick_target(storage: bool, trade: bool, cart: bool) -> Option<QuickTo> {
    [(storage, QuickTo::Storage), (trade, QuickTo::Trade), (cart, QuickTo::Cart)].into_iter().find(|(open, _)| *open).map(|(_, to)| to)
}

/// An item's link on the chat line, as the chat shows links.
// ponytail: the tag the chat already reads for item links; the official client's own link isn't knowable.
fn item_link(id: u32, name: &str) -> String {
    format!("<ITEM>{name}<INFO>{id}</INFO></ITEM>")
}

/// Magnifier and Novice Magnifier (rathena's ITEMID_MAGNIFIER and ITEMID_NOVICE_MAGNIFIER).
const MAGNIFIERS: [u32; 2] = [611, 12325];
/// "You don't have any Magnifier."
const NO_MAGNIFIER: usize = 2735;

/// What a left click on the world does (`Game::world_click`).
enum WorldClick {
    /// A warp portal's cell.
    Warp([i32; 2]),
    Talk(u32),
    Attack(u32),
    Swing(u32),
    PickUp(u32),
    Ground,
}

/// CZ_REQUEST_MOVE goes no sooner than this after the last one; a walk asked for sooner is dropped. The held button
/// tries again this often.
const MOVE_GAP: Duration = Duration::from_millis(300);
/// Two presses this close, the pointer not moved at all between them, make a double press.
const DOUBLE_PRESS: Duration = Duration::from_millis(500);

fn may_move(last: Option<Instant>, now: Instant) -> bool {
    last.is_none_or(|last| now.saturating_duration_since(last) >= MOVE_GAP)
}

/// Whether the held button walks toward the pointer now: held, nothing pending (an item being picked up), and
/// MOVE_GAP since the last try.
fn follows_pointer(held: bool, pending: bool, last_try: Option<Instant>, now: Instant) -> bool {
    held && !pending && last_try.is_none_or(|last| now.saturating_duration_since(last) > MOVE_GAP)
}

/// A press now, after the last one counted (when, where, and whether the pointer moved off since): whether it's a
/// double press, and the press the next one counts from. Presses come in pairs: after a double press the next one
/// starts afresh, so pressing on in one spot acts on every other press.
fn press_pair(last: Option<(Instant, [f32; 2], bool)>, now: Instant, cursor: [f32; 2]) -> (bool, Option<(Instant, [f32; 2], bool)>) {
    let double = double_press(last, now);
    (double, (!double).then_some((now, cursor, false)))
}

/// Whether the button held down after a double press on the ground goes on following the pointer: as after a first
/// press, not with Ctrl or Shift, nor sitting.
// ponytail: only when the double press was on the ground.
fn double_holds(ctrl: bool, shift: bool, sitting: bool) -> bool {
    !ctrl && !shift && !sitting
}

/// Whether a press now, after the last one (when, and whether the pointer moved since), is a double press.
fn double_press(last: Option<(Instant, [f32; 2], bool)>, now: Instant) -> bool {
    last.is_some_and(|(at, _, moved)| !moved && now.saturating_duration_since(at) < DOUBLE_PRESS)
}

/// How far the pointer may wander from a press and still count as still there (a double click, a click not a
/// drag), in canvas pixels: a hand's tremor on a mouse or trackpad, which a scaled window turns into fractions.
// ponytail: an estimate; one canvas pixel was too strict on a scaled window.
const POINTER_SLACK: f32 = 2.0;

/// Whether the pointer at `to` has moved off a press at `from`.
fn moved_off(from: [f32; 2], to: [f32; 2]) -> bool {
    (to[0] - from[0]).abs().max((to[1] - from[1]).abs()) >= POINTER_SLACK
}

fn offset(origin: [f32; 2], offset: [f32; 2]) -> [f32; 2] {
    [origin[0] + offset[0], origin[1] + offset[1]]
}

fn contains(position: [f32; 2], size: [f32; 2], point: [f32; 2]) -> bool {
    (0..2).all(|axis| point[axis] >= position[axis] && point[axis] < position[axis] + size[axis])
}

/// The button image for the pointer state `(position, mouse down)`.
fn button_sprite(pointer: ([f32; 2], bool), position: [f32; 2], images: &ButtonImages) -> Sprite<'_> {
    let image = match (contains(position, images[0].size, pointer.0), pointer.1) {
        (true, true) => &images[2],
        (true, false) => &images[1],
        (false, _) => &images[0],
    };
    (image, position, image.size)
}

/// What circles a unit: spirit spheres (or coins), Millennium Shield shields, charms (element 1-4 and how many) and
/// souls.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Orbs {
    spheres: u16,
    shields: u16,
    charms: (u16, u16),
    souls: u16,
}

/// Changes what circles a unit; a unit left with nothing leaves the map.
fn change_orbs(spirits: &mut HashMap<Option<u32>, Orbs>, unit: Option<u32>, change: impl FnOnce(&mut Orbs)) {
    let orbs = spirits.entry(unit).or_default();
    change(orbs);
    if orbs.charms.1 == 0 {
        orbs.charms = (0, 0);
    }
    if *orbs == Orbs::default() {
        spirits.remove(&unit);
    }
}

/// The Royal Guard line, whose rage spheres rathena sends as shields (and as spheres when one comes into view).
// ponytail: both packets go into the one sphere count for these jobs.
fn guard_job(job: u16) -> bool {
    matches!(job, 4066 | 4073 | 4082 | 4083 | 4102 | 4110 | 4258 | 4281)
}

/// Where `count` things circling a body at `height` are now: each one's center, and whether it's on the near side.
fn orbit(count: u16, height: f32, turned: f32) -> impl Iterator<Item = ([f32; 2], bool)> {
    (0..count).map(move |place| {
        let angle = turned * SPIRIT_TURN + f32::from(place) * std::f32::consts::TAU / f32::from(count);
        ([angle.cos() * SPIRIT_RADIUS, -height + angle.sin() * SPIRIT_RADIUS * SPIRIT_TILT], angle.sin() > 0.0)
    })
}

/// Jobs whose spirits are coins: Gunslinger, Rebellion, their baby forms, Night Watch; anyone else's are spheres.
// ponytail: inferred from who has Coin Flip; the server doesn't say.
fn coin_job(job: u16) -> bool {
    matches!(job, 24 | 4215 | 4228 | 4229 | 4306)
}

/// First menu row shown, scrolled so the highlighted item stays visible.
fn menu_first_row(selected: usize, count: usize) -> usize {
    selected.saturating_sub(MENU_ROWS - 1).min(count.saturating_sub(MENU_ROWS))
}

/// Where the client opens NPC windows.
fn npc_box_position() -> [f32; 2] {
    [(screen()[0] / 3.0).max(20.0).floor(), (screen()[1] / 2.0 - 200.0).max(100.0).floor()]
}

fn menu_position() -> [f32; 2] {
    [(screen()[0] / 3.0).max(20.0).floor(), (screen()[1] / 2.0 + 76.0).max(376.0).floor()]
}

/// Splits NPC text into lines that fit `width`; `^RRGGBB` switches color. Returns colored pieces per line.
/// A message box's lines: the client's messages break lines with runs of spaces; each part wraps at word ends.
fn message_lines(font: &text::Font, text: &str, width: f32) -> Vec<String> {
    text.split("  ")
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .flat_map(|part| wrap_colored(font, part, width).into_iter().map(|pieces| pieces.into_iter().map(|(_, piece)| piece).collect::<String>()))
        .collect()
}

/// Links in NPC text (`<NAVI>name<INFO>where</INFO></NAVI>`, `<ITEM>`, `<ITEMLINK>`, `<URL>` alike) as the client
/// shows them: the name alone in the link's color, then the color before it again.
// ponytail: no underline and no clicking (navigation, item info) yet.
fn show_links(text: &str) -> String {
    // Item and navigation links are marked runs by now (links::mark); URLs are only coloured.
    const LINKS: [(&str, &str); 1] = [("URL", "0070C0")];
    let mut shown = String::new();
    let mut rest = text;
    'scan: while let Some(open) = rest.find('<') {
        for (tag, color) in LINKS {
            let start = format!("<{tag}>");
            let end = format!("</{tag}>");
            if let Some(inner) = rest[open..].strip_prefix(start.as_str())
                && let Some(close) = inner.find(end.as_str())
            {
                let name = inner[..close].split("<INFO>").next().unwrap_or_default();
                shown.push_str(&rest[..open]);
                // The color in force before the link comes back after it.
                let before = shown
                    .rmatch_indices('^')
                    .find_map(|(at, _)| shown.get(at + 1..at + 7).filter(|code| code.chars().all(|digit| digit.is_ascii_hexdigit())).map(str::to_owned));
                shown.push_str(&format!("^{color}{name}^{}", before.unwrap_or_else(|| "000000".to_owned())));
                rest = &inner[close + end.len()..];
                continue 'scan;
            }
        }
        shown.push_str(&rest[..=open]);
        rest = &rest[open + 1..];
    }
    shown.push_str(rest);
    shown
}

fn wrap_colored(font: &text::Font, text: &str, width: f32) -> iteminfo::ColoredLines {
    wrap_linked(font, text, width).into_iter().map(|pieces| pieces.into_iter().map(|(color, piece, _)| (color, piece)).collect()).collect()
}

/// Colour-coded text wrapped to `width`, item links in their colour and each piece with the link it's part of.
fn wrap_linked(font: &text::Font, text: &str, width: f32) -> links::LinkedLines {
    let mut wrapped: links::LinkedLines = vec![Vec::new()];
    let mut line_width = 0.0;
    let mut color = [0, 0, 0];
    for (part, item) in links::pieces(text) {
        for (row, pieces) in wrap_plain(font, &part, width, line_width, &mut color).into_iter().enumerate() {
            if row > 0 {
                wrapped.push(Vec::new());
                line_width = 0.0;
            }
            for (color, piece) in pieces {
                line_width += font.width(&piece);
                let color = item.as_ref().map_or(color, links::Target::color);
                wrapped.last_mut().unwrap().push((color, piece, item.clone()));
            }
        }
    }
    wrapped
}

/// Colour-coded text wrapped to `width`, its first line already `used` wide and `color` in force (left as the text
/// leaves it).
fn wrap_plain(font: &text::Font, text: &str, width: f32, used: f32, color: &mut [u8; 3]) -> iteminfo::ColoredLines {
    let mut lines = vec![Vec::new()];
    let mut line_width = used;
    let mut rest = text;
    while !rest.is_empty() {
        if let Some(code) =
            rest.strip_prefix('^').filter(|code| code.len() >= 6 && code.is_char_boundary(6) && code[..6].chars().all(|digit| digit.is_ascii_hexdigit()))
        {
            let value = u32::from_str_radix(&code[..6], 16).unwrap();
            *color = [(value >> 16) as u8, (value >> 8) as u8, value as u8];
            rest = &code[6..];
            continue;
        }
        // Next word (with its leading space) up to the next space or color code.
        let first = rest.chars().next().map_or(1, char::len_utf8);
        let end = rest[first..].find([' ', '^']).map_or(rest.len(), |index| index + first);
        let word = &rest[..end];
        let word_width = font.width(word);
        if line_width + word_width > width && line_width > 0.0 {
            lines.push(Vec::new());
            line_width = 0.0;
        }
        let word = if line_width == 0.0 { word.trim_start() } else { word };
        lines.last_mut().unwrap().push((*color, word.to_owned()));
        line_width += font.width(word);
        rest = &rest[end..];
    }
    lines
}

// ponytail: placeholder wording; the client's own messages live in msgstringtable.csv, map them once observed.
/// The client message for a login refusal (AC_REFUSE_LOGIN) or a ban notice (SC_NOTIFY_BAN).
fn login_error_message(error: &LoginError) -> usize {
    match *error {
        LoginError::Refused(code) => match code {
            0 => 6,
            1 => 267,
            2 => 8,
            3 => 3,
            4 => 266,
            5 => 310,
            6 => 449,
            7 => 439,
            8 => 681,
            9..=15 => 703 + code as usize - 9,
            17 => 1380,
            18 => 1381,
            19 => 1751,
            20 => 1752,
            21 => 1786,
            22..=27 | 30 => 3440,
            28 => 2731,
            29 => 2732,
            33 => 2682,
            36 => 3453,
            99 => 368,
            100 => 809,
            101 => 810,
            102 => 811,
            103 => 859,
            104 => 860,
            105 => 1372,
            106 => 1293,
            108 => 1393,
            109 => 1394,
            110 => 2345,
            112 => 2410,
            242 => 3439,
            243 => 3687,
            244 => 4190,
            _ => 9,
        },
        LoginError::Notified(code) => match code {
            1 => 4,
            2 => 5,
            3 => 241,
            4 => 264,
            5 => 305,
            6 => 764,
            8 => 440,
            _ => 3,
        },
        // "Failed to connect to the server."
        LoginError::Io(_) => 1,
    }
}

/// What a frame shows: images plus text to rasterize (with color), drawn in that order per layer.
#[derive(Default)]
struct Layer<'a> {
    sprites: Vec<Sprite<'a>>,
    texts: Vec<(String, [f32; 2], [u8; 3])>,
    /// How see-through it is drawn, 0 (opaque) to 1: a window being dragged.
    faded: f32,
}

impl Layer<'_> {
    /// White text with a black outline, as the client draws names and chat over the world.
    fn outlined(&mut self, text: &str, position: [f32; 2]) {
        self.outlined_in(text, position, CHAT_WHITE);
    }

    fn outlined_in(&mut self, text: &str, position: [f32; 2], color: [u8; 3]) {
        for [dx, dy] in [[-1.0, 0.0], [1.0, 0.0], [0.0, -1.0], [0.0, 1.0]] {
            self.texts.push((text.to_owned(), offset(position, [dx, dy]), [0, 0, 0]));
        }
        self.texts.push((text.to_owned(), position, color));
    }
}

struct App {
    grf: Grf,
    /// NPC and monster sprite names by job ID, from the client's Lua tables.
    job_names: HashMap<u16, String>,
    /// Sprite scale by monster job. A job that isn't listed is drawn at 1×.
    monster_sizes: HashMap<u16, f32>,
    /// Shadow width by sprite job, in cells (`actor::shadow_factor`).
    shadow_factors: HashMap<u16, f32>,
    /// Pets' tables and what they say.
    pets: pet::Data,
    /// The achievements (the translation's achievements.lub) and the titles they give, by ID.
    achievement_info: HashMap<u32, lua::AchievementInfo>,
    /// Laphine boxes by the box's item ID.
    synthesis_boxes: HashMap<u32, lua::SynthesisBox>,
    upgrade_boxes: HashMap<u32, lua::UpgradeBox>,
    title_names: HashMap<u32, String>,
    /// Item names and resources by item ID, and the client's numbered messages.
    items: HashMap<u32, ItemInfo>,
    /// The battlegrounds the queue window lists.
    battlegrounds: Vec<lua::Battleground>,
    /// The cards' words in item names.
    card_names: inventory::CardNames,
    /// Card illustration names (`num2cardillustnametable.txt`).
    card_pictures: iteminfo::CardPictures,
    /// What each reform item can turn into, from the client's reform list.
    reforms: HashMap<u32, Vec<lua::ReformInfo>>,
    /// Enchant tables from the client's Lua, by table number.
    enchant_tables: HashMap<u64, lua::EnchantTable>,
    /// The styling shop's rows (stylingshopinfo). Empty when the files aren't in the data.
    styling_shop: lua::StylingShop,
    messages: Vec<String>,
    /// Item icons by file, loaded when first shown.
    icons: HashMap<String, Option<Texture>>,
    modifiers: ModifiersState,
    /// Maps with the indoor camera.
    indoor: HashSet<String>,
    /// Maps with their own camera table; the view the player left; whether it's written (not when driven).
    viewpoints: HashMap<String, rustnarok_client::map::Viewpoint>,
    saved_view: SavedView,
    save_view: bool,
    look_tables: LookTables,
    /// Job names shown to players.
    pc_job_names: HashMap<u16, String>,
    /// Maps' display names, and the world maps with their box colors.
    map_names: HashMap<String, String>,
    worlds: (Vec<rustnarok_client::lua::World>, [[u8; 4]; 2]),
    /// Skill names and icons, the skill window's layout, and status icon files (the client's Lua tables).
    skill_info: HashMap<u16, SkillInfo>,
    /// Skills' descriptions, line by line.
    skill_descriptions: HashMap<u16, Vec<String>>,
    skill_trees: SkillTrees,
    /// The client's reputation groups and reputations.
    reputation_tables: reputation::Tables,
    /// The client's navigation tables: maps, the ways between them, NPCs and monsters.
    navi_tables: lua::NaviTables,
    status_icon_files: HashMap<u16, (u8, String)>,
    /// What each status icon says when pointed at.
    state_infos: HashMap<u16, lua::StateInfo>,
    /// Which effects things play.
    effect_table: effects::Table,
    /// Quests' titles, descriptions and summaries.
    quest_texts: HashMap<u32, quest::QuestText>,
    /// Sound out, the player's sound settings, and the music each map plays.
    audio: Option<sound::Audio>,
    sound: sound::Settings,
    music: HashMap<String, String>,
    /// /effect: effects are drawn and heard; off, they still play, unseen and silent.
    graphics: options::GraphicsSettings,
    shortcuts: options::ShortcutSettings,
    /// Keys to actions, from the game's hotkey tables and the player's UserKeys file.
    keybinds: shortcuts::Shortcuts,
    /// Print Screen writes the next frame here and keeps running.
    print_screen: Option<std::path::PathBuf>,
    /// The file a screenshot was just saved to, for its chat line.
    shot_saved: Option<String>,
    cursor_look: cursor::Cursor,
    /// The client's loading pictures, and the loading screen while it's up.
    loading_images: Vec<String>,
    loading: Option<Loading>,
    font: Font,
    proxy: EventLoopProxy<NetEvent>,
    gpu: Option<Gpu>,
    assets: Option<Assets>,
    scene: Scene,
    char_session: Option<CharSession>,
    /// The login servers the data folder's clientinfo.xml lists, and the one picked.
    connections: Vec<rustnarok_client::clientinfo::Connection>,
    connection: usize,
    /// Which char server we picked (going back to character select reconnects to it), and leaving was agreed.
    char_server: usize,
    quitting: bool,
    /// The game went back to character select at least once.
    returned: bool,
    /// The slot of the character last played, selected again back at character select.
    played_slot: Option<usize>,
    /// The login that's in progress: what the char and map servers need to see.
    account: Option<LoginAccepted>,
    /// The character chosen to play, until its map is entered.
    playing: Option<Character>,
    /// Connected to the map server, waiting to enter: session and map name; and what the server sent meanwhile (the
    /// achievements, at login), handled once the game is on.
    entering: Option<(MapSession, String)>,
    early_events: Vec<MapEvent>,
    /// The server's latest tick and when it came, to place other units' walks in time.
    server_clock: Option<(u32, Instant)>,
    fields: [String; 2],
    /// Remember the ID for next time (the login window's save box).
    save_id: bool,
    focus: usize,
    cursor: [f32; 2],
    mouse_down: bool,
    /// Dev switch RUSTNAROK_CLIENT_CLICKS: when (after entering the map), where, and where to drag to, last first.
    auto_input: Vec<ScriptedInput>,
    blink_start: Instant,
    drawn_blink_phase: u128,
    started: Instant,
    screenshot: Option<String>,
    /// The character select's dev switches (RUSTNAROK_CLIENT_MAKE, RUSTNAROK_CLIENT_DELETE) ran.
    scripted: bool,
}

impl App {
    /// A map's music, as mp3nametable.txt names it, or off the maps the title's (bgm\01.mp3).
    fn play_music(&mut self, map: Option<&str>) {
        let Some(audio) = &mut self.audio else { return };
        let file = map.and_then(|map| self.music.get(&map_key(map))).map_or("bgm\\01.mp3", String::as_str);
        audio.play_music(Some(&sound::music_path(file)));
    }

    fn blink_phase(&self) -> u128 {
        self.blink_start.elapsed().as_millis() / CARET_BLINK.as_millis()
    }

    /// Milliseconds since the client started, as the server's tick packets expect.
    fn client_tick(&self) -> u32 {
        self.started.elapsed().as_millis() as u32
    }

    fn wait(&mut self) {
        let scene = std::mem::replace(&mut self.scene, Scene::Login);
        self.scene = Scene::Waiting(Box::new(scene));
    }

    fn message(&mut self, text: impl Into<String>, back: Scene) {
        self.scene = Scene::Message(text.into(), Box::new(back));
    }

    fn submit_login(&mut self) {
        if self.fields[0].is_empty() {
            return;
        }
        self.wait();
        let proxy = self.proxy.clone();
        let [id, password] = self.fields.clone();
        let (server, version) = self.login_server();
        // The connection's langtype (RUSTNAROK_CLIENT_LANGTYPE overrides it) picks the code page of chat and names.
        let langtype = std::env::var("RUSTNAROK_CLIENT_LANGTYPE").ok().and_then(|langtype| langtype.parse().ok());
        net::set_codepage(rustnarok_client::clientinfo::codepage(
            langtype.unwrap_or_else(|| self.connections.get(self.connection).map_or(0, |connection| connection.langtype)),
        ));
        std::thread::spawn(move || {
            let _ = proxy.send_event(NetEvent::Login(net::login(&server, version, &id, &password)));
        });
    }

    /// Where to log in, and the client version to say: RUSTNAROK_CLIENT_SERVER, else the connection picked from
    /// clientinfo.xml, else the local default.
    fn login_server(&self) -> (String, u32) {
        let connection = self.connections.get(self.connection);
        let version = connection.map_or(0, |connection| connection.version);
        let server = std::env::var("RUSTNAROK_CLIENT_SERVER").ok().or_else(|| connection.map(|connection| connection.server()));
        (server.unwrap_or_else(|| DEFAULT_SERVER.to_owned()), version)
    }

    /// Takes a connection: its loading pictures, if it names any, and on to the login window.
    fn choose_connection(&mut self, picked: usize) {
        self.connection = picked;
        if let Some(pictures) = self.connections.get(picked).map(|connection| &connection.loading).filter(|pictures| !pictures.is_empty()) {
            self.loading_images = pictures.iter().map(|picture| format!("{UI}{picture}")).collect();
        }
        self.scene = Scene::Login;
    }

    fn enter_char_server(&mut self) {
        let Scene::ServerSelect { accepted, selected } = &self.scene else { return };
        let proxy = self.proxy.clone();
        self.account = Some(accepted.clone());
        self.char_server = *selected;
        match CharSession::connect(&accepted.char_servers[*selected], accepted, move |event| {
            let _ = proxy.send_event(NetEvent::Char(event));
        }) {
            Ok(session) => {
                self.char_session = Some(session);
                self.wait();
            }
            Err(_) => self.message("서버에 접속할 수 없습니다.", Scene::Login),
        }
    }

    fn start_game(&mut self) {
        let Scene::CharSelect(select) = &self.scene else { return };
        if !select.unlocked() {
            return;
        }
        let Some(character) = select.selected_character().cloned() else { return };
        match self.char_session.as_mut().map(|session| session.select(character.slot)) {
            Some(Ok(())) => {
                self.playing = Some(character);
                self.wait();
            }
            _ => self.message(self.messages.get(DISCONNECTED_MESSAGE).cloned().unwrap_or_default(), Scene::Login),
        }
    }

    fn back_to_login(&mut self) {
        self.char_session = None;
        self.entering = None;
        self.scene = Scene::Login;
    }

    fn message_text(&self, id: usize) -> String {
        self.messages.get(id).cloned().unwrap_or_default()
    }

    /// Does what a click or key on the character select asked for.
    fn select_action(&mut self, action: char_select::Action) {
        use char_select::Action;
        if !matches!(action, Action::Close) && matches!(&self.scene, Scene::CharSelect(select) if !select.unlocked()) {
            return;
        }
        let sent = match action {
            Action::Start => return self.start_game(),
            Action::Make(slot) => {
                let male = self.account.as_ref().is_none_or(|account| account.sex == 1);
                if let Scene::CharSelect(select) = std::mem::replace(&mut self.scene, Scene::Login) {
                    self.scene = Scene::CharMake(Box::new(char_make::Make::new(&self.grf, &self.look_tables, slot, male)), select);
                }
                return;
            }
            Action::Reserve(id) => self.char_session.as_mut().map(|session| session.reserve_delete(id)),
            Action::CancelReserve(id) => self.char_session.as_mut().map(|session| session.cancel_delete(id)),
            Action::Delete(id) => return self.ask(DELETE_QUESTION, Then::AskBirthdate(id)),
            Action::Close => return self.ask(QUIT_MESSAGE, Then::BackToLogin),
            Action::Move(from, to, moves) => return self.ask(MOVE_QUESTION, Then::Move(from, to, moves)),
            Action::Rename(id) => {
                let Scene::CharSelect(select) = &self.scene else { return };
                let renames = select.characters.iter().find(|character| character.id == id).map_or(0, |character| character.renames);
                // ponytail: the prompt's text is the rename count message; the real client's rename window may differ.
                let text = rustnarok_client::messages::fill(&self.message_text(RENAME_PROMPT), &[&renames]);
                let back = std::mem::replace(&mut self.scene, Scene::Login);
                self.scene = Scene::Prompt { text, input: String::new(), then: Then::ConfirmName(id), back: Box::new(back) };
                return;
            }
        };
        match sent {
            Some(Ok(())) => self.wait(),
            _ => self.message(self.message_text(DISCONNECTED_MESSAGE), Scene::Login),
        }
    }

    /// Runs autostart (`RUSTNAROK_CLIENT_AUTOSTART=<slot>`) and `RUSTNAROK_CLIENT_MAKE`/`DELETE` once PIN state 0 is in.
    fn finish_char_select_entry(&mut self) {
        let first = !std::mem::replace(&mut self.scripted, true);
        if !first {
            return;
        }
        if std::env::var("RUSTNAROK_CLIENT_AUTOSTART").ok().and_then(|slot| slot.parse::<usize>().ok()).is_some() {
            self.start_game();
        }
        self.run_select_switches();
    }

    fn apply_pin_action(&mut self, action: pin::PinAction) {
        let sent = match action {
            pin::PinAction::None => return,
            pin::PinAction::Check { seed, pin } => self.char_session.as_mut().map(|session| session.check_pin(seed, &pin)),
            pin::PinAction::Make { seed, pin } => self.char_session.as_mut().map(|session| session.make_pin(seed, &pin)),
            pin::PinAction::Change { seed, old_pin, new_pin } => self.char_session.as_mut().map(|session| session.change_pin(seed, &old_pin, &new_pin)),
            pin::PinAction::Message(id) => {
                let text = self.message_text(id);
                let back = std::mem::replace(&mut self.scene, Scene::Login);
                self.message(text, back);
                return;
            }
            pin::PinAction::Close => return self.back_to_login(),
        };
        if !matches!(sent, Some(Ok(()))) {
            self.message(self.message_text(DISCONNECTED_MESSAGE), Scene::Login);
        }
    }

    /// The syllable the input method is composing, shown where text is typed.
    fn compose(&mut self, text: String) {
        match &mut self.scene {
            Scene::Game(game) if game.dialog.is_none() && !game.storage.typing && !game.chat_mode.chat_line_open() => game.chat.preedit.clear(),
            Scene::Game(game) if game.dialog.is_none() && !game.storage.typing => game.chat.preedit = text,
            Scene::CharMake(make, _) => make.preedit = text,
            _ => {}
        }
    }

    /// Text the input method finished (Hangul): it goes where typing goes.
    fn commit(&mut self, text: &str) {
        let typed = text.chars().filter(|character| !character.is_control());
        match &mut self.scene {
            Scene::Game(game) => match &mut game.dialog {
                // NPC text input takes it; number input and plain dialogs don't.
                Some(dialog) => {
                    if let Some((true, value)) = &mut dialog.input {
                        value.extend(typed);
                    }
                }
                None if game.storage.typing => game.storage.type_text(text),
                None if game.navi_window.as_ref().is_some_and(|window| window.typing) => {
                    let window = game.navi_window.as_mut().unwrap();
                    window.text.push_str(text);
                    window.search(&self.navi_tables);
                }
                None if game.catalog.as_ref().is_some_and(|catalog| catalog.focus.is_some()) => game.catalog.as_mut().unwrap().type_text(text),
                None if game.room_make.is_some() => game.room_make.as_mut().unwrap().type_text(text),
                None if game.macro_window.as_ref().is_some_and(|window| window.editing.is_some()) => {
                    game.macro_window.as_ref().unwrap().type_text(&mut game.macro_lines, text)
                }
                None if game.vend_setup.as_ref().is_some_and(|setup| setup.typing) => game.vend_setup.as_mut().unwrap().type_text(text),
                None if game.buying_setup.as_ref().is_some_and(|setup| setup.focus.is_some()) => game.buying_setup.as_mut().unwrap().type_text(text),
                None if game.mail.typing() => game.mail.type_text(text),
                None if game.room.is_some() => game.room.as_mut().unwrap().input.push_str(text),
                None if !game.chat_mode.chat_line_open() => {}
                None => game.chat.type_text(&typed.collect::<String>()),
            },
            Scene::CharMake(make, _) => make.type_text(text),
            Scene::Prompt { input, then: Then::ConfirmName(_), .. } => {
                let room = NAME_MOST - input.chars().count().min(NAME_MOST);
                input.extend(typed.take(room));
            }
            _ => {}
        }
    }

    /// RUSTNAROK_CLIENT_MAKE and RUSTNAROK_CLIENT_DELETE, once the first character list is in.
    fn run_select_switches(&mut self) {
        let slot_and = |name: &str| {
            let value = std::env::var(name).ok()?;
            let (slot, rest) = value.split_once(':').unwrap_or((&value, ""));
            Some((slot.parse::<usize>().ok()?, rest.to_owned()))
        };
        if let Some((slot, name)) = slot_and("RUSTNAROK_CLIENT_MAKE") {
            if let Scene::CharSelect(select) = &mut self.scene {
                select.selected = slot;
            }
            self.select_action(char_select::Action::Make(slot as u8));
            if let Scene::CharMake(make, _) = &mut self.scene
                && !name.is_empty()
            {
                make.type_text(&name);
                self.make_action(char_make::Action::Make);
            }
        }
        if let Some((slot, step)) = slot_and("RUSTNAROK_CLIENT_DELETE") {
            let Scene::CharSelect(select) = &mut self.scene else { return };
            select.selected = slot;
            let Some(id) = select.selected_character().map(|character| character.id) else { return };
            match step.as_str() {
                "reserve" => self.select_action(char_select::Action::Reserve(id)),
                "cancel" => self.select_action(char_select::Action::CancelReserve(id)),
                // The question, and the birthdate prompt after it.
                "ask" => self.select_action(char_select::Action::Delete(id)),
                "prompt" => {
                    self.select_action(char_select::Action::Delete(id));
                    self.answer();
                }
                birthdate => {
                    let sent = self.char_session.as_mut().map(|session| session.delete(id, birthdate.trim_start_matches('-')));
                    if let Some(Ok(())) = sent {
                        self.wait();
                    }
                }
            }
        }
    }

    /// Does what a click or key on character creation asked for.
    fn make_action(&mut self, action: char_make::Action) {
        match action {
            char_make::Action::Make => {
                let Scene::CharMake(make, _) = &self.scene else { return };
                let (name, slot, color, hair, job, sex) = (make.name.trim().to_owned(), make.slot, make.color, make.hair, u32::from(make.job()), make.sex());
                match self.char_session.as_mut().map(|session| session.make(&name, slot, color, hair, job, sex)) {
                    Some(Ok(())) => self.wait(),
                    _ => self.message(self.message_text(DISCONNECTED_MESSAGE), Scene::Login),
                }
            }
            char_make::Action::ShortName => {
                let text = self.message_text(char_make::SHORT_NAME_MESSAGE);
                let back = std::mem::replace(&mut self.scene, Scene::Login);
                self.message(text, back);
            }
            char_make::Action::Back => {
                if let Scene::CharMake(_, select) = std::mem::replace(&mut self.scene, Scene::Login) {
                    self.scene = Scene::CharSelect(select);
                }
            }
        }
    }

    /// Puts a question (message `id`) with OK and Cancel over the current scene.
    fn ask(&mut self, id: usize, then: Then) {
        let text = self.message_text(id);
        let back = std::mem::replace(&mut self.scene, Scene::Login);
        self.scene = Scene::Confirm(text, then, Box::new(back));
    }

    /// OK (or Enter) on a question or a prompt.
    fn answer(&mut self) {
        let (then, input, back) = match std::mem::replace(&mut self.scene, Scene::Login) {
            Scene::Confirm(_, then, back) => (then, String::new(), back),
            Scene::Prompt { input, then, back, .. } => (then, input, back),
            scene => return self.scene = scene,
        };
        self.scene = *back;
        match then {
            Then::BackToLogin => self.back_to_login(),
            Then::AskBirthdate(id) => {
                let text = self.message_text(BIRTHDATE_MESSAGE);
                let back = std::mem::replace(&mut self.scene, Scene::Login);
                self.scene = Scene::Prompt { text, input: String::new(), then: Then::Delete(id), back: Box::new(back) };
            }
            Then::Delete(id) => match self.char_session.as_mut().map(|session| session.delete(id, &input)) {
                Some(Ok(())) => self.wait(),
                _ => self.message(self.message_text(DISCONNECTED_MESSAGE), Scene::Login),
            },
            Then::Move(from, to, moves) => {
                if let Scene::CharSelect(select) = &mut self.scene {
                    select.moved_to = Some(usize::from(to));
                }
                match self.char_session.as_mut().map(|session| session.move_slot(u16::from(from), u16::from(to), moves)) {
                    Some(Ok(())) => self.wait(),
                    _ => self.message(self.message_text(DISCONNECTED_MESSAGE), Scene::Login),
                }
            }
            Then::ConfirmName(id) => {
                let name = input.trim().to_owned();
                if name.is_empty() {
                    return;
                }
                let text = rustnarok_client::messages::fill(&self.message_text(RENAME_QUESTION), &[&name]);
                let back = std::mem::replace(&mut self.scene, Scene::Login);
                self.scene = Scene::Confirm(text, Then::Rename(id, name), Box::new(back));
            }
            Then::Rename(id, name) => match self.char_session.as_mut().map(|session| session.rename(id, &name)) {
                Some(Ok(())) => self.wait(),
                _ => self.message(self.message_text(DISCONNECTED_MESSAGE), Scene::Login),
            },
        }
    }

    /// Cancel (or Escape) on a question or a prompt.
    fn dismiss(&mut self) {
        if let Scene::Confirm(_, _, back) | Scene::Prompt { back, .. } = std::mem::replace(&mut self.scene, Scene::Login) {
            self.scene = *back;
        }
    }

    /// Back to the scene under a finished request.
    fn settle(&mut self) {
        self.scene = match std::mem::replace(&mut self.scene, Scene::Login) {
            Scene::Waiting(inner) => *inner,
            scene => scene,
        };
    }

    fn click(&mut self, event_loop: &ActiveEventLoop) {
        let Some(assets) = &self.assets else { return };
        let cursor = self.cursor;
        let shift = self.modifiers.shift_key();
        let hit = |position: [f32; 2], images: &ButtonImages| contains(position, images[0].size, cursor);
        // A click anywhere but the storage's search field takes the caret from it.
        if let Scene::Game(game) = &mut self.scene {
            game.storage.typing = false;
            if let Some(trade) = &mut game.trade {
                trade.typing = false;
            }
            if let Some(bank) = &mut game.bank {
                bank.typing = false;
            }
        }
        match &mut self.scene {
            Scene::Login => {
                let login = login_position(&assets.login);
                if hit(offset(login, LOGIN_START), &assets.connect) {
                    self.submit_login();
                } else if contains(offset(login, LOGIN_EXIT.0), LOGIN_EXIT.1, cursor) {
                    event_loop.exit();
                } else if contains(offset(login, LOGIN_SAVE.0), LOGIN_SAVE.1, cursor) {
                    self.save_id = !self.save_id;
                } else if let Some(field) = LOGIN_FIELDS.iter().position(|&field| contains(offset(login, field), LOGIN_FIELD_SIZE, cursor)) {
                    self.focus = field;
                    self.blink_start = Instant::now();
                }
            }
            // The server list: pick one, OK goes on to the login window, Cancel leaves the game.
            Scene::Connections { selected } => {
                let service = dialog_position(&assets.service);
                let list = offset(service, SERVER_LIST);
                if let Some(row) = (0..self.connections.len()).find(|&row| contains(offset(list, [0.0, row as f32 * SERVER_ROW[1]]), SERVER_ROW, cursor)) {
                    *selected = row;
                } else if hit(offset(service, LEFT_BUTTON), &assets.ok) {
                    let picked = *selected;
                    self.choose_connection(picked);
                } else if hit(offset(service, RIGHT_BUTTON), &assets.cancel) {
                    self.quitting = true;
                }
            }
            Scene::ServerSelect { accepted, selected } => {
                let service = dialog_position(&assets.service);
                let list = offset(service, SERVER_LIST);
                if let Some(row) = (0..accepted.char_servers.len()).find(|&row| contains(offset(list, [0.0, row as f32 * SERVER_ROW[1]]), SERVER_ROW, cursor)) {
                    *selected = row;
                } else if hit(offset(service, LEFT_BUTTON), &assets.ok) {
                    self.enter_char_server();
                } else if hit(offset(service, RIGHT_BUTTON), &assets.cancel) {
                    self.scene = Scene::Login;
                }
            }
            Scene::CharSelect(select) => {
                if let Some(pad) = &mut select.pin_pad {
                    if let Some(button) = pad.hit(cursor) {
                        let action = pad.press(button);
                        self.apply_pin_action(action);
                    }
                } else if select.unlocked() {
                    let action = if select.menu.is_some() { select.click_menu(cursor) } else { select.hit(cursor).and_then(|hit| select.click(hit)) };
                    if let Some(action) = action {
                        self.select_action(action);
                    }
                } else if select.hit(cursor) == Some(char_select::Hit::Close) {
                    self.select_action(char_select::Action::Close);
                }
            }
            Scene::CharMake(make, _) => {
                if let Some(action) = make.hit(cursor).and_then(|hit| make.click(hit, &self.grf, &self.look_tables)) {
                    self.make_action(action);
                }
            }
            // A link in the chat log or the NPC window: an item's information, or a navigation to the place.
            Scene::Game(game) if game.link_at(cursor).is_some() => match game.link_at(cursor).cloned().unwrap() {
                links::Target::Item(item) => {
                    let shown = iteminfo::Shown {
                        item: item.id,
                        identified: true,
                        refine: item.refine,
                        cards: item.cards,
                        damaged: None,
                        expires: 0,
                        card: self.card_pictures.name(item.id).is_some(),
                    };
                    let position = game.item_window.as_ref().map(|window| window.position);
                    let mut window = iteminfo::ItemWindow::new(shown);
                    if let Some(position) = position {
                        window.position = position;
                    }
                    game.item_window = Some(window);
                }
                links::Target::Navi { map, cell } => {
                    let map = navigation::map_key(&map);
                    let name = self.navi_tables.maps.get(&map).cloned().unwrap_or_else(|| map.clone());
                    let target = navigation::Target { cell: (cell != [0, 0]).then_some(cell), monster: None, name, map };
                    game.navigation = Some(navigation::Active::new(target, NAVI_SERVICES));
                }
            },
            Scene::Game(game) if game.deal_choice.is_some() => {
                let npc = game.deal_choice.unwrap();
                let dialog = dialog_position(&assets.message);
                let pressed = DEAL_BUTTONS.iter().position(|&place| hit(offset(dialog, place), assets.shop_buttons()[0]));
                match pressed {
                    Some(choice @ 0..=1) => {
                        game.deal_choice = None;
                        let _ = game.session.choose_deal(npc, choice == 1);
                    }
                    Some(_) => game.deal_choice = None,
                    None => {}
                }
            }
            // The world map covers the screen; its close button closes it.
            Scene::Game(game) if game.world_map.is_some() => {
                if game.world_map.as_ref().unwrap().closes(cursor) {
                    game.world_map = None;
                }
            }
            // The skill window's Apply asks first: OK sends the pending levels; Cancel keeps them pending.
            Scene::Game(game) if game.apply_asked && contains(dialog_position(&assets.message), assets.message.size, cursor) => {
                let dialog = dialog_position(&assets.message);
                for (place, yes) in [(LEFT_BUTTON, true), (RIGHT_BUTTON, false)] {
                    if hit(offset(dialog, place), if yes { &assets.ok } else { &assets.cancel }) {
                        game.answer_apply(yes);
                    }
                }
            }
            // Save point asks first. OK goes; Cancel stays.
            Scene::Game(game) if game.save_asked && contains(dialog_position(&assets.message), assets.message.size, cursor) => {
                let dialog = dialog_position(&assets.message);
                for (place, yes) in [(LEFT_BUTTON, true), (RIGHT_BUTTON, false)] {
                    if hit(offset(dialog, place), if yes { &assets.ok } else { &assets.cancel }) {
                        game.save_asked = false;
                        if yes {
                            game.escape = None;
                            let _ = game.session.restart(false);
                        }
                    }
                }
            }
            // The option menu keeps the pointer until it's closed.
            Scene::Game(game) if game.escape.is_some() => match game.escape.as_ref().unwrap().hit(cursor, game.can_resurrect()).flatten() {
                Some(escape::Choice::SavePoint) => game.save_asked = true,
                Some(escape::Choice::CharSelect) => {
                    game.escape = None;
                    let _ = game.session.restart(true);
                }
                Some(escape::Choice::Exit) => {
                    game.escape = None;
                    let _ = game.session.quit();
                }
                Some(escape::Choice::Return) => game.escape = None,
                Some(escape::Choice::Resurrect) => {
                    game.escape = None;
                    let _ = game.session.standing_resurrection();
                }
                // Game settings and shortcut settings are the windows from #68.
                Some(escape::Choice::GameSettings) => {
                    game.escape = None;
                    game.graphics_window = Some(options::GraphicsWindow::new());
                    escape::touch(&mut game.esc_order, escape::WindowId::GraphicsSettings);
                }
                Some(escape::Choice::Shortcuts) => {
                    game.escape = None;
                    game.shortcuts_window = Some(options::ShortcutsWindow::new());
                    escape::touch(&mut game.esc_order, escape::WindowId::ShortcutSettings);
                }
                None => {}
            },
            Scene::Game(game) if game.whisper_at(cursor) => {
                game.whisper_click(cursor);
            }
            // The audio settings: checkboxes and arrows (the sliders go on press).
            Scene::Game(game) if game.audio_window.as_ref().is_some_and(|window| window.hit(cursor).is_some()) => {
                let window = game.audio_window.as_mut().unwrap();
                let hit = window.hit(cursor).unwrap();
                if window.click(hit, &mut self.sound) {
                    if let Some(audio) = &mut self.audio {
                        audio.set(&self.sound);
                    }
                    self.sound.save();
                }
                if !window.frame.open {
                    game.audio_window = None;
                }
            }
            Scene::Game(game) if game.graphics_window.as_ref().is_some_and(|window| window.hit(cursor).is_some()) => {
                let window = game.graphics_window.as_mut().unwrap();
                let hit = window.hit(cursor).unwrap();
                if window.click(hit, &mut self.graphics) {
                    self.graphics.save();
                }
                if !window.frame.open {
                    game.graphics_window = None;
                }
            }
            Scene::Game(game) if game.shortcuts_window.as_ref().is_some_and(|window| window.hit(cursor).is_some()) => {
                let window = game.shortcuts_window.as_mut().unwrap();
                let hit = window.hit(cursor).unwrap();
                window.click(hit);
                if !window.frame.open {
                    game.shortcuts_window = None;
                }
            }
            // The quest window: tabs, quests, their locks, and the picked one's details.
            Scene::Game(game) if game.achievement_window.covers(cursor) => {
                let hit = game.achievement_window.hit(cursor, &self.achievement_info, &game.achievements).unwrap();
                if let Some(id) = game.achievement_window.click(hit) {
                    let _ = game.session.claim_achievement(id);
                }
            }
            Scene::Game(game) if game.quest_window.hit(cursor, &game.quests).is_some() => {
                let hit = game.quest_window.hit(cursor, &game.quests).unwrap();
                let tracker = game.quest_window.tracker;
                if let Some((id, active)) = game.quest_window.click(hit, &game.quests) {
                    let _ = game.session.set_quest_active(id, active);
                }
                // The tracker's checkbox is kept with the settings.
                if game.quest_window.tracker != tracker
                    && let Some(path) = settings_path("quest_tracker")
                {
                    let _ = path
                        .parent()
                        .map_or(Ok(()), std::fs::create_dir_all)
                        .and_then(|()| std::fs::write(path, if game.quest_window.tracker { "1" } else { "0" }));
                }
            }
            // The mail windows: the mailbox, a mail read, one written, and the new-mail icon.
            Scene::Game(game) if game.mail.hit(cursor).is_some() => {
                let hit = game.mail.hit(cursor).unwrap();
                if let Some(action) = game.mail.click(hit)
                    && let Some(id) = game.mail_action(action)
                {
                    game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ERROR);
                }
            }
            // Setting up a shop: its name, its goods and prices, OK or cancel.
            Scene::Game(game) if game.vend_setup.as_ref().is_some_and(|setup| setup.hit(cursor, &game.cart.items).is_some()) => {
                let setup = game.vend_setup.as_mut().unwrap();
                let hit = setup.hit(cursor, &game.cart.items).unwrap();
                match setup.click(hit, &game.cart.items) {
                    Some(vend::Action::Price(index)) => game.amount = Some(window::Amount::new(Asked::VendPrice(index), None)),
                    Some(vend::Action::Open) => {
                        let goods = setup.goods(&game.cart.items);
                        let _ = game.session.open_shop(&setup.name.clone(), true, &goods);
                    }
                    Some(vend::Action::Cancel) => {
                        let _ = game.session.open_shop("", false, &[]);
                        game.vend_setup = None;
                    }
                    None => {}
                }
            }
            // Our open shop: closing it closes the shop.
            Scene::Game(game) if game.my_shop.as_ref().is_some_and(|shop| shop.hit(cursor).is_some()) => {
                if game.my_shop.as_ref().unwrap().hit(cursor) == Some(true) {
                    let _ = game.session.close_shop();
                    game.my_shop = None;
                    game.signs.remove(&game.account_id);
                }
            }
            // Setting up a buying store: its name, zeny limit, wanted items, OK or cancel.
            Scene::Game(game)
                if game.amount.is_none() && game.buying_setup.as_ref().is_some_and(|setup| setup.hit(cursor, &game.inventory.items).is_some()) =>
            {
                let setup = game.buying_setup.as_mut().unwrap();
                let hit = setup.hit(cursor, &game.inventory.items).unwrap();
                match setup.click(hit, &game.inventory.items) {
                    Some(buying::Action::Price(index, item_id)) => {
                        game.amount = Some(window::Amount::price(Asked::BuyingPrice(index, item_id)));
                    }
                    Some(buying::Action::Open) => {
                        let zeny = game.status.get(&net::STATUS_ZENY).copied().unwrap_or(0).max(0) as u32;
                        match setup.validate(&game.inventory.items, zeny) {
                            Ok((limit, items)) => {
                                let name = setup.name.clone();
                                let _ = game.session.open_buying_store(&name, limit, true, &items);
                            }
                            Err(Some((msg_id, Some(item_id)))) => {
                                let item_name = inventory::item_name(&self.items, item_id, true, 0);
                                let line = rustnarok_client::messages::fill(self.messages.get(msg_id).map_or("", String::as_str), &[&item_name]);
                                game.chat_line(line, CHAT_ERROR);
                            }
                            Err(Some((msg_id, None))) => {
                                game.chat_line(self.messages.get(msg_id).cloned().unwrap_or_default(), CHAT_ERROR);
                            }
                            Err(None) => {}
                        }
                    }
                    Some(buying::Action::Cancel) => {
                        let _ = game.session.open_buying_store("", 0, false, &[]);
                        game.buying_setup = None;
                    }
                    None => {}
                }
            }
            // Our open buying store: closing it closes the store and drops the sign.
            Scene::Game(game) if game.amount.is_none() && game.my_buying_store.as_ref().is_some_and(|store| store.hit(cursor).is_some()) => {
                if game.my_buying_store.as_ref().unwrap().hit(cursor) == Some(true) {
                    let _ = game.session.close_buying_store();
                    game.my_buying_store = None;
                    game.signs.remove(&game.account_id);
                }
            }
            // A card's picture: its close button. A click anywhere on it stays there.
            Scene::Game(game) if game.card_view.as_ref().is_some_and(|view| view.hit(cursor).is_some()) => {
                if game.card_view.as_ref().unwrap().hit(cursor) == Some(iteminfo::ViewHit::Close) {
                    game.card_view = None;
                }
            }
            // The item description window: its close button, a card's picture, or a card in a slot.
            Scene::Game(game) if game.item_window.as_ref().is_some_and(|window| window.hit(cursor, &self.items).is_some()) => {
                match game.item_window.as_ref().unwrap().hit(cursor, &self.items) {
                    Some(iteminfo::Hit::Close) => {
                        game.item_window = None;
                        game.card_view = None;
                    }
                    Some(iteminfo::Hit::View) => {
                        let item = game.item_window.as_ref().unwrap().shown.item;
                        if let Some(name) = self.card_pictures.name(item) {
                            game.card_view = Some(iteminfo::CardView::new(iteminfo::cardbmp_path(name)));
                            escape::touch(&mut game.esc_order, escape::WindowId::CardPicture);
                        }
                    }
                    Some(iteminfo::Hit::Card(id)) => {
                        let position = game.item_window.as_ref().map(|window| offset(window.position, [20.0, 20.0]));
                        let mut window = iteminfo::ItemWindow::new(iteminfo::Shown {
                            item: id,
                            identified: true,
                            refine: 0,
                            cards: [0; 4],
                            damaged: None,
                            expires: 0,
                            card: true,
                        });
                        if let Some(position) = position {
                            window.position = position;
                        }
                        game.item_window = Some(window);
                        game.card_view = None;
                    }
                    _ => {}
                }
            }
            // The right-click menu takes the next click, wherever it lands.
            Scene::Game(game) if game.menu.is_some() => match game.menu.take().unwrap().hit(cursor).flatten() {
                Some(Choice::Trade(id)) => {
                    game.trade_with = game.units.get(&id).map(|actor| actor.name.clone()).unwrap_or_default();
                    let _ = game.session.request_trade(id);
                }
                Some(Choice::ViewEquipment(id)) => {
                    let _ = game.session.view_equipment(id);
                }
                Some(Choice::Invite(id)) => {
                    if let Some(actor) = game.units.get(&id) {
                        let _ = game.session.invite_to_party(&actor.name.clone());
                    }
                }
                Some(Choice::Adopt(id)) => {
                    let _ = game.session.ask_adoption(id);
                }
                Some(Choice::GuildInvite(id)) => {
                    if let Some(actor) = game.units.get(&id) {
                        let _ = game.session.invite_to_guild(&actor.name.clone());
                    }
                }
                Some(Choice::Ally(id)) => {
                    let _ = game.session.ask_alliance(id, game.account_id, game.me.id);
                }
                Some(Choice::Antagonist(id)) => {
                    let _ = game.session.set_antagonist(id);
                }
                // Feeding asks first; the status opens the pet window.
                Some(Choice::Pet(net::PET_FEED)) => game.pet_asked = Some(PetAsked::Feed),
                Some(Choice::Pet(net::PET_INFO)) => game.open_pet_window(),
                Some(Choice::Pet(command)) => game.pet_command(command),
                Some(Choice::Leader(account)) => game.leader_asked = Some(account),
                Some(Choice::RoomOwner(row)) => {
                    if let Some(name) = game.room.as_ref().and_then(|room| room.member_choices(row, &game.me.name)) {
                        let _ = game.session.hand_over_chat_room(&name);
                    }
                }
                Some(Choice::RoomKick(row)) => {
                    if let Some(name) = game.room.as_ref().and_then(|room| room.member_choices(row, &game.me.name)) {
                        let _ = game.session.kick_from_chat_room(&name);
                    }
                }
                Some(Choice::QuestInfo(id)) => {
                    game.quest_window.frame.open = true;
                    game.quest_window.tab = 0;
                    game.quest_window.picked = Some(id);
                    game.sync_esc();
                }
                Some(Choice::QuestAside(id)) => {
                    let _ = game.session.set_quest_active(id, false);
                }
                Some(Choice::GuildMove(row, position)) => {
                    if let Some(member) = game.guild.as_ref().and_then(|guild| guild.members.get(row)) {
                        let _ = game.session.move_member(member.account, member.character, position);
                    }
                }
                Some(Choice::GuildLeave) => game.guild_asked = Some(GuildAsked::Leave),
                Some(Choice::Befriend(id)) => {
                    if let Some(actor) = game.units.get(&id) {
                        let _ = game.session.add_friend(&actor.name.clone());
                    }
                }
                None => {}
            },
            // The room maker's fields and buttons.
            // The emotion list: a click shows that emotion.
            Scene::Game(game) if game.emotion_list.as_ref().is_some_and(|list| list.hit(cursor).is_some()) => {
                match game.emotion_list.as_ref().unwrap().hit(cursor) {
                    Some(emotion_list::Hit::Title(TitleHit::Close)) => game.emotion_list = None,
                    Some(emotion_list::Hit::Emotion(kind)) => {
                        let _ = game.session.emote(kind);
                    }
                    _ => {}
                }
            }
            // The macro window: a line's field takes the typing.
            Scene::Game(game) if game.macro_window.as_ref().is_some_and(|window| window.hit(cursor).is_some()) => {
                match game.macro_window.as_ref().unwrap().hit(cursor) {
                    Some(macros::Hit::Title(TitleHit::Close)) => game.toggle_macro_window(),
                    Some(macros::Hit::Line(line)) => game.macro_window.as_mut().unwrap().editing = Some(line),
                    _ => game.macro_window.as_mut().unwrap().editing = None,
                }
            }
            Scene::Game(game) if game.room_make.as_ref().is_some_and(|make| make.hit(cursor).is_some()) => {
                let make = game.room_make.as_mut().unwrap();
                let hit = make.hit(cursor).unwrap();
                if make.click(hit) {
                    game.make_room();
                } else if hit == chatroom::MakeHit::Cancel {
                    game.room_make = None;
                }
            }
            // The room we're in: closing it leaves.
            Scene::Game(game) if game.room.as_ref().is_some_and(|room| room.hit(cursor, &game.me.name).is_some()) => {
                let room = game.room.as_mut().unwrap();
                match room.hit(cursor, &game.me.name) {
                    Some(chatroom::RoomHit::Title(TitleHit::Close)) => {
                        let _ = game.session.leave_chat_room();
                    }
                    Some(chatroom::RoomHit::Title(title)) => room.frame.click_title(title),
                    // The owner's Room Setup: the make window, filled from the room.
                    Some(chatroom::RoomHit::Setup) => game.room_make = Some(chatroom::Make::from_sign(&room.sign)),
                    _ => {}
                }
            }
            // A chat room's sign: open rooms let us in, locked ones ask for the password.
            Scene::Game(game)
                if game.room.is_none()
                    && game.sign_rects.iter().any(|(rect, ..)| contains([rect[0], rect[1]], [rect[2] - rect[0], rect[3] - rect[1]], cursor)) =>
            {
                let &(_, id, kind) =
                    game.sign_rects.iter().rev().find(|(rect, ..)| contains([rect[0], rect[1]], [rect[2] - rect[0], rect[3] - rect[1]], cursor)).unwrap();
                let _ = match kind {
                    // A vendor's sign opens their shop.
                    chatroom::SHOP if id != game.account_id => game.session.browse_vendor(id),
                    chatroom::SHOP => Ok(()),
                    chatroom::BUYING_STORE if id != game.account_id => game.session.click_buying_store(id),
                    chatroom::BUYING_STORE => Ok(()),
                    0 => {
                        game.amount = Some(window::Amount::name(Asked::RoomPassword(id), ROOM_PASSWORD));
                        Ok(())
                    }
                    _ => game.session.enter_chat_room(id, ""),
                };
            }
            // A guild invites us: OK or cancel.
            Scene::Game(game) if game.guild_invite.is_some() && contains(dialog_position(&assets.message), assets.message.size, cursor) => {
                let dialog = dialog_position(&assets.message);
                for (place, accept) in [(LEFT_BUTTON, true), (RIGHT_BUTTON, false)] {
                    if hit(offset(dialog, place), if accept { &assets.ok } else { &assets.cancel }) {
                        let (guild, _) = game.guild_invite.take().unwrap();
                        let _ = game.session.answer_guild(guild, accept);
                    }
                }
            }
            // A guild question: OK or cancel. Whoever is asked for an alliance answers either way.
            Scene::Game(game) if game.guild_asked.is_some() && contains(dialog_position(&assets.message), assets.message.size, cursor) => {
                let dialog = dialog_position(&assets.message);
                for (place, yes) in [(LEFT_BUTTON, true), (RIGHT_BUTTON, false)] {
                    if hit(offset(dialog, place), if yes { &assets.ok } else { &assets.cancel }) {
                        let _ = match game.guild_asked.take().unwrap() {
                            GuildAsked::Ally { id, .. } => game.session.answer_alliance(id, yes),
                            GuildAsked::End { id, antagonist } if yes => game.session.end_relation(id, antagonist),
                            GuildAsked::End { .. } => Ok(()),
                            GuildAsked::Leave if yes => match &game.guild {
                                Some(guild) => game.session.leave_guild(guild.id, game.account_id, game.me.id, ""),
                                None => Ok(()),
                            },
                            GuildAsked::Leave => Ok(()),
                        };
                    }
                }
            }
            // Wearing an item that binds: OK wears it.
            Scene::Game(game) if game.bind_asked.is_some() && contains(dialog_position(&assets.message), assets.message.size, cursor) => {
                let dialog = dialog_position(&assets.message);
                for (place, yes) in [(LEFT_BUTTON, true), (RIGHT_BUTTON, false)] {
                    if hit(offset(dialog, place), if yes { &assets.ok } else { &assets.cancel }) {
                        game.answer_bind(yes);
                    }
                }
            }
            // Growing the inventory: OK or cancel.
            Scene::Game(game) if game.expansion_asked.is_some() && contains(dialog_position(&assets.message), assets.message.size, cursor) => {
                let dialog = dialog_position(&assets.message);
                for (place, yes) in [(LEFT_BUTTON, true), (RIGHT_BUTTON, false)] {
                    if hit(offset(dialog, place), if yes { &assets.ok } else { &assets.cancel }) {
                        game.expansion_asked = None;
                        let _ = game.session.answer_expansion(yes);
                    }
                }
            }
            // A queue question: OK applies, leaves the queue or enters the battle; Cancel doesn't (or declines entering).
            Scene::Game(game) if game.queue_asked.is_some() && contains(dialog_position(&assets.message), assets.message.size, cursor) => {
                let dialog = dialog_position(&assets.message);
                for (place, yes) in [(LEFT_BUTTON, true), (RIGHT_BUTTON, false)] {
                    if hit(offset(dialog, place), if yes { &assets.ok } else { &assets.cancel }) {
                        let _ = match game.queue_asked.take().unwrap() {
                            QueueAsked::Apply(kind, name) if yes => game.session.apply_to_queue(kind, &name),
                            QueueAsked::Cancel(name) if yes => game.session.leave_queue(&name),
                            QueueAsked::Lobby(name, _) => game.session.answer_lobby(yes, &name),
                            _ => Ok(()),
                        };
                    }
                }
            }
            // The queue window: a battleground picked, applying, cancelling.
            Scene::Game(game) if game.queue_window.frame.open && game.queue_window.hit(cursor, &self.battlegrounds, game.queued.is_some()).is_some() => {
                match game.queue_window.hit(cursor, &self.battlegrounds, game.queued.is_some()) {
                    Some(battle_queue::Hit::Title(title)) => game.queue_window.frame.click_title(title),
                    Some(battle_queue::Hit::Row(row)) => game.queue_window.picked = Some(row),
                    Some(battle_queue::Hit::Apply(kind)) => {
                        if let Some(entry) = game.queue_window.picked.and_then(|row| self.battlegrounds.get(row)) {
                            game.queue_asked = Some(QueueAsked::Apply(kind, entry.name.clone()));
                        }
                    }
                    Some(battle_queue::Hit::Cancel) => {
                        if let Some((name, _)) = &game.queued {
                            game.queue_asked = Some(QueueAsked::Cancel(name.clone()));
                        }
                    }
                    _ => {}
                }
            }
            // The big minimap: a click on it, before the small one's buttons (it can cover them).
            Scene::Game(game) if game.minimap_window.hit(cursor, [game.world.gat.width as u32, game.world.gat.height as u32]).is_some() => {
                let size = [game.world.gat.width as u32, game.world.gat.height as u32];
                if let Some(hit) = game.minimap_window.hit(cursor, size) {
                    game.minimap_window.click(hit);
                    game.sync_esc();
                }
            }
            // The minimap's bottom row: zoom (kept with the settings), the big map, the route flag.
            Scene::Game(game) if game.window_at(cursor).is_none() && minimap::button_at(cursor).is_some() => match minimap::button_at(cursor) {
                Some(button @ ("plus" | "minus")) => {
                    game.minimap_zoom = if button == "plus" { minimap::zoom_in(game.minimap_zoom) } else { minimap::zoom_out(game.minimap_zoom) };
                    if let Some(path) = settings_path("minimap_zoom") {
                        let _ = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(path, game.minimap_zoom.to_string()));
                    }
                }
                Some("mini") => {
                    if let Some(gpu) = &self.gpu {
                        let no_picture = self.messages.get(minimap_window::NO_PICTURE).cloned().unwrap_or_default();
                        toggle_big_minimap(game, gpu, no_picture);
                    }
                }
                // ponytail: #276 draws the navigation route when this is on. The route itself is not drawn here.
                Some("object") => game.navigation_route = !game.navigation_route,
                // ponytail: viewon shows the camera's view wedge on the minimap. That wedge isn't drawn.
                Some("viewon") => {}
                _ => {}
            },
            // The new-quest icon: the quest window.
            Scene::Game(game) if game.quest_icon && quest_notice::on_icon(cursor) && game.window_at(cursor).is_none() => {
                game.quest_window.frame.open = true;
                game.quest_window.frame.minimized = false;
                game.quest_window.tab = 0;
                game.quest_icon = false;
                game.sync_esc();
            }
            // A level up button: the status window (base level) or the skill window (job level), restored if minimised.
            Scene::Game(game) if game.level_ups.hit(cursor).is_some() && game.window_at(cursor).is_none() => {
                match game.level_ups.hit(cursor).unwrap() {
                    level_up::Button::Base => {
                        game.status_window.frame.open = true;
                        game.status_window.frame.minimized = false;
                        game.raise(Panel::Status);
                    }
                    level_up::Button::Job => {
                        game.skill_window.frame.open = true;
                        game.skill_window.frame.minimized = false;
                        game.raise(Panel::Skills);
                    }
                }
                game.sync_esc();
            }
            // A skill's description: its ESC button closes it; a click elsewhere on it does nothing.
            Scene::Game(game) if game.skill_description.as_ref().is_some_and(|window| window.contains(cursor)) => {
                if game.skill_description.as_ref().is_some_and(|window| window.on_close(cursor)) {
                    game.skill_description = None;
                }
            }
            // The store search: the kind, the fields, searching, paging, and a row twice to find its store.
            // A store it opened stands over it.
            Scene::Game(game)
                if game.catalog.as_ref().and_then(|catalog| catalog.hit(cursor)).is_some() && !game.shop.as_ref().is_some_and(|shop| shop.covers(cursor)) =>
            {
                let hit = game.catalog.as_ref().and_then(|catalog| catalog.hit(cursor));
                game.click_catalog(hit.unwrap(), &self.items, &self.messages);
            }
            // The ranking board: another ranking, or closing it.
            Scene::Game(game) if game.ranking.as_ref().and_then(|board| board.hit(cursor)).is_some() => {
                match game.ranking.as_ref().and_then(|board| board.hit(cursor)) {
                    Some(ranking::Hit::Kind(kind)) => {
                        let _ = game.session.ask_ranking(kind);
                    }
                    Some(ranking::Hit::Title(TitleHit::Close)) => game.ranking = None,
                    Some(ranking::Hit::Title(title)) => game.ranking.as_mut().unwrap().frame.click_title(title),
                    _ => {}
                }
            }
            // The navigation window: its field, a result to go to, stopping.
            Scene::Game(game) if game.navi_window.as_ref().and_then(|window| window.hit(cursor)).is_some() => {
                let window = game.navi_window.as_mut().unwrap();
                match window.hit(cursor).unwrap() {
                    navigation::Hit::Title(TitleHit::Close) => game.navi_window = None,
                    navigation::Hit::Title(title) => window.frame.click_title(title),
                    navigation::Hit::Field => window.typing = true,
                    navigation::Hit::Result(index) => {
                        let target = window.results[index].clone();
                        window.typing = false;
                        game.navigation = Some(navigation::Active::new(target, NAVI_SERVICES));
                    }
                    navigation::Hit::Stop => {
                        window.typing = false;
                        game.navigation = None;
                    }
                    navigation::Hit::Window => window.typing = false,
                }
            }
            // The reputation window: its title bar.
            Scene::Game(game) if game.reputation_window.as_ref().and_then(|window| window.hit(cursor)).is_some() => {
                match game.reputation_window.as_ref().and_then(|window| window.hit(cursor)) {
                    Some(reputation::Hit::Title(TitleHit::Close)) => game.reputation_window = None,
                    Some(reputation::Hit::Title(title)) => game.reputation_window.as_mut().unwrap().frame.click_title(title),
                    _ => {}
                }
            }
            // The dress room: pickers, turning, closing.
            Scene::Game(game) if game.dressroom.as_ref().and_then(|room| room.hit(cursor)).is_some() => {
                let room = game.dressroom.as_mut().unwrap();
                if room.hit(cursor).is_some_and(|hit| room.click(hit, &game.dress_lists)) {
                    game.dressroom = None;
                    game.dress_figure = None;
                }
            }
            // Handing the party over: OK asks the server.
            Scene::Game(game) if game.leader_asked.is_some() && contains(dialog_position(&assets.message), assets.message.size, cursor) => {
                let dialog = dialog_position(&assets.message);
                for (place, yes) in [(LEFT_BUTTON, true), (RIGHT_BUTTON, false)] {
                    if hit(offset(dialog, place), if yes { &assets.ok } else { &assets.cancel }) {
                        let account = game.leader_asked.take().unwrap();
                        if yes {
                            let _ = game.session.change_party_leader(account);
                        }
                    }
                }
            }
            // The party setup window: choices, OK and Cancel.
            Scene::Game(game) if game.party_setup.as_ref().and_then(|setup| setup.hit(cursor)).is_some() => {
                let setup = game.party_setup.as_mut().unwrap();
                match setup.hit(cursor).and_then(|hit| setup.click(hit)) {
                    Some(party::SetupAction::Send(settings)) => {
                        game.party_setup = None;
                        let _ = game.session.party_options(settings.exp_even, settings.pickup, settings.share);
                    }
                    Some(party::SetupAction::Close) => game.party_setup = None,
                    None => {}
                }
            }
            // Dismissing the mercenary: OK lets it go.
            Scene::Game(game) if game.dismiss_asked && contains(dialog_position(&assets.message), assets.message.size, cursor) => {
                let dialog = dialog_position(&assets.message);
                for (place, yes) in [(LEFT_BUTTON, true), (RIGHT_BUTTON, false)] {
                    if hit(offset(dialog, place), if yes { &assets.ok } else { &assets.cancel }) {
                        game.answer_dismiss(yes, &self.messages);
                    }
                }
            }
            // Naming the homunculus: OK sends the name.
            Scene::Game(game) if game.homunculus_rename.is_some() && contains(dialog_position(&assets.message), assets.message.size, cursor) => {
                let dialog = dialog_position(&assets.message);
                for (place, yes) in [(LEFT_BUTTON, true), (RIGHT_BUTTON, false)] {
                    if hit(offset(dialog, place), if yes { &assets.ok } else { &assets.cancel }) {
                        game.answer_homunculus_name(yes);
                    }
                }
            }
            // A pet question: OK feeds it or renames it.
            Scene::Game(game) if game.pet_asked.is_some() && contains(dialog_position(&assets.message), assets.message.size, cursor) => {
                let dialog = dialog_position(&assets.message);
                for (place, yes) in [(LEFT_BUTTON, true), (RIGHT_BUTTON, false)] {
                    if hit(offset(dialog, place), if yes { &assets.ok } else { &assets.cancel }) {
                        game.answer_pet(yes);
                    }
                }
            }
            // A cash shop purchase: OK sends the cart.
            Scene::Game(game) if game.cashshop_buy_asked && contains(dialog_position(&assets.message), assets.message.size, cursor) => {
                let dialog = dialog_position(&assets.message);
                for (place, accept) in [(LEFT_BUTTON, true), (RIGHT_BUTTON, false)] {
                    if hit(offset(dialog, place), if accept { &assets.ok } else { &assets.cancel }) {
                        game.cashshop_buy_asked = false;
                        if let Some((free, items)) = game.cashshop.as_ref().and_then(|shop| shop.buy_request(accept)) {
                            let _ = game.session.buy_cashshop(free, &items);
                        }
                    }
                }
            }
            // Someone wants to be friends: OK or cancel.
            Scene::Game(game) if game.friend_asked.is_some() && contains(dialog_position(&assets.message), assets.message.size, cursor) => {
                let dialog = dialog_position(&assets.message);
                for (place, accept) in [(LEFT_BUTTON, true), (RIGHT_BUTTON, false)] {
                    if hit(offset(dialog, place), if accept { &assets.ok } else { &assets.cancel }) {
                        let (account, character, _) = game.friend_asked.take().unwrap();
                        let _ = game.session.answer_friend(account, character, accept);
                    }
                }
            }
            // Asked to be adopted: OK asks again, then accepts; Cancel at either step refuses.
            Scene::Game(game) if game.adopt_asked.is_some() && contains(dialog_position(&assets.message), assets.message.size, cursor) => {
                let dialog = dialog_position(&assets.message);
                for (place, yes) in [(LEFT_BUTTON, true), (RIGHT_BUTTON, false)] {
                    if hit(offset(dialog, place), if yes { &assets.ok } else { &assets.cancel }) {
                        let (parent, partner, name, again) = game.adopt_asked.take().unwrap();
                        if yes && !again {
                            // What a baby class gives up doesn't fit the box: the whole of it in the chat, the box asks.
                            game.chat_line(self.messages.get(ADOPT_ASKS_AGAIN).cloned().unwrap_or_default(), CHAT_ERROR);
                            game.adopt_asked = Some((parent, partner, name, true));
                        } else {
                            let _ = game.session.answer_adoption(parent, partner, yes);
                        }
                    }
                }
            }
            // A party invites us: OK or cancel.
            Scene::Game(game) if game.party_invite.is_some() && contains(dialog_position(&assets.message), assets.message.size, cursor) => {
                let dialog = dialog_position(&assets.message);
                for (place, accept) in [(LEFT_BUTTON, true), (RIGHT_BUTTON, false)] {
                    if hit(offset(dialog, place), if accept { &assets.ok } else { &assets.cancel }) {
                        let (party, _) = game.party_invite.take().unwrap();
                        let _ = game.session.answer_party(party, accept);
                    }
                }
            }
            // Someone asks for a deal: OK or cancel.
            Scene::Game(game) if game.trade_asked.is_some() && contains(dialog_position(&assets.message), assets.message.size, cursor) => {
                let dialog = dialog_position(&assets.message);
                for (place, accept) in [(LEFT_BUTTON, true), (RIGHT_BUTTON, false)] {
                    if hit(offset(dialog, place), if accept { &assets.ok } else { &assets.cancel }) {
                        game.trade_asked = None;
                        let _ = game.session.answer_trade(accept);
                    }
                }
            }
            Scene::Game(game) if game.bank.as_ref().is_some_and(|bank| bank.hit(cursor).is_some()) => {
                let carried = game.status.get(&net::STATUS_ZENY).copied().unwrap_or(0).max(0) as u32;
                let bank = game.bank.as_mut().unwrap();
                let hit = bank.hit(cursor).unwrap();
                let account = game.account_id;
                match bank.click(hit, carried) {
                    Some(bank::Action::Deposit(zeny)) => {
                        let _ = game.session.move_zeny(true, account, zeny);
                    }
                    Some(bank::Action::Withdraw(zeny)) => {
                        let _ = game.session.move_zeny(false, account, zeny);
                    }
                    Some(bank::Action::Refuse(id)) => game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ERROR),
                    Some(bank::Action::Close) => {
                        let _ = game.session.close_bank(account);
                        game.bank = None;
                    }
                    None => {}
                }
            }
            Scene::Game(game) if game.cashshop.as_ref().is_some_and(|shop| shop.hit(cursor).is_some()) => {
                let shop = game.cashshop.as_mut().unwrap();
                let hit = shop.hit(cursor).unwrap();
                match shop.click(hit) {
                    Some(cashshop::Action::Buy) => game.cashshop_buy_asked = true,
                    Some(cashshop::Action::Close) => game.close_cashshop(),
                    None => {}
                }
            }
            Scene::Game(game) if game.viewing.as_ref().is_some_and(|viewed| viewed.window.hit(cursor, &viewed.items).is_some()) => {
                let viewed = game.viewing.as_mut().unwrap();
                match viewed.window.hit(cursor, &viewed.items).unwrap() {
                    equipment::Hit::Title(TitleHit::Close) => game.viewing = None,
                    hit => {
                        viewed.window.click(hit);
                    }
                }
            }
            // Sense's window: its close button.
            Scene::Game(game) if game.monster_window.as_ref().is_some_and(|window| window.hit(cursor).is_some()) => {
                if game.monster_window.as_ref().unwrap().hit(cursor) == Some(true) {
                    game.monster_window = None;
                }
            }
            // The Lucky Roulette: START spins, Get Prize claims (neither while the wheel spins), close lets it go.
            Scene::Game(game) if game.roulette.as_ref().is_some_and(|window| window.hit(cursor).is_some()) => {
                let window = game.roulette.as_mut().unwrap();
                let hit = window.hit(cursor).unwrap();
                match window.click(hit, control::now()) {
                    Some(roulette::Action::Spin) => _ = game.session.roulette(net::RouletteRequest::Spin),
                    Some(roulette::Action::Claim) => _ = game.session.roulette(net::RouletteRequest::Claim),
                    Some(roulette::Action::Busy) => game.chat_line(self.messages.get(roulette::SPINNING).cloned().unwrap_or_default(), CHAT_ERROR),
                    Some(roulette::Action::Close) => game.close_roulette(),
                    None => {}
                }
            }
            // Its icon asks for it.
            Scene::Game(game) if roulette::icon_hit(cursor) => {
                if game.roulette.is_none() {
                    let _ = game.session.roulette(net::RouletteRequest::Open);
                }
            }
            // The material window: OK sends what's in; Cancel and the close button send nothing in (rathena forgets the
            // window either way).
            Scene::Game(game) if game.materials.as_ref().is_some_and(|window| window.hit(cursor).is_some()) => {
                let window = game.materials.as_mut().unwrap();
                let hit = window.hit(cursor).unwrap();
                if let Some(ok) = window.click(hit) {
                    let entries = if ok { window.entries.clone() } else { Vec::new() };
                    let _ = game.session.send_materials(window.kind, ok, &entries);
                    game.materials = None;
                }
            }
            Scene::Game(game) if game.synthesis.as_ref().is_some_and(|window| window.hit(cursor).is_some()) => {
                let hit = game.synthesis.as_ref().unwrap().hit(cursor).unwrap();
                match game.synthesis.as_mut().unwrap().click(hit) {
                    Some(laphine::Action::Make) => {
                        let (box_id, entries) = {
                            let window = game.synthesis.as_ref().unwrap();
                            (window.box_id, window.entries())
                        };
                        let _ = game.session.synthesize(box_id, &entries);
                    }
                    Some(laphine::Action::Close) => {
                        let _ = game.session.close_synthesis();
                        game.synthesis = None;
                    }
                    None => {}
                }
            }
            Scene::Game(game) if game.upgrade.as_ref().is_some_and(|window| window.hit(cursor).is_some()) => {
                let hit = game.upgrade.as_ref().unwrap().hit(cursor).unwrap();
                match game.upgrade.as_mut().unwrap().click(hit) {
                    Some(laphine::Action::Make) => {
                        if let Some(index) = game.upgrade.as_ref().unwrap().index() {
                            let box_id = game.upgrade.as_ref().unwrap().box_id;
                            let _ = game.session.upgrade_item(box_id, index);
                        }
                    }
                    Some(laphine::Action::Close) => {
                        let _ = game.session.close_upgrade();
                        game.upgrade = None;
                    }
                    None => {}
                }
            }
            Scene::Game(game) if game.reform.as_ref().is_some_and(|window| window.hit(cursor, &game.inventory.items).is_some()) => {
                let hit = game.reform.as_ref().unwrap().hit(cursor, &game.inventory.items).unwrap();
                match game.reform.as_mut().unwrap().click(hit, &game.inventory.items) {
                    Some(reform::Action::Reform { item, index }) => {
                        let _ = game.session.reform(item, index);
                    }
                    // Until the server hears it, it holds the player and refuses NPC clicks.
                    Some(reform::Action::Close) => {
                        let _ = game.session.close_reform();
                        game.reform = None;
                    }
                    None => {}
                }
            }
            Scene::Game(game) if game.enchant.as_ref().is_some_and(|window| window.hit(cursor).is_some()) => {
                let zeny = game.status.get(&net::STATUS_ZENY).copied().unwrap_or(0).max(0) as u32;
                let action = {
                    let carried = &game.inventory.items;
                    let count = |id: u32| carried.iter().filter(|item| item.id == id).map(|item| u32::from(item.amount)).sum::<u32>();
                    let index = game.enchant.as_ref().and_then(|window| window.item);
                    let item = index.and_then(|index| carried.iter().find(|item| item.index == index)).cloned();
                    let window = game.enchant.as_mut().unwrap();
                    let hit = window.hit(cursor).unwrap();
                    window.click(hit, item.as_ref(), count, zeny)
                };
                match action {
                    Some(enchant::Action::Send(request)) => {
                        let result = match request {
                            enchant::Request::Random { table, index } => game.session.enchant_random(table, index),
                            enchant::Request::Perfect { table, index, item } => game.session.enchant_perfect(table, index, item),
                            enchant::Request::Upgrade { table, index, slot } => game.session.enchant_upgrade(table, index, slot),
                            enchant::Request::Reset { table, index } => game.session.enchant_reset(table, index),
                        };
                        // Nothing went out, so the window must not sit waiting for an answer that never comes.
                        if result.is_err()
                            && let Some(window) = &mut game.enchant
                        {
                            window.forget_request();
                        }
                    }
                    Some(enchant::Action::Refuse(id)) => game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ERROR),
                    // Until the server hears the close, it holds the player and ignores the next request.
                    Some(enchant::Action::Close) => {
                        let _ = game.session.close_enchant();
                        game.enchant = None;
                    }
                    None => {}
                }
            }
            Scene::Game(game)
                if game.stylist.as_ref().is_some_and(|window| {
                    let job = game.player.look.as_ref().map(|look| look.job).unwrap_or(game.me.job);
                    window.hit(cursor, &self.styling_shop, job).is_some()
                }) =>
            {
                let job = game.player.look.as_ref().map(|look| look.job).unwrap_or(game.me.job);
                let hair = game.player.look.as_ref().map(|look| look.hair).unwrap_or(0);
                let zeny = game.status.get(&net::STATUS_ZENY).copied().unwrap_or(0);
                let carried = &game.inventory.items;
                let has = |id: u32| carried.iter().filter(|item| item.id == id).map(|item| u32::from(item.amount)).sum::<u32>();
                let window = game.stylist.as_mut().unwrap();
                let hit = window.hit(cursor, &self.styling_shop, job).unwrap();
                match window.click(hit, &self.styling_shop, job, hair, zeny, has) {
                    Some(stylist::Action::Buy(picks)) => {
                        let _ = game.session.buy_style(picks);
                    }
                    Some(stylist::Action::Refuse(id)) => game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ERROR),
                    Some(stylist::Action::Close) => {
                        let _ = game.session.close_stylist();
                        game.stylist = None;
                    }
                    None => {}
                }
                let look = game.player.look.clone().unwrap_or_default();
                if let Some(window) = &mut game.stylist {
                    dress_stylist(window, &self.grf, &self.look_tables, &look, &self.items, &self.styling_shop);
                }
            }
            Scene::Game(game) if game.refine.as_ref().is_some_and(|window| window.hit(cursor).is_some()) => {
                let zeny = game.status.get(&net::STATUS_ZENY).copied().unwrap_or(0).max(0) as u32;
                let carried = &game.inventory.items;
                let count = |id: u32| carried.iter().filter(|item| item.id == id).map(|item| u32::from(item.amount)).sum::<u32>();
                let window = game.refine.as_mut().unwrap();
                let hit = window.hit(cursor).unwrap();
                match window.click(hit, count, zeny) {
                    Some(refine::Action::Refine { index, ore, blessings }) => {
                        let _ = game.session.refine(index, ore, blessings);
                    }
                    Some(refine::Action::Refuse(id)) => game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ERROR),
                    // Until the server hears it, it won't open the window again and holds the player.
                    Some(refine::Action::Close) => {
                        let _ = game.session.close_refine();
                        game.refine = None;
                    }
                    None => {}
                }
            }
            // Grade enchanting: Start sends the try, a refusal is the client's own line, close tells the server.
            Scene::Game(game) if game.grade.as_ref().is_some_and(|window| window.hit(cursor).is_some()) => {
                let zeny = game.status.get(&net::STATUS_ZENY).copied().unwrap_or(0).max(0) as u32;
                let carried = &game.inventory.items;
                let count = |id: u32| carried.iter().filter(|item| item.id == id).map(|item| u32::from(item.amount)).sum::<u32>();
                let window = game.grade.as_mut().unwrap();
                let hit = window.hit(cursor).unwrap();
                match window.click(hit, count, zeny) {
                    Some(grade::Action::Enchant { index, material, steps }) => {
                        let _ = game.session.grade_enchant(index, material, steps);
                    }
                    Some(grade::Action::Refuse(id)) => game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ERROR),
                    Some(grade::Action::Close) => {
                        let _ = game.session.close_grade();
                        game.grade = None;
                    }
                    None => {}
                }
            }
            Scene::Game(game) if game.trade.as_ref().is_some_and(|trade| trade.hit(cursor).is_some()) => {
                let trade = game.trade.as_mut().unwrap();
                let hit = trade.hit(cursor).unwrap();
                match trade.click(hit) {
                    Some(trade::Action::Lock(zeny)) => game.lock_trade(zeny),
                    Some(trade::Action::Exchange) => {
                        let _ = game.session.finish_trade();
                    }
                    Some(trade::Action::Cancel) => {
                        let _ = game.session.cancel_trade();
                        game.trade = None;
                    }
                    None => {}
                }
            }
            // The box asking how many keeps the pointer until it's answered.
            Scene::Game(game) if game.amount.is_some() => {
                if let Some(Some(ok)) = window::Amount::<Asked>::hit(cursor, assets.message.size, assets.ok[0].size) {
                    game.answer_amount(ok, &self.messages);
                }
            }
            Scene::Game(game) if game.pick.as_ref().is_some_and(|(list, _)| list.covers(cursor)) => {
                let (list, picking) = game.pick.as_mut().unwrap();
                let hit = list.hit(cursor).unwrap();
                // Item merge: rows pick and unpick, OK sends the picked stacks and waits for the answer.
                let refused = match (*picking, hit) {
                    (Picking::Merge, pick_list::Hit::Row(row)) => list.toggle(row).err(),
                    (Picking::Merge, pick_list::Hit::Ok) => match list.merge_request() {
                        Ok(indexes) => {
                            let _ = game.session.merge_items(&indexes);
                            None
                        }
                        Err(line) => Some(line),
                    },
                    _ => {
                        if let Some(answer) = list.click(hit) {
                            game.answer_pick(answer);
                        }
                        None
                    }
                };
                if let Some(line) = refused {
                    game.chat_line(self.messages.get(line).cloned().unwrap_or_default(), CHAT_ERROR);
                }
            }
            Scene::Game(game) if game.shop.as_ref().is_some_and(|shop| shop.covers(cursor)) => {
                let shop = game.shop.as_mut().unwrap();
                let hit = shop.hit(cursor).unwrap();
                match shop.click(hit) {
                    Some(shop::Action::Deal) => {
                        let _ = shop.send(&mut game.session);
                        // An NPC shop closes as the deal goes.
                        if shop.plain() {
                            game.shop = None;
                        }
                    }
                    Some(shop::Action::Close) => game.close_shop(),
                    Some(shop::Action::Ask(row, _)) => game.amount = Some(window::Amount::new(Asked::Shop(row), None)),
                    Some(shop::Action::AskOut(..) | shop::Action::Say(..)) | None => {}
                }
            }
            // A small party window: its close button removes it. A click on the body was a drag (handled on press).
            Scene::Game(game)
                if game.member_windows.hit(cursor).is_some() || game.party_window.compact && party::compact_hit(cursor, game.party.as_ref()).is_some() =>
            {
                if let Some((account, party::MemberHit::Close)) = game.member_windows.hit(cursor) {
                    game.member_windows.close(account);
                }
            }
            Scene::Game(game) if game.window_at(cursor).is_some() => {
                let id = game.window_at(cursor).unwrap();
                game.raise(id);
                match id {
                    Panel::Hotkeys => {
                        let hit = game.hotkeys.hit(cursor).unwrap();
                        game.hotkeys.click(hit);
                        if let hotkeys::Hit::Slot(slot) = hit
                            && game.hotkeys.clicks.double(slot as u16)
                        {
                            game.press_hotkey(slot);
                        }
                    }
                    Panel::BasicInfo => {
                        let hit = game.basic_info.hit(cursor).unwrap();
                        match game.basic_info.click(hit) {
                            Some("equip") => {
                                game.equipment.frame.toggle();
                                game.raise(Panel::Equipment);
                            }
                            Some("item") => {
                                game.inventory.frame.toggle();
                                game.raise(Panel::Inventory);
                            }
                            Some("skill") => {
                                game.skill_window.frame.toggle();
                                game.raise(Panel::Skills);
                            }
                            Some("party") => {
                                game.party_window.frame.toggle();
                                game.raise(Panel::Party);
                            }
                            Some("status") => {
                                game.status_window.frame.toggle();
                                game.raise(Panel::Status);
                            }
                            Some("guild") => {
                                game.guild_window.frame.toggle();
                                game.raise(Panel::Guild);
                                if game.guild_window.frame.open {
                                    game.ask_guild_pages();
                                }
                            }
                            // The bank opens when the server says so: it answers in its own words where banking is off.
                            Some("bank") => {
                                let _ = game.session.open_bank(game.account_id);
                            }
                            Some("achievement") => game.achievement_window.frame.toggle(),
                            Some("mail") => {
                                let action = game.mail.toggle();
                                game.mail_action(action);
                            }
                            Some("quest") => {
                                game.quest_window.frame.toggle();
                                game.sync_esc();
                            }
                            // The game settings and the shortcut settings, as the game menu opens them (#68).
                            Some("option") => {
                                game.graphics_window = if game.graphics_window.is_some() { None } else { Some(options::GraphicsWindow::new()) };
                                game.sync_esc();
                            }
                            Some("keyboard") => {
                                game.shortcuts_window = if game.shortcuts_window.is_some() { None } else { Some(options::ShortcutsWindow::new()) };
                                game.sync_esc();
                            }
                            Some("repute") => {
                                game.toggle_reputation(&self.reputation_tables);
                                game.sync_esc();
                            }
                            Some("navigation") => {
                                game.toggle_navigation();
                                game.sync_esc();
                            }
                            Some("map") => {
                                game.world_map = match (game.world_map.take(), &self.gpu) {
                                    (None, Some(gpu)) => worldmap::WorldMap::open(gpu, &self.grf, &self.worlds.0, self.worlds.1, &map_key(&game.map)),
                                    _ => None,
                                };
                            }
                            _ => {}
                        }
                    }
                    Panel::Inventory => {
                        let hit = game.inventory.hit(cursor).unwrap();
                        // Shift + click puts the item's link on the chat line.
                        if shift
                            && let inventory::Hit::Item(index) = hit
                            && let Some(item) = game.inventory.get(index)
                        {
                            let name = inventory::item_name(&self.items, item.id, item.identified, item.refine);
                            game.chat.type_whole(&item_link(item.id, &name));
                        } else if let Some(index) = game.inventory.click(hit) {
                            game.use_item(index);
                        }
                    }
                    Panel::Equipment => {
                        let hit = game.equipment.hit(cursor, &game.inventory.items).unwrap();
                        // The box changes when the server answers.
                        if hit == equipment::Hit::ShowBox {
                            let _ = game.session.show_equipment(!game.equipment.shown);
                        }
                        // The title changes when the server answers.
                        if let equipment::Hit::WearTitle(title) = hit {
                            let _ = game.session.change_title(title);
                        }
                        if hit == equipment::Hit::TakeOffAll {
                            let _ = game.session.take_off_all();
                        }
                        if hit == equipment::Hit::SwitchAll {
                            match game.equipment.switch_all(&game.inventory.items, control::now()) {
                                Ok(true) => drop(game.session.switch_all()),
                                Ok(false) => {}
                                Err(id) => game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ERROR),
                            }
                        }
                        // In the swap view, a double click takes the item out of the set.
                        match game.equipment.click(hit) {
                            Some(index) if game.equipment.swapping => {
                                game.switch_asked = Some(index);
                                let _ = game.session.switch_remove(index);
                            }
                            Some(index) if !game.store_open() => drop(game.session.take_off(index)),
                            _ => {}
                        }
                    }
                    Panel::Cart => {
                        let hit = game.cart.hit(cursor).unwrap();
                        game.cart.click(hit);
                    }
                    Panel::Guild => match game.guild.as_ref().and_then(|guild| game.guild_window.hit(cursor, guild)) {
                        Some(guild::Hit::Title(title)) => game.guild_window.frame.click_title(title),
                        Some(guild::Hit::Tab(tab)) => {
                            game.guild_window.show_tab(tab);
                            // Each page but the notice is asked for when shown.
                            if tab <= 4 {
                                let _ = game.session.guild_page(tab as u32);
                            }
                        }
                        Some(guild::Hit::LevelUp(skill)) => {
                            let _ = game.session.skill_up(skill);
                        }
                        Some(guild::Hit::Relation(id, antagonist)) => game.guild_asked = Some(GuildAsked::End { id, antagonist }),
                        // The master's positions page: a box switches its permission; the title and tax are asked for.
                        Some(guild::Hit::Permission(id, permission)) => {
                            if let Some(guild) = &game.guild {
                                let _ = game.session.change_position(&guild.switched(id, permission), &guild.title(id));
                            }
                        }
                        Some(guild::Hit::PositionTitle(id)) => game.amount = Some(window::Amount::name(Asked::PositionTitle(id), POSITION_TITLE)),
                        Some(guild::Hit::Tax(id)) => game.amount = Some(window::Amount::new(Asked::PositionTax(id), None)),
                        _ => {}
                    },
                    Panel::Clan => {
                        if let Some(clan::Hit::Title(title)) = game.clan.as_ref().and_then(|_| game.clan_window.hit(cursor)) {
                            game.clan_window.frame.click_title(title);
                        }
                    }
                    Panel::Mercenary => match game.mercenary_window.hit(cursor) {
                        Some(mercenary::Hit::Title(title)) => game.mercenary_window.frame.click_title(title),
                        Some(mercenary::Hit::Skills) => {
                            game.mercenary_skill_list.frame.toggle();
                            game.raise(Panel::MercenarySkills);
                        }
                        Some(mercenary::Hit::Dismiss) => game.dismiss_asked = true,
                        _ => {}
                    },
                    Panel::MercenarySkills => match game.mercenary_skill_list.hit(cursor, &game.mercenary_skills) {
                        Some(mercenary::SkillHit::Title(title)) => game.mercenary_skill_list.frame.click_title(title),
                        Some(mercenary::SkillHit::Skill(id)) => game.use_mercenary_skill(id, u16::MAX),
                        _ => {}
                    },
                    Panel::Homunculus => match game.homunculus.as_ref().and_then(|homunculus| game.homunculus_window.hit(cursor, homunculus)) {
                        Some(homunculus::Hit::Title(title)) => game.homunculus_window.frame.click_title(title),
                        Some(homunculus::Hit::Feed) => {
                            let _ = game.session.homunculus_command(net::HOMUNCULUS_FEED);
                        }
                        Some(homunculus::Hit::Skills) => {
                            game.homunculus_skill_list.frame.toggle();
                            game.raise(Panel::HomunculusSkills);
                        }
                        Some(homunculus::Hit::Rename) => game.amount = Some(window::Amount::name(Asked::HomunculusName, homunculus::RENAME_PROMPT)),
                        _ => {}
                    },
                    Panel::HomunculusSkills => {
                        let points = game.homunculus.as_ref().map_or(0, |homunculus| homunculus.skill_points);
                        match game.homunculus_skill_list.hit(cursor, &game.homunculus_skills, points) {
                            Some(homunculus::SkillHit::Title(title)) => game.homunculus_skill_list.frame.click_title(title),
                            Some(homunculus::SkillHit::Skill(id)) => game.use_skill(id, u16::MAX),
                            Some(homunculus::SkillHit::Raise(id)) => {
                                let _ = game.session.skill_up(id);
                            }
                            _ => {}
                        }
                    }
                    Panel::Pet => match game.pet.as_ref().and_then(|pet| game.pet_window.hit(cursor, pet)) {
                        Some(pet::Hit::Title(title)) => game.pet_window.frame.click_title(title),
                        Some(pet::Hit::Rename) => game.amount = Some(window::Amount::name(Asked::PetName, PET_NAME_RULE)),
                        Some(pet::Hit::Feed) => game.pet_asked = Some(PetAsked::Feed),
                        _ => {}
                    },
                    Panel::Status => {
                        let hit = game.status_window.hit(cursor, &assets.status, &game.status);
                        if let Some(stat) = hit.and_then(|hit| game.status_window.click(hit)) {
                            // A trait (4th jobs) has its own request.
                            if (status::TRAIT_POW..status::TRAIT_POW + 6).contains(&stat) {
                                let _ = game.session.raise_trait(stat);
                            } else {
                                let _ = game.session.raise_stat(stat);
                            }
                        }
                    }
                    Panel::Party => match game.party_window.hit(cursor, game.party.as_ref(), &game.friends, game.account_id) {
                        Some(party::Hit::Title(title)) => game.party_window.frame.click_title(title),
                        Some(party::Hit::Tab(friends)) => {
                            game.party_window.friends = friends;
                            game.party_window.picked = None;
                        }
                        Some(party::Hit::Friend(row)) => game.party_window.picked = Some(row),
                        // A friend picked: whisper them (the name goes into the chat's name box), or drop them.
                        Some(party::Hit::Button(party::Button::Whisper)) => {
                            if let Some(friend) = game.party_window.picked.and_then(|row| game.friends.get(row)) {
                                game.chat.whisper = friend.name.clone();
                                game.chat.naming = false;
                            }
                        }
                        Some(party::Hit::Button(party::Button::Delete)) => {
                            if let Some(friend) = game.party_window.picked.and_then(|row| game.friends.get(row)) {
                                let _ = game.session.remove_friend(friend.account, friend.character);
                            }
                        }
                        Some(party::Hit::Button(party::Button::Create)) => game.amount = Some(window::Amount::name(Asked::PartyName, PARTY_CREATE)),
                        Some(party::Hit::Button(party::Button::Invite)) => game.amount = Some(window::Amount::name(Asked::Invite, PARTY_INVITE)),
                        Some(party::Hit::Button(party::Button::Leave)) => {
                            let _ = game.session.leave_party();
                        }
                        Some(party::Hit::Button(party::Button::Setup)) => {
                            if let Some(party) = &game.party {
                                let leader = party.leader() == Some(game.account_id);
                                game.party_setup = Some(party::Setup::new(party, leader, game.party_window.frame.position));
                            }
                        }
                        Some(party::Hit::Button(party::Button::FriendSetup)) => game.open_friend_setup(),
                        Some(party::Hit::LastRow) => game.party_window.compact = !game.party_window.compact,
                        _ => {}
                    },
                    Panel::Storage => {
                        let hit = game.storage.hit(cursor, &assets.storage, &self.items);
                        if hit.is_some_and(|hit| game.storage.click(hit)) {
                            let _ = game.session.close_storage();
                        }
                    }
                    Panel::Skills => {
                        let jobs = skills::tabs(&self.skill_trees, game.me.job, &game.skills);
                        let points = game.skill_points();
                        let skill_info = game.skill_info.clone();
                        let sheet = skills::Sheet { trees: &self.skill_trees, jobs: &jobs, skills: &game.skills.clone(), info: &skill_info, points };
                        let hit = game.skill_window.hit(cursor, &sheet);
                        match hit.and_then(|hit| game.skill_window.click(hit)) {
                            Some(skills::Action::Use(id)) => {
                                let learned = game.skills.iter().find(|skill| skill.id == id).map_or(0, |skill| skill.level);
                                let level = if learned > 0 { game.skill_window.use_level(id, learned) } else { 0 };
                                if level > 0 {
                                    game.use_skill(id, level);
                                }
                            }
                            // A click (let go, not dragged off): a level goes in pending, with what it needs first; a
                            // prerequisite the points can't reach is named in red.
                            Some(skills::Action::LevelUp(id)) => {
                                if let Err(short) = game.skill_window.raise(&sheet, id) {
                                    let name = skill_info.get(&short).map_or_else(|| short.to_string(), |info| info.name.clone());
                                    let line = rustnarok_client::messages::fill(self.messages.get(skills::CANNOT_LEARN).map_or("", String::as_str), &[&name]);
                                    game.chat_line(line, CHAT_ERROR);
                                }
                            }
                            Some(skills::Action::Apply) => game.apply_asked = true,
                            None => {}
                        }
                    }
                }
            }
            // The chat tab settings: a kind of line on or off for the tab.
            Scene::Game(game) if game.chat.settings.as_ref().and_then(|settings| settings.hit(cursor)).is_some() => {
                match game.chat.settings.as_ref().and_then(|settings| settings.hit(cursor)).unwrap() {
                    chat::SettingsHit::Title(TitleHit::Close) => game.chat.settings = None,
                    chat::SettingsHit::Title(title) => game.chat.settings.as_mut().unwrap().frame.click_title(title),
                    chat::SettingsHit::Kind(kind) => game.chat.toggle_kind(kind),
                    chat::SettingsHit::Window => {}
                }
            }
            Scene::Game(game) if game.chat.on_list(cursor) || game.chat.blocks(cursor) => {
                game.chat.click(cursor);
                // ponytail: a click on the chat bar opens the battle-mode line; Enter is the key the spec names.
                game.chat_mode.open_line();
            }
            Scene::Game(game) if game.dialog.is_some() => {
                // NPC windows take the click; the map doesn't react while talking.
                let dialog = game.dialog.as_mut().unwrap();
                if dialog.button != DialogButton::None && hit(offset(npc_box_position(), DIALOG_BUTTON), &assets.next) {
                    game.press_dialog_button();
                } else if let Some((items, selected)) = &mut dialog.menu {
                    let menu = menu_position();
                    let first = menu_first_row(*selected, items.len());
                    let list = offset(menu, [MENU_CONTENT[0] as f32, MENU_CONTENT[1] as f32]);
                    let row = (0..MENU_ROWS.min(items.len())).find(|&row| contains(offset(list, [0.0, row as f32 * MENU_ROW[1]]), MENU_ROW, cursor));
                    if let Some(row) = row {
                        // Clicking the highlighted item again picks it.
                        if *selected == first + row {
                            game.choose(false);
                        } else {
                            *selected = first + row;
                        }
                    } else if hit(offset(menu, MENU_OK), &assets.ok) {
                        game.choose(false);
                    } else if hit(offset(menu, MENU_CANCEL), &assets.cancel) {
                        game.choose(true);
                    }
                }
            }
            // Taming: a monster under the pointer is the one to try (rathena checks the distance); anything else keeps
            // aiming.
            Scene::Game(game) if game.capturing => {
                if let Some(Owner::Unit(id)) = hovered_unit(&game.hit_boxes, cursor, false)
                    && game.units.get(&id).is_some_and(|actor| actor.kind == MONSTER)
                {
                    game.capturing = false;
                    let _ = game.session.try_capture(id);
                }
            }
            Scene::Game(game) => {
                if game.store_open() {
                    return;
                }
                // Whatever is under the cursor (see `picking::pick`): talk to NPCs, attack monsters, pick items up. The
                // ground walked when the button went down (`press`).
                match game.world_click(cursor, shift) {
                    // Started on the press.
                    WorldClick::Attack(_) => {}
                    WorldClick::Swing(id) => drop(game.session.act(id, net::ACT_ATTACK)),
                    // The rest went with the press.
                    WorldClick::Talk(_) | WorldClick::PickUp(_) | WorldClick::Warp(_) | WorldClick::Ground => {}
                }
            }
            Scene::Message(..) => {
                if hit(offset(dialog_position(&assets.message), RIGHT_BUTTON), &assets.ok) {
                    self.close_message();
                }
            }
            Scene::Confirm(..) | Scene::Prompt { .. } => {
                let dialog = dialog_position(&assets.message);
                if hit(offset(dialog, LEFT_BUTTON), &assets.ok) {
                    self.answer();
                } else if hit(offset(dialog, RIGHT_BUTTON), &assets.cancel) {
                    self.dismiss();
                }
            }
            Scene::Waiting(_) => {}
        }
    }

    /// The pointer moved: windows and items being dragged follow it.
    fn pointer_moved(&mut self) {
        if let Scene::Game(game) = &mut self.scene {
            let cursor = self.cursor;
            if let Some((_, from, moved)) = &mut game.press_mark {
                *moved |= moved_off(*from, cursor);
            }
            if game.right_press.is_some_and(|(from, _)| moved_off(from, cursor)) {
                game.right_moved = true;
            }
            let (inventory_height, equipment_height, info_height) = (game.inventory.height(), game.equipment.height(), game.basic_info.height());
            game.basic_info.frame.drag_to(self.cursor, info_height);
            game.chat.drag_to(self.cursor);
            let hotkeys_height = game.hotkeys.height();
            game.hotkeys.frame.drag_to(self.cursor, hotkeys_height);
            game.inventory.frame.drag_to(self.cursor, inventory_height);
            game.equipment.frame.drag_to(self.cursor, equipment_height);
            let skills_height = game.skill_window.height();
            game.skill_window.frame.drag_to(self.cursor, skills_height);
            let storage_height = game.storage.height();
            game.storage.frame.drag_to(self.cursor, storage_height);
            if let Some(trade) = &mut game.trade {
                let height = trade.height();
                trade.frame.drag_to(self.cursor, height);
            }
            if let Some(shop) = &mut game.shop {
                shop.drag_to(self.cursor);
            }
            let party_height = game.party_window.height(game.party.as_ref(), &game.friends);
            game.party_window.frame.drag_to(self.cursor, party_height);
            game.member_windows.drag_to(self.cursor);
            let status_height = game.status_height;
            game.status_window.frame.drag_to(self.cursor, status_height);
            let guild_height = game.guild_window.height();
            game.guild_window.frame.drag_to(self.cursor, guild_height);
            let clan_height = game.clan_window.height();
            game.clan_window.frame.drag_to(self.cursor, clan_height);
            let cart_height = game.cart.height();
            game.cart.frame.drag_to(self.cursor, cart_height);
            game.pet_window.frame.drag_to(self.cursor, pet::PetWindow::height());
            game.mercenary_window.frame.drag_to(self.cursor, mercenary::Window::height());
            let skills_height = mercenary::SkillList::height(game.mercenary_skills.len());
            game.mercenary_skill_list.frame.drag_to(self.cursor, skills_height);
            game.homunculus_window.frame.drag_to(self.cursor, homunculus::Window::height());
            let homunculus_skills_height = homunculus::SkillList::height(game.homunculus_skills.len());
            game.homunculus_skill_list.frame.drag_to(self.cursor, homunculus_skills_height);
            game.talks.drag_to(self.cursor);
            if let Some(setup) = &mut game.friend_setup {
                setup.frame.drag_to(self.cursor, whisper::FriendSetup::height());
            }
            if let Some(list) = &mut game.block_list {
                let height = list.height();
                list.frame.drag_to(self.cursor, height);
            }
            if let Some(view) = &mut game.card_view {
                view.drag_to(self.cursor);
            }
            if let Some(window) = &mut game.item_window {
                window.drag_to(self.cursor);
            }
            if let Some(room) = &mut game.room {
                let height = room.height();
                room.frame.drag_to(self.cursor, height);
            }
            game.mail.drag_to(self.cursor);
            game.quest_window.frame.drag_to(self.cursor, 466.0);
            game.minimap_window.frame.drag_to(self.cursor, minimap_window::HEIGHT);
            game.achievement_window.frame.drag_to(self.cursor, achievement::HEIGHT);
            if let Some(window) = &mut game.roulette {
                window.frame.drag_to(self.cursor, roulette::HEIGHT);
            }
            if let Some(window) = &mut game.grade {
                window.frame.drag_to(self.cursor, grade::HEIGHT);
            }
            if let Some(window) = &mut game.synthesis {
                window.frame.drag_to(self.cursor, laphine::Synthesis::height());
            }
            if let Some(window) = &mut game.upgrade {
                window.frame.drag_to(self.cursor, laphine::Upgrade::height());
            }
            if let Some(window) = &mut game.reform {
                window.frame.drag_to(self.cursor, reform::HEIGHT);
            }
            if let Some(window) = &mut game.enchant {
                window.frame.drag_to(self.cursor, enchant::HEIGHT);
            }
            if let Some(window) = &mut game.monster_window {
                window.frame.drag_to(self.cursor, monster_info::MonsterWindow::height());
            }
            if let Some(window) = &mut game.stylist {
                window.frame.drag_to(self.cursor, stylist::HEIGHT);
            }
            if let Some(window) = &mut game.audio_window
                && window.drag_to(self.cursor, &mut self.sound)
                && let Some(audio) = &mut self.audio
            {
                audio.set(&self.sound);
            }
            if let Some(window) = &mut game.graphics_window {
                window.drag_to(self.cursor);
            }
            if let Some(window) = &mut game.shortcuts_window {
                window.drag_to(self.cursor);
            }
            game.camera_goal.pointer_moved(cursor);
            game.camera_goal.drag(self.cursor, self.modifiers);
            if let Some((_, start, moved)) = &mut game.dragging {
                *moved |= (self.cursor[0] - start[0]).abs() + (self.cursor[1] - start[1]).abs() > 4.0;
            }
        }
    }

    /// The right button: Alt moves stacks to storage, a click on a player opens their menu, a press aims the camera.
    fn right_button(&mut self, state: ElementState) {
        // A right press lets go of a held drag and closes the box asking how many of an item.
        if let Scene::Game(game) = &mut self.scene
            && state == ElementState::Pressed
        {
            let dropped = game.dragging.take().is_some();
            game.drag_cancelled |= dropped;
            let closed = game.amount.take_if(|amount| amount.what.moves_items()).is_some();
            if dropped || closed {
                return;
            }
        }
        // Alt + right press orders the homunculus (when it's out), off the windows.
        if let (Scene::Game(game), Some(assets)) = (&mut self.scene, &self.assets)
            && state == ElementState::Pressed
            && self.modifiers.alt_key()
            && !game.on_ui(self.cursor, &self.items, assets.message.size)
            && game.companion_press(true, self.cursor, self.modifiers.shift_key())
        {
            return;
        }
        // On the character select, a card's menu of moving and renaming (only once the PIN check has passed).
        if let Scene::CharSelect(select) = &mut self.scene
            && state == ElementState::Pressed
            && select.unlocked()
        {
            select.open_menu(self.cursor, &self.font, &self.messages);
        }
        // The room owner's right click on another member's name: hand the room over, or kick them out.
        if let Scene::Game(game) = &mut self.scene
            && state == ElementState::Released
            && let Some((start, None)) = game.right_press
            && (start[0] - self.cursor[0]).abs() + (start[1] - self.cursor[1]).abs() < 4.0
            && let Some(room) = &game.room
            && let Some(chatroom::RoomHit::Member(row)) = room.hit(self.cursor, &game.me.name)
            && room.member_choices(row, &game.me.name).is_some()
        {
            game.right_press = None;
            let label = |message: usize| self.messages.get(message).cloned().unwrap_or_default();
            let choices = vec![Some((label(ROOM_HAND_OVER), Choice::RoomOwner(row))), Some((label(ROOM_KICK), Choice::RoomKick(row)))];
            game.menu = Some(menu::Menu::new(self.cursor, choices, &self.font));
            return;
        }
        // The leader's right click (not a drag) on another member's row in the party window: hand the party over.
        if let Scene::Game(game) = &mut self.scene
            && state == ElementState::Released
            && let Some((start, None)) = game.right_press
            && (start[0] - self.cursor[0]).abs() + (start[1] - self.cursor[1]).abs() < 4.0
            && game.window_at(self.cursor) == Some(Panel::Party)
            && let Some(party::Hit::Member(account)) = game.party_window.hit(self.cursor, game.party.as_ref(), &game.friends, game.account_id)
            && game.party.as_ref().is_some_and(|party| party.offers_leader(game.account_id, account))
        {
            game.right_press = None;
            let label = self.messages.get(LEADER_CHOICE).cloned().unwrap_or_default();
            game.menu = Some(menu::Menu::new(self.cursor, vec![Some((label, Choice::Leader(account)))], &self.font));
            return;
        }
        // A right click on a tracked quest's title: its details, or putting it aside.
        if let Scene::Game(game) = &mut self.scene
            && state == ElementState::Released
            && let Some((start, None)) = game.right_press
            && (start[0] - self.cursor[0]).abs() + (start[1] - self.cursor[1]).abs() < 4.0
            && game.quest_window.tracker
            && game.window_at(self.cursor).is_none()
            && let Some(id) = quest_tracker::title_at(
                &quest_tracker::blocks(&game.quests, &self.quest_texts, &game.quest_window.hidden, &self.messages),
                &self.font,
                self.cursor,
            )
        {
            game.right_press = None;
            let label = |message: usize| self.messages.get(message).cloned().unwrap_or_default();
            let choices = vec![Some((label(QUEST_INFO_CHOICE), Choice::QuestInfo(id))), Some((label(QUEST_DELETE_CHOICE), Choice::QuestAside(id)))];
            game.menu = Some(menu::Menu::new(self.cursor, choices, &self.font));
            return;
        }
        // A right click on a skill, in the skill window or on the hotkey bar: its description (the same skill again
        // closes it).
        if let Scene::Game(game) = &mut self.scene
            && state == ElementState::Released
            && let Some((start, None)) = game.right_press
            && (start[0] - self.cursor[0]).abs() + (start[1] - self.cursor[1]).abs() < 4.0
        {
            let cursor = self.cursor;
            let opened = match game.window_at(cursor) {
                Some(Panel::Skills) => {
                    let jobs = skills::tabs(&self.skill_trees, game.me.job, &game.skills);
                    let sheet =
                        skills::Sheet { trees: &self.skill_trees, jobs: &jobs, skills: &game.skills, info: &game.skill_info, points: game.skill_points() };
                    // The skill's prerequisites stay lit until another right click.
                    let skill = match game.skill_window.hit(cursor, &sheet) {
                        Some(skills::Hit::Skill(id)) => Some(id),
                        _ => None,
                    };
                    game.skill_window.pin(skill);
                    skill.map(|id| (id, None))
                }
                Some(Panel::Hotkeys) => match game.hotkeys.hit(cursor) {
                    Some(hotkeys::Hit::Slot(slot)) => game.hotkeys.keys.get(slot).copied().flatten().filter(|key| key.skill).and_then(|key| {
                        let row = game.hotkeys.slot_boxes().into_iter().find(|(index, _)| *index == slot).map(|(_, area)| [area[1], area[1] + area[3]])?;
                        Some((key.id as u16, Some(row)))
                    }),
                    _ => None,
                },
                _ => None,
            };
            if let Some((id, row)) = opened {
                game.right_press = None;
                if game.skill_description.as_ref().is_some_and(|window| window.skill == id) {
                    game.skill_description = None;
                } else {
                    let lines = self.skill_descriptions.get(&id).cloned().unwrap_or_default();
                    game.skill_description = Some(skill_descript::SkillDescription::new(id, &lines, |height| match row {
                        Some(row) => skill_descript::at_hotbar(cursor[0], row, height),
                        None => skill_descript::at_window(cursor, height),
                    }));
                }
                game.sync_esc();
                return;
            }
        }
        // A right click on a row of the guild's members page: the master moves someone else, a member leaves.
        if let Scene::Game(game) = &mut self.scene
            && state == ElementState::Released
            && let Some((start, None)) = game.right_press
            && (start[0] - self.cursor[0]).abs() + (start[1] - self.cursor[1]).abs() < 4.0
            && game.window_at(self.cursor) == Some(Panel::Guild)
            && let Some(guild) = &game.guild
            && let Some(guild::Hit::Member(row)) = game.guild_window.hit(self.cursor, guild)
        {
            let choices = guild.member_menu(game.me.id, row);
            if !choices.is_empty() {
                game.right_press = None;
                let entries = choices
                    .into_iter()
                    .map(|choice| match choice {
                        guild::MemberChoice::Move(position) => Some((guild.title(position), Choice::GuildMove(row, position))),
                        guild::MemberChoice::Leave => Some((self.messages.get(LEAVE_CHOICE).cloned().unwrap_or_default(), Choice::GuildLeave)),
                    })
                    .collect();
                game.menu = Some(menu::Menu::new(self.cursor, entries, &self.font));
                return;
            }
        }
        if let Scene::Game(game) = &mut self.scene {
            let assets = self.assets.as_ref();
            // A right press lets go of a skill being aimed.
            if state == ElementState::Pressed {
                game.targeting = None;
            }
            // A right press on an NPC talks too.
            if state == ElementState::Pressed
                && let Some(assets) = assets
                && !game.on_ui(self.cursor, &self.items, assets.message.size)
                && let Some(Owner::Unit(id)) = hovered_unit(&game.hit_boxes, self.cursor, false)
                && game.units.get(&id).is_some_and(|actor| actor.kind == NPC && actor.job != WARP_JOB)
            {
                game.talk_to(id);
            }
            // A right click on an inventory item with a modifier: Alt sends the stack to what's open, Shift moves it
            // into the favourites or out, Ctrl identifies it with a Magnifier.
            let (alt, shift, ctrl) = (self.modifiers.alt_key(), self.modifiers.shift_key(), self.modifiers.control_key());
            if state == ElementState::Pressed
                && game.window_at(self.cursor) == Some(Panel::Inventory)
                && let Some(inventory::Hit::Item(index)) = game.inventory.hit(self.cursor)
                && let Some(item) = game.inventory.get(index).cloned()
                && let Some(action) = inventory_right(alt, shift, ctrl)
            {
                let line = match action {
                    InventoryRight::QuickMove => game.quick_move(index),
                    InventoryRight::Favorite => game.session.move_favorite(index, item.favorite).err().and(None),
                    InventoryRight::Identify if item.identified => None,
                    InventoryRight::Identify if game.inventory.items.iter().any(|carried| MAGNIFIERS.contains(&carried.id)) => {
                        game.session.identify_one(index).err().and(None)
                    }
                    InventoryRight::Identify => Some(NO_MAGNIFIER),
                };
                if let Some(line) = line {
                    game.chat_line(self.messages.get(line).cloned().unwrap_or_default(), CHAT_ERROR);
                }
                return;
            }
            // Alt + right click takes a whole stack out of the open storage.
            let moved = match game.window_at(self.cursor) {
                _ if state != ElementState::Pressed || !self.modifiers.alt_key() || !game.storage.frame.open => None,
                Some(Panel::Storage) => match assets.and_then(|assets| game.storage.hit(self.cursor, &assets.storage, &self.items)) {
                    Some(storage::Hit::Item(index)) => game.storage.get(index).map(|item| (Asked::Take(index), item.amount)),
                    _ => None,
                },
                _ => None,
            };
            // A right click (not a drag) on another player (or our pet) opens the menu of what can be done with them;
            // the monsters and items around don't get in the way.
            let with_menu = game.hit_boxes.iter().copied().filter(|target| match target.owner {
                Owner::Unit(id) => game.units.get(&id).is_some_and(|actor| actor.kind == PLAYER) || game.pet_id == Some(id),
                _ => false,
            });
            let on_unit = match picking::pick(with_menu, self.cursor, true) {
                _ if game.window_at(self.cursor).is_some() || game.chat.blocks(self.cursor) => None,
                Some(Owner::Unit(id)) => Some(id),
                _ => None,
            };
            if state == ElementState::Pressed {
                game.menu = None;
                game.right_press = Some((self.cursor, on_unit));
                game.right_moved = false;
            } else if let Some((_, Some(id))) = game.right_press.take()
                && menu_opens(game.right_moved, self.modifiers.shift_key(), self.mouse_down, false)
                && let Some(actor) = game.units.get(&id)
            {
                let fill = |id: usize| rustnarok_client::messages::fill(self.messages.get(id).map_or("", String::as_str), &[&actor.name]);
                let mut choices = Vec::new();
                // Our pet: its commands, the accessory's while it wears one.
                if Some(id) == game.pet_id {
                    let accessory = game.pet.as_ref().is_some_and(|pet| pet.accessory != 0);
                    let commands = PET_CHOICES.into_iter().filter(|&(_, command)| command != net::PET_UNEQUIP || accessory);
                    choices.extend(commands.map(|(label, command)| Some((fill(label), Choice::Pet(command)))));
                } else {
                    choices.push(Some((fill(TRADE_CHOICE), Choice::Trade(id))));
                    // The guild's master can invite, and ask another guild for an alliance or set it as an antagonist.
                    // ponytail: where the two sit in the menu is an estimate.
                    if game.guild.as_ref().is_some_and(|guild| guild.master && guild.members.iter().all(|member| member.account != id)) {
                        choices.push(Some((fill(GUILD_CHOICE), Choice::GuildInvite(id))));
                    }
                    if game.guild.as_ref().is_some_and(|guild| guild.master && actor.guild != 0 && actor.guild != guild.id) {
                        choices.push(Some((fill(ALLY_CHOICE), Choice::Ally(id))));
                        choices.push(Some((fill(ANTAGONIST_CHOICE), Choice::Antagonist(id))));
                    }
                    // Not yet a friend: offer it, under a line.
                    if game.friends.iter().all(|friend| friend.account != id) {
                        choices.extend([None, Some((fill(FRIEND_CHOICE), Choice::Befriend(id)))]);
                    }
                    // The party's leader can invite players not in it.
                    if let Some(party) = &game.party
                        && party.leader() == Some(game.account_id)
                        && party.members.iter().all(|member| member.info.account != id)
                    {
                        choices.extend([None, Some((fill(PARTY_ASK), Choice::Invite(id)))]);
                    }
                    // Married (wearing the ring): a party member can be asked to be adopted.
                    // ponytail: where it sits in the menu is an estimate.
                    if game.party.as_ref().is_some_and(|party| party.members.iter().any(|member| member.info.account == id))
                        && game.inventory.items.iter().any(|item| item.worn != 0 && WEDDING_RINGS.contains(&item.id))
                    {
                        choices.extend([None, Some((fill(ADOPT_CHOICE), Choice::Adopt(id)))]);
                    }
                    // ponytail: last, under a line; where the real client lists it isn't known.
                    choices.extend([None, Some((fill(VIEW_CHOICE), Choice::ViewEquipment(id)))]);
                }
                game.menu = Some(menu::Menu::new(self.cursor, choices, &self.font));
            }
            // A right click on an item (inventory, storage, what's worn, a shop's row) describes it.
            let shop_row = match game.shop.as_ref().and_then(|shop| shop.hit(self.cursor)) {
                Some(shop::Hit::Row(window, row)) => game.shop.as_ref().and_then(|shop| shop.offer_at(window, row)),
                _ => None,
            };
            let described = match game.window_at(self.cursor) {
                _ if state != ElementState::Pressed || self.modifiers.alt_key() => None,
                _ if shop_row.is_some() => shop_row.map(|offer| net::InventoryItem {
                    id: offer.item,
                    kind: offer.kind,
                    amount: 1,
                    identified: offer.identified,
                    refine: offer.refine,
                    ..Default::default()
                }),
                Some(Panel::Inventory) => match game.inventory.hit(self.cursor) {
                    Some(inventory::Hit::Item(index)) => game.inventory.get(index).cloned(),
                    _ => None,
                },
                Some(Panel::Storage) => match assets.and_then(|assets| game.storage.hit(self.cursor, &assets.storage, &self.items)) {
                    Some(storage::Hit::Item(index)) => game.storage.get(index).cloned(),
                    _ => None,
                },
                Some(Panel::Cart) => match game.cart.hit(self.cursor) {
                    Some(inventory::Hit::Item(index)) => game.cart.get(index).cloned(),
                    _ => None,
                },
                Some(Panel::Equipment) => match game.equipment.hit(self.cursor, &game.inventory.items) {
                    Some(equipment::Hit::Slot(index)) => game.inventory.get(index).cloned(),
                    _ => None,
                },
                _ => None,
            };
            if let Some(item) = described {
                let damaged = item.broken.then(|| self.messages.get(ITEM_DAMAGED).cloned().unwrap_or_default());
                let shown = iteminfo::Shown::of(&item, damaged);
                let position = game.item_window.as_ref().map(|window| window.position);
                let mut window = iteminfo::ItemWindow::new(shown);
                if let Some(position) = position {
                    window.position = position;
                }
                game.item_window = Some(window);
            } else if let Some((asked, amount)) = moved {
                game.give(asked, u32::from(amount));
            } else if state == ElementState::Pressed && (game.targeting.take().is_some() | std::mem::take(&mut game.capturing)) {
            } else if state == ElementState::Pressed {
                game.camera_goal.press(self.cursor, Instant::now(), self.modifiers.control_key(), self.modifiers.shift_key());
            } else {
                game.camera_goal.dragging = None;
            }
        }
    }

    /// The left button went down or up: a press grabs, a release drops or clicks.
    /// The wheel turned `rows` notches (positive: toward the player): it scrolls what the pointer is over, or zooms.
    fn wheel(&mut self, rows: i32) {
        // Over a window the wheel zooms only with Ctrl (and never while aiming).
        if let (Scene::Game(game), Some(assets)) = (&mut self.scene, &self.assets)
            && self.modifiers.control_key()
            && game.targeting.is_none()
            && game.on_ui(self.cursor, &self.items, assets.message.size)
        {
            game.camera_goal.wheel(rows, Instant::now(), true, self.modifiers.shift_key());
            return;
        }
        if let Scene::Game(game) = &mut self.scene {
            // Over the inventory the wheel scrolls it; elsewhere it zooms (away from you zooms out).
            let on_shop = game.shop.as_ref().is_some_and(|shop| shop.covers(self.cursor));
            let on_pick = game.pick.as_ref().is_some_and(|(list, _)| list.covers(self.cursor));
            let on_item = game.item_window.as_ref().is_some_and(|window| window.hit(self.cursor, &self.items).is_some());
            let on_setup = game.vend_setup.as_ref().is_some_and(|setup| setup.hit(self.cursor, &game.cart.items).is_some());
            let on_buying_setup = game.buying_setup.as_ref().is_some_and(|setup| setup.hit(self.cursor, &game.inventory.items).is_some());
            let on_cashshop = game.cashshop.as_ref().is_some_and(|shop| shop.hit(self.cursor).is_some());
            let on_synthesis = game.synthesis.as_ref().is_some_and(|window| window.hit(self.cursor).is_some());
            let on_upgrade = game.upgrade.as_ref().is_some_and(|window| window.hit(self.cursor).is_some());
            let on_reform = game.reform.as_ref().is_some_and(|window| window.hit(self.cursor, &game.inventory.items).is_some());
            match game.window_at(self.cursor) {
                // Aiming, the wheel picks the level (up raises it), wherever the pointer is.
                _ if game.targeting.is_some() => {
                    let (id, level) = game.targeting.unwrap();
                    let separate = self.skill_info.get(&id).is_some_and(|info| info.separate_level);
                    let learned = game.skills.iter().find(|skill| skill.id == id).map_or(level, |skill| skill.level);
                    game.targeting = Some((id, aim_level(level, learned, rows, separate)));
                }
                _ if on_synthesis => game.synthesis.as_mut().unwrap().scroll_by(rows),
                _ if on_upgrade => game.upgrade.as_mut().unwrap().scroll_by(rows),
                _ if game.enchant.as_ref().is_some_and(|window| window.hit(self.cursor).is_some()) => {
                    let index = game.enchant.as_ref().and_then(|window| window.item);
                    let item = index.and_then(|index| game.inventory.items.iter().find(|item| item.index == index)).cloned();
                    game.enchant.as_mut().unwrap().scroll_by(rows, item.as_ref());
                }
                _ if on_cashshop => game.cashshop.as_mut().unwrap().scroll_at(self.cursor, rows),
                _ if on_reform => game.reform.as_mut().unwrap().scroll_by(rows, &game.inventory.items),
                _ if on_setup => {
                    let count = game.cart.items.len();
                    game.vend_setup.as_mut().unwrap().scroll_by(rows, count);
                }
                _ if on_buying_setup => {
                    let inv = &game.inventory.items;
                    game.buying_setup.as_mut().unwrap().scroll_by(rows, inv);
                }
                _ if game.card_view.as_ref().is_some_and(|view| view.hit(self.cursor).is_some()) => {}
                _ if on_item => {
                    let lines = game.item_lines;
                    game.item_window.as_mut().unwrap().scroll_by(rows, lines);
                }
                _ if on_shop => game.shop.as_mut().unwrap().scroll_by(self.cursor, rows),
                _ if game.skill_description.as_ref().is_some_and(|window| window.contains(self.cursor)) => {
                    game.skill_description.as_mut().unwrap().scroll_by(rows);
                }
                _ if on_pick => game.pick.as_mut().unwrap().0.scroll_by(rows),
                _ if game.achievement_window.covers(self.cursor) => game.achievement_window.scroll_by(rows, &self.achievement_info),
                Some(Panel::Inventory) => game.inventory.scroll_by(rows),
                Some(Panel::Cart) => game.cart.scroll_by(rows),
                Some(Panel::Skills) => {
                    let jobs = skills::tabs(&self.skill_trees, game.me.job, &game.skills);
                    let sheet =
                        skills::Sheet { trees: &self.skill_trees, jobs: &jobs, skills: &game.skills, info: &game.skill_info, points: game.skill_points() };
                    game.skill_window.scroll_by(&sheet, rows);
                }
                Some(Panel::Guild) => {
                    if let Some(guild) = &game.guild {
                        game.guild_window.scroll_by(rows, guild);
                    }
                }
                Some(Panel::Storage) => {
                    if let Some(assets) = &self.assets {
                        game.storage.scroll_by(rows, &assets.storage, &self.items);
                    }
                }
                Some(_) => {}
                None if game.chat.contains(self.cursor) => game.chat.scroll_by(rows),
                None => game.camera_goal.wheel(rows, Instant::now(), self.modifiers.control_key(), self.modifiers.shift_key()),
            }
        }
    }

    fn left_button(&mut self, event_loop: &ActiveEventLoop, state: ElementState) {
        self.mouse_down = state == ElementState::Pressed;
        if let Scene::Game(game) = &mut self.scene
            && state == ElementState::Released
        {
            game.walk_held = false;
            // An attack held down stops when let go, once it went out.
            if let Some(attack) = &mut game.attack
                && attack.release() == Release::Cancel
            {
                game.attack = None;
                let _ = game.session.stop_attack();
            }
        }
        let message_size = self.assets.as_ref().map_or([0.0; 2], |assets| assets.message.size);
        let dragged = match &mut self.scene {
            Scene::Game(game) if state == ElementState::Released => {
                let item = game.dragging.take();
                // An item, a skill or a hotkey let go over a hotkey slot goes there (`hotkeys::drop_on`).
                if let Some((dragged, _, true)) = item
                    && let Some(hotkeys::Hit::Slot(slot)) = game.hotkeys.hit(self.cursor)
                {
                    let key = match dragged {
                        Dragged::Hotkey(from) if from != slot => game.hotkeys.keys[from],
                        Dragged::Item(index) => game.inventory.get(index).map(|item| net::Hotkey { skill: false, id: item.id, count: item.amount as i16 }),
                        // At the level picked in the skill window.
                        Dragged::Skill(id) => game.skills.iter().find(|skill| skill.id == id).map(|skill| net::Hotkey {
                            skill: true,
                            id: u32::from(id),
                            count: game.skill_window.use_level(id, skill.level) as i16,
                        }),
                        Dragged::Stored(_) | Dragged::Carted(_) | Dragged::Hotkey(_) | Dragged::Worn(_) | Dragged::ShopOffer(_) | Dragged::ShopDeal(_) => None,
                    };
                    if let Some(key) = key {
                        for (slot, key) in hotkeys::drop_on(&game.hotkeys.keys, key, slot) {
                            game.hotkeys.keys[slot] = key;
                            let _ = game.session.set_hotkey(game.hotkeys.tab(), slot as u16, key);
                        }
                    }
                }
                // A skill hotkey let go off the slots, on the bar or over the world, is taken off. An item hotkey stays:
                // over the world it can't be dropped. Over another window nothing happens.
                if let Some((Dragged::Hotkey(from), _, true)) = item
                    && let Some(key) = game.hotkeys.keys[from]
                {
                    let on_bar = game.hotkeys.hit(self.cursor);
                    let elsewhere = on_bar.is_none() && game.window_at(self.cursor).is_some();
                    match on_bar {
                        Some(hotkeys::Hit::Slot(_)) => {}
                        _ if elsewhere => {}
                        _ if key.skill => {
                            game.hotkeys.keys[from] = None;
                            let _ = game.session.set_hotkey(game.hotkeys.tab(), from as u16, None);
                        }
                        Some(_) => {}
                        None => game.chat_line(self.messages.get(HOTKEY_ITEM_STAYS).cloned().unwrap_or_default(), CHAT_ERROR),
                    }
                }
                // A shop's offer let go over its deal goes in; a deal row let go over its goods comes out.
                if let Some((Dragged::ShopOffer(row) | Dragged::ShopDeal(row), _, true)) = item
                    && let Some(shop) = &mut game.shop
                {
                    let zeny = game.status.get(&net::STATUS_ZENY).copied().unwrap_or(0).max(0) as u64;
                    shop.zeny = zeny;
                    let action = match (item.map(|(dragged, ..)| dragged), shop.window_at(self.cursor)) {
                        (Some(Dragged::ShopOffer(_)), Some(1)) => shop.drop_offer(row),
                        (Some(Dragged::ShopDeal(_)), Some(0)) => shop.drop_deal(row),
                        _ => None,
                    };
                    match action {
                        Some(shop::Action::Ask(row, fill)) => game.amount = Some(window::Amount::new(Asked::Shop(row), (fill > 0).then_some(fill))),
                        Some(shop::Action::AskOut(row, fill)) => game.amount = Some(window::Amount::new(Asked::ShopOut(row), Some(fill))),
                        Some(shop::Action::Say(message, arg)) => {
                            let text = self.messages.get(message).map_or("", String::as_str);
                            let line = arg.map_or_else(|| text.to_owned(), |arg| rustnarok_client::messages::fill(text, &[&arg]));
                            game.chat_line(line, CHAT_ERROR);
                        }
                        _ => {}
                    }
                }
                // An item let go over the storage goes in; a stored one let go over the inventory comes out.
                let on_trade = game.trade.as_ref().is_some_and(|trade| trade.hit(self.cursor).is_some());
                let on_mail = matches!(game.mail.hit(self.cursor), Some(mail::Hit::Compose(_)));
                let on_refine = game.refine.as_ref().is_some_and(|window| window.hit(self.cursor).is_some());
                let on_grade = game.grade.as_ref().is_some_and(|window| window.hit(self.cursor).is_some());
                let on_enchant = game.enchant.as_ref().is_some_and(|window| window.hit(self.cursor).is_some());
                let on_materials = game.materials.as_ref().is_some_and(|window| window.hit(self.cursor).is_some());
                let on_synthesis = game.synthesis.as_ref().is_some_and(|window| window.hit(self.cursor).is_some());
                let on_upgrade = game.upgrade.as_ref().is_some_and(|window| window.hit(self.cursor).is_some());
                match (item, game.window_at(self.cursor)) {
                    (Some((Dragged::Item(index), _, true)), _) if on_synthesis || on_upgrade => {
                        if let Some(item) = game.inventory.get(index).cloned() {
                            let said = if on_synthesis {
                                game.synthesis.as_mut().and_then(|window| match window.put(&item) {
                                    laphine::Put::Say(id) => Some(id),
                                    _ => None,
                                })
                            } else {
                                game.upgrade.as_mut().and_then(|window| match window.put(&item) {
                                    laphine::Put::Say(id) => Some(id),
                                    _ => None,
                                })
                            };
                            if let Some(id) = said {
                                game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ERROR);
                            }
                        }
                    }
                    (Some((Dragged::Item(index), _, true)), _) if on_mail => game.move_item(Asked::Mail(index)),
                    (Some((Dragged::Item(index), _, true)), _) if on_materials => game.move_item(Asked::Material(index)),
                    // Into the refine machine: the server lists what refines it.
                    (Some((Dragged::Item(index), _, true)), _) if on_refine => {
                        let _ = game.session.refine_select(index);
                    }
                    // Onto the grade window: the server lists what raises it.
                    (Some((Dragged::Item(index), _, true)), _) if on_grade => {
                        let _ = game.session.grade_select(index);
                    }
                    // Into the enchant window: it stays here until a request; the server hears the item with that.
                    (Some((Dragged::Item(index), _, true)), _) if on_enchant => {
                        let slots = game.inventory.get(index).and_then(|item| self.items.get(&item.id)).map(|info| info.slots).unwrap_or(0);
                        if let Some(item) = game.inventory.get(index).cloned()
                            && let Some(window) = &mut game.enchant
                            && let Err(Some(id)) = window.try_item(&item, slots)
                        {
                            game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ERROR);
                        }
                    }
                    (Some((Dragged::Item(index), _, true)), _) if on_trade => {
                        if let Some(id) = game.offer(index, false) {
                            game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ERROR);
                        }
                    }
                    (Some((Dragged::Item(index), _, true)), Some(Panel::Storage)) => game.move_item(Asked::Store(index)),
                    // Let go over the world: dropped at the player's feet, asking how many of a stack. The drop lock holds it.
                    (Some((Dragged::Item(index), _, true)), None)
                        if inventory::may_ground_drop(game.inventory.lock_drop)
                            && !game.on_ui(self.cursor, &self.items, message_size)
                            && !game.store_open() =>
                    {
                        game.drop_on_ground(index, &self.messages)
                    }
                    // Onto the equipment window, wherever: worn. Off it onto the inventory: taken off. Not while trading.
                    (Some((Dragged::Item(index), _, true)), Some(Panel::Equipment))
                        if !game.equipment.swapping
                            && game.equipment.owner.is_none()
                            && game.trade.is_none()
                            && game.inventory.get(index).is_some_and(|item| item.location != 0 && item.worn == 0) =>
                    {
                        game.use_item(index)
                    }
                    (Some((Dragged::Worn(index), _, true)), Some(Panel::Inventory)) if game.trade.is_none() => {
                        let _ = game.session.take_off(index);
                    }
                    (Some((Dragged::Item(index), _, true)), Some(Panel::Cart)) => game.move_item(Asked::ToCart(index)),
                    (Some((Dragged::Carted(index), _, true)), Some(Panel::Inventory)) => game.move_item(Asked::FromCart(index)),
                    (Some((Dragged::Stored(index), _, true)), Some(Panel::Inventory)) => game.move_item(Asked::Take(index)),
                    // Onto the swap view: into the Swap Equipment set, where it would be worn.
                    (Some((Dragged::Item(index), _, true)), Some(Panel::Equipment)) if game.equipment.swapping => {
                        if let Some(item) = game.inventory.get(index) {
                            game.switch_asked = Some(index);
                            let _ = game.session.switch_add(index, item.location);
                        }
                    }
                    _ => {}
                }
                let trade_moved = game.chat.drop_drag()
                    | game.trade.as_mut().is_some_and(|trade| trade.frame.drop_drag())
                    | game.shop.as_mut().is_some_and(|shop| shop.frames.iter_mut().fold(false, |moved, frame| frame.drop_drag() | moved))
                    | game.room.as_mut().is_some_and(|room| room.frame.drop_drag())
                    | game.card_view.as_mut().is_some_and(|view| view.drop_drag())
                    | game.item_window.as_mut().is_some_and(|window| window.drop_drag())
                    | game.mail.drop_drag()
                    | game.quest_window.frame.drop_drag()
                    | game.achievement_window.frame.drop_drag()
                    | game.synthesis.as_mut().is_some_and(|window| window.frame.drop_drag())
                    | game.upgrade.as_mut().is_some_and(|window| window.frame.drop_drag())
                    | game.roulette.as_mut().is_some_and(|window| window.frame.drop_drag())
                    | game.reform.as_mut().is_some_and(|window| window.frame.drop_drag())
                    | game.grade.as_mut().is_some_and(|window| window.frame.drop_drag())
                    | game.enchant.as_mut().is_some_and(|window| window.frame.drop_drag())
                    | game.monster_window.as_mut().is_some_and(|window| window.frame.drop_drag())
                    | game.stylist.as_mut().is_some_and(|window| window.frame.drop_drag());
                // A slider let go keeps its volume for next time.
                let slid = game.audio_window.as_mut().is_some_and(options::AudioWindow::drop_drag);
                if slid {
                    self.sound.save();
                }
                let option_windows = game.graphics_window.as_mut().is_some_and(options::GraphicsWindow::drop_drag)
                    | game.shortcuts_window.as_mut().is_some_and(options::ShortcutsWindow::drop_drag);
                let member_held =
                    game.member_windows.release(game.party_window.hit(self.cursor, game.party.as_ref(), &game.friends, game.account_id).is_none(), self.cursor);
                let windows = trade_moved
                    | slid
                    | option_windows
                    | member_held
                    | game.party_window.frame.drop_drag()
                    | game.status_window.frame.drop_drag()
                    | game.guild_window.frame.drop_drag()
                    | game.clan_window.frame.drop_drag()
                    | game.cart.frame.drop_drag()
                    | game.pet_window.frame.drop_drag()
                    | game.mercenary_window.frame.drop_drag()
                    | game.mercenary_skill_list.frame.drop_drag()
                    | game.homunculus_window.frame.drop_drag()
                    | game.homunculus_skill_list.frame.drop_drag()
                    | game.storage.frame.drop_drag()
                    | game.inventory.frame.drop_drag()
                    | game.equipment.frame.drop_drag()
                    | game.basic_info.frame.drop_drag()
                    | game.hotkeys.frame.drop_drag()
                    | game.skill_window.frame.drop_drag()
                    | game.talks.drop_drag()
                    | game.friend_setup.as_mut().is_some_and(|setup| setup.frame.drop_drag())
                    | game.block_list.as_mut().is_some_and(|list| list.frame.drop_drag())
                    | game.minimap_window.frame.drop_drag();
                windows || item.is_some_and(|(_, _, moved)| moved) || std::mem::take(&mut game.drag_cancelled)
            }
            _ => false,
        };
        if state == ElementState::Pressed {
            self.press();
        } else if !dragged {
            self.click(event_loop);
        }
    }

    /// Dev switch `RUSTNAROK_CLIENT_CLICKS=x,y[>x,y][;...]`: clicks, or drags to the second point, one a second from
    /// `RUSTNAROK_CLIENT_CLICKS_AT` seconds (4 by default) after entering the map.
    fn auto_input(&mut self, event_loop: &ActiveEventLoop) {
        let Scene::Game(game) = &self.scene else { return };
        let elapsed = game.entered.elapsed();
        while let Some(&(at, right, from, to)) = self.auto_input.last()
            && elapsed >= at
        {
            self.auto_input.pop();
            self.cursor = from;
            self.pointer_moved();
            let button = |app: &mut Self, state| if right { app.right_button(state) } else { app.left_button(event_loop, state) };
            button(self, ElementState::Pressed);
            if let Some(to) = to {
                self.cursor = to;
                self.pointer_moved();
            }
            button(self, ElementState::Released);
        }
    }

    /// A mouse press: grabs a window by its title bar, and notes it for Esc.
    fn press(&mut self) {
        let cursor = self.cursor;
        // Aiming a skill: the press on its target uses it; any other press lets go of the aim. On the world that's
        // all it does (no walk); on a window the window gets the press too.
        if let (Scene::Game(game), Some(assets)) = (&mut self.scene, &self.assets)
            && game.targeting.is_some()
        {
            // ponytail: a click on the member's window casts on their account id (a player's unit id). Range is the
            // same as aiming at them in the world; the close button still closes.
            if let Some(account) = game.party_member_at(cursor) {
                game.aim_skill(SkillTarget::Unit(account));
                return;
            }
            let on_ui = game.on_ui(cursor, &self.items, assets.message.size);
            let shift = self.modifiers.shift_key();
            if on_ui {
                game.targeting = None;
            } else {
                // An NPC is talked to; a unit the press doesn't take, the ground under a unit skill, an item or a warp:
                // the aim just ends. Over 90 % weight a unit skill is refused.
                let weight = |id: u16| game.status.get(&id).copied().unwrap_or(0);
                let heavy = attack_refused(weight(net::STATUS_WEIGHT), weight(net::STATUS_MAX_WEIGHT), false);
                match game.aim_target(cursor, game.players_first(shift)) {
                    Some(SkillTarget::Unit(id)) if game.units.get(&id).is_some_and(|actor| actor.kind == NPC && actor.job != WARP_JOB) => {
                        game.targeting = None;
                        game.talk_to(id);
                    }
                    Some(SkillTarget::Unit(id)) if game.aim_takes_unit(id, shift) => match heavy {
                        Some(line) => {
                            game.targeting = None;
                            game.chat_line(self.messages.get(line).cloned().unwrap_or_default(), CHAT_ERROR);
                        }
                        None => game.aim_skill(SkillTarget::Unit(id)),
                    },
                    Some(target @ SkillTarget::Cell(_)) => game.aim_skill(target),
                    _ => game.targeting = None,
                }
                return;
            }
        }
        // A press on the ground walks there at once, and holding the button keeps following the pointer. Shift turns
        // to face it, Ctrl does nothing, and sitting only turns. A second press within DOUBLE_PRESS with the pointer
        // still is ignored. Units and items are the click's (on release).
        if let (Scene::Game(game), Some(assets)) = (&mut self.scene, &self.assets)
            && game.dragging.is_none()
            && !game.on_ui(cursor, &self.items, assets.message.size)
            && !game.store_open()
            && game.targeting.is_none()
            && !game.capturing
        {
            let now = Instant::now();
            let (double, mark) = press_pair(game.press_mark, now, cursor);
            game.press_mark = mark;
            let (shift, ctrl) = (self.modifiers.shift_key(), self.modifiers.control_key());
            // Alt + left press orders the mercenary (when it's out).
            if !double && self.modifiers.alt_key() && game.companion_press(false, cursor, shift) {
                return;
            }
            // A new press ends the attack under way.
            game.attack = None;
            match game.world_click(cursor, shift) {
                // A double press does nothing, but held on the ground the button goes on following the pointer.
                WorldClick::Ground if double => {
                    if double_holds(ctrl, shift, game.player.sitting) {
                        game.walk_held = true;
                    }
                }
                _ if double => {}
                // A monster with Shift (without /noshift): nothing.
                WorldClick::Attack(id) if shift && !game.no_shift && game.units.get(&id).is_some_and(|actor| actor.kind == MONSTER) => {}
                // An attack: kept up with Ctrl or /noctrl (on someone else's homunculus always), otherwise while the
                // button is held.
                WorldClick::Attack(id) => {
                    let homunculus = game.units.get(&id).is_some_and(|actor| actor.kind == HOMUNCULUS);
                    let weight = |id: u16| game.status.get(&id).copied().unwrap_or(0);
                    match attack_refused(weight(net::STATUS_WEIGHT), weight(net::STATUS_MAX_WEIGHT), game.riding) {
                        Some(line) => game.chat_line(self.messages.get(line).cloned().unwrap_or_default(), CHAT_ERROR),
                        None => {
                            game.attack = Some(Attack::new(id, ctrl || game.no_ctrl || homunculus));
                            game.attack_step(now);
                        }
                    }
                }
                WorldClick::Talk(id) => game.talk_to(id),
                // Held, the button follows the pointer again once the item is picked up.
                WorldClick::PickUp(id) if !game.player.sitting => {
                    game.pick_up(id, now);
                    game.walk_held = true;
                    game.last_walk_try = Some(now);
                }
                WorldClick::Warp(cell) if !ctrl => {
                    game.walk_toward(cell, now);
                    game.walk_held = true;
                    game.last_walk_try = Some(now);
                }
                WorldClick::Ground if !ctrl => {
                    if let Some(cell) = game.ground_cell(cursor) {
                        if shift || game.player.sitting {
                            game.turn_to(cell);
                        } else {
                            game.walk_toward(cell, now);
                            game.walk_held = true;
                            game.last_walk_try = Some(now);
                        }
                    }
                }
                _ => {}
            }
            return;
        }
        // The big minimap is above the other windows, under the world map.
        if let Scene::Game(game) = &mut self.scene
            && game.world_map.is_none()
            && game.minimap_window.covers(cursor)
        {
            let size = [game.world.gat.width as u32, game.world.gat.height as u32];
            if game.minimap_window.hit(cursor, size) == Some(minimap_window::Hit::Title(TitleHit::Title)) {
                game.minimap_window.frame.start_drag(cursor);
            }
            game.press_esc(cursor);
            return;
        }
        if let Scene::Game(game) = &mut self.scene {
            // Windows opened since the last Esc join the list before this press moves one to the front.
            game.sync_esc();
            // A button on the game menu is not a press on the window under it.
            let on_menu = game.escape.as_ref().is_some_and(|menu| menu.hit(cursor, game.can_resurrect()).is_some());
            if !on_menu && game.card_view.as_ref().is_some_and(|view| view.hit(cursor).is_some()) {
                escape::touch(&mut game.esc_order, escape::WindowId::CardPicture);
            } else if !on_menu && game.item_window.as_ref().is_some_and(|window| window.hit(cursor, &self.items).is_some()) {
                escape::touch(&mut game.esc_order, escape::WindowId::ItemInfo);
            } else if !on_menu {
                game.press_esc(cursor);
            }
        }
        if let Scene::Game(game) = &mut self.scene {
            let job = game.player.look.as_ref().map(|look| look.job).unwrap_or(game.me.job);
            let hit = game.stylist.as_ref().and_then(|window| window.hit(cursor, &self.styling_shop, job));
            if let Some(hit) = hit {
                if hit == stylist::Hit::Title(TitleHit::Title) {
                    game.stylist.as_mut().unwrap().frame.start_drag(cursor);
                }
                return;
            }
        }
        // A card's picture sits over the item info.
        if let Scene::Game(game) = &mut self.scene
            && let Some(view) = &mut game.card_view
            && let Some(hit) = view.hit(cursor)
        {
            if hit == iteminfo::ViewHit::Grip {
                view.start_drag(cursor);
            }
            return;
        }
        // The chat window goes by its tab strip.
        if let Scene::Game(game) = &mut self.scene
            && game.chat.press(cursor)
        {
            return;
        }
        // The item description window goes by its top (the NPC window's links are over it).
        if let Scene::Game(game) = &mut self.scene
            && links::at(&game.dialog_links, cursor).is_none()
            && let Some(window) = &mut game.item_window
            && let Some(hit) = window.hit(cursor, &self.items)
        {
            if hit == iteminfo::Hit::Grip {
                window.start_drag(cursor);
            }
            return;
        }
        // The chat room's window is over the others too.
        if let Scene::Game(game) = &mut self.scene
            && let Some(room) = &mut game.room
            && let Some(hit) = room.hit(cursor, &game.me.name)
        {
            if hit == chatroom::RoomHit::Title(TitleHit::Title) {
                room.frame.start_drag(cursor);
            }
            return;
        }
        // The deal's window is over the others.
        if let Scene::Game(game) = &mut self.scene
            && let Some(trade) = &mut game.trade
            && let Some(hit) = trade.hit(cursor)
        {
            if hit == trade::Hit::Title(TitleHit::Title) {
                trade.frame.start_drag(cursor);
            }
            return;
        }
        // A shop's row starts a drag at once; its title bar moves the window.
        if let Scene::Game(game) = &mut self.scene
            && let Some(hit) = game.shop.as_ref().and_then(|shop| shop.hit(cursor))
        {
            match hit {
                shop::Hit::Row(0, row) => game.dragging = Some((Dragged::ShopOffer(row), cursor, false)),
                shop::Hit::Row(_, row) => game.dragging = Some((Dragged::ShopDeal(row), cursor, false)),
                shop::Hit::Title(window, TitleHit::Title) => game.shop.as_mut().unwrap().frames[window].start_drag(cursor),
                _ => {}
            }
            return;
        }
        // So are the mail windows, and the audio settings.
        if let Scene::Game(game) = &mut self.scene
            && game.mail.hit(cursor).is_some()
        {
            game.mail.start_drag(cursor);
            return;
        }
        if let Scene::Game(game) = &mut self.scene
            && let Some(window) = &mut game.monster_window
            && window.hit(cursor) == Some(false)
        {
            if window.frame.local(cursor, monster_info::MonsterWindow::height()).and_then(|local| window.frame.title_hit(local)) == Some(TitleHit::Title) {
                window.frame.start_drag(cursor);
            }
            return;
        }
        if let Scene::Game(game) = &mut self.scene
            && let Some(hit) = game.synthesis.as_ref().and_then(|window| window.hit(cursor))
        {
            if hit == laphine::Hit::Title(TitleHit::Title) {
                game.synthesis.as_mut().unwrap().frame.start_drag(cursor);
            }
            return;
        }
        if let Scene::Game(game) = &mut self.scene
            && let Some(hit) = game.upgrade.as_ref().and_then(|window| window.hit(cursor))
        {
            if hit == laphine::Hit::Title(TitleHit::Title) {
                game.upgrade.as_mut().unwrap().frame.start_drag(cursor);
            }
            return;
        }
        if let Scene::Game(game) = &mut self.scene
            && let Some(hit) = game.enchant.as_ref().and_then(|window| window.hit(cursor))
        {
            if hit == enchant::Hit::Title(TitleHit::Title) {
                game.enchant.as_mut().unwrap().frame.start_drag(cursor);
            }
            return;
        }
        if let Scene::Game(game) = &mut self.scene
            && let Some(hit) = game.roulette.as_ref().and_then(|window| window.hit(cursor))
        {
            if hit == roulette::Hit::Title(TitleHit::Title) {
                game.roulette.as_mut().unwrap().frame.start_drag(cursor);
            }
            return;
        }
        if let Scene::Game(game) = &mut self.scene
            && let Some(hit) = game.reform.as_ref().and_then(|window| window.hit(cursor, &game.inventory.items))
        {
            if hit == reform::Hit::Title(TitleHit::Title) {
                game.reform.as_mut().unwrap().frame.start_drag(cursor);
            }
            return;
        }
        if let Scene::Game(game) = &mut self.scene
            && let Some(hit) = game.grade.as_ref().and_then(|window| window.hit(cursor))
        {
            if hit == grade::Hit::Title(TitleHit::Title) {
                game.grade.as_mut().unwrap().frame.start_drag(cursor);
            }
            return;
        }
        if let Scene::Game(game) = &mut self.scene
            && game.achievement_window.covers(cursor)
        {
            if game.achievement_window.hit(cursor, &self.achievement_info, &game.achievements) == Some(achievement::Hit::Title(TitleHit::Title)) {
                game.achievement_window.frame.start_drag(cursor);
            }
            return;
        }
        if let Scene::Game(game) = &mut self.scene
            && let Some(hit) = game.quest_window.hit(cursor, &game.quests)
        {
            if hit == quest::Hit::Title(TitleHit::Title) {
                game.quest_window.frame.start_drag(cursor);
            }
            return;
        }
        if let Scene::Game(game) = &mut self.scene
            && let Some(window) = &mut game.audio_window
            && let Some(hit) = window.hit(cursor)
        {
            if window.press(hit, cursor, &mut self.sound)
                && let Some(audio) = &mut self.audio
            {
                audio.set(&self.sound);
            }
            return;
        }
        if let Scene::Game(game) = &mut self.scene
            && let Some(window) = &mut game.graphics_window
            && let Some(hit) = window.hit(cursor)
        {
            window.press(hit, cursor);
            return;
        }
        if let Scene::Game(game) = &mut self.scene
            && let Some(window) = &mut game.shortcuts_window
            && let Some(hit) = window.hit(cursor)
        {
            window.press(hit, cursor);
            return;
        }
        if let Scene::Game(game) = &mut self.scene
            && let Some((account, hit)) = game.member_windows.hit(cursor)
        {
            if hit == party::MemberHit::Body {
                game.member_windows.press_window(account, cursor);
            }
            return;
        }
        if let Scene::Game(game) = &mut self.scene
            && game.party_window.compact
            && party::compact_hit(cursor, game.party.as_ref()).is_some()
        {
            return;
        }
        if let Scene::Game(game) = &mut self.scene
            && let Some(id) = game.window_at(cursor)
        {
            game.raise(id);
            if let (Panel::Inventory, Some(inventory::Hit::Item(index))) = (id, game.inventory.hit(cursor)) {
                game.dragging = Some((Dragged::Item(index), cursor, false));
            }
            if let (Panel::Hotkeys, Some(hotkeys::Hit::Slot(slot))) = (id, game.hotkeys.hit(cursor))
                && game.hotkeys.keys.get(slot).is_some_and(Option::is_some)
            {
                game.dragging = Some((Dragged::Hotkey(slot), cursor, false));
            }
            if let (Panel::Cart, Some(inventory::Hit::Item(index))) = (id, game.cart.hit(cursor)) {
                game.dragging = Some((Dragged::Carted(index), cursor, false));
            }
            if let (Panel::Equipment, Some(equipment::Hit::Slot(index))) = (id, game.equipment.hit(cursor, &game.inventory.items))
                && game.equipment.owner.is_none()
                && !game.equipment.swapping
            {
                game.dragging = Some((Dragged::Worn(index), cursor, false));
            }
            if id == Panel::Storage
                && let Some(assets) = &self.assets
            {
                match game.storage.hit(cursor, &assets.storage, &self.items) {
                    Some(storage::Hit::Item(index)) => game.dragging = Some((Dragged::Stored(index), cursor, false)),
                    Some(storage::Hit::Title(TitleHit::Title)) => game.storage.frame.start_drag(cursor),
                    _ => {}
                }
            }
            if id == Panel::Skills {
                let jobs = skills::tabs(&self.skill_trees, game.me.job, &game.skills);
                let sheet = skills::Sheet { trees: &self.skill_trees, jobs: &jobs, skills: &game.skills, info: &game.skill_info, points: game.skill_points() };
                match game.skill_window.hit(cursor, &sheet) {
                    // Not while levels are pending.
                    Some(skills::Hit::Skill(skill))
                        if game.skill_window.draggable() && game.skills.iter().any(|known| known.id == skill && known.level > 0 && known.kind != 0) =>
                    {
                        game.dragging = Some((Dragged::Skill(skill), cursor, false));
                    }
                    Some(skills::Hit::Title(TitleHit::Title)) => game.skill_window.frame.start_drag(cursor),
                    _ => {}
                }
            }
            match id {
                Panel::BasicInfo if game.basic_info.hit(cursor) == Some(basic_info::Hit::Title) => game.basic_info.frame.start_drag(cursor),
                Panel::Inventory if game.inventory.hit(cursor) == Some(inventory::Hit::Title(TitleHit::Title)) => game.inventory.frame.start_drag(cursor),
                Panel::Party if matches!(game.party_window.hit(cursor, game.party.as_ref(), &game.friends, game.account_id), Some(party::Hit::Member(_))) => {
                    if let Some(party::Hit::Member(account)) = game.party_window.hit(cursor, game.party.as_ref(), &game.friends, game.account_id) {
                        game.member_windows.press_row(account);
                    }
                }
                Panel::Party
                    if game.party_window.hit(cursor, game.party.as_ref(), &game.friends, game.account_id) == Some(party::Hit::Title(TitleHit::Title)) =>
                {
                    game.party_window.frame.start_drag(cursor)
                }
                Panel::Status
                    if game.status_window.frame.local(cursor, game.status_height).and_then(|local| game.status_window.frame.title_hit(local))
                        == Some(TitleHit::Title) =>
                {
                    game.status_window.frame.start_drag(cursor)
                }
                Panel::Guild if game.guild.as_ref().and_then(|guild| game.guild_window.hit(cursor, guild)) == Some(guild::Hit::Title(TitleHit::Title)) => {
                    game.guild_window.frame.start_drag(cursor)
                }
                Panel::Clan if game.clan.is_some() && game.clan_window.hit(cursor) == Some(clan::Hit::Title(TitleHit::Title)) => {
                    game.clan_window.frame.start_drag(cursor)
                }
                Panel::Cart if game.cart.hit(cursor) == Some(inventory::Hit::Title(TitleHit::Title)) => game.cart.frame.start_drag(cursor),
                Panel::Pet if game.pet.as_ref().and_then(|pet| game.pet_window.hit(cursor, pet)) == Some(pet::Hit::Title(TitleHit::Title)) => {
                    game.pet_window.frame.start_drag(cursor)
                }
                Panel::Homunculus
                    if game.homunculus.as_ref().and_then(|homunculus| game.homunculus_window.hit(cursor, homunculus))
                        == Some(homunculus::Hit::Title(TitleHit::Title)) =>
                {
                    game.homunculus_window.frame.start_drag(cursor)
                }
                Panel::HomunculusSkills
                    if game.homunculus_skill_list.hit(
                        cursor,
                        &game.homunculus_skills,
                        game.homunculus.as_ref().map_or(0, |homunculus| homunculus.skill_points),
                    ) == Some(homunculus::SkillHit::Title(TitleHit::Title)) =>
                {
                    game.homunculus_skill_list.frame.start_drag(cursor)
                }
                Panel::Mercenary if game.mercenary_window.hit(cursor) == Some(mercenary::Hit::Title(TitleHit::Title)) => {
                    game.mercenary_window.frame.start_drag(cursor)
                }
                Panel::MercenarySkills
                    if game.mercenary_skill_list.hit(cursor, &game.mercenary_skills) == Some(mercenary::SkillHit::Title(TitleHit::Title)) =>
                {
                    game.mercenary_skill_list.frame.start_drag(cursor)
                }
                Panel::Equipment if game.equipment.hit(cursor, &game.inventory.items) == Some(equipment::Hit::Title(TitleHit::Title)) => {
                    game.equipment.frame.start_drag(cursor)
                }
                _ => {}
            }
        }
    }

    fn close_message(&mut self) {
        if let Scene::Message(_, back) = std::mem::replace(&mut self.scene, Scene::Login) {
            self.scene = *back;
        }
    }

    /// Sends what's on the chat's input line, as Enter does: the client's sound commands (/music, /sound, /bv, /v),
    /// else `Game::send_chat` (party with `party`, guild with `guild`).
    fn send_chat_input(&mut self, party: bool, guild: bool) {
        let Scene::Game(game) = &mut self.scene else { return };
        if let Some(shown) = self.sound.command(game.chat.input.trim()) {
            game.chat.input.clear();
            if let Some(audio) = &mut self.audio {
                audio.set(&self.sound);
            }
            self.sound.save();
            if let Some(id) = shown {
                game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ITEM);
            }
        } else {
            let effects_on = self.graphics.effects_on;
            // The receiving group button's choice, or the key held.
            let (party, guild) = (party || game.chat.send_to == chat::SendTo::Party, guild || game.chat.send_to == chat::SendTo::Guild);
            if let Some(id) = game.send_chat(party, guild, &self.messages, &mut self.graphics.effects_on) {
                game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ERROR);
            }
            if effects_on != self.graphics.effects_on {
                self.graphics.save();
            }
        }
    }

    /// Alt+1 to Alt+0: sends macro line `line` as if typed and sent with Enter, keeping what the player was typing.
    fn send_macro(&mut self, line: usize) {
        let Scene::Game(game) = &mut self.scene else { return };
        let text = game.macro_lines[line].clone();
        if text.trim().is_empty() {
            return;
        }
        let typing = std::mem::replace(&mut game.chat.input, text);
        self.send_chat_input(false, false);
        if let Scene::Game(game) = &mut self.scene {
            game.chat.input = typing;
        }
    }

    /// One Esc in the game: the battle-mode line, a focused text box, a question's No, the newest window, or the menu.
    fn game_escape(&mut self) {
        let Scene::Game(game) = &mut self.scene else { return };
        game.sync_esc();
        let view = escape::EscView {
            // The always-on line is not this. An open battle-mode line is the first Esc.
            battle_line: game.chat_mode.battle_line_open(),
            whisper_list: game.chat.whisper_list_open(),
            focused: game.focused_text(),
            conversation: game.dialog.as_ref().is_some_and(|dialog| dialog.warp.is_none()),
            question: game.question_open(),
            newest: game.esc_order.first().copied(),
        };
        let act = escape::decide(false, view);
        let Scene::Game(game) = &mut self.scene else { return };
        match act {
            Some(escape::EscAct::CloseWhisperList) => game.chat.close_whisper_list(),
            Some(escape::EscAct::CloseBattleChat) => {
                game.chat.close_battle_line();
                game.chat_mode.close_line();
            }
            Some(escape::EscAct::Cancel(id) | escape::EscAct::Close(id)) => game.close_esc(id),
            Some(escape::EscAct::ShopCancel) => game.close_esc(escape::WindowId::Shop),
            Some(escape::EscAct::AnswerNo) => game.answer_no(&self.messages),
            Some(escape::EscAct::ToggleMenu) => {
                game.escape = if game.escape.is_some() { None } else { Some(escape::Menu { dead: game.player.dead }) };
            }
            None => {}
        }
    }

    /// The next frame goes to ScreenShot/screen<server><NNN>.jpg in the folder the client runs in.
    // ponytail: "next to the client" taken as the folder it runs in (where its data is), not the executable's.
    fn request_screenshot(&mut self) {
        let server = self.account.as_ref().and_then(|account| account.char_servers.get(self.char_server)).map(|server| server.name.clone()).unwrap_or_default();
        let folder = std::path::Path::new(screenshot::FOLDER);
        let _ = std::fs::create_dir_all(folder);
        self.print_screen = Some(screenshot::next_path(folder, &server));
    }

    /// The shortcut this key fires, if it has one. A repeated key-down does not.
    fn fire_shortcut(&mut self, event: &control::KeyPress) -> bool {
        let Scene::Game(game) = &self.scene else { return false };
        let text_focus = game.text_box_focused();
        let fired = shortcuts::action(
            &self.keybinds,
            &shortcuts::KeyEvent {
                key: &event.logical_key,
                physical: Some(event.physical_key),
                alt: self.modifiers.alt_key(),
                shift: self.modifiers.shift_key(),
                ctrl: self.modifiers.control_key(),
                repeat: event.repeat,
                text_focus,
                bar: if game.hotkeys.tab() == 0 { 1 } else { 2 },
            },
        );
        fired.is_some_and(|fired| self.perform_shortcut(fired))
    }

    /// Runs a shortcut. False leaves the key for whatever else is listening (Enter still sends chat).
    fn perform_shortcut(&mut self, fired: shortcuts::Action) -> bool {
        if matches!(fired, shortcuts::Action::ChatMode | shortcuts::Action::Unused) {
            return false;
        }
        if let shortcuts::Action::Macro(line) = fired {
            self.send_macro(usize::from(line));
            return true;
        }
        if fired == shortcuts::Action::Screenshot {
            self.request_screenshot();
            return true;
        }
        if fired == shortcuts::Action::Minimap {
            if let Scene::Game(game) = &mut self.scene {
                game.minimap_view = game.minimap_view.next();
                minimap::save_view(game.minimap_view);
            }
            return true;
        }
        if fired == shortcuts::Action::MinimapWindow {
            let no_picture = self.messages.get(minimap_window::NO_PICTURE).cloned().unwrap_or_default();
            if let (Scene::Game(game), Some(gpu)) = (&mut self.scene, &self.gpu) {
                toggle_big_minimap(game, gpu, no_picture);
            }
            return true;
        }
        let Scene::Game(game) = &mut self.scene else { return true };
        match fired {
            shortcuts::Action::BasicInfo => {
                game.basic_info.frame.toggle();
                game.raise(Panel::BasicInfo);
            }
            shortcuts::Action::Equipment => {
                game.equipment.frame.toggle();
                game.raise(Panel::Equipment);
            }
            shortcuts::Action::Skills => {
                game.skill_window.frame.toggle();
                game.raise(Panel::Skills);
            }
            shortcuts::Action::Quests => game.quest_window.frame.toggle(),
            shortcuts::Action::Friends => {
                if game.party_window.frame.open && game.party_window.friends {
                    game.party_window.frame.open = false;
                } else {
                    game.party_window.friends = true;
                    game.party_window.picked = None;
                    game.party_window.frame.open = true;
                    game.raise(Panel::Party);
                }
            }
            shortcuts::Action::FriendSetup => game.open_friend_setup(),
            shortcuts::Action::Party => {
                game.party_window.frame.toggle();
                game.raise(Panel::Party);
            }
            shortcuts::Action::ChatRoom => {
                game.room_make = if game.room_make.is_some() || game.room.is_some() { None } else { Some(chatroom::Make::default()) };
            }
            shortcuts::Action::Cart if game.cart.cart.is_some() => {
                game.cart.frame.toggle();
                game.raise(Panel::Cart);
            }
            shortcuts::Action::Cart => {}
            shortcuts::Action::WorldMap => {
                let map = map_key(&game.map);
                game.world_map = match (game.world_map.take(), &self.gpu) {
                    (None, Some(gpu)) => worldmap::WorldMap::open(gpu, &self.grf, &self.worlds.0, self.worlds.1, &map),
                    _ => None,
                };
            }
            shortcuts::Action::Inventory => {
                game.inventory.frame.toggle();
                game.raise(Panel::Inventory);
            }
            shortcuts::Action::Guild => {
                game.guild_window.frame.toggle();
                game.raise(Panel::Guild);
                if game.guild_window.frame.open {
                    game.ask_guild_pages();
                }
            }
            shortcuts::Action::Emotions => game.toggle_emotion_list(),
            shortcuts::Action::Sit => game.toggle_sit(),
            shortcuts::Action::Macros => game.toggle_macro_window(),
            shortcuts::Action::CloseWindows => game.close_windows(),
            shortcuts::Action::HotkeyBar => game.hotkeys.frame.toggle(),
            shortcuts::Action::Chat => game.chat.toggle_hidden(),
            shortcuts::Action::ChatSize => game.chat.cycle_size(),
            shortcuts::Action::HpSp => game.show_bars = !game.show_bars,
            shortcuts::Action::Pet if game.pet.is_some() => {
                game.pet_window.frame.toggle();
                game.raise(Panel::Pet);
                if game.pet_window.frame.open {
                    let _ = game.session.pet_command(net::PET_INFO);
                }
            }
            shortcuts::Action::Pet => {}
            shortcuts::Action::Homunculus if game.homunculus.is_some() => {
                game.homunculus_window.frame.toggle();
                game.raise(Panel::Homunculus);
            }
            shortcuts::Action::Homunculus => {}
            shortcuts::Action::Mercenary if game.mercenary.is_some() => {
                game.mercenary_window.frame.toggle();
                game.raise(Panel::Mercenary);
            }
            shortcuts::Action::Mercenary => {}
            shortcuts::Action::HomunculusFollow if game.homunculus.is_some() => {
                game.homunculus_steering.following = !game.homunculus_steering.following;
            }
            shortcuts::Action::HomunculusFollow => {}
            shortcuts::Action::MercenaryFollow if game.mercenary.is_some() => game.steering.following = !game.steering.following,
            shortcuts::Action::MercenaryFollow => {}
            shortcuts::Action::Menu => {
                if !game.basic_info.frame.open {
                    game.basic_info.frame.open = true;
                    game.basic_info.menu_open = true;
                    game.raise(Panel::BasicInfo);
                } else {
                    game.basic_info.menu_open = !game.basic_info.menu_open;
                }
            }
            shortcuts::Action::Clan if game.clan.is_some() => {
                game.clan_window.frame.toggle();
                game.raise(Panel::Clan);
            }
            shortcuts::Action::Clan => {}
            shortcuts::Action::Bank => {
                if game.bank.is_some() {
                    let _ = game.session.close_bank(game.account_id);
                    game.bank = None;
                } else {
                    let _ = game.session.open_bank(game.account_id);
                }
            }
            shortcuts::Action::Status => {
                game.status_window.frame.toggle();
                game.raise(Panel::Status);
            }
            shortcuts::Action::Reputation => {
                game.toggle_reputation(&self.reputation_tables);
                game.sync_esc();
            }
            shortcuts::Action::RotateHotkeys => game.hotkeys.rotate(),
            shortcuts::Action::SwitchHotkeys => game.hotkeys.switch(),
            shortcuts::Action::Costume => {
                if game.equipment.frame.open && game.equipment.showing_costume() {
                    game.equipment.frame.open = false;
                } else {
                    game.equipment.show_costume();
                    game.equipment.frame.open = true;
                    game.raise(Panel::Equipment);
                }
            }
            // Only the showing bar's keys come here (see shortcuts::action).
            shortcuts::Action::Hotbar { slot, .. } => game.press_hotkey(usize::from(slot)),
            // ponytail: no window yet (memorial, commands, ground cursor, tip, party board, hunting, flags). The key is eaten.
            shortcuts::Action::Memorial
            | shortcuts::Action::Commands
            | shortcuts::Action::GroundCursor
            | shortcuts::Action::Tip
            | shortcuts::Action::PartyBoard
            | shortcuts::Action::Hunting
            | shortcuts::Action::Flag(_) => {}
            shortcuts::Action::Macro(_)
            | shortcuts::Action::Screenshot
            | shortcuts::Action::Minimap
            | shortcuts::Action::MinimapWindow
            | shortcuts::Action::ChatMode
            | shortcuts::Action::Unused => {}
        }
        true
    }

    fn key(&mut self, event: &control::KeyPress) {
        // Print Screen on its release (Windows sends no press for it), Scroll Lock, and on Macs Cmd+Shift+S (they have
        // no Print Screen; a ponytail choice): a screenshot.
        let pressed = event.state == ElementState::Pressed && !event.repeat;
        let shot = match &event.logical_key {
            Key::Named(NamedKey::PrintScreen) => event.state == ElementState::Released,
            Key::Named(NamedKey::ScrollLock) => pressed,
            _ => {
                cfg!(target_os = "macos")
                    && pressed
                    && self.modifiers.super_key()
                    && self.modifiers.shift_key()
                    && event.physical_key == PhysicalKey::Code(KeyCode::KeyS)
            }
        };
        if shot {
            self.request_screenshot();
            return;
        }
        if event.state != ElementState::Pressed || event.logical_key == Key::Named(NamedKey::PrintScreen) {
            return;
        }
        let key = &event.logical_key;
        if let Scene::Game(game) = &mut self.scene
            && game.shortcuts_window.as_ref().is_some_and(|window| window.listening.is_some())
        {
            if event.repeat {
                return;
            }
            if let Some(key) = shortcut_key(key) {
                let slot = game.shortcuts_window.as_ref().unwrap().listening;
                if game.shortcuts_window.as_mut().unwrap().bind_key(key, &mut self.shortcuts) {
                    self.shortcuts.save();
                    if let Some(slot) = slot {
                        self.keybinds.set_hotbar(slot, shortcut_to_bind(key));
                    }
                }
                return;
            }
            if *key == Key::Named(NamedKey::Escape) {
                game.shortcuts_window.as_mut().unwrap().listening = None;
                return;
            }
        }
        // Ctrl+Cmd+F, as Macs have it, or Alt+Enter elsewhere: fullscreen (the canvas widened to the screen's shape), or back.
        let fullscreen = if cfg!(target_os = "macos") {
            self.modifiers.super_key() && self.modifiers.control_key() && event.physical_key == PhysicalKey::Code(KeyCode::KeyF)
        } else {
            self.modifiers.alt_key() && *key == Key::Named(NamedKey::Enter)
        };
        if fullscreen && let Some(gpu) = &self.gpu {
            gpu.window.set_fullscreen(gpu.window.fullscreen().is_none().then_some(winit::window::Fullscreen::Borderless(None)));
            return;
        }
        if self.fire_shortcut(event) {
            return;
        }
        match &mut self.scene {
            Scene::Login => {
                self.blink_start = Instant::now();
                match key {
                    Key::Named(NamedKey::Tab) => self.focus ^= 1,
                    Key::Named(NamedKey::Enter) => self.submit_login(),
                    Key::Named(NamedKey::Backspace) => {
                        self.fields[self.focus].pop();
                    }
                    _ => {
                        let field = &mut self.fields[self.focus];
                        for character in event.text.as_deref().unwrap_or_default().chars().filter(char::is_ascii_graphic) {
                            if field.len() < NAME_LENGTH - 1 {
                                field.push(character);
                            }
                        }
                    }
                }
            }
            Scene::Connections { selected } => match key {
                Key::Named(NamedKey::ArrowUp) => *selected = selected.saturating_sub(1),
                Key::Named(NamedKey::ArrowDown) => *selected = (*selected + 1).min(self.connections.len().saturating_sub(1)),
                Key::Named(NamedKey::Enter) => {
                    let picked = *selected;
                    self.choose_connection(picked);
                }
                Key::Named(NamedKey::Escape) => self.quitting = true,
                _ => {}
            },
            Scene::ServerSelect { accepted, selected } => match key {
                Key::Named(NamedKey::ArrowUp) => *selected = selected.saturating_sub(1),
                Key::Named(NamedKey::ArrowDown) => *selected = (*selected + 1).min(accepted.char_servers.len().saturating_sub(1)),
                Key::Named(NamedKey::Enter) => self.enter_char_server(),
                Key::Named(NamedKey::Escape) => self.scene = Scene::Login,
                _ => {}
            },
            Scene::CharSelect(select) if select.pin_pad.is_some() => {
                let pad = select.pin_pad.as_mut().unwrap();
                // ponytail: the official pad is mouse-only (message 1906); also accept digit keys so scenarios can `type` a PIN.
                let action = match key {
                    Key::Named(NamedKey::Backspace) => {
                        pad.backspace();
                        pin::PinAction::None
                    }
                    Key::Named(NamedKey::Enter) => pad.press(pin::PinButton::Ok),
                    Key::Named(NamedKey::Escape) => pad.press(pin::PinButton::Cancel),
                    _ => event
                        .text
                        .as_deref()
                        .and_then(|text| text.chars().next())
                        .filter(char::is_ascii_digit)
                        .map_or(pin::PinAction::None, |digit| pad.push_digit(digit)),
                };
                self.apply_pin_action(action);
            }
            Scene::CharSelect(select) => match key {
                Key::Named(NamedKey::ArrowLeft) => select.step(-1),
                Key::Named(NamedKey::ArrowRight) => select.step(1),
                Key::Named(NamedKey::ArrowUp) => select.step(-char_select::Select::ROW),
                Key::Named(NamedKey::ArrowDown) => select.step(char_select::Select::ROW),
                Key::Named(NamedKey::Enter) => {
                    if let Some(action) = select.enter() {
                        self.select_action(action);
                    }
                }
                // Escape closes the card's menu or stops picking where to move first.
                Key::Named(NamedKey::Escape) if select.menu.is_some() || select.moving.is_some() => {
                    select.menu = None;
                    select.moving = None;
                }
                Key::Named(NamedKey::Escape) => self.select_action(char_select::Action::Close),
                _ => {}
            },
            Scene::CharMake(make, _) => match key {
                Key::Named(NamedKey::Enter) => {
                    if let Some(action) = make.click(char_make::Hit::Make, &self.grf, &self.look_tables) {
                        self.make_action(action);
                    }
                }
                Key::Named(NamedKey::Escape) => self.make_action(char_make::Action::Back),
                Key::Named(NamedKey::Backspace) => {
                    make.name.pop();
                }
                _ => make.type_text(event.text.as_deref().unwrap_or_default()),
            },
            // Esc, one window per press, and never on a key that is auto-repeating.
            Scene::Game(_) if *key == Key::Named(NamedKey::Escape) => {
                if !event.repeat {
                    self.game_escape();
                }
            }
            Scene::Game(game) if game.dialog.is_some() => {
                let dialog = game.dialog.as_mut().unwrap();
                match (key, &mut dialog.input, &mut dialog.menu) {
                    (Key::Named(NamedKey::Enter), Some(_), _) => game.submit_input(),
                    (Key::Named(NamedKey::Backspace), Some((_, value)), _) => {
                        value.pop();
                    }
                    (_, Some((text, value)), _) => value.extend(
                        event
                            .text
                            .as_deref()
                            .unwrap_or_default()
                            .chars()
                            .filter(|character| if *text { !character.is_control() } else { character.is_ascii_digit() || *character == '-' }),
                    ),
                    (Key::Named(NamedKey::ArrowUp), _, Some((_, selected))) => *selected = selected.saturating_sub(1),
                    (Key::Named(NamedKey::ArrowDown), _, Some((items, selected))) => *selected = (*selected + 1).min(items.len().saturating_sub(1)),
                    (Key::Named(NamedKey::Enter), _, Some(_)) => game.choose(false),
                    (Key::Named(NamedKey::Enter), None, None) => game.press_dialog_button(),
                    _ => {}
                }
            }
            // The question holds the player busy until it's answered.
            Scene::Game(game) if game.bind_asked.is_some() && matches!(key, Key::Named(NamedKey::Enter | NamedKey::Escape)) => {
                game.answer_bind(*key == Key::Named(NamedKey::Enter));
            }
            Scene::Game(game) if game.expansion_asked.is_some() && matches!(key, Key::Named(NamedKey::Enter | NamedKey::Escape)) => {
                game.expansion_asked = None;
                let _ = game.session.answer_expansion(*key == Key::Named(NamedKey::Enter));
            }
            Scene::Game(game) if game.apply_asked && matches!(key, Key::Named(NamedKey::Enter | NamedKey::Escape)) => {
                game.answer_apply(*key == Key::Named(NamedKey::Enter));
            }
            // Tab over the skill window: its next tab, round to the first.
            // ponytail: with the pointer over the window; the game's own focus rule isn't known.
            Scene::Game(game) if *key == Key::Named(NamedKey::Tab) && game.window_at(self.cursor) == Some(Panel::Skills) => {
                let jobs = skills::tabs(&self.skill_trees, game.me.job, &game.skills);
                let sheet = skills::Sheet {
                    trees: &self.skill_trees,
                    jobs: &jobs,
                    skills: &game.skills.clone(),
                    info: &game.skill_info.clone(),
                    points: game.skill_points(),
                };
                game.skill_window.next_tab(&sheet);
            }
            Scene::Game(game) if game.leader_asked.is_some() && matches!(key, Key::Named(NamedKey::Enter | NamedKey::Escape)) => {
                let account = game.leader_asked.take().unwrap();
                if *key == Key::Named(NamedKey::Enter) {
                    let _ = game.session.change_party_leader(account);
                }
            }
            Scene::Game(game) if game.party_setup.is_some() && *key == Key::Named(NamedKey::Escape) => game.party_setup = None,
            Scene::Game(game) if game.ranking.is_some() && *key == Key::Named(NamedKey::Escape) => game.ranking = None,
            Scene::Game(game) if game.dressroom.is_some() && *key == Key::Named(NamedKey::Escape) => {
                game.dressroom = None;
                game.dress_figure = None;
            }
            Scene::Game(game) if game.dismiss_asked && matches!(key, Key::Named(NamedKey::Enter | NamedKey::Escape)) => {
                game.answer_dismiss(*key == Key::Named(NamedKey::Enter), &self.messages);
            }
            Scene::Game(game) if game.homunculus_rename.is_some() && matches!(key, Key::Named(NamedKey::Enter | NamedKey::Escape)) => {
                game.answer_homunculus_name(*key == Key::Named(NamedKey::Enter));
            }
            Scene::Game(game) if game.pet_asked.is_some() && matches!(key, Key::Named(NamedKey::Enter | NamedKey::Escape)) => {
                game.answer_pet(*key == Key::Named(NamedKey::Enter));
            }
            Scene::Game(game) if game.cashshop_buy_asked && matches!(key, Key::Named(NamedKey::Enter | NamedKey::Escape)) => {
                game.cashshop_buy_asked = false;
                if let Some((free, items)) = game.cashshop.as_ref().and_then(|shop| shop.buy_request(*key == Key::Named(NamedKey::Enter))) {
                    let _ = game.session.buy_cashshop(free, &items);
                }
            }
            Scene::Game(game) if game.amount.is_some() => match key {
                Key::Named(NamedKey::Enter) => game.answer_amount(true, &self.messages),
                Key::Named(NamedKey::Escape) => game.answer_amount(false, &self.messages),
                Key::Named(NamedKey::Backspace) => {
                    game.amount.as_mut().unwrap().typed.pop();
                }
                _ => game.amount.as_mut().unwrap().type_text(event.text.as_deref().unwrap_or_default()),
            },
            Scene::Game(game) if game.vend_setup.as_ref().is_some_and(|setup| setup.typing) && game.amount.is_none() => {
                let setup = game.vend_setup.as_mut().unwrap();
                match key {
                    Key::Named(NamedKey::Enter | NamedKey::Escape) => setup.typing = false,
                    Key::Named(NamedKey::Backspace) => {
                        setup.name.pop();
                    }
                    _ => setup.type_text(event.text.as_deref().unwrap_or_default()),
                }
            }
            Scene::Game(game) if game.buying_setup.as_ref().is_some_and(|setup| setup.focus.is_some()) && game.amount.is_none() => {
                let setup = game.buying_setup.as_mut().unwrap();
                match key {
                    Key::Named(NamedKey::Enter | NamedKey::Escape) => setup.focus = None,
                    Key::Named(NamedKey::Tab) => {
                        setup.focus = match setup.focus {
                            Some(buying::Focus::Name) => Some(buying::Focus::Limit),
                            _ => Some(buying::Focus::Name),
                        };
                    }
                    Key::Named(NamedKey::Backspace) => setup.backspace(),
                    _ => setup.type_text(event.text.as_deref().unwrap_or_default()),
                }
            }
            Scene::Game(game) if game.mail.typing() && game.amount.is_none() => match key {
                Key::Named(NamedKey::Escape) => game.mail.stop_typing(),
                Key::Named(NamedKey::Backspace) => game.mail.backspace(),
                Key::Named(NamedKey::Enter) => game.mail.enter(false),
                Key::Named(NamedKey::Tab) => game.mail.enter(true),
                _ => game.mail.type_text(event.text.as_deref().unwrap_or_default()),
            },
            // A macro line being edited takes the typing; Enter or Esc ends it.
            Scene::Game(game) if game.macro_window.as_ref().is_some_and(|window| window.editing.is_some()) => {
                let window = game.macro_window.as_mut().unwrap();
                match key {
                    Key::Named(NamedKey::Escape | NamedKey::Enter) => {
                        window.editing = None;
                        macros::save(macros::path().as_deref(), &game.macro_lines);
                    }
                    Key::Named(NamedKey::Backspace) => window.backspace(&mut game.macro_lines),
                    _ => window.type_text(&mut game.macro_lines, event.text.as_deref().unwrap_or_default()),
                }
            }
            Scene::Game(game) if game.room_make.is_some() => {
                let make = game.room_make.as_mut().unwrap();
                match key {
                    Key::Named(NamedKey::Escape) => game.room_make = None,
                    Key::Named(NamedKey::Enter) => game.make_room(),
                    Key::Named(NamedKey::Tab) => make.on_password = !make.on_password && !make.public,
                    Key::Named(NamedKey::Backspace) => make.backspace(),
                    _ => make.type_text(event.text.as_deref().unwrap_or_default()),
                }
            }
            // In a chat room, typing goes to its line; Enter says it there.
            Scene::Game(game) if game.room.is_some() && game.amount.is_none() => {
                let room = game.room.as_mut().unwrap();
                match key {
                    Key::Named(NamedKey::Enter) => {
                        let text = std::mem::take(&mut room.input);
                        if !text.trim().is_empty() {
                            let _ = game.session.chat(&game.me.name, text.trim());
                        }
                    }
                    Key::Named(NamedKey::Backspace) => {
                        room.input.pop();
                    }
                    Key::Named(NamedKey::Escape) => room.input.clear(),
                    _ if !self.modifiers.alt_key() && !self.modifiers.super_key() && !self.modifiers.control_key() => {
                        room.input.extend(event.text.as_deref().unwrap_or_default().chars().filter(|character| !character.is_control()));
                    }
                    _ => {}
                }
            }
            Scene::Game(game) if game.cashshop.as_ref().is_some_and(|shop| shop.typing) => {
                let shop = game.cashshop.as_mut().unwrap();
                match key {
                    Key::Named(NamedKey::Enter | NamedKey::Escape) => shop.typing = false,
                    Key::Named(NamedKey::Backspace) => shop.backspace(),
                    _ => shop.type_text(event.text.as_deref().unwrap_or_default()),
                }
            }
            Scene::Game(game) if game.bank.as_ref().is_some_and(|bank| bank.typing) => {
                let bank = game.bank.as_mut().unwrap();
                match key {
                    Key::Named(NamedKey::Enter | NamedKey::Escape) => bank.typing = false,
                    Key::Named(NamedKey::Backspace) => {
                        bank.typed.pop();
                    }
                    _ => bank.type_text(event.text.as_deref().unwrap_or_default()),
                }
            }
            Scene::Game(game) if game.trade.as_ref().is_some_and(|trade| trade.typing) => {
                let trade = game.trade.as_mut().unwrap();
                match key {
                    Key::Named(NamedKey::Enter | NamedKey::Escape) => trade.typing = false,
                    Key::Named(NamedKey::Backspace) => {
                        trade.typed.pop();
                    }
                    _ => trade.type_text(event.text.as_deref().unwrap_or_default()),
                }
            }
            // The navigation window's search field: each change searches again.
            Scene::Game(game) if game.navi_window.as_ref().is_some_and(|window| window.typing) => {
                let window = game.navi_window.as_mut().unwrap();
                match key {
                    Key::Named(NamedKey::Enter | NamedKey::Escape) => window.typing = false,
                    Key::Named(NamedKey::Backspace) => {
                        window.text.pop();
                    }
                    _ => window.text.push_str(event.text.as_deref().unwrap_or_default()),
                }
                window.search(&self.navi_tables);
            }
            Scene::Game(game) if game.storage.typing => match key {
                Key::Named(NamedKey::Enter) => game.storage.typing = false,
                Key::Named(NamedKey::Escape) => {
                    game.storage.search.clear();
                    game.storage.typing = false;
                }
                Key::Named(NamedKey::Backspace) => {
                    game.storage.search.pop();
                }
                _ => game.storage.type_text(event.text.as_deref().unwrap_or_default()),
            },
            Scene::Game(game) if game.catalog.is_some() && matches!(key, Key::Named(NamedKey::Escape)) => game.close_catalog(),
            Scene::Game(game) if game.catalog.as_ref().is_some_and(|catalog| catalog.focus.is_some()) => {
                let catalog = game.catalog.as_mut().unwrap();
                match key {
                    Key::Named(NamedKey::Enter) => game.click_catalog(catalog::Hit::Search, &self.items, &self.messages),
                    Key::Named(NamedKey::Tab) => {
                        catalog.focus = Some(match catalog.focus {
                            Some(catalog::Field::Name) => catalog::Field::Min,
                            Some(catalog::Field::Min) => catalog::Field::Max,
                            _ => catalog::Field::Name,
                        })
                    }
                    Key::Named(NamedKey::Backspace) => catalog.backspace(),
                    _ => catalog.type_text(event.text.as_deref().unwrap_or_default()),
                }
            }
            // Naming a memo on the big minimap. Esc cancels it (game_escape) before this.
            Scene::Game(game) if game.minimap_window.naming() => match key {
                Key::Named(NamedKey::Enter) => {
                    game.minimap_window.store();
                }
                Key::Named(NamedKey::Backspace) => game.minimap_window.backspace(),
                _ => game.minimap_window.type_text(event.text.as_deref().unwrap_or_default()),
            },
            Scene::Game(game) if game.deal_choice.is_some() && matches!(key, Key::Named(NamedKey::Escape)) => game.deal_choice = None,
            // Enter is the deal-type box's default button: Buy.
            Scene::Game(game) if game.deal_choice.is_some() && matches!(key, Key::Named(NamedKey::Enter)) => {
                let npc = game.deal_choice.take().unwrap();
                let _ = game.session.choose_deal(npc, false);
            }
            // Earlier arms are windows that want the key. Esc is game_escape, which closes an open battle-mode
            // line first. Here the chat line has the key. Off: the line is always open and letters type. On: Enter
            // opens it, Enter on an empty line closes it, and a closed line sends letters and digits to shortcuts.
            Scene::Game(game) if game.away_prompt.is_some() => match key {
                Key::Named(NamedKey::Enter) => {
                    let text = game.away_prompt.take().map(|prompt| prompt.typed).unwrap_or_default();
                    game.finish_away(&text);
                }
                Key::Named(NamedKey::Backspace) => {
                    if let Some(prompt) = &mut game.away_prompt {
                        prompt.typed.pop();
                    }
                }
                _ if !self.modifiers.alt_key() && !self.modifiers.super_key() && !self.modifiers.control_key() => {
                    if let Some(prompt) = &mut game.away_prompt {
                        prompt.type_text(event.text.as_deref().unwrap_or_default());
                    }
                }
                _ => {}
            },
            Scene::Game(game) if game.talks.typing() => match key {
                Key::Named(NamedKey::Enter) => game.send_focused_talk(),
                Key::Named(NamedKey::Backspace) => game.talks.backspace(),
                _ if !self.modifiers.alt_key() && !self.modifiers.super_key() && !self.modifiers.control_key() => {
                    game.talks.type_text(event.text.as_deref().unwrap_or_default());
                }
                _ => {}
            },
            Scene::Game(game) => {
                let kind = battle_key(key);
                let take = game.chat_mode.take_key(kind, game.chat.input.trim().is_empty(), game.chat.naming);
                match take {
                    battle_mode::Take::Opened => game.chat.naming = false,
                    // A blank line is not a message. Drop the spaces so the next Enter opens an empty line.
                    battle_mode::Take::Closed => game.chat.close_battle_line(),
                    battle_mode::Take::Shortcut | battle_mode::Take::Type => {
                        let typing = take == battle_mode::Take::Type;
                        // shortcuts::action already ran. A letter it fired never reaches here. An unbound letter
                        // while the line is closed is not typed either.
                        if typing || !matches!(kind, battle_mode::KeyKind::Char(_)) {
                            match key {
                                Key::Named(NamedKey::Tab) if typing => game.chat.naming = !game.chat.naming,
                                Key::Named(NamedKey::Insert) => game.toggle_sit(),
                                Key::Named(NamedKey::Enter) if typing && game.chat.naming => game.chat.naming = false,
                                Key::Named(NamedKey::Enter) if typing => self.send_chat_input(self.modifiers.control_key(), self.modifiers.alt_key()),
                                Key::Named(NamedKey::Backspace) if typing => {
                                    game.chat.typing().pop();
                                }
                                _ if typing && !self.modifiers.alt_key() && !self.modifiers.super_key() && !self.modifiers.control_key() => {
                                    let typed: String = event.text.as_deref().unwrap_or_default().chars().filter(|character| !character.is_control()).collect();
                                    game.chat.type_text(&typed);
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
            Scene::Message(..) => {
                if matches!(key, Key::Named(NamedKey::Enter | NamedKey::Escape)) {
                    self.close_message();
                }
            }
            Scene::Prompt { input, then, .. } if !matches!(key, Key::Named(NamedKey::Enter | NamedKey::Escape)) => {
                if matches!(key, Key::Named(NamedKey::Backspace)) {
                    input.pop();
                }
                let text = event.text.as_deref().unwrap_or_default();
                // A new name takes any letters; the birthdate only digits.
                if let Then::ConfirmName(_) = then {
                    let room = NAME_MOST - input.chars().count().min(NAME_MOST);
                    input.extend(text.chars().filter(|character| !character.is_control()).take(room));
                } else {
                    input.extend(text.chars().filter(char::is_ascii_digit).take(PROMPT_LENGTH - input.len().min(PROMPT_LENGTH)));
                }
            }
            Scene::Confirm(..) | Scene::Prompt { .. } => match key {
                Key::Named(NamedKey::Enter) => self.answer(),
                Key::Named(NamedKey::Escape) => self.dismiss(),
                _ => {}
            },
            Scene::Waiting(_) => {}
        }
    }

    fn net_event(&mut self, event: NetEvent) {
        match event {
            NetEvent::Login(Ok(accepted)) => {
                // Dev runs (RUSTNAROK_CLIENT_LOGIN) leave the saved ID alone.
                if std::env::var_os("RUSTNAROK_CLIENT_LOGIN").is_none() {
                    remember_id(self.save_id.then_some(self.fields[0].as_str()));
                }
                self.scene = Scene::ServerSelect { accepted, selected: 0 };
                if std::env::var_os("RUSTNAROK_CLIENT_AUTOSTART").is_some() {
                    self.enter_char_server();
                }
            }
            NetEvent::Login(Err(error)) => {
                let text = self.messages.get(login_error_message(&error)).cloned().unwrap_or_default();
                self.message(text, Scene::Login);
            }
            // Events from a session already closed by going back to login.
            NetEvent::Char(CharEvent::Disconnected(_)) if self.char_session.is_none() => {}
            NetEvent::Char(_) if self.char_session.is_none() => {}
            NetEvent::Char(CharEvent::Characters { slots, characters }) => {
                let figures = characters.iter().map(|character| SpriteSet::player(&self.grf, &character_look(character), &self.look_tables)).collect();
                let mut select = char_select::Select::new(characters, figures, usize::from(slots));
                // A list sent again (after a deletion) keeps the cursor and PIN verification where they were.
                if let Scene::CharSelect(previous) = self.scene.base() {
                    select.selected = previous.selected;
                    select.pin_verified = previous.pin_verified;
                } else if let Some(slot) = self.played_slot.take() {
                    // Back from the game: the character just played.
                    select.selected = slot;
                }
                let autostart = std::env::var("RUSTNAROK_CLIENT_AUTOSTART").ok().and_then(|slot| slot.parse::<usize>().ok());
                // Only the first list: later ones (after a move or a rename) keep the player's selection.
                if let Some(slot) = autostart
                    && !self.scripted
                {
                    select.selected = slot;
                }
                let verified = select.pin_verified;
                match &mut self.scene {
                    // After a rename its message stays up over the new list.
                    Scene::Message(_, back) if matches!(**back, Scene::CharSelect(_)) => **back = Scene::CharSelect(Box::new(select)),
                    scene => *scene = Scene::CharSelect(Box::new(select)),
                }
                // Autostart and MAKE/DELETE wait for PIN state 0 (`finish_char_select_entry`).
                if verified {
                    self.finish_char_select_entry();
                }
            }
            NetEvent::Char(CharEvent::DeleteReserved { id, result, seconds }) => {
                self.settle();
                match (result, &mut self.scene) {
                    (1, Scene::CharSelect(select)) => select.set_deletion(id, Some(seconds)),
                    // Already reserved, a database error, in a guild, in a party.
                    (code, _) => {
                        let text = self.message_text(match code {
                            3 => 1817,
                            4 => 1818,
                            5 => 1819,
                            _ => 1816,
                        });
                        let back = std::mem::replace(&mut self.scene, Scene::Login);
                        self.message(text, back);
                    }
                }
            }
            NetEvent::Char(CharEvent::DeleteCanceled { id, result }) => {
                self.settle();
                match (result, &mut self.scene) {
                    (1, Scene::CharSelect(select)) => select.set_deletion(id, None),
                    _ => {
                        let back = std::mem::replace(&mut self.scene, Scene::Login);
                        self.message(self.message_text(1817), back);
                    }
                }
            }
            NetEvent::Char(CharEvent::Moved { result, .. }) => {
                self.settle();
                let Scene::CharSelect(select) = &mut self.scene else { return };
                let to = select.moved_to.take();
                match (result, to) {
                    // The list that follows keeps the selection, so it follows the character there.
                    (0, Some(to)) => select.selected = to,
                    (0, None) => {}
                    _ => {
                        let back = std::mem::replace(&mut self.scene, Scene::Login);
                        self.message(self.message_text(MOVE_FAILED), back);
                    }
                }
            }
            NetEvent::Char(CharEvent::Renamed(result)) => {
                self.settle();
                // Renamed, renamed before, bad account, failed, name taken, in a guild, in a party, too long, symbols,
                // prohibited, unknown.
                let text = self.message_text(match result {
                    0 => 1214,
                    1 => 1218,
                    2 => 1219,
                    3 => 1215,
                    4 => 1220,
                    5 => 2022,
                    6 => 2023,
                    7 => 2027,
                    8 => 2028,
                    9 => 2029,
                    _ => 2024,
                });
                let back = std::mem::replace(&mut self.scene, Scene::Login);
                self.message(text, back);
            }
            // A deletion the server made comes after the new character list, which already replaced the wait.
            NetEvent::Char(CharEvent::Deleted { result: 1, .. }) => {}
            NetEvent::Char(CharEvent::Deleted { result, .. }) => {
                self.settle();
                let text = self.message_text(match result {
                    2 => 1820,
                    3 => 1817,
                    4 => 1821,
                    5 => 1822,
                    _ => 1816,
                });
                let back = std::mem::replace(&mut self.scene, Scene::Login);
                self.message(text, back);
            }
            NetEvent::Char(CharEvent::Made(character)) => {
                self.settle();
                if let Scene::CharMake(_, mut select) = std::mem::replace(&mut self.scene, Scene::Login) {
                    let figure = SpriteSet::player(&self.grf, &character_look(&character), &self.look_tables);
                    select.add(character, figure);
                    self.scene = Scene::CharSelect(select);
                }
            }
            NetEvent::Char(CharEvent::MakeRefused(code)) => {
                self.settle();
                // Name taken, underage, symbols in the name, slot not available, refused.
                let text = self.message_text(match code {
                    0 => 10,
                    1 => 298,
                    2 => 1272,
                    3 => 1355,
                    _ => 11,
                });
                let back = std::mem::replace(&mut self.scene, Scene::Login);
                self.message(text, back);
            }
            NetEvent::Char(CharEvent::ZoneServer { map, ip, port, character_id }) => {
                self.char_session = None;
                let (Some(account), Some(_)) = (&self.account, &self.playing) else { return };
                let ticket = MapTicket { address: (ip, port), account_id: account.account_id, character_id, login_id1: account.login_id1, sex: account.sex };
                let proxy = self.proxy.clone();
                match MapSession::connect(&ticket, self.client_tick(), move |event| {
                    let _ = proxy.send_event(NetEvent::Map(event));
                }) {
                    Ok(session) => {
                        self.entering = Some((session, map));
                        self.early_events.clear();
                    }
                    Err(_) => self.message("맵 서버에 접속할 수 없습니다.", Scene::Login),
                }
            }
            NetEvent::Char(CharEvent::Pin { seed, state }) => {
                self.settle();
                let Scene::CharSelect(select) = self.scene.base_mut() else { return };
                let outcome = pin::PinPad::apply_server(&mut select.pin_pad, seed, state);
                match outcome {
                    pin::PinServerOutcome::Passed { message } => {
                        select.pin_verified = true;
                        self.finish_char_select_entry();
                        if let Some(id) = message
                            && matches!(self.scene, Scene::CharSelect(_))
                        {
                            let text = self.message_text(id);
                            let back = std::mem::replace(&mut self.scene, Scene::Login);
                            self.message(text, back);
                        }
                    }
                    pin::PinServerOutcome::Prompt => {
                        select.pin_verified = false;
                        if state == 1 && !select.auto_pin_tried {
                            select.auto_pin_tried = true;
                            if let Ok(auto_pin) = std::env::var("RUSTNAROK_CLIENT_PIN")
                                && auto_pin.len() == 4
                                && auto_pin.bytes().all(|byte| byte.is_ascii_digit())
                                && let Some(pad) = &mut select.pin_pad
                            {
                                let mut action = pin::PinAction::None;
                                for digit in auto_pin.chars() {
                                    action = pad.push_digit(digit);
                                }
                                self.apply_pin_action(action);
                            }
                        }
                    }
                    pin::PinServerOutcome::Message { id, tries_left } => {
                        select.pin_verified = false;
                        let text = messages::fill(&self.message_text(id), &[&tries_left]);
                        let back = std::mem::replace(&mut self.scene, Scene::Login);
                        self.message(text, back);
                    }
                    pin::PinServerOutcome::LockedOut(id) => {
                        self.char_session = None;
                        let text = self.message_text(id);
                        self.message(text, Scene::Login);
                    }
                }
            }
            NetEvent::Char(CharEvent::Refused(code)) => {
                self.char_session = None;
                self.message(format!("접속이 거부되었습니다. ({code})"), Scene::Login);
            }
            NetEvent::Char(CharEvent::Disconnected(_)) => {
                self.char_session = None;
                self.message(self.messages.get(DISCONNECTED_MESSAGE).cloned().unwrap_or_default(), Scene::Login);
            }
            NetEvent::Map(event) => self.map_event(event),
            NetEvent::Control(_) => unreachable!("user_event runs commands"),
        }
    }

    /// Loads the map the map server put us on and starts playing.
    fn enter_map(&mut self, position: [u16; 2], direction: u8) {
        let (Some((mut session, map)), Some(me), Some(account), Some(gpu)) = (self.entering.take(), self.playing.clone(), &self.account, &self.gpu) else {
            return;
        };
        match World::load(gpu, &self.grf, &map) {
            Ok(world) => {
                let _ = session.loaded();
                // The player's own title comes with their own name tag.
                let _ = session.ask_name(account.account_id);
                let look = character_look(&me);
                let sprites = SpriteSet::player(&self.grf, &look, &self.look_tables);
                let mut player = Actor::new(me.name.clone(), sprites, position.map(i32::from), direction, 150);
                (player.kind, player.job, player.look) = (PLAYER, me.job, Some(look));
                player.sync_companions(me.option, &self.grf);
                let indoor = self.indoor.contains(&map_key(&map));
                let camera_goal = CameraGoal::arrive(indoor, self.viewpoints.get(&map_key(&map)).copied(), self.saved_view.of(indoor), zoom_out_saved());
                let camera = camera_goal.camera(world.cell_position(position.map(f32::from)));
                let now = Instant::now();
                let minimap = minimap::load_map(&self.grf, &map);
                // Until the server sends updates, the character list's values.
                let status = HashMap::from([
                    (net::STATUS_HP, me.hp),
                    (net::STATUS_MAX_HP, me.max_hp),
                    (net::STATUS_SP, me.sp),
                    (net::STATUS_MAX_SP, me.max_sp),
                    (net::STATUS_BASE_LEVEL, i64::from(me.base_level)),
                    (net::STATUS_JOB_LEVEL, i64::from(me.job_level)),
                    (net::STATUS_BASE_EXP, me.exp),
                    (net::STATUS_JOB_EXP, me.job_exp),
                    (net::STATUS_ZENY, i64::from(me.zeny)),
                ]);
                let minimap_map = map_key(&map);
                self.scene = Scene::Game(Box::new(Game {
                    saved_places: String::new(),
                    session,
                    world,
                    map,
                    me,
                    account_id: account.account_id,
                    player,
                    units: HashMap::new(),
                    camera,
                    camera_goal,
                    minimap,
                    minimap_crop: None,
                    minimap_view: minimap::saved_view(),
                    minimap_zoom: settings_path("minimap_zoom")
                        .and_then(|path| std::fs::read_to_string(path).ok())
                        .and_then(|text| text.trim().parse::<f32>().ok())
                        .map_or(1.0, |zoom| zoom.clamp(1.0, minimap::MOST_ZOOM)),
                    navigation_route: false,
                    minimap_window: minimap_window::Window::new(gpu, &minimap_map),
                    guild_places: HashMap::new(),
                    party_colors: Vec::new(),
                    show_bars: true,
                    last_frame: now,
                    chat: chat::Chat::default(),
                    entered: now,
                    last_keepalive: now,
                    shadow: SpriteSet::at(&self.grf, "data\\sprite\\shadow"),
                    emotion_sprite: SpriteSet::at(&self.grf, "data\\sprite\\이팩트\\emotion"),
                    emotions: HashMap::new(),
                    mvp_sprite: SpriteSet::at(&self.grf, "data\\sprite\\이팩트\\mvp"),
                    mvps: HashMap::new(),
                    mark_sprites: ["status-stun", "status-sleep", "status-curse", "얼음땡"]
                        .into_iter()
                        .filter_map(|name| Some((name, SpriteSet::at(&self.grf, &format!("data\\sprite\\이팩트\\{name}"))?)))
                        .collect(),
                    spirits: HashMap::new(),
                    sphere: gpu.try_load_texture(&self.grf, "data\\texture\\effect\\기구.bmp", true).ok(),
                    shield: render::load_image(&self.grf, "data\\texture\\effect\\shield.bmp").ok().map(|image| gpu.texture(&render::glow(image), true)),
                    charm: render::load_image(&self.grf, "data\\texture\\effect\\charm.bmp").ok().map(|image| gpu.texture(&render::glow(image), true)),
                    coin_sprite: SpriteSet::at(&self.grf, "data\\sprite\\이팩트\\동전"),
                    digits: Digits::load(gpu, &self.grf),
                    message_sprite: SpriteSet::at(&self.grf, "data\\sprite\\이팩트\\msg"),
                    dialog: None,
                    hit_boxes: Vec::new(),
                    ground: HashMap::new(),
                    item_sprites: HashMap::new(),
                    inventory: {
                        let mut inventory = Inventory::new(std::env::var_os("RUSTNAROK_CLIENT_INVENTORY").is_some());
                        inventory.apply_boxes(inventory::Boxes::load());
                        inventory
                    },
                    equipment: {
                        let mut equipment = Equipment::default();
                        equipment.frame.open = std::env::var_os("RUSTNAROK_CLIENT_EQUIPMENT").is_some();
                        equipment
                    },
                    basic_info: BasicInfo::load(screen()),
                    hotkeys: hotkeys::Hotkeys::default(),
                    dragging: None,
                    drag_cancelled: false,
                    skills: Vec::new(),
                    skill_window: {
                        let mut window = skills::SkillWindow::default();
                        window.frame.open = std::env::var_os("RUSTNAROK_CLIENT_SKILLS").is_some();
                        window
                    },
                    targeting: None,
                    capturing: false,
                    slot_sprite: SpriteSet::at(&self.grf, "data\\sprite\\slotmachine"),
                    slot: None,
                    casts: HashMap::new(),
                    cast_squares: HashMap::new(),
                    combos: HashMap::new(),
                    locks: Vec::new(),
                    speech: HashMap::new(),
                    status_icons: Vec::new(),
                    status_tip: None,
                    cooldowns: skills::Cooldowns::default(),
                    talkie_text: None,
                    talkie_spots: HashMap::new(),
                    cooldown_line: self.messages.get(COOLDOWN_REFUSED).cloned().unwrap_or_default(),
                    effects: effects::Effects::default(),
                    deal_choice: None,
                    shop: None,
                    storage: storage::Storage::default(),
                    cutin: None,
                    marks: Vec::new(),
                    amount: None,
                    escape: None,
                    audio_window: None,
                    graphics_window: None,
                    shortcuts_window: None,
                    world_map: None,
                    quests: Vec::new(),
                    quest_window: quest::QuestWindow {
                        tracker: settings_path("quest_tracker").and_then(|path| std::fs::read_to_string(path).ok()).is_none_or(|text| text.trim() != "0"),
                        ..quest::QuestWindow::load()
                    },
                    achievement_totals: net::AchievementTotals::default(),
                    achievements: HashMap::new(),
                    achievement_window: achievement::AchievementWindow::default(),
                    item_window: None,
                    card_view: None,
                    item_lines: 0,
                    whispered: VecDeque::new(),
                    whisper_asked: VecDeque::new(),
                    pick: None,
                    card_asked: None,
                    item_skill: None,
                    trade: None,
                    refine: None,
                    grade: None,
                    synthesis: None,
                    upgrade: None,
                    reform: None,
                    enchant: None,
                    stylist: None,
                    materials: None,
                    roulette: None,
                    monster_window: None,
                    boss: None,
                    viewing: None,
                    bank: None,
                    cashshop: None,
                    cashshop_lists: vec![Vec::new(); 9],
                    cashshop_list_requested: false,
                    cashshop_buy_asked: false,
                    trade_asked: None,
                    trade_with: String::new(),
                    menu: None,
                    weather: weather::Weather::default(),
                    right_press: None,
                    party: None,
                    status_window: {
                        let mut window = status::StatusWindow::default();
                        window.frame.open = std::env::var_os("RUSTNAROK_CLIENT_STATUS").is_some();
                        window
                    },
                    status_height: 0.0,
                    party_window: {
                        let mut window = party::PartyWindow::default();
                        let order = std::env::var("RUSTNAROK_CLIENT_PARTY").unwrap_or_default();
                        window.friends = order.split(',').any(|part| part == "friends");
                        window.frame.open = window.friends || order.split(',').any(|part| part == "window");
                        window
                    },
                    member_windows: party::MemberWindows::default(),
                    party_invite: None,
                    partner: None,
                    adopt_asked: None,
                    friends: Vec::new(),
                    friend_asked: None,
                    expansion_asked: None,
                    switch_asked: None,
                    last_wear: None,
                    bind_asked: None,
                    broken_before: None,
                    pet_id: None,
                    pet: None,
                    pet_window: pet::PetWindow::default(),
                    pet_asked: None,
                    mercenary: None,
                    refusing_party: None,
                    announcements: announce::Bar::default(),
                    map_property: None,
                    dress_lists: lua::dress_lists(&self.grf)
                        .map(|[hair, hair_color, clothes_color]| dressroom::Lists { hair, hair_color, clothes_color })
                        .unwrap_or_default(),
                    dressroom: None,
                    ranking: None,
                    reputation: HashMap::new(),
                    reputation_window: None,
                    navigation: None,
                    navi_window: None,
                    catalog: None,
                    skill_description: None,
                    pickup_popup: None,
                    new_items: false,
                    level_ups: level_up::LevelUps::default(),
                    overweight: basic_info::OVERWEIGHT,
                    quest_icon: false,
                    banners: quest_notice::Banners::default(),
                    char_names: HashMap::new(),
                    names_asked: HashSet::new(),
                    dress_figure: None,
                    head_emblems: Vec::new(),
                    bars_drawn: Vec::new(),
                    no_shift: settings_path("noshift").and_then(|path| std::fs::read_to_string(path).ok()).is_some_and(|text| text.trim() == "1"),
                    skill_sent: None,
                    right_moved: false,
                    companion_mark: None,
                    camera_snap: settings_path("camerasnap").and_then(|path| std::fs::read_to_string(path).ok()).is_some_and(|text| text.trim() == "1"),
                    quake_flip: false,
                    no_ctrl: settings_path("noctrl").and_then(|path| std::fs::read_to_string(path).ok()).is_none_or(|text| text.trim() != "0"),
                    snaps: Snaps::load(),
                    cursor_at: [-1.0; 2],
                    attack: None,
                    last_attack: None,
                    riding: false,
                    chat_mode: battle_mode::Mode::load(),
                    map_type: None,
                    battle: battleground::Battle::default(),
                    queue_window: battle_queue::QueueWindow::default(),
                    queued: None,
                    queue_asked: None,
                    camp_looks: None,
                    pvp_rank: None,
                    rank_font: None,
                    time_font: None,
                    time_attack: None,
                    last_rank: 0,
                    first_since: None,
                    top_ten: HashSet::new(),
                    blocked: HashSet::new(),
                    party_setup: None,
                    leader_asked: None,
                    talks: whisper::Talks::default(),
                    whisper_open: whisper::OpenSettings::load(),
                    friend_setup: None,
                    block_list: None,
                    away: whisper::Away::default(),
                    away_prompt: None,
                    mercenary_skills: Vec::new(),
                    mercenary_window: mercenary::Window::default(),
                    mercenary_skill_list: mercenary::SkillList::default(),
                    steering: mercenary::Steering::default(),
                    dismiss_asked: false,
                    homunculus: None,
                    homunculus_id: None,
                    elemental: None,
                    homunculus_skills: Vec::new(),
                    homunculus_window: homunculus::Window::default(),
                    homunculus_skill_list: homunculus::SkillList::default(),
                    homunculus_steering: mercenary::Steering::default(),
                    homunculus_rename: None,
                    homunculus_hunger: None,
                    vend_setup: None,
                    my_shop: None,
                    buying_setup: None,
                    my_buying_store: None,
                    mail: mail::Mail::default(),
                    cart: {
                        let mut cart = inventory::Inventory::new(false);
                        cart.title = CART_TITLE;
                        cart.frame.position = CART_POSITION;
                        cart.frame.open = std::env::var_os("RUSTNAROK_CLIENT_CART").is_some();
                        cart
                    },
                    guild: None,
                    guild_window: guild::GuildWindow::default(),
                    guild_invite: None,
                    guild_asked: None,
                    guild_shown: false,
                    clan: None,
                    clan_window: clan::ClanWindow::default(),
                    emblems: HashMap::new(),
                    name_tags: HashMap::new(),
                    pointed: None,
                    signs: HashMap::new(),
                    sign_rects: Vec::new(),
                    chat_links: Vec::new(),
                    dialog_links: Vec::new(),
                    room: None,
                    room_make: None,
                    emotion_list: None,
                    macro_window: None,
                    macro_lines: macros::load(macros::path().as_deref()),
                    room_tried: false,
                    esc_order: Vec::new(),
                    save_asked: false,
                    apply_asked: false,
                    windows: vec![
                        Panel::Hotkeys,
                        Panel::BasicInfo,
                        Panel::Inventory,
                        Panel::Equipment,
                        Panel::Skills,
                        Panel::Storage,
                        Panel::Party,
                        Panel::Status,
                        Panel::Guild,
                        Panel::Clan,
                        Panel::Cart,
                        Panel::Pet,
                        Panel::Mercenary,
                        Panel::MercenarySkills,
                        Panel::Homunculus,
                        Panel::HomunculusSkills,
                    ],
                    damages: Vec::new(),
                    status,
                    unit_hp: HashMap::new(),
                    auto_talk: std::env::var("RUSTNAROK_CLIENT_TALK").ok().map(|talk| talk.split(':').next().unwrap_or_default().to_owned()),
                    auto_next: std::env::var("RUSTNAROK_CLIENT_TALK").ok().and_then(|talk| talk.split(':').nth(1)?.parse().ok()).unwrap_or(0),
                    auto_menu: std::env::var("RUSTNAROK_CLIENT_MENU")
                        .unwrap_or_default()
                        .split(',')
                        .rev()
                        .filter_map(|choice| choice.trim().parse().ok())
                        .collect(),
                    auto_store: std::env::var("RUSTNAROK_CLIENT_STORE").ok(),
                    auto_attack: std::env::var_os("RUSTNAROK_CLIENT_ATTACK").is_some(),
                    auto_pick_up: std::env::var_os("RUSTNAROK_CLIENT_PICKUP").is_some(),
                    auto_wear: std::env::var_os("RUSTNAROK_CLIENT_WEAR").is_some(),
                    auto_keys: std::env::var("RUSTNAROK_CLIENT_HOTKEYS")
                        .unwrap_or_default()
                        .split(',')
                        .filter_map(|slot| slot.trim().parse().ok())
                        .enumerate()
                        .map(|(order, slot)| (Duration::from_secs(3 + order as u64), slot))
                        .collect(),
                    auto_skill: std::env::var("RUSTNAROK_CLIENT_SKILL").ok().and_then(|skill| {
                        let (id, level) = skill.split_once(':')?;
                        Some((id.parse().ok()?, level.parse().ok()?))
                    }),
                    attack_range: 1,
                    pending: None,
                    pending_walk: None,
                    auto_say: std::env::var("RUSTNAROK_CLIENT_SAY").ok(),
                    last_move: None,
                    walk_goal: None,
                    walks: 0,
                    walk_held: false,
                    last_walk_try: None,
                    walk_after_swing: None,
                    skill_info: self.skill_info.clone(),
                    press_mark: None,
                    auto_walk: std::env::var("RUSTNAROK_CLIENT_WALK").ok().and_then(|cell| {
                        let (x, y) = cell.split_once(',')?;
                        Some([x.parse().ok()?, y.parse().ok()?])
                    }),
                }));
                if let Scene::Game(game) = &mut self.scene {
                    game.restore_places();
                }
                for event in std::mem::take(&mut self.early_events) {
                    self.map_event(event);
                }
            }
            Err(error) => self.message(format!("맵을 불러올 수 없습니다: {map} ({error})"), Scene::Login),
        }
    }

    /// Shows a loading screen; the map loads once it's on screen.
    fn show_loading(&mut self, target: LoadTarget) {
        let Some(gpu) = &self.gpu else { return };
        let image = (!self.loading_images.is_empty()).then(|| {
            let pick =
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().subsec_nanos() as usize % self.loading_images.len();
            gpu.try_load_texture(&self.grf, &self.loading_images[pick], true).ok()
        });
        self.loading = Some(Loading { image: image.flatten(), shown: false, target });
        gpu.window.request_redraw();
    }

    /// Loads another map on the same map server.
    fn change_map(&mut self, map: String, position: [u16; 2]) {
        let no_picture = self.messages.get(minimap_window::NO_PICTURE).cloned().unwrap_or_default();
        let (Scene::Game(game), Some(gpu)) = (&mut self.scene, &self.gpu) else { return };
        match World::load(gpu, &self.grf, &map) {
            Ok(world) => {
                let indoor = self.indoor.contains(&map_key(&map));
                game.camera_goal = CameraGoal::arrive(indoor, self.viewpoints.get(&map_key(&map)).copied(), self.saved_view.of(indoor), zoom_out_saved());
                game.camera = game.camera_goal.camera(world.cell_position(position.map(f32::from)));
                game.minimap = minimap::load_map(&self.grf, &map);
                game.minimap_crop = None;
                let image = game.minimap.clone();
                // ponytail: said when a warp leaves the big map open on a map with no overview. A picture that fails to decode counts as none.
                if game.minimap_window.change_map(gpu, image.as_ref(), &map_key(&map)) {
                    game.chat_line(no_picture, CHAT_ERROR);
                    game.sync_esc();
                }
                game.guild_places.clear();
                game.world = world;
                game.map = map;
                game.units.clear();
                game.spirits.clear();
                game.ground.clear();
                game.marks.clear();
                game.signs.clear();
                // The server ends the search on leaving the map.
                game.catalog = None;
                if let Some(shop) = &game.my_shop {
                    game.signs.insert(game.account_id, chatroom::Sign::shop(game.account_id, shop.name.clone()));
                }
                if let Some(store) = &game.my_buying_store {
                    game.signs.insert(game.account_id, chatroom::Sign::buying_store(game.account_id, store.name.clone()));
                }
                game.effects.clear();
                game.capturing = false;
                // The server announces the pet again on load.
                (game.pet_id, game.pet) = (None, None);
                game.player.stop_at(position.map(i32::from));
                let _ = game.session.loaded();
            }
            Err(error) => game.chat_line(format!("맵을 불러올 수 없습니다: {map} ({error})"), CHAT_ERROR),
        }
    }

    /// The loading screen: the picture over everything and an empty progress bar.
    fn draw_loading(&mut self, event_loop: &ActiveEventLoop) {
        let (Some(gpu), Some(assets), Some(loading)) = (&mut self.gpu, &self.assets, &mut self.loading) else { return };
        let mut layer = Layer::default();
        if let Some(image) = &loading.image {
            let (at, size) = picture_place();
            layer.sprites.push((image, at, size));
        }
        let [width, height] = LOADING_BAR;
        let corner = [((screen()[0] - width) / 2.0).floor(), (screen()[1] * 0.75).floor()];
        layer.sprites.push((&assets.loading_bar[0], corner, LOADING_BAR));
        layer.sprites.push((&assets.loading_bar[1], offset(corner, [1.0, 1.0]), [width - 2.0, height - 2.0]));
        let percent = "0%";
        layer.texts.push((percent.to_owned(), [((screen()[0] - self.font.width(percent)) / 2.0).floor(), corner[1] + 1.0], [255, 255, 0]));
        let composed = compose(gpu, &self.font, vec![layer]);
        let (sprites, alphas) = flatten(&composed);
        // Screenshots wait for the game itself, unless RUSTNAROK_CLIENT_SHOT_LOADING asks for this screen.
        match &self.screenshot {
            Some(path) if std::env::var_os("RUSTNAROK_CLIENT_SHOT_LOADING").is_some() => {
                gpu.capture(None, &sprites, &alphas, path);
                event_loop.exit();
            }
            _ => gpu.draw(None, &sprites, &alphas),
        }
        loading.shown = true;
    }

    fn map_event(&mut self, event: MapEvent) {
        if let MapEvent::Entered { position, direction, clock } = event {
            self.server_clock = Some(clock);
            self.show_loading(LoadTarget::Enter { position, direction });
            return;
        }
        if let MapEvent::ServerTime(tick, at) = event {
            self.server_clock = Some((tick, at));
            return;
        }

        let Scene::Game(game) = &mut self.scene else {
            if self.entering.is_some() {
                self.early_events.push(event);
            }
            return;
        };
        let (grf, job_names, look_tables, pets) = (&self.grf, &self.job_names, &self.look_tables, &self.pets);
        // Which kind of pick a making list is (read before the event is taken apart).
        let event_kind = match &event {
            MapEvent::MakeList(_) => Some(Picking::Make),
            MapEvent::CookList { kind, .. } => Some(Picking::Cook(*kind)),
            MapEvent::ArrowList(_) => Some(Picking::Arrow),
            _ => None,
        };
        let (gpu, table) = (self.gpu.as_ref(), &self.effect_table);
        // The effects a skill plays at a moment on a unit (`role` as in the client's skill effect table).
        let skill_effects = |game: &mut Game, skill: u16, role: &str, unit: u32| {
            if let Some(gpu) = gpu {
                for effect in table.skill(skill, role) {
                    game.play_effect(gpu, grf, table, effect, unit);
                }
            }
        };
        let new_actor = |unit: &net::Unit| {
            let sprites = match unit.kind {
                PLAYER => SpriteSet::player(grf, &unit_look(unit), look_tables),
                PET => job_names.get(&unit.job).and_then(|name| pet_sprites(grf, name, pets.tables.accessory_acts.get(&unit.head_bottom))),
                MERCENARY if (6017..=6046).contains(&unit.job) => job_names.get(&unit.job).and_then(|name| SpriteSet::mercenary(grf, unit.job, name)),
                // Hidden NPCs (script triggers) have no sprite in the client either; a warp portal is only its rings.
                _ if HIDDEN_JOBS.contains(&unit.job) || unit.job == WARP_JOB => None,
                _ => job_names.get(&unit.job).and_then(|name| SpriteSet::single(grf, name, unit.kind)),
            };
            let mut actor = Actor::new(unit.name.clone(), sprites, unit.position.map(i32::from), unit.direction, unit.speed);
            actor.kind = unit.kind;
            actor.job = unit.job;
            actor.level = unit.level;
            actor.look = (unit.kind == PLAYER).then(|| unit_look(unit));
            actor.hidden = HIDDEN_JOBS.contains(&unit.job) || unit.job == WARP_JOB;
            actor.cloaked = unit.option & actor::UNSEEN_OPTIONS != 0;
            actor.hide_option = unit.option;
            actor.head_direction = unit.head_direction;
            actor.guild = unit.guild;
            actor
        };
        match event {
            MapEvent::Unit(unit) => {
                // A warp portal shows its rings while it's there (EF_WARPZONE2).
                if unit.job == WARP_JOB
                    && !game.units.contains_key(&unit.id)
                    && let Some(gpu) = gpu
                {
                    let at = game.world.cell_position(unit.position.map(f32::from));
                    game.effects.play(gpu, grf, table, EF_WARPZONE2, effects::Target::Unit(Some(unit.id)), at, Instant::now());
                }
                let actor = game.units.entry(unit.id).or_insert_with(|| new_actor(&unit));
                // Back in sight, or a monster that respawned under its corpse's ID: a unit of its own again.
                if actor.remove_at.is_some() {
                    *actor = new_actor(&unit);
                }
                actor.body_state = unit.body_state;
                actor.health_state = unit.health_state;
                actor.stop_at(unit.position.map(i32::from));
                actor.set_direction(unit.direction);
                actor.head_direction = unit.head_direction;
                actor.sitting = unit.sitting;
                // A player seen again may have changed clothes (or guilds) meanwhile.
                if actor.look.as_ref().is_some_and(|look| *look != unit_look(&unit)) {
                    actor.look = Some(unit_look(&unit));
                    actor.sprites = SpriteSet::player(grf, actor.look.as_ref().unwrap(), look_tables);
                }
                actor.guild = unit.guild;
                actor.cloaked = unit.option & actor::UNSEEN_OPTIONS != 0;
                // Our own entry is the player actor. A copy in the unit list doesn't grow a second falcon or warg.
                if unit.id == game.account_id {
                    game.player.cloaked = actor.cloaked;
                    game.player.sync_companions(unit.option, grf);
                } else {
                    actor.sync_companions(unit.option, grf);
                }
                game.want_emblem(unit.guild, unit.emblem);
                game.monster_hp(&unit);
            }
            MapEvent::Turn { id, direction, head_direction } => {
                let actor = if id == game.account_id { Some(&mut game.player) } else { game.units.get_mut(&id) };
                if let Some(actor) = actor {
                    actor.set_direction(direction);
                    actor.head_direction = head_direction;
                }
            }
            MapEvent::UnitWalk { unit, to, start_time, sub } => {
                let actor = game.units.entry(unit.id).or_insert_with(|| new_actor(&unit));
                if actor.remove_at.is_some() {
                    *actor = new_actor(&unit);
                }
                actor.body_state = unit.body_state;
                actor.health_state = unit.health_state;
                let from = unit.position.map(i32::from);
                actor.guild = unit.guild;
                // On the server's clock, a little behind it; from where it really is in its cell.
                let now = control::now();
                let begin = self.server_clock.map_or(now, |clock| actor::local_time(clock, start_time) + actor::REMOTE_DELAY);
                let path = world::find_path(&game.world.gat, from, to.map(i32::from)).unwrap_or_default();
                actor.walk_from(from, sub, path, begin, now);
                actor.cloaked = unit.option & actor::UNSEEN_OPTIONS != 0;
                // A walk into view is often the first packet. The falcon and warg come with it.
                if unit.id == game.account_id {
                    game.player.cloaked = actor.cloaked;
                    game.player.sync_companions(unit.option, grf);
                } else {
                    actor.sync_companions(unit.option, grf);
                }
                game.want_emblem(unit.guild, unit.emblem);
                game.monster_hp(&unit);
            }
            // The bank: open on the server's word, then ask the balance; refusals by rathena's reasons.
            // ponytail: which message goes with which reason is read off their words.
            MapEvent::BankOpened => {
                game.bank = Some(bank::Bank::new());
                let _ = game.session.check_bank(game.account_id);
            }
            MapEvent::BankBalance(balance) => {
                if let Some(bank) = &mut game.bank {
                    bank.balance = balance;
                }
            }
            MapEvent::BankMoved { deposit, reason, balance, zeny } => {
                if let Some(bank) = &mut game.bank {
                    bank.balance = balance;
                }
                game.status.insert(net::STATUS_ZENY, i64::from(zeny));
                let refusal = match (deposit, reason) {
                    (_, 0) => None,
                    (true, 2) => Some(BANK_NO_ZENY),
                    (true, 3) => Some(BANK_OVERFLOW),
                    (false, 1) => Some(BANK_SHORT),
                    _ => Some(BANK_DOWN),
                };
                if let Some(id) = refusal {
                    game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ERROR);
                }
            }
            MapEvent::CashShopOpen { cash, free, tab } => {
                let mut shop = cashshop::CashShop::new(cash, free);
                shop.tabs.clone_from(&game.cashshop_lists);
                if usize::try_from(tab).ok().is_some_and(|tab| tab < shop.tabs.len() && !shop.tabs[tab].is_empty()) {
                    shop.selected_tab = tab as usize;
                }
                game.cashshop = Some(shop);
                if !game.cashshop_list_requested {
                    game.cashshop_list_requested = true;
                    let _ = game.session.request_cashshop_list();
                }
            }
            MapEvent::CashShopList { tab, items } => {
                if let Some(list) = usize::try_from(tab).ok().and_then(|tab| game.cashshop_lists.get_mut(tab)) {
                    list.extend(items.iter().copied());
                }
                if let Some(shop) = &mut game.cashshop {
                    shop.append(tab, items);
                    if shop.tabs[shop.selected_tab].is_empty()
                        && let Some(first) = shop.tabs.iter().position(|items| !items.is_empty())
                    {
                        shop.selected_tab = first;
                    }
                }
            }
            MapEvent::CashShopBuyResult { result, cash, free, .. } => {
                if let Some(shop) = &mut game.cashshop {
                    shop.clear_cart_on_success(result, cash, free);
                }
                let id = cashshop::buy_result_message(result);
                game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), if result == 0 { CHAT_ITEM } else { CHAT_ERROR });
            }
            // Another player's equipment: their window with their figure, its weapon and shield from what they wear.
            MapEvent::OtherEquipment(equipped) => {
                let held = |location: u32| equipped.items.iter().find(|(item, _)| item.worn & location != 0).map_or(0, |(_, look)| u32::from(*look));
                let look = Look {
                    job: equipped.job,
                    sex: equipped.sex,
                    hair: equipped.hair,
                    hair_color: equipped.hair_color,
                    clothes_color: equipped.clothes_color,
                    weapon: held(2),
                    shield: held(32),
                    head_top: equipped.head_top,
                    head_mid: equipped.head_mid,
                    head_bottom: equipped.head_bottom,
                    robe: equipped.robe,
                    option: 0,
                };
                game.viewing = Some(Viewed {
                    window: equipment::Equipment::of(equipped.name),
                    items: equipped.items.into_iter().map(|(item, _)| item).collect(),
                    sprites: SpriteSet::player(grf, &look, look_tables),
                });
            }
            MapEvent::ShowEquipment { shown, answer } => {
                game.equipment.shown = shown;
                if answer {
                    let id = if shown { EQUIPMENT_SHOWN } else { EQUIPMENT_HIDDEN };
                    game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ITEM);
                }
            }
            MapEvent::ExpansionAsked { result: 0, voucher } => game.expansion_asked = Some(voucher),
            // The results, done (0) or why not; the question's own result 0 is handled above.
            MapEvent::ExpansionAsked { result, .. } | MapEvent::ExpansionDone(result) => {
                let id = EXPANSION_RESULTS.get(usize::from(result)).copied().unwrap_or(EXPANSION_RESULTS[1]);
                game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), if result == 0 { CHAT_ITEM } else { CHAT_ERROR });
            }
            // Change Material or Four Spirit Analysis: the material window, with the inventory beside it.
            MapEvent::MaterialWindow(kind) => {
                game.materials = Some(material::Materials::new(kind));
                game.inventory.frame.open = true;
                game.raise(Panel::Inventory);
            }
            // The refine window opens with the inventory beside it.
            MapEvent::RefineOpen => {
                game.refine = Some(refine::Refine::new());
                game.inventory.frame.open = true;
                game.raise(Panel::Inventory);
            }
            // Kind 8 is grade enchanting and opens the inventory beside it. Kind 1 is the stylist.
            MapEvent::UiOpen { kind, .. } => {
                if kind == grade::UI_KIND {
                    game.grade = Some(grade::GradeEnchant::new());
                    game.inventory.frame.open = true;
                    game.raise(Panel::Inventory);
                } else if kind == stylist::UI_KIND {
                    game.stylist = Some(stylist::Window::open());
                    let look = game.player.look.clone().unwrap_or_default();
                    if let Some(window) = &mut game.stylist {
                        dress_stylist(window, grf, look_tables, &look, &self.items, &self.styling_shop);
                    }
                }
            }
            MapEvent::GradeMaterials { index, chance, blessing, materials } => {
                if index < 0 {
                    game.chat_line(self.messages.get(grade::CANT_UPGRADE).cloned().unwrap_or_default(), CHAT_ERROR);
                    if let Some(window) = &mut game.grade {
                        window.clear();
                    }
                } else if let Some(window) = &mut game.grade {
                    window.listed(index as u16, chance, blessing, materials);
                }
            }
            // The item's new grade. Result 3 already deleted it. 0 and 1 ask for the list again.
            MapEvent::GradeResult { index, grade: next, result } => {
                if result != 3
                    && let Some(item) = game.inventory.items.iter_mut().find(|item| item.index == index)
                {
                    grade::apply_result(item, next, result);
                }
                if let Some(window) = &mut game.grade {
                    let again = window.finished(result);
                    if let Some(&id) = grade::RESULT_LINES.get(result as usize) {
                        let color = if result == 0 || result == 4 { CHAT_ITEM } else { CHAT_ERROR };
                        game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), color);
                    }
                    if again {
                        let _ = game.session.grade_select(index);
                    }
                }
            }
            MapEvent::SynthesisOpen(id) => match self.synthesis_boxes.get(&id) {
                Some(info) => {
                    game.synthesis = Some(laphine::Synthesis::new(id, info));
                    game.inventory.frame.open = true;
                    game.raise(Panel::Inventory);
                }
                None => {
                    let _ = game.session.close_synthesis();
                }
            },
            MapEvent::UpgradeOpen(id) => match self.upgrade_boxes.get(&id) {
                Some(info) => {
                    game.upgrade = Some(laphine::Upgrade::new(id, info));
                    game.inventory.frame.open = true;
                    game.raise(Panel::Inventory);
                }
                None => {
                    let _ = game.session.close_upgrade();
                }
            },
            // A reform item the client has no entries for: tell the server and open nothing.
            MapEvent::ReformOpen(item) => match self.reforms.get(&item).filter(|entries| !entries.is_empty()) {
                Some(entries) => {
                    game.reform = Some(reform::Reform::open(item, entries.clone()));
                    game.inventory.frame.open = true;
                    game.raise(Panel::Inventory);
                }
                None => {
                    game.reform = None;
                    let _ = game.session.close_reform();
                }
            },
            MapEvent::Synthesized(result) => {
                if result == 0
                    && let Some(window) = &mut game.synthesis
                {
                    window.clear();
                }
                let id = match result {
                    5 => Some(laphine::NOT_ENOUGH),
                    7 => Some(laphine::NOTHING),
                    _ => None,
                };
                if let Some(id) = id {
                    game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ERROR);
                }
            }
            MapEvent::LaphineUpgraded(result) => {
                // ponytail: 2971 and 2972 are guesses for the upgrade's result lines.
                let (id, color) = if result == 0 {
                    if let Some(window) = &mut game.upgrade {
                        window.clear();
                    }
                    (laphine::UPGRADE_OK, CHAT_ITEM)
                } else {
                    (laphine::UPGRADE_FAIL, CHAT_ERROR)
                };
                game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), color);
            }
            MapEvent::ItemPreview { index, cards, refine, broken, grade } => {
                let damaged_line = self.messages.get(ITEM_DAMAGED).cloned().unwrap_or_default();
                let shown = game.inventory.items.iter_mut().find(|item| item.index == index).map(|item| {
                    item.cards = cards;
                    item.refine = refine;
                    item.grade = grade;
                    item.broken = broken;
                    let damaged = item.broken.then(|| damaged_line.clone());
                    iteminfo::Shown::of(item, damaged)
                });
                // ponytail: that the preview is the item info window is a guess.
                if let Some(shown) = shown {
                    let position = game.item_window.as_ref().map(|window| window.position);
                    let mut window = iteminfo::ItemWindow::new(shown);
                    if let Some(position) = position {
                        window.position = position;
                    }
                    game.item_window = Some(window);
                }
            }
            // Result 0: rathena has forgotten the window, so close it. The inventory updates arrived on their own.
            MapEvent::Reformed { result: 0, .. } => game.reform = None,
            MapEvent::Reformed { .. } => {}
            // A table this client doesn't have: say so, and tell rathena the window is closed.
            MapEvent::EnchantOpen(table) => match self.enchant_tables.get(&table).cloned() {
                Some(info) => {
                    game.enchant = Some(enchant::Enchant::open(table, info));
                    game.inventory.frame.open = true;
                    game.raise(Panel::Inventory);
                }
                None => {
                    game.chat_line(self.messages.get(enchant::NOT_FOUND).cloned().unwrap_or_default(), CHAT_ERROR);
                    let _ = game.session.close_enchant();
                }
            },
            MapEvent::Enchanted { message, item } => {
                let success = message == enchant::SUCCESS as u32;
                if let Some(window) = &game.enchant
                    && let Some(index) = window.item
                    && let Some(carried) = game.inventory.items.iter_mut().find(|carried| carried.index == index)
                {
                    carried.cards = window.cards_after(message, item, carried.cards);
                }
                let text = self.messages.get(message as usize).cloned().unwrap_or_default();
                game.chat_line(text, if success { CHAT_ITEM } else { CHAT_ERROR });
                game.enchant = None;
            }
            MapEvent::RefineMaterials { index, blessings, materials } => {
                if materials.is_empty() {
                    game.chat_line(self.messages.get(CANT_REFINE).cloned().unwrap_or_default(), CHAT_ERROR);
                } else if let Some(window) = &mut game.refine {
                    window.listed(index, blessings, materials);
                }
            }
            // Any refine (the window, Upgrade Weapon, @refine): the item's new level; a broken one leaves the window.
            MapEvent::Refined { result, index, refine } => {
                if result != 1
                    && let Some(item) = game.inventory.items.iter_mut().find(|item| item.index == index)
                {
                    item.refine = refine as u8;
                }
                if let Some(window) = &mut game.refine {
                    if result == 1 {
                        window.clear();
                    }
                    if let Some(&(id, color)) = REFINE_RESULTS.get(usize::from(result)) {
                        game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), color);
                    }
                }
            }
            MapEvent::Spirits { id, count } => {
                change_orbs(&mut game.spirits, (id != game.account_id).then_some(id), |orbs| orbs.spheres = count);
            }
            // A Royal Guard's rage spheres come as shields too.
            MapEvent::Shields { id, count } => {
                let job = if id == game.account_id { Some(game.me.job) } else { game.units.get(&id).map(|actor| actor.job) };
                let guard = job.is_some_and(guard_job);
                change_orbs(&mut game.spirits, (id != game.account_id).then_some(id), |orbs| {
                    *if guard { &mut orbs.spheres } else { &mut orbs.shields } = count
                });
            }
            MapEvent::Charms { id, kind, count } => {
                change_orbs(&mut game.spirits, (id != game.account_id).then_some(id), |orbs| {
                    orbs.charms = if (1..=4).contains(&kind) { (kind, count) } else { (0, 0) }
                });
            }
            MapEvent::Souls { id, count } => change_orbs(&mut game.spirits, (id != game.account_id).then_some(id), |orbs| orbs.souls = count),
            MapEvent::Vanish { id, reason } => {
                if id != game.account_id {
                    game.spirits.remove(&Some(id));
                }
                lock_pair(&mut game.locks, id, id, false);
                game.hold_locked();
                // A body keeps what plays on it (the killing hit's effects) until it goes.
                if reason != 1 {
                    game.effects.forget(id);
                }
                // Reason 1: died. The body falls when the killing hit lands, then stays a moment.
                let key = (id != game.account_id).then_some(id);
                let lands = game.damages.iter().filter(|damage| damage.target == key).map(|damage| damage.start).fold(Instant::now(), Instant::max);
                let dying = if id == game.account_id { Some(&mut game.player) } else { game.units.get_mut(&id) };
                // Our own death brings up the option menu with the save point.
                if id == game.account_id && reason == 1 {
                    game.escape = Some(escape::Menu { dead: true });
                }
                // Teleporting and logging out leave rings and a sound on the spot (not for a GM hidden from sight).
                if id != game.account_id
                    && let (Some(gpu), Some(actor)) = (gpu, dying.as_deref())
                    && let Some(effect) = vanish_effect(reason, actor.hide_option)
                {
                    let at = game.world.cell_position(actor.position(Instant::now()));
                    game.effects.play(gpu, grf, table, effect, effects::Target::Ground, at, Instant::now());
                }
                let dying = if id == game.account_id { Some(&mut game.player) } else { game.units.get_mut(&id) };
                match (reason, dying) {
                    (1, Some(actor)) => fall(actor, lands),
                    // Out of sight: it fades out, its shadow gone at once.
                    (0, Some(actor)) if id != game.account_id => {
                        actor.fade_out(Instant::now(), OUT_OF_SIGHT_FADE_MS);
                        game.unit_hp.remove(&id);
                    }
                    _ => {
                        game.units.remove(&id);
                        game.unit_hp.remove(&id);
                        // Whoever comes back with this account may be another character.
                        game.name_tags.remove(&id);
                    }
                }
            }
            MapEvent::PlayerMove { from, to, .. } => {
                let from = from.map(i32::from);
                if let Some(path) = world::find_path(&game.world.gat, from, to.map(i32::from)) {
                    game.player.walk_on(from, path, control::now());
                }
            }
            // A knockback (ZC_HIGHJUMP) slides the unit there; the stop that follows at the same cell leaves the slide
            // running.
            MapEvent::Stop { id, position } | MapEvent::Slide { id, position } => {
                let cell = position.map(i32::from);
                let slide = matches!(event, MapEvent::Slide { .. });
                let now = control::now();
                let actor = if id == game.account_id { Some(&mut game.player) } else { game.units.get_mut(&id) };
                if let Some(actor) = actor {
                    if slide {
                        actor.slide_to(cell, now);
                    } else if actor.sliding_to() != Some(cell) {
                        actor.stop_at(cell);
                    }
                }
                if id == game.account_id
                    && let Some((_, arrived)) = &mut game.pending
                {
                    *arrived = Some(Instant::now());
                }
            }
            // In a chat room, talk goes to its window.
            MapEvent::Chat { id, message, .. } if game.room.is_some() => {
                game.room.as_mut().unwrap().lines.push((message, if id.is_none() { chatroom::OWN_LINE } else { [0, 0, 0] }));
            }
            MapEvent::Chat { id, message, color } => {
                game.say(id, message.clone(), color.unwrap_or(CHAT_WHITE));
                game.chat_line(message, color.unwrap_or(if id.is_none() { CHAT_SELF } else { CHAT_WHITE }));
            }
            MapEvent::MapChange { map, position } => {
                // The new map's weather comes once it has loaded, and the mercenary's news, its type and the PvP count.
                game.weather.clear();
                game.pvp_rank = None;
                game.map_type = None;
                game.battle = battleground::Battle::default();
                game.last_rank = 0;
                game.first_since = None;
                game.top_ten.clear();
                game.forget_mercenary(false);
                game.forget_homunculus(false);
                // Back at the save point: up again.
                if game.escape.as_ref().is_some_and(|menu| menu.dead) {
                    game.escape = None;
                }
                game.player.revive();
                // The NPC is left behind: its shop and its buy-or-sell question close (the server doesn't say so).
                game.close_shop();
                game.close_cashshop();
                // rathena forgets the refine window on a map change; the material window, the roulette and the reform
                // window go too. The reform window sends no close: the server already forgot it.
                // rathena forgets the refine window and the grade window on a map change (no packet); the material window and the roulette go too.
                game.refine = None;
                game.grade = None;
                // rathena forgets both Laphine windows, the enchant window and the stylist on a map change, without a close packet.
                game.synthesis = None;
                game.upgrade = None;
                game.reform = None;
                game.enchant = None;
                game.stylist = None;
                game.roulette = None;
                // The mirror's status ends with the warp.
                game.boss = None;
                game.materials = None;
                game.deal_choice = None;
                game.attack = None;
                // The merge window closes without a word.
                if matches!(game.pick, Some((_, Picking::Merge))) {
                    game.pick = None;
                }
                game.combos.clear();
                game.locks.clear();
                game.hold_locked();
                if map == game.map {
                    // A teleport on the same map: no loading, but the server takes the player off the map until
                    // the load is acknowledged (it drops everything else till then) and sends what's in view again.
                    game.player.stop_at(position.map(i32::from));
                    game.units.clear();
                    game.ground.clear();
                    let _ = game.session.loaded();
                } else {
                    self.show_loading(LoadTarget::Change { map, position });
                }
            }
            MapEvent::ComboWindow { id, ms } => {
                game.combos.insert((id != game.account_id).then_some(id), (control::now(), Duration::from_millis(u64::from(ms))));
            }
            // Locked units turn to face each other.
            // ponytail: the facing is an estimate.
            MapEvent::BladeStop { source, target, on } => {
                lock_pair(&mut game.locks, source, target, on);
                let me = game.account_id;
                let cell = |game: &Game, id: u32| if id == me { Some(game.player.cell) } else { game.units.get(&id).map(|actor| actor.cell) };
                if on && let (Some(from), Some(to)) = (cell(game, source), cell(game, target)) {
                    for (id, toward) in [(source, to), (target, from)] {
                        if let Some(actor) = if id == me { Some(&mut game.player) } else { game.units.get_mut(&id) } {
                            actor.face(toward);
                        }
                    }
                }
                game.hold_locked();
            }
            MapEvent::NpcSay { npc, text } => {
                let name_of = |item: &links::ItemLink| inventory::item_name(&self.items, item.id, true, item.refine);
                game.dialog_for(npc).lines.push(show_links(&links::mark(&text, &name_of)));
            }
            MapEvent::NpcNext { npc } => game.dialog_for(npc).button = DialogButton::Next,
            // A script may end in `close` without having said anything (rathena's item scripts do): no box for
            // that, but answer it so the script doesn't keep the player.
            MapEvent::NpcClose { npc } if game.dialog.as_ref().is_none_or(|dialog| dialog.npc != npc) => {
                let _ = game.session.close_dialog(npc);
            }
            MapEvent::NpcClose { npc } => game.dialog_for(npc).button = DialogButton::Close,
            MapEvent::NpcClear { npc } => {
                if game.dialog.as_ref().is_some_and(|dialog| dialog.npc == npc) {
                    game.dialog = None;
                    game.cutin = None;
                }
            }
            MapEvent::NpcMenu { npc, items } => {
                let items = items.into_iter().filter(|item| !item.is_empty()).collect();
                game.dialog_for(npc).menu = Some((items, 0));
            }
            MapEvent::NpcInput { npc, text } => game.dialog_for(npc).input = Some((text, String::new())),
            // Where Warp Portal or Teleport goes, in the NPC menu's box: "Random Area" for Teleport's random spot, the
            // maps by their file names, then Cancel.
            MapEvent::WarpList { skill, maps } => {
                let message = |id: usize| self.messages.get(id).cloned().unwrap_or_default();
                let mut items: Vec<String> =
                    maps.iter().map(|map| if map.trim_end_matches(".gat") == "Random" { message(RANDOM_AREA) } else { map.clone() }).collect();
                items.push(message(WARP_CANCEL));
                game.dialog =
                    Some(Dialog { npc: 0, lines: Vec::new(), button: DialogButton::None, menu: Some((items, 0)), input: None, warp: Some((skill, maps)) });
            }
            MapEvent::Act(act) => {
                // A landed hit on a monster sparks (EF_HIT1).
                let monster = game.units.get(&act.target).is_some_and(|actor| actor.kind == MONSTER);
                if let Some(gpu) = gpu
                    && monster
                    && act.damage > 0
                    && matches!(act.kind, 0 | 4 | 8 | 9 | 10 | 13)
                {
                    let lands = Instant::now() + Duration::from_millis(act.source_speed.max(0) as u64);
                    game.play_effect_from(gpu, grf, table, EF_HIT1, act.target, lands);
                }
                game.act(act, look_tables);
            }
            MapEvent::Status { id, value } => {
                let before = game.status.insert(id, value);
                game.level_ups.changed(id, before, value);
            }
            MapEvent::Statuses(values) => game.status.extend(values),
            // -1 takes the bar away.
            MapEvent::UnitHp { id, hp, max_hp } if hp < 0 || max_hp <= 0 => {
                game.unit_hp.remove(&id);
            }
            MapEvent::UnitHp { id, hp, max_hp } => {
                game.unit_hp.insert(id, (hp, max_hp));
            }
            MapEvent::AttackRange(range) => game.attack_range = i32::from(range),
            MapEvent::GroundItem(item) => {
                let resource = ground_sprite(&self.items, item.item, item.identified).to_owned();
                game.item_sprites.entry(resource.clone()).or_insert_with(|| SpriteSet::at(grf, &format!("data\\sprite\\아이템\\{resource}")));
                let [x, y] = item.cell;
                // Sixteenths of a cell from its low corner (8 is the middle).
                let [sub_x, sub_y] = item.sub.map(|sub| f32::from(sub) / 16.0);
                game.ground.insert(
                    item.id,
                    FloorItem {
                        item: item.item,
                        identified: item.identified,
                        amount: item.amount,
                        at: [i32::from(x), i32::from(y)],
                        cell: [f32::from(x) - 0.5 + sub_x, f32::from(y) - 0.5 + sub_y],
                        seen: Instant::now(),
                        dropped: item.falling.then(Instant::now),
                    },
                );
            }
            MapEvent::ItemGone { id } => {
                game.ground.remove(&id);
                if game.pending.is_some_and(|(pending, _)| pending == Pending::PickUp(id)) {
                    game.pending = None;
                }
            }
            MapEvent::PickedUp { result, item } => {
                let message = |id: usize| self.messages.get(id).cloned().unwrap_or_default();
                let held = game.inventory.get(item.index).map_or(0, |held| held.amount);
                // A forged or brewed item names its maker: asked for once.
                if let Some(maker) = inventory::maker(&item.cards)
                    && !game.char_names.contains_key(&maker)
                    && game.names_asked.insert(maker)
                {
                    let _ = game.session.ask_char_name(maker);
                }
                let name = |slots: bool| inventory::full_name(&self.items, &self.card_names, &self.messages, &game.char_names, &item, slots);
                match pickup::obtained(result, item.amount, held) {
                    Some(amount) => {
                        let line = messages::fill(&message(messages::GOT_ITEM), &[&name(true), &amount]);
                        // The popup: the name without its slot count, how many came, the icon.
                        let (short, maker) = inventory::full_name_maker(&self.items, &self.card_names, &self.messages, &game.char_names, &item, false);
                        let text = format!("{short}{}", messages::fill(&message(pickup::OBTAINED), &[&amount]));
                        let icon = self.items.get(&item.id).map(|info| inventory::icon_path(info, item.identified)).unwrap_or_default();
                        let mut popup = pickup::Popup::new(text, icon, self.font.width(&line), control::now());
                        popup.maker = maker;
                        game.pickup_popup = Some(popup);
                        game.new_items = true;
                        game.chat_line(line, CHAT_ITEM);
                        game.inventory.add(net::InventoryItem { amount, ..item });
                    }
                    None => {
                        if let Some(id) = pickup::failure_line(result) {
                            game.chat_line(message(id), CHAT_ERROR);
                        }
                    }
                }
            }
            // The inventory again: what broke since says so; the out pet's egg (marked broken) stays hidden.
            MapEvent::InventoryStart { kind: 0 } => {
                let had = !game.inventory.items.is_empty();
                game.broken_before = had.then(|| game.inventory.items.iter().filter(|item| item.broken).map(|item| item.index).collect());
                game.inventory.clear();
            }
            MapEvent::InventoryItems { kind: 0, items } => {
                // ponytail: rathena never sends ZC_EQUIPITEM_DAMAGED, so the client notices a break in the resent list.
                if let Some(before) = &game.broken_before {
                    let message = |id: usize| self.messages.get(id).map_or("", String::as_str);
                    let lines: Vec<String> = inventory::newly_broken(before, &items)
                        .into_iter()
                        .map(|item| {
                            let place = message(if item.kind == net::ITEM_WEAPON { DAMAGED_WEAPON } else { DAMAGED_ARMOR });
                            messages::fill(message(DAMAGED), &[&game.me.name, &place])
                        })
                        .collect();
                    for line in lines {
                        game.chat_line(line, CHAT_ERROR);
                    }
                }
                game.inventory.extend(items.into_iter().filter(|item| !inventory::hidden_egg(item)).collect());
            }
            MapEvent::BoundOnEquip(index) => {
                if let Some(item) = game.inventory.items.iter_mut().find(|item| item.index == index) {
                    item.bind = 2;
                    let name = inventory::item_name(&self.items, item.id, item.identified, item.refine);
                    game.chat_line(messages::fill(self.messages.get(BOUND).map_or("", String::as_str), &[&name]), CHAT_ITEM);
                }
            }
            // ponytail: no line fits a refusal; it says nothing.
            MapEvent::TookOffAll { .. } => {}
            MapEvent::SwitchSet(set) => {
                for item in &mut game.inventory.items {
                    item.switch = set.iter().find(|(index, _)| *index == item.index).map_or(0, |(_, places)| *places);
                }
            }
            // The player's own add and remove get a line.
            // ponytail: whether the client also says so for items a switch moves isn't knowable; it doesn't.
            MapEvent::SwitchAdded { index, places, result } => {
                let message = |id: usize| self.messages.get(id).cloned().unwrap_or_default();
                let asked = game.switch_asked.take_if(|asked| *asked == index).is_some();
                let name = game.inventory.items.iter_mut().find(|item| item.index == index && result == 0).map(|item| {
                    item.switch = places;
                    inventory::item_name(&self.items, item.id, item.identified, item.refine)
                });
                match result {
                    0 if asked => game.chat_line(format!("{}{}", name.unwrap_or_default(), message(SWITCH_ADDED)), CHAT_ITEM),
                    0 => {}
                    1 => game.chat_line(message(1773), CHAT_ERROR),
                    _ => game.chat_line(message(372), CHAT_ERROR),
                }
            }
            MapEvent::SwitchRemoved { index, .. } => {
                let asked = game.switch_asked.take_if(|asked| *asked == index).is_some();
                let name = game.inventory.items.iter_mut().find(|item| item.index == index).map(|item| {
                    item.switch = 0;
                    inventory::item_name(&self.items, item.id, item.identified, item.refine)
                });
                if let Some(name) = name.filter(|_| asked) {
                    game.chat_line(format!("{name}{}", self.messages.get(SWITCH_REMOVED).cloned().unwrap_or_default()), CHAT_ITEM);
                }
            }
            // The wear and take-off answers that follow move the items.
            MapEvent::Switched { .. } => {}
            MapEvent::FavoriteMoved { index, favorite } => {
                if let Some(item) = game.inventory.items.iter_mut().find(|item| item.index == index) {
                    item.favorite = favorite;
                }
            }
            // The storage opens with the inventory beside it.
            MapEvent::InventoryStart { kind: 2 } => {
                game.storage.open();
                game.raise(Panel::Storage);
                game.inventory.frame.open = true;
            }
            MapEvent::InventoryItems { kind: 2, items } => game.storage.extend(items),
            MapEvent::InventoryStart { kind: 1 } => game.cart.clear(),
            MapEvent::InventoryItems { kind: 1, items } => game.cart.extend(items),
            MapEvent::InventoryStart { .. } | MapEvent::InventoryItems { .. } => {}
            MapEvent::Quests(quests) => game.quests = quests,
            // A new quest: its line and icon; one already known is refreshed in place.
            MapEvent::QuestAdded(quest) => {
                if quest_notice::add(&mut game.quests, quest) {
                    game.chat_line(self.messages.get(quest_notice::RECEIVED).cloned().unwrap_or_default(), quest_notice::YELLOW);
                    game.quest_icon = true;
                }
            }
            MapEvent::QuestRemoved(id) => {
                if game.quests.iter().any(|quest| quest.id == id) {
                    let title = self.quest_texts.get(&id).map_or_else(|| id.to_string(), |text| text.title.clone());
                    let line = messages::fill(self.messages.get(quest_notice::DELETED).map_or("", String::as_str), &[&title]);
                    game.chat_line(line, quest_notice::YELLOW);
                }
                game.quests.retain(|quest| quest.id != id);
                if game.quest_window.picked == Some(id) {
                    game.quest_window.picked = None;
                }
            }
            MapEvent::QuestActive { id, active } => {
                if let Some(quest) = game.quests.iter_mut().find(|quest| quest.id == id) {
                    quest.active = active;
                }
            }
            // A counted kill: the counts, a progress line each and its banner.
            MapEvent::QuestProgress(hunts) => {
                let now = control::now();
                for (id, count, needed) in hunts {
                    let Some(quest) = game.quests.iter_mut().find(|quest| quest.hunts.iter().any(|hunt| hunt.id == id)) else { continue };
                    let quest_id = quest.id;
                    let hunt = quest.hunts.iter_mut().find(|hunt| hunt.id == id).unwrap();
                    (hunt.count, hunt.needed) = (count, needed);
                    let hunt = hunt.clone();
                    let title = self.quest_texts.get(&quest_id).map_or_else(|| quest_id.to_string(), |text| text.title.clone());
                    game.chat_line(quest_notice::progress_line(&self.messages, &title, &hunt), quest_notice::YELLOW);
                    game.banners.show(quest_notice::banner_line(&self.messages, &hunt), now);
                }
            }
            MapEvent::QuestHunts(hunts) => {
                for (id, count, needed) in hunts {
                    if let Some(hunt) = game.quests.iter_mut().flat_map(|quest| &mut quest.hunts).find(|hunt| hunt.id == id) {
                        (hunt.count, hunt.needed) = (count, needed);
                    }
                }
            }
            // RODEX. Closing the window written in after sending lets the server know writing is over.
            event @ (MapEvent::MailUnread(_)
            | MapEvent::MailList(_)
            | MapEvent::MailRead { .. }
            | MapEvent::MailClaimed { .. }
            | MapEvent::MailDeleted { .. }
            | MapEvent::MailWrite { .. }
            | MapEvent::MailReceiver { .. }
            | MapEvent::MailItemAdded { .. }
            | MapEvent::MailItemRemoved { .. }
            | MapEvent::MailSent(_)) => {
                if matches!(event, MapEvent::MailSent(0)) {
                    let _ = game.session.cancel_mail();
                }
                if let Some((id, error)) = game.mail.event(event) {
                    game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), if error { CHAT_ERROR } else { CHAT_ITEM });
                }
            }
            // Announcements, and the client's own messages by number.
            // In the chat and the bar; a blocked speaker's not at all.
            MapEvent::Broadcast { text, color, kind, size } => {
                if let net::BroadcastKind::Speaker(name) = &kind
                    && game.blocked.contains(name)
                {
                    return;
                }
                game.chat_line(text.clone(), color);
                game.announcements.push(text, color, size.unwrap_or(announce::SIZE), control::now());
            }
            // ponytail: in the announcements' yellow; a monster's drop says the box's line with the monster's name, as
            // rathena's notes have it, unchecked against the real client.
            MapEvent::ItemObtained { item, player, box_item, monster } => {
                let name = |id: u32| inventory::item_name(&self.items, id, true, 0);
                let template = |id: usize| self.messages.get(id).map_or("", String::as_str);
                let text = match (box_item, monster) {
                    (Some(container), _) => messages::fill(template(ITEM_FROM_BOX), &[&player, &name(container), &name(item)]),
                    (_, Some(monster)) => messages::fill(template(ITEM_FROM_BOX), &[&player, &monster, &name(item)]),
                    _ => messages::fill(template(ITEM_FROM_NPC), &[&player, &name(item)]),
                };
                game.chat_line(text, CHAT_BROADCAST);
            }
            MapEvent::RefineAnnounced { player, item, level, success } => {
                let template = self.messages.get(if success { REFINE_SUCCEEDED } else { REFINE_FAILED }).map_or("", String::as_str);
                game.chat_line(messages::fill(template, &[&player, &level, &inventory::item_name(&self.items, item, true, 0)]), CHAT_BROADCAST);
            }
            // The same yellow as a refine announcement. Refine 0 so the name has no +N.
            // ponytail: the grade's letter is a guess.
            MapEvent::GradeAnnounced { player, item, grade: next, success } => {
                let template = self.messages.get(if success { grade::ANNOUNCE_OK } else { grade::ANNOUNCE_FAIL }).map_or("", String::as_str);
                let name = inventory::item_name(&self.items, item, true, 0);
                game.chat_line(messages::fill(template, &[&player, &grade::grade_letter(next), &name]), CHAT_BROADCAST);
            }
            // The banner effect, and the MVP sprite over the winner.
            // ponytail: ZC_MVP starts the sprite and it plays once through, like an emotion; neither is pinned down.
            MapEvent::Mvp(id) => {
                if let Some(gpu) = gpu {
                    game.play_effect(gpu, grf, table, EF_MVP, id);
                }
                game.mvps.insert((id != game.account_id).then_some(id), control::now());
            }
            // ponytail: the reward lines in the items' cyan; the real client's color is unknown.
            MapEvent::MvpItem(item) => {
                let [before, after] = MVP_ITEM.map(|id| self.messages.get(id).cloned().unwrap_or_default());
                game.chat_line(format!("{before}{}{after}", inventory::item_name(&self.items, item, true, 0)), CHAT_ITEM);
            }
            MapEvent::MvpExp(exp) => {
                let [before, after] = MVP_EXP.map(|id| self.messages.get(id).cloned().unwrap_or_default());
                game.chat_line(format!("{before}{exp}{after}"), CHAT_ITEM);
            }
            MapEvent::OverweightPercent(percent) => game.overweight = percent,
            MapEvent::GuildMemberPlace { account, cell: Some(cell) } => {
                game.guild_places.insert(account, cell);
            }
            MapEvent::GuildMemberPlace { account, cell: None } => {
                game.guild_places.remove(&account);
            }
            MapEvent::MvpOverweight => game.chat_line(self.messages.get(MVP_OVERWEIGHT).cloned().unwrap_or_default(), CHAT_ERROR),
            MapEvent::HomunculusState { state: net::HOMUNCULUS_OWNED, id, .. } => game.homunculus_id = Some(id),
            MapEvent::HomunculusState { state: net::HOMUNCULUS_INTIMACY, value, .. } => {
                if let Some(homunculus) = &mut game.homunculus {
                    homunculus.intimacy = value as u16;
                }
            }
            MapEvent::HomunculusState { state: net::HOMUNCULUS_HUNGER, value, .. } => {
                let hunger = value.min(u32::from(u16::MAX)) as u16;
                if homunculus::starving_now(game.homunculus_hunger, hunger) {
                    game.chat_line(self.messages.get(homunculus::STARVING).cloned().unwrap_or_default(), CHAT_ERROR);
                }
                game.homunculus_hunger = Some(hunger);
                if let Some(homunculus) = &mut game.homunculus {
                    homunculus.hunger = hunger;
                }
            }
            MapEvent::HomunculusState { .. } => {}
            MapEvent::HomunculusInfo(info) => {
                game.homunculus_hunger = Some(info.hunger);
                game.homunculus = Some(info);
            }
            MapEvent::HomunculusChanged { kind, value } => {
                if let Some(homunculus) = &mut game.homunculus {
                    homunculus.set(kind, value);
                }
            }
            MapEvent::HomunculusSkills(list) => game.homunculus_skills = list,
            MapEvent::HomunculusSkill(update) => match game.homunculus_skills.iter_mut().find(|skill| skill.id == update.id) {
                Some(skill) => {
                    skill.level = update.level;
                    skill.sp = update.sp;
                    skill.range = update.range;
                    skill.upgradable = update.upgradable;
                }
                None => game.homunculus_skills.push(update),
            },
            // ponytail: the table has no homunculus line for this; the pet's "food not available" says it.
            MapEvent::HomunculusFed { ok: false, food } => {
                let name = inventory::item_name(&self.items, food, true, 0);
                game.chat_line(messages::fill(self.messages.get(PET_NO_FOOD).map_or("", String::as_str), &[&name]), CHAT_ERROR);
            }
            MapEvent::HomunculusFed { ok: true, .. } => {}
            MapEvent::MercenaryInfo(hired) => game.mercenary = Some(hired),
            MapEvent::ElementalInfo { id, values } => game.elemental = Some((id, values)),
            MapEvent::ElementalChanged { kind, value } => {
                if let Some((_, values)) = &mut game.elemental
                    && let Some(slot) = kind.checked_sub(net::MERCENARY_HP).and_then(|at| values.get_mut(usize::from(at)))
                {
                    *slot = value;
                }
            }
            MapEvent::MercenarySkills(list) => game.mercenary_skills = list,
            MapEvent::MercenaryChanged { kind, value } => {
                if let Some(hired) = &mut game.mercenary {
                    hired.set(kind, value);
                }
            }
            MapEvent::Message { id, value, skill, color } => {
                // The contract is over, or the mercenary died: it's gone.
                if [mercenary::EXPIRED, mercenary::DIED].contains(&id) {
                    game.forget_mercenary(true);
                }
                let template = self.messages.get(id).map_or("", String::as_str);
                let text = match (value, skill) {
                    (Some(value), _) => messages::fill(template, &[&value]),
                    (_, Some(skill)) => format!("[{}] {template}", self.skill_info.get(&skill).map_or_else(|| skill.to_string(), |info| info.name.clone())),
                    _ => template.to_owned(),
                };
                // ponytail: plain ones in red, as nearly all of them are refusals; calibrate against the real client.
                game.chat_line(text, color.unwrap_or(if skill.is_some() { CHAT_SKILL_MESSAGE } else { CHAT_ERROR }));
            }
            MapEvent::ActionFailure(kind) => {
                let id = ACTION_FAILURES.get(usize::from(kind)).copied().unwrap_or(ACTION_FAILURES[0]);
                game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), if kind == 3 { CHAT_ITEM } else { CHAT_ERROR });
            }
            // What an NPC's script plays: a sound from where the unit stands, music until the map changes.
            // ponytail: a repeated sound (act 1) plays once and a stop (act 2) is let go; rathena notes neither works.
            MapEvent::Sound { file, act: 0 | 1, unit } => {
                let me = game.player.cell;
                let at = if unit == game.account_id { Some(me) } else { game.units.get(&unit).map(|actor| actor.cell) };
                let distance = at.map_or(0.0, |[x, y]| f32::hypot((x - me[0]) as f32, (y - me[1]) as f32));
                if let Some(audio) = &mut self.audio {
                    audio.play(grf, &file, heard(distance));
                }
            }
            MapEvent::Sound { .. } => {}
            MapEvent::NpcMusic(file) => {
                if let Some(audio) = &mut self.audio {
                    audio.play_music(Some(&sound::music_path(&file)));
                }
            }
            MapEvent::MemoResult(result) => {
                let id = MEMO_RESULTS.get(usize::from(result)).copied().unwrap_or(MEMO_RESULTS[2]);
                game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), if result == 0 { CHAT_ITEM } else { CHAT_ERROR });
            }
            MapEvent::MapForbids(kind) => {
                if let Some(&id) = MAP_FORBIDS.get(usize::from(kind)) {
                    game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ERROR);
                }
            }
            // Vending: setting up, our shop, others' signs and goods.
            MapEvent::VendSetup(most) => {
                let mut setup = vend::Setup::new(most);
                // Dev switch RUSTNAROK_CLIENT_VEND=<name>:<price>: everything in the cart at that price, and open.
                if let Some((name, price)) = std::env::var("RUSTNAROK_CLIENT_VEND")
                    .ok()
                    .and_then(|vend| vend.split_once(':').map(|(name, price)| (name.to_owned(), price.parse().unwrap_or(1))))
                {
                    setup.type_text(&name);
                    for item in game.cart.items.iter().take(usize::from(most)) {
                        setup.prices.insert(item.index, price);
                    }
                    let _ = game.session.open_shop(&name, true, &setup.goods(&game.cart.items));
                }
                game.vend_setup = Some(setup);
            }
            // Refused for a zero price (2); other refusals the server explains itself.
            MapEvent::VendOpened(2) => game.chat_line(self.messages.get(VEND_FAILED).cloned().unwrap_or_default(), CHAT_ERROR),
            MapEvent::VendOpened(_) => {}
            MapEvent::MyShop(goods) => {
                let name = game.vend_setup.take().map(|setup| setup.name).unwrap_or_default();
                game.signs.insert(game.account_id, chatroom::Sign::shop(game.account_id, name.clone()));
                game.my_shop = Some(vend::MyShop::new(name, goods));
            }
            MapEvent::VendSold { index, amount, .. } => {
                let Some(shop) = &mut game.my_shop else { return };
                let name = shop
                    .goods
                    .iter()
                    .find(|good| good.index == index)
                    .map(|good| inventory::item_name(&self.items, good.item.id, good.item.identified, good.item.refine));
                shop.sold(index, amount);
                let sold_out = shop.goods.is_empty();
                if let Some(name) = name {
                    game.chat_line(rustnarok_client::messages::fill(self.messages.get(VEND_SOLD).map_or("", String::as_str), &[&name, &amount]), CHAT_ITEM);
                }
                if sold_out {
                    game.chat_line(self.messages.get(VEND_SOLD_OUT).cloned().unwrap_or_default(), CHAT_ITEM);
                }
            }
            MapEvent::VendorSign { id, title } => {
                game.signs.insert(id, chatroom::Sign::shop(id, title));
            }
            MapEvent::VendorGone { id } => {
                game.signs.remove(&id);
            }
            MapEvent::VendorGoods { vendor, shop: shop_id, goods } => {
                // A store opened from the catalog may stand out of sight: its name as the search found it.
                let name = game.signs.get(&vendor).map(|sign| sign.title.clone()).unwrap_or_else(|| {
                    game.catalog
                        .as_ref()
                        .and_then(|catalog| catalog.rows.iter().find(|row| row.account == vendor))
                        .map(|row| row.shop.clone())
                        .unwrap_or_default()
                });
                let offers = goods
                    .into_iter()
                    .map(|good| shop::Offer {
                        item: good.item.id,
                        price: good.price,
                        index: good.index,
                        have: good.item.amount,
                        kind: good.item.kind,
                        identified: good.item.identified,
                        refine: good.item.refine,
                        ..Default::default()
                    })
                    .collect();
                let mut store = shop::Shop::new(false, offers);
                store.vendor = Some((vendor, shop_id, name));
                game.shop = Some(store);
                game.auto_deal();
            }
            MapEvent::VendBought { result, .. } => {
                let id = match result {
                    0 => None,
                    1 => Some(55),
                    2 => Some(56),
                    4 => Some(VEND_SOLD_OUT),
                    _ => Some(57),
                };
                if let Some(id) = id {
                    game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ERROR);
                    game.shop = None;
                }
            }
            MapEvent::OpenBuyingStore { slots } => {
                game.buying_setup = Some(buying::Setup::new(slots));
            }
            MapEvent::BuyingStoreOpenFailed { result, .. } => {
                let id = match result {
                    1 => Some(1741),
                    2 => Some(1742),
                    8 => Some(1797),
                    _ => None,
                };
                if let Some(id) = id {
                    game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ERROR);
                }
            }
            MapEvent::MyBuyingStore { limit, items, .. } => {
                let name = game.buying_setup.take().map(|setup| setup.name).unwrap_or_default();
                game.signs.insert(game.account_id, chatroom::Sign::buying_store(game.account_id, name.clone()));
                game.my_buying_store = Some(buying::MyStore::new(name, limit, items));
            }
            MapEvent::BuyingStoreUpdate { item_id, amount, zeny, limit_left, .. } => {
                if let Some(store) = &mut game.my_buying_store {
                    store.bought(item_id, amount, limit_left);
                }
                // ponytail: 1736 reads like the owner's line for a purchase; which line the official client prints isn't knowable from rathena.
                let name = inventory::item_name(&self.items, item_id, true, 0);
                let paid = shop::group(u64::from(zeny));
                let line = rustnarok_client::messages::fill(self.messages.get(1736).map_or("", String::as_str), &[&name, &paid, &amount]);
                game.chat_line(line, CHAT_ITEM);
            }
            MapEvent::BuyingStoreBuyerFailed { result } => {
                game.my_buying_store = None;
                game.signs.remove(&game.account_id);
                let id = match result {
                    3 => Some(1743),
                    4 => Some(1744),
                    _ => None,
                };
                if let Some(id) = id {
                    game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ITEM);
                }
            }
            MapEvent::BuyingStoreSign { owner, title } => {
                game.signs.insert(owner, chatroom::Sign::buying_store(owner, title));
            }
            MapEvent::BuyingStoreSignGone { owner } => {
                game.signs.remove(&owner);
            }
            MapEvent::BuyingStoreItems { owner, store_id, limit, items } => {
                game.shop = Some(shop::Shop::new_buying_store(owner, store_id, limit, &items, &game.inventory.items));
            }
            MapEvent::BuyingStoreSold { index, amount, price } => {
                let name = game.inventory.get(index).map(|item| inventory::item_name(&self.items, item.id, item.identified, item.refine)).unwrap_or_default();
                game.inventory.remove(index, amount);
                let total = u64::from(price) * u64::from(amount);
                let line = rustnarok_client::messages::fill(self.messages.get(1746).map_or("", String::as_str), &[&name, &amount, &total]);
                game.chat_line(line, CHAT_ITEM);
                // ponytail: whether the official client keeps the seller's window open isn't knowable.
                game.shop = None;
            }
            MapEvent::BuyingStoreSellerFailed { result, item_id } => {
                let line = match result {
                    5 => self.messages.get(57).cloned(),
                    6 => {
                        let name = inventory::item_name(&self.items, item_id, true, 0);
                        Some(rustnarok_client::messages::fill(self.messages.get(1747).map_or("", String::as_str), &[&name]))
                    }
                    7 => self.messages.get(1745).cloned(),
                    _ => self.messages.get(57).cloned(),
                };
                if let Some(line) = line {
                    game.chat_line(line, CHAT_ERROR);
                }
                // ponytail: whether the official client keeps the seller's window open isn't knowable.
                game.shop = None;
            }
            MapEvent::CartCount { most, weight, max_weight, .. } => game.cart.cart = Some((most, [weight, max_weight])),
            MapEvent::CartAdded(item) => game.cart.add(item),
            MapEvent::CartRemoved { index, amount } => game.cart.remove(index, amount.min(u32::from(u16::MAX)) as u16),
            MapEvent::StorageCount { amount, max } => {
                game.storage.count = [amount, max];
                game.auto_store();
            }
            MapEvent::StorageAdded(mut item) => {
                // Where equipment goes isn't sent with it; the inventory still holds it.
                if let Some(carried) = game.inventory.items.iter().find(|carried| carried.id == item.id) {
                    item.location = carried.location;
                }
                game.storage.add(item);
            }
            MapEvent::StorageRemoved { index, amount } => game.storage.remove(index, amount),
            MapEvent::StorageClosed => game.storage.close(),
            // Going back to character select: the map session closes and the char server lists the characters again.
            MapEvent::RestartAck(1) => {
                let Some(account) = self.account.clone() else { return };
                let proxy = self.proxy.clone();
                self.scene = Scene::Login;
                self.played_slot = self.playing.take().map(|character| usize::from(character.slot));
                self.returned = true;
                self.play_music(None);
                match CharSession::connect(&account.char_servers[self.char_server.min(account.char_servers.len() - 1)], &account, move |event| {
                    let _ = proxy.send_event(NetEvent::Char(event));
                }) {
                    Ok(session) => {
                        self.char_session = Some(session);
                        self.wait();
                    }
                    Err(_) => self.message("서버에 접속할 수 없습니다.", Scene::Login),
                }
            }
            MapEvent::RestartAck(_) | MapEvent::QuitAck(1..) => game.chat_line(self.messages.get(LOGOUT_WAIT).cloned().unwrap_or_default(), CHAT_ERROR),
            MapEvent::QuitAck(_) => self.quitting = true,
            MapEvent::Emotion { id, kind } => {
                if let Some(action) = emotion::sprite_action(kind) {
                    game.emotions.insert((id != game.account_id).then_some(id), (action, Instant::now()));
                }
            }
            MapEvent::Revived { id } => {
                if id == game.account_id {
                    game.escape = None;
                    game.player.revive();
                } else if let Some(actor) = game.units.get_mut(&id) {
                    actor.revive();
                }
            }
            // Chat rooms: signs over heads, the one we're in.
            MapEvent::ChatRoom { owner, id, limit, users, kind, title } => {
                let sign = chatroom::Sign { id, limit, users, kind, title };
                if let Some(room) = game.room.as_mut().filter(|room| room.sign.id == id || owner == game.account_id) {
                    room.sign = chatroom::Sign { id: room.sign.id.max(id), ..sign.clone() };
                }
                if owner != game.account_id {
                    game.signs.insert(owner, sign);
                }
            }
            MapEvent::ChatRoomGone { id } => game.signs.retain(|_, sign| sign.id != id),
            MapEvent::ChatRoomMade(result) => {
                let message = |id: usize| self.messages.get(id).cloned().unwrap_or_default();
                match result {
                    0 => {
                        if let Some(make) = game.room_make.take() {
                            let sign =
                                chatroom::Sign { id: 0, limit: make.limit(), users: 1, kind: u8::from(make.public), title: make.title.trim().to_owned() };
                            game.room = Some(chatroom::Room::new(sign, vec![(game.me.name.clone(), true)]));
                        }
                        game.chat_line(message(64), CHAT_ITEM);
                    }
                    1 => game.chat_line(message(65), CHAT_ERROR),
                    _ => game.chat_line(message(66), CHAT_ERROR),
                }
            }
            MapEvent::ChatRoomRefused(result) => {
                let id = match result {
                    0 => Some(67),
                    1 => Some(7),
                    2 => Some(68),
                    4 => Some(55),
                    5 => Some(432),
                    6 => Some(433),
                    7 => Some(434),
                    _ => None,
                };
                if let Some(id) = id {
                    game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ERROR);
                }
            }
            MapEvent::ChatRoomEntered { id, members } => {
                let sign = game.signs.values().find(|sign| sign.id == id).cloned().unwrap_or(chatroom::Sign {
                    id,
                    limit: 0,
                    users: members.len() as u16,
                    kind: 1,
                    title: String::new(),
                });
                game.room = Some(chatroom::Room::new(chatroom::Sign { users: members.len() as u16, ..sign }, members));
            }
            MapEvent::ChatRoomJoined { users, name } => {
                if let Some(room) = &mut game.room {
                    room.sign.users = users;
                    room.lines.push((rustnarok_client::messages::fill(self.messages.get(179).map_or("", String::as_str), &[&name]), chatroom::JOIN_LINE));
                    room.members.push((name, false));
                }
            }
            MapEvent::ChatRoomLeft { users, name, kicked } => {
                if name == game.me.name {
                    game.room = None;
                    if kicked {
                        game.chat_line(self.messages.get(68).cloned().unwrap_or_default(), CHAT_ERROR);
                    }
                } else if let Some(room) = &mut game.room {
                    room.sign.users = users;
                    room.members.retain(|(member, _)| *member != name);
                    let line = rustnarok_client::messages::fill(self.messages.get(if kicked { 181 } else { 180 }).map_or("", String::as_str), &[&name]);
                    room.lines.push((line, chatroom::LEAVE_LINE));
                }
            }
            MapEvent::ChatRoomOwner { owner, name } => {
                for (member, owns) in game.room.iter_mut().flat_map(|room| room.members.iter_mut()) {
                    if *member == name {
                        *owns = owner;
                    }
                }
            }
            // The guild: what it is, its pages, its talk, and who comes and goes.
            MapEvent::Guild { id, name, master, emblem } => {
                let guild = game.guild.get_or_insert_with(guild::Guild::default);
                guild.id = id;
                guild.name = name;
                guild.master = master;
                guild.emblem = emblem;
                game.want_emblem(id, emblem);
            }
            MapEvent::GuildInfo(info) => {
                if let Some(guild) = &mut game.guild {
                    guild.name.clone_from(&info.name);
                    guild.info = Some(info);
                }
            }
            MapEvent::GuildMembers(members) => {
                if let Some(guild) = &mut game.guild {
                    guild.members = members;
                }
            }
            MapEvent::GuildPositions(positions) => {
                if let Some(guild) = &mut game.guild {
                    guild.positions = positions.into_iter().collect();
                }
            }
            MapEvent::GuildPositionChanged(position, title) => {
                if let Some(guild) = &mut game.guild {
                    guild.change_position(position, title);
                }
            }
            MapEvent::GuildMemberPosition { character, position, .. } => {
                if let Some(guild) = &mut game.guild {
                    guild.move_member(character, position);
                }
            }
            // The line under a player's name; a tag not known yet waits for the whole of it.
            MapEvent::PositionTitle { account, title } => {
                if let Some(tag) = game.name_tags.get_mut(&account) {
                    tag.position = title;
                }
            }
            MapEvent::GuildPositionInfo(info) => {
                if let Some(guild) = &mut game.guild {
                    guild.position_info = info;
                }
            }
            MapEvent::GuildSkills { points, skills } => {
                if let Some(guild) = &mut game.guild {
                    guild.skill_points = points;
                    guild.skills = skills;
                }
            }
            MapEvent::GuildExpelled(expelled) => {
                if let Some(guild) = &mut game.guild {
                    guild.expelled = expelled;
                }
            }
            MapEvent::GuildEmblem { guild, version, data } => {
                if let Some(gpu) = gpu {
                    match guild::decode_emblem(&data) {
                        Some(image) => _ = game.emblems.insert(guild, (version, Some(gpu.texture(&image, false)))),
                        None => eprintln!("guild {guild}: emblem {version} isn't a compressed BMP"),
                    }
                }
            }
            MapEvent::GuildEmblemChanged { account, guild, emblem } => {
                // Joining a guild brings this (with its info) rather than ZC_UPDATE_GDID.
                if account == game.account_id {
                    if guild == 0 {
                        game.guild = None;
                        game.guild_window.frame.open = false;
                    } else if game.guild.as_ref().is_none_or(|ours| ours.id != guild) {
                        game.guild = Some(guild::Guild { id: guild, ..Default::default() });
                    }
                    if let Some(ours) = &mut game.guild {
                        ours.emblem = emblem;
                    }
                } else if let Some(actor) = game.units.get_mut(&account) {
                    actor.guild = guild;
                }
                game.want_emblem(guild, emblem);
            }
            MapEvent::NameTag(id, tag) => {
                if id == game.account_id {
                    game.equipment.title = tag.title;
                }
                game.name_tags.insert(id, tag);
            }
            MapEvent::GuildNotice { subject, text } => {
                if !subject.is_empty() {
                    game.chat_line(format!("[{subject}] {text}"), GUILD_CHAT);
                }
                if let Some(guild) = &mut game.guild {
                    guild.notice = (subject, text);
                }
            }
            MapEvent::GuildChat(text) => game.chat_line(text, GUILD_CHAT),
            MapEvent::GuildMemberOnline { account, character, online } => {
                let member =
                    game.guild.as_mut().and_then(|guild| guild.members.iter_mut().find(|member| member.account == account && member.character == character));
                if let Some(member) = member {
                    member.online = online;
                    let name = member.name.clone();
                    let text = rustnarok_client::messages::fill(self.messages.get(GUILD_ONLINE + usize::from(!online)).map_or("", String::as_str), &[&name]);
                    game.chat_line(text, GUILD_CHAT);
                }
            }
            MapEvent::GuildMade(result) => {
                let (id, color) = match result {
                    0 => (374, CHAT_ITEM),
                    1 => (375, CHAT_ERROR),
                    2 => (376, CHAT_ERROR),
                    _ => (405, CHAT_ERROR),
                };
                game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), color);
            }
            MapEvent::GuildInvite { id, name } => {
                // Dev switch: join.
                if std::env::var("RUSTNAROK_CLIENT_GUILD").is_ok_and(|answer| answer.split(',').any(|part| part == "accept")) {
                    let _ = game.session.answer_guild(id, true);
                } else {
                    game.guild_invite = Some((id, name));
                }
            }
            MapEvent::GuildRelations(relations) => {
                if let Some(guild) = &mut game.guild {
                    guild.relations = relations;
                }
            }
            MapEvent::AllyAsked { id, guild } => game.guild_asked = Some(GuildAsked::Ally { id, guild }),
            MapEvent::AllyResult(result) | MapEvent::AntagonistResult(result) => {
                let (line, good) = if matches!(event, MapEvent::AllyResult(_)) { guild::ally_result(result) } else { guild::antagonist_result(result) };
                game.chat_line(self.messages.get(line).cloned().unwrap_or_default(), if good { CHAT_ITEM } else { CHAT_ERROR });
            }
            MapEvent::GuildRelationEnded { id, antagonist } => {
                if let Some(guild) = &mut game.guild {
                    guild.relations.retain(|relation| (relation.id, relation.antagonist) != (id, antagonist));
                }
            }
            // Disbanded: the guild is gone, as if the player had left it.
            MapEvent::GuildDisbanded(result) => {
                let (line, good) = guild::disband_result(result);
                if good {
                    game.guild = None;
                    game.guild_window.frame.open = false;
                }
                game.chat_line(self.messages.get(line).cloned().unwrap_or_default(), if good { CHAT_ITEM } else { CHAT_ERROR });
            }
            MapEvent::GuildInviteResult(result) => {
                let id = 378 + usize::from(result.min(3));
                game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), if result == 2 { CHAT_ITEM } else { CHAT_ERROR });
            }
            MapEvent::GuildLeft { character, reason, expelled } => {
                if character == game.me.id {
                    game.guild = None;
                    game.guild_window.frame.open = false;
                    if !expelled {
                        game.chat_line(self.messages.get(LEFT_GUILD).cloned().unwrap_or_default(), GUILD_CHAT);
                    }
                } else if let Some(guild) = &mut game.guild {
                    let name = guild.members.iter().find(|member| member.character == character).map(|member| member.name.clone());
                    guild.members.retain(|member| member.character != character);
                    // ponytail: the leave lines' color (the guild chat's) is an estimate.
                    if let Some(name) = name.filter(|_| !expelled) {
                        let line = |id: usize, value: &str| messages::fill(self.messages.get(id).map_or("", String::as_str), &[&value]);
                        game.chat_line(line(MEMBER_LEFT, &name), GUILD_CHAT);
                        if !reason.is_empty() {
                            game.chat_line(line(LEFT_BECAUSE, &reason), GUILD_CHAT);
                        }
                    }
                }
            }
            // The clan: its info at login or on joining, how many are online, leaving, and its chat.
            MapEvent::ClanInfo { id, name, master, map, allies, hostile } => {
                let clan = game.clan.get_or_insert_with(clan::Clan::default);
                clan.id = id;
                clan.name = name;
                clan.master = master;
                clan.map = map;
                clan.allies = allies;
                clan.hostile = hostile;
            }
            // rathena sends the info (0x098A) before the first count; a count without a clan is ignored.
            MapEvent::ClanOnlineCount { online, max } => {
                if let Some(clan) = &mut game.clan {
                    clan.online = online;
                    clan.max = max;
                }
            }
            MapEvent::ClanLeft => {
                game.clan = None;
                game.clan_window.frame.open = false;
            }
            MapEvent::ClanChat(text) => game.chat_line(text, CLAN_CHAT),
            // Friends: the list, who's online (said in the chat), asking and answering.
            MapEvent::Friends(friends) => game.friends = friends,
            MapEvent::FriendOnline { account, character, online } => {
                if let Some(friend) = game.friends.iter_mut().find(|friend| friend.account == account && friend.character == character) {
                    friend.online = online;
                    let text =
                        rustnarok_client::messages::fill(self.messages.get(FRIEND_ONLINE + usize::from(!online)).map_or("", String::as_str), &[&friend.name]);
                    game.chat_line(text, CHAT_ITEM);
                }
            }
            MapEvent::FriendAsked { account, character, name } => {
                // Dev switch: say yes.
                if std::env::var("RUSTNAROK_CLIENT_FRIEND").is_ok_and(|answer| answer == "accept") {
                    let _ = game.session.answer_friend(account, character, true);
                } else {
                    game.friend_asked = Some((account, character, name));
                }
            }
            MapEvent::FriendAdded { result, friend } => {
                let id = match result {
                    0 => 821,
                    1 => 822,
                    2 => 819,
                    _ => 820,
                };
                let text = rustnarok_client::messages::fill(self.messages.get(id).map_or("", String::as_str), &[&friend.name]);
                game.chat_line(text, if result == 0 { CHAT_ITEM } else { CHAT_ERROR });
                if result == 0 && game.friends.iter().all(|known| known.character != friend.character) {
                    game.friends.push(friend);
                }
            }
            MapEvent::FriendRemoved { account, character } => {
                game.friends.retain(|friend| !(friend.account == account && friend.character == character));
                game.party_window.picked = None;
            }
            // The party: kept member by member, as the window and minimap show it.
            MapEvent::Party { name, members } => {
                let mut party = game.party.take().unwrap_or_default();
                let old = std::mem::take(&mut party.members);
                party.name = name;
                party.members = members
                    .into_iter()
                    .map(|info| party::Member {
                        info: info.clone(),
                        ..old.iter().find(|member| member.info.account == info.account).cloned().unwrap_or_default()
                    })
                    .collect();
                game.party = Some(party);
                game.member_windows.retain(game.party.as_ref());
            }
            MapEvent::PartyMember { party: name, member, cell, .. } => {
                let party = game.party.get_or_insert_with(party::Party::default);
                party.name = name;
                let account = member.account;
                party.upsert(member);
                if let Some(member) = party.member(account) {
                    member.cell = Some(cell.map(i32::from));
                }
            }
            MapEvent::PartyLeft { account, result, .. } => {
                let message = |id: usize| self.messages.get(id).cloned().unwrap_or_default();
                match result {
                    // Leaving isn't allowed here.
                    2 => game.chat_line(message(1872), CHAT_ERROR),
                    3 => game.chat_line(message(1873), CHAT_ERROR),
                    _ if account == game.account_id => {
                        game.party = None;
                        game.member_windows.retain(None);
                        game.chat_line(message(PARTY_LEFT), CHAT_ITEM);
                    }
                    _ => {
                        if let Some(party) = &mut game.party {
                            party.members.retain(|member| member.info.account != account);
                        }
                        game.member_windows.retain(game.party.as_ref());
                        game.unit_hp.remove(&account);
                    }
                }
            }
            MapEvent::PartyMade(result) => {
                let (id, color) = match result {
                    0 => (PARTY_MADE, CHAT_ITEM),
                    1 => (78, CHAT_ERROR),
                    2 => (79, CHAT_ERROR),
                    _ => (1387, CHAT_ERROR),
                };
                game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), color);
            }
            MapEvent::PartyInvite { party, name } => {
                game.party_invite = Some((party, name));
                // Dev switch: join.
                if std::env::var("RUSTNAROK_CLIENT_PARTY").is_ok_and(|order| order.split(',').any(|part| part == "accept")) {
                    game.party_invite = None;
                    let _ = game.session.answer_party(party, true);
                }
            }
            // How an invitation went, in the client's words.
            MapEvent::PartyInviteResult { name, result } => {
                let id = match result {
                    0 => 80,
                    1 => 81,
                    2 => 82,
                    3 => 83,
                    4 => 608,
                    5 => 1324,
                    7 => 71,
                    8 => 1388,
                    10 => 3027,
                    _ => 1871,
                };
                let text = rustnarok_client::messages::fill(self.messages.get(id).map_or("", String::as_str), &[&name]);
                game.chat_line(text, if result == 2 { CHAT_ITEM } else { CHAT_ERROR });
            }
            // A map's property: its notices, then the "ready" the server waits for.
            MapEvent::MapProperty { property, flags } => {
                game.map_property = Some((property, flags));
                // Allies are spared on siege maps: their list comes with the guild's first page.
                if versus::siege(flags) && game.guild.is_some() {
                    let _ = game.session.guild_page(0);
                }
                for id in arrival_notices(flags, game.me.job) {
                    game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), MAP_NOTICE);
                }
                let _ = game.session.ready_to_play();
            }
            MapEvent::MapType(kind) => game.map_type = Some(kind),
            // A battleground: teammates' HP bars as party members', their dots, everyone's camp, the score, the team chat.
            MapEvent::BattleHp { account, hp, max } => {
                if account != game.account_id {
                    game.unit_hp.insert(account, (hp, max));
                }
            }
            MapEvent::BattlePosition { account, cell } => game.battle.place(account, cell),
            MapEvent::BattleCamp { account, camp } => {
                game.battle.camps.insert(account, camp);
                if game.camp_looks.is_none()
                    && let Some(gpu) = gpu
                {
                    let solid = |[r, g, b]: [u8; 3]| gpu.solid([r, g, b, 255]);
                    game.camp_looks = Some((solid(battleground::OUR_COLOR), [solid(battleground::OUR_COLOR), solid(battleground::THEIR_COLOR)]));
                }
            }
            MapEvent::BattleScore(score) => game.battle.score = Some(score),
            // The queue: applied (a line by result; 0 has rathena's own), how many wait, left, the battle ready.
            MapEvent::BattleQueue { result, name } => {
                if result == 1 {
                    game.queued = Some((name, None));
                }
                if let Some(id) = battle_queue::apply_result(result) {
                    game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), if result == 1 { CHAT_ITEM } else { CHAT_ERROR });
                }
            }
            MapEvent::QueueNumber { name, number } => game.queued = Some((name, Some(number))),
            MapEvent::QueueLeft { result, .. } => {
                if result == 1 {
                    game.queued = None;
                }
                let id = battle_queue::cancel_result(result);
                game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ITEM);
            }
            MapEvent::QueueReady { name } => {
                game.queue_asked = Some(QueueAsked::Lobby(name, Instant::now() + Duration::from_secs(battle_queue::LOBBY_SECONDS)));
            }
            // ponytail: no sound for it; which one rathena's "gong" is isn't knowable.
            MapEvent::QueueAccepted { result: 1, .. } => {
                game.chat_line(self.messages.get(battle_queue::LOBBY_ACCEPTED).cloned().unwrap_or_default(), CHAT_ITEM);
            }
            MapEvent::QueueAccepted { .. } => {}
            MapEvent::QueueReset => {
                game.queued = None;
                if matches!(game.queue_asked, Some(QueueAsked::Lobby(..))) {
                    game.queue_asked = None;
                }
            }
            MapEvent::BattleChat { text, .. } => game.chat_line(text, battleground::CHAT_COLOR),
            MapEvent::DressRoom => game.open_dressroom(),
            // The board opens (or shows the new ranking); the names it lacks are asked for once each.
            MapEvent::Ranking { kind, ids, points, mine } => {
                let frame = game.ranking.as_ref().map(|board| board.frame.position);
                let mut board = ranking::Board::new(kind, ids, points, mine);
                if let Some(position) = frame {
                    board.frame.position = position;
                }
                for id in ranking::missing_names(&ids, &game.char_names) {
                    if game.names_asked.insert(id) {
                        let _ = game.session.ask_char_name(id);
                    }
                }
                game.ranking = Some(board);
            }
            MapEvent::CharName { id, name } => {
                game.char_names.insert(id, name);
            }
            // ponytail: the point and mission lines' color is an estimate.
            MapEvent::RankingPoints { kind, added, total } => {
                let id = match kind {
                    0 => RANK_POINTS_BLACKSMITH,
                    1 => RANK_POINTS_ALCHEMIST,
                    2 => RANK_POINTS_TAEKWON,
                    _ => RANK_POINTS_SLAYER,
                };
                let text = rustnarok_client::messages::fill(self.messages.get(id).map_or("", String::as_str), &[&added, &total]);
                game.chat_line(text, CHAT_ITEM);
            }
            MapEvent::StarSkill { name, count, kind: net::STAR_MISSION, .. } => {
                let text = rustnarok_client::messages::fill(self.messages.get(MISSION_TARGET).map_or("", String::as_str), &[&name, &count]);
                game.chat_line(text, CHAT_ITEM);
            }
            // The Star Gladiator's Feeling and Hatred aren't shown yet.
            MapEvent::StarSkill { .. } => {}
            // Sent to the area: only our own counts. Rank 0 or no players hides the counter; a hidden player's (all
            // ones) shows nothing.
            // Only on a map that counts.
            MapEvent::PvpRank { .. } if game.map_property.is_none_or(|(_, flags)| flags & net::MAP_COUNT_PK == 0) => {}
            MapEvent::PvpRank { account, rank, players } => {
                // ponytail: what EF_TOPRANK looks like isn't in this data or rathena's sources ("PvP circle"); the effect
                // plays (nothing until effects.txt has it) and who carries it is kept.
                if (1..=10).contains(&rank) {
                    if game.top_ten.insert(account)
                        && let Some(gpu) = gpu
                    {
                        game.play_effect(gpu, grf, table, TOP_RANK_EFFECT, account);
                    }
                } else {
                    game.top_ten.remove(&account);
                }
                if account == game.account_id {
                    game.pvp_rank = (rank != 0 && rank != u32::MAX && players != 0).then_some((rank, players));
                    if rank != game.last_rank {
                        if let Some(id) = rank_line(game.last_rank, rank) {
                            let text = rustnarok_client::messages::fill(self.messages.get(id).map_or("", String::as_str), &[&game.me.name, &rank]);
                            game.chat_line(text, RANK_LINE);
                        }
                        if let Some(audio) = &mut self.audio {
                            audio.play(grf, "effect\\number_change.wav", 1.0);
                        }
                    }
                    game.first_since = match (rank, game.first_since) {
                        (1, Some(since)) => Some(since),
                        (1, None) => Some(control::now()),
                        _ => None,
                    };
                    game.last_rank = rank;
                }
            }
            MapEvent::PartyChat { account, text } => game.chat_line(text, if account == game.account_id { PARTY_CHAT_OURS } else { PARTY_CHAT }),
            MapEvent::PartyPosition { account, cell } => {
                if let Some(member) = game.party.as_mut().and_then(|party| party.member(account)) {
                    member.cell = cell.map(|cell| cell.map(i32::from));
                }
            }
            // ponytail: the line and its color (the party chat's) for a member's pickup are estimates.
            MapEvent::PartyPickup { account, item, identified, refine } => {
                let member = game.party.as_mut().and_then(|party| party.member(account)).map(|member| member.info.name.clone());
                let name = inventory::item_name(&self.items, item, identified, refine);
                let text = rustnarok_client::messages::fill(self.messages.get(PARTY_PICKUP).map_or("", String::as_str), &[&member.unwrap_or_default(), &name]);
                game.chat_line(text, PARTY_CHAT);
            }
            // It comes at every map load: a line only when it changes.
            MapEvent::PartyRefusing(refusing) => {
                if game.refusing_party.is_some_and(|known| known != refusing) {
                    game.chat_line(self.messages.get(if refusing { PARTY_REFUSING } else { PARTY_ALLOWING }).cloned().unwrap_or_default(), CHAT_ITEM);
                }
                game.refusing_party = Some(refusing);
            }
            // Members' HP: in the window, and as a bar under them.
            MapEvent::PartyHp { account, hp, max } => {
                if let Some(member) = game.party.as_mut().and_then(|party| party.member(account)) {
                    member.hp = Some([hp, max]);
                }
                if account != game.account_id {
                    game.unit_hp.insert(account, (hp, max));
                }
            }
            // EXP 2: an even share was refused (the levels are too far apart); the item settings still change.
            MapEvent::PartyOptions { exp, pickup, share } => {
                if let Some(party) = &mut game.party {
                    if exp != 2 {
                        party.exp_shared = exp == 1;
                    }
                    party.pickup_shared = pickup;
                    party.item_shared = share;
                }
                if exp == 2 {
                    // ponytail: which line the official client prints for a refused share is an estimate.
                    game.chat_line(self.messages.get(PARTY_SETUP_REFUSED).cloned().unwrap_or_default(), CHAT_ERROR);
                }
            }
            MapEvent::PartyLeader { old, new } => {
                for member in game.party.iter_mut().flat_map(|party| party.members.iter_mut()) {
                    if member.info.account == old || member.info.account == new {
                        member.info.leader = member.info.account == new;
                    }
                }
            }
            MapEvent::PartyMemberInfo { account, job, level } => {
                if let Some(member) = game.party.as_mut().and_then(|party| party.member(account)) {
                    member.info.job = job;
                    member.info.level = level;
                }
            }
            MapEvent::PartyDead { account, dead } => {
                if let Some(member) = game.party.as_mut().and_then(|party| party.member(account)) {
                    member.dead = dead;
                }
            }
            MapEvent::TradeAsked { name, id, level } => {
                game.trade_with = name.clone();
                game.trade_asked = Some((name, id, level));
                // Dev switch: take it.
                if std::env::var("RUSTNAROK_CLIENT_TRADE").is_ok_and(|trade| trade == "accept") {
                    game.trade_asked = None;
                    let _ = game.session.answer_trade(true);
                }
            }
            // The deal is on: the window opens with the inventory beside it.
            MapEvent::TradeAnswer { result: 3, id, level } => {
                game.trade_asked = None;
                game.trade = Some(trade::Trade::new(format!("{}  Lv{level} ({})", game.trade_with, trade::id_code(id))));
                game.inventory.frame.open = true;
                game.raise(Panel::Inventory);
                game.auto_trade();
            }
            MapEvent::TradeAnswer { result, .. } => {
                if result <= 4 {
                    game.chat_line(self.messages.get(TRADE_ANSWERS + usize::from(result)).cloned().unwrap_or_default(), CHAT_ERROR);
                }
            }
            MapEvent::TradeItem(item) => {
                if let Some(trade) = &mut game.trade {
                    trade.items[1].push(item);
                }
            }
            MapEvent::TradeZeny(zeny) => {
                if let Some(trade) = &mut game.trade {
                    trade.their_zeny = zeny;
                }
            }
            // Ours went in: it leaves the inventory until the deal ends (the server gives it back if cancelled).
            MapEvent::TradeAdded { index, result } => {
                if let Some(trade) = &mut game.trade
                    && let Some(amount) = trade.pending.remove(&index)
                    && result == 0
                    && let Some(item) = game.inventory.get(index)
                {
                    trade.items[0].push(net::InventoryItem { amount: amount as u16, ..item.clone() });
                    game.inventory.remove(index, amount as u16);
                }
                let id = match result {
                    1 => Some(TRADE_HEAVY),
                    2 => Some(TRADE_CANCELLED),
                    3 => Some(1938),
                    4 => Some(1939),
                    _ => None,
                };
                if let Some(id) = id {
                    game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ERROR);
                }
            }
            MapEvent::TradeLocked { partner } => {
                if let Some(trade) = &mut game.trade {
                    trade.locked[usize::from(partner)] = true;
                    // Dev switch: exchange once both are in.
                    if trade.locked == [true; 2] && std::env::var_os("RUSTNAROK_CLIENT_TRADE_GIVE").is_some() {
                        let _ = game.session.finish_trade();
                    }
                }
            }
            MapEvent::TradeCancelled => {
                game.trade = None;
                game.chat_line(self.messages.get(TRADE_CANCELLED).cloned().unwrap_or_default(), CHAT_ERROR);
            }
            MapEvent::TradeDone { ok } => {
                game.trade = None;
                let (id, color) = if ok { (TRADE_DONE, CHAT_ITEM) } else { (TRADE_FAILED, CHAT_ERROR) };
                game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), color);
            }
            // Whispers, yellow both ways (the client's "(From name)" wording). A 1:1 window opens when that sender's box is on.
            MapEvent::Whisper { from, text } => game.receive_whisper(from, text),
            MapEvent::WhisperResult(result) => {
                let (to, text) = game.whispered.pop_front().unwrap_or_default();
                let line = match result {
                    0 => format!("(To {to}) : {text}"),
                    1 => self.messages.get(WHISPER_FAILED).cloned().unwrap_or_default(),
                    _ => format!("{to}{}", self.messages.get(WHISPER_FAILED + usize::from(result) - 1).cloned().unwrap_or_default()),
                };
                game.chat_line_kind(line, WHISPER_COLOR, chat::Kind::Whisper);
            }
            MapEvent::ItemSkill(skill) => game.cast_item_skill(skill),
            // Eggs to hatch: the pick list; an empty one shows nothing.
            MapEvent::PetEggs(indexes) => {
                let eggs: Vec<_> = indexes.iter().filter_map(|&index| game.inventory.get(index).cloned()).collect();
                if !eggs.is_empty() {
                    game.pick = Some((pick_list::PickList::new(PET_EGGS, eggs), Picking::Hatch));
                }
            }
            MapEvent::PetState { kind: net::PET_OWNED, id, .. } => game.pet_id = Some(id),
            MapEvent::PetState { kind: net::PET_EGGS_DONE, .. } => {
                if matches!(game.pick, Some((_, Picking::Hatch))) {
                    game.pick = None;
                }
            }
            // ponytail: neglect is told when intimacy falls while the pet starves; when the real client tells it isn't known.
            MapEvent::PetState { kind: net::PET_INTIMACY, value, .. } => {
                if game.pet.as_ref().is_some_and(|pet| pet.hunger == 0 && value < i32::from(pet.intimacy)) {
                    game.chat_line(self.messages.get(PET_NEGLECTED).cloned().unwrap_or_default(), CHAT_ERROR);
                }
                if let Some(pet) = &mut game.pet {
                    pet.intimacy = value as u16;
                }
            }
            MapEvent::PetState { kind: net::PET_HUNGER, value, .. } => {
                if let Some(pet) = &mut game.pet {
                    pet.hunger = value as u16;
                }
            }
            // An accessory on or off: the pet moves by another ACT.
            MapEvent::PetState { kind: net::PET_ACCESSORY, id, value } => {
                if let Some(pet) = game.pet.as_mut().filter(|_| game.pet_id == Some(id)) {
                    pet.accessory = value as u16;
                }
                if let Some(actor) = game.units.get_mut(&id)
                    && let Some(name) = job_names.get(&actor.job)
                {
                    actor.sprites = pet_sprites(grf, name, pets.tables.accessory_acts.get(&(value as u16)));
                }
            }
            // ponytail: a performance plays action 4 + its number once (pets' sprites have more actions than plain
            // monsters'); which action the real client plays isn't known. Our pet says something about it.
            MapEvent::PetState { kind: net::PET_PERFORMANCE, id, value } => {
                let action = 4 + value.clamp(0, 4) as usize;
                if let Some(actor) = game.units.get_mut(&id)
                    && let Some(duration) =
                        actor.sprites.as_ref().filter(|sprites| sprites.frames(action * 8) > 0).and_then(|sprites| sprites.action_ms(action, actor.direction))
                {
                    actor.play(action, Instant::now(), duration, false);
                }
                if let Some(pet) = game.pet.as_ref().filter(|_| game.pet_id == Some(id)) {
                    let act = if value == 4 { pet::ACT_PERFORMED_SPECIAL } else { pet::ACT_PERFORMED + value.clamp(1, 3) as u8 - 1 };
                    let _ = game.session.pet_act(&net::PetAct::Talk { monster: pet.monster, act, hunger: pet::hunger_band(pet.hunger) });
                }
            }
            MapEvent::PetState { .. } => {}
            MapEvent::PetInfo(pet) => {
                // Its picture, loaded when it's another monster's.
                if game.pet_window.picture.as_ref().is_none_or(|(monster, _)| *monster != pet.monster) {
                    let path = pets.tables.illustrations.get(&pet.monster).map(|name| format!("{UI}illust\\{name}"));
                    let picture = gpu.zip(path).and_then(|(gpu, path)| gpu.try_load_texture(grf, &path, false).ok());
                    game.pet_window.picture = Some((pet.monster, picture));
                }
                game.pet = Some(pet);
            }
            MapEvent::PetFed { ok: false, food } => {
                let name = inventory::item_name(&self.items, food, true, 0);
                game.chat_line(messages::fill(self.messages.get(PET_NO_FOOD).map_or("", String::as_str), &[&name]), CHAT_ERROR);
            }
            // Fed (its hunger came just before): it says so.
            MapEvent::PetFed { ok: true, .. } => {
                if let Some(pet) = &game.pet {
                    let _ = game.session.pet_act(&net::PetAct::Talk { monster: pet.monster, act: pet::ACT_FEEDING, hunger: pet::hunger_band(pet.hunger) });
                }
            }
            MapEvent::PetAct { id, act: net::PetAct::Emotion(kind) } => {
                if let Some(action) = emotion::sprite_action(kind) {
                    game.emotions.insert(Some(id), (action, Instant::now()));
                }
            }
            MapEvent::PetAct { id, act: net::PetAct::Talk { monster, act, hunger } } => {
                if let Some(line) = pets.line(monster, act, hunger) {
                    game.say(Some(id), line, CHAT_WHITE);
                }
            }
            MapEvent::UnitName { id, name } => {
                if let Some(actor) = game.units.get_mut(&id) {
                    actor.name = name;
                }
            }
            // A taming item: aim at the monster to tame.
            MapEvent::CaptureStart => {
                game.targeting = None;
                game.capturing = true;
            }
            MapEvent::CaptureResult(caught) => {
                let action = if caught { SLOT_CAUGHT } else { SLOT_MISSED };
                game.slot = Some((action, Instant::now()));
                let sound = game.slot_sprite.as_ref().and_then(|sprite| sprite.body_frame(action, Timing::Span(0.0))?.1.map(str::to_owned));
                if let (Some(audio), Some(sound)) = (&mut self.audio, sound) {
                    audio.play(grf, &sound, 1.0);
                }
                let (line, color) = if caught { (TAMED, CHAT_ITEM) } else { (NOT_TAMED, CHAT_ERROR) };
                game.chat_line(self.messages.get(line).cloned().unwrap_or_default(), color);
            }
            // Achievements: the whole list, or one changed; a newly done one says so.
            MapEvent::Achievements { totals, list } => {
                game.achievement_totals = totals;
                game.achievements = list.into_iter().map(|achievement| (achievement.id, achievement)).collect();
                game.equipment.titles = earned_titles(&game.achievements, &self.achievement_info, &self.title_names);
            }
            MapEvent::AchievementUpdate { totals, achievement } => {
                game.achievement_totals = totals;
                if let Some(achievement) = achievement {
                    // ponytail: a chat line when one is done; how the real client tells it isn't known.
                    if achievement.completed && !game.achievements.get(&achievement.id).is_some_and(|known| known.completed) {
                        let info = self.achievement_info.get(&achievement.id);
                        let name = info.map_or_else(|| achievement.id.to_string(), |info| info.title.clone());
                        game.chat_line(messages::fill(self.messages.get(achievement::ACHIEVED).map_or("", String::as_str), &[&name]), CHAT_ITEM);
                    }
                    game.achievements.insert(achievement.id, achievement);
                    game.equipment.titles = earned_titles(&game.achievements, &self.achievement_info, &self.title_names);
                }
            }
            // A claim's result shows in the achievement's update.
            MapEvent::AchievementReward { .. } => {}
            MapEvent::TitleChanged { ok: true, title } => game.equipment.title = title,
            MapEvent::TitleChanged { .. } => {}
            // The Lucky Roulette: opened (asked or by @roulette), its prizes, a spin, a claim.
            MapEvent::RouletteOpen { prize, bonus, points } => {
                let window = game.roulette.get_or_insert_with(roulette::Roulette::new);
                (window.prize, window.bonus, window.points) = (prize, bonus, points);
                if window.prizes.is_empty() {
                    let _ = game.session.roulette(net::RouletteRequest::Prizes);
                }
            }
            MapEvent::RoulettePrizes(prizes) => {
                if let Some(window) = &mut game.roulette {
                    window.prizes = prizes;
                }
            }
            // ponytail: the wheel's sound plays as it starts; when the real client plays it isn't known.
            MapEvent::RouletteSpun { result, row, column, bonus, points } => {
                if let Some(window) = &mut game.roulette {
                    (window.bonus, window.points) = (bonus, points);
                    if result == 0 || result == 3 {
                        window.spun(row, column, control::now());
                        if let Some(audio) = &mut self.audio {
                            audio.play(grf, "roulette.wav", 1.0);
                        }
                    }
                }
                if let Some(id) = roulette::spin_message(result) {
                    game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ERROR);
                }
            }
            MapEvent::RouletteClaimed { result, bonus } => {
                if let Some(window) = &mut game.roulette {
                    window.bonus = bonus;
                    if result == 0 {
                        window.prize = None;
                    }
                }
                if let Some(id) = roulette::claim_message(result) {
                    game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ERROR);
                }
            }
            // Sense: the window, named after a unit on screen of that class (the packet has no name).
            // ponytail: where the real client takes the name from isn't knowable.
            MapEvent::MonsterInfo(info) => {
                let name = game.units.values().find(|actor| actor.job == info.class).map(|actor| actor.name.clone()).unwrap_or_default();
                game.monster_window = Some(monster_info::MonsterWindow::new(info, name));
            }
            // The Convex Mirror: none here; the boss found, moving; dead, back in a while.
            // ponytail: the lines in the server lines' yellow.
            MapEvent::BossInfo { kind, cell, hours, minutes, name } => {
                let message = |id: usize| self.messages.get(id).map_or("", String::as_str);
                match kind {
                    0 => game.chat_line(message(NO_BOSS).to_owned(), CHAT_BROADCAST),
                    2 => {
                        game.chat_line(messages::fill(message(BOSS_SHOWN), &[&name]), CHAT_BROADCAST);
                        game.boss = Some((name, cell));
                    }
                    1 => game.boss = Some((name, cell)),
                    _ => {
                        game.boss = None;
                        let line = messages::fill(message(monster_info::boss_line(hours, minutes)), &[&name, &hours, &minutes]);
                        game.chat_line(line, CHAT_BROADCAST);
                    }
                }
            }
            // A Talkie Box went off: its words over it, like speech.
            // ponytail: a box never seen (a hidden trap) puts them in the chat; where the real client shows them isn't known.
            MapEvent::TalkieBox { unit, text } => match game.effects.ground_unit_at(unit) {
                Some(at) => {
                    game.talkie_spots.insert(unit, at);
                    game.say(Some(unit), text, CHAT_WHITE);
                }
                None => game.chat_line(text, CHAT_WHITE),
            },
            MapEvent::IdentifyList(indexes) => {
                let items: Vec<_> = indexes.iter().filter_map(|&index| game.inventory.get(index).cloned()).collect();
                if items.is_empty() {
                    let _ = game.session.identify(None);
                } else {
                    game.pick = Some((pick_list::PickList::new(ITEM_APPRAISAL, items), Picking::Identify));
                }
            }
            // What to make, cook or craft: the pick list with the items by ID.
            MapEvent::MakeList(items) | MapEvent::CookList { items, .. } | MapEvent::ArrowList(items) if items.is_empty() => {
                game.chat_line(self.messages.get(NOTHING_TO_MAKE).cloned().unwrap_or_default(), CHAT_ERROR);
                // An empty making list still waits for an answer.
                let _ = match event_kind {
                    Some(Picking::Make) => game.session.make(0),
                    Some(Picking::Cook(kind)) => game.session.cook(kind, 0),
                    _ => Ok(()),
                };
            }
            MapEvent::MakeList(items) | MapEvent::CookList { items, .. } | MapEvent::ArrowList(items) => {
                let picking = event_kind.unwrap_or(Picking::Arrow);
                let rows = items.into_iter().map(|id| net::InventoryItem { id, identified: true, ..Default::default() }).collect();
                let mut list = pick_list::PickList::new(if matches!(picking, Picking::Arrow) { ARROW_TITLE } else { MAKING_TITLE }, rows);
                list.making = !matches!(picking, Picking::Arrow);
                game.pick = Some((list, picking));
            }
            // Blacksmith services: the target's broken items, the caster's weapons; an empty list opens nothing.
            // ponytail: the results in the chat; whether the real client uses a box isn't known.
            // Skills to pick (Hindsight's spells, Shadow Spell's copies): the pick list with skill rows.
            MapEvent::SpellList(skills) => {
                let rows = skills.into_iter().map(|id| net::InventoryItem { id, identified: true, ..Default::default() }).collect();
                let mut list = pick_list::PickList::new(SPELL_TITLE, rows);
                list.skills = true;
                game.pick = Some((list, Picking::Spell));
            }
            MapEvent::SkillSelect { why, skills } => {
                let rows = skills.into_iter().map(|id| net::InventoryItem { id: u32::from(id), identified: true, ..Default::default() }).collect();
                let mut list = pick_list::PickList::new(SPELL_TITLE, rows);
                list.skills = true;
                game.pick = Some((list, Picking::SkillSelect(why)));
            }
            MapEvent::RepairList(items) | MapEvent::UpgradeList(items) if items.is_empty() => {}
            MapEvent::RepairList(items) => game.pick = Some((pick_list::PickList::new(REPAIR_TITLE, items), Picking::Repair)),
            MapEvent::MergeOpen(indexes) => {
                let stacks = indexes.iter().filter_map(|&index| game.inventory.get(index).cloned()).collect();
                game.pick = Some((pick_list::PickList::merging(stacks), Picking::Merge));
            }
            // Merged: the stack that kept everything has its new amount (the others' deletions came first).
            // ponytail: which line the official client prints for each result isn't knowable.
            MapEvent::Merged { index, amount, result } => {
                let message = |id: usize| self.messages.get(id).cloned().unwrap_or_default();
                if result == 0 {
                    if let Some(stack) = game.inventory.items.iter_mut().find(|item| item.index == index) {
                        stack.amount = amount;
                    }
                    game.chat_line(message(MERGED), CHAT_ITEM);
                    if matches!(game.pick, Some((_, Picking::Merge))) {
                        game.pick = None;
                    }
                } else {
                    game.chat_line(message(MERGE_TOO_MANY), CHAT_ERROR);
                }
            }
            MapEvent::UpgradeList(items) => game.pick = Some((pick_list::PickList::new(UPGRADE_TITLE, items), Picking::Upgrade)),
            MapEvent::Repaired { ok, .. } => {
                let (id, color) = if ok { (REPAIRED, CHAT_ITEM) } else { (NOT_REPAIRED, CHAT_ERROR) };
                game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), color);
            }
            MapEvent::Upgraded { result, item } => {
                let (id, color) = UPGRADE_RESULTS.get(result as usize).copied().unwrap_or(UPGRADE_RESULTS[1]);
                let name = inventory::item_name(&self.items, item, true, 0);
                game.chat_line(messages::fill(self.messages.get(id).map_or("", String::as_str), &[&name]), color);
            }
            MapEvent::Made { result, item } => {
                let name = inventory::item_name(&self.items, item, true, 0);
                let (id, color) = if result % 2 == 0 { (MADE, CHAT_ITEM) } else { (NOT_MADE, CHAT_ERROR) };
                game.chat_line(messages::fill(self.messages.get(id).map_or("", String::as_str), &[&name]), color);
            }
            MapEvent::CardList(indexes) => {
                let items: Vec<_> = indexes.iter().filter_map(|&index| game.inventory.get(index).cloned()).collect();
                if let Some(card) = game.card_asked.take()
                    && !items.is_empty()
                {
                    game.pick = Some((pick_list::PickList::new(INSERT_CARD, items), Picking::Card(card)));
                }
            }
            // rathena takes the card without telling; it goes into the equipment's first empty slot.
            // ponytail: the result in the chat; whether the real client shows it in a box isn't known.
            MapEvent::CardInserted { equip, card, ok } => {
                if ok {
                    let id = game.inventory.get(card).map(|item| item.id);
                    game.inventory.remove(card, 1);
                    if let Some(id) = id
                        && let Some(slot) =
                            game.inventory.items.iter_mut().find(|item| item.index == equip).and_then(|item| item.cards.iter_mut().find(|slot| **slot == 0))
                    {
                        *slot = id;
                    }
                }
                let id = if ok { CARD_DONE } else { CARD_FAILED };
                game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), if ok { CHAT_ITEM } else { CHAT_ERROR });
            }
            MapEvent::Identified { index, ok } => {
                if ok && let Some(item) = game.inventory.items.iter_mut().find(|item| item.index == index) {
                    item.identified = true;
                }
            }
            // Replies to the client's own chat commands.
            MapEvent::UserCount(count) => {
                game.chat_line(rustnarok_client::messages::fill(self.messages.get(USER_COUNT).map_or("", String::as_str), &[&count]), CHAT_ITEM);
            }
            MapEvent::WhisperSetting { allow, result } => {
                let name = game.whisper_asked.pop_front().unwrap_or_default();
                if result == 0 {
                    if allow {
                        game.blocked.remove(&name);
                        if let Some(list) = &mut game.block_list {
                            list.remove(&name);
                        }
                    } else {
                        game.blocked.insert(name.clone());
                    }
                }
                let id = if allow { ALLOW_RESULTS } else { BLOCK_RESULTS } + usize::from(result.min(2));
                game.chat_line(format!("{name}{}", self.messages.get(id).cloned().unwrap_or_default()), if result == 0 { CHAT_ITEM } else { CHAT_ERROR });
            }
            MapEvent::WhisperList(names) => {
                game.blocked = names.iter().cloned().collect();
                let empty = self.messages.get(BLOCKED_NONE).cloned().unwrap_or_default();
                game.block_list = Some(whisper::BlockList::new(names, empty));
                game.sync_esc();
            }
            MapEvent::WhisperAll { allow, result } => {
                let id = if allow { ALLOW_ALL_RESULTS } else { BLOCK_ALL_RESULTS } + usize::from(result != 0);
                game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), if result == 0 { CHAT_ITEM } else { CHAT_ERROR });
            }
            MapEvent::Cutin { image, place } => {
                let path = format!("{UI}illust\\{image}{}", if image.contains('.') { "" } else { ".bmp" });
                game.cutin = gpu.filter(|_| !image.is_empty()).and_then(|gpu| gpu.try_load_texture(grf, &path, false).ok()).map(|texture| (texture, place));
            }
            MapEvent::CatalogOpen { remote, uses } => game.catalog = Some(catalog::Catalog::new(remote, uses)),
            MapEvent::CatalogResults { first, more, uses, rows } => {
                if let Some(catalog) = &mut game.catalog {
                    catalog.page(first, more, uses, rows);
                }
            }
            MapEvent::CatalogFailed(reason) => {
                game.chat_line(self.messages.get(catalog::failure_message(reason)).cloned().unwrap_or_default(), CHAT_ERROR);
            }
            // Where the store picked stands: a yellow mark on the minimap (none: it's gone).
            MapEvent::CatalogMark(cell) => {
                game.marks.retain(|(key, ..)| *key != CATALOG_MARK);
                if let Some(catalog) = &mut game.catalog {
                    catalog.mark = cell;
                }
                if let (Some(cell), Some(gpu)) = (cell, gpu) {
                    game.marks.push((CATALOG_MARK, cell.map(i32::from), gpu.solid([0xFF, 0xFF, 0x00, 255]), None, control::now()));
                }
            }
            MapEvent::Mark { npc, kind, cell, id, color } => {
                game.marks.retain(|(key, ..)| *key != (npc, id));
                if kind != 2
                    && let Some(gpu) = gpu
                {
                    let until = (kind == 0).then(|| control::now() + Duration::from_secs(15));
                    game.marks.push(((npc, id), cell.map(i32::from), gpu.solid([color[0], color[1], color[2], 255]), until, control::now()));
                }
            }
            MapEvent::InventoryExpansion(slots) => game.inventory.expansion = slots,
            MapEvent::Hotkeys { tab, keys, .. } => game.hotkeys.set_tab(tab, keys),
            MapEvent::Skills(skills) => game.skills = skills,
            MapEvent::SkillChanged(changed) => match game.skills.iter_mut().find(|skill| skill.id == changed.id) {
                // ZC_SKILLINFO_UPDATE doesn't say the kind.
                Some(skill) => *skill = net::Skill { kind: if changed.kind == u32::MAX { skill.kind } else { changed.kind }, ..changed },
                None => game.skills.push(net::Skill { kind: if changed.kind == u32::MAX { 0 } else { changed.kind }, ..changed }),
            },
            MapEvent::SkillRemoved(id) => game.skills.retain(|skill| skill.id != id),
            MapEvent::Casting { source, target, cell, skill, element, cast_ms } => {
                // A cast with a cast time glows in its element's color under the caster (EF_BEGINSPELL*).
                if cast_ms > 0
                    && let Some(gpu) = gpu
                {
                    game.play_effect(gpu, grf, table, CAST_AURAS.get(element as usize).copied().unwrap_or(CAST_AURAS[0]), source);
                }
                let me = game.account_id;
                let goal = if target == me { Some(game.player.cell) } else { game.units.get(&target).map(|actor| actor.cell) };
                let caster = if source == me { Some(&mut game.player) } else { game.units.get_mut(&source) };
                if let Some(caster) = caster {
                    caster.face(goal.unwrap_or([i32::from(cell[0]), i32::from(cell[1])]));
                }
                if cast_ms > 0 {
                    let (start, length) = (Instant::now(), Duration::from_millis(u64::from(cast_ms)));
                    game.casts.insert((source != me).then_some(source), (start, length));
                    // A cast at a cell: the magic square there. The packet has no level: ours is the skill tree's.
                    // ponytail: our level is the highest learned, not the one picked; others' is level 10.
                    let level = (source == me).then(|| game.skills.iter().find(|known| known.id == skill).map(|known| known.level)).flatten();
                    match (target == 0).then(|| cast_square::Square::new(skill, level, cell, start, length)).flatten() {
                        Some(square) => game.cast_squares.insert((source != me).then_some(source), square),
                        None => game.cast_squares.remove(&(source != me).then_some(source)),
                    };
                }
                game.skill_motion(source, skill, 0, overhead::Shout::Cast, cast_ms as f32, &self.skill_info, look_tables);
                skill_effects(game, skill, "beginCastEffectId", source);
            }
            MapEvent::SkillHit { skill, level, act } => {
                game.casts.remove(&(act.source != game.account_id).then_some(act.source));
                let me = game.account_id;
                let target_cell = if act.target == me { Some(game.player.cell) } else { game.units.get(&act.target).map(|actor| actor.cell) };
                let source = if act.source == me { Some(&mut game.player) } else { game.units.get_mut(&act.source) };
                if let Some(source) = source {
                    if let Some(cell) = target_cell.filter(|_| act.source != act.target) {
                        source.face(cell);
                    }
                    // Monsters swing as for an attack; players take the skill's motion (`skill_motion`).
                    if source.kind != PLAYER {
                        source.play_at_pace(actor::combat_actions(source.kind).0, Instant::now(), actor::attack_scale(act.source_speed, false), false);
                    }
                }
                let scale = actor::attack_scale(act.source_speed, false);
                let hit_ms = game.skill_motion(act.source, skill, i32::from(level as i16), overhead::Shout::Hit, scale, &self.skill_info, look_tables);
                // Hits land on the motion's hit frame (else after the attack motion), each `hit_gap_ms` after the last.
                let hit_ms = hit_ms.unwrap_or(act.source_speed.max(0) as f32);
                let gap = Duration::from_millis(skill_motions::hit_gap_ms(skill));
                if act.damage != 0 && !(act.source == act.target && act.kind == 6) {
                    game.show_hit(&act, Some(hit_ms), gap);
                    // Hit by hit: what comes at the target first (a bolt's arrows), then what shows as it lands.
                    if let Some(gpu) = gpu {
                        let now = Instant::now();
                        let lands = now + Duration::from_secs_f32(hit_ms / 1000.0);
                        for hit in 0..u32::from(act.hits.max(1)) {
                            for effect in table.skill(skill, "beforeHitEffectId") {
                                game.play_effect_from(gpu, grf, table, effect, act.target, now + gap * hit);
                            }
                            for effect in table.skill(skill, "hitEffectId") {
                                game.play_effect_from(gpu, grf, table, effect, act.target, lands + gap * hit);
                            }
                        }
                    }
                }
                skill_effects(game, skill, "effectId", act.target);
                skill_effects(game, skill, "effectIdOnCaster", act.source);
            }
            MapEvent::SkillUsed { skill, amount, target, source, .. } => {
                game.casts.remove(&(source != game.account_id).then_some(source));
                let me = game.account_id;
                let target_cell = if target == me { Some(game.player.cell) } else { game.units.get(&target).map(|actor| actor.cell) };
                if source != target
                    && let (Some(cell), Some(caster)) = (target_cell, if source == me { Some(&mut game.player) } else { game.units.get_mut(&source) })
                {
                    caster.face(cell);
                }
                game.skill_motion(source, skill, amount, overhead::Shout::Used, 1.0, &self.skill_info, look_tables);
                if HEAL_SKILLS.contains(&skill) {
                    game.damages.push(Damage {
                        target: (target != me).then_some(target),
                        amount,
                        start: Instant::now(),
                        kind: DamageKind::Heal { sp: false },
                        flinch: None,
                        drift: [0.0; 2],
                        source: None,
                    });
                }
                skill_effects(game, skill, "effectId", target);
                skill_effects(game, skill, "effectIdOnCaster", source);
            }
            MapEvent::GroundSkill { skill, source, level, cell } => {
                game.casts.remove(&(source != game.account_id).then_some(source));
                if let Some(gpu) = gpu {
                    for effect in table.skill(skill, "effectId") {
                        game.play_effect_at(gpu, grf, table, effect, cell);
                    }
                }
                skill_effects(game, skill, "effectIdOnCaster", source);
                let caster = if source == game.account_id { Some(&mut game.player) } else { game.units.get_mut(&source) };
                if let Some(caster) = caster {
                    caster.face([i32::from(cell[0]), i32::from(cell[1])]);
                }
                game.skill_motion(source, skill, i32::from(level as i16), overhead::Shout::Hit, 1.0, &self.skill_info, look_tables);
            }
            MapEvent::SkillUnit { id, kind, cell } => {
                if let Some(gpu) = gpu {
                    let at = game.world.cell_position(cell.map(f32::from));
                    game.effects.play_unit(gpu, grf, table, kind, id, at, Instant::now());
                }
            }
            MapEvent::SkillUnitGone { id } => game.effects.forget(id),
            // Cooldowns: kept across map changes (the server doesn't repeat them), sent again after a relog.
            MapEvent::SkillCooldown { skill, ms } => game.cooldowns.set(skill, ms, control::now()),
            MapEvent::SkillFailed { ok: true, .. } => {}
            // A cut-off cast: its bar and its magic square go.
            // Reputation points: the window fits the rows they show.
            MapEvent::Reputation(entries) => {
                game.reputation.extend(entries);
                if let Some(window) = &mut game.reputation_window {
                    window.refresh(&self.reputation_tables, &game.reputation);
                }
            }
            MapEvent::ReputationOpen { .. } => {
                if game.reputation_window.is_none() {
                    game.toggle_reputation(&self.reputation_tables);
                }
                game.sync_esc();
            }
            // The server starts a navigation: to a cell (kind 0), a map (1) or a monster's map (3).
            MapEvent::Navigate { kind, services, hide_window, map, cell, monster } => {
                let map = navigation::map_key(&map);
                let name = self.navi_tables.maps.get(&map).cloned().unwrap_or_else(|| map.clone());
                let target = navigation::Target { cell: (kind == 0).then_some(cell), monster: (kind == 3).then_some(monster), name, map };
                game.navigation = Some(navigation::Active::new(target, services));
                if !hide_window && game.navi_window.is_none() {
                    game.toggle_navigation();
                    game.sync_esc();
                }
            }
            MapEvent::CastCancelled(source) => {
                let caster = (source != game.account_id).then_some(source);
                game.casts.remove(&caster);
                game.cast_squares.remove(&caster);
            }
            MapEvent::SkillFailed { skill, detail, cause, .. } => {
                game.casts.remove(&None);
                // The client's messages for why: a skill's own reason, else the general cause.
                let by_skill = (detail != 0).then(|| match skill {
                    1 if detail < 7 => 159 + detail as usize,
                    1 if detail == 7 => 383,
                    27 => 214,
                    50 => 205,
                    52 => 207,
                    _ => 204,
                });
                let by_cause = match cause {
                    1 => Some(202),
                    2 => Some(203),
                    3 => Some(808),
                    4 => Some(219),
                    5 => Some(233),
                    6 => Some(239),
                    7 => Some(246),
                    8 => Some(247),
                    9 => Some(580),
                    10 => Some(285),
                    13 => Some(1398),
                    83 => Some(661),
                    _ => None,
                };
                if let Some(id) = by_cause.or(by_skill) {
                    game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ERROR);
                }
            }
            // A pushcart comes and goes as a status, its type the status's value.
            MapEvent::StatusIcon { kind: actor::PUSH_CART, id, on, value, .. } => {
                let actor = if id == game.account_id { Some(&mut game.player) } else { game.units.get_mut(&id) };
                if let Some(actor) = actor {
                    let (sprite, act) = actor::cart_path(value, actor.job);
                    let sprites = SpriteSet::with_act(grf, &sprite, &act).filter(|_| on);
                    let leader = actor::world_xz(actor.cell.map(|value| value as f32));
                    actor.cart = sprites.map(|sprites| actor::Cart::behind(sprites, leader, actor.direction));
                }
                if id == game.account_id {
                    game.status_icons.retain(|(icon, ..)| *icon != actor::PUSH_CART);
                    if on && self.status_icon_files.contains_key(&actor::PUSH_CART) {
                        game.status_icons.push((actor::PUSH_CART, Instant::now(), None));
                    }
                }
            }
            MapEvent::StatusIcon { kind, id, on, left_ms, value, .. } => {
                // ChangeSize's percent multiplies the size table when the status is on.
                if kind == actor::CHANGE_SIZE || kind == actor::CHANGE_SIZE_MONSTER {
                    let actor = if id == game.account_id { Some(&mut game.player) } else { game.units.get_mut(&id) };
                    if let Some(actor) = actor {
                        // The short status packet carries no percent. 0 would shrink the sprite away.
                        actor.size_percent = (on && value != 0).then_some(value);
                    }
                }
                // The mirror's 10 minutes are over: the boss leaves the minimap.
                if id == game.account_id && kind == BOSS_ALARM && !on {
                    game.boss = None;
                }
                // A lock's status ending lets go of the unit's pairs too: rathena sends no ZC_BLADESTOP release for
                // Cursed Circle (its caster's count of caught units stays 0).
                if !on && LOCK_ICONS.contains(&kind) {
                    lock_pair(&mut game.locks, id, id, false);
                    game.hold_locked();
                }
                if id == game.account_id && kind == ALL_RIDING {
                    game.riding = on;
                }
                if id == game.account_id {
                    game.status_icons.retain(|(icon, ..)| *icon != kind);
                    if on && self.status_icon_files.contains_key(&kind) {
                        let now = Instant::now();
                        let ends = (left_ms > 0).then(|| now + Duration::from_millis(u64::from(left_ms)));
                        game.status_icons.push((kind, now, ends));
                    }
                }
            }
            // Potions and the like: green HP or blue SP over the player.
            MapEvent::Recovered { kind, amount } => {
                game.damages.push(Damage {
                    target: None,
                    amount,
                    start: Instant::now(),
                    kind: DamageKind::Heal { sp: kind == net::STATUS_SP },
                    flinch: None,
                    drift: [0.0; 2],
                    source: None,
                });
            }
            // Mounting, dismounting or an outfit puts a player in another body. Body and health state colour it.
            MapEvent::OptionChange { id, option, body, health } => {
                let me = game.player.cell;
                let actor = if id == game.account_id { Some(&mut game.player) } else { game.units.get_mut(&id) };
                if let Some(actor) = actor {
                    actor.cloaked = option & actor::UNSEEN_OPTIONS != 0;
                    actor.hide_option = option;
                    let changed = state_sounds((actor.body_state, actor.health_state), (body, health));
                    if let Some(audio) = &mut self.audio {
                        let loudness = heard(f32::hypot((actor.cell[0] - me[0]) as f32, (actor.cell[1] - me[1]) as f32));
                        for sound in changed {
                            audio.play(grf, sound, loudness);
                        }
                    }
                    actor.body_state = body;
                    actor.health_state = health;
                }
                game.hold_locked();
                let actor = if id == game.account_id { Some(&mut game.player) } else { game.units.get_mut(&id) };
                if let Some(actor) = actor
                    && let Some(look) = &mut actor.look
                    && look.option != option & actor::BODY_OPTIONS
                {
                    look.option = option & actor::BODY_OPTIONS;
                    actor.sprites = SpriteSet::player(grf, look, look_tables);
                }
                let actor = if id == game.account_id { Some(&mut game.player) } else { game.units.get_mut(&id) };
                if let Some(actor) = actor {
                    actor.sync_companions(option, grf);
                }
            }
            MapEvent::LookChange { id, kind, value, extra } => {
                // A skill's ground unit becoming another (rathena's Warp Portal opens this way): the new one's effect.
                if kind == net::LOOK_JOB
                    && let Some(at) = game.effects.ground_unit_at(id)
                    && let Some(gpu) = gpu
                {
                    game.effects.play_unit(gpu, grf, table, value, id, at, Instant::now());
                }
                // A job change comes as the player's new look.
                if id == game.account_id && kind == net::LOOK_JOB {
                    game.me.job = value as u16;
                }
                let actor = if id == game.account_id { Some(&mut game.player) } else { game.units.get_mut(&id) };
                if let Some(actor) = actor
                    && let Some(look) = &mut actor.look
                    && change_look(look, kind, value, extra)
                {
                    actor.sprites = SpriteSet::player(grf, look, look_tables);
                }
                // The preview follows the real look once the server has applied it. Picks stay until the window closes.
                if id == game.account_id {
                    let look = game.player.look.clone().unwrap_or_default();
                    if let Some(window) = &mut game.stylist {
                        dress_stylist(window, grf, look_tables, &look, &self.items, &self.styling_shop);
                    }
                }
            }
            MapEvent::Equipped { index, location, result } => {
                let name = game.inventory.get(index).map(|item| inventory::item_name(&self.items, item.id, item.identified, item.refine)).unwrap_or_default();
                let message = |id: usize| self.messages.get(id).cloned().unwrap_or_default();
                match result {
                    0 => {
                        game.inventory.set_worn(index, location);
                        game.chat_line(format!("{name}{}", message(170)), CHAT_ITEM);
                    }
                    1 => game.chat_line(message(1773), CHAT_ERROR),
                    _ => game.chat_line(message(372), CHAT_ERROR),
                }
            }
            // A rental ended: its warning, its time up and gone from the inventory (the take-off and delete that may follow
            // find nothing then).
            // ponytail: rathena's comment says minutes; under a minute it's said in seconds.
            MapEvent::RentalTime { item, seconds } => {
                let name = inventory::item_name(&self.items, item, true, 0);
                let (id, count) = if seconds < 60 { (RENTAL_SECONDS, seconds) } else { (RENTAL_MINUTES, seconds / 60) };
                game.chat_line(messages::fill(self.messages.get(id).map_or("", String::as_str), &[&name, &count]).trim_start().to_owned(), CHAT_ITEM);
            }
            MapEvent::RentalExpired { index, item } => {
                let name = inventory::item_name(&self.items, item, true, 0);
                game.chat_line(messages::fill(self.messages.get(RENTAL_ENDED).map_or("", String::as_str), &[&name]).trim_start().to_owned(), CHAT_ITEM);
                game.inventory.remove(index, u16::MAX);
            }
            // An item no longer carried (a rental that ended) comes off without a word.
            MapEvent::Unequipped { index, failed: false, .. } if game.inventory.get(index).is_none() => {}
            MapEvent::Unequipped { index, failed: false, .. } => {
                let name = game.inventory.get(index).map(|item| inventory::item_name(&self.items, item.id, item.identified, item.refine)).unwrap_or_default();
                game.inventory.set_worn(index, 0);
                game.chat_line(format!("{name} {}", self.messages.get(171).cloned().unwrap_or_default()), CHAT_ERROR);
            }
            MapEvent::Unequipped { .. } => {}
            MapEvent::ItemRemoved { index, amount } => game.inventory.remove(index, amount),
            MapEvent::ItemUsed { index, item, user, remaining, ok } => {
                if ok && user == game.account_id {
                    game.inventory.set_amount(index, remaining);
                }
                if ok && let (Some(gpu), Some(effect)) = (gpu, table.item(item)) {
                    game.play_effect(gpu, grf, table, effect, user);
                }
            }
            // Weather comes as effects on the player: it shows until the map changes.
            MapEvent::Effect { id, effect } if id == game.account_id && weather::is_weather(effect) => game.weather.start(effect, control::now()),
            MapEvent::Effect { id, effect } => {
                if let Some(gpu) = gpu {
                    game.play_effect(gpu, grf, table, effect, id);
                }
            }
            // Marriage: the wedding, the partner's name, divorce; adoption: being asked, and refusals.
            MapEvent::Congratulation(id) => {
                if let Some(gpu) = gpu {
                    game.play_effect(gpu, grf, table, WEDDING_EFFECT, id);
                }
            }
            // ponytail: the name comes to everyone near whoever called, without who; taken as ours while we have the
            // ring's skill, so another couple calling nearby overwrites it.
            MapEvent::PartnerName(name) => {
                if !name.is_empty() && game.skills.iter().any(|skill| skill.id == CALL_PARTNER) {
                    game.partner = Some(name);
                }
            }
            // ponytail: the line names the player, then the partner; the order is an estimate.
            MapEvent::Divorced(partner) => {
                let line = rustnarok_client::messages::fill(self.messages.get(DIVORCED).map_or("", String::as_str), &[&game.me.name, &partner]);
                game.chat_line(line, CHAT_WHITE);
                game.partner = None;
            }
            MapEvent::AdoptAsked { parent, partner, name } => game.adopt_asked = Some((parent, partner, name, false)),
            MapEvent::AdoptRefused(reason) => {
                if let Some(&id) = ADOPT_REFUSALS.get(reason as usize) {
                    game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ERROR);
                }
            }
            MapEvent::DealChoice { npc } => {
                game.dialog = None;
                game.deal_choice = Some(npc);
                // Dev switch: answer at once.
                if let Ok(deal) = std::env::var("RUSTNAROK_CLIENT_DEAL") {
                    game.deal_choice = None;
                    let _ = game.session.choose_deal(npc, deal.starts_with("sell"));
                }
            }
            MapEvent::ShopGoods(goods) => {
                let offers = goods
                    .into_iter()
                    .map(|goods| shop::Offer {
                        item: goods.item,
                        price: goods.discounted,
                        listed: goods.price,
                        kind: goods.kind,
                        identified: true,
                        ..Default::default()
                    })
                    .collect();
                game.shop = Some(shop::Shop::new(false, offers));
                game.auto_deal();
            }
            MapEvent::ShopWants(wants) => {
                let offers = wants
                    .into_iter()
                    .filter_map(|(index, listed, price)| {
                        let item = game.inventory.get(index)?;
                        Some(shop::Offer {
                            item: item.id,
                            price,
                            listed,
                            index,
                            have: item.amount,
                            kind: item.kind,
                            identified: item.identified,
                            refine: item.refine,
                            ..Default::default()
                        })
                    })
                    .collect();
                game.shop = Some(shop::Shop::new(true, offers));
                game.auto_deal();
            }
            // Markets and barters: the plain shop's windows with stock and costs; they stay open after a purchase.
            MapEvent::Market { kind, goods } => {
                let offers = goods
                    .into_iter()
                    .map(|goods| {
                        // The inventory index of what it costs: the field's name says so; rathena doesn't read it.
                        let cost_index = goods
                            .costs
                            .first()
                            .and_then(|&(item, _)| game.inventory.items.iter().find(|carried| carried.id == item))
                            .map_or(0, |carried| carried.index);
                        shop::Offer {
                            item: goods.item,
                            price: goods.zeny,
                            kind: goods.kind,
                            identified: true,
                            stock: goods.stock,
                            costs: goods.costs,
                            shop_index: goods.index,
                            cost_index,
                            ..Default::default()
                        }
                    })
                    .collect();
                let mut store = shop::Shop::new(false, offers);
                store.market = Some(kind);
                game.shop = Some(store);
            }
            MapEvent::MarketBought { ok, items } => {
                if ok && let Some(shop) = &mut game.shop {
                    shop.bought(&items.iter().map(|&(item, amount)| (item, u32::from(amount))).collect::<Vec<_>>());
                }
                // ponytail: rathena sends 0 or 0xFFFF only; the lines are the plain shop's.
                let (id, color) = if ok { (54, CHAT_ITEM) } else { (57, CHAT_ERROR) };
                game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), color);
            }
            // A barter's answer: its own lines (by their words), the window stays with what's left.
            MapEvent::Bought(result)
                if game.shop.as_ref().is_some_and(|shop| matches!(shop.market, Some(net::MarketKind::Barter | net::MarketKind::Expanded))) =>
            {
                let (id, color) = match result {
                    0 => (3555, CHAT_ITEM),
                    1 => (55, CHAT_ERROR),
                    2 => (56, CHAT_ERROR),
                    11 => (3554, CHAT_ERROR),
                    13 => (3557, CHAT_ERROR),
                    14 => (3556, CHAT_ERROR),
                    _ => (57, CHAT_ERROR),
                };
                if result == 0
                    && let Some(shop) = &mut game.shop
                {
                    let deal: Vec<_> = shop.deal.iter().map(|&(offer, amount)| (shop.offers[offer].item, amount)).collect();
                    shop.bought(&deal);
                }
                game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), color);
            }
            MapEvent::Bought(result) | MapEvent::Sold(result) => {
                // The client's words for how it went (some results say nothing).
                let sold = matches!(event, MapEvent::Sold(_));
                if let Some((id, color)) = deal_result(sold, result) {
                    game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), color);
                }
                game.shop = None;
            }
            MapEvent::MiscEffect { id, kind } => {
                if let (Some(gpu), Some(effect)) = (gpu, table.misc(kind)) {
                    game.play_effect(gpu, grf, table, effect, id);
                }
            }
            // Out of reach after all: chase it again, as asked (kept up unless it was only while held).
            // ponytail: the chase starts from the client's positions, not the server's in the packet.
            MapEvent::TooFar { target, range, .. } => {
                game.attack_range = i32::from(range);
                let (continuous, held) = game.attack.map_or((true, false), |attack| (attack.continuous, attack.held));
                game.attack = Some(Attack { held, ..Attack::new(target, continuous) });
                game.attack_step(Instant::now());
            }
            MapEvent::StyleChanged(false) => {
                game.stylist = None;
                game.chat_line(self.messages.get(stylist::DONE).cloned().unwrap_or_default(), CHAT_ITEM);
            }
            // ponytail: no client line fits a failed style change, so the result is printed plainly. The window stays.
            MapEvent::StyleChanged(true) => game.chat_line(stylist::FAILED_LINE.to_owned(), CHAT_ERROR),
            MapEvent::Entered { .. } | MapEvent::ServerTime(..) => unreachable!("handled above"),
            MapEvent::Disconnected(_) => self.message(self.messages.get(DISCONNECTED_MESSAGE).cloned().unwrap_or_default(), Scene::Login),
        }
    }

    fn redraw(&mut self, event_loop: &ActiveEventLoop) {
        if self.loading.is_some() {
            return self.draw_loading(event_loop);
        }
        if matches!(self.scene, Scene::Game(_)) {
            return self.redraw_game(event_loop);
        }
        let blink_phase = self.blink_phase();
        let (Some(gpu), Some(assets)) = (&mut self.gpu, &self.assets) else { return };
        let font = &self.font;
        // Covered scenes don't react to the pointer.
        let interactive =
            matches!(self.scene, Scene::Connections { .. } | Scene::Login | Scene::ServerSelect { .. } | Scene::CharSelect(_) | Scene::CharMake(..));
        let base_pointer = if interactive { (self.cursor, self.mouse_down) } else { ([-1.0; 2], false) };

        // Character figures decode their textures first (mutably), then the frame borrows them.
        let poses = match self.scene.base_mut() {
            Scene::CharSelect(select) => {
                for path in select.job_icons() {
                    self.icons.entry(path.clone()).or_insert_with(|| gpu.try_load_texture(&self.grf, &path, false).ok());
                }
                select.pose(gpu)
            }
            _ => Vec::new(),
        };
        let make_poses = match self.scene.base_mut() {
            Scene::CharMake(make, _) => make.pose(gpu),
            _ => [None, None, None],
        };

        let mut base = Layer::default();
        let (at, size) = picture_place();
        base.sprites.push((&assets.background, at, size));
        let mut caret = None;
        match self.scene.base() {
            Scene::Login => {
                let login = login_position(&assets.login);
                base.sprites.push((&assets.login, login, assets.login.size));
                base.sprites.push(button_sprite(base_pointer, offset(login, LOGIN_START), &assets.connect));
                let save = &assets.save[usize::from(self.save_id)];
                base.sprites.push((save, offset(login, LOGIN_SAVE.0), save.size));
                for (index, value) in self.fields.iter().enumerate() {
                    let shown = if index == 1 { "*".repeat(value.len()) } else { value.clone() };
                    // Long input scrolls: keep the tail that fits the box.
                    let mut start = 0;
                    while font.width(&shown[start..]) > LOGIN_FIELD_SIZE[0] - 2.0 * FIELD_PADDING {
                        start += 1;
                    }
                    // Centered in the field.
                    let field = offset(login, LOGIN_FIELDS[index]);
                    let left = ((LOGIN_FIELD_SIZE[0] - font.width(&shown[start..])) / 2.0).floor();
                    let position = [field[0] + left, field[1] + ((LOGIN_FIELD_SIZE[1] - font.line_height()) / 2.0).floor()];
                    if index == self.focus && matches!(self.scene, Scene::Login) && blink_phase.is_multiple_of(2) {
                        caret = Some([position[0] + font.width(&shown[start..]).round(), position[1] + 1.0]);
                    }
                    base.texts.push((shown[start..].to_owned(), position, TEXT_COLOR));
                }
            }
            Scene::Connections { selected } => {
                let service = dialog_position(&assets.service);
                let list = offset(service, SERVER_LIST);
                base.sprites.push((&assets.service, service, assets.service.size));
                base.sprites.push((&assets.selection, offset(list, [0.0, *selected as f32 * SERVER_ROW[1]]), SERVER_ROW));
                base.sprites.push(button_sprite(base_pointer, offset(service, LEFT_BUTTON), &assets.ok));
                base.sprites.push(button_sprite(base_pointer, offset(service, RIGHT_BUTTON), &assets.cancel));
                for (row, connection) in self.connections.iter().enumerate() {
                    base.texts.push((connection.display.clone(), offset(list, [4.0, row as f32 * SERVER_ROW[1] + 3.0]), TEXT_COLOR));
                }
            }
            Scene::ServerSelect { accepted, selected } => {
                let service = dialog_position(&assets.service);
                let list = offset(service, SERVER_LIST);
                base.sprites.push((&assets.service, service, assets.service.size));
                base.sprites.push((&assets.selection, offset(list, [0.0, *selected as f32 * SERVER_ROW[1]]), SERVER_ROW));
                base.sprites.push(button_sprite(base_pointer, offset(service, LEFT_BUTTON), &assets.ok));
                base.sprites.push(button_sprite(base_pointer, offset(service, RIGHT_BUTTON), &assets.cancel));
                for (row, server) in accepted.char_servers.iter().enumerate() {
                    base.texts.push((format!("{} ({})", server.name, server.users), offset(list, [4.0, row as f32 * SERVER_ROW[1] + 3.0]), TEXT_COLOR));
                }
            }
            Scene::CharSelect(select) => {
                let select_pointer = if select.pin_pad.is_some() { ([-1.0; 2], false) } else { base_pointer };
                select.draw(&mut base, &assets.char_select, font, &self.messages, &self.pc_job_names, &self.map_names, &self.icons, select_pointer, &poses);
                if let Some(pad) = &select.pin_pad {
                    pad.draw(&mut base, &assets.chrome, &assets.pin, font, &self.messages, base_pointer);
                }
            }
            Scene::CharMake(make, _) => {
                let caret = matches!(self.scene, Scene::CharMake(..)) && blink_phase.is_multiple_of(2);
                make.draw(&mut base, &assets.char_make, font, &self.messages, base_pointer, &make_poses, caret)
            }
            Scene::Game(_) | Scene::Waiting(_) | Scene::Message(..) | Scene::Confirm(..) | Scene::Prompt { .. } => unreachable!("handled elsewhere"),
        }

        let mut overlay = Layer::default();
        let pointer = (self.cursor, self.mouse_down);
        let dialog = dialog_position(&assets.message);
        let (text, buttons) = match &self.scene {
            Scene::Message(text, _) => (Some(text), vec![(RIGHT_BUTTON, &assets.ok)]),
            Scene::Confirm(text, ..) | Scene::Prompt { text, .. } => (Some(text), vec![(LEFT_BUTTON, &assets.ok), (RIGHT_BUTTON, &assets.cancel)]),
            _ => (None, Vec::new()),
        };
        if let Some(text) = text {
            overlay.sprites.push((&assets.message, dialog, assets.message.size));
            for (place, images) in buttons {
                overlay.sprites.push(button_sprite(pointer, offset(dialog, place), images));
            }
            let lines = message_lines(font, text, MESSAGE_WIDTH);
            let top = if matches!(self.scene, Scene::Prompt { .. }) { MESSAGE_TEXT[1] - 16.0 } else { MESSAGE_TEXT[1] };
            for (row, line) in lines.into_iter().enumerate() {
                overlay.texts.push((line.trim_start().to_owned(), offset(dialog, [MESSAGE_TEXT[0], top + row as f32 * MESSAGE_LINE]), TEXT_COLOR));
            }
        }
        if let Scene::Prompt { input, .. } = &self.scene {
            let field = offset(dialog, PROMPT_FIELD);
            overlay.sprites.push((&assets.grey, offset(field, [-1.0, -1.0]), [PROMPT_FIELD_SIZE[0] + 2.0, PROMPT_FIELD_SIZE[1] + 2.0]));
            overlay.sprites.push((&assets.white, field, PROMPT_FIELD_SIZE));
            let at = [field[0] + FIELD_PADDING, field[1] + ((PROMPT_FIELD_SIZE[1] - font.line_height()) / 2.0).floor()];
            overlay.texts.push((input.clone(), at, TEXT_COLOR));
            if blink_phase.is_multiple_of(2) {
                caret = Some([at[0] + font.width(input).round(), at[1] + 1.0]);
            }
        }

        // A character card's menu, over the cards.
        let mut card_menu = Layer::default();
        if let Scene::CharSelect(select) = &self.scene {
            select.draw_menu(&mut card_menu, &assets.context_menu, font, self.cursor);
        }
        let composed = compose(gpu, font, vec![base, card_menu, overlay]);
        let (mut sprites, alphas) = flatten(&composed);
        if let Some(position) = caret {
            sprites.push((&assets.black, position, [1.0, font.line_height() - 2.0]));
        }
        self.cursor_look.set(cursor::Kind::Default);
        sprites.extend(self.cursor_look.draw(gpu, self.cursor));

        // With RUSTNAROK_CLIENT_AUTOSTART=<slot> the shot waits for the game; any other value stops at character select.
        let autostart = std::env::var("RUSTNAROK_CLIENT_AUTOSTART").ok();
        let playing_slot = autostart.as_deref().is_some_and(|value| value.parse::<usize>().is_ok());
        let settled = !matches!(self.scene, Scene::Waiting(_))
            && match autostart {
                None => true,
                // Back from the game (the option menu's character select) counts too.
                Some(_) if playing_slot => matches!(self.scene, Scene::Message(..)) || (self.returned && matches!(self.scene, Scene::CharSelect(_))),
                Some(_) => matches!(self.scene, Scene::CharSelect(_) | Scene::CharMake(..) | Scene::Message(..) | Scene::Confirm(..) | Scene::Prompt { .. }),
            };
        match &self.screenshot {
            Some(path) if settled => {
                gpu.capture(None, &sprites, &alphas, path);
                event_loop.exit();
            }
            _ => gpu.draw(None, &sprites, &alphas),
        }
        self.drawn_blink_phase = blink_phase;
    }

    fn redraw_game(&mut self, event_loop: &ActiveEventLoop) {
        let (Some(gpu), Some(assets), Scene::Game(game)) = (&mut self.gpu, &self.assets, &mut self.scene) else { return };
        // ponytail: the client's own line for a saved screenshot isn't in the data; a plain one.
        if let Some(name) = self.shot_saved.take() {
            game.chat_line(format!("Screenshot saved: {name}"), CHAT_ITEM);
        }
        let (grf, items, messages, icons, cursor_look, pc_job_names) =
            (&self.grf, &self.items, &self.messages, &mut self.icons, &mut self.cursor_look, &self.pc_job_names);
        let (audio, sound) = (&mut self.audio, &self.sound);
        let (worlds, map_names, quest_texts) = (&self.worlds, &self.map_names, &self.quest_texts);
        let card_names = &self.card_names;
        let (achievement_info, title_names) = (&self.achievement_info, &self.title_names);
        let (skill_info, skill_trees, status_icon_files) = (&self.skill_info, &self.skill_trees, &self.status_icon_files);
        let state_infos = &self.state_infos;
        let monster_sizes = &self.monster_sizes;
        let shadow_factors = &self.shadow_factors;
        let font = &self.font;
        let now = control::now();
        let pointer = (self.cursor, self.mouse_down);
        let players_first = game.players_first(self.modifiers.shift_key());
        let message = |id: usize| messages.get(id).map_or("", String::as_str);

        // The mercenary and the homunculus follow the player unless told otherwise.
        if let Some(id) = game.mercenary.as_ref().map(|hired| hired.id) {
            game.follow_player(id, false, now);
        }
        if let Some(id) = game.homunculus_id {
            game.follow_player(id, true, now);
        }

        // Where the windows were left is kept for next time.
        game.save_places();
        // Fireworks weather sets off fireworks around the player.
        for [dx, dy] in game.weather.fireworks_due(now) {
            let cell = [game.player.cell[0] + dx, game.player.cell[1] + dy].map(|value| value.max(0) as u16);
            game.play_effect_at(gpu, grf, &self.effect_table, weather::FIREWORK, cell);
        }

        // Bodies that finished dying leave; hits that land make their target flinch.
        game.units.retain(|_, actor| actor.remove_at.is_none_or(|at| at > now));
        // A hit's sound when it lands, at the target.
        let mut hit_sounds = Vec::new();
        for damage in &mut game.damages {
            if damage.start <= now
                && let Some(source) = damage.source.take()
            {
                let target = damage.target.map_or(Some(&game.player), |id| game.units.get(&id));
                let attacker = if source == game.account_id { Some(&game.player) } else { game.units.get(&source) };
                if let Some(target) = target {
                    let lineage = (target.kind == PLAYER).then(|| self.skill_trees.lineage(target.job));
                    let weapon = attacker.and_then(|actor| actor.look.as_ref()).map(|look| self.look_tables.weapon_type(look.weapon));
                    let roll = damage.start.duration_since(game.entered).subsec_nanos() >> 8;
                    let at = target.position(now);
                    let me = game.player.position(now);
                    hit_sounds.push((hit_sound(lineage.as_deref(), weapon, roll), heard(f32::hypot(at[0] - me[0], at[1] - me[1]))));
                }
            }
            if damage.start <= now && damage.flinch.is_some() {
                let motion = damage.flinch.take();
                let target = match damage.target {
                    None => Some(&mut game.player),
                    Some(id) => game.units.get_mut(&id),
                };
                // The flinch stops a walk once its step is done and gets a sitting player up.
                if let Some(target) =
                    target.filter(|target| target.remove_at.is_none() && !target.dead && actor::flinches(target.kind, target.animation(now).0))
                    && let Some(motion) = motion
                {
                    let (_, flinch, _) = actor::combat_actions(target.kind);
                    target.flinch(flinch, now, motion);
                    target.stop_after_step();
                    target.sitting = false;
                    target.stand_ready();
                }
            }
        }
        game.damages.retain(|damage| now < damage.start || now.duration_since(damage.start).as_secs_f32() < damage.kind.duration(damage.amount));

        // Move everyone, follow the player, then decode the sprite frames (mutably) before borrowing them.
        let walking = game.player.moving();
        let player_cell = game.player.update(now);
        game.player.step_companions(actor::world_xz(player_cell), walking);
        if walking
            && !game.player.moving()
            && let Some((_, arrived)) = &mut game.pending
        {
            *arrived = Some(now);
        }
        let player_feet = game.world.cell_position(player_cell);
        game.camera_goal.drag_frame();
        game.camera_goal.ease(&mut game.camera, now.duration_since(game.last_frame), player_feet, game.camera_snap);
        // An earthquake shakes the view sideways and in and out, the other way each frame.
        let quake = if self.graphics.effects_on { game.effects.quake(now) } else { 0.0 };
        if quake > 0.0 {
            game.quake_flip = !game.quake_flip;
            let shake = if game.quake_flip { quake } else { -quake };
            game.camera.target[0] += game.camera.yaw.cos() * shake;
            game.camera.target[2] -= game.camera.yaw.sin() * shake;
            game.camera.distance += shake;
        }
        // The eye stays above the ground under it.
        let horizontal = game.camera.distance * game.camera.pitch.cos();
        let eye = [game.camera.target[0] - horizontal * game.camera.yaw.sin(), game.camera.target[2] - horizontal * game.camera.yaw.cos()];
        game.camera.ground = -game.world.gat.height(eye.map(|axis| (axis / world::CELL).floor() as i32));
        game.last_frame = now;
        let view = game.camera.view(screen()[0] / screen()[1]);
        let yaw = game.camera.yaw;
        let mut frames = Vec::new();
        let mut sounds: Vec<(String, f32)> = game.player.sounds_now(now).into_iter().map(|sound| (sound, 1.0)).chain(hit_sounds).collect();
        let (action, timing) = game.player.animation(now);
        let head_direction = game.player.head_direction;
        let player_seen = seen_direction(game.player.rot, yaw, rounds_facing(PLAYER, game.player.job, action));
        // Pushcarts, apart from the bodies (their textures are the cart's, and they cast no shadow of their own).
        // A cart's ACT has one action per direction; its frames turn the wheels, so they run only while walking.
        // ponytail: the cart's anchor half a cell behind its owner; place it against the real client.
        // A cart stands on the ground where its rope has it, facing its owner, its wheels at the distance rolled. Its
        // ACT has one action per direction.
        let world = &game.world;
        let cart_frame = |cart: &mut actor::Cart| {
            let seen = usize::from(seen_direction(cart.rot, yaw, true));
            let frame = cart.frame(cart.sprites.frames(seen));
            let count = cart.sprites.frames(seen).max(1) as f32;
            let placed = cart.sprites.frame_at(gpu, seen, 0, Timing::Span((frame as f32 + 0.5) / count));
            (world.ground_under([cart.at[0], 0.0, cart.at[1]]), placed)
        };
        let mut carts = Vec::new();
        let mut companion_draws: Vec<CompanionDraw> = Vec::new();
        if let Some(cart) = &mut game.player.cart
            && actor::seen(game.player.hide_option, true) == actor::Seen::Whole
        {
            let (feet, placed) = cart_frame(cart);
            carts.push((Owner::Player, feet, placed));
        }
        let player_tint = actor::status_tint(game.player.body_state, game.player.health_state);
        let player_scale = unit_draw_scale(monster_sizes, game.player.kind, game.player.job, game.player.size_percent);
        // Our own character in Hiding shows its shadow alone (a frame with no sprite); cloaked or GM-hidden, nothing.
        match actor::seen(game.player.hide_option, true) {
            actor::Seen::Whole => {
                if let Some(sprites) = &mut game.player.sprites {
                    sprites.set_tint(player_tint);
                    let mut placed = sprites.frame(gpu, action, player_seen, head_direction, timing);
                    scale_frame(&mut placed, player_scale);
                    frames.push((Owner::Player, player_feet, placed));
                }
            }
            actor::Seen::Shadow => frames.push((Owner::Player, player_feet, Vec::new())),
            actor::Seen::Nothing => {}
        }
        if !game.player.cloaked {
            if let Some(falcon) = &mut game.player.falcon {
                push_companion(falcon, false, Owner::Player, gpu, &game.world, yaw, now, &mut companion_draws);
            }
            if let Some(warg) = &mut game.player.warg {
                push_companion(warg, true, Owner::Player, gpu, &game.world, yaw, now, &mut companion_draws);
            }
        }
        // The dress room's figure: rebuilt when a pick changes, standing the way it's turned.
        if let Some(room) = &mut game.dressroom
            && room.changed
        {
            game.dress_figure = SpriteSet::player(grf, &room.look(&character_look(&game.me), &game.dress_lists), &self.look_tables);
            room.changed = false;
        }
        let dress_placements = match (&game.dressroom, &mut game.dress_figure) {
            (Some(room), Some(figure)) => figure.flat_frame(gpu, actor::IDLE, room.direction, 0, Timing::Loop(0.0)),
            _ => Vec::new(),
        };
        // The equipment window shows the character standing, facing front.
        let preview_feet = game.equipment.preview_feet();
        let viewed_feet = game.viewing.as_ref().and_then(|viewed| viewed.window.preview_feet());
        let viewed_placements = match (viewed_feet, game.viewing.as_mut().and_then(|viewed| viewed.sprites.as_mut())) {
            (Some(_), Some(sprites)) => sprites.flat_frame(gpu, actor::IDLE, 0, 0, Timing::Loop(0.0)),
            _ => Vec::new(),
        };
        let preview_placements = match (preview_feet, &mut game.player.sprites) {
            (Some(_), Some(sprites)) => {
                // The equipment figure keeps the sprite's own colours.
                sprites.set_tint(None);
                sprites.flat_frame(gpu, actor::IDLE, 0, 0, Timing::Loop(0.0))
            }
            _ => Vec::new(),
        };
        let stylist_placements = game.stylist.as_mut().map(|window| window.pose(gpu)).unwrap_or_default();
        let mut labels = Vec::new();
        // Whose names show grey (ours) and against what level monsters' are judged.
        let own_level = game.status.get(&net::STATUS_BASE_LEVEL).copied().unwrap_or(1);
        let our_summons = [game.pet_id, game.homunculus_id, game.mercenary.as_ref().map(|mercenary| mercenary.id)];
        // Talkie Boxes saying their words stand where their traps were.
        let mut feet_of = game.talkie_spots.clone();
        for (&id, actor) in &mut game.units {
            let walking = actor.moving();
            let cell = actor.update(now);
            actor.step_companions(actor::world_xz(cell), walking);
            // Hidden players aren't drawn at all: no sprite, shadow, name or bar, nothing to click.
            if actor::seen(actor.hide_option, false) != actor::Seen::Whole {
                continue;
            }
            let feet = game.world.cell_position(cell);
            feet_of.insert(id, feet);
            let loudness = heard(f32::hypot(cell[0] - player_cell[0], cell[1] - player_cell[1]));
            sounds.extend(actor.sounds_now(now).into_iter().map(|sound| (sound, loudness)));
            let (action, timing) = actor.animation(now);
            let head_direction = actor.head_direction;
            let seen = seen_direction(actor.rot, yaw, rounds_facing(actor.kind, actor.job, action));
            // NPC names show under them; players' and monsters' on hover (below).
            if !actor.hidden && (actor.kind == NPC || actor.sprites.is_none()) {
                labels.push((actor.name.clone(), feet, name_color(actor.kind, actor.job, actor.level, our_summons.contains(&Some(id)), own_level)));
            }
            if let Some(cart) = &mut actor.cart {
                let (feet, placed) = cart_frame(cart);
                carts.push((Owner::Unit(id), feet, placed));
            }
            let tint = crate::actor::status_tint(actor.body_state, actor.health_state);
            let scale = unit_draw_scale(monster_sizes, actor.kind, actor.job, actor.size_percent);
            if let Some(sprites) = &mut actor.sprites {
                sprites.set_tint(tint);
                let mut placed = sprites.frame(gpu, action, seen, head_direction, timing);
                scale_frame(&mut placed, scale);
                frames.push((Owner::Unit(id), feet, placed));
            }
            if !actor.cloaked {
                if let Some(falcon) = &mut actor.falcon {
                    push_companion(falcon, false, Owner::Unit(id), gpu, &game.world, yaw, now, &mut companion_draws);
                }
                if let Some(warg) = &mut actor.warg {
                    push_companion(warg, true, Owner::Unit(id), gpu, &game.world, yaw, now, &mut companion_draws);
                }
            }
        }
        if let Some(audio) = audio {
            for (sound, volume) in sounds {
                audio.play(grf, &sound, volume);
            }
        }
        // Items on the ground; just-dropped ones fall into place.
        for (&id, item) in &game.ground {
            let Some(Some(sprites)) = game.item_sprites.get_mut(ground_sprite(items, item.item, item.identified)) else { continue };
            let mut feet = game.world.ground_under(game.world.cell_position(item.cell));
            if let Some(dropped) = item.dropped {
                feet[1] += item_fall_height(now.saturating_duration_since(dropped).as_secs_f32() / 0.024);
            }
            frames.push((Owner::Item(id), feet, sprites.frame(gpu, 0, 0, 0, Timing::Loop(0.0))));
        }

        // Clickable areas: each unit's and item's sprite bounds and the cell under it on screen, in logical pixels.
        // The dead and the hidden can't be clicked.
        let to_screen = |[x, y]: [f32; 2]| [x * screen()[0], y * screen()[1]];
        let siege_map = game.map_property.is_some_and(|(_, flags)| versus::siege(flags));
        let our_guild = game.guild.as_ref().map_or(0, |guild| guild.id);
        game.hit_boxes = frames
            .iter()
            .filter(|(owner, ..)| !matches!(owner, Owner::Unit(id) if game.units.get(id).is_some_and(|actor| actor.remove_at.is_some() || actor.hidden)))
            .filter(|(_, _, placements)| !placements.is_empty())
            .filter_map(|(owner, feet, placements)| {
                let (mut low, mut high) = ([f32::MAX; 2], [f32::MIN; 2]);
                for (_, position, size, _) in placements {
                    for axis in 0..2 {
                        let (a, b) = (position[axis], position[axis] + size[axis]);
                        low[axis] = low[axis].min(a.min(b));
                        high[axis] = high[axis].max(a.max(b));
                    }
                }
                let [left, top, right, bottom] = view.sprite_rect(*feet, low, high)?;
                let sprite = [left * screen()[0], top * screen()[1], right * screen()[0], bottom * screen()[1]];
                let half = world::CELL / 2.0;
                let corners: Option<Vec<_>> = [[-half, -half], [half, -half], [half, half], [-half, half]]
                    .iter()
                    .map(|[dx, dz]| view.project([feet[0] + dx, feet[1], feet[2] + dz]).map(to_screen))
                    .collect();
                let cell = corners.map_or(sprite, |corners| picking::bounds(&corners));
                let depth = (0..3).map(|axis| (feet[axis] - view.eye[axis]).powi(2)).sum::<f32>();
                let kind = match owner {
                    Owner::Player => picking::Kind::Player,
                    Owner::Item(_) => picking::Kind::Item,
                    Owner::Unit(id) => match game.units.get(id).map(|actor| actor.kind) {
                        Some(MONSTER) => picking::Kind::Monster,
                        Some(NPC) => picking::Kind::Npc,
                        Some(HOMUNCULUS) => picking::Kind::Homunculus,
                        // On a siege map players outside our guild rank with monsters.
                        Some(PLAYER) if siege_map && game.units.get(id).is_some_and(|actor| our_guild == 0 || actor.guild != our_guild) => picking::Kind::Foe,
                        Some(PLAYER) => picking::Kind::Player,
                        _ => picking::Kind::Other,
                    },
                };
                let min = if kind == picking::Kind::Item { picking::ITEM_MIN } else { picking::UNIT_MIN };
                Some(picking::Target { owner: *owner, kind, sprite, area: picking::area(sprite, cell, min), depth })
            })
            .collect();

        // Warp portals have no sprite: their pick box is drawn around the feet.
        for (&id, actor) in game.units.iter().filter(|(_, actor)| actor.job == WARP_JOB) {
            let feet = game.world.cell_position(actor.cell.map(|value| value as f32));
            if let Some([left, top, right, bottom]) = warp_pick_rect(&view, feet) {
                let cell = picking::bounds(&[to_screen([left, top]), to_screen([right, bottom])]);
                let depth = (0..3).map(|axis| (feet[axis] - view.eye[axis]).powi(2)).sum::<f32>();
                game.hit_boxes.push(picking::Target {
                    owner: Owner::Unit(id),
                    kind: picking::Kind::Other,
                    sprite: cell,
                    area: picking::area(cell, cell, picking::UNIT_MIN),
                    depth,
                });
            }
        }

        // The message sprite's frames for critical bursts and Lucky, posed while the sprite can still decode them.
        let message_frames: Vec<Vec<actor::Placed>> = match &mut game.message_sprite {
            Some(sprite) => game
                .damages
                .iter()
                .map(|damage| {
                    let elapsed = now.saturating_duration_since(damage.start).as_secs_f32();
                    match damage.kind {
                        _ if damage.start > now => Vec::new(),
                        DamageKind::Critical => {
                            let frames = sprite.frames(actor::BURST_ACTION).max(1) as f32;
                            sprite.frame_at(gpu, actor::BURST_ACTION, 0, Timing::Span((elapsed / 0.024 / 2.0).floor() / frames))
                        }
                        DamageKind::Lucky => sprite.frame_at(gpu, actor::LUCKY_ACTION, 0, Timing::Once(elapsed * 1000.0, 1.0)),
                        _ => Vec::new(),
                    }
                })
                .collect(),
            None => Vec::new(),
        };

        // Shadows first, so characters standing at the same depth draw over them.
        let shadow_placements = game.shadow.as_mut().map(|shadow| shadow.frame(gpu, 0, 0, 0, Timing::Loop(0.0))).unwrap_or_default();
        let mut billboards: Vec<Billboard> = Vec::new();
        if let Some(shadow) = &game.shadow {
            // On the ground under each unit, even one in the air (a falling item).
            let cast = |actor: &actor::Actor, scale: f32| {
                let factor = actor::shadow_factor(shadow_factors, actor.kind == PLAYER, actor.job, actor.option, actor.sitting, actor.dead);
                factor * scale
            };
            for (owner, feet, _) in &frames {
                let dead = matches!(owner, Owner::Unit(id) if game.units.get(id).is_some_and(|actor| actor.fading(now)));
                let scale = match owner {
                    Owner::Item(_) => ITEM_SHADOW,
                    Owner::Player => cast(&game.player, player_scale),
                    Owner::Unit(id) => {
                        game.units.get(id).map(|actor| cast(actor, unit_draw_scale(monster_sizes, actor.kind, actor.job, actor.size_percent))).unwrap_or(1.0)
                    }
                };
                if !dead && scale > 0.0 {
                    let ground = game.world.ground_under(*feet);
                    billboards.extend(shadow_placements.iter().map(|(key, position, size, angle)| {
                        (
                            shadow.texture(*key),
                            ground,
                            position.map(|value| value * scale),
                            size.map(|value| value * scale),
                            [1.0; 4],
                            *angle,
                            DepthLayer::default(),
                        )
                    }));
                }
            }
            // A cart's, a body's.
            for (_, feet, _) in &carts {
                billboards.extend(
                    shadow_placements
                        .iter()
                        .map(|(key, position, size, angle)| (shadow.texture(*key), *feet, *position, *size, [1.0; 4], *angle, DepthLayer::default())),
                );
            }
            // The warg's shadow is twice a body's. The falcon, up in the air, casts none.
            for draw in companion_draws.iter().filter(|draw| draw.warg) {
                let scale = actor::WARG_SHADOW;
                billboards.extend(shadow_placements.iter().map(|(key, position, size, angle)| {
                    (
                        shadow.texture(*key),
                        draw.feet,
                        position.map(|value| value * scale),
                        size.map(|value| value * scale),
                        [1.0; 4],
                        *angle,
                        DepthLayer::default(),
                    )
                }));
            }
        }
        let shadows = billboards.len();
        let sprites_of = |owner: &Owner| match owner {
            Owner::Player => game.player.sprites.as_ref(),
            Owner::Unit(id) => game.units[id].sprites.as_ref(),
            Owner::Item(id) => {
                let item = &game.ground[id];
                game.item_sprites.get(ground_sprite(items, item.item, item.identified)).and_then(Option::as_ref)
            }
        };
        // Spirit spheres, or coins for the Gunslinger line, circling the body at chest height: the far half of the
        // circle behind the body, the near half in front.
        let turned = now.duration_since(game.entered).as_secs_f32();
        let coin = game.coin_sprite.as_mut().map(|sprite| sprite.frame_at(gpu, 0, 0, Timing::Loop(turned * 1000.0))).unwrap_or_default();
        let mut spirits: [Vec<Billboard>; 2] = [Vec::new(), Vec::new()];
        let orbit_layer = |near: bool| DepthLayer { nudge: if near { IN_FRONT } else { -1.0 }, ..DepthLayer::default() };
        for (unit, orbs) in &game.spirits {
            let (feet, job) = match unit {
                None => (player_feet, game.me.job),
                Some(id) => match (feet_of.get(id), game.units.get(id)) {
                    (Some(feet), Some(actor)) => (*feet, actor.job),
                    _ => continue,
                },
            };
            // Shields at the waist, souls over the spheres, charms over the head.
            let (element, charms) = orbs.charms;
            let pictures = [
                (game.shield.as_ref(), orbs.shields, SHIELD_HEIGHT, SHIELD_SCALE, [1.0; 4]),
                (game.sphere.as_ref(), orbs.souls, SOUL_HEIGHT, SPHERE_SCALE, SOUL_TINT),
                (game.charm.as_ref(), charms, CHARM_HEIGHT, CHARM_SCALE, CHARM_TINTS[usize::from(element.clamp(1, 4) - 1)]),
            ];
            for (picture, count, height, scale, tint) in pictures {
                let Some(picture) = picture else { continue };
                let size = picture.size.map(|side| side * scale);
                for (center, near) in orbit(count, height, turned) {
                    spirits[usize::from(near)].push((
                        picture,
                        feet,
                        [center[0] - size[0] / 2.0, center[1] - size[1] / 2.0],
                        size,
                        tint,
                        0.0,
                        orbit_layer(near),
                    ));
                }
            }
            for (center, near) in orbit(orbs.spheres, SPIRIT_HEIGHT, turned) {
                let side = &mut spirits[usize::from(near)];
                if coin_job(job) {
                    if let Some(sprite) = &game.coin_sprite {
                        side.extend(coin.iter().map(|(key, position, size, angle)| {
                            (sprite.texture(*key), feet, [center[0] + position[0], center[1] + position[1]], *size, [1.0; 4], *angle, orbit_layer(near))
                        }));
                    }
                } else if let Some(sphere) = &game.sphere {
                    let size = sphere.size.map(|side| side * SPHERE_SCALE);
                    side.push((sphere, feet, [center[0] - size[0] / 2.0, center[1] - size[1] / 2.0], size, [1.0; 4], 0.0, orbit_layer(near)));
                }
            }
        }
        let [behind, in_front] = spirits;
        billboards.extend(behind);
        // Carts before the bodies, so the one pulling a cart draws over it.
        let cart_of = |owner: &Owner| match owner {
            Owner::Player => game.player.cart.as_ref().map(|cart| &cart.sprites),
            Owner::Unit(id) => game.units.get(id).and_then(|actor| actor.cart.as_ref()).map(|cart| &cart.sprites),
            Owner::Item(_) => None,
        };
        billboards.extend(carts.iter().flat_map(|(owner, feet, placements)| {
            let cart = cart_of(owner).unwrap();
            placements.iter().enumerate().map(move |(index, (key, position, size, angle))| {
                (cart.texture(*key), *feet, *position, *size, [1.0; 4], *angle, sprite_layer(index, false, false))
            })
        }));
        // The dead lie flat.
        let stand_of = |owner: &Owner| match owner {
            Owner::Player => (true, game.player.dead),
            Owner::Unit(id) => game.units.get(id).map_or((false, false), |actor| (actor.kind == PLAYER, actor.dead || actor.remove_at.is_some())),
            Owner::Item(_) => (false, false),
        };
        billboards.extend(frames.iter().flat_map(|(owner, feet, placements)| {
            let sprites = sprites_of(owner).unwrap();
            let tint = match owner {
                Owner::Unit(id) => [1.0, 1.0, 1.0, game.units.get(id).map_or(1.0, |actor| actor.alpha(now))],
                Owner::Item(id) if game.ground.get(id).is_some_and(|item| item_glints(now.saturating_duration_since(item.seen).as_secs_f32() / 0.024)) => {
                    [1.0, 1.0, 0.0, 1.0]
                }
                _ => [1.0; 4],
            };
            let (player, flat) = stand_of(owner);
            placements.iter().enumerate().map(move |(index, (key, position, size, angle))| {
                (sprites.texture(*key), *feet, *position, *size, tint, *angle, sprite_layer(index, player, flat))
            })
        }));
        for draw in &companion_draws {
            let actor = match draw.owner {
                Owner::Player => &game.player,
                Owner::Unit(id) => match game.units.get(&id) {
                    Some(actor) => actor,
                    None => continue,
                },
                Owner::Item(_) => continue,
            };
            let Some(sprites) = (if draw.warg { actor.warg.as_ref() } else { actor.falcon.as_ref() }).and_then(actor::Companion::sprites) else { continue };
            billboards.extend(draw.placed.iter().enumerate().map(|(index, (key, position, size, angle))| {
                (sprites.texture(*key), draw.feet, *position, *size, [1.0; 4], *angle, sprite_layer(index, false, false))
            }));
        }
        billboards.extend(in_front);
        // The emotion list's pictures, posed while the sprite can still decode them.
        let emotion_icons = match (&game.emotion_list, &mut game.emotion_sprite) {
            (Some(list), Some(sprite)) => list.icons(gpu, sprite),
            _ => Vec::new(),
        };
        // Taming's slot machine: its result played once, then held a moment.
        // The PvP counter while the map counts and a rank came, the first-place clock while we're first among more than
        // one, and the banner its first 3 s: each glyph's frame at its anchor.
        let counting = game.map_property.is_some_and(|(_, flags)| flags & net::MAP_COUNT_PK != 0);
        let glyph_frames =
            |font: &mut Option<Option<SpriteSet>>, path: &str, glyphs: Vec<(usize, [f32; 2])>, timing: Timing| -> Vec<(Vec<actor::Placed>, [f32; 2])> {
                match font.get_or_insert_with(|| SpriteSet::at(grf, path)) {
                    Some(font) => glyphs.into_iter().map(|(action, at)| (font.frame_at(gpu, action, 0, timing), at)).collect(),
                    None => Vec::new(),
                }
            };
        let counter = match game.pvp_rank.filter(|_| counting) {
            Some((rank, players)) => {
                glyph_frames(&mut game.rank_font, "data\\sprite\\이팩트\\rankfont", counter_glyphs(rank, players, screen()), Timing::Span(0.0))
            }
            None => Vec::new(),
        };
        let first_place = game.first_since.filter(|_| counting && game.pvp_rank.is_some_and(|(_, players)| players > 1));
        let clock = match first_place {
            Some(since) => {
                let glyphs = clock_glyphs(now.saturating_duration_since(since).as_secs());
                let left = screen()[0] / 2.0 - 20.0 * glyphs.len() as f32;
                let placed = glyphs.into_iter().enumerate().map(|(n, glyph)| (glyph, [left + n as f32 * 40.0, 40.0])).collect();
                glyph_frames(&mut game.time_font, "data\\sprite\\이팩트\\timefont", placed, Timing::Span(0.0))
            }
            None => Vec::new(),
        };
        let banner = match first_place.map(|since| now.saturating_duration_since(since)).filter(|elapsed| *elapsed < FIRST_PLACE_BANNER) {
            Some(elapsed) => glyph_frames(
                &mut game.time_attack,
                "data\\sprite\\이팩트\\timeattack",
                vec![(0, [screen()[0] / 2.0, 120.0])],
                Timing::Loop(elapsed.as_secs_f32() * 1000.0),
            ),
            None => Vec::new(),
        };
        let slot_frame = match (&mut game.slot_sprite, game.slot) {
            (Some(sprite), Some((action, start))) => {
                let length = sprite.index_ms(action).unwrap_or(1000.0).max(1.0);
                let elapsed = now.saturating_duration_since(start).as_secs_f32() * 1000.0;
                if elapsed > length + SLOT_HOLD_MS {
                    game.slot = None;
                    Vec::new()
                } else {
                    sprite.frame_at(gpu, action, 0, Timing::Span((elapsed / length).min(1.0)))
                }
            }
            _ => Vec::new(),
        };
        // Emotions play once over heads.
        let mut shown = Vec::new();
        if let Some(sprite) = &mut game.emotion_sprite {
            game.emotions.retain(|_, (action, start)| head_marks::playing(now.duration_since(*start), sprite.index_ms(*action).unwrap_or(emotion::SHOWN_MS)));
            for (unit, (action, start)) in &game.emotions {
                let feet = match unit {
                    None => player_feet,
                    Some(id) => match feet_of.get(id) {
                        Some(feet) => *feet,
                        None => continue,
                    },
                };
                let elapsed = now.duration_since(*start).as_secs_f32() * 1000.0;
                shown.push((feet, sprite.frame_at(gpu, *action, 0, Timing::Loop(elapsed))));
            }
        }
        // The MVP sprite plays once over the winner, where emotions hang, following them.
        let mut mvp_shown = Vec::new();
        if let Some(sprite) = &mut game.mvp_sprite {
            let length = sprite.index_ms(0).unwrap_or(emotion::SHOWN_MS);
            game.mvps.retain(|_, start| head_marks::playing(now.duration_since(*start), length));
            for (unit, start) in &game.mvps {
                let Some(feet) = unit.map_or(Some(player_feet), |id| feet_of.get(&id).copied()) else { continue };
                mvp_shown.push((feet, sprite.frame_at(gpu, 0, 0, Timing::Loop(now.duration_since(*start).as_secs_f32() * 1000.0))));
            }
        }
        // Status marks: stun stars, sleep, curse and silence's "..." over heads, the ice block at a frozen unit's feet.
        let looped = now.duration_since(game.entered).as_secs_f32() * 1000.0;
        let mut marked = Vec::new();
        let states = std::iter::once((player_feet, game.player.body_state, game.player.health_state))
            .chain(game.units.iter().filter_map(|(id, actor)| feet_of.get(id).map(|&feet| (feet, actor.body_state, actor.health_state))));
        for (feet, body, health) in states {
            for mark in head_marks::marks(body, health) {
                let anchor = head_marks::anchor(feet, mark.height());
                match mark.sprite() {
                    Some(name) => {
                        if let Some(sprite) = game.mark_sprites.get_mut(name) {
                            marked.push((Some(name), anchor, sprite.frame_at(gpu, 0, 0, Timing::Loop(looped))));
                        }
                    }
                    None => {
                        if let (Some(sprite), Some(action)) = (&mut game.emotion_sprite, emotion::sprite_action(head_marks::SILENCE_EMOTION)) {
                            marked.push((None, anchor, sprite.frame_at(gpu, action, 0, Timing::Loop(looped))));
                        }
                    }
                }
            }
        }
        if let Some(sprite) = &game.emotion_sprite {
            billboards.extend(shown.into_iter().flat_map(|(feet, placements)| {
                let anchor = head_marks::anchor(feet, head_marks::ANCHOR);
                placements.into_iter().map(move |(key, position, size, angle)| {
                    (sprite.texture(key), anchor, position, size, [1.0; 4], angle, DepthLayer { nudge: IN_FRONT, ..DepthLayer::default() })
                })
            }));
        }
        if let Some(sprite) = &game.mvp_sprite {
            billboards.extend(mvp_shown.into_iter().flat_map(|(feet, placements)| {
                let anchor = head_marks::anchor(feet, head_marks::ANCHOR);
                placements.into_iter().map(move |(key, position, size, angle)| {
                    (sprite.texture(key), anchor, position, size, [1.0; 4], angle, DepthLayer { nudge: IN_FRONT, ..DepthLayer::default() })
                })
            }));
        }
        for (name, anchor, placements) in marked {
            let sprite = match name {
                Some(name) => game.mark_sprites.get(name),
                None => game.emotion_sprite.as_ref(),
            };
            if let Some(sprite) = sprite {
                billboards.extend(placements.into_iter().map(|(key, position, size, angle)| {
                    (sprite.texture(key), anchor, position, size, [1.0; 4], angle, DepthLayer { nudge: IN_FRONT, ..DepthLayer::default() })
                }));
            }
        }

        // Numbers over units: hits arc and drift away from where the target faces, shrinking and fading; MISS rises
        // over the attacker; heals rise slowly; a multi-hit's total pops up high (the newest only); a critical's burst
        // follows its number; Lucky plays once over the dodger.
        if let Some(digits) = &game.digits {
            let mut newest_combo: HashMap<Option<u32>, Instant> = HashMap::new();
            for damage in game.damages.iter().filter(|damage| damage.start <= now && damage.kind == DamageKind::Combo) {
                let newest = newest_combo.entry(damage.target).or_insert(damage.start);
                *newest = (*newest).max(damage.start);
            }
            let message_sprite = game.message_sprite.as_ref();
            for (index, damage) in game.damages.iter().enumerate().filter(|(_, damage)| damage.start <= now) {
                let Some(feet) = damage.target.map_or(Some(player_feet), |id| feet_of.get(&id).copied()) else { continue };
                let elapsed = now.duration_since(damage.start).as_secs_f32();
                let posed = message_frames.get(index).map(Vec::as_slice).unwrap_or_default();
                if elapsed >= damage.kind.duration(damage.amount) {
                    continue;
                }
                let ticks = elapsed / 0.024;
                let at = |motion: &actor::NumberMotion| {
                    let [east, north] = damage.drift.map(|sign| sign * motion.drift);
                    [feet[0] + east, feet[1] + motion.height, feet[2] + north]
                };
                // Digits stand a step in front of a critical's burst behind them.
                let digits_layer = DepthLayer { nudge: 1.0, ..DepthLayer::default() };
                let number = |motion: actor::NumberMotion, color: [f32; 3]| {
                    let tint = [color[0], color[1], color[2], motion.alpha];
                    let anchor = at(&motion);
                    digits
                        .place(damage.amount, motion.scale)
                        .into_iter()
                        .map(move |(texture, position, size)| (texture, anchor, position, size, tint, 0.0, digits_layer))
                };
                match damage.kind {
                    DamageKind::Hit { taken, sp } => {
                        let color = if sp {
                            [0x1E as f32 / 255.0, 0x90 as f32 / 255.0, 1.0]
                        } else if taken {
                            [1.0, 0.0, 0.0]
                        } else {
                            [1.0; 3]
                        };
                        billboards.extend(number(actor::hit_number(ticks, false), color));
                    }
                    DamageKind::Critical => {
                        if let Some(sprite) = message_sprite {
                            let motion = actor::burst(ticks);
                            let tint = [0xAA as f32 / 255.0, 0xAA as f32 / 255.0, 0xAA as f32 / 255.0, motion.alpha];
                            let anchor = at(&motion);
                            billboards.extend(posed.iter().map(|&(key, position, size, angle)| {
                                (
                                    sprite.texture(key),
                                    anchor,
                                    position.map(|value| value * motion.scale),
                                    size.map(|value| value * motion.scale),
                                    tint,
                                    angle,
                                    DepthLayer::default(),
                                )
                            }));
                        }
                        billboards.extend(number(actor::hit_number(ticks, true), [1.0, 1.0, 0.0]));
                    }
                    DamageKind::Miss { red } => {
                        let motion = actor::miss_number(ticks);
                        let size = digits.miss.size.map(|value| value * motion.scale);
                        let color = if red { [1.0, 0.0, 0.0, motion.alpha] } else { [1.0, 1.0, 1.0, motion.alpha] };
                        billboards.push((&digits.miss, at(&motion), [-size[0] / 2.0, -size[1] / 2.0], size, color, 0.0, DepthLayer::default()));
                    }
                    DamageKind::Lucky => {
                        if let Some(sprite) = message_sprite {
                            let anchor = [feet[0], feet[1] + actor::LUCKY_HEIGHT, feet[2]];
                            billboards.extend(
                                posed
                                    .iter()
                                    .map(|&(key, position, size, angle)| (sprite.texture(key), anchor, position, size, [1.0; 4], angle, DepthLayer::default())),
                            );
                        }
                    }
                    DamageKind::Heal { sp } => billboards.extend(number(actor::heal_number(ticks), if sp { [0.0, 0.0, 1.0] } else { [0.0, 1.0, 0.0] })),
                    DamageKind::Combo => {
                        if newest_combo.get(&damage.target) == Some(&damage.start) {
                            billboards.extend(number(actor::total_number(ticks), [1.0, 1.0, 0.0]));
                        }
                    }
                }
            }
        }

        let viewed_preview: Vec<Sprite> = match (viewed_feet, game.viewing.as_ref().and_then(|viewed| viewed.sprites.as_ref())) {
            (Some(feet), Some(sprites)) => {
                viewed_placements.iter().map(|(key, position, size, _)| (sprites.texture(*key), offset(feet, *position), *size)).collect()
            }
            _ => Vec::new(),
        };
        let preview: Vec<Sprite> = match (preview_feet, &game.player.sprites) {
            (Some(feet), Some(sprites)) => {
                preview_placements.iter().map(|(key, position, size, _)| (sprites.texture(*key), offset(feet, *position), *size)).collect()
            }
            _ => Vec::new(),
        };

        // A monster or another player under the pointer shows its name at its feet (NPCs show theirs always). A
        // player's adds their party, and a line with their guild and position after the guild's emblem (drawn with the
        // names below); the server tells those each time the pointer comes onto them, as what it sends unasked can be
        // stale.
        let mut guild_line = None;
        let mut title_line = None;
        let pointed = match hovered(&game.hit_boxes, pointer.0, players_first) {
            Some(Owner::Unit(id)) if game.units.get(&id).is_some_and(|actor| !actor.hidden && actor.kind != NPC && actor.sprites.is_some()) => Some(id),
            _ => None,
        };
        if let Some(id) = pointed
            && let Some(actor) = game.units.get(&id)
            && let Some(&feet) = feet_of.get(&id)
        {
            let tag = game.name_tags.get(&id).filter(|_| actor.kind == PLAYER);
            let name = match tag {
                Some(tag) if !tag.party.is_empty() => format!("{} ({})", actor.name, tag.party),
                _ => actor.name.clone(),
            };
            // ponytail: the title over the name; where the real client shows it isn't known.
            if let Some(title) = tag.and_then(|tag| title_names.get(&tag.title)) {
                title_line = Some((format!("[{title}]"), feet));
            }
            if let Some(tag) = tag.filter(|tag| !tag.guild.is_empty()) {
                guild_line = Some((name.clone(), format!("{} [{}]", tag.guild, tag.position), feet, actor.guild));
            }
            let color = name_color(actor.kind, actor.job, actor.level, our_summons.contains(&Some(id)), own_level);
            labels.push((name, feet, color));
            if actor.kind == PLAYER && game.pointed != pointed {
                let _ = game.session.ask_name(id);
            }
        }
        game.pointed = pointed;
        // The item under the pointer shows its name and amount.
        if let Some(Owner::Item(id)) = hovered(&game.hit_boxes, pointer.0, players_first)
            && let Some(item) = game.ground.get(&id)
        {
            let name = inventory::item_name(items, item.item, item.identified, 0);
            labels.push((messages::fill(message(messages::ITEM_AMOUNT), &[&name, &item.amount]), game.world.cell_position(item.cell), ITEM_NAME_COLOR));
        }

        // In-world overlays: under the feet (in sprite pixels, the way sprites are drawn) HP bars and names under them;
        // over the head cast bars and what's said.
        let sprite_point = |feet: [f32; 3], y: f32| view.sprite_rect(feet, [0.0, y], [0.0, y]).map(|[x, y, ..]| [x * screen()[0], y * screen()[1]]);
        let mut base = Layer::default();
        for (name, feet, color) in &labels {
            let name = name.split('#').next().unwrap_or_default();
            if let Some(point) = sprite_point(*feet, overhead::NAME_BELOW) {
                let [x, y] = overhead::label_corner(point, font.width(name), font.line_height(), screen());
                base.outlined_in(name, [x, y + overhead::NAME_TEXT_TOP], *color);
            }
        }
        // ponytail: a player's title, then their guild, on lines under the name; where titles go isn't known.
        let mut under_name = 1.0;
        if let Some((title, feet)) = &title_line
            && let Some(point) = sprite_point(*feet, overhead::NAME_BELOW)
        {
            let [x, y] = overhead::label_corner(point, font.width(title), font.line_height(), screen());
            base.outlined(title, [x, y + overhead::NAME_TEXT_TOP + font.line_height()]);
            under_name += 1.0;
        }
        if let Some((name, line, feet, guild_id)) = &guild_line
            && let Some(point) = sprite_point(*feet, overhead::NAME_BELOW)
        {
            let corner = |text: &str| overhead::label_corner(point, font.width(text), font.line_height(), screen());
            let top = corner(name)[1] + overhead::NAME_TEXT_TOP + under_name * font.line_height();
            base.outlined(line, [corner(line)[0], top]);
            // ponytail: the emblem left of the guild line; calibrate against the real client.
            if let Some(emblem) = game.emblems.get(guild_id).and_then(|(_, emblem)| emblem.as_ref()) {
                base.sprites.push((emblem, [corner(line)[0] - guild::EMBLEM_SIZE[0] - 2.0, top], guild::EMBLEM_SIZE));
            }
        }
        // ponytail: nothing says how camps look; a square in the camp's color over each player's head, and the score
        // as "A : B" under the minimap (the top middle is the announcements').
        if let (Some(ours), Some((_, marks))) = (game.battle.camp(game.account_id), &game.camp_looks) {
            for (id, camp) in &game.battle.camps {
                let Some(above) = feet_of.get(id).and_then(|&feet| sprite_point(feet, -100.0)) else { continue };
                let mark = &marks[usize::from(*camp != ours)];
                base.sprites.push((mark, [above[0] - 4.0, above[1] - 12.0], [8.0, 8.0]));
            }
        }
        if let Some([a, b]) = game.battle.score {
            let score = format!("{a} : {b}");
            base.outlined(&score, [(screen()[0] - 12.0 - font.width(&score)).floor(), 166.0]);
        }
        // On a siege map (not a battleground) other players in a guild, and the guardians, carry its emblem over their
        // heads; it follows the unit but doesn't grow with the zoom.
        game.head_emblems.clear();
        if game.map_property.is_some_and(|(_, flags)| versus::siege(flags)) {
            for (id, actor) in &game.units {
                let wears = guild::wears_head_emblem(actor.kind == PLAYER, actor.job, actor.guild, actor.cloaked || actor.hidden, *id == game.account_id);
                let Some(&feet) = feet_of.get(id).filter(|_| wears) else { continue };
                let (Some(emblem), Some(at_feet), Some(above)) =
                    (game.emblems.get(&actor.guild).and_then(|(_, emblem)| emblem.as_ref()), sprite_point(feet, 0.0), sprite_point(feet, -100.0))
                else {
                    continue;
                };
                // The unit's perspective factor, from the screen pixels a sprite pixel takes at its feet.
                let perspective = guild::perspective((at_feet[1] - above[1]) / 100.0, screen()[1]);
                let [left, top, right, bottom] = guild::head_emblem_box(at_feet, screen(), perspective);
                base.sprites.push((emblem, [left.round(), top.round()], guild::EMBLEM_SIZE));
                game.head_emblems.push((actor.name.clone(), actor.guild, [left, top, right, bottom]));
            }
        }
        let status = |id| game.status.get(&id).copied().unwrap_or(0);
        let [border, background, green, red, blue, hunger, third] = &assets.bars;
        // Each bar: whose, where, its rows (value, maximum, color, low color), its border and background.
        let mut bars = Vec::new();
        if game.show_bars {
            let mut rows =
                vec![(status(net::STATUS_HP), status(net::STATUS_MAX_HP), green, red), (status(net::STATUS_SP), status(net::STATUS_MAX_SP), blue, blue)];
            // ponytail: a 4th job's third row taken as its AP.
            if basic_info::fourth_job(game.me.job) {
                rows.push((status(net::STATUS_AP), status(net::STATUS_MAX_AP), third, third));
            }
            bars.push(("player".to_string(), player_feet, rows, [border, background], overhead::bar_fill as overhead::Fill));
        }
        // The player's own summons while in sight: HP, SP, and a homunculus's or pet's hunger.
        // ponytail: hunger fills over a maximum of 100.
        let pair = |value: u32, max: u32| (i64::from(value), i64::from(max));
        let hp_sp = |[hp, max_hp, sp, max_sp]: [u32; 4]| {
            let ((hp, max_hp), (sp, max_sp)) = (pair(hp, max_hp), pair(sp, max_sp));
            vec![(hp, max_hp, green, red), (sp, max_sp, blue, blue)]
        };
        let mut summons = Vec::new();
        if let (Some(id), Some(homunculus)) = (game.homunculus_id, &game.homunculus) {
            let mut rows = hp_sp([homunculus.hp, homunculus.max_hp, homunculus.sp, homunculus.max_sp]);
            rows.push((i64::from(homunculus.hunger), 100, hunger, red));
            summons.push(("homunculus", id, rows));
        }
        if let Some(hired) = &game.mercenary {
            summons.push(("mercenary", hired.id, hp_sp([hired.hp, hired.max_hp, hired.sp, hired.max_sp])));
        }
        if let Some((id, values)) = game.elemental {
            summons.push(("elemental", id, hp_sp(values)));
        }
        if let (Some(id), Some(pet)) = (game.pet_id, &game.pet) {
            summons.push(("pet", id, vec![(i64::from(pet.hunger), 100, hunger, red)]));
        }
        for (kind, id, rows) in summons {
            if let Some(&feet) = feet_of.get(&id) {
                bars.push((kind.to_string(), feet, rows, [border, background], overhead::bar_fill));
            }
        }
        // Monsters' bars in their own colors; a dying unit's bar shows empty until it's gone. Party members and
        // battleground teammates fill by whole percents.
        bars.extend(game.unit_hp.iter().filter_map(|(id, &(hp, max))| {
            let actor = game.units.get(id)?;
            let hp = if actor.remove_at.is_some() { 0 } else { hp };
            let monster = actor.kind == MONSTER;
            let [border, background, fill, low] = if monster { assets.monster_bars.each_ref() } else { [border, background, green, red] };
            let filled: overhead::Fill = if monster { overhead::bar_fill } else { overhead::party_fill };
            Some((id.to_string(), *feet_of.get(id)?, vec![(i64::from(hp), i64::from(max), fill, low)], [border, background], filled))
        }));
        game.bars_drawn.clear();
        for (whose, feet, rows, [border, background], filled) in bars {
            let Some(point) = sprite_point(feet, overhead::BAR_BELOW) else { continue };
            let [x, y] = [point[0].trunc() - overhead::BAR_WIDTH / 2.0, point[1].trunc()];
            let rects = overhead::gage(&rows, border, background, filled);
            game.bars_drawn.push((whose, [x, y, rects[0].1[2], rects[0].1[3]], rows.iter().map(|&(value, max, ..)| filled(value, max)).collect()));
            for (texture, [left, top, width, height]) in rects {
                base.sprites.push((texture, [x + left, y + top], [width, height]));
            }
        }

        // Over heads: cast bars filling green, and the words said there.
        game.casts.retain(|_, (start, length)| now.duration_since(*start) < *length);
        game.speech.retain(|_, (.., said)| now.duration_since(*said) < SPEECH_TIME);
        game.talkie_spots.retain(|id, _| game.speech.contains_key(&Some(*id)));
        let head = |unit: &Option<u32>| -> Option<[f32; 2]> {
            let feet = match unit {
                None => player_feet,
                Some(id) => *feet_of.get(id)?,
            };
            sprite_point(feet, -overhead::HEAD).map(|point| point.map(f32::round))
        };
        let [border, background, ..] = &assets.bars;
        for (unit, (start, length)) in &game.casts {
            let feet = match unit {
                None => player_feet,
                Some(id) => match feet_of.get(id) {
                    Some(&feet) => feet,
                    None => continue,
                },
            };
            let Some(top) = sprite_point(feet, -overhead::CAST_ABOVE) else { continue };
            let [left, y, width, height] = overhead::cast_rect(top);
            let done = now.duration_since(*start).as_secs_f32() / length.as_secs_f32();
            base.sprites.push((border, [left, y], [width, height]));
            base.sprites.push((background, [left + 1.0, y + 1.0], [width - 2.0, height - 2.0]));
            base.sprites.push((&assets.cast_fill, [left + 1.0, y + 1.0], [overhead::cast_fill(done), height - 2.0]));
        }
        // Combo windows: a bar under the cast bar's place, emptying as the window runs out.
        // ponytail: how the official client shows a combo window isn't knowable from rathena.
        game.combos.retain(|_, (start, length)| now.duration_since(*start) < *length);
        for (unit, &(start, length)) in &game.combos {
            let Some([x, y]) = head(unit) else { continue };
            let [left, top] = [x - overhead::CAST_BAR[0] / 2.0, y + overhead::CAST_BAR[1] + 1.0];
            base.sprites.push((border, [left, top], overhead::CAST_BAR));
            base.sprites.push((background, [left + 1.0, top + 1.0], [overhead::CAST_BAR[0] - 2.0, overhead::CAST_BAR[1] - 2.0]));
            base.sprites.push((
                &assets.combo_fill,
                [left + 1.0, top + 1.0],
                [((overhead::CAST_BAR[0] - 2.0) * share_left(start, length, now)).round(), overhead::CAST_BAR[1] - 2.0],
            ));
        }
        // Navigation: there is the end of it; else the trail to where it heads on this map, and the arrow over the head.
        let here = navigation::map_key(&game.map);
        if game.navigation.as_ref().is_some_and(|active| navigation::arrived(&here, game.player.cell, &active.target)) {
            game.navigation = None;
        }
        let navi_next = game.navigation.as_ref().and_then(|active| navigation::next_cell(&self.navi_tables, &here, &active.target, active.services));
        if let Some(active) = &mut game.navigation {
            let gat = &game.world.gat;
            active.update_trail(&here, game.player.cell, navi_next, |from, to| world::find_path(gat, from, to));
        }
        if let Some(next) = navi_next {
            // A point a few cells toward it, so a far goal off the screen still turns the arrow.
            let toward = [f32::from(next[0]) - game.player.cell[0] as f32, f32::from(next[1]) - game.player.cell[1] as f32];
            let length = toward[0].hypot(toward[1]).max(1.0);
            let near = [0, 1].map(|axis| game.player.cell[axis] as f32 + toward[axis] / length * NAVI_ARROW_LOOK.min(length));
            let goal = game.world.cell_position(near);
            if let (Some(from), Some(to), Some([x, y])) = (sprite_point(player_feet, 0.0), sprite_point(goal, 0.0), head(&None)) {
                let arrow = &assets.navi_arrows[navigation::arrow([to[0] - from[0], to[1] - from[1]])];
                base.sprites.push((arrow, [x - arrow.size[0] / 2.0, y - NAVI_ARROW_RISE - arrow.size[1]], arrow.size));
            }
        }
        // Chat room signs over their owners' heads, like speech (not the room we're in).
        game.sign_rects.clear();
        for (owner, sign) in &game.signs {
            if game.room.as_ref().is_some_and(|room| room.sign.id == sign.id) {
                continue;
            }
            let unit = (*owner != game.account_id).then_some(*owner);
            let feet = match unit {
                None => player_feet,
                Some(id) => match feet_of.get(&id) {
                    Some(&feet) => feet,
                    None => continue,
                },
            };
            let Some(point) = sprite_point(feet, -chatroom::SIGN_ABOVE) else { continue };
            // ponytail: the one yellow shop board is the vendor whose shop window is open.
            let picked = game.shop.as_ref().and_then(|shop| shop.vendor.as_ref()).is_some_and(|(vendor, ..)| vendor == owner);
            let rect = chatroom::draw_sign(&mut base, &assets.chatroom, font, sign, point.map(f32::round), picked);
            game.sign_rects.push((rect, sign.id, sign.kind));
        }
        for (unit, (text, color, _)) in &game.speech {
            let Some(anchor) = head(unit) else { continue };
            let lines = overhead::lines(text);
            let size = overhead::size(lines.iter().map(|line| font.width(line)).fold(0.0, f32::max), lines.len());
            let corner = overhead::corner(anchor, size, screen());
            base.sprites.push((&assets.speech_fill, corner, size));
            for (edge, piece) in overhead::border(size) {
                base.sprites.push((&assets.speech_border, offset(corner, edge), piece));
            }
            for (row, line) in lines.into_iter().enumerate() {
                let place = offset(corner, [overhead::TEXT_AT[0], overhead::TEXT_AT[1] + row as f32 * overhead::LINE_STEP]);
                base.texts.push((line.clone(), offset(place, [1.0, 1.0]), [0, 0, 0]));
                base.texts.push((line, place, *color));
            }
        }

        // NPC windows on top.
        let mut overlay = Layer::default();
        let mut dialog_links = Vec::new();
        if let Some(dialog) = &game.dialog {
            // A warp list is the menu alone.
            if dialog.warp.is_none() {
                let position = npc_box_position();
                overlay.sprites.push((&assets.dialog, position, assets.dialog.size));
                let mut lines: links::LinkedLines = dialog.lines.iter().flat_map(|line| wrap_linked(font, line, DIALOG_TEXT_WIDTH)).collect();
                if let Some((text, value)) = &dialog.input {
                    lines.push(vec![([0, 0, 0], format!("{}: {value}_", if *text { "입력" } else { "수량" }), None)]);
                }
                let shown = lines.len().saturating_sub(DIALOG_LINES);
                for (row, pieces) in lines[shown..].iter().enumerate() {
                    let mut x = position[0] + DIALOG_TEXT[0];
                    let y = position[1] + DIALOG_TEXT[1] + row as f32 * DIALOG_LINE;
                    for (color, piece, item) in pieces {
                        let width = font.width(piece);
                        overlay.texts.push((piece.clone(), [x, y], *color));
                        // Item links underlined, and clickable.
                        if let Some(item) = item {
                            let line = if item.item().is_some() { &assets.link_line } else { &assets.navi_line };
                            overlay.sprites.push((line, [x, y + font.line_height() - 1.0], [width, 1.0]));
                            dialog_links.push(([x, y, width, font.line_height()], item.clone()));
                        }
                        x += width;
                    }
                }
                match dialog.button {
                    DialogButton::Next => overlay.sprites.push(button_sprite(pointer, offset(position, DIALOG_BUTTON), &assets.next)),
                    DialogButton::Close => overlay.sprites.push(button_sprite(pointer, offset(position, DIALOG_BUTTON), &assets.close)),
                    DialogButton::None => {}
                }
            }
            if let Some((items, selected)) = &dialog.menu {
                let menu = menu_position();
                overlay.sprites.push((&assets.menu, menu, assets.menu.size));
                let list = offset(menu, [MENU_CONTENT[0] as f32, MENU_CONTENT[1] as f32]);
                let first = menu_first_row(*selected, items.len());
                for (row, item) in items.iter().enumerate().skip(first).take(MENU_ROWS) {
                    let top = offset(list, [0.0, (row - first) as f32 * MENU_ROW[1]]);
                    if row == *selected {
                        overlay.sprites.push((&assets.menu_selection, top, MENU_ROW));
                    }
                    let mut x = top[0] + 5.0;
                    for (color, piece) in wrap_colored(font, item, MENU_ROW[0]).into_iter().next().unwrap_or_default() {
                        overlay.texts.push((piece.clone(), [x, top[1] + 3.0], color));
                        x += font.width(&piece);
                    }
                }
                overlay.sprites.push(button_sprite(pointer, offset(menu, MENU_OK), &assets.ok));
                overlay.sprites.push(button_sprite(pointer, offset(menu, MENU_CANCEL), &assets.cancel));
            }
        }

        // Windows over the world, NPC conversations over windows, tooltips over everything.
        let shown_items = game
            .inventory
            .items
            .iter()
            .chain(game.cart.items.iter().filter(|_| game.cart.frame.open))
            .map(|item| (item.id, item.identified))
            .chain(game.hotkeys.keys.iter().flatten().filter(|key| !key.skill).map(|key| (key.id, true)))
            .chain(game.viewing.iter().flat_map(|viewed| viewed.items.iter()).map(|item| (item.id, item.identified)));
        for (id, identified) in shown_items {
            if let Some(info) = items.get(&id) {
                let path = inventory::icon_path(info, identified);
                icons.entry(path.clone()).or_insert_with(|| gpu.try_load_texture(grf, &path, false).ok());
            }
        }
        let jobs = skills::tabs(skill_trees, game.me.job, &game.skills);
        let points = game.skill_points();
        let sheet = skills::Sheet { trees: skill_trees, jobs: &jobs, skills: &game.skills, info: skill_info, points };
        let skill_icons = game
            .skill_window
            .icons(&sheet)
            .into_iter()
            .chain(game.guild.as_ref().map(|guild| game.guild_window.icons(guild, skill_info)).unwrap_or_default())
            .chain(mercenary::SkillList::icons(&game.mercenary_skills, skill_info))
            .chain(homunculus::SkillList::icons(&game.homunculus_skills, skill_info))
            .chain(game.hotkeys.keys.iter().flatten().filter(|key| key.skill).filter_map(|key| skill_info.get(&(key.id as u16)).map(skills::icon_path)));
        let status_paths =
            game.status_icons.iter().filter_map(|(kind, ..)| status_icon_files.get(kind).map(|(_, file)| format!("data\\texture\\effect\\{file}")));
        let mut shop_icons = game.shop.as_ref().map(|shop| shop.icons(items)).unwrap_or_default();
        shop_icons.extend(game.cashshop.as_ref().map(|shop| shop.icons(items)).unwrap_or_default());
        let mut pick_icons = game.pick.as_ref().map(|(list, _)| list.icons(items, skill_info)).unwrap_or_default();
        pick_icons.extend(game.refine.as_ref().map(|window| window.icons(items, &game.inventory.items)).unwrap_or_default());
        pick_icons.extend(game.reform.as_ref().map(|window| window.icons(items)).unwrap_or_default());
        pick_icons.extend(game.grade.as_ref().map(|window| window.icons(items, &game.inventory.items)).unwrap_or_default());
        pick_icons.extend(game.enchant.as_ref().map(|window| window.icons(items, &game.inventory.items)).unwrap_or_default());
        pick_icons.extend(game.materials.as_ref().map(|window| window.icons(items, &game.inventory.items)).unwrap_or_default());
        pick_icons.extend(game.synthesis.as_ref().map(|window| window.icons(items, &game.inventory.items)).unwrap_or_default());
        pick_icons.extend(game.upgrade.as_ref().map(|window| window.icons(items, &game.inventory.items)).unwrap_or_default());
        pick_icons.extend(game.achievement_window.icons(achievement_info));
        pick_icons.extend(game.roulette.as_ref().map(|window| window.icons(items)).unwrap_or_default());
        let compared = compared_windows(&game.inventory, items, messages, pointer.0);
        let mut trade_icons = game.trade.as_ref().map(|trade| trade.icons(items)).unwrap_or_default();
        trade_icons.extend(game.item_window.as_ref().map(|window| window.images(items)).unwrap_or_default());
        trade_icons.extend(compared.iter().flat_map(|window| window.images(items)));
        trade_icons.extend(game.card_view.iter().map(|view| view.path.clone()));
        if game.vend_setup.is_some() {
            trade_icons.extend(vend::icons(items, game.cart.items.iter().map(|item| (item.id, item.identified))));
        }
        if let Some(shop) = &game.my_shop {
            trade_icons.extend(vend::icons(items, shop.goods.iter().map(|good| (good.item.id, good.item.identified))));
        }
        if game.buying_setup.is_some() {
            trade_icons.extend(vend::icons(items, game.inventory.items.iter().map(|item| (item.id, item.identified))));
        }
        if let Some(store) = &game.my_buying_store {
            trade_icons.extend(vend::icons(items, store.items.iter().map(|item| (item.item_id, true))));
        }
        trade_icons.extend(game.mail.icons(items));
        trade_icons.extend(game.pickup_popup.as_ref().map(|popup| popup.icon.clone()).filter(|icon| !icon.is_empty()));
        trade_icons.extend(game.catalog.as_ref().map(|catalog| catalog.icons(items)).unwrap_or_default());
        for path in skill_icons.chain(status_paths).chain(shop_icons).chain(pick_icons).chain(game.storage.icons(items)).chain(trade_icons) {
            icons.entry(path.clone()).or_insert_with(|| gpu.try_load_texture(grf, &path, false).ok());
        }
        let mut tooltip = Layer::default();
        let mut chat_layer = Layer::default();
        let open = game.chat_mode.chat_line_open();
        let caret = open && (game.entered.elapsed().as_millis() / CARET_BLINK.as_millis()).is_multiple_of(2);
        let name_of = |item: &links::ItemLink| inventory::item_name(items, item.id, true, item.refine);
        // The basic info and chat windows are kept as they're left.
        game.basic_info.save_if_changed();
        game.chat.save_if_changed();
        game.chat_links = game.chat.draw(&mut chat_layer, &assets.chat, font, messages, open, caret, &name_of);
        // The input bar buttons' tooltips, over the pointer.
        if let Some(text) = game.chat.tooltip(pointer.0).and_then(|id| messages.get(id)) {
            let size = [font.width(text) + 8.0, font.line_height() + 4.0];
            let corner = [pointer.0[0] - size[0] / 2.0, pointer.0[1] - size[1] - 6.0];
            tooltip.sprites.push((&assets.speech_fill, corner, size));
            for (edge, piece) in overhead::border(size) {
                tooltip.sprites.push((&assets.speech_border, offset(corner, edge), piece));
            }
            tooltip.texts.push((text.clone(), offset(corner, [4.0, 2.0]), [0, 0, 0]));
        }
        let size = [game.world.gat.width as u32, game.world.gat.height as u32];
        // NPC marks last until they're taken down. Each blinks from the moment it was set.
        game.marks.retain(|(_, _, _, until, _)| until.is_none_or(|until| until > now));
        // The pickup popup's five seconds; an open inventory has been seen.
        if game.pickup_popup.as_ref().is_some_and(|popup| !popup.alive(control::now())) {
            game.pickup_popup = None;
        }
        if game.inventory.frame.open {
            game.new_items = false;
        }
        if game.quest_window.frame.open {
            game.quest_icon = false;
        }
        game.basic_info.fourth = basic_info::fourth_job(game.me.job);
        game.basic_info.doram = basic_info::doram(game.me.job);
        game.banners.current(control::now());
        game.level_ups.seen(level_up::opened(&game.status_window.frame), level_up::opened(&game.skill_window.frame));
        let marks: Vec<_> = game
            .marks
            .iter()
            .filter(|(_, _, _, _, set)| minimap::mark_visible(0, now.saturating_duration_since(*set).as_millis()))
            .map(|(_, cell, color, _, _)| (*cell, color))
            .collect();
        // Party members on this map, coloured by their place in the party list.
        if game.party_colors.is_empty() {
            game.party_colors = (0..16).map(|place| gpu.solid(minimap::party_color(place))).collect();
        }
        let here = game.map.trim_end_matches(".gat").to_owned();
        let members: Vec<_> = game
            .party
            .iter()
            .flat_map(|party| party.members.iter().enumerate())
            .filter(|(_, member)| member.info.account != game.account_id && member.info.online && member.info.map.trim_end_matches(".gat") == here)
            .filter_map(|(place, member)| Some((place, member.info.account, member.cell?)))
            .collect();
        // On a battleground, teammates in our camp's color.
        let mut dots: Vec<_> = members.iter().map(|&(place, _, cell)| (cell, &game.party_colors[place.min(15)])).collect();
        if let Some((team_dot, _)) = &game.camp_looks {
            dots.extend(game.battle.dots.values().map(|&[x, y]| ([i32::from(x), i32::from(y)], team_dot)));
        }
        // Guild members not in the party.
        let in_party = |account: &u32| members.iter().any(|(_, member, _)| member == account);
        let guild: Vec<[i32; 2]> =
            game.guild_places.iter().filter(|(account, _)| !in_party(account) && **account != game.account_id).map(|(_, cell)| *cell).collect();
        let boss = game.boss.as_ref().map(|(_, cell)| *cell);
        // The overview's window, cut again only when the zoom, the window or the view changed.
        let overview = match (&game.minimap, game.minimap_view) {
            (_, minimap::View::Hidden) | (None, _) => None,
            (Some(image), view) => {
                let window = minimap::window(minimap::share(game.player.cell, size), game.minimap_zoom);
                let key = minimap::crop_key(image, window, game.minimap_zoom, view);
                if game.minimap_crop.as_ref().is_none_or(|(cut, _)| *cut != key) {
                    let part = minimap::crop(image, window, game.minimap_zoom, view.alpha());
                    game.minimap_crop = Some((key, gpu.texture(&part, true)));
                }
                game.minimap_crop.as_ref().map(|(_, texture)| texture)
            }
        };
        let trail = game.navigation.as_ref().map_or(&[][..], |active| active.trail.as_slice());
        let goal = navi_next.map(|cell| cell.map(i32::from));
        let markers = minimap::Markers { cell: game.player.cell, yaw, marks: &marks, party: &dots, guild: &guild, boss, trail, goal };
        minimap::draw(&mut chat_layer, &assets.minimap, overview, size, game.minimap_zoom, &markers, minimap::Pointer { at: pointer.0, down: pointer.1 });
        if !game.minimap_window.covers(pointer.0)
            && game.window_at(pointer.0).is_none()
            && let Some(name) = minimap::button_at(pointer.0)
        {
            let text = messages.get(minimap::tooltip_id(name)).map(String::as_str).unwrap_or("");
            if !text.is_empty() {
                window::tooltip(&mut tooltip, &assets.chrome, font, text, offset(pointer.0, [12.0, 12.0]), [255; 3]);
            }
        }
        // Status icons down the right, each with the clock of its time (orange in the last minute).
        // In the game's five priority passes, 42 as 12.
        let started: Vec<(u16, (Instant, Option<Instant>))> = game.status_icons.iter().map(|&(kind, start, end)| (kind, (start, end))).collect();
        let order = status_tip::placed(&started, |kind| status_icon_files.get(&kind).map(|(priority, _)| *priority));
        let shown = order.iter().filter_map(|(kind, (start, end))| {
            let (_, file) = status_icon_files.get(kind)?;
            Some((*kind, icons.get(&format!("data\\texture\\effect\\{file}"))?.as_ref()?, start, end))
        });
        game.status_tip = None;
        for (order, (kind, icon, start, end)) in shown.enumerate() {
            let corner = status_tip::icon_corner(order, screen());
            // Pointed at: what it is and the time it has left, in a box at its left, rebuilt each frame.
            if status_tip::on_icon(corner, pointer.0)
                && let Some(info) = state_infos.get(&kind)
            {
                let left = end.filter(|_| info.time_limit).map(|end| end.saturating_duration_since(now).as_secs());
                let lines = status_tip::lines(info, left, messages);
                let widest = lines.iter().map(|(line, _)| font.width(line)).fold(0.0, f32::max);
                let (box_corner, size) = status_tip::place(corner, widest, lines.len(), font.line_height());
                tooltip.sprites.push((&assets.speech_fill, box_corner, size));
                for (edge, piece) in overhead::border(size) {
                    tooltip.sprites.push((&assets.speech_border, offset(box_corner, edge), piece));
                }
                for (row, (line, color)) in lines.iter().enumerate() {
                    let at = status_tip::line_at(box_corner, row, font.line_height());
                    tooltip.texts.push((line.clone(), offset(at, [1.0, 1.0]), [0, 0, 0]));
                    tooltip.texts.push((line.clone(), at, *color));
                }
                game.status_tip = Some(lines.into_iter().map(|(line, _)| line).collect());
            }
            let scale = (STATUS_ICON / icon.size[0].max(icon.size[1])).min(1.0);
            let size = icon.size.map(|value| value * scale);
            chat_layer.sprites.push((icon, offset(corner, [(STATUS_ICON - size[0]) / 2.0, (STATUS_ICON - size[1]) / 2.0]), size));
            if let Some(end) = end {
                let left = end.saturating_duration_since(now).as_secs_f32();
                let (color, share) = if left < 60.0 {
                    (1, 1.0 - left / 60.0)
                } else {
                    (0, now.duration_since(*start).as_secs_f32() / (end.duration_since(*start).as_secs_f32() - 60.0).max(1.0))
                };
                let step = ((share.clamp(0.0, 1.0) * CLOCK_STEPS as f32) as usize).min(CLOCK_STEPS);
                chat_layer.sprites.push((&assets.clocks[color][step], corner, [STATUS_ICON; 2]));
            }
        }
        // The quest tracker, under the minimap.
        if game.quest_window.tracker {
            quest_tracker::draw(&mut chat_layer, &quest_tracker::blocks(&game.quests, quest_texts, &game.quest_window.hidden, messages));
        }
        // An NPC's illustration, over the world and under the windows.
        let mut cutin_layer = Layer::default();
        if let Some((image, place)) = &game.cutin {
            let [width, height] = image.size;
            let corner = match place {
                0 => [0.0, screen()[1] - height],
                1 => [((screen()[0] - width) / 2.0).floor(), screen()[1] - height],
                2 => [screen()[0] - width, screen()[1] - height],
                _ => [((screen()[0] - width) / 2.0).floor(), ((screen()[1] - height) / 2.0).floor()],
            };
            if *place <= 4 {
                cutin_layer.sprites.push((image, corner, image.size));
            }
        }
        game.status_height = game.status_window.height(&assets.status);
        // Our own HP in the party window comes from our status.
        let own_hp = [net::STATUS_HP, net::STATUS_MAX_HP].map(|kind| game.status.get(&kind).copied().unwrap_or(0) as i32);
        let account = game.account_id;
        if let Some(member) = game.party.as_mut().and_then(|party| party.member(account)) {
            member.hp = Some(own_hp);
        }
        // Announcements under the windows.
        game.announcements.expire(now);
        game.announcements.prepare(gpu, font);
        let mut announce_layer = Layer::default();
        game.announcements.draw(&mut announce_layer);
        game.level_ups.draw(&mut announce_layer, &assets.level_up, pointer);
        // ponytail: the new-quest icon covers the base level button when both are up.
        if game.quest_icon {
            quest_notice::draw_icon(&mut announce_layer, &assets.quest_notice, pointer);
        }
        game.banners.draw(&mut announce_layer, &assets.quest_notice, font);
        // The pickup popup takes no clicks.
        if let Some(popup) = &game.pickup_popup {
            let icon = icons.get(&popup.icon).and_then(Option::as_ref);
            popup.draw(&mut announce_layer, &assets.chrome, &assets.pickup, icon, font);
        }
        // Weather over the map, under the names and windows.
        let mut weather_layer = Layer::default();
        if self.graphics.effects_on {
            game.weather.draw(&mut weather_layer, &assets.weather, now);
        }
        let mut layers = vec![weather_layer, base, announce_layer, cutin_layer, chat_layer];
        let mut preview = Some(preview);
        let cooling = skills::Cooling { cooldowns: &game.cooldowns, fan: &assets.fan, now: control::now() };
        for &id in &game.windows {
            let mut layer = Layer::default();
            match id {
                Panel::Hotkeys => game.hotkeys.draw(
                    &mut layer,
                    &mut tooltip,
                    &assets.chrome,
                    &assets.hotkeys,
                    icons,
                    items,
                    skill_info,
                    &game.inventory.items,
                    font,
                    pointer.0,
                    &cooling,
                ),
                Panel::Skills => {
                    game.skill_window.draw(&mut layer, &mut tooltip, &assets.chrome, &assets.skills, icons, font, messages, &sheet, pointer, &cooling)
                }
                Panel::BasicInfo => {
                    let info = basic_info::Info {
                        name: &game.me.name,
                        job: pc_job_names.get(&game.me.job).map_or("", String::as_str),
                        status: &game.status,
                        unread_mail: game.mail.unread,
                        reward_waiting: game.achievements.values().any(|achievement| achievement::claimable(Some(achievement))),
                        new_items: game.new_items,
                        new_level: game.level_ups.base,
                        new_job_level: game.level_ups.job,
                        overweight: game.overweight,
                    };
                    game.basic_info.draw(&mut layer, &assets.chrome, &assets.basic_info, font, messages, &info, pointer.0);
                }
                Panel::Inventory => game.inventory.draw(
                    &mut layer,
                    &mut tooltip,
                    &assets.chrome,
                    &assets.inventory,
                    icons,
                    items,
                    messages,
                    font,
                    pointer.0,
                    (card_names, &game.char_names),
                ),
                Panel::Cart if game.cart.cart.is_some() => game.cart.draw(
                    &mut layer,
                    &mut tooltip,
                    &assets.chrome,
                    &assets.inventory,
                    icons,
                    items,
                    messages,
                    font,
                    pointer.0,
                    (card_names, &game.char_names),
                ),
                Panel::Cart => {}
                Panel::Pet => {
                    if let Some(pet) = &game.pet {
                        game.pet_window.draw(&mut layer, &assets.chrome, &assets.pet, font, messages, pet, pointer);
                    }
                }
                Panel::Mercenary => {
                    if let Some(hired) = &game.mercenary {
                        let job = game.units.get(&hired.id).map_or(0, |actor| actor.job);
                        let unix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.as_secs());
                        game.mercenary_window.draw(&mut layer, &assets.chrome, &assets.mercenary, font, messages, hired, job, unix, pointer);
                    }
                }
                Panel::MercenarySkills => {
                    if game.mercenary.is_some() {
                        let list = &game.mercenary_skill_list;
                        list.draw(&mut layer, &assets.chrome, &assets.skills, icons, font, messages, &game.mercenary_skills, skill_info, pointer.0);
                    }
                }
                Panel::Homunculus => {
                    if let Some(homunculus) = &game.homunculus {
                        game.homunculus_window.draw(&mut layer, &assets.chrome, &assets.homunculus, font, messages, homunculus, pointer);
                    }
                }
                Panel::HomunculusSkills => {
                    if let Some(homunculus) = &game.homunculus {
                        let list = &game.homunculus_skill_list;
                        let skills = &game.homunculus_skills;
                        list.draw(&mut layer, &assets.chrome, &assets.skills, icons, font, messages, skills, homunculus.skill_points, skill_info, pointer);
                    }
                }
                Panel::Status => {
                    let guild = game.guild.as_ref().map_or("", |guild| guild.name.as_str());
                    game.status_window.draw(&mut layer, &assets.chrome, &assets.status, font, messages, &game.status, guild, pointer);
                }
                Panel::Guild => {
                    if let Some(guild) = &game.guild {
                        let emblem = game.emblems.get(&guild.id).and_then(|(_, emblem)| emblem.as_ref());
                        game.guild_window.draw(
                            &mut layer,
                            &mut tooltip,
                            &assets.chrome,
                            (&assets.guild, &assets.skills),
                            icons,
                            font,
                            messages,
                            guild,
                            (pc_job_names, skill_info),
                            emblem,
                            pointer,
                        );
                    }
                }
                Panel::Clan => {
                    if let Some(clan) = &game.clan {
                        game.clan_window.draw(&mut layer, &assets.chrome, &assets.clan, messages, clan, pointer.0);
                    }
                }
                Panel::Party => {
                    let stem = |map: &str| map.trim_end_matches(".gat").to_owned();
                    game.party_window.draw(
                        &mut layer,
                        &mut tooltip,
                        &assets.chrome,
                        &assets.party,
                        font,
                        messages,
                        game.party.as_ref(),
                        &game.friends,
                        (game.account_id, &stem),
                        pointer,
                    );
                }
                Panel::Storage => game.storage.draw(&mut layer, &mut tooltip, &assets.chrome, &assets.storage, icons, items, messages, font, pointer, caret),
                Panel::Equipment => game.equipment.draw(
                    &mut layer,
                    &mut tooltip,
                    &assets.chrome,
                    &assets.equipment,
                    icons,
                    items,
                    messages,
                    font,
                    pointer.0,
                    &game.inventory.items,
                    preview.take().unwrap_or_default(),
                    (card_names, &game.char_names),
                ),
            }
            layer.faded = game.panel_faded(id);
            layers.push(layer);
        }
        // Party members dragged out of the list, and the compact stack, over the other windows.
        let mut member_layer = Layer::default();
        game.member_windows.draw(
            &mut member_layer,
            &assets.chrome,
            &assets.party,
            font,
            game.party.as_ref(),
            here.as_str(),
            game.party_window.compact,
            pointer.0,
        );
        layers.push(member_layer);
        // A dragged item's or skill's icon follows the pointer.
        let dragged_icon = match game.dragging {
            Some((Dragged::Item(index) | Dragged::Worn(index), _, true)) => {
                game.inventory.get(index).and_then(|item| items.get(&item.id).map(|info| inventory::icon_path(info, item.identified)))
            }
            Some((Dragged::Skill(id), _, true)) => skill_info.get(&id).map(skills::icon_path),
            Some((Dragged::Stored(index), _, true)) => {
                game.storage.get(index).and_then(|item| items.get(&item.id).map(|info| inventory::icon_path(info, item.identified)))
            }
            Some((Dragged::ShopOffer(row), _, true)) => game
                .shop
                .as_ref()
                .and_then(|shop| shop.offer_at(0, row))
                .and_then(|offer| items.get(&offer.item).map(|info| inventory::icon_path(info, offer.identified))),
            Some((Dragged::ShopDeal(row), _, true)) => game
                .shop
                .as_ref()
                .and_then(|shop| shop.offer_at(1, row))
                .and_then(|offer| items.get(&offer.item).map(|info| inventory::icon_path(info, offer.identified))),
            Some((Dragged::Carted(index), _, true)) => {
                game.cart.get(index).and_then(|item| items.get(&item.id).map(|info| inventory::icon_path(info, item.identified)))
            }
            Some((Dragged::Hotkey(slot), _, true)) => match game.hotkeys.keys.get(slot).copied().flatten() {
                Some(net::Hotkey { skill: true, id, .. }) => u16::try_from(id).ok().and_then(|id| skill_info.get(&id)).map(skills::icon_path),
                Some(net::Hotkey { id, .. }) => items.get(&id).map(|info| inventory::icon_path(info, true)),
                None => None,
            },
            _ => None,
        };
        if let Some(Some(icon)) = dragged_icon.and_then(|path| icons.get(&path)) {
            tooltip.sprites.push((icon, [pointer.0[0] - icon.size[0] / 2.0, pointer.0[1] - icon.size[1] / 2.0], icon.size));
        }

        // An aimed skill: its name in a balloon at the top, "select a target" in one at the bottom, centred. Taming has
        // no skill: the bottom one only.
        let mut aim_lines = Vec::new();
        if let Some((id, _)) = game.targeting {
            aim_lines.push((skill_info.get(&id).map_or_else(|| id.to_string(), |info| info.name.clone()), screen()[1] * 46.0 / 480.0));
        }
        if game.targeting.is_some() || game.capturing {
            aim_lines.push((message(TARGET_MESSAGE).to_owned(), screen()[1] * 400.0 / 480.0));
        }
        for (text, top) in aim_lines {
            let size = overhead::size(font.width(&text), 1);
            let corner = [((screen()[0] - size[0]) / 2.0).floor(), top.floor()];
            tooltip.sprites.push((&assets.speech_fill, corner, size));
            for (edge, piece) in overhead::border(size) {
                tooltip.sprites.push((&assets.speech_border, offset(corner, edge), piece));
            }
            let place = offset(corner, overhead::TEXT_AT);
            tooltip.texts.push((text.clone(), offset(place, [1.0, 1.0]), [0, 0, 0]));
            tooltip.texts.push((text, place, AIM_TEXT));
        }
        // The store search under the shops it opens from afar.
        let mut catalog_layer = Layer::default();
        if let Some(catalog) = &game.catalog {
            catalog.draw(&mut catalog_layer, &assets.chrome, &assets.catalog, icons, items, font, messages, (pointer.0, pointer.1, caret));
        }
        // A shop's windows over the others; its question in a message box.
        let mut shop_layer = Layer::default();
        if let Some(settings) = &game.chat.settings {
            settings.draw(&mut shop_layer, &assets.chrome, messages, &game.chat.tabs[settings.tab].kinds, pointer.0);
        }
        // A skill's description, over the windows.
        if let Some(window) = &game.skill_description {
            window.draw(&mut shop_layer, &assets.chrome, &assets.skill_descript, font, pointer.0);
        }
        if let Some((list, _)) = &game.pick {
            list.draw(&mut shop_layer, &assets.chrome, &assets.pick_list, icons, items, skill_info, messages, font, pointer);
        }
        if let Some(shop) = &game.shop {
            shop.draw(&mut shop_layer, &assets.chrome, &assets.shop, icons, items, messages, font, pointer);
        }
        if game.deal_choice.is_some() {
            let dialog = dialog_position(&assets.message);
            shop_layer.sprites.push((&assets.message, dialog, assets.message.size));
            for (images, place) in assets.shop_buttons().into_iter().zip(DEAL_BUTTONS) {
                shop_layer.sprites.push(button_sprite(pointer, offset(dialog, place), images));
            }
            shop_layer.texts.push((message(DEAL_MESSAGE).to_owned(), offset(dialog, MESSAGE_TEXT), TEXT_COLOR));
        }
        if let Some(room) = &game.room {
            room.draw(&mut shop_layer, &assets.chrome, font, messages, &game.me.name, pointer.0, caret);
        }
        if let Some(setup) = &game.vend_setup {
            setup.draw(&mut shop_layer, &assets.chrome, &assets.vend, icons, items, &game.cart.items, font, messages, pointer, caret);
        }
        if let Some(shop) = &game.my_shop {
            shop.draw(&mut shop_layer, &assets.chrome, &assets.vend, icons, items, font, messages, pointer);
        }
        if let Some(setup) = &game.buying_setup {
            setup.draw(&mut shop_layer, &assets.chrome, &assets.vend, icons, items, &game.inventory.items, font, messages, pointer, caret);
        }
        if let Some(store) = &game.my_buying_store {
            store.draw(&mut shop_layer, &assets.chrome, &assets.vend, icons, items, font, messages, pointer);
        }
        game.quest_window.draw(&mut shop_layer, &assets.chrome, &assets.quest, font, messages, &game.quests, quest_texts, pointer.0);
        game.achievement_window.draw(
            &mut shop_layer,
            &assets.chrome,
            &assets.achievement,
            icons,
            font,
            messages,
            achievement_info,
            &game.achievements,
            &game.achievement_totals,
            title_names,
            items,
            pointer,
        );
        // ponytail: the roulette's icon stands left of the minimap; where the real client puts it isn't known.
        roulette::draw_icon(&mut shop_layer, &mut tooltip, &assets.chrome, &assets.roulette, font, messages, pointer.0);
        if let Some(window) = &game.monster_window {
            window.draw(&mut shop_layer, &assets.chrome, font, messages, pointer.0);
        }
        if let Some(window) = &game.roulette {
            window.draw(&mut shop_layer, &mut tooltip, &assets.chrome, &assets.roulette, icons, items, font, messages, control::now(), pointer);
        }
        if let Some(list) = &game.emotion_list {
            list.draw(&mut shop_layer, &assets.chrome, font, messages, game.emotion_sprite.as_ref(), &emotion_icons, pointer.0);
        }
        if let Some(window) = &game.macro_window {
            window.draw(&mut shop_layer, &assets.chrome, font, messages, &game.macro_lines, pointer.0, caret);
        }
        game.mail.draw(&mut shop_layer, &mut tooltip, &assets.chrome, &assets.mail, icons, items, font, messages, pc_job_names, pointer, caret);
        if let Some(make) = &game.room_make {
            make.draw(&mut shop_layer, &assets.chrome, &assets.chatroom, messages, pointer, caret);
        }
        if let Some(bank) = &game.bank {
            let carried = game.status.get(&net::STATUS_ZENY).copied().unwrap_or(0).max(0) as u32;
            bank.draw(&mut shop_layer, &assets.chrome, &assets.bank, messages, font, pointer, caret, carried);
        }
        if let Some(shop) = &game.cashshop {
            shop.draw(&mut shop_layer, &assets.chrome, &assets.cashshop, icons, items, messages, font, pointer, caret);
        }
        if let Some(viewed) = &game.viewing {
            let window = &viewed.window;
            window.draw(
                &mut shop_layer,
                &mut tooltip,
                &assets.chrome,
                &assets.equipment,
                icons,
                items,
                messages,
                font,
                pointer.0,
                &viewed.items,
                viewed_preview,
                (card_names, &game.char_names),
            );
        }
        if let Some(window) = &game.refine {
            window.draw(&mut shop_layer, &assets.chrome, &assets.refine, icons, items, &game.inventory.items, messages, font, pointer);
        }
        if let Some(window) = &game.reform {
            window.draw(&mut shop_layer, &assets.chrome, &assets.reform, icons, items, &game.inventory.items, messages, font, pointer);
        }
        if let Some(window) = &game.grade {
            window.draw(&mut shop_layer, &assets.chrome, &assets.grade, icons, items, &game.inventory.items, messages, font, pointer);
        }
        if let Some(window) = &game.enchant {
            window.draw(&mut shop_layer, &assets.chrome, &assets.enchant, icons, items, &game.inventory.items, messages, font, pointer);
        }
        if let Some(window) = &game.stylist {
            let job = game.player.look.as_ref().map(|look| look.job).unwrap_or(game.me.job);
            window.draw(&mut shop_layer, &assets.chrome, &assets.stylist, items, &self.styling_shop, job, messages, font, pointer, &stylist_placements);
        }
        if let Some(window) = &game.materials {
            window.draw(&mut shop_layer, &assets.chrome, &assets.pick_list, icons, items, &game.inventory.items, messages, font, pointer);
        }
        if let Some(window) = &game.synthesis {
            window.draw(&mut shop_layer, &assets.chrome, &assets.laphine, icons, items, &game.inventory.items, messages, font, pointer);
        }
        if let Some(window) = &game.upgrade {
            window.draw(&mut shop_layer, &assets.chrome, &assets.laphine, icons, items, &game.inventory.items, messages, font, pointer);
        }
        if let Some(trade) = &game.trade {
            trade.draw(&mut shop_layer, &mut tooltip, &assets.chrome, &assets.trade, icons, items, font, pointer, caret);
        }
        // Someone asks for a deal: who, their ID in letters and level, OK or cancel.
        if let Some((name, id, level)) = &game.trade_asked {
            let dialog = dialog_position(&assets.message);
            shop_layer.sprites.push((&assets.message, dialog, assets.message.size));
            shop_layer.sprites.push(button_sprite(pointer, offset(dialog, LEFT_BUTTON), &assets.ok));
            shop_layer.sprites.push(button_sprite(pointer, offset(dialog, RIGHT_BUTTON), &assets.cancel));
            let lines = [format!("({name}){}", message(TRADE_ASKS)), format!("PN: {}     Lv.{level}", trade::id_code(*id))];
            for (row, line) in lines.into_iter().enumerate() {
                shop_layer.texts.push((line, offset(dialog, [MESSAGE_TEXT[0], MESSAGE_TEXT[1] + row as f32 * font.line_height()]), TEXT_COLOR));
            }
        }
        if game.cashshop_buy_asked
            && let Some(shop) = &game.cashshop
        {
            let dialog = dialog_position(&assets.message);
            shop_layer.sprites.push((&assets.message, dialog, assets.message.size));
            shop_layer.sprites.push(button_sprite(pointer, offset(dialog, LEFT_BUTTON), &assets.ok));
            shop_layer.sprites.push(button_sprite(pointer, offset(dialog, RIGHT_BUTTON), &assets.cancel));
            let cash = shop.cart_total().saturating_sub(shop.free_to_spend);
            let question = messages::fill(message(1312), &[&cash, &shop.free_to_spend]);
            for (row, line) in message_lines(font, &question, MESSAGE_WIDTH).into_iter().enumerate() {
                shop_layer.texts.push((line, offset(dialog, [MESSAGE_TEXT[0], MESSAGE_TEXT[1] + row as f32 * font.line_height()]), TEXT_COLOR));
            }
        }
        // A guild invites us, asks for an alliance, or the master is about to end a relation.
        let guild_question = match (&game.guild_invite, &game.guild_asked) {
            (Some((_, name)), _) => Some(format!("({name}){}", message(GUILD_INVITED))),
            (_, Some(GuildAsked::Ally { guild, .. })) => Some(format!("({guild}){}", message(ALLY_ASKS))),
            (_, Some(GuildAsked::End { .. })) => Some(message(DELETE_ASKS).to_owned()),
            (_, Some(GuildAsked::Leave)) => Some(message(LEAVE_ASKS).to_owned()),
            _ => None,
        };
        if let Some(question) = guild_question {
            let dialog = dialog_position(&assets.message);
            shop_layer.sprites.push((&assets.message, dialog, assets.message.size));
            shop_layer.sprites.push(button_sprite(pointer, offset(dialog, LEFT_BUTTON), &assets.ok));
            shop_layer.sprites.push(button_sprite(pointer, offset(dialog, RIGHT_BUTTON), &assets.cancel));
            for (row, line) in message_lines(font, &question, MESSAGE_WIDTH).into_iter().enumerate() {
                shop_layer.texts.push((line, offset(dialog, [MESSAGE_TEXT[0], MESSAGE_TEXT[1] + row as f32 * font.line_height()]), TEXT_COLOR));
            }
        }
        // Growing the inventory (which voucher, from how many slots to how many) or a pet question, in the message's
        // colors.
        let colored_question = match (game.expansion_asked, &game.pet_asked) {
            _ if game.dismiss_asked => Some(message(mercenary::DISMISS_QUESTION).to_owned()),
            _ if game.homunculus_rename.is_some() => Some(messages::fill(message(homunculus::RENAME_QUESTION), &[game.homunculus_rename.as_ref().unwrap()])),
            _ if game.leader_asked.is_some() => Some(message(LEADER_QUESTION).to_owned()),
            _ if game.bind_asked.is_some() => Some(message(BIND_ASKS).to_owned()),
            (Some(voucher), _) => {
                let now = inventory::BASE_SLOTS + usize::from(game.inventory.expansion);
                let name = inventory::item_name(items, voucher, true, 0);
                Some(messages::fill(message(EXPANSION_ASKS), &[&name, &now, &(now + VOUCHER_SLOTS)]))
            }
            (None, Some(PetAsked::Feed)) => Some(message(PET_FEED_ASKS).to_owned()),
            (None, Some(PetAsked::Rename(name))) => Some(messages::fill(message(PET_RENAME_ASKS), &[name])),
            (None, None) => None,
        };
        if let Some(question) = colored_question {
            let dialog = dialog_position(&assets.message);
            shop_layer.sprites.push((&assets.message, dialog, assets.message.size));
            shop_layer.sprites.push(button_sprite(pointer, offset(dialog, LEFT_BUTTON), &assets.ok));
            shop_layer.sprites.push(button_sprite(pointer, offset(dialog, RIGHT_BUTTON), &assets.cancel));
            for (row, pieces) in wrap_colored(font, &question, MESSAGE_WIDTH).into_iter().enumerate() {
                let mut x = dialog[0] + MESSAGE_TEXT[0];
                for (color, piece) in pieces {
                    let width = font.width(&piece);
                    // The message's black is the box's text color.
                    let color = if color == [0, 0, 0] { TEXT_COLOR } else { color };
                    shop_layer.texts.push((piece, [x, dialog[1] + MESSAGE_TEXT[1] + row as f32 * font.line_height()], color));
                    x += width;
                }
            }
        }
        // Someone asks to be friends.
        if let Some((_, _, name)) = &game.friend_asked {
            let dialog = dialog_position(&assets.message);
            shop_layer.sprites.push((&assets.message, dialog, assets.message.size));
            shop_layer.sprites.push(button_sprite(pointer, offset(dialog, LEFT_BUTTON), &assets.ok));
            shop_layer.sprites.push(button_sprite(pointer, offset(dialog, RIGHT_BUTTON), &assets.cancel));
            let question = messages::fill(message(FRIEND_ASKS), &[name]);
            for (row, line) in message_lines(font, &question, MESSAGE_WIDTH).into_iter().enumerate() {
                shop_layer.texts.push((line, offset(dialog, [MESSAGE_TEXT[0], MESSAGE_TEXT[1] + row as f32 * font.line_height()]), TEXT_COLOR));
            }
        }
        // The battleground queue, and its question: applying, leaving, or entering with the time left.
        if game.queue_window.frame.open {
            game.queue_window.draw(&mut shop_layer, &assets.chrome, &assets.battle_queue, font, messages, &self.battlegrounds, game.queued.as_ref(), pointer);
        }
        let queue_question = match &game.queue_asked {
            Some(QueueAsked::Apply(kind, _)) => {
                let index = [net::QUEUE_ALONE, net::QUEUE_PARTY, net::QUEUE_GUILD].iter().position(|each| each == kind).unwrap_or(0);
                Some(message(battle_queue::APPLY_ASKS[index]).to_owned())
            }
            Some(QueueAsked::Cancel(_)) => Some(message(battle_queue::CANCEL_ASKS).to_owned()),
            Some(QueueAsked::Lobby(_, until)) => {
                let left = until.saturating_duration_since(Instant::now()).as_secs();
                let note = battle_queue::LOBBY_NOTE.iter().map(|&id| message(id)).collect::<Vec<_>>().join(" ");
                Some(format!("{} {note} {}", message(battle_queue::LOBBY_ASKS), messages::fill(message(battle_queue::LOBBY_TIME), &[&left])))
            }
            None => None,
        };
        if let Some(question) = queue_question {
            let dialog = dialog_position(&assets.message);
            shop_layer.sprites.push((&assets.message, dialog, assets.message.size));
            shop_layer.sprites.push(button_sprite(pointer, offset(dialog, LEFT_BUTTON), &assets.ok));
            shop_layer.sprites.push(button_sprite(pointer, offset(dialog, RIGHT_BUTTON), &assets.cancel));
            for (row, line) in message_lines(font, &question, MESSAGE_WIDTH).into_iter().enumerate() {
                shop_layer.texts.push((line, offset(dialog, [MESSAGE_TEXT[0], MESSAGE_TEXT[1] + row as f32 * font.line_height()]), TEXT_COLOR));
            }
        }
        // Asked to be adopted: who asks, then whether they're sure (what a baby class gives up is in the chat).
        // ponytail: the second question (888) is an estimate, and so is splitting it: the box holds three lines, so it
        // shows the last sentence.
        if let Some((_, _, name, again)) = &game.adopt_asked {
            let question = if *again { last_sentence(message(ADOPT_ASKS_AGAIN)).to_owned() } else { format!("({name}){}", message(ADOPT_ASKS)) };
            let dialog = dialog_position(&assets.message);
            shop_layer.sprites.push((&assets.message, dialog, assets.message.size));
            shop_layer.sprites.push(button_sprite(pointer, offset(dialog, LEFT_BUTTON), &assets.ok));
            shop_layer.sprites.push(button_sprite(pointer, offset(dialog, RIGHT_BUTTON), &assets.cancel));
            for (row, line) in message_lines(font, &question, MESSAGE_WIDTH).into_iter().enumerate() {
                shop_layer.texts.push((line, offset(dialog, [MESSAGE_TEXT[0], MESSAGE_TEXT[1] + row as f32 * font.line_height()]), TEXT_COLOR));
            }
        }
        // A party invites us: its name and the question, OK or cancel.
        if let Some((_, name)) = &game.party_invite {
            let dialog = dialog_position(&assets.message);
            shop_layer.sprites.push((&assets.message, dialog, assets.message.size));
            shop_layer.sprites.push(button_sprite(pointer, offset(dialog, LEFT_BUTTON), &assets.ok));
            shop_layer.sprites.push(button_sprite(pointer, offset(dialog, RIGHT_BUTTON), &assets.cancel));
            let question = format!("{name} {}", message(PARTY_INVITED).trim_start());
            for (row, line) in message_lines(font, &question, MESSAGE_WIDTH).into_iter().enumerate() {
                shop_layer.texts.push((line, offset(dialog, [MESSAGE_TEXT[0], MESSAGE_TEXT[1] + row as f32 * font.line_height()]), TEXT_COLOR));
            }
        }
        if let Some(amount) = &game.amount {
            amount.draw(&mut shop_layer, &assets.chrome, &assets.message, [&assets.ok, &assets.cancel], messages, pointer);
        }
        if let Some(window) = &game.item_window {
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |time| time.as_secs());
            let lines = window.lines(items, messages, now, &|text, width| wrap_colored(font, text, width));
            game.item_lines = lines.len();
            window.draw(&mut shop_layer, &assets.chrome, &assets.item_window, icons, items, font, &lines, pointer.0);
        }
        if let Some(view) = &game.card_view {
            view.draw(&mut shop_layer, &assets.chrome, &assets.item_window, icons, pointer.0);
        }
        let unix_now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |time| time.as_secs());
        for window in &compared {
            let lines = window.lines(items, messages, unix_now, &|text, width| wrap_colored(font, text, width));
            window.draw(&mut tooltip, &assets.chrome, &assets.item_window, icons, items, font, &lines, pointer.0);
        }
        if game.apply_asked {
            let dialog = dialog_position(&assets.message);
            let question = messages.get(skills::APPLY_QUESTION).map_or("", String::as_str);
            shop_layer.sprites.push((&assets.message, dialog, assets.message.size));
            shop_layer.sprites.push(button_sprite(pointer, offset(dialog, LEFT_BUTTON), &assets.ok));
            shop_layer.sprites.push(button_sprite(pointer, offset(dialog, RIGHT_BUTTON), &assets.cancel));
            for (row, line) in message_lines(font, question, MESSAGE_WIDTH).into_iter().enumerate() {
                shop_layer.texts.push((line, offset(dialog, [MESSAGE_TEXT[0], MESSAGE_TEXT[1] + row as f32 * font.line_height()]), TEXT_COLOR));
            }
        }
        if game.save_asked {
            let dialog = dialog_position(&assets.message);
            let question = messages.get(escape::SAVE_QUESTION).map_or("Do you really want to go back to your savepoint?", String::as_str);
            shop_layer.sprites.push((&assets.message, dialog, assets.message.size));
            shop_layer.sprites.push(button_sprite(pointer, offset(dialog, LEFT_BUTTON), &assets.ok));
            shop_layer.sprites.push(button_sprite(pointer, offset(dialog, RIGHT_BUTTON), &assets.cancel));
            for (row, line) in message_lines(font, question, MESSAGE_WIDTH).into_iter().enumerate() {
                shop_layer.texts.push((line, offset(dialog, [MESSAGE_TEXT[0], MESSAGE_TEXT[1] + row as f32 * font.line_height()]), TEXT_COLOR));
            }
        }
        if let Some(menu) = &game.escape {
            menu.draw(&mut shop_layer, &assets.chrome, &assets.escape, messages, pointer, game.can_resurrect());
        }
        if let Some(setup) = &game.party_setup {
            setup.draw(&mut shop_layer, &assets.chrome, &assets.party, (&assets.ok, &assets.cancel), messages, pointer);
        }
        game.talks.draw(&mut shop_layer, &assets.chrome, font, messages, pointer);
        if let Some(setup) = &game.friend_setup {
            setup.draw(&mut shop_layer, &assets.chrome, messages, pointer.0);
        }
        if let Some(list) = &game.block_list {
            list.draw(&mut shop_layer, &assets.chrome, pointer.0);
        }
        if let Some(prompt) = &game.away_prompt {
            prompt.draw(&mut shop_layer, &assets.chrome, messages, pointer);
        }
        if let Some(board) = &game.ranking {
            board.draw(&mut shop_layer, &assets.chrome, font, messages, &game.char_names, pointer.0);
        }
        if let Some(window) = &game.navi_window {
            let caret = (game.entered.elapsed().as_millis() / CARET_BLINK.as_millis()).is_multiple_of(2);
            window.draw(&mut shop_layer, &assets.chrome, font, messages, game.navigation.as_ref().map(|active| &active.target), pointer.0, caret);
        }
        if let Some(window) = &game.reputation_window {
            window.draw(&mut shop_layer, &assets.chrome, &assets.reputation, font, messages, &self.reputation_tables, &game.reputation, pointer.0);
        }
        if let Some(room) = &game.dressroom {
            let feet = room.feet();
            let figure = match &game.dress_figure {
                Some(sprites) => dress_placements.iter().map(|(key, position, size, _)| (sprites.texture(*key), offset(feet, *position), *size)).collect(),
                None => Vec::new(),
            };
            room.draw(&mut shop_layer, &assets.chrome, &assets.dressroom, font, messages, &game.dress_lists, figure, pointer);
        }
        if let Some(window) = &game.audio_window {
            window.draw(&mut shop_layer, &assets.chrome, &assets.options, font, messages, sound, pointer.0);
        }
        if let Some(window) = &game.graphics_window {
            window.draw(&mut shop_layer, &assets.chrome, &assets.options, font, messages, &self.graphics, pointer.0);
        }
        if let Some(window) = &game.shortcuts_window {
            window.draw(&mut shop_layer, &assets.chrome, font, messages, &self.shortcuts, pointer.0);
        }
        let mut menu_layer = Layer::default();
        if let Some(menu) = &game.menu {
            menu.draw(&mut menu_layer, &assets.context_menu, font, pointer.0);
        }
        let mut world_layer = Layer::default();
        if let Some(world_map) = &game.world_map {
            world_map.draw(&mut world_layer, &assets.chrome, font, &worlds.0, map_names, &map_key(&game.map), pointer.0);
        }
        game.dialog_links = dialog_links;
        let mut minimap_layer = Layer::default();
        {
            let show_quests = game.minimap_window.show_quests;
            let show_boss = game.minimap_window.show_boss;
            minimap_layer.faded = game.minimap_window.frame.faded();
            let quest_cells: Vec<[i32; 2]> = if show_quests {
                game.quests.iter().filter(|quest| quest.active).filter_map(|quest| quest_texts.get(&quest.id).and_then(|text| text.npc)).collect()
            } else {
                Vec::new()
            };
            let title = map_names.get(&map_key(&game.map)).cloned().unwrap_or_else(|| map_key(&game.map));
            let naming_caret = (now.saturating_duration_since(game.entered).as_millis() / CARET_BLINK.as_millis()).is_multiple_of(2);
            let boss_pin = show_boss.then_some(boss).flatten();
            game.minimap_window.draw(
                &mut minimap_layer,
                &assets.chrome,
                &assets.minimap,
                messages,
                &title,
                size,
                &minimap_window::Pins { player: game.player.cell, yaw, marks: &marks, party: &dots, guild: &guild, boss: boss_pin, quests: &quest_cells },
                pointer.0,
                naming_caret,
            );
        }
        layers.extend([catalog_layer, shop_layer, minimap_layer, overlay, menu_layer, tooltip, world_layer]);
        let composed = compose(gpu, font, layers);
        let (mut sprites, alphas) = flatten(&composed);

        // The cursor shows what a click would do.
        let on_window = game.on_ui(pointer.0, items, assets.message.size);
        let ground = view.pick_ground([pointer.0[0] / screen()[0], pointer.0[1] / screen()[1]], &game.world.gat);
        let (shift, alt) = (self.modifiers.shift_key(), self.modifiers.alt_key());
        cursor_look.set(if turning_cursor(game.camera_goal.dragging.is_some(), shift, alt) {
            cursor::Kind::Rotate
        } else if pointer.1 && game.attack.is_some_and(|attack| attack.held) {
            cursor::Kind::Attack
        } else if (game.targeting.is_some() || game.capturing) && !on_window {
            if game.aim_valid(pointer.0, self.modifiers.shift_key()) { cursor::Kind::TargetValid } else { cursor::Kind::Target }
        } else if game.link_at(pointer.0).is_some() {
            cursor::Kind::Clickable
        } else if on_window {
            if game.on_control(pointer.0) { cursor::Kind::Clickable } else { cursor::Kind::Default }
        } else if game.companion_mark.is_some_and(|(_, target)| hovered(&game.hit_boxes, pointer.0, false) == Some(Owner::Unit(target))) {
            cursor::Kind::TargetValid
        } else {
            match hovered(&game.hit_boxes, pointer.0, players_first) {
                Some(Owner::Unit(id)) => match game.units.get(&id).map(|actor| actor.kind) {
                    Some(MONSTER) => monster_cursor(shift, game.no_shift),
                    Some(NPC) => npc_cursor(game.units[&id].job),
                    Some(PLAYER) if game.opponent(id).is_some_and(|other| game.side().attack_cursor(&other, players_first, false)) => cursor::Kind::Attack,
                    Some(HOMUNCULUS | MERCENARY | ELEMENTAL) if versus::companion_cursor(game.side().flags) => cursor::Kind::Attack,
                    _ => cursor::Kind::Default,
                },
                Some(Owner::Item(_)) => cursor::Kind::Pick,
                // Past the map's edge counts as somewhere that can't be walked.
                _ => match ground {
                    Some(cell) if game.world.gat.walkable(cell) => cursor::Kind::Default,
                    _ => cursor::Kind::NoWalk,
                },
            }
        });
        for (font, frames) in [(&game.rank_font, &counter), (&game.time_font, &clock), (&game.time_attack, &banner)] {
            if let Some(Some(font)) = font {
                for (placed, anchor) in frames {
                    sprites.extend(placed.iter().map(|(key, at, size, _)| (font.texture(*key), [anchor[0] + at[0], anchor[1] + at[1]], *size)));
                }
            }
        }
        if let Some(sprite) = &game.slot_sprite {
            let middle = [screen()[0] / 2.0, screen()[1] / 2.0];
            sprites.extend(slot_frame.into_iter().map(|(key, offset, size, _)| (sprite.texture(key), [middle[0] + offset[0], middle[1] + offset[1]], size)));
        }
        // The cursor drawn at the middle of what it snaps to (clicks still go by the pointer).
        let aiming_unit = game.targeting.is_some_and(|(id, _)| {
            game.skills.iter().chain(&game.item_skill).find(|skill| skill.id == id).is_none_or(|skill| skill.kind & net::Skill::GROUND == 0)
        });
        let picked =
            if game.targeting.is_some() { hovered_unit(&game.hit_boxes, pointer.0, players_first) } else { hovered(&game.hit_boxes, pointer.0, players_first) };
        let under = picked.filter(|_| !on_window).and_then(|owner| {
            let area = game.hit_boxes.iter().find(|target| target.owner == owner)?.area;
            let kind = match owner {
                Owner::Item(_) => SnapTo::Item,
                Owner::Unit(id) => match game.units.get(&id).map(|actor| actor.kind) {
                    Some(MONSTER) => SnapTo::Monster,
                    Some(HOMUNCULUS) if game.homunculus_id == Some(id) => SnapTo::OurHomunculus,
                    _ => SnapTo::Other,
                },
                Owner::Player => SnapTo::Other,
            };
            Some((kind, area))
        });
        let pitcher = game.targeting.is_some_and(|(id, _)| id == POTION_PITCHER);
        game.cursor_at = snap_point(under, aiming_unit, pitcher, game.snaps).unwrap_or(pointer.0);
        cursor_look.press(pointer.1);
        sprites.extend(cursor_look.draw(gpu, game.cursor_at));

        game.effects.advance(gpu, now, yaw);
        // With /effect off, effects still play on, unseen, and their sounds are still heard.
        let effects_on = self.graphics.effects_on;
        if let Some(audio) = audio {
            for (file, at) in game.effects.due_sounds(now) {
                audio.play(grf, &file, heard(f32::hypot(at[0] - player_feet[0], at[2] - player_feet[2]) / world::CELL));
            }
        }
        let feet_of_unit = |unit: Option<u32>| match unit {
            None => Some(player_feet),
            Some(id) => feet_of.get(&id).copied(),
        };
        let (effect_quads, mut effect_meshes) =
            if effects_on { (game.effects.quads(now, feet_of_unit), game.effects.meshes(now, feet_of_unit)) } else { (Vec::new(), Vec::new()) };
        // Magic squares under ground casts, for as long as their casts last; flat on the ground, a little over it.
        game.cast_squares.retain(|caster, square| square.alive(now) && game.casts.contains_key(caster));
        if let Some(texture) = &assets.magic_target {
            for square in game.cast_squares.values() {
                let steps = square.width.ceil() as usize;
                let vertices = square
                    .triangles(now, steps)
                    .into_iter()
                    .map(|(cell, uv)| {
                        let ground = game.world.cell_position(cell);
                        world::MeshVertex {
                            position: [cell[0] * world::CELL + world::CELL / 2.0, ground[1] + CAST_SQUARE_LIFT, cell[1] * world::CELL + world::CELL / 2.0],
                            uv,
                            tint: [1.0; 4],
                        }
                    })
                    .collect();
                effect_meshes.push((texture, vertices, true));
            }
        }
        let frame = WorldFrame {
            world: &game.world,
            view: &view,
            billboards: &billboards,
            shadows,
            effects: &effect_quads,
            meshes: &effect_meshes,
            time: now.duration_since(game.entered).as_secs_f32(),
        };
        if let Some(path) = self.print_screen.take() {
            let _ = path.parent().map(std::fs::create_dir_all);
            let shot = image::DynamicImage::ImageRgba8(gpu.snapshot(Some(&frame), &sprites, &alphas)).to_rgb8();
            if shot.save(&path).is_ok() {
                self.shot_saved = path.file_name().map(|name| name.to_string_lossy().into_owned());
            }
        }
        let shot_delay = std::env::var("RUSTNAROK_CLIENT_SHOT_DELAY").ok().and_then(|seconds| seconds.parse().ok()).unwrap_or(2.0);
        match &self.screenshot {
            Some(path) if game.entered.elapsed().as_secs_f32() > shot_delay => {
                gpu.capture(Some(&frame), &sprites, &alphas, path);
                event_loop.exit();
            }
            _ => gpu.draw(Some(&frame), &sprites, &alphas),
        }
    }
}

/// A layer with its text rasterized.
type Composed<'a> = (Vec<Sprite<'a>>, Vec<(Texture, [f32; 2])>, f32);

/// Rasterizes each layer's text.
fn compose<'a>(gpu: &Gpu, font: &Font, layers: Vec<Layer<'a>>) -> Vec<Composed<'a>> {
    layers
        .into_iter()
        .map(|layer| {
            let texts = layer.texts.iter().map(|(text, position, color)| (gpu.text(font, text, *color), *position)).collect();
            (layer.sprites, texts, layer.faded)
        })
        .collect()
}

/// Everything to draw, in order: each layer's sprites, then its text, over the layers before it; and how opaque each is.
fn flatten<'a>(composed: &'a [Composed<'a>]) -> (Vec<Sprite<'a>>, Vec<f32>) {
    let mut sprites = Vec::new();
    let mut alphas = Vec::new();
    for (layer, texts, faded) in composed {
        let before = sprites.len();
        sprites.extend(layer.iter().copied().chain(texts.iter().map(|(texture, position)| (texture, *position, texture.size))));
        alphas.resize(sprites.len(), 1.0);
        alphas[before..].fill(1.0 - faded);
    }
    (sprites, alphas)
}

/// The unit or item the pointer is on (see `picking::pick`), the player aside (only skills aim at oneself).
fn hovered(hit_boxes: &[picking::Target<Owner>], cursor: [f32; 2], players_first: bool) -> Option<Owner> {
    picking::pick(hit_boxes.iter().copied().filter(|target| target.owner != Owner::Player), cursor, players_first)
}

/// The unit the pointer is on, the player included: what skills and taming aim at.
fn hovered_unit(hit_boxes: &[picking::Target<Owner>], cursor: [f32; 2], players_first: bool) -> Option<Owner> {
    picking::pick(hit_boxes.iter().copied().filter(|target| !matches!(target.owner, Owner::Item(_))), cursor, players_first)
}

impl ApplicationHandler<NetEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gpu.is_some() {
            return;
        }
        // RUSTNAROK_CLIENT_WINDOW=<width>x<height> opens the window at another size (wide screens in tests).
        let size = std::env::var("RUSTNAROK_CLIENT_WINDOW").ok().and_then(|size| {
            let (width, height) = size.split_once('x')?;
            Some(LogicalSize::new(width.parse().ok()?, height.parse().ok()?))
        });
        let attributes = Window::default_attributes()
            .with_title("Ragnarok")
            .with_inner_size(size.unwrap_or(LogicalSize::new(1024.0, 768.0)))
            .with_min_inner_size(LogicalSize::new(512.0, 384.0));
        let gpu = Gpu::new(Arc::new(event_loop.create_window(attributes).expect("create window")));
        // The client draws its own cursor.
        gpu.window.set_cursor_visible(false);
        // Hangul is typed through the input method.
        gpu.window.set_ime_allowed(true);
        self.assets = Some(Assets::load(&gpu, &self.grf, &self.font, &self.messages));
        self.gpu = Some(gpu);

        if let Some((id, password)) = std::env::var("RUSTNAROK_CLIENT_LOGIN").ok().as_deref().and_then(|login| login.split_once(':')) {
            self.fields = [id.into(), password.into()];
            self.submit_login();
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: NetEvent) {
        match event {
            NetEvent::Control(request) => control::run(self, event_loop, request),
            event => {
                control::observe(&event);
                self.net_event(event);
            }
        }
        if let Some(gpu) = &self.gpu {
            gpu.window.request_redraw();
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        let Some(gpu) = &mut self.gpu else { return };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => gpu.resize(),
            WindowEvent::Focused(focused) if control::driven() => gpu.hidden = !focused,
            // Driven over the control socket: the real mouse and keyboard would stray into the commands.
            WindowEvent::CursorMoved { .. } | WindowEvent::CursorLeft { .. } | WindowEvent::MouseInput { .. } | WindowEvent::MouseWheel { .. }
                if control::driven() => {}
            WindowEvent::ModifiersChanged(_) | WindowEvent::KeyboardInput { .. } | WindowEvent::Ime(_) if control::driven() => {}
            // RUSTNAROK_CLIENT_CURSOR pins the pointer for screenshots.
            WindowEvent::CursorMoved { .. } | WindowEvent::CursorLeft { .. } if std::env::var_os("RUSTNAROK_CLIENT_CURSOR").is_some() => {}
            // Screen pixels to the canvas, through its place in the window (black bars above and below in one narrower than 4:3).
            WindowEvent::CursorMoved { position, .. } => {
                let [left, top, width, height] = render::viewport([gpu.config.width, gpu.config.height]);
                gpu.window.request_redraw();
                self.cursor = [(position.x as f32 - left) * screen()[0] / width, (position.y as f32 - top) * screen()[1] / height];
                self.pointer_moved();
            }
            WindowEvent::MouseInput { state, button: MouseButton::Right, .. } => {
                gpu.window.request_redraw();
                self.right_button(state);
            }
            WindowEvent::ModifiersChanged(modifiers) => self.modifiers = modifiers.state(),
            WindowEvent::CursorLeft { .. } => {
                self.cursor = [-1.0; 2];
                gpu.window.request_redraw();
            }
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
                gpu.window.request_redraw();
                self.left_button(event_loop, state);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let rows = match delta {
                    MouseScrollDelta::LineDelta(_, y) => -y.signum() as i32,
                    MouseScrollDelta::PixelDelta(position) => -position.y.signum() as i32,
                };
                self.wheel(rows);
                if let Some(gpu) = &self.gpu {
                    gpu.window.request_redraw();
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                gpu.window.request_redraw();
                self.key(&(&event).into());
            }
            WindowEvent::Ime(Ime::Preedit(text, _)) => {
                gpu.window.request_redraw();
                self.compose(text);
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                gpu.window.request_redraw();
                self.compose(String::new());
                self.commit(&text);
            }
            WindowEvent::RedrawRequested => self.redraw(event_loop),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.quitting {
            return event_loop.exit();
        }
        // A loading screen is up: load now.
        if self.loading.as_ref().is_some_and(|loading| loading.shown) {
            match self.loading.take().unwrap().target {
                LoadTarget::Enter { position, direction } => self.enter_map(position, direction),
                LoadTarget::Change { map, position } => self.change_map(map, position),
            }
            if let Scene::Game(game) = &self.scene {
                let map = game.map.clone();
                self.play_music(Some(&map));
            }
            if let Some(gpu) = &self.gpu {
                gpu.window.request_redraw();
            }
        }
        self.auto_input(event_loop);
        let tick = self.client_tick();
        match &mut self.scene {
            Scene::Game(game) => {
                // Animate continuously and keep the map connection alive.
                if game.last_keepalive.elapsed() >= KEEPALIVE {
                    game.last_keepalive = Instant::now();
                    let _ = game.session.tick(tick);
                }
                // The view the player leaves is kept for the next map of its kind.
                self.saved_view.follow(&game.camera_goal, Instant::now(), self.save_view);
                // The held button follows the pointer: another walk each time MOVE_GAP has passed since the last
                // try, not over a window, nor while an attack or a pickup is on its way.
                let now = Instant::now();
                game.attack_step(now);
                game.pending_step(now);
                if let Some(cell) = game.walk_after_swing.filter(|_| !game.player.swinging(control::now())) {
                    game.walk_after_swing = None;
                    game.walk_toward(cell, now);
                }
                if follows_pointer(game.walk_held, game.pending.is_some(), game.last_walk_try, now)
                    && let Some(assets) = &self.assets
                    && !game.on_ui(self.cursor, &self.items, assets.message.size)
                    && let Some(cell) = game.ground_cell(self.cursor)
                {
                    game.last_walk_try = Some(now);
                    game.walk_toward(cell, now);
                }
                if game.entered.elapsed() > Duration::from_secs(1) {
                    if let Some(cell) = game.auto_walk.take() {
                        let _ = game.session.walk(cell);
                    }
                    // Through the chat line, so the client's commands (/organize, %) work too.
                    let say_at = std::env::var("RUSTNAROK_CLIENT_SAY_AT").ok().and_then(|seconds| seconds.parse().ok()).unwrap_or(1.0);
                    if let Some(texts) = game.auto_say.take_if(|_| game.entered.elapsed().as_secs_f32() >= say_at) {
                        let effects_on = self.graphics.effects_on;
                        for text in texts.split('|') {
                            game.chat.input = text.to_owned();
                            let _ = game.send_chat(false, false, &self.messages, &mut self.graphics.effects_on);
                        }
                        if effects_on != self.graphics.effects_on {
                            self.graphics.save();
                        }
                    }
                    // Dev switch RUSTNAROK_CLIENT_GUILD=window: the guild window, once the guild is known.
                    if game.guild.is_some()
                        && !game.guild_window.frame.open
                        && game.entered.elapsed() > Duration::from_secs(3)
                        && std::env::var("RUSTNAROK_CLIENT_GUILD").is_ok_and(|order| order.split(',').any(|part| part == "window"))
                        && !std::mem::replace(&mut game.guild_shown, true)
                    {
                        game.guild_window.frame.open = true;
                        game.raise(Panel::Guild);
                        game.ask_guild_pages();
                    }
                    // Dev switch RUSTNAROK_CLIENT_ROOM=window:<title> (the room maker, typed in) or make:<title> (and OK).
                    if game.entered.elapsed() > Duration::from_secs(2)
                        && game.room.is_none()
                        && game.room_make.is_none()
                        && let Some((how, title)) = std::env::var("RUSTNAROK_CLIENT_ROOM")
                            .ok()
                            .and_then(|room| room.split_once(':').map(|(how, title)| (how.to_owned(), title.to_owned())))
                        && !game.signs.values().any(|sign| sign.title == title)
                        && !std::mem::replace(&mut game.room_tried, true)
                    {
                        let mut make = chatroom::Make::default();
                        make.type_text(&title);
                        game.room_make = Some(make);
                        if how == "make" {
                            game.make_room();
                        }
                    }
                    if let Some(name) = std::env::var("RUSTNAROK_CLIENT_TRADE").ok().and_then(|trade| trade.strip_prefix("ask:").map(str::to_owned))
                        && game.trade_with.is_empty()
                        && game.trade.is_none()
                        && game.entered.elapsed() > Duration::from_secs(3)
                    {
                        game.ask_trade(&name);
                    }
                    if let Some(whispers) = std::env::var("RUSTNAROK_CLIENT_WHISPER").ok().filter(|_| game.whispered.is_empty() && game.chat.whisper.is_empty())
                    {
                        let effects_on = self.graphics.effects_on;
                        for whisper in whispers.split(';') {
                            let (to, text) = whisper.split_once('|').unwrap_or((whisper, "hi"));
                            game.chat.whisper = to.to_owned();
                            game.chat.input = text.to_owned();
                            let _ = game.send_chat(false, false, &self.messages, &mut self.graphics.effects_on);
                        }
                        if effects_on != self.graphics.effects_on {
                            self.graphics.save();
                        }
                    }
                }
                if game.entered.elapsed() > Duration::from_millis(1500) {
                    if let Some(name) = game.auto_talk.take()
                        && let Some((&id, _)) = game.units.iter().find(|(_, actor)| actor.kind == NPC && actor.name.contains(&name))
                    {
                        let _ = game.session.talk(id);
                    }
                    if game.auto_next > 0 && game.dialog.as_ref().is_some_and(|dialog| dialog.button != DialogButton::None) {
                        game.auto_next -= 1;
                        game.press_dialog_button();
                    }
                    if let Some(dialog) = &mut game.dialog
                        && let Some((items, selected)) = &mut dialog.menu
                        && let Some(choice) = game.auto_menu.pop()
                    {
                        *selected = choice.saturating_sub(1).min(items.len().saturating_sub(1));
                        game.choose(false);
                    }
                    let me = game.player.cell;
                    let distance = |cell: [i32; 2]| (cell[0] - me[0]).abs().max((cell[1] - me[1]).abs());
                    if game.auto_attack
                        && let Some((&id, _)) = game.units.iter().filter(|(_, actor)| actor.kind == MONSTER).min_by_key(|(_, actor)| distance(actor.cell))
                    {
                        game.auto_attack = false;
                        game.attack(id);
                    }
                    if game.auto_pick_up
                        && let Some((&id, _)) = game.ground.iter().min_by_key(|(_, item)| distance(item.at))
                    {
                        game.auto_pick_up = false;
                        game.pick_up(id, now);
                    }
                    if game.entered.elapsed() >= Duration::from_secs(3)
                        && !game.skills.is_empty()
                        && let Some((id, level)) = game.auto_skill.take()
                    {
                        game.use_skill(id, level);
                        if game.targeting.is_some() {
                            game.aim_skill(SkillTarget::Unit(game.account_id));
                        }
                    }
                    if let Some(&(at, slot)) = game.auto_keys.first()
                        && game.entered.elapsed() >= at
                    {
                        game.auto_keys.remove(0);
                        game.press_hotkey(slot);
                    }
                    if game.entered.elapsed() > Duration::from_millis(2500) && std::mem::take(&mut game.auto_wear) {
                        let wearable: Vec<(u16, u32)> = game
                            .inventory
                            .items
                            .iter()
                            .filter(|item| item.location != 0 && item.worn == 0 && item.identified && item.kind != net::ITEM_CARD)
                            .map(|item| (item.index, item.location))
                            .collect();
                        for (index, location) in wearable {
                            let _ = game.session.wear(index, location);
                        }
                    }
                }
                if let Some(gpu) = &self.gpu {
                    gpu.window.request_redraw();
                }
                event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + FRAME_TIME));
            }
            Scene::Login => {
                let phase = self.blink_phase();
                if phase != self.drawn_blink_phase
                    && let Some(gpu) = &self.gpu
                {
                    gpu.window.request_redraw();
                }
                let next_toggle = self.blink_start + CARET_BLINK * (phase as u32 + 1);
                event_loop.set_control_flow(ControlFlow::WaitUntil(next_toggle));
            }
            // The select and creation screens animate (the selected card, the characters, the caret).
            scene if matches!(scene.base(), Scene::CharSelect(_) | Scene::CharMake(..)) => {
                if let Some(gpu) = &self.gpu {
                    gpu.window.request_redraw();
                }
                event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + FRAME_TIME));
            }
            _ => event_loop.set_control_flow(ControlFlow::Wait),
        }
    }
}

fn main() {
    // Every panic, also a caught one (a packet skipped), goes to crash.log in the settings folder with its backtrace:
    // a bundled app's stderr goes nowhere.
    // ponytail: the file only grows; cap it if a server ever sends a stream of unreadable packets.
    let report = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        use std::io::Write as _;
        if let Some(path) = settings_path("crash.log")
            && path.parent().map_or(Ok(()), std::fs::create_dir_all).is_ok()
            && let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path)
        {
            let seconds = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
            let thread = std::thread::current().name().unwrap_or("unnamed").to_owned();
            let _ = writeln!(file, "unix time {seconds}, thread {thread}: {info}\n{}", std::backtrace::Backtrace::force_capture());
        }
        report(info);
    }));
    // Started as Ragnarok.app: work in the bundle's folder, where data.grf is (Finder starts apps in /).
    if let Ok(exe) = std::env::current_exe()
        && let Some(folder) = exe.ancestors().find(|path| path.extension().is_some_and(|extension| extension == "app")).and_then(|bundle| bundle.parent())
    {
        let _ = std::env::set_current_dir(folder);
    }
    let grf_path = std::env::var("RUSTNAROK_CLIENT_GRF").unwrap_or_else(|_| "data.grf".into());
    let mut grf = Grf::open(&grf_path).unwrap_or_else(|error| panic!("{grf_path}: {error}"));
    // Like the official client, loose files in the data folder (here, a translation) go before the archive.
    let overlay = std::env::var("RUSTNAROK_CLIENT_DATA").unwrap_or_else(|_| ".".into());
    if overlay != "none" {
        let found = grf.add_overlay(std::path::Path::new(&overlay));
        if found > 0 {
            eprintln!("{found} files from the data folder at {overlay}");
        }
    }
    #[cfg_attr(not(target_os = "macos"), allow(unused_mut))]
    let mut event_loop = EventLoop::<NetEvent>::with_user_event();
    // A client driven over the control socket opens without taking the keyboard from the app in use.
    #[cfg(target_os = "macos")]
    winit::platform::macos::EventLoopBuilderExtMacOS::with_activate_ignoring_other_apps(
        &mut event_loop,
        std::env::var_os("RUSTNAROK_CLIENT_CONTROL").is_none(),
    );
    let event_loop = event_loop.build().expect("event loop");
    control::listen(event_loop.create_proxy());
    let pets = pet::Data::load(&grf);
    // ponytail: without the translation's achievements.lub the window lists nothing and names achievements by ID.
    let achievement_info = lua::achievements(&grf).unwrap_or_default();
    let synthesis_boxes = lua::synthesis_boxes(&grf).unwrap_or_else(|error| {
        eprintln!("no laphine synthesis table: {error}");
        HashMap::new()
    });
    let upgrade_boxes = lua::upgrade_boxes(&grf).unwrap_or_else(|error| {
        eprintln!("no laphine upgrade table: {error}");
        HashMap::new()
    });
    let title_names = lua::titles(&grf).unwrap_or_default();
    let job_names = lua::job_names(&grf).unwrap_or_else(|error| {
        eprintln!("no NPC/monster sprite names: {error}");
        HashMap::new()
    });
    let monster_sizes = lua::monster_sizes(&grf);
    let shadow_factors = lua::shadow_factors(&grf).unwrap_or_else(|error| {
        eprintln!("no shadow table: {error}");
        HashMap::new()
    });
    let items = lua::item_info(&grf).unwrap_or_else(|error| {
        eprintln!("no item names: {error}");
        HashMap::new()
    });
    let card_names = inventory::CardNames::load(&grf);
    let card_pictures = iteminfo::CardPictures::load(&grf);
    let battlegrounds = lua::entry_queue(&grf).unwrap_or_else(|error| {
        eprintln!("no battleground list: {error}");
        Vec::new()
    });
    // ItemDBNameTbl is shared with the enchant window: it calls lua::item_db_names on the same file.
    let item_names = grf.read(lua::ITEM_DB_NAMES).and_then(lua::item_db_names).unwrap_or_else(|error| {
        eprintln!("no item name table: {error}");
        HashMap::new()
    });
    let reforms = grf.read(lua::ITEM_REFORM).and_then(|bytes| lua::reform_table(bytes, &item_names)).unwrap_or_else(|error| {
        eprintln!("no item reform list: {error}");
        HashMap::new()
    });
    let enchant_tables = lua::enchants(&grf).unwrap_or_else(|error| {
        eprintln!("no enchant tables: {error}");
        HashMap::new()
    });
    let styling_shop = lua::load_styling_shop(&grf).unwrap_or_else(|error| {
        eprintln!("no styling shop: {error}");
        lua::StylingShop::default()
    });
    let messages = messages::load(&grf).unwrap_or_else(|error| {
        eprintln!("no client messages: {error}");
        Vec::new()
    });
    let indoor = rustnarok_client::map::indoor_maps(&grf).unwrap_or_default();
    let viewpoints = rustnarok_client::map::viewpoints(&grf).unwrap_or_default();
    // A client the harness drives starts from the default view and keeps it to itself.
    let driven = std::env::var_os("RUSTNAROK_CLIENT_CONTROL").is_some();
    let saved_view = if driven {
        SavedView::harness()
    } else {
        settings_path("camera").and_then(|path| std::fs::read_to_string(path).ok()).map_or_else(SavedView::default, |text| SavedView::parse(&text))
    };
    let pc_job_names = lua::pc_job_names(&grf).unwrap_or_default();
    let map_names = rustnarok_client::map::map_names(&grf).unwrap_or_default();
    let skill_descriptions = lua::skill_descriptions(&grf).unwrap_or_else(|error| {
        eprintln!("no skill descriptions: {error}");
        HashMap::new()
    });
    let skill_info = lua::skill_info(&grf).unwrap_or_else(|error| {
        eprintln!("no skill names: {error}");
        HashMap::new()
    });
    let skill_trees = lua::skill_trees(&grf).unwrap_or_default();
    let reputation_tables = reputation::Tables::load(&grf);
    let navi_tables = lua::navigation(&grf).unwrap_or_default();
    let status_icon_files = lua::status_icons(&grf).unwrap_or_default();
    let state_infos = lua::state_info(&grf).unwrap_or_else(|error| {
        eprintln!("no status icon descriptions: {error}");
        HashMap::new()
    });
    let mut look_tables = lua::look_tables(&grf).unwrap_or_else(|error| {
        eprintln!("no equipment sprite names: {error}");
        LookTables::default()
    });
    look_tables.item_views = items.iter().map(|(&id, info)| (id, info.class_num)).collect();
    let cursor_look = cursor::Cursor::load(&grf);
    let mut loading_images: Vec<String> =
        grf.paths().filter(|path| path.starts_with(&format!("{UI}loading")) && path.ends_with(".jpg")).map(str::to_owned).collect();
    loading_images.sort();
    let music = sound::music_table(&grf);
    let worlds = rustnarok_client::lua::worlds(&grf).unwrap_or_default();
    let connections = rustnarok_client::clientinfo::load(&grf);
    // Several servers: the list comes first, as in the official client (dev logins take the first).
    let picking = connections.len() > 1 && std::env::var_os("RUSTNAROK_CLIENT_SERVER").is_none() && std::env::var_os("RUSTNAROK_CLIENT_LOGIN").is_none();
    let quest_texts = quest::texts(&grf);
    let sound = sound::Settings::load();
    let graphics = options::GraphicsSettings::load();
    let shortcuts = options::ShortcutSettings::load();
    let mut keybinds = shortcuts::Shortcuts::load(&grf);
    // ponytail: the shortcut-settings window writes its own file, not UserKeys, so a saved bar overlays hotbar 1.
    if shortcuts != options::ShortcutSettings::default_bindings() {
        let mut keys = [None; 36];
        for (slot, key) in shortcuts.slots.into_iter().enumerate() {
            keys[slot] = key.and_then(shortcut_to_bind);
        }
        keybinds.apply_bar_keys(keys);
    }
    let mut app = App {
        grf,
        job_names,
        monster_sizes,
        shadow_factors,
        pets,
        achievement_info,
        synthesis_boxes,
        upgrade_boxes,
        title_names,
        items,
        battlegrounds,
        card_names,
        card_pictures,
        reforms,
        enchant_tables,
        styling_shop,
        messages,
        icons: HashMap::new(),
        modifiers: ModifiersState::empty(),
        indoor,
        viewpoints,
        saved_view,
        save_view: !driven,
        look_tables,
        pc_job_names,
        map_names,
        worlds,
        quest_texts,
        skill_info,
        skill_descriptions,
        skill_trees,
        reputation_tables,
        navi_tables,
        status_icon_files,
        state_infos,
        effect_table: effects::Table::load(),
        // Driven or screenshot runs stay quiet unless RUSTNAROK_CLIENT_SOUND=1.
        audio: (std::env::var("RUSTNAROK_CLIENT_SOUND").is_ok_and(|sound| sound == "1")
            || std::env::var_os("RUSTNAROK_CLIENT_SCREENSHOT").is_none() && std::env::var_os("RUSTNAROK_CLIENT_CONTROL").is_none())
        .then(|| sound::Audio::new(&sound))
        .flatten(),
        sound,
        music,
        graphics,
        shortcuts,
        keybinds,
        print_screen: None,
        shot_saved: None,
        cursor_look,
        loading_images,
        loading: None,
        font: Font::load(),
        proxy: event_loop.create_proxy(),
        gpu: None,
        assets: None,
        scene: if picking { Scene::Connections { selected: 0 } } else { Scene::Login },
        connections,
        connection: 0,
        char_session: None,
        char_server: 0,
        quitting: false,
        returned: false,
        account: None,
        playing: None,
        entering: None,
        early_events: Vec::new(),
        server_clock: None,
        fields: [saved_id().unwrap_or_default(), String::new()],
        save_id: saved_id().is_some(),
        focus: usize::from(saved_id().is_some()),
        cursor: std::env::var("RUSTNAROK_CLIENT_CURSOR")
            .ok()
            .and_then(|cursor| {
                let (x, y) = cursor.split_once(',')?;
                Some([x.parse().ok()?, y.parse().ok()?])
            })
            .unwrap_or([-1.0; 2]),
        mouse_down: false,
        auto_input: {
            let start = std::env::var("RUSTNAROK_CLIENT_CLICKS_AT").ok().and_then(|seconds| seconds.parse().ok()).unwrap_or(4);
            let point = |text: &str| -> Option<[f32; 2]> {
                let (x, y) = text.trim().split_once(',')?;
                Some([x.trim().parse().ok()?, y.trim().parse().ok()?])
            };
            let steps = std::env::var("RUSTNAROK_CLIENT_CLICKS").unwrap_or_default();
            let steps: Vec<_> = steps
                .split(';')
                .filter(|step| !step.trim().is_empty())
                .enumerate()
                .filter_map(|(order, step)| {
                    // "r" first: the right button.
                    let (right, step) = step.trim().strip_prefix('r').map_or((false, step), |rest| (true, rest));
                    let (from, to) = match step.split_once('>') {
                        Some((from, to)) => (point(from)?, Some(point(to)?)),
                        None => (point(step)?, None),
                    };
                    Some((Duration::from_secs(start + order as u64), right, from, to))
                })
                .collect();
            steps.into_iter().rev().collect()
        },
        blink_start: Instant::now(),
        drawn_blink_phase: u128::MAX,
        started: Instant::now(),
        screenshot: std::env::var("RUSTNAROK_CLIENT_SCREENSHOT").ok(),
        scripted: false,
        played_slot: None,
    };
    if !picking && !app.connections.is_empty() {
        app.choose_connection(0);
    }
    app.play_music(None);
    event_loop.run_app(&mut app).expect("run");
}

/// Puts the real look, plus the window's picks, on the stylist's preview.
fn dress_stylist(window: &mut stylist::Window, grf: &Grf, tables: &LookTables, look: &Look, items: &HashMap<u32, ItemInfo>, shop: &lua::StylingShop) {
    window.dress(grf, tables, look, shop, |id| items.get(&id).map(|info| info.class_num as u16).unwrap_or(0));
}

/// Locks a pair of units (Blade Stop, Cursed Circle), or lets go of every pair that holds either of them.
fn lock_pair(locks: &mut Vec<(u32, u32)>, source: u32, target: u32, on: bool) {
    if !on {
        locks.retain(|&(a, b)| ![a, b].iter().any(|id| *id == source || *id == target));
    } else if !locks.contains(&(source, target)) {
        locks.push((source, target));
    }
}

/// How many units a unit is locked with.
fn locked_with(locks: &[(u32, u32)], id: u32) -> usize {
    locks.iter().filter(|&&(source, target)| source == id || target == id).count()
}

/// The share of a window (a combo's) still left.
fn share_left(start: Instant, length: Duration, now: Instant) -> f32 {
    1.0 - (now.saturating_duration_since(start).as_secs_f32() / length.as_secs_f32().max(f32::EPSILON)).min(1.0)
}

/// What an NPC deal's result says, and in which colour (None: nothing). Selling: done or failed. Buying: done, zeny,
/// weight, the item limits, RODEX, the exchange results.
fn deal_result(sold: bool, result: u8) -> Option<(usize, [u8; 3])> {
    match (sold, result) {
        (_, 0) => Some((54, CHAT_ITEM)),
        (true, 1) => Some((57, CHAT_ERROR)),
        (true, _) => None,
        (false, 1) => Some((55, CHAT_ERROR)),
        (false, 2) => Some((56, CHAT_ERROR)),
        (false, 3) => Some((221, CHAT_ERROR)),
        (false, 9) => Some((1940, CHAT_ERROR)),
        (false, 10) => Some((2917, CHAT_ITEM)),
        (false, 11) => Some((3554, CHAT_ERROR)),
        (false, 12) => Some((3555, CHAT_ITEM)),
        (false, 13) => Some((3557, CHAT_ERROR)),
        (false, 14) => Some((3556, CHAT_ERROR)),
        (false, _) => None,
    }
}

/// Turning on the spot toward rathena direction `target` (0 north, counting counter-clockwise): the body and the head
/// (0 straight, 1 right, 2 left) after. Ahead or behind, the body turns. 45° aside, the head turns, or the body when the
/// head already looks that way. 90° to 135° aside, the body turns one sector short and the head looks the rest.
fn turn_toward(body: u8, head: u8, target: u8) -> (u8, u8) {
    match (body + 8 - target) % 8 {
        1 if head == 1 => (target, 0),
        1 => (body, 1),
        2 | 3 => ((target + 1) % 8, 1),
        7 if head == 2 => (target, 0),
        7 => (body, 2),
        5 | 6 => ((target + 7) % 8, 2),
        _ => (target, 0),
    }
}

/// Why an item can't be dropped on the ground now: `Some(None)` quietly, `Some(Some(message))` saying so; `None` when
/// it can. In order: a wear asked for under 2 s ago, a deal or talk under way, the equipment window, the storage, the
/// mail window, the ammunition worn.
fn ground_drop_refused(since_wear: Option<Duration>, busy: bool, equipment: bool, storage: bool, mail: bool, ammo: bool) -> Option<Option<usize>> {
    if since_wear.is_some_and(|since| since < WEAR_PAUSE) || busy {
        Some(None)
    } else if equipment {
        Some(Some(DROP_EQUIPMENT_OPEN))
    } else if storage {
        Some(None)
    } else if mail {
        Some(Some(DROP_MAIL_OPEN))
    } else if ammo {
        Some(Some(DROP_AMMO_WORN))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_head_turns_before_the_body() {
        // Facing north (0): north-east (7) is 45° right: the head looks right, then the body turns.
        assert_eq!(turn_toward(0, 0, 7), (0, 1));
        assert_eq!(turn_toward(0, 1, 7), (7, 0));
        // East (6), 90° right: the body to north-east, the head right; south-east (5), 135°: the body east.
        assert_eq!(turn_toward(0, 0, 6), (7, 1));
        assert_eq!(turn_toward(0, 0, 5), (6, 1));
        // Left: north-west (1) the head; west (2) and south-west (3) one sector short.
        assert_eq!(turn_toward(0, 0, 1), (0, 2));
        assert_eq!(turn_toward(0, 2, 1), (1, 0));
        assert_eq!(turn_toward(0, 0, 2), (1, 2));
        assert_eq!(turn_toward(0, 0, 3), (2, 2));
        // Ahead and behind: the body, the head straight.
        assert_eq!(turn_toward(0, 2, 0), (0, 0));
        assert_eq!(turn_toward(0, 1, 4), (4, 0));
    }

    #[test]
    fn some_things_open_keep_items_off_the_ground() {
        let refused = |since: Option<u64>, open: [bool; 5]| ground_drop_refused(since.map(Duration::from_millis), open[0], open[1], open[2], open[3], open[4]);
        assert_eq!(refused(None, [false; 5]), None);
        assert_eq!(refused(Some(2000), [false; 5]), None);
        assert_eq!(refused(Some(1999), [false; 5]), Some(None));
        assert_eq!(refused(None, [true, true, false, false, false]), Some(None), "a trade first, quietly");
        assert_eq!(refused(None, [false, true, true, true, true]), Some(Some(DROP_EQUIPMENT_OPEN)));
        assert_eq!(refused(None, [false, false, true, true, true]), Some(None), "the storage, quietly");
        assert_eq!(refused(None, [false, false, false, true, true]), Some(Some(DROP_MAIL_OPEN)));
        assert_eq!(refused(None, [false, false, false, false, true]), Some(Some(DROP_AMMO_WORN)));
    }

    #[test]
    fn saves_where_the_windows_were_left() {
        let saved = window_places([("inventory", [0.0, 120.0], false), ("status", [185.5, 300.0], true)].into_iter());
        assert_eq!(saved, "inventory 0 120 0\nstatus 185.5 300 1\n");
        let read = parse_window_places(&saved);
        assert_eq!((read["inventory"], read["status"]), (([0.0, 120.0], false), ([185.5, 300.0], true)));
        assert!(parse_window_places("broken line\nskills 1 2 0").contains_key("skills"));
    }

    #[test]
    fn colours_names_by_who_they_are() {
        // NPCs by type or job, monsters by job and level against ours, our own summons, players.
        assert_eq!(name_color(NPC, 100, 0, false, 50), NPC_NAME_COLOR);
        assert_eq!(name_color(MONSTER, 10_005, 0, false, 50), NPC_NAME_COLOR);
        assert_eq!(name_color(MONSTER, 1002, 50, false, 50), MONSTER_NAME_COLOR);
        assert_eq!(name_color(MONSTER, 1002, 60, false, 50), MONSTER_NAME_COLOR);
        assert_eq!(name_color(MONSTER, 1002, 61, false, 50), STRONG_NAME_COLOR);
        assert_eq!(name_color(MONSTER, 1002, 35, false, 50), MONSTER_NAME_COLOR);
        assert_eq!(name_color(MONSTER, 1002, 34, false, 50), WEAK_NAME_COLOR);
        assert_eq!(name_color(HOMUNCULUS, 6001, 10, true, 50), SUMMON_NAME_COLOR);
        assert_eq!(name_color(HOMUNCULUS, 6001, 10, false, 50), CHAT_WHITE);
        assert_eq!(name_color(PLAYER, 4008, 99, false, 50), CHAT_WHITE);
    }

    #[test]
    fn canvas_widens_with_the_window() {
        assert_eq!(canvas_width([2048, 1536]), 1024.0);
        assert_eq!(canvas_width([3840, 2160]), 1365.0, "16:9");
        assert_eq!(canvas_width([1000, 1000]), 1024.0, "narrower than 4:3 keeps 1024, with bars above and below");
    }

    #[test]
    fn rings_where_a_unit_teleported_or_logged_out() {
        assert_eq!(vanish_effect(3, 0), Some(304));
        assert_eq!(vanish_effect(3, actor::OPTION_INVISIBLE), None);
        assert_eq!(vanish_effect(2, 0), Some(314));
        assert_eq!((vanish_effect(0, 0), vanish_effect(1, 0)), (None, None));
    }

    #[test]
    fn shows_the_attack_and_turning_cursors_only_when_they_apply() {
        assert_eq!(monster_cursor(false, false), cursor::Kind::Attack);
        assert_eq!(monster_cursor(true, false), cursor::Kind::Default);
        assert_eq!(monster_cursor(true, true), cursor::Kind::Attack);
        // The right button held: no key and Ctrl turn (Ctrl isn't asked); Shift tilts; Alt doesn't.
        assert!(turning_cursor(true, false, false) && !turning_cursor(true, true, false) && !turning_cursor(true, false, true));
        assert!(!turning_cursor(false, false, false));
    }

    #[test]
    fn picks_warps_in_a_box_around_their_feet() {
        let camera = |distance: f32| Camera { target: [100.0, 0.0, 100.0], distance, pitch: 45f32.to_radians(), yaw: 0.0, ground: f32::MIN };
        let size = |distance: f32| {
            let [left, top, right, bottom] = warp_pick_rect(&camera(distance).view(4.0 / 3.0), [100.0, 0.0, 100.0]).unwrap();
            [(right - left) * 1024.0, (bottom - top) * 768.0]
        };
        // About 166 by 109 screen pixels at the default distance, centred on the feet; smaller farther out.
        let [width, height] = size(300.0);
        assert!((width - 166.0).abs() < 8.0 && (height - 109.0).abs() < 6.0, "{width} {height}");
        let far = size(400.0);
        assert!(far[0] < width * 0.8 && far[1] < height * 0.8, "{far:?}");
    }

    #[test]
    fn snaps_the_cursor_to_the_middle_of_what_it_points_at() {
        let first_run = Snaps { snap: false, item: false, skill: true };
        let all = Snaps { snap: true, item: true, skill: true };
        let box_at = |kind: SnapTo| Some((kind, [10.0, 20.0, 50.0, 100.0]));
        // Aiming a skill at a unit: a monster with /skillsnap; not aiming: only with /snap.
        assert_eq!(snap_point(box_at(SnapTo::Monster), true, false, first_run), Some([30.0, 60.0]));
        assert_eq!(snap_point(box_at(SnapTo::Monster), false, false, first_run), None);
        assert_eq!(snap_point(box_at(SnapTo::Monster), false, false, all), Some([30.0, 60.0]));
        assert_eq!(snap_point(box_at(SnapTo::Monster), true, false, Snaps { skill: false, ..all }), None);
        // Items with /itemsnap; pets, players and the like never; our homunculus only for Potion Pitcher.
        assert_eq!(
            (snap_point(box_at(SnapTo::Item), false, false, first_run), snap_point(box_at(SnapTo::Item), false, false, all)),
            (None, Some([30.0, 60.0]))
        );
        assert_eq!(snap_point(box_at(SnapTo::Other), true, false, all), None);
        assert_eq!(snap_point(box_at(SnapTo::OurHomunculus), true, false, all), None);
        assert_eq!(snap_point(box_at(SnapTo::OurHomunculus), true, true, first_run), Some([30.0, 60.0]));
        assert_eq!(snap_point(None, true, false, all), None);
    }

    #[test]
    fn dead_players_lie_there_until_revived() {
        let now = Instant::now();
        let mut player = Actor::new("Someone".into(), None, [10, 10], 0, 150);
        player.kind = PLAYER;
        fall(&mut player, now);
        assert!(player.dead && player.remove_at.is_none(), "a player's body stays");
        player.revive();
        assert!(!player.dead);
        let mut monster = Actor::new("Poring".into(), None, [10, 10], 0, 150);
        monster.kind = MONSTER;
        fall(&mut monster, now);
        // No sprite here, so no death to play: the corpse fades at once, over 6.12 s, then goes.
        assert_eq!(monster.remove_at, Some(now + Duration::from_millis(6120)));
        assert!(monster.fading(now) && !monster.fading(now - Duration::from_millis(1)));
        assert_eq!(monster.alpha(now), 1.0);
        assert!((monster.alpha(now + Duration::from_millis(3060)) - 0.5).abs() < 0.001);
        assert_eq!(monster.alpha(now + Duration::from_millis(7000)), 0.0);
    }

    #[test]
    fn drops_items_with_a_hop_and_lets_them_glint() {
        // 15 units up, a little higher at about 87 ms, on the ground by about 0.42 s, and staying there.
        assert_eq!(item_fall_height(0.0), 15.0);
        assert!(item_fall_height(3.6) > 16.0 && item_fall_height(3.6) < 16.1);
        assert!(item_fall_height(17.4) > 0.0 && item_fall_height(17.6) == 0.0 && item_fall_height(100.0) == 0.0);
        // A yellow glint for 2 ticks in every 92.
        assert_eq!([0.0, 89.9, 90.0, 91.9, 92.0, 182.5].map(item_glints), [false, false, true, true, false, true]);
        // No picture of its own: the apple.
        let mut items = HashMap::new();
        assert_eq!(ground_sprite(&items, 501, true), FALLBACK_ITEM_SPRITE);
        items.insert(501, ItemInfo { resource: "빨간포션".into(), ..ItemInfo::default() });
        assert_eq!((ground_sprite(&items, 501, true), ground_sprite(&items, 501, false)), ("빨간포션", FALLBACK_ITEM_SPRITE));
    }

    #[test]
    fn units_out_of_sight_fade_quickly() {
        let now = Instant::now();
        let mut unit = Actor::new("Poring".into(), None, [10, 10], 0, 150);
        assert!(!unit.fading(now) && unit.alpha(now) == 1.0);
        unit.fade_out(now, OUT_OF_SIGHT_FADE_MS);
        assert_eq!(unit.remove_at, Some(now + Duration::from_millis(510)));
        assert!((unit.alpha(now + Duration::from_millis(255)) - 0.5).abs() < 0.001);
    }

    #[test]
    fn places_the_pvp_counter() {
        let screen = [1024.0, 768.0];
        assert_eq!(counter_glyphs(3, 12, screen), [(3, [919.0, 718.0]), (GLYPH_SEPARATOR, [944.0, 728.0]), (1, [969.0, 738.0]), (2, [994.0, 738.0])]);
        // A two-digit rank ends at the same place; the count stops at 9999.
        let glyphs = counter_glyphs(12, 15_000, screen);
        assert_eq!(&glyphs[..2], [(1, [894.0, 718.0]), (2, [919.0, 718.0])]);
        assert_eq!(glyphs[3..].iter().map(|(digit, _)| *digit).collect::<Vec<_>>(), [9, 9, 9, 9]);
    }

    #[test]
    fn spells_the_first_place_clock() {
        let colon = GLYPH_SEPARATOR;
        assert_eq!(clock_glyphs(59), [0, 0, colon, 5, 9]);
        assert_eq!(clock_glyphs(61), [0, 1, colon, 0, 1]);
        assert_eq!(clock_glyphs(3 * 3600 + 2 * 60 + 1), [0, 3, colon, 0, 2, colon, 0, 1]);
        assert_eq!(clock_glyphs(2 * 86_400), [0, 2, colon, 0, 0, colon, 0, 0, colon, 0, 0]);
        assert_eq!(clock_glyphs(0), [0, 0, colon, 0, 0]);
        assert_eq!(clock_glyphs(3661), [0, 1, colon, 0, 1, colon, 0, 1]);
        assert_eq!(clock_glyphs(90_061), [0, 1, colon, 0, 1, colon, 0, 1, colon, 0, 1]);
    }

    #[test]
    fn tells_when_the_rank_crosses_the_top_ten() {
        assert_eq!(rank_line(11, 10), Some(RANK_UP));
        assert_eq!(rank_line(10, 11), Some(RANK_DOWN));
        assert_eq!(rank_line(0, 11), Some(RANK_DOWN), "a first rank outside the top ten");
        assert_eq!(rank_line(0, 3), Some(RANK_UP));
        assert_eq!(rank_line(5, 3), None);
    }

    #[test]
    fn keeps_a_messages_last_sentence() {
        assert_eq!(last_sentence("Stats are limited. Are you sure?"), "Are you sure?");
        assert_eq!(last_sentence("Are you sure?"), "Are you sure?");
    }

    #[test]
    fn tells_what_a_map_turns_off() {
        let miracle = net::MAP_SUN_MOON_STAR_MIRACLE;
        assert_eq!(arrival_notices(miracle, 0), Vec::<usize>::new(), "prontera: nothing");
        assert_eq!(arrival_notices(miracle | net::MAP_NO_COSTUME, 4047), [NO_COSTUME_NOTICE]);
        assert_eq!(arrival_notices(0, 4047), [NO_MIRACLE_NOTICE], "a Star Gladiator where the Miracle is off");
        assert_eq!(arrival_notices(0, 4302), [NO_MIRACLE_NOTICE], "a Sky Emperor too");
        assert_eq!(arrival_notices(0, 4008), Vec::<usize>::new(), "anyone else: nothing");
        assert_eq!(arrival_notices(net::MAP_NO_COSTUME, 4316), [NO_COSTUME_NOTICE, NO_MIRACLE_NOTICE]);
    }

    #[test]
    fn keeps_what_circles_a_unit() {
        let mut spirits = HashMap::new();
        change_orbs(&mut spirits, None, |orbs| orbs.spheres = 5);
        change_orbs(&mut spirits, None, |orbs| orbs.charms = (3, 2));
        change_orbs(&mut spirits, Some(7), |orbs| orbs.souls = 4);
        assert_eq!(spirits[&None], Orbs { spheres: 5, charms: (3, 2), ..Orbs::default() });
        // A 0 clears only its kind; a unit with nothing left leaves.
        change_orbs(&mut spirits, None, |orbs| orbs.charms = (3, 0));
        assert_eq!(spirits[&None], Orbs { spheres: 5, ..Orbs::default() });
        change_orbs(&mut spirits, Some(7), |orbs| orbs.souls = 0);
        assert!(!spirits.contains_key(&Some(7)));
        change_orbs(&mut spirits, None, |orbs| orbs.spheres = 0);
        assert!(spirits.is_empty());
        assert!(guard_job(4066) && !guard_job(4054));
        // A glow on black: black goes clear, a dim red stays red, half as opaque.
        let image = render::glow(image::RgbaImage::from_pixel(1, 1, image::Rgba([128, 0, 0, 255])));
        assert_eq!(image.get_pixel(0, 0).0, [255, 0, 0, 128]);
        assert_eq!(render::glow(image::RgbaImage::new(1, 1)).get_pixel(0, 0).0, [0; 4]);
    }

    #[test]
    fn locks_and_lets_go_of_pairs() {
        // Cursed Circle: one Sura (1) catches two units; the release names the last one and the Sura.
        let mut locks = Vec::new();
        lock_pair(&mut locks, 1, 2, true);
        lock_pair(&mut locks, 1, 3, true);
        lock_pair(&mut locks, 1, 3, true);
        assert_eq!((locked_with(&locks, 1), locked_with(&locks, 2), locked_with(&locks, 4)), (2, 1, 0));
        lock_pair(&mut locks, 3, 1, false);
        assert!(locks.is_empty());
        // Blade Stop: either side lets go of the pair.
        lock_pair(&mut locks, 5, 6, true);
        lock_pair(&mut locks, 6, 0, false);
        assert!(locks.is_empty());
        // A combo window empties as it runs out.
        let start = Instant::now();
        let length = Duration::from_millis(800);
        assert_eq!(share_left(start, length, start), 1.0);
        assert_eq!(share_left(start, length, start + Duration::from_millis(200)), 0.75);
        assert_eq!(share_left(start, length, start + Duration::from_secs(2)), 0.0);
    }

    #[test]
    fn rounds_or_floors_the_facing_by_unit() {
        let attack = actor::attack_action(0, 0, 1);
        assert!(rounds_facing(PLAYER, 0, actor::IDLE) && rounds_facing(PLAYER, 0, actor::WALK) && rounds_facing(PLAYER, 0, actor::SIT));
        assert!(!rounds_facing(PLAYER, 0, attack));
        assert!(!rounds_facing(MONSTER, 1002, actor::IDLE) && !rounds_facing(MONSTER, 1002, actor::WALK));
        assert!(rounds_facing(NPC, 100, actor::IDLE) && !rounds_facing(NPC, 100, actor::WALK) && !rounds_facing(NPC, WARP_JOB, actor::IDLE));
        assert!(rounds_facing(MONSTER, 6020, actor::WALK) && !rounds_facing(MONSTER, 6020, attack));
    }

    #[test]
    fn moves_items_with_a_modifier() {
        assert_eq!(
            [inventory_right(true, true, false), inventory_right(false, true, true), inventory_right(false, false, true), inventory_right(false, false, false)],
            [Some(InventoryRight::QuickMove), Some(InventoryRight::Favorite), Some(InventoryRight::Identify), None]
        );
        assert_eq!(quick_target(true, true, true), Some(QuickTo::Storage));
        assert_eq!(quick_target(false, true, true), Some(QuickTo::Trade));
        assert_eq!(quick_target(false, false, true), Some(QuickTo::Cart));
        assert_eq!(quick_target(false, false, false), None);
        // Our Shift-click's link reads back as a link to the item.
        let (shown, runs) = links::split(&links::mark(&item_link(512, "Apple"), &|_| String::new()));
        assert_eq!((shown.as_str(), runs[0].1.item().map(|item| item.id)), ("Apple", Some(512)));
    }

    #[test]
    fn glides_and_keeps_the_view() {
        // A tenth of the way a 60th of a second, the same at any frame rate.
        assert!((follow_step(Duration::from_secs_f32(1.0 / 60.0)) - 0.1).abs() < 1e-4);
        assert!((follow_step(Duration::from_secs_f32(2.0 / 60.0)) - 0.19).abs() < 1e-4);
        // The view left on each kind of map, within that kind's limits.
        let outdoors = CameraGoal::arrive(false, None, [80.0, 500.0], false);
        assert_eq!((outdoors.pitch, outdoors.distance(), outdoors.yaw), (65.0, 400.0, 0.0));
        let indoors = CameraGoal::arrive(true, None, [20.0, 400.0], false);
        assert_eq!((indoors.pitch, indoors.distance(), indoors.yaw), (35.0, 300.0, YAW_INDOOR));
        // /zoom: outdoors out to 730, indoors no further.
        let mut far = CameraGoal::arrive(false, None, [45.0, 511.3], true);
        assert!((far.distance() - 511.3).abs() < 0.01);
        far.set_distance(1000.0);
        assert_eq!(far.distance(), ZOOM_OUT_DISTANCE);
        far.zoom_out = false;
        far.set_distance(far.distance());
        assert_eq!(far.distance(), 400.0);
        assert_eq!(CameraGoal::arrive(true, None, [45.0, 511.3], true).distance(), 300.0);
        // A map's own table starts it, and holds it.
        let table = rustnarok_client::map::Viewpoint {
            distance: [210.0, 260.0],
            distance_in: 250.0,
            yaw: [45.0, 65.0],
            yaw_in: 50.0,
            pitch: [35.0, 45.0],
            pitch_in: 40.0,
        };
        let mut own = CameraGoal::arrive(false, Some(table), [80.0, 500.0], true);
        assert_eq!((own.pitch, own.distance(), own.yaw), (40.0, 250.0, 50.0));
        own.steer(100.0, 0.0, false, false, false);
        assert_eq!(own.yaw, 65.0);
        // Saved views: read back, followed per kind of map.
        let mut saved = SavedView::parse("50 350 45 280");
        assert_eq!((saved.of(false), saved.of(true)), ([50.0, 350.0], [45.0, 280.0]));
        assert_eq!(SavedView::parse(&saved.text()), SavedView { changed: None, ..saved });
        assert_eq!(SavedView::parse("junk"), SavedView::default());
        // A first run: 400 outdoors, 300 indoors, tilted 45; the harness's client 300 both ways.
        assert_eq!(SavedView::default().views, [[45.0, 400.0], [45.0, 300.0]]);
        assert_eq!(SavedView::harness().views, [[45.0, 300.0]; 2]);
        // Arriving, a saved tilt is kept within 25 to 65 outdoors and 35 to 55 indoors.
        assert_eq!(CameraGoal::arrive(true, None, [60.0, 300.0], false).pitch, 55.0);
        assert_eq!(CameraGoal::arrive(false, None, [60.0, 400.0], false).pitch, 60.0);
        assert_eq!(CameraGoal::arrive(true, None, [20.0, 300.0], false).pitch, 35.0);
        assert_eq!(CameraGoal::arrive(false, None, [45.0, 400.0], false).distance(), 400.0);
        let now = Instant::now();
        saved.follow(&outdoors, now, false);
        assert_eq!((saved.of(false), saved.of(true)), ([65.0, 400.0], [45.0, 280.0]));
        assert!(saved.changed.is_some());
        saved.follow(&outdoors, now + VIEW_SAVE_AFTER, false);
        assert!(saved.changed.is_none());
    }

    #[test]
    fn steers_the_camera_at_the_games_rates() {
        let now = Instant::now();
        let mut outdoors = CameraGoal::new(false);
        // A degree a pixel, unlimited outdoors; Alt does nothing.
        outdoors.steer(360.0, 0.0, false, false, false);
        outdoors.steer(90.0, 0.0, false, false, true);
        assert_eq!(outdoors.yaw, 360.0);
        // Shift tilts half a degree a pixel, once a frame, 50 to 65 (25 to 65 with Ctrl).
        let drag = |goal: &mut CameraGoal, dx: f32, dy: f32, ctrl: bool, shift: bool| {
            goal.steer(dx, dy, ctrl, shift, false);
            goal.drag_frame();
        };
        drag(&mut outdoors, 0.0, 200.0, false, true);
        assert_eq!(outdoors.pitch, 65.0);
        drag(&mut outdoors, 0.0, -400.0, false, true);
        assert_eq!(outdoors.pitch, 50.0);
        // A pixel a frame drops its half degree each frame: ten frames, no tilt. Five in one frame: two steps.
        for _ in 0..10 {
            drag(&mut outdoors, 0.0, 1.0, true, true);
        }
        assert_eq!(outdoors.pitch, 50.0);
        drag(&mut outdoors, 0.0, 5.0, true, true);
        assert_eq!(outdoors.pitch, 52.0);
        // Moves within one frame add up before the halving.
        outdoors.steer(0.0, 1.0, true, true, false);
        outdoors.steer(0.0, 1.0, true, true, false);
        outdoors.drag_frame();
        assert_eq!(outdoors.pitch, 53.0);
        drag(&mut outdoors, 0.0, -400.0, true, true);
        assert_eq!(outdoors.pitch, 25.0);
        // Ctrl zooms (down comes closer) and turns too; 230 to 400 units.
        drag(&mut outdoors, 10.0, 400.0, true, false);
        assert_eq!((outdoors.distance(), outdoors.yaw), (230.0, 370.0));
        drag(&mut outdoors, 0.0, -1000.0, true, false);
        assert_eq!(outdoors.distance(), 400.0);
        // The wheel: one step however far, at most every 15 ms; with Shift it tilts.
        outdoors.wheel(3, now, false, false);
        assert!((outdoors.distance() - 375.7).abs() < 0.01);
        outdoors.wheel(1, now + Duration::from_millis(5), false, false);
        assert!((outdoors.distance() - 375.7).abs() < 0.01);
        // (Shift alone keeps to 50..65: the 25 it had comes up to 50.)
        outdoors.wheel(-1, now + Duration::from_millis(20), false, true);
        assert_eq!(outdoors.pitch, 50.0);
        // A double right-click with the pointer still: Ctrl the distance, Shift the tilt, otherwise the turn.
        outdoors.press([5.0, 5.0], now, false, false);
        outdoors.press([5.0, 5.0], now + Duration::from_millis(200), true, true);
        assert_eq!((outdoors.distance(), outdoors.pitch), (300.0, PITCH_RESET));
        // After a double click the next press starts a new pair: the third press of a triple click doesn't reset,
        // the fourth (its pair) turns back.
        outdoors.press([5.0, 5.0], now + Duration::from_millis(300), false, false);
        assert_eq!(outdoors.yaw, 370.0);
        outdoors.press([5.0, 5.0], now + Duration::from_millis(350), false, false);
        assert_eq!(outdoors.yaw, 360.0);
        outdoors.yaw = 30.0;
        outdoors.pointer_moved([9.0, 5.0]);
        outdoors.press([9.0, 5.0], now + Duration::from_millis(400), false, false);
        assert_eq!(outdoors.yaw, 30.0, "moved: not a double click");
        // A hand's tremor between the presses still makes one.
        outdoors.pointer_moved([9.6, 5.4]);
        outdoors.press([9.6, 5.4], now + Duration::from_millis(500), false, false);
        assert_eq!(outdoors.yaw, 0.0, "a tremor isn't a move");
        // Indoors: the turn stays within 25 to 65 and isn't reset; the tilt is fixed at 60 without Ctrl.
        let mut indoors = CameraGoal::new(true);
        indoors.steer(100.0, 0.0, false, false, false);
        assert_eq!(indoors.yaw, 65.0);
        indoors.steer(0.0, -50.0, false, true, false);
        indoors.drag_frame();
        assert_eq!(indoors.pitch, 60.0);
        indoors.press([5.0, 5.0], now, false, false);
        indoors.press([5.0, 5.0], now + Duration::from_millis(100), false, false);
        assert_eq!(indoors.yaw, 65.0);
        indoors.steer(0.0, -1000.0, true, false, false);
        indoors.drag_frame();
        assert_eq!(indoors.distance(), 300.0);
    }

    #[test]
    fn opens_menus_and_orders_companions() {
        assert!(menu_opens(false, false, false, false));
        assert!(!menu_opens(true, false, false, false) && !menu_opens(false, true, false, false));
        assert!(!menu_opens(false, false, true, false) && !menu_opens(false, false, false, true));
        // Without Shift the first press marks the monster and the second sends the attack; with Shift at once.
        assert_eq!(companion_mark(None, 1, 9, false), (Some((1, 9)), false));
        assert_eq!(companion_mark(Some((1, 9)), 1, 9, false), (None, true));
        assert_eq!(companion_mark(Some((1, 8)), 1, 9, false), (Some((1, 9)), false));
        assert_eq!(companion_mark(None, 1, 9, true), (None, true));
    }

    #[test]
    fn hits_sound_by_body_or_weapon() {
        // A Knight (7 from Swordman 1), a Hunter (11 from Archer 3), a Priest (8 from Acolyte 4).
        assert_eq!(hit_sound(Some(&[7, 1, 0]), None, 0), "player_metal.wav");
        assert_eq!(hit_sound(Some(&[11, 3, 0]), Some(1), 0), "player_wooden_male.wav");
        assert_eq!(hit_sound(Some(&[8, 4, 0]), None, 0), "player_clothes.wav");
        // On a monster: the attacker's weapon type, bare hands and monsters picking one of four.
        assert_eq!(hit_sound(None, Some(1), 0), "_hit_sword.wav");
        assert_eq!(hit_sound(None, Some(11), 0), "_hit_arrow.wav");
        assert_eq!(hit_sound(None, Some(8), 0), "_hit_mace.wav");
        assert_eq!(hit_sound(None, Some(0), 6), "_hit_fist3.wav");
        assert_eq!(hit_sound(None, None, 3), "_enemy_hit_normal4.wav");
    }

    #[test]
    fn states_sound_as_they_start_and_break() {
        assert_eq!(state_sounds((0, 0), (6, 0)), ["_stonecurse.wav"]);
        assert_eq!(state_sounds((6, 0), (1, 0)), Vec::<&str>::new(), "stone setting in after the wait");
        assert_eq!(state_sounds((1, 0), (0, 0)), ["_stone_explosion.wav"]);
        assert_eq!(state_sounds((2, 0), (0, 0)), ["_frozen_explosion.wav"]);
        assert_eq!(state_sounds((0, 0x01), (3, 0x13)), ["_stun.wav", "_curse.wav", "_blind.wav"]);
    }

    #[test]
    fn sounds_fall_off_with_distance() {
        assert_eq!([heard(0.0), heard(8.0), heard(16.0), heard(50.0)], [1.0, 1.0, 0.5, 0.0]);
        assert!((heard(30.0) - 40.0 / 150.0).abs() < 1e-6);
    }

    #[test]
    fn skills_reach_by_level_and_world_distance() {
        // Fire Bolt's table: 9 at every level. A list that says 10 at the learned level adds 1 at any level.
        assert_eq!(skill_range(&[9; 10], 3, 10, 10), 10.0);
        assert_eq!(skill_range(&[1, 2, 3], 1, 3, 3), 1.0, "a range that grows by level, used at level 1");
        assert_eq!(skill_range(&[], 5, 5, 0), 1.0, "nothing known: 1");
        assert_eq!(skill_range(&[], 5, 5, 9), 9.0);
        // range x 5 + 5.5 world units: 9 cells reach 10.1 cells away, not 10.2.
        assert!(skill_in_reach([0.0, 0.0], [10.1, 0.0], 9.0) && !skill_in_reach([0.0, 0.0], [10.2, 0.0], 9.0));
        assert!(skill_in_reach([0.0, 0.0], [1.0, 1.0], 1.0));
    }

    #[test]
    fn the_cooldown_fan_clears_clockwise_from_twelve() {
        let dark = |image: &RgbaImage, x: u32, y: u32| image.get_pixel(x, y)[3] == 170;
        // A quarter in: 12 to 3 o'clock is clear, the rest dark.
        let fan = cooldown_fan(0.25);
        assert!(!dark(&fan, 20, 4) && !dark(&fan, 28, 12) && dark(&fan, 28, 20) && dark(&fan, 12, 28) && dark(&fan, 4, 12) && dark(&fan, 12, 4));
        assert!(dark(&cooldown_fan(0.0), 20, 4) && !dark(&cooldown_fan(1.0), 4, 12));
    }

    #[test]
    fn a_press_takes_by_shift_and_skill() {
        let takes = |at, support, shift| aim_takes(at, support, shift, false, false);
        // Enemy skills: monsters without Shift, players with it, never yourself.
        assert!(takes(AimAt::Monster, false, false) && !takes(AimAt::Monster, false, true));
        assert!(takes(AimAt::Player, false, true) && !takes(AimAt::Player, false, false));
        assert!(!takes(AimAt::Me, false, false) && !takes(AimAt::Me, false, true));
        // Support skills the other way round; yourself without Shift.
        assert!(takes(AimAt::Player, true, false) && takes(AimAt::Monster, true, true) && !takes(AimAt::Monster, true, false));
        assert!(takes(AimAt::Me, true, false) && !takes(AimAt::Me, true, true));
        // /noshift takes both whatever Shift does; a PvP map lets players in.
        assert!(aim_takes(AimAt::Monster, false, true, true, false) && aim_takes(AimAt::Player, false, false, true, false));
        assert!(aim_takes(AimAt::Player, false, false, false, true) && !aim_takes(AimAt::Me, false, false, true, true));
    }

    #[test]
    fn aims_where_the_pointer_is() {
        // Ground skills ignore the unit under the pointer; unit skills need one, and a press without lets go.
        assert!(matches!(aim_use(true, Some(9), Some([3, 4])), Some(SkillTarget::Cell([3, 4]))));
        assert!(matches!(aim_use(false, Some(9), Some([3, 4])), Some(SkillTarget::Unit(9))));
        assert!(aim_use(false, None, Some([3, 4])).is_none() && aim_use(true, Some(9), None).is_none());
        // The wheel: up raises, down lowers, 1 to the learned level; not for a skill without a level to pick.
        assert_eq!([aim_level(10, 10, 2, true), aim_level(8, 10, -1, true), aim_level(10, 10, -1, true), aim_level(1, 10, 3, true)], [8, 9, 10, 1]);
        assert_eq!(aim_level(5, 10, 2, false), 5);
    }

    #[test]
    fn talks_and_picks_up_from_close() {
        assert!(may_talk(false, false, false) && may_talk(true, false, true));
        assert!(!may_talk(true, false, false) && !may_talk(false, true, false));
        assert!(within_pickup([5, 5], [6, 6]) && within_pickup([5, 5], [5, 5]));
        assert!(!within_pickup([5, 5], [7, 5]) && !within_pickup([5, 5], [6, 7]));
        assert_eq!([npc_cursor(WARP_JOB), npc_cursor(86)], [cursor::Kind::Warp, cursor::Kind::Talk]);
    }

    #[test]
    fn attacks_as_the_player_asked() {
        // Reach: range × 5 + 5.5 units.
        assert!(in_attack_reach([0, 0], [2, 0], 1));
        assert!(!in_attack_reach([0, 0], [2, 1], 1));
        assert!(in_attack_reach([0, 0], [10, 0], 9) && !in_attack_reach([0, 0], [11, 0], 9));
        let start = Instant::now();
        assert!(may_attack(None, start) && !may_attack(Some(start), start + Duration::from_millis(1199)));
        assert!(may_attack(Some(start), start + ATTACK_GAP));
        // Kept up: letting go changes nothing.
        let mut kept = Attack::new(5, true);
        assert_eq!((kept.release(), kept.action()), (Release::Keep, net::ACT_ATTACK_REPEAT));
        // Held and sent: letting go cancels it.
        let mut held = Attack { sent: true, ..Attack::new(5, false) };
        assert_eq!(held.action(), net::ACT_ATTACK_REPEAT);
        assert_eq!(held.release(), Release::Cancel);
        // Let go before it reached: one swing when it comes.
        let mut early = Attack::new(5, false);
        assert_eq!(early.release(), Release::Keep);
        assert_eq!(early.action(), net::ACT_ATTACK);
        assert_eq!(
            [attack_refused(900, 1000, false), attack_refused(899, 1000, false), attack_refused(0, 1000, true)],
            [Some(OVERWEIGHT), None, Some(RIDING_NO_ATTACK)]
        );
    }

    #[test]
    fn walks_beside_an_item_to_pick_it_up() {
        let open = |_: [i32; 2]| true;
        // From the west, the south-west and on top of it.
        assert_eq!(pickup_spot([10, 10], [14, 10], open), [13, 10]);
        assert_eq!(pickup_spot([10, 6], [14, 10], open), [13, 9]);
        assert_eq!(pickup_spot([14, 10], [14, 10], open), [14, 10]);
        // That cell a wall: the first open neighbour in the fixed order (east is first).
        assert_eq!(pickup_spot([10, 10], [14, 10], |cell| cell != [13, 10]), [15, 10]);
        assert_eq!(pickup_spot([10, 10], [14, 10], |cell| ![[13, 10], [15, 10], [15, 11]].contains(&cell)), [15, 9]);
    }

    #[test]
    fn picks_up_on_the_way() {
        // Four cells away and walking there: nothing until the player's cell is next to the item.
        assert_eq!(pickup_step([10, 10], [14, 10], None), PickStep::Walk);
        assert_eq!(pickup_step([11, 10], [14, 10], Some([14, 10])), PickStep::Wait);
        assert_eq!(pickup_step([12, 11], [14, 10], Some([14, 10])), PickStep::Wait);
        assert_eq!(pickup_step([13, 11], [14, 10], Some([14, 10])), PickStep::PickUp);
        assert_eq!(pickup_step([14, 10], [14, 10], None), PickStep::PickUp);
        // A walk the move limit held back is asked for again on a later frame, once.
        let start = Instant::now();
        let (mut walked, mut last) = (None, Some(start));
        let mut walks = Vec::new();
        for ms in [100, 200, 299, 300, 350, 700] {
            let now = start + Duration::from_millis(ms);
            if pickup_step([10, 10], [14, 10], walked) == PickStep::Walk && may_move(last, now) {
                (walked, last) = (Some([14, 10]), Some(now));
                walks.push(ms);
            }
        }
        assert_eq!(walks, [300]);
        // The held button follows the pointer only once nothing is pending, MOVE_GAP after its last try.
        assert!(!follows_pointer(true, true, None, start));
        assert!(follows_pointer(true, false, None, start));
        assert!(!follows_pointer(true, false, Some(start), start + Duration::from_millis(300)));
        assert!(follows_pointer(true, false, Some(start), start + Duration::from_millis(301)));
        assert!(!follows_pointer(false, false, None, start));
    }

    #[test]
    fn walks_no_more_than_every_300_ms() {
        let start = Instant::now();
        let at = |ms: u64| start + Duration::from_millis(ms);
        assert!(may_move(None, start));
        assert!(!may_move(Some(start), at(299)));
        assert!(may_move(Some(start), at(300)));
        // A second press: double only when soon and the pointer stayed still.
        assert!(double_press(Some((start, [0.0; 2], false)), at(400)));
        assert!(!double_press(Some((start, [0.0; 2], true)), at(400)));
        assert!(!double_press(Some((start, [0.0; 2], false)), at(500)));
        // A tremor isn't a move; two pixels are.
        assert!(!moved_off([100.0, 100.0], [101.4, 99.2]));
        assert!(moved_off([100.0, 100.0], [100.0, 102.0]));
        assert!(!double_press(None, start));
        // Four presses in place 200 ms apart: fresh, double, fresh, double.
        let mut mark = None;
        let doubles: Vec<bool> = (0..4)
            .map(|n| {
                let (double, next) = press_pair(mark, at(200 * n), [5.0, 5.0]);
                mark = next;
                double
            })
            .collect();
        assert_eq!(doubles, [false, true, false, true]);
        // A move off the press between two presses makes both fresh.
        let (_, first) = press_pair(None, start, [5.0, 5.0]);
        assert!(!press_pair(first.map(|(at, from, _)| (at, from, true)), at(200), [9.0, 5.0]).0);
        // Held after a double press on the ground, the button follows the pointer; not with Ctrl, Shift or sitting.
        assert!(
            double_holds(false, false, false) && !double_holds(true, false, false) && !double_holds(false, true, false) && !double_holds(false, false, true)
        );
    }

    #[test]
    fn coins_for_the_gunslinger_line() {
        assert!([24, 4215, 4228, 4229, 4306].into_iter().all(coin_job));
        assert!(![15, 4016, 0].into_iter().any(coin_job));
    }

    #[test]
    fn breaks_message_lines_where_the_client_padded() {
        let font = Font::load();
        let question = "한 번 삭제된 캐릭터 및 관련 정보는      다시 복구되지 않습니다.                                 캐릭터를 삭제하시겠습니까?";
        assert_eq!(
            message_lines(&font, question, MESSAGE_WIDTH),
            ["한 번 삭제된 캐릭터 및 관련 정보는", "다시 복구되지 않습니다.", "캐릭터를 삭제하시겠습니까?"]
        );
    }

    #[test]
    fn keeps_wrapped_links_clickable() {
        let font = Font::load();
        let apple = links::Target::Item(links::ItemLink { id: 512, ..links::ItemLink::default() });
        let text = format!("^0000FFTake {} home^000000", links::marked("Big Red Apple", &apple));
        // Narrow enough that the link breaks: both of its lines are the link, in its colour; the colour before it
        // comes back after it.
        let width = font.width("^Take Big Red");
        let lines = wrap_linked(&font, &text, width);
        let linked: Vec<(usize, String)> = lines
            .iter()
            .enumerate()
            .flat_map(|(row, pieces)| pieces.iter().filter(|(.., item)| item.as_ref() == Some(&apple)).map(move |(_, piece, _)| (row, piece.trim().to_owned())))
            .filter(|(_, piece)| !piece.is_empty())
            .collect();
        assert!(linked.iter().any(|(row, _)| *row == 0) && linked.iter().any(|(row, _)| *row > 0), "{lines:?}");
        assert!(lines.iter().flatten().all(|(color, _, item)| item.is_none() || *color == links::COLOR));
        assert_eq!(lines.last().unwrap().last().map(|(color, piece, _)| (*color, piece.trim())), Some(([0, 0, 0xFF], "home")));
        // The click test over a link wrapped onto two lines: either box finds the item, the gap beside them nothing.
        let boxes = [([10.0, 20.0, 40.0, 14.0], apple.clone()), ([0.0, 34.0, 25.0, 14.0], apple.clone())];
        let at = |cursor: [f32; 2]| links::at(&boxes, cursor).and_then(links::Target::item).map(|item| item.id);
        assert_eq!([at([30.0, 25.0]), at([5.0, 40.0]), at([30.0, 40.0]), at([50.0, 25.0])], [Some(512), Some(512), None, None]);
    }

    #[test]
    fn shows_npc_links_by_name() {
        let navi = "Let me mark the ^B9062F<NAVI>[Tool Shop]<INFO>prontera,134,221,0,000,0</INFO></NAVI>^000000 for you.";
        // The place is a link to navigate to; the colour codes around it stay for the wrapping to read.
        let (shown, runs) = links::split(&show_links(&links::mark(navi, &|_| String::new())));
        assert_eq!(shown, "Let me mark the ^B9062F[Tool Shop]^000000 for you.");
        assert_eq!(runs.iter().map(|(_, target)| target.clone()).collect::<Vec<_>>(), [links::Target::Navi { map: "prontera".into(), cell: [134, 221] }]);
        let marked = show_links(&links::mark("a <ITEM>Apple<INFO>512</INFO></ITEM> < b", &|_| String::new()));
        assert_eq!(links::split(&marked).0, "a Apple < b");
    }

    #[test]
    fn turns_the_camera_only_while_the_right_button_is_down() {
        let mut goal = CameraGoal::new(false);
        let none = ModifiersState::empty();
        goal.drag([100.0, 100.0], none);
        goal.drag([200.0, 100.0], none);
        assert_eq!((goal.yaw, goal.dragging), (0.0, None));
        goal.press([200.0, 100.0], Instant::now(), false, false);
        goal.drag([300.0, 100.0], none);
        assert!(goal.yaw > 0.0);
    }
}
