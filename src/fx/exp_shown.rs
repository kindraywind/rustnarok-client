//! The base and job exp a kill gave, floating up slowly over the spot where the monster died, fading like a MISS: a small "EXP" or
//! "JOB" and the amount in the damage numbers' digits, quiet enough to show on every kill.
//!
//! The server sends a kill's gain (ZC_NOTIFY_EXP2: base, then job) as the monster dies, and the death itself (its
//! vanish, reason 1) about 250 ms later, so each gain waits here for the next monster to die.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use image::RgbaImage;
use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};

/// How long a gain waits for its monster's death; after that it came from something else (a quest, an item).
const WAIT: Duration = Duration::from_secs(1);
/// How long a kill's lines float (a MISS's 80 ticks of 24 ms).
pub(crate) const SHOWN: Duration = Duration::from_millis(1920);
/// A MISS's motion (its size, fading by tick 83) rising at half its speed.
pub(crate) fn motion(ticks: f32) -> crate::actor::NumberMotion {
    crate::actor::NumberMotion { height: 12.0 + 0.27 * ticks, ..crate::actor::miss_number(ticks) }
}

/// The label's pixel size (chat text is 12).
const LABEL_SIZE: f32 = 10.0;
/// The labels are rasterized this many times larger, to stay sharp when the camera comes close.
pub(crate) const RASTER: f32 = 2.0;
/// Soft gold for base, lilac for job: apart from the damage numbers' white, red, yellow, green and blue.
pub(crate) const COLORS: [[u8; 3]; 2] = [[0xFF, 0xD0, 0x60], [0xC8, 0xA0, 0xFF]];

#[derive(Default)]
pub(crate) struct ExpShown {
    /// Gains heard and not yet shown, oldest first: when, base, job (none until the job packet comes).
    waiting: VecDeque<(Instant, i64, Option<i64>)>,
    /// What floats now: the world spot, when it starts, base, job.
    pub(crate) shown: Vec<([f32; 3], Instant, i64, i64)>,
    /// "EXP" and "JOB", made when first shown (a map's `Game` has its own, so a new font shows from the next map).
    pub(crate) labels: Option<[Texture; 2]>,
}

impl ExpShown {
    /// A gain of `amount` base (or `job`) exp. Base starts a kill's pair; job joins the last kill that has no job yet.
    // ponytail: a kill giving only job exp right after one giving only base pairs them; split by the time heard if seen.
    pub(crate) fn gained(&mut self, now: Instant, job: bool, amount: i64) {
        match self.waiting.back_mut() {
            Some((_, _, last @ None)) if job => *last = Some(amount),
            _ if job => self.waiting.push_back((now, 0, Some(amount))),
            _ => self.waiting.push_back((now, amount, None)),
        }
    }

    /// A monster died at `at`: the oldest gain still waiting floats there from `start` (when its body falls).
    pub(crate) fn died(&mut self, now: Instant, at: [f32; 3], start: Instant) {
        self.waiting.retain(|(heard, ..)| now.saturating_duration_since(*heard) < WAIT);
        self.shown.retain(|(_, from, ..)| now.saturating_duration_since(*from) < SHOWN);
        if let Some((_, base, job)) = self.waiting.pop_front()
            && (base, job) != (0, Some(0))
        {
            self.shown.push((at, start, base, job.unwrap_or(0)));
        }
    }

    /// Makes the labels the first time a kill shows.
    pub(crate) fn prepare(&mut self, gpu: &Gpu, font: &Font) {
        if self.labels.is_none() && !self.shown.is_empty() {
            self.labels = Some([("EXP", COLORS[0]), ("JOB", COLORS[1])].map(|(text, color)| gpu.texture(&label(font, text, color), true)));
        }
    }
}

/// `text` in `color` with a black outline, `RASTER` times its size.
fn label(font: &Font, text: &str, color: [u8; 3]) -> RgbaImage {
    let [black, colored] = [[0; 3], color].map(|color| font.render_sized(text, color, LABEL_SIZE * RASTER));
    let outline = RASTER as i64;
    let mut image = RgbaImage::new(black.width() + 2 * outline as u32, black.height() + 2 * outline as u32);
    for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1), (-1, -1), (1, -1), (-1, 1), (1, 1)] {
        image::imageops::overlay(&mut image, &black, outline + dx * outline, outline + dy * outline);
    }
    image::imageops::overlay(&mut image, &colored, outline, outline);
    image
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_death_takes_the_oldest_gain_heard_in_the_last_second() {
        let t0 = Instant::now();
        let ms = |ms| t0 + Duration::from_millis(ms);
        let mut exp = ExpShown::default();
        // Two monsters killed at once (base, job, base, job), dying 250 ms later; then a quest's gain nobody died for.
        exp.gained(t0, false, 54);
        exp.gained(t0, true, 30);
        exp.gained(t0, false, 7);
        exp.gained(t0, true, 0);
        exp.died(ms(250), [1.0; 3], ms(300));
        exp.died(ms(250), [2.0; 3], ms(300));
        assert_eq!(exp.shown, [([1.0; 3], ms(300), 54, 30), ([2.0; 3], ms(300), 7, 0)]);
        exp.gained(ms(500), false, 1000);
        exp.died(ms(1600), [3.0; 3], ms(1600));
        assert_eq!(exp.shown.len(), 2, "a gain over a second old isn't the dead monster's");
        // Only job exp (base at the top level); a kill at both top levels gives zeros and shows nothing.
        exp.gained(ms(2000), true, 12);
        exp.gained(ms(2000), false, 0);
        exp.gained(ms(2000), true, 0);
        exp.died(ms(2200), [4.0; 3], ms(2200));
        exp.died(ms(2200), [5.0; 3], ms(2200));
        assert_eq!(exp.shown.last(), Some(&([4.0; 3], ms(2200), 0, 12)));
        // Lines that finished floating are dropped at the next death.
        exp.died(ms(9000), [6.0; 3], ms(9000));
        assert!(exp.shown.is_empty());
    }
}
