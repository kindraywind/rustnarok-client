//! Bash's burst (EF_BASH 16), as the game draws it a 16 ms tick at a time: flat on the screen 40 px over the caster's
//! feet, a soft white disc and 20 rays turning counter-clockwise, over everything in the world.

use std::time::Instant;

use super::{ScreenShape, TICK};

pub const DISC: &str = "data\\texture\\effect\\alpha_down.tga";
pub const RAY: &str = "data\\texture\\effect\\alpha_center.tga";
/// The parts' life in ticks, the effect's length.
pub const LIFE: u32 = 40;
/// Where the parts sit: screen pixels over the feet.
const RISE: f32 = -40.0;
const DISC_RADIUS: f32 = 100.0;
const RAYS: usize = 20;

/// A part's alpha (0 to 255, whole) at drawn tick `n`: up `step` a tick to `max`, then from tick `fade` down evenly to
/// nothing at `life`. Stepped tick by tick in f32, as the game does.
fn alpha(n: u32, step: f32, max: f32, fade: u32, life: u32) -> f32 {
    let (mut alpha, mut step) = (0.0_f32, step);
    for age in 0..=n {
        if age == fade {
            step = -alpha / (life - age) as f32;
        }
        alpha = (alpha + step).clamp(0.0, max);
    }
    alpha.trunc()
}

/// A whole-degree point `radius` out from `center` (y down): counter-clockwise on the screen. A negative angle is
/// looked up a degree up, as the game's tables do.
fn point(center: [f32; 2], radius: f32, degrees: f32) -> [f32; 2] {
    let whole = degrees.trunc();
    let (sin, cos) = (if whole < 0.0 { whole + 1.0 } else { whole }).to_radians().sin_cos();
    [center[0] + radius * cos, center[1] - radius * sin]
}

/// One ray: a thin triangle from the centre, clear at both ends.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Ray {
    /// Degrees, and the turn a tick at first (slowing by a 60th of it each tick).
    angle: f32,
    turn: f32,
    /// Half its width in degrees, its length and growth a tick in pixels.
    half: f32,
    length: f32,
    growth: f32,
}

impl Ray {
    fn new(mut rand: impl FnMut(u32) -> u32) -> Self {
        let mut draw = |n: u32| rand(n) as f32;
        let angle = draw(360);
        let turn = (draw(60) + 10.0) / 10.0;
        let half = (draw(25) + 5.0) / 10.0;
        let length = draw(40) + 20.0;
        let growth = (draw(30) + 20.0) / 10.0;
        Self { angle, turn, half, length, growth }
    }

    /// Its angle and length at drawn tick `n`, after `n + 1` steps.
    fn at(&self, n: u32) -> (f32, f32) {
        let steps = (n + 1) as f32;
        (self.angle + self.turn * steps - self.turn / 60.0 * steps * (steps + 1.0) / 2.0, self.length + self.growth * steps)
    }
}

/// One burst on a unit (None = the player).
pub struct Burst {
    start: Instant,
    pub unit: Option<u32>,
    rays: [Ray; RAYS],
    /// The ticks it's drawn: its parts' life, unless the effect ends first.
    drawn: u32,
}

impl Burst {
    /// A burst from `now` in an effect `length` ticks long, its choices from `rand(n)` (0 to n − 1). Parts outliving a
    /// shorter effect never show (a quick cast's aura).
    pub fn new(unit: Option<u32>, now: Instant, length: u32, mut rand: impl FnMut(u32) -> u32) -> Self {
        let drawn = if length < LIFE { 0 } else { LIFE.min(length - 1) };
        Self { start: now, unit, rays: std::array::from_fn(|_| Ray::new(&mut rand)), drawn }
    }

    fn tick(&self, now: Instant) -> Option<u32> {
        let tick = (now.checked_duration_since(self.start)?.as_millis() / TICK.as_millis()) as u32;
        (tick < self.drawn).then_some(tick)
    }

    pub fn alive(&self, now: Instant) -> bool {
        now < self.start || self.tick(now).is_some()
    }

    /// The disc, then the rays, at `now`.
    pub fn shapes(&self, now: Instant) -> Vec<ScreenShape> {
        let Some(n) = self.tick(now) else { return Vec::new() };
        let center = [0.0, RISE];
        let tint = |alpha: f32| [1.0, 1.0, 1.0, alpha / 255.0];
        // Ten 36° segments from the centre, the texture repeating every four; v 1 at the centre, 0 at the rim.
        let disc_tint = tint(alpha(n, 170.0 / 6.0, 170.0, 30, LIFE));
        let disc = (0..10)
            .flat_map(|segment| {
                let u = (segment % 4) as f32 * 0.25;
                let [from, to] = [segment, segment + 1].map(|side| point(center, DISC_RADIUS, side as f32 * 36.0));
                [(center, [u, 1.0], disc_tint), (to, [u + 0.25, 0.0], disc_tint), (from, [u, 0.0], disc_tint)]
            })
            .collect();
        let ray_tint = tint(alpha(n, 20.0, 200.0, 27, LIFE));
        let rays = self.rays.iter().map(|ray| {
            let (angle, length) = ray.at(n);
            let corners = [(center, [0.5, 0.0]), (point(center, length, angle - ray.half), [1.0, 1.0]), (point(center, length, angle + ray.half), [0.0, 1.0])];
            (RAY, self.unit, corners.map(|(at, uv)| (at, uv, ray_tint)).to_vec())
        });
        std::iter::once((DISC, self.unit, disc)).chain(rays).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bash_burst() {
        // The disc's and a ray's alpha at every drawn tick.
        let disc: Vec<f32> = (0..39).map(|n| alpha(n, 170.0 / 6.0, 170.0, 30, LIFE)).collect();
        let mut want = vec![28.0, 56.0, 85.0, 113.0, 141.0];
        want.extend([170.0; 25]);
        want.extend([153.0, 136.0, 119.0, 102.0, 85.0, 68.0, 51.0, 34.0, 17.0]);
        assert_eq!(disc, want);
        let ray: Vec<f32> = (0..39).map(|n| alpha(n, 20.0, 200.0, 27, LIFE)).collect();
        let mut want: Vec<f32> = (1..=10).map(|step| 20.0 * step as f32).collect();
        want.extend([200.0; 17]);
        want.extend([184.0, 169.0, 153.0, 138.0, 123.0, 107.0, 92.0, 76.0, 61.0, 46.0, 30.0, 15.0]);
        assert_eq!(ray, want);
        // A ray from fixed draws: angle 100, turn 3.0, half-width 1.0, length 30, growth 3.0.
        let mut draws = [100, 20, 5, 10, 10].into_iter();
        let ray = Ray::new(|_| draws.next().unwrap());
        assert_eq!(ray, Ray { angle: 100.0, turn: 3.0, half: 1.0, length: 30.0, growth: 3.0 });
        let (angle, length) = ray.at(9);
        assert!((angle - (100.0 + 3.0 * 10.0 * (1.0 - 11.0 / 120.0))).abs() < 1e-3 && length == 60.0);
        // Drawn ticks 0 to 38: the disc and 20 rays; nothing at tick 39.
        let start = Instant::now();
        let burst = Burst::new(None, start, LIFE, |n| n / 2);
        let shapes = burst.shapes(start + TICK * 38);
        assert_eq!((shapes.len(), shapes[0].0, shapes[0].2.len(), shapes[1].0), (21, DISC, 30, RAY));
        assert_eq!(shapes[0].2[0].0, [0.0, RISE]);
        assert!(burst.shapes(start + TICK * 39).is_empty() && !burst.alive(start + TICK * 39));
        // As a 500 ms cast's aura (31 ticks) it never shows.
        assert!(Burst::new(None, start, 31, |n| n / 2).shapes(start).is_empty());
        // A negative whole degree is looked up a degree up.
        assert_eq!(point([0.0; 2], 1.0, -0.5), point([0.0; 2], 1.0, 0.0));
        assert_eq!(point([0.0; 2], 1.0, -1.5), point([0.0; 2], 1.0, 0.0));
    }
}
