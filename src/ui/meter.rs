//! This client's FPS and ping corner (Extras > FPS and ping): the frames drawn over the last second or so, and how long
//! the map server took to answer the latest CZ_REQUEST_TIME with its ZC_NOTIFY_TIME. The client sends that at each map
//! load and every 12 s anyway (`net::REQUEST_TIME_EVERY`), so the ping changes that often and costs no packet of its own.

use std::time::{Duration, Instant};

#[derive(Default)]
pub struct Meter {
    /// When the stretch being counted began, and the frames drawn in it.
    stretch: Option<(Instant, u32)>,
    pub fps: Option<u32>,
    pub ping: Option<Duration>,
    /// The time frames took to make in the stretch, waiting for the display left out, and how many; and the average of
    /// the last stretch in milliseconds (the control socket's `meter.frame_ms`, for measuring the client's own cost).
    work: (Duration, u32),
    pub frame_ms: Option<f32>,
}

impl Meter {
    /// A frame is drawn at `now`. Once a second has gone by, the frames counted make the rate (per whole second taken,
    /// so a stall shows as a low number rather than being lost).
    pub fn frame(&mut self, now: Instant) {
        match &mut self.stretch {
            Some((start, frames)) => {
                let elapsed = now.saturating_duration_since(*start);
                if elapsed >= Duration::from_secs(1) {
                    self.fps = Some((f64::from(*frames) / elapsed.as_secs_f64()).round() as u32);
                    let (work, made) = std::mem::take(&mut self.work);
                    self.frame_ms = (made > 0).then(|| work.as_secs_f32() * 1000.0 / made as f32);
                    self.stretch = Some((now, 1));
                } else {
                    *frames += 1;
                }
            }
            None => self.stretch = Some((now, 1)),
        }
    }

    /// A frame took `took` to make, waiting for the display left out.
    pub fn worked(&mut self, took: Duration) {
        self.work.0 += took;
        self.work.1 += 1;
    }

    /// The server's answer to our last time request was heard `round_trip` after it went out.
    pub fn ping(&mut self, round_trip: Duration) {
        self.ping = Some(round_trip);
    }

    /// "FPS 60  Ping 12 ms", with "-" for what isn't known yet.
    pub fn text(&self) -> String {
        let fps = self.fps.map_or("-".to_owned(), |fps| fps.to_string());
        let ping = self.ping.map_or("-".to_owned(), |ping| format!("{} ms", ping.as_millis()));
        format!("FPS {fps}  Ping {ping}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_frames_a_second_and_shows_the_last_ping() {
        let mut meter = Meter::default();
        assert_eq!(meter.text(), "FPS -  Ping -");
        let start = Instant::now();
        // 60 frames 1/60 s apart, then the one at 1 s closes the second.
        for frame in 0..=60 {
            meter.frame(start + Duration::from_secs_f64(f64::from(frame) / 60.0));
        }
        assert_eq!(meter.text(), "FPS 60  Ping -");
        // Frames that took 2 ms and 4 ms of work average 3 ms when the next second closes.
        meter.worked(Duration::from_millis(2));
        meter.worked(Duration::from_millis(4));
        // A 2 s stall with 10 frames before it: 10 frames over 2 s.
        for frame in 1..10 {
            meter.frame(start + Duration::from_millis(1000 + frame * 10));
        }
        meter.frame(start + Duration::from_secs(3));
        assert_eq!(meter.text(), "FPS 5  Ping -");
        assert_eq!(meter.frame_ms, Some(3.0));
        meter.ping(Duration::from_micros(12_700));
        assert_eq!(meter.text(), "FPS 5  Ping 12 ms");
    }
}
