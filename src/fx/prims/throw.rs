//! The item throw: an item's own picture, a lead quad and three fainter copies of it standing in the world, spins and
//! flies from the thrower to the target, goes on a little past it and fades. Potion Pitcher (299), Berserk Pitcher (541)
//! and Spore Explosion (914) fly over 26 updates; the other throws (Acid Terror, Stone Fling, Acid Demonstration's two
//! bottles, Throw Venom Knife, the ninja throws, Kunai Splash's ring of eight, Kunai Explosion and Release Ninja Spell's
//! charms) each have their own picture, tint, size, speed, hop, delay and count (`params`).

use std::collections::HashMap;
use std::time::Instant;

use super::TICK;
use crate::world::{EffectMesh, MeshVertex, Vector};

/// The game's π as its trig tables use it.
#[allow(clippy::approx_constant)]
const PI: f32 = 3.141_592;
/// It's drawn for its first 40 flight updates at most; it freezes while its thrower is this far from the player.
const DRAWN: u32 = 40;
const FAR: f32 = 130.0;
/// How high over the target's feet it aims.
const AIM_RISE: f32 = 8.0;
/// The lead spins 45° an update; the first ghost's alpha is 150 less than the lead's, the others' 25 less than the one ahead.
const SPIN: f32 = 45.0;
const GHOST_BELOW: [f32; 3] = [150.0, 25.0, 25.0];
/// Once landed an alpha falls 20 an update.
const LANDED_FADE: f32 = 20.0;
/// Kunai Splash's thrower turns this much a tick while its first kunai shows.
const FAN_TURN: f32 = 45.0;
/// Where a throw at its own thrower aims: this far behind it along −x.
const BEHIND: f32 = 8.0;

/// Kunai Splash's eight kunai and Release Ninja Spell's charms.
pub const KUNAI_SPLASH: u32 = 981;
pub const KUNAI_EXPLOSION: u32 = 983;
pub const CHARMS: u32 = 989;
/// Acid Demonstration's fire bottle and acid bottle, thrown together at its cast's start.
pub const FIRE_BOTTLE: u32 = 539;
pub const ACID_DEMONSTRATION_SKILL: u16 = 490;
pub const ACID_BOTTLE: u32 = 980;

/// Release Ninja Spell's orb by the charms' element, 1 to 4.
const CHARM_ORBS: [&str; 4] = [
    "data\\texture\\effect\\WaterOrb.bmp",
    "data\\texture\\effect\\StoneOrb.bmp",
    "data\\texture\\effect\\FireOrb.bmp",
    "data\\texture\\effect\\LightningOrb.bmp",
];

/// How far the lead goes an update.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Step {
    /// A 25th of the distance: the same time for any distance.
    Even,
    /// These many units, whatever the distance.
    Fixed(f32),
}

/// What tells one throw from another.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Params {
    /// The picture (its path in the GRF).
    pub icon: &'static str,
    /// How many rows of a lead and its ghosts (Kunai Splash's eight, one per charm), and how many quads in a row (4, or 1:
    /// the lead alone).
    pub rows: usize,
    pub quads: usize,
    /// Kunai Splash: the rows head 45° apart and the thrower spins while the first one shows.
    pub fan: bool,
    pub tint: [f32; 3],
    /// The quad's size: 1.5 is a 3 × 3 unit square.
    pub scale: f32,
    /// The lead's spin before its first update.
    pub spin: f32,
    pub step: Step,
    /// The arc's height over the flight.
    pub hop: f32,
    /// Updates before it leaves, unseen; they count in its length.
    pub delay: u32,
    /// Updates it lasts at most.
    pub length: u32,
    /// How high over the thrower's feet it starts.
    pub rise: f32,
    pub additive: bool,
}

const PINK: [f32; 3] = [1.0, 135.0 / 255.0, 135.0 / 255.0];
const GREEN: [f32; 3] = [135.0 / 255.0, 1.0, 135.0 / 255.0];
const NINJA: [f32; 3] = [1.0, 200.0 / 255.0, 1.0];

/// The pitchers' throw, which the others change.
const PITCHER: Params = Params {
    icon: "",
    rows: 1,
    quads: 4,
    fan: false,
    tint: PINK,
    scale: 1.5,
    spin: 0.0,
    step: Step::Even,
    hop: 30.0,
    delay: 0,
    length: 300,
    rise: 8.0,
    additive: false,
};
/// Throw Venom Knife's and the ninja throws'.
const KNIFE: Params = Params { tint: NINJA, scale: 2.0, spin: 330.0, step: Step::Fixed(3.0), hop: 1.0, length: 200, ..PITCHER };
const KUNAI: Params = Params { icon: "data\\texture\\유저인터페이스\\item\\쿠나이_독.bmp", scale: 3.0, step: Step::Fixed(1.5), ..KNIFE };

/// The effects thrown as items, with their parameters (Release Ninja Spell's picture and rows come from the charms).
pub const THROWS: [(u32, Params); 15] = [
    (299, Params { icon: "data\\texture\\유저인터페이스\\item\\빈포션병.bmp", ..PITCHER }),
    (541, Params { icon: "data\\texture\\유저인터페이스\\item\\버서크포션.bmp", ..PITCHER }),
    (914, Params { icon: "data\\texture\\유저인터페이스\\item\\폭탄버섯포자.bmp", ..PITCHER }),
    (298, Params { icon: "data\\texture\\유저인터페이스\\item\\빈병.bmp", tint: GREEN, ..PITCHER }),
    (308, Params { icon: "data\\texture\\유저인터페이스\\item\\돌.bmp", tint: [1.0; 3], scale: 1.0, step: Step::Fixed(2.0), hop: 10.0, ..PITCHER }),
    (FIRE_BOTTLE, Params { icon: "data\\texture\\유저인터페이스\\item\\화염병.bmp", tint: [1.0; 3], hop: 1.0, delay: 5, ..PITCHER }),
    (ACID_BOTTLE, Params { icon: "data\\texture\\유저인터페이스\\item\\염산병.bmp", tint: [1.0; 3], hop: 1.0, delay: 10, ..PITCHER }),
    (600, Params { icon: "data\\texture\\유저인터페이스\\item\\베넘나이프.bmp", ..KNIFE }),
    (613, Params { icon: "data\\texture\\유저인터페이스\\item\\수리검.bmp", ..KNIFE }),
    (614, Params { icon: KUNAI.icon, ..KNIFE }),
    (615, Params { icon: "data\\texture\\유저인터페이스\\item\\풍마_열화.bmp", ..KNIFE }),
    (616, Params { icon: "data\\texture\\effect\\coin_a.bmp", ..KNIFE }),
    (KUNAI_SPLASH, Params { rows: 8, fan: true, ..KUNAI }),
    (KUNAI_EXPLOSION, KUNAI),
    (CHARMS, Params { icon: CHARM_ORBS[0], quads: 1, tint: [1.0; 3], scale: 2.0, hop: 0.0, rise: 15.0, additive: true, ..KUNAI }),
];

/// An effect's throw.
pub fn params(effect: u32) -> Option<Params> {
    THROWS.iter().find(|(id, _)| *id == effect).map(|(_, params)| *params)
}

/// Release Ninja Spell's throw for charms of `element` (1 water, 2 earth, 3 fire, 4 wind), `count` of them: one orb a
/// charm, or none.
pub fn charms(element: u16, count: u16) -> Option<Params> {
    let icon = CHARM_ORBS.get(usize::from(element).checked_sub(1)?)?;
    (count > 0).then(|| Params { icon, rows: usize::from(count), ..params(CHARMS).expect("989 is a throw") })
}

/// Sine and cosine of a whole number of degrees, from f32 radians k/180·π.
fn trig(degrees: f32) -> (f32, f32) {
    (degrees / 180.0 * PI).sin_cos()
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Quad {
    at: Vector,
    spin: f32,
    alpha: f32,
    arc: f32,
}

/// A lead and its ghosts, on one heading.
#[derive(Clone, Debug, PartialEq)]
struct Row {
    heading: i32,
    quads: [Quad; 4],
}

pub struct Throw {
    /// The tick cursor: the time up to which ticks have been run.
    cursor: Instant,
    /// The thrower (None = the player) and the last place they were seen.
    unit: Option<u32>,
    last: Option<Vector>,
    params: Params,
    /// Ticks waited before leaving, and flight updates made.
    waited: u32,
    updates: u32,
    step: f32,
    climb: f32,
    steps: u32,
    rows: Vec<Row>,
    /// The sound made at the first flight update, at the thrower.
    sound: Option<String>,
}

impl Throw {
    /// A throw by `unit` from its feet `from` at the feet `to`. A throw at its own thrower aims a little behind it for a
    /// fixed step and isn't made for an even one.
    pub fn new(params: Params, unit: Option<u32>, from: Vector, to: Vector, now: Instant, sound: Option<String>) -> Option<Self> {
        let start = [from[0], from[1] + params.rise, from[2]];
        let mut aim = [to[0], to[1] + AIM_RISE, to[2]];
        if from == to {
            aim = [start[0] - BEHIND, start[1], start[2]];
        }
        let [dx, dz] = [aim[0] - start[0], aim[2] - start[2]];
        let d = (dx * dx + dz * dz).sqrt();
        let (step, steps) = match params.step {
            Step::Even if d == 0.0 || from == to => return None,
            Step::Even => (d / 25.0, (0..).take(1000).find(|&k| k as f32 * (d / 25.0) > d)?),
            // Too near or too far: it doesn't move.
            Step::Fixed(step) => (step, if d == 0.0 { 0 } else { (0..100).find(|&k| k as f32 * step > d).unwrap_or(0) }),
        };
        // Whole degrees rounded down, so the flight is up to 1° off the true bearing.
        let mut heading = if d == 0.0 { 0 } else { ((dx / d).clamp(-1.0, 1.0).acos() * 180.0 / PI).floor() as i32 };
        if dz < 0.0 {
            heading = 360 - heading;
        }
        heading = heading.rem_euclid(360);
        let quad = Quad { at: start, spin: params.spin, alpha: 255.0, arc: 0.0 };
        let rows = (0..params.rows)
            .map(|row| Row { heading: if params.fan { (heading + 45 * row as i32).rem_euclid(360) } else { heading }, quads: [quad; 4] })
            .collect();
        let climb = if d == 0.0 { 0.0 } else { step / d * (aim[1] - start[1]) };
        Some(Throw { cursor: now, unit, last: None, params, waited: 0, updates: 0, step, climb, steps, rows, sound })
    }

    /// One tick: waiting out the delay, or update a = `updates`: each lead spins, each ghost copies the one ahead, and
    /// they step, or fade once landed.
    fn update(&mut self) {
        if self.waited < self.params.delay {
            self.waited += 1;
            return;
        }
        let a = self.updates;
        let t = a as f32 / self.steps as f32;
        let landed = self.steps == 0 || t > 1.05;
        for row in &mut self.rows {
            let (sin, cos) = trig(row.heading as f32);
            let mut ahead = row.quads[0];
            for j in 0..self.params.quads {
                let mut quad = row.quads[j];
                if j == 0 {
                    quad.spin = (quad.spin + SPIN).rem_euclid(360.0);
                } else {
                    quad.alpha = ahead.alpha - GHOST_BELOW[j - 1];
                    (quad.at, quad.spin) = (ahead.at, ahead.spin);
                }
                if landed {
                    quad.alpha = (quad.alpha - LANDED_FADE).max(0.0);
                    quad.arc = 0.0;
                } else {
                    quad.at[0] += self.step * cos;
                    quad.at[1] += self.climb;
                    quad.at[2] += self.step * sin;
                    quad.arc = self.params.hop * trig((180.0 * t).floor()).0;
                }
                row.quads[j] = quad;
                ahead = quad;
            }
        }
        self.updates += 1;
    }

    /// Ticks run, the wait included.
    fn ticks(&self) -> u32 {
        self.waited + self.updates
    }

    /// Whether it still has ticks to run: within its length and its 40 drawn updates, and not every quad faded.
    fn alive(&self) -> bool {
        self.ticks() < self.params.length && self.updates < DRAWN && (self.updates == 0 || self.quads().any(|quad| quad.alpha > 0.0))
    }

    /// Its quads in use, row by row, the lead first.
    fn quads(&self) -> impl Iterator<Item = &Quad> {
        self.rows.iter().flat_map(|row| &row.quads[..self.params.quads])
    }

    /// Whether the thrower spins now: Kunai Splash's while its first kunai shows.
    fn spins(&self) -> bool {
        self.params.fan && self.updates > 0 && self.rows.first().is_some_and(|row| row.quads[0].alpha > 0.0)
    }
}

/// The throws in flight.
#[derive(Default)]
pub struct Throws {
    list: Vec<Throw>,
    /// Degrees each thrower (None = the player) has been turned and not yet asked about.
    turns: HashMap<Option<u32>, f32>,
}

impl Throws {
    pub fn throw(&mut self, throw: Throw) {
        self.list.push(throw);
    }

    /// Runs the throws up to `now`, a tick at a time (one update a tick): not while their thrower is 130 or more from the
    /// local player. The sounds made: the file and where.
    pub fn advance(&mut self, now: Instant, position_of: impl Fn(Option<u32>) -> Option<Vector>) -> Vec<(String, Vector)> {
        let mut sounds = Vec::new();
        for throw in &mut self.list {
            while throw.cursor <= now && throw.alive() {
                if let Some(at) = position_of(throw.unit) {
                    throw.last = Some(at);
                }
                let far = match (throw.last, position_of(None)) {
                    (Some(thrower), Some(player)) => (0..3).map(|k| (thrower[k] - player[k]).powi(2)).sum::<f32>().sqrt() >= FAR,
                    _ => false,
                };
                throw.cursor += TICK;
                if far {
                    continue;
                }
                if throw.updates == 0
                    && throw.waited == throw.params.delay
                    && let (Some(file), Some(at)) = (throw.sound.take(), throw.last)
                {
                    sounds.push((file, at));
                }
                throw.update();
                if throw.spins() {
                    *self.turns.entry(throw.unit).or_default() += FAN_TURN;
                }
            }
        }
        // Gone once its last quad faded, its 40 updates were drawn or its length ran out.
        self.list.retain(Throw::alive);
        sounds
    }

    /// How many degrees `unit` (None = the player) has been turned since this was last asked.
    pub fn turned(&mut self, unit: Option<u32>) -> f32 {
        self.turns.remove(&unit).unwrap_or(0.0)
    }

    /// The quads to draw, row by row, the lead first then its ghosts: standing in the world, tipped by the camera's pitch
    /// (degrees) and turned by their spins, tinted with their alpha. A throw shows from its first flight update.
    pub fn meshes<'a>(&self, texture: impl Fn(&str) -> Option<&'a crate::render::Texture>, pitch: f32) -> Vec<EffectMesh<'a>> {
        let mut meshes = Vec::new();
        for throw in self.list.iter().filter(|throw| throw.updates > 0) {
            let Some(texture) = texture(throw.params.icon) else { continue };
            let ratio = crate::world_fx::padded_ratio(texture.size);
            let [r, g, b] = throw.params.tint;
            for quad in throw.quads().filter(|quad| quad.alpha > 0.0) {
                let tint = [r, g, b, quad.alpha / 255.0];
                let [a, b, c, d] = corners(quad, pitch, throw.params.scale, ratio).map(|(position, uv)| MeshVertex { position, uv, tint });
                meshes.push((texture, vec![a, b, c, b, d, c], throw.params.additive));
            }
        }
        meshes
    }
}

/// A quad's four corners, with their texture coordinates, for a camera pitched `pitch` degrees: the game's latitude is the
/// pitch negated; the picture stands 1 unit behind its point and is upside down when face-on. The square is 2·`scale`
/// wide; a picture filling `ratio` of its power-of-two surface takes that much of it from the top left.
fn corners(quad: &Quad, pitch: f32, scale: f32, ratio: [f32; 2]) -> [(Vector, [f32; 2]); 4] {
    let (s, c) = (-pitch / 180.0 * PI).sin_cos();
    let (sigma_sin, sigma_cos) = (quad.spin / 180.0 * PI).sin_cos();
    let [right, bottom] = [-1.0 + 2.0 * ratio[0], 1.0 - 2.0 * ratio[1]];
    [((-1.0, 1.0), [0.0, 0.0]), ((right, 1.0), [1.0, 0.0]), ((-1.0, bottom), [0.0, 1.0]), ((right, bottom), [1.0, 1.0])].map(|((lx, ly), uv)| {
        let w = scale * ly * s + c;
        let position = [
            quad.at[0] + scale * lx * sigma_cos + w * sigma_sin,
            quad.at[1] + quad.arc + s - scale * ly * c,
            quad.at[2] - scale * lx * sigma_sin + w * sigma_cos,
        ];
        (position, uv)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn icon_of(effect: u32) -> Option<&'static str> {
        params(effect).map(|params| params.icon)
    }

    fn throw_at(dx: f32, dz: f32, rise: f32) -> Option<Throw> {
        Throw::new(params(299).unwrap(), None, [0.0; 3], [dx, rise, dz], Instant::now(), None)
    }

    fn thrown(effect: u32, d: f32) -> Throw {
        Throw::new(params(effect).unwrap(), None, [0.0; 3], [d, 0.0, 0.0], Instant::now(), None).unwrap()
    }

    /// The lead's flight updates until it lands: how many moves and how far.
    fn flight(mut throw: Throw) -> (u32, f32) {
        let mut moves = 0;
        let mut x = throw.rows[0].quads[0].at[0];
        while throw.alive() {
            throw.update();
            let now = throw.rows[0].quads[0].at[0];
            if now != x {
                moves += 1;
            }
            x = now;
        }
        (moves, x)
    }

    #[test]
    fn heads_along_the_whole_degree_and_takes_26_steps() {
        let heading = |dx: f32, dz: f32| throw_at(dx, dz, 0.0).unwrap().rows[0].heading;
        assert_eq!(
            [heading(30.0, 40.0), heading(30.0, -40.0), heading(0.0, 45.0), heading(-45.0, 0.0), heading(45.0, 45.0), heading(50.0, 0.0)],
            [53, 307, 90, 180, 45, 0]
        );
        let steps = |d: f32| throw_at(d, 0.0, 0.0).unwrap().steps;
        assert_eq!([steps(50.0), steps(15.0), steps(30.0)], [26, 25, 25]);
        assert!(throw_at(0.0, 0.0, 0.0).is_none(), "a throw at oneself isn't drawn");
    }

    #[test]
    fn follows_the_worked_trace() {
        // From the origin 8 up, the aim 50 along +x and 10 higher: step 2, 0.4 up a step.
        let mut throw = throw_at(50.0, 0.0, 10.0).unwrap();
        assert_eq!((throw.rows[0].heading, throw.steps, throw.step), (0, 26, 2.0));
        assert!((throw.climb - 0.4).abs() < 1e-6);
        let mut seen = Vec::new();
        for a in 0..=28 {
            throw.update();
            if [0, 1, 2, 12, 13, 24, 25, 26, 27, 28].contains(&a) {
                seen.push((a, throw.rows[0].quads));
            }
        }
        let near = |got: f32, want: f32| (got - want).abs() < 0.01;
        // The lead's x, arc and spin: 2(min(a, 27) + 1), 0 / 3.136 / 6.749 / 29.776 / 30 / 7.258 / 3.656 / 0 / −3.136 / 0.
        let arcs = [0.0, 3.136, 6.749, 29.776, 30.0, 7.258, 3.656, 0.0, -3.136, 0.0];
        let xs = [2.0, 4.0, 6.0, 26.0, 28.0, 50.0, 52.0, 54.0, 56.0, 56.0];
        let spins = [45.0, 90.0, 135.0, 225.0, 270.0, 45.0, 90.0, 135.0, 180.0, 225.0];
        for (index, (a, quads)) in seen.iter().enumerate() {
            assert!(near(quads[0].at[0], xs[index]) && near(quads[0].arc, arcs[index]) && quads[0].spin == spins[index], "a {a}: {:?}", quads[0]);
            assert!(near(quads[0].at[1], 8.0 + 0.4 * ((*a).min(27) + 1) as f32), "a {a}");
        }
        // Alphas 255, 105, 80, 55 while in flight, 235, 65, 20, 0 at a = 28; ghosts 1, 2, 3 steps ahead; landed on the lead's spot.
        assert_eq!(seen[0].1.map(|quad| quad.alpha), [255.0, 105.0, 80.0, 55.0]);
        assert_eq!([seen[0].1[1].at[0], seen[0].1[2].at[0], seen[0].1[3].at[0]], [4.0, 6.0, 8.0]);
        assert_eq!(seen[8].1.map(|quad| quad.at[0]), [56.0, 58.0, 60.0, 62.0]);
        assert_eq!(seen[9].1.map(|quad| quad.alpha), [235.0, 65.0, 20.0, 0.0]);
        assert!(seen[9].1.iter().all(|quad| quad.at[0] == 56.0));
        // The landed fade: the lead 235 at 28 … 15 at 39; ghost 1 65, 45, 25, 5, 0; all faded after 40 updates.
        let fade: Vec<[f32; 4]> = (29..=40)
            .map(|_| {
                throw.update();
                throw.rows[0].quads.map(|quad| quad.alpha)
            })
            .collect();
        assert_eq!(fade[0], [215.0, 45.0, 0.0, 0.0]);
        assert_eq!(fade[10][0], 15.0);
        assert_eq!(fade[11][0], 0.0);
        assert_eq!([fade[1][1], fade[2][1], fade[3][1], fade[4][1]], [25.0, 5.0, 0.0, 0.0]);
        assert!(!throw.alive());
    }

    #[test]
    fn stops_a_little_past_the_aim_at_s_25() {
        // D = 15: S = 25, landing from a = 27, 8 % of D past the aim; the arc −3.7 at a = 26.
        let mut throw = throw_at(15.0, 0.0, 0.0).unwrap();
        for _ in 0..=27 {
            throw.update();
        }
        assert!((throw.rows[0].quads[0].at[0] - 15.0 * 1.08).abs() < 0.05, "{}", throw.rows[0].quads[0].at[0]);
        assert_eq!(throw.rows[0].quads[0].arc, 0.0);
    }

    #[test]
    fn corners_stand_tipped_by_the_pitch() {
        let quad = Quad { at: [10.0, 20.0, 30.0], spin: 0.0, alpha: 255.0, arc: 0.0 };
        let corners = |quad: &Quad, pitch: f32| corners(quad, pitch, 1.5, [0.75, 0.75]);
        let [top_left, top_right, bottom_left, _] = corners(&quad, 45.0);
        // Top corners 1.77 below the point, the bottom ones by the same formula 0.18 below it; 2.25 wide.
        assert!((top_left.0[1] - 20.0 + 1.77).abs() < 0.01 && (top_right.0[1] - top_left.0[1]).abs() < 1e-6);
        assert!((bottom_left.0[1] - 20.0 + 0.177).abs() < 0.01);
        assert!((top_right.0[0] - top_left.0[0] - 2.25).abs() < 1e-4);
        // Turned by 90° the width lies along z; a different pitch moves the corners.
        let turned = corners(&Quad { spin: 90.0, ..quad }, 45.0);
        assert!((turned[0].0[2] - turned[1].0[2]).abs() - 2.25 < 1e-3 && (turned[0].0[0] - turned[1].0[0]).abs() < 1e-3);
        assert!((corners(&quad, 80.0)[0].0[1] - top_left.0[1]).abs() > 0.1);
        // A full surface at scale 2 is 4 wide; a 27-pixel orb on a 32-pixel one 3.375.
        let width = |scale: f32, ratio: f32| {
            let [left, right, ..] = super::corners(&quad, 45.0, scale, [ratio; 2]);
            right.0[0] - left.0[0]
        };
        assert!((width(2.0, 1.0) - 4.0).abs() < 1e-4 && (width(2.0, 0.84375) - 3.375).abs() < 1e-4);
    }

    #[test]
    fn freezes_far_from_the_player_and_flies_on_without_its_thrower() {
        let start = Instant::now();
        let near = |_: Option<u32>| Some([0.0; 3]);
        let far = |unit: Option<u32>| Some(if unit.is_some() { [200.0, 0.0, 0.0] } else { [0.0; 3] });
        let mut throws = Throws::default();
        throws.throw(Throw::new(params(914).unwrap(), Some(7), [0.0; 3], [50.0, 0.0, 0.0], start, Some("effect\\G스포어익스플로젼.wav".to_owned())).unwrap());
        // Far: no update, and the sound waits.
        assert!(throws.advance(start + TICK * 5, far).is_empty() && throws.list[0].updates == 0);
        // Near again: it runs and the sound comes once at update 0, at the thrower.
        let sounds = throws.advance(start + TICK * 8, near);
        assert_eq!(sounds.len(), 1);
        assert!(throws.list[0].updates > 0);
        assert!(throws.advance(start + TICK * 12, near).is_empty());
        // A vanished thrower: it flies on from where it was seen.
        let before = throws.list[0].updates;
        throws.advance(start + TICK * 14, |unit| if unit.is_some() { None } else { Some([0.0; 3]) });
        assert!(throws.list[0].updates > before);
        // Gone once faded.
        throws.advance(start + TICK * 60, near);
        assert!(throws.list.is_empty());
        assert!(icon_of(299).is_some() && icon_of(541).is_some() && icon_of(914).is_some() && icon_of(1).is_none());
    }

    #[test]
    fn each_throw_has_its_parameters() {
        let p = |id| params(id).unwrap();
        let icon = |id| p(id).icon.rsplit('\\').next().unwrap();
        assert_eq!(
            [298, 308, 539, 980, 600, 613, 614, 615, 616, 981, 983].map(icon),
            [
                "빈병.bmp",
                "돌.bmp",
                "화염병.bmp",
                "염산병.bmp",
                "베넘나이프.bmp",
                "수리검.bmp",
                "쿠나이_독.bmp",
                "풍마_열화.bmp",
                "coin_a.bmp",
                "쿠나이_독.bmp",
                "쿠나이_독.bmp"
            ]
        );
        assert!(p(616).icon.starts_with("data\\texture\\effect\\") && p(613).icon.starts_with("data\\texture\\유저인터페이스\\item\\"));
        let shape = |id| {
            let p = p(id);
            (p.rows, p.quads, p.scale, p.spin, p.step, p.hop, p.delay, p.length)
        };
        assert_eq!(shape(298), (1, 4, 1.5, 0.0, Step::Even, 30.0, 0, 300));
        assert_eq!(shape(308), (1, 4, 1.0, 0.0, Step::Fixed(2.0), 10.0, 0, 300));
        assert_eq!(shape(539), (1, 4, 1.5, 0.0, Step::Even, 1.0, 5, 300));
        assert_eq!(shape(980), (1, 4, 1.5, 0.0, Step::Even, 1.0, 10, 300));
        for id in [600, 613, 614, 615, 616] {
            assert_eq!(shape(id), (1, 4, 2.0, 330.0, Step::Fixed(3.0), 1.0, 0, 200), "{id}");
            assert_eq!(p(id).tint, [1.0, 200.0 / 255.0, 1.0]);
        }
        assert_eq!(shape(981), (8, 4, 3.0, 330.0, Step::Fixed(1.5), 1.0, 0, 200));
        assert_eq!(shape(983), (1, 4, 3.0, 330.0, Step::Fixed(1.5), 1.0, 0, 200));
        assert_eq!(shape(989), (1, 1, 2.0, 330.0, Step::Fixed(1.5), 0.0, 0, 200));
        assert_eq!([p(298).tint, p(308).tint, p(539).tint, p(989).tint], [[135.0 / 255.0, 1.0, 135.0 / 255.0], [1.0; 3], [1.0; 3], [1.0; 3]]);
        assert!(p(981).fan && !p(983).fan && p(989).additive && !p(613).additive && !p(298).additive);
        assert_eq!([p(298).rise, p(613).rise, p(989).rise], [8.0, 8.0, 15.0]);
        // The pitchers keep theirs.
        assert_eq!((shape(299), p(299).tint), ((1, 4, 1.5, 0.0, Step::Even, 30.0, 0, 300), PINK));
    }

    #[test]
    fn a_fixed_step_flies_at_a_constant_speed() {
        // A shuriken at 6 cells: S = 11, 12 moves, 36 units; a stone: S = 16, 17 moves, 34 units.
        let shuriken = thrown(613, 30.0);
        assert_eq!(shuriken.steps, 11);
        assert_eq!(flight(shuriken), (12, 36.0));
        let stone = thrown(308, 30.0);
        assert_eq!(stone.steps, 16);
        assert_eq!(flight(stone), (17, 34.0));
        // Too near or too far: S = 0, and it fades where it starts.
        for (d, height) in [(0.0, 5.0), (297.0, 0.0)] {
            let throw = Throw::new(params(613).unwrap(), None, [0.0; 3], [d, height, 0.0], Instant::now(), None).unwrap();
            assert_eq!(throw.steps, 0, "{d}");
            assert_eq!(flight(throw), (0, 0.0), "{d}");
        }
        // An even step stays the pitchers' 26 / 25.
        assert_eq!([thrown(298, 50.0).steps, thrown(298, 15.0).steps], [26, 25]);
    }

    #[test]
    fn spins_from_its_start_and_hops_by_its_height() {
        let mut throw = thrown(613, 30.0);
        let spins: Vec<f32> = (0..3)
            .map(|_| {
                throw.update();
                throw.rows[0].quads[0].spin
            })
            .collect();
        assert_eq!(spins, [15.0, 60.0, 105.0]);
        // At t = 0.5 (S = 26, 30 and 20) the arc is the hop.
        let hop_at_half = |mut throw: Throw| {
            for _ in 0..=throw.steps / 2 {
                throw.update();
            }
            throw.rows[0].quads[0].arc
        };
        let hops = [hop_at_half(thrown(299, 50.0)), hop_at_half(thrown(308, 59.0)), hop_at_half(thrown(613, 57.0))];
        assert!(hops.iter().zip([30.0, 10.0, 1.0]).all(|(got, want)| (got - want).abs() < 1e-4), "{hops:?}");
        let orbs = Throw::new(charms(2, 1).unwrap(), None, [0.0; 3], [60.0, 0.0, 0.0], Instant::now(), None).unwrap();
        assert_eq!(hop_at_half(orbs), 0.0);
    }

    #[test]
    fn the_bottles_wait_their_delay() {
        for (id, delay) in [(FIRE_BOTTLE, 5), (ACID_BOTTLE, 10)] {
            let start = Instant::now();
            let mut throws = Throws::default();
            throws.throw(Throw::new(params(id).unwrap(), None, [0.0; 3], [50.0, 0.0, 0.0], start, None).unwrap());
            // Ticks 0 … delay − 1: not moved, not drawn; the next one is its first flight update.
            throws.advance(start + TICK * (delay - 1), |_| Some([0.0; 3]));
            assert_eq!((throws.list[0].waited, throws.list[0].updates), (delay, 0), "{id}");
            assert!(throws.list[0].rows[0].quads[0].at[0] == 0.0);
            throws.advance(start + TICK * delay, |_| Some([0.0; 3]));
            assert_eq!(throws.list[0].updates, 1, "{id}: flies at tick {}", delay + 1);
            assert_eq!(throws.list[0].ticks(), delay + 1);
        }
    }

    #[test]
    fn kunai_splash_fans_out_and_spins_its_thrower() {
        let start = Instant::now();
        let throw = Throw::new(params(KUNAI_SPLASH).unwrap(), Some(3), [0.0; 3], [30.0, 0.0, 0.0], start, None).unwrap();
        assert_eq!(throw.rows.iter().map(|row| row.heading).collect::<Vec<_>>(), [0, 45, 90, 135, 180, 225, 270, 315]);
        assert_eq!(throw.quads().count(), 32);
        // Cast on itself: 8 behind, heading 180, S = 6, 7 moves of 1.5.
        let own = Throw::new(params(KUNAI_SPLASH).unwrap(), Some(3), [10.0, 0.0, 20.0], [10.0, 0.0, 20.0], start, None).unwrap();
        assert_eq!((own.rows[0].heading, own.steps), (180, 6));
        let (moves, x) = flight(own);
        assert_eq!(moves, 7);
        assert!((x - (10.0 - 10.5)).abs() < 1e-4, "{x}");
        // The thrower turns 45° a tick while the first kunai shows, then stops.
        let mut throws = Throws::default();
        throws.throw(throw);
        throws.advance(start + TICK * 2, |_| Some([0.0; 3]));
        assert_eq!(throws.turned(Some(3)), 135.0);
        assert_eq!((throws.turned(Some(3)), throws.turned(None)), (0.0, 0.0));
        throws.advance(start + TICK * 199, |_| Some([0.0; 3]));
        let turned = throws.turned(Some(3));
        assert!(throws.list.is_empty());
        throws.advance(start + TICK * 220, |_| Some([0.0; 3]));
        assert_eq!(throws.turned(Some(3)), 0.0);
        // S = 21: 23 moves, then 12 fading updates with alpha left; 3 of those 35 ticks were asked about already.
        assert_eq!(turned, 45.0 * (35 - 3) as f32);
    }

    #[test]
    fn kunai_explosion_and_the_charms() {
        assert_eq!(icon_of(KUNAI_EXPLOSION), Some("data\\texture\\유저인터페이스\\item\\쿠나이_독.bmp"));
        let stones = charms(2, 3).unwrap();
        assert_eq!((stones.icon, stones.rows, stones.quads, stones.additive, stones.hop), ("data\\texture\\effect\\StoneOrb.bmp", 3, 1, true, 0.0,));
        assert_eq!([1, 3, 4].map(|element| charms(element, 1).unwrap().icon.rsplit('\\').next().unwrap()), ["WaterOrb.bmp", "FireOrb.bmp", "LightningOrb.bmp"]);
        assert!(charms(2, 0).is_none() && charms(5, 3).is_none() && charms(0, 3).is_none());
        // Three rows of one quad each, from 15 up, on top of each other.
        let mut throw = Throw::new(stones, None, [0.0; 3], [30.0, 0.0, 0.0], Instant::now(), None).unwrap();
        throw.update();
        assert_eq!(throw.quads().count(), 3);
        assert!(throw.rows.iter().all(|row| row.quads[0] == throw.rows[0].quads[0]));
        assert!((throw.rows[0].quads[0].at[1] - (15.0 - 1.5 / 30.0 * 7.0)).abs() < 1e-5);
    }
}
