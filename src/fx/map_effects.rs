//! The map's own effects (RSW type-4 objects): emitters and smoke started once when the map loads, and the rest
//! (torches, fireflies, bubbles, glows) each on a timer of 16 ms ticks that fires while the player is near.

use rustnarok_client::map::EffectSource;

use crate::world::Vector;

/// Effects started once at the map's load, wherever the player is: the emitters (974, 1073), smoke (44) and a few
/// others.
const STARTED_ONCE: [u32; 7] = [974, 44, 2343, 1073, 1039, 1025, 1412];
/// EF_FIREFLY: its period and start drift by chance.
const FIREFLY: u32 = 45;
/// A timer fires only with the player under 130 world units (26 cells) away, on the ground plane.
const RANGE: f32 = 130.0;
const TICK_MS: u64 = 16;
/// At most this many ticks are caught up in one frame (a long pause doesn't fire everything at once).
const MOST_TICKS: u64 = 64;

/// A timed effect: what plays where, its period, and its counter (compared unsigned, so a negative start fires on the
/// first tick).
#[derive(Debug, Clone, PartialEq)]
pub struct Timer {
    pub id: u32,
    pub at: Vector,
    pub period: i32,
    pub counter: i32,
}

/// A timer's period and start counter: P = int(speed) and int(speed − rand() % 24); a firefly (45) P = int(speed −
/// speed / 20 + rand() % trunc(speed / 10)), starting at rand() % trunc(speed + 1).
pub fn timer(id: u32, speed: f32, mut rand: impl FnMut() -> u32) -> (i32, i32) {
    let modulo = |value: u32, by: f32| if by >= 1.0 { value % by as u32 } else { 0 };
    if id == FIREFLY {
        let period = (speed - speed / 20.0 + modulo(rand(), (speed / 10.0).trunc()) as f32) as i32;
        (period, modulo(rand(), (speed + 1.0).trunc()) as i32)
    } else {
        (speed as i32, (speed - (rand() % 24) as f32) as i32)
    }
}

/// One 16 ms tick of a timer: in range with the counter at its period or past it (unsigned), it fires and starts
/// over; the counter then goes up by one, in range or not.
pub fn tick(timer: &mut Timer, in_range: bool) -> bool {
    let fires = in_range && timer.counter as u32 >= timer.period as u32;
    if fires {
        timer.counter = 0;
    }
    timer.counter = timer.counter.wrapping_add(1);
    fires
}

pub struct MapEffects {
    /// Played once, at the map's first frame.
    started: Vec<(u32, Vector)>,
    pub timers: Vec<Timer>,
    /// The last 16 ms tick run, and how many timed plays there have been (for the test driver).
    last: Option<u64>,
    pub fired: u32,
    pub last_fired: Option<u64>,
}

impl MapEffects {
    /// The map's effect objects, placed in the world (x and z moved by `half` from the map's centre, y up).
    pub fn new(sources: &[EffectSource], half: [f32; 2], mut rand: impl FnMut() -> u32) -> Self {
        let (mut started, mut timers) = (Vec::new(), Vec::new());
        for source in sources {
            let Ok(id) = u32::try_from(source.id) else { continue };
            let at = [source.position[0] + half[0], -source.position[1], source.position[2] + half[1]];
            if STARTED_ONCE.contains(&id) {
                started.push((id, at));
            } else {
                let (period, counter) = timer(id, source.speed, &mut rand);
                timers.push(Timer { id, at, period, counter });
            }
        }
        Self { started, timers, last: None, fired: 0, last_fired: None }
    }

    /// What plays this frame (`now_ms` in whole ms, the player at `player` on the ground plane): the start-once
    /// effects on the first call, then each timer that fires on each 16 ms tick passed since the last call.
    pub fn due(&mut self, now_ms: u64, player: [f32; 2]) -> Vec<(u32, Vector)> {
        let mut plays = std::mem::take(&mut self.started);
        let tick_now = now_ms / TICK_MS;
        let ticks = self.last.map_or(1, |last| tick_now.saturating_sub(last).min(MOST_TICKS));
        self.last = Some(tick_now);
        for timer in &mut self.timers {
            let in_range = f32::hypot(timer.at[0] - player[0], timer.at[2] - player[1]) < RANGE;
            for _ in 0..ticks {
                if tick(timer, in_range) {
                    plays.push((timer.id, timer.at));
                    self.fired += 1;
                    self.last_fired = Some(now_ms);
                }
            }
        }
        plays
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(id: i32, speed: f32) -> EffectSource {
        EffectSource { name: String::new(), position: [10.0, -5.0, 20.0], id, speed, params: [0.0; 4] }
    }

    #[test]
    fn times_torches_and_fireflies() {
        // A torch (47, speed 125): P 125, starting at 125 − rand % 24.
        assert_eq!(timer(47, 125.0, || 30), (125, 119));
        // A firefly (45, speed 500): P = int(500 − 25 + rand % 50), starting at rand % 501.
        let mut draws = [520u32, 7].into_iter();
        assert_eq!(timer(45, 500.0, || draws.next().unwrap()), (495, 7));
        // A negative start fires on the first tick (unsigned), in range only; out of range the counter still grows.
        let mut fresh = Timer { id: 47, at: [0.0; 3], period: 125, counter: -3 };
        assert!(!tick(&mut fresh.clone(), false));
        assert!(tick(&mut fresh, true));
        assert_eq!(fresh.counter, 1);
        let mut away = Timer { id: 47, at: [0.0; 3], period: 3, counter: 0 };
        for _ in 0..5 {
            assert!(!tick(&mut away, false));
        }
        assert_eq!(away.counter, 5);
        // Back in range past its period: at once.
        assert!(tick(&mut away, true));
    }

    #[test]
    fn plays_the_once_ids_and_the_slow_ones_once() {
        // Speed 9 999 999: within 23 ticks of the load, then not again for days.
        let (period, counter) = timer(700, 9_999_999.0, || 23);
        assert_eq!((period, counter), (9_999_999, 9_999_976));
        let mut glow = Timer { id: 700, at: [0.0; 3], period, counter };
        let fired: Vec<u32> = (0..100).filter(|_| tick(&mut glow, true)).collect();
        assert_eq!(fired.len(), 1);
        // The start-once ids play on the first frame, wherever the player is, and never again.
        let mut effects = MapEffects::new(&[source(974, 0.0), source(44, 0.0), source(47, 125.0)], [100.0, 100.0], || 0);
        let first = effects.due(1000, [9999.0, 9999.0]);
        assert_eq!(first.iter().map(|(id, _)| *id).collect::<Vec<_>>(), [974, 44]);
        assert_eq!(first[0].1, [110.0, 5.0, 120.0]);
        assert!(effects.due(1016, [9999.0, 9999.0]).is_empty());
        // The torch near the player, a frame a tick: at once (its counter is past its period), then every 125 ticks.
        let near = [110.0, 120.0];
        let fired: Vec<u64> = (0..300).filter(|tick| !effects.due(1032 + 16 * tick, near).is_empty()).collect();
        assert_eq!(fired, [0, 125, 250]);
    }
}
