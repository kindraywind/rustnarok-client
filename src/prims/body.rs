//! The kicks' and Swoo's body changes on a unit (413, 415, 417, 420): squashed flat, a hop, a white flash and a smaller body, each
//! counted a 16 ms tick at a time and read by the unit's drawing. They are the unit's own look, so `/effect off` keeps them.
//!
//! e is the ticks since the effect started; f is the counter each change names.

use std::time::Instant;

use super::passes;
use crate::actor::{GLOW_BLUE, GLOW_DROP, GLOW_RED, GLOW_SOUL, GLOW_STACK, GLOW_WHITE, PARAM_SHIFT, Placed, copies_alpha};

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
    /// Soul Link: a blue double outline on a player, fading in over 3.2 s.
    Soul,
    /// Spirit Link's ka on a monster: a stack of white outlines that settles to a halo.
    Stack,
    /// High Jump and Lope: the character drops in from the sky, spinning.
    Drop,
    /// Kaupe and Kaite: a blue glow that rises quickly and fades over 2.4 s.
    BlueGlow,
    /// The Star Gladiator's warmth: a red glow that rises over 1.4 s and settles.
    RedGlow,
    /// Madness Canceller: the body blinks blue every 4th tick for 0.9 s.
    BlueBlink,
    /// Adjustment and Gatling Fever: the same in red.
    RedBlink,
    /// Misty Frost: Hovering's blue copies fading in over 1.4 s and held.
    MistyFrost,
    /// Comfort of the Sun, Moon and Stars: Soul Link's blue outline swelling for 3.2 s and ebbing for 3.2 s.
    Comfort,
}

impl Change {
    fn length(self) -> u32 {
        match self {
            Change::Squash => 50,
            Change::Hop | Change::Flash => 100,
            Change::Shrink => 99_999,
            Change::Soul | Change::Stack | Change::RedGlow | Change::MistyFrost => 99_999_999,
            Change::BlueGlow => 200,
            Change::BlueBlink | Change::RedBlink => 60,
            Change::Comfort => 400,
            Change::Drop => 35,
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

/// Soul Link's glow alpha at the tick e: 1 a tick to 200.
pub fn soul_alpha(e: u32) -> u32 {
    e.min(200)
}

/// The red glow's counter at the tick e: e / 8 up to 200, then 25.
pub fn red_counter(e: u32) -> u32 {
    if e <= 200 { e / 8 } else { 25 }
}

/// Misty Frost's counter at the tick e: e / 8 up to 150, then 18.
pub fn misty_counter(e: u32) -> u32 {
    if e <= 150 { e / 8 } else { 18 }
}

/// Spirit Link's counter at the tick e: 50 more than the tick up to 149, then 199.
pub fn stack_counter(e: u32) -> u32 {
    if e < 150 { e + 50 } else { 199 }
}

/// The landing's lift in screen pixels at its counter f for a camera `unit` pixels to a world unit: 500 units up, falling 20 a tick.
pub fn drop_lift(f: u32, unit: f32) -> f32 {
    (20.0 * (25.0 - f as f32)).max(0.0) * unit
}

/// The landing's alpha (0 to 255) at its counter f: invisible to 10, 17 at 11, opaque from 25.
pub fn drop_alpha(f: u32) -> f32 {
    (17.0 * (f as f32 - 10.0)).clamp(0.0, 255.0)
}

/// The landing's spin of every layer at its counter f: 180 − 180 cos(5g°) with g = f + 11 while f < 25, else upright.
pub fn drop_spin(f: u32) -> f32 {
    let g = f + 11;
    if f < 25 && g <= 36 { 180.0 - 180.0 * (5.0 * g as f32).to_radians().cos() } else { 0.0 }
}

impl Bodies {
    /// `change` starts on `unit` (None = the player).
    pub fn start(&mut self, change: Change, unit: Option<u32>, now: Instant) {
        // A new Soul Link or landing replaces the old one on the unit.
        if matches!(change, Change::Soul | Change::Drop | Change::RedGlow | Change::MistyFrost) {
            self.active.retain(|each| !(each.unit == unit && each.change == change));
        }
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

    /// The body flags of what changes `unit` now, one number in the top byte: the landing first, then a white flash, a Soul Link's
    /// glow and a monster's stack.
    // ponytail: only one counter fits the flags, so a flash during a Soul Link hides its glow for those ticks.
    pub fn glow(&self, unit: Option<u32>, now: Instant) -> Option<(u32, Option<[u8; 3]>)> {
        if let Some(e) = self.tick(unit, Change::Drop, now) {
            return Some((GLOW_DROP | e << PARAM_SHIFT, None));
        }
        if let Some(alpha) = self.tick(unit, Change::Flash, now).and_then(|e| flash_alpha(e.min(14))) {
            return Some((GLOW_WHITE | (alpha as u32) << PARAM_SHIFT, None));
        }
        if let Some(e) = self.tick(unit, Change::Soul, now) {
            return Some((GLOW_SOUL | soul_alpha(e) << PARAM_SHIFT, None));
        }
        if let Some(f) = self.tick(unit, Change::Comfort, now).and_then(super::soul::comfort_outline) {
            return Some((GLOW_SOUL | f << PARAM_SHIFT, None));
        }
        if let Some(e) = self.tick(unit, Change::Stack, now) {
            return Some((GLOW_STACK | stack_counter(e) << PARAM_SHIFT, None));
        }
        if let Some(e) = self.tick(unit, Change::BlueGlow, now).filter(|&e| e <= 150) {
            return Some((GLOW_BLUE | (copies_alpha(e / 3) as u32) << PARAM_SHIFT, None));
        }
        // The blinks: ticks 0, 4 … 56, the copies at f = 20 (alpha 160) for that tick only.
        for (change, flag) in [(Change::BlueBlink, GLOW_BLUE), (Change::RedBlink, GLOW_RED)] {
            if self.tick(unit, change, now).is_some_and(|e| e % 4 == 0 && e <= 56) {
                return Some((flag | (copies_alpha(20) as u32) << PARAM_SHIFT, None));
            }
        }
        if let Some(e) = self.tick(unit, Change::MistyFrost, now) {
            return Some((GLOW_BLUE | (copies_alpha(misty_counter(e)) as u32) << PARAM_SHIFT, None));
        }
        let e = self.tick(unit, Change::RedGlow, now)?;
        Some((GLOW_RED | (copies_alpha(red_counter(e)) as u32) << PARAM_SHIFT, None))
    }

    /// The warmth's red glow ends with its status.
    pub fn forget_status(&mut self, unit: Option<u32>, status: u16) {
        if matches!(status, 165..=167 | 232 | 660) {
            self.active.retain(|each| !(each.unit == unit && each.change == Change::RedGlow));
        }
        if status == 1141 {
            self.active.retain(|each| !(each.unit == unit && each.change == Change::MistyFrost));
        }
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

    #[test]
    fn soul_link_spirit_link_and_the_landing_have_their_counters() {
        let start = Instant::now();
        let mut bodies = Bodies::default();
        bodies.start(Change::Soul, Some(1), start);
        // f at e = 0, 1, 199, 200, 201: 0, 1, 199, 200, 200.
        assert_eq!([0, 1, 199, 200, 201].map(soul_alpha), [0, 1, 199, 200, 200]);
        let flags = |bodies: &Bodies, tick: u32| bodies.glow(Some(1), start + TICK * tick);
        assert_eq!(flags(&bodies, 100), Some((GLOW_SOUL | 100 << PARAM_SHIFT, None)));
        // A second Soul Link replaces the first.
        bodies.start(Change::Soul, Some(1), start + TICK * 50);
        assert_eq!(bodies.active.len(), 1);
        // Spirit Link's counter: 50 more than e up to 149, then 199.
        assert_eq!([0, 149, 150, 400].map(stack_counter), [50, 199, 199, 199]);
        let mut monster = Bodies::default();
        monster.start(Change::Stack, Some(2), start);
        assert_eq!(monster.glow(Some(2), start + TICK * 5), Some((GLOW_STACK | 55 << PARAM_SHIFT, None)));
        // The landing: lift L at f = 0, 24, 25 (20·25·400/D px, 20·400/D, 0) for a camera 400 away; alpha at f = 10, 11, 25.
        assert_eq!([0, 24, 25].map(|f| drop_lift(f, 1.0)), [500.0, 20.0, 0.0]);
        assert_eq!(drop_lift(0, 2.0), 1000.0);
        assert_eq!([10, 11, 25].map(drop_alpha), [0.0, 17.0, 255.0]);
        // The spin at g = 11, 18, 24, 35 (f = 0, 7, 13, 24) and none from e = 25: 76.8, 180, 270, 359.3, 0.
        let spins = [0, 7, 13, 24, 25].map(drop_spin);
        let want = [76.8, 180.0, 270.0, 359.3, 0.0];
        assert!(spins.iter().zip(want).all(|(got, want)| (got - want).abs() < 0.1), "{spins:?}");
        // A second launch replaces the first; the effect ends at tick 35.
        let mut landing = Bodies::default();
        landing.start(Change::Drop, None, start);
        landing.start(Change::Drop, None, start + TICK * 10);
        assert_eq!(landing.active.len(), 1);
        assert_eq!(landing.glow(None, start + TICK * 12), Some((GLOW_DROP | 2 << PARAM_SHIFT, None)));
        landing.advance(start + TICK * 46);
        assert!(landing.active.is_empty() && landing.glow(None, start + TICK * 46).is_none());
    }

    #[test]
    fn madness_canceller_and_adjustment_blink_every_fourth_tick() {
        let start = Instant::now();
        let mut bodies = Bodies::default();
        bodies.start(Change::BlueBlink, Some(1), start);
        bodies.start(Change::RedBlink, Some(2), start);
        let flags = |unit: u32, tick: u32| bodies.glow(Some(unit), start + TICK * tick);
        // On at e = 0, 4 … 56: 15 ticks of 60, the copies at alpha 160 in blue for 625 and red for 626.
        let lit: Vec<u32> = (0..60).filter(|&tick| flags(1, tick).is_some()).collect();
        assert_eq!(lit, (0..=56).step_by(4).collect::<Vec<_>>());
        assert_eq!(lit.len(), 15);
        assert_eq!(flags(1, 0), Some((GLOW_BLUE | 160 << PARAM_SHIFT, None)));
        assert_eq!(flags(2, 56), Some((GLOW_RED | 160 << PARAM_SHIFT, None)));
        assert!([1, 2, 3, 57, 59, 60].iter().all(|&tick| flags(1, tick).is_none() && flags(2, tick).is_none()));
        // Nothing for another unit, and the effect ends at tick 60.
        assert!(bodies.glow(Some(3), start).is_none());
        bodies.advance(start + TICK * 61);
        assert!(bodies.active.is_empty());
    }

    #[test]
    fn misty_frost_fades_its_blue_glow_in_over_1_4_seconds() {
        let start = Instant::now();
        let mut bodies = Bodies::default();
        bodies.start(Change::MistyFrost, Some(1), start);
        let glow = |bodies: &Bodies, tick: u32| bodies.glow(Some(1), start + TICK * tick).map(|(flags, _)| (flags & 0xFFFFFF, flags >> PARAM_SHIFT));
        // f = e / 8 up to 150 then 18; the alpha at e = 0, 7, 8, 80, 87, 88, 144, 150, 151, 1000.
        assert_eq!([0, 7, 8, 80, 87, 88, 144, 150, 151, 1000].map(misty_counter), [0, 0, 1, 10, 10, 11, 18, 18, 18, 18]);
        let alphas = [0, 7, 8, 80, 87, 88, 144, 150, 151, 1000].map(|e| glow(&bodies, e).unwrap());
        assert_eq!(alphas.map(|(_, alpha)| alpha), [0, 0, 15, 150, 150, 160, 160, 160, 160, 160]);
        assert!(alphas.iter().all(|&(flags, _)| flags == GLOW_BLUE));
        // A second status replaces the first; the status going off takes it away.
        bodies.start(Change::MistyFrost, Some(1), start + TICK * 10);
        assert_eq!(bodies.active.len(), 1);
        bodies.forget_status(Some(1), 1141);
        assert!(glow(&bodies, 20).is_none());
    }

    #[test]
    fn kaupe_glows_blue_and_the_warmth_glows_red() {
        let start = Instant::now();
        let mut bodies = Bodies::default();
        bodies.start(Change::BlueGlow, Some(1), start);
        bodies.start(Change::RedGlow, Some(2), start);
        let blue = |tick: u32| bodies.glow(Some(1), start + TICK * tick).map(|(flags, _)| (flags & 0xFFFFFF, flags >> PARAM_SHIFT));
        // f = e / 3; alpha at e = 0, 3, 30, 33, 60, 150: 0, 15, 150, 160, 160, 5; nothing from 151 (the effect runs to 200).
        assert_eq!([0, 3, 30, 33, 60, 150].map(|e| blue(e).unwrap().1), [0, 15, 150, 160, 160, 5]);
        assert_eq!(blue(0).unwrap().0, GLOW_BLUE);
        assert!(blue(151).is_none() && blue(200).is_none());
        // The warmth's counter f = e / 8 to 200, 25 after; alpha at f = 0, 10, 11, 20, 21, 25: 0, 150, 160, 160, 150, 130.
        assert_eq!([0, 8, 200, 201, 1000].map(red_counter), [0, 1, 25, 25, 25]);
        let red = |bodies: &Bodies, tick: u32| bodies.glow(Some(2), start + TICK * tick).map(|(flags, _)| (flags & 0xFFFFFF, flags >> PARAM_SHIFT));
        assert_eq!([0, 80, 88, 160, 168, 400].map(|e| red(&bodies, e).unwrap().1), [0, 150, 160, 160, 150, 130]);
        assert_eq!(red(&bodies, 10).unwrap().0, GLOW_RED);
        // It stays for as long as the status does, a new one replaces it, and the status going takes it away.
        assert!(red(&bodies, 100_000).is_some());
        bodies.start(Change::RedGlow, Some(2), start + TICK * 50);
        assert_eq!(bodies.active.iter().filter(|each| each.change == Change::RedGlow).count(), 1);
        bodies.forget_status(Some(2), 14);
        assert!(red(&bodies, 100).is_some());
        bodies.forget_status(Some(2), 166);
        assert!(bodies.glow(Some(2), start + TICK * 100).is_none());
    }
}
