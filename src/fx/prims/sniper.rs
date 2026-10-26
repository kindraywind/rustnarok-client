//! The Sniper's, Professor's and Wanderer's skills' code-made shapes, a 16 ms tick at a time: True Sight's 48 rays (386), Sharp
//! Shooting's and Arrow Vulcan's faint lines along the shot (388, 393), Wind Walk's four cloudy arcs (389), and the body
//! colours Soul Change, Falcon Assault, Wind Walk and Reject Sword give (385, 387, 389, 392).
//!
//! A shape's own age e counts from 1 at its first tick; t is the ticks since the effect started.

use std::time::Instant;

use super::passes;
use super::tanji::bearing;
use super::walls::Wall;
use crate::world::{MeshVertex, Vector};

pub const RAY: &str = "data\\texture\\effect\\alpha_center.tga";
pub const CLOUD: &str = "data\\texture\\effect\\cloud11.tga";
/// True Sight: 12 prims of 4 elements, drawn on ticks 0 to 69.
const RAY_COUNT: usize = 48;
const RAY_TICKS: u32 = 70;
const RAY_TINT: [f32; 3] = [250.0 / 255.0; 3];
/// The lines: 388 makes a prim on each of ticks 0 to 3, 393 on 0 to 8, 4 elements each.
const SHARP_PRIMS: u32 = 4;
const VULCAN_PRIMS: u32 = 9;
/// Wind Walk: four bands, drawn on every tick to the 200th.
const ARC_TICKS: u32 = 200;
/// A body's colour left by a flash fades back to white by 8 a channel a tick.
const FADE: u32 = 8;

/// The body flags and colour an effect gives a unit.
pub type Glow = Option<(u32, Option<[u8; 3]>)>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shot {
    SharpShooting,
    ArrowVulcan,
    /// Crescent Elbow's eight blue lines, drawn over everything.
    CrescentElbow,
    /// Dark Piercing's eight dark lines through the target along the line to the caster, drawn over everything.
    DarkPiercing,
}

/// One element of a line prim: when it was made, its wait, and its numbers.
struct Element {
    made: u32,
    age: i32,
    start: Vector,
    radius: f32,
    height: f32,
    length: f32,
    bearing: f32,
}

struct Lines {
    shot: Shot,
    /// The unit the lines start from: the caster's, or Dark Piercing's target.
    caster: Option<u32>,
    aim: Vector,
    start: Instant,
    /// Prims made so far, and where the caster last stood.
    made: u32,
    last: Option<Vector>,
    elements: Vec<Element>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Ray {
    lambda: f32,
    plane: f32,
}

struct Rays {
    unit: Option<u32>,
    start: Instant,
    rays: Vec<Ray>,
}

struct Band {
    height: f32,
    turn: f32,
    radius: f32,
    lean: f32,
}

struct Arcs {
    at: Vector,
    start: Instant,
    bands: Vec<Band>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Colour {
    /// Soul Change's gold on both units (385).
    Link,
    /// Wind Walk's pale green on the unit (389).
    WindWalk,
    /// Falcon Assault's flashes of the target (387).
    Falcon,
    /// Reject Sword's blinking outline (392).
    Reject,
}

struct Coloured {
    unit: Option<u32>,
    start: Instant,
    colour: Colour,
}

/// A target spinning (387): +30° each tick from tick 30, 25 times.
struct Spin {
    unit: Option<u32>,
    start: Instant,
    done: u32,
}

#[derive(Default)]
pub struct Sniper {
    rays: Vec<Rays>,
    lines: Vec<Lines>,
    arcs: Vec<Arcs>,
    bodies: Vec<Coloured>,
    spins: Vec<Spin>,
}

/// A True Sight ray's alpha at its age e: 6 a tick to 60, one more tick at 60, then 1 less a tick.
pub fn ray_alpha(e: u32) -> f32 {
    match e {
        1..=10 => 6.0 * e as f32,
        11 => 60.0,
        12..=70 => (71 - e) as f32,
        _ => 0.0,
    }
}

/// How far a ray's rim is from the middle at e.
pub fn ray_radius(e: u32) -> f32 {
    2.0 + 3.0 * e as f32
}

/// A shot line's alpha at its age e (drawn for 1 to 14): up to 5 at the first rate, then down at the second. Dark Piercing's:
/// 8 a tick to 160 at 20, then 2 less a tick (drawn for 1 to 99).
pub fn line_alpha(shot: Shot, e: u32) -> f32 {
    if shot == Shot::DarkPiercing {
        return match e {
            1..=20 => 8.0 * e as f32,
            21..=99 => 160.0 - 2.0 * (e - 20) as f32,
            _ => 0.0,
        };
    }
    let (rise, fall) = if shot == Shot::SharpShooting { (2.0, 1.0) } else { (8.0, 4.0) };
    match e {
        1..=5 => rise * e as f32,
        6..=14 => rise * 5.0 - fall * (e - 5) as f32,
        _ => 0.0,
    }
}

/// A Wind Walk band's alpha at its age e: 10 a tick to 110, held to 20, then 2 less a tick.
pub fn arc_alpha(e: u32) -> f32 {
    match e {
        1..=11 => 10.0 * e as f32,
        12..=20 => 110.0,
        _ => (110.0 - 2.0 * (e - 20) as f32).max(0.0),
    }
}

/// The arc's span in degrees at e: 30 and 3 a tick, at most 120.
pub fn arc_span(e: u32) -> f32 {
    (30.0 + 3.0 * e as f32).min(120.0)
}

/// How many quads a band draws at e: points every ⌊span/21⌋ degrees.
#[cfg(test)]
pub fn arc_quads(e: u32) -> usize {
    let span = arc_span(e);
    let step = (span / 21.0).floor();
    (span / step).ceil() as usize - 1
}

/// A cloudy arc ribbon 1 unit tall, `span` degrees round `at` from `turn`, `radius` out at `height`, leaning out `lean`; past the
/// 21st point the quads fan out from the 20th.
pub fn arc_ribbon(at: Vector, height: f32, turn: f32, radius: f32, lean: f32, span: f32, alpha: f32) -> Vec<MeshVertex> {
    let step = (span / 21.0).floor();
    let points = (span / step).ceil() as usize;
    let tint = [1.0, 1.0, 1.0, alpha / 255.0];
    let (lean_sin, lean_cos) = lean.to_radians().sin_cos();
    let base = |i: usize| {
        let (sin, cos) = ((turn + i as f32 * step) % 360.0).to_radians().sin_cos();
        (
            [at[0] + radius * cos, at[1] + height, at[2] + radius * sin],
            [at[0] + (lean_cos + radius) * cos, at[1] + height + lean_sin, at[2] + (lean_cos + radius) * sin],
        )
    };
    let mut vertices = Vec::new();
    for i in 1..points {
        // Only 21 points exist: from the 21st on a quad runs from point 19 to point i, with the last quad's u.
        let (first, j) = if i >= 21 { (19, 19) } else { (i - 1, i - 1) };
        let ((foot0, top0), (foot1, top1)) = (base(first), base(i));
        let u = [j as f32 / 21.0, (j + 1) as f32 / 21.0];
        let [a, b, c, d] = [
            MeshVertex { position: foot0, uv: [u[0], 1.0], tint },
            MeshVertex { position: foot1, uv: [u[1], 1.0], tint },
            MeshVertex { position: top1, uv: [u[1], 0.0], tint },
            MeshVertex { position: top0, uv: [u[0], 0.0], tint },
        ];
        vertices.extend([a, b, c, a, c, d]);
    }
    vertices
}

impl Band {
    /// A band at its age e: its ribbon.
    fn walls(&self, at: Vector, e: u32) -> Vec<MeshVertex> {
        let turn = (self.turn + 5.0 * e as f32) % 360.0;
        let radius = if e <= 20 { self.radius } else { self.radius + 0.1 * (e - 20) as f32 };
        arc_ribbon(at, self.height, turn, radius, self.lean, arc_span(e), arc_alpha(e))
    }
}

/// A body fades back to white by 8 a channel a tick from the colour a flash left.
fn fade_back(from: [u8; 3], ticks: u32) -> Option<[u8; 3]> {
    let drift = (ticks * FADE) as u16;
    let tint = from.map(|channel| (u16::from(channel) + drift).min(255) as u8);
    (tint != [255; 3]).then_some(tint)
}

impl Coloured {
    /// The flags and colour at tick e, and what a flash leaves after its last tick.
    fn at(&self, e: u32) -> (Glow, Option<[u8; 3]>) {
        use crate::actor::{GLOW_ADDITIVE, GLOW_OUTLINE};
        match self.colour {
            Colour::Link => (
                (40..=70).contains(&e).then_some((GLOW_ADDITIVE, Some([0xC8, 0x96, 0x32]))),
                e.checked_sub(70).and_then(|after| fade_back([0xC8, 0x96, 0x32], after)),
            ),
            Colour::WindWalk => (
                (5..=25).contains(&e).then_some((GLOW_ADDITIVE, Some([0xDC, 0xFA, 0xDC]))),
                e.checked_sub(25).and_then(|after| fade_back([0xDC, 0xFA, 0xDC], after)),
            ),
            Colour::Falcon => ((matches!(e, 30..=34 | 40..=44 | 50)).then_some((GLOW_ADDITIVE, None)), None),
            Colour::Reject => {
                let level = |e: u32| if e % 6 < 3 { 100 } else { 250 };
                ((e <= 29).then(|| (GLOW_OUTLINE, Some([level(e); 3]))), e.checked_sub(29).and_then(|after| fade_back([level(29); 3], after)))
            }
        }
    }
}

impl Sniper {
    /// True Sight on `unit`: 48 rays, per element its plane's turn from `rand(n)` (0 to n − 1).
    pub fn true_sight(&mut self, unit: Option<u32>, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        let mut rays = Vec::with_capacity(RAY_COUNT);
        for i in 0..12 {
            for k in 0..4 {
                rays.push(Ray { lambda: (7 * i + 90 * k) as f32, plane: rand(360) as f32 });
            }
        }
        self.rays.push(Rays { unit, start: now, rays });
    }

    /// Sharp Shooting's (388) or Arrow Vulcan's (393) lines from `caster` toward `aim`: a prim of four elements on each tick.
    pub fn shot(&mut self, vulcan: bool, caster: Option<u32>, aim: Vector, now: Instant) {
        let shot = if vulcan { Shot::ArrowVulcan } else { Shot::SharpShooting };
        self.lines.push(Lines { shot, caster, aim, start: now, made: 0, last: None, elements: Vec::new() });
    }

    /// Crescent Elbow's (892) lines from `caster` toward `aim`: a prim of four elements on each of ticks 0 and 1.
    pub fn elbow(&mut self, caster: Option<u32>, aim: Vector, now: Instant) {
        self.lines.push(Lines { shot: Shot::CrescentElbow, caster, aim, start: now, made: 0, last: None, elements: Vec::new() });
    }

    /// Dark Piercing's (1030) lines through `target`, along the line to the caster's feet `aim`: a prim of four elements on
    /// each of ticks 0 and 1.
    pub fn dark_piercing(&mut self, target: Option<u32>, aim: Vector, now: Instant) {
        self.lines.push(Lines { shot: Shot::DarkPiercing, caster: target, aim, start: now, made: 0, last: None, elements: Vec::new() });
    }

    /// Wind Walk's four arcs over the place `at`, each band's radius and lean from `rand(n)`.
    pub fn wind_walk(&mut self, at: Vector, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        let bands = (0..4)
            .map(|k| {
                let radius = 4.5 + 0.03 * rand(101) as f32;
                let lean = 80.0 + rand(21) as f32;
                Band { height: (2 * k + 5) as f32, turn: (90 * k) as f32, radius, lean }
            })
            .collect();
        self.arcs.push(Arcs { at, start: now, bands });
    }

    /// Soul Change's glow on `unit` (385).
    pub fn glow_link(&mut self, unit: Option<u32>, now: Instant) {
        self.bodies.push(Coloured { unit, start: now, colour: Colour::Link });
    }

    pub fn glow_wind_walk(&mut self, unit: Option<u32>, now: Instant) {
        self.bodies.push(Coloured { unit, start: now, colour: Colour::WindWalk });
    }

    /// Falcon Assault on its target: the flashes and the spin.
    pub fn falcon(&mut self, unit: Option<u32>, now: Instant) {
        self.bodies.push(Coloured { unit, start: now, colour: Colour::Falcon });
        self.spins.push(Spin { unit, start: now, done: 0 });
    }

    pub fn reject_sword(&mut self, unit: Option<u32>, now: Instant) {
        self.bodies.push(Coloured { unit, start: now, colour: Colour::Reject });
    }

    /// How many degrees `unit` has turned since this was last asked (30 a tick on ticks 30 to 54).
    pub fn turned(&mut self, unit: Option<u32>, now: Instant) -> f32 {
        let mut degrees = 0.0;
        for spin in self.spins.iter_mut().filter(|spin| spin.unit == unit) {
            let due = passes(spin.start, now).saturating_sub(30).min(25);
            degrees += 30.0 * due.saturating_sub(spin.done) as f32;
            spin.done = spin.done.max(due);
        }
        degrees
    }

    /// Makes the lines' prims as their ticks come (the caster's place then, the numbers from `rand(n)`), and drops what ended.
    pub fn advance(&mut self, now: Instant, position_of: impl Fn(Option<u32>) -> Option<Vector>, mut rand: impl FnMut(u32) -> u32) {
        for lines in &mut self.lines {
            lines.last = position_of(lines.caster).or(lines.last);
            let prims = match lines.shot {
                Shot::SharpShooting => SHARP_PRIMS,
                Shot::ArrowVulcan => VULCAN_PRIMS,
                Shot::CrescentElbow | Shot::DarkPiercing => 2,
            };
            while lines.made < prims && passes(lines.start, now) > lines.made {
                if let Some(feet) = lines.last {
                    let made = lines.made;
                    for _ in 0..4 {
                        lines.elements.push(Element::make(lines.shot, made, feet, lines.aim, &mut rand));
                    }
                }
                lines.made += 1;
            }
        }
        self.rays.retain(|rays| passes(rays.start, now) <= RAY_TICKS);
        self.lines.retain(|lines| passes(lines.start, now) <= 200);
        self.arcs.retain(|arcs| passes(arcs.start, now) <= ARC_TICKS);
        self.bodies.retain(|coloured| passes(coloured.start, now) <= 200);
        self.spins.retain(|spin| passes(spin.start, now) <= 60);
    }

    /// The body flags and colour an effect gives `unit` at `now`.
    pub fn glow(&self, unit: Option<u32>, now: Instant) -> Glow {
        self.bodies.iter().filter(|coloured| coloured.unit == unit).find_map(|coloured| coloured.at(passes(coloured.start, now).checked_sub(1)?).0)
    }

    /// The colour a flash left on `unit`'s body, fading back to white.
    pub fn body_tint(&self, unit: Option<u32>, now: Instant) -> Option<[u8; 3]> {
        self.bodies.iter().filter(|coloured| coloured.unit == unit).find_map(|coloured| coloured.at(passes(coloured.start, now).checked_sub(1)?).1)
    }

    /// Crescent Elbow's and Dark Piercing's lines at `now`, drawn over everything with no depth test.
    pub fn top_meshes(&self, now: Instant) -> Vec<Wall> {
        let mut walls = Vec::new();
        for lines in self.lines.iter().filter(|lines| matches!(lines.shot, Shot::CrescentElbow | Shot::DarkPiercing)) {
            let Some(t) = passes(lines.start, now).checked_sub(1) else { continue };
            let last = if lines.shot == Shot::DarkPiercing { 99 } else { 14 };
            for element in &lines.elements {
                let e = element.age + t as i32 - element.made as i32 + 1;
                if (1..=last).contains(&e) {
                    walls.extend(element.walls(lines.shot, line_alpha(lines.shot, e as u32)));
                }
            }
        }
        walls
    }

    /// Every mesh to draw at `now`; `position_of` gives a unit's feet.
    pub fn meshes(&self, now: Instant, position_of: impl Fn(Option<u32>) -> Option<Vector>) -> Vec<Wall> {
        let mut walls = Vec::new();
        for rays in &self.rays {
            let (Some(feet), Some(t)) = (position_of(rays.unit), passes(rays.start, now).checked_sub(1)) else { continue };
            let e = t + 1;
            let alpha = ray_alpha(e);
            if e > 70 || alpha <= 0.0 {
                continue;
            }
            let radius = ray_radius(e);
            let tint = [RAY_TINT[0], RAY_TINT[1], RAY_TINT[2], alpha / 255.0];
            let (mut thin, mut wide) = (Vec::new(), Vec::new());
            for ray in &rays.rays {
                let (plane_sin, plane_cos) = ray.plane.to_radians().sin_cos();
                let at = |angle: f32| {
                    let (sin, cos) = angle.rem_euclid(360.0).to_radians().sin_cos();
                    [feet[0] + radius * cos * plane_cos, feet[1] + 12.0 - radius * cos * plane_sin, feet[2] + radius * sin]
                };
                let middle = [feet[0], feet[1] + 12.0, feet[2]];
                for (width, vertices) in [(2.0, &mut thin), (5.0, &mut wide)] {
                    vertices.extend([
                        MeshVertex { position: at(ray.lambda - width), uv: [0.0, 1.0], tint },
                        MeshVertex { position: at(ray.lambda + width), uv: [1.0, 1.0], tint },
                        MeshVertex { position: middle, uv: [1.0, 0.0], tint },
                    ]);
                }
            }
            walls.push(Wall { texture: RAY, vertices: thin, additive: true, forced: false });
            walls.push(Wall { texture: RAY, vertices: wide, additive: false, forced: false });
        }
        for lines in self.lines.iter().filter(|lines| matches!(lines.shot, Shot::SharpShooting | Shot::ArrowVulcan)) {
            let Some(t) = passes(lines.start, now).checked_sub(1) else { continue };
            for element in &lines.elements {
                let e = element.age + t as i32 - element.made as i32 + 1;
                if !(1..=14).contains(&e) {
                    continue;
                }
                walls.extend(element.walls(lines.shot, line_alpha(lines.shot, e as u32)));
            }
        }
        for arcs in &self.arcs {
            let Some(t) = passes(arcs.start, now).checked_sub(1) else { continue };
            if t >= ARC_TICKS {
                continue;
            }
            for band in &arcs.bands {
                walls.push(Wall { texture: CLOUD, vertices: band.walls(arcs.at, t + 1), additive: true, forced: false });
            }
        }
        walls
    }
}

impl Element {
    /// A line element made on tick `made` of a shot from `feet` toward `aim`: its numbers from `rand(n)`, in the order of the table.
    fn make(shot: Shot, made: u32, feet: Vector, aim: Vector, rand: &mut impl FnMut(u32) -> u32) -> Self {
        let (dx, dz) = (aim[0] - feet[0], aim[2] - feet[2]);
        let distance = f32::hypot(dx, dz);
        let bearing = bearing(dx, dz) as f32;
        if shot == Shot::CrescentElbow {
            // Its wait, then five R(100): x, z, how far it backs off, the height and the length.
            let age = -(rand(20) as i32);
            let x = feet[0] - 3.0 + 0.06 * rand(100) as f32;
            let z = feet[2] - 3.0 + 0.06 * rand(100) as f32;
            let back = 0.15 * rand(100) as f32;
            let height = 8.0 + 0.04 * rand(100) as f32;
            let length = 20.0 + 0.2 * rand(100) as f32;
            let (sin, cos) = bearing.to_radians().sin_cos();
            return Self { made, age, start: [x - cos * back, feet[1], z - sin * back], radius: 0.25, height, length, bearing };
        }
        if shot == Shot::DarkPiercing {
            // Its wait, then four R(100): x, z, the height and the length; it starts 100 beyond the target, away from the caster.
            let age = -(rand(20) as i32);
            let x = feet[0] - 15.0 + 0.3 * rand(100) as f32;
            let z = feet[2] - 15.0 + 0.3 * rand(100) as f32;
            let height = 8.0 + 0.04 * rand(100) as f32;
            let length = 100.0 + 0.5 * rand(100) as f32;
            let (sin, cos) = bearing.to_radians().sin_cos();
            return Self { made, age, start: [x - 100.0 * cos, feet[1], z - 100.0 * sin], radius: 0.25, height, length, bearing };
        }
        let vulcan = shot == Shot::ArrowVulcan;
        let age = if vulcan { -30 - rand(110) as i32 } else { -45 - rand(20) as i32 };
        let x = feet[0] + 0.08 * rand(100) as f32 - 4.0;
        let z = feet[2] + 0.08 * rand(100) as f32 - 4.0;
        let back = 0.15 * rand(100) as f32;
        let (sin, cos) = bearing.to_radians().sin_cos();
        let radius = if vulcan { 0.02 + 0.008 * rand(11) as f32 } else { 0.05 };
        let height = if vulcan { 4.0 + 0.06 * rand(100) as f32 } else { 8.0 + 0.04 * rand(100) as f32 };
        let length = distance + if vulcan { 0.3 } else { 0.6 } * rand(100) as f32;
        Self { made, age, start: [x - cos * back, feet[1], z - sin * back], radius, height, length, bearing }
    }

    /// The tube round the line in two passes: thin and blended, then fatter and added on.
    fn walls(&self, shot: Shot, alpha: f32) -> [Wall; 2] {
        let (sin, cos) = self.bearing.to_radians().sin_cos();
        let end = [self.start[0] + self.length * cos, self.start[1], self.start[2] + self.length * sin];
        let (side_sin, side_cos) = ((self.bearing + 90.0) % 360.0).to_radians().sin_cos();
        let pink = [1.0, 89.0 / 255.0, 182.0 / 255.0];
        let (rgb, wide) = match shot {
            Shot::SharpShooting => ([1.0; 3], self.radius + 0.25),
            Shot::ArrowVulcan => (pink, 2.0 * self.radius),
            Shot::CrescentElbow => ([55.0 / 255.0, 55.0 / 255.0, 1.0], 0.5),
            Shot::DarkPiercing => ([55.0 / 255.0; 3], 0.5),
        };
        let texture = if matches!(shot, Shot::CrescentElbow | Shot::DarkPiercing) { CLOUD } else { RAY };
        let tint = [rgb[0], rgb[1], rgb[2], alpha / 255.0];
        let pass = |rho: f32, additive: bool| {
            let ring = |at: Vector, s: u32| {
                let (phi_sin, phi_cos) = ((72 * s) as f32).to_radians().sin_cos();
                [at[0] + rho * phi_cos * side_cos, at[1] + self.height - rho * phi_sin, at[2] + rho * phi_cos * side_sin]
            };
            let mut vertices = Vec::new();
            for s in 1..=5 {
                let [a, b, c, d] = [
                    MeshVertex { position: ring(self.start, s - 1), uv: [0.0, 1.0], tint },
                    MeshVertex { position: ring(self.start, s), uv: [1.0, 1.0], tint },
                    MeshVertex { position: ring(end, s), uv: [1.0, 0.0], tint },
                    MeshVertex { position: ring(end, s - 1), uv: [0.0, 0.0], tint },
                ];
                vertices.extend([a, b, c, a, c, d]);
            }
            Wall { texture, vertices, additive, forced: false }
        };
        [pass(self.radius, false), pass(wide, true)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prims::TICK;

    fn near(got: f32, want: f32) -> bool {
        (got - want).abs() < 0.01
    }

    #[test]
    fn true_sight_bursts_48_rays() {
        let start = Instant::now();
        let mut sniper = Sniper::default();
        sniper.true_sight(Some(1), start, |_| 0);
        assert_eq!(sniper.rays[0].rays.len(), 48);
        // λ = 7i + 90k: 347 for prim 11, element 3.
        assert_eq!(sniper.rays[0].rays.last().unwrap().lambda, 347.0);
        // Alpha and r at e = 1, 10, 11, 12, 70, 71: 6/5, 60/32, 60/35, 59/38, 1/212, hidden.
        assert_eq!([1, 10, 11, 12, 70, 71].map(ray_alpha), [6.0, 60.0, 60.0, 59.0, 1.0, 0.0]);
        assert_eq!([1, 10, 11, 12, 70].map(ray_radius), [5.0, 32.0, 35.0, 38.0, 212.0]);
        // Drawn on ticks 0…69; 96 triangles (the wedges W1 additive, W2 blended).
        let feet = [0.0; 3];
        let walls = sniper.meshes(start, |_| Some(feet));
        assert_eq!(walls.len(), 2);
        assert!(walls[0].additive && !walls[1].additive && walls.iter().all(|wall| wall.vertices.len() == 144));
        assert_eq!(walls[0].vertices[0].tint[..3], [250.0 / 255.0; 3]);
        assert_eq!(sniper.meshes(start + TICK * 69, |_| Some(feet)).len(), 2);
        assert!(sniper.meshes(start + TICK * 70, |_| Some(feet)).is_empty());
        // The ray λ = 0 spans 358…2 (W1) and 355…5 (W2): tip points at radius 5, plane turn 0.
        let thin = &walls[0].vertices;
        let wide = &walls[1].vertices;
        let angle = |position: Vector| position[2].atan2(position[0]).to_degrees().rem_euclid(360.0);
        assert!(near(angle(thin[0].position), 358.0) && near(angle(thin[1].position), 2.0));
        assert!(near(angle(wide[0].position), 355.0) && near(angle(wide[1].position), 5.0));
        assert_eq!(thin[2].position, [0.0, 12.0, 0.0]);
        assert_eq!([thin[0].uv, thin[1].uv, thin[2].uv], [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0]]);
        // It follows the caster.
        assert!(sniper.meshes(start, |_| None).is_empty());
    }

    #[test]
    fn sharp_shooting_and_arrow_vulcan_leave_lines_along_the_shot() {
        let start = Instant::now();
        let feet = [10.0, 1.0, 10.0];
        let aim = [30.0, 1.0, 10.0];
        let mut sharp = Sniper::default();
        sharp.shot(false, Some(1), aim, start);
        let advance = |sniper: &mut Sniper, tick: u32| sniper.advance(start + TICK * tick, |_| Some(feet), |_| 0);
        for tick in 0..8 {
            advance(&mut sharp, tick);
        }
        // Four prims of four elements, made on ticks 0…3.
        assert_eq!(sharp.lines[0].elements.len(), 16);
        assert_eq!(sharp.lines[0].elements.iter().filter(|element| element.made == 3).count(), 4);
        // Every draw 0: P0 = E + (−4, 0, −4), 8 up, r0 0.05, the length the distance (20), along the bearing (due east).
        let element = &sharp.lines[0].elements[0];
        assert!(near(element.start[0], 6.0) && near(element.start[2], 6.0) && element.height == 8.0 && element.radius == 0.05 && near(element.length, 20.0));
        // Alpha at e = 1, 5, 6, 14, 15: 2, 10, 9, 1, hidden.
        assert_eq!([1, 5, 6, 14, 15].map(|e| line_alpha(Shot::SharpShooting, e)), [2.0, 10.0, 9.0, 1.0, 0.0]);
        // A tube first shows on tick 45 + the prim's tick, two passes of 30 vertices; pass 2 radius 0.30.
        let walls = |sniper: &Sniper, tick: u32| sniper.meshes(start + TICK * tick, |_| None);
        assert!(walls(&sharp, 44).is_empty());
        let first = walls(&sharp, 45);
        assert_eq!(first.len(), 2 * 4);
        assert!(!first[0].additive && first[1].additive && first.iter().all(|wall| wall.vertices.len() == 30));
        assert_eq!(first[0].vertices[0].tint[..3], [1.0; 3]);
        assert!((first[0].vertices[0].tint[3] * 255.0).round() == 2.0);
        // The ring radius round the line: 0.05 and 0.30 across it.
        let across = |wall: &Wall| wall.vertices.iter().map(|vertex| (vertex.position[2] - element.start[2]).abs()).fold(0.0, f32::max);
        assert!(across(&first[0]) <= 0.051 && across(&first[1]) <= 0.301 && across(&first[1]) > 0.1);
        // The line points along the bearing to the aim: the far end 20 on.
        let far = first[0].vertices.iter().map(|vertex| vertex.position[0]).fold(f32::MIN, f32::max);
        assert!(far > element.start[0] + 19.0);
        // Arrow Vulcan: nine prims, first seen on tick 30 + the prim's tick, pink, pass 2 radius 0.04, alpha 8 … 40 … 4.
        let mut vulcan = Sniper::default();
        vulcan.shot(true, Some(1), aim, start);
        for tick in 0..12 {
            advance(&mut vulcan, tick);
        }
        assert_eq!(vulcan.lines[0].elements.len(), 36);
        let element = &vulcan.lines[0].elements[0];
        assert!(near(element.radius, 0.02) && element.height == 4.0);
        assert_eq!([1, 5, 6, 14, 15].map(|e| line_alpha(Shot::ArrowVulcan, e)), [8.0, 40.0, 36.0, 4.0, 0.0]);
        assert!(walls(&vulcan, 29).is_empty());
        let first = walls(&vulcan, 30);
        assert_eq!(first.len(), 2 * 4);
        assert_eq!(first[0].vertices[0].tint[..3], [1.0, 89.0 / 255.0, 182.0 / 255.0]);
        let across = |wall: &Wall, around: f32| wall.vertices.iter().map(|vertex| (vertex.position[2] - around).abs()).fold(0.0, f32::max);
        assert!(across(&first[1], element.start[2]) <= 0.0401 && across(&first[1], element.start[2]) > 0.02);
    }

    #[test]
    fn wind_walk_spins_four_cloudy_arcs() {
        let start = Instant::now();
        let mut sniper = Sniper::default();
        sniper.wind_walk([5.0, 1.0, 5.0], start, |_| 0);
        let bands = &sniper.arcs[0].bands;
        assert_eq!(bands.iter().map(|band| band.height).collect::<Vec<_>>(), [5.0, 7.0, 9.0, 11.0]);
        assert_eq!(bands.iter().map(|band| band.turn).collect::<Vec<_>>(), [0.0, 90.0, 180.0, 270.0]);
        assert!(bands.iter().all(|band| near(band.radius, 4.5) && band.lean == 80.0));
        // Alpha at e = 1, 11, 20, 21, 74, 75: 10, 110, 110, 108, 2, 0; the span 33 at e = 1 and 120 from e = 30.
        assert_eq!([1, 11, 20, 21, 74, 75].map(arc_alpha), [10.0, 110.0, 110.0, 108.0, 2.0, 0.0]);
        assert_eq!((arc_span(1), arc_span(30), arc_span(60)), (33.0, 120.0, 120.0));
        // Quads: 32 at e = 1, 23 at e = 30; those past j = 19 use u = 19/21 … 20/21.
        assert_eq!((arc_quads(1), arc_quads(30)), (32, 23));
        let walls = sniper.meshes(start, |_| None);
        assert_eq!(walls.len(), 4);
        assert!(walls.iter().all(|wall| wall.texture == CLOUD && wall.additive && wall.vertices.len() == 6 * 32));
        let last = &walls[0].vertices[6 * 31..];
        assert!(near(last[0].uv[0], 19.0 / 21.0) && near(last[1].uv[0], 20.0 / 21.0));
        let early = &walls[0].vertices[6 * 5..6 * 6];
        assert!(near(early[0].uv[0], 5.0 / 21.0) && near(early[1].uv[0], 6.0 / 21.0));
        // The radius is r0 to e = 20, then 0.1 more a tick: a vertex at e = 21 sits 4.6 out from the unit's place.
        let radius_of = |tick: u32| {
            let vertices = &sniper.meshes(start + TICK * tick, |_| None)[0].vertices;
            let position = vertices[0].position;
            f32::hypot(position[0] - 5.0, position[2] - 5.0)
        };
        assert!(near(radius_of(0), 4.5) && near(radius_of(19), 4.5) && near(radius_of(20), 4.6));
        // Still drawn (invisible) to tick 199, none at 200; the arcs stay where the unit stood.
        assert_eq!(sniper.meshes(start + TICK * 199, |_| None).len(), 4);
        assert!(sniper.meshes(start + TICK * 200, |_| None).is_empty());
        // The body: additive #DCFADC on ticks 5…25 only.
        sniper.glow_wind_walk(Some(1), start);
        let glow = |tick: u32| sniper.glow(Some(1), start + TICK * tick);
        assert_eq!(glow(5), Some((crate::actor::GLOW_ADDITIVE, Some([0xDC, 0xFA, 0xDC]))));
        assert!(glow(4).is_none() && glow(25).is_some() && glow(26).is_none());
    }

    #[test]
    fn soul_change_falcon_assault_and_reject_sword_colour_the_body() {
        let start = Instant::now();
        let mut sniper = Sniper::default();
        sniper.glow_link(Some(1), start);
        let glow = |sniper: &Sniper, unit: u32, tick: u32| sniper.glow(Some(unit), start + TICK * tick);
        assert_eq!(glow(&sniper, 1, 40), Some((crate::actor::GLOW_ADDITIVE, Some([0xC8, 0x96, 0x32]))));
        assert!(glow(&sniper, 1, 39).is_none() && glow(&sniper, 1, 70).is_some() && glow(&sniper, 1, 71).is_none());
        assert!(sniper.body_tint(Some(1), start + TICK * 71).is_some(), "the gold fades back");
        // Falcon Assault: additive on ticks 30…34, 40…44, 50; the spin is +30° a tick on ticks 30…54, 750° in all.
        sniper.falcon(Some(2), start);
        let flashes: Vec<u32> = (0..70).filter(|&tick| glow(&sniper, 2, tick).is_some()).collect();
        assert_eq!(flashes, [30, 31, 32, 33, 34, 40, 41, 42, 43, 44, 50]);
        assert_eq!(glow(&sniper, 2, 30), Some((crate::actor::GLOW_ADDITIVE, None)));
        let mut total = 0.0;
        let mut first_turn = 0.0;
        for tick in 0..70 {
            let turned = sniper.turned(Some(2), start + TICK * tick);
            if tick == 30 {
                first_turn = turned;
            }
            total += turned;
        }
        assert_eq!((first_turn, total), (30.0, 750.0));
        let mut again = Sniper::default();
        again.falcon(Some(2), start);
        assert_eq!(again.turned(Some(2), start + TICK * 29), 0.0);
        assert_eq!(again.turned(Some(2), start + TICK * 30), 30.0);
        assert_eq!(again.turned(Some(2), start + TICK * 30), 0.0, "asked twice in one tick");
        assert_eq!(again.turned(Some(3), start + TICK * 40), 0.0, "another unit");
        // Reject Sword: outline copies on ticks 0…29 only, the colour 100 on 0…2 and 6…8, 250 on 3…5.
        sniper.reject_sword(Some(3), start);
        let outline = |tick: u32| glow(&sniper, 3, tick);
        assert_eq!(outline(0), Some((crate::actor::GLOW_OUTLINE, Some([100; 3]))));
        assert_eq!(outline(3), Some((crate::actor::GLOW_OUTLINE, Some([250; 3]))));
        assert_eq!((outline(6).unwrap().1, outline(8).unwrap().1, outline(5).unwrap().1), (Some([100; 3]), Some([100; 3]), Some([250; 3])));
        assert!(outline(29).is_some() && outline(30).is_none());
    }

    #[test]
    fn crescent_elbow_leaves_eight_blue_lines_over_everything() {
        let start = Instant::now();
        let feet = [10.0, 1.0, 10.0];
        let aim = [30.0, 1.0, 10.0];
        let mut sniper = Sniper::default();
        sniper.elbow(Some(1), aim, start);
        for tick in 0..4 {
            sniper.advance(start + TICK * tick, |_| Some(feet), |_| 0);
        }
        // Two prims of four, made on ticks 0 and 1.
        assert_eq!(sniper.lines[0].elements.len(), 8);
        // Every draw 0: 3 west and 3 north of the caster's feet, 8 up, 0.25 round, 20 long, along the bearing to the aim.
        let element = &sniper.lines[0].elements[0];
        assert!(near(element.start[0], 7.0) && near(element.start[2], 7.0) && element.height == 8.0 && element.radius == 0.25 && near(element.length, 20.0));
        // Alpha at e = 1, 5, 6, 14, 15: 8, 40, 36, 4, hidden.
        assert_eq!([1, 5, 6, 14, 15].map(|e| line_alpha(Shot::CrescentElbow, e)), [8.0, 40.0, 36.0, 4.0, 0.0]);
        // Over everything, never in the depth-tested pass: thin and blended, then 0.5 wide and added on, in #3737FF.
        assert!(sniper.meshes(start, |_| Some(feet)).is_empty());
        let walls = sniper.top_meshes(start);
        assert_eq!(walls.len(), 2 * 4);
        assert!(!walls[0].additive && walls[1].additive && walls.iter().all(|wall| wall.texture == CLOUD && wall.vertices.len() == 30));
        assert_eq!(walls[0].vertices[0].tint[..3], [55.0 / 255.0, 55.0 / 255.0, 1.0]);
        assert_eq!((walls[0].vertices[0].tint[3] * 255.0).round(), 8.0);
        let across = |wall: &Wall| wall.vertices.iter().map(|vertex| (vertex.position[2] - element.start[2]).abs()).fold(0.0, f32::max);
        assert!(across(&walls[0]) <= 0.251 && across(&walls[1]) <= 0.501 && across(&walls[1]) > 0.3);
        assert_eq!(sniper.top_meshes(start + TICK * 1).len(), 2 * 8);
    }

    #[test]
    fn dark_piercing_leaves_eight_dark_lines_through_the_target() {
        let start = Instant::now();
        let feet = [100.0, 1.0, 100.0];
        // The caster stands due east: u = (1, 0, 0).
        let aim = [130.0, 1.0, 100.0];
        let make = |draw: u32| {
            let mut sniper = Sniper::default();
            sniper.dark_piercing(Some(1), aim, start);
            for tick in 0..4 {
                sniper.advance(start + TICK * tick, |_| Some(feet), |n| draw.min(n - 1));
            }
            sniper
        };
        // Two prims of four, made on ticks 0 and 1 only: 8 lines a launch, 16 for a packet's two.
        let low = make(0);
        assert_eq!(low.lines[0].elements.len(), 8);
        assert_eq!(low.lines[0].elements.iter().map(|element| element.made).collect::<Vec<_>>(), [0, 0, 0, 0, 1, 1, 1, 1]);
        // The draws in order: the age from R(20), then x, z, h, L from R(100).
        let mut asked = Vec::new();
        let mut sniper = Sniper::default();
        sniper.dark_piercing(Some(1), aim, start);
        sniper.advance(
            start,
            |_| Some(feet),
            |n| {
                asked.push(n);
                0
            },
        );
        assert_eq!(asked[..5], [20, 100, 100, 100, 100]);
        // Every draw 0: age 0, P0 = (E.x − 15, E.y, E.z − 15) − 100 u, 8 up, 100 long, toward the caster.
        let element = &low.lines[0].elements[0];
        assert!(element.age == 0 && near(element.start[0], -15.0) && element.start[1] == 1.0 && near(element.start[2], 85.0));
        assert!(element.height == 8.0 && element.radius == 0.25 && near(element.length, 100.0) && element.bearing == 0.0);
        // Every R(100) draw 99 (and R(20) 19): 29.7 more in x and z, 11.96 up, 149.5 long, the age −19.
        let high = make(99);
        let element = &high.lines[0].elements[0];
        assert!(element.age == -19 && near(element.start[0], 14.7) && near(element.start[2], 114.7));
        assert!(near(element.height, 11.96) && near(element.length, 149.5));
        // Alpha at n = 1, 20, 21, 99, 100: 8, 160, 158, 2, 0.
        assert_eq!([1, 20, 21, 99, 100].map(|n| line_alpha(Shot::DarkPiercing, n)), [8.0, 160.0, 158.0, 2.0, 0.0]);
        // Nothing while the age is ≤ 0; over everything, thin and blended (0.25), then 0.5 and added on, in #373737 on cloud11.
        assert!(high.top_meshes(start).is_empty() && high.meshes(start, |_| Some(feet)).is_empty());
        let walls = low.top_meshes(start);
        assert_eq!(walls.len(), 2 * 4);
        assert!(!walls[0].additive && walls[1].additive && walls.iter().all(|wall| wall.texture == CLOUD && wall.vertices.len() == 30));
        assert_eq!(walls[0].vertices[0].tint[..3], [55.0 / 255.0; 3]);
        assert_eq!((walls[0].vertices[0].tint[3] * 255.0).round(), 8.0);
        let across = |wall: &Wall| wall.vertices.iter().map(|vertex| (vertex.position[2] - 85.0).abs()).fold(0.0, f32::max);
        assert!(across(&walls[0]) <= 0.251 && across(&walls[1]) <= 0.501 && across(&walls[1]) > 0.3);
        // Drawn to n = 99: a line made on tick 0 with age 0 shows on tick 98, not 99.
        assert_eq!(low.top_meshes(start + TICK * 98).len(), 2 * 8);
        assert_eq!(low.top_meshes(start + TICK * 99).len(), 2 * 4, "the second prim's lines");
        assert!(low.top_meshes(start + TICK * 100).is_empty());
        // The lines stay where they were made when the target moves.
        let mut moved = make(0);
        moved.advance(start + TICK * 10, |_| Some([0.0; 3]), |_| 0);
        assert_eq!(moved.top_meshes(start + TICK * 10)[0].vertices[0].position, low.top_meshes(start + TICK * 10)[0].vertices[0].position);
    }
}
