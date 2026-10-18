//! The magic square under a ground skill being cast: (area + 2) cells wide on the target cell, turning and pulsing
//! for the cast time.

use std::time::{Duration, Instant};

/// Skill id to its area's radius per level (rathena's skill_db.yml).
const AREAS: &str = include_str!("ground_areas.txt");
/// ponytail: how fast it turns (radians a second), and how much and how fast it pulses.
const TURN: f32 = std::f32::consts::FRAC_PI_2;
const PULSE: f32 = 0.05;
const PULSE_HZ: f32 = 1.0;

/// The radius of a ground skill's area at `level`, or at its level 10 (or last) when the level isn't known.
pub fn radius(skill: u16, level: Option<u16>) -> Option<u16> {
    let line = AREAS.lines().filter(|line| !line.starts_with('#')).find(|line| line.split('\t').next() == Some(&skill.to_string()))?;
    let levels: Vec<u16> = line.split('\t').nth(1)?.split(',').filter_map(|value| value.parse().ok()).collect();
    let index = usize::from(level.unwrap_or(10).max(1) - 1).min(levels.len().checked_sub(1)?);
    levels.get(index).copied()
}

/// How many cells wide the square is for an area of this radius: the area and a cell on each side.
pub fn width(radius: u16) -> f32 {
    f32::from(2 * radius + 1 + 2)
}

pub struct Square {
    pub cell: [u16; 2],
    pub width: f32,
    pub start: Instant,
    pub length: Duration,
}

impl Square {
    /// The square for `skill` cast at `cell`, if it's a ground skill with an area.
    pub fn new(skill: u16, level: Option<u16>, cell: [u16; 2], start: Instant, length: Duration) -> Option<Self> {
        Some(Self { cell, width: width(radius(skill, level)?), start, length })
    }

    pub fn alive(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.start) < self.length
    }

    /// Its turn (radians) and size (times its width) at `now`.
    pub fn pose(&self, now: Instant) -> (f32, f32) {
        let seconds = now.saturating_duration_since(self.start).as_secs_f32();
        (seconds * TURN, 1.0 + PULSE * (seconds * PULSE_HZ * std::f32::consts::TAU).sin())
    }

    /// Triangles over the square in cell coordinates (fractional cells, the target cell's middle at its own
    /// coordinates), each corner with its texture coordinate: a grid of `steps` × `steps` so it can follow the ground.
    pub fn triangles(&self, now: Instant, steps: usize) -> Vec<([f32; 2], [f32; 2])> {
        let (angle, scale) = self.pose(now);
        let (sin, cos) = angle.sin_cos();
        let half = self.width * scale / 2.0;
        let corner = |column: usize, row: usize| {
            let uv = [column as f32 / steps as f32, row as f32 / steps as f32];
            let local = [(uv[0] * 2.0 - 1.0) * half, (uv[1] * 2.0 - 1.0) * half];
            let turned = [local[0] * cos - local[1] * sin, local[0] * sin + local[1] * cos];
            ([f32::from(self.cell[0]) + turned[0], f32::from(self.cell[1]) + turned[1]], uv)
        };
        (0..steps)
            .flat_map(|row| (0..steps).map(move |column| (column, row)))
            .flat_map(|(column, row)| {
                let [a, b, c, d] = [corner(column, row), corner(column + 1, row), corner(column, row + 1), corner(column + 1, row + 1)];
                [a, b, c, c, b, d]
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_from_the_area() {
        // Storm Gust: a 9×9 area, so 11 cells wide; Heaven's Drive 5×5, so 7.
        assert_eq!(radius(89, Some(10)).map(width), Some(11.0));
        assert_eq!(radius(91, None).map(width), Some(7.0));
        // Thunderstorm's level 11 reaches further; unknown levels take level 10.
        assert_eq!((radius(21, Some(11)), radius(21, None), radius(21, Some(1))), (Some(3), Some(2), Some(2)));
        // Not a ground skill (Bash), or one drawn its own way (Fire Wall).
        assert_eq!((radius(5, None), radius(18, None)), (None, None));
    }

    #[test]
    fn lives_for_the_cast_and_covers_its_width() {
        let start = Instant::now();
        let square = Square::new(89, None, [100, 50], start, Duration::from_millis(6000)).unwrap();
        assert!(square.alive(start + Duration::from_millis(5999)) && !square.alive(start + Duration::from_millis(6000)));
        // At the start it isn't turned or pulsed: corners 5.5 cells out from the middle.
        let triangles = square.triangles(start, 2);
        assert_eq!(triangles.len(), 2 * 2 * 6);
        assert_eq!(triangles[0], ([94.5, 44.5], [0.0, 0.0]));
        assert_eq!(triangles.last(), Some(&([105.5, 55.5], [1.0, 1.0])));
        // A quarter second later it has turned and grown.
        let (angle, scale) = square.pose(start + Duration::from_millis(250));
        assert!(angle > 0.3 && scale > 1.0);
    }
}
