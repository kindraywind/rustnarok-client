//! Effects played on the map, for the effect IDs the server, skills and items name: the client's STR animations,
//! moving textures and sprites, cylinders, sprite animations, the models of skills' units on the ground (traps), and
//! the camera shakes the client does in code.

use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use rustnarok_client::effect::Str;
use rustnarok_client::grf::Grf;
use rustnarok_client::model::Rsm;

use crate::actor::{Placed, SpriteSet, Timing};
use crate::prims::{
    balls, blitz, bolts, boomerang, brawl, burst, circle, dancer, first, ground, guard, huujin, knight, monk, sage, shafts, shards, sight, sniper, songs,
    spray, star, stone, tanji, throw, transcend, walls, waterball,
};
use crate::render::{Gpu, Texture};
use crate::world::{self, CELL, EffectMesh, EffectQuad, MeshVertex, Vector};

/// A quad's corners as texture coordinates: top left, top right, bottom right, bottom left.
pub(crate) const QUAD_UV: [[f32; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];

/// An STR moves on one frame each 16 ms tick, whatever its file says.
const STR_TICK_MS: f32 = 16.0;
/// An STR's (320, 240) sits this many screen pixels above its owner's feet, at any zoom.
pub const STR_RISE: f32 = 80.0;

/// How many pixels above the feet an effect's STR is anchored: 80, but for Shield Charge's anchor, which sits on the feet.
pub fn str_rise(effect: u32) -> f32 {
    if effect == SHIELD_CHARGE { 0.0 } else { STR_RISE }
}

/// One STR part of an effect.
struct Part {
    file: String,
    /// Where its textures are, under data\texture\effect\.
    folder: String,
    attached: bool,
    /// `%d` in the file name is a random number from this range.
    random: Option<(u32, u32)>,
    /// Plays once on a skill's ground unit, where the rest starts again until the unit goes.
    once: bool,
}

/// A sprite's animation (SPR effects): a sprite under data\sprite\이팩트\, whether it follows the unit, and how it
/// plays as name=value parameters.
struct SpritePart {
    file: String,
    attached: bool,
    parameters: HashMap<String, String>,
}

/// Which effects things play (effects.txt).
pub struct Table {
    parts: HashMap<u32, Vec<Part>>,
    /// Textures or sprites moving over their duration, as name=value parameters (2D and 3D effects).
    floaters: HashMap<u32, Vec<HashMap<String, String>>>,
    /// Textured cylinders, the same way (CYLINDER effects).
    rings: HashMap<u32, Vec<HashMap<String, String>>>,
    sprites: HashMap<u32, Vec<SpritePart>>,
    /// Camera shakes, which the client does in code (FUNC effects): how long after the start, and for how long.
    quakes: HashMap<u32, Vec<(Duration, Duration)>>,
    /// Sounds and shakes of a skill's use packet (`skillwav`, `skillquake`).
    skill_sounds: HashMap<u16, Vec<Sound>>,
    skill_quakes: HashMap<u16, Vec<(Duration, Duration)>>,
    /// The same for a skill's damage packet (`hitwav`, `hitquake`).
    hit_sounds: HashMap<u16, Vec<Sound>>,
    hit_quakes: HashMap<u16, Vec<(Duration, Duration)>>,
    /// What skills' units on the ground show while they're there, by rathena's UNT_ number: an effect, a model.
    units: HashMap<u32, u32>,
    /// Units whose effect plays once and is kept rather than starting again.
    units_once: std::collections::HashSet<u32>,
    models: HashMap<u32, String>,
    /// Effects that end (or, on a ground unit, start again) by their own length in ticks rather than their STR's.
    lengths: HashMap<u32, u32>,
    misc: HashMap<u32, u32>,
    skills: HashMap<(u16, String), Vec<u32>>,
    items: HashMap<u32, u32>,
    sounds: HashMap<u32, Vec<Sound>>,
    /// Damaging skills: the two hit effects on the target, the caster's effect, and what else plays as the packet
    /// comes (on the caster or the target).
    hits: HashMap<u16, [Option<u32>; 2]>,
    casters: HashMap<u16, Option<u32>>,
    /// The aura of a cast with a cast time, by skill: 12 by element, −1 none, else that effect.
    casts: HashMap<u16, i32>,
    struck: HashMap<u16, Vec<(Who, u32)>>,
    /// Skills without damage, and ground skills (at the cell or on the caster).
    used: HashMap<u16, Vec<(Who, u32)>>,
    grounds: HashMap<u16, Vec<(Who, u32)>>,
    /// What a trap (by skill unit type) shows when it goes off.
    traps: HashMap<u32, u32>,
    /// What a status (by EFST id) plays again and again on its unit while it's on.
    statuses: HashMap<u16, Vec<u32>>,
}

/// How long a standard camera shake and a short one last.
pub const QUAKE_LONG: Duration = Duration::from_millis(650);
pub const QUAKE_SHORT: Duration = Duration::from_millis(200);

/// Where a skill's effect plays.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Who {
    Caster,
    Target,
    Ground,
}

/// The caster effect of the newest jobs' skills: none.
const NO_CASTER_EFFECT: [std::ops::RangeInclusive<u16>; 3] = [5401..=5507, 6001..=6006, 6525..=6606];
/// What most damaging skills show on their caster.
const CASTER_EFFECT: u32 = 16;

/// A sound an effect plays (a file under data\wav\ without .wav), `delay_ms` after it starts; `%d` in the
/// file picks from `random`.
struct Sound {
    file: String,
    delay_ms: u64,
    random: Option<(u32, u32)>,
    /// Made at the listener's own place, heard at full volume wherever the effect plays.
    listener: bool,
}

impl Table {
    pub fn load() -> Self {
        let mut table = Self {
            parts: HashMap::new(),
            floaters: HashMap::new(),
            rings: HashMap::new(),
            sprites: HashMap::new(),
            quakes: HashMap::new(),
            skill_sounds: HashMap::new(),
            skill_quakes: HashMap::new(),
            hit_sounds: HashMap::new(),
            hit_quakes: HashMap::new(),
            units: HashMap::new(),
            units_once: std::collections::HashSet::new(),
            models: HashMap::new(),
            lengths: HashMap::new(),
            misc: HashMap::new(),
            skills: HashMap::new(),
            items: HashMap::new(),
            sounds: HashMap::new(),
            hits: HashMap::new(),
            casters: HashMap::new(),
            casts: HashMap::new(),
            struck: HashMap::new(),
            used: HashMap::new(),
            grounds: HashMap::new(),
            traps: HashMap::new(),
            statuses: HashMap::new(),
        };
        let parameters = |text: &str| -> HashMap<String, String> {
            text.split(';').filter_map(|pair| pair.split_once('=')).map(|(name, value)| (name.to_owned(), value.to_owned())).collect()
        };
        for line in include_str!("effects.txt").lines().filter(|line| !line.starts_with('#')) {
            let fields: Vec<&str> = line.split('\t').collect();
            let number = |index: usize| fields.get(index).and_then(|field| field.parse::<u32>().ok());
            // -1 for none.
            let effect = |index: usize| fields.get(index).and_then(|field| field.parse::<i64>().ok()).and_then(|value| u32::try_from(value).ok());
            let who = |field: &str| match field {
                "caster" => Who::Caster,
                "ground" => Who::Ground,
                _ => Who::Target,
            };
            match fields[..] {
                ["cast", skill, effect] => {
                    if let (Ok(skill), Ok(effect)) = (skill.parse(), effect.parse()) {
                        table.casts.insert(skill, effect);
                    }
                }
                ["str", _, file, folder, attached, ..] => {
                    let Some(id) = number(1) else { continue };
                    let folder = if folder == "-" { String::new() } else { folder.to_owned() };
                    let random = number(5).zip(number(6));
                    let once = fields.last() == Some(&"once");
                    table.parts.entry(id).or_default().push(Part { file: file.to_owned(), folder, attached: attached == "1", random, once });
                }
                ["spr", _, file, attached, text] => {
                    let Some(id) = number(1) else { continue };
                    table.sprites.entry(id).or_default().push(SpritePart { file: file.to_owned(), attached: attached == "1", parameters: parameters(text) });
                }
                ["func", _, "quake", ..] => {
                    if let (Some(id), Some(delay), Some(length)) = (number(1), number(3), number(4)) {
                        let length = if length == 200 { QUAKE_SHORT } else { QUAKE_LONG };
                        table.quakes.entry(id).or_default().push((Duration::from_millis(delay.into()), length));
                    }
                }
                ["unit", ..] => {
                    if let (Some(kind), Some(id)) = (number(1), number(2)) {
                        table.units.insert(kind, id);
                        if fields.get(3) == Some(&"once") {
                            table.units_once.insert(kind);
                        }
                    }
                }
                ["length", ..] => {
                    if let (Some(id), Some(ticks)) = (number(1), number(2)) {
                        table.lengths.insert(id, ticks);
                    }
                }
                ["rsm", _, file] => {
                    if let Some(kind) = number(1) {
                        table.models.insert(kind, file.to_owned());
                    }
                }
                ["wav", _, file, ..] => {
                    let Some(id) = number(1) else { continue };
                    let listener = fields.last() == Some(&"listener");
                    let sound = Sound { file: file.to_owned(), delay_ms: number(3).map_or(0, u64::from), random: number(4).zip(number(5)), listener };
                    table.sounds.entry(id).or_default().push(sound);
                }
                ["skillwav", _, file, ..] => {
                    let Some(skill) = number(1) else { continue };
                    let listener = fields.last() == Some(&"listener");
                    let sound = Sound { file: file.to_owned(), delay_ms: number(3).map_or(0, u64::from), random: None, listener };
                    table.skill_sounds.entry(skill as u16).or_default().push(sound);
                }
                ["hitwav", _, file, ..] => {
                    let Some(skill) = number(1) else { continue };
                    let listener = fields.last() == Some(&"listener");
                    let sound = Sound { file: file.to_owned(), delay_ms: number(3).map_or(0, u64::from), random: None, listener };
                    table.hit_sounds.entry(skill as u16).or_default().push(sound);
                }
                ["hitquake", _, _, _] => {
                    if let (Some(skill), Some(delay), Some(length)) = (number(1), number(2), number(3)) {
                        let length = if length == 200 { QUAKE_SHORT } else { QUAKE_LONG };
                        table.hit_quakes.entry(skill as u16).or_default().push((Duration::from_millis(delay.into()), length));
                    }
                }
                ["skillquake", _, _, _] => {
                    if let (Some(skill), Some(delay), Some(length)) = (number(1), number(2), number(3)) {
                        let length = if length == 200 { QUAKE_SHORT } else { QUAKE_LONG };
                        table.skill_quakes.entry(skill as u16).or_default().push((Duration::from_millis(delay.into()), length));
                    }
                }
                ["part", _, _, text] => {
                    let Some(id) = number(1) else { continue };
                    table.floaters.entry(id).or_default().push(parameters(text));
                }
                ["ring", _, text] => {
                    let Some(id) = number(1) else { continue };
                    table.rings.entry(id).or_default().push(parameters(text));
                }
                ["misc", ..] => {
                    if let (Some(kind), Some(id)) = (number(1), number(2)) {
                        table.misc.insert(kind, id);
                    }
                }
                ["skill", _, role, _] => {
                    if let (Some(skill), Some(id)) = (number(1), number(3)) {
                        table.skills.entry((skill as u16, role.to_owned())).or_default().push(id);
                    }
                }
                ["item", ..] => {
                    if let (Some(item), Some(id)) = (number(1), number(2)) {
                        table.items.insert(item, id);
                    }
                }
                ["hit", ..] => {
                    if let Some(skill) = number(1) {
                        table.hits.insert(skill as u16, [effect(2), effect(3)]);
                    }
                }
                ["caster", ..] => {
                    if let Some(skill) = number(1) {
                        table.casters.insert(skill as u16, effect(2));
                    }
                }
                ["struck" | "used" | "ground", _, place, _] => {
                    let list = match fields[0] {
                        "struck" => &mut table.struck,
                        "used" => &mut table.used,
                        _ => &mut table.grounds,
                    };
                    if let (Some(skill), Some(id)) = (number(1), number(3)) {
                        list.entry(skill as u16).or_default().push((who(place), id));
                    }
                }
                ["trap", ..] => {
                    if let (Some(kind), Some(id)) = (number(1), number(2)) {
                        table.traps.insert(kind, id);
                    }
                }
                ["status", ..] => {
                    if let (Some(status), Some(id)) = (number(1), number(2)) {
                        table.statuses.entry(status as u16).or_default().push(id);
                    }
                }
                _ => {}
            }
        }
        table
    }

    /// A damaging skill's two effects on its target at each hit, when the table has the skill.
    pub fn hit(&self, skill: u16) -> Option<[Option<u32>; 2]> {
        self.hits.get(&skill).copied()
    }

    /// What a damaging skill shows on its caster as the packet comes: the table's, none for the newest jobs' skills,
    /// else 16.
    /// The aura a cast of `skill` with a cast time glows in: `by_element` (the cast's element's aura) for the table's 12,
    /// none for −1, else the row's effect; a skill not listed shows Bash's burst (16), the 4th jobs' none.
    // ponytail: the table's own `None` rows show no aura.
    pub fn cast_aura(&self, skill: u16, by_element: u32) -> Option<u32> {
        match self.casts.get(&skill) {
            Some(-1) => None,
            Some(12) => Some(by_element),
            Some(&effect) => u32::try_from(effect).ok(),
            None if NO_CASTER_EFFECT.iter().any(|range| range.contains(&skill)) => None,
            None => Some(CASTER_EFFECT),
        }
    }

    pub fn caster(&self, skill: u16) -> Option<u32> {
        match self.casters.get(&skill) {
            Some(&effect) => effect,
            None if NO_CASTER_EFFECT.iter().any(|range| range.contains(&skill)) => None,
            None => Some(CASTER_EFFECT),
        }
    }

    /// What else a damaging skill shows as the packet comes, when the table has the skill.
    pub fn struck(&self, skill: u16) -> Option<&[(Who, u32)]> {
        self.struck.get(&skill).map(Vec::as_slice)
    }

    /// What a skill without damage shows, when the table has the skill.
    pub fn used(&self, skill: u16) -> Option<&[(Who, u32)]> {
        self.used.get(&skill).map(Vec::as_slice)
    }

    /// What a ground skill shows, when the table has the skill.
    pub fn ground(&self, skill: u16) -> Option<&[(Who, u32)]> {
        self.grounds.get(&skill).map(Vec::as_slice)
    }

    /// What a trap of this skill unit type shows when it goes off.
    pub fn trap(&self, kind: u32) -> Option<u32> {
        self.traps.get(&kind).copied()
    }

    /// The effects a status (EFST id) keeps playing on its unit while it's on.
    pub fn status(&self, status: u16) -> &[u32] {
        self.statuses.get(&status).map_or(&[], Vec::as_slice)
    }

    /// The effect of a ZC_NOTIFY_EFFECT type (level ups, refining, ...).
    pub fn misc(&self, kind: u32) -> Option<u32> {
        self.misc.get(&kind).copied()
    }

    /// The effects a skill plays at a moment: `effectId` (on the target), `effectIdOnCaster`, `hitEffectId`, ...
    pub fn skill(&self, skill: u16, role: &str) -> Vec<u32> {
        self.skills.get(&(skill, role.to_owned())).cloned().unwrap_or_default()
    }

    pub fn item(&self, item: u32) -> Option<u32> {
        self.items.get(&item).copied()
    }
}

/// An STR file with its textures, by layer.
struct Loaded {
    animation: Str,
    textures: Vec<Vec<Option<Texture>>>,
}

/// Where an effect plays: on a unit (None = the player), at a place, or at a skill's ground unit until it goes.
#[derive(Clone, Copy)]
enum Anchor {
    Unit(Option<u32>),
    At(Vector),
    Lasting(u32, Vector),
    /// On a unit (None = the player) for as long as its status (EFST id) is on.
    Status(Option<u32>, u16),
}

impl Anchor {
    /// Where it is now; `position_of` gives a unit's feet.
    fn place(self, position_of: &impl Fn(Option<u32>) -> Option<Vector>) -> Option<Vector> {
        match self {
            Anchor::Unit(unit) | Anchor::Status(unit, _) => position_of(unit),
            Anchor::At(at) | Anchor::Lasting(_, at) => Some(at),
        }
    }

    /// The unit (None = the player) it is on, if it is on one.
    fn unit(self) -> Option<Option<u32>> {
        match self {
            Anchor::Unit(unit) | Anchor::Status(unit, _) => Some(unit),
            _ => None,
        }
    }

    fn on(self, unit: u32) -> bool {
        matches!(self, Anchor::Unit(Some(id)) | Anchor::Lasting(id, _) | Anchor::Status(Some(id), _) if id == unit)
    }
}

/// What an effect is played on.
#[derive(Clone, Copy)]
pub enum Target {
    /// A unit (None = the player): parts that follow units follow it.
    Unit(Option<u32>),
    Ground,
    /// A skill's unit on the ground (Fire Wall, a warp portal): its parts start again until it goes.
    Lasting(u32),
    /// A skill's unit on the ground whose effect plays once and is kept until it goes (Sanctuary, Gospel).
    Kept(u32),
    /// A unit (None = the player) under a status (EFST id): its parts follow it and start again until
    /// `forget_status`.
    Status(Option<u32>, u16),
}

struct Playing {
    loaded: Rc<Loaded>,
    /// Its files to pick from again at each restart (`%d` ones), else just `loaded`.
    variants: Vec<Rc<Loaded>>,
    start: Instant,
    anchor: Anchor,
    again: bool,
    /// The effect's own length in ticks, when shorter than its STR (or how often a ground unit's starts again).
    length: Option<u32>,
    effect: u32,
}

impl Playing {
    /// A copy as it stands, to play out under its restart.
    fn clone_now(&self) -> Self {
        Self {
            loaded: self.loaded.clone(),
            variants: self.variants.clone(),
            start: self.start,
            anchor: self.anchor,
            again: self.again,
            length: self.length,
            effect: self.effect,
        }
    }
}

/// The ticks an STR shows: to its last key, or its effect's length if that's shorter.
fn str_ticks(str: &Str, length: Option<u32>) -> u32 {
    let keys = str.last_key().max(0) as u32 + 1;
    length.map_or(keys, |length| length.min(keys))
}

/// Reject Sword's file ends at its last key, but two smudge layers (9 and 10) stay frozen where they were to tick 199.
// ponytail: the layers are counted from 0 in the file's order.
const REJECT_KEPT: [usize; 2] = [9, 10];
const REJECT_SWORD_TICKS: u32 = 200;

/// The frame to show a layer at once it has ended, if the effect keeps it: its last key's.
fn kept_layer_frame(effect: u32, index: usize, layer: &rustnarok_client::effect::Layer, frame: f32) -> Option<f32> {
    (effect == REJECT_SWORD && REJECT_KEPT.contains(&index) && frame < REJECT_SWORD_TICKS as f32)
        .then(|| layer.keys.iter().map(|key| key.frame).max().unwrap_or(0) as f32)
}

/// How many ticks an STR playing for `effect` is kept: to its last key, or its effect's length if shorter; Reject Sword's 200.
fn playing_ticks(effect: u32, str: &Str, length: Option<u32>) -> u32 {
    if effect == REJECT_SWORD { REJECT_SWORD_TICKS } else { str_ticks(str, length) }
}

/// How often a ground unit's effect starts again, in ticks: 8 before its length ends (Quagmire's 20).
fn restart_ticks(effect: u32, length: u32) -> u32 {
    // Shockwave's waves start again every 75 ticks.
    if effect == 145 {
        return 75;
    }
    length.saturating_sub(if effect == 95 { 20 } else { 8 }).max(1)
}

/// The STR frame `elapsed` into an effect: whole 16 ms ticks.
fn str_frame(elapsed: Duration) -> u32 {
    (elapsed.as_secs_f32() * 1000.0 / STR_TICK_MS) as u32
}

/// A key's corner in sprite pixels from the anchor: STR space is centred on (320, 240).
fn str_corner(position: [f32; 2], corner: [f32; 2], angle: f32) -> [f32; 2] {
    let (sin, cos) = angle.to_radians().sin_cos();
    let [x, y] = corner;
    [x * cos - y * sin + position[0] - 320.0, x * sin + y * cos + position[1] - 240.0]
}

/// A sprite playing one of its actions (SPR effects).
struct Animation {
    /// The sprite's path without extension.
    sprite: String,
    start: Instant,
    /// When it goes, if it repeats for a while.
    end: Option<Instant>,
    anchor: Anchor,
    /// Which action; else the one facing the camera.
    action: Option<usize>,
    /// Milliseconds a frame; else the sprite's own.
    frame_ms: Option<f32>,
    repeat: bool,
    /// Stays on its last frame.
    hold: bool,
    /// Where it shows from the feet, in sprite pixels.
    offset: [f32; 2],
    /// How much bigger it is drawn on top of its layers' own zoom, how opaque (0 to 1), and which frame it starts on.
    scale: f32,
    alpha: f32,
    first: f32,
    /// Drawn over everything with no depth test.
    over: bool,
    /// Its images now, from `advance`.
    placed: Vec<Placed>,
    effect: u32,
}

/// How far through its frames a repeating or playing sprite is, 0 to 1: `elapsed` ms in, `frame_ms` a frame, starting on frame
/// `first` (then a frame is picked whole).
fn animation_progress(elapsed: f32, frame_ms: f32, frames: f32, first: f32, repeat: bool) -> f32 {
    let pass = frames * frame_ms.max(1.0);
    match (repeat, first > 0.0) {
        // Played once from a later frame, a frame at a time, and held on the last.
        (false, true) => ((elapsed / frame_ms.max(1.0)) as u32 as f32 + first).min(frames - 1.0).floor() / frames + 0.5 / frames,
        (false, false) => (elapsed / pass).min(1.0),
        (true, false) => elapsed % pass / pass,
        (true, true) => (((elapsed / frame_ms.max(1.0)) as u32 as f32 + first) % frames).floor() / frames + 0.5 / frames,
    }
}

impl Animation {
    fn new(part: &SpritePart, effect: u32, start: Instant, anchor: Anchor, lasting: bool) -> Self {
        let number = |name: &str| part.parameters.get(name).and_then(|value| value.parse::<f32>().ok());
        let flag = |name: &str| part.parameters.get(name).is_some_and(|value| value == "true");
        let rise = if flag("head") { -100.0 } else { 0.0 };
        let repeat = flag("repeat") || (lasting && !flag("hold"));
        Self {
            sprite: format!("data\\sprite\\이팩트\\{}", part.file),
            start,
            end: number("duration").filter(|_| repeat).map(|ms| start + Duration::from_secs_f32(ms.max(0.0) / 1000.0)),
            anchor,
            action: number("frame").map(|action| action as usize),
            frame_ms: number("delayFrame").map(|ms| ms.max(1.0)),
            repeat,
            hold: flag("stopAtEnd") || flag("hold"),
            offset: [number("xOffset").unwrap_or(0.0), number("yOffset").unwrap_or(rise)],
            scale: number("scale").unwrap_or(1.0),
            alpha: number("alpha").map_or(1.0, |alpha| alpha / 255.0),
            first: number("firstFrame").unwrap_or(0.0),
            over: flag("onTop"),
            placed: Vec::new(),
            effect,
        }
    }
}

/// Where a unit's standing model is.
fn standing_at<T>(standing: &[(T, u32, Vector)], unit: u32) -> Option<Vector> {
    standing.iter().find(|(_, id, _)| *id == unit).map(|(_, _, at)| *at)
}

/// A model standing on a skill's ground unit (traps): by texture, its triangles' corners.
struct Model(Vec<(String, Vec<Corner>)>);

/// Where a corner is from the model's feet, in world units, and its texture coordinates.
type Corner = (Vector, [f32; 2]);

/// A texture or sprite moving from a start to an end (2D and 3D effects), its random choices made.
struct Floating {
    /// Textures under data\texture\, shown in turn `frame_ms` apart.
    textures: Vec<String>,
    frame_ms: f32,
    /// Or a sprite (its path without extension), sized in percent, on its first frame or playing (`play_sprite`).
    sprite: Option<String>,
    play_sprite: bool,
    /// The sprite's images now, from `advance`.
    placed: Vec<Placed>,
    /// Starts again when it ends.
    again: bool,
    start: Instant,
    end: Instant,
    anchor: Anchor,
    /// Start and end on each axis, in cells: across, along and up.
    position: [[f32; 2]; 3],
    smooth: [bool; 3],
    /// Width and height, start and end, in sprite pixels.
    size: [[f32; 2]; 2],
    size_smooth: bool,
    alpha: f32,
    fade_in: bool,
    fade_out: bool,
    color: [f32; 3],
    /// Degrees at the start and the end.
    angle: [f32; 2],
    /// A ray: drawn outward from the center along its angle.
    ray: bool,
    additive: bool,
    effect: u32,
}

/// A textured cylinder growing, turning and fading (CYLINDER effect), in cells.
struct Ring {
    texture: String,
    start: Instant,
    end: Instant,
    anchor: Anchor,
    /// Sides of the whole circle, and how many of them show.
    sides: usize,
    shown: usize,
    repeat: f32,
    top: f32,
    bottom: f32,
    height: f32,
    /// Animations: 1 grows up, 2 widens at the top, 3 shrinks away, 4 widens, 5 grows up and back.
    animation: u32,
    fade: bool,
    color: [f32; 4],
    offset: [f32; 3],
    /// Degrees about the across, up and along axes; and whether it spins.
    angles: [f32; 3],
    spin: bool,
    additive: bool,
    /// Plays again and again while its unit is there (warp portals).
    again: bool,
    effect: u32,
}

impl Ring {
    fn new(parameters: &HashMap<String, String>, effect: u32, start: Instant, anchor: Anchor) -> Option<Self> {
        let number = |name: &str| parameters.get(name).and_then(|value| value.parse::<f32>().ok());
        let flag = |name: &str| parameters.get(name).is_some_and(|value| value == "true");
        let sides = number("totalCircleSides").unwrap_or(20.0).max(3.0) as usize;
        let start = start + std::time::Duration::from_secs_f32(number("delayStart").unwrap_or(0.0).max(0.0) / 1000.0);
        let turn = |name: &str| number(name).unwrap_or(0.0) + number(&format!("{name}Random")).map_or(0.0, |spread| random(0.0, spread).floor());
        Some(Self {
            texture: format!("data\\texture\\effect\\{}.tga", parameters.get("textureName")?),
            start,
            end: start + std::time::Duration::from_secs_f32(number("duration").unwrap_or(1000.0).max(1.0) / 1000.0),
            anchor,
            sides,
            shown: number("circleSides").map_or(sides, |shown| (shown as usize).min(sides)),
            repeat: number("repeatTextureX").unwrap_or(1.0),
            top: number("topSize").unwrap_or(1.0),
            bottom: number("bottomSize").unwrap_or(1.0),
            height: number("height").unwrap_or(1.0),
            animation: number("animation").unwrap_or(0.0) as u32,
            fade: flag("fade"),
            color: [
                number("red").unwrap_or(1.0),
                number("green").unwrap_or(1.0),
                number("blue").unwrap_or(1.0),
                number("alphaMax").filter(|alpha| *alpha > 0.0).unwrap_or(1.0),
            ],
            offset: [number("posX").unwrap_or(0.0), number("posZ").unwrap_or(0.0), number("posY").unwrap_or(0.0)],
            angles: [turn("angleX"), turn("angleY"), turn("angleZ")],
            spin: flag("rotate"),
            additive: number("blendMode") == Some(2.0),
            again: flag("repeat"),
            effect,
        })
    }

    /// Its triangles at `now`, standing on `at`.
    fn mesh(&self, now: Instant, at: Vector) -> Vec<MeshVertex> {
        let length = self.end.duration_since(self.start).as_secs_f32() * 1000.0;
        let elapsed = now.duration_since(self.start).as_secs_f32() * 1000.0;
        let share = (elapsed / length).clamp(0.0, 1.0);
        let grow = (elapsed / length.min(1000.0)).min(1.0);
        let up_and_back = if share < 0.5 { share * 2.0 } else { (1.0 - share) * 2.0 };
        let (mut top, mut bottom, mut height) = (self.top, self.bottom, self.height);
        match self.animation {
            1 => height *= grow,
            2 => top *= grow,
            3 => {
                top *= 1.0 - share;
                bottom *= 1.0 - share;
                height *= up_and_back;
            }
            4 => {
                top *= share;
                bottom *= share;
            }
            5 => height *= up_and_back,
            _ => {}
        }
        let mut color = self.color;
        if self.fade {
            let alpha = if share < 0.25 {
                share * 4.0
            } else if share > 0.75 {
                (1.0 - share) * 4.0
            } else {
                1.0
            };
            color[3] *= alpha;
        }
        // Turns: a spin about the up axis (a quarter degree a millisecond), then the set angles.
        let spin = if self.spin { now.duration_since(self.start).as_secs_f32() * 250.0 } else { 0.0 };
        let turn = |point: [f32; 3]| -> [f32; 3] {
            let about_up = |[x, y, z]: [f32; 3], degrees: f32| {
                let (sin, cos) = degrees.to_radians().sin_cos();
                [x * cos + z * sin, y, -x * sin + z * cos]
            };
            let mut point = about_up(point, spin);
            let (sin, cos) = self.angles[0].to_radians().sin_cos();
            point = [point[0], point[1] * cos - point[2] * sin, point[1] * sin + point[2] * cos];
            point = about_up(point, self.angles[1]);
            let (sin, cos) = self.angles[2].to_radians().sin_cos();
            [point[0] * cos - point[1] * sin, point[0] * sin + point[1] * cos, point[2]]
        };
        let place = |local: [f32; 3]| {
            let turned = turn(local);
            std::array::from_fn(|axis| at[axis] + (self.offset[axis] + turned[axis]) * CELL)
        };
        let ring = |side: usize, radius: f32, rise: f32, v: f32| {
            let share = side as f32 / self.sides as f32;
            let (sin, cos) = (share * std::f32::consts::TAU).sin_cos();
            MeshVertex {
                position: place([sin * radius, rise, cos * radius]),
                uv: [share * (self.sides as f32 / self.shown as f32) * self.repeat, v],
                tint: color,
            }
        };
        (0..self.shown)
            .flat_map(|side| {
                let [low, high, next_low, next_high] =
                    [ring(side, bottom, 0.0, 1.0), ring(side, top, height, 0.0), ring(side + 1, bottom, 0.0, 1.0), ring(side + 1, top, height, 0.0)];
                [low, high, next_low, high, next_low, next_high]
            })
            .collect()
    }
}

/// EF_HIT1, made in code: a blue ring flying off the target the way it faces and four sparks flying out with short
/// trails. Everything moves once a tick (16 ms), changing its speed first and then moving, so it has made its first move
/// when it's first drawn; it lasts 50 ticks and stays where it was made.
struct HitSpark {
    start: Instant,
    /// The target until the hit lands, then where its feet were then.
    anchor: Anchor,
    /// The way the ring flies across the ground (x east, z north), a unit vector.
    way: [f32; 2],
    sparks: [Spark; 4],
    /// Each spark's place from the feet on each drawn tick, worked out as the hit lands (`settle_hits`).
    paths: Option<[Vec<Vector>; 4]>,
    /// The spark sprite's images, from `advance`.
    placed: Vec<Placed>,
}

/// One of a hit's sparks, its random choices made.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Spark {
    /// Its yaw from the north–south line (toward the east) and its pitch, in radians.
    yaw: f32,
    pitch: f32,
    /// How far out along its heading it starts, and how fast it goes a tick at first (falling evenly to half).
    out: f32,
    speed: f32,
    /// The back pair: it flies level and hops, rising this much a tick at first.
    back: bool,
    rise: f32,
    scale: f32,
    /// Ticks it lives.
    life: u32,
}

const HIT_TICK_MS: f32 = 16.0;
const HIT_TICKS: u32 = 50;
const RING_TICKS: u32 = 10;
const RING_TEXTURE: &str = "data\\texture\\effect\\ring_blue.tga";
const SPARK_SPRITE: &str = "data\\sprite\\이팩트\\particle1";
/// The ring's middle and the sparks' start, above the feet; the ring's inner and outer radius, and how much further
/// along its way the outer edge is (a shallow funnel opening the way it flies).
const HIT_HEIGHT: f32 = 10.0;
const RING_RADII: [f32; 2] = [5.0, 10.0];
const RING_FUNNEL: f32 = 3.5;
/// 10 sides of 36°; the texture's u goes up a quarter a side and starts again at 0 after four.
const RING_SIDES: usize = 10;
/// Each spark is drawn this many times, where it was that many ticks ago, smaller and fainter.
const TRAIL: u32 = 3;
const NEAR_OPAQUE: f32 = 254.0 / 255.0;
/// A back-pair spark falling onto higher ground bounces: its vertical speed divided by this.
const SPARK_BOUNCE: f32 = -1.5;

/// The way a hit's ring flies for a target facing `rot` degrees (0 south, 90 east, as `Actor::rot`): the way it faces.
fn drift(rot: f32) -> [f32; 2] {
    let (sin, cos) = rot.to_radians().sin_cos();
    [sin, -cos]
}

/// The ring's alpha on a drawn tick: 254 to tick 4, then 51 less a tick, 0 on tick 9.
fn ring_alpha(tick: u32) -> f32 {
    (254.0 - 51.0 * tick.saturating_sub(4) as f32).max(0.0) / 255.0
}

/// How far the ring has flown on a drawn tick: moves of 0.665 falling by 0.035 a tick, one made before it's drawn.
fn ring_travel(tick: u32) -> f32 {
    (1..=tick.min(RING_TICKS - 1) + 1).map(|step| 0.7 - 0.035 * step as f32).sum()
}

/// A hit's four sparks for a ring flying `way`: two within −40°…+39° of it and two behind (140°…219° off), each
/// pitched −50°…49°; `random(from, to)` picks every range.
fn sparks(way: [f32; 2], mut random: impl FnMut(f32, f32) -> f32) -> [Spark; 4] {
    let toward = way[0].atan2(way[1]);
    std::array::from_fn(|index| {
        let back = index >= 2;
        Spark {
            yaw: toward + if back { random(140.0, 220.0) } else { random(-40.0, 40.0) }.to_radians(),
            pitch: random(-50.0, 50.0).to_radians(),
            out: random(4.0, 7.0),
            speed: random(0.6, 1.5),
            back,
            rise: if back { random(0.3, 1.2) } else { 0.0 },
            scale: random(0.6, 1.6),
            life: random(6.0, 30.0) as u32,
        }
    })
}

impl Spark {
    /// Its heading, a unit vector (x east, y up, z north): the pitch turns it about the east–west axis.
    fn heading(&self) -> Vector {
        let ((sin, cos), (pitch_sin, pitch_cos)) = (self.yaw.sin_cos(), self.pitch.sin_cos());
        [sin, cos * pitch_sin, cos * pitch_cos]
    }

    /// Where it starts from the feet: `out` along its heading, 10 up.
    fn origin(&self) -> Vector {
        let heading = self.heading();
        [heading[0] * self.out, heading[1] * self.out + HIT_HEIGHT, heading[2] * self.out]
    }

    /// How far it has gone after `moves` moves: v0·(n − n(n+1)/(4L)).
    fn travel(&self, moves: u32) -> f32 {
        let n = moves as f32;
        self.speed * (n - n * (n + 1.0) / (4.0 * self.life as f32))
    }

    /// The back pair's height above its start after `moves` moves over flat ground: r·n·(1 − (n+1)/L).
    #[cfg(test)]
    fn hop(&self, moves: u32) -> f32 {
        let n = moves as f32;
        self.rise * n * (1.0 - (n + 1.0) / self.life as f32)
    }

    /// Its place from the feet on each drawn tick of its life (tick a after a + 1 moves). The front pair flies along its
    /// heading; the back pair flies level along its yaw and hops, bouncing where it falls onto higher ground
    /// (`ground(x, z)`: the ground's height there, from the feet).
    // ponytail: the bounce flips the vertical speed after the move that takes it under the ground; it isn't put back up.
    fn path(&self, ground: impl Fn(f32, f32) -> f32) -> Vec<Vector> {
        let origin = self.origin();
        if !self.back {
            let heading = self.heading();
            return (0..self.life).map(|tick| std::array::from_fn(|axis| origin[axis] + heading[axis] * self.travel(tick + 1))).collect();
        }
        let (sin, cos) = self.yaw.sin_cos();
        let (mut place, mut up) = (origin, self.rise);
        (1..=self.life)
            .map(|step| {
                let speed = self.speed * (1.0 - 0.5 * step as f32 / self.life as f32);
                up -= 2.0 * self.rise / self.life as f32;
                place = [place[0] + sin * speed, place[1] + up, place[2] + cos * speed];
                if up < 0.0 && place[1] < ground(place[0], place[2]) {
                    up /= SPARK_BOUNCE;
                }
                place
            })
            .collect()
    }

    /// Its alpha on a drawn tick: 254·(1 − (a + 1)/L), 0 on its last.
    fn alpha(&self, tick: u32) -> f32 {
        NEAR_OPAQUE * (1.0 - (tick + 1) as f32 / self.life as f32).max(0.0)
    }

    /// Its trail on a drawn tick: each copy's place from the feet, alpha and scale. Copy i is where it was i ticks ago
    /// (at the feet, on the ground, before its first tick), at 1, 5/6 and 2/3 of its scale and 1, 2/3 and 1/3 of its alpha.
    fn trail(&self, tick: u32, path: &[Vector]) -> Vec<(Vector, f32, f32)> {
        if tick >= self.life {
            return Vec::new();
        }
        (0..TRAIL)
            .map(|copy| {
                let place = tick.checked_sub(copy).and_then(|then| path.get(then as usize)).copied().unwrap_or([0.0; 3]);
                let share = (TRAIL - copy) as f32 / TRAIL as f32;
                (place, self.alpha(tick) * share, self.scale * (6 - copy) as f32 / 6.0)
            })
            .collect()
    }
}

impl HitSpark {
    /// Whole ticks since it started.
    fn tick(&self, now: Instant) -> u32 {
        (now.saturating_duration_since(self.start).as_secs_f32() * 1000.0 / HIT_TICK_MS) as u32
    }

    /// The ring's triangles on a drawn tick, the feet `at`: upright across its way, its outer edge 3.5 further along.
    fn ring(&self, tick: u32, at: Vector) -> Vec<MeshVertex> {
        let travel = ring_travel(tick);
        let middle = [at[0] + self.way[0] * travel, at[1] + HIT_HEIGHT, at[2] + self.way[1] * travel];
        let tint = [1.0, 1.0, 1.0, ring_alpha(tick)];
        // Around in the upright plane across the way; the texture's v runs from the inner edge (1) to the outer (0).
        let point = |side: usize, edge: usize, u: f32| {
            let (sin, cos) = (side as f32 / RING_SIDES as f32 * std::f32::consts::TAU).sin_cos();
            let (radius, along) = (RING_RADII[edge], edge as f32 * RING_FUNNEL);
            MeshVertex {
                position: [
                    middle[0] + self.way[1] * radius * cos + self.way[0] * along,
                    middle[1] + radius * sin,
                    middle[2] - self.way[0] * radius * cos + self.way[1] * along,
                ],
                uv: [u, 1.0 - edge as f32],
                tint,
            }
        };
        (0..RING_SIDES)
            .flat_map(|side| {
                let u = (side % 4) as f32 * 0.25;
                let [inner, outer, next_inner, next_outer] = [point(side, 0, u), point(side, 1, u), point(side + 1, 0, u + 0.25), point(side + 1, 1, u + 0.25)];
                [inner, outer, next_inner, outer, next_inner, next_outer]
            })
            .collect()
    }
}

/// A band round a unit's feet: an arc (degrees, 360 closed) from a turn, its foot `radius` out on the ground, leaning
/// `lean` degrees above the ground, `height` tall at most, at an alpha of 0 to 255.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Band {
    arc: f32,
    turn: f32,
    radius: f32,
    lean: f32,
    height: f32,
    alpha: f32,
}

/// Quads along a band.
const BAND_QUADS: usize = 20;

/// A band's 21 points from the feet: each its foot on the ground and its top, `height(s)` along the lean.
// ponytail: 0° points north (+z) and turns go toward the east.
fn band_points(band: &Band, height: impl Fn(usize) -> f32) -> Vec<(Vector, Vector)> {
    let (lean_sin, lean_cos) = band.lean.to_radians().sin_cos();
    (0..=BAND_QUADS)
        .map(|s| {
            let (sin, cos) = (band.turn + s as f32 * band.arc / BAND_QUADS as f32).to_radians().sin_cos();
            let (out, up) = (band.radius + height(s) * lean_cos, height(s) * lean_sin);
            ([sin * band.radius, 0.0, cos * band.radius], [sin * out, up, cos * out])
        })
        .collect()
}

/// A band's triangles at the feet `at`: u = s / 21 along it, v from the foot (0) to the top (1).
fn band_mesh(at: Vector, band: &Band, height: impl Fn(usize) -> f32, color: [f32; 3]) -> Vec<MeshVertex> {
    let tint = [color[0], color[1], color[2], band.alpha.clamp(0.0, 255.0) / 255.0];
    let vertex = |point: Vector, s: usize, v: f32| MeshVertex { position: std::array::from_fn(|axis| at[axis] + point[axis]), uv: [s as f32 / 21.0, v], tint };
    let points = band_points(band, height);
    (0..BAND_QUADS)
        .flat_map(|s| {
            let ((foot, top), (next_foot, next_top)) = (points[s], points[s + 1]);
            let [a, b, c, d] = [vertex(foot, s, 0.0), vertex(top, s, 1.0), vertex(next_foot, s + 1, 0.0), vertex(next_top, s + 1, 1.0)];
            [a, b, c, b, c, d]
        })
        .collect()
}

/// An effect of bands round the feet, made in code: a cast's aura (EF_BEGINSPELL 12 and the elements' 54–59), Heal
/// (312, 313, 325), the teleport (304), the arrival (344) and the logout (314). Moved to its anchor every 16 ms tick.
struct Banded {
    effect: u32,
    anchor: Anchor,
    start: Instant,
    /// How long it lasts, in ticks.
    ticks: u32,
    /// The bands' start turns, picked at random (Heal's two, Safety Wall's three).
    turns: [f32; 3],
    /// The undead heal's walls' height and first radius, picked at random.
    extra: [f32; 2],
    /// Heal's sparkles: the tick each starts and where on the ground, from the feet.
    sparkles: Vec<(u32, [f32; 2])>,
}

/// The couple and blue casting auras: the blooming bands of Heal's cast in pink and in blue.
const COUPLE_CASTING: u32 = 342;
const BLUE_CASTING: u32 = 441;

/// A world offset from a point as pixels from its projection: the camera turned `yaw` radians and tipped `pitch` degrees, 7 pixels
/// a unit.
// ponytail: an orthographic view of the offset, not the frame's perspective.
fn screen_offset(offset: Vector, yaw: f32, pitch: f32) -> [f32; 2] {
    let (sin, cos) = yaw.sin_cos();
    let across = offset[0] * cos + offset[2] * sin;
    let deep = -offset[0] * sin + offset[2] * cos;
    let (tip_sin, tip_cos) = pitch.to_radians().sin_cos();
    [across * PIXELS_PER_UNIT, (deep * tip_sin - offset[1] * tip_cos) * PIXELS_PER_UNIT]
}

/// Whether a status's circle ignores a packet: Song of Mana's (439) and Wug Dance's (441) run out their 400 ticks when the
/// status goes off, and start again only for an on-packet whose value is exactly 1.
pub fn circle_ignores(status: u16, on: bool, value: i64) -> bool {
    matches!(status, 439 | 441) && (!on || value != 1)
}

/// A cast's aura: one a caster has at a time.
pub fn is_cast_aura(effect: u32) -> bool {
    matches!(effect, 12 | 54..=59 | COUPLE_CASTING | BLUE_CASTING | 1872 | 1876 | 1902 | 1910 | 1918 | 1923)
}

/// The effects made of bands.
pub fn is_banded(effect: u32) -> bool {
    matches!(effect, 12 | 54..=59 | 304 | 312 | 313 | 314 | 315 | 320 | 325 | 342 | 344 | 367 | 441 | 1872 | 1876 | 1902 | 1910 | 1918 | 1923)
}

/// How long a band effect lasts, in ticks: Heal and the arrival 100, the teleport and logout 200, Aura Blade's bloom 60.
fn banded_ticks(effect: u32) -> u32 {
    match effect {
        304 | 314 => 200,
        367 => 60,
        // Safety Wall stands until its skill unit goes.
        315 => u32::MAX / 4,
        _ => 100,
    }
}

/// The fade of Heal's, the arrival's and the logout's bands: +5 alpha a tick from age 1 to 15 (75), held until 60, then
/// −2 a tick to nothing (about age 97).
fn band_fade(age: u32) -> f32 {
    match age {
        0..=15 => 5.0 * age as f32,
        16..=60 => 75.0,
        _ => (75.0 - 2.0 * (age - 60) as f32).max(0.0),
    }
}

/// Heal's sparkle bursts: 4 every 6 ticks until age 50, each −6…+6 units on both ground axes.
fn heal_sparkles(mut random: impl FnMut(f32, f32) -> f32) -> Vec<(u32, [f32; 2])> {
    (0..50).step_by(6).flat_map(|age| (0..4).map(|_| (age, [random(-6.0, 6.0), random(-6.0, 6.0)])).collect::<Vec<_>>()).collect()
}

/// A sparkle's place above its spot and its alpha, `since` ticks after it starts, while it shows.
// ponytail: it rises 0.3 a tick from 2 up and fades out over 24 ticks, 10 sprite pixels across.
fn sparkle_at(since: u32) -> Option<(f32, f32)> {
    (since < SPARKLE_TICKS).then(|| (2.0 + 0.3 * since as f32, 1.0 - since as f32 / SPARKLE_TICKS as f32))
}

const SPARKLE_TICKS: u32 = 24;
const SPARKLE_SIZE: f32 = 10.0;
const SPARKLE_TEXTURE: &str = "data\\texture\\effect\\pok1.tga";

/// The teleport's (304) four closed columns: radius, height, lean, start turn and alpha.
const TELEPORT_BANDS: [[f32; 5]; 4] =
    [[1.5, 100.0, 90.0, 180.0, 100.0], [3.0, 70.0, 89.0, 270.0, 80.0], [4.0, 40.0, 88.0, 0.0, 60.0], [5.0, 15.0, 87.0, 90.0, 40.0]];
/// The arrival's (344): two upright walls and two near-flat rings, as radius, height and lean.
const ARRIVAL_BANDS: [[f32; 3]; 4] = [[3.7, 30.0, 90.0], [3.4, 30.0, 90.0], [3.6, 4.0, 10.0], [3.7, 4.0, 5.0]];

/// How long a cast's aura lasts: the cast time in 16 ms ticks, at least 56 for 12 and 58 and 70 for the rest.
fn aura_ticks(effect: u32, cast_ms: u32) -> u32 {
    (cast_ms / 16).max(if matches!(effect, 12 | 58 | COUPLE_CASTING | BLUE_CASTING | 1872 | 1876 | 1902 | 1910 | 1918 | 1923) { 56 } else { 70 })
}

/// A band effect's texture, colour and whether it adds on.
fn aura_look(effect: u32) -> (&'static str, [u8; 3], bool) {
    match effect {
        367 => ("ring_white", [0xA4, 0xCD, 0xFE], true),
        312 | 313 | 325 => ("alpha_down", [0x8C, 0xD2, 0x8C], true),
        315 => ("alpha_down", [0xFF, 0x59, 0xB6], true),
        320 => ("alpha_down", [0xFF, 0xFF, 0xFF], true),
        304 => ("Magic_Violet", [0x64, 0x64, 0xFF], true),
        314 => ("ring_purple", [0x64, 0x64, 0xFF], true),
        344 => ("ring_blue", [0x64, 0x64, 0xFF], true),
        58 => ("ring_white", [0xFF, 0xFF, 0xFF], true),
        COUPLE_CASTING => ("ring_red", [0xFF, 0x59, 0xB6], true),
        BLUE_CASTING => ("ring_blue", [0x64, 0x64, 0xFF], true),
        54 => ("ring_blue", [0xC3, 0xC3, 0xFF], false),
        55 => ("ring_yellow", [0xFF, 0xAF, 0xAF], true),
        57 => ("ring_yellow", [0xFF, 0xFF, 0xFF], false),
        56 => ("Magic_Green", [0xFF, 0xFF, 0xFF], false),
        59 => ("Magic_Violet", [0xFF, 0xFF, 0xFF], false),
        1872 => ("ring_white", [0x2B, 0x28, 0x2E], false),
        1876 => ("ring_white", [0x7D, 0xB6, 0xA0], false),
        1923 => ("ring_red", [0xB4, 0x37, 0x1E], false),
        1902 => ("new_abr_mother\\abr_mother_ring", [0xFF, 0xFF, 0xFF], true),
        1910 => ("new_abr_infinity\\ring_infinity", [0xFF, 0xFF, 0xFF], true),
        1918 => ("new_d_cannon\\abr_dual_cannon_ring", [0xFF, 0xFF, 0xFF], true),
        _ => ("ring_yellow", [0xFF, 0xFF, 0xAA], true),
    }
}

fn aura_texture(effect: u32) -> String {
    format!("data\\texture\\effect\\{}.tga", aura_look(effect).0)
}

/// The texture of bloom set `set` of an aura: Aura Blade's second set is ring_yellow, the others' the aura's own.
fn aura_set_texture(effect: u32, set: usize) -> String {
    if effect == 367 && set == 1 { "data\\texture\\effect\\ring_yellow.tga".to_owned() } else { aura_texture(effect) }
}

/// How many sets of bands an aura draws with their own textures.
fn aura_sets(effect: u32) -> usize {
    if effect == 367 { 2 } else { 1 }
}

/// The blooming bands' first alphas (two sets of four), their heights and turns.
const BLOOM_ALPHAS: [[i32; 4]; 2] = [[180, 135, 90, 45], [160, 115, 70, 25]];
const BLOOM_HEIGHTS: [f32; 4] = [20.0, 19.0, 18.0, 17.0];
const BLOOM_TURNS: [f32; 4] = [180.0, 270.0, 0.0, 90.0];

/// Blooming band `band` of set `set` (12 and 58) `age` ticks into an aura of `ticks`: each tick it widens 0.07 and leans
/// 1° less (at 10° it's gone), gaining 10 alpha while under radius 4 and losing 3 from there; gone, it starts again at
/// 3.37 leaning 74° while the aura has more than 30 ticks to go. An aura of 60 ticks or less starts every band at alpha
/// 0, band k 5k ticks late. None before it starts or while gone.
fn bloom(set: usize, band: usize, ticks: u32, age: u32) -> Option<Band> {
    let short = ticks <= 60;
    let late = if short { 5 * band as u32 } else { 0 };
    let since = age.checked_sub(late)?;
    // Hundredths of a unit, so the 0.07 steps meet 4.0 exactly.
    let (mut radius, mut lean, mut alpha) = (410, 80, if short { 0 } else { BLOOM_ALPHAS[set][band] });
    for tick in 0..=since {
        alpha = if radius < 400 { alpha + 10 } else { (alpha - 3).max(0) };
        radius += 7;
        lean -= 1;
        if lean <= 10 {
            alpha = 0;
        }
        if alpha == 0 && late + tick + 30 < ticks {
            (radius, lean) = (337, 74);
        }
    }
    (alpha > 0).then(|| Band {
        arc: 360.0,
        turn: BLOOM_TURNS[band],
        radius: radius as f32 / 100.0,
        lean: lean as f32,
        height: BLOOM_HEIGHTS[band],
        alpha: alpha as f32,
    })
}

/// A blooming band's top waving along it: h·(1 + 0.3·sin 9s°·sin θ), θ = (age + 90k)° for bands 0–1 and 2·(age + 45k)°
/// for bands 2–3.
fn bloom_height(band: usize, height: f32, age: u32, s: usize) -> f32 {
    let k = band as f32;
    let theta = if band < 2 { age as f32 + 90.0 * k } else { 2.0 * (age as f32 + 45.0 * k) };
    height * (1.0 + 0.3 * (9.0 * s as f32).to_radians().sin() * theta.to_radians().sin())
}

/// The turning bands (54–57, 59): three open 315° arcs and a faint column, `age` ticks into an aura of `ticks`. The arcs
/// turn 3, 4 and 5° a tick; in the last 40 ticks everything fades, 5 a tick (the column 2).
fn turning(band: usize, ticks: u32, age: u32) -> Band {
    let fading = age.saturating_sub(ticks.saturating_sub(40)) as f32;
    if band == 3 {
        let alpha = if age == 0 { 70.0 } else { 60.0 };
        return Band { arc: 360.0, turn: 0.0, radius: 4.0, lean: 89.0, height: 250.0, alpha: (alpha - 2.0 * fading).max(0.0) };
    }
    let k = band as f32;
    Band {
        arc: 315.0,
        turn: 90.0 * k + (3.0 + k) * age as f32,
        radius: 4.5 + 0.5 * k,
        lean: [70.0, 57.0, 45.0][band],
        height: [25.0, 22.0, 19.0][band],
        alpha: (180.0 - 5.0 * fading).max(0.0),
    }
}

/// A turning arc's top along it: h·sin 9s°, rising with sin(age°) over the first 90 ticks.
// ponytail: the column stands its full height all along.
fn turning_height(band: usize, height: f32, age: u32, s: usize) -> f32 {
    if band == 3 {
        return height;
    }
    height * (9.0 * s as f32).to_radians().sin() * (age.min(90) as f32).to_radians().sin()
}

/// A band rising over the first 90 ticks: height × sin(age°).
// ponytail: its rim doesn't wave.
fn rising_height(_band: usize, height: f32, age: u32, _s: usize) -> f32 {
    height * (age.min(90) as f32).to_radians().sin()
}

/// A wall's alpha at its age `b` (from 1): 5 a tick to 75 at 15, held, then 2 less a tick once `b` is past `held`.
fn wall_alpha(b: u32, held: u32) -> f32 {
    if b <= 15 { 5.0 * b as f32 } else { (75.0 - 2.0 * b.saturating_sub(held) as f32).max(0.0) }
}

/// A wall's top at its age `age` (b = age + 1), point `s` of the band `k`: h·(0.75 + 0.25·sin θ) with θ = 34 s + (4 − k) b
/// degrees, times sin b while b is up to 90: the tops roll in waves and rise over the first 1.5 s.
fn rim_height(k: usize, height: f32, age: u32, s: usize) -> f32 {
    let b = age + 1;
    let theta = (34 * s as u32 + (4 - k as u32) * b) % 360;
    let rise = if b <= 90 { (b as f32).to_radians().sin() } else { 1.0 };
    (height * (0.75 + 0.25 * (theta as f32).to_radians().sin()) * rise).max(0.0)
}

/// A band its full height all along.
fn full_height(_band: usize, height: f32, _age: u32, _s: usize) -> f32 {
    height
}

/// The teleport's columns at a tick.
// ponytail: they stand still and fade out over the last 40 ticks.
fn teleport_bands(ticks: u32, age: u32) -> [Band; 4] {
    let left = ticks.saturating_sub(age).min(40) as f32 / 40.0;
    TELEPORT_BANDS.map(|[radius, height, lean, turn, alpha]| Band { arc: 360.0, turn, radius, lean, height, alpha: alpha * left })
}

/// The arrival's bands at a tick.
fn arrival_bands(age: u32) -> [Band; 4] {
    ARRIVAL_BANDS.map(|[radius, height, lean]| Band { arc: 360.0, turn: 0.0, radius, lean, height, alpha: band_fade(age) })
}

/// The auras made of two sets of blooming bands.
fn is_bloom(effect: u32) -> bool {
    matches!(effect, 12 | 58 | 367 | COUPLE_CASTING | BLUE_CASTING | 1872 | 1876 | 1902 | 1910 | 1918 | 1923)
}

impl Banded {
    /// A blooming aura's bands at a tick, with their set and the height along each.
    fn bloom_bands(&self, age: u32) -> Vec<(usize, Band, Vec<f32>)> {
        (0..2)
            .flat_map(|set| (0..4).filter_map(move |band| bloom(set, band, self.ticks, age).map(|shape| (set, band, shape))))
            .map(|(set, band, shape)| (set, shape, (0..=BAND_QUADS).map(|s| bloom_height(band, shape.height, age, s)).collect()))
            .collect()
    }

    /// The bands of set `set` of a blooming aura (Aura Blade's, which draws the sets in their own textures).
    fn bands_in_set(&self, age: u32, set: usize) -> Vec<(Band, Vec<f32>)> {
        self.bloom_bands(age).into_iter().filter(|(each, ..)| *each == set).map(|(_, band, heights)| (band, heights)).collect()
    }

    /// Its bands at a tick, each with its height along it.
    fn bands(&self, age: u32) -> Vec<(Band, Vec<f32>)> {
        let heights = |band: usize, height: f32, wave: fn(usize, f32, u32, usize) -> f32| (0..=BAND_QUADS).map(|s| wave(band, height, age, s)).collect();
        let shaped = |bands: &[Band], wave: fn(usize, f32, u32, usize) -> f32| {
            bands.iter().enumerate().map(|(index, band)| (*band, heights(index, band.height, wave))).collect()
        };
        match self.effect {
            // Two pale green upright walls; 313 and 325 add their own ring from the table.
            312 | 313 | 325 => {
                let wall = |radius: f32, turn: f32| Band { arc: 360.0, turn, radius, lean: 90.0, height: 40.0, alpha: band_fade(age) };
                return shaped(&[wall(4.6, self.turns[0]), wall(4.8, self.turns[1])], rising_height);
            }
            // Safety Wall: three nested pink walls; the undead heal: two white ones; both roll their tops in waves.
            315 => {
                let b = age + 1;
                let walls = [(34.0, 3.6, 5500), (37.0, 3.3, 3080), (40.0, 3.0, 3080)];
                let bands: Vec<Band> = walls
                    .iter()
                    .enumerate()
                    .map(|(k, &(height, radius, fade))| Band { arc: 360.0, turn: self.turns[k], radius, lean: 90.0, height, alpha: wall_alpha(b, fade - 1) })
                    .collect();
                return shaped(&bands, rim_height);
            }
            320 => {
                let b = age + 1;
                let [height, radius] = self.extra;
                let wall = |turn: f32, radius: f32| Band { arc: 360.0, turn, radius, lean: 90.0, height, alpha: wall_alpha(b, 29) };
                return shaped(&[wall(self.turns[0], radius), wall(self.turns[1], radius + 0.2)], rim_height);
            }
            304 => return shaped(&teleport_bands(self.ticks, age), full_height),
            // ponytail: the logout stands as the teleport's columns, with the fade.
            314 => return shaped(&teleport_bands(self.ticks, age).map(|band| Band { alpha: band_fade(age), ..band }), rising_height),
            344 => return shaped(&arrival_bands(age), rising_height),
            _ => {}
        }
        if is_bloom(self.effect) {
            self.bloom_bands(age).into_iter().map(|(_, band, heights)| (band, heights)).collect()
        } else {
            // ponytail: earth (56) and poison (59) turn like water, fire and wind.
            (0..4).map(|band| turning(band, self.ticks, age)).enumerate().map(|(band, shape)| (shape, heights(band, shape.height, turning_height))).collect()
        }
    }
}

/// Blessing (effect 42), made in code: four angels trailing one another 25 units above the feet, a flat cyan glow under
/// them over everything, and sparkles; 150 ticks, following the unit.
struct Blessing {
    start: Instant,
    anchor: Anchor,
    /// Each sparkle's heading across the ground (a unit vector), one every 4 ticks.
    sparkles: Vec<[f32; 2]>,
    /// The angels' images now (one list per angel) and the sparkle's, from `advance`.
    angels: [Vec<Placed>; 4],
    sparkle: Vec<Placed>,
}

const BLESSING_TICKS: u32 = 150;
const ANGEL_SPRITE: &str = "data\\sprite\\이팩트\\축복";
const BLESSING_SPARKLE: &str = "data\\sprite\\이팩트\\particle6";
const BLESSING_GLOW: &str = "data\\texture\\effect\\alpha_down.tga";
/// The angels: launched 3 ticks apart at these alphas, 25 units above the feet.
const ANGEL_STARTS: [u32; 4] = [0, 3, 6, 9];
const ANGEL_ALPHAS: [f32; 4] = [200.0, 182.0, 164.0, 146.0];
const ANGEL_RISE: f32 = 25.0;
/// ponytail: the angel's frames 30 ms each; the angels fade out over the last 20 ticks.
const ANGEL_FRAME_MS: f32 = 30.0;
const ANGEL_FADE: u32 = 20;
/// The glow: #20B0E8, 10 units each way from the feet (ponytail: 10 taken as the half width).
const GLOW_COLOR: [u8; 3] = [0x20, 0xB0, 0xE8];
const GLOW_HALF: f32 = 10.0;
/// A sparkle every 4 ticks, living 80.
const SPARKLE_EVERY: u32 = 4;
const SPARKLE_LIFE: u32 = 80;

/// An angel's alpha (0 to 255) at a tick, from its launch: its own alpha, fading over the effect's last ticks.
fn angel_alpha(angel: usize, tick: u32) -> Option<f32> {
    if tick < ANGEL_STARTS[angel] || tick >= BLESSING_TICKS {
        return None;
    }
    let left = (BLESSING_TICKS - tick).min(ANGEL_FADE) as f32 / ANGEL_FADE as f32;
    Some(ANGEL_ALPHAS[angel] * left)
}

/// The glow's alpha (0 to 255) at a tick: up 3.3 a tick to 100, held, down to nothing over the last 30.
fn glow_alpha(tick: u32) -> f32 {
    let rising = (3.3 * tick as f32).min(100.0);
    let left = BLESSING_TICKS.saturating_sub(tick).min(30) as f32 / 30.0;
    rising * left
}

/// The ticks the sparkles start at.
fn sparkle_starts() -> impl Iterator<Item = u32> {
    (0..BLESSING_TICKS).step_by(SPARKLE_EVERY as usize)
}

/// A sparkle's place from the feet and its alpha (0 to 1), `since` ticks after it starts, while it lives.
// ponytail: it drifts out 0.2 a tick and rises 0.1 from 5 up, its alpha pulsing once each 20 ticks.
fn sparkle_place(heading: [f32; 2], since: u32) -> Option<(Vector, f32)> {
    (since < SPARKLE_LIFE).then(|| {
        let out = 0.2 * since as f32;
        let pulse = (since as f32 * std::f32::consts::PI / 20.0).sin().abs();
        ([heading[0] * out, 5.0 + 0.1 * since as f32, heading[1] * out], pulse)
    })
}

/// A status's looping effect made in code: a song's coloured puffs (861, 865, 867, 871, 873, 875, 877) or Steel Body's
/// glints (254), on its unit while the status is on.
struct StatusPrim {
    kind: PrimKind,
    effect: u32,
    start: Instant,
    anchor: Anchor,
    /// Picks every "random" choice (yaws, places, textures) from its own numbers.
    seed: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum PrimKind {
    /// A song's puffs, by colour variant (2 green … 8 grey).
    Puffs(u8),
    Glints,
}

/// The song effects' colour variants.
fn song_variant(effect: u32) -> Option<u8> {
    match effect {
        861 => Some(2),
        865 => Some(3),
        867 => Some(4),
        871 => Some(5),
        873 => Some(6),
        875 => Some(7),
        877 => Some(8),
        _ => None,
    }
}

const PUFF_TEXTURE: &str = "data\\texture\\effect\\freeze_a.bmp";
/// A song's effect lasts 400 ticks: the next status packet plays it again.
const PUFF_TICKS: u32 = 400;
/// The eight puffs' first ticks: two sets of four, 20 apart, the second 10 later.
const PUFF_STARTS: [u32; 8] = [0, 20, 40, 60, 10, 30, 50, 70];
/// Each puff's other channels by its place in its set.
const PUFF_SHADES: [u8; 4] = [105, 5, 55, 155];

/// A small hash for the effects' random choices: the same numbers give the same choice.
fn noise(seed: u32, a: u32, b: u32) -> u32 {
    let mut x = seed ^ a.wrapping_mul(0x9E37_79B1) ^ b.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^ (x >> 16)
}

/// A puff `age` ticks after its start: its size r, alpha (0–100), pulse angle and yaw (degrees); None before it
/// starts or once it has stopped. r starts at 10 and grows ×1.02 a tick; the alpha holds at 100 until r passes 20, then
/// falls 3 a tick; at nothing the puff starts again with a new yaw, but not after age 300.
fn puff_at(seed: u32, puff: u32, age: i64) -> Option<(f32, f32, f32, f32)> {
    if age < 0 {
        return None;
    }
    let (mut r, mut alpha, mut round) = (10.0f32, 100.0f32, 0);
    let pulse = (noise(seed, puff, 1_000) % 360) as f32;
    for tick in 0..age {
        r *= 1.02;
        if r > 20.0 {
            alpha -= 3.0;
        }
        if alpha <= 0.0 {
            if tick + 1 > 300 {
                return None;
            }
            (r, alpha, round) = (10.0, 100.0, round + 1);
        }
    }
    let yaw = (noise(seed, puff, round) % 360) as f32;
    Some((r, alpha, pulse + age as f32, yaw))
}

/// A puff's colour: the variant's full channels at 255 and the others at the puff's shade.
fn puff_color(variant: u8, index: usize) -> [u8; 3] {
    let v = PUFF_SHADES[index % 4];
    match variant {
        2 => [v, 255, v],
        3 => [v, v, 255],
        4 => [255, 255, v],
        5 => [255, v, v],
        6 => [255, v, 255],
        7 => [v, 255, 255],
        8 if index % 4 == 3 => [255, 255, 255],
        8 => [v, v, v],
        _ => [v, v, 255],
    }
}

const GLINT_TEXTURES: [&str; 5] = [
    "data\\texture\\effect\\super1.bmp",
    "data\\texture\\effect\\super2.bmp",
    "data\\texture\\effect\\super3.bmp",
    "data\\texture\\effect\\super4.bmp",
    "data\\texture\\effect\\super5.bmp",
];
/// Steel Body's 16 glints: four sets whose glints start at these alphas.
const GLINT_ALPHAS: [i32; 4] = [70, 130, 190, 250];
/// A glint's half diagonal and the circle it sits on, in units; #6464FF.
const GLINT_RADIUS: f32 = 3.5;
const GLINT_CIRCLE: f32 = 4.0;
const GLINT_COLOR: [u8; 3] = [0x64, 0x64, 0xFF];
/// Sprite pixels to a world unit.
const PIXELS_PER_UNIT: f32 = 7.0;

/// How many sets of four glints an effect has: Explosion Spirits one, Steel Body four.
fn glint_sets(effect: u32) -> u32 {
    if effect == GUMGANG { 1 } else { 4 }
}

/// A glint `age` ticks in: its alpha (0–255), and which round it's on (each round a new place, height and texture).
/// It starts at its set's alpha with a counter of 10; each tick the counter goes up, and the alpha +25 while it's under
/// 10, −12 from there; under 12 it starts a new round at 0.
fn glint_at(first_alpha: i32, age: u32) -> (i32, u32) {
    let (mut counter, mut alpha, mut round) = (10, first_alpha, 0);
    for _ in 0..age {
        counter += 1;
        alpha += if counter < 10 { 25 } else { -12 };
        if alpha < 12 {
            (counter, alpha, round) = (0, 0, round + 1);
        }
    }
    (alpha.max(0), round)
}

#[derive(Default)]
pub struct Effects {
    /// Each unit's facing in degrees: set each frame (Preserve's dome starts from it).
    pub facings: HashMap<Option<u32>, f32>,
    /// How much higher (or lower) a unit's glints sit: set each frame (`glint_rise`).
    pub rises: HashMap<Option<u32>, f32>,
    loaded: HashMap<String, Option<Rc<Loaded>>>,
    playing: Vec<Playing>,
    textures: HashMap<String, Option<Texture>>,
    floating: Vec<Floating>,
    rings: Vec<Ring>,
    /// Sprites by path without extension, with their images decoded so far.
    sprites: HashMap<String, Option<SpriteSet>>,
    animations: Vec<Animation>,
    models: HashMap<String, Option<Rc<Model>>>,
    /// Models standing on skills' ground units: the model, the unit and where.
    standing: Vec<(Rc<Model>, u32, Vector)>,
    /// Camera shakes waiting to start (when, how long), the end of the running shake, and the sign flip.
    quakes: Vec<(Instant, Duration)>,
    quake_until: Option<Instant>,
    quake_flip: bool,
    /// Sounds waiting for their moment: when, the file (under data\wav\), and where.
    sounds: Vec<(Instant, String, Vector)>,
    /// The skill unit type of each ground unit shown, for what it shows when it changes (a trap going off).
    unit_kinds: HashMap<u32, u32>,
    hits: Vec<HitSpark>,
    auras: Vec<Banded>,
    /// The game's hand-made effects (Soul Strike's ghosts, Fire Ball's balls, spikes…), the ghost sprite's layers, the
    /// fire ball's frame by frame, and the camera's turn in degrees (fire balls turn with it).
    prims: crate::prims::Prims,
    ghost: Vec<Placed>,
    /// The first jobs' star sprites' layers (one frame each), by path.
    stars: HashMap<&'static str, Vec<Placed>>,
    /// Holy Water's bottle sprite's frames (1 to 41), plain white, by frame.
    holy_frames: HashMap<usize, Vec<Placed>>,
    /// Water Ball's three pictures' layers.
    water_frames: HashMap<u32, Vec<Placed>>,
    /// The claw slash's layers by action and frame.
    claw_frames: HashMap<(&'static str, usize, usize), Vec<Placed>>,
    /// Sight Rasher's sprites' frames, plain white, by sprite and frame.
    sight_frames: HashMap<(&'static str, usize), Vec<Placed>>,
    /// Dark Strike's red ghost sprite's layers.
    red_ghost: Vec<Placed>,
    fire_ball: Vec<Vec<Placed>>,
    /// Spear Boomerang's spear, its first frame's layers.
    spear: Vec<Placed>,
    yaw: f32,
    /// Fire Bolt's spark sprite's layers, frame by frame.
    spark_frames: Vec<Vec<Placed>>,
    /// /effect off: only the force-drawn effects show (`force_drawn`), the rest play on unseen.
    pub only_forced: bool,
    /// /mineffect: the lighter effects (`min_file`, the hit spark's ring only).
    pub min: bool,
    /// On ecl_fild01, where Shield Boomerang's alpha is cut to 100.
    pub dim: bool,
    /// The camera's pitch in degrees, which tips the thrown items' pictures.
    pub pitch: f32,
    /// Bows' arrows, and the arrow sprite's layers by direction.
    arrows: crate::arrows::Arrows,
    arrow: Vec<Vec<Placed>>,
    blessings: Vec<Blessing>,
    status_prims: Vec<StatusPrim>,
}

/// The effects drawn even with /effect off (the data's ForceRenderEffect list): warp portals, Safety Wall, the
/// level-up angels, the refine results…
pub fn force_drawn(effect: u32) -> bool {
    matches!(
        effect,
        35 | 36 | 60 | 114 | 137 | 138 | 154 | 155 | 159 | 160 | 164 | 166 | 203 | 228 | 261 | 295 | 296 | 315 | 316 | 317 | 321 | 332 | 334 | 338
            | 357 | 371 | 419 | 424 | 426 | 445 | 446 | 449 | 475..=484 | 542 | 544 | 548 | 549 | 582 | 629 | 655 | 656 | 718 | 735..=738
            | 758..=767 | 788 | 833 | 837 | 1072 | 1110 | 1135 | 1186..=1191 | 1214..=1217 | 1225 | 1262..=1265 | 1607..=1609 | 1617..=1619
            | 1869..=1871 | 2371 | 2372 | 2379..=2381 | 2383..=2389 | 2395..=2399 | 2401 | 2402
    )
}

/// The STR an effect plays instead under /mineffect (under data\texture\effect\, without .str).
pub fn min_file(effect: u32) -> Option<&'static str> {
    Some(match effect {
        10 => "memor_min",
        41 | 717 => "jong_mini",
        66 => "cure_min",
        75 => "gloria_min",
        76 => "magnificat_min",
        77 => "resurrection_min",
        85 => "lexaeterna_min",
        88 => "suffragium_min",
        103 => "WeaponPerfection_min",
        104 => "maximize_min",
        112 => "kyrie_min",
        _ => return None,
    })
}

/// Whether /mineffect is on: the player's own choice, or forced by the map (ZC_MAPPROPERTY_R2's simple-effect flag:
/// siege maps and those marked so), each map property starting again from the player's choice.
pub fn min_effect(choice: bool, map_flags: Option<u32>) -> bool {
    choice || map_flags.is_some_and(|flags| flags & rustnarok_client::net::MAP_SIMPLE_EFFECT != 0)
}

/// A whole number in `from..=to`.
pub fn pick(from: u32, to: u32) -> u32 {
    (random(from as f32, to as f32 + 1.0) as u32).min(to)
}

/// A random number in `from..=to`.
fn random(from: f32, to: f32) -> f32 {
    let noise = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().subsec_nanos();
    // Scramble the clock so draws made together differ.
    let mixed = (noise ^ (noise >> 13)).wrapping_mul(0x9E37_79B1) ^ RANDOM_SALT.fetch_add(0x632B_E5AB, std::sync::atomic::Ordering::Relaxed);
    from + (mixed % 10_000) as f32 / 10_000.0 * (to - from)
}

static RANDOM_SALT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

impl Floating {
    /// Makes the random choices of `parameters` for the duplicate numbered `copy`, starting at `start`.
    fn new(parameters: &HashMap<String, String>, effect: u32, copy: u32, start: Instant, anchor: Anchor) -> Option<Self> {
        let number = |name: &str| parameters.get(name).and_then(|value| value.parse::<f32>().ok());
        let flag = |name: &str| parameters.get(name).is_some_and(|value| value == "true");
        let range = |name: &str| -> Option<(f32, f32)> {
            let (from, to) = parameters.get(name)?.split_once(',')?;
            Some((from.trim().parse().ok()?, to.trim().parse().ok()?))
        };
        // ponytail: missiles between the caster and the target (Soul Strike, Fire Ball) need the caster's place, which
        // effects aren't given yet; skipped rather than drawn standing on the target.
        if flag("toSrc") || flag("fromSrc") {
            return None;
        }
        let textures: Vec<String> = match parameters.get("fileList").or_else(|| parameters.get("file")) {
            Some(list) => list.split(',').map(|file| format!("data\\texture\\{}", file.replace('/', "\\"))).collect(),
            None => Vec::new(),
        };
        // Without textures, a sprite: under data\sprite\이팩트\ or anywhere.
        let sprite = match (parameters.get("absoluteSpriteName"), parameters.get("spriteName")) {
            _ if !textures.is_empty() => None,
            (Some(path), _) => Some(path.replace('/', "\\")),
            (None, Some(name)) => Some(format!("data\\sprite\\이팩트\\{name}")),
            (None, None) => return None,
        };
        // Where it goes, axis by axis: fixed, random once, or random at each end around a middle.
        let axis = |name: &str| -> ([f32; 2], bool) {
            let mut ends = [number(&format!("pos{name}Start")).unwrap_or(0.0), number(&format!("pos{name}End")).unwrap_or(0.0)];
            if let Some(both) = number(&format!("pos{name}")) {
                ends = [both; 2];
            }
            if let Some(spread) = number(&format!("pos{name}Rand")) {
                ends = [random(-spread, spread); 2];
            }
            if let Some(spread) = number(&format!("pos{name}RandDiff")) {
                ends = [random(-spread, spread), random(-spread, spread)];
            }
            for (end, which) in [(0, "Start"), (1, "End")] {
                if let Some(spread) = number(&format!("pos{name}{which}Rand")) {
                    let middle = number(&format!("pos{name}{which}RandMiddle")).unwrap_or(0.0);
                    ends[end] = random(middle - spread, middle + spread);
                }
            }
            (ends, flag(&format!("pos{name}Smooth")))
        };
        let (x, y, z) = (axis("x"), axis("y"), axis("z"));
        let copy_f = copy as f32;
        let base = number("size").map_or(1.0, |size| size + number("sizeDelta").unwrap_or(0.0) * copy_f);
        let mut size = [[base; 2]; 2];
        if let Some(value) = number("sizeStart") {
            size[0][0] = value;
            size[1][0] = value;
        }
        if let Some(value) = number("sizeEnd") {
            size[0][1] = value;
            size[1][1] = value;
        }
        for (dimension, letter) in [(0, "X"), (1, "Y")] {
            if let Some(value) = number(&format!("size{letter}")) {
                size[dimension] = [value; 2];
            }
            for (end, which) in [(0, "Start"), (1, "End")] {
                if let Some(value) = number(&format!("size{which}{letter}")) {
                    size[dimension][end] = value;
                }
                if let Some((from, to)) = range(&format!("sizeRand{which}{letter}")) {
                    size[dimension][end] = random(from, to);
                }
            }
            if let Some(spread) = number(&format!("sizeRand{letter}")) {
                let middle = number(&format!("sizeRand{letter}Middle")).unwrap_or(100.0);
                size[dimension] = [random(middle - spread, middle + spread); 2];
            }
        }
        if let Some(spread) = number("sizeRand") {
            size = [[base + random(-spread, spread); 2]; 2];
        }
        let duration = range("durationRand").map_or_else(|| number("duration").unwrap_or(1000.0), |(from, to)| random(from, to));
        let start = start + std::time::Duration::from_secs_f32(number("delayStart").unwrap_or(0.0).max(0.0) / 1000.0);
        let first_angle = range("angleRand").map_or_else(|| number("angle").unwrap_or(0.0), |(from, to)| random(from, to));
        let last_angle = if flag("rotate") { number("toAngle").unwrap_or(0.0) } else { first_angle };
        let alpha = (number("alphaMax").unwrap_or(1.0) + number("alphaMaxDelta").unwrap_or(0.0) * copy_f).clamp(0.0, 1.0);
        Some(Self {
            textures,
            frame_ms: number("frameDelay").unwrap_or(10.0).max(1.0),
            sprite,
            play_sprite: flag("playSprite"),
            placed: Vec::new(),
            again: flag("repeat"),
            start,
            end: start + std::time::Duration::from_secs_f32(duration.max(1.0) / 1000.0),
            anchor,
            position: [x.0, y.0, z.0],
            smooth: [x.1, y.1, z.1],
            size,
            size_smooth: flag("sizeSmooth"),
            alpha,
            fade_in: flag("fadeIn"),
            fade_out: flag("fadeOut") || flag("fade"),
            color: ["red", "green", "blue"].map(|channel| number(channel).filter(|value| *value > 0.0).unwrap_or(1.0)),
            angle: [first_angle, last_angle],
            ray: flag("circlePattern"),
            additive: number("blendMode") == Some(2.0),
            effect,
        })
    }
}

/// Skills whose splash's other targets still get the whole hit (hit effects, the caster's pose): Napalm Beat, Arrow
/// Shower, Asura Strike's combo finish, a Taekwon kick, and one newer one.
// ponytail: the game's list (0xcca5b0) has a few more of the newer jobs'.
pub const SPLASH_FULL: [u16; 5] = [11, 47, 273, 413, 732];

/// The hit effect on each hit of damage from a skill's ground unit: Fire Wall's FIREHIT, Magnus Exorcismus's,
/// Lord of Vermilion's WINDHIT, Storm Gust's and its monster version's COLDHIT, else EF_HIT1 below skill 493.
// ponytail: the game's table goes on for higher ids (428–430, 0xa1b, 0x1461, 5219–5371); EF_HIT1 stands in below 493 only.
pub fn unit_hit_effect(skill: u16) -> Option<u32> {
    match skill {
        18 => Some(49),
        79 => Some(152),
        85 => Some(52),
        89 | 723 => Some(51),
        _ if skill < 493 => Some(0),
        _ => None,
    }
}

/// EF_FROSTDIVER: Frost Diver's line of ice spikes from the caster to the target; EF_FROSTDIVER2: the spikes round the
/// target with its first hit (`prims`).
const FROST_DIVER: u32 = 27;
pub const FROST_DIVER_HIT: u32 = 28;

/// EF_COLDHIT: Cold Bolt's, Frost Diver's and Storm Gust's hit, ice shards and smoke drawn on the screen (`prims`).
pub const COLD_HIT: u32 = 51;

/// EF_NAPALMBEAT: explosion puffs flat on the screen round the target (`prims::napalm`).
pub const NAPALM_BEAT: u32 = 32;

/// EF_HALLUTIME: white glints round a copy (`prims::sparkle`).
pub const HALLUTIME: u32 = crate::prims::sparkle::EF_HALLUTIME;

/// EF_QUAGMIRE: only its sound is handled in code (one time in 12, quietly).
const QUAGMIRE: u32 = 95;
/// Whether a Quagmire cell's launch makes its sound, by one draw of 12: one time in 12.
pub fn quagmire_heard(draw: u32) -> bool {
    draw == 0
}

/// EF_STORMGUST: its STR (`stormgust`, or `storm_min` for the ground skills under /mineffect, effect 708) and the ice crystals.
const STORM_GUST: u32 = 89;
/// EF_SIGHTRASHER: its 64 sprites and ring, made in code (`prims::sight`); it ends the caster's Sight (22).
const SIGHT_RASHER: u32 = 62;
const SIGHT: u32 = 22;
/// EF_SIGNUMCRUCIS: its STR, and a burst half a second in (`prims::burst`).
const SIGNUM_CRUCIS: u32 = 40;
/// EF_BASH: the burst on a damaging skill's caster (`prims::burst`); none at all under /mineffect.
const BASH: u32 = 16;
/// EF_TURNUNDEAD: its burst round the priest and the ring on the ground (`prims::burst`).
const TURN_UNDEAD: u32 = 82;
/// EF_SONICBLOW: Sonic Blow's ring, flat on the screen round the caster (`prims::burst`).
const SONIC_BLOW: u32 = 121;

/// EF_SOULSTRIKE: ghosts from the caster to the target, a hit each (`prims`).
const SOUL_STRIKE: u32 = 15;
/// EF_FIREBALL, EF_YUFITEL (Jupitel Thunder) and EF_GRIMTOOTH (`prims`).
const FIRE_BALL: u32 = 24;
/// Whether the server sends this effect (ZC_NOTIFY_EFFECT2) with no target, so it flies from its unit at the map's
/// centre: Fire Ball's and Frost Diver's.
pub fn aimed_at_centre(effect: u32) -> bool {
    matches!(effect, FIRE_BALL | FROST_DIVER | JUPITEL | SPHERE | STRAIGHT_SPHERE | POTION_PITCHER | BERSERK_PITCHER | SPORE_EXPLOSION)
}

/// The effect that throws the item of a skill used (ZC_USE_SKILL): Potion Pitcher's, Berserk Pitcher's, Spore
/// Explosion's.
pub fn thrown_item(skill: u16) -> Option<u32> {
    match skill {
        231 => Some(POTION_PITCHER),
        446 => Some(BERSERK_PITCHER),
        2481 => Some(SPORE_EXPLOSION),
        _ => None,
    }
}

/// Storm Gust's ground skills (WZ_STORMGUST and NPC_STORMGUST2) play the light effect 708 under /mineffect.
pub fn storm_gust_effect(skill: u16, effect: u32, min: bool) -> u32 {
    if effect == STORM_GUST && min && matches!(skill, 89 | 723) { 708 } else { effect }
}

/// The hit count of an effect the packet gave one (`prims::tanji::hit_count`): the game's rule for Finger Offensive's
/// sphere, Dark Strike's balls and Huujin's blades.
pub fn hit_count(effect: u32, count: u16, kind: u8) -> u16 {
    if matches!(effect, SPHERE | DARK_STRIKE | HUUJIN) { crate::prims::tanji::hit_count(count, kind) } else { count }
}

/// EF_SPEARBMR: Spear Boomerang's four spears from the caster to the target (`prims::balls`).
const SPEAR_BOOMERANG: u32 = 80;
/// EF_BLITZBEAT: the streaks and the ring on Blitz Beat's target, made in code (`prims::blitz`).
pub const BLITZ_BEAT: u32 = 115;
/// EF_TANJI (265, from Finger Offensive and Duple Light), EF_DARKSTRIKE (451, Dark Strike's red balls) and EF_HUUJIN
/// (621, Huujin's wind blades), made in code; 412 is the straight sphere that only the server sends.
const SPHERE: u32 = 265;
const STRAIGHT_SPHERE: u32 = 412;
const DARK_STRIKE: u32 = 451;
const HUUJIN: u32 = 621;
/// EF_POTIONPITCHER, EF_BERSERKPITCHER and EF_SPOREEXPLOSION: the item thrown at the target (`prims::throw`).
const POTION_PITCHER: u32 = 299;
const BERSERK_PITCHER: u32 = 541;
const SPORE_EXPLOSION: u32 = 914;
/// EF_SHIELDBOOMERANG: Shield Boomerang's shield, out to the target, round and back past the caster (`prims::boomerang`).
const SHIELD_BOOMERANG: u32 = 249;
/// EF_WATERBALL2: Water Ball's ball, one a damage packet (`prims::waterball`).
const WATER_BALL: u32 = 117;
/// EF_OVERTHRUST: the ring of Sonic Blow's, on the unit (`prims::burst`).
const OVER_THRUST: u32 = 128;
/// EF_FIREPILLARON's funnels and EF_VENOMDUST2's puffs stand with their skill units; EF_GUMGANG is the monk's four glints;
/// Berserk Pitcher's tint, cylinder and streaks, Defender's walls and Grand Cross's cross of light (`prims::walls`).
/// EF_INTIMIDATE: its shards stay in the world where they burst (`prims::shards`).
const INTIMIDATE: u32 = 227;
/// EF_SHIELDCHARGE's anchor; EF_PROVIDENCE's tubes, EF_REFLECTSHIELD's yellow walls and EF_ABSORBSPIRITS' curtains (`prims::walls`).
const SHIELD_CHARGE: u32 = 246;
const PROVIDENCE: u32 = 248;
const REFLECT_SHIELD: u32 = 252;
const ABSORB_SPIRITS: u32 = 253;
/// Investigate's sparks and claw slash and Asura Strike's streaks (`prims::monk`); Steel Body's ribbons (`prims::walls`).
const INVESTIGATE_SPARKS: u32 = 264;
/// Steal Coin's coins (`prims::shards`), Back Stab's sparks and Raid's streaks (`prims::monk`), Chemical Protection's star (`prims::star`).
const STEAL_COIN: u32 = 274;
/// Magnus' and Sanctuary's boxes, Triple Attack's streaks, Slim Pitcher's potion and Full Protection's strobe (`prims::ground`).
/// Twilight Alchemy's icons, Shrink's flashes, Close Confine's blinking, Pang Voice's and Wink of Charm's sprites and the performers'
/// cones of bubbles (`prims::songs`, `prims::monk`).
/// Ganbantein's crescents (`prims::walls`), Spiral Pierce's cones, Soul Breaker's slashes, Pressure's cross and Martyr's Reckoning's rays
/// (`prims::knight`).
/// Gospel's cross, Soul Drain's and Indulge's sparks, the colours they and Magic Crasher give a body, Soul Change's tube
/// (`prims::transcend`).
/// Soul Change's glow, True Sight's rays, Falcon Assault's spin, the shot lines, Wind Walk's arcs and Reject Sword's blinking
/// (`prims::sniper`).
/// Moonlit Water Mill's crescents and Spider Web's web (`prims::dancer`), Marionette's tube (`prims::transcend`), Head Crush's and
/// Joint Beat's rays (`prims::knight`) and Mind Breaker's flicker.
/// Fog Wall's boxes, Full Strip's pieces, Preserve's dome and the flickers of Enchant Deadly Poison and Spiral Pierce's cast
/// (`prims::guard`); Meteor Assault's slashes (`prims::knight`).
/// Memorize's and Double Casting's flickers and Chase Walk's cast (`prims::guard`), Cart Termination's dots and Shield Chain's shields
/// (`prims::spray`), Gravitation Field's spikes (`prims::stone`).
const MEMORIZE: u32 = 505;
const CART_TERMINATION: u32 = 518;
const SHIELD_CHAIN: u32 = 520;
const DOUBLE_CAST: u32 = 521;
const GRAVITATION: u32 = 522;
const CHASE_WALK_CAST: u32 = 1195;
/// Poem of Netherworld's blot (a held sprite), the Dark Illusion shake, Reverberation's red tornado, the head swing, the cast spin,
/// the green flicker and Hovering's blue glow (`prims::brawl`, `prims::sparkle`, `prims::guard`).
const NPC_STOP: u32 = 453;
/// Poison Smoke's cell, Phantom Menace's crescents, the Mechanic's cast spin, and Acceleration's and Shape Shift's pulse.
const POISON_SMOKE_CELL: u32 = 772;
const PHANTOM_RIBBONS: u32 = 773;
const CAST_SPIN2: u32 = 775;
const ACCELERATION: u32 = 777;
const SHAPE_SHIFT: u32 = 779;
/// Detonator's flush, Release's spheres, Oratio's status circle, Enchant Blade's tint, the poisons' magenta and Weapon Crush's weapons.
const CONCENTRATION2: u32 = 750;
const RELEASE: u32 = 751;
const MAGIC_CIRCLE: u32 = 755;
const POTION_BERSERK2: u32 = 756;
const PURPLE_BODY: u32 = 768;
const WEAPON_CRUSH: u32 = 770;
/// Renovatio's shafts, Hundred Spear's flashes, Oratio's ring (Detecting's) and Camouflage's translucent unit.
const RENOVATIO: u32 = 719;
const SPEAR_FLASH: u32 = 723;
const ORATIO: u32 = 725;
const CLOAKING: u32 = 744;
/// Sky Net Blow's red flash and late shake.
const RED_HIT: u32 = 548;
const SKY_NET_SHAKE: u32 = 550;
const QUAKE_BODY: u32 = 458;
pub const REVERBERATION: u32 = 464;
const SPINED_BODY: u32 = 466;
const CAST_SPIN: u32 = 501;
const GREEN_BODY: u32 = 538;
const HATED: u32 = 543;
/// The status Poem of Netherworld gives, which its blot stands for.
const POEM_STATUS: u16 = 433;
/// Self-Destruction's streaks, Spirit Recovery's and Blood Sucker's drops, Storm Blast's ribbons and hopping lines, Rage Burst's rays,
/// Sound of Destruction's copies, Windmill's tornado and Dark Illusion's lines (`prims::brawl`).
const SELF_DESTRUCTION: u32 = 183;
const BLOOD_DRAIN: u32 = 216;
const STORM_BLAST: u32 = 263;
const HIT_LINE: u32 = 330;
const RAGE_BURST: u32 = 377;
const FOUR_WAY_BODY: u32 = 425;
const STORM_KICK: u32 = 435;
const CHEMICAL: u32 = 439;
const FOG_WALL: u32 = 405;
const METEOR_ASSAULT: u32 = 409;
const ENCHANT_DEADLY_POISON: u32 = 493;
const FULL_STRIP: u32 = 495;
const PRESERVE: u32 = 496;
const PIERCE_CAST: u32 = 502;
const MOONLIT: u32 = 394;
const MARIONETTE_LINK: u32 = 395;
const HEAD_CRUSH: u32 = 399;
const JOINT_BEAT: u32 = 400;
const MIND_BREAKER: u32 = 403;
const SPIDER_WEB: u32 = 404;
const SOUL_CHANGE_GLOW: u32 = 385;
const TRUE_SIGHT: u32 = 386;
const FALCON_ASSAULT: u32 = 387;
const SHARP_SHOOTING: u32 = 388;
const WIND_WALK: u32 = 389;
const REJECT_SWORD: u32 = 392;
const ARROW_VULCAN: u32 = 393;
const GOSPEL: u32 = 370;
const SOUL_DRAIN: u32 = 378;
const DRAIN_TARGET: u32 = 379;
const MAGIC_CRASHER: u32 = 380;
const INDULGE: u32 = 383;
const LINE_LINK: u32 = 384;
const GANBANTEIN_FX: u32 = 223;
const SPIRAL_PIERCE: u32 = 339;
const SOUL_BREAKER: u32 = 361;
const PRESSURE: u32 = 365;
const MARTYR: u32 = 366;
const SHRINK: u32 = 599;
const CLOSE_CONFINE: u32 = 602;
const PANG_VOICE: u32 = 606;
const WINK_OF_CHARM: u32 = 607;
const BUBBLE_CONE: u32 = 840;
/// The coloured cones of the other songs (841 to 845), Adaptation's and Eternal Chaos' flicker and streaks.
const LAST_BUBBLE_CONE: u32 = 845;
const ADAPTATION: u32 = 848;
const ETERNAL_CHAOS: u32 = 864;
const MAGNUS: u32 = 318;
const SANCTUARY: u32 = 319;
const TRIPLE_ATTACK: u32 = 329;
const SLIM_PITCHER: u32 = 497;
const CHEMICAL_BODY: u32 = 500;
const BACK_STAB: u32 = 275;
const RAID: u32 = 276;
const CHEMICAL_PROTECTION: u32 = 300;
const CLAW_SLASH: u32 = 267;
const ASURA_STRIKE: u32 = 266;
const RIBBONS: u32 = 261;
const FIRE_PILLAR: u32 = 138;
const VENOM_DUST: u32 = 171;
const GUMGANG: u32 = 203;
const BERSERK: u32 = 220;
const DEFENDER: u32 = 222;
const GRAND_CROSS: u32 = 226;
/// Two body colours multiplied.
pub fn mix_tints(a: Option<[u8; 3]>, b: Option<[u8; 3]>) -> Option<[u8; 3]> {
    match (a, b) {
        (Some(a), Some(b)) => Some(std::array::from_fn(|k| (u16::from(a[k]) * u16::from(b[k]) / 255) as u8)),
        (tint, other) => tint.or(other),
    }
}

/// Where a sound that is heard at full volume wherever it plays is made.
pub const LISTENER: Vector = [f32::NAN; 3];
const JUPITEL: u32 = 93;
const GRIMTOOTH: u32 = 123;
/// Effects launched on the caster and aimed at where the target stands, made in code (`aimed`).
pub const AIMED: [u32; 30] = [
    RELEASE,
    SPEAR_FLASH,
    BLOOD_DRAIN,
    CHEMICAL,
    MARIONETTE_LINK,
    SHARP_SHOOTING,
    ARROW_VULCAN,
    SOUL_DRAIN,
    LINE_LINK,
    SOUL_BREAKER,
    PANG_VOICE,
    WINK_OF_CHARM,
    TRIPLE_ATTACK,
    BACK_STAB,
    INVESTIGATE_SPARKS,
    CLAW_SLASH,
    WATER_BALL,
    SOUL_STRIKE,
    FIRE_BALL,
    FROST_DIVER,
    JUPITEL,
    GRIMTOOTH,
    SPEAR_BOOMERANG,
    SHIELD_BOOMERANG,
    SPHERE,
    DARK_STRIKE,
    HUUJIN,
    POTION_PITCHER,
    BERSERK_PITCHER,
    SPORE_EXPLOSION,
];
/// EF_EARTHSPIKE and EF_HEAVENSDRIVE: stone spikes where they play (`prims`).
const EARTH_SPIKE: u32 = 79;
const HEAVENS_DRIVE: u32 = 142;
/// EF_ICEWALL: the ice spikes of a wall's cell, standing with their unit (`prims::stone`).
const ICE_WALL: u32 = 74;

/// Cold Bolt's and Fire Bolt's falling arrows (EF_ICEARROW, EF_FIREARROW), drawn in code (`prims::bolts`).
const BOLTS: [u32; 2] = [26, FIRE_BOLT];
const FIRE_BOLT: u32 = 31;

/// When a bolt with `hits` hits drops its arrows, from the moment it's played: one an hit (at most 50), the first
/// 12 ticks of 16 ms in and one every 10 ticks after. Empty for an effect that isn't a bolt.
pub fn bolt_drops(id: u32, hits: u16) -> Vec<Duration> {
    if !BOLTS.contains(&id) {
        return Vec::new();
    }
    (0..u64::from(hits.clamp(1, 50))).map(|drop| Duration::from_millis(16 * (12 + 10 * drop))).collect()
}

/// From `ends[0]` to `ends[1]` at `steps` (0 to 100): straight, or easing out (a log curve).
fn between(ends: [f32; 2], steps: f32, smooth: bool) -> f32 {
    let share = if smooth { (steps * 0.09 + 1.0).log10() } else { steps / 100.0 };
    ends[0] + (ends[1] - ends[0]) * share
}

impl Effects {
    /// Plays effect `id` from `now` on `target` at `at`; parts that don't follow units stay there.
    pub fn play(&mut self, gpu: &Gpu, grf: &Grf, table: &Table, id: u32, target: Target, at: Vector, now: Instant) {
        if id == BASH && self.min {
            return;
        }
        // Quagmire sounds one time in 12, from 100 units under the ground (so quietly); each of its cells launches its own.
        if id == QUAGMIRE {
            if quagmire_heard(pick(0, 11)) {
                self.play_sounds(table, id, [at[0], at[1] - 100.0, at[2]], now);
            }
        } else {
            self.play_sounds(table, id, at, now);
        }
        self.show(gpu, grf, table, id, target, at, now);
    }

    /// A cast's aura `id` on `unit` at `at`: Bash's burst (16) lasts the cast (`cast_ms`, so one under 640 ms shows only
    /// its sound), any other effect plays as it is.
    #[allow(clippy::too_many_arguments)]
    pub fn play_cast_aura(&mut self, gpu: &Gpu, grf: &Grf, table: &Table, id: u32, unit: Option<u32>, at: Vector, cast_ms: u32, now: Instant) {
        // Poison Smoke's cast flickers the caster magenta; one with no cast time shows nothing.
        if id == PURPLE_BODY {
            if cast_ms > 0 {
                self.play_sounds(table, id, at, now);
                self.prims.guard.purple(unit, now);
            }
            return;
        }
        // Spirit Recovery's cast throws three drops round the caster; one with no cast time shows nothing.
        if id == BLOOD_DRAIN {
            if cast_ms > 0 {
                self.sprites.entry(crate::prims::GHOST_SPRITE.to_owned()).or_insert_with(|| SpriteSet::at(grf, crate::prims::GHOST_SPRITE));
                self.prims.blood_drain(at, at, now);
            }
            return;
        }
        // Chase Walk's cast spins the caster.
        if matches!(id, CHASE_WALK_CAST | CAST_SPIN | CAST_SPIN2) {
            self.play_sounds(table, id, at, now);
            self.prims.guard.chase_walk(unit, cast_ms / crate::prims::TICK.as_millis() as u32, now);
            return;
        }
        // Spiral Pierce's cast: the caster glows and flickers yellow for the cast.
        if id == PIERCE_CAST {
            self.play_sounds(table, id, at, now);
            self.prims.guard.pierce(unit, cast_ms / crate::prims::TICK.as_millis() as u32, now);
            return;
        }
        // Twilight Alchemy: 320 icons round the own player for the cast.
        if matches!(id, 573..=575) {
            let pictures: &[&str] = match id {
                573 => &[songs::HERB],
                574 => &[songs::WHITE_SLIM_POTION],
                _ => &songs::MIXED,
            };
            for path in pictures {
                self.textures.entry((*path).to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, false).ok());
            }
            self.play_sounds(table, id, at, now);
            self.prims.songs.twilight(id, cast_ms / crate::prims::TICK.as_millis() as u32, now, |n| pick(0, n.saturating_sub(1)));
            return;
        }
        if id != BASH {
            return self.play(gpu, grf, table, id, Target::Unit(unit), at, now);
        }
        if self.min {
            return;
        }
        self.play_sounds(table, id, at, now);
        for path in [burst::DISC, burst::RAY, burst::RING] {
            self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok());
        }
        let length = cast_ms / crate::prims::TICK.as_millis() as u32;
        self.prims.bursts.push(burst::Burst::bash(unit, now, length, |n| pick(0, n.saturating_sub(1))));
    }

    /// Plays effect `id` told how many hits it has (`hits`): a bolt drops that many arrows (`bolt_drops`), anything
    /// else plays once.
    #[allow(clippy::too_many_arguments)]
    pub fn play_hits(&mut self, gpu: &Gpu, grf: &Grf, table: &Table, id: u32, target: Target, at: Vector, now: Instant, hits: u16) {
        let drops = bolt_drops(id, hits);
        if drops.is_empty() {
            return self.play(gpu, grf, table, id, target, at, now);
        }
        // One sound, as the first arrow falls.
        self.play_sounds(table, id, at, now + drops[0]);
        if let Target::Unit(unit) = target {
            self.bolt(gpu, grf, id, unit, hits, now);
        }
    }

    /// Blitz Beat's effect (115) on a unit (None = the player) facing `facing` degrees: ten streaks, and a ring if
    /// `rings` is 1 (the hit effect's; the server's own 115 has none).
    pub fn blitz_beat(&mut self, gpu: &Gpu, grf: &Grf, unit: Option<u32>, facing: f32, rings: u32, now: Instant) {
        for path in [blitz::STREAK, blitz::RING] {
            self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok());
        }
        self.prims.blitzes.push(blitz::Blitz::new(unit, facing, rings, now, |n| pick(0, n.saturating_sub(1))));
    }

    /// The texture of a Sage ground look or loop.
    fn load_sage(&mut self, gpu: &Gpu, grf: &Grf, id: u32) {
        if let Some(&(_, path)) = sage::LOOKS.iter().chain(&sage::LOOPS).find(|(each, _)| *each == id) {
            self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok());
        }
    }

    /// A bolt (26 or 31) with `hits` arrows on a unit: what it needs loaded, then its volley.
    fn bolt(&mut self, gpu: &Gpu, grf: &Grf, id: u32, unit: Option<u32>, hits: u16, now: Instant) {
        let fire = id == FIRE_BOLT;
        let textures: Vec<&str> =
            if fire { bolts::FIRE_TEXTURES.iter().copied().chain([bolts::RING_YELLOW]).collect() } else { vec![bolts::ICE_ARROW, bolts::RING_BLUE] };
        for path in textures {
            self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok());
        }
        if fire {
            self.sprites.entry(bolts::SPARK_SPRITE.to_owned()).or_insert_with(|| SpriteSet::at(grf, bolts::SPARK_SPRITE));
        }
        self.prims.volleys.push(bolts::Volley::new(unit, fire, hits, now, |n| pick(0, n.saturating_sub(1))));
    }

    fn play_sounds(&mut self, table: &Table, id: u32, at: Vector, now: Instant) {
        for sound in table.sounds.get(&id).into_iter().flatten() {
            let file = match sound.random {
                Some((from, to)) => sound.file.replace("%d", &pick(from, to).to_string()),
                None => sound.file.clone(),
            };
            self.sounds.push((now + Duration::from_millis(sound.delay_ms), format!("{file}.wav"), if sound.listener { LISTENER } else { at }));
        }
    }

    /// What effect `id` draws, from `now` on.
    #[allow(clippy::too_many_arguments)]
    fn show(&mut self, gpu: &Gpu, grf: &Grf, table: &Table, id: u32, target: Target, at: Vector, now: Instant) {
        // Oratio's ring is Detecting's.
        if id == ORATIO {
            return self.show(gpu, grf, table, first::DETECTING, target, at, now);
        }
        // Poem of Netherworld's blot: one to a unit, standing for its status however it is played.
        if let (NPC_STOP, Target::Unit(unit) | Target::Status(unit, _)) = (id, target) {
            self.animations.retain(|animation| {
                !(animation.effect == NPC_STOP && matches!(animation.anchor, Anchor::Unit(each) | Anchor::Status(each, _) if each == unit))
            });
            if matches!(target, Target::Unit(_)) {
                return self.show(gpu, grf, table, id, Target::Status(unit, POEM_STATUS), at, now);
            }
        }
        // The Sage's ground looks, drawn in code at the cell.
        if sage::LOOKS.iter().any(|&(look, _)| look == id) {
            self.load_sage(gpu, grf, id);
            self.prims.sage.look(id, at, now);
        }
        // The first jobs' small heal, Endure, Magnum Break, Steal, Detoxify and Decrease AGI, built in code (a status plays it once).
        // (Flasher's and Remove Trap's play at a trap's cell.)
        let first_unit = match target {
            Target::Unit(unit) | Target::Status(unit, _) => Some(unit),
            _ if matches!(id, first::FLASHER_FX | first::REMOVE_TRAP | first::DETECTING) => Some(None),
            _ => None,
        };
        if let (true, Some(unit)) = (first::makes(id), first_unit) {
            for path in [
                first::ALPHA_DOWN,
                first::ENDURE,
                first::RING_YELLOW,
                first::DOME,
                first::SLOW,
                first::STREAK,
                first::DEX_AGI_TEXT,
                first::PIKAPIKA,
                first::THUNDER_CENTER,
                first::POK1,
                first::POK3,
                first::FASHASHA,
            ] {
                self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok());
            }
            for path in [first::STAR, first::GOLD_STAR, first::CYAN_STAR, first::POISON_STAR, first::HOLY_WATER_SPRITE] {
                self.sprites.entry(path.to_owned()).or_insert_with(|| SpriteSet::at(grf, path));
            }
            self.prims.first.play(id, unit, at, now, self.min, |n| pick(0, n.saturating_sub(1)));
        }
        // Adrenaline Rush's rays follow its unit on the screen; Flasher's burst from the trap's cell stays put.
        if matches!(id, first::ADRENALINE_RUSH | first::FLASHER_FX) {
            for path in [burst::DISC, burst::RAY, burst::RING] {
                self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok());
            }
            let rand = |n: u32| pick(0, n.saturating_sub(1));
            match (id, target) {
                (first::ADRENALINE_RUSH, Target::Unit(unit)) => self.prims.bursts.push(burst::Burst::adrenaline(unit, now, 300, rand)),
                (first::FLASHER_FX, _) => self.prims.bursts.push(burst::Burst::flasher(at, now, 250, rand)),
                _ => {}
            }
        }
        // Storm Gust's ice crystals, not under /mineffect.
        if id == STORM_GUST && !self.min {
            self.textures.entry(crate::prims::ICE_TEXTURE.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, crate::prims::ICE_TEXTURE, true).ok());
            self.prims.stone.storm_gust(at, now, |n| pick(0, n.saturating_sub(1)));
        }
        // Sight Rasher: four waves of sprites with shadows and a ring at the caster's feet; the caster's Sight stops.
        if let (SIGHT_RASHER, Target::Unit(unit)) = (id, target) {
            for path in [sight::SIGHT, sight::SHADOW] {
                self.sprites.entry(path.to_owned()).or_insert_with(|| SpriteSet::at(grf, path));
            }
            self.textures.entry(sight::RING.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, sight::RING, true).ok());
            self.prims.sights.play(at, now, |n| pick(0, n.saturating_sub(1)));
            self.stop_effect(SIGHT, unit);
        }
        // Signum Crucis' burst, half a second into its STR; it ignores /mineffect.
        // ponytail: the screen shapes draw after the STRs, so the cross lies under the burst; the game draws it over.
        if let (SIGNUM_CRUCIS, Target::Unit(unit)) = (id, target) {
            for path in [burst::DISC, burst::RAY, burst::RING] {
                self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok());
            }
            self.prims.bursts.push(burst::Burst::signum(unit, now + crate::prims::TICK * 30, 122, |n| pick(0, n.saturating_sub(1))));
        }
        if let (BASH | TURN_UNDEAD | SONIC_BLOW | OVER_THRUST, Target::Unit(unit)) = (id, target) {
            for path in [burst::DISC, burst::RAY, burst::RING] {
                self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok());
            }
            let rand = |n: u32| pick(0, n.saturating_sub(1));
            let burst = match id {
                BASH => burst::Burst::bash(unit, now, burst::LIFE, rand),
                SONIC_BLOW => burst::Burst::sonic_blow(unit, now),
                // A recast within its 40 ticks takes the place of the ring.
                OVER_THRUST => {
                    self.prims.bursts.retain(|burst| !(burst.over_thrust && burst.unit == unit));
                    burst::Burst::over_thrust(unit, now)
                }
                _ => burst::Burst::turn_undead(unit, now, rand),
            };
            self.prims.bursts.push(burst);
        }
        // Asura Strike's streaks stay where the target stood; Steel Body's ribbons follow their unit.
        if id == ASURA_STRIKE {
            self.textures.entry(monk::LENS.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, monk::LENS, true).ok());
            self.prims.monk.asura(at, now, self.min, |n| pick(0, n.saturating_sub(1)));
        }
        if let (RIBBONS, Target::Unit(unit)) = (id, target) {
            self.textures.entry(walls::RING_YELLOW.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, walls::RING_YELLOW, true).ok());
            self.prims.walls.ribbons(unit, now);
        }
        // Memorize and Double Casting flicker the body; Cart Termination's dots, Shield Chain's shields and Gravitation Field's spikes.
        match (id, target) {
            (SELF_DESTRUCTION, Target::Unit(unit)) => {
                for path in brawl::LENS {
                    self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok());
                }
                self.prims.brawl.flares(unit, now, |n| pick(0, n.saturating_sub(1)));
            }
            (STORM_BLAST, Target::Unit(unit)) => {
                self.textures.entry(walls::RING_YELLOW.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, walls::RING_YELLOW, true).ok());
                self.prims.walls.storm_blast(unit, now);
            }
            (HIT_LINE, Target::Unit(_)) => {
                self.textures.entry(brawl::WHITE.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, brawl::WHITE, true).ok());
                self.prims.brawl.hops(at, now, |n| pick(0, n.saturating_sub(1)));
            }
            (RAGE_BURST, Target::Unit(unit)) => {
                self.textures.entry(brawl::RAY.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, brawl::RAY, true).ok());
                self.prims.brawl.rays(unit, now, |n| pick(0, n.saturating_sub(1)));
            }
            (FOUR_WAY_BODY, Target::Unit(unit)) => self.prims.brawl.spread(unit, now),
            (STORM_KICK, Target::Unit(unit)) => {
                for path in [brawl::STORM, brawl::CLOUD] {
                    self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok());
                }
                self.prims.brawl.windmill(unit, at, now, |n| pick(0, n.saturating_sub(1)));
            }
            (RENOVATIO, Target::Unit(_)) => {
                self.textures.entry(shafts::CLOUD.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, shafts::CLOUD, true).ok());
                self.prims.shafts.play(at, now);
            }
            (CLOAKING, Target::Status(unit, _)) => self.prims.brawl.cloak(unit, now),
            (PHANTOM_RIBBONS, Target::Unit(unit)) => {
                self.textures.entry(walls::RING_PURPLE.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, walls::RING_PURPLE, true).ok());
                self.prims.walls.phantom_menace(unit, now);
            }
            (ACCELERATION | SHAPE_SHIFT, Target::Unit(unit)) => {
                self.textures.entry(walls::STREAK.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, walls::STREAK, true).ok());
                self.prims.guard.pulse(unit, now);
                self.prims.walls.pulse(unit, now, |n| pick(0, n.saturating_sub(1)));
            }
            (CONCENTRATION2, Target::Unit(unit)) => self.prims.guard.concentration(unit, now),
            (PURPLE_BODY, Target::Unit(unit)) => self.prims.guard.purple(unit, now),
            (MAGIC_CIRCLE, Target::Status(unit, _)) => {
                self.textures.entry(circle::CIRCLE.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, circle::CIRCLE, true).ok());
                self.prims.circles.oratio(unit, now, |n| pick(0, n.saturating_sub(1)));
            }
            (WEAPON_CRUSH, _) => {
                for path in guard::WEAPONS {
                    self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok());
                }
                self.prims.guard.weapon_crush(at, now, |n| pick(0, n.saturating_sub(1)));
            }
            (QUAKE_BODY, Target::Unit(unit)) => self.prims.brawl.shake(unit, now, 0, 34),
            (SKY_NET_SHAKE, Target::Unit(unit)) => self.prims.brawl.shake(unit, now, 31, 50),
            (RED_HIT, Target::Unit(unit)) => self.prims.brawl.red(unit, now),
            (SPINED_BODY, Target::Unit(unit)) => self.prims.brawl.swing(unit, now),
            (GREEN_BODY, Target::Unit(unit)) => self.prims.guard.green(unit, now),
            (HATED, Target::Unit(unit)) => {
                for path in crate::prims::sparkle::TEXTURES {
                    self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok());
                }
                self.prims.brawl.blue(unit, now);
                self.prims.blue_sparkles(unit, now);
            }
            (REVERBERATION, Target::Kept(trap)) => {
                for path in [brawl::STORM, brawl::CLOUD] {
                    self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok());
                }
                self.prims.brawl.trap_tornado(trap, at, now, |n| pick(0, n.saturating_sub(1)));
            }
            (MEMORIZE, Target::Unit(unit)) => self.prims.guard.memorize(unit, now),
            (DOUBLE_CAST, Target::Unit(unit)) => self.prims.guard.double_cast(unit, now),
            (CART_TERMINATION, Target::Unit(unit)) => {
                self.textures.entry(spray::DOT.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, spray::DOT, true).ok());
                self.prims.spray.termination(unit, now, |n| pick(0, n.saturating_sub(1)));
            }
            (SHIELD_CHAIN, Target::Unit(unit)) => {
                self.textures.entry(spray::SHIELD.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, spray::SHIELD, true).ok());
                self.prims.spray.chain(unit, now);
            }
            (GRAVITATION, Target::Ground) => {
                for path in [stone::STONE, crate::prims::ICE_TEXTURE] {
                    self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok());
                }
                self.prims.stone.gravitation(at, now, None);
            }
            _ => {}
        }
        // Meteor Assault's slashes, Full Strip's shields and swords, Preserve's dome, Enchant Deadly Poison's flicker.
        match (id, target) {
            (METEOR_ASSAULT, Target::Unit(unit)) => {
                self.textures.entry(knight::PURPLE_SLASH.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, knight::PURPLE_SLASH, true).ok());
                self.prims.knight.meteor(unit, at, now, |n| pick(0, n.saturating_sub(1)));
            }
            (ENCHANT_DEADLY_POISON, Target::Unit(unit)) => self.prims.guard.poison(unit, now),
            (FULL_STRIP, _) => {
                for path in [guard::SHIELD, guard::SWORD] {
                    self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok());
                }
                self.prims.guard.strip(at, now, |n| pick(0, n.saturating_sub(1)));
            }
            (PRESERVE, Target::Unit(unit)) => {
                self.textures.entry(guard::TILE.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, guard::TILE, true).ok());
                let facing = self.facings.get(&unit).copied().unwrap_or(0.0);
                self.prims.guard.preserve(unit, facing, now);
            }
            _ => {}
        }
        // Moonlit Water Mill's crescents, Head Crush's and Joint Beat's rays, Mind Breaker's flicker and the web on a unit.
        match (id, target) {
            (MOONLIT, Target::Status(unit, status)) => {
                self.textures.entry(dancer::FIRE.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, dancer::FIRE, true).ok());
                self.prims.dancer.mill(unit, Some(status), now, |n| pick(0, n.saturating_sub(1)));
            }
            (MOONLIT, Target::Unit(unit)) if !self.prims.dancer.has_mill(unit) => {
                self.textures.entry(dancer::FIRE.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, dancer::FIRE, true).ok());
                self.prims.dancer.mill(unit, None, now, |n| pick(0, n.saturating_sub(1)));
            }
            (HEAD_CRUSH | JOINT_BEAT, Target::Unit(unit)) => {
                self.textures.entry(knight::RAY.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, knight::RAY, true).ok());
                let rand = |n: u32| pick(0, n.saturating_sub(1));
                if id == HEAD_CRUSH { self.prims.knight.head_crush(unit, now, rand) } else { self.prims.knight.joint_beat(unit, now, rand) }
            }
            (MIND_BREAKER, Target::Unit(unit)) => self.prims.transcend.breaker(unit, now, |n| pick(0, n.saturating_sub(1))),
            (SPIDER_WEB, Target::Status(unit, _)) => {
                self.textures.entry(dancer::WEB.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, dancer::WEB, true).ok());
                self.prims.dancer.web(None, Some(unit), at, now, |n| pick(0, n.saturating_sub(1)));
            }
            _ => {}
        }
        // Soul Change's glow, True Sight's rays, Falcon Assault's spin, Wind Walk's arcs, Reject Sword's blinking body.
        match (id, target) {
            (SOUL_CHANGE_GLOW, Target::Unit(unit)) => self.prims.sniper.glow_link(unit, now),
            (TRUE_SIGHT, Target::Unit(unit)) => {
                self.textures.entry(sniper::RAY.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, sniper::RAY, true).ok());
                self.prims.sniper.true_sight(unit, now, |n| pick(0, n.saturating_sub(1)));
            }
            (FALCON_ASSAULT, Target::Unit(unit)) => self.prims.sniper.falcon(unit, now),
            (WIND_WALK, Target::Unit(unit)) => {
                self.textures.entry(sniper::CLOUD.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, sniper::CLOUD, true).ok());
                self.prims.sniper.wind_walk(at, now, |n| pick(0, n.saturating_sub(1)));
                self.prims.sniper.glow_wind_walk(unit, now);
            }
            (REJECT_SWORD, Target::Unit(unit)) => self.prims.sniper.reject_sword(unit, now),
            _ => {}
        }
        // Soul Drain's target turns blue, Magic Crasher flickers its target, Indulge's sparks loop out and back.
        match (id, target) {
            (DRAIN_TARGET, Target::Unit(unit)) => self.prims.transcend.drain_target(unit, now),
            (MAGIC_CRASHER, Target::Unit(unit)) => self.prims.transcend.crasher(unit, now, |n| pick(0, n.saturating_sub(1))),
            (INDULGE, Target::Unit(unit)) => {
                self.sprites.entry(crate::prims::GHOST_SPRITE.to_owned()).or_insert_with(|| SpriteSet::at(grf, crate::prims::GHOST_SPRITE));
                self.prims.indulge(at, now, |n| pick(0, n.saturating_sub(1)));
                self.prims.transcend.indulge(unit, now);
            }
            _ => {}
        }
        // Ganbantein's crescents over the cell, Spiral Pierce's cones, Pressure's cross and Martyr's Reckoning's rays, made in code.
        match (id, target) {
            (GANBANTEIN_FX, _) => {
                self.textures.entry(walls::RING_WHITE.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, walls::RING_WHITE, true).ok());
                self.prims.walls.ganbantein(at, now);
            }
            (SPIRAL_PIERCE, Target::Unit(unit)) => {
                self.textures.entry(walls::RING_YELLOW.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, walls::RING_YELLOW, true).ok());
                self.prims.knight.spiral(unit, now);
            }
            (PRESSURE, Target::Unit(unit)) => {
                for (path, keyed) in [(knight::CROSS, false), (knight::CROSS_FLASH, true)] {
                    self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, keyed).ok());
                }
                self.prims.knight.pressure(unit, now, |n| pick(0, n.saturating_sub(1)));
            }
            (MARTYR, Target::Unit(unit)) => {
                self.textures.entry(knight::RAY.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, knight::RAY, true).ok());
                self.prims.knight.martyr(unit, now, |n| pick(0, n.saturating_sub(1)));
            }
            _ => {}
        }
        // Shrink's flashes, Close Confine's blinking and the performers' cones of bubbles, made in code.
        match (id, target) {
            (SHRINK, Target::Unit(unit)) => self.prims.songs.shrink(unit, now),
            (CLOSE_CONFINE, Target::Unit(unit)) => self.prims.songs.confine(unit, now),
            (BUBBLE_CONE..=LAST_BUBBLE_CONE, Target::Unit(unit)) => {
                for path in songs::BUBBLES {
                    self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok());
                }
                self.prims.songs.cone(id, unit, 120, now);
            }
            // Adaptation on its caster; Eternal Chaos on whoever has the status, a new status-on replacing the old copy.
            (ADAPTATION | ETERNAL_CHAOS, Target::Unit(_) | Target::Status(..)) => {
                self.textures.entry(songs::STREAK_TEXTURE.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, songs::STREAK_TEXTURE, true).ok());
                let (unit, status) = match target {
                    Target::Status(unit, status) => (unit, Some(status)),
                    Target::Unit(unit) => (unit, None),
                    _ => (None, None),
                };
                if let Some(status) = status {
                    self.prims.songs.forget_status(unit, status);
                    self.prims.transcend.forget_status(unit, status);
                    self.prims.dancer.forget_status(unit, status);
                }
                self.prims.songs.dance(unit, status, id == ETERNAL_CHAOS, now, |n| pick(0, n.saturating_sub(1)));
            }
            _ => {}
        }
        // Slim Pitcher's potion falls on the cell, Full Protection strobes its target's body.
        match (id, target) {
            (SLIM_PITCHER, _) => {
                self.textures.entry(ground::FLASH.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, ground::FLASH, true).ok());
                self.textures.entry(ground::RED_POTION.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, ground::RED_POTION, false).ok());
                self.prims.ground.slim(at, ground::RED_POTION, now, |n| pick(0, n.saturating_sub(1)));
            }
            (CHEMICAL_BODY, Target::Unit(unit)) => self.prims.ground.chemical(unit, now),
            _ => {}
        }
        // Steal Coin's coins fly out round the target's feet, Raid's streaks round the user's, Chemical Protection's star flashes.
        match id {
            STEAL_COIN => {
                self.textures.entry(shards::COIN_TEXTURE.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, shards::COIN_TEXTURE, true).ok());
                self.prims.shards.coins(at, now, |n| pick(0, n.saturating_sub(1)));
            }
            RAID => {
                self.textures.entry(monk::LENS_RAID.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, monk::LENS_RAID, true).ok());
                self.prims.monk.raid(at, now, |n| pick(0, n.saturating_sub(1)));
            }
            CHEMICAL_PROTECTION => {
                self.textures.entry(star::TEXTURE.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, star::TEXTURE, true).ok());
                self.prims.stars.play(at, now);
            }
            _ => {}
        }
        // Intimidate's shards burst round the spot, for good.
        if id == INTIMIDATE {
            self.textures.entry(shards::TEXTURE.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, shards::TEXTURE, true).ok());
            self.prims.shards.burst(at, now, |n| pick(0, n.saturating_sub(1)));
        }
        // Berserk Pitcher's tint, cylinder and streaks, Defender's walls and Grand Cross's cross, made in code.
        let wall_textures: &[&str] = match id {
            BERSERK | POTION_BERSERK2 => &[walls::ALPHA_DOWN, walls::STREAK],
            DEFENDER => &[walls::RING_BLACK],
            PROVIDENCE | REFLECT_SHIELD => &[walls::RING_YELLOW],
            ABSORB_SPIRITS => &[walls::RING_BLUE],
            GRAND_CROSS => &[walls::RING_WHITE, walls::RING_YELLOW],
            _ => &[],
        };
        for path in wall_textures {
            self.textures.entry((*path).to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok());
        }
        match (id, target) {
            (BERSERK, Target::Unit(unit)) => self.prims.walls.berserk(unit, now, &walls::BERSERK, |n| pick(0, n.saturating_sub(1))),
            (POTION_BERSERK2, Target::Unit(unit)) => self.prims.walls.berserk(unit, now, &walls::ENCHANT_BLADE, |n| pick(0, n.saturating_sub(1))),
            (DEFENDER, Target::Unit(unit)) => self.prims.walls.defender(unit, now, walls::RING_BLACK),
            (REFLECT_SHIELD, Target::Unit(unit)) => self.prims.walls.defender(unit, now, walls::RING_YELLOW),
            (PROVIDENCE, Target::Unit(unit)) => self.prims.walls.providence(unit, now),
            (ABSORB_SPIRITS, Target::Unit(unit)) => self.prims.walls.curtains(unit, now, |n| pick(0, n.saturating_sub(1))),
            (GRAND_CROSS, _) => self.prims.walls.grand_cross(at, now),
            _ => {}
        }
        // A bolt told no hit count (by the server): one arrow.
        if let (true, Target::Unit(unit)) = (BOLTS.contains(&id), target) {
            self.bolt(gpu, grf, id, unit, 1, now);
        }
        // Frost Diver's second hit effect: ice spikes round the target.
        if id == FROST_DIVER_HIT {
            self.textures.entry(crate::prims::ICE_TEXTURE.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, crate::prims::ICE_TEXTURE, true).ok());
            self.prims.stone.frost_hit(at, now, |n| pick(0, n.saturating_sub(1)));
        }
        // COLDHIT is drawn in code, flat on the screen over its unit.
        if let (COLD_HIT, Target::Unit(unit)) = (id, target) {
            for path in [crate::prims::ICE_TEXTURE, crate::prims::SMOKE_TEXTURE] {
                self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok());
            }
            self.prims.cold_hit(unit, now, |n| pick(0, n.saturating_sub(1)));
        }
        // Napalm Beat is drawn in code, flat on the screen over its unit.
        if let (NAPALM_BEAT, Target::Unit(unit)) = (id, target) {
            for path in crate::prims::napalm::TEXTURES {
                self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok());
            }
            self.prims.napalm_beat(unit, now, |n| pick(0, n.saturating_sub(1)));
        }
        // EF_HALLUTIME (793): white glints round a copy.
        if id == HALLUTIME {
            for path in crate::prims::sparkle::TEXTURES {
                self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok());
            }
            let unit = match target {
                Target::Unit(unit) | Target::Status(unit, _) => unit,
                _ => None,
            };
            self.prims.sparkle(unit, at, now, |n| pick(0, n.saturating_sub(1)));
        }
        let anchor = |attached: bool| match target {
            Target::Unit(unit) if attached => Anchor::Unit(unit),
            Target::Lasting(unit) | Target::Kept(unit) => Anchor::Lasting(unit, at),
            Target::Status(unit, status) => Anchor::Status(unit, status),
            _ => Anchor::At(at),
        };
        let lasting = matches!(target, Target::Lasting(_) | Target::Status(..));
        // On a ground unit everything starts again, but what says it doesn't.
        let again = |parameters: &HashMap<String, String>| lasting && parameters.get("repeat").is_none_or(|value| value != "false");
        let attached = |parameters: &HashMap<String, String>| parameters.get("attachedEntity").is_some_and(|value| value == "true");
        for parameters in table.floaters.get(&id).into_iter().flatten() {
            let copies = parameters.get("duplicate").and_then(|value| value.parse::<i32>().ok()).unwrap_or(1).clamp(1, 50) as u32;
            let gap = parameters.get("timeBetweenDupli").and_then(|value| value.parse::<f32>().ok()).unwrap_or(200.0).max(0.0);
            for copy in 0..copies {
                let start = now + Duration::from_secs_f32(gap * copy as f32 / 1000.0);
                let Some(mut floating) = Floating::new(parameters, id, copy, start, anchor(attached(parameters))) else { continue };
                floating.again |= again(parameters);
                for path in &floating.textures {
                    self.textures.entry(path.clone()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok());
                }
                if let Some(path) = &floating.sprite {
                    self.sprites.entry(path.clone()).or_insert_with(|| SpriteSet::at(grf, path));
                }
                self.floating.push(floating);
            }
        }
        for parameters in table.rings.get(&id).into_iter().flatten() {
            let copies = parameters.get("duplicate").and_then(|value| value.parse::<i32>().ok()).unwrap_or(1).clamp(1, 50) as u32;
            let gap = parameters.get("timeBetweenDupli").and_then(|value| value.parse::<f32>().ok()).unwrap_or(200.0).max(0.0);
            for copy in 0..copies {
                let start = now + Duration::from_secs_f32(gap * copy as f32 / 1000.0);
                let Some(mut ring) = Ring::new(parameters, id, start, anchor(attached(parameters))) else { continue };
                ring.again |= again(parameters);
                self.textures.entry(ring.texture.clone()).or_insert_with(|| gpu.try_load_texture(grf, &ring.texture, true).ok());
                self.rings.push(ring);
            }
        }
        for part in table.sprites.get(&id).into_iter().flatten() {
            let animation = Animation::new(part, id, now, anchor(part.attached), lasting);
            self.sprites.entry(animation.sprite.clone()).or_insert_with(|| SpriteSet::at(grf, &animation.sprite));
            self.animations.push(animation);
        }
        self.play_quakes(table, id, now);
        if matches!(id, EARTH_SPIKE | HEAVENS_DRIVE) {
            self.textures.entry(stone::STONE.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, stone::STONE, true).ok());
            if id == EARTH_SPIKE { self.prims.stone.earth_spike(at, now) } else { self.prims.stone.heavens_drive(at, now) }
        }
        // Ice Wall's cell: a skill unit's, the spikes standing until it goes.
        if let (ICE_WALL, Target::Lasting(unit) | Target::Kept(unit)) = (id, target) {
            self.textures.entry(crate::prims::ICE_TEXTURE.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, crate::prims::ICE_TEXTURE, true).ok());
            self.prims.stone.ice_wall(unit, at, now, |n| pick(0, n.saturating_sub(1)));
        }
        // Blessing, made in code.
        // A song's puffs and Steel Body's glints, made in code.
        let kind = song_variant(id).map(PrimKind::Puffs).or((id == 254 || id == GUMGANG).then_some(PrimKind::Glints));
        if let Some(kind) = kind {
            let paths: &[&str] = if kind == PrimKind::Glints { &GLINT_TEXTURES } else { &[PUFF_TEXTURE] };
            for path in paths {
                self.textures.entry((*path).to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, kind == PrimKind::Glints).ok());
            }
            // Explosion Spirits' glints start again from a new packet: the old copy goes first.
            if let (GUMGANG, Target::Unit(unit) | Target::Status(unit, _)) = (id, target) {
                self.status_prims
                    .retain(|prim| !(prim.effect == GUMGANG && matches!(prim.anchor, Anchor::Unit(each) | Anchor::Status(each, _) if each == unit)));
            }
            self.status_prims.push(StatusPrim { kind, effect: id, start: now, anchor: anchor(true), seed: random(0.0, 1.0e9) as u32 });
        }
        if id == 42 {
            for path in [ANGEL_SPRITE, BLESSING_SPARKLE] {
                self.sprites.entry(path.to_owned()).or_insert_with(|| SpriteSet::at(grf, path));
            }
            self.textures.entry(BLESSING_GLOW.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, BLESSING_GLOW, true).ok());
            let sparkles = sparkle_starts()
                .map(|_| {
                    let (sin, cos) = random(0.0, 360.0).to_radians().sin_cos();
                    [sin, cos]
                })
                .collect();
            self.blessings.push(Blessing { start: now, anchor: anchor(true), sparkles, angels: Default::default(), sparkle: Vec::new() });
        }
        // Heal, the teleport, the arrival and the logout: bands made in code (a cast's aura comes by `play_aura`).
        if is_banded(id) && !is_cast_aura(id) {
            for path in [format!("data\\texture\\effect\\{}.tga", aura_look(id).0), SPARKLE_TEXTURE.to_owned()] {
                self.textures.entry(path.clone()).or_insert_with(|| gpu.try_load_texture(grf, &path, true).ok());
            }
            let healing = matches!(id, 312 | 313 | 325);
            let whole = |n: u32| pick(0, n - 1) as f32;
            // Safety Wall's three walls take a turn each; the undead heal's two take a turn, a height, a radius and a turn.
            let (turns, extra) = match id {
                315 => ([whole(360), whole(360), whole(360)], [0.0; 2]),
                320 => {
                    let first = whole(360);
                    let height = 40.0 + whole(20);
                    let radius = 4.5 + 0.005 * whole(100);
                    ([first, whole(360), 0.0], [height, radius])
                }
                _ => ([random(0.0, 360.0), random(0.0, 360.0), 0.0], [0.0; 2]),
            };
            self.auras.push(Banded {
                effect: id,
                anchor: anchor(true),
                start: now,
                ticks: banded_ticks(id),
                turns,
                extra,
                sparkles: if healing { heal_sparkles(random) } else { Vec::new() },
            });
        }
        for part in table.parts.get(&id).into_iter().flatten() {
            // Every file a `%d` names is loaded, so each restart can pick again.
            let files: Vec<String> = match (min_file(id).filter(|_| self.min), part.random) {
                (Some(file), _) => vec![file.to_owned()],
                (None, Some((from, to))) => (from..=to).map(|number| part.file.replace("%d", &number.to_string())).collect(),
                (None, None) => vec![part.file.clone()],
            };
            let variants: Vec<Rc<Loaded>> = files
                .iter()
                .filter_map(|file| self.loaded.entry(format!("{}|{file}", part.folder)).or_insert_with(|| load(gpu, grf, file, &part.folder)).clone())
                .collect();
            let Some(loaded) = variants.get(pick(0, variants.len().saturating_sub(1) as u32) as usize).cloned() else { continue };
            let again = lasting && !part.once;
            self.playing.push(Playing {
                loaded,
                variants,
                start: now,
                anchor: anchor(part.attached),
                again,
                length: table.lengths.get(&id).copied(),
                effect: id,
            });
        }
    }

    /// EF_HIT1 on a unit (None = the player) facing `rot` degrees (as `Actor::rot`), from `start`.
    pub fn play_hit(&mut self, gpu: &Gpu, grf: &Grf, unit: Option<u32>, rot: f32, start: Instant) {
        self.textures.entry(RING_TEXTURE.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, RING_TEXTURE, true).ok());
        self.sprites.entry(SPARK_SPRITE.to_owned()).or_insert_with(|| SpriteSet::at(grf, SPARK_SPRITE));
        let way = drift(rot);
        self.hits.push(HitSpark { start, anchor: Anchor::Unit(unit), way, sparks: sparks(way, random), paths: None, placed: Vec::new() });
    }

    /// The hit sparks (for the test driver): the way each ring flies (x east, z north), the milliseconds until it lands,
    /// its drawn tick, and where it stays once landed.
    pub fn hit_sparks(&self, now: Instant) -> Vec<([f32; 2], u64, u32, Option<Vector>)> {
        self.hits
            .iter()
            .map(|hit| {
                let landing = hit.start.saturating_duration_since(now).as_millis() as u64;
                (hit.way, landing, hit.tick(now), hit.paths.as_ref().and_then(|_| hit.anchor.place(&|_| None)))
            })
            .collect()
    }

    /// Hits landing by `now` stay where their targets' feet are (`position_of`), and their sparks' paths are worked out
    /// over the ground there (`ground_at(x, z)`: the ground's height in the world).
    pub fn settle_hits(&mut self, now: Instant, position_of: impl Fn(Option<u32>) -> Option<Vector>, ground_at: impl Fn(f32, f32) -> f32) {
        self.prims.first.advance(now, &ground_at);
        self.prims.songs.advance(now, position_of(None), &ground_at, |n| pick(0, n.saturating_sub(1)));
        for hit in self.hits.iter_mut().filter(|hit| hit.paths.is_none() && hit.start <= now) {
            let Some(at) = hit.anchor.place(&position_of) else { continue };
            hit.anchor = Anchor::At(at);
            hit.paths = Some(hit.sparks.map(|spark| spark.path(|x, z| ground_at(at[0] + x, at[2] + z) - at[1])));
        }
    }

    /// A cast's aura (12 or 54–59) on a unit (None = the player) for a cast of `cast_ms`, replacing its last one.
    pub fn play_aura(&mut self, gpu: &Gpu, grf: &Grf, unit: Option<u32>, effect: u32, cast_ms: u32, start: Instant) {
        for set in 0..aura_sets(effect) {
            let texture = aura_set_texture(effect, set);
            self.textures.entry(texture.clone()).or_insert_with(|| gpu.try_load_texture(grf, &texture, true).ok());
        }
        let anchor = Anchor::Unit(unit);
        self.auras.retain(|aura| !(is_cast_aura(aura.effect) && matches!(aura.anchor, Anchor::Unit(each) if each == unit)));
        self.auras.push(Banded { effect, anchor, start, ticks: aura_ticks(effect, cast_ms), turns: [0.0; 3], extra: [0.0; 2], sparkles: Vec::new() });
    }

    /// A cast aura launched again with a skill's damage: it blooms for its own length, beside any cast's aura.
    pub fn bloom_aura(&mut self, gpu: &Gpu, grf: &Grf, unit: Option<u32>, effect: u32, start: Instant) {
        for set in 0..aura_sets(effect) {
            let texture = aura_set_texture(effect, set);
            self.textures.entry(texture.clone()).or_insert_with(|| gpu.try_load_texture(grf, &texture, true).ok());
        }
        self.auras.push(Banded {
            effect,
            anchor: Anchor::Unit(unit),
            start,
            ticks: aura_ticks(effect, 0),
            turns: [0.0; 3],
            extra: [0.0; 2],
            sparkles: Vec::new(),
        });
    }

    /// Queues a camera shake starting at `start` for `length` (`QUAKE_SHORT` or `QUAKE_LONG`).
    pub fn start_quake(&mut self, start: Instant, length: Duration) {
        let length = if length == QUAKE_SHORT { QUAKE_SHORT } else { QUAKE_LONG };
        self.quakes.push((start, length));
    }

    /// Queues the camera shakes of effect `id` starting from `now`.
    pub fn play_quakes(&mut self, table: &Table, id: u32, now: Instant) {
        for &(delay, length) in table.quakes.get(&id).into_iter().flatten() {
            self.start_quake(now + delay, length);
        }
    }

    /// Shows what a skill's unit on the ground shows (`kind`: rathena's UNT_ number) until `forget(id)`.
    pub fn play_unit(&mut self, gpu: &Gpu, grf: &Grf, table: &Table, kind: u32, id: u32, at: Vector, now: Instant) {
        // The server tells again of units coming back into sight.
        self.forget(id);
        self.unit_kinds.insert(id, kind);
        if let Some(&effect) = table.units.get(&kind) {
            // Volcano's, Deluge's, Violent Gale's and Land Protector's cells keep their loops, drawn in code.
            if effect == GRAVITATION {
                for path in [stone::STONE, crate::prims::ICE_TEXTURE] {
                    self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok());
                }
                self.prims.stone.gravitation(at, now, Some(id));
            } else if effect == FOG_WALL {
                self.textures.entry(guard::FOG.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, guard::FOG, true).ok());
                self.prims.guard.keep_fog(id, at, now, |n| pick(0, n.saturating_sub(1)));
            } else if effect == SPIDER_WEB {
                self.textures.entry(dancer::WEB.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, dancer::WEB, true).ok());
                self.prims.dancer.web(Some(id), None, at, now, |n| pick(0, n.saturating_sub(1)));
            } else if effect == GOSPEL {
                self.textures.entry(transcend::CROSS.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, transcend::CROSS, false).ok());
                self.prims.transcend.keep_gospel(id, at, now, |n| pick(0, n.saturating_sub(1)));
            } else if matches!(effect, MAGNUS | SANCTUARY) {
                // Magnus' and Sanctuary's glowing boxes stand over each cell with their unit.
                let look = if effect == MAGNUS { ground::RING_RED } else { ground::ALPHA_DOWN };
                self.textures.entry(look.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, look, true).ok());
                self.prims.ground.keep_box(effect, id, at, now, self.min, |n| pick(0, n.saturating_sub(1)));
            } else if effect == POISON_SMOKE_CELL {
                // Poison Smoke's cells throw violet sparkles over everything while their unit stands.
                self.sprites.entry(first::POISON_STAR.to_owned()).or_insert_with(|| SpriteSet::at(grf, first::POISON_STAR));
                self.prims.walls.keep_smoke(id, at, now);
            } else if matches!(effect, FIRE_PILLAR | VENOM_DUST) {
                // The Fire Pillar's funnels and Venom Dust's puffs stand with their unit, drawn in code.
                self.textures.entry(walls::MAGIC_RED.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, walls::MAGIC_RED, true).ok());
                self.sprites.entry(first::POISON_STAR.to_owned()).or_insert_with(|| SpriteSet::at(grf, first::POISON_STAR));
                self.prims.walls.keep(effect, id, at, now);
            } else if sage::LOOPS.iter().any(|&(kept, _)| kept == effect) {
                self.load_sage(gpu, grf, effect);
                self.prims.sage.keep(effect, id, at, now, |n| pick(0, n.saturating_sub(1)));
            } else {
                let target = if table.units_once.contains(&kind) { Target::Kept(id) } else { Target::Lasting(id) };
                self.play(gpu, grf, table, effect, target, at, now);
            }
        }
        let Some(file) = table.models.get(&kind) else { return };
        let model = self.models.entry(file.clone()).or_insert_with(|| load_model(grf, file).map(Rc::new)).clone();
        if let Some(model) = model {
            for (path, _) in &model.0 {
                self.textures.entry(path.clone()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok());
            }
            self.standing.push((model, id, at));
        }
    }

    /// The kinds (rathena's UNT_ numbers) of the units standing as models now, sorted (for the test driver).
    pub fn standing_kinds(&self) -> Vec<u32> {
        let mut kinds: Vec<u32> = self.standing.iter().filter_map(|(_, id, _)| self.unit_kinds.get(id).copied()).collect();
        kinds.sort_unstable();
        kinds
    }

    /// The STRs playing on skills' units on the ground (for the test driver): the unit's kind (rathena's UNT_ number),
    /// the effect, whether it starts again, and its milliseconds since it (last) started; sorted.
    pub fn unit_strs(&self, now: Instant) -> Vec<(u32, u32, bool, u64)> {
        let mut strs: Vec<_> = self
            .playing
            .iter()
            .filter_map(|playing| match playing.anchor {
                Anchor::Lasting(id, _) => {
                    let kind = self.unit_kinds.get(&id).copied()?;
                    Some((kind, playing.effect, playing.again, now.saturating_duration_since(playing.start).as_millis() as u64))
                }
                _ => None,
            })
            .collect();
        strs.sort_unstable();
        strs
    }

    /// The sounds and shakes a skill's use packet makes, wherever the caster is.
    pub fn play_skill_sounds(&mut self, table: &Table, skill: u16, now: Instant) {
        for sound in table.skill_sounds.get(&skill).into_iter().flatten() {
            self.sounds.push((now + Duration::from_millis(sound.delay_ms), format!("{}.wav", sound.file), LISTENER));
        }
        for &(delay, length) in table.skill_quakes.get(&skill).into_iter().flatten() {
            self.start_quake(now + delay, length);
        }
    }

    /// The sounds and shakes a skill's damage packet makes, wherever the caster is.
    pub fn play_hit_sounds(&mut self, table: &Table, skill: u16, now: Instant) {
        for sound in table.hit_sounds.get(&skill).into_iter().flatten() {
            self.sounds.push((now + Duration::from_millis(sound.delay_ms), format!("{}.wav", sound.file), LISTENER));
        }
        for &(delay, length) in table.hit_quakes.get(&skill).into_iter().flatten() {
            self.start_quake(now + delay, length);
        }
    }

    /// The camera shake this frame at `now`: `[sideways, distance, pitch_degrees]`, with the sign flipping each frame
    /// and fresh noise each frame; `None` when no shake runs.
    pub fn quake(&mut self, now: Instant) -> Option<[f32; 3]> {
        let (mut due, later): (Vec<_>, Vec<_>) = std::mem::take(&mut self.quakes).into_iter().partition(|(start, _)| *start <= now);
        self.quakes = later;
        due.sort_by_key(|&(start, _)| start);
        for (start, length) in due {
            self.quake_until = Some(start + length);
        }
        if self.quake_until.is_none_or(|until| now >= until) {
            self.quake_until = None;
            return None;
        }
        self.quake_flip = !self.quake_flip;
        let sign = if self.quake_flip { 1.0 } else { -1.0 };
        let noise = || (pick(0, 4) as i32 - 2) as f32;
        Some([sign * 1.0 + noise() / 10.0, sign * 0.24 + noise() / 10.0, sign * 0.2 + noise() / 15.0])
    }

    /// The sounds whose moment came: the file (under data\wav\), and where.
    pub fn due_sounds(&mut self, now: Instant) -> Vec<(String, Vector)> {
        let (due, later): (Vec<_>, Vec<_>) = std::mem::take(&mut self.sounds).into_iter().partition(|(when, ..)| *when <= now);
        self.sounds = later;
        due.into_iter().map(|(_, file, at)| (file, at)).collect()
    }

    /// Frost Diver from `from` at `to`: its line of ice spikes.
    fn frost_diver(&mut self, gpu: &Gpu, grf: &Grf, from: Vector, to: Vector, now: Instant) {
        // /mineffect: no line at all.
        if self.min {
            return;
        }
        self.textures.entry(crate::prims::ICE_TEXTURE.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, crate::prims::ICE_TEXTURE, true).ok());
        self.prims.stone.frost_diver(from, to, now, |n| pick(0, n.saturating_sub(1)));
    }

    /// An effect launched on the caster at `from`, aimed at `to` (one of `AIMED`), told the skill's hit count.
    #[allow(clippy::too_many_arguments)]
    pub fn aimed(
        &mut self,
        gpu: &Gpu,
        grf: &Grf,
        table: &Table,
        id: u32,
        (caster, target, own): (Option<u32>, u32, u32),
        from: Vector,
        to: Vector,
        hits: u16,
        now: Instant,
    ) {
        match id {
            SOUL_STRIKE => self.soul_strike(grf, from, to, hits, now, false),
            DARK_STRIKE => self.soul_strike(grf, from, to, hits, now, true),
            SPHERE | STRAIGHT_SPHERE => {
                for path in [tanji::SPHERE, tanji::RING] {
                    self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok());
                }
                // 412 has no sound.
                if id == SPHERE {
                    self.play_sounds(table, id, from, now);
                }
                self.prims.spheres.throw(tanji::Sphere::new(id == STRAIGHT_SPHERE, from, to, hits, now, self.dim));
            }
            POTION_PITCHER | BERSERK_PITCHER | SPORE_EXPLOSION => {
                if let Some(path) = throw::icon_of(id) {
                    self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, false).ok());
                }
                // Only Spore Explosion has a sound, made at the thrower's first update.
                let sound = table.sounds.get(&id).and_then(|sounds| sounds.first()).map(|sound| format!("{}.wav", sound.file));
                if let Some(throw) = throw::Throw::new(id, caster, from, to, now, sound) {
                    self.prims.throws.throw(throw);
                }
            }
            HUUJIN => {
                self.textures.entry(huujin::WIND.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, huujin::WIND, true).ok());
                self.play_sounds(table, id, from, now);
                self.prims.huujin.throw(from, hits, target, own, now, |n| pick(0, n.saturating_sub(1)));
            }
            FROST_DIVER => self.frost_diver(gpu, grf, from, to, now),
            FIRE_BALL => {
                self.sprites.entry(balls::FIRE_BALL_SPRITE.to_owned()).or_insert_with(|| SpriteSet::at(grf, balls::FIRE_BALL_SPRITE));
                // Each ball sounds as it leaves.
                for tick in balls::FIRE_BALL_TICKS {
                    self.play_sounds(table, id, from, now + crate::prims::TICK * tick);
                }
                self.prims.balls.fire_ball(from, to, now);
            }
            SHIELD_BOOMERANG => {
                self.textures.entry(boomerang::SHIELD.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, boomerang::SHIELD, true).ok());
                self.play_sounds(table, id, from, now);
                self.prims.boomerangs.throw(caster, from, to, now, self.dim);
            }
            SPEAR_BOOMERANG => {
                self.sprites.entry(balls::SPEAR_SPRITE.to_owned()).or_insert_with(|| SpriteSet::at(grf, balls::SPEAR_SPRITE));
                // A sound with each spear, set by the table's delays.
                self.play_sounds(table, id, from, now);
                self.prims.balls.spear_boomerang(from, to, now);
            }
            JUPITEL => {
                for path in std::iter::once(balls::THUNDER_CORE).chain(balls::THUNDER_BALL) {
                    self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok());
                }
                self.play_sounds(table, id, from, now);
                self.prims.balls.thunder(from, to, now);
            }
            TRIPLE_ATTACK => {
                self.textures.entry(ground::CLOUD.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, ground::CLOUD, true).ok());
                self.play_sounds(table, id, from, now);
                self.prims.ground.triple(from, to, now, |n| pick(0, n.saturating_sub(1)));
            }
            BACK_STAB => {
                self.textures.entry(first::THUNDER_CENTER.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, first::THUNDER_CENTER, true).ok());
                self.play_sounds(table, id, from, now);
                self.prims.monk.back_stab(caster, to, now, |n| pick(0, n.saturating_sub(1)));
            }
            INVESTIGATE_SPARKS => {
                self.textures.entry(first::THUNDER_CENTER.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, first::THUNDER_CENTER, true).ok());
                self.prims.monk.investigate(caster, to, now, |n| pick(0, n.saturating_sub(1)));
            }
            SHARP_SHOOTING | ARROW_VULCAN => {
                self.textures.entry(sniper::RAY.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, sniper::RAY, true).ok());
                self.play_sounds(table, id, from, now);
                self.prims.sniper.shot(id == ARROW_VULCAN, caster, to, now);
            }
            SOUL_DRAIN => {
                self.sprites.entry(crate::prims::GHOST_SPRITE.to_owned()).or_insert_with(|| SpriteSet::at(grf, crate::prims::GHOST_SPRITE));
                self.prims.soul_drain(from, to, now, |n| pick(0, n.saturating_sub(1)));
                self.prims.transcend.drain_caster(caster, now);
            }
            MARIONETTE_LINK => {
                self.textures.entry(transcend::TUBE.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, transcend::TUBE, true).ok());
                self.play_sounds(table, id, from, now);
                self.prims.transcend.marionette(caster, target, own, now);
            }
            LINE_LINK => {
                self.textures.entry(transcend::TUBE.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, transcend::TUBE, true).ok());
                self.play_sounds(table, id, from, now);
                self.prims.transcend.link(caster, target, own, now);
            }
            SOUL_BREAKER => {
                self.textures.entry(knight::PURPLE_SLASH.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, knight::PURPLE_SLASH, true).ok());
                self.play_sounds(table, id, from, now);
                self.prims.knight.breaker(caster, from, to, now, |n| pick(0, n.saturating_sub(1)));
            }
            PANG_VOICE | WINK_OF_CHARM => {
                let sprite = if id == PANG_VOICE { monk::FVOICE } else { monk::WINK };
                self.sprites.entry(sprite.to_owned()).or_insert_with(|| SpriteSet::at(grf, sprite));
                self.play_sounds(table, id, from, now);
                let longitude = self.yaw.to_degrees().floor() as i32;
                if id == PANG_VOICE { self.prims.monk.pang_voice(from, to, longitude, now) } else { self.prims.monk.wink(from, to, longitude, now) }
            }
            CLAW_SLASH => {
                self.sprites.entry(monk::CLAW.to_owned()).or_insert_with(|| SpriteSet::at(grf, monk::CLAW));
                self.play_sounds(table, id, from, now);
                // ponytail: the camera's longitude is its yaw in whole degrees, as the game's turning table reads it.
                self.prims.monk.claw(from, to, self.yaw.to_degrees().floor() as i32, now);
            }
            WATER_BALL => {
                self.sprites.entry(waterball::SPRITE.to_owned()).or_insert_with(|| SpriteSet::at(grf, waterball::SPRITE));
                self.play_sounds(table, id, from, now);
                self.prims.waterballs.throw(from, to, now);
            }
            RELEASE => {
                self.sprites.entry(balls::FIRE_BALL_SPRITE.to_owned()).or_insert_with(|| SpriteSet::at(grf, balls::FIRE_BALL_SPRITE));
                self.play_sounds(table, id, from, now);
                self.prims.balls.release(from, to, now);
            }
            SPEAR_FLASH => {
                self.sprites.entry(monk::SPEARS.to_owned()).or_insert_with(|| SpriteSet::at(grf, monk::SPEARS));
                self.play_sounds(table, id, from, now);
                self.prims.monk.hundred_spear(from, to, self.yaw.to_degrees().floor() as i32, now);
            }
            BLOOD_DRAIN => {
                self.sprites.entry(crate::prims::GHOST_SPRITE.to_owned()).or_insert_with(|| SpriteSet::at(grf, crate::prims::GHOST_SPRITE));
                // The drops leave the target for the caster: `to` is the one hit.
                self.prims.blood_drain(to, from, now);
            }
            CHEMICAL => {
                self.textures.entry(brawl::STRIP.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, brawl::STRIP, true).ok());
                self.play_sounds(table, id, LISTENER, now);
                self.prims.brawl.lines(from, to, now, |n| pick(0, n.saturating_sub(1)));
            }
            GRIMTOOTH => {
                self.textures.entry(stone::STONE.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, stone::STONE, true).ok());
                self.play_sounds(table, id, from, now);
                self.prims.stone.grimtooth(from, to, now);
            }
            _ => {}
        }
    }

    /// Soul Strike from `from` at `to` with `hits` hits: a ghost a hit, each with its sound as it leaves.
    fn soul_strike(&mut self, grf: &Grf, from: Vector, to: Vector, hits: u16, now: Instant, red: bool) {
        let sprite = if red { crate::prims::RED_GHOST_SPRITE } else { crate::prims::GHOST_SPRITE };
        self.sprites.entry(sprite.to_owned()).or_insert_with(|| SpriteSet::at(grf, sprite));
        for at in crate::prims::ghost_times(hits) {
            self.sounds.push((now + at, "effect\\EF_SoulStrike.wav".to_owned(), from));
        }
        self.prims.soul_strike(from, to, hits, now, red);
    }

    /// A bow's arrow from over the archer's `feet`, leaving `wait` after `now` (`arrows`).
    pub fn shoot_arrow(&mut self, grf: &Grf, feet: Vector, aim: crate::arrows::Aim, now: Instant, wait: Duration) {
        self.sprites.entry(crate::arrows::SPRITE.to_owned()).or_insert_with(|| SpriteSet::at(grf, crate::arrows::SPRITE));
        self.arrows.shoot(feet, aim, now, wait);
    }

    /// Shields in flight turn back to where their casters are (`position_of`).
    pub fn follow_boomerangs(&mut self, now: Instant, position_of: impl Fn(Option<u32>) -> Option<Vector>) {
        self.prims.boomerangs.advance(now, &position_of);
        self.prims.spheres.advance(now, position_of(None), |n| pick(0, n.saturating_sub(1)));
        self.prims.huujin.advance(now, &position_of, |n| pick(0, n.saturating_sub(1)));
        self.prims.knight.advance(now, &position_of);
        self.prims.transcend.advance(now, &position_of);
        self.prims.sniper.advance(now, &position_of, |n| pick(0, n.saturating_sub(1)));
        self.prims.spray.advance(now, &position_of, |n| pick(0, n.saturating_sub(1)));
        self.prims.brawl.advance(now, |n| pick(0, n.saturating_sub(1)));
        for blues in &mut self.prims.blues {
            blues.advance(now, &position_of, |n| pick(0, n.saturating_sub(1)));
        }
        self.prims.monk.advance(now, &position_of, |n| pick(0, n.saturating_sub(1)));
        for (file, at) in self.prims.throws.advance(now, &position_of) {
            self.sounds.push((now, file, at));
        }
    }

    /// Arrows leaving now take their aim at where their units are (`position_of`).
    pub fn aim_arrows(&mut self, now: Instant, position_of: impl Fn(Option<u32>) -> Option<Vector>) {
        self.arrows.aim(now, position_of);
    }

    /// Effects that finished go and those that repeat start again; sprites pick their images for `now`, seen through
    /// a camera turned `yaw` radians.
    pub fn advance(&mut self, gpu: &Gpu, now: Instant, yaw: f32) {
        // A song's puffs end with their 400 ticks.
        self.status_prims.retain(|prim| !matches!(prim.kind, PrimKind::Puffs(_)) || str_frame(now.saturating_duration_since(prim.start)) < PUFF_TICKS);
        // The game checks /mineffect each tick, so turning it on stops a Frost Diver's line.
        if self.min {
            self.prims.stone.stop_lines(now);
        }
        self.prims.advance(now);
        if let Some(Some(sprite)) = self.sprites.get_mut(crate::prims::GHOST_SPRITE) {
            self.ghost = sprite.frame_at(gpu, 0, 0, Timing::Loop(0.0));
        }
        for path in [first::STAR, first::GOLD_STAR, first::CYAN_STAR, first::POISON_STAR] {
            if let (Some(Some(sprite)), false) = (self.sprites.get_mut(path), self.stars.contains_key(path)) {
                let layers = sprite.frame_at(gpu, 0, 0, Timing::Loop(0.0));
                self.stars.insert(path, layers);
            }
        }
        self.prims.sights.advance(now);
        for drawn in self.prims.sights.sprites(now) {
            if let (Some(Some(sprite)), false) = (self.sprites.get_mut(drawn.sprite), self.sight_frames.contains_key(&(drawn.sprite, drawn.frame))) {
                let frames = sprite.frames(0).max(1);
                let layers = sprite.frame_plain(gpu, 0, Timing::Span((drawn.frame as f32 + 0.5) / frames as f32));
                self.sight_frames.insert((drawn.sprite, drawn.frame), layers);
            }
        }
        for draw in self.prims.monk.claws(now) {
            if let Some(Some(sprite)) = self.sprites.get_mut(draw.sprite) {
                let frames = sprite.frames(draw.action).max(1);
                self.claw_frames
                    .entry((draw.sprite, draw.action, draw.frame))
                    .or_insert_with(|| sprite.frame_plain(gpu, draw.action, Timing::Span((draw.frame as f32 + 0.5) / frames as f32)));
            }
        }
        if let Some(Some(sprite)) = self.sprites.get_mut(waterball::SPRITE) {
            let frames = sprite.frames(0).max(1);
            for drawn in self.prims.waterballs.drawn(now) {
                self.water_frames.entry(drawn.image).or_insert_with(|| sprite.frame_plain(gpu, 0, Timing::Span((drawn.image as f32 + 0.5) / frames as f32)));
            }
        }
        if let Some(Some(sprite)) = self.sprites.get_mut(first::HOLY_WATER_SPRITE) {
            for (_, frame) in self.prims.first.holy_waters(now, |_| Some([0.0; 3])) {
                self.holy_frames.entry(frame).or_insert_with(|| sprite.frame_plain(gpu, 0, Timing::Span((frame as f32 + 0.5) / 42.0)));
            }
        }
        if let Some(Some(sprite)) = self.sprites.get_mut(crate::prims::RED_GHOST_SPRITE) {
            self.red_ghost = sprite.frame_at(gpu, 0, 0, Timing::Loop(0.0));
        }
        if let Some(Some(sprite)) = self.sprites.get_mut(balls::FIRE_BALL_SPRITE).filter(|_| self.fire_ball.is_empty()) {
            let frames = sprite.frames(0).max(1);
            self.fire_ball = (0..frames).map(|frame| sprite.frame_at(gpu, 0, 0, Timing::Span((frame as f32 + 0.5) / frames as f32))).collect();
        }
        if let Some(Some(sprite)) = self.sprites.get_mut(balls::SPEAR_SPRITE).filter(|_| self.spear.is_empty()) {
            let frames = sprite.frames(0).max(1);
            self.spear = sprite.frame_at(gpu, 0, 0, Timing::Span(0.5 / frames as f32));
        }
        self.yaw = yaw.to_degrees();
        if self.spark_frames.is_empty()
            && let Some(Some(sprite)) = self.sprites.get_mut(bolts::SPARK_SPRITE)
        {
            let count = sprite.frames(0).max(1);
            self.spark_frames = (0..count).map(|frame| sprite.frame_at(gpu, 0, 0, Timing::Span(frame as f32 / count as f32))).collect();
        }
        if let Some(Some(sprite)) = self.sprites.get_mut(crate::arrows::SPRITE).filter(|_| !self.arrows.is_empty()) {
            self.arrow = (0..8).map(|direction| sprite.frame_at(gpu, direction, 0, Timing::Loop(0.0))).collect();
        }
        for floating in self.floating.iter_mut().filter(|floating| floating.again && floating.end <= now) {
            let period = floating.end.duration_since(floating.start);
            while floating.end <= now {
                floating.start += period;
                floating.end += period;
            }
        }
        self.floating.retain(|floating| now < floating.end);
        for ring in self.rings.iter_mut().filter(|ring| ring.again && ring.end <= now) {
            let period = ring.end.duration_since(ring.start);
            while ring.end <= now {
                ring.start += period;
                ring.end += period;
            }
        }
        self.rings.retain(|ring| now < ring.end);
        // An STR ends at its last key or its effect's length; a ground unit's starts again 8 ticks before its length
        // (Quagmire's 20), picking its file again, while the copy before plays out.
        let mut tails = Vec::new();
        let tick = Duration::from_secs_f32(STR_TICK_MS / 1000.0);
        self.playing.retain_mut(|playing| {
            let length = tick * str_ticks(&playing.loaded.animation, playing.length);
            let period = tick * restart_ticks(playing.effect, playing.length.unwrap_or(str_ticks(&playing.loaded.animation, None)));
            while playing.again && now >= playing.start + period {
                let previous = Playing { variants: Vec::new(), again: false, ..playing.clone_now() };
                playing.start += period;
                if let Some(next) = playing.variants.get(pick(0, playing.variants.len().saturating_sub(1) as u32) as usize) {
                    playing.loaded = next.clone();
                }
                if now < previous.start + length {
                    tails.push(previous);
                }
            }
            now < playing.start + tick * playing_ticks(playing.effect, &playing.loaded.animation, playing.length)
        });
        self.playing.extend(tails);

        // ponytail: sprites face south as seen; the few effect sprites drawn in 8 directions would face their unit.
        let facing = usize::from(crate::seen_direction(0.0, yaw, true));
        let sprites = &mut self.sprites;
        self.animations.retain_mut(|animation| {
            let Some(Some(sprite)) = sprites.get_mut(&animation.sprite) else { return false };
            let index = animation.action.unwrap_or(facing) % sprite.actions().max(1);
            let frames = sprite.frames(index).max(1) as f32;
            let frame_ms = animation.frame_ms.unwrap_or_else(|| sprite.index_ms(index).unwrap_or(100.0) / frames);
            // Played once from a later frame, it ends after the frames it shows.
            let pass = (frames - animation.first.min(frames - 1.0)) * frame_ms.max(1.0);
            let elapsed = now.saturating_duration_since(animation.start).as_secs_f32() * 1000.0;
            if animation.end.is_some_and(|end| now >= end) || (!animation.repeat && !animation.hold && elapsed >= pass) {
                return false;
            }
            let progress = animation_progress(elapsed, frame_ms, frames, animation.first, animation.repeat);
            animation.placed = sprite.frame_at(gpu, index, 0, Timing::Span(progress));
            true
        });
        self.auras.retain(|aura| (now.saturating_duration_since(aura.start).as_secs_f32() * 1000.0 / HIT_TICK_MS) < aura.ticks as f32);
        // The angels play action 0 from their launches; the sparkles show particle6's first frame.
        self.blessings.retain_mut(|blessing| {
            let elapsed = now.saturating_duration_since(blessing.start).as_secs_f32() * 1000.0;
            if elapsed / HIT_TICK_MS >= BLESSING_TICKS as f32 {
                return false;
            }
            if let Some(Some(sprite)) = sprites.get_mut(ANGEL_SPRITE) {
                let frames = sprite.frames(0).max(1) as f32;
                for (angel, placed) in blessing.angels.iter_mut().enumerate() {
                    let since = (elapsed - ANGEL_STARTS[angel] as f32 * HIT_TICK_MS).max(0.0);
                    *placed = sprite.frame_at(gpu, 0, 0, Timing::Span(since / ANGEL_FRAME_MS % frames / frames));
                }
            }
            if let Some(Some(sprite)) = sprites.get_mut(BLESSING_SPARKLE) {
                blessing.sparkle = sprite.frame_at(gpu, 0, 0, Timing::Loop(0.0));
            }
            true
        });
        // ponytail: the sparks show particle1's first action, frame 0.
        self.hits.retain_mut(|hit| {
            if hit.tick(now) >= HIT_TICKS {
                return false;
            }
            if let Some(Some(sprite)) = sprites.get_mut(SPARK_SPRITE) {
                hit.placed = sprite.frame_at(gpu, 0, 0, Timing::Loop(0.0));
            }
            true
        });
        for floating in &mut self.floating {
            let Some(Some(sprite)) = floating.sprite.as_ref().and_then(|path| sprites.get_mut(path)) else { continue };
            let index = facing % sprite.actions().max(1);
            let elapsed = if floating.play_sprite { now.saturating_duration_since(floating.start).as_secs_f32() * 1000.0 } else { 0.0 };
            floating.placed = sprite.frame_at(gpu, index, 0, Timing::Loop(elapsed));
        }
    }

    /// Stops everything (another map, where the server tells again of what's in sight); what's loaded stays.
    pub fn clear(&mut self) {
        *self = Self {
            loaded: std::mem::take(&mut self.loaded),
            textures: std::mem::take(&mut self.textures),
            sprites: std::mem::take(&mut self.sprites),
            models: std::mem::take(&mut self.models),
            ..Self::default()
        };
    }

    /// Stops what effect `effect` plays on `unit` (None = the player): its STR and its parts.
    fn stop_effect(&mut self, effect: u32, unit: Option<u32>) {
        let on = |anchor: Anchor| matches!(anchor, Anchor::Unit(each) if each == unit);
        self.playing.retain(|playing| !(playing.effect == effect && on(playing.anchor)));
        self.floating.retain(|floating| !(floating.effect == effect && on(floating.anchor)));
    }

    /// Drops the effects playing on a unit that went away.
    /// Where a skill's ground unit stands, while an effect plays there for it or a model stands on it (a trap).
    pub fn ground_unit_at(&self, unit: u32) -> Option<Vector> {
        let anchors = self.playing.iter().map(|playing| playing.anchor);
        let anchors = anchors.chain(self.floating.iter().map(|floating| floating.anchor)).chain(self.rings.iter().map(|ring| ring.anchor));
        let playing = anchors.chain(self.animations.iter().map(|animation| animation.anchor)).find_map(|anchor| match anchor {
            Anchor::Lasting(id, at) if id == unit => Some(at),
            _ => None,
        });
        playing.or_else(|| standing_at(&self.standing, unit))
    }

    /// The skill unit type a ground unit was shown as.
    pub fn unit_kind(&self, unit: u32) -> Option<u32> {
        self.unit_kinds.get(&unit).copied()
    }

    /// Stops what a status plays on its unit (None = the player).
    pub fn forget_status(&mut self, unit: Option<u32>, status: u16) {
        self.prims.songs.forget_status(unit, status);
        // Camouflage, Stealth Field or Elemental Veil went off: the unit climbs back to opaque.
        if status == 330 {
            self.prims.circles.forget(unit);
        }
        if matches!(status, 360 | 379 | 1301) {
            self.prims.brawl.uncloak(unit, Instant::now());
        }
        let on = |anchor: Anchor| matches!(anchor, Anchor::Status(each, kind) if each == unit && kind == status);
        self.status_prims.retain(|prim| !on(prim.anchor));
        self.auras.retain(|aura| !on(aura.anchor));
        self.playing.retain(|playing| !on(playing.anchor));
        self.floating.retain(|floating| !on(floating.anchor));
        self.rings.retain(|ring| !on(ring.anchor));
        self.animations.retain(|animation| !on(animation.anchor));
    }

    pub fn forget(&mut self, unit: u32) {
        self.prims.sage.forget(unit);
        self.prims.stone.forget(unit);
        self.prims.forget(unit);
        self.hits.retain(|hit| !hit.anchor.on(unit));
        self.status_prims.retain(|prim| !prim.anchor.on(unit));
        self.blessings.retain(|blessing| !blessing.anchor.on(unit));
        self.auras.retain(|aura| !aura.anchor.on(unit));
        self.playing.retain(|playing| !playing.anchor.on(unit));
        self.floating.retain(|floating| !floating.anchor.on(unit));
        self.rings.retain(|ring| !ring.anchor.on(unit));
        self.animations.retain(|animation| !animation.anchor.on(unit));
        self.standing.retain(|(_, id, _)| *id != unit);
    }

    /// The colour an effect gives `unit`'s body at `now` (Berserk Pitcher's red).
    pub fn body_tint(&self, unit: Option<u32>, now: Instant) -> Option<[u8; 3]> {
        let flashing = mix_tints(self.prims.walls.body_tint(unit, now), self.prims.ground.body_tint(unit, now));
        let flashing = mix_tints(flashing, self.prims.guard.body_tint(unit, now));
        let flashing = mix_tints(flashing, self.prims.sniper.body_tint(unit, now));
        let flashing = mix_tints(flashing, self.prims.songs.body_tint(unit, now));
        mix_tints(flashing, self.prims.transcend.body_tint(unit, now))
    }

    /// The body flags and colour an effect gives `unit` now (Full Protection's strobe).
    pub fn body_glow(&self, unit: Option<u32>, now: Instant) -> Option<(u32, Option<[u8; 3]>)> {
        self.prims
            .ground
            .glow(unit, now)
            .or_else(|| self.prims.songs.glow(unit, now))
            .or_else(|| self.prims.knight.glow(unit, now))
            .or_else(|| self.prims.transcend.glow(unit, now))
            .or_else(|| self.prims.sniper.glow(unit, now))
            .or_else(|| self.prims.guard.glow(unit, now))
            .or_else(|| self.prims.brawl.glow(unit, now))
    }

    /// How many degrees `unit` has been turned since this was last asked (Falcon Assault's spin, Chase Walk's cast while it stands).
    pub fn turned(&mut self, unit: Option<u32>, now: Instant, standing: bool) -> f32 {
        self.prims.sniper.turned(unit, now) + self.prims.guard.turned(unit, now, standing) + self.prims.brawl.turned(unit, now)
    }

    /// A cast cut off: Twilight Alchemy's icons go.
    pub fn cut_twilight(&mut self) {
        self.prims.songs.cut_twilight();
        self.prims.guard.cut_pierce();
    }

    /// How opaque an effect draws `unit` now (Camouflage's translucence), 0 to 1.
    pub fn cloak_alpha(&self, unit: Option<u32>, now: Instant) -> f32 {
        self.prims.brawl.cloak_alpha(unit, now).unwrap_or(1.0)
    }

    /// Whether `unit`'s parts shake now (Close Confine's).
    pub fn shakes(&self, unit: Option<u32>, now: Instant) -> bool {
        self.prims.songs.shakes(unit, now) || self.prims.brawl.shaking(unit, now)
    }

    /// Whether effect `effect`'s parts draw: all of them, or with /effect off the force-drawn ones.
    fn shows(&self, effect: u32) -> bool {
        !self.only_forced || force_drawn(effect)
    }

    /// Shapes drawn over everything, with no depth test: Blessing's glow lying flat under the feet, and the songs' puffs.
    pub fn top_meshes(&self, now: Instant, position_of: impl Fn(Option<u32>) -> Option<Vector>) -> Vec<EffectMesh<'_>> {
        let mut meshes = Vec::new();
        // Magnum Break's rings lie over the units and the ground.
        if !self.only_forced {
            let rings = self.prims.first.top_meshes(now);
            meshes.extend(rings.into_iter().filter_map(|(path, triangles)| Some((self.textures.get(path)?.as_ref()?, triangles, false))));
            // Dark Illusion's lines, with no depth test.
            meshes.extend(
                self.prims
                    .brawl
                    .top_meshes(now)
                    .into_iter()
                    .filter_map(|wall| Some((self.textures.get(wall.texture)?.as_ref()?, wall.vertices, wall.additive))),
            );
            // Raid's streaks, with no depth test.
            meshes.extend(
                self.prims.monk.meshes(now, true).into_iter().filter_map(|(path, vertices)| Some((self.textures.get(path)?.as_ref()?, vertices, true))),
            );
        }
        let Some(Some(texture)) = self.textures.get(BLESSING_GLOW).filter(|_| self.shows(42)) else { return meshes };
        let glows = self.blessings.iter().filter(|blessing| blessing.start <= now).filter_map(|blessing| {
            let at = blessing.anchor.place(&position_of)?;
            let alpha = glow_alpha(str_frame(now.duration_since(blessing.start))) / 255.0;
            let tint = [f32::from(GLOW_COLOR[0]) / 255.0, f32::from(GLOW_COLOR[1]) / 255.0, f32::from(GLOW_COLOR[2]) / 255.0, alpha];
            let corner = |x: f32, z: f32, uv: [f32; 2]| MeshVertex { position: [at[0] + x, at[1] + 0.2, at[2] + z], uv, tint };
            let [a, b, c, d] = [
                corner(-GLOW_HALF, -GLOW_HALF, [0.0, 0.0]),
                corner(GLOW_HALF, -GLOW_HALF, [1.0, 0.0]),
                corner(GLOW_HALF, GLOW_HALF, [1.0, 1.0]),
                corner(-GLOW_HALF, GLOW_HALF, [0.0, 1.0]),
            ];
            Some((texture, vec![a, b, c, a, c, d], false))
        });
        meshes.extend(glows);
        meshes
    }

    /// The effect shapes showing now: cylinders and models.
    pub fn meshes(&self, now: Instant, position_of: impl Fn(Option<u32>) -> Option<Vector>) -> Vec<EffectMesh<'_>> {
        let rings = self.rings.iter().filter(|ring| ring.start <= now && self.shows(ring.effect)).filter_map(|ring| {
            let at = ring.anchor.place(&position_of)?;
            let Some(Some(texture)) = self.textures.get(&ring.texture) else { return None };
            Some((texture, ring.mesh(now, at), ring.additive))
        });
        // ponytail: traps' models stand with /effect off too, as the skill units they are.
        let models = self.standing.iter().flat_map(|(model, _, at)| {
            model.0.iter().filter_map(|(path, triangles)| {
                let Some(Some(texture)) = self.textures.get(path) else { return None };
                let place = |position: Vector| std::array::from_fn(|axis| at[axis] + position[axis]);
                Some((texture, triangles.iter().map(|&(position, uv)| MeshVertex { position: place(position), uv, tint: [1.0; 4] }).collect(), false))
            })
        });
        let hits = self.hits.iter().filter(|hit| hit.paths.is_some() && hit.tick(now) < RING_TICKS && self.shows(0)).filter_map(|hit| {
            let at = hit.anchor.place(&position_of)?;
            let Some(Some(texture)) = self.textures.get(RING_TEXTURE) else { return None };
            Some((texture, hit.ring(hit.tick(now), at), false))
        });
        let auras = self.auras.iter().filter(|aura| aura.start <= now && self.shows(aura.effect)).flat_map(|aura| {
            let age = (now.duration_since(aura.start).as_secs_f32() * 1000.0 / HIT_TICK_MS) as u32;
            let (_, color, additive) = aura_look(aura.effect);
            let color = color.map(|value| f32::from(value) / 255.0);
            let at = aura.anchor.place(&position_of);
            (0..aura_sets(aura.effect)).filter_map(move |set| {
                let at = at?;
                let texture = self.textures.get(&aura_set_texture(aura.effect, set)).and_then(Option::as_ref)?;
                let bands = if aura_sets(aura.effect) == 1 { aura.bands(age) } else { aura.bands_in_set(age, set) };
                let triangles = bands.iter().flat_map(|(band, heights)| band_mesh(at, band, |s| heights[s], color)).collect();
                Some((texture, triangles, additive))
            })
        });
        // The bolts' arrows and their impact rings, blended over.
        let volleys = self.prims.volleys.iter().filter_map(|volley| Some((volley, position_of(volley.unit)?)));
        let bolt_meshes: Vec<EffectMesh<'_>> = volleys
            .flat_map(|(volley, feet)| {
                let texture = |path: &str| self.textures.get(path).and_then(Option::as_ref);
                let ring = texture(if volley.fire { bolts::RING_YELLOW } else { bolts::RING_BLUE });
                let arrows = volley.arrows(now, feet).into_iter().filter_map(move |(quads, frame, alpha)| {
                    let texture = texture(if volley.fire { bolts::FIRE_TEXTURES[frame] } else { bolts::ICE_ARROW })?;
                    let tint = [1.0, 1.0, 1.0, alpha];
                    Some((texture, quads.iter().flatten().map(|&(position, uv)| MeshVertex { position, uv, tint }).collect(), false))
                });
                let rings = volley.rings(now, feet).into_iter().filter_map(move |(triangles, alpha)| {
                    let tint = [1.0, 1.0, 1.0, alpha];
                    Some((ring?, triangles.into_iter().map(|(position, uv)| MeshVertex { position, uv, tint }).collect(), false))
                });
                arrows.chain(rings).collect::<Vec<_>>()
            })
            .collect();
        // Blitz Beat's streaks and ring on its target, blended over.
        let blitz_meshes: Vec<EffectMesh<'_>> = self
            .prims
            .blitzes
            .iter()
            .filter_map(|blitz| Some((blitz, position_of(blitz.unit)?)))
            .flat_map(|(blitz, feet)| {
                let texture = |path: &str| self.textures.get(path).and_then(Option::as_ref);
                let streaks = blitz.streaks(now, feet).into_iter().filter_map(|(quads, alpha)| {
                    let tint = [1.0, 1.0, 1.0, alpha];
                    Some((texture(blitz::STREAK)?, quads.iter().flatten().map(|&(position, uv)| MeshVertex { position, uv, tint }).collect(), false))
                });
                let rings = blitz.rings(now, feet).into_iter().filter_map(|(triangles, alpha)| {
                    let tint = [1.0, 1.0, 1.0, alpha];
                    Some((texture(blitz::RING)?, triangles.into_iter().map(|(position, uv)| MeshVertex { position, uv, tint }).collect(), false))
                });
                streaks.chain(rings).collect::<Vec<_>>()
            })
            .collect();
        // The spirit spheres' rings and Huujin's blades, added on.
        let sphere_meshes: Vec<EffectMesh<'_>> = self
            .prims
            .spheres
            .rings(self.textures.get(tanji::RING).and_then(Option::as_ref), now)
            .into_iter()
            .chain(self.prims.huujin.meshes(self.textures.get(huujin::WIND).and_then(Option::as_ref)))
            .chain(self.prims.throws.meshes(|path| self.textures.get(path).and_then(Option::as_ref), self.pitch))
            .collect();
        // The spikes, in ice and in stone, blended over.
        let sight_rings: Vec<EffectMesh<'_>> = match self.textures.get(sight::RING).and_then(Option::as_ref) {
            Some(texture) => self.prims.sights.ring(now).into_iter().map(|triangles| (texture, triangles, false)).collect(),
            None => Vec::new(),
        };
        let first_meshes: Vec<EffectMesh<'_>> = self
            .prims
            .first
            .meshes(now, &position_of)
            .into_iter()
            .filter_map(|(path, triangles)| Some((self.textures.get(path)?.as_ref()?, triangles, false)))
            .collect();
        // The Fire Pillar's funnels draw even with /effect off; the other walls don't.
        let (forced_walls, other_walls): (Vec<_>, Vec<_>) = self.prims.walls.meshes(now, &position_of).into_iter().partition(|wall| wall.forced);
        let wall_mesh = |wall: walls::Wall| Some((self.textures.get(wall.texture)?.as_ref()?, wall.vertices, wall.additive));
        let forced_walls: Vec<EffectMesh<'_>> = forced_walls.into_iter().filter_map(wall_mesh).collect();
        let other_walls: Vec<EffectMesh<'_>> = other_walls.into_iter().filter_map(wall_mesh).collect();
        let spikes = [crate::prims::ICE_TEXTURE, stone::STONE]
            .into_iter()
            .filter_map(|path| Some((self.textures.get(path)?.as_ref()?, self.prims.stone.mesh(path), false)));
        // Turn Undead's ring, flat over the caster's feet.
        let ring = self.textures.get(burst::RING).and_then(Option::as_ref);
        let burst_rings = self.prims.bursts.iter().filter_map(|burst| {
            let (half, alpha) = burst.ring(now)?;
            let [x, y, z] = position_of(burst.unit)?;
            let tint = [1.0, 1.0, 1.0, alpha];
            let corner = |across: f32, along: f32, uv| MeshVertex { position: [x + across * half, y + burst::RING_RISE, z + along * half], uv, tint };
            let [a, b, c, d] = [corner(1.0, 1.0, [0.0, 0.0]), corner(1.0, -1.0, [1.0, 0.0]), corner(-1.0, 1.0, [0.0, 1.0]), corner(-1.0, -1.0, [1.0, 1.0])];
            Some((ring?, vec![a, b, c, d, c, b], false))
        });
        // The Sage's ground looks and loops (Land Protector's square adds on).
        let sage = self.prims.sage.shapes(now).into_iter().filter_map(|(path, additive, corners)| {
            let texture = self.textures.get(path)?.as_ref()?;
            Some((texture, corners.into_iter().map(|(position, uv, alpha)| MeshVertex { position, uv, tint: [1.0, 1.0, 1.0, alpha] }).collect(), additive))
        });
        // The effects made in code are none of them force-drawn.
        let prims = spikes
            .chain(bolt_meshes)
            .chain(blitz_meshes)
            .chain(sphere_meshes)
            .chain(first_meshes)
            .chain(sight_rings)
            .chain(sage)
            .chain(burst_rings)
            .chain(other_walls)
            .chain(
                self.prims
                    .guard
                    .meshes(now, &position_of)
                    .into_iter()
                    .filter_map(|wall| Some((self.textures.get(wall.texture)?.as_ref()?, wall.vertices, wall.additive))),
            )
            .chain(
                self.prims
                    .dancer
                    .meshes(now, &position_of)
                    .into_iter()
                    .filter_map(|wall| Some((self.textures.get(wall.texture)?.as_ref()?, wall.vertices, wall.additive))),
            )
            .chain(
                self.prims
                    .sniper
                    .meshes(now, &position_of)
                    .into_iter()
                    .filter_map(|wall| Some((self.textures.get(wall.texture)?.as_ref()?, wall.vertices, wall.additive))),
            )
            .chain(
                self.prims
                    .transcend
                    .meshes(now, &position_of)
                    .into_iter()
                    .filter_map(|wall| Some((self.textures.get(wall.texture)?.as_ref()?, wall.vertices, wall.additive))),
            )
            .chain(
                self.prims
                    .knight
                    .meshes(now, &position_of)
                    .into_iter()
                    .filter_map(|wall| Some((self.textures.get(wall.texture)?.as_ref()?, wall.vertices, wall.additive))),
            )
            .chain(
                self.prims.shafts.meshes(now).into_iter().filter_map(|wall| Some((self.textures.get(wall.texture)?.as_ref()?, wall.vertices, wall.additive))),
            )
            .chain(
                self.prims
                    .brawl
                    .meshes(now, &position_of)
                    .into_iter()
                    .filter_map(|wall| Some((self.textures.get(wall.texture)?.as_ref()?, wall.vertices, wall.additive))),
            )
            .chain(
                self.prims
                    .songs
                    .meshes(now, &position_of)
                    .into_iter()
                    .filter_map(|wall| Some((self.textures.get(wall.texture)?.as_ref()?, wall.vertices, wall.additive))),
            )
            .chain(
                self.prims.ground.meshes(now).into_iter().filter_map(|wall| Some((self.textures.get(wall.texture)?.as_ref()?, wall.vertices, wall.additive))),
            )
            .chain(self.prims.monk.meshes(now, false).into_iter().filter_map(|(path, vertices)| Some((self.textures.get(path)?.as_ref()?, vertices, true))))
            .chain(self.prims.shards.meshes(now).into_iter().filter_map(|(path, vertices)| Some((self.textures.get(path)?.as_ref()?, vertices, true))))
            .filter(|_| !self.only_forced);
        rings.chain(models).chain(hits).chain(auras).chain(forced_walls).chain(prims).collect()
    }

    /// What's drawn flat on the screen over units now (COLDHIT's shards and smoke): texture, the unit's feet, and the
    /// triangles' corners in screen pixels from there.
    pub fn screen_shapes(&self, now: Instant, position_of: impl Fn(Option<u32>) -> Option<Vector>) -> Vec<world::ScreenShape<'_>> {
        if self.only_forced {
            return Vec::new();
        }
        self.prims
            .screen_shapes(now)
            .into_iter()
            .filter_map(|(path, unit, corners)| Some((self.textures.get(path)?.as_ref()?, position_of(unit)?, corners)))
            .chain(self.prims.fixed_shapes(now).into_iter().filter_map(|(at, (path, _, corners))| Some((self.textures.get(path)?.as_ref()?, at, corners))))
            .collect()
    }

    /// Every effect quad showing now; `position_of` gives a unit's feet (None = the player).
    pub fn quads(&self, now: Instant, position_of: impl Fn(Option<u32>) -> Option<Vector>) -> Vec<EffectQuad<'_>> {
        let mut quads = Vec::new();
        for floating in self.floating.iter().filter(|floating| floating.start <= now && self.shows(floating.effect)) {
            let Some(at) = floating.anchor.place(&position_of) else { continue };
            let length = floating.end.duration_since(floating.start).as_secs_f32();
            let elapsed = now.duration_since(floating.start).as_secs_f32();
            let steps = (elapsed / length * 100.0).min(100.0);
            let [across, along, up] = std::array::from_fn(|axis| between(floating.position[axis], steps, floating.smooth[axis]));
            let anchor = [at[0] + across * crate::world::CELL, at[1] + up * crate::world::CELL, at[2] + along * crate::world::CELL];
            let quarter = length / 4.0;
            let alpha = if floating.fade_in && elapsed < quarter {
                floating.alpha * elapsed / quarter
            } else if floating.fade_out && elapsed > length - quarter {
                floating.alpha * (length - elapsed) / quarter
            } else {
                floating.alpha
            };
            let [width, height] = [0, 1].map(|dimension| between(floating.size[dimension], steps, floating.size_smooth));
            let angle = between(floating.angle, steps, false).to_radians();
            let (sin, cos) = angle.sin_cos();
            let corner = |[x, y]: [f32; 2]| [x * cos - y * sin, x * sin + y * cos];
            let [red, green, blue] = floating.color;
            let color = [red, green, blue, alpha.clamp(0.0, 1.0)];
            // A sprite's images, sized in percent about its feet.
            if let Some(Some(Some(sprite))) = floating.sprite.as_ref().map(|path| self.sprites.get(path)) {
                let scale = [width / 100.0, height / 100.0];
                for &(key, position, size, layer_angle) in &floating.placed {
                    let corners = QUAD_UV.map(|uv| {
                        let [x, y] = world::sprite_corner(position, size, layer_angle, uv).0;
                        corner([x * scale[0], y * scale[1]])
                    });
                    quads.push((sprite.texture(key), anchor, corners, color, floating.additive, false));
                }
                continue;
            }
            let frame = (elapsed * 1000.0 / floating.frame_ms) as usize % floating.textures.len().max(1);
            let Some(Some(texture)) = floating.textures.get(frame).and_then(|path| self.textures.get(path)) else { continue };
            // A ray reaches out from the center; anything else is centered on it.
            let top = if floating.ray { -height } else { -height / 2.0 };
            let corners = [[-width / 2.0, top], [width / 2.0, top], [width / 2.0, top + height], [-width / 2.0, top + height]].map(corner);
            quads.push((texture, anchor, corners, color, floating.additive, false));
        }
        // Heal's sparkles rising and fading.
        if let Some(Some(texture)) = self.textures.get(SPARKLE_TEXTURE) {
            for aura in self.auras.iter().filter(|aura| aura.start <= now && !aura.sparkles.is_empty() && self.shows(aura.effect)) {
                let Some(at) = aura.anchor.place(&position_of) else { continue };
                let age = (now.duration_since(aura.start).as_secs_f32() * 1000.0 / HIT_TICK_MS) as u32;
                for &(start, [x, z]) in &aura.sparkles {
                    let Some((up, alpha)) = age.checked_sub(start).and_then(sparkle_at) else { continue };
                    let half = SPARKLE_SIZE / 2.0;
                    quads.push((
                        texture,
                        [at[0] + x, at[1] + up, at[2] + z],
                        [[-half, -half], [half, -half], [half, half], [-half, half]],
                        [1.0, 1.0, 1.0, alpha],
                        true,
                        false,
                    ));
                }
            }
        }
        // Steel Body's glints (four sets) and Explosion Spirits' (one): camera-facing squares round the feet, added on in #6464FF.
        for prim in self.status_prims.iter().filter(|prim| prim.kind == PrimKind::Glints && prim.start <= now && self.shows(prim.effect)) {
            let Some(at) = prim.anchor.place(&position_of) else { continue };
            // A rider's glints sit higher, a baby's or a Doram's lower.
            let lift = prim.anchor.unit().and_then(|unit| self.rises.get(&unit)).copied().unwrap_or(0.0);
            let age = str_frame(now.duration_since(prim.start));
            let half = GLINT_RADIUS * std::f32::consts::FRAC_1_SQRT_2 * PIXELS_PER_UNIT;
            for glint in 0..4 * glint_sets(prim.effect) {
                let (alpha, round) = glint_at(GLINT_ALPHAS[(glint % 4) as usize], age);
                let pick = |what: u32| noise(prim.seed, glint * 1_000 + round, what);
                let Some(Some(texture)) = self.textures.get(GLINT_TEXTURES[(pick(0) % 5) as usize]) else { continue };
                let (sin, cos) = ((pick(1) % 360) as f32).to_radians().sin_cos();
                let height = if round == 0 { (pick(2) % 10) as f32 } else { 3.0 + (pick(2) % 8) as f32 };
                let place = [at[0] + GLINT_CIRCLE * cos, at[1] + height + lift, at[2] + GLINT_CIRCLE * sin];
                let [red, green, blue] = GLINT_COLOR.map(|value| f32::from(value) / 255.0);
                let corners = [[-half, -half], [half, -half], [half, half], [-half, half]];
                quads.push((texture, place, corners, [red, green, blue, alpha as f32 / 255.0], true, false));
            }
        }
        // Blessing's angels (25 units up, sized like a unit's sprite) and its sparkles.
        for blessing in self.blessings.iter().filter(|blessing| blessing.start <= now && self.shows(42)) {
            let Some(at) = blessing.anchor.place(&position_of) else { continue };
            let tick = str_frame(now.duration_since(blessing.start));
            let place = |offset: Vector| std::array::from_fn(|axis| at[axis] + offset[axis]);
            if let Some(Some(sprite)) = self.sprites.get(ANGEL_SPRITE) {
                for (angel, placed) in blessing.angels.iter().enumerate() {
                    let Some(alpha) = angel_alpha(angel, tick) else { continue };
                    for &(key, position, size, layer_angle) in placed {
                        let corners = QUAD_UV.map(|uv| world::sprite_corner(position, size, layer_angle, uv).0);
                        quads.push((sprite.texture(key), place([0.0, ANGEL_RISE, 0.0]), corners, [1.0, 1.0, 1.0, alpha / 255.0], false, false));
                    }
                }
            }
            if let Some(Some(sprite)) = self.sprites.get(BLESSING_SPARKLE) {
                for (start, &heading) in sparkle_starts().zip(&blessing.sparkles) {
                    let Some((offset, alpha)) = tick.checked_sub(start).and_then(|since| sparkle_place(heading, since)) else { continue };
                    for &(key, position, size, layer_angle) in &blessing.sparkle {
                        let corners = QUAD_UV.map(|uv| world::sprite_corner(position, size, layer_angle, uv).0);
                        quads.push((sprite.texture(key), place(offset), corners, [1.0, 1.0, 1.0, alpha], false, false));
                    }
                }
            }
        }
        // The hits' sparks, each with its trail, sized like a unit's sprite; /mineffect leaves the ring alone.
        if let Some(Some(sprite)) = self.sprites.get(SPARK_SPRITE).filter(|_| self.shows(0) && !self.min) {
            for hit in &self.hits {
                let (Some(paths), Some(at)) = (&hit.paths, hit.anchor.place(&position_of)) else { continue };
                for (spark, path) in hit.sparks.iter().zip(paths) {
                    for (place, alpha, scale) in spark.trail(hit.tick(now), path) {
                        let anchor = std::array::from_fn(|axis| at[axis] + place[axis]);
                        for &(key, position, size, layer_angle) in &hit.placed {
                            let corners = QUAD_UV.map(|uv| world::sprite_corner(position, size, layer_angle, uv).0.map(|value| value * scale));
                            quads.push((sprite.texture(key), anchor, corners, [1.0, 1.0, 1.0, alpha], false, false));
                        }
                    }
                }
            }
        }
        for animation in self.animations.iter().filter(|animation| self.shows(animation.effect)) {
            let Some(at) = animation.anchor.place(&position_of) else { continue };
            let Some(Some(sprite)) = self.sprites.get(&animation.sprite) else { continue };
            let [x, y] = animation.offset;
            for &(key, position, size, layer_angle) in &animation.placed {
                // Over everything, `STR_RISE` is taken off again.
                let rise = if animation.over { STR_RISE } else { 0.0 };
                let corners = QUAD_UV.map(|uv| {
                    let [left, top] = world::sprite_corner(position, size, layer_angle, uv).0;
                    [left * animation.scale + x, top * animation.scale + y + rise]
                });
                quads.push((sprite.texture(key), at, corners, [1.0, 1.0, 1.0, animation.alpha], false, animation.over));
            }
        }
        for playing in self.playing.iter().filter(|playing| playing.start <= now && self.shows(playing.effect)) {
            let Some(at) = playing.anchor.place(&position_of) else { continue };
            let animation = &playing.loaded.animation;
            // Whole ticks: the state changes once each 16 ms, with nothing in between.
            let frame = str_frame(now.duration_since(playing.start)) as f32;
            for (index, (layer, textures)) in animation.layers.iter().zip(&playing.loaded.textures).enumerate() {
                let state = layer.state(frame).or_else(|| kept_layer_frame(playing.effect, index, layer, frame).and_then(|kept| layer.state(kept)));
                let Some(state) = state else { continue };
                let Some(Some(texture)) = textures.get(state.texture) else { continue };
                // In sprite pixels, y down, turned by the key's angle; drawn over everything, `STR_RISE` up.
                // (Drawn `STR_RISE` up, so an anchor with less rise is moved down by the difference.)
                let drop = STR_RISE - str_rise(playing.effect);
                let corner = |index: usize| {
                    let [x, y] = str_corner(state.position, [state.corners[index], state.corners[4 + index]], state.angle);
                    [x, y + drop]
                };
                quads.push((texture, at, [corner(0), corner(1), corner(2), corner(3)], state.color, state.additive, true));
            }
        }
        // The effects made in code are none of them force-drawn.
        if self.only_forced {
            return quads;
        }
        // Fire Bolt's sparks.
        if let Some(Some(sprite)) = self.sprites.get(bolts::SPARK_SPRITE).filter(|_| !self.spark_frames.is_empty()) {
            for volley in &self.prims.volleys {
                let Some(feet) = position_of(volley.unit) else { continue };
                for (at, frame, scale, alpha) in volley.sparks(now) {
                    let anchor = std::array::from_fn(|axis| feet[axis] + at[axis]);
                    for &(key, position, size, layer_angle) in &self.spark_frames[frame as usize % self.spark_frames.len()] {
                        let corners = QUAD_UV.map(|uv| world::sprite_corner(position, size, layer_angle, uv).0.map(|value| value * scale));
                        quads.push((sprite.texture(key), anchor, corners, [1.0, 1.0, 1.0, alpha], false, false));
                    }
                }
            }
        }
        // Arrows, each in the direction of its flight as the camera sees it.
        if let Some(Some(sprite)) = self.sprites.get(crate::arrows::SPRITE) {
            for (at, [x, z]) in self.arrows.flying(now) {
                let seen = crate::actor::view_direction(x.atan2(-z).to_degrees(), self.yaw, true);
                for &(key, position, size, layer_angle) in self.arrow.get(usize::from(seen)).into_iter().flatten() {
                    let corners = QUAD_UV.map(|uv| world::sprite_corner(position, size, layer_angle, uv).0);
                    quads.push((sprite.texture(key), at, corners, [1.0; 4], false, false));
                }
            }
        }
        // Soul Strike's ghosts, each copy of a trail a little fainter.
        for (path, layers, red) in [(crate::prims::GHOST_SPRITE, &self.ghost, false), (crate::prims::RED_GHOST_SPRITE, &self.red_ghost, true)] {
            let Some(Some(sprite)) = self.sprites.get(path) else { continue };
            for ghost in self.prims.ghosts().into_iter().filter(|ghost| ghost.red == red) {
                for &(key, position, size, layer_angle) in layers {
                    let corners = QUAD_UV.map(|uv| world::sprite_corner(position, size, layer_angle + ghost.turn, uv).0.map(|value| value * ghost.scale));
                    quads.push((sprite.texture(key), ghost.at, corners, [ghost.tint[0], ghost.tint[1], ghost.tint[2], ghost.alpha], false, false));
                }
            }
        }
        let fire_ball = self.sprites.get(balls::FIRE_BALL_SPRITE).and_then(Option::as_ref);
        let spear = (self.sprites.get(balls::SPEAR_SPRITE).and_then(Option::as_ref), self.spear.as_slice());
        quads.extend(self.prims.boomerangs.quads(self.textures.get(boomerang::SHIELD).and_then(Option::as_ref)));
        if !self.only_forced {
            quads.extend(self.prims.spheres.quads(self.textures.get(tanji::SPHERE).and_then(Option::as_ref)));
            // Sight Rasher's sprites and their shadows, white with their alpha.
            for drawn in self.prims.sights.sprites(now) {
                let Some(Some(sprite)) = self.sprites.get(drawn.sprite) else { continue };
                for &(key, position, size, layer_angle) in self.sight_frames.get(&(drawn.sprite, drawn.frame)).into_iter().flatten() {
                    let corners = QUAD_UV.map(|uv| world::sprite_corner(position, size, layer_angle, uv).0.map(|value| value * drawn.scale));
                    quads.push((sprite.texture(key), drawn.at, corners, [1.0, 1.0, 1.0, drawn.alpha.trunc().clamp(0.0, 255.0) / 255.0], false, false));
                }
            }
            // The claw slash: white at alpha 200, 2.5 times as big, over everything. `STR_RISE` is taken off again.
            for draw in self.prims.monk.claws(now) {
                let Some(Some(sprite)) = self.sprites.get(draw.sprite) else { continue };
                for &(key, position, size, layer_angle) in self.claw_frames.get(&(draw.sprite, draw.action, draw.frame)).into_iter().flatten() {
                    let corners = QUAD_UV.map(|[u, v]| {
                        let uv = if draw.mirrored { [1.0 - u, v] } else { [u, v] };
                        let [x, y] = world::sprite_corner(position, size, layer_angle + draw.turn, uv).0;
                        [x * draw.scale, y * draw.scale + STR_RISE]
                    });
                    quads.push((sprite.texture(key), draw.at, corners, [1.0, 1.0, 1.0, draw.alpha / 255.0], false, true));
                }
            }
            // The songs' puffs: upright squares turned by their yaws, 10 above the feet, added on and drawn over everything with
            // no depth, their corners projected to the screen.
            if let Some(Some(texture)) = self.textures.get(PUFF_TEXTURE) {
                for prim in self.status_prims.iter().filter(|prim| prim.start <= now) {
                    let PrimKind::Puffs(variant) = prim.kind else { continue };
                    let age = str_frame(now.duration_since(prim.start));
                    let Some(at) = prim.anchor.place(&position_of).filter(|_| age < PUFF_TICKS) else { continue };
                    for (puff, &first) in PUFF_STARTS.iter().enumerate() {
                        let Some((r, alpha, pulse, yaw)) = puff_at(prim.seed, puff as u32, i64::from(age) - i64::from(first)) else { continue };
                        let half = r * (0.9 + 0.1 * pulse.to_radians().sin()) / 2.0;
                        let [red, green, blue] = puff_color(variant, puff).map(|value| f32::from(value) / 255.0);
                        let (sin, cos) = yaw.to_radians().sin_cos();
                        let corner = |across: f32, up: f32| {
                            let [x, y] = screen_offset([across * cos, 10.0 + up, across * sin], self.yaw, self.pitch);
                            [x, y + STR_RISE]
                        };
                        // Texture coordinates (0, 0), (1, 0), (1, 1), (0, 1) on the corners a, b, c, d: the quad's order is d, c, b, a.
                        let corners = [corner(-half, -half), corner(half, -half), corner(half, half), corner(-half, half)];
                        quads.push((texture, at, corners, [red, green, blue, alpha / 255.0], true, true));
                    }
                }
            }
            // Cart Termination's dots: soft squares facing the camera, added on.
            if let Some(Some(texture)) = self.textures.get(spray::DOT) {
                for dot in self.prims.spray.dots(now, &position_of) {
                    let half = dot.half * PIXELS_PER_UNIT;
                    let corners = [[-half, half], [half, half], [half, -half], [-half, -half]];
                    quads.push((texture, dot.at, corners, [dot.tint[0], dot.tint[1], dot.tint[2], dot.alpha / 255.0], true, false));
                }
            }
            // Shield Chain's shields and their ghosts: white squares facing the camera.
            if let Some(Some(texture)) = self.textures.get(spray::SHIELD) {
                for shield in self.prims.spray.shields(now, self.dim) {
                    let corners = ground::plain_square(shield.spin, spray::Spray::shield_half() * PIXELS_PER_UNIT, false);
                    quads.push((texture, shield.at, corners, [1.0, 1.0, 1.0, shield.alpha.trunc().clamp(0.0, 255.0) / 255.0], false, false));
                }
            }
            // Full Strip's shields and swords: additive squares facing the camera, red.
            for piece in self.prims.guard.pieces(now) {
                let Some(Some(texture)) = self.textures.get(piece.potion.icon) else { continue };
                let corners = ground::plain_square(piece.potion.spin, piece.half * PIXELS_PER_UNIT, false);
                let [red, green, blue] = guard::STRIP_TINT;
                quads.push((texture, piece.potion.at, corners, [red, green, blue, piece.potion.alpha.trunc().clamp(0.0, 255.0) / 255.0], true, false));
            }
            // Gospel's cross over its cell: a square facing the camera, added on.
            if let Some(Some(texture)) = self.textures.get(transcend::CROSS) {
                for cross in self.prims.transcend.crosses(now, self.dim) {
                    quads.push((texture, cross.at, cross.corners, [1.0, 1.0, 1.0, cross.alpha / 255.0], true, false));
                }
            }
            // Pressure's cross and its ghosts: additive squares facing the camera.
            for cross in self.prims.knight.crosses(now, &position_of) {
                let Some(Some(texture)) = self.textures.get(cross.icon) else { continue };
                let corners = ground::plain_square(cross.spin, 12.0 * PIXELS_PER_UNIT, true);
                quads.push((texture, cross.at, corners, [1.0, 1.0, 1.0, cross.alpha.trunc().clamp(0.0, 255.0) / 255.0], true, false));
            }
            // Twilight Alchemy's icons: additive squares facing the camera, the picture in the top left three quarters.
            for icon in self.prims.songs.icons() {
                let Some(Some(texture)) = self.textures.get(icon.picture) else { continue };
                let corners = ground::padded_square(icon.spin, icon.half * PIXELS_PER_UNIT, false);
                quads.push((texture, icon.at, corners, [1.0, 1.0, 1.0, icon.alpha.trunc().clamp(0.0, 255.0) / 255.0], true, false));
            }
            // Slim Pitcher's potion and its ghosts: a square facing the camera, the icon in its top left three quarters.
            for potion in self.prims.ground.potions(now) {
                let Some(Some(texture)) = self.textures.get(potion.icon) else { continue };
                let corners = ground::padded_square(potion.spin, ground::POTION_HALF * PIXELS_PER_UNIT, true);
                let tint = [1.0, 135.0 / 255.0, 135.0 / 255.0, potion.alpha.trunc().clamp(0.0, 255.0) / 255.0];
                quads.push((texture, potion.at, corners, tint, false, false));
            }
            // Chemical Protection's star: rays facing the camera, added on.
            if let Some(Some(texture)) = self.textures.get(star::TEXTURE) {
                for triangle in self.prims.stars.triangles(now) {
                    quads.push((texture, triangle.at, triangle.corners, triangle.tint, true, false));
                }
            }
            // Water Ball's balls and their tails, white with their alpha.
            if let Some(Some(sprite)) = self.sprites.get(waterball::SPRITE) {
                for drawn in self.prims.waterballs.drawn(now) {
                    for &(key, position, size, layer_angle) in self.water_frames.get(&drawn.image).into_iter().flatten() {
                        let corners = QUAD_UV.map(|uv| world::sprite_corner(position, size, layer_angle, uv).0.map(|value| value * drawn.scale));
                        quads.push((sprite.texture(key), drawn.at, corners, [1.0, 1.0, 1.0, drawn.alpha.trunc().clamp(0.0, 255.0) / 255.0], false, false));
                    }
                }
            }
            // Holy Water's bottle: every layer plain white at alpha 254.
            if let Some(Some(sprite)) = self.sprites.get(first::HOLY_WATER_SPRITE) {
                for (at, frame) in self.prims.first.holy_waters(now, &position_of) {
                    for &(key, position, size, layer_angle) in self.holy_frames.get(&frame).into_iter().flatten() {
                        let corners = QUAD_UV.map(|uv| world::sprite_corner(position, size, layer_angle, uv).0);
                        quads.push((sprite.texture(key), at, corners, [1.0; 4], false, false));
                    }
                }
            }
            // First Aid's glow, Sprinkle Sand's specks and the undead heal's sparkles: camera-facing squares, added on.
            for square in self.prims.first.squares(now, &position_of).into_iter().chain(self.prims.monk.squares(now)) {
                let Some(Some(texture)) = self.textures.get(square.texture) else { continue };
                let pixels = square.radius * 35.0 / world::CELL;
                let corner = |j: u32| {
                    let (sin, cos) = (square.spin + 90.0 * j as f32).to_radians().sin_cos();
                    [pixels * cos, pixels * sin]
                };
                // Plain: corners 0 to 3 at texture (0, 1), (1, 1), (1, 0), (0, 0); mirrored: (1, 1), (0, 1), (0, 0), (1, 0).
                let corners = if square.mirrored { [corner(2), corner(3), corner(0), corner(1)] } else { [corner(3), corner(2), corner(1), corner(0)] };
                let alpha = if self.dim { square.alpha.min(100.0) } else { square.alpha };
                let [red, green, blue] = square.tint.map(|channel| f32::from(channel) / 255.0);
                quads.push((texture, square.at, corners, [red, green, blue, alpha.trunc().clamp(0.0, 255.0) / 255.0], true, false));
            }
            // Poison Smoke's sparkles: the sprite particles again, over everything with no depth test.
            for particle in self.prims.walls.smoke_particles(now) {
                let Some(Some(sprite)) = self.sprites.get(particle.sprite) else { continue };
                for &(key, position, size, layer_angle) in self.stars.get(particle.sprite).into_iter().flatten() {
                    let corners = QUAD_UV.map(|uv| {
                        let [x, y] = world::sprite_corner(position, size, layer_angle + particle.spin, uv).0;
                        [x * particle.scale, y * particle.scale + STR_RISE]
                    });
                    quads.push((sprite.texture(key), particle.at, corners, [1.0, 1.0, 1.0, particle.alpha.trunc().clamp(0.0, 255.0) / 255.0], false, true));
                }
            }
            // The first jobs' sparkles and sparks.
            for particle in self.prims.first.particles(now, &position_of).into_iter().chain(self.prims.walls.particles(now)) {
                let Some(Some(sprite)) = self.sprites.get(particle.sprite) else { continue };
                for &(key, position, size, layer_angle) in self.stars.get(particle.sprite).into_iter().flatten() {
                    let corners = QUAD_UV.map(|uv| world::sprite_corner(position, size, layer_angle + particle.spin, uv).0.map(|value| value * particle.scale));
                    quads.push((sprite.texture(key), particle.at, corners, [1.0, 1.0, 1.0, particle.alpha.trunc().clamp(0.0, 255.0) / 255.0], false, false));
                }
            }
        }
        quads.extend(self.prims.balls.quads(fire_ball, &self.fire_ball, spear, &self.textures, self.yaw));
        quads.extend(self.prims.circles.quads(now, |path| self.textures.get(path).and_then(Option::as_ref), &position_of));
        quads.extend(self.prims.sparkle_quads(now, self.dim, |path| self.textures.get(path).and_then(Option::as_ref), &position_of));
        quads
    }
}

fn load(gpu: &Gpu, grf: &Grf, file: &str, folder: &str) -> Option<Rc<Loaded>> {
    let animation = Str::parse(&grf.read(&format!("data\\texture\\effect\\{file}.str")).ok()?).ok()?;
    let textures = animation
        .layers
        .iter()
        .map(|layer| layer.textures.iter().map(|name| gpu.try_load_texture(grf, &format!("data\\texture\\effect\\{folder}{name}"), true).ok()).collect())
        .collect();
    Some(Rc::new(Loaded { animation, textures }))
}

/// A model under data\model\ (an RSM 1 named without .rsm, an RSM 2 with its .rsm2), its triangles grouped by
/// texture.
fn load_model(grf: &Grf, file: &str) -> Option<Model> {
    let path = if file.to_lowercase().ends_with(".rsm2") { format!("data\\model\\{file}") } else { format!("data\\model\\{file}.rsm") };
    let rsm = Rsm::parse(&grf.read(&path).ok()?).ok()?;
    let mut model = Model(rsm.textures.iter().map(|name| (format!("data\\texture\\{name}"), Vec::new())).collect());
    for triangle in rsm.triangles() {
        // Model space has y down.
        model.0[triangle.texture].1.extend(triangle.positions.into_iter().zip(triangle.uvs).map(|([x, y, z], uv)| ([x, -y, z], uv)));
    }
    Some(model)
}

/// Blind is health bit 0x10. Deep Blind is status 1205.
pub const BLIND_BIT: u16 = 0x10;
pub const DEEP_BLIND: u16 = 1205;
/// The overlay's centre sits this many world units above the feet.
pub const BLIND_ABOVE: f32 = 9.0;
/// Each inner square's side, in world units. The bands start here.
pub const BLIND_INNER: f32 = 65.0;
/// The bands' outer edge. The overlay is twice this across.
pub const BLIND_OUTER: f32 = 165.0;
/// #0A0A0A.
const BLIND_RGB: f32 = 10.0 / 255.0;

/// Overlay alpha after this many 16 ms ticks: it starts at 0 and gains 1 a tick, held at 255.
pub fn blind_alpha(ticks: u32) -> f32 {
    (ticks.min(255) as f32) / 255.0
}

/// The overlay's colour at this fade alpha.
pub fn blind_color(alpha: f32) -> [f32; 4] {
    [BLIND_RGB, BLIND_RGB, BLIND_RGB, alpha]
}

/// When the own character's overlay started. Another packet while it shows does not start the fade again.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BlindView {
    since: Option<Instant>,
}

impl BlindView {
    /// `own` is the player's character. `on` is health bit 0x10 or status 1205. Other units change nothing.
    pub fn update(&mut self, own: bool, on: bool, now: Instant) {
        if !own {
            return;
        }
        if on {
            self.since.get_or_insert(now);
        } else {
            self.since = None;
        }
    }

    pub fn showing(&self) -> bool {
        self.since.is_some()
    }

    /// Ticks since it started, or 0 while it's off. Not capped at the fade.
    pub fn ticks(&self, now: Instant) -> u32 {
        let Some(since) = self.since else { return 0 };
        u32::try_from(now.saturating_duration_since(since).as_millis() / 16).unwrap_or(u32::MAX)
    }

    pub fn alpha(&self, now: Instant) -> f32 {
        if self.showing() { blind_alpha(self.ticks(now)) } else { 0.0 }
    }
}

/// One piece of the overlay, in world units from its centre (x right, y down on the camera): corners top left, top
/// right, bottom right, bottom left, and each corner's texture coordinates. `fullb` is fullb.tga; the bands are white02.bmp.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlindPiece {
    pub corners: [[f32; 2]; 4],
    pub uv: [[f32; 2]; 4],
    pub fullb: bool,
}

/// The centre of the overlay: 9 world units above the feet. World y is up.
pub fn blind_center(feet: Vector) -> Vector {
    [feet[0], feet[1] + BLIND_ABOVE, feet[2]]
}

/// Four squares of side 65 around the centre, then four bands from 65 out to 165. Each square's texture coordinate
/// (0, 0) — fullb.tga's clear corner — sits on the centre.
pub fn blind_pieces() -> [BlindPiece; 8] {
    let (s, o) = (BLIND_INNER, BLIND_OUTER);
    let square = |corners: [[f32; 2]; 4], uv: [[f32; 2]; 4]| BlindPiece { corners, uv, fullb: true };
    let band = |corners: [[f32; 2]; 4]| BlindPiece { corners, uv: QUAD_UV, fullb: false };
    [
        square([[0.0, 0.0], [s, 0.0], [s, s], [0.0, s]], [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]),
        square([[-s, 0.0], [0.0, 0.0], [0.0, s], [-s, s]], [[1.0, 0.0], [0.0, 0.0], [0.0, 1.0], [1.0, 1.0]]),
        square([[0.0, -s], [s, -s], [s, 0.0], [0.0, 0.0]], [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]]),
        square([[-s, -s], [0.0, -s], [0.0, 0.0], [-s, 0.0]], [[1.0, 1.0], [0.0, 1.0], [0.0, 0.0], [1.0, 0.0]]),
        band([[-o, -o], [o, -o], [o, -s], [-o, -s]]),
        band([[-o, s], [o, s], [o, o], [-o, o]]),
        band([[-o, -s], [-s, -s], [-s, s], [-o, s]]),
        band([[s, -s], [o, -s], [o, s], [s, s]]),
    ]
}

/// A piece's corners in sprite pixels (y down). One world unit is 35/CELL sprite pixels.
pub fn blind_offsets(piece: &BlindPiece) -> [[f32; 2]; 4] {
    let scale = 35.0 / CELL;
    piece.corners.map(|[x, y]| [x * scale, y * scale])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stands_the_four_trap_models() {
        let table = Table::load();
        let models: Vec<&str> = (20852..=20855).filter_map(|kind| table.models.get(&kind).map(String::as_str)).collect();
        assert_eq!(
            models,
            [
                "effect\\deepblindtrap\\deep_trap.rsm2",
                "effect\\solidtrap\\sol_trap.rsm2",
                "effect\\swifttrap\\swi_trap.rsm2",
                "effect\\flametrap\\fla_trap.rsm2"
            ]
        );
    }

    #[test]
    fn ground_units_hit_with_their_own_effect() {
        // Storm Gust COLDHIT, Lord of Vermilion WINDHIT, Fire Wall FIREHIT, Heaven's Drive EF_HIT1, none for new jobs.
        assert_eq!([89, 85, 18, 91, 2000].map(unit_hit_effect), [Some(51), Some(52), Some(49), Some(0), None]);
        // Napalm Beat's splash targets still get the whole hit; Fire Ball's don't.
        assert!(SPLASH_FULL.contains(&11) && !SPLASH_FULL.contains(&17));
    }

    #[test]
    fn bolts_drop_an_arrow_a_hit() {
        let ms = |drops: Vec<Duration>| drops.into_iter().map(|drop| drop.as_millis()).collect::<Vec<_>>();
        // Fire Bolt level 5: five arrows, 192 ms in and 160 ms apart; Cold Bolt alike; other effects play once.
        assert_eq!(ms(bolt_drops(31, 5)), [192, 352, 512, 672, 832]);
        assert_eq!(ms(bolt_drops(26, 1)), [192]);
        assert_eq!(bolt_drops(26, 0).len(), 1);
        assert_eq!(bolt_drops(26, 99).len(), 50);
        assert!(bolt_drops(29, 10).is_empty() && bolt_drops(24, 5).is_empty());
    }

    #[test]
    fn sight_rasher_stops_the_casters_sight() {
        let now = Instant::now();
        let floating = |effect: u32, unit: Option<u32>| Floating {
            textures: Vec::new(),
            frame_ms: 0.0,
            sprite: None,
            play_sprite: false,
            placed: Vec::new(),
            again: false,
            start: now,
            end: now,
            anchor: Anchor::Unit(unit),
            position: [[0.0; 2]; 3],
            smooth: [false; 3],
            size: [[0.0; 2]; 2],
            size_smooth: false,
            alpha: 1.0,
            fade_in: false,
            fade_out: false,
            color: [1.0; 3],
            angle: [0.0; 2],
            ray: false,
            additive: false,
            effect,
        };
        // Our Sight, someone else's, and our Heal.
        let mut effects = Effects { floating: vec![floating(SIGHT, None), floating(SIGHT, Some(7)), floating(7, None)], ..Effects::default() };
        effects.stop_effect(SIGHT, None);
        assert_eq!(
            effects.floating.iter().map(|floating| (floating.effect, matches!(floating.anchor, Anchor::Unit(None)))).collect::<Vec<_>>(),
            [(SIGHT, false), (7, true)]
        );
    }

    #[test]
    fn rolls_safety_walls_and_the_undead_heals_walls() {
        let banded = |effect: u32, turns: [f32; 3], extra: [f32; 2]| Banded {
            effect,
            anchor: Anchor::At([0.0; 3]),
            start: Instant::now(),
            ticks: 100,
            turns,
            extra,
            sparkles: Vec::new(),
        };
        // Safety Wall: three nested walls (34, 37, 40 tall; radii 3.6, 3.3, 3.0), lean 90, closed.
        let safety = banded(315, [10.0, 20.0, 30.0], [0.0; 2]);
        let walls = safety.bands(0);
        assert_eq!(
            walls.iter().map(|(band, _)| (band.height, band.radius, band.lean, band.arc, band.turn)).collect::<Vec<_>>(),
            [(34.0, 3.6, 90.0, 360.0, 10.0), (37.0, 3.3, 90.0, 360.0, 20.0), (40.0, 3.0, 90.0, 360.0, 30.0)]
        );
        // Alpha 5 b to 75 at b = 15; band 1 fades from b = 3080 (73, 0 at 3117), band 0 only from 5500.
        let alpha = |age: u32, band: usize| safety.bands(age)[band].0.alpha;
        assert_eq!(
            [alpha(0, 0), alpha(14, 0), alpha(3079, 1), alpha(3115, 1), alpha(3116, 1), alpha(3116, 0), alpha(5499, 0)],
            [5.0, 75.0, 73.0, 1.0, 0.0, 75.0, 73.0]
        );
        // The rim: h (0.75 + 0.25 sin θ) with θ = 34 s + (4 − k) b, times sin b up to b = 90.
        let rim = |age: u32, band: usize, s: usize| safety.bands(age)[band].1[s];
        assert!((rim(89, 0, 0) - 34.0 * 0.75).abs() < 0.01, "b = 90: θ = 360 → 0.75 h");
        assert!((rim(90, 0, 0) - 34.0 * (0.75 + 0.25 * 4.0_f32.to_radians().sin())).abs() < 0.01, "b = 91: full height factor");
        assert!((rim(44, 1, 10) - 37.0 * (0.75 + 0.25 * ((340 + 135) as f32 % 360.0).to_radians().sin()) * 45.0_f32.to_radians().sin()).abs() < 0.01);
        // The undead heal: two walls of equal height, radii 0.2 apart; alpha 75 at b = 15 and 29, 73 at 30, 0 at 67.
        let heal = banded(320, [10.0, 20.0, 0.0], [45.0, 4.6]);
        let [(first, _), (second, _)] = [heal.bands(14)[0].clone(), heal.bands(14)[1].clone()];
        assert_eq!((first.height, second.height, first.radius, first.alpha), (45.0, 45.0, 4.6, 75.0));
        assert!((second.radius - 4.8).abs() < 1e-5);
        let heal_alpha = |age: u32| heal.bands(age)[0].0.alpha;
        assert_eq!([heal_alpha(28), heal_alpha(29), heal_alpha(65), heal_alpha(66)], [75.0, 73.0, 1.0, 0.0]);
        // Safety Wall stands until its unit goes; both show under /effect off (force-drawn 315).
        assert!(is_banded(315) && is_banded(320) && force_drawn(315));
        assert!(banded_ticks(315) > 1_000_000);
    }

    #[test]
    fn picks_a_casts_aura_by_skill() {
        let table = Table::load();
        // Heal's and Fire Bolt's follow the cast's element (here a stand-in 55 or 58); Assumptio's 58 is fixed.
        assert_eq!([table.cast_aura(28, 58), table.cast_aura(19, 55), table.cast_aura(361, 55)], [Some(58), Some(55), Some(58)]);
        // Bash's burst (16) for Bash and for a skill not listed; none for Sonic Blow, Pierce and the table's None rows.
        assert_eq!([table.cast_aura(5, 12), table.cast_aura(2000, 12)], [Some(16), Some(16)]);
        assert_eq!([table.cast_aura(136, 12), table.cast_aura(56, 12), table.cast_aura(271, 12)], [None; 3]);
        // A fixed effect: a song 441, and a few skills with effects of their own.
        assert_eq!(
            [table.cast_aura(317, 12), table.cast_aura(45, 12), table.cast_aura(57, 12), table.cast_aura(2052, 12)],
            [Some(441), Some(43), Some(144), Some(1560)]
        );
        // The 4th jobs' ranges show none; the edges of the ranges and just outside.
        assert_eq!([5401, 5507, 6001, 6006, 6525, 6606].map(|skill| table.cast_aura(skill, 12)), [None; 6]);
        assert_eq!([5400, 6007, 6607].map(|skill| table.cast_aura(skill, 12)).len(), 3);
        // The table's rows, as the issue lists them: 766 skills and 5 None rows.
        assert_eq!(table.casts.len(), 772);
        assert!(include_str!("effects.txt").lines().filter(|line| line.starts_with("cast\t")).count() == 772);
    }

    #[test]
    fn says_what_skills_show() {
        let table = Table::load();
        // Hit effects: Bash 0 and EF_HIT2 with its first hit, Magnum Break 49 on each target, Pierce none (not in the
        // table, so the skill lines). Napalm Beat (11) keeps [0, 1], no caster effect, no 32.
        assert_eq!((table.hit(5), table.hit(7)), (Some([Some(0), Some(1)]), Some([Some(49), None])));
        assert_eq!(table.hit(11), Some([Some(0), Some(1)]));
        assert_eq!(table.hit(56), None);
        // On the caster: Bash and the default 16, Brandish Spear 144, the bolts none, the newest jobs' none, Napalm Beat none.
        assert_eq!((table.caster(5), table.caster(57), table.caster(19), table.caster(5450)), (Some(16), Some(144), None, None));
        assert_eq!(table.caster(11), None);
        assert_eq!(table.caster(4000), Some(16));
        // Effect 32 plays effect\ef_napalmbeat at 0 ms.
        assert_eq!(table.sounds[&32][0].file, "effect\\ef_napalmbeat");
        assert_eq!(table.sounds[&32][0].delay_ms, 0);
        // As the packet comes: Cold Bolt's falling arrows on the target.
        assert_eq!(table.struck(14), Some(&[(Who::Target, 26)][..]));
        // Ground skills: Volcano and Land Protector at the cell; Ice Wall too.
        assert_eq!(
            (table.ground(285), table.ground(288), table.ground(87)),
            (Some(&[(Who::Ground, 225)][..]), Some(&[(Who::Ground, 238)][..]), Some(&[(Who::Ground, 74)][..]))
        );
        // A Claymore Trap (0x98) and a Sandman (0x95) going off.
        assert_eq!((table.trap(0x98), table.trap(0x95), table.trap(0x7e)), (Some(107), Some(139), None));
        // Units: Sacramenti, Gospel and Earth Strain.
        assert_eq!((table.units.get(&0x82), table.units.get(&0xb3), table.units.get(&0xcb)), (Some(&91), Some(&370), Some(&142)));
        // Earth Strain and Gospel play once and are kept; a warp portal starts again.
        assert!(table.units_once.contains(&0xcb) && table.units_once.contains(&0xb3) && !table.units_once.contains(&0x80));
        // Wall of Thorns shows 912, started again while it stands: its cast and the ground under it.
        assert_eq!(table.units.get(&0xe6), Some(&912));
        assert!(!table.units_once.contains(&0xe6));
        let thorns: Vec<(&str, &str, bool, bool)> =
            table.parts[&912].iter().map(|part| (part.file.as_str(), part.folder.as_str(), part.attached, part.once)).collect();
        assert_eq!(
            thorns,
            [
                ("mineffect/new_wallofthorn/new_wallofthorn_cast/new_wallofthorn_cast", "mineffect/new_wallofthorn/new_wallofthorn_cast/", false, false),
                ("new_wallofthorn/new_wallofthorn_cast_bottom/new_wallofthorn_cast_bottom", "new_wallofthorn/new_wallofthorn_cast_bottom/", false, false)
            ]
        );
        // Earth Strain's 142 keeps its quake and sound, which play once with it.
        assert_eq!(table.quakes[&142], [(Duration::ZERO, QUAKE_LONG)]);
        assert_eq!(table.sounds[&142][0].file, "effect\\wizard_earthspike");
    }

    #[test]
    fn says_what_a_status_keeps_playing() {
        let table = Table::load();
        // The quickens' afterimage, Explosion Spirits' aura with it, Steel Body's glow; Kyrie and Provoke nothing.
        assert_eq!((table.status(2), table.status(161)), (&[166][..], &[166][..]));
        assert_eq!(table.status(86), [203, 166]);
        assert!(table.status(87).contains(&254));
        assert!(table.status(19).is_empty() && table.status(0).is_empty());
    }

    #[test]
    fn finds_a_trap_by_its_model() {
        let standing = [((), 7u32, [1.0, 2.0, 3.0]), ((), 9, [4.0, 5.0, 6.0])];
        assert_eq!(standing_at(&standing, 9), Some([4.0, 5.0, 6.0]));
        assert_eq!(standing_at(&standing, 8), None);
    }

    #[test]
    fn effects_name_their_sounds() {
        let table = Table::load();
        let ice = &table.sounds[&26][0];
        assert_eq!((ice.file.as_str(), ice.delay_ms, ice.random), ("effect\\ef_icearrow%d", 0, Some((1, 3))));
        assert!((0..100).map(|_| pick(1, 3)).all(|picked| (1..=3).contains(&picked)));
    }

    #[test]
    fn puffs_songs_and_glints_steel_body() {
        // A puff: none before its start; r 10 growing ×1.02, alpha 100 until r passes 20 (tick 36), then −3 a tick
        // to nothing at 69, where it starts again; not after age 300.
        assert_eq!(puff_at(7, 0, -1), None);
        let (r, alpha, ..) = puff_at(7, 0, 0).unwrap();
        assert_eq!((r, alpha), (10.0, 100.0));
        assert_eq!(puff_at(7, 0, 35).unwrap().1, 100.0);
        assert_eq!(puff_at(7, 0, 36).unwrap().1, 97.0);
        let (r, alpha, ..) = puff_at(7, 0, 69).unwrap();
        assert!((r - 10.0).abs() < 1e-4 && alpha == 100.0);
        assert!(puff_at(7, 0, 399).is_none());
        // Eight puffs in two sets of four; the variants' colours by the puff's place.
        assert_eq!(PUFF_STARTS, [0, 20, 40, 60, 10, 30, 50, 70]);
        assert_eq!([puff_color(2, 0), puff_color(3, 1), puff_color(5, 2), puff_color(8, 3)], [[105, 255, 105], [5, 5, 255], [255, 55, 55], [255; 3]]);
        assert_eq!([861, 865, 877, 868].map(song_variant), [Some(2), Some(3), Some(8), None]);
        // A glint: from its set's alpha down 12 a tick, a new round under 12, then 9 ticks up 25 to 225.
        assert_eq!([glint_at(70, 0), glint_at(70, 1), glint_at(70, 5), glint_at(70, 14), glint_at(70, 15)], [(70, 0), (58, 0), (0, 1), (225, 1), (213, 1)]);
        // Aura Blade's bloom: 60 ticks of the cast aura's bands, light blue and added on.
        assert!(is_banded(367) && banded_ticks(367) == 60);
        assert_eq!(aura_look(367).1, [0xA4, 0xCD, 0xFE]);
    }

    #[test]
    fn blesses_with_angels_a_glow_and_sparkles() {
        // Four angels launched at ticks 0, 3, 6, 9 at alphas 200, 182, 164, 146, 25 units up, until tick 150.
        assert_eq!([0, 1, 2, 3].map(|angel| angel_alpha(angel, ANGEL_STARTS[angel])), [Some(200.0), Some(182.0), Some(164.0), Some(146.0)]);
        assert_eq!((angel_alpha(1, 2), angel_alpha(3, 8)), (None, None));
        assert_eq!((angel_alpha(0, 140), angel_alpha(0, 149), angel_alpha(0, 150)), (Some(100.0), Some(10.0), None));
        assert_eq!(ANGEL_RISE, 25.0);
        // The glow: 0, up 3.3 a tick to 100, held, gone at 150.
        assert_eq!([0, 30, 120, 150].map(glow_alpha).map(f32::round), [0.0, 99.0, 100.0, 0.0]);
        assert_eq!(glow_alpha(135), 50.0);
        // A sparkle every 4 ticks while it lasts, each living 80.
        let starts: Vec<u32> = sparkle_starts().collect();
        assert_eq!((starts.len(), starts[..3].to_vec(), *starts.last().unwrap()), (38, vec![0, 4, 8], 148));
        assert!(sparkle_place([1.0, 0.0], 79).is_some() && sparkle_place([1.0, 0.0], 80).is_none());
        assert_eq!(BLESSING_TICKS, 150);
    }

    #[test]
    fn times_and_places_strs() {
        // One frame each whole 16 ms tick.
        assert_eq!([0, 15, 16, 31, 32].map(|ms| str_frame(Duration::from_millis(ms))), [0, 0, 1, 1, 2]);
        // To the last key (frames 0–120: 121 ticks), or a shorter length.
        let key = rustnarok_client::effect::Key {
            frame: 0,
            kind: 0,
            position: [320.0, 240.0],
            corners: [-10.0, 10.0, 10.0, -10.0, -10.0, -10.0, 10.0, 10.0],
            texture: 0.0,
            texture_motion: 0,
            delay: 0.0,
            angle: 0.0,
            color: [1.0; 4],
            source_blend: 5,
            destination_blend: 6,
        };
        let layer = rustnarok_client::effect::Layer { textures: Vec::new(), keys: vec![key, rustnarok_client::effect::Key { frame: 120, ..key }] };
        let str = Str { fps: 60, frames: 160, layers: vec![layer] };
        assert_eq!((str_ticks(&str, None), str_ticks(&str, Some(40)), str_ticks(&str, Some(500))), (121, 40, 121));
        // A ground unit's starts again 8 ticks before its length; Quagmire's 20.
        assert_eq!((restart_ticks(25, 40), restart_ticks(95, 150)), (32, 130));
        // (320, 240) is the anchor: a corner 10 left and 10 up of it is (−10, −10) sprite pixels, and on screen that's
        // q times it from the feet, then 80 screen pixels up whatever q is (the shader's pixel offset).
        let corner = str_corner([320.0, 240.0], [-10.0, -10.0], 0.0);
        assert_eq!(corner, [-10.0, -10.0]);
        let screen = |q: f32| [500.0 + q * corner[0], 400.0 + q * corner[1] - STR_RISE];
        assert_eq!(screen(1.0), [490.0, 310.0]);
        let [x, y] = screen(1.39);
        assert!((x - 486.1).abs() < 1e-3 && (y - 306.1).abs() < 1e-3);
        // Turned 90°: (10, 0) goes to (0, 10).
        let turned = str_corner([320.0, 240.0], [10.0, 0.0], 90.0);
        assert!(turned[0].abs() < 1e-4 && (turned[1] - 10.0).abs() < 1e-4);
        // The table's lengths and the units that play once.
        let table = Table::load();
        assert_eq!([table.lengths[&25], table.lengths[&95], table.lengths[&920]], [40, 150, 40]);
        assert!([130, 136, 221].iter().all(|kind| table.units_once.contains(kind)));
    }

    #[test]
    fn heals_teleports_and_arrives_in_bands() {
        // Up 5 a tick to 75 by age 15, held to 60, down 2 a tick to nothing about 97.
        assert_eq!([0, 1, 15, 60, 61, 97, 98, 120].map(band_fade), [0.0, 5.0, 75.0, 75.0, 73.0, 1.0, 0.0, 0.0]);
        // Heal: 4 sparkles every 6 ticks until 50, each within ±6 units of the feet.
        let sparkles = heal_sparkles(random);
        assert_eq!(sparkles.len(), 36);
        assert_eq!(
            sparkles.iter().map(|(age, _)| *age).collect::<std::collections::BTreeSet<_>>().into_iter().collect::<Vec<_>>(),
            [0, 6, 12, 18, 24, 30, 36, 42, 48]
        );
        assert!(sparkles.iter().all(|(_, [x, z])| x.abs() <= 6.0 && z.abs() <= 6.0));
        assert!(sparkle_at(0).is_some() && sparkle_at(SPARKLE_TICKS).is_none());
        let heal = Banded { effect: 312, anchor: Anchor::Unit(None), start: Instant::now(), ticks: 100, turns: [10.0, 200.0, 0.0], extra: [0.0; 2], sparkles };
        let walls = heal.bands(15);
        assert_eq!(
            walls.iter().map(|(band, _)| (band.radius, band.lean, band.height, band.turn, band.alpha)).collect::<Vec<_>>(),
            [(4.6, 90.0, 40.0, 10.0, 75.0), (4.8, 90.0, 40.0, 200.0, 75.0)]
        );
        // The teleport's four columns, and the arrival's two walls and two near-flat rings.
        let columns = teleport_bands(200, 0).map(|band| [band.radius, band.height, band.lean, band.turn, band.alpha]);
        assert_eq!(columns, TELEPORT_BANDS);
        assert_eq!(teleport_bands(200, 199)[0].alpha, 100.0 / 40.0);
        let arrival = arrival_bands(15).map(|band| [band.radius, band.height, band.lean, band.alpha]);
        assert_eq!(arrival, [[3.7, 30.0, 90.0, 75.0], [3.4, 30.0, 90.0, 75.0], [3.6, 4.0, 10.0, 75.0], [3.7, 4.0, 5.0, 75.0]]);
        // Their textures, colours and blends; how long they last.
        assert_eq!(aura_look(312), ("alpha_down", [0x8C, 0xD2, 0x8C], true));
        assert!([304, 314, 344].iter().all(|&effect| aura_look(effect).1 == [0x64, 0x64, 0xFF] && aura_look(effect).2));
        assert_eq!([banded_ticks(312), banded_ticks(344), banded_ticks(304), banded_ticks(314)], [100, 100, 200, 200]);
        assert!(is_banded(325) && is_banded(12) && !is_banded(316));
    }

    #[test]
    fn lays_bands_round_the_feet() {
        // Radius 4 leaning 60°, 10 high: the top 5 further out and 8.66 up; s = 10 halfway round, s = 20 back.
        let band = Band { arc: 360.0, turn: 0.0, radius: 4.0, lean: 60.0, height: 10.0, alpha: 255.0 };
        let points = band_points(&band, |_| 10.0);
        let near = |a: Vector, b: Vector| a.iter().zip(b).all(|(a, b)| (a - b).abs() < 1e-4);
        assert!(near(points[0].0, [0.0, 0.0, 4.0]) && near(points[0].1, [0.0, 8.660_254, 9.0]));
        assert!(near(points[10].0, [0.0, 0.0, -4.0]) && near(points[10].1, [0.0, 8.660_254, -9.0]));
        assert!(near(points[20].0, points[0].0));
        // A 90° arc from 90°: s = 20 ends a quarter round.
        let quarter = band_points(&Band { arc: 90.0, turn: 90.0, ..band }, |_| 0.0);
        assert!(near(quarter[0].0, [4.0, 0.0, 0.0]) && near(quarter[20].0, [0.0, 0.0, -4.0]));
        assert_eq!(band_mesh([0.0; 3], &band, |_| 10.0, [1.0; 3]).len(), BAND_QUADS * 6);
    }

    #[test]
    fn blooms_turns_and_fades_cast_auras() {
        // At least 56 ticks for 12 and 58, 70 for the elements; else the cast time in ticks.
        assert_eq!([aura_ticks(12, 0), aura_ticks(58, 2000), aura_ticks(55, 500), aura_ticks(55, 1500)], [56, 125, 70, 93]);
        // The faintest band fades out by tick 8 and starts again at 3.37 leaning 74°: 9 ticks up to 90, 30 down, 39 in all.
        let shape = |age: u32| bloom(1, 3, 1000, age);
        assert_eq!(shape(8), None);
        let first = shape(9).unwrap();
        assert_eq!((first.radius, first.lean, first.alpha), (3.44, 73.0, 10.0));
        let top = shape(17).unwrap();
        assert_eq!((top.radius, top.alpha), (4.0, 90.0));
        assert_eq!(shape(18).unwrap().alpha, 87.0);
        assert_eq!(shape(47), None);
        assert_eq!(shape(48), Some(first));
        // Within 30 ticks of the aura's end a band that fades out stays gone: the start at 47 plays out to 86.
        assert!(bloom(1, 3, 100, 85).is_some() && (86..100).all(|age| bloom(1, 3, 100, age).is_none()));
        // A short aura: every band from nothing, band k 5k ticks late.
        assert_eq!((bloom(0, 2, 56, 9), bloom(0, 2, 56, 10)), (None, None));
        assert_eq!(bloom(0, 2, 56, 11).unwrap().alpha, 10.0);
        // The elements' arcs turn 3, 4, 5° a tick and rise over 90 ticks; everything fades in the last 40.
        assert_eq!([turning(0, 200, 10).turn, turning(2, 200, 10).turn], [30.0, 230.0]);
        assert!((turning_height(0, 25.0, 30, 10) - 12.5).abs() < 1e-4 && (turning_height(0, 25.0, 200, 10) - 25.0).abs() < 1e-4);
        assert_eq!([turning(0, 200, 160).alpha, turning(0, 200, 161).alpha, turning(0, 200, 199).alpha], [180.0, 175.0, 0.0]);
        assert_eq!([turning(3, 200, 0).alpha, turning(3, 200, 1).alpha, turning(3, 200, 161).alpha], [70.0, 60.0, 58.0]);
    }

    #[test]
    fn flies_the_hit_ring_the_way_the_target_faces() {
        // Each of the 8 facings (0 south, turning west) and 30° (south-east of south): the way it faces.
        let near = |[x, z]: [f32; 2], [want_x, want_z]: [f32; 2]| (x - want_x).abs() < 1e-5 && (z - want_z).abs() < 1e-5;
        let half = std::f32::consts::FRAC_1_SQRT_2;
        let ways = [[0.0, -1.0], [-half, -half], [-1.0, 0.0], [-half, half], [0.0, 1.0], [half, half], [1.0, 0.0], [half, -half]];
        for (direction, way) in (0..8).zip(ways) {
            assert!(near(drift(crate::actor::rot_of(direction)), way), "{direction}");
        }
        assert!(near(drift(30.0), [0.5, -(3.0f32.sqrt()) / 2.0]));
        // Distance and alpha on drawn ticks 0–9: it has already made its first move.
        let travel = [0.665, 1.295, 1.890, 2.450, 2.975, 3.465, 3.920, 4.340, 4.725, 5.075];
        assert!((0..10).all(|tick| (ring_travel(tick) - travel[tick as usize]).abs() < 1e-4));
        assert_eq!((0..10).map(|tick| (ring_alpha(tick) * 255.0).round() as u32).collect::<Vec<_>>(), [254, 254, 254, 254, 254, 203, 152, 101, 50, 0]);
        let time = Instant::now();
        let hit =
            HitSpark { start: time, anchor: Anchor::Unit(None), way: [0.0, 1.0], sparks: sparks([0.0, 1.0], |from, _| from), paths: None, placed: Vec::new() };
        assert_eq!((hit.tick(time + Duration::from_millis(799)), hit.tick(time + Duration::from_millis(800))), (49, HIT_TICKS));
        // A funnel of 10 sides: the inner edge 5 from its middle, the outer 10 out and 3.5 further along its way (north).
        let ring = hit.ring(0, [0.0; 3]);
        assert_eq!(ring.len(), RING_SIDES * 6);
        let reach = |vertex: &MeshVertex| f32::hypot(vertex.position[0], vertex.position[1] - HIT_HEIGHT);
        let along = |vertex: &MeshVertex| vertex.position[2] - ring_travel(0);
        assert!(ring.iter().all(|vertex| {
            ((reach(vertex) - 5.0).abs() < 1e-4 && along(vertex).abs() < 1e-4) || ((reach(vertex) - 10.0).abs() < 1e-4 && (along(vertex) - 3.5).abs() < 1e-4)
        }));
        // u goes up 0.25 a side and starts again after four: seams after sides 4 and 8.
        let starts: Vec<f32> = ring.chunks(6).map(|side| side[0].uv[0]).collect();
        assert_eq!(starts, [0.0, 0.25, 0.5, 0.75, 0.0, 0.25, 0.5, 0.75, 0.0, 0.25]);
        assert!(ring.chunks(6).all(|side| (side[5].uv[0] - side[0].uv[0] - 0.25).abs() < 1e-6));
    }

    #[test]
    fn throws_the_hit_sparks_within_their_ranges() {
        let way = drift(0.0);
        let toward = way[0].atan2(way[1]).to_degrees();
        for random in [(|from: f32, _: f32| from) as fn(f32, f32) -> f32, |_, to| to - 1e-4, |from, to| (from + to) / 2.0] {
            for (index, spark) in sparks(way, random).iter().enumerate() {
                let off = (spark.yaw.to_degrees() - toward).rem_euclid(360.0);
                if index < 2 {
                    assert!(!(40.01..319.99).contains(&off), "{off}");
                    assert!(!spark.back && spark.rise == 0.0);
                } else {
                    assert!((139.99..=220.0).contains(&off), "{off}");
                    assert!(spark.back && (0.3..1.2).contains(&spark.rise));
                }
                assert!((-50.01..50.0).contains(&spark.pitch.to_degrees()));
                assert!((4.0..7.0).contains(&spark.out) && (0.6..1.5).contains(&spark.speed) && (0.6..1.6).contains(&spark.scale));
                assert!((6..=29).contains(&spark.life));
            }
        }
    }

    #[test]
    fn turns_a_sparks_pitch_about_the_east_west_axis() {
        let spark = |yaw: f32, pitch: f32| Spark {
            yaw: yaw.to_radians(),
            pitch: pitch.to_radians(),
            out: 4.0,
            speed: 1.0,
            back: false,
            rise: 0.0,
            scale: 1.0,
            life: 10,
        };
        // Heading east (yaw 90°): level whatever its pitch; north (0°): up sin p.
        let east = spark(90.0, 49.0).heading();
        assert!((east[0] - 1.0).abs() < 1e-5 && east[1].abs() < 1e-5);
        let north = spark(0.0, 30.0).heading();
        assert!((north[1] - 0.5).abs() < 1e-5 && (north[2] - 3.0f32.sqrt() / 2.0).abs() < 1e-5);
        // (sin y, cos y·sin p, cos y·cos p) at yaw 60°, pitch −40°.
        let [x, up, z] = spark(60.0, -40.0).heading();
        let (yaw, pitch) = (60.0f32.to_radians(), (-40.0f32).to_radians());
        assert!((x - yaw.sin()).abs() < 1e-5 && (up - yaw.cos() * pitch.sin()).abs() < 1e-5 && (z - yaw.cos() * pitch.cos()).abs() < 1e-5);
    }

    #[test]
    fn moves_a_spark_before_it_is_drawn() {
        // v0 1.2, life 12, north and level: on drawn tick a it has made a + 1 moves, v0·(n − n(n+1)/(4L)) from its start.
        let front = Spark { yaw: 0.0, pitch: 0.0, out: 5.0, speed: 1.2, back: false, rise: 0.0, scale: 1.0, life: 12 };
        let path = front.path(|_, _| 0.0);
        assert_eq!(path.len(), 12);
        for (tick, place) in path.iter().enumerate() {
            let n = tick as f32 + 1.0;
            assert!((place[2] - (5.0 + 1.2 * (n - n * (n + 1.0) / 48.0))).abs() < 1e-4 && (place[1] - HIT_HEIGHT).abs() < 1e-5);
        }
        assert!((path[0][2] - 5.0 - 1.15).abs() < 1e-5);
        // Alpha 254·(1 − (a + 1)/L): already below 254 on its first tick, 0 on its last.
        assert!((front.alpha(0) * 255.0 - 254.0 * 11.0 / 12.0).abs() < 1e-3 && front.alpha(11) == 0.0);
        // The back pair (r 0.9) flies level along its yaw (its pitch only places its start) at its full speed and hops
        // r·n·(1 − (n + 1)/L): back at its start height on tick L − 2, r below it on its last.
        let back = Spark { yaw: std::f32::consts::PI, pitch: 0.6, back: true, rise: 0.9, ..front };
        let start = back.origin();
        let path = back.path(|_, _| -100.0);
        for (tick, place) in path.iter().enumerate() {
            let moves = tick as u32 + 1;
            assert!((place[1] - start[1] - back.hop(moves)).abs() < 1e-4, "{tick}");
            assert!((start[2] - place[2] - back.travel(moves)).abs() < 1e-4 && (place[0] - start[0]).abs() < 1e-4);
        }
        assert!((path[10][1] - start[1]).abs() < 1e-4 && (path[11][1] - start[1] + 0.9).abs() < 1e-4);
        // On flat ground it never falls to the ground; where the ground rises under it, it bounces.
        assert_eq!(back.path(|_, _| 0.0), path);
        let bounced = back.path(|_, z| if z < start[2] - 6.0 { start[1] + 0.5 } else { 0.0 });
        assert_ne!(bounced, path);
    }

    #[test]
    fn trails_a_spark_from_the_feet_smaller_and_fainter() {
        let spark = Spark { yaw: 0.0, pitch: 0.0, out: 4.0, speed: 1.0, back: false, rise: 0.0, scale: 1.2, life: 10 };
        let path = spark.path(|_, _| 0.0);
        let places = |tick: u32| spark.trail(tick, &path).iter().map(|(place, _, _)| *place).collect::<Vec<_>>();
        // Drawn tick 0: copies 1 and 2 still at the feet, on the ground; tick 1: copy 2; then where it was.
        assert_eq!(places(0), [path[0], [0.0; 3], [0.0; 3]]);
        assert_eq!(places(1), [path[1], path[0], [0.0; 3]]);
        assert_eq!(places(4), [path[4], path[3], path[2]]);
        let alpha = spark.alpha(4);
        let trail = spark.trail(4, &path);
        assert_eq!(trail.iter().map(|(_, faded, _)| *faded).collect::<Vec<_>>(), [alpha, alpha * 2.0 / 3.0, alpha / 3.0]);
        assert_eq!(trail.iter().map(|(_, _, scale)| *scale).collect::<Vec<_>>(), [1.2, 1.2 * 5.0 / 6.0, 1.2 * 4.0 / 6.0]);
        assert!(spark.trail(10, &path).is_empty());
    }

    #[test]
    fn leaves_a_hit_where_it_was_made() {
        let mut effects = Effects::default();
        let time = Instant::now();
        let way = drift(0.0);
        effects.hits.push(HitSpark { start: time, anchor: Anchor::Unit(Some(7)), way, sparks: sparks(way, |from, _| from), paths: None, placed: Vec::new() });
        // Not landed yet: it still follows the unit.
        effects.settle_hits(time - Duration::from_millis(1), |_| Some([1.0, 0.0, 1.0]), |_, _| 0.0);
        assert!(effects.hits[0].paths.is_none());
        effects.settle_hits(time, |_| Some([50.0, 2.0, 60.0]), |_, _| 0.0);
        let at = |effects: &Effects| effects.hits[0].anchor.place(&|_| Some([90.0, 0.0, 90.0]));
        assert_eq!(at(&effects), Some([50.0, 2.0, 60.0]));
        // The unit walks on; the hit stays.
        effects.settle_hits(time + Duration::from_millis(100), |_| Some([80.0, 2.0, 60.0]), |_, _| 0.0);
        assert_eq!(at(&effects), Some([50.0, 2.0, 60.0]));
    }

    #[test]
    fn reads_the_effect_table() {
        let table = Table::load();
        // Base level up plays the angel (EF_ANGEL), on the unit.
        let angel = table.misc(0).unwrap();
        assert_eq!(angel, 371);
        // A super novice's job level up (type 8): EF_JOBLVUP50.
        assert_eq!(table.misc(8), Some(337));
        // The teleport's, the logout's, the arrival's and Heal's bands are made in code; the warp portals' rings are
        // ×#6464FF (open) and untinted (waiting), adding on.
        assert!([304, 312, 314, 344].iter().all(|effect| !table.rings.contains_key(effect)));
        assert_eq!(table.rings[&313].len(), 1);
        for ring in &table.rings[&317] {
            assert_eq!([&ring["red"], &ring["green"], &ring["blue"], &ring["blendMode"]], ["0.392", "0.392", "1", "2"]);
        }
        assert_eq!([&table.rings[&316][0]["red"], &table.rings[&316][0]["green"]], ["1", "1"]);
        let part = &table.parts[&angel][0];
        assert_eq!((part.file.as_str(), part.attached), ("angel", true));
        assert_eq!(table.item(501), Some(204));
        assert_eq!(table.skill(28, "effectId"), [312]);
    }

    #[test]
    fn effect_switches() {
        // /effect off: the force list's ranges, in and out at each edge.
        let ranges = [
            (475, 484),
            (735, 738),
            (758, 767),
            (1186, 1191),
            (1214, 1217),
            (1262, 1265),
            (1607, 1609),
            (1617, 1619),
            (1869, 1871),
            (2379, 2381),
            (2383, 2389),
            (2395, 2399),
        ];
        for (from, to) in ranges {
            assert!(force_drawn(from) && force_drawn(to) && !force_drawn(from - 1) && !force_drawn(to + 1), "{from}–{to}");
        }
        assert!([35, 316, 317, 321, 315, 338, 371, 582, 154, 155, 166, 2402].into_iter().all(force_drawn));
        // A Fire Bolt, Heal, the cast aura and the hit spark don't show.
        assert!(![31, 312, 12, 0, 2382, 2400].into_iter().any(force_drawn));
        // /mineffect's lighter STRs.
        let min = [10, 41, 717, 66, 75, 76, 77, 85, 88, 103, 104, 112].map(min_file);
        let files = [
            "memor_min",
            "jong_mini",
            "jong_mini",
            "cure_min",
            "gloria_min",
            "magnificat_min",
            "resurrection_min",
            "lexaeterna_min",
            "suffragium_min",
            "WeaponPerfection_min",
            "maximize_min",
            "kyrie_min",
        ];
        assert_eq!(min, files.map(Some));
        assert_eq!(min_file(31), None);
        // The map forces it on with its simple-effect flag (bit 3); without, the player's choice stands.
        assert!(min_effect(false, Some(0b1000)) && min_effect(true, Some(0b1000)));
        assert!(!min_effect(false, Some(0b0111)) && min_effect(true, Some(0)));
        assert!(!min_effect(false, None) && min_effect(true, None));
    }

    #[test]
    fn skills_play_their_rows_as_the_game() {
        let table = Table::load();
        let ms = Duration::from_millis;
        // Meteor Storm: Meteor1–3 at random, one long shake 640 ms in.
        assert_eq!(table.parts[&92][0].random, Some((1, 3)));
        assert_eq!(table.quakes[&92], [(ms(640), QUAKE_LONG)]);
        // Lord of Vermilion: four shakes and its 11 sounds; Bowling Bash's shockwave is silent.
        assert_eq!(table.quakes[&90], [800, 1600, 2400, 3200].map(|at| (ms(at), QUAKE_LONG)));
        let sounds: Vec<(&str, u64)> = table.sounds[&90].iter().map(|sound| (sound.file.as_str(), sound.delay_ms)).collect();
        assert_eq!(sounds.len(), 11);
        assert_eq!(sounds[..3], [("effect\\wizard_meteo", 0), ("effect\\hunter_blastmine", 0), ("effect\\hunter_blastmine", 160)]);
        assert!(!table.sounds.contains_key(&149));
        // Jupitel Thunder from the server flies one ball from its unit at the map's centre; Dark Thunder launches two; Quagmire
        // sounds one time in 12; Repair Weapon sounds at 320 and 880 ms; Adrenaline Rush at 0 and 480 ms.
        assert!(aimed_at_centre(93) && AIMED.contains(&93));
        assert_eq!(table.struck(341), Some(&[(Who::Caster, 93), (Who::Caster, 93)][..]));
        assert_eq!((0..12).filter(|&draw| quagmire_heard(draw)).count(), 1);
        assert_eq!(table.sounds[&101].iter().map(|sound| sound.delay_ms).collect::<Vec<_>>(), [320, 880]);
        assert_eq!(
            table.sounds[&98].iter().map(|sound| (sound.file.as_str(), sound.delay_ms)).collect::<Vec<_>>(),
            [("effect\\black_adrenalinerush_a", 0), ("effect\\black_adrenalinerush_b", 480)]
        );
        assert!(
            [99, 100, 98].iter().all(|&id| first::makes(id))
                && [150, 191, 194, 264].iter().all(|&kind| table.trap(kind) == Some(99) || kind == 150 && table.trap(kind).is_some())
        );
        // The priests' sounds at their own ticks, Aspersio's sparkles in code, Melody of Sink's Lex Divina once, Storm Gust's
        // light STR under /mineffect.
        let delays = |id: u32| table.sounds[&id].iter().map(|sound| sound.delay_ms).collect::<Vec<_>>();
        assert_eq!([delays(84), delays(85), delays(86)], [[960], [240], [1120]]);
        assert_eq!(delays(87), [0, 320, 560, 960, 1120]);
        assert!(first::makes(86) && table.parts[&87][0].once);
        assert_eq!(table.skill(763, "effectIdOnCaster"), [89]);
        assert_eq!(
            [
                storm_gust_effect(89, 89, true),
                storm_gust_effect(723, 89, true),
                storm_gust_effect(89, 89, false),
                storm_gust_effect(664, 89, true),
                storm_gust_effect(89, 74, true)
            ],
            [708, 708, 89, 89, 74]
        );
        // Sight Rasher is built in code (no generic parts, its sound stays) and ends the caster's Sight.
        assert!(!table.floaters.contains_key(&62) && table.sounds.contains_key(&62) && table.used(81) == Some(&[(Who::Caster, 62)][..]));
        // Cart Revolution's sounds, First Aid, Sprinkle Sand and the undead Heal are built in code or in bands.
        assert_eq!(table.sounds[&170].iter().map(|sound| sound.delay_ms).collect::<Vec<_>>(), [112, 320]);
        for id in [309, 310, 320, 315, 170] {
            assert!(!table.rings.contains_key(&id) && !table.floaters.contains_key(&id), "{id}");
        }
        assert!(first::makes(170) && first::makes(309) && first::makes(310) && first::makes(320));
        // Holy Water's bottle, Signum Crucis' burst, DEX AGI UP's text and streaks and Provoke's two sounds.
        assert!(first::makes(39) && first::makes(43) && !table.sprites.contains_key(&39) && !table.floaters.contains_key(&43));
        assert_eq!(table.used(31), Some(&[(Who::Caster, 39)][..]));
        assert_eq!(table.used(32), Some(&[(Who::Caster, 40)][..]));
        let sounds = |id: u32| table.sounds[&id].iter().map(|sound| (sound.file.as_str(), sound.delay_ms)).collect::<Vec<_>>();
        assert_eq!(sounds(40), [("effect\\ef_signum", 480), ("effect\\ef_bash", 480)]);
        assert_eq!(sounds(67), [("effect\\swordman_provoke", 80), ("effect\\swordman_provoke", 480)]);
        // Kouenka's first hit and Flame Launcher's every packet show Fire Hit (49); Flicker shows 153 on its source.
        assert_eq!(table.hit(534), Some([None, Some(49)]));
        assert_eq!(table.struck(2259), Some(&[(Who::Target, 787), (Who::Target, 49)][..]));
        assert_eq!(table.used(2556), Some(&[(Who::Caster, 153)][..]));
        // Heal (small), Endure, Magnum Break, Steal, Detoxify and Decrease AGI are built in code: no generic parts or rings,
        // their sounds stay.
        for id in [7, 11, 17, 18, 21, 38] {
            assert!(first::makes(id) && !table.floaters.contains_key(&id) && !table.rings.contains_key(&id) && !table.parts.contains_key(&id), "{id}");
            assert!(table.sounds.contains_key(&id), "{id}");
        }
        // The pitchers throw their item at the packet (ok = 1 only), through the aimed effects; no generic part and no
        // `effectId` fallback for Potion Pitcher.
        assert_eq!([thrown_item(231), thrown_item(446), thrown_item(2481), thrown_item(28)], [Some(299), Some(541), Some(914), None]);
        assert!(
            [299, 541, 914].iter().all(|id| AIMED.contains(id) && aimed_at_centre(*id) && !table.floaters.contains_key(id) && !table.parts.contains_key(id))
        );
        assert!(table.skill(231, "effectId").is_empty());
        assert_eq!(table.sounds[&914].iter().map(|sound| (sound.file.as_str(), sound.delay_ms)).collect::<Vec<_>>(), [("effect\\G스포어익스플로젼", 0)]);
        // Finger Offensive's sphere (265), Dark Strike's balls (451) and Huujin's blades (621): by the game's hit count, in the
        // aimed effects, with no generic part; Finger Offensive's first hit shows EF_HIT2 too, Dark Strike's `struck` is its own.
        assert_eq!([hit_count(265, 0, 8), hit_count(451, 37, 8), hit_count(621, 36, 9), hit_count(265, 5, 4), hit_count(24, 7, 8)], [1, 1, 36, 1, 7]);
        assert!([265, 451, 621].iter().all(|id| AIMED.contains(id) && !table.floaters.contains_key(id) && !table.parts.contains_key(id)));
        assert_eq!(table.hit(267), Some([Some(0), Some(1)]));
        assert_eq!(table.struck(340), Some(&[(Who::Caster, 451)][..]));
        assert!(aimed_at_centre(265) && aimed_at_centre(412) && !aimed_at_centre(451));
        // Fire Ball (24) and Frost Diver (27) from the server fly from their unit at the map's centre; 24 has no floater.
        assert!(aimed_at_centre(24) && aimed_at_centre(27) && !aimed_at_centre(26));
        assert!(!table.floaters.contains_key(&24) && !table.parts.contains_key(&24) && !table.sprites.contains_key(&24));
        assert_eq!(table.sounds[&24].iter().map(|sound| sound.delay_ms).collect::<Vec<_>>(), [0]);
        // Blitz Beat: 115 (drawn in code) is its second hit effect, not its fallback, with its two sounds.
        assert_eq!(table.hit(129), Some([None, Some(115)]));
        assert!(table.skill(129, "effectId").is_empty());
        assert_eq!(
            table.sounds[&115].iter().map(|sound| (sound.file.as_str(), sound.delay_ms)).collect::<Vec<_>>(),
            [("effect\\hunter_blitzbeat_1st", 80), ("effect\\hunter_blitzbeat", 480)]
        );
        // Sonic Blow: its ring (drawn in code, so no parts) and SonicBlow.str on the caster.
        assert_eq!(table.struck(136), Some(&[(Who::Caster, 121), (Who::Caster, 143)][..]));
        assert!(!table.floaters.contains_key(&121) && !table.parts.contains_key(&121) && !table.sprites.contains_key(&121));
        // A skill's fallback effect never repeats one of its hit effects (Holy Light's 152 plays once, at the hit).
        for (&skill, hit) in &table.hits {
            let repeated = table.skill(skill, "effectId").into_iter().any(|effect| hit.contains(&Some(effect)));
            assert!(table.struck(skill).is_some() || !repeated, "skill {skill}");
        }
    }

    #[test]
    fn reads_sprites_quakes_and_ground_units() {
        let table = Table::load();
        // Blessing is made in code; the fire ring plays again and again.
        assert!(!table.sprites.contains_key(&42));
        let now = Instant::now();
        let ring = &table.sprites[&634][0];
        assert_eq!((ring.file.as_str(), ring.attached), ("화염진", true));
        assert!(Animation::new(ring, 634, now, Anchor::Unit(None), false).repeat);
        // Earth Spike shakes the camera; Fire Bolt's arrows fall before each hit.
        assert_eq!(table.quakes[&79], [(Duration::ZERO, QUAKE_SHORT)]);
        assert_eq!(table.skill(19, "beforeHitEffectId"), [31]);
        // Chase Walk's footprints: the catalog's textures, played on the ground.
        assert_eq!(table.floaters[&326][0]["file"], "effect/foot_l_b.tga");
        assert_eq!(table.floaters[&327][0]["file"], "effect/foot_r_b.tga");
        // Fire Wall, Safety Wall (its rise once), Pneuma and warp portals on the ground; Ankle Snare's model.
        assert_eq!([127, 126, 133, 129, 128].map(|kind| table.units[&kind]), [25, 315, 141, 316, 317]);
        assert!(table.parts[&315][0].once && !table.parts[&25][0].once);
        assert_eq!(table.models[&145], "외부소품\\트랩01");
    }

    #[test]
    fn shakes_restarts_and_multi_shake_effects() {
        let table = Table::load();
        // Heaven's Drive shakes 650 ms, Earth Spike 200; Jupitel Thunder throws its ball twice.
        assert_eq!(table.quakes[&142], [(Duration::ZERO, QUAKE_LONG)]);
        assert_eq!(table.quakes[&79], [(Duration::ZERO, QUAKE_SHORT)]);
        assert_eq!(table.struck(84), Some(&[(Who::Caster, JUPITEL); 2][..]));
        // 563 (EF_SCREEN_QUAKE): 20 shakes at ticks 0, 5, ..., 95 (every 80 ms), each 650 ms.
        let expected_563: Vec<(Duration, Duration)> = (0..20).map(|step| (Duration::from_millis(step * 80), QUAKE_LONG)).collect();
        assert_eq!(table.quakes[&563], expected_563);
        // 666 (EF_NPC_EARTHQUAKE): 3 shakes at ticks 0, 35 and 70 (0, 560 and 1120 ms), each 650 ms.
        assert_eq!(
            table.quakes[&666],
            [(Duration::ZERO, QUAKE_LONG), (Duration::from_millis(35 * 16), QUAKE_LONG), (Duration::from_millis(70 * 16), QUAKE_LONG)]
        );

        // A short shake lasts 200 ms; a standard shake lasts 650 ms, and another shake while it runs restarts the clock.
        let start = Instant::now();
        let mut effects = Effects::default();
        effects.start_quake(start, QUAKE_SHORT);
        assert!(effects.quake(start + Duration::from_millis(199)).is_some());
        assert!(effects.quake(start + QUAKE_SHORT).is_none());

        effects.start_quake(start, QUAKE_LONG);
        assert!(effects.quake(start + Duration::from_millis(400)).is_some());
        effects.start_quake(start + Duration::from_millis(400), QUAKE_LONG);
        assert!(effects.quake(start + Duration::from_millis(1049)).is_some());
        assert!(effects.quake(start + Duration::from_millis(1050)).is_none());

        // Each rendered frame flips the sign and stays within the noise ranges.
        let mut effects = Effects::default();
        effects.start_quake(start, QUAKE_LONG);
        for frame in 0..40 {
            let [side, distance, pitch] = effects.quake(start + Duration::from_millis(frame * 16)).unwrap();
            let sign = if frame % 2 == 0 { 1.0 } else { -1.0 };
            assert!((0.8 - 1e-5..=1.2 + 1e-5).contains(&(side * sign)), "side {side} at frame {frame}");
            assert!((0.04 - 1e-5..=0.44 + 1e-5).contains(&(distance * sign)), "distance {distance} at frame {frame}");
            assert!((0.2 - 2.0 / 15.0 - 1e-5..=0.2 + 2.0 / 15.0 + 1e-5).contains(&(pitch * sign)), "pitch {pitch} at frame {frame}");
        }

        // 563 runs through tick 95 + 650 ms (2170 ms); 666 through tick 70 + 650 ms (1770 ms).
        let mut effects = Effects::default();
        effects.play_quakes(&table, 563, start);
        assert!(effects.quake(start + Duration::from_millis(2169)).is_some());
        assert!(effects.quake(start + Duration::from_millis(2170)).is_none());

        let mut effects = Effects::default();
        effects.play_quakes(&table, 666, start);
        assert!(effects.quake(start + Duration::from_millis(1769)).is_some());
        assert!(effects.quake(start + Duration::from_millis(1770)).is_none());
    }

    #[test]
    fn blind_starts_once_for_the_player() {
        let start = Instant::now();
        let later = start + Duration::from_millis(100 * 16);
        let mut view = BlindView::default();
        // Another unit turning blind, or ending it, leaves the player's overlay alone.
        view.update(false, true, start);
        assert!(!view.showing());
        view.update(true, true, start);
        view.update(false, false, later);
        assert!(view.showing());
        // A repeat while it shows does not start the fade again.
        view.update(true, true, later);
        assert_eq!(view.ticks(later), 100);
        assert_eq!(view.alpha(start), 0.0);
        view.update(true, false, later);
        assert!(!view.showing());
        assert_eq!(view.alpha(later), 0.0);
    }

    #[test]
    fn blind_fades_one_alpha_a_tick() {
        assert_eq!(blind_alpha(0), 0.0);
        assert_eq!(blind_alpha(100), 100.0 / 255.0);
        assert_eq!(blind_alpha(255), 1.0);
        assert_eq!(blind_alpha(400), 1.0);
        let start = Instant::now();
        let mut view = BlindView::default();
        view.update(true, true, start);
        assert_eq!(view.alpha(start), blind_alpha(0));
        assert_eq!(view.alpha(start + Duration::from_millis(100 * 16)), blind_alpha(100));
        assert_eq!(view.alpha(start + Duration::from_millis(255 * 16)), blind_alpha(255));
        assert_eq!(view.alpha(start + Duration::from_millis(400 * 16)), blind_alpha(400));
        assert_eq!(blind_color(1.0), [10.0 / 255.0, 10.0 / 255.0, 10.0 / 255.0, 1.0]);
    }

    #[test]
    fn blind_squares_keep_the_clear_corner_at_the_centre() {
        let pieces = blind_pieces();
        let squares = pieces.into_iter().filter(|piece| piece.fullb).collect::<Vec<_>>();
        let bands = pieces.into_iter().filter(|piece| !piece.fullb).collect::<Vec<_>>();
        assert_eq!((squares.len(), bands.len()), (4, 4));
        for square in &squares {
            let centre = square.corners.iter().position(|[x, y]| *x == 0.0 && *y == 0.0).unwrap();
            assert_eq!(square.uv[centre], [0.0, 0.0]);
        }
        assert_eq!(squares[0].corners, [[0.0, 0.0], [65.0, 0.0], [65.0, 65.0], [0.0, 65.0]]);
        assert_eq!(squares[0].uv, [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
        assert_eq!(squares[1].corners, [[-65.0, 0.0], [0.0, 0.0], [0.0, 65.0], [-65.0, 65.0]]);
        assert_eq!(squares[1].uv, [[1.0, 0.0], [0.0, 0.0], [0.0, 1.0], [1.0, 1.0]]);
        assert_eq!(squares[2].corners, [[0.0, -65.0], [65.0, -65.0], [65.0, 0.0], [0.0, 0.0]]);
        assert_eq!(squares[2].uv, [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]]);
        assert_eq!(squares[3].corners, [[-65.0, -65.0], [0.0, -65.0], [0.0, 0.0], [-65.0, 0.0]]);
        assert_eq!(squares[3].uv, [[1.0, 1.0], [0.0, 1.0], [0.0, 0.0], [1.0, 0.0]]);
        assert_eq!(bands[0].corners, [[-165.0, -165.0], [165.0, -165.0], [165.0, -65.0], [-165.0, -65.0]]);
        assert_eq!(bands[1].corners, [[-165.0, 65.0], [165.0, 65.0], [165.0, 165.0], [-165.0, 165.0]]);
        assert_eq!(bands[2].corners, [[-165.0, -65.0], [-65.0, -65.0], [-65.0, 65.0], [-165.0, 65.0]]);
        assert_eq!(bands[3].corners, [[65.0, -65.0], [165.0, -65.0], [165.0, 65.0], [65.0, 65.0]]);
        for band in &bands {
            assert_eq!(band.uv, QUAD_UV);
        }
        assert_eq!(blind_center([1.0, 2.0, 3.0]), [1.0, 11.0, 3.0]);
        assert_eq!(blind_offsets(&squares[0])[2], [65.0 * 7.0, 65.0 * 7.0]);
    }

    #[test]
    fn deep_blind_keeps_its_effect_and_drops_the_wav() {
        let table = Table::load();
        assert_eq!(table.status(DEEP_BLIND), &[334, 164]);
        assert!(!table.sounds.contains_key(&334));
    }

    #[test]
    fn hammer_fall_maximize_power_blast_mine_and_kyrie_sound_at_the_games_ticks() {
        let table = Table::load();
        let sounds = |id: u32| table.sounds[&id].iter().map(|sound| (sound.file.as_str(), sound.delay_ms)).collect::<Vec<_>>();
        assert_eq!(sounds(102), [("effect\\wizard_fire_pillar_b", 480)]);
        assert_eq!(table.quakes[&102], [(Duration::from_millis(560), QUAKE_LONG)]);
        assert_eq!(
            sounds(104),
            [
                ("effect\\black_maximize_power_circle", 0),
                ("effect\\black_maximize_power_sword", 640),
                ("effect\\black_maximize_power_sword", 752),
                ("effect\\black_maximize_power_sword_bic", 1040)
            ]
        );
        assert_eq!(sounds(106), [("effect\\hunter_blastmine", 320)]);
        assert_eq!(sounds(112), [("effect\\priest_kyrie_eleison_b", 0), ("effect\\priest_kyrie_eleison_a", 240)]);
        for (id, file) in [(103, "black_weapon_perfection"), (107, "hunter_claymoretrap"), (108, "hunter_freezingtrap"), (111, "hunter_springtrap")] {
            assert_eq!(sounds(id), [(format!("effect\\{file}").as_str(), 0)], "{id}");
        }
        // Every use packet plays Maximize Power's effect on its source, the one that ends it too.
        assert_eq!(table.used(114), Some(&[(Who::Caster, 104)][..]));
        // The traps' triggers by unit kind; a fired trap (0x8c) has none.
        let traps = [(143, 106), (149, 139), (150, 99), (151, 108), (152, 107), (214, 107), (215, 108), (216, 107), (217, 107)];
        let new = [(190, 107), (193, 108), (228, 106), (230, 106), (191, 99), (194, 99), (264, 99)];
        for (kind, effect) in traps.into_iter().chain(new) {
            assert_eq!(table.trap(kind), Some(effect), "{kind}");
        }
        assert_eq!(table.trap(0x8c), None);
    }

    #[test]
    fn water_ball_detecting_enchant_poison_and_over_thrust_play_in_code() {
        let table = Table::load();
        // Water Ball: one 117 on the caster for each damage packet, flown in code; its sound stays.
        assert!(AIMED.contains(&WATER_BALL) && !aimed_at_centre(WATER_BALL));
        assert_eq!(table.struck(86), Some(&[(Who::Caster, 117)][..]));
        assert!(!table.floaters.contains_key(&117) && table.sounds.contains_key(&117));
        // Enchant Poison and Venom Splasher on their targets, a monster's poison at its first hit; two sounds at tick 0.
        assert_eq!(table.used(138), Some(&[(Who::Target, 125)][..]));
        assert_eq!(table.used(141), Some(&[(Who::Target, 125)][..]));
        assert_eq!(table.hit(176), Some([Some(0), Some(125)]));
        let sounds = |id: u32| table.sounds[&id].iter().map(|sound| (sound.file.as_str(), sound.delay_ms)).collect::<Vec<_>>();
        assert_eq!(sounds(125), [("effect\\assasin_enchantpoison", 0), ("effect\\EF_PoisonAttack", 0)]);
        // Over Thrust: both sounds at tick 0, on the source.
        assert_eq!(sounds(128), [("effect\\black_overthrust", 0), ("effect\\EF_StoneCurse", 0)]);
        assert_eq!(table.used(113), Some(&[(Who::Caster, 128)][..]));
        assert!(first::makes(119) && first::makes(125) && sounds(119) == [("effect\\hunter_detecting", 0)]);
    }

    #[test]
    fn walls_and_puffs_are_built_in_code() {
        let table = Table::load();
        // Fire Pillar, Defender and Grand Cross no longer play from rings, Venom Dust and Explosion Spirits from parts.
        for id in [138, 222, 226] {
            assert!(!table.rings.contains_key(&id), "{id}");
        }
        assert!(!table.floaters.contains_key(&171) && !table.floaters.contains_key(&203));
        assert_eq!((table.units[&135], table.units[&197], table.units[&146]), (138, 138, 171));
        assert!(table.units_once.contains(&135) && table.units_once.contains(&146));
        // Shockwave's waves loop on the trap and the shocker every 75 ticks, with no sound.
        assert_eq!((table.units[&148], table.units[&216]), (145, 145));
        assert!(!table.units_once.contains(&148) && !table.units_once.contains(&216) && !table.sounds.contains_key(&145));
        assert!(table.parts.contains_key(&145));
        assert_eq!((restart_ticks(145, 80), restart_ticks(145, 200), restart_ticks(25, 40)), (75, 75, 32));
        // Explosion Spirits' glints: one set of four against Steel Body's sixteen; both forced or not as the list says.
        assert_eq!((4 * glint_sets(203), 4 * glint_sets(254)), (4, 16));
        assert!(force_drawn(203) && force_drawn(138) && !force_drawn(254));
        assert_eq!(table.status(86), &[203, 166]);
        assert_eq!(table.used(270), Some(&[(Who::Caster, 203), (Who::Caster, 261)][..]));
        // Berserk Pitcher's sound and quake at tick 18 (288 ms), heard at full volume from the listener's own place.
        assert_eq!(table.sounds[&220].iter().map(|sound| (sound.file.as_str(), sound.delay_ms)).collect::<Vec<_>>(), [("effect\\ac_concentration", 288)]);
        assert_eq!(table.quakes[&220], [(Duration::from_millis(288), QUAKE_SHORT)]);
        assert!(LISTENER[0].is_nan());
        assert_eq!((table.used(446), table.used(578)), (Some(&[(Who::Target, 220)][..]), Some(&[(Who::Caster, 220)][..])));
        // Defender plays on both packets; Grand Cross on each damage packet, its sound with each.
        assert_eq!((table.used(257), table.used(8219)), (Some(&[(Who::Caster, 222)][..]), Some(&[(Who::Caster, 222)][..])));
        assert_eq!(table.struck(254), Some(&[(Who::Caster, 226)][..]));
        assert_eq!(table.sounds[&226].len(), 1);
        // Body colours multiply.
        assert_eq!(mix_tints(Some([250, 120, 120]), Some([128, 255, 0])), Some([125, 120, 0]));
        assert_eq!((mix_tints(None, Some([1, 2, 3])), mix_tints(Some([4, 5, 6]), None), mix_tints(None, None)), (Some([1, 2, 3]), Some([4, 5, 6]), None));
    }

    #[test]
    fn intimidate_bursts_shards_and_holy_cross_sounds_at_the_listener() {
        let table = Table::load();
        // One burst for each of Intimidate's damage packets on its caster, Expulsion's on its target.
        assert_eq!((table.struck(219), table.struck(674)), (Some(&[(Who::Caster, 227)][..]), Some(&[(Who::Target, 227)][..])));
        assert!(table.skill(219, "effectId").is_empty());
        let sounds = |id: u32| table.sounds[&id].iter().map(|sound| (sound.file.as_str(), sound.delay_ms, sound.listener)).collect::<Vec<_>>();
        assert_eq!(sounds(227), [("effect\\EF_Bash", 0, true), ("effect\\rog_intimidate", 560, false)]);
        assert_eq!(sounds(245), [("effect\\cru_holy cross", 0, true)]);
        assert_eq!(sounds(220), [("effect\\ac_concentration", 288, true)]);
        // Other sounds keep their place.
        let mut listeners: Vec<u32> = table.sounds.iter().filter(|(_, sounds)| sounds.iter().any(|sound| sound.listener)).map(|(&id, _)| id).collect();
        listeners.sort_unstable();
        // (Every other sound keeps its place: the list grows with the effects that are heard at full volume.)
        for id in [220, 227, 245, 266, 497, 606, 607] {
            assert!(listeners.contains(&id), "{id}");
        }
    }

    #[test]
    fn six_cast_auras_bloom_cones_and_play_sound() {
        let auras = [1872, 1876, 1902, 1910, 1918, 1923];
        // is_cast_aura and is_banded are true for all six.
        for &id in &auras {
            assert!(is_cast_aura(id) && is_banded(id));
            // aura_ticks is 56 for casts of 0 and 500 ms and 125 for 2000 ms.
            assert_eq!([aura_ticks(id, 0), aura_ticks(id, 500), aura_ticks(id, 2000)], [56, 56, 125]);
        }
        // aura_look gives the texture, colour and blend of the table (1872, 1876, 1923 not additive; the other three additive).
        assert_eq!(aura_look(1872), ("ring_white", [0x2B, 0x28, 0x2E], false));
        assert_eq!(aura_look(1876), ("ring_white", [0x7D, 0xB6, 0xA0], false));
        assert_eq!(aura_look(1923), ("ring_red", [0xB4, 0x37, 0x1E], false));
        assert_eq!(aura_look(1902), ("new_abr_mother\\abr_mother_ring", [0xFF, 0xFF, 0xFF], true));
        assert_eq!(aura_look(1910), ("new_abr_infinity\\ring_infinity", [0xFF, 0xFF, 0xFF], true));
        assert_eq!(aura_look(1918), ("new_d_cannon\\abr_dual_cannon_ring", [0xFF, 0xFF, 0xFF], true));

        // each makes 8 bands (two sets of 4) with heights 20, 19, 18, 17 in each set.
        let now = Instant::now();
        for &id in &auras {
            let banded = Banded { effect: id, anchor: Anchor::Unit(None), start: now, ticks: 100, turns: [0.0; 3], extra: [0.0; 2], sparkles: Vec::new() };
            let bands = banded.bands(0);
            assert_eq!(bands.len(), 8);
            let heights: Vec<f32> = bands.iter().map(|(shape, _)| shape.height).collect();
            assert_eq!(heights, vec![20.0, 19.0, 18.0, 17.0, 20.0, 19.0, 18.0, 17.0]);
        }

        // a second cast aura on the same caster replaces the first.
        let mut effects = Effects::default();
        effects.auras.push(Banded {
            effect: 1872,
            anchor: Anchor::Unit(Some(42)),
            start: now,
            ticks: 56,
            turns: [0.0; 3],
            extra: [0.0; 2],
            sparkles: Vec::new(),
        });
        effects.auras.retain(|aura| !(is_cast_aura(aura.effect) && matches!(aura.anchor, Anchor::Unit(each) if each == Some(42))));
        effects.auras.push(Banded {
            effect: 1902,
            anchor: Anchor::Unit(Some(42)),
            start: now,
            ticks: 56,
            turns: [0.0; 3],
            extra: [0.0; 2],
            sparkles: Vec::new(),
        });
        assert_eq!(effects.auras.len(), 1);
        assert_eq!(effects.auras[0].effect, 1902);

        // wav rows for the six.
        let table = Table::load();
        for &id in &auras {
            let sounds = &table.sounds[&id];
            assert_eq!(sounds.len(), 1);
            assert_eq!(sounds[0].file, "effect\\ef_beginspell");
            assert_eq!(sounds[0].delay_ms, 0);
        }

        // the cast rows of #680 name them.
        assert_eq!(table.cast_aura(5331, 12), Some(1872));
        assert_eq!(table.cast_aura(5333, 12), Some(1876));
        assert_eq!(table.cast_aura(5304, 12), Some(1902));
        assert_eq!(table.cast_aura(5305, 12), Some(1910));
        assert_eq!(table.cast_aura(5303, 12), Some(1918));
        assert_eq!(table.cast_aura(5335, 12), Some(1923));
    }

    #[test]
    fn blind_showing_for_health_bit_and_deep_blind() {
        let start = Instant::now();
        let mut view = BlindView::default();
        // bit 0x10 alone turns blind showing on
        view.update(true, (BLIND_BIT & BLIND_BIT) != 0, start);
        assert!(view.showing());
        view.update(true, false, start);
        assert!(!view.showing());
        // status 1205 (DEEP_BLIND) alone turns blind showing on
        let status = DEEP_BLIND;
        view.update(true, status == DEEP_BLIND, start);
        assert!(view.showing());
        view.update(true, false, start);
        assert!(!view.showing());
    }

    #[test]
    fn shield_charge_providence_reflect_shield_absorb_spirits_and_the_endows() {
        let table = Table::load();
        // Shield Charge's anchor sits on the feet, every other file's 80 px up.
        assert_eq!((str_rise(246), str_rise(248), str_rise(220)), (0.0, 80.0, 80.0));
        assert_eq!(table.struck(250), Some(&[(Who::Target, 246)][..]));
        // Providence's file plays with its code-made tubes; Reflect Shield and Absorb Spirits are no longer rings and parts.
        assert_eq!(table.used(256), Some(&[(Who::Target, 248)][..]));
        for id in [252, 253] {
            assert!(!table.rings.contains_key(&id) && !table.floaters.contains_key(&id), "{id}");
        }
        assert_eq!((table.used(252), table.struck(8401).map(|list| list.contains(&(Who::Target, 252)))), (Some(&[(Who::Caster, 252)][..]), Some(true)));
        assert_eq!(
            (table.used(262), table.used(335), table.used(8604)),
            (Some(&[(Who::Caster, 253)][..]), Some(&[(Who::Target, 253)][..]), Some(&[(Who::Target, 253)][..]))
        );
        assert_eq!(table.sounds[&253].iter().map(|sound| sound.delay_ms).collect::<Vec<_>>(), [0]);
        // The Endows' sound comes at tick 10 (160 ms).
        for id in [255, 256] {
            assert_eq!(table.sounds[&id].iter().map(|sound| (sound.file.as_str(), sound.delay_ms)).collect::<Vec<_>>(), [("_enemy_hit_wind1", 160)], "{id}");
        }
    }

    #[test]
    fn endows_strips_ribbons_sparks_and_asura_play_as_the_game() {
        let table = Table::load();
        let sounds = |id: u32| table.sounds[&id].iter().map(|sound| (sound.file.as_str(), sound.delay_ms, sound.listener)).collect::<Vec<_>>();
        // The endows' wind at tick 10 and the strips' sound at tick 6; Steel Body's effect has no sound of its own.
        assert_eq!((sounds(257), sounds(258)), (vec![("_enemy_hit_wind1", 160, false)], vec![("_enemy_hit_wind1", 160, false)]));
        assert_eq!((sounds(269), sounds(270)), (vec![("effect\\복호격", 96, false)], vec![("effect\\복호격", 96, false)]));
        assert!(!table.sounds.contains_key(&261) && !table.rings.contains_key(&261));
        // Asura Strike's sound is heard at full volume, with a shake; Investigate's claw keeps its own at the caster.
        assert_eq!(sounds(266), [("effect\\mon_아수라 패황권", 0, true)]);
        assert_eq!(table.quakes[&266], [(Duration::ZERO, QUAKE_SHORT)]);
        assert!(!table.floaters.contains_key(&266) && sounds(267) == [("effect\\mon_침투경", 0, false)]);
        // The use packets of Steel Body, Explosion Spirits and SL_SKA sound at the listener, with a shake; the ribbons come with them.
        for skill in [268, 270, 472] {
            assert_eq!(table.skill_sounds[&skill].len(), 1, "{skill}");
            assert!(table.skill_sounds[&skill][0].listener && table.skill_quakes[&skill] == [(Duration::ZERO, QUAKE_SHORT)]);
        }
        assert_eq!(table.skill_sounds[&268][0].file, "effect\\mon_맹룡과강");
        assert_eq!((table.skill_sounds[&270][0].file.as_str(), table.skill_sounds[&472][0].file.as_str()), ("effect\\mon_폭기", "effect\\T_방어형"));
        assert!(table.used(268).unwrap().contains(&(Who::Caster, 261)) && table.used(270).unwrap().contains(&(Who::Caster, 261)));
        assert!(table.used(472).unwrap().contains(&(Who::Target, 261)));
        assert_eq!(table.item(684), Some(261));
        // Investigate: the sparks, then the claw, on each damage packet; both aimed at the target; Asura Strike on the target.
        assert_eq!(table.struck(266), Some(&[(Who::Caster, 264), (Who::Caster, 267)][..]));
        assert!(AIMED.contains(&264) && AIMED.contains(&267) && !AIMED.contains(&266));
        assert_eq!(table.struck(271), Some(&[(Who::Target, 266)][..]));
        assert!(force_drawn(261) && !force_drawn(266));
    }

    #[test]
    fn strips_coins_back_stab_raid_and_chemical_protection_play_as_the_game() {
        let table = Table::load();
        let sounds = |id: u32| table.sounds[&id].iter().map(|sound| (sound.file.as_str(), sound.delay_ms)).collect::<Vec<_>>();
        // The strips' sound at tick 6, Back Stab's at tick 20; the coins' and the star's stay at 0.
        assert_eq!((sounds(271), sounds(272)), (vec![("effect\\복호격", 96)], vec![("effect\\복호격", 96)]));
        assert_eq!(
            (sounds(275), sounds(274), sounds(300)),
            (vec![("effect\\rog_back stap", 320)], vec![("effect\\rog_steal coin", 0)], vec![("apocalips_attack", 0)])
        );
        // Coins and Raid's streaks are made in code, with no sound for Raid.
        assert!(!table.floaters.contains_key(&274) && !table.floaters.contains_key(&276) && !table.sounds.contains_key(&276));
        assert_eq!((table.used(211), table.used(214)), (Some(&[(Who::Target, 274)][..]), Some(&[(Who::Caster, 276)][..])));
        // Back Stab's sparks fly from the caster to the target.
        assert_eq!(table.struck(212), Some(&[(Who::Caster, 275)][..]));
        assert!(AIMED.contains(&275));
        // Chemical Protection on the target of all five skills.
        for skill in [234, 235, 236, 237, 479] {
            assert!(table.used(skill).unwrap().contains(&(Who::Target, 300)), "{skill}");
        }
        // Frost Joke and Scream have no picture of their own to draw.
        assert_eq!((table.used(318), table.used(326)), (Some(&[(Who::Caster, 295)][..]), Some(&[(Who::Caster, 296)][..])));
    }

    #[test]
    fn demonstration_boxes_triple_attack_auras_slim_pitcher_and_full_protection() {
        let table = Table::load();
        // Demonstration's flame: a frame every 64 ms from frame 1 over 5 frames, 1.2 times as big, at alpha 200; the ground
        // packet shows nothing.
        let frame = |tick: u32| (animation_progress(16.0 * tick as f32, 64.0, 5.0, 1.0, true) * 5.0) as usize;
        assert_eq!([0, 3, 4, 12, 16, 19, 20].map(frame), [1, 1, 2, 4, 0, 0, 1]);
        let flame = &table.sprites[&302][0];
        assert_eq!(flame.parameters.get("delayFrame").map(String::as_str), Some("64"));
        assert!(flame.parameters.get("scale").is_some_and(|scale| scale == "1.2") && flame.parameters.get("alpha").is_some_and(|alpha| alpha == "200"));
        assert!(table.skill(229, "groundEffectId").is_empty());
        assert_eq!((table.units[&177], table.units[&132], table.units[&131]), (302, 318, 319));
        // Magnus' and Sanctuary's boxes are made in code, one a cell.
        assert!(!table.rings.contains_key(&318) && !table.rings.contains_key(&319));
        // Triple Attack: the sounds at ticks 10, 20 and 30, on the caster; the streaks aimed at the target.
        let sounds = |id: u32| table.sounds[&id].iter().map(|sound| (sound.file.as_str(), sound.delay_ms)).collect::<Vec<_>>();
        assert_eq!(sounds(329), [("effect\\ef_hit2", 160), ("effect\\ef_hit4", 320), ("effect\\ef_hit2", 480)]);
        assert_eq!(table.struck(263), Some(&[(Who::Caster, 329)][..]));
        assert!(AIMED.contains(&329));
        // The couple and blue casting auras bloom like Heal's: 56 ticks at least, pink and blue.
        assert_eq!((aura_ticks(342, 0), aura_ticks(441, 0), aura_ticks(342, 2000), aura_ticks(367, 0)), (56, 56, 125, 70));
        assert!(is_cast_aura(342) && is_cast_aura(441) && is_banded(342) && is_banded(441));
        assert_eq!((aura_look(342), aura_look(441)), (("ring_red", [0xFF, 0x59, 0xB6], true), ("ring_blue", [0x64, 0x64, 0xFF], true)));
        assert_eq!((sounds(342), sounds(441)), (vec![("effect\\EF_BeginSpell", 0)], vec![("effect\\EF_BeginSpell", 0)]));
        // Slim Pitcher's potion lands on the cell, with its sound at the listener.
        assert_eq!(table.ground(478), Some(&[(Who::Ground, 497)][..]));
        assert!(table.sounds[&497][0].listener && table.sounds[&497][0].file == "assulter_attack");
    }

    #[test]
    fn twilight_shrink_confine_pang_voice_wink_and_the_cones_play_in_code() {
        let table = Table::load();
        let sounds = |id: u32| table.sounds[&id].iter().map(|sound| (sound.file.as_str(), sound.delay_ms, sound.listener)).collect::<Vec<_>>();
        // Twilight Alchemy: the skills' effect rows no longer show 497 to 499; the cast auras are 573 to 575.
        for skill in [496, 497, 498] {
            assert!(table.skill(skill, "effectId").is_empty(), "{skill}");
        }
        assert_eq!([496, 497, 498].map(|skill| table.cast_aura(skill, 12)), [Some(573), Some(574), Some(575)]);
        // Shrink's sound at tick 5, Close Confine's at tick 20; Pang Voice's and Wink of Charm's at the listener at once.
        assert_eq!((sounds(599), sounds(602)), (vec![("effect\\EF_BeginSpell", 80, false)], vec![("effect\\ef_hit6", 320, false)]));
        assert_eq!((sounds(606), sounds(607)), (vec![("amon_ra_die01", 0, true)], vec![("effect\\vallentine", 0, true)]));
        assert!(!table.sprites.contains_key(&606) && !table.sprites.contains_key(&607));
        // Both are aimed from the caster at the target, played by the skill's use packet.
        assert!(AIMED.contains(&606) && AIMED.contains(&607));
        assert_eq!((table.used(1010), table.used(1011)), (Some(&[(Who::Caster, 606)][..]), Some(&[(Who::Caster, 607)][..])));
        assert_eq!((table.used(1002), table.used(1005)), (Some(&[(Who::Caster, 599)][..]), Some(&[(Who::Target, 602)][..])));
        // The performers' cones: the song first at the listener, the begin-spell sound at the unit.
        assert_eq!(sounds(840), [("effect\\EF_BeginSpell", 0, false)]);
        for (skill, file) in [(320, "effect\\석양의 어쌔신"), (329, "effect\\행운의"), (330, "effect\\당신을 위한 서비스")] {
            assert_eq!((table.skill_sounds[&skill][0].file.as_str(), table.skill_sounds[&skill][0].listener), (file, true), "{skill}");
            assert_eq!(table.used(skill).unwrap().last(), Some(&(Who::Caster, 840)));
        }
    }

    #[test]
    fn the_song_cones_and_the_dances_play_in_code() {
        let table = Table::load();
        // Each song's cone: the colour by id from the use rows; the sound at the unit.
        for (skill, cone) in [(322, 841), (307, 842), (309, 842), (310, 842), (312, 842), (313, 842), (317, 843), (325, 843), (321, 843)] {
            assert!(table.used(skill).unwrap().contains(&(Who::Caster, cone)), "{skill}");
        }
        for (skill, cone) in [(308, 844), (319, 844), (327, 844), (328, 844), (306, 845), (311, 845)] {
            assert!(table.used(skill).unwrap().contains(&(Who::Caster, cone)), "{skill}");
        }
        for id in 841..=845 {
            assert_eq!(table.sounds[&id].iter().map(|sound| (sound.file.as_str(), sound.delay_ms)).collect::<Vec<_>>(), [("effect\\EF_BeginSpell", 0)], "{id}");
        }
        // Adaptation on its caster; Eternal Chaos and Loki's Veil on the units with the status.
        assert_eq!((table.used(304), table.used(5068)), (Some(&[(Who::Caster, 848)][..]), Some(&[(Who::Caster, 848)][..])));
        assert!([79, 82, 448].iter().all(|status| table.status(*status).contains(&864)));
        assert!(!table.sounds.contains_key(&848) && !table.sounds.contains_key(&864));
    }

    #[test]
    fn song_circles_draw_over_everything_and_two_keep_after_they_end() {
        let table = Table::load();
        // The squares live 400 ticks and are drawn as quads over everything, their corners projected 7 pixels a unit.
        assert_eq!(PUFF_TICKS, 400);
        let near = |got: [f32; 2], want: [f32; 2]| (got[0] - want[0]).abs() < 0.01 && (got[1] - want[1]).abs() < 0.01;
        assert!(near(screen_offset([5.0, 0.0, 0.0], 0.0, 0.0), [35.0, 0.0]));
        assert!(near(screen_offset([0.0, 10.0, 0.0], 0.0, 0.0), [0.0, -70.0]), "up is up the screen");
        assert!(near(screen_offset([0.0, 10.0, 0.0], 0.0, 90.0), [0.0, 0.0]) && near(screen_offset([0.0, 0.0, 5.0], 0.0, 90.0), [0.0, 35.0]));
        // Song of Mana (439) and Wug Dance (441) ignore the off packet and an on-packet whose value isn't 1; the others don't.
        assert!(circle_ignores(439, false, 1) && circle_ignores(441, false, 0) && circle_ignores(439, true, 2) && circle_ignores(441, true, 0));
        assert!(!circle_ignores(439, true, 1) && !circle_ignores(441, true, 1));
        assert!(!circle_ignores(70, false, 0) && !circle_ignores(865, true, 5) && !circle_ignores(73, false, 1));
        // Steel Body's sound belongs to effect 1132, started by its status.
        assert!(table.status(87).contains(&254) && table.status(87).contains(&1132) && table.status(235).contains(&1132));
        assert_eq!(table.sounds[&1132].iter().map(|sound| (sound.file.as_str(), sound.delay_ms)).collect::<Vec<_>>(), [("effect\\mon_금강불괴", 0)]);
        assert!(!table.sounds.contains_key(&254));
    }

    #[test]
    fn ganbantein_spiral_pierce_soul_breaker_pressure_martyrs_reckoning_and_the_two_tints() {
        let table = Table::load();
        let sounds = |id: u32| table.sounds[&id].iter().map(|sound| (sound.file.as_str(), sound.delay_ms, sound.listener)).collect::<Vec<_>>();
        // Ganbantein's fire wall sound at the listener; Spiral Pierce's at ticks 20 and 30; Soul Breaker's at 20; Pressure's at 3.
        assert_eq!(sounds(223), [("effect\\EF_FireWall", 0, true)]);
        assert_eq!(table.ground(483), Some(&[(Who::Ground, 223)][..]));
        assert_eq!(sounds(339), [("permeter_attack", 320, false), ("effect\\ef_magnumbreak", 480, false)]);
        assert_eq!((sounds(361), sounds(365)), (vec![("effect\\기공포", 320, false)], vec![("effect\\프레셔", 48, false)]));
        // The shake of Pressure at tick 33; Martyr's Reckoning's sound at tick 22 and no old bash3d.
        assert_eq!(table.quakes[&365], [(Duration::from_millis(528), QUAKE_LONG)]);
        assert_eq!(sounds(366), [("effect\\세크리파이스", 352, false)]);
        for id in [361, 365, 366] {
            assert!(!table.floaters.contains_key(&id) && !table.rings.contains_key(&id), "{id}");
        }
        // Soul Breaker is aimed from the caster at the target; Spiral Pierce, Pressure and the rays play on their packet's units.
        assert!(AIMED.contains(&361));
        assert_eq!(
            (table.struck(379), table.struck(397), table.struck(367), table.struck(368)),
            (Some(&[(Who::Caster, 361)][..]), Some(&[(Who::Target, 339)][..]), Some(&[(Who::Target, 365)][..]), Some(&[(Who::Caster, 366)][..]))
        );
        // Aura Blade's second set of bands is yellow, the first white; the others have one texture.
        assert!(aura_set_texture(367, 0).ends_with("ring_white.tga") && aura_set_texture(367, 1).ends_with("ring_yellow.tga"));
        assert_eq!((aura_sets(367), aura_sets(12), aura_set_texture(12, 1)), (2, 1, aura_texture(12)));
        // Concentration tints the body #FFFFA0, drawn as it is.
        assert_eq!(crate::actor::status_body(369), Some((0, Some([0xFF, 0xFF, 0xA0]))));
        assert_eq!(table.status(379), &[744][..]);
    }

    #[test]
    fn gospel_soul_drain_magic_crasher_magic_power_indulge_and_soul_change_play_in_code() {
        let table = Table::load();
        let sounds = |id: u32| table.sounds[&id].iter().map(|sound| (sound.file.as_str(), sound.delay_ms, sound.listener)).collect::<Vec<_>>();
        // Gospel makes no sound itself and shows with its unit; Soul Drain's sparks are aimed, its target turns blue.
        assert!(!table.sounds.contains_key(&370) && table.units[&179] == 370 && table.units_once.contains(&179));
        assert_eq!((table.used(364), table.used(373)), (Some(&[(Who::Caster, 378), (Who::Target, 379)][..]), Some(&[(Who::Caster, 383)][..])));
        assert!(AIMED.contains(&378) && AIMED.contains(&384));
        assert!(!table.floaters.contains_key(&378) && !table.floaters.contains_key(&383));
        // Magic Crasher: the sound at 400 ms, the shake at 480 ms for 650; also from Soul Burn's use.
        assert_eq!(sounds(380), [("effect\\매직 크래쉬", 400, false)]);
        assert_eq!(table.quakes[&380], [(Duration::from_millis(480), QUAKE_LONG)]);
        assert!(table.used(375).unwrap().contains(&(Who::Target, 380)) && table.used(375).unwrap().contains(&(Who::Target, 406)));
        // Magic Power is a status only; its skill's sound is heard at the listener when used.
        assert!(table.used(366).is_none() && !table.sprites.contains_key(&382));
        assert!(table.status(113).contains(&382));
        assert_eq!((table.skill_sounds[&366][0].file.as_str(), table.skill_sounds[&366][0].listener), ("effect\\마법력 증폭", true));
        // Soul Change's tube from the caster, the target's own effect.
        assert_eq!(table.used(374), Some(&[(Who::Caster, 384), (Who::Target, 385), (Who::Caster, 385)][..]));
        assert_eq!(sounds(384), [("effect\\소울 체인지", 0, false)]);
    }

    #[test]
    fn the_snipers_effects_and_reject_swords_kept_smudges() {
        let table = Table::load();
        let sounds = |id: u32| table.sounds[&id].iter().map(|sound| (sound.file.as_str(), sound.delay_ms)).collect::<Vec<_>>();
        // Soul Change glows on both units; True Sight keeps its sound; Falcon Assault has none of its own, its shake at tick 30.
        assert_eq!(
            (table.used(374), table.used(380)),
            (Some(&[(Who::Caster, 384), (Who::Target, 385), (Who::Caster, 385)][..]), Some(&[(Who::Caster, 386)][..]))
        );
        assert_eq!(sounds(386), [("effect\\hunter_detecting", 0)]);
        assert!(!table.sounds.contains_key(&387));
        assert_eq!(table.quakes[&387], [(Duration::from_millis(480), QUAKE_LONG)]);
        // The shot lines are aimed from the caster; their sounds at tick 30. Meltdown plays a sound at last.
        assert!(AIMED.contains(&388) && AIMED.contains(&393));
        assert_eq!((table.struck(382), table.struck(394)), (Some(&[(Who::Caster, 388)][..]), Some(&[(Who::Caster, 393)][..])));
        assert_eq!((sounds(388), sounds(393)), (vec![("effect\\샤프슈팅", 480)], vec![("effect\\애로우 발칸", 480)]));
        assert_eq!(sounds(390), [("effect\\black_overthrust", 0)]);
        assert_eq!((table.used(383), table.struck(8407)), (Some(&[(Who::Caster, 389)][..]), Some(&[(Who::Target, 389)][..])));
        // Reject Sword keeps layers 9 and 10 frozen on their last key until tick 199, and plays 200 ticks.
        let key = rustnarok_client::effect::Key {
            frame: 100,
            kind: 0,
            position: [0.0; 2],
            corners: [0.0; 8],
            texture: 0.0,
            texture_motion: 0,
            delay: 0.0,
            angle: 0.0,
            color: [1.0; 4],
            source_blend: 5,
            destination_blend: 2,
        };
        let layer = rustnarok_client::effect::Layer { textures: Vec::new(), keys: vec![rustnarok_client::effect::Key { frame: 0, ..key }, key] };
        assert_eq!([9, 10, 8, 11].map(|index| kept_layer_frame(392, index, &layer, 150.0)), [Some(100.0), Some(100.0), None, None]);
        assert_eq!(
            (kept_layer_frame(392, 9, &layer, 199.0), kept_layer_frame(392, 9, &layer, 200.0), kept_layer_frame(391, 9, &layer, 150.0)),
            (Some(100.0), None, None)
        );
        assert_eq!(REJECT_SWORD_TICKS, 200);
    }

    #[test]
    fn moonlit_marionette_head_crush_joint_beat_napalm_vulcan_mind_breaker_and_the_web() {
        let table = Table::load();
        let sounds = |id: u32| table.sounds[&id].iter().map(|sound| (sound.file.as_str(), sound.delay_ms)).collect::<Vec<_>>();
        // Moonlit Water Mill shows with the dance's statuses and the use packet, and makes no sound.
        assert!(!table.sounds.contains_key(&394) && table.status(123).contains(&394) && table.status(646).contains(&394));
        assert_eq!(table.used(395), Some(&[(Who::Caster, 394)][..]));
        // Marionette's tube: aimed from the caster, sounding at once, ended by the control's status.
        assert!(AIMED.contains(&395) && table.used(396).unwrap().contains(&(Who::Caster, 395)));
        assert_eq!(sounds(395), [("effect\\소울 체인지", 0)]);
        assert!(table.status(121).contains(&396));
        // Head Crush's and Joint Beat's rays are made in code: no rings, the sound at tick 0 and 22.
        assert!(!table.rings.contains_key(&399) && !table.rings.contains_key(&400));
        assert_eq!((sounds(399), sounds(400)), (vec![("effect\\헤드 크러쉬", 0)], vec![("effect\\비트 조인트", 352)]));
        assert_eq!((table.struck(398), table.struck(399)), (Some(&[(Who::Caster, 399)][..]), Some(&[(Who::Caster, 400)][..])));
        // Napalm Vulcan's five sounds on its target; Mind Breaker's at tick 25.
        assert_eq!(sounds(401), [320, 480, 640, 800, 960].map(|ms| ("effect\\EF_NapalmBeat", ms)));
        assert_eq!(table.struck(400), Some(&[(Who::Target, 401)][..]));
        assert_eq!((sounds(403), table.used(402)), (vec![("effect\\swordman_provoke", 400)], Some(&[(Who::Target, 403)][..])));
        // The web: on the skill unit and on the status.
        assert_eq!(table.units[&183], 404);
        assert!(table.units_once.contains(&183) && table.status(735).contains(&404) && !table.sounds.contains_key(&404));
    }

    #[test]
    fn fog_wall_meteor_assault_deadly_poison_full_strip_preserve_and_the_pierce_cast() {
        let table = Table::load();
        let sounds = |id: u32| table.sounds[&id].iter().map(|sound| (sound.file.as_str(), sound.delay_ms)).collect::<Vec<_>>();
        // Fog Wall's boxes stand with their units, Meteor Assault's slashes come from the use and the pulse strikes.
        assert!(table.units[&182] == 405 && table.units_once.contains(&182) && !table.sounds.contains_key(&405));
        assert_eq!(
            (table.used(406), table.struck(661), table.struck(712)),
            (Some(&[(Who::Caster, 409)][..]), Some(&[(Who::Caster, 409)][..]), Some(&[(Who::Caster, 409)][..]))
        );
        assert!(!table.floaters.contains_key(&409) && sounds(409) == [("effect\\메테오 어썰트", 0)]);
        // Enchant Deadly Poison: six sounds, its own file; Full Strip: four, the strip sound; Preserve keeps its sound.
        assert_eq!(sounds(493), [0, 80, 176, 288, 416, 640].map(|ms| ("effect\\assasin_enchantpoison", ms)));
        assert_eq!(sounds(495), [16, 176, 336, 496].map(|ms| ("effect\\복호격", ms)));
        assert_eq!(sounds(496), [("effect\\black_maximize_power_sword_bic", 0)]);
        assert_eq!(
            (table.used(378), table.used(476), table.used(475)),
            (Some(&[(Who::Caster, 493)][..]), Some(&[(Who::Target, 495)][..]), Some(&[(Who::Caster, 496)][..]))
        );
        // Spiral Pierce's cast glows: its aura is 502, with the begin-spell sound.
        assert_eq!(table.cast_aura(397, 12), Some(502));
        assert_eq!(sounds(502), [("effect\\EF_BeginSpell", 0)]);
    }

    #[test]
    fn parses_refine_wav_delays() {
        let table = Table::load();
        let s154 = table.sounds.get(&154).and_then(|s| s.first()).expect("154 wav");
        assert_eq!(s154.file, "effect\\bs_refinesuccess");
        assert_eq!(s154.delay_ms, 240);
        let s155 = table.sounds.get(&155).and_then(|s| s.first()).expect("155 wav");
        assert_eq!(s155.file, "effect\\bs_refinefailed");
        assert_eq!(s155.delay_ms, 80);
    }

    #[test]
    fn self_destruction_blood_drain_storm_blast_rage_burst_windmill_and_dark_illusion_have_their_rows() {
        let table = Table::load();
        let sounds = |id: u32| table.sounds[&id].iter().map(|sound| (sound.file.as_str(), sound.delay_ms, sound.listener)).collect::<Vec<_>>();
        // Self-Destruction: three quakes of 200 ms at ticks 3, 7 and 9, none at 0.
        assert_eq!(table.quakes[&183], [48, 112, 144].map(|ms| (Duration::from_millis(ms), QUAKE_SHORT)));
        // Blood Drain, Storm Blast's ribbons and Rage Burst's rays are made in code: no part or ring rows.
        assert!(!table.floaters.contains_key(&BLOOD_DRAIN) && !table.rings.contains_key(&STORM_BLAST) && !table.rings.contains_key(&RAGE_BURST));
        assert!(AIMED.contains(&BLOOD_DRAIN) && AIMED.contains(&CHEMICAL));
        // Blood Sucker throws from its target; Spirit Recovery from the caster's cast; both with their sounds.
        assert!(table.used(2480).unwrap().contains(&(Who::Target, BLOOD_DRAIN)) && table.cast_aura(2464, 0) == Some(BLOOD_DRAIN));
        assert_eq!((table.skill_sounds[&2480][0].file.as_str(), table.skill_sounds[&2464][0].file.as_str()), ("effect\\S바큠익스트림", "effect\\_heal_effect"));
        // Storm Blast and Combo Finish: the ribbons and the hopping lines, with the sound and a long shake.
        let combo_finish = table.struck(273).unwrap();
        assert!(combo_finish.contains(&(Who::Target, STORM_BLAST)) && combo_finish.contains(&(Who::Target, HIT_LINE)));
        let used = table.used(2017).unwrap();
        assert!(used.contains(&(Who::Caster, 17)) && used.contains(&(Who::Target, STORM_BLAST)) && used.contains(&(Who::Target, HIT_LINE)));
        assert_eq!(sounds(HIT_LINE), [("effect\\mon_맹룡과강", 0, true)]);
        assert_eq!(table.quakes[&HIT_LINE], [(Duration::ZERO, QUAKE_LONG)]);
        // Chain Combo, Tiger Fist and Chain Crush Combo keep their other effects.
        assert!(table.struck(272).unwrap().contains(&(Who::Target, STORM_BLAST)) && table.struck(272).unwrap().contains(&(Who::Target, 262)));
        let tiger = table.struck(371).unwrap();
        assert!(tiger.contains(&(Who::Target, STORM_BLAST)) && tiger.contains(&(Who::Target, RAGE_BURST)));
        assert!(table.struck(372).unwrap().contains(&(Who::Target, STORM_BLAST)) && table.struck(372).unwrap().contains(&(Who::Target, 512)));
        // Rage Burst's sound comes at tick 5 (80 ms) at the unit; Windmill's too; Dark Illusion's at the listener, Sling Item too.
        assert_eq!(sounds(RAGE_BURST), [("effect\\mon_맹룡과강", 80, false)]);
        assert_eq!(sounds(STORM_KICK), [("effect\\T_회오리차기", 80, false)]);
        assert_eq!(sounds(CHEMICAL), [("effect\\G스포어익스플로젼", 0, true)]);
        assert!(table.used(2493).unwrap().contains(&(Who::Caster, CHEMICAL)) && table.struck(2023).unwrap().contains(&(Who::Caster, CHEMICAL)));
        assert!(table.used(2337).unwrap().contains(&(Who::Caster, STORM_KICK)) && table.struck(469).unwrap().contains(&(Who::Target, FOUR_WAY_BODY)));
    }

    #[test]
    fn poem_netherworld_dark_illusion_reverberation_marsh_cast_spin_refresh_and_hovering_have_their_rows() {
        let table = Table::load();
        let sounds = |id: u32| table.sounds[&id].iter().map(|sound| (sound.file.as_str(), sound.delay_ms, sound.listener)).collect::<Vec<_>>();
        // The blot: Close Confine's sprite from frame 1, 192 ms a frame, held, white at alpha 100, over everything.
        let blot = &table.sprites[&NPC_STOP][0];
        assert_eq!(blot.file, "cconfine");
        let animation = Animation::new(blot, NPC_STOP, Instant::now(), Anchor::Status(None, POEM_STATUS), true);
        assert!(animation.hold && !animation.repeat && animation.over && animation.first == 1.0 && animation.frame_ms == Some(192.0));
        assert!((animation.alpha * 255.0 - 100.0).abs() < 0.01);
        // The frame at ages 0, 11, 12, 48 and 500 ticks: 1, 1, 2, 5, 11 (of 12).
        let frame = |ticks: u32| (animation_progress(16.0 * ticks as f32, 192.0, 12.0, 1.0, false) * 12.0) as usize;
        assert_eq!([0, 11, 12, 48, 500].map(frame), [1, 1, 2, 5, 11]);
        // Dark Illusion's kick sound comes at tick 5; Knuckle Arrow's own at the listener; the targets shake.
        assert_eq!(sounds(457), [("effect\\t_날라차기", 80, false)]);
        assert_eq!(sounds(893), [("effect\\SR_KNUCKLEARROW", 0, true)]);
        for skill in [421, 2023, 2336] {
            assert!(table.struck(skill).unwrap().contains(&(Who::Target, QUAKE_BODY)), "{skill}");
        }
        // Reverberation's tornado: its sound at tick 0 on both traps.
        assert_eq!(sounds(REVERBERATION), [("effect\\REVERBERATION", 0, false)]);
        assert_eq!((table.trap(218), table.trap(222)), (Some(REVERBERATION), Some(REVERBERATION)));
        // The swing: Marsh of Abyss, Turn Kick (429 is its other) and Great Echo; its sound at tick 5, Great Echo's at the listener.
        assert!(table.used(2205).unwrap().contains(&(Who::Target, SPINED_BODY)) && table.struck(2426).unwrap().contains(&(Who::Target, SPINED_BODY)));
        assert_eq!(sounds(SPINED_BODY), [("effect\\T_돌려차기", 80, false)]);
        assert_eq!(sounds(863), [("effect\\LG_EARTHDRIVE", 0, true)]);
        assert_eq!(table.quakes[&863], [(Duration::ZERO, QUAKE_LONG)]);
        // The cast spin: the begin-spell sound; the casts of Invisibility, Feint Bomb and Teleport have it.
        assert_eq!(sounds(CAST_SPIN), [("effect\\EF_BeginSpell", 0, false)]);
        assert!([555, 2290, 2304].iter().all(|&skill| table.cast_aura(skill, 12) == Some(CAST_SPIN)));
        // Refresh and Abundance: their sounds belong to the use; Hovering and Emergency Cool have none under 543.
        assert_eq!(table.skill_sounds[&2013][0].file, "_heal_effect");
        assert_eq!(table.skill_sounds[&2019][0].file, "effect\\흡기");
        assert!(!table.sounds.contains_key(&HATED));
        assert_eq!((table.skill_sounds[&2263][0].file.as_str(), table.skill_sounds[&2269][0].file.as_str()), ("effect\\HOVERING", "effect\\ef_decagility"));
        assert!(table.used(2263).unwrap().contains(&(Who::Caster, HATED)) && table.used(2013).unwrap().contains(&(Who::Caster, GREEN_BODY)));
    }

    #[test]
    fn sky_net_blow_crush_strike_renovatio_hundred_spear_oratio_marsh_and_camouflage_have_their_rows() {
        let table = Table::load();
        let sounds = |id: u32| table.sounds[&id].iter().map(|sound| (sound.file.as_str(), sound.delay_ms, sound.listener)).collect::<Vec<_>>();
        // Sky Net Blow: the red flash and the late shake on its target; Cross Ripper Slasher the shake.
        assert!(table.struck(2327).unwrap().contains(&(Who::Target, RED_HIT)) && table.struck(2327).unwrap().contains(&(Who::Target, SKY_NET_SHAKE)));
        assert!(table.struck(2037).unwrap().contains(&(Who::Target, SKY_NET_SHAKE)));
        assert!(table.casters.get(&2270) == Some(&Some(RED_HIT)) && table.casters.get(&2520) == Some(&Some(RED_HIT)));
        // The splash: from frame 1, 4 ticks a frame, the 17 frames of 18, over everything; no sound of its own, the skills' with the damage.
        for (effect, frames) in [(589, 18.0), (729, 16.0)] {
            let part = &table.sprites[&effect][0];
            let animation = Animation::new(part, effect, Instant::now(), Anchor::Unit(Some(1)), false);
            assert!(animation.over && !animation.repeat && animation.frame_ms == Some(64.0) && animation.first == 1.0, "{effect}");
            let frame = |tick: u32| (animation_progress(16.0 * tick as f32, 64.0, frames, 1.0, false) * frames) as usize;
            let ends = (frames - 1.0) * 64.0 / 16.0;
            assert_eq!(frame(0), 1);
            assert_eq!(frame(4), 2);
            assert_eq!(frame(ends as u32 - 1), frames as usize - 1);
        }
        assert!(!table.sounds.contains_key(&589) && !table.sounds.contains_key(&719));
        assert_eq!((table.hit_sounds[&2012][0].file.as_str(), table.hit_sounds[&2031][0].file.as_str()), ("_enemy_hit_wind2", "effect\\EF_PoisonAttack"));
        assert!(table.hit_sounds[&2012][0].listener && table.hit_quakes[&2012] == [(Duration::ZERO, QUAKE_LONG)]);
        // Renovatio's sound belongs to the use; Hundred Spear's has no folder; Oratio's ring is Detecting's with its sound.
        assert_eq!(table.skill_sounds[&2050][0].file, "effect\\AB_renovatio");
        assert!(AIMED.contains(&SPEAR_FLASH) && table.struck(2004).unwrap().contains(&(Who::Caster, SPEAR_FLASH)));
        assert_eq!(sounds(SPEAR_FLASH), [("hundred", 0, false)]);
        assert_eq!(sounds(ORATIO), [("effect\\priest_benedictio", 0, false)]);
        assert_eq!(table.skill_sounds[&2205][0].file, "effect\\t_에너지방출");
        // Camouflage, Stealth Field and Elemental Veil: the cloaking sound at tick 0.
        assert_eq!(sounds(CLOAKING), [("effect\\assasin_cloaking", 0, false)]);
        assert!([360, 379, 1301].iter().all(|&status| table.status(status).contains(&CLOAKING)));
    }

    #[test]
    fn detonator_release_warg_dash_oratio_enchant_blade_the_poisons_and_weapon_crush_have_their_rows() {
        let table = Table::load();
        let sounds = |id: u32| table.sounds[&id].iter().map(|sound| (sound.file.as_str(), sound.delay_ms, sound.listener)).collect::<Vec<_>>();
        // Detonator's sound; Release's at tick 5 at the listener, launched from the caster at the target.
        assert_eq!(sounds(CONCENTRATION2), [("effect\\ac_concentration", 0, false)]);
        assert_eq!(sounds(RELEASE), [("effect\\EF_FireBall", 80, true)]);
        assert!(AIMED.contains(&RELEASE) && table.used(2230).unwrap().contains(&(Who::Caster, RELEASE)));
        // Warg Dash leaves copies like the quickens; Oratio's circle is on its status.
        assert!(table.status(358).contains(&crate::actor::AFTERIMAGE) && table.status(330).contains(&MAGIC_CIRCLE));
        // Enchant Blade and its sisters: no sound of their own, the shake at tick 18, the casting aura on the last two.
        assert!(!table.sounds.contains_key(&POTION_BERSERK2));
        assert_eq!(table.quakes[&POTION_BERSERK2], [(Duration::from_millis(288), QUAKE_SHORT)]);
        assert_eq!((table.skill_sounds[&2001][0].file.as_str(), table.skill_sounds[&2014][0].file.as_str()), ("effect\\닌자_던지기", "effect\\닌자_던지기"));
        assert!(!table.skill_sounds.contains_key(&2015) && !table.skill_sounds.contains_key(&2018));
        assert!(table.used(2015).unwrap().contains(&(Who::Caster, 441)) && table.used(2018).unwrap().contains(&(Who::Caster, 441)));
        // The poisons: Antidote's and Poison Smoke's cast flicker; their skills' sounds.
        assert!(table.used(2026).unwrap().contains(&(Who::Target, PURPLE_BODY)) && table.cast_aura(2032, 12) == Some(PURPLE_BODY));
        assert_eq!((table.skill_sounds[&2028][0].file.as_str(), table.skill_sounds[&2021][0].file.as_str()), ("effect\\t_낙법", "effect\\수둔"));
        assert_eq!((table.hit_sounds[&2034][0].file.as_str(), table.hit_sounds[&2031][0].file.as_str()), ("effect\\mon_맹룡과강", "effect\\EF_PoisonAttack"));
        // Cross Ripper Slasher: five sounds at the listener at ticks 5, 20, 35, 50, 65.
        assert!(table.struck(2037).unwrap().contains(&(Who::Caster, 769)));
        assert_eq!(sounds(769), [80, 320, 560, 800, 1040].map(|ms| ("effect\\T_날라차기", ms, true)));
        // Weapon Crush: four sounds on the target.
        assert_eq!(sounds(WEAPON_CRUSH), [16, 176, 336, 496].map(|ms| ("effect\\복호격", ms, false)));
        assert!(table.struck(2030).unwrap().contains(&(Who::Target, WEAPON_CRUSH)));
    }

    #[test]
    fn poisoning_weapon_poison_smoke_phantom_menace_the_mechanic_spin_vulcan_arm_and_acceleration_have_their_rows() {
        let table = Table::load();
        let sounds = |id: u32| table.sounds[&id].iter().map(|sound| (sound.file.as_str(), sound.delay_ms, sound.listener)).collect::<Vec<_>>();
        // Two sounds at tick 1 at the listener.
        assert_eq!(sounds(771), [("effect\\assasin_enchantpoison", 16, true), ("effect\\EF_PoisonAttack", 16, true)]);
        // Poison Smoke's cells keep 772 with their unit; Phantom Menace's crescents come with the damage.
        assert_eq!(table.units[&225], POISON_SMOKE_CELL);
        assert!(table.units_once.contains(&225));
        assert!(table.struck(2034).unwrap().contains(&(Who::Caster, PHANTOM_RIBBONS)));
        // The Mechanic's cast spin: 2267 and 2280, the begin-spell sound; Vulcan Arm's sound at tick 45.
        assert!([2267, 2280].iter().all(|&skill| table.cast_aura(skill, 12) == Some(CAST_SPIN2)));
        assert_eq!(sounds(CAST_SPIN2), [("effect\\EF_BeginSpell", 0, false)]);
        assert!(table.struck(2258).unwrap().contains(&(Who::Caster, 776)));
        assert_eq!(sounds(776), [("effect\\데스페라도", 720, true)]);
        // Acceleration and Shape Shift: their sounds at tick 0; the Side Slides' ring after Hovering.
        assert_eq!((sounds(ACCELERATION), sounds(SHAPE_SHIFT)), (vec![("effect\\EF_IncAgility", 0, false)], vec![("effect\\소울 체인지", 0, false)]));
        assert!(table.used(2262).unwrap().contains(&(Who::Caster, ACCELERATION)) && table.used(2268).unwrap().contains(&(Who::Caster, SHAPE_SHIFT)));
        assert!([2263, 2264, 2265].iter().all(|&skill| table.used(skill).unwrap().contains(&(Who::Caster, first::SIDE_SLIDE))));
    }
}
