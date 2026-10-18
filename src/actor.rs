//! Characters: layered player sprites (body + head with dye palettes) and movement along a path of cells.

use std::collections::HashMap;
use std::time::Instant;

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
    /// Milliseconds into a looping action (idle, walk), at the ACT's own frame delays.
    Loop(f32),
    /// Fraction (0..1) of a one-shot action stretched over a given duration (attack, flinch, death).
    Span(f32),
}
/// ACT delays are in units of 25 ms.
const DELAY_UNIT_MS: f32 = 25.0;

// ponytail: standard jobs only; the client's Lua job tables cover the rest once a Lua VM is in.
/// Body sprite name under data\sprite\인간족\몸통\ for a job ID.
pub fn job_sprite_name(job: u16) -> Option<&'static str> {
    Some(match job {
        0 | 4001 => "초보자",
        1 | 4002 => "검사",
        2 | 4003 => "마법사",
        3 | 4004 => "궁수",
        4 | 4005 => "성직자",
        5 | 4006 => "상인",
        6 | 4007 => "도둑",
        7 => "기사",
        8 => "프리스트",
        9 => "위저드",
        10 => "제철공",
        11 => "헌터",
        12 => "어세신",
        13 => "페코페코_기사",
        14 => "크루세이더",
        15 => "몽크",
        16 => "세이지",
        17 => "로그",
        18 => "연금술사",
        19 => "바드",
        20 => "무희",
        21 => "신페코크루세이더",
        23 => "슈퍼노비스",
        24 => "건너",
        25 => "닌자",
        4008 => "로드나이트",
        4009 => "하이프리",
        4010 => "하이위저드",
        4011 => "화이트스미스",
        4012 => "스나이퍼",
        4013 => "어쌔신크로스",
        4014 => "로드페코",
        4015 => "팔라딘",
        4016 => "챔피온",
        4017 => "프로페서",
        4018 => "스토커",
        4019 => "크리에이터",
        4020 => "클라운",
        4046 => "태권소년",
        4047 => "권성",
        4049 => "소울링커",
        4054 => "룬나이트",
        4055 => "워록",
        4056 => "레인져",
        4057 => "아크비숍",
        4058 => "미케닉",
        4059 => "길로틴크로스",
        4067 => "소서러",
        4068 => "민스트럴",
        4070 => "슈라",
        4071 => "제네릭",
        4072 => "쉐도우체이서",
        4211 => "kagerou",
        4212 => "oboro",
        4218 | 4220 => "summoner",
        4308 => "spirit_handler",
        _ => return None,
    })
}

/// Doram jobs live under 도람족 instead of 인간족.
pub fn is_doram(job: u16) -> bool {
    matches!(job, 4218 | 4220 | 4308)
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

/// Identifies a decoded image: layer, image kind, index and the layer's tint.
type ImageKey = (usize, u32, usize, [u8; 4]);

/// One image of a frame: which image, top-left offset from the feet, size (negative width = mirrored).
pub type Placed = (ImageKey, [f32; 2], [f32; 2]);

/// A character's sprites (body, head, what it wears) or a single NPC/monster sprite, plus the textures decoded so far.
pub struct SpriteSet {
    layers: Vec<Layer>,
    /// Laid out like a player's body: standing and sitting frames go by head direction.
    player: bool,
    /// A player's: where its garment, head and headgear go among its layers.
    dressing: Option<Dressing>,
    textures: HashMap<ImageKey, Texture>,
}

impl SpriteSet {
    pub fn player(grf: &Grf, look: &Look, tables: &LookTables) -> Option<Self> {
        let sex = if look.sex == 0 { "여" } else { "남" };
        let class = job_sprite_name(look.job)?;
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
            dye(format!("{body_palettes}\\{class}_{sex}_{}.pal", look.clothes_color), look.clothes_color),
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
            load(format!("data\\sprite\\{race}\\{class}\\{class}_{sex}{name}"), None, Part::Weapon)
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
                        .map(|frame| rules.and_then(|rules| rules.robe_over_body(look.robe, look.sex, look.job, index, frame)).unwrap_or(fallback))
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
                .map(|(_, part, view, slot, default)| (part, place(view, slot, default)))
                .collect();
        let dressing = Dressing {
            head_first: grf.read(&format!("data\\imf\\{class}_{sex}.imf")).ok().and_then(|bytes| sprite::parse_imf(&bytes).ok()).unwrap_or_default(),
            robe_over_body,
            robe_over_headgear: robe.is_some() && rules.is_some_and(|rules| rules.robe_over_headgear(look.robe)),
            headgear,
            head_hidden: [look.head_top, look.head_mid, look.head_bottom].iter().any(|view| HEAD_HIDING.contains(view)),
        };
        Some(Self {
            layers: [Some(body), head, bottom, mid, top, weapon.flatten(), shield.flatten(), robe].into_iter().flatten().collect(),
            player: true,
            dressing: Some(dressing),
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
        Some(Self { layers: [Some(body), weapon].into_iter().flatten().collect(), player: true, dressing: None, textures: HashMap::new() })
    }

    pub fn single(grf: &Grf, name: &str, monster: bool) -> Option<Self> {
        let folders = if monster { ["몬스터", "npc"] } else { ["npc", "몬스터"] };
        folders.iter().find_map(|folder| Self::at(grf, &format!("data\\sprite\\{folder}\\{name}")))
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
            textures: HashMap::new(),
        })
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
            if !self.textures.contains_key(&key) {
                let layer = &self.layers[key.0];
                if let Some(mut image) = layer.spr.image(key.1, key.2, layer.palette.as_ref()) {
                    if key.3 != [255; 4] {
                        for pixel in image.pixels_mut() {
                            for (channel, tint) in pixel.0.iter_mut().zip(key.3) {
                                *channel = (u16::from(*channel) * u16::from(tint) / 255) as u8;
                            }
                        }
                    }
                    self.textures.insert(key, gpu.texture(&image, false));
                }
            }
        }
        placed.retain(|(key, ..)| self.textures.contains_key(key));
        placed
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
                Part::Head | Part::Weapon | Part::Shield => (frame_index < frames.len()).then_some(frame_index),
            };
            let Some(frame) = frame.and_then(|frame| frames.get(frame)) else { continue };
            let shift = if part.on_head() { sprite::head_shift(body_frame, frame) } else { [0, 0] };
            placed.extend(
                sprite::place(&layer.spr, frame, shift)
                    .into_iter()
                    .map(|placement| ((number, placement.kind, placement.image, placement.color), placement.position, placement.size)),
            );
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
    pub fn index_ms(&self, index: usize) -> Option<f32> {
        let frames = self.layers[0].act.actions.get(index)?.len();
        let delay = self.layers[0].act.delays.get(index).copied().unwrap_or(4.0).max(1.0);
        Some(frames as f32 * delay * DELAY_UNIT_MS)
    }

    pub fn texture(&self, key: ImageKey) -> &Texture {
        &self.textures[&key]
    }
}

/// Which of `count` frames of the layer's action `index` plays at `timing`, at the layer's own frame delay.
fn animated(layer: &Layer, index: usize, count: usize, timing: Timing) -> usize {
    match timing {
        Timing::Loop(elapsed_ms) => {
            let frame_ms = layer.act.delays.get(index).copied().unwrap_or(4.0).max(1.0) * DELAY_UNIT_MS;
            (elapsed_ms / frame_ms) as usize % count
        }
        Timing::Span(progress) => ((progress * count as f32) as usize).min(count - 1),
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
        walk.push(Part::Weapon);
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

/// What places a player's garment, head and headgear among its layers.
struct Dressing {
    /// By action and frame: the head goes behind the body (the job's .imf).
    head_first: Vec<Vec<bool>>,
    /// By action and body frame: the garment goes over the body rather than behind everything.
    robe_over_body: Vec<Vec<bool>>,
    /// Over the body, the garment goes over the headgear too.
    robe_over_headgear: bool,
    /// Each headgear layer's place facing each way; None hides it there.
    headgear: Vec<(Part, [Option<i32>; 8])>,
    /// The headgear hides the head.
    head_hidden: bool,
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
        let headgear: Vec<(Part, i32)> = self.headgear.iter().filter_map(|(part, places)| Some((*part, places[direction]?))).collect();
        // Weapons and shields show in the battle stance and the weapon attacks.
        let armed = matches!(index, 32..=39 | 80..=95);
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
    duration_ms: f32,
    /// Keep showing the last frame afterwards (death).
    hold: bool,
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
    pub direction: u8,
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
    /// The pushcart it pulls, drawn behind it.
    pub cart: Option<SpriteSet>,
    /// The action and frame last seen, so a frame's sound plays once each time it comes up.
    heard: Option<(usize, usize)>,
    /// Locked by Blade Stop or Cursed Circle since then: the animation holds that frame.
    pub held: Option<Instant>,
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
            direction,
            head_direction: 0,
            animation_started: now,
            shot: None,
            ready: false,
            sitting: false,
            remove_at: None,
            cart: None,
            heard: None,
            held: None,
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
    }

    pub fn play(&mut self, action: usize, start: Instant, duration_ms: f32, hold: bool) {
        self.shot = Some(Shot { action, started: start, duration_ms: duration_ms.max(50.0), hold });
    }

    pub fn face(&mut self, toward: [i32; 2]) {
        if toward != self.cell {
            self.direction = world::direction([toward[0] - self.cell[0], toward[1] - self.cell[1]]);
        }
    }

    pub fn walk(&mut self, path: Vec<[i32; 2]>) {
        self.ready = false;
        self.sitting = false;
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
            if progress < 1.0 {
                return [self.cell[0] as f32 + step[0] as f32 * progress, self.cell[1] as f32 + step[1] as f32 * progress];
            }
            self.cell = next;
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
            let progress = now.saturating_duration_since(shot.started).as_secs_f32() * 1000.0 / shot.duration_ms;
            if progress < 1.0 || shot.hold {
                return (shot.action, Timing::Span(progress.min(1.0)));
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
        (action, Timing::Loop(now.saturating_duration_since(self.animation_started).as_secs_f32() * 1000.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(parts, [Part::Body, Part::Head, Part::HeadBottom, Part::HeadMid, Part::HeadTop, Part::Weapon, Part::Shield, Part::Robe]);
        // Facing the camera the garment goes first, behind the body (the dagger and guard hide behind it then).
        let layers = |index: usize| -> Vec<usize> { novice.place(index, 0, Timing::Loop(0.0)).iter().map(|((layer, ..), ..)| *layer).collect() };
        assert_eq!(layers(0), [7, 0, 1, 3, 4, 2]);
        // Facing away it goes over the body, under the head and the hats. The dagger and guard only show in the battle
        // stance (and the dagger in its attack motion), under the hats.
        assert_eq!(layers(4), [0, 7, 1, 3, 4, 2]);
        let ready = layers(READY * 8);
        assert_eq!(ready[ready.len() - 3..], [3, 4, 2], "{ready:?}");
        assert!(ready.contains(&5) && ready.contains(&6), "{ready:?}");
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
    fn stacks_layers_back_to_front() {
        use Part::*;
        let hats = [(HeadMid, 100), (HeadTop, 200), (HeadBottom, 300)];
        // Facing the camera with a cape behind: the cape, body, head, then headgear over all of it.
        assert_eq!(stack(0, false, false, RobeAt::Behind, &hats), [Robe, Body, Head, HeadMid, HeadTop, HeadBottom]);
        // In the battle stance the weapon and a shield turned this way go between the head and the headgear.
        assert_eq!(stack(0, true, false, RobeAt::Behind, &hats), [Robe, Body, Head, Weapon, Shield, HeadMid, HeadTop, HeadBottom]);
        // Turned away: the shield first, the garment over the body under the head.
        assert_eq!(stack(4, true, false, RobeAt::OverBody, &hats), [Shield, Body, Robe, Head, Weapon, HeadMid, HeadTop, HeadBottom]);
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
}
