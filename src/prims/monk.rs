//! The monk and assassin skills' code-made shapes: Investigate's 20 blue sparks (EF_TEIHIT2, the game's prim kind 45) and
//! its claw slash (EF_CHIMTO, kind 46), Back Stab's 80 sparks (EF_BACKSTAP), Asura Strike's 96 white streaks (EF_TEIHIT1X,
//! kind 44) and Raid's 80 cyan ones (EF_TEIHIT3).
//!
//! A shape's count n starts below 0 and goes up one a tick: n = counter0 + t + 1 at tick t of its effect.

use std::time::Instant;

use super::first::{Square, THUNDER_CENTER};
use super::passes;
use crate::world::{MeshVertex, Vector};

/// Asura Strike's streak texture; the claw slash's sprite.
pub const LENS: &str = "data\\texture\\effect\\lens1.tga";
pub const LENS_RAID: &str = "data\\texture\\effect\\lens2.tga";
pub const CLAW: &str = "data\\sprite\\이팩트\\침투경_";
pub const FVOICE: &str = "data\\sprite\\이팩트\\fvoice";
pub const WINK: &str = "data\\sprite\\이팩트\\wink";
/// Investigate's sparks are 5 prims of 4, Back Stab's 20; lifted 8 units, coloured #8282FA.
const INVESTIGATE_SPARKS: usize = 20;
const BACK_STAB_SPARKS: usize = 80;
const SPARK_RISE: f32 = 8.0;
const SPARK_TINT: [u8; 3] = [0x82, 0x82, 0xFA];

/// What tells Asura Strike's streaks from Raid's.
pub struct StreakLook {
    texture: &'static str,
    count: usize,
    /// Each streak's count starts at −`counter_from` − R(`counter_span`).
    counter_from: i32,
    counter_span: u32,
    /// How far out the centre moves a tick, and the strip's half-width and half-length.
    step: f32,
    half_width: f32,
    half_length: f32,
    tint: [f32; 3],
    /// Drawn over everything, with no depth test.
    over: bool,
}

/// Asura Strike's: 24 prims of 4, half-width 0.7 and length factor 24, so 16.8 either side of the centre.
const ASURA: StreakLook = StreakLook {
    texture: LENS,
    count: 96,
    counter_from: 10,
    counter_span: 25,
    step: 3.0,
    half_width: 0.7,
    half_length: 24.0 * 0.7,
    tint: [1.0; 3],
    over: false,
};
/// Raid's: 20 prims of 4, half-width 1 and half-length 16, in #41B8F0.
const RAID: StreakLook = StreakLook {
    texture: LENS_RAID,
    count: 80,
    counter_from: 20,
    counter_span: 10,
    step: 2.0,
    half_width: 1.0,
    half_length: 16.0,
    tint: [65.0 / 255.0, 184.0 / 255.0, 240.0 / 255.0],
    over: true,
};

/// What tells one sprite at a target from another: the claw slash is drawn 2.5 times as big in white at alpha 200 for 15 ticks, 2
/// ticks a frame, turned and mirrored; Pang Voice's ring and Wink of Charm's heart are as they are, 6 ticks a frame for 99, and
/// show frame 0 after their last.
pub struct SpriteLook {
    pub sprite: &'static str,
    pub scale: f32,
    pub alpha: f32,
    per_frame: u32,
    /// The last frame that counts up; after it frame 0 shows.
    last: usize,
    life: u32,
    turned: bool,
}

const SLASH: SpriteLook = SpriteLook { sprite: CLAW, scale: 2.5, alpha: 200.0, per_frame: 2, last: usize::MAX, life: 15, turned: true };
const PANG_VOICE: SpriteLook = SpriteLook { sprite: FVOICE, scale: 1.0, alpha: 255.0, per_frame: 6, last: 11, life: 99, turned: false };
const WINK_OF_CHARM: SpriteLook = SpriteLook { sprite: WINK, scale: 1.0, alpha: 255.0, per_frame: 6, last: 5, life: 99, turned: false };

struct Spark {
    counter: i32,
    size: f32,
    speed: f32,
    fall: f32,
    /// Its heading, once n reaches 1.
    heading: Option<f32>,
}

struct Investigation {
    caster: Option<u32>,
    aim: Vector,
    start: Instant,
    sparks: Vec<Spark>,
}

struct Streak {
    counter: i32,
    yaw: f32,
    angle: f32,
}

struct Starburst {
    look: &'static StreakLook,
    at: Vector,
    start: Instant,
    min: bool,
    streaks: Vec<Streak>,
}

/// A claw slash at the target's feet: which of the sprite's 4 actions, and when it started.
pub struct Claw {
    look: &'static SpriteLook,
    pub at: Vector,
    pub action: usize,
    pub start: Instant,
}

/// What the claw slash draws: where, its action and frame, and how it is turned and mirrored.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClawDraw {
    pub sprite: &'static str,
    pub scale: f32,
    pub alpha: f32,
    pub at: Vector,
    pub action: usize,
    pub frame: usize,
    pub turn: f32,
    pub mirrored: bool,
}

#[derive(Default)]
pub struct Monk {
    investigations: Vec<Investigation>,
    starbursts: Vec<Starburst>,
    claws: Vec<Claw>,
}

/// A spark's alpha at count n: 0 at 1, 15 more a tick to 135 at 10, then 3 less a tick, off at 0 (n = 55).
pub fn spark_alpha(n: i32) -> f32 {
    match n {
        ..=1 => 0.0,
        2..=10 => 15.0 * (n - 1) as f32,
        _ => (135.0 - 3.0 * (n - 10) as f32).max(0.0),
    }
}

/// A streak's alpha at count n: up 15 a tick from n = 11 to 150 at 20 (8 a tick to 80 under /mineffect), then 5 less a tick.
pub fn streak_alpha(n: i32, min: bool) -> f32 {
    if n <= 10 {
        return 0.0;
    }
    let (rise, top) = if min { (8.0, 80.0) } else { (15.0, 150.0) };
    let up = (rise * (n - 10) as f32).min(top);
    if n <= 20 { up } else { (top - 5.0 * (n - 20) as f32).max(0.0) }
}

/// How far a streak's centre is from where it bursts at count n.
fn streak_distance(look: &StreakLook, n: i32) -> f32 {
    -10.0 + look.step * n as f32
}

/// A streak's corners over `at` at distance `r`: along the vertical plane at `yaw`, pointing at `angle` in it.
fn streak_corners(look: &StreakLook, at: Vector, r: f32, yaw: f32, angle: f32) -> [Vector; 4] {
    let (sin_a, cos_a) = angle.to_radians().sin_cos();
    let (sin_b, cos_b) = yaw.to_radians().sin_cos();
    let corner = |d: f32, w: f32| {
        let x = d * cos_a - w * sin_a;
        [at[0] + x * cos_b, at[1] - (d * sin_a + w * cos_a), at[2] + x * sin_b]
    };
    let (inner, outer) = (r - look.half_length, r + look.half_length);
    [corner(outer, -look.half_width), corner(outer, look.half_width), corner(inner, look.half_width), corner(inner, -look.half_width)]
}

/// The claw slash's action for d, the camera's longitude plus the direction to the caster, whole degrees.
pub fn claw_action(d: u32) -> usize {
    match d % 360 {
        180..=269 => 0,
        90..=179 => 1,
        0..=89 => 2,
        _ => 3,
    }
}

/// The claw slash's frame at tick t: 1, 1, 2, 2 … 8 (frames 0 and 9 to 11 never show).
#[cfg(test)]
pub fn claw_frame(t: u32) -> usize {
    sprite_frame(&SLASH, t)
}

/// A sprite look's frame at tick t: from frame 1, a frame every `per_frame` ticks, then frame 0 past its last.
fn sprite_frame(look: &SpriteLook, t: u32) -> usize {
    let frame = 1 + (t / look.per_frame) as usize;
    if frame > look.last { 0 } else { frame }
}

impl Monk {
    /// Investigate's sparks, flying from `aim` (the target's place) after the caster `caster`; each spark's choices from
    /// `rand(n)` (0 to n − 1): its count, size, speed and fall.
    pub fn investigate(&mut self, caster: Option<u32>, aim: Vector, now: Instant, rand: impl FnMut(u32) -> u32) {
        self.spark(INVESTIGATE_SPARKS, caster, aim, now, rand);
    }

    /// Back Stab's 80 sparks, the same way.
    pub fn back_stab(&mut self, caster: Option<u32>, aim: Vector, now: Instant, rand: impl FnMut(u32) -> u32) {
        self.spark(BACK_STAB_SPARKS, caster, aim, now, rand);
    }

    fn spark(&mut self, count: usize, caster: Option<u32>, aim: Vector, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        let sparks = (0..count)
            .map(|_| {
                let counter = rand(10) as i32 - 40;
                let size = 0.1 + 0.05 * rand(16) as f32;
                let speed = 0.75 + 0.01 * rand(50) as f32;
                let fall = 0.01 * rand(200) as f32 - 1.0;
                Spark { counter, size, speed, fall, heading: None }
            })
            .collect();
        self.investigations.push(Investigation { caster, aim, start: now, sparks });
    }

    /// Asura Strike's streaks over `at`: per streak its count, yaw and angle.
    pub fn asura(&mut self, at: Vector, now: Instant, min: bool, rand: impl FnMut(u32) -> u32) {
        self.burst(&ASURA, at, now, min, rand);
    }

    /// Raid's streaks over the user's feet `at`.
    pub fn raid(&mut self, at: Vector, now: Instant, rand: impl FnMut(u32) -> u32) {
        self.burst(&RAID, at, now, false, rand);
    }

    fn burst(&mut self, look: &'static StreakLook, at: Vector, now: Instant, min: bool, mut rand: impl FnMut(u32) -> u32) {
        let streaks = (0..look.count)
            .map(|_| {
                let counter = -look.counter_from - rand(look.counter_span) as i32;
                let yaw = rand(360) as f32;
                let angle = rand(360) as f32;
                Streak { counter, yaw, angle }
            })
            .collect();
        self.starbursts.push(Starburst { look, at, start: now, min, streaks });
    }

    /// A claw slash at the target's feet `to`, the caster at `from`, the camera at `longitude` degrees.
    pub fn claw(&mut self, from: Vector, to: Vector, longitude: i32, now: Instant) {
        self.sprite_at(&SLASH, from, to, longitude, now);
    }

    /// Pang Voice's ring (606) and Wink of Charm's heart (607) at the target, the same way.
    pub fn pang_voice(&mut self, from: Vector, to: Vector, longitude: i32, now: Instant) {
        self.sprite_at(&PANG_VOICE, from, to, longitude, now);
    }

    pub fn wink(&mut self, from: Vector, to: Vector, longitude: i32, now: Instant) {
        self.sprite_at(&WINK_OF_CHARM, from, to, longitude, now);
    }

    fn sprite_at(&mut self, look: &'static SpriteLook, from: Vector, to: Vector, longitude: i32, now: Instant) {
        let toward = (from[2] - to[2]).atan2(from[0] - to[0]).to_degrees().trunc() as i32;
        let action = claw_action((longitude + toward).rem_euclid(360) as u32);
        self.claws.push(Claw { look, at: to, action, start: now });
    }

    /// A spark takes its heading when its count reaches 1: toward the aim from the caster's place then, a whole number of
    /// degrees, turned by R(60) − 30; and effects that ended go.
    pub fn advance(&mut self, now: Instant, position_of: impl Fn(Option<u32>) -> Option<Vector>, mut rand: impl FnMut(u32) -> u32) {
        for investigation in &mut self.investigations {
            let Some(t) = passes(investigation.start, now).checked_sub(1) else { continue };
            for spark in &mut investigation.sparks {
                if spark.heading.is_none()
                    && spark.counter + t as i32 + 1 >= 1
                    && let Some(caster) = position_of(investigation.caster)
                {
                    let toward = (investigation.aim[2] - caster[2]).atan2(investigation.aim[0] - caster[0]).to_degrees().trunc();
                    spark.heading = Some((toward + rand(60) as f32 - 30.0).rem_euclid(360.0));
                }
            }
        }
        self.investigations.retain(|investigation| passes(investigation.start, now) <= 100);
        self.starbursts.retain(|burst| passes(burst.start, now) <= 100);
        self.claws.retain(|claw| passes(claw.start, now) <= claw.look.life);
    }

    /// Investigate's sparks at `now`: camera-facing squares, added on.
    pub fn squares(&self, now: Instant) -> Vec<Square> {
        let mut squares = Vec::new();
        for investigation in &self.investigations {
            let Some(t) = passes(investigation.start, now).checked_sub(1) else { continue };
            for spark in &investigation.sparks {
                let n = spark.counter + t as i32 + 1;
                let (true, Some(heading)) = ((2..=54).contains(&n), spark.heading) else { continue };
                let (sin, cos) = heading.to_radians().sin_cos();
                let steps = n as f32;
                let at = [
                    investigation.aim[0] + spark.speed * cos * steps,
                    investigation.aim[1] + SPARK_RISE - spark.fall * steps,
                    investigation.aim[2] + spark.speed * sin * steps,
                ];
                let spin = (-5.0 * steps).rem_euclid(360.0);
                squares.push(Square { texture: THUNDER_CENTER, at, radius: spark.size, spin, alpha: spark_alpha(n), mirrored: false, tint: SPARK_TINT });
            }
        }
        squares
    }

    /// The streaks at `now`, six vertices each, by texture: the ones drawn over everything (`over`) or the others.
    pub fn meshes(&self, now: Instant, over: bool) -> Vec<(&'static str, Vec<MeshVertex>)> {
        let mut meshes: Vec<(&'static str, Vec<MeshVertex>)> = Vec::new();
        for burst in self.starbursts.iter().filter(|burst| burst.look.over == over) {
            let vertices = Self::streaks(burst, now);
            match meshes.iter_mut().find(|(texture, _)| *texture == burst.look.texture) {
                Some((_, all)) => all.extend(vertices),
                None => meshes.push((burst.look.texture, vertices)),
            }
        }
        meshes
    }

    /// One burst's streaks: the texture runs from a streak's outer end (v 1 at one side).
    fn streaks(burst: &Starburst, now: Instant) -> Vec<MeshVertex> {
        let Some(t) = passes(burst.start, now).checked_sub(1) else { return Vec::new() };
        let mut vertices = Vec::new();
        for streak in &burst.streaks {
            let n = streak.counter + t as i32 + 1;
            let alpha = streak_alpha(n, burst.min);
            if !(11..=49).contains(&n) || alpha <= 0.0 {
                continue;
            }
            let tint = [burst.look.tint[0], burst.look.tint[1], burst.look.tint[2], alpha.trunc() / 255.0];
            let [c0, c1, c2, c3] = streak_corners(burst.look, burst.at, streak_distance(burst.look, n), streak.yaw, streak.angle);
            let corner = |position, uv| MeshVertex { position, uv, tint };
            let [a, b, c, d] = [corner(c0, [0.0, 1.0]), corner(c1, [1.0, 1.0]), corner(c2, [1.0, 0.0]), corner(c3, [0.0, 0.0])];
            vertices.extend([a, b, c, d, c, a]);
        }
        vertices
    }

    /// The claw slashes showing at `now`.
    pub fn claws(&self, now: Instant) -> Vec<ClawDraw> {
        self.claws
            .iter()
            .filter_map(|claw| {
                let t = passes(claw.start, now).checked_sub(1).filter(|&t| t < claw.look.life)?;
                // The slash is turned 325° for actions 0 and 2, 35° for 1 and 3, and mirrored for 2 and 3.
                let turn = if !claw.look.turned {
                    0.0
                } else if claw.action % 2 == 0 {
                    325.0
                } else {
                    35.0
                };
                let (sprite, scale, alpha) = (claw.look.sprite, claw.look.scale, claw.look.alpha);
                Some(ClawDraw {
                    sprite,
                    scale,
                    alpha,
                    at: claw.at,
                    action: claw.action,
                    frame: sprite_frame(claw.look, t),
                    turn,
                    mirrored: claw.look.turned && claw.action >= 2,
                })
            })
            .collect()
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
    fn investigate_flies_20_sparks_out_after_31_to_40_ticks() {
        let start = Instant::now();
        let caster = [0.0, 0.0, 0.0];
        let aim = [10.0, 3.0, 0.0];
        let mut monk = Monk::default();
        monk.investigate(Some(1), aim, start, |_| 0);
        assert_eq!(monk.investigations[0].sparks.len(), 20);
        let run = |monk: &mut Monk, tick: u32| {
            let now = start + TICK * tick;
            monk.advance(now, |_| Some(caster), |_| 0);
            monk.squares(now)
        };
        // Every draw 0: counter −40, so n = 1 at tick 40 (nothing drawn), n = 2 at tick 41.
        assert!(run(&mut monk, 39).is_empty() && run(&mut monk, 40).is_empty());
        let squares = run(&mut monk, 41);
        assert_eq!(squares.len(), 20);
        // Heading 0° (east) − 30° = 330° from the caster to the aim; it starts 8 up and is at start + n × velocity.
        let heading = 330.0_f32.to_radians();
        let n = 2.0;
        assert!(near(squares[0].at[0], aim[0] + 0.75 * heading.cos() * n) && near(squares[0].at[2], aim[2] + 0.75 * heading.sin() * n));
        assert!(near(squares[0].at[1], 3.0 + 8.0 + n), "fall −1.0: up 1 a tick");
        assert!(squares[0].texture == THUNDER_CENTER && squares[0].tint == [0x82, 0x82, 0xFA] && near(squares[0].radius, 0.1) && !squares[0].mirrored);
        // Alpha 15 at n = 2, 135 at 10, 132 at 11, drawn to n = 54 (alpha 3), 0 at n = 55; the spin at n = 3 is 345°.
        assert_eq!([2, 10, 11, 54, 55].map(spark_alpha), [15.0, 135.0, 132.0, 3.0, 0.0]);
        assert_eq!(run(&mut monk, 42)[0].spin, 345.0);
        assert_eq!(run(&mut monk, 49)[0].alpha, 135.0);
        assert_eq!(run(&mut monk, 93).len(), 20);
        assert!(run(&mut monk, 94).is_empty());
        // The sparks wait 31 to 40 ticks: counter −31 shows at tick 32.
        let mut late = Monk::default();
        late.investigate(None, aim, start, |n| if n == 10 { 9 } else { 0 });
        late.advance(start + TICK * 31, |_| Some(caster), |_| 0);
        assert_eq!(late.squares(start + TICK * 31).len(), 0, "n = 1 at tick 31");
        late.advance(start + TICK * 32, |_| Some(caster), |_| 0);
        assert_eq!(late.squares(start + TICK * 32).len(), 20, "the first picture at 32");
    }

    #[test]
    fn asura_strike_shoots_96_streaks_out() {
        let start = Instant::now();
        let at = [5.0, 1.0, 5.0];
        let mut monk = Monk::default();
        monk.asura(at, start, false, |_| 0);
        assert_eq!(monk.starbursts[0].streaks.len(), 96);
        // Every draw 0: counter −10, so n = 11 at tick 20; alpha 15 there, 150 at n = 20, 145 at 21, 0 at 50.
        let mesh = |tick: u32| monk.meshes(start + TICK * tick, false).into_iter().next().map(|(_, vertices)| vertices).unwrap_or_default();
        assert!(mesh(19).is_empty());
        assert_eq!(mesh(20).len(), 96 * 6);
        assert_eq!([11, 20, 21, 49, 50].map(|n| streak_alpha(n, false)), [15.0, 150.0, 145.0, 5.0, 0.0]);
        assert_eq!((mesh(20)[0].tint[3] * 255.0).round(), 15.0);
        assert_eq!((mesh(29)[0].tint[3] * 255.0).round(), 150.0);
        assert!(mesh(58).len() == 96 * 6 && mesh(59).is_empty(), "drawn for n = 11…49");
        // r = 23 at n = 11, 137 at n = 49.
        assert_eq!([11, 49].map(|n| streak_distance(&ASURA, n)), [23.0, 137.0]);
        // For A = 0, B = 0 at r = 23: the corners (39.8, 0.7, 0), (39.8, −0.7, 0), (6.2, −0.7, 0), (6.2, 0.7, 0) (x, height, z).
        let corners = streak_corners(&ASURA, [0.0; 3], 23.0, 0.0, 0.0);
        let want = [[39.8, 0.7, 0.0], [39.8, -0.7, 0.0], [6.2, -0.7, 0.0], [6.2, 0.7, 0.0]];
        assert!((0..4).all(|k| (0..3).all(|axis| near(corners[k][axis], want[k][axis]))), "{corners:?}");
        // Under /mineffect the rise is 8 a tick: 80 at n = 20 and gone at 36.
        assert_eq!([11, 20, 21, 35, 36].map(|n| streak_alpha(n, true)), [8.0, 80.0, 75.0, 5.0, 0.0]);
        // The streaks stay where the target stood: the first, for A = 0, B = 0 at r = 23, starts 39.8 east of it.
        let tick20 = mesh(20);
        assert!(near(tick20[0].position[0], at[0] + 39.8) && near(tick20[0].position[1], at[1] + 0.7) && near(tick20[0].position[2], at[2]));
        assert_eq!(tick20[0].uv, [0.0, 1.0]);
        monk.advance(start + TICK * 101, |_| None, |_| 0);
        assert!(monk.starbursts.is_empty());
    }

    #[test]
    fn the_claw_slash_picks_its_action_and_steps_a_frame_every_two_ticks() {
        let start = Instant::now();
        // d 0, 90, 180, 270 → actions 2, 1, 0, 3.
        assert_eq!([0, 89, 90, 179, 180, 269, 270, 359].map(claw_action), [2, 2, 1, 1, 0, 0, 3, 3]);
        // Frames 1, 1, 2, 2 … 7, 7, 8 on ticks 0…14.
        assert_eq!((0..15).map(claw_frame).collect::<Vec<_>>(), [1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8]);
        // The caster 5 east of the target (h = 0) with the camera at 180°: d = 180, action 0.
        let mut monk = Monk::default();
        monk.claw([5.0, 0.0, 0.0], [0.0; 3], 180, start);
        let draws = |monk: &Monk, tick: u32| monk.claws(start + TICK * tick);
        let drawn = draws(&monk, 0)[0];
        assert_eq!((drawn.action, drawn.frame, drawn.turn, drawn.mirrored, drawn.at), (0, 1, 325.0, false, [0.0; 3]));
        // Fifteen draws, none at tick 15.
        assert_eq!(draws(&monk, 14).len(), 1);
        assert!(draws(&monk, 15).is_empty());
        // Action 3 is turned 35° and mirrored; action 1 turned 35° only; action 2 turned 325° and mirrored.
        let mut other = Monk::default();
        other.claw([0.0, 0.0, 5.0], [0.0; 3], 180, start);
        let drawn = other.claws(start)[0];
        assert_eq!((drawn.action, drawn.turn, drawn.mirrored), (3, 35.0, true));
        let mut third = Monk::default();
        third.claw([5.0, 0.0, 0.0], [0.0; 3], 0, start);
        assert_eq!((third.claws(start)[0].action, third.claws(start)[0].turn, third.claws(start)[0].mirrored), (2, 325.0, true));
        let mut fourth = Monk::default();
        fourth.claw([0.0, 0.0, -5.0], [0.0; 3], 180, start);
        assert_eq!((fourth.claws(start)[0].action, fourth.claws(start)[0].turn, fourth.claws(start)[0].mirrored), (1, 35.0, false));
    }

    #[test]
    fn pang_voice_and_wink_stream_in_at_the_target() {
        let start = Instant::now();
        let mut monk = Monk::default();
        monk.pang_voice([5.0, 0.0, 0.0], [0.0; 3], 180, start);
        monk.wink([5.0, 0.0, 0.0], [0.0; 3], 180, start);
        let draws = |monk: &Monk, tick: u32| monk.claws(start + TICK * tick);
        // Frame 1 on ticks 0…5, frame 2 on 6…11; 606 shows frame 0 from tick 66, 607 from tick 30.
        assert_eq!([0, 5, 6, 11].map(|tick| draws(&monk, tick)[0].frame), [1, 1, 2, 2]);
        assert_eq!([65, 66, 98].map(|tick| draws(&monk, tick)[0].frame), [11, 0, 0]);
        assert_eq!([29, 30, 98].map(|tick| draws(&monk, tick)[1].frame), [5, 0, 0]);
        // Scale 1, the layer's own colour (full), no turn, no mirror; the action as the claw slash's (d = 180: 0).
        let first = draws(&monk, 0);
        assert_eq!((first[0].sprite, first[1].sprite), (FVOICE, WINK));
        assert!(first.iter().all(|draw| draw.scale == 1.0 && draw.turn == 0.0 && !draw.mirrored && draw.action == 0 && draw.at == [0.0; 3]));
        // Drawn on ticks 0…98 only.
        assert_eq!(draws(&monk, 98).len(), 2);
        assert!(draws(&monk, 99).is_empty());
        monk.advance(start + TICK * 101, |_| None, |_| 0);
        assert!(monk.claws.is_empty());
    }

    #[test]
    fn back_stab_sprays_80_sparks_and_raid_shoots_80_cyan_streaks() {
        let start = Instant::now();
        // Back Stab: the same sparks, 20 prims of 4.
        let mut monk = Monk::default();
        monk.back_stab(Some(1), [10.0, 3.0, 0.0], start, |_| 0);
        assert_eq!(monk.investigations[0].sparks.len(), 80);
        monk.advance(start + TICK * 40, |_| Some([0.0; 3]), |_| 0);
        assert_eq!(monk.squares(start + TICK * 41).len(), 80);
        assert_eq!(monk.squares(start + TICK * 41)[0].alpha, 15.0);
        assert!(monk.squares(start + TICK * 94).is_empty());
        // Raid: counter −20 − R(10), so with every draw 0 n = 11 at tick 30; r = −10 + 2n.
        let at = [5.0, 1.0, 5.0];
        let mut raid = Monk::default();
        raid.raid(at, start, |_| 0);
        assert_eq!(raid.starbursts[0].streaks.len(), 80);
        let over = |tick: u32| raid.meshes(start + TICK * tick, true).into_iter().next().unwrap_or(("", Vec::new()));
        assert!(over(29).1.is_empty());
        let (texture, vertices) = over(30);
        assert_eq!((texture, vertices.len()), (LENS_RAID, 80 * 6));
        assert!(raid.meshes(start + TICK * 30, false).is_empty(), "no depth test: not among the plain ones");
        assert_eq!([11, 20, 49, 50].map(|n| streak_alpha(n, false)), [15.0, 150.0, 5.0, 0.0]);
        assert_eq!([11, 49].map(|n| streak_distance(&RAID, n)), [12.0, 88.0]);
        // #41B8F0; the first alpha 15; drawn to n = 49 (tick 68).
        assert_eq!(vertices[0].tint[..3], [65.0 / 255.0, 184.0 / 255.0, 240.0 / 255.0]);
        assert_eq!((vertices[0].tint[3] * 255.0).round(), 15.0);
        assert_eq!((over(68).1.len(), over(69).1.len()), (80 * 6, 0));
        // The strip for A = 0, B = 0 at r = 12: (28, 1), (28, −1), (−4, −1), (−4, 1) along, height.
        let corners = streak_corners(&RAID, [0.0; 3], 12.0, 0.0, 0.0);
        let want = [[28.0, 1.0], [28.0, -1.0], [-4.0, -1.0], [-4.0, 1.0]];
        assert!((0..4).all(|k| near(corners[k][0], want[k][0]) && near(corners[k][1], want[k][1]) && near(corners[k][2], 0.0)), "{corners:?}");
    }
}
