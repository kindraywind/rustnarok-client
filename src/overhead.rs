//! What shows over units' heads: when a skill's name is called, and the balloon that holds it and chat.

/// Skills whose name never shows: Frost Joke, Scream, Enchant Arms (from items).
const SILENT: [u16; 3] = [318, 326, 492];
/// Skills a player (or NPC, homunculus...) uses without calling them: identifying, shops, hiding and cloaking, and a
/// few follow-up attacks.
const QUIET: [u16; 13] = [40, 41, 51, 135, 389, 2033, 2247, 2290, 2342, 2535, 3001, 5382, 5383];
/// Called when they land, not when the cast starts: Brandish Spear, Bowling Bash.
const CALLED_ON_LANDING: [u16; 2] = [57, 62];
/// Sight (MG_SRECOVERY's id is 9): a non-monster doesn't call it when used.
const UNCALLED_USE: u16 = 9;

/// The packet a skill comes in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shout {
    /// A cast starts (ZC_USESKILL_ACK).
    Cast,
    /// A skill without damage (ZC_USE_SKILL).
    Used,
    /// A skill hit or a ground skill (ZC_NOTIFY_SKILL, ZC_NOTIFY_GROUNDSKILL).
    Hit,
}

/// Whether a unit calls a skill's name over its head. A level below 0 marks a silent follow-up hit. Monsters call
/// only skills without damage; everyone else calls every skill but the quiet ones, so a skill with a cast time is
/// called twice: as the cast starts and as it lands.
pub fn shouts(skill: u16, level: i32, monster: bool, packet: Shout) -> bool {
    if level < 0 || SILENT.contains(&skill) {
        return false;
    }
    if monster {
        return packet == Shout::Used;
    }
    let quiet = QUIET.contains(&skill) || (packet == Shout::Cast && CALLED_ON_LANDING.contains(&skill)) || (packet == Shout::Used && skill == UNCALLED_USE);
    !quiet
}

/// How far over the feet the balloon's bottom (and the cast bar's top) sits, in sprite pixels straight up the screen.
pub const HEAD: f32 = 81.0;
/// How far over the feet the cast bar's top sits, in sprite pixels.
pub const CAST_ABOVE: f32 = 96.7;
/// The cast bar: 60 by 6, centred on the head.
pub const CAST_BAR: [f32; 2] = [60.0, 6.0];

/// The cast bar's box with its top centre at `point` (whole pixels).
pub fn cast_rect(point: [f32; 2]) -> [f32; 4] {
    [(point[0] - CAST_BAR[0] / 2.0).round(), point[1].round(), CAST_BAR[0], CAST_BAR[1]]
}

/// How wide the fill is `done` of the way through the cast: trunc(60 * pct / 100 - 2) with pct a whole percent.
pub fn cast_fill(done: f32) -> f32 {
    let percent = (done.clamp(0.0, 1.0) * 100.0).floor();
    (CAST_BAR[0] * percent / 100.0 - 2.0).trunc().max(0.0)
}

/// How far under the feet HP/SP bars start, and names under them, in sprite pixels.
pub const BAR_BELOW: f32 = 12.0;
pub const NAME_BELOW: f32 = 20.0;
/// Bytes per balloon line (a double-byte character counts two).
const LINE_BYTES: usize = 35;
/// The balloon's padding around its text: left, top; and the step from line to line.
pub const TEXT_AT: [f32; 2] = [6.0, 4.0];
// ponytail: a 12 px line plus 4; the line height of the 12 px font is an estimate.
pub const LINE_STEP: f32 = 16.0;
/// How far a balloon may stick out past the screen's edges.
const OVER_EDGE: f32 = 3.0;

/// A balloon's lines: broken at newlines and spaces to at most `LINE_BYTES` bytes, counting a character beyond ASCII
/// as two; a longer word is cut.
pub fn lines(text: &str) -> Vec<String> {
    let width = |text: &str| text.chars().map(|character| if character.is_ascii() { 1 } else { 2 }).sum::<usize>();
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph.split(' ') {
            let joined = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
            if width(&joined) <= LINE_BYTES {
                line = joined;
                continue;
            }
            if !line.is_empty() {
                lines.push(std::mem::take(&mut line));
            }
            for character in word.chars() {
                if width(&line) + width(&character.to_string()) > LINE_BYTES {
                    lines.push(std::mem::take(&mut line));
                }
                line.push(character);
            }
        }
        lines.push(line);
    }
    lines
}

/// A balloon's size for lines `widest` px wide at most.
pub fn size(widest: f32, count: usize) -> [f32; 2] {
    [widest.ceil() + 2.0 * TEXT_AT[0], TEXT_AT[1] + LINE_STEP * count as f32]
}

/// The balloon's top-left corner: its bottom centred on `anchor`, kept on a `screen` (allowing `OVER_EDGE` past it).
pub fn corner(anchor: [f32; 2], size: [f32; 2], screen: [f32; 2]) -> [f32; 2] {
    let place = |at: f32, axis: usize| at.clamp(-OVER_EDGE, screen[axis] - size[axis] + OVER_EDGE).round();
    [place(anchor[0] - (size[0] / 2.0).floor(), 0), place(anchor[1] - size[1], 1)]
}

/// A name's top-left corner: centred under `point`, kept on a `screen`.
pub fn label_corner(point: [f32; 2], width: f32, height: f32, screen: [f32; 2]) -> [f32; 2] {
    let place = |at: f32, size: f32, axis: usize| at.clamp(0.0, (screen[axis] - size).max(0.0)).round();
    [place(point[0] - width / 2.0, width, 0), place(point[1], height, 1)]
}

/// The balloon's border pixels as rectangles (corner, size): edges with each corner cut by 3 px on a diagonal.
pub fn border([width, height]: [f32; 2]) -> Vec<([f32; 2], [f32; 2])> {
    const CUT: f32 = 3.0;
    let mut pieces = vec![
        ([CUT, 0.0], [width - 2.0 * CUT, 1.0]),
        ([CUT, height - 1.0], [width - 2.0 * CUT, 1.0]),
        ([0.0, CUT], [1.0, height - 2.0 * CUT]),
        ([width - 1.0, CUT], [1.0, height - 2.0 * CUT]),
    ];
    for step in 1..CUT as usize {
        let (a, b) = (step as f32, CUT - step as f32);
        pieces.extend([[a, b], [width - 1.0 - a, b], [a, height - 1.0 - b], [width - 1.0 - a, height - 1.0 - b]].map(|at| (at, [1.0, 1.0])));
    }
    pieces
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_the_cast_bar() {
        assert_eq!(cast_rect([512.0, 300.4]), [482.0, 300.0, 60.0, 6.0]);
        assert_eq!([0.0, 0.01, 0.5, 0.99, 1.0, 1.5].map(cast_fill), [0.0, 0.0, 28.0, 57.0, 58.0, 58.0]);
    }

    #[test]
    fn who_calls_which_skill() {
        // Heal: called as it's cast and as it's used, by players; a monster only as it's used.
        assert!(shouts(28, 10, false, Shout::Cast) && shouts(28, 10, false, Shout::Used));
        assert!(!shouts(28, 10, true, Shout::Cast) && shouts(28, 10, true, Shout::Used));
        // A monster's bolt hits without its name; a player's calls it.
        assert!(!shouts(19, 5, true, Shout::Hit) && shouts(19, 5, false, Shout::Hit));
        // Silent follow-ups, hiding, Frost Joke.
        assert!(!shouts(19, -1, false, Shout::Hit));
        assert!(!shouts(51, 1, false, Shout::Used) && shouts(51, 1, true, Shout::Used));
        assert!(!shouts(318, 5, true, Shout::Used) && !shouts(318, 5, false, Shout::Used));
        // Bowling Bash only as it lands.
        assert!(!shouts(62, 10, false, Shout::Cast) && shouts(62, 10, false, Shout::Hit));
        assert!(!shouts(9, 1, false, Shout::Used));
    }

    #[test]
    fn lines_break_at_35_bytes() {
        assert_eq!(lines("Heal !!"), ["Heal !!"]);
        let chat = "McpTester : the quick brown fox jumps over the lazy dog";
        assert_eq!(lines(chat), ["McpTester : the quick brown fox", "jumps over the lazy dog"]);
        assert!(lines(chat).iter().all(|line| line.len() <= 35));
        // Hangul counts two a character: 17 fit.
        assert_eq!(lines(&"가".repeat(20)), ["가".repeat(17), "가".repeat(3)]);
        assert_eq!(lines("a\nb"), ["a", "b"]);
    }

    #[test]
    fn balloons_sit_on_the_anchor_and_stay_on_screen() {
        let screen = [1024.0, 768.0];
        // One line 40 px wide: 52 by 20, its bottom on the anchor.
        assert_eq!(size(40.0, 1), [52.0, 20.0]);
        assert_eq!(corner([500.0, 300.0], [52.0, 20.0], screen), [474.0, 280.0]);
        // At the edges: at most 3 px past them.
        assert_eq!(corner([5.0, 10.0], [52.0, 20.0], screen), [-3.0, -3.0]);
        assert_eq!(corner([1020.0, 300.0], [52.0, 20.0], screen), [975.0, 280.0]);
    }

    #[test]
    fn names_stay_on_screen() {
        let screen = [1024.0, 768.0];
        assert_eq!(label_corner([500.0, 300.0], 60.0, 15.0, screen), [470.0, 300.0]);
        assert_eq!(label_corner([10.0, 760.0], 60.0, 15.0, screen), [0.0, 753.0]);
        assert_eq!(label_corner([1020.0, -5.0], 60.0, 15.0, screen), [964.0, 0.0]);
    }

    #[test]
    fn border_corners_are_cut() {
        let pieces = border([52.0, 20.0]);
        let covers = |[x, y]: [f32; 2]| pieces.iter().any(|([left, top], [width, height])| x >= *left && x < left + width && y >= *top && y < top + height);
        assert!(covers([3.0, 0.0]) && covers([0.0, 3.0]) && covers([2.0, 1.0]) && covers([1.0, 2.0]));
        assert!(!covers([0.0, 0.0]) && !covers([1.0, 1.0]) && !covers([51.0, 19.0]) && !covers([5.0, 5.0]));
        assert!(covers([49.0, 18.0]) && covers([50.0, 17.0]) && !covers([50.0, 18.0]) && covers([48.0, 19.0]) && covers([51.0, 16.0]));
    }
}
