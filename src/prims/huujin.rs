//! Huujin's wind blades (NJ_HUUJIN, effect 621; the game's prim kind 125): a blade a hit leaves the caster, curves in
//! toward the target from 25° to 45° off to one side, fades once it lines up (or at tick 21) and flies on past; each
//! blade is a flat square with three copies trailing it.

use std::time::Instant;

use super::{passes, tanji::bearing};
use crate::world::{EffectMesh, MeshVertex, Vector};

pub const WIND: &str = "data\\texture\\effect\\270바람.tga";
/// The effect's length in ticks, and how high over the caster's feet a blade flies.
const LENGTH: u32 = 200;
const RISE: f32 = 10.0;
/// The squares' alpha: up 8 a tick to 120 for ticks 2…20, then down 5 a tick; the copies are 0.8 of what they copy.
const RAMP: f32 = 8.0;
const TOP: f32 = 120.0;
const FADE: f32 = 5.0;
/// The turn a tick: 5° steering in, 1° fading; and how near the bearing (3°) it lines up.
const STEER: i32 = 5;
const FADING_STEER: i32 = 1;
const LINED_UP: i32 = 3;

/// One of a blade's four squares: the blade itself (0) and its trail.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Element {
    at: Vector,
    spin: f32,
    alpha: f32,
    size: f32,
    speed: f32,
    on: bool,
}

struct Blade {
    start: Instant,
    /// Which way it curves: 0 or 1.
    side: usize,
    /// The unit it flies at (None = the local player), by the AID as the game keeps it, through an f32.
    unit: Option<u32>,
    ticks: u32,
    heading: i32,
    fading: bool,
    squares: [Element; 4],
}

impl Blade {
    /// The blade's `tick` for k = ticks + 1: `target` is where its target stands now, `rand(n)` (0 to n − 1) the heading's
    /// draw.
    fn step(&mut self, target: Option<Vector>, mut rand: impl FnMut(u32) -> u32) {
        self.ticks += 1;
        let k = self.ticks;
        if !self.squares[0].on {
            return;
        }
        if target.is_none() {
            self.fading = true;
            if k == 1 {
                return;
            }
        }
        let side = if self.side == 0 { 1 } else { -1 };
        self.squares[0].spin = (self.squares[0].spin + 30.0).rem_euclid(360.0);
        if k == 1 {
            if let Some(at) = target {
                let toward = bearing(at[0] - self.squares[0].at[0], at[2] - self.squares[0].at[2]);
                // Side 0 turns +25…45 off the line, side 1 −25…45.
                self.heading = (toward + side * (25 + rand(21) as i32)).rem_euclid(360);
            }
        } else if k <= 20 {
            self.squares[0].alpha = (self.squares[0].alpha + RAMP).min(TOP);
        } else {
            self.fading = true;
        }
        if k > 5 {
            if !self.fading {
                if let Some(at) = target {
                    let on = bearing(at[0] - self.squares[0].at[0], at[2] - self.squares[0].at[2]);
                    // Lined up within 3° (plain numbers, no wrap round 360): it fades, no turn this tick.
                    if (on - LINED_UP..=on + LINED_UP).contains(&self.heading) {
                        self.fading = true;
                    } else {
                        self.heading = (self.heading - side * STEER).rem_euclid(360);
                    }
                }
            } else {
                for square in &mut self.squares {
                    square.alpha -= FADE;
                    if square.alpha < 0.0 {
                        square.alpha = 0.0;
                        square.on = false;
                    }
                }
                self.heading = (self.heading - side * FADING_STEER).rem_euclid(360);
            }
        }
        let (sin, cos) = f64::from(self.heading).to_radians().sin_cos();
        let speed = self.squares[0].speed;
        self.squares[0].at[0] += speed * cos as f32;
        self.squares[0].at[2] += speed * sin as f32;
        if k.is_multiple_of(3) {
            for j in (1..4).rev() {
                let from = self.squares[j - 1];
                self.squares[j] = Element { at: from.at, spin: from.spin, alpha: (from.alpha * 0.8).trunc(), ..self.squares[j] };
            }
        }
    }
}

/// The blades in flight.
#[derive(Default)]
pub struct Huujin {
    blades: Vec<Blade>,
}

impl Huujin {
    /// `count` blades from the caster's feet `from` at the unit with AID `target` (the local player's being `own`), from
    /// `now`; every random choice from `rand(n)` (0 to n − 1), the squares of every blade drawn first.
    // ponytail: the AID goes through an f32 as the game keeps it: above 16 777 216 it rounds, so a server whose ids pass that
    // finds nobody and the blades fly toward +x; if the ids stay below 2²⁴ it never shows.
    pub fn throw(&mut self, from: Vector, count: u16, target: u32, own: u32, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        let unit = if target as f32 == own as f32 { None } else { Some(target as f32 as u32) };
        let at = [from[0], from[1] + RISE, from[2]];
        for index in 0..count.max(1) {
            let squares = std::array::from_fn(|j| {
                let size = rand(11) as f32 * 0.5 + 10.0;
                let speed = rand(21) as f32 * 0.1 + 1.0;
                Element { at, spin: j as f32, alpha: 0.0, size, speed, on: true }
            });
            self.blades.push(Blade { start: now, side: usize::from(index % 2), unit, ticks: 0, heading: 0, fading: false, squares });
        }
    }

    /// Runs the blades up to `now`, a tick at a time, finding their targets with `position_of`.
    pub fn advance(&mut self, now: Instant, position_of: impl Fn(Option<u32>) -> Option<Vector>, mut rand: impl FnMut(u32) -> u32) {
        for blade in &mut self.blades {
            while blade.ticks < passes(blade.start, now).min(LENGTH) {
                blade.step(position_of(blade.unit), &mut rand);
            }
        }
        self.blades.retain(|blade| passes(blade.start, now) <= LENGTH && blade.squares[0].on);
    }

    /// The squares to draw, flat at their blades' height, the blade first: green, added on.
    pub fn meshes<'a>(&self, texture: Option<&'a crate::render::Texture>) -> Vec<EffectMesh<'a>> {
        let Some(texture) = texture else { return Vec::new() };
        let mut meshes = Vec::new();
        for blade in self.blades.iter().filter(|blade| blade.ticks > 0) {
            for square in blade.squares.iter().filter(|square| square.on && square.alpha > 0.0) {
                let tint = [80.0 / 255.0, 1.0, 80.0 / 255.0, square.alpha / 255.0];
                let uvs = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
                let corners: Vec<MeshVertex> = (0..4)
                    .map(|j| {
                        let (sin, cos) = (square.spin + 90.0 * (j + 1) as f32).to_radians().sin_cos();
                        MeshVertex { position: [square.at[0] + square.size * cos, square.at[1], square.at[2] + square.size * sin], uv: uvs[j], tint }
                    })
                    .collect();
                meshes.push((texture, vec![corners[0], corners[1], corners[2], corners[0], corners[2], corners[3]], true));
            }
        }
        meshes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A single blade at the origin at 10 up, side `side`, speed from `speed_draw`, a target 30 along +x (or none), run
    /// `ticks` ticks; every R(11) is 0 and the heading's R(21) is `heading_draw`.
    fn blade(side: usize, speed_draw: u32, heading_draw: u32, target: bool, ticks: u32) -> Blade {
        let mut huujin = Huujin::default();
        let mut draws = [0, speed_draw].into_iter().cycle();
        huujin.throw([0.0, -10.0, 0.0], (side + 1) as u16, 7, 1, Instant::now(), |_| draws.next().unwrap());
        let mut blade = huujin.blades.remove(side);
        for _ in 0..ticks {
            blade.step(target.then_some([30.0, 0.0, 0.0]), |_| heading_draw);
        }
        blade
    }

    fn off_tick(side: usize, speed_draw: u32, heading_draw: u32, target: bool, element: usize) -> u32 {
        let mut blade = blade(side, speed_draw, heading_draw, target, 0);
        (1..100)
            .find(|_| {
                blade.step(target.then_some([30.0, 0.0, 0.0]), |_| heading_draw);
                !blade.squares[element].on
            })
            .map_or(0, |_| blade.ticks)
    }

    #[test]
    fn makes_a_blade_a_hit_on_alternate_sides() {
        let mut huujin = Huujin::default();
        let mut draws = 0;
        huujin.throw([0.0; 3], 6, 7, 1, Instant::now(), |_| {
            draws += 1;
            0
        });
        assert_eq!(huujin.blades.iter().map(|blade| blade.side).collect::<Vec<_>>(), [0, 1, 0, 1, 0, 1]);
        // Six blades of four squares, each drawing R(11) and R(21): 48 draws before any tick; a size 10, a speed 1.
        assert_eq!(draws, 6 * 4 * 2);
        assert_eq!((huujin.blades[0].squares[0].size, huujin.blades[0].squares[0].speed, huujin.blades[0].squares[3].spin), (10.0, 1.0, 3.0));
        assert_eq!(huujin.blades[0].squares[0].at, [0.0, 10.0, 0.0]);
    }

    #[test]
    fn finds_its_target_through_an_f32() {
        let mut huujin = Huujin::default();
        // The own AID is the player (None); another unit keeps its id; above 2²⁴ an id rounds to a multiple of 8.
        huujin.throw([0.0; 3], 1, 2_000_000, 2_000_000, Instant::now(), |_| 0);
        huujin.throw([0.0; 3], 1, 2_000_001, 2_000_000, Instant::now(), |_| 0);
        huujin.throw([0.0; 3], 1, 110_000_001, 2_000_000, Instant::now(), |_| 0);
        assert_eq!(huujin.blades.iter().map(|blade| blade.unit).collect::<Vec<_>>(), [None, Some(2_000_001), Some(110_000_000)]);
    }

    #[test]
    fn curves_in_fades_and_flies_on() {
        // Side 0, R(21) = 0, speed 2.0 (draw 10): e0's alpha 120 at tick 20; e3, e2, e1, e0 off at ticks 39, 42, 44, 45.
        assert_eq!(blade(0, 10, 0, true, 20).squares[0].alpha, 120.0);
        assert_eq!([3, 2, 1, 0].map(|element| off_tick(0, 10, 0, true, element)), [39, 42, 44, 45]);
        // Side 1, R(21) = 20, speed 1.0: lines up at tick 19 (fading from the next), alpha 115 at tick 20, off at 39, 42, 43, 44.
        let lined = blade(1, 0, 20, true, 19);
        assert!(lined.fading && !blade(1, 0, 20, true, 18).fading);
        assert_eq!(blade(1, 0, 20, true, 20).squares[0].alpha, 115.0);
        assert_eq!([3, 2, 1, 0].map(|element| off_tick(1, 0, 20, true, element)), [39, 42, 43, 44]);
        // No target: nothing at tick 1, e2 and e3 off at tick 6, alpha 77 at tick 20, e1 and e0 off at 35 and 36.
        assert_eq!(blade(0, 0, 0, false, 1).squares[0].spin, 0.0);
        assert_eq!([3, 2, 1, 0].map(|element| off_tick(0, 0, 0, false, element)), [6, 6, 35, 36]);
        assert_eq!(blade(0, 0, 0, false, 20).squares[0].alpha, 77.0);
    }

    #[test]
    fn a_target_lost_fades_for_good() {
        // Found at ticks 1…2, lost from tick 3: fading begins that tick and the later return doesn't clear it.
        let mut blade = blade(0, 10, 0, true, 2);
        assert!(!blade.fading);
        blade.step(None, |_| 0);
        assert!(blade.fading);
        for _ in 0..4 {
            blade.step(Some([30.0, 0.0, 0.0]), |_| 0);
        }
        assert!(blade.fading, "never cleared");
        // An element that is off stays off after a later copy.
        blade.squares[2].on = false;
        blade.squares[2].alpha = 0.0;
        blade.squares[1].alpha = 50.0;
        blade.ticks = 5;
        blade.step(Some([30.0, 0.0, 0.0]), |_| 0);
        assert!(!blade.squares[2].on);
    }
}
