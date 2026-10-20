//! The kicks' and Swoo's body changes on a unit (413, 415, 417, 420): squashed flat, a hop, a white flash and a smaller body, each
//! counted a 16 ms tick at a time and read by the unit's drawing. They are the unit's own look, so `/effect off` keeps them.
//!
//! e is the ticks since the effect started; f is the counter each change names.

use std::time::Instant;

use super::passes;
use crate::actor::{GLOW_WHITE, PARAM_SHIFT, Placed};

/// What changes a unit's body, and how long it lasts in ticks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    /// Down Kick: squashed flat from tick 17.
    Squash,
    /// Counter Kick and Flash Kick: a hop from tick 30.
    Hop,
    /// Swhoo: a white flash.
    Flash,
    /// Swoo: half size.
    Shrink,
}

impl Change {
    fn length(self) -> u32 {
        match self {
            Change::Squash => 50,
            Change::Hop | Change::Flash => 100,
            Change::Shrink => 99_999,
        }
    }
}

struct Active {
    unit: Option<u32>,
    change: Change,
    start: Instant,
}

#[derive(Default)]
pub struct Bodies {
    active: Vec<Active>,
}

/// The squash's scale of a layer at its counter f (0 to 32): 1.0, 0.8, 0.6, 0.4 for f = 0 to 3, then 0.40 less a draw of 0 to 5 hundredths.
pub fn squash_scale(f: u32, draw: u32) -> f32 {
    if f <= 3 { (100.0 - 20.0 * f as f32) / 100.0 } else { 0.40 - draw as f32 / 100.0 }
}

/// How far the hop lifts a layer at its counter f, as a share of its height.
pub fn hop_lift(f: u32) -> f32 {
    match f {
        0..=18 => 0.3 * (5.0 * f as f32).to_radians().sin(),
        19 | 20 => 0.3,
        21..=23 => 0.1 * (23 - f) as f32,
        _ => 0.0,
    }
}

/// The white copy's alpha (0 to 255) at the flash's counter f, none from 12.
pub fn flash_alpha(f: u32) -> Option<f32> {
    match f {
        0..=5 => Some(50.0 * f as f32),
        6..=8 => Some(255.0),
        9..=11 => Some(895.0 - 80.0 * f as f32),
        _ => None,
    }
}

/// The size factor of the shrink at its counter n (it stops at 45): 1.0 down to 0.5 along a cosine.
pub fn shrink_factor(n: u32) -> f32 {
    let angle = 4 * n.min(45) + 90;
    if angle < 270 { 0.25 * (1.0 + (angle as f32).to_radians().sin()) + 0.5 } else { 0.5 }
}

impl Bodies {
    /// `change` starts on `unit` (None = the player).
    pub fn start(&mut self, change: Change, unit: Option<u32>, now: Instant) {
        self.active.push(Active { unit, change, start: now });
    }

    pub fn forget(&mut self, unit: u32) {
        self.active.retain(|each| each.unit != Some(unit));
    }

    pub fn advance(&mut self, now: Instant) {
        self.active.retain(|each| passes(each.start, now) <= each.change.length());
    }

    /// The tick e of the oldest running `change` on `unit` (two copies act as one, the older's counter).
    fn tick(&self, unit: Option<u32>, change: Change, now: Instant) -> Option<u32> {
        let each = self.active.iter().filter(|each| each.unit == unit && each.change == change).min_by_key(|each| each.start)?;
        passes(each.start, now).checked_sub(1).filter(|&e| e < change.length())
    }

    /// The factor the unit's sprite is multiplied by now (Swoo's shrink), 1.0 with none.
    pub fn scale(&self, unit: Option<u32>, now: Instant) -> f32 {
        self.tick(unit, Change::Shrink, now).map_or(1.0, shrink_factor)
    }

    /// The body flags of a white flash on `unit` now: the copy's alpha is in the top byte.
    pub fn glow(&self, unit: Option<u32>, now: Instant) -> Option<(u32, Option<[u8; 3]>)> {
        let e = self.tick(unit, Change::Flash, now)?;
        let alpha = flash_alpha(e.min(14))?;
        Some((GLOW_WHITE | (alpha as u32) << PARAM_SHIFT, None))
    }

    /// Squashes or lifts the layers `placed` of `unit` (their `[x, y]` offsets and sizes in sprite pixels, y down): a squash moves each
    /// top edge down by its height's share, a hop moves both edges up. `rand(n)` gives a squashed layer's shudder.
    pub fn reshape(&self, unit: Option<u32>, now: Instant, placed: &mut [Placed], mut rand: impl FnMut(u32) -> u32) {
        if let Some(e) = self.tick(unit, Change::Squash, now).filter(|&e| e >= 17) {
            let f = e - 17;
            for (_, position, size, _) in placed.iter_mut() {
                let s = squash_scale(f, if f >= 4 { rand(6) } else { 0 });
                let drop = size[1] * (1.0 - s);
                position[1] += drop;
                size[1] -= drop;
            }
        }
        if let Some(e) = self.tick(unit, Change::Hop, now).filter(|&e| e >= 30) {
            let lift = hop_lift(e - 30);
            for (_, position, size, _) in placed.iter_mut() {
                position[1] -= size[1] * lift;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prims::TICK;

    fn near(got: f32, want: f32) -> bool {
        (got - want).abs() < 1e-3
    }

    fn layer() -> Vec<Placed> {
        vec![((0, 0, 0, [255; 4], 0), [-10.0, -80.0], [20.0, 80.0], 0.0)]
    }

    #[test]
    fn down_kick_squashes_the_target_from_tick_17() {
        let start = Instant::now();
        let mut bodies = Bodies::default();
        bodies.start(Change::Squash, Some(1), start);
        let at = |tick: u32, draw: u32| {
            let mut placed = layer();
            bodies.reshape(Some(1), start + TICK * tick, &mut placed, |_| draw);
            placed
        };
        // Nothing at e = 16; f = 0 at e = 17: the layer is whole; the scale 1.0, 0.8, 0.6, 0.4 for f = 0 to 3, 0.40 and 0.35 at f = 4.
        assert_eq!(at(16, 0), layer());
        assert_eq!(at(17, 0), layer());
        assert_eq!([0, 1, 2, 3].map(|f| squash_scale(f, 0)), [1.0, 0.8, 0.6, 0.4]);
        assert!(near(squash_scale(4, 0), 0.40) && near(squash_scale(4, 5), 0.35));
        // The top edge moves down by h (1 − s), the bottom stays: h = 80, s = 0.4 at f = 3.
        let squashed = at(20, 0);
        assert!(near(squashed[0].1[1], -80.0 + 48.0) && near(squashed[0].1[1] + squashed[0].2[1], 0.0));
        // A new draw for each layer each frame: the draw 5 squashes a little further.
        assert!(at(30, 5)[0].2[1] < at(30, 0)[0].2[1]);
        // Nothing at e = 50; another unit is not touched.
        assert_eq!(at(50, 0), layer());
        let mut other = layer();
        bodies.reshape(Some(2), start + TICK * 25, &mut other, |_| 0);
        assert_eq!(other, layer());
    }

    #[test]
    fn counter_kick_hops_the_target_from_tick_30() {
        let start = Instant::now();
        let mut bodies = Bodies::default();
        bodies.start(Change::Hop, None, start);
        let top = |tick: u32| {
            let mut placed = layer();
            bodies.reshape(None, start + TICK * tick, &mut placed, |_| 0);
            (placed[0].1[1], placed[0].2[1])
        };
        assert_eq!(top(29), (-80.0, 80.0));
        // k at f = 0, 9, 18, 19, 20, 21, 22, 23, 24: 0, 0.2121, 0.3, 0.3, 0.3, 0.2, 0.1, 0, 0; both edges move up by h k.
        let ks = [0, 9, 18, 19, 20, 21, 22, 23, 24].map(hop_lift);
        let want = [0.0, 0.2121, 0.3, 0.3, 0.3, 0.2, 0.1, 0.0, 0.0];
        assert!(ks.iter().zip(want).all(|(got, want)| (got - want).abs() < 1e-3), "{ks:?}");
        assert!(near(top(30 + 18).0, -80.0 - 24.0) && top(30 + 18).1 == 80.0);
        assert_eq!(top(30 + 24), (-80.0, 80.0));
        // The effect ends at tick 100.
        bodies.advance(start + TICK * 100);
        assert!(bodies.active.is_empty());
    }

    #[test]
    fn swhoo_flashes_white_and_swoo_shrinks() {
        let start = Instant::now();
        let mut bodies = Bodies::default();
        bodies.start(Change::Flash, Some(1), start);
        bodies.start(Change::Shrink, Some(1), start);
        // The counter f = e up to 14; the copy's alpha at f = 0, 5, 6, 8, 9, 10, 11 and none from 12.
        assert_eq!([0, 5, 6, 8, 9, 10, 11].map(|f| flash_alpha(f).unwrap()), [0.0, 250.0, 255.0, 255.0, 175.0, 95.0, 15.0]);
        assert_eq!(flash_alpha(12), None);
        let glow = |tick: u32| bodies.glow(Some(1), start + TICK * tick);
        assert_eq!(glow(5), Some((GLOW_WHITE | 250 << PARAM_SHIFT, None)));
        assert!(glow(12).is_none() && glow(14).is_none() && glow(99).is_none());
        assert!(bodies.glow(Some(2), start).is_none() && glow(100).is_none());
        // Swoo's factor at n = 0, 10, 45, 46: 1.0, 0.942, 0.5, 0.5; it stays until the unit goes.
        assert_eq!([0, 10, 45, 46].map(shrink_factor).map(|factor| (factor * 1000.0).round() / 1000.0), [1.0, 0.942, 0.5, 0.5]);
        assert!(near(bodies.scale(Some(1), start + TICK * 99), 0.5) && bodies.scale(Some(2), start) == 1.0);
        bodies.advance(start + TICK * 5000);
        assert_eq!(bodies.active.len(), 1);
        bodies.forget(1);
        assert!(bodies.active.is_empty());
    }

    #[test]
    fn two_copies_on_one_unit_use_the_older_ones_counter() {
        let start = Instant::now();
        let mut bodies = Bodies::default();
        bodies.start(Change::Shrink, None, start + TICK * 20);
        bodies.start(Change::Shrink, None, start);
        assert!(near(bodies.scale(None, start + TICK * 10), shrink_factor(10)));
    }
}
