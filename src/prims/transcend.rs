//! The High Wizard's, Professor's and Priest's skills' code-made shapes, a 16 ms tick at a time: Gospel's bobbing cross (370), the
//! bodies Soul Drain, Magic Crasher and Indulge colour (378 to 383), and Soul Change's glowing tube between two units (384).
//!
//! A shape's own age n counts from 1 at its first tick; e is the ticks since the effect started.

use std::time::Instant;

use super::passes;
use super::walls::Wall;
use crate::world::{MeshVertex, Vector};

pub const CROSS: &str = "data\\texture\\effect\\cross_old.bmp";
/// The status of the Marionette control, which ends the tube.
pub const MARIONETTE_STATUS: u16 = 121;
pub const TUBE: &str = "data\\texture\\effect\\alpha_center.tga";
/// Shadow Form's dark grey tube (819) and the status that ends it.
pub const CLOUD: &str = "data\\texture\\effect\\cloud11.tga";
pub const SHADOW_STATUS: u16 = 394;

/// A Gospel's cross over a skill unit's cell.
struct Gospel {
    id: u32,
    at: Vector,
    start: Instant,
    turn: f32,
}

/// What colours a body: Soul Drain's target (a blue that deepens) and caster (a blue flash), Magic Crasher's target (random
/// colours) and Indulge's caster (violet outline copies).
enum Body {
    DrainTarget,
    DrainCaster,
    Crasher([[u8; 3]; 31]),
    /// Mind Breaker's colour a tick on ticks 0 to 60, drawn as it is.
    Breaker([[u8; 3]; 61]),
    Indulge,
}

struct Coloured {
    unit: Option<u32>,
    start: Instant,
    body: Body,
}

/// Soul Change's tube between the caster and the unit the packet names by id.
struct Link {
    /// Marionette Control's pink tube (395), which breathes and stays until the control's status packet, instead of Soul Change's.
    marionette: bool,
    /// Shadow Form's dark grey tube (819): it pulses between alpha 43 and 53 and stays until its status goes.
    shadow: bool,
    /// Soul Unity's yellow tube with a blue edge (1210): the same pulse, for the effect's 200 ticks.
    unity: bool,
    caster: Option<u32>,
    /// The id of the target as the game reads it, through a 32-bit float.
    aim: u32,
    own: u32,
    start: Instant,
    lost: bool,
}

/// A cross to draw: where, its corners in sprite pixels (y down) and its alpha (0 to 255).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CrossQuad {
    pub at: Vector,
    pub corners: [[f32; 2]; 4],
    pub alpha: f32,
}

#[derive(Default)]
pub struct Transcend {
    gospels: Vec<Gospel>,
    bodies: Vec<Coloured>,
    links: Vec<Link>,
}

/// A Gospel cross at its count n: turned `turn0 + n`, 6 − 3 sin(turn) up, a radius that breathes 1.5 ± 0.075.
pub fn gospel_state(turn0: f32, n: u32) -> (f32, f32) {
    let turn = (turn0 + n as f32) % 360.0;
    let wobble = (5 * n) as f32 % 360.0;
    (6.0 - 3.0 * turn.to_radians().sin(), 1.5 + 1.5 * wobble.to_radians().sin() * 0.05)
}

/// The corners of a Gospel cross of radius r in sprite pixels (7 to a unit), y down: top left uv (0, 0), top right (1, 0),
/// bottom right (1, 1), bottom left (0, 1), in the order of the quad's texture coordinates (0, 1), (1, 1), (1, 0), (0, 0).
pub fn gospel_corners(radius: f32) -> [[f32; 2]; 4] {
    let half = radius * std::f32::consts::FRAC_1_SQRT_2 * 7.0;
    [[-half, half], [half, half], [half, -half], [-half, -half]]
}

/// The tube's alpha at its count n: 6 a tick to 114, held, then 3 less a tick from 41; drawn for 1 to 77.
pub fn tube_alpha(n: u32) -> f32 {
    match n {
        1..=19 => 6.0 * n as f32,
        20..=40 => 114.0,
        41..=77 => 114.0 - 3.0 * (n - 40) as f32,
        _ => 0.0,
    }
}

/// Marionette's tube radius at its count n: 0.7 and 0.005 more for the sine of each degree so far, a 360-tick breath.
pub fn marionette_width(n: u32) -> f32 {
    // The sum of sin(k°) for k = 1…n.
    let sum = (f64::from(n) / 2.0).to_radians().sin() * (f64::from(n + 1) / 2.0).to_radians().sin() / 0.5_f64.to_radians().sin();
    0.7 + 0.005 * sum as f32
}

/// Marionette's tube alpha at n: 1 a tick to 80, held for good.
pub fn marionette_alpha(n: u32) -> f32 {
    n.min(80) as f32
}

/// Shadow Form's tube alpha at n: 45 and 2 more a tick while n mod 10 is under 5, else 2 less.
pub fn shadow_alpha(n: u32) -> f32 {
    45.0 + (1..=n).map(|i| if i % 10 < 5 { 2.0 } else { -2.0 }).sum::<f32>()
}

/// How far along the line the tube's far end is at n: sin(6n)° of the way for n under 15, then at the target.
pub fn tube_growth(n: u32) -> f32 {
    if n < 15 { (6.0 * n as f32).to_radians().sin() } else { 1.0 }
}

/// The unit a Soul Change names by `id` (read through a 32-bit float): the player when the float is the player's own id's,
/// else the unit whose id is the float truncated.
pub fn linked_unit(id: u32, own: u32) -> Option<u32> {
    let aim = id as f32;
    (aim != own as f32).then_some(aim as u32)
}

/// What the colour of a body is `drift` ticks after a flash left it at `from`: it fades back to white by 8 a channel a tick.
fn fade_back(from: [u8; 3], ticks: u32) -> Option<[u8; 3]> {
    let drift = (ticks * 8) as u16;
    let tint = from.map(|channel| (u16::from(channel) + drift).min(255) as u8);
    (tint != [255; 3]).then_some(tint)
}

impl Transcend {
    /// Gospel's cross over the cell `at` of skill unit `id`; its turn from `rand(n)`.
    pub fn keep_gospel(&mut self, id: u32, at: Vector, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        self.gospels.push(Gospel { id, at, start: now, turn: rand(360) as f32 });
    }

    pub fn forget(&mut self, id: u32) {
        self.gospels.retain(|gospel| gospel.id != id);
    }

    /// Soul Drain's target (379) takes a deepening blue for 200 ticks.
    pub fn drain_target(&mut self, unit: Option<u32>, now: Instant) {
        self.bodies.push(Coloured { unit, start: now, body: Body::DrainTarget });
    }

    /// Soul Drain's caster (378) flashes blue on ticks 55 to 65.
    pub fn drain_caster(&mut self, unit: Option<u32>, now: Instant) {
        self.bodies.push(Coloured { unit, start: now, body: Body::DrainCaster });
    }

    /// Magic Crasher's target (380) flickers in a colour a tick on ticks 30 to 60: per tick the blue, the green, then the red.
    pub fn crasher(&mut self, unit: Option<u32>, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        let colours = [(); 31].map(|()| {
            let blue = rand(255) as u8;
            let green = rand(255) as u8;
            let red = rand(255) as u8;
            [red, green, blue]
        });
        self.bodies.push(Coloured { unit, start: now, body: Body::Crasher(colours) });
    }

    /// Indulge's caster (383) glows in outline copies on ticks 50 to 80.
    pub fn indulge(&mut self, unit: Option<u32>, now: Instant) {
        self.bodies.push(Coloured { unit, start: now, body: Body::Indulge });
    }

    /// Soul Change's tube from `caster` to the unit named by `target` (the player is `own`).
    pub fn link(&mut self, caster: Option<u32>, target: u32, own: u32, now: Instant) {
        self.links.push(Link { marionette: false, shadow: false, unity: false, caster, aim: target, own, start: now, lost: false });
    }

    /// Soul Unity's tube (1210) from `unit` (the target) to the unit the packet names by the id `aim` (the caster).
    pub fn unity(&mut self, unit: Option<u32>, aim: u32, own: u32, now: Instant) {
        self.links.push(Link { marionette: false, shadow: false, unity: true, caster: unit, aim, own, start: now, lost: false });
    }

    /// Marionette Control's tube (395): it stays until the status packet of the control (EFST 121) takes it.
    pub fn marionette(&mut self, caster: Option<u32>, target: u32, own: u32, now: Instant) {
        self.links.push(Link { marionette: true, shadow: false, unity: false, caster, aim: target, own, start: now, lost: false });
    }

    /// Shadow Form's tube (819) from `caster` to the unit the packet names, replacing the caster's last.
    pub fn shadow(&mut self, caster: Option<u32>, target: u32, own: u32, now: Instant) {
        self.links.retain(|link| !(link.shadow && link.caster == caster));
        self.links.push(Link { marionette: false, shadow: true, unity: false, caster, aim: target, own, start: now, lost: false });
    }

    /// A status packet of the Marionette control (EFST 121), on or off, takes the unit's tube.
    pub fn forget_status(&mut self, unit: Option<u32>, status: u16) {
        if status == MARIONETTE_STATUS {
            self.links.retain(|link| !(link.marionette && link.caster == unit));
        }
        if status == SHADOW_STATUS {
            self.links.retain(|link| !(link.shadow && link.caster == unit));
        }
    }

    /// Mind Breaker's flicker on its target: a colour a tick on ticks 0 to 60 (the blue, the green, then the red).
    pub fn breaker(&mut self, unit: Option<u32>, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        let colours = [(); 61].map(|()| {
            let blue = rand(255) as u8;
            let green = rand(255) as u8;
            let red = rand(255) as u8;
            [red, green, blue]
        });
        self.bodies.push(Coloured { unit, start: now, body: Body::Breaker(colours) });
    }

    /// A tube whose end is not found for a tick is hidden for good; what ended goes.
    pub fn advance(&mut self, now: Instant, position_of: impl Fn(Option<u32>) -> Option<Vector>) {
        for link in &mut self.links {
            let target = linked_unit(link.aim, link.own);
            let started = passes(link.start, now) >= 1;
            if started && !link.lost && (position_of(link.caster).is_none() || position_of(target).is_none()) {
                link.lost = true;
            }
        }
        self.bodies.retain(|coloured| passes(coloured.start, now) <= 260);
        self.links.retain(|link| link.marionette || link.shadow || passes(link.start, now) <= 200);
    }

    /// The crosses to draw: over each Gospel's cell. Alpha 200, or at most 100 on `dim` maps.
    pub fn crosses(&self, now: Instant, dim: bool) -> Vec<CrossQuad> {
        self.gospels
            .iter()
            .filter_map(|gospel| {
                let n = passes(gospel.start, now);
                (n >= 1).then(|| {
                    let (height, radius) = gospel_state(gospel.turn, n);
                    CrossQuad {
                        at: [gospel.at[0], gospel.at[1] + height, gospel.at[2]],
                        corners: gospel_corners(radius),
                        alpha: if dim { 100.0 } else { 200.0 },
                    }
                })
            })
            .collect()
    }

    /// The body flags and colour an effect gives `unit` at `now`.
    pub fn glow(&self, unit: Option<u32>, now: Instant) -> Option<(u32, Option<[u8; 3]>)> {
        self.bodies.iter().filter(|coloured| coloured.unit == unit).find_map(|coloured| {
            let e = passes(coloured.start, now).checked_sub(1)?;
            match &coloured.body {
                Body::DrainCaster => (55..=65).contains(&e).then_some((crate::actor::GLOW_ADDITIVE, Some([0x64, 0x64, 0xFF]))),
                Body::Crasher(colours) => (30..=60).contains(&e).then(|| (crate::actor::GLOW_ADDITIVE, Some(colours[(e - 30) as usize]))),
                Body::Indulge => (50..=80).contains(&e).then(|| {
                    let level = (2 * (125 - e as i32)) as u8;
                    (crate::actor::GLOW_OUTLINE, Some([level, level, 250]))
                }),
                Body::DrainTarget | Body::Breaker(_) => None,
            }
        })
    }

    /// The colour a body is drawn in: Soul Drain's target on ticks 0 to 199, and what the flashes leave, fading back to white.
    pub fn body_tint(&self, unit: Option<u32>, now: Instant) -> Option<[u8; 3]> {
        self.bodies.iter().filter(|coloured| coloured.unit == unit).find_map(|coloured| {
            let e = passes(coloured.start, now).checked_sub(1)?;
            match &coloured.body {
                Body::DrainTarget if e < 200 => Some([(205 - e) as u8, (205 - e) as u8, 255]),
                Body::DrainTarget => fade_back([6, 6, 255], e - 199),
                Body::Crasher(colours) if e > 60 => fade_back(colours[30], e - 60),
                Body::Breaker(colours) if e <= 60 => Some(colours[e as usize]),
                Body::Breaker(colours) => fade_back(colours[60], e - 60),
                _ => None,
            }
        })
    }

    /// Soul Change's tubes at `now`: an inner blue and an outer orange, 10 facets each; `position_of` gives a unit's feet.
    pub fn meshes(&self, now: Instant, position_of: impl Fn(Option<u32>) -> Option<Vector>) -> Vec<Wall> {
        let mut walls = Vec::new();
        for link in self.links.iter().filter(|link| !link.lost) {
            let n = passes(link.start, now);
            if n == 0 || (!link.marionette && !link.shadow && !link.unity && n > 77) || (link.unity && n > 199) {
                continue;
            }
            let target = linked_unit(link.aim, link.own);
            let (Some(caster), Some(far)) = (position_of(link.caster), position_of(target)) else { continue };
            let (p0, p1) = ([caster[0], caster[1] + 8.0, caster[2]], [far[0], far[1] + 8.0, far[2]]);
            let grow = tube_growth(n);
            let end = [p0[0] + (p1[0] - p0[0]) * grow, p0[1] + (p1[1] - p0[1]) * grow, p0[2] + (p1[2] - p0[2]) * grow];
            // The bearing A of (P0 − P1) in x/z (x = cos, z = sin) turned a quarter: the facets are round the line, across it.
            let bearing = super::tanji::bearing(p0[0] - p1[0], p0[2] - p1[2]) as f32;
            let (sin, cos) = ((bearing + 90.0) % 360.0).to_radians().sin_cos();
            let shadow = [(0.4, shadow_alpha(n)), (0.8, shadow_alpha(n))];
            let tubes = if link.unity {
                // A thin yellow core and a wider blue edge on the pulse of Shadow Form's.
                let alpha = shadow_alpha(n);
                [(0.4, [1.0, 1.0, 0.0], alpha), (0.8, [0.0, 0.0, 1.0], (2.0 * alpha).min(255.0))]
            } else if link.shadow {
                shadow.map(|(width, alpha)| (width, [50.0 / 255.0; 3], (alpha / 2.0).floor()))
            } else if link.marionette {
                let (width, alpha) = (marionette_width(n), marionette_alpha(n));
                let pink = [1.0, 89.0 / 255.0, 182.0 / 255.0];
                [(width, pink, alpha), (2.0 * width, pink, 2.0 * alpha)]
            } else {
                let alpha = tube_alpha(n);
                [(0.2, [0.0, 100.0 / 255.0, 150.0 / 255.0], alpha), (0.4, [1.0, 50.0 / 255.0, 0.0], (2.0 * alpha).min(255.0))]
            };
            for (width, rgb, vertex_alpha) in tubes {
                let tint = [rgb[0], rgb[1], rgb[2], vertex_alpha / 255.0];
                let ring = |at: Vector, j: u32| {
                    let (phi_sin, phi_cos) = ((36 * j) as f32).to_radians().sin_cos();
                    [at[0] + width * phi_cos * cos, at[1] + width * phi_sin, at[2] + width * phi_cos * sin]
                };
                let mut vertices = Vec::new();
                for j in 1..=10 {
                    let [a, b, c, d] = [
                        MeshVertex { position: ring(p0, j - 1), uv: [0.0, 1.0], tint },
                        MeshVertex { position: ring(p0, j), uv: [1.0, 1.0], tint },
                        MeshVertex { position: ring(end, j), uv: [1.0, 0.0], tint },
                        MeshVertex { position: ring(end, j - 1), uv: [0.0, 0.0], tint },
                    ];
                    vertices.extend([a, b, c, a, c, d]);
                }
                walls.push(if link.unity {
                    Wall { texture: CLOUD, vertices, additive: true, forced: false }
                } else if link.shadow {
                    Wall { texture: CLOUD, vertices, additive: false, forced: false }
                } else {
                    Wall { texture: TUBE, vertices, additive: true, forced: false }
                });
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
        (got - want).abs() < 0.01
    }

    #[test]
    fn gospel_floats_a_cross_over_its_cell() {
        let start = Instant::now();
        let mut transcend = Transcend::default();
        transcend.keep_gospel(9, [10.0, 2.0, 20.0], start, |_| 0);
        // With turn₀ = 0 the height at n = 1, 90, 270 is 5.948, 3.0, 9.0; the radius at n = 18, 54, 72: 1.575, 1.425, 1.5.
        assert!(near(gospel_state(0.0, 1).0, 6.0 - 3.0 * 1.0_f32.to_radians().sin()) && near(gospel_state(0.0, 1).0, 5.948));
        assert!(near(gospel_state(0.0, 90).0, 3.0) && near(gospel_state(0.0, 270).0, 9.0));
        assert!(near(gospel_state(0.0, 18).1, 1.575) && near(gospel_state(0.0, 54).1, 1.425) && near(gospel_state(0.0, 72).1, 1.5));
        // Alpha 200 everywhere, 100 on the dim map; the corners are in the quad's order for uv (0, 0) top left.
        let crosses = transcend.crosses(start, false);
        assert_eq!(crosses.len(), 1);
        assert_eq!(crosses[0].alpha, 200.0);
        assert_eq!(transcend.crosses(start, true)[0].alpha, 100.0);
        let [bottom_left, bottom_right, top_right, top_left] = crosses[0].corners;
        assert!(bottom_left[0] < 0.0 && bottom_left[1] > 0.0 && top_right[0] > 0.0 && top_right[1] < 0.0);
        assert!(bottom_right[0] > 0.0 && bottom_right[1] > 0.0 && top_left[0] < 0.0 && top_left[1] < 0.0);
        // Side r√2: 2.12 units across at r = 1.5 (7 pixels a unit).
        assert!(near((top_right[0] - top_left[0]) / 7.0, 1.5 * std::f32::consts::SQRT_2));
        // Still there after 30 000 ticks while the unit stands; gone with it at once.
        assert_eq!(transcend.crosses(start + TICK * 30_000, false).len(), 1);
        transcend.forget(9);
        assert!(transcend.crosses(start, false).is_empty());
    }

    #[test]
    fn soul_drain_turns_the_target_blue_and_the_caster_flashes() {
        let start = Instant::now();
        let mut transcend = Transcend::default();
        transcend.drain_target(Some(1), start);
        transcend.drain_caster(Some(2), start);
        let tint = |e: u32| transcend.body_tint(Some(1), start + TICK * e);
        // (205 − e, 205 − e, 255): #CDCDFF at 0, #6969FF at 100, #0606FF at 199; fading back from 200.
        assert_eq!((tint(0), tint(100), tint(199)), (Some([205, 205, 255]), Some([105, 105, 255]), Some([6, 6, 255])));
        assert_eq!(tint(200), Some([14, 14, 255]));
        assert_eq!(tint(250), None);
        // The caster: additive #6464FF on ticks 55…65 only.
        let glow = |e: u32| transcend.glow(Some(2), start + TICK * e);
        assert_eq!(glow(55), Some((crate::actor::GLOW_ADDITIVE, Some([0x64, 0x64, 0xFF]))));
        assert!(glow(54).is_none() && glow(65).is_some() && glow(66).is_none());
    }

    #[test]
    fn magic_crasher_flickers_random_colours_and_indulge_glows_violet() {
        let start = Instant::now();
        let mut transcend = Transcend::default();
        // Three draws a tick: blue, green, red.
        let mut draws = (0u32..).map(|n| n % 255);
        transcend.crasher(Some(1), start, |_| draws.next().unwrap());
        let glow = |e: u32| transcend.glow(Some(1), start + TICK * e);
        assert_eq!(glow(30), Some((crate::actor::GLOW_ADDITIVE, Some([2, 1, 0]))));
        assert_eq!(glow(31), Some((crate::actor::GLOW_ADDITIVE, Some([5, 4, 3]))));
        assert!(glow(29).is_none() && glow(60).is_some() && glow(61).is_none());
        assert!(transcend.body_tint(Some(1), start + TICK * 61).is_some(), "the last colour fades back to white");
        // Indulge: outline copies in (2 (125 − e), 2 (125 − e), 250) on ticks 50…80.
        transcend.indulge(Some(2), start);
        let outline = |e: u32| transcend.glow(Some(2), start + TICK * e);
        assert_eq!(outline(50), Some((crate::actor::GLOW_OUTLINE, Some([150, 150, 250]))));
        assert_eq!(outline(80), Some((crate::actor::GLOW_OUTLINE, Some([90, 90, 250]))));
        assert!(outline(49).is_none() && outline(81).is_none());
    }

    #[test]
    fn soul_change_links_two_units_with_a_two_layer_tube() {
        let start = Instant::now();
        let mut transcend = Transcend::default();
        let feet = |unit: Option<u32>| match unit {
            None => Some([0.0, 0.0, 0.0]),
            Some(7) => Some([20.0, 0.0, 0.0]),
            _ => None,
        };
        transcend.link(None, 7, 1, start);
        // Alpha at n = 1, 19, 20, 40, 41, 77, 78: 6, 114, 114, 114, 111, 3, hidden.
        assert_eq!([1, 19, 20, 40, 41, 77, 78].map(tube_alpha), [6.0, 114.0, 114.0, 114.0, 111.0, 3.0, 0.0]);
        // The far end: 0.10 of the way at n = 1, 0.99 at 14, the target from 15.
        assert!((tube_growth(1) - 0.1045).abs() < 0.001 && (tube_growth(14) - 0.9945).abs() < 0.001 && tube_growth(15) == 1.0);
        // Two tubes of 10 quads: w 0.2 (#006496) and 0.4 (#FF3200); the outer alpha doubles (228 at n = 19).
        let walls = transcend.meshes(start + TICK * 18, feet);
        assert_eq!(walls.len(), 2);
        assert!(walls.iter().all(|wall| wall.texture == TUBE && wall.additive && wall.vertices.len() == 60));
        assert_eq!(walls[0].vertices[0].tint[..3], [0.0, 100.0 / 255.0, 150.0 / 255.0]);
        assert_eq!(walls[1].vertices[0].tint[..3], [1.0, 50.0 / 255.0, 0.0]);
        assert_eq!((walls[0].vertices[0].tint[3] * 255.0).round(), 114.0);
        assert_eq!((walls[1].vertices[0].tint[3] * 255.0).round(), 228.0);
        // Drawn for n = 1…77 (ticks 0…76).
        assert_eq!(transcend.meshes(start, feet).len(), 2);
        assert!(transcend.meshes(start + TICK * 76, feet).len() == 2 && transcend.meshes(start + TICK * 77, feet).is_empty());
        // The ids are read through a float: a player's id is exact, 16 777 217 is the unit 16 777 216, the player's own is None.
        assert_eq!(linked_unit(7, 1), Some(7));
        assert_eq!(linked_unit(1, 1), None);
        assert_eq!(linked_unit(16_777_217, 1), Some(16_777_216));
        // A target missing for one tick hides the tube for good.
        transcend.advance(start + TICK * 5, |unit| if unit == Some(7) { None } else { Some([0.0; 3]) });
        assert!(transcend.meshes(start + TICK * 6, feet).is_empty());
        transcend.advance(start + TICK * 7, feet);
        assert!(transcend.meshes(start + TICK * 7, feet).is_empty());
        transcend.advance(start + TICK * 201, feet);
        assert!(transcend.links.is_empty());
    }

    #[test]
    fn marionette_control_breathes_a_pink_tube_until_its_status_comes() {
        let start = Instant::now();
        let mut transcend = Transcend::default();
        let feet = |unit: Option<u32>| match unit {
            None => Some([0.0, 0.0, 0.0]),
            Some(7) => Some([20.0, 0.0, 0.0]),
            _ => None,
        };
        transcend.marionette(None, 7, 1, start);
        // Alpha at n = 1, 80, 81, 300: 1, 80, 80, 80; the width at n = 1, 180, 360: 0.70009, 1.273, 0.70.
        assert_eq!([1, 80, 81, 300].map(marionette_alpha), [1.0, 80.0, 80.0, 80.0]);
        assert!(
            (marionette_width(1) - 0.700_09).abs() < 0.0001 && (marionette_width(180) - 1.273).abs() < 0.005 && (marionette_width(360) - 0.7).abs() < 0.0001
        );
        // Both tubes #FF59B6, added on: the inner of the width and the alpha, the outer of twice each (alpha 2a at most 160).
        let walls = transcend.meshes(start + TICK * 99, feet);
        assert_eq!(walls.len(), 2);
        assert!(walls.iter().all(|wall| wall.additive && wall.vertices[0].tint[..3] == [1.0, 89.0 / 255.0, 182.0 / 255.0]));
        assert_eq!((walls[0].vertices[0].tint[3] * 255.0).round(), 80.0);
        assert_eq!((walls[1].vertices[0].tint[3] * 255.0).round(), 160.0);
        // It stays for good (well past Soul Change's 77 ticks), follows both units, and is hidden once an end is missing.
        assert_eq!(transcend.meshes(start + TICK * 5000, feet).len(), 2);
        transcend.advance(start + TICK * 5000, feet);
        assert_eq!(transcend.links.len(), 1);
        // The control's status packet takes it (EFST 121, on or off); other statuses and other units don't.
        transcend.forget_status(Some(5), 121);
        transcend.forget_status(None, 396);
        assert_eq!(transcend.links.len(), 1);
        transcend.forget_status(None, 121);
        assert!(transcend.links.is_empty());
        // Mind Breaker: a colour a tick on 0…60, from the draws blue, green, red; drawn as it is; faded back afterwards.
        let mut breaker = Transcend::default();
        let mut draws = [10, 20, 30].into_iter().cycle();
        breaker.breaker(Some(1), start, |_| draws.next().unwrap());
        let tint = |tick: u32| breaker.body_tint(Some(1), start + TICK * tick);
        assert_eq!((tint(0), tint(60)), (Some([30, 20, 10]), Some([30, 20, 10])));
        assert_eq!(tint(61), Some([38, 28, 18]));
        assert!(breaker.glow(Some(1), start).is_none(), "not added on");
    }

    #[test]
    fn shadow_form_pulses_a_dark_grey_tube_until_its_status_goes() {
        let start = Instant::now();
        let mut transcend = Transcend::default();
        let feet = |unit: Option<u32>| match unit {
            None => Some([0.0, 0.0, 0.0]),
            Some(7) => Some([20.0, 0.0, 0.0]),
            _ => None,
        };
        transcend.shadow(None, 7, 1, start);
        transcend.shadow(None, 7, 1, start);
        assert_eq!(transcend.links.len(), 1);
        // Alpha at n = 1, 4, 5, 9, 10, 14, 15: 47, 53, 51, 43, 45, 53, 51; the vertex alpha half of it, cut.
        assert_eq!([1, 4, 5, 9, 10, 14, 15].map(shadow_alpha), [47.0, 53.0, 51.0, 43.0, 45.0, 53.0, 51.0]);
        let alpha = |tick: u32| (transcend.meshes(start + TICK * tick, feet)[0].vertices[0].tint[3] * 255.0).round();
        assert_eq!([0, 3, 4, 8, 9, 13, 14].map(alpha), [23.0, 26.0, 25.0, 21.0, 22.0, 26.0, 25.0]);
        // Two tubes of 10 quads, #323232, blended, in the cloud texture: radius 0.4 and 0.8 round the line.
        let walls = transcend.meshes(start + TICK * 20, feet);
        assert_eq!(walls.len(), 2);
        assert!(walls.iter().all(|wall| wall.texture == CLOUD && !wall.additive && wall.vertices.len() == 60));
        assert_eq!(walls[0].vertices[0].tint[..3], [50.0 / 255.0; 3]);
        let radius = |wall: &Wall| wall.vertices.iter().map(|vertex| f32::hypot(vertex.position[1] - 8.0, vertex.position[2])).fold(0.0, f32::max);
        assert!((radius(&walls[0]) - 0.4).abs() < 0.01 && (radius(&walls[1]) - 0.8).abs() < 0.01);
        // It stays well past Soul Change's 77 ticks, until the status of Shadow Form (EFST 394) takes it.
        assert_eq!(transcend.meshes(start + TICK * 5000, feet).len(), 2);
        transcend.advance(start + TICK * 5000, feet);
        assert_eq!(transcend.links.len(), 1);
        transcend.forget_status(Some(5), 394);
        transcend.forget_status(None, 121);
        assert_eq!(transcend.links.len(), 1);
        transcend.forget_status(None, 394);
        assert!(transcend.links.is_empty());
        // Hidden for good once an end is missing for a tick.
        transcend.shadow(None, 99, 1, start);
        transcend.advance(start + TICK * 2, feet);
        assert!(transcend.meshes(start + TICK * 3, feet).is_empty());
    }

    #[test]
    fn soul_unity_links_the_target_to_the_caster_in_a_yellow_tube_with_a_blue_edge() {
        let start = Instant::now();
        let mut transcend = Transcend::default();
        // The target (unit 3) to the caster (unit 4, named by id as a float); the player is 9.
        transcend.unity(Some(3), 4, 9, start);
        let feet = |unit: Option<u32>| match unit {
            Some(3) => Some([0.0, 0.0, 0.0]),
            Some(4) => Some([100.0, 0.0, 0.0]),
            _ => None,
        };
        // Nothing before the first tick; at n = 1 the alpha is 47 and the far end sin(6°) of the way.
        let walls = transcend.meshes(start, feet);
        assert_eq!(walls.len(), 2);
        assert_eq!([1, 4, 5, 9, 10].map(shadow_alpha), [47.0, 53.0, 51.0, 43.0, 45.0]);
        assert!(walls.iter().all(|wall| wall.texture == CLOUD && wall.additive && wall.vertices.len() == 60));
        assert_eq!(walls[0].vertices[0].tint, [1.0, 1.0, 0.0, 47.0 / 255.0]);
        assert_eq!(walls[1].vertices[0].tint, [0.0, 0.0, 1.0, 94.0 / 255.0]);
        // The ring radii 0.4 and 0.8; the line grows 0.10, 0.99, 1.0 of its length at n = 1, 14, 15.
        assert!((walls[0].vertices[0].position[1] - 8.0).abs() <= 0.4 + 1e-4 && (walls[1].vertices[0].position[1] - 8.0).abs() <= 0.8 + 1e-4);
        assert_eq!([1, 14, 15].map(|n| (tube_growth(n) * 100.0).round() / 100.0), [0.10, 0.99, 1.0]);
        // Both ends follow their units (the player when the id is its own); the tube goes for good once an end is missing.
        let far = transcend.meshes(start + TICK * 20, |unit| feet(unit).map(|at| [at[0] + 50.0, at[1], at[2]]));
        assert!(far[0].vertices.iter().any(|vertex| vertex.position[0] > 140.0));
        assert!(transcend.meshes(start + TICK * 198, feet).len() == 2 && transcend.meshes(start + TICK * 199, feet).is_empty());
        transcend.advance(start + TICK * 5, |_| None);
        assert!(transcend.meshes(start + TICK * 6, feet).is_empty());
        // Not drawn at all when the caster is not found at the first tick.
        let mut missing = Transcend::default();
        missing.unity(Some(3), 7, 9, start);
        missing.advance(start, feet);
        assert!(missing.meshes(start + TICK * 2, feet).is_empty());
        // The effect ends at tick 200.
        transcend.advance(start + TICK * 201, feet);
        assert!(transcend.links.is_empty());
    }
}
