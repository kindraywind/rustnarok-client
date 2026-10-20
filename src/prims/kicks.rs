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

/// A footprint Running launched (effects 447 and 448 have no look yet): which, where, which way and when.
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
    footprints: Vec<Footprint>,
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
        Self { age: 0, alpha: 0, height: 0.0, inner: 1.0, length: 2.0, turn: 90.0 * k as f32 + prim }
    }

    /// One tick: it flies out, grows 0.2 up to 2, brightens for 5 ticks and fades once its inner edge passes 10.
    pub fn step(&mut self) {
        self.age += 1;
        self.inner += 1.0;
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
                        self.footprints.push(Footprint { effect, at: place, heading, start: now });
                    }
                    run.last = place.or(run.last);
                    if run.c == 14 {
                        run.c = 1;
                    }
                }
            }
        }
        self.runs.retain(|run| passes(run.start, now) <= RUN_LIFE);
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

    /// The footprints launched since this was last asked.
    pub fn take_footprints(&mut self) -> Vec<Footprint> {
        std::mem::take(&mut self.footprints)
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
            steps.extend(kicks.take_footprints());
        }
        // The first check (c = 7) only remembers the place and e is not over 7; then 448 at c = 14, 447 at c = 7 again (c goes back to 1).
        assert!(steps.iter().all(|step| step.heading == (bearing(1.0, 0.0) as f32 + 135.0) % 360.0));
        assert_eq!(steps.iter().map(|step| step.effect).collect::<Vec<_>>(), [448, 447, 448, 447, 448]);
        // Standing still: none.
        let mut idle = Kicks::default();
        idle.run(None, start);
        for tick in 0..=40 {
            idle.advance(start + TICK * tick, |_| Some([0.0; 3]), |_| 0);
        }
        assert!(idle.take_footprints().is_empty());
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
}
