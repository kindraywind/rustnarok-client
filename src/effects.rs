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

/// Effects not following a unit sit half a cell lower, in sprite pixels.
const GROUND_DROP: f32 = 17.5;

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
    start: Instant,
    anchor: Anchor,
    again: bool,
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
        for part in table.parts.get(&id).into_iter().flatten() {
            let file = match part.random {
                Some((from, to)) => part.file.replace("%d", &pick(from, to).to_string()),
                None => part.file.clone(),
            };
            let key = format!("{}|{file}", part.folder);
            let loaded = self.loaded.entry(key).or_insert_with(|| load(gpu, grf, &file, &part.folder)).clone();
            if let Some(loaded) = loaded {
                self.playing.push(Playing { loaded, start: now, anchor: anchor(part.attached), again: lasting && !part.once });
            }
        }
    }

    /// EF_HIT1 on a unit (None = the player) facing `facing`, from `start`.
    pub fn play_hit(&mut self, gpu: &Gpu, grf: &Grf, unit: Option<u32>, facing: u8, start: Instant) {
        self.textures.entry(RING_TEXTURE.to_owned()).or_insert_with(|| gpu.try_load_texture(grf, RING_TEXTURE, true).ok());
        self.sprites.entry(SPARK_SPRITE.to_owned()).or_insert_with(|| SpriteSet::at(grf, SPARK_SPRITE));
        let way = drift(facing);
        self.hits.push(HitSpark { start, anchor: Anchor::Unit(unit), way, sparks: sparks(way, random), placed: Vec::new() });
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

    /// Effects that finished go and those that repeat start again; sprites pick their images for `now`, seen through
    /// a camera turned `yaw` radians.
    pub fn advance(&mut self, gpu: &Gpu, now: Instant, yaw: f32) {
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
        // A repeating one starts again 8 frames before it ends, so there's no gap: the old one plays out underneath.
        let mut tails = Vec::new();
        self.playing.retain_mut(|playing| {
            let animation = &playing.loaded.animation;
            let fps = animation.fps.max(1) as f32;
            let length = Duration::from_secs_f32(animation.frames as f32 / fps);
            let period = Duration::from_secs_f32(animation.frames.saturating_sub(8).max(1) as f32 / fps);
            while playing.again && !length.is_zero() && now >= playing.start + period {
                let previous = playing.start;
                playing.start += period;
                if now < previous + length {
                    tails.push(Playing { loaded: playing.loaded.clone(), start: previous, anchor: playing.anchor, again: false });
                }
            }
            now < playing.start + length
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
        rings.chain(models).chain(hits).collect()
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
                    quads.push((sprite.texture(key), anchor, corners, color, floating.additive));
                }
                continue;
            }
            let frame = (elapsed * 1000.0 / floating.frame_ms) as usize % floating.textures.len().max(1);
            let Some(Some(texture)) = floating.textures.get(frame).and_then(|path| self.textures.get(path)) else { continue };
            // A ray reaches out from the center; anything else is centered on it.
            let top = if floating.ray { -height } else { -height / 2.0 };
            let corners = [[-width / 2.0, top], [width / 2.0, top], [width / 2.0, top + height], [-width / 2.0, top + height]].map(corner);
            quads.push((texture, anchor, corners, color, floating.additive));
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
                            quads.push((sprite.texture(key), anchor, corners, [1.0, 1.0, 1.0, alpha], false));
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
                quads.push((sprite.texture(key), at, corners, [1.0; 4], false));
            }
        }
        for playing in self.playing.iter().filter(|playing| playing.start <= now) {
            let Some(at) = playing.anchor.place(&position_of) else { continue };
            let drop = if matches!(playing.anchor, Anchor::Unit(_)) { 0.0 } else { GROUND_DROP };
            let animation = &playing.loaded.animation;
            let frame = now.duration_since(playing.start).as_secs_f32() * animation.fps as f32;
            for (layer, textures) in animation.layers.iter().zip(&playing.loaded.textures) {
                let Some(state) = layer.state(frame) else { continue };
                let Some(Some(texture)) = textures.get(state.texture) else { continue };
                // STR space is 640x640 around (320, 320), in sprite pixels, y down; quads turn by their angle.
                let (sin, cos) = state.angle.to_radians().sin_cos();
                let center = [state.position[0] - 320.0, state.position[1] - 320.0 + drop];
                let corner = |index: usize| {
                    let [x, y] = [state.corners[index], state.corners[4 + index]];
                    [x * cos - y * sin + center[0], x * sin + y * cos + center[1]]
                };
                quads.push((texture, at, [corner(0), corner(1), corner(2), corner(3)], state.color, state.additive));
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
        // Hit effects: Bash 0, Magnum Break 49 on each target, Pierce none (not in the table, so the skill lines).
        assert_eq!((table.hit(5), table.hit(7)), (Some([Some(0), None]), Some([Some(49), None])));
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
        // The teleport's violet rings and the logout's four purple ones.
        let textures = |effect: u32| table.rings[&effect].iter().map(|ring| ring["textureName"].as_str()).collect::<Vec<_>>();
        assert_eq!(textures(304), ["Magic_Violet"; 4]);
        assert_eq!(textures(314), ["ring_purple"; 4]);
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
