//! Shapes standing or falling on the ground, built in code a 16 ms tick at a time: Magnus' and Sanctuary's glowing boxes over
//! each cell (318, 319), Triple Attack's eight thin streaks (329), Slim Pitcher's falling potion and red flash (497) and Full
//! Protection's blue strobe of the body (500).
//!
//! A shape's own age a counts from 1 at its first tick (p = a − 1 from 0).

use std::time::Instant;

use super::passes;
use super::walls::Wall;
use crate::world::{MeshVertex, Vector};

pub const RING_RED: &str = "data\\texture\\effect\\ring_red.tga";
pub const ALPHA_DOWN: &str = "data\\texture\\effect\\alpha_down.tga";
pub const CLOUD: &str = "data\\texture\\effect\\cloud11.tga";
pub const FLASH: &str = "data\\texture\\effect\\bbbb.bmp";
/// The potion's half-diagonal in world units.
pub const POTION_HALF: f32 = 8.0;
/// The red potion's icon, 24 × 24 in a texture padded to 32 × 32.
pub const RED_POTION: &str = "data\\texture\\유저인터페이스\\item\\레드슬림포션.bmp";

/// Magnus' box (318): white, additive, alpha 30, 50 high (10 under /mineffect), half-side 5, held 864 ticks.
/// Sanctuary's (319): #EBFFEB, blended, alpha 120, 16 high, half-side 2.5, held 1880.
struct Look {
    texture: &'static str,
    rgb: [f32; 3],
    additive: bool,
    alpha: f32,
    height: f32,
    half: f32,
    hold: u32,
}

const MAGNUS: Look = Look { texture: RING_RED, rgb: [1.0; 3], additive: true, alpha: 30.0, height: 50.0, half: 5.0, hold: 864 };
const SANCTUARY: Look =
    Look { texture: ALPHA_DOWN, rgb: [235.0 / 255.0, 1.0, 235.0 / 255.0], additive: false, alpha: 120.0, height: 16.0, half: 2.5, hold: 1880 };
/// Magnus' height under /mineffect.
const MAGNUS_MIN_HEIGHT: f32 = 10.0;

/// A box standing over a skill unit's cell.
struct BoxCell {
    id: u32,
    look: &'static Look,
    height: f32,
    at: Vector,
    start: Instant,
    /// The phase of its breathing at the start, in degrees.
    phase: f32,
}

/// One Triple Attack streak, made on tick `made` of its effect.
struct Streak {
    made: u32,
    wait: i32,
    start: Vector,
    end: Vector,
    height: f32,
    heading: f32,
}

struct Triple {
    start: Instant,
    streaks: Vec<Streak>,
}

/// A potion falling on a cell and the flash under it.
struct Slim {
    at: Vector,
    start: Instant,
    icon: &'static str,
    spin: f32,
}

/// A body strobing blue.
struct Chemical {
    unit: Option<u32>,
    start: Instant,
}

/// The potion's pictures to draw: its place, spin (degrees) and alpha (0 to 255).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Potion {
    pub icon: &'static str,
    pub at: Vector,
    pub spin: f32,
    pub alpha: f32,
}

#[derive(Default)]
pub struct Ground {
    boxes: Vec<BoxCell>,
    triples: Vec<Triple>,
    slims: Vec<Slim>,
    chemicals: Vec<Chemical>,
}

/// A box's top at its age a: its breathing, rising over the first 90 ticks.
pub fn box_top(height: f32, phase: f32, a: u32) -> f32 {
    let breath = (phase + a as f32) % 360.0;
    let top = height * breath.to_radians().sin() * 0.35 + height * 0.65;
    if a < 90 { top * (a as f32).to_radians().sin() } else { top }
}

/// A box's alpha at age a: its own, then 2 less a tick once its hold is over.
pub fn box_alpha(alpha: f32, hold: u32, a: u32) -> f32 {
    if a <= hold { alpha } else { (alpha - 2.0 * (a - hold) as f32).max(0.0) }
}

impl BoxCell {
    /// The four walls round the cell, from 1 unit up to 1 + the top.
    fn walls(&self, a: u32) -> Option<Wall> {
        let alpha = box_alpha(self.look.alpha, self.look.hold, a);
        if alpha <= 0.0 {
            return None;
        }
        let top = box_top(self.height, self.phase, a);
        let tint = [self.look.rgb[0], self.look.rgb[1], self.look.rgb[2], alpha.trunc() / 255.0];
        let half = self.look.half;
        let corners = [[half, -half], [half, half], [-half, half], [-half, -half]];
        let mut vertices = Vec::new();
        for side in 0..4 {
            let ([x0, z0], [x1, z1]) = (corners[side], corners[(side + 1) % 4]);
            let point = |x: f32, z: f32, up: f32, uv: [f32; 2]| MeshVertex { position: [self.at[0] + x, self.at[1] + up, self.at[2] + z], uv, tint };
            let [foot0, foot1, top1, top0] =
                [point(x0, z0, 1.0, [0.0, 1.0]), point(x1, z1, 1.0, [1.0, 1.0]), point(x1, z1, 1.0 + top, [1.0, 0.0]), point(x0, z0, 1.0 + top, [0.0, 0.0])];
            vertices.extend([foot0, foot1, top1, foot0, top1, top0]);
        }
        Some(Wall { texture: self.look.texture, vertices, additive: self.look.additive, forced: false })
    }
}

/// Triple Attack's alpha at a streak's age a (from 1): 8 a tick to 40, then 4 less a tick; drawn for 1 to 14.
pub fn triple_alpha(a: i32) -> f32 {
    match a {
        1..=5 => 8.0 * a as f32,
        6..=14 => 40.0 - 4.0 * (a - 5) as f32,
        _ => 0.0,
    }
}

impl Streak {
    /// The tube along the heading in two passes: thin and yellow blended, fat and pale added on.
    fn walls(&self, alpha: f32) -> [Wall; 2] {
        let heading = self.heading + 90.0;
        let (sin, cos) = heading.to_radians().sin_cos();
        let pass = |rho: f32, rgb: [f32; 3], additive: bool| {
            let tint = [rgb[0], rgb[1], rgb[2], alpha.trunc() / 255.0];
            let ring = |at: Vector, j: u32| {
                let theta = (72 * j) as f32;
                let (theta_sin, theta_cos) = theta.to_radians().sin_cos();
                MeshVertex {
                    position: [at[0] + rho * theta_cos * cos, at[1] + self.height + rho * theta_sin, at[2] + rho * theta_cos * sin],
                    uv: [0.0, 0.0],
                    tint,
                }
            };
            let mut vertices = Vec::new();
            for j in 1..=5 {
                let corner = |mut vertex: MeshVertex, uv: [f32; 2]| {
                    vertex.uv = uv;
                    vertex
                };
                let [a0, a1, b1, b0] = [
                    corner(ring(self.start, j - 1), [0.0, 1.0]),
                    corner(ring(self.start, j), [1.0, 1.0]),
                    corner(ring(self.end, j), [1.0, 0.0]),
                    corner(ring(self.end, j - 1), [0.0, 0.0]),
                ];
                vertices.extend([a0, a1, b1, a0, b1, b0]);
            }
            Wall { texture: CLOUD, vertices, additive, forced: false }
        };
        [pass(0.25, [1.0, 1.0, 55.0 / 255.0], false), pass(0.5, [1.0, 1.0, 200.0 / 255.0], true)]
    }
}

/// The corners (sprite pixels from the middle, y down) of a square of half-diagonal `half` pixels spun `spin` degrees that shows
/// a picture in the top-left three quarters of its texture (a 24 × 24 picture padded to 32 × 32): the whole texture drawn a third
/// bigger and moved so that the picture's centre sits on the middle. In the quad's order of texture coordinates (0, 1), (1, 1),
/// (1, 0), (0, 0), or, `mirrored`, the order with the corner at `spin` getting (1, 1), (0, 1), (0, 0), (1, 0).
pub fn padded_square(spin: f32, half: f32, mirrored: bool) -> [[f32; 2]; 4] {
    let big = half / 0.75;
    // The corner whose texture coordinate is (1, 1) is at `spin` when mirrored, a quarter turn on otherwise; the picture's
    // centre is a quarter of the big half-diagonal toward the corner opposite it.
    let bright = if mirrored { spin } else { spin + 90.0 };
    let (sin, cos) = bright.to_radians().sin_cos();
    let shift = [0.25 * big * cos, 0.25 * big * sin];
    let corner = |j: u32| {
        let (sin, cos) = (spin + 90.0 * j as f32).to_radians().sin_cos();
        [shift[0] + big * cos, shift[1] + big * sin]
    };
    if mirrored { [corner(1), corner(0), corner(3), corner(2)] } else { [corner(0), corner(1), corner(2), corner(3)] }
}

/// The potion's height over its cell at tick n (from 1): 80 − 3n to n = 24, then 8.
pub fn potion_height(n: u32) -> f32 {
    80.0 - 3.0 * n.min(24) as f32
}

/// The potion's alpha at n: 20 a tick to 200, held, then from n = 33 5 less a tick; drawn for n = 1 to 71.
pub fn potion_alpha(n: u32) -> f32 {
    match n {
        1..=10 => 20.0 * n as f32,
        11..=32 => 200.0,
        33..=71 => (200.0 - 5.0 * (n - 32) as f32).max(0.0),
        _ => 0.0,
    }
}

/// The potion's spin at n: down 15° a tick to n = 24, then still.
pub fn potion_spin(spin: f32, n: u32) -> f32 {
    (spin - 15.0 * n.min(24) as f32).rem_euclid(360.0)
}

/// The flash's half-diagonal 0.9 r and alpha at tick t: its count n is t − 15; r is 15, then 7 % more each tick from n = 1.
pub fn flash_size(t: u32) -> (f32, f32) {
    let n = t.saturating_sub(15);
    let alpha = match n {
        1..=10 => 2.0 * n as f32,
        11..=29 => 20.0 - (n - 10) as f32,
        _ => 0.0,
    };
    (0.9 * 15.0 * 1.07_f32.powi(n as i32), alpha)
}

impl Slim {
    fn walls(&self, t: u32) -> Option<Wall> {
        let (half, alpha) = flash_size(t);
        if alpha <= 0.0 {
            return None;
        }
        let tint = [1.0, 0.0, 0.0, alpha / 255.0];
        let corner = |x: f32, z: f32, uv: [f32; 2]| MeshVertex { position: [self.at[0] + x, self.at[1] + 5.0, self.at[2] + z], uv, tint };
        let [a, b, c, d] = [corner(half, 0.0, [0.0, 1.0]), corner(0.0, half, [1.0, 1.0]), corner(-half, 0.0, [1.0, 0.0]), corner(0.0, -half, [0.0, 0.0])];
        // Drawn twice at the same place: double the brightness.
        let once = [a, b, c, a, c, d];
        Some(Wall { texture: FLASH, vertices: [once, once].concat(), additive: true, forced: false })
    }

    /// The potion and its three ghosts at tick t.
    fn potions(&self, t: u32) -> Vec<Potion> {
        let n = t + 1;
        let at = |n: u32| [self.at[0], self.at[1] + potion_height(n), self.at[2]];
        let mut potions = Vec::new();
        if (1..=71).contains(&n) {
            potions.push(Potion { icon: self.icon, at: at(n), spin: potion_spin(self.spin, n), alpha: potion_alpha(n) });
        }
        for k in 1..=3 {
            let Some(earlier) = n.checked_sub(k).filter(|&earlier| earlier >= 1) else { continue };
            let alpha = (potion_alpha(earlier) - (75 + 25 * k) as f32).max(0.0);
            if alpha > 0.0 && n <= 71 + k {
                potions.push(Potion { icon: self.icon, at: at(earlier), spin: potion_spin(self.spin, n), alpha });
            }
        }
        potions
    }
}

/// Full Protection's strobe ends after tick 30; the colour it leaves drifts back to white by 8 a channel a tick.
const STROBE_TICKS: u32 = 31;

impl Ground {
    /// Magnus' (318) or Sanctuary's (319) box over skill unit `id`'s cell, its phase from `rand(n)` (0 to n − 1).
    pub fn keep_box(&mut self, effect: u32, id: u32, at: Vector, now: Instant, min: bool, mut rand: impl FnMut(u32) -> u32) {
        let look = if effect == 318 { &MAGNUS } else { &SANCTUARY };
        let height = if effect == 318 && min { MAGNUS_MIN_HEIGHT } else { look.height };
        self.boxes.push(BoxCell { id, look, height, at, start: now, phase: rand(360) as f32 });
    }

    pub fn forget(&mut self, id: u32) {
        self.boxes.retain(|cell| cell.id != id);
    }

    /// A Triple Attack from `caster`'s feet toward `aim`: two sets of four streaks, made on ticks 0 and 1; each streak's
    /// choices from `rand(n)` (0 to n − 1) in the order the game draws them.
    pub fn triple(&mut self, caster: Vector, aim: Vector, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        // Straight from the caster to the aim, a whole number of degrees; 180 on the same spot.
        let (dx, dz) = (aim[0] - caster[0], aim[2] - caster[2]);
        let heading = if dx == 0.0 && dz == 0.0 { 180.0 } else { dz.atan2(dx).to_degrees().trunc() };
        let (sin, cos) = heading.to_radians().sin_cos();
        let mut streaks = Vec::new();
        for made in [0, 1] {
            for _ in 0..4 {
                let wait = -(rand(20) as i32);
                let x = caster[0] - 3.0 + 0.06 * rand(100) as f32;
                let z = caster[2] - 3.0 + 0.06 * rand(100) as f32;
                let back = 0.15 * rand(100) as f32;
                let height = 8.0 + 0.04 * rand(100) as f32;
                let length = 20.0 + 0.2 * rand(100) as f32;
                let start = [x - back * cos, caster[1], z - back * sin];
                let end = [start[0] + length * cos, caster[1], start[2] + length * sin];
                streaks.push(Streak { made, wait, start, end, height, heading });
            }
        }
        self.triples.push(Triple { start: now, streaks });
    }

    /// Slim Pitcher's potion (the icon's texture path) falling on the cell `at`; its spin from `rand(n)`.
    pub fn slim(&mut self, at: Vector, icon: &'static str, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        self.slims.push(Slim { at, start: now, icon, spin: rand(360) as f32 });
    }

    /// Full Protection's strobe on `unit` (None = the player).
    pub fn chemical(&mut self, unit: Option<u32>, now: Instant) {
        self.chemicals.push(Chemical { unit, start: now });
    }

    /// Drops what ended.
    pub fn advance(&mut self, now: Instant) {
        self.triples.retain(|triple| passes(triple.start, now) <= 100);
        self.slims.retain(|slim| passes(slim.start, now) <= 250);
        self.chemicals.retain(|chemical| passes(chemical.start, now) <= STROBE_TICKS + 40);
    }

    /// Every mesh to draw at `now`.
    pub fn meshes(&self, now: Instant) -> Vec<Wall> {
        let mut walls = Vec::new();
        walls.extend(self.boxes.iter().filter_map(|cell| cell.walls(passes(cell.start, now))));
        for triple in &self.triples {
            let Some(t) = passes(triple.start, now).checked_sub(1) else { continue };
            for streak in triple.streaks.iter().filter(|streak| t >= streak.made) {
                let a = streak.wait + (t - streak.made) as i32 + 1;
                let alpha = triple_alpha(a);
                if alpha > 0.0 {
                    walls.extend(streak.walls(alpha));
                }
            }
        }
        for slim in &self.slims {
            if let Some(t) = passes(slim.start, now).checked_sub(1)
                && let Some(wall) = slim.walls(t)
            {
                walls.push(wall);
            }
        }
        walls
    }

    /// The falling potions and their ghosts at `now`.
    pub fn potions(&self, now: Instant) -> Vec<Potion> {
        self.slims.iter().filter_map(|slim| Some(slim.potions(passes(slim.start, now).checked_sub(1)?))).flatten().collect()
    }

    /// The body flags and colour Full Protection gives `unit` on ticks 0 to 30: the outline copies every tick, and an additive
    /// blue body on the even ones.
    pub fn glow(&self, unit: Option<u32>, now: Instant) -> Option<(u32, Option<[u8; 3]>)> {
        self.chemicals.iter().filter(|chemical| chemical.unit == unit).find_map(|chemical| {
            let t = passes(chemical.start, now).checked_sub(1).filter(|&t| t < STROBE_TICKS)?;
            Some(if t % 2 == 0 { (crate::actor::GLOW_OUTLINE | crate::actor::GLOW_ADDITIVE, Some([0, 0, 255])) } else { (crate::actor::GLOW_OUTLINE, None) })
        })
    }

    /// The blue the body is left in after the strobe, drifting back to white by 8 a channel a tick.
    pub fn body_tint(&self, unit: Option<u32>, now: Instant) -> Option<[u8; 3]> {
        self.chemicals.iter().filter(|chemical| chemical.unit == unit).find_map(|chemical| {
            let t = passes(chemical.start, now).checked_sub(1).filter(|&t| t >= STROBE_TICKS)?;
            let drift = (t - STROBE_TICKS + 1) as u16 * 8;
            let tint = [drift.min(255) as u8, drift.min(255) as u8, 255];
            (tint != [255; 3]).then_some(tint)
        })
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
    fn magnus_and_sanctuary_stand_a_breathing_box_over_each_cell() {
        let start = Instant::now();
        let mut ground = Ground::default();
        ground.keep_box(318, 7, [10.0, 2.0, 20.0], start, false, |_| 0);
        ground.keep_box(319, 8, [30.0, 2.0, 20.0], start, false, |_| 0);
        // The top at a = 1 for phase 0: ((50 sin 1°) 0.35 + 32.5) sin 1°; at a = 90 h (0.65 + 0.35 sin phase).
        let want = (50.0 * 1.0_f32.to_radians().sin() * 0.35 + 32.5) * 1.0_f32.to_radians().sin();
        assert!(near(box_top(50.0, 0.0, 1), want));
        assert!(
            near(box_top(50.0, 0.0, 90), 50.0 * (0.65 + 0.35 * 90.0_f32.to_radians().sin()))
                && near(box_top(16.0, 0.0, 450), 16.0 * (0.65 + 0.35 * 90.0_f32.to_radians().sin()))
        );
        // Alpha 30 and 120 held to 864 and 1880 ticks, then 2 less a tick.
        assert_eq!([1, 864, 865, 879, 880].map(|a| box_alpha(30.0, 864, a)), [30.0, 30.0, 28.0, 0.0, 0.0]);
        assert_eq!(box_alpha(120.0, 1880, 1881), 118.0);
        // Four walls of 6 vertices; Magnus added on in white, Sanctuary blended in #EBFFEB.
        let walls = ground.meshes(start);
        assert_eq!(walls.len(), 2);
        assert!(walls.iter().all(|wall| wall.vertices.len() == 24));
        assert!(walls[0].texture == RING_RED && walls[0].additive && walls[0].vertices[0].tint[..3] == [1.0; 3]);
        assert!(walls[1].texture == ALPHA_DOWN && !walls[1].additive && walls[1].vertices[0].tint[..3] == [235.0 / 255.0, 1.0, 235.0 / 255.0]);
        assert_eq!((walls[0].vertices[0].tint[3] * 255.0).round(), 30.0);
        assert_eq!((walls[1].vertices[0].tint[3] * 255.0).round(), 120.0);
        // From 1 unit up to 1 + the top, half-side 5 and 2.5, whole texture on each wall.
        let wall = &walls[0].vertices;
        assert!(wall.iter().all(|vertex| near((vertex.position[0] - 10.0).abs(), 5.0) || near((vertex.position[2] - 20.0).abs(), 5.0)));
        assert!(
            near(wall[0].position[1], 3.0)
                && near(wall[2].position[1], 3.0 + want)
                && wall[0].uv == [0.0, 1.0]
                && wall[1].uv == [1.0, 1.0]
                && wall[2].uv == [1.0, 0.0]
                && wall[5].uv == [0.0, 0.0]
        );
        assert!(walls[1].vertices.iter().all(|vertex| near((vertex.position[0] - 30.0).abs(), 2.5) || near((vertex.position[2] - 20.0).abs(), 2.5)));
        // Under /mineffect Magnus is 10 high; the box goes with its unit.
        let mut small = Ground::default();
        small.keep_box(318, 9, [0.0; 3], start, true, |_| 90);
        assert_eq!(small.boxes[0].height, 10.0);
        assert_eq!(small.boxes[0].phase, 90.0);
        ground.forget(7);
        assert_eq!(ground.meshes(start).len(), 1);
    }

    #[test]
    fn triple_attack_throws_eight_streaks_in_two_sets() {
        let start = Instant::now();
        let mut ground = Ground::default();
        ground.triple([10.0, 1.0, 10.0], [20.0, 1.0, 10.0], start, |_| 0);
        assert_eq!(ground.triples[0].streaks.len(), 8);
        assert_eq!(ground.triples[0].streaks.iter().filter(|streak| streak.made == 0).count(), 4);
        // Every draw 0: the streaks start 3 behind and to the side of the caster, 8 up, and end 20 on along the heading (east).
        let streak = &ground.triples[0].streaks[0];
        assert!(near(streak.start[0], 7.0) && near(streak.start[2], 7.0) && streak.start[1] == 1.0 && streak.height == 8.0);
        assert!(near(streak.end[0] - streak.start[0], 20.0) && near(streak.end[2], streak.start[2]));
        // Alpha 8 at a = 1, 40 at 5, 36 at 6, 4 at 14, 0 at 15.
        assert_eq!([1, 5, 6, 14, 15].map(triple_alpha), [8.0, 40.0, 36.0, 4.0, 0.0]);
        // Tick 0 shows the first four (two passes each), tick 1 all eight; each streak lives 14 draws.
        let count = |tick: u32| ground.meshes(start + TICK * tick).len();
        assert_eq!((count(0), count(1)), (8, 16));
        assert_eq!(count(13), 16);
        assert_eq!(count(14), 8, "the first set's 15th");
        assert_eq!(count(15), 0);
        // A pentagon tube: 5 quads of 6 vertices, in two passes: thin blended yellow, fat added pale.
        let walls = ground.meshes(start);
        assert!(walls[0].vertices.len() == 30 && !walls[0].additive && walls[1].additive && walls[0].texture == CLOUD);
        assert_eq!(walls[0].vertices[0].tint[..3], [1.0, 1.0, 55.0 / 255.0]);
        assert_eq!(walls[1].vertices[0].tint[..3], [1.0, 1.0, 200.0 / 255.0]);
        assert_eq!((walls[0].vertices[0].tint[3] * 255.0).round(), 8.0);
        // On the same spot the heading is 180°: the streaks run west.
        let mut here = Ground::default();
        here.triple([0.0; 3], [0.0; 3], start, |_| 0);
        assert!(here.triples[0].streaks[0].end[0] < here.triples[0].streaks[0].start[0]);
    }

    #[test]
    fn slim_pitcher_drops_a_potion_with_three_ghosts_and_flashes() {
        let start = Instant::now();
        let mut ground = Ground::default();
        ground.slim([5.0, 2.0, 5.0], RED_POTION, start, |_| 100);
        let potions = |tick: u32| ground.potions(start + TICK * tick);
        // Tick: potion height, alpha, then the ghosts' alpha.
        let line = |tick: u32| potions(tick).iter().map(|potion| (potion.at[1] - 2.0, potion.alpha)).collect::<Vec<_>>();
        assert_eq!(line(0), [(77.0, 20.0)]);
        assert_eq!(line(5), [(62.0, 120.0)]);
        assert_eq!(potions(9).iter().map(|potion| potion.alpha).collect::<Vec<_>>(), [200.0, 80.0, 35.0]);
        assert_eq!(potions(12).iter().map(|potion| potion.alpha).collect::<Vec<_>>(), [200.0, 100.0, 75.0, 50.0]);
        assert_eq!(line(23)[0], (8.0, 200.0));
        assert_eq!(line(32)[0], (8.0, 195.0));
        // Ghost 1 sits where the potion was a tick earlier.
        assert_eq!(potions(12)[1].at[1] - 2.0, potion_height(12));
        // Drawn for n = 1…71: nothing at tick 71 (n = 72) but the ghosts' tail, and none after tick 74.
        assert!(potions(70).iter().any(|potion| potion.alpha == 5.0));
        assert!(potions(71).iter().all(|potion| potion.alpha < 5.0));
        assert!(potions(75).is_empty());
        // The spin goes down 15° a tick to n = 24, then stays.
        assert_eq!([1, 2, 24, 25, 40].map(|n| potion_spin(100.0, n)), [85.0, 70.0, 100.0, 100.0, 100.0]);
        // The flash: 0.9 r is 13.5 to tick 15, 14.445 at 16, 26.6 at 25; alpha 20 at n = 10, 0 at n = 30; two quads, 5 up.
        assert!(near(flash_size(15).0, 13.5) && near(flash_size(16).0, 14.445) && (flash_size(25).0 - 26.56).abs() < 0.05);
        assert_eq!([15, 16, 25, 26, 44, 45].map(|t| flash_size(t).1), [0.0, 2.0, 20.0, 19.0, 1.0, 0.0]);
        let flash = ground.meshes(start + TICK * 25);
        assert_eq!(flash.len(), 1);
        assert!(
            flash[0].texture == FLASH && flash[0].additive && flash[0].vertices.len() == 12 && flash[0].vertices.iter().all(|vertex| vertex.position[1] == 7.0)
        );
        assert_eq!(flash[0].vertices[0].tint[..3], [1.0, 0.0, 0.0]);
        assert!(ground.meshes(start + TICK * 15).is_empty() && ground.meshes(start + TICK * 45).is_empty());
        ground.advance(start + TICK * 251);
        assert!(ground.slims.is_empty());
    }

    #[test]
    fn a_padded_square_puts_the_picture_on_the_middle() {
        // Unspun: the whole texture is a third bigger; the middle of its top-left three quarters is on the origin.
        let near_pair = |got: [f32; 2], want: [f32; 2]| (got[0] - want[0]).abs() < 0.01 && (got[1] - want[1]).abs() < 0.01;
        let [c0, c1, c2, c3] = padded_square(0.0, 75.0, false);
        // Half-diagonal 100: corners at 0°, 90°, 180°, 270° from a centre moved 25 toward 90° (down).
        assert!(near_pair(c0, [100.0, 25.0]) && near_pair(c1, [0.0, 125.0]) && near_pair(c2, [-100.0, 25.0]) && near_pair(c3, [0.0, -75.0]));
        // The picture's centre, three eighths in from the (0, 0) corner on both axes, lies on the origin: the (0, 0) corner is
        // c3 (uv 0, 0) and the (1, 1) corner c1; the point 0.375 of the way from c3 to c1 is the origin.
        let at = [c3[0] + 0.375 * (c1[0] - c3[0]), c3[1] + 0.375 * (c1[1] - c3[1])];
        assert!(near_pair(at, [0.0, 0.0]), "{at:?}");
        // Mirrored: the corner at the spin gets (1, 1), listed second; (0, 0) is the fourth, 0.375 of the way to it is the origin.
        let [m0, m1, m2, m3] = padded_square(0.0, 75.0, true);
        let at = [m3[0] + 0.375 * (m1[0] - m3[0]), m3[1] + 0.375 * (m1[1] - m3[1])];
        assert!(
            near_pair(at, [0.0, 0.0])
                && near_pair(m1, [125.0, 0.0])
                && near_pair(m0, [25.0, 100.0])
                && near_pair(m2, [25.0, -100.0])
                && near_pair(m3, [-75.0, 0.0])
        );
    }

    #[test]
    fn full_protection_strobes_the_body_blue_for_31_ticks_then_lets_it_drift_back() {
        let start = Instant::now();
        let mut ground = Ground::default();
        ground.chemical(Some(3), start);
        let glow = |tick: u32| ground.glow(Some(3), start + TICK * tick);
        // The outline copies every tick; an additive blue body on the even ones, a white one on the odd ones.
        assert_eq!(glow(0), Some((crate::actor::GLOW_OUTLINE | crate::actor::GLOW_ADDITIVE, Some([0, 0, 255]))));
        assert_eq!(glow(1), Some((crate::actor::GLOW_OUTLINE, None)));
        assert_eq!(glow(30), glow(0));
        assert!((0..31).all(|tick| glow(tick).is_some()));
        assert_eq!(glow(31), None);
        // The colour it is left in drifts back by 8 a channel a tick.
        let tint = |tick: u32| ground.body_tint(Some(3), start + TICK * tick);
        assert_eq!((tint(30), tint(31), tint(32), tint(61)), (None, Some([8, 8, 255]), Some([16, 16, 255]), Some([248, 248, 255])));
        assert_eq!(tint(62), None);
        // Other units are not touched.
        assert!(ground.glow(None, start).is_none() && ground.body_tint(Some(4), start + TICK * 31).is_none());
    }
}
