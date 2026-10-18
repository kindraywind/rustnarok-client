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
    /// A skill hit (ZC_NOTIFY_SKILL).
    Hit,
    /// A skill on the ground (ZC_NOTIFY_GROUNDSKILL).
    Ground,
}

/// A monster for who calls what: job 1001–3998 or 20021–31999, whatever the object type (pets and elementals are;
/// homunculi, mercenaries and NPCs with other jobs aren't).
pub fn monster_job(job: u16) -> bool {
    matches!(job, 1001..=3998 | 20021..=31999)
}

/// The words a unit calls for a skill: "<name> !!"; for our own wedding skills the partner's name first ("<partner>
/// !!  <words>", Call Partner ending in ♡); the two Summoner follow-ups by their first skill's name. None for a skill
/// without a name.
// ponytail: the partner's name only when we cast them (whose name another couple's shows isn't known), and Call
// Partner's name is the partner's too.
pub fn shout_text(skill: u16, name_of: impl Fn(u16) -> Option<String>, partner: Option<&str>, protect_words: &str) -> Option<String> {
    let skill = match skill {
        SU_CN_METEOR2 => SU_CN_METEOR,
        SU_LUNATICCARROTBEAT2 => SU_LUNATICCARROTBEAT,
        skill => skill,
    };
    let name = name_of(skill)?;
    Some(match (skill, partner) {
        (WE_MALE | WE_FEMALE, Some(partner)) => format!("{partner} !!  {protect_words}"),
        (WE_CALLPARTNER, Some(partner)) => format!("{partner} !!  {name} ♡"),
        _ => format!("{name} !!"),
    })
}

/// The wedding skills: I Will Protect You (both), Call Partner.
const WE_MALE: u16 = 334;
const WE_FEMALE: u16 = 335;
const WE_CALLPARTNER: u16 = 336;
/// Summoner follow-ups, called by the skills they follow.
const SU_CN_METEOR: u16 = 5028;
const SU_LUNATICCARROTBEAT: u16 = 5036;
const SU_CN_METEOR2: u16 = 5042;
const SU_LUNATICCARROTBEAT2: u16 = 5043;

/// Whether a unit calls a skill's name over its head. A level below 0 marks a silent follow-up hit. Monsters call
/// only skills without damage; everyone else calls every skill but the quiet ones, so a skill with a cast time is
/// called twice: as the cast starts and as it lands.
pub fn shouts(skill: u16, level: i32, job: u16, kind: u8, packet: Shout) -> bool {
    if level < 0 || SILENT.contains(&skill) {
        return false;
    }
    if monster_job(job) {
        // ponytail: elementals and the like (types 10, 13 and 14) call their ZC_NOTIFY_SKILL hits too.
        return packet == Shout::Used || (packet == Shout::Hit && matches!(kind, 10 | 13 | 14));
    }
    let quiet = QUIET.contains(&skill) || (packet == Shout::Cast && CALLED_ON_LANDING.contains(&skill)) || (packet == Shout::Used && skill == UNCALLED_USE);
    !quiet
}

/// Screen pixels per sprite pixel on the 768-high screen, at the default camera.
const SCREEN_PER_SPRITE: f32 = 1.34;
/// How far over the feet the balloon's bottom and the cast bar's top sit, in sprite pixels straight up the screen:
/// 81/480 of the screen's height.
pub const HEAD: f32 = 129.0 / SCREEN_PER_SPRITE;
pub const CAST_ABOVE: f32 = HEAD;
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

/// How far under the feet HP/SP bars start (12/480 of the screen's height) and the name's box under them (20/480),
/// in sprite pixels; the name's text starts this many screen pixels into its box.
pub const BAR_BELOW: f32 = 19.0 / SCREEN_PER_SPRITE;
pub const NAME_BELOW: f32 = 32.0 / SCREEN_PER_SPRITE;
pub const NAME_TEXT_TOP: f32 = 4.0;
/// Bars under units are 60 px wide: a 3 px row a value, with 1 px of border around and between the rows.
pub const BAR_WIDTH: f32 = 60.0;

/// A row's fill: trunc(58 · value / max) px; none when the maximum is 0.
pub fn bar_fill(value: i64, max: i64) -> Option<f32> {
    (max > 0).then(|| ((BAR_WIDTH as i64 - 2) * value.clamp(0, max) / max) as f32)
}

/// A party member's (or battleground teammate's) row: trunc(0.6 · pct − 2) px with pct a whole percent; none at 0
/// or less.
pub fn party_fill(value: i64, max: i64) -> Option<f32> {
    party_fill_width(value, max, BAR_WIDTH)
}

/// The party fill on a bar `width` wide (60 under units, 66 in a small party window): trunc(width·pct/100 − 2), pct a
/// whole percent; none at 0.
pub fn party_fill_width(value: i64, max: i64, width: f32) -> Option<f32> {
    if max <= 0 {
        return None;
    }
    let percent = 100 * value.clamp(0, max) / max;
    let width = (width * percent as f32 / 100.0 - 2.0).trunc();
    (width > 0.0).then_some(width)
}

/// How a bar's rows fill: `bar_fill` or `party_fill`.
pub type Fill = fn(i64, i64) -> Option<f32>;

/// A bar under a unit as rectangles from its top left: the border, the background, the lines between rows, then each
/// row's fill (by `fill`). A row is its value, its maximum, its color, and the color it takes under a quarter full.
pub fn gage<T: Copy>(rows: &[(i64, i64, T, T)], border: T, background: T, fill: Fill) -> Vec<(T, [f32; 4])> {
    let height = 1.0 + 4.0 * rows.len() as f32;
    let mut rects = vec![(border, [0.0, 0.0, BAR_WIDTH, height]), (background, [1.0, 1.0, BAR_WIDTH - 2.0, height - 2.0])];
    rects.extend((1..rows.len()).map(|row| (border, [0.0, 4.0 * row as f32, BAR_WIDTH, 1.0])));
    for (row, &(value, max, color, low)) in rows.iter().enumerate() {
        if let Some(width) = fill(value, max) {
            rects.push((if value * 4 < max { low } else { color }, [1.0, 1.0 + 4.0 * row as f32, width, 3.0]));
        }
    }
    rects
}
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
/// A hovered player's name block's lines: `[Title] Name (Party)`, then `Guild [Position]` when they have a guild.
pub fn name_lines(name: &str, title: Option<&str>, party: &str, guild: &str, position: &str) -> Vec<String> {
    let mut first = title.filter(|title| !title.is_empty()).map_or_else(String::new, |title| format!("[{title}] "));
    first.push_str(name);
    if !party.is_empty() {
        first.push_str(&format!(" ({party})"));
    }
    let mut lines = vec![first];
    if !guild.is_empty() {
        lines.push(format!("{guild} [{position}]"));
    }
    lines
}

/// A name block's lines are 14 px apart, the first 4 px below its top.
pub const NAME_LINE: f32 = 14.0;
/// Where a name block's parts start from its left: the emblem's frame (on siege maps), the emblem (24 px, centred on
/// the lines) and the text. Without an emblem the text starts at 0.
#[derive(Debug, PartialEq)]
pub struct BlockLayout {
    pub frame_x: Option<f32>,
    pub emblem_x: Option<f32>,
    pub text_x: f32,
}

/// The block's layout with an emblem or not, and with its frame of `frame` px wide or not.
pub fn block_layout(emblem: bool, frame: Option<f32>) -> BlockLayout {
    match (emblem, frame) {
        (false, _) => BlockLayout { frame_x: None, emblem_x: None, text_x: 0.0 },
        (true, None) => BlockLayout { frame_x: None, emblem_x: Some(4.0), text_x: 32.0 },
        (true, Some(width)) => BlockLayout { frame_x: Some(2.0), emblem_x: Some(4.0), text_x: width + 4.0 },
    }
}

/// The top of something `height` tall centred on a block's `lines` lines.
pub fn centred_on_lines(lines: usize, height: f32) -> f32 {
    NAME_TEXT_TOP + NAME_LINE * lines as f32 / 2.0 - height / 2.0
}

/// A name shown as `??????`: a unit under the unknown-name status (672) on a PvP map (map type 5, 6 or 19) or a
/// siege map (or map type 8 or 18).
pub fn name_hidden(map_type: Option<u16>, siege: bool, unknown_name: bool) -> bool {
    unknown_name && (siege || matches!(map_type, Some(5 | 6 | 8 | 18 | 19)))
}

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
    fn lays_out_a_hovered_players_name_block() {
        // Every part, then without each.
        assert_eq!(name_lines("Kafra", Some("Hero"), "Crabs", "CrabGuild", "Master"), ["[Hero] Kafra (Crabs)", "CrabGuild [Master]"]);
        assert_eq!(name_lines("Kafra", None, "Crabs", "CrabGuild", "Member"), ["Kafra (Crabs)", "CrabGuild [Member]"]);
        assert_eq!(name_lines("Kafra", Some("Hero"), "", "", ""), ["[Hero] Kafra"]);
        assert_eq!(name_lines("Kafra", Some(""), "", "", ""), ["Kafra"]);
        // The emblem at 4 and the text at 32; with the siege frame (28 wide) behind it at 2, the text at its width + 4.
        assert_eq!(block_layout(false, Some(28.0)), BlockLayout { frame_x: None, emblem_x: None, text_x: 0.0 });
        assert_eq!(block_layout(true, None), BlockLayout { frame_x: None, emblem_x: Some(4.0), text_x: 32.0 });
        assert_eq!(block_layout(true, Some(28.0)), BlockLayout { frame_x: Some(2.0), emblem_x: Some(4.0), text_x: 32.0 });
        // The 24 px emblem centred on two lines (4 to 32): from 6.
        assert_eq!(centred_on_lines(2, 24.0), 6.0);
        // ?????? under the unknown-name status on a siege map and a type-19 map, not in town or without the status.
        assert!(name_hidden(Some(0), true, true) && name_hidden(Some(19), false, true));
        assert!(!name_hidden(Some(0), false, true) && !name_hidden(Some(19), false, false));
    }

    /// A bar's fills: (color, top, width).
    fn fills(rows: &[(i64, i64, char, char)]) -> Vec<(char, f32, f32)> {
        gage(rows, '#', '.', bar_fill).into_iter().filter(|(color, _)| !"#.".contains(*color)).map(|(color, [_, top, width, _])| (color, top, width)).collect()
    }

    #[test]
    fn stacks_bars_a_row_a_value() {
        // A homunculus: HP, SP, hunger over 100, 60×13 with border lines at 4 and 8.
        let homunculus = |hp, sp, hunger| [(hp, 200, 'g', 'r'), (sp, 80, 'b', 'b'), (hunger, 100, 'h', 'r')];
        let rects = gage(&homunculus(200, 0, 100), '#', '.', bar_fill);
        assert_eq!(&rects[..4], [('#', [0.0, 0.0, 60.0, 13.0]), ('.', [1.0, 1.0, 58.0, 11.0]), ('#', [0.0, 4.0, 60.0, 1.0]), ('#', [0.0, 8.0, 60.0, 1.0])]);
        assert_eq!(fills(&homunculus(200, 0, 100)), [('g', 1.0, 58.0), ('b', 5.0, 0.0), ('h', 9.0, 58.0)]);
        // 50 % HP; 20 % HP and hunger 24 both take the low color; SP never does.
        assert_eq!(fills(&homunculus(100, 40, 25)), [('g', 1.0, 29.0), ('b', 5.0, 29.0), ('h', 9.0, 14.0)]);
        assert_eq!(fills(&homunculus(40, 4, 24)), [('r', 1.0, 11.0), ('b', 5.0, 2.0), ('r', 9.0, 13.0)]);
        // A pet: hunger alone, 60×5.
        assert_eq!(gage(&[(30, 100, 'h', 'r')], '#', '.', bar_fill)[..2], [('#', [0.0, 0.0, 60.0, 5.0]), ('.', [1.0, 1.0, 58.0, 3.0])]);
        assert_eq!(fills(&[(30, 100, 'h', 'r')]), [('h', 1.0, 17.0)]);
        // A mercenary whose maximum HP isn't known: the frame, no HP fill.
        assert_eq!(gage(&[(10, 0, 'g', 'r'), (5, 10, 'b', 'b')], '#', '.', bar_fill)[0], ('#', [0.0, 0.0, 60.0, 9.0]));
        assert_eq!(fills(&[(10, 0, 'g', 'r'), (5, 10, 'b', 'b')]), [('b', 5.0, 29.0)]);
        // A 4th job's own bar: HP, SP and the third row, 60×13; over the maximum fills it.
        let fourth = [(900, 1000, 'g', 'r'), (100, 100, 'b', 'b'), (150, 100, 'a', 'a')];
        assert_eq!(gage(&fourth, '#', '.', bar_fill)[0], ('#', [0.0, 0.0, 60.0, 13.0]));
        // Party members fill by whole percents: trunc(0.6 · pct − 2), nothing at 0 or less.
        let party = [0, 1, 3, 4, 50, 99, 100].map(|percent| party_fill(percent, 100));
        assert_eq!(party, [None, None, None, None, Some(28.0), Some(57.0), Some(58.0)]);
        assert_eq!((party_fill(1, 3), bar_fill(1, 3)), (Some(17.0), Some(19.0)));
        let small = [0, 1, 3, 4, 50, 99, 100].map(|percent| party_fill_width(percent, 100, 66.0));
        assert_eq!(small, [None, None, None, None, Some(31.0), Some(63.0), Some(64.0)]);
        assert_eq!(fills(&fourth), [('g', 1.0, 52.0), ('b', 5.0, 58.0), ('a', 9.0, 58.0)]);
    }

    #[test]
    fn sizes_the_cast_bar() {
        assert_eq!(cast_rect([512.0, 300.4]), [482.0, 300.0, 60.0, 6.0]);
        assert_eq!([0.0, 0.01, 0.5, 0.99, 1.0, 1.5].map(cast_fill), [0.0, 0.0, 28.0, 57.0, 58.0, 58.0]);
    }

    #[test]
    fn who_calls_which_skill() {
        let (player, poring) = ((0, 0), (1002, 5));
        let calls = |skill: u16, level: i32, (job, kind): (u16, u8), packet: Shout| shouts(skill, level, job, kind, packet);
        // Heal: called as it's cast and as it's used, by players; a monster only as it's used.
        assert!(calls(28, 10, player, Shout::Cast) && calls(28, 10, player, Shout::Used));
        assert!(!calls(28, 10, poring, Shout::Cast) && calls(28, 10, poring, Shout::Used));
        // A monster's bolt hits without its name; a player's calls it.
        assert!(!calls(19, 5, poring, Shout::Hit) && calls(19, 5, player, Shout::Hit));
        // Silent follow-ups, hiding, Frost Joke.
        assert!(!calls(19, -1, player, Shout::Hit));
        assert!(!calls(51, 1, player, Shout::Used) && calls(51, 1, poring, Shout::Used));
        assert!(!calls(318, 5, poring, Shout::Used) && !calls(318, 5, player, Shout::Used));
        // Bowling Bash only as it lands.
        assert!(!calls(62, 10, player, Shout::Cast) && calls(62, 10, player, Shout::Hit));
        assert!(!calls(9, 1, player, Shout::Used));
        // A pet goes by its job: a monster. An elemental too, but it calls its skill hits; never its casts or ground skills.
        let (pet, elemental, homunculus) = ((1002, 7), (2114, 10), (6001, 8));
        assert!(!calls(19, 5, pet, Shout::Hit) && calls(28, 5, pet, Shout::Used));
        assert!(calls(19, 5, elemental, Shout::Hit) && calls(28, 5, elemental, Shout::Used));
        assert!(!calls(19, 5, elemental, Shout::Cast) && !calls(19, 5, elemental, Shout::Ground));
        assert!(calls(19, 5, homunculus, Shout::Cast) && calls(19, 5, homunculus, Shout::Ground));
        assert_eq!([1000, 1001, 3998, 3999, 20020, 20021, 31999, 32000].map(monster_job), [false, true, true, false, false, true, true, false]);
    }

    #[test]
    fn calls_the_wedding_skills_and_the_follow_ups() {
        let name_of = |skill: u16| match skill {
            334 | 335 => Some("I Will Protect You".to_owned()),
            336 => Some("Call Partner".to_owned()),
            5028 => Some("Meteor".to_owned()),
            5036 => Some("Carrot Beat".to_owned()),
            19 => Some("Fire Bolt".to_owned()),
            _ => None,
        };
        let words = "I will protect you!";
        assert_eq!(shout_text(334, name_of, Some("Juliet"), words).as_deref(), Some("Juliet !!  I will protect you!"));
        assert_eq!(shout_text(334, name_of, None, words).as_deref(), Some("I Will Protect You !!"));
        assert_eq!(shout_text(335, name_of, Some("Romeo"), words).as_deref(), Some("Romeo !!  I will protect you!"));
        assert_eq!(shout_text(4999, name_of, None, words), None, "no name, nothing called");
        assert_eq!(shout_text(336, name_of, Some("Juliet"), words).as_deref(), Some("Juliet !!  Call Partner ♡"));
        assert_eq!(shout_text(336, name_of, None, words).as_deref(), Some("Call Partner !!"));
        assert_eq!(
            (shout_text(5042, name_of, None, words).as_deref(), shout_text(5043, name_of, None, words).as_deref()),
            (Some("Meteor !!"), Some("Carrot Beat !!"))
        );
        assert_eq!(shout_text(19, name_of, Some("Juliet"), words).as_deref(), Some("Fire Bolt !!"));
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
