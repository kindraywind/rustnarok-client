//! Second-password (PIN) number pad at character select (HC_SECOND_PASSWD_LOGIN 0x08B9).
#![allow(dead_code)]

use rustnarok_client::net;

pub const TITLE_MESSAGE: usize = 1899;
pub const ENTER_MESSAGE: usize = 2566;
pub const RECOMMEND_MESSAGE: usize = 1900;
pub const MOUSE_HINT_MESSAGE: usize = 1906;
pub const LOCKOUT_WARNING_MESSAGE: usize = 1907;
pub const AUTH_OK_MESSAGE: usize = 1888;
pub const CREATED_MESSAGE: usize = 1889;
pub const CHANGED_MESSAGE: usize = 1891;
pub const WRONG_MESSAGE: usize = 1892;
pub const MISMATCH_MESSAGE: usize = 1893;
pub const RESTRICTED_MESSAGE: usize = 1896;

const MAX_TRIES: u8 = 3;

/// rathena's `pincode_allowed` (`src/char/char_clif.cpp`): 4 ASCII digits, not all identical
/// (`pincode_allow_repeated: no`), and not an ascending or descending run wrapping past 9 (`pincode_allow_sequential: no`).
pub fn pincode_allowed(pin: &str) -> bool {
    if pin.len() != 4 || !pin.bytes().all(|byte| byte.is_ascii_digit()) {
        return false;
    }
    let digits: Vec<u8> = pin.bytes().map(|byte| byte - b'0').collect();
    if digits.iter().all(|&digit| digit == digits[0]) {
        return false;
    }
    let ascending = (1..4).all(|i| (digits[0] + i as u8) % 10 == digits[i]);
    let descending = (1..4).all(|i| (digits[0] + 10 - i as u8) % 10 == digits[i]);
    !ascending && !descending
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PinStep {
    /// State 1: enter existing 4-digit PIN.
    Ask,
    /// States 2 and 4: enter new 4-digit PIN (first time).
    NewFirst,
    /// States 2 and 4: re-enter new 4-digit PIN to confirm.
    NewConfirm { first: String },
    /// State 3 or player-chosen change from state 1: enter current 4-digit PIN.
    ChangeOld,
    /// Enter new 4-digit PIN (first time) after entering `old`.
    ChangeNewFirst { old: String },
    /// Re-enter new 4-digit PIN to confirm after entering `old` and `first`.
    ChangeNewConfirm { old: String, first: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinButton {
    /// Pad button index `0..=9` (shows digit `tab[index]`).
    Digit(u8),
    /// Clear button (`btn_big_numr`).
    Clear,
    /// Offer to change PIN while in `Ask` (ponytail: estimate).
    Change,
    /// Submit current 4 digits.
    Ok,
    /// Cancel change flow (if started from `Ask`) or leave character select back to login.
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PinAction {
    None,
    /// Send `CH_SECOND_PASSWD_ACK` (`0x08B8`).
    Check {
        seed: u32,
        pin: String,
    },
    /// Send `CH_MAKE_SECOND_PASSWD` (`0x08BA`).
    Make {
        seed: u32,
        pin: String,
    },
    /// Send `CH_EDIT_SECOND_PASSWD` (`0x08BE`).
    Change {
        seed: u32,
        old_pin: String,
        new_pin: String,
    },
    /// Show a message box over character select (`MISMATCH_MESSAGE` 1893 or `RESTRICTED_MESSAGE` 1896).
    Message(usize),
    /// Close character select and return to login screen.
    Close,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PinServerOutcome {
    /// State 0: PIN verified or not needed; pad closes and character select is unlocked.
    Passed,
    /// Pad stays open (states 1, 2, 3, 4).
    Prompt,
    /// State 5 or state 8 (< 3 tries): show message `id` (with `tries_left` for 1892) over character select.
    Message { id: usize, tries_left: u8 },
    /// State 8 at 3 wrong tries: show message `1907` and return to login.
    LockedOut(usize),
}

#[derive(Debug, Clone)]
pub struct PinPad {
    pub seed: u32,
    pub step: PinStep,
    /// Whether the server forced the change flow (state 3) rather than the player clicking Change at state 1.
    pub forced_change: bool,
    /// Logical digits (`'0'..='9'`) entered in the current 4-digit step.
    pub input: String,
    pub wrong_tries: u8,
}

impl PinPad {
    pub fn new(seed: u32, step: PinStep, forced_change: bool) -> Self {
        Self { seed, step, forced_change, input: String::new(), wrong_tries: 0 }
    }

    /// Applies a `0x08B9` (`HC_SECOND_PASSWD_LOGIN`) state from the char server.
    pub fn apply_server(pad: &mut Option<Self>, seed: u32, state: u16) -> PinServerOutcome {
        match state {
            0 => {
                *pad = None;
                PinServerOutcome::Passed
            }
            1 => {
                let wrong_tries = pad.as_ref().map_or(0, |existing| existing.wrong_tries);
                let mut next = Self::new(seed, PinStep::Ask, false);
                next.wrong_tries = wrong_tries;
                *pad = Some(next);
                PinServerOutcome::Prompt
            }
            2 | 4 => {
                *pad = Some(Self::new(seed, PinStep::NewFirst, false));
                PinServerOutcome::Prompt
            }
            3 => {
                *pad = Some(Self::new(seed, PinStep::ChangeOld, true));
                PinServerOutcome::Prompt
            }
            5 => {
                if let Some(existing) = pad.as_mut() {
                    existing.seed = seed;
                    existing.input.clear();
                    existing.step = match &existing.step {
                        PinStep::ChangeOld | PinStep::ChangeNewFirst { .. } | PinStep::ChangeNewConfirm { .. } => PinStep::ChangeOld,
                        _ => PinStep::NewFirst,
                    };
                } else {
                    *pad = Some(Self::new(seed, PinStep::NewFirst, false));
                }
                PinServerOutcome::Message { id: RESTRICTED_MESSAGE, tries_left: 0 }
            }
            8 => {
                let tries = pad.as_ref().map_or(1, |existing| existing.wrong_tries.saturating_add(1));
                if tries >= MAX_TRIES {
                    *pad = None;
                    // ponytail: after 3 wrong PINs rathena logs the account out; return to login with message 1907.
                    return PinServerOutcome::LockedOut(LOCKOUT_WARNING_MESSAGE);
                }
                if let Some(existing) = pad.as_mut() {
                    existing.seed = seed;
                    existing.input.clear();
                    existing.wrong_tries = tries;
                    existing.step = if existing.forced_change { PinStep::ChangeOld } else { PinStep::Ask };
                } else {
                    let mut next = Self::new(seed, PinStep::Ask, false);
                    next.wrong_tries = tries;
                    *pad = Some(next);
                }
                PinServerOutcome::Message { id: WRONG_MESSAGE, tries_left: MAX_TRIES - tries }
            }
            _ => PinServerOutcome::Prompt,
        }
    }

    /// Control-state `pin` string: `"ask"`, `"new"`, or `"change"`.
    pub fn kind(&self) -> &'static str {
        match self.step {
            PinStep::Ask => "ask",
            PinStep::NewFirst | PinStep::NewConfirm { .. } => "new",
            PinStep::ChangeOld | PinStep::ChangeNewFirst { .. } | PinStep::ChangeNewConfirm { .. } => "change",
        }
    }

    /// The 10-element digit permutation for `self.seed` (`tab[button]` is the digit `0..=9` shown on `button`).
    pub fn table(&self) -> [u8; 10] {
        net::pin_table(self.seed)
    }

    /// Prompt message ID shown at the top of the pad.
    pub fn prompt_message(&self) -> usize {
        // ponytail: which message shows for each step is an estimate from msgstringtable.
        match self.step {
            PinStep::Ask | PinStep::ChangeOld => ENTER_MESSAGE,
            PinStep::NewFirst | PinStep::NewConfirm { .. } | PinStep::ChangeNewFirst { .. } | PinStep::ChangeNewConfirm { .. } => RECOMMEND_MESSAGE,
        }
    }

    /// Presses a button on the pad.
    pub fn press(&mut self, button: PinButton) -> PinAction {
        match button {
            PinButton::Digit(index) if index < 10 => {
                let digit = char::from(b'0' + self.table()[usize::from(index)]);
                self.push_digit(digit)
            }
            PinButton::Digit(_) => PinAction::None,
            PinButton::Clear => {
                self.input.clear();
                PinAction::None
            }
            PinButton::Change => {
                // ponytail: how the official pad offers a voluntary PIN change at state 1 is an estimate.
                if self.step == PinStep::Ask {
                    self.step = PinStep::ChangeOld;
                    self.input.clear();
                }
                PinAction::None
            }
            PinButton::Ok => {
                if self.input.len() == 4 {
                    self.complete_entry()
                } else {
                    PinAction::None
                }
            }
            PinButton::Cancel => {
                if !self.forced_change && matches!(self.step, PinStep::ChangeOld | PinStep::ChangeNewFirst { .. } | PinStep::ChangeNewConfirm { .. }) {
                    self.step = PinStep::Ask;
                    self.input.clear();
                    PinAction::None
                } else {
                    PinAction::Close
                }
            }
        }
    }

    /// Enters a logical digit `'0'..='9'` (either from clicking the button showing that digit or pressing a digit key).
    pub fn push_digit(&mut self, digit: char) -> PinAction {
        if !digit.is_ascii_digit() || self.input.len() >= 4 {
            return PinAction::None;
        }
        self.input.push(digit);
        if self.input.len() == 4 { self.complete_entry() } else { PinAction::None }
    }

    pub fn backspace(&mut self) {
        self.input.pop();
    }

    fn complete_entry(&mut self) -> PinAction {
        let entered = std::mem::take(&mut self.input);
        match &self.step {
            PinStep::Ask => PinAction::Check { seed: self.seed, pin: entered },
            PinStep::NewFirst => {
                self.step = PinStep::NewConfirm { first: entered };
                PinAction::None
            }
            PinStep::NewConfirm { first } => {
                let first = first.clone();
                if entered != first {
                    self.step = PinStep::NewFirst;
                    PinAction::Message(MISMATCH_MESSAGE)
                } else if !pincode_allowed(&entered) {
                    self.step = PinStep::NewFirst;
                    PinAction::Message(RESTRICTED_MESSAGE)
                } else {
                    PinAction::Make { seed: self.seed, pin: entered }
                }
            }
            PinStep::ChangeOld => {
                self.step = PinStep::ChangeNewFirst { old: entered };
                PinAction::None
            }
            PinStep::ChangeNewFirst { old } => {
                self.step = PinStep::ChangeNewConfirm { old: old.clone(), first: entered };
                PinAction::None
            }
            PinStep::ChangeNewConfirm { old, first } => {
                let (old, first) = (old.clone(), first.clone());
                if entered != first {
                    self.step = PinStep::ChangeNewFirst { old };
                    PinAction::Message(MISMATCH_MESSAGE)
                } else if !pincode_allowed(&entered) {
                    self.step = PinStep::ChangeNewFirst { old };
                    PinAction::Message(RESTRICTED_MESSAGE)
                } else {
                    PinAction::Change { seed: self.seed, old_pin: old, new_pin: entered }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_allowed_pincodes() {
        for bad in ["", "123", "12345", "abcd", "0000", "7777", "1234", "8901", "9012", "3210", "1098", "9876"] {
            assert!(!pincode_allowed(bad), "{bad} should be disallowed");
        }
        for good in ["1357", "2468", "0192"] {
            assert!(pincode_allowed(good), "{good} should be allowed");
        }
    }

    #[test]
    fn walks_ask_new_and_change_flows() {
        let mut pad = None;

        // State 1 (Ask): pressing the 4 buttons corresponding to "1357" returns Check.
        assert_eq!(PinPad::apply_server(&mut pad, 0x1234_5678, 1), PinServerOutcome::Prompt);
        let p = pad.as_mut().unwrap();
        assert_eq!(p.kind(), "ask");
        let table = p.table();
        let mut action = PinAction::None;
        for ch in "1357".bytes() {
            let button = table.iter().position(|&d| d == ch - b'0').unwrap() as u8;
            action = p.press(PinButton::Digit(button));
        }
        assert_eq!(action, PinAction::Check { seed: 0x1234_5678, pin: "1357".into() });

        // State 8 twice returns tries_left 2 then 1, and 3rd state 8 locks out (1907) and clears pad.
        assert_eq!(PinPad::apply_server(&mut pad, 0x1111, 8), PinServerOutcome::Message { id: WRONG_MESSAGE, tries_left: 2 });
        assert_eq!(PinPad::apply_server(&mut pad, 0x2222, 8), PinServerOutcome::Message { id: WRONG_MESSAGE, tries_left: 1 });
        assert_eq!(PinPad::apply_server(&mut pad, 0x3333, 8), PinServerOutcome::LockedOut(LOCKOUT_WARNING_MESSAGE));
        assert!(pad.is_none());

        // State 4 (NewFirst -> NewConfirm): mismatch -> 1893, disallowed "1234" -> 1896, matching "1357" -> Make.
        assert_eq!(PinPad::apply_server(&mut pad, 0xABCD, 4), PinServerOutcome::Prompt);
        let p = pad.as_mut().unwrap();
        assert_eq!(p.kind(), "new");
        for ch in "1357".chars() {
            assert_eq!(p.push_digit(ch), PinAction::None);
        }
        assert_eq!(p.step, PinStep::NewConfirm { first: "1357".into() });
        for ch in "2468".chars() {
            action = p.push_digit(ch);
        }
        assert_eq!(action, PinAction::Message(MISMATCH_MESSAGE));
        assert_eq!(p.step, PinStep::NewFirst);

        for _ in 0..2 {
            for ch in "1234".chars() {
                action = p.push_digit(ch);
            }
        }
        assert_eq!(action, PinAction::Message(RESTRICTED_MESSAGE));
        assert_eq!(p.step, PinStep::NewFirst);

        for _ in 0..2 {
            for ch in "1357".chars() {
                action = p.push_digit(ch);
            }
        }
        assert_eq!(action, PinAction::Make { seed: 0xABCD, pin: "1357".into() });

        // Voluntary change from state 1 (PinButton::Change).
        assert_eq!(PinPad::apply_server(&mut pad, 0x5555, 1), PinServerOutcome::Prompt);
        let p = pad.as_mut().unwrap();
        assert_eq!(p.press(PinButton::Change), PinAction::None);
        assert_eq!(p.kind(), "change");
        for ch in "1357".chars() {
            assert_eq!(p.push_digit(ch), PinAction::None);
        }
        for _ in 0..2 {
            for ch in "2468".chars() {
                action = p.push_digit(ch);
            }
        }
        assert_eq!(action, PinAction::Change { seed: 0x5555, old_pin: "1357".into(), new_pin: "2468".into() });

        // Forced state 3 (ChangeOld -> ChangeNewFirst -> ChangeNewConfirm).
        assert_eq!(PinPad::apply_server(&mut pad, 0x6666, 3), PinServerOutcome::Prompt);
        let p = pad.as_mut().unwrap();
        assert!(p.forced_change);
        for ch in "1357".chars() {
            assert_eq!(p.push_digit(ch), PinAction::None);
        }
        for _ in 0..2 {
            for ch in "2468".chars() {
                action = p.push_digit(ch);
            }
        }
        assert_eq!(action, PinAction::Change { seed: 0x6666, old_pin: "1357".into(), new_pin: "2468".into() });

        // State 0 sets pad to None and returns Passed.
        assert_eq!(PinPad::apply_server(&mut pad, 0x7777, 0), PinServerOutcome::Passed);
        assert!(pad.is_none());
    }
}
