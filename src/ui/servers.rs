//! The char-server list after login: a 280×200 framed window, crowd words, and which row opens next time.

pub const WIDTH: f32 = 280.0;
pub const HEIGHT: f32 = 200.0;
/// The list inside the window, and how much of it shows.
pub const LIST: [f32; 2] = [12.0, 22.0];
pub const LIST_SIZE: [f32; 2] = [256.0, 145.0];
pub const ROW: f32 = 16.0;
pub const VISIBLE: usize = 9;
/// The selected row's white band (one pixel shorter than the row).
pub const BAND: [f32; 2] = [256.0, 15.0];
pub const TEXT_X: f32 = 3.0;
pub const OK: [f32; 2] = [189.0, 176.0];
pub const CANCEL: [f32; 2] = [234.0, 176.0];
/// #F0F0F0 behind the rows.
pub const LIST_COLOR: [u8; 4] = [0xF0, 0xF0, 0xF0, 0xFF];
// ponytail: the bottom bar is btnbar_mid2, 27px like the pick lists, along the window's bottom.
pub const FOOTER: f32 = 27.0;
pub const TITLE: usize = 2082;
const NEW_SERVER: usize = 482;
const AGE: usize = 3258;
const PAID: usize = 3259;
const FREE: usize = 3260;
const SMOOTH: usize = 3261;
const NORMAL: usize = 3262;
const BUSY: usize = 3263;
const CROWDED: usize = 3264;
const ROW_BYTES: usize = 64;

const SMOOTH_COLOR: [u8; 3] = [0x00, 0xB5, 0x09];
const NORMAL_COLOR: [u8; 3] = [0xFF, 0x78, 0x00];
const BUSY_COLOR: [u8; 3] = [0xFF, 0x00, 0x00];
const CROWDED_COLOR: [u8; 3] = [0xBA, 0x00, 0xFF];

/// The message lines a row is built from.
pub struct Words<'a> {
    pub new_server: &'a str,
    pub age: &'a str,
    pub paid: &'a str,
    pub free: &'a str,
    pub smooth: &'a str,
    pub normal: &'a str,
    pub busy: &'a str,
    pub crowded: &'a str,
}

impl<'a> Words<'a> {
    pub fn from_messages(messages: &'a [String]) -> Self {
        let line = |id: usize| messages.get(id).map(String::as_str).unwrap_or("");
        Self {
            new_server: line(NEW_SERVER),
            age: line(AGE),
            paid: line(PAID),
            free: line(FREE),
            smooth: line(SMOOTH),
            normal: line(NORMAL),
            busy: line(BUSY),
            crowded: line(CROWDED),
        }
    }
}

/// One row: the crowd word in `color`, then `rest` in black (the two spaces live on `rest`).
pub struct Row {
    pub crowd: String,
    pub color: [u8; 3],
    pub rest: String,
}

/// Where the window sits on a screen of `size`: ((W − 640)/2 + 185, floor(300·H/480) − 80).
pub fn place(size: [f32; 2]) -> [f32; 2] {
    [((size[0] - 640.0) / 2.0 + 185.0).floor(), (300.0 * size[1] / 480.0).floor() - 80.0]
}

/// The row for one char server. `kind` is the packet's type; `new` is 1 for a new server.
pub fn row(name: &str, users: u16, kind: u16, new: u16, words: &Words) -> Row {
    let suffix = match kind {
        2 => words.age,
        3 => words.paid,
        4 => words.free,
        _ => "",
    };
    // Four or more users and a typed server: the suffix alone, with no name.
    if users >= 4 && !suffix.is_empty() {
        return Row { crowd: String::new(), color: [0, 0, 0], rest: cut(suffix) };
    }
    let mut body = String::new();
    if new == 1 {
        body.push_str(words.new_server);
    }
    body.push_str(name);
    body.push_str(suffix);
    if users >= 4 {
        return Row { crowd: String::new(), color: [0, 0, 0], rest: cut(&body) };
    }
    let (word, color) = match users {
        0 => (words.smooth, SMOOTH_COLOR),
        1 => (words.normal, NORMAL_COLOR),
        2 => (words.busy, BUSY_COLOR),
        _ => (words.crowded, CROWDED_COLOR),
    };
    let full = cut(&format!("{word}  {body}"));
    if full.len() <= word.len() {
        Row { crowd: full, color, rest: String::new() }
    } else {
        Row { crowd: word.to_owned(), color, rest: full[word.len()..].to_owned() }
    }
}

/// Cuts `text` so it is at most 64 bytes, on a character boundary.
fn cut(text: &str) -> String {
    if text.len() <= ROW_BYTES {
        return text.to_owned();
    }
    let mut end = ROW_BYTES;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}

/// The saved row, kept inside a list of `len` (0 when nothing was saved).
pub fn clamped_row(saved: Option<usize>, len: usize) -> usize {
    match len {
        0 => 0,
        len => saved.unwrap_or(0).min(len - 1),
    }
}

/// Scrolls so `selected` stays among the nine rows on screen.
pub fn keep_visible(selected: usize, scroll: usize) -> usize {
    if selected < scroll {
        selected
    } else if selected >= scroll + VISIBLE {
        selected + 1 - VISIBLE
    } else {
        scroll
    }
}

/// The rows on screen, from `scroll`.
pub fn shown(scroll: usize, len: usize) -> std::ops::Range<usize> {
    scroll..len.min(scroll + VISIBLE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words() -> Words<'static> {
        Words {
            new_server: "[New Server] ",
            age: " - Age 18+",
            paid: " - Paid server",
            free: " - Free server",
            smooth: "Smooth",
            normal: "Normal",
            busy: "Busy",
            crowded: "Crowded",
        }
    }

    fn shown_text(users: u16, new: u16, kind: u16) -> (String, [u8; 3], String) {
        let row = row("Prontera", users, kind, new, &words());
        (row.crowd, row.color, row.rest)
    }

    #[test]
    fn row_text_for_crowd_new_and_type() {
        assert_eq!(shown_text(0, 0, 0), ("Smooth".into(), SMOOTH_COLOR, "  Prontera".into()));
        assert_eq!(shown_text(1, 0, 0), ("Normal".into(), NORMAL_COLOR, "  Prontera".into()));
        assert_eq!(shown_text(2, 0, 0), ("Busy".into(), BUSY_COLOR, "  Prontera".into()));
        assert_eq!(shown_text(3, 0, 0), ("Crowded".into(), CROWDED_COLOR, "  Prontera".into()));
        assert_eq!(shown_text(4, 0, 0), (String::new(), [0, 0, 0], "Prontera".into()));
        assert_eq!(shown_text(0, 1, 0), ("Smooth".into(), SMOOTH_COLOR, "  [New Server] Prontera".into()));
        assert_eq!(shown_text(4, 1, 0), (String::new(), [0, 0, 0], "[New Server] Prontera".into()));
        assert_eq!(shown_text(0, 0, 2), ("Smooth".into(), SMOOTH_COLOR, "  Prontera - Age 18+".into()));
        assert_eq!(shown_text(0, 0, 3), ("Smooth".into(), SMOOTH_COLOR, "  Prontera - Paid server".into()));
        assert_eq!(shown_text(0, 0, 4), ("Smooth".into(), SMOOTH_COLOR, "  Prontera - Free server".into()));
        // users 4 with a type: the suffix alone, even when the server is new.
        assert_eq!(shown_text(4, 1, 3), (String::new(), [0, 0, 0], " - Paid server".into()));
    }

    #[test]
    fn cuts_a_row_at_64_bytes() {
        let name = "a".repeat(80);
        let ascii = row(&name, 4, 0, 0, &words());
        assert_eq!(ascii.rest.len(), 64);
        assert!(ascii.crowd.is_empty());
        // A character that straddles byte 64 is left off rather than split.
        let name = "가".repeat(22);
        let korean = row(&name, 4, 0, 0, &words());
        assert_eq!(korean.rest.len(), 63);
        assert!(korean.rest.is_char_boundary(korean.rest.len()));
    }

    #[test]
    fn window_place_at_800_and_1024() {
        assert_eq!(place([640.0, 480.0]), [185.0, 220.0]);
        assert_eq!(place([800.0, 600.0]), [265.0, 295.0]);
        assert_eq!(place([1024.0, 768.0]), [377.0, 400.0]);
    }

    #[test]
    fn clamps_a_saved_row_to_a_shorter_list() {
        assert_eq!(clamped_row(Some(5), 3), 2);
        assert_eq!(clamped_row(Some(1), 4), 1);
        assert_eq!(clamped_row(None, 4), 0);
        assert_eq!(clamped_row(Some(2), 0), 0);
    }

    #[test]
    fn scrolls_to_keep_row_12_shown() {
        let scroll = keep_visible(12, 0);
        assert_eq!(scroll, 4);
        assert!(shown(scroll, 20).contains(&12));
        assert_eq!(keep_visible(3, 4), 3);
    }
}
