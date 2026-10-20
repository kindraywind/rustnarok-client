//! The Soul Linker's Esma (553, 554) and Estun (555): two wind discs curling round to the target, the frost picture swelling on the
//! target, and the invisible mover that launches the sparks (556), a 16 ms tick at a time.
//!
//! A shape's own age n counts from 1 on its first tick; e is the ticks since the effect started.

use std::time::Instant;

use super::passes;
use super::soul::SquareDraw;
use super::tanji::bearing;
use super::walls::Wall;
use crate::world::{MeshVertex, Vector};

pub use super::soul::BLADE;
pub const FROST: &str = "data\\texture\\effect\\freeze_a.bmp";
const DISC_LIFE: u32 = 200;
const FROST_LIFE: u32 = 200;
const MOVER_LIFE: u32 = 200;
/// Where the mover's last launch is (its tick e = 198).
const MOVER_LAST: u32 = 198;
/// The blade's colour #5050FF and the frost's #8282FA.
const BLADE_TINT: [f32; 3] = [80.0 / 255.0, 80.0 / 255.0, 1.0];
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

struct Mover {
    start: Instant,
    done: u32,
    age: i32,
    from: Vector,
    aim: Vector,
    heading: f32,
    at: Vector,
}

#[derive(Default)]
pub struct Linker {
    discs: Vec<Disc>,
    frosts: Vec<Frost>,
    movers: Vec<Mover>,
    /// The places the mover launches 556 at, since the last ask (556 has no look yet).
    launched: Vec<Vector>,
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
    fn new(from: Vector, target: Option<u32>, sense: i32, now: Instant) -> Self {
        let at = [from[0], from[1] + 10.0, from[2]];
        let elements = std::array::from_fn(|k| Element { at, spin: k as f32, alpha: 0, dead: false });
        Self { start: now, target, sense, done: 0, age: 0, home: from, heading: 0, state: State::Homing, elements }
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
        self.discs.push(Disc::new(from, target, 0, now));
        self.discs.push(Disc::new(from, target, 1, now));
    }

    /// The frost picture on `unit`.
    pub fn frost(&mut self, unit: Option<u32>, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        // The one draw the game makes for the square's angle is thrown away.
        let _ = rand(360);
        self.frosts.push(Frost { unit, start: now, done: 0, alpha: 0, radius: 10.0, wobble: 0.0 });
    }

    /// Estun's invisible mover from the caster's feet toward `aim`; the four heights from `rand(n)`.
    pub fn mover(&mut self, from: Vector, aim: Vector, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        // ponytail: the other three elements move but nothing reads them, so only their four draws are made.
        let heights = [(); 4].map(|()| 10.0 - rand(5) as f32);
        let at = [from[0], from[1] + heights[0], from[2]];
        self.movers.push(Mover { start: now, done: 0, age: -3, from, aim, heading: 0.0, at });
    }

    /// The places 556 was launched at since this was last asked.
    pub fn take_launches(&mut self) -> Vec<Vector> {
        std::mem::take(&mut self.launched)
    }

    /// Runs the discs, the frost and the movers up to `now`, a tick at a time, and drops what ended.
    pub fn advance(&mut self, now: Instant, position_of: impl Fn(Option<u32>) -> Option<Vector>) {
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
            while mover.done < passes(mover.start, now).min(MOVER_LIFE) {
                let e = mover.done;
                mover.done += 1;
                mover.step();
                if e <= MOVER_LAST {
                    self.launched.push(mover.at);
                }
            }
        }
        self.movers.retain(|mover| passes(mover.start, now) <= MOVER_LIFE);
    }

    /// The discs and their trails: flat blue squares added on.
    pub fn meshes(&self) -> Vec<Wall> {
        let mut vertices = Vec::new();
        for disc in &self.discs {
            for element in disc.elements.iter().filter(|element| element.alpha > 0) {
                let tint = [BLADE_TINT[0], BLADE_TINT[1], BLADE_TINT[2], element.alpha as f32 / 255.0];
                let corner = |angle: f32, uv: [f32; 2]| {
                    let (sin, cos) = (element.spin + angle).to_radians().sin_cos();
                    MeshVertex { position: [element.at[0] + 25.0 * cos, element.at[1], element.at[2] + 25.0 * sin], uv, tint }
                };
                let [p0, p1, p2, p3] = [corner(90.0, [0.0, 1.0]), corner(180.0, [1.0, 1.0]), corner(270.0, [1.0, 0.0]), corner(360.0, [0.0, 0.0])];
                vertices.extend([p0, p1, p2, p0, p2, p3]);
            }
        }
        if vertices.is_empty() {
            return Vec::new();
        }
        vec![Wall { texture: BLADE, vertices, additive: true, forced: false }]
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
        let mut disc = Disc::new(FEET, Some(1), sense, Instant::now());
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
        linker.advance(at(0), |_| Some([300.0, 3.0, 100.0]));
        assert!(linker.meshes().is_empty());
        linker.advance(at(30), |_| Some([300.0, 3.0, 100.0]));
        let walls = linker.meshes();
        assert_eq!(walls.len(), 1);
        let wall = &walls[0];
        assert!(wall.additive && wall.texture == BLADE && !wall.forced);
        assert_eq!(wall.vertices.len() % 12, 0);
        assert!(wall.vertices.iter().all(|vertex| vertex.tint[..3] == BLADE_TINT));
        // Corner distance 25 from the element's place.
        let first = wall.vertices[0].position;
        assert!(near(f32::hypot(first[0] - linker.discs[0].elements[0].at[0], first[2] - linker.discs[0].elements[0].at[2]), 25.0));
        linker.advance(at(201), |_| None);
        assert!(linker.discs.is_empty());
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
        linker.advance(start + TICK * 39, feet);
        assert!(linker.squares(feet).is_empty());
        // The 41st tick (n = 41): alpha 8, 10.7 across, the wobble 2n.
        linker.advance(start + TICK * 40, feet);
        let draw = linker.squares(feet)[0];
        assert_eq!((draw.texture, draw.alpha, draw.additive), (FROST, 8.0, true));
        assert_eq!(draw.at, [50.0, 12.0, 60.0]);
        assert!(near(draw.half, 10.7 + 10.7 * 82.0_f32.to_radians().sin() * 0.05));
        assert_eq!(draw.rgb, FROST_TINT);
        linker.advance(start + TICK * 81, feet);
        assert!(linker.squares(feet).is_empty() && linker.frosts[0].wobble == 164.0);
        // It ends with the effect.
        linker.advance(start + TICK * 201, feet);
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
        // Nothing moves until n = 1 (the fourth tick, e = 3): then 0.8 a tick toward the aim.
        linker.advance(start + TICK * 2, |_| None);
        let still = linker.take_launches();
        assert!(still.len() == 3 && still.iter().all(|at| *at == [100.0, 13.0, 100.0]));
        linker.advance(start + TICK * 3, |_| None);
        let launches = linker.take_launches();
        assert_eq!(launches.len(), 1);
        assert!(near(launches[0][0], 100.8) && near(launches[0][2], 100.0));
        // Ten more ticks: 8.8 units out, still at the height of the first element.
        linker.advance(start + TICK * 13, |_| None);
        let launches = linker.take_launches();
        assert_eq!(launches.len(), 10);
        assert!(near(launches[9][0], 108.8) && launches[9][1] == 13.0);
        // One launch each tick e = 0 to 198, none after.
        linker.advance(start + TICK * 250, |_| None);
        assert_eq!(linker.take_launches().len(), 198 - 13);
        assert!(linker.movers.is_empty());
        assert!(linker.meshes().is_empty() && linker.squares(|_| None).is_empty());
    }
}
