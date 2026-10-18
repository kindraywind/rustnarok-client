//! The max-level aura under a unit's feet. The client draws it itself from textured quads, not from effect files:
//! for level 99, two pulsing glows on the ground and three tilted ribbons turning round the body (EF_LEVEL99), and 16
//! motes rising from the feet (EF_LEVEL99_ORB1, or ORB2 for babies). Sizes are in world units (5 a cell).

use rustnarok_client::grf::Grf;

use crate::render::{Gpu, Texture};
use crate::world::{EffectMesh, EffectQuad, MeshVertex, Vector};

/// The client moves the aura on in steps of 16 ms; the rates below are per step.
const STEP_SECONDS: f32 = 0.016;
/// Sprite pixels to a world unit (35 a cell of 5).
const PIXELS: f32 = 7.0;
/// Not drawn farther than this from the player, in cells.
pub const SEEN_CELLS: f32 = 26.0;

pub struct Textures {
    glow: Option<Texture>,
    ribbon: Option<Texture>,
    mote: Option<Texture>,
    baby_mote: Option<Texture>,
}

impl Textures {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let load = |name: &str| gpu.try_load_texture(grf, &format!("data\\texture\\effect\\{name}"), true).ok();
        Self { glow: load("pikapika2.bmp"), ribbon: load("ring_blue.tga"), mote: load("whitelight.tga"), baby_mote: load("freezing_circle.bmp") }
    }
}

/// Whether a unit of sprite job `job` at `level` has the level-99 aura, and its motes' kind (`Some(true)`: a baby's).
/// The aura shows only at exactly the job group's top level: 99 for novices, 1st, 2nd and transcendent jobs, babies,
/// Super Novices, Taekwons, Star Gladiators, Soul Linkers, Gunslingers, Ninjas, mercenaries and homunculi.
// ponytail: the 3rd and 4th job groups (200 and 275) have auras of their own (EF_LEVEL185, EF_LEVEL4TH); not drawn yet.
pub fn level99(job: u16, level: u16) -> Option<bool> {
    let group = matches!(job, 0..=25 | 4001..=4049 | 6001..=6016 | 6017..=6046);
    (group && level == 99).then_some((4023..=4045).contains(&job))
}

/// Draws a unit's level-99 aura at `feet`, `seconds` into the map; `seed` scatters its motes. `full`: the glows and
/// ribbons too (`/aura` on), else the motes only.
pub fn draw<'a>(
    textures: &'a Textures,
    feet: Vector,
    baby: bool,
    seconds: f32,
    seed: u32,
    full: bool,
    meshes: &mut Vec<EffectMesh<'a>>,
    quads: &mut Vec<EffectQuad<'a>>,
) {
    let steps = seconds / STEP_SECONDS;
    let at = |x: f32, up: f32, z: f32| [feet[0] + x, feet[1] + up, feet[2] + z];
    let vertex = |position: Vector, uv: [f32; 2], tint: [f32; 4]| MeshVertex { position, uv, tint };
    if full && let Some(glow) = &textures.glow {
        // Two flat glows turned 0° and 22°, pulsing 9 to 10.8 units half across, half a pulse apart.
        let tint = [1.0, 1.0, 1.0, 200.0 / 255.0];
        let mut triangles = Vec::new();
        for (turn, phase) in [(0f32, 0f32), (22.0, 180.0)] {
            let size = 9.0 + 0.9 * ((phase + 3.0 * steps).to_radians().sin() + 1.0);
            let (sin, cos) = turn.to_radians().sin_cos();
            let corner = |x: f32, z: f32, uv: [f32; 2]| vertex(at(x * cos + z * sin, 0.2, -x * sin + z * cos), uv, tint);
            let [a, b, c, d] =
                [corner(-size, -size, [0.0, 0.0]), corner(size, -size, [1.0, 0.0]), corner(size, size, [1.0, 1.0]), corner(-size, size, [0.0, 1.0])];
            triangles.extend([a, b, c, a, c, d]);
        }
        meshes.push((glow, triangles, true));
    }
    if full && let Some(ribbon) = &textures.ribbon {
        // Three ribbons over 315° arcs (22 points 15° apart), rising and falling along the arc, leaning outward,
        // turning 3, 4 and 5° a step.
        let tint = [100.0 / 255.0, 100.0 / 255.0, 1.0, 120.0 / 255.0];
        let mut triangles = Vec::new();
        for i in 0..3 {
            let band = i as f32;
            let (radius, height, tilt) = (3.9 + 0.2 * band, 15.0 - 2.0 * band, (55.0 - 5.0 * band).to_radians());
            let start = 90.0 * band + (3.0 + band) * steps;
            let column = |s: usize| {
                let (sin, cos) = (start + 15.0 * s as f32).to_radians().sin_cos();
                let rise = (height * (9.0 * s as f32).to_radians().sin()).max(0.0);
                let out = radius + rise * tilt.cos();
                let u = s as f32 / 21.0;
                (vertex(at(sin * radius, 0.0, cos * radius), [u, 1.0], tint), vertex(at(sin * out, rise * tilt.sin(), cos * out), [u, 0.0], tint))
            };
            for s in 0..21 {
                let ((low, high), (next_low, next_high)) = (column(s), column(s + 1));
                triangles.extend([low, high, next_low, high, next_high, next_low]);
            }
        }
        meshes.push((ribbon, triangles, true));
    }
    let (texture, size, color) = if baby { (&textures.baby_mote, 0.55, [1.0, 1.0, 1.0]) } else { (&textures.mote, 1.8, [80.0 / 255.0, 80.0 / 255.0, 1.0]) };
    let Some(texture) = texture else { return };
    for mote in 0..16 {
        // Each mote's own wait (up to 100/0.15 steps), start and wander, from the unit and its number.
        let id = seed.wrapping_mul(16).wrapping_add(mote);
        let random = |salt: u32| (id.wrapping_mul(2_654_435_761).wrapping_add(salt.wrapping_mul(40_503)) >> 8) % 1000;
        let (rise, alpha, drift) = mote_at((random(1) % 100) as f32 / 0.15, steps + random(2) as f32, random(3) as f32);
        if alpha <= 0.0 {
            continue;
        }
        let half = size * PIXELS;
        let corners = [[-half, -half], [half, -half], [half, half], [-half, half]];
        quads.push((texture, at(drift[0], rise, drift[1]), corners, [color[0], color[1], color[2], alpha], true));
    }
}

/// A mote `steps` into its cycle: how high it is, how opaque (0: waiting, unseen), and how far it wandered. It waits
/// `wait` steps at the feet, then rises 0.15 a step, fully opaque to 20 units and fading 4/255 a step after, and
/// starts again above 30.
// ponytail: each mote keeps one wait and wanders on two sine waves; the client draws a new wait each time and steers
// its drift toward random targets.
fn mote_at(wait: f32, steps: f32, phase: f32) -> (f32, f32, [f32; 2]) {
    let t = steps % (wait + 30.0 / 0.15);
    if t < wait {
        return (0.0, 0.0, [0.0; 2]);
    }
    let rise = (t - wait) * 0.15;
    let alpha = if rise <= 20.0 { 1.0 } else { (255.0 - 4.0 * (rise - 20.0) / 0.15).max(0.0) / 255.0 };
    let drift = [2.0 * (t * 0.05 + phase).sin(), 2.0 * (t * 0.04 + phase * 1.7).sin()];
    (rise, alpha, drift)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shows_at_the_groups_top_level() {
        assert_eq!(level99(0, 99), Some(false), "a Novice at 99");
        assert_eq!(level99(4008, 99), Some(false), "a Lord Knight");
        assert_eq!(level99(4030, 99), Some(true), "a baby Knight: the baby's motes");
        assert_eq!([level99(7, 98), level99(4054, 99), level99(1002, 99)], [None; 3], "below 99, a 3rd job, a monster");
    }

    #[test]
    fn motes_wait_then_rise_and_fade() {
        // Waiting 100 steps: unseen. Then rising 0.15 a step, opaque to 20 units, fading after; again after 300.
        assert_eq!(mote_at(100.0, 50.0, 0.0).1, 0.0);
        let (rise, alpha, _) = mote_at(100.0, 200.0, 0.0);
        assert!((rise - 15.0).abs() < 1e-3 && alpha == 1.0);
        let (rise, alpha, _) = mote_at(100.0, 260.0, 0.0);
        assert!((rise - 24.0).abs() < 1e-3 && (alpha - (255.0 - 4.0 * 4.0 / 0.15) / 255.0).abs() < 1e-3);
        assert_eq!(mote_at(100.0, 350.0, 0.0).1, 0.0);
    }
}
