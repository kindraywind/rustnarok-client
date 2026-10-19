//! A bow's arrows: one from over the archer at its target with each swing (attacks, Double Strafe, Arrow Shower), and
//! Arrow Shower's at points round its cells. Each waits, then flies straight in 24 ms steps, facing its way; nothing
//! shows where it lands.

use std::time::{Duration, Instant};

use crate::world::Vector;

/// The arrow's sprite: an action per direction of flight, as a unit's eight.
pub const SPRITE: &str = "data\\sprite\\몬스터\\skel_archer_arrow";
/// A step every 24 ms: the whole way in 8, or 5 units a step when that's under 40 units.
const STEP: Duration = Duration::from_millis(24);
const STEPS: f32 = 8.0;
const NEAR: f32 = 40.0;
const NEAR_STEP: f32 = 5.0;
/// How high it leaves over the archer's feet and flies at over a unit's feet, and how high over a point.
const RISE: f32 = 10.0;
const POINT_RISE: f32 = 20.0;
/// How long Arrow Shower's arrows at points wait.
pub const POINT_WAIT: Duration = Duration::from_millis(432);

/// What an arrow flies at: a unit (None = the player), where it is as the arrow leaves; or a point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Aim {
    Unit(Option<u32>),
    Point(Vector),
}

struct Arrow {
    leaves: Instant,
    from: Vector,
    aim: Aim,
    /// Where it flies to, fixed as it leaves.
    to: Option<Vector>,
}

#[derive(Default)]
pub struct Arrows(Vec<Arrow>);

impl Arrows {
    /// An arrow from over the archer's `feet`, leaving `wait` after `now`.
    pub fn shoot(&mut self, feet: Vector, aim: Aim, now: Instant, wait: Duration) {
        self.0.push(Arrow { leaves: now + wait, from: [feet[0], feet[1] + RISE, feet[2]], aim, to: None });
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Arrows leaving now take their aim (`position_of` gives a unit's feet; one at a unit gone is dropped), and those
    /// that got there go.
    pub fn aim(&mut self, now: Instant, position_of: impl Fn(Option<u32>) -> Option<Vector>) {
        for arrow in self.0.iter_mut().filter(|arrow| arrow.to.is_none() && arrow.leaves <= now) {
            arrow.to = Some(match arrow.aim {
                Aim::Unit(unit) => position_of(unit).map_or(arrow.from, |feet| [feet[0], feet[1] + RISE, feet[2]]),
                Aim::Point(at) => [at[0], at[1] + POINT_RISE, at[2]],
            });
        }
        self.0.retain(|arrow| arrow.to.is_none_or(|to| flight(arrow.from, to, steps(arrow.leaves, now)).is_some()));
    }

    /// The arrows in flight: where each is, and its way along the ground (world x, z).
    pub fn flying(&self, now: Instant) -> Vec<(Vector, [f32; 2])> {
        self.0
            .iter()
            .filter_map(|arrow| {
                let to = arrow.to?;
                Some((flight(arrow.from, to, steps(arrow.leaves, now))?, [to[0] - arrow.from[0], to[2] - arrow.from[2]]))
            })
            .collect()
    }
}

/// Whole 24 ms steps from `leaves` to `now`.
fn steps(leaves: Instant, now: Instant) -> u32 {
    (now.saturating_duration_since(leaves).as_millis() / STEP.as_millis()) as u32
}

/// Where an arrow from `from` to `to` is after `steps` steps; None once it's there.
fn flight(from: Vector, to: Vector, steps: u32) -> Option<Vector> {
    let way = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
    let distance = way.iter().map(|axis| axis * axis).sum::<f32>().sqrt();
    let share = if distance < NEAR { steps as f32 * NEAR_STEP / distance } else { steps as f32 / STEPS };
    (share < 1.0).then(|| [0, 1, 2].map(|axis| from[axis] + way[axis] * share))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flies_eight_steps_or_five_units_a_step_when_near() {
        // 9 cells away (45 units): 8 steps of 24 ms, 192 ms in all.
        let (from, far) = ([0.0; 3], [0.0, 0.0, 45.0]);
        assert_eq!(flight(from, far, 4), Some([0.0, 0.0, 22.5]));
        assert_eq!((flight(from, far, 7).is_some(), flight(from, far, 8)), (true, None));
        // 4 cells away (20 units): 5 units a step, there in 4 steps (96 ms).
        let near = [0.0, 0.0, 20.0];
        assert_eq!(flight(from, near, 1), Some([0.0, 0.0, 5.0]));
        assert_eq!(flight(from, near, 4), None);
        // On the archer: nothing to fly.
        assert_eq!(flight(from, from, 0), None);
    }

    #[test]
    fn aims_as_it_leaves_and_goes_once_there() {
        let now = Instant::now();
        let mut arrows = Arrows::default();
        arrows.shoot([0.0; 3], Aim::Unit(Some(7)), now, Duration::from_millis(300));
        arrows.shoot([0.0; 3], Aim::Point([10.0, 0.0, 0.0]), now, POINT_WAIT);
        // Waiting: nothing flies.
        arrows.aim(now, |_| Some([0.0, 0.0, 45.0]));
        assert!(arrows.flying(now).is_empty());
        // At 300 ms the first takes the unit's place then, 10 up, and flies at it; it isn't tracked after.
        let at = |ms: u64| now + Duration::from_millis(ms);
        arrows.aim(at(300), |_| Some([0.0, 0.0, 45.0]));
        assert_eq!(arrows.flying(at(300)), [([0.0, 10.0, 0.0], [0.0, 45.0])]);
        arrows.aim(at(396), |_| Some([0.0, 0.0, 90.0]));
        assert_eq!(arrows.flying(at(396)), [([0.0, 10.0, 22.5], [0.0, 45.0])]);
        // At 432 ms the point arrow leaves for 20 units over its point; at 492 ms the first is there and goes.
        arrows.aim(at(492), |_| None);
        assert_eq!(arrows.flying(at(492)).len(), 1);
        assert_eq!(arrows.flying(at(492))[0].1, [10.0, 0.0]);
        arrows.aim(at(2000), |_| None);
        assert!(arrows.is_empty());
    }
}
