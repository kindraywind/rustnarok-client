//! Emotions over heads (data\sprite\이팩트\emotion.spr): the client's ET_* numbers, which action of the sprite each
//! plays, and the chat commands that show them (`/!`, `/ho`, ... and their Korean shortcuts).

/// By ET_* number: the sprite's action and the commands for it.
const EMOTIONS: &[(usize, &[&str])] = &[
    (0, &["!"]),
    (1, &["?"]),
    (2, &["ho", "delight", "rlQma", "ㄱ"]),
    (3, &["lv", "heart", "gkxm"]),
    (5, &["swt", "sweat", "Eka"]),
    (6, &["ic", "aha", "dkgk"]),
    (7, &["an", "fret", "Wkwmd"]),
    (8, &["ag", "ghk", "anger"]),
    (9, &["$", "money", "ehs"]),
    (10, &["..."]),
    (12, &["scissors", "rkdnl", "gawi"]),
    (11, &["rock", "wnajr", "bawi", "qkdnl", "바위"]),
    (13, &["paper", "qh", "bo"]),
    (14, &[]),
    (4, &["lv2"]),
    (15, &["thx", "ㄳ"]),
    (16, &["wah"]),
    (17, &["sry", "sorry"]),
    (18, &["heh", "하하", "ㅇ", "smile"]),
    (19, &["swt2"]),
    (20, &["hmm"]),
    (21, &["no1", "원츄", "ㅊ"]),
    (22, &["??"]),
    (23, &["omg", "ㅎ"]),
    (24, &["oh", "o"]),
    (25, &["X", "x"]),
    (26, &["hlp", "ㅊㅊ"]),
    (27, &["go"]),
    (28, &["sob"]),
    (29, &["gg", "킥", "ㅋ", "케케"]),
    (30, &["kis"]),
    (31, &["kis2"]),
    (32, &["pif"]),
    (33, &["ok", "끄덕"]),
    (1000, &[]),
    (34, &[]),
    (35, &["bzz", "e1", "ㅉ"]),
    (36, &["rice", "e2"]),
    (37, &["awsm", "e3"]),
    (38, &["meh", "e4"]),
    (39, &["shy", "e5", "부"]),
    (40, &["pat", "e6"]),
    (41, &["mp", "엠", "e7"]),
    (42, &["slur", "e8"]),
    (43, &["com", "e9"]),
    (44, &["yawn", "ㅈ", "e10"]),
    (45, &["grat", "ㅊㅋ", "e11", "grat"]),
    (46, &["hp", "피", "e12"]),
    (47, &[]),
    (48, &[]),
    (49, &[]),
    (50, &[]),
    (51, &["fsh", "e13"]),
    (52, &["spin", "e14"]),
    (53, &["sigh", "한숨", "e15"]),
    (54, &["dum", "e16"]),
    (55, &["crwd", "시끌시끌", "e17"]),
    (56, &["desp", "otl", "e18"]),
    (57, &["dice", "e19"]),
    (58, &[]),
    (59, &[]),
    (60, &[]),
    (61, &[]),
    (62, &[]),
    (63, &[]),
    (64, &["love", "e20"]),
    (65, &[]),
    (66, &[]),
    (67, &["mobile", "e21"]),
    (68, &["mail", "e22"]),
    (69, &["antenna0", "e23"]),
    (70, &["antenna1", "e24"]),
    (71, &["antenna2", "e25"]),
    (72, &["antenna3", "e26"]),
    (73, &["hum", "흠", "e27"]),
    (74, &["abs", "멍", "e28"]),
    (75, &["oops", "웁", "e29"]),
    (76, &["spit", "웩", "e30"]),
    (77, &["ene", "e31", "탈력"]),
    (78, &["panic", "e32", "øÈ²"]),
    (79, &["whisp", "e33", "위스퍼"]),
    (80, &[]),
    (81, &[]),
    (82, &[]),
    (83, &[]),
    (84, &[]),
    (85, &[]),
    (86, &[]),
];

/// The emotion list's entries in order, by ET_* number, each with its label (a message number): the order of
/// `EMOTION_ORDERLIST` and the `InsertEmotionListTable` calls in data.grf's
/// `data\luafiles514\lua files\emotion\emotionlist.lub` (compiled Lua needing the official client, so copied here).
pub fn list() -> Vec<(u8, usize)> {
    let runs: [(&[u8], usize); 7] = [
        (&[0, 1, 2, 3, 14, 4, 5, 6, 7, 8, 9], 544),
        (&[11, 10, 12], 1501),
        (&[15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33], 555),
        (&[36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47], 772),
        (&[52, 53, 54, 55, 56, 57], 945),
        (&[65, 68, 69], 1504),
        (&[71, 72, 73, 74, 75, 76, 77, 78, 79, 80], 1508),
    ];
    runs.iter().flat_map(|(kinds, first)| kinds.iter().enumerate().map(move |(n, &kind)| (kind, first + n))).collect()
}

/// How long an emotion stays when its sprite doesn't say.
pub const SHOWN_MS: f32 = 2500.0;

/// The sprite action of an emotion, by its ET_* number.
pub fn sprite_action(kind: u8) -> Option<usize> {
    EMOTIONS.get(usize::from(kind)).map(|(action, _)| *action).filter(|&action| action < 1000)
}

/// The emotion a chat command asks for (`/ho`), if it is one.
pub fn by_command(text: &str) -> Option<u8> {
    let command = text.strip_prefix('/')?;
    EMOTIONS.iter().position(|(_, commands)| commands.contains(&command)).map(|kind| kind as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_the_emotions_in_the_datas_order() {
        let list = list();
        assert_eq!(list.len(), 64);
        assert_eq!(&list[..5], &[(0, 544), (1, 545), (2, 546), (3, 547), (14, 548)]);
        assert_eq!(list[11..14], [(11, 1501), (10, 1502), (12, 1503)]);
        assert_eq!(list[63], (80, 1517));
        assert!(list.iter().all(|&(kind, label)| sprite_action(kind).is_some() && label > 0));
    }

    #[test]
    fn finds_emotions_by_command() {
        assert_eq!(by_command("/!"), Some(0));
        assert_eq!(by_command("/ho"), Some(2));
        assert_eq!(by_command("/lv2"), Some(14));
        assert_eq!(by_command("/ㄳ"), Some(15));
        assert_eq!(by_command("hello"), None);
        assert_eq!(sprite_action(4), Some(5));
        assert_eq!(sprite_action(34), None);
    }
}
