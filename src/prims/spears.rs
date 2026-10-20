//! Great Echo's white "V" on the ground (863) and Harmonize's emitter of lights (849), a 16 ms tick at a time.
//!
//! A shape's own age n counts from 1 at its first tick.

use std::time::Instant;

use super::passes;
use super::tanji::bearing;
use super::walls::Wall;
use crate::world::{MeshVertex, Vector};

pub const LINE: &str = super::brawl::STRIP;
/// The wedges' tip length and width, and how many ticks the chevron lasts.
const TIP_LENGTH: f32 = 50.0;
const TIP_WIDTH: f32 = 5.0;
const CHEVRON_LAST: u32 = 85;
/// Harmonize: the emitter lives 500 ticks and launches on the first 299.
const EMITTER_LIFE: u32 = 500;
const LAUNCH_LAST: u32 = 299;

/// A "V" of two flat wedges where the caster stood, toward `heading`.
struct Chevron {
    start: Instant,
    at: Vector,
    heading: f32,
}

/// Harmonize's emitter on a unit: its strength, spiral radius and age.
struct Emitter {
    unit: Option<u32>,
    start: Instant,
    age: u32,
    strength: i32,
    radius: f32,
}

#[derive(Default)]
pub struct Spears {
    chevrons: Vec<Chevron>,
    emitters: Vec<Emitter>,
    /// The lights the emitters launched since last asked: the unit and the offset from its feet (effect 850 has no look yet).
    launched: Vec<(Option<u32>, Vector)>,
}

/// A chevron's alpha at its age n: 13 a tick to 195, held to 20, then 3 less a tick.
pub fn chevron_alpha(n: u32) -> f32 {
    match n {
        1..=15 => 13.0 * n as f32,
        16..=20 => 195.0,
        21..=85 => (195.0 - 3.0 * (n - 20) as f32).max(0.0),
        _ => 0.0,
    }
}

#[cfg(test)]
/// The emitter's strength after its age: 20 more a tick to 200 at 10, held, then 5 less a tick from 271.
pub fn strength_at(age: u32) -> i32 {
    let mut strength = 0;
    for n in 1..=age {
        if n <= 10 {
            strength += 20;
        } else if n > 270 {
            strength = (strength - 5).max(0);
        }
    }
    strength
}

/// The orbit point at an emitter's age: the spiral radius (5 widening by 0.05 a tick, whole units) at 6° a tick of the age mod 60.
pub fn orbit(age: u32) -> Vector {
    let radius = (5.0 + 0.05 * age as f32).trunc();
    let (sin, cos) = ((6 * (age % 60)) as f32).to_radians().sin_cos();
    [radius * sin, 25.0, radius * cos]
}

impl Spears {
    /// Great Echo's "V" over the caster's place `at` (6 up), toward the aim `to`.
    pub fn chevron(&mut self, at: Vector, to: Vector, now: Instant) {
        let heading = bearing(to[0] - at[0], to[2] - at[2]) as f32;
        self.chevrons.push(Chevron { start: now, at, heading });
    }

    /// Harmonize's emitter on `unit`.
    pub fn emitter(&mut self, unit: Option<u32>, now: Instant) {
        self.emitters.push(Emitter { unit, start: now, age: 0, strength: 0, radius: 5.0 });
    }

    /// What the emitters launched since this was last asked.
    pub fn take_launched(&mut self) -> Vec<(Option<u32>, Vector)> {
        std::mem::take(&mut self.launched)
    }

    /// Runs the emitters up to `now`, a tick at a time, and drops what ended.
    pub fn advance(&mut self, now: Instant) {
        for emitter in &mut self.emitters {
            while emitter.age < passes(emitter.start, now).min(EMITTER_LIFE) {
                emitter.age += 1;
                if emitter.age <= 10 {
                    emitter.strength += 20;
                } else if emitter.age > 270 {
                    emitter.strength = (emitter.strength - 5).max(0);
                }
                emitter.radius += 0.05;
                // ponytail: once a tick, not once a drawn frame; effect 850 has no look yet.
                if emitter.age <= LAUNCH_LAST && emitter.strength > 0 {
                    self.launched.push((emitter.unit, orbit(emitter.age)));
                }
            }
        }
        self.emitters.retain(|emitter| passes(emitter.start, now) <= EMITTER_LIFE);
        self.chevrons.retain(|chevron| passes(chevron.start, now) <= CHEVRON_LAST + 1);
    }

    /// The "V" at `now`: the wedges blended at a quarter of the alpha, then added on, in white.
    pub fn meshes(&self, now: Instant) -> Vec<Wall> {
        let mut walls = Vec::new();
        for chevron in &self.chevrons {
            let n = passes(chevron.start, now);
            let alpha = chevron_alpha(n);
            if n == 0 || alpha <= 0.0 {
                continue;
            }
            let origin = [chevron.at[0], chevron.at[1] + 6.0, chevron.at[2]];
            for (alpha, additive) in [((alpha / 4.0).floor(), false), (alpha, true)] {
                let tint = [1.0, 1.0, 1.0, alpha / 255.0];
                let mut vertices = Vec::new();
                for angle in [chevron.heading - 25.0, chevron.heading + 25.0] {
                    let (sin, cos) = angle.rem_euclid(360.0).to_radians().sin_cos();
                    let tip = [origin[0] + TIP_LENGTH * cos, origin[1], origin[2] + TIP_LENGTH * sin];
                    let (side_sin, side_cos) = (angle - 90.0).rem_euclid(360.0).to_radians().sin_cos();
                    let side = [tip[0] + TIP_WIDTH * side_cos, tip[1], tip[2] + TIP_WIDTH * side_sin];
                    vertices.extend([
                        MeshVertex { position: origin, uv: [0.0, 1.0], tint },
                        MeshVertex { position: side, uv: [1.0, 1.0], tint },
                        MeshVertex { position: tip, uv: [1.0, 0.0], tint },
                    ]);
                }
                walls.push(Wall { texture: LINE, vertices, additive, forced: false });
            }
        }
        walls
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prims::TICK;

    fn near(got: f32, want: f32) -> bool {
        (got - want).abs() < 0.05
    }

    #[test]
    fn great_echo_draws_a_white_v_toward_the_target() {
        let start = Instant::now();
        let mut spears = Spears::default();
        spears.chevron([10.0, 1.0, 10.0], [110.0, 1.0, 10.0], start);
        // Alpha at n = 1, 15, 20, 21, 85, 86: 13, 195, 195, 192, 0, ended; the blended pass a quarter, cut: 3, 48, 48, 48.
        assert_eq!([1, 15, 20, 21, 85, 86].map(chevron_alpha), [13.0, 195.0, 195.0, 192.0, 0.0, 0.0]);
        assert_eq!([1, 15, 20, 21].map(|n| (chevron_alpha(n) / 4.0).floor()), [3.0, 48.0, 48.0, 48.0]);
        // Two walls a frame (blended then added on), two wedges of one triangle each, 6 up.
        let walls = spears.meshes(start + TICK * 14);
        assert_eq!(walls.len(), 2);
        assert!(!walls[0].additive && walls[1].additive && walls.iter().all(|wall| wall.vertices.len() == 6 && wall.texture == LINE));
        assert!(walls[0].vertices.iter().all(|vertex| vertex.position[1] == 7.0));
        // Toward +x (φ = 0): the wedge at +25° has its tip at (45.3, 21.1) from the caster and the side 5 off it at (47.4, 16.6).
        let tip = walls[1].vertices[5].position;
        let side = walls[1].vertices[4].position;
        assert!(near(tip[0] - 10.0, 45.32) && near(tip[2] - 10.0, 21.13), "{tip:?}");
        assert!(near(side[0] - 10.0, 47.4) && near(side[2] - 10.0, 16.6), "{side:?}");
        // It ends at n = 86 and stays where the caster stood.
        assert!(spears.meshes(start + TICK * 85).is_empty());
        spears.advance(start + TICK * 90);
        assert!(spears.chevrons.is_empty());
    }

    #[test]
    fn harmonize_launches_lights_on_a_widening_spiral() {
        let start = Instant::now();
        let mut spears = Spears::default();
        spears.emitter(Some(1), start);
        spears.emitter(Some(2), start);
        // Strength at ages 1, 10, 11, 270, 271, 299: 20, 200, 200, 200, 195, 55.
        assert_eq!([1, 10, 11, 270, 271, 299].map(strength_at), [20, 200, 200, 200, 195, 55]);
        // The spiral radius at ages 1, 150, 295: 5, 12, 19; the point at age 15 is (5, 25 up, 0).
        let radius = |age: u32| (5.0 + 0.05 * age as f32).trunc();
        assert_eq!([1, 150, 295].map(radius), [5.0, 12.0, 19.0]);
        let point = orbit(15);
        assert!(near(point[0], 5.0) && point[1] == 25.0 && near(point[2], 0.0));
        // Launches only at ages 1 to 299, from both units; none after.
        spears.advance(start + TICK * 400);
        let launched = spears.take_launched();
        assert_eq!(launched.len(), 2 * 299);
        assert_eq!(launched.iter().filter(|(unit, _)| *unit == Some(2)).count(), 299);
        assert!(spears.take_launched().is_empty());
        spears.advance(start + TICK * 600);
        assert!(spears.emitters.is_empty());
    }
}
