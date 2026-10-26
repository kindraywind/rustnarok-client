//! Comfort of the Sun, Moon and Stars' eight breathing cones (488), Soul Link's mote and the letters SOUL LINK (503) and Stin's
//! spinning wind blade (547), a 16 ms tick at a time.
//!
//! A shape's own age n counts from 1 on its first tick; e is the ticks since the effect started.

use std::time::Instant;

use super::passes;
use super::tanji::bearing;
use super::walls::Wall;
use crate::world::{MeshVertex, Vector};

pub const RING_BLUE: &str = "data\\texture\\effect\\ring_blue.tga";
pub const MOTE: &str = super::spray::DOT;
pub const BLADE: &str = "data\\texture\\effect\\270바람.tga";
/// The letters' pictures: l, u, o, s, k, n, i.
pub const LETTERS: [&str; 7] = [
    "data\\texture\\effect\\soul_l.tga",
    "data\\texture\\effect\\soul_u.tga",
    "data\\texture\\effect\\soul_o.tga",
    "data\\texture\\effect\\soul_s.tga",
    "data\\texture\\effect\\soul_k.tga",
    "data\\texture\\effect\\soul_n.tga",
    "data\\texture\\effect\\soul_i.tga",
];
const PIXELS_PER_UNIT: f32 = 7.0;
const COMFORT_LIFE: u32 = 400;
const LINK_LIFE: u32 = 500;
const BLADE_LIFE: u32 = 200;
/// Comfort's cones, per prim and band: radius, lean, base height and turn.
const CONES: [[(f32, f32, f32, f32); 4]; 2] = [
    [(4.5, 85.0, 27.0, 0.0), (5.0, 80.0, 28.0, 90.0), (5.5, 70.0, 29.0, 180.0), (5.0, 75.0, 30.0, 270.0)],
    [(5.5, 50.0, 30.0, 90.0), (6.0, 55.0, 29.0, 180.0), (6.5, 65.0, 28.0, 270.0), (6.0, 60.0, 27.0, 0.0)],
];
/// The letters' pictures (indices into `LETTERS`), sideways offsets and start ages: SOUL made at tick 1, LINK at 21.
const SOUL_LETTERS: [(usize, f32, i32); 4] = [(0, -4.0, -60), (1, -10.0, -40), (2, -16.0, -20), (3, -22.0, 0)];
const LINK_LETTERS: [(usize, f32, i32); 4] = [(4, 22.0, -120), (5, 16.0, -100), (6, 10.0, -80), (0, 4.0, -60)];
/// The letters' halo caps: a copy's alpha cap is 33 + 23 i.
const LETTER_CAPS: [i32; 10] = [33, 56, 79, 102, 125, 148, 171, 194, 217, 240];

struct Comfort {
    unit: Option<u32>,
    start: Instant,
}

struct Letter {
    picture: usize,
    sideways: f32,
    age: i32,
    size: f32,
    alphas: [i32; 10],
}

struct Link {
    unit: Option<u32>,
    start: Instant,
    done: u32,
    mote_alpha: i32,
    theta: f32,
    letters: Vec<Letter>,
}

/// One element of the wind blade: its place, spin and alpha.
#[derive(Clone, Copy, Debug)]
pub struct Element {
    pub at: Vector,
    pub spin: f32,
    pub alpha: i32,
}

struct Blade {
    start: Instant,
    aim: Vector,
    done: u32,
    age: i32,
    heading: f32,
    speed: f32,
    gone: bool,
    elements: [Element; 4],
}

/// A camera-facing square to draw: picture, place, half-diagonal, colour, alpha (0 to 255) and whether it adds on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SquareDraw {
    pub texture: &'static str,
    pub at: Vector,
    pub half: f32,
    pub rgb: [f32; 3],
    pub alpha: f32,
    pub additive: bool,
}

#[derive(Default)]
pub struct Soul {
    comforts: Vec<Comfort>,
    links: Vec<Link>,
    blades: Vec<Blade>,
    /// The soul lights (506) the mote launched: on whom and at what offset from the feet, since the last ask.
    launched: Vec<(Option<u32>, Vector)>,
}

/// A cone band's alpha at its age n: 5 a tick to 95, held to 110, then 1 less a tick (0 at 205).
pub fn cone_alpha(n: u32) -> f32 {
    match n {
        0 => 0.0,
        1..=19 => 5.0 * n as f32,
        20..=110 => 95.0,
        _ => (95.0 - (n - 110) as f32).max(0.0),
    }
}

/// A cone band's height at its age n: its base ±2.5 breathing with a period of 90 ticks.
pub fn cone_height(base: f32, n: u32) -> f32 {
    base + 2.5 * (4.0 * (n % 90) as f32).to_radians().sin()
}

/// A cone rim's height at point s of its band at the age n.
pub fn cone_profile(height: f32, s: u32, n: u32) -> f32 {
    let grow = if n <= 90 { (n as f32).to_radians().sin() } else { 1.0 };
    (height * (9.0 * s as f32).to_radians().sin() * grow).max(0.0)
}

/// Comfort's blue outline alpha at its tick e: up to 199 over 200 ticks, then back down to 1.
pub fn comfort_outline(e: u32) -> Option<u32> {
    match e {
        0..=199 => Some(e),
        200..=399 => Some(400 - e),
        _ => None,
    }
}

/// The mote's alpha at its age n: 20 a tick to 200, held to 170, then 5 less a tick (0 at 210).
pub fn mote_alpha(n: u32) -> i32 {
    match n {
        0 => 0,
        1..=10 => 20 * n as i32,
        11..=170 => 200,
        _ => (200 - 5 * (n as i32 - 170)).max(0),
    }
}

/// The mote's angle after n ticks: n (n + 1) / 60, at most 359.
pub fn mote_theta(n: u32) -> f32 {
    ((n * (n + 1)) as f32 / 60.0).min(359.0)
}

/// The mote's offset across the screen and up at the angle theta: -30 to +29.8, and 40 ± 20.
pub fn mote_offset(theta: f32) -> (f32, f32) {
    let whole = theta.floor();
    (whole / 6.0 - 30.0, 40.0 + 20.0 * whole.to_radians().sin())
}

impl Letter {
    fn new((picture, sideways, age): (usize, f32, i32)) -> Self {
        Self { picture, sideways, age, size: 5.0, alphas: [0; 10] }
    }

    fn step(&mut self) {
        self.age += 1;
        if self.age <= 0 {
            return;
        }
        let n = self.age;
        if n < 20 {
            for (alpha, cap) in self.alphas.iter_mut().zip(LETTER_CAPS) {
                *alpha = (*alpha + 13).min(cap);
            }
        } else {
            for alpha in &mut self.alphas[..8] {
                *alpha = (*alpha - 2).max(0);
            }
            if n > 100 {
                for alpha in &mut self.alphas[8..] {
                    *alpha = (*alpha - 5).max(0);
                    if *alpha > 0 {
                        self.size *= 1.005;
                    }
                }
            }
        }
    }
}

impl Blade {
    /// One tick: the blade turns 30°, picks its heading at n = 1, brightens to 225 by n = 10 then fades, speeds up and flies; the
    /// copies take its place every third tick at four fifths of the alpha before.
    fn step(&mut self) {
        self.age += 1;
        if self.age <= 0 {
            return;
        }
        let n = self.age;
        if !self.gone {
            let blade = &mut self.elements[0];
            blade.spin = (blade.spin + 30.0) % 360.0;
            if n == 1 {
                self.heading = bearing(self.aim[0] - blade.at[0], self.aim[2] - blade.at[2]) as f32;
            } else if n <= 10 {
                blade.alpha = (blade.alpha + 25).min(250);
                self.speed *= 1.02;
            } else {
                blade.alpha -= 5;
                self.speed *= 1.04;
            }
            if blade.alpha < 0 {
                blade.alpha = 0;
                self.gone = true;
            } else {
                let (sin, cos) = self.heading.to_radians().sin_cos();
                blade.at[0] += self.speed * cos;
                blade.at[2] += self.speed * sin;
            }
        }
        if n % 3 == 0 {
            for k in (1..4).rev() {
                let before = self.elements[k - 1];
                self.elements[k] = Element { at: before.at, spin: before.spin, alpha: (0.8 * before.alpha as f32).floor() as i32 };
            }
        }
    }
}

impl Soul {
    /// Comfort's cones round `unit`'s feet.
    pub fn comfort(&mut self, unit: Option<u32>, now: Instant) {
        self.comforts.push(Comfort { unit, start: now });
    }

    /// Soul Link's mote and letters over `unit`.
    pub fn link(&mut self, unit: Option<u32>, now: Instant) {
        self.links.push(Link { unit, start: now, done: 0, mote_alpha: 0, theta: 0.0, letters: Vec::new() });
    }

    /// Stin's wind blade from `from` (the caster's feet) toward `aim`; the heights from `rand(n)`.
    pub fn stin(&mut self, from: Vector, aim: Vector, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        let elements = [(); 4].map(|()| Element { at: [from[0], from[1] + 10.0 - rand(5) as f32, from[2]], spin: 0.0, alpha: 0 });
        let elements: [Element; 4] = std::array::from_fn(|k| Element { spin: k as f32, ..elements[k] });
        let age = -3;
        self.blades.push(Blade { start: now, aim, done: 0, age, heading: 0.0, speed: 0.8, gone: false, elements });
    }

    /// The soul lights launched since this was last asked (they have no look yet).
    pub fn take_launches(&mut self) -> Vec<(Option<u32>, Vector)> {
        std::mem::take(&mut self.launched)
    }

    /// Runs the letters, the mote and the blade up to `now`, a tick at a time, and drops what ended.
    pub fn advance(&mut self, now: Instant, position_of: impl Fn(Option<u32>) -> Option<Vector>, yaw: f32) {
        let mut lights = Vec::new();
        for link in &mut self.links {
            while link.done < passes(link.start, now).min(LINK_LIFE) {
                let e = link.done;
                link.done += 1;
                if e == 1 {
                    link.letters.extend(SOUL_LETTERS.map(Letter::new));
                }
                if e == 21 {
                    link.letters.extend(LINK_LETTERS.map(Letter::new));
                }
                // The mote: n = e + 1 on the first tick.
                let n = e + 1;
                link.mote_alpha = mote_alpha(n);
                link.theta = mote_theta(n);
                if let (true, Some(_)) = (n < 180, position_of(link.unit)) {
                    let (across, height) = mote_offset(link.theta);
                    let (sin, cos) = yaw.to_radians().sin_cos();
                    lights.push((link.unit, [across * cos, height, across * sin]));
                }
                for letter in &mut link.letters {
                    letter.step();
                }
            }
        }
        self.launched.extend(lights);
        self.links.retain(|link| passes(link.start, now) <= LINK_LIFE);
        self.comforts.retain(|comfort| passes(comfort.start, now) <= COMFORT_LIFE);
        for blade in &mut self.blades {
            while blade.done < passes(blade.start, now).min(BLADE_LIFE) {
                blade.done += 1;
                blade.step();
            }
        }
        self.blades.retain(|blade| passes(blade.start, now) <= BLADE_LIFE);
    }

    /// The cones and the blade at `now`.
    pub fn meshes(&self, now: Instant, position_of: impl Fn(Option<u32>) -> Option<Vector>) -> Vec<Wall> {
        let mut walls = Vec::new();
        for comfort in &self.comforts {
            let (Some(feet), Some(t)) = (position_of(comfort.unit), passes(comfort.start, now).checked_sub(1)) else { continue };
            let n = t + 1;
            let alpha = cone_alpha(n);
            if alpha <= 0.0 {
                continue;
            }
            let mut vertices = Vec::new();
            for prim in &CONES {
                for &(radius, lean, base, turn) in prim {
                    let height = cone_height(base, n);
                    let (lean_sin, lean_cos) = lean.to_radians().sin_cos();
                    let tint = [100.0 / 255.0, 100.0 / 255.0, 1.0, alpha / 255.0];
                    let corner = |s: u32, top: bool, uv: [f32; 2]| {
                        let (sin, cos) = ((turn + 15.0 * s as f32) % 360.0).to_radians().sin_cos();
                        let (reach, up) = if top {
                            let p = cone_profile(height, s, n);
                            (radius + p * lean_cos, p * lean_sin)
                        } else {
                            (radius, 0.0)
                        };
                        MeshVertex { position: [feet[0] + reach * cos, feet[1] + up, feet[2] + reach * sin], uv, tint }
                    };
                    for s in 1..=20 {
                        let (u0, u1) = ((s - 1) as f32 / 21.0, s as f32 / 21.0);
                        let [a, b, c, d] =
                            [corner(s - 1, false, [u0, 1.0]), corner(s, false, [u1, 1.0]), corner(s, true, [u1, 0.0]), corner(s - 1, true, [u0, 0.0])];
                        vertices.extend([a, b, c, a, c, d]);
                    }
                }
            }
            walls.push(Wall { texture: RING_BLUE, vertices, additive: true, forced: false });
        }
        let mut blade_vertices = Vec::new();
        for blade in &self.blades {
            for element in blade.elements.iter().filter(|element| element.alpha > 0) {
                let tint = [80.0 / 255.0, 80.0 / 255.0, 1.0, element.alpha as f32 / 255.0];
                let corner = |angle: f32, uv: [f32; 2]| {
                    let (sin, cos) = (element.spin + angle).to_radians().sin_cos();
                    MeshVertex { position: [element.at[0] + 20.0 * cos, element.at[1], element.at[2] + 20.0 * sin], uv, tint }
                };
                let [p0, p1, p2, p3] = [corner(90.0, [0.0, 1.0]), corner(180.0, [1.0, 1.0]), corner(270.0, [1.0, 0.0]), corner(360.0, [0.0, 0.0])];
                blade_vertices.extend([p0, p1, p2, p0, p2, p3]);
            }
        }
        if !blade_vertices.is_empty() {
            walls.push(Wall { texture: BLADE, vertices: blade_vertices, additive: true, forced: false });
        }
        walls
    }

    /// The mote and the letters' squares at `now` over their units' feet, turned for a camera at `yaw` degrees.
    pub fn squares(&self, yaw: f32, position_of: impl Fn(Option<u32>) -> Option<Vector>) -> Vec<SquareDraw> {
        let mut draws = Vec::new();
        let (sin, cos) = yaw.to_radians().sin_cos();
        for link in &self.links {
            let Some(feet) = position_of(link.unit) else { continue };
            if link.mote_alpha > 0 {
                let (across, height) = mote_offset(link.theta);
                let at = [feet[0] + across * cos, feet[1] + height, feet[2] + across * sin];
                draws.push(SquareDraw {
                    texture: MOTE,
                    at,
                    half: 2.5,
                    rgb: [125.0 / 255.0, 125.0 / 255.0, 1.0],
                    alpha: link.mote_alpha as f32,
                    additive: true,
                });
            }
            for letter in link.letters.iter().filter(|letter| letter.age > 0) {
                let at = [feet[0] + letter.sideways * cos, feet[1] + 40.0, feet[2] + letter.sideways * sin];
                for j in 0..10 {
                    let alpha = letter.alphas[9 - j];
                    if alpha > 0 {
                        draws.push(SquareDraw {
                            texture: LETTERS[letter.picture],
                            at,
                            half: letter.size + j as f32,
                            rgb: [1.0; 3],
                            alpha: alpha as f32,
                            additive: false,
                        });
                    }
                }
            }
        }
        draws
    }

    /// The squares as quads with their textures (upright, in screen pixels).
    pub fn corners(half: f32) -> [[f32; 2]; 4] {
        let side = half * PIXELS_PER_UNIT;
        [225.0_f32, 315.0, 45.0, 135.0].map(|angle| {
            let (sin, cos) = angle.to_radians().sin_cos();
            [side * cos, side * sin]
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prims::TICK;

    fn near(got: f32, want: f32) -> bool {
        (got - want).abs() < 1e-3
    }

    #[test]
    fn comfort_opens_eight_breathing_cones_and_a_swelling_outline() {
        let start = Instant::now();
        let mut soul = Soul::default();
        soul.comfort(Some(1), start);
        // Alpha at n = 1, 19, 20, 110, 111, 205: 5, 95, 95, 95, 94, 0.
        assert_eq!([1, 19, 20, 110, 111, 205].map(cone_alpha), [5.0, 95.0, 95.0, 95.0, 94.0, 0.0]);
        // The height at n = 1, 22, 23, 90: h0 + 2.5 sin 4°, sin 88°, sin 92°, h0.
        let h0 = 27.0;
        assert!(near(cone_height(h0, 1), h0 + 2.5 * 4.0_f32.to_radians().sin()) && near(cone_height(h0, 22), h0 + 2.5 * 88.0_f32.to_radians().sin()));
        assert!(near(cone_height(h0, 23), h0 + 2.5 * 92.0_f32.to_radians().sin()) && near(cone_height(h0, 90), h0));
        // The profile at n = 45, s = 10: h sin 90° sin 45°; zero at both ends.
        assert!(
            near(cone_profile(20.0, 10, 45), 20.0 * 45.0_f32.to_radians().sin()) && cone_profile(20.0, 0, 45) == 0.0 && cone_profile(20.0, 20, 45).abs() < 1e-3
        );
        // Eight bands of 20 quads, added on, blue-violet, round the feet, following the caster; gone at n = 206.
        let walls = soul.meshes(start + TICK * 50, |_| Some([10.0, 1.0, 10.0]));
        assert_eq!(walls.len(), 1);
        assert!(walls[0].additive && walls[0].texture == RING_BLUE && walls[0].vertices.len() == 8 * 20 * 6);
        assert_eq!(walls[0].vertices[0].tint[..3], [100.0 / 255.0, 100.0 / 255.0, 1.0]);
        assert!(soul.meshes(start, |_| None).is_empty());
        assert!(soul.meshes(start + TICK * 205, |_| Some([0.0; 3])).is_empty());
        // The outline's f at e = 0, 199, 200, 399: 0, 199, 200, 1; none from 400.
        assert_eq!([0, 199, 200, 399].map(|e| comfort_outline(e).unwrap()), [0, 199, 200, 1]);
        assert_eq!(comfort_outline(400), None);
        soul.advance(start + TICK * 401, |_| None, 0.0);
        assert!(soul.comforts.is_empty());
    }

    #[test]
    fn the_mote_swoops_and_the_letters_spell_soul_link() {
        // The mote's alpha at n = 1, 10, 11, 170, 171, 210: 20, 200, 200, 200, 195, 0.
        assert_eq!([1, 10, 11, 170, 171, 210].map(mote_alpha), [20, 200, 200, 200, 195, 0]);
        // θ at n = 1, 146, 147, 148: 0.033, 357.7, 359, 359; x at n = 147 and 1: 29.8 and −30; the height at n = 1: 40.
        assert!(near(mote_theta(1), 2.0 / 60.0) && near(mote_theta(146), 146.0 * 147.0 / 60.0) && mote_theta(147) == 359.0 && mote_theta(148) == 359.0);
        let (x, _) = mote_offset(mote_theta(147));
        assert!((x - 29.83).abs() < 0.01);
        assert_eq!(mote_offset(mote_theta(1)), (-30.0, 40.0));
        let start = Instant::now();
        let feet = [5.0, 1.0, 7.0];
        let mut soul = Soul::default();
        soul.link(Some(1), start);
        let mut first = [None; 8];
        let mut launches = 0;
        for tick in 0..=200 {
            soul.advance(start + TICK * tick, |_| Some(feet), 0.0);
            launches += soul.take_launches().len();
            for (index, letter) in soul.links[0].letters.iter().enumerate() {
                if letter.age > 0 && first[index].is_none() {
                    first[index] = Some(tick);
                }
            }
        }
        // 506 launched on n = 1 … 179 only.
        assert_eq!(launches, 179);
        // Two prims of four letters: the letters appear at e = 1, 21 … 141, S, O, U, L, L, I, N, K (the element order of the table).
        let mut times: Vec<(u32, usize)> = first.iter().enumerate().map(|(index, tick)| (tick.unwrap(), index)).collect();
        times.sort();
        assert_eq!(times.iter().map(|(tick, _)| *tick).collect::<Vec<_>>(), [1, 21, 41, 61, 81, 101, 121, 141]);
        let letters = &soul.links[0].letters;
        let spelled: Vec<usize> = times.iter().map(|(_, index)| letters[*index].picture).collect();
        // s, o, u, l, then l, i, n, k.
        assert_eq!(spelled, [3, 2, 1, 0, 0, 6, 5, 4]);
        assert!(letters[3].size > 5.0 && letters[4].size == 5.0, "the first core swelled past n = 100, the last has not got there");
        // A letter's alphas: a9 at n = 1, 18, 19, 100, 101, 148: 13, 234, 240, 240, 235, 0; a0 at n = 1, 3, 19, 20, 36: 13, 33, 33, 31, 0.
        let mut letter = Letter::new((3, 0.0, 0));
        let mut nines = Vec::new();
        let mut zeros = Vec::new();
        for n in 1..=148 {
            letter.step();
            if [1, 18, 19, 100, 101, 148].contains(&n) {
                nines.push(letter.alphas[9]);
            }
            if [1, 3, 19, 20, 36].contains(&n) {
                zeros.push(letter.alphas[0]);
            }
        }
        assert_eq!(nines, [13, 234, 240, 240, 235, 0]);
        assert_eq!(zeros, [13, 33, 33, 31, 0]);
        // Ten copies of a letter with half-diagonals size + 0 … 9, normal blend, white, 40 up; the mote added on in #7D7DFF.
        let mut late = Soul::default();
        late.link(Some(1), start);
        for tick in 0..=30 {
            late.advance(start + TICK * tick, |_| Some(feet), 0.0);
        }
        let squares = late.squares(0.0, |_| Some(feet));
        let letter_squares: Vec<&SquareDraw> = squares.iter().filter(|draw| draw.texture != MOTE).collect();
        assert!(!letter_squares.is_empty() && letter_squares.iter().all(|draw| !draw.additive && draw.rgb == [1.0; 3] && draw.at[1] == 41.0));
        let mote = squares.iter().find(|draw| draw.texture == MOTE).unwrap();
        assert!(mote.additive && mote.rgb == [125.0 / 255.0, 125.0 / 255.0, 1.0] && mote.half == 2.5);
        // Follows the target.
        assert!(late.squares(0.0, |_| Some([100.0, 1.0, 100.0])).iter().all(|draw| draw.at[0] > 60.0));
        late.advance(start + TICK * 501, |_| None, 0.0);
        assert!(late.links.is_empty());
    }

    #[test]
    fn stin_shoots_a_spinning_wind_disc_with_three_trailing_copies() {
        let start = Instant::now();
        let from = [10.0, 1.0, 10.0];
        let aim = [50.0, 1.0, 10.0];
        let mut soul = Soul::default();
        let mut draws = Vec::new();
        soul.stin(from, aim, start, |n| {
            draws.push(n);
            0
        });
        // Four elements, a height draw each (10 − R(5) up), only element 0 driven; nothing moves for the first 3 ticks.
        assert_eq!(draws, [5, 5, 5, 5]);
        assert!(soul.blades[0].elements.iter().all(|element| element.at[1] == 11.0));
        for tick in 0..3 {
            soul.advance(start + TICK * tick, |_| None, 0.0);
        }
        assert_eq!(soul.blades[0].elements[0].at, [10.0, 11.0, 10.0]);
        // n = 1 (the fourth tick): the heading toward the aim (due +x: 0) and the first step at 0.8.
        soul.advance(start + TICK * 3, |_| None, 0.0);
        assert_eq!(soul.blades[0].heading, 0.0);
        assert!(near(soul.blades[0].elements[0].at[0], 10.8) && soul.blades[0].elements[0].spin == 30.0);
        // Alpha at n = 2, 10, 11, 55, 56: 25, 225, 220, 0, gone; the speed 0.8·1.02⁹ after n = 10, ·1.04 at 11.
        let mut seen = Vec::new();
        for n in 2..=56 {
            soul.advance(start + TICK * (n + 2), |_| None, 0.0);
            if [2, 10, 11, 55, 56].contains(&n) {
                seen.push((soul.blades[0].elements[0].alpha, soul.blades[0].speed, soul.blades[0].gone));
            }
        }
        assert_eq!(seen.iter().map(|each| each.0).collect::<Vec<_>>(), [25, 225, 220, 0, 0]);
        assert!(near(seen[1].1, 0.8 * 1.02_f32.powi(9)) && near(seen[2].1, 0.8 * 1.02_f32.powi(9) * 1.04));
        assert!(!seen[3].2 && seen[4].2);
        // It passes a target 40 away without stopping (it ends about 130 out).
        assert!(soul.blades[0].elements[0].at[0] - 10.0 > 100.0);
        // The trail copies take 0.8 of the alpha before on every third tick.
        let mut trail = Soul::default();
        trail.stin(from, aim, start, |_| 0);
        for tick in 0..=8 {
            trail.advance(start + TICK * tick, |_| None, 0.0);
        }
        let elements = &trail.blades[0].elements;
        assert_eq!((elements[0].alpha, elements[1].alpha), (125, 100));
        // Drawn flat in #5050FF added on; the spin 30 a tick; it ends with the effect.
        let walls = trail.meshes(start, |_| None);
        assert!(walls[0].additive && walls[0].texture == BLADE && walls[0].vertices[0].tint[..3] == [80.0 / 255.0, 80.0 / 255.0, 1.0]);
        trail.advance(start + TICK * 201, |_| None, 0.0);
        assert!(trail.blades.is_empty());
    }
}
