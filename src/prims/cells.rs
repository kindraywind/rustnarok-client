//! The sprites standing on skill units' cells (Wall of Thorns 912, Vacuum Extreme 921, the Ifrits of Demonic Fire and the
//! Insignias 916 to 918, Warmer's Imps 929) and Poison Buster's eight blue blasts (923), a 16 ms tick at a time.
//!
//! A shape's own age a counts from 0 on the tick it is made; n from 1 on its first tick after.

use std::collections::HashMap;
use std::time::Instant;

use super::crowds::{CrowdDraw, planted_alpha};
use super::passes;
use super::sight::Drawn;
use super::walls::Wall;
use crate::world::{MeshVertex, Vector};

pub const ANCIENT_TREE: &str = "data\\sprite\\몬스터\\Ancient_tree";
pub const BREEZE: &str = "data\\sprite\\몬스터\\breeze";
pub const IFRIT: &str = "data\\sprite\\몬스터\\ifrit";
pub const IMP: &str = "data\\sprite\\몬스터\\imp";
pub const BLAST: &str = "data\\sprite\\이팩트\\m_ef03";
/// A copy of the tree or the whirlwinds lasts 999 ticks.
const STAND_PERIOD: u32 = 999;
const IFRITS: u32 = 20;
/// Poison Buster's sprite has 24 frames, 4 ticks each; a blast flies 0.3 a tick from 5 out and 5 up.
const BLAST_FRAMES: u32 = 24;
const BLAST_TICKS: u32 = 4 * (BLAST_FRAMES - 1);
const BLAST_SPEED: f32 = 0.3;
/// The Sight option's flame (22): the effect lasts 270 ticks, a counter starts it again then; a pair of sprites every 2nd tick.
const SIGHT_LENGTH: u32 = 270;
const SIGHT_LIFE: u32 = 20;
const SIGHT_FRAMES: u32 = 25;
/// The counter of a unit that has just got the option: past the length, so the first tick starts the flame.
const COUNTER_NEW: u32 = 36_000;
/// The monsters' element changes (172 to 178): a ring 4 across leaning 89° whose rim rises to a hump 250 high, for 100 ticks.
const SHEET_LENGTH: u32 = 100;
const SHEET_RADIUS: f32 = 4.0;
const SHEET_LEAN: f32 = 89.0;
const SHEET_PEAK: f32 = 250.0;

/// One sprite on a cell: its ticks a frame, alpha, scale, action and colour.
struct Look {
    sprite: &'static str,
    per_frame: u32,
    alpha: f32,
    scale: f32,
    action: usize,
    tint: [f32; 3],
}

/// The tree or the whirlwinds on a skill unit's cell, this copy made at `start`.
struct Stand {
    id: u32,
    effect: u32,
    at: Vector,
    start: Instant,
    looks: Vec<Look>,
}

/// One of the Ifrits: the tick it came, where it stands, its scale and action.
struct Ifrit {
    born: u32,
    at: Vector,
    scale: f32,
    action: usize,
}

/// What tells the Ifrits' double ring from the Imps': the sprite, the two distances, the scales they start at and how much a draw
/// adds, the first action and the life; a copy lasts one tick less.
struct RingLook {
    sprite: &'static str,
    near: f32,
    far: f32,
    near_scale: f32,
    far_scale: f32,
    step: f32,
    action: usize,
    life: u32,
}

const IFRIT_RING: RingLook = RingLook { sprite: IFRIT, near: 6.0, far: 12.0, near_scale: 0.4, far_scale: 0.5, step: 0.03, action: 32, life: 2000 };
const IMP_RING: RingLook = RingLook { sprite: IMP, near: 4.0, far: 10.0, near_scale: 1.5, far_scale: 2.0, step: 0.05, action: 0, life: 9999 };

fn ring_look(effect: u32) -> &'static RingLook {
    if effect == 929 { &IMP_RING } else { &IFRIT_RING }
}

/// The double ring of Ifrits or Imps on a skill unit's cell.
struct Ring {
    id: u32,
    effect: u32,
    tint: Option<[f32; 3]>,
    at: Vector,
    start: Instant,
    made: u32,
    ifrits: Vec<Ifrit>,
}

struct Blasts {
    unit: Option<u32>,
    start: Instant,
}

/// A flame circling a unit, started at `start`; it follows the unit.
struct Circling {
    unit: Option<u32>,
    start: Instant,
}

/// A coloured flame sheet at a monster's feet.
struct Sheet {
    effect: u32,
    at: Vector,
    start: Instant,
}

#[derive(Default)]
pub struct Cells {
    circlings: Vec<Circling>,
    sheets: Vec<Sheet>,
    /// Per unit with the Sight option, the ticks since its flame started and the time they are counted to.
    sights: HashMap<Option<u32>, (u32, Instant)>,
    stands: Vec<Stand>,
    rings: Vec<Ring>,
    blasts: Vec<Blasts>,
    /// The effects whose next copy started since last asked (their sounds play again).
    restarts: Vec<u32>,
}

/// A sprite to draw: which, where, its scale, action, frame steps, alpha (0 to 255) and colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StandDraw {
    pub sprite: &'static str,
    pub at: Vector,
    pub scale: f32,
    pub action: usize,
    pub steps: u32,
    pub alpha: f32,
    pub tint: [f32; 3],
}

/// One of Poison Buster's blasts: where, and its frame (1 to 23).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlastDraw {
    pub at: Vector,
    pub frame: usize,
}

/// A blast layer's alpha in the body and in its halo, for the layer's own alpha: 0.85 and 0.5 of it, whole numbers.
pub fn blast_alphas(layer: f32) -> (f32, f32) {
    ((0.85 * layer).floor(), (0.5 * layer).floor())
}

const fn rgb(red: u8, green: u8, blue: u8) -> [f32; 3] {
    [red as f32 / 255.0, green as f32 / 255.0, blue as f32 / 255.0]
}

const GREEN: [f32; 3] = rgb(0x55, 0xFF, 0x55);
const GREY: [f32; 3] = rgb(0x9B, 0x9B, 0x9B);

/// The tint of an Insignia's Ifrits by its skill unit's type: Water, Wind and Earth; none for the rest.
/// The angle of a flame pair made at tick `e` of the effect: -5e degrees, plus 1 unless it comes to 0.
pub fn flame_angle(e: u32) -> f32 {
    match (-5 * e as i32).rem_euclid(360) {
        0 => 0.0,
        angle => (angle + 1) as f32,
    }
}

/// The texture of an element change's sheet (172 to 178).
pub fn sheet_texture(effect: u32) -> &'static str {
    match effect {
        172 => "data\\texture\\effect\\ring_black.tga",
        173 => "data\\texture\\effect\\ring_red.tga",
        174 => "data\\texture\\effect\\ring_blue.tga",
        175 => "data\\texture\\effect\\ring_yellow.tga",
        176 => "data\\texture\\effect\\ring_jadu.tga",
        177 => "data\\texture\\effect\\ring_brown.tga",
        179 => "data\\texture\\effect\\ring_purple.tga",
        _ => "data\\texture\\effect\\ring_white.tga",
    }
}

/// The height of a sheet's rim at point s (0 to 20) at its age n: a hump rising over 90 ticks.
pub fn sheet_profile(s: u32, n: u32) -> f32 {
    SHEET_PEAK * (9.0 * s as f32).to_radians().sin() * (n.min(90) as f32).to_radians().sin()
}

pub fn insignia_tint(kind: u32) -> Option<[f32; 3]> {
    match kind {
        242 => Some(rgb(0x14, 0x14, 0xB4)),
        243 => Some(rgb(0x14, 0xB4, 0x14)),
        244 => Some(rgb(0x28, 0x28, 0x28)),
        _ => None,
    }
}

fn looks_of(effect: u32, rand: &mut impl FnMut(u32) -> u32) -> Vec<Look> {
    if effect == 912 {
        // Draws in this order: ticks a frame, alpha, scale, action.
        let per_frame = 8 + rand(3);
        let alpha = (70 + rand(71)) as f32;
        let scale = 0.4 + 0.04 * rand(11) as f32;
        let action = rand(8) as usize;
        return vec![Look { sprite: ANCIENT_TREE, per_frame, alpha, scale, action, tint: GREEN }];
    }
    [(3, 0), (2, 4)].map(|(per_frame, action)| Look { sprite: BREEZE, per_frame, alpha: 205.0, scale: 2.5, action, tint: GREY }).into()
}

impl Cells {
    /// Wall of Thorns' tree (912) or Vacuum Extreme's whirlwinds (921) on the cell `at` of skill unit `id`, from 1 up.
    pub fn stand(&mut self, effect: u32, id: u32, at: Vector, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        let looks = looks_of(effect, &mut rand);
        self.stands.push(Stand { id, effect, at: [at[0], at[1] + 1.0, at[2]], start: now, looks });
    }

    /// The Ifrits (916 to 918) or Imps (929) of skill unit `id` on the cell `at`, tinted `tint`.
    pub fn ring(&mut self, effect: u32, id: u32, tint: Option<[f32; 3]>, at: Vector, now: Instant) {
        self.rings.push(Ring { id, effect, tint, at, start: now, made: 0, ifrits: Vec::new() });
    }

    /// The Sight flame (22) on `unit`, started now.
    pub fn sight(&mut self, unit: Option<u32>, now: Instant) {
        self.circlings.push(Circling { unit, start: now });
    }

    /// Stops the flames circling `unit` (Sight Rasher ends them).
    pub fn stop_sight(&mut self, unit: Option<u32>) {
        self.circlings.retain(|circling| circling.unit != unit);
    }

    /// The units with the Sight option now: each starts a flame the moment it has it and every 270 ticks while it lasts (one check a
    /// tick); one that is not listed or has lost it starts counting from nothing again.
    pub fn watch_sight(&mut self, units: impl IntoIterator<Item = (Option<u32>, bool)>, now: Instant) {
        let mut seen = Vec::new();
        for (unit, on) in units {
            if !on {
                continue;
            }
            seen.push(unit);
            let (counter, clock) = self.sights.entry(unit).or_insert((COUNTER_NEW, now - super::TICK));
            // A long gap (a hidden window) counts at most one period.
            let mut steps = 0;
            while *clock + super::TICK <= now && steps < SIGHT_LENGTH {
                *clock += super::TICK;
                steps += 1;
                if *counter >= SIGHT_LENGTH {
                    *counter = 0;
                    self.circlings.push(Circling { unit, start: *clock });
                }
                *counter += 1;
            }
            if *clock + super::TICK <= now {
                *clock = now;
            }
        }
        self.sights.retain(|unit, _| seen.contains(unit));
    }

    /// A flame sheet of element change `effect` at the monster's feet `at`.
    pub fn sheet(&mut self, effect: u32, at: Vector, now: Instant) {
        self.sheets.push(Sheet { effect, at, start: now });
    }

    /// Poison Buster's eight blasts round `unit`.
    pub fn poison_buster(&mut self, unit: Option<u32>, now: Instant) {
        self.blasts.push(Blasts { unit, start: now });
    }

    pub fn forget(&mut self, id: u32) {
        self.stands.retain(|stand| stand.id != id);
        self.rings.retain(|ring| ring.id != id);
    }

    /// The effects whose next copy started since this was last asked.
    pub fn take_restarts(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.restarts)
    }

    /// Starts the next copy of what has run its length, makes the Ifrits whose ticks have come (their height from `ground_at(x, z)`),
    /// the choices of each from `rand(n)` in the game's order, and drops what ended.
    pub fn advance(&mut self, now: Instant, ground_at: impl Fn(f32, f32) -> f32, mut rand: impl FnMut(u32) -> u32) {
        for stand in &mut self.stands {
            while passes(stand.start, now) > STAND_PERIOD {
                stand.start += super::TICK * STAND_PERIOD;
                stand.looks = looks_of(stand.effect, &mut rand);
                self.restarts.push(stand.effect);
            }
        }
        for ring in &mut self.rings {
            let look = ring_look(ring.effect);
            while passes(ring.start, now) > look.life - 1 {
                ring.start += super::TICK * (look.life - 1);
                (ring.made, ring.ifrits) = (0, Vec::new());
                self.restarts.push(ring.effect);
            }
            while ring.made < IFRITS && passes(ring.start, now) > ring.made {
                let e = ring.made;
                let scale = if e % 2 == 0 { look.near_scale } else { look.far_scale } + look.step * rand(11) as f32;
                let _ = rand(3);
                let action = look.action + rand(8) as usize;
                let distance = if e % 2 == 0 { look.near } else { look.far };
                let (sin, cos) = (18.0 * e as f32).to_radians().sin_cos();
                let (x, z) = (ring.at[0] + distance * sin, ring.at[2] + distance * cos);
                ring.ifrits.push(Ifrit { born: e, at: [x, ground_at(x, z), z], scale, action });
                ring.made += 1;
            }
        }
        self.blasts.retain(|blasts| passes(blasts.start, now) <= BLAST_TICKS + 3);
        self.circlings.retain(|circling| passes(circling.start, now) <= SIGHT_LENGTH);
        self.sheets.retain(|sheet| passes(sheet.start, now) <= SHEET_LENGTH);
    }

    /// The tree and the whirlwinds standing at `now`.
    pub fn stands(&self, now: Instant) -> Vec<StandDraw> {
        let mut draws = Vec::new();
        for stand in &self.stands {
            let Some(a) = passes(stand.start, now).checked_sub(1).filter(|&a| a < STAND_PERIOD) else { continue };
            for look in &stand.looks {
                let steps = a / look.per_frame + 1;
                draws.push(StandDraw { sprite: look.sprite, at: stand.at, scale: look.scale, action: look.action, steps, alpha: look.alpha, tint: look.tint });
            }
        }
        draws
    }

    /// The Ifrits standing at `now`, in the shape of the crowds' sprites.
    pub fn ifrits(&self, now: Instant) -> Vec<CrowdDraw> {
        let mut draws = Vec::new();
        for ring in &self.rings {
            let look = ring_look(ring.effect);
            let Some(e) = passes(ring.start, now).checked_sub(1).filter(|&e| e < look.life - 1) else { continue };
            for ifrit in ring.ifrits.iter().filter(|ifrit| ifrit.born <= e) {
                let n = e + 1 - ifrit.born;
                let alpha = planted_alpha(n, look.life - ifrit.born);
                if alpha > 0.0 {
                    let tint = ring.tint.unwrap_or([1.0; 3]);
                    draws.push(CrowdDraw { sprite: look.sprite, at: ifrit.at, scale: ifrit.scale, action: ifrit.action, steps: (n - 1) / 4, alpha, tint });
                }
            }
        }
        draws
    }

    /// The Sight flames and their shadows at `now` over their units' feet (`position_of`): on each even tick of the effect a pair
    /// turns up on a circle round the unit, shrinking and fading for 20 ticks.
    pub fn flames(&self, now: Instant, position_of: impl Fn(Option<u32>) -> Option<Vector>) -> Vec<Drawn> {
        let mut drawn = Vec::new();
        for circling in &self.circlings {
            let (Some(feet), Some(t)) = (position_of(circling.unit), passes(circling.start, now).checked_sub(1)) else { continue };
            for e in (0..SIGHT_LENGTH).step_by(2).take_while(|&e| e <= t) {
                let a = t - e;
                if a >= SIGHT_LIFE.min(SIGHT_LENGTH - e - 1) {
                    continue;
                }
                let (sin, cos) = flame_angle(e).to_radians().sin_cos();
                let (x, z) = (feet[0] + 15.0 * sin, feet[2] - 15.0 * cos);
                let a_f = a as f32;
                drawn.push(Drawn {
                    sprite: super::sight::SIGHT,
                    frame: ((a / 3 + 1) % SIGHT_FRAMES) as usize,
                    at: [x, feet[1] + 20.0, z],
                    scale: 2.5 - 0.1 * (a_f + 1.0),
                    alpha: 150.0 - 3.0 * (a_f + 1.0),
                });
                drawn.push(Drawn { sprite: super::sight::SHADOW, frame: 0, at: [x, feet[1], z], scale: 1.5 - 0.06 * (a_f + 1.0), alpha: 120.0 - (a_f + 1.0) });
            }
        }
        drawn
    }

    /// The sheets at `now`: a ring of 20 quads whose rim rises to a hump over 90 ticks, blended, 80 alpha, standing where it was made.
    pub fn meshes(&self, now: Instant) -> Vec<Wall> {
        let mut walls = Vec::new();
        for sheet in &self.sheets {
            let n = passes(sheet.start, now);
            if n == 0 || n >= SHEET_LENGTH {
                continue;
            }
            let (lean_sin, lean_cos) = SHEET_LEAN.to_radians().sin_cos();
            let tint = [1.0, 1.0, 1.0, 80.0 / 255.0];
            let corner = |s: u32, top: bool, uv: [f32; 2]| {
                let (sin, cos) = (18.0 * s as f32).to_radians().sin_cos();
                let (reach, up) = if top {
                    let rise = sheet_profile(s, n);
                    (SHEET_RADIUS + rise * lean_cos, rise * lean_sin)
                } else {
                    (SHEET_RADIUS, 0.0)
                };
                MeshVertex { position: [sheet.at[0] + reach * cos, sheet.at[1] + up, sheet.at[2] + reach * sin], uv, tint }
            };
            let mut vertices = Vec::with_capacity(20 * 6);
            for j in 0..20 {
                let (u0, u1) = (j as f32 / 21.0, (j + 1) as f32 / 21.0);
                let [a, b, c, d] = [corner(j, false, [u0, 1.0]), corner(j + 1, false, [u1, 1.0]), corner(j + 1, true, [u1, 0.0]), corner(j, true, [u0, 0.0])];
                vertices.extend([a, b, c, a, c, d]);
            }
            walls.push(Wall { texture: sheet_texture(sheet.effect), vertices, additive: false, forced: false });
        }
        walls
    }

    /// The frames Poison Buster's blasts show at `now` (1 to 23), for the caller to have ready.
    pub fn blast_frames(&self, now: Instant) -> Vec<usize> {
        let mut frames = Vec::new();
        for blasts in &self.blasts {
            let Some(t) = passes(blasts.start, now).checked_sub(1) else { continue };
            frames.extend((0..2).filter_map(|prim| t.checked_sub(prim).filter(|&a| a < BLAST_TICKS).map(|a| (a / 4 + 1) as usize)));
        }
        frames
    }

    /// Poison Buster's blasts at `now` round their units' feet (`position_of`): two prims a tick apart of four, 45° apart, flying out.
    pub fn blasts(&self, now: Instant, position_of: impl Fn(Option<u32>) -> Option<Vector>) -> Vec<BlastDraw> {
        let mut draws = Vec::new();
        for blasts in &self.blasts {
            let (Some(feet), Some(t)) = (position_of(blasts.unit), passes(blasts.start, now).checked_sub(1)) else { continue };
            for prim in 0..2 {
                let Some(a) = t.checked_sub(prim).filter(|&a| a < BLAST_TICKS) else { continue };
                let distance = 5.0 + BLAST_SPEED * (a + 1) as f32;
                for band in 0..4 {
                    let (sin, cos) = (45.0 * (4 * prim + band) as f32).to_radians().sin_cos();
                    draws.push(BlastDraw { at: [feet[0] + distance * cos, feet[1] + 5.0, feet[2] + distance * sin], frame: (a / 4 + 1) as usize });
                }
            }
        }
        draws
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prims::TICK;

    fn near(got: f32, want: f32) -> bool {
        (got - want).abs() < 1e-3
    }

    #[test]
    fn wall_of_thorns_stands_a_faint_green_tree_on_the_cell() {
        let start = Instant::now();
        let at = [10.0, 2.0, 20.0];
        let mut cells = Cells::default();
        let mut draws = Vec::new();
        cells.stand(912, 7, at, start, |n| {
            draws.push(n);
            0
        });
        // Ticks a frame, alpha, scale, action: R(3), R(71), R(11), R(8).
        assert_eq!(draws, [3, 71, 11, 8]);
        let tree = cells.stands(start)[0];
        assert_eq!((tree.sprite, tree.scale, tree.action, tree.alpha, tree.tint), (ANCIENT_TREE, 0.4, 0, 70.0, GREEN));
        assert_eq!(tree.at, [10.0, 3.0, 20.0], "1 up");
        // The largest draws: ticks 10, alpha 140, scale 0.8, action 7.
        let mut high = Cells::default();
        high.stand(912, 8, at, start, |n| n - 1);
        let tree = high.stands(start)[0];
        assert!(near(tree.scale, 0.8) && tree.action == 7 && tree.alpha == 140.0);
        assert_eq!(high.stands[0].looks[0].per_frame, 10);
        // The first drawn frame is 1; it goes up every 8 ticks; the alpha never changes.
        let steps = |cells: &Cells, tick: u32| cells.stands(start + TICK * tick)[0].steps;
        assert_eq!([0, 7, 8, 16].map(|tick| steps(&cells, tick)), [1, 1, 2, 3]);
        assert!(cells.stands(start + TICK * 500)[0].alpha == 70.0);
        // Drawn on ticks 0 to 998; a new copy, with new draws, at the 999th.
        assert!(!cells.stands(start + TICK * 998).is_empty() && cells.stands(start + TICK * 999).is_empty());
        let mut again = 0;
        cells.advance(
            start + TICK * 999,
            |_, _| 0.0,
            |_| {
                again += 1;
                0
            },
        );
        assert_eq!(again, 4);
        assert_eq!(cells.take_restarts(), [912]);
        assert_eq!(cells.stands(start + TICK * 999).len(), 1);
        assert_eq!(steps(&cells, 999), 1);
        // It goes with its unit.
        cells.forget(7);
        assert!(cells.stands(start + TICK * 999).is_empty());
    }

    #[test]
    fn vacuum_extreme_stands_two_grey_whirlwinds() {
        let start = Instant::now();
        let mut cells = Cells::default();
        cells.stand(921, 1, [0.0; 3], start, |_| panic!("no draws"));
        let draws = cells.stands(start + TICK * 6);
        assert_eq!(draws.len(), 2);
        assert_eq!([draws[0].steps, draws[1].steps], [3, 4], "3 and 2 ticks a frame");
        assert_eq!([draws[0].action, draws[1].action], [0, 4]);
        assert!(draws.iter().all(|draw| draw.sprite == BREEZE && draw.alpha == 205.0 && draw.scale == 2.5 && draw.tint == GREY && draw.at[1] == 1.0));
        assert!(!cells.stands(start + TICK * 998).is_empty() && cells.stands(start + TICK * 999).is_empty());
    }

    #[test]
    fn demonic_fire_rings_a_cell_with_twenty_ifrits() {
        let start = Instant::now();
        let at = [10.0, 0.0, 10.0];
        let mut cells = Cells::default();
        cells.ring(916, 3, None, at, start);
        let mut draws = Vec::new();
        for tick in 0..=21 {
            cells.advance(
                start + TICK * tick,
                |_, _| 2.0,
                |n| {
                    if tick == 1 {
                        draws.push(n);
                    }
                    0
                },
            );
        }
        // One a tick on ticks 0 to 19, none on 20: scale R(11), R(3) unused, action R(8).
        assert_eq!(cells.rings[0].ifrits.len(), 20);
        assert_eq!(draws, [11, 3, 8]);
        // Every draw 0: angle 18e, distance 6 on even ticks and 12 on odd, scale 0.4 and 0.5, action 32.
        let first = &cells.rings[0].ifrits[0];
        assert_eq!((first.at[0], first.at[1], first.at[2]), (10.0, 2.0, 16.0));
        assert!(near(first.scale, 0.4) && first.action == 32);
        let second = &cells.rings[0].ifrits[1];
        assert!(near(second.at[0], 10.0 + 12.0 * 18.0_f32.to_radians().sin()) && near(second.scale, 0.5));
        // The largest draws: 0.7, 0.8 and action 39.
        let mut high = Cells::default();
        high.ring(916, 4, None, at, start);
        high.advance(start + TICK, |_, _| 0.0, |n| n - 1);
        assert!(near(high.rings[0].ifrits[0].scale, 0.7) && high.rings[0].ifrits[0].action == 39 && near(high.rings[0].ifrits[1].scale, 0.8));
        // Alpha 5 a tick to 150 by n = 30, then all twenty fade out together over the last 50 ticks of their lives.
        let alphas = |tick: u32| cells.ifrits(start + TICK * tick).iter().map(|draw| draw.alpha).collect::<Vec<_>>();
        assert_eq!(alphas(0), [5.0]);
        assert_eq!(alphas(19).len(), 20);
        assert!(alphas(100).iter().all(|&alpha| alpha == 150.0));
        assert!(alphas(1949).iter().all(|&alpha| alpha == 150.0) && alphas(1951)[0] < 150.0);
        assert!(alphas(1980).iter().all(|&alpha| alpha == 0.0) || alphas(1980).is_empty());
        // The tints by unit type, none for the rest; the colour goes to the sprite.
        assert_eq!([231, 232, 233, 241].map(insignia_tint), [None; 4]);
        assert_eq!(insignia_tint(242), Some([20.0 / 255.0, 20.0 / 255.0, 180.0 / 255.0]));
        assert_eq!(insignia_tint(243), Some([20.0 / 255.0, 180.0 / 255.0, 20.0 / 255.0]));
        assert_eq!(insignia_tint(244), Some([40.0 / 255.0; 3]));
        let mut water = Cells::default();
        water.ring(916, 5, insignia_tint(242), at, start);
        water.advance(start, |_, _| 0.0, |_| 0);
        assert_eq!(water.ifrits(start)[0].tint, [20.0 / 255.0, 20.0 / 255.0, 180.0 / 255.0]);
        // A new copy at the 1999th tick, its sound again.
        assert!(!cells.ifrits(start + TICK * 1998).is_empty() || alphas(1998).is_empty());
        cells.advance(start + TICK * 1999, |_, _| 0.0, |_| 0);
        assert_eq!(cells.take_restarts(), [916]);
        assert_eq!(cells.rings[0].ifrits.len(), 1);
    }

    #[test]
    fn poison_buster_throws_eight_blue_blasts_outward() {
        let start = Instant::now();
        let feet = [10.0, 1.0, 20.0];
        let mut cells = Cells::default();
        cells.poison_buster(Some(1), start);
        let draws = |cells: &Cells, tick: u32| cells.blasts(start + TICK * tick, |_| Some(feet));
        // One prim at tick 0, the second a tick behind: four blasts, then eight.
        assert_eq!((draws(&cells, 0).len(), draws(&cells, 1).len()), (4, 8));
        // Headings 0, 45 … 315; 5.3 out and 5 up at a = 0.
        let first = draws(&cells, 0);
        assert!(first.iter().all(|blast| blast.at[1] == 6.0 && blast.frame == 1));
        assert!(near(first[0].at[0], feet[0] + 5.3) && near(first[1].at[2], feet[2] + 5.3 * 45.0_f32.to_radians().sin()));
        let both = draws(&cells, 1);
        assert!(near(both[4].at[0], feet[0] - 5.3) && near(both[4].at[2], feet[2]));
        // 32.6 out after a = 91; the frames 1 at a = 0 and 3, 2 at 4, 23 at 91.
        let last = draws(&cells, 91);
        assert!(near((last[0].at[0] - feet[0]).hypot(last[0].at[2] - feet[2]), 32.6) && last[0].frame == 23);
        assert_eq!([0, 3, 4].map(|tick| draws(&cells, tick)[0].frame), [1, 1, 2]);
        // The first prim ends after a = 91 (92 draws), the second a tick later.
        assert_eq!((draws(&cells, 92).len(), draws(&cells, 93).len()), (4, 0));
        assert_eq!(cells.blast_frames(start + TICK * 4), [2, 1]);
        // It follows the target.
        assert!(cells.blasts(start, |_| None).is_empty());
        assert!(cells.blasts(start, |_| Some([100.0, 1.0, 100.0])).iter().all(|blast| blast.at[0] > 90.0));
        // Each layer: #3737FF at 0.85 of its alpha and a halo at 0.5 (216 and 127 for 255).
        assert_eq!(blast_alphas(255.0), (216.0, 127.0));
        cells.advance(start + TICK * 100, |_, _| 0.0, |_| 0);
        assert!(cells.blasts.is_empty());
    }

    #[test]
    fn warmer_rings_a_cell_with_twenty_large_imps() {
        let start = Instant::now();
        let at = [10.0, 0.0, 10.0];
        let mut cells = Cells::default();
        cells.ring(929, 3, None, at, start);
        let mut draws = Vec::new();
        cells.advance(
            start,
            |_, _| 2.0,
            |n| {
                draws.push(n);
                0
            },
        );
        assert_eq!(draws, [11, 3, 8]);
        for tick in 1..=21 {
            cells.advance(start + TICK * tick, |_, _| 2.0, |_| 0);
        }
        assert_eq!(cells.rings[0].ifrits.len(), 20);
        // Every draw 0: angle 18e, distance 4 on even ticks and 10 on odd, scale 1.5 and 2.0, action 0.
        let (first, second) = (&cells.rings[0].ifrits[0], &cells.rings[0].ifrits[1]);
        assert!(near(first.at[2], 14.0) && near(first.scale, 1.5) && first.action == 0);
        assert!(near((second.at[0] - 10.0).hypot(second.at[2] - 10.0), 10.0) && near(second.scale, 2.0));
        // The largest draws: 2.0, 2.5 and action 7.
        let mut high = Cells::default();
        high.ring(929, 4, None, at, start);
        high.advance(start + TICK, |_, _| 0.0, |n| n - 1);
        assert!(near(high.rings[0].ifrits[0].scale, 2.0) && near(high.rings[0].ifrits[1].scale, 2.5) && high.rings[0].ifrits[0].action == 7);
        // The Imp sprite, ticks a frame 4, no tint; fading together near tick 9950 to 9980, a new copy at the 9998th tick.
        let draws = cells.ifrits(start + TICK * 19);
        assert_eq!(draws.len(), 20);
        assert!(draws.iter().all(|draw| draw.sprite == IMP && draw.tint == [1.0; 3]));
        assert_eq!(cells.ifrits(start + TICK * 100)[0].steps, 25);
        assert!(cells.ifrits(start + TICK * 9948).iter().all(|draw| draw.alpha == 150.0) && cells.ifrits(start + TICK * 9951)[0].alpha < 150.0);
        assert!(cells.ifrits(start + TICK * 9981).is_empty());
        cells.advance(start + TICK * 9998, |_, _| 0.0, |_| 0);
        assert_eq!(cells.take_restarts(), [929]);
        assert_eq!(cells.rings[0].ifrits.len(), 1);
    }

    #[test]
    fn the_sight_option_keeps_a_flame_circling_its_unit() {
        let start = Instant::now();
        let mut cells = Cells::default();
        // The flame starts the moment the option shows, again every 270 ticks, and never without it.
        cells.watch_sight([(Some(1), false)], start);
        assert!(cells.circlings.is_empty());
        cells.watch_sight([(Some(1), true)], start);
        assert_eq!(cells.circlings.len(), 1);
        cells.watch_sight([(Some(1), true)], start + TICK * 100);
        assert_eq!(cells.circlings.len(), 1);
        cells.watch_sight([(Some(1), true)], start + TICK * 269);
        assert_eq!(cells.circlings.len(), 1, "the counter is at 269");
        cells.watch_sight([(Some(1), true)], start + TICK * 270);
        assert_eq!(cells.circlings.len(), 2, "again 270 ticks on");
        // Losing the option starts the count over; the flame already running plays out.
        cells.watch_sight([(Some(1), false)], start + TICK * 300);
        cells.watch_sight([(Some(1), true)], start + TICK * 301);
        assert_eq!(cells.circlings.len(), 3);
        // Sight Rasher ends a unit's flames.
        cells.stop_sight(Some(1));
        assert!(cells.circlings.is_empty());
        // The angle of a pair made at e = 0, 2, 4, 72, 142: 0, 351, 341, 0, 11.
        assert_eq!([0, 2, 4, 72, 142].map(flame_angle), [0.0, 351.0, 341.0, 0.0, 11.0]);
    }

    #[test]
    fn two_sprites_on_every_even_tick_circle_and_shrink() {
        let start = Instant::now();
        let feet = [10.0, 1.0, 20.0];
        let mut cells = Cells::default();
        cells.sight(Some(1), start);
        let at = |tick: u32| cells.flames(start + TICK * tick, |_| Some(feet));
        // e = 0: the flame (20 up, 15 behind) and its shadow (on the ground); scale 2.4 and 1.44, alpha 147 and 119.
        let first = at(0);
        assert_eq!(first.len(), 2);
        assert_eq!((first[0].sprite, first[1].sprite), (crate::prims::sight::SIGHT, crate::prims::sight::SHADOW));
        assert!(near(first[0].at[0], 10.0) && near(first[0].at[1], 21.0) && near(first[0].at[2], 5.0) && near(first[1].at[1], 1.0));
        assert!(near(first[0].scale, 2.4) && near(first[0].alpha, 147.0) && near(first[1].scale, 1.44) && near(first[1].alpha, 119.0));
        // Pairs only on even ticks: 2 at tick 0, 2 at 1 (the first aged), 4 at tick 2.
        assert_eq!((at(1).len(), at(2).len()), (2, 4));
        // At a = 19: scale 0.5 and 0.30, alpha 90 and 100; the frame 1, 1, 2, 7 for a = 0, 2, 3, 19 and 0 for the shadow.
        let old = at(19);
        assert!(near(old[0].scale, 0.5) && near(old[0].alpha, 90.0) && near(old[1].scale, 0.30) && near(old[1].alpha, 100.0));
        assert_eq!([0, 2, 3, 19].map(|tick| at(tick)[0].frame), [1, 1, 2, 7]);
        assert!(at(5).iter().skip(1).step_by(2).all(|shadow| shadow.frame == 0));
        // A pair lasts 20 ticks: 10 pairs are alive from tick 19 on.
        assert_eq!(at(40).len(), 2 * 10);
        // The prims made at e = 252 … 268 are cut by the end: e = 268 is drawn once (270 − 268 − 1).
        assert_eq!(at(268).len(), 2 * 10);
        assert!(at(269).is_empty(), "the last tick draws none of the cut pairs");
        // The flames follow the unit's feet.
        assert!(cells.flames(start, |_| Some([100.0, 0.0, 100.0]))[0].at[0] > 99.0);
        assert!(cells.flames(start, |_| None).is_empty());
        cells.advance(start + TICK * 270, |_, _| 0.0, |_| 0);
        assert!(cells.circlings.is_empty());
    }

    #[test]
    fn an_element_change_raises_a_tall_coloured_sheet_at_the_monsters_feet() {
        let start = Instant::now();
        let at = [10.0, 2.0, 20.0];
        let mut cells = Cells::default();
        cells.sheet(172, at, start);
        // The profile at n = 1, 45, 90, 91 for s = 10: 250 sin 1°, 250 sin 45°, 250, 250; zero at s = 0 and 20.
        assert!(near(sheet_profile(10, 1), 250.0 * 1.0_f32.to_radians().sin()) && near(sheet_profile(10, 45), 250.0 * 45.0_f32.to_radians().sin()));
        assert!(near(sheet_profile(10, 90), 250.0) && near(sheet_profile(10, 91), 250.0));
        assert!(sheet_profile(0, 50).abs() < 1e-3 && sheet_profile(20, 50).abs() < 1e-2);
        // One band of 20 quads, blended, alpha 80 at any age, radius 4, lean 89.
        let walls = cells.meshes(start);
        assert_eq!(walls.len(), 1);
        assert!(!walls[0].additive && walls[0].vertices.len() == 20 * 6 && walls[0].texture.ends_with("ring_black.tga"));
        assert!(walls[0].vertices.iter().all(|vertex| vertex.tint == [1.0, 1.0, 1.0, 80.0 / 255.0]));
        assert!(cells.meshes(start + TICK * 80)[0].vertices.iter().all(|vertex| vertex.tint[3] == 80.0 / 255.0));
        // The foot of quad 0 is 4 out at angle 0; the top at s = 10 is the highest, 250 sin 89° at n = 90.
        let feet = walls[0].vertices[0].position;
        assert!(near(feet[0], 14.0) && near(feet[1], 2.0) && near(feet[2], 20.0));
        let top = cells.meshes(start + TICK * 89)[0].vertices.iter().map(|vertex| vertex.position[1] - 2.0).fold(0.0, f32::max);
        assert!(near(top, 250.0 * 89.0_f32.to_radians().sin()));
        // The uv runs j / 21 across the ring, 1 at the foot and 0 at the top.
        assert_eq!([walls[0].vertices[0].uv, walls[0].vertices[1].uv, walls[0].vertices[2].uv], [[0.0, 1.0], [1.0 / 21.0, 1.0], [1.0 / 21.0, 0.0]]);
        // Drawn on 99 ticks, 0 to 98; it stays where the monster was made, and goes at 100.
        assert!(!cells.meshes(start + TICK * 98).is_empty() && cells.meshes(start + TICK * 99).is_empty());
        cells.advance(start + TICK * 100, |_, _| 0.0, |_| 0);
        assert!(cells.sheets.is_empty());
        // The textures by id.
        let names: Vec<&str> = (172..=178).map(|id| sheet_texture(id).rsplit('\\').next().unwrap()).collect();
        assert_eq!(names, ["ring_black.tga", "ring_red.tga", "ring_blue.tga", "ring_yellow.tga", "ring_jadu.tga", "ring_brown.tga", "ring_white.tga"]);
    }

    #[test]
    fn change_poison_raises_a_violet_sheet() {
        assert!(sheet_texture(179).ends_with("ring_purple.tga"));
        let start = Instant::now();
        let mut cells = Cells::default();
        cells.sheet(179, [0.0; 3], start);
        assert!(cells.meshes(start)[0].texture.ends_with("ring_purple.tga"));
    }
}
