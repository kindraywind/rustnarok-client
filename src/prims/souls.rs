//! Harmonize's and Arrullo's drifting soul dots (850, 932): two bands of three soft dots that rise, sink and drift round a unit's
//! head, a 16 ms tick at a time.
//!
//! A band's own age n counts from 1 on the first tick after the effect starts.

use std::time::Instant;

use super::passes;
use super::spray::DotDraw;
use crate::world::Vector;

pub const DOT: &str = super::spray::DOT;
/// The effect lasts 100 ticks; a band's dots are gone from n = 90.
const LIFE: u32 = 100;
/// How far a dot drifts a tick.
const SPEED: f32 = 0.07;

/// Harmonize's three dots (#2828FF, #0A0AFF, #4646FF) and Arrullo's three pinks, one for all three dots of a band.
const BLUES: [[f32; 3]; 3] = [rgb(0x28, 0x28, 0xFF), rgb(0x0A, 0x0A, 0xFF), rgb(0x46, 0x46, 0xFF)];
const PINKS: [[f32; 3]; 3] = [rgb(0xFF, 0xC8, 0xC8), rgb(0xFF, 0xA0, 0xA0), rgb(0xFF, 0x78, 0x78)];

const fn rgb(red: u8, green: u8, blue: u8) -> [f32; 3] {
    [red as f32 / 255.0, green as f32 / 255.0, blue as f32 / 255.0]
}

/// One band: its dots' size, the six angles in use and their turning points (whole degrees), age and alpha, and the three dots'
/// offsets from the unit's feet.
struct Band {
    size: f32,
    angle: [i32; 6],
    turn: [i32; 6],
    n: u32,
    alpha: f32,
    /// Which pink (0 to 2) a band of 932 has, none for 850.
    pink: Option<usize>,
    dots: [Vector; 3],
}

struct Lights {
    unit: Option<u32>,
    start: Instant,
    done: u32,
    bands: [Band; 2],
}

#[derive(Default)]
pub struct Souls {
    lights: Vec<Lights>,
}

/// A band's alpha at its age n: 20 a tick to 180 at n = 9, held to 30, then 3 less a tick.
#[cfg(test)]
pub fn soul_alpha(n: u32) -> f32 {
    let mut alpha: f32 = 0.0;
    for each in 1..=n {
        if each < 10 {
            alpha += 20.0;
        } else if each > 30 {
            alpha = (alpha - 3.0).max(0.0);
        }
    }
    alpha
}

fn sin(degrees: i32) -> f32 {
    (degrees as f32).to_radians().sin()
}

impl Band {
    /// A band starting at the offset `at`: its size, then the twelve angles, then (for Arrullo) its pink from `rand(n)`.
    fn new(arrullo: bool, at: Vector, rand: &mut impl FnMut(u32) -> u32) -> Self {
        let size = 1.0 + 0.1 * rand(11) as f32;
        let mut angle = [0; 6];
        let mut turn = [0; 6];
        for pair in 0..6 {
            angle[pair] = rand(360) as i32;
            turn[pair] = rand(360) as i32;
        }
        let pink = arrullo.then(|| rand(3) as usize);
        Self { size, angle, turn, n: 0, alpha: 0.0, pink, dots: [at; 3] }
    }

    /// One tick: the angles wander, the alpha follows its curve and the dots drift.
    fn step(&mut self, rand: &mut impl FnMut(u32) -> u32) {
        self.n += 1;
        for pair in 0..6 {
            let (angle, turn) = (&mut self.angle[pair], &mut self.turn[pair]);
            if *turn > *angle {
                *angle = (*angle + 3).min(359);
                if *angle > *turn {
                    *turn = if *angle == 0 { 0 } else { rand(*angle as u32) as i32 };
                }
            } else {
                *angle = (*angle - 2).max(0);
                if *angle <= *turn {
                    *turn = if *angle == 0 { 359 } else { 359 - rand(*angle as u32) as i32 };
                }
            }
        }
        if self.n < 10 {
            self.alpha += 20.0;
        } else if self.n > 30 {
            self.alpha = (self.alpha - 3.0).max(0.0);
        }
        let [g1, g2, g3, g4, g5, g6] = self.angle.map(sin);
        let [rising, sinking, level] = &mut self.dots;
        if rising[1] > 0.0 {
            rising[0] += SPEED * g1;
            rising[2] += SPEED * g2;
        }
        rising[1] += SPEED;
        if sinking[1] > 0.0 {
            sinking[0] -= SPEED * g3;
            sinking[2] -= SPEED * g4;
        }
        sinking[1] -= SPEED;
        if level[1] > 0.0 {
            level[0] += 1.2 * SPEED * g5;
            level[2] -= 1.2 * SPEED * g6;
        }
    }
}

impl Souls {
    /// Harmonize's lights (850), or Arrullo's pink ones (`arrullo`, 932), on `unit` from the offset `at` its feet: two bands, each one's
    /// choices from `rand(n)`.
    pub fn launch(&mut self, arrullo: bool, unit: Option<u32>, at: Vector, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        let bands = [(); 2].map(|()| Band::new(arrullo, at, &mut rand));
        self.lights.push(Lights { unit, start: now, done: 0, bands });
    }

    pub fn forget(&mut self, unit: u32) {
        self.lights.retain(|lights| lights.unit != Some(unit));
    }

    /// Runs the bands up to `now`, a tick at a time, and drops what ended.
    pub fn advance(&mut self, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        for lights in &mut self.lights {
            while lights.done < passes(lights.start, now).saturating_sub(1).min(LIFE) {
                lights.done += 1;
                for band in &mut lights.bands {
                    band.step(&mut rand);
                }
            }
        }
        self.lights.retain(|lights| passes(lights.start, now) <= LIFE);
    }

    /// The dots at `now` over their units' feet (`position_of`): soft squares facing the camera, added on. One is skipped while its
    /// middle is not higher than half its size above the feet.
    pub fn dots(&self, position_of: impl Fn(Option<u32>) -> Option<Vector>) -> Vec<DotDraw> {
        let mut dots = Vec::new();
        for lights in &self.lights {
            let Some(feet) = position_of(lights.unit) else { continue };
            for band in lights.bands.iter().filter(|band| band.alpha > 0.0) {
                for (index, offset) in band.dots.iter().enumerate() {
                    if offset[1] <= 0.5 * band.size {
                        continue;
                    }
                    let tint = match band.pink {
                        Some(pink) => PINKS[pink],
                        None => BLUES[index],
                    };
                    let at = [feet[0] + offset[0], feet[1] + offset[1], feet[2] + offset[2]];
                    dots.push(DotDraw { at, half: std::f32::consts::FRAC_1_SQRT_2 * band.size, alpha: band.alpha, tint });
                }
            }
        }
        dots
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prims::TICK;

    fn near(got: f32, want: f32) -> bool {
        (got - want).abs() < 1e-3
    }

    #[test]
    fn each_launch_makes_two_bands_of_three_dots() {
        let start = Instant::now();
        let at = [3.0, 25.0, 4.0];
        let mut souls = Souls::default();
        let mut draws = Vec::new();
        souls.launch(false, Some(1), at, start, |n| {
            draws.push(n);
            0
        });
        // 850: R(11) then twelve R(360) a band, 13 each; 932 adds R(3), 14 each.
        assert_eq!(draws.len(), 26);
        assert_eq!(draws[..3], [11, 360, 360]);
        let mut pink = Vec::new();
        souls.launch(true, Some(1), at, start, |n| {
            pink.push(n);
            0
        });
        assert_eq!(pink.len(), 28);
        assert_eq!((pink[13], pink[27]), (3, 3));
        // Every draw 0: size 1.0, all angles 0, all three dots at the launch offset; 932's pink is selector 1.
        let band = &souls.lights[0].bands[0];
        assert!(band.size == 1.0 && band.angle == [0; 6] && band.dots == [at; 3] && band.pink.is_none());
        assert_eq!(souls.lights[1].bands[1].pink, Some(0));
        // The pink is 1, 2 or 3 for draws 0, 1, 2; the size 1.0 to 2.0.
        let mut high = Souls::default();
        high.launch(true, None, at, start, |n| n - 1);
        assert_eq!(high.lights[0].bands[0].pink, Some(2));
        assert!(near(high.lights[0].bands[0].size, 2.0));
    }

    #[test]
    fn the_alpha_rises_holds_and_fades() {
        assert_eq!([1, 9, 10, 30, 31, 90, 91].map(soul_alpha), [20.0, 180.0, 180.0, 180.0, 177.0, 0.0, 0.0]);
        let start = Instant::now();
        let mut souls = Souls::default();
        souls.launch(false, Some(1), [0.0, 25.0, 0.0], start, |_| 0);
        for tick in 0..=100 {
            souls.advance(start + TICK * tick, |_| 0);
            if let Some(lights) = souls.lights.first() {
                assert_eq!(lights.bands[0].alpha, soul_alpha(tick), "tick {tick}");
            }
        }
    }

    #[test]
    fn the_angles_wander_between_random_turning_points() {
        // One tick for the first pair at angle g with turning point t; the other pairs sit at 0 and draw nothing. Every draw is 7.
        let one = |g: i32, t: i32| {
            let mut band = Band::new(false, [0.0, 25.0, 0.0], &mut |_| 0);
            (band.angle[0], band.turn[0]) = (g, t);
            let mut draws = Vec::new();
            band.step(&mut |n| {
                draws.push(n);
                7
            });
            ((band.angle[0], band.turn[0]), draws)
        };
        // At 0 with the point 0: down to 0, the point 359 with no draw.
        assert_eq!(one(0, 0), ((0, 359), vec![]));
        // The point above: up 3, no draw until crossed; crossed, the point is redrawn below the angle.
        assert_eq!(one(10, 50), ((13, 50), vec![]));
        assert_eq!(one(48, 50), ((51, 7), vec![51]));
        // Stops at 359 and does not redraw there, then comes back down and redraws.
        assert_eq!(one(358, 359), ((359, 359), vec![]));
        assert_eq!(one(359, 359), ((357, 359 - 7), vec![357]));
        // The point at or below: down 2, no draw until it reaches the point; then 359 − R(g), or 359 at 0 with no draw.
        assert_eq!(one(50, 10), ((48, 10), vec![]));
        assert_eq!(one(11, 10), ((9, 352), vec![9]));
        assert_eq!(one(1, 0), ((0, 359), vec![]));
    }

    #[test]
    fn the_dots_rise_sink_and_drift() {
        let mut band = Band::new(false, [1.0, 25.0, 2.0], &mut |_| 0);
        // Angles 0: one tick keeps x and z; the first dot rises 0.07, the second sinks 0.07, the third stays level.
        band.step(&mut |_| 0);
        assert!(near(band.dots[0][1], 25.07) && near(band.dots[1][1], 24.93) && near(band.dots[2][1], 25.0));
        assert!(band.dots.iter().all(|dot| dot[0] == 1.0 && dot[2] == 2.0));
        // 90 ticks of the rise: 6.3.
        for _ in 0..89 {
            band.step(&mut |_| 0);
        }
        assert!(near(band.dots[0][1], 25.0 + 6.3) && near(band.dots[1][1], 25.0 - 6.3));
        // With g1 = 90 the first dot moves 0.07 in x; with g5 = 90 the third 0.084.
        let mut drift = Band::new(false, [0.0, 25.0, 0.0], &mut |_| 0);
        drift.angle = [90, 0, 0, 0, 90, 0];
        drift.turn = [359, 0, 0, 0, 359, 0];
        drift.step(&mut |_| 0);
        // The angle moved up 3° first: the sines of 93°.
        let wobble = (93.0_f32).to_radians().sin();
        assert!(near(drift.dots[0][0], 0.07 * wobble) && near(drift.dots[2][0], 0.084 * wobble) && near(drift.dots[1][0], 0.0));
    }

    #[test]
    fn the_dots_follow_the_unit_in_their_colours_and_end_with_the_effect() {
        let start = Instant::now();
        let mut souls = Souls::default();
        souls.launch(false, Some(1), [0.0, 25.0, 0.0], start, |_| 0);
        souls.launch(true, Some(2), [0.0, 25.0, 0.0], start, |n| if n == 3 { 2 } else { 0 });
        souls.advance(start + TICK * 2, |_| 0);
        let feet = |unit: Option<u32>| Some(if unit == Some(1) { [10.0, 1.0, 10.0] } else { [50.0, 1.0, 50.0] });
        let dots = souls.dots(feet);
        // Two bands of three dots for each unit, the unit's feet plus the offset, added on.
        assert_eq!(dots.len(), 12);
        assert!(
            dots[..6].iter().all(|dot| (dot.at[0] - 10.0).abs() < 0.1 && (dot.at[1] - 26.0).abs() < 1.0 && near(dot.half, 0.70710677) && dot.alpha == 40.0)
        );
        assert_eq!([dots[0].tint, dots[1].tint, dots[2].tint], BLUES);
        assert!(dots[6..].iter().all(|dot| dot.tint == PINKS[2] && (dot.at[0] - 50.0).abs() < 0.1));
        // Following a moved unit; skipped with no unit; skipped while not higher than half its size.
        assert!(souls.dots(|_| Some([100.0, 1.0, 100.0])).iter().all(|dot| dot.at[0] > 99.0));
        assert!(souls.dots(|_| None).is_empty());
        let mut low = Souls::default();
        low.launch(false, Some(1), [0.0, 0.55, 0.0], start, |_| 0);
        low.advance(start + TICK * 2, |_| 0);
        assert_eq!(low.dots(feet).len(), 4, "the sinking dot of each band is under 0.5");
        // Drawn to tick 99, gone with the effect at tick 100.
        souls.advance(start + TICK * 99, |_| 0);
        assert!(souls.dots(feet).is_empty() && !souls.lights.is_empty());
        souls.advance(start + TICK * 100, |_| 0);
        assert!(souls.lights.is_empty());
    }
}
