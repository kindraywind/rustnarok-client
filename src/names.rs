//! Names asked of the server one at a time, as the game asks them: whoever wants a name queues its ID, and each frame
//! sends at most one request, skipping a name already known or asked less than 10 s ago. Our frames also come on
//! packets and input, so "a frame" is at least `FRAME_TIME` apart here.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// How long an asked name counts as on its way: asked again after that if still wanted and unanswered.
const IN_FLIGHT: Duration = Duration::from_secs(10);

#[derive(Default)]
pub(crate) struct NameQueue {
    /// Wanted and not yet sent, oldest first.
    wanted: Vec<u32>,
    /// When each name on its way was asked.
    asked: HashMap<u32, Instant>,
    /// Names the server has answered.
    known: HashSet<u32>,
    /// When the last request went out.
    sent: Option<Instant>,
}

impl NameQueue {
    pub(crate) fn want(&mut self, id: u32) {
        if !self.known.contains(&id) && !self.wanted.contains(&id) {
            self.wanted.push(id);
        }
    }

    pub(crate) fn answered(&mut self, id: u32) {
        self.known.insert(id);
        self.wanted.retain(|&wanted| wanted != id);
        self.asked.remove(&id);
    }

    /// A unit gone: its name is asked again when it comes back.
    pub(crate) fn forget(&mut self, id: u32) {
        self.known.remove(&id);
        self.wanted.retain(|&wanted| wanted != id);
        self.asked.remove(&id);
    }

    /// The name to ask for this frame, if any.
    pub(crate) fn pump(&mut self, now: Instant) -> Option<u32> {
        if self.sent.is_some_and(|sent| now.saturating_duration_since(sent) < crate::FRAME_TIME) {
            return None;
        }
        self.asked.retain(|_, at| now.saturating_duration_since(*at) < IN_FLIGHT);
        let next = self.wanted.iter().position(|id| !self.asked.contains_key(id))?;
        let id = self.wanted.remove(next);
        self.asked.insert(id, now);
        self.sent = Some(now);
        Some(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_a_frame_never_known_ones_again_after_ten_seconds() {
        let start = Instant::now();
        let mut names = NameQueue::default();
        names.want(1);
        names.want(2);
        names.want(1);
        assert_eq!(names.pump(start), Some(1));
        // One a frame, however often frames come.
        assert_eq!(names.pump(start + Duration::from_millis(15)), None);
        let frame = start + crate::FRAME_TIME;
        assert_eq!(names.pump(frame), Some(2));
        assert_eq!(names.pump(frame + crate::FRAME_TIME), None);
        // Still wanted (the pointer stays on it) but on its way: not again until 10 s have passed.
        names.want(1);
        assert_eq!(names.pump(start + Duration::from_millis(9_999)), None);
        assert_eq!(names.pump(start + IN_FLIGHT), Some(1));
        // Answered: never asked again, until the unit goes and comes back.
        names.answered(1);
        names.want(1);
        assert_eq!(names.pump(start + IN_FLIGHT * 3), None);
        names.forget(1);
        names.want(1);
        assert_eq!(names.pump(start + IN_FLIGHT * 3), Some(1));
        // A unit that goes while its name is on its way is asked at once when it's back.
        names.forget(1);
        names.want(1);
        assert_eq!(names.pump(start + IN_FLIGHT * 3 + crate::FRAME_TIME), Some(1));
    }
}
