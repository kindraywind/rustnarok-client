//! Status icons: where each sits down the right side, and what one says when pointed at (its name in its colour, its
//! description, and the time it has left, in a small dark box beside it).

use rustnarok_client::lua::StateInfo;
use rustnarok_client::messages;

use crate::SCREEN;

/// "%d hour(s)", " %d minute(s)", " %d second(s)" (older data: "hour", "minute", "second" after the number).
const UNITS: [usize; 3] = [1697, 1807, 1808];
/// Lines with no colour of their own.
// ponytail: plain lines in white; the data gives them none.
const PLAIN: [u8; 3] = [255, 255, 255];
/// Padding left and on top, and between lines.
const PAD: [f32; 2] = [6.0, 4.0];

/// A status icon's size, and the grid: 45 between columns, 35 between rows, from (W - 48, 169).
pub const ICON: f32 = 32.0;
const STEP: [f32; 2] = [45.0, 35.0];
const FIRST: [f32; 2] = [48.0, 169.0];

/// Rows in a column on a screen this high: a column ends when 150 + 35 * row reaches the height less 50.
pub fn rows(height: f32) -> usize {
    ((height - 50.0 - 150.0) / STEP[1]).ceil().max(1.0) as usize
}

/// The `order`th icon's box corner (columns right to left, rows top down).
// ponytail: icons go in the order they came; the game sorts them in five priority passes from its icon table.
pub fn icon_corner(order: usize, [width, height]: [f32; 2]) -> [f32; 2] {
    let per = rows(height);
    [width - FIRST[0] - (order / per) as f32 * STEP[0], FIRST[1] + (order % per) as f32 * STEP[1]]
}

/// Whether the pointer is on an icon whose box is at `corner`: 30 by 30, from its left edge and one pixel up.
pub fn on_icon(corner: [f32; 2], pointer: [f32; 2]) -> bool {
    (corner[0]..corner[0] + 30.0).contains(&pointer[0]) && pointer[1] > corner[1] - 1.0 && pointer[1] <= corner[1] + 29.0
}

/// The time left as the tooltip says it: hours, minutes and seconds, each left out when 0.
pub fn time_text(messages: &[String], seconds: u64) -> String {
    let parts = [seconds / 3600, seconds / 60 % 60, seconds % 60];
    let mut text = String::new();
    for (value, &id) in parts.into_iter().zip(&UNITS) {
        if value == 0 {
            continue;
        }
        let unit = messages.get(id).map_or("", String::as_str);
        if unit.contains("%d") {
            text.push_str(&messages::fill(unit, &[&value]));
        } else {
            text.push_str(&format!(" {value} {}", unit.trim()));
        }
    }
    text.trim_start().to_owned()
}

/// The tooltip's lines and colours: the time line filled in, or left out when there's no time limit (`left` None).
pub fn lines(info: &StateInfo, left: Option<u64>, messages: &[String]) -> Vec<(String, [u8; 3])> {
    info.lines
        .iter()
        .filter_map(|(text, color)| {
            let text = if text.contains("%s") { text.replace("%s", &time_text(messages, left?)) } else { text.clone() };
            Some((text, color.unwrap_or(PLAIN)))
        })
        .collect()
}

/// The box beside an icon (`icon` its corner on screen): widest line + 12 wide, 4 + (line + 4) a line high; its right
/// edge on the icon's left, its top 3 below the icon's, pulled up to stay on screen.
pub fn place(icon: [f32; 2], widest: f32, count: usize, line_height: f32) -> ([f32; 2], [f32; 2]) {
    let size = [widest + 2.0 * PAD[0], PAD[1] + count as f32 * (line_height + PAD[1])];
    let corner = [icon[0] - size[0], (icon[1] + 3.0).min(SCREEN[1] - size[1]).max(0.0)];
    (corner, size)
}

/// Where a line's text goes in the box.
pub fn line_at(corner: [f32; 2], row: usize, line_height: f32) -> [f32; 2] {
    [corner[0] + PAD[0], corner[1] + PAD[1] + row as f32 * (line_height + PAD[1])]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn says_the_time_left_and_lines() {
        let mut messages = vec![String::new(); 1810];
        messages[1697] = "%d hour(s)".into();
        messages[1807] = " %d minute(s)".into();
        messages[1808] = " %d second(s)".into();
        assert_eq!(time_text(&messages, 3725), "1 hour(s) 2 minute(s) 5 second(s)");
        assert_eq!(time_text(&messages, 59), "59 second(s)");
        // Older data: the unit after the number.
        let mut older = messages.clone();
        older[1697] = "hour".into();
        older[1807] = "minute".into();
        older[1808] = "second".into();
        assert_eq!(time_text(&older, 3660), "1 hour 1 minute");
        let blessing = StateInfo {
            lines: vec![("Blessing".into(), Some([155, 202, 155])), ("%s".into(), Some([255, 176, 98])), ("Increases STR".into(), None)],
            time_limit: true,
        };
        assert_eq!(
            lines(&blessing, Some(59), &messages),
            [("Blessing".into(), [155, 202, 155]), ("59 second(s)".into(), [255, 176, 98]), ("Increases STR".into(), [255; 3])]
        );
        // No time limit: no time line.
        assert_eq!(lines(&blessing, None, &messages).len(), 2);
    }

    #[test]
    fn lays_icons_on_the_games_grid() {
        // 17 to a column at 768 high, 12 at 600; the 18th starts a column 45 to the left.
        assert_eq!((rows(768.0), rows(600.0)), (17, 12));
        assert_eq!(icon_corner(0, [1024.0, 768.0]), [976.0, 169.0]);
        assert_eq!(icon_corner(16, [1024.0, 768.0]), [976.0, 169.0 + 16.0 * 35.0]);
        assert_eq!(icon_corner(17, [1024.0, 768.0]), [931.0, 169.0]);
        assert_eq!(icon_corner(16, [800.0, 600.0]), [707.0, 169.0 + 4.0 * 35.0]);
        assert_eq!(icon_corner(17, [800.0, 600.0]), [707.0, 169.0 + 5.0 * 35.0]);
        // The hit box: x in [976, 1006), y in (168, 198].
        let corner = icon_corner(0, [1024.0, 768.0]);
        assert!(on_icon(corner, [976.0, 198.0]) && on_icon(corner, [1005.9, 168.5]));
        assert!(!on_icon(corner, [1006.0, 180.0]) && !on_icon(corner, [980.0, 168.0]) && !on_icon(corner, [980.0, 198.5]));
    }

    #[test]
    fn sits_left_of_the_icon_on_screen() {
        // Three 12px lines, 100 wide, beside an icon at (956, 166).
        let (corner, size) = place([956.0, 166.0], 100.0, 3, 12.0);
        assert_eq!((corner, size), ([844.0, 169.0], [112.0, 52.0]));
        assert_eq!(line_at(corner, 1, 12.0), [850.0, 189.0]);
        // Near the bottom it's pulled up.
        let (corner, size) = place([956.0, 740.0], 100.0, 3, 12.0);
        assert_eq!(corner[1] + size[1], SCREEN[1]);
    }
}
