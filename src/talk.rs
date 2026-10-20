//! The line a Bard (Frost Joke) or a Dancer (Scream) says in the chat when you use the skill: one picked from a text file
//! in the game's data, read at run time.

use rustnarok_client::clientinfo::codepage;

pub const FROST_JOKE: u16 = 318;
pub const SCREAM: u16 = 326;

/// The file a skill's lines are in (under data\, in data\english\ for langtype 10) and how many lines it may pick from.
fn source(skill: u16, langtype: u8) -> Option<(String, u32)> {
    let (file, lines) = match skill {
        FROST_JOKE => ("ba_frostjoke.txt", 117),
        SCREAM => ("dc_scream.txt", 114),
        _ => return None,
    };
    Some((if langtype == 10 { format!("data\\english\\{file}") } else { format!("data\\{file}") }, lines))
}

/// The byte Korea's files store for `byte`, or the one stored for it: letters mirrored (a to z), 0xB0 to 0xBF end for end.
pub fn mirror(byte: u8) -> u8 {
    match byte {
        b'a'..=b'z' => b'z' - (byte - b'a'),
        b'A'..=b'Z' => b'Z' - (byte - b'A'),
        0xB0..=0xBF => 0xBF - (byte - 0xB0),
        _ => byte,
    }
}

/// Segment `n` of the file's bytes, split at each CR (segment 0 is never chosen), without LF and TAB, at most 255 bytes, as
/// text in the langtype's code page; Korea's file is mirrored first. None when there is no such text.
pub fn line(data: &[u8], n: usize, langtype: u8) -> Option<String> {
    let mut bytes: Vec<u8> = data.split(|&byte| byte == 0x0D).nth(n)?.iter().copied().filter(|byte| !matches!(byte, 0x0A | 0x09)).take(255).collect();
    if langtype == 0 {
        bytes.iter_mut().for_each(|byte| *byte = mirror(*byte));
    }
    let (text, _) = codepage(langtype).decode_without_bom_handling(&bytes);
    (!text.is_empty()).then(|| text.into_owned())
}

/// What the own character says after using `skill` (`ok`: the use succeeded, `own`: it was them): `read` gives the file's bytes,
/// `rand(n)` one number below n. None for anyone else, a failure, a skill without lines, or a file that can't be read.
pub fn says(skill: u16, own: bool, ok: bool, langtype: u8, read: impl FnOnce(&str) -> Option<Vec<u8>>, mut rand: impl FnMut(u32) -> u32) -> Option<String> {
    if !(own && ok) {
        return None;
    }
    let (path, lines) = source(skill, langtype)?;
    line(&read(&path)?, 1 + rand(lines) as usize, langtype)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_a_segment_after_a_carriage_return_and_cleans_it() {
        let data = b"not this\r\nfirst\tline\r\nsecond\r\nthird";
        // A draw of 0 is segment 1: LF and TAB dropped.
        assert_eq!(line(data, 1, 10).as_deref(), Some("firstline"));
        assert_eq!(line(data, 3, 10).as_deref(), Some("third"));
        assert_eq!(line(data, 4, 10), None);
        // At most 255 bytes.
        let long = [b"x\r".as_slice(), &[b'a'; 400]].concat();
        assert_eq!(line(&long, 1, 10).unwrap().len(), 255);
    }

    #[test]
    fn korea_stores_the_letters_mirrored() {
        assert_eq!(mirror(0xB0), 0xBF);
        assert_eq!(mirror(0xBF), 0xB0);
        assert_eq!(mirror(b' '), b' ');
        let mirrored: Vec<u8> = b"Zyx az".iter().map(|&byte| mirror(byte)).collect();
        assert_eq!(mirrored, b"Abc za");
        // Decoded with langtype 0; the same bytes unmirrored with any other.
        assert_eq!(line(b"-\rZyx az", 1, 0).as_deref(), Some("Abc za"));
        assert_eq!(line(b"-\rZyx az", 1, 10).as_deref(), Some("Zyx az"));
    }

    #[test]
    fn says_a_line_only_for_the_own_character_after_a_success() {
        let file = || Some(b"-\rhello\rbye".to_vec());
        assert_eq!(says(FROST_JOKE, true, true, 10, |path| (path == "data\\english\\ba_frostjoke.txt").then(file).flatten(), |_| 0).as_deref(), Some("hello"));
        assert_eq!(
            says(SCREAM, true, true, 0, |path| (path == "data\\dc_scream.txt").then(file).flatten(), |_| 1),
            Some(line(b"-\rhello\rbye", 2, 0).unwrap())
        );
        assert_eq!(says(FROST_JOKE, false, true, 10, |_| file(), |_| 0), None, "not the own character");
        assert_eq!(says(FROST_JOKE, true, false, 10, |_| file(), |_| 0), None, "the use failed");
        assert_eq!(says(FROST_JOKE, true, true, 10, |_| None, |_| 0), None, "the file can't be read");
        assert_eq!(says(100, true, true, 10, |_| file(), |_| 0), None, "no lines for this skill");
        // Frost Joke draws from 117 lines, Scream from 114.
        let mut asked = 0;
        let _ = says(
            FROST_JOKE,
            true,
            true,
            10,
            |_| file(),
            |n| {
                asked = n;
                0
            },
        );
        assert_eq!(asked, 117);
        let _ = says(
            SCREAM,
            true,
            true,
            10,
            |_| file(),
            |n| {
                asked = n;
                0
            },
        );
        assert_eq!(asked, 114);
    }
}
