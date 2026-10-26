//! Taming: a press on a monster while aiming with a taming item opens the slot machine; a press on the machine makes
//! the try, and the reel stops on the result.

use std::time::{Duration, Instant};

/// The machine: 196 × 256, no frame, in the middle of the screen.
pub const SIZE: [f32; 2] = [196.0, 256.0];
/// Spinning before the press, then after it (eight frames).
const SPIN: usize = 0;
const TRY: usize = 1;
/// The frame the reel stops on: caught, missed.
const CAUGHT_FRAME: usize = 7;
const MISSED_FRAME: usize = 3;
/// After the result the reel moves a frame every 7 drawn frames.
const SLOW: u32 = 7;
/// Stopped this long, it closes.
const SHOWN_AFTER: Duration = Duration::from_millis(2000);
// ponytail: one sprite frame per drawn frame on a 60-a-second clock, as the drawing rate isn't fixed.
const FRAMES_A_SECOND: u32 = 60;

/// Where the machine sits on a screen this size.
pub fn place(screen: [f32; 2]) -> [f32; 2] {
    [screen[0] / 2.0 - SIZE[0] / 2.0, screen[1] / 2.0 - SIZE[1] / 2.0]
}

/// Whether a press while aiming may open the machine on a unit: not on a player, nor on our own pet.
pub fn tameable(player: bool, id: u32, our_pet: Option<u32>) -> bool {
    !player && our_pet != Some(id)
}

/// Drawn frames in a span of time.
fn ticks(span: Duration) -> u32 {
    (span.as_millis() * u128::from(FRAMES_A_SECOND) / 1000) as u32
}

pub struct Machine {
    /// The unit it's for.
    pub monster: u32,
    opened: Instant,
    pressed: Option<Instant>,
    /// The result: caught or not, and when it came.
    answer: Option<(bool, Instant)>,
}

impl Machine {
    pub fn new(monster: u32, now: Instant) -> Self {
        Self { monster, opened: now, pressed: None, answer: None }
    }

    /// The first press on the machine makes the try (true: send it); later ones do nothing.
    pub fn press(&mut self, now: Instant) -> bool {
        let first = self.pressed.is_none();
        self.pressed.get_or_insert(now);
        first
    }

    pub fn answer(&mut self, caught: bool, now: Instant) {
        if self.pressed.is_some() && self.answer.is_none() {
            self.answer = Some((caught, now));
        }
    }

    /// The action and frame to draw now (`frames`: how many each of actions 0 and 1 has), or None once it has stood
    /// stopped 2000 ms.
    pub fn frame(&self, now: Instant, frames: [usize; 2]) -> Option<(usize, usize)> {
        let [spin, reel] = frames.map(|count| count.max(1));
        let Some(pressed) = self.pressed else {
            return Some((SPIN, ticks(now.saturating_duration_since(self.opened)) as usize % spin));
        };
        let spinning = |at: Instant| ticks(at.saturating_duration_since(pressed)) as usize % reel;
        let Some((caught, answered)) = self.answer else { return Some((TRY, spinning(now))) };
        let stop = (if caught { CAUGHT_FRAME } else { MISSED_FRAME }).min(reel - 1);
        let from = spinning(answered);
        let steps = (stop + reel - from) % reel;
        let moved = (ticks(now.saturating_duration_since(answered)) / SLOW) as usize;
        if moved < steps {
            return Some((TRY, (from + moved) % reel));
        }
        let stopped = answered + Duration::from_millis(u64::from(SLOW) * steps as u64 * 1000 / u64::from(FRAMES_A_SECOND));
        (now.saturating_duration_since(stopped) < SHOWN_AFTER).then_some((TRY, stop))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_in_the_middle_for_others() {
        assert_eq!(place([1024.0, 768.0]), [414.0, 256.0]);
        assert!(tameable(false, 5, None) && tameable(false, 5, Some(6)));
        assert!(!tameable(true, 5, None) && !tameable(false, 6, Some(6)));
    }

    #[test]
    fn spins_then_stops_on_the_result() {
        let start = Instant::now();
        let at = |ms: u64| start + Duration::from_millis(ms);
        let mut machine = Machine::new(9, start);
        // Before the press: action 0, a frame a drawn frame (60 a second); an answer now changes nothing.
        assert_eq!(machine.frame(at(50), [4, 8]), Some((0, 3)));
        machine.answer(true, at(60));
        assert_eq!(machine.frame(at(67), [4, 8]), Some((0, 0)));
        // The first press tries, the next doesn't.
        assert!(machine.press(at(100)) && !machine.press(at(150)));
        assert_eq!(machine.frame(at(150), [4, 8]), Some((1, 3)));
        // Caught at frame 1 (117 ms after the press): a frame every 7 drawn frames, six steps to frame 7.
        machine.answer(true, at(117));
        assert_eq!(machine.frame(at(117), [4, 8]), Some((1, 1)));
        assert_eq!(machine.frame(at(117 + 117), [4, 8]), Some((1, 2)));
        assert_eq!(machine.frame(at(117 + 699), [4, 8]), Some((1, 6)));
        assert_eq!(machine.frame(at(117 + 700), [4, 8]), Some((1, 7)));
        // Stopped at 117 + 700 ms; it closes 2000 ms later.
        assert_eq!(machine.frame(at(117 + 700 + 1999), [4, 8]), Some((1, 7)));
        assert_eq!(machine.frame(at(117 + 700 + 2000), [4, 8]), None);
        // Missed: frame 3.
        let mut missed = Machine::new(9, start);
        missed.press(start);
        missed.answer(false, start);
        assert!(missed.frame(at(10_000), [4, 8]).is_none());
        assert_eq!(missed.frame(at(351), [4, 8]), Some((1, 3)));
    }
}
