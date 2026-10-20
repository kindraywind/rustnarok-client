//! Dead Hill Here's Geographers (853), Dance with Warg's Wargs (866), Poem of Netherworld's floating note (860) and Randomize Spell's
//! ten nested rune frames (862), a 16 ms tick at a time.
//!
//! A shape's own age n counts from 1 at its first tick; e is the ticks since the effect started.

use std::time::Instant;

use super::passes;
use crate::render::Texture;
use crate::world::{EffectQuad, Vector};

pub const GEOGRAPHER: &str = "data\\sprite\\몬스터\\geographer";
pub const WARG: &str = "data\\sprite\\몬스터\\워그";
pub const MELODY: &str = "data\\texture\\effect\\melody_a.bmp";
pub const RUNES: &str = "data\\texture\\effect\\priest_spell.bmp";
const PIXELS_PER_UNIT: f32 = 7.0;
/// A crowd lasts 400 ticks; it plants one sprite a tick while e is under its count.
const CROWD_LIFE: u32 = 400;
/// The note lives 29 999 ticks, the frames 300.
const NOTE_LIFE: u32 = 29_999;
const RUNE_LIFE: u32 = 300;

/// Which crowd: Dead Hill Here's Geographers or Dance with Warg's Wargs.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Geographers,
    Wargs,
}

/// One sprite put down: the tick it came, where it stands, its scale, ticks a frame, action and life.
struct Planted {
    born: u32,
    at: Vector,
    per_frame: u32,
    action: usize,
}

struct Crowd {
    kind: Kind,
    unit: Option<u32>,
    start: Instant,
    made: u32,
    last: Option<Vector>,
    sprites: Vec<Planted>,
}

/// A sprite to draw: which, where, how big, its action and frame (looping) and its alpha (0 to 255).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CrowdDraw {
    pub sprite: &'static str,
    pub at: Vector,
    pub scale: f32,
    pub action: usize,
    pub steps: u32,
    pub alpha: f32,
}

struct Note {
    id: u32,
    at: Vector,
    start: Instant,
    bob: f32,
}

struct Runes {
    unit: Option<u32>,
    start: Instant,
}

#[derive(Default)]
pub struct Crowds {
    crowds: Vec<Crowd>,
    notes: Vec<Note>,
    runes: Vec<Runes>,
}

impl Kind {
    fn count(self) -> u32 {
        if self == Kind::Geographers { 50 } else { 10 }
    }

    pub fn sprite(self) -> &'static str {
        if self == Kind::Geographers { GEOGRAPHER } else { WARG }
    }

    fn scale(self) -> f32 {
        if self == Kind::Geographers { 0.7 } else { 0.6 }
    }
}

/// A planted sprite's alpha at its age n for a life of `life`: 5 a tick to 150, held, then 5 less a tick over the last 50.
pub fn planted_alpha(n: u32, life: u32) -> f32 {
    if n < 50 {
        (5 * n).min(150) as f32
    } else if n > life.saturating_sub(50) {
        (150 - 5 * (n - life.saturating_sub(50)) as i32).max(0) as f32
    } else {
        150.0
    }
}

/// The note's half-diagonal at its spin σ: 5.7 to 6.3.
pub fn note_half(sigma: f32) -> f32 {
    6.0 * sigma.to_radians().sin() * 0.05 + 6.0
}

/// The note's height over its cell at its bob phase τ: 3 to 9.
pub fn note_height(tau: f32) -> f32 {
    6.0 - 3.0 * tau.to_radians().sin()
}

/// Randomize Spell's frame levels V1 to V10 and radius after tick `n`.
pub fn rune_levels(n: u32) -> ([i32; 10], f32) {
    let mut levels = [0; 10];
    let mut radius = 21.0_f32;
    for tick in 1..=n {
        if tick < 20 {
            for (index, level) in levels.iter_mut().enumerate() {
                *level = (*level + 10).min(50 + 20 * (index as i32 + 1));
            }
        } else {
            for level in &mut levels[..9] {
                *level = (*level - 4).max(0);
            }
            if tick > 90 {
                levels[9] -= 5;
                if levels[9] > 0 {
                    radius *= 1.01;
                } else {
                    levels[9] = 0;
                }
            }
        }
        if tick < 50 {
            radius -= 0.2;
        }
    }
    (levels, radius)
}

impl Crowds {
    /// A crowd round `unit`, one sprite a tick.
    pub fn plant(&mut self, kind: Kind, unit: Option<u32>, now: Instant) {
        self.crowds.push(Crowd { kind, unit, start: now, made: 0, last: None, sprites: Vec::new() });
    }

    /// A note over the cell `at` of the skill unit `id`; its bob phase from `rand(n)`.
    pub fn note(&mut self, id: u32, at: Vector, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        self.notes.push(Note { id, at, start: now, bob: rand(360) as f32 });
    }

    pub fn forget(&mut self, id: u32) {
        self.notes.retain(|note| note.id != id);
    }

    /// Rune frames round `unit`.
    pub fn runes(&mut self, unit: Option<u32>, now: Instant) {
        self.runes.push(Runes { unit, start: now });
    }

    /// Plants the sprites whose ticks have come (round the target's feet then; their height from `ground_at(x, z)`), the choices of
    /// each from `rand(n)` in the order the game draws them, and drops what ended.
    pub fn advance(
        &mut self,
        now: Instant,
        position_of: impl Fn(Option<u32>) -> Option<Vector>,
        ground_at: impl Fn(f32, f32) -> f32,
        mut rand: impl FnMut(u32) -> u32,
    ) {
        for crowd in &mut self.crowds {
            crowd.last = position_of(crowd.unit).or(crowd.last);
            while crowd.made < crowd.kind.count() && passes(crowd.start, now) > crowd.made {
                if let Some(feet) = crowd.last {
                    let e = crowd.made;
                    let (theta, distance, per_frame, action) = match crowd.kind {
                        Kind::Geographers => {
                            let theta = rand(360) as f32;
                            let distance = (5 + rand(45)) as f32;
                            let _ = rand(3);
                            (theta, distance, 4, 0)
                        }
                        Kind::Wargs => {
                            let _ = rand(3);
                            let per_frame = 6 + rand(3);
                            let action = rand(8) as usize;
                            (36.0 * e as f32, 40.0, per_frame, action)
                        }
                    };
                    let (sin, cos) = theta.to_radians().sin_cos();
                    let (x, z) = (feet[0] + distance * sin, feet[2] + distance * cos);
                    crowd.sprites.push(Planted { born: e, at: [x, ground_at(x, z), z], per_frame, action });
                }
                crowd.made += 1;
            }
        }
        self.crowds.retain(|crowd| passes(crowd.start, now) <= CROWD_LIFE);
        self.notes.retain(|note| passes(note.start, now) <= NOTE_LIFE);
        self.runes.retain(|runes| passes(runes.start, now) <= RUNE_LIFE);
    }

    /// The sprites standing at `now`.
    pub fn draws(&self, now: Instant) -> Vec<CrowdDraw> {
        let mut draws = Vec::new();
        for crowd in &self.crowds {
            let Some(e) = passes(crowd.start, now).checked_sub(1).filter(|&e| e < CROWD_LIFE) else { continue };
            for planted in &crowd.sprites {
                let n = e + 1 - planted.born;
                let alpha = planted_alpha(n, CROWD_LIFE - planted.born);
                if alpha > 0.0 {
                    draws.push(CrowdDraw {
                        sprite: crowd.kind.sprite(),
                        at: planted.at,
                        scale: crowd.kind.scale(),
                        action: planted.action,
                        steps: (n - 1) / planted.per_frame,
                        alpha,
                    });
                }
            }
        }
        draws
    }

    /// The notes and rune frames at `now`, camera-facing, added on or blended; the notes shake afresh each frame from `rand(n)`.
    pub fn quads<'a>(
        &self,
        now: Instant,
        texture_of: impl Fn(&str) -> Option<&'a Texture>,
        position_of: &impl Fn(Option<u32>) -> Option<Vector>,
        rand: impl Fn(u32) -> u32,
    ) -> Vec<EffectQuad<'a>> {
        let mut quads = Vec::new();
        if let Some(texture) = texture_of(MELODY) {
            for note in &self.notes {
                let n = passes(note.start, now);
                if n == 0 {
                    continue;
                }
                let (sigma, tau) = ((5 * n % 360) as f32, (note.bob + n as f32) % 360.0);
                let half = note_half(sigma) * PIXELS_PER_UNIT;
                let c0 = tau;
                let c1 = (c0 + 85.0 + rand(16) as f32) % 360.0;
                let c2 = (c1 + 85.0 + rand(16) as f32) % 360.0;
                let c3 = (c2 + 85.0 + rand(16) as f32) % 360.0;
                let corners = [c0, c1, c2, c3].map(|angle| {
                    let (sin, cos) = angle.to_radians().sin_cos();
                    [half * cos, half * sin]
                });
                let centre = [note.at[0], note.at[1] + note_height(tau), note.at[2]];
                quads.push((texture, centre, corners, [1.0, 1.0, 1.0, 120.0 / 255.0], false, false));
                for offset in [[0.2, 0.0], [-0.2, 0.0], [0.0, 0.2], [0.0, -0.2]] {
                    let place = [centre[0] + offset[0], centre[1], centre[2] + offset[1]];
                    quads.push((texture, place, corners, [5.0 / 255.0, 5.0 / 255.0, 1.0, 60.0 / 255.0], true, false));
                }
            }
        }
        if let Some(texture) = texture_of(RUNES) {
            for runes in &self.runes {
                let (Some(feet), n) = (position_of(runes.unit), passes(runes.start, now)) else { continue };
                if n == 0 || n > RUNE_LIFE {
                    continue;
                }
                let (levels, radius) = rune_levels(n);
                let place = [feet[0], feet[1] + 40.0, feet[2]];
                for i in (1..=10).rev() {
                    let level = levels[i - 1];
                    if level <= 0 {
                        continue;
                    }
                    let reach = (3.0 * (10 - i) as f32 + radius) * PIXELS_PER_UNIT;
                    let corners = |scale: f32| {
                        [35.0_f32, 145.0, 215.0, 325.0].map(|angle| {
                            let (sin, cos) = angle.to_radians().sin_cos();
                            [reach * scale * cos, reach * scale * sin]
                        })
                    };
                    let alpha = level as f32 / 255.0;
                    quads.push((texture, place, corners(1.0), [1.0, 1.0, 1.0, alpha], true, false));
                    quads.push((texture, place, corners(1.1), [1.0, 5.0 / 255.0, 5.0 / 255.0, alpha], true, false));
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
    fn dead_hill_here_plants_50_geographers_one_a_tick() {
        let start = Instant::now();
        let mut crowds = Crowds::default();
        crowds.plant(Kind::Geographers, Some(1), start);
        let mut draws = 0;
        for tick in 0..60 {
            crowds.advance(
                start + TICK * tick,
                |_| Some([10.0, 1.0, 10.0]),
                |_, _| 2.0,
                |_| {
                    draws += 1;
                    0
                },
            );
            assert_eq!(crowds.crowds[0].sprites.len() as u32, (tick + 1).min(50), "tick {tick}");
        }
        // Three draws a sprite; every draw 0: θ 0, d 5 (z + 5), 4 ticks a frame, action 0, scale 0.7, on the ground.
        assert_eq!(draws, 150);
        let first = &crowds.crowds[0].sprites[0];
        assert_eq!((first.at, first.per_frame, first.action), ([10.0, 2.0, 15.0], 4, 0));
        assert_eq!(Kind::Geographers.scale(), 0.7);
        // Alpha at n = 1, 30, 31, 49, 50: 5, 150, 150, 150, 150; the first sprite (life 400) at n = 350, 351, 380: 150, 145, 0;
        // the last (made at e = 49, life 351) at n = 301, 302: 150, 145.
        assert_eq!([1, 30, 31, 49, 50].map(|n| planted_alpha(n, 400)), [5.0, 150.0, 150.0, 150.0, 150.0]);
        assert_eq!([350, 351, 380].map(|n| planted_alpha(n, 400)), [150.0, 145.0, 0.0]);
        assert_eq!([301, 302].map(|n| planted_alpha(n, 351)), [150.0, 145.0]);
        // A sprite steps a frame every 4 ticks; they stay put when the target moves; all gone at the end.
        let draws = crowds.draws(start + TICK * 59);
        assert_eq!(draws.len(), 50);
        assert_eq!((draws[0].steps, draws[0].action), (14, 0));
        crowds.advance(start + TICK * 60, |_| Some([90.0, 1.0, 90.0]), |_, _| 2.0, |_| 0);
        assert_eq!(crowds.crowds[0].sprites[0].at, [10.0, 2.0, 15.0]);
        assert!(crowds.draws(start + TICK * 400).is_empty());
        crowds.advance(start + TICK * 401, |_| None, |_, _| 0.0, |_| 0);
        assert!(crowds.crowds.is_empty());
    }

    #[test]
    fn dance_with_warg_stands_ten_wargs_in_a_ring() {
        let start = Instant::now();
        let mut crowds = Crowds::default();
        crowds.plant(Kind::Wargs, None, start);
        // Draws in order: an unused R(3), the ticks a frame R(3), the action R(8): here 0, 1, 5.
        let mut script = [0, 1, 5].into_iter().cycle();
        for tick in 0..20 {
            crowds.advance(start + TICK * tick, |_| Some([0.0, 0.0, 0.0]), |_, _| 0.0, |n| script.next().unwrap().min(n - 1));
        }
        let sprites = &crowds.crowds[0].sprites;
        assert_eq!(sprites.len(), 10);
        assert!(sprites.iter().all(|planted| planted.per_frame == 7 && planted.action == 5));
        // θ = 0, 36 … 324 at distance 40, scale 0.6.
        let angle = |planted: &Planted| planted.at[0].atan2(planted.at[2]).to_degrees().rem_euclid(360.0);
        assert!(near(angle(&sprites[0]), 0.0) && near(angle(&sprites[1]), 36.0) && near(angle(&sprites[9]), 324.0));
        assert!(sprites.iter().all(|planted| near(f32::hypot(planted.at[0], planted.at[2]), 40.0)));
        assert_eq!(Kind::Wargs.scale(), 0.6);
    }

    #[test]
    fn the_poem_of_netherworld_floats_a_shaking_note() {
        // ρ at σ = 90 and 270: 6.3 and 5.7; the height at τ = 0, 90, 270: 6, 3, 9.
        assert!(near(note_half(90.0), 6.3) && near(note_half(270.0), 5.7));
        assert!(near(note_height(0.0), 6.0) && near(note_height(90.0), 3.0) && near(note_height(270.0), 9.0));
        let start = Instant::now();
        let mut crowds = Crowds::default();
        crowds.note(7, [10.0, 1.0, 10.0], start, |_| 0);
        assert_eq!(crowds.notes.len(), 1);
        crowds.forget(7);
        assert!(crowds.notes.is_empty());
        crowds.note(7, [10.0, 1.0, 10.0], start, |_| 0);
        crowds.advance(start + TICK * 30_005, |_| None, |_, _| 0.0, |_| 0);
        assert!(crowds.notes.is_empty());
    }

    #[test]
    fn randomize_spell_shrinks_ten_nested_rune_frames_in() {
        // V1 at n = 7, 20, 37: 70, 66, 0; V10 at n = 19, 90, 91, 127, 128: 190, 190, 185, 5, 0; V9 at n = 67: 0.
        assert_eq!([7, 20, 37].map(|n| rune_levels(n).0[0]), [70, 66, 0]);
        assert_eq!([19, 90, 91, 127, 128].map(|n| rune_levels(n).0[9]), [190, 190, 185, 5, 0]);
        assert_eq!(rune_levels(67).0[8], 0);
        assert_eq!(rune_levels(66).0[8], 2);
        // r at n = 49, 90, 91: 11.2, 11.2, 11.31; R for i = 10 and 1 at n = 49: 11.2 and 38.2.
        assert!(near(rune_levels(49).1, 11.2) && near(rune_levels(90).1, 11.2) && near(rune_levels(91).1, 11.31));
        let radius = rune_levels(49).1;
        assert!(near(3.0 * 0.0 + radius, 11.2) && near(3.0 * 9.0 + radius, 38.2));
    }
}
