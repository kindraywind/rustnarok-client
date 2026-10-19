//! What flies from where the caster stood to where the target stood, straight across the ground at an even speed:
//! Fire Ball's balls (EF_FIREBALL 24), Jupitel Thunder's ball (EF_YUFITEL 93) and Spear Boomerang's spears (EF_SPEARBMR 80).

use std::collections::HashMap;
use std::time::Instant;

use super::{TICK, passes};
use crate::actor::{Placed, SpriteSet};
use crate::effects::QUAD_UV;
use crate::render::Texture;
use crate::world::{self, EffectQuad, Vector};

/// Fire Ball's ball (under data\sprite\이팩트\) and how big it's drawn.
pub const FIRE_BALL_SPRITE: &str = "data\\sprite\\이팩트\\fireball";
const FIRE_BALL_SCALE: f32 = 1.3;
/// When Fire Ball lets its four balls out (ticks); each flies 20 ticks, 15 units up.
pub const FIRE_BALL_TICKS: [u32; 4] = [20, 24, 28, 32];
const FIRE_BALL_LIFE: u32 = 20;
const FIRE_BALL_RISE: f32 = 15.0;
/// The lead ball as it is, the three after it gold and fainter: color and alpha (0–255).
const FIRE_BALL_LOOKS: [[f32; 4]; 4] = [[255.0, 255.0, 255.0, 254.0], [246.0, 199.0, 76.0, 180.0], [246.0, 199.0, 76.0, 130.0], [246.0, 199.0, 76.0, 80.0]];
/// A fire ball goes before it's drawn once this near where it's aimed, across the ground (world units).
const FIRE_BALL_NEAR: f32 = 3.0;

/// Spear Boomerang's spear (under data\sprite\이팩트\), four of them at ticks 0, 4, 8 and 12, each flying 15 ticks, 15
/// units up, fainter than the last: alpha (0–255).
pub const SPEAR_SPRITE: &str = "data\\sprite\\이팩트\\창";
pub const SPEAR_TICKS: [u32; 4] = [0, 4, 8, 12];
const SPEAR_LIFE: u32 = 15;
const SPEAR_RISE: f32 = 15.0;
const SPEAR_ALPHAS: [f32; 4] = [254.0, 180.0, 130.0, 80.0];

/// Jupitel Thunder's ball: a core and a ball over it cycling six textures a tick each, added on, 5 units up, flying
/// 20 ticks; half the side of each square in world units.
// ponytail: a texture a tick, squares facing the camera and flying the whole 20 ticks are read from the game's code,
// not seen in it; what it does once within 3 units of the target isn't known.
pub const THUNDER_CORE: &str = "data\\texture\\effect\\thunder_center.bmp";
pub const THUNDER_BALL: [&str; 6] = [
    "data\\texture\\effect\\thunder_ball_a.bmp",
    "data\\texture\\effect\\thunder_ball_b.bmp",
    "data\\texture\\effect\\thunder_ball_c.bmp",
    "data\\texture\\effect\\thunder_ball_d.bmp",
    "data\\texture\\effect\\thunder_ball_e.bmp",
    "data\\texture\\effect\\thunder_ball_f.bmp",
];
const THUNDER_LIFE: u32 = 20;
const THUNDER_RISE: f32 = 5.0;
const THUNDER_SIZES: [f32; 2] = [3.5, 4.5];

/// A fire ball's turn on the screen in whole degrees: 180 − the true bearing (+ 360 when negative) less the camera's
/// turn, truncated, a negative one looked up a degree up.
fn fire_ball_turn(bearing: f32, yaw: f32) -> f32 {
    let base = 180.0 - bearing;
    let turn = ((if base < 0.0 { base + 360.0 } else { base }) - yaw).trunc();
    if turn < 0.0 { turn + 1.0 } else { turn }
}

/// A spear's turn on the screen, in whole degrees: 180 + (180 − its heading) − the camera's turn, so it points along its
/// flight; a negative angle is looked up a degree up.
fn spear_turn(heading: f32, yaw: f32) -> f32 {
    let turn = (360.0 - heading - yaw).trunc();
    if turn < 0.0 { turn + 1.0 } else { turn }
}

/// A flight: leaving over where the caster stood at `born`, a step a tick across the ground so as to be over where
/// it's aimed after its life.
struct Flight {
    born: Instant,
    /// Ticks run.
    ticks: u32,
    from: Vector,
    step: [f32; 2],
    aim: [f32; 2],
    /// The true bearing to the aim (as `actor::step_rot`), which a fire ball's turn on the screen keeps.
    bearing: f32,
}

impl Flight {
    // ponytail: Jupitel Thunder's ball goes straight at the target; the game's turn truncates to whole degrees, 1–2° off.
    fn new(born: Instant, from: Vector, rise: f32, to: Vector, life: u32) -> Self {
        let step = [(to[0] - from[0]) / life as f32, (to[2] - from[2]) / life as f32];
        let bearing = crate::actor::step_rot(to[0] - from[0], to[2] - from[2]);
        Self { born, ticks: 0, from: [from[0], from[1] + rise, from[2]], step, aim: [to[0], to[2]], bearing }
    }

    /// Like `new`, but along the whole degree before the heading's own (up to 2° off the line, as the game's fire balls
    /// and spears fly), the same distance: heading 0 toward −z, 90 toward +x.
    fn skewed(born: Instant, from: Vector, rise: f32, to: Vector, life: u32) -> Self {
        let [dx, dz] = [to[0] - from[0], to[2] - from[2]];
        let heading = crate::actor::step_rot(dx, dz).trunc();
        let (sin, cos) = (if heading == 0.0 { 0.0 } else { heading - 1.0 }).to_radians().sin_cos();
        let each = f32::hypot(dx, dz) / life as f32;
        Self { step: [each * sin, -each * cos], ..Self::new(born, from, rise, to, life) }
    }

    /// Where it is: a step on for each tick run.
    fn at(&self) -> Vector {
        let steps = self.ticks as f32;
        [self.from[0] + self.step[0] * steps, self.from[1], self.from[2] + self.step[1] * steps]
    }

    fn near(&self) -> bool {
        let at = self.at();
        f32::hypot(at[0] - self.aim[0], at[2] - self.aim[1]) <= FIRE_BALL_NEAR
    }
}

/// A Fire Ball letting its balls out.
struct FireBall {
    start: Instant,
    /// Ticks run.
    ticks: u32,
    from: Vector,
    to: Vector,
}

/// One of Fire Ball's balls (the game's prim kind 5): which of the four, and whether it got there.
struct Ball {
    flight: Flight,
    look: usize,
    gone: bool,
}

/// Fire Balls, their balls in flight, and Jupitel Thunder's balls (the game's prim kind 15, a core and a ball
/// moving together).
#[derive(Default)]
pub struct Balls {
    fire_balls: Vec<FireBall>,
    balls: Vec<Ball>,
    thunder: Vec<Flight>,
    spear_boomerangs: Vec<FireBall>,
    spears: Vec<Ball>,
}

impl Balls {
    /// Fire Ball from `from` at `to` from `now`: four balls at ticks 20, 24, 28 and 32 (320–512 ms), each reaching
    /// over the target 20 ticks later.
    pub fn fire_ball(&mut self, from: Vector, to: Vector, now: Instant) {
        self.fire_balls.push(FireBall { start: now, ticks: 0, from, to });
    }

    /// Jupitel Thunder from `from` at `to` from `now`: one ball, over the target 20 ticks (320 ms) later.
    pub fn thunder(&mut self, from: Vector, to: Vector, now: Instant) {
        self.thunder.push(Flight::new(now, from, THUNDER_RISE, to, THUNDER_LIFE));
    }

    /// Spear Boomerang from `from` at `to` from `now`: four spears at ticks 0, 4, 8 and 12, each reaching over the target
    /// 15 ticks later, or gone once within 3 units of it.
    pub fn spear_boomerang(&mut self, from: Vector, to: Vector, now: Instant) {
        self.spear_boomerangs.push(FireBall { start: now, ticks: 0, from, to });
    }

    /// Runs everything up to `now`, a tick at a time.
    pub fn advance(&mut self, now: Instant) {
        for launch in &mut self.spear_boomerangs {
            let due = passes(launch.start, now);
            for age in launch.ticks..due {
                if let Some(look) = SPEAR_TICKS.iter().position(|&tick| tick == age) {
                    let flight = Flight::skewed(launch.start + TICK * age, launch.from, SPEAR_RISE, launch.to, SPEAR_LIFE);
                    self.spears.push(Ball { flight, look, gone: false });
                }
            }
            launch.ticks = launch.ticks.max(due);
        }
        self.spear_boomerangs.retain(|launch| launch.ticks <= SPEAR_TICKS[3]);
        for spear in &mut self.spears {
            while !spear.gone && spear.flight.ticks < passes(spear.flight.born, now).min(SPEAR_LIFE) {
                spear.flight.ticks += 1;
                spear.gone = spear.flight.near();
            }
        }
        self.spears.retain(|spear| !spear.gone && passes(spear.flight.born, now) <= SPEAR_LIFE);
        for fire_ball in &mut self.fire_balls {
            let due = passes(fire_ball.start, now);
            for age in fire_ball.ticks..due {
                if let Some(look) = FIRE_BALL_TICKS.iter().position(|&tick| tick == age) {
                    let flight = Flight::skewed(fire_ball.start + TICK * age, fire_ball.from, FIRE_BALL_RISE, fire_ball.to, FIRE_BALL_LIFE);
                    self.balls.push(Ball { flight, look, gone: false });
                }
            }
            fire_ball.ticks = fire_ball.ticks.max(due);
        }
        self.fire_balls.retain(|fire_ball| fire_ball.ticks <= FIRE_BALL_TICKS[3]);
        for ball in &mut self.balls {
            while !ball.gone && ball.flight.ticks < passes(ball.flight.born, now).min(FIRE_BALL_LIFE) {
                ball.flight.ticks += 1;
                ball.gone = ball.flight.near();
            }
        }
        self.balls.retain(|ball| !ball.gone && passes(ball.flight.born, now) <= FIRE_BALL_LIFE);
        for flight in &mut self.thunder {
            flight.ticks = flight.ticks.max(passes(flight.born, now).min(THUNDER_LIFE));
        }
        self.thunder.retain(|flight| passes(flight.born, now) <= THUNDER_LIFE);
    }

    /// The quads showing. Fire balls are `sprite` at its `frames` (one a tick), turned so their top leads on a screen
    /// whose camera is turned `yaw` degrees; Jupitel's core and ball are squares facing the camera, added on.
    pub fn quads<'a>(
        &self,
        sprite: Option<&'a SpriteSet>,
        frames: &[Vec<Placed>],
        spear: (Option<&'a SpriteSet>, &[Placed]),
        textures: &'a HashMap<String, Option<Texture>>,
        yaw: f32,
    ) -> Vec<EffectQuad<'a>> {
        let mut quads = Vec::new();
        if let (Some(sprite), frame) = spear {
            for spear in self.spears.iter().filter(|spear| spear.flight.ticks > 0) {
                let [x, z] = spear.flight.step;
                let turn = spear_turn(crate::actor::step_rot(x, z), yaw);
                let alpha = SPEAR_ALPHAS[spear.look] / 255.0;
                for &(key, position, size, angle) in frame {
                    let corners = QUAD_UV.map(|uv| world::sprite_corner(position, size, angle + turn, uv).0);
                    quads.push((sprite.texture(key), spear.flight.at(), corners, [1.0, 1.0, 1.0, alpha], false, false));
                }
            }
        }
        if let Some(sprite) = sprite.filter(|_| !frames.is_empty()) {
            for ball in self.balls.iter().filter(|ball| ball.flight.ticks > 0) {
                let turn = fire_ball_turn(ball.flight.bearing, yaw);
                let [red, green, blue, alpha] = FIRE_BALL_LOOKS[ball.look].map(|value| value / 255.0);
                for &(key, position, size, angle) in &frames[ball.flight.ticks as usize % frames.len()] {
                    let corners = QUAD_UV.map(|uv| world::sprite_corner(position, size, angle + turn, uv).0.map(|value| value * FIRE_BALL_SCALE));
                    quads.push((sprite.texture(key), ball.flight.at(), corners, [red, green, blue, alpha], false, false));
                }
            }
        }
        // ponytail: the core's alpha 170 does nothing as the game adds it on (one to one); both draw at full strength.
        let pixels = 35.0 / world::CELL;
        for flight in self.thunder.iter().filter(|flight| flight.ticks > 0) {
            let ball = THUNDER_BALL[flight.ticks as usize % THUNDER_BALL.len()];
            for (path, size) in [THUNDER_CORE, ball].into_iter().zip(THUNDER_SIZES) {
                let Some(Some(texture)) = textures.get(path) else { continue };
                let half = size * pixels;
                quads.push((texture, flight.at(), [[-half, -half], [half, -half], [half, half], [-half, half]], [1.0; 4], true, false));
            }
        }
        quads
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fire_ball_lets_four_balls_out_that_reach_the_target() {
        let start = Instant::now();
        let mut balls = Balls::default();
        // Due north (+z): heading 179, 1° off, so the ball drifts a little east.
        balls.fire_ball([0.0; 3], [0.0, 0.0, 45.0], start);
        // Ticks 20, 24, 28 and 32.
        balls.advance(start + TICK * 19);
        assert!(balls.balls.is_empty());
        balls.advance(start + TICK * 32);
        assert_eq!(balls.balls.iter().map(|ball| ball.look).collect::<Vec<_>>(), [0, 1, 2, 3]);
        assert!(balls.fire_balls.is_empty());
        // 15 units up, 2.25 units a tick along 179°: 9 cells away a ball is drawn 18 ticks, then goes within 3 units of it.
        let lead = &balls.balls[0];
        let (sin, cos) = 179.0_f32.to_radians().sin_cos();
        let at = lead.flight.at();
        assert_eq!(lead.flight.ticks, 13);
        assert!((at[0] - 13.0 * 2.25 * sin).abs() < 1e-3 && (at[2] + 13.0 * 2.25 * cos).abs() < 1e-3 && at[1] == 15.0, "{at:?}");
        balls.advance(start + TICK * (20 + 17));
        assert_eq!(balls.balls[0].flight.ticks, 18);
        balls.advance(start + TICK * (20 + 18));
        assert_eq!(balls.balls[0].look, 1);
        balls.advance(start + TICK * (32 + 18));
        assert!(balls.balls.is_empty());
        // Nothing flies at a target on the caster.
        balls.fire_ball([0.0; 3], [0.0, 0.0, 3.0], start);
        balls.advance(start + TICK * 40);
        assert!(balls.balls.is_empty());
        // Far off (200 units due north) a ball never comes within 3 of the aim: it shows all 20 ticks and ends 3.49 east of it.
        balls.fire_ball([0.0; 3], [0.0, 0.0, 200.0], start);
        balls.advance(start + TICK * 39);
        let far = &balls.balls[0];
        assert_eq!(far.flight.ticks, 20);
        assert!((far.flight.at()[0] - 3.49).abs() < 0.01, "{:?}", far.flight.at());
    }

    #[test]
    fn a_fire_ball_turns_by_the_true_bearing() {
        // The heading is the whole degree before the bearing's, 0 below 1°.
        let heading = |dx: f32, dz: f32| {
            let flight = Flight::skewed(Instant::now(), [0.0; 3], 0.0, [dx, 0.0, dz], 10);
            flight.step[0].atan2(-flight.step[1]).to_degrees().rem_euclid(360.0)
        };
        assert!((heading(0.0, 10.0) - 179.0).abs() < 1e-3);
        assert!(heading(0.0087, -10.0).abs() < 1e-3, "0.5° gives 0");
        // The screen turn: 180 − bearing (+360 when negative) less the camera, whole degrees, a negative one a degree up.
        assert_eq!([fire_ball_turn(180.0, 0.0), fire_ball_turn(90.5, 10.0), fire_ball_turn(270.0, 30.0)], [0.0, 79.0, 240.0]);
        assert_eq!(fire_ball_turn(180.0, 45.0), -44.0);
    }

    #[test]
    fn spear_boomerang_throws_four_spears_that_end_near_the_target() {
        let start = Instant::now();
        let shown = |to: [f32; 3]| {
            let mut balls = Balls::default();
            balls.spear_boomerang([0.0; 3], to, start);
            balls.advance(start + TICK * 12);
            let looks: Vec<usize> = balls.spears.iter().map(|spear| spear.look).collect();
            balls.advance(start + TICK * 40);
            (looks, balls.spears.len())
        };
        // Four spears at ticks 0, 4, 8 and 12, their alphas fading.
        assert_eq!(shown([0.0, 0.0, 60.0]).0, [0, 1, 2, 3]);
        assert_eq!(SPEAR_ALPHAS, [254.0, 180.0, 130.0, 80.0]);
        // A spear of a target due +z (heading 180, flown along 179): 15 steps of D / 15, 15 up.
        let mut balls = Balls::default();
        balls.spear_boomerang([0.0; 3], [0.0, 0.0, 60.0], start);
        balls.advance(start);
        let at = balls.spears[0].flight.at();
        let (sin, cos) = 179.0_f32.to_radians().sin_cos();
        assert!((at[0] - 4.0 * sin).abs() < 1e-3 && (at[2] + 4.0 * cos).abs() < 1e-3 && at[1] == 15.0, "{at:?}");
        // How many ticks a spear is drawn: D 30 ends early (within 3 across the ground), D 60 shows 14 of its 15, D 200 all 15.
        let drawn = |distance: f32| {
            let mut flight = Flight::skewed(start, [0.0; 3], SPEAR_RISE, [0.0, 0.0, distance], SPEAR_LIFE);
            let mut shown = 0;
            while flight.ticks < SPEAR_LIFE {
                flight.ticks += 1;
                if flight.near() {
                    break;
                }
                shown += 1;
            }
            shown
        };
        assert_eq!([drawn(30.0), drawn(60.0), drawn(200.0)], [13, 14, 15]);
        // The turn on the screen: 360 − heading − camera, whole degrees, negative ones a degree up.
        assert_eq!([spear_turn(180.0, 0.0), spear_turn(179.0, 30.0), spear_turn(90.0, 450.0)], [180.0, 151.0, -179.0]);
    }

    #[test]
    fn jupitel_thunder_flies_one_ball_20_ticks() {
        let start = Instant::now();
        let mut balls = Balls::default();
        balls.thunder([0.0; 3], [40.0, 0.0, 0.0], start);
        balls.advance(start);
        assert_eq!(balls.thunder[0].at(), [2.0, 5.0, 0.0]);
        balls.advance(start + TICK * 19);
        assert_eq!(balls.thunder[0].at(), [40.0, 5.0, 0.0]);
        balls.advance(start + TICK * 20);
        assert!(balls.thunder.is_empty());
    }
}
