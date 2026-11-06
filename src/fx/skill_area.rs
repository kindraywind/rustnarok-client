//! The red cells a cast will hit, as the server shows them (ZC_SKILL_SCALE, rathena's ShowScale skills): a block of
//! cells around the caster's cell, its size the skill's `SkillScale` at the cast's level. Each cell is a soft red tile
//! added onto the ground, pulsing for the cast time; it stays for that time even if the caster goes.

use std::time::{Duration, Instant};

pub struct Area {
    pub cell: [u16; 2],
    /// Cells across and down (x, y).
    pub size: [u16; 2],
    pub start: Instant,
    pub length: Duration,
}

impl Area {
    pub fn alive(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.start) < self.length
    }

    /// The cells it covers: `size` across and down, from half the size before the caster's cell.
    pub fn cells(&self) -> impl Iterator<Item = [i32; 2]> + use<> {
        let ([x, y], [across, down]) = (self.cell.map(i32::from), self.size.map(i32::from));
        (0..across).flat_map(move |column| (0..down).map(move |row| [x - across / 2 + column, y - down / 2 + row]))
    }

    /// Its alpha (0–1): 150 of 255 at first, down 2 a 16 ms tick to 50, then up to 250 and down to 50 again, and so on.
    pub fn alpha(&self, now: Instant) -> f32 {
        let ticks = (now.saturating_duration_since(self.start).as_millis() / crate::prims::TICK.as_millis()) as i32;
        let value = match ticks {
            0..=50 => 150 - 2 * ticks,
            _ => match (ticks - 50) % 200 {
                rise @ 0..=100 => 50 + 2 * rise,
                fall => 250 - 2 * (fall - 100),
            },
        };
        value as f32 / 255.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn covers_the_block_around_the_caster() {
        let area = Area { cell: [120, 86], size: [5, 3], start: Instant::now(), length: Duration::from_secs(5) };
        let cells: Vec<_> = area.cells().collect();
        assert_eq!(cells.len(), 15);
        assert_eq!((cells[0], cells[14]), ([118, 85], [122, 87]));
        // An even size reaches one cell further before the caster than after.
        let even = Area { size: [2, 2], ..area };
        assert_eq!(even.cells().collect::<Vec<_>>(), [[119, 85], [119, 86], [120, 85], [120, 86]]);
    }

    #[test]
    fn pulses_between_50_and_250() {
        let start = Instant::now();
        let area = Area { cell: [0, 0], size: [1, 1], start, length: Duration::from_secs(10) };
        let at = |ticks: u64| (area.alpha(start + crate::prims::TICK * ticks as u32) * 255.0).round() as i32;
        assert_eq!([at(0), at(1), at(50), at(51), at(150), at(151), at(250), at(251)], [150, 148, 50, 52, 250, 248, 50, 52]);
        assert!(area.alive(start + Duration::from_millis(9_999)) && !area.alive(start + Duration::from_secs(10)));
    }
}
