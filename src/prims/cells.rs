//! The sprites standing on skill units' cells (Wall of Thorns 912, Vacuum Extreme 921, the Ifrits of Demonic Fire and the
//! Insignias 916 to 918) and Poison Buster's eight blue blasts (923), a 16 ms tick at a time.
//!
//! A shape's own age a counts from 0 on the tick it is made; n from 1 on its first tick after.

use std::time::Instant;

use super::crowds::{CrowdDraw, planted_alpha};
use super::passes;
use crate::world::Vector;

pub const ANCIENT_TREE: &str = "data\\sprite\\몬스터\\Ancient_tree";
pub const BREEZE: &str = "data\\sprite\\몬스터\\breeze";
pub const IFRIT: &str = "data\\sprite\\몬스터\\ifrit";
pub const BLAST: &str = "data\\sprite\\이팩트\\m_ef03";
/// A copy of the tree or the whirlwinds lasts 999 ticks, one of the Ifrits 1 999.
const STAND_PERIOD: u32 = 999;
const IFRIT_PERIOD: u32 = 1999;
const IFRITS: u32 = 20;
/// Poison Buster's sprite has 24 frames, 4 ticks each; a blast flies 0.3 a tick from 5 out and 5 up.
const BLAST_FRAMES: u32 = 24;
const BLAST_TICKS: u32 = 4 * (BLAST_FRAMES - 1);
const BLAST_SPEED: f32 = 0.3;

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

/// The double ring of Ifrits on a skill unit's cell.
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

#[derive(Default)]
pub struct Cells {
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

    /// The Ifrits (916 to 918) of skill unit `id` on the cell `at`, tinted `tint`.
    pub fn ring(&mut self, effect: u32, id: u32, tint: Option<[f32; 3]>, at: Vector, now: Instant) {
        self.rings.push(Ring { id, effect, tint, at, start: now, made: 0, ifrits: Vec::new() });
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
            while passes(ring.start, now) > IFRIT_PERIOD {
                ring.start += super::TICK * IFRIT_PERIOD;
                (ring.made, ring.ifrits) = (0, Vec::new());
                self.restarts.push(ring.effect);
            }
            while ring.made < IFRITS && passes(ring.start, now) > ring.made {
                let e = ring.made;
                let scale = if e % 2 == 0 { 0.4 + 0.03 * rand(11) as f32 } else { 0.5 + 0.03 * rand(11) as f32 };
                let _ = rand(3);
                let action = 32 + rand(8) as usize;
                let distance = if e % 2 == 0 { 6.0 } else { 12.0 };
                let (sin, cos) = (18.0 * e as f32).to_radians().sin_cos();
                let (x, z) = (ring.at[0] + distance * sin, ring.at[2] + distance * cos);
                ring.ifrits.push(Ifrit { born: e, at: [x, ground_at(x, z), z], scale, action });
                ring.made += 1;
            }
        }
        self.blasts.retain(|blasts| passes(blasts.start, now) <= BLAST_TICKS + 3);
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
            let Some(e) = passes(ring.start, now).checked_sub(1).filter(|&e| e < IFRIT_PERIOD) else { continue };
            for ifrit in ring.ifrits.iter().filter(|ifrit| ifrit.born <= e) {
                let n = e + 1 - ifrit.born;
                let alpha = planted_alpha(n, IFRIT_PERIOD + 1 - ifrit.born);
                if alpha > 0.0 {
                    let tint = ring.tint.unwrap_or([1.0; 3]);
                    draws.push(CrowdDraw { sprite: IFRIT, at: ifrit.at, scale: ifrit.scale, action: ifrit.action, steps: (n - 1) / 4, alpha, tint });
                }
            }
        }
        draws
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
}
