//! The item throw (Potion Pitcher 299, Berserk Pitcher 541, Spore Explosion 914): the item's own picture, four copies of
//! it standing in the world as flat quads, spins and flies from the thrower to the target over 26 updates, goes on a
//! little past it and fades.

use std::time::Instant;

use super::TICK;
use crate::world::{EffectMesh, MeshVertex, Vector};

/// The game's π as its trig tables use it.
#[allow(clippy::approx_constant)]
const PI: f32 = 3.141_592;
/// How high over the feet it starts and aims.
const RISE: f32 = 8.0;
/// The throw has 300 updates, drawn for the first 40 (the rest unseen); it freezes while its thrower is this far from the player.
const DRAWN: u32 = 40;
const FAR: f32 = 130.0;
/// The lead spins 45° an update; the first ghost's alpha is 150 less than the lead's, the others' 25 less than the one ahead.
const SPIN: f32 = 45.0;
const GHOST_BELOW: [f32; 3] = [150.0, 25.0, 25.0];
/// Once landed an alpha falls 20 an update; the arc is 30 high over the flight.
const LANDED_FADE: f32 = 20.0;
const ARC: f32 = 30.0;
/// The picture's colour: pink.
const TINT: [f32; 3] = [1.0, 135.0 / 255.0, 135.0 / 255.0];

/// The effect ids this makes and the item pictures they throw.
pub const ICONS: [(u32, &str); 3] = [
    (299, "data\\texture\\유저인터페이스\\item\\빈포션병.bmp"),
    (541, "data\\texture\\유저인터페이스\\item\\버서크포션.bmp"),
    (914, "data\\texture\\유저인터페이스\\item\\폭탄버섯포자.bmp"),
];

/// The picture an effect throws.
pub fn icon_of(effect: u32) -> Option<&'static str> {
    ICONS.iter().find(|(id, _)| *id == effect).map(|(_, path)| *path)
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

pub struct Throw {
    /// The tick cursor: the time up to which ticks have been run.
    cursor: Instant,
    /// The thrower (None = the player) and the last place they were seen.
    unit: Option<u32>,
    last: Option<Vector>,
    icon: &'static str,
    /// Updates made.
    updates: u32,
    step: f32,
    climb: f32,
    steps: u32,
    heading: i32,
    quads: [Quad; 4],
    /// 914's sound, made at update 0 at the thrower.
    sound: Option<String>,
}

impl Throw {
    /// A throw by `unit` from its feet `from` at the feet `to`, or none when they are on one spot.
    pub fn new(effect: u32, unit: Option<u32>, from: Vector, to: Vector, now: Instant, sound: Option<String>) -> Option<Self> {
        let start = [from[0], from[1] + RISE, from[2]];
        let aim = [to[0], to[1] + RISE, to[2]];
        let [dx, dz] = [aim[0] - start[0], aim[2] - start[2]];
        let d = (dx * dx + dz * dz).sqrt();
        if d == 0.0 {
            return None;
        }
        let rise = aim[1] - start[1];
        // Whole degrees rounded down, so the flight is up to 1° off the true bearing.
        let mut heading = ((dx / d).clamp(-1.0, 1.0).acos() * 180.0 / PI).floor() as i32;
        if dz < 0.0 {
            heading = 360 - heading;
        }
        heading = heading.rem_euclid(360);
        let step = d / 25.0;
        let steps = (0..).take(1000).find(|&k| k as f32 * step > d)?;
        let quad = Quad { at: start, spin: 0.0, alpha: 255.0, arc: 0.0 };
        Some(Throw { cursor: now, unit, last: None, icon: icon_of(effect)?, updates: 0, step, climb: step / d * rise, steps, heading, quads: [quad; 4], sound })
    }

    /// Update a = `updates`: the lead spins, each ghost copies the one ahead and steps, or fades once landed.
    fn update(&mut self) {
        let a = self.updates;
        let t = a as f32 / self.steps as f32;
        let (sin, cos) = trig(self.heading as f32);
        let mut ahead = self.quads[0];
        for j in 0..4 {
            let mut quad = self.quads[j];
            if j == 0 {
                quad.spin = (quad.spin + SPIN).rem_euclid(360.0);
            } else {
                quad.alpha = ahead.alpha - GHOST_BELOW[j - 1];
                (quad.at, quad.spin) = (ahead.at, ahead.spin);
            }
            if t > 1.05 {
                quad.alpha = (quad.alpha - LANDED_FADE).max(0.0);
                quad.arc = 0.0;
            } else {
                quad.at[0] += self.step * cos;
                quad.at[1] += self.climb;
                quad.at[2] += self.step * sin;
                quad.arc = ARC * trig((180.0 * t).floor()).0;
            }
            self.quads[j] = quad;
            ahead = quad;
        }
        self.updates += 1;
    }
}

/// The throws in flight.
#[derive(Default)]
pub struct Throws {
    list: Vec<Throw>,
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
            while throw.cursor <= now && throw.updates < DRAWN {
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
                    && let (Some(file), Some(at)) = (throw.sound.take(), throw.last)
                {
                    sounds.push((file, at));
                }
                throw.update();
            }
        }
        // Gone the tick after its 40th update is drawn.
        self.list.retain(|throw| throw.updates < DRAWN || throw.cursor > now);
        sounds
    }

    /// The quads to draw, the lead first then its ghosts: standing in the world, tipped by the camera's pitch (degrees)
    /// and turned by their spins, tinted pink with their alpha.
    pub fn meshes<'a>(&self, texture: impl Fn(&str) -> Option<&'a crate::render::Texture>, pitch: f32) -> Vec<EffectMesh<'a>> {
        let mut meshes = Vec::new();
        for throw in self.list.iter().filter(|throw| throw.updates > 0 && throw.updates <= DRAWN) {
            let Some(texture) = texture(throw.icon) else { continue };
            for quad in throw.quads.iter().filter(|quad| quad.alpha > 0.0) {
                let tint = [TINT[0], TINT[1], TINT[2], quad.alpha / 255.0];
                let [a, b, c, d] = corners(quad, pitch).map(|(position, uv)| MeshVertex { position, uv, tint });
                meshes.push((texture, vec![a, b, c, b, d, c], false));
            }
        }
        meshes
    }
}

/// A quad's four corners, with their texture coordinates, for a camera pitched `pitch` degrees: the game's latitude is the
/// pitch negated; the picture stands 1 unit behind its point and is upside down when face-on.
fn corners(quad: &Quad, pitch: f32) -> [(Vector, [f32; 2]); 4] {
    let (s, c) = (-pitch / 180.0 * PI).sin_cos();
    let (sigma_sin, sigma_cos) = (quad.spin / 180.0 * PI).sin_cos();
    [((-1.0, 1.0), [0.0, 0.0]), ((0.5, 1.0), [1.0, 0.0]), ((-1.0, -0.5), [0.0, 1.0]), ((0.5, -0.5), [1.0, 1.0])].map(|((lx, ly), uv)| {
        let w = 1.5 * ly * s + c;
        let position =
            [quad.at[0] + 1.5 * lx * sigma_cos + w * sigma_sin, quad.at[1] + quad.arc + s - 1.5 * ly * c, quad.at[2] - 1.5 * lx * sigma_sin + w * sigma_cos];
        (position, uv)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn throw_at(dx: f32, dz: f32, rise: f32) -> Option<Throw> {
        Throw::new(299, None, [0.0; 3], [dx, rise, dz], Instant::now(), None)
    }

    #[test]
    fn heads_along_the_whole_degree_and_takes_26_steps() {
        let heading = |dx: f32, dz: f32| throw_at(dx, dz, 0.0).unwrap().heading;
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
        assert_eq!((throw.heading, throw.steps, throw.step), (0, 26, 2.0));
        assert!((throw.climb - 0.4).abs() < 1e-6);
        let mut seen = Vec::new();
        for a in 0..=28 {
            throw.update();
            if [0, 1, 2, 12, 13, 24, 25, 26, 27, 28].contains(&a) {
                seen.push((a, throw.quads));
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
        // The landed fade: the lead 235 at 28 … 15 at 39; ghost 1 65, 45, 25, 5, 0; drawn for 40 updates.
        let fade: Vec<[f32; 4]> = (29..=40)
            .map(|_| {
                throw.update();
                throw.quads.map(|quad| quad.alpha)
            })
            .collect();
        assert_eq!(fade[0], [215.0, 45.0, 0.0, 0.0]);
        assert_eq!(fade[10][0], 15.0);
        assert_eq!(fade[11][0], 0.0);
        assert_eq!([fade[1][1], fade[2][1], fade[3][1], fade[4][1]], [25.0, 5.0, 0.0, 0.0]);
    }

    #[test]
    fn stops_a_little_past_the_aim_at_s_25() {
        // D = 15: S = 25, landing from a = 27, 8 % of D past the aim; the arc −3.7 at a = 26.
        let mut throw = throw_at(15.0, 0.0, 0.0).unwrap();
        for _ in 0..=27 {
            throw.update();
        }
        assert!((throw.quads[0].at[0] - 15.0 * 1.08).abs() < 0.05, "{}", throw.quads[0].at[0]);
        assert_eq!(throw.quads[0].arc, 0.0);
    }

    #[test]
    fn corners_stand_tipped_by_the_pitch() {
        let quad = Quad { at: [10.0, 20.0, 30.0], spin: 0.0, alpha: 255.0, arc: 0.0 };
        let [top_left, top_right, bottom_left, _] = corners(&quad, 45.0);
        // Top corners 1.77 below the point, the bottom ones by the same formula 0.18 below it; 2.25 wide.
        assert!((top_left.0[1] - 20.0 + 1.77).abs() < 0.01 && (top_right.0[1] - top_left.0[1]).abs() < 1e-6);
        assert!((bottom_left.0[1] - 20.0 + 0.177).abs() < 0.01);
        assert!((top_right.0[0] - top_left.0[0] - 2.25).abs() < 1e-4);
        // Turned by 90° the width lies along z; a different pitch moves the corners.
        let turned = corners(&Quad { spin: 90.0, ..quad }, 45.0);
        assert!((turned[0].0[2] - turned[1].0[2]).abs() - 2.25 < 1e-3 && (turned[0].0[0] - turned[1].0[0]).abs() < 1e-3);
        assert!((corners(&quad, 80.0)[0].0[1] - top_left.0[1]).abs() > 0.1);
    }

    #[test]
    fn freezes_far_from_the_player_and_flies_on_without_its_thrower() {
        let start = Instant::now();
        let near = |_: Option<u32>| Some([0.0; 3]);
        let far = |unit: Option<u32>| Some(if unit.is_some() { [200.0, 0.0, 0.0] } else { [0.0; 3] });
        let mut throws = Throws::default();
        throws.throw(Throw::new(914, Some(7), [0.0; 3], [50.0, 0.0, 0.0], start, Some("effect\\G스포어익스플로젼.wav".to_owned())).unwrap());
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
        // Drawn for 40 updates, then gone.
        throws.advance(start + TICK * 60, near);
        assert!(throws.list.is_empty());
        assert!(icon_of(299).is_some() && icon_of(541).is_some() && icon_of(914).is_some() && icon_of(1).is_none());
    }
}
