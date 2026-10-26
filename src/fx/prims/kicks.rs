//! The Taekwon kicks' Hit Line 4, 5 and 6 (429, 430, 431), Running's footprints (442 to 448) and the eight fins of its stop (444), a
//! 16 ms tick at a time.
//!
//! A shape's own age n counts from 1 on its first tick; e is the ticks since the effect started.

use std::time::Instant;

use super::passes;
use super::tanji::bearing;
use super::walls::Wall;
use crate::world::{MeshVertex, Vector};

pub const LINE: &str = super::brawl::WHITE;
pub const FIN: &str = "data\\texture\\effect\\magic_blue.tga";
/// Running's footprints: the left foot (447) and the right (448).
pub const FOOT_LEFT: &str = "data\\texture\\effect\\foot_l2.tga";
pub const FOOT_RIGHT: &str = "data\\texture\\effect\\foot_r2.tga";
/// Chase Walk's footprints: the left (326) and the right (327), pale grey.
pub const CHASE_LEFT: &str = "data\\texture\\effect\\foot_l_b.tga";
pub const CHASE_RIGHT: &str = "data\\texture\\effect\\foot_r_b.tga";
/// Chase Walk's print is 5 across to the corner on the ground (4.5 in all), drawn once.
const CHASE_RADIUS: f32 = 0.9 * 5.0;
/// A footprint lasts 340 ticks, 6 across to the corner on the ground and 0.2 up.
const PRINT_LIFE: u32 = 340;
const PRINT_RADIUS: f32 = 0.9 * 6.0;
const PRINT_UP: f32 = 0.2;
const LINE_LIFE: u32 = 200;
const FIN_LIFE: u32 = 100;
/// Running's start lasts 9 999 ticks.
const RUN_LIFE: u32 = 9999;
/// The tube round a line runs 5 up from the anchor.
const TUBE_UP: f32 = 5.0;

/// Which Hit Line: Turn Kick's and Estun's (429), Counter Kick's and Flash Kick's (430), Down Kick's, Esma's and Swhoo's (431).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Turn,
    Counter,
    Down,
}

impl Kind {
    /// The ticks of the effect a prim of four lines is made on.
    fn ticks(self) -> std::ops::RangeInclusive<u32> {
        match self {
            Kind::Turn => 6..=9,
            Kind::Counter => 0..=2,
            Kind::Down => 0..=4,
        }
    }

    /// The tube radius and the trail length of its lines.
    fn tube(self) -> (f32, usize) {
        match self {
            Kind::Turn => (0.05, 4),
            Kind::Counter => (0.06, 3),
            Kind::Down => (0.07, 4),
        }
    }
}

/// One arcing line: its age (negative while it waits), alpha, heading, reach, height factor, angle and the last angles.
pub struct Line {
    pub age: i32,
    pub alpha: i32,
    start_alpha: i32,
    phi: f32,
    reach: f32,
    k: f32,
    theta: f32,
    history: Vec<f32>,
}

struct Hit {
    kind: Kind,
    /// The target the lines follow, and where the attacker stood (Counter Kick's heading).
    unit: Option<u32>,
    from: Option<Vector>,
    at: Vector,
    start: Instant,
    done: u32,
    lines: Vec<Line>,
}

/// One of Running's stop fins.
pub struct Fin {
    pub age: u32,
    pub alpha: i32,
    pub height: f32,
    pub inner: f32,
    pub length: f32,
    pub turn: f32,
    /// Kaizel's second mode: it grows by 2 and 0.4 and starts at 1.5 up.
    kaizel: bool,
}

struct Fins {
    unit: Option<u32>,
    start: Instant,
    fins: Vec<Fin>,
    done: u32,
}

/// A run (Running's start): the ticks counted and where the unit was at the last check.
struct Run {
    unit: Option<u32>,
    start: Instant,
    c: u32,
    last: Option<Vector>,
    done: u32,
}

/// A footprint Running launched (effects 447 and 448): which, where, which way and when.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Footprint {
    pub effect: u32,
    pub at: Vector,
    pub heading: f32,
    pub start: Instant,
}

#[derive(Default)]
pub struct Kicks {
    hits: Vec<Hit>,
    fins: Vec<Fins>,
    runs: Vec<Run>,
    /// The units that have Running's status on (443).
    running: Vec<Option<u32>>,
    /// The footprints on the ground now.
    pub prints: Vec<Footprint>,
}

/// A print's picture, the radius of its corners and how many times it is drawn, by the effect that left it.
fn print_look(effect: u32) -> (&'static str, f32, usize) {
    match effect {
        326 => (CHASE_LEFT, CHASE_RADIUS, 1),
        327 => (CHASE_RIGHT, CHASE_RADIUS, 1),
        448 => (FOOT_RIGHT, PRINT_RADIUS, 2),
        _ => (FOOT_LEFT, PRINT_RADIUS, 2),
    }
}

/// A footprint's alpha at its age n: 150 less 1 for every other tick (0 at 299).
pub fn print_alpha(n: u32) -> i32 {
    (150 - n.div_ceil(2) as i32).max(0)
}

/// A line's alpha at its age n for a start alpha: held to 35, then 5 less a tick.
pub fn line_alpha(start: i32, n: u32) -> i32 {
    (start - 5 * n.saturating_sub(35) as i32).max(0)
}

impl Line {
    fn new(kind: Kind, from: Option<Vector>, at: Vector, rand: &mut impl FnMut(u32) -> u32) -> Self {
        let delay = match kind {
            Kind::Turn => -(rand(31) as i32),
            Kind::Counter => -25 - rand(21) as i32,
            Kind::Down => -30 - rand(41) as i32,
        };
        let alpha = 125 + rand(121) as i32;
        let phi = match kind {
            Kind::Counter => {
                let way = from.map_or(180, |from| bearing(at[0] - from[0], at[2] - from[2]));
                (way + rand(121) as i32 - 60).rem_euclid(360) as f32
            }
            _ => rand(360) as f32,
        };
        let reach = 6.0 + if kind == Kind::Counter { rand(7) } else { rand(6) } as f32;
        let k = if kind == Kind::Down { 0.6 } else { 0.8 } + 0.1 * rand(7) as f32;
        let (_, trail) = kind.tube();
        Self { age: delay, alpha, start_alpha: alpha, phi, reach, k, theta: 0.0, history: vec![0.0; trail + 1] }
    }

    fn step(&mut self) {
        self.age += 1;
        if self.age <= 0 {
            return;
        }
        self.alpha = line_alpha(self.start_alpha, self.age as u32);
        self.theta = (self.theta + 6.0).min(180.0);
        // Entry 0 takes the angle, then each entry takes the one before: entries 0 and 1 are the angle, 2 the angle less 6.
        self.history[0] = self.theta;
        for i in (1..self.history.len()).rev() {
            self.history[i] = self.history[i - 1];
        }
    }

    /// The arc point at the angle t over the anchor.
    fn point(&self, anchor: Vector, t: f32) -> Vector {
        let along = if t <= 0.0 { 0.0 } else { t / 90.0 };
        let (sin, cos) = self.phi.to_radians().sin_cos();
        [anchor[0] + along * self.reach * cos, anchor[1] + self.reach * self.k * t.to_radians().sin(), anchor[2] + along * self.reach * sin]
    }
}

impl Fin {
    /// The fin of prim turn `prim` and element `k`.
    fn new(prim: f32, k: u32) -> Self {
        Self { age: 0, alpha: 0, height: 0.0, inner: 1.0, length: 2.0, turn: 90.0 * k as f32 + prim, kaizel: false }
    }

    /// Kaizel's fin: 1.5 up to start, in the second mode.
    fn kaizel(prim: f32, k: u32) -> Self {
        Self { height: 1.5, kaizel: true, ..Self::new(prim, k) }
    }

    /// One tick: it flies out, grows 0.2 up to 2, brightens for 5 ticks and fades once its inner edge passes 10.
    pub fn step(&mut self) {
        self.age += 1;
        self.inner += 1.0;
        if self.kaizel {
            // Kaizel's: the outer edge goes twice as fast, up to 4, brightening for 7 ticks.
            if self.inner > 10.0 {
                self.alpha -= 2;
            }
            self.length += 2.0;
            if self.height < 4.0 {
                self.height += 0.4;
            }
            if self.age <= 7 {
                self.alpha += 5;
            }
            return;
        }
        self.length += 1.0;
        if self.height < 2.0 {
            self.height += 0.2;
        }
        if self.age <= 5 {
            self.alpha += 5;
        }
        if self.inner > 10.0 {
            self.alpha -= 2;
        }
    }
}

/// One tube round a segment: ring radius `radius`, colour and the alpha it has, between the points `a` and `b`.
fn tube(a: Vector, b: Vector, radius: f32, rgb: [f32; 3], alpha: f32) -> Vec<MeshVertex> {
    let psi = (bearing(a[0] - b[0], a[2] - b[2]) as f32 + 90.0).rem_euclid(360.0);
    let (psi_sin, psi_cos) = psi.to_radians().sin_cos();
    let ring = |angle: f32| {
        let (sin, cos) = angle.to_radians().sin_cos();
        [radius * cos * psi_cos, TUBE_UP - radius * sin, radius * cos * psi_sin]
    };
    let tint = [rgb[0], rgb[1], rgb[2], alpha / 255.0];
    let at =
        |point: Vector, offset: Vector, uv: [f32; 2]| MeshVertex { position: [point[0] + offset[0], point[1] + offset[1], point[2] + offset[2]], uv, tint };
    let mut vertices = Vec::new();
    for (previous, angle) in [(0.0, 120.0), (120.0, 240.0), (240.0, 360.0)] {
        let [p0, p1] = [ring(previous), ring(angle)];
        let [q0, q1, q2, q3] = [at(a, p1, [0.0, 1.0]), at(b, p1, [1.0, 1.0]), at(b, p0, [1.0, 0.0]), at(a, p0, [0.0, 0.0])];
        vertices.extend([q0, q1, q2, q0, q2, q3]);
    }
    vertices
}

impl Kicks {
    /// A Hit Line of `kind` on the target `unit`, standing at `at`; Counter Kick's is told where the attacker `from` stood.
    pub fn hit(&mut self, kind: Kind, unit: Option<u32>, from: Option<Vector>, at: Vector, now: Instant) {
        self.hits.push(Hit { kind, unit, from, at, start: now, done: 0, lines: Vec::new() });
    }

    /// Running's stop on `unit`: two prims of four fins.
    pub fn stop(&mut self, unit: Option<u32>, now: Instant) {
        let fins = [0.0, 45.0].iter().flat_map(|&prim| (0..4).map(move |k| Fin::new(prim, k))).collect();
        self.fins.push(Fins { unit, start: now, fins, done: 0 });
    }

    /// Kaizel's eight fins on `unit`, shooting out 15 ticks after `now`.
    pub fn kaizel(&mut self, unit: Option<u32>, now: Instant) {
        let fins = [0.0, 45.0].iter().flat_map(|&prim| (0..4).map(move |k| Fin::kaizel(prim, k))).collect();
        self.fins.push(Fins { unit, start: now + super::TICK * 15, fins, done: 0 });
    }

    /// Makes the lines whose ticks have come (the target's place then from `position_of`), runs everything a tick at a time and drops
    /// what ended. The choices of each line come from `rand(n)`.
    pub fn advance(&mut self, now: Instant, position_of: impl Fn(Option<u32>) -> Option<Vector>, mut rand: impl FnMut(u32) -> u32) {
        for hit in &mut self.hits {
            hit.at = position_of(hit.unit).unwrap_or(hit.at);
            while hit.done < passes(hit.start, now).min(LINE_LIFE) {
                if hit.kind.ticks().contains(&hit.done) {
                    for _ in 0..4 {
                        hit.lines.push(Line::new(hit.kind, hit.from, hit.at, &mut rand));
                    }
                }
                for line in &mut hit.lines {
                    line.step();
                }
                hit.done += 1;
            }
        }
        self.hits.retain(|hit| passes(hit.start, now) <= LINE_LIFE);
        for run in &mut self.runs {
            while run.done < passes(run.start, now).min(RUN_LIFE) {
                let e = run.done;
                run.done += 1;
                run.c += 1;
                if run.c == 7 || run.c == 14 {
                    let place = position_of(run.unit);
                    if let (Some(place), Some(last)) = (place, run.last)
                        && place != last
                        && e > 7
                    {
                        let heading = (bearing(place[0] - last[0], place[2] - last[2]) as f32 + 135.0).rem_euclid(360.0);
                        let effect = if run.c == 7 { 447 } else { 448 };
                        self.prints.push(Footprint { effect, at: place, heading, start: now });
                    }
                    run.last = place.or(run.last);
                    if run.c == 14 {
                        run.c = 1;
                    }
                }
            }
        }
        self.runs.retain(|run| passes(run.start, now) <= RUN_LIFE);
        self.prints.retain(|print| passes(print.start, now) <= PRINT_LIFE);
        for fins in &mut self.fins {
            while fins.done < passes(fins.start, now).min(FIN_LIFE) {
                fins.done += 1;
                for fin in &mut fins.fins {
                    fin.step();
                }
            }
        }
        self.fins.retain(|fins| passes(fins.start, now) <= FIN_LIFE);
    }

    /// Whether Running's status (133, effect 443) is on `unit`.
    pub fn is_running(&self, unit: Option<u32>) -> bool {
        self.running.contains(&unit)
    }

    /// Running's status comes on (`on`) or goes off; either way its start ends.
    pub fn status(&mut self, unit: Option<u32>, on: bool) {
        self.running.retain(|each| *each != unit);
        self.runs.retain(|run| run.unit != unit);
        if on {
            self.running.push(unit);
        }
    }

    /// Running's start (442) on `unit`: footprints every 7 ticks from the 8th. It does not start with another running.
    pub fn run(&mut self, unit: Option<u32>, now: Instant) {
        if !self.runs.iter().any(|run| run.unit == unit) {
            self.runs.push(Run { unit, start: now, c: 0, last: None, done: 0 });
        }
    }

    /// A Chase Walk print (326 left, 327 right) at `at`, turned `heading` degrees.
    pub fn chase_print(&mut self, effect: u32, at: Vector, heading: f32, now: Instant) {
        self.prints.push(Footprint { effect, at, heading, start: now });
    }

    /// The footprints' squares at `now`: a flat print 0.2 up, fading from 150 by 1 every other tick (gone at 4.8 s), blended: Running's
    /// dark grey ones drawn twice, Chase Walk's pale ones once.
    pub fn print_meshes(&self, now: Instant) -> Vec<Wall> {
        let mut walls = Vec::new();
        for texture in [FOOT_LEFT, FOOT_RIGHT, CHASE_LEFT, CHASE_RIGHT] {
            let mut vertices = Vec::new();
            for print in self.prints.iter().filter(|print| print_look(print.effect).0 == texture) {
                let (_, radius, copies) = print_look(print.effect);
                let alpha = print_alpha(passes(print.start, now));
                if alpha <= 0 {
                    continue;
                }
                let tint = [1.0, 1.0, 1.0, alpha as f32 / 255.0];
                let corner = |j: u32, uv: [f32; 2]| {
                    let (sin, cos) = (print.heading + 90.0 * j as f32).to_radians().sin_cos();
                    MeshVertex { position: [print.at[0] + radius * cos, print.at[1] + PRINT_UP, print.at[2] + radius * sin], uv, tint }
                };
                let [a, b, c, d] = [corner(0, [0.0, 1.0]), corner(1, [1.0, 1.0]), corner(2, [1.0, 0.0]), corner(3, [0.0, 0.0])];
                for _ in 0..copies {
                    vertices.extend([a, b, c, a, c, d]);
                }
            }
            if !vertices.is_empty() {
                walls.push(Wall { texture, vertices, additive: false, forced: false });
            }
        }
        walls
    }

    /// The arc lines' tubes at `now` over their targets, white-grey with a blue glow, added on.
    pub fn hit_meshes(&self) -> Vec<Wall> {
        let mut vertices = Vec::new();
        for hit in &self.hits {
            let (w, _) = hit.kind.tube();
            for line in hit.lines.iter().filter(|line| line.age > 0 && line.alpha > 0) {
                for j in 0..line.history.len() - 1 {
                    let (a, b) = (line.history[j], line.history[j + 1]);
                    if a == b {
                        continue;
                    }
                    let (pa, pb) = (line.point(hit.at, a), line.point(hit.at, b));
                    let alpha = line.alpha as f32 - 10.0 * j as f32;
                    let grey = (105.0 + 30.0 * j as f32) / 255.0;
                    if alpha > 0.0 {
                        vertices.extend(tube(pa, pb, w, [grey; 3], alpha));
                    }
                    let glow = ((alpha) / 2.0).floor();
                    if glow > 0.0 {
                        vertices.extend(tube(pa, pb, 4.0 * w, [0.0, 0.0, 1.0], glow));
                    }
                }
            }
        }
        if vertices.is_empty() { Vec::new() } else { vec![Wall { texture: LINE, vertices, additive: true, forced: false }] }
    }

    /// The fins at `now` over their units' feet (`position_of`): per fin three copies 1° apart, the middle one the brightest.
    pub fn fin_meshes(&self, position_of: impl Fn(Option<u32>) -> Option<Vector>) -> Vec<Wall> {
        let mut vertices = Vec::new();
        for fins in &self.fins {
            let Some(feet) = position_of(fins.unit) else { continue };
            for fin in fins.fins.iter().filter(|fin| fin.alpha > 0) {
                for (pass, (rise, tip)) in [(0.1, 0.4), (0.2, 1.0), (0.1, 0.4)].into_iter().enumerate() {
                    let angle = (fin.turn - 1.0 + pass as f32).rem_euclid(360.0);
                    let (sin, cos) = angle.to_radians().sin_cos();
                    let alpha = if pass == 1 { fin.alpha as f32 } else { (fin.alpha as f32 / 3.0).floor() } / 255.0;
                    let tint = [1.0, 1.0, 1.0, alpha];
                    let outer = fin.inner + fin.length;
                    let point =
                        |reach: f32, up: f32, uv: [f32; 2]| MeshVertex { position: [feet[0] + reach * cos, feet[1] + up, feet[2] + reach * sin], uv, tint };
                    let [p0, p1, p2, p3] = [
                        point(fin.inner, 0.0, [0.0, 1.0]),
                        point(fin.inner, rise * fin.height, [1.0, 1.0]),
                        point(outer, 0.0, [1.0, 0.0]),
                        point(outer, tip * fin.height, [0.0, 0.0]),
                    ];
                    vertices.extend([p0, p1, p2, p0, p2, p3]);
                }
            }
        }
        if vertices.is_empty() { Vec::new() } else { vec![Wall { texture: FIN, vertices, additive: true, forced: false }] }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prims::TICK;

    fn near(got: f32, want: f32) -> bool {
        (got - want).abs() < 1e-3
    }

    fn made(kind: Kind, ticks: u32, rand: impl FnMut(u32) -> u32) -> Kicks {
        let start = Instant::now();
        let mut kicks = Kicks::default();
        kicks.hit(kind, Some(1), Some([0.0; 3]), [10.0, 1.0, 10.0], start);
        let mut rand = rand;
        for tick in 0..=ticks {
            kicks.advance(start + TICK * tick, |_| Some([10.0, 1.0, 10.0]), &mut rand);
        }
        kicks
    }

    #[test]
    fn the_hit_lines_are_made_four_at_a_time_on_their_ticks() {
        // 429 on e = 6 … 9 (16 lines), 430 on 0 … 2 (12), 431 on 0 … 4 (20), none the tick after.
        for (kind, last, lines) in [(Kind::Turn, 9, 16), (Kind::Counter, 2, 12), (Kind::Down, 4, 20)] {
            assert_eq!(made(kind, last, |_| 0).hits[0].lines.len(), lines, "{kind:?}");
            assert_eq!(made(kind, last + 5, |_| 0).hits[0].lines.len(), lines);
        }
        assert_eq!(made(Kind::Turn, 5, |_| 0).hits[0].lines.len(), 0);
        // Every draw 0: delays 0, −25, −30; alpha 125; reach 6; height factor 0.8, 0.8, 0.6; heading as drawn (0).
        let low: Vec<(i32, i32, f32, f32)> = [Kind::Turn, Kind::Counter, Kind::Down]
            .map(|kind| {
                let line = Line::new(kind, None, [0.0; 3], &mut |_| 0);
                (line.age, line.start_alpha, line.reach, line.k)
            })
            .to_vec();
        assert_eq!(low, [(0, 125, 6.0, 0.8), (-25, 125, 6.0, 0.8), (-30, 125, 6.0, 0.6)]);
        // The largest draws: delay −30, −45, −70; alpha 245; reach 11, 12, 11; k 1.4, 1.4, 1.2.
        let high = |kind: Kind| Line::new(kind, None, [0.0; 3], &mut |n| n - 1);
        assert_eq!([Kind::Turn, Kind::Counter, Kind::Down].map(|kind| high(kind).age), [-30, -45, -70]);
        assert_eq!([Kind::Turn, Kind::Counter, Kind::Down].map(|kind| high(kind).reach), [11.0, 12.0, 11.0]);
        assert!([Kind::Turn, Kind::Counter, Kind::Down].iter().zip([1.4, 1.4, 1.2]).all(|(kind, k)| near(high(*kind).k, k)));
        assert_eq!(high(Kind::Turn).start_alpha, 245);
        // Counter Kick's heading is the bearing of the hit unit from the attacker, + R − 60: attacker 10 west of the unit, so 0 ± 60.
        let counter = Line::new(Kind::Counter, Some([0.0, 0.0, 10.0]), [10.0, 0.0, 10.0], &mut |n| if n == 121 { 60 } else { 0 });
        assert_eq!(counter.phi, 0.0, "bearing 0 + 60 − 60");
    }

    #[test]
    fn a_line_sweeps_its_arc_and_fades() {
        let mut line = Line::new(Kind::Turn, None, [0.0; 3], &mut |_| 0);
        line.start_alpha = 245;
        let mut thetas = Vec::new();
        for age in 1..=31 {
            line.step();
            if [1, 29, 30, 31].contains(&age) {
                thetas.push(line.theta);
            }
        }
        assert_eq!(thetas, [6.0, 174.0, 180.0, 180.0]);
        // The history after 3 ticks for m = 4: 18, 18, 12, 6, 0.
        let mut trail = Line::new(Kind::Turn, None, [0.0; 3], &mut |_| 0);
        for _ in 0..3 {
            trail.step();
        }
        assert_eq!(trail.history, [18.0, 18.0, 12.0, 6.0, 0.0]);
        // Alpha of a line made at 245 at ages 35, 36, 84, 85: 245, 240, 0, 0 (5 less a tick from age 36).
        assert_eq!([35, 36, 83, 84, 85].map(|age| line_alpha(245, age)), [245, 240, 5, 0, 0]);
        // A waiting line does nothing.
        let mut waiting = Line::new(Kind::Down, None, [0.0; 3], &mut |_| 0);
        waiting.step();
        assert_eq!((waiting.theta, waiting.age), (0.0, -29));
    }

    #[test]
    fn each_segment_is_two_tubes_grey_and_blue() {
        let start = Instant::now();
        let mut kicks = Kicks::default();
        kicks.hit(Kind::Down, Some(1), None, [10.0, 1.0, 10.0], start);
        for tick in 0..=40 {
            kicks.advance(start + TICK * tick, |_| Some([10.0, 1.0, 10.0]), |_| 0);
        }
        // Delays −30: a line is up from tick 31; each of 20 lines has segment 0 (nothing) and segments 1 … 3.
        let walls = kicks.hit_meshes();
        assert_eq!(walls.len(), 1);
        assert!(walls[0].additive && walls[0].texture == LINE);
        // Grey 105 + 30j on all channels with alpha − 10j, then the blue tube at #0000FF with half of it.
        let first = &walls[0].vertices;
        assert_eq!(first[0].tint[..3], [(105.0 + 30.0) / 255.0; 3]);
        assert!(first.iter().any(|vertex| vertex.tint[..3] == [0.0, 0.0, 1.0]));
        // The blue tube's alpha is half the grey one's, rounded down (the segment j = 1 here: the alpha less 10).
        let alpha = |vertex: &MeshVertex| (vertex.tint[3] * 255.0).round();
        let blue = first.iter().find(|vertex| vertex.tint[..3] == [0.0, 0.0, 1.0]).unwrap();
        assert_eq!(alpha(blue), ((alpha(&first[0])) / 2.0).floor());
        // Not drawn before any line is up; the lines follow the target.
        assert!(made(Kind::Down, 10, |_| 0).hit_meshes().is_empty());
        let moved = kicks.hit_meshes();
        assert!(moved[0].vertices.iter().all(|vertex| vertex.position[0] < 40.0));
        kicks.advance(start + TICK * 300, |_| None, |_| 0);
        assert!(kicks.hits.is_empty());
    }

    #[test]
    fn running_leaves_footprints_every_seven_ticks_once_it_has_moved() {
        let start = Instant::now();
        let mut kicks = Kicks::default();
        kicks.run(Some(1), start);
        kicks.run(Some(1), start);
        assert_eq!(kicks.runs.len(), 1, "no second start");
        let place = std::cell::Cell::new([0.0, 0.0, 0.0]);
        let mut steps = Vec::new();
        for tick in 0..=40 {
            place.set([tick as f32, 0.0, 0.0]);
            kicks.advance(start + TICK * tick, |_| Some(place.get()), |_| 0);
        }
        steps.extend(kicks.prints.clone());
        // The first check (c = 7) only remembers the place and e is not over 7; then 448 at c = 14, 447 at c = 7 again (c goes back to 1).
        assert!(steps.iter().all(|step| step.heading == (bearing(1.0, 0.0) as f32 + 135.0) % 360.0));
        assert_eq!(steps.iter().map(|step| step.effect).collect::<Vec<_>>(), [448, 447, 448, 447, 448]);
        // Standing still: none.
        let mut idle = Kicks::default();
        idle.run(None, start);
        for tick in 0..=40 {
            idle.advance(start + TICK * tick, |_| Some([0.0; 3]), |_| 0);
        }
        assert!(idle.prints.is_empty());
        // The status takes a start away and a use with the status on is a stop.
        kicks.status(Some(1), true);
        assert!(kicks.runs.is_empty() && kicks.is_running(Some(1)) && !kicks.is_running(Some(2)));
        kicks.status(Some(1), false);
        assert!(!kicks.is_running(Some(1)));
    }

    #[test]
    fn running_stops_with_eight_fins() {
        let start = Instant::now();
        let feet = [10.0, 1.0, 20.0];
        let mut kicks = Kicks::default();
        kicks.stop(Some(1), start);
        let fins = &kicks.fins[0].fins;
        assert_eq!(fins.len(), 8);
        let mut turns: Vec<f32> = fins.iter().map(|fin| fin.turn).collect();
        turns.sort_by(f32::total_cmp);
        assert_eq!(turns, [0.0, 45.0, 90.0, 135.0, 180.0, 225.0, 270.0, 315.0]);
        // r at n = 1, 10, 11, 22: 2, 11, 12, 23; alpha at n = 1, 5, 9, 10, 21, 22: 5, 25, 25, 23, 1, gone; h at n = 10, 11: 2.
        let mut fin = Fin::new(0.0, 0);
        let mut seen = Vec::new();
        for n in 1..=22 {
            fin.step();
            seen.push((n, fin.inner, fin.alpha, fin.height));
        }
        let at = |n: usize| seen[n - 1];
        assert_eq!([1, 10, 11, 22].map(|n| at(n).1), [2.0, 11.0, 12.0, 23.0]);
        assert_eq!([1, 5, 9, 10, 21, 22].map(|n| at(n).2), [5, 25, 25, 23, 1, -1]);
        assert!(near(at(10).3, 2.0) && near(at(11).3, 2.0));
        // Three passes a fin at turn − 1, turn, turn + 1 with (c, d) (0.1, 0.4), (0.2, 1.0), (0.1, 0.4); alphas a/3, a, a/3.
        for tick in 0..=9 {
            kicks.advance(start + TICK * tick, |_| Some(feet), |_| 0);
        }
        let walls = kicks.fin_meshes(|_| Some(feet));
        assert_eq!(walls.len(), 1);
        assert!(walls[0].additive && walls[0].texture == FIN && walls[0].vertices.len() == 8 * 3 * 6);
        let alphas: Vec<f32> = (0..3).map(|pass| (walls[0].vertices[pass * 6].tint[3] * 255.0).round()).collect();
        assert_eq!(alphas, [7.0, 23.0, 7.0]);
        // It follows the caster and ends with the effect.
        assert!(kicks.fin_meshes(|_| None).is_empty());
        kicks.advance(start + TICK * 101, |_| Some(feet), |_| 0);
        assert!(kicks.fins.is_empty());
    }

    #[test]
    fn kaizel_shoots_eight_longer_fins_from_tick_15() {
        let start = Instant::now();
        let feet = [10.0, 1.0, 20.0];
        let mut kicks = Kicks::default();
        kicks.kaizel(Some(1), start);
        let fins = &kicks.fins[0].fins;
        assert_eq!(fins.len(), 8);
        let mut turns: Vec<f32> = fins.iter().map(|fin| fin.turn).collect();
        turns.sort_by(f32::total_cmp);
        assert_eq!(turns, [0.0, 45.0, 90.0, 135.0, 180.0, 225.0, 270.0, 315.0]);
        assert!(fins.iter().all(|fin| fin.height == 1.5 && fin.inner == 1.0 && fin.length == 2.0 && fin.alpha == 0));
        // Nothing is drawn before tick 15.
        kicks.advance(start + TICK * 14, |_| Some(feet), |_| 0);
        assert!(kicks.fin_meshes(|_| Some(feet)).is_empty());
        // r at n = 1, 10, 26: 2, 11, 27; s at n = 1, 26: 4, 54; h at n = 1, 6, 7, 8: 1.9, 3.9, 4.3, 4.3;
        // alpha at n = 1, 7, 8, 9, 10, 11, 26, 27: 5, 35, 35, 35, 33, 31, 1, -1.
        let mut fin = Fin::kaizel(0.0, 0);
        let mut seen = Vec::new();
        for _ in 1..=27 {
            fin.step();
            seen.push((fin.inner, fin.length, fin.height, fin.alpha));
        }
        let at = |n: usize| seen[n - 1];
        assert_eq!([1, 10, 26].map(|n| at(n).0), [2.0, 11.0, 27.0]);
        assert_eq!([1, 26].map(|n| at(n).1), [4.0, 54.0]);
        assert!([1, 6, 7, 8].iter().zip([1.9, 3.9, 4.3, 4.3]).all(|(&n, want)| near(at(n).2, want)));
        assert_eq!([1, 7, 8, 9, 10, 11, 26, 27].map(|n| at(n).3), [5, 35, 35, 35, 33, 31, 1, -1]);
        // Three passes at turn - 1, turn, turn + 1 with alphas a/3, a, a/3 at n = 7 (the tick e = 21).
        kicks.advance(start + TICK * 21, |_| Some(feet), |_| 0);
        let walls = kicks.fin_meshes(|_| Some(feet));
        assert!(walls[0].additive && walls[0].texture == FIN && walls[0].vertices.len() == 8 * 3 * 6);
        let alphas: Vec<f32> = (0..3).map(|pass| (walls[0].vertices[pass * 6].tint[3] * 255.0).round()).collect();
        assert_eq!(alphas, [11.0, 35.0, 11.0]);
        // It follows the target.
        assert!(kicks.fin_meshes(|_| None).is_empty());
    }

    #[test]
    fn a_chase_walk_print_is_one_pale_square_of_radius_4_5() {
        let start = Instant::now();
        let mut kicks = Kicks::default();
        kicks.chase_print(326, [10.0, 1.0, 20.0], 135.0, start);
        kicks.chase_print(327, [12.0, 1.0, 20.0], 0.0, start);
        let walls = kicks.print_meshes(start);
        // One wall for each picture, one blended white quad each, alpha 149 at n = 1, 0.2 up, radius 4.5 at the heading.
        assert_eq!(walls.iter().map(|wall| wall.texture).collect::<Vec<_>>(), [CHASE_LEFT, CHASE_RIGHT]);
        assert!(walls.iter().all(|wall| !wall.additive && wall.vertices.len() == 6));
        let first = &walls[0].vertices[0];
        assert_eq!(first.tint, [1.0, 1.0, 1.0, 149.0 / 255.0]);
        assert!((first.position[1] - 1.2).abs() < 1e-5);
        let (dx, dz) = (first.position[0] - 10.0, first.position[2] - 20.0);
        assert!((dx.hypot(dz) - 4.5).abs() < 1e-4 && (dz.atan2(dx).to_degrees() - 135.0).abs() < 1e-3);
        // Alpha at n = 1, 2, 3, 299, 300: 149, 149, 148, 0, 0; gone at tick 340, and Running's own prints are still twice.
        assert_eq!([1, 2, 3, 299, 300].map(print_alpha), [149, 149, 148, 0, 0]);
        kicks.advance(start + TICK * 341, |_| None, |_| 0);
        assert!(kicks.prints.is_empty());
        kicks.prints.push(Footprint { effect: 447, at: [0.0; 3], heading: 0.0, start });
        assert_eq!(kicks.print_meshes(start)[0].vertices.len(), 12);
    }

    #[test]
    fn a_footprint_lies_on_the_ground_and_fades_over_4_8_seconds() {
        let start = Instant::now();
        // Alpha at n = 1, 2, 3, 4, 297, 298, 299, 300: 149, 149, 148, 148, 1, 1, 0, 0.
        assert_eq!([1, 2, 3, 4, 297, 298, 299, 300].map(print_alpha), [149, 149, 148, 148, 1, 1, 0, 0]);
        let mut kicks = Kicks::default();
        kicks.prints.push(Footprint { effect: 447, at: [10.0, 1.0, 20.0], heading: 30.0, start });
        kicks.prints.push(Footprint { effect: 448, at: [12.0, 1.0, 20.0], heading: 0.0, start });
        let walls = kicks.print_meshes(start);
        // One wall for each foot's picture, two identical normal-blend quads each, white at the alpha, 0.2 up on radius 5.4.
        assert_eq!(walls.len(), 2);
        assert_eq!((walls[0].texture, walls[1].texture), (FOOT_LEFT, FOOT_RIGHT));
        assert!(walls.iter().all(|wall| !wall.additive && wall.vertices.len() == 12));
        let vertices = &walls[0].vertices;
        assert!(vertices[..6].iter().zip(&vertices[6..]).all(|(a, b)| a.position == b.position && a.uv == b.uv && a.tint == b.tint));
        assert_eq!(vertices[0].tint, [1.0, 1.0, 1.0, 149.0 / 255.0]);
        assert!((vertices[0].position[1] - 1.2).abs() < 1e-5);
        let (dx, dz) = (vertices[0].position[0] - 10.0, vertices[0].position[2] - 20.0);
        assert!((dx.hypot(dz) - 5.4).abs() < 1e-4 && (dz.atan2(dx).to_degrees() - 30.0).abs() < 1e-3);
        assert_eq!(vertices[0].uv, [0.0, 1.0]);
        // Nothing once the alpha is 0, and the print is gone after tick 340; it never follows anything.
        assert!(kicks.print_meshes(start + TICK * 298).is_empty());
        kicks.advance(start + TICK * 341, |_| Some([99.0; 3]), |_| 0);
        assert!(kicks.prints.is_empty());
    }
}
