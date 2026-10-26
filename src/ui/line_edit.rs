//! Editing a one-line text box: a caret, a selection, typing over it, deleting either way. The caret is kept as a
//! distance from the end, so a box whose text is set from outside keeps it at the end.

use std::ops::Range;

/// Where the caret is and where a selection started, each in bytes back from the end of the text.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Caret {
    back: usize,
    anchor: Option<usize>,
}

/// A caret move.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Move {
    Left,
    Right,
    Home,
    End,
}

impl Caret {
    /// The caret's byte in `text`, on a character's start.
    pub fn at(&self, text: &str) -> usize {
        boundary(text, text.len().saturating_sub(self.back))
    }

    fn set(&mut self, text: &str, at: usize) {
        self.back = text.len() - at;
    }

    /// The selected bytes, if any.
    pub fn selection(&self, text: &str) -> Option<Range<usize>> {
        let anchor = boundary(text, text.len().saturating_sub(self.anchor?));
        let at = self.at(text);
        (anchor != at).then(|| anchor.min(at)..anchor.max(at))
    }

    /// Keeps the caret and the anchor inside `text` (it may have been emptied or set from outside).
    fn clamp(&mut self, text: &str) {
        self.back = self.back.min(text.len());
        self.anchor = self.anchor.map(|anchor| anchor.min(text.len()));
    }

    /// Moves the caret; with `select` the selection grows from where it was, else it goes.
    pub fn go(&mut self, text: &str, to: Move, select: bool) {
        self.clamp(text);
        let at = self.at(text);
        if select {
            self.anchor.get_or_insert(self.back);
        } else if let Some(range) = self.selection(text).filter(|_| matches!(to, Move::Left | Move::Right)) {
            // Left or Right on a selection lands on its edge.
            self.anchor = None;
            return self.set(text, if to == Move::Left { range.start } else { range.end });
        } else {
            self.anchor = None;
        }
        let to = match to {
            Move::Left => text[..at].char_indices().next_back().map_or(0, |(start, _)| start),
            Move::Right => text[at..].chars().next().map_or(at, |character| at + character.len_utf8()),
            Move::Home => 0,
            Move::End => text.len(),
        };
        self.set(text, to);
    }

    pub fn select_all(&mut self, text: &str) {
        self.anchor = Some(text.len());
        self.back = 0;
    }

    /// Takes the selection out of `text`; returns it.
    pub fn take_selection(&mut self, text: &mut String) -> String {
        self.clamp(text);
        let Some(range) = self.selection(text) else { return String::new() };
        let after = text.len() - range.end;
        let taken = text.drain(range).collect();
        self.back = after;
        self.anchor = None;
        taken
    }

    /// Types `typed` at the caret (over the selection), as much of it as `fits` lets the whole text be.
    pub fn insert(&mut self, text: &mut String, typed: &str, fits: &dyn Fn(&str) -> bool) {
        self.clamp(text);
        let mut edited = text.clone();
        let mut caret = *self;
        caret.take_selection(&mut edited);
        let at = caret.at(&edited);
        let mut kept = 0;
        for (start, character) in typed.char_indices() {
            let end = start + character.len_utf8();
            let candidate = format!("{}{}{}", &edited[..at], &typed[..end], &edited[at..]);
            if !fits(&candidate) {
                break;
            }
            kept = end;
        }
        edited.insert_str(at, &typed[..kept]);
        caret.anchor = None;
        *text = edited;
        *self = caret;
    }

    /// Backspace: the selection, else the character before the caret.
    pub fn backspace(&mut self, text: &mut String) {
        self.clamp(text);
        if self.selection(text).is_none() {
            self.anchor = None;
            let at = self.at(text);
            let Some((start, _)) = text[..at].char_indices().next_back() else { return };
            text.replace_range(start..at, "");
            return;
        }
        self.take_selection(text);
    }

    /// Delete: the selection, else the character after the caret.
    pub fn delete(&mut self, text: &mut String) {
        self.clamp(text);
        if self.selection(text).is_none() {
            self.anchor = None;
            let at = self.at(text);
            let Some(character) = text[at..].chars().next() else { return };
            text.replace_range(at..at + character.len_utf8(), "");
            self.back -= character.len_utf8();
            return;
        }
        self.take_selection(text);
    }

    /// Back to the end, nothing selected (the box was emptied or filled from outside).
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

fn boundary(text: &str, mut at: usize) -> usize {
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// The last lines sent, oldest first, and Up / Down walking them with the unsent text kept as a draft.
#[derive(Debug, Default)]
pub struct History {
    lines: Vec<String>,
    at: Option<usize>,
    draft: String,
}

/// How many lines the history keeps.
pub const HISTORY: usize = 30;

impl History {
    /// A line sent: kept at the end (not an empty one), the oldest past 30 dropped; the walk starts over.
    pub fn sent(&mut self, line: &str) {
        self.at = None;
        if line.is_empty() {
            return;
        }
        self.lines.push(line.to_owned());
        let over = self.lines.len().saturating_sub(HISTORY);
        self.lines.drain(..over);
    }

    /// Up: the line before the one shown (the newest first, keeping `typed` as the draft); it stops at the oldest.
    pub fn up(&mut self, typed: &str) -> Option<&str> {
        let at = match self.at {
            None if self.lines.is_empty() => return None,
            None => {
                self.draft = typed.to_owned();
                self.lines.len() - 1
            }
            Some(at) => at.saturating_sub(1),
        };
        self.at = Some(at);
        Some(&self.lines[at])
    }

    /// Down: the line after the one shown, then the draft back past the newest.
    pub fn down(&mut self) -> Option<&str> {
        let at = self.at?;
        if at + 1 < self.lines.len() {
            self.at = Some(at + 1);
            Some(&self.lines[at + 1])
        } else {
            self.at = None;
            Some(&self.draft)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn any(_: &str) -> bool {
        true
    }

    #[test]
    fn walks_the_history_with_a_draft() {
        let mut history = History::default();
        assert_eq!(history.up("x"), None);
        for line in ["a", "b", "", "c"] {
            history.sent(line);
        }
        assert_eq!(history.up("draft").map(str::to_owned), Some("c".into()));
        assert_eq!(history.up("c").map(str::to_owned), Some("b".into()));
        assert_eq!(history.up("b").map(str::to_owned), Some("a".into()));
        assert_eq!(history.up("a").map(str::to_owned), Some("a".into()), "stops at the oldest");
        assert_eq!(history.down().map(str::to_owned), Some("b".into()));
        assert_eq!(history.down().map(str::to_owned), Some("c".into()));
        assert_eq!(history.down().map(str::to_owned), Some("draft".into()));
        assert_eq!(history.down(), None);
        for line in 0..40 {
            history.sent(&line.to_string());
        }
        assert_eq!(history.lines.len(), HISTORY);
        assert_eq!(history.lines[0], "10");
    }

    #[test]
    fn moves_the_caret_and_edits_at_it() {
        let mut text = "hello".to_owned();
        let mut caret = Caret::default();
        assert_eq!(caret.at(&text), 5);
        caret.go(&text, Move::Left, false);
        caret.go(&text, Move::Left, false);
        caret.insert(&mut text, "XY", &any);
        assert_eq!((text.as_str(), caret.at(&text)), ("helXYlo", 5));
        caret.backspace(&mut text);
        caret.delete(&mut text);
        assert_eq!((text.as_str(), caret.at(&text)), ("helXo", 4));
        caret.go(&text, Move::Home, false);
        caret.delete(&mut text);
        assert_eq!((text.as_str(), caret.at(&text)), ("elXo", 0));
        caret.go(&text, Move::End, false);
        assert_eq!(caret.at(&text), 4);
        // Korean moves a syllable at a time.
        let mut korean = "가나".to_owned();
        let mut caret = Caret::default();
        caret.go(&korean, Move::Left, false);
        assert_eq!(caret.at(&korean), 3);
        caret.backspace(&mut korean);
        assert_eq!(korean, "나");
        // Set from outside: the caret stays at the end; emptied, typing starts over at the end.
        let caret = Caret::default();
        assert_eq!(caret.at("another"), 7);
        let mut caret = Caret::default();
        let mut text = "abc".to_owned();
        caret.go(&text, Move::Home, false);
        text.clear();
        for typed in ["x", "y", "z"] {
            caret.insert(&mut text, typed, &any);
        }
        assert_eq!(text, "xyz");
    }

    #[test]
    fn selects_cuts_and_types_over() {
        let mut text = "abc def".to_owned();
        let mut caret = Caret::default();
        caret.go(&text, Move::Left, false);
        caret.go(&text, Move::Home, true);
        assert_eq!(caret.selection(&text), Some(0..6));
        assert_eq!(caret.take_selection(&mut text), "abc de");
        assert_eq!((text.as_str(), caret.at(&text)), ("f", 0));
        caret.select_all(&text);
        caret.insert(&mut text, "xyz", &any);
        assert_eq!((text.as_str(), caret.at(&text)), ("xyz", 3));
        // Left on a selection lands on its start; typing goes only as far as the box takes.
        caret.go(&text, Move::Home, true);
        caret.go(&text, Move::Left, false);
        assert_eq!((caret.at(&text), caret.selection(&text)), (0, None));
        caret.insert(&mut text, "12345", &|candidate: &str| candidate.len() <= 5);
        assert_eq!(text, "12xyz");
        caret.select_all(&text);
        caret.backspace(&mut text);
        assert_eq!(text, "");
    }
}
