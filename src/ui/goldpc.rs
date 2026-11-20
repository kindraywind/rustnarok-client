//! The Gold PC Cafe points (ZC_GOLDPCCAFE_POINT): a small box at the top of the screen with the time left to the next
//! point and the points so far by a coin. It has no frame and doesn't move; /goldpc shows or hides it, and a click asks
//! the server for its Gold PC Cafe NPC.

use std::time::Instant;

use rustnarok_client::grf::Grf;
use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};
use crate::window::{self, Chrome};
use crate::{Layer, UI, contains, offset, screen};

/// "You can accumulate points of Internet cafe Gold up to %d points maximum.", the hover's text.
const TIP: usize = 2619;
/// The points stop at this many; the time then shows as "-- : --".
const MAX_POINTS: i32 = 300;
/// A point for each hour played.
const HOUR: i32 = 3600;
/// The box's corner: this far left of the screen's right edge, the picture's width included, and down.
const FROM_RIGHT: f32 = 340.0;
const TOP: f32 = 12.0;
/// The box's size: the plain picture's (54 × 49), which the game sizes and places it by.
const SIZE: [f32; 2] = [54.0, 49.0];
/// The time's top, centred in the width; the points' right edge in from the right and their top up from the bottom.
const TIME_Y: f32 = 15.0;
const POINTS_RIGHT: f32 = 9.0;
const POINTS_UP: f32 = 16.0;
/// The NPC a click asks for.
pub const NPC: &str = "GOLDPCCAFE";

/// The plain box (mode 1) and the gold "x2" one (mode 2).
pub struct Assets {
    boxes: [Option<Texture>; 2],
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |mode: u8| gpu.try_load_texture(grf, &format!("{UI}basic_interface\\mileage_bg{mode}.bmp"), false).ok();
        Self { boxes: [image(1), image(2)] }
    }

    fn picture(&self, mode: i8) -> Option<&Texture> {
        self.boxes.get(usize::try_from(mode).ok()?.checked_sub(1)?)?.as_ref()
    }
}

pub struct GoldPc {
    /// Shown: set by each count (active and /goldpc on) and by /goldpc.
    pub shown: bool,
    pub mode: i8,
    pub points: i32,
    /// Seconds to the next point when the server last said, and when that was.
    left: i32,
    since: Instant,
}

impl GoldPc {
    /// The window as a ZC_GOLDPCCAFE_POINT leaves it (made by the first one): inactive hides it and keeps the rest;
    /// active takes the new numbers and shows it if /goldpc is on.
    pub fn update(window: Option<Self>, active: bool, mode: i8, points: i32, played: i32, switched_on: bool, now: Instant) -> Self {
        let mut window = window.unwrap_or(Self { shown: switched_on, mode: 1, points: 0, left: 0, since: now });
        window.shown = active && switched_on;
        if active {
            window.mode = mode;
            window.points = points;
            window.left = (HOUR - played).max(0);
            window.since = now;
        }
        window
    }

    /// Seconds to the next point: counted down by whole seconds from what the server said, stopping at 0.
    pub fn left_at(&self, now: Instant) -> i32 {
        let gone = now.saturating_duration_since(self.since).as_secs();
        (i64::from(self.left) - gone as i64).max(0) as i32
    }

    /// The time as the box shows it: minutes and seconds, or dashes once the points are full.
    pub fn time_text(&self, now: Instant) -> String {
        if self.points >= MAX_POINTS {
            return "-- : --".into();
        }
        let left = self.left_at(now);
        format!("{:02} : {:02}", left / 60, left % 60)
    }

    /// The box on screen when it shows.
    pub fn area(&self) -> Option<[f32; 4]> {
        self.shown.then_some([screen()[0] - SIZE[0] - FROM_RIGHT, TOP, SIZE[0], SIZE[1]])
    }

    pub fn covers(&self, cursor: [f32; 2]) -> bool {
        self.area().is_some_and(|[x, y, width, height]| contains([x, y], [width, height], cursor))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &self,
        layer: &mut Layer<'a>,
        tooltip: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        font: &Font,
        messages: &[String],
        cursor: [f32; 2],
        now: Instant,
    ) {
        let Some([x, y, width, height]) = self.area() else { return };
        if let Some(texture) = assets.picture(self.mode) {
            layer.sprites.push((texture, [x, y], texture.size));
        }
        let time = self.time_text(now);
        layer.outlined(&time, [x + ((width - font.width(&time)) / 2.0).floor(), y + TIME_Y]);
        let points = self.points.to_string();
        layer.outlined(&points, [x + width - font.width(&points) - POINTS_RIGHT, y + height - POINTS_UP]);
        if self.covers(cursor) {
            let tip = messages.get(TIP).map_or(String::new(), |text| text.replace("%d", &MAX_POINTS.to_string()));
            window::tooltip(tooltip, chrome, font, &tip, offset(cursor, [10.0, -20.0]), [255; 3]);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn counts_down_to_the_next_point() {
        let now = Instant::now();
        // 2446 s of the hour played: 19 min 14 s to go.
        let window = GoldPc::update(None, true, 1, 2, 2446, true, now);
        assert!(window.shown);
        assert_eq!(window.time_text(now), "19 : 14");
        assert_eq!(window.time_text(now + Duration::from_millis(1999)), "19 : 13");
        assert_eq!(window.left_at(now + Duration::from_secs(5000)), 0);
        assert_eq!(window.time_text(now + Duration::from_secs(5000)), "00 : 00");
        // Over an hour played still reads 0.
        assert_eq!(GoldPc::update(None, true, 1, 2, 4000, true, now).left_at(now), 0);
    }

    #[test]
    fn full_points_show_dashes() {
        let now = Instant::now();
        assert_eq!(GoldPc::update(None, true, 2, MAX_POINTS, 10, true, now).time_text(now), "-- : --");
        assert_eq!(GoldPc::update(None, true, 2, MAX_POINTS - 1, 3540, true, now).time_text(now), "01 : 00");
    }

    #[test]
    fn inactive_or_switched_off_hides_it() {
        let now = Instant::now();
        let window = GoldPc::update(None, true, 2, 7, 0, true, now);
        let window = GoldPc::update(Some(window), false, 1, 0, 0, true, now);
        // Hidden, the last numbers kept.
        assert!(!window.shown);
        assert_eq!((window.mode, window.points), (2, 7));
        assert!(!GoldPc::update(Some(window), true, 1, 8, 0, false, now).shown);
    }
}
