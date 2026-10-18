//! Clean-room native Ragnarok Online client. Assets come from the user's own data.grf at runtime.
//!
//! README.md says what it does so far and how to set it up.
//!
//! Dev switches: `RUSTNAROK_CLIENT_GRF` (data.grf path), `RUSTNAROK_CLIENT_SERVER` (login server, over the data folder's
//! clientinfo.xml and the default 127.0.0.1:6900),
//! `RUSTNAROK_CLIENT_LOGIN=user:password` (fill in and submit on start), `RUSTNAROK_CLIENT_AUTOSTART=<slot>` (then pick the first
//! server and play that slot; any other value stops at character select), `RUSTNAROK_CLIENT_WALK=x,y` (walk there a second after entering the map),
//! `RUSTNAROK_CLIENT_TALK=<name>[:<n>]` (talk to that NPC, pressing "next" or "close" up to n times), `RUSTNAROK_CLIENT_ATTACK=1` (attack the
//! nearest monster in view), `RUSTNAROK_CLIENT_SAY=<text>[|<text>...]` (type each into the chat a second after entering, e.g.
//! GM commands or /organize; `RUSTNAROK_CLIENT_SAY_AT` seconds in instead),
//! `RUSTNAROK_CLIENT_PICKUP=1` (pick up the nearest item once one lies around), `RUSTNAROK_CLIENT_INVENTORY=1` (open the inventory),
//! `RUSTNAROK_CLIENT_CURSOR=x,y` (where the pointer starts, to see hover labels in screenshots),
//! `RUSTNAROK_CLIENT_CAMERA=yaw,pitch,zoom` (the camera turned, in degrees and zoom units, on every map),
//! `RUSTNAROK_CLIENT_WEAR=1` (put on every piece of equipment carried, once the inventory is in), `RUSTNAROK_CLIENT_EQUIPMENT=1` (open
//! the equipment window), `RUSTNAROK_CLIENT_SHOT_LOADING=1` (screenshot the loading screen instead of the game),
//! `RUSTNAROK_CLIENT_SKILLS=1` (open the skill window), `RUSTNAROK_CLIENT_SKILL=<id>:<level>` (use a skill 3 seconds in, on oneself
//! if it needs a target), `RUSTNAROK_CLIENT_HOTKEYS=<slot>[,<slot>...]` (press those hotkeys, one a second from 3 seconds in),
//! `RUSTNAROK_CLIENT_DEAL=buy|sell[:<row>:<amount>]` (answer a shop's question at once, and make that deal),
//! `RUSTNAROK_CLIENT_MENU=<n>[,<n>...]` (pick those NPC menu items, counting from 1, as menus come up),
//! `RUSTNAROK_CLIENT_STORE=put|take:<item>:<amount>` (once storage opens, move that much of an item in or out),
//! `RUSTNAROK_CLIENT_TRADE=ask:<name>|accept` (ask that player for a deal, or take the first one asked),
//! `RUSTNAROK_CLIENT_TRADE_GIVE=<item>:<amount>[,...][,z<zeny>]` (once a deal opens, put those in, lock and exchange),
//! `RUSTNAROK_CLIENT_ROOM=window|make:<title>` (open the chat room maker with that title, or make the room),
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

mod achievement;
mod actor;
mod bank;
mod basic_info;
mod char_make;
mod char_select;
mod chat;
mod chatroom;
mod clan;
mod control;
mod cursor;
mod effects;
mod emotion;
mod emotion_list;
mod equipment;
mod escape;
mod guild;
mod hotkeys;
mod inventory;
mod iteminfo;
mod macros;
mod mail;
mod material;
mod menu;
mod mercenary;
mod minimap;
mod monster_info;
mod options;
mod overhead;
mod party;
mod pet;
mod pick_list;
mod picking;
mod quest;
mod refine;
mod render;
mod roulette;
mod shop;
mod skills;
mod sound;
mod status;
mod storage;
mod trade;
mod vend;
mod weather;
mod window;
mod world;
mod worldmap;

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
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
use world::{Billboard, Camera, World, WorldFrame};

/// The client renders in these logical pixels; Retina scaling happens in the projection.
pub const SCREEN: [f32; 2] = [1024.0, 768.0];
const UI: &str = "data\\texture\\유저인터페이스\\";
const DEFAULT_SERVER: &str = "127.0.0.1:6900";
const CARET_BLINK: Duration = Duration::from_millis(500);
const TEXT_COLOR: [u8; 3] = [0, 0, 0];
const FRAME_TIME: Duration = Duration::from_millis(16);
/// The client pings the map server so idle connections aren't dropped.
const KEEPALIVE: Duration = Duration::from_secs(10);
/// Waits this long after the walk to a target ends before attacking; the server may still have the player a cell back.
const ATTACK_SETTLE: Duration = Duration::from_millis(50);
/// Chat colors: others, our own, item messages, errors.
const CHAT_WHITE: [u8; 3] = [255, 255, 255];
const CHAT_SELF: [u8; 3] = [0x00, 0xFF, 0x00];
const CHAT_ITEM: [u8; 3] = [0x00, 0xFF, 0xFF];
const CHAT_ERROR: [u8; 3] = [0xFF, 0x00, 0x00];
// ponytail: the two announcement shades are guesses; calibrate them against the real client.
const CHAT_BROADCAST: [u8; 3] = [0xFF, 0xFF, 0x00];
const CHAT_BROADCAST_BLUE: [u8; 3] = [0x00, 0xC0, 0xFF];
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
const HEARING_CELLS: f32 = 20.0;
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
const PARTY_CHAT_OURS: [u8; 3] = [200, 200, 100];
const PARTY_CHAT: [u8; 3] = [230, 215, 200];
const PARTY_CREATE: usize = 2054;
/// What a locked chat room asks ("Please enter Password.").
const ROOM_PASSWORD: usize = 15;
const PARTY_INVITE: usize = 2056;
/// Items can be picked up from this far (closer means walking first).
const PICKUP_RANGE: f32 = 2.0;
/// A dropped item starts this many cells up and falls one cell per 40ms.
const DROP_HEIGHT: f32 = 5.0;
const DROP_MS_PER_CELL: f32 = 40.0;
/// Items cast a quarter of a character's shadow.
const ITEM_SHADOW: f32 = 0.25;

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
/// Words over a head stay this long.
const SPEECH_TIME: Duration = Duration::from_secs(5);
/// Emotions sit this many sprite pixels above the feet, over the head.
const EMOTION_HEIGHT: f32 = 70.0;
/// Spirit spheres and coins: how far from the body they circle, how high, how fast (radians a second), and how
/// much flatter the circle looks than it is.
// ponytail: estimates; calibrate against the real client.
const SPIRIT_RADIUS: f32 = 28.0;
const SPIRIT_HEIGHT: f32 = 45.0;
const SPIRIT_TURN: f32 = 1.5;
const SPIRIT_TILT: f32 = 0.4;
/// The sphere picture's ball is about 22px across in a 94px square.
const SPHERE_SCALE: f32 = 0.5;
/// Heal, Highness Heal and Coluceo Heal report how much they healed.
const HEAL_SKILLS: [u16; 3] = [28, 2043, 2044];
const CAST_BAR: [f32; 2] = [60.0, 6.0];
/// Status icons down the right side, 32px a row of 36, columns 45 apart.
const STATUS_TOP: f32 = 166.0;
const STATUS_RIGHT: f32 = 20.0;
const STATUS_ICON: f32 = 32.0;
const STATUS_STEP: [f32; 2] = [45.0, 36.0];
/// Steps of the clock drawn over a status icon.
const CLOCK_STEPS: usize = 32;
/// A skill still blocked: "There is a Delay after using a Skill.", and the words after a cooldown's seconds left.
const SKILL_DELAY: usize = 219;
/// The Talkie Box (a Hunter's trap holding a message), and the box asking for its message.
// ponytail: 436 "Please type a message for the Talkie Box"; 435 may be the official one.
const TALKIE_BOX: u16 = 125;
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
/// The Convex Mirror: no boss here, the boss will show on the minimap; its status icon (EFST_CASH_BOSS_ALARM).
const NO_BOSS: usize = 1223;
const BOSS_SHOWN: usize = 1225;
const BOSS_ALARM: u16 = 253;
const COOLDOWN_LEFT: usize = 3230;
/// The spark of a landed hit, and a warp portal's rings.
const EF_HIT1: u32 = 0;
const EF_WARPZONE2: u32 = 321;
/// The MVP's banner (mvp.str).
const EF_MVP: u32 = 68;
/// The cast aura by the skill's element: neutral, water, earth, fire, wind, poison, holy, shadow, ghost, undead.
const CAST_AURAS: [u32; 10] = [12, 54, 57, 55, 56, 59, 58, 454, 58, 454];
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
/// HP/SP bars under characters: border, background, HP, low HP (under a quarter), SP; and monsters' HP bars.
const BAR_WIDTH: f32 = 60.0;
const BAR_COLORS: [[u8; 4]; 5] = [[0x14, 0x19, 0x9B, 255], [0x44, 0x44, 0x44, 255], [0x16, 0xEA, 0x25, 255], [0xEA, 0x16, 0x16, 255], [0x1D, 0x65, 0xDB, 255]];
const MONSTER_BAR_COLORS: [[u8; 4]; 4] = [[0x10, 0x18, 0x9C, 255], [0x42, 0x41, 0x42, 255], [0xFF, 0x00, 0xE7, 255], [0xFF, 0xFF, 0x00, 255]];
/// Damage numbers rise and shrink over this long.
const DAMAGE_DURATION: f32 = 1.5;
const MISS_DURATION: f32 = 0.8;
/// A multi-hit's total stays this long; the hits come this far apart.
const COMBO_DURATION: f32 = 3.0;
const MULTI_HIT_DELAY: Duration = Duration::from_millis(200);
/// How long a dead unit stays on the ground.
const CORPSE_TIME: Duration = Duration::from_millis(1500);

/// Unit object types (rathena's clif_bl_type).
const PLAYER: u8 = 0;
const MONSTER: u8 = 5;
const NPC: u8 = 6;
const PET: u8 = 7;
const HOMUNCULUS: u8 = 8;
const MERCENARY: u8 = 9;
/// JT_HIDDEN_NPC, JT_HIDDEN_WARP_NPC: invisible script NPCs.
const HIDDEN_JOBS: [u16; 2] = [111, 139];
/// JT_WARPNPC: warp portals, drawn as an effect without a name.
const WARP_JOB: u16 = 45;

// The camera as tuned against the official client: zoom 125 (62.5 cells) and 50 degrees up by default.
// ponytail: the zoom-out limit (5x) is a guess; calibrate against the real client.
const ZOOM: f32 = 125.0;
const ZOOM_MIN: f32 = 65.0;
const ZOOM_MAX: f32 = 325.0;
const ZOOM_MAX_INDOOR: f32 = 162.5;
const ZOOM_STEP: f32 = 15.0;
/// World units per zoom unit.
const UNITS_PER_ZOOM: f32 = 2.5;
const PITCH: f32 = 50.0;
const PITCH_RANGE: [f32; 2] = [10.0, 90.0];
const PITCH_RANGE_INDOOR: [f32; 2] = [40.0, 60.0];
const YAW_RANGE: [f32; 2] = [-360.0, 360.0];
/// Indoors the camera keeps a fixed slant you can only adjust a little.
const YAW_INDOOR: f32 = 40.0;
const YAW_RANGE_INDOOR: [f32; 2] = [25.0, 60.0];
const CAMERA_DOUBLE_CLICK: Duration = Duration::from_millis(500);
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
    session: MapSession,
    world: World,
    map: String,
    me: Character,
    account_id: u32,
    player: Actor,
    units: HashMap<u32, Actor>,
    camera: Camera,
    camera_goal: CameraGoal,
    /// The map's overview picture, for the minimap.
    minimap: Option<Texture>,
    last_frame: Instant,
    chat: chat::Chat,
    entered: Instant,
    last_keepalive: Instant,
    /// Dev switch: a cell to walk to shortly after entering.
    auto_walk: Option<[u16; 2]>,
    auto_say: Option<String>,
    /// The blob under every character.
    shadow: Option<SpriteSet>,
    /// The emotion sprite, and the emotions showing: by unit (None = the player), the sprite action and when.
    emotion_sprite: Option<SpriteSet>,
    emotions: HashMap<Option<u32>, (usize, Instant)>,
    /// Spirit spheres or coins: by unit (None = the player), how many; the sphere's picture, the coin's sprite.
    spirits: HashMap<Option<u32>, u16>,
    sphere: Option<Texture>,
    coin_sprite: Option<SpriteSet>,
    digits: Option<Digits>,
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
    skills: Vec<net::Skill>,
    skill_window: skills::SkillWindow,
    /// A Talkie Box's message waiting to be placed, and the boxes saying theirs (where each stands).
    talkie_text: Option<String>,
    talkie_spots: HashMap<u32, world::Vector>,
    /// Skills blocked after use, and the line one still blocked prints when used.
    cooldowns: skills::Cooldowns,
    skill_delay_line: String,
    /// A skill waiting for its target: ID and level.
    targeting: Option<(u16, u16)>,
    /// A taming item waiting for its monster; the slot machine, and the result it plays (action, since when).
    capturing: bool,
    slot_sprite: Option<SpriteSet>,
    slot: Option<(usize, Instant)>,
    /// Units casting (None = the player): when it began and how long it takes.
    casts: HashMap<Option<u32>, (Instant, Duration)>,
    /// Words over heads (chat, skill names), by unit (None = the player): text, color, when said.
    speech: HashMap<Option<u32>, (String, [u8; 3], Instant)>,
    /// Status effects on the player with an icon: the client's EFST id, when it began and ends (None: no end).
    status_icons: Vec<(u16, Instant, Option<Instant>)>,
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
    /// The material window (Change Material, Four Spirit Analysis).
    materials: Option<material::Materials>,
    /// The Lucky Roulette's window.
    roulette: Option<roulette::Roulette>,
    /// Sense's window, and the map's boss as the Convex Mirror shows it (name and cell).
    monster_window: Option<monster_info::MonsterWindow>,
    boss: Option<(String, [i32; 2])>,
    viewing: Option<Viewed>,
    bank: Option<bank::Bank>,
    trade_asked: Option<(String, u32, u16)>,
    trade_with: String,
    /// The status window, and its height (its background's).
    status_window: status::StatusWindow,
    status_height: f32,
    /// The party, its window, and an invitation waiting for an answer (party ID and name).
    party: Option<party::Party>,
    party_window: party::PartyWindow,
    party_invite: Option<(u32, String)>,
    /// Setting up our shop, and our open shop.
    vend_setup: Option<vend::Setup>,
    my_shop: Option<vend::MyShop>,
    /// RODEX: the mailbox, a mail read and one written.
    mail: mail::Mail,
    /// The cart (shown like the inventory, once the server says there is one).
    cart: inventory::Inventory,
    /// The guild we're in, its window, and a guild inviting us (ID and name).
    guild: Option<guild::Guild>,
    guild_window: guild::GuildWindow,
    guild_invite: Option<(u32, String)>,
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
    mercenary_skills: Vec<net::Skill>,
    mercenary_window: mercenary::Window,
    mercenary_skill_list: mercenary::SkillList,
    steering: mercenary::Steering,
    dismiss_asked: bool,
    /// Each party member's minimap color.
    party_dots: HashMap<u32, Texture>,
    /// Chat rooms over their owners' heads (by owner), and where their signs were last drawn (room ID, password).
    signs: HashMap<u32, chatroom::Sign>,
    sign_rects: Vec<([f32; 4], u32, u8)>,
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
    /// Open windows from back to front.
    windows: Vec<Panel>,
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
    /// What to do once the walk toward it ends (and when it ended).
    pending: Option<(Pending, Option<Instant>)>,
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
    last_press: Option<(Instant, [f32; 2])>,
}

impl CameraGoal {
    fn new(indoor: bool) -> Self {
        let mut goal = Self { yaw: if indoor { YAW_INDOOR } else { 0.0 }, pitch: PITCH, zoom: ZOOM, indoor, dragging: None, last_press: None };
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
        Camera { target, distance: self.zoom * UNITS_PER_ZOOM, pitch: self.pitch.to_radians(), yaw: self.yaw.to_radians() }
    }

    /// Right button down; a quick second press resets the tilt (Shift), the zoom (Ctrl) or the turn.
    fn press(&mut self, cursor: [f32; 2], modifiers: ModifiersState) {
        let now = Instant::now();
        if let Some((at, place)) = self.last_press
            && now.duration_since(at) < CAMERA_DOUBLE_CLICK
            && (place[0] - cursor[0]).abs() < 10.0
            && (place[1] - cursor[1]).abs() < 10.0
        {
            if modifiers.shift_key() {
                self.pitch = PITCH;
            }
            if modifiers.control_key() {
                self.zoom = ZOOM;
            } else {
                self.yaw = if self.indoor { YAW_INDOOR } else { 0.0 };
            }
        }
        self.last_press = Some((now, cursor));
        self.dragging = Some(cursor);
    }

    /// Dragging turns the view; with Shift it tilts, with Ctrl it zooms.
    fn drag(&mut self, cursor: [f32; 2], modifiers: ModifiersState) {
        // Only while the right button is down: `replace` would start a drag on any move.
        let Some(last) = self.dragging else { return };
        self.dragging = Some(cursor);
        let [dx, dy] = [cursor[0] - last[0], cursor[1] - last[1]];
        if modifiers.shift_key() {
            let [low, high] = if self.indoor { PITCH_RANGE_INDOOR } else { PITCH_RANGE };
            self.pitch = (self.pitch + dy / SCREEN[1] * 300.0).clamp(low, high);
        } else if modifiers.control_key() {
            self.zoom = self.clamp_zoom(self.zoom - dy * ZOOM_STEP / 10.0);
        } else {
            let [low, high] = if self.indoor { YAW_RANGE_INDOOR } else { YAW_RANGE };
            self.yaw = (self.yaw + dx / SCREEN[0] * 720.0).clamp(low, high);
        }
    }

    fn wheel(&mut self, notches: f32) {
        self.zoom = self.clamp_zoom(self.zoom + notches * ZOOM_STEP);
    }

    fn clamp_zoom(&self, zoom: f32) -> f32 {
        zoom.clamp(ZOOM_MIN, if self.indoor { ZOOM_MAX_INDOOR } else { ZOOM_MAX })
    }

    /// Moves the camera part of the way to the goal; turning past half a circle wraps both around.
    fn ease(&mut self, camera: &mut Camera, elapsed: Duration) {
        let step = (elapsed.as_secs_f32() * 1000.0 * 0.006 * 2.0).min(1.0);
        let mut yaw = camera.yaw.to_degrees();
        if self.yaw > 180.0 && yaw > 180.0 {
            (self.yaw, yaw) = (self.yaw - 360.0, yaw - 360.0);
        } else if self.yaw < -180.0 && yaw < -180.0 {
            (self.yaw, yaw) = (self.yaw + 360.0, yaw + 360.0);
        }
        camera.yaw = (yaw + (self.yaw - yaw) * step).to_radians();
        camera.pitch += (self.pitch.to_radians() - camera.pitch) * step;
        camera.distance += (self.zoom * UNITS_PER_ZOOM - camera.distance) * step;
    }
}

/// The sprite direction to draw for a unit facing `direction`, seen through a camera turned `yaw` radians.
fn seen_direction(direction: u8, yaw: f32) -> u8 {
    (i32::from(direction) - (yaw.to_degrees() / 45.0).round() as i32).rem_euclid(8) as u8
}

/// A pet: its monster's sprite, moving by its accessory's ACT when it wears one the data has.
fn pet_sprites(grf: &Grf, name: &str, act: Option<&String>) -> Option<SpriteSet> {
    let worn = act.and_then(|act| SpriteSet::with_act(grf, &format!("data\\sprite\\몬스터\\{name}"), &format!("data\\sprite\\몬스터\\{act}")));
    worn.or_else(|| SpriteSet::single(grf, name, true))
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
/// An NPC's mark on the minimap: (NPC, mark ID), cell, image, and until when (None while the NPC keeps it).
type Mark = ((u32, u8), [i32; 2], Texture, Option<Instant>);

/// A scripted input (RUSTNAROK_CLIENT_CLICKS): when, right button, where, and where to drag to.
type ScriptedInput = (Duration, bool, [f32; 2], Option<[f32; 2]>);

fn map_key(map: &str) -> String {
    map.trim_end_matches(".gat").trim_end_matches(".rsw").to_lowercase()
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
}

/// What the box asking how many is for.
#[derive(Clone, Copy)]
enum Asked {
    /// A row of the shop, into the deal.
    Shop(usize),
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
    /// A Talkie Box's message, before it's placed at the cell.
    TalkieBox {
        level: u16,
        cell: [i32; 2],
    },
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
    /// See what they wear.
    ViewEquipment(u32),
    /// A command to the player's pet (net::PET_INFO and on).
    Pet(u8),
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
    Attack(u32),
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

    fn attack(&mut self, target: u32) {
        if let Some(goal) = self.units.get(&target).map(|actor| actor.cell) {
            self.attack_from(target, self.player.cell, goal);
        }
    }

    /// Attacks `target`, walking into reach first: the server only swings from within range.
    fn attack_from(&mut self, target: u32, from: [i32; 2], goal: [i32; 2]) {
        if world::in_reach(from, goal, self.attack_range) {
            self.pending = None;
            let _ = self.session.act(target, net::ACT_ATTACK_REPEAT);
        } else if let Some(stop) =
            world::find_path(&self.world.gat, from, goal).and_then(|path| path.into_iter().find(|&cell| world::in_reach(cell, goal, self.attack_range)))
        {
            self.pending = Some((Pending::Attack(target), None));
            let _ = self.session.walk(stop.map(|value| value as u16));
        }
    }

    /// Picks up an item, walking over first when it's out of reach.
    fn pick_up(&mut self, id: u32) {
        let Some(item) = self.ground.get(&id) else { return };
        let at = item.at;
        self.player.face(at);
        let [dx, dy] = [(at[0] - self.player.cell[0]) as f32, (at[1] - self.player.cell[1]) as f32];
        if dx.hypot(dy) > PICKUP_RANGE {
            self.pending = Some((Pending::PickUp(id), None));
            let _ = self.session.walk(at.map(|value| value as u16));
        } else {
            self.pending = None;
            let _ = self.session.pick_up(id);
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
        })
    }

    fn skill_points(&self) -> i64 {
        self.status.get(&net::STATUS_SKILL_POINT).copied().unwrap_or(0)
    }

    /// Starts using a skill: skills on oneself go off at once, the others wait for a target.
    fn use_skill(&mut self, id: u16, level: u16) {
        // ponytail: a blocked skill says so and sends nothing (the server drops it without a word unless @showdelay is
        // on); whether the real client asks the server anyway isn't known.
        if self.cooldowns.left(id, control::now()).is_some() {
            self.chat_line(self.skill_delay_line.clone(), CHAT_ERROR);
            return;
        }
        let Some(skill) = self.skills.iter().find(|skill| skill.id == id) else { return self.use_mercenary_skill(id, level) };
        let level = level.clamp(1, skill.level.max(1));
        if skill.kind & net::Skill::SELF != 0 {
            let _ = self.session.use_skill(id, level, self.account_id);
        } else if skill.kind & (net::Skill::ENEMY | net::Skill::GROUND | net::Skill::FRIEND) != 0 {
            self.targeting = Some((id, level));
        }
    }

    /// An item casts a skill: on oneself at once, otherwise aimed as a known one would be.
    fn cast_item_skill(&mut self, skill: net::Skill) {
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
        // The mercenary casts its own skills from where it stands; out of its range the server does nothing.
        if self.mercenary_skills.iter().any(|skill| skill.id == id) {
            let _ = match target {
                SkillTarget::Unit(unit) => self.session.use_skill(id, level, unit),
                SkillTarget::Cell(cell) => self.session.use_skill_at(id, level, cell.map(|value| value.max(0) as u16)),
            };
            return;
        }
        let goal = match target {
            SkillTarget::Unit(unit) if unit == self.account_id => Some(self.player.cell),
            SkillTarget::Unit(unit) => self.units.get(&unit).map(|actor| actor.cell),
            SkillTarget::Cell(cell) => Some(cell),
        };
        let Some(goal) = goal else { return };
        let range = self.skills.iter().chain(&self.item_skill).find(|skill| skill.id == id).map_or(1, |skill| i32::from(skill.range).max(1));
        if world::in_reach(self.player.cell, goal, range) {
            self.pending = None;
            let _ = match target {
                SkillTarget::Unit(unit) => self.session.use_skill(id, level, unit),
                SkillTarget::Cell(cell) => self.send_skill_at(id, level, cell.map(|value| value as u16)),
            };
        } else if let Some(stop) =
            world::find_path(&self.world.gat, self.player.cell, goal).and_then(|path| path.into_iter().find(|&cell| world::in_reach(cell, goal, range)))
        {
            self.pending = Some((Pending::Skill { id, level, target }, None));
            let _ = self.session.walk(stop.map(|value| value as u16));
        }
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
            self.whispered.push_back((to, text.to_owned()));
        }
        None
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
    }

    /// Opens the macro window, or closes it keeping what was typed.
    fn toggle_macro_window(&mut self) {
        if self.macro_window.take().is_none() {
            self.macro_window = Some(macros::Window::default());
        } else {
            macros::save(macros::path().as_deref(), &self.macro_lines);
        }
    }

    /// Whether the pointer is over the interface (a window, the chat, a dialog) rather than the world.
    fn on_ui(&self, cursor: [f32; 2], items: &HashMap<u32, ItemInfo>) -> bool {
        self.window_at(cursor).is_some()
            || self.dialog.is_some()
            || self.amount.is_some()
            || self.escape.is_some()
            || self.world_map.is_some()
            || self.audio_window.as_ref().is_some_and(|window| window.hit(cursor).is_some())
            || self.item_window.as_ref().is_some_and(|window| window.hit(cursor, items).is_some())
            || self.menu.as_ref().is_some_and(|menu| menu.hit(cursor).is_some())
            || self.trade.as_ref().is_some_and(|trade| trade.hit(cursor).is_some())
            || self.room.as_ref().is_some_and(|room| room.hit(cursor).is_some())
            || self.room_make.as_ref().is_some_and(|make| make.hit(cursor).is_some())
            || self.vend_setup.as_ref().is_some_and(|setup| setup.hit(cursor, &self.cart.items).is_some())
            || self.my_shop.as_ref().is_some_and(|shop| shop.hit(cursor).is_some())
            || self.mail.hit(cursor).is_some()
            || self.quest_window.hit(cursor, &self.quests).is_some()
            || self.achievement_window.covers(cursor)
            || self.roulette.as_ref().is_some_and(|window| window.hit(cursor).is_some())
            || roulette::icon_hit(cursor)
            || self.monster_window.as_ref().is_some_and(|window| window.hit(cursor).is_some())
            || self.chat.contains(cursor)
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
            Asked::Shop(_) | Asked::PartyName | Asked::Invite | Asked::RoomPassword(_) | Asked::VendPrice(_) | Asked::PetName | Asked::TalkieBox { .. } => None,
        };
        match amount {
            Some(1) => self.give(asked, 1),
            Some(amount) => self.amount = Some(window::Amount::new(asked, Some(u32::from(amount)))),
            None => {}
        }
    }

    /// The box asking how many (or for a name) is answered.
    fn answer_amount(&mut self, ok: bool) {
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
            Asked::PartyName | Asked::Invite | Asked::PetName => Ok(()),
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
            Asked::Store(index) => {
                let have = self.inventory.get(index).map_or(0, |item| u32::from(item.amount));
                self.session.put_in_storage(index, amount.min(have))
            }
            Asked::Take(index) => {
                let have = self.storage.get(index).map_or(0, |item| u32::from(item.amount));
                self.session.take_from_storage(index, amount.min(have))
            }
            Asked::Drop(index) => {
                let have = self.inventory.get(index).map_or(0, |item| item.amount);
                self.session.drop_item(index, (amount.min(u32::from(have))) as u16)
            }
            Asked::PartyName | Asked::Invite | Asked::RoomPassword(_) | Asked::PetName | Asked::TalkieBox { .. } => Ok(()),
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
    fn offer(&mut self, index: u16) -> Option<usize> {
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
        self.move_item(Asked::Trade(index));
        None
    }

    /// The room maker's OK: opens the room (the window stays until the server says).
    fn make_room(&mut self) {
        if let Some(make) = &self.room_make
            && !make.title.trim().is_empty()
        {
            let _ = self.session.make_chat_room(make.title.trim(), make.limit(), make.public, &make.password);
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

    /// Words over a unit's head (None = the player).
    fn say(&mut self, unit: Option<u32>, text: String, color: [u8; 3]) {
        self.speech.insert(unit, (text, color, Instant::now()));
    }

    /// A unit starts or uses a skill: players strike the skill pose, and units call its name (see `overhead::shouts`).
    fn skill_motion(&mut self, source: u32, skill: u16, level: i32, packet: overhead::Shout, duration: f32, info: &HashMap<u16, SkillInfo>) {
        let me = self.account_id;
        let Some(actor) = (if source == me { Some(&mut self.player) } else { self.units.get_mut(&source) }) else { return };
        if actor.remove_at.is_some() {
            return;
        }
        let monster = actor.kind == MONSTER;
        if actor.kind == PLAYER {
            actor.play(SKILL_ACTION, Instant::now(), duration.max(100.0), false);
            actor.stand_ready();
        }
        if overhead::shouts(skill, level, monster, packet)
            && let Some(info) = info.get(&skill)
        {
            let unit = (source != me).then_some(source);
            self.say(unit, format!("{} !!", info.name), CHAT_WHITE);
        }
    }

    /// Puts a window in front of the others.
    fn raise(&mut self, id: Panel) {
        self.windows.retain(|&window| window != id);
        self.windows.push(id);
    }

    /// Uses an inventory item: consumables are used, equipment is put on.
    fn use_item(&mut self, index: u16) {
        let Some(item) = self.inventory.get(index) else { return };
        // ponytail: a voucher asks to grow the inventory; how the real client lets the player ask isn't known.
        if VOUCHERS.contains(&item.id) {
            let _ = self.session.ask_expansion();
            return;
        }
        match item.kind {
            net::ITEM_HEALING | net::ITEM_USABLE | net::ITEM_CASH => {
                let _ = self.session.use_item(index, self.account_id);
            }
            // An egg (armor worn nowhere) hatches from the egg list only.
            net::ITEM_ARMOR if item.location == 0 => {}
            // An item that binds once worn asks first.
            net::ITEM_WEAPON | net::ITEM_ARMOR | net::ITEM_SHADOW_GEAR if item.identified && item.bind == 1 => self.bind_asked = Some(index),
            net::ITEM_WEAPON | net::ITEM_ARMOR | net::ITEM_SHADOW_GEAR | net::ITEM_PET_ARMOR | net::ITEM_AMMO if item.identified => {
                let _ = self.session.wear(index, item.location);
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
        };
    }

    /// Answers the bind question: OK wears the item.
    fn answer_bind(&mut self, yes: bool) {
        let Some(index) = self.bind_asked.take().filter(|_| yes) else { return };
        if let Some(location) = self.inventory.get(index).map(|item| item.location) {
            let _ = self.session.wear(index, location);
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

    fn chat_line(&mut self, text: String, color: [u8; 3]) {
        self.chat.push(text, color);
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
        if let Some(source) = source {
            if let Some(cell) = target_cell {
                source.face(cell);
            }
            let attack = match &source.look {
                Some(look) => actor::attack_action(look.job, look.sex, tables.weapon_type(look.weapon)),
                None => actor::combat_actions(source.kind).0,
            };
            source.play(attack, Instant::now(), act.source_speed.max(100) as f32, false);
            source.stand_ready();
        }
        self.show_hit(&act);
    }

    /// The numbers of a hit as it lands: split over a multi-hit's hits (with the running total over monsters),
    /// yellow for a critical, "Lucky" for a lucky dodge.
    fn show_hit(&mut self, act: &net::Act) {
        let me = self.account_id;
        let target = (act.target != me).then_some(act.target);
        let on_player = act.target == me || target.and_then(|id| self.units.get(&id)).is_some_and(|actor| actor.kind == PLAYER);
        let amount = act.damage + act.left_damage;
        let lands = Instant::now() + Duration::from_millis(act.source_speed.max(0) as u64);
        // Misses, endured hits and lucky dodges don't flinch.
        let flinch = (amount > 0 && !matches!(act.kind, 4 | 9 | 11)).then_some(act.target_speed.max(100) as f32);
        let kind = match act.kind {
            11 => DamageKind::Lucky,
            10 | 13 => DamageKind::Critical,
            _ => DamageKind::Hit { taken: on_player },
        };
        let hits = if amount > 0 { i32::from(act.hits.max(1)) } else { 1 };
        for hit in 0..hits {
            let start = lands + MULTI_HIT_DELAY * hit as u32;
            self.damages.push(Damage { target, amount: amount / hits, start, kind, flinch });
            if hits > 1 && !on_player {
                let combo = DamageKind::Combo { last: hit + 1 == hits };
                self.damages.push(Damage { target, amount: amount / hits * (hit + 1), start, kind: combo, flinch: None });
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
    amount: i32,
    start: Instant,
    kind: DamageKind,
    flinch: Option<f32>,
}

/// How a number looks and moves.
#[derive(Clone, Copy, PartialEq)]
enum DamageKind {
    /// White, or red on a player (`taken`).
    Hit { taken: bool },
    /// Yellow on the critical burst.
    Critical,
    /// "Lucky": the hit was dodged.
    Lucky,
    /// Green HP or blue SP, rising slowly.
    Heal { sp: bool },
    /// The running total of a multi-hit, high above in yellow; the last one stays longer.
    Combo { last: bool },
}

impl DamageKind {
    fn duration(self, amount: i32) -> f32 {
        match self {
            DamageKind::Lucky => MISS_DURATION,
            DamageKind::Hit { .. } if amount <= 0 => MISS_DURATION,
            DamageKind::Combo { .. } => COMBO_DURATION,
            _ => DAMAGE_DURATION,
        }
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
    skills: skills::Assets,
    shop: shop::Assets,
    pick_list: pick_list::Assets,
    storage: storage::Assets,
    trade: trade::Assets,
    refine: refine::Assets,
    bank: bank::Assets,
    context_menu: menu::Assets,
    weather: weather::Assets,
    mercenary: mercenary::Assets,
    party: party::Assets,
    chatroom: chatroom::Assets,
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
    speech_fill: Texture,
    speech_border: Texture,
    target_fill: Texture,
    clocks: [Vec<Texture>; 2],
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
    bars: [Texture; 5],
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
            skills: skills::Assets::load(gpu, grf),
            shop: shop::Assets::load(gpu, grf),
            pick_list: pick_list::Assets::load(gpu, grf),
            storage: storage::Assets::load(gpu, grf),
            trade: trade::Assets::load(gpu, grf),
            refine: refine::Assets::load(gpu, grf),
            bank: bank::Assets::load(gpu, grf),
            context_menu: menu::Assets::load(gpu),
            weather: weather::Assets::load(gpu, grf),
            mercenary: mercenary::Assets::load(gpu, grf),
            party: party::Assets::load(gpu, grf),
            chatroom: chatroom::Assets::load(gpu, grf),
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
            cast_fill: gpu.solid([0x00, 0xFF, 0x00, 0xFF]),
            speech_fill: gpu.solid([0, 0, 0, 102]),
            speech_border: gpu.solid([0x5C, 0x5C, 0x5C, 0xFF]),
            target_fill: gpu.solid([0, 0, 0, 128]),
            clocks: [[255, 255, 255, 166], [255, 150, 50, 166]]
                .map(|color| (0..=CLOCK_STEPS).map(|step| gpu.texture(&clock(step as f32 / CLOCK_STEPS as f32, color), false)).collect()),
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
    [((SCREEN[0] - window.size[0]) / 2.0).floor(), LOGIN_TOP]
}

/// Where the player's settings file `name` is kept: Application Support on a Mac, %APPDATA% on Windows, the XDG config
/// folder elsewhere.
fn settings_path(name: &str) -> Option<std::path::PathBuf> {
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

fn dialog_position(dialog: &Texture) -> [f32; 2] {
    [((SCREEN[0] - dialog.size[0]) / 2.0).floor(), ((SCREEN[1] - dialog.size[1]) / 1.5).floor()]
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
    [(SCREEN[0] / 3.0).max(20.0).floor(), (SCREEN[1] / 2.0 - 200.0).max(100.0).floor()]
}

fn menu_position() -> [f32; 2] {
    [(SCREEN[0] / 3.0).max(20.0).floor(), (SCREEN[1] / 2.0 + 76.0).max(376.0).floor()]
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
    const LINKS: [(&str, &str); 4] = [("NAVI", "C00000"), ("ITEMLINK", "0070C0"), ("ITEM", "0070C0"), ("URL", "0070C0")];
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
    let mut lines = vec![Vec::new()];
    let mut line_width = 0.0;
    let mut color = [0, 0, 0];
    let mut rest = text;
    while !rest.is_empty() {
        if let Some(code) =
            rest.strip_prefix('^').filter(|code| code.len() >= 6 && code.is_char_boundary(6) && code[..6].chars().all(|digit| digit.is_ascii_hexdigit()))
        {
            let value = u32::from_str_radix(&code[..6], 16).unwrap();
            color = [(value >> 16) as u8, (value >> 8) as u8, value as u8];
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
        lines.last_mut().unwrap().push((color, word.to_owned()));
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
    /// Pets' tables and what they say.
    pets: pet::Data,
    /// The achievements (the translation's achievements.lub) and the titles they give, by ID.
    achievement_info: HashMap<u32, lua::AchievementInfo>,
    title_names: HashMap<u32, String>,
    /// Item names and resources by item ID, and the client's numbered messages.
    items: HashMap<u32, ItemInfo>,
    messages: Vec<String>,
    /// Item icons by file, loaded when first shown.
    icons: HashMap<String, Option<Texture>>,
    modifiers: ModifiersState,
    /// Maps with the indoor camera.
    indoor: HashSet<String>,
    look_tables: LookTables,
    /// Job names shown to players.
    pc_job_names: HashMap<u16, String>,
    /// Maps' display names, and the world maps with their box colors.
    map_names: HashMap<String, String>,
    worlds: (Vec<rustnarok_client::lua::World>, [[u8; 4]; 2]),
    /// Skill names and icons, the skill window's layout, and status icon files (the client's Lua tables).
    skill_info: HashMap<u16, SkillInfo>,
    skill_trees: SkillTrees,
    status_icon_files: HashMap<u16, (u8, String)>,
    /// Which effects things play.
    effect_table: effects::Table,
    /// Quests' titles, descriptions and summaries.
    quest_texts: HashMap<u32, quest::QuestText>,
    /// Sound out, the player's sound settings, and the music each map plays.
    audio: Option<sound::Audio>,
    sound: sound::Settings,
    music: HashMap<String, String>,
    /// /effect: effects are drawn and heard; off, they still play, unseen and silent. Kept until the client quits.
    effects_on: bool,
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

    /// The syllable the input method is composing, shown where text is typed.
    fn compose(&mut self, text: String) {
        match &mut self.scene {
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
                None if game.room_make.is_some() => game.room_make.as_mut().unwrap().type_text(text),
                None if game.macro_window.as_ref().is_some_and(|window| window.editing.is_some()) => {
                    game.macro_window.as_ref().unwrap().type_text(&mut game.macro_lines, text)
                }
                None if game.vend_setup.as_ref().is_some_and(|setup| setup.typing) => game.vend_setup.as_mut().unwrap().type_text(text),
                None if game.mail.typing() => game.mail.type_text(text),
                None if game.room.is_some() => game.room.as_mut().unwrap().input.push_str(text),
                None => game.chat.typing().extend(typed),
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
                let action = if select.menu.is_some() { select.click_menu(cursor) } else { select.hit(cursor).and_then(|hit| select.click(hit)) };
                if let Some(action) = action {
                    self.select_action(action);
                }
            }
            Scene::CharMake(make, _) => {
                if let Some(action) = make.hit(cursor).and_then(|hit| make.click(hit, &self.grf, &self.look_tables)) {
                    self.make_action(action);
                }
            }
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
            // The option menu keeps the pointer until it's closed.
            Scene::Game(game) if game.escape.is_some() => match game.escape.as_ref().unwrap().hit(cursor).flatten() {
                Some(escape::Choice::SavePoint) => {
                    let _ = game.session.restart(false);
                }
                Some(escape::Choice::CharSelect) => {
                    let _ = game.session.restart(true);
                }
                Some(escape::Choice::Exit) => {
                    let _ = game.session.quit();
                }
                Some(escape::Choice::Cancel) => game.escape = None,
                Some(escape::Choice::Sound) => {
                    game.escape = None;
                    game.audio_window = Some(options::AudioWindow::new());
                }
                // ponytail: the graphics and hotkey settings windows aren't made yet.
                Some(escape::Choice::Graphics | escape::Choice::Hotkeys) | None => {}
            },
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
            // The quest window: tabs, quests, their locks, and the picked one's details.
            Scene::Game(game) if game.achievement_window.covers(cursor) => {
                let hit = game.achievement_window.hit(cursor, &self.achievement_info, &game.achievements).unwrap();
                if let Some(id) = game.achievement_window.click(hit) {
                    let _ = game.session.claim_achievement(id);
                }
            }
            Scene::Game(game) if game.quest_window.hit(cursor, &game.quests).is_some() => {
                let hit = game.quest_window.hit(cursor, &game.quests).unwrap();
                if let Some((id, active)) = game.quest_window.click(hit, &game.quests) {
                    let _ = game.session.set_quest_active(id, active);
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
            // The item description window: its close button.
            Scene::Game(game) if game.item_window.as_ref().is_some_and(|window| window.hit(cursor, &self.items).is_some()) => {
                if game.item_window.as_ref().unwrap().hit(cursor, &self.items) == Some(iteminfo::Hit::Close) {
                    game.item_window = None;
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
                Some(Choice::GuildInvite(id)) => {
                    if let Some(actor) = game.units.get(&id) {
                        let _ = game.session.invite_to_guild(&actor.name.clone());
                    }
                }
                // Feeding asks first; the status opens the pet window.
                Some(Choice::Pet(net::PET_FEED)) => game.pet_asked = Some(PetAsked::Feed),
                Some(Choice::Pet(net::PET_INFO)) => game.open_pet_window(),
                Some(Choice::Pet(command)) => game.pet_command(command),
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
            Scene::Game(game) if game.room.as_ref().is_some_and(|room| room.hit(cursor).is_some()) => {
                let room = game.room.as_mut().unwrap();
                match room.hit(cursor) {
                    Some(chatroom::RoomHit::Title(TitleHit::Close)) => {
                        let _ = game.session.leave_chat_room();
                    }
                    Some(chatroom::RoomHit::Title(title)) => room.frame.click_title(title),
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
            // Dismissing the mercenary: OK lets it go.
            Scene::Game(game) if game.dismiss_asked && contains(dialog_position(&assets.message), assets.message.size, cursor) => {
                let dialog = dialog_position(&assets.message);
                for (place, yes) in [(LEFT_BUTTON, true), (RIGHT_BUTTON, false)] {
                    if hit(offset(dialog, place), if yes { &assets.ok } else { &assets.cancel }) {
                        game.answer_dismiss(yes, &self.messages);
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
                    game.answer_amount(ok);
                }
            }
            Scene::Game(game) if game.pick.as_ref().is_some_and(|(list, _)| list.covers(cursor)) => {
                let (list, _) = game.pick.as_mut().unwrap();
                let hit = list.hit(cursor).unwrap();
                if let Some(answer) = list.click(hit) {
                    game.answer_pick(answer);
                }
            }
            Scene::Game(game) if game.shop.as_ref().is_some_and(|shop| shop.covers(cursor)) => {
                let shop = game.shop.as_mut().unwrap();
                let hit = shop.hit(cursor).unwrap();
                match shop.click(hit) {
                    Some(shop::Action::Deal) => {
                        let _ = shop.send(&mut game.session);
                    }
                    Some(shop::Action::Close) => game.close_shop(),
                    Some(shop::Action::Ask(row)) => game.amount = Some(window::Amount::new(Asked::Shop(row), None)),
                    None => {}
                }
            }
            Scene::Game(game) if game.window_at(cursor).is_some() => {
                let id = game.window_at(cursor).unwrap();
                game.raise(id);
                match id {
                    Panel::Hotkeys => {
                        let hit = game.hotkeys.hit(cursor).unwrap();
                        game.hotkeys.click(hit);
                        if let hotkeys::Hit::Slot(slot) = hit {
                            game.press_hotkey(slot);
                        }
                    }
                    Panel::BasicInfo => {
                        let hit = game.basic_info.hit(cursor).unwrap();
                        match game.basic_info.click(hit).map(|button| basic_info::MENU[button]) {
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
                        if let Some(index) = game.inventory.click(hit) {
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
                        if let Some(index) = game.equipment.click(hit) {
                            let _ = game.session.take_off(index);
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
                    Panel::Pet => match game.pet.as_ref().and_then(|pet| game.pet_window.hit(cursor, pet)) {
                        Some(pet::Hit::Title(title)) => game.pet_window.frame.click_title(title),
                        Some(pet::Hit::Rename) => game.amount = Some(window::Amount::name(Asked::PetName, PET_NAME_RULE)),
                        Some(pet::Hit::Feed) => game.pet_asked = Some(PetAsked::Feed),
                        _ => {}
                    },
                    Panel::Status => {
                        let hit = game.status_window.hit(cursor, &assets.status, &game.status);
                        if let Some(stat) = hit.and_then(|hit| game.status_window.click(hit)) {
                            let _ = game.session.raise_stat(stat);
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
                        _ => {}
                    },
                    Panel::Storage => {
                        let hit = game.storage.hit(cursor, &assets.storage, &self.items);
                        if hit.is_some_and(|hit| game.storage.click(hit)) {
                            let _ = game.session.close_storage();
                        }
                    }
                    Panel::Skills => {
                        let jobs = self.skill_trees.lineage(game.me.job);
                        let points = game.skill_points();
                        let hit = game.skill_window.hit(cursor, &self.skill_trees, &jobs, &game.skills, points);
                        match hit.and_then(|hit| game.skill_window.click(hit)) {
                            Some(skills::Action::Use(id)) => {
                                let learned = game.skills.iter().find(|skill| skill.id == id).map_or(0, |skill| skill.level);
                                let level = if learned > 0 { game.skill_window.use_level(id, learned) } else { 0 };
                                if level > 0 {
                                    game.use_skill(id, level);
                                }
                            }
                            Some(skills::Action::LevelUp(id)) => {
                                let _ = game.session.skill_up(id);
                            }
                            None => {}
                        }
                    }
                }
            }
            Scene::Game(game) if game.chat.contains(cursor) => game.chat.click(cursor),
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
            Scene::Game(game) if game.targeting.is_some() => {
                // An aimed skill goes to the unit under the pointer, or the cell for ground skills.
                let (id, _) = game.targeting.unwrap();
                let ground = game.skills.iter().find(|skill| skill.id == id).is_some_and(|skill| skill.kind & net::Skill::GROUND != 0);
                let unit = match hovered_unit(&game.hit_boxes, cursor, game.players_first(shift)) {
                    Some(Owner::Unit(unit)) => Some((unit, game.units.get(&unit).map(|actor| actor.cell))),
                    Some(Owner::Player) => Some((game.account_id, Some(game.player.cell))),
                    _ => None,
                };
                let height = game.world.cell_position(game.player.cell.map(|value| value as f32))[1];
                let view = game.camera.view(SCREEN[0] / SCREEN[1]);
                let cell = view
                    .pick([cursor[0] / SCREEN[0], cursor[1] / SCREEN[1]], height)
                    .map(|point| [(point[0] / world::CELL).floor() as i32, (point[2] / world::CELL).floor() as i32]);
                match (ground, unit, cell) {
                    (true, Some((_, Some(at))), _) => game.aim_skill(SkillTarget::Cell(at)),
                    (true, _, Some(cell)) => game.aim_skill(SkillTarget::Cell(cell)),
                    (false, Some((unit, _)), _) => game.aim_skill(SkillTarget::Unit(unit)),
                    // Clicking nothing keeps aiming.
                    _ => {}
                }
            }
            Scene::Game(game) => {
                // Whatever is under the cursor (see `picking::pick`): talk to NPCs, attack monsters, pick items up.
                match hovered(&game.hit_boxes, cursor, shift) {
                    Some(Owner::Unit(id)) => match game.units.get(&id).map(|actor| actor.kind) {
                        Some(NPC) => {
                            let _ = game.session.talk(id);
                            return;
                        }
                        Some(MONSTER) => {
                            game.attack(id);
                            return;
                        }
                        _ => {}
                    },
                    Some(Owner::Item(id)) => {
                        game.pick_up(id);
                        return;
                    }
                    _ => {}
                }
                game.pending = None;
                // Walk to the clicked cell: pick the ground plane at the player's height.
                let height = game.world.cell_position(game.player.cell.map(|value| value as f32))[1];
                let view = game.camera.view(SCREEN[0] / SCREEN[1]);
                if let Some(point) = view.pick([cursor[0] / SCREEN[0], cursor[1] / SCREEN[1]], height) {
                    let cell = [(point[0] / world::CELL).floor() as i32, (point[2] / world::CELL).floor() as i32];
                    if game.world.gat.walkable(cell) {
                        let _ = game.session.walk(cell.map(|value| value as u16));
                    }
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
            let (inventory_height, equipment_height, info_height) = (game.inventory.height(), game.equipment.height(), game.basic_info.height());
            game.basic_info.frame.drag_to(self.cursor, info_height);
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
            let party_height = game.party_window.height(game.party.as_ref(), &game.friends);
            game.party_window.frame.drag_to(self.cursor, party_height);
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
            if let Some(window) = &mut game.item_window {
                window.drag_to(self.cursor);
            }
            if let Some(room) = &mut game.room {
                let height = room.height();
                room.frame.drag_to(self.cursor, height);
            }
            game.mail.drag_to(self.cursor);
            game.quest_window.frame.drag_to(self.cursor, 466.0);
            game.achievement_window.frame.drag_to(self.cursor, achievement::HEIGHT);
            if let Some(window) = &mut game.roulette {
                window.frame.drag_to(self.cursor, roulette::HEIGHT);
            }
            if let Some(window) = &mut game.monster_window {
                window.frame.drag_to(self.cursor, monster_info::MonsterWindow::height());
            }
            if let Some(window) = &mut game.audio_window
                && window.drag_to(self.cursor, &mut self.sound)
                && let Some(audio) = &mut self.audio
            {
                audio.set(&self.sound);
            }
            game.camera_goal.drag(self.cursor, self.modifiers);
            if let Some((_, start, moved)) = &mut game.dragging {
                *moved |= (self.cursor[0] - start[0]).abs() + (self.cursor[1] - start[1]).abs() > 4.0;
            }
        }
    }

    /// The right button: Alt moves stacks to storage, a click on a player opens their menu, a press aims the camera.
    fn right_button(&mut self, state: ElementState) {
        // ponytail: Ctrl + right click (not a drag, not on a window) orders the mercenary, an estimate of the official
        // client's: at a monster it attacks, on the ground it goes there.
        if let Scene::Game(game) = &mut self.scene
            && state == ElementState::Released
            && self.modifiers.control_key()
            && let Some(id) = game.mercenary.as_ref().map(|hired| hired.id)
            && let Some((start, _)) = game.right_press
            && (start[0] - self.cursor[0]).abs() + (start[1] - self.cursor[1]).abs() < 4.0
            && game.window_at(self.cursor).is_none()
            && !game.chat.contains(self.cursor)
        {
            game.right_press = None;
            let monster = match hovered(&game.hit_boxes, self.cursor, false) {
                Some(Owner::Unit(target)) if game.units.get(&target).is_some_and(|actor| actor.kind == MONSTER) => Some(target),
                _ => None,
            };
            if let Some(target) = monster {
                // ponytail: rathena reads non-zero as "keep attacking" and says the client sends 0; 1 keeps it at it.
                let _ = game.session.act_npc(id, target, 1);
                game.steering.order = Some(mercenary::Order::Attack(target));
                return;
            }
            let height = game.world.cell_position(game.player.cell.map(|value| value as f32))[1];
            let view = game.camera.view(SCREEN[0] / SCREEN[1]);
            if let Some(point) = view.pick([self.cursor[0] / SCREEN[0], self.cursor[1] / SCREEN[1]], height) {
                let cell = [(point[0] / world::CELL).floor() as i32, (point[2] / world::CELL).floor() as i32];
                if game.world.gat.walkable(cell) {
                    let _ = game.session.move_npc(id, cell.map(|value| value as u16));
                    game.steering.order = Some(mercenary::Order::Move(game.player.cell));
                }
            }
            return;
        }
        // On the character select, a card's menu of moving and renaming.
        if let Scene::CharSelect(select) = &mut self.scene
            && state == ElementState::Pressed
        {
            select.open_menu(self.cursor, &self.font, &self.messages);
        }
        if let Scene::Game(game) = &mut self.scene {
            let assets = self.assets.as_ref();
            // Alt + right click moves a whole stack between the inventory and the open storage.
            let moved = match game.window_at(self.cursor) {
                _ if state != ElementState::Pressed || !self.modifiers.alt_key() || !game.storage.frame.open => None,
                Some(Panel::Inventory) => match game.inventory.hit(self.cursor) {
                    Some(inventory::Hit::Item(index)) => game.inventory.get(index).map(|item| (Asked::Store(index), item.amount)),
                    _ => None,
                },
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
                _ if game.window_at(self.cursor).is_some() || game.chat.contains(self.cursor) => None,
                Some(Owner::Unit(id)) => Some(id),
                _ => None,
            };
            if state == ElementState::Pressed {
                game.menu = None;
                game.right_press = Some((self.cursor, on_unit));
            } else if let Some((start, Some(id))) = game.right_press.take()
                && (start[0] - self.cursor[0]).abs() + (start[1] - self.cursor[1]).abs() < 4.0
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
                    // The guild's master can invite.
                    if game.guild.as_ref().is_some_and(|guild| guild.master && guild.members.iter().all(|member| member.account != id)) {
                        choices.push(Some((fill(GUILD_CHOICE), Choice::GuildInvite(id))));
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
                    // ponytail: last, under a line; where the real client lists it isn't known.
                    choices.extend([None, Some((fill(VIEW_CHOICE), Choice::ViewEquipment(id)))]);
                }
                game.menu = Some(menu::Menu::new(self.cursor, choices, &self.font));
            }
            // A right click on an item (inventory, storage or what's worn) describes it.
            let described = match game.window_at(self.cursor) {
                _ if state != ElementState::Pressed || self.modifiers.alt_key() => None,
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
                let shown =
                    iteminfo::Shown { item: item.id, identified: item.identified, refine: item.refine, cards: item.cards, damaged, expires: item.expires };
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
                game.camera_goal.press(self.cursor, self.modifiers);
            } else {
                game.camera_goal.dragging = None;
            }
        }
    }

    /// The left button went down or up: a press grabs, a release drops or clicks.
    /// The wheel turned `rows` notches (positive: toward the player): it scrolls what the pointer is over, or zooms.
    fn wheel(&mut self, rows: i32) {
        if let Scene::Game(game) = &mut self.scene {
            // Over the inventory the wheel scrolls it; elsewhere it zooms (away from you zooms out).
            let on_shop = game.shop.as_ref().is_some_and(|shop| shop.covers(self.cursor));
            let on_pick = game.pick.as_ref().is_some_and(|(list, _)| list.covers(self.cursor));
            let on_item = game.item_window.as_ref().is_some_and(|window| window.hit(self.cursor, &self.items).is_some());
            let on_setup = game.vend_setup.as_ref().is_some_and(|setup| setup.hit(self.cursor, &game.cart.items).is_some());
            match game.window_at(self.cursor) {
                _ if on_setup => {
                    let count = game.cart.items.len();
                    game.vend_setup.as_mut().unwrap().scroll_by(rows, count);
                }
                _ if on_item => {
                    let lines = game.item_lines;
                    game.item_window.as_mut().unwrap().scroll_by(rows, lines);
                }
                _ if on_shop => game.shop.as_mut().unwrap().scroll_by(self.cursor, rows),
                _ if on_pick => game.pick.as_mut().unwrap().0.scroll_by(rows),
                _ if game.achievement_window.covers(self.cursor) => game.achievement_window.scroll_by(rows, &self.achievement_info),
                Some(Panel::Inventory) => game.inventory.scroll_by(rows),
                Some(Panel::Cart) => game.cart.scroll_by(rows),
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
                None => game.camera_goal.wheel(-rows as f32),
            }
        }
    }

    fn left_button(&mut self, event_loop: &ActiveEventLoop, state: ElementState) {
        self.mouse_down = state == ElementState::Pressed;
        let dragged = match &mut self.scene {
            Scene::Game(game) if state == ElementState::Released => {
                let item = game.dragging.take();
                // An item or a skill let go over a hotkey slot goes there.
                if let Some((dragged, _, true)) = item
                    && let Some(hotkeys::Hit::Slot(slot)) = game.hotkeys.hit(self.cursor)
                {
                    let key = match dragged {
                        Dragged::Item(index) => game.inventory.get(index).map(|item| net::Hotkey { skill: false, id: item.id, count: item.amount as i16 }),
                        // At the level picked in the skill window.
                        Dragged::Skill(id) => game.skills.iter().find(|skill| skill.id == id).map(|skill| net::Hotkey {
                            skill: true,
                            id: u32::from(id),
                            count: game.skill_window.use_level(id, skill.level) as i16,
                        }),
                        Dragged::Stored(_) | Dragged::Carted(_) | Dragged::Hotkey(_) => None,
                    };
                    if let Some(key) = key {
                        game.hotkeys.keys[slot] = Some(key);
                        let _ = game.session.set_hotkey(0, slot as u16, Some(key));
                    }
                }
                // A hotkey moved along the bar trades places with what's there; let go off the bar, it's taken off.
                if let Some((Dragged::Hotkey(from), _, true)) = item {
                    let to = match game.hotkeys.hit(self.cursor) {
                        Some(hotkeys::Hit::Slot(to)) => Some(to),
                        Some(_) => Some(from),
                        None => None,
                    };
                    match to {
                        Some(to) if to != from => {
                            game.hotkeys.keys.swap(from, to);
                            for slot in [from, to] {
                                let _ = game.session.set_hotkey(0, slot as u16, game.hotkeys.keys[slot]);
                            }
                        }
                        Some(_) => {}
                        None => {
                            game.hotkeys.keys[from] = None;
                            let _ = game.session.set_hotkey(0, from as u16, None);
                        }
                    }
                }
                // An item let go over the storage goes in; a stored one let go over the inventory comes out.
                let on_trade = game.trade.as_ref().is_some_and(|trade| trade.hit(self.cursor).is_some());
                let on_mail = matches!(game.mail.hit(self.cursor), Some(mail::Hit::Compose(_)));
                let on_refine = game.refine.as_ref().is_some_and(|window| window.hit(self.cursor).is_some());
                let on_materials = game.materials.as_ref().is_some_and(|window| window.hit(self.cursor).is_some());
                match (item, game.window_at(self.cursor)) {
                    (Some((Dragged::Item(index), _, true)), _) if on_mail => game.move_item(Asked::Mail(index)),
                    (Some((Dragged::Item(index), _, true)), _) if on_materials => game.move_item(Asked::Material(index)),
                    // Into the refine machine: the server lists what refines it.
                    (Some((Dragged::Item(index), _, true)), _) if on_refine => {
                        let _ = game.session.refine_select(index);
                    }
                    (Some((Dragged::Item(index), _, true)), _) if on_trade => {
                        if let Some(id) = game.offer(index) {
                            game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ERROR);
                        }
                    }
                    (Some((Dragged::Item(index), _, true)), Some(Panel::Storage)) => game.move_item(Asked::Store(index)),
                    // Let go over the world: dropped at the player's feet, asking how many of a stack.
                    (Some((Dragged::Item(index), _, true)), None) if !game.on_ui(self.cursor, &self.items) => game.move_item(Asked::Drop(index)),
                    (Some((Dragged::Item(index), _, true)), Some(Panel::Cart)) => game.move_item(Asked::ToCart(index)),
                    (Some((Dragged::Carted(index), _, true)), Some(Panel::Inventory)) => game.move_item(Asked::FromCart(index)),
                    (Some((Dragged::Stored(index), _, true)), Some(Panel::Inventory)) => game.move_item(Asked::Take(index)),
                    _ => {}
                }
                let trade_moved = game.trade.as_mut().is_some_and(|trade| trade.frame.drop_drag())
                    | game.room.as_mut().is_some_and(|room| room.frame.drop_drag())
                    | game.item_window.as_mut().is_some_and(|window| window.drop_drag())
                    | game.mail.drop_drag()
                    | game.quest_window.frame.drop_drag()
                    | game.achievement_window.frame.drop_drag()
                    | game.roulette.as_mut().is_some_and(|window| window.frame.drop_drag())
                    | game.monster_window.as_mut().is_some_and(|window| window.frame.drop_drag());
                // A slider let go keeps its volume for next time.
                let slid = game.audio_window.as_mut().is_some_and(options::AudioWindow::drop_drag);
                if slid {
                    self.sound.save();
                }
                let windows = trade_moved
                    | slid
                    | game.party_window.frame.drop_drag()
                    | game.status_window.frame.drop_drag()
                    | game.guild_window.frame.drop_drag()
                    | game.clan_window.frame.drop_drag()
                    | game.cart.frame.drop_drag()
                    | game.pet_window.frame.drop_drag()
                    | game.mercenary_window.frame.drop_drag()
                    | game.mercenary_skill_list.frame.drop_drag()
                    | game.storage.frame.drop_drag()
                    | game.inventory.frame.drop_drag()
                    | game.equipment.frame.drop_drag()
                    | game.basic_info.frame.drop_drag()
                    | game.hotkeys.frame.drop_drag()
                    | game.skill_window.frame.drop_drag();
                windows || item.is_some_and(|(_, _, moved)| moved)
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

    /// A mouse press: grabs a window by its title bar.
    fn press(&mut self) {
        let cursor = self.cursor;
        // The item description window goes by its top.
        if let Scene::Game(game) = &mut self.scene
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
            && let Some(hit) = room.hit(cursor)
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
            && let Some(hit) = game.roulette.as_ref().and_then(|window| window.hit(cursor))
        {
            if hit == roulette::Hit::Title(TitleHit::Title) {
                game.roulette.as_mut().unwrap().frame.start_drag(cursor);
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
                let jobs = self.skill_trees.lineage(game.me.job);
                match game.skill_window.hit(cursor, &self.skill_trees, &jobs, &game.skills, game.skill_points()) {
                    Some(skills::Hit::Skill(skill)) if game.skills.iter().any(|known| known.id == skill && known.level > 0 && known.kind != 0) => {
                        game.dragging = Some((Dragged::Skill(skill), cursor, false));
                    }
                    Some(skills::Hit::Title(TitleHit::Title)) => game.skill_window.frame.start_drag(cursor),
                    _ => {}
                }
            }
            match id {
                Panel::BasicInfo if game.basic_info.hit(cursor) == Some(basic_info::Hit::Title) => game.basic_info.frame.start_drag(cursor),
                Panel::Inventory if game.inventory.hit(cursor) == Some(inventory::Hit::Title(TitleHit::Title)) => game.inventory.frame.start_drag(cursor),
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
        } else if let Some(id) = game.send_chat(party, guild, &self.messages, &mut self.effects_on) {
            game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ERROR);
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

    fn key(&mut self, event: &control::KeyPress) {
        if event.state != ElementState::Pressed {
            return;
        }
        let key = &event.logical_key;
        // Ctrl+Cmd+F, as Macs have it, or Alt+Enter elsewhere: fullscreen (the canvas scaled to fit, bars on the sides), or back.
        let fullscreen = if cfg!(target_os = "macos") {
            self.modifiers.super_key() && self.modifiers.control_key() && event.physical_key == PhysicalKey::Code(KeyCode::KeyF)
        } else {
            self.modifiers.alt_key() && *key == Key::Named(NamedKey::Enter)
        };
        if fullscreen && let Some(gpu) = &self.gpu {
            gpu.window.set_fullscreen(gpu.window.fullscreen().is_none().then_some(winit::window::Fullscreen::Borderless(None)));
            return;
        }
        // Ctrl+G opens or closes the clan window while in a clan.
        if let Scene::Game(game) = &mut self.scene
            && self.modifiers.control_key()
            && !self.modifiers.alt_key()
            && !self.modifiers.super_key()
            && event.physical_key == PhysicalKey::Code(KeyCode::KeyG)
        {
            if game.clan.is_some() {
                game.clan_window.frame.toggle();
                game.raise(Panel::Clan);
            }
            return;
        }
        // Alt+E and Alt+Q open the inventory and the equipment anywhere in the game (by physical key: Option+letter
        // types dead keys on a Mac).
        // Ctrl+R, the game's key for the mercenary window, and Ctrl+T, its follow switch, while there's a mercenary.
        if let Scene::Game(game) = &mut self.scene
            && self.modifiers.control_key()
            && game.mercenary.is_some()
        {
            match event.physical_key {
                PhysicalKey::Code(KeyCode::KeyR) => {
                    game.mercenary_window.frame.toggle();
                    game.raise(Panel::Mercenary);
                    return;
                }
                PhysicalKey::Code(KeyCode::KeyT) => {
                    game.steering.following = !game.steering.following;
                    return;
                }
                _ => {}
            }
        }
        if let Scene::Game(game) = &mut self.scene
            && self.modifiers.alt_key()
        {
            let window = match event.physical_key {
                PhysicalKey::Code(KeyCode::KeyE) => Some((Panel::Inventory, &mut game.inventory.frame)),
                PhysicalKey::Code(KeyCode::KeyQ) => Some((Panel::Equipment, &mut game.equipment.frame)),
                PhysicalKey::Code(KeyCode::KeyV) => Some((Panel::BasicInfo, &mut game.basic_info.frame)),
                PhysicalKey::Code(KeyCode::KeyS) => Some((Panel::Skills, &mut game.skill_window.frame)),
                PhysicalKey::Code(KeyCode::KeyZ) => Some((Panel::Party, &mut game.party_window.frame)),
                PhysicalKey::Code(KeyCode::KeyA) => Some((Panel::Status, &mut game.status_window.frame)),
                PhysicalKey::Code(KeyCode::KeyG) => Some((Panel::Guild, &mut game.guild_window.frame)),
                PhysicalKey::Code(KeyCode::KeyW) if game.cart.cart.is_some() => Some((Panel::Cart, &mut game.cart.frame)),
                // Alt+J, the game's own key for the pet window, while there's a pet.
                PhysicalKey::Code(KeyCode::KeyJ) if game.pet.is_some() => Some((Panel::Pet, &mut game.pet_window.frame)),
                _ => None,
            };
            if let Some((id, frame)) = window {
                frame.toggle();
                game.raise(id);
                // The guild window asks for what it shows.
                if id == Panel::Guild && game.guild_window.frame.open {
                    game.ask_guild_pages();
                }
                // The pet window asks for fresh news of the pet.
                if id == Panel::Pet && game.pet_window.frame.open {
                    let _ = game.session.pet_command(net::PET_INFO);
                }
                return;
            }
            // Alt+L: the emotion list; Alt+M: the macro window.
            if event.physical_key == PhysicalKey::Code(KeyCode::KeyL) {
                game.toggle_emotion_list();
                return;
            }
            if event.physical_key == PhysicalKey::Code(KeyCode::KeyM) {
                game.toggle_macro_window();
                return;
            }
            // Alt+1 to Alt+0: the ten macro lines.
            let digits = [
                KeyCode::Digit1,
                KeyCode::Digit2,
                KeyCode::Digit3,
                KeyCode::Digit4,
                KeyCode::Digit5,
                KeyCode::Digit6,
                KeyCode::Digit7,
                KeyCode::Digit8,
                KeyCode::Digit9,
                KeyCode::Digit0,
            ];
            if let PhysicalKey::Code(code) = event.physical_key
                && let Some(line) = digits.iter().position(|digit| *digit == code)
            {
                self.send_macro(line);
                return;
            }
            // Alt+U: the quest window.
            if event.physical_key == PhysicalKey::Code(KeyCode::KeyU) {
                game.quest_window.frame.toggle();
                return;
            }
            // Alt+C: make a chat room (not while in one).
            if event.physical_key == PhysicalKey::Code(KeyCode::KeyC) {
                game.room_make = if game.room_make.is_some() || game.room.is_some() { None } else { Some(chatroom::Make::default()) };
                return;
            }
        }
        // F1-F9 press the hotkeys of the first row; F12 shows or hides the bar.
        if let Scene::Game(game) = &mut self.scene
            && let Key::Named(named) = key
        {
            let function = [NamedKey::F1, NamedKey::F2, NamedKey::F3, NamedKey::F4, NamedKey::F5, NamedKey::F6, NamedKey::F7, NamedKey::F8, NamedKey::F9]
                .iter()
                .position(|function| function == named);
            if let Some(slot) = function {
                game.press_hotkey(slot);
                return;
            }
            if *named == NamedKey::F12 {
                game.hotkeys.frame.toggle();
                return;
            }
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
                    (Key::Named(NamedKey::Escape), _, Some(_)) => game.choose(true),
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
            Scene::Game(game) if game.dismiss_asked && matches!(key, Key::Named(NamedKey::Enter | NamedKey::Escape)) => {
                game.answer_dismiss(*key == Key::Named(NamedKey::Enter), &self.messages);
            }
            Scene::Game(game) if game.pet_asked.is_some() && matches!(key, Key::Named(NamedKey::Enter | NamedKey::Escape)) => {
                game.answer_pet(*key == Key::Named(NamedKey::Enter));
            }
            Scene::Game(game) if game.amount.is_some() => match key {
                Key::Named(NamedKey::Enter) => game.answer_amount(true),
                Key::Named(NamedKey::Escape) => game.answer_amount(false),
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
            Scene::Game(game) if game.deal_choice.is_some() && matches!(key, Key::Named(NamedKey::Escape)) => game.deal_choice = None,
            // Typing always goes to the chat line, as in the client; Enter sends it, whispered when the name box has
            // a name. Tab moves between the name box and the message.
            Scene::Game(game) => match key {
                Key::Named(NamedKey::Tab) => game.chat.naming = !game.chat.naming,
                Key::Named(NamedKey::Insert) => game.toggle_sit(),
                Key::Named(NamedKey::Enter) if game.chat.naming => game.chat.naming = false,
                Key::Named(NamedKey::Enter) => self.send_chat_input(self.modifiers.control_key(), self.modifiers.alt_key()),
                Key::Named(NamedKey::Escape) if game.world_map.is_some() => game.world_map = None,
                Key::Named(NamedKey::Escape) if game.pick.is_some() => game.answer_pick(None),
                Key::Named(NamedKey::Escape) if game.capturing => game.capturing = false,
                Key::Named(NamedKey::Escape) if game.roulette.is_some() => game.close_roulette(),
                Key::Named(NamedKey::Escape) if game.monster_window.is_some() => game.monster_window = None,
                Key::Named(NamedKey::Escape) if game.targeting.is_some() => game.targeting = None,
                Key::Named(NamedKey::Escape) if !game.chat.typing().is_empty() => game.chat.typing().clear(),
                Key::Named(NamedKey::Escape) => game.escape = if game.escape.is_some() { None } else { Some(escape::Menu { dead: false }) },
                Key::Named(NamedKey::Backspace) => {
                    game.chat.typing().pop();
                }
                _ if !self.modifiers.alt_key() && !self.modifiers.super_key() && !self.modifiers.control_key() => {
                    game.chat.typing().extend(event.text.as_deref().unwrap_or_default().chars().filter(|character| !character.is_control()));
                }
                _ => {}
            },
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
                // A list sent again (after a deletion) keeps the cursor where it was.
                if let Scene::CharSelect(previous) = self.scene.base() {
                    select.selected = previous.selected;
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
                match &mut self.scene {
                    // After a rename its message stays up over the new list.
                    Scene::Message(_, back) if matches!(**back, Scene::CharSelect(_)) => **back = Scene::CharSelect(Box::new(select)),
                    scene => *scene = Scene::CharSelect(Box::new(select)),
                }
                let first = !std::mem::replace(&mut self.scripted, true);
                if autostart.is_some() && first {
                    self.start_game();
                }
                if first {
                    self.run_select_switches();
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
                let camera_goal = CameraGoal::new(self.indoor.contains(&map_key(&map)));
                let camera = camera_goal.camera(world.cell_position(position.map(f32::from)));
                let now = Instant::now();
                let minimap = minimap::load_map(gpu, &self.grf, &map);
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
                self.scene = Scene::Game(Box::new(Game {
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
                    last_frame: now,
                    chat: chat::Chat::default(),
                    entered: now,
                    last_keepalive: now,
                    shadow: SpriteSet::at(&self.grf, "data\\sprite\\shadow"),
                    emotion_sprite: SpriteSet::at(&self.grf, "data\\sprite\\이팩트\\emotion"),
                    emotions: HashMap::new(),
                    spirits: HashMap::new(),
                    sphere: gpu.try_load_texture(&self.grf, "data\\texture\\effect\\기구.bmp", true).ok(),
                    coin_sprite: SpriteSet::at(&self.grf, "data\\sprite\\이팩트\\동전"),
                    digits: Digits::load(gpu, &self.grf),
                    dialog: None,
                    hit_boxes: Vec::new(),
                    ground: HashMap::new(),
                    item_sprites: HashMap::new(),
                    inventory: Inventory::new(std::env::var_os("RUSTNAROK_CLIENT_INVENTORY").is_some()),
                    equipment: {
                        let mut equipment = Equipment::default();
                        equipment.frame.open = std::env::var_os("RUSTNAROK_CLIENT_EQUIPMENT").is_some();
                        equipment
                    },
                    basic_info: BasicInfo::default(),
                    hotkeys: hotkeys::Hotkeys::default(),
                    dragging: None,
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
                    speech: HashMap::new(),
                    status_icons: Vec::new(),
                    cooldowns: skills::Cooldowns::default(),
                    talkie_text: None,
                    talkie_spots: HashMap::new(),
                    skill_delay_line: self.messages.get(SKILL_DELAY).cloned().unwrap_or_default(),
                    effects: effects::Effects::default(),
                    deal_choice: None,
                    shop: None,
                    storage: storage::Storage::default(),
                    cutin: None,
                    marks: Vec::new(),
                    amount: None,
                    escape: None,
                    audio_window: None,
                    world_map: None,
                    quests: Vec::new(),
                    quest_window: quest::QuestWindow::default(),
                    achievement_totals: net::AchievementTotals::default(),
                    achievements: HashMap::new(),
                    achievement_window: achievement::AchievementWindow::default(),
                    item_window: None,
                    item_lines: 0,
                    whispered: VecDeque::new(),
                    whisper_asked: VecDeque::new(),
                    pick: None,
                    card_asked: None,
                    item_skill: None,
                    trade: None,
                    refine: None,
                    materials: None,
                    roulette: None,
                    monster_window: None,
                    boss: None,
                    viewing: None,
                    bank: None,
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
                    party_invite: None,
                    party_dots: HashMap::new(),
                    friends: Vec::new(),
                    friend_asked: None,
                    expansion_asked: None,
                    bind_asked: None,
                    broken_before: None,
                    pet_id: None,
                    pet: None,
                    pet_window: pet::PetWindow::default(),
                    pet_asked: None,
                    mercenary: None,
                    mercenary_skills: Vec::new(),
                    mercenary_window: mercenary::Window::default(),
                    mercenary_skill_list: mercenary::SkillList::default(),
                    steering: mercenary::Steering::default(),
                    dismiss_asked: false,
                    vend_setup: None,
                    my_shop: None,
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
                    guild_shown: false,
                    clan: None,
                    clan_window: clan::ClanWindow::default(),
                    emblems: HashMap::new(),
                    name_tags: HashMap::new(),
                    pointed: None,
                    signs: HashMap::new(),
                    sign_rects: Vec::new(),
                    room: None,
                    room_make: None,
                    emotion_list: None,
                    macro_window: None,
                    macro_lines: macros::load(macros::path().as_deref()),
                    room_tried: false,
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
                    auto_say: std::env::var("RUSTNAROK_CLIENT_SAY").ok(),
                    auto_walk: std::env::var("RUSTNAROK_CLIENT_WALK").ok().and_then(|cell| {
                        let (x, y) = cell.split_once(',')?;
                        Some([x.parse().ok()?, y.parse().ok()?])
                    }),
                }));
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
        let (Scene::Game(game), Some(gpu)) = (&mut self.scene, &self.gpu) else { return };
        match World::load(gpu, &self.grf, &map) {
            Ok(world) => {
                game.camera_goal = CameraGoal::new(self.indoor.contains(&map_key(&map)));
                game.camera = game.camera_goal.camera(world.cell_position(position.map(f32::from)));
                game.minimap = minimap::load_map(gpu, &self.grf, &map);
                game.world = world;
                game.map = map;
                game.units.clear();
                game.spirits.clear();
                game.ground.clear();
                game.marks.clear();
                game.signs.clear();
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
            layer.sprites.push((image, [0.0, 0.0], SCREEN));
        }
        let [width, height] = LOADING_BAR;
        let corner = [((SCREEN[0] - width) / 2.0).floor(), (SCREEN[1] * 0.75).floor()];
        layer.sprites.push((&assets.loading_bar[0], corner, LOADING_BAR));
        layer.sprites.push((&assets.loading_bar[1], offset(corner, [1.0, 1.0]), [width - 2.0, height - 2.0]));
        let percent = "0%";
        layer.texts.push((percent.to_owned(), [((SCREEN[0] - self.font.width(percent)) / 2.0).floor(), corner[1] + 1.0], [255, 255, 0]));
        let composed = compose(gpu, &self.font, vec![layer]);
        let sprites = flatten(&composed);
        // Screenshots wait for the game itself, unless RUSTNAROK_CLIENT_SHOT_LOADING asks for this screen.
        match &self.screenshot {
            Some(path) if std::env::var_os("RUSTNAROK_CLIENT_SHOT_LOADING").is_some() => {
                gpu.capture(None, &sprites, path);
                event_loop.exit();
            }
            _ => gpu.draw(None, &sprites),
        }
        loading.shown = true;
    }

    fn map_event(&mut self, event: MapEvent) {
        if let MapEvent::Entered { position, direction } = event {
            self.show_loading(LoadTarget::Enter { position, direction });
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
                // Hidden NPCs (script triggers) have no sprite in the client either.
                _ if HIDDEN_JOBS.contains(&unit.job) => None,
                _ => job_names.get(&unit.job).and_then(|name| SpriteSet::single(grf, name, unit.kind == MONSTER)),
            };
            let mut actor = Actor::new(unit.name.clone(), sprites, unit.position.map(i32::from), unit.direction, unit.speed);
            actor.kind = unit.kind;
            actor.job = unit.job;
            actor.look = (unit.kind == PLAYER).then(|| unit_look(unit));
            actor.hidden = HIDDEN_JOBS.contains(&unit.job) || unit.job == WARP_JOB;
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
                actor.stop_at(unit.position.map(i32::from));
                actor.direction = unit.direction;
                actor.head_direction = unit.head_direction;
                actor.sitting = unit.sitting;
                // A player seen again may have changed clothes (or guilds) meanwhile.
                if actor.look.as_ref().is_some_and(|look| *look != unit_look(&unit)) {
                    actor.look = Some(unit_look(&unit));
                    actor.sprites = SpriteSet::player(grf, actor.look.as_ref().unwrap(), look_tables);
                }
                actor.guild = unit.guild;
                game.want_emblem(unit.guild, unit.emblem);
                game.monster_hp(&unit);
            }
            MapEvent::Turn { id, direction, head_direction } => {
                let actor = if id == game.account_id { Some(&mut game.player) } else { game.units.get_mut(&id) };
                if let Some(actor) = actor {
                    actor.direction = direction;
                    actor.head_direction = head_direction;
                }
            }
            MapEvent::UnitWalk { unit, to, .. } => {
                let actor = game.units.entry(unit.id).or_insert_with(|| new_actor(&unit));
                let from = unit.position.map(i32::from);
                actor.guild = unit.guild;
                if actor.cell != from && !actor.moving() {
                    actor.cell = from;
                }
                if let Some(path) = world::find_path(&game.world.gat, actor.cell, to.map(i32::from)) {
                    actor.walk(path);
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
                let key = (id != game.account_id).then_some(id);
                if count > 0 {
                    game.spirits.insert(key, count);
                } else {
                    game.spirits.remove(&key);
                }
            }
            MapEvent::Vanish { id, reason } => {
                if id != game.account_id {
                    game.spirits.remove(&Some(id));
                }
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
                match (reason, dying) {
                    (1, Some(actor)) => {
                        let (.., die) = actor::combat_actions(actor.kind);
                        actor.play(die, lands, 800.0, true);
                        actor.remove_at = key.map(|_| lands + CORPSE_TIME);
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
                let player = &mut game.player;
                if !player.moving() {
                    player.cell = from.map(i32::from);
                }
                if let Some(path) = world::find_path(&game.world.gat, player.cell, to.map(i32::from)) {
                    player.walk(path);
                }
            }
            MapEvent::Stop { id, position } => {
                let cell = position.map(i32::from);
                if id == game.account_id {
                    game.player.stop_at(cell);
                    if let Some((_, arrived)) = &mut game.pending {
                        *arrived = Some(Instant::now());
                    }
                } else if let Some(actor) = game.units.get_mut(&id) {
                    actor.stop_at(cell);
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
                // The new map's weather comes once it has loaded, and the mercenary's news.
                game.weather.clear();
                game.forget_mercenary(false);
                // Back at the save point: up again.
                if game.escape.as_ref().is_some_and(|menu| menu.dead) {
                    game.escape = None;
                }
                game.player.revive();
                // The NPC is left behind: its shop and its buy-or-sell question close (the server doesn't say so).
                game.close_shop();
                // rathena forgets the refine window on a map change; the material window and the roulette go too.
                game.refine = None;
                game.roulette = None;
                // The mirror's status ends with the warp.
                game.boss = None;
                game.materials = None;
                game.deal_choice = None;
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
            MapEvent::NpcSay { npc, text } => game.dialog_for(npc).lines.push(show_links(&text)),
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
                game.status.insert(id, value);
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
                let info = self.items.get(&item.item);
                if let Some(info) = info {
                    let resource = info.resource(item.identified).to_owned();
                    game.item_sprites.entry(resource.clone()).or_insert_with(|| SpriteSet::at(grf, &format!("data\\sprite\\아이템\\{resource}")));
                }
                let [x, y] = item.cell;
                let [sub_x, sub_y] = item.sub.map(|sub| f32::from(sub) / 12.0);
                game.ground.insert(
                    item.id,
                    FloorItem {
                        item: item.item,
                        identified: item.identified,
                        amount: item.amount,
                        at: [i32::from(x), i32::from(y)],
                        cell: [f32::from(x) - 0.5 + sub_x, f32::from(y) - 0.5 + sub_y],
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
                let name = inventory::item_name(&self.items, item.id, item.identified, item.refine);
                let message = |id: usize| self.messages.get(id).cloned().unwrap_or_default();
                match result {
                    0 => {
                        game.chat_line(messages::fill(&message(messages::GOT_ITEM), &[&name, &item.amount]), CHAT_ITEM);
                        game.inventory.add(item);
                    }
                    2 => game.chat_line(message(messages::CANT_CARRY_WEIGHT), CHAT_ERROR),
                    _ => game.chat_line(message(messages::CANT_CARRY), CHAT_ERROR),
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
            MapEvent::QuestAdded(quest) => {
                game.quests.retain(|known| known.id != quest.id);
                game.quests.push(quest);
            }
            MapEvent::QuestRemoved(id) => {
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
            MapEvent::Broadcast { text, color, blue } => game.chat_line(text, color.unwrap_or(if blue { CHAT_BROADCAST_BLUE } else { CHAT_BROADCAST })),
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
            // ponytail: the banner effect alone; the data also has a sprite, data\sprite\이팩트\mvp.spr, the real
            // client may show with it.
            MapEvent::Mvp(id) => {
                if let Some(gpu) = gpu {
                    game.play_effect(gpu, grf, table, EF_MVP, id);
                }
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
            MapEvent::MvpOverweight => game.chat_line(self.messages.get(MVP_OVERWEIGHT).cloned().unwrap_or_default(), CHAT_ERROR),
            MapEvent::MercenaryInfo(hired) => game.mercenary = Some(hired),
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
                let distance = at.map_or(0, |[x, y]| (x - me[0]).abs().max((y - me[1]).abs()));
                if let Some(audio) = &mut self.audio {
                    audio.play(grf, &file, 1.0 - distance as f32 / HEARING_CELLS);
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
                let name = game.signs.get(&vendor).map(|sign| sign.title.clone()).unwrap_or_default();
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
            MapEvent::GuildInviteResult(result) => {
                let id = 378 + usize::from(result.min(3));
                game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), if result == 2 { CHAT_ITEM } else { CHAT_ERROR });
            }
            MapEvent::GuildLeft { character, .. } => {
                if character == game.me.id {
                    game.guild = None;
                    game.guild_window.frame.open = false;
                } else if let Some(guild) = &mut game.guild {
                    guild.members.retain(|member| member.character != character);
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
                        game.chat_line(message(PARTY_LEFT), CHAT_ITEM);
                    }
                    _ => {
                        if let Some(party) = &mut game.party {
                            party.members.retain(|member| member.info.account != account);
                        }
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
                    _ => 1871,
                };
                let text = rustnarok_client::messages::fill(self.messages.get(id).map_or("", String::as_str), &[&name]);
                game.chat_line(text, if result == 2 { CHAT_ITEM } else { CHAT_ERROR });
            }
            MapEvent::PartyChat { account, text } => game.chat_line(text, if account == game.account_id { PARTY_CHAT_OURS } else { PARTY_CHAT }),
            MapEvent::PartyPosition { account, cell } => {
                if let Some(member) = game.party.as_mut().and_then(|party| party.member(account)) {
                    member.cell = Some(cell.map(i32::from));
                }
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
            MapEvent::PartyOptions { exp, .. } => {
                if let Some(party) = &mut game.party {
                    party.exp_shared = exp == 1;
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
            // Whispers, yellow both ways (the client's "(From name)" wording).
            MapEvent::Whisper { from, text } => game.chat_line(format!("(From {from}) : {text}"), WHISPER_COLOR),
            MapEvent::WhisperResult(result) => {
                let (to, text) = game.whispered.pop_front().unwrap_or_default();
                let line = match result {
                    0 => format!("(To {to}) : {text}"),
                    1 => self.messages.get(WHISPER_FAILED).cloned().unwrap_or_default(),
                    _ => format!("{to}{}", self.messages.get(WHISPER_FAILED + usize::from(result) - 1).cloned().unwrap_or_default()),
                };
                game.chat_line(line, WHISPER_COLOR);
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
                let id = if allow { ALLOW_RESULTS } else { BLOCK_RESULTS } + usize::from(result.min(2));
                game.chat_line(format!("{name}{}", self.messages.get(id).cloned().unwrap_or_default()), if result == 0 { CHAT_ITEM } else { CHAT_ERROR });
            }
            MapEvent::WhisperList(names) if names.is_empty() => game.chat_line(self.messages.get(BLOCKED_NONE).cloned().unwrap_or_default(), CHAT_ITEM),
            MapEvent::WhisperList(names) => {
                for name in names {
                    game.chat_line(name, CHAT_ITEM);
                }
            }
            MapEvent::WhisperAll { allow, result } => {
                let id = if allow { ALLOW_ALL_RESULTS } else { BLOCK_ALL_RESULTS } + usize::from(result != 0);
                game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), if result == 0 { CHAT_ITEM } else { CHAT_ERROR });
            }
            MapEvent::Cutin { image, place } => {
                let path = format!("{UI}illust\\{image}{}", if image.contains('.') { "" } else { ".bmp" });
                game.cutin = gpu.filter(|_| !image.is_empty()).and_then(|gpu| gpu.try_load_texture(grf, &path, false).ok()).map(|texture| (texture, place));
            }
            MapEvent::Mark { npc, kind, cell, id, color } => {
                game.marks.retain(|(key, ..)| *key != (npc, id));
                if kind != 2
                    && let Some(gpu) = gpu
                {
                    let until = (kind == 0).then(|| Instant::now() + Duration::from_secs(15));
                    game.marks.push(((npc, id), cell.map(i32::from), gpu.solid([color[0], color[1], color[2], 255]), until));
                }
            }
            MapEvent::InventoryExpansion(slots) => game.inventory.expansion = slots,
            MapEvent::Hotkeys { tab: 0, keys, .. } => {
                for (slot, key) in keys.into_iter().enumerate().take(game.hotkeys.keys.len()) {
                    game.hotkeys.keys[slot] = key;
                }
            }
            MapEvent::Hotkeys { .. } => {}
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
                    game.casts.insert((source != me).then_some(source), (Instant::now(), Duration::from_millis(u64::from(cast_ms))));
                }
                game.skill_motion(source, skill, 0, overhead::Shout::Cast, cast_ms as f32, &self.skill_info);
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
                    // Monsters swing as for an attack; players take the skill pose.
                    if source.kind != PLAYER {
                        source.play(actor::combat_actions(source.kind).0, Instant::now(), act.source_speed.max(100) as f32, false);
                    }
                }
                game.skill_motion(act.source, skill, i32::from(level as i16), overhead::Shout::Hit, act.source_speed as f32, &self.skill_info);
                if act.damage != 0 && !(act.source == act.target && act.kind == 6) {
                    game.show_hit(&act);
                    // Hit by hit: what comes at the target first (a bolt's arrows), then what shows as it lands.
                    if let Some(gpu) = gpu {
                        let now = Instant::now();
                        let lands = now + Duration::from_millis(act.source_speed.max(0) as u64);
                        for hit in 0..u32::from(act.hits.max(1)) {
                            for effect in table.skill(skill, "beforeHitEffectId") {
                                game.play_effect_from(gpu, grf, table, effect, act.target, now + MULTI_HIT_DELAY * hit);
                            }
                            for effect in table.skill(skill, "hitEffectId") {
                                game.play_effect_from(gpu, grf, table, effect, act.target, lands + MULTI_HIT_DELAY * hit);
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
                game.skill_motion(source, skill, amount, overhead::Shout::Used, 400.0, &self.skill_info);
                if HEAL_SKILLS.contains(&skill) {
                    game.damages.push(Damage {
                        target: (target != me).then_some(target),
                        amount,
                        start: Instant::now(),
                        kind: DamageKind::Heal { sp: false },
                        flinch: None,
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
                game.skill_motion(source, skill, i32::from(level as i16), overhead::Shout::Hit, 400.0, &self.skill_info);
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
                    actor.cart = if on { SpriteSet::at(grf, &actor::cart_path(value)) } else { None };
                }
                if id == game.account_id {
                    game.status_icons.retain(|(icon, ..)| *icon != actor::PUSH_CART);
                    if on && self.status_icon_files.contains_key(&actor::PUSH_CART) {
                        game.status_icons.push((actor::PUSH_CART, Instant::now(), None));
                    }
                }
            }
            MapEvent::StatusIcon { kind, id, on, left_ms, .. } => {
                // The mirror's 10 minutes are over: the boss leaves the minimap.
                if id == game.account_id && kind == BOSS_ALARM && !on {
                    game.boss = None;
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
                game.damages.push(Damage { target: None, amount, start: Instant::now(), kind: DamageKind::Heal { sp: kind == net::STATUS_SP }, flinch: None });
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
                    .map(|goods| shop::Offer { item: goods.item, price: goods.discounted, kind: goods.kind, identified: true, ..Default::default() })
                    .collect();
                game.shop = Some(shop::Shop::new(false, offers));
                game.auto_deal();
            }
            MapEvent::ShopWants(wants) => {
                let offers = wants
                    .into_iter()
                    .filter_map(|(index, _, price)| {
                        let item = game.inventory.get(index)?;
                        Some(shop::Offer {
                            item: item.id,
                            price,
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
                // The client's words for how it went.
                let (id, color) = match result {
                    0 => (54, CHAT_ITEM),
                    1 => (55, CHAT_ERROR),
                    2 => (56, CHAT_ERROR),
                    4 => (230, CHAT_ERROR),
                    5 => (281, CHAT_ERROR),
                    7 => (1797, CHAT_ERROR),
                    _ => (57, CHAT_ERROR),
                };
                game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), color);
                game.shop = None;
            }
            MapEvent::MiscEffect { id, kind } => {
                if let (Some(gpu), Some(effect)) = (gpu, table.misc(kind)) {
                    game.play_effect(gpu, grf, table, effect, id);
                }
            }
            // The server's positions: walk from where it has us.
            MapEvent::TooFar { target, target_cell, cell, range } => {
                game.attack_range = i32::from(range);
                game.attack_from(target, cell.map(i32::from), target_cell.map(i32::from));
            }
            MapEvent::Entered { .. } => unreachable!("handled above"),
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
        base.sprites.push((&assets.background, [0.0; 2], SCREEN));
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
                select.draw(&mut base, &assets.char_select, font, &self.messages, &self.pc_job_names, &self.map_names, &self.icons, base_pointer, &poses)
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
        let mut sprites = flatten(&composed);
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
                gpu.capture(None, &sprites, path);
                event_loop.exit();
            }
            _ => gpu.draw(None, &sprites),
        }
        self.drawn_blink_phase = blink_phase;
    }

    fn redraw_game(&mut self, event_loop: &ActiveEventLoop) {
        let (Some(gpu), Some(assets), Scene::Game(game)) = (&mut self.gpu, &self.assets, &mut self.scene) else { return };
        let (grf, items, messages, icons, cursor_look, pc_job_names) =
            (&self.grf, &self.items, &self.messages, &mut self.icons, &mut self.cursor_look, &self.pc_job_names);
        let (audio, sound) = (&mut self.audio, &self.sound);
        let (worlds, map_names, quest_texts) = (&self.worlds, &self.map_names, &self.quest_texts);
        let (achievement_info, title_names) = (&self.achievement_info, &self.title_names);
        let (skill_info, skill_trees, status_icon_files) = (&self.skill_info, &self.skill_trees, &self.status_icon_files);
        let font = &self.font;
        let now = control::now();
        let pointer = (self.cursor, self.mouse_down);
        let players_first = game.players_first(self.modifiers.shift_key());
        let message = |id: usize| messages.get(id).map_or("", String::as_str);

        // The mercenary follows the player unless told otherwise; an attack order lasts while its target is there.
        if let Some(id) = game.mercenary.as_ref().map(|hired| hired.id)
            && let Some(cell) = game.units.get(&id).map(|actor| actor.cell)
        {
            if let Some(mercenary::Order::Attack(target)) = game.steering.order
                && game.units.get(&target).is_none_or(|actor| actor.remove_at.is_some())
            {
                game.steering.order = None;
            }
            if game.steering.should_call(cell, game.player.cell, now) {
                let _ = game.session.move_to_owner(id);
            }
        }

        // Fireworks weather sets off fireworks around the player.
        for [dx, dy] in game.weather.fireworks_due(now) {
            let cell = [game.player.cell[0] + dx, game.player.cell[1] + dy].map(|value| value.max(0) as u16);
            game.play_effect_at(gpu, grf, &self.effect_table, weather::FIREWORK, cell);
        }

        // Bodies that finished dying leave; hits that land make their target flinch.
        game.units.retain(|_, actor| actor.remove_at.is_none_or(|at| at > now));
        for damage in &mut game.damages {
            if damage.start <= now
                && let Some(duration) = damage.flinch.take()
            {
                let target = match damage.target {
                    None => Some(&mut game.player),
                    Some(id) => game.units.get_mut(&id),
                };
                if let Some(target) = target.filter(|target| target.remove_at.is_none()) {
                    let (_, flinch, _) = actor::combat_actions(target.kind);
                    target.play(flinch, now, duration, false);
                    target.stand_ready();
                }
            }
        }
        game.damages.retain(|damage| now < damage.start || now.duration_since(damage.start).as_secs_f32() < damage.kind.duration(damage.amount));

        // Move everyone, follow the player, then decode the sprite frames (mutably) before borrowing them.
        let walking = game.player.moving();
        let player_cell = game.player.update(now);
        if walking
            && !game.player.moving()
            && let Some((_, arrived)) = &mut game.pending
        {
            *arrived = Some(now);
        }
        let player_feet = game.world.cell_position(player_cell);
        game.camera.target = player_feet;
        if self.effects_on {
            game.camera.target[1] += game.effects.quake(now);
        }
        game.camera_goal.ease(&mut game.camera, now.duration_since(game.last_frame));
        game.last_frame = now;
        let view = game.camera.view(SCREEN[0] / SCREEN[1]);
        let yaw = game.camera.yaw;
        let mut frames = Vec::new();
        let mut sounds: Vec<(String, f32)> = game.player.sound_now(now).map(|sound| (sound, 1.0)).into_iter().collect();
        let (action, timing) = game.player.animation(now);
        let (direction, head_direction) = (game.player.direction, game.player.head_direction);
        // Pushcarts, apart from the bodies (their textures are the cart's, and they cast no shadow of their own).
        // A cart's ACT has one action per direction; its frames turn the wheels, so they run only while walking.
        // ponytail: the cart's anchor half a cell behind its owner; place it against the real client.
        let behind = |feet: [f32; 3], direction: u8| {
            // RO directions from south, turning west: the cell step each faces.
            let [dx, dy] = [[0.0, -1.0], [-1.0, -1.0], [-1.0, 0.0], [-1.0, 1.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [1.0, -1.0]][usize::from(direction % 8)];
            let length = f32::hypot(dx, dy);
            [feet[0] - dx / length * world::CELL * 0.5, feet[1], feet[2] - dy / length * world::CELL * 0.5]
        };
        let mut carts = Vec::new();
        let cart_timing = |action: usize, timing: Timing| if action == actor::WALK { timing } else { Timing::Loop(0.0) };
        if let Some(cart) = &mut game.player.cart {
            carts.push((
                Owner::Player,
                behind(player_feet, direction),
                cart.frame_at(gpu, usize::from(seen_direction(direction, yaw)), 0, cart_timing(action, timing)),
            ));
        }
        if let Some(sprites) = &mut game.player.sprites {
            frames.push((Owner::Player, player_feet, sprites.frame(gpu, action, seen_direction(direction, yaw), head_direction, timing)));
        }
        // The equipment window shows the character standing, facing front.
        let preview_feet = game.equipment.preview_feet();
        let viewed_feet = game.viewing.as_ref().and_then(|viewed| viewed.window.preview_feet());
        let viewed_placements = match (viewed_feet, game.viewing.as_mut().and_then(|viewed| viewed.sprites.as_mut())) {
            (Some(_), Some(sprites)) => sprites.frame(gpu, actor::IDLE, 0, 0, Timing::Loop(0.0)),
            _ => Vec::new(),
        };
        let preview_placements = match (preview_feet, &mut game.player.sprites) {
            (Some(_), Some(sprites)) => sprites.frame(gpu, actor::IDLE, 0, 0, Timing::Loop(0.0)),
            _ => Vec::new(),
        };
        let mut labels = Vec::new();
        // Talkie Boxes saying their words stand where their traps were.
        let mut feet_of = game.talkie_spots.clone();
        for (&id, actor) in &mut game.units {
            let cell = actor.update(now);
            let feet = game.world.cell_position(cell);
            feet_of.insert(id, feet);
            if let Some(sound) = actor.sound_now(now) {
                let distance = (cell[0] - player_cell[0]).abs().max((cell[1] - player_cell[1]).abs());
                sounds.push((sound, 1.0 - distance as f32 / HEARING_CELLS));
            }
            let (action, timing) = actor.animation(now);
            let (direction, head_direction) = (actor.direction, actor.head_direction);
            // NPC names show under them; players' and monsters' on hover (below).
            if !actor.hidden && (actor.kind == NPC || actor.sprites.is_none()) {
                labels.push((actor.name.clone(), feet));
            }
            if let Some(cart) = &mut actor.cart {
                carts.push((
                    Owner::Unit(id),
                    behind(feet, direction),
                    cart.frame_at(gpu, usize::from(seen_direction(direction, yaw)), 0, cart_timing(action, timing)),
                ));
            }
            if let Some(sprites) = &mut actor.sprites {
                frames.push((Owner::Unit(id), feet, sprites.frame(gpu, action, seen_direction(direction, yaw), head_direction, timing)));
            }
        }
        if let Some(audio) = audio {
            for (sound, volume) in sounds {
                audio.play(grf, &sound, volume);
            }
        }
        // Items on the ground; just-dropped ones fall into place.
        for (&id, item) in &game.ground {
            let Some(info) = items.get(&item.item) else { continue };
            let Some(Some(sprites)) = game.item_sprites.get_mut(info.resource(item.identified)) else { continue };
            let mut feet = game.world.cell_position(item.cell);
            if let Some(dropped) = item.dropped {
                let fallen = now.duration_since(dropped).as_secs_f32() * 1000.0 / DROP_MS_PER_CELL;
                feet[1] += (DROP_HEIGHT - fallen).max(0.0) * world::CELL;
            }
            frames.push((Owner::Item(id), feet, sprites.frame(gpu, 0, 0, 0, Timing::Loop(0.0))));
        }

        // Clickable areas: each unit's and item's sprite bounds and the cell under it on screen, in logical pixels.
        // The dead and the hidden can't be clicked.
        let to_screen = |[x, y]: [f32; 2]| [x * SCREEN[0], y * SCREEN[1]];
        game.hit_boxes = frames
            .iter()
            .filter(|(owner, ..)| !matches!(owner, Owner::Unit(id) if game.units.get(id).is_some_and(|actor| actor.remove_at.is_some() || actor.hidden)))
            .filter_map(|(owner, feet, placements)| {
                let (mut low, mut high) = ([f32::MAX; 2], [f32::MIN; 2]);
                for (_, position, size) in placements {
                    for axis in 0..2 {
                        let (a, b) = (position[axis], position[axis] + size[axis]);
                        low[axis] = low[axis].min(a.min(b));
                        high[axis] = high[axis].max(a.max(b));
                    }
                }
                let [left, top, right, bottom] = view.sprite_rect(*feet, low, high)?;
                let sprite = [left * SCREEN[0], top * SCREEN[1], right * SCREEN[0], bottom * SCREEN[1]];
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
                        Some(PLAYER) => picking::Kind::Player,
                        _ => picking::Kind::Other,
                    },
                };
                let min = if kind == picking::Kind::Item { picking::ITEM_MIN } else { picking::UNIT_MIN };
                Some(picking::Target { owner: *owner, kind, sprite, area: picking::area(sprite, cell, min), depth })
            })
            .collect();

        // Shadows first, so characters standing at the same depth draw over them.
        let shadow_placements = game.shadow.as_mut().map(|shadow| shadow.frame(gpu, 0, 0, 0, Timing::Loop(0.0))).unwrap_or_default();
        let mut billboards: Vec<Billboard> = Vec::new();
        if let Some(shadow) = &game.shadow {
            for (owner, feet, _) in &frames {
                let dead = matches!(owner, Owner::Unit(id) if game.units.get(id).is_some_and(|actor| actor.remove_at.is_some()));
                let scale = if matches!(owner, Owner::Item(_)) { ITEM_SHADOW } else { 1.0 };
                if !dead {
                    billboards.extend(shadow_placements.iter().map(|(key, position, size)| {
                        (shadow.texture(*key), *feet, position.map(|value| value * scale), size.map(|value| value * scale), [1.0; 4])
                    }));
                }
            }
        }
        let shadows = billboards.len();
        let sprites_of = |owner: &Owner| match owner {
            Owner::Player => game.player.sprites.as_ref(),
            Owner::Unit(id) => game.units[id].sprites.as_ref(),
            Owner::Item(id) => {
                let item = &game.ground[id];
                game.item_sprites.get(items[&item.item].resource(item.identified)).and_then(Option::as_ref)
            }
        };
        // Spirit spheres, or coins for the Gunslinger line, circling the body at chest height: the far half of the
        // circle behind the body, the near half in front.
        let turned = now.duration_since(game.entered).as_secs_f32();
        let coin = game.coin_sprite.as_mut().map(|sprite| sprite.frame_at(gpu, 0, 0, Timing::Loop(turned * 1000.0))).unwrap_or_default();
        let mut spirits: [Vec<Billboard>; 2] = [Vec::new(), Vec::new()];
        for (unit, &count) in &game.spirits {
            let (feet, job) = match unit {
                None => (player_feet, game.me.job),
                Some(id) => match (feet_of.get(id), game.units.get(id)) {
                    (Some(feet), Some(actor)) => (*feet, actor.job),
                    _ => continue,
                },
            };
            for place in 0..count {
                let angle = turned * SPIRIT_TURN + f32::from(place) * std::f32::consts::TAU / f32::from(count);
                let center = [angle.cos() * SPIRIT_RADIUS, -SPIRIT_HEIGHT + angle.sin() * SPIRIT_RADIUS * SPIRIT_TILT];
                let side = &mut spirits[usize::from(angle.sin() > 0.0)];
                if coin_job(job) {
                    if let Some(sprite) = &game.coin_sprite {
                        side.extend(
                            coin.iter()
                                .map(|(key, position, size)| (sprite.texture(*key), feet, [center[0] + position[0], center[1] + position[1]], *size, [1.0; 4])),
                        );
                    }
                } else if let Some(sphere) = &game.sphere {
                    let size = sphere.size.map(|side| side * SPHERE_SCALE);
                    side.push((sphere, feet, [center[0] - size[0] / 2.0, center[1] - size[1] / 2.0], size, [1.0; 4]));
                }
            }
        }
        let [behind, in_front] = spirits;
        billboards.extend(behind);
        // Carts before the bodies, so the one pulling a cart draws over it.
        let cart_of = |owner: &Owner| match owner {
            Owner::Player => game.player.cart.as_ref(),
            Owner::Unit(id) => game.units.get(id).and_then(|actor| actor.cart.as_ref()),
            Owner::Item(_) => None,
        };
        billboards.extend(carts.iter().flat_map(|(owner, feet, placements)| {
            let cart = cart_of(owner).unwrap();
            placements.iter().map(move |(key, position, size)| (cart.texture(*key), *feet, *position, *size, [1.0; 4]))
        }));
        billboards.extend(frames.iter().flat_map(|(owner, feet, placements)| {
            let sprites = sprites_of(owner).unwrap();
            placements.iter().map(move |(key, position, size)| (sprites.texture(*key), *feet, *position, *size, [1.0; 4]))
        }));
        billboards.extend(in_front);
        // The emotion list's pictures, posed while the sprite can still decode them.
        let emotion_icons = match (&game.emotion_list, &mut game.emotion_sprite) {
            (Some(list), Some(sprite)) => list.icons(gpu, sprite),
            _ => Vec::new(),
        };
        // Taming's slot machine: its result played once, then held a moment.
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
            game.emotions
                .retain(|_, (action, start)| now.duration_since(*start).as_secs_f32() * 1000.0 < sprite.index_ms(*action).unwrap_or(emotion::SHOWN_MS));
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
        if let Some(sprite) = &game.emotion_sprite {
            billboards.extend(shown.into_iter().flat_map(|(feet, placements)| {
                placements
                    .into_iter()
                    .map(move |(key, position, size)| (sprite.texture(key), feet, [position[0], position[1] - EMOTION_HEIGHT], size, [1.0; 4]))
            }));
        }

        // Numbers over units, moving as the client's do: hits drift diagonally on an arc, shrinking and fading; misses
        // and lucky dodges rise straight up; heals hold, then rise; a multi-hit's total pops up high (the newest only).
        if let Some(digits) = &game.digits {
            let mut newest_combo: HashMap<Option<u32>, Instant> = HashMap::new();
            for damage in game.damages.iter().filter(|damage| damage.start <= now && matches!(damage.kind, DamageKind::Combo { .. })) {
                let newest = newest_combo.entry(damage.target).or_insert(damage.start);
                *newest = (*newest).max(damage.start);
            }
            for damage in game.damages.iter().filter(|damage| damage.start <= now) {
                let Some(feet) = damage.target.map_or(Some(player_feet), |id| feet_of.get(&id).copied()) else { continue };
                let progress = now.duration_since(damage.start).as_secs_f32() / damage.kind.duration(damage.amount);
                if progress >= 1.0 {
                    continue;
                }
                let alpha = 1.0 - progress;
                let at = |rise: f32, drift: f32| [feet[0] + drift * world::CELL, feet[1] + rise * world::CELL, feet[2] - drift * world::CELL];
                let centered = |texture: &Texture, scale: f32| {
                    let size = texture.size.map(|value| value * scale);
                    ([-size[0] / 2.0, -size[1] / 2.0], size)
                };
                let rising: &[&Texture] = match damage.kind {
                    DamageKind::Lucky => &[&digits.lucky_background, &digits.lucky],
                    DamageKind::Hit { .. } if damage.amount <= 0 => &[&digits.miss],
                    _ => &[],
                };
                for texture in rising {
                    // Half the sprite pixel scale.
                    let (position, size) = centered(texture, 0.5);
                    billboards.push((texture, at(3.5 + progress * 7.0, 0.0), position, size, [1.0, 1.0, 1.0, alpha]));
                }
                match damage.kind {
                    _ if !rising.is_empty() => {}
                    DamageKind::Hit { .. } | DamageKind::Critical | DamageKind::Lucky => {
                        let arc = (-std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * (0.5 + progress * 1.5)).sin() * 5.0;
                        let anchor = at(2.0 + arc, progress * 4.0);
                        // Drawn at (1 - progress) * 4 times the sprite pixel scale.
                        let scale = (1.0 - progress) * 4.0;
                        let tint = match damage.kind {
                            DamageKind::Critical => {
                                let (position, size) = centered(&digits.critical, 0.6 * scale);
                                billboards.push((&digits.critical, anchor, [position[0], position[1] - 6.0], size, [0.66, 0.66, 0.66, alpha]));
                                [0.9, 0.9, 0.15, alpha]
                            }
                            DamageKind::Hit { taken: true } => [1.0, 0.0, 0.0, alpha],
                            _ => [1.0, 1.0, 1.0, alpha],
                        };
                        billboards
                            .extend(digits.place(damage.amount, scale).into_iter().map(|(texture, position, size)| (texture, anchor, position, size, tint)));
                    }
                    DamageKind::Heal { sp } => {
                        let scale = ((1.0 - progress * 2.0) * 3.0).max(0.8);
                        let anchor = at(2.0 + if progress < 0.4 { 0.0 } else { (progress - 0.4) * 5.0 }, 0.0);
                        let tint = if sp { [0.13, 0.19, 0.75, alpha] } else { [0.0, 1.0, 0.0, alpha] };
                        billboards
                            .extend(digits.place(damage.amount, scale).into_iter().map(|(texture, position, size)| (texture, anchor, position, size, tint)));
                    }
                    DamageKind::Combo { last } => {
                        if newest_combo.get(&damage.target) != Some(&damage.start) || (!last && progress > 0.15) {
                            continue;
                        }
                        let scale = progress.min(0.05) * 70.0;
                        let anchor = at(7.0 + progress, 0.0);
                        billboards.extend(
                            digits
                                .place(damage.amount, scale)
                                .into_iter()
                                .map(|(texture, position, size)| (texture, anchor, position, size, [0.9, 0.9, 0.15, alpha])),
                        );
                    }
                }
            }
        }

        let viewed_preview: Vec<Sprite> = match (viewed_feet, game.viewing.as_ref().and_then(|viewed| viewed.sprites.as_ref())) {
            (Some(feet), Some(sprites)) => {
                viewed_placements.iter().map(|(key, position, size)| (sprites.texture(*key), offset(feet, *position), *size)).collect()
            }
            _ => Vec::new(),
        };
        let preview: Vec<Sprite> = match (preview_feet, &game.player.sprites) {
            (Some(feet), Some(sprites)) => {
                preview_placements.iter().map(|(key, position, size)| (sprites.texture(*key), offset(feet, *position), *size)).collect()
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
            labels.push((name, feet));
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
            labels.push((messages::fill(message(messages::ITEM_AMOUNT), &[&name, &item.amount]), game.world.cell_position(item.cell)));
        }

        // In-world overlays: under the feet (in sprite pixels, the way sprites are drawn) HP bars and names under them;
        // over the head cast bars and what's said.
        let sprite_point = |feet: [f32; 3], y: f32| view.sprite_rect(feet, [0.0, y], [0.0, y]).map(|[x, y, ..]| [x * SCREEN[0], y * SCREEN[1]]);
        let mut base = Layer::default();
        for (name, feet) in &labels {
            let name = name.split('#').next().unwrap_or_default();
            if let Some(point) = sprite_point(*feet, overhead::NAME_BELOW) {
                base.outlined(name, overhead::label_corner(point, font.width(name), font.line_height(), SCREEN));
            }
        }
        // ponytail: a player's title, then their guild, on lines under the name; where titles go isn't known.
        let mut under_name = 1.0;
        if let Some((title, feet)) = &title_line
            && let Some(point) = sprite_point(*feet, overhead::NAME_BELOW)
        {
            let [x, y] = overhead::label_corner(point, font.width(title), font.line_height(), SCREEN);
            base.outlined(title, [x, y + font.line_height()]);
            under_name += 1.0;
        }
        if let Some((name, line, feet, guild_id)) = &guild_line
            && let Some(point) = sprite_point(*feet, overhead::NAME_BELOW)
        {
            let corner = |text: &str| overhead::label_corner(point, font.width(text), font.line_height(), SCREEN);
            let top = corner(name)[1] + under_name * font.line_height();
            base.outlined(line, [corner(line)[0], top]);
            // ponytail: the emblem left of the guild line; calibrate against the real client.
            if let Some(emblem) = game.emblems.get(guild_id).and_then(|(_, emblem)| emblem.as_ref()) {
                base.sprites.push((emblem, [corner(line)[0] - guild::EMBLEM_SIZE[0] - 2.0, top], guild::EMBLEM_SIZE));
            }
        }
        let ratio = |value: Option<&i64>, maximum: Option<&i64>| match (value, maximum) {
            (Some(&value), Some(&maximum)) if maximum > 0 => Some((value as f32 / maximum as f32).clamp(0.0, 1.0)),
            _ => None,
        };
        let hp = ratio(game.status.get(&net::STATUS_HP), game.status.get(&net::STATUS_MAX_HP));
        let sp = ratio(game.status.get(&net::STATUS_SP), game.status.get(&net::STATUS_MAX_SP));
        let [border, background, green, red, blue] = &assets.bars;
        let mut bars = vec![(player_feet, hp, sp, [border, background, green, red])];
        // Monsters' bars in their own colors; a dying unit's bar shows empty until it's gone.
        bars.extend(game.unit_hp.iter().filter_map(|(id, &(hp, max))| {
            let actor = game.units.get(id)?;
            let hp = if actor.remove_at.is_some() { 0 } else { hp };
            let colors = if actor.kind == MONSTER { assets.monster_bars.each_ref() } else { [border, background, green, red] };
            Some((*feet_of.get(id)?, (max > 0).then(|| (hp as f32 / max as f32).clamp(0.0, 1.0)), None, colors))
        }));
        for (feet, hp, sp, [border, background, fill, low]) in bars {
            let (Some(hp), Some(point)) = (hp, sprite_point(feet, overhead::BAR_BELOW)) else { continue };
            let [x, y] = [point[0].trunc() - BAR_WIDTH / 2.0, point[1].trunc()];
            let height = if sp.is_some() { 9.0 } else { 5.0 };
            base.sprites.push((border, [x, y], [BAR_WIDTH, height]));
            base.sprites.push((background, [x + 1.0, y + 1.0], [BAR_WIDTH - 2.0, height - 2.0]));
            base.sprites.push((if hp < 0.25 { low } else { fill }, [x + 1.0, y + 1.0], [((BAR_WIDTH - 2.0) * hp).floor(), 3.0]));
            if let Some(sp) = sp {
                base.sprites.push((border, [x, y + 4.0], [BAR_WIDTH, 1.0]));
                base.sprites.push((blue, [x + 1.0, y + 5.0], [((BAR_WIDTH - 2.0) * sp).floor(), 3.0]));
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
            let Some([x, y]) = head(unit) else { continue };
            let left = x - CAST_BAR[0] / 2.0;
            let done = (now.duration_since(*start).as_secs_f32() / length.as_secs_f32()).min(1.0);
            base.sprites.push((border, [left, y], CAST_BAR));
            base.sprites.push((background, [left + 1.0, y + 1.0], [CAST_BAR[0] - 2.0, CAST_BAR[1] - 2.0]));
            base.sprites.push((&assets.cast_fill, [left + 1.0, y + 1.0], [((CAST_BAR[0] - 2.0) * done).round(), CAST_BAR[1] - 2.0]));
        }
        // Chat room signs over their owners' heads, like speech (not the room we're in).
        game.sign_rects.clear();
        for (owner, sign) in &game.signs {
            if game.room.as_ref().is_some_and(|room| room.sign.id == sign.id) {
                continue;
            }
            let unit = (*owner != game.account_id).then_some(*owner);
            let Some([x, y]) = head(&unit) else { continue };
            let rect = chatroom::draw_sign(&mut base, &assets.chatroom, font, sign, [x, y - chatroom::SIGN[1] - 6.0]);
            game.sign_rects.push((rect, sign.id, sign.kind));
        }
        for (unit, (text, color, _)) in &game.speech {
            let Some(anchor) = head(unit) else { continue };
            let lines = overhead::lines(text);
            let size = overhead::size(lines.iter().map(|line| font.width(line)).fold(0.0, f32::max), lines.len());
            let corner = overhead::corner(anchor, size, SCREEN);
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
        if let Some(dialog) = &game.dialog {
            // A warp list is the menu alone.
            if dialog.warp.is_none() {
                let position = npc_box_position();
                overlay.sprites.push((&assets.dialog, position, assets.dialog.size));
                let mut lines: Vec<Vec<([u8; 3], String)>> = dialog.lines.iter().flat_map(|line| wrap_colored(font, line, DIALOG_TEXT_WIDTH)).collect();
                if let Some((text, value)) = &dialog.input {
                    lines.push(vec![([0, 0, 0], format!("{}: {value}_", if *text { "입력" } else { "수량" }))]);
                }
                let shown = lines.len().saturating_sub(DIALOG_LINES);
                for (row, pieces) in lines[shown..].iter().enumerate() {
                    let mut x = position[0] + DIALOG_TEXT[0];
                    for (color, piece) in pieces {
                        overlay.texts.push((piece.clone(), [x, position[1] + DIALOG_TEXT[1] + row as f32 * DIALOG_LINE], *color));
                        x += font.width(piece);
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
        let jobs = skill_trees.lineage(game.me.job);
        let skill_icons = game
            .skill_window
            .icons(skill_trees, &jobs, skill_info)
            .into_iter()
            .chain(game.guild.as_ref().map(|guild| game.guild_window.icons(guild, skill_info)).unwrap_or_default())
            .chain(mercenary::SkillList::icons(&game.mercenary_skills, skill_info))
            .chain(game.hotkeys.keys.iter().flatten().filter(|key| key.skill).filter_map(|key| skill_info.get(&(key.id as u16)).map(skills::icon_path)));
        let status_paths =
            game.status_icons.iter().filter_map(|(kind, ..)| status_icon_files.get(kind).map(|(_, file)| format!("data\\texture\\effect\\{file}")));
        let shop_icons = game.shop.as_ref().map(|shop| shop.icons(items)).unwrap_or_default();
        let mut pick_icons = game.pick.as_ref().map(|(list, _)| list.icons(items, skill_info)).unwrap_or_default();
        pick_icons.extend(game.refine.as_ref().map(|window| window.icons(items, &game.inventory.items)).unwrap_or_default());
        pick_icons.extend(game.materials.as_ref().map(|window| window.icons(items, &game.inventory.items)).unwrap_or_default());
        pick_icons.extend(game.achievement_window.icons(achievement_info));
        pick_icons.extend(game.roulette.as_ref().map(|window| window.icons(items)).unwrap_or_default());
        let mut trade_icons = game.trade.as_ref().map(|trade| trade.icons(items)).unwrap_or_default();
        trade_icons.extend(game.item_window.as_ref().map(|window| window.images(items)).unwrap_or_default());
        if game.vend_setup.is_some() {
            trade_icons.extend(vend::icons(items, game.cart.items.iter().map(|item| (item.id, item.identified))));
        }
        if let Some(shop) = &game.my_shop {
            trade_icons.extend(vend::icons(items, shop.goods.iter().map(|good| (good.item.id, good.item.identified))));
        }
        trade_icons.extend(game.mail.icons(items));
        for path in skill_icons.chain(status_paths).chain(shop_icons).chain(pick_icons).chain(game.storage.icons(items)).chain(trade_icons) {
            icons.entry(path.clone()).or_insert_with(|| gpu.try_load_texture(grf, &path, false).ok());
        }
        let mut tooltip = Layer::default();
        let mut chat_layer = Layer::default();
        let caret = (game.entered.elapsed().as_millis() / CARET_BLINK.as_millis()).is_multiple_of(2);
        game.chat.draw(&mut chat_layer, &assets.chat, font, messages, caret);
        let size = [game.world.gat.width as u32, game.world.gat.height as u32];
        // NPC marks blink, shown half of each second.
        game.marks.retain(|(.., until)| until.is_none_or(|until| until > now));
        let blink_on = game.entered.elapsed().as_millis() % 1000 > 500;
        let marks: Vec<_> = game.marks.iter().filter(|_| blink_on).map(|(_, cell, color, _)| (*cell, color)).collect();
        // Party members on this map, each in a color of their own, picked at random.
        let here = game.map.trim_end_matches(".gat").to_owned();
        let members: Vec<_> = game
            .party
            .iter()
            .flat_map(|party| party.members.iter())
            .filter(|member| member.info.account != game.account_id && member.info.online && member.info.map.trim_end_matches(".gat") == here)
            .filter_map(|member| Some((member.info.account, member.cell?)))
            .collect();
        for (account, _) in &members {
            game.party_dots.entry(*account).or_insert_with(|| {
                let [r, g, b, ..] = account.wrapping_mul(2_654_435_761).to_le_bytes();
                gpu.solid([r, g, b, 255])
            });
        }
        let dots: Vec<_> = members.iter().map(|(account, cell)| (*cell, &game.party_dots[account])).collect();
        let boss = game.boss.as_ref().map(|(_, cell)| *cell);
        minimap::draw(&mut chat_layer, &assets.minimap, game.minimap.as_ref(), size, game.player.cell, game.player.direction, &marks, &dots, boss);
        // Status icons down the right, each with the clock of its time (orange in the last minute).
        let per_column = ((SCREEN[1] - STATUS_TOP) / STATUS_STEP[1]) as usize;
        let shown = game.status_icons.iter().filter_map(|(kind, start, end)| {
            let (_, file) = status_icon_files.get(kind)?;
            Some((icons.get(&format!("data\\texture\\effect\\{file}"))?.as_ref()?, start, end))
        });
        for (order, (icon, start, end)) in shown.enumerate() {
            let corner = [
                SCREEN[0] - STATUS_RIGHT - STATUS_ICON - (order / per_column) as f32 * STATUS_STEP[0],
                STATUS_TOP + (order % per_column) as f32 * STATUS_STEP[1],
            ];
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
        // An NPC's illustration, over the world and under the windows.
        let mut cutin_layer = Layer::default();
        if let Some((image, place)) = &game.cutin {
            let [width, height] = image.size;
            let corner = match place {
                0 => [0.0, SCREEN[1] - height],
                1 => [((SCREEN[0] - width) / 2.0).floor(), SCREEN[1] - height],
                2 => [SCREEN[0] - width, SCREEN[1] - height],
                _ => [((SCREEN[0] - width) / 2.0).floor(), ((SCREEN[1] - height) / 2.0).floor()],
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
        // Weather over the map, under the names and windows.
        let mut weather_layer = Layer::default();
        if self.effects_on {
            game.weather.draw(&mut weather_layer, &assets.weather, now);
        }
        let mut layers = vec![weather_layer, base, cutin_layer, chat_layer];
        let mut preview = Some(preview);
        let cooling = skills::Cooling { cooldowns: &game.cooldowns, clocks: &assets.clocks[0], now: control::now(), note: message(COOLDOWN_LEFT) };
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
                Panel::Skills => game.skill_window.draw(
                    &mut layer,
                    &mut tooltip,
                    &assets.chrome,
                    &assets.skills,
                    icons,
                    font,
                    messages,
                    (skill_trees, &jobs, skill_info, pc_job_names),
                    &game.skills,
                    game.skill_points(),
                    pointer,
                    &cooling,
                ),
                Panel::BasicInfo => {
                    let info = basic_info::Info {
                        name: &game.me.name,
                        job: pc_job_names.get(&game.me.job).map_or("", String::as_str),
                        status: &game.status,
                        unread_mail: game.mail.unread,
                        reward_waiting: game.achievements.values().any(|achievement| achievement::claimable(Some(achievement))),
                    };
                    game.basic_info.draw(&mut layer, &assets.chrome, &assets.basic_info, font, messages, &info, pointer.0);
                }
                Panel::Inventory => game.inventory.draw(&mut layer, &mut tooltip, &assets.chrome, &assets.inventory, icons, items, messages, font, pointer.0),
                Panel::Cart if game.cart.cart.is_some() => {
                    game.cart.draw(&mut layer, &mut tooltip, &assets.chrome, &assets.inventory, icons, items, messages, font, pointer.0)
                }
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
                ),
            }
            layers.push(layer);
        }
        // A dragged item's or skill's icon follows the pointer.
        let dragged_icon = match game.dragging {
            Some((Dragged::Item(index), _, true)) => {
                game.inventory.get(index).and_then(|item| items.get(&item.id).map(|info| inventory::icon_path(info, item.identified)))
            }
            Some((Dragged::Skill(id), _, true)) => skill_info.get(&id).map(skills::icon_path),
            Some((Dragged::Stored(index), _, true)) => {
                game.storage.get(index).and_then(|item| items.get(&item.id).map(|info| inventory::icon_path(info, item.identified)))
            }
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

        // An aimed skill: its name at the top, "select a target" at the bottom, its level by the pointer. Taming has
        // no skill: the bottom line only.
        let mut aim_lines = Vec::new();
        if let Some((id, level)) = game.targeting {
            aim_lines.push((skill_info.get(&id).map_or_else(|| id.to_string(), |info| info.name.clone()), 45.0));
            tooltip.outlined(&level.to_string(), [pointer.0[0] + 20.0, pointer.0[1] - 18.0]);
        }
        if game.targeting.is_some() || game.capturing {
            aim_lines.push((message(TARGET_MESSAGE).to_owned(), SCREEN[1] - 60.0 - 23.0));
        }
        for (text, top) in aim_lines {
            let width = font.width(&text).ceil() + 14.0;
            let left = ((SCREEN[0] - width) / 2.0).floor();
            tooltip.sprites.push((&assets.target_fill, [left, top], [width, 23.0]));
            tooltip.texts.push((text.clone(), [left + 8.0, top + 6.0], [0, 0, 0]));
            tooltip.texts.push((text, [left + 7.0, top + 5.0], [0x00, 0xFF, 0x00]));
        }
        // A shop's windows over the others; its question in a message box.
        let mut shop_layer = Layer::default();
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
            room.draw(&mut shop_layer, &assets.chrome, font, pointer.0, caret);
        }
        if let Some(setup) = &game.vend_setup {
            setup.draw(&mut shop_layer, &assets.chrome, &assets.vend, icons, items, &game.cart.items, font, messages, pointer, caret);
        }
        if let Some(shop) = &game.my_shop {
            shop.draw(&mut shop_layer, &assets.chrome, &assets.vend, icons, items, font, messages, pointer);
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
            );
        }
        if let Some(window) = &game.refine {
            window.draw(&mut shop_layer, &assets.chrome, &assets.refine, icons, items, &game.inventory.items, messages, font, pointer);
        }
        if let Some(window) = &game.materials {
            window.draw(&mut shop_layer, &assets.chrome, &assets.pick_list, icons, items, &game.inventory.items, messages, font, pointer);
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
        // A guild invites us.
        if let Some((_, name)) = &game.guild_invite {
            let dialog = dialog_position(&assets.message);
            shop_layer.sprites.push((&assets.message, dialog, assets.message.size));
            shop_layer.sprites.push(button_sprite(pointer, offset(dialog, LEFT_BUTTON), &assets.ok));
            shop_layer.sprites.push(button_sprite(pointer, offset(dialog, RIGHT_BUTTON), &assets.cancel));
            let question = format!("({name}){}", message(GUILD_INVITED));
            for (row, line) in message_lines(font, &question, MESSAGE_WIDTH).into_iter().enumerate() {
                shop_layer.texts.push((line, offset(dialog, [MESSAGE_TEXT[0], MESSAGE_TEXT[1] + row as f32 * font.line_height()]), TEXT_COLOR));
            }
        }
        // Growing the inventory (which voucher, from how many slots to how many) or a pet question, in the message's
        // colors.
        let colored_question = match (game.expansion_asked, &game.pet_asked) {
            _ if game.dismiss_asked => Some(message(mercenary::DISMISS_QUESTION).to_owned()),
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
        if let Some(menu) = &game.escape {
            menu.draw(&mut shop_layer, &assets.chrome, &assets.escape, messages, pointer);
        }
        if let Some(window) = &game.audio_window {
            window.draw(&mut shop_layer, &assets.chrome, &assets.options, font, messages, sound, pointer.0);
        }
        let mut menu_layer = Layer::default();
        if let Some(menu) = &game.menu {
            menu.draw(&mut menu_layer, &assets.context_menu, font, pointer.0);
        }
        let mut world_layer = Layer::default();
        if let Some(world_map) = &game.world_map {
            world_map.draw(&mut world_layer, &assets.chrome, font, &worlds.0, map_names, &map_key(&game.map), pointer.0);
        }
        layers.extend([shop_layer, overlay, menu_layer, tooltip, world_layer]);
        let composed = compose(gpu, font, layers);
        let mut sprites = flatten(&composed);

        // The cursor shows what a click would do.
        let on_window = game.on_ui(pointer.0, items);
        let ground = view
            .pick([pointer.0[0] / SCREEN[0], pointer.0[1] / SCREEN[1]], player_feet[1])
            .map(|point| [(point[0] / world::CELL).floor() as i32, (point[2] / world::CELL).floor() as i32]);
        let near_warp = |cell: [i32; 2]| {
            game.units.values().any(|actor| actor.job == WARP_JOB && (actor.cell[0] - cell[0]).abs() <= 1 && (actor.cell[1] - cell[1]).abs() <= 1)
        };
        cursor_look.set(if game.camera_goal.dragging.is_some() {
            cursor::Kind::Rotate
        } else if (game.targeting.is_some() || game.capturing) && !on_window {
            cursor::Kind::Target
        } else if on_window {
            cursor::Kind::Default
        } else {
            match hovered(&game.hit_boxes, pointer.0, players_first) {
                Some(Owner::Unit(id)) => match game.units.get(&id).map(|actor| actor.kind) {
                    Some(MONSTER) => cursor::Kind::Attack,
                    Some(NPC) => cursor::Kind::Talk,
                    _ => cursor::Kind::Default,
                },
                Some(Owner::Item(_)) => cursor::Kind::Pick,
                _ => match ground {
                    Some(cell) if near_warp(cell) => cursor::Kind::Warp,
                    Some(cell) if !game.world.gat.walkable(cell) => cursor::Kind::NoWalk,
                    _ => cursor::Kind::Default,
                },
            }
        });
        if let Some(sprite) = &game.slot_sprite {
            let middle = [SCREEN[0] / 2.0, SCREEN[1] / 2.0];
            sprites.extend(slot_frame.into_iter().map(|(key, offset, size)| (sprite.texture(key), [middle[0] + offset[0], middle[1] + offset[1]], size)));
        }
        sprites.extend(cursor_look.draw(gpu, pointer.0));

        game.effects.advance(gpu, now, yaw);
        // With /effect off, effects still play on, unseen, and their sounds come due without being heard.
        let effects_on = self.effects_on;
        if let Some(audio) = audio {
            for (file, at) in game.effects.due_sounds(now).into_iter().filter(|_| effects_on) {
                let distance = (at[0] - player_feet[0]).abs().max((at[2] - player_feet[2]).abs()) / world::CELL;
                audio.play(grf, &file, 1.0 - distance / HEARING_CELLS);
            }
        }
        let feet_of_unit = |unit: Option<u32>| match unit {
            None => Some(player_feet),
            Some(id) => feet_of.get(&id).copied(),
        };
        let (effect_quads, effect_meshes) =
            if effects_on { (game.effects.quads(now, feet_of_unit), game.effects.meshes(now, feet_of_unit)) } else { (Vec::new(), Vec::new()) };
        let frame = WorldFrame {
            world: &game.world,
            view: &view,
            billboards: &billboards,
            shadows,
            effects: &effect_quads,
            meshes: &effect_meshes,
            time: now.duration_since(game.entered).as_secs_f32(),
        };
        let shot_delay = std::env::var("RUSTNAROK_CLIENT_SHOT_DELAY").ok().and_then(|seconds| seconds.parse().ok()).unwrap_or(2.0);
        match &self.screenshot {
            Some(path) if game.entered.elapsed().as_secs_f32() > shot_delay => {
                gpu.capture(Some(&frame), &sprites, path);
                event_loop.exit();
            }
            _ => gpu.draw(Some(&frame), &sprites),
        }
    }
}

/// A layer with its text rasterized.
type Composed<'a> = (Vec<Sprite<'a>>, Vec<(Texture, [f32; 2])>);

/// Rasterizes each layer's text.
fn compose<'a>(gpu: &Gpu, font: &Font, layers: Vec<Layer<'a>>) -> Vec<Composed<'a>> {
    layers
        .into_iter()
        .map(|layer| {
            let texts = layer.texts.iter().map(|(text, position, color)| (gpu.text(font, text, *color), *position)).collect();
            (layer.sprites, texts)
        })
        .collect()
}

/// Everything to draw, in order: each layer's sprites, then its text, over the layers before it.
fn flatten<'a>(composed: &'a [Composed<'a>]) -> Vec<Sprite<'a>> {
    composed
        .iter()
        .flat_map(|(sprites, texts)| sprites.iter().copied().chain(texts.iter().map(|(texture, position)| (texture, *position, texture.size))))
        .collect()
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
        let attributes = Window::default_attributes()
            .with_title("Ragnarok")
            .with_inner_size(LogicalSize::new(SCREEN[0], SCREEN[1]))
            .with_min_inner_size(LogicalSize::new(SCREEN[0] / 2.0, SCREEN[1] / 2.0));
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
            // Screen pixels to the canvas, through its place in the window (black bars in fullscreen).
            WindowEvent::CursorMoved { position, .. } => {
                let [left, top, width, height] = render::viewport([gpu.config.width, gpu.config.height]);
                gpu.window.request_redraw();
                self.cursor = [(position.x as f32 - left) * SCREEN[0] / width, (position.y as f32 - top) * SCREEN[1] / height];
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
                if game.entered.elapsed() > Duration::from_secs(1) {
                    if let Some(cell) = game.auto_walk.take() {
                        let _ = game.session.walk(cell);
                    }
                    // Through the chat line, so the client's commands (/organize, %) work too.
                    let say_at = std::env::var("RUSTNAROK_CLIENT_SAY_AT").ok().and_then(|seconds| seconds.parse().ok()).unwrap_or(1.0);
                    if let Some(texts) = game.auto_say.take_if(|_| game.entered.elapsed().as_secs_f32() >= say_at) {
                        for text in texts.split('|') {
                            game.chat.input = text.to_owned();
                            let _ = game.send_chat(false, false, &self.messages, &mut self.effects_on);
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
                        for whisper in whispers.split(';') {
                            let (to, text) = whisper.split_once('|').unwrap_or((whisper, "hi"));
                            game.chat.whisper = to.to_owned();
                            game.chat.input = text.to_owned();
                            let _ = game.send_chat(false, false, &self.messages, &mut self.effects_on);
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
                        game.pick_up(id);
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
                if let Some((pending, Some(arrived))) = game.pending
                    && arrived.elapsed() >= ATTACK_SETTLE
                {
                    game.pending = None;
                    match pending {
                        Pending::Attack(target) => game.attack(target),
                        Pending::PickUp(id) => game.pick_up(id),
                        Pending::Skill { id, level, target } => {
                            game.targeting = Some((id, level));
                            game.aim_skill(target);
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
    let title_names = lua::titles(&grf).unwrap_or_default();
    let job_names = lua::job_names(&grf).unwrap_or_else(|error| {
        eprintln!("no NPC/monster sprite names: {error}");
        HashMap::new()
    });
    let items = lua::item_info(&grf).unwrap_or_else(|error| {
        eprintln!("no item names: {error}");
        HashMap::new()
    });
    let messages = messages::load(&grf).unwrap_or_else(|error| {
        eprintln!("no client messages: {error}");
        Vec::new()
    });
    let indoor = rustnarok_client::map::indoor_maps(&grf).unwrap_or_default();
    let pc_job_names = lua::pc_job_names(&grf).unwrap_or_default();
    let map_names = rustnarok_client::map::map_names(&grf).unwrap_or_default();
    let skill_info = lua::skill_info(&grf).unwrap_or_else(|error| {
        eprintln!("no skill names: {error}");
        HashMap::new()
    });
    let skill_trees = lua::skill_trees(&grf).unwrap_or_default();
    let status_icon_files = lua::status_icons(&grf).unwrap_or_default();
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
    let mut app = App {
        grf,
        job_names,
        pets,
        achievement_info,
        title_names,
        items,
        messages,
        icons: HashMap::new(),
        modifiers: ModifiersState::empty(),
        indoor,
        look_tables,
        pc_job_names,
        map_names,
        worlds,
        quest_texts,
        skill_info,
        skill_trees,
        status_icon_files,
        effect_table: effects::Table::load(),
        // Driven or screenshot runs stay quiet unless RUSTNAROK_CLIENT_SOUND=1.
        audio: (std::env::var("RUSTNAROK_CLIENT_SOUND").is_ok_and(|sound| sound == "1")
            || std::env::var_os("RUSTNAROK_CLIENT_SCREENSHOT").is_none() && std::env::var_os("RUSTNAROK_CLIENT_CONTROL").is_none())
        .then(|| sound::Audio::new(&sound))
        .flatten(),
        sound,
        music,
        effects_on: true,
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn shows_npc_links_by_name() {
        let navi = "Let me mark the ^B9062F<NAVI>[Tool Shop]<INFO>prontera,134,221,0,000,0</INFO></NAVI>^000000 for you.";
        assert_eq!(show_links(navi), "Let me mark the ^B9062F^C00000[Tool Shop]^B9062F^000000 for you.");
        assert_eq!(show_links("a <ITEM>Apple<INFO>512</INFO></ITEM> < b"), "a ^0070C0Apple^000000 < b");
    }

    #[test]
    fn turns_the_camera_only_while_the_right_button_is_down() {
        let mut goal = CameraGoal::new(false);
        let none = ModifiersState::empty();
        goal.drag([100.0, 100.0], none);
        goal.drag([200.0, 100.0], none);
        assert_eq!((goal.yaw, goal.dragging), (0.0, None));
        goal.press([200.0, 100.0], none);
        goal.drag([300.0, 100.0], none);
        assert!(goal.yaw > 0.0);
    }
}
