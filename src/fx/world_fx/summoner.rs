//! The Summoner (Doram) objects: Spirit Marble's three spirits (1096) and Tuna Party's tuna (1097) circling their owner,
//! the catnip meteors (1106, 1112) and Lunatic Carrot Beat's carrots (1099, 1111) falling on a spot, Silvervine Stem
//! Spear's four spears (1100) and Spirit of Savage's spirit (1128) flying at the target, Catnip Powdering's haze (1104),
//! Silvervine Root Twist's roots (1110) and Nyang Grass' tufts (1125).

use super::{Assets, Card, Class, End, FLAGS_EFFECT, FLAGS_TRANSLUCENT, Face, Fx, Launch, Seen, StrPart, Tex, WorldFx, triangles};
use crate::world::Vector;
use crate::world_str::Scene;

pub const MABLE: u32 = 1096;
pub const TUNA: u32 = 1097;
pub const CARROTS: u32 = 1099;
pub const STEM_SPEAR: u32 = 1100;
/// Silvervine Stem Spear's STR at the caster.
pub const STEM_SPEAR_STR: u32 = 1101;
pub const POWDER: u32 = 1104;
pub const METEOR: u32 = 1106;
pub const ROOTS: u32 = 1110;
pub const CARROTS_2: u32 = 1111;
pub const METEOR_2: u32 = 1112;
pub const GRASS: u32 = 1125;
pub const SAVAGE: u32 = 1128;
/// Spirit of Savage's STR on the target.
pub const SAVAGE_STR: u32 = 1129;

/// SU_SV_STEMSPEAR and SU_SVG_SPIRIT: their damage packets throw a spear or a spirit.
pub const STEM_SPEAR_SKILL: u16 = 5026;
pub const SAVAGE_SKILL: u16 = 5046;

/// Catnip Powdering's haze.
pub const SMOKE: &str = "data\\texture\\effect\\smoke2.bmp";
const GRASS_STR: &str = "su_nyanggrass\\su_nyanggrass";

/// Registers the Summoner objects.
pub fn register(fx: &mut WorldFx) {
    fx.register(MABLE, || Box::new(Orbit::mable()));
    fx.register(TUNA, || Box::new(Orbit::tuna()));
    for id in [CARROTS, CARROTS_2, METEOR, METEOR_2] {
        fx.register(id, || Box::new(Fall::new(pick)));
    }
    for id in [STEM_SPEAR, SAVAGE] {
        fx.register(id, || Box::new(Spear::default()));
    }
    fx.register(POWDER, || Box::new(Powder::new(pick)));
    fx.register(ROOTS, || Box::new(Roots::default()));
    fx.register(GRASS, || Box::new(Grass::default()));
}

/// One random whole number below `n`.
fn pick(n: u32) -> u32 {
    crate::effects::pick(0, n.saturating_sub(1))
}

/// A sprite under data\sprite\이팩트\.
fn effect_sprite(name: &str) -> String {
    format!("data\\sprite\\이팩트\\{name}")
}

/// A status object's launch: Spirit Marble starts afresh each time its status comes on, the others keep the copy running.
pub fn restarts_on_status(effect: u32) -> bool {
    effect == MABLE
}

/// What a damage packet starts.
#[derive(Debug, Clone, PartialEq)]
pub enum Start {
    /// An STR world effect: its id, owner (None = at the point) and point.
    Str(u32, Option<Option<u32>>, Vector),
    Object(Launch),
}

/// A damage packet of Silvervine Stem Spear (its STR at the caster's place, then its spears) or Spirit of Savage (its
/// spirit, then its STR on the target), from `caster` (None = the player) on `target`.
pub fn shot_starts(skill: u16, (caster, caster_at): (Option<u32>, Vector), (target, target_at): (Option<u32>, Vector)) -> Vec<Start> {
    let shot = |id: u32| Start::Object(Launch { target: Some(target), aim: target_at, ..Launch::at(id, Some(caster), caster_at) });
    match skill {
        STEM_SPEAR_SKILL => vec![Start::Str(STEM_SPEAR_STR, None, caster_at), shot(STEM_SPEAR)],
        SAVAGE_SKILL => vec![shot(SAVAGE), Start::Str(SAVAGE_STR, Some(target), target_at)],
        _ => Vec::new(),
    }
}

/// The carrot object a use packet of Lunatic Carrot Beat (5036) or its second skill (5043) throws.
pub fn carrots_of(skill: u16) -> Option<u32> {
    match skill {
        5036 => Some(CARROTS),
        5043 => Some(CARROTS_2),
        _ => None,
    }
}

/// The carrots a level throws: 3 for levels 1 – 2, 5 for 3 – 4, else 7 (also for −1 and 0).
pub fn carrot_count(level: i32) -> usize {
    match level {
        1 | 2 => 3,
        3 | 4 => 5,
        _ => 7,
    }
}

/// Catnip Powdering's side in cells: 3, 5 or 7, none past level 5.
pub fn powder_count(level: i32) -> Option<u32> {
    match level {
        1 | 2 => Some(3),
        3 | 4 => Some(5),
        5 => Some(7),
        _ => None,
    }
}

/// Nyang Grass' side in cells: 5, 7 or 9, none past level 5.
pub fn grass_count(level: i32) -> Option<u32> {
    match level {
        1 | 2 => Some(5),
        3 | 4 => Some(7),
        5 => Some(9),
        _ => None,
    }
}

/// Spirit Marble's frame: the orbit angle (whole degrees) against the camera's longitude (any number of turns) picks a
/// facing of 12.
pub fn mable_frame(a: f32, longitude: f32) -> usize {
    let mut k = a.to_degrees().trunc().rem_euclid(360.0) - longitude.rem_euclid(360.0);
    if k < 0.0 {
        k += 360.0;
    }
    let frame = ((k - 75.0) / 30.0).trunc() as i32;
    (if frame < 0 { frame + 12 } else { frame }) as usize
}

/// Tuna Party's frame: the orbit angle against the camera's longitude picks a facing of 8.
pub fn tuna_frame(a: f32, longitude: f32) -> usize {
    let mut d = a.to_degrees().rem_euclid(360.0) - longitude.rem_euclid(360.0);
    if d < 0.0 {
        d += 360.0;
    }
    let frame = ((d - 157.5) / 45.0).trunc() as i32;
    (if frame < 0 { frame + 8 } else { frame }) as usize
}

/// A card circling its owner.
struct Circler {
    card: Card,
    /// The orbit angle and the bob's, in radians.
    a: f32,
    b: f32,
    centre: Vector,
}

/// Spirit Marble's three spirits or Tuna Party's tuna, circling the owner's feet.
struct Orbit {
    tuna: bool,
    cards: Vec<Circler>,
}

impl Orbit {
    fn mable() -> Self {
        let cards = ["green", "red", "blue"]
            .iter()
            .enumerate()
            .map(|(i, colour)| {
                let mut card = Card::new(&effect_sprite(&format!("su_spritemable\\su_spritemable_{colour}")));
                card.scale = 0.3;
                card.alpha = 200.0;
                card.flags = 1;
                card.layer_colours = false;
                card.end = End::Still;
                Circler { card, a: i as f32 * 2.094_394_7, b: 0.0, centre: [0.0; 3] }
            })
            .collect();
        Self { tuna: false, cards }
    }

    fn tuna() -> Self {
        let mut card = Card::new(&effect_sprite("su_tunaparty\\su_tunaparty"));
        card.flags = 0x101;
        card.layer_colours = false;
        card.end = End::Still;
        Self { tuna: true, cards: vec![Circler { card, a: 0.0, b: 0.0, centre: [0.0; 3] }] }
    }

    /// The offset from the feet: the spirits 6 out, 3 – 9 up and bobbing; the tuna 10 out and 10 up.
    fn offset(&self, a: f32, b: f32) -> Vector {
        if self.tuna { [10.0 * a.sin(), 10.0, 10.0 * a.cos()] } else { [6.0 * a.sin(), 6.0 - 3.0 * b.sin(), 6.0 * a.cos()] }
    }
}

impl Class for Orbit {
    fn update(&mut self, fx: &mut Fx, scene: &Scene, assets: &mut dyn Assets) {
        let (tuna, feet) = (self.tuna, fx.position);
        for i in 0..self.cards.len() {
            let circler = &mut self.cards[i];
            if tuna {
                circler.a += 0.05;
            } else {
                circler.a += 0.02;
                circler.b += 0.05;
            }
            let (a, b) = (circler.a, circler.b);
            let offset = self.offset(a, b);
            let circler = &mut self.cards[i];
            circler.centre = std::array::from_fn(|axis| feet[axis] + offset[axis]);
            circler.card.frame = if tuna { tuna_frame(a, scene.longitude) } else { mable_frame(a, scene.longitude) };
            // End mode 3 never steps: this reads the frame picked.
            circler.card.step(assets);
        }
    }

    fn draw<'a>(&'a self, _: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        for circler in &self.cards {
            circler.card.faces(circler.centre, 1.0, false, &seen.eye, out);
        }
    }

    fn alive(&self, fx: &Fx, scene: &Scene) -> bool {
        fx.owned(scene)
    }
}

/// The catnip meteor or the carrots falling on a spot, 4 ticks a frame: 12 frames of fall, 12 of impact.
struct Fall {
    rand: fn(u32) -> u32,
    cards: Vec<(Card, Vector)>,
}

/// A card shows frame 12, the impact, at these counters; the camera shakes at each.
const IMPACT: std::ops::RangeInclusive<u32> = 48..=51;

impl Fall {
    fn new(rand: fn(u32) -> u32) -> Self {
        Self { rand, cards: Vec::new() }
    }
}

/// The points of `n` carrots round `point`: each x then z drawn over a 5n + 1 unit square.
pub fn carrot_points(point: Vector, n: usize, rand: &mut impl FnMut(u32) -> u32) -> Vec<Vector> {
    let span = 5 * n as u32;
    let half = span as f32 / 2.0;
    (0..n)
        .map(|_| {
            let x = point[0] + rand(span + 1) as f32 - half;
            let z = point[2] + rand(span + 1) as f32 - half;
            [x, point[1], z]
        })
        .collect()
}

impl Class for Fall {
    fn begin(&mut self, fx: &mut Fx, _: &mut dyn Assets) {
        let carrots = matches!(fx.id, CARROTS | CARROTS_2);
        let points = if carrots { carrot_points(fx.position, carrot_count(fx.a7), &mut self.rand) } else { vec![fx.position] };
        let sprite = effect_sprite(if carrots { "su_lunaticcarrotbeat\\su_lunaticcarrotbeat" } else { "su_cn_meteor\\su_cn_meteor" });
        self.cards = points
            .into_iter()
            .map(|point| {
                let mut card = Card::new(&sprite);
                card.flags = 1;
                card.delay = 4.0;
                card.bias = 1.5e-4;
                card.end = End::Stop;
                // The level-1 skills draw it faint.
                card.alpha = if matches!(fx.id, CARROTS | METEOR) { 100.0 } else { 255.0 };
                (card, point)
            })
            .collect();
        // Each card shakes the camera on every tick it shows the impact's first frame.
        for _ in &self.cards {
            fx.events.extend(IMPACT.map(|age| (age, super::Event::Quake)));
        }
    }

    /// The frame steps once a tick here, not once a drawn frame.
    fn update(&mut self, _: &mut Fx, _: &Scene, assets: &mut dyn Assets) {
        self.cards.retain(|(card, _)| !card.ended);
        for (card, _) in &mut self.cards {
            card.step(assets);
            // A sprite that can't load shows nothing and ends at once.
            card.ended |= card.frames == 0;
            card.e8 += 1;
        }
    }

    fn draw<'a>(&'a self, _: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        for (card, point) in &self.cards {
            card.faces(*point, 1.0, false, &seen.eye, out);
        }
    }

    fn alive(&self, _: &Fx, _: &Scene) -> bool {
        !self.cards.is_empty()
    }
}

/// The heading of a shot from `from` to `to` in whole degrees, 0 … 360.
pub fn heading(from: Vector, to: Vector) -> f32 {
    let (dx, dz) = (to[0] - from[0], to[2] - from[2]);
    (180.0 - dx.atan2(-dz).to_degrees()).trunc().rem_euclid(360.0)
}

/// One spear or spirit in flight.
struct Shot {
    card: Card,
    /// Below 0 it waits; drawn while 0 < counter < 16.
    counter: i32,
    offset: Vector,
    /// Only the low byte is drawn.
    alpha: i32,
    at: Vector,
}

/// Silvervine Stem Spear's four spears, one every 4 ticks, or Spirit of Savage's spirit, flying from the caster to the
/// target 10 up in 16 ticks.
#[derive(Default)]
struct Spear {
    shots: Vec<Shot>,
    heading: f32,
    speed: f32,
}

/// A shot's flight in ticks.
const FLIGHT: i32 = 16;

impl Class for Spear {
    fn begin(&mut self, fx: &mut Fx, _: &mut dyn Assets) {
        let spears = fx.id == STEM_SPEAR;
        let (dx, dz) = (fx.aim[0] - fx.position[0], fx.aim[2] - fx.position[2]);
        self.heading = heading(fx.position, fx.aim);
        self.speed = (dx * dx + dz * dz).sqrt() / FLIGHT as f32;
        let sprite = if spears { effect_sprite("sv_stemspear\\sv_stemspear") } else { "data\\sprite\\몬스터\\svg_spirit".to_owned() };
        self.shots = (0..if spears { 4 } else { 1 })
            .map(|i| {
                let mut card = Card::new(&sprite);
                card.flags = 0x101;
                Shot { card, counter: -4 * i, offset: [0.0; 3], alpha: 255, at: fx.position }
            })
            .collect();
        fx.length = if spears { 27 } else { 15 };
    }

    fn update(&mut self, fx: &mut Fx, scene: &Scene, assets: &mut dyn Assets) {
        fx.aim_at(scene);
        let toward: Vector = std::array::from_fn(|axis| fx.aim[axis] - fx.position[axis]);
        let length = toward.iter().map(|v| v * v).sum::<f32>().sqrt();
        let step: Vector = if length > 0.0 { toward.map(|v| v / length * self.speed) } else { [0.0; 3] };
        for shot in &mut self.shots {
            if shot.counter >= 0 {
                // ponytail: the turn's sign against the longitude is unchecked: look at one throw.
                shot.card.spin = self.heading - scene.longitude;
                shot.offset = std::array::from_fn(|axis| shot.offset[axis] + step[axis]);
                shot.at = [fx.position[0] + shot.offset[0], fx.position[1] + 10.0, fx.position[2] + shot.offset[2]];
                if fx.age >= 10 {
                    shot.alpha -= 40;
                }
                shot.card.alpha = f32::from((shot.alpha & 0xff) as u8);
                shot.card.step(assets);
            }
            shot.counter += 1;
        }
    }

    fn draw<'a>(&'a self, _: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        for shot in self.shots.iter().filter(|shot| 0 < shot.counter && shot.counter < FLIGHT) {
            shot.card.faces(shot.at, 1.0, false, &seen.eye, out);
        }
    }
}

/// One of Catnip Powdering's glows: it swells while it fades in to its peak and out, then takes a new look.
#[derive(Debug, Clone, PartialEq)]
struct Haze {
    offset: [f32; 2],
    growth: f32,
    color: [f32; 3],
    max: i32,
    length: i32,
    scale: f32,
    alpha: i32,
    /// 0 fading in, 1 out, 2 done.
    state: u8,
}

impl Haze {
    fn new(n: u32, rand: &mut impl FnMut(u32) -> u32) -> Self {
        let mut haze = Self { offset: spot(n, rand), growth: 0.0, color: [1.0; 3], max: 0, length: 1, scale: 0.0, alpha: 0, state: 0 };
        haze.look(rand);
        haze
    }

    /// Its look: green and blue drawn apart, its life and its peak.
    fn look(&mut self, rand: &mut impl FnMut(u32) -> u32) {
        let g = rand(50) as f32;
        self.length = 30 + rand(21) as i32;
        self.max = 50 + rand(100) as i32;
        let b = rand(100) as f32;
        let r = rand(50) as f32;
        self.growth = (0.1 * (g + 1.0) + 6.0) / (2.0 * self.length as f32);
        self.color = [(115.0 - r) / 255.0, 1.0, (150.0 - b) / 255.0];
        (self.scale, self.alpha, self.state) = (0.0, 0, 0);
    }

    fn update(&mut self, standing: bool, rand: &mut impl FnMut(u32) -> u32) {
        if self.state == 2 {
            return;
        }
        self.scale += self.growth;
        let step = self.max / self.length;
        if self.state == 0 {
            self.alpha += step;
            if self.alpha >= self.max {
                (self.alpha, self.state) = (self.max, 1);
            }
        } else {
            self.alpha -= step;
            if self.alpha < 0 {
                if standing {
                    self.look(rand);
                } else {
                    (self.alpha, self.state) = (0, 2);
                }
            }
        }
    }
}

/// A spot of Catnip Powdering's square, of 5n units.
fn spot(n: u32, rand: &mut impl FnMut(u32) -> u32) -> [f32; 2] {
    let x = rand(5 * n) as f32 - (5 * n / 2) as f32 + 1.0;
    let z = rand(5 * n) as f32 - (5 * n / 2) as f32 + 1.0;
    [x, z]
}

/// One of Catnip Powdering's specks: a pale dot that twinkles in and out while it sinks, then shows somewhere else.
struct Speck {
    card: Card,
    offset: Vector,
    alpha: i32,
    /// 0 fading in, 1 out, 2 done.
    state: u8,
}

impl Speck {
    fn new(n: u32, rand: &mut impl FnMut(u32) -> u32) -> Self {
        let mut card = Card::new(&effect_sprite("particle2"));
        card.flags = 1;
        card.layer_colours = false;
        card.alpha = 0.0;
        card.end = End::Still;
        let mut speck = Self { card, offset: [0.0; 3], alpha: 0, state: 0 };
        speck.place(n, rand);
        speck
    }

    /// A new spot (x, height, z) and size.
    fn place(&mut self, n: u32, rand: &mut impl FnMut(u32) -> u32) {
        let x = rand(5 * n) as f32 - (5 * n / 2) as f32 + 1.0;
        let y = 7.0 + rand(7) as f32;
        let z = rand(5 * n) as f32 - (5 * n / 2) as f32 + 1.0;
        self.offset = [x, y, z];
        self.card.scale = 0.5 - 0.1 * (rand(3) + 1) as f32;
        (self.alpha, self.state) = (0, 0);
    }

    fn update(&mut self, n: u32, standing: bool, rand: &mut impl FnMut(u32) -> u32) {
        match self.state {
            0 => {
                self.alpha += rand(30) as i32;
                if self.alpha >= 255 {
                    (self.alpha, self.state) = (254, 1);
                }
            }
            1 => {
                self.alpha -= rand(30) as i32;
                if self.alpha < 0 {
                    if standing {
                        self.place(n, rand);
                    } else {
                        (self.alpha, self.state) = (0, 2);
                    }
                }
            }
            _ => return,
        }
        self.offset[1] -= rand(10) as f32 * 0.01;
        self.card.alpha = self.alpha as f32;
    }
}

/// Catnip Powdering's n² glows and n² specks over its unit's n × n cells, for as long as the unit stands.
struct Powder {
    rand: fn(u32) -> u32,
    n: u32,
    haze: Vec<Haze>,
    specks: Vec<Speck>,
}

impl Powder {
    fn new(rand: fn(u32) -> u32) -> Self {
        Self { rand, n: 0, haze: Vec::new(), specks: Vec::new() }
    }

    fn done(&self) -> bool {
        self.haze.iter().all(|haze| haze.state == 2) && self.specks.iter().all(|speck| speck.state == 2)
    }
}

impl Class for Powder {
    fn begin(&mut self, fx: &mut Fx, assets: &mut dyn Assets) {
        let Some(n) = powder_count(fx.a7) else { return };
        assets.texture(SMOKE);
        self.n = n;
        self.haze = (0..n * n).map(|_| Haze::new(n, &mut self.rand)).collect();
        self.specks = (0..n * n).map(|_| Speck::new(n, &mut self.rand)).collect();
    }

    fn update(&mut self, fx: &mut Fx, _: &Scene, assets: &mut dyn Assets) {
        let standing = !fx.released;
        for haze in &mut self.haze {
            haze.update(standing, &mut self.rand);
        }
        for speck in &mut self.specks {
            speck.update(self.n, standing, &mut self.rand);
            speck.card.step(assets);
        }
    }

    fn draw<'a>(&'a self, fx: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        let at = |x: f32, y: f32, z: f32| [fx.position[0] + x, fx.position[1] + y, fx.position[2] + z];
        for haze in self.haze.iter().filter(|haze| haze.state != 2) {
            let point = at(haze.offset[0], 10.0, haze.offset[1]);
            let corner = |x: f32, y: f32| std::array::from_fn(|axis| point[axis] + haze.scale * (x * seen.eye.right[axis] + y * seen.eye.up[axis]));
            let tint = [haze.color[0], haze.color[1], haze.color[2], f32::from((haze.alpha & 0xff) as u8) / 255.0];
            out.push(Face {
                texture: Tex::Path(SMOKE),
                vertices: triangles(
                    [corner(-1.0, 1.0), corner(1.0, 1.0), corner(-1.0, -1.0), corner(1.0, -1.0)],
                    [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]],
                    tint,
                ),
                additive: true,
                flags: FLAGS_EFFECT,
            });
        }
        for speck in self.specks.iter().filter(|speck| speck.state != 2) {
            speck.card.faces(at(speck.offset[0], speck.offset[1], speck.offset[2]), 1.0, false, &seen.eye, out);
        }
    }

    /// While its unit stands, then until every piece has finished; far away (where nothing steps) at once.
    fn alive(&self, fx: &Fx, _: &Scene) -> bool {
        !(fx.released || self.haze.is_empty()) || !(self.done() || fx.far)
    }
}

/// Silvervine Root Twist's roots, 3 units below the owner's feet, looping 8 ticks a frame.
#[derive(Default)]
struct Roots {
    card: Option<Card>,
}

impl Class for Roots {
    fn begin(&mut self, _: &mut Fx, _: &mut dyn Assets) {
        let mut card = Card::new(&effect_sprite("su_sv_roottwist\\su_sv_roottwist"));
        card.flags = 0x101;
        card.delay = 8.0;
        card.bias = 1.5e-4;
        self.card = Some(card);
    }

    fn update(&mut self, _: &mut Fx, _: &Scene, _: &mut dyn Assets) {
        if let Some(card) = &mut self.card {
            card.e8 += 1;
        }
    }

    fn frame(&mut self, _: &Fx, assets: &mut dyn Assets) {
        if let Some(card) = &mut self.card {
            card.step(assets);
        }
    }

    fn draw<'a>(&'a self, fx: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        if let Some(card) = &self.card {
            card.faces([fx.position[0], fx.position[1] - 3.0, fx.position[2]], 1.0, false, &seen.eye, out);
        }
    }

    fn alive(&self, fx: &Fx, scene: &Scene) -> bool {
        fx.owned(scene)
    }
}

/// Nyang Grass' tufts, one looping STR on each of its unit's n × n cells.
#[derive(Default)]
struct Grass {
    parts: Vec<StrPart>,
}

impl Class for Grass {
    fn begin(&mut self, fx: &mut Fx, assets: &mut dyn Assets) {
        let Some(n) = grass_count(fx.a7) else { return };
        let h = (n / 2) as f32;
        for r in 0..n {
            for c in 0..n {
                let mut part = StrPart::new(GRASS_STR, false, assets);
                part.point = [fx.position[0] + 5.0 * (c as f32 - h), fx.position[1], fx.position[2] + 5.0 * (r as f32 - h)];
                part.timeline.looped = true;
                part.offset = [0.0; 2];
                part.flags = FLAGS_TRANSLUCENT;
                self.parts.push(part);
            }
        }
    }

    fn update(&mut self, _: &mut Fx, _: &Scene, _: &mut dyn Assets) {
        for part in &mut self.parts {
            part.step();
        }
    }

    fn draw<'a>(&'a self, _: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        for part in &self.parts {
            part.faces(&seen.eye, out);
        }
    }

    fn alive(&self, fx: &Fx, _: &Scene) -> bool {
        !fx.released && !self.parts.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Eye, Lists};
    use super::*;
    use crate::actor::Placed;
    use crate::effects::Loaded;
    use crate::world_str::Fired;
    use rustnarok_client::effect::{Key, Layer, Str};
    use std::rc::Rc;

    const OWNER: u32 = 0x800;
    const TARGET: u32 = 0x900;
    const FEET: Vector = [30.0, 4.0, 50.0];

    /// Sprites of one 32 px layer with each file's frame count; every STR 50 frames of one layer; the files asked for kept.
    #[derive(Default)]
    struct Fake {
        strs: Vec<String>,
        textures: Vec<String>,
    }

    fn frames_of(path: &str) -> usize {
        let name = path.rsplit('\\').next().unwrap_or(path);
        match name {
            "su_spritemable_green" | "su_spritemable_red" | "su_spritemable_blue" => 12,
            "su_tunaparty" => 8,
            "su_cn_meteor" | "su_lunaticcarrotbeat" => 24,
            "su_sv_roottwist" => 22,
            _ => 1,
        }
    }

    impl Assets for Fake {
        fn texture(&mut self, path: &str) -> Option<[f32; 2]> {
            self.textures.push(path.to_owned());
            Some([128.0; 2])
        }
        fn str(&mut self, file: &str) -> Option<Rc<Loaded>> {
            self.strs.push(file.to_owned());
            let key = |frame| Key {
                frame,
                kind: 0,
                position: [320.0, 240.0],
                corners: [-30.0, 30.0, 30.0, -30.0, -20.0, -20.0, 20.0, 20.0],
                texture: 0.0,
                texture_motion: 0,
                delay: 0.0,
                angle: 0.0,
                color: [1.0, 1.0, 1.0, 165.0 / 255.0],
                source_blend: 5,
                destination_blend: 6,
            };
            let layer = || Layer { textures: vec!["a.bmp".into()], keys: vec![key(0), key(49)] };
            Some(Rc::new(Loaded { animation: Str { fps: 60, frames: 49, layers: vec![layer(), layer()] }, textures: vec![Vec::new(); 2] }))
        }
        fn sprite(&mut self, path: &str, _: usize, frame: usize) -> Option<(usize, Vec<Placed>)> {
            Some((frames_of(path), vec![((0, frame as u32, 0, [255; 4], 0), [-16.0, -16.0], [32.0, 32.0], 0.0)]))
        }
    }

    /// Runs `f` on a scene with the units in `here` standing at their feet (None = the player, at the origin).
    fn scene<R>(here: &[(u32, Vector)], longitude: f32, f: impl FnOnce(&Scene) -> R) -> R {
        let feet = |unit: Option<u32>| match unit {
            Some(id) => here.iter().find(|(each, _)| *each == id).map(|(_, at)| *at),
            None => Some([0.0; 3]),
        };
        let body = |_: Option<u32>| None;
        let eye = Eye::at(45.0, longitude);
        f(&Scene { player: FEET, feet: &feet, body: &body, longitude, right: eye.right, up: eye.up, ground: &|_, _| 0.0 })
    }

    fn seen<'a>(shows: &'a dyn Fn(u32) -> bool) -> Seen<'a> {
        Seen { eye: Eye::at(45.0, 0.0), shows, hidden: &|_| false, size: &|_| 1.0, ground: &|_, _| 0.0, dim: false }
    }

    fn all(_: u32) -> bool {
        true
    }

    /// A fixed draw: the largest, one less than n.
    fn top(n: u32) -> u32 {
        n.saturating_sub(1)
    }

    /// One object run on its own as the registry runs it: it follows its owner, updates and ages.
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

        fn tick(&mut self, here: &[(u32, Vector)], assets: &mut Fake) {
            scene(here, 0.0, |scene| {
                if let Some(feet) = self.fx.owner.and_then(|owner| (scene.feet)(owner)) {
                    self.fx.position = feet;
                }
                self.class.update(&mut self.fx, scene, assets);
            });
            self.fx.updated = true;
            self.fx.age += 1;
        }

        fn alive(&self, here: &[(u32, Vector)]) -> bool {
            scene(here, 0.0, |scene| self.class.alive(&self.fx, scene))
        }

        fn faces(&self) -> Vec<Face<'_>> {
            let mut out = Vec::new();
            self.class.draw(&self.fx, &seen(&all), &mut out);
            out
        }
    }

    fn tick(fx: &mut WorldFx, here: &[(u32, Vector)], assets: &mut Fake) -> Vec<Fired> {
        scene(here, 0.0, |scene| fx.tick(scene, assets))
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    fn close3(a: Vector, b: Vector) -> bool {
        (0..3).all(|axis| close(a[axis], b[axis]))
    }

    /// A face's centre.
    fn centre(face: &Face) -> Vector {
        std::array::from_fn(|axis| face.vertices.iter().map(|vertex| vertex.position[axis]).sum::<f32>() / 6.0)
    }

    const HERE: [(u32, Vector); 1] = [(OWNER, FEET)];

    #[test]
    fn spirit_marbles_three_spirits_circle_120_degrees_apart() {
        let mut assets = Fake::default();
        let mut solo = Solo::new(Orbit::mable(), Launch::at(MABLE, Some(Some(OWNER)), FEET), &mut assets);
        let start: Vec<f32> = solo.class.cards.iter().map(|card| card.a).collect();
        assert!(close(start[0], 0.0) && close(start[1], 2.094_394_7) && close(start[2], 4.188_789_4));
        let sprites: Vec<&str> = solo.class.cards.iter().map(|card| card.card.sprite.as_str()).collect();
        assert_eq!(sprites, ["green", "red", "blue"].map(|colour| format!("data\\sprite\\이팩트\\su_spritemable\\su_spritemable_{colour}")));
        // The centre for a = 0, b = 0: 6 out along z, 6 up.
        assert_eq!(solo.class.offset(0.0, 0.0), [0.0, 6.0, 6.0]);
        let mut heights = (f32::MAX, f32::MIN);
        for _ in 0..100 {
            solo.tick(&HERE, &mut assets);
            let y = solo.class.cards[0].centre[1] - FEET[1];
            heights = (heights.0.min(y), heights.1.max(y));
        }
        let first = &solo.class.cards[0];
        assert!(close(first.a, 2.0) && close(first.b, 5.0));
        // 3 – 9 up.
        assert!(heights.0 >= 3.0 - 1e-3 && heights.1 <= 9.0 + 1e-3 && heights.0 < 3.2 && heights.1 > 8.8, "{heights:?}");
        assert!(close((first.centre[0] - FEET[0]).hypot(first.centre[2] - FEET[2]), 6.0));
        let card = &first.card;
        assert_eq!((card.scale, card.alpha, card.flags, card.layer_colours, card.end), (0.3, 200.0, 1, false, End::Still));
        assert_eq!(card.frame, mable_frame(2.0, 0.0));
        assert_eq!(solo.faces().len(), 3);
        // The owner gone takes it.
        assert!(solo.alive(&HERE) && !solo.alive(&[]));
    }

    #[test]
    fn spirit_marbles_frame_faces_its_way_round() {
        let deg = |d: f32| d.to_radians() + 1e-4;
        assert_eq!(mable_frame(deg(30.0), 0.0), 11);
        assert_eq!(mable_frame(deg(75.0), 0.0), 0);
        assert_eq!(mable_frame(deg(135.0), 0.0), 2);
        assert_eq!(mable_frame(deg(359.0), 0.0), 9);
        // The longitude turns it: orbit 105 seen from 30 is k = 75; orbit 10 seen from 40 wraps to k = 330.
        assert_eq!(mable_frame(deg(105.0), 30.0), 0);
        assert_eq!(mable_frame(deg(10.0), 40.0), 8);
        // A longitude past a whole turn either way is the same.
        assert_eq!(mable_frame(deg(359.0), -90.0), mable_frame(deg(359.0), 270.0));
        assert_eq!(mable_frame(deg(10.0), 760.0), 8);
        // Frame 0 covers 59 degrees, 1 – 8 and 11 30, 9 15 and 10 16.
        let covered = |frame: usize| (0..360).filter(|&k| mable_frame(deg(k as f32), 0.0) == frame).count();
        assert_eq!((0..12).map(covered).collect::<Vec<_>>(), [59, 30, 30, 30, 30, 30, 30, 30, 30, 15, 16, 30]);
    }

    #[test]
    fn spirit_marble_hides_with_mable_off_and_keeps_orbiting() {
        let mut assets = Fake::default();
        let mut fx = WorldFx::default();
        fx.launch(&Launch::at(MABLE, Some(Some(OWNER)), FEET), &mut assets);
        tick(&mut fx, &HERE, &mut assets);
        let before: Vec<Vector> = fx.faces(&seen(&all)).iter().map(centre).collect();
        assert_eq!(before.len(), 3);
        // /mable off: the draw's gate shows every id but 1096.
        let mable_off = |id: u32| id != MABLE;
        for _ in 0..10 {
            tick(&mut fx, &HERE, &mut assets);
            assert!(fx.faces(&seen(&mable_off)).is_empty());
        }
        let after: Vec<Vector> = fx.faces(&seen(&all)).iter().map(centre).collect();
        assert!(!close3(before[0], after[0]));
        assert_eq!(fx.iter().count(), 1);
    }

    #[test]
    fn tuna_party_swims_round_its_unit() {
        assert_eq!(tuna_frame(0.0, 0.0), 5);
        assert_eq!(tuna_frame(160f32.to_radians(), 0.0), 0);
        assert_eq!(tuna_frame(300f32.to_radians(), 0.0), 3);
        assert_eq!(tuna_frame(10f32.to_radians(), 10.0), 5);
        assert_eq!(tuna_frame(10f32.to_radians(), -350.0), 5);
        let mut assets = Fake::default();
        let mut solo = Solo::new(Orbit::tuna(), Launch::at(TUNA, Some(Some(OWNER)), FEET), &mut assets);
        for _ in 0..126 {
            solo.tick(&HERE, &mut assets);
        }
        // One turn per 126 ticks, 10 out and 10 up.
        let circler = &solo.class.cards[0];
        assert!(close(circler.a, 6.3));
        assert!(close(circler.centre[1] - FEET[1], 10.0) && close((circler.centre[0] - FEET[0]).hypot(circler.centre[2] - FEET[2]), 10.0));
        let card = &circler.card;
        assert_eq!((card.scale, card.alpha, card.flags, card.layer_colours, card.end), (1.0, 255.0, 0x101, false, End::Still));
        assert_eq!(card.frame, tuna_frame(circler.a, 0.0));
        // A status sent again keeps the tuna running: one copy.
        let mut fx = WorldFx::default();
        let unique = Launch { unique: true, ..Launch::at(TUNA, Some(Some(OWNER)), FEET) };
        let serial = fx.launch(&unique, &mut assets);
        assert_eq!(fx.launch(&unique, &mut assets), serial);
        assert_eq!(fx.iter().count(), 1);
        assert!(restarts_on_status(MABLE) && !restarts_on_status(TUNA) && !restarts_on_status(ROOTS));
    }

    #[test]
    fn the_carrot_counts_and_their_spots() {
        assert_eq!([1, 2, 3, 4, 5, 6, -1, 0].map(carrot_count), [3, 3, 5, 5, 7, 7, 7, 7]);
        assert_eq!((carrots_of(5036), carrots_of(5043), carrots_of(5028)), (Some(CARROTS), Some(CARROTS_2), None));
        // n = 3: R(16) − 7.5, x then z.
        let mut draws = vec![0, 15, 8, 3, 1, 2].into_iter();
        let mut asked = Vec::new();
        let points = carrot_points([100.0, 2.0, 200.0], 3, &mut |n| {
            asked.push(n);
            draws.next().unwrap()
        });
        assert_eq!(asked, [16; 6]);
        assert_eq!(points, [[92.5, 2.0, 207.5], [100.5, 2.0, 195.5], [93.5, 2.0, 194.5]]);
        // Level 5: a 35 unit square.
        let far = carrot_points([0.0; 3], 7, &mut top);
        assert_eq!(far, [[17.5, 0.0, 17.5]; 7]);
    }

    #[test]
    fn the_meteors_and_carrots_fall_burst_shake_and_end() {
        for (id, a7, cards, alpha) in [(METEOR, -1, 1, 100.0), (METEOR_2, -1, 1, 255.0), (CARROTS, 1, 3, 100.0), (CARROTS_2, 5, 7, 255.0)] {
            let mut assets = Fake::default();
            let mut fx = WorldFx::default();
            fx.launch(&Launch { a7: Some(a7), ..Launch::at(id, None, FEET) }, &mut assets);
            let (mut quakes, mut drawn, mut age) = (0, 0, 0);
            while !fx.is_empty() {
                let fired = tick(&mut fx, &[], &mut assets);
                let shakes = fired.iter().filter(|fired| **fired == Fired::Quake).count();
                assert!(shakes == 0 || (48..=51).contains(&age), "{id}: a shake at {age}");
                quakes += shakes;
                let shown = fx.faces(&seen(&all));
                if !shown.is_empty() {
                    drawn += 1;
                    assert_eq!(shown.len(), cards, "{id}");
                    assert!(shown.iter().all(|face| close(face.vertices[0].tint[3], alpha / 255.0) && face.flags == 1), "{id}");
                }
                age += 1;
            }
            // 96 ticks drawn; updated at ages 0 … 97, gone at the tick of age 98; a shake on each of the 4 ticks frame 12
            // shows, for each card.
            assert_eq!((drawn, age), (96, 99), "{id}");
            assert_eq!(quakes, 4 * cards, "{id}");
        }
    }

    #[test]
    fn a_fall_cards_frame_goes_up_every_4_ticks_and_ends_after_23() {
        let mut assets = Fake::default();
        let mut solo = Solo::new(Fall::new(top), Launch::at(METEOR, None, FEET), &mut assets);
        let mut shown = Vec::new();
        for _ in 0..98 {
            solo.tick(&[], &mut assets);
            shown.push(solo.class.cards.first().map(|(card, _)| (card.frame, card.ended)));
        }
        // Frame k at ages 4k … 4k + 3; at age 96 the last frame ends the card, at 97 it goes.
        assert_eq!(
            [shown[3], shown[4], shown[48], shown[95], shown[96], shown[97]],
            [Some((0, false)), Some((1, false)), Some((12, false)), Some((23, false)), Some((23, true)), None]
        );
        assert!(!solo.alive(&[]));
        let solo = Solo::new(Fall::new(top), Launch::at(METEOR, None, FEET), &mut assets);
        let (card, point) = &solo.class.cards[0];
        assert_eq!((card.delay, card.bias, card.end, card.layer_colours, *point), (4.0, 1.5e-4, End::Stop, true, FEET));
        assert_eq!(card.sprite, "data\\sprite\\이팩트\\su_cn_meteor\\su_cn_meteor");
    }

    #[test]
    fn the_shots_head_for_their_target() {
        // Straight in front (dx = 0, dz = −10): 180; to the side (dx = 10, dz = 0): 90.
        assert_eq!(heading([0.0; 3], [0.0, 0.0, -10.0]), 180.0);
        assert_eq!(heading([0.0; 3], [10.0, 0.0, 0.0]), 90.0);
        assert_eq!(heading([0.0; 3], [-10.0, 0.0, 0.0]), 270.0);
        assert_eq!(heading([0.0; 3], [0.0, 0.0, 10.0]), 0.0);
        // Stem Spear: its STR at the caster's place with no owner, then the spears on the caster aimed at the target.
        let (caster, target) = ((Some(7), [1.0, 0.0, 2.0]), (Some(9), [21.0, 0.0, 2.0]));
        let aimed = |id: u32, owner: Option<u32>| Launch { target: Some(Some(9)), aim: [21.0, 0.0, 2.0], ..Launch::at(id, Some(owner), [1.0, 0.0, 2.0]) };
        assert_eq!(
            shot_starts(STEM_SPEAR_SKILL, caster, target),
            [Start::Str(STEM_SPEAR_STR, None, [1.0, 0.0, 2.0]), Start::Object(aimed(STEM_SPEAR, Some(7)))]
        );
        // Spirit of Savage: the spirit, then its STR on the target.
        assert_eq!(
            shot_starts(SAVAGE_SKILL, (None, [1.0, 0.0, 2.0]), target),
            [Start::Object(aimed(SAVAGE, None)), Start::Str(SAVAGE_STR, Some(Some(9)), [21.0, 0.0, 2.0])]
        );
        assert!(shot_starts(5028, caster, target).is_empty());
    }

    #[test]
    fn stem_spears_four_spears_fly_one_every_4_ticks() {
        let aim = [FEET[0], FEET[1], FEET[2] - 32.0];
        let mut assets = Fake::default();
        let launch = Launch { target: Some(Some(TARGET)), aim, ..Launch::at(STEM_SPEAR, Some(Some(OWNER)), FEET) };
        let mut solo = Solo::new(Spear::default(), launch, &mut assets);
        assert_eq!(solo.class.shots.iter().map(|shot| shot.counter).collect::<Vec<_>>(), [0, -4, -8, -12]);
        assert_eq!(solo.fx.length, 27);
        assert_eq!(solo.class.shots[0].card.sprite, "data\\sprite\\이팩트\\sv_stemspear\\sv_stemspear");
        let here = [(OWNER, FEET), (TARGET, aim)];
        let mut drawn: Vec<Vec<u32>> = vec![Vec::new(); 4];
        let mut alphas = Vec::new();
        for age in 0..27 {
            solo.tick(&here, &mut assets);
            for (i, shot) in solo.class.shots.iter().enumerate() {
                if 0 < shot.counter && shot.counter < FLIGHT {
                    drawn[i].push(age);
                }
            }
            alphas.push(solo.class.shots[3].card.alpha);
            if age == 15 {
                // After 16 ticks the first spear has flown the whole way, 10 up, turned along its flight.
                let first = &solo.class.shots[0];
                assert!(close3(first.offset, [0.0, 0.0, -32.0]) && close3(first.at, [aim[0], FEET[1] + 10.0, aim[2]]));
                assert!(close(first.card.spin, 180.0));
            }
        }
        for (i, ages) in drawn.iter().enumerate() {
            let i = i as u32;
            assert_eq!(*ages, (4 * i..=4 * i + 14).collect::<Vec<_>>(), "spear {i}");
        }
        // Spear 3 (from age 12) loses 40 a tick with no floor: the low byte flickers back.
        assert_eq!(alphas[12..21], [215.0, 175.0, 135.0, 95.0, 55.0, 15.0, 231.0, 191.0, 151.0]);
        assert_eq!(solo.faces().len(), 1);
        // Its length runs out after age 26.
        assert!(!solo.fx.timed());
    }

    #[test]
    fn savages_spirit_flies_once_and_follows_the_caster() {
        let aim = [FEET[0] + 20.0, FEET[1], FEET[2]];
        let mut assets = Fake::default();
        let launch = Launch { target: Some(Some(TARGET)), aim, ..Launch::at(SAVAGE, Some(Some(OWNER)), FEET) };
        let mut solo = Solo::new(Spear::default(), launch, &mut assets);
        assert_eq!((solo.class.shots.len(), solo.fx.length), (1, 15));
        assert_eq!(solo.class.shots[0].card.sprite, "data\\sprite\\몬스터\\svg_spirit");
        let moved = [FEET[0], FEET[1], FEET[2] + 5.0];
        let mut alphas = Vec::new();
        for age in 0..15 {
            solo.tick(&[(OWNER, moved), (TARGET, aim)], &mut assets);
            let shot = &solo.class.shots[0];
            assert!(0 < shot.counter && shot.counter < FLIGHT, "{age}");
            alphas.push(shot.card.alpha);
            // It sits on the caster's feet as they are now.
            assert!(close(shot.at[1], moved[1] + 10.0));
        }
        assert_eq!(alphas[9..], [255.0, 215.0, 175.0, 135.0, 95.0, 55.0]);
        // At 90 degrees, with the target to the side.
        assert!(close(solo.class.shots[0].card.spin, 90.0));
        assert!(!solo.fx.timed());
    }

    #[test]
    fn the_powder_and_grass_counts() {
        assert_eq!([1, 2, 3, 4, 5, 6].map(powder_count), [Some(3), Some(3), Some(5), Some(5), Some(7), None]);
        assert_eq!([1, 2, 3, 4, 5, 6].map(grass_count), [Some(5), Some(5), Some(7), Some(7), Some(9), None]);
    }

    #[test]
    fn a_haze_draws_in_order_rises_falls_and_looks_again() {
        let mut asked = Vec::new();
        let mut draws = vec![14, 3, 9, 20, 99, 40, 49].into_iter();
        let mut haze = Haze::new(3, &mut |n| {
            asked.push(n);
            draws.next().unwrap()
        });
        // The offsets (R(15) − 7 + 1), then g, L, max, b, r.
        assert_eq!(asked, [15, 15, 50, 21, 100, 100, 50]);
        assert_eq!(haze.offset, [8.0, -3.0]);
        assert_eq!((haze.length, haze.max, haze.scale, haze.alpha), (50, 149, 0.0, 0));
        assert!(close(haze.growth, (0.1 * 10.0 + 6.0) / 100.0));
        assert!(close(haze.color[0], 66.0 / 255.0) && close(haze.color[1], 1.0) && close(haze.color[2], 110.0 / 255.0));
        let mut alphas = Vec::new();
        while alphas.len() < 2 || haze.alpha != 0 {
            haze.update(true, &mut top);
            alphas.push(haze.alpha);
        }
        // Up by max / L = 2 a tick to 149, then down by 2; below 0 a new look while the unit stands.
        assert_eq!(alphas[..3], [2, 4, 6]);
        let peak = alphas.iter().position(|&alpha| alpha == 149).unwrap();
        assert_eq!((peak, alphas[peak + 1], alphas.len()), (74, 147, 150));
        // The new look (top draws: L 50, max 149), at the same spot, from nothing.
        assert_eq!((haze.length, haze.max, haze.state, haze.scale, haze.offset), (50, 149, 0, 0.0, [8.0, -3.0]));
        // Unit gone: it ends at its next fall below 0.
        (haze.alpha, haze.state) = (1, 1);
        haze.update(false, &mut top);
        assert_eq!((haze.alpha, haze.state), (0, 2));
    }

    #[test]
    fn a_speck_twinkles_sinks_and_moves() {
        let mut asked = Vec::new();
        let mut draws = vec![0, 6, 14, 2].into_iter();
        let mut speck = Speck::new(3, &mut |n| {
            asked.push(n);
            draws.next().unwrap()
        });
        // x, height, z, size.
        assert_eq!(asked, [15, 7, 15, 3]);
        assert_eq!(speck.offset, [-6.0, 13.0, 8.0]);
        assert!(close(speck.card.scale, 0.2));
        assert_eq!((speck.card.flags, speck.card.layer_colours, speck.card.sprite.as_str()), (1, false, "data\\sprite\\이팩트\\particle2"));
        // + R(30), and it sinks R(10) / 100.
        speck.update(3, true, &mut top);
        assert_eq!(speck.alpha, 29);
        assert!(close(speck.offset[1], 13.0 - 0.09));
        for _ in 0..8 {
            speck.update(3, true, &mut top);
        }
        assert_eq!((speck.alpha, speck.state), (254, 1));
        // Below 0 with the unit standing: a new spot.
        speck.alpha = 5;
        speck.update(3, true, &mut top);
        assert_eq!((speck.state, speck.offset[1]), (0, 13.0 - 0.09));
        speck.state = 1;
        speck.update(3, false, &mut top);
        assert_eq!(speck.state, 2);
    }

    #[test]
    fn catnip_powdering_stands_with_its_unit_then_finishes() {
        let mut assets = Fake::default();
        let unit = 0x1234;
        let powder = |level: i32| Launch { a7: Some(level), unique: true, ..Launch::at(POWDER, Some(Some(unit)), FEET) };
        let mut solo = Solo::new(Powder::new(top), powder(3), &mut assets);
        assert_eq!((solo.class.haze.len(), solo.class.specks.len()), (25, 25));
        assert_eq!(assets.textures, [SMOKE]);
        // The ground unit has no feet: the object stands at its cell all the same.
        for _ in 0..300 {
            solo.tick(&[], &mut assets);
            assert!(solo.alive(&[]));
        }
        assert_eq!(solo.fx.position, FEET);
        let lists = Lists::sort(solo.faces(), |_| 0.0);
        assert_eq!((lists.effect.len(), lists.translucent.len()), (25, 25));
        assert!(lists.effect.iter().all(|face| face.additive && matches!(face.texture, Tex::Path(SMOKE))));
        // Gone: no new pieces; it goes once all of them finish.
        solo.fx.released = true;
        let mut ticks = 0;
        while solo.alive(&[]) {
            solo.tick(&[], &mut assets);
            ticks += 1;
            assert!(ticks < 200);
        }
        assert!(ticks > 1 && solo.faces().is_empty());
        // Released far away, where nothing steps: it goes at once.
        let mut far = Solo::new(Powder::new(top), powder(3), &mut assets);
        (far.fx.released, far.fx.far) = (true, true);
        assert!(!far.alive(&[]));
        // Level 6 makes nothing.
        let solo = Solo::new(Powder::new(top), powder(6), &mut assets);
        assert!(!solo.alive(&[]));
        // The registry: seen again, the released copy is taken back.
        let mut fx = WorldFx::default();
        let serial = fx.launch(&powder(3), &mut assets);
        fx.release(Some(Some(unit)));
        assert_eq!(fx.launch(&powder(3), &mut assets), serial);
        assert!(!fx.iter().next().unwrap().fx.released);
    }

    #[test]
    fn root_twist_loops_under_the_feet_and_draws_with_effect_off() {
        let mut assets = Fake::default();
        let mut solo = Solo::new(Roots::default(), Launch::at(ROOTS, Some(Some(OWNER)), FEET), &mut assets);
        let mut frames = Vec::new();
        for _ in 0..200 {
            solo.tick(&HERE, &mut assets);
            solo.class.frame(&solo.fx, &mut assets);
            frames.push(solo.class.card.as_ref().unwrap().frame);
        }
        // 8 ticks a frame, 22 frames: a 176-tick loop.
        assert_eq!((frames[6], frames[7], frames[174], frames[175]), (0, 1, 21, 0));
        let card = solo.class.card.as_ref().unwrap();
        assert_eq!((card.flags, card.delay, card.bias, card.end), (0x101, 8.0, 1.5e-4, End::Loop));
        // 3 units below the feet (the face is pulled toward the camera by its bias).
        let at = centre(&solo.faces()[0]);
        assert!(close(at[0], FEET[0]) && at[1] < FEET[1] - 2.5);
        assert!(solo.alive(&HERE) && !solo.alive(&[]));
        // Force-drawn: /effect off keeps it, and only it.
        let effect_off = crate::effects::force_drawn;
        let mut fx = WorldFx::default();
        fx.launch(&Launch::at(ROOTS, Some(Some(OWNER)), FEET), &mut assets);
        tick(&mut fx, &HERE, &mut assets);
        fx.frame(&seen(&all), &mut assets);
        assert_eq!(fx.faces(&seen(&effect_off)).len(), 1);
        for id in [MABLE, TUNA, CARROTS, STEM_SPEAR, POWDER, METEOR, CARROTS_2, METEOR_2, GRASS, SAVAGE] {
            assert!(!effect_off(id), "{id}");
        }
    }

    #[test]
    fn nyang_grass_fills_its_field_until_the_unit_goes() {
        let mut assets = Fake::default();
        let grass = |level: i32| Launch { a7: Some(level), ..Launch::at(GRASS, Some(Some(0x1235)), FEET) };
        let mut solo = Solo::new(Grass::default(), grass(1), &mut assets);
        assert_eq!(assets.strs.len(), 25);
        assert!(assets.strs.iter().all(|file| file == GRASS_STR));
        let parts = &solo.class.parts;
        // Row r, column c at 5 (c − 2), 5 (r − 2).
        assert_eq!(parts[0].point, [FEET[0] - 10.0, FEET[1], FEET[2] - 10.0]);
        assert_eq!(parts[6].point, [FEET[0] - 5.0, FEET[1], FEET[2] - 5.0]);
        assert_eq!(parts[7].point, [FEET[0], FEET[1], FEET[2] - 5.0]);
        assert_eq!(parts[24].point, [FEET[0] + 10.0, FEET[1], FEET[2] + 10.0]);
        assert!(parts.iter().all(|part| part.timeline.looped && part.offset == [0.0, 0.0] && part.flags == 0x21 && !part.world));
        for _ in 0..500 {
            solo.tick(&[], &mut assets);
        }
        assert_eq!(Lists::sort(solo.faces(), |_| 0.0).translucent.len(), 25);
        assert!(solo.alive(&[]));
        solo.fx.released = true;
        assert!(!solo.alive(&[]));
        // Level 3: 49 tufts; level 6: none.
        let mut assets = Fake::default();
        Solo::new(Grass::default(), grass(3), &mut assets);
        assert_eq!(assets.strs.len(), 49);
        assert!(!Solo::new(Grass::default(), grass(6), &mut assets).alive(&[]));
    }

    #[test]
    fn the_rows_start_the_objects() {
        let table = include_str!("../effects.txt");
        let has = |row: &str| table.lines().any(|line| line == row);
        for row in [
            "status\t937\t1096",
            "status\t919\t1097",
            "status\t896\t1110",
            "ground\t5028\tground\t1106",
            "ground\t5042\tground\t1112",
            "caster\t5036\t-1",
            "caster\t5043\t-1",
            "unit\t262\t1104\tonce",
            "unit\t263\t1125\tonce",
            "wav\t1104\teffect\\su_cn_powdering\t0",
            "wav\t1110\teffect\\su_roottwist\t0",
            "wav\t1125\teffect\\su_nyanggrass\t0",
        ] {
            assert!(has(row), "{row}");
        }
        let fx = WorldFx::default();
        for id in [MABLE, TUNA, CARROTS, STEM_SPEAR, POWDER, METEOR, ROOTS, CARROTS_2, METEOR_2, GRASS, SAVAGE] {
            assert!(fx.registered(id), "{id}");
        }
        assert!(!fx.registered(STEM_SPEAR_STR) && !fx.registered(SAVAGE_STR));
    }
}
