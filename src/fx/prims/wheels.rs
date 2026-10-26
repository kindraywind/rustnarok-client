//! Kamaitachi's five rolling wind wheels (624), a 16 ms tick at a time: one wheel a tick for five ticks, each flying toward the
//! target with three fading copies behind it, standing upright in its plane of travel.
//!
//! A wheel's own age n counts from 1 on its first tick; e is the ticks since the effect started.

use std::time::Instant;

use super::passes;
use super::tanji::bearing;
use super::walls::Wall;
use crate::world::{MeshVertex, Vector};

pub use super::soul::BLADE;
const WHEELS: u32 = 5;
const LIFE: u32 = 200;
/// Where each wheel starts from the caster's feet, by the tick it is made on.
const OFFSETS: [[f32; 2]; 5] = [[0.0, 0.0], [5.0, 0.0], [-5.0, 0.0], [0.0, 5.0], [0.0, -5.0]];
const RADIUS: f32 = 20.0;
const TINT: [f32; 3] = [80.0 / 255.0, 1.0, 80.0 / 255.0];

/// One element of a wheel: its place, its own heading (the copies keep theirs), spin and alpha.
#[derive(Clone, Copy, Debug)]
pub struct Element {
    pub at: Vector,
    pub heading: f32,
    pub spin: f32,
    pub alpha: i32,
}

pub struct Wheel {
    start: Instant,
    done: u32,
    age: i32,
    home: Vector,
    aim: Vector,
    speed: f32,
    gone: bool,
    pub elements: [Element; 4],
}

#[derive(Default)]
pub struct Wheels {
    /// The throws not all five wheels of which are made yet: the caster, where it first stood, the aim and the start.
    throws: Vec<Throw>,
    wheels: Vec<Wheel>,
}

struct Throw {
    caster: Option<u32>,
    last: Vector,
    aim: Vector,
    start: Instant,
    made: u32,
}

impl Wheel {
    /// The wheel made from the caster's feet `home`, started by the offset of its tick; its choices from `rand(n)`.
    fn new(home: Vector, offset: [f32; 2], aim: Vector, start: Instant, rand: &mut impl FnMut(u32) -> u32) -> Self {
        let elements = std::array::from_fn(|k| {
            let height = 10.0 - rand(5) as f32;
            let heading = rand(360) as f32;
            Element { at: [home[0] + offset[0], home[1] + height, home[2] + offset[1]], heading, spin: k as f32, alpha: 0 }
        });
        Self { start, done: 0, age: -3, home, aim, speed: 1.5, gone: false, elements }
    }

    /// One tick: it turns 30°, picks its heading at n = 1, brightens to 54 by n = 10 then fades, speeds up and flies; the copies
    /// take its place every third tick at four fifths of the alpha before.
    fn step(&mut self) {
        self.age += 1;
        if self.age <= 0 {
            return;
        }
        let n = self.age;
        if !self.gone {
            let wheel = &mut self.elements[0];
            wheel.spin = (wheel.spin + 30.0) % 360.0;
            if n == 1 {
                wheel.heading = bearing(self.aim[0] - self.home[0], self.aim[2] - self.home[2]) as f32;
            } else if n <= 10 {
                wheel.alpha = (wheel.alpha + 6).min(60);
                self.speed *= 1.02;
            } else {
                wheel.alpha -= 3;
                self.speed *= 1.04;
            }
            if wheel.alpha < 0 {
                wheel.alpha = 0;
                self.gone = true;
            } else {
                let (sin, cos) = wheel.heading.to_radians().sin_cos();
                wheel.at[0] += self.speed * cos;
                wheel.at[2] += self.speed * sin;
            }
        }
        if n % 3 == 0 {
            for k in (1..4).rev() {
                let before = self.elements[k - 1];
                let kept = self.elements[k].heading;
                self.elements[k] = Element { alpha: (0.8 * before.alpha as f32).floor() as i32, heading: kept, ..before };
            }
        }
    }
}

impl Wheels {
    /// Kamaitachi's wheels from the feet of `caster` (at `from` until it can be found), flying toward `aim`.
    pub fn throw(&mut self, caster: Option<u32>, from: Vector, aim: Vector, now: Instant) {
        self.throws.push(Throw { caster, last: from, aim, start: now, made: 0 });
    }

    /// Makes the wheel of each tick e = 0 … 4 (from the caster's feet then), runs them a tick at a time and drops what ended; the
    /// choices of each wheel come from `rand(n)`.
    pub fn advance(&mut self, now: Instant, position_of: impl Fn(Option<u32>) -> Option<Vector>, mut rand: impl FnMut(u32) -> u32) {
        for throw in &mut self.throws {
            throw.last = position_of(throw.caster).unwrap_or(throw.last);
            while throw.made < WHEELS && passes(throw.start, now) > throw.made {
                let offset = OFFSETS[throw.made as usize];
                let start = throw.start + super::TICK * throw.made;
                self.wheels.push(Wheel::new(throw.last, offset, throw.aim, start, &mut rand));
                throw.made += 1;
            }
        }
        self.throws.retain(|throw| throw.made < WHEELS);
        for wheel in &mut self.wheels {
            while wheel.done < passes(wheel.start, now).min(LIFE) {
                wheel.done += 1;
                wheel.step();
            }
        }
        self.wheels.retain(|wheel| passes(wheel.start, now) <= LIFE);
    }

    /// The wheels: green squares standing upright in the plane of travel of each element's heading, added on.
    pub fn meshes(&self) -> Vec<Wall> {
        let mut vertices = Vec::new();
        for wheel in &self.wheels {
            for element in wheel.elements.iter().filter(|element| element.alpha > 0) {
                let tint = [TINT[0], TINT[1], TINT[2], element.alpha as f32 / 255.0];
                let (sin, cos) = element.heading.to_radians().sin_cos();
                let corner = |j: u32, uv: [f32; 2]| {
                    let (a_sin, a_cos) = (element.spin + 90.0 + 90.0 * j as f32).to_radians().sin_cos();
                    MeshVertex {
                        position: [element.at[0] + cos * RADIUS * a_cos, element.at[1] + RADIUS * a_sin, element.at[2] + sin * RADIUS * a_cos],
                        uv,
                        tint,
                    }
                };
                let [p0, p1, p2, p3] = [corner(0, [0.0, 1.0]), corner(1, [1.0, 1.0]), corner(2, [1.0, 0.0]), corner(3, [0.0, 0.0])];
                vertices.extend([p0, p1, p2, p0, p2, p3]);
            }
        }
        if vertices.is_empty() {
            return Vec::new();
        }
        vec![Wall { texture: BLADE, vertices, additive: true, forced: false }]
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

    fn wheel(aim: Vector, ticks: u32) -> Wheel {
        let mut wheel = Wheel::new(FEET, [0.0, 0.0], aim, Instant::now(), &mut |_| 0);
        for _ in 0..ticks {
            wheel.step();
        }
        wheel
    }

    #[test]
    fn five_wheels_are_made_one_a_tick_with_four_elements_each() {
        let start = Instant::now();
        let mut wheels = Wheels::default();
        wheels.throw(Some(1), FEET, [300.0, 3.0, 100.0], start);
        let mut draws = Vec::new();
        wheels.advance(
            start,
            |_| Some(FEET),
            |n| {
                draws.push(n);
                0
            },
        );
        assert_eq!(wheels.wheels.len(), 1);
        // Per element R(5) then R(360); every draw 0: height 10, heading 0, age k − 3 (−3 then ticks to −2 in the first tick).
        assert_eq!(draws, [5, 360, 5, 360, 5, 360, 5, 360]);
        // Start offsets (0, 0), (+5, 0), (−5, 0), (0, +5), (0, −5), the new wheel read as it is made; none on e = 5.
        let mut places = vec![[wheels.wheels[0].elements[0].at[0] - 100.0, wheels.wheels[0].elements[0].at[2] - 100.0]];
        for tick in 1..=4 {
            wheels.advance(start + TICK * tick, |_| Some(FEET), |_| 0);
            let newest = wheels.wheels.last().unwrap();
            places.push([newest.elements[0].at[0] - 100.0, newest.elements[0].at[2] - 100.0]);
        }
        assert_eq!(wheels.wheels.len(), 5);
        assert_eq!(places, [[0.0, 0.0], [5.0, 0.0], [-5.0, 0.0], [0.0, 5.0], [0.0, -5.0]]);
        wheels.advance(start + TICK * 5, |_| Some(FEET), |_| 0);
        assert_eq!(wheels.wheels.len(), 5);
        assert!(wheels.elements_are_at_height(13.0));
        // The heights 10 and 6 for the draws 0 and 4.
        let high = Wheel::new(FEET, [0.0; 2], FEET, start, &mut |n| if n == 5 { 4 } else { 0 });
        assert_eq!(high.elements[0].at[1], 3.0 + 6.0);
    }

    impl Wheels {
        fn elements_are_at_height(&self, height: f32) -> bool {
            self.wheels.iter().all(|wheel| wheel.elements.iter().all(|element| element.at[1] == height))
        }
    }

    #[test]
    fn a_wheel_waits_three_ticks_then_flies_toward_the_aim_and_fades() {
        let aim = [300.0, 3.0, 100.0];
        // Ages −2, −1, 0 do nothing; the heading is the bearing of the aim from the wheel's place at n = 1.
        assert!(wheel(aim, 3).elements[0].at == [100.0, 13.0, 100.0] && wheel(aim, 3).elements[0].spin == 0.0);
        let first = wheel(aim, 4);
        assert_eq!((first.elements[0].heading, first.elements[0].spin), (0.0, 30.0));
        assert!(near(first.elements[0].at[0], 101.5));
        // Speed 1.5, 1.53 at n = 2, ×1.04 from n = 11; spin 30 n mod 360.
        assert!(near(wheel(aim, 5).speed, 1.53));
        assert!(near(wheel(aim, 13).speed, 1.5 * 1.02_f32.powi(9)));
        assert!(near(wheel(aim, 14).speed, 1.5 * 1.02_f32.powi(9) * 1.04));
        assert_eq!(wheel(aim, 3 + 13).elements[0].spin, (13.0 * 30.0) % 360.0);
        // Alpha at n = 1, 2, 10, 11, 28 and then gone at 29: 0, 6, 54, 51, 0.
        let alpha = |n: u32| wheel(aim, 3 + n).elements[0].alpha;
        assert_eq!([1, 2, 10, 11, 28].map(alpha), [0, 6, 54, 51, 0]);
        assert!(!wheel(aim, 3 + 28).gone && wheel(aim, 3 + 29).gone);
    }

    #[test]
    fn the_copies_take_four_fifths_of_the_alpha_and_keep_their_own_heading() {
        let aim = [300.0, 3.0, 100.0];
        let mut wheel = Wheel::new(FEET, [0.0; 2], aim, Instant::now(), &mut |n| if n == 360 { 77 } else { 0 });
        for _ in 0..3 + 3 {
            wheel.step();
        }
        // After n = 3 element 1 has ⌊0.8 × 12⌋ = 9; its heading stays the random 77, the wheel's is the bearing.
        assert_eq!(wheel.elements.map(|element| element.alpha), [12, 9, 0, 0]);
        assert_eq!(wheel.elements[1].heading, 77.0);
        assert_eq!(wheel.elements[0].heading, 0.0);
    }

    #[test]
    fn the_wheel_stands_upright_in_its_plane_of_travel() {
        let aim = [300.0, 3.0, 100.0];
        let mut wheels = Wheels::default();
        let start = Instant::now();
        wheels.throw(None, FEET, aim, start);
        wheels.advance(start, |_| Some(FEET), |_| 0);
        // Give the first wheel an alpha and a spin of 0 at heading 0 and read the corners σ = 90, 180, 270, 360.
        wheels.wheels[0].elements[0] = Element { at: [10.0, 20.0, 30.0], heading: 0.0, spin: 0.0, alpha: 50 };
        let walls = wheels.meshes();
        assert_eq!(walls.len(), 1);
        let wall = &walls[0];
        assert!(wall.additive && wall.texture == BLADE && !wall.forced);
        let corner = |index: usize| wall.vertices[index].position;
        // Vertices come as p0 p1 p2 p0 p2 p3.
        let [p0, p1, p2, p3] = [corner(0), corner(1), corner(2), corner(5)];
        assert!(near(p0[0], 10.0) && near(p0[1], 40.0) && near(p0[2], 30.0));
        assert!(near(p1[0], -10.0) && near(p1[1], 20.0) && near(p1[2], 30.0));
        assert!(near(p2[0], 10.0) && near(p2[1], 0.0) && near(p2[2], 30.0));
        assert!(near(p3[0], 30.0) && near(p3[1], 20.0) && near(p3[2], 30.0));
        assert_eq!(wall.vertices[0].tint, [80.0 / 255.0, 1.0, 80.0 / 255.0, 50.0 / 255.0]);
        // Gone with the effect at its 200th tick.
        wheels.advance(start + TICK * 210, |_| None, |_| 0);
        assert!(wheels.wheels.is_empty() && wheels.meshes().is_empty());
    }
}
