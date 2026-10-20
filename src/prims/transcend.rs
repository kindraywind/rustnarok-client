//! The High Wizard's, Professor's and Priest's skills' code-made shapes, a 16 ms tick at a time: Gospel's bobbing cross (370), the
//! bodies Soul Drain, Magic Crasher and Indulge colour (378 to 383), and Soul Change's glowing tube between two units (384).
//!
//! A shape's own age n counts from 1 at its first tick; e is the ticks since the effect started.

use std::time::Instant;

use super::passes;
use super::walls::Wall;
use crate::world::{MeshVertex, Vector};

pub const CROSS: &str = "data\\texture\\effect\\cross_old.bmp";
pub const TUBE: &str = "data\\texture\\effect\\alpha_center.tga";

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
    Indulge,
}

struct Coloured {
    unit: Option<u32>,
    start: Instant,
    body: Body,
}

/// Soul Change's tube between the caster and the unit the packet names by id.
struct Link {
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
        self.links.push(Link { caster, aim: target, own, start: now, lost: false });
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
        self.links.retain(|link| passes(link.start, now) <= 200);
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
                Body::DrainTarget => None,
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
                _ => None,
            }
        })
    }

    /// Soul Change's tubes at `now`: an inner blue and an outer orange, 10 facets each; `position_of` gives a unit's feet.
    pub fn meshes(&self, now: Instant, position_of: impl Fn(Option<u32>) -> Option<Vector>) -> Vec<Wall> {
        let mut walls = Vec::new();
        for link in self.links.iter().filter(|link| !link.lost) {
            let n = passes(link.start, now);
            if !(1..=77).contains(&n) {
                continue;
            }
            let target = linked_unit(link.aim, link.own);
            let (Some(caster), Some(far)) = (position_of(link.caster), position_of(target)) else { continue };
            let (p0, p1) = ([caster[0], caster[1] + 8.0, caster[2]], [far[0], far[1] + 8.0, far[2]]);
            let grow = tube_growth(n);
            let end = [p0[0] + (p1[0] - p0[0]) * grow, p0[1] + (p1[1] - p0[1]) * grow, p0[2] + (p1[2] - p0[2]) * grow];
            let alpha = tube_alpha(n);
            // The bearing of (P0 − P1) in x/z turned a quarter: the facets are round the line, across it.
            let bearing = crate::actor::step_rot(p0[0] - p1[0], p0[2] - p1[2]);
            let (sin, cos) = ((bearing + 90.0) % 360.0).to_radians().sin_cos();
            for (width, rgb, vertex_alpha) in [(0.2, [0.0, 100.0 / 255.0, 150.0 / 255.0], alpha), (0.4, [1.0, 50.0 / 255.0, 0.0], (2.0 * alpha).min(255.0))] {
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
                walls.push(Wall { texture: TUBE, vertices, additive: true, forced: false });
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
}
