//! The Soul Linker's Esma (553, 554) and Estun (555): two wind discs curling round to the target, the frost picture swelling on the
//! target, and the invisible mover that launches the sparks (556); and the two spark movers (551, 1204) that leave a trail of open
//! cone rings (552, 1205), a 16 ms tick at a time.
//!
//! A shape's own age n counts from 1 on its first tick; e is the ticks since the effect started.

use std::time::Instant;

use super::passes;
use super::sage::{ring_point, ring_quads};
use super::soul::SquareDraw;
use super::tanji::bearing;
use super::walls::Wall;
use crate::world::{MeshVertex, Vector};

pub use super::soul::BLADE;
pub const FROST: &str = "data\\texture\\effect\\freeze_a.bmp";
const DISC_LIFE: u32 = 200;
const FROST_LIFE: u32 = 200;
const MOVER_LIFE: u32 = 200;
/// The spark movers (551, 1204) last 100 ticks, their cones (552, 1205) 100 and the sparks (556) 50.
const SPARK_MOVER_LIFE: u32 = 100;
const CONE_LIFE: u32 = 100;
const SPARKS_LIFE: u32 = 50;
/// The cones' ring picture, and the sparks' two pictures (the second at the twinkle).
pub use super::soul::RING_BLUE;
pub const THUNDER: [&str; 2] = ["data\\texture\\effect\\thunder_ball_0002.bmp", "data\\texture\\effect\\thunder_ball_0003.bmp"];
/// The cones' colours: blue (#5A5AFF) added on, and dark grey (#464646) blended.
const CONE_BLUE: [f32; 3] = [90.0 / 255.0, 90.0 / 255.0, 1.0];
const CONE_DARK: [f32; 3] = [70.0 / 255.0; 3];
/// The sparks' colour (#9696FA).
const SPARK_TINT: [f32; 3] = [150.0 / 255.0, 150.0 / 255.0, 250.0 / 255.0];
/// Where the mover's last launch is (its tick e = 198).
const MOVER_LAST: u32 = 198;
/// The blade's colour #5050FF and the frost's #8282FA.
const BLADE_TINT: [f32; 3] = [80.0 / 255.0, 80.0 / 255.0, 1.0];
/// Eswhoo's dark grey discs (#464646), blended, not added on.
const DARK_TINT: [f32; 3] = [70.0 / 255.0; 3];
const FROST_TINT: [f32; 3] = [130.0 / 255.0, 130.0 / 255.0, 250.0 / 255.0];

/// What a disc is doing: homing on the target, fading out or curling back home.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Homing,
    Fading,
    Home,
}

/// One element of a disc: its place, spin and alpha, and whether it faded out for good.
#[derive(Clone, Copy, Debug)]
pub struct Element {
    pub at: Vector,
    pub spin: f32,
    pub alpha: i32,
    dead: bool,
}

pub struct Disc {
    start: Instant,
    target: Option<u32>,
    /// 0 turns the heading down, 1 up.
    sense: i32,
    done: u32,
    age: i32,
    home: Vector,
    /// Eswhoo's: dark grey, blended.
    dark: bool,
    pub heading: i32,
    pub state: State,
    pub elements: [Element; 4],
}

struct Frost {
    unit: Option<u32>,
    start: Instant,
    done: u32,
    alpha: i32,
    radius: f32,
    wobble: f32,
}

/// What a mover launches: Estun's sparks (mode 1) every tick, or a cone ring every 8th tick of its age, blue (mode 0, effect 551) or
/// dark (mode 99, effect 1204).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Sparks,
    Blue,
    Dark,
}

struct Mover {
    start: Instant,
    done: u32,
    age: i32,
    from: Vector,
    aim: Vector,
    heading: f32,
    at: Vector,
    mode: Mode,
}

/// One of a cone's three open bands: the rim's height, radius, lean and turn, its age and alpha, and the rim's hump.
struct ConeBand {
    k: usize,
    height: f32,
    radius: f32,
    lean: f32,
    turn: f32,
    age: u32,
    alpha: i32,
    profile: [f32; 21],
}

/// Three open cones at a fixed point (kind 121: 552, 1205), given the age s.x and the odd/even s.y of the mover that launched them.
struct Cone {
    start: Instant,
    done: u32,
    at: Vector,
    s: [f32; 2],
    dark: bool,
    bands: [ConeBand; 3],
}

/// Four electric sparks round a point (kind 59: 556): the size, place and rise of each.
struct SparkDot {
    half: f32,
    at: Vector,
    rise: f32,
}

struct Sparks {
    start: Instant,
    dots: [SparkDot; 4],
}

#[derive(Default)]
pub struct Linker {
    discs: Vec<Disc>,
    frosts: Vec<Frost>,
    movers: Vec<Mover>,
    cones: Vec<Cone>,
    sparks: Vec<Sparks>,
}

/// The frost's alpha at its age n: 0 to 40, 8 more a tick to 168 at 61, then 8 less (0 at 82).
pub fn frost_alpha(n: u32) -> i32 {
    match n {
        0..=40 => 0,
        41..=61 => 8 * (n as i32 - 40),
        _ => (168 - 8 * (n as i32 - 61)).max(0),
    }
}

/// The frost's radius at its age n: 10, growing 0.7 a tick while it shows (up to n = 81).
pub fn frost_radius(n: u32) -> f32 {
    10.0 + 0.7 * n.clamp(40, 81).saturating_sub(40) as f32
}

fn wrap(degrees: i32) -> i32 {
    degrees.rem_euclid(360)
}

impl Disc {
    fn new(from: Vector, target: Option<u32>, sense: i32, dark: bool, now: Instant) -> Self {
        let at = [from[0], from[1] + 10.0, from[2]];
        let elements = std::array::from_fn(|k| Element { at, spin: k as f32, alpha: 0, dead: false });
        Self { start: now, target, sense, done: 0, age: 0, home: from, dark, heading: 0, state: State::Homing, elements }
    }

    fn turn(&self) -> i32 {
        if self.sense == 0 { -1 } else { 1 }
    }

    /// Turns the heading 5 degrees its own way unless it is within 3 of `want`.
    fn steer(&mut self, want: i32) {
        if self.heading < want - 3 || self.heading > want + 3 {
            self.heading = wrap(self.heading + 5 * self.turn());
        }
    }

    /// One tick; `target` is where the target stands now, None when it is gone.
    fn step(&mut self, target: Option<Vector>) {
        self.age += 1;
        let n = self.age;
        if target.is_none() && self.state != State::Fading {
            self.state = State::Home;
            if n == 1 {
                return;
            }
        }
        let blade = self.elements[0].at;
        self.elements[0].spin = (self.elements[0].spin + 30.0) % 360.0;
        if n == 1 {
            if let Some(target) = target {
                self.heading = bearing(target[0] - self.home[0], target[2] - self.home[2]);
                self.heading = wrap(self.heading + if self.sense == 0 { 90 } else { -90 });
            }
        } else if n <= 20 {
            self.elements[0].alpha = (self.elements[0].alpha + 15).min(250);
        } else if n > 140 && self.state == State::Homing {
            self.state = State::Fading;
        }
        if n > 5 {
            match (self.state, target) {
                (State::Homing, Some(target)) => self.steer(bearing(target[0] - blade[0], target[2] - blade[2])),
                (State::Fading, _) => {
                    for element in &mut self.elements {
                        element.alpha -= 5;
                        if element.alpha < 0 {
                            element.alpha = 0;
                            element.dead = true;
                        }
                    }
                    self.heading = wrap(self.heading + self.turn());
                }
                (State::Home, _) => {
                    self.steer(bearing(self.home[0] - blade[0], self.home[2] - blade[2]));
                    if n > 50 {
                        self.state = State::Fading;
                    }
                }
                _ => {}
            }
        }
        let (sin, cos) = (self.heading as f32).to_radians().sin_cos();
        self.elements[0].at[0] += 2.0 * cos;
        self.elements[0].at[2] += 2.0 * sin;
        if n % 3 == 0 {
            for k in (1..4).rev() {
                let before = self.elements[k - 1];
                if self.elements[k].dead {
                    continue;
                }
                self.elements[k] = Element { alpha: (0.8 * before.alpha as f32).floor() as i32, dead: false, ..before };
            }
        }
    }
}

impl Frost {
    fn step(&mut self, n: u32) {
        self.alpha = frost_alpha(n);
        if self.alpha > 0 {
            self.radius = frost_radius(n);
        }
        self.wobble = (2 * n % 360) as f32;
    }
}

/// Which of the sparks' two pictures square k shows at its age n: the second at n = 35 − 7k and 36 − 7k, else the first.
pub fn spark_picture(k: usize, n: u32) -> usize {
    usize::from(n as i32 == 36 - 7 * k as i32 || n as i32 == 35 - 7 * k as i32)
}

/// A cone band's alpha at its age n (s.x the launcher's age): 5 a tick to 180 by n = 39, held, 5 less a tick from n = 60, cut to
/// 200 − 5 (s.x − 50) when the launcher is older than 50.
pub fn cone_alpha(alpha: i32, n: u32, launched_at: f32) -> i32 {
    let mut alpha = if n >= 60 {
        (alpha - 5).max(0)
    } else if n < 40 {
        (alpha + 5).min(180)
    } else {
        alpha
    };
    if launched_at > 50.0 {
        alpha = alpha.min((200.0 - (launched_at - 50.0) * 5.0).trunc() as i32).max(0);
    }
    alpha
}

impl ConeBand {
    fn new(k: usize, odd: f32) -> Self {
        Self {
            k,
            height: [13.0, 11.0, 9.0][k],
            radius: [3.4, 3.6, 3.8][k] - odd,
            lean: [55.0, 50.0, 45.0][k],
            turn: 90.0 * k as f32,
            age: 0,
            alpha: 0,
            profile: [0.0; 21],
        }
    }

    /// One tick: it turns 3 + k degrees, brightens and fades, and its rim rises to h sin 9t° by n = 90.
    fn step(&mut self, launched_at: f32) {
        self.age += 1;
        let n = self.age;
        self.turn = (self.turn + 3.0 + self.k as f32) % 360.0;
        self.alpha = cone_alpha(self.alpha, n, launched_at);
        if n <= 90 {
            for (t, rim) in self.profile.iter_mut().enumerate() {
                *rim = self.height * (9.0 * t as f32).to_radians().sin() * (n as f32).to_radians().sin();
            }
        }
    }
}

impl Cone {
    fn new(dark: bool, at: Vector, s: [f32; 2], start: Instant) -> Self {
        Self { start, done: 0, at, s, dark, bands: std::array::from_fn(|k| ConeBand::new(k, s[1])) }
    }
}

impl Mover {
    fn step(&mut self) {
        self.age += 1;
        if self.age <= 0 {
            return;
        }
        if self.age == 1 {
            self.heading = bearing(self.aim[0] - self.from[0], self.aim[2] - self.from[2]) as f32;
        }
        let (sin, cos) = self.heading.to_radians().sin_cos();
        self.at[0] += 0.8 * cos;
        self.at[2] += 0.8 * sin;
    }
}

impl Linker {
    /// Esma's two discs from the caster's feet `from`, aimed at `target` (None = the player); the left one first.
    pub fn discs(&mut self, from: Vector, target: Option<u32>, now: Instant) {
        self.discs.push(Disc::new(from, target, 0, false, now));
        self.discs.push(Disc::new(from, target, 1, false, now));
    }

    /// Eswhoo's two dark grey discs: Esma's, in #464646 with the normal blend.
    pub fn dark_discs(&mut self, from: Vector, target: Option<u32>, now: Instant) {
        self.discs.push(Disc::new(from, target, 0, true, now));
        self.discs.push(Disc::new(from, target, 1, true, now));
    }

    /// The frost picture on `unit`.
    pub fn frost(&mut self, unit: Option<u32>, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        // The one draw the game makes for the square's angle is thrown away.
        let _ = rand(360);
        self.frosts.push(Frost { unit, start: now, done: 0, alpha: 0, radius: 10.0, wobble: 0.0 });
    }

    /// Estun's invisible mover from the caster's feet toward `aim`; the four heights from `rand(n)`.
    pub fn mover(&mut self, from: Vector, aim: Vector, now: Instant, rand: impl FnMut(u32) -> u32) {
        self.spawn(Mode::Sparks, from, aim, now, rand);
    }

    /// A spark mover (551 blue, 1204 dark) from the unit's feet toward the world's origin, the game's aim when no launcher gives one.
    pub fn spark_mover(&mut self, dark: bool, from: Vector, now: Instant, rand: impl FnMut(u32) -> u32) {
        self.spawn(if dark { Mode::Dark } else { Mode::Blue }, from, [0.0; 3], now, rand);
    }

    fn spawn(&mut self, mode: Mode, from: Vector, aim: Vector, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        // ponytail: the other three elements move but nothing reads them, so only their four draws are made.
        let heights = [(); 4].map(|()| 10.0 - rand(5) as f32);
        let at = [from[0], from[1] + heights[0], from[2]];
        self.movers.push(Mover { start: now, done: 0, age: -3, from, aim, heading: 0.0, at, mode });
    }

    /// Runs the discs, the frost, the movers and what they launched up to `now`, a tick at a time, and drops what ended; the sparks'
    /// choices come from `rand(n)`.
    pub fn advance(&mut self, now: Instant, position_of: impl Fn(Option<u32>) -> Option<Vector>, mut rand: impl FnMut(u32) -> u32) {
        for disc in &mut self.discs {
            while disc.done < passes(disc.start, now).min(DISC_LIFE) {
                disc.done += 1;
                disc.step(position_of(disc.target));
            }
        }
        self.discs.retain(|disc| passes(disc.start, now) <= DISC_LIFE);
        for frost in &mut self.frosts {
            while frost.done < passes(frost.start, now).min(FROST_LIFE) {
                frost.done += 1;
                frost.step(frost.done);
            }
        }
        self.frosts.retain(|frost| passes(frost.start, now) <= FROST_LIFE);
        for mover in &mut self.movers {
            let life = if mover.mode == Mode::Sparks { MOVER_LIFE } else { SPARK_MOVER_LIFE };
            while mover.done < passes(mover.start, now).min(life) {
                let e = mover.done;
                mover.done += 1;
                mover.step();
                let launched = mover.start + super::TICK * e;
                match mover.mode {
                    // Estun's launches the sparks every tick e = 0 to 198.
                    Mode::Sparks if e <= MOVER_LAST => {
                        let dots = std::array::from_fn(|_| {
                            let half = 1.2 + 0.08 * rand(10) as f32;
                            let (x, z) = (mover.at[0] + rand(9) as f32 - 4.0, mover.at[2] + rand(9) as f32 - 4.0);
                            let rise = 0.10 + 0.01 * rand(20) as f32;
                            SparkDot { half, at: [x, mover.at[1] + 6.0, z], rise }
                        });
                        self.sparks.push(Sparks { start: launched, dots });
                    }
                    // The others launch a cone ring on each tick whose age is a multiple of 8: 13 of them.
                    Mode::Blue | Mode::Dark if mover.age >= 0 && mover.age % 8 == 0 => {
                        let n = mover.age as f32;
                        let s = [n, if mover.age % 16 == 0 { 0.0 } else { 1.0 }];
                        self.cones.push(Cone::new(mover.mode == Mode::Dark, mover.at, s, launched));
                    }
                    _ => {}
                }
            }
        }
        self.movers.retain(|mover| passes(mover.start, now) <= if mover.mode == Mode::Sparks { MOVER_LIFE } else { SPARK_MOVER_LIFE });
        for cone in &mut self.cones {
            while cone.done < passes(cone.start, now).min(CONE_LIFE) {
                cone.done += 1;
                for band in &mut cone.bands {
                    band.step(cone.s[0]);
                }
            }
        }
        self.cones.retain(|cone| passes(cone.start, now) <= CONE_LIFE);
        self.sparks.retain(|sparks| passes(sparks.start, now) <= SPARKS_LIFE);
    }

    /// The cone rings: open ribbons round their points, blue ones added on and dark ones blended.
    pub fn cone_meshes(&self) -> Vec<Wall> {
        let mut walls = Vec::new();
        for dark in [false, true] {
            let mut vertices = Vec::new();
            let rgb = if dark { CONE_DARK } else { CONE_BLUE };
            for cone in self.cones.iter().filter(|cone| cone.dark == dark) {
                for band in cone.bands.iter().filter(|band| band.alpha > 0) {
                    // 20 quads over 300 degrees: 21 points 15 apart, the rim at the band's hump.
                    let points: Vec<_> =
                        (0..=20).map(|t| ring_point(cone.at, (band.turn + 15.0 * t as f32) % 360.0, band.radius, band.profile[t], band.lean)).collect();
                    for (position, uv, alpha) in ring_quads(&points, band.alpha as f32 / 255.0) {
                        vertices.push(MeshVertex { position, uv, tint: [rgb[0], rgb[1], rgb[2], alpha] });
                    }
                }
            }
            if !vertices.is_empty() {
                walls.push(Wall { texture: RING_BLUE, vertices, additive: !dark, forced: false });
            }
        }
        walls
    }

    /// The electric sparks as camera-facing squares at `now`, added on.
    // ponytail: drawn at half the alpha, as the other kind-59 squares are.
    pub fn quads<'a>(&self, now: Instant, texture_of: impl Fn(&str) -> Option<&'a crate::render::Texture>) -> Vec<crate::world::EffectQuad<'a>> {
        let mut quads = Vec::new();
        for sparks in &self.sparks {
            let n = passes(sparks.start, now);
            let alpha = super::sparkle::alpha_at(n);
            if n == 0 || alpha == 0 {
                continue;
            }
            for (k, dot) in sparks.dots.iter().enumerate() {
                let Some(texture) = texture_of(THUNDER[spark_picture(k, n)]) else { continue };
                let place = [dot.at[0], dot.at[1] + n as f32 * dot.rise, dot.at[2]];
                let corners = super::sparkle::corners(dot.half, super::sparkle::spin_at(n));
                quads.push((texture, place, corners, [SPARK_TINT[0], SPARK_TINT[1], SPARK_TINT[2], alpha as f32 / 2.0 / 255.0], true, false));
            }
        }
        quads
    }

    /// The discs and their trails: flat squares, Esma's blue and added on, Eswhoo's dark grey and blended.
    pub fn meshes(&self) -> Vec<Wall> {
        let mut walls = Vec::new();
        for dark in [false, true] {
            let mut vertices = Vec::new();
            let rgb = if dark { DARK_TINT } else { BLADE_TINT };
            for disc in self.discs.iter().filter(|disc| disc.dark == dark) {
                for element in disc.elements.iter().filter(|element| element.alpha > 0) {
                    let tint = [rgb[0], rgb[1], rgb[2], element.alpha as f32 / 255.0];
                    let corner = |angle: f32, uv: [f32; 2]| {
                        let (sin, cos) = (element.spin + angle).to_radians().sin_cos();
                        MeshVertex { position: [element.at[0] + 25.0 * cos, element.at[1], element.at[2] + 25.0 * sin], uv, tint }
                    };
                    let [p0, p1, p2, p3] = [corner(90.0, [0.0, 1.0]), corner(180.0, [1.0, 1.0]), corner(270.0, [1.0, 0.0]), corner(360.0, [0.0, 0.0])];
                    vertices.extend([p0, p1, p2, p0, p2, p3]);
                }
            }
            if !vertices.is_empty() {
                walls.push(Wall { texture: BLADE, vertices, additive: !dark, forced: false });
            }
        }
        walls
    }

    /// The frost pictures over their units' feet, facing the camera.
    pub fn squares(&self, position_of: impl Fn(Option<u32>) -> Option<Vector>) -> Vec<SquareDraw> {
        self.frosts
            .iter()
            .filter(|frost| frost.alpha > 0)
            .filter_map(|frost| {
                let feet = position_of(frost.unit)?;
                let half = frost.radius + frost.radius * frost.wobble.to_radians().sin() * 0.05;
                Some(SquareDraw { texture: FROST, at: [feet[0], feet[1] + 10.0, feet[2]], half, rgb: FROST_TINT, alpha: frost.alpha as f32, additive: true })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prims::TICK;

    fn near(got: f32, want: f32) -> bool {
        (got - want).abs() < 1e-3
    }

    const FEET: Vector = [100.0, 3.0, 100.0];

    /// Runs one disc `ticks` ticks with the target fixed at `target`.
    fn run(sense: i32, target: Option<Vector>, ticks: u32) -> Disc {
        let mut disc = Disc::new(FEET, Some(1), sense, false, Instant::now());
        for _ in 0..ticks {
            disc.step(target);
        }
        disc
    }

    #[test]
    fn two_discs_start_at_the_casters_feet_ten_up_and_launch_sideways() {
        let start = Instant::now();
        let mut linker = Linker::default();
        linker.discs(FEET, Some(1), start);
        assert_eq!(linker.discs.len(), 2);
        assert_eq!([linker.discs[0].sense, linker.discs[1].sense], [0, 1]);
        assert!(linker.discs.iter().all(|disc| disc.elements.iter().all(|element| element.at == [100.0, 13.0, 100.0] && element.alpha == 0)));
        assert_eq!(linker.discs[0].elements.map(|element| element.spin), [0.0, 1.0, 2.0, 3.0]);
        // At n = 1 the heading is the bearing to a target due +x (0) turned 90 one way or the other; it flies 2 units that way.
        let target = Some([300.0, 3.0, 100.0]);
        let (left, right) = (run(0, target, 1), run(1, target, 1));
        assert_eq!((left.heading, right.heading), (90, 270));
        assert!(left.elements[0].at == [100.0, 13.0, 102.0] && right.elements[0].at == [100.0, 13.0, 98.0] && left.elements[0].alpha == 0);
        // Then 15 more alpha a tick to 250 at n = 18; the disc spins 30 a tick.
        let alpha = |n: u32| run(0, target, n).elements[0].alpha;
        assert_eq!([2, 17, 18, 21].map(alpha), [15, 240, 250, 250]);
        assert_eq!(run(0, target, 13).elements[0].spin, (13.0 * 30.0) % 360.0);
        // Four ticks straight on at 2 a tick along the 90 degree heading: 8 units.
        let moved = run(0, target, 4);
        assert!(near(moved.elements[0].at[2] - 100.0, 8.0) && near(moved.elements[0].at[0], 100.0));
    }

    #[test]
    fn a_disc_steers_five_degrees_its_own_way_outside_three_of_the_bearing() {
        // Past n = 5 and 90 degrees from a target due +x: the left disc turns down 5 a tick, the right one up.
        let target = Some([1000.0, 3.0, 100.0]);
        assert_eq!(run(0, target, 6).heading, 85);
        assert_eq!(run(1, target, 6).heading, 275);
        // Within 3 of the bearing the heading stays.
        let mut ahead = run(0, target, 1);
        ahead.heading = 90;
        ahead.steer(93);
        assert_eq!(ahead.heading, 90);
        ahead.steer(94);
        assert_eq!(ahead.heading, 85);
        // The heading is a plain whole number: a left disc at 0 goes to 355.
        let mut disc = run(0, Some([1000.0, 3.0, 100.0]), 1);
        disc.heading = 0;
        disc.steer(180);
        assert_eq!(disc.heading, 355);
    }

    #[test]
    fn the_trail_copies_take_four_fifths_of_the_alpha_every_third_tick() {
        let target = Some([300.0, 3.0, 100.0]);
        let alphas = |n: u32| run(0, target, n).elements.map(|element| element.alpha);
        assert_eq!(alphas(3), [30, 24, 0, 0]);
        assert_eq!(alphas(6), [75, 60, 19, 0]);
    }

    #[test]
    fn a_disc_fades_from_141_and_a_gone_target_sends_it_home() {
        let target = Some([300.0, 3.0, 100.0]);
        let alphas = |n: u32| run(0, target, n).elements[0].alpha;
        assert_eq!([140, 141, 142, 189, 190].map(alphas), [250, 245, 240, 5, 0]);
        // The four elements drop together: at n = 141 the copies are 5 less as well.
        let disc = run(0, target, 141);
        assert_eq!(disc.state, State::Fading);
        assert!(disc.elements.iter().skip(1).all(|element| element.alpha <= 245));
        // Gone at once: state 20 (home) and nothing at all on the first tick.
        let gone = run(0, None, 1);
        assert_eq!(gone.state, State::Home);
        assert!(gone.elements[0].spin == 0.0 && gone.elements[0].alpha == 0);
        // It curls back and fades from n = 51.
        assert_eq!(run(0, None, 50).state, State::Home);
        assert_eq!(run(0, None, 51).state, State::Fading);
    }

    #[test]
    fn the_discs_draw_flat_blue_squares_and_end_with_the_effect() {
        let start = Instant::now();
        let mut linker = Linker::default();
        linker.discs(FEET, Some(1), start);
        let at = |tick: u32| start + TICK * tick;
        linker.advance(at(0), |_| Some([300.0, 3.0, 100.0]), |_| 0);
        assert!(linker.meshes().is_empty());
        linker.advance(at(30), |_| Some([300.0, 3.0, 100.0]), |_| 0);
        let walls = linker.meshes();
        assert_eq!(walls.len(), 1);
        let wall = &walls[0];
        assert!(wall.additive && wall.texture == BLADE && !wall.forced);
        assert_eq!(wall.vertices.len() % 12, 0);
        assert!(wall.vertices.iter().all(|vertex| vertex.tint[..3] == BLADE_TINT));
        // Corner distance 25 from the element's place.
        let first = wall.vertices[0].position;
        assert!(near(f32::hypot(first[0] - linker.discs[0].elements[0].at[0], first[2] - linker.discs[0].elements[0].at[2]), 25.0));
        linker.advance(at(201), |_| None, |_| 0);
        assert!(linker.discs.is_empty());
    }

    #[test]
    fn eswhoos_discs_are_dark_grey_and_blended() {
        let start = Instant::now();
        let mut linker = Linker::default();
        linker.dark_discs(FEET, Some(1), start);
        linker.discs(FEET, Some(1), start);
        linker.advance(start + TICK * 30, |_| Some([300.0, 3.0, 100.0]), |_| 0);
        // The same numbers as Esma's, two walls: the blue ones added on, the dark ones in #464646 with the normal blend.
        assert_eq!(linker.discs.len(), 4);
        let walls = linker.meshes();
        assert_eq!(walls.len(), 2);
        let (blue, dark) = (&walls[0], &walls[1]);
        assert!(blue.additive && !dark.additive && dark.texture == BLADE && !dark.forced);
        assert!(dark.vertices.iter().all(|vertex| vertex.tint[..3] == [70.0 / 255.0; 3]));
        assert_eq!(dark.vertices.len(), blue.vertices.len());
        assert_eq!(linker.discs[0].elements.map(|element| element.alpha), linker.discs[2].elements.map(|element| element.alpha));
    }

    #[test]
    fn the_frost_swells_from_the_41st_tick() {
        assert_eq!([40, 41, 61, 62, 82].map(frost_alpha), [0, 8, 168, 160, 0]);
        assert!(near(frost_radius(40), 10.0) && near(frost_radius(41), 10.7));
        assert!(near(frost_radius(81), 38.7) && near(frost_radius(82), 38.7));
        let start = Instant::now();
        let mut linker = Linker::default();
        let mut draws = Vec::new();
        linker.frost(Some(7), start, |n| {
            draws.push(n);
            0
        });
        assert_eq!(draws, [360]);
        let feet = |unit: Option<u32>| (unit == Some(7)).then_some([50.0, 2.0, 60.0]);
        linker.advance(start + TICK * 39, feet, |_| 0);
        assert!(linker.squares(feet).is_empty());
        // The 41st tick (n = 41): alpha 8, 10.7 across, the wobble 2n.
        linker.advance(start + TICK * 40, feet, |_| 0);
        let draw = linker.squares(feet)[0];
        assert_eq!((draw.texture, draw.alpha, draw.additive), (FROST, 8.0, true));
        assert_eq!(draw.at, [50.0, 12.0, 60.0]);
        assert!(near(draw.half, 10.7 + 10.7 * 82.0_f32.to_radians().sin() * 0.05));
        assert_eq!(draw.rgb, FROST_TINT);
        linker.advance(start + TICK * 81, feet, |_| 0);
        assert!(linker.squares(feet).is_empty() && linker.frosts[0].wobble == 164.0);
        // It ends with the effect.
        linker.advance(start + TICK * 201, feet, |_| 0);
        assert!(linker.frosts.is_empty());
    }

    #[test]
    fn the_mover_flies_at_eight_tenths_and_launches_556_every_tick() {
        let start = Instant::now();
        let mut linker = Linker::default();
        let mut draws = [0, 4, 0, 0].into_iter();
        linker.mover(FEET, [300.0, 3.0, 100.0], start, |n| {
            assert_eq!(n, 5);
            draws.next().unwrap()
        });
        assert_eq!(linker.movers[0].at, [100.0, 13.0, 100.0]);
        let mut other = Linker::default();
        let mut draws = [4, 0, 0, 0].into_iter();
        other.mover(FEET, [300.0, 3.0, 100.0], start, |_| draws.next().unwrap());
        assert_eq!(other.movers[0].at[1], 9.0);
        // Nothing moves until n = 1 (the fourth tick, e = 3): then 0.8 a tick toward the aim. A spark set is launched each tick at the
        // mover's place (6 up from it), the draws in the order size, x, z, rise.
        let mut asked = Vec::new();
        linker.advance(
            start + TICK * 2,
            |_| None,
            |n| {
                asked.push(n);
                0
            },
        );
        assert_eq!(linker.sparks.len(), 3);
        assert_eq!(asked[..4], [10, 9, 9, 20]);
        assert!(linker.sparks.iter().all(|sparks| sparks.dots[0].at == [96.0, 19.0, 96.0]));
        linker.advance(start + TICK * 3, |_| None, |_| 0);
        assert_eq!(linker.sparks.len(), 4);
        assert!(near(linker.sparks[3].dots[0].at[0], 100.8 - 4.0) && near(linker.sparks[3].dots[0].at[2], 100.0 - 4.0));
        // Ten more ticks: 8.8 units out, still at the height of the first element.
        linker.advance(start + TICK * 13, |_| None, |_| 0);
        assert_eq!(linker.sparks.len(), 14);
        assert!(near(linker.sparks[13].dots[0].at[0], 108.8 - 4.0) && linker.sparks[13].dots[0].at[1] == 19.0);
        // One launch each tick e = 0 to 198 and none after (a set lives 50 ticks).
        let mut counted = Linker::default();
        counted.mover(FEET, [300.0, 3.0, 100.0], start, |_| 0);
        let launches: Vec<usize> = (0..250)
            .map(|tick| {
                counted.advance(start + TICK * tick, |_| None, |_| 0);
                counted.sparks.iter().filter(|sparks| sparks.start == start + TICK * tick).count()
            })
            .collect();
        assert!(launches[..199].iter().all(|&count| count == 1) && launches[199..].iter().all(|&count| count == 0));
        assert!(counted.movers.is_empty());
        linker.advance(start + TICK * 400, |_| None, |_| 0);
        assert!(linker.movers.is_empty() && linker.sparks.is_empty());
        assert!(linker.meshes().is_empty() && linker.squares(|_| None).is_empty());
    }

    #[test]
    fn a_spark_mover_launches_13_cone_rings_6_4_units_apart() {
        let start = Instant::now();
        for (dark, mode) in [(false, Mode::Blue), (true, Mode::Dark)] {
            let mut linker = Linker::default();
            let mut draws = [0, 4, 0, 0].into_iter();
            linker.spark_mover(dark, FEET, start, |n| {
                assert_eq!(n, 5);
                draws.next().unwrap()
            });
            // One mover at the unit's feet, 10 and 6 up for the draws 0 and 4, speed 0.8, heading for the origin.
            assert_eq!((linker.movers.len(), linker.movers[0].mode, linker.movers[0].aim), (1, mode, [0.0; 3]));
            assert_eq!(linker.movers[0].at, [100.0, 13.0, 100.0]);
            // Launches at e = 2, 10 … 98 (the age 0, 8 … 96) and none else; s = (age, 0 if the age is a multiple of 16 else 1).
            let mut seen = Vec::new();
            for tick in 0..120 {
                let before = linker.cones.len();
                linker.advance(start + TICK * tick, |_| None, |_| 0);
                if linker.cones.len() > before {
                    seen.push((tick, linker.cones.last().unwrap().s, linker.cones.last().unwrap().at));
                }
            }
            let ticks: Vec<u32> = seen.iter().map(|(tick, ..)| *tick).collect();
            assert_eq!(ticks, (0..13).map(|k| 2 + 8 * k).collect::<Vec<_>>());
            assert_eq!([seen[0].1, seen[1].1, seen[2].1], [[0.0, 0.0], [8.0, 1.0], [16.0, 0.0]]);
            assert_eq!(seen[0].2, [100.0, 13.0, 100.0]);
            // The first launch is at the start, each next 8 ticks × 0.8 = 6.4 units further along the straight path.
            let step = |k: usize| f32::hypot(seen[k + 1].2[0] - seen[k].2[0], seen[k + 1].2[2] - seen[k].2[2]);
            assert!((step(0) - 6.4).abs() < 1e-3 && (step(5) - 6.4).abs() < 1e-3);
            assert!(linker.cones.iter().all(|cone| cone.dark == dark) && linker.movers.is_empty());
        }
    }

    #[test]
    fn a_cone_ring_has_three_open_bands_that_turn_brighten_and_fade() {
        let start = Instant::now();
        let mut linker = Linker::default();
        linker.cones.push(Cone::new(false, [10.0, 2.0, 20.0], [0.0, 0.0], start));
        linker.cones.push(Cone::new(true, [10.0, 2.0, 20.0], [0.0, 1.0], start));
        // The bands: h 13/11/9, turn 0/90/180, lean 55/50/45; the radius 3.4/3.6/3.8 for s.y = 0 and 2.4/2.6/2.8 for 1.
        let [blue, dark] = [&linker.cones[0], &linker.cones[1]];
        assert_eq!(
            blue.bands.iter().map(|band| (band.height, band.turn, band.lean)).collect::<Vec<_>>(),
            [(13.0, 0.0, 55.0), (11.0, 90.0, 50.0), (9.0, 180.0, 45.0)]
        );
        assert!(blue.bands.iter().zip([3.4, 3.6, 3.8]).all(|(band, radius)| near(band.radius, radius)));
        assert!(dark.bands.iter().zip([2.4, 2.6, 2.8]).all(|(band, radius)| near(band.radius, radius)));
        // Alpha at n = 1, 36, 59, 60, 96: 5, 180, 180, 175, 0; the turn after n ticks (90 k + (3 + k) n) mod 360.
        let alpha = |n: u32| {
            let mut band = ConeBand::new(0, 0.0);
            (0..n).for_each(|_| band.step(0.0));
            band.alpha
        };
        assert_eq!([1, 36, 59, 60, 96].map(alpha), [5, 180, 180, 175, 0]);
        let turn = |k: usize, n: u32| {
            let mut band = ConeBand::new(k, 0.0);
            (0..n).for_each(|_| band.step(0.0));
            band.turn
        };
        assert_eq!([turn(0, 10), turn(1, 10), turn(2, 10), turn(2, 100)], [30.0, (90.0 + 40.0) % 360.0, (180.0 + 50.0) % 360.0, (180.0 + 500.0) % 360.0]);
        // The launcher's age caps it: none up to 50, 170 at 56, 10 at 88, 0 at 96.
        let capped = |launched: f32| {
            let mut band = ConeBand::new(0, 0.0);
            (0..36).for_each(|_| band.step(launched));
            band.alpha
        };
        assert_eq!([0.0, 48.0, 50.0, 56.0, 64.0, 72.0, 80.0, 88.0, 96.0].map(capped), [180, 180, 180, 170, 130, 90, 50, 10, 0]);
        // The rim's hump h sin 9t° times sin n°, up to n = 90, then unchanged.
        let profile = |n: u32| {
            let mut band = ConeBand::new(0, 0.0);
            (0..n).for_each(|_| band.step(0.0));
            band.profile
        };
        assert!((profile(1)[10] - 13.0 * 1.0_f32.to_radians().sin()).abs() < 1e-3);
        assert!((profile(45)[10] - 13.0 * 90.0_f32.to_radians().sin() * 45.0_f32.to_radians().sin()).abs() < 1e-3);
        assert!((profile(90)[10] - 13.0).abs() < 1e-3 && profile(91) == profile(90) && profile(95) == profile(90));
        assert!(profile(90)[0].abs() < 1e-3 && profile(90)[20].abs() < 1e-3);
        // 20 quads a band, 60 a prim; blue added on in #5A5AFF, dark blended in #464646; open over 300 degrees.
        linker.advance(start + TICK * 40, |_| None, |_| 0);
        let walls = linker.cone_meshes();
        assert_eq!(walls.len(), 2);
        assert!(walls[0].additive && walls[0].texture == RING_BLUE && !walls[1].additive);
        assert!(walls.iter().all(|wall| wall.vertices.len() == 3 * 20 * 6));
        assert_eq!(walls[0].vertices[0].tint[..3], [90.0 / 255.0, 90.0 / 255.0, 1.0]);
        assert_eq!(walls[1].vertices[0].tint[..3], [70.0 / 255.0; 3]);
        // The foot of quad 1 stands on the ring at angle (turn + 0), radius r, round the given point.
        let foot = walls[0].vertices[0].position;
        assert!((f32::hypot(foot[0] - 10.0, foot[2] - 20.0) - 3.4).abs() < 1e-3 && (foot[1] - 2.0).abs() < 1e-4);
        // The prim ends at tick 100.
        linker.advance(start + TICK * 101, |_| None, |_| 0);
        assert!(linker.cones.is_empty() && linker.cone_meshes().is_empty());
    }

    #[test]
    fn the_electric_sparks_are_four_blue_violet_squares_that_rise_and_fade() {
        let start = Instant::now();
        let mut linker = Linker::default();
        // The mover's 556: four dots, size 1.2 – 1.92, x and z within ±4 of the point, 6 up from its height.
        linker.sparks.push(Sparks {
            start,
            dots: [0.0, 1.0, 2.0, 3.0].map(|k| SparkDot { half: 1.2 + 0.08 * k, at: [10.0 + k - 4.0, 8.0 + 6.0, 20.0 + 4.0], rise: 0.1 + 0.01 * k }),
        });
        // Alpha (halved when drawn) at n = 1, 2, 10, 11, 48: 0, 15, 135, 132, 21; nothing from the tick 50.
        let texture = |_: &str| None;
        assert!(linker.quads(start, texture).is_empty());
        assert_eq!([1, 2, 10, 11, 48].map(crate::prims::sparkle::alpha_at), [0, 15, 135, 132, 21]);
        // thunder_ball_0003 at n = 35, 36 for k = 0 and 28, 29 for k = 1, else 0002.
        assert_eq!([34, 35, 36, 37].map(|n| spark_picture(0, n)), [0, 1, 1, 0]);
        assert_eq!([27, 28, 29, 30].map(|n| spark_picture(1, n)), [0, 1, 1, 0]);
        assert!(THUNDER[0].ends_with("0002.bmp") && THUNDER[1].ends_with("0003.bmp"));
        assert!(linker.quads(start + TICK * 60, texture).is_empty());
        linker.advance(start + TICK * 51, |_| None, |_| 0);
        assert!(linker.sparks.is_empty());
    }
}
