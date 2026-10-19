//! Bursts flat on the screen 40 px over the caster's feet, as the game draws them a 16 ms tick at a time, over
//! everything in the world: Bash's (EF_BASH 16), a soft white disc and 20 rays turning out counter-clockwise; Turn
//! Undead's (EF_TURNUNDEAD 82), a disc collapsing and five rays shrinking in, then a ring opening on the ground; Sonic
//! Blow's (EF_SONICBLOW 121), a thin white ring opening round the caster.

use std::time::Instant;

use super::{ScreenShape, TICK};
use crate::world::Vector;

pub const DISC: &str = "data\\texture\\effect\\alpha_down.tga";
pub const RAY: &str = "data\\texture\\effect\\alpha_center.tga";
pub const RING: &str = "data\\texture\\effect\\ring_liner.tga";
/// Bash's effect length in ticks, its parts' life.
pub const LIFE: u32 = 40;
/// Turn Undead's effect length.
const TURN_UNDEAD_LENGTH: u32 = 250;
/// Where the parts sit: screen pixels over the feet.
const RISE: f32 = -40.0;
/// Sonic Blow's ring: its centre over the feet, how far it opens a tick and the slowing of that, and its band's width.
const BAND_RISE: f32 = -20.0;
const BAND_OPEN: f32 = 7.0;
const BAND_SLOW: f32 = 0.0875;
const BAND_WIDTH: f32 = 10.0;
/// Turn Undead's ground ring: the tick it starts, its life, and how far over the feet it lies.
const RING_START: u32 = 50;
const RING_LIFE: u32 = 30;
pub const RING_RISE: f32 = 7.0;

/// What a burst draws.
struct Look {
    /// The parts' life in ticks.
    life: u32,
    /// The disc's radius and its change a tick (pixels); its alpha's rise a tick, top and the tick it starts fading.
    radius: f32,
    shrink: f32,
    disc_alpha: (f32, f32, u32),
    rays: usize,
    /// A ray's length R(n) + base, its growth a tick ±(R(n) + base)/10, its turn slowing by turn / `drag` a tick, and
    /// its alpha.
    length: (u32, f32),
    growth: (u32, f32, f32),
    drag: f32,
    ray_alpha: (f32, f32, u32),
    /// The ground ring at tick 50.
    ring: bool,
    /// Where the parts sit over the feet (screen pixels), the rays' life, and the base of a ray's half-width (÷ 10 degrees).
    rise: f32,
    ray_life: u32,
    half_base: f32,
    /// Whether it has its disc, what a ray's growth is divided by (10, or 13), and whether a ray reloads at age 40 (its length
    /// stops and its turn becomes a constant of its own: Adrenaline Rush).
    disc: bool,
    growth_div: f32,
    reload: bool,
    /// Sonic Blow's flat ring instead of a disc and rays.
    band: bool,
}

const BASH: Look = Look {
    life: LIFE,
    radius: 100.0,
    shrink: 0.0,
    disc_alpha: (170.0 / 6.0, 170.0, 30),
    rays: 20,
    length: (40, 20.0),
    growth: (30, 20.0, 1.0),
    drag: 60.0,
    ray_alpha: (20.0, 200.0, 27),
    ring: false,
    rise: RISE,
    ray_life: LIFE,
    half_base: 5.0,
    disc: true,
    growth_div: 10.0,
    reload: false,
    band: false,
};
const TURN_UNDEAD: Look = Look {
    life: 20,
    radius: 370.0,
    shrink: -18.5,
    disc_alpha: (130.0 / 6.0, 130.0, 10),
    rays: 5,
    length: (30, 350.0),
    growth: (30, 150.0, -1.0),
    drag: 30.0,
    ray_alpha: (15.0, 150.0, 14),
    ring: true,
    rise: RISE,
    ray_life: 20,
    half_base: 5.0,
    disc: true,
    growth_div: 10.0,
    reload: false,
    band: false,
};
/// Signum Crucis' burst (half a second into its STR): Bash's with its own numbers, centred 120 px over the feet.
const SIGNUM: Look = Look {
    life: LIFE,
    radius: 100.0,
    shrink: 0.0,
    disc_alpha: (170.0 / 6.0, 170.0, 27),
    rays: 20,
    length: (60, 30.0),
    growth: (35, 95.0, 1.0),
    drag: 52.5,
    ray_alpha: (20.0, 200.0, 99),
    ring: false,
    rise: -120.0,
    ray_life: 35,
    half_base: 15.0,
    disc: true,
    growth_div: 10.0,
    reload: false,
    band: false,
};
/// Adrenaline Rush's rays: no disc, 20 rays living 80 ticks that grow for 40 ticks and then turn at a pace of their own, at full
/// alpha to the end.
const ADRENALINE: Look = Look {
    life: 80,
    radius: 0.0,
    shrink: 0.0,
    disc_alpha: (0.0, 0.0, 0),
    rays: 20,
    length: (0, 0.0),
    growth: (20, 50.0, 1.0),
    drag: 120.0,
    ray_alpha: (254.0, 254.0, 999),
    ring: false,
    rise: RISE,
    ray_life: 80,
    half_base: 5.0,
    disc: false,
    growth_div: 13.0,
    reload: true,
    band: false,
};
/// Flasher's rays: no disc, 20 rays living 70 ticks.
const FLASHER: Look = Look {
    life: 70,
    radius: 0.0,
    shrink: 0.0,
    disc_alpha: (0.0, 0.0, 0),
    rays: 20,
    length: (40, 20.0),
    growth: (30, 50.0, 1.0),
    drag: 105.0,
    ray_alpha: (20.0, 200.0, 47),
    ring: false,
    rise: RISE,
    ray_life: 70,
    half_base: 5.0,
    disc: false,
    growth_div: 10.0,
    reload: false,
    band: false,
};
const SONIC_BLOW: Look = Look {
    life: LIFE,
    radius: 0.0,
    shrink: 0.0,
    disc_alpha: (0.0, 0.0, 0),
    rays: 0,
    length: (0, 0.0),
    growth: (0, 0.0, 0.0),
    drag: 1.0,
    ray_alpha: (0.0, 0.0, 0),
    ring: false,
    rise: RISE,
    ray_life: 0,
    half_base: 0.0,
    disc: true,
    growth_div: 10.0,
    reload: false,
    band: true,
};

/// A part's alpha (0 to 255, whole) at drawn tick `n`: from `start`, up `step` a tick to `max`, then from tick `fade`
/// down evenly to nothing at `life`. Stepped tick by tick in f32, as the game does.
pub fn alpha(n: u32, start: f32, step: f32, max: f32, fade: u32, life: u32) -> f32 {
    let (mut alpha, mut step) = (start, step);
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

/// Turn Undead's ground ring at drawn tick `n` of its own: its half-size (world units) and alpha. It opens by a rate
/// from 1.5 rising 0.15 a tick, from tick 15 one from 0.5 falling a 30th.
fn ring(n: u32) -> (f32, f32) {
    let (mut half, mut rate, mut change) = (0.0_f32, 1.5_f32, 0.15_f32);
    for age in 0..=n {
        if age == 15 {
            (rate, change) = (0.5, -1.0 / 30.0);
        }
        rate += change;
        half += rate;
    }
    (half, alpha(n, 150.0, 0.0, 150.0, 20, RING_LIFE))
}

/// Sonic Blow's ring at drawn tick `n`: its outer radius, its inner radius (a filled disc on the first tick) and its alpha
/// (0 to 255, whole). It opens 7 a tick slowing by 0.175 a tick, the step taken before each draw.
fn band(n: u32) -> (f32, f32, f32) {
    let steps = (n + 1) as f32;
    let outer = BAND_OPEN * steps - BAND_SLOW * steps * (steps + 1.0);
    (outer, outer - outer.min(BAND_WIDTH), (254.0 - 6.35 * steps).trunc())
}

/// One ray: a thin triangle from the centre, clear at both ends.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Ray {
    /// Degrees, and the turn a tick at first (slowing by a fixed share of it each tick).
    angle: f32,
    turn: f32,
    /// Half its width in degrees, its length and growth a tick in pixels.
    half: f32,
    length: f32,
    growth: f32,
    /// The constant turn it takes at age 40 (Adrenaline Rush's).
    reload: f32,
}

impl Ray {
    fn new(look: &Look, mut rand: impl FnMut(u32) -> u32) -> Self {
        let mut draw = |n: u32| rand(n) as f32;
        let angle = draw(360);
        let turn = (draw(60) + 10.0) / 10.0;
        let half = (draw(25) + look.half_base) / 10.0;
        let length = if look.length.0 == 0 { look.length.1 } else { draw(look.length.0) + look.length.1 };
        let (spread, base, sign) = look.growth;
        let growth = sign * (draw(spread) + base) / look.growth_div;
        let reload = if look.reload { (draw(10) + 15.0) / 10.0 } else { 0.0 };
        Self { angle, turn, half, length, growth, reload }
    }

    /// Its angle and length at drawn tick `n`, after `n + 1` steps, its turn slowing by turn / `drag` before each. A ray that
    /// reloads has, from age 40, its length stopped and a constant turn of its own.
    fn at(&self, n: u32, look: &Look) -> (f32, f32) {
        let steps = (n.min(if look.reload { 39 } else { n }) + 1) as f32;
        let (angle, length) = (self.angle + self.turn * steps - self.turn / look.drag * steps * (steps + 1.0) / 2.0, self.length + self.growth * steps);
        if look.reload && n >= 40 { (angle + self.reload * (n - 39) as f32, length) } else { (angle, length) }
    }
}

/// One burst on a unit (None = the player).
pub struct Burst {
    start: Instant,
    pub unit: Option<u32>,
    look: &'static Look,
    rays: Vec<Ray>,
    /// The ticks its disc and rays are drawn: their life, unless the effect ends first.
    drawn: u32,
    /// A burst over a place in the world (Flasher's) rather than a unit.
    pub fixed: Option<Vector>,
}

impl Burst {
    /// Bash's burst from `now` in an effect `length` ticks long, its choices from `rand(n)` (0 to n − 1).
    pub fn bash(unit: Option<u32>, now: Instant, length: u32, rand: impl FnMut(u32) -> u32) -> Self {
        Self::new(&BASH, unit, now, length, rand)
    }

    /// Sonic Blow's ring, on the caster.
    pub fn sonic_blow(unit: Option<u32>, now: Instant) -> Self {
        Self::new(&SONIC_BLOW, unit, now, LIFE, |_| 0)
    }

    /// Adrenaline Rush's rays on a unit (None = the player).
    pub fn adrenaline(unit: Option<u32>, now: Instant, length: u32, rand: impl FnMut(u32) -> u32) -> Self {
        Self::new(&ADRENALINE, unit, now, length, rand)
    }

    /// Flasher's rays over the trap's cell `at`.
    pub fn flasher(at: Vector, now: Instant, length: u32, rand: impl FnMut(u32) -> u32) -> Self {
        Self { fixed: Some(at), ..Self::new(&FLASHER, None, now, length, rand) }
    }

    /// Signum Crucis' burst on a unit, from `start` (480 ms into the effect, whose length is `length` ticks).
    pub fn signum(unit: Option<u32>, start: Instant, length: u32, rand: impl FnMut(u32) -> u32) -> Self {
        Self::new(&SIGNUM, unit, start, length, rand)
    }

    /// Turn Undead's, on the caster.
    pub fn turn_undead(unit: Option<u32>, now: Instant, rand: impl FnMut(u32) -> u32) -> Self {
        Self::new(&TURN_UNDEAD, unit, now, TURN_UNDEAD_LENGTH, rand)
    }

    /// Parts outliving a shorter effect never show (a quick cast's aura).
    fn new(look: &'static Look, unit: Option<u32>, now: Instant, length: u32, mut rand: impl FnMut(u32) -> u32) -> Self {
        let drawn = if length < look.life { 0 } else { look.life.min(length - 1) };
        Self { start: now, unit, look, rays: (0..look.rays).map(|_| Ray::new(look, &mut rand)).collect(), drawn, fixed: None }
    }

    fn tick(&self, now: Instant) -> Option<u32> {
        Some((now.checked_duration_since(self.start)?.as_millis() / TICK.as_millis()) as u32)
    }

    pub fn alive(&self, now: Instant) -> bool {
        let end = if self.look.ring { RING_START + RING_LIFE } else { self.drawn };
        self.tick(now).is_none_or(|tick| tick < end)
    }

    /// The ground ring's half-size (world units) and alpha (0 to 1) at `now`, while it shows.
    pub fn ring(&self, now: Instant) -> Option<(f32, f32)> {
        let n = self.tick(now)?.checked_sub(RING_START).filter(|n| self.look.ring && *n < RING_LIFE)?;
        let (half, alpha) = ring(n);
        Some((half, alpha / 255.0))
    }

    /// Sonic Blow's ring at drawn tick `n`: 36 segments of 10° between the inner and outer edge, the texture repeating
    /// every four, v 1 on the inner edge and 0 on the outer; white, over the caster's feet.
    // ponytail: the screen shapes draw after the STRs, so SonicBlow.str lies under the ring; the game draws it over.
    fn band(&self, n: u32) -> Vec<ScreenShape> {
        let (outer, inner, alpha) = band(n);
        let tint = [1.0, 1.0, 1.0, alpha / 255.0];
        let center = [0.0, BAND_RISE];
        let corners = (0..36)
            .flat_map(|segment| {
                let u = (segment % 4) as f32 * 0.25;
                let [a0, a1] = [segment, segment + 1].map(|side| side as f32 * 10.0);
                let (inner0, inner1) = (point(center, inner, a0), point(center, inner, a1));
                let (outer0, outer1) = (point(center, outer, a0), point(center, outer, a1));
                [
                    (inner0, [u, 1.0], tint),
                    (outer1, [u + 0.25, 0.0], tint),
                    (outer0, [u, 0.0], tint),
                    (inner0, [u, 1.0], tint),
                    (inner1, [u + 0.25, 1.0], tint),
                    (outer1, [u + 0.25, 0.0], tint),
                ]
            })
            .collect();
        vec![(RAY, self.unit, corners)]
    }

    /// The disc, then the rays, at `now`.
    pub fn shapes(&self, now: Instant) -> Vec<ScreenShape> {
        let Some(n) = self.tick(now).filter(|n| *n < self.drawn) else { return Vec::new() };
        let look = self.look;
        if look.band {
            return self.band(n);
        }
        let center = [0.0, look.rise];
        let tint = |alpha: f32| [1.0, 1.0, 1.0, alpha / 255.0];
        // Ten 36° segments from the centre, the texture repeating every four; v 1 at the centre, 0 at the rim.
        let (step, max, fade) = look.disc_alpha;
        let disc_tint = tint(alpha(n, 0.0, step, max, fade, look.life));
        let radius = look.radius + look.shrink * (n + 1) as f32;
        let disc = (0..10)
            .flat_map(|segment| {
                let u = (segment % 4) as f32 * 0.25;
                let [from, to] = [segment, segment + 1].map(|side| point(center, radius, side as f32 * 36.0));
                [(center, [u, 1.0], disc_tint), (to, [u + 0.25, 0.0], disc_tint), (from, [u, 0.0], disc_tint)]
            })
            .collect();
        let (step, max, fade) = look.ray_alpha;
        let ray_tint = tint(alpha(n, 0.0, step, max, fade, look.life));
        let rays = self.rays.iter().filter(|_| n < look.ray_life).map(|ray| {
            let (angle, length) = ray.at(n, look);
            let corners = [(center, [0.5, 0.0]), (point(center, length, angle - ray.half), [1.0, 1.0]), (point(center, length, angle + ray.half), [0.0, 1.0])];
            (RAY, self.unit, corners.map(|(at, uv)| (at, uv, ray_tint)).to_vec())
        });
        look.disc.then_some((DISC, self.unit, disc)).into_iter().chain(rays).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bash_burst() {
        // The disc's and a ray's alpha at every drawn tick.
        let disc: Vec<f32> = (0..39).map(|n| alpha(n, 0.0, 170.0 / 6.0, 170.0, 30, LIFE)).collect();
        let mut want = vec![28.0, 56.0, 85.0, 113.0, 141.0];
        want.extend([170.0; 25]);
        want.extend([153.0, 136.0, 119.0, 102.0, 85.0, 68.0, 51.0, 34.0, 17.0]);
        assert_eq!(disc, want);
        let ray: Vec<f32> = (0..39).map(|n| alpha(n, 0.0, 20.0, 200.0, 27, LIFE)).collect();
        let mut want: Vec<f32> = (1..=10).map(|step| 20.0 * step as f32).collect();
        want.extend([200.0; 17]);
        want.extend([184.0, 169.0, 153.0, 138.0, 123.0, 107.0, 92.0, 76.0, 61.0, 46.0, 30.0, 15.0]);
        assert_eq!(ray, want);
        // A ray from fixed draws: angle 100, turn 3.0, half-width 1.0, length 30, growth 3.0.
        let mut draws = [100, 20, 5, 10, 10].into_iter();
        let ray = Ray::new(&BASH, |_| draws.next().unwrap());
        assert_eq!(ray, Ray { angle: 100.0, turn: 3.0, half: 1.0, length: 30.0, growth: 3.0, reload: 0.0 });
        let (angle, length) = ray.at(9, &BASH);
        assert!((angle - (100.0 + 3.0 * 10.0 * (1.0 - 11.0 / 120.0))).abs() < 1e-3 && length == 60.0);
        // Drawn ticks 0 to 38: the disc and 20 rays; nothing at tick 39.
        let start = Instant::now();
        let burst = Burst::bash(None, start, LIFE, |n| n / 2);
        let shapes = burst.shapes(start + TICK * 38);
        assert_eq!((shapes.len(), shapes[0].0, shapes[0].2.len(), shapes[1].0), (21, DISC, 30, RAY));
        assert_eq!(shapes[0].2[0].0, [0.0, RISE]);
        assert!(burst.shapes(start + TICK * 39).is_empty() && !burst.alive(start + TICK * 39));
        // As a 500 ms cast's aura (31 ticks) it never shows.
        assert!(Burst::bash(None, start, 31, |n| n / 2).shapes(start).is_empty());
        // A negative whole degree is looked up a degree up.
        assert_eq!(point([0.0; 2], 1.0, -0.5), point([0.0; 2], 1.0, 0.0));
        assert_eq!(point([0.0; 2], 1.0, -1.5), point([0.0; 2], 1.0, 0.0));
    }

    #[test]
    fn sonic_blow_ring() {
        let near = |got: f32, want: f32| (got - want).abs() < 1e-3;
        // Outer radius, inner radius and alpha: a disc on tick 0, then a 10 px band; 6 left at tick 38.
        let (outer, inner, alpha) = band(0);
        assert!(near(outer, 6.825) && inner == 0.0 && alpha == 247.0);
        let (outer, inner, _) = band(1);
        assert!(near(outer, 13.475) && near(inner, 3.475));
        let (outer, inner, alpha) = band(38);
        assert!(near(outer, 136.5) && near(inner, 126.5) && alpha == 6.0);
        // 36 segments of two triangles, centred 20 px over the feet, the texture repeating every four segments.
        let start = Instant::now();
        let ring = Burst::sonic_blow(Some(3), start);
        let shapes = ring.shapes(start + TICK * 38);
        assert_eq!((shapes.len(), shapes[0].0, shapes[0].1), (1, RAY, Some(3)));
        let corners = &shapes[0].2;
        assert_eq!(corners.len(), 72 * 3);
        let us: Vec<f32> = (0..36).map(|segment| corners[segment * 6].1[0]).collect();
        assert_eq!(us[..5], [0.0, 0.25, 0.5, 0.75, 0.0]);
        assert!(near(corners[2].0[0], 136.5) && near(corners[2].0[1], BAND_RISE) && corners[0].1[1] == 1.0 && corners[2].1[1] == 0.0);
        assert!(shapes[0].2.iter().all(|corner| corner.2[..3] == [1.0; 3]));
        // Gone from tick 39; the effect is 40 ticks.
        assert!(ring.shapes(start + TICK * 39).is_empty() && LIFE == 40);
    }

    #[test]
    fn adrenaline_rush_turns_rays_that_reload_at_age_40() {
        // Per ray, in order: angle, ω0, half-width, growth, ω1 (no length draw): 100, 3.0, 1.0, 5 draws then 15…24.
        let mut draws = [100, 20, 5, 13, 5].into_iter();
        let ray = Ray::new(&ADRENALINE, |_| draws.next().unwrap());
        assert_eq!(ray, Ray { angle: 100.0, turn: 3.0, half: 1.0, length: 0.0, growth: 63.0 / 13.0, reload: 2.0 });
        // Ages 0…39: angle a0 + ω0 (a + 1)(1 − (a + 2)/240), length growth (a + 1); from 40 the length stops and the turn is 2 a tick.
        let near = |got: f32, want: f32| (got - want).abs() < 1e-3;
        let (angle0, length0) = ray.at(0, &ADRENALINE);
        assert!(near(angle0, 100.0 + 3.0 * (1.0 - 2.0 / 240.0)) && near(length0, 63.0 / 13.0));
        let (angle39, length39) = ray.at(39, &ADRENALINE);
        assert!(near(angle39, 100.0 + 3.0 * 40.0 * (1.0 - 41.0 / 240.0)) && near(length39, 63.0 / 13.0 * 40.0));
        let (angle40, length40) = ray.at(40, &ADRENALINE);
        assert!(near(angle40, angle39 + 2.0) && near(length40, length39));
        assert!(near(ray.at(41, &ADRENALINE).0, angle39 + 4.0));
        // 20 rays and no disc at the screen's centre 40 px up, alpha 254 with no fade, drawn ages 0…79, gone at 80.
        let start = Instant::now();
        let burst = Burst::adrenaline(None, start, 300, |_| 1);
        assert_eq!(burst.shapes(start).len(), 20);
        assert!(burst.shapes(start).iter().all(|(path, ..)| *path == RAY));
        assert_eq!(burst.shapes(start + TICK * 79).len(), 20);
        assert!(burst.shapes(start + TICK * 80).is_empty());
        assert_eq!(burst.shapes(start + TICK * 79)[0].2[0].0, [0.0, RISE]);
        assert!((burst.shapes(start + TICK * 79)[0].2[0].2[3] - 254.0 / 255.0).abs() < 1e-3);
    }

    #[test]
    fn flasher_rays_follow_the_trap_cell() {
        // Rand order: angle, ω0, half-width, length, growth; the turn slows by ω0/105; alpha 20 a tick to 200 at age 9, 200 to 46, then down.
        let mut draws = [100, 20, 5, 0, 0].into_iter();
        let ray = Ray::new(&FLASHER, |_| draws.next().unwrap());
        assert_eq!(ray, Ray { angle: 100.0, turn: 3.0, half: 1.0, length: 20.0, growth: 5.0, reload: 0.0 });
        let rays: Vec<f32> = (0..70).map(|n| alpha(n, 0.0, 20.0, 200.0, 47, 70)).collect();
        assert_eq!((rays[0], rays[9], rays[46]), (20.0, 200.0, 200.0));
        assert_eq!((rays[47], rays[68], rays[69]), (191.0, 8.0, 0.0));
        // It sits over a cell, not a unit: its shapes are the fixed ones.
        let start = Instant::now();
        let burst = Burst::flasher([5.0, 6.0, 7.0], start, 250, |n| n / 2);
        assert_eq!(burst.fixed, Some([5.0, 6.0, 7.0]));
        assert_eq!(burst.shapes(start).len(), 20);
        assert_eq!(burst.shapes(start + TICK * 69).len(), 20);
        assert!(burst.shapes(start + TICK * 70).is_empty());
    }

    #[test]
    fn signum_crucis_burst() {
        // The disc's alpha over its 40 drawn ticks: 28 … 170 (ages 5–26), then down to 0 at age 39; rays 20 … 200 at age 9, held to age 34.
        let disc: Vec<f32> = (0..40).map(|n| alpha(n, 0.0, 170.0 / 6.0, 170.0, 27, LIFE)).collect();
        assert_eq!(disc[..6], [28.0, 56.0, 85.0, 113.0, 141.0, 170.0]);
        assert_eq!((disc[26], disc[27], disc[28], disc[38], disc[39]), (170.0, 156.0, 143.0, 13.0, 0.0));
        let ray: Vec<f32> = (0..35).map(|n| alpha(n, 0.0, 20.0, 200.0, 99, LIFE)).collect();
        assert_eq!((ray[0], ray[9], ray[10], ray[34]), (20.0, 200.0, 200.0, 200.0));
        // A ray from fixed draws: angle 100, turn 3.0, half-width (R(25) = 0 → 1.5), length 30, growth 9.5; the turn slows by turn / 52.5.
        let mut draws = [100, 20, 0, 0, 0].into_iter();
        let ray = Ray::new(&SIGNUM, |_| draws.next().unwrap());
        assert_eq!(ray, Ray { angle: 100.0, turn: 3.0, half: 1.5, length: 30.0, growth: 9.5, reload: 0.0 });
        let (angle, length) = ray.at(9, &SIGNUM);
        assert!((angle - (100.0 + 3.0 * 10.0 * (1.0 - 11.0 / 105.0))).abs() < 1e-3 && (length - (30.0 + 95.0)).abs() < 1e-3);
        // Nothing before its start; 21 shapes from it (the disc and 20 rays) for 35 ticks, the disc alone to 39, the centre 120 px up.
        let start = Instant::now();
        let burst = Burst::signum(None, start + TICK * 30, 122, |n| n / 2);
        assert!(burst.shapes(start + TICK * 29).is_empty());
        let shapes = burst.shapes(start + TICK * 30);
        assert_eq!((shapes.len(), shapes[0].0, shapes[0].2[0].0), (21, DISC, [0.0, -120.0]));
        assert_eq!(burst.shapes(start + TICK * (30 + 34)).len(), 21);
        assert_eq!(burst.shapes(start + TICK * (30 + 35)).len(), 1);
        assert!(burst.shapes(start + TICK * (30 + 40)).is_empty());
    }

    #[test]
    fn turn_undead_burst() {
        // The disc collapses from 351.5 to 0 in 20 ticks; its alpha and the rays'.
        let disc: Vec<f32> = (0..20).map(|n| alpha(n, 0.0, 130.0 / 6.0, 130.0, 10, 20)).collect();
        let mut want = vec![21.0, 43.0, 65.0, 86.0, 108.0];
        want.extend([130.0; 5]);
        want.extend([117.0, 104.0, 91.0, 78.0, 65.0, 52.0, 39.0, 26.0, 13.0, 0.0]);
        assert_eq!(disc, want);
        let ray: Vec<f32> = (0..20).map(|n| alpha(n, 0.0, 15.0, 150.0, 14, 20)).collect();
        let mut want: Vec<f32> = (1..=10).map(|step| 15.0 * step as f32).collect();
        want.extend([150.0; 4]);
        want.extend([125.0, 100.0, 75.0, 50.0, 25.0, 0.0]);
        assert_eq!(ray, want);
        // A ray 350 + 10 long, shrinking (150 + 10)/10 a tick: 344 at tick 0, 40 at tick 19.
        let mut draws = [0, 0, 0, 10, 10].into_iter();
        let ray = Ray::new(&TURN_UNDEAD, |_| draws.next().unwrap());
        assert_eq!((ray.length, ray.growth), (360.0, -16.0));
        assert_eq!((ray.at(0, &TURN_UNDEAD).1, ray.at(19, &TURN_UNDEAD).1), (344.0, 40.0));
        let start = Instant::now();
        let burst = Burst::turn_undead(Some(7), start, |n| n / 2);
        let shapes = burst.shapes(start);
        assert_eq!(shapes.len(), 6);
        assert!((shapes[0].2[2].0[0] - 351.5).abs() < 1e-3);
        assert!(burst.shapes(start + TICK * 20).is_empty() && burst.ring(start + TICK * 49).is_none());
        // The ground ring: ticks 50 to 79, opening fast for 15 ticks, then creeping to 44.
        let near = |n: u32, want: f32| (burst.ring(start + TICK * (RING_START + n)).unwrap().0 - want).abs() < 0.01;
        assert!(near(0, 1.65) && near(14, 40.5) && near(15, 40.97) && near(29, 44.0));
        assert_eq!(burst.ring(start + TICK * 69).unwrap().1, 150.0 / 255.0);
        assert_eq!(burst.ring(start + TICK * 70).unwrap().1, 135.0 / 255.0);
        assert_eq!(burst.ring(start + TICK * 79).unwrap().1, 0.0);
        assert!(burst.alive(start + TICK * 79) && !burst.alive(start + TICK * 80) && burst.ring(start + TICK * 80).is_none());
    }
}
