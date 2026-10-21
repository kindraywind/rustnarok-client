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
    balls, blitz, body, bolts, boomerang, brawl, burst, cells, chi, circle, crowds, dancer, emblems, first, ground, guard, holes, huujin, kicks, knight,
    linker, mechanic, monk, notes, painter, petals, portal, sage, shafts, shards, sheet, sight, sniper, snow, songs, soul, souls, spears, sphere, spray, star,
    stone, tanji, throw, transcend, vortex, walls, waterball, waves, wheels,
};
use crate::render::{Gpu, Texture};
use crate::world::{self, CELL, EffectMesh, EffectQuad, MeshVertex, Vector};
use crate::world_fx::{self, WorldFx};
use crate::world_str::{self, Worlds};

/// A quad's corners as texture coordinates: top left, top right, bottom right, bottom left.
pub(crate) const QUAD_UV: [[f32; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];

/// An STR moves on one frame each 16 ms tick, whatever its file says.
const STR_TICK_MS: f32 = 16.0;
/// How far the black fill of an enchant screen reaches from its point, in px: past any screen.
const WORLD_TINT_REACH: f32 = 4000.0;
/// The most ticks the STR world copies run in one frame.
const WORLD_TICKS_MAX: u32 = 10;
/// The STR world copies' tick, exact: a float 16 ms comes out a hair long and drops every other tick at 16 ms frames.
const WORLD_TICK: Duration = Duration::from_millis(16);
/// An STR's (320, 240) sits this many screen pixels above its owner's feet, at any zoom.
pub const STR_RISE: f32 = 80.0;

/// How many pixels above the feet an effect's STR is anchored: 80, but for Shield Charge's anchor, which sits on the feet.
pub fn str_rise(effect: u32) -> f32 {
    match effect {
        SHIELD_CHARGE => 0.0,
        // Offertorium's ring: the file itself puts it at head height.
        OFFERTORIUM_RING => 5.0,
        _ => STR_RISE,
    }
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
    /// After the file's last key its last frame stays drawn to the effect's length (`hold`).
    hold: bool,
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
    /// The units whose effect plays on a grid of points round their cell, again and again: the effect and how far below the ground
    /// its anchor is put (`area`).
    areas: HashMap<u32, (u32, f32)>,
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
    casts: HashMap<u16, Vec<i32>>,
    struck: HashMap<u16, Vec<(Who, u32)>>,
    /// Skills without damage, and ground skills (at the cell or on the caster).
    used: HashMap<u16, Vec<(Who, u32)>>,
    grounds: HashMap<u16, Vec<(Who, u32)>>,
    /// What a trap (by skill unit type) shows when it goes off.
    traps: HashMap<u32, u32>,
    /// What a status (by EFST id) plays again and again on its unit while it's on.
    statuses: HashMap<u16, Vec<u32>>,
    /// The effect started instead under /mineffect.
    min_ids: HashMap<u32, u32>,
    /// The STR world effects (world_str.txt), but for the ids a `str` row here plays.
    worlds: world_str::Table,
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
            areas: HashMap::new(),
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
            min_ids: HashMap::new(),
            worlds: world_str::Table::load(),
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
                        table.casts.entry(skill).or_default().push(effect);
                    }
                }
                ["str", _, file, folder, attached, ..] => {
                    let Some(id) = number(1) else { continue };
                    let folder = if folder == "-" { String::new() } else { folder.to_owned() };
                    let random = number(5).zip(number(6));
                    let once = fields.last() == Some(&"once");
                    let hold = fields.last() == Some(&"hold");
                    table.parts.entry(id).or_default().push(Part { file: file.to_owned(), folder, attached: attached == "1", random, once, hold });
                }
                ["area", kind, effect, lift] => {
                    if let (Ok(kind), Ok(effect), Ok(lift)) = (kind.parse(), effect.parse(), lift.parse()) {
                        table.areas.insert(kind, (effect, lift));
                    }
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
                ["minid", ..] => {
                    if let (Some(id), Some(min)) = (number(1), number(2)) {
                        table.min_ids.insert(id, min);
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
        // An id with a `str` row here keeps playing as it does.
        for id in table.parts.keys() {
            table.worlds.remove(*id);
        }
        table
    }

    /// Whether an STR row of this table plays effect `id`.
    #[cfg(test)]
    pub fn has_str(&self, id: u32) -> bool {
        self.parts.contains_key(&id)
    }

    /// The row of the STR world effect `id`.
    pub fn world(&self, id: u32) -> Option<&world_str::Row> {
        self.worlds.get(id)
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
        match self.casts.get(&skill).and_then(|rows| rows.first()) {
            Some(-1) => None,
            Some(12) => Some(by_element),
            Some(&effect) => u32::try_from(effect).ok(),
            None if NO_CASTER_EFFECT.iter().any(|range| range.contains(&skill)) => None,
            None => Some(CASTER_EFFECT),
        }
    }

    /// The effects a cast starts beside its aura: the rows of the skill after the first.
    pub fn cast_extras(&self, skill: u16) -> Vec<u32> {
        self.casts.get(&skill).into_iter().flat_map(|rows| rows.iter().skip(1)).filter_map(|&effect| u32::try_from(effect).ok()).collect()
    }

    pub fn caster(&self, skill: u16) -> Option<u32> {
        match self.casters.get(&skill) {
            Some(&effect) => effect,
            None if NO_CASTER_EFFECT.iter().any(|range| range.contains(&skill)) => None,
            None => Some(CASTER_EFFECT),
        }
    }

    /// The effect that starts for `id`: its `minid` one under /mineffect.
    pub fn min_id(&self, id: u32, min: bool) -> u32 {
        if min { self.min_ids.get(&id).copied().unwrap_or(id) } else { id }
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
pub(crate) struct Loaded {
    pub(crate) animation: Str,
    pub(crate) textures: Vec<Vec<Option<Texture>>>,
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
    /// Keeps its last frame to the effect's length (`hold`).
    hold: bool,
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
            hold: self.hold,
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
fn playing_ticks(effect: u32, str: &Str, length: Option<u32>, hold: bool) -> u32 {
    match (effect == REJECT_SWORD, hold, length) {
        (true, ..) => REJECT_SWORD_TICKS,
        // A held picture lasts the effect's length, however short its file is.
        (_, true, Some(length)) => length,
        _ => str_ticks(str, length),
    }
}

/// The frame an STR shows at `frame`: a held one stays on its last key's after the file ends.
fn held_frame(str: &Str, hold: bool, frame: f32) -> f32 {
    if hold { frame.min(str.last_key().max(0) as f32) } else { frame }
}

/// A skill unit whose effect plays on a grid of points round its cell: the unit, the effect, how far below the ground it is put,
/// the unit's range, the cell's centre, when the next grid starts and the ticks between grids.
struct Area {
    id: u32,
    effect: u32,
    lift: f32,
    range: u8,
    at: Vector,
    next: Instant,
    period: u32,
}

/// A frame's layers, each with its ACT alpha.
type Layers = Vec<(Placed, f32)>;

/// A sprite frame's layers plain white, each with the alpha the ACT gives it.
fn plain_layers(gpu: &Gpu, sprite: &mut SpriteSet, action: usize, frame: usize, frames: usize) -> Layers {
    let timing = Timing::Span((frame as f32 + 0.5) / frames as f32);
    let alphas: Vec<f32> = sprite.frame_at(gpu, action, 0, timing).iter().map(|(key, ..)| f32::from(key.3[3])).collect();
    sprite.frame_plain(gpu, action, timing).into_iter().enumerate().map(|(index, placed)| (placed, alphas.get(index).copied().unwrap_or(255.0))).collect()
}

/// How often a ground unit's effect starts again, in ticks: 8 before its length ends (Quagmire's 20), by the skill unit's type 1
/// before it for Wall of Thorns, the Insignias, Kaensin, Ground Drift and their like, 7 for Fire Walk.
fn restart_ticks(effect: u32, kind: u32, length: u32) -> u32 {
    // Shockwave's waves start again every 75 ticks.
    if effect == 145 {
        return 75;
    }
    // Offertorium's ring runs its 121 ticks and starts again.
    if effect == OFFERTORIUM_RING {
        return 121;
    }
    // Electric Walk's wave runs its 29 frames and one more tick.
    if effect == 926 {
        return 30;
    }
    length.saturating_sub(restart_early(effect, kind)).max(1)
}

/// How many ticks before its length a ground unit's effect starts again.
fn restart_early(effect: u32, kind: u32) -> u32 {
    match kind {
        228 | 230..=233 | 235 | 239..=244 | 258 => 1,
        220 | 221 => 7,
        // Kaensin's and Ground Drift's cells start the next copy a tick before this one ends.
        189..=194 => 1,
        _ if effect == 95 => 20,
        _ => 8,
    }
}

/// The STR frame `elapsed` into an effect: whole 16 ms ticks.
fn str_frame(elapsed: Duration) -> u32 {
    (elapsed.as_secs_f32() * 1000.0 / STR_TICK_MS) as u32
}

/// The screen direction φ (degrees, counter-clockwise on the screen from facing the camera) a unit facing `rot` shows to a
/// camera turned `yaw` degrees, both in whole degrees.
// ponytail: the camera angle is taken 0 … 359 first, so φ is never negative here.
pub fn screen_phi(yaw: f32, rot: f32) -> i32 {
    let phi = yaw.rem_euclid(360.0) as i32 + rot as i32;
    if phi >= 360 { phi - 360 } else { phi }
}

/// Cannon Spear's blade for a screen direction φ: one of eight, by the way it points; none for a negative φ.
pub fn cannon_sector(phi: i32) -> Option<u32> {
    Some(match phi {
        ..0 => return None,
        0..=22 | 337.. => 1404,
        23..=66 => 1403,
        67..=111 => 1402,
        112..=156 => 1401,
        157..=201 => 1400,
        202..=246 => 1407,
        247..=291 => 1406,
        _ => 1405,
    })
}

/// Dragon Breath's picture for a screen direction φ: the quadrant beside the caster it faces (fire, or water 8 ids on).
pub fn breath_sector(phi: i32, water: bool) -> u32 {
    let fire = match phi {
        1..=89 => 1381,
        90..=179 => 1380,
        180..=269 => 1383,
        _ => 1382,
    };
    if water { fire + 8 } else { fire }
}

/// What a Dragon Breath packet starts, each with where and its start delay in ms: the breath on the caster after 200 ms
/// unless that picture still runs there, then the burst on the target after 350 ms (its ground patch only without
/// /mineffect). Nothing without the caster's direction φ.
pub fn breath_launches(table: &Table, min: bool, skill: u16, phi: Option<i32>, running: impl Fn(u32) -> bool) -> Vec<(u32, Who, i32)> {
    let Some(phi) = phi else { return Vec::new() };
    let water = skill == WATER_BREATH;
    let breath = table.min_id(breath_sector(phi, water), min);
    let (burst, patch) = if water { (1386, 1387) } else { (1378, 1379) };
    let mut launches = Vec::new();
    if !running(breath) {
        launches.push((breath, Who::Caster, 200));
    }
    launches.push((table.min_id(burst, min), Who::Target, 350));
    if !min {
        launches.push((patch, Who::Target, 350));
    }
    launches
}

/// The blade's start delay in ms: the caster's swing, from the frame delay of the action it is in.
pub fn cannon_delay_ms(frame_delay: f32) -> i32 {
    (frame_delay * 100.0 + 100.0) as i32
}

/// What a Cannon Spear packet starts, with each one's start delay in ms: nothing while the caster's vortex still plays,
/// else the vortex at once and the blade after `delay_ms`; their lighter ids under /mineffect.
pub fn cannon_launches(table: &Table, min: bool, phi: i32, delay_ms: i32, playing: impl Fn(u32) -> bool) -> Vec<(u32, i32)> {
    let vortex = table.min_id(CANNON_VORTEX, min);
    if playing(vortex) {
        return Vec::new();
    }
    std::iter::once((vortex, 0)).chain(cannon_sector(phi).map(|blade| (table.min_id(blade, min), delay_ms))).collect()
}

/// How many whole world ticks fit in `elapsed`.
fn world_ticks(elapsed: Duration) -> u32 {
    (elapsed.as_micros() / WORLD_TICK.as_micros()) as u32
}

/// A key's corner in sprite pixels from the anchor: STR space is centred on (320, 240).
pub(crate) fn str_corner(position: [f32; 2], corner: [f32; 2], angle: f32) -> [f32; 2] {
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
    /// The ticks a frame as the first and how many it picks from (`frameTicks`, `frameTicksRan`), drawn again at each start.
    frame_ticks: Option<[u32; 2]>,
    /// Action 0, or this one when a draw of two comes up 0 (`sideAction`): the slash turned to one of two sides.
    side_action: Option<usize>,
    /// On a ground unit it starts again each time the unit's effect does (`restart`).
    restarts: bool,
    /// The table's length of its effect, for that restart.
    length: Option<u32>,
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
        let repeat = match part.parameters.get("repeat").map(String::as_str) {
            Some("true") => true,
            Some("false") => false,
            _ => lasting && !flag("hold"),
        };
        // `delay` (or `delayStart`): made late, nothing shows before.
        let start = start + Duration::from_secs_f32(number("delay").or(number("delayStart")).unwrap_or(0.0).max(0.0) / 1000.0);
        // `duration` (ms) ends a repeating one; `drawn` (ticks) ends one that plays once, whatever frames are left.
        let lasts = number("duration")
            .filter(|_| repeat)
            .map(|ms| Duration::from_secs_f32(ms.max(0.0) / 1000.0))
            .or(number("drawn").map(|ticks| Duration::from_millis(STR_TICK_MS as u64 * ticks.max(0.0) as u64)));
        Self {
            sprite: format!("data\\sprite\\이팩트\\{}", part.file),
            start,
            end: lasts.map(|lasts| start + lasts),
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
            frame_ticks: number("frameTicks").map(|base| [base as u32, number("frameTicksRan").map_or(1, |spread| spread.max(1.0) as u32)]),
            side_action: number("sideAction").map(|action| action as usize),
            restarts: flag("restart"),
            length: None,
            placed: Vec::new(),
            effect,
        }
    }

    /// The random choices of a start: the ticks a frame and the side of the slash.
    fn roll(&mut self, mut rand: impl FnMut(u32) -> u32) {
        if let Some([base, spread]) = self.frame_ticks {
            self.frame_ms = Some(STR_TICK_MS * (base + rand(spread)) as f32);
        }
        if let Some(side) = self.side_action {
            self.action = Some(if rand(2) == 0 { side } else { 0 });
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
    /// EF_HITDARK's: the sparks are near black (#141414) and `/mineffect` does not cut them.
    dark: bool,
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

/// The sparks' colour: white, or near black (#141414) for EF_HITDARK's.
fn spark_tint(dark: bool) -> [f32; 3] {
    if dark { [20.0 / 255.0; 3] } else { [1.0; 3] }
}

/// Whether a hit's sparks are drawn: `/mineffect` keeps only the ring of EF_HIT1, but EF_HITDARK always has its sparks.
fn sparks_shown(min: bool, dark: bool) -> bool {
    dark || !min
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
    matches!(status, 439 | 441 | 451) && (!on || value != 1)
}

/// A cast's aura: one a caster has at a time.
pub fn is_cast_aura(effect: u32) -> bool {
    matches!(
        effect,
        12 | HOMUN_CASTING | 54
            ..=59
                | TETRA_CASTING
                | SHADOW_CAST
                | DARK_CASTING
                | BUBBLE_CAST
                | COUPLE_CASTING
                | BLUE_CASTING
                | GREEN_CASTING
                | COMET_CAST
                | JACK_CAST
                | MAELSTROM_CAST
                | CLEARANCE_CAST
                | VARETYR_CAST
                | EARTH_CAST
                | CART_CAST
                | 1872
                | 1876
                | 1902
                | 1910
                | 1918
                | 1923
    )
}

/// The effects made of bands.
pub fn is_banded(effect: u32) -> bool {
    matches!(
        effect,
        12 | HOMUN_CASTING | 54
            ..=59
                | 304
                | 312
                | 313
                | 314
                | 315
                | 320
                | 325
                | 342
                | 344
                | 367
                | 441
                | TETRA_CASTING
                | GREEN_CASTING
                | SHADOW_CAST
                | DARK_CASTING
                | BUBBLE_CAST
                | COMET_CAST
                | JACK_CAST
                | MAELSTROM_CAST
                | CLEARANCE_CAST
                | VARETYR_CAST
                | EARTH_CAST
                | CART_CAST
                | 1872
                | 1876
                | 1902
                | 1910
                | 1918
                | 1923
    )
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

/// How long a cast's aura lasts: the cast time in 16 ms ticks, at least 56 for the blooming ones and 70 for the rest.
fn aura_ticks(effect: u32, cast_ms: u32) -> u32 {
    (cast_ms / 16).max(if effect == BUBBLE_CAST {
        100
    } else if matches!(
        effect,
        12 | 58
            | TETRA_CASTING
            | SHADOW_CAST
            | DARK_CASTING
            | COUPLE_CASTING
            | BLUE_CASTING
            | GREEN_CASTING
            | COMET_CAST
            | MAELSTROM_CAST
            | CLEARANCE_CAST
            | VARETYR_CAST
            | CART_CAST
            | 1872
            | 1876
            | 1902
            | 1910
            | 1918
            | 1923
    ) {
        56
    } else if effect == HOMUN_CASTING {
        60
    } else {
        70
    })
}

/// A band effect's colour at its age (for bloom set `set`): its own, but for Tetra Vortex's cast, which cycles blue, red, green and pale yellow, 25 ticks
/// each, by the aura's age (the first band's tick + 1).
fn aura_colour(effect: u32, set: usize, age: u32) -> [u8; 3] {
    // Maelstrom's second set of cones is a lighter blue.
    if effect == MAELSTROM_CAST && set == 1 {
        return [0x23, 0x92, 0xB9];
    }
    if effect != TETRA_CASTING {
        return aura_look(effect).1;
    }
    [[0x64, 0x64, 0xFF], [0xFF, 0x64, 0x64], [0x64, 0xFF, 0x64], [0xFF, 0xFF, 0xAA]][((age + 1) % 100 / 25) as usize]
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
        58 | TETRA_CASTING => ("ring_white", [0xFF, 0xFF, 0xFF], true),
        HOMUN_CASTING => ("ring_red", [0xFF, 0x59, 0xB6], true),
        SHADOW_CAST | DARK_CASTING => ("ring_black", [0x32, 0x32, 0x32], false),
        BUBBLE_CAST => ("ring_white", [0xFF, 0xFF, 0xFF], true),
        COUPLE_CASTING => ("ring_red", [0xFF, 0x59, 0xB6], true),
        BLUE_CASTING => ("ring_blue", [0x64, 0x64, 0xFF], true),
        GREEN_CASTING => ("ring_white", [0x64, 0xFF, 0x64], true),
        COMET_CAST => ("new_comet\\new_comet_cast_ring\\ring_rainbow", [0x0A, 0x42, 0x72], true),
        MAELSTROM_CAST => ("ring_blue", [0x0A, 0x42, 0x72], true),
        CLEARANCE_CAST => ("ring_white", [0xA4, 0xCD, 0xFE], true),
        VARETYR_CAST => ("ring_yellow", [0x54, 0xA9, 0xCD], true),
        EARTH_CAST => ("ring_yellow", [0xFF; 3], false),
        CART_CAST => ("ring_white", [0xF4, 0xBB, 0x7C], true),
        JACK_CAST => ("ring_ice", [0xC3, 0xC3, 0xFF], false),
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
    if matches!(effect, 367 | BUBBLE_CAST | MAELSTROM_CAST) { 2 } else { 1 }
}

/// The blooming bands' first alphas (two sets of four), their heights and turns.
const BLOOM_ALPHAS: [[i32; 4]; 2] = [[180, 135, 90, 45], [160, 115, 70, 25]];
const BLOOM_HEIGHTS: [f32; 4] = [20.0, 19.0, 18.0, 17.0];
/// Clearance's cones stand 5 lower.
const CLEARANCE_HEIGHTS: [f32; 4] = [15.0, 14.0, 13.0, 12.0];
const BLOOM_TURNS: [f32; 4] = [180.0, 270.0, 0.0, 90.0];

/// How many sets of cones a blooming aura makes: Comet's only the first, the others both.
fn bloom_sets(effect: u32) -> usize {
    if effect == COMET_CAST { 1 } else { 2 }
}

/// Blooming band `band` of set `set` (12 and 58) `age` ticks into an aura of `ticks`: each tick it widens 0.07 and leans
/// 1° less (at 10° it's gone), gaining 10 alpha while under radius 4 and losing 3 from there; gone, it starts again at
/// 3.37 leaning 74° while the aura has more than 30 ticks to go. An aura of 60 ticks or less starts every band at alpha
/// 0, band k 5k ticks late. Clearance's gain 5 alpha a tick (not 10) and start again leaning 64°. None before it starts or
/// while gone.
fn bloom(effect: u32, set: usize, band: usize, ticks: u32, age: u32) -> Option<Band> {
    let (gain, restart) = if effect == CLEARANCE_CAST { (5, 64) } else { (10, 74) };
    let short = ticks <= 60;
    let late = if short { 5 * band as u32 } else { 0 };
    let since = age.checked_sub(late)?;
    // Hundredths of a unit, so the 0.07 steps meet 4.0 exactly.
    let (mut radius, mut lean, mut alpha) = (410, 80, if short { 0 } else { BLOOM_ALPHAS[set][band] });
    for tick in 0..=since {
        alpha = if radius < 400 { alpha + gain } else { (alpha - 3).max(0) };
        radius += 7;
        lean -= 1;
        if lean <= 10 {
            alpha = 0;
        }
        if alpha == 0 && late + tick + 30 < ticks {
            (radius, lean) = (337, restart);
        }
    }
    (alpha > 0).then(|| Band {
        arc: 360.0,
        turn: BLOOM_TURNS[band],
        radius: radius as f32 / 100.0,
        lean: lean as f32,
        height: if effect == CLEARANCE_CAST { CLEARANCE_HEIGHTS[band] } else { BLOOM_HEIGHTS[band] },
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

/// A turning band's top along it (the arcs and the sheet): h·sin 9s°, rising with sin(age°) over the first 90 ticks.
fn turning_height(_band: usize, height: f32, age: u32, s: usize) -> f32 {
    height * (9.0 * s as f32).to_radians().sin() * (age.min(90) as f32).to_radians().sin()
}

/// Earth Grave's sheet's rim at its age n (from 1) as a share of its hump: sin n° over the first 90 ticks, then it breathes
/// between 0.1 and 1.0 with a 360-tick period.
fn earth_sheet_rim(n: u32) -> f32 {
    if n <= 90 { (n as f32).to_radians().sin() } else { 0.55 + 0.45 * ((n % 360) as f32).to_radians().sin() }
}

/// Earth Grave's bands' tops: the crescents' as the other turning auras', the sheet's with its breathing rim.
fn earth_height(band: usize, height: f32, age: u32, s: usize) -> f32 {
    if band < 3 { turning_height(band, height, age, s) } else { height * (9.0 * s as f32).to_radians().sin() * earth_sheet_rim(age + 1) }
}

/// Earth Grave's turning bands, `age` ticks into an aura of `ticks` (its n counts from 1): the other turning auras' arcs and
/// sheet, fading 5 a tick (the sheet 3) over the last 40 ticks, the sheet's alpha starting at 70, gaining 8 a tick to 100.
fn earth_turning(band: usize, ticks: u32, age: u32) -> Band {
    let n = age + 1;
    let fading = (n + 41).saturating_sub(ticks) as f32;
    let shape = turning(band, ticks, age);
    let alpha = if band == 3 { (70.0 + 8.0 * n.min(20) as f32).min(100.0) - 3.0 * fading } else { 180.0 - 5.0 * fading };
    Band { alpha: alpha.max(0.0), ..shape }
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
    matches!(
        effect,
        12 | HOMUN_CASTING
            | 58
            | 367
            | TETRA_CASTING
            | SHADOW_CAST
            | DARK_CASTING
            | BUBBLE_CAST
            | COUPLE_CASTING
            | BLUE_CASTING
            | GREEN_CASTING
            | COMET_CAST
            | MAELSTROM_CAST
            | CLEARANCE_CAST
            | VARETYR_CAST
            | CART_CAST
            | 1872
            | 1876
            | 1902
            | 1910
            | 1918
            | 1923
    )
}

impl Banded {
    /// A blooming aura's bands at a tick, with their set and the height along each.
    fn bloom_bands(&self, age: u32) -> Vec<(usize, Band, Vec<f32>)> {
        (0..bloom_sets(self.effect))
            .flat_map(|set| (0..4).filter_map(move |band| bloom(self.effect, set, band, self.ticks, age).map(|shape| (set, band, shape))))
            .map(|(set, band, shape)| {
                // The bubble cast's cones stand 10 higher.
                let shape = if self.effect == BUBBLE_CAST { Band { height: shape.height + 10.0, ..shape } } else { shape };
                (set, shape, (0..=BAND_QUADS).map(|s| bloom_height(band, shape.height, age, s)).collect())
            })
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
            // ponytail: earth (56) and poison (59) turn like water, fire and wind; Earth Grave's cast (1664) has its own sheet.
            let earth = self.effect == EARTH_CAST;
            let shape_of: fn(usize, u32, u32) -> Band = if earth { earth_turning } else { turning };
            let wave: fn(usize, f32, u32, usize) -> f32 = if earth { earth_height } else { turning_height };
            (0..4).map(|band| shape_of(band, self.ticks, age)).enumerate().map(|(band, shape)| (shape, heights(band, shape.height, wave))).collect()
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
    match effect {
        GUMGANG => 1,
        POWER_UP | WARMTH_GLINTS => 2,
        _ => 4,
    }
}

/// The first alphas of the warmth's glints.
const WARMTH_ALPHAS: [i32; 4] = [40, 80, 120, 160];

/// The squares a glint is drawn as: their half-diagonals and colours. Steel Body's is one; the warmth's is three, a red one and two
/// dark red halos.
fn glint_looks(effect: u32) -> Vec<(f32, [u8; 3])> {
    if effect == WARMTH_GLINTS {
        vec![(2.0, [0xFF, 0x64, 0x64]), (2.4, [0x78, 0x00, 0x00]), (4.0, [0x78, 0x00, 0x00])]
    } else {
        vec![(GLINT_RADIUS, glint_color(effect))]
    }
}

/// The colour of an effect's glints: Steel Body's and Explosion Spirits' blue, Power Up's red.
fn glint_color(effect: u32) -> [u8; 3] {
    if effect == POWER_UP { [0xFF, 0x14, 0x14] } else { GLINT_COLOR }
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

/// A bouncing Stapo (EF_STONERING, 797) on a target's feet.
struct Stapo {
    start: Instant,
    anchor: Anchor,
    action: usize,
    placed: Vec<Placed>,
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
    /// Boost Knuckle's fist by action, frame and whether it is a plain white ghost.
    fist_frames: HashMap<(usize, usize, bool), Vec<Placed>>,
    /// The crowds' monsters by sprite, action and frame.
    crowd_frames: HashMap<(&'static str, usize, usize), Vec<Placed>>,
    /// The skill units whose effect plays on a grid of points, and the points made and waiting to play.
    areas: Vec<Area>,
    area_queue: Vec<(u32, Vector, Instant)>,
    /// The cells' sprites' frames, plain white, each layer with its ACT alpha, by sprite, action and frame.
    cell_frames: HashMap<(&'static str, usize, usize), Layers>,
    /// The sound files a cell effect plays again with each copy.
    cell_sounds: HashMap<u32, Vec<String>>,
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
    stapos: Vec<Stapo>,
    /// The STR world effects running, the time up to which their ticks have run, and the black fill's texture.
    worlds: Worlds,
    world_clock: Option<Instant>,
    black: Option<Texture>,
    /// The world objects that build their pictures in code (`world_fx`); they tick with the STR world copies.
    fx: WorldFx,
}

/// What the world objects load, through the effects' caches.
struct FxAssets<'a> {
    gpu: &'a Gpu,
    grf: &'a Grf,
    textures: &'a mut HashMap<String, Option<Texture>>,
    loaded: &'a mut HashMap<String, Option<Rc<Loaded>>>,
    sprites: &'a mut HashMap<String, Option<SpriteSet>>,
}

impl world_fx::Assets for FxAssets<'_> {
    fn texture(&mut self, path: &str) -> Option<[f32; 2]> {
        let (gpu, grf) = (self.gpu, self.grf);
        self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok()).as_ref().map(|texture| texture.size)
    }

    fn str(&mut self, file: &str) -> Option<Rc<Loaded>> {
        let (gpu, grf) = (self.gpu, self.grf);
        // The textures sit in the file's folder.
        let folder = file.rsplit_once(['/', '\\']).map_or(String::new(), |(directory, _)| format!("{directory}/"));
        self.loaded.entry(format!("{folder}|{file}")).or_insert_with(|| load(gpu, grf, file, &folder)).clone()
    }

    fn sprite(&mut self, path: &str, action: usize, frame: usize) -> Option<(usize, Vec<Placed>)> {
        let (gpu, grf) = (self.gpu, self.grf);
        let sprite = self.sprites.entry(path.to_owned()).or_insert_with(|| SpriteSet::at(grf, path)).as_mut()?;
        let frames = sprite.frame_count(action)?.max(1);
        Some((frames, sprite.frame_at(gpu, action, 0, Timing::Span((frame as f32 + 0.5) / frames as f32))))
    }
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

/// EF_COLDTHROW: Cold Slower's snowflakes thrown at the cell.
pub const EF_COLDTHROW: u32 = 791;
/// EF_STONERING: Sienna Execrate's bouncing Stapo (`data\sprite\몬스터\stapo`).
pub const EF_STONERING: u32 = 797;
/// EF_STASIS: Stasis's electric rings.
pub const EF_STASIS: u32 = 799;
/// EF_FROSTDIVER3: Jack Frost's star of dark blue ice spikes.
pub const EF_FROSTDIVER3: u32 = 801;

/// EF_RECOGNIZED: Recognized Spell's skill icons over the caster.
pub const EF_RECOGNIZED: u32 = 803;
/// EF_TETRA: Tetra Vortex's cards and light columns.
pub const EF_TETRA: u32 = 804;

pub const STAPO_SPRITE: &str = "data\\sprite\\몬스터\\stapo";

/// Stapo's action: 0 if rand == 0, else 7.
pub fn stapo_action(rand: u32) -> usize {
    if rand == 0 { 0 } else { 7 }
}

/// Stapo's frame: ⌊age / 4⌋ + 1 for ages 0..=47 (frames 1..=12), ends at age 48.
pub fn stapo_frame(age: u32) -> Option<usize> {
    if age < 48 { Some((age / 4) as usize + 1) } else { None }
}

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
/// EF_BLEEDING: Wide Bleeding's red wash over the screen (`prims::sheet`, drawn by `cover_quads`), and its STR claws.
const WIDE_BLEEDING: u32 = 669;
/// EF_SUMMONSLAVE: twenty white rays, then a smoke puff (`prims::burst`, the `spr` row).
const SUMMON_SLAVE: u32 = 215;

/// EF_SOULSTRIKE: ghosts from the caster to the target, a hit each (`prims`).
const SOUL_STRIKE: u32 = 15;
/// EF_FIREBALL, EF_YUFITEL (Jupitel Thunder) and EF_GRIMTOOTH (`prims`).
const FIRE_BALL: u32 = 24;
/// The bearing from the target at `to` to the caster at `from`.
fn bearing_between(from: Vector, to: Vector) -> i32 {
    crate::prims::tanji::bearing(from[0] - to[0], from[2] - to[2])
}

/// Which quarter of the circle the camera's longitude plus the bearing from the target to the caster falls in (0 to 3).
fn blast_quarter(longitude: i32, heading: i32) -> usize {
    (longitude + heading).rem_euclid(360) as usize / 90
}

/// Full Buster's action: 0 for a sum of 0 to 89, 3 for 90 to 179, 2 for 180 to 269 and 1 for 270 to 359.
// ponytail: the screen direction of these four diagonals is not checked against the game.
pub fn full_buster_action(longitude: i32, heading: i32) -> usize {
    [0, 3, 2, 1][blast_quarter(longitude, heading)]
}

/// Spread Attack's action for the same sum: 0, 6, 4 and 2.
pub fn spread_attack_action(longitude: i32, heading: i32) -> usize {
    [0, 6, 4, 2][blast_quarter(longitude, heading)]
}

/// Moonlight's slash by the packet's hit count: one (565), two (566) or three or more (567).
pub fn moonlight_effect(hits: u16) -> u32 {
    match hits {
        0 | 1 => 565,
        2 => 566,
        _ => 567,
    }
}

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

/// Rampage Blaster (2332) under /mineffect flashes only the body, 1545, where the chi burst (886) would also open its crescents.
pub fn rampage_effect(skill: u16, effect: u32, min: bool) -> u32 {
    if effect == CHI_BURST && min && skill == 2332 { RAMPAGE_BODY } else { effect }
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
/// EF_SHIELDBOOMERANG2: Tomahawk's axe, thrown as the shield is.
const AXE_THROW: u32 = 494;
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
/// Tiger Cannon's starburst, Crescent Elbow's and Knuckle Arrow's blue lines and Hesperus Lit's disc (`prims::monk`, `prims::sniper`,
/// `prims::brawl`).
const TIGER_CANNON_FX: u32 = 889;
const ELBOW_LINES: u32 = 892;
const KNUCKLE_LINES: u32 = 893;
const GROUND_SHAKE: u32 = 894;
/// Hesperus Lit's pillars, Trample's red disc, Shield Press' shield, Pinpoint Attack's shields and swords, the three buffs' needles
/// and the Genetic casts' green aura (`prims::chi`, `prims::brawl`, `prims::knight`, `prims::guard`).
const BLUE_LINE: u32 = 899;
const GROUND_SHAKE_RED: u32 = 905;
const SHIELD_PRESS_FX: u32 = 906;
const PINPOINT_FX: u32 = 907;
const PRESTIGE_FX: u32 = 908;
const INSPIRATION_FX: u32 = 910;
const GREEN_CASTING: u32 = 911;
/// The casts of Comet, Jack Frost, Maelstrom, Clearance and Varetyr Spear: blooming cones in other colours, and Jack Frost's
/// turning crescents.
const COMET_CAST: u32 = 1418;
const JACK_CAST: u32 = 1449;
const MAELSTROM_CAST: u32 = 1556;
const CLEARANCE_CAST: u32 = 1560;
const VARETYR_CAST: u32 = 1565;
/// Epiclesis' ground rings, Soul Ascetic's bubble cones, Earth Grave's crescents and sheet and Cart Boost's cones (`prims::holes`,
/// `prims::songs`).
const EPICLESIS_CELL: u32 = 1655;
const SOUL_CAST: u32 = 1656;
const EARTH_CAST: u32 = 1664;
const CART_CAST: u32 = 1677;
/// Exceed Break's needles, the Dragon Combo's rays, the Sura's chi burst and Moon Slasher's spin (`prims::chi`, `prims::brawl`).
const PRIME_CHARGE: u32 = 882;
const DRAGON_RAYS: u32 = 885;
const CHI_BURST: u32 = 886;
const MOON_SPIN: u32 = 890;
/// Severe Rainstorm's cast ribbons (`prims::songs`).
const RAINSTORM_CAST: u32 = 1286;
/// Cannon Spear (2307): its gold vortex, and its blades by the screen direction the caster faces (`cannon_sector`).
pub const CANNON_SPEAR: u16 = 2307;
const CANNON_VORTEX: u32 = 1408;
/// Dragon Breath, its monster skill and Water Dragon Breath: a breath on the caster by its screen quadrant, a burst on each
/// target (`breath_launches`).
pub const DRAGON_BREATHS: [u16; 3] = [2008, 731, 5004];
const WATER_BREATH: u16 = 5004;
const BREATH_SOUND: &str = "effect\\폭염룡.wav";
/// Cart Tornado's spin of its caster (`prims::brawl`).
const CART_TORNADO: u32 = 1365;
/// Cannon Spear's pale blue lines (`prims::brawl`).
const CANNON_LINES: u32 = 1409;
/// Sound of Destruction's red music notes (`prims::guard`).
const SOUND_NOTES: u32 = 869;
/// Dead Hill Here's Geographers, Poem of Netherworld's note, Randomize Spell's frames and Dance with Warg's Wargs (`prims::crowds`).
const DEAD_HILL: u32 = 853;
const MELODY_NOTE: u32 = 860;
const RANDOMIZE: u32 = 862;
const WARG_DANCE: u32 = 866;
/// Hundred Spear's gold lines, Swing Dance's pink flicker, Harmonize's emitter and Great Echo's "V" (`prims::brawl`,
/// `prims::songs`, `prims::spears`).
const SPEAR_RUSH: u32 = 835;
/// Rampage Blaster's blue body flash under /mineffect, Cross Impact's orange lines and Reverberation's note and rings (`prims::brawl`,
/// `prims::crowds`, `prims::holes`).
const RAMPAGE_BODY: u32 = 1545;
const CROSS_LINES: u32 = 1568;
const REVERB_FX: u32 = 1569;
const SWING_DANCE: u32 = 847;
const HARMONIZE: u32 = 849;
/// Arrullo's drifting lights (`prims::souls`); Harmonize's (850) are launched by its emitter.
const SOUL_LIGHT_PINK: u32 = 932;
/// Wall of Thorns' tree, the Ifrits of Demonic Fire and the Insignias, Vacuum Extreme's whirlwinds and Poison Buster's blasts
/// (`prims::cells`).
const WALL_OF_THORNS: u32 = 912;
const DEMONIC_FIRE: u32 = 916;
const DEMONIC_FIRE_3: u32 = 918;
const VACUUM_EXTREME: u32 = 921;
const POISON_BUSTER_FX: u32 = 923;
/// Warmer's Imps, Arrullo's spirals and Elemental Action's crescents (`prims::cells`, `prims::spears`, `prims::walls`).
const WARMER: u32 = 929;
const ARRULLO_FX: u32 = 931;
const ELEMENTAL_ACTION: u32 = 934;
/// The monsters' Grand Darkness, Power Up and Anti-Magic (`prims::walls`, `glint_sets`, `prims::guard`).
const GRAND_DARKNESS: u32 = 450;
const POWER_UP: u32 = 455;
/// Star Gladiator's warmth: eight small red glints with dark halos (`PrimKind::Glints`).
const WARMTH_GLINTS: u32 = 418;
const POWER_UP_TICKS: u32 = 1500;
const ANTI_MAGIC: u32 = 675;
/// EF_HITDARK's near-black sparks and Lope's motes and petals (`HitSpark`, `prims::petals`).
pub const HIT_DARK: u32 = 180;
const LOPE: u32 = 411;
/// The kicks' and Swoo's body changes (`prims::body`).
const SOUL_BODY: u32 = 424;
const SOUL_BODY_MONSTER: u32 = 427;
const LAND_BODY: u32 = 446;
const PRESSED_BODY: u32 = 413;
const KICKED_BODY: u32 = 415;
const HIT_BODY: u32 = 417;
const BABY_BODY: u32 = 420;
/// The Hit Lines, Running's start and stop (`prims::kicks`).
const HIT_LINE_TURN: u32 = 429;
const HIT_LINE_COUNTER: u32 = 430;
const HIT_LINE_DOWN: u32 = 431;
const RUN_START: u32 = 442;
const RUN_STATUS: u32 = 443;
const RUN_STOP: u32 = 444;
/// Running's status (133) and how long a footprint lives.
const RUN_STATUS_EFST: u16 = 133;
/// The Soul Reaper's dark cast, AGI up, Seven Wind and the blue and red body glows (`aura_look`, `prims::walls`, `prims::brawl`,
/// `prims::body`).
const DARK_CASTING: u32 = 454;
const AGI_UP: u32 = 456;
const SEVEN_WIND: u32 = 461;
/// Circle of Fire's dome, Hurricane Rage's and Zephyr's funnels, the Filir's pink bloom and Moonlight's slashes (by hit count).
const CIRCLE_GUARD: u32 = 336;
const HURRICANE_RAGE: u32 = 460;
const ZEPHYR: u32 = 465;
pub const HOMUN_CASTING: u32 = 564;
pub const MOONLIGHT_SKILL: u16 = 8009;
const BLUE_BODY: u32 = 542;
const RED_LIGHT_BODY: u32 = 544;
/// Comfort's cones, Soul Link's mote and letters and Stin's blade (`prims::soul`).
const COMFORT: u32 = 488;
const SOUL_LINK_FX: u32 = 503;
const STIN: u32 = 547;
/// Estin's and Estun's blue flash (`prims::brawl`), Esma's discs and frost and Estun's mover (`prims::linker`).
const BLUE_HIT: u32 = 549;
/// The two spark movers (551, 1204), which leave a trail of open cone rings (552, 1205), and the sparks (556) Estun's leaves.
const SPARK_MOVER: u32 = 551;
const SPARK_MOVER_DARK: u32 = 1204;
const ESMA_DISCS: u32 = 553;
const ESMA_FROST: u32 = 554;
const ESTUN_MOVER: u32 = 555;
/// The warmth's sparkles, Full Buster's blast, Kaizel's fins and Kouenka's petals and rays.
const WARM_SPARKLES: u32 = 572;
const FULL_BUSTER: u32 = 584;
/// Spread Attack's blast, aimed at the target; Granitic Armor's, Kyomu's and Gensou's grey tint.
const SPREAD_ATTACK: u32 = 645;
const GREY_BODY: u32 = 963;
const KAIZEL_FINS: u32 = 590;
/// Eswhoo's dark discs, Kaute's pale and dark sparks and Soul Unity's tube.
const ESWHOO_DISCS: u32 = 1206;
const KAUTE_PALE: u32 = 1207;
const KAUTE_DARK: u32 = 1208;
const SOUL_UNITY: u32 = 1210;
const KOUENKA: u32 = 618;
/// Lullaby Deep Sleep's wandering notes (`prims::notes`).
const LULLABY: u32 = 969;
/// Resurrection's rings and the Call Partner portal's warp (`prims::portal`).
pub const REVIVE_FX: u32 = 140;
const PORTAL_FX: u32 = 341;
/// I'm All Yours' heart of flame flowers and Baby's tumbling ribbons (`prims::sphere`).
const HEART_CAST: u32 = 343;
const BABY_FX: u32 = 408;
/// King's Grace's emblems, Offertorium's ring and Escape's tint and copies (`prims::emblems`, `actor`).
const KINGS_GRACE: u32 = 1051;
const OFFERTORIUM_RING: u32 = 1057;
pub const ESCAPE_FX: u32 = 1081;
const GREAT_ECHO_V: u32 = 863;
/// Sonic Wave's crescents and Duple Light's orbs (`prims::waves`).
const SONIC_WAVE: u32 = 832;
const DUPLE_LIGHT: u32 = 833;
/// Shadow Form's cast aura, the ground cells of Chaos Panic, Maelstrom and Bloody Lust, the bubble casts of the Arch Bishop and the
/// Sura (`prims::holes`, `prims::songs`).
const SHADOW_CAST: u32 = 826;
const CHAOS_CELL: u32 = 827;
const MAELSTROM_CELL: u32 = 828;
const BLOODY_CELL: u32 = 829;
const BUBBLE_CAST: u32 = 830;
const SURA_CAST: u32 = 831;
/// Shadow Form's tube, Strip Accessory's icons, Manhole's and Dimension Door's cells and Manhole's body (`prims::transcend`,
/// `prims::guard`, `prims::holes`, `prims::shafts`, `prims::brawl`).
const SHADOW_LINK: u32 = 819;
const STRIP_ACCESSORY: u32 = 820;
const MANHOLE_CELL: u32 = 822;
pub const MANHOLE_BODY: u32 = 823;
const DIMENSION_CELL: u32 = 825;
/// Tetra Vortex's cast, Boost Knuckle's fists, Shadow Form's silhouette, Body Painting's strokes, Deadly Infect's blink and the
/// Masquerade debuffs' hairlines.
const TETRA_CASTING: u32 = 805;
const BOOST_KNUCKLE: u32 = 806;
const SHADOW_FORM: u32 = 809;
const BODY_PAINT: u32 = 811;
const DEADLY_INFECT: u32 = 812;
/// Magnetic Field's arcs, Neutral Barrier's dome and cells, Stealth Field's cells, Repair's spanners, Analyze's mark and Venom
/// Impress' icon (`prims::mechanic`, `prims::guard`, `prims::sparkle`, `prims::circle`).
const MAGNETIC_FIELD: u32 = 781;
/// Infrared Scan's two radar rays (`prims::radar`).
const INFRARED_SCAN: u32 = 794;
const NEUTRAL_BARRIER: u32 = 782;
const BARRIER_CELL: u32 = 783;
const STEALTH_CELL: u32 = 784;
const REPAIR: u32 = 785;
const ANALYZE: u32 = 786;
const VENOM_ICON: u32 = 788;
/// The status Venom Impress gives, which its icon stands for.
const VENOM_STATUS: u16 = 328;
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
/// Piercing Shot's lines, Kamaitachi's wheels (`prims::wheels`) and Disarm's guns.
const PIERCING_LINES: u32 = 623;
const KAMAITACHI: u32 = 624;
const DISARM: u32 = 627;
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
/// Prominence Kick's spin (the same swing), and Misty Frost's blue glow.
const PROMINENCE_SPIN: u32 = 1224;
const MISTY_FROST: u32 = 1420;
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
const DUST: u32 = 628;
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
pub const AIMED: [u32; 52] = [
    EF_TETRA,
    CANNON_LINES,
    STIN,
    CROSS_LINES,
    ESWHOO_DISCS,
    SOUL_UNITY,
    PIERCING_LINES,
    KAMAITACHI,
    ESMA_DISCS,
    ESTUN_MOVER,
    FULL_BUSTER,
    SPREAD_ATTACK,
    HIT_LINE_COUNTER,
    PINPOINT_FX,
    ELBOW_LINES,
    KNUCKLE_LINES,
    SPEAR_RUSH,
    GREAT_ECHO_V,
    SONIC_WAVE,
    SHADOW_LINK,
    BOOST_KNUCKLE,
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
    AXE_THROW,
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
/// Spear of Ice's spikes (`prims::stone`), and the blinks of Madness Canceller (blue) and Adjustment and Gatling Fever (red).
const HYOUSENSOU: u32 = 619;
const MADNESS_BLUE: u32 = 625;
const MADNESS_RED: u32 = 626;
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
    /// Starts the STR world effect `id` on `owner` (`Some(None)` = the player; None = the cell `point`), the launchers' entry.
    /// A `single` row or a `unique` launch returns the copy already running for the owner; `life` > 0 (ticks) makes a timed,
    /// looping copy. Returns its handle, for `world_copy`.
    #[allow(clippy::too_many_arguments)]
    pub fn launch_world(
        &mut self,
        gpu: &Gpu,
        grf: &Grf,
        table: &Table,
        id: u32,
        owner: Option<Option<u32>>,
        point: Vector,
        unique: bool,
        life: u32,
    ) -> Option<u32> {
        let row = table.worlds.get(id)?;
        if row.tint > 0 && self.black.is_none() {
            self.black = Some(gpu.solid([0, 0, 0, 255]));
        }
        let launch = world_str::Launch { id, owner, point, unique, life, min: self.min, roll: pick(0, 2) };
        let loaded = &mut self.loaded;
        self.worlds.launch(row, &launch, |file| {
            // The textures sit in the file's folder.
            let folder = file.rsplit_once('/').map_or(String::new(), |(directory, _)| format!("{directory}/"));
            loaded.entry(format!("{folder}|{file}")).or_insert_with(|| load(gpu, grf, file, &folder)).clone()
        })
    }

    /// The launchers' controls on a copy `launch_world` returned: start delay, screen turn, cut, repeats, scale, alpha,
    /// offset, mirror, hold, fast-forward and the far distance.
    #[allow(dead_code)]
    pub fn world_copy(&mut self, serial: u32) -> Option<&mut world_str::World> {
        self.worlds.get_mut(serial)
    }

    /// Removes the STR world copy of an owner's id.
    #[allow(dead_code)]
    pub fn remove_world(&mut self, owner: Option<Option<u32>>, id: u32) {
        self.worlds.remove(owner, id);
    }

    /// Removes all of an owner's STR world copies.
    #[allow(dead_code)]
    pub fn remove_worlds_of(&mut self, owner: Option<u32>) {
        self.worlds.remove_owner(owner);
    }

    /// The owners (None = the player) whose body the head rule of a running copy needs.
    pub fn world_head_owners(&self) -> Vec<Option<u32>> {
        self.worlds.head_owners()
    }

    fn fx_assets<'a>(&'a mut self, gpu: &'a Gpu, grf: &'a Grf) -> (&'a mut WorldFx, FxAssets<'a>) {
        (&mut self.fx, FxAssets { gpu, grf, textures: &mut self.textures, loaded: &mut self.loaded, sprites: &mut self.sprites })
    }

    /// Runs the STR world copies' and the world objects' 16 ms ticks that came due, before the units, and queues the sounds
    /// and shakes they fire; then the drawn objects' frame step.
    pub fn advance_world(&mut self, gpu: &Gpu, grf: &Grf, now: Instant, scene: &world_str::Scene, seen: &world_fx::Seen) {
        let clock = self.world_clock.get_or_insert(now);
        let due = world_ticks(now.saturating_duration_since(*clock));
        // A long stall (a loading screen) skips ahead rather than running every tick.
        *clock = if due > WORLD_TICKS_MAX { now } else { *clock + WORLD_TICK * due };
        if self.worlds.is_empty() && self.fx.is_empty() {
            return;
        }
        for _ in 0..due.min(WORLD_TICKS_MAX) {
            let mut fired = self.worlds.tick(scene);
            let (fx, mut assets) = self.fx_assets(gpu, grf);
            fired.extend(fx.tick(scene, &mut assets));
            for fired in fired {
                match fired {
                    world_str::Fired::Sound(file, at) => self.sounds.push((now, file, at)),
                    world_str::Fired::Quake => self.quakes.push((now, QUAKE_LONG)),
                }
            }
        }
        let (fx, mut assets) = self.fx_assets(gpu, grf);
        fx.frame(seen, &mut assets);
    }

    /// Starts world object `id` (`Some(None)` owner = the player, None = at the point), or returns the copy running.
    pub fn launch_fx(&mut self, gpu: &Gpu, grf: &Grf, launch: &world_fx::Launch) -> Option<u32> {
        let (fx, mut assets) = self.fx_assets(gpu, grf);
        fx.launch(launch, &mut assets)
    }

    /// A unit's hat effects (`None` = the player) turning on or off: the footprint hats start or end their world objects.
    pub fn hat_effects(&mut self, gpu: &Gpu, grf: &Grf, owner: Option<u32>, on: bool, hats: &[u16]) {
        let (fx, mut assets) = self.fx_assets(gpu, grf);
        world_fx::footprints::hats(fx, Some(owner), on, hats, &mut assets);
    }

    /// A heal object (an effect packet's or a script's 253, 312, 313, 320, 325, 1143, 1144) on `unit` (None = the player): a
    /// new copy each time, with the table's sounds for the id.
    #[allow(clippy::too_many_arguments)]
    pub fn heal_object(&mut self, gpu: &Gpu, grf: &Grf, table: &Table, unit: Option<u32>, id: u32, at: Vector, now: Instant) {
        self.play_sounds(table, id, at, now);
        self.launch_fx(gpu, grf, &world_fx::Launch::at(id, Some(unit), at));
    }

    /// A fall packet's drop effects for ground item `item` lying at `at`: the item info's effect number and the mode's colour
    /// (`world_fx::drops::launches`). A pillar stands while the item lies; any other effect plays once at the item.
    #[allow(clippy::too_many_arguments)]
    pub fn item_dropped(&mut self, gpu: &Gpu, grf: &Grf, table: &Table, item: u32, info: Option<u32>, (show, mode): (bool, u16), at: Vector, now: Instant) {
        let drops = world_fx::drops::launches(info, show, mode);
        // A fall packet for an item already standing in a pillar replaces it.
        if !drops.is_empty() {
            self.item_gone(item);
        }
        for drop in drops {
            match drop {
                world_fx::drops::Drop::Pillar(effect) => {
                    let (fx, mut assets) = self.fx_assets(gpu, grf);
                    world_fx::drops::stand(fx, effect, item, at, &mut assets);
                }
                world_fx::drops::Drop::Effect(effect) => self.play(gpu, grf, table, effect, Target::Ground, at, now),
            }
        }
    }

    /// A ground item taken or gone: its pillars go.
    pub fn item_gone(&mut self, item: u32) {
        self.fx.remove_all_of(Some(Some(item)));
    }

    /// The world objects' faces by draw list; `depth_of` sorts the translucent and adding lists.
    pub fn fx_lists(&self, seen: &world_fx::Seen, depth_of: impl Fn(Vector) -> f32) -> world_fx::Lists<EffectMesh<'_>> {
        world_fx::Lists::sort(self.fx.faces(seen), depth_of).map(|face| {
            let texture = match face.texture {
                world_fx::Tex::Path(path) => self.textures.get(path)?.as_ref()?,
                world_fx::Tex::Str(loaded, layer, index) => loaded.textures.get(layer)?.get(index)?.as_ref()?,
                world_fx::Tex::Sprite(path, key) => self.sprites.get(path)?.as_ref()?.texture_of(key)?,
            };
            Some((texture, face.vertices, face.additive))
        })
    }

    /// Cannon Spear's packet on `caster` (None = the player) standing at `at`: the vortex at once and the blade after the
    /// caster's swing (`cannon_launches`).
    #[allow(clippy::too_many_arguments)]
    pub fn cannon_spear(&mut self, gpu: &Gpu, grf: &Grf, table: &Table, caster: Option<u32>, at: Vector, phi: i32, delay_ms: i32) {
        let owner = Some(caster);
        for (id, delay) in cannon_launches(table, self.min, phi, delay_ms, |id| self.worlds.playing(owner, id)) {
            if let Some(copy) = self.launch_world(gpu, grf, table, id, owner, at, false, 0).and_then(|serial| self.worlds.get_mut(serial)) {
                copy.timeline.delay = delay;
            }
        }
    }

    /// A Dragon Breath packet of `skill` from `caster` (None = the player) at `caster_at` on `target` at `target_at`, the
    /// caster facing φ on the screen (`breath_launches`); its sound once a packet at the listener.
    #[allow(clippy::too_many_arguments)]
    pub fn dragon_breath(
        &mut self,
        gpu: &Gpu,
        grf: &Grf,
        table: &Table,
        skill: u16,
        (caster, caster_at): (Option<u32>, Vector),
        (target, target_at): (Option<u32>, Option<Vector>),
        phi: i32,
        now: Instant,
    ) {
        self.queue_breath_sound(now);
        for (id, who, delay) in breath_launches(table, self.min, skill, Some(phi), |id| self.worlds.playing(Some(caster), id)) {
            let (owner, at) = match who {
                Who::Caster => (caster, caster_at),
                _ => match target_at {
                    Some(at) => (target, at),
                    None => continue,
                },
            };
            if let Some(copy) = self.launch_world(gpu, grf, table, id, Some(owner), at, false, 0).and_then(|serial| self.worlds.get_mut(serial)) {
                copy.timeline.delay = delay;
            }
        }
    }

    /// Dragon Breath's sound, at once and at the listener.
    fn queue_breath_sound(&mut self, now: Instant) {
        self.sounds.push((now, BREATH_SOUND.to_owned(), LISTENER));
    }

    /// Sounds effect `id` makes, with none of its looks (Manhole's on a monster).
    pub fn sound_of(&mut self, table: &Table, id: u32, at: Vector, now: Instant) {
        self.play_sounds(table, id, at, now);
    }

    /// Plays effect `id` from `now` on `target` at `at`; parts that don't follow units stay there.
    pub fn play(&mut self, gpu: &Gpu, grf: &Grf, table: &Table, id: u32, target: Target, at: Vector, now: Instant) {
        let id = table.min_id(id, self.min);
        // An STR world effect that names its file (the hat rows and the launcher-given ones play from their launchers): its own
        // sounds and shakes come with it.
        if table.world(id).is_some_and(|row| matches!(row.file, world_str::Source::Named(_))) {
            let (owner, status) = match target {
                Target::Unit(unit) => (Some(unit), None),
                Target::Status(unit, status) => (Some(unit), Some(status)),
                _ => (None, None),
            };
            // A status's copy is one per unit (a status sent again keeps it) and goes when the status goes off.
            let serial = self.launch_world(gpu, grf, table, id, owner, at, status.is_some(), 0);
            if let Some(copy) = serial.zip(status).and_then(|(serial, _)| self.worlds.get_mut(serial)) {
                copy.status = copy.status.or(status);
            }
            return;
        }
        // A registered world object; the footprint classes come only with a hat effect, the heal objects only with a packet,
        // the pillars with a dropped item.
        if self.fx.registered(id) && !world_fx::footprints::is_class(id) && !world_fx::heal::is_class(id) && !world_fx::drops::is_pillar(id) {
            let owner = match target {
                Target::Unit(unit) | Target::Status(unit, _) => Some(unit),
                _ => None,
            };
            self.launch_fx(gpu, grf, &world_fx::Launch::at(id, owner, at));
            return;
        }
        if id == BASH && self.min {
            return;
        }
        // Quagmire sounds one time in 12, from 100 units under the ground (so quietly); each of its cells launches its own.
        if id == QUAGMIRE {
            if quagmire_heard(pick(0, 11)) {
                self.play_sounds(table, id, [at[0], at[1] - 100.0, at[2]], now);
            }
        } else if id == GREEN_CASTING {
            // The Genetic casts sound 100 up, so quietly.
            self.play_sounds(table, id, [at[0], at[1] + 100.0, at[2]], now);
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
        // I'm All Yours' cast: twenty flame flowers on a heart round the caster's feet for the cast.
        if id == HEART_CAST {
            self.textures.entry(sphere::RING_RED.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, sphere::RING_RED, true).ok());
            self.play_sounds(table, id, at, now);
            let ticks = cast_ms / crate::prims::TICK.as_millis() as u32;
            self.prims.sphere.heart(unit, at, ticks, now, |n| pick(0, n.saturating_sub(1)));
            return;
        }
        // Exceed Break's cast: 160 needles converge on the caster for the cast (500 ticks when there is no cast time).
        if id == PRIME_CHARGE {
            self.textures.entry(chi::NEEDLE.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, chi::NEEDLE, true).ok());
            self.play_sounds(table, id, at, now);
            let ticks = cast_ms / crate::prims::TICK.as_millis() as u32;
            self.prims.chi.charge(unit, if ticks == 0 { 500 } else { ticks }, now, |n| pick(0, n.saturating_sub(1)));
            return;
        }
        // Shadow Form's cast aura shows with no sound; the sound comes with the skill packet.
        if id == SHADOW_CAST {
            return;
        }
        // Severe Rainstorm's cast: two rings of music-note ribbons round the caster for the cast.
        if id == RAINSTORM_CAST {
            for path in songs::MUSIC {
                self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok());
            }
            self.play_sounds(table, id, at, now);
            let ticks = cast_ms / crate::prims::TICK.as_millis() as u32;
            self.prims.songs.rain(unit, ticks, self.min, now);
            return;
        }
        // The Sura casts: bubble cones over two blue squares.
        if id == SURA_CAST {
            let ticks = (cast_ms / crate::prims::TICK.as_millis() as u32).max(120);
            for path in songs::BUBBLES.into_iter().chain([holes::SURA_CIRCLE]) {
                self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok());
            }
            self.play_sounds(table, id, at, now);
            self.prims.songs.cone(SURA_CAST, unit, ticks, now);
            self.prims.holes.squares(unit, ticks, now);
            return;
        }
        // Soul Ascetic's and Reverberation's casts: violet bubble cones round the caster for the whole cast.
        // ponytail: the sound plays at full volume, not 40 %.
        if id == SOUL_CAST {
            let ticks = cast_ms / crate::prims::TICK.as_millis() as u32;
            for path in songs::BUBBLES {
                self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok());
            }
            self.play_sounds(table, id, at, now);
            self.prims.songs.soul_cone(unit, ticks, self.min, now);
            return;
        }
        // Tetra Vortex's cast also flickers the caster in a fresh colour each tick.
        if id == TETRA_CASTING {
            self.prims.brawl.tetra(unit, aura_ticks(id, cast_ms), now);
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
        if let (BASH | TURN_UNDEAD | SONIC_BLOW | OVER_THRUST | SUMMON_SLAVE, Target::Unit(unit)) = (id, target) {
            for path in [burst::DISC, burst::RAY, burst::RING] {
                self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok());
            }
            let rand = |n: u32| pick(0, n.saturating_sub(1));
            let burst = match id {
                BASH => burst::Burst::bash(unit, now, burst::LIFE, rand),
                SONIC_BLOW => burst::Burst::sonic_blow(unit, now),
                SUMMON_SLAVE => burst::Burst::summon_slave(unit, now, table.lengths.get(&id).copied().unwrap_or(500), rand),
                // A recast within its 40 ticks takes the place of the ring.
                OVER_THRUST => {
                    self.prims.bursts.retain(|burst| !(burst.over_thrust && burst.unit == unit));
                    burst::Burst::over_thrust(unit, now)
                }
                _ => burst::Burst::turn_undead(unit, now, rand),
            };
            self.prims.bursts.push(burst);
        }
        // Wide Bleeding's red sheet over the view, made with its STR; it ignores /mineffect.
        if let (WIDE_BLEEDING, Target::Unit(unit)) = (id, target) {
            self.textures.entry(sheet::TEXTURE.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, sheet::TEXTURE, false).ok());
            self.prims.sheets.start(unit, now);
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
            (SHADOW_FORM, Target::Status(unit, _)) => self.prims.brawl.shadow(unit, now),
            (BODY_PAINT, Target::Unit(unit)) => {
                self.textures.entry(painter::STICK.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, painter::STICK, true).ok());
                self.prims.painter.paint(unit, at, now);
            }
            (DEADLY_INFECT, Target::Unit(unit)) => self.prims.guard.blink(unit, now),
            (DARK_CASTING, Target::Unit(unit)) => {
                // A start other than a cast shows the aura for 60 ticks.
                self.play_aura(gpu, grf, unit, DARK_CASTING, 960, now);
            }
            (AGI_UP, Target::Unit(unit) | Target::Status(unit, _)) => {
                self.textures.entry(walls::STREAK.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, walls::STREAK, true).ok());
                self.prims.walls.agility(unit, now, |n| pick(0, n.saturating_sub(1)));
            }
            (CIRCLE_GUARD, Target::Unit(unit)) => {
                for path in [guard::GUARD_K, guard::GUARD_K2] {
                    self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok());
                }
                let facing = self.facings.get(&unit).copied().unwrap_or(0.0);
                self.prims.guard.circle_of_fire(unit, facing, now);
            }
            (HURRICANE_RAGE, Target::Unit(unit)) => {
                for path in [brawl::STORM, brawl::CLOUD] {
                    self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok());
                }
                self.prims.brawl.hurricane(unit, at, now, |n| pick(0, n.saturating_sub(1)));
            }
            (ZEPHYR, Target::Lasting(id)) => {
                for path in [brawl::STORM, brawl::CLOUD] {
                    self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok());
                }
                self.prims.brawl.zephyr(id, at, now, |n| pick(0, n.saturating_sub(1)));
            }
            (SEVEN_WIND, Target::Unit(unit)) => {
                for path in [brawl::STORM, brawl::CLOUD] {
                    self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok());
                }
                self.prims.brawl.seven_wind(unit, at, now, |n| pick(0, n.saturating_sub(1)));
            }
            (MADNESS_BLUE, Target::Unit(unit)) => self.prims.bodies.start(body::Change::BlueBlink, unit, now),
            (MADNESS_RED, Target::Unit(unit)) => self.prims.bodies.start(body::Change::RedBlink, unit, now),
            (BLUE_BODY, Target::Unit(unit)) => self.prims.bodies.start(body::Change::BlueGlow, unit, now),
            (RED_LIGHT_BODY, Target::Unit(unit) | Target::Status(unit, _)) => self.prims.bodies.start(body::Change::RedGlow, unit, now),
            (SHADOW_CAST, Target::Unit(unit)) => {
                // The skill packet's aura: 60 ticks, with the sound at the listener.
                self.play_aura(gpu, grf, unit, SHADOW_CAST, 960, now);
                self.sounds.push((now, "effect\\shadowform.wav".to_owned(), LISTENER));
            }
            (DUPLE_LIGHT, Target::Status(unit, _)) | (DUPLE_LIGHT, Target::Unit(unit))
                if !matches!(target, Target::Unit(_)) || !self.prims.waves.has_orbs(unit) =>
            {
                for path in [waves::GLOW, waves::PANE] {
                    self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok());
                }
                self.prims.waves.duple_light(unit, now, |n| pick(0, n.saturating_sub(1)));
            }
            (SWING_DANCE, Target::Unit(unit)) => {
                self.textures.entry(songs::STREAK_TEXTURE.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, songs::STREAK_TEXTURE, true).ok());
                self.prims.songs.swing_dance(unit, now, |n| pick(0, n.saturating_sub(1)));
            }
            (HARMONIZE, Target::Unit(unit)) => {
                self.textures.entry(souls::DOT.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, souls::DOT, true).ok());
                self.prims.spears.emitter(unit, now);
            }
            // Arrullo's lights come from its own emitter; one made by hand starts at 25 up.
            (SOUL_LIGHT_PINK, Target::Unit(unit)) => {
                self.textures.entry(souls::DOT.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, souls::DOT, true).ok());
                self.prims.souls.launch(true, unit, [0.0, 25.0, 0.0], now, |n| pick(0, n.saturating_sub(1)));
            }
            (DRAGON_RAYS, Target::Unit(unit)) => {
                self.textures.entry(brawl::RAY.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, brawl::RAY, true).ok());
                self.prims.brawl.dragon_rays(unit, now, |n| pick(0, n.saturating_sub(1)));
            }
            (CHI_BURST, Target::Unit(unit)) => {
                self.textures.entry(walls::RING_BLUE.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, walls::RING_BLUE, true).ok());
                self.prims.walls.chi_burst(unit, now);
                self.prims.guard.chi_burst(unit, now);
                self.prims.brawl.chi_spread(unit, now);
            }
            (RAMPAGE_BODY, Target::Unit(unit)) => {
                self.prims.guard.chi_burst(unit, now);
                self.prims.brawl.chi_spread(unit, now);
            }
            (REVERB_FX, Target::Unit(unit)) => {
                let note = if self.min { crowds::REVERB_MELODY_MIN } else { crowds::REVERB_MELODY };
                self.textures.entry(note.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, note, false).ok());
                self.textures.entry(walls::RING_WHITE.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, walls::RING_WHITE, true).ok());
                self.prims.crowds.reverb_note(unit, self.min, now, |n| pick(0, n.saturating_sub(1)));
                self.prims.holes.reverb(at, now);
            }
            (MOON_SPIN, Target::Unit(unit)) => self.prims.brawl.moon_spin(unit, now),
            (CART_TORNADO, Target::Unit(unit)) => self.prims.brawl.caster_spin(unit, now),
            (TIGER_CANNON_FX, _) => {
                self.textures.entry(sniper::RAY.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, sniper::RAY, true).ok());
                self.prims.monk.tiger_cannon(at, now, self.min, |n| pick(0, n.saturating_sub(1)));
            }
            (BLUE_LINE, _) => {
                self.textures.entry(chi::RING_BLUE.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, chi::RING_BLUE, true).ok());
                self.prims.chi.pillars(at, now, |n| pick(0, n.saturating_sub(1)));
            }
            (GROUND_SHAKE_RED, Target::Unit(unit)) => {
                self.textures.entry(brawl::BLAST.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, brawl::BLAST, true).ok());
                self.prims.brawl.disc(true, unit, now);
            }
            (SHIELD_PRESS_FX, Target::Unit(unit)) => {
                for (path, keyed) in [(knight::SHIELD, false), (knight::CROSS_FLASH, true)] {
                    self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, keyed).ok());
                }
                self.prims.knight.pressure(true, unit, now, |n| pick(0, n.saturating_sub(1)));
            }
            (PRESTIGE_FX..=INSPIRATION_FX, Target::Unit(unit) | Target::Status(unit, _)) => {
                self.textures.entry(chi::NEEDLE.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, chi::NEEDLE, true).ok());
                self.prims.chi.buff(id, unit, 500, now, |n| pick(0, n.saturating_sub(1)));
            }
            (SIGHT, Target::Unit(unit)) => {
                for path in [sight::SIGHT, sight::SHADOW] {
                    self.sprites.entry(path.to_owned()).or_insert_with(|| SpriteSet::at(grf, path));
                }
                self.prims.cells.sight(unit, now);
            }
            (GRAND_DARKNESS, _) => {
                self.textures.entry(walls::RING_BLACK.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, walls::RING_BLACK, true).ok());
                self.prims.walls.grand_darkness(at, now);
            }
            (ANTI_MAGIC, Target::Unit(unit)) => {
                for path in [guard::GUARD_K, guard::GUARD_K2] {
                    self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok());
                }
                let facing = self.facings.get(&unit).copied().unwrap_or(0.0);
                self.prims.guard.anti_magic(unit, facing, now);
            }
            (172..=179, Target::Unit(_) | Target::Ground) => {
                let path = cells::sheet_texture(id);
                self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok());
                self.prims.cells.sheet(id, at, now);
            }
            (HIT_LINE_TURN | HIT_LINE_DOWN, Target::Unit(unit)) => {
                self.textures.entry(kicks::LINE.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, kicks::LINE, true).ok());
                let kind = if id == HIT_LINE_TURN { kicks::Kind::Turn } else { kicks::Kind::Down };
                self.prims.kicks.hit(kind, unit, None, at, now);
            }
            (RUN_START, Target::Unit(unit)) => {
                // Running's start leaves footprints; once its status is on, the use is its stop: eight fins shoot out of the feet.
                for path in [kicks::FOOT_LEFT, kicks::FOOT_RIGHT] {
                    self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, false).ok());
                }
                if self.prims.kicks.is_running(unit) {
                    self.textures.entry(kicks::FIN.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, kicks::FIN, false).ok());
                    self.play_sounds(table, RUN_STOP, at, now);
                    self.prims.kicks.stop(unit, now);
                } else {
                    self.prims.kicks.run(unit, now);
                }
            }
            (RUN_STATUS, Target::Status(unit, _)) => self.prims.kicks.status(unit, true),
            (COMFORT, Target::Unit(unit)) => {
                self.textures.entry(soul::RING_BLUE.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, soul::RING_BLUE, true).ok());
                self.prims.soul.comfort(unit, now);
                self.prims.bodies.start(body::Change::Comfort, unit, now);
            }
            (SOUL_LINK_FX, Target::Unit(unit)) => {
                for path in std::iter::once(soul::MOTE).chain(soul::LETTERS) {
                    self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, false).ok());
                }
                self.prims.soul.link(unit, now);
            }
            (BABY_FX, Target::Unit(unit)) => {
                self.textures.entry(sphere::FIRE_IVY.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, sphere::FIRE_IVY, false).ok());
                self.prims.sphere.ribbons(unit, now, |n| pick(0, n.saturating_sub(1)));
            }
            (REVIVE_FX, Target::Unit(_)) => {
                self.textures.entry(walls::RING_YELLOW.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, walls::RING_YELLOW, true).ok());
                self.prims.portal.revive(at, now);
            }
            (LOPE, Target::Unit(_)) => {
                self.textures.entry(petals::MOTE.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, petals::MOTE, true).ok());
                for path in petals::PETALS {
                    self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, false).ok());
                }
                self.prims.petals.lope(at, now);
            }
            (WARM_SPARKLES, Target::Unit(unit)) => {
                for path in crate::prims::sparkle::TEXTURES {
                    self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok());
                }
                self.prims.warm_sparkles(unit, now);
            }
            (KAIZEL_FINS, Target::Unit(unit)) => {
                self.textures.entry(kicks::FIN.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, kicks::FIN, false).ok());
                self.prims.kicks.kaizel(unit, now);
            }
            (KOUENKA, Target::Unit(unit)) => {
                self.textures.entry(brawl::RAY.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, brawl::RAY, true).ok());
                self.sprites.entry(petals::SAKURA.to_owned()).or_insert_with(|| SpriteSet::at(grf, petals::SAKURA));
                // ponytail: the rays' draws come before the petals', not after the first petal's.
                self.prims.brawl.petal_rays(unit, now, |n| pick(0, n.saturating_sub(1)));
                self.prims.petals.kouenka(unit, at, now);
            }
            (SOUL_BODY, Target::Unit(unit)) => self.prims.bodies.start(body::Change::Soul, unit, now),
            (SOUL_BODY_MONSTER, Target::Unit(unit)) => self.prims.bodies.start(body::Change::Stack, unit, now),
            (LAND_BODY, Target::Unit(unit)) => self.prims.bodies.start(body::Change::Drop, unit, now),
            (PRESSED_BODY, Target::Unit(unit)) => self.prims.bodies.start(body::Change::Squash, unit, now),
            (KICKED_BODY, Target::Unit(unit)) => self.prims.bodies.start(body::Change::Hop, unit, now),
            (HIT_BODY, Target::Unit(unit)) => self.prims.bodies.start(body::Change::Flash, unit, now),
            (BABY_BODY, Target::Unit(unit)) => self.prims.bodies.start(body::Change::Shrink, unit, now),
            (LULLABY, Target::Unit(unit)) => {
                for (name, path) in notes::TEXTURES {
                    self.textures.entry(name.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok());
                }
                self.prims.notes.lullaby(unit, at, now);
            }
            (ARRULLO_FX, Target::Unit(unit)) => {
                self.textures.entry(souls::DOT.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, souls::DOT, true).ok());
                self.prims.spears.arrullo(unit, now);
            }
            (ELEMENTAL_ACTION, Target::Unit(unit)) => {
                self.textures.entry(walls::RING_WHITE.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, walls::RING_WHITE, true).ok());
                self.prims.walls.elemental_action(unit, now);
                self.prims.guard.elemental(unit, now);
            }
            (POISON_BUSTER_FX, Target::Unit(unit)) => {
                self.sprites.entry(cells::BLAST.to_owned()).or_insert_with(|| SpriteSet::at(grf, cells::BLAST));
                self.prims.cells.poison_buster(unit, now);
            }
            (GROUND_SHAKE, Target::Unit(unit)) => {
                self.textures.entry(brawl::BLAST.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, brawl::BLAST, true).ok());
                self.prims.brawl.disc(false, unit, now);
            }
            (SOUND_NOTES, _) => {
                for path in guard::NOTES {
                    self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, false).ok());
                }
                self.prims.guard.notes(at, now, |n| pick(0, n.saturating_sub(1)));
            }
            (DEAD_HILL | WARG_DANCE | 868 | 870 | 872 | 874 | 876 | 878, Target::Unit(unit)) => {
                let kind = match id {
                    DEAD_HILL => crowds::Kind::Geographers,
                    WARG_DANCE => crowds::Kind::Wargs,
                    _ => crowds::Kind::of_song(id).unwrap_or(crowds::Kind::Violy),
                };
                self.sprites.entry(kind.sprite().to_owned()).or_insert_with(|| SpriteSet::at(grf, kind.sprite()));
                self.prims.crowds.plant(kind, unit, now);
            }
            (RANDOMIZE, Target::Unit(unit)) => {
                self.textures.entry(crowds::RUNES.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, crowds::RUNES, true).ok());
                self.prims.crowds.runes(unit, now);
            }
            (STRIP_ACCESSORY, _) => {
                for path in guard::ACCESSORIES {
                    self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, false).ok());
                }
                self.prims.guard.accessory(at, now, |n| pick(0, n.saturating_sub(1)));
            }
            (MANHOLE_BODY, Target::Status(unit, _)) => self.prims.brawl.manhole(unit),
            (EF_COLDTHROW, target) => {
                let unit = match target {
                    Target::Unit(unit) | Target::Status(unit, _) => unit,
                    _ => None,
                };
                self.cold_slower(gpu, grf, table, unit, at, at, now);
            }
            (EF_STONERING, target) => {
                self.sprites.entry(STAPO_SPRITE.to_owned()).or_insert_with(|| SpriteSet::at(grf, STAPO_SPRITE));
                let action = stapo_action(pick(0, 1));
                let anchor = match target {
                    Target::Unit(unit) | Target::Status(unit, _) => Anchor::Unit(unit),
                    _ => Anchor::At(at),
                };
                self.stapos.push(Stapo { start: now, anchor, action, placed: Vec::new() });
            }
            (EF_STASIS, target) => {
                let unit = match target {
                    Target::Unit(unit) | Target::Status(unit, _) => unit,
                    _ => None,
                };
                for path in snow::THUNDER_BALLS {
                    self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok());
                }
                self.prims.snow.stasis(unit, now, |n| pick(0, n.saturating_sub(1)));
            }
            (EF_FROSTDIVER3, _) => {
                self.jack_frost(gpu, grf, at, now, |n| pick(0, n.saturating_sub(1)));
            }
            (EF_RECOGNIZED, Target::Unit(unit)) => {
                for path in vortex::MG_ICONS.into_iter().chain(vortex::WZ_ICONS) {
                    self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok());
                }
                self.prims.vortex.recognized(unit, now);
            }
            (813..=818, Target::Unit(unit)) => {
                self.textures.entry(songs::STREAK_TEXTURE.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, songs::STREAK_TEXTURE, true).ok());
                self.prims.songs.hairlines(unit, now, |n| pick(0, n.saturating_sub(1)));
            }
            (MAGNETIC_FIELD, Target::Unit(unit)) => {
                self.textures.entry(mechanic::ELEC.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, mechanic::ELEC, true).ok());
                self.prims.mechanic.magnetic_field(unit, now);
            }
            (INFRARED_SCAN, Target::Unit(unit)) => {
                self.textures.entry(mechanic::ELEC.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, mechanic::ELEC, true).ok());
                let facing = self.facings.get(&unit).copied().unwrap_or(0.0);
                self.prims.radar.scan(unit, facing, now, |n| pick(0, n.saturating_sub(1)));
            }
            (NEUTRAL_BARRIER, Target::Unit(unit)) => {
                for path in [guard::TILE, guard::HOLY_TILE] {
                    self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok());
                }
                let facing = self.facings.get(&unit).copied().unwrap_or(0.0);
                self.prims.guard.neutral_barrier(unit, facing, now);
            }
            (REPAIR, Target::Unit(unit)) => {
                self.textures
                    .entry(crate::prims::sparkle::SPANNER_PICTURE.to_owned())
                    .or_insert_with(|| gpu.try_load_texture(grf, crate::prims::sparkle::SPANNER_PICTURE, false).ok());
                self.prims.spanners(unit, now);
            }
            (ANALYZE, Target::Unit(unit)) => {
                self.textures.entry(circle::TARGET.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, circle::TARGET, true).ok());
                self.prims.circles.mark(unit, now, |n| pick(0, n.saturating_sub(1)));
                self.prims.guard.analyze(unit, now);
            }
            (VENOM_ICON, Target::Status(unit, _)) => {
                self.textures.entry(circle::VENOM.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, circle::VENOM, false).ok());
                self.prims.circles.venom(unit, now);
            }
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
            (DISARM, _) => {
                for path in guard::GUNS {
                    self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok());
                }
                self.prims.guard.disarm(at, now, |n| pick(0, n.saturating_sub(1)));
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
            (GREY_BODY, Target::Status(unit, _)) => self.prims.guard.grey(unit),
            (SPARK_MOVER | SPARK_MOVER_DARK, Target::Unit(_)) => {
                self.textures.entry(linker::RING_BLUE.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, linker::RING_BLUE, true).ok());
                self.prims.linker.spark_mover(id == SPARK_MOVER_DARK, at, now, |n| pick(0, n.saturating_sub(1)));
            }
            (BLUE_HIT, Target::Unit(unit)) => self.prims.brawl.blue_hit(unit, now),
            (ESMA_FROST, Target::Unit(unit)) => {
                self.textures.entry(linker::FROST.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, linker::FROST, false).ok());
                self.prims.linker.frost(unit, now, |n| pick(0, n.saturating_sub(1)));
            }
            (SPINED_BODY | PROMINENCE_SPIN, Target::Unit(unit)) => self.prims.brawl.swing(unit, now),
            (MISTY_FROST, Target::Status(unit, _)) => self.prims.bodies.start(body::Change::MistyFrost, unit, now),
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
            (HEAD_CRUSH | JOINT_BEAT | DUST, Target::Unit(unit)) => {
                self.textures.entry(knight::RAY.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, knight::RAY, true).ok());
                let rand = |n: u32| pick(0, n.saturating_sub(1));
                match id {
                    HEAD_CRUSH => self.prims.knight.head_crush(unit, now, rand),
                    JOINT_BEAT => self.prims.knight.joint_beat(unit, now, rand),
                    _ => self.prims.knight.dust(unit, now, rand),
                }
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
            (KAUTE_PALE | KAUTE_DARK, Target::Unit(unit)) => {
                self.sprites.entry(crate::prims::GHOST_SPRITE.to_owned()).or_insert_with(|| SpriteSet::at(grf, crate::prims::GHOST_SPRITE));
                self.prims.kaute_sparks(at, now, id == KAUTE_DARK, |n| pick(0, n.saturating_sub(1)));
                self.prims.transcend.indulge(unit, now);
            }
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
                self.prims.knight.pressure(false, unit, now, |n| pick(0, n.saturating_sub(1)));
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
            let mut animation = Animation::new(part, id, now, anchor(part.attached), lasting);
            animation.length = table.lengths.get(&id).copied();
            animation.roll(|n| pick(0, n.saturating_sub(1)));
            self.sprites.entry(animation.sprite.clone()).or_insert_with(|| SpriteSet::at(grf, &animation.sprite));
            self.animations.push(animation);
        }
        self.play_quakes(table, id, now);
        if matches!(id, EARTH_SPIKE | HEAVENS_DRIVE | HYOUSENSOU) {
            let path = if id == HYOUSENSOU { crate::prims::ICE_TEXTURE } else { stone::STONE };
            self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok());
            match id {
                EARTH_SPIKE => self.prims.stone.earth_spike(at, now),
                HYOUSENSOU => self.prims.stone.ice_spike(at, now),
                _ => self.prims.stone.heavens_drive(at, now),
            }
        }
        // Ice Wall's cell: a skill unit's, the spikes standing until it goes.
        if let (ICE_WALL, Target::Lasting(unit) | Target::Kept(unit)) = (id, target) {
            self.textures.entry(crate::prims::ICE_TEXTURE.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, crate::prims::ICE_TEXTURE, true).ok());
            self.prims.stone.ice_wall(unit, at, now, |n| pick(0, n.saturating_sub(1)));
        }
        // Blessing, made in code.
        // A song's puffs and Steel Body's glints, made in code.
        let kind = song_variant(id).map(PrimKind::Puffs).or((id == 254 || id == GUMGANG || id == POWER_UP || id == WARMTH_GLINTS).then_some(PrimKind::Glints));
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
            // Power Up's status removes the unit's running glints first; its use alone only starts a set.
            if let (POWER_UP, Target::Status(unit, _)) = (id, target) {
                self.status_prims
                    .retain(|prim| !(prim.effect == POWER_UP && matches!(prim.anchor, Anchor::Unit(each) | Anchor::Status(each, _) if each == unit)));
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
                hold: part.hold,
            });
        }
    }

    /// EF_HIT1 on a unit (None = the player) facing `rot` degrees (as `Actor::rot`), from `start`.
    pub fn play_hit(&mut self, gpu: &Gpu, grf: &Grf, unit: Option<u32>, rot: f32, start: Instant) {
        self.textures.entry(RING_TEXTURE.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, RING_TEXTURE, true).ok());
        self.sprites.entry(SPARK_SPRITE.to_owned()).or_insert_with(|| SpriteSet::at(grf, SPARK_SPRITE));
        let way = drift(rot);
        self.hits.push(HitSpark { start, anchor: Anchor::Unit(unit), way, sparks: sparks(way, random), paths: None, placed: Vec::new(), dark: false });
    }

    /// EF_HITDARK (180): EF_HIT1 with near-black sparks that `/mineffect` keeps.
    pub fn play_hit_dark(&mut self, gpu: &Gpu, grf: &Grf, unit: Option<u32>, rot: f32, start: Instant) {
        self.play_hit(gpu, grf, unit, rot, start);
        if let Some(hit) = self.hits.last_mut() {
            hit.dark = true;
        }
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
        self.prims.crowds.advance(now, &position_of, &ground_at, |n| pick(0, n.saturating_sub(1)));
        self.prims.cells.advance(now, &ground_at, |n| pick(0, n.saturating_sub(1)));
        self.make_areas(now, &ground_at, |n| pick(0, n.saturating_sub(1)));
        for effect in self.prims.cells.take_restarts() {
            for file in self.cell_sounds.get(&effect).into_iter().flatten() {
                self.sounds.push((now, file.clone(), LISTENER));
            }
        }
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
        if effect == BUBBLE_CAST {
            for path in songs::BUBBLES {
                self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok());
            }
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

    /// A Chase Walk print (326 left, 327 right) where our walker stands, turned `heading` degrees.
    pub fn chase_print(&mut self, gpu: &Gpu, grf: &Grf, effect: u32, at: Vector, heading: f32, now: Instant) {
        for path in [kicks::CHASE_LEFT, kicks::CHASE_RIGHT] {
            self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, false).ok());
        }
        self.prims.kicks.chase_print(effect, at, heading, now);
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
    #[allow(clippy::too_many_arguments)]
    pub fn play_unit(&mut self, gpu: &Gpu, grf: &Grf, table: &Table, kind: u32, id: u32, at: Vector, range: u8, now: Instant) {
        // Epiclesis' rings start once with their unit: a second sight of it leaves them be.
        if table.units.get(&kind) == Some(&EPICLESIS_CELL) && self.prims.holes.stands(id) {
            return;
        }
        // The server tells again of units coming back into sight.
        self.forget(id);
        self.unit_kinds.insert(id, kind);
        // Poison Mist's cells put a puff of mist on a grid round them, now and every length − 20 ticks.
        if let Some(&(effect, lift)) = table.areas.get(&kind) {
            let period = table.lengths.get(&effect).copied().unwrap_or(21).saturating_sub(20).max(1);
            self.areas.push(Area { id, effect, lift, range, at, next: now, period });
        }
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
            } else if effect == MELODY_NOTE {
                // Poem of Netherworld's cell floats a melody note, with its sound at the unit's place.
                self.textures.entry(crowds::MELODY.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, crowds::MELODY, false).ok());
                self.play_sounds(table, effect, at, now);
                self.prims.crowds.note(id, at, now, |n| pick(0, n.saturating_sub(1)));
            } else if matches!(effect, CHAOS_CELL | MAELSTROM_CELL | BLOODY_CELL | EPICLESIS_CELL) {
                // Chaos Panic's, Maelstrom's and Bloody Lust's cells turn their squares, and Epiclesis' cell spreads yellow-green rings,
                // for as long as their unit stands.
                let look = match effect {
                    CHAOS_CELL => holes::Look::ChaosPanic,
                    MAELSTROM_CELL => holes::Look::Maelstrom,
                    EPICLESIS_CELL => holes::Look::Epiclesis,
                    _ => holes::Look::BloodyLust,
                };
                for path in holes::textures(&look, self.min).into_iter().chain([walls::RING_WHITE]) {
                    self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok());
                }
                self.prims.holes.keep(id, look, at, now, self.min, |n| pick(0, n.saturating_sub(1)));
            } else if matches!(effect, MANHOLE_CELL | DIMENSION_CELL) {
                // Manhole's and Dimension Door's cells turn a blue swirl and throw ring triples, with the door's shafts.
                let door = effect == DIMENSION_CELL;
                let look = if door { holes::Look::DimensionDoor } else { holes::Look::Manhole };
                for path in holes::textures(&look, self.min).into_iter().chain([walls::RING_WHITE, shafts::CLOUD]) {
                    self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok());
                }
                self.prims.holes.keep(id, look, at, now, self.min, |n| pick(0, n.saturating_sub(1)));
                if door {
                    self.prims.shafts.keep(id, at, now);
                }
            } else if matches!(effect, BARRIER_CELL | STEALTH_CELL) {
                // Neutral Barrier's and Stealth Field's cells breathe a faint band while their unit stands.
                let path = if effect == BARRIER_CELL { mechanic::BARRIER_BAND } else { mechanic::STEALTH_BAND };
                self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok());
                self.prims.mechanic.keep_band(id, effect == STEALTH_CELL, at, now, |n| pick(0, n.saturating_sub(1)));
            } else if effect == POISON_SMOKE_CELL {
                // Poison Smoke's cells throw violet sparkles over everything while their unit stands.
                self.sprites.entry(first::POISON_STAR.to_owned()).or_insert_with(|| SpriteSet::at(grf, first::POISON_STAR));
                self.prims.walls.keep_smoke(id, at, now);
            } else if matches!(effect, WALL_OF_THORNS | VACUUM_EXTREME | WARMER | DEMONIC_FIRE..=DEMONIC_FIRE_3) {
                // Wall of Thorns' tree, Vacuum Extreme's whirlwinds and the Ifrits of Demonic Fire and the Insignias stand with their
                // unit, a new copy at the end of each length.
                let path = match effect {
                    WALL_OF_THORNS => cells::ANCIENT_TREE,
                    VACUUM_EXTREME => cells::BREEZE,
                    WARMER => cells::IMP,
                    _ => cells::IFRIT,
                };
                self.sprites.entry(path.to_owned()).or_insert_with(|| SpriteSet::at(grf, path));
                self.cell_sounds.insert(effect, table.sounds.get(&effect).into_iter().flatten().map(|sound| format!("{}.wav", sound.file)).collect());
                self.play_sounds(table, effect, at, now);
                if effect == WALL_OF_THORNS || effect == VACUUM_EXTREME {
                    self.prims.cells.stand(effect, id, at, now, |n| pick(0, n.saturating_sub(1)));
                } else {
                    self.prims.cells.ring(effect, id, cells::insignia_tint(kind), at, now);
                }
            } else if effect == PORTAL_FX {
                // The family call's cell burns a pink warp for its first 23 seconds.
                for path in [portal::RING_RED, portal::CLOUD] {
                    self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok());
                }
                self.prims.portal.keep(id, at, now, |n| pick(0, n.saturating_sub(1)));
            } else if effect == KINGS_GRACE {
                // King's Grace's four emblems stand round the cell with their unit.
                self.textures.entry(emblems::EMBLEM.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, emblems::EMBLEM, false).ok());
                self.prims.emblems.keep(id, at, now);
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

    /// Cold Slower (791): three prims of four snowflakes thrown from `feet` toward `aim`.
    pub fn cold_slower(&mut self, gpu: &Gpu, grf: &Grf, table: &Table, unit: Option<u32>, feet: Vector, aim: Vector, now: Instant) {
        self.play_sounds(table, EF_COLDTHROW, feet, now);
        self.textures.entry(snow::SNOW_TEXTURE.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, snow::SNOW_TEXTURE, true).ok());
        self.prims.snow.cold_slower(unit, feet, aim, now, |n| pick(0, n.saturating_sub(1)));
    }

    /// Jack Frost (EF_FROSTDIVER3): 10 lines of Frost Diver spikes on source feet aimed 80 units out.
    pub fn jack_frost(&mut self, gpu: &Gpu, grf: &Grf, from: Vector, now: Instant, rand: impl FnMut(u32) -> u32) {
        self.sounds.push((now, "jack_frost.wav".to_owned(), LISTENER));
        if self.min {
            return;
        }
        self.textures.entry(crate::prims::ICE_TEXTURE.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, crate::prims::ICE_TEXTURE, true).ok());
        self.prims.stone.jack_frost_star(from, now, rand);
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
            HIT_LINE_COUNTER => {
                self.textures.entry(kicks::LINE.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, kicks::LINE, true).ok());
                self.prims.kicks.hit(kicks::Kind::Counter, (target != own).then_some(target), Some(from), to, now);
            }
            FULL_BUSTER | SPREAD_ATTACK => {
                // The blast flares toward the target's side of the screen: one of the sprite's four diagonals, chosen once.
                self.play(gpu, grf, table, id, Target::Unit(caster), from, now);
                let (longitude, heading) = (self.yaw.to_degrees().floor() as i32, bearing_between(from, to));
                let action = if id == SPREAD_ATTACK { spread_attack_action(longitude, heading) } else { full_buster_action(longitude, heading) };
                for animation in self.animations.iter_mut().filter(|animation| animation.effect == id && animation.start == now) {
                    animation.action = Some(action);
                }
            }
            STIN => {
                self.textures.entry(soul::BLADE.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, soul::BLADE, false).ok());
                self.play_sounds(table, id, from, now);
                self.prims.soul.stin(from, to, now, |n| pick(0, n.saturating_sub(1)));
            }
            PIERCING_LINES => {
                self.textures.entry(brawl::GOLD_STRIP.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, brawl::GOLD_STRIP, true).ok());
                self.play_sounds(table, id, from, now);
                self.prims.brawl.shot_lines(from, to, now, |n| pick(0, n.saturating_sub(1)));
            }
            KAMAITACHI => {
                self.textures.entry(wheels::BLADE.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, wheels::BLADE, false).ok());
                self.play_sounds(table, id, from, now);
                self.prims.wheels.throw(caster, from, to, now);
            }
            ESWHOO_DISCS => {
                self.textures.entry(linker::BLADE.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, linker::BLADE, false).ok());
                self.play_sounds(table, id, from, now);
                self.prims.linker.dark_discs(from, (target != own).then_some(target), now);
            }
            SOUL_UNITY => {
                self.textures.entry(transcend::CLOUD.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, transcend::CLOUD, true).ok());
                // The tube runs from the target to the caster, whose id the packet names.
                self.prims.transcend.unity((target != own).then_some(target), caster.unwrap_or(own), own, now);
            }
            ESMA_DISCS => {
                self.textures.entry(linker::BLADE.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, linker::BLADE, false).ok());
                self.play_sounds(table, id, from, now);
                self.prims.linker.discs(from, (target != own).then_some(target), now);
            }
            ESTUN_MOVER => {
                for path in linker::THUNDER {
                    self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok());
                }
                self.play_sounds(table, id, from, now);
                self.prims.linker.mover(from, to, now, |n| pick(0, n.saturating_sub(1)));
            }
            AXE_THROW => {
                self.textures.entry(boomerang::AXE.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, boomerang::AXE, true).ok());
                self.play_sounds(table, id, from, now);
                self.prims.boomerangs.throw_axe(caster, from, to, now, self.dim);
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
            EF_TETRA => {
                for path in vortex::TV_TEXTURES {
                    self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, false).ok());
                }
                self.textures.entry(vortex::RING_RED.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, vortex::RING_RED, true).ok());
                self.play_sounds(table, id, from, now);
                self.play_quakes(table, id, now);
                self.prims.vortex.tetra(to, now, |n| pick(0, n.saturating_sub(1)));
            }
            PINPOINT_FX => {
                for path in [guard::SHIELD, guard::SWORD] {
                    self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok());
                }
                self.play_sounds(table, id, from, now);
                self.prims.guard.pinpoint(caster, to, now, |n| pick(0, n.saturating_sub(1)));
            }
            ELBOW_LINES => {
                self.textures.entry(sniper::CLOUD.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, sniper::CLOUD, true).ok());
                self.play_sounds(table, id, from, now);
                self.prims.sniper.elbow(caster, to, now);
            }
            KNUCKLE_LINES => {
                self.textures.entry(brawl::STRIP.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, brawl::STRIP, true).ok());
                self.play_sounds(table, id, from, now);
                self.prims.brawl.knuckle_lines(from, to, now, |n| pick(0, n.saturating_sub(1)));
            }
            CROSS_LINES => {
                for path in if self.min { [brawl::CROSS_PITCH_MIN, brawl::CROSS_RED_MIN] } else { [brawl::CROSS_PITCH, brawl::CROSS_RED] } {
                    self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok());
                }
                self.prims.brawl.cross_lines(from, to, self.min, now, |n| pick(0, n.saturating_sub(1)));
            }
            SPEAR_RUSH => {
                self.textures.entry(brawl::GOLD_STRIP.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, brawl::GOLD_STRIP, true).ok());
                self.play_quakes(table, id, now);
                self.prims.brawl.spear_lines(from, to, now, |n| pick(0, n.saturating_sub(1)));
                self.prims.guard.spear(caster, now);
            }
            CANNON_LINES => {
                self.textures.entry(brawl::GOLD_STRIP.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, brawl::GOLD_STRIP, true).ok());
                self.play_sounds(table, id, from, now);
                self.prims.brawl.cannon_lines(from, to, now, |n| pick(0, n.saturating_sub(1)));
            }
            GREAT_ECHO_V => {
                self.textures.entry(spears::LINE.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, spears::LINE, true).ok());
                self.play_sounds(table, id, from, now);
                self.play_quakes(table, id, now);
                self.prims.spears.chevron(from, to, now);
            }
            SONIC_WAVE => {
                for path in songs::BUBBLES {
                    self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok());
                }
                self.play_sounds(table, id, from, now);
                self.prims.waves.sonic(from, to, self.min, now);
                self.prims.guard.sonic(caster, now);
            }
            SHADOW_LINK => {
                self.textures.entry(transcend::CLOUD.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, transcend::CLOUD, true).ok());
                self.prims.transcend.shadow(caster, target, own, now);
            }
            BOOST_KNUCKLE => {
                self.sprites.entry(balls::FIST_SPRITE.to_owned()).or_insert_with(|| SpriteSet::at(grf, balls::FIST_SPRITE));
                self.play_sounds(table, id, from, now);
                self.prims.balls.boost_knuckle(from, to, self.yaw.to_degrees().floor() as i32, now);
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
        self.prims.mechanic.advance(now, &position_of, |n| pick(0, n.saturating_sub(1)));
        self.prims.waves.advance(now, |n| pick(0, n.saturating_sub(1)));
        self.prims.chi.advance(now);
        self.prims.radar.advance(now);
        // Harmonize's emitters launch a pair of bands of soft lights (850) on their units each tick.
        self.prims.spears.advance(now);
        for (arrullo, unit, offset) in self.prims.spears.take_launched() {
            self.prims.souls.launch(arrullo, unit, offset, now, |n| pick(0, n.saturating_sub(1)));
        }
        self.prims.souls.advance(now, |n| pick(0, n.saturating_sub(1)));
        self.prims.notes.advance(now, &position_of, |n| pick(0, n.saturating_sub(1)));
        self.prims.emblems.advance(now);
        self.prims.sheets.advance(now);
        self.prims.bodies.advance(now);
        self.prims.portal.advance(now, |n| pick(0, n.saturating_sub(1)));
        self.prims.sphere.advance(now, |n| pick(0, n.saturating_sub(1)));
        self.prims.petals.advance(now, |n| pick(0, n.saturating_sub(1)));
        self.prims.petals.drift(now, &position_of, |n| pick(0, n.saturating_sub(1)));
        self.prims.soul.advance(now, &position_of, self.yaw.to_degrees());
        // The soul lights (506) the mote launches along its path.
        for (unit, offset) in self.prims.soul.take_launches() {
            self.prims.souls.launch_soul(unit, offset, now, |n| pick(0, n.saturating_sub(1)));
        }
        self.prims.linker.advance(now, &position_of, |n| pick(0, n.saturating_sub(1)));
        self.prims.wheels.advance(now, &position_of, |n| pick(0, n.saturating_sub(1)));
        self.prims.kicks.advance(now, &position_of, |n| pick(0, n.saturating_sub(1)));
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
        self.status_prims.retain(|prim| {
            let age = str_frame(now.saturating_duration_since(prim.start));
            match prim.kind {
                PrimKind::Puffs(_) => age < PUFF_TICKS,
                // Power Up's glints last 1 500 ticks a launch.
                PrimKind::Glints if prim.effect == POWER_UP => age < POWER_UP_TICKS,
                PrimKind::Glints => true,
            }
        });
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
        for drawn in self.prims.sights.sprites(now).into_iter().chain(self.prims.cells.flames(now, |_| Some([0.0; 3]))) {
            if let (Some(Some(sprite)), false) = (self.sprites.get_mut(drawn.sprite), self.sight_frames.contains_key(&(drawn.sprite, drawn.frame))) {
                let frames = sprite.frames(0).max(1);
                let layers = sprite.frame_plain(gpu, 0, Timing::Span((drawn.frame as f32 + 0.5) / frames as f32));
                self.sight_frames.insert((drawn.sprite, drawn.frame), layers);
            }
        }
        for draw in self.prims.crowds.draws(now).into_iter().chain(self.prims.cells.ifrits(now)).chain(self.prims.petals.falls()) {
            if let Some(Some(sprite)) = self.sprites.get_mut(draw.sprite) {
                let frames = sprite.frames(draw.action).max(1);
                let frame = draw.steps as usize % frames;
                self.crowd_frames
                    .entry((draw.sprite, draw.action, frame))
                    .or_insert_with(|| sprite.frame_at(gpu, draw.action, 0, Timing::Span((frame as f32 + 0.5) / frames as f32)));
            }
        }
        // The tree, the whirlwinds and the blasts: plain frames, each layer with the alpha its ACT gives it.
        let standing = self.prims.cells.stands(now).into_iter().map(|draw| (draw.sprite, draw.action, draw.steps as usize));
        let blasts = self.prims.cells.blast_frames(now).into_iter().map(|frame| (cells::BLAST, 0, frame));
        for (path, action, steps) in standing.chain(blasts) {
            if let Some(Some(sprite)) = self.sprites.get_mut(path) {
                let frames = sprite.frames(action).max(1);
                let frame = steps % frames;
                self.cell_frames.entry((path, action, frame)).or_insert_with(|| plain_layers(gpu, sprite, action, frame, frames));
            }
        }
        for draw in self.prims.balls.fist_draws() {
            if let Some(Some(sprite)) = self.sprites.get_mut(balls::FIST_SPRITE) {
                let frames = sprite.frames(draw.action).max(1);
                let timing = Timing::Span((draw.frame as f32 + 0.5) / frames as f32);
                let ghost = draw.ghost.is_some();
                self.fist_frames
                    .entry((draw.action, draw.frame, ghost))
                    .or_insert_with(|| if ghost { sprite.frame_plain(gpu, draw.action, timing) } else { sprite.frame_at(gpu, draw.action, 0, timing) });
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
        let unit_kinds = &self.unit_kinds;
        let tick = Duration::from_secs_f32(STR_TICK_MS / 1000.0);
        self.playing.retain_mut(|playing| {
            let length = tick * str_ticks(&playing.loaded.animation, playing.length);
            let kind = match playing.anchor {
                Anchor::Lasting(id, _) => unit_kinds.get(&id).copied().unwrap_or(0),
                _ => 0,
            };
            let period = tick * restart_ticks(playing.effect, kind, playing.length.unwrap_or(str_ticks(&playing.loaded.animation, None)));
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
            now < playing.start + tick * playing_ticks(playing.effect, &playing.loaded.animation, playing.length, playing.hold)
        });
        self.playing.extend(tails);

        // ponytail: sprites face south as seen; the few effect sprites drawn in 8 directions would face their unit.
        let facing = usize::from(crate::seen_direction(0.0, yaw, true));
        let sprites = &mut self.sprites;
        let unit_kinds = &self.unit_kinds;
        let tick = Duration::from_secs_f32(STR_TICK_MS / 1000.0);
        self.animations.retain_mut(|animation| {
            let Some(Some(sprite)) = sprites.get_mut(&animation.sprite) else { return false };
            // A ground unit's sprite starts again when its effect does, with new choices.
            if let (true, Anchor::Lasting(unit, _), Some(length)) = (animation.restarts, animation.anchor, animation.length) {
                let kind = unit_kinds.get(&unit).copied().unwrap_or(0);
                let period = tick * restart_ticks(animation.effect, kind, length);
                // After a stall it starts once, on the period it is in.
                let missed = (now.saturating_duration_since(animation.start).as_nanos() / period.as_nanos().max(1)) as u32;
                if missed > 0 {
                    animation.start += period * missed;
                    animation.roll(|n| pick(0, n.saturating_sub(1)));
                }
                animation.end = Some(animation.start + period);
            }
            let index = animation.action.unwrap_or(facing) % sprite.actions().max(1);
            let frames = sprite.frames(index).max(1) as f32;
            let frame_ms = animation.frame_ms.unwrap_or_else(|| sprite.index_ms(index).unwrap_or(100.0) / frames);
            // Played once from a later frame, it ends after the frames it shows.
            let pass = (frames - animation.first.min(frames - 1.0)) * frame_ms.max(1.0);
            if now < animation.start {
                animation.placed.clear();
                return true;
            }
            let elapsed = now.saturating_duration_since(animation.start).as_secs_f32() * 1000.0;
            if animation.end.is_some_and(|end| now >= end) || (!animation.repeat && !animation.hold && elapsed >= pass) {
                return false;
            }
            let progress = animation_progress(elapsed, frame_ms, frames, animation.first, animation.repeat);
            animation.placed = sprite.frame_at(gpu, index, 0, Timing::Span(progress));
            true
        });
        self.stapos.retain_mut(|stapo| {
            let age = str_frame(now.saturating_duration_since(stapo.start));
            let Some(frame) = stapo_frame(age) else { return false };
            let Some(Some(sprite)) = sprites.get_mut(STAPO_SPRITE) else { return true };
            let frames = sprite.frames(stapo.action).max(1);
            stapo.placed = sprite.frame_at(gpu, stapo.action, 0, Timing::Span((frame as f32 + 0.5) / frames as f32));
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
        if effect == SIGHT {
            self.prims.cells.stop_sight(unit);
        }
    }

    /// The units that have the Sight option now (None = the player): each starts its circling flame the moment it has it and every
    /// 270 ticks while it lasts.
    pub fn watch_sight(&mut self, grf: &Grf, units: impl IntoIterator<Item = (Option<u32>, bool)>, now: Instant) {
        let units: Vec<_> = units.into_iter().collect();
        // The flame and its shadow are loaded when a unit first has the option, not only by a Wide Sight.
        if units.iter().any(|&(_, on)| on) {
            for path in [sight::SIGHT, sight::SHADOW] {
                self.sprites.entry(path.to_owned()).or_insert_with(|| SpriteSet::at(grf, path));
            }
        }
        self.prims.cells.watch_sight(units, now);
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

    /// Starts the grids that are due by `now`: per skill unit of an `area` type, (2r + 1)² points 6 units apart round its cell (r its
    /// range), each jittered by `rand(11)` draws (x, z, then the height), the ground under it from `ground_at(x, z)`. They wait to be
    /// played by `take_areas`.
    pub fn make_areas(&mut self, now: Instant, ground_at: impl Fn(f32, f32) -> f32, mut rand: impl FnMut(u32) -> u32) {
        let tick = Duration::from_secs_f32(STR_TICK_MS / 1000.0);
        for area in &mut self.areas {
            // A long gap (a hidden window) starts one grid, not all it missed.
            if area.next + tick * area.period * 3 < now {
                area.next = now;
            }
            while area.next <= now {
                let r = i32::from(area.range);
                for i in -r..=r {
                    for j in -r..=r {
                        let x = area.at[0] + 6.0 * i as f32 + 0.2 * rand(11) as f32 - 1.0;
                        let z = area.at[2] + 6.0 * j as f32 + 0.2 * rand(11) as f32 - 1.0;
                        let y = ground_at(x, z) - area.lift + 0.4 * rand(11) as f32;
                        self.area_queue.push((area.effect, [x, y, z], area.next));
                    }
                }
                area.next += tick * area.period;
            }
        }
    }

    /// The grid points made since this was last asked: the effect, where to play it and when.
    pub fn take_areas(&mut self) -> Vec<(u32, Vector, Instant)> {
        std::mem::take(&mut self.area_queue)
    }

    /// The skill unit type a ground unit was shown as.
    pub fn unit_kind(&self, unit: u32) -> Option<u32> {
        self.unit_kinds.get(&unit).copied()
    }

    /// Stops what a status plays on its unit (None = the player).
    pub fn forget_status(&mut self, unit: Option<u32>, status: u16) {
        self.worlds.remove_status(unit, Some(status));
        self.prims.songs.forget_status(unit, status);
        self.prims.transcend.forget_status(unit, status);
        self.prims.brawl.forget_status(unit, status);
        // Granitic Armor, Kyomu or Gensou went off (or comes on again: its new copy follows at once).
        if matches!(status, 606 | 655 | 659) {
            self.prims.guard.ungrey(unit, Instant::now());
        }
        self.prims.waves.forget_status(unit, status);
        self.prims.chi.forget_status(unit, status);
        // Running's status takes its start away, and ends with it.
        if status == RUN_STATUS_EFST {
            self.prims.kicks.status(unit, false);
        }
        self.prims.bodies.forget_status(unit, status);
        self.prims.snow.forget_status(unit, status);
        if status == 330 {
            self.prims.circles.forget(unit);
        }
        if status == VENOM_STATUS {
            self.prims.circles.forget_venom(unit);
        }
        // Camouflage, Stealth Field, Elemental Veil or Shadow Form went off: the unit climbs back to opaque.
        if matches!(status, 360 | 379 | 1301 | 394) {
            self.prims.brawl.uncloak(unit, Instant::now());
        }
        let on = |anchor: Anchor| matches!(anchor, Anchor::Status(each, kind) if each == unit && kind == status);
        self.status_prims.retain(|prim| !on(prim.anchor));
        self.auras.retain(|aura| !on(aura.anchor));
        self.playing.retain(|playing| !on(playing.anchor));
        self.floating.retain(|floating| !on(floating.anchor));
        self.rings.retain(|ring| !on(ring.anchor));
        self.animations.retain(|animation| !on(animation.anchor));
        self.stapos.retain(|stapo| !on(stapo.anchor));
    }

    /// Stops a playing STR effect on its unit (None = the player).
    pub fn forget_effect(&mut self, unit: Option<u32>, effect: u32) {
        self.playing.retain(|p| !(p.anchor.unit() == Some(unit) && p.effect == effect));
    }

    pub fn forget(&mut self, unit: u32) {
        self.areas.retain(|area| area.id != unit);
        self.worlds.remove_status(Some(unit), None);
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
        self.stapos.retain(|stapo| !stapo.anchor.on(unit));
    }

    /// The colour an effect gives `unit`'s body at `now` (Berserk Pitcher's red).
    pub fn body_tint(&self, unit: Option<u32>, now: Instant) -> Option<[u8; 3]> {
        let flashing = mix_tints(self.prims.walls.body_tint(unit, now), self.prims.ground.body_tint(unit, now));
        let flashing = mix_tints(flashing, self.prims.guard.body_tint(unit, now));
        let flashing = mix_tints(flashing, self.prims.sniper.body_tint(unit, now));
        let flashing = mix_tints(flashing, self.prims.brawl.body_tint(unit, now));
        let flashing = mix_tints(flashing, self.prims.songs.body_tint(unit, now));
        let flashing = mix_tints(flashing, self.prims.transcend.body_tint(unit, now));
        mix_tints(flashing, self.prims.snow.body_tint(unit, now))
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
            .or_else(|| self.prims.bodies.glow(unit, now))
            .or_else(|| self.prims.snow.glow(unit, now))
    }

    /// How many degrees `unit` has been turned since this was last asked (Falcon Assault's spin, Chase Walk's cast while it stands).
    pub fn turned(&mut self, unit: Option<u32>, now: Instant, standing: bool) -> f32 {
        self.prims.sniper.turned(unit, now) + self.prims.guard.turned(unit, now, standing) + self.prims.brawl.turned(unit, now)
    }

    /// A cast cut off: Twilight Alchemy's icons go.
    pub fn cut_twilight(&mut self) {
        self.prims.songs.cut_twilight();
        self.prims.guard.cut_pierce();
        self.prims.sphere.cut_hearts();
    }

    /// The factor an effect multiplies `unit`'s sprite by now (Swoo's shrink).
    pub fn body_scale(&self, unit: Option<u32>, now: Instant) -> f32 {
        self.prims.bodies.scale(unit, now)
    }

    /// Squashes or lifts the layers of `unit`'s frame (Down Kick, Counter Kick, Flash Kick).
    pub fn reshape(&self, unit: Option<u32>, now: Instant, placed: &mut [Placed]) {
        self.prims.bodies.reshape(unit, now, placed, |n| pick(0, n.saturating_sub(1)));
    }

    /// How opaque an effect draws `unit` now (Camouflage's translucence), 0 to 1.
    pub fn cloak_tint(&self, unit: Option<u32>, now: Instant) -> [f32; 4] {
        self.prims.brawl.cloak_tint(unit, now).unwrap_or([1.0; 4])
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
            // Crescent Elbow's lines, with no depth test.
            meshes.extend(
                self.prims
                    .sniper
                    .top_meshes(now)
                    .into_iter()
                    .filter_map(|wall| Some((self.textures.get(wall.texture)?.as_ref()?, wall.vertices, wall.additive))),
            );
            // Strip Accessory's icons in upright planes, seen through the ground.
            meshes.extend(
                self.prims
                    .guard
                    .upright_pieces(now)
                    .into_iter()
                    .filter_map(|wall| Some((self.textures.get(wall.texture)?.as_ref()?, wall.vertices, wall.additive))),
            );
            // Dark Illusion's lines, with no depth test.
            meshes.extend(
                self.prims
                    .brawl
                    .top_meshes(now, &position_of)
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

    /// Wide Bleeding's red sheet over the view, for a camera looking along `forward`: drawn last, unfogged, over everything.
    pub fn cover_quads(&self, now: Instant, forward: Vector, position_of: impl Fn(Option<u32>) -> Option<Vector>) -> Vec<world::CoverQuad<'_>> {
        let Some(Some(texture)) = self.textures.get(sheet::TEXTURE).filter(|_| !self.only_forced) else { return Vec::new() };
        let toward = forward.map(|axis| -axis);
        self.prims.sheets.draws(now, toward, position_of).into_iter().map(|draw| (texture, draw.at, draw.corners, draw.uv, draw.tint)).collect()
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
            let (_, _, additive) = aura_look(aura.effect);
            let at = aura.anchor.place(&position_of);
            (0..aura_sets(aura.effect)).filter_map(move |set| {
                let at = at?;
                // The bubble cast: the picture moves every tick, set 1 blue and added on, set 2 white and blended.
                let path = if aura.effect == BUBBLE_CAST { songs::BUBBLES[((age + 1) % 27) as usize].to_owned() } else { aura_set_texture(aura.effect, set) };
                let color = aura_colour(aura.effect, set, age).map(|value| f32::from(value) / 255.0);
                let (color, additive) = if aura.effect == BUBBLE_CAST {
                    (if set == 0 { [100.0 / 255.0, 100.0 / 255.0, 1.0] } else { [1.0; 3] }, set == 0)
                } else {
                    (color, additive)
                };
                let texture = self.textures.get(&path).and_then(Option::as_ref)?;
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
        let spikes = self
            .prims
            .stone
            .meshes(|n| pick(0, n.saturating_sub(1)))
            .into_iter()
            .filter_map(|(path, triangles, additive)| Some((self.textures.get(path)?.as_ref()?, triangles, additive)));
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
        let sage = self.prims.sage.shapes(now).into_iter().filter_map(|(path, additive, [red, green, blue], corners)| {
            let texture = self.textures.get(path)?.as_ref()?;
            Some((texture, corners.into_iter().map(|(position, uv, alpha)| MeshVertex { position, uv, tint: [red, green, blue, alpha] }).collect(), additive))
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
                self.prims
                    .holes
                    .meshes(now, &position_of)
                    .into_iter()
                    .filter_map(|wall| Some((self.textures.get(wall.texture)?.as_ref()?, wall.vertices, wall.additive))),
            )
            .chain(
                self.prims
                    .mechanic
                    .meshes(now, |n| pick(0, n.saturating_sub(1)))
                    .into_iter()
                    .filter_map(|wall| Some((self.textures.get(wall.texture)?.as_ref()?, wall.vertices, wall.additive))),
            )
            .chain(
                self.prims
                    .radar
                    .meshes(now, &position_of, |n| pick(0, n.saturating_sub(1)))
                    .into_iter()
                    .filter_map(|wall| Some((self.textures.get(wall.texture)?.as_ref()?, wall.vertices, wall.additive))),
            )
            .chain(
                self.prims.spears.meshes(now).into_iter().filter_map(|wall| Some((self.textures.get(wall.texture)?.as_ref()?, wall.vertices, wall.additive))),
            )
            .chain(
                self.prims
                    .sphere
                    .meshes(now, self.yaw.to_degrees(), &position_of)
                    .into_iter()
                    .filter_map(|wall| Some((self.textures.get(wall.texture)?.as_ref()?, wall.vertices, wall.additive))),
            )
            .chain(
                self.prims.kicks.hit_meshes().into_iter().filter_map(|wall| Some((self.textures.get(wall.texture)?.as_ref()?, wall.vertices, wall.additive))),
            )
            .chain(
                self.prims
                    .kicks
                    .fin_meshes(&position_of)
                    .into_iter()
                    .filter_map(|wall| Some((self.textures.get(wall.texture)?.as_ref()?, wall.vertices, wall.additive))),
            )
            .chain(
                self.prims
                    .soul
                    .meshes(now, &position_of)
                    .into_iter()
                    .filter_map(|wall| Some((self.textures.get(wall.texture)?.as_ref()?, wall.vertices, wall.additive))),
            )
            .chain(self.prims.wheels.meshes().into_iter().filter_map(|wall| Some((self.textures.get(wall.texture)?.as_ref()?, wall.vertices, wall.additive))))
            .chain(
                self.prims.linker.cone_meshes().into_iter().filter_map(|wall| Some((self.textures.get(wall.texture)?.as_ref()?, wall.vertices, wall.additive))),
            )
            .chain(self.prims.linker.meshes().into_iter().filter_map(|wall| Some((self.textures.get(wall.texture)?.as_ref()?, wall.vertices, wall.additive))))
            .chain(
                self.prims
                    .kicks
                    .print_meshes(now)
                    .into_iter()
                    .filter_map(|wall| Some((self.textures.get(wall.texture)?.as_ref()?, wall.vertices, wall.additive))),
            )
            .chain(
                self.prims.portal.meshes(now).into_iter().filter_map(|wall| Some((self.textures.get(wall.texture)?.as_ref()?, wall.vertices, wall.additive))),
            )
            .chain(self.prims.cells.meshes(now).into_iter().filter_map(|wall| Some((self.textures.get(wall.texture)?.as_ref()?, wall.vertices, wall.additive))))
            .chain(
                self.prims
                    .chi
                    .meshes(now, &position_of)
                    .into_iter()
                    .filter_map(|wall| Some((self.textures.get(wall.texture)?.as_ref()?, wall.vertices, wall.additive))),
            )
            .chain(
                self.prims
                    .waves
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
            .chain(self.prims.vortex.meshes().into_iter().filter_map(|wall| Some((self.textures.get(wall.texture)?.as_ref()?, wall.vertices, wall.additive))))
            .chain(
                self.prims
                    .snow
                    .meshes(&position_of)
                    .into_iter()
                    .filter_map(|wall| Some((self.textures.get(wall.texture)?.as_ref()?, wall.vertices, wall.additive))),
            )
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
            let warmth = prim.effect == WARMTH_GLINTS;
            let first_alphas = if warmth { &WARMTH_ALPHAS } else { &GLINT_ALPHAS };
            let looks = glint_looks(prim.effect);
            for glint in 0..4 * glint_sets(prim.effect) {
                let (alpha, round) = glint_at(first_alphas[(glint % 4) as usize], age);
                // The warmth's glints fade in over their first ticks.
                let alpha = if warmth { alpha.min(age as i32) } else { alpha };
                let pick = |what: u32| noise(prim.seed, glint * 1_000 + round, what);
                let Some(Some(texture)) = self.textures.get(GLINT_TEXTURES[(pick(0) % 5) as usize]) else { continue };
                let (sin, cos) = ((pick(1) % 360) as f32).to_radians().sin_cos();
                let height = if round == 0 { (pick(2) % 10) as f32 } else { 3.0 + (pick(2) % 8) as f32 };
                let place = [at[0] + GLINT_CIRCLE * cos, at[1] + height + lift, at[2] + GLINT_CIRCLE * sin];
                for &(radius, color) in &looks {
                    let half = radius * std::f32::consts::FRAC_1_SQRT_2 * PIXELS_PER_UNIT;
                    let [red, green, blue] = color.map(|value| f32::from(value) / 255.0);
                    let corners = [[-half, -half], [half, -half], [half, half], [-half, half]];
                    quads.push((texture, place, corners, [red, green, blue, alpha as f32 / 255.0], true, false));
                }
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
        if let Some(Some(sprite)) = self.sprites.get(SPARK_SPRITE).filter(|_| self.shows(0)) {
            for hit in self.hits.iter().filter(|hit| sparks_shown(self.min, hit.dark)) {
                let (Some(paths), Some(at)) = (&hit.paths, hit.anchor.place(&position_of)) else { continue };
                let rgb = spark_tint(hit.dark);
                for (spark, path) in hit.sparks.iter().zip(paths) {
                    for (place, alpha, scale) in spark.trail(hit.tick(now), path) {
                        let anchor = std::array::from_fn(|axis| at[axis] + place[axis]);
                        for &(key, position, size, layer_angle) in &hit.placed {
                            let corners = QUAD_UV.map(|uv| world::sprite_corner(position, size, layer_angle, uv).0.map(|value| value * scale));
                            quads.push((sprite.texture(key), anchor, corners, [rgb[0], rgb[1], rgb[2], alpha], false, false));
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
            let frame = held_frame(animation, playing.hold, str_frame(now.duration_since(playing.start)) as f32);
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
        // The STR world copies: each its black fill (the enchant and rune screens) under its layers.
        for copy in self.worlds.iter().filter(|copy| self.shows(copy.id)) {
            if let (Some(alpha), Some(black)) = (copy.tint(), &self.black) {
                let reach = WORLD_TINT_REACH;
                quads.push((
                    black,
                    copy.position,
                    [[-reach, -reach], [reach, -reach], [reach, reach], [-reach, reach]],
                    [1.0, 1.0, 1.0, alpha as f32 / 255.0],
                    false,
                    true,
                ));
            }
            let Some(loaded) = &copy.loaded else { continue };
            for drawn in copy.drawn() {
                let Some(Some(texture)) = loaded.textures.get(drawn.layer).and_then(|layer| layer.get(drawn.texture)) else { continue };
                quads.push((texture, drawn.at, drawn.corners, drawn.color, drawn.additive, true));
            }
        }
        // Body Painting's strokes, over everything.
        quads.extend(self.prims.painter.quads(now, STR_RISE, |path| self.textures.get(path).and_then(Option::as_ref), &position_of));
        // Venom Impress' icon is force-drawn, over everything.
        quads.extend(self.prims.circles.venom_quads(now, STR_RISE, |path| self.textures.get(path).and_then(Option::as_ref), &position_of));
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
        quads.extend(self.prims.boomerangs.quads(|path| self.textures.get(path).and_then(Option::as_ref)));
        if !self.only_forced {
            quads.extend(self.prims.spheres.quads(self.textures.get(tanji::SPHERE).and_then(Option::as_ref)));
            // King's Grace's emblems: the glow added on, then the emblem, both facing the camera.
            if let Some(Some(texture)) = self.textures.get(emblems::EMBLEM) {
                quads.extend(self.prims.emblems.draws(now).into_iter().map(|draw| (texture, draw.at, draw.corners, draw.tint, draw.additive, false)));
            }
            // Soul Link's mote and the letters SOUL LINK, facing the camera.
            for draw in self.prims.soul.squares(self.yaw.to_degrees(), &position_of).into_iter().chain(self.prims.linker.squares(&position_of)) {
                if let Some(Some(texture)) = self.textures.get(draw.texture) {
                    let [red, green, blue] = draw.rgb;
                    quads.push((texture, draw.at, soul::Soul::corners(draw.half), [red, green, blue, draw.alpha / 255.0], draw.additive, false));
                }
            }
            // Estun's electric sparks, facing the camera.
            quads.extend(self.prims.linker.quads(now, |path| self.textures.get(path).and_then(Option::as_ref)));
            // Recognized Spell's skill icons and Tetra Vortex's cards, facing the camera.
            quads.extend(self.prims.vortex.quads(now, self.yaw.to_degrees(), |path| self.textures.get(path).and_then(Option::as_ref), &position_of));
            // Cold Slower's snowflakes, facing the camera.
            quads.extend(self.prims.snow.quads(now, |path| self.textures.get(path).and_then(Option::as_ref)));
            // Lope's motes and petals, facing the camera.
            quads.extend(self.prims.petals.quads(|path| self.textures.get(path).and_then(Option::as_ref)));
            // Lullaby Deep Sleep's notes, over everything (`STR_RISE` is taken off again).
            quads.extend(self.prims.notes.quads(|path| self.textures.get(path).and_then(Option::as_ref), STR_RISE));
            // Sight Rasher's sprites and their shadows, white with their alpha.
            for drawn in self.prims.sights.sprites(now).into_iter().chain(self.prims.cells.flames(now, &position_of)) {
                let Some(Some(sprite)) = self.sprites.get(drawn.sprite) else { continue };
                for &(key, position, size, layer_angle) in self.sight_frames.get(&(drawn.sprite, drawn.frame)).into_iter().flatten() {
                    let corners = QUAD_UV.map(|uv| world::sprite_corner(position, size, layer_angle, uv).0.map(|value| value * drawn.scale));
                    quads.push((sprite.texture(key), drawn.at, corners, [1.0, 1.0, 1.0, drawn.alpha.trunc().clamp(0.0, 255.0) / 255.0], false, false));
                }
            }
            // Dead Hill Here's and Dance with Warg's monsters stand like units, each with a glowing outline a pixel bigger on every side.
            for draw in self.prims.crowds.draws(now).into_iter().chain(self.prims.cells.ifrits(now)).chain(self.prims.petals.falls()) {
                let Some(Some(sprite)) = self.sprites.get(draw.sprite) else { continue };
                let frames = sprite.frames(draw.action).max(1);
                let frame = draw.steps as usize % frames;
                for &(key, position, size, layer_angle) in self.crowd_frames.get(&(draw.sprite, draw.action, frame)).into_iter().flatten() {
                    let corners = QUAD_UV.map(|uv| world::sprite_corner(position, size, layer_angle, uv).0.map(|value| value * draw.scale));
                    let tint = [draw.tint[0], draw.tint[1], draw.tint[2], draw.alpha / 255.0];
                    quads.push((sprite.texture(key), draw.at, corners, tint, false, false));
                    let (low, high) = corners.iter().fold(([f32::MAX; 2], [f32::MIN; 2]), |(low, high), corner| {
                        ([low[0].min(corner[0]), low[1].min(corner[1])], [high[0].max(corner[0]), high[1].max(corner[1])])
                    });
                    let middle = [(low[0] + high[0]) / 2.0, (low[1] + high[1]) / 2.0];
                    let grown = corners.map(|[x, y]| [x + if x <= middle[0] { -1.0 } else { 1.0 }, y + if y <= middle[1] { -1.0 } else { 1.0 }]);
                    quads.push((sprite.texture(key), draw.at, grown, tint, true, false));
                }
            }
            // Wall of Thorns' tree and Vacuum Extreme's whirlwinds: the sprite in the prim's colour and alpha on every layer, added on.
            for draw in self.prims.cells.stands(now) {
                let Some(Some(sprite)) = self.sprites.get(draw.sprite) else { continue };
                let frame = draw.steps as usize % sprite.frames(draw.action).max(1);
                for &((key, position, size, layer_angle), _) in self.cell_frames.get(&(draw.sprite, draw.action, frame)).into_iter().flatten() {
                    let corners = QUAD_UV.map(|uv| world::sprite_corner(position, size, layer_angle, uv).0.map(|value| value * draw.scale));
                    let [red, green, blue] = draw.tint;
                    quads.push((sprite.texture(key), draw.at, corners, [red, green, blue, draw.alpha / 255.0], true, false));
                }
            }
            // Poison Buster's blasts: each layer in #3737FF at 0.85 of its alpha, and a halo 4 pixels bigger on every side at 0.5.
            if let Some(Some(sprite)) = self.sprites.get(cells::BLAST) {
                for draw in self.prims.cells.blasts(now, &position_of) {
                    for &((key, position, size, layer_angle), alpha) in self.cell_frames.get(&(cells::BLAST, 0, draw.frame)).into_iter().flatten() {
                        let corners = QUAD_UV.map(|uv| world::sprite_corner(position, size, layer_angle, uv).0);
                        let (low, high) = corners.iter().fold(([f32::MAX; 2], [f32::MIN; 2]), |(low, high), corner| {
                            ([low[0].min(corner[0]), low[1].min(corner[1])], [high[0].max(corner[0]), high[1].max(corner[1])])
                        });
                        let middle = [(low[0] + high[0]) / 2.0, (low[1] + high[1]) / 2.0];
                        let grown = corners.map(|[x, y]| [x + if x <= middle[0] { -4.0 } else { 4.0 }, y + if y <= middle[1] { -4.0 } else { 4.0 }]);
                        let (body, halo) = cells::blast_alphas(alpha);
                        quads.push((sprite.texture(key), draw.at, corners, [55.0 / 255.0, 55.0 / 255.0, 1.0, body / 255.0], true, false));
                        quads.push((sprite.texture(key), draw.at, grown, [55.0 / 255.0, 55.0 / 255.0, 1.0, halo / 255.0], true, false));
                    }
                }
            }
            // Boost Knuckle's fists: the first as the sprite colours it, the three after it white ghosts.
            if let Some(Some(sprite)) = self.sprites.get(balls::FIST_SPRITE) {
                for draw in self.prims.balls.fist_draws() {
                    let tint = [1.0, 1.0, 1.0, draw.ghost.map_or(1.0, |alpha| alpha / 255.0)];
                    for &(key, position, size, layer_angle) in self.fist_frames.get(&(draw.action, draw.frame, draw.ghost.is_some())).into_iter().flatten() {
                        let corners = QUAD_UV.map(|uv| world::sprite_corner(position, size, layer_angle, uv).0);
                        quads.push((sprite.texture(key), draw.at, corners, tint, false, false));
                    }
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
            // Sienna Execrate: bouncing Stapo at target's feet + 8 units up, over everything.
            for stapo in &self.stapos {
                let Some(at) = stapo.anchor.place(&position_of) else { continue };
                let Some(Some(sprite)) = self.sprites.get(STAPO_SPRITE) else { continue };
                let at = [at[0], at[1] + 8.0, at[2]];
                for &(key, position, size, layer_angle) in &stapo.placed {
                    let corners = QUAD_UV.map(|uv| {
                        let [left, top] = world::sprite_corner(position, size, layer_angle, uv).0;
                        [left, top + STR_RISE]
                    });
                    quads.push((sprite.texture(key), at, corners, [1.0; 4], false, true));
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
            // Harmonize's and Arrullo's soul lights: soft squares facing the camera, added on.
            if let Some(Some(texture)) = self.textures.get(souls::DOT) {
                for dot in self.prims.souls.dots(&position_of) {
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
        quads.extend(self.prims.crowds.quads(now, |path| self.textures.get(path).and_then(Option::as_ref), &position_of, |n| pick(0, n.saturating_sub(1))));
        quads.extend(self.prims.waves.quads(now, |path| self.textures.get(path).and_then(Option::as_ref), &position_of));
        quads.extend(self.prims.circles.mark_quads(now, |path| self.textures.get(path).and_then(Option::as_ref), &position_of));
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
        .map(|layer| {
            layer
                .textures
                .iter()
                .map(|name| {
                    // A name without an extension is a .bmp.
                    let extension = if name.contains('.') { "" } else { ".bmp" };
                    let path = format!("data\\texture\\effect\\{folder}{name}{extension}");
                    let image = crate::render::load_image(grf, &path).ok()?;
                    Some(gpu.texture(&crate::render::key_out_black(image, path.to_lowercase().ends_with(".tga")), true))
                })
                .collect()
        })
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
    fn cannon_spear_picks_its_blade_by_the_screen_direction() {
        let phis = [0, 22, 23, 66, 67, 111, 112, 156, 157, 201, 202, 246, 247, 291, 292, 336, 337, 359];
        let ids = [1404, 1404, 1403, 1403, 1402, 1402, 1401, 1401, 1400, 1400, 1407, 1407, 1406, 1406, 1405, 1405, 1404, 1404];
        for (phi, id) in phis.into_iter().zip(ids) {
            assert_eq!(cannon_sector(phi), Some(id), "{phi}");
        }
        assert_eq!(cannon_sector(-1), None);
        // By the sprite direction the caster shows: 0 faces the camera, 4 away, 6 to the screen's right, 2 to its left.
        for yaw in [0.0f32, 45.0, -30.0, 200.0] {
            for (direction, id) in [(0, 1404), (4, 1400), (6, 1402), (2, 1406)] {
                let rot = (-yaw - 45.0 * direction as f32).rem_euclid(360.0);
                assert_eq!(crate::actor::view_direction(rot, yaw, true), direction);
                assert_eq!(cannon_sector(screen_phi(yaw, rot)), Some(id), "yaw {yaw}, direction {direction}");
            }
        }
    }

    #[test]
    fn world_ticks_keep_pace_with_16_ms_frames() {
        let start = Instant::now();
        let mut clock = start;
        let mut ticks = 0;
        for frame in 1..=100 {
            let now = start + Duration::from_millis(16 * frame);
            let due = world_ticks(now - clock);
            clock += WORLD_TICK * due;
            ticks += due;
        }
        assert_eq!(ticks, 100);
        assert_eq!(world_ticks(Duration::from_micros(15_999)), 0);
    }

    #[test]
    fn cannon_spears_blade_waits_for_the_swing() {
        assert_eq!([4.0, 1.0, 2.5].map(cannon_delay_ms), [500, 200, 350]);
        let mut timeline = world_str::Timeline::default();
        timeline.delay = 500;
        let first = (0..100).position(|_| matches!(timeline.step(40), world_str::Step::Show(_)));
        assert_eq!(first, Some(32));
    }

    /// An in-memory STR whose last frame is `last`.
    fn frames(last: i32) -> Rc<Loaded> {
        let key = |frame| rustnarok_client::effect::Key {
            frame,
            kind: 0,
            position: [320.0, 240.0],
            corners: [0.0; 8],
            texture: 0.0,
            texture_motion: 0,
            delay: 0.0,
            angle: 0.0,
            color: [1.0; 4],
            source_blend: 5,
            destination_blend: 2,
        };
        let layer = rustnarok_client::effect::Layer { textures: vec!["a.bmp".into()], keys: vec![key(0), key(last)] };
        Rc::new(Loaded { animation: Str { fps: 60, frames: last as u32, layers: vec![layer] }, textures: vec![Vec::new()] })
    }

    /// The last frame of each Cannon Spear file: the blades 40 (20 lighter), the vortices 30.
    fn cannon_last(id: u32) -> i32 {
        match id {
            1400..=1407 => 40,
            1511..=1518 => 20,
            _ => 30,
        }
    }

    /// Plays one Cannon Spear packet from `caster` into `worlds`, as `Effects::cannon_spear` does.
    fn cannon_packet(worlds: &mut Worlds, table: &Table, min: bool, caster: u32, phi: i32) -> Vec<(u32, i32)> {
        let owner = Some(Some(caster));
        let launches = cannon_launches(table, min, phi, 500, |id| worlds.playing(owner, id));
        for &(id, delay) in &launches {
            let launch = world_str::Launch { id, owner, point: [0.0; 3], unique: false, life: 0, min, roll: 0 };
            let serial = worlds.launch(table.world(id).unwrap(), &launch, |_| Some(frames(cannon_last(id)))).unwrap();
            worlds.get_mut(serial).unwrap().timeline.delay = delay;
        }
        launches
    }

    fn cannon_ticks(worlds: &mut Worlds, count: usize, units: &HashMap<u32, Vector>) {
        let feet = |unit: Option<u32>| unit.map_or(Some([0.0; 3]), |id| units.get(&id).copied());
        let body = |_: Option<u32>| None;
        let scene =
            world_str::Scene { player: [0.0; 3], feet: &feet, body: &body, longitude: 0.0, right: [1.0, 0.0, 0.0], up: [0.0, 1.0, 0.0], ground: &|_, _| 0.0 };
        for _ in 0..count {
            worlds.tick(&scene);
        }
    }

    #[test]
    fn cannon_spear_starts_its_vortex_and_blade_once_a_vortex() {
        let table = Table::load();
        let mut worlds = Worlds::default();
        let units = HashMap::from([(7, [10.0, 0.0, 10.0]), (8, [20.0, 0.0, 20.0])]);
        assert_eq!(cannon_packet(&mut worlds, &table, false, 7, 0), [(1408, 0), (1404, 500)]);
        // While the vortex plays (its 31 ticks), another packet starts nothing.
        cannon_ticks(&mut worlds, 31, &units);
        assert_eq!(cannon_packet(&mut worlds, &table, false, 7, 180), []);
        // Another caster is independent.
        assert_eq!(cannon_packet(&mut worlds, &table, false, 8, 90), [(1408, 0), (1402, 500)]);
        // Both follow their caster.
        assert!(worlds.iter().filter(|copy| copy.owner == Some(Some(8))).all(|copy| copy.position == [0.0; 3]));
        cannon_ticks(&mut worlds, 1, &units);
        assert!(worlds.iter().filter(|copy| copy.owner == Some(Some(8))).all(|copy| copy.position == [20.0, 0.0, 20.0]));
        // After them, a packet starts both again.
        assert_eq!(cannon_packet(&mut worlds, &table, false, 7, 270), [(1408, 0), (1406, 500)]);
        // No blade for a negative direction.
        let mut fresh = Worlds::default();
        assert_eq!(cannon_packet(&mut fresh, &table, false, 7, -5), [(1408, 0)]);
    }

    #[test]
    fn dragon_breath_picks_its_quadrant_by_the_screen_direction() {
        let phis = [0, 1, 89, 90, 179, 180, 269, 270, 359];
        let fire = [1382, 1381, 1381, 1380, 1380, 1383, 1383, 1382, 1382];
        for (phi, id) in phis.into_iter().zip(fire) {
            assert_eq!(breath_sector(phi, false), id, "fire {phi}");
            assert_eq!(breath_sector(phi, true), id + 8, "water {phi}");
        }
        assert_eq!(breath_sector(-5, false), 1382);
        // A caster facing the camera, at any camera turn.
        for yaw in [0.0f32, 45.0, -30.0, 200.0] {
            let rot = (-yaw).rem_euclid(360.0);
            assert_eq!(crate::actor::view_direction(rot, yaw, true), 0);
            assert_eq!((breath_sector(screen_phi(yaw, rot), false), breath_sector(screen_phi(yaw, rot), true)), (1382, 1390));
        }
        // The camera angle and the facing both count, and the sum wraps at 360.
        assert_eq!(screen_phi(200.0, 200.0), 40);
        assert_eq!(breath_sector(screen_phi(200.0, 200.0), false), 1381);
        assert_eq!(screen_phi(-90.0, 0.0), 270);
    }

    /// The last frame of each Dragon Breath file: the breaths 50, the bursts 40 (35 lighter), the patches 54.
    fn breath_last(id: u32) -> i32 {
        match id {
            1378 | 1386 => 40,
            1379 | 1387 => 54,
            1520 | 1526 => 35,
            _ => 50,
        }
    }

    /// Plays one Dragon Breath packet of `skill` from `caster` on `target` into `worlds`, as `Effects::dragon_breath` does.
    fn breath_packet(worlds: &mut Worlds, table: &Table, min: bool, skill: u16, caster: u32, target: u32, phi: Option<i32>) -> Vec<(u32, Who, i32)> {
        let launches = breath_launches(table, min, skill, phi, |id| worlds.playing(Some(Some(caster)), id));
        for &(id, who, delay) in &launches {
            let owner = Some(Some(if who == Who::Caster { caster } else { target }));
            let launch = world_str::Launch { id, owner, point: [0.0; 3], unique: false, life: 0, min, roll: 0 };
            let serial = worlds.launch(table.world(id).unwrap(), &launch, |_| Some(frames(breath_last(id)))).unwrap();
            worlds.get_mut(serial).unwrap().timeline.delay = delay;
        }
        launches
    }

    #[test]
    fn dragon_breath_starts_its_breath_once_a_direction_and_a_burst_on_every_target() {
        let table = Table::load();
        let mut worlds = Worlds::default();
        let units = HashMap::from([(7, [10.0, 0.0, 10.0]), (8, [20.0, 0.0, 20.0]), (9, [30.0, 0.0, 30.0]), (5, [0.0; 3])]);
        let (caster, target) = (Who::Caster, Who::Target);
        assert_eq!(breath_packet(&mut worlds, &table, false, 2008, 7, 8, Some(0)), [(1382, caster, 200), (1378, target, 350), (1379, target, 350)]);
        // Another target while the breath runs: only its burst.
        assert_eq!(breath_packet(&mut worlds, &table, false, 2008, 7, 9, Some(0)), [(1378, target, 350), (1379, target, 350)]);
        // Another direction starts that direction's breath.
        assert_eq!(breath_packet(&mut worlds, &table, false, 2008, 7, 9, Some(180)), [(1383, caster, 200), (1378, target, 350), (1379, target, 350)]);
        // Another caster is independent; the monster skill plays fire, Water Dragon Breath water.
        assert_eq!(breath_packet(&mut worlds, &table, false, 731, 5, 8, Some(0))[0], (1382, caster, 200));
        assert_eq!(breath_packet(&mut worlds, &table, false, 5004, 5, 8, Some(45)), [(1389, caster, 200), (1386, target, 350), (1387, target, 350)]);
        // No known caster: nothing.
        assert!(breath_packet(&mut worlds, &table, false, 2008, 6, 8, None).is_empty());
        // Each follows its unit.
        cannon_ticks(&mut worlds, 1, &units);
        assert!(worlds.iter().filter(|copy| copy.id == 1382 && copy.owner == Some(Some(7))).all(|copy| copy.position == [10.0, 0.0, 10.0]));
        assert!(worlds.iter().filter(|copy| copy.id == 1378).all(|copy| copy.position == units[&copy.owner.unwrap().unwrap()]));
        // The breath plays to tick 64; from tick 65 a packet starts it again.
        cannon_ticks(&mut worlds, 63, &units);
        assert!(breath_packet(&mut worlds, &table, false, 2008, 7, 8, Some(0)).iter().all(|launch| launch.1 == target));
        cannon_ticks(&mut worlds, 1, &units);
        assert_eq!(breath_packet(&mut worlds, &table, false, 2008, 7, 8, Some(0))[0], (1382, caster, 200));
    }

    #[test]
    fn dragon_breath_under_mineffect() {
        let table = Table::load();
        for (id, min) in
            [(1380, 1521), (1381, 1522), (1382, 1523), (1383, 1524), (1388, 1527), (1389, 1528), (1390, 1529), (1391, 1530), (1378, 1520), (1386, 1526)]
        {
            assert_eq!(table.min_id(id, true), min, "{id}");
        }
        assert_eq!((table.min_id(1379, true), table.min_id(1387, true)), (1379, 1387));
        let mut worlds = Worlds::default();
        let (caster, target) = (Who::Caster, Who::Target);
        assert_eq!(breath_packet(&mut worlds, &table, true, 2008, 7, 8, Some(100)), [(1521, caster, 200), (1520, target, 350)]);
        assert_eq!(breath_packet(&mut worlds, &table, true, 5004, 5, 8, Some(300)), [(1529, caster, 200), (1526, target, 350)]);
        // The running 1521 holds the next breath back; the normal 1380 is not what is looked for.
        assert_eq!(breath_packet(&mut worlds, &table, true, 2008, 7, 9, Some(100)), [(1520, target, 350)]);
        assert!(!worlds.playing(Some(Some(7)), 1380));
    }

    #[test]
    fn dragon_breaths_pictures_wait_and_end() {
        // The first drawn step and the tick the copy is over, for a start delay.
        let steps = |last: i32, delay: i32| {
            let mut timeline = world_str::Timeline::default();
            timeline.delay = delay;
            let run: Vec<world_str::Step> = (0..200).map(|_| timeline.step(last)).collect();
            let first = run.iter().position(|step| matches!(step, world_str::Step::Show(_))).unwrap();
            let over = run.iter().position(|step| *step == world_str::Step::Over).unwrap();
            (first, over)
        };
        assert_eq!(steps(breath_last(1380), 200), (13, 64));
        assert_eq!(steps(breath_last(1521), 200), (13, 64));
        // The bursts draw frames 0 … 40, 0 … 54 and 0 … 35 after their 22 held ticks.
        assert_eq!(steps(breath_last(1378), 350), (22, 22 + 41));
        assert_eq!(steps(breath_last(1379), 350), (22, 22 + 55));
        assert_eq!(steps(breath_last(1520), 350), (22, 22 + 36));
    }

    #[test]
    fn dragon_breaths_sound_and_rows() {
        let now = Instant::now();
        let mut effects = Effects::default();
        effects.queue_breath_sound(now);
        assert_eq!(effects.sounds.len(), 1);
        let (when, file, at) = &effects.sounds[0];
        assert!(*when == now && file == "effect\\폭염룡.wav" && at.iter().all(|value| value.is_nan()));
        // The old rows stay as they are.
        let table = Table::load();
        assert_eq!(table.skill(2008, "hitEffectId"), [587]);
        for skill in DRAGON_BREATHS {
            assert_eq!(table.caster(skill), None, "{skill}");
        }
    }

    #[test]
    fn cannon_spear_under_mineffect() {
        let table = Table::load();
        assert_eq!(table.min_id(1408, true), 1519);
        assert_eq!(table.min_id(1408, false), 1408);
        for id in 1400..=1407 {
            assert_eq!(table.min_id(id, true), id + 111);
        }
        let mut worlds = Worlds::default();
        assert_eq!(cannon_packet(&mut worlds, &table, true, 7, 0), [(1519, 0), (1515, 500)]);
        // The running 1519 holds the next packet back.
        assert_eq!(cannon_packet(&mut worlds, &table, true, 7, 0), []);
        assert!(!worlds.playing(Some(Some(7)), 1408));
    }

    #[test]
    fn cannon_spears_pictures_end_and_sound() {
        let ends = |last: i32, delay: i32| {
            let mut timeline = world_str::Timeline::default();
            timeline.delay = delay;
            (0..200).position(|_| timeline.step(last) == world_str::Step::Over).unwrap()
        };
        assert_eq!([cannon_last(1400), cannon_last(1408), cannon_last(1511), cannon_last(1519)].map(|last| ends(last, 0)), [41, 31, 21, 31]);
        // A blade with the swing's delay ends that much later.
        assert_eq!(ends(cannon_last(1404), 500), 41 + 32);
        let table = Table::load();
        for id in [1408, 1519] {
            let sounds = &table.world(id).unwrap().events;
            assert_eq!(sounds, &[world_str::Event::Sound(10, "effect\\EF_hit2.wav".into()), world_str::Event::Sound(20, "effect\\EF_hit4.wav".into())]);
        }
    }

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
        assert_eq!(table.casts.len(), 776);
        assert!(include_str!("effects.txt").lines().filter(|line| line.starts_with("cast\t")).count() == 781);
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
        // Wall of Thorns shows 912, started again while it stands: its tree is drawn in code, with no STR.
        assert_eq!(table.units.get(&0xe6), Some(&912));
        assert!(!table.units_once.contains(&0xe6) && !table.parts.contains_key(&912));
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
        assert_eq!((restart_ticks(25, 0, 40), restart_ticks(95, 0, 150)), (32, 130));
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
        // (Electric Walk's wave, 221, loops every 30 ticks now.)
        assert!([130, 136].iter().all(|kind| table.units_once.contains(kind)) && !table.units_once.contains(&221));
    }

    #[test]
    fn the_heal_objects_have_their_sounds_and_stay_out_of_a_skills_heal() {
        let table = Table::load();
        let sound = |id: u32| table.sounds.get(&id).map(|sounds| sounds.iter().map(|sound| (sound.file.as_str(), sound.delay_ms)).collect::<Vec<_>>());
        assert_eq!(sound(253), Some(vec![("effect\\흡기", 0)]));
        for id in [312, 313, 320, 325] {
            assert_eq!(sound(id), Some(vec![("_heal_effect", 0)]), "{id}");
        }
        assert_eq!((sound(1143), sound(1144)), (None, None));
        // The object is registered for all seven ids; `play` leaves them to the packet's own entry, so a skill's heal (Banded,
        // `heal_effect`) goes on through the table.
        let fx = WorldFx::default();
        for id in [253, 312, 313, 320, 325, 1143, 1144] {
            assert!(fx.registered(id) && world_fx::heal::is_class(id), "{id}");
        }
        assert!(!world_fx::heal::is_class(311));
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
        let shape = |age: u32| bloom(12, 1, 3, 1000, age);
        assert_eq!(shape(8), None);
        let first = shape(9).unwrap();
        assert_eq!((first.radius, first.lean, first.alpha), (3.44, 73.0, 10.0));
        let top = shape(17).unwrap();
        assert_eq!((top.radius, top.alpha), (4.0, 90.0));
        assert_eq!(shape(18).unwrap().alpha, 87.0);
        assert_eq!(shape(47), None);
        assert_eq!(shape(48), Some(first));
        // Within 30 ticks of the aura's end a band that fades out stays gone: the start at 47 plays out to 86.
        assert!(bloom(12, 1, 3, 100, 85).is_some() && (86..100).all(|age| bloom(12, 1, 3, 100, age).is_none()));
        // A short aura: every band from nothing, band k 5k ticks late.
        assert_eq!((bloom(12, 0, 2, 56, 9), bloom(12, 0, 2, 56, 10)), (None, None));
        assert_eq!(bloom(12, 0, 2, 56, 11).unwrap().alpha, 10.0);
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
        let hit = HitSpark {
            start: time,
            anchor: Anchor::Unit(None),
            way: [0.0, 1.0],
            sparks: sparks([0.0, 1.0], |from, _| from),
            paths: None,
            placed: Vec::new(),
            dark: false,
        };
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
        effects.hits.push(HitSpark {
            start: time,
            anchor: Anchor::Unit(Some(7)),
            way,
            sparks: sparks(way, |from, _| from),
            paths: None,
            placed: Vec::new(),
            dark: false,
        });
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
        // Chase Walk's footprints are drawn in code, with no part rows.
        assert!(!table.floaters.contains_key(&326) && !table.floaters.contains_key(&327));
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
        assert_eq!((restart_ticks(145, 148, 80), restart_ticks(145, 216, 200), restart_ticks(25, 0, 40)), (75, 75, 32));
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

    #[test]
    fn infrared_scan_has_its_sound_at_the_listener_and_starts_with_its_damage_packet() {
        let table = Table::load();
        let sounds = |id: u32| table.sounds[&id].iter().map(|sound| (sound.file.as_str(), sound.delay_ms, sound.listener)).collect::<Vec<_>>();
        assert_eq!(sounds(INFRARED_SCAN), [("effect\\INFRAREDSCAN", 0, true)]);
        assert!(table.struck(2270).unwrap().contains(&(Who::Caster, INFRARED_SCAN)));
        assert!(!force_drawn(INFRARED_SCAN));
    }

    #[test]
    fn infrared_scan_magnetic_field_neutral_barrier_the_cells_analyze_and_venom_impress_have_their_rows() {
        let table = Table::load();
        let sounds = |id: u32| table.sounds[&id].iter().map(|sound| (sound.file.as_str(), sound.delay_ms, sound.listener)).collect::<Vec<_>>();
        assert_eq!(sounds(first::RED_RING), [("effect\\cru_grand cross", 0, false)]);
        assert_eq!(sounds(MAGNETIC_FIELD), [("effect\\MAGNETICFIELD", 80, false)]);
        assert_eq!(sounds(NEUTRAL_BARRIER), [("effect\\black_maximize_power_sword_bic", 0, false), ("effect\\mon_금강불괴", 0, true)]);
        assert_eq!(sounds(ANALYZE), [("effect\\ANALYZE", 0, false)]);
        assert!(!table.sounds.contains_key(&BARRIER_CELL) && !table.sounds.contains_key(&STEALTH_CELL) && !table.sounds.contains_key(&REPAIR));
        assert!(table.struck(2270).unwrap().contains(&(Who::Caster, first::RED_RING)) && table.struck(2272).unwrap().contains(&(Who::Caster, MAGNETIC_FIELD)));
        assert!(table.used(2273).unwrap().contains(&(Who::Caster, NEUTRAL_BARRIER)) && table.used(2275).unwrap().contains(&(Who::Target, REPAIR)));
        assert!(table.struck(2271).unwrap().contains(&(Who::Target, ANALYZE)));
        assert_eq!((table.units[&226], table.units[&227]), (BARRIER_CELL, STEALTH_CELL));
        assert!(table.units_once.contains(&226) && table.units_once.contains(&227));
        assert!(table.status(328).contains(&VENOM_ICON) && VENOM_STATUS == 328);
    }

    #[test]
    fn tetra_vortex_boost_knuckle_shadow_form_body_painting_deadly_infect_and_the_debuffs_have_their_rows() {
        let table = Table::load();
        let sounds = |id: u32| table.sounds[&id].iter().map(|sound| (sound.file.as_str(), sound.delay_ms, sound.listener)).collect::<Vec<_>>();
        // The cast's colour by the aura's age α = 1, 24, 25, 49, 50, 74, 75, 99, 100 (the age + 1).
        let colour = |alpha: u32| aura_colour(TETRA_CASTING, 0, alpha - 1);
        let (blue, red, green, yellow) = ([0x64, 0x64, 0xFF], [0xFF, 0x64, 0x64], [0x64, 0xFF, 0x64], [0xFF, 0xFF, 0xAA]);
        assert_eq!([1, 24, 25, 49, 50, 74, 75, 99, 100].map(colour), [blue, blue, red, red, green, green, yellow, yellow, blue]);
        assert_eq!(aura_colour(58, 0, 40), [255; 3]);
        // A cast of 1 000 ms lasts 62 ticks and one of 400 ms 56; one at a time, like the others.
        assert_eq!((aura_ticks(TETRA_CASTING, 1000), aura_ticks(TETRA_CASTING, 400)), (62, 56));
        assert!(is_cast_aura(TETRA_CASTING) && is_banded(TETRA_CASTING) && table.cast_aura(2217, 12) == Some(TETRA_CASTING));
        assert_eq!(sounds(TETRA_CASTING), [("effect\\EF_BeginSpell", 0, true)]);
        // Boost Knuckle: launched on the caster at the target, its sound at the listener.
        assert!(AIMED.contains(&BOOST_KNUCKLE) && table.struck(2256).unwrap().contains(&(Who::Caster, BOOST_KNUCKLE)));
        assert_eq!(sounds(BOOST_KNUCKLE), [("effect\\맹호경파산", 0, true)]);
        // Shadow Form on its status with the cloaking sound; Body Painting's sound at the listener; Deadly Infect's too.
        assert!(table.status(394).contains(&SHADOW_FORM));
        assert_eq!(sounds(SHADOW_FORM), [("effect\\assasin_cloaking", 0, false)]);
        assert_eq!(sounds(BODY_PAINT), [("effect\\bodypaint", 0, true)]);
        assert!(table.used(2289).unwrap().contains(&(Who::Caster, BODY_PAINT)) && table.used(2291).unwrap().contains(&(Who::Caster, DEADLY_INFECT)));
        assert_eq!(sounds(DEADLY_INFECT), [("effect\\deadlyinfect", 0, true)]);
        // The three debuffs' sounds at the listener, on the targets they already have.
        for (effect, skill, file) in [(813, 2292, "effect\\enervation"), (814, 2293, "effect\\groomy"), (815, 2294, "effect\\ignorance")] {
            assert_eq!(sounds(effect), [(file, 0, true)]);
            assert!(table.used(skill).unwrap().contains(&(Who::Target, effect)));
        }
    }

    #[test]
    fn the_masquerade_debuffs_shadow_form_strip_accessory_manhole_and_dimension_door_have_their_rows() {
        let table = Table::load();
        let sounds = |id: u32| table.sounds[&id].iter().map(|sound| (sound.file.as_str(), sound.delay_ms, sound.listener)).collect::<Vec<_>>();
        // The three debuffs' sounds at the listener at tick 0.
        for (effect, file) in [(816, "effect\\laziness"), (817, "effect\\unlucky"), (818, "effect\\weakness")] {
            assert_eq!(sounds(effect), [(file, 0, true)]);
        }
        // Shadow Form's tube is launched from the source at the target; Strip Accessory's four sounds.
        assert!(AIMED.contains(&SHADOW_LINK) && table.used(2287).unwrap().contains(&(Who::Caster, SHADOW_LINK)));
        assert!(!table.sounds.contains_key(&SHADOW_LINK));
        assert_eq!(sounds(STRIP_ACCESSORY), [16, 176, 336, 496].map(|ms| ("effect\\복호격", ms, false)));
        assert!(table.used(2298).unwrap().contains(&(Who::Target, STRIP_ACCESSORY)));
        // Manhole: no sound on its cells, none of the skill's own rows; the body's sound at the listener.
        assert!(!table.sounds.contains_key(&MANHOLE_CELL));
        assert!(
            table.skill(2299, "groundEffectId").is_empty() && table.skill(2299, "successEffectId").is_empty() && table.skill(2300, "groundEffectId").is_empty()
        );
        assert_eq!(sounds(MANHOLE_BODY), [("effect\\manhole", 0, true)]);
        assert!(table.status(422).contains(&MANHOLE_BODY));
        assert_eq!((table.units[&204], table.units[&205]), (MANHOLE_CELL, DIMENSION_CELL));
        assert!(table.units_once.contains(&204) && !table.units_once.contains(&205));
        // Dimension Door: its sound once at tick 1 at the listener.
        assert_eq!(sounds(DIMENSION_CELL), [("effect\\dimension", 16, true)]);
    }

    #[test]
    fn shadow_form_the_cells_and_the_bubble_casts_have_their_rows() {
        let table = Table::load();
        let sounds = |id: u32| table.sounds[&id].iter().map(|sound| (sound.file.as_str(), sound.delay_ms, sound.listener)).collect::<Vec<_>>();
        // Shadow Form's aura: dark grey, blended; a cast of 40 ticks gives 56, the skill packet's 60; its sound is not a `wav` row.
        assert_eq!(aura_look(SHADOW_CAST), ("ring_black", [0x32; 3], false));
        assert!(is_cast_aura(SHADOW_CAST) && is_banded(SHADOW_CAST) && table.cast_aura(2287, 12) == Some(SHADOW_CAST));
        assert_eq!((aura_ticks(SHADOW_CAST, 640), aura_ticks(SHADOW_CAST, 960)), (56, 60));
        assert!(!table.sounds.contains_key(&SHADOW_CAST));
        assert!(table.used(2287).unwrap().contains(&(Who::Caster, SHADOW_CAST)));
        // The cells: their units keep them, the skills' own rows are gone, the sounds stay.
        for (unit, effect, skill) in [(206, CHAOS_CELL, 2301), (207, MAELSTROM_CELL, 2302), (208, BLOODY_CELL, 2303)] {
            assert_eq!(table.units[&unit], effect);
            assert!(!table.units_once.contains(&unit) && table.skill(skill, "groundEffectId").is_empty());
            assert!(table.sounds.contains_key(&effect));
        }
        // The Arch Bishop's casts: at least 100 ticks, two sets, the sound at tick 1 at the listener.
        assert_eq!((aura_ticks(BUBBLE_CAST, 400), aura_ticks(BUBBLE_CAST, 3000)), (100, 187));
        assert!(is_cast_aura(BUBBLE_CAST) && aura_sets(BUBBLE_CAST) == 2);
        assert_eq!(sounds(BUBBLE_CAST), [("effect\\EF_BeginSpell", 16, true)]);
        assert!([2041, 2042, 2045, 2047, 2048, 5072, 2322].iter().all(|&skill| table.cast_aura(skill, 12) == Some(BUBBLE_CAST)));
        // The Sura's: an ordinary effect on the caster for the cast, with the begin-spell sound at tick 0.
        assert!([2326, 2327, 2328, 2329, 2330, 2518].iter().all(|&skill| table.cast_aura(skill, 12) == Some(SURA_CAST)));
        assert_eq!(sounds(SURA_CAST), [("effect\\EF_BeginSpell", 0, false)]);
        assert!(!is_cast_aura(SURA_CAST));
    }

    #[test]
    fn sonic_wave_and_duple_light_have_their_rows() {
        let table = Table::load();
        let sounds = |id: u32| table.sounds[&id].iter().map(|sound| (sound.file.as_str(), sound.delay_ms, sound.listener)).collect::<Vec<_>>();
        // Sonic Wave: launched on the caster at the target's feet, its sound at the listener.
        assert!(AIMED.contains(&SONIC_WAVE) && table.struck(2002).unwrap().contains(&(Who::Caster, SONIC_WAVE)));
        assert_eq!(sounds(SONIC_WAVE), [("effect\\기공포", 0, true)]);
        // Duple Light: on the status and on the skill's target; its sound at the listener.
        assert!(table.status(350).contains(&DUPLE_LIGHT) && table.used(2054).unwrap().contains(&(Who::Target, DUPLE_LIGHT)));
        assert_eq!(sounds(DUPLE_LIGHT), [("effect\\AB_duplelight", 0, true)]);
    }

    #[test]
    fn hundred_spear_swing_dance_harmonize_and_great_echo_have_their_rows() {
        let table = Table::load();
        let sounds = |id: u32| table.sounds[&id].iter().map(|sound| (sound.file.as_str(), sound.delay_ms, sound.listener)).collect::<Vec<_>>();
        // Hundred Spear: both effects with the damage, the quake at tick 44 for 650 ms, no sound of its own.
        let struck = table.struck(2004).unwrap();
        assert!(struck.contains(&(Who::Caster, 723)) && struck.contains(&(Who::Caster, SPEAR_RUSH)));
        assert!(AIMED.contains(&SPEAR_RUSH) && !table.sounds.contains_key(&SPEAR_RUSH));
        assert_eq!(table.quakes[&SPEAR_RUSH], [(Duration::from_millis(704), QUAKE_LONG)]);
        // Swing Dance and its sisters flicker the target, each with its own sound at the listener from its use, none by effect.
        assert!([2350, 2351, 2352].iter().all(|&skill| table.used(skill).unwrap().contains(&(Who::Target, SWING_DANCE))));
        assert!(!table.sounds.contains_key(&SWING_DANCE) && table.skill(2425, "effectId").is_empty());
        let files = [2350, 2351, 2352].map(|skill| (table.skill_sounds[&skill][0].file.as_str(), table.skill_sounds[&skill][0].listener));
        assert_eq!(files, [("effect\\스윙댄스", true), ("effect\\연인들을위한심포니", true), ("effect\\AB_judex", true)]);
        // Harmonize: an emitter on both units; its lights (850) and Arrullo's (932) run 100 ticks, with no line of their own.
        let used = table.used(2383).unwrap();
        assert!(used.contains(&(Who::Target, HARMONIZE)) && used.contains(&(Who::Caster, HARMONIZE)));
        assert!(!table.sounds.contains_key(&850) && !table.floaters.contains_key(&850) && !table.rings.contains_key(&850));
        assert!(!table.sounds.contains_key(&932) && table.used(2383).unwrap().iter().all(|&(_, effect)| effect != 850 && effect != SOUL_LIGHT_PINK));
        assert_eq!((table.lengths[&850], table.lengths[&SOUL_LIGHT_PINK]), (100, 100));
        // Great Echo's "V": launched from the caster at the target, with the sound and the shake it had.
        assert!(AIMED.contains(&GREAT_ECHO_V) && table.struck(2426).unwrap().contains(&(Who::Caster, GREAT_ECHO_V)));
        assert_eq!(sounds(GREAT_ECHO_V), [("effect\\LG_EARTHDRIVE", 0, true)]);
        assert_eq!(table.quakes[&GREAT_ECHO_V], [(Duration::ZERO, QUAKE_LONG)]);
    }

    #[test]
    fn dead_hill_dance_with_warg_the_poem_note_and_randomize_spell_have_their_rows() {
        let table = Table::load();
        let sounds = |id: u32| table.sounds[&id].iter().map(|sound| (sound.file.as_str(), sound.delay_ms, sound.listener)).collect::<Vec<_>>();
        // Dead Hill Here: the song circle and the Geographers; both sounds at the listener; one Warg copy a use.
        let used = table.used(2421).unwrap();
        assert!(used.contains(&(Who::Target, 861)) && used.contains(&(Who::Target, DEAD_HILL)));
        assert_eq!(sounds(DEAD_HILL), [("effect\\사망의골짜기에서", 0, true)]);
        assert_eq!(sounds(WARG_DANCE), [("effect\\워그와함께춤을", 0, true)]);
        assert!(table.skill(2428, "groundEffectId").is_empty() && table.used(2428).unwrap().contains(&(Who::Target, WARG_DANCE)));
        // Poem of Netherworld: the unit keeps the note and sounds at its place; the generic parts and the skills' starts are gone.
        assert_eq!(table.units[&222], MELODY_NOTE);
        assert!(!table.units_once.contains(&222) && !table.floaters.contains_key(&MELODY_NOTE));
        assert_eq!(sounds(MELODY_NOTE), [("effect\\POEMOFNETHERWORLD", 0, false)]);
        assert!(table.skill(2415, "effectId").is_empty() && table.skill(2419, "groundEffectId").is_empty());
        // Randomize Spell: one copy on the target, the ten parts gone, the sound at the listener.
        assert!(table.used(2424).unwrap().contains(&(Who::Target, RANDOMIZE)) && table.skill(2424, "effectId").is_empty());
        assert!(!table.floaters.contains_key(&RANDOMIZE));
        assert_eq!(sounds(RANDOMIZE), [("effect\\AB_ancilla", 0, true)]);
    }

    #[test]
    fn the_wanderer_and_minstrel_songs_ring_the_target_and_sound_of_destruction_has_its_notes() {
        let table = Table::load();
        let sounds = |id: u32| table.sounds[&id].iter().map(|sound| (sound.file.as_str(), sound.delay_ms, sound.listener)).collect::<Vec<_>>();
        // The six songs: one ring a use, from the use only, each sound at the listener at tick 0.
        let files = [
            (2427, 868, "effect\\마나의노래"),
            (2430, 870, "effect\\새터데이나이트피버"),
            (2431, 872, "effect\\SR_LIGHTNINGWALK"),
            (2432, 874, "effect\\멜로디오브싱크"),
            (2433, 876, "effect\\비욘드오브워크라이"),
            (2434, 878, "effect\\언리미티드허밍보이스"),
        ];
        for (skill, effect, file) in files {
            assert!(table.used(skill).unwrap().contains(&(Who::Target, effect)), "{skill}");
            assert!(table.skill(skill, "groundEffectId").is_empty(), "{skill}");
            assert_eq!(sounds(effect), [(file, 0, true)]);
        }
        // Lerad's and Saturday Night Fever's circles come from their statuses alone.
        assert!(table.skill(2430, "effectIdOnCaster").is_empty() && table.skill(2431, "effectIdOnCaster").is_empty());
        assert!(table.status(76).contains(&871) && table.status(80).contains(&871) && table.status(451).contains(&871));
        // The status of 451 starts a circle only for an on value of exactly 1; 76 and 80 always start and remove.
        assert!(!circle_ignores(451, true, 1) && circle_ignores(451, true, 2) && circle_ignores(451, false, 1));
        assert!(!circle_ignores(76, true, 2) && !circle_ignores(80, false, 0));
        // Sound of Destruction: the notes and the four sliding copies with the damage; its sounds.
        let struck = table.struck(2429).unwrap();
        assert!(struck.contains(&(Who::Target, SOUND_NOTES)) && struck.contains(&(Who::Target, 425)));
        assert_eq!(
            sounds(SOUND_NOTES),
            [
                ("effect\\metalic", 0, true),
                ("effect\\복호격", 16, false),
                ("effect\\복호격", 176, false),
                ("effect\\복호격", 336, false),
                ("effect\\복호격", 496, false)
            ]
        );
    }

    #[test]
    fn exceed_break_the_dragon_combo_the_chi_burst_and_moon_slasher_have_their_rows() {
        let table = Table::load();
        let sounds = |id: u32| table.sounds[&id].iter().map(|sound| (sound.file.as_str(), sound.delay_ms, sound.listener)).collect::<Vec<_>>();
        // Exceed Break's cast, its sound at tick 10.
        assert_eq!(table.cast_aura(2316, 12), Some(PRIME_CHARGE));
        assert_eq!(sounds(PRIME_CHARGE), [("effect\\LG_EXEEDBREAK", 160, false)]);
        // The Dragon Combo's rays: no rings, the sound at tick 5, on the damage packets and the ground one.
        assert!(!table.rings.contains_key(&DRAGON_RAYS));
        assert_eq!(sounds(DRAGON_RAYS), [("effect\\mon_맹룡과강", 80, false)]);
        for skill in [2320, 2326, 2344] {
            assert!(table.struck(skill).unwrap().contains(&(Who::Caster, DRAGON_RAYS)), "{skill}");
        }
        assert!(table.ground(2518).unwrap().contains(&(Who::Caster, DRAGON_RAYS)) && table.ground(2518).unwrap().contains(&(Who::Caster, CHI_BURST)));
        // The chi burst's extra launches, including the Sorcerer's ground skills (Psychic Wave keeps its own cell effect).
        for skill in [2320, 2327, 2332] {
            assert!(table.struck(skill).unwrap().contains(&(Who::Caster, CHI_BURST)), "{skill}");
        }
        assert!(table.used(2337).unwrap().contains(&(Who::Caster, CHI_BURST)));
        for skill in [2446, 2449, 2450] {
            assert!(table.ground(skill).unwrap().contains(&(Who::Caster, CHI_BURST)), "{skill}");
        }
        assert!(table.ground(2449).unwrap().contains(&(Who::Ground, 922)));
        assert!(!table.sounds.contains_key(&CHI_BURST));
        // Moon Slasher and Sky Net Blow turn the caster.
        assert!(table.struck(2320).unwrap().contains(&(Who::Caster, MOON_SPIN)) && table.struck(2327).unwrap().contains(&(Who::Caster, MOON_SPIN)));
    }

    #[test]
    fn tiger_cannon_crescent_elbow_knuckle_arrow_and_hesperus_lit_have_their_rows() {
        let table = Table::load();
        let sounds = |id: u32| table.sounds[&id].iter().map(|sound| (sound.file.as_str(), sound.delay_ms, sound.listener)).collect::<Vec<_>>();
        // Tiger Cannon on its target with its own sound 80 ms in; no part row left to double it.
        assert!(table.struck(2330).unwrap().contains(&(Who::Target, TIGER_CANNON_FX)));
        assert_eq!(sounds(TIGER_CANNON_FX), [("effect\\mon_맹룡과강", 80, false)]);
        // Crescent Elbow launched by its caster (aimed, so its sounds play from the arm), two hits 160 and 320 ms in.
        assert!(AIMED.contains(&ELBOW_LINES) && table.used(2342).unwrap().contains(&(Who::Caster, ELBOW_LINES)));
        assert_eq!(sounds(ELBOW_LINES), [("effect\\EF_hit2", 160, false), ("effect\\EF_hit4", 320, false)]);
        // Knuckle Arrow keeps its sound at the listener and aims its lines; Hesperus Lit's disc goes on both its skills' targets.
        assert!(AIMED.contains(&KNUCKLE_LINES) && table.struck(2336).unwrap().contains(&(Who::Caster, KNUCKLE_LINES)));
        assert_eq!(sounds(KNUCKLE_LINES), [("effect\\SR_KNUCKLEARROW", 0, true)]);
        assert!(table.struck(2324).unwrap().contains(&(Who::Target, GROUND_SHAKE)) && table.struck(8418).unwrap().contains(&(Who::Caster, GROUND_SHAKE)));
    }

    #[test]
    fn the_royal_guard_buffs_hesperus_lit_trample_and_the_genetic_casts_have_their_rows() {
        let table = Table::load();
        let sounds = |id: u32| table.sounds[&id].iter().map(|sound| (sound.file.as_str(), sound.delay_ms, sound.listener)).collect::<Vec<_>>();
        // 899: Hesperus Lit's target and Ride in Lightning's cell; Hesperus Lit's sound is the skill's, at the use.
        assert!(table.struck(2324).unwrap().contains(&(Who::Target, BLUE_LINE)) && table.ground(2518).unwrap().contains(&(Who::Ground, BLUE_LINE)));
        assert!(!table.sounds.contains_key(&BLUE_LINE));
        assert_eq!((table.skill_sounds[&2324][0].file.as_str(), table.skill_sounds[&2324][0].listener), ("effect\\LG_HESPERUSLIT", true));
        // 905: Trample's disc on its caster with the sound at the listener.
        assert!(table.struck(2309).unwrap().contains(&(Who::Caster, GROUND_SHAKE_RED)));
        assert_eq!(sounds(GROUND_SHAKE_RED), [("effect\\LG_TRAMPLE", 0, true)]);
        // 906: no part rows or skill row left; the sounds at 48 and 400 ms and the shake at 528 ms.
        assert_eq!(sounds(SHIELD_PRESS_FX), [("effect\\프레셔", 48, false), ("effect\\LG_SHIELDPRESS", 400, false)]);
        assert_eq!(table.quakes[&SHIELD_PRESS_FX], [(Duration::from_millis(528), QUAKE_LONG)]);
        assert!(!table.parts.contains_key(&SHIELD_PRESS_FX) && table.skill(2310, "beforeHitEffectId").is_empty());
        assert!(table.struck(2310).unwrap().contains(&(Who::Caster, SHIELD_PRESS_FX)) && table.struck(2324).unwrap().contains(&(Who::Target, SHIELD_PRESS_FX)));
        // 907: aimed from the caster at the target, its sound 416 ms in at the caster.
        assert!(AIMED.contains(&PINPOINT_FX) && table.struck(2312).unwrap().contains(&(Who::Caster, PINPOINT_FX)));
        assert_eq!(sounds(PINPOINT_FX), [("effect\\LG_PINPOINTATTACK", 416, false)]);
        // 908 to 910: the skills' effect rows are gone; Banding's status holds 909; the sounds at 160 ms (908 has none new).
        for skill in [2318, 2319, 2325] {
            assert!(table.skill(skill, "effectId").is_empty(), "{skill}");
        }
        assert!(table.used(2318).unwrap().contains(&(Who::Caster, PRESTIGE_FX)) && table.used(2325).unwrap().contains(&(Who::Caster, INSPIRATION_FX)));
        assert!(table.status(405).contains(&(PRESTIGE_FX + 1)));
        assert_eq!((sounds(909), sounds(910)), (vec![("effect\\LG_BANDING", 160, false)], vec![("effect\\LG_INSPIRATION", 160, false)]));
        assert_eq!(sounds(PRESTIGE_FX), [("effect\\lg_prestige", 0, false)]);
    }

    #[test]
    fn the_genetic_casts_glow_green_and_sound_100_up() {
        // A cast aura like 826's: white rings tinted #64FF64, added on, the short aura (60 ticks or less) and the long cycle.
        assert!(is_cast_aura(GREEN_CASTING) && is_banded(GREEN_CASTING) && is_bloom(GREEN_CASTING));
        assert_eq!(aura_look(GREEN_CASTING), ("ring_white", [0x64, 0xFF, 0x64], true));
        assert_eq!((aura_ticks(GREEN_CASTING, 400), aura_ticks(GREEN_CASTING, 2000), aura_ticks(GREEN_CASTING, 0)), (56, 125, 56));
        let first = |ticks: u32| (0..4).map(|band| bloom(12, 0, band, ticks, 0).map(|shape| shape.alpha)).collect::<Vec<_>>();
        assert_eq!(first(60), [None, None, None, None], "the short aura starts every band at 0");
        assert_eq!(first(62)[0], Some(177.0), "the long cycle starts from 180, 3 down on the first tick");
        let table = Table::load();
        assert_eq!(table.cast_aura(2494, 12), Some(GREEN_CASTING));
        assert!(table.caster(2494).is_some_and(|effect| effect == GREEN_CASTING));
        assert_eq!(
            table.sounds[&GREEN_CASTING].iter().map(|sound| (sound.file.as_str(), sound.delay_ms, sound.listener)).collect::<Vec<_>>(),
            [("effect\\EF_BeginSpell", 0, false)]
        );
    }

    #[test]
    fn epiclesis_soul_ascetic_earth_grave_and_cart_boost_have_their_rows() {
        let table = Table::load();
        let sounds = |id: u32| table.sounds[&id].iter().map(|sound| (sound.file.to_lowercase(), sound.delay_ms, sound.listener)).collect::<Vec<_>>();
        // Epiclesis' unit shows 1655 with no sound; the three casts sound at the caster at tick 0.
        assert_eq!(table.units[&202], EPICLESIS_CELL);
        assert!(!table.sounds.contains_key(&EPICLESIS_CELL));
        for id in [SOUL_CAST, EARTH_CAST, CART_CAST] {
            assert_eq!(sounds(id), [("effect\\ef_beginspell".to_owned(), 0, false)], "{id}");
        }
        // Soul Ascetic's three Shinkiro casts show 1656 and 812; Reverberation's shows 1656 only.
        for skill in [5493, 5494, 5495] {
            assert_eq!((table.cast_aura(skill, 12), table.cast_extras(skill)), (Some(SOUL_CAST), vec![812]), "{skill}");
        }
        assert_eq!((table.cast_aura(2414, 12), table.cast_extras(2414)), (Some(SOUL_CAST), Vec::new()));
        assert_eq!((table.cast_aura(2446, 12), table.cast_aura(2478, 12)), (Some(EARTH_CAST), Some(CART_CAST)));
        // Soul Ascetic's cone is made in code, not a band aura.
        assert!(!is_cast_aura(SOUL_CAST) && !is_banded(SOUL_CAST));
    }

    #[test]
    fn earth_graves_cast_turns_white_crescents_and_a_breathing_sheet() {
        assert!(is_cast_aura(EARTH_CAST) && is_banded(EARTH_CAST) && !is_bloom(EARTH_CAST));
        // At least 70 ticks, 100 when started any other way; the look is ring_yellow in white, blended.
        assert_eq!((aura_ticks(EARTH_CAST, 100), aura_ticks(EARTH_CAST, 2000), banded_ticks(EARTH_CAST)), (70, 125, 100));
        assert_eq!(aura_look(EARTH_CAST), ("ring_yellow", [0xFF; 3], false));
        // The crescents: alpha 180 until n = length − 41, 175 at n = length − 40, 0 at n = length − 5 (n = age + 1).
        let length = 200;
        let crescent = |n: u32| earth_turning(0, length, n - 1).alpha;
        assert_eq!([length - 41, length - 40, length - 5].map(crescent), [180.0, 175.0, 0.0]);
        // They turn 3, 4 and 5° a tick like the other turning auras.
        assert_eq!([earth_turning(0, length, 10).turn, earth_turning(2, length, 10).turn], [30.0, 230.0]);
        // The sheet: 70 + 8 a tick to 100 (78, 86, 94, 100 for n = 1 … 4), then 3 less a tick over the last 40: 97 at n = length − 40.
        let sheet = |n: u32| earth_turning(3, length, n - 1).alpha;
        assert_eq!([1, 2, 3, 4, 5, 100].map(sheet), [78.0, 86.0, 94.0, 100.0, 100.0, 100.0]);
        assert_eq!([length - 41, length - 40, length - 8].map(sheet), [100.0, 97.0, 1.0]);
        assert_eq!([length - 7, length].map(sheet), [0.0, 0.0]);
        let sheet_shape = earth_turning(3, length, 0);
        assert_eq!((sheet_shape.arc, sheet_shape.radius, sheet_shape.lean, sheet_shape.height), (360.0, 4.0, 89.0, 250.0));
        // Its rim: sin n° to n = 90, then 0.55 + 0.45·sin(n mod 360)° between 0.1 and 1.0.
        let near = |got: f32, want: f32| (got - want).abs() < 1e-4;
        assert!(near(earth_sheet_rim(45), 45.0_f32.to_radians().sin()) && near(earth_sheet_rim(90), 1.0));
        assert!(near(earth_sheet_rim(91), 0.55 + 0.45 * 91.0_f32.to_radians().sin()) && near(earth_sheet_rim(270), 0.1));
        // The hump along the rim is h·sin 9s°: 250 high at s = 10 and nothing at its ends.
        assert!(near(earth_height(3, 250.0, 89, 10), 250.0) && near(earth_height(3, 250.0, 89, 0), 0.0) && near(earth_height(3, 250.0, 269, 10), 25.0));
        // Every other turning aura's sheet rises like the crescents: half-way at age 30, the hump from 90.
        assert!(
            near(turning_height(3, 250.0, 30, 10), 125.0) && near(turning_height(3, 250.0, 90, 10), 250.0) && near(turning_height(3, 250.0, 200, 10), 250.0)
        );
        assert!(near(turning_height(3, 250.0, 200, 5), 250.0 * 45.0_f32.to_radians().sin()));
    }

    #[test]
    fn cart_boosts_cast_is_two_sets_of_light_orange_blooming_cones() {
        assert!(is_cast_aura(CART_CAST) && is_banded(CART_CAST) && is_bloom(CART_CAST));
        assert_eq!((aura_ticks(CART_CAST, 100), aura_ticks(CART_CAST, 2000)), (56, 125));
        assert_eq!(aura_look(CART_CAST), ("ring_white", [0xF4, 0xBB, 0x7C], true));
        let aura =
            Banded { effect: CART_CAST, anchor: Anchor::Unit(None), start: Instant::now(), ticks: 100, turns: [0.0; 3], extra: [0.0; 2], sparkles: Vec::new() };
        let bands = aura.bloom_bands(0);
        assert_eq!(bands.len(), 8);
        assert_eq!(bands.iter().take(4).map(|(_, band, _)| band.height).collect::<Vec<_>>(), [20.0, 19.0, 18.0, 17.0]);
    }

    #[test]
    fn comet_jack_frost_maelstrom_clearance_and_varetyr_spear_cast_their_auras() {
        let table = Table::load();
        let sounds = |id: u32| table.sounds[&id].iter().map(|sound| (sound.file.as_str(), sound.delay_ms)).collect::<Vec<_>>();
        for id in [COMET_CAST, JACK_CAST, MAELSTROM_CAST, CLEARANCE_CAST, VARETYR_CAST] {
            assert!(is_cast_aura(id) && is_banded(id), "{id}");
            assert_eq!(sounds(id), [("effect\\ef_beginspell", 0)], "{id}");
        }
        for (skill, id) in [(2213, COMET_CAST), (2204, JACK_CAST), (2302, MAELSTROM_CAST), (2052, CLEARANCE_CAST), (2454, VARETYR_CAST)] {
            assert_eq!(table.cast_aura(skill, 12), Some(id), "{skill}");
        }
        // At least 56 ticks for the four blooming ones, 70 for Jack Frost's.
        assert_eq!([COMET_CAST, MAELSTROM_CAST, CLEARANCE_CAST, VARETYR_CAST].map(|id| (aura_ticks(id, 100), aura_ticks(id, 2000))), [(56, 125); 4]);
        assert_eq!((aura_ticks(JACK_CAST, 100), aura_ticks(JACK_CAST, 2000)), (70, 125));
        assert!([COMET_CAST, MAELSTROM_CAST, CLEARANCE_CAST, VARETYR_CAST].iter().all(|&id| is_bloom(id)) && !is_bloom(JACK_CAST));
        // The looks: Comet's rainbow ring, Jack Frost's ice ring blended, the others added on.
        assert_eq!(aura_look(COMET_CAST), ("new_comet\\new_comet_cast_ring\\ring_rainbow", [0x0A, 0x42, 0x72], true));
        assert_eq!(aura_look(MAELSTROM_CAST), ("ring_blue", [0x0A, 0x42, 0x72], true));
        assert_eq!(aura_look(CLEARANCE_CAST), ("ring_white", [0xA4, 0xCD, 0xFE], true));
        assert_eq!(aura_look(VARETYR_CAST), ("ring_yellow", [0x54, 0xA9, 0xCD], true));
        assert_eq!(aura_look(JACK_CAST), ("ring_ice", [0xC3, 0xC3, 0xFF], false));
        assert_eq!((aura_colour(MAELSTROM_CAST, 0, 5), aura_colour(MAELSTROM_CAST, 1, 5)), ([0x0A, 0x42, 0x72], [0x23, 0x92, 0xB9]));
        assert_eq!((aura_sets(MAELSTROM_CAST), aura_sets(COMET_CAST), aura_sets(JACK_CAST)), (2, 1, 1));
        // Bands: Comet makes the first set only, the others both; Clearance's stand 15 to 12.
        let aura = |effect: u32| Banded {
            effect,
            anchor: Anchor::Unit(None),
            start: Instant::now(),
            ticks: 100,
            turns: [0.0; 3],
            extra: [0.0; 2],
            sparkles: Vec::new(),
        };
        assert_eq!(
            aura(COMET_CAST).bloom_bands(0).iter().map(|(set, band, _)| (*set, band.height)).collect::<Vec<_>>(),
            [(0, 20.0), (0, 19.0), (0, 18.0), (0, 17.0)]
        );
        assert_eq!(aura(MAELSTROM_CAST).bloom_bands(0).len(), 8);
        assert_eq!(aura(VARETYR_CAST).bloom_bands(0).len(), 8);
        let clearance = aura(CLEARANCE_CAST).bloom_bands(0);
        assert_eq!(clearance.len(), 8);
        assert_eq!(clearance.iter().take(4).map(|(_, band, _)| band.height).collect::<Vec<_>>(), [15.0, 14.0, 13.0, 12.0]);
        // Clearance gains 5 alpha a tick under radius 4 (12 and 58 gain 10) and starts again leaning 64° (not 74°).
        let rise = |effect: u32| {
            let alpha = |age: u32| bloom(effect, 0, 3, 1000, age).map_or(0.0, |band| band.alpha);
            (2..300).find(|&age| alpha(age) > alpha(age - 1) && alpha(age - 1) > 0.0).map(|age| alpha(age) - alpha(age - 1))
        };
        assert_eq!((rise(CLEARANCE_CAST), rise(58)), (Some(5.0), Some(10.0)));
        let lean = |effect: u32| {
            let at = |age: u32| bloom(effect, 0, 3, 1000, age);
            (2..300).find(|&age| at(age).is_some() && at(age - 1).is_none()).and_then(at).map(|band| band.lean)
        };
        assert_eq!((lean(CLEARANCE_CAST), lean(58)), (Some(63.0), Some(73.0)));
        // Jack Frost turns like 54: crescents fading over the last 40 ticks, 5 a tick.
        let ice = aura(JACK_CAST);
        assert_eq!(ice.bands(0).len(), 4);
        assert_eq!([turning(0, 100, 60).alpha, turning(0, 100, 61).alpha, turning(0, 100, 99).alpha], [180.0, 175.0, 0.0]);
    }

    #[test]
    fn severe_rainstorm_cart_tornado_and_cannon_spear_have_their_rows() {
        let table = Table::load();
        let sounds = |id: u32| table.sounds[&id].iter().map(|sound| (sound.file.as_str(), sound.delay_ms, sound.listener)).collect::<Vec<_>>();
        // Severe Rainstorm's cast: four begin-spell sounds at tick 0, one per prim.
        assert_eq!(table.cast_aura(2418, 0), Some(RAINSTORM_CAST));
        assert_eq!(sounds(RAINSTORM_CAST), [("effect\\EF_BeginSpell", 0, false); 4]);
        // Cart Tornado: 50 ticks on its caster, the sound at 80 ms.
        assert_eq!(table.lengths[&CART_TORNADO], 50);
        assert!(table.used(2476).unwrap().contains(&(Who::Caster, CART_TORNADO)));
        assert_eq!(sounds(CART_TORNADO), [("effect\\T_회오리차기", 80, false)]);
        // Cannon Spear: the lines from the caster at the target at each damage packet, the sound at the listener.
        assert_eq!(table.lengths[&CANNON_LINES], 300);
        assert!(AIMED.contains(&CANNON_LINES) && !aimed_at_centre(CANNON_LINES));
        assert!(table.struck(2307).unwrap().contains(&(Who::Caster, CANNON_LINES)));
        assert_eq!(sounds(CANNON_LINES), [("effect\\LG_CANNONSPEAR", 0, true)]);
    }

    #[test]
    fn rampage_blaster_cross_impact_and_reverberation_have_their_rows() {
        let table = Table::load();
        assert_eq!([RAMPAGE_BODY, CROSS_LINES, REVERB_FX].map(|id| table.lengths[&id]), [300, 300, 150]);
        // Rampage Blaster's chi burst is the body flash alone under /mineffect.
        assert_eq!((rampage_effect(2332, CHI_BURST, true), rampage_effect(2332, CHI_BURST, false)), (RAMPAGE_BODY, CHI_BURST));
        assert_eq!((rampage_effect(2324, CHI_BURST, true), rampage_effect(2332, 549, true)), (CHI_BURST, 549));
        assert!(table.struck(2332).unwrap().contains(&(Who::Caster, CHI_BURST)));
        // Cross Impact and Rush Strike race the lines from the caster at the target.
        assert!(AIMED.contains(&CROSS_LINES) && !aimed_at_centre(CROSS_LINES));
        for skill in [2022, 6506] {
            assert!(table.struck(skill).unwrap().contains(&(Who::Caster, CROSS_LINES)), "{skill}");
        }
        assert!(!table.sounds.contains_key(&CROSS_LINES) && !table.sounds.contains_key(&RAMPAGE_BODY));
        // Reverberation's note and rings sound at the listener at once.
        assert!(table.struck(2414).unwrap().contains(&(Who::Target, REVERB_FX)));
        assert_eq!(
            table.sounds[&REVERB_FX].iter().map(|sound| (sound.file.as_str(), sound.delay_ms, sound.listener)).collect::<Vec<_>>(),
            [("effect\\REVERBERATION", 0, true)]
        );
    }

    #[test]
    fn the_cells_sprites_blasts_and_strikings_copies_have_their_rows() {
        let table = Table::load();
        let sounds = |id: u32| table.sounds[&id].iter().map(|sound| (sound.file.as_str(), sound.delay_ms, sound.listener)).collect::<Vec<_>>();
        // A ground unit starts again 1 tick before its length for the walls, the Insignias and their like, 7 for Fire Walk's.
        for kind in [228, 230, 231, 232, 233, 235, 241, 242, 243, 244, 258] {
            assert_eq!(restart_ticks(0, kind, 100), 99, "{kind}");
        }
        assert_eq!((restart_ticks(920, 220, 40), restart_ticks(0, 221, 40), restart_ticks(0, 9, 100), restart_ticks(95, 0, 150)), (33, 33, 92, 130));
        assert_eq!((restart_ticks(912, 230, 1000), restart_ticks(916, 231, 2000)), (999, 1999));
        // Wall of Thorns: no STR left; the Ifrits and the whirlwinds stand on their units, not on skills' lines.
        assert!(!table.parts.contains_key(&WALL_OF_THORNS) && table.units[&230] == WALL_OF_THORNS);
        assert!(
            [231, 241, 242, 243, 244].iter().all(|kind| table.units[kind] == DEMONIC_FIRE) && table.units[&232] == 917 && table.units[&233] == DEMONIC_FIRE_3
        );
        assert_eq!(table.units[&235], VACUUM_EXTREME);
        for (skill, field, effect) in
            [(2485, "effectId", 916), (2486, "effectId", 917), (2487, "effectId", 918), (2453, "effectId", 921), (2443, "groundEffectId", 920)]
        {
            assert!(table.skill(skill, field).is_empty(), "{skill} {effect}");
        }
        // The sounds at the listener at once.
        for (effect, file) in [
            (916, "effect\\EF_FireWall"),
            (917, "effect\\EF_FireWall"),
            (918, "effect\\EF_FireWall"),
            (921, "effect\\S스펠피스트"),
            (923, "effect\\S포이즌버스터"),
        ] {
            assert_eq!(sounds(effect), [(file, 0, true)], "{effect}");
        }
        // Poison Buster on its targets; Striking on its status, with no looks of its own.
        assert!(
            table.struck(2448).unwrap().contains(&(Who::Target, POISON_BUSTER_FX)) && table.struck(742).unwrap().contains(&(Who::Target, POISON_BUSTER_FX))
        );
        assert!(table.status(445).contains(&crate::actor::STRIKING) && !table.sounds.contains_key(&crate::actor::STRIKING));
    }

    #[test]
    fn electric_walk_warmer_arrullo_elemental_action_and_lux_anima_have_their_rows() {
        let table = Table::load();
        let sounds = |id: u32| table.sounds[&id].iter().map(|sound| (sound.file.as_str(), sound.delay_ms, sound.listener)).collect::<Vec<_>>();
        // 926: the wave runs again every 30 ticks while the unit stands, from no `once`, with no skill-line copy.
        assert_eq!((restart_ticks(926, 221, 29), restart_ticks(926, 221, 100)), (30, 30));
        assert!(!table.units_once.contains(&221) && table.units[&221] == 926 && table.skill(2444, "groundEffectId").is_empty());
        // 929: the Imps on Warmer's unit, the sound at the listener, no skill-line copy.
        assert_eq!(table.units[&228], WARMER);
        assert_eq!(sounds(WARMER), [("effect\\s워머", 0, true)]);
        assert!(table.skill(2452, "effectId").is_empty());
        assert_eq!(restart_ticks(WARMER, 228, 9999), 9998);
        // 931: Arrullo's use puts the blue aura, 886 and the spirals on its target, with the sound of Poison Buster at the listener.
        let used = table.used(2455).unwrap();
        assert!(used.contains(&(Who::Target, 886)) && used.contains(&(Who::Target, BLUE_CASTING)) && used.contains(&(Who::Target, ARRULLO_FX)));
        assert_eq!(sounds(ARRULLO_FX), [("effect\\S포이즌버스터", 0, true)]);
        // 934: the use and the cast start of Akumukesu; its cast also keeps the aura 12 and starts the crescents beside it.
        assert!(table.used(2461).unwrap().contains(&(Who::Caster, ELEMENTAL_ACTION)) && !table.sounds.contains_key(&ELEMENTAL_ACTION));
        assert_eq!(table.cast_aura(5478, 58), Some(58));
        assert_eq!(table.cast_extras(5478), [ELEMENTAL_ACTION]);
        assert!(table.cast_extras(2455).is_empty() && table.cast_extras(1).is_empty());
        // 1044: Lux Anima's sound with its STR.
        assert_eq!(sounds(1044), [("effect\\rk_luxanima", 0, false)]);
        assert!(table.parts.contains_key(&1044) && table.used(5005).unwrap().contains(&(Who::Caster, 1044)));
    }

    #[test]
    fn lullaby_deep_sleep_has_its_sound_at_the_listener() {
        let table = Table::load();
        assert_eq!(
            table.sounds[&LULLABY].iter().map(|sound| (sound.file.as_str(), sound.delay_ms, sound.listener)).collect::<Vec<_>>(),
            [("effect\\안식의자장가", 0, true)]
        );
        assert!(table.used(2422).unwrap().contains(&(Who::Target, LULLABY)) && !table.parts.contains_key(&LULLABY));
    }

    #[test]
    fn kings_grace_offertoriums_ring_and_escape_have_their_rows() {
        let table = Table::load();
        assert_eq!(table.units[&258], KINGS_GRACE);
        assert!(!table.sounds.contains_key(&KINGS_GRACE));
        // Offertorium's ring sits 5 up, the others 80, and starts again every 121 ticks.
        assert_eq!((str_rise(OFFERTORIUM_RING), str_rise(1043), str_rise(220)), (5.0, 80.0, 80.0));
        assert_eq!(restart_ticks(OFFERTORIUM_RING, 0, 121), 121);
        assert_eq!(table.status(716), &[OFFERTORIUM_RING]);
        // Escape on the source of its use, with no sound and no line of its own for 793.
        assert!(table.used(5010).unwrap().contains(&(Who::Caster, ESCAPE_FX)) && !table.sounds.contains_key(&ESCAPE_FX));
        assert!(!table.parts.contains_key(&793) && !table.sounds.contains_key(&793));
    }

    #[test]
    fn sight_and_the_monsters_element_changes_have_their_rows() {
        let table = Table::load();
        // 22 comes from the option and from Wide Sight, not from the Sight skill; its sound belongs to the skill.
        assert!(table.skill(10, "effectId").is_empty() && !table.parts.contains_key(&SIGHT) && !table.sounds.contains_key(&SIGHT));
        assert!(table.struck(669).unwrap().contains(&(Who::Caster, SIGHT)));
        assert_eq!((table.skill_sounds[&10][0].file.as_str(), table.skill_sounds[&10][0].delay_ms), ("effect\\ef_sight", 0));
        assert_eq!(table.lengths[&SIGHT], 270);
        // The element changes: the six rows hold and Telekinesis is added; 100 ticks each.
        for (skill, effect) in [(168, 172), (164, 173), (162, 174), (165, 175), (169, 176), (163, 177), (167, 178)] {
            assert_eq!(table.skill(skill, "effectId"), [effect], "{skill}");
        }
        assert!((172..=178).all(|id| table.lengths[&id] == 100 && !table.sounds.contains_key(&id)));
    }

    #[test]
    fn resurrection_the_family_calls_portal_and_tomahawk_have_their_rows() {
        let table = Table::load();
        let sounds = |id: u32| table.sounds[&id].iter().map(|sound| (sound.file.as_str(), sound.delay_ms, sound.listener)).collect::<Vec<_>>();
        // 140 comes with 77 at a Resurrection's use and has no sound of its own (77's plays once).
        let skill = table.skill(54, "effectId");
        assert_eq!(skill, [77, REVIVE_FX]);
        assert!(!table.sounds.contains_key(&REVIVE_FX) && sounds(77).len() == 1);
        assert_eq!(table.lengths[&REVIVE_FX], 250);
        // 341 is the family call's unit, 9 999 ticks long, restarting 8 ticks before; no sound.
        assert_eq!(table.units[&178], PORTAL_FX);
        assert_eq!(table.lengths[&PORTAL_FX], 9999);
        assert_eq!(restart_ticks(PORTAL_FX, 178, 9999), 9991);
        assert!(!table.sounds.contains_key(&PORTAL_FX) && !table.units_once.contains(&178));
        // 494: the axe is aimed from the caster at the target, with one sound.
        assert!(AIMED.contains(&AXE_THROW) && table.struck(337).unwrap().contains(&(Who::Caster, AXE_THROW)));
        assert_eq!(sounds(AXE_THROW), [("effect\\cru_shield boomerang", 0, false)]);
        assert_eq!(table.lengths.get(&AXE_THROW), None);
    }

    #[test]
    fn im_all_yours_cast_and_babys_ribbons_have_their_rows() {
        let table = Table::load();
        let sounds = |id: u32| table.sounds[&id].iter().map(|sound| (sound.file.as_str(), sound.delay_ms, sound.listener)).collect::<Vec<_>>();
        // The cast of skill 335 starts 343; one sound at the caster's place at once.
        assert_eq!(table.cast_aura(335, 12), Some(HEART_CAST));
        assert_eq!(sounds(HEART_CAST), [("effect\\EF_BeginSpell", 0, false)]);
        // Baby and Cheer Up put 408 on their target for 200 ticks, with the Blessing sound.
        assert!(table.used(408).unwrap().contains(&(Who::Target, BABY_FX)) && table.used(5065).unwrap().contains(&(Who::Target, BABY_FX)));
        assert_eq!(sounds(BABY_FX), [("effect\\EF_Blessing", 0, false)]);
        assert_eq!(table.lengths[&BABY_FX], 200);
    }

    #[test]
    fn poison_mists_units_put_a_puff_on_a_grid_round_each_cell() {
        let start = Instant::now();
        let table = Table::load();
        let (effect, lift) = table.areas[&245];
        assert_eq!((effect, lift), (959, 20.0));
        let period = table.lengths[&effect] - 20;
        assert_eq!(period, 40);
        let tick = |n: u32| start + Duration::from_secs_f32(STR_TICK_MS / 1000.0) * n;
        let area = |id: u32, range: u8, at: Vector, now: Instant| Area { id, effect, lift, range, at, next: now, period };
        let flat = |_: f32, _: f32| 2.0;
        let copies = |effects: &mut Effects, ticks: &[u32]| -> Vec<usize> {
            ticks
                .iter()
                .map(|&n| {
                    effects.make_areas(tick(n), flat, |_| 0);
                    effects.take_areas().len()
                })
                .collect()
        };
        // Range 0: one copy at tick 0, none at 1 … 39, one more at 40 and 80.
        let mut one = Effects { areas: vec![area(1, 0, [10.0, 0.0, 10.0], start)], ..Effects::default() };
        assert_eq!(copies(&mut one, &[0, 1, 20, 39, 40, 41, 80]), [1, 0, 0, 0, 1, 0, 1]);
        // 49 units (a 7 x 7 area) start 49 copies at tick 0, each on its own cell.
        let cells: Vec<Area> = (0..49).map(|k| area(k, 0, [5.0 * (k % 7) as f32, 0.0, 5.0 * (k / 7) as f32], start)).collect();
        let mut seven = Effects { areas: cells, ..Effects::default() };
        seven.make_areas(start, flat, |_| 5);
        let made = seven.take_areas();
        assert_eq!(made.len(), 49);
        assert!(made.iter().all(|&(each, _, when)| each == 959 && when == start));
        // A unit that comes into sight 10 ticks later renews at its own tick 50.
        let mut late = Effects { areas: vec![area(2, 0, [0.0; 3], tick(10))], ..Effects::default() };
        assert_eq!(copies(&mut late, &[10, 49, 50]), [1, 0, 1]);
        // Ranges 1, 2, 3 make 9, 25, 49 copies, none between grids.
        for (range, want) in [(1, 9), (2, 25), (3, 49)] {
            let mut grid = Effects { areas: vec![area(3, range, [0.0; 3], start)], ..Effects::default() };
            assert_eq!(copies(&mut grid, &[0, 1, 39, 40, 80]), [want, 0, 0, want, want], "range {range}");
        }
        // The offsets of a range-3 grid are 6·i, 6·j (outer i, inner j) plus the jitter; draws 0 give −1.0 and no rise, draws 10 give
        // +1.0 and 4.0 up, the lift 20 under the ground.
        let mut low = Effects { areas: vec![area(4, 3, [100.0, 0.0, 200.0], start)], ..Effects::default() };
        low.make_areas(start, flat, |_| 0);
        let low = low.take_areas();
        assert_eq!((low[0].1, low[1].1), ([100.0 - 18.0 - 1.0, 2.0 - 20.0, 200.0 - 18.0 - 1.0], [100.0 - 18.0 - 1.0, 2.0 - 20.0, 200.0 - 12.0 - 1.0]));
        assert_eq!(low.last().unwrap().1[0], 100.0 + 18.0 - 1.0);
        let mut high = Effects { areas: vec![area(5, 0, [0.0; 3], start)], ..Effects::default() };
        high.make_areas(start, flat, |_| 10);
        let [x, y, z] = high.take_areas()[0].1;
        assert!((x - 1.0).abs() < 1e-4 && (z - 1.0).abs() < 1e-4 && (y - (2.0 - 20.0 + 4.0)).abs() < 1e-4);
        // The draws of a point are x, z, then the height.
        let mut order = Effects { areas: vec![area(6, 0, [0.0; 3], start)], ..Effects::default() };
        let mut asked = Vec::new();
        order.make_areas(start, flat, |n| {
            asked.push(n);
            asked.len() as u32
        });
        assert_eq!(asked, [11, 11, 11]);
        let [x, y, z] = order.take_areas()[0].1;
        assert!((x - (0.2 - 1.0)).abs() < 1e-4 && (z - (0.4 - 1.0)).abs() < 1e-4 && (y - (2.0 - 20.0 + 1.2)).abs() < 1e-4);
        // A gone unit makes no more grids.
        one.forget(1);
        assert_eq!(copies(&mut one, &[120]), [0]);
        // Rows: no skill-line copy, the puff stays where it was made, and Power of Gaia restarts a tick before its length.
        assert!(table.skill(8020, "effectId").is_empty() && !table.parts[&959][0].attached && table.lengths[&959] == 60);
        // (Zephyr's cell, type 239, restarts a tick early too.)
        assert_eq!((restart_ticks(1088, 240, 2000), restart_ticks(0, 239, 2000), restart_ticks(0, 9, 2000)), (1999, 1999, 1992));
    }

    #[test]
    fn the_monsters_change_poison_grand_darkness_power_up_and_anti_magic_have_their_rows() {
        let table = Table::load();
        let sounds = |id: u32| table.sounds[&id].iter().map(|sound| (sound.file.as_str(), sound.delay_ms, sound.listener)).collect::<Vec<_>>();
        assert_eq!(table.lengths[&179], 100);
        assert!(table.used(166).unwrap().contains(&(Who::Caster, 179)) && !table.sounds.contains_key(&179));
        // Grand Darkness: one more cross a damage packet, its sound once each, 999 ticks.
        assert!(table.struck(339).unwrap().contains(&(Who::Caster, GRAND_DARKNESS)));
        assert_eq!((sounds(GRAND_DARKNESS), table.lengths[&GRAND_DARKNESS]), (vec![("effect\\cru_grand cross", 0, false)], 999));
        // Power Up: two sets of four glints in red where Steel Body keeps four sets in blue and the Explosion Spirits' one.
        assert_eq!((4 * glint_sets(POWER_UP), 4 * glint_sets(254), 4 * glint_sets(GUMGANG)), (8, 16, 4));
        assert_eq!((glint_color(POWER_UP), glint_color(254)), ([0xFF, 0x14, 0x14], GLINT_COLOR));
        assert!(table.used(349).unwrap().contains(&(Who::Caster, POWER_UP)) && table.status(98).contains(&POWER_UP));
        assert_eq!((table.lengths[&POWER_UP], POWER_UP_TICKS), (1500, 1500));
        assert_eq!((table.skill_sounds[&349][0].file.as_str(), table.skill_sounds[&349][0].listener), ("effect\\mon_폭기", true));
        // Anti-Magic: the dome on the caster for 200 ticks, its sound as before.
        assert!(table.struck(676).unwrap().contains(&(Who::Caster, ANTI_MAGIC)));
        assert_eq!((sounds(ANTI_MAGIC), table.lengths[&ANTI_MAGIC]), (vec![("effect\\kyrie_guard", 0, false)], 200));
    }

    #[test]
    fn the_dark_hit_lope_and_the_warmth_have_their_rows_and_looks() {
        let table = Table::load();
        // 180: Darkness Attack, Curse Explosion and Spa; near-black sparks that /mineffect keeps; the ring and the random draws as EF_HIT1.
        for skill in [190, 2600, 2602] {
            assert_eq!(table.hit(skill), Some([Some(HIT_DARK), None]), "{skill}");
        }
        assert_eq!((spark_tint(true), spark_tint(false)), ([20.0 / 255.0; 3], [1.0; 3]));
        assert_eq!([(true, true), (true, false), (false, true), (false, false)].map(|(min, dark)| sparks_shown(min, dark)), [true, false, true, true]);
        assert_eq!(table.lengths[&HIT_DARK], 50);
        // 411: Lope's use (beside 446), 300 ticks, no sound.
        let used = table.used(5023).unwrap();
        assert!(used.contains(&(Who::Caster, LOPE)) && used.contains(&(Who::Caster, 446)));
        assert_eq!(table.lengths[&LOPE], 300);
        assert!(!table.sounds.contains_key(&LOPE));
        // 418: the warmth's eight glints, three passes each, from the rows that start it.
        assert_eq!((4 * glint_sets(WARMTH_GLINTS), 4 * glint_sets(254)), (8, 16));
        assert_eq!(WARMTH_ALPHAS, [40, 80, 120, 160]);
        assert_eq!(glint_looks(WARMTH_GLINTS), vec![(2.0, [0xFF, 0x64, 0x64]), (2.4, [0x78, 0, 0]), (4.0, [0x78, 0, 0])]);
        assert_eq!(glint_looks(254), vec![(GLINT_RADIUS, GLINT_COLOR)]);
        for skill in [428, 429, 430] {
            assert!(table.used(skill).unwrap().contains(&(Who::Caster, WARMTH_GLINTS)), "{skill}");
        }
        assert!([165, 166, 167, 232].iter().all(|&status| table.status(status).contains(&WARMTH_GLINTS)));
        assert_eq!(table.lengths[&WARMTH_GLINTS], 99_999_999);
        // At age 1 a glint's alpha is the lesser of its cycle's (148) and the age: 1; at 40 the cycle's own.
        assert_eq!((glint_at(WARMTH_ALPHAS[3], 1).0.min(1), glint_at(WARMTH_ALPHAS[3], 1).0), (1, 148));
        assert!(glint_at(WARMTH_ALPHAS[3], 40).0.min(40) <= 40);
    }

    #[test]
    fn the_kicks_and_swoo_change_the_targets_body() {
        let table = Table::load();
        let sounds = |id: u32| table.sounds[&id].iter().map(|sound| (sound.file.as_str(), sound.delay_ms, sound.listener)).collect::<Vec<_>>();
        // Down Kick squashes, Counter Kick and Flash Kick hop, Swhoo flashes, Swoo shrinks; the lengths and sounds.
        assert!(table.struck(415).unwrap().contains(&(Who::Target, PRESSED_BODY)));
        assert!(table.struck(419).unwrap().contains(&(Who::Target, KICKED_BODY)) && table.struck(2580).unwrap().contains(&(Who::Target, KICKED_BODY)));
        assert!(table.struck(2604).unwrap().contains(&(Who::Target, HIT_BODY)) && table.used(470).unwrap().contains(&(Who::Target, BABY_BODY)));
        assert_eq!([PRESSED_BODY, KICKED_BODY, HIT_BODY, BABY_BODY].map(|id| table.lengths[&id]), [50, 100, 100, 99_999]);
        assert_eq!(sounds(PRESSED_BODY), [("effect\\T_내려찍기", 80, false)]);
        assert_eq!(sounds(KICKED_BODY), [("effect\\EF_hit2", 272, false), ("effect\\EF_hit4", 400, false)]);
        assert!(!table.sounds.contains_key(&HIT_BODY) && !table.sounds.contains_key(&BABY_BODY));
        // They are the unit's own look: the body flags come with /effect off too (no `shows` gate on body_glow).
        let mut effects = Effects { only_forced: true, ..Effects::default() };
        let start = Instant::now();
        effects.prims.bodies.start(body::Change::Flash, Some(3), start);
        assert!(effects.body_glow(Some(3), start + crate::prims::TICK * 5).is_some());
        effects.prims.bodies.start(body::Change::Shrink, Some(3), start);
        assert!(effects.body_scale(Some(3), start + crate::prims::TICK * 50) == 0.5);
    }

    #[test]
    fn soul_link_spirit_link_and_the_landing_have_their_rows() {
        let table = Table::load();
        for skill in [445, 447, 448, 449, 450, 451, 2596, 2597, 2598, 2599] {
            assert!(table.used(skill).unwrap().contains(&(Who::Target, SOUL_BODY)), "{skill}");
        }
        assert!(table.used(471).unwrap().contains(&(Who::Target, SOUL_BODY_MONSTER)));
        for skill in [426, 2564, 2581, 5023] {
            assert!(table.used(skill).unwrap().contains(&(Who::Caster, LAND_BODY)), "{skill}");
        }
        assert_eq!([SOUL_BODY, SOUL_BODY_MONSTER, LAND_BODY].map(|id| table.lengths[&id]), [99_999_999, 99_999_999, 35]);
        assert!([SOUL_BODY, SOUL_BODY_MONSTER, LAND_BODY].iter().all(|id| !table.sounds.contains_key(id)));
    }

    #[test]
    fn the_hit_lines_and_runnings_start_and_stop_have_their_rows() {
        let table = Table::load();
        // Turn Kick and Estun: 429; Counter Kick and Flash Kick: 430 aimed from the attacker; Down Kick, Esma and Swhoo: 431.
        for skill in [417, 2593, 468] {
            assert!(table.struck(skill).unwrap().contains(&(Who::Target, HIT_LINE_TURN)), "{skill}");
        }
        for skill in [419, 2580] {
            assert!(table.struck(skill).unwrap().contains(&(Who::Target, HIT_LINE_COUNTER)), "{skill}");
        }
        for skill in [415, 469, 2604] {
            assert!(table.struck(skill).unwrap().contains(&(Who::Target, HIT_LINE_DOWN)), "{skill}");
        }
        assert!(AIMED.contains(&HIT_LINE_COUNTER) && !AIMED.contains(&HIT_LINE_TURN));
        assert!([HIT_LINE_TURN, HIT_LINE_COUNTER, HIT_LINE_DOWN].iter().all(|id| table.lengths[id] == 200 && !table.sounds.contains_key(id)));
        // Running: the use starts 442 (9 999 ticks), the status 133 plays 443 with its bounce sound, the stop 444 has its sound.
        assert!(table.used(411).unwrap().contains(&(Who::Caster, RUN_START)));
        assert_eq!((table.lengths[&RUN_START], table.lengths[&RUN_STOP]), (9999, 100));
        assert!(table.status(133).contains(&RUN_STATUS));
        assert_eq!(table.sounds[&RUN_STATUS].iter().map(|sound| (sound.file.as_str(), sound.delay_ms)).collect::<Vec<_>>(), [("effect\\T_벽튕김", 0)]);
        assert_eq!(table.sounds[&RUN_STOP].iter().map(|sound| (sound.file.as_str(), sound.delay_ms)).collect::<Vec<_>>(), [("effect\\t_효과음1", 0)]);
        assert!(!table.sounds.contains_key(&RUN_START));
        // The footprints (447 and 448) are drawn in code, with no STR.
        assert!(!table.parts.contains_key(&447) && !table.parts.contains_key(&448));
    }

    #[test]
    fn the_dark_cast_agi_up_seven_wind_and_the_body_glows_have_their_rows() {
        let table = Table::load();
        let sounds = |id: u32| table.sounds[&id].iter().map(|sound| (sound.file.as_str(), sound.delay_ms)).collect::<Vec<_>>();
        // 454: a banded cast aura like 826's in dark grey, blended; 56 ticks for a 500 ms cast, 125 for 2000 ms; no stand-in ring.
        assert!(is_cast_aura(DARK_CASTING) && is_banded(DARK_CASTING) && is_bloom(DARK_CASTING));
        assert_eq!(aura_look(DARK_CASTING), ("ring_black", [0x32; 3], false));
        assert_eq!((aura_ticks(DARK_CASTING, 500), aura_ticks(DARK_CASTING, 2000)), (56, 125));
        for skill in [2600, 2601, 2602, 2603, 2604, 2606, 2610] {
            assert_eq!(table.cast_aura(skill, 12), Some(DARK_CASTING), "{skill}");
        }
        assert!(!table.rings.contains_key(&DARK_CASTING));
        assert_eq!(sounds(DARK_CASTING), [("effect\\ef_beginspell", 0)]);
        // 456: its own sound, 100 ticks.
        assert_eq!(sounds(AGI_UP), [("effect\\EF_IncAgility", 0)]);
        assert_eq!((table.lengths[&AGI_UP], table.lengths[&SEVEN_WIND]), (100, 100));
        assert!(table.used(425).unwrap().contains(&(Who::Caster, SEVEN_WIND)) && !table.sounds.contains_key(&SEVEN_WIND));
        // 542 for Kaupe and Kaite (beside Kaite's own body); 544 for the warmth statuses.
        assert!(table.used(464).unwrap().contains(&(Who::Target, BLUE_BODY)) && table.used(465).unwrap().contains(&(Who::Target, BLUE_BODY)));
        assert!(table.used(465).unwrap().contains(&(Who::Target, 419)));
        assert!([165, 166, 167, 232, 660].iter().all(|&status| table.status(status).contains(&RED_LIGHT_BODY)));
        assert_eq!((table.lengths[&BLUE_BODY], table.lengths[&RED_LIGHT_BODY]), (200, 99_999_999));
    }

    #[test]
    fn comfort_soul_link_and_stin_have_their_rows() {
        let table = Table::load();
        let sounds = |id: u32| table.sounds[&id].iter().map(|sound| (sound.file.as_str(), sound.delay_ms)).collect::<Vec<_>>();
        for skill in [431, 432, 433] {
            assert!(table.used(skill).unwrap().contains(&(Who::Caster, COMFORT)), "{skill}");
        }
        assert_eq!(sounds(COMFORT), [("effect\\T_안락한마법", 80)]);
        for skill in [445, 447, 448, 449, 450, 451, 2596, 2597, 2598, 2599] {
            assert!(table.used(skill).unwrap().contains(&(Who::Target, SOUL_LINK_FX)), "{skill}");
        }
        assert_eq!(sounds(SOUL_LINK_FX), [("effect\\T_캐스팅", 0), ("effect\\T_영혼", 2320)]);
        assert!(AIMED.contains(&STIN) && table.struck(467).unwrap().contains(&(Who::Caster, STIN)));
        assert_eq!(sounds(STIN), [("effect\\T_바람마법", 160)]);
        assert_eq!([COMFORT, SOUL_LINK_FX, STIN].map(|id| table.lengths[&id]), [400, 500, 200]);
    }

    #[test]
    fn the_footprints_and_soul_links_sparkles_have_their_lengths() {
        let table = Table::load();
        assert_eq!([447, 448, 506].map(|id| table.lengths[&id]), [340, 340, 100]);
        assert!([447, 448, 506].iter().all(|id| !table.sounds.contains_key(id)));
    }

    #[test]
    fn estin_esma_and_estun_have_their_rows() {
        let table = Table::load();
        let sounds = |id: u32| table.sounds[&id].iter().map(|sound| (sound.file.as_str(), sound.delay_ms, sound.listener)).collect::<Vec<_>>();
        assert_eq!(table.hits[&467], [Some(BLUE_HIT), None]);
        assert_eq!(table.hits[&468], [Some(BLUE_HIT), None]);
        assert_eq!(table.hits[&2332], [Some(BLUE_HIT), Some(SKY_NET_SHAKE)]);
        assert!(AIMED.contains(&ESMA_DISCS) && table.struck(469).unwrap().contains(&(Who::Caster, ESMA_DISCS)));
        assert!(table.struck(469).unwrap().contains(&(Who::Target, ESMA_FROST)));
        assert!(AIMED.contains(&ESTUN_MOVER) && table.struck(468).unwrap().contains(&(Who::Caster, ESTUN_MOVER)));
        assert_eq!(sounds(ESMA_DISCS), [80, 320, 560, 800, 1040].map(|delay| ("effect\\T_날라차기", delay, true)));
        assert_eq!([BLUE_HIT, ESMA_DISCS, ESMA_FROST, ESTUN_MOVER].map(|id| table.lengths[&id]), [60, 200, 200, 200]);
    }

    #[test]
    fn the_warmth_full_buster_kaizel_and_kouenka_have_their_rows() {
        let table = Table::load();
        let sounds = |id: u32| table.sounds[&id].iter().map(|sound| (sound.file.as_str(), sound.delay_ms, sound.random)).collect::<Vec<_>>();
        for skill in [428, 429, 430] {
            let used = table.used(skill).unwrap();
            assert!(used.contains(&(Who::Caster, 418)) && used.contains(&(Who::Caster, WARM_SPARKLES)), "{skill}");
        }
        assert_eq!(sounds(WARM_SPARKLES), [("effect\\T_따듯한마법", 160, None)]);
        // Full Buster: aimed from the caster, the sprite from frame 1 at 4 ticks a frame, once, over everything.
        assert!(AIMED.contains(&FULL_BUSTER) && table.struck(519).unwrap().contains(&(Who::Caster, FULL_BUSTER)));
        let part = &table.sprites[&FULL_BUSTER][0];
        let animation = Animation::new(part, FULL_BUSTER, Instant::now(), Anchor::Unit(Some(1)), false);
        assert!(animation.over && !animation.repeat && animation.frame_ms == Some(64.0) && animation.first == 1.0 && animation.action.is_none());
        assert_eq!(sounds(FULL_BUSTER), [("effect\\풀버스터", 80, None)]);
        // The frame at ages 0, 3, 4, 23, 24, 27: 1, 1, 2, 6, 7, 7 (the last frame is empty); the action for d = 0 … 359.
        let frame = |age: u32| (animation_progress(16.0 * age as f32, 64.0, 8.0, 1.0, false) * 8.0) as usize;
        assert_eq!([0, 3, 4, 23, 24, 27, 28].map(frame), [1, 1, 2, 6, 7, 7, 7]);
        assert_eq!([0, 89, 90, 179, 180, 269, 270, 359].map(|d| full_buster_action(0, d)), [0, 0, 3, 3, 2, 2, 1, 1]);
        assert_eq!(full_buster_action(300, 100), 0);
        assert_eq!(full_buster_action(-30, 100), 0);
        assert_eq!(bearing_between([0.0, 0.0, 0.0], [10.0, 0.0, 0.0]), 180);
        // Kaizel's fins ride on its target; Kouenka makes no picture of its own and four crackles at 192 … 240 ms.
        assert_eq!(table.skill(462, "effectId"), [KAIZEL_FINS]);
        assert!(table.struck(534).unwrap().contains(&(Who::Target, KOUENKA)) && !table.parts.contains_key(&KOUENKA));
        assert_eq!(sounds(KOUENKA), [192, 208, 224, 240].map(|delay| ("effect\\EF_FireArrow%d", delay, Some((1, 3)))));
        assert_eq!([WARM_SPARKLES, FULL_BUSTER, KAIZEL_FINS, KOUENKA].map(|id| table.lengths[&id]), [300, 100, 200, 300]);
    }

    #[test]
    fn spear_of_ice_hidden_water_the_thunderstorm_and_the_blinks_have_their_rows() {
        let table = Table::load();
        // Spear of Ice: Earth Spike's quake and length, a sound of three, no stand-in picture.
        assert!(table.struck(537).unwrap().contains(&(Who::Target, HYOUSENSOU)) && !table.parts.contains_key(&HYOUSENSOU));
        assert_eq!(table.quakes[&HYOUSENSOU], [(Duration::ZERO, Duration::from_millis(200))]);
        assert_eq!(table.sounds[&HYOUSENSOU][0].random, Some((1, 3)));
        // Hidden Water's cells: once on each skill unit of type 187.
        assert!(table.units_once.contains(&187) && table.units[&187] == 620);
        assert!(sage::LOOPS.iter().any(|&(kept, _)| kept == sage::HIDDEN_WATER));
        // The thunderstorm: 2 ticks a frame from frame 1, looped, 99 ticks, 2.5 times as big, 31.5 pixels up, over everything.
        assert!(!table.parts.contains_key(&622) && table.used(541).unwrap().contains(&(Who::Caster, 622)));
        let animation = Animation::new(&table.sprites[&622][0], 622, Instant::now(), Anchor::Unit(Some(1)), false);
        assert!(animation.over && animation.repeat && animation.frame_ms == Some(32.0) && animation.first == 1.0);
        assert!(animation.scale == 2.5 && animation.offset == [0.0, -31.5]);
        assert_eq!(((animation.end.unwrap() - animation.start).as_secs_f32() * 1000.0).round(), 1584.0);
        let frame = |age: u32| (animation_progress(16.0 * age as f32, 32.0, 16.0, 1.0, true) * 16.0) as usize;
        assert_eq!([0, 1, 2, 29, 30, 31, 32, 98].map(frame), [1, 1, 2, 15, 0, 0, 1, 2]);
        // The blinks.
        assert!(table.used(504).unwrap().contains(&(Who::Caster, MADNESS_BLUE)));
        assert!(table.used(505).unwrap().contains(&(Who::Caster, MADNESS_RED)) && table.used(517).unwrap().contains(&(Who::Caster, MADNESS_RED)));
        assert_eq!([HYOUSENSOU, 620, 622, MADNESS_BLUE, MADNESS_RED].map(|id| table.lengths[&id]), [250, 29_999, 100, 60, 60]);
    }

    #[test]
    fn piercing_shot_kamaitachi_and_disarm_have_their_rows() {
        let table = Table::load();
        let sounds = |id: u32| table.sounds[&id].iter().map(|sound| (sound.file.as_str(), sound.delay_ms)).collect::<Vec<_>>();
        assert!(AIMED.contains(&PIERCING_LINES) && table.struck(514).unwrap().contains(&(Who::Caster, PIERCING_LINES)));
        assert!(AIMED.contains(&KAMAITACHI) && table.struck(542).unwrap().contains(&(Who::Caster, KAMAITACHI)));
        assert!(table.used(513).unwrap().contains(&(Who::Target, DISARM)));
        assert_eq!(sounds(PIERCING_LINES), [("effect\\피어싱샷", 448)]);
        assert_eq!(sounds(KAMAITACHI), [("effect\\t_바람방출", 16)]);
        assert_eq!(sounds(DISARM), [("effect\\디스암", 336)]);
        assert_eq!([PIERCING_LINES, KAMAITACHI, DISARM].map(|id| table.lengths[&id]), [300, 200, 300]);
    }

    #[test]
    fn dust_the_ninja_slashes_kaensin_and_the_dragon_and_ice_pictures_have_their_rows() {
        let table = Table::load();
        // Dust: Joint Beat's rays in code, no stand-in rings, its own sound.
        assert!(
            !table.rings.contains_key(&DUST) && table.sounds[&DUST][0].file == "effect\\더스트샷" && table.struck(518).unwrap().contains(&(Who::Target, DUST))
        );
        // The sprites: (effect, frames, ticks a frame, last shown age + 1, over everything, rise).
        let animation = |effect: u32| Animation::new(&table.sprites[&effect][0], effect, Instant::now(), Anchor::Unit(Some(1)), false);
        for (effect, frames, t, ends, over, rise) in
            [(630, 9.0, 4, 32, true, 0.0), (631, 13.0, 6, 72, false, -6.0), (632, 11.0, 4, 40, true, 0.0), (633, 30.0, 2, 58, true, -6.0)]
        {
            let animation = animation(effect);
            assert!(!animation.repeat && animation.over == over && animation.first == 1.0 && animation.offset[1] == rise, "{effect}");
            let frame_ms = animation.frame_ms.unwrap();
            assert_eq!(frame_ms, 16.0 * t as f32, "{effect}");
            let frame = |age: u32| (animation_progress(16.0 * age as f32, frame_ms, frames, 1.0, false) * frames) as usize;
            assert_eq!((frame(0), frame(t - 1), frame(t), frame(ends - 1)), (1, 1, 2, frames as usize - 1), "{effect}");
            assert_eq!((frames - 1.0) * frame_ms / 16.0, ends as f32, "{effect}");
        }
        assert_eq!(table.sprites[&631][0].file, "다다미");
        // 630 and 632 turn to one of two sides on a draw of two.
        for (effect, side) in [(630, 4), (632, 2)] {
            let mut slash = animation(effect);
            slash.roll(|_| 0);
            assert_eq!(slash.action, Some(side));
            slash.roll(|_| 1);
            assert_eq!(slash.action, Some(0));
        }
        assert!(animation(631).side_action.is_none() && animation(633).side_action.is_none());
        // Issen's sprite has 9 actions: it plays action 0, not the camera's facing.
        assert_eq!(animation(633).action, Some(0));
        // Kaensin's flame: looped from frame 1, 3 + a draw of five ticks a frame, 22 draws a copy, starts again with its unit.
        let mut flame = Animation::new(&table.sprites[&634][0], 634, Instant::now(), Anchor::Lasting(1, [0.0; 3]), true);
        assert!(flame.repeat && flame.restarts && !flame.over && flame.offset[1] == -1.0 && table.lengths[&634] == 23);
        for draw in 0..5 {
            flame.roll(|n| {
                assert_eq!(n, 5);
                draw
            });
            assert_eq!(flame.frame_ms, Some(16.0 * (3 + draw) as f32));
        }
        flame.roll(|_| 0);
        let frame = |age: u32| (animation_progress(16.0 * age as f32, 48.0, 5.0, 1.0, true) * 5.0) as usize;
        assert_eq!([0, 2, 3, 12, 14, 15, 21].map(frame), [1, 1, 2, 0, 0, 1, 3]);
        assert_eq!(table.units[&189], 634);
        assert_eq!([189, 190, 191, 192, 193, 194].map(|kind| restart_ticks(634, kind, 23)), [22; 6]);
        assert_eq!((restart_ticks(634, 188, 23), restart_ticks(0, 195, 100)), (15, 92));
        // Bakuenryu's and Hyousyouraku's pictures follow their unit.
        assert!([635, 636].iter().all(|id| table.parts[id].iter().all(|part| part.attached)));
        assert_eq!([635, 636].map(|id| table.sounds[&id][0].delay_ms), [0, 0]);
    }

    #[test]
    fn hallucination_walk_sienna_execrate_and_jack_frost_have_their_rows_and_timing() {
        let table = Table::load();
        let sounds = |id: u32| table.sounds[&id].iter().map(|sound| (sound.file.as_str(), sound.delay_ms, sound.listener)).collect::<Vec<_>>();
        // 792 EF_MAKEHALLU: length 9999, no sound, EFST 334 & 869.
        assert_eq!(table.lengths[&792], 9999);
        assert!(!table.sounds.contains_key(&792));
        assert!(table.status(334).contains(&792));
        assert!(table.status(869).contains(&792));

        // 797 EF_STONERING: Sienna Execrate (skill 2207), length 300, sound wl_siennaexecrate at tick 0 at listener.
        assert_eq!(table.lengths[&EF_STONERING], 300);
        assert_eq!(sounds(EF_STONERING), [("effect\\wl_siennaexecrate", 0, true)]);
        assert!(table.used(2207).unwrap().contains(&(Who::Target, EF_STONERING)));
        // Stapo's random action: 0 for 0, 7 for 1.
        assert_eq!(stapo_action(0), 0);
        assert_eq!(stapo_action(1), 7);
        // Stapo's frame: floor(age / 4) + 1 for ages 0..=47 (frames 1..=12), ends at age 48.
        for age in 0..48 {
            assert_eq!(stapo_frame(age), Some((age as usize / 4) + 1));
        }
        assert_eq!(stapo_frame(0), Some(1));
        assert_eq!(stapo_frame(3), Some(1));
        assert_eq!(stapo_frame(4), Some(2));
        assert_eq!(stapo_frame(47), Some(12));
        assert_eq!(stapo_frame(48), None);

        // 801 EF_FROSTDIVER3: Jack Frost (skills 2204 & 720), length 150, no wav row.
        assert_eq!(table.lengths[&EF_FROSTDIVER3], 150);
        assert!(!table.sounds.contains_key(&EF_FROSTDIVER3));
        assert!(table.used(2204).unwrap().contains(&(Who::Caster, EF_FROSTDIVER3)));
        assert!(table.used(720).unwrap().contains(&(Who::Caster, EF_FROSTDIVER3)));
    }

    #[test]
    fn zangetsu_kagemusya_misty_frost_and_prominence_kick_have_their_rows() {
        let table = Table::load();
        assert_eq!(table.status(657), [1003]);
        assert_eq!(table.status(656), [crate::actor::KAGEMUSYA]);
        assert_eq!(table.status(1141), [MISTY_FROST]);
        assert!(table.struck(2593).unwrap().contains(&(Who::Target, PROMINENCE_SPIN)));
        assert_eq!(table.sounds[&PROMINENCE_SPIN].iter().map(|sound| (sound.file.as_str(), sound.delay_ms)).collect::<Vec<_>>(), [("effect\\mon_침투경", 80)]);
        assert_eq!([1003, crate::actor::KAGEMUSYA, MISTY_FROST, PROMINENCE_SPIN].map(|id| table.lengths[&id]), [99_999_999, 99_999_999, 99_999_999, 50]);
        // The spin shares the swing's flag: on ticks 14 to 49 only, the counter from 0 to 35.
        let start = Instant::now();
        let mut brawl = brawl::Brawl::default();
        brawl.swing(Some(1), start);
        let g = |tick: u32| brawl.glow(Some(1), start + crate::prims::TICK * tick).map(|(flags, _)| flags >> crate::actor::PARAM_SHIFT);
        assert_eq!([13, 14, 49, 50].map(g), [None, Some(0), Some(35), None]);
    }

    #[test]
    fn the_gunslingers_sprites_ground_drifts_spark_and_the_grey_have_their_rows() {
        let table = Table::load();
        let sounds = |id: u32| table.sounds[&id].iter().map(|sound| (sound.file.as_str(), sound.delay_ms, sound.listener)).collect::<Vec<_>>();
        // Each: the effect, ticks a frame, the sprite's frame count, the ages to read the frame at, the frames, and the end age.
        type Row = (u32, f32, f32, [u32; 4], [usize; 4]);
        let rows: [Row; 6] = [
            (637, 4.0, 24.0, [0, 3, 4, 91], [1, 1, 2, 23]),
            (643, 4.0, 20.0, [0, 3, 4, 75], [1, 1, 2, 19]),
            (644, 2.0, 21.0, [0, 1, 2, 39], [1, 1, 2, 20]),
            (645, 4.0, 12.0, [0, 3, 4, 43], [1, 1, 2, 11]),
            (647, 2.0, 15.0, [0, 1, 2, 27], [1, 1, 2, 14]),
            (648, 2.0, 15.0, [0, 1, 2, 27], [1, 1, 2, 14]),
        ];
        for (effect, ticks, frames, ages, want) in rows {
            let animation = Animation::new(&table.sprites[&effect][0], effect, Instant::now(), Anchor::Unit(Some(1)), false);
            assert!(animation.over && !animation.repeat && animation.first == 1.0 && animation.scale == 1.0 && animation.offset == [0.0, 0.0], "{effect}");
            assert_eq!(animation.frame_ms, Some(16.0 * ticks), "{effect}");
            let frame = |age: u32| (animation_progress(16.0 * age as f32, 16.0 * ticks, frames, 1.0, false) * frames) as usize;
            assert_eq!(ages.map(frame), want, "{effect}");
            // It ends after the frames it shows: (count − 1) × ticks.
            assert_eq!((frames - 1.0) * ticks, (ages[3] + 1) as f32, "{effect}");
        }
        // Triple Action is made 30 ticks late.
        let triple = Animation::new(&table.sprites[&648][0], 648, Instant::now(), Anchor::Unit(Some(1)), false);
        assert_eq!(((triple.start - Instant::now()).as_secs_f32() * 1000.0).round(), 480.0);
        // The rows hold: where they play, their sounds and lengths.
        assert!(table.used(516).unwrap().contains(&(Who::Caster, 637)) && table.used(2565).unwrap().contains(&(Who::Caster, 637)));
        for (skill, who, effect) in
            [(515, Who::Target, 643), (507, Who::Target, 644), (512, Who::Target, 647), (502, Who::Target, 648), (520, Who::Caster, 645)]
        {
            assert!(table.struck(skill).unwrap().contains(&(who, effect)), "{skill}");
        }
        assert_eq!(sounds(637), [("effect\\데스페라도", 160, false)]);
        assert_eq!(sounds(643), [("effect\\래피드샤워", 320, false)]);
        assert_eq!(sounds(644), [("effect\\매지컬블릿", 0, false)]);
        assert_eq!(sounds(648), [("effect\\트리플액션", 400, true)]);
        assert!(!table.sounds.contains_key(&645) && !table.sounds.contains_key(&647));
        assert_eq!([637, 643, 644, 645, 647, 648].map(|id| table.lengths[&id]), [200, 100, 100, 100, 100, 100]);
        // Spread Attack is aimed; its action for the sums 0, 89, 90, 179, 180, 269, 270, 359.
        assert!(AIMED.contains(&SPREAD_ATTACK));
        assert_eq!([0, 89, 90, 179, 180, 269, 270, 359].map(|d| spread_attack_action(0, d)), [0, 0, 6, 6, 4, 4, 2, 2]);
        // Ground Drift's wind: one frame of action 0 looped, 1 up, depth tested; the next copy a tick early.
        let spark = Animation::new(&table.sprites[&638][0], 638, Instant::now(), Anchor::Lasting(1, [0.0; 3]), true);
        assert!(spark.repeat && !spark.over && spark.action == Some(0) && spark.offset == [0.0, -7.0]);
        assert_eq!(table.lengths[&638], 23);
        assert_eq!(restart_ticks(638, 190, 23), 22);
        assert!((189..=194).all(|kind| restart_ticks(0, kind, 100) == 99) && restart_ticks(0, 195, 100) == 92);
        // The grey: the three statuses play it, once each.
        assert!([606, 655, 659].iter().all(|&status| table.status(status).contains(&GREY_BODY)));
        assert_eq!(table.lengths[&GREY_BODY], 99_999);
    }

    #[test]
    fn circle_of_fire_the_funnels_the_pink_bloom_moonlight_and_castling_have_their_rows() {
        let table = Table::load();
        let sounds = |id: u32| table.sounds[&id].iter().map(|sound| (sound.file.as_str(), sound.delay_ms)).collect::<Vec<_>>();
        // 336: the struck row stays, the three stand-in rings are gone.
        assert!(table.struck(8401).unwrap().contains(&(Who::Target, CIRCLE_GUARD)) && !table.rings.contains_key(&CIRCLE_GUARD));
        assert_eq!(sounds(CIRCLE_GUARD), [("effect\\kyrie_guard", 0)]);
        // 460 on Hurricane Rage's target at 80 ms; 465 on Zephyr's cell, starting again every 999 ticks.
        assert!(table.struck(8435).unwrap().contains(&(Who::Target, HURRICANE_RAGE)));
        assert_eq!(sounds(HURRICANE_RAGE), [("ice_titan_attack", 80)]);
        assert_eq!(table.units[&239], ZEPHYR);
        assert_eq!((restart_ticks(ZEPHYR, 239, 1000), restart_ticks(0, 238, 100), restart_ticks(0, 245, 100)), (999, 92, 92));
        assert_eq!([CIRCLE_GUARD, HURRICANE_RAGE, ZEPHYR].map(|id| table.lengths[&id]), [200, 100, 1000]);
        // 564: a pink bloom of ring_red for 60 ticks on both of the Filir's skills, two sets of four bands starting at alpha 0.
        assert!(table.used(8010).unwrap().contains(&(Who::Caster, HOMUN_CASTING)) && table.used(8011).unwrap().contains(&(Who::Caster, HOMUN_CASTING)));
        assert!(is_cast_aura(HOMUN_CASTING) && is_banded(HOMUN_CASTING) && is_bloom(HOMUN_CASTING));
        assert_eq!(aura_look(HOMUN_CASTING), ("ring_red", [0xFF, 0x59, 0xB6], true));
        assert_eq!((aura_ticks(HOMUN_CASTING, 0), table.lengths[&HOMUN_CASTING]), (60, 60));
        assert_eq!(sounds(HOMUN_CASTING), [("effect\\ef_beginspell", 0)]);
        // Band k begins 5k ticks late with nothing drawn at first; its first tick is alpha 10.
        for band in 0..4 {
            assert!(bloom(HOMUN_CASTING, 0, band, 60, 5 * band as u32).is_none() && bloom(HOMUN_CASTING, 1, band, 60, 5 * band as u32).is_none());
            let first = bloom(HOMUN_CASTING, 0, band, 60, 5 * band as u32 + 1).unwrap();
            assert_eq!(first.alpha, 10.0);
        }
        // Moonlight's slashes by the hit count 1, 2, 3, 4, 10.
        assert_eq!([1, 2, 3, 4, 10].map(moonlight_effect), [565, 566, 567, 567, 567]);
        assert!(table.struck(8012).unwrap().contains(&(Who::Target, 567)));
        assert!(table.parts.contains_key(&565) && table.parts.contains_key(&566) && table.parts.contains_key(&567));
        // 570: two ticks a frame from frame 1, once, over everything, one sound a use.
        let castling = Animation::new(&table.sprites[&570][0], 570, Instant::now(), Anchor::Unit(Some(1)), false);
        assert!(castling.over && !castling.repeat && castling.frame_ms == Some(32.0) && castling.first == 1.0);
        let frame = |age: u32| (animation_progress(16.0 * age as f32, 32.0, 13.0, 1.0, false) * 13.0) as usize;
        assert_eq!([0, 1, 2, 23].map(frame), [1, 1, 2, 12]);
        assert!(table.used(8005).unwrap().contains(&(Who::Caster, 570)) && table.used(8005).unwrap().contains(&(Who::Target, 570)));
        assert!(!table.sounds.contains_key(&570));
        assert_eq!(table.skill_sounds[&8005].iter().map(|sound| sound.file.as_str()).collect::<Vec<_>>(), ["effect\\h_castling"]);
        assert_eq!(table.lengths[&570], 100);
    }

    #[test]
    fn the_spark_movers_ground_drifts_balls_and_chase_walks_prints_have_their_rows() {
        let table = Table::load();
        // No row starts 551 or 1204 (the server's packet does), and 552, 1205 and 556 are launched by the movers only.
        let launched_by_rows = |effect: u32| {
            [table.used.values(), table.struck.values(), table.grounds.values()].into_iter().flatten().flatten().any(|&(_, each)| each == effect)
                || table.skills.values().any(|effects| effects.contains(&effect))
                || table.units.values().any(|&each| each == effect)
        };
        assert!(![SPARK_MOVER, SPARK_MOVER_DARK, 552, 1205, 556].iter().any(|&effect| launched_by_rows(effect)));
        assert_eq!([SPARK_MOVER, SPARK_MOVER_DARK, 556].map(|id| table.lengths[&id]), [100, 100, 50]);
        assert!(!table.sounds.contains_key(&SPARK_MOVER) && !table.sounds.contains_key(&556));
        // Ground Drift's four element balls: Kaensin's pattern, their own sprites, 22 ticks between copies.
        for (effect, kind, name) in [(639, 191, "블라인드스피어"), (640, 192, "포이즌스피어"), (641, 193, "프리징스피어"), (642, 194, "플레어스피어")]
        {
            assert_eq!((table.units[&kind], table.lengths[&effect]), (effect, 23), "{effect}");
            let part = &table.sprites[&effect][0];
            assert_eq!((part.file.as_str(), part.attached), (name, false), "{effect}");
            let animation = Animation::new(part, effect, Instant::now(), Anchor::Lasting(1, [0.0; 3]), true);
            assert!(animation.repeat && !animation.over && animation.action == Some(0) && animation.offset == [0.0, -7.0], "{effect}");
            assert_eq!(restart_ticks(effect, kind, 23), 22, "{effect}");
        }
        // Chase Walk's prints are drawn in code: no part rows, 340 ticks.
        assert!(!table.floaters.contains_key(&326) && !table.floaters.contains_key(&327));
        assert_eq!([326, 327].map(|id| table.lengths[&id]), [340, 340]);
    }

    #[test]
    fn recognized_spell_and_tetra_vortex_have_their_rows() {
        let table = Table::load();
        // 803 Recognized Spell: cast 2206 803 and cast 2206 58 both there and skill 2206 effectId 803 gone.
        assert_eq!(table.cast_aura(2206, 58), Some(58));
        assert_eq!(table.cast_extras(2206), [EF_RECOGNIZED]);
        assert!(table.skill(2206, "effectId").is_empty());
        assert_eq!(table.lengths[&EF_RECOGNIZED], 500);

        // 804 Tetra Vortex: wav 804 rows at 0 (two) and 160, 320, 800, 1280 ms and none other; func 804 quake at 0, 640, 1280 ms for 650 ms; struck 2218 caster 804 kept.
        assert_eq!(table.lengths[&EF_TETRA], 600);
        let wav = table.sounds[&EF_TETRA].iter().map(|s| (s.file.as_str(), s.delay_ms)).collect::<Vec<_>>();
        assert_eq!(
            wav,
            [
                ("effect\\wizard_meteo", 0),
                ("effect\\hunter_blastmine", 0),
                ("effect\\wl_tetravortex", 160),
                ("effect\\wl_tetravortex", 320),
                ("effect\\wl_tetravortex", 800),
                ("effect\\wl_tetravortex", 1280),
            ]
        );
        let quakes = table.quakes[&EF_TETRA].iter().map(|&(delay, len)| (delay.as_millis() as u32, len.as_millis() as u32)).collect::<Vec<_>>();
        assert_eq!(quakes, [(0, 650), (640, 650), (1280, 650)]);
        assert!(table.struck(2218).unwrap().contains(&(Who::Caster, EF_TETRA)));
        assert!(AIMED.contains(&EF_TETRA));
    }

    #[test]
    fn eswhoo_kaute_and_soul_unity_have_their_rows() {
        let table = Table::load();
        assert!(AIMED.contains(&ESWHOO_DISCS) && table.struck(2604).unwrap().contains(&(Who::Caster, ESWHOO_DISCS)));
        assert_eq!(
            table.sounds[&ESWHOO_DISCS].iter().map(|sound| (sound.file.as_str(), sound.delay_ms, sound.listener)).collect::<Vec<_>>(),
            [80, 320, 560, 800, 1040].map(|delay| ("effect\\T_날라차기", delay, true))
        );
        assert!(table.used(2612).unwrap().contains(&(Who::Target, KAUTE_PALE)) && table.used(2612).unwrap().contains(&(Who::Caster, KAUTE_DARK)));
        assert!(AIMED.contains(&SOUL_UNITY) && table.used(2605).unwrap().contains(&(Who::Target, SOUL_UNITY)));
        assert!(!table.sounds.contains_key(&KAUTE_PALE) && !table.sounds.contains_key(&KAUTE_DARK) && !table.sounds.contains_key(&SOUL_UNITY));
        assert_eq!([ESWHOO_DISCS, KAUTE_PALE, KAUTE_DARK, SOUL_UNITY].map(|id| table.lengths[&id]), [200; 4]);
    }

    #[test]
    fn cold_slower_and_stasis_have_their_rows() {
        let table = Table::load();
        // 791 EF_COLDTHROW: Cold Slower (skill 2260), length 200, sound ice_titan_attack at tick 0 at listener.
        assert_eq!(table.lengths[&EF_COLDTHROW], 200);
        let sounds_791 = table.sounds[&EF_COLDTHROW].iter().map(|s| (s.file.as_str(), s.delay_ms, s.listener)).collect::<Vec<_>>();
        assert_eq!(sounds_791, [("ice_titan_attack", 0, true)]);
        assert!(table.grounds[&2260].contains(&(Who::Caster, EF_COLDTHROW)));

        // 799 EF_STASIS: Stasis (skill 2209, status 453), length 300, sound wl_stasis at tick 6 (96 ms) at listener.
        assert_eq!(table.lengths[&EF_STASIS], 300);
        let sounds_799 = table.sounds[&EF_STASIS].iter().map(|s| (s.file.as_str(), s.delay_ms, s.listener)).collect::<Vec<_>>();
        assert_eq!(sounds_799, [("effect\\wl_stasis", 96, true)]);
        assert!(table.used(2209).unwrap().contains(&(Who::Caster, EF_STASIS)));
        assert!(table.status(453).contains(&EF_STASIS));
    }

    #[test]
    fn summon_slave_earthquake_dragon_fear_and_wide_bleeding_have_their_rows_and_timing() {
        let table = Table::load();
        let ms = |duration: Duration| duration.as_millis();
        let sounds = |id: u32| table.sounds[&id].iter().map(|sound| (sound.file.as_str(), sound.delay_ms)).collect::<Vec<_>>();
        // 215: on the monster at the use, length 500, no sound; the smoke puff is made at tick 35 (560 ms) where it stood, once.
        assert!(table.used(196).unwrap().contains(&(Who::Caster, SUMMON_SLAVE)));
        assert_eq!(table.lengths[&SUMMON_SLAVE], 500);
        assert!(!table.sounds.contains_key(&SUMMON_SLAVE));
        let puff = &table.sprites[&SUMMON_SLAVE][0];
        assert!(puff.file == "smoke" && !puff.attached && puff.parameters["delay"] == "560");
        let now = Instant::now();
        let puff = Animation::new(puff, SUMMON_SLAVE, now, Anchor::Unit(Some(1)), false);
        assert_eq!(ms(puff.start - now), 560);
        assert!(!puff.repeat && puff.end.is_none() && table.sprites[&SUMMON_SLAVE].len() == 1);
        // A row without `delay` starts at once.
        let plain = SpritePart { file: "smoke".to_owned(), attached: true, parameters: HashMap::new() };
        assert_eq!(Animation::new(&plain, SUMMON_SLAVE, now, Anchor::Unit(Some(1)), false).start, now);

        // 666: on the caster at each damage packet, length 100, the quake sound and three shakes as before.
        assert!(table.struck(653).unwrap().contains(&(Who::Caster, 666)) && table.struck(750).unwrap().contains(&(Who::Caster, 666)));
        assert_eq!(table.lengths[&666], 100);
        assert_eq!(sounds(666), [("effect\\earth_quake", 0)]);
        assert_eq!(table.quakes[&666].iter().map(|(delay, _)| ms(*delay)).collect::<Vec<_>>(), [0, 560, 1120]);
        let crack = &table.sprites[&666][0];
        assert!(crack.attached && table.sprites[&666].len() == 1);
        let crack = Animation::new(crack, 666, now, Anchor::Unit(Some(1)), false);
        assert!(!crack.repeat && crack.over && crack.first == 1.0 && crack.action == Some(0) && crack.scale == 1.0 && crack.alpha == 1.0);
        assert_eq!((crack.frame_ms, crack.offset), (Some(64.0), [0.0, 0.0]));
        // Frame ⌊a / 4⌋ + 1 of its 31: 1 at ages 0 and 3, 2 at 4, 25 at 96 and 98; 99 draws.
        let frame = |age: u32| (animation_progress(16.0 * age as f32, 64.0, 31.0, 1.0, false) * 31.0) as usize;
        assert_eq!([0, 3, 4, 96, 98].map(frame), [1, 1, 2, 25, 25]);
        assert_eq!(ms(crack.end.unwrap() - crack.start), 1584);

        // 668: dfear.str held to its length 28, the roar and the shake at tick 1.
        assert!(table.struck(659).unwrap().contains(&(Who::Caster, 668)));
        assert_eq!(table.lengths[&668], 28);
        let rings = &table.parts[&668];
        assert!(rings.len() == 1 && rings[0].file == "dfear" && rings[0].attached && rings[0].hold && !rings[0].once);
        assert_eq!(sounds(668), [("effect\\dragonfear", 16)]);
        assert_eq!(table.quakes[&668].iter().map(|(delay, _)| ms(*delay)).collect::<Vec<_>>(), [16]);

        // 669: wideb.str held to its length 70, the sound at tick 1.
        assert!(table.struck(665).unwrap().contains(&(Who::Caster, 669)) && table.struck(756).unwrap().contains(&(Who::Caster, 669)));
        assert_eq!(table.lengths[&669], 70);
        let claws = &table.parts[&669];
        assert!(claws.len() == 1 && claws[0].file == "wideb" && claws[0].attached && claws[0].hold);
        assert_eq!(sounds(669), [("effect\\wideb", 16)]);
        // Other pictures are not held.
        assert!(table.parts[&10].iter().all(|part| !part.hold));
    }

    #[test]
    fn a_held_picture_keeps_its_last_frame_to_the_effects_length() {
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
        let layer = |last: i32| rustnarok_client::effect::Layer { textures: Vec::new(), keys: vec![key, rustnarok_client::effect::Key { frame: last, ..key }] };
        // Dragon Fear: the last key is at frame 23, the effect lasts 28 ticks; a layer that ended earlier is not drawn at the end.
        let str = Str { fps: 60, frames: 24, layers: vec![layer(11), layer(23)] };
        assert_eq!(
            (playing_ticks(668, &str, Some(28), true), playing_ticks(668, &str, Some(28), false), playing_ticks(668, &str, Some(10), true)),
            (28, 24, 10)
        );
        let shown = |frame: u32| str.layers.iter().map(|layer| layer.state(held_frame(&str, true, frame as f32)).is_some()).collect::<Vec<_>>();
        assert_eq!([23, 24, 27].map(shown), [[false, true]; 3]);
        // Frame 23 and 24 … 27 are the same; a picture not held goes on counting.
        assert!((24..28).all(|frame| held_frame(&str, true, frame as f32) == 23.0) && held_frame(&str, false, 24.0) == 24.0);
        assert_eq!(held_frame(&str, true, 5.0), 5.0);
        // Nothing is drawn from the tick after the length.
        assert!(playing_ticks(669, &str, None, true) == 24);
    }
}
