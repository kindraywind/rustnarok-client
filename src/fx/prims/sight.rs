//! Sight Rasher's burst (EF_SIGHTRASHER 62): 64 sprites in four waves of eight (a Sight sprite and a ground shadow each) that
//! fly out from the caster's feet, slow to a drift and fade, and a wide ring spreading over the ground.
//!
//! A part made at tick `a0` has its own age k = t − a0; the numbers are the state after that tick's step.

use std::time::Instant;

use super::passes;
use crate::world::{MeshVertex, Vector};

pub const SIGHT: &str = "data\\sprite\\이팩트\\sight";
pub const SHADOW: &str = "data\\sprite\\shadow";
pub const RING: &str = "data\\texture\\effect\\ring_yellow.tga";
/// The effect's length; its sprites end by tick 84 and its ring by tick 39.
const LENGTH: u32 = 250;
const LAST_SPRITE_TICK: u32 = 84;
/// A wave every 5 ticks to tick 15, eight sprites each; the tick the sprites slow and drift.
const WAVE_EVERY: u32 = 5;
const WAVE_UNTIL: u32 = 20;
const SLOWS_AT: u32 = 35;
/// The drift's acceleration a tick: toward −x and +z.
const DRIFT: f32 = 0.004;
/// The tick the ring is made, its life, and how wide its band is.
const RING_TICK: u32 = 10;
const RING_LIFE: u32 = 30;
const RING_WIDTH: f32 = 10.0;
/// Sight's frames are 25, a frame every `n` ticks.
const FRAMES: usize = 25;

/// One of a wave's sprites: a Sight sprite or its shadow.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Piece {
    pub sprite: &'static str,
    /// The tick it was made on, and its direction (a unit vector across the ground).
    born: u32,
    dir: [f32; 2],
    /// How high over the feet, how big (a multiplier of its pixels), and ticks to a frame.
    height: f32,
    scale: f32,
    frame_every: u32,
}

/// A sprite to draw at a moment: which, its frame, where, its size and alpha (0 to 255).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drawn {
    pub sprite: &'static str,
    pub frame: usize,
    pub at: Vector,
    pub scale: f32,
    pub alpha: f32,
}

struct Burst {
    start: Instant,
    at: Vector,
    pieces: Vec<Piece>,
}

/// The sights' and shadows' bursts.
#[derive(Default)]
pub struct Sights {
    list: Vec<Burst>,
}

/// The heading of sprite `i` of a wave: −45 i degrees, a negative whole angle looked up one degree up (0, −44, −89 … −314).
pub fn heading(i: u32) -> f32 {
    let angle = -45.0 * i as f32;
    if angle < 0.0 { angle + 1.0 } else { angle }
}

/// Its direction across the ground: (sin a, −cos a) in the game's x and z.
pub fn direction(i: u32) -> [f32; 2] {
    let (sin, cos) = heading(i).to_radians().sin_cos();
    [sin, -cos]
}

impl Burst {
    fn new(at: Vector, now: Instant, mut rand: impl FnMut(u32) -> u32) -> Self {
        let mut pieces = Vec::new();
        for born in (0..WAVE_UNTIL).step_by(WAVE_EVERY as usize) {
            let wave = born / WAVE_EVERY;
            for i in 0..8 {
                let dir = direction(i);
                pieces.push(Piece { sprite: SIGHT, born, dir, height: (20 - born / 3) as f32, scale: 2.5 - born as f32 / 10.0, frame_every: 3 + rand(2) });
                let _ = wave;
            }
            for i in 0..8 {
                pieces.push(Piece { sprite: SHADOW, born, dir: direction(i), height: 0.0, scale: 1.0 - born as f32 / 30.0, frame_every: 3 });
            }
        }
        Burst { start: now, at, pieces }
    }

    fn tick(&self, now: Instant) -> Option<u32> {
        passes(self.start, now).checked_sub(1)
    }
}

impl Piece {
    /// The radius out from the burst's point at tick `t` and the drift (−x, +z) there, while it's drawn.
    fn place(&self, t: u32) -> (f32, f32) {
        let wave = (self.born / WAVE_EVERY) as f32;
        let start = wave + 5.0;
        if t < SLOWS_AT {
            return (start + (t - self.born + 1) as f32, 0.0);
        }
        let j = (t - (SLOWS_AT - 1)) as f32;
        (start + (SLOWS_AT - self.born) as f32 + 0.5 * j, DRIFT * j * (j + 1.0) / 2.0)
    }

    /// Its alpha at tick `t`: (75 − a0)·2 until tick 64, then down by a twentieth of that each tick.
    fn alpha(&self, t: u32) -> f32 {
        let top = (75 - self.born) as f32 * 2.0;
        if t <= 64 { top } else { (top - (t - 64) as f32 * top / 20.0).max(0.0) }
    }

    /// Sight's frame at its age: one more every `n` ticks, the first drawn being 1.
    fn frame(&self, t: u32) -> usize {
        if self.sprite == SHADOW { 0 } else { ((t - self.born) / self.frame_every + 1) as usize % FRAMES }
    }
}

impl Sights {
    /// Sight Rasher's burst at the caster's feet `at` from `now`; the sprites' frame speeds from `rand(n)`.
    pub fn play(&mut self, at: Vector, now: Instant, rand: impl FnMut(u32) -> u32) {
        self.list.push(Burst::new(at, now, rand));
    }

    pub fn advance(&mut self, now: Instant) {
        self.list.retain(|burst| passes(burst.start, now) <= LENGTH);
    }

    /// The sprites at `now`: each wave's, from its tick to tick 84.
    pub fn sprites(&self, now: Instant) -> Vec<Drawn> {
        let mut drawn = Vec::new();
        for burst in &self.list {
            let Some(t) = burst.tick(now).filter(|&t| t <= LAST_SPRITE_TICK) else { continue };
            for piece in burst.pieces.iter().filter(|piece| piece.born <= t) {
                let (radius, drift) = piece.place(t);
                let at = [burst.at[0] + piece.dir[0] * radius - drift, burst.at[1] + piece.height, burst.at[2] + piece.dir[1] * radius + drift];
                drawn.push(Drawn { sprite: piece.sprite, frame: piece.frame(t), at, scale: piece.scale, alpha: piece.alpha(t) });
            }
        }
        drawn
    }

    /// The ring (made at tick 10) at `now`: a flat band on the ground, depth-tested.
    pub fn ring(&self, now: Instant) -> Vec<Vec<MeshVertex>> {
        self.list
            .iter()
            .filter_map(|burst| {
                let k = burst.tick(now)?.checked_sub(RING_TICK).filter(|&k| k < RING_LIFE)?;
                Some(ring(burst.at, k))
            })
            .collect()
    }
}

/// The ring's inner radius at age `k`: 2.5 (k + 1) − 0.0277778 (k + 1)(k + 2).
fn radius(k: u32) -> f32 {
    let n = (k + 1) as f32;
    2.5 * n - 0.027_777_8 * n * (n + 1.0)
}

/// The ring's alpha at age `k`: 150 for ages 0…19, then 15 less a tick, 0 at 29.
pub fn ring_alpha(k: u32) -> f32 {
    if k <= 19 { 150.0 } else { (150.0 - 15.0 * (k - 19) as f32).max(0.0) }
}

fn ring(at: Vector, k: u32) -> Vec<MeshVertex> {
    let inner = radius(k);
    let outer = inner + RING_WIDTH;
    let tint = [1.0, 1.0, 1.0, ring_alpha(k).trunc() / 255.0];
    (0..10)
        .flat_map(|segment| {
            let u = (segment % 4) as f32 * 0.25;
            let point = |side: u32, radius: f32, u: f32, v: f32| {
                let (sin, cos) = ((36 * side) as f32).to_radians().sin_cos();
                MeshVertex { position: [at[0] + radius * cos, at[1], at[2] + radius * sin], uv: [u, v], tint }
            };
            // The edge at R is the texture's clear top (v 0), the one at R + 10 the opaque white (v 1).
            let [a0, b0, a1, b1] = [
                point(segment, inner, u, 0.0),
                point(segment, outer, u, 1.0),
                point(segment + 1, inner, u + 0.25, 0.0),
                point(segment + 1, outer, u + 0.25, 1.0),
            ];
            [a0, b0, a1, b0, a1, b1]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn tick(start: Instant, t: u32) -> Instant {
        start + Duration::from_millis(16 * u64::from(t))
    }

    fn burst() -> (Sights, Instant) {
        let start = Instant::now();
        let mut sights = Sights::default();
        sights.play([0.0, 0.0, 0.0], start, |_| 0);
        (sights, start)
    }

    #[test]
    fn heads_every_45_degrees_looked_up_a_degree_up() {
        assert_eq!((0..8).map(heading).collect::<Vec<_>>(), [0.0, -44.0, -89.0, -134.0, -179.0, -224.0, -269.0, -314.0]);
        let near = |got: [f32; 2], want: [f32; 2]| (got[0] - want[0]).abs() < 1e-3 && (got[1] - want[1]).abs() < 1e-3;
        assert!(near(direction(0), [0.0, -1.0]) && near(direction(1), [-0.695, -0.719]) && near(direction(2), [-0.9998, -0.017]));
    }

    #[test]
    fn throws_four_waves_of_eight_and_a_shadow_each() {
        let (sights, start) = burst();
        let count = |t: u32| sights.sprites(tick(start, t)).len();
        // 16 at tick 0 (8 sights, 8 shadows), 32 at 5, 48 at 10, 64 at 15 and after, to tick 84; none after.
        assert_eq!([count(0), count(4), count(5), count(10), count(15), count(20), count(84), count(85)], [16, 16, 32, 48, 64, 64, 64, 0]);
        // Sight sprites' heights 20, 19, 17, 15 and scale 2.5, 2.0, 1.5, 1.0; shadows on the ground 1, 0.833, 0.667, 0.5.
        let at = sights.sprites(tick(start, 15));
        let sight: Vec<_> = at.iter().filter(|drawn| drawn.sprite == SIGHT).collect();
        assert_eq!(sight.len(), 32);
        let wave_height = |wave: usize| sight[wave * 8].at[1];
        assert_eq!([wave_height(0), wave_height(1), wave_height(2), wave_height(3)], [20.0, 19.0, 17.0, 15.0]);
        assert_eq!([0, 1, 2, 3].map(|wave| sight[wave * 8].scale), [2.5, 2.0, 1.5, 1.0]);
        let shadow: Vec<_> = at.iter().filter(|drawn| drawn.sprite == SHADOW).collect();
        assert!(shadow.iter().all(|drawn| drawn.at[1] == 0.0));
        let scales: Vec<f32> = [0, 1, 2, 3].map(|wave| shadow[wave * 8].scale).to_vec();
        assert!((scales[1] - 0.8333).abs() < 1e-3 && (scales[2] - 0.6667).abs() < 1e-3 && scales[3] == 0.5 && scales[0] == 1.0);
    }

    #[test]
    fn flies_out_then_slows_to_a_drift_and_fades() {
        let (sights, start) = burst();
        let radius = |t: u32, wave: usize| {
            let at = sights.sprites(tick(start, t));
            let first = at.iter().filter(|drawn| drawn.sprite == SIGHT).nth(wave * 8).unwrap();
            // Sprite 0 of a wave flies along (0, −1): its z is −radius.
            -first.at[2]
        };
        // Radius 6 at tick 0 (wave 0: start 5 + 1), 40 at tick 34; 40.5 at 35; 65, 61, 57, 53 at tick 84 for waves 0 … 3.
        assert_eq!(radius(0, 0), 6.0);
        assert_eq!(radius(34, 0), 5.0 + 35.0);
        assert!((radius(35, 0) - 40.5).abs() < 0.2, "{}", radius(35, 0));
        // (its z also holds the 5.1 drift toward +z)
        assert!([0, 1, 2, 3].map(|wave| radius(84, wave) + 5.1).iter().zip([65.0, 61.0, 57.0, 53.0]).all(|(got, want)| (got - want).abs() < 0.3));
        // The drift at tick 84 is 0.004 · 50 · 51 / 2 = 5.1 toward −x and +z on each axis.
        let at = sights.sprites(tick(start, 84));
        let first = at.iter().find(|drawn| drawn.sprite == SIGHT).unwrap();
        assert!((first.at[0] + 5.1).abs() < 0.01, "{}", first.at[0]);
        // Alpha 150, 140, 130, 120 to tick 64, 0 at 84; a wave is drawn from its tick.
        let alpha = |t: u32, wave: usize| sights.sprites(tick(start, t)).iter().filter(|drawn| drawn.sprite == SIGHT).nth(wave * 8).unwrap().alpha;
        assert_eq!([alpha(15, 0), alpha(15, 1), alpha(15, 2), alpha(15, 3)], [150.0, 140.0, 130.0, 120.0]);
        assert_eq!([alpha(64, 0), alpha(65, 0), alpha(84, 0), alpha(84, 3)], [150.0, 142.5, 0.0, 0.0]);
    }

    #[test]
    fn sight_cycles_its_frames_from_1() {
        let (sights, start) = burst();
        let frame = |t: u32| sights.sprites(tick(start, t)).iter().find(|drawn| drawn.sprite == SIGHT).unwrap().frame;
        // n = 3 (every draw 0): frame 1 at ages 0 to 2, 2 at 3 … and the frame 0 never first.
        assert_eq!([frame(0), frame(2), frame(3), frame(6)], [1, 1, 2, 3]);
        assert!(sights.sprites(tick(start, 0)).iter().filter(|drawn| drawn.sprite == SHADOW).all(|drawn| drawn.frame == 0));
        // One R(2) a Sight sprite (32 in all): n = 3 + R(2), so draws of 1 make every frame 4 ticks.
        let mut sights = Sights::default();
        let mut draws = 0;
        sights.play([0.0; 3], start, |_| {
            draws += 1;
            1
        });
        assert_eq!(draws, 32);
        assert_eq!(sights.sprites(start + Duration::from_millis(16 * 3))[0].frame, 1);
        assert_eq!(sights.sprites(start + Duration::from_millis(16 * 4))[0].frame, 2);
    }

    #[test]
    fn spreads_a_ring_over_the_ground_from_tick_10() {
        let (sights, start) = burst();
        assert!(sights.ring(tick(start, 9)).is_empty() && sights.ring(tick(start, 10)).len() == 1 && sights.ring(tick(start, 39)).len() == 1);
        assert!(sights.ring(tick(start, 40)).is_empty());
        // R 2.44 at age 0, 21.94 at 9, 38.33 at 19, 49.17 at 29; the band 10 wide.
        let near = |got: f32, want: f32| (got - want).abs() < 0.01;
        assert!(near(radius(0), 2.444) && near(radius(9), 21.94) && near(radius(19), 38.33) && near(radius(29), 49.17));
        let mesh = &sights.ring(tick(start, 10))[0];
        assert!(near(f32::hypot(mesh[0].position[0], mesh[0].position[2]), 2.444) && near(f32::hypot(mesh[1].position[0], mesh[1].position[2]), 12.444));
        assert_eq!((mesh[0].uv[1], mesh[1].uv[1]), (0.0, 1.0));
        // Alpha 150 to age 19, 135 at 20, 15 at 28, 0 at 29.
        assert_eq!([0, 19, 20, 28, 29].map(ring_alpha), [150.0, 150.0, 135.0, 15.0, 0.0]);
        // It stays where the caster stood.
        assert!(mesh.iter().all(|vertex| vertex.position[0].abs() < 60.0));
    }
}
