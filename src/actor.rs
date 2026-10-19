//! Characters: layered player sprites (body + head with dye palettes) and movement along a path of cells.

use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use image::RgbaImage;
use rustnarok_client::grf::Grf;
use rustnarok_client::lua::LookTables;
use rustnarok_client::sprite::{self, Act, Palette, Pick, Spr};

use crate::render::{Gpu, Texture};
use crate::world;

/// Sprite actions: each has 8 directions. Indices by kind of unit.
pub const IDLE: usize = 0;
pub const WALK: usize = 1;

/// The status (EFST_ON_PUSH_CART) a unit pushing a cart has; its value is the cart's type.
pub const PUSH_CART: u16 = 673;
/// EFST_CHANGE_SIZE and EFST_CHANGE_SIZE_MONSTER (status.hpp). val1 on ZC_MSG_STATE_CHANGE3 is the size in percent.
pub const CHANGE_SIZE: u16 = 1420;
pub const CHANGE_SIZE_MONSTER: u16 = 1421;

/// Jobs of the Super Novice line, whose cart is always their own.
const SUPER_NOVICE_JOBS: [u16; 4] = [23, 4045, 4190, 4191];

/// Baby jobs (rathena's JOB_BABY… and JOB_SUPER_BABY…), whose carts move by a baby cart's ACT.
fn baby_job(job: u16) -> bool {
    matches!(job, 4023..=4045 | 4096..=4112 | 4191 | 4220 | 4222..=4229 | 4238 | 4241 | 4242 | 4244)
}

// ponytail: types 10 to 13 follow the names (손수레9 …), which this data doesn't have; Hyper Novice's cart isn't known.
/// The sprite (without extension) and ACT of a pushcart type for a job: 손수레, 손수레1 … by type, the Super Novice's
/// 슈노손수레 whatever the type; a baby's by its own ACT (베이비손수레…, with the grown-up's sprite) or, for types 6 to
/// 9, a sprite of its own (손수레5베이비 …).
pub fn cart_path(kind: i32, job: u16) -> (String, String) {
    let folder = "data\\sprite\\이팩트\\";
    let baby = baby_job(job);
    let name = if SUPER_NOVICE_JOBS.contains(&job) {
        "슈노손수레".to_string()
    } else if kind > 1 {
        format!("손수레{}", kind - 1)
    } else {
        "손수레".to_string()
    };
    let (sprite, act) = match name.strip_prefix("손수레").and_then(|number| number.parse::<i32>().ok()) {
        Some(5..) if baby => (format!("{name}베이비"), format!("{name}베이비")),
        _ if baby => (name.clone(), format!("베이비{name}")),
        _ => (name.clone(), name),
    };
    (format!("{folder}{sprite}"), format!("{folder}{act}.act"))
}

/// Players only.
pub const SIT: usize = 2;
pub const PICKUP: usize = 3;
/// A player's battle stance, weapon drawn (most weapons only show here and while attacking).
pub const READY: usize = 4;

/// Attack, flinch and death actions for a unit's object type (players vs. monsters and the rest).
pub fn combat_actions(kind: u8) -> (usize, usize, usize) {
    match kind {
        // ponytail: players always use their first attack motion; the weapon decides it in the client.
        // Mercenaries (9) are laid out like players.
        0 | 9 => (5, 6, 8),
        _ => (2, 3, 4),
    }
}

/// How far into an action a frame is drawn.
#[derive(Clone, Copy)]
pub enum Timing {
    /// Milliseconds into a looping action (standing), at the ACT's own frame delays.
    Loop(f32),
    /// Fraction (0..1) of a one-shot action stretched over a given duration (a skill's pose, a pickup).
    Span(f32),
    /// Milliseconds into a one-shot action at its ACT pace times a scale (attack, flinch, death), the last frame held.
    Once(f32, f32),
    /// World units walked so far: legs move with the distance walked, not the time.
    Walk(f32),
}
/// ACT delays are in units of 24 ms.
const DELAY_UNIT_MS: f32 = 24.0;
/// Walking frames per world unit walked, times the ACT delay.
const WALK_FRAMES: f32 = 1.48;
/// An attack plays at its ACT pace times min(attackMT, ATTACK_PACE) / ATTACK_PACE (a bow, uncapped).
const ATTACK_PACE: f32 = 432.0;
/// A player's swing with a bow lands this much after its hit frame: the arrow's flight.
const BOW_FLIGHT_MS: f32 = 192.0;

/// When a swing's hit lands, `hit_ms` being its hit frame: with a `bow`, the arrow's flight later.
pub fn bow_flight(hit_ms: f32, bow: bool) -> f32 {
    if bow { hit_ms + BOW_FLIGHT_MS } else { hit_ms }
}

/// The frame of `count` a looping action shows `ms` in, at an ACT delay of `delay` units.
pub fn loop_frame(ms: f32, delay: f32, count: usize) -> usize {
    (ms / (delay * DELAY_UNIT_MS)) as usize % count.max(1)
}

/// The frame a one-shot action shows `ms` in: as looping, then its last frame.
pub fn once_frame(ms: f32, delay: f32, count: usize) -> usize {
    ((ms / (delay * DELAY_UNIT_MS)) as usize).min(count.max(1) - 1)
}

/// The walking frame after `distance` world units walked (a cell is 5).
pub fn walk_frame(distance: f32, delay: f32, count: usize) -> usize {
    (distance * WALK_FRAMES / delay) as usize % count.max(1)
}

/// How much an attack's ACT pace is stretched for an attack motion of `attack_ms` (ZC_NOTIFY_ACT's source speed):
/// capped at ATTACK_PACE, except for players shooting a bow. An attack motion of 0 (Arrow Shower's) plays at the ACT's
/// own pace.
pub fn attack_scale(attack_ms: i32, bow: bool) -> f32 {
    if attack_ms <= 0 {
        return 1.0;
    }
    let motion = attack_ms as f32;
    (if bow { motion } else { motion.min(ATTACK_PACE) }) / ATTACK_PACE
}

/// A job's sprite names: its body (also its .imf and garment motions), its weapons' folder and file prefix, and its
/// body dyes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JobSprites {
    pub body: &'static str,
    pub weapon: &'static str,
    pub palette: &'static str,
}

/// The sprite names of a job (src/job_sprites.txt), mounted and costume bodies included.
pub fn job_sprites(job: u16) -> Option<JobSprites> {
    static TABLE: LazyLock<HashMap<u16, JobSprites>> = LazyLock::new(|| parse_job_sprites(include_str!("job_sprites.txt")));
    TABLE.get(&job).copied()
}

/// Lines of job, body, weapon and palette names, tab-separated; `#` starts a comment line.
fn parse_job_sprites(text: &'static str) -> HashMap<u16, JobSprites> {
    text.lines()
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let job = fields.next()?.parse().ok()?;
            Some((job, JobSprites { body: fields.next()?, weapon: fields.next()?, palette: fields.next()? }))
        })
        .collect()
}

/// Option bits (rathena's OPTION_*) that put a player in another body: outfits, mounts and the Star Gladiator's
/// union.
const WEDDING: u32 = 0x1000;
const UNION: u32 = 0x8000;
const XMAS: u32 = 0x1_0000;
const SUMMER: u32 = 0x4_0000;
const HANBOK: u32 = 0x800_0000;
const OKTOBERFEST: u32 = 0x1000_0000;
const RIDING: u32 = 0x20;
/// The five dragon colours.
const DRAGONS: [u32; 5] = [0x8_0000, 0x80_0000, 0x100_0000, 0x200_0000, 0x400_0000];
const WOLF_RIDING: u32 = 0x20_0000;
const MADOGEAR: u32 = 0x40_0000;
/// The mounts that swap the body for a mounted one.
const MOUNTS: u32 = RIDING | DRAGONS[0] | DRAGONS[1] | DRAGONS[2] | DRAGONS[3] | DRAGONS[4] | WOLF_RIDING | MADOGEAR;
/// rathena `OPTION_FALCON` (`src/map/status.hpp`).
pub const OPTION_FALCON: u32 = 0x10;
/// rathena `OPTION_WUG`.
pub const OPTION_WUG: u32 = 0x10_0000;
/// rathena `OPTION_WUGRIDER`: the wolf the player is riding. Same bit as the mount body.
pub const OPTION_WUGRIDER: u32 = WOLF_RIDING;
/// rathena's OPTION_HIDE, OPTION_CLOAK and OPTION_CHASEWALK: the unit is out of sight.
pub const UNSEEN_OPTIONS: u32 = OPTION_HIDE | OPTION_CLOAK | OPTION_CHASEWALK;
/// rathena's option bits that keep a unit out of sight: Hiding, Cloaking, a GM's @hide, Chase Walk.
pub const OPTION_HIDE: u32 = 0x2;
pub const OPTION_CLOAK: u32 = 0x4;
pub const OPTION_INVISIBLE: u32 = 0x40;
pub const OPTION_CHASEWALK: u32 = 0x4000;
/// rathena OPTION_ORCISH: the head is the orc face.
pub const OPTION_ORCISH: u32 = 0x800;
const HIDDEN_OPTIONS: u32 = UNSEEN_OPTIONS | OPTION_INVISIBLE;
/// rathena EFST_CLAIRVOYANCE. While it is on, hidden units show as silhouettes.
pub const CLAIRVOYANCE: u16 = 184;
/// rathena EF_FOOT and EF_FOOT2 (EF_HIT1 is 0, so these are 326 and 327): a chase walk's footprints.
pub const EF_FOOT: u32 = 326;
pub const EF_FOOT2: u32 = 327;

/// How much of a unit shows.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Seen {
    Whole,
    /// Its shadow only: our own character in Hiding.
    Shadow,
    /// A black figure, no shadow: someone hidden while Clairvoyance is on.
    Silhouette,
    Nothing,
}

/// What shows of a unit with these option bits. Others in Hiding, Cloaking or Chase Walk are silhouettes while
/// `clairvoyance` (status 184) is on; GM hide shows nothing. Our own character in Hiding is a shadow, and is never
/// a silhouette.
// ponytail: clairvoyance is taken from our own character's status icons.
pub fn seen(option: u32, own: bool, clairvoyance: bool) -> Seen {
    if option & HIDDEN_OPTIONS == 0 {
        Seen::Whole
    } else if own && option & (OPTION_CLOAK | OPTION_INVISIBLE | OPTION_CHASEWALK) == 0 {
        Seen::Shadow
    } else if !own && clairvoyance && option & OPTION_INVISIBLE == 0 && option & UNSEEN_OPTIONS != 0 {
        Seen::Silhouette
    } else {
        Seen::Nothing
    }
}

/// The footprint a chase walk leaves on step `n` (0 the first): 326, then 327.
// ponytail: they alternate, one per cell stepped, and only our own walker leaves them (others don't see the walker,
// so they don't see our footprints). A new chase starts again at 326.
pub fn chase_footprint(step: u32) -> u32 {
    if step.is_multiple_of(2) { EF_FOOT } else { EF_FOOT2 }
}

/// The option bits that change what a player wears (`Look::option`): outfits, mounts and the orcish face.
pub const BODY_OPTIONS: u32 = WEDDING
    | UNION
    | XMAS
    | SUMMER
    | HANBOK
    | OKTOBERFEST
    | RIDING
    | DRAGONS[0]
    | DRAGONS[1]
    | DRAGONS[2]
    | DRAGONS[3]
    | DRAGONS[4]
    | WOLF_RIDING
    | MADOGEAR
    | OPTION_ORCISH;

/// The job whose body a player of `job` is drawn with under `option`: a wedding or holiday outfit (not in the union),
/// else its mount's or union's body, else its own.
pub fn sprite_job(job: u16, option: u32) -> u16 {
    if option & WEDDING != 0 {
        return 22;
    }
    if option & UNION == 0
        && let Some(&(_, outfit)) = [(XMAS, 26), (SUMMER, 27), (HANBOK, 28), (OKTOBERFEST, 29)].iter().find(|(bit, _)| option & bit != 0)
    {
        return outfit;
    }
    let bodies: [(u32, &[(u16, u16)]); 9] = [
        (RIDING, &[(7, 13), (14, 21), (4008, 4014), (4015, 4022), (4030, 4036), (4037, 4044), (4066, 4082), (4073, 4083), (4102, 4110), (4258, 4281)]),
        (DRAGONS[0], &[(4054, 4080), (4060, 4081), (4096, 4109), (4252, 4280)]),
        (DRAGONS[1], &[(4054, 4088), (4060, 4089)]),
        (DRAGONS[2], &[(4054, 4090), (4060, 4091)]),
        (DRAGONS[3], &[(4054, 4092), (4060, 4093)]),
        (DRAGONS[4], &[(4054, 4094), (4060, 4095)]),
        (WOLF_RIDING, &[(4056, 4084), (4062, 4085), (4098, 4111), (4257, 4278)]),
        (MADOGEAR, &[(4058, 4086), (4064, 4087), (4100, 4112), (4253, 4279)]),
        (UNION, &[(4047, 4048), (4226, 4238), (4239, 4243), (4241, 4244), (4302, 4316)]),
    ];
    bodies.iter().filter(|(bit, _)| option & bit != 0).flat_map(|(_, pairs)| pairs.iter()).find(|(from, _)| *from == job).map_or(job, |&(_, to)| to)
}

/// Outfit bodies (wedding and holidays) show no weapon or shield.
fn outfit(job: u16) -> bool {
    matches!(job, 22 | 26..=30)
}

/// How big a player of `job` is drawn: babies smaller (Doram babies 0.8, the first baby jobs 0.75, the rest 0.82).
fn body_scale(job: u16) -> f32 {
    let baby = matches!(
        job,
        4023..=4045 | 4096..=4112 | 4158..=4182 | 4191 | 4193..=4196 | 4205..=4210 | 4220 | 4222..=4238 | 4241 | 4242 | 4244 | 4247 | 4248 | 4352 | 4354
    );
    match job {
        _ if !baby => 1.0,
        _ if is_doram(job) => 0.8,
        4023..=4029 | 4045 | 4191 => 0.75,
        _ => 0.82,
    }
}

/// Doram jobs live under 도람족 instead of 인간족.
pub fn is_doram(job: u16) -> bool {
    matches!(job, 4217..=4221 | 4308 | 4315)
}

/// The head sprite under `data\sprite\` when `option` has the orcish bit. None keeps the hair.
pub fn orcish_head(job: u16, option: u32) -> Option<&'static str> {
    (option & OPTION_ORCISH != 0).then_some(if is_doram(job) { r"이팩트\orcface_doram" } else { r"이팩트\orcface" })
}

/// The glow sprite under `data\sprite\` for a mercenary of `job`. Archers, and jobs outside 6017–6046, have none.
pub fn mercenary_glow(job: u16) -> Option<&'static str> {
    match job {
        6027..=6036 => Some(r"인간족\용병\창용병_창_검광"),
        6037..=6046 => Some(r"인간족\용병\검용병_검_검광"),
        _ => None,
    }
}

struct Layer {
    spr: Spr,
    act: Act,
    palette: Option<Palette>,
    part: Part,
}

/// What a sprite layer is on a character.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Part {
    Body,
    Head,
    HeadBottom,
    HeadMid,
    HeadTop,
    Weapon,
    /// The weapon's glow (`_검광`), right over it.
    WeaponGlow,
    Shield,
    Robe,
}

impl Part {}

/// How a character looks: job, hair and the view IDs of what it wears.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Look {
    pub job: u16,
    pub sex: u8,
    pub hair: u16,
    pub hair_color: u16,
    pub clothes_color: u16,
    pub weapon: u32,
    pub shield: u32,
    pub head_top: u16,
    pub head_mid: u16,
    pub head_bottom: u16,
    pub robe: u16,
    /// The option bits that change the body (`BODY_OPTIONS`): outfits, mounts and the orcish face.
    pub option: u32,
}

/// A character's head alone, in their hair colour: the first standing frame facing front (the big minimap's player).
pub fn head_image(grf: &Grf, look: &Look) -> Option<RgbaImage> {
    let sex = if look.sex == 0 { "여" } else { "남" };
    let race = if is_doram(look.job) { "도람족" } else { "인간족" };
    let palettes = if is_doram(look.job) { "data\\palette\\도람족\\머리" } else { "data\\palette\\머리" };
    let hair = look.hair.max(1);
    let path = format!("data\\sprite\\{race}\\머리통\\{sex}\\{hair}_{sex}");
    let spr = Spr::parse(&grf.read(&format!("{path}.spr")).ok()?).ok()?;
    let act = Act::parse(&grf.read(&format!("{path}.act")).ok()?).ok()?;
    let palette = (look.hair_color > 0)
        .then(|| grf.read(&format!("{palettes}\\머리{hair}_{sex}_{}.pal", look.hair_color)).ok().and_then(|bytes| sprite::parse_palette(&bytes)))
        .flatten();
    let layer = act.actions.first()?.first()?.layers.iter().find(|layer| layer.image >= 0)?;
    spr.image(layer.kind, layer.image as usize, palette.as_ref())
}

/// The cell a drawn position (in cells, centres on the integers) is on: floor(p + 0.5) per axis.
pub fn drawn_cell(position: [f32; 2]) -> [i32; 2] {
    position.map(|value| (value + 0.5).floor() as i32)
}

/// The attack motion for a player's job and weapon type: action 5, 10 or 11.
pub fn attack_action(job: u16, sex: u8, weapon_type: u32) -> usize {
    // Weapon types (the client's Weapon_IDs, rathena's weapon_type).
    const DAGGER: u32 = 1;
    const SWORD: u32 = 2;
    const TWO_HAND_SWORD: u32 = 3;
    const SPEAR: u32 = 4;
    const TWO_HAND_SPEAR: u32 = 5;
    const AXE: u32 = 6;
    const TWO_HAND_AXE: u32 = 7;
    const MACE: u32 = 8;
    const TWO_HAND_MACE: u32 = 9;
    const ROD: u32 = 10;
    const BOW: u32 = 11;
    const KNUCKLE: u32 = 12;
    const INSTRUMENT: u32 = 13;
    const WHIP: u32 = 14;
    const BOOK: u32 = 15;
    const KATAR: u32 = 16;
    const REVOLVER: u32 = 17;
    const RIFLE: u32 = 18;
    const GATLING: u32 = 19;
    const SHOTGUN: u32 = 20;
    const GRENADE: u32 = 21;
    const HUUMA: u32 = 22;
    const TWO_HAND_ROD: u32 = 23;
    let female = sex == 0;
    let w = weapon_type;
    let blade_or_blunt = |w: u32| matches!(w, SWORD | TWO_HAND_SWORD | AXE | TWO_HAND_AXE | MACE | TWO_HAND_MACE);
    let rod = |w: u32| matches!(w, ROD | TWO_HAND_ROD);
    // Rebirth and third jobs swing like the class they grew from.
    let base = match job {
        4001..=4007 => job - 4001,
        4008 => 7,
        4009 => 8,
        4010 => 9,
        4011 => 10,
        4012 => 11,
        4013 => 12,
        4014 => 13,
        4015 => 14,
        4016 => 15,
        4017 => 16,
        4018 => 17,
        4019 => 18,
        4020 => 19,
        4021 => 20,
        4054 => 7,
        4055 => 9,
        4056 => 11,
        4057 => 8,
        4058 => 10,
        4059 => 12,
        4066 => 14,
        4067 => 16,
        4068 => 19,
        4069 => 20,
        4070 => 15,
        4071 => 18,
        4072 => 17,
        // Soul Linkers swing like wizards.
        4049 => 27,
        job => job,
    };
    let motion = match base {
        0 if female => {
            if w == DAGGER {
                2
            } else if rod(w) || blade_or_blunt(w) {
                1
            } else {
                0
            }
        }
        0 => {
            if w == DAGGER {
                1
            } else if rod(w) || blade_or_blunt(w) {
                2
            } else {
                0
            }
        }
        1 | 7 | 13 | 14 | 21 => {
            if matches!(w, SPEAR | TWO_HAND_SPEAR) {
                2
            } else if w == DAGGER || blade_or_blunt(w) {
                1
            } else {
                0
            }
        }
        2 => {
            if rod(w) {
                1
            } else if w == DAGGER {
                2
            } else {
                0
            }
        }
        3 => {
            if w == BOW {
                1
            } else if w == DAGGER {
                2
            } else {
                0
            }
        }
        4 => {
            if rod(w) || matches!(w, MACE | TWO_HAND_MACE) {
                1
            } else {
                0
            }
        }
        5 => {
            if w == DAGGER {
                2
            } else if blade_or_blunt(w) {
                1
            } else {
                0
            }
        }
        6 | 17 => {
            if w == BOW {
                2
            } else if matches!(w, SWORD | TWO_HAND_SWORD | DAGGER) {
                1
            } else {
                0
            }
        }
        8 => {
            if w == BOOK {
                2
            } else if rod(w) || matches!(w, MACE | TWO_HAND_MACE) {
                1
            } else {
                0
            }
        }
        9 | 27 if female => {
            if rod(w) {
                2
            } else if w == DAGGER {
                1
            } else {
                0
            }
        }
        9 | 27 => {
            if w == DAGGER {
                2
            } else if rod(w) {
                1
            } else {
                0
            }
        }
        10 | 18 => {
            if w == DAGGER {
                1
            } else if blade_or_blunt(w) {
                2
            } else {
                0
            }
        }
        11 => {
            if w == BOW {
                2
            } else if w == DAGGER {
                1
            } else {
                0
            }
        }
        12 => {
            if w == KATAR || w >= 25 {
                2
            } else if matches!(w, AXE | SWORD | DAGGER) {
                1
            } else {
                0
            }
        }
        15 => {
            if w == KNUCKLE {
                2
            } else if rod(w) || matches!(w, MACE | TWO_HAND_MACE) {
                1
            } else {
                0
            }
        }
        16 => {
            if rod(w) || w == BOOK {
                2
            } else if w == DAGGER {
                1
            } else {
                0
            }
        }
        19 => {
            if w == BOW {
                2
            } else if matches!(w, DAGGER | INSTRUMENT) {
                1
            } else {
                0
            }
        }
        20 => {
            if w == BOW {
                2
            } else if w == WHIP {
                1
            } else {
                0
            }
        }
        23 if female => {
            if w == DAGGER {
                2
            } else if rod(w) || matches!(w, AXE | TWO_HAND_AXE | MACE | TWO_HAND_MACE | SWORD) {
                1
            } else {
                0
            }
        }
        23 => {
            if w == DAGGER {
                1
            } else if rod(w) || matches!(w, AXE | TWO_HAND_AXE | MACE | TWO_HAND_MACE | SWORD) {
                2
            } else {
                0
            }
        }
        25 => {
            if w == HUUMA {
                2
            } else if w == DAGGER {
                1
            } else {
                0
            }
        }
        24 => {
            if matches!(w, GATLING | RIFLE | GRENADE) {
                2
            } else if matches!(w, 0 | REVOLVER | SHOTGUN) {
                1
            } else {
                0
            }
        }
        _ => 0,
    };
    [5, 10, 11][motion]
}

/// Identifies a decoded image: layer, image kind, index, the layer's tint, and how it was turned ahead of time for a
/// flat preview (`SpriteSet::flat_frame`): 0 upright, else degrees clockwise, plus 360 when mirrored first.
type ImageKey = (usize, u32, usize, [u8; 4], u16);

/// One image of a frame: which image, top-left offset from the feet, size (negative width = mirrored), and degrees
/// to turn it clockwise about its middle.
pub type Placed = (ImageKey, [f32; 2], [f32; 2], f32);

/// An image turned `degrees` clockwise about its middle (mirrored left to right first when `mirror`), on a canvas just
/// big enough for it; pixels picked nearest, the rest clear.
fn turn_image(image: &RgbaImage, degrees: i32, mirror: bool) -> RgbaImage {
    let (sin, cos) = (degrees as f32).to_radians().sin_cos();
    let [width, height] = [image.width() as f32, image.height() as f32];
    let size = [(width * cos.abs() + height * sin.abs()).round().max(1.0), (width * sin.abs() + height * cos.abs()).round().max(1.0)];
    RgbaImage::from_fn(size[0] as u32, size[1] as u32, |x, y| {
        let [dx, dy] = [x as f32 + 0.5 - size[0] / 2.0, y as f32 + 0.5 - size[1] / 2.0];
        // Turned back: where this pixel came from.
        let [source_x, source_y] = [dx * cos + dy * sin + width / 2.0, -dx * sin + dy * cos + height / 2.0];
        if source_x < 0.0 || source_y < 0.0 || source_x >= width || source_y >= height {
            return image::Rgba([0; 4]);
        }
        let column = source_x as u32;
        *image.get_pixel(if mirror { image.width() - 1 - column } else { column }, source_y as u32)
    })
}

/// rathena's OPT1_* (status.hpp): the body states that colour a sprite.
const OPT1_STONE: u16 = 1;
const OPT1_FREEZE: u16 = 2;
const OPT1_STONEWAIT: u16 = 6;
/// rathena's OPT2_* bits (status.hpp).
const OPT2_POISON: u16 = 0x0001;
const OPT2_CURSE: u16 = 0x0002;
const OPT2_DPOISON: u16 = 0x0080;

/// The RGB a unit's whole sprite is multiplied by. A body state wins over a health state.
/// None when the state has no colour, or it has ended.
pub fn status_tint(body: u16, health: u16) -> Option<[u8; 3]> {
    match body {
        OPT1_STONE => Some([64, 64, 64]),
        OPT1_FREEZE => Some([0, 128, 255]),
        OPT1_STONEWAIT => Some([128, 128, 128]),
        // ponytail: stacked health bits take deadly poison, then curse over poison.
        _ if health & OPT2_DPOISON != 0 => Some([255, 50, 255]),
        _ if health & OPT2_CURSE != 0 => Some([200, 50, 50]),
        _ if health & OPT2_POISON != 0 => Some([255, 128, 255]),
        _ => None,
    }
}

/// Whether a body state (stone, frozen) stops the unit's animation where it is.
pub fn stilled(body: u16) -> bool {
    matches!(body, OPT1_STONE | OPT1_FREEZE)
}

/// Multiplies a layer's RGB by `tint` and keeps its alpha.
fn multiply_rgb(color: [u8; 4], tint: [u8; 3]) -> [u8; 4] {
    [
        (u16::from(color[0]) * u16::from(tint[0]) / 255) as u8,
        (u16::from(color[1]) * u16::from(tint[1]) / 255) as u8,
        (u16::from(color[2]) * u16::from(tint[2]) / 255) as u8,
        color[3],
    ]
}

/// One layer's colour under a status tint. None leaves the layer's own colour.
fn layer_color(color: [u8; 4], tint: Option<[u8; 3]>) -> [u8; 4] {
    tint.map_or(color, |tint| multiply_rgb(color, tint))
}

/// A character's sprites (body, head, what it wears) or a single NPC/monster sprite, plus the textures decoded so far.
pub struct SpriteSet {
    layers: Vec<Layer>,
    /// Laid out like a player's body: standing and sitting frames go by head direction.
    player: bool,
    /// A player's: where its garment, head and headgear go among its layers.
    dressing: Option<Dressing>,
    /// How big it's drawn (babies are smaller).
    scale: f32,
    /// Status colour multiplied into every layer. None keeps each layer's own colour.
    tint: Option<[u8; 3]>,
    textures: HashMap<ImageKey, Texture>,
    /// A mounted rider's body without the mount, when a headgear sits by it.
    unmounted: Option<Act>,
    /// The action the unit is in and the frame delay it took when it started it (`action_delay`).
    delay: std::cell::Cell<Option<(usize, f32)>>,
}

impl SpriteSet {
    /// Which part an image is (`PartGroup`).
    pub fn part_group(&self, key: ImageKey) -> PartGroup {
        match self.layers.get(key.0).map(|layer| layer.part) {
            Some(Part::Weapon | Part::WeaponGlow) => PartGroup::Weapon,
            Some(Part::Shield | Part::Robe) => PartGroup::Gear,
            _ => PartGroup::Body,
        }
    }
}

impl SpriteSet {
    pub fn player(grf: &Grf, look: &Look, tables: &LookTables) -> Option<Self> {
        let sex = if look.sex == 0 { "여" } else { "남" };
        // Outfits and mounts change the body; the weapons and dyes go by it too.
        let job = sprite_job(look.job, look.option);
        let names = job_sprites(job)?;
        let class = names.body;
        let race = if is_doram(look.job) { "도람족" } else { "인간족" };
        // Doram palettes sit in their own folder.
        let palettes = if is_doram(look.job) { "data\\palette\\도람족\\머리" } else { "data\\palette\\머리" };
        let load = |path: String, palette: Option<Palette>, part: Part| -> Option<Layer> {
            Some(Layer {
                spr: Spr::parse(&grf.read(&format!("{path}.spr")).ok()?).ok()?,
                act: Act::parse(&grf.read(&format!("{path}.act")).ok()?).ok()?,
                palette,
                part,
            })
        };
        // Color 0 is the sprite's own palette.
        let dye = |path: String, color: u16| -> Option<Palette> { (color > 0).then(|| grf.read(&path).ok().and_then(|bytes| sprite::parse_palette(&bytes)))? };
        let body_palettes = if is_doram(look.job) { "data\\palette\\도람족\\body" } else { "data\\palette\\몸" };
        // ponytail: there is no head sprite 0; rathena allows hair 0, so show the first style.
        let hair = look.hair.max(1);
        let body = load(
            format!("data\\sprite\\{race}\\몸통\\{sex}\\{class}_{sex}"),
            dye(format!("{body_palettes}\\{}_{sex}_{}.pal", names.palette, look.clothes_color), look.clothes_color),
            Part::Body,
        )?;
        // The orcish bit replaces the hair with the orc face, placed as the head.
        let head = match orcish_head(look.job, look.option) {
            Some(face) => load(format!("data\\sprite\\{face}"), None, Part::Head),
            None => load(
                format!("data\\sprite\\{race}\\머리통\\{sex}\\{hair}_{sex}"),
                dye(format!("{palettes}\\머리{hair}_{sex}_{}.pal", look.hair_color), look.hair_color),
                Part::Head,
            ),
        };
        // ponytail: the orc face replaces the head, so headgear is left off.
        let orcish = orcish_head(look.job, look.option).is_some();
        // A headgear filling several slots shows once. Doram wear their own cut of some.
        let headgear = |view: u16, part: Part| {
            let name = tables.headgears.get(&view)?;
            let doram = is_doram(look.job).then(|| load(format!("data\\sprite\\악세사리\\{sex}_doram\\{sex}{name}"), None, part)).flatten();
            doram.or_else(|| load(format!("data\\sprite\\악세사리\\{sex}\\{sex}{name}"), None, part))
        };
        let bottom = (!orcish).then(|| headgear(look.head_bottom, Part::HeadBottom)).flatten();
        let mid = (!orcish && look.head_mid != look.head_bottom).then(|| headgear(look.head_mid, Part::HeadMid)).flatten();
        let top = (!orcish && look.head_top != look.head_bottom && look.head_top != look.head_mid).then(|| headgear(look.head_top, Part::HeadTop)).flatten();
        let weapon = (look.weapon > 0).then(|| {
            let view = tables.view(look.weapon);
            let name = tables.weapons.get(&view).cloned().unwrap_or_else(|| format!("_{view}"));
            let path = format!("data\\sprite\\{race}\\{0}\\{0}_{sex}{name}", names.weapon);
            (load(path.clone(), None, Part::Weapon), load(format!("{path}_검광"), None, Part::WeaponGlow))
        });
        // The four classic shields have names; newer ones are named by their view ID.
        let shield = (look.shield > 0).then(|| {
            let name = match tables.view(look.shield) {
                1 => "_가드".to_owned(),
                2 => "_버클러".to_owned(),
                3 => "_쉴드".to_owned(),
                4 => "_미러쉴드".to_owned(),
                view => format!("_{view}_방패"),
            };
            load(format!("data\\sprite\\방패\\{class}\\{class}_{sex}{name}"), None, Part::Shield)
        });
        // Most garments share one picture for every job (Doram have their own); how it moves is per job.
        let robe = tables.robes.get(&look.robe).and_then(|name| {
            let per_job = format!("data\\sprite\\로브\\{name}\\{sex}\\{class}_{sex}");
            let shared = format!("data\\sprite\\로브\\{name}\\{name}{}.spr", if is_doram(look.job) { "_doram" } else { "" });
            Some(Layer {
                spr: Spr::parse(&grf.read(&shared).or_else(|_| grf.read(&format!("{per_job}.spr"))).ok()?).ok()?,
                act: Act::parse(&grf.read(&format!("{per_job}.act")).ok()?).ok()?,
                palette: None,
                part: Part::Robe,
            })
        });

        let rules = tables.layers.as_ref();
        let robe_over_body = match &robe {
            Some(_) => body
                .act
                .actions
                .iter()
                .enumerate()
                .map(|(index, frames)| {
                    // Without the tables: over the body only facing away.
                    let fallback = (3..=5).contains(&(index % 8));
                    (0..frames.len())
                        .map(|frame| rules.and_then(|rules| rules.robe_over_body(look.robe, look.sex, job, index, frame)).unwrap_or(fallback))
                        .collect()
                })
                .collect(),
            None => Vec::new(),
        };
        let place = |view: u16, slot: u8, default: i32| -> [Option<i32>; 8] {
            std::array::from_fn(|direction| {
                let place = rules.and_then(|rules| rules.headgear_priority(direction, slot, view)).unwrap_or(default);
                (place != -1).then_some(place)
            })
        };
        let headgear =
            [(&bottom, Part::HeadBottom, look.head_bottom, 2, 300), (&mid, Part::HeadMid, look.head_mid, 4, 100), (&top, Part::HeadTop, look.head_top, 3, 200)]
                .into_iter()
                .filter(|(layer, ..)| layer.is_some())
                .map(|(_, part, view, slot, default)| {
                    let offsets = std::array::from_fn(|direction| {
                        is_doram(look.job).then(|| rules?.doram_headgear_offset(view, direction, look.sex)).flatten().unwrap_or_default()
                    });
                    let layer = usize::from(HEAD_HIDING.contains(&view));
                    let ignores_riding = rules.is_some_and(|rules| rules.ignores_riding(view));
                    Worn { part, places: place(view, slot, default), offsets, layer, ignores_riding }
                })
                .collect::<Vec<_>>();
        // On a mount, headgear the data marks sits by the body without it: that body's ACT, loaded only then.
        let mounted = job == sprite_job(look.job, look.option & MOUNTS) && job != look.job;
        let unmounted = (mounted && headgear.iter().any(|worn| worn.ignores_riding))
            .then(|| {
                let class = job_sprites(look.job)?.body;
                Act::parse(&grf.read(&format!("data\\sprite\\{race}\\몸통\\{sex}\\{class}_{sex}.act")).ok()?).ok()
            })
            .flatten();
        let dressing = Dressing {
            head_first: grf.read(&format!("data\\imf\\{class}_{sex}.imf")).ok().and_then(|bytes| sprite::parse_imf(&bytes).ok()).unwrap_or_default(),
            robe_over_body,
            robe_over_headgear: robe.is_some() && rules.is_some_and(|rules| rules.robe_over_headgear(look.robe)),
            headgear,
            head_hidden: !orcish && [look.head_top, look.head_mid, look.head_bottom].iter().any(|view| HEAD_HIDING.contains(view)),
            outfit: outfit(job),
            madogear: MADOGEAR_BODIES.contains(&job),
            female: look.sex == 0,
        };
        let (weapon, glow) = weapon.unwrap_or_default();
        Some(Self {
            layers: [Some(body), head, bottom, mid, top, weapon, glow, shield.flatten(), robe].into_iter().flatten().collect(),
            player: true,
            dressing: Some(dressing),
            scale: body_scale(job),
            tint: None,
            textures: HashMap::new(),
            unmounted,
            delay: Default::default(),
        })
    }

    /// An NPC or monster: one sprite named in the client's `JobNameTable`.
    /// A mercenary (jobs 6017 to 6046): its body by the job name table's path under the human bodies (`여\활용병`), laid
    /// out like a player's, and the weapon of its kind (the unit carries none). Lancers and swordsmen also draw the
    /// weapon's glow, right after the weapon, on the weapon's frame.
    pub fn mercenary(grf: &Grf, job: u16, name: &str) -> Option<Self> {
        let weapon = match job {
            6017..=6026 => "활용병_활",
            6027..=6036 => "창용병_창",
            6037..=6046 => "검용병_검",
            _ => return None,
        };
        let load = |path: String, part: Part| -> Option<Layer> {
            Some(Layer {
                spr: Spr::parse(&grf.read(&format!("{path}.spr")).ok()?).ok()?,
                act: Act::parse(&grf.read(&format!("{path}.act")).ok()?).ok()?,
                palette: None,
                part,
            })
        };
        let body = load(format!("data\\sprite\\인간족\\몸통\\{name}"), Part::Body)?;
        let weapon = load(format!("data\\sprite\\인간족\\용병\\{weapon}"), Part::Weapon);
        let glow = mercenary_glow(job).and_then(|path| load(format!("data\\sprite\\{path}"), Part::WeaponGlow));
        Some(Self {
            layers: [Some(body), weapon, glow].into_iter().flatten().collect(),
            player: true,
            dressing: None,
            scale: 1.0,
            tint: None,
            textures: HashMap::new(),
            unmounted: None,
            delay: Default::default(),
        })
    }

    /// The sprite folders to look in for a unit of this kind (5 monster, 8 homunculus), first match wins.
    pub fn folders(kind: u8) -> &'static [&'static str] {
        match kind {
            5 => &["몬스터", "npc"],
            8 => &["homun", "몬스터", "npc"],
            _ => &["npc", "몬스터"],
        }
    }

    pub fn single(grf: &Grf, name: &str, kind: u8) -> Option<Self> {
        Self::folders(kind).iter().find_map(|folder| Self::at(grf, &format!("data\\sprite\\{folder}\\{name}")))
    }

    /// One sprite by path without extension, e.g. `data\sprite\shadow`.
    pub fn at(grf: &Grf, path: &str) -> Option<Self> {
        Self::with_act(grf, path, &format!("{path}.act"))
    }

    /// Like `at`, moving by another ACT (a pet's accessory changes how it moves).
    pub fn with_act(grf: &Grf, path: &str, act: &str) -> Option<Self> {
        Some(Self {
            layers: vec![Layer {
                spr: Spr::parse(&grf.read(&format!("{path}.spr")).ok()?).ok()?,
                act: Act::parse(&grf.read(act).ok()?).ok()?,
                palette: None,
                part: Part::Body,
            }],
            player: false,
            dressing: None,
            scale: 1.0,
            tint: None,
            textures: HashMap::new(),
            unmounted: None,
            delay: Default::default(),
        })
    }

    /// Colours every layer from here on. None clears the status colour.
    pub fn set_tint(&mut self, tint: Option<[u8; 3]>) {
        self.tint = tint;
    }

    /// Places the frame of `action` facing `direction` (head turned `head_direction`: 0 straight, 1 and 2 to the sides)
    /// at `timing`, decoding any images not seen yet.
    pub fn frame(&mut self, gpu: &Gpu, action: usize, direction: u8, head_direction: u16, timing: Timing) -> Vec<Placed> {
        self.frame_at(gpu, action * 8 + usize::from(direction % 8), head_direction, timing)
    }

    /// Like `frame`, by the ACT's own action number (sprites like the cursors aren't laid out in eights).
    pub fn frame_at(&mut self, gpu: &Gpu, index: usize, head_direction: u16, timing: Timing) -> Vec<Placed> {
        let mut placed = self.place(index, head_direction, timing);
        for &(key, ..) in &placed {
            self.decode(gpu, key);
        }
        placed.retain(|(key, ..)| self.textures.contains_key(key));
        placed
    }

    /// A frame for a flat preview (character select, the equipment window): as `frame`, but turned layers come as
    /// pictures turned ahead of time and placed upright by their bounding box, as 2D sprites don't turn.
    pub fn flat_frame(&mut self, gpu: &Gpu, action: usize, direction: u8, head_direction: u16, timing: Timing) -> Vec<Placed> {
        let mut placed = self.frame(gpu, action, direction, head_direction, timing);
        for (key, position, size, angle) in &mut placed {
            let degrees = (angle.round() as i32).rem_euclid(360) as u16;
            if degrees == 0 {
                continue;
            }
            let mirrored = size[0] < 0.0;
            let turned = (key.0, key.1, key.2, key.3, degrees + if mirrored { 360 } else { 0 });
            self.decode(gpu, turned);
            let (Some(upright), Some(image)) = (self.textures.get(key), self.textures.get(&turned)) else { continue };
            // The picture keeps the layer's zoom: its pixels are as big as the upright image's.
            let zoom = [size[0].abs() / upright.size[0], size[1] / upright.size[1]];
            let middle = [position[0] + size[0] / 2.0, position[1] + size[1] / 2.0];
            let turned_size = [image.size[0] * zoom[0], image.size[1] * zoom[1]];
            (*key, *position, *size, *angle) = (turned, [middle[0] - turned_size[0] / 2.0, middle[1] - turned_size[1] / 2.0], turned_size, 0.0);
        }
        placed
    }

    /// Decodes an image into a texture once: its layer's palette and tint, and, for a flat preview, turned.
    fn decode(&mut self, gpu: &Gpu, key: ImageKey) {
        if self.textures.contains_key(&key) {
            return;
        }
        let layer = &self.layers[key.0];
        let Some(mut image) = layer.spr.image(key.1, key.2, layer.palette.as_ref()) else { return };
        if key.3 != [255; 4] {
            for pixel in image.pixels_mut() {
                for (channel, tint) in pixel.0.iter_mut().zip(key.3) {
                    *channel = (u16::from(*channel) * u16::from(tint) / 255) as u8;
                }
            }
        }
        if key.4 != 0 {
            image = turn_image(&image, i32::from(key.4 % 360), key.4 >= 360);
        }
        self.textures.insert(key, gpu.texture(&image, false));
    }

    fn place(&self, index: usize, head_direction: u16, timing: Timing) -> Vec<Placed> {
        let action = index / 8;
        let Some(body_frames) = self.layers[0].act.actions.get(index).filter(|frames| !frames.is_empty()) else { return Vec::new() };
        // One delay for the action, in every direction and for every layer.
        // ponytail: garments and headgear with frames of their own play them at the body's delay too.
        let delay = self.action_delay(action);
        let animated = |_: &Layer, count: usize| animated(delay, count, timing);
        // A player standing or sitting has its frames in three groups, one per head direction: the body holds its
        // group's frame and the head animates within its group. Cycling through the groups shakes the head side to
        // side, as the head anchors differ.
        let look = (self.player && (action == IDLE || action == SIT)).then(|| usize::from(head_direction.min(2)));
        let frame_index = match look {
            Some(look) => look.min(body_frames.len() - 1),
            None => animated(&self.layers[0], body_frames.len()),
        };
        let body_frame = &body_frames[frame_index];

        let frames_of =
            |part: Part| self.layers.iter().position(|layer| layer.part == part).and_then(|number| Some((number, self.layers[number].act.actions.get(index)?)));
        // The head's frames count the frames of what's worn on it.
        let head_count = frames_of(Part::Head).map(|(_, frames)| frames.len()).filter(|&count| count > 0).unwrap_or(body_frames.len());
        let order = match &self.dressing {
            Some(dressing) => dressing.order(index, frame_index),
            None => vec![Part::Body, Part::Weapon, Part::WeaponGlow],
        };

        // A player's parts draw one ACT layer each; the headgear go by the head's anchor.
        let pc = self.dressing.is_some();
        let head_frame = frames_of(Part::Head).and_then(|(_, frames)| frames.get(frame_index));
        let head_shift = head_frame.map_or([0, 0], |head| sprite::head_shift(body_frame, head));
        let mut placed = Vec::new();
        for part in order {
            let Some((number, frames)) = frames_of(part) else { continue };
            let layer = &self.layers[number];
            // The head goes with the body's frame (standing or sitting, the head direction's), as do weapons and
            // shields; garments and headgear too unless they have frames to play in turn (garments then from the
            // first group while standing or sitting).
            let own = |count: usize| animated(layer, count);
            let frame = match part {
                Part::Body => Some(frame_index),
                Part::Robe => layer_frame(frames.len(), head_count, if look.is_some() { 0 } else { frame_index }, frame_index, own),
                Part::HeadBottom | Part::HeadMid | Part::HeadTop => layer_frame(frames.len(), head_count, frame_index, frame_index, own),
                // A frame the layer lacks: nothing drawn.
                Part::Head | Part::Weapon | Part::WeaponGlow | Part::Shield => Some(frame_index),
            };
            let Some(frame) = frame.and_then(|frame| frames.get(frame)) else { continue };
            let worn = self.dressing.as_ref().and_then(|dressing| dressing.headgear.iter().find(|worn| worn.part == part));
            let (shift, pick) = match (part, worn) {
                (_, None) if !pc => ([0, 0], Pick::All),
                (Part::Body | Part::Weapon | Part::WeaponGlow, _) => ([0, 0], Pick::Layer(0)),
                (Part::Head, _) => (head_shift, Pick::Layer(1)),
                (_, Some(worn)) => {
                    let mut shift = sprite::gear_shift(head_shift, head_frame, frame);
                    // On a mount: the body's anchor height without the mount, standing, walking, sitting or dead.
                    if worn.ignores_riding
                        && (matches!(action, IDLE | WALK | SIT) || DEAD_ACTIONS.contains(&index))
                        && let Some(&[_, y, _]) = body_frame.anchors.first()
                        && let Some(&[_, unmounted, _]) = self.unmounted.as_ref().and_then(|act| act.actions.get(index)?.get(frame_index)?.anchors.first())
                    {
                        shift[1] += unmounted - y;
                    }
                    // Moved for its wearer, by the way the head looks (turned one way or the other off the body's).
                    let turn = match look {
                        Some(1) => 1,
                        Some(2) => 7,
                        _ => 0,
                    };
                    let [x, y] = worn.offsets[(index + turn) % 8];
                    ([shift[0] + x, shift[1] + y], Pick::Unzoomed(worn.layer))
                }
                _ => ([0, 0], Pick::All),
            };
            placed.extend(sprite::place(&layer.spr, frame, shift, pick).into_iter().map(|placement| {
                (
                    (number, placement.kind, placement.image, layer_color(placement.color, self.tint), 0),
                    placement.position.map(|value| value * self.scale),
                    placement.size.map(|value| value * self.scale),
                    placement.angle as f32,
                )
            }));
        }
        placed
    }

    /// The body's frame at `timing` (not for standing or sitting players, whose frames follow the head) and the sound
    /// it plays: its ACT event, other than "atk", which marks when a hit lands.
    pub fn body_frame(&self, index: usize, timing: Timing) -> Option<(usize, Option<&str>)> {
        let body = &self.layers[0];
        let frames = body.act.actions.get(index).filter(|frames| !frames.is_empty())?;
        let frame = animated(self.action_delay(index / 8), frames.len(), timing);
        let sound = usize::try_from(frames[frame].event).ok().and_then(|event| body.act.events.get(event)).map(String::as_str).filter(|&name| name != "atk");
        Some((frame, sound))
    }

    /// The `.wav` an ACT frame's event plays, if any.
    pub fn frame_sound(&self, index: usize, frame: usize) -> Option<&str> {
        let body = &self.layers[0];
        let event = usize::try_from(body.act.actions.get(index)?.get(frame)?.event).ok()?;
        body.act.events.get(event).map(String::as_str).filter(|name| name.to_lowercase().ends_with(".wav"))
    }

    /// How long `action` takes to play through once at its own frame delays.
    pub fn action_ms(&self, action: usize, direction: u8) -> Option<f32> {
        self.index_ms(action * 8 + usize::from(direction % 8))
    }

    /// How many actions the sprite has.
    pub fn actions(&self) -> usize {
        self.layers[0].act.actions.len()
    }

    /// How many frames the ACT's action `index` has.
    pub fn frames(&self, index: usize) -> usize {
        self.layers[0].act.actions.get(index).map_or(0, Vec::len)
    }

    /// Like `action_ms`, by the ACT's own action number.
    /// How many frames an action (by index) has.
    pub fn frame_count(&self, index: usize) -> Option<usize> {
        self.layers.first()?.act.actions.get(index).map(Vec::len)
    }

    /// When a hit lands in an attack (action `index`) played at `scale` times its ACT pace: at the frame with an
    /// "atk" event, else the second-to-last.
    pub fn hit_ms(&self, index: usize, scale: f32) -> Option<f32> {
        let body = &self.layers[0];
        let frames = body.act.actions.get(index).filter(|frames| !frames.is_empty())?;
        let atk = |frame: &sprite::Frame| usize::try_from(frame.event).ok().and_then(|event| body.act.events.get(event)).is_some_and(|name| name == "atk");
        let frame = frames.iter().position(atk).unwrap_or(frames.len().saturating_sub(2));
        let delay = body.act.delays.get(index).copied().unwrap_or(4.0).max(1.0);
        Some(frame as f32 * delay * DELAY_UNIT_MS * scale)
    }

    /// When frame `frame` of action `index` comes, played at `scale` times its ACT pace.
    pub fn frame_ms(&self, index: usize, frame: usize, scale: f32) -> Option<f32> {
        let delay = self.first_delay(index / 8);
        self.layers[0].act.actions.get(index).map(|_| frame as f32 * delay * DELAY_UNIT_MS * scale)
    }

    pub fn index_ms(&self, index: usize) -> Option<f32> {
        let frames = self.layers[0].act.actions.get(index)?.len();
        Some(frames as f32 * self.first_delay(index / 8) * DELAY_UNIT_MS)
    }

    /// An action's frame delay, the same in every direction: its direction 0's (4 when the ACT has none there), at
    /// least 1. Used for how long an action lasts; see `action_delay` for what's drawn.
    // ponytail: a delay of 0 or less, which keeps the previous action's when drawn, counts as 4 here.
    fn first_delay(&self, action: usize) -> f32 {
        let delay = self.layers[0].act.delays.get(action * 8).copied().unwrap_or(4.0);
        if delay > 0.0 { delay.max(1.0) } else { 4.0 }
    }

    /// The frame delay the unit's frames play at in `action`: taken when the action starts from its direction 0 (4
    /// when the ACT has none there; under 1 is 1), or the one it had when that's 0 or less, and kept in every direction
    /// until the next action.
    fn action_delay(&self, action: usize) -> f32 {
        let (was, had) = self.delay.get().unwrap_or((usize::MAX, 4.0));
        if was == action {
            return had;
        }
        let delay = self.layers[0].act.delays.get(action * 8).copied().unwrap_or(4.0);
        let delay = if delay > 0.0 { delay.max(1.0) } else { had };
        self.delay.set(Some((action, delay)));
        delay
    }

    pub fn texture(&self, key: ImageKey) -> &Texture {
        &self.textures[&key]
    }

    /// A texture decoded earlier, if it still is (a copy left behind a unit whose look has changed since).
    pub fn texture_of(&self, key: ImageKey) -> Option<&Texture> {
        self.textures.get(&key)
    }
}

/// Which of `count` frames plays at `timing`, at the action's frame `delay` (`SpriteSet::action_delay`).
fn animated(delay: f32, count: usize, timing: Timing) -> usize {
    match timing {
        Timing::Loop(elapsed_ms) => loop_frame(elapsed_ms, delay, count),
        Timing::Span(progress) => ((progress * count as f32) as usize).min(count - 1),
        Timing::Once(elapsed_ms, scale) => once_frame(elapsed_ms, delay * scale, count),
        Timing::Walk(distance) => walk_frame(distance, delay, count),
    }
}

/// Which of a garment's or headgear's `count` frames shows with the body at frame `base`. One with n frames to each of
/// the head's `head_count` plays them in turn (`own(n)`: which of n frames its own clock is at) within group `group`, so
/// balloons bob and wings flap; any other shows frame `base`. None when it has no such frame.
fn layer_frame(count: usize, head_count: usize, group: usize, base: usize, own: impl Fn(usize) -> usize) -> Option<usize> {
    let n = count / head_count.max(1);
    let frame = if n > 1 && count.is_multiple_of(head_count) { own(n) + n * group } else { base };
    (frame < count).then_some(frame)
}

/// Where a garment goes among a character's layers.
#[derive(Clone, Copy, PartialEq, Debug)]
enum RobeAt {
    /// Behind everything.
    Behind,
    /// Over the body, under the head.
    OverBody,
    /// Over the body and the headgear too (bags).
    OverHeadgear,
}

/// Garments over the headgear go at this place among the layers (headgear: middle 100, upper 200, lower 300).
const OVER_HEADGEAR: i32 = 400;

/// A character's layers back to front, facing `direction` (0 the camera, turning clockwise): a garment behind, a shield
/// turned away, the body and head (the head behind the body in some frames facing away: `head_first`) with a garment
/// over the body between them, the weapon and a shield turned this way (when `armed`), then the headgear by its
/// place (`headgear`: part and place; the body's layers are 1, 2, ... so a negative place goes behind them all), and a
/// garment worn over the headgear.
fn stack(direction: usize, armed: bool, head_first: bool, robe: RobeAt, headgear: &[(Part, i32)]) -> Vec<Part> {
    let turned_away = (2..=5).contains(&direction);
    let mut body = vec![Part::Body];
    if robe == RobeAt::OverBody {
        body.push(Part::Robe);
    }
    let mut walk = Vec::new();
    if robe == RobeAt::Behind {
        walk.push(Part::Robe);
    }
    if armed && turned_away {
        walk.push(Part::Shield);
    }
    if head_first {
        walk.push(Part::Head);
        walk.extend(body);
    } else {
        walk.extend(body);
        walk.push(Part::Head);
    }
    if armed {
        walk.extend([Part::Weapon, Part::WeaponGlow]);
        if !turned_away {
            walk.push(Part::Shield);
        }
    }
    let mut placed: Vec<(i32, Part)> = (1..).zip(walk).chain(headgear.iter().map(|&(part, place)| (place, part))).collect();
    if robe == RobeAt::OverHeadgear {
        placed.push((OVER_HEADGEAR, Part::Robe));
    }
    placed.sort_by_key(|&(place, _)| place);
    placed.into_iter().map(|(_, part)| part).collect()
}

/// Headgear views that hide the head under them.
const HEAD_HIDING: [u16; 7] = [331, 1931, 2171, 2494, 2587, 2606, 2607];

/// A headgear layer's place among the layers facing each way (None hides it there), and how far it's moved facing
/// each way (Doram wear some headgear a little off).
struct Worn {
    part: Part,
    places: [Option<i32>; 8],
    offsets: [[i32; 2]; 8],
    /// The ACT layer drawn: 0, or 1 for a headgear that hides the head.
    layer: usize,
    /// On a mount it sits by the body without the mount (the data's `isIgnoreRiding`).
    ignores_riding: bool,
}

/// What places a player's garment, head and headgear among its layers.
struct Dressing {
    /// By action and frame: the head goes behind the body (the job's .imf).
    head_first: Vec<Vec<bool>>,
    /// By action and body frame: the garment goes over the body rather than behind everything.
    robe_over_body: Vec<Vec<bool>>,
    /// Over the body, the garment goes over the headgear too.
    robe_over_headgear: bool,
    /// Each headgear layer: its place and how far it's moved, facing each way.
    headgear: Vec<Worn>,
    /// The headgear hides the head.
    head_hidden: bool,
    /// An outfit body: no weapon or shield.
    outfit: bool,
    /// A Madogear body: never a shield, and no garment for a woman.
    madogear: bool,
    female: bool,
}

/// Sprite jobs on a Madogear.
const MADOGEAR_BODIES: [u16; 3] = [4086, 4087, 4112];
/// A player's dead action (all eight directions).
const DEAD_ACTIONS: std::ops::RangeInclusive<usize> = 64..=71;

impl Dressing {
    /// The layers to draw in action `index` (direction included) at body frame `frame`, back to front.
    fn order(&self, index: usize, frame: usize) -> Vec<Part> {
        let direction = index % 8;
        let at = |table: &[Vec<bool>]| table.get(index).and_then(|frames| frames.get(frame)).copied().unwrap_or(false);
        let robe = match (at(&self.robe_over_body), self.robe_over_headgear) {
            (false, _) => RobeAt::Behind,
            (true, false) => RobeAt::OverBody,
            (true, true) => RobeAt::OverHeadgear,
        };
        let headgear: Vec<(Part, i32)> = self.headgear.iter().filter_map(|worn| Some((worn.part, worn.places[direction]?))).collect();
        // Weapons and shields show in the battle stance and the weapon attacks.
        let armed = !self.outfit && matches!(index, 32..=39 | 80..=95);
        let mut order = stack(direction, armed, at(&self.head_first), robe, &headgear);
        if self.head_hidden {
            order.retain(|&part| part != Part::Head);
        }
        // The dead and a woman on a Madogear wear no garment; a Madogear carries no shield.
        let robe_off = DEAD_ACTIONS.contains(&index) || self.madogear && self.female;
        order.retain(|&part| !(part == Part::Robe && robe_off || part == Part::Shield && self.madogear));
        order
    }
}

/// A one-shot action playing over a fixed time.
struct Shot {
    action: usize,
    started: Instant,
    pace: Pace,
    /// Keep showing the last frame afterwards (death).
    hold: bool,
}

/// How a flinch plays for d = the hit's target motion / 288 (`delay`: the action's ACT frame delay). Over 1, at the ACT's
/// pace with the last frame held until d times its length; up to 1, a player's frames last d times their delay and
/// anyone else's d × 24 ms each; at 0, as the ACT has it.
fn flinch_pace(d: f32, player: bool, delay: f32) -> Pace {
    match d {
        d if d > 1.0 => Pace::Hold(d),
        d if d > 0.0 && player => Pace::Act(d),
        d if d > 0.0 => Pace::Act(d / delay),
        _ => Pace::Act(1.0),
    }
}

/// The frames whose sounds play when an animation moves from `last` to `now` ((action, frame)) in an action of `count`
/// frames: those passed since the last, wrapping round a loop. In a new action from frame 1: a sound on frame 0 of an
/// action just begun doesn't play.
pub fn frames_passed(last: Option<(usize, usize)>, now: (usize, usize), count: usize) -> Vec<usize> {
    match last {
        Some((action, frame)) if action == now.0 && frame == now.1 => Vec::new(),
        Some((action, frame)) if action == now.0 && frame < now.1 => (frame + 1..=now.1).collect(),
        Some((action, frame)) if action == now.0 => (frame + 1..count).chain(0..=now.1).collect(),
        _ => (1..=now.1).collect(),
    }
}

/// Whether a hit that lands makes its target flinch, by what it plays now: not a player in its own swing, nor a monster
/// or NPC in one of its special idle actions (5 to 8). A monster's own attack is cut.
pub fn flinches(kind: u8, playing: usize) -> bool {
    match kind {
        0 => playing != combat_actions(kind).0,
        _ => !(5..=8).contains(&playing),
    }
}

/// How a one-shot action plays: stretched over a duration, at its ACT pace times a scale, or at its ACT pace with the
/// last frame held until its length times a factor.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Pace {
    Stretch(f32),
    Act(f32),
    Hold(f32),
}

/// The client's damage numbers (data\sprite\이팩트\숫자.spr), one image per digit, and its MISS label (msg.spr).
pub struct Digits {
    digits: Vec<Texture>,
    pub miss: Texture,
}

impl Digits {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Option<Self> {
        let spr = Spr::parse(&grf.read("data\\sprite\\이팩트\\숫자.spr").ok()?).ok()?;
        let digits = (0..10).map(|digit| Some(gpu.texture(&spr.image(0, digit, None)?, false))).collect::<Option<_>>()?;
        let messages = Spr::parse(&grf.read("data\\sprite\\이팩트\\msg.spr").ok()?).ok()?;
        let frame = |index: usize| messages.image(0, index, None).or_else(|| messages.image(1, index, None)).map(|image| gpu.texture(&image, false));
        Some(Self { digits, miss: frame(0)? })
    }

    /// The digits of `number` centered on the anchor: texture, top-left offset and size in sprite pixels. Each digit is
    /// centred on its own slot, `DIGIT_STEP` apart, whatever its width.
    pub fn place(&self, number: i32, scale: f32) -> Vec<(&Texture, [f32; 2], [f32; 2])> {
        let digits: Vec<&Texture> = number.unsigned_abs().to_string().bytes().map(|digit| &self.digits[usize::from(digit - b'0')]).collect();
        let count = digits.len();
        digits
            .into_iter()
            .enumerate()
            .map(|(index, texture)| {
                let size = texture.size.map(|value| value * scale);
                let middle = digit_slot(index, count) * scale;
                (texture, [middle - size[0] / 2.0, -size[1] / 2.0], size)
            })
            .collect()
    }
}

/// The middle of digit `index` (the first, leftmost, is 0) of a `count`-digit number, in sprite pixels from the
/// number's middle at scale 1.
pub fn digit_slot(index: usize, count: usize) -> f32 {
    (index as f32 - (count as f32 - 1.0) / 2.0) * DIGIT_STEP
}

/// Monster jobs (sprite IDs): 1001–3998 and 20021–31999.
pub fn is_monster_job(job: u16) -> bool {
    (1001..=3998).contains(&job) || (20021..=31999).contains(&job)
}

/// NPC jobs: 45–999.
pub fn is_npc_job(job: u16) -> bool {
    (45..=999).contains(&job)
}

/// Sprite pixels between the middles of two digits of a number at scale 1.
pub const DIGIT_STEP: f32 = 9.55;
/// A hit's number lasts 70 ticks of 24 ms.
pub const HIT_NUMBER_TICKS: f32 = 70.0;

/// Where a hit's number is `ticks` (24 ms each) after it shows, and how it looks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NumberMotion {
    /// World units above the target's feet: up to 32.7 at tick 30, then below the feet by the end.
    pub height: f32,
    /// World units it has drifted along each ground axis (`number_drift` gives the signs).
    pub drift: f32,
    pub scale: f32,
    pub alpha: f32,
}

/// A hit's number: it pops up from 2.7 world units over the feet, arcs up and falls away, drifting diagonally,
/// shrinking from 5 times its size to 1.2 (1.3 for a critical) and fading.
pub fn hit_number(ticks: f32, critical: bool) -> NumberMotion {
    NumberMotion {
        height: 8.0 / 3.0 + 2.0 * ticks - ticks * ticks / 30.0,
        drift: 0.8 * (ticks / 3.0 + 3.0),
        scale: (5.0 - 0.24 * ticks).max(if critical { 1.3 } else { 1.2 }),
        alpha: ((250.0 - 3.4 * ticks) / 255.0).clamp(0.0, 1.0),
    }
}

/// How long, in 24 ms ticks: a heal's number, a multi-hit's total, a MISS.
pub const HEAL_NUMBER_TICKS: f32 = 120.0;
pub const TOTAL_NUMBER_TICKS: f32 = 120.0;
pub const MISS_TICKS: f32 = 80.0;
/// The message sprite's (`이팩트\msg`) critical burst and Lucky actions; Lucky plays once (28 frames of 24 ms).
pub const BURST_ACTION: usize = 2;
pub const LUCKY_ACTION: usize = 3;
pub const LUCKY_MS: f32 = 672.0;
/// Lucky shows this many world units over the feet.
pub const LUCKY_HEIGHT: f32 = 20.0;

/// A heal's number: from 12 units over the feet, rising slowly, shrinking from 5 times its size to 1, fading.
pub fn heal_number(ticks: f32) -> NumberMotion {
    NumberMotion { height: 12.0 + 0.18 * ticks, drift: 0.0, scale: (5.0 - 0.24 * ticks).max(1.0), alpha: ((250.0 - 2.0 * ticks) / 255.0).clamp(0.0, 1.0) }
}

/// A multi-hit's running total: high over the target at 27 units, rising slowly, popping from half its size to 3
/// times, then fading after tick 90.
// ponytail: the game grows it by 0.18·t every drawn frame (so faster at higher frame rates); here every 24 ms tick.
pub fn total_number(ticks: f32) -> NumberMotion {
    let ticks_whole = ticks.floor();
    NumberMotion {
        height: 27.0 + 0.1 * ticks,
        drift: 0.0,
        scale: (0.5 + 0.09 * ticks_whole * (ticks_whole + 1.0)).min(3.0),
        alpha: ((250.0 - 8.0 * (ticks - 90.0).max(0.0)) / 255.0).clamp(0.0, 1.0),
    }
}

/// MISS: over the attacker, from 12 units over its feet, rising quickly at its own size, fading.
pub fn miss_number(ticks: f32) -> NumberMotion {
    NumberMotion { height: 12.0 + 0.54 * ticks, drift: 0.0, scale: 1.0, alpha: ((250.0 - 3.0 * ticks) / 255.0).clamp(0.0, 1.0) }
}

/// The burst behind a critical hit's number: on the number's path, shrinking from 3 times its size to 1, fading; its
/// frames go by every two ticks and hold on the last.
pub fn burst(ticks: f32) -> NumberMotion {
    NumberMotion { scale: (3.0 - 0.144 * ticks).max(1.0), alpha: ((255.0 - 3.45 * ticks) / 255.0).clamp(0.0, 1.0), ..hit_number(ticks, true) }
}

/// Which way (x east, z north: each 1 or -1) a hit's number drifts from a unit facing `rot` (`angle_of`'s degrees):
/// diagonally away from where it faces. A facing the server set counts from 180 (north) up past 360, so all but
/// north and north-west drift north-east, as the game does.
pub fn number_drift(rot: f32, from_server: bool) -> [f32; 2] {
    let angle = match (from_server, rot) {
        (true, rot) if rot < 180.0 => rot + 360.0,
        (false, rot) if rot > 180.0 => rot - 360.0,
        (_, rot) => rot,
    };
    match angle {
        angle if (0.0..90.0).contains(&angle) => [-1.0, 1.0],
        angle if (90.0..180.0).contains(&angle) => [-1.0, -1.0],
        angle if (180.0..270.0).contains(&angle) => [1.0, -1.0],
        _ => [1.0, 1.0],
    }
}

/// How far the cart's rope lets it lag: one cell.
pub const CART_ROPE: f32 = world::CELL;
/// Cart frames a world unit rolled.
const CART_FRAMES: f32 = 0.92;
/// The falcon flies this far above the ground.
pub const FALCON_HEIGHT: f32 = 25.0;
/// The falcon and warg move by rules per game tick, 1/60 s.
const TICK: f32 = 1.0 / 60.0;
// ponytail: a new falcon starts two cells behind its hunter, a new warg three behind its ranger.
const FALCON_START: f32 = world::CELL * 2.0;
const WARG_START: f32 = world::CELL * 3.0;
/// A new falcon closes 1/25 of the way a tick until it first comes within 25. Then it flies 1 a tick while farther
/// than 12, closes 1/20 of the way while farther than 3, and hovers within that.
const FALCON_ARRIVED: f32 = 25.0;
const FALCON_FAR: f32 = 12.0;
const FALCON_NEAR: f32 = 3.0;
/// The frame the falcon shows while it's arriving.
const FALCON_ARRIVING_FRAME: usize = 8;
/// The warg waits until its owner is farther than 15, then runs 0.7 a tick; within 15.7 it's put at 14.9.
const WARG_WAIT: f32 = 15.0;
const WARG_RUN: f32 = 0.7;
const WARG_CATCH: f32 = 15.7;
const WARG_STOP: f32 = 14.9;
/// The warg's shadow, twice a body's; the baby warg's a body's.
pub const WARG_SHADOW: f32 = 2.0;
const BABY_WARG_SHADOW: f32 = 1.0;
/// The warg's hurt and dead actions (a monster's).
const WARG_HURT: usize = 3;
const WARG_DEAD: usize = 4;
/// A player's flinch and dead actions.
const PLAYER_FLINCH: usize = 6;
const PLAYER_DEAD: usize = 8;
/// Baby Hunters and Rangers, whose falcon is the baby one; Baby Rangers' warg too; and the Windhawks.
const BABY_FALCON_JOBS: [u16; 3] = [4034, 4098, 4111];
const BABY_WARG_JOBS: [u16; 2] = [4098, 4111];
const WINDHAWK_JOBS: [u16; 2] = [4257, 4278];

/// The MVP tomb, which casts no shadow.
const MVP_TOMB: u16 = 565;

/// Sprite jobs that keep their shadow while sitting: the Star Gladiator line's union forms, which float.
const SITTING_SHADOW_JOBS: [u16; 5] = [4048, 4238, 4243, 4244, 4316];

/// How wide a unit's shadow is, in cells (0 for none): its sprite job's factor from the data's tables, 1 when the job
/// isn't listed. A player (`player`) casts none while dead or sitting, but for the floating union forms.
pub fn shadow_factor(factors: &HashMap<u16, f32>, player: bool, job: u16, option: u32, sitting: bool, dead: bool) -> f32 {
    let sprite = if player { sprite_job(job, option) } else { job };
    if player && (dead || sitting && !SITTING_SHADOW_JOBS.contains(&sprite)) || sprite == MVP_TOMB {
        return 0.0;
    }
    factors.get(&sprite).copied().unwrap_or(1.0)
}

/// Cell steps of the eight facings, from south, turning west. World X is east, Z is north.
const FACING: [[f32; 2]; 8] = [[0.0, -1.0], [-1.0, -1.0], [-1.0, 0.0], [-1.0, 1.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [1.0, -1.0]];

/// World XZ of a fractional cell's center.
pub fn world_xz(cell: [f32; 2]) -> [f32; 2] {
    [(cell[0] + 0.5) * world::CELL, (cell[1] + 0.5) * world::CELL]
}

/// One step of a rope. Farther than `reach` from `leader`, move straight in until exactly `reach` away.
pub fn rope_step(from: [f32; 2], leader: [f32; 2], reach: f32) -> [f32; 2] {
    let dx = leader[0] - from[0];
    let dz = leader[1] - from[1];
    let distance = (dx * dx + dz * dz).sqrt();
    if distance <= reach || distance < 1.0e-4 {
        return from;
    }
    let scale = (distance - reach) / distance;
    [from[0] + dx * scale, from[1] + dz * scale]
}

/// `reach` world units behind a leader facing `direction`.
fn place_behind(leader: [f32; 2], direction: u8, reach: f32) -> [f32; 2] {
    let [dx, dz] = FACING[usize::from(direction % 8)];
    let length = (dx * dx + dz * dz).sqrt();
    [leader[0] - dx / length * reach, leader[1] - dz / length * reach]
}

/// What a share `fraction` taken each tick comes to over `ticks` ticks.
fn per_ticks(fraction: f32, ticks: f32) -> f32 {
    1.0 - (1.0 - fraction).powf(ticks)
}

/// `fraction` of the way from `from` to `to`.
fn toward(from: [f32; 2], to: [f32; 2], fraction: f32) -> [f32; 2] {
    [from[0] + (to[0] - from[0]) * fraction, from[1] + (to[1] - from[1]) * fraction]
}

/// `distance` from `from` toward `to`, not past it.
fn advance(from: [f32; 2], to: [f32; 2], distance: f32) -> [f32; 2] {
    let apart = f32::hypot(to[0] - from[0], to[1] - from[1]);
    if apart <= distance || apart < 1e-4 { to } else { toward(from, to, distance / apart) }
}

/// The falcon's step over `ticks` ticks toward its `owner`: where it gets to, and whether it's still arriving.
fn falcon_step(from: [f32; 2], owner: [f32; 2], arriving: bool, ticks: f32) -> ([f32; 2], bool) {
    let distance = f32::hypot(owner[0] - from[0], owner[1] - from[1]);
    if arriving && distance > FALCON_ARRIVED {
        return (toward(from, owner, per_ticks(1.0 / 25.0, ticks)), true);
    }
    let at = if distance > FALCON_FAR {
        advance(from, owner, ticks)
    } else if distance > FALCON_NEAR {
        toward(from, owner, per_ticks(1.0 / 20.0, ticks))
    } else {
        from
    };
    (at, false)
}

/// The warg's step over `ticks` ticks toward its `owner`: where it gets to, and whether it ran.
fn warg_step(from: [f32; 2], owner: [f32; 2], ticks: f32) -> ([f32; 2], bool) {
    if f32::hypot(owner[0] - from[0], owner[1] - from[1]) <= WARG_WAIT {
        return (from, false);
    }
    let at = advance(from, owner, WARG_RUN * ticks);
    let left = f32::hypot(at[0] - owner[0], at[1] - owner[1]);
    if left < WARG_CATCH && left > 1e-4 {
        return (toward(owner, at, WARG_STOP / left), true);
    }
    (at, true)
}

/// The exact way (degrees, as `Actor::rot`) from `from` to its owner; `rot` when it's right on them.
fn facing(from: [f32; 2], owner: [f32; 2], rot: f32) -> f32 {
    let [dx, dz] = [owner[0] - from[0], owner[1] - from[1]];
    if f32::hypot(dx, dz) < 1e-3 { rot } else { dx.atan2(-dz).to_degrees().rem_euclid(360.0) }
}

// ponytail: the data's other falcons (매2, OWL, demon_pungus, explosion, lude) aren't picked by anything known.
/// The falcon's or warg's sprite for an owner of `job`, as (sprite path, ACT path) to try in order, and its shadow.
fn companion_sprites(warg: bool, job: u16) -> (Vec<(String, String)>, f32) {
    let pair = |path: &str, act: &str| (format!("data\\sprite\\{path}"), format!("data\\sprite\\{act}.act"));
    let same = |path: &str| pair(path, path);
    let mut tries = Vec::new();
    let mut shadow = if warg { WARG_SHADOW } else { 0.0 };
    match (warg, job) {
        // ponytail: the Windhawk wolf's shadow is the warg's.
        (false, job) if WINDHAWK_JOBS.contains(&job) => tries.push(same("이팩트\\windhawk_hawk")),
        (true, job) if WINDHAWK_JOBS.contains(&job) => tries.push(same("이팩트\\windhawk_wolf")),
        (false, job) if BABY_FALCON_JOBS.contains(&job) => tries.push(pair("이팩트\\매", "이팩트\\베이비매")),
        (true, job) if BABY_WARG_JOBS.contains(&job) => {
            tries.push(pair("몬스터\\워그", "몬스터\\베이비워그"));
            shadow = BABY_WARG_SHADOW;
        }
        _ => {}
    }
    let name = if warg { "워그" } else { "매" };
    tries.extend(["이팩트", "몬스터", "npc"].map(|folder| same(&format!("{folder}\\{name}"))));
    (tries, shadow)
}

fn falcon_out(option: u32) -> bool {
    option & OPTION_FALCON != 0
}

/// The warg on the ground. Hidden while the player is riding it.
fn warg_out(option: u32) -> bool {
    option & OPTION_WUG != 0 && option & OPTION_WUGRIDER == 0
}

/// A falcon or a warg following a player: where it stands, the way it faces, and its sprite.
/// A pushcart on its rope: where it stands (world XZ), the way it faces (degrees, as `Actor::rot`) and how far it has
/// rolled (world units).
pub struct Cart {
    pub sprites: SpriteSet,
    pub at: [f32; 2],
    pub rot: f32,
    pub rolled: f32,
}

impl Cart {
    /// A rope's length behind a leader facing `direction`.
    pub fn behind(sprites: SpriteSet, leader: [f32; 2], direction: u8) -> Self {
        Self { sprites, at: place_behind(leader, direction, CART_ROPE), rot: rot_of(direction), rolled: 0.0 }
    }

    /// Pulled in to a rope's length when the leader is farther, turned toward it. A leader suddenly far off (a warp,
    /// a teleport) has the cart placed behind it again.
    pub fn step(&mut self, leader: [f32; 2], direction: u8) {
        let [dx, dz] = [leader[0] - self.at[0], leader[1] - self.at[1]];
        // ponytail: more than two ropes away counts as a jump, not a pull.
        if f32::hypot(dx, dz) > CART_ROPE * 3.0 {
            (self.at, self.rot) = (place_behind(leader, direction, CART_ROPE), rot_of(direction));
            return;
        }
        let at = rope_step(self.at, leader, CART_ROPE);
        self.rolled += f32::hypot(at[0] - self.at[0], at[1] - self.at[1]);
        self.at = at;
        let [dx, dz] = [leader[0] - at[0], leader[1] - at[1]];
        if f32::hypot(dx, dz) > 1e-3 {
            self.rot = dx.atan2(-dz).to_degrees().rem_euclid(360.0);
        }
    }

    /// Which of `count` frames shows: the wheels turn with the distance rolled, looping.
    pub fn frame(&self, count: usize) -> usize {
        (self.rolled * CART_FRAMES) as usize % count.max(1)
    }
}

pub struct Companion {
    pub at: [f32; 2],
    /// The exact way it faces, in degrees (as `Actor::rot`): always toward its owner.
    pub rot: f32,
    /// It moved in its last step (the falcon flying in, the warg running).
    pub moving: bool,
    /// The falcon on its way in from where it appeared, until it first comes within 25.
    arriving: bool,
    /// What it plays and since when.
    action: (usize, Instant),
    /// When it last stepped: the rules run per tick over the time since.
    stepped: Instant,
    /// Its shadow, a body's being 1 (the falcon, up in the air, has none).
    pub shadow: f32,
    sprites: Option<SpriteSet>,
}

impl Companion {
    fn spawn(grf: &Grf, warg: bool, job: u16, at: [f32; 2], owner: [f32; 2]) -> Self {
        let (tries, shadow) = companion_sprites(warg, job);
        let sprites = tries.iter().find_map(|(path, act)| SpriteSet::with_act(grf, path, act));
        let now = Instant::now();
        let action = if warg { IDLE } else { WALK };
        Self { at, rot: facing(at, owner, 0.0), moving: false, arriving: !warg, action: (action, now), stepped: now, shadow, sprites }
    }

    pub fn sprites(&self) -> Option<&SpriteSet> {
        self.sprites.as_ref()
    }

    /// Plays `action` from now, unless it already is.
    fn play(&mut self, action: usize, now: Instant) {
        if self.action.0 != action {
            self.action = (action, now);
        }
    }

    /// The falcon flaps (its walk), holding one frame while it arrives. The warg runs, stands, flinches once or lies
    /// dead.
    pub fn frame(&mut self, gpu: &Gpu, direction: u8, now: Instant) -> Vec<Placed> {
        let Some(sprites) = &mut self.sprites else { return Vec::new() };
        let (action, since) = self.action;
        let action = if action < sprites.actions() / 8 { action } else { IDLE };
        let ms = now.saturating_duration_since(since).as_secs_f32() * 1000.0;
        let count = sprites.frames(action * 8 + usize::from(direction % 8));
        let timing = match action {
            _ if self.arriving && count > FALCON_ARRIVING_FRAME => Timing::Span((FALCON_ARRIVING_FRAME as f32 + 0.5) / count as f32),
            WARG_HURT | WARG_DEAD => Timing::Once(ms, 1.0),
            _ => Timing::Loop(ms),
        };
        sprites.frame(gpu, action, direction, 0, timing)
    }
}

/// The exact facing, in degrees, of a step or an offset toward a target: atan2(dx, −dz), so 0 faces south (the
/// camera at its default turn), 90 east, 180 north, 270 west.
pub fn angle_of(step: [i32; 2]) -> f32 {
    (step[0] as f32).atan2(-(step[1] as f32)).to_degrees().rem_euclid(360.0)
}

/// The exact facing of one of the sprites' eight directions (0 south, turning west).
pub fn rot_of(direction: u8) -> f32 {
    (360.0 - f32::from(direction % 8) * 45.0) % 360.0
}

/// Which of its eight directions a unit facing `rot` shows to a camera turned `yaw` degrees: the angle between them,
/// rounded to the nearest direction (`round`) or floored, so the unit switches as soon as the camera turns at all one
/// way.
pub fn view_direction(rot: f32, yaw: f32, round: bool) -> u8 {
    let seen = (-yaw - rot).rem_euclid(360.0);
    let direction = if !round {
        (seen / 45.0) as u8
    } else if !(22.5..=337.5).contains(&seen) {
        0
    } else {
        ((seen + 22.5) / 45.0) as u8
    };
    direction % 8
}

/// Other units show this long after the server moves them.
pub const REMOTE_DELAY: Duration = Duration::from_millis(72);
/// A knockback slides the unit to its new cell over this many ms.
const SLIDE_MS: u64 = 200;
// ponytail: farther than this many cells a move (a pet put back at its owner, say) jumps instead of sliding.
const SLIDE_REACH: f32 = 5.0;

/// When a walk at `speed` ms a cell from `start` (in cells) at `begin` (whole ms, `whole_ms`) reaches each cell of
/// `path`: the first after its straight-line distance, then `speed` a straight step and `speed`·1.414 a diagonal one
/// (rathena counts speed·14/10), each cut to whole ms.
fn arrivals(start: [f32; 2], begin: u64, path: &[[i32; 2]], speed: f32) -> Vec<([i32; 2], u64)> {
    let (mut from, mut at) = (start, begin);
    path.iter()
        .enumerate()
        .map(|(index, &cell)| {
            let to = cell.map(|value| value as f32);
            let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
            at += match index {
                0 => (speed * (dy * dy + dx * dx).sqrt()) as u64,
                _ if dx != 0.0 && dy != 0.0 => (speed * 1.414) as u64,
                _ => speed as u64,
            };
            from = to;
            (cell, at)
        })
        .collect()
}

/// The world point (on a frame centred on a map `half` its size in cells) of a cell position: 5 units a cell, cells'
/// centres 2.5 in.
fn world_point(cell: [f32; 2], half: [f32; 2]) -> [f32; 2] {
    [0, 1].map(|axis| (cell[axis] - half[axis]) * world::CELL + 2.5)
}

/// The facing of a step of `dx`, `dz` world units, as the game works it out: atan2(dx, −dz) in degrees by 3.141592,
/// not π, so every angle is 1.0000002 times too big (north 180.0000458, east 90.0000229, west 269.99997, south 0).
#[allow(clippy::approx_constant)]
pub fn step_rot(dx: f32, dz: f32) -> f32 {
    let rot = (f64::from(dx).atan2(-f64::from(dz)) as f32) / 3.141_592 * 180.0;
    if rot < 0.0 { rot + 360.0 } else { rot }
}

/// When a server tick comes here, by a tick the server told and when that came.
pub fn local_time((tick, at): (u32, Instant), server_tick: u32) -> Instant {
    let ms = server_tick.wrapping_sub(tick) as i32;
    let apart = Duration::from_millis(u64::from(ms.unsigned_abs()));
    if ms >= 0 { at + apart } else { at.checked_sub(apart).unwrap_or(at) }
}

/// A tick the server sends this far behind our clock counts as late; four late in a row step the clock back this much.
const CLOCK_STEP: i64 = 144;
const LATE_IN_A_ROW: u8 = 4;

/// Our guess of the server's tick: set outright on entering a map and by the first ZC_NOTIFY_TIME after a map load,
/// then nudged by every tick the server sends (ZC_NOTIFY_TIME, move entries, our own walks).
#[derive(Default)]
pub struct ServerClock {
    at: Option<(u32, Instant)>,
    late: u8,
}

impl ServerClock {
    pub fn set(&mut self, tick: u32, at: Instant) {
        self.at = Some((tick, at));
        self.late = 0;
        crate::set_shown_clock(tick, at);
    }

    /// The server's tick at `at`, by our guess.
    pub fn tick(&self, at: Instant) -> Option<u32> {
        let (tick, then) = self.at?;
        let ms = if at >= then { at.duration_since(then).as_millis() as i64 } else { -(then.duration_since(at).as_millis() as i64) };
        Some(tick.wrapping_add(ms as u32))
    }

    /// A tick heard at `at`: one ahead of our guess sets it at once; more than 144 ms behind on four packets in a row
    /// steps it back 144 ms; anything else starts that count again.
    pub fn heard(&mut self, tick: u32, at: Instant) {
        let Some(guess) = self.tick(at) else { return self.set(tick, at) };
        let ahead = i64::from(tick.wrapping_sub(guess) as i32);
        if ahead > 0 {
            self.set(tick, at);
        } else if -ahead > CLOCK_STEP {
            self.late += 1;
            if self.late == LATE_IN_A_ROW
                && let Some((tick, then)) = self.at
            {
                self.set(tick.wrapping_sub(CLOCK_STEP as u32), then);
            }
        } else {
            self.late = 0;
        }
    }

    /// When the server's `tick` is on our clock.
    pub fn local(&self, tick: u32) -> Option<Instant> {
        self.at.map(|clock| local_time(clock, tick))
    }
}

/// How opaque a unit spawned in view is this long after: 68/255, then 8 more every 1/60 s, opaque after 24 steps
/// (400 ms).
// ponytail: the steps counted to the nearest 60th of a second.
pub fn spawn_alpha(since: Duration) -> f32 {
    let steps = (since.as_secs_f32() * 60.0).round();
    (68.0 + 8.0 * steps).min(255.0) / 255.0
}

/// Something on the map that moves cell by cell.
pub struct Actor {
    pub name: String,
    /// Object type from the map server (player, NPC, monster, ...).
    pub kind: u8,
    /// Job (class, or NPC/monster sprite) ID.
    pub job: u16,
    /// A player's guild (0: none).
    pub guild: u32,
    /// The level the server shows (monsters' and players'; 0 for NPCs and pets).
    pub level: u16,
    /// A player's looks, to rebuild its sprites when they change.
    pub look: Option<Look>,
    /// Script NPCs the client never draws.
    pub hidden: bool,
    pub sprites: Option<SpriteSet>,
    pub cell: [i32; 2],
    /// The cells ahead and when (whole ms) the unit reaches each.
    path: Vec<([i32; 2], u64)>,
    /// The walk's nodes already reached, for a shown clock that steps back.
    passed: Vec<([i32; 2], u64)>,
    /// Where it's drawn, in world units on a frame centred on its map (`half` the map's size in cells), and when it
    /// was last updated (whole ms). Each update moves it toward the end of the step under way.
    at: [f32; 2],
    half: [f32; 2],
    last: u64,
    /// Milliseconds per straight step (the server's speed).
    pub step_ms: f32,
    /// The walk's odometer: world units it has been drawn moving along a path, since it appeared. Each update adds
    /// the way it moved, so a changed walk never winds the legs back.
    distance: f32,
    /// Cells a finished step just left (not a slide). Cleared at the start of the next update.
    stepped: Vec<[i32; 2]>,
    /// Chase-walk footprints left so far, so 326 and 327 alternate.
    pub footprints: u32,
    /// The facing in the sprites' eight directions, and exactly, in degrees (`angle_of`, `step_rot`: 0 south, 90
    /// east).
    pub direction: u8,
    pub rot: f32,
    /// The facing came from the server (appearing, turning) rather than from walking or turning to a target.
    pub facing_from_server: bool,
    /// Sliding (a knockback): the path is one straight move that doesn't turn the unit.
    sliding: bool,
    /// Frames left showing the walk after a stop that put it on the stop cell's centre.
    walk_frames: u8,
    /// Players only: 0 looking straight ahead, 1 and 2 turned to the sides. `kept_head` is the one its last entry
    /// packet or own turn gave, which it goes back to after doing something else (`turn`).
    pub head_direction: u16,
    kept_head: u16,
    animation_started: Instant,
    shot: Option<Shot>,
    /// A player who fought stands ready until moving again.
    ready: bool,
    /// Sitting down (until it stands or walks).
    pub sitting: bool,
    /// When a dying unit is removed.
    pub remove_at: Option<Instant>,
    /// Fading out: from when, over how many milliseconds (a corpse, a unit walking out of sight).
    pub fade: Option<(Instant, f32)>,
    /// Spawned in view: fading in since then (`spawn_alpha`).
    pub fade_in: Option<Instant>,
    /// A dead player: lying there until revived (players aren't removed when they die).
    pub dead: bool,
    /// Its cast bar is up (`Game::casts`), set each frame before `update`.
    pub casting: bool,
    /// Hiding, cloaking or chase-walking (the unit's option has UNSEEN_OPTIONS).
    pub cloaked: bool,
    /// The option bits that hide it (`seen`).
    pub hide_option: u32,
    /// rathena's opt1 and opt2: the body and health state that colour the sprite.
    pub body_state: u16,
    pub health_state: u16,
    /// The pushcart it pulls.
    pub cart: Option<Cart>,
    /// The server's option word (rathena OPTION_*), falcon and warg included.
    pub option: u32,
    /// The falcon, while OPTION_FALCON is set.
    pub falcon: Option<Companion>,
    /// The warg, while OPTION_WUG is set and the player is not riding it.
    pub warg: Option<Companion>,
    /// The action and frame last seen, so a frame's sound plays once each time it comes up.
    heard: Option<(usize, usize)>,
    /// Locked by Blade Stop or Cursed Circle since then: the animation holds that frame.
    pub held: Option<Instant>,
    /// ChangeSize's percent, when that status is on. It multiplies the size table's factor.
    pub size_percent: Option<i32>,
    /// The statuses on that leave copies of the unit behind it (the quickens and the like), and the copies.
    blurs: Vec<u16>,
    /// What statuses do to how its own parts are drawn: by status, the body flags and colour (`status_body`).
    glows: Vec<(u16, u32, Option<[u8; 3]>)>,
    pub ghosts: Vec<Ghost>,
    /// Cursed Circle: the body darkened.
    pub cursed: bool,
}

/// The effect that leaves copies of a unit behind it while a status is on (the quickens, Overthrust...).
pub const AFTERIMAGE: u32 = 166;

/// How a status has its unit's own parts drawn: the body, head and headgears added on; the weapon added on; two grown
/// added-on copies of the body, head and headgears (Assumptio); four growing copies of every part, the part itself at
/// alpha 150 (Kaite).
pub const GLOW_ADDITIVE: u32 = 0x1;
pub const GLOW_WEAPON: u32 = 0x2;
pub const GLOW_OUTLINE: u32 = 0x8;
pub const GLOW_REFLECT: u32 = 0x200;

/// The body flags and colour a status's looping effect gives its unit: Berserk red, Marionette pink with the outline,
/// Assumptio's outline, Kaite's copies, Aura Blade's weapon.
pub fn status_body(effect: u32) -> Option<(u32, Option<[u8; 3]>)> {
    match effect {
        368 => Some((GLOW_ADDITIVE, Some([0xFF, 0x64, 0x64]))),
        396 => Some((GLOW_ADDITIVE | GLOW_OUTLINE, Some([0xFF, 0x59, 0xB6]))),
        375 => Some((GLOW_OUTLINE, None)),
        419 => Some((GLOW_REFLECT, None)),
        474 => Some((GLOW_WEAPON, None)),
        _ => None,
    }
}

/// Which of a unit's parts an image is, for the body flags.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PartGroup {
    /// The body, head and headgears (and a monster's or an NPC's one sprite).
    Body,
    /// The weapon and its glow.
    Weapon,
    /// The shield and the garment.
    Gear,
}

/// One copy drawn for a part under the body flags: added on or blended, the colour to multiply by (alpha included),
/// and how many screen pixels it's grown by on every side.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlowCopy {
    pub additive: bool,
    pub color: [f32; 4],
    pub grow: f32,
}

/// How a part is drawn under the body flags `flags` and colour `tint`: its own copy (added on, or blended at its own
/// alpha), and the copies around it. `breath` is Assumptio's angle in degrees (0–180), `spread` Kaite's 0–20, and
/// `unit` the screen pixels one unit of growth takes (400 / the camera's distance).
pub fn glow_copies(flags: u32, tint: Option<[u8; 3]>, group: PartGroup, breath: f32, spread: f32, unit: f32) -> (GlowCopy, Vec<GlowCopy>) {
    let color = tint.map_or([1.0; 4], |[red, green, blue]| [f32::from(red) / 255.0, f32::from(green) / 255.0, f32::from(blue) / 255.0, 1.0]);
    let body = group == PartGroup::Body;
    let additive = (body && flags & GLOW_ADDITIVE != 0) || (group == PartGroup::Weapon && flags & GLOW_WEAPON != 0);
    let mut part = GlowCopy { additive, color, grow: 0.0 };
    let mut copies = Vec::new();
    if body && flags & GLOW_OUTLINE != 0 {
        let sin = breath.to_radians().sin();
        copies.push(GlowCopy { additive: true, color: [1.0, 1.0, 1.0, 100.0 / 255.0], grow: (3.0 + sin) * unit });
        copies.push(GlowCopy { additive: true, color: [1.0, 1.0, 1.0, 120.0 / 255.0], grow: (5.0 + 2.0 * sin) * unit });
    }
    if flags & GLOW_REFLECT != 0 && !part.additive {
        for k in [5.0, 10.0, 15.0, 20.0] {
            let s = (spread + k) % 20.0;
            copies.push(GlowCopy { additive: false, color: [1.0, 1.0, 1.0, (100.0 - 5.0 * s) / 255.0], grow: s * unit });
        }
        part.color = [1.0, 1.0, 1.0, 150.0 / 255.0];
    }
    (part, copies)
}

/// Cursed Circle's statuses, which darken the body to this.
pub const CURSED_CIRCLE: [u16; 2] = [416, 417];
pub const CURSED_TINT: [u8; 3] = [64, 64, 64];
/// A copy is left every 5 ticks, and fades by 16 of 255 a tick until under 16.
const GHOST_EVERY: f32 = 5.0;
const GHOST_FADE: f32 = 16.0;

/// A copy of the unit left behind while it blurs: when, where its feet were, and the frame it showed.
pub struct Ghost {
    pub left: Instant,
    pub feet: [f32; 3],
    pub placed: Vec<Placed>,
}

/// How see-through a copy left `age` ago is drawn: 255 less 16 a tick, out of 255; gone (0) once under 16.
// ponytail: the copies keep the unit's own colours; how the game tints them isn't pinned down.
pub fn ghost_alpha(age: Duration) -> f32 {
    let alpha = 255.0 - GHOST_FADE * (age.as_secs_f32() / TICK).floor();
    if alpha < GHOST_FADE { 0.0 } else { alpha / 255.0 }
}

impl Actor {
    pub fn new(name: String, sprites: Option<SpriteSet>, cell: [i32; 2], direction: u8, speed: u16) -> Self {
        let now = Instant::now();
        Self {
            name,
            kind: 0,
            job: 0,
            guild: 0,
            level: 0,
            look: None,
            hidden: false,
            sprites,
            cell,
            path: Vec::new(),
            passed: Vec::new(),
            at: world_point(cell.map(|value| value as f32), [0.0; 2]),
            half: [0.0; 2],
            last: crate::shown_ms(crate::control::now()),
            walk_frames: 0,
            step_ms: f32::from(speed.max(1)),
            distance: 0.0,
            stepped: Vec::new(),
            footprints: 0,
            direction,
            rot: rot_of(direction),
            facing_from_server: true,
            sliding: false,
            head_direction: 0,
            kept_head: 0,
            animation_started: now,
            shot: None,
            ready: false,
            sitting: false,
            remove_at: None,
            fade: None,
            fade_in: None,
            dead: false,
            casting: false,
            cloaked: false,
            hide_option: 0,
            body_state: 0,
            health_state: 0,
            cart: None,
            option: 0,
            falcon: None,
            warg: None,
            heard: None,
            held: None,
            size_percent: None,
            blurs: Vec::new(),
            glows: Vec::new(),
            ghosts: Vec::new(),
            cursed: false,
        }
    }

    /// The colour the body is drawn in: Cursed Circle's dark, else the body and health states' (see `status_tint`).
    pub fn body_tint(&self) -> Option<[u8; 3]> {
        if self.cursed { Some(CURSED_TINT) } else { status_tint(self.body_state, self.health_state) }
    }

    /// A status comes on or goes off: what its looping effects do to how the parts are drawn.
    pub fn glow(&mut self, status: u16, effects: &[u32], on: bool) {
        self.glows.retain(|&(each, ..)| each != status);
        if on {
            self.glows.extend(effects.iter().filter_map(|&effect| status_body(effect)).map(|(flags, tint)| (status, flags, tint)));
        }
    }

    /// The body flags of every status on, and the first colour.
    pub fn glow_flags(&self) -> (u32, Option<[u8; 3]>) {
        (self.glows.iter().fold(0, |flags, &(_, each, _)| flags | each), self.glows.iter().find_map(|&(.., tint)| tint))
    }

    /// A status that leaves copies behind it comes on or goes off.
    pub fn blur(&mut self, status: u16, on: bool) {
        self.blurs.retain(|&each| each != status);
        if on {
            self.blurs.push(status);
        }
    }

    /// Leaves a copy of the frame drawn at `feet` every 5 ticks while a blurring status is on; old copies go.
    pub fn leave_ghost(&mut self, now: Instant, feet: [f32; 3], placed: &[Placed]) {
        self.ghosts.retain(|ghost| ghost_alpha(now.saturating_duration_since(ghost.left)) > 0.0);
        let due = self.ghosts.last().is_none_or(|ghost| now.saturating_duration_since(ghost.left).as_secs_f32() >= GHOST_EVERY * TICK);
        if !self.blurs.is_empty() && due && !placed.is_empty() {
            self.ghosts.push(Ghost { left: now, feet, placed: placed.to_vec() });
        }
    }

    /// The sounds of the ACT frames the animation passed since it was last asked (`frames_passed`).
    pub fn sounds_now(&mut self, now: Instant) -> Vec<String> {
        let (action, timing) = self.animation(now);
        let index = action * 8 + usize::from(self.direction % 8);
        let Some((frame, _)) = self.sprites.as_ref().and_then(|sprites| sprites.body_frame(index, timing)) else { return Vec::new() };
        let last = self.heard.replace((action, frame));
        let Some(sprites) = &self.sprites else { return Vec::new() };
        frames_passed(last, (action, frame), sprites.frames(index))
            .into_iter()
            .filter_map(|frame| sprites.frame_sound(index, frame))
            .map(str::to_owned)
            .collect()
    }

    /// Plays `action` once from `start` over `duration_ms` (attack, flinch), or holds its last frame (death).
    /// Players stay in their battle stance after attacking or being hit.
    pub fn stand_ready(&mut self) {
        if self.look.is_some() {
            self.ready = true;
        }
    }

    /// Back on its feet: whatever it was holding (dead on the ground) ends.
    pub fn revive(&mut self) {
        self.shot = None;
        self.dead = false;
    }

    pub fn play(&mut self, action: usize, start: Instant, duration_ms: f32, hold: bool) {
        self.shot = Some(Shot { action, started: start, pace: Pace::Stretch(duration_ms.max(50.0)), hold });
        self.head_direction = self.kept_head;
    }

    /// The head as an entry packet or our own turn gives it: shown, and gone back to after doing something else.
    pub fn set_head(&mut self, head: u16) {
        (self.head_direction, self.kept_head) = (head, head);
    }

    /// ZC_CHANGE_DIRECTION: the body turns; the head turn shows only on one standing or sitting as it comes, and isn't
    /// kept (a turn heard while walking is dropped).
    pub fn turn(&mut self, direction: u8, head: u16, now: Instant) {
        self.set_direction(direction);
        if !self.moving() && matches!(self.animation(now).0, IDLE | READY | SIT) {
            self.head_direction = head;
        }
    }

    /// The job the unit is drawn with: a player look's `sprite_job` (outfits, the wedding dress), else its job.
    pub fn drawn_job(&self) -> u16 {
        if self.look.is_some() { sprite_job(self.job, self.option) } else { self.job }
    }

    /// Plays `action` once at its ACT pace times `scale` (attack, flinch), or holds its last frame (death).
    /// How opaque it's drawn: fading in after a spawn; 1 until a fade out starts, then down to 0 over the fade.
    pub fn alpha(&self, now: Instant) -> f32 {
        let out = self.fade.map_or(1.0, |(from, ms)| (1.0 - now.saturating_duration_since(from).as_secs_f32() * 1000.0 / ms).clamp(0.0, 1.0));
        out.min(self.fade_in.map_or(1.0, |from| spawn_alpha(now.saturating_duration_since(from))))
    }

    /// Casts no shadow: fading out, or not yet opaque after a spawn.
    pub fn shadowless(&self, now: Instant) -> bool {
        self.fading(now) || self.fade_in.is_some_and(|from| spawn_alpha(now.saturating_duration_since(from)) < 1.0)
    }

    /// Fading now (a corpse, a unit out of sight): it casts no shadow.
    pub fn fading(&self, now: Instant) -> bool {
        self.fade.is_some_and(|(from, _)| now >= from)
    }

    /// Fades out from `from` over `ms`, then goes.
    pub fn fade_out(&mut self, from: Instant, ms: f32) {
        self.fade = Some((from, ms));
        self.remove_at = Some(from + Duration::from_micros((f64::from(ms) * 1000.0).round() as u64));
    }

    /// Flinches (action `action`) from a hit with the target motion `motion` (rathena's dMotion); see `flinch_pace`.
    pub fn flinch(&mut self, action: usize, start: Instant, motion: f32) {
        let index = action * 8 + usize::from(self.direction % 8);
        let delay = self.sprites.as_ref().and_then(|sprites| sprites.layers[0].act.delays.get(index).copied()).unwrap_or(4.0).max(1.0);
        self.shot = Some(Shot { action, started: start, pace: flinch_pace(motion / 288.0, self.kind == 0, delay), hold: false });
    }

    /// Plays `action` once at its ACT pace, then holds its last frame until `ms` after `start` (a cast).
    pub fn hold_for(&mut self, action: usize, start: Instant, ms: f32) {
        let length = self.sprites.as_ref().and_then(|sprites| sprites.index_ms(action * 8 + usize::from(self.direction % 8))).unwrap_or(0.0);
        self.shot = Some(Shot { action, started: start, pace: Pace::Hold((ms / length.max(1.0)).max(1.0)), hold: false });
        self.head_direction = self.kept_head;
    }

    /// Its attack action: a player's swing for its weapon.
    pub fn attack_motion(&self, tables: &LookTables) -> usize {
        self.look.as_ref().map_or(combat_actions(self.kind).0, |look| attack_action(look.job, look.sex, tables.weapon_type(look.weapon)))
    }

    /// A player holding a bow: its swings play at an uncapped pace and land with their arrow.
    pub fn holds_bow(&self, tables: &LookTables) -> bool {
        self.look.as_ref().is_some_and(|look| tables.weapon_type(look.weapon) == crate::BOW)
    }

    /// In `action` now, played once or held (a swing, a skill's motion, a cast), rather than standing or walking.
    pub fn playing(&self, action: usize, now: Instant) -> bool {
        matches!(self.animation(now), (playing, Timing::Once(..) | Timing::Span(_)) if playing == action)
    }

    /// Back to standing from a held `action` (a cast cut short).
    pub fn stop_holding(&mut self, action: usize) {
        if self.shot.as_ref().is_some_and(|shot| shot.action == action) {
            self.shot = None;
        }
    }

    /// In its own attack swing now.
    pub fn swinging(&self, now: Instant) -> bool {
        self.animation(now).0 == combat_actions(self.kind).0
    }

    /// In a motion a walk waits for: sitting, or a one-shot action playing (its attack swing, the pickup, a skill's
    /// motion), but not a flinch, death or a held cast.
    pub fn busy(&self, now: Instant) -> bool {
        let (_, flinch, death) = combat_actions(self.kind);
        let (action, timing) = self.animation(now);
        let held_cast = self.shot.as_ref().is_some_and(|shot| matches!(shot.pace, Pace::Hold(_)));
        self.sitting || (matches!(timing, Timing::Once(..) | Timing::Span(_)) && action != flinch && action != death && !held_cast)
    }

    /// Stops walking once the step under way is done (a slide goes on).
    pub fn stop_after_step(&mut self) {
        if !self.sliding {
            self.path.truncate(1);
        }
    }

    pub fn play_at_pace(&mut self, action: usize, start: Instant, scale: f32, hold: bool) {
        self.shot = Some(Shot { action, started: start, pace: Pace::Act(scale.max(0.01)), hold });
        self.head_direction = self.kept_head;
    }

    /// Where the unit is drawn, in cells (centres on the integers), as the last update left it, whatever `now`.
    pub fn position(&self, _now: Instant) -> [f32; 2] {
        [0, 1].map(|axis| (self.at[axis] - 2.5) / world::CELL + self.half[axis])
    }

    /// The cell and sub-cell under its drawn point, as the game works them out from the map-centred point: per axis
    /// v = (W div 2) + x / 5, the cell floor(v) and the sub-cell the sixteenths past it. On an odd-sized map a unit on a
    /// cell's centre gets sub 0.
    pub fn cell_and_sub(&self) -> ([i32; 2], [u8; 2]) {
        let v = [0, 1].map(|axis| ((self.half[axis] * 2.0) as i32 / 2) as f32 + self.at[axis] / world::CELL);
        let cell = v.map(|value| value.floor() as i32);
        let sub = [0, 1].map(|axis| (((v[axis] - cell[axis] as f32) * 16.0).floor() as i32).rem_euclid(16) as u8);
        (cell, sub)
    }

    /// The map it's on, its GAT's width and height: the frame its drawn point is kept in. It stays where it's drawn.
    pub fn set_map(&mut self, size: [usize; 2]) {
        let drawn = self.position(Instant::now());
        self.half = size.map(|cells| cells as f32 * 0.5);
        self.at = world_point(drawn, self.half);
    }

    /// Turns to face a cell: the nearest of the eight directions, and its exact angle.
    pub fn face(&mut self, toward: [i32; 2]) {
        if toward != self.cell {
            let step = [toward[0] - self.cell[0], toward[1] - self.cell[1]];
            self.direction = world::direction(step);
            self.rot = angle_of(step);
            self.facing_from_server = false;
        }
    }

    /// Faces one of the eight directions (the server's, or our own turn).
    pub fn set_direction(&mut self, direction: u8) {
        self.direction = direction;
        self.rot = rot_of(direction);
        self.facing_from_server = true;
    }

    /// Walks `path` as the server has it: from `from` and its sixteenths of a cell `sub` (8: the centre), beginning at
    /// `begin`. A walk that would be over already plays from now at the speed·10/13 (whole ms). Only the path changes:
    /// a unit already `known` on the map goes on from where it's drawn; a new one starts on `from`'s point, as of now.
    pub fn walk_from(&mut self, from: [i32; 2], sub: [u8; 2], path: Vec<[i32; 2]>, begin: Instant, now: Instant, known: bool) {
        self.start_walking(now);
        self.cell = from;
        let start = [0, 1].map(|axis| from[axis] as f32 + (f32::from(sub[axis]) - 8.0) / 16.0);
        let now_ms = crate::shown_ms(now);
        if !known {
            (self.at, self.last) = (world_point(start, self.half), now_ms);
        }
        let mut steps = arrivals(start, crate::shown_ms(begin), &path, self.step_ms);
        if steps.last().is_some_and(|&(_, arrives)| arrives <= now_ms) {
            steps = arrivals(start, now_ms, &path, (self.step_ms as u64 * 10 / 13) as f32);
        }
        self.path = steps;
        self.passed.clear();
    }

    /// Put on `cell` at once (a knockback): no slide, and what it was doing goes on.
    pub fn place_at(&mut self, cell: [i32; 2]) {
        self.cell = cell;
        self.at = world_point(cell.map(|value| value as f32), self.half);
        self.sliding = false;
    }

    fn start_walking(&mut self, now: Instant) {
        // A walk's path replaces a slide's.
        self.sliding = false;
        self.ready = false;
        self.sitting = false;
        // Walking looks straight ahead again.
        self.set_head(0);
        // Walking interrupts an attack or flinch, not a death.
        if self.shot.as_ref().is_some_and(|shot| !shot.hold) {
            self.shot = None;
        }
        if self.path.is_empty() {
            self.animation_started = now;
        }
    }

    pub fn stop_at(&mut self, cell: [i32; 2]) {
        self.cell = cell;
        self.at = world_point(cell.map(|value| value as f32), self.half);
        self.sliding = false;
        if !self.path.is_empty() {
            self.animation_started = Instant::now();
        }
        self.path.clear();
        self.passed.clear();
    }

    /// ZC_STOPMOVE: `stop` becomes its cell and its walk ends. Drawn on the stop cell, or with no path from the drawn
    /// cell to it (`path`), it stands where it's drawn; else it goes to the stop cell's centre at once, showing its walk
    /// for that frame. It faces as it was either way.
    pub fn stop_drawn(&mut self, stop: [i32; 2], path: bool, now: Instant) {
        let (drawn, at) = (self.position(now), self.at);
        let walking = !self.path.is_empty();
        self.stop_at(stop);
        if drawn_cell(drawn) == stop || !path {
            self.at = at;
        } else if walking {
            self.walk_frames = 2;
        }
    }

    /// The cell its walk ends on (the one it's on when it isn't walking).
    pub fn destination(&self) -> [i32; 2] {
        self.path.last().map_or(self.cell, |&(cell, _)| cell)
    }

    pub fn moving(&self) -> bool {
        !self.path.is_empty()
    }

    /// Cells a step finished on this update just left, until the next update.
    pub fn steps_left(&self) -> &[[i32; 2]] {
        &self.stepped
    }

    /// One frame: the cell follows the node times; the drawn point closes (now − last) / (end − last) of the way on
    /// the end of the step under way (past it on a frame late for the last node), facing the way it moved and adding
    /// it to the odometer; past the last node it stands on it. Returns where it's drawn, in cells.
    pub fn update(&mut self, now: Instant) -> [f32; 2] {
        let now_ms = crate::shown_ms(now);
        self.stepped.clear();
        self.walk_frames = self.walk_frames.saturating_sub(1);
        // The shown clock stepped back: nodes it hasn't reached again come back ahead, for the glide. The cell stays
        // where the walk got to.
        while let Some(&node) = self.passed.last().filter(|&&(_, arrives)| arrives > now_ms) {
            self.passed.pop();
            self.path.insert(0, node);
        }
        let mut passed = None;
        while let Some(&(next, arrives)) = self.path.first().filter(|&&(_, arrives)| arrives <= now_ms) {
            if !self.sliding {
                self.stepped.push(self.cell);
            }
            self.cell = next;
            self.path.remove(0);
            self.passed.push((next, arrives));
            passed = Some((next, arrives));
        }
        if let Some((to, end)) = self.path.first().copied().or(passed) {
            let target = world_point(to.map(|value| value as f32), self.half);
            let (whole, gone) = (end.saturating_sub(self.last), now_ms.saturating_sub(self.last));
            let before = self.at;
            if whole > 0 && gone > 0 {
                let share = gone as f32 / whole as f32;
                self.at = [0, 1].map(|axis| (target[axis] - self.at[axis]) * share + self.at[axis]);
            }
            let (dx, dz) = (self.at[0] - before[0], self.at[1] - before[1]);
            if dx != 0.0 || dz != 0.0 {
                // A slide doesn't turn the unit.
                if !self.sliding {
                    self.rot = step_rot(dx, dz);
                    self.direction = view_direction(self.rot, 0.0, true);
                    self.facing_from_server = false;
                }
                self.distance += (dx * dx + dz * dz).sqrt();
            }
            if self.path.is_empty() {
                self.at = target;
                self.animation_started = now;
                self.sliding = false;
                self.passed.clear();
            }
        }
        // A player in its swing, sitting, dead, casting or in a skill's motion keeps the old time, so its first walking
        // frame after one covers more of the step.
        if !self.keeps_glide_clock(now) {
            self.last = now_ms;
        }
        self.position(now)
    }

    /// Whether this frame leaves the glide's last update time as it was: a player (our own or another) in its swing,
    /// sitting, dead, casting or in a skill's motion. Every other unit, and a player standing, walking, flinching or
    /// picking up, sets it every frame.
    // ponytail: every one-shot action but a flinch and the pickup counts as a swing or a skill's motion, the special
    // motions some skills play among them.
    fn keeps_glide_clock(&self, now: Instant) -> bool {
        if self.kind != 0 {
            return false;
        }
        let (action, timing) = self.animation(now);
        let (_, flinch, _) = combat_actions(self.kind);
        let motion = matches!(timing, Timing::Once(..) | Timing::Span(_)) && action != flinch && action != PICKUP;
        self.dead || self.sitting || self.casting || motion
    }

    /// Slides straight to `cell` from where it is drawn, over 200 ms, facing as it was. A long way jumps.
    pub fn slide_to(&mut self, cell: [i32; 2], now: Instant) {
        let from = self.position(now);
        if f32::hypot(cell[0] as f32 - from[0], cell[1] as f32 - from[1]) > SLIDE_REACH {
            self.stop_at(cell);
            return;
        }
        self.path = vec![(cell, crate::shown_ms(now) + SLIDE_MS)];
        self.passed.clear();
        self.sliding = true;
    }

    /// The cell a slide under way ends on.
    pub fn sliding_to(&self) -> Option<[i32; 2]> {
        self.path.last().map(|&(cell, _)| cell).filter(|_| self.sliding)
    }

    /// Current animation and how far into it: a one-shot action while it plays, else idle or walk.
    pub fn animation(&self, now: Instant) -> (usize, Timing) {
        // ponytail: a locked unit holding its frame is an estimate of how the official client shows it.
        let now = self.held.map_or(now, |at| at.min(now));
        if let Some(shot) = self.shot.as_ref().filter(|shot| shot.started <= now) {
            let elapsed = now.saturating_duration_since(shot.started).as_secs_f32() * 1000.0;
            match shot.pace {
                Pace::Stretch(duration) if elapsed < duration || shot.hold => return (shot.action, Timing::Span((elapsed / duration).min(1.0))),
                Pace::Act(scale) => {
                    let length = self.sprites.as_ref().and_then(|sprites| sprites.index_ms(shot.action * 8 + usize::from(self.direction % 8))).unwrap_or(0.0);
                    if elapsed < length * scale || shot.hold {
                        return (shot.action, Timing::Once(elapsed, scale));
                    }
                }
                Pace::Hold(factor) => {
                    let length = self.sprites.as_ref().and_then(|sprites| sprites.index_ms(shot.action * 8 + usize::from(self.direction % 8))).unwrap_or(0.0);
                    if elapsed < length * factor {
                        return (shot.action, Timing::Once(elapsed, 1.0));
                    }
                }
                Pace::Stretch(_) => {}
            }
        }
        let action = if (self.moving() && !self.sliding) || self.walk_frames > 0 {
            WALK
        } else if self.sitting {
            SIT
        } else if self.ready {
            READY
        } else {
            IDLE
        };
        if action == WALK {
            return (action, Timing::Walk(self.distance));
        }
        (action, Timing::Loop(now.saturating_duration_since(self.animation_started).as_secs_f32() * 1000.0))
    }

    /// Shows or hides the falcon and the warg from the option word. A new one starts behind the player.
    pub fn sync_companions(&mut self, option: u32, grf: &Grf) {
        self.option = option;
        // Monsters and NPCs carry no player look, so the bits aren't a falcon or a warg.
        if self.look.is_none() {
            self.falcon = None;
            self.warg = None;
            return;
        }
        let leader = world_xz(self.cell.map(|value| value as f32));
        let job = self.look.as_ref().map_or(self.job, |look| look.job);
        if falcon_out(option) && self.falcon.is_none() {
            self.falcon = Some(Companion::spawn(grf, false, job, place_behind(leader, self.direction, FALCON_START), leader));
        } else if !falcon_out(option) {
            self.falcon = None;
        }
        if warg_out(option) && self.warg.is_none() {
            self.warg = Some(Companion::spawn(grf, true, job, place_behind(leader, self.direction, WARG_START), leader));
        } else if !warg_out(option) {
            self.warg = None;
        }
    }

    /// The cart's rope step, and the falcon's and warg's steps for the ticks since their last, turned toward the player.
    /// The warg flinches when the player does and lies down when the player dies.
    pub fn step_companions(&mut self, leader: [f32; 2], now: Instant) {
        if let Some(cart) = &mut self.cart {
            cart.step(leader, self.direction);
        }
        let owner_action = self.animation(now).0;
        if let Some(falcon) = &mut self.falcon {
            let ticks = now.saturating_duration_since(falcon.stepped).as_secs_f32() / TICK;
            let (at, arriving) = falcon_step(falcon.at, leader, falcon.arriving, ticks);
            falcon.moving = f32::hypot(at[0] - falcon.at[0], at[1] - falcon.at[1]) > 1e-3;
            (falcon.at, falcon.arriving, falcon.stepped) = (at, arriving, now);
            falcon.rot = facing(at, leader, falcon.rot);
        }
        if let Some(warg) = &mut self.warg {
            let ticks = now.saturating_duration_since(warg.stepped).as_secs_f32() / TICK;
            let (at, ran) = warg_step(warg.at, leader, ticks);
            (warg.at, warg.moving, warg.stepped) = (at, ran, now);
            warg.rot = facing(at, leader, warg.rot);
            let action = match owner_action {
                PLAYER_DEAD => WARG_DEAD,
                PLAYER_FLINCH => WARG_HURT,
                _ if ran => WALK,
                _ => IDLE,
            };
            warg.play(action, now);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draws_parts_again_for_statuses() {
        let near = |a: f32, b: f32| (a - b).abs() < 1e-4;
        // Berserk: the body added on in #FF6464, the weapon blended in the same colour, nothing grown.
        let (flags, tint) = status_body(368).unwrap();
        let (body, copies) = glow_copies(flags, tint, PartGroup::Body, 0.0, 0.0, 1.0);
        assert!(body.additive && copies.is_empty() && near(body.color[1], 100.0 / 255.0));
        assert!(!glow_copies(flags, tint, PartGroup::Weapon, 0.0, 0.0, 1.0).0.additive);
        // Assumptio: two added-on copies of the body, grown 3 + sin and 5 + 2·sin units (at 90°: 4 and 7), alpha 100 and
        // 120; none for the weapon.
        let (flags, tint) = status_body(375).unwrap();
        let (body, copies) = glow_copies(flags, tint, PartGroup::Body, 90.0, 0.0, 400.0 / 300.0);
        assert!(!body.additive);
        assert_eq!(copies.len(), 2);
        assert!(copies.iter().all(|copy| copy.additive));
        assert!(near(copies[0].grow, 4.0 * 400.0 / 300.0) && near(copies[1].grow, 7.0 * 400.0 / 300.0));
        assert!(near(copies[0].color[3], 100.0 / 255.0) && near(copies[1].color[3], 120.0 / 255.0));
        assert!(glow_copies(flags, tint, PartGroup::Weapon, 90.0, 0.0, 1.0).1.is_empty());
        // Kaite: every part at alpha 150 with four growing copies 5 apart, fainter as they grow.
        let (flags, tint) = status_body(419).unwrap();
        let (part, copies) = glow_copies(flags, tint, PartGroup::Gear, 0.0, 2.0, 1.0);
        assert!(near(part.color[3], 150.0 / 255.0));
        assert_eq!(copies.iter().map(|copy| copy.grow).collect::<Vec<_>>(), [7.0, 12.0, 17.0, 2.0]);
        assert!(near(copies[3].color[3], 90.0 / 255.0) && !copies[0].additive);
        // Aura Blade: the weapon added on; Marionette pink with the outline.
        assert!(glow_copies(status_body(474).unwrap().0, None, PartGroup::Weapon, 0.0, 0.0, 1.0).0.additive);
        assert_eq!(status_body(396), Some((GLOW_ADDITIVE | GLOW_OUTLINE, Some([0xFF, 0x59, 0xB6]))));
        assert_eq!(status_body(166), None);
    }

    #[test]
    fn stops_where_drawn() {
        assert_eq!([drawn_cell([13.5, 10.0]), drawn_cell([14.49, 10.0]), drawn_cell([13.49, 9.5])], [[14, 10], [14, 10], [13, 10]]);
        // 150 ms a cell, walking east from (12, 10) to (16, 10); stopped at `at` ms with the stop cell (14, 10).
        let start = ms(0);
        let walker = |at_ms: u64| {
            let mut unit = Actor::new("Poring".into(), None, [12, 10], 0, 150);
            unit.step_ms = 150.0;
            unit.walk_from([12, 10], [8, 8], vec![[13, 10], [14, 10], [15, 10], [16, 10]], start, start, false);
            let now = start + Duration::from_millis(at_ms);
            unit.update(now);
            (unit, now)
        };
        let east = 6;
        // Drawn at 14.33: stands there, on cell 14, facing east.
        let (mut unit, now) = walker(350);
        let drawn = unit.position(now);
        assert!((drawn[0] - 14.333).abs() < 0.01);
        unit.stop_drawn([14, 10], true, now);
        assert_eq!((unit.position(now), unit.cell, unit.direction, unit.moving()), (drawn, [14, 10], east, false));
        // Drawn at 13.5: counts as cell 14, stays.
        let (mut unit, now) = walker(225);
        unit.stop_drawn([14, 10], true, now);
        assert!((unit.position(now)[0] - 13.5).abs() < 0.01);
        // Drawn at 13.4 (cell 13) with a path: on (14, 10) at once, still facing east, the walk shown one frame.
        let (mut unit, now) = walker(210);
        unit.stop_drawn([14, 10], true, now);
        assert_eq!((unit.position(now), unit.direction), ([14.0, 10.0], east));
        unit.update(now);
        assert_eq!(unit.animation(now).0, WALK);
        unit.update(now);
        assert_ne!(unit.animation(now).0, WALK);
        // No path from the drawn cell: stays where drawn, its cell the stop cell.
        let (mut unit, now) = walker(210);
        let drawn = unit.position(now);
        unit.stop_drawn([14, 10], false, now);
        assert_eq!((unit.position(now), unit.cell), (drawn, [14, 10]));
        // Standing, a stop on its own cell: nothing moves.
        let mut standing = Actor::new("Poring".into(), None, [5, 5], 0, 150);
        standing.stop_drawn([5, 5], false, start);
        assert_eq!(standing.position(start), [5.0, 5.0]);
        // Knocked onto the next cell of its path: no (0, 0) step, the facing stays.
        let (mut unit, now) = walker(100);
        unit.place_at([13, 10]);
        unit.update(now);
        assert_eq!(unit.direction, east);
    }

    #[test]
    fn turns_images_for_flat_previews() {
        let (red, blue) = (image::Rgba([255, 0, 0, 255]), image::Rgba([0, 0, 255, 255]));
        let strip = RgbaImage::from_fn(2, 1, |x, _| if x == 0 { red } else { blue });
        // A quarter turn clockwise stands the strip up, its left end on top; mirrored first, its right end.
        let turned = turn_image(&strip, 90, false);
        assert_eq!((turned.dimensions(), *turned.get_pixel(0, 0), *turned.get_pixel(0, 1)), ((1, 2), red, blue));
        assert_eq!(*turn_image(&strip, 90, true).get_pixel(0, 0), blue);
        // Upright stays as it was; a slight turn grows the canvas and leaves its corners clear.
        assert_eq!(turn_image(&strip, 0, false), strip);
        let square = RgbaImage::from_pixel(10, 10, red);
        let tilted = turn_image(&square, 30, false);
        assert_eq!(tilted.dimensions(), (14, 14));
        assert_eq!(tilted.get_pixel(0, 0).0[3], 0);
        assert_eq!(*tilted.get_pixel(7, 7), red);
    }

    #[test]
    fn moves_hit_numbers_as_the_game_does() {
        let close = |a: f32, b: f32| (a - b).abs() < 0.01;
        // Pops up 2.7 units over the feet, tops out 32.7 up at tick 30 (720 ms), back at 2.7 by tick 60, ends 20.7 below.
        let [start, top, back, end] = [0.0, 30.0, 60.0, 70.0].map(|ticks| hit_number(ticks, false));
        assert!(close(start.height, 2.67) && close(top.height, 32.67) && close(back.height, 2.67) && close(end.height, -20.67));
        // Drifts 2.4 units at once, 21 by the end, on each axis.
        assert!(close(start.drift, 2.4) && close(end.drift, 21.07));
        // Five times its size, shrinking to 1.2 (a critical to 1.3) by about 380 ms, and fading out.
        assert_eq!((start.scale, top.scale, hit_number(30.0, true).scale), (5.0, 1.2, 1.3));
        assert!(close(start.alpha, 250.0 / 255.0) && close(end.alpha, 12.0 / 255.0));
        assert_eq!(hit_number(80.0, false).alpha, 0.0);
    }

    #[test]
    fn moves_misses_heals_totals_and_bursts() {
        let close = |a: f32, b: f32| (a - b).abs() < 0.01;
        // MISS rises 0.54 a tick from 12 units at its own size, gone after 80 ticks.
        let miss = miss_number(40.0);
        assert!(close(miss.height, 33.6) && miss.scale == 1.0 && close(miss.alpha, 130.0 / 255.0));
        assert_eq!(miss_number(MISS_TICKS + 4.0).alpha, 0.0);
        // A heal rises slowly from 12 units, 5 times its size down to 1.
        let [heal_start, heal_late] = [0.0, 100.0].map(heal_number);
        assert!(close(heal_start.height, 12.0) && heal_start.scale == 5.0 && heal_late.scale == 1.0 && close(heal_late.height, 30.0));
        // A total pops from half its size to 3 times within a few ticks, holds, then fades after tick 90.
        assert_eq!([0.0, 2.0, 5.0, 60.0].map(|ticks| total_number(ticks).scale), [0.5, 1.04, 3.0, 3.0]);
        assert!(close(total_number(60.0).alpha, 250.0 / 255.0) && close(total_number(100.0).alpha, 170.0 / 255.0));
        assert!(close(total_number(0.0).height, 27.0) && total_number(0.0).drift == 0.0);
        // A critical's burst rides its number, shrinking from 3 times to 1.
        let burst_now = burst(10.0);
        assert_eq!((burst_now.height, burst_now.drift), (hit_number(10.0, true).height, hit_number(10.0, true).drift));
        assert!(close(burst_now.scale, 1.56) && burst(20.0).scale == 1.0);
    }

    #[test]
    fn drifts_hit_numbers_away_from_where_the_target_faces() {
        // Turned by walking or toward a target: diagonally away from the facing.
        let walked = |direction: u8| number_drift(angle_of(FACING[usize::from(direction)].map(|step| step as i32)), false);
        assert_eq!(
            (0..8).map(walked).collect::<Vec<_>>(),
            [[-1.0, 1.0], [1.0, 1.0], [1.0, 1.0], [1.0, 1.0], [1.0, -1.0], [-1.0, -1.0], [-1.0, -1.0], [-1.0, 1.0]]
        );
        // Facing as the server set it: north and north-west drift south-east, the rest north-east.
        let set = |direction: u8| number_drift(rot_of(direction), true);
        assert_eq!((0..8).map(set).collect::<Vec<_>>(), [[1.0, 1.0], [1.0, 1.0], [1.0, 1.0], [1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [1.0, 1.0], [1.0, 1.0]]);
    }

    #[test]
    fn spaces_digits_on_fixed_slots() {
        assert_eq!(digit_slot(0, 1), 0.0);
        assert_eq!([digit_slot(0, 2), digit_slot(1, 2)], [-DIGIT_STEP / 2.0, DIGIT_STEP / 2.0]);
        assert_eq!([digit_slot(0, 3), digit_slot(2, 3)], [-DIGIT_STEP, DIGIT_STEP]);
        assert!(is_monster_job(1002) && is_monster_job(20021) && !is_monster_job(4001) && is_npc_job(45) && !is_npc_job(1002));
    }

    #[test]
    fn hides_by_option_bits() {
        assert_eq!(seen(0, false, false), Seen::Whole);
        for bit in [OPTION_HIDE, OPTION_CLOAK, OPTION_INVISIBLE, OPTION_CHASEWALK] {
            assert_eq!(seen(bit, false, false), Seen::Nothing, "others with {bit:#x}");
        }
        assert_eq!(seen(OPTION_HIDE, true, false), Seen::Shadow);
        assert_eq!(
            (seen(OPTION_CLOAK, true, false), seen(OPTION_INVISIBLE, true, false), seen(OPTION_HIDE | OPTION_CLOAK, true, false)),
            (Seen::Nothing, Seen::Nothing, Seen::Nothing)
        );
        assert_eq!(seen(0x8, true, false), Seen::Whole, "other option bits (a cart) don't hide");
    }

    #[test]
    fn silhouettes_hidden_units_while_clairvoyance_is_on() {
        // Status 184 draws others in Hiding, Cloaking or Chase Walk as black figures. GM hide stays invisible.
        for bit in [OPTION_HIDE, OPTION_CLOAK, OPTION_CHASEWALK] {
            assert_eq!(seen(bit, false, false), Seen::Nothing, "without 184, {bit:#x}");
            assert_eq!(seen(bit, false, true), Seen::Silhouette, "with 184, {bit:#x}");
        }
        assert_eq!(seen(OPTION_INVISIBLE, false, true), Seen::Nothing);
        assert_eq!(seen(OPTION_HIDE | OPTION_INVISIBLE, false, true), Seen::Nothing);
        // Our own character is never a silhouette: Hiding stays a shadow, Cloaking and Chase Walk stay hidden.
        assert_eq!(seen(OPTION_HIDE, true, true), Seen::Shadow);
        assert_eq!(seen(OPTION_CLOAK, true, true), Seen::Nothing);
        assert_eq!(seen(OPTION_CHASEWALK, true, true), Seen::Nothing);
        assert_eq!(seen(OPTION_INVISIBLE, true, true), Seen::Nothing);
        assert_eq!(seen(0, false, true), Seen::Whole);
        assert_eq!(seen(0x8, false, true), Seen::Whole);
        // The figure is the sprite's own colour multiplied by black, which keeps the alpha.
        assert_eq!(layer_color([10, 20, 30, 128], Some([0, 0, 0])), [0, 0, 0, 128]);
    }

    #[test]
    fn names_mercenary_glows_and_the_orcish_head() {
        assert_eq!(mercenary_glow(6017), None);
        assert_eq!(mercenary_glow(6026), None);
        assert_eq!(mercenary_glow(6027), Some(r"인간족\용병\창용병_창_검광"));
        assert_eq!(mercenary_glow(6036), Some(r"인간족\용병\창용병_창_검광"));
        assert_eq!(mercenary_glow(6037), Some(r"인간족\용병\검용병_검_검광"));
        assert_eq!(mercenary_glow(6046), Some(r"인간족\용병\검용병_검_검광"));
        assert_eq!(mercenary_glow(6047), None);
        assert_eq!(mercenary_glow(0), None);
        assert_eq!(orcish_head(0, 0), None);
        assert_eq!(orcish_head(4008, OPTION_ORCISH), Some(r"이팩트\orcface"));
        assert_eq!(orcish_head(4218, 0), None);
        assert_eq!(orcish_head(4218, OPTION_ORCISH), Some(r"이팩트\orcface_doram"));
        assert_eq!(orcish_head(4308, OPTION_ORCISH | RIDING), Some(r"이팩트\orcface_doram"));
        assert_ne!(BODY_OPTIONS & OPTION_ORCISH, 0);
    }

    #[test]
    fn leaves_a_footprint_on_each_cell_a_chase_walk_steps() {
        assert_eq!([0, 1, 2].map(chase_footprint), [EF_FOOT, EF_FOOT2, EF_FOOT]);
        let mut actor = Actor::new("a".into(), None, [10, 10], 0, 100);
        let start = ms(0);
        actor.walk_from([10, 10], [8, 8], vec![[11, 10], [12, 10]], start, start, false);
        let at = |ms: u64| start + Duration::from_millis(ms);
        actor.update(at(50));
        assert!(actor.steps_left().is_empty(), "still on the first cell");
        actor.update(at(150));
        assert_eq!(actor.steps_left(), &[[10, 10]]);
        actor.update(at(250));
        assert_eq!(actor.steps_left(), &[[11, 10]]);
    }

    #[test]
    fn looks_for_sprites_by_kind() {
        assert_eq!(SpriteSet::folders(5), ["몬스터", "npc"]);
        assert_eq!(SpriteSet::folders(8), ["homun", "몬스터", "npc"]);
        assert_eq!(SpriteSet::folders(6), ["npc", "몬스터"]);
    }

    #[test]
    fn plays_frames_at_the_games_pace() {
        // 24 ms units: a delay of 4 shows a frame for 96 ms; looping wraps, once holds the last.
        assert_eq!([0.0, 95.0, 96.0, 400.0].map(|ms| loop_frame(ms, 4.0, 4)), [0, 0, 1, 0]);
        assert_eq!([95.0, 96.0, 10_000.0].map(|ms| once_frame(ms, 4.0, 4)), [0, 1, 3]);
        // Walking: frames by distance (a cell is 5 units), the same whatever the speed.
        assert_eq!([0.0, 2.7, 2.71, 5.0, 10.8].map(|distance| walk_frame(distance, 1.0, 8)), [0, 3, 4, 7, 7]);
        assert_eq!(walk_frame(30.0, 4.0, 8), 3);
        // The attack's pace: attackMT over 432, capped there unless a bow; 0 is the ACT's own.
        assert_eq!(
            [attack_scale(200, false), attack_scale(432, false), attack_scale(800, false), attack_scale(800, true), attack_scale(0, true)],
            [200.0 / 432.0, 1.0, 1.0, 800.0 / 432.0, 1.0]
        );
        // A bow's swing lands 192 ms after its hit frame.
        assert_eq!([bow_flight(300.0, true), bow_flight(300.0, false)], [492.0, 300.0]);
    }

    #[test]
    fn faces_exact_angles_and_shows_them_to_the_camera() {
        assert_eq!([[0, -1], [1, 0], [0, 1], [-1, 0]].map(angle_of), [0.0, 90.0, 180.0, 270.0]);
        assert!((angle_of([2, -1]) - 63.435).abs() < 0.01);
        // The eight directions' angles agree with their steps.
        for step in [[0, -1], [-1, -1], [-1, 0], [-1, 1], [0, 1], [1, 1], [1, 0], [1, -1]] {
            assert!((rot_of(world::direction(step)) - angle_of(step)).abs() < 1e-3, "{step:?}");
        }
        // (rot, camera yaw) -> (rounded, floored): facing south, the camera turned 10 and 30 degrees either way. A
        // floored unit switches as soon as the camera turns one way, a rounded one only past 22.5 degrees.
        let table = [
            ((0.0, 0.0), (0, 0)),
            ((0.0, 10.0), (0, 7)),
            ((0.0, -10.0), (0, 0)),
            ((0.0, 30.0), (7, 7)),
            ((0.0, -30.0), (1, 0)),
            ((0.0, -22.5), (1, 0)),
            ((0.0, 22.5), (0, 7)),
            ((180.0, 0.0), (4, 4)),
            ((90.0, 45.0), (5, 5)),
            ((angle_of([2, -1]), 0.0), (7, 6)),
        ];
        for ((rot, yaw), (round, floor)) in table {
            assert_eq!((view_direction(rot, yaw, true), view_direction(rot, yaw, false)), (round, floor), "rot {rot} yaw {yaw}");
        }
    }

    /// Needs the official data.grf, at `RUSTNAROK_CLIENT_GRF` or `./data.grf`.
    #[test]
    fn standing_players_hold_their_head_still() {
        let Some(grf) = Grf::for_tests() else { return };
        let tables = rustnarok_client::lua::look_tables(&grf).unwrap();
        let novice = SpriteSet::player(&grf, &Look { hair: 1, ..Default::default() }, &tables).unwrap();
        for action in [IDLE, SIT] {
            let at = |ms: f32, head_direction: u16| novice.place(action * 8, head_direction, Timing::Loop(ms));
            assert!([100.0, 250.0, 1000.0].iter().all(|&ms| at(ms, 0) == at(0.0, 0)), "action {action} moves while standing still");
            assert_ne!(at(0.0, 1), at(0.0, 0), "action {action} ignores the head direction");
        }
    }

    /// Needs the official data.grf, at `RUSTNAROK_CLIENT_GRF` or `./data.grf`.
    #[test]
    fn dresses_a_player() {
        let Some(grf) = Grf::for_tests() else { return };
        let tables = rustnarok_client::lua::look_tables(&grf).unwrap();
        let look = Look { hair: 1, weapon: 1, shield: 1, head_top: 16, head_mid: 12, head_bottom: 52, robe: 3, ..Default::default() };
        let novice = SpriteSet::player(&grf, &look, &tables).unwrap();
        let parts: Vec<Part> = novice.layers.iter().map(|layer| layer.part).collect();
        assert_eq!(parts, [Part::Body, Part::Head, Part::HeadBottom, Part::HeadMid, Part::HeadTop, Part::Weapon, Part::WeaponGlow, Part::Shield, Part::Robe]);
        // Facing the camera the garment goes first, behind the body.
        let layers = |index: usize| -> Vec<usize> { novice.place(index, 0, Timing::Loop(0.0)).iter().map(|((layer, ..), ..)| *layer).collect() };
        assert_eq!(layers(0), [8, 0, 1, 3, 4, 2]);
        // Facing away it goes over the body, under the head and the hats. The dagger and guard only show in the battle
        // stance (and the dagger in its attack motion), under the hats.
        assert_eq!(layers(4), [0, 8, 1, 3, 4, 2]);
        let ready = layers(READY * 8);
        assert_eq!(ready[ready.len() - 3..], [3, 4, 2], "{ready:?}");
        assert!(ready.contains(&5) && ready.contains(&7), "{ready:?}");
        assert!(layers(attack_action(0, 0, 1) * 8).contains(&5));
        // A garment drawn once for every job (its picture shared, its motion per job) shows.
        let ribbon = SpriteSet::player(&grf, &Look { hair: 1, robe: 42, ..Default::default() }, &tables).unwrap();
        assert!(ribbon.layers.iter().any(|layer| layer.part == Part::Robe));
        // An animated wing flaps while its wearer stands still.
        let winged = SpriteSet::player(&grf, &Look { hair: 1, robe: 61, ..Default::default() }, &tables).unwrap();
        assert_ne!(winged.place(0, 0, Timing::Loop(0.0)), winged.place(0, 0, Timing::Loop(100.0)));
    }

    #[test]
    fn picks_each_layers_frame() {
        // Standing with the head turned (base and group 1): a 3-frame hat shows frame 1 whatever its clock says; a
        // 24-frame balloon plays frames 8 to 15, and a 24-frame wing (group 0) frames 0 to 7.
        assert_eq!(layer_frame(3, 3, 1, 1, |_| 2), Some(1));
        assert_eq!(layer_frame(24, 3, 1, 1, |n| n - 1), Some(15));
        assert_eq!(layer_frame(24, 3, 0, 1, |_| 3), Some(3));
        // Walking: in step with the body; a layer short of the body's frame doesn't show.
        assert_eq!(layer_frame(8, 8, 5, 5, |_| 0), Some(5));
        assert_eq!(layer_frame(4, 8, 5, 5, |_| 0), None);
        assert_eq!(layer_frame(16, 8, 5, 5, |n| n - 1), Some(11));
    }

    #[test]
    fn names_each_jobs_sprites() {
        let table = parse_job_sprites("# job\tbody\tweapon\tpalette\n4014\t로드페코\t페코페코_기사\t로드페코\nbad line\n");
        assert_eq!(table.len(), 1);
        assert_eq!(table[&4014], JobSprites { body: "로드페코", weapon: "페코페코_기사", palette: "로드페코" });
        // The table in the repository: a transcendent knight's weapons are a knight's, a Doram's dyes their own.
        assert_eq!(job_sprites(4008).map(|names| (names.body, names.weapon)), Some(("로드나이트", "기사")));
        assert_eq!(job_sprites(4218).map(|names| names.palette), Some("묘족"));
        assert!(job_sprites(4023).is_some() && job_sprites(4080).is_some() && job_sprites(31).is_none());
    }

    #[test]
    fn mounts_and_outfits_change_the_body() {
        assert_eq!(sprite_job(7, 0), 7);
        assert_eq!(sprite_job(7, RIDING), 13);
        assert_eq!(sprite_job(4008, RIDING), 4014);
        assert_eq!(sprite_job(4066, RIDING), 4082);
        // Riding means nothing to a job without a mount.
        assert_eq!(sprite_job(4054, RIDING), 4054);
        assert_eq!(sprite_job(4054, DRAGONS[0]), 4080);
        assert_eq!(sprite_job(4060, DRAGONS[3]), 4093);
        assert_eq!(sprite_job(4056, WOLF_RIDING), 4084);
        assert_eq!(sprite_job(4058, MADOGEAR), 4086);
        assert_eq!(sprite_job(4047, UNION), 4048);
        // Outfits come first: a wedding over everything, holidays unless in the union.
        assert_eq!(sprite_job(7, WEDDING | RIDING), 22);
        assert_eq!(sprite_job(7, XMAS | RIDING), 26);
        assert_eq!(sprite_job(4047, XMAS | UNION), 4048);
        assert!(outfit(sprite_job(1, HANBOK)) && !outfit(sprite_job(1, 0)));
        // Hiding, carts and the like don't change the body.
        assert_eq!(BODY_OPTIONS & (0x2 | 0x8 | 0x40), 0);
    }

    #[test]
    fn babies_are_smaller() {
        assert_eq!(body_scale(4008), 1.0);
        assert_eq!(body_scale(4023), 0.75);
        assert_eq!(body_scale(4030), 0.82);
        assert_eq!(body_scale(4036), 0.82);
        assert_eq!(body_scale(4220), 0.8);
        assert_eq!(body_scale(4218), 1.0);
    }

    #[test]
    fn leaves_off_garments_and_shields_where_the_game_does() {
        let dressing = |madogear: bool, female: bool| Dressing {
            head_first: Vec::new(),
            robe_over_body: Vec::new(),
            robe_over_headgear: false,
            headgear: Vec::new(),
            head_hidden: false,
            outfit: false,
            madogear,
            female,
        };
        let has = |dressing: &Dressing, index: usize, part: Part| dressing.order(index, 0).contains(&part);
        let knight = dressing(false, false);
        // Standing with a garment; lying dead without it.
        assert!(has(&knight, 0, Part::Robe) && !has(&knight, 64, Part::Robe) && !has(&knight, 71, Part::Robe));
        // A shield in an attack, but never on a Madogear; a woman on one has no garment, a man does.
        assert!(has(&knight, 80, Part::Shield) && !has(&dressing(true, false), 80, Part::Shield));
        assert!(!has(&dressing(true, true), 0, Part::Robe) && has(&dressing(true, false), 0, Part::Robe));
    }

    #[test]
    fn stacks_layers_back_to_front() {
        use Part::*;
        let hats = [(HeadMid, 100), (HeadTop, 200), (HeadBottom, 300)];
        // Facing the camera with a cape behind: the cape, body, head, then headgear over all of it.
        assert_eq!(stack(0, false, false, RobeAt::Behind, &hats), [Robe, Body, Head, HeadMid, HeadTop, HeadBottom]);
        // In the battle stance the weapon, its glow and a shield turned this way go between the head and the headgear.
        assert_eq!(stack(0, true, false, RobeAt::Behind, &hats), [Robe, Body, Head, Weapon, WeaponGlow, Shield, HeadMid, HeadTop, HeadBottom]);
        // Turned away: the shield first, the garment over the body under the head.
        assert_eq!(stack(4, true, false, RobeAt::OverBody, &hats), [Shield, Body, Robe, Head, Weapon, WeaponGlow, HeadMid, HeadTop, HeadBottom]);
        // The head behind the body, a bag over the headgear, and a headgear placed behind everything.
        assert_eq!(stack(3, false, true, RobeAt::OverHeadgear, &[(HeadTop, 200), (HeadBottom, -100)]), [HeadBottom, Head, Body, HeadTop, Robe]);
    }

    /// The shown clock at `ms` whole milliseconds.
    fn ms(ms: u64) -> Instant {
        *crate::START + Duration::from_millis(ms)
    }

    /// A unit on a 20 × 20 map at 150 ms a cell, made at `made` from a move entry: from `from` and its sub-cell `sub`
    /// along `path`, the walk starting at `t0`.
    fn walker(from: [i32; 2], sub: [u8; 2], path: &[[i32; 2]], t0: u64, made: u64) -> Actor {
        let mut unit = Actor::new("Poring".into(), None, from, 0, 150);
        unit.set_map([20, 20]);
        unit.walk_from(from, sub, path.to_vec(), ms(t0), ms(made), false);
        unit
    }

    fn assert_close(a: f32, b: f32) {
        assert!((a - b).abs() < 1e-3, "{a} != {b}");
    }

    /// How far a unit of `kind`, standing on (10, 10) and updated every 16 ms from 1000, moves on its first walking
    /// frame at `start`: east, node 1 at (11, 10) 222 ms after that frame. `busy` sets it up at 1000 (a swing, a sit…).
    /// A start of 1000 + 16k is 16 ms after the last frame.
    fn first_walking_frame(kind: u8, start: u64, busy: impl Fn(&mut Actor)) -> f32 {
        let mut unit = Actor::new("a".into(), None, [10, 10], 0, 150);
        unit.kind = kind;
        unit.set_map([20, 20]);
        unit.update(ms(1000));
        busy(&mut unit);
        let mut at = 1016;
        while at < start {
            unit.update(ms(at));
            at += 16;
        }
        unit.walk_from([10, 10], [8, 8], vec![[11, 10]], ms(start + 72), ms(start), true);
        unit.update(ms(start))[0] - 10.0
    }

    #[test]
    fn players_walk_out_of_a_motion_with_a_stale_frame_time() {
        let swing = |unit: &mut Actor| unit.play(5, ms(1000), 2000.0, false);
        // A player standing (16 ms since the last frame), and one walking out of a swing 200, 500 and 1000 ms old.
        assert_close(first_walking_frame(0, 1016, |_| {}), 16.0 / 238.0);
        assert_close(first_walking_frame(0, 1200, swing), 200.0 / 422.0);
        assert_close(first_walking_frame(0, 1500, swing), 500.0 / 722.0);
        assert_close(first_walking_frame(0, 2000, swing), 1000.0 / 1222.0);
        // Sitting for a second, standing and walking in the same frame.
        assert_close(first_walking_frame(0, 2000, |unit: &mut Actor| unit.sitting = true), 0.818);
        // Casting too.
        assert_close(first_walking_frame(0, 1200, |unit: &mut Actor| unit.casting = true), 200.0 / 422.0);
        // A monster in its swing keeps a fresh time: 16 ms.
        assert_close(first_walking_frame(5, 1208, |unit: &mut Actor| unit.play(2, ms(1000), 2000.0, false)), 16.0 / 238.0);
        // A player being hit (flinching) or picking up keeps a fresh time too.
        assert_close(first_walking_frame(0, 1208, |unit: &mut Actor| unit.play(6, ms(1000), 2000.0, false)), 16.0 / 238.0);
        assert_close(first_walking_frame(0, 1208, |unit: &mut Actor| unit.play(PICKUP, ms(1000), 2000.0, false)), 16.0 / 238.0);
    }

    /// A unit walking east from (10, 10) at 150 ms a cell from shown tick 0, updated every 16 ms through `until`; then
    /// the shown clock moves by `step` and the next `frames` frames' x come back.
    fn after_a_clock_change(kind: u8, until: u64, step: i64, frames: usize) -> (f32, Vec<f32>) {
        crate::SHOWN_OFFSET.set(0);
        let mut unit = Actor::new("a".into(), None, [10, 10], 0, 150);
        unit.kind = kind;
        unit.set_map([40, 40]);
        let east: Vec<[i32; 2]> = (11..20).map(|x| [x, 10]).collect();
        unit.walk_from([10, 10], [8, 8], east, ms(0), ms(0), false);
        let mut at = 0;
        let mut x = 0.0;
        while at <= until {
            x = unit.update(ms(at))[0];
            at += 16;
        }
        crate::SHOWN_OFFSET.set(step);
        let after = (0..frames).map(|frame| unit.update(ms(at + 16 * frame as u64))[0]).collect();
        crate::SHOWN_OFFSET.set(0);
        (x, after)
    }

    #[test]
    fn walks_follow_the_server_clock_as_it_changes() {
        let close = |got: &[f32], want: &[f32]| got.iter().zip(want).all(|(a, b)| (a - b).abs() < 2e-3) && got.len() == want.len();
        // Stepped back 144 ms after shown 992: held a frame, back toward (16, 10) it passed at 900, then on.
        let (x, after) = after_a_clock_change(5, 992, -144, 5);
        assert_close(x, 16.613);
        assert!(close(&after, &[16.613, 16.341, 16.068, 16.165, 16.262]), "{after:?}");
        // The same after shown 1072: held at 944, six frames back toward (17, 10), then on.
        let (x, after) = after_a_clock_change(5, 1072, -144, 8);
        assert_close(x, 17.147);
        assert!(close(&after, &[17.147, 17.125, 17.102, 17.080, 17.058, 17.036, 17.014, 17.112]), "{after:?}");
        // A packet 100 ms ahead after shown 992: every walk is 100 ms further along at once.
        let (_, after) = after_a_clock_change(5, 992, 100, 3);
        assert!(close(&after, &[17.387, 17.493, 17.600]), "{after:?}");
        // Our own walk the same.
        let (_, after) = after_a_clock_change(0, 992, -144, 5);
        assert!(close(&after, &[16.613, 16.341, 16.068, 16.165, 16.262]), "{after:?}");
    }

    #[test]
    fn replays_a_walk_over_by_the_shown_clock() {
        // The shown clock is 1000 behind the local one: a walk begun at shown 0, three cells east, heard at shown 500.
        crate::SHOWN_OFFSET.set(-1000);
        let mut unit = Actor::new("a".into(), None, [10, 10], 0, 150);
        unit.set_map([40, 40]);
        unit.walk_from([10, 10], [8, 8], vec![[11, 10], [12, 10], [13, 10]], ms(1000), ms(1500), false);
        assert_eq!(unit.path.iter().map(|&(_, at)| at).collect::<Vec<_>>(), [615, 730, 845]);
        crate::SHOWN_OFFSET.set(0);
    }

    #[test]
    fn replays_a_late_walk_at_ten_thirteenths_of_a_whole_speed() {
        let times = |speed: u16, path: &[[i32; 2]], begin: u64, now: u64| {
            let mut unit = Actor::new("a".into(), None, [10, 10], 0, speed);
            unit.set_map([40, 40]);
            unit.walk_from([10, 10], [8, 8], path.to_vec(), ms(begin), ms(now), false);
            unit.path.iter().map(|&(_, at)| at - begin.max(now)).collect::<Vec<_>>()
        };
        let path = [[11, 10], [12, 10], [13, 11], [14, 11], [15, 11], [16, 12], [17, 12]];
        // On time: from t0 at full speed.
        assert_eq!(times(150, &path, 5000, 5000), [150, 300, 512, 662, 812, 1024, 1174]);
        // Heard after its end: from now at 150·10/13 = 115 a cell, 162 a diagonal.
        assert_eq!(times(150, &path, 1000, 5000), [115, 230, 392, 507, 622, 784, 899]);
        let east: Vec<[i32; 2]> = (11..16).map(|x| [x, 10]).collect();
        assert_eq!(times(75, &east, 1000, 5000), [57, 114, 171, 228, 285]);
    }

    #[test]
    fn a_new_unit_with_no_path_stands_on_its_sub_cell() {
        let mut unit = Actor::new("a".into(), None, [10, 10], 0, 150);
        unit.set_map([40, 40]);
        unit.walk_from([10, 10], [2, 14], Vec::new(), ms(1000), ms(1000), false);
        assert_near(unit.position(ms(1000)), [9.625, 10.375]);
        assert_close(unit.rot, 0.0);
    }

    #[test]
    fn finds_the_cell_and_sub_cell_under_the_drawn_point() {
        let under = |size: usize, p: [f32; 2]| {
            let mut unit = Actor::new("a".into(), None, [10, 10], 0, 150);
            unit.set_map([size, size]);
            unit.at = world_point(p, unit.half);
            unit.cell_and_sub()
        };
        // Even: v = p + 0.5.
        assert_eq!(under(40, [10.3, 10.3]), ([10, 10], [12, 12]));
        assert_eq!(under(40, [9.97, 9.97]), ([10, 10], [7, 7]));
        assert_eq!(under(40, [10.0, 10.0]), ([10, 10], [8, 8]));
        // Odd: v = p, so a cell's centre is sub 0.
        assert_eq!(under(41, [10.0, 10.0]), ([10, 10], [0, 0]));
        // Our own walk whose source is its destination, drawn on that cell: there's no path, so nothing changes.
        let gat = rustnarok_client::map::Gat { width: 40, height: 40, cells: vec![([0.0; 4], 0); 1600] };
        assert_eq!(world::find_unit_path(&gat, [10, 10], [10, 10], 0), None);
    }

    #[test]
    fn closes_on_the_steps_end_each_frame() {
        // Straight, frames every 50 ms: today's linear motion, then standing on the last cell, two cells walked.
        let mut unit = walker([5, 5], [8, 8], &[[6, 5], [7, 5]], 1000, 1000);
        for (at, x) in [(1050, 5.3333), (1100, 5.6667), (1150, 6.0), (1200, 6.3333), (1250, 6.6667)] {
            assert_near(unit.update(ms(at)), [x, 5.0]);
        }
        assert_eq!((unit.update(ms(1300)), unit.moving()), ([7.0, 5.0], false));
        assert_close(unit.distance, 10.0);
        // A corner: the frame that crosses (6, 5) aims straight at (6, 6) from where the unit is.
        let mut unit = walker([5, 5], [8, 8], &[[6, 5], [6, 6]], 1000, 1000);
        let frames = [1000, 1100, 1200, 1300].map(|at| (unit.update(ms(at)), unit.rot, unit.distance));
        for ((at, rot, distance), (want, want_distance)) in
            frames.iter().zip([([5.0, 5.0], 0.0), ([5.6667, 5.0], 3.3333), ([5.8333, 5.5], 5.9686), ([6.0, 6.0], 8.6038)])
        {
            assert_near(*at, want);
            assert_close(*distance, want_distance);
            if want[1] > 5.0 {
                assert_close(*rot, 161.565);
            }
        }
        assert!(!unit.moving());
        // A unit new to the client moves from its first frame, before t0, toward node 1.
        let mut unit = walker([5, 5], [8, 8], &[[6, 5]], 1072, 1000);
        assert_close(unit.update(ms(1016))[0], 5.0721);
        assert_close(unit.update(ms(1072))[0], 5.3243);
        assert_eq!((unit.update(ms(1222)), unit.moving()), ([6.0, 5.0], false));
        // A late packet: no jump, it catches up within what's left of the step under way.
        let mut unit = walker([5, 5], [8, 8], &[[6, 5], [7, 5], [8, 5]], 1000, 1300);
        assert_close(unit.update(ms(1316))[0], 5.32);
        assert_close(unit.update(ms(1332))[0], 5.64);
        assert_eq!((unit.update(ms(1450)), unit.moving()), ([8.0, 5.0], false));
    }

    #[test]
    fn a_move_packet_only_replaces_the_path() {
        // Walking east from 1000; at 1100 the server turns it north from (5, 5) a third of a cell east, from 1060.
        let mut unit = walker([5, 5], [8, 8], &[[6, 5], [7, 5], [8, 5]], 1000, 1000);
        unit.update(ms(1050));
        let drawn = unit.position(ms(1100));
        unit.walk_from([5, 5], [14, 8], vec![[5, 6], [5, 7]], ms(1060), ms(1100), true);
        assert_eq!(unit.position(ms(1100)), drawn);
        assert_near(unit.update(ms(1100)), [5.2353, 5.2941]);
        assert_close(unit.rot, 198.435);
        for at in (1150..=1350).step_by(50) {
            unit.update(ms(at));
        }
        assert_eq!((unit.update(ms(1400)), unit.moving()), ([5.0, 7.0], false));
        assert_close(unit.distance, 12.8955);
        // A re-path heard after its first node's time (a held walk over a slow link): no frame jumps.
        let mut unit = walker([5, 5], [8, 8], &[[6, 5], [7, 5], [8, 5], [9, 5]], 1000, 1000);
        let mut drawn = unit.update(ms(1000));
        for at in (1016..=1700).step_by(16) {
            if at == 1400 {
                unit.walk_from([7, 5], [8, 8], vec![[8, 6], [9, 6]], ms(1150), ms(at), true);
            }
            let now = unit.update(ms(at));
            assert!(f32::hypot(now[0] - drawn[0], now[1] - drawn[1]) < 0.3, "{drawn:?} -> {now:?} at {at}");
            drawn = now;
        }
        assert_eq!(drawn, [9.0, 6.0]);
        // A frame late for the last node overshoots it, counted, then stands on it, still facing east.
        let mut unit = walker([5, 5], [8, 8], &[[6, 5]], 1000, 1000);
        unit.update(ms(1000));
        assert_close(unit.update(ms(1140))[0], 5.9333);
        assert_eq!((unit.update(ms(1160)), unit.moving()), ([6.0, 5.0], false));
        assert_close(unit.distance, 5.3333);
        assert_eq!(unit.rot, 90.000_02);
    }

    #[test]
    fn faces_each_frames_step_as_the_game_works_it_out() {
        let rot = |to: [i32; 2]| {
            let mut unit = walker([5, 5], [8, 8], &[to], 1000, 1000);
            unit.update(ms(1050));
            (unit.rot, unit.direction)
        };
        // North, east, west, south: 3.141592 makes the angles a little too big (180.0000458, 90.0000229, 269.99997).
        let steps = [[5, 6], [6, 5], [4, 5], [5, 4]].map(rot);
        assert_eq!(steps.map(|(rot, _)| rot), [180.000_05, 90.000_02, 269.999_97, 0.0]);
        assert_eq!(steps.map(|(_, direction)| direction), [4, 6, 2, 0]);
        // Floored, at the outdoor camera (0) and an indoor one (45).
        assert_eq!(steps.map(|(rot, _)| view_direction(rot, 0.0, false)), [3, 5, 2, 0]);
        assert_eq!(steps.map(|(rot, _)| view_direction(rot, 45.0, false)), [2, 4, 1, 7]);
        // A diagonal step with dx = dz.
        let (diagonal, _) = rot([6, 6]);
        assert_eq!(diagonal, 135.000_03);
        assert_eq!([0.0, 45.0].map(|yaw| view_direction(diagonal, yaw, false)), [4, 3]);
    }

    #[test]
    fn counts_the_clock_in_whole_ms() {
        assert_eq!(crate::whole_ms(*crate::START + Duration::from_micros(1_000_900)), 1000);
    }

    #[test]
    fn keeps_the_server_clock() {
        let start = Instant::now();
        let at = |ms: u64| start + Duration::from_millis(ms);
        let mut clock = ServerClock::default();
        clock.set(10_000, start);
        assert_eq!(clock.tick(at(500)), Some(10_500));
        // A packet ahead sets it at once.
        clock.heard(10_600, at(500));
        assert_eq!(clock.tick(at(500)), Some(10_600));
        // Three late ones don't move it; one in time starts the count again.
        for _ in 0..3 {
            clock.heard(10_400, at(500));
        }
        clock.heard(10_550, at(500));
        for _ in 0..3 {
            clock.heard(10_400, at(500));
        }
        assert_eq!(clock.tick(at(500)), Some(10_600));
        // The fourth late one in a row steps it back 144 ms.
        clock.heard(10_400, at(500));
        assert_eq!(clock.tick(at(500)), Some(10_456));
        assert_eq!(clock.local(10_456), Some(at(500)));
    }

    #[test]
    fn pulls_the_cart_on_a_one_cell_rope() {
        let sprites = SpriteSet {
            layers: Vec::new(),
            player: false,
            dressing: None,
            scale: 1.0,
            tint: None,
            textures: HashMap::new(),
            unmounted: None,
            delay: Default::default(),
        };
        // Placed a rope's length behind a leader facing south (0): to its north, facing south.
        let mut cart = Cart::behind(sprites, [50.0, 50.0], 0);
        assert_near(cart.at, [50.0, 55.0]);
        assert_eq!(cart.rot, 0.0);
        // Closer than the rope: it stays, turning to face the leader (east of it: 90).
        cart.step([53.0, 55.0], 6);
        assert_near(cart.at, [50.0, 55.0]);
        assert!((cart.rot - 90.0).abs() < 1e-3, "{}", cart.rot);
        assert_eq!(cart.rolled, 0.0);
        // Farther: pulled straight in to exactly a rope's length, rolling as far as it moved.
        cart.step([58.0, 55.0], 6);
        assert_near(cart.at, [53.0, 55.0]);
        assert!((cart.rolled - 3.0).abs() < 1e-4);
        // The wheels turn 0.92 frames a world unit, looping.
        assert_eq!(cart.frame(4), 2);
        cart.rolled = 5.0;
        assert_eq!(cart.frame(4), 0);
        // A jump (a warp) puts it behind the leader again.
        cart.step([200.0, 200.0], 4);
        assert_near(cart.at, [200.0, 195.0]);
    }

    #[test]
    fn names_each_cart_type() {
        let path = |kind: i32, job: u16| {
            let (sprite, act) = cart_path(kind, job);
            (sprite.trim_start_matches("data\\sprite\\이팩트\\").to_string(), act.trim_start_matches("data\\sprite\\이팩트\\").to_string())
        };
        let same = |name: &str| (name.to_string(), format!("{name}.act"));
        assert_eq!(path(1, 5), same("손수레"));
        for kind in 2..=13 {
            assert_eq!(path(kind, 10), same(&format!("손수레{}", kind - 1)));
        }
        // The Super Novice's own, whatever the type.
        assert_eq!(path(3, 23), same("슈노손수레"));
        assert_eq!(path(1, 4045), ("슈노손수레".to_string(), "베이비슈노손수레.act".to_string()));
        // Babies: a baby ACT on the grown-up sprite, or a baby sprite of its own from type 6.
        assert_eq!(path(1, 4028), ("손수레".to_string(), "베이비손수레.act".to_string()));
        assert_eq!(path(5, 4036), ("손수레4".to_string(), "베이비손수레4.act".to_string()));
        assert_eq!(path(6, 4107), same("손수레5베이비"));
    }

    #[test]
    fn sounds_every_frame_passed() {
        assert_eq!(frames_passed(Some((5, 2)), (5, 2), 8), Vec::<usize>::new());
        assert_eq!(frames_passed(Some((5, 2)), (5, 5), 8), [3, 4, 5]);
        assert_eq!(frames_passed(Some((1, 6)), (1, 1), 8), [7, 0, 1], "round a loop");
        assert_eq!(frames_passed(Some((0, 3)), (5, 2), 8), [1, 2], "a new action: not frame 0");
        assert_eq!(frames_passed(None, (5, 0), 8), Vec::<usize>::new());
    }

    #[test]
    fn flinches_by_who_and_for_how_long() {
        // A Poring's 480: held to 1.67 times the action; a player's 288 or less: frames shortened; 0: as it is.
        assert_eq!(flinch_pace(480.0 / 288.0, false, 3.0), Pace::Hold(480.0 / 288.0));
        assert_eq!(flinch_pace(0.5, true, 3.0), Pace::Act(0.5));
        assert_eq!(flinch_pace(0.5, false, 4.0), Pace::Act(0.125), "d x 24 ms a frame");
        assert_eq!(flinch_pace(0.0, false, 4.0), Pace::Act(1.0));
        // A player mid-swing doesn't flinch; a monster mid-attack does, but not in a special idle.
        assert!(!flinches(0, 5));
        assert!(flinches(0, WALK));
        assert!(flinches(5, 2));
        assert!(!flinches(5, 6));
    }

    #[test]
    fn walks_in_step_with_the_server() {
        let start = ms(0);
        // From a quarter cell east of the centre: the first cell after 0.75 cells, then a diagonal, then a straight.
        let path = [[11, 10], [12, 11], [13, 11]];
        let times: Vec<u64> = arrivals([10.25, 10.0], 0, &path, 150.0).into_iter().map(|(_, at)| at).collect();
        assert_eq!(times, [112, 324, 474]);
        let mut actor = Actor::new("a".into(), None, [10, 10], 0, 150);
        actor.walk_from([10, 10], [12, 8], path.to_vec(), start, start, false);
        assert_near(actor.update(start), [10.25, 10.0]);
        assert_near(actor.update(start + Duration::from_millis(56)), [10.625, 10.0]);
        // Heard of after it ended: replayed from now at 150·10/13 = 115 ms a cell, so the first cell after 86 ms.
        let now = ms(10_000);
        actor.walk_from([10, 10], [12, 8], path.to_vec(), start, now, false);
        assert_near(actor.update(now), [10.25, 10.0]);
        assert_near(actor.update(ms(10_043)), [10.625, 10.0]);
        assert!(actor.moving());
        // The server's ticks placed on our clock, across the counter wrapping too.
        assert_eq!(local_time((1000, start), 1500), start + Duration::from_millis(500));
        assert_eq!(local_time((1000, now), 900), now - Duration::from_millis(100));
        assert_eq!(local_time((u32::MAX - 10, start), 5), start + Duration::from_millis(16));
    }

    #[test]
    fn slides_back_over_a_fifth_of_a_second() {
        let start = ms(0);
        let at = |ms: u64| start + Duration::from_millis(ms);
        let mut actor = Actor::new("a".into(), None, [10, 10], 4, 150);
        actor.slide_to([10, 13], start);
        // Straight there in 200 ms, still facing the way it did, and not walking.
        assert_near(actor.update(at(0)), [10.0, 10.0]);
        assert_near(actor.update(at(100)), [10.0, 11.5]);
        assert_eq!((actor.direction, actor.sliding_to()), (4, Some([10, 13])));
        assert_eq!(actor.animation(at(100)).0, IDLE);
        assert_near(actor.update(at(200)), [10.0, 13.0]);
        assert_eq!((actor.cell, actor.sliding_to(), actor.moving()), ([10, 13], None, false));
        // Farther than five cells: a jump.
        actor.slide_to([10, 20], at(300));
        assert_eq!((actor.cell, actor.moving()), ([10, 20], false));
    }

    fn assert_near(a: [f32; 2], b: [f32; 2]) {
        assert!((a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3, "{a:?} != {b:?}");
    }

    #[test]
    fn casts_shadows_by_sprite_job() {
        let factors = HashMap::from([(14, 1.5), (4048, 1.0), (1002, 0.5), (45, 0.0)]);
        // By the sprite job: a Crusader (14) is listed here; riding, its sprite job is 21, which isn't.
        assert_eq!(shadow_factor(&factors, true, 14, 0, false, false), 1.5);
        assert_eq!(shadow_factor(&factors, true, 14, RIDING, false, false), 1.0);
        // Sitting or dead players: none, but for the floating union forms.
        assert_eq!(shadow_factor(&factors, true, 0, 0, true, false), 0.0);
        assert_eq!(shadow_factor(&factors, true, 0, 0, false, true), 0.0);
        assert_eq!(shadow_factor(&factors, true, 4047, UNION, true, false), 1.0);
        // Monsters and NPCs by their job, sitting or not; warps none.
        assert_eq!(shadow_factor(&factors, false, 1002, 0, false, false), 0.5);
        assert_eq!(shadow_factor(&factors, false, 45, 0, false, false), 0.0);
        assert_eq!(shadow_factor(&factors, false, 1031, 0, false, false), 1.0);
    }

    #[test]
    fn companion_bits_match_rathena() {
        // status.hpp: OPTION_FALCON 0x10, OPTION_WUG 0x100000, OPTION_WUGRIDER 0x200000.
        assert_eq!(OPTION_FALCON, 0x10);
        assert_eq!(OPTION_WUG, 0x10_0000);
        assert_eq!(OPTION_WUGRIDER, 0x20_0000);
        assert_eq!(BODY_OPTIONS & (OPTION_FALCON | OPTION_WUG), 0);
        assert_ne!(BODY_OPTIONS & OPTION_WUGRIDER, 0);
        assert!(falcon_out(OPTION_FALCON) && !falcon_out(0));
        assert!(warg_out(OPTION_WUG));
        assert!(!warg_out(OPTION_WUG | OPTION_WUGRIDER), "riding hides the warg");
        assert!(!warg_out(OPTION_WUGRIDER) && !warg_out(0));
        assert_eq!(FALCON_HEIGHT, 25.0);
        assert_eq!(WARG_SHADOW, 2.0);
    }

    #[test]
    fn falcon_arrives_then_hovers_at_its_hunter() {
        let owner = [0.0, 0.0];
        let at = |distance: f32| [distance, 0.0];
        // Arriving from 30: a 25th of the way a tick, still arriving; at 60 fps a frame is one tick.
        assert_eq!(falcon_step(at(30.0), owner, true, 1.0), (at(30.0 - 30.0 / 25.0), true));
        // Within 25 it has arrived; at 20 it flies 1 a tick, scaled by the time (2.5 ticks).
        let (step, arriving) = falcon_step(at(20.0), owner, true, 2.5);
        assert!(!arriving);
        assert_near(step, at(17.5));
        // At 8 it closes a 20th of the way a tick; over two ticks 1 − (19/20)².
        assert_near(falcon_step(at(8.0), owner, false, 2.0).0, at(8.0 * (19.0f32 / 20.0).powi(2)));
        // At 2 it stays; and 1 a tick never takes it past its hunter.
        assert_eq!(falcon_step(at(2.0), owner, false, 1.0), (at(2.0), false));
        assert_near(falcon_step(at(13.0), owner, false, 30.0).0, owner);
    }

    #[test]
    fn warg_waits_then_runs_after_its_ranger() {
        let owner = [0.0, 0.0];
        // At 20: runs 0.7 a tick.
        let (at, ran) = warg_step([20.0, 0.0], owner, 2.0);
        assert!(ran);
        assert_near(at, [18.6, 0.0]);
        // At 15.5, a tick takes it within 15.7: put at 14.9.
        let (at, ran) = warg_step([15.5, 0.0], owner, 1.0);
        assert!(ran);
        assert_near(at, [14.9, 0.0]);
        // At 10 it waits.
        assert_eq!(warg_step([10.0, 0.0], owner, 5.0), ([10.0, 0.0], false));
    }

    #[test]
    fn companions_face_their_owner() {
        // South of its owner it faces north (180); east of it, west (270); right on it, it keeps its way.
        assert!((facing([0.0, -10.0], [0.0, 0.0], 0.0) - 180.0).abs() < 1e-3);
        assert!((facing([10.0, 0.0], [0.0, 0.0], 0.0) - 270.0).abs() < 1e-3);
        assert_eq!(facing([1.0, 1.0], [1.0, 1.0], 45.0), 45.0);
        // The camera turned 10° doesn't change the way it shows; 30° does.
        assert_eq!(view_direction(180.0, 0.0, true), view_direction(180.0, 10.0, true));
        assert_ne!(view_direction(180.0, 0.0, true), view_direction(180.0, 30.0, true));
    }

    #[test]
    fn picks_the_companions_sprite_by_job() {
        let first = |warg, job| companion_sprites(warg, job).0[0].1.clone();
        assert_eq!(first(false, 11), "data\\sprite\\이팩트\\매.act");
        assert_eq!(first(false, 4034), "data\\sprite\\이팩트\\베이비매.act");
        assert_eq!(companion_sprites(false, 4034).0[0].0, "data\\sprite\\이팩트\\매", "the grown-up falcon's picture");
        assert_eq!(first(false, 4257), "data\\sprite\\이팩트\\windhawk_hawk.act");
        assert_eq!(first(true, 4068), "data\\sprite\\이팩트\\워그.act");
        assert_eq!(first(true, 4098), "data\\sprite\\몬스터\\베이비워그.act");
        assert_eq!(first(true, 4278), "data\\sprite\\이팩트\\windhawk_wolf.act");
        // A baby's falls back to the grown-up's; shadows: the warg's 2, a baby's 1, the falcon none.
        assert!(companion_sprites(true, 4098).0.iter().any(|(path, _)| path == "data\\sprite\\몬스터\\워그"));
        assert_eq!([companion_sprites(true, 4068).1, companion_sprites(true, 4111).1, companion_sprites(false, 11).1], [2.0, 1.0, 0.0]);
    }

    #[test]
    fn colours_a_unit_by_its_state() {
        assert_eq!(status_tint(OPT1_STONE, 0), Some([64, 64, 64]));
        assert_eq!(status_tint(OPT1_STONEWAIT, 0), Some([128, 128, 128]));
        assert_eq!(status_tint(OPT1_FREEZE, 0), Some([0, 128, 255]));
        assert_eq!(status_tint(0, OPT2_POISON), Some([255, 128, 255]));
        assert_eq!(status_tint(0, OPT2_DPOISON), Some([255, 50, 255]));
        assert_eq!(status_tint(0, OPT2_CURSE), Some([200, 50, 50]));
        // A body state wins when a health state is set too.
        assert_eq!(status_tint(OPT1_FREEZE, OPT2_POISON), Some([0, 128, 255]));
        assert_eq!(status_tint(OPT1_STONE, OPT2_CURSE | OPT2_POISON), Some([64, 64, 64]));
        assert_eq!(status_tint(OPT1_STONEWAIT, OPT2_DPOISON), Some([128, 128, 128]));
        // Two health bits at once: deadly poison, then curse over poison.
        assert_eq!(status_tint(0, OPT2_POISON | OPT2_DPOISON), Some([255, 50, 255]));
        assert_eq!(status_tint(0, OPT2_POISON | OPT2_CURSE), Some([200, 50, 50]));
        // Stone and freeze stop the animation; the others don't.
        assert!(stilled(OPT1_STONE) && stilled(OPT1_FREEZE) && !stilled(OPT1_STONEWAIT) && !stilled(0));
        // Stun, silence and a state that has ended leave the sprite alone.
        assert_eq!(status_tint(3, 0x0010), None);
        assert_eq!(status_tint(0, 0), None);
    }

    #[test]
    fn multiplies_every_layer_by_the_tint() {
        let tint = status_tint(OPT1_FREEZE, OPT2_POISON);
        let layers = [[255, 255, 255, 255], [128, 64, 32, 128], [0, 255, 0, 255]];
        assert_eq!(layers.map(|color| layer_color(color, tint)), [[0, 128, 255, 255], [0, 32, 32, 128], [0, 128, 0, 255]]);
        // Ending the state restores each layer's own colour.
        assert_eq!(layers.map(|color| layer_color(color, status_tint(0, 0))), layers);
    }

    /// A sprite of one blank 4×4 image (SPR 1.0).
    fn blank_spr() -> Spr {
        let mut bytes = b"SP\x00\x01".to_vec();
        bytes.extend([1u16, 4, 4].iter().flat_map(|value| value.to_le_bytes()));
        bytes.extend([0u8; 16]);
        Spr::parse(&bytes).unwrap()
    }

    /// A frame of `layers` layers (`empty` ones first) and an anchor.
    fn test_frame(layers: usize, empty: usize, anchor: Option<[i32; 3]>) -> sprite::Frame {
        let layer =
            |image| sprite::Layer { offset: [0, 0], image, mirror: false, color: [255; 4], scale: [1.0; 2], angle: 0, kind: 0, size: None, top_left: false };
        sprite::Frame { layers: (0..layers).map(|index| layer(if index < empty { -1 } else { 0 })).collect(), event: -1, anchors: anchor.into_iter().collect() }
    }

    /// An ACT whose 104 actions each have `frames` copies of `frame`.
    fn test_act(frame: sprite::Frame, frames: usize) -> Act {
        Act { actions: vec![vec![frame; frames]; 104], delays: vec![4.0; 104], events: Vec::new() }
    }

    /// A monster's body: 13 actions of 8 directions, `frames` frames each but 3 facing direction 2, at `delays`.
    fn test_monster(delays: Vec<f32>, frames: usize) -> SpriteSet {
        let mut act = test_act(test_frame(1, 0, None), frames);
        for action in 0..13 {
            act.actions[action * 8 + 2].truncate(3);
        }
        act.delays = delays;
        SpriteSet {
            layers: vec![Layer { spr: blank_spr(), act, palette: None, part: Part::Body }],
            player: false,
            dressing: None,
            scale: 1.0,
            tint: None,
            textures: HashMap::new(),
            unmounted: None,
            delay: Default::default(),
        }
    }

    #[test]
    fn plays_an_action_at_its_direction_0_delay_every_way() {
        // A walk whose ACT gives its directions [2, 1, 1, 1, 1, 1, 1, 1]: 2 every way.
        let mut delays = vec![4.0; 104];
        delays[8..16].copy_from_slice(&[2.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0]);
        let ghoul = test_monster(delays.clone(), 8);
        assert_eq!(ghoul.action_delay(WALK), 2.0);
        let frames: Vec<usize> = [0, 1, 4, 7].iter().map(|&direction| ghoul.body_frame(8 + direction, Timing::Loop(130.0)).unwrap().0).collect();
        assert_eq!(frames, [2, 2, 2, 2], "130 ms at 2 × 25 ms a frame");
        // The frame count is the drawn direction's: 3 facing direction 2.
        assert_eq!(ghoul.body_frame(8 + 2, Timing::Loop(130.0)).unwrap().0, 2);
        assert_eq!(ghoul.body_frame(8 + 2, Timing::Loop(180.0)).unwrap().0, 0);
        // No delay at direction 0: 4. Under 1: 1.
        assert_eq!(test_monster(vec![4.0; 8], 8).action_delay(WALK), 4.0);
        delays[8] = 0.5;
        assert_eq!(test_monster(delays.clone(), 8).action_delay(WALK), 1.0);
        // 0 keeps the previous action's delay.
        delays[0] = 3.0;
        delays[8] = 0.0;
        let kept = test_monster(delays, 8);
        assert_eq!((kept.action_delay(IDLE), kept.action_delay(WALK)), (3.0, 3.0));
    }

    #[test]
    fn head_turns_show_only_standing_or_sitting_and_are_not_kept() {
        let mut player = Actor::new("a".into(), None, [10, 10], 0, 150);
        player.set_map([40, 40]);
        player.set_head(1);
        // Standing: a turn's head shows.
        player.turn(2, 2, ms(1000));
        assert_eq!((player.direction, player.head_direction), (2, 2));
        // Walking: it's dropped; after the walk the head is straight.
        player.walk_from([10, 10], [8, 8], vec![[11, 10], [12, 10]], ms(1000), ms(1000), true);
        player.turn(4, 1, ms(1010));
        assert_eq!(player.head_direction, 0);
        player.update(ms(1400));
        assert!(!player.moving());
        assert_eq!(player.head_direction, 0);
        // Standing again after a swing: the head goes back to the one the last entry packet or own turn gave.
        player.set_head(1);
        player.turn(0, 2, ms(1500));
        assert_eq!(player.head_direction, 2);
        player.play(5, ms(1500), 300.0, false);
        assert_eq!(player.head_direction, 1);
        // Sitting: it shows.
        let mut sitting = Actor::new("b".into(), None, [10, 10], 0, 150);
        sitting.sitting = true;
        sitting.turn(0, 2, ms(1000));
        assert_eq!(sitting.head_direction, 2);
    }

    /// A player: a two-layer body (anchor at y −80), a head in layer 1, a headgear and a weapon with one frame an action.
    fn test_player(ignores_riding: bool, unmounted: Option<Act>) -> SpriteSet {
        let layer = |act, part| Layer { spr: blank_spr(), act, palette: None, part };
        let worn = Worn { part: Part::HeadTop, places: [Some(200); 8], offsets: [[0, 0]; 8], layer: 0, ignores_riding };
        let dressing = Dressing {
            head_first: Vec::new(),
            robe_over_body: Vec::new(),
            robe_over_headgear: false,
            headgear: vec![worn],
            head_hidden: false,
            outfit: false,
            madogear: false,
            female: false,
        };
        SpriteSet {
            layers: vec![
                layer(test_act(test_frame(2, 0, Some([0, -80, 0])), 2), Part::Body),
                layer(test_act(test_frame(2, 1, Some([0, -10, 0])), 2), Part::Head),
                layer(test_act(test_frame(1, 0, Some([0, -12, 0])), 2), Part::HeadTop),
                layer(test_act(test_frame(1, 0, None), 1), Part::Weapon),
            ],
            player: true,
            dressing: Some(dressing),
            scale: 1.0,
            tint: None,
            textures: HashMap::new(),
            unmounted,
            delay: Default::default(),
        }
    }

    #[test]
    fn knows_the_action_it_plays() {
        let start = Instant::now();
        let mut actor = Actor::new("Me".into(), None, [10, 10], 0, 150);
        actor.sprites = Some(test_player(false, None));
        // A swing (2 frames of 96 ms) plays till 192 ms; a walk ends it.
        actor.play_at_pace(5, start, 1.0, false);
        assert!(actor.playing(5, start + Duration::from_millis(100)) && !actor.playing(5, start + Duration::from_millis(200)));
        actor.walk_from([10, 10], [8, 8], vec![[11, 10]], start, start, true);
        assert!(!actor.playing(5, start));
    }

    #[test]
    fn draws_one_layer_a_part_and_nothing_for_a_missing_frame() {
        let sprites = test_player(false, None);
        let parts = |index, progress| sprites.place(index, 0, Timing::Span(progress)).iter().map(|((number, ..), ..)| *number).collect::<Vec<_>>();
        // In the battle stance: the body's layer 0 only, the head's layer 1, the headgear, the weapon.
        let mut drawn = parts(32, 0.0);
        drawn.sort_unstable();
        assert_eq!(drawn, [0, 1, 2, 3]);
        // The weapon has no second frame: it isn't drawn then.
        assert!(!parts(32, 0.9).contains(&3));
    }

    #[test]
    fn sits_riding_headgear_by_the_unmounted_body() {
        let unmounted = || test_act(test_frame(1, 0, Some([0, -60, 0])), 2);
        let top = |sprites: &SpriteSet, index| sprites.place(index, 0, Timing::Span(0.0)).iter().find(|((number, ..), ..)| *number == 2).unwrap().1[1];
        // On the head: (−80 − −10) + (−10 − −12) = −68, the 4×4 image's top 2 above that.
        let plain = test_player(false, Some(unmounted()));
        assert_eq!(top(&plain, 0), -70.0);
        // Flagged, standing: by the unmounted body's anchor, 20 lower; in an attack, by the mounted one.
        let riding = test_player(true, Some(unmounted()));
        assert_eq!((top(&riding, 0), top(&riding, 8), top(&riding, 64), top(&riding, 88)), (-50.0, -50.0, -50.0, -70.0));
    }

    #[test]
    fn leaves_fading_copies_while_a_status_blurs() {
        let start = Instant::now();
        let at = |ticks: f32| start + Duration::from_secs_f32(ticks * TICK);
        // 255 less 16 a tick: gone once under 16, after the 15th tick.
        assert_eq!(ghost_alpha(Duration::ZERO), 1.0);
        assert_eq!(ghost_alpha(Duration::from_secs_f32(1.5 * TICK)), 239.0 / 255.0);
        assert_eq!(ghost_alpha(Duration::from_secs_f32(14.5 * TICK)), 31.0 / 255.0);
        assert_eq!(ghost_alpha(Duration::from_secs_f32(15.5 * TICK)), 0.0);
        let mut actor = Actor::new("Knight".into(), None, [0, 0], 0, 150);
        let placed: Vec<Placed> = vec![((0, 0, 0, [255; 4], 0), [0.0; 2], [1.0; 2], 0.0)];
        // No blurring status: no copies.
        actor.leave_ghost(at(0.0), [0.0; 3], &placed);
        assert!(actor.ghosts.is_empty());
        // Two-Hand Quicken on: a copy now, none two ticks on, another after five.
        actor.blur(2, true);
        actor.leave_ghost(at(0.0), [0.0; 3], &placed);
        actor.leave_ghost(at(2.0), [1.0; 3], &placed);
        actor.leave_ghost(at(5.0), [2.0; 3], &placed);
        assert_eq!(actor.ghosts.iter().map(|ghost| ghost.feet[0]).collect::<Vec<_>>(), [0.0, 2.0]);
        // Off: no more copies, and the old ones fade away.
        actor.blur(2, false);
        actor.leave_ghost(at(10.0), [3.0; 3], &placed);
        assert_eq!(actor.ghosts.len(), 2);
        actor.leave_ghost(at(21.0), [3.0; 3], &placed);
        assert!(actor.ghosts.is_empty());
        // Cursed Circle darkens the body over any other state.
        actor.cursed = true;
        assert_eq!(actor.body_tint(), Some(CURSED_TINT));
    }
}
