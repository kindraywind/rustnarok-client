//! The map's ambient sounds: each RSW sound source plays its wav every cycle from the point of its box nearest the
//! player, full within a sixth of its range, fading as min / distance beyond, silent from its range on.

use std::time::{Duration, Instant};

use rustnarok_client::map::SoundSource;

/// How far past its range a source still keeps time (a play there is silent but restarts the cycle).
const BEYOND_RANGE: f32 = 75.0;

/// A source placed in world XZ (x east, z north).
#[derive(Debug, Clone)]
pub struct Source {
    pub file: String,
    pub center: [f32; 2],
    /// Its box: width along x, height along z.
    pub size: [f32; 2],
    pub volume: f32,
    pub range: f32,
    pub cycle: Duration,
    last: Option<Instant>,
}

impl Source {
    /// An RSW source, its position from the map's center moved by `half` (half the map, world units).
    pub fn placed(source: &SoundSource, half: [f32; 2]) -> Self {
        Self {
            file: source.file.clone(),
            center: [source.position[0] + half[0], source.position[2] + half[1]],
            size: [source.width as f32, source.height as f32],
            volume: source.volume,
            range: source.range,
            cycle: Duration::from_secs_f32(source.cycle.max(0.0)),
            last: None,
        }
    }
}

/// The point of a `size` box around `center` nearest `player` (the player itself when inside it).
pub fn nearest(center: [f32; 2], size: [f32; 2], player: [f32; 2]) -> [f32; 2] {
    std::array::from_fn(|axis| player[axis].clamp(center[axis] - size[axis] / 2.0, center[axis] + size[axis] / 2.0))
}

/// How loud a source of `range` is `distance` away: 1 within range / 6 (whole units), min / distance beyond, 0 from the
/// range on.
pub fn loudness(distance: f32, range: f32) -> f32 {
    let near = (range / 6.0).trunc();
    if distance >= range {
        0.0
    } else if distance <= near {
        1.0
    } else {
        near / distance
    }
}

/// The map's sources.
#[derive(Debug, Default)]
pub struct Ambient {
    sources: Vec<Source>,
}

impl Ambient {
    pub fn new(sources: Vec<Source>) -> Self {
        Self { sources }
    }

    /// The sources due at `now` for a player at `player` (world XZ): each wav and its volume. A source with no width
    /// is skipped, one farther than its range + 75 waits; the first play comes at once.
    pub fn due(&mut self, player: [f32; 2], now: Instant) -> Vec<(String, f32)> {
        let mut plays = Vec::new();
        for source in &mut self.sources {
            if source.size[0] == 0.0 {
                continue;
            }
            let at = nearest(source.center, source.size, player);
            let distance = f32::hypot(player[0] - at[0], player[1] - at[1]);
            if distance > source.range + BEYOND_RANGE {
                continue;
            }
            if source.last.is_some_and(|last| now.saturating_duration_since(last) <= source.cycle) {
                continue;
            }
            source.last = Some(now);
            plays.push((source.file.clone(), loudness(distance, source.range) * source.volume));
        }
        plays
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bell() -> Source {
        let rsw = SoundSource { file: "bell.wav".into(), position: [0.0, -40.0, 100.0], volume: 0.5, width: 50, height: 40, range: 70.0, cycle: 6.0 };
        Source::placed(&rsw, [500.0, 500.0])
    }

    #[test]
    fn hears_from_the_nearest_point_of_the_box() {
        let bell = bell();
        assert_eq!((bell.center, bell.size), ([500.0, 600.0], [50.0, 40.0]));
        // Inside: the player itself; beside it: its edge; past a corner: the corner.
        assert_eq!(nearest(bell.center, bell.size, [510.0, 590.0]), [510.0, 590.0]);
        assert_eq!(nearest(bell.center, bell.size, [400.0, 610.0]), [475.0, 610.0]);
        assert_eq!(nearest(bell.center, bell.size, [600.0, 700.0]), [525.0, 620.0]);
    }

    #[test]
    fn fades_from_a_sixth_of_the_range() {
        // Range 70: full within 11 units, 11 / d beyond, silent from 70.
        assert_eq!([0.0, 11.0, 22.0, 55.0, 69.0, 70.0].map(|d| loudness(d, 70.0)), [1.0, 1.0, 0.5, 0.2, 11.0 / 69.0, 0.0]);
    }

    #[test]
    fn plays_once_a_cycle_and_waits_far_away() {
        let mut ambient = Ambient::new(vec![bell(), Source { size: [0.0, 1.0], ..bell() }]);
        let start = Instant::now();
        let near = [500.0, 560.0];
        // The first play at once, 20 units from the box's south edge: 11 / 20 of the half volume.
        assert_eq!(ambient.due(near, start), [("bell.wav".to_owned(), 0.5 * 11.0 / 20.0)]);
        assert!(ambient.due(near, start + Duration::from_secs(6)).is_empty());
        assert_eq!(ambient.due(near, start + Duration::from_millis(6001)).len(), 1);
        // 30 cells (150 units) off: past range + 75, nothing; its timer waits.
        let far = [500.0, 430.0];
        assert!(ambient.due(far, start + Duration::from_secs(20)).is_empty());
        // Between range and range + 75: a silent play that restarts the cycle.
        assert_eq!(ambient.due([500.0, 500.0], start + Duration::from_secs(30)), [("bell.wav".to_owned(), 0.0)]);
    }
}
