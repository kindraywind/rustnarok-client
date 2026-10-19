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
mod ambient;
mod announce;
mod aura;
mod bank;
mod basic_info;
mod battle_mode;
mod battle_queue;
mod battleground;
mod booking_window;
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
mod cutin;
mod draw;
mod dressroom;
mod effects;
mod emotion;
mod emotion_list;
mod enchant;
mod equipment;
mod escape;
mod events;
mod game;
mod grade;
mod guild;
mod head_marks;
mod homunculus;
mod hotkeys;
mod input;
mod inventory;
mod iteminfo;
mod laphine;
mod level_up;
mod line_edit;
mod links;
mod macros;
mod mail;
mod material;
mod memorial;
mod menu;
mod mercenary;
mod minimap;
mod minimap_window;
mod monster_info;
mod navigation;
mod npc_input;
mod npc_menu;
mod npc_say;
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
use rustnarok_client::booking;
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
/// /aura: "Simplified Aura [OFF]" / "[ON]"; /aura2: "Aura effect is ON" / "OFF"; /aura while /aura2 has it off.
const SIMPLIFIED_AURA_OFF: usize = 711;
const SIMPLIFIED_AURA_ON: usize = 712;
const AURA_EFFECT_ON: usize = 2994;
const AURA_EFFECT_OFF: usize = 2995;
const AURA_OFF_FIRST: usize = 2996;
const EFFECTS_OFF: usize = 24;
/// /quake: "Screen shake ON" / "Screen shake OFF".
// ponytail: /quake's chat lines use the message table's 2805 and 2806.
const QUAKE_ON: usize = 2805;
const QUAKE_OFF: usize = 2806;
/// Deals: "requests a deal", no one of that name, "can't overlap items", "no more than 10 kinds", how asking went
/// (70 + the result), how it ended (75 done, 76 failed, 74 cancelled), too heavy.
/// "Invalid Command", in yellow.
const INVALID_COMMAND: usize = 95;
const TRADE_ASKS: usize = 93;
const TRADE_NO_ONE: usize = 71;
const TRADE_DONE: usize = 75;
const TRADE_FAILED: usize = 76;
const TRADE_CANCELLED: usize = 74;
const TRADE_HEAVY: usize = 73;
/// "Request a deal with %s".
/// "Request a deal with (%s)".
const TRADE_CHOICE: usize = 391;
/// "Trade : ", the deal window's caption.
const TRADE_CAPTION: usize = 206;
/// "Please finish the task in progress (NPC conversation, ...) and try again.", in white.
const TASK_IN_PROGRESS: usize = 1923;
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
/// Our pet's menu: Check Pet Status, Feed Pet, Performance, Unequip Accessory, Return to Egg; all five always.
const PET_CHOICES: [(usize, u8); 5] = [(596, net::PET_INFO), (592, net::PET_FEED), (593, net::PET_PERFORM), (595, net::PET_UNEQUIP), (594, net::PET_RETURN)];
/// A player's menu: "Open 1:1 Chat", "Send a mail", "Block Chat (Toggle)", "Kick %s".
const WHISPER_CHOICE: usize = 360;
const MAIL_CHOICE: usize = 2808;
const BLOCK_CHOICE: usize = 3820;
const KICK_CHOICE: usize = 458;
/// Our homunculus's menu: "View Status", "Feed", "Stand By".
const HOMUNCULUS_CHOICES: [(usize, Choice); 3] = [(1112, Choice::HomunculusStatus), (1113, Choice::HomunculusFeed), (1114, Choice::HomunculusStandBy)];
/// Ourselves with a stall open: "Close stall".
const CLOSE_STALL_CHOICE: usize = 3493;
const PET_FEED_ASKS: usize = 601;
const PET_RENAME_ASKS: usize = 2931;
const PET_NO_FOOD: usize = 591;
/// "Your pet has been fed. %s (%d) remaining", and the lines' colours.
const PET_FED: usize = 2578;
const PET_FED_COLOR: [u8; 3] = [0x64, 0xFF, 0xFF];
const PET_NO_FOOD_COLOR: [u8; 3] = [0xF5, 0x1E, 0x1E];
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
/// "Are sure want to assign %s as guild leader? ..." with the member's name twice.
const GUILD_LEADER_ASKS: usize = 2924;
/// The master's menu on an ally or antagonist: "Delete".
const RELATION_DELETE: usize = 351;
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
/// Parties: "Ask (%s) to join your party", made, " Party has sent you an invitation...", "You left the party",
/// party chat colors (ours, others'), the prompts' titles.
const PARTY_ASK: usize = 392;
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

/// What a unit coming into view shows. Only a spawn in view (ZC_NOTIFY_NEWENTRY) of a unit the client doesn't have,
/// and not our own character: a player gets the blue rings unless a GM hides it; anything else fades in.
#[derive(Debug, PartialEq)]
enum Arrival {
    Fade,
    Rings,
    Plain,
}

fn arrival(spawned: bool, known: bool, own: bool, kind: u8, option: u32) -> Arrival {
    match () {
        _ if !spawned || known || own => Arrival::Plain,
        _ if kind == PLAYER && option & GM_HIDE != 0 => Arrival::Plain,
        _ if kind == PLAYER => Arrival::Rings,
        _ => Arrival::Fade,
    }
}

/// A GM hidden from view (rathena's OPTION_INVISIBLE).
const GM_HIDE: u32 = 0x40;
/// The blue rings and the portal sound of a player arriving (EF_ENTRY2).
const EF_ENTRY2: u32 = 344;
/// Where a unit that comes back while fading out leaves its fading copy: under its ID with the top bit set (no ID the
/// server gives has it).
const LEFT_BEHIND: u32 = 0x8000_0000;

/// A unit fading out (or a corpse) whose ID comes back: its copy moves aside to fade out where it was.
fn leave_behind(units: &mut HashMap<u32, Actor>, id: u32) {
    if units.get(&id).is_some_and(|actor| actor.remove_at.is_some())
        && let Some(old) = units.remove(&id)
    {
        units.insert(id | LEFT_BEHIND, old);
    }
}

/// A stand entry's state byte, for players only: 1 dead (lying as after a death in view, falling once), 2 sitting,
/// anything else standing (a dead or sitting player stands up).
fn take_posture(actor: &mut Actor, posture: Option<u8>, now: Instant) {
    let Some(posture) = posture.filter(|_| actor.kind == PLAYER) else { return };
    match posture {
        1 if !actor.dead => fall(actor, now),
        1 => {}
        _ => {
            if actor.dead {
                actor.revive();
            }
            actor.sitting = posture == 2;
        }
    }
}

/// Monsters whose sprites face the other way round: turned (dir + 4) on the entry that brings them.
fn turned_round(job: u16) -> bool {
    matches!(job, 21064..=21088 | 22551..=22668)
}

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
/// "Party leader is [%s].", the setup lines' and the new leader's colour, and the invitation lines'.
const PARTY_NEW_LEADER: usize = 1552;
const PARTY_SETUP_LINE: [u8; 3] = [0xFF, 0xFF, 0x00];
const PARTY_NOTICE: [u8; 3] = [0xFF, 0xFF, 0x64];
/// "Only the Party Leader can change this setting." in the chat.
const PARTY_LEADER_ONLY_LINE: [u8; 3] = [0x64, 0xFF, 0xFF];
/// "Can't re-assign party leader", for an offline member.
const LEADER_OFFLINE: usize = 1534;
const PARTY_LEAVE_ASKS: usize = 357;
const PARTY_EXPEL_ASKS: usize = 363;
/// "Item cannot be dropped from the hotkey bar."
const HOTKEY_ITEM_STAYS: usize = 1696;
/// Dropping on the ground with the equipment window open, the mail window open, or the ammunition worn.
const DROP_EQUIPMENT_OPEN: usize = 189;
const DROP_MAIL_OPEN: usize = 2736;
const DROP_AMMO_WORN: usize = 657;
/// Nothing is dropped on the ground this soon after asking to wear something.
const WEAR_PAUSE: Duration = Duration::from_secs(2);
const LEADER_QUESTION: usize = 1533;
/// What a locked chat room asks ("Please enter Password.").
const ROOM_PASSWORD: usize = 15;
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
/// The login window (bg_login.tga) and its parts, window-local: the account drop-down, the ID and password boxes, the
/// Save ID box and its label, Opening and Sign Up under them, the round Login button, the close X.
const LOGIN_DROP: ([f32; 2], [f32; 2]) = ([13.0, 10.0], [160.0, 22.0]);
const LOGIN_FIELDS: [[f32; 2]; 2] = [[15.0, 40.0], [15.0, 64.0]];
const LOGIN_FIELD_SIZE: [f32; 2] = [153.0, 16.0];
const LOGIN_SAVE: ([f32; 2], [f32; 2]) = ([56.0, 85.0], [12.0, 12.0]);
const LOGIN_SAVE_LABEL: ([f32; 2], [f32; 2]) = ([73.0, 87.0], [72.0, 22.0]);
const LOGIN_OPENING: ([f32; 2], [f32; 2]) = ([13.0, 104.0], [67.0, 21.0]);
const LOGIN_SIGN_UP: ([f32; 2], [f32; 2]) = ([87.0, 104.0], [84.0, 21.0]);
const LOGIN_START: [f32; 2] = [192.0, 32.0];
const LOGIN_START_SIZE: [f32; 2] = [84.0, 84.0];
const LOGIN_EXIT: ([f32; 2], [f32; 2]) = ([273.0, 15.0], [19.0, 19.0]);
/// The drop-down's two accounts, the Save ID label, Opening and Sign Up, and their colours.
const LOGIN_ACCOUNTS: [usize; 2] = [3250, 3251];
const SAVE_ID_LABEL: usize = 3411;
const OPENING_LABEL: usize = 3431;
const SIGN_UP_LABEL: usize = 3414;
const SAVE_ID_COLOR: [u8; 3] = [0x33, 0x33, 0x33];
const LOGIN_BUTTON_COLOR: [u8; 3] = [0x31, 0x31, 0x31];
/// An ID under 4 bytes and a password under 6 aren't sent: "%d or more" lines.
const ID_TOO_SHORT: (usize, usize) = (3029, 4);
const PASSWORD_TOO_SHORT: (usize, usize) = (3030, 6);
const BUTTON_SOUND: &str = "버튼소리.wav";
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
/// What I Will Protect You says after the partner's name.
const PROTECT_WORDS: usize = 759;
/// "Do you want to return to the login screen?" (Esc at character select).
const RETURN_TO_LOGIN_MESSAGE: usize = 3432;
const DELETE_QUESTION: usize = 19;
const BIRTHDATE_MESSAGE: usize = 1815;
/// Talking to another NPC while one's window is open.
const TALK_REFUSED: usize = 4027;

/// NPC dialogs are drawn as shapes, like the client draws them.
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
/// Heals that show their amount and an effect by it: Heal, Lord Heal, the NPC heal, Coluceo Heal, Highness Heal and the
/// homunculus's (not Epiclesis).
const HEAL_SKILLS: [u16; 6] = [28, 334, 729, 2043, 2051, 8001];
/// SP Recovery's amount shows blue.
const SP_RECOVERY: u16 = 9;
/// The look a trap takes when it goes off.
const FIRED_TRAP: u32 = 0x8c;
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
/// EFST_UNKNOWN_NAME: the unit's name shows as ?????? on PvP and siege maps.
const UNKNOWN_NAME: u16 = 672;
const HIDDEN_NAME: &str = "??????";
/// The spark of a landed hit, and a warp portal's rings.
const EF_HIT1: u32 = 0;
/// A critical's second spark, and the spark of the Taekwon line's blows.
const EF_HIT2: u32 = 1;
const TAEKWON_HIT: u32 = 434;
/// The Taekwon line: Taekwon, Star Gladiator (and its union), Star Emperor (and its union).
const TAEKWON_LINE: [u16; 5] = [4046, 4047, 4048, 4239, 4240];

/// The sparks a landed normal attack shows on its target: none for a miss or a lucky dodge; EF_HIT1 (434 from the
/// Taekwon line), and EF_HIT2 too on a critical.
fn attack_sparks(damage: i32, kind: u8, taekwon: bool) -> Vec<u32> {
    if damage <= 0 || !matches!(kind, 0 | 4 | 8 | 9 | 10 | 13) {
        return Vec::new();
    }
    let first = if taekwon { TAEKWON_HIT } else { EF_HIT1 };
    if matches!(kind, 10 | 13) { vec![first, EF_HIT2] } else { vec![first] }
}

/// A heal's effect by its amount.
fn heal_effect(amount: i32) -> u32 {
    match amount {
        4000.. => 325,
        2000.. => 313,
        200.. => 312,
        _ => 7,
    }
}
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

/// The character after a warp: standing on `position`'s centre, facing the way it did on entering the map server.
fn after_warp(player: &mut Actor, position: [u16; 2], entered_facing: u8) {
    player.stop_at(position.map(i32::from));
    player.sitting = false;
    player.set_direction(entered_facing);
}

/// The effect a unit leaves where it vanished, by rathena's `clr_type`: 3 teleported (EF_TELEPORTATION2, its violet
/// rings) unless a GM hidden from sight (OPTION_INVISIBLE), 2 logged out (EF_EXIT2, purple rings, hidden GMs too);
/// nothing for going out of sight or dying.
// ponytail: the 300 and 600 the game passes with these aren't used.
fn vanish_effect(reason: u8, option: u32) -> Option<u32> {
    match reason {
        3 if option & actor::OPTION_INVISIBLE == 0 => Some(304),
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
/// Where an attack's target is: a unit still there, or a skill's unit on the ground; none ends the attack.
fn attack_goal(units: &HashMap<u32, Actor>, skill_units: &HashMap<u32, (u32, [u16; 2])>, target: u32) -> Option<[i32; 2]> {
    units
        .get(&target)
        .filter(|actor| actor.remove_at.is_none())
        .map(|actor| actor.cell)
        .or_else(|| skill_units.get(&target).map(|(_, cell)| cell.map(i32::from)))
}

/// Skill units on the ground a plain attack breaks (rathena's `battle_check_target` takes them): Ice Wall, Blast Mine,
/// Claymore Trap, Reverberation, Poem of Netherworld and the four Windhawk traps.
fn attackable_ground_unit(kind: u32) -> bool {
    matches!(kind, 0x8d | 0x8f | 0x98 | 0xda | 0xde | 0x5174..=0x5177)
}

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
    /// The way the character faced on entering the map server: it faces so again after every warp.
    entered_facing: u8,
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
    /// The NPC say window's place, scroll and button throttle.
    say: npc_say::SayWindow,
    /// Where the NPC menu is.
    npc_menu: window::Frame,
    /// Last frame's clickable units and items.
    hit_boxes: Vec<picking::Target<Owner>>,
    /// Skills' units on the ground in view: their unit kind and cell.
    skill_units: HashMap<u32, (u32, [u16; 2])>,
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
    /// The last web page an NPC's link opened, under the control socket (which doesn't open it).
    opened_page: Option<String>,
    /// The player's progress bar (a script's `progressbar`).
    progress: ProgressBar,
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
    cutin: Option<cutin::Cutin>,
    /// Compass marks on the minimap, by mark id: the cell, its color, until when (None: until taken away) and when set.
    marks: Vec<(u8, Mark)>,
    /// NPCs' quest marks (ZC_QUEST_NOTIFY_EFFECT): the NPC, its cell, the effect and the mark's colour.
    quest_marks: Vec<(u32, [i32; 2], u16, u16)>,
    /// When each NPC's quest mark over its head started looping: made again by a new effect or colour, or the NPC
    /// coming back into view.
    quest_started: HashMap<u32, Instant>,
    /// Where the store picked in the Universal Catalog stands.
    store_mark: Option<[i32; 2]>,
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
    /// A card's own window, opened from a description's strip, where it opens, and its text's lines.
    card_window: Option<iteminfo::ItemWindow>,
    card_place: [f32; 2],
    card_lines: usize,
    item_lines: usize,
    /// Where the item description window opens: where it was last (saved), else its first place.
    item_place: [f32; 2],
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
    /// The party booking board, its web server (address and who we are there), the answers coming back from the
    /// requests sent to it, and someone asking to join through our post (character, account, name, level, job).
    booking: Option<booking_window::BookingWindow>,
    booking_web: (String, booking::Auth),
    booking_answers: (std::sync::mpsc::Sender<BookingAnswer>, std::sync::mpsc::Receiver<BookingAnswer>),
    booking_asked: Option<(u32, u32, String, u16, u16)>,
    /// The NPC shop windows' sizes (goods width, goods and deal rows): kept, and given to the next shop.
    shop_sizes: (f32, [usize; 2]),
    trade_with: String,
    /// /notrade: deal requests are refused at once.
    no_trade: bool,
    /// Dev switch `RUSTNAROK_CLIENT_TRADE_GIVE`: the items still to put in, one per answer, then the zeny to lock with.
    trade_give: VecDeque<(u16, u32)>,
    trade_give_zeny: Option<u32>,
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
    /// The vending windows' size, kept for the next ones.
    vend_size: vend::GridSize,
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
    /// The reason box for leaving the guild or expelling a member.
    reason_box: Option<guild::ReasonBox>,
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
    /// /loginout: the guild notice and members' log-in lines (on unless turned off; saved). /notalkmsg2: guild chat
    /// hidden (saved).
    login_notices: bool,
    no_guild_talk: bool,
    /// /emblem: emblems next to hovered players' names (saved; on unless turned off).
    show_emblems: bool,
    /// Units under the unknown-name status (672), whose names hide on PvP and siege maps.
    unknown_names: HashSet<u32>,
    /// The hovered player's name block as drawn: its lines and the emblem's box (for the test driver).
    hover_block: Option<(Vec<String>, Option<[f32; 4]>)>,
    /// /noctrl: a click on a monster keeps attacking without Ctrl (saved; on unless turned off).
    no_ctrl: bool,
    /// /snap, /itemsnap, /skillsnap (saved; only /skillsnap on unless changed).
    snaps: Snaps,
    /// Where the cursor is drawn: the pointer, or the middle of what it snapped to.
    cursor_at: [f32; 2],
    /// /camera: the camera sticks to the player (saved; off unless turned on).
    camera_snap: bool,
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
    /// The memorial dungeon, and whether Destroy is asking yes or no.
    memorial: memorial::Window,
    memorial_asked: bool,
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
    party_dialog: Option<party::PartyDialog>,
    leader_asked: Option<u32>,
    /// A question or notice in the message box: leaving, disbanding, expelling a member, "only the leader can invite", a
    /// plain notice.
    question_box: Option<BoxQuestion>,
    /// The last press on a party member's row (when, whose): a second soon after on the same row is a double click.
    row_press: Option<(Instant, u32)>,
    /// The small party windows as last saved for this character.
    small_saved: String,
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
    right_press: Option<([f32; 2], Option<MenuFor>)>,
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
    /// A camera shake is running (when it stops, the look-at point, distance and pitch snap straight back).
    shaking: bool,
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
            shaking: false,
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

    /// Shakes the camera (`[sideways, distance, pitch_degrees]`), or snaps the look-at point, distance and pitch
    /// straight back when the shake ends.
    fn shake(&mut self, camera: &mut Camera, feet: world::Vector, quake: Option<[f32; 3]>) {
        if let Some([side, distance, pitch]) = quake {
            self.shaking = true;
            camera.target[0] += camera.yaw.cos() * side;
            camera.target[2] -= camera.yaw.sin() * side;
            camera.distance += distance;
            camera.pitch += pitch.to_radians();
        } else if std::mem::replace(&mut self.shaking, false) {
            camera.target = feet;
            camera.distance = self.distance();
            camera.pitch = self.pitch.to_radians();
        }
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
    /// Its shadow's size (a body's is 1; none for the falcon).
    shadow: f32,
}

/// A falcon or warg to draw: its direction rounds the camera's angle, as a standing player's does.
fn push_companion(companion: &mut actor::Companion, warg: bool, owner: Owner, gpu: &Gpu, world: &World, yaw: f32, now: Instant, out: &mut Vec<CompanionDraw>) {
    let lift = if warg { 0.0 } else { actor::FALCON_HEIGHT };
    let placed = companion.frame(gpu, seen_direction(companion.rot, yaw, true), now);
    if placed.is_empty() {
        return;
    }
    out.push(CompanionDraw { owner, warg, feet: companion_feet(world, companion.at, lift), placed, shadow: companion.shadow });
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

/// Whether a unit is drawn as a player: a player, or any unit with a player's job (0–30, 4001–5999), as an `@clone`
/// or a script NPC with a player class is. It stays what it is for clicks, names, the cursor and the HP bar.
fn drawn_as_player(kind: u8, job: u16) -> bool {
    kind == PLAYER || matches!(job, 0..=30 | 4001..=5999)
}

/// The look an entry brings for a unit already there, when it changes it: every move entry's, and a stand or new
/// entry's only for a player (another unit keeps its job on those).
fn refreshed_look(actor: &Actor, unit: &net::Unit, move_entry: bool) -> Option<Look> {
    let look = unit_look(unit);
    (drawn_as_player(unit.kind, unit.job) && (move_entry || unit.kind == PLAYER) && actor.look.as_ref() != Some(&look)).then_some(look)
}

/// A unit not drawn as a player taking another job (ZC_SPRITE_CHANGE type 0, ZC_NPCSPRITE_CHANGE, a move entry):
/// whether its sprites need loading again.
fn take_job(actor: &mut Actor, job: u16) -> bool {
    let changed = actor.job != job;
    actor.job = job;
    changed
}

/// A unit taking another job at once (ZC_SPRITE_CHANGE type 0, ZC_NPCSPRITE_CHANGE): drawn as a player with it, or
/// with the job's own sprite.
fn change_job(actor: &mut Actor, job: u16, grf: &Grf, job_names: &HashMap<u16, String>, look_tables: &LookTables) {
    if drawn_as_player(actor.kind, job) {
        let look = Look { job, ..actor.look.clone().unwrap_or_default() };
        if actor.look.as_ref() != Some(&look) {
            actor.sprites = SpriteSet::player(grf, &look, look_tables);
            actor.look = Some(look);
        }
        actor.job = job;
    } else if take_job(actor, job) {
        actor.look = None;
        actor.sprites = job_sprites(grf, job_names, actor.kind, job);
    }
}

/// A unit's sprites for a job it isn't drawn as a player with: none for hidden jobs and warps.
fn job_sprites(grf: &Grf, job_names: &HashMap<u16, String>, kind: u8, job: u16) -> Option<SpriteSet> {
    if HIDDEN_JOBS.contains(&job) || job == WARP_JOB {
        return None;
    }
    job_names.get(&job).and_then(|name| SpriteSet::single(grf, name, kind))
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
type Mark = ([i32; 2], Texture, Option<Instant>, Instant);

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
        Panel::Cutin => return None,
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
    /// An NPC's picture.
    Cutin,
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
    /// The password of a chat room, by ID.
    RoomPassword(u32),
    /// An inventory item, into the mail being written.
    Mail(u16),
    /// An inventory item, into the material window.
    Material(u16),
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
    /// A party member's menu entry, for this member (by account).
    Member(party::MemberChoice, u32),
    /// The room's owner hands the room to this member, or kicks them out (by their row in the member list).
    RoomOwner(usize),
    RoomKick(usize),
    /// A tracked quest: its details in the quest window, or put it aside.
    QuestInfo(u32),
    QuestAside(u32),
    /// Ask this party member (by account) to be adopted.
    Adopt(u32),
    /// The member menu on the guild's members page: an entry, for a member (by place in the member list).
    GuildMember(guild::MemberChoice, usize),
    /// A guild skill's description.
    GuildSkillInfo(u16),
    /// The master ends the relation with this guild (antagonist or not), without asking.
    EndRelation(u32, bool),
    /// Whisper this player: their name in the chat's name box.
    Whisper(u32),
    /// Take off the cart or the mount (CZ_UNINSTALLATION's kind).
    Uninstall(u8),
    /// Write them a mail.
    Mail(u32),
    /// Block their whispers.
    Block(u32),
    /// Our homunculus: its window, feeding it, or it staying put rather than following.
    HomunculusStatus,
    HomunculusFeed,
    HomunculusStandBy,
    /// Close our stall.
    CloseStall,
}

/// Whom a still right-click opens the menu for.
#[derive(Debug, PartialEq, Clone, Copy)]
enum MenuFor {
    Player(u32),
    Pet(u32),
    Homunculus(u32),
    /// Ourselves, while our stall is open.
    Stall,
}

/// The menu a still right-click opens for what's under the pointer (`picked`, by the usual ranks with Shift's aside):
/// another player, our pet or homunculus, or ourselves while our stall is open; nothing for anything else.
fn menu_for(picked: Option<Owner>, is_player: impl Fn(u32) -> bool, pet: Option<u32>, homunculus: Option<u32>, stall: bool) -> Option<MenuFor> {
    match picked? {
        Owner::Unit(id) if pet == Some(id) => Some(MenuFor::Pet(id)),
        Owner::Unit(id) if homunculus == Some(id) => Some(MenuFor::Homunculus(id)),
        Owner::Unit(id) if is_player(id) => Some(MenuFor::Player(id)),
        Owner::Player if stall => Some(MenuFor::Stall),
        _ => None,
    }
}

/// What we can do with another player, from where we stand.
#[derive(Default, Clone, Copy)]
struct PlayerTies {
    trading: bool,
    /// We lead a party they aren't in.
    party_invite: bool,
    /// We're a guild master: they aren't in our guild; they're in another guild.
    guild_invite: bool,
    guild_relation: bool,
    friend: bool,
    adoptable: bool,
    /// Their row in the chat room we own.
    in_our_room: Option<usize>,
}

/// A player's menu entries in the game's order, each when it applies: the message and the choice.
// ponytail: no separator lines; good/bad points, the chat-block record and removing all equipment are left out.
fn player_choices(id: u32, ties: PlayerTies) -> Vec<(usize, Choice)> {
    let mut choices = Vec::new();
    if !ties.trading {
        choices.push((TRADE_CHOICE, Choice::Trade(id)));
    }
    if ties.party_invite {
        choices.push((PARTY_ASK, Choice::Invite(id)));
    }
    if ties.guild_invite {
        choices.push((GUILD_CHOICE, Choice::GuildInvite(id)));
    }
    if ties.guild_relation {
        choices.extend([(ALLY_CHOICE, Choice::Ally(id)), (ANTAGONIST_CHOICE, Choice::Antagonist(id))]);
    }
    choices.push((WHISPER_CHOICE, Choice::Whisper(id)));
    if !ties.friend {
        choices.push((FRIEND_CHOICE, Choice::Befriend(id)));
    }
    choices.extend([(MAIL_CHOICE, Choice::Mail(id)), (VIEW_CHOICE, Choice::ViewEquipment(id))]);
    if ties.adoptable {
        choices.push((ADOPT_CHOICE, Choice::Adopt(id)));
    }
    choices.push((BLOCK_CHOICE, Choice::Block(id)));
    if let Some(row) = ties.in_our_room {
        choices.push((KICK_CHOICE, Choice::RoomKick(row)));
    }
    choices
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
    /// A new name turned down: the line why (either button closes it).
    // ponytail: drawn as the question box; the game's has OK alone.
    Refused(usize),
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
    /// A skill's unit on the ground that can be attacked (`attackable_ground_unit`).
    SkillUnit(u32),
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

#[derive(PartialEq)]
enum DialogButton {
    None,
    Next,
    Close,
}

/// A guild question, answered with OK or Cancel.
/// The message box's buttons and whether each answers yes: OK and Cancel, or OK alone where Cancel would be.
fn box_buttons(asks: bool) -> Vec<([f32; 2], bool)> {
    if asks { vec![(LEFT_BUTTON, true), (RIGHT_BUTTON, false)] } else { vec![(RIGHT_BUTTON, true)] }
}

/// A question or notice in the message box.
#[derive(Debug, Clone, PartialEq)]
enum BoxQuestion {
    /// "Are you sure that you want to leave?" (357).
    Leave,
    /// Nobody to hand the party to: disband instead? (3257).
    Disband,
    /// "Are you sure that you want to expel this player?" (363), for this member (account, name).
    Expel(u32, String),
    /// "Only the leader can invite." (2070), OK only.
    OnlyLeader,
    /// "In the last row" (3145): a small party window let go outside the used cells. OK only.
    LastRow,
    /// A plain notice of this message, OK only (the big minimap refused on this map).
    Notice(usize),
    /// The master hands the guild to this member (2924, both %s their name).
    GuildLeader { account: u32, character: u32, name: String },
}

impl BoxQuestion {
    fn message(&self) -> usize {
        match self {
            Self::Leave => PARTY_LEAVE_ASKS,
            Self::Disband => party::ASK_DISBAND,
            Self::Expel(..) => PARTY_EXPEL_ASKS,
            Self::OnlyLeader => party::ONLY_LEADER_INVITES,
            Self::LastRow => party::IN_THE_LAST_ROW,
            Self::Notice(message) => *message,
            Self::GuildLeader { .. } => GUILD_LEADER_ASKS,
        }
    }

    /// Its words: the message, with the member's name in the leader question.
    fn text(&self, messages: &[String]) -> String {
        let template = messages.get(self.message()).map_or("", String::as_str);
        match self {
            Self::GuildLeader { name, .. } => rustnarok_client::messages::fill(template, &[name, name]),
            _ => template.to_owned(),
        }
    }

    /// Whether it asks (OK and Cancel), else it tells (OK alone).
    fn asks(&self) -> bool {
        !matches!(self, Self::OnlyLeader | Self::LastRow | Self::Notice(_))
    }
}

enum GuildAsked {
    /// Another guild (its name) asks for an alliance, through the player who asked (account ID).
    Ally { id: u32, guild: String },
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

/// The talking state an NPC packet sets, for that NPC: a page, Next or a menu holds the player; the script's close
/// frees them. Other packets (the input boxes among them) leave it.
fn npc_holds(event: &MapEvent) -> Option<(u32, bool)> {
    match *event {
        MapEvent::NpcSay { npc, .. } | MapEvent::NpcNext { npc } | MapEvent::NpcMenu { npc, .. } => Some((npc, true)),
        MapEvent::NpcClose { npc } => Some((npc, false)),
        _ => None,
    }
}

/// Talking to an NPC holds the player (no walking, attacking, picking up, other NPCs or ground drops), except while
/// their cast bar is up.
fn talk_holds(talking: bool, cast_bar: bool) -> bool {
    talking && !cast_bar
}

/// When the player's progress bar runs out.
#[derive(Default)]
struct ProgressBar(Option<Instant>);

impl ProgressBar {
    fn start(&mut self, now: Instant, length: Duration) {
        self.0 = Some(now + length);
    }

    /// Whether the bar ends now, its time up or `cancelled`: true once.
    fn ends(&mut self, now: Instant, cancelled: bool) -> bool {
        let ended = self.0.is_some_and(|end| cancelled || now >= end);
        if ended {
            self.0 = None;
        }
        ended
    }
}

struct Dialog {
    npc: u32,
    /// Set by a page, Next or a menu, ended by the script's close: the player is held while talking.
    talking: bool,
    lines: Vec<String>,
    /// Next was pressed: the text stays until the next page wipes it.
    wipe: bool,
    button: DialogButton,
    menu: Option<npc_menu::Menu>,
    /// Waiting for a number or text, in its own box.
    input: Option<npc_input::Input>,
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

/// Esc on a screen before a map: the login window, the service list and the server list ask to quit; character
/// select asks to go back to the login screen.
// ponytail: the game picks 3432 or 17 at character select by a setting we don't have; 3432 is what Esc does there.
fn pre_map_escape(scene: &Scene) -> Option<(usize, Then)> {
    match scene {
        Scene::Login | Scene::Connections { .. } | Scene::ServerSelect { .. } => Some((QUIT_MESSAGE, Then::Quit)),
        Scene::CharSelect(_) => Some((RETURN_TO_LOGIN_MESSAGE, Then::BackToLogin)),
        _ => None,
    }
}

/// What a confirmed question or a prompt goes on to do.
enum Then {
    BackToLogin,
    /// Quit the client (Esc before a map, answered yes).
    Quit,
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
    /// The save-ID box, off and on; Opening, Sign Up and the close X (normal, over, press).
    save: [Texture; 2],
    opening: ButtonImages,
    sign_up: ButtonImages,
    exit: ButtonImages,
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
    /// NPC text's link underlines: web links', every other link's.
    url_line: Texture,
    link_line: Texture,
    /// The magic square under a ground cast.
    magic_target: Option<Texture>,
    aura: aura::Textures,
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
    say: npc_say::Assets,
    npc_menu: npc_menu::Assets,
    npc_input: npc_input::Assets,
    cutin: cutin::Assets,
    next: ButtonImages,
    close: ButtonImages,
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

impl Assets {
    fn load(gpu: &Gpu, grf: &Grf, font: &Font, messages: &[String]) -> Self {
        let login = format!("{UI}login_interface\\");
        let image = |path: String| gpu.load_texture(grf, &path, false);
        let button = |folder: &str, name: &str| ["", "_a", "_b"].map(|suffix| image(format!("{folder}{name}{suffix}.bmp")));
        Self {
            background: gpu.load_texture(grf, &format!("{UI}bgi_temp.bmp"), true),
            login: image(format!("{login}bg_login.tga")),
            save: ["checkbox_off", "checkbox_on"].map(|name| image(format!("{login}{name}.bmp"))),
            opening: ["normal", "over", "press"].map(|state| image(format!("{login}bt_close_{state}.bmp"))),
            sign_up: ["normal", "over", "press"].map(|state| image(format!("{login}bt_join_{state}.bmp"))),
            exit: ["normal", "over", "press"].map(|state| image(format!("{login}bt_close2_{state}.bmp"))),
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
            url_line: gpu.solid([links::URL_COLOR[0], links::URL_COLOR[1], links::URL_COLOR[2], 0xFF]),
            link_line: gpu.solid([links::NPC_COLOR[0], links::NPC_COLOR[1], links::NPC_COLOR[2], 0xFF]),
            magic_target: gpu.try_load_texture(grf, "data\\texture\\effect\\magic_target.tga", false).ok(),
            aura: aura::Textures::load(gpu, grf),
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
            say: npc_say::Assets::load(gpu),
            npc_menu: npc_menu::Assets::load(gpu, grf),
            npc_input: npc_input::Assets::load(gpu),
            cutin: cutin::Assets::load(gpu, grf),
            next: button(UI, "btn_next"),
            close: button(UI, "btn_close"),
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

/// The login window's top-left: (W − 640) / 2 + 185 across, 300·H/480 down.
fn login_position() -> [f32; 2] {
    escape::menu_origin(screen())
}

/// A part of the login window.
#[derive(Clone, Copy, Debug, PartialEq)]
enum LoginPart {
    Drop,
    /// A row of the open drop-down's list.
    Account(usize),
    Field(usize),
    Save,
    Opening,
    SignUp,
    Start,
    Exit,
}

/// The open drop-down's rows, under it.
// ponytail: the list's rows take the drop-down's size, one under the other.
fn account_row(row: usize) -> [f32; 2] {
    offset(LOGIN_DROP.0, [0.0, (row + 1) as f32 * LOGIN_DROP.1[1]])
}

/// The login window's part under a window-local point (the drop-down's list over the rest while it's open).
fn login_part(local: [f32; 2], list_open: bool) -> Option<LoginPart> {
    if let Some(row) = (0..LOGIN_ACCOUNTS.len()).find(|&row| list_open && contains(account_row(row), LOGIN_DROP.1, local)) {
        return Some(LoginPart::Account(row));
    }
    let parts = [
        (LOGIN_DROP, LoginPart::Drop),
        ((LOGIN_FIELDS[0], LOGIN_FIELD_SIZE), LoginPart::Field(0)),
        ((LOGIN_FIELDS[1], LOGIN_FIELD_SIZE), LoginPart::Field(1)),
        (LOGIN_SAVE, LoginPart::Save),
        (LOGIN_OPENING, LoginPart::Opening),
        (LOGIN_SIGN_UP, LoginPart::SignUp),
        ((LOGIN_START, LOGIN_START_SIZE), LoginPart::Start),
        (LOGIN_EXIT, LoginPart::Exit),
    ];
    parts.into_iter().find(|((corner, size), _)| contains(*corner, *size, local)).map(|(_, part)| part)
}

/// What stops a login before it's sent: the line to show and its number (4 for an ID, 6 for a password).
fn login_refusal(id: &str, password: &str) -> Option<(usize, usize)> {
    if id.len() < ID_TOO_SHORT.1 {
        Some(ID_TOO_SHORT)
    } else if password.len() < PASSWORD_TOO_SHORT.1 {
        Some(PASSWORD_TOO_SHORT)
    } else {
        None
    }
}

fn shortcut_to_bind(key: options::ShortcutKey) -> Option<shortcuts::ShortcutKey> {
    match key {
        options::ShortcutKey::NamedF(number) => shortcuts::function_key(number),
        options::ShortcutKey::Char(character) => Some(shortcuts::ShortcutKey::Char(character.to_ascii_lowercase())),
    }
}

/// The system clipboard's text up to its first line break, for a one-line box.
fn clipboard_line() -> String {
    let text = arboard::Clipboard::new().and_then(|mut clipboard| clipboard.get_text()).unwrap_or_default();
    text.lines().next().unwrap_or_default().chars().filter(|character| !character.is_control()).collect()
}

/// Puts text on the system clipboard (nothing for an empty selection).
fn set_clipboard(text: &str) {
    if !text.is_empty() {
        let _ = arboard::Clipboard::new().and_then(|mut clipboard| clipboard.set_text(text.to_owned()));
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

/// A party booking request and what the web server said to it.
type BookingAnswer = (booking::Request, std::io::Result<booking::Reply>);

/// The web server's address (party booking): RUSTNAROK_CLIENT_WEB, else the login server's host on 8888.
// ponytail: the web server's port, rathena's default; the client's own source of it isn't in our data.
fn web_address(login: &str) -> String {
    std::env::var("RUSTNAROK_CLIENT_WEB").ok().filter(|address| !address.is_empty()).unwrap_or_else(|| {
        let host = login.rsplit_once(':').map_or(login, |(host, _)| host);
        format!("{host}:8888")
    })
}

/// The lines of the window places that keep sizes: the inventory's and the cart's columns and rows, the storage's
/// rows, and an NPC shop's goods width and both windows' rows (the next shop opens at those).
const INVENTORY_SIZE: &str = "inventory_size";
const CART_SIZE: &str = "cart_size";
const STORAGE_SIZE: &str = "storage_size";
const SHOP_SIZE: &str = "shop_size";
const SHOP_DEAL_SIZE: &str = "shop_deal_size";
/// The party window's size mode, kept with the places.
const PARTY_MODE: &str = "party_mode";
const VEND_SIZE: &str = "vend_size";
const SKILL_LIST_SIZE: &str = "skill_list_size";
const ITEM_PLACE: &str = "item_info";
const QUEST_PLACE: &str = "quest";
const CARD_PLACE: &str = "card_info";

/// The sizes' lines of the window places: the inventory's and the cart's columns and rows, the storage's rows, an NPC
/// shop's goods width and rows, then its deal's rows.
fn size_lines(inventory: [usize; 2], cart: [usize; 2], storage: usize, (width, [goods, deal]): (f32, [usize; 2])) -> String {
    let [[columns, rows], [cart_columns, cart_rows]] = [inventory, cart];
    format!(
        "{INVENTORY_SIZE} {columns} {rows} 0\n{CART_SIZE} {cart_columns} {cart_rows} 0\n{STORAGE_SIZE} 0 {storage} 0\n{SHOP_SIZE} {width} {goods} 0\n{SHOP_DEAL_SIZE} 0 {deal} 0\n"
    )
}

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
/// The world map opens, or closes when open (its shortcut, and the minimap's View On).
fn toggle_world_map(game: &mut Game, gpu: Option<&render::Gpu>, grf: &Grf, worlds: &(Vec<rustnarok_client::lua::World>, [[u8; 4]; 2])) {
    let map = map_key(&game.map);
    game.world_map = match (game.world_map.take(), gpu) {
        (None, Some(gpu)) => worldmap::WorldMap::open(gpu, grf, &worlds.0, worlds.1, &map),
        _ => None,
    };
}

/// Opens or closes the big minimap, with the player's head; on a map in the exception table, the OK box of
/// `minimap_window::NO_PICTURE` instead.
fn toggle_big_minimap(game: &mut Game, gpu: &render::Gpu, grf: &Grf, exceptions: &HashSet<String>) {
    let map = map_key(&game.map);
    if !game.minimap_window.frame.open && exceptions.contains(&map) {
        game.question_box = Some(BoxQuestion::Notice(minimap_window::NO_PICTURE));
        return;
    }
    let image = game.minimap.clone();
    let head = game.player.look.as_ref().and_then(|look| actor::head_image(grf, look));
    game.minimap_window.set_head(gpu, head.as_ref());
    game.minimap_window.toggle(gpu, image.as_ref(), &map);
    game.sync_esc();
}

/// Whether the basic info and chat windows are kept between runs: always for a player, and for a client the test
/// driver runs (`RUSTNAROK_CLIENT_CONTROL`) only with the dev switch `RUSTNAROK_CLIENT_KEEP_WINDOWS`, so one
/// scenario's windows don't move the next one's.
pub(crate) fn keeps_windows() -> bool {
    std::env::var_os("RUSTNAROK_CLIENT_CONTROL").is_none() || std::env::var_os("RUSTNAROK_CLIENT_KEEP_WINDOWS").is_some()
}

/// Where the player's settings file `name` is kept: Application Support on a Mac, %APPDATA% on Windows, the XDG config
/// folder elsewhere.
/// The small party windows saved for a character (by name): a member's account and cell a line.
fn small_windows_path(character: &str) -> Option<std::path::PathBuf> {
    settings_path(&format!("party_mini_{character}"))
}

fn read_small_windows(character: &str) -> String {
    small_windows_path(character).and_then(|path| std::fs::read_to_string(path).ok()).unwrap_or_default()
}

fn write_small_windows(character: &str, saved: &str) {
    if let Some(path) = small_windows_path(character) {
        let _ = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(path, saved));
    }
}

pub(crate) fn settings_path(name: &str) -> Option<std::path::PathBuf> {
    // Dev switch: another settings folder (the test harness gives each scenario a fresh one).
    if let Some(folder) = std::env::var_os("RUSTNAROK_CLIENT_SETTINGS").filter(|folder| !folder.is_empty()) {
        return Some(std::path::PathBuf::from(folder).join(name));
    }
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

/// A window that a button opens and shuts: shut when it's open, else a new one.
fn toggled<T>(window: Option<T>, open: impl FnOnce() -> T) -> Option<T> {
    if window.is_some() { None } else { Some(open()) }
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

/// Splits NPC text into lines that fit `width`; `^RRGGBB` switches color. Returns colored pieces per line.
/// A message box's lines: the client's messages break lines with runs of spaces; each part wraps at word ends.
fn message_lines(font: &text::Font, text: &str, width: f32) -> Vec<String> {
    text.split("  ")
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .flat_map(|part| wrap_colored(font, part, width).into_iter().map(|pieces| pieces.into_iter().map(|(_, piece)| piece).collect::<String>()))
        .collect()
}

/// Opens a web page in the system's browser. A client driven over the control socket only notes it.
fn open_page(game: &mut Game, page: &str) {
    if control::driven() {
        game.opened_page = Some(page.to_owned());
        return;
    }
    // The address goes as one argument to a program that takes it as is; no shell reads it.
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(windows) {
        "explorer"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(opener).arg(page).spawn();
}
fn wrap_colored(font: &text::Font, text: &str, width: f32) -> iteminfo::ColoredLines {
    wrap_linked(font, text, width).into_iter().map(|pieces| pieces.into_iter().map(|(color, piece, _)| (color, piece)).collect()).collect()
}

/// Colour-coded text wrapped to `width`, item links in their colour and each piece with the link it's part of.
fn wrap_linked(font: &text::Font, text: &str, width: f32) -> links::LinkedLines {
    wrap_linked_by(font, text, width, false, links::Target::color)
}

/// `wrap_linked`, breaking between words or (`by_char`, as NPC text does) wherever the next character would pass
/// `width`, links in `color_of` theirs.
fn wrap_linked_by(font: &text::Font, text: &str, width: f32, by_char: bool, color_of: fn(&links::Target) -> [u8; 3]) -> links::LinkedLines {
    let mut wrapped: links::LinkedLines = vec![Vec::new()];
    let mut line_width = 0.0;
    let mut color = [0, 0, 0];
    for (part, item) in links::pieces(text) {
        for (row, pieces) in wrap_plain(font, &part, width, line_width, &mut color, by_char).into_iter().enumerate() {
            if row > 0 {
                wrapped.push(Vec::new());
                line_width = 0.0;
            }
            for (color, piece) in pieces {
                line_width += font.width(&piece);
                let color = item.as_ref().map_or(color, color_of);
                wrapped.last_mut().unwrap().push((color, piece, item.clone()));
            }
        }
    }
    wrapped
}

/// Colour-coded text wrapped to `width` between words or `by_char`, its first line already `used` wide and `color` in
/// force (left as the text leaves it).
fn wrap_plain(font: &text::Font, text: &str, width: f32, used: f32, color: &mut [u8; 3], by_char: bool) -> iteminfo::ColoredLines {
    let mut lines: iteminfo::ColoredLines = vec![Vec::new()];
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
        // Next word (with its leading space) up to the next space or color code, or the next character.
        let first = rest.chars().next().map_or(1, char::len_utf8);
        let end = if by_char { first } else { rest[first..].find([' ', '^']).map_or(rest.len(), |index| index + first) };
        let word = &rest[..end];
        let word_width = font.width(word);
        if line_width + word_width > width && line_width > 0.0 {
            lines.push(Vec::new());
            line_width = 0.0;
        }
        let word = if line_width == 0.0 && !by_char { word.trim_start() } else { word };
        let line = lines.last_mut().unwrap();
        // Characters of one colour make one piece.
        match line.last_mut() {
            Some((last, piece)) if by_char && last == color => piece.push_str(word),
            _ => line.push((*color, word.to_owned())),
        }
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

/// What a frame shows: images plus text to rasterize (with color), drawn in that order per layer (per window: see `window`).
#[derive(Default)]
struct Layer<'a> {
    sprites: Vec<Sprite<'a>>,
    texts: Vec<(String, [f32; 2], [u8; 3])>,
    /// How see-through it is drawn, 0 (opaque) to 1: a window being dragged.
    faded: f32,
    /// Where each window after the first starts (sprites, texts): it covers the text of the windows before it.
    windows: Vec<(usize, usize)>,
}

impl Layer<'_> {
    /// Starts another window: what's drawn from here goes over everything drawn so far, text included.
    fn window(&mut self) -> &mut Self {
        self.windows.push((self.sprites.len(), self.texts.len()));
        self
    }

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
    /// Maps with no big minimap (`data\ExceptionMinimapNameTable.txt`).
    minimap_exceptions: HashSet<String>,
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
    /// Tab is held down (the fourth shortcut modifier).
    tab_held: bool,
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
    /// Our guess of the server's tick, to place walks in time; and whether a map has loaded since the last
    /// ZC_NOTIFY_TIME (the first one after a load sets the clock outright).
    server_clock: actor::ServerClock,
    map_loaded: bool,
    fields: [String; 2],
    /// Remember the ID for next time (the login window's save box).
    save_id: bool,
    /// The login window's account drop-down: the pick (of LOGIN_ACCOUNTS), and whether its list is open.
    // ponytail: the pick isn't sent with the login (rathena has no account class).
    account_kind: usize,
    account_list: bool,
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

    /// The Login button or Enter: the button's sound, then an ID under 4 bytes or a password under 6 says so and stays;
    /// else the login goes out.
    fn submit_login(&mut self) {
        if let Some(audio) = &mut self.audio {
            audio.play(&self.grf, BUTTON_SOUND, 1.0);
        }
        if let Some((id, least)) = login_refusal(&self.fields[0], &self.fields[1]) {
            let text = rustnarok_client::messages::fill(&self.message_text(id), &[&least]);
            self.message(text, Scene::Login);
            return;
        }
        self.send_login();
    }

    /// Sends the login (the dev switch's too, which skips the button's checks).
    fn send_login(&mut self) {
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
                // The NPC's input box takes it; plain dialogs don't.
                Some(dialog) => {
                    if let Some(input) = &mut dialog.input {
                        input.type_text(&typed.collect::<String>());
                    }
                }
                None if game.storage.typing => game.storage.type_text(text),
                None if game.pet_window.typing => game.pet_window.name_text.get_or_insert_with(String::new).push_str(text),
                None if game.booking.as_ref().is_some_and(|window| window.focus.is_some()) => game.booking.as_mut().unwrap().type_text(text),
                None if game.party_dialog.as_ref().is_some_and(|dialog| dialog.typing) => game.party_dialog.as_mut().unwrap().type_text(text),
                None if game.reason_box.is_some() => game.reason_box.as_mut().unwrap().type_text(text),
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
    /// Esc on a screen before a map: its question (`pre_map_escape`).
    fn escape_before_map(&mut self) {
        if let Some((question, then)) = pre_map_escape(&self.scene) {
            self.ask(question, then);
        }
    }

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
            Then::Quit => self.quitting = true,
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
}

/// A layer with its text rasterized.
type Composed<'a> = (Vec<Sprite<'a>>, Vec<(Texture, [f32; 2])>, f32);

/// Rasterizes each layer's text, one piece per window (see `Layer::window`).
fn compose<'a>(gpu: &Gpu, font: &Font, layers: Vec<Layer<'a>>) -> Vec<Composed<'a>> {
    layers
        .into_iter()
        .flat_map(|layer| {
            window_spans(&layer.windows, layer.sprites.len(), layer.texts.len())
                .into_iter()
                .map(|(sprites, texts)| {
                    let texts = layer.texts[texts].iter().map(|(text, position, color)| (gpu.text(font, text, *color), *position)).collect();
                    (layer.sprites[sprites].to_vec(), texts, layer.faded)
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Each window's sprites and texts, from where each starts (see `Layer::window`).
fn window_spans(starts: &[(usize, usize)], sprites: usize, texts: usize) -> Vec<(std::ops::Range<usize>, std::ops::Range<usize>)> {
    let mut from = (0, 0);
    starts
        .iter()
        .copied()
        .chain([(sprites, texts)])
        .map(|to| {
            let span = (from.0..to.0, from.1..to.1);
            from = to;
            span
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

/// The middle of the screen, where picking splits it in quarters.
fn screen_middle() -> [f32; 2] {
    [screen()[0] / 2.0, screen()[1] / 2.0]
}

/// The unit or item the pointer is on (see `picking::pick`), the player aside (only skills aim at oneself).
fn hovered(hit_boxes: &[picking::Target<Owner>], cursor: [f32; 2], players_first: bool) -> Option<Owner> {
    picking::pick(hit_boxes.iter().copied().filter(|target| target.owner != Owner::Player), cursor, players_first, screen_middle())
}

/// The unit the pointer is on, the player included: what skills and taming aim at.
fn hovered_unit(hit_boxes: &[picking::Target<Owner>], cursor: [f32; 2], players_first: bool) -> Option<Owner> {
    picking::pick(hit_boxes.iter().copied().filter(|target| !matches!(target.owner, Owner::Item(_))), cursor, players_first, screen_middle())
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
            self.send_login();
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
                    && !game.on_ui(self.cursor, assets.message.size)
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
                        let before = self.graphics;
                        for text in texts.split('|') {
                            game.chat.input = text.to_owned();
                            let _ = game.send_chat(false, false, &self.messages, &mut self.graphics);
                        }
                        if before != self.graphics {
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
                        let before = self.graphics;
                        for whisper in whispers.split(';') {
                            let (to, text) = whisper.split_once('|').unwrap_or((whisper, "hi"));
                            game.chat.whisper = to.to_owned();
                            game.chat.input = text.to_owned();
                            let _ = game.send_chat(false, false, &self.messages, &mut self.graphics);
                        }
                        if before != self.graphics {
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
                    game.end_progress(false);
                    if game.auto_next > 0 && game.dialog.as_ref().is_some_and(|dialog| dialog.button != DialogButton::None) {
                        game.auto_next -= 1;
                        game.press_dialog_button(false);
                    }
                    if let Some(dialog) = &mut game.dialog
                        && let Some(menu) = &mut dialog.menu
                        && let Some(choice) = game.auto_menu.pop()
                    {
                        menu.selected = choice.saturating_sub(1).min(menu.items.len().saturating_sub(1));
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
                        game.press_hotkey(slot, &self.messages);
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
    let minimap_exceptions = minimap_window::load_exceptions(&grf);
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
        minimap_exceptions,
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
        tab_held: false,
        returned: false,
        account: None,
        playing: None,
        entering: None,
        early_events: Vec::new(),
        server_clock: actor::ServerClock::default(),
        map_loaded: false,
        fields: [saved_id().unwrap_or_default(), String::new()],
        save_id: saved_id().is_some(),
        account_kind: 0,
        account_list: false,
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
    fn a_window_covers_the_text_of_the_windows_before_it() {
        // Two windows: the first's 3 sprites and 2 texts, then the second's 1 sprite and 1 text.
        assert_eq!(window_spans(&[(3, 2)], 4, 3), [(0..3, 0..2), (3..4, 2..3)]);
        assert_eq!(window_spans(&[], 4, 3), [(0..4, 0..3)]);
    }

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
        // The sizes ride along and read back.
        let sizes = size_lines([8, 4], [6, 5], 13, (248.0, [2, 7]));
        let read = parse_window_places(&sizes);
        assert_eq!(
            [INVENTORY_SIZE, CART_SIZE, STORAGE_SIZE, SHOP_SIZE, SHOP_DEAL_SIZE].map(|name| read[name].0),
            [[8.0, 4.0], [6.0, 5.0], [0.0, 13.0], [248.0, 2.0], [0.0, 7.0]]
        );
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
        assert_eq!(vanish_effect(2, actor::OPTION_INVISIBLE), Some(314), "a hidden GM's logout shows");
        assert_eq!((vanish_effect(0, 0), vanish_effect(1, 0)), (None, None));
    }

    #[test]
    fn faces_the_way_it_entered_after_a_warp() {
        // Entered facing north-east (5), then sat facing south: a warp (same map or not) stands it facing 5 again.
        let mut player = Actor::new("Me".into(), None, [10, 10], 5, 150);
        player.set_direction(0);
        player.sitting = true;
        after_warp(&mut player, [40, 41], 5);
        assert_eq!((player.cell, player.direction, player.sitting, player.moving()), ([40, 41], 5, false, false));
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
    fn units_spawned_in_view_fade_in() {
        let ms = |ms: u64| Duration::from_millis(ms);
        let byte = |since: Duration| (actor::spawn_alpha(since) * 255.0).round() as u32;
        assert_eq!([byte(ms(0)), byte(Duration::from_secs_f32(1.0 / 60.0)), byte(ms(383)), byte(ms(400)), byte(ms(1000))], [68, 76, 252, 255, 255]);
        let now = Instant::now();
        let mut poring = Actor::new("Poring".into(), None, [10, 10], 0, 150);
        poring.fade_in = Some(now);
        assert!(poring.shadowless(now + ms(383)) && !poring.shadowless(now + ms(400)));
        // Which units fade, which get the rings, which simply show.
        assert_eq!(arrival(true, false, false, MONSTER, 0), Arrival::Fade);
        assert_eq!(arrival(true, false, false, NPC, 0), Arrival::Fade);
        assert_eq!(arrival(true, false, false, PLAYER, 0), Arrival::Rings);
        assert_eq!(arrival(true, false, false, PLAYER, GM_HIDE), Arrival::Plain);
        assert_eq!(arrival(false, false, false, MONSTER, 0), Arrival::Plain, "already there or walking in");
        assert_eq!(arrival(true, true, false, MONSTER, 0), Arrival::Plain, "known to the client");
        assert_eq!(arrival(true, false, true, PLAYER, 0), Arrival::Plain, "our own character");
        // Turned round: the two job ranges and their edges.
        assert_eq!([21063, 21064, 21088, 21089, 22550, 22551, 22668, 22669].map(turned_round), [false, true, true, false, false, true, true, false]);
    }

    #[test]
    fn draws_player_jobs_as_players() {
        // Any unit with a player job (0–30, 4001–5999) is drawn as a player; other jobs only on players.
        assert_eq!([0, 30, 31, 4000, 4001, 5999, 6000].map(|job| drawn_as_player(MONSTER, job)), [true, true, false, false, true, true, false]);
        assert!(drawn_as_player(NPC, 4008) && !drawn_as_player(NPC, 45) && drawn_as_player(PLAYER, 1002));
        // A type 0 change gives a monster another job.
        let mut poring = Actor::new("Poring".into(), None, [10, 10], 0, 150);
        (poring.kind, poring.job) = (MONSTER, 1002);
        assert!(take_job(&mut poring, 1031) && poring.job == 1031);
        assert!(!take_job(&mut poring, 1031), "the same job: nothing to load");
        // A move entry with new headgear changes a known player's look; a stand entry does too, for a player only.
        let unit = net::Unit { id: 150_014, kind: PLAYER, job: 7, sex: 1, head_top: 5, ..net::Unit::default() };
        let mut player = Actor::new("Someone".into(), None, [10, 10], 0, 150);
        player.look = Some(unit_look(&net::Unit { head_top: 0, ..unit.clone() }));
        assert_eq!(refreshed_look(&player, &unit, true).map(|look| look.head_top), Some(5));
        player.look = refreshed_look(&player, &unit, true);
        assert_eq!(refreshed_look(&player, &unit, true), None, "already so");
        let clone = net::Unit { kind: MONSTER, head_top: 9, ..unit.clone() };
        assert!(refreshed_look(&player, &clone, false).is_none() && refreshed_look(&player, &clone, true).is_some());
    }

    #[test]
    fn players_in_view_lie_sit_or_stand() {
        let now = Instant::now();
        let mut player = Actor::new("Someone".into(), None, [10, 10], 0, 150);
        player.kind = PLAYER;
        take_posture(&mut player, Some(1), now);
        assert!(player.dead && player.remove_at.is_none());
        take_posture(&mut player, Some(2), now);
        assert!(!player.dead && player.sitting);
        take_posture(&mut player, Some(0), now);
        assert!(!player.dead && !player.sitting);
        take_posture(&mut player, None, now);
        assert!(!player.dead && !player.sitting, "no state byte: as it was");
        // A monster's byte is ignored.
        let mut poring = Actor::new("Poring".into(), None, [10, 10], 0, 150);
        poring.kind = MONSTER;
        take_posture(&mut poring, Some(1), now);
        take_posture(&mut poring, Some(2), now);
        assert!(!poring.dead && !poring.sitting && poring.remove_at.is_none());
    }

    #[test]
    fn a_returning_unit_leaves_its_fading_copy() {
        let now = Instant::now();
        let mut units = HashMap::new();
        let mut leaving = Actor::new("Poring".into(), None, [10, 10], 0, 150);
        leaving.fade_out(now, OUT_OF_SIGHT_FADE_MS);
        units.insert(110_000_001, leaving);
        units.insert(110_000_002, Actor::new("Lunatic".into(), None, [12, 10], 0, 150));
        leave_behind(&mut units, 110_000_001);
        leave_behind(&mut units, 110_000_002);
        assert!(!units.contains_key(&110_000_001) && units[&(110_000_001 | LEFT_BEHIND)].remove_at.is_some());
        assert!(units.contains_key(&110_000_002), "a unit not fading stays itself");
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
    fn talking_holds_from_a_page_to_the_close() {
        let npc = 110_000_005;
        assert_eq!(npc_holds(&MapEvent::NpcSay { npc, text: "Hi".into() }), Some((npc, true)));
        assert_eq!(npc_holds(&MapEvent::NpcNext { npc }), Some((npc, true)));
        assert_eq!(npc_holds(&MapEvent::NpcMenu { npc, items: vec!["Yes".into()] }), Some((npc, true)));
        assert_eq!(npc_holds(&MapEvent::NpcClose { npc }), Some((npc, false)));
        assert_eq!(npc_holds(&MapEvent::NpcInput { npc, text: false }), None);
        // The cast bar (a progress bar) lets map clicks through.
        assert!(talk_holds(true, false) && !talk_holds(true, true) && !talk_holds(false, false));
    }

    #[test]
    fn ends_a_progress_bar_once() {
        let start = Instant::now();
        let mut bar = ProgressBar::default();
        assert!(!bar.ends(start, true), "no bar, nothing to end");
        bar.start(start, Duration::from_secs(3));
        assert!(!bar.ends(start + Duration::from_millis(2999), false));
        assert!(bar.ends(start + Duration::from_secs(3), false));
        assert!(!bar.ends(start + Duration::from_secs(4), false));
        // Taken away by the server: once, too.
        bar.start(start, Duration::from_secs(3));
        assert!(bar.ends(start, true) && !bar.ends(start, true));
    }

    #[test]
    fn attacks_walls_and_traps_on_their_cells() {
        // Which ground units a click attacks.
        assert!([0x8d, 0x8f, 0x98, 0xda, 0xde, 0x5174, 0x5177].into_iter().all(attackable_ground_unit));
        assert!(![0x7e, 0x86, 0x8c, 0x90, 0x5173, 0x5178].into_iter().any(attackable_ground_unit));
        // The attack goes for the wall's cell, and ends once the wall is gone.
        let units = HashMap::new();
        let mut walls = HashMap::from([(3354, (0x8d, [156, 190]))]);
        assert_eq!(attack_goal(&units, &walls, 3354), Some([156, 190]));
        walls.remove(&3354);
        assert_eq!(attack_goal(&units, &walls, 3354), None);
    }

    #[test]
    fn places_the_login_window_and_its_parts() {
        let place = |screen: [f32; 2]| escape::menu_origin(screen);
        assert_eq!(
            [[640.0, 480.0], [800.0, 600.0], [1024.0, 768.0], [1920.0, 1080.0]].map(place),
            [[185.0, 300.0], [265.0, 375.0], [377.0, 480.0], [825.0, 675.0]]
        );
        assert_eq!(login_part([20.0, 15.0], false), Some(LoginPart::Drop));
        assert_eq!(login_part([16.0, 41.0], false), Some(LoginPart::Field(0)));
        assert_eq!(login_part([167.0, 79.0], false), Some(LoginPart::Field(1)));
        assert_eq!(login_part([168.0, 64.0], false), None);
        assert_eq!(login_part([57.0, 86.0], false), Some(LoginPart::Save));
        assert_eq!(login_part([14.0, 105.0], false), Some(LoginPart::Opening));
        assert_eq!(login_part([170.0, 124.0], false), Some(LoginPart::SignUp));
        assert_eq!(login_part([193.0, 33.0], false), Some(LoginPart::Start));
        assert_eq!(login_part([274.0, 16.0], false), Some(LoginPart::Exit));
        // The open drop-down's rows lie over the boxes under it.
        assert_eq!(login_part([20.0, 40.0], true), Some(LoginPart::Account(0)));
        assert_eq!(login_part([20.0, 60.0], true), Some(LoginPart::Account(1)));
        assert_eq!(login_part([20.0, 66.0], true), Some(LoginPart::Account(1)));
        assert_eq!(login_part([20.0, 66.0], false), Some(LoginPart::Field(1)));
    }

    #[test]
    fn refuses_short_ids_and_passwords() {
        assert_eq!(login_refusal("abc", "secret1"), Some((3029, 4)));
        assert_eq!(login_refusal("abcd", "12345"), Some((3030, 6)));
        assert_eq!(login_refusal("abcd", "123456"), None);
        // Bytes, not characters: two Korean letters are 6 bytes.
        assert_eq!(login_refusal("가나", "123456"), None);
    }

    #[test]
    fn settings_buttons_open_then_shut_their_window() {
        let mut window = toggled(None, || 7);
        assert_eq!(window, Some(7));
        window = toggled(window, || 8);
        assert_eq!(window, None);
    }

    #[test]
    fn asks_before_leaving_a_screen_before_the_map() {
        assert!(matches!(pre_map_escape(&Scene::Login), Some((QUIT_MESSAGE, Then::Quit))));
        assert!(matches!(pre_map_escape(&Scene::Connections { selected: 0 }), Some((QUIT_MESSAGE, Then::Quit))));
        let waiting = Scene::Waiting(Box::new(Scene::Login));
        assert!(pre_map_escape(&waiting).is_none(), "a box over the screen takes Esc itself");
    }

    #[test]
    fn breaks_npc_text_mid_word() {
        let font = Font::load();
        // Room for "Hello wor": the word breaks there, its colour carried onto the next line.
        let width = font.width("Hello wor") + 0.5;
        let lines = wrap_linked_by(&font, "^0000FFHello world^000000 again", width, true, links::Target::npc_color);
        let text: Vec<Vec<([u8; 3], &str)>> = lines.iter().map(|pieces| pieces.iter().map(|(color, piece, _)| (*color, piece.as_str())).collect()).collect();
        assert_eq!(text[0], [([0, 0, 0xFF], "Hello wor")]);
        assert_eq!(text[1][0], ([0, 0, 0xFF], "ld"));
        assert_eq!(text[1][1].0, [0, 0, 0]);
        // Between words it would have moved "world" down whole.
        assert_eq!(wrap_linked(&font, "Hello world", width)[0].iter().map(|(_, piece, _)| piece.as_str()).collect::<String>(), "Hello");
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
        let (shown, runs) = links::split(&links::mark(navi, &|_| String::new()));
        assert_eq!(shown, "Let me mark the ^B9062F[Tool Shop]^000000 for you.");
        assert_eq!(runs.iter().map(|(_, target)| target.clone()).collect::<Vec<_>>(), [links::Target::Navi { map: "prontera".into(), cell: [134, 221] }]);
        let marked = links::mark("a <ITEM>Apple<INFO>512</INFO></ITEM> < b", &|_| String::new());
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

    #[test]
    fn camera_shakes_and_snaps_back_at_the_end() {
        let mut goal = CameraGoal::new(false);
        let feet = [100.0, -5.0, 200.0];
        let mut camera = goal.camera(feet);
        let before = (camera.target, camera.distance, camera.pitch);

        let start = Instant::now();
        let mut effects = effects::Effects::default();
        effects.start_quake(start, effects::QUAKE_LONG);
        for frame in 0..10 {
            let now = start + Duration::from_millis(frame * 16);
            goal.ease(&mut camera, Duration::from_millis(16), feet, false);
            goal.shake(&mut camera, feet, effects.quake(now));
            assert!(goal.shaking);
            assert_ne!((camera.target, camera.distance, camera.pitch), before);
        }
        // When the shake ends, the look-at point, distance and pitch snap straight back.
        goal.ease(&mut camera, Duration::from_millis(16), feet, false);
        goal.shake(&mut camera, feet, effects.quake(start + effects::QUAKE_LONG));
        assert!(!goal.shaking);
        assert_eq!((camera.target, camera.distance, camera.pitch), before);
    }

    #[test]
    fn opens_the_menu_for_what_is_under_the_pointer() {
        let players = [7u32, 8];
        let is_player = |id: u32| players.contains(&id);
        let menu = |picked| menu_for(picked, is_player, Some(20), Some(30), false);
        // A player alone: their menu. A monster or an item over them is what's picked: no menu.
        assert_eq!(menu(Some(Owner::Unit(7))), Some(MenuFor::Player(7)));
        assert_eq!((menu(Some(Owner::Unit(99))), menu(Some(Owner::Item(5))), menu(None)), (None, None, None));
        // Our pet and our homunculus have their own; we have one only with our stall open.
        assert_eq!((menu(Some(Owner::Unit(20))), menu(Some(Owner::Unit(30)))), (Some(MenuFor::Pet(20)), Some(MenuFor::Homunculus(30))));
        assert_eq!(menu(Some(Owner::Player)), None);
        assert_eq!(menu_for(Some(Owner::Player), is_player, None, None, true), Some(MenuFor::Stall));
        // The pick itself goes by the usual ranks: a monster's area over a player's wins.
        let target = |owner, kind| picking::Target { owner, kind, sprite: [0.0; 4], area: [100.0, 100.0, 200.0, 200.0], depth: 1.0 };
        let boxes = [target(Owner::Unit(7), picking::Kind::Player), target(Owner::Unit(99), picking::Kind::Monster)];
        assert!(picking::pick(boxes, [150.0, 150.0], false, [512.0, 384.0]) == Some(Owner::Unit(99)));
    }

    #[test]
    fn lists_a_players_entries_in_the_games_order() {
        let labels = |ties| player_choices(7, ties).into_iter().map(|(label, _)| label).collect::<Vec<_>>();
        // A stranger: a deal, 1:1 chat, friends, mail, equipment, block.
        assert_eq!(labels(PlayerTies::default()), [391, 360, 358, 2808, 1360, 3820]);
        // Our party's leader, a guild master facing another guild's member, a friend, adoption, our chat room.
        let all =
            PlayerTies { trading: false, party_invite: true, guild_invite: true, guild_relation: true, friend: true, adoptable: true, in_our_room: Some(2) };
        assert_eq!(labels(all), [391, 392, 382, 399, 403, 360, 2808, 1360, 887, 3820, 458]);
        assert!(player_choices(7, all).contains(&(458, Choice::RoomKick(2))));
        // While trading there's no deal to ask for.
        assert_eq!(labels(PlayerTies { trading: true, friend: true, ..PlayerTies::default() }), [360, 2808, 1360, 3820]);
        // The pet's status comes first, all five always; the homunculus has three entries.
        assert_eq!(PET_CHOICES.map(|(label, _)| label), [596, 592, 593, 595, 594]);
        assert_eq!(HOMUNCULUS_CHOICES.map(|(label, _)| label), [1112, 1113, 1114]);
    }

    #[test]
    fn sparks_landed_attacks_and_heals_by_amount() {
        // A hit sparks EF_HIT1, a critical EF_HIT2 too, the Taekwon line 434; a miss or a lucky dodge nothing.
        assert_eq!((attack_sparks(12, 0, false), attack_sparks(40, 10, false)), (vec![EF_HIT1], vec![EF_HIT1, EF_HIT2]));
        assert_eq!(attack_sparks(12, 8, true), [TAEKWON_HIT]);
        assert!(attack_sparks(0, 0, false).is_empty() && attack_sparks(12, 11, false).is_empty());
        assert_eq!([heal_effect(5000), heal_effect(2000), heal_effect(450), heal_effect(80)], [325, 313, 312, 7]);
        assert!(HEAL_SKILLS.contains(&2051) && !HEAL_SKILLS.contains(&2044), "Highness Heal, not Epiclesis");
    }
}
