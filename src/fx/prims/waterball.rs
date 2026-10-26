//! Water Ball (EF_WATERBALL2): one sprite ball a damage packet, swinging back over the caster's feet, arcing over the
//! target and coming down a little past it, with a tail of twelve copies.

use std::time::Instant;

use super::TICK;
use crate::world::Vector;

pub const SPRITE: &str = "data\\sprite\\이팩트\\Waterball";
/// The ball is drawn on its ticks 1 to 40.
const LIFE: u32 = 40;
const COPIES: u32 = 12;
const SCALE: f32 = 0.75;
const ALPHA: f32 = 254.0;
/// The picture of each block of four ticks.
const IMAGES: [u32; 10] = [0, 1, 1, 2, 2, 0, 0, 1, 1, 2];

/// One copy of the ball to draw: where, which picture of the sprite, its size and its alpha (0 to 255).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drawn {
    pub at: Vector,
    pub image: u32,
    pub scale: f32,
    pub alpha: f32,
}

struct Ball {
    start: Instant,
    feet: Vector,
    /// Along the ground toward the aim point, and how far away it is.
    forward: [f32; 2],
    distance: f32,
}

impl Ball {
    /// The ball from the caster's `feet` at the target's `aim`; straight at it (not corrected for the game's angle table).
    // ponytail: the flight is straight along the ground; the game turns by whole degrees.
    fn new(start: Instant, feet: Vector, aim: Vector) -> Self {
        let [dx, dz] = [aim[0] - feet[0], aim[2] - feet[2]];
        let distance = f32::hypot(dx, dz);
        let forward = if distance > 0.0 { [dx / distance, dz / distance] } else { [0.0, 1.0] };
        Self { start, feet, forward, distance }
    }

    /// Its tick at `now`, from 1.
    fn tick(&self, now: Instant) -> Option<u32> {
        let ticks = now.checked_duration_since(self.start)?.as_millis() / TICK.as_millis();
        Some(ticks as u32 + 1)
    }

    /// The ball at its tick `n` (from 1): along the ground it first swings back, then goes on past the target; up 10 + 2n
    /// less a 20th of n (n − 1). A tail copy older than the ball's first tick sits at the feet.
    fn at(&self, n: i32) -> Vector {
        if n < 1 {
            return self.feet;
        }
        let n = n as f32;
        let along = n * (self.distance / LIFE as f32 - 3.5) + 0.0875 * n * (n + 1.0);
        let up = 10.0 + 2.0 * n - 0.05 * n * (n - 1.0);
        [self.feet[0] + self.forward[0] * along, self.feet[1] + up, self.feet[2] + self.forward[1] * along]
    }

    /// The copies at `now`, the ball first: copy i is where the ball was i ticks ago, i twelfths fainter and smaller.
    fn drawn(&self, now: Instant) -> Vec<Drawn> {
        let Some(n) = self.tick(now).filter(|&n| n <= LIFE) else { return Vec::new() };
        let image = IMAGES[((n - 1) / 4) as usize];
        (0..COPIES)
            .map(|i| {
                let share = i as f32 / COPIES as f32;
                Drawn { at: self.at(n as i32 - i as i32), image, scale: SCALE * (1.0 - share / 2.0), alpha: ALPHA * (1.0 - share) }
            })
            .collect()
    }
}

/// The balls in flight.
#[derive(Default)]
pub struct Waterballs {
    list: Vec<Ball>,
}

impl Waterballs {
    /// A ball from the caster's feet `from` at the feet `to` of the target.
    pub fn throw(&mut self, from: Vector, to: Vector, now: Instant) {
        self.list.push(Ball::new(now, from, to));
    }

    pub fn advance(&mut self, now: Instant) {
        self.list.retain(|ball| ball.tick(now).is_none_or(|n| n <= LIFE));
    }

    pub fn drawn(&self, now: Instant) -> Vec<Drawn> {
        self.list.iter().flat_map(|ball| ball.drawn(now)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(got: f32, want: f32) -> bool {
        (got - want).abs() < 0.06
    }

    #[test]
    fn swings_back_arcs_over_and_lands_past_the_target() {
        // 25 units away along +x.
        let ball = Ball::new(Instant::now(), [10.0, 3.0, 20.0], [35.0, 99.0, 20.0]);
        let along = |n: i32| ball.at(n)[0] - 10.0;
        assert!(near(along(1), -2.7) && near(along(16), -22.2) && near(along(40), 28.5));
        assert!(ball.at(1)[2] == 20.0, "straight along the ground");
        // 3 up from the feet plus 12 at the first tick, 31 at tick 20 and 12 at tick 40; the aim's height is never used.
        let up = |n: i32| ball.at(n)[1] - 3.0;
        assert!(near(up(1), 12.0) && near(up(20), 31.0) && near(up(40), 12.0));
        // 3.5 past a target 35 away.
        let far = Ball::new(Instant::now(), [0.0; 3], [0.0, 0.0, 35.0]);
        assert!(near(far.at(40)[2], 38.5));
        // None away: the way the ghost faces.
        assert!(Ball::new(Instant::now(), [0.0; 3], [0.0; 3]).forward == [0.0, 1.0]);
    }

    #[test]
    fn shows_one_picture_a_block_of_four_ticks_and_a_tail_of_twelve() {
        let start = Instant::now();
        let ball = Ball::new(start, [0.0; 3], [25.0, 0.0, 0.0]);
        let drawn = |tick: u32| ball.drawn(start + TICK * (tick - 1));
        // The images by block: 0, 1, 1, 2, 2, 0, 0, 1, 1, 2.
        let images: Vec<u32> = (0..10).map(|block| drawn(1 + 4 * block)[0].image).collect();
        assert_eq!(images, [0, 1, 1, 2, 2, 0, 0, 1, 1, 2]);
        assert_eq!(drawn(4)[0].image, 0);
        assert_eq!(drawn(5)[0].image, 1);
        assert!(drawn(5).iter().all(|copy| copy.image == 1), "one picture for every copy");
        // Twelve copies from the first tick, the ones behind the ball at the feet.
        let first = drawn(1);
        assert_eq!(first.len(), 12);
        assert_eq!(first[0].at[1], 12.0);
        assert!(first[1..].iter().all(|copy| copy.at == [0.0; 3]));
        // Copy 1 leaves the feet on tick 2, where the ball was at tick 1; copy 11 on tick 12.
        assert_eq!(drawn(2)[1].at, first[0].at);
        assert_eq!(drawn(11)[10].at, ball.at(1));
        assert_eq!(drawn(12)[11].at, ball.at(1));
        // The last copy: alpha 254 − 254·11/12 = 21.17, scale 0.75 − 0.75/12·11·0.5 = 0.40625; the ball 254 and 0.75.
        assert!(near(first[11].alpha, 254.0 / 12.0) && near(first[11].scale, 0.406_25));
        assert!(first[0].alpha == 254.0 && first[0].scale == 0.75);
        // Drawn on ticks 1 to 40, gone at 41.
        assert_eq!(drawn(40).len(), 12);
        assert!(drawn(41).is_empty());
    }

    #[test]
    fn a_ball_a_packet() {
        let start = Instant::now();
        let mut balls = Waterballs::default();
        for packet in 0..5 {
            balls.throw([0.0; 3], [25.0, 0.0, 0.0], start + TICK * packet * 9);
        }
        // Five packets 9 ticks apart: all five fly on the first one's tick 40, and the first is gone a tick later.
        assert_eq!(balls.drawn(start + TICK * 39).len(), 5 * 12);
        assert_eq!(balls.drawn(start + TICK * 40).len(), 4 * 12);
        balls.advance(start + TICK * 100);
        assert!(balls.list.is_empty());
    }
}
