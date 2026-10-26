//! The Clown's, Gypsy's and Rogue's skills' code-made shapes, a 16 ms tick at a time: Moonlit Water Mill's twenty tilted fire
//! crescents (394) and Spider Web's flat web on the ground (404).
//!
//! A shape's own count n is the ticks run, 1 on its first tick.

use std::time::Instant;

use super::passes;
use super::walls::Wall;
use crate::world::{MeshVertex, Vector};

pub const FIRE: &str = "data\\texture\\effect\\fire_ivy.bmp";
pub const WEB: &str = "data\\texture\\effect\\spiderweb.tga";
/// The crescents' colour (#FF59B6), radius, centre height and alpha.
const PINK: [f32; 3] = [1.0, 89.0 / 255.0, 182.0 / 255.0];
const MILL_RADIUS: f32 = 14.0;
const MILL_HEIGHT: f32 = 14.0;
const MILL_ALPHA: f32 = 100.0;
/// The web is gone after this many ticks.
const WEB_TICKS: u32 = 29_999;
/// Spider Web is a status too.
pub const SPIDER_STATUS: u16 = 735;

/// One crescent: half its width, its turn, its arc and its speed, and the tilt of its axis.
struct Band {
    half: f32,
    turn: f32,
    arc: f32,
    speed: f32,
    tilt: f32,
}

/// Moonlit Water Mill's twenty crescents round a dancer.
struct Mill {
    unit: Option<u32>,
    status: Option<u16>,
    start: Instant,
    bands: Vec<Band>,
}

/// A web on a cell (a skill unit's) or following a unit with the status.
struct Web {
    id: Option<u32>,
    unit: Option<Option<u32>>,
    at: Vector,
    start: Instant,
    spin: f32,
}

#[derive(Default)]
pub struct Dancer {
    mills: Vec<Mill>,
    webs: Vec<Web>,
}

/// A web's half-diagonal at its spin: 9.6 to 12.0, a 360-tick breath.
pub fn web_half(spin: f32) -> f32 {
    (spin.to_radians().sin() + 1.0) * 1.2 + 9.6
}

/// The points a crescent of arc `arc` draws: every ⌊arc/20⌋ degrees while they fit the arc.
pub fn mill_points(arc: f32) -> usize {
    let step = (arc / 20.0).floor();
    (0..).take_while(|s| step * *s as f32 <= arc).count()
}

impl Band {
    fn walls(&self, feet: Vector, n: u32) -> Vec<MeshVertex> {
        let turn = (self.turn + self.speed * n as f32) % 360.0;
        let step = (self.arc / 20.0).floor();
        let (tilt_sin, tilt_cos) = self.tilt.to_radians().sin_cos();
        let tint = [PINK[0], PINK[1], PINK[2], MILL_ALPHA / 255.0];
        // The edges at each point: the centre of the cylinder's strip there, moved along the axis either way.
        let edge = |s: usize, side: f32| {
            let (sin, cos) = ((turn + step * s as f32) % 360.0).to_radians().sin_cos();
            let centre = [MILL_RADIUS * tilt_cos * cos, MILL_HEIGHT - MILL_RADIUS * tilt_sin * cos, MILL_RADIUS * sin];
            [feet[0] + centre[0] - side * self.half * tilt_sin, feet[1] + centre[1] - side * self.half * tilt_cos, feet[2] + centre[2]]
        };
        let mut vertices = Vec::new();
        for s in 1..mill_points(self.arc) {
            // Only 21 points exist: a band with more repeats the last quad.
            let (before, after, j) = if s > 20 { (19, 20, 19) } else { (s - 1, s, s - 1) };
            let u = [j as f32 / 21.0, (j + 1) as f32 / 21.0];
            let [a, b, c, d] = [
                MeshVertex { position: edge(before, 1.0), uv: [u[0], 1.0], tint },
                MeshVertex { position: edge(after, 1.0), uv: [u[1], 1.0], tint },
                MeshVertex { position: edge(after, -1.0), uv: [u[1], 0.0], tint },
                MeshVertex { position: edge(before, -1.0), uv: [u[0], 0.0], tint },
            ];
            vertices.extend([a, b, c, a, c, d]);
        }
        vertices
    }
}

impl Dancer {
    /// Moonlit Water Mill on `unit` (for `status`, when a status shows it): 5 prims of 4 bands made now, each band's half-width,
    /// turn, arc and speed from `rand(n)` (0 to n − 1).
    pub fn mill(&mut self, unit: Option<u32>, status: Option<u16>, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        let mut bands = Vec::with_capacity(20);
        for phase in [0, 9, 18, 27, 36] {
            for k in 0..4 {
                let half = rand(11) as f32 * 0.2 + 4.0;
                let turn = rand(360) as f32;
                let arc = rand(91) as f32 + 180.0;
                let speed = rand(4) as f32 + 3.0;
                bands.push(Band { half, turn, arc, speed, tilt: (45 * k + phase) as f32 });
            }
        }
        self.mills.push(Mill { unit, status, start: now, bands });
    }

    /// Whether `unit` has a mill already.
    pub fn has_mill(&self, unit: Option<u32>) -> bool {
        self.mills.iter().any(|mill| mill.unit == unit)
    }

    /// A web over a cell, for skill unit `id`, or following `unit` while it has the status; its spin from `rand(n)`.
    pub fn web(&mut self, id: Option<u32>, unit: Option<Option<u32>>, at: Vector, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        self.webs.push(Web { id, unit, at, start: now, spin: rand(360) as f32 });
    }

    pub fn forget(&mut self, id: u32) {
        self.webs.retain(|web| web.id != Some(id));
    }

    /// A status coming on again or going off takes its mills and webs.
    pub fn forget_status(&mut self, unit: Option<u32>, status: u16) {
        self.mills.retain(|mill| !(mill.unit == unit && mill.status == Some(status)));
        if status == SPIDER_STATUS {
            self.webs.retain(|web| web.unit != Some(unit));
        }
    }

    pub fn advance(&mut self, now: Instant) {
        self.webs.retain(|web| passes(web.start, now) <= WEB_TICKS);
    }

    /// The mills' crescents and the webs at `now`; `position_of` gives a unit's feet.
    pub fn meshes(&self, now: Instant, position_of: impl Fn(Option<u32>) -> Option<Vector>) -> Vec<Wall> {
        let mut walls = Vec::new();
        for mill in &self.mills {
            let (Some(feet), n) = (position_of(mill.unit), passes(mill.start, now)) else { continue };
            if n >= 1 {
                walls.extend(mill.bands.iter().map(|band| Wall { texture: FIRE, vertices: band.walls(feet, n), additive: true, forced: false }));
            }
        }
        for web in &self.webs {
            let n = passes(web.start, now);
            let Some(at) = web.unit.map_or(Some(web.at), &position_of).filter(|_| n >= 1) else { continue };
            let spin = (web.spin + n as f32) % 360.0;
            let half = web_half(spin);
            let tint = [1.0, 1.0, 1.0, 100.0 / 255.0];
            let corner = |x: f32, z: f32, uv: [f32; 2]| MeshVertex { position: [at[0] + x, at[1] + 2.0, at[2] + z], uv, tint };
            let [c0, c1, c2, c3] =
                [corner(half, 0.0, [0.0, 1.0]), corner(0.0, half, [1.0, 1.0]), corner(-half, 0.0, [1.0, 0.0]), corner(0.0, -half, [0.0, 0.0])];
            walls.push(Wall { texture: WEB, vertices: vec![c0, c1, c2, c3, c2, c0], additive: true, forced: false });
        }
        walls
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
    fn moonlit_water_mill_fans_twenty_pink_crescents() {
        let start = Instant::now();
        let mut dancer = Dancer::default();
        dancer.mill(Some(1), Some(123), start, |_| 0);
        let bands = &dancer.mills[0].bands;
        assert_eq!(bands.len(), 20);
        // The tilts 0, 9 … 171, each once.
        let mut tilts: Vec<f32> = bands.iter().map(|band| band.tilt).collect();
        tilts.sort_by(f32::total_cmp);
        assert_eq!(tilts, (0..20).map(|k| 9.0 * k as f32).collect::<Vec<_>>());
        // Every draw 0: half-width 4.0, turn 0, arc 180, speed 3: the turn is 3 after one tick and 0 after 120.
        assert!(bands.iter().all(|band| band.half == 4.0 && band.turn == 0.0 && band.arc == 180.0 && band.speed == 3.0));
        assert_eq!(mill_points(180.0), 21);
        assert_eq!(mill_points(259.0), 22);
        let feet = [10.0, 1.0, 10.0];
        let walls = dancer.meshes(start, |_| Some(feet));
        assert_eq!(walls.len(), 20);
        assert!(walls.iter().all(|wall| wall.texture == FIRE && wall.additive && wall.vertices.len() == 20 * 6));
        // Pink #FF59B6, alpha 100 at every tick.
        assert_eq!(walls[0].vertices[0].tint, [1.0, 89.0 / 255.0, 182.0 / 255.0, 100.0 / 255.0]);
        assert_eq!(dancer.meshes(start + TICK * 500, |_| Some(feet))[0].vertices[0].tint[3], 100.0 / 255.0);
        // The first band (tilt 0) is a strip 14 from the axis and 14 up: its first edge point after one tick is turned 3°.
        let first = &walls[0].vertices[0].position;
        let (sin, cos) = 3.0_f32.to_radians().sin_cos();
        assert!(near(first[0], 10.0 + 14.0 * cos) && near(first[2], 10.0 + 14.0 * sin) && near(first[1], 1.0 + 14.0 - 4.0));
        // After 120 ticks it is back where it began (360° at 3° a tick): the same point as at tick −1, i.e. count 120.
        let back = dancer.meshes(start + TICK * 119, |_| Some(feet))[0].vertices[0].position;
        assert!(near(back[0], 10.0 + 14.0) && near(back[2], 10.0));
        // A band with 22 points repeats the quad j = 19.
        let mut wide = Dancer::default();
        wide.mill(None, None, start, |bound| if bound == 91 { 79 } else { 0 });
        assert_eq!(wide.mills[0].bands[0].arc, 259.0);
        let vertices = wide.meshes(start, |_| Some(feet))[0].vertices.clone();
        assert_eq!(vertices.len(), 21 * 6);
        assert!(near(vertices[20 * 6].uv[0], 19.0 / 21.0));
        // A status packet replaces rather than stacks; a unit that already has one is known.
        assert!(dancer.has_mill(Some(1)) && !dancer.has_mill(Some(2)));
        dancer.forget_status(Some(1), 123);
        assert!(!dancer.has_mill(Some(1)));
    }

    #[test]
    fn a_spider_web_lies_on_the_cell_and_breathes() {
        let start = Instant::now();
        let mut dancer = Dancer::default();
        // Start spin 90: half-diagonal 12.0; 270: 9.6; one tick after 90, just under 12.0.
        assert!(near(web_half(90.0), 12.0) && near(web_half(270.0), 9.6));
        assert!((web_half(91.0) - 11.9998).abs() < 0.001);
        dancer.web(Some(8), None, [5.0, 0.0, 5.0], start, |_| 90);
        let walls = dancer.meshes(start, |_| None);
        assert_eq!(walls.len(), 1);
        // Six vertices, added on, alpha 100, 2 up, corners on the axes.
        assert!(walls[0].texture == WEB && walls[0].additive && walls[0].vertices.len() == 6);
        assert!(walls[0].vertices.iter().all(|vertex| vertex.position[1] == 2.0 && vertex.tint == [1.0, 1.0, 1.0, 100.0 / 255.0]));
        assert!(near(walls[0].vertices[0].position[0], 5.0 + web_half(91.0)) && near(walls[0].vertices[0].position[2], 5.0));
        assert_eq!(walls[0].vertices[0].uv, [0.0, 1.0]);
        // Gone after 29 999 ticks, or with its unit.
        assert_eq!(dancer.meshes(start + TICK * 29_998, |_| None).len(), 1);
        dancer.advance(start + TICK * 30_000);
        assert!(dancer.webs.is_empty());
        dancer.web(Some(8), None, [0.0; 3], start, |_| 0);
        dancer.forget(8);
        assert!(dancer.webs.is_empty());
        // On a unit with EFST 735 it follows the unit and goes with the status.
        dancer.web(None, Some(Some(3)), [0.0; 3], start, |_| 0);
        assert_eq!(dancer.meshes(start, |unit| (unit == Some(3)).then_some([7.0, 0.0, 7.0])).len(), 1);
        assert!(dancer.meshes(start, |_| None).is_empty());
        dancer.forget_status(Some(3), 735);
        assert!(dancer.webs.is_empty());
    }
}
