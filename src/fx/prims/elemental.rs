//! The earth elemental's Stone Rain (944), a 16 ms tick at a time: ten prims of four rocks falling on and round the target, each
//! with a red glow, fading out where they land.
//!
//! e is the ticks since the effect started; a band's own age n counts from 1 on the first tick after its prim is made.

use std::time::Instant;

use super::passes;
use crate::render::Texture;
use crate::world::{EffectQuad, Vector};

pub const STONE: &str = "data\\texture\\effect\\stone_part.bmp";
const PIXELS_PER_UNIT: f32 = 7.0;
/// A prim every 10 ticks from e = 0 to 90, four bands each; the effect ends at e = 100 and cuts them.
const EVERY: u32 = 10;
const PRIMS: u32 = 10;
const BANDS: usize = 4;
const LENGTH: u32 = 100;
/// A stone's colour by its pick (#FFFFFF, #FFA0A0, #FF5A5A), and its glow's (#FF0101), 1.1 times as big.
const TINTS: [[f32; 3]; 3] = [[1.0, 1.0, 1.0], [1.0, 160.0 / 255.0, 160.0 / 255.0], [1.0, 90.0 / 255.0, 90.0 / 255.0]];
const GLOW: [f32; 3] = [1.0, 1.0 / 255.0, 1.0 / 255.0];
const GLOW_SIZE: f32 = 1.1;

/// A square to draw: its place, its corners in screen pixels, its colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StoneDraw {
    pub at: Vector,
    pub corners: [[f32; 2]; 4],
    pub tint: [f32; 4],
}

/// One falling stone: how fast it falls, its half-diagonal, where round the prim's place, how high, its colour, age and alpha.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Band {
    speed: f32,
    size: f32,
    heading: f32,
    radius: f32,
    height: f32,
    pick: usize,
    age: u32,
    alpha: i32,
}

impl Band {
    /// A band: fall speed, half-diagonal, heading, radius, start height and colour from `rand(n)` in the game's order.
    fn new(rand: &mut impl FnMut(u32) -> u32) -> Self {
        let speed = 2.5 + 0.01 * rand(51) as f32;
        let size = 7.0 + 0.1 * rand(31) as f32;
        let heading = rand(360) as f32;
        let radius = rand(33) as f32;
        let height = 80.0 - rand(21) as f32;
        let pick = rand(3) as usize;
        Self { speed, size, heading, radius, height, pick, age: 0, alpha: 0 }
    }

    /// One tick: it fades in for 9 ticks, falls until it is on the ground, then fades 15 a tick.
    fn step(&mut self) {
        self.age += 1;
        if self.age < 10 {
            self.alpha = (self.alpha + 25).min(255);
        }
        if self.height <= 0.0 {
            self.alpha -= 15;
        } else {
            self.height -= self.speed;
        }
    }

    fn gone(&self) -> bool {
        self.alpha < 0
    }
}

/// A prim: where the target stood when it was made, its start, the most ticks it runs before the effect ends, the ticks run
/// and its bands.
struct Prim {
    at: Vector,
    start: Instant,
    last: u32,
    done: u32,
    bands: Vec<Band>,
}

struct Rain {
    unit: Option<u32>,
    start: Instant,
    /// Where the target last stood, and how many prims are made.
    feet: Vector,
    made: u32,
}

#[derive(Default)]
pub struct Elemental {
    rains: Vec<Rain>,
    prims: Vec<Prim>,
}

impl Elemental {
    /// Stone Rain on `unit`, now standing at `feet`.
    pub fn stone_rain(&mut self, unit: Option<u32>, feet: Vector, now: Instant) {
        self.rains.push(Rain { unit, start: now, feet, made: 0 });
    }

    /// Makes the prims whose ticks have come (at the target's feet then), runs the bands a tick at a time and drops what the
    /// effect's end cut. The choices of each band come from `rand(n)`.
    pub fn advance(&mut self, now: Instant, position_of: impl Fn(Option<u32>) -> Option<Vector>, mut rand: impl FnMut(u32) -> u32) {
        for rain in &mut self.rains {
            rain.feet = position_of(rain.unit).unwrap_or(rain.feet);
            while rain.made < PRIMS && passes(rain.start, now) > rain.made * EVERY {
                let born = rain.made * EVERY;
                let bands = (0..BANDS).map(|_| Band::new(&mut rand)).collect();
                self.prims.push(Prim { at: rain.feet, start: rain.start + super::TICK * born, last: LENGTH - 1 - born, done: 0, bands });
                rain.made += 1;
            }
        }
        self.prims.retain(|prim| passes(prim.start, now).saturating_sub(1) <= prim.last);
        for prim in &mut self.prims {
            while prim.done < passes(prim.start, now).saturating_sub(1) {
                prim.done += 1;
                for band in prim.bands.iter_mut().filter(|band| !band.gone()) {
                    band.step();
                }
            }
        }
        self.rains.retain(|rain| rain.made < PRIMS);
    }

    /// The stones' squares: per stone two facing the camera and added on, the stone in its colour, then its red glow, drawn over
    /// everything (`rise` screen pixels are taken off again).
    pub fn draws(&self, rise: f32) -> Vec<StoneDraw> {
        let mut draws = Vec::new();
        for prim in &self.prims {
            for band in prim.bands.iter().filter(|band| band.age > 0 && band.alpha > 0) {
                let (sin, cos) = band.heading.to_radians().sin_cos();
                let at = [prim.at[0] + band.radius * cos, prim.at[1] + band.height, prim.at[2] + band.radius * sin];
                let alpha = band.alpha as f32 / 255.0;
                for (scale, [red, green, blue]) in [(1.0, TINTS[band.pick]), (GLOW_SIZE, GLOW)] {
                    let half = band.size * scale * PIXELS_PER_UNIT;
                    // The corners at 315, 225, 135 and 45° (y up) take the texture's (0, 0), (1, 0), (1, 1) and (0, 1).
                    let corners = [315.0_f32, 225.0, 135.0, 45.0].map(|angle| {
                        let (sin, cos) = angle.to_radians().sin_cos();
                        [half * cos, -half * sin + rise]
                    });
                    draws.push(StoneDraw { at, corners, tint: [red, green, blue, alpha] });
                }
            }
        }
        draws
    }

    /// The `draws` as quads with their texture.
    pub fn quads<'a>(&self, texture: Option<&'a Texture>, rise: f32) -> Vec<EffectQuad<'a>> {
        let Some(texture) = texture else { return Vec::new() };
        self.draws(rise).into_iter().map(|draw| (texture, draw.at, draw.corners, draw.tint, true, true)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prims::TICK;

    fn near(got: f32, want: f32) -> bool {
        (got - want).abs() < 1e-4
    }

    /// Stone Rain run to tick `e` with every draw `pick(n)`, the target standing still at the origin.
    fn rain_to(e: u32, pick: impl Fn(u32) -> u32) -> (Elemental, Instant) {
        let start = Instant::now();
        let mut elemental = Elemental::default();
        elemental.stone_rain(Some(1), [0.0; 3], start);
        for tick in 0..=e {
            elemental.advance(start + TICK * tick, |_| Some([0.0; 3]), &pick);
        }
        (elemental, start)
    }

    #[test]
    fn ten_prims_of_four_stones_come_every_ten_ticks() {
        let start = Instant::now();
        let mut elemental = Elemental::default();
        elemental.stone_rain(Some(1), [0.0; 3], start);
        let mut draws = Vec::new();
        let mut made = Vec::new();
        for e in 0..100 {
            elemental.advance(
                start + TICK * e,
                |_| Some([e as f32, 0.0, 0.0]),
                |n| {
                    draws.push(n);
                    0
                },
            );
            made.push(elemental.prims.len());
        }
        // One prim at e = 0, 10 … 90, none else; each of four bands, each six draws in order.
        assert_eq!(draws.len(), 10 * 4 * 6);
        assert_eq!(draws[..6], [51, 31, 360, 33, 21, 3]);
        assert_eq!([0, 9, 10, 90].map(|e| made[e]), [1, 1, 2, 10]);
        assert!(elemental.prims.iter().all(|prim| prim.bands.len() == 4));
        // Each takes the target's feet at its own tick, and stays there.
        assert_eq!(elemental.prims.iter().map(|prim| prim.at[0]).collect::<Vec<_>>(), (0..10).map(|k| (10 * k) as f32).collect::<Vec<_>>());
        // The effect's end at e = 100 cuts them all.
        elemental.advance(start + TICK * 100, |_| None, |_| 0);
        assert!(elemental.prims.is_empty() && elemental.rains.is_empty());
        // The highest draws: 3.00, 10.0, 32, 60 up, colour 2.
        let high = Band::new(&mut |n| n - 1);
        assert!(near(high.speed, 3.0) && near(high.size, 10.0) && high.heading == 359.0 && high.radius == 32.0 && high.height == 60.0 && high.pick == 2);
    }

    #[test]
    fn a_stone_fades_in_falls_and_fades_out() {
        // Every draw 0: 2.5 a tick from 80 up; with the start 60, it lands after 24 ticks.
        let mut band = Band::new(&mut |n| if n == 21 { 20 } else { 0 });
        assert!(near(band.height, 60.0) && near(band.speed, 2.5));
        let mut alphas = Vec::new();
        let mut heights = Vec::new();
        while !band.gone() {
            band.step();
            alphas.push(band.alpha);
            heights.push(band.height);
        }
        // 25 at n = 1, 225 at n = 9 and held; on the ground at n = 24, fading 15 a tick from 25, gone on the 16th (n = 40).
        assert_eq!((alphas[0], alphas[8], alphas[23]), (25, 225, 225));
        assert!(near(heights[22], 2.5) && near(heights[23], 0.0));
        assert_eq!((alphas[24], alphas[38]), (210, 0));
        assert_eq!(alphas.len(), 40);
    }

    #[test]
    fn the_last_prims_vanish_in_the_air() {
        // The prims made at e = 80 and 90 draw 19 and 9 ticks: from the tick after they are made to e = 99.
        let drawn = |born: u32| {
            (0..100)
                .filter(|&e| {
                    let (elemental, _) = rain_to(e, |_| 0);
                    elemental.prims.iter().any(|prim| prim.start.duration_since(elemental.prims[0].start) == TICK * born && !draws_of(prim).is_empty())
                })
                .count()
        };
        fn draws_of(prim: &Prim) -> Vec<Band> {
            prim.bands.iter().copied().filter(|band| band.age > 0 && band.alpha > 0).collect()
        }
        assert_eq!((drawn(80), drawn(90)), (19, 9));
        // And they are still in the air at the end: from 80 up at 2.5 a tick, 9 ticks fall 22.5.
        let (elemental, _) = rain_to(99, |_| 0);
        assert!(elemental.prims.last().unwrap().bands.iter().all(|band| near(band.height, 80.0 - 22.5)));
    }

    #[test]
    fn each_stone_is_two_added_squares_with_a_red_glow() {
        let (elemental, _) = rain_to(1, |n| if n == 3 { 1 } else { 0 });
        let draws = elemental.draws(0.0);
        // Four stones of the first prim: the stone #FFA0A0, then the glow #FF0101 at 1.1 times; the alpha 25.
        assert_eq!(draws.len(), 4 * 2);
        let (stone, glow) = (draws[0], draws[1]);
        assert_eq!(stone.tint, [1.0, 160.0 / 255.0, 160.0 / 255.0, 25.0 / 255.0]);
        assert_eq!(glow.tint, [1.0, 1.0 / 255.0, 1.0 / 255.0, 25.0 / 255.0]);
        let reach = |draw: StoneDraw| f32::hypot(draw.corners[0][0], draw.corners[0][1]);
        assert!(near(reach(stone), 7.0 * 7.0) && near(reach(glow), 7.7 * 7.0));
        // The first corner at 315°: right and down on the screen.
        assert!(stone.corners[0][0] > 0.0 && stone.corners[0][1] > 0.0);
        // At (0, 80 − 2.5, 0) with radius 0 and heading 0.
        assert!(near(stone.at[1], 77.5) && near(stone.at[0], 0.0));
        // The other picks: white and #FF5A5A.
        let tint = |pick: u32| rain_to(1, move |n| if n == 3 { pick } else { 0 }).0.draws(0.0)[0].tint;
        assert_eq!((tint(0)[1], tint(2)[1]), (1.0, 90.0 / 255.0));
    }
}
