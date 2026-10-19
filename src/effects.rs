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
use crate::render::{Gpu, Texture};
use crate::world::{self, CELL, EffectMesh, EffectQuad, MeshVertex, Vector};

/// A quad's corners as texture coordinates: top left, top right, bottom right, bottom left.
const QUAD_UV: [[f32; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];

/// An STR moves on one frame each 16 ms tick, whatever its file says.
const STR_TICK_MS: f32 = 16.0;
/// An STR's (320, 240) sits this many screen pixels above its owner's feet, at any zoom.
pub const STR_RISE: f32 = 80.0;

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
}

impl Table {
    pub fn load() -> Self {
        let mut table = Self {
            parts: HashMap::new(),
            floaters: HashMap::new(),
            rings: HashMap::new(),
            sprites: HashMap::new(),
            quakes: HashMap::new(),
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
                    let sound = Sound { file: file.to_owned(), delay_ms: number(3).map_or(0, u64::from), random: number(4).zip(number(5)) };
                    table.sounds.entry(id).or_default().push(sound);
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

/// How often a ground unit's effect starts again, in ticks: 8 before its length ends (Quagmire's 20).
fn restart_ticks(effect: u32, length: u32) -> u32 {
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
    /// Its images now, from `advance`.
    placed: Vec<Placed>,
}

impl Animation {
    fn new(part: &SpritePart, start: Instant, anchor: Anchor, lasting: bool) -> Self {
        let number = |name: &str| part.parameters.get(name).and_then(|value| value.parse::<f32>().ok());
        let flag = |name: &str| part.parameters.get(name).is_some_and(|value| value == "true");
        let rise = if flag("head") { -100.0 } else { 0.0 };
        let repeat = flag("repeat") || lasting;
        Self {
            sprite: format!("data\\sprite\\이팩트\\{}", part.file),
            start,
            end: number("duration").filter(|_| repeat).map(|ms| start + Duration::from_secs_f32(ms.max(0.0) / 1000.0)),
            anchor,
            action: number("frame").map(|action| action as usize),
            frame_ms: number("delayFrame").map(|ms| ms.max(1.0)),
            repeat,
            hold: flag("stopAtEnd"),
            offset: [number("xOffset").unwrap_or(0.0), number("yOffset").unwrap_or(rise)],
            placed: Vec::new(),
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
}

impl Ring {
    fn new(parameters: &HashMap<String, String>, start: Instant, anchor: Anchor) -> Option<Self> {
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

/// EF_HIT1, made in code: a blue ring drifting off the target and four sparks flying out with short trails. Everything
/// moves once a tick (16 ms); it lasts 50 ticks and follows its unit.
struct HitSpark {
    start: Instant,
    anchor: Anchor,
    /// The way the ring drifts across the ground (x east, z north), a unit vector.
    way: [f32; 2],
    sparks: [Spark; 4],
    /// The spark sprite's images, from `advance`.
    placed: Vec<Placed>,
}

/// One of a hit's sparks, its random choices made.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Spark {
    /// Its heading, a unit vector (y up).
    heading: Vector,
    /// How far out along its heading it starts, and how fast it goes a tick at first (falling evenly to half).
    out: f32,
    speed: f32,
    /// How fast it rises a tick at first: the back pair only.
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
/// The ring's middle and the sparks' start, above the feet; the ring's inner and outer radius.
const HIT_HEIGHT: f32 = 10.0;
const RING_RADII: [f32; 2] = [5.0, 10.0];
const RING_SIDES: usize = 24;
/// Each spark is drawn this many times, where it was that many ticks ago.
const TRAIL: u32 = 3;
const NEAR_OPAQUE: f32 = 254.0 / 255.0;

/// The way a hit's ring drifts for a target facing `facing` (0 south, turning west): straight away from its face.
// ponytail: the game turns the ring by 180° − the facing; which way that points in these axes is a choice.
fn drift(facing: u8) -> [f32; 2] {
    let (sin, cos) = (f32::from(facing % 8) * 45.0).to_radians().sin_cos();
    [sin, cos]
}

/// The ring's alpha at a tick: 254 until tick 5, then straight down to 0 at tick 10.
fn ring_alpha(tick: u32) -> f32 {
    NEAR_OPAQUE * ((RING_TICKS as f32 - tick as f32) / 5.0).clamp(0.0, 1.0)
}

/// The ring's speed at a tick: 0.7 units, slowing evenly to 0.35 by its end.
fn ring_speed(tick: u32) -> f32 {
    0.7 - 0.35 * tick as f32 / RING_TICKS as f32
}

/// How far the ring has drifted by a tick.
fn ring_travel(tick: u32) -> f32 {
    (0..tick.min(RING_TICKS)).map(ring_speed).sum()
}

/// A hit's four sparks for a ring drifting `way`: two within ±40° of it and two the other way (140°…219° off), each
/// pitched −50°…49°; `random(from, to)` picks every range.
fn sparks(way: [f32; 2], mut random: impl FnMut(f32, f32) -> f32) -> [Spark; 4] {
    let toward = way[0].atan2(way[1]);
    std::array::from_fn(|index| {
        let back = index >= 2;
        let turn = toward + if back { random(140.0, 220.0) } else { random(-40.0, 40.0) }.to_radians();
        let (sin, cos) = random(-50.0, 50.0).to_radians().sin_cos();
        Spark {
            heading: [turn.sin() * cos, sin, turn.cos() * cos],
            out: random(4.0, 7.0),
            speed: random(0.6, 1.5),
            rise: if back { random(0.3, 1.2) } else { 0.0 },
            scale: random(0.6, 1.6),
            life: random(6.0, 30.0) as u32,
        }
    })
}

impl Spark {
    fn speed_at(&self, tick: u32) -> f32 {
        self.speed * (1.0 - 0.5 * tick as f32 / self.life as f32)
    }

    // ponytail: the rise falls by 2 × its start / life a tick, taken at each tick's middle so the back pair lands
    // exactly back at its start height.
    fn rise_at(&self, tick: u32) -> f32 {
        self.rise - 2.0 * self.rise / self.life as f32 * (tick as f32 + 0.5)
    }

    /// Where it is at a tick, from the feet.
    fn place(&self, tick: u32) -> Vector {
        let ticks = 0..tick.min(self.life);
        let out = self.out + ticks.clone().map(|tick| self.speed_at(tick)).sum::<f32>();
        let up = HIT_HEIGHT + ticks.map(|tick| self.rise_at(tick)).sum::<f32>();
        [self.heading[0] * out, self.heading[1] * out + up, self.heading[2] * out]
    }

    /// Its alpha at a tick: 254 straight down to 0 over its life.
    fn alpha(&self, tick: u32) -> f32 {
        NEAR_OPAQUE * (1.0 - tick as f32 / self.life as f32).max(0.0)
    }

    /// Its trail at a tick: each copy's place from the feet and alpha, older ones fainter.
    // ponytail: copy i is where it was i ticks ago at alpha × (3 − i) / 3.
    fn trail(&self, tick: u32) -> Vec<(Vector, f32)> {
        if tick >= self.life {
            return Vec::new();
        }
        (0..TRAIL.min(tick + 1)).map(|copy| (self.place(tick - copy), self.alpha(tick) * (TRAIL - copy) as f32 / TRAIL as f32)).collect()
    }
}

impl HitSpark {
    /// Whole ticks since it started.
    fn tick(&self, now: Instant) -> u32 {
        (now.saturating_duration_since(self.start).as_secs_f32() * 1000.0 / HIT_TICK_MS) as u32
    }

    /// The ring's triangles at a tick, the feet `at`: a flat ring standing upright across its way.
    fn ring(&self, tick: u32, at: Vector) -> Vec<MeshVertex> {
        let travel = ring_travel(tick);
        let middle = [at[0] + self.way[0] * travel, at[1] + HIT_HEIGHT, at[2] + self.way[1] * travel];
        let tint = [1.0, 1.0, 1.0, ring_alpha(tick)];
        // Around in the upright plane across the way; the texture's v runs from the inner edge (1) to the outer (0).
        let point = |side: usize, edge: usize| {
            let share = side as f32 / RING_SIDES as f32;
            let (sin, cos) = (share * std::f32::consts::TAU).sin_cos();
            let radius = RING_RADII[edge];
            MeshVertex {
                position: [middle[0] + self.way[1] * radius * cos, middle[1] + radius * sin, middle[2] - self.way[0] * radius * cos],
                uv: [share, 1.0 - edge as f32],
                tint,
            }
        };
        (0..RING_SIDES)
            .flat_map(|side| {
                let [inner, outer, next_inner, next_outer] = [point(side, 0), point(side, 1), point(side + 1, 0), point(side + 1, 1)];
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
    /// Heal's two bands' start turns, picked at random.
    turns: [f32; 2],
    /// Heal's sparkles: the tick each starts and where on the ground, from the feet.
    sparkles: Vec<(u32, [f32; 2])>,
}

/// A cast's aura: one a caster has at a time.
pub fn is_cast_aura(effect: u32) -> bool {
    matches!(effect, 12 | 54..=59)
}

/// The effects made of bands.
pub fn is_banded(effect: u32) -> bool {
    matches!(effect, 12 | 54..=59 | 304 | 312 | 313 | 314 | 325 | 344)
}

/// How long a band effect lasts, in ticks: Heal and the arrival 100, the teleport and logout 200.
fn banded_ticks(effect: u32) -> u32 {
    match effect {
        304 | 314 => 200,
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
    (cast_ms / 16).max(if matches!(effect, 12 | 58) { 56 } else { 70 })
}

/// A band effect's texture, colour and whether it adds on.
fn aura_look(effect: u32) -> (&'static str, [u8; 3], bool) {
    match effect {
        312 | 313 | 325 => ("alpha_down", [0x8C, 0xD2, 0x8C], true),
        304 => ("Magic_Violet", [0x64, 0x64, 0xFF], true),
        314 => ("ring_purple", [0x64, 0x64, 0xFF], true),
        344 => ("ring_blue", [0x64, 0x64, 0xFF], true),
        58 => ("ring_white", [0xFF, 0xFF, 0xFF], true),
        54 => ("ring_blue", [0xC3, 0xC3, 0xFF], false),
        55 => ("ring_yellow", [0xFF, 0xAF, 0xAF], true),
        57 => ("ring_yellow", [0xFF, 0xFF, 0xFF], false),
        56 => ("Magic_Green", [0xFF, 0xFF, 0xFF], false),
        59 => ("Magic_Violet", [0xFF, 0xFF, 0xFF], false),
        _ => ("ring_yellow", [0xFF, 0xFF, 0xAA], true),
    }
}

fn aura_texture(effect: u32) -> String {
    format!("data\\texture\\effect\\{}.tga", aura_look(effect).0)
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

impl Banded {
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
            304 => return shaped(&teleport_bands(self.ticks, age), full_height),
            // ponytail: the logout stands as the teleport's columns, with the fade.
            314 => return shaped(&teleport_bands(self.ticks, age).map(|band| Band { alpha: band_fade(age), ..band }), rising_height),
            344 => return shaped(&arrival_bands(age), rising_height),
            _ => {}
        }
        if matches!(self.effect, 12 | 58) {
            (0..2)
                .flat_map(|set| (0..4).filter_map(move |band| bloom(set, band, self.ticks, age).map(|shape| (band, shape))))
                .map(|(band, shape)| (shape, heights(band, shape.height, bloom_height)))
                .collect()
        } else {
            // ponytail: earth (56) and poison (59) turn like water, fire and wind.
            (0..4).map(|band| turning(band, self.ticks, age)).enumerate().map(|(band, shape)| (shape, heights(band, shape.height, turning_height))).collect()
        }
    }
}

#[derive(Default)]
pub struct Effects {
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
    /// The game's hand-made effects (Soul Strike's ghosts), and the ghost sprite's layers.
    prims: crate::prims::Prims,
    ghost: Vec<Placed>,
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
    fn new(parameters: &HashMap<String, String>, copy: u32, start: Instant, anchor: Anchor) -> Option<Self> {
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

/// EF_COLDHIT: Cold Bolt's, Frost Diver's and Storm Gust's hit, ice shards and smoke drawn on the screen (`prims`).
pub const COLD_HIT: u32 = 51;

/// EF_SOULSTRIKE: ghosts from the caster to the target, a hit each (`prims`).
pub const SOUL_STRIKE: u32 = 15;

/// Cold Bolt's and Fire Bolt's falling arrows (EF_ICEARROW, EF_FIREARROW).
const BOLTS: [u32; 2] = [26, 31];

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
        self.play_sounds(table, id, at, now);
        self.show(gpu, grf, table, id, target, at, now);
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
        for drop in drops {
            self.show(gpu, grf, table, id, target, at, now + drop);
        }
    }

    fn play_sounds(&mut self, table: &Table, id: u32, at: Vector, now: Instant) {
        for sound in table.sounds.get(&id).into_iter().flatten() {
            let file = match sound.random {
                Some((from, to)) => sound.file.replace("%d", &pick(from, to).to_string()),
                None => sound.file.clone(),
            };
            self.sounds.push((now + Duration::from_millis(sound.delay_ms), format!("{file}.wav"), at));
        }
    }

    /// What effect `id` draws, from `now` on.
    #[allow(clippy::too_many_arguments)]
    fn show(&mut self, gpu: &Gpu, grf: &Grf, table: &Table, id: u32, target: Target, at: Vector, now: Instant) {
        // COLDHIT is drawn in code, flat on the screen over its unit.
        if let (COLD_HIT, Target::Unit(unit)) = (id, target) {
            for path in [crate::prims::ICE_TEXTURE, crate::prims::SMOKE_TEXTURE] {
                self.textures.entry(path.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, path, true).ok());
            }
            self.prims.cold_hit(unit, now, |n| pick(0, n.saturating_sub(1)));
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
                let Some(mut floating) = Floating::new(parameters, copy, start, anchor(attached(parameters))) else { continue };
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
                let Some(mut ring) = Ring::new(parameters, start, anchor(attached(parameters))) else { continue };
                ring.again |= again(parameters);
                self.textures.entry(ring.texture.clone()).or_insert_with(|| gpu.try_load_texture(grf, &ring.texture, true).ok());
                self.rings.push(ring);
            }
        }
        for part in table.sprites.get(&id).into_iter().flatten() {
            let animation = Animation::new(part, now, anchor(part.attached), lasting);
            self.sprites.entry(animation.sprite.clone()).or_insert_with(|| SpriteSet::at(grf, &animation.sprite));
            self.animations.push(animation);
        }
        self.play_quakes(table, id, now);
        // Heal, the teleport, the arrival and the logout: bands made in code (a cast's aura comes by `play_aura`).
        if is_banded(id) && !is_cast_aura(id) {
            for path in [format!("data\\texture\\effect\\{}.tga", aura_look(id).0), SPARKLE_TEXTURE.to_owned()] {
                self.textures.entry(path.clone()).or_insert_with(|| gpu.try_load_texture(grf, &path, true).ok());
            }
            let healing = matches!(id, 312 | 313 | 325);
            self.auras.push(Banded {
                effect: id,
                anchor: anchor(true),
                start: now,
                ticks: banded_ticks(id),
                turns: [random(0.0, 360.0), random(0.0, 360.0)],
                sparkles: if healing { heal_sparkles(random) } else { Vec::new() },
            });
        }
        for part in table.parts.get(&id).into_iter().flatten() {
            // Every file a `%d` names is loaded, so each restart can pick again.
            let files: Vec<String> = match part.random {
                Some((from, to)) => (from..=to).map(|number| part.file.replace("%d", &number.to_string())).collect(),
                None => vec![part.file.clone()],
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

    /// EF_HIT1 on a unit (None = the player) facing `facing`, from `start`.
    pub fn play_hit(&mut self, gpu: &Gpu, grf: &Grf, unit: Option<u32>, facing: u8, start: Instant) {
        self.textures.entry(RING_TEXTURE.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, RING_TEXTURE, true).ok());
        self.sprites.entry(SPARK_SPRITE.to_owned()).or_insert_with(|| SpriteSet::at(grf, SPARK_SPRITE));
        let way = drift(facing);
        self.hits.push(HitSpark { start, anchor: Anchor::Unit(unit), way, sparks: sparks(way, random), placed: Vec::new() });
    }

    /// A cast's aura (12 or 54–59) on a unit (None = the player) for a cast of `cast_ms`, replacing its last one.
    pub fn play_aura(&mut self, gpu: &Gpu, grf: &Grf, unit: Option<u32>, effect: u32, cast_ms: u32, start: Instant) {
        let texture = aura_texture(effect);
        self.textures.entry(texture.clone()).or_insert_with(|| gpu.try_load_texture(grf, &texture, true).ok());
        let anchor = Anchor::Unit(unit);
        self.auras.retain(|aura| !(is_cast_aura(aura.effect) && matches!(aura.anchor, Anchor::Unit(each) if each == unit)));
        self.auras.push(Banded { effect, anchor, start, ticks: aura_ticks(effect, cast_ms), turns: [0.0; 2], sparkles: Vec::new() });
    }

    /// A cast aura launched again with a skill's damage: it blooms for its own length, beside any cast's aura.
    pub fn bloom_aura(&mut self, gpu: &Gpu, grf: &Grf, unit: Option<u32>, effect: u32, start: Instant) {
        let texture = aura_texture(effect);
        self.textures.entry(texture.clone()).or_insert_with(|| gpu.try_load_texture(grf, &texture, true).ok());
        self.auras.push(Banded { effect, anchor: Anchor::Unit(unit), start, ticks: aura_ticks(effect, 0), turns: [0.0; 2], sparkles: Vec::new() });
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
            let target = if table.units_once.contains(&kind) { Target::Kept(id) } else { Target::Lasting(id) };
            self.play(gpu, grf, table, effect, target, at, now);
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

    /// Soul Strike from `from` at `to` with `hits` hits: a ghost a hit, each with its sound as it leaves.
    #[allow(clippy::too_many_arguments)]
    pub fn soul_strike(&mut self, grf: &Grf, from: Vector, to: Vector, hits: u16, now: Instant) {
        self.sprites.entry(crate::prims::GHOST_SPRITE.to_owned()).or_insert_with(|| SpriteSet::at(grf, crate::prims::GHOST_SPRITE));
        for at in crate::prims::ghost_times(hits) {
            self.sounds.push((now + at, "effect\\EF_SoulStrike.wav".to_owned(), from));
        }
        self.prims.soul_strike(from, to, hits, now);
    }

    /// Effects that finished go and those that repeat start again; sprites pick their images for `now`, seen through
    /// a camera turned `yaw` radians.
    pub fn advance(&mut self, gpu: &Gpu, now: Instant, yaw: f32) {
        self.prims.advance(now);
        if let Some(Some(sprite)) = self.sprites.get_mut(crate::prims::GHOST_SPRITE) {
            self.ghost = sprite.frame_at(gpu, 0, 0, Timing::Loop(0.0));
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
            now < playing.start + tick * str_ticks(&playing.loaded.animation, playing.length)
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
            let pass = frames * frame_ms.max(1.0);
            let elapsed = now.saturating_duration_since(animation.start).as_secs_f32() * 1000.0;
            if animation.end.is_some_and(|end| now >= end) || (!animation.repeat && !animation.hold && elapsed >= pass) {
                return false;
            }
            let progress = if animation.repeat { elapsed % pass / pass } else { (elapsed / pass).min(1.0) };
            animation.placed = sprite.frame_at(gpu, index, 0, Timing::Span(progress));
            true
        });
        self.auras.retain(|aura| (now.saturating_duration_since(aura.start).as_secs_f32() * 1000.0 / HIT_TICK_MS) < aura.ticks as f32);
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
        let on = |anchor: Anchor| matches!(anchor, Anchor::Status(each, kind) if each == unit && kind == status);
        self.playing.retain(|playing| !on(playing.anchor));
        self.floating.retain(|floating| !on(floating.anchor));
        self.rings.retain(|ring| !on(ring.anchor));
        self.animations.retain(|animation| !on(animation.anchor));
    }

    pub fn forget(&mut self, unit: u32) {
        self.hits.retain(|hit| !hit.anchor.on(unit));
        self.auras.retain(|aura| !aura.anchor.on(unit));
        self.playing.retain(|playing| !playing.anchor.on(unit));
        self.floating.retain(|floating| !floating.anchor.on(unit));
        self.rings.retain(|ring| !ring.anchor.on(unit));
        self.animations.retain(|animation| !animation.anchor.on(unit));
        self.standing.retain(|(_, id, _)| *id != unit);
    }

    /// The effect shapes showing now: cylinders and models.
    pub fn meshes(&self, now: Instant, position_of: impl Fn(Option<u32>) -> Option<Vector>) -> Vec<EffectMesh<'_>> {
        let rings = self.rings.iter().filter(|ring| ring.start <= now).filter_map(|ring| {
            let at = ring.anchor.place(&position_of)?;
            let Some(Some(texture)) = self.textures.get(&ring.texture) else { return None };
            Some((texture, ring.mesh(now, at), ring.additive))
        });
        let models = self.standing.iter().flat_map(|(model, _, at)| {
            model.0.iter().filter_map(|(path, triangles)| {
                let Some(Some(texture)) = self.textures.get(path) else { return None };
                let place = |position: Vector| std::array::from_fn(|axis| at[axis] + position[axis]);
                Some((texture, triangles.iter().map(|&(position, uv)| MeshVertex { position: place(position), uv, tint: [1.0; 4] }).collect(), false))
            })
        });
        let hits = self.hits.iter().filter(|hit| hit.start <= now && hit.tick(now) < RING_TICKS).filter_map(|hit| {
            let at = hit.anchor.place(&position_of)?;
            let Some(Some(texture)) = self.textures.get(RING_TEXTURE) else { return None };
            Some((texture, hit.ring(hit.tick(now), at), false))
        });
        let auras = self.auras.iter().filter(|aura| aura.start <= now).flat_map(|aura| {
            let age = (now.duration_since(aura.start).as_secs_f32() * 1000.0 / HIT_TICK_MS) as u32;
            let (_, color, additive) = aura_look(aura.effect);
            let color = color.map(|value| f32::from(value) / 255.0);
            let texture = self.textures.get(&aura_texture(aura.effect)).and_then(Option::as_ref);
            let at = aura.anchor.place(&position_of);
            texture.zip(at).map(|(texture, at)| {
                let triangles = aura.bands(age).iter().flat_map(|(band, heights)| band_mesh(at, band, |s| heights[s], color)).collect();
                (texture, triangles, additive)
            })
        });
        rings.chain(models).chain(hits).chain(auras).collect()
    }

    /// What's drawn flat on the screen over units now (COLDHIT's shards and smoke): texture, the unit's feet, and the
    /// triangles' corners in screen pixels from there.
    pub fn screen_shapes(&self, now: Instant, position_of: impl Fn(Option<u32>) -> Option<Vector>) -> Vec<world::ScreenShape<'_>> {
        self.prims
            .screen_shapes(now)
            .into_iter()
            .filter_map(|(path, unit, corners)| Some((self.textures.get(path)?.as_ref()?, position_of(unit)?, corners)))
            .collect()
    }

    /// Every effect quad showing now; `position_of` gives a unit's feet (None = the player).
    pub fn quads(&self, now: Instant, position_of: impl Fn(Option<u32>) -> Option<Vector>) -> Vec<EffectQuad<'_>> {
        let mut quads = Vec::new();
        for floating in self.floating.iter().filter(|floating| floating.start <= now) {
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
            for aura in self.auras.iter().filter(|aura| aura.start <= now && !aura.sparkles.is_empty()) {
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
        // The hits' sparks, each with its trail, sized like a unit's sprite.
        if let Some(Some(sprite)) = self.sprites.get(SPARK_SPRITE) {
            for hit in self.hits.iter().filter(|hit| hit.start <= now) {
                let Some(at) = hit.anchor.place(&position_of) else { continue };
                for spark in &hit.sparks {
                    for (place, alpha) in spark.trail(hit.tick(now)) {
                        let anchor = std::array::from_fn(|axis| at[axis] + place[axis]);
                        for &(key, position, size, layer_angle) in &hit.placed {
                            let corners = QUAD_UV.map(|uv| world::sprite_corner(position, size, layer_angle, uv).0.map(|value| value * spark.scale));
                            quads.push((sprite.texture(key), anchor, corners, [1.0, 1.0, 1.0, alpha], false, false));
                        }
                    }
                }
            }
        }
        for animation in &self.animations {
            let Some(at) = animation.anchor.place(&position_of) else { continue };
            let Some(Some(sprite)) = self.sprites.get(&animation.sprite) else { continue };
            let [x, y] = animation.offset;
            for &(key, position, size, layer_angle) in &animation.placed {
                let corners = QUAD_UV.map(|uv| {
                    let [left, top] = world::sprite_corner(position, size, layer_angle, uv).0;
                    [left + x, top + y]
                });
                quads.push((sprite.texture(key), at, corners, [1.0; 4], false, false));
            }
        }
        for playing in self.playing.iter().filter(|playing| playing.start <= now) {
            let Some(at) = playing.anchor.place(&position_of) else { continue };
            let animation = &playing.loaded.animation;
            // Whole ticks: the state changes once each 16 ms, with nothing in between.
            let frame = str_frame(now.duration_since(playing.start)) as f32;
            for (layer, textures) in animation.layers.iter().zip(&playing.loaded.textures) {
                let Some(state) = layer.state(frame) else { continue };
                let Some(Some(texture)) = textures.get(state.texture) else { continue };
                // In sprite pixels, y down, turned by the key's angle; drawn over everything, `STR_RISE` up.
                let corner = |index: usize| str_corner(state.position, [state.corners[index], state.corners[4 + index]], state.angle);
                quads.push((texture, at, [corner(0), corner(1), corner(2), corner(3)], state.color, state.additive, true));
            }
        }
        // Soul Strike's ghosts, each copy of a trail a little fainter.
        if let Some(Some(sprite)) = self.sprites.get(crate::prims::GHOST_SPRITE) {
            for (at, alpha, scale) in self.prims.ghosts() {
                for &(key, position, size, layer_angle) in &self.ghost {
                    let corners = QUAD_UV.map(|uv| world::sprite_corner(position, size, layer_angle, uv).0.map(|value| value * scale));
                    quads.push((sprite.texture(key), at, corners, [1.0, 1.0, 1.0, alpha], false, false));
                }
            }
        }
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
    fn says_what_skills_show() {
        let table = Table::load();
        // Hit effects: Bash 0 and EF_HIT2 with its first hit, Magnum Break 49 on each target, Pierce none (not in the
        // table, so the skill lines).
        assert_eq!((table.hit(5), table.hit(7)), (Some([Some(0), Some(1)]), Some([Some(49), None])));
        assert_eq!(table.hit(56), None);
        // On the caster: Bash and the default 16, Brandish Spear 144, the bolts none, the newest jobs' none.
        assert_eq!((table.caster(5), table.caster(57), table.caster(19), table.caster(5450)), (Some(16), Some(144), None, None));
        assert_eq!(table.caster(4000), Some(16));
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
        let heal = Banded { effect: 312, anchor: Anchor::Unit(None), start: Instant::now(), ticks: 100, turns: [10.0, 200.0], sparkles };
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
    fn fades_and_slows_the_hit_ring() {
        // 254 until tick 5, about half at 7–8, gone at 10; 0.7 a tick slowing evenly to 0.35.
        assert!((0..=5).all(|tick| ring_alpha(tick) == NEAR_OPAQUE));
        assert!([7, 8].iter().all(|&tick| (100.0..=155.0).contains(&(ring_alpha(tick) * 255.0))));
        assert_eq!(ring_alpha(10), 0.0);
        assert!((ring_speed(0) - 0.7).abs() < 1e-6 && (ring_speed(10) - 0.35).abs() < 1e-6);
        assert!((ring_travel(2) - (0.7 + 0.665)).abs() < 1e-5 && ring_travel(10) == ring_travel(40));
        // A target facing south (0) sends it north; facing west (2), east.
        let [x, z] = drift(0);
        assert!(x.abs() < 1e-6 && (z - 1.0).abs() < 1e-6);
        let [x, z] = drift(2);
        assert!((x - 1.0).abs() < 1e-6 && z.abs() < 1e-6);
        let time = Instant::now();
        let hit = HitSpark { start: time, anchor: Anchor::Unit(None), way: [0.0, 1.0], sparks: sparks([0.0, 1.0], |from, _| from), placed: Vec::new() };
        assert_eq!((hit.tick(time + Duration::from_millis(799)), hit.tick(time + Duration::from_millis(800))), (49, HIT_TICKS));
        // The ring stands upright across its way, 5 to 10 from its middle, 10 above the feet.
        let ring = hit.ring(0, [0.0; 3]);
        assert_eq!(ring.len(), RING_SIDES * 6);
        assert!(ring.iter().all(|vertex| vertex.position[2].abs() < 1e-5));
        let reach = |vertex: &MeshVertex| f32::hypot(vertex.position[0], vertex.position[1] - HIT_HEIGHT);
        assert!(ring.iter().all(|vertex| (reach(vertex) - 5.0).abs() < 1e-4 || (reach(vertex) - 10.0).abs() < 1e-4));
    }

    #[test]
    fn throws_the_hit_sparks_within_their_ranges() {
        let way = drift(0);
        for random in [(|from: f32, _: f32| from) as fn(f32, f32) -> f32, |_, to| to - 1e-4, |from, to| (from + to) / 2.0] {
            for (index, spark) in sparks(way, random).iter().enumerate() {
                let off = spark.heading[0].atan2(spark.heading[2]).to_degrees().rem_euclid(360.0);
                let pitch = spark.heading[1].asin().to_degrees();
                if index < 2 {
                    assert!(!(40.01..319.99).contains(&off), "{off}");
                    assert_eq!(spark.rise, 0.0);
                } else {
                    assert!((139.99..=220.0).contains(&off), "{off}");
                    assert!((0.3..1.2).contains(&spark.rise));
                }
                assert!((-50.01..50.0).contains(&pitch));
                assert!((4.0..7.0).contains(&spark.out) && (0.6..1.5).contains(&spark.speed) && (0.6..1.6).contains(&spark.scale));
                assert!((6..=29).contains(&spark.life));
            }
        }
        // 4 out and 10 up at first; speed falls evenly to half; the back pair comes down to where it started.
        let spark = Spark { heading: [0.0, 0.0, 1.0], out: 4.0, speed: 1.0, rise: 1.0, scale: 1.0, life: 10 };
        assert_eq!(spark.place(0), [0.0, HIT_HEIGHT, 4.0]);
        assert!((spark.speed_at(10) - 0.5).abs() < 1e-6);
        assert!((spark.place(10)[1] - HIT_HEIGHT).abs() < 1e-4 && spark.place(5)[1] > HIT_HEIGHT);
        assert!((spark.alpha(0) - NEAR_OPAQUE).abs() < 1e-6 && spark.alpha(10) == 0.0);
        // Three copies where it was 0, 1 and 2 ticks ago, each fainter; none past its life.
        let trail = spark.trail(4);
        assert_eq!(trail.iter().map(|(place, _)| *place).collect::<Vec<_>>(), [spark.place(4), spark.place(3), spark.place(2)]);
        let alpha = spark.alpha(4);
        assert_eq!(trail.iter().map(|(_, faded)| *faded).collect::<Vec<_>>(), [alpha, alpha * 2.0 / 3.0, alpha / 3.0]);
        assert_eq!(spark.trail(0).len(), 1);
        assert!(spark.trail(10).is_empty());
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
    fn reads_sprites_quakes_and_ground_units() {
        let table = Table::load();
        // Blessing's angel: action 0 at 30 ms a frame for 1.5 s, above the head.
        let blessing = &table.sprites[&42][0];
        assert_eq!((blessing.file.as_str(), blessing.attached), ("축복", true));
        let now = Instant::now();
        let animation = Animation::new(blessing, now, Anchor::Unit(None), false);
        assert_eq!((animation.action, animation.frame_ms, animation.repeat, animation.offset), (Some(0), Some(30.0), true, [0.0, -120.0]));
        assert_eq!(animation.end, Some(now + Duration::from_millis(1500)));
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
}
