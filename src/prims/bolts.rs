//! Cold Bolt's and Fire Bolt's arrows (EF_ICEARROW 26, EF_FIREARROW 31), as the game builds them a 16 ms tick at a
//! time on the target: one arrow a hit, 12 ticks in and every 10 after, each a crossed streak falling from high up to
//! the target's feet; a flat ring opening at the feet as each lands; and for Fire Bolt a spark every 4 ticks.

use std::time::Instant;

use super::TICK;
use crate::world::Vector;

pub const FIRE_TEXTURES: [&str; 6] = [
    "data\\texture\\effect\\불화살1.tga",
    "data\\texture\\effect\\불화살2.tga",
    "data\\texture\\effect\\불화살3.tga",
    "data\\texture\\effect\\불화살4.tga",
    "data\\texture\\effect\\불화살5.tga",
    "data\\texture\\effect\\불화살6.tga",
];
pub const ICE_ARROW: &str = "data\\texture\\effect\\icearrow.tga";
pub const RING_YELLOW: &str = "data\\texture\\effect\\ring_yellow.tga";
pub const RING_BLUE: &str = "data\\texture\\effect\\ring_blue.tga";
pub const SPARK_SPRITE: &str = "data\\sprite\\이팩트\\particle4";

/// When the arrows come, in ticks of the effect: the first at 12, then every 10; at most 50.
const FIRST: u32 = 12;
const EVERY: u32 = 10;
const MOST: u32 = 50;
/// How long the effect runs, in ticks.
const LENGTH: u32 = 160;
/// An arrow's speed (world units a tick) and life; on landing it jumps to tick 45 of its life, holds to 50 and fades.
const SPEED: f32 = 2.75;
const ARROW_LIFE: u32 = 70;
const LANDED_AGE: u32 = 45;
const FADE_FROM: u32 = 50;
/// The impact ring's life, when it has opened, and how fast its edge first grows (slowing by twice that over 70).
const RING_LIFE: u32 = 30;
const RING_STEP: f32 = 1.2;
const RING_WIDTH: f32 = 10.0;
const RING_SIDES: usize = 24;
/// A little over the ground, so the ground doesn't cover the ring.
const RING_LIFT: f32 = 0.2;
/// Fire Bolt's sparks come every 4 ticks from tick 4 to the effect's end.
const SPARK_EVERY: u32 = 4;
const OPAQUE: f32 = 254.0 / 255.0;

/// One arrow, all from the target's feet (world units, y up).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Arrow {
    /// The effect's tick it comes on.
    born: u32,
    from: Vector,
    /// Unit vector it flies along, and its heading on the ground (degrees, 0 north toward east).
    way: Vector,
    yaw: f32,
    /// Ticks after its birth it reaches the feet's height and stops.
    lands: u32,
}

/// A Fire Bolt spark (the game's prim kind 5 with `particle4`).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Spark {
    born: u32,
    from: Vector,
    way: Vector,
    speed: f32,
    life: u32,
    scale: f32,
}

/// One bolt cast on a unit (None = the player).
pub struct Volley {
    start: Instant,
    pub unit: Option<u32>,
    pub fire: bool,
    arrows: Vec<Arrow>,
    sparks: Vec<Spark>,
}

/// A textured triangle corner: where (world units) and its texture coordinate.
pub type Corner = (Vector, [f32; 2]);
/// An arrow to draw: its two crossed quads (two triangles each), its texture frame and opacity.
pub type ArrowDraw = ([[Corner; 6]; 2], usize, f32);
/// An impact ring to draw: its triangles and opacity.
pub type RingDraw = (Vec<Corner>, f32);

/// A sprite to draw: where from the feet, its frame (from the effect's start, a frame every 4 ticks), how big and how
/// opaque.
pub type SparkDraw = (Vector, u32, f32, f32);

impl Arrow {
    /// An arrow born at `born` from `rand(n)` (0 to n − 1): it starts 25–34 units east, 15–24 north and 60 up, and aims
    /// at a point up to 2 units off the feet.
    fn new(born: u32, mut rand: impl FnMut(u32) -> u32) -> Self {
        let from = [(rand(10) + 25) as f32, 60.0, (rand(10) + 15) as f32];
        let aim = [rand(4) as f32 - 2.0, 0.0, rand(4) as f32 - 2.0];
        let delta: Vector = std::array::from_fn(|axis| aim[axis] - from[axis]);
        let length = delta.iter().map(|value| value * value).sum::<f32>().sqrt();
        let way = delta.map(|value| value / length);
        let lands = (0..).find(|&tick| from[1] + way[1] * SPEED * (tick + 1) as f32 <= 0.0).unwrap_or(ARROW_LIFE);
        Arrow { born, from, way, yaw: way[0].atan2(way[2]).to_degrees(), lands }
    }

    /// Where it is `age` ticks after its birth: moving until it lands, then still.
    fn place(&self, age: u32) -> Vector {
        let moved = SPEED * (age.min(self.lands) + 1) as f32;
        std::array::from_fn(|axis| self.from[axis] + self.way[axis] * moved)
    }

    /// Its opacity `age` ticks after its birth, or None once it's gone: full until 5 ticks after landing, then fading
    /// over 20.
    fn alpha(&self, age: u32) -> Option<f32> {
        let life = if age >= self.lands { LANDED_AGE + age - self.lands } else { age };
        (life < ARROW_LIFE).then(|| OPAQUE * if life < FADE_FROM { 1.0 } else { (ARROW_LIFE - life) as f32 / (ARROW_LIFE - FADE_FROM) as f32 })
    }

    /// Its two crossed quads at `age`: `length` and `width` are halves, the texture running front (u 0) to back.
    fn quads(&self, age: u32, feet: Vector, length: f32, width: f32) -> [[Corner; 6]; 2] {
        let middle = self.place(age);
        let along = self.way;
        let across = {
            let flat = f32::hypot(along[0], along[2]).max(1e-6);
            [along[2] / flat, 0.0, -along[0] / flat]
        };
        let up = [along[1] * across[2] - along[2] * across[1], along[2] * across[0] - along[0] * across[2], along[0] * across[1] - along[1] * across[0]];
        let point = |side: Vector, x: f32, z: f32| std::array::from_fn(|axis| feet[axis] + middle[axis] + side[axis] * x + along[axis] * z);
        [across, up].map(|side| {
            let corners = [
                (point(side, -width, length), [0.0, 1.0]),
                (point(side, -width, -length), [1.0, 1.0]),
                (point(side, width, length), [0.0, 0.0]),
                (point(side, width, -length), [1.0, 0.0]),
            ];
            [corners[0], corners[1], corners[2], corners[3], corners[2], corners[1]]
        })
    }
}

/// The impact ring's outer edge `age` ticks after it opens: it grows by 1.2 less 2.4/70 more each tick.
fn ring_edge(age: u32) -> f32 {
    (1..=age + 1).map(|step| RING_STEP - step as f32 * 2.0 * RING_STEP / 70.0).sum()
}

/// Its opacity: up 25.4 a tick to 254, held until tick 20, then down to 0 at 30.
fn ring_alpha(age: u32) -> f32 {
    let alpha = if age <= 20 { (25.4 * (age + 1) as f32).min(254.0) } else { 254.0 * (RING_LIFE - age) as f32 / 10.0 };
    alpha.max(0.0) / 255.0
}

impl Volley {
    /// A bolt with `hits` hits on `unit`, from `now`; every random choice from `rand(n)`.
    pub fn new(unit: Option<u32>, fire: bool, hits: u16, now: Instant, mut rand: impl FnMut(u32) -> u32) -> Self {
        let arrows: Vec<Arrow> = (0..u32::from(hits).clamp(1, MOST)).map(|index| Arrow::new(FIRST + EVERY * index, &mut rand)).collect();
        // Each spark comes from where the newest arrow is then (the feet before the first), heading its way ±20°.
        let sparks = if fire {
            (1..)
                .map(|step| step * SPARK_EVERY)
                .take_while(|&tick| tick < LENGTH)
                .map(|tick| {
                    let newest = arrows.iter().rev().find(|arrow| arrow.born <= tick);
                    let from = newest.map_or([0.0; 3], |arrow| arrow.place(tick - arrow.born));
                    let life = rand(24) + 6;
                    let yaw = newest.map_or(0.0, |arrow| arrow.yaw) + rand(40) as f32 - 20.0;
                    let pitch = rand(20) as f32 - 10.0;
                    let speed = (rand(90) + 60) as f32 / 100.0;
                    let scale = (rand(30) + 20) as f32 / 100.0;
                    let ((sin_y, cos_y), (sin_p, cos_p)) = (yaw.to_radians().sin_cos(), pitch.to_radians().sin_cos());
                    Spark { born: tick, from, way: [cos_p * sin_y, sin_p, cos_p * cos_y], speed, life, scale }
                })
                .collect()
        } else {
            Vec::new()
        };
        Volley { start: now, unit, fire, arrows, sparks }
    }

    fn tick(&self, now: Instant) -> Option<u32> {
        Some((now.checked_duration_since(self.start)?.as_millis() / TICK.as_millis()) as u32)
    }

    pub fn alive(&self, now: Instant) -> bool {
        self.tick(now).is_none_or(|tick| tick < LENGTH)
    }

    /// The arrows to draw at `now` on feet `feet`: each one's two crossed quads, its texture frame (one a tick) and
    /// opacity.
    pub fn arrows(&self, now: Instant, feet: Vector) -> Vec<ArrowDraw> {
        let Some(tick) = self.tick(now) else { return Vec::new() };
        let (length, width) = if self.fire { (14.0, 3.5) } else { (11.5, 3.8) };
        self.arrows
            .iter()
            .filter(|arrow| arrow.born <= tick)
            .filter_map(|arrow| {
                let age = tick - arrow.born;
                let alpha = arrow.alpha(age)?;
                Some((arrow.quads(age, feet, length, width), age as usize % FIRE_TEXTURES.len(), alpha))
            })
            .collect()
    }

    /// The impact rings at `now` on feet `feet`: flat on the ground, opening outward, a tick after each arrow lands.
    pub fn rings(&self, now: Instant, feet: Vector) -> Vec<RingDraw> {
        let Some(tick) = self.tick(now) else { return Vec::new() };
        self.arrows
            .iter()
            .filter_map(|arrow| {
                let opens = arrow.born + arrow.lands + 1;
                let age = tick.checked_sub(opens).filter(|&age| age < RING_LIFE)?;
                let outer = ring_edge(age);
                let inner = outer - outer.min(RING_WIDTH);
                let point = |side: usize, radius: f32, v: f32| {
                    let (sin, cos) = (side as f32 / RING_SIDES as f32 * std::f32::consts::TAU).sin_cos();
                    ([feet[0] + radius * sin, feet[1] + RING_LIFT, feet[2] + radius * cos], [side as f32 / RING_SIDES as f32, v])
                };
                let triangles = (0..RING_SIDES)
                    .flat_map(|side| {
                        let [a, b, c, d] = [point(side, inner, 1.0), point(side, outer, 0.0), point(side + 1, inner, 1.0), point(side + 1, outer, 0.0)];
                        [a, b, c, b, c, d]
                    })
                    .collect();
                Some((triangles, ring_alpha(age)))
            })
            .collect()
    }

    /// Fire Bolt's sparks at `now`: each slows to a third and shrinks to nothing over its life, fading from the start.
    pub fn sparks(&self, now: Instant) -> Vec<SparkDraw> {
        let Some(tick) = self.tick(now) else { return Vec::new() };
        self.sparks
            .iter()
            .filter(|spark| spark.born <= tick && tick - spark.born < spark.life)
            .map(|spark| {
                let age = tick - spark.born;
                let slowing = spark.speed / (1.5 * spark.life as f32);
                let moved: f32 = (1..=age + 1).map(|step| spark.speed - step as f32 * slowing).sum();
                let at = std::array::from_fn(|axis| spark.from[axis] + spark.way[axis] * moved);
                let share = (spark.life - age) as f32 / spark.life as f32;
                (at, tick / SPARK_EVERY, spark.scale * share, OPAQUE * share)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bolts_fall_land_and_open_rings() {
        let start = Instant::now();
        // Five hits, every draw 0: arrows from (25, 60, 15) aimed at (−2, 0, −2), coming at ticks 12, 22, … 52.
        let volley = Volley::new(None, true, 5, start, |_| 0);
        assert_eq!(volley.arrows.iter().map(|arrow| arrow.born).collect::<Vec<_>>(), [12, 22, 32, 42, 52]);
        let arrow = volley.arrows[0];
        // 2.75 units a tick along a 68-unit way down 60: it reaches the feet's height on its 25th tick.
        assert_eq!(arrow.lands, 24);
        assert!(arrow.place(arrow.lands)[1] <= 0.0 && arrow.place(arrow.lands - 1)[1] > 0.0);
        assert_eq!(arrow.place(40), arrow.place(arrow.lands));
        // Full until 5 ticks after landing, half way through its fade 15 after, gone 25 after.
        assert_eq!(arrow.alpha(arrow.lands + 4), Some(OPAQUE));
        assert_eq!(arrow.alpha(arrow.lands + 15), Some(OPAQUE * 0.5));
        assert_eq!(arrow.alpha(arrow.lands + 25), None);
        // Nothing before tick 12; one arrow at 12; its ring opens the tick after it lands.
        assert!(volley.arrows(start + TICK * 11, [0.0; 3]).is_empty());
        assert_eq!(volley.arrows(start + TICK * 12, [0.0; 3]).len(), 1);
        assert!(volley.rings(start + TICK * (12 + 24), [0.0; 3]).is_empty());
        assert_eq!(volley.rings(start + TICK * (12 + 25), [0.0; 3]).len(), 1);
        // The ring's edge: 1.17 at first, past 10 by tick 9 (the width then stays 10), about 20 at its end.
        assert!((ring_edge(0) - (1.2 - 2.4 / 70.0)).abs() < 1e-5 && ring_edge(8) < 10.0 && ring_edge(9) > 10.0);
        assert_eq!([0, 9, 20, 25].map(ring_alpha), [25.4 / 255.0, 254.0 / 255.0, 254.0 / 255.0, 127.0 / 255.0]);
        // Fire Bolt sparks every 4 ticks up to 156; Cold Bolt none.
        assert_eq!(volley.sparks.len(), 39);
        assert_eq!(volley.sparks[0].from, [0.0; 3]);
        assert!(Volley::new(None, false, 5, start, |_| 0).sparks.is_empty());
        assert!(volley.alive(start + TICK * 159) && !volley.alive(start + TICK * 160));
    }
}
