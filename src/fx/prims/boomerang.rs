//! Shield Boomerang's shield (EF_SHIELDBOOMERANG 249), as the game moves it a 16 ms tick at a time: a spinning shield
//! flies out toward the target, loops back counter-clockwise, flies on past the caster and fades, three fading copies
//! behind it.

use std::time::Instant;

use super::passes;
use crate::world::{EffectQuad, Vector};

pub const SHIELD: &str = "data\\texture\\effect\\shield_boomerang.bmp";
/// Tomahawk's axe, thrown the way the shield is.
pub const AXE: &str = "data\\texture\\effect\\토마.bmp";
/// The effect's length in ticks.
const LIFE: u32 = 499;
/// How high over the caster's feet it starts, and how far it flies a tick.
const RISE: f32 = 8.0;
const SPEED: f32 = 2.0;
/// The turn: degrees a tick, and how near the way to the caster it flies back on.
const TURN: i32 = 5;
const CLOSE: i32 = 4;
/// Flying back it fades this much a tick (of 255); it spins this many degrees a tick.
const FADE: f32 = 12.0;
const SPIN: f32 = -15.0;
/// Every third tick the copies move up a place; copy 1's alpha is the shield's less 150, each later copy's 25 less.
const COPY_EVERY: u32 = 3;
const COPY_BELOW: [f32; 2] = [150.0, 25.0];
/// On ecl_fild01 every alpha over 100 is cut to 100.
const DIM_ALPHA: f32 = 100.0;
/// The square's corners lie this far (world units) from its centre; a world unit is 7 sprite pixels.
const RADIUS: f32 = 7.0;
const PIXELS: f32 = 35.0 / crate::world::CELL;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Phase {
    Out,
    Turn,
    Back,
}

/// A copy of the shield where it was: its place and spin.
#[derive(Clone, Copy)]
struct Copy {
    at: Vector,
    spin: f32,
}

struct Shield {
    /// Tomahawk's: the axe's picture, not the shield's.
    axe: bool,
    start: Instant,
    /// The caster (None = the player), whose place it turns back to.
    unit: Option<u32>,
    /// Ticks run (n).
    ticks: u32,
    at: Vector,
    /// How far it climbs a tick, and the distance across the ground to the target.
    climb: f32,
    reach: f32,
    /// Whole degrees: x = cos, z = sin.
    heading: i32,
    phase: Phase,
    alpha: f32,
    spin: f32,
    copies: [Option<Copy>; 3],
}

/// A heading in whole degrees toward (`dx`, `dz`), as the game finds it: the first whole degree from 0 whose cosine is
/// no more than the direction's x, then 360 less that when z is below 0. So straight along +z is 91.
fn heading(dx: f32, dz: f32) -> i32 {
    let length = f64::hypot(f64::from(dx), f64::from(dz));
    if length == 0.0 {
        return 0;
    }
    let x = f64::from(dx) / length;
    let degrees = (0..=180).find(|&degrees| f64::from(degrees).to_radians().cos() <= x).unwrap_or(180);
    if dz < 0.0 { 360 - degrees } else { degrees }
}

impl Shield {
    fn new(unit: Option<u32>, from: Vector, to: Vector, now: Instant, dim: bool) -> Self {
        let reach = f32::hypot(to[0] - from[0], to[2] - from[2]);
        let climb = if reach > 0.0 { SPEED * (to[1] - from[1]) / reach } else { 0.0 };
        Self {
            axe: false,
            start: now,
            unit,
            ticks: 0,
            at: [from[0], from[1] + RISE, from[2]],
            climb,
            reach,
            heading: heading(to[0] - from[0], to[2] - from[2]),
            phase: Phase::Out,
            alpha: if dim { DIM_ALPHA } else { 255.0 },
            spin: 0.0,
            copies: [None; 3],
        }
    }

    /// One tick, the caster at `caster` (x, z): the copies move up a place on every third, then it moves by its phase.
    fn step(&mut self, caster: Option<[f32; 2]>) {
        self.ticks += 1;
        if self.ticks.is_multiple_of(COPY_EVERY) {
            self.copies = [Some(Copy { at: self.at, spin: self.spin }), self.copies[0], self.copies[1]];
        }
        match self.phase {
            Phase::Out => self.at[1] += self.climb,
            Phase::Turn => {
                // Back on its way once within 4° of the way to the caster; otherwise it turns on, moving on the new way.
                let toward = caster.map(|at| heading(at[0] - self.at[0], at[1] - self.at[2]));
                if toward.is_some_and(|way| (way - CLOSE..=way + CLOSE).contains(&self.heading)) {
                    self.phase = Phase::Back;
                } else {
                    self.heading = (self.heading + TURN) % 360;
                }
            }
            Phase::Back => {
                self.alpha = (self.alpha - FADE).max(0.0);
                self.at[1] -= self.climb;
            }
        }
        let (sin, cos) = f64::from(self.heading).to_radians().sin_cos();
        self.at[0] += SPEED * cos as f32;
        self.at[2] += SPEED * sin as f32;
        if self.phase == Phase::Out && SPEED * self.ticks as f32 > self.reach {
            self.phase = Phase::Turn;
        }
        self.spin += SPIN;
    }

    /// The copies' alphas (0 to 255): the shield's less 150, then 25 less each.
    fn copy_alphas(&self) -> [f32; 3] {
        let first = (self.alpha - COPY_BELOW[0]).max(0.0);
        let second = (first - COPY_BELOW[1]).max(0.0);
        [first, second, (second - COPY_BELOW[1]).max(0.0)]
    }
}

/// The shields in flight.
#[derive(Default)]
pub struct Boomerangs {
    shields: Vec<Shield>,
}

impl Boomerangs {
    /// A shield thrown by `unit` (None = the player) standing at `from` at the target at `to` from `now`; `dim` on
    /// ecl_fild01.
    pub fn throw(&mut self, unit: Option<u32>, from: Vector, to: Vector, now: Instant, dim: bool) {
        self.shields.push(Shield::new(unit, from, to, now, dim));
    }

    /// Tomahawk's axe, thrown the same way.
    pub fn throw_axe(&mut self, unit: Option<u32>, from: Vector, to: Vector, now: Instant, dim: bool) {
        self.shields.push(Shield { axe: true, ..Shield::new(unit, from, to, now, dim) });
    }

    /// Runs the shields up to `now`, a tick at a time, turning back to where `position_of` has their casters.
    // ponytail: the caster's place is taken as it is now, for every tick run in a frame.
    pub fn advance(&mut self, now: Instant, position_of: impl Fn(Option<u32>) -> Option<Vector>) {
        for shield in &mut self.shields {
            let caster = position_of(shield.unit).map(|at| [at[0], at[2]]);
            while shield.ticks < passes(shield.start, now).min(LIFE) {
                shield.step(caster);
            }
        }
        self.shields.retain(|shield| passes(shield.start, now) <= LIFE);
    }

    /// The squares to draw: the shield, then its copies, white and faint as their alphas say.
    pub fn quads<'a>(&self, texture_of: impl Fn(&str) -> Option<&'a crate::render::Texture>) -> Vec<EffectQuad<'a>> {
        let mut quads = Vec::new();
        for shield in self.shields.iter().filter(|shield| shield.ticks > 0) {
            let Some(texture) = texture_of(if shield.axe { AXE } else { SHIELD }) else { continue };
            let square = |at: Vector, spin: f32, alpha: f32| -> Option<EffectQuad<'a>> {
                let corners = [225.0_f32, 315.0, 45.0, 135.0].map(|angle| {
                    let (sin, cos) = (angle + spin).to_radians().sin_cos();
                    [RADIUS * PIXELS * cos, RADIUS * PIXELS * sin]
                });
                (alpha > 0.0).then_some((texture, at, corners, [1.0, 1.0, 1.0, alpha / 255.0], false, false))
            };
            quads.extend(square(shield.at, shield.spin, shield.alpha));
            for (copy, alpha) in shield.copies.iter().zip(shield.copy_alphas()) {
                quads.extend(copy.and_then(|copy| square(copy.at, copy.spin, alpha)));
            }
        }
        quads
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A shield from the origin at a target 50 along x, the caster staying there: its place after each of `ticks`.
    fn fly(ticks: u32, dim: bool) -> Shield {
        let mut shield = Shield::new(None, [0.0; 3], [50.0, 0.0, 0.0], Instant::now(), dim);
        for _ in 0..ticks {
            shield.step(Some([0.0, 0.0]));
        }
        shield
    }

    #[test]
    fn headings_walk_the_cosine_table() {
        assert_eq!([heading(1.0, 0.0), heading(0.0, 1.0), heading(0.0, -1.0), heading(-1.0, 0.0)], [0, 91, 269, 180]);
        assert_eq!(heading(1.0, 1.0), 46);
    }

    #[test]
    fn flies_out_loops_and_fades_back() {
        // Out n = 1…26 (heading 0, 2 a tick, first drawn 2 out), turning n = 27…71 (5 … 225), then straight on.
        let first = fly(1, false);
        assert_eq!((first.at[0], first.heading, first.phase), (2.0, 0, Phase::Out));
        let last_out = fly(26, false);
        assert_eq!((last_out.at[0], last_out.phase), (52.0, Phase::Turn));
        let turning = fly(27, false);
        assert_eq!((turning.heading, turning.phase), (5, Phase::Turn));
        assert_eq!((fly(71, false).heading, fly(71, false).phase), (225, Phase::Turn));
        // It starts back at 72 with the heading it has; it fades from 73, alpha 243 … 3 at 93, 0 from 94.
        let back = fly(72, false);
        assert_eq!((back.heading, back.phase, back.alpha), (225, Phase::Back, 255.0));
        assert_eq!((fly(73, false).alpha, fly(93, false).alpha, fly(94, false).alpha), (243.0, 3.0, 0.0));
        // Passing the caster's point about (1.6, 5.9) off, flying on.
        let gone = fly(94, false);
        assert!((gone.at[0] - 1.6).abs() < 0.3 && (gone.at[2] - 5.9).abs() < 0.3, "{:?}", gone.at);
        // Spinning −15° a tick, 8 above the feet, level while it loops (the target as high as the caster).
        assert_eq!((fly(3, false).spin, fly(3, false).at[1]), (-45.0, 8.0));
    }

    #[test]
    fn copies_lag_and_fade_with_the_shield() {
        // Made on every third tick: copy 1 shows at n = 3, 2 at 6, 3 at 9, with 105, 80 and 55 of a shield at 255.
        assert_eq!(fly(2, false).copies.iter().flatten().count(), 0);
        assert_eq!(fly(3, false).copies.iter().flatten().count(), 1);
        assert_eq!(fly(6, false).copies.iter().flatten().count(), 2);
        let nine = fly(9, false);
        assert_eq!(nine.copies.iter().flatten().count(), 3);
        assert_eq!(nine.copy_alphas(), [105.0, 80.0, 55.0]);
        // A copy takes the shield's place and spin before that tick's move: copy 1 of tick 3 is where tick 2 left it.
        let copy = nine.copies[0].unwrap();
        assert_eq!((copy.at, copy.spin), (fly(8, false).at, fly(8, false).spin));
        // They go once the shield is below 150 (flying back), and there are none on ecl_fild01, where alpha caps at 100.
        assert_eq!(fly(80, false).copy_alphas(), [9.0, 0.0, 0.0]);
        assert_eq!(fly(82, false).copy_alphas(), [0.0; 3]);
        assert_eq!(fly(9, true).copy_alphas(), [0.0; 3]);
        // The fade back there: 100, 88 … 4, then 0.
        assert_eq!([fly(72, true).alpha, fly(73, true).alpha, fly(80, true).alpha, fly(81, true).alpha], [100.0, 88.0, 4.0, 0.0]);
    }

    #[test]
    fn a_target_straight_along_z_flies_off_at_91() {
        let shield = Shield::new(None, [0.0; 3], [0.0, 0.0, 40.0], Instant::now(), false);
        assert_eq!(shield.heading, 91);
    }

    #[test]
    fn ends_with_the_effect() {
        let start = Instant::now();
        let mut boomerangs = Boomerangs::default();
        boomerangs.throw(None, [0.0; 3], [50.0, 0.0, 0.0], start, false);
        boomerangs.advance(start + super::super::TICK * 498, |_| Some([0.0; 3]));
        assert_eq!(boomerangs.shields.len(), 1);
        boomerangs.advance(start + super::super::TICK * 499, |_| Some([0.0; 3]));
        assert!(boomerangs.shields.is_empty());
    }

    #[test]
    fn tomahawk_throws_the_axe_the_way_the_shield_flies() {
        let start = Instant::now();
        let mut boomerangs = Boomerangs::default();
        boomerangs.throw(None, [0.0; 3], [50.0, 0.0, 0.0], start, false);
        boomerangs.throw_axe(None, [0.0; 3], [50.0, 0.0, 0.0], start, false);
        boomerangs.advance(start + crate::prims::TICK * 30, |_| Some([0.0; 3]));
        // The axe is the shield with another picture: the same place, spin and alpha.
        assert!(!boomerangs.shields[0].axe && boomerangs.shields[1].axe);
        assert_eq!(boomerangs.shields[0].at, boomerangs.shields[1].at);
        assert_eq!((boomerangs.shields[0].spin, boomerangs.shields[0].alpha), (boomerangs.shields[1].spin, boomerangs.shields[1].alpha));
        assert_eq!(AXE, "data\\texture\\effect\\토마.bmp");
    }
}
