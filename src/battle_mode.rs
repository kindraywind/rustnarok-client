//! Battle mode (`ChangeChatMode`, `/bm`): the chat line stays closed and letter keys are shortcuts.
//!
//! Off, the line is always open and letters type. On, it starts closed. Enter opens it when no other
//! window wants the key; Enter on an empty line closes it; Esc closes it. While it is closed, keys go
//! to the shortcut layer.

use std::path::{Path, PathBuf};

/// messages[785] battle mode on, messages[786] off.
pub const ON_MESSAGE: usize = 785;
pub const OFF_MESSAGE: usize = 786;

/// The saved option, and whether the line is open while it is on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mode {
    on: bool,
    /// Meaningful while `on`. Turning battle mode on starts with the line closed.
    line_open: bool,
}

impl Mode {
    #[cfg(test)]
    fn enabled(self) -> bool {
        self.on
    }

    /// The control socket's `chat_line_open`: always true while battle mode is off.
    pub fn chat_line_open(self) -> bool {
        !self.on || self.line_open
    }

    /// Esc's first rule (#178): the battle-mode line, and not the always-on line.
    pub fn battle_line_open(self) -> bool {
        self.on && self.line_open
    }

    /// Esc closed the line. The chat line clears the draft.
    pub fn close_line(&mut self) {
        self.line_open = false;
    }

    /// Turns battle mode on or off. On starts closed; off leaves the line open. The message to print.
    pub fn toggle(&mut self) -> usize {
        self.on = !self.on;
        self.line_open = false;
        if self.on { ON_MESSAGE } else { OFF_MESSAGE }
    }

    /// A click on the input bar opens the line. Enter is the key the commands use.
    pub fn open_line(&mut self) {
        if self.on {
            self.line_open = true;
        }
    }

    /// Routes a key that no other window took.
    /// `naming`: Enter is in the whisper-name box, so it leaves that box instead of closing the line.
    pub fn take_key(&mut self, key: KeyKind, line_empty: bool, naming: bool) -> Take {
        if !self.on {
            return Take::Type;
        }
        if !self.line_open {
            if key == KeyKind::Enter {
                self.line_open = true;
                return Take::Opened;
            }
            return Take::Shortcut;
        }
        if key == KeyKind::Enter && naming {
            return Take::Type;
        }
        if key == KeyKind::Enter && line_empty {
            self.line_open = false;
            return Take::Closed;
        }
        if key == KeyKind::Escape {
            self.line_open = false;
            return Take::Closed;
        }
        Take::Type
    }

    pub fn load() -> Self {
        Self::path().map_or_else(Self::default, |path| Self::load_from(&path))
    }

    pub fn save(&self) {
        if let Some(path) = Self::path() {
            self.save_to(&path);
        }
    }

    pub fn load_from(path: &Path) -> Self {
        let on = std::fs::read_to_string(path).is_ok_and(|text| text.trim() == "1");
        Self { on, line_open: false }
    }

    pub fn save_to(&self, path: &Path) {
        let _ = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(path, if self.on { "1" } else { "0" }));
    }

    fn path() -> Option<PathBuf> {
        // The ChangeChatMode option, beside the other saved settings.
        crate::settings_path("chatmode")
    }
}

/// A key as battle mode sees it. Letters and digits are lowercased.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyKind {
    Enter,
    Escape,
    Char(char),
    Other,
}

/// What `take_key` decided. `Type` is today's chat line. The others do not type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Take {
    Type,
    Opened,
    Closed,
    Shortcut,
}

/// What one key did, once the binding table has been asked. A shortcut and a typed character are never both.
#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Delivery {
    /// The chat line takes the key.
    Typed,
    /// The line opened. This key is not typed.
    Opened,
    /// The line closed. This key is not typed.
    Closed,
    /// Hotbar slot fired. The character is not typed.
    Fired(usize),
    /// The line is closed and nothing is bound. The character is not typed.
    Ignored,
    /// Another window wanted the key. The line is left as it was.
    Window,
}

/// Stand-in for #177's shortcut table: a letter or digit bound to a hotbar slot.
#[cfg(test)]
#[derive(Clone, Debug, Default)]
struct Bindings {
    slots: Vec<(char, usize)>,
}

#[cfg(test)]
impl Bindings {
    pub fn bind(&mut self, key: char, slot: usize) {
        let key = key.to_ascii_lowercase();
        self.slots.retain(|(bound, _)| *bound != key);
        self.slots.push((key, slot));
    }

    pub fn slot(&self, key: char) -> Option<usize> {
        let key = key.to_ascii_lowercase();
        self.slots.iter().find(|(bound, _)| *bound == key).map(|(_, slot)| *slot)
    }
}

/// Routes one key. `window_wants` is a dialog or other box that already took it.
/// ponytail: #177 owns the real shortcut table (hotkey_v2.lub and UserKeys). `bindings` is the stand-in the tests use.
#[cfg(test)]
fn press(mode: &mut Mode, key: KeyKind, line_empty: bool, naming: bool, window_wants: bool, bindings: &Bindings) -> Delivery {
    if window_wants {
        return Delivery::Window;
    }
    match mode.take_key(key, line_empty, naming) {
        Take::Type => Delivery::Typed,
        Take::Opened => Delivery::Opened,
        Take::Closed => Delivery::Closed,
        Take::Shortcut => match key {
            KeyKind::Char(character) => bindings.slot(character).map_or(Delivery::Ignored, Delivery::Fired),
            _ => Delivery::Ignored,
        },
    }
}

/// Applies `press` to the draft on the chat line. Only `Typed` letters are appended, so a fired shortcut cannot land there too.
#[cfg(test)]
fn apply_draft(mode: &mut Mode, draft: &mut String, key: KeyKind, naming: bool, window_wants: bool, bindings: &Bindings) -> Delivery {
    let delivery = press(mode, key, draft.trim().is_empty(), naming, window_wants, bindings);
    if let (Delivery::Typed, KeyKind::Char(character)) = (delivery, key) {
        draft.push(character);
    }
    delivery
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Row 2 slot 1: nine slots a row, row 1 is F1–F9.
    const ROW2_SLOT1: usize = 9;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("rustnarok-chatmode-{}-{name}", std::process::id()))
    }

    fn q_bindings() -> Bindings {
        let mut bindings = Bindings::default();
        bindings.bind('Q', ROW2_SLOT1);
        bindings
    }

    #[test]
    fn off_the_line_stays_open_and_letters_type() {
        let mut mode = Mode::default();
        let bindings = q_bindings();
        assert!(!mode.enabled());
        assert!(mode.chat_line_open());
        let mut draft = String::new();
        assert_eq!(apply_draft(&mut mode, &mut draft, KeyKind::Char('q'), false, false, &bindings), Delivery::Typed);
        assert_eq!(draft, "q");
        assert!(mode.chat_line_open());
        assert_eq!(press(&mut mode, KeyKind::Enter, true, false, false, &bindings), Delivery::Typed);
        assert!(mode.chat_line_open());
    }

    #[test]
    fn toggle_uses_the_spec_messages_and_starts_closed() {
        let mut mode = Mode::default();
        assert_eq!(mode.toggle(), ON_MESSAGE);
        assert!(mode.enabled());
        assert!(!mode.chat_line_open());
        assert_eq!(mode.toggle(), OFF_MESSAGE);
        assert!(!mode.enabled());
        assert!(mode.chat_line_open());
    }

    #[test]
    fn open_type_send_then_empty_enter_closes() {
        let mut mode = Mode::default();
        mode.toggle();
        let bindings = q_bindings();
        let mut draft = String::new();
        // Q is bound: it fires and does not type.
        assert_eq!(apply_draft(&mut mode, &mut draft, KeyKind::Char('q'), false, false, &bindings), Delivery::Fired(ROW2_SLOT1));
        assert!(draft.is_empty());
        assert!(!mode.chat_line_open());
        // Enter opens. Typing goes to the line. Enter with text is the send. Enter on the empty line closes.
        assert_eq!(apply_draft(&mut mode, &mut draft, KeyKind::Enter, false, false, &bindings), Delivery::Opened);
        assert!(mode.chat_line_open());
        assert_eq!(apply_draft(&mut mode, &mut draft, KeyKind::Char('h'), false, false, &bindings), Delivery::Typed);
        assert_eq!(apply_draft(&mut mode, &mut draft, KeyKind::Char('i'), false, false, &bindings), Delivery::Typed);
        assert_eq!(draft, "hi");
        assert_eq!(apply_draft(&mut mode, &mut draft, KeyKind::Enter, false, false, &bindings), Delivery::Typed);
        draft.clear();
        assert_eq!(apply_draft(&mut mode, &mut draft, KeyKind::Enter, false, false, &bindings), Delivery::Closed);
        assert!(!mode.chat_line_open());
        assert!(draft.is_empty());
    }

    #[test]
    fn a_bound_letter_does_not_also_type() {
        let mut mode = Mode::default();
        mode.toggle();
        let bindings = q_bindings();
        let mut draft = String::new();
        let delivery = apply_draft(&mut mode, &mut draft, KeyKind::Char('Q'), false, false, &bindings);
        assert_eq!(delivery, Delivery::Fired(ROW2_SLOT1));
        assert!(draft.is_empty());
        // The same key types once the line is open, and does not fire.
        apply_draft(&mut mode, &mut draft, KeyKind::Enter, false, false, &bindings);
        assert_eq!(apply_draft(&mut mode, &mut draft, KeyKind::Char('q'), false, false, &bindings), Delivery::Typed);
        assert_eq!(draft, "q");
    }

    #[test]
    fn an_unbound_letter_or_digit_does_nothing_while_closed() {
        let mut mode = Mode::default();
        mode.toggle();
        let mut bindings = Bindings::default();
        bindings.bind('2', 10);
        let mut draft = String::new();
        assert_eq!(apply_draft(&mut mode, &mut draft, KeyKind::Char('w'), false, false, &bindings), Delivery::Ignored);
        assert_eq!(apply_draft(&mut mode, &mut draft, KeyKind::Char('2'), false, false, &bindings), Delivery::Fired(10));
        assert!(draft.is_empty());
        assert!(!mode.chat_line_open());
    }

    #[test]
    fn enter_does_not_open_when_another_window_wants_it() {
        let mut mode = Mode::default();
        mode.toggle();
        let bindings = Bindings::default();
        assert_eq!(press(&mut mode, KeyKind::Enter, true, false, true, &bindings), Delivery::Window);
        assert!(!mode.chat_line_open());
    }

    #[test]
    fn the_always_on_line_is_not_the_battle_line() {
        let mut mode = Mode::default();
        assert!(mode.chat_line_open());
        assert!(!mode.battle_line_open());
        mode.toggle();
        assert!(!mode.battle_line_open());
        mode.open_line();
        assert!(mode.battle_line_open());
        mode.close_line();
        assert!(!mode.battle_line_open());
        assert!(!mode.chat_line_open());
    }

    #[test]
    fn escape_closes_an_open_line_and_leaves_the_draft() {
        let mut mode = Mode::default();
        mode.toggle();
        let bindings = Bindings::default();
        let draft = String::from("hi");
        mode.open_line();
        assert!(mode.chat_line_open());
        assert_eq!(press(&mut mode, KeyKind::Escape, false, false, false, &bindings), Delivery::Closed);
        assert!(!mode.chat_line_open());
        assert_eq!(draft, "hi");
        // A window that wants Esc keeps the line open.
        mode.open_line();
        assert_eq!(press(&mut mode, KeyKind::Escape, false, false, true, &bindings), Delivery::Window);
        assert!(mode.chat_line_open());
        // Esc while closed is a shortcut key (the quit menu), not another close.
        assert_eq!(mode.take_key(KeyKind::Escape, false, false), Take::Closed);
        assert_eq!(press(&mut mode, KeyKind::Escape, draft.trim().is_empty(), false, false, &bindings), Delivery::Ignored);
        assert!(!mode.chat_line_open());
    }

    #[test]
    fn enter_in_the_name_box_does_not_close_an_empty_line() {
        let mut mode = Mode::default();
        mode.toggle();
        mode.open_line();
        let bindings = Bindings::default();
        assert_eq!(press(&mut mode, KeyKind::Enter, true, true, false, &bindings), Delivery::Typed);
        assert!(mode.chat_line_open());
    }

    #[test]
    fn a_blank_line_counts_as_empty() {
        let mut mode = Mode::default();
        mode.toggle();
        mode.open_line();
        let bindings = Bindings::default();
        let mut draft = String::from("  ");
        assert_eq!(apply_draft(&mut mode, &mut draft, KeyKind::Enter, false, false, &bindings), Delivery::Closed);
        assert!(!mode.chat_line_open());
    }

    #[test]
    fn the_option_round_trips_and_starts_closed() {
        let path = temp_path("round");
        let _ = std::fs::remove_file(&path);
        assert!(!Mode::load_from(&path).enabled());
        assert!(Mode::load_from(&path).chat_line_open());
        let mut mode = Mode::default();
        mode.toggle();
        mode.save_to(&path);
        let loaded = Mode::load_from(&path);
        assert!(loaded.enabled());
        assert!(!loaded.chat_line_open());
        mode.toggle();
        mode.save_to(&path);
        assert!(!Mode::load_from(&path).enabled());
        std::fs::write(&path, "1\n").unwrap();
        assert!(Mode::load_from(&path).enabled());
        std::fs::write(&path, "nope").unwrap();
        assert!(!Mode::load_from(&path).enabled());
        let _ = std::fs::remove_file(path);
    }
}
