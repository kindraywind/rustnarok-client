//! Lullaby Deep Sleep's 96 wandering notes (969): twelve prims of four bands, each band two notes that drift out of the target in
//! three waves, a 16 ms tick at a time. Metallic Sound's 18 orange notes (1584) steer the same way inside ±45° of the way to the
//! target, and Reverberation's 24 pale blue ones (1594) are Lullaby's smaller.
//!
//! A band's own age n counts from 1 on the first tick after its prim is made; e is the ticks since the effect started.

use std::time::Instant;

use super::passes;
use super::tanji::bearing;
use crate::render::Texture;
use crate::world::{EffectQuad, Vector};

/// The two note pictures, under names of their own: they are loaded with magenta clear, which the Poem's note is not.
pub const TEXTURES: [(&str, &str); 2] =
    [("lullaby\\melody_a", "data\\texture\\effect\\melody_a.bmp"), ("lullaby\\melody_b", "data\\texture\\effect\\melody_b.bmp")];
pub const METAL_NOTES: [&str; 3] = [
    "data\\texture\\effect\\new_metalicsound\\new_metalicsound_icon\\met_particle_1.tga",
    "data\\texture\\effect\\new_metalicsound\\new_metalicsound_icon\\met_particle_2.tga",
    "data\\texture\\effect\\new_metalicsound\\new_metalicsound_icon\\met_particle_3.tga",
];
pub const METAL_NOTES_MIN: [&str; 3] = [
    "data\\texture\\effect\\mineffect\\new_metalicsound\\new_metalicsound_icon\\met_particle_1.tga",
    "data\\texture\\effect\\mineffect\\new_metalicsound\\new_metalicsound_icon\\met_particle_2.tga",
    "data\\texture\\effect\\mineffect\\new_metalicsound\\new_metalicsound_icon\\met_particle_3.tga",
];
pub const REVERB_NOTES: [&str; 2] = ["data\\texture\\effect\\new_reverberation\\rev_note_01.tga", "data\\texture\\effect\\new_reverberation\\rev_note_02.tga"];
pub const REVERB_NOTES_MIN: [&str; 2] =
    ["data\\texture\\effect\\mineffect\\new_reverberation\\rev_note_01.tga", "data\\texture\\effect\\mineffect\\new_reverberation\\rev_note_02.tga"];
const PIXELS_PER_UNIT: f32 = 7.0;
/// The waves come at ticks 0, 10 and 20; a band lasts to n = 146 (Metallic Sound's to 96) and the last one ends 20 ticks later.
const WAVES: [u32; 3] = [0, 10, 20];
const LAST_AGE: u32 = 146;
const METAL_LAST_AGE: u32 = 96;

/// Whose notes: Lullaby Deep Sleep's, Metallic Sound's or Reverberation's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Lullaby,
    Metal,
    Reverb,
}

impl Kind {
    /// The pictures of a wave's prims, in order, and the bands of each.
    fn wave(self, min: bool) -> (&'static [&'static str], usize) {
        const LULLABY: [&str; 4] = [TEXTURES[0].0, TEXTURES[1].0, TEXTURES[0].0, TEXTURES[1].0];
        match self {
            Kind::Lullaby => (&LULLABY, 4),
            Kind::Metal if min => (&METAL_NOTES_MIN, 1),
            Kind::Metal => (&METAL_NOTES, 1),
            Kind::Reverb if min => (&REVERB_NOTES_MIN, 4),
            Kind::Reverb => (&REVERB_NOTES, 4),
        }
    }

    fn last_age(self) -> u32 {
        if self == Kind::Metal { METAL_LAST_AGE } else { LAST_AGE }
    }
}

/// A square to draw: its picture's name, its place, its corners in screen pixels, its colour and whether it adds on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NoteDraw {
    pub texture: &'static str,
    pub at: Vector,
    pub corners: [[f32; 2]; 4],
    pub tint: [f32; 4],
    pub additive: bool,
}

/// One note: its yaw and the yaw it turns to, its pitch and the pitch it turns to (whole degrees), where it is from the prim's place
/// and its alpha.
struct Note {
    target: i32,
    yaw: i32,
    pitch_target: i32,
    pitch: i32,
    at: Vector,
    alpha: i32,
}

struct Band {
    speed: f32,
    size: f32,
    notes: [Note; 2],
    age: u32,
    active: bool,
    /// Metallic Sound's: the bearing to the target, which the headings keep within 45° of.
    aim: Option<i32>,
}

/// A prim: whose, which picture, where the unit stood, the tick it was made and the ticks run.
struct Prim {
    kind: Kind,
    texture: &'static str,
    at: Vector,
    start: Instant,
    done: u32,
    bands: Vec<Band>,
}

/// An effect making waves of prims where its unit stands (and aimed at `aim`, Metallic Sound's).
struct Source {
    kind: Kind,
    unit: Option<u32>,
    start: Instant,
    /// Where the unit last stood, and how many waves are made.
    last: Vector,
    waves: usize,
    aim: Option<Vector>,
    min: bool,
}

#[derive(Default)]
pub struct Notes {
    sources: Vec<Source>,
    prims: Vec<Prim>,
}

/// One degree-step toward a target: the new angle and whether it got there. Angles are in whole degrees, kept in 0 to 359.
pub fn steer(current: i32, target: i32, step: i32) -> (i32, bool) {
    let d = current - target;
    if (0..180).contains(&d) {
        let current = current - step;
        (current, current <= target)
    } else if d < -180 {
        let current = current - step;
        (current, current <= target - 360)
    } else {
        let current = current + step;
        (current, current >= target)
    }
}

/// A band's alpha at its age n: 25 a tick to 255, held to 60, then 3 less a tick (0 at 145).
#[cfg(test)]
pub fn note_alpha(n: u32) -> i32 {
    let mut alpha = 0;
    for each in 1..=n {
        if each < 50 {
            alpha = (alpha + 25).min(255);
        } else if each > 60 {
            alpha = (alpha - 3).max(0);
        }
    }
    alpha
}

impl Note {
    fn new(target: i32, yaw: i32) -> Self {
        Self { target, yaw, pitch_target: 180, pitch: 180, at: [0.0, 5.0, 0.0], alpha: 0 }
    }

    /// The heading and the pitch turn toward their targets, picking new ones on getting there (a heading within 45° of `aim`).
    fn turn(&mut self, aim: Option<i32>, rand: &mut impl FnMut(u32) -> u32) {
        let (yaw, hit) = steer(self.yaw, self.target, 2);
        self.yaw = yaw;
        if hit {
            self.target = aimed(aim, rand);
        }
        let (pitch, hit) = steer(self.pitch, self.pitch_target, 1);
        self.pitch = pitch;
        if hit {
            self.pitch_target = 175 + rand(11) as i32;
        }
        for angle in [&mut self.yaw, &mut self.target, &mut self.pitch, &mut self.pitch_target] {
            *angle = angle.rem_euclid(360);
        }
    }

    /// One step of `speed` along its heading and, up or down by `sign`, its pitch.
    fn drift(&mut self, speed: f32, sign: f32) {
        let (yaw_sin, yaw_cos) = (self.yaw as f32).to_radians().sin_cos();
        self.at[0] += speed * yaw_cos;
        self.at[2] += speed * yaw_sin;
        self.at[1] += sign * speed * (self.pitch as f32).to_radians().sin();
    }
}

/// A heading: any (`R(360)`), or within 45° of `aim` (`aim − 45 + R(91)`), in 0 to 359.
fn aimed(aim: Option<i32>, rand: &mut impl FnMut(u32) -> u32) -> i32 {
    match aim {
        Some(aim) => (aim - 45 + rand(91) as i32).rem_euclid(360),
        None => rand(360) as i32,
    }
}

impl Band {
    /// A band: its speed, size and the four angles (yaw target, yaw, for each note) from `rand(n)` in the game's order.
    fn new(kind: Kind, aim: Option<i32>, rand: &mut impl FnMut(u32) -> u32) -> Self {
        let speed = if kind == Kind::Metal { 0.8 + 0.01 * rand(21) as f32 } else { 0.4 + 0.001 * rand(21) as f32 };
        let size = if kind == Kind::Reverb { 1.0 + 0.1 * rand(41) as f32 } else { 3.0 + 0.1 * rand(21) as f32 };
        let aim = aim.filter(|_| kind == Kind::Metal);
        let mut angles = [0; 4];
        for angle in &mut angles {
            *angle = aimed(aim, rand);
        }
        let notes = [Note::new(angles[0], angles[1]), Note::new(angles[2], angles[3])];
        Self { speed, size, notes, age: 0, active: true, aim }
    }

    fn step(&mut self, rand: &mut impl FnMut(u32) -> u32) {
        self.age += 1;
        for note in &mut self.notes {
            note.turn(self.aim, rand);
        }
        // Metallic Sound's rise only to n = 9 and fade from 21.
        let (rise, hold) = if self.aim.is_some() { (10, 20) } else { (50, 60) };
        for note in &mut self.notes {
            if self.age < rise {
                note.alpha = (note.alpha + 25).min(255);
            } else if self.age > hold {
                note.alpha -= 3;
                if note.alpha < 0 {
                    note.alpha = 0;
                    self.active = false;
                }
            }
        }
        self.notes[0].drift(self.speed, 1.0);
        self.notes[1].drift(self.speed, -1.0);
    }
}

impl Notes {
    /// Lullaby Deep Sleep on `unit`, now standing at `feet`.
    pub fn lullaby(&mut self, unit: Option<u32>, feet: Vector, now: Instant) {
        self.start(Kind::Lullaby, unit, feet, None, false, now);
    }

    /// Metallic Sound from the caster `unit`, now standing at `feet`, at the target's place `aim` (`min` is /mineffect).
    pub fn metallic(&mut self, unit: Option<u32>, feet: Vector, aim: Vector, min: bool, now: Instant) {
        self.start(Kind::Metal, unit, feet, Some(aim), min, now);
    }

    /// Reverberation's notes on `unit`, now standing at `feet` (`min` is /mineffect).
    pub fn reverb(&mut self, unit: Option<u32>, feet: Vector, min: bool, now: Instant) {
        self.start(Kind::Reverb, unit, feet, None, min, now);
    }

    fn start(&mut self, kind: Kind, unit: Option<u32>, feet: Vector, aim: Option<Vector>, min: bool, now: Instant) {
        self.sources.push(Source { kind, unit, start: now, last: feet, waves: 0, aim, min });
    }

    /// Makes the waves whose ticks have come (at the unit's feet then), runs the bands a tick at a time and drops what ended. The
    /// choices of each band come from `rand(n)`.
    pub fn advance(&mut self, now: Instant, position_of: impl Fn(Option<u32>) -> Option<Vector>, mut rand: impl FnMut(u32) -> u32) {
        for source in &mut self.sources {
            source.last = position_of(source.unit).unwrap_or(source.last);
            while source.waves < WAVES.len() && passes(source.start, now) > WAVES[source.waves] {
                let start = source.start + super::TICK * WAVES[source.waves];
                let at = source.last;
                let aim = source.aim.map(|aim| bearing(aim[0] - at[0], aim[2] - at[2]));
                let (textures, bands) = source.kind.wave(source.min);
                for &texture in textures {
                    let bands = (0..bands).map(|_| Band::new(source.kind, aim, &mut rand)).collect();
                    self.prims.push(Prim { kind: source.kind, texture, at, start, done: 0, bands });
                }
                source.waves += 1;
            }
        }
        for prim in &mut self.prims {
            while prim.done < passes(prim.start, now).saturating_sub(1).min(prim.kind.last_age()) {
                prim.done += 1;
                for band in prim.bands.iter_mut().filter(|band| band.active) {
                    band.step(&mut rand);
                }
            }
        }
        self.prims.retain(|prim| passes(prim.start, now) <= prim.kind.last_age() + 2);
        self.sources.retain(|source| passes(source.start, now) <= WAVES[WAVES.len() - 1] + 1);
    }

    /// The notes' squares facing the camera, drawn over everything (`rise` screen pixels are taken off again). Lullaby's: per note a
    /// white blended one at a third of the alpha, then a pink-red added-on one. Metallic Sound's: per note one white added-on one at
    /// the full alpha, the picture mirrored; Reverberation's the same for the first note only.
    pub fn draws(&self, rise: f32) -> Vec<NoteDraw> {
        let mut draws = Vec::new();
        for prim in &self.prims {
            for band in prim.bands.iter().filter(|band| band.active) {
                let half = band.size * PIXELS_PER_UNIT;
                // ponytail: the picture stands upright; the corner angles' own uv order would turn it over.
                let corners = [225.0_f32, 315.0, 45.0, 135.0].map(|angle| {
                    let (sin, cos) = angle.to_radians().sin_cos();
                    [half * cos, half * sin + rise]
                });
                // Reverberation's second notes still steer and drift (their draws keep the order of the choices) but are not drawn.
                let notes = if prim.kind == Kind::Reverb { &band.notes[..1] } else { &band.notes[..] };
                for note in notes {
                    let alpha = note.alpha as f32;
                    if alpha < 1.0 {
                        continue;
                    }
                    let at = [prim.at[0] + note.at[0], prim.at[1] + note.at[1], prim.at[2] + note.at[2]];
                    let texture = prim.texture;
                    if prim.kind == Kind::Lullaby {
                        draws.push(NoteDraw { texture, at, corners, tint: [1.0, 1.0, 1.0, (alpha / 3.0).floor() / 255.0], additive: false });
                        draws.push(NoteDraw { texture, at, corners, tint: [1.0, 50.0 / 255.0, 50.0 / 255.0, alpha / 255.0], additive: true });
                    } else {
                        let mirrored = corners.map(|[x, y]| [-x, y]);
                        draws.push(NoteDraw { texture, at, corners: mirrored, tint: [1.0, 1.0, 1.0, alpha / 255.0], additive: true });
                    }
                }
            }
        }
        draws
    }

    /// The same as quads with their textures.
    pub fn quads<'a>(&self, texture_of: impl Fn(&str) -> Option<&'a Texture>, rise: f32) -> Vec<EffectQuad<'a>> {
        self.draws(rise).into_iter().filter_map(|draw| Some((texture_of(draw.texture)?, draw.at, draw.corners, draw.tint, draw.additive, true))).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prims::TICK;

    fn near(got: f32, want: f32) -> bool {
        (got - want).abs() < 1e-4
    }

    #[test]
    fn twelve_prims_of_four_bands_come_in_three_waves() {
        let start = Instant::now();
        let mut notes = Notes::default();
        notes.lullaby(Some(1), [5.0, 1.0, 5.0], start);
        let mut draws = Vec::new();
        notes.advance(
            start,
            |_| Some([5.0, 1.0, 5.0]),
            |n| {
                draws.push(n);
                0
            },
        );
        // Four prims at tick 0, each of four bands, each six draws: R(21), R(21), then four R(360).
        assert_eq!(notes.prims.len(), 4);
        assert_eq!(draws.len(), 4 * 4 * 6);
        assert_eq!(draws[..6], [21, 21, 360, 360, 360, 360]);
        assert_eq!(notes.prims.iter().map(|prim| prim.texture).collect::<Vec<_>>(), [TEXTURES[0].0, TEXTURES[1].0, TEXTURES[0].0, TEXTURES[1].0]);
        assert!(notes.prims.iter().all(|prim| prim.bands.len() == 4 && prim.bands.iter().all(|band| band.notes.len() == 2)));
        // Every draw 0: s 0.400, r 3.0, all angles 0, both notes at (0, 5 up, 0).
        let band = &notes.prims[0].bands[0];
        assert!(near(band.speed, 0.4) && near(band.size, 3.0));
        assert!(band.notes.iter().all(|note| note.yaw == 0 && note.target == 0 && note.at == [0.0, 5.0, 0.0] && note.pitch == 180 && note.pitch_target == 180));
        // Nothing more until tick 10 and 20, none after.
        notes.advance(start + TICK * 9, |_| Some([0.0; 3]), |_| 0);
        assert_eq!(notes.prims.len(), 4);
        notes.advance(start + TICK * 10, |_| Some([30.0, 1.0, 30.0]), |_| 0);
        assert_eq!(notes.prims.len(), 8);
        assert_eq!(notes.prims[4].at, [30.0, 1.0, 30.0], "the target's feet at the wave's tick");
        notes.advance(start + TICK * 20, |_| Some([60.0, 1.0, 60.0]), |_| 0);
        assert_eq!(notes.prims.len(), 12);
        notes.advance(start + TICK * 30, |_| Some([90.0, 1.0, 90.0]), |_| 0);
        assert_eq!(notes.prims.len(), 12);
        // The largest R(21): s 0.420, r 5.0.
        let high = Band::new(Kind::Lullaby, None, &mut |n| if n == 21 { 20 } else { 0 });
        assert!(near(high.speed, 0.42) && near(high.size, 5.0));
    }

    #[test]
    fn the_alpha_rises_holds_and_fades() {
        assert_eq!([1, 2, 10, 11, 49, 60, 61, 145].map(note_alpha), [25, 50, 250, 255, 255, 255, 252, 0]);
        let mut band = Band::new(Kind::Lullaby, None, &mut |_| 0);
        let mut seen = Vec::new();
        for n in 1..=146 {
            if !band.active {
                break;
            }
            band.step(&mut |_| 0);
            if [1, 2, 10, 11, 49, 60, 61, 145].contains(&n) {
                seen.push(band.notes[0].alpha);
            }
        }
        assert_eq!(seen, [25, 50, 250, 255, 255, 255, 252, 0]);
        assert!(!band.active, "ended at n = 146");
    }

    #[test]
    fn notes_steer_toward_random_targets() {
        // C = 10, T = 10 turns to 8 and hits; C = 0, T = 100 goes up 2; C = 359, T = 10 goes to 361, a hit.
        assert_eq!(steer(10, 10, 2), (8, true));
        assert_eq!(steer(0, 100, 2), (2, false));
        assert_eq!(steer(359, 10, 2), (361, true));
        assert_eq!(steer(100, 90, 2), (98, false));
        assert_eq!(steer(10, 350, 2), (8, false), "d = −340: down through 0, a hit only past −10");
        assert_eq!(steer(-9, 350, 2), (-11, true));
        // The pitch moves 1 degree a tick; a hit redraws the targets as φ + R(360) and 175 + R(11), and wraps the angles.
        let mut note = Note::new(10, 359);
        note.pitch_target = 100;
        note.pitch = 100;
        note.turn(None, &mut |n| n - 1);
        assert_eq!((note.yaw, note.target), (1, 359));
        assert_eq!((note.pitch, note.pitch_target), (99, 185));
        let mut miss = Note::new(100, 0);
        let mut drawn = 0;
        miss.turn(None, &mut |_| {
            drawn += 1;
            0
        });
        assert_eq!((miss.yaw, drawn), (2, 1), "only the pitch hit (180 → 179 toward 180)");
    }

    #[test]
    fn notes_drift_along_their_headings_and_up_or_down() {
        let mut band = Band::new(Kind::Lullaby, None, &mut |_| 0);
        band.notes[0].yaw = 2;
        band.notes[0].target = 0;
        band.notes[1].yaw = 2;
        band.notes[1].target = 0;
        // Heading about 0 (it steers to 0 first): x +0.4 a tick, z and up almost unchanged at pitch 180.
        band.step(&mut |_| 0);
        let note = &band.notes[0];
        assert!(near(note.at[0], 0.4) && note.at[2].abs() < 1e-3);
        let mut tilted = Band::new(Kind::Lullaby, None, &mut |_| 0);
        for note in &mut tilted.notes {
            note.pitch = 185;
            note.pitch_target = 190;
        }
        tilted.step(&mut |_| 0);
        // The pitch steps to 186; note 1 sinks (sin 186° < 0) and note 2 rises by as much.
        let sink = 0.4 * 186.0_f32.to_radians().sin();
        assert!(near(tilted.notes[0].at[1], 5.0 + sink) && near(tilted.notes[1].at[1], 5.0 - sink));
        assert!(tilted.notes[0].at[1] < 5.0 && tilted.notes[1].at[1] > 5.0);
    }

    #[test]
    fn each_note_draws_a_white_square_then_a_pink_red_one_over_everything() {
        let start = Instant::now();
        let mut notes = Notes::default();
        notes.lullaby(Some(1), [10.0, 1.0, 10.0], start);
        notes.advance(start, |_| Some([10.0, 1.0, 10.0]), |_| 0);
        // Not drawn at alpha 0 (the prims' first tick).
        assert!(notes.draws(80.0).is_empty());
        notes.advance(start + TICK, |_| Some([10.0, 1.0, 10.0]), |_| 0);
        // 16 bands × 2 notes × 2 squares; alpha 25 at n = 1: white at ⌊25/3⌋ = 8, then added on at 25.
        let draws = notes.draws(80.0);
        assert_eq!(draws.len(), 4 * 4 * 2 * 2);
        assert!(!draws[0].additive && draws[1].additive);
        assert_eq!(draws[0].tint, [1.0, 1.0, 1.0, 8.0 / 255.0]);
        assert_eq!(draws[1].tint, [1.0, 50.0 / 255.0, 50.0 / 255.0, 25.0 / 255.0]);
        // The side is 1.414 r (r = 3.0 here), in screen pixels at 7 a unit, and the 80 pixels are added to every corner's y.
        let corners = draws[0].corners;
        assert!(near((corners[1][0] - corners[0][0]).abs(), std::f32::consts::SQRT_2 * 3.0 * 7.0));
        assert!(corners.iter().all(|corner| (corner[1] - 80.0).abs() < 3.0 * 7.0));
        // The pictures alternate by prim; each note stands 5 up from where the target stood.
        assert_eq!(draws[0].texture, TEXTURES[0].0);
        assert!((draws[0].at[1] - 6.0).abs() < 0.02 && (draws[0].at[0] - 10.4).abs() < 0.02);
        // They stay put when the target moves, and are gone with the last band at tick 166.
        notes.advance(start + TICK * 2, |_| Some([90.0, 1.0, 90.0]), |_| 0);
        assert!(notes.draws(0.0).iter().all(|draw| draw.at[0] < 30.0));
        notes.advance(start + TICK * 200, |_| None, |_| 0);
        assert!(notes.prims.is_empty() && notes.sources.is_empty());
    }

    #[test]
    fn metallic_sound_flies_nine_prims_of_two_notes_at_the_target() {
        let start = Instant::now();
        let mut notes = Notes::default();
        // The target straight along +x from the caster: φ = 0.
        notes.metallic(Some(1), [10.0, 1.0, 10.0], [110.0, 1.0, 10.0], false, start);
        let mut draws = Vec::new();
        notes.advance(
            start,
            |_| Some([10.0, 1.0, 10.0]),
            |n| {
                draws.push(n);
                0
            },
        );
        // Three prims at tick 0 in the three pictures, one band of two notes each; per band R(21), R(21), four R(91).
        assert_eq!(notes.prims.iter().map(|prim| prim.texture).collect::<Vec<_>>(), METAL_NOTES);
        assert!(notes.prims.iter().all(|prim| prim.bands.len() == 1 && prim.bands[0].notes.len() == 2));
        assert_eq!(draws, [21, 21, 91, 91, 91, 91].repeat(3));
        // Every draw 0: s 0.80, r 3.0, T and C = φ − 45 (315).
        let band = &notes.prims[0].bands[0];
        assert!(near(band.speed, 0.8) && near(band.size, 3.0) && band.aim == Some(0));
        assert!(band.notes.iter().all(|note| note.target == 315 && note.yaw == 315 && note.at == [0.0, 5.0, 0.0]));
        // The largest draws: s 1.00, r 5.0, T and C = φ + 45.
        let high = Band::new(Kind::Metal, Some(90), &mut |n| n - 1);
        assert!(near(high.speed, 1.0) && near(high.size, 5.0));
        assert!(high.notes.iter().all(|note| note.target == 135 && note.yaw == 135));
        // Three more prims at ticks 10 and 20, where the caster stands then: nine in all.
        notes.advance(start + TICK * 10, |_| Some([30.0, 1.0, 10.0]), |_| 0);
        assert_eq!(notes.prims.len(), 6);
        assert_eq!(notes.prims[3].at, [30.0, 1.0, 10.0]);
        notes.advance(start + TICK * 20, |_| Some([30.0, 1.0, 10.0]), |_| 0);
        notes.advance(start + TICK * 40, |_| Some([30.0, 1.0, 10.0]), |_| 0);
        assert_eq!(notes.prims.len(), 9);
        // /mineffect's pictures.
        let mut min = Notes::default();
        min.metallic(None, [0.0; 3], [10.0, 0.0, 0.0], true, start);
        min.advance(start, |_| None, |_| 0);
        assert_eq!(min.prims.iter().map(|prim| prim.texture).collect::<Vec<_>>(), METAL_NOTES_MIN);
        // All gone with the last band at tick 20 + 96.
        notes.advance(start + TICK * 120, |_| None, |_| 0);
        assert!(notes.prims.is_empty() && notes.sources.is_empty());
    }

    #[test]
    fn metallic_notes_rise_to_225_and_fade_by_95() {
        let mut band = Band::new(Kind::Metal, Some(0), &mut |_| 0);
        let mut seen = Vec::new();
        for n in 1..=96 {
            band.step(&mut |_| 0);
            if [1, 9, 10, 20, 21, 95].contains(&n) {
                seen.push(band.notes[0].alpha);
            }
            assert_eq!(band.active, n < 96, "n = {n}");
        }
        assert_eq!(seen, [25, 225, 225, 225, 222, 0]);
        // A reached yaw target is redrawn inside φ ± 45: φ = 100, R(91) = 90 gives 145, R(91) = 0 gives 55.
        let mut note = Note::new(100, 101);
        note.turn(Some(100), &mut |n| if n == 91 { 90 } else { 0 });
        assert_eq!((note.yaw, note.target), (99, 145));
        let mut note = Note::new(100, 101);
        note.turn(Some(100), &mut |_| 0);
        assert_eq!(note.target, 55);
    }

    #[test]
    fn metallic_and_reverberation_notes_draw_one_white_mirrored_added_on_square() {
        let start = Instant::now();
        let mut notes = Notes::default();
        notes.metallic(None, [0.0; 3], [10.0, 0.0, 0.0], false, start);
        notes.advance(start + TICK, |_| None, |_| 0);
        // Three bands × two notes, one square each: white, additive, the full alpha (25 at n = 1), corners mirrored left to right.
        let draws = notes.draws(0.0);
        assert_eq!(draws.len(), 3 * 2);
        assert!(draws.iter().all(|draw| draw.additive && draw.tint == [1.0, 1.0, 1.0, 25.0 / 255.0]));
        assert!(draws[0].corners[0][0] > 0.0 && draws[0].corners[1][0] < 0.0, "{:?}", draws[0].corners);
        assert!(near((draws[0].corners[0][0] - draws[0].corners[1][0]).abs(), std::f32::consts::SQRT_2 * 3.0 * 7.0));
        let mut reverb = Notes::default();
        reverb.reverb(Some(2), [0.0; 3], false, start);
        reverb.advance(start + TICK, |_| Some([0.0; 3]), |_| 0);
        // Two prims of four bands: only note 1 of each is drawn.
        let draws = reverb.draws(0.0);
        assert_eq!(draws.len(), 2 * 4);
        assert!(draws.iter().all(|draw| draw.additive && draw.tint == [1.0, 1.0, 1.0, 25.0 / 255.0]));
        assert!(draws[0].corners[0][0] > 0.0);
    }

    #[test]
    fn reverberation_drifts_24_small_pale_blue_notes_out_of_the_target() {
        let start = Instant::now();
        let mut notes = Notes::default();
        notes.reverb(Some(2), [5.0, 1.0, 5.0], false, start);
        let mut draws = Vec::new();
        notes.advance(
            start,
            |_| Some([5.0, 1.0, 5.0]),
            |n| {
                draws.push(n);
                0
            },
        );
        // Two prims at tick 0 (rev_note_01, rev_note_02), four bands each; per band R(21), R(41), four R(360).
        assert_eq!(notes.prims.iter().map(|prim| prim.texture).collect::<Vec<_>>(), REVERB_NOTES);
        assert!(notes.prims.iter().all(|prim| prim.bands.len() == 4));
        assert_eq!(draws, [21, 41, 360, 360, 360, 360].repeat(8));
        // Size 1.0 and 5.0 for draws 0 and 40; speed 0.400 as Lullaby's.
        let band = &notes.prims[0].bands[0];
        assert!(near(band.size, 1.0) && near(band.speed, 0.4) && band.aim.is_none());
        let high = Band::new(Kind::Reverb, None, &mut |n| if n == 41 { 40 } else { 0 });
        assert!(near(high.size, 5.0));
        // Six prims after tick 20; the alpha as Lullaby's (255 held to 60, ended at 146).
        notes.advance(start + TICK * 20, |_| Some([5.0, 1.0, 5.0]), |_| 0);
        assert_eq!(notes.prims.len(), 6);
        let mut band = Band::new(Kind::Reverb, None, &mut |_| 0);
        for _ in 0..61 {
            band.step(&mut |_| 0);
        }
        assert_eq!(band.notes[0].alpha, note_alpha(61));
        let mut min = Notes::default();
        min.reverb(None, [0.0; 3], true, start);
        min.advance(start, |_| None, |_| 0);
        assert_eq!(min.prims.iter().map(|prim| prim.texture).collect::<Vec<_>>(), REVERB_NOTES_MIN);
    }
}
