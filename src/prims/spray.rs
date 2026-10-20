//! Cart Termination's 48 pink dots (518) and Shield Chain's five shields flying across the target (520), a 16 ms tick at a time.
//!
//! A shape's own age n counts from 1 at its first tick; t is the ticks since the effect started.

use std::time::Instant;

use super::ground::Potion;
use super::passes;
use crate::world::Vector;

pub const DOT: &str = "data\\texture\\effect\\whitelight.tga";
pub const SHIELD: &str = "data\\texture\\effect\\shield_boomerang.bmp";
/// Cart Termination: 4 prims of 4 elements of 3 dots, all made at tick 30.
const DOTS: usize = 48;
const DOT_TICK: u32 = 30;
const DOT_LAST: u32 = 63;
/// Dot colours by their number (#FFC8C8, #FFA0A0, #FF7878).
const DOT_TINTS: [[f32; 3]; 3] = [[1.0, 200.0 / 255.0, 200.0 / 255.0], [1.0, 160.0 / 255.0, 160.0 / 255.0], [1.0, 120.0 / 255.0, 120.0 / 255.0]];
/// Shield Chain: five shields made on ticks 0, 3 … 12, each 8 up, visible for 38 ticks.
const SHIELDS: u32 = 5;
const SHIELD_LAST: u32 = 38;
const SHIELD_HALF: f32 = 5.0;

struct Dot {
    heading: f32,
    pitch: f32,
    size: f32,
    colour: usize,
}

struct Termination {
    unit: Option<u32>,
    start: Instant,
    dots: Vec<Dot>,
}

struct Shield {
    /// The tick it was made on, where (8 up over the target's feet) and its heading.
    made: u32,
    from: Vector,
    heading: f32,
}

struct Chain {
    unit: Option<u32>,
    start: Instant,
    made: u32,
    last: Option<Vector>,
    shields: Vec<Shield>,
}

/// A dot to draw: where, its half-side in world units, its alpha (0 to 255) and colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DotDraw {
    pub at: Vector,
    pub half: f32,
    pub alpha: f32,
    pub tint: [f32; 3],
}

#[derive(Default)]
pub struct Spray {
    terminations: Vec<Termination>,
    chains: Vec<Chain>,
}

/// How far a dot is from the feet at its age n: 1 a tick, then from n = 26 the speed drops by a tenth after each move.
pub fn dot_distance(n: u32) -> f32 {
    let (mut speed, mut distance) = (1.0_f32, 0.0_f32);
    for age in 1..=n {
        distance += speed;
        if age > 25 {
            speed *= 0.9;
        }
    }
    distance
}

/// A dot's alpha at n: 230 to 25, then 6 less a tick.
pub fn dot_alpha(n: u32) -> f32 {
    if n <= 25 { 230.0 } else { (230.0 - 6.0 * (n - 25) as f32).max(0.0) }
}

/// A shield's alpha at its age m: 25 at first, 45 a tick up to 250 at 5, held to 14, then 10 less a tick.
fn shield_alpha(m: u32) -> f32 {
    match m {
        0 => 25.0,
        1..=5 => 25.0 + 45.0 * m as f32,
        6..=14 => 250.0,
        _ => (250.0 - 10.0 * (m - 14) as f32).max(0.0),
    }
}

/// A shield and its three ghosts after n ticks as (age of the place, alpha) of the shield and each ghost: every third tick, before
/// the move, the ghosts shift down one and the first takes the shield's place with its alpha less 150, each of the others the one
/// before's less 25. On a dim map every alpha above 100 is cut to 100.
/// A shield's (age, alpha) and its three ghosts'.
type AgeAlpha = (u32, f32);

pub fn shield_state(n: u32, dim: bool) -> (AgeAlpha, [Option<AgeAlpha>; 3]) {
    let cut = |alpha: f32| if dim { alpha.min(100.0) } else { alpha };
    let mut ghosts: [Option<(u32, f32)>; 3] = [None; 3];
    for m in 1..=n {
        if m % 3 == 0 {
            let before = cut(shield_alpha(m - 1));
            let faded = |ghost: Option<(u32, f32)>| ghost.map(|(age, alpha)| (age, (alpha - 25.0).max(0.0)));
            ghosts = [Some((m - 1, (before - 150.0).max(0.0))), faded(ghosts[0]), faded(ghosts[1])];
            // (The second and third keep the places of those they follow; their alphas fall by 25 from the ones they follow.)
        }
    }
    ((n, cut(shield_alpha(n))), ghosts)
}

impl Spray {
    /// Cart Termination on `unit`: 48 dots made at its tick 30, their choices from `rand(n)` (0 to n − 1) in the order the game
    /// draws them (per element its size, a heading and pitch for each dot, then the colour).
    pub fn termination(&mut self, unit: Option<u32>, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        let mut dots = Vec::with_capacity(DOTS);
        for _ in 0..16 {
            let size = rand(6) as f32 * 0.1 + 2.5;
            let angles = [(); 3].map(|()| (rand(360) as f32, rand(180) as f32));
            let colour = rand(3) as usize;
            dots.extend(angles.map(|(heading, pitch)| Dot { heading, pitch, size, colour }));
        }
        self.terminations.push(Termination { unit, start: now, dots });
    }

    /// Shield Chain on `unit`: the shields are made on ticks 0, 3 … 12 as `advance` runs.
    pub fn chain(&mut self, unit: Option<u32>, now: Instant) {
        self.chains.push(Chain { unit, start: now, made: 0, last: None, shields: Vec::new() });
    }

    /// Makes the chains' shields as their ticks come, over the target's feet then (`position_of`), a heading from `rand(n)`.
    pub fn advance(&mut self, now: Instant, position_of: impl Fn(Option<u32>) -> Option<Vector>, mut rand: impl FnMut(u32) -> u32) {
        for chain in &mut self.chains {
            chain.last = position_of(chain.unit).or(chain.last);
            while chain.made < SHIELDS && passes(chain.start, now) > 3 * chain.made {
                if let Some(feet) = chain.last {
                    chain.shields.push(Shield { made: 3 * chain.made, from: [feet[0], feet[1] + 8.0, feet[2]], heading: rand(360) as f32 });
                }
                chain.made += 1;
            }
        }
        self.terminations.retain(|termination| passes(termination.start, now) <= DOT_TICK + DOT_LAST + 1);
        self.chains.retain(|chain| passes(chain.start, now) <= 3 * (SHIELDS - 1) + SHIELD_LAST + 1);
    }

    /// The dots showing at `now`, over their target's feet (`position_of`); one is skipped while its middle is not higher than half
    /// its size above the feet.
    pub fn dots(&self, now: Instant, position_of: impl Fn(Option<u32>) -> Option<Vector>) -> Vec<DotDraw> {
        let mut dots = Vec::new();
        for termination in &self.terminations {
            let (Some(feet), Some(t)) = (position_of(termination.unit), passes(termination.start, now).checked_sub(1)) else { continue };
            let Some(n) = t.checked_sub(DOT_TICK - 1).filter(|&n| (1..=DOT_LAST).contains(&n)) else { continue };
            let (distance, alpha) = (dot_distance(n), dot_alpha(n));
            for dot in &termination.dots {
                let (pitch_sin, pitch_cos) = dot.pitch.to_radians().sin_cos();
                let (sin, cos) = dot.heading.to_radians().sin_cos();
                let up = distance * pitch_sin;
                if up <= 0.5 * dot.size || alpha <= 0.0 {
                    continue;
                }
                let at = [feet[0] + distance * pitch_cos * sin, feet[1] + up, feet[2] + distance * pitch_cos * cos];
                dots.push(DotDraw { at, half: std::f32::consts::FRAC_1_SQRT_2 * dot.size, alpha, tint: DOT_TINTS[dot.colour] });
            }
        }
        dots
    }

    /// The shields and their ghosts showing at `now`: the shield first, then its ghosts 1, 2 and 3.
    pub fn shields(&self, now: Instant, dim: bool) -> Vec<Potion> {
        let mut shields = Vec::new();
        for chain in &self.chains {
            let Some(t) = passes(chain.start, now).checked_sub(1) else { continue };
            for shield in &chain.shields {
                let Some(n) = (t + 1).checked_sub(shield.made).filter(|&n| (1..=SHIELD_LAST).contains(&n)) else { continue };
                let (dx, dz) = shield.heading.to_radians().sin_cos();
                // Its place after m ticks: back along the heading, 37.5 − 2.5 m from the target's point.
                let place = |m: u32| {
                    let back = 37.5 - 2.5 * m as f32;
                    [shield.from[0] - back * dz, shield.from[1], shield.from[2] - back * dx]
                };
                let spin = |m: u32| (-15.0 * m as f32).rem_euclid(360.0);
                let ((age, alpha), ghosts) = shield_state(n, dim);
                shields.push(Potion { icon: SHIELD, at: place(age), spin: spin(age), alpha });
                shields.extend(ghosts.iter().flatten().filter(|(_, alpha)| *alpha > 0.0).map(|&(age, alpha)| Potion {
                    icon: SHIELD,
                    at: place(age),
                    spin: spin(age),
                    alpha,
                }));
            }
        }
        shields
    }

    /// The half-diagonal of a shield in world units.
    pub fn shield_half() -> f32 {
        SHIELD_HALF
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
    fn cart_termination_sprays_48_dots_up_and_out() {
        let start = Instant::now();
        let mut spray = Spray::default();
        // Draws of 0, except 90 on every third from the second: size 2.5, colour 2 (#FF7878).
        let mut draws = (0..).map(|n: u32| if n % 3 == 1 { 90 } else { 0 });
        spray.termination(Some(1), start, |bound| draws.next().unwrap().min(bound - 1));
        assert_eq!(spray.terminations[0].dots.len(), 48);
        // Distance at n = 26, 27, 30, 40, 63: 26, 26.9, 29.1, 32.9, 34.8; alpha at n = 25, 26, 40, 63, 64: 230, 224, 140, 2, 0.
        assert_eq!([1, 26, 27, 30, 40, 63].map(|n| (dot_distance(n) * 10.0).round() / 10.0), [1.0, 26.0, 26.9, 29.1, 32.9, 34.8]);
        assert_eq!([25, 26, 40, 63, 64].map(dot_alpha), [230.0, 224.0, 140.0, 2.0, 0.0]);
        let feet = [10.0, 1.0, 10.0];
        let dots = |tick: u32| spray.dots(start + TICK * tick, |_| Some(feet));
        // Nothing before tick 30; drawn on ticks 30…92 (a dot skipped while it is no higher than half its size: n = 1 is 1.0 < 1.25).
        assert!(dots(29).is_empty());
        assert!(dots(30).is_empty(), "straight up 1 unit is under 0.5 · 2.5");
        let second = dots(31);
        assert!(!second.is_empty());
        assert!(near(second[0].half, std::f32::consts::FRAC_1_SQRT_2 * 2.5) && second[0].tint == DOT_TINTS[2]);
        assert!(near(second[0].at[1], 1.0 + 2.0));
        assert!(!dots(92).is_empty() && dots(93).is_empty());
        // A dot with pitch 0 never shows (it has no height).
        let mut flat = Spray::default();
        flat.termination(None, start, |_| 0);
        assert!((30..93).all(|tick| flat.dots(start + TICK * tick, |_| Some(feet)).is_empty()));
        // It rides along with the target.
        assert!(spray.dots(start + TICK * 40, |_| None).is_empty());
        let moved = spray.dots(start + TICK * 40, |_| Some([50.0, 1.0, 50.0]));
        assert!(moved.iter().all(|dot| dot.at[0] > 40.0));
    }

    #[test]
    fn shield_chain_sends_five_shields_through_the_target_with_ghosts() {
        let start = Instant::now();
        let mut spray = Spray::default();
        spray.chain(Some(1), start);
        let feet = [10.0, 1.0, 10.0];
        for tick in 0..=13 {
            spray.advance(start + TICK * tick, |_| Some(feet), |_| 0);
        }
        // Five shields made on ticks 0, 3, 6, 9, 12, each 8 up.
        assert_eq!(spray.chains[0].shields.iter().map(|shield| shield.made).collect::<Vec<_>>(), [0, 3, 6, 9, 12]);
        assert!(spray.chains[0].shields.iter().all(|shield| shield.from == [10.0, 9.0, 10.0]));
        // Alpha at n = 1, 5, 14, 15, 27, 38, 39: 70, 250, 250, 240, 120, 10, 0.
        assert_eq!([1, 5, 14, 15, 27, 38, 39].map(|n| shield_state(n, false).0.1), [70.0, 250.0, 250.0, 240.0, 120.0, 10.0, 0.0]);
        // The ghosts (1 / 2 / 3) at n = 5, 6, 9, 12, 18, 21, 24, 27.
        let ghosts = |n: u32| shield_state(n, false).1.map(|ghost| ghost.map_or(0.0, |(_, alpha)| alpha));
        assert_eq!(ghosts(5), [0.0, 0.0, 0.0]);
        assert_eq!(ghosts(6), [100.0, 0.0, 0.0]);
        assert_eq!(ghosts(9), [100.0, 75.0, 0.0]);
        assert_eq!(ghosts(12), [100.0, 75.0, 50.0]);
        assert_eq!(ghosts(18), [70.0, 75.0, 50.0]);
        assert_eq!(ghosts(21), [40.0, 45.0, 50.0]);
        assert_eq!(ghosts(24), [10.0, 15.0, 20.0]);
        assert_eq!(ghosts(27), [0.0, 0.0, 0.0]);
        // On a dim map the shield is 70 and 100 to n = 14 and no ghost shows.
        assert_eq!((shield_state(1, true).0.1, shield_state(14, true).0.1), (70.0, 100.0));
        assert!((1..=38).all(|n| shield_state(n, true).1.iter().all(|ghost| ghost.is_none_or(|(_, alpha)| alpha == 0.0))));
        // With heading 0 the near-side distance is 35 at n = 1, 0 at n = 15 and −2.5 at n = 16.
        let at = |tick: u32| spray.shields(start + TICK * tick, false);
        let first = at(0);
        assert_eq!(first.len(), 1);
        assert!(near(first[0].at[0], 10.0 - 35.0) && near(first[0].at[1], 9.0) && first[0].icon == SHIELD && first[0].alpha == 70.0);
        assert!(near(at(14)[0].at[0], 10.0) && near(at(15)[0].at[0], 10.0 + 2.5));
        // Visible for n = 1…38 (ticks 0…37 for the first), gone at 38; the spin goes down 15° a tick.
        assert!(!at(37).is_empty() && at(60).is_empty());
        assert!(near(at(1)[0].spin - at(0)[0].spin, -15.0));
        spray.advance(start + TICK * 100, |_| Some(feet), |_| 0);
        assert!(spray.chains.is_empty());
    }
}
