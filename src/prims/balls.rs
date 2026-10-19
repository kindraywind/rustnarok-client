//! What flies from where the caster stood to where the target stood, straight across the ground at an even speed:
//! Fire Ball's balls (EF_FIREBALL 24) and Jupitel Thunder's ball (EF_YUFITEL 93).

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

/// A flight: leaving over where the caster stood at `born`, a step a tick across the ground so as to be over where
/// it's aimed after its life.
struct Flight {
    born: Instant,
    /// Ticks run.
    ticks: u32,
    from: Vector,
    step: [f32; 2],
    aim: [f32; 2],
}

impl Flight {
    // ponytail: straight at the target; the game's turn truncates to whole degrees, 1–2° off.
    fn new(born: Instant, from: Vector, rise: f32, to: Vector, life: u32) -> Self {
        let step = [(to[0] - from[0]) / life as f32, (to[2] - from[2]) / life as f32];
        Self { born, ticks: 0, from: [from[0], from[1] + rise, from[2]], step, aim: [to[0], to[2]] }
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

    /// Runs everything up to `now`, a tick at a time.
    pub fn advance(&mut self, now: Instant) {
        for fire_ball in &mut self.fire_balls {
            let due = passes(fire_ball.start, now);
            for age in fire_ball.ticks..due {
                if let Some(look) = FIRE_BALL_TICKS.iter().position(|&tick| tick == age) {
                    let flight = Flight::new(fire_ball.start + TICK * age, fire_ball.from, FIRE_BALL_RISE, fire_ball.to, FIRE_BALL_LIFE);
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
        textures: &'a HashMap<String, Option<Texture>>,
        yaw: f32,
    ) -> Vec<EffectQuad<'a>> {
        let mut quads = Vec::new();
        if let Some(sprite) = sprite.filter(|_| !frames.is_empty()) {
            for ball in self.balls.iter().filter(|ball| ball.flight.ticks > 0) {
                let [x, z] = ball.flight.step;
                let turn = 180.0 - crate::actor::step_rot(x, z) - yaw;
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
        balls.fire_ball([0.0; 3], [0.0, 0.0, 45.0], start);
        // Ticks 20, 24, 28 and 32.
        balls.advance(start + TICK * 19);
        assert!(balls.balls.is_empty());
        balls.advance(start + TICK * 32);
        assert_eq!(balls.balls.iter().map(|ball| ball.look).collect::<Vec<_>>(), [0, 1, 2, 3]);
        assert!(balls.fire_balls.is_empty());
        // 15 units up, 2.25 units a tick: 9 cells away a ball is drawn 18 ticks, then goes within 3 units of it.
        let lead = &balls.balls[0];
        assert_eq!((lead.flight.ticks, lead.flight.at()), (13, [0.0, 15.0, 13.0 * 2.25]));
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
