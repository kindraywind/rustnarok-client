//! Napalm Beat's effect (EF_NAPALMBEAT 32): 15 explosion puffs flat on the screen round the target, one a tick,
//! drawn in code when the server sends it.

use std::time::Instant;

use super::{ScreenShape, TICK};

pub const TEXTURES: [&str; 7] = [
    "data\\texture\\effect\\폭발2.tga",
    "data\\texture\\effect\\폭발3.tga",
    "data\\texture\\effect\\폭발4.tga",
    "data\\texture\\effect\\폭발5.tga",
    "data\\texture\\effect\\폭발6.tga",
    "data\\texture\\effect\\폭발7.tga",
    "data\\texture\\effect\\폭발8.tga",
];

/// How many puffs are launched: one a tick at ticks 0–14.
pub const PUFF_COUNT: usize = 15;

/// Total length of the effect in ticks: puff 14 is drawn at tick 27, and nothing shows from tick 28.
pub const LIFE: u32 = 28;

/// A puff's drawn alpha for its 14 ticks: 0–9 step up 25.4 a tick in f32 (truncated), then 254 for ticks 10–13.
pub const ALPHAS: [u8; 14] = [25, 50, 76, 101, 127, 152, 177, 203, 228, 253, 254, 254, 254, 254];

/// The texture for a puff at its tick (from 0): two ticks each of 폭발2..폭발8; tick 14 goes undrawn.
pub fn texture(tick: u32) -> Option<&'static str> {
    if tick < 14 { Some(TEXTURES[(tick / 2) as usize]) } else { None }
}

/// A puff's alpha at its tick: 0.0 to 1.0.
pub fn alpha(tick: u32) -> f32 {
    ALPHAS.get(tick as usize).map_or(0.0, |&a| a as f32 / 255.0)
}

/// One explosion puff round the target: stationary upright square drawn flat on the screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Puff {
    pub born: u32,
    pub center: [f32; 2],
    pub s: f32,
}

impl Puff {
    /// Draws d = R(50) + 10, a = R(360), s = R(40) + 20 in that order.
    pub fn new(born: u32, mut rand: impl FnMut(u32) -> u32) -> Self {
        let d = rand(50) as f32 + 10.0;
        let a = rand(360) as f32;
        let s = rand(40) as f32 + 20.0;
        let (sin, cos) = a.to_radians().sin_cos();
        let center = [sin * d, -cos * d - 20.0];
        Self { born, center, s }
    }

    /// The six vertices of the quad flat on the screen from the unit's feet.
    pub fn shape(&self, tick: u32) -> Vec<([f32; 2], [f32; 2], [f32; 4])> {
        let color = [1.0, 1.0, 1.0, alpha(tick)];
        let middle = [self.center[0] - 0.5, self.center[1] - 0.5];
        let corner = |x: f32, y: f32, uv: [f32; 2]| ([middle[0] + x, middle[1] + y], uv, color);
        let [bottom_left, top_left, bottom_right, top_right] = [
            corner(-self.s, self.s, [0.0, 1.0]),
            corner(-self.s, -self.s, [0.0, 0.0]),
            corner(self.s, self.s, [1.0, 1.0]),
            corner(self.s, -self.s, [1.0, 0.0]),
        ];
        vec![bottom_left, top_left, bottom_right, bottom_right, top_left, top_right]
    }
}

/// Napalm Beat (EF_NAPALMBEAT 32) on a unit: 15 explosion puffs, one a tick at ticks 0–14.
pub struct Napalm {
    pub start: Instant,
    pub unit: Option<u32>,
    pub puffs: [Puff; PUFF_COUNT],
}

impl Napalm {
    pub fn new(unit: Option<u32>, start: Instant, mut rand: impl FnMut(u32) -> u32) -> Self {
        let puffs = std::array::from_fn(|born| Puff::new(born as u32, &mut rand));
        Self { start, unit, puffs }
    }

    pub fn tick(&self, now: Instant) -> Option<u32> {
        let age = now.checked_duration_since(self.start)?;
        Some((age.as_millis() / TICK.as_millis()) as u32)
    }

    pub fn alive(&self, now: Instant) -> bool {
        self.tick(now).is_some_and(|tick| tick < LIFE)
    }

    pub fn shapes(&self, now: Instant) -> Vec<ScreenShape> {
        let Some(effect_tick) = self.tick(now).filter(|&t| t < LIFE) else { return Vec::new() };
        let mut shapes = Vec::new();
        for puff in &self.puffs {
            if effect_tick >= puff.born {
                let puff_tick = effect_tick - puff.born;
                if let Some(texture) = texture(puff_tick) {
                    shapes.push((texture, self.unit, puff.shape(puff_tick)));
                }
            }
        }
        shapes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn puff_texture_per_tick() {
        assert_eq!(
            (0..14).map(|t| texture(t).unwrap()).collect::<Vec<_>>(),
            [
                "data\\texture\\effect\\폭발2.tga",
                "data\\texture\\effect\\폭발2.tga",
                "data\\texture\\effect\\폭발3.tga",
                "data\\texture\\effect\\폭발3.tga",
                "data\\texture\\effect\\폭발4.tga",
                "data\\texture\\effect\\폭발4.tga",
                "data\\texture\\effect\\폭발5.tga",
                "data\\texture\\effect\\폭발5.tga",
                "data\\texture\\effect\\폭발6.tga",
                "data\\texture\\effect\\폭발6.tga",
                "data\\texture\\effect\\폭발7.tga",
                "data\\texture\\effect\\폭발7.tga",
                "data\\texture\\effect\\폭발8.tga",
                "data\\texture\\effect\\폭발8.tga",
            ]
        );
        assert_eq!(texture(14), None);
        assert_eq!(texture(15), None);
    }

    #[test]
    fn alpha_list() {
        let expected = [25, 50, 76, 101, 127, 152, 177, 203, 228, 253, 254, 254, 254, 254];
        assert_eq!(ALPHAS, expected);

        // Ticks 0–9: up 25.4 a tick in f32, truncated.
        let stepped: Vec<u8> = {
            let mut a = 0.0_f32;
            (0..10)
                .map(|_| {
                    a += 25.4;
                    a as u8
                })
                .collect()
        };
        assert_eq!(stepped, &expected[0..10]);

        for tick in 0..14 {
            assert_eq!(alpha(tick), expected[tick as usize] as f32 / 255.0);
        }
        assert_eq!(alpha(14), 0.0);
    }

    #[test]
    fn puff_centre_and_size_with_fixed_draws() {
        // Draw order: R(50) -> d = R + 10; R(360) -> a; R(40) -> s = R + 20.
        // With draws [15, 90, 10]: d = 25, a = 90, s = 30.
        let mut draws = [15, 90, 10].into_iter();
        let puff = Puff::new(0, |_| draws.next().unwrap());
        assert_eq!(puff.s, 30.0);
        assert!((puff.center[0] - 25.0).abs() < 1e-4);
        assert!((puff.center[1] - (-20.0)).abs() < 1e-4);

        // With draws [0, 0, 0]: d = 10, a = 0, s = 20.
        let mut draws = [0, 0, 0].into_iter();
        let puff = Puff::new(0, |_| draws.next().unwrap());
        assert_eq!(puff.s, 20.0);
        assert!((puff.center[0] - 0.0).abs() < 1e-4);
        assert!((puff.center[1] - (-30.0)).abs() < 1e-4);

        // With draws [49, 180, 39]: d = 59, a = 180, s = 59.
        let mut draws = [49, 180, 39].into_iter();
        let puff = Puff::new(0, |_| draws.next().unwrap());
        assert_eq!(puff.s, 59.0);
        assert!((puff.center[0] - 0.0).abs() < 1e-4);
        assert!((puff.center[1] - (39.0)).abs() < 1e-4);

        // Corners have -0.5 offset: middle is [center[0] - 0.5, center[1] - 0.5].
        let shape = puff.shape(0);
        assert_eq!(shape.len(), 6);
        // bottom_left: middle[0] - s, middle[1] + s
        assert!((shape[0].0[0] - (-59.5)).abs() < 1e-4);
        assert!((shape[0].0[1] - 97.5).abs() < 1e-4);
        assert_eq!(shape[0].1, [0.0, 1.0]);
        // top_left: middle[0] - s, middle[1] - s
        assert!((shape[1].0[0] - (-59.5)).abs() < 1e-4);
        assert!((shape[1].0[1] - (-20.5)).abs() < 1e-4);
        assert_eq!(shape[1].1, [0.0, 0.0]);
    }

    #[test]
    fn one_new_puff_a_tick_and_lifetime() {
        let start = Instant::now();
        let napalm = Napalm::new(None, start, |_| 0);

        // One new puff a tick at ticks 0–14 and none after.
        for tick in 0..=14 {
            let new_puffs = napalm.puffs.iter().filter(|p| p.born == tick).count();
            assert_eq!(new_puffs, 1, "expected 1 new puff at tick {tick}");
        }
        for tick in 15..35 {
            let new_puffs = napalm.puffs.iter().filter(|p| p.born == tick).count();
            assert_eq!(new_puffs, 0, "expected 0 new puffs at tick {tick}");
        }

        // Ticks 0..=13: count of drawn puffs grows from 1 to 14.
        assert_eq!(napalm.shapes(start).len(), 1);
        assert_eq!(napalm.shapes(start + TICK * 1).len(), 2);
        assert_eq!(napalm.shapes(start + TICK * 13).len(), 14);

        // At tick 14: puff 0 is at tick 14 (undrawn), puff 14 is at tick 0 (drawn), total 14.
        assert_eq!(napalm.shapes(start + TICK * 14).len(), 14);

        // At tick 27: only puff 14 is drawn (at puff tick 13).
        assert_eq!(napalm.shapes(start + TICK * 27).len(), 1);
        assert!(napalm.alive(start + TICK * 27));

        // Nothing drawn from tick 28; all gone after 28 ticks.
        assert_eq!(napalm.shapes(start + TICK * 28).len(), 0);
        assert!(!napalm.alive(start + TICK * 28));
    }
}
