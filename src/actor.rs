//! Characters: layered player sprites (body + head with dye palettes) and movement along a path of cells.

use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::Instant;

use image::RgbaImage;
use rustnarok_client::grf::Grf;
use rustnarok_client::lua::LookTables;
use rustnarok_client::sprite::{self, Act, Palette, Spr};

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

// ponytail: cart types 1 to 9 as 손수레, 손수레1 … 손수레8, as the files run; baby jobs' carts (베이비손수레…) not yet.
/// The sprite (without extension) of a pushcart type.
pub fn cart_path(kind: i32) -> String {
    let number = if kind > 1 { (kind - 1).to_string() } else { String::new() };
    format!("data\\sprite\\이팩트\\손수레{number}")
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
/// capped at ATTACK_PACE, except for players shooting a bow.
pub fn attack_scale(attack_ms: i32, bow: bool) -> f32 {
    let motion = attack_ms.max(1) as f32;
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
/// rathena `OPTION_FALCON` (`src/map/status.hpp`).
pub const OPTION_FALCON: u32 = 0x10;
/// rathena `OPTION_WUG`.
pub const OPTION_WUG: u32 = 0x10_0000;
/// rathena `OPTION_WUGRIDER`: the wolf the player is riding. Same bit as the mount body.
pub const OPTION_WUGRIDER: u32 = WOLF_RIDING;
/// All of them: the only option bits a look keeps.
/// rathena's OPTION_HIDE, OPTION_CLOAK and OPTION_CHASEWALK: the unit is out of sight.
pub const UNSEEN_OPTIONS: u32 = OPTION_HIDE | OPTION_CLOAK | OPTION_CHASEWALK;
/// rathena's option bits that keep a unit out of sight: Hiding, Cloaking, a GM's @hide, Chase Walk.
pub const OPTION_HIDE: u32 = 0x2;
pub const OPTION_CLOAK: u32 = 0x4;
pub const OPTION_INVISIBLE: u32 = 0x40;
pub const OPTION_CHASEWALK: u32 = 0x4000;
const HIDDEN_OPTIONS: u32 = UNSEEN_OPTIONS | OPTION_INVISIBLE;

/// How much of a unit shows.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Seen {
    Whole,
    /// Its shadow only: our own character in Hiding.
    Shadow,
    Nothing,
}

/// What shows of a unit with these option bits: others hidden in any way not at all; our own character in Hiding
/// as a shadow, cloaked, chase-walking or GM-hidden not at all.
// ponytail: our own Chase Walk is taken as Cloaking (nothing); the issue names Hide and Cloak.
pub fn seen(option: u32, own: bool) -> Seen {
    if option & HIDDEN_OPTIONS == 0 {
        Seen::Whole
    } else if own && option & (OPTION_CLOAK | OPTION_INVISIBLE | OPTION_CHASEWALK) == 0 {
        Seen::Shadow
    } else {
        Seen::Nothing
    }
}
pub const BODY_OPTIONS: u32 =
    WEDDING | UNION | XMAS | SUMMER | HANBOK | OKTOBERFEST | RIDING | DRAGONS[0] | DRAGONS[1] | DRAGONS[2] | DRAGONS[3] | DRAGONS[4] | WOLF_RIDING | MADOGEAR;

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

impl Part {
    /// The head and headgear sit on the body's anchor point.
    fn on_head(self) -> bool {
        matches!(self, Part::Head | Part::HeadBottom | Part::HeadMid | Part::HeadTop)
    }
}

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
    /// The option bits that change the body (`BODY_OPTIONS`): outfits and mounts.
    pub option: u32,
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
        // ponytail: stacked health bits take deadly poison, then poison, then curse.
        _ if health & OPT2_DPOISON != 0 => Some([255, 50, 255]),
        _ if health & OPT2_POISON != 0 => Some([255, 128, 255]),
        _ if health & OPT2_CURSE != 0 => Some([200, 50, 50]),
        _ => None,
    }
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
        let head = load(
            format!("data\\sprite\\{race}\\머리통\\{sex}\\{hair}_{sex}"),
            dye(format!("{palettes}\\머리{hair}_{sex}_{}.pal", look.hair_color), look.hair_color),
            Part::Head,
        );
        // A headgear filling several slots shows once. Doram wear their own cut of some.
        let headgear = |view: u16, part: Part| {
            let name = tables.headgears.get(&view)?;
            let doram = is_doram(look.job).then(|| load(format!("data\\sprite\\악세사리\\{sex}_doram\\{sex}{name}"), None, part)).flatten();
            doram.or_else(|| load(format!("data\\sprite\\악세사리\\{sex}\\{sex}{name}"), None, part))
        };
        let bottom = headgear(look.head_bottom, Part::HeadBottom);
        let mid = (look.head_mid != look.head_bottom).then(|| headgear(look.head_mid, Part::HeadMid)).flatten();
        let top = (look.head_top != look.head_bottom && look.head_top != look.head_mid).then(|| headgear(look.head_top, Part::HeadTop)).flatten();
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
                    Worn { part, places: place(view, slot, default), offsets }
                })
                .collect();
        let dressing = Dressing {
            head_first: grf.read(&format!("data\\imf\\{class}_{sex}.imf")).ok().and_then(|bytes| sprite::parse_imf(&bytes).ok()).unwrap_or_default(),
            robe_over_body,
            robe_over_headgear: robe.is_some() && rules.is_some_and(|rules| rules.robe_over_headgear(look.robe)),
            headgear,
            head_hidden: [look.head_top, look.head_mid, look.head_bottom].iter().any(|view| HEAD_HIDING.contains(view)),
            outfit: outfit(job),
        };
        let (weapon, glow) = weapon.unwrap_or_default();
        Some(Self {
            layers: [Some(body), head, bottom, mid, top, weapon, glow, shield.flatten(), robe].into_iter().flatten().collect(),
            player: true,
            dressing: Some(dressing),
            scale: body_scale(job),
            tint: None,
            textures: HashMap::new(),
        })
    }

    /// An NPC or monster: one sprite named in the client's `JobNameTable`.
    /// A mercenary (jobs 6017 to 6046): its body by the job name table's path under the human bodies (`여\활용병`), laid
    /// out like a player's, and the weapon of its kind (the unit carries none).
    pub fn mercenary(grf: &Grf, job: u16, name: &str) -> Option<Self> {
        // ponytail: the weapons' glow sprites (`_검광`) are left out.
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
        Some(Self {
            layers: [Some(body), weapon].into_iter().flatten().collect(),
            player: true,
            dressing: None,
            scale: 1.0,
            tint: None,
            textures: HashMap::new(),
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
        let animated = |layer: &Layer, count: usize| animated(layer, index, count, timing);
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
            None => vec![Part::Body, Part::Weapon],
        };

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
                // A frame the layer lacks falls back to its first.
                Part::Head | Part::Weapon | Part::WeaponGlow | Part::Shield => Some(if frame_index < frames.len() { frame_index } else { 0 }),
            };
            let Some(frame) = frame.and_then(|frame| frames.get(frame)) else { continue };
            let mut shift = if part.on_head() { sprite::head_shift(body_frame, frame) } else { [0, 0] };
            // Headgear moved for its wearer, by the way the head looks (turned one way or the other off the body's).
            if let Some(Worn { offsets, .. }) = self.dressing.as_ref().and_then(|dressing| dressing.headgear.iter().find(|worn| worn.part == part)) {
                let turn = match look {
                    Some(1) => 1,
                    Some(2) => 7,
                    _ => 0,
                };
                let [x, y] = offsets[(index + turn) % 8];
                shift = [shift[0] + x, shift[1] + y];
            }
            placed.extend(sprite::place(&layer.spr, frame, shift).into_iter().map(|placement| {
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
        let frame = animated(body, index, frames.len(), timing);
        let sound = usize::try_from(frames[frame].event).ok().and_then(|event| body.act.events.get(event)).map(String::as_str).filter(|&name| name != "atk");
        Some((frame, sound))
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

    pub fn index_ms(&self, index: usize) -> Option<f32> {
        let frames = self.layers[0].act.actions.get(index)?.len();
        let delay = self.layers[0].act.delays.get(index).copied().unwrap_or(4.0).max(1.0);
        Some(frames as f32 * delay * DELAY_UNIT_MS)
    }

    pub fn texture(&self, key: ImageKey) -> &Texture {
        &self.textures[&key]
    }
}

/// Which of `count` frames of the layer's action `index` plays at `timing`, at the layer's own frame delay (4 when
/// it has none, at least 1).
fn animated(layer: &Layer, index: usize, count: usize, timing: Timing) -> usize {
    let delay = layer.act.delays.get(index).copied().unwrap_or(4.0).max(1.0);
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
}

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

/// How a one-shot action plays: stretched over a duration, or at its ACT pace times a scale.
#[derive(Clone, Copy)]
enum Pace {
    Stretch(f32),
    Act(f32),
}

/// The client's damage numbers (data\sprite\이팩트\숫자.spr), one image per digit, and its MISS label (msg.spr).
pub struct Digits {
    digits: Vec<Texture>,
    pub miss: Texture,
    /// The burst behind a critical hit's number, and "Lucky" on its background (msg.spr frames 3, 5 and 4).
    pub critical: Texture,
    pub lucky: Texture,
    pub lucky_background: Texture,
}

impl Digits {
    /// Pixels between digits.
    const PADDING: f32 = 2.0;

    pub fn load(gpu: &Gpu, grf: &Grf) -> Option<Self> {
        let spr = Spr::parse(&grf.read("data\\sprite\\이팩트\\숫자.spr").ok()?).ok()?;
        let digits = (0..10).map(|digit| Some(gpu.texture(&spr.image(0, digit, None)?, false))).collect::<Option<_>>()?;
        let messages = Spr::parse(&grf.read("data\\sprite\\이팩트\\msg.spr").ok()?).ok()?;
        let frame = |index: usize| messages.image(0, index, None).or_else(|| messages.image(1, index, None)).map(|image| gpu.texture(&image, false));
        Some(Self { digits, miss: frame(0)?, critical: frame(3)?, lucky: frame(5)?, lucky_background: frame(4)? })
    }

    /// The digits of `number` centered on the anchor: texture, top-left offset and size in sprite pixels.
    pub fn place(&self, number: i32, scale: f32) -> Vec<(&Texture, [f32; 2], [f32; 2])> {
        let digits: Vec<&Texture> = number.unsigned_abs().to_string().bytes().map(|digit| &self.digits[usize::from(digit - b'0')]).collect();
        let width: f32 = digits.iter().map(|texture| texture.size[0] + Self::PADDING).sum::<f32>() - Self::PADDING;
        let mut x = -width / 2.0;
        digits
            .into_iter()
            .map(|texture| {
                let placed = (texture, [x * scale, -texture.size[1] / 2.0 * scale], [texture.size[0] * scale, texture.size[1] * scale]);
                x += texture.size[0] + Self::PADDING;
                placed
            })
            .collect()
    }
}

/// The falcon's sprite name (`몬스터\매`, then `npc\매`).
const FALCON_SPRITE: &str = "매";
/// The warg's sprite name (`몬스터\워그`, then `npc\워그`).
const WARG_SPRITE: &str = "워그";

/// How far the cart's rope lets it lag (#196): one cell. The cart is still pinned behind its owner until that lands.
pub const CART_ROPE: f32 = world::CELL;
/// The falcon flies this far above the ground.
pub const FALCON_HEIGHT: f32 = 25.0;
// ponytail: two cells, longer than the cart's one-cell rope. rope_step is that rope; the cart should call it with CART_ROPE once #196 lands.
pub const FALCON_REACH: f32 = CART_ROPE * 2.0;
/// The warg keeps about three cells out.
pub const WARG_REACH: f32 = world::CELL * 3.0;
/// The warg's shadow, twice a body's.
pub const WARG_SHADOW: f32 = 2.0;

/// Sprite jobs that keep their shadow while sitting: the Star Gladiator line's union forms, which float.
const SITTING_SHADOW_JOBS: [u16; 5] = [4048, 4238, 4243, 4244, 4316];

/// How wide a unit's shadow is, in cells (0 for none): its sprite job's factor from the data's tables, 1 when the job
/// isn't listed. A player (`player`) casts none while dead or sitting, but for the floating union forms.
pub fn shadow_factor(factors: &HashMap<u16, f32>, player: bool, job: u16, option: u32, sitting: bool, dead: bool) -> f32 {
    let sprite = if player { sprite_job(job, option) } else { job };
    if player && (dead || sitting && !SITTING_SHADOW_JOBS.contains(&sprite)) {
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

/// One step of the cart's rope (#196). Farther than `reach` from `leader`, move straight in until exactly `reach` away.
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

/// The 8-way facing of a move in world XZ, or none when it barely moved.
fn move_direction(from: [f32; 2], to: [f32; 2]) -> Option<u8> {
    let dx = to[0] - from[0];
    let dz = to[1] - from[1];
    if dx * dx + dz * dz < 0.05 * 0.05 {
        return None;
    }
    // atan2's 0 is east; facing 6 is east, and the index falls as the turn goes clockwise.
    let eighth = (dz.atan2(dx) / std::f32::consts::FRAC_PI_4).round() as i32;
    Some((6 - eighth).rem_euclid(8) as u8)
}

struct FollowStep {
    at: [f32; 2],
    direction: u8,
    moving: bool,
}

/// The falcon's rope, longer than the cart's. It faces the way it moved.
fn falcon_step(from: [f32; 2], facing: u8, leader: [f32; 2]) -> FollowStep {
    let at = rope_step(from, leader, FALCON_REACH);
    match move_direction(from, at) {
        Some(direction) => FollowStep { at, direction, moving: true },
        None => FollowStep { at, direction: facing, moving: false },
    }
}

/// The warg's rope, about three cells. It runs while the player is moving.
fn warg_step(from: [f32; 2], facing: u8, leader: [f32; 2], player_moving: bool) -> FollowStep {
    let at = rope_step(from, leader, WARG_REACH);
    FollowStep { at, direction: move_direction(from, at).unwrap_or(facing), moving: player_moving }
}

fn falcon_out(option: u32) -> bool {
    option & OPTION_FALCON != 0
}

/// The warg on the ground. Hidden while the player is riding it.
fn warg_out(option: u32) -> bool {
    option & OPTION_WUG != 0 && option & OPTION_WUGRIDER == 0
}

/// A falcon or a warg following a player: where it stands, the way it faces, and its sprite.
pub struct Companion {
    pub at: [f32; 2],
    pub direction: u8,
    pub moving: bool,
    sprites: Option<SpriteSet>,
    started: Instant,
}

impl Companion {
    fn spawn(grf: &Grf, name: &str, at: [f32; 2], direction: u8) -> Self {
        // Kind 5: monster folders, `몬스터` then `npc`.
        Self { at, direction, moving: false, sprites: SpriteSet::single(grf, name, 5), started: Instant::now() }
    }

    pub fn sprites(&self) -> Option<&SpriteSet> {
        self.sprites.as_ref()
    }

    /// The falcon keeps the walk cycle (its flap). The warg walks only while it is running.
    pub fn frame(&mut self, gpu: &Gpu, direction: u8, now: Instant, flying: bool) -> Vec<Placed> {
        let Some(sprites) = &mut self.sprites else { return Vec::new() };
        let action = if (flying || self.moving) && sprites.actions() > WALK { WALK } else { IDLE };
        let ms = now.saturating_duration_since(self.started).as_secs_f32() * 1000.0;
        sprites.frame(gpu, action, direction, 0, Timing::Loop(ms))
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

/// Something on the map that moves cell by cell.
pub struct Actor {
    pub name: String,
    /// Object type from the map server (player, NPC, monster, ...).
    pub kind: u8,
    /// Job (class, or NPC/monster sprite) ID.
    pub job: u16,
    /// A player's guild (0: none).
    pub guild: u32,
    /// A player's looks, to rebuild its sprites when they change.
    pub look: Option<Look>,
    /// Script NPCs the client never draws.
    pub hidden: bool,
    pub sprites: Option<SpriteSet>,
    pub cell: [i32; 2],
    path: Vec<[i32; 2]>,
    step_started: Instant,
    /// Milliseconds per straight step (diagonals take √2 as long).
    pub step_ms: f32,
    /// World units walked in finished steps since the unit appeared.
    distance: f32,
    /// The facing in the sprites' eight directions, and exactly, in degrees (`angle_of`: 0 south, 90 east).
    pub direction: u8,
    pub rot: f32,
    /// Players only: 0 looking straight ahead, 1 and 2 turned to the sides.
    pub head_direction: u16,
    animation_started: Instant,
    shot: Option<Shot>,
    /// A player who fought stands ready until moving again.
    ready: bool,
    /// Sitting down (until it stands or walks).
    pub sitting: bool,
    /// When a dying unit is removed.
    pub remove_at: Option<Instant>,
    /// A dead player: lying there until revived (players aren't removed when they die).
    pub dead: bool,
    /// Hiding, cloaking or chase-walking (the unit's option has UNSEEN_OPTIONS).
    pub cloaked: bool,
    /// The option bits that hide it (`seen`).
    pub hide_option: u32,
    /// rathena's opt1 and opt2: the body and health state that colour the sprite.
    pub body_state: u16,
    pub health_state: u16,
    /// The pushcart it pulls, drawn behind it.
    pub cart: Option<SpriteSet>,
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
}

impl Actor {
    pub fn new(name: String, sprites: Option<SpriteSet>, cell: [i32; 2], direction: u8, speed: u16) -> Self {
        let now = Instant::now();
        Self {
            name,
            kind: 0,
            job: 0,
            guild: 0,
            look: None,
            hidden: false,
            sprites,
            cell,
            path: Vec::new(),
            step_started: now,
            step_ms: f32::from(speed.max(1)),
            distance: 0.0,
            direction,
            rot: rot_of(direction),
            head_direction: 0,
            animation_started: now,
            shot: None,
            ready: false,
            sitting: false,
            remove_at: None,
            dead: false,
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
        }
    }

    /// The sound the animation plays now, if it just reached a frame with one.
    pub fn sound_now(&mut self, now: Instant) -> Option<String> {
        let (action, timing) = self.animation(now);
        let (frame, sound) = self.sprites.as_ref()?.body_frame(action * 8 + usize::from(self.direction % 8), timing)?;
        let heard = self.heard.replace((action, frame));
        sound.filter(|_| heard != Some((action, frame))).map(str::to_owned)
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
    }

    /// Plays `action` once at its ACT pace times `scale` (attack, flinch), or holds its last frame (death).
    pub fn play_at_pace(&mut self, action: usize, start: Instant, scale: f32, hold: bool) {
        self.shot = Some(Shot { action, started: start, pace: Pace::Act(scale.max(0.01)), hold });
    }

    /// Whether a one-shot action (an attack, a skill's pose, a flinch, dying) is playing now.
    pub fn in_action(&self, now: Instant) -> bool {
        !matches!(self.animation(now).0, IDLE | WALK | SIT | READY)
    }

    /// World units walked since the unit appeared, the step under way included.
    fn walked(&self, now: Instant) -> f32 {
        let Some(&next) = self.path.first() else { return self.distance };
        let step = [next[0] - self.cell[0], next[1] - self.cell[1]];
        let length = if step[0] != 0 && step[1] != 0 { std::f32::consts::SQRT_2 } else { 1.0 };
        let progress = now.saturating_duration_since(self.step_started).as_secs_f32() * 1000.0 / (self.step_ms * length);
        self.distance + progress.min(1.0) * length * world::CELL
    }

    /// Turns to face a cell: the nearest of the eight directions, and its exact angle.
    pub fn face(&mut self, toward: [i32; 2]) {
        if toward != self.cell {
            let step = [toward[0] - self.cell[0], toward[1] - self.cell[1]];
            self.direction = world::direction(step);
            self.rot = angle_of(step);
        }
    }

    /// Faces one of the eight directions (the server's, or our own turn).
    pub fn set_direction(&mut self, direction: u8) {
        self.direction = direction;
        self.rot = rot_of(direction);
    }

    pub fn walk(&mut self, path: Vec<[i32; 2]>) {
        self.ready = false;
        self.sitting = false;
        // Walking looks straight ahead again.
        self.head_direction = 0;
        let now = Instant::now();
        // Walking interrupts an attack or flinch, not a death.
        if self.shot.as_ref().is_some_and(|shot| !shot.hold) {
            self.shot = None;
        }
        if self.path.is_empty() {
            self.step_started = now;
            self.animation_started = now;
        }
        self.path = path;
    }

    pub fn stop_at(&mut self, cell: [i32; 2]) {
        self.cell = cell;
        if !self.path.is_empty() {
            self.animation_started = Instant::now();
        }
        self.path.clear();
    }

    pub fn moving(&self) -> bool {
        !self.path.is_empty()
    }

    /// Advances along the path; returns the (fractional) cell the actor is at now.
    pub fn update(&mut self, now: Instant) -> [f32; 2] {
        while let Some(&next) = self.path.first() {
            let step = [next[0] - self.cell[0], next[1] - self.cell[1]];
            let duration = self.step_ms * if step[0] != 0 && step[1] != 0 { std::f32::consts::SQRT_2 } else { 1.0 };
            let progress = now.saturating_duration_since(self.step_started).as_secs_f32() * 1000.0 / duration;
            self.direction = world::direction(step);
            self.rot = angle_of(step);
            if progress < 1.0 {
                return [self.cell[0] as f32 + step[0] as f32 * progress, self.cell[1] as f32 + step[1] as f32 * progress];
            }
            self.cell = next;
            self.distance += if step[0] != 0 && step[1] != 0 { std::f32::consts::SQRT_2 } else { 1.0 } * world::CELL;
            self.path.remove(0);
            self.step_started += std::time::Duration::from_secs_f32(duration / 1000.0);
            if self.path.is_empty() {
                self.animation_started = now;
            }
        }
        self.cell.map(|value| value as f32)
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
                Pace::Stretch(_) => {}
            }
        }
        let action = if self.moving() {
            WALK
        } else if self.sitting {
            SIT
        } else if self.ready {
            READY
        } else {
            IDLE
        };
        if action == WALK {
            return (action, Timing::Walk(self.walked(now)));
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
        if falcon_out(option) && self.falcon.is_none() {
            self.falcon = Some(Companion::spawn(grf, FALCON_SPRITE, place_behind(leader, self.direction, FALCON_REACH), self.direction));
        } else if !falcon_out(option) {
            self.falcon = None;
        }
        if warg_out(option) && self.warg.is_none() {
            self.warg = Some(Companion::spawn(grf, WARG_SPRITE, place_behind(leader, self.direction, WARG_REACH), self.direction));
        } else if !warg_out(option) {
            self.warg = None;
        }
    }

    /// One rope step. `moving` is the player walking this frame, which is when the warg runs.
    pub fn step_companions(&mut self, leader: [f32; 2], moving: bool) {
        if let Some(falcon) = &mut self.falcon {
            let step = falcon_step(falcon.at, falcon.direction, leader);
            (falcon.at, falcon.direction, falcon.moving) = (step.at, step.direction, step.moving);
        }
        if let Some(warg) = &mut self.warg {
            let step = warg_step(warg.at, warg.direction, leader, moving);
            (warg.at, warg.direction, warg.moving) = (step.at, step.direction, step.moving);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn hides_by_option_bits() {
        assert_eq!(seen(0, false), Seen::Whole);
        for bit in [OPTION_HIDE, OPTION_CLOAK, OPTION_INVISIBLE, OPTION_CHASEWALK] {
            assert_eq!(seen(bit, false), Seen::Nothing, "others with {bit:#x}");
        }
        assert_eq!(seen(OPTION_HIDE, true), Seen::Shadow);
        assert_eq!(
            (seen(OPTION_CLOAK, true), seen(OPTION_INVISIBLE, true), seen(OPTION_HIDE | OPTION_CLOAK, true)),
            (Seen::Nothing, Seen::Nothing, Seen::Nothing)
        );
        assert_eq!(seen(0x8, true), Seen::Whole, "other option bits (a cart) don't hide");
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
        // The attack's pace: attackMT over 432, capped there unless a bow.
        assert_eq!(
            [attack_scale(200, false), attack_scale(432, false), attack_scale(800, false), attack_scale(800, true)],
            [200.0 / 432.0, 1.0, 1.0, 800.0 / 432.0]
        );
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

    #[test]
    fn walks_cell_by_cell_at_its_speed() {
        let mut actor = Actor::new("a".into(), None, [10, 10], 0, 100);
        let start = Instant::now();
        actor.step_started = start;
        actor.walk(vec![[11, 10], [12, 11]]);
        actor.step_started = start;
        let at = |ms: u64| start + std::time::Duration::from_millis(ms);
        assert_eq!(actor.update(at(50)), [10.5, 10.0]);
        assert_eq!(actor.direction, 6);
        // The diagonal step takes ~141 ms after the first 100.
        let halfway = actor.update(at(170));
        assert!((halfway[0] - 11.5).abs() < 0.01 && (halfway[1] - 10.5).abs() < 0.01, "{halfway:?}");
        assert_eq!(actor.direction, 5);
        assert_eq!(actor.update(at(400)), [12.0, 11.0]);
        assert!(!actor.moving());
    }

    fn assert_near(a: [f32; 2], b: [f32; 2]) {
        assert!((a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3, "{a:?} != {b:?}");
    }

    fn apart(a: [f32; 2], b: [f32; 2]) -> f32 {
        let (dx, dz) = (a[0] - b[0], a[1] - b[1]);
        (dx * dx + dz * dz).sqrt()
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
        assert_eq!(FALCON_REACH, CART_ROPE * 2.0);
        assert_eq!(WARG_REACH, world::CELL * 3.0);
    }

    #[test]
    fn falcon_follows_on_a_longer_rope() {
        let leader = [100.0, 200.0];
        // Closer than the rope: it stays, and keeps its facing.
        let stay = falcon_step([100.0, 205.0], 4, leader);
        assert_near(stay.at, [100.0, 205.0]);
        assert_eq!((stay.direction, stay.moving), (4, false));
        // Farther: pulled in until exactly the reach, facing the way it moved (south).
        let pulled = falcon_step([100.0, 200.0 + FALCON_REACH + 12.0], 4, leader);
        assert_near(pulled.at, [100.0, 200.0 + FALCON_REACH]);
        assert_eq!((pulled.direction, pulled.moving), (0, true));
        assert!((apart(pulled.at, leader) - FALCON_REACH).abs() < 1e-3);
        // Spawned behind a south-facing player, the next step leaves it there.
        let behind = place_behind(leader, 0, FALCON_REACH);
        assert_near(behind, [100.0, 200.0 + FALCON_REACH]);
        let held = falcon_step(behind, 0, leader);
        assert_near(held.at, behind);
        assert!(!held.moving);
        // A diagonal pull lands on the rope and faces the way it moved.
        let diagonal = falcon_step([100.0 + 40.0, 200.0 + 40.0], 0, leader);
        assert!((apart(diagonal.at, leader) - FALCON_REACH).abs() < 1e-3);
        assert_eq!(diagonal.direction, 1);
    }

    #[test]
    fn warg_stays_three_cells_out_and_runs_with_the_player() {
        let leader = [0.0, 0.0];
        // Inside three cells it stays. It runs because the player is moving, and stands when the player does.
        let close = warg_step([10.0, 0.0], 6, leader, true);
        assert_near(close.at, [10.0, 0.0]);
        assert!(close.moving && close.direction == 6);
        let idle = warg_step([10.0, 0.0], 6, leader, false);
        assert_near(idle.at, [10.0, 0.0]);
        assert!(!idle.moving);
        // Past three cells the rope pulls it back to exactly that, facing the way it moved.
        let far = warg_step([WARG_REACH + 8.0, 0.0], 2, leader, true);
        assert_near(far.at, [WARG_REACH, 0.0]);
        assert_eq!((far.direction, far.moving), (2, true));
        assert!((apart(far.at, leader) - 3.0 * world::CELL).abs() < 1e-3);
        // Standing still still closes the rope, without the run.
        let stopped = warg_step([WARG_REACH + 8.0, 0.0], 2, leader, false);
        assert_near(stopped.at, [WARG_REACH, 0.0]);
        assert!(!stopped.moving);
        // Behind an east-facing player is three cells west.
        assert_near(place_behind(leader, 6, WARG_REACH), [-WARG_REACH, 0.0]);
    }

    #[test]
    fn faces_each_way_it_moves() {
        let face = |step: [f32; 2]| move_direction([0.0, 0.0], step).unwrap();
        assert_eq!(face([0.0, -10.0]), 0);
        assert_eq!(face([-10.0, -10.0]), 1);
        assert_eq!(face([-10.0, 0.0]), 2);
        assert_eq!(face([-10.0, 10.0]), 3);
        assert_eq!(face([0.0, 10.0]), 4);
        assert_eq!(face([10.0, 10.0]), 5);
        assert_eq!(face([10.0, 0.0]), 6);
        assert_eq!(face([10.0, -10.0]), 7);
        // A little north of east is still east.
        assert_eq!(face([10.0, 1.0]), 6);
        assert_eq!(move_direction([3.0, 3.0], [3.0, 3.0]), None);
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
        // Two health bits at once: deadly poison, then poison, then curse.
        assert_eq!(status_tint(0, OPT2_POISON | OPT2_DPOISON), Some([255, 50, 255]));
        assert_eq!(status_tint(0, OPT2_POISON | OPT2_CURSE), Some([255, 128, 255]));
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
}
