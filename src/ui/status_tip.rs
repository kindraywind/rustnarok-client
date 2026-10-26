//! Status icons: where each sits down the right side, and what one says when pointed at (its name in its colour, its
//! description, and the time it has left, in a small dark box beside it).

use rustnarok_client::lua::StateInfo;
use rustnarok_client::messages;

use crate::screen;

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

/// The most icons shown.
const MOST_ICONS: usize = 99;
/// Increase AGI's two status ids: 42 is shown as 12.
const AGI_SHOWN_AS: (u16, u16) = (42, 12);

/// The icons in the order they're placed: five passes, priority 0 to 4 from the icon table (`priority` of a status,
/// None when it has no icon), each taking its statuses in the order they started; 42 as 12, each once; at most 99.
// ponytail: the icons known without the Lua table come with priority 0, and a priority past 4 goes after the fifth
// pass.
pub fn placed<T: Copy>(icons: &[(u16, T)], priority: impl Fn(u16) -> Option<u8>) -> Vec<(u16, T)> {
    let mut seen = Vec::new();
    let shown: Vec<(u16, T, u8)> = icons
        .iter()
        .map(|&(id, data)| (if id == AGI_SHOWN_AS.0 { AGI_SHOWN_AS.1 } else { id }, data))
        .filter_map(|(id, data)| {
            let priority = priority(id)?;
            (!seen.contains(&id)).then(|| {
                seen.push(id);
                (id, data, priority)
            })
        })
        .collect();
    let mut order: Vec<(u16, T)> = Vec::new();
    for pass in 0..=4 {
        order.extend(shown.iter().filter(|(.., priority)| *priority == pass).map(|&(id, data, _)| (id, data)));
    }
    order.extend(shown.iter().filter(|(.., priority)| *priority > 4).map(|&(id, data, _)| (id, data)));
    order.truncate(MOST_ICONS);
    order
}

/// What sprites a status icon draws: always its icon sprite; its timer clock only when timed and not blind.
pub fn icon_sprites(end: Option<std::time::Instant>, blind: bool) -> (bool, bool) {
    (true, end.is_some() && !blind)
}

/// The `order`th icon's box corner (columns right to left, rows top down).
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
    let corner = [icon[0] - size[0], (icon[1] + 2.0).min(screen()[1] - size[1]).max(0.0)];
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
    fn places_icons_by_priority_passes() {
        let table = |id: u16| match id {
            10 => Some(0), // Blessing
            12 => Some(0), // Increase AGI
            30 => Some(2),
            40 => Some(1),
            50 => Some(9),
            _ => None,
        };
        // Started in any order, they come out the same: pass 0 (in start order), 1, 2, then past 4.
        let started = [(30, 'a'), (50, 'b'), (12, 'c'), (40, 'd'), (10, 'e'), (77, 'f')];
        assert_eq!(placed(&started, table).iter().map(|(id, _)| *id).collect::<Vec<_>>(), [12, 10, 40, 30, 50]);
        let other = [(10, 'e'), (40, 'd'), (30, 'a'), (12, 'c'), (50, 'b')];
        assert_eq!(placed(&other, table).iter().map(|(id, _)| *id).collect::<Vec<_>>(), [10, 12, 40, 30, 50]);
        // 42 is 12's icon, and once only.
        assert_eq!(placed(&[(42, 'x'), (12, 'y')], table), [(12, 'x')]);
        // At most 99.
        let many: Vec<(u16, ())> = (0..150).map(|id| (id, ())).collect();
        assert_eq!(placed(&many, |_| Some(0)).len(), 99);
    }

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
        assert_eq!((corner, size), ([844.0, 168.0], [112.0, 52.0]));
        assert_eq!(line_at(corner, 1, 12.0), [850.0, 188.0]);
        // The top is the icon's top + 2: 171 for the first row's icons at 169.
        assert_eq!(place([956.0, 169.0], 100.0, 1, 12.0).0[1], 171.0);
        // Near the bottom it's pulled up.
        let (corner, size) = place([956.0, 740.0], 100.0, 3, 12.0);
        assert_eq!(corner[1] + size[1], screen()[1]);
    }

    #[test]
    fn hides_clock_sprites_while_blind() {
        use crate::effects::{BLIND_BIT, DEEP_BLIND};
        let now = std::time::Instant::now();
        let end = Some(now + std::time::Duration::from_secs(60));
        let is_blind = |health_state: u16, status: u16| health_state & BLIND_BIT != 0 || status == DEEP_BLIND;
        // While blind (bit 0x10 and status 1205, each alone) clock sprites are left out; icon sprites are drawn in both cases.
        assert_eq!(icon_sprites(end, is_blind(BLIND_BIT, 0)), (true, false));
        assert_eq!(icon_sprites(end, is_blind(0, DEEP_BLIND)), (true, false));
        assert_eq!(icon_sprites(end, is_blind(BLIND_BIT, DEEP_BLIND)), (true, false));
        assert_eq!(icon_sprites(end, is_blind(0, 0)), (true, true));
        // Without an end time, no clock sprite is drawn even when not blind.
        assert_eq!(icon_sprites(None, is_blind(0, 0)), (true, false));
    }
}
