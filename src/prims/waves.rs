//! The Rune Knight's Sonic Wave (832) and the Arch Bishop's Duple Light (833), a 16 ms tick at a time: four bubble crescents sliding
//! toward the target, and two glowing balls circling a unit, each trailing four icy panes.
//!
//! A shape's own age n counts from 1 at its first tick; a is the ticks run since the effect started minus one.

use std::time::Instant;

use super::first::THUNDER_CENTER;
use super::passes;
use super::songs::BUBBLES;
use super::tanji::bearing;
use super::walls::Wall;
use crate::render::Texture;
use crate::world::{EffectQuad, MeshVertex, Vector};

pub const PANE: &str = "data\\texture\\effect\\freeze_a.bmp";
pub const GLOW: &str = THUNDER_CENTER;
const PIXELS_PER_UNIT: f32 = 7.0;
/// Sonic Wave: four bands, the rim 30 high; ages start at −8 k.
const CRESCENT_RIM: f32 = 30.0;
const CRESCENT_QUADS: usize = 21;
/// Duple Light: two orbs of four panes.
const ORBS: usize = 2;
const PANES: usize = 4;
const PANE_TINTS: [[f32; 3]; 4] =
    [[105.0 / 255.0, 105.0 / 255.0, 1.0], [35.0 / 255.0, 35.0 / 255.0, 1.0], [75.0 / 255.0, 75.0 / 255.0, 1.0], [5.0 / 255.0, 5.0 / 255.0, 1.0]];

/// One of Sonic Wave's crescents after a tick: whether it runs, its age, alpha, lean, radius and the rim's profile.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Band {
    pub active: bool,
    pub age: i32,
    pub alpha: i32,
    pub lean: i32,
    pub radius: f32,
    pub profile: [f32; 21],
}

/// The rim's weight at point s: 0.4 at both ends, up to 1.0 over the middle.
fn weight(s: usize) -> f32 {
    match s.min(20 - s) {
        0 => 0.4,
        1 => 0.5,
        2 => 0.6,
        3 => 0.7,
        4 => 0.8,
        5 => 0.9,
        _ => 1.0,
    }
}

/// The four bands and the slide count after `ticks` ticks of the prim (`min` is /mineffect: alpha at most 125).
pub fn bands(ticks: u32, min: bool) -> ([Band; 4], u32) {
    let mut bands: [Band; 4] = std::array::from_fn(|k| Band {
        active: true,
        age: -8 * k as i32,
        alpha: 0,
        lean: 5 * (14 - k as i32),
        radius: 3.0 - 0.1 * k as f32,
        profile: [0.0; 21],
    });
    let mut slide = 0;
    let cap = if min { 125 } else { 250 };
    for _ in 0..ticks {
        for (k, band) in bands.iter_mut().enumerate() {
            if !band.active {
                continue;
            }
            band.age += 1;
            if band.age <= 0 {
                continue;
            }
            if k == 0 {
                slide += 1;
            }
            if band.age < 10 {
                band.alpha = (band.alpha + 25).min(cap);
            } else {
                band.alpha -= 10;
                if band.alpha < 0 {
                    band.alpha = 0;
                    band.lean -= 20;
                    band.radius -= 0.4;
                    band.age = 0;
                    if band.lean <= 40 {
                        band.active = false;
                        continue;
                    }
                }
            }
            let grow = if band.age <= 15 { (6.0 * band.age as f32).to_radians().sin() } else { 1.0 };
            for (s, p) in band.profile.iter_mut().enumerate() {
                *p = (CRESCENT_RIM * weight(s) * grow).max(0.0);
            }
        }
    }
    (bands, slide)
}

/// Sonic Wave's crescents where the caster stood, toward the way `heading` (degrees).
struct Crescents {
    start: Instant,
    at: Vector,
    heading: f32,
    min: bool,
}

/// One spark of an orb: its age, angle round, size, alpha and the way it opens.
#[derive(Clone, Copy, Debug)]
struct Pane {
    age: i32,
    theta: f32,
    size: f32,
    alpha: i32,
    open: f32,
}

/// One orb of Duple Light.
struct Orb {
    angles: [(i32, i32); 3],
    sigma: f32,
    orbit: f32,
    age: u32,
    panes: [Pane; PANES],
}

/// The orbs of a unit.
struct Orbs {
    unit: Option<u32>,
    start: Instant,
    orbs: Vec<Orb>,
}

#[derive(Default)]
pub struct Waves {
    crescents: Vec<Crescents>,
    orbs: Vec<Orbs>,
}

impl Orb {
    /// One tick: the glow brightens, the angles wander, the orb circles and its panes open.
    fn step(&mut self, mut rand: impl FnMut(u32) -> u32) {
        self.age += 1;
        for (current, target) in &mut self.angles {
            if *target > *current {
                *current = (*current + 1).min(359);
                if *current > *target {
                    *target = if *current == 0 { 0 } else { rand(*current as u32) as i32 };
                }
            } else {
                *current = (*current - 1).max(0);
                if *current <= *target {
                    *target = 359 - if *current == 0 { 0 } else { rand(*current as u32) as i32 };
                }
            }
        }
        if self.age.is_multiple_of(2) {
            self.orbit = (self.orbit + 1.0) % 360.0;
        }
        self.sigma = (self.sigma + 5.0 + rand(5) as f32) % 360.0;
        for pane in &mut self.panes {
            pane.age += 1;
            if pane.age > 0 {
                pane.theta = (pane.theta + 1.0) % 360.0;
                pane.size *= 1.02;
                if pane.size > 6.0 {
                    pane.alpha -= 3;
                    if pane.alpha <= 0 {
                        pane.size = 3.0;
                        pane.open = rand(360) as f32;
                        pane.alpha = 200;
                    }
                }
            }
        }
    }

    /// The glow's alpha now.
    pub fn glow_alpha(&self) -> f32 {
        self.age.min(200) as f32
    }

    /// Where it is round `feet`.
    fn place(&self, feet: Vector) -> Vector {
        let (c0, c1, c2) = (self.angles[0].0 as f32, self.angles[1].0 as f32, self.angles[2].0 as f32);
        let (sin, cos) = self.orbit.to_radians().sin_cos();
        [feet[0] + 0.5 * c0.to_radians().sin() + 7.0 * sin, feet[1] + 15.0 - 0.5 * c2.to_radians().sin(), feet[2] + 0.5 * c1.to_radians().sin() + 7.0 * cos]
    }
}

impl Pane {
    /// Its drawn size: the size with its breath of 10 %.
    fn drawn(&self) -> f32 {
        (self.size * self.theta.to_radians().sin() * 0.1 + self.size * 0.9).max(0.0)
    }
}

impl Waves {
    /// Sonic Wave from the caster standing at `from` toward `to` (/mineffect `min`).
    pub fn sonic(&mut self, from: Vector, to: Vector, min: bool, now: Instant) {
        let heading = bearing(to[0] - from[0], to[2] - from[2]) as f32;
        self.crescents.push(Crescents { start: now, at: from, heading, min });
    }

    /// Duple Light on `unit` (two orbs), the choices of each from `rand(n)` in the order the game draws them.
    pub fn duple_light(&mut self, unit: Option<u32>, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        let orbs = (0..ORBS)
            .map(|index| {
                let angles = [(); 3].map(|()| (rand(360) as i32, rand(360) as i32));
                let sigma = rand(360) as f32;
                let panes = std::array::from_fn(|k| {
                    let theta = rand(360) as f32;
                    let open = rand(360) as f32;
                    Pane { age: -20 * k as i32, theta, size: 3.0, alpha: 200, open }
                });
                Orb { angles, sigma, orbit: 360.0 * index as f32 / ORBS as f32, age: 0, panes }
            })
            .collect();
        self.orbs.push(Orbs { unit, start: now, orbs });
    }

    /// Whether `unit` has Duple Light's orbs.
    pub fn has_orbs(&self, unit: Option<u32>) -> bool {
        self.orbs.iter().any(|orbs| orbs.unit == unit)
    }

    /// A status of `unit`'s went off or came on again: its orbs go first (Duple Light's, EFST 350).
    pub fn forget_status(&mut self, unit: Option<u32>, status: u16) {
        if status == 350 {
            self.orbs.retain(|orbs| orbs.unit != unit);
        }
    }

    pub fn forget(&mut self, unit: u32) {
        self.orbs.retain(|orbs| orbs.unit != Some(unit));
    }

    /// Runs the orbs up to `now`, a tick at a time.
    pub fn advance(&mut self, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        for orbs in &mut self.orbs {
            let due = passes(orbs.start, now);
            for orb in &mut orbs.orbs {
                while orb.age < due {
                    orb.step(&mut rand);
                }
            }
        }
        self.crescents.retain(|crescents| passes(crescents.start, now) <= 80);
    }

    /// Sonic Wave's crescents (blended) and Duple Light's panes (added on) at `now`.
    pub fn meshes(&self, now: Instant, position_of: impl Fn(Option<u32>) -> Option<Vector>) -> Vec<Wall> {
        let mut walls = Vec::new();
        for crescents in &self.crescents {
            let Some(a) = passes(crescents.start, now).checked_sub(1).filter(|&a| a < 80) else { continue };
            let (bands, slide) = bands(a + 1, crescents.min);
            let (sin_h, cos_h) = crescents.heading.to_radians().sin_cos();
            let turn = (crescents.heading + 195.0) % 360.0;
            let d = slide as f32;
            for band in bands.iter().filter(|band| band.active && band.age > 0) {
                let picture = BUBBLES[2 * (band.age as usize % 13) + 1];
                let (lean_sin, lean_cos) = (band.lean as f32).to_radians().sin_cos();
                let tint = [1.0, 1.0, 1.0, band.alpha as f32 / 255.0];
                let point = |s: usize| {
                    let (sin, cos) = ((15.0 * s as f32 + turn) % 360.0).to_radians().sin_cos();
                    let p = band.profile[s.min(20)];
                    let r = band.radius;
                    let foot = [crescents.at[0] + r * cos + 0.75 * d * cos_h, crescents.at[1], crescents.at[2] + r * sin + 0.75 * d * sin_h];
                    let rim = [
                        crescents.at[0] + (p * lean_cos + r) * cos + d * cos_h,
                        crescents.at[1] + p * lean_sin,
                        crescents.at[2] + (p * lean_cos + r) * sin + d * sin_h,
                    ];
                    (foot, rim)
                };
                let mut vertices = Vec::new();
                for s in 1..=CRESCENT_QUADS {
                    let ((foot0, rim0), (foot1, rim1)) = (point(s - 1), point(s));
                    let j = (s - 1) as f32;
                    let [p, q, r, t] = [
                        MeshVertex { position: foot0, uv: [j / 21.0, 1.0], tint },
                        MeshVertex { position: foot1, uv: [(j + 1.0) / 21.0, 1.0], tint },
                        MeshVertex { position: rim1, uv: [(j + 1.0) / 21.0, 0.0], tint },
                        MeshVertex { position: rim0, uv: [j / 21.0, 0.0], tint },
                    ];
                    vertices.extend([p, q, r, p, r, t]);
                }
                walls.push(Wall { texture: picture, vertices, additive: false, forced: false });
            }
        }
        for orbs in &self.orbs {
            let Some(feet) = position_of(orbs.unit) else { continue };
            for orb in &orbs.orbs {
                if orb.glow_alpha() <= 0.0 {
                    continue;
                }
                let at = orb.place(feet);
                let mut vertices = Vec::new();
                for (k, pane) in orb.panes.iter().enumerate() {
                    let size = pane.drawn();
                    if pane.age <= 0 || size <= 0.0 {
                        continue;
                    }
                    let w = 0.5 * size;
                    let other = (pane.open + 180.0) % 360.0;
                    let (sin_a, cos_a) = pane.open.to_radians().sin_cos();
                    let (sin_b, cos_b) = other.to_radians().sin_cos();
                    let q1 = [at[0] + w * cos_a, at[1], at[2] + w * sin_a];
                    let q2 = [at[0] + w * cos_b, at[1], at[2] + w * sin_b];
                    let tint = [PANE_TINTS[k][0], PANE_TINTS[k][1], PANE_TINTS[k][2], pane.alpha as f32 / 255.0];
                    let low = |q: Vector| [q[0], q[1] - w, q[2]];
                    let [a, b, c, d] = [
                        MeshVertex { position: low(q1), uv: [0.0, 1.0], tint },
                        MeshVertex { position: low(q2), uv: [1.0, 1.0], tint },
                        MeshVertex { position: q2, uv: [1.0, 0.0], tint },
                        MeshVertex { position: q1, uv: [0.0, 0.0], tint },
                    ];
                    vertices.extend([a, b, c, a, c, d]);
                }
                if !vertices.is_empty() {
                    walls.push(Wall { texture: PANE, vertices, additive: true, forced: false });
                }
            }
        }
        walls
    }

    /// Duple Light's glows and cores: camera-facing squares at `now`.
    pub fn quads<'a>(
        &self,
        now: Instant,
        texture_of: impl Fn(&str) -> Option<&'a Texture>,
        position_of: &impl Fn(Option<u32>) -> Option<Vector>,
    ) -> Vec<EffectQuad<'a>> {
        let Some(texture) = texture_of(GLOW) else { return Vec::new() };
        let mut quads = Vec::new();
        for orbs in &self.orbs {
            let (Some(feet), true) = (position_of(orbs.unit), passes(orbs.start, now) > 0) else { continue };
            for orb in &orbs.orbs {
                let alpha = orb.glow_alpha() / 255.0;
                if alpha <= 0.0 {
                    continue;
                }
                let sin = orb.sigma.to_radians().sin();
                for (rho, rgb) in [(1.0 + 0.05 * sin, [68.0 / 255.0, 42.0 / 255.0, 1.0]), (0.5 + 0.05 * sin, [155.0 / 255.0, 155.0 / 255.0, 1.0])] {
                    let r = rho * PIXELS_PER_UNIT;
                    let corners = [[-r, 0.0], [0.0, r], [r, 0.0], [0.0, -r]];
                    quads.push((texture, orb.place(feet), corners, [rgb[0], rgb[1], rgb[2], alpha], true, false));
                }
            }
        }
        quads
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
    fn sonic_wave_slides_four_crescents_toward_the_target() {
        // The start values: ages 0, −8, −16, −24, leans 70, 65, 60, 55, radii 3.0, 2.9, 2.8, 2.7.
        let (start, _) = bands(0, false);
        assert_eq!(start.map(|band| band.age), [0, -8, -16, -24]);
        assert_eq!(start.map(|band| band.lean), [70, 65, 60, 55]);
        assert_eq!(start.map(|band| band.radius), [3.0, 2.9, 2.8, 2.7].map(|value: f32| value));
        // Band 0's alpha at ages 1, 9, 10, 31, then the restart at 32 (age 0, alpha 0, lean 50).
        let band0 = |ticks: u32, min: bool| bands(ticks, min).0[0];
        assert_eq!([1, 9, 10, 31].map(|age| band0(age, false).alpha), [25, 225, 215, 5]);
        let restart = band0(32, false);
        assert_eq!((restart.age, restart.alpha, restart.lean), (0, 0, 50));
        assert!(near(restart.radius, 2.6));
        // With /mineffect: 125 from age 5, 115 at 10, 5 at 21, the restart at 22.
        assert_eq!([5, 9, 10, 21].map(|age| band0(age, true).alpha), [125, 125, 115, 5]);
        assert_eq!(band0(22, true).age, 0);
        // Band 0 ends after its second restart (tick 63), band 1 after its own (71), bands 2 and 3 after the first (47, 55).
        assert!(bands(63, false).0[0].active && !bands(64, false).0[0].active);
        assert!(bands(71, false).0[1].active && !bands(72, false).0[1].active);
        assert!(bands(47, false).0[2].active && !bands(48, false).0[2].active);
        assert!(bands(55, false).0[3].active && !bands(56, false).0[3].active);
        // The slide counts band 0's ticks to 64.
        assert_eq!(bands(80, false).1, 64);
        assert_eq!(bands(1, false).1, 1);
        // The rim at age 1 for s = 0, 10: 1.25, 3.14; at age 15 for s = 0, 10, 20: 12.0, 30.0, 12.0.
        let (profile1, profile15) = (band0(1, false).profile, band0(15, false).profile);
        assert!(near(profile1[0], 1.25) && near(profile1[10], 3.136));
        assert!(near(profile15[0], 12.0) && near(profile15[10], 30.0) && near(profile15[20], 12.0));
        // Drawn while alpha > 0: band 0 on ticks 0 to 30 and 32 to 62, band 1 on 8 to 38 and 40 to 70, nothing after 70.
        let drawn = |k: usize, tick: u32| {
            let band = bands(tick + 1, false).0[k];
            band.active && band.age > 0
        };
        assert_eq!([0, 30, 31, 32, 62, 63].map(|tick| drawn(0, tick)), [true, true, false, true, true, false]);
        assert_eq!([7, 8, 38, 39, 40, 70, 71].map(|tick| drawn(1, tick)), [false, true, true, false, true, true, false]);
        let start_time = Instant::now();
        let mut waves = Waves::default();
        waves.sonic([0.0; 3], [40.0, 0.0, 0.0], false, start_time);
        let shown = |tick: u32| waves.meshes(start_time + TICK * tick, |_| None).len();
        assert_eq!([0, 7, 8, 71, 72].map(shown), [1, 1, 2, 0, 0]);
        // The picture is the age mod 13: W_bubble04 (index 1) at age 14 … the first at age 13 is W_bubble02.
        let wall = &waves.meshes(start_time, |_| None)[0];
        assert_eq!(wall.texture, BUBBLES[3]);
        assert!(!wall.additive && wall.vertices.len() == 21 * 6);
        // The crescent stands where the caster did and slides 1 unit a tick along +x (the way to the target).
        let tip = |tick: u32| waves.meshes(start_time + TICK * tick, |_| None)[0].vertices[0].position;
        assert!(near(tip(10)[0] - tip(9)[0], 0.75));
    }

    #[test]
    fn duple_light_circles_two_glowing_balls_with_four_panes_each() {
        let start = Instant::now();
        let mut waves = Waves::default();
        let mut draws = 0;
        waves.duple_light(Some(1), start, |_| {
            draws += 1;
            0
        });
        // 15 draws an orb, 30 in all; the orbits start opposite; one effect to a unit; a status takes it first.
        assert_eq!(draws, 30);
        assert_eq!(waves.orbs[0].orbs.iter().map(|orb| orb.orbit).collect::<Vec<_>>(), [0.0, 180.0]);
        assert!(waves.has_orbs(Some(1)) && !waves.has_orbs(Some(2)));
        // Run 110 ticks with every draw 0.
        for tick in 0..110 {
            waves.advance(start + TICK * tick, |_| 0);
        }
        let orb = &waves.orbs[0].orbs[0];
        // The glow brightens 1 a tick to 200; φ advances every second tick.
        assert_eq!(orb.age, 110);
        assert_eq!(orb.orbit, 55.0);
        assert_eq!([1, 200, 201].map(|age| age.min(200)), [1, 200, 200]);
        // Pane 1 first shows at orb tick 21; its size 3 · 1.02^m: 3.06 at m = 1, 6.0 at 35, 6.12 at 36 with alpha 197.
        let size = |m: i32| 3.0 * 1.02_f32.powi(m);
        assert!(near(size(1), 3.06) && near(size(35), 6.0) && near(size(36), 6.12));
        let pane = |ticks: u32| {
            let mut test = Waves::default();
            test.duple_light(None, start, |_| 0);
            for tick in 0..ticks {
                test.advance(start + TICK * tick, |_| 0);
            }
            test.orbs[0].orbs[0].panes[1]
        };
        assert_eq!(pane(20).age, 0);
        assert_eq!(pane(21).age, 1);
        let at36 = pane(20 + 36);
        assert!(near(at36.size, 6.12) && at36.alpha == 197);
        let at102 = pane(20 + 102);
        assert!(near(at102.size, 3.0) && at102.alpha == 200, "{at102:?}");
        let at101 = pane(20 + 101);
        assert!((at101.size - 22.2).abs() < 0.05 && at101.alpha == 2, "{at101:?}");
        // The wander step: (10, 20) → c = 11 and t unchanged; (20, 19) → c = 19 and t = 359 − R(19); (0, 0) → c = 0 and t = 359.
        let mut orb = Orb { angles: [(10, 20), (20, 19), (0, 0)], sigma: 0.0, orbit: 0.0, age: 0, panes: orb_panes() };
        orb.step(|_| 5);
        assert_eq!(orb.angles, [(11, 20), (19, 354), (0, 359)]);
        // The glow and the core breathe 5 %: between 0.95 and 1.05, and 0.45 and 0.55.
        assert!((0.95..=1.05).contains(&(1.0 + 0.05 * 1.0_f32.sin())) && (0.45..=0.55).contains(&(0.5 + 0.05 * (-1.0_f32).sin())));
        assert_eq!(PANE_TINTS[0], [105.0 / 255.0, 105.0 / 255.0, 1.0]);
        // A status packet takes the unit's orbs; the player's other statuses leave them.
        waves.forget_status(Some(1), 349);
        assert!(waves.has_orbs(Some(1)));
        waves.forget_status(Some(1), 350);
        assert!(!waves.has_orbs(Some(1)));
    }

    fn orb_panes() -> [Pane; PANES] {
        std::array::from_fn(|k| Pane { age: -20 * k as i32, theta: 0.0, size: 3.0, alpha: 200, open: 0.0 })
    }
}
