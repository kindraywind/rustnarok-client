//! Fourth-job status and counter objects: Climax's glow (1800), the A-Machine drone (1839), Eternal Slash's burst (1912)
//! and count (1913), the Abyss Chaser's shadow balls (2109), the frostbite frame (2272), the Soul Ascetic's god marks
//! (2355), the Dragon Knight's swords (2460) and pierce orbs (2461), and Shadow Exceed's double (2467).

use std::any::Any;

use super::{Assets, Class, Event, FLAGS_EFFECT, FLAGS_FLOOR, Face, Fx, Launch, Seen, StrPart, WorldFx, add, scaled};
use crate::world::Vector;
use crate::world_str::{Fired, Scene};

pub const CLIMAX: u32 = 1800;
pub const A_MACHINE: u32 = 1839;
pub const ETERNAL_SLASH: u32 = 1912;
pub const SLASH_COUNT: u32 = 1913;
pub const SHADOW_BALLS: u32 = 2109;
pub const FROST_FRAME: u32 = 2272;
pub const GOD_MARKS: u32 = 2355;
pub const SERVANT_WEAPON: u32 = 2460;
pub const CHARGING_PIERCE: u32 = 2461;
pub const SHADOW_EXCEED: u32 = 2467;
/// Frostbite's thaw, an STR world effect: it plays as the status ends.
pub const FROSTBITE_END: u32 = 2271;

/// EFST_CLIMAX, EFST_SERVANTWEAPON, EFST_CHARGINGPIERCE_COUNT, EFST_SHADOW_EXCEED, EFST_E_SLASH_COUNT,
/// EFST_HANDICAPSTATE_FROSTBITE, EFST_A_MACHINE and EFST_T_FIRST_GOD … EFST_T_FOURTH_GOD.
pub const CLIMAX_STATUS: u16 = 1152;
pub const SERVANT_STATUS: u16 = 1172;
pub const PIERCE_STATUS: u16 = 1175;
pub const SHADOW_EXCEED_STATUS: u16 = 1192;
pub const SLASH_COUNT_STATUS: u16 = 1196;
pub const FROSTBITE_STATUS: u16 = 1208;
pub const A_MACHINE_STATUS: u16 = 1236;
pub const GOD_STATUSES: std::ops::RangeInclusive<u16> = 1360..=1363;

/// DK_SERVANTWEAPON_ATK (what a spent sword counts as), DK_SERVANT_W_SIGN, SHC_SHADOW_EXCEED, SHC_ETERNAL_SLASH and
/// MT_A_MACHINE.
pub const SERVANT_SKILL: u16 = 5202;
pub const SIGN_SKILL: u16 = 5203;
pub const SHADOW_EXCEED_SKILL: u16 = 5285;
pub const ETERNAL_SLASH_SKILL: u16 = 5289;
pub const A_MACHINE_SKILL: u16 = 5298;

/// The Abyss Chaser, and the Dragon Knight with its riding forms.
const ABYSS_CHASER: u16 = 4260;
const DRAGON_KNIGHTS: [u16; 3] = [4252, 4265, 4280];

pub const FROST_TEXTURE: &str = "data\\texture\\effect\\state_effect\\frostbite\\frostbite_screen\\frostbite_screen.tga";
const A_MACHINE_WAV: &str = "effect\\MT_A_MACHINE.wav";
const SERVANT_WAV: &str = "effect\\DK_SERVANTWEAPON.wav";

/// Registers the fourth-job status and counter objects.
pub fn register(fx: &mut WorldFx) {
    fx.register(CLIMAX, || Box::new(Climax::default()));
    fx.register(A_MACHINE, || Box::new(Drone::default()));
    fx.register(ETERNAL_SLASH, || Box::new(Slash::default()));
    fx.register(SLASH_COUNT, || Box::new(Slash::default()));
    fx.register(SHADOW_BALLS, || Box::new(Balls::default()));
    fx.register(FROST_FRAME, || Box::new(Frame));
    fx.register(GOD_MARKS, || Box::new(Marks::default()));
    fx.register(SERVANT_WEAPON, || Box::new(Swords::default()));
    fx.register(CHARGING_PIERCE, || Box::new(Orbs::default()));
    fx.register(SHADOW_EXCEED, || Box::new(Exceed::default()));
}

/// The objects only their own launchers start: a status's value, a spirit count or a skill packet sets them up.
pub fn is_class(effect: u32) -> bool {
    matches!(
        effect,
        CLIMAX | A_MACHINE | ETERNAL_SLASH | SLASH_COUNT | SHADOW_BALLS | FROST_FRAME | GOD_MARKS | SERVANT_WEAPON | CHARGING_PIERCE | SHADOW_EXCEED
    )
}

/// Whose spirits these objects show instead of the sphere stand-in: the Abyss Chaser's balls, the Dragon Knight's swords.
pub fn object_spheres(job: u16) -> bool {
    job == ABYSS_CHASER || DRAGON_KNIGHTS.contains(&job)
}

/// What a status plays on its unit as it ends: frostbite's thaw.
pub fn end_effects(status: u16) -> &'static [u32] {
    if status == FROSTBITE_STATUS { &[FROSTBITE_END] } else { &[] }
}

fn sound(file: &str) -> Event {
    Event::Sound { file: file.to_owned(), listener: false }
}

/// A point moved `right` along the camera's right and `up` along its up: the same on the screen at any latitude.
fn on_screen(point: Vector, [right, up]: [f32; 2], scene: &Scene) -> Vector {
    add(add(point, scaled(scene.right, right)), scaled(scene.up, up))
}

/// Where the older data keeps a file that newer data keeps under its job's and skill's folders.
// ponytail: the older data has no Servant Weapon Demolition shot and only the newer pierce orb, taken here for
// chargingpierce_cast2; check both against data that has the files.
fn older_file(file: &str) -> Option<String> {
    if file == ORB_FILE {
        return Some("new_chargingpierce\\new_chargingpierce_orb\\new_chargingpierce_orb".to_owned());
    }
    if let Some(rest) = file.strip_prefix("SOUL_ASCETIC\\") {
        let (_, name) = rest.split_once('\\')?;
        return Some(format!("soul_ascetic\\efst_t_xxxx_god\\{name}"));
    }
    let folders = [
        ("DRAGON_KNIGHT\\DK_SERVANTWEAPON\\", "servantweapon\\"),
        ("DRAGON_KNIGHT\\DK_SERVANT_W_SIGN\\", "servant_sign\\"),
        ("DRAGON_KNIGHT\\DK_SERVANT_W_PHANTOM\\", "servant_phantom\\"),
        ("DRAGON_KNIGHT\\DK_SERVANT_W_DEMOL\\", "servant_demolition\\"),
        ("SHADOW_CROSS\\SHC_SHADOW_EXCEED\\shadow_exceed\\", "shadow_exceed\\shadow_exceed\\"),
    ];
    folders.iter().find_map(|(newer, older)| file.strip_prefix(newer).map(|rest| format!("{older}{rest}")))
}

/// An STR part of `file` with no step offset, looping or not, on the floor list or the effect list; the older data's
/// file when the data has no `file`.
fn part(file: &str, min: bool, looped: bool, floor: bool, assets: &mut dyn Assets) -> StrPart {
    let mut part = StrPart::new(file, min, assets);
    if part.loaded.is_none()
        && let Some(older) = older_file(file)
    {
        part = StrPart::new(&older, min, assets);
    }
    part.offset = [0.0; 2];
    part.timeline.looped = looped;
    part.flags = if floor { FLAGS_FLOOR } else { FLAGS_EFFECT };
    part
}

/// An STR part drawn in the world (`StrPart::faces` 3-D), its STR px `scale` world units.
fn world_part(file: &str, min: bool, looped: bool, scale: f32, assets: &mut dyn Assets) -> StrPart {
    let mut part = part(file, min, looped, false, assets);
    part.world = true;
    part.controls.scale = scale;
    part
}

fn draw_all<'a>(parts: impl IntoIterator<Item = &'a StrPart>, seen: &Seen, out: &mut Vec<Face<'a>>) {
    for part in parts {
        part.faces(&seen.eye, out);
    }
}

/// A status of a unit (owner `Some(None)` = the own player) standing at `at` turning on with its value, or off.
pub fn status(fx: &mut WorldFx, owner: Option<Option<u32>>, at: Vector, (status, on, value): (u16, bool, i32), min: bool, assets: &mut dyn Assets) {
    let launch = |id: u32, a7: Option<i32>| Launch { a7, min, ..Launch::at(id, owner, at) };
    match status {
        CLIMAX_STATUS => {
            fx.remove(owner, CLIMAX);
            if on {
                fx.launch(&launch(CLIMAX, Some(value)), assets);
            }
        }
        A_MACHINE_STATUS if on => {
            fx.launch(&launch(A_MACHINE, None), assets);
        }
        A_MACHINE_STATUS => fx.remove(owner, A_MACHINE),
        // Every count makes another copy with its own number; the old ones end by themselves.
        SLASH_COUNT_STATUS if on => {
            fx.launch(&launch(SLASH_COUNT, Some(value)), assets);
        }
        SLASH_COUNT_STATUS => fx.remove(owner, SLASH_COUNT),
        // The frame is the own player's only.
        FROSTBITE_STATUS if owner == Some(None) => {
            if on {
                fx.launch(&launch(FROST_FRAME, None), assets);
            } else {
                fx.remove(owner, FROST_FRAME);
            }
        }
        _ if GOD_STATUSES.contains(&status) => {
            let k = i32::from(status - GOD_STATUSES.start() + 1);
            if on {
                fx.remove(owner, GOD_MARKS);
                fx.launch(&launch(GOD_MARKS, Some(k)), assets);
            } else if let Some((fx, marks)) = fx.class_mut::<Marks>(owner, &[GOD_MARKS])
                && fx.a7 == k
            {
                marks.end(fx.min, assets);
            }
        }
        SERVANT_STATUS if !on => fx.remove(owner, SERVANT_WEAPON),
        PIERCE_STATUS if on => {
            fx.launch(&launch(CHARGING_PIERCE, None), assets);
            if let Some((fx, orbs)) = fx.class_mut::<Orbs>(owner, &[CHARGING_PIERCE]) {
                orbs.set_count(value, fx.min, assets);
            }
        }
        PIERCE_STATUS => fx.remove(owner, CHARGING_PIERCE),
        _ => {}
    }
}

/// The spirit count of a unit of `job` standing at `at`, `seen` unless hidden or cloaked: the Abyss Chaser's balls or the
/// Dragon Knight's swords.
#[allow(clippy::too_many_arguments)]
pub fn spirits(fx: &mut WorldFx, owner: Option<Option<u32>>, job: u16, count: u16, seen: bool, at: Vector, min: bool, assets: &mut dyn Assets) {
    let launch = |id: u32| Launch { min, ..Launch::at(id, owner, at) };
    if job == ABYSS_CHASER {
        if count == 0 || !seen {
            fx.remove(owner, SHADOW_BALLS);
            return;
        }
        fx.launch(&launch(SHADOW_BALLS), assets);
        if let Some((fx, balls)) = fx.class_mut::<Balls>(owner, &[SHADOW_BALLS]) {
            balls.set_count(usize::from(count), fx.min, assets);
        }
        return;
    }
    if !DRAGON_KNIGHTS.contains(&job) {
        return;
    }
    if !seen {
        fx.remove(owner, SERVANT_WEAPON);
        return;
    }
    fx.launch(&launch(SERVANT_WEAPON), assets);
    if let Some((fx, swords)) = fx.class_mut::<Swords>(owner, &[SERVANT_WEAPON]) {
        while swords.swords.len() < usize::from(count) && swords.add(fx, assets) {}
        while swords.swords.len() > usize::from(count) && swords.shoot(fx, SERVANT_SKILL, assets) {}
    }
}

/// The cast start of Servant Weapon Sign by `owner`: a sword flies off glowing red.
pub fn sign_cast(fx: &mut WorldFx, owner: Option<Option<u32>>, assets: &mut dyn Assets) {
    if let Some((fx, swords)) = fx.class_mut::<Swords>(owner, &[SERVANT_WEAPON]) {
        swords.shoot(fx, SIGN_SKILL, assets);
    }
}

/// A damage packet of Attack Machine from `caster` on a target standing at `at`: the caster's drone fires, and the hit
/// sounds at the target.
pub fn a_machine_hit(fx: &mut WorldFx, caster: Option<Option<u32>>, at: Vector, assets: &mut dyn Assets) -> Fired {
    if let Some((fx, drone)) = fx.class_mut::<Drone>(caster, &[A_MACHINE]) {
        drone.set(1, fx.min, assets);
    }
    // ponytail: a `wav` row plays at the effect's place; this one plays at the target.
    Fired::Sound(A_MACHINE_WAV.to_owned(), at)
}

/// The use packet of Eternal Slash or Shadow Exceed from `source` standing at `at`: its object on the source.
pub fn used(skill: u16, source: Option<Option<u32>>, at: Vector, min: bool) -> Option<Launch> {
    let id = match skill {
        ETERNAL_SLASH_SKILL => ETERNAL_SLASH,
        SHADOW_EXCEED_SKILL => SHADOW_EXCEED,
        _ => return None,
    };
    Some(Launch { min, ..Launch::at(id, source, at) })
}

// Climax (1800)

/// A spinning light over the caster and a ground glow in its level's colour under it.
#[derive(Default)]
struct Climax {
    parts: Vec<StrPart>,
}

/// The ground glow's file for Climax's level, none outside 1 … 5.
fn climax_floor(level: i32) -> Option<String> {
    (1..=5).contains(&level).then(|| format!("climax\\climax_bottom\\climax_bottom_{level}"))
}

impl Class for Climax {
    fn begin(&mut self, fx: &mut Fx, assets: &mut dyn Assets) {
        fx.single = true;
        self.parts.push(part("climax\\climax\\climax", fx.min, true, false, assets));
        if let Some(file) = climax_floor(fx.a7) {
            self.parts.push(part(&file, fx.min, true, true, assets));
        }
        fx.events.push((0, sound("effect\\AG_CLIMAX.wav")));
    }

    fn update(&mut self, fx: &mut Fx, scene: &Scene, _: &mut dyn Assets) {
        for part in &mut self.parts {
            part.point = on_screen(fx.position, [0.0, 10.0], scene);
        }
        self.parts.retain_mut(StrPart::step);
    }

    fn draw<'a>(&'a self, _: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        draw_all(&self.parts, seen, out);
    }

    fn alive(&self, fx: &Fx, scene: &Scene) -> bool {
        fx.owned(scene)
    }
}

// The A-Machine (1839)

/// The drone over the Meister's head: unfolding (state 0), firing (1) or hovering (2); only the current state's STR.
#[derive(Default)]
struct Drone {
    state: u8,
    part: Option<StrPart>,
}

impl Drone {
    const FILES: [&str; 3] =
        ["a_machine\\a_machine\\a_machine_attack_waiting", "a_machine\\a_machine\\a_machine_attack", "a_machine\\a_machine\\a_machine_waiting"];

    /// Goes to `state`, its STR from frame 0.
    fn set(&mut self, state: u8, min: bool, assets: &mut dyn Assets) {
        self.state = state;
        self.part = Some(part(Self::FILES[usize::from(state)], min, state == 2, false, assets));
    }
}

impl Class for Drone {
    fn begin(&mut self, fx: &mut Fx, assets: &mut dyn Assets) {
        fx.single = true;
        self.set(0, fx.min, assets);
    }

    fn update(&mut self, fx: &mut Fx, scene: &Scene, assets: &mut dyn Assets) {
        let Some(part) = &mut self.part else { return };
        part.point = on_screen(fx.position, [0.0, 8.0], scene);
        if !part.step() {
            self.set(2, fx.min, assets);
        }
    }

    fn draw<'a>(&'a self, _: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        draw_all(&self.part, seen, out);
    }

    fn alive(&self, fx: &Fx, scene: &Scene) -> bool {
        fx.owned(scene)
    }

    fn any(&mut self) -> Option<&mut dyn Any> {
        Some(self)
    }
}

// Eternal Slash (1912) and its count (1913)

/// Eternal Slash's star of blade streaks over a ground ring round the chest (1912), or the count's digit in a smoky
/// burst over the unit (1913).
#[derive(Default)]
struct Slash {
    parts: Vec<StrPart>,
    up: f32,
}

/// The count's file for `n`, none outside 1 … 5.
fn count_file(n: i32) -> Option<String> {
    (1..=5).contains(&n).then(|| format!("eternal_slash\\eternal_slash_count\\eternal_slash_count_{n:02}"))
}

impl Class for Slash {
    fn begin(&mut self, fx: &mut Fx, assets: &mut dyn Assets) {
        if fx.id == ETERNAL_SLASH {
            self.up = 13.0;
            self.parts.push(part("eternal_slash\\eternal_slash_bottom\\eternal_slash_bottom", fx.min, false, true, assets));
            self.parts.push(part("eternal_slash\\eternal_slash\\eternal_slash", fx.min, false, false, assets));
        } else {
            self.up = 25.0;
            self.parts.extend(count_file(fx.a7).map(|file| part(&file, fx.min, false, false, assets)));
        }
    }

    fn update(&mut self, fx: &mut Fx, scene: &Scene, _: &mut dyn Assets) {
        for part in &mut self.parts {
            part.point = on_screen(fx.position, [0.0, self.up], scene);
        }
        self.parts.retain_mut(StrPart::step);
    }

    fn draw<'a>(&'a self, _: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        draw_all(&self.parts, seen, out);
    }

    fn alive(&self, _: &Fx, _: &Scene) -> bool {
        !self.parts.is_empty()
    }
}

// The Abyss Chaser's shadow balls (2109)

/// Each ball's camera-space offset (right, up) and part scale.
const BALL_OFFSETS: [[f32; 2]; 5] = [[-4.8, 9.1], [-3.4, 12.8], [-0.7, 14.0], [2.7, 14.0], [4.3, 10.0]];
const BALL_SCALES: [f32; 5] = [0.8, 0.6, 0.5, 0.7, 0.4];

const BALL_CREATE: &str = "from_the_abyss\\from_the_abyss_shadowball_create\\from_the_abyss_shadowball_create";
const BALL: &str = "from_the_abyss\\from_the_abyss_shadowball\\from_the_abyss_shadowball";
const BALL_DIE: &str = "from_the_abyss\\from_the_abyss_shadowball_die\\from_the_abyss_shadowball_die";

/// A ball being made (1), floating (2) or collapsing (3).
struct Ball {
    part: StrPart,
    state: u8,
    scale: f32,
    offset: [f32; 2],
}

impl Ball {
    fn new(file: &str, state: u8, scale: f32, offset: [f32; 2], min: bool, assets: &mut dyn Assets) -> Self {
        let mut part = part(file, min, state == 2, false, assets);
        part.controls.scale = scale;
        Self { part, state, scale, offset }
    }
}

/// Up to five black-violet orbs in an arc over the Abyss Chaser's head, one a sphere.
#[derive(Default)]
struct Balls {
    balls: [Option<Ball>; 5],
    count: usize,
}

impl Balls {
    /// New balls crackle into being, spent ones collapse; a count over 5 does nothing.
    fn set_count(&mut self, n: usize, min: bool, assets: &mut dyn Assets) {
        if n > 5 {
            return;
        }
        for k in self.count..n {
            self.balls[k] = Some(Ball::new(BALL_CREATE, 1, BALL_SCALES[k], BALL_OFFSETS[k], min, assets));
        }
        for k in (n..self.count).rev() {
            if let Some((scale, offset)) = self.balls[k].as_ref().map(|ball| (ball.scale, ball.offset)) {
                self.balls[k] = Some(Ball::new(BALL_DIE, 3, scale, offset, min, assets));
            }
        }
        self.count = n;
    }
}

impl Class for Balls {
    fn begin(&mut self, fx: &mut Fx, _: &mut dyn Assets) {
        fx.single = true;
    }

    fn update(&mut self, fx: &mut Fx, scene: &Scene, assets: &mut dyn Assets) {
        for slot in &mut self.balls {
            let Some(ball) = slot else { continue };
            ball.part.point = on_screen(fx.position, ball.offset, scene);
            if ball.part.step() {
                continue;
            }
            let (state, scale, offset) = (ball.state, ball.scale, ball.offset);
            *slot = (state == 1).then(|| Ball::new(BALL, 2, scale, offset, fx.min, assets));
        }
    }

    fn draw<'a>(&'a self, _: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        draw_all(self.balls.iter().flatten().map(|ball| &ball.part), seen, out);
    }

    fn alive(&self, fx: &Fx, scene: &Scene) -> bool {
        fx.owned(scene)
    }

    fn any(&mut self) -> Option<&mut dyn Any> {
        Some(self)
    }
}

// The frostbite frame (2272)

/// Frost on every edge of the screen while the own player is frost-bitten: drawn by `frost_quads`, nothing in the world.
struct Frame;

impl Class for Frame {
    fn begin(&mut self, fx: &mut Fx, assets: &mut dyn Assets) {
        fx.single = true;
        assets.texture(FROST_TEXTURE);
    }

    fn update(&mut self, _: &mut Fx, _: &Scene, _: &mut dyn Assets) {}

    fn draw<'a>(&'a self, _: &Fx, _: &Seen, _: &mut Vec<Face<'a>>) {}

    fn alive(&self, fx: &Fx, scene: &Scene) -> bool {
        fx.owned(scene)
    }
}

/// A quad's corners in screen px and their texture coordinates.
pub type ScreenQuad = ([[f32; 2]; 4], [[f32; 2]; 4]);

/// The frame's four quads on a `width` × `height` screen: each a quarter (top left, top right, bottom left, bottom
/// right), its corners in screen px (top left, top right, bottom left, bottom right; y down) and their texture
/// coordinates. The texture's frost lies along its top and right, so mirroring the left and bottom quarters puts it on
/// every outer edge.
pub fn frost_quads(width: f32, height: f32) -> [ScreenQuad; 4] {
    let (half_w, half_h) = (width / 2.0, height / 2.0);
    let (low, high) = (0.5 / 512.0, 512.5 / 512.0);
    std::array::from_fn(|j| {
        let [x, y] = [(j % 2) as f32 * half_w, (j / 2) as f32 * half_h];
        let corners = [[x, y], [x + half_w, y], [x, y + half_h], [x + half_w, y + half_h]];
        let [u0, u1] = if j % 2 == 0 { [high, low] } else { [low, high] };
        let [v0, v1] = if j >= 2 { [high, low] } else { [low, high] };
        (corners, [[u0, v0], [u1, v0], [u0, v1], [u1, v1]])
    })
}

// The god marks (2355)

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Start,
    Loop,
    End,
}

/// The gods by mark: the status name's ordinal and the file's name.
const GODS: [(&str, &str); 4] = [("FIRST", "blue_dragon"), ("SECOND", "white_tiger"), ("THIRD", "red_phoenix"), ("FOURTH", "black_tortoise")];

/// One mark: the god `i`, its phase's STR and its angle on the ring (degrees).
struct Mark {
    i: usize,
    phase: Phase,
    angle: u32,
    part: StrPart,
}

impl Mark {
    fn new(i: usize, phase: Phase, angle: u32, min: bool, assets: &mut dyn Assets) -> Self {
        let (ordinal, god) = GODS[i];
        let phase_name = match phase {
            Phase::Start => "start",
            Phase::Loop => "loop",
            Phase::End => "end",
        };
        let file = format!("SOUL_ASCETIC\\EFST_T_{ordinal}_GOD\\{god}_{phase_name}");
        Self { i, phase, angle, part: part(&file, min, phase == Phase::Loop, true, assets) }
    }

    /// Mark i's starting angle: 0, 180, 90, 270.
    fn first_angle(i: usize) -> u32 {
        (i as u32 / 2 + 2 * (i as u32 % 2)) * 90
    }
}

/// The ring point for `angle`: 8.5 round a centre 20 up, angle 0 right, 90 below (clockwise on the screen).
fn ring_offset(angle: u32) -> [f32; 2] {
    let (sin, cos) = (angle as f32).to_radians().sin_cos();
    [8.5 * cos, 20.0 - 8.5 * sin]
}

/// One to four god emblems circling the Soul Ascetic's upper body.
#[derive(Default)]
struct Marks {
    marks: Vec<Mark>,
    ending: bool,
}

impl Marks {
    /// The status went off: each mark plays its end where it stands.
    fn end(&mut self, min: bool, assets: &mut dyn Assets) {
        self.ending = true;
        for mark in &mut self.marks {
            *mark = Mark::new(mark.i, Phase::End, mark.angle, min, assets);
        }
    }
}

impl Class for Marks {
    fn begin(&mut self, fx: &mut Fx, assets: &mut dyn Assets) {
        fx.single = true;
        let n = fx.a7.clamp(0, 4) as usize;
        self.marks = (0..n).map(|i| Mark::new(i, Phase::Start, Mark::first_angle(i), fx.min, assets)).collect();
    }

    fn update(&mut self, fx: &mut Fx, scene: &Scene, assets: &mut dyn Assets) {
        let mut index = 0;
        while index < self.marks.len() {
            let mark = &mut self.marks[index];
            if mark.phase == Phase::Loop {
                mark.angle = (mark.angle + 1) % 360;
            }
            mark.part.point = on_screen(fx.position, ring_offset(mark.angle), scene);
            if mark.part.step() {
                index += 1;
                continue;
            }
            if mark.phase == Phase::Start {
                // Its loop takes over in the same update.
                *mark = Mark::new(mark.i, Phase::Loop, mark.angle, fx.min, assets);
                mark.part.point = on_screen(fx.position, ring_offset(mark.angle), scene);
                mark.part.step();
                index += 1;
            } else {
                self.marks.remove(index);
            }
        }
    }

    fn draw<'a>(&'a self, _: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        draw_all(self.marks.iter().map(|mark| &mark.part), seen, out);
    }

    fn alive(&self, fx: &Fx, scene: &Scene) -> bool {
        fx.owned(scene) && !(self.ending && self.marks.is_empty())
    }

    fn any(&mut self) -> Option<&mut dyn Any> {
        Some(self)
    }
}

// The Dragon Knight's swords (2460)

const SWORD_SCALE: f32 = 0.12;
const SWORD_CAST: &str = "DRAGON_KNIGHT\\DK_SERVANTWEAPON\\servantweapon_cast\\servantweapon_cast";
const SWORD: &str = "DRAGON_KNIGHT\\DK_SERVANTWEAPON\\servantweapon\\servantweapon";

/// A sword's STR (or its shot's) at its angle on the ring.
struct Sword {
    part: StrPart,
    angle: f32,
}

/// The shot a sword flies off as, by the skill that used it.
fn shot_file(skill: u16) -> Option<&'static str> {
    Some(match skill.checked_sub(SERVANT_SKILL)? {
        0 => "DRAGON_KNIGHT\\DK_SERVANTWEAPON\\servantweapon_shoot\\servantweapon_shoot",
        1 => "DRAGON_KNIGHT\\DK_SERVANT_W_SIGN\\servant_sign_shot\\servant_sign_shot",
        2 => "DRAGON_KNIGHT\\DK_SERVANT_W_PHANTOM\\servant_phantom_shot\\servant_phantom_shot",
        3 => "DRAGON_KNIGHT\\DK_SERVANT_W_DEMOL\\servant_demolition_shot\\servant_demolition_shot",
        _ => return None,
    })
}

/// A point on the ring of radius 13 round `feet`, 10 up, at `angle` degrees in world axes.
fn sword_point(feet: Vector, angle: f32) -> Vector {
    let (sin, cos) = angle.to_radians().sin_cos();
    [feet[0] + 13.0 * cos, feet[1] + 10.0, feet[2] + 13.0 * sin]
}

/// Up to five spectral swords hovering in a ring round the Dragon Knight; a used one flies straight up.
#[derive(Default)]
struct Swords {
    swords: Vec<Sword>,
    shots: Vec<Sword>,
}

impl Swords {
    /// A new sword with its sound, at most five: false when there are five.
    fn add(&mut self, fx: &mut Fx, assets: &mut dyn Assets) -> bool {
        if self.swords.len() >= 5 {
            return false;
        }
        let angle = (72.0 * self.swords.len() as f32 + 90.0) % 360.0;
        self.swords.push(Sword { part: world_part(SWORD_CAST, fx.min, false, SWORD_SCALE, assets), angle });
        fx.events.push((fx.age, sound(SERVANT_WAV)));
        true
    }

    /// The last sword flies off as `skill`'s shot: false when there is none or the skill has no shot.
    fn shoot(&mut self, fx: &mut Fx, skill: u16, assets: &mut dyn Assets) -> bool {
        let (Some(last), Some(file)) = (self.swords.last(), shot_file(skill)) else { return false };
        let angle = last.angle;
        self.swords.pop();
        self.shots.push(Sword { part: world_part(file, fx.min, false, SWORD_SCALE, assets), angle });
        fx.events.push((fx.age, sound(SERVANT_WAV)));
        true
    }
}

impl Class for Swords {
    fn begin(&mut self, fx: &mut Fx, _: &mut dyn Assets) {
        fx.single = true;
    }

    fn update(&mut self, fx: &mut Fx, _: &Scene, assets: &mut dyn Assets) {
        for sword in &mut self.swords {
            sword.part.point = sword_point(fx.position, sword.angle);
            // The loop takes over in the same update.
            if !sword.part.step() {
                sword.part = world_part(SWORD, fx.min, true, SWORD_SCALE, assets);
                sword.part.point = sword_point(fx.position, sword.angle);
                sword.part.step();
            }
        }
        for shot in &mut self.shots {
            shot.part.point = sword_point(fx.position, shot.angle);
        }
        self.shots.retain_mut(|shot| shot.part.step());
        // The queue holds only the sounds still to come.
        let age = fx.age;
        fx.events.retain(|(at, _)| *at >= age);
    }

    fn draw<'a>(&'a self, _: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        draw_all(self.swords.iter().chain(&self.shots).map(|sword| &sword.part), seen, out);
    }

    fn alive(&self, fx: &Fx, scene: &Scene) -> bool {
        fx.owned(scene)
    }

    fn any(&mut self) -> Option<&mut dyn Any> {
        Some(self)
    }
}

// The Dragon Knight's pierce orbs (2461)

const ORB_FILE: &str = "DRAGON_KNIGHT\\DK_CHARGINGPIERCE\\chargingpierce_cast2\\chargingpierce_cast2";

/// Orb k's camera-space offset (right, up): a 5 × 2 grid over the head.
fn orb_offset(k: usize) -> [f32; 2] {
    [-7.5 + 3.75 * (k % 5) as f32, if k < 5 { 23.45 } else { 20.0 }]
}

/// A dark-red orb gathering above the Dragon Knight's head per stack.
#[derive(Default)]
struct Orbs {
    orbs: Vec<StrPart>,
}

impl Orbs {
    /// Lowering the count gathers every orb again; raising it gathers the new ones; over 10 does nothing.
    fn set_count(&mut self, n: i32, min: bool, assets: &mut dyn Assets) {
        let Ok(n) = usize::try_from(n) else { return };
        if n > 10 {
            return;
        }
        if self.orbs.len() > n {
            self.orbs.clear();
        }
        for _ in self.orbs.len()..n {
            let mut orb = part(ORB_FILE, min, false, false, assets);
            orb.timeline.hold = true;
            orb.controls.scale = 0.75;
            self.orbs.push(orb);
        }
    }
}

impl Class for Orbs {
    fn begin(&mut self, fx: &mut Fx, _: &mut dyn Assets) {
        fx.single = true;
    }

    fn update(&mut self, fx: &mut Fx, scene: &Scene, _: &mut dyn Assets) {
        for (k, orb) in self.orbs.iter_mut().enumerate() {
            orb.point = on_screen(fx.position, orb_offset(k), scene);
            orb.step();
        }
    }

    fn draw<'a>(&'a self, _: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        draw_all(&self.orbs, seen, out);
    }

    fn alive(&self, fx: &Fx, scene: &Scene) -> bool {
        fx.owned(scene)
    }

    fn any(&mut self) -> Option<&mut dyn Any> {
        Some(self)
    }
}

// Shadow Exceed (2467)

const EXCEED_FOLDER: &str = "SHADOW_CROSS\\SHC_SHADOW_EXCEED\\shadow_exceed\\";

/// A part of the double: its STR, how far up it sits, and whether it is the back image.
struct Shadow {
    part: StrPart,
    up: f32,
    back: bool,
}

/// The point one cell (5 units) straight behind a unit at `feet` facing `rot` degrees (0 south, 90 east).
pub fn behind(feet: Vector, rot: f32) -> Vector {
    let (sin, cos) = rot.to_radians().sin_cos();
    [feet[0] - 5.0 * sin, feet[1], feet[2] + 5.0 * cos]
}

/// Whether the back image shows for a unit seen in sprite direction `d` (0 … 7).
fn shows_back(d: u8) -> bool {
    (2..=5).contains(&d)
}

/// A black double of the Shadow Cross rising one cell behind it, front or back by how the unit is seen; it leaves
/// Kagemusya's afterimages when it is gone.
#[derive(Default)]
struct Exceed {
    parts: Vec<Shadow>,
    /// The sprite direction the owner showed at the last update.
    seen: u8,
    /// The age the afterimages start at, once the parts are gone.
    ended: Option<u32>,
}

impl Class for Exceed {
    fn begin(&mut self, fx: &mut Fx, assets: &mut dyn Assets) {
        fx.single = true;
        for (name, up, floor, back) in [
            ("shadow_exceed_f_shadow_bottom", 12.0, true, false),
            ("shadow_exceed_f_shadow", 10.0, false, false),
            ("shadow_exceed_b_shadow_bottom", 12.0, true, true),
            ("shadow_exceed_b_shadow", 10.0, false, true),
        ] {
            let mut part = world_part(&format!("{EXCEED_FOLDER}{name}"), fx.min, false, 0.14, assets);
            part.flags = if floor { FLAGS_FLOOR } else { FLAGS_EFFECT };
            part.bias = if back { 3e-4 } else { 0.0 };
            self.parts.push(Shadow { part, up, back });
        }
        fx.events.push((0, sound("effect\\SHC_SHADOW_EXCEED.wav")));
    }

    fn update(&mut self, fx: &mut Fx, scene: &Scene, _: &mut dyn Assets) {
        if self.ended.is_some() {
            return;
        }
        let rot = fx.owner.and_then(|owner| (scene.body)(owner)).map_or(0.0, |body| body.rot);
        let at = behind(fx.position, rot);
        self.seen = crate::actor::view_direction(rot, scene.longitude, true);
        for shadow in &mut self.parts {
            shadow.part.point = [at[0], at[1] + shadow.up, at[2]];
            shadow.part.controls.mirror_x = if shadow.back { matches!(self.seen, 4 | 5) } else { matches!(self.seen, 0 | 1) };
        }
        self.parts.retain_mut(|shadow| shadow.part.step());
        // The afterimages start once the double is gone.
        if self.parts.is_empty() {
            self.ended = Some(fx.age + 1);
            fx.events.push((fx.age + 1, Event::Afterimage));
        }
    }

    fn draw<'a>(&'a self, _: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        let back = shows_back(self.seen);
        draw_all(self.parts.iter().filter(|shadow| shadow.back == back).map(|shadow| &shadow.part), seen, out);
    }

    fn alive(&self, fx: &Fx, scene: &Scene) -> bool {
        fx.owned(scene) && self.ended.is_none_or(|end| fx.age <= end)
    }
}

#[cfg(test)]
mod tests {
    use super::super::Eye;
    use super::*;
    use crate::actor::Placed;
    use crate::effects::Loaded;
    use crate::world_str::Body;
    use rustnarok_client::effect::{Key, Layer, Str};
    use std::rc::Rc;

    const UNIT: u32 = 0x700;
    const OWNER: Option<Option<u32>> = Some(Some(UNIT));
    const ME: Option<Option<u32>> = Some(None);
    const FEET: Vector = [30.0, 4.0, 50.0];

    /// The frame count of each fake STR, by its file.
    fn frames_of(file: &str) -> i32 {
        let name = file.rsplit('\\').next().unwrap_or(file).trim_start_matches("min_");
        match name {
            "a_machine_attack_waiting" => 71,
            "a_machine_waiting" => 60,
            "eternal_slash" | "eternal_slash_bottom" => 24,
            name if name.starts_with("eternal_slash_count_") => 92,
            "from_the_abyss_shadowball_create" => 75,
            "from_the_abyss_shadowball_die" => 18,
            name if name.ends_with("_start") => 8,
            name if name.ends_with("_end") => 9,
            "servantweapon_cast" => 37,
            name if name.ends_with("_shoot") || name.ends_with("_shot") => 11,
            "chargingpierce_cast2" => 40,
            name if name.ends_with("_shadow_bottom") => 82,
            name if name.ends_with("_shadow") => 96,
            _ => 59,
        }
    }

    /// STRs of one drawn layer centred on (320, 240), with keys at 0 and their last frame; the files asked for kept.
    #[derive(Default)]
    struct Fake {
        strs: Vec<String>,
        textures: Vec<String>,
    }

    impl Assets for Fake {
        fn texture(&mut self, path: &str) -> Option<[f32; 2]> {
            self.textures.push(path.to_owned());
            Some([512.0; 2])
        }
        fn str(&mut self, file: &str) -> Option<Rc<Loaded>> {
            self.strs.push(file.to_owned());
            let frames = frames_of(file);
            let key = |frame| Key {
                frame,
                kind: 0,
                position: [320.0, 240.0],
                corners: [-30.0, 30.0, 30.0, -30.0, -20.0, -20.0, 20.0, 20.0],
                texture: 0.0,
                texture_motion: 0,
                delay: 0.0,
                angle: 0.0,
                color: [1.0; 4],
                source_blend: 5,
                destination_blend: 2,
            };
            let layer = || Layer { textures: vec!["a.bmp".into()], keys: vec![key(0), key(frames - 1)] };
            Some(Rc::new(Loaded { animation: Str { fps: 60, frames: (frames - 1) as u32, layers: vec![layer(), layer()] }, textures: vec![Vec::new(); 2] }))
        }
        fn sprite(&mut self, _: &str, _: usize, _: usize) -> Option<(usize, Vec<Placed>)> {
            None
        }
    }

    fn eye(longitude: f32) -> Eye {
        Eye::at(45.0, longitude)
    }

    /// Runs `f` on a scene where every unit stands at `FEET` facing `rot`, the camera turned `longitude`.
    fn scene<R>(rot: f32, longitude: f32, f: impl FnOnce(&Scene) -> R) -> R {
        let feet = |_: Option<u32>| Some(FEET);
        let body = move |_: Option<u32>| Some(Body { height: 27.2, scale: 1.0, rides: false, doram: false, rot });
        let eye = eye(longitude);
        f(&Scene { player: FEET, feet: &feet, body: &body, longitude, right: eye.right, up: eye.up, ground: &|_, _| 0.0 })
    }

    fn ticks(fx: &mut WorldFx, count: u32, fake: &mut Fake) -> Vec<Fired> {
        scene(0.0, 0.0, |scene| (0..count).flat_map(|_| fx.tick(scene, fake)).collect())
    }

    fn faces(fx: &WorldFx) -> usize {
        fx.faces(&Seen { eye: eye(0.0), shows: &|_| true, hidden: &|_| false, size: &|_| 1.0, ground: &|_, _| 0.0, dim: false }).len()
    }

    /// One object of class `C` run on its own, as the registry runs it.
    struct Solo<C> {
        fx: Fx,
        class: C,
    }

    impl<C: Class> Solo<C> {
        fn new(mut class: C, launch: Launch, assets: &mut Fake) -> Self {
            let mut fx = Fx::of(&launch);
            class.begin(&mut fx, assets);
            Self { fx, class }
        }

        fn tick(&mut self, rot: f32, longitude: f32, assets: &mut Fake) {
            scene(rot, longitude, |scene| {
                self.fx.position = FEET;
                self.class.update(&mut self.fx, scene, assets);
            });
            self.fx.updated = true;
            self.fx.age += 1;
        }

        fn ticks(&mut self, count: u32, assets: &mut Fake) {
            for _ in 0..count {
                self.tick(0.0, 0.0, assets);
            }
        }

        fn alive(&self) -> bool {
            scene(0.0, 0.0, |scene| self.class.alive(&self.fx, scene))
        }
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    fn sub(a: Vector, b: Vector) -> Vector {
        std::array::from_fn(|axis| a[axis] - b[axis])
    }

    fn dot(a: Vector, b: Vector) -> f32 {
        a.iter().zip(b).map(|(a, b)| a * b).sum()
    }

    /// The (right, up) a point sits at from `FEET` on the screen.
    fn screen_offset(point: Vector) -> [f32; 2] {
        let moved = sub(point, FEET);
        [dot(moved, eye(0.0).right), dot(moved, eye(0.0).up)]
    }

    fn running(fx: &WorldFx, id: u32) -> Vec<i32> {
        fx.iter().filter(|object| object.fx.id == id).map(|object| object.fx.a7).collect()
    }

    #[test]
    fn older_data_keeps_the_files_elsewhere() {
        assert_eq!(older_file(SWORD_CAST).as_deref(), Some("servantweapon\\servantweapon_cast\\servantweapon_cast"));
        assert_eq!(older_file(shot_file(SIGN_SKILL).unwrap()).as_deref(), Some("servant_sign\\servant_sign_shot\\servant_sign_shot"));
        assert_eq!(older_file("SOUL_ASCETIC\\EFST_T_THIRD_GOD\\red_phoenix_loop").as_deref(), Some("soul_ascetic\\efst_t_xxxx_god\\red_phoenix_loop"));
        assert_eq!(older_file(&format!("{EXCEED_FOLDER}shadow_exceed_b_shadow")).as_deref(), Some("shadow_exceed\\shadow_exceed\\shadow_exceed_b_shadow"));
        assert_eq!(older_file("climax\\climax\\climax"), None);
        // A missing file falls back to the older one.
        struct Older(Vec<String>);
        impl Assets for Older {
            fn texture(&mut self, _: &str) -> Option<[f32; 2]> {
                None
            }
            fn str(&mut self, file: &str) -> Option<Rc<Loaded>> {
                self.0.push(file.to_owned());
                file.starts_with("servantweapon").then(|| Fake::default().str(file)).flatten()
            }
            fn sprite(&mut self, _: &str, _: usize, _: usize) -> Option<(usize, Vec<Placed>)> {
                None
            }
        }
        let mut older = Older(Vec::new());
        assert!(world_part(SWORD, true, true, SWORD_SCALE, &mut older).loaded.is_some());
        assert_eq!(older.0, ["DRAGON_KNIGHT\\DK_SERVANTWEAPON\\servantweapon\\min_servantweapon", "servantweapon\\servantweapon\\min_servantweapon"]);
    }

    #[test]
    fn climax_glows_in_its_levels_colour() {
        let mut fx = WorldFx::default();
        let mut fake = Fake::default();
        status(&mut fx, OWNER, FEET, (CLIMAX_STATUS, true, 3), false, &mut fake);
        assert_eq!(running(&fx, CLIMAX), [3]);
        assert_eq!(fake.strs, ["climax\\climax\\climax", "climax\\climax_bottom\\climax_bottom_3"]);
        let object = fx.iter().next().unwrap();
        assert!(object.fx.single && object.fx.events == [(0, sound("effect\\AG_CLIMAX.wav"))]);
        for level in 1..=5 {
            assert_eq!(climax_floor(level), Some(format!("climax\\climax_bottom\\climax_bottom_{level}")));
        }
        assert_eq!([climax_floor(0), climax_floor(6)], [None, None]);
        // Sent again it starts afresh with the new level; /mineffect plays the min_ files.
        let first = fx.iter().next().unwrap().serial;
        status(&mut fx, OWNER, FEET, (CLIMAX_STATUS, true, 5), true, &mut fake);
        assert_eq!(running(&fx, CLIMAX), [5]);
        assert_ne!(fx.iter().next().unwrap().serial, first);
        assert!(fake.strs.ends_with(&["climax\\climax\\min_climax".into(), "climax\\climax_bottom\\min_climax_bottom_5".into()]));
        // Both parts loop, 10 up on the screen.
        ticks(&mut fx, 70, &mut fake);
        assert_eq!(faces(&fx), 2);
        status(&mut fx, OWNER, FEET, (CLIMAX_STATUS, false, 0), false, &mut fake);
        assert!(fx.is_empty());
        let mut solo = Solo::new(Climax::default(), Launch { a7: Some(0), ..Launch::at(CLIMAX, OWNER, FEET) }, &mut fake);
        solo.tick(0.0, 0.0, &mut fake);
        assert_eq!(solo.class.parts.len(), 1);
        let [right, up] = screen_offset(solo.class.parts[0].point);
        assert!(close(right, 0.0) && close(up, 10.0));
    }

    #[test]
    fn a_map_load_replays_the_kept_value() {
        let mut fx = WorldFx::default();
        let mut fake = Fake::default();
        status(&mut fx, ME, FEET, (CLIMAX_STATUS, true, 2), false, &mut fake);
        fx.remove_all();
        status(&mut fx, ME, FEET, (CLIMAX_STATUS, true, 2), false, &mut fake);
        assert_eq!(running(&fx, CLIMAX), [2]);
    }

    #[test]
    fn the_slash_count_makes_a_copy_per_count() {
        let mut fx = WorldFx::default();
        let mut fake = Fake::default();
        for n in [1, 2, 0, 6] {
            status(&mut fx, OWNER, FEET, (SLASH_COUNT_STATUS, true, n), false, &mut fake);
        }
        assert_eq!(running(&fx, SLASH_COUNT), [1, 2, 0, 6]);
        assert_eq!(fake.strs, ["eternal_slash\\eternal_slash_count\\eternal_slash_count_01", "eternal_slash\\eternal_slash_count\\eternal_slash_count_02"]);
        // 0 and 6 make no part and end; the others play their 92 frames.
        ticks(&mut fx, 2, &mut fake);
        assert_eq!(running(&fx, SLASH_COUNT), [1, 2]);
        ticks(&mut fx, 91, &mut fake);
        assert_eq!(running(&fx, SLASH_COUNT), [1, 2]);
        ticks(&mut fx, 1, &mut fake);
        assert!(fx.is_empty());
        // 25 up on the screen.
        let mut solo = Solo::new(Slash::default(), Launch { a7: Some(4), ..Launch::at(SLASH_COUNT, OWNER, FEET) }, &mut fake);
        solo.tick(0.0, 0.0, &mut fake);
        let [right, up] = screen_offset(solo.class.parts[0].point);
        assert!(close(right, 0.0) && close(up, 25.0));
    }

    #[test]
    fn eternal_slash_bursts_on_its_source() {
        let mut fake = Fake::default();
        let launch = used(ETERNAL_SLASH_SKILL, OWNER, FEET, false).unwrap();
        assert_eq!((launch.id, launch.owner), (ETERNAL_SLASH, OWNER));
        assert_eq!(used(SHADOW_EXCEED_SKILL, OWNER, FEET, false).map(|launch| launch.id), Some(SHADOW_EXCEED));
        assert!(used(SERVANT_SKILL, OWNER, FEET, false).is_none());
        let mut solo = Solo::new(Slash::default(), launch, &mut fake);
        assert_eq!(fake.strs, ["eternal_slash\\eternal_slash_bottom\\eternal_slash_bottom", "eternal_slash\\eternal_slash\\eternal_slash"]);
        assert_eq!(solo.class.parts[0].flags, FLAGS_FLOOR);
        solo.tick(0.0, 0.0, &mut fake);
        let [_, up] = screen_offset(solo.class.parts[1].point);
        assert!(close(up, 13.0));
        // Both end at tick 24; the object at tick 25.
        solo.ticks(23, &mut fake);
        assert!(solo.alive());
        solo.ticks(1, &mut fake);
        assert!(!solo.alive());
    }

    #[test]
    fn the_a_machine_settles_and_fires() {
        let mut fx = WorldFx::default();
        let mut fake = Fake::default();
        status(&mut fx, OWNER, FEET, (A_MACHINE_STATUS, true, 1), false, &mut fake);
        status(&mut fx, OWNER, FEET, (A_MACHINE_STATUS, true, 1), false, &mut fake);
        assert_eq!(running(&fx, A_MACHINE).len(), 1);
        // State 0 plays its 71 frames, then the hover loops.
        ticks(&mut fx, 71, &mut fake);
        assert_eq!(fx.class_mut::<Drone>(OWNER, &[A_MACHINE]).unwrap().1.state, 0);
        ticks(&mut fx, 1, &mut fake);
        assert_eq!(fx.class_mut::<Drone>(OWNER, &[A_MACHINE]).unwrap().1.state, 2);
        assert_eq!(fake.strs.last().unwrap(), "a_machine\\a_machine\\a_machine_waiting");
        // A hit fires it from frame 0 and sounds at the target; only the current state's part draws.
        let target = [1.0, 2.0, 3.0];
        assert_eq!(a_machine_hit(&mut fx, OWNER, target, &mut fake), Fired::Sound("effect\\MT_A_MACHINE.wav".into(), target));
        assert_eq!(fx.class_mut::<Drone>(OWNER, &[A_MACHINE]).unwrap().1.state, 1);
        ticks(&mut fx, 1, &mut fake);
        assert_eq!(faces(&fx), 1);
        // Another caster's hit only sounds.
        assert!(matches!(a_machine_hit(&mut fx, Some(Some(9)), target, &mut fake), Fired::Sound(..)));
        status(&mut fx, OWNER, FEET, (A_MACHINE_STATUS, false, 0), false, &mut fake);
        assert!(fx.is_empty());
    }

    #[test]
    fn shadow_balls_come_and_go_with_the_count() {
        let mut fake = Fake::default();
        let mut balls = Balls::default();
        balls.set_count(3, false, &mut fake);
        for k in 0..3 {
            let ball = balls.balls[k].as_ref().unwrap();
            assert_eq!((ball.state, ball.scale, ball.offset, ball.part.controls.scale), (1, BALL_SCALES[k], BALL_OFFSETS[k], BALL_SCALES[k]));
        }
        assert!(balls.balls[3].is_none() && fake.strs.iter().all(|file| file == BALL_CREATE));
        // Down to 1: balls 2 and 1 collapse where they are.
        balls.set_count(1, false, &mut fake);
        assert_eq!(balls.balls.iter().map(|ball| ball.as_ref().map(|ball| ball.state)).collect::<Vec<_>>(), [Some(1), Some(3), Some(3), None, None]);
        assert_eq!(balls.balls[2].as_ref().map(|ball| (ball.scale, ball.offset)), Some((0.5, [-0.7, 14.0])));
        assert_eq!(fake.strs[3..], [BALL_DIE, BALL_DIE]);
        // A count over 5 does nothing.
        balls.set_count(6, false, &mut fake);
        assert_eq!(balls.count, 1);
        // The dying ones go after 18 frames; the made one loops after its 75.
        let mut solo = Solo { fx: Fx::of(&Launch::at(SHADOW_BALLS, OWNER, FEET)), class: balls };
        solo.ticks(19, &mut fake);
        assert!(solo.class.balls[1].is_none() && solo.class.balls[2].is_none());
        let [right, up] = screen_offset(solo.class.balls[0].as_ref().unwrap().part.point);
        assert!(close(right, -4.8) && close(up, 9.1));
        solo.ticks(56, &mut fake);
        assert_eq!(solo.class.balls[0].as_ref().unwrap().state, 1);
        solo.ticks(1, &mut fake);
        let ball = solo.class.balls[0].as_ref().unwrap();
        assert_eq!((ball.state, ball.scale, ball.part.timeline.looped), (2, 0.8, true));
        assert_eq!(fake.strs.last().unwrap(), BALL);
    }

    #[test]
    fn spirit_counts_start_their_objects_for_their_jobs_only() {
        let mut fx = WorldFx::default();
        let mut fake = Fake::default();
        spirits(&mut fx, OWNER, ABYSS_CHASER, 2, true, FEET, false, &mut fake);
        assert_eq!(fx.class_mut::<Balls>(OWNER, &[SHADOW_BALLS]).unwrap().1.count, 2);
        // Out of sight, or no spheres: no balls.
        spirits(&mut fx, OWNER, ABYSS_CHASER, 2, false, FEET, false, &mut fake);
        assert!(fx.is_empty());
        spirits(&mut fx, OWNER, ABYSS_CHASER, 3, true, FEET, false, &mut fake);
        spirits(&mut fx, OWNER, ABYSS_CHASER, 0, true, FEET, false, &mut fake);
        assert!(fx.is_empty());
        spirits(&mut fx, OWNER, 4215, 3, true, FEET, false, &mut fake);
        assert!(fx.is_empty());
        for job in [ABYSS_CHASER, 4252, 4265, 4280] {
            assert!(object_spheres(job), "{job}");
        }
        for job in [0, 4215, 4253, 4261, 4302] {
            assert!(!object_spheres(job), "{job}");
        }
    }

    #[test]
    fn the_frostbite_frame() {
        let mut fx = WorldFx::default();
        let mut fake = Fake::default();
        // Only the own player's frostbite frames the screen.
        status(&mut fx, OWNER, FEET, (FROSTBITE_STATUS, true, 1), false, &mut fake);
        assert!(fx.is_empty());
        status(&mut fx, ME, FEET, (FROSTBITE_STATUS, true, 1), false, &mut fake);
        assert!(fx.running(ME, FROST_FRAME) && fake.textures == [FROST_TEXTURE]);
        ticks(&mut fx, 1, &mut fake);
        assert_eq!(faces(&fx), 0);
        status(&mut fx, ME, FEET, (FROSTBITE_STATUS, false, 0), false, &mut fake);
        assert!(fx.is_empty());
        // The thaw plays as it ends; /effect off hides the frame.
        assert_eq!(end_effects(FROSTBITE_STATUS), [FROSTBITE_END]);
        assert!(end_effects(CLIMAX_STATUS).is_empty());
        assert!(!crate::effects::force_drawn(FROST_FRAME));
        // Four quarters, each mirrored so the frost lies on the outer edges.
        let (low, high) = (0.5 / 512.0, 512.5 / 512.0);
        let quads = frost_quads(1024.0, 768.0);
        assert_eq!(quads[0], ([[0.0, 0.0], [512.0, 0.0], [0.0, 384.0], [512.0, 384.0]], [[high, low], [low, low], [high, high], [low, high]]));
        assert_eq!(quads[1], ([[512.0, 0.0], [1024.0, 0.0], [512.0, 384.0], [1024.0, 384.0]], [[low, low], [high, low], [low, high], [high, high]]));
        assert_eq!(quads[2], ([[0.0, 384.0], [512.0, 384.0], [0.0, 768.0], [512.0, 768.0]], [[high, high], [low, high], [high, low], [low, low]]));
        assert_eq!(quads[3], ([[512.0, 384.0], [1024.0, 384.0], [512.0, 768.0], [1024.0, 768.0]], [[low, high], [high, high], [low, low], [high, low]]));
    }

    #[test]
    fn the_god_marks_circle_and_end() {
        let mut fake = Fake::default();
        let launch = |k: i32| Launch { a7: Some(k), ..Launch::at(GOD_MARKS, OWNER, FEET) };
        let mut solo = Solo::new(Marks::default(), launch(4), &mut fake);
        assert_eq!(solo.class.marks.iter().map(|mark| mark.angle).collect::<Vec<_>>(), [0, 180, 90, 270]);
        assert_eq!(solo.class.marks[3].part.flags, FLAGS_FLOOR);
        assert_eq!(fake.strs[..2], ["SOUL_ASCETIC\\EFST_T_FIRST_GOD\\blue_dragon_start", "SOUL_ASCETIC\\EFST_T_SECOND_GOD\\white_tiger_start"]);
        assert_eq!(Solo::new(Marks::default(), launch(6), &mut fake).class.marks.len(), 4);
        assert_eq!(Solo::new(Marks::default(), launch(2), &mut fake).class.marks.len(), 2);
        // The start's 8 frames stand still; then the loop takes over and turns a degree a tick.
        solo.ticks(8, &mut fake);
        assert_eq!((solo.class.marks[0].phase, solo.class.marks[0].angle), (Phase::Start, 0));
        solo.ticks(1, &mut fake);
        assert_eq!((solo.class.marks[0].phase, solo.class.marks[0].angle), (Phase::Loop, 0));
        assert_eq!(fake.strs.last().unwrap(), "SOUL_ASCETIC\\EFST_T_FOURTH_GOD\\black_tortoise_loop");
        solo.ticks(90, &mut fake);
        assert_eq!(solo.class.marks[0].angle, 90);
        let [right, up] = screen_offset(solo.class.marks[0].part.point);
        assert!(close(right, 0.0) && close(up, 11.5));
        let [right, up] = ring_offset(0);
        assert!(close(right, 8.5) && close(up, 20.0));
        // Off: each plays its end where it stands and the object ends after them.
        solo.class.end(false, &mut fake);
        assert!(solo.class.marks.iter().all(|mark| mark.phase == Phase::End));
        solo.ticks(9, &mut fake);
        assert_eq!(solo.class.marks[0].angle, 90);
        assert!(solo.alive());
        solo.ticks(1, &mut fake);
        assert!(!solo.alive());
    }

    #[test]
    fn the_next_god_replaces_the_marks() {
        let mut fx = WorldFx::default();
        let mut fake = Fake::default();
        status(&mut fx, OWNER, FEET, (1360, true, 1), false, &mut fake);
        // The first god ends as the second starts.
        status(&mut fx, OWNER, FEET, (1360, false, 0), false, &mut fake);
        assert!(fx.class_mut::<Marks>(OWNER, &[GOD_MARKS]).unwrap().1.ending);
        status(&mut fx, OWNER, FEET, (1361, true, 1), false, &mut fake);
        assert_eq!(running(&fx, GOD_MARKS), [2]);
        assert_eq!(fx.class_mut::<Marks>(OWNER, &[GOD_MARKS]).unwrap().1.marks.len(), 2);
        // An older god's end leaves them.
        status(&mut fx, OWNER, FEET, (1360, false, 0), false, &mut fake);
        assert!(!fx.class_mut::<Marks>(OWNER, &[GOD_MARKS]).unwrap().1.ending);
        status(&mut fx, OWNER, FEET, (1363, true, 1), false, &mut fake);
        assert_eq!(running(&fx, GOD_MARKS), [4]);
    }

    #[test]
    fn the_dragon_knights_swords() {
        let mut fx = WorldFx::default();
        let mut fake = Fake::default();
        spirits(&mut fx, OWNER, 4252, 7, true, FEET, false, &mut fake);
        let (object, swords) = fx.class_mut::<Swords>(OWNER, &[SERVANT_WEAPON]).unwrap();
        assert_eq!(swords.swords.iter().map(|sword| sword.angle).collect::<Vec<_>>(), [90.0, 162.0, 234.0, 306.0, 18.0]);
        assert!(swords.swords.iter().all(|sword| sword.part.world && sword.part.controls.scale == SWORD_SCALE));
        assert_eq!(object.events.len(), 5);
        // Sign's cast shoots the last sword, glowing red.
        sign_cast(&mut fx, OWNER, &mut fake);
        assert_eq!(fake.strs.last().unwrap(), "DRAGON_KNIGHT\\DK_SERVANT_W_SIGN\\servant_sign_shot\\servant_sign_shot");
        let (_, swords) = fx.class_mut::<Swords>(OWNER, &[SERVANT_WEAPON]).unwrap();
        assert_eq!((swords.swords.len(), swords.shots[0].angle), (4, 18.0));
        // A lower count shoots the rest as Servant Weapon.
        spirits(&mut fx, OWNER, 4280, 2, true, FEET, false, &mut fake);
        let (_, swords) = fx.class_mut::<Swords>(OWNER, &[SERVANT_WEAPON]).unwrap();
        assert_eq!((swords.swords.len(), swords.shots.len(), swords.shots[2].angle), (2, 3, 234.0));
        assert_eq!(fake.strs.last().unwrap(), "DRAGON_KNIGHT\\DK_SERVANTWEAPON\\servantweapon_shoot\\servantweapon_shoot");
        // The casts settle into the loop; the shots go after their 11 frames.
        ticks(&mut fx, 12, &mut fake);
        let (_, swords) = fx.class_mut::<Swords>(OWNER, &[SERVANT_WEAPON]).unwrap();
        assert!(swords.shots.is_empty());
        ticks(&mut fx, 26, &mut fake);
        assert_eq!(fake.strs.last().unwrap(), SWORD);
        assert_eq!(faces(&fx), 2);
        // Out of sight, or the status gone: no swords.
        spirits(&mut fx, OWNER, 4265, 2, false, FEET, false, &mut fake);
        assert!(fx.is_empty());
        spirits(&mut fx, OWNER, 4265, 2, true, FEET, false, &mut fake);
        status(&mut fx, OWNER, FEET, (SERVANT_STATUS, false, 0), false, &mut fake);
        assert!(fx.is_empty());
        // The shot files by skill, and the ring.
        assert!([5202, 5203, 5204, 5205].iter().all(|&skill| shot_file(skill).is_some()));
        assert_eq!([shot_file(5201), shot_file(5206)], [None, None]);
        assert_eq!(sword_point(FEET, 0.0), [43.0, 14.0, 50.0]);
        let [x, _, z] = sword_point(FEET, 90.0);
        assert!(close(x, 30.0) && close(z, 63.0));
    }

    #[test]
    fn the_pierce_orbs_gather_per_stack() {
        let mut fx = WorldFx::default();
        let mut fake = Fake::default();
        status(&mut fx, OWNER, FEET, (PIERCE_STATUS, true, 5), false, &mut fake);
        assert_eq!(fx.class_mut::<Orbs>(OWNER, &[CHARGING_PIERCE]).unwrap().1.orbs.len(), 5);
        assert_eq!([orb_offset(0), orb_offset(4), orb_offset(5), orb_offset(9)], [[-7.5, 23.45], [7.5, 23.45], [-7.5, 20.0], [7.5, 20.0]]);
        // Hold keeps the last frame.
        ticks(&mut fx, 60, &mut fake);
        assert_eq!(faces(&fx), 5);
        let (object, orbs) = fx.class_mut::<Orbs>(OWNER, &[CHARGING_PIERCE]).unwrap();
        let [right, up] = screen_offset(orbs.orbs[4].point);
        assert!(close(right, 7.5) && close(up, 23.45));
        assert_eq!(orbs.orbs[0].controls.scale, 0.75);
        // Over 10 does nothing; lowering gathers every orb again; raising gathers only the new ones.
        orbs.set_count(11, object.min, &mut fake);
        assert_eq!(orbs.orbs.len(), 5);
        orbs.set_count(3, false, &mut fake);
        assert!(orbs.orbs.len() == 3 && orbs.orbs.iter().all(|orb| orb.timeline.frame == 0));
        ticks(&mut fx, 2, &mut fake);
        let (_, orbs) = fx.class_mut::<Orbs>(OWNER, &[CHARGING_PIERCE]).unwrap();
        orbs.set_count(4, false, &mut fake);
        assert_eq!(orbs.orbs.iter().map(|orb| orb.timeline.frame).collect::<Vec<_>>(), [2, 2, 2, 0]);
        status(&mut fx, OWNER, FEET, (PIERCE_STATUS, false, 0), false, &mut fake);
        assert!(fx.is_empty());
    }

    /// A unit's facing for sprite direction `d` (0 south, turning west), seen from the camera at longitude 0 as `d`.
    fn rot(d: u8) -> f32 {
        crate::actor::rot_of(d)
    }

    #[test]
    fn the_shadow_exceed_double_stands_behind() {
        // One cell straight behind, against the way the unit faces.
        let steps: [[f32; 2]; 8] = [[0.0, -1.0], [-1.0, -1.0], [-1.0, 0.0], [-1.0, 1.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [1.0, -1.0]];
        for (d, [dx, dz]) in steps.into_iter().enumerate() {
            let length = dx.hypot(dz);
            let at = behind(FEET, rot(d as u8));
            assert!(close(at[0], FEET[0] - 5.0 * dx / length) && close(at[2], FEET[2] - 5.0 * dz / length) && at[1] == FEET[1], "{d}");
        }
        assert_eq!((0..8).map(shows_back).collect::<Vec<_>>(), [false, false, true, true, true, true, false, false]);
        let mut fake = Fake::default();
        let mut solo = Solo::new(Exceed::default(), Launch::at(SHADOW_EXCEED, OWNER, FEET), &mut fake);
        assert_eq!(solo.fx.events, [(0, sound("effect\\SHC_SHADOW_EXCEED.wav"))]);
        let biases: Vec<_> = solo.class.parts.iter().map(|shadow| (shadow.back, shadow.part.bias, shadow.part.flags, shadow.part.controls.scale)).collect();
        assert_eq!(
            biases,
            [(false, 0.0, FLAGS_FLOOR, 0.14), (false, 0.0, FLAGS_EFFECT, 0.14), (true, 3e-4, FLAGS_FLOOR, 0.14), (true, 3e-4, FLAGS_EFFECT, 0.14)]
        );
        // The mirrors by the direction seen: the front for 0 and 1, the back for 4 and 5.
        for (d, front, back) in [(0, true, false), (1, true, false), (2, false, false), (4, false, true), (5, false, true), (7, false, false)] {
            solo.tick(rot(d), 0.0, &mut fake);
            let mirrors: Vec<_> = solo.class.parts.iter().map(|shadow| shadow.part.controls.mirror_x).collect();
            assert_eq!(mirrors, [front, front, back, back], "{d}");
        }
        // The back image draws while the unit is seen from 2 to 5, pulled nearer by its bias.
        solo.tick(rot(3), 0.0, &mut fake);
        let at = behind(FEET, rot(3));
        let seen = Seen { eye: eye(0.0), shows: &|_| true, hidden: &|_| false, size: &|_| 1.0, ground: &|_, _| 0.0, dim: false };
        let mut out = Vec::new();
        solo.class.draw(&solo.fx, &seen, &mut out);
        assert_eq!(out.len(), 2);
        let centre = |face: &Face| -> Vector { std::array::from_fn(|axis| (face.vertices[0].position[axis] + face.vertices[5].position[axis]) / 2.0) };
        assert!(close(dot(sub(centre(&out[1]), [at[0], at[1] + 10.0, at[2]]), eye(0.0).forward), -0.3));
        solo.tick(rot(0), 0.0, &mut fake);
        let mut out = Vec::new();
        solo.class.draw(&solo.fx, &seen, &mut out);
        let at = behind(FEET, rot(0));
        assert!(close(dot(sub(centre(&out[0]), [at[0], at[1] + 12.0, at[2]]), eye(0.0).forward), 0.0));
    }

    #[test]
    fn the_shadow_exceed_double_ends_in_afterimages() {
        let mut fake = Fake::default();
        let mut solo = Solo::new(Exceed::default(), Launch::at(SHADOW_EXCEED, OWNER, FEET), &mut fake);
        // The bottoms go at tick 82, the figures at 96.
        solo.ticks(82, &mut fake);
        assert_eq!(solo.class.parts.len(), 4);
        solo.ticks(1, &mut fake);
        assert_eq!(solo.class.parts.len(), 2);
        solo.ticks(13, &mut fake);
        assert_eq!(solo.class.parts.len(), 2);
        solo.ticks(1, &mut fake);
        assert!(solo.class.parts.is_empty() && solo.fx.events.last() == Some(&(97, Event::Afterimage)));
        // Through the registry: the owner's afterimages start as it ends.
        let mut fx = WorldFx::default();
        fx.launch(&used(SHADOW_EXCEED_SKILL, OWNER, FEET, false).unwrap(), &mut fake);
        let fired = ticks(&mut fx, 100, &mut fake);
        assert!(fired.contains(&Fired::Afterimage(Some(UNIT))));
        assert!(fx.is_empty());
    }
}
