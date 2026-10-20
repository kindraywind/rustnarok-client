//! Wide Bleeding's red wash (669): one flat white square facing the camera, a 16 ms tick at a time.
//!
//! A sheet's own age n counts from 1 on its first drawn tick.

use std::time::Instant;

use super::TICK;
use crate::world::Vector;

pub const TEXTURE: &str = "data\\texture\\effect\\white02.bmp";
/// The effect's length in ticks (`length 669`), the sheet's life; it is drawn on all but the last.
// ponytail: the life is copied from effects.txt; the table's length would replace it if the sheet were made from a row.
const LIFE: u32 = 70;
/// Half its side in world units, how far toward the camera it is moved, and how much higher it starts (the first tick only).
const HALF: f32 = 200.0;
const TOWARD: f32 = 10.0;
const LIFT: f32 = 9.0;
/// Sprite pixels a world unit is.
const PIXELS_PER_UNIT: f32 = 7.0;
/// Where its texture's corners go: top left, top right, bottom right, bottom left.
const UV: [[f32; 2]; 4] = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];

/// One sheet on a unit (None = the player).
struct Sheet {
    unit: Option<u32>,
    start: Instant,
}

/// A sheet to draw: the point it is centred on, its corners in sprite pixels (y down), their texture coordinates and its colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SheetDraw {
    pub at: Vector,
    pub corners: [[f32; 2]; 4],
    pub uv: [[f32; 2]; 4],
    pub tint: [f32; 4],
}

#[derive(Default)]
pub struct Sheets {
    sheets: Vec<Sheet>,
}

/// A sheet's alpha (0 to 255) at its age n: 10 more a tick to 50, held to n = 65, then 10 less a tick.
pub fn sheet_alpha(n: u32) -> f32 {
    let alpha = match n {
        1..=5 => 10 * n,
        6..=65 => 50,
        66..=69 => 50 - 10 * (n - 65),
        _ => 0,
    };
    alpha as f32
}

impl Sheets {
    /// A sheet over `unit`, made at `now`.
    pub fn start(&mut self, unit: Option<u32>, now: Instant) {
        self.sheets.push(Sheet { unit, start: now });
    }

    /// Drops the sheets that have run their life.
    pub fn advance(&mut self, now: Instant) {
        self.sheets.retain(|sheet| tick(sheet.start, now) < LIFE);
    }

    /// The sheets at `now`: through their unit's feet, 10 units along `toward` (the unit vector from the scene to the camera).
    pub fn draws(&self, now: Instant, toward: Vector, position_of: impl Fn(Option<u32>) -> Option<Vector>) -> Vec<SheetDraw> {
        let side = HALF * PIXELS_PER_UNIT;
        self.sheets
            .iter()
            .filter_map(|sheet| {
                let age = tick(sheet.start, now);
                let (feet, n) = (position_of(sheet.unit)?, age + 1);
                if n >= LIFE {
                    return None;
                }
                let lift = if age == 0 { LIFT } else { 0.0 };
                let at = [feet[0] + toward[0] * TOWARD, feet[1] + lift + toward[1] * TOWARD, feet[2] + toward[2] * TOWARD];
                let corners = [[-side, -side], [side, -side], [side, side], [-side, side]];
                Some(SheetDraw { at, corners, uv: UV, tint: [1.0, 0.0, 0.0, sheet_alpha(n) / 255.0] })
            })
            .collect()
    }
}

/// The ticks from `start` to `now`: 0 on the tick it is made.
fn tick(start: Instant, now: Instant) -> u32 {
    (now.saturating_duration_since(start).as_millis() / TICK.as_millis()) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(got: f32, want: f32) -> bool {
        (got - want).abs() < 1e-3
    }

    #[test]
    fn the_sheet_rises_to_50_holds_and_falls() {
        // Alpha at n = 1, 5, 6, 65, 66, 69: 10, 50, 50, 50, 40, 10; none before the first or after the last.
        assert_eq!([1, 5, 6, 65, 66, 69].map(sheet_alpha), [10.0, 50.0, 50.0, 50.0, 40.0, 10.0]);
        assert_eq!([0, 70].map(sheet_alpha), [0.0, 0.0]);
        assert_eq!([66, 67, 68].map(sheet_alpha), [40.0, 30.0, 20.0]);
    }

    #[test]
    fn it_is_drawn_69_times_and_gone_at_70() {
        let start = Instant::now();
        let mut sheets = Sheets::default();
        sheets.start(Some(3), start);
        let feet = |unit: Option<u32>| (unit == Some(3)).then_some([100.0, 2.0, 50.0]);
        let drawn: Vec<u32> = (0..80).filter(|tick| !sheets.draws(start + TICK * *tick, [0.0, 0.0, 1.0], feet).is_empty()).collect();
        assert_eq!(drawn, (0..69).collect::<Vec<u32>>());
        // Alive for its 70 ticks, gone at 70.
        sheets.advance(start + TICK * 69);
        assert_eq!(sheets.sheets.len(), 1);
        sheets.advance(start + TICK * 70);
        assert!(sheets.sheets.is_empty());
        // The alpha it draws at those ticks: n = tick + 1.
        sheets.start(Some(3), start);
        let alpha = |tick: u32| sheets.draws(start + TICK * tick, [0.0, 0.0, 1.0], feet)[0].tint[3] * 255.0;
        assert!([0, 4, 5, 64, 65, 68].iter().zip([10.0, 50.0, 50.0, 50.0, 40.0, 10.0]).all(|(tick, want)| near(alpha(*tick), want)));
    }

    #[test]
    fn it_is_a_red_square_of_400_units_10_toward_the_camera() {
        let start = Instant::now();
        let mut sheets = Sheets::default();
        sheets.start(Some(3), start);
        let feet = |unit: Option<u32>| (unit == Some(3)).then_some([100.0, 2.0, 50.0]);
        // The camera is toward −z and up: the unit vector from the scene to it.
        let toward = [0.0, 0.6, -0.8];
        let first = sheets.draws(start, toward, feet)[0];
        // The first tick is 9 units higher; both are 10 along `toward`.
        assert!(near(first.at[0], 100.0) && near(first.at[1], 2.0 + 9.0 + 6.0) && near(first.at[2], 50.0 - 8.0));
        let later = sheets.draws(start + TICK, toward, feet)[0];
        assert!(near(later.at[1], 2.0 + 6.0) && near(later.at[2], 50.0 - 8.0));
        // 400 × 400 units: corners at ±200, 7 pixels a unit; the uv over them (0, 1), (1, 1), (1, 0), (0, 0).
        assert_eq!(later.corners, [[-1400.0, -1400.0], [1400.0, -1400.0], [1400.0, 1400.0], [-1400.0, 1400.0]]);
        assert_eq!(later.uv, [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]]);
        // #FF0000.
        assert_eq!(later.tint[..3], [1.0, 0.0, 0.0]);
        // It follows the unit; one that is gone shows nothing.
        let moved = |unit: Option<u32>| (unit == Some(3)).then_some([0.0, 0.0, 0.0]);
        assert!(near(sheets.draws(start + TICK, toward, moved)[0].at[2], -8.0));
        assert!(sheets.draws(start + TICK, toward, |_| None).is_empty());
    }
}
