//! Quest news: a new quest's line and its icon in the bottom right corner, a hunt's progress line and banner across
//! the screen, and a removed quest's line.

use std::time::{Duration, Instant};

use rustnarok_client::grf::Grf;
use rustnarok_client::messages;
use rustnarok_client::net::{Hunt, Quest};
use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};
use crate::{Layer, UI, contains, overhead, screen};

/// "You have received a new quest.", "[%s] quest - defeating [%s] progress (%d/%d).", "[%s] quest was deleted."
pub const RECEIVED: usize = 1374;
pub const PROGRESS: usize = 1610;
pub const DELETED: usize = 1611;
/// "Complete", for a hunt done.
const COMPLETE: usize = 2030;
pub const YELLOW: [u8; 3] = [255, 255, 0];
const BANNER_LIFE: Duration = Duration::from_secs(2);
/// The banner's strip, 130/480 of the way down.
fn banner_top() -> f32 {
    screen()[1] * 130.0 / 480.0
}
/// A message's rounded box: the text 6 in and 4 down, lines a line height and 4 apart, boxes 10 apart.
// ponytail: the box's size follows the status tooltip's rule: text width + 12, 4 + (line height + 4) a line.
const BANNER_TEXT: [f32; 2] = [6.0, 4.0];
const BANNER_GAP: f32 = 10.0;

/// Each banner's box (x, y, width, height), stacked down the strip from its top, centred across `screen_width`.
pub fn banner_boxes(widths: &[(f32, usize)], line_height: f32, top: f32, screen_width: f32) -> Vec<[f32; 4]> {
    let mut y = top;
    widths
        .iter()
        .map(|&(width, lines)| {
            let size = [width + 2.0 * BANNER_TEXT[0], BANNER_TEXT[1] + lines as f32 * (line_height + 4.0)];
            let corner = [((screen_width - size[0]) / 2.0).floor(), y];
            y += size[1] + BANNER_GAP;
            [corner[0], corner[1], size[0], size[1]]
        })
        .collect()
}
const ICON: [f32; 2] = [43.0, 43.0];

/// A quest from ZC_ADD_QUEST_EX: a new one goes last; one already known is refreshed where it stands. True when new.
pub fn add(quests: &mut Vec<Quest>, quest: Quest) -> bool {
    match quests.iter_mut().find(|known| known.id == quest.id) {
        Some(known) => {
            *known = quest;
            false
        }
        None => {
            quests.push(quest);
            true
        }
    }
}

/// A hunt's progress line in the chat.
pub fn progress_line(messages: &[String], title: &str, hunt: &Hunt) -> String {
    messages::fill(messages.get(PROGRESS).map_or("", String::as_str), &[&title, &hunt.name, &hunt.count, &hunt.needed])
}

/// A hunt's banner: "<monster> [ n / m ]", or "[ Complete ]" when done.
pub fn banner_line(messages: &[String], hunt: &Hunt) -> String {
    if hunt.count >= hunt.needed {
        format!("{} [ {} ]", hunt.name, messages.get(COMPLETE).map_or("", String::as_str))
    } else {
        format!("{} [ {} / {} ]", hunt.name, hunt.count, hunt.needed)
    }
}

/// The banners showing, each until its time is up.
#[derive(Default)]
pub struct Banners(Vec<(String, Instant)>);

impl Banners {
    pub fn show(&mut self, text: String, now: Instant) {
        self.0.push((text, now + BANNER_LIFE));
    }

    /// Drops those whose two seconds are over; the rest, oldest first.
    pub fn current(&mut self, now: Instant) -> Vec<String> {
        self.0.retain(|(_, until)| *until > now);
        self.0.iter().map(|(text, _)| text.clone()).collect()
    }

    /// The banners' lines (for the control state).
    pub fn texts(&self) -> Vec<String> {
        self.0.iter().map(|(text, _)| text.clone()).collect()
    }

    pub fn draw<'a>(&self, layer: &mut Layer<'a>, assets: &'a Assets, font: &Font) {
        let lines: Vec<Vec<&str>> = self.0.iter().map(|(text, _)| text.lines().collect()).collect();
        let widths: Vec<(f32, usize)> = lines.iter().map(|lines| (lines.iter().map(|line| font.width(line)).fold(0.0, f32::max), lines.len().max(1))).collect();
        for (lines, [x, y, w, h]) in lines.iter().zip(banner_boxes(&widths, font.line_height(), banner_top(), screen()[0])) {
            layer.sprites.push((&assets.banner, [x + 1.0, y + 1.0], [w - 2.0, h - 2.0]));
            for (edge, piece) in overhead::border([w, h]) {
                layer.sprites.push((&assets.banner_edge, [x + edge[0], y + edge[1]], piece));
            }
            for (row, line) in lines.iter().enumerate() {
                let at = [x + BANNER_TEXT[0], y + BANNER_TEXT[1] + row as f32 * (font.line_height() + 4.0)];
                layer.texts.push((line.to_string(), [at[0] + 1.0, at[1] + 1.0], [0, 0, 0]));
                layer.texts.push((line.to_string(), at, YELLOW));
            }
        }
    }
}

/// The new-quest icon's corner: the bottom right, where the base level button also sits.
pub fn icon_corner() -> [f32; 2] {
    [screen()[0] - ICON[0], screen()[1] - ICON[1]]
}

pub fn on_icon(cursor: [f32; 2]) -> bool {
    contains(icon_corner(), ICON, cursor)
}

pub struct Assets {
    /// The new-quest icon, out and pressed.
    icon: [Texture; 2],
    banner: Texture,
    banner_edge: Texture,
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |name: &str| gpu.try_load_texture(grf, &format!("{UI}basic_interface\\{name}.bmp"), false).unwrap_or_else(|_| gpu.solid([0, 0, 0, 0]));
        // The banner's box as the announcement bar's: black at a quarter opacity inside a grey border.
        Self { icon: [image("quest_off"), image("quest_on")], banner: gpu.solid([0, 0, 0, 0x40]), banner_edge: gpu.solid([92, 92, 92, 255]) }
    }
}

pub fn draw_icon<'a>(layer: &mut Layer<'a>, assets: &'a Assets, (pointer, down): ([f32; 2], bool)) {
    let look = &assets.icon[usize::from(down && on_icon(pointer))];
    layer.sprites.push((look, icon_corner(), ICON));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quest(id: u32) -> Quest {
        Quest { id, active: true, ..Default::default() }
    }

    #[test]
    fn stacks_banner_boxes_10_apart() {
        // Two one-line banners, 100 and 40 wide, with a 14-high font: 112 x 22 and 52 x 22, the second 10 under.
        let boxes = banner_boxes(&[(100.0, 1), (40.0, 1)], 14.0, 200.0, 1024.0);
        assert_eq!(boxes, [[456.0, 200.0, 112.0, 22.0], [486.0, 232.0, 52.0, 22.0]]);
        // Two lines: 4 + 2 x 18.
        assert_eq!(banner_boxes(&[(60.0, 2)], 14.0, 0.0, 1024.0)[0][3], 40.0);
    }

    #[test]
    fn refreshes_a_known_quest_where_it_stands() {
        let mut quests = vec![quest(1), quest(2), quest(3)];
        assert!(!add(&mut quests, Quest { end: 99, ..quest(2) }));
        assert_eq!(quests.iter().map(|quest| quest.id).collect::<Vec<_>>(), [1, 2, 3]);
        assert_eq!(quests[1].end, 99);
        assert!(add(&mut quests, quest(4)));
        assert_eq!(quests.last().unwrap().id, 4);
    }

    #[test]
    fn says_progress_and_keeps_banners_two_seconds() {
        let mut messages = vec![String::new(); 2031];
        messages[PROGRESS] = "[%s] quest - defeating [%s] progress (%d/%d).".into();
        messages[COMPLETE] = "Complete".into();
        let hunt = Hunt { id: 1, name: "Drops".into(), count: 3, needed: 10, levels: [0, 0] };
        assert_eq!(progress_line(&messages, "Ninja training", &hunt), "[Ninja training] quest - defeating [Drops] progress (3/10).");
        assert_eq!(banner_line(&messages, &hunt), "Drops [ 3 / 10 ]");
        assert_eq!(banner_line(&messages, &Hunt { count: 10, ..hunt }), "Drops [ Complete ]");
        let start = Instant::now();
        let mut banners = Banners::default();
        banners.show("Drops [ 3 / 10 ]".into(), start);
        assert_eq!(banners.current(start + Duration::from_millis(1_999)).len(), 1);
        assert!(banners.current(start + BANNER_LIFE).is_empty());
    }
}
