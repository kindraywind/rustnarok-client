//! Fourth-job shots and throws: Storm Cannon's sweep (1755), the shot STRs turned toward their target (Crimson Arrow
//! 1756, Savage Impact 1837, Frenzy Shot 2114, Unlucky Rush 2154), Jupitel Thunderstorm's ball and ring of bolts (2366)
//! and the Biolo's acid blob (2206, 2214, 2222, 2230).

use super::{Assets, Class, Event, FLAGS_EFFECT, FLAGS_FLOOR, Face, Fx, Launch, Seen, StrPart, WorldFx, add, aim_turn, normalized, scaled};
use crate::world::Vector;
use crate::world_str::Scene;

pub const STORM_CANNON: u32 = 1755;
pub const CRIMSON_ARROW: u32 = 1756;
pub const SAVAGE_IMPACT: u32 = 1837;
pub const FRENZY_SHOT: u32 = 2114;
pub const UNLUCKY_RUSH: u32 = 2154;
pub const JUPITEL: u32 = 2366;
/// The acid blob: fire, water, wind, ground.
pub const ACID: [u32; 4] = [2206, 2214, 2222, 2230];

/// AG_STORM_CANNON, AG_CRIMSON_ARROW, SHC_SAVAGE_IMPACT, ABC_FRENZY_SHOT, ABC_UNLUCKY_RUSH, HN_JUPITEL_THUNDER_STORM and
/// BO_ACIDIFIED_ZONE_WATER, _GROUND, _WIND, _FIRE.
pub const STORM_CANNON_SKILL: u16 = 5234;
pub const CRIMSON_ARROW_SKILL: u16 = 5235;
pub const SAVAGE_IMPACT_SKILL: u16 = 5287;
pub const FRENZY_SHOT_SKILL: u16 = 5322;
pub const UNLUCKY_RUSH_SKILL: u16 = 5315;
pub const JUPITEL_SKILL: u16 = 5456;
pub const ACID_WATER_SKILL: u16 = 5340;
pub const ACID_GROUND_SKILL: u16 = 5341;
pub const ACID_WIND_SKILL: u16 = 5342;
pub const ACID_FIRE_SKILL: u16 = 5343;

const STORM_CANNON_WAV: &str = "effect\\AG_STORM_CANNON.wav";
const JUPITEL_WAV: &str = "effect\\HYPER_NOVICE\\hu_jupitel_thunder_storm.wav";
const ACID_WAV: &str = "effect\\BO_ACIDIFIED_ZONE.wav";

/// Storm Cannon's sweep: units a tick, and alpha lost a tick once past the aim.
const CANNON_SPEED: f32 = 1.7;
const CANNON_FADE: i32 = 10;
/// Jupitel Thunderstorm's ball rolls to the aim in this many ticks.
const BALL_TICKS: u32 = 10;
/// The acid blob's flight parameter loses this much a tick.
const ACID_STEP: f32 = 0.07;
/// The age of the update the throw lands on (u ≤ 0): the 15th.
const ACID_LANDING: u32 = 14;

/// Registers the shots and throws.
pub fn register(fx: &mut WorldFx) {
    fx.register(STORM_CANNON, || Box::new(Cannon::default()));
    for id in [CRIMSON_ARROW, SAVAGE_IMPACT, FRENZY_SHOT, UNLUCKY_RUSH] {
        fx.register(id, || Box::new(Shot::default()));
    }
    fx.register(JUPITEL, || Box::new(Jupitel::new(Box::new(pick))));
    for id in ACID {
        fx.register(id, || Box::new(Acid::default()));
    }
}

/// The objects only a skill packet starts: they need its source and target.
pub fn is_class(effect: u32) -> bool {
    matches!(effect, STORM_CANNON | CRIMSON_ARROW | SAVAGE_IMPACT | FRENZY_SHOT | UNLUCKY_RUSH | JUPITEL) || ACID.contains(&effect)
}

/// The object a use-skill packet of `skill` starts.
pub fn used(skill: u16) -> Option<u32> {
    Some(match skill {
        STORM_CANNON_SKILL => STORM_CANNON,
        CRIMSON_ARROW_SKILL => CRIMSON_ARROW,
        FRENZY_SHOT_SKILL => FRENZY_SHOT,
        SAVAGE_IMPACT_SKILL => SAVAGE_IMPACT,
        JUPITEL_SKILL => JUPITEL,
        ACID_FIRE_SKILL => ACID[0],
        ACID_WATER_SKILL => ACID[1],
        ACID_WIND_SKILL => ACID[2],
        ACID_GROUND_SKILL => ACID[3],
        _ => return None,
    })
}

/// The object every damage packet of `skill` starts.
pub fn struck(skill: u16) -> Option<u32> {
    (skill == UNLUCKY_RUSH_SKILL).then_some(UNLUCKY_RUSH)
}

/// `effect`'s launch from `caster` standing at `from` toward `target` standing at `aim`, at `level`. Storm Cannon's sweep
/// has no owner: nothing moves it but itself.
pub fn launch(effect: u32, caster: Option<u32>, from: Vector, (target, aim): (Option<u32>, Vector), level: i32, min: bool) -> Launch {
    let owner = if effect == STORM_CANNON { None } else { Some(caster) };
    Launch { target: Some(target), aim, a7: Some(level), min, ..Launch::at(effect, owner, from) }
}

/// Draws one random whole number below its argument.
type Rand = Box<dyn FnMut(u32) -> u32>;

/// One random whole number below `n`.
fn pick(n: u32) -> u32 {
    crate::effects::pick(0, n.saturating_sub(1))
}

fn sound(file: &str) -> Event {
    Event::Sound { file: file.to_owned(), listener: false }
}

fn sub(a: Vector, b: Vector) -> Vector {
    std::array::from_fn(|axis| a[axis] - b[axis])
}

fn length(v: Vector) -> f32 {
    v.iter().map(|value| value * value).sum::<f32>().sqrt()
}

fn dot(a: Vector, b: Vector) -> f32 {
    a.iter().zip(b).map(|(a, b)| a * b).sum()
}

/// An STR part of `file` with no step offset, on the floor list or the effect list.
fn part(file: &str, min: bool, looped: bool, floor: bool, assets: &mut dyn Assets) -> StrPart {
    let mut part = StrPart::new(file, min, assets);
    part.offset = [0.0; 2];
    part.timeline.looped = looped;
    part.flags = if floor { FLAGS_FLOOR } else { FLAGS_EFFECT };
    part
}

/// The screen turn (`aim_turn`) from `from` to `to`, both in the world.
fn turn_toward(from: Vector, to: Vector, scene: &Scene) -> f32 {
    let step = sub(to, from);
    aim_turn([0.0; 2], [dot(step, scene.right), -dot(step, scene.up)])
}

/// A point moved `up` along the camera's up: the same on the screen at any latitude.
fn screen_up(point: Vector, up: f32, scene: &Scene) -> Vector {
    add(point, scaled(scene.up, up))
}

fn draw_all<'a>(parts: impl IntoIterator<Item = &'a StrPart>, seen: &Seen, out: &mut Vec<Face<'a>>) {
    for part in parts {
        part.faces(&seen.eye, out);
    }
}

// Storm Cannon (1755)

/// A pale blue whirlwind over a shining ground ring sweeping from the caster to the target's spot, fading just past it.
#[derive(Default)]
struct Cannon {
    parts: Vec<StrPart>,
    start: Vector,
    dir: Vector,
    reach: f32,
    alpha: i32,
}

impl Class for Cannon {
    fn begin(&mut self, fx: &mut Fx, assets: &mut dyn Assets) {
        let files = ["stormcannon\\stormcannon_bottom\\stormcannon_bottom", "stormcannon\\stormcannon\\stormcannon"];
        self.parts = files.iter().zip([true, false]).map(|(file, floor)| part(file, fx.min, true, floor, assets)).collect();
        self.start = fx.position;
        let along = sub(fx.aim, fx.position);
        self.reach = length(along);
        self.dir = normalized(along);
        self.alpha = 255;
        fx.events.push((0, sound(STORM_CANNON_WAV)));
    }

    fn update(&mut self, fx: &mut Fx, _: &Scene, _: &mut dyn Assets) {
        fx.position = add(fx.position, scaled(self.dir, CANNON_SPEED));
        // ponytail: with the target on the caster there is no direction; it stays and fades.
        if self.reach == 0.0 || length(sub(fx.position, self.start)) > self.reach {
            self.alpha = (self.alpha - CANNON_FADE).max(0);
        }
        let [x, y, z] = fx.position;
        for part in &mut self.parts {
            part.point = [x - 1.5, y + 17.0, z];
            part.controls.alpha = self.alpha as f32 / 255.0;
            part.step();
        }
    }

    fn draw<'a>(&'a self, _: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        draw_all(&self.parts, seen, out);
    }

    fn alive(&self, _: &Fx, _: &Scene) -> bool {
        self.alpha > 0
    }
}

// The aimed shots (1756, 1837, 2114, 2154)

/// A shot's STR and how far over the caster's feet it plays.
fn shot_file(effect: u32) -> (&'static str, f32) {
    match effect {
        CRIMSON_ARROW => ("crimsonarrow\\crimsonarrow\\crimsonarrow", 13.0),
        SAVAGE_IMPACT => ("savage_impact\\savage_impact\\savage_impact", 12.0),
        FRENZY_SHOT => ("frenzy_shot\\frenzy_shot\\frenzy_shot", 4.0),
        _ => ("unlucky_rush\\unlucky_rush\\unlucky_rush", 0.0),
    }
}

/// One STR on the caster turned on the screen toward where the target stood at launch.
#[derive(Default)]
struct Shot {
    part: Option<StrPart>,
    up: f32,
    turned: bool,
    over: bool,
}

impl Class for Shot {
    fn begin(&mut self, fx: &mut Fx, assets: &mut dyn Assets) {
        let (file, up) = shot_file(fx.id);
        self.part = Some(part(file, fx.min, false, false, assets));
        self.up = up;
    }

    fn update(&mut self, fx: &mut Fx, scene: &Scene, _: &mut dyn Assets) {
        let Some(part) = &mut self.part else { return };
        // The files point to the right: taken once, the turn points them at the aim.
        if !self.turned {
            part.controls.turn = turn_toward(fx.position, fx.aim, scene);
            self.turned = true;
        }
        part.point = add(fx.position, [0.0, self.up, 0.0]);
        self.over = !part.step();
    }

    fn draw<'a>(&'a self, _: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        draw_all(&self.part, seen, out);
    }

    fn alive(&self, fx: &Fx, scene: &Scene) -> bool {
        !self.over && fx.owned(scene)
    }
}

// Jupitel Thunderstorm (2366)

/// One of Jupitel Thunderstorm's bolts: its file's number, its offset from the aim, and its start delay.
#[derive(Debug, Clone, PartialEq)]
struct Bolt {
    file: u32,
    offset: Vector,
    delay_ms: i32,
}

/// The bolts at `level`: one every 80 ms while 80·m stays within ((level − 1) div 2 + 6) · 150, going round the target's
/// four quadrants, each a random distance below 5·((level − 1) div 4) + 12 out on each axis (z drawn first, then x, then the
/// file).
fn bolts(level: i32, rand: &mut dyn FnMut(u32) -> u32) -> Vec<Bolt> {
    let n = level.max(1);
    let reach = (5 * ((n - 1) / 4) + 12) as u32;
    let max = ((n - 1) / 2 + 6) * 150;
    const SIGNS: [(f32, f32); 4] = [(-1.0, 1.0), (1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)];
    (1..)
        .take_while(|m| 80 * m <= max)
        .enumerate()
        .map(|(i, m)| {
            let (tx, tz) = SIGNS[i % 4];
            let dz = (rand(reach) as f32 + 0.5) * tz;
            let dx = (rand(reach) as f32 + 0.5) * tx;
            Bolt { file: rand(3), offset: [dx, 15.0, dz], delay_ms: 80 * m }
        })
        .collect()
}

/// A crackling gold ball rolling from the Hyper Novice to the target, while gold-orange bolts strike round the target one
/// every 80 ms, thunder every fourth.
struct Jupitel {
    rand: Rand,
    ball: Option<StrPart>,
    vel: Vector,
    bolts: Vec<StrPart>,
}

impl Jupitel {
    fn new(rand: Rand) -> Self {
        Self { rand, ball: None, vel: [0.0; 3], bolts: Vec::new() }
    }
}

impl Class for Jupitel {
    fn begin(&mut self, fx: &mut Fx, assets: &mut dyn Assets) {
        let mut ball = part("HYPER_NOVICE\\HN_JUPITEL_THUNDER_STORM\\jupitel_thunder_storm_ball\\jupitel_thunder_storm_ball", false, false, false, assets);
        ball.point = fx.position;
        self.ball = Some(ball);
        self.vel = scaled(sub(fx.aim, fx.position), 1.0 / BALL_TICKS as f32);
        for (i, bolt) in bolts(fx.a7, &mut self.rand).into_iter().enumerate() {
            let file = format!("HYPER_NOVICE\\HN_JUPITEL_THUNDER_STORM\\jupitel_thunder_storm\\jupitel_thunder_storm_{:02}", bolt.file);
            let mut part = part(&file, false, false, false, assets);
            part.point = add(fx.aim, bolt.offset);
            part.timeline.delay = bolt.delay_ms;
            // Thunder with the first bolt of each round, a tick after its first frame.
            // ponytail: it sounds at the object's place, the caster's feet; the strikes are round the target.
            if i % 4 == 0 {
                fx.events.push((bolt.delay_ms as u32 / 16 + 1, sound(JUPITEL_WAV)));
            }
            self.bolts.push(part);
        }
    }

    fn update(&mut self, fx: &mut Fx, scene: &Scene, _: &mut dyn Assets) {
        if let Some(ball) = &mut self.ball {
            if fx.age == 0 {
                ball.controls.turn = turn_toward(ball.point, fx.aim, scene);
            }
            ball.point = if fx.age < BALL_TICKS { add(ball.point, self.vel) } else { fx.aim };
            if !ball.step() {
                self.ball = None;
            }
        }
        self.bolts.retain_mut(StrPart::step);
    }

    fn draw<'a>(&'a self, _: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        draw_all(self.ball.iter().chain(&self.bolts), seen, out);
    }

    fn alive(&self, _: &Fx, _: &Scene) -> bool {
        self.ball.is_some() || !self.bolts.is_empty()
    }
}

// The acid blob (2206, 2214, 2222, 2230)

/// The element's name in the acid files and sounds, by id: fire, water, wind, ground.
fn element(effect: u32) -> (&'static str, &'static str) {
    match effect {
        2214 => ("water", "WATER"),
        2222 => ("wind", "WIND"),
        2230 => ("ground", "GROUND"),
        _ => ("fire", "FIRE"),
    }
}

/// The acid STR `piece` (c, b, b_bottom, water) of an element: each in its own folder.
fn acid_file(name: &str, piece: &str) -> String {
    format!("acidified_zone_{name}\\acidified_zone_{name}_{piece}\\acidified_zone_{name}_{piece}")
}

/// `v` turned `degrees` about the vertical.
// ponytail: the turn's sign is unchecked: look at a throw to either side.
fn turned(v: Vector, degrees: f32) -> Vector {
    let (sin, cos) = degrees.to_radians().sin_cos();
    [v[0] * cos + v[2] * sin, v[1], -v[0] * sin + v[2] * cos]
}

/// The quadratic Bézier from `from` (u = 1) over `control` to `to` (u = 0).
fn bezier(from: Vector, control: Vector, to: Vector, u: f32) -> Vector {
    add(add(scaled(from, u * u), scaled(control, 2.0 * u * (1.0 - u))), scaled(to, (1.0 - u) * (1.0 - u)))
}

/// Where the throw is with its parameter at `u`, from the source's lifted hand `from` to the target at `to`, curving to the
/// side the target is on.
fn throw_point(from: Vector, to: Vector, u: f32, scene: &Scene) -> Vector {
    let side = if dot(to, scene.right) >= dot(from, scene.right) { 240.0 } else { 120.0 };
    let control = add(to, turned(sub(to, from), side));
    bezier(from, control, to, u)
}

/// A spinning blob of tinted liquid curving from the Biolo's hand to the target in 15 ticks, then a splash, a ground crack and
/// coloured smoke on the target.
#[derive(Default)]
struct Acid {
    throw: Option<StrPart>,
    u: f32,
    impacts: Vec<StrPart>,
    landed: bool,
    /// The element's name in the files.
    name: &'static str,
}

impl Class for Acid {
    fn begin(&mut self, fx: &mut Fx, assets: &mut dyn Assets) {
        let (name, upper) = element(fx.id);
        self.name = name;
        self.throw = Some(part(&acid_file(name, "c"), false, true, false, assets));
        self.u = 1.0;
        // The throw lands on the same update at any distance: the sounds play then.
        // ponytail: they sound at the object's place, the thrower's feet; the blob lands on the target.
        fx.events.push((ACID_LANDING, sound(ACID_WAV)));
        fx.events.push((ACID_LANDING, sound(&format!("effect\\BO_ACIDIFIED_ZONE_{upper}.wav"))));
    }

    fn update(&mut self, fx: &mut Fx, scene: &Scene, assets: &mut dyn Assets) {
        if !fx.owned(scene) {
            return;
        }
        fx.aim_at(scene);
        let to = fx.aim;
        if let Some(throw) = &mut self.throw {
            let from = screen_up(fx.position, 10.0, scene);
            self.u -= ACID_STEP;
            if self.u > 0.0 {
                throw.point = throw_point(from, to, self.u, scene);
                throw.step();
                return;
            }
            // Landed: the throw goes undrawn and the splash starts with the next tick.
            self.throw = None;
            let files = [("b", false), ("b_bottom", true), ("water", false)];
            self.impacts = files.iter().map(|&(piece, floor)| part(&acid_file(self.name, piece), false, false, floor, assets)).collect();
            self.landed = true;
            return;
        }
        for impact in &mut self.impacts {
            impact.point = [to[0] - 1.0, to[1] + 14.0, to[2]];
        }
        self.impacts.retain_mut(StrPart::step);
    }

    fn draw<'a>(&'a self, _: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        draw_all(self.throw.iter().chain(&self.impacts), seen, out);
    }

    fn alive(&self, fx: &Fx, scene: &Scene) -> bool {
        fx.owned(scene) && (!self.landed || !self.impacts.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::super::Eye;
    use super::*;
    use crate::actor::Placed;
    use crate::effects::Loaded;
    use rustnarok_client::effect::{Key, Layer, Str};
    use std::collections::HashMap;
    use std::rc::Rc;

    const CASTER: u32 = 0x700;
    const TARGET: u32 = 0x701;
    const FROM: Vector = [30.0, 4.0, 50.0];

    fn close(a: Vector, b: Vector) -> bool {
        a.iter().zip(b).all(|(a, b)| (a - b).abs() < 1e-3)
    }

    /// The last key of each fake STR, by its file.
    fn last_key(file: &str) -> i32 {
        let name = file.rsplit('\\').next().unwrap_or(file).trim_start_matches("min_");
        match name {
            "crimsonarrow" => 30,
            "frenzy_shot" => 21,
            "unlucky_rush" => 14,
            "savage_impact" => 11,
            "jupitel_thunder_storm_ball" => 29,
            name if name.starts_with("jupitel_thunder_storm_") => 69,
            name if name.ends_with("_b") => 60,
            name if name.ends_with("_bottom") && name.starts_with("acidified") => 50,
            name if name.ends_with("_water") => 84,
            _ => 39,
        }
    }

    /// STRs of one drawn layer, with keys at 0 and their last frame; the files asked for kept.
    #[derive(Default)]
    struct Fake {
        strs: Vec<String>,
    }

    impl Assets for Fake {
        fn texture(&mut self, _: &str) -> Option<[f32; 2]> {
            Some([64.0; 2])
        }
        fn str(&mut self, file: &str) -> Option<Rc<Loaded>> {
            self.strs.push(file.to_owned());
            let last = last_key(file);
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
            let layer = || Layer { textures: vec!["a.bmp".into()], keys: vec![key(0), key(last)] };
            Some(Rc::new(Loaded { animation: Str { fps: 60, frames: last as u32, layers: vec![layer(), layer()] }, textures: vec![Vec::new(); 2] }))
        }
        fn sprite(&mut self, _: &str, _: usize, _: usize) -> Option<(usize, Vec<Placed>)> {
            None
        }
    }

    /// Runs `f` on a scene with the caster at `FROM` and the target where `units` says, the camera at longitude 0.
    fn scene<R>(units: &HashMap<u32, Vector>, f: impl FnOnce(&Scene) -> R) -> R {
        let feet = |unit: Option<u32>| match unit {
            None => Some(FROM),
            Some(id) => units.get(&id).copied(),
        };
        let body = |_: Option<u32>| None;
        let eye = Eye::at(45.0, 0.0);
        f(&Scene { player: FROM, feet: &feet, body: &body, longitude: 0.0, right: eye.right, up: eye.up, ground: &|_, _| 0.0 })
    }

    fn units(target: Vector) -> HashMap<u32, Vector> {
        HashMap::from([(CASTER, FROM), (TARGET, target)])
    }

    /// One object of class `C` run on its own, as the registry runs it (an owner's feet set its position first).
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

        /// One tick; the sounds it fired.
        fn tick(&mut self, units: &HashMap<u32, Vector>, assets: &mut Fake) -> Vec<String> {
            let age = self.fx.age;
            let fired = self.fx.events.iter().filter(|(at, _)| *at == age).filter_map(|(_, event)| match event {
                Event::Sound { file, .. } => Some(file.clone()),
                _ => None,
            });
            let fired = fired.collect();
            scene(units, |scene| {
                if let Some(feet) = self.fx.owner.and_then(|owner| (scene.feet)(owner)) {
                    self.fx.position = feet;
                }
                self.class.update(&mut self.fx, scene, assets);
            });
            self.fx.updated = true;
            self.fx.age += 1;
            fired
        }

        fn alive(&self, units: &HashMap<u32, Vector>) -> bool {
            scene(units, |scene| self.class.alive(&self.fx, scene))
        }
    }

    fn shot_launch(effect: u32, target: Vector, level: i32) -> Launch {
        launch(effect, Some(CASTER), FROM, (Some(TARGET), target), level, false)
    }

    #[test]
    fn skills_start_their_objects() {
        assert_eq!(
            [STORM_CANNON_SKILL, CRIMSON_ARROW_SKILL, FRENZY_SHOT_SKILL, SAVAGE_IMPACT_SKILL, JUPITEL_SKILL].map(used),
            [Some(1755), Some(1756), Some(2114), Some(1837), Some(2366)]
        );
        assert_eq!([ACID_FIRE_SKILL, ACID_WATER_SKILL, ACID_WIND_SKILL, ACID_GROUND_SKILL].map(used), [Some(2206), Some(2214), Some(2222), Some(2230)]);
        // Unlucky Rush per damage packet only; Rhythm Shooting and Judgement Cross nothing.
        assert_eq!((used(UNLUCKY_RUSH_SKILL), struck(UNLUCKY_RUSH_SKILL)), (None, Some(2154)));
        assert_eq!((struck(CRIMSON_ARROW_SKILL), used(5236), used(5379)), (None, None, None));
        let fx = WorldFx::default();
        for id in [1755, 1756, 1837, 2114, 2154, 2366, 2206, 2214, 2222, 2230] {
            assert!(fx.registered(id) && is_class(id), "{id}");
        }
        // Storm Cannon's sweep has no owner; the rest follow the caster.
        let target = [60.0, 4.0, 50.0];
        assert_eq!(launch(STORM_CANNON, Some(CASTER), FROM, (Some(TARGET), target), 5, false).owner, None);
        let shot = shot_launch(CRIMSON_ARROW, target, 5);
        assert_eq!((shot.owner, shot.target, shot.aim, shot.a7), (Some(Some(CASTER)), Some(Some(TARGET)), target, Some(5)));
    }

    #[test]
    fn storm_cannon_sweeps_to_the_aim_and_fades_past_it() {
        let mut fake = Fake::default();
        // 33 units to the right: past it on the 20th tick.
        let target = [63.0, 4.0, 50.0];
        let at = units(target);
        let mut cannon = Solo::new(Cannon::default(), launch(STORM_CANNON, Some(CASTER), FROM, (Some(TARGET), target), 5, false), &mut fake);
        assert_eq!(fake.strs, ["stormcannon\\stormcannon_bottom\\stormcannon_bottom", "stormcannon\\stormcannon\\stormcannon"]);
        let flags: Vec<_> = cannon.class.parts.iter().map(|part| (part.flags, part.timeline.looped)).collect();
        assert_eq!(flags, [(FLAGS_FLOOR, true), (FLAGS_EFFECT, true)]);
        assert_eq!(cannon.tick(&at, &mut fake), [STORM_CANNON_WAV]);
        for _ in 1..10 {
            cannon.tick(&at, &mut fake);
        }
        // 17 units along after 10 ticks; the parts 1.5 left of it and 17 up.
        assert!(close(cannon.fx.position, [47.0, 4.0, 50.0]));
        assert!(cannon.class.parts.iter().all(|part| close(part.point, [45.5, 21.0, 50.0]) && part.controls.alpha == 1.0));
        // 255 until it passes 33 units (the 20th tick), then 26 ticks of fade while it keeps going.
        let mut alphas = Vec::new();
        while cannon.alive(&at) {
            cannon.tick(&at, &mut fake);
            alphas.push(cannon.class.alpha);
        }
        assert_eq!(alphas.iter().take_while(|&&alpha| alpha == 255).count(), 9);
        assert_eq!(alphas.len(), 9 + 26);
        assert_eq!(&alphas[9..12], [245, 235, 225]);
        assert!((cannon.class.parts[1].controls.alpha - 0.0).abs() < 1e-6);
        assert!(close(cannon.fx.position, [30.0 + 45.0 * 1.7, 4.0, 50.0]));
        // A target on the caster: it stays and fades.
        let mut still = Solo::new(Cannon::default(), launch(STORM_CANNON, None, FROM, (Some(TARGET), FROM), 1, true), &mut fake);
        assert!(fake.strs.ends_with(&["stormcannon\\stormcannon_bottom\\min_stormcannon_bottom".into(), "stormcannon\\stormcannon\\min_stormcannon".into()]));
        still.tick(&at, &mut fake);
        assert_eq!((still.fx.position, still.class.alpha), (FROM, 245));
    }

    #[test]
    fn the_shots_turn_toward_the_aim_once_and_follow_the_caster() {
        for (effect, file, up, frames) in [
            (CRIMSON_ARROW, "crimsonarrow\\crimsonarrow\\crimsonarrow", 13.0, 31),
            (FRENZY_SHOT, "frenzy_shot\\frenzy_shot\\frenzy_shot", 4.0, 22),
            (UNLUCKY_RUSH, "unlucky_rush\\unlucky_rush\\unlucky_rush", 0.0, 15),
            (SAVAGE_IMPACT, "savage_impact\\savage_impact\\savage_impact", 12.0, 12),
        ] {
            let mut fake = Fake::default();
            // To the right: no turn.
            let right = [50.0, 4.0, 50.0];
            let mut shot = Solo::new(Shot::default(), shot_launch(effect, right, 1), &mut fake);
            assert_eq!(fake.strs, [file]);
            shot.tick(&units(right), &mut fake);
            let part = shot.class.part.as_ref().unwrap();
            assert_eq!((part.controls.turn, part.offset, part.flags, part.timeline.looped), (0.0, [0.0; 2], FLAGS_EFFECT, false));
            assert!(close(part.point, add(FROM, [0.0, up, 0.0])));
            // Toward the camera (down the screen): 90, kept while the target walks off and the caster moves.
            let down = [30.0, 4.0, 30.0];
            let mut shot = Solo::new(Shot::default(), shot_launch(effect, down, 1), &mut fake);
            shot.tick(&units(down), &mut fake);
            let moved = HashMap::from([(CASTER, [35.0, 4.0, 52.0]), (TARGET, [80.0, 4.0, 50.0])]);
            shot.tick(&moved, &mut fake);
            let part = shot.class.part.as_ref().unwrap();
            assert!((part.controls.turn - 90.0).abs() < 1e-3, "{effect}");
            assert!(close(part.point, [35.0, 4.0 + up, 52.0]));
            // It ends with its STR.
            let mut ticks = 2;
            while shot.alive(&moved) {
                shot.tick(&moved, &mut fake);
                ticks += 1;
            }
            assert_eq!(ticks, frames + 1, "{effect}");
        }
    }

    #[test]
    fn a_shot_goes_with_its_caster() {
        let mut fake = Fake::default();
        let target = [50.0, 4.0, 50.0];
        let mut shot = Solo::new(Shot::default(), shot_launch(CRIMSON_ARROW, target, 1), &mut fake);
        shot.tick(&units(target), &mut fake);
        assert!(shot.alive(&units(target)));
        assert!(!shot.alive(&HashMap::from([(TARGET, target)])));
    }

    #[test]
    fn jupitel_bolts_by_level() {
        let counts: Vec<_> = (1..=10).map(|level| bolts(level, &mut |_| 0).len()).collect();
        assert_eq!(counts, [11, 11, 13, 13, 15, 15, 16, 16, 18, 18]);
        // The reach by level: 12, 17, 22; z drawn first, then x, then the file.
        for (level, reach) in [(1, 12), (4, 12), (5, 17), (8, 17), (9, 22)] {
            let mut asked = Vec::new();
            bolts(level, &mut |n| {
                asked.push(n);
                0
            });
            assert!(asked.chunks(3).all(|draws| draws == [reach, reach, 3]), "{level}");
        }
        let mut draws = [4, 7, 2, 1, 3, 0].into_iter().cycle();
        let bolts = bolts(1, &mut |_| draws.next().unwrap());
        assert_eq!(bolts[0], Bolt { file: 2, offset: [-7.5, 15.0, 4.5], delay_ms: 80 });
        assert_eq!(bolts[1], Bolt { file: 0, offset: [3.5, 15.0, 1.5], delay_ms: 160 });
        // Round the quadrants: (−, +), (+, +), (−, −), (+, −).
        let signs: Vec<_> = bolts.iter().take(4).map(|bolt| (bolt.offset[0].signum(), bolt.offset[2].signum())).collect();
        assert_eq!(signs, [(-1.0, 1.0), (1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)]);
    }

    #[test]
    fn jupitel_rolls_its_ball_and_strikes_round_the_aim() {
        let mut fake = Fake::default();
        let target = [50.0, 4.0, 30.0];
        let at = units(target);
        let mut jupitel = Solo::new(Jupitel::new(Box::new(|_| 1)), shot_launch(JUPITEL, target, 1), &mut fake);
        assert_eq!(fake.strs.len(), 1 + 11);
        assert!(fake.strs[1..].iter().all(|file| file.ends_with("jupitel_thunder_storm\\jupitel_thunder_storm_01")));
        // The bolts at the aim, 1.5 out each way and 15 up, 80·m ms late.
        let bolt = &jupitel.class.bolts[0];
        assert_eq!((bolt.point, bolt.timeline.delay, bolt.offset), ([48.5, 19.0, 31.5], 80, [0.0; 2]));
        assert_eq!(jupitel.class.bolts[10].timeline.delay, 880);
        // Thunder at 6, 26, 46 at level 1.
        let thunder: Vec<_> = jupitel.fx.events.iter().map(|(age, _)| *age).collect();
        assert_eq!(thunder, [6, 26, 46]);
        let level = |n| Solo::new(Jupitel::new(Box::new(|_| 0)), shot_launch(JUPITEL, target, n), &mut Fake::default()).fx.events.len();
        assert_eq!([1, 2, 3, 8, 9, 10].map(level), [3, 3, 4, 4, 5, 5]);
        // The ball: (aim − feet)/10 a tick for 10 ticks, then at the aim, turned toward it.
        let step = [2.0, 0.0, -2.0];
        jupitel.tick(&at, &mut fake);
        let ball = jupitel.class.ball.as_ref().unwrap();
        assert!(close(ball.point, add(FROM, step)));
        // 20 right and 20 toward the camera: 20 px right and 14.1 down at latitude 45.
        assert!((ball.controls.turn - 14.142_f32.atan2(20.0).to_degrees()).abs() < 1e-2);
        for _ in 1..10 {
            jupitel.tick(&at, &mut fake);
        }
        assert!(close(jupitel.class.ball.as_ref().unwrap().point, target));
        jupitel.tick(&at, &mut fake);
        assert!(close(jupitel.class.ball.as_ref().unwrap().point, target));
        // The last bolt ends on tick 125.
        let mut ended = None;
        while jupitel.alive(&at) {
            ended = Some(jupitel.fx.age);
            jupitel.tick(&at, &mut fake);
        }
        assert_eq!(ended, Some(125));
        // The first bolt shows its first frame on tick 5.
        let mut early = Solo::new(Jupitel::new(Box::new(|_| 0)), shot_launch(JUPITEL, target, 1), &mut fake);
        for age in 0..6 {
            early.tick(&at, &mut fake);
            assert_eq!(early.class.bolts[0].timeline.frame > 0, age >= 5, "{age}");
        }
    }

    #[test]
    fn the_acid_blob_curves_to_the_target_and_splashes() {
        let mut fake = Fake::default();
        for (effect, name, upper) in [(2206, "fire", "FIRE"), (2214, "water", "WATER"), (2222, "wind", "WIND"), (2230, "ground", "GROUND")] {
            let target = [60.0, 4.0, 50.0];
            let at = units(target);
            fake.strs.clear();
            let mut acid = Solo::new(Acid::default(), shot_launch(effect, target, 1), &mut fake);
            assert_eq!(fake.strs, [format!("acidified_zone_{name}\\acidified_zone_{name}_c\\acidified_zone_{name}_c")]);
            let mut sounds = Vec::new();
            for update in 1..=15 {
                sounds.extend(acid.tick(&at, &mut fake));
                assert_eq!(acid.class.throw.is_some(), update <= 14, "{update}");
                assert!(sounds.is_empty() == (update < 15), "{update}");
            }
            assert_eq!(sounds, [ACID_WAV.to_owned(), format!("effect\\BO_ACIDIFIED_ZONE_{upper}.wav")]);
            assert_eq!(
                fake.strs[1..],
                [
                    format!("acidified_zone_{name}\\acidified_zone_{name}_b\\acidified_zone_{name}_b"),
                    format!("acidified_zone_{name}\\acidified_zone_{name}_b_bottom\\acidified_zone_{name}_b_bottom"),
                    format!("acidified_zone_{name}\\acidified_zone_{name}_water\\acidified_zone_{name}_water"),
                ]
            );
            let flags: Vec<_> = acid.class.impacts.iter().map(|part| part.flags).collect();
            assert_eq!(flags, [FLAGS_EFFECT, FLAGS_FLOOR, FLAGS_EFFECT]);
        }
    }

    #[test]
    fn the_acid_throw_follows_its_bezier_to_either_side() {
        let mut fake = Fake::default();
        let eye = Eye::at(45.0, 0.0);
        let hand = add(FROM, scaled(eye.up, 10.0));
        for (target, side) in [([60.0, 4.0, 50.0], 240.0), ([0.0, 4.0, 50.0], 120.0)] {
            let at = units(target);
            let control = add(target, turned(sub(target, hand), side));
            let mut acid = Solo::new(Acid::default(), shot_launch(ACID[0], target, 1), &mut fake);
            let mut points = Vec::new();
            for _ in 0..14 {
                acid.tick(&at, &mut fake);
                points.push(acid.class.throw.as_ref().unwrap().point);
            }
            // u = 0.93 near the hand, 0.51 half way round, 0.02 near the target.
            assert!(close(points[0], bezier(hand, control, target, 0.93)));
            assert!(close(points[6], bezier(hand, control, target, 0.51)));
            assert!(close(points[13], bezier(hand, control, target, 0.02)));
            assert!(length(sub(points[0], hand)) < length(sub(points[0], target)));
            assert!(length(sub(points[13], target)) < 2.0);
        }
    }

    #[test]
    fn the_acid_splash_follows_the_target_and_ends() {
        let mut fake = Fake::default();
        let target = [60.0, 4.0, 50.0];
        let mut acid = Solo::new(Acid::default(), shot_launch(ACID[1], target, 1), &mut fake);
        for _ in 0..15 {
            acid.tick(&units(target), &mut fake);
        }
        // The splash starts the tick after the landing, 1 left of the target and 14 up, and goes with it.
        let walked = [62.0, 4.0, 48.0];
        acid.tick(&units(walked), &mut fake);
        assert!(acid.class.impacts.iter().all(|part| close(part.point, [61.0, 18.0, 48.0]) && part.timeline.frame == 1));
        // It ends with its last part: the smoke shows its 85 frames and is freed on the next tick.
        let mut ticks = 1;
        while acid.alive(&units(walked)) {
            acid.tick(&units(walked), &mut fake);
            ticks += 1;
        }
        assert_eq!(ticks, 86);
        // Or at once when the source is gone.
        let mut gone = Solo::new(Acid::default(), shot_launch(ACID[2], target, 1), &mut fake);
        gone.tick(&units(target), &mut fake);
        assert!(!gone.alive(&HashMap::from([(TARGET, target)])));
    }
}
