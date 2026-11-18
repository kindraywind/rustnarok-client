//! The bank (the basic information menu's bank button): what the account keeps, what's carried, and an amount to put
//! in or take out.

use rustnarok_client::grf::Grf;
use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};
use crate::shop::group;
use crate::window::{BUTTON_SIZE, Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI, contains};

// Places inside the window, title bar included, over bg_bank.bmp (280x136 at (0, 17): a white top with the two
// amounts, a grey band with the money bag, the box and Deposit over Withdraw, a striped footer with the quick buttons).
const WIDTH: f32 = 280.0;
const HEIGHT: f32 = TITLE_HEIGHT + 136.0;
/// The labels; both amounts start at the width of "In Bank:" plus VALUE_GAP.
const BALANCE: [f32; 2] = [30.0, 29.0];
const CARRIED: [f32; 2] = [30.0, 46.0];
const VALUE_GAP: f32 = 43.0;
const AMOUNT: [f32; 2] = [78.0, 90.0];
const AMOUNT_SIZE: [f32; 2] = [99.0, 17.0];
/// The +1 and -1 zeny arrows, touching the box's right edge.
const ARROWS: [([f32; 2], [f32; 2]); 2] = [([177.0, 89.0], [18.0, 10.0]), ([177.0, 99.0], [18.0, 9.0])];
const DEPOSIT: [f32; 2] = [209.0, 77.0];
const WITHDRAW: [f32; 2] = [209.0, 102.0];
const BUTTON: [f32; 2] = [42.0, 20.0];
/// The footer row: Max, then 1000만, 100만 and 10만 zeny (ten million, a million, a hundred thousand) added to the
/// amount (the English pictures say "+100TSN"), 2 px apart.
const MAX: [f32; 2] = [45.0, 132.0];
const QUICK: [f32; 2] = [93.0, 132.0];
const QUICK_SIZE: [f32; 2] = [46.0, 16.0];
const QUICK_AMOUNTS: [i64; 3] = [10_000_000, 1_000_000, 100_000];
/// The red status line, centred on the window.
const STATUS_Y: f32 = 112.0;
const FIELD: [u8; 4] = [0xF7, 0xF7, 0xF7, 0xFF];
/// The most zeny anyone can hold, and so move at once.
const MOST: i64 = 2_147_483_647;
const TITLE: usize = 2771;
const IN_BANK: usize = 2772;
const ON_HAND: usize = 2773;
/// What the Max button writes into the box.
const MAX_WORD: usize = 2778;
/// The client's own refusals, as a chat line: a crafting window open, nothing typed, not a number, over the most,
/// under 1, no zeny, more than carried, more than kept, more than the bank or the purse can hold.
const BUSY: usize = 2489;
const NOTHING_TYPED: usize = 2779;
const NOT_A_NUMBER: usize = 2488;
const OVER_MOST: usize = 2768;
const UNDER_ONE: usize = 2769;
const NO_ZENY: usize = 2770;
const NOT_CARRIED: usize = 2456;
const BALANCE_SHORT: usize = 2455;
const MAX_ZENY: usize = 2787;
const CANNOT_HOLD: usize = 2459;
/// ... and as the status line.
const INPUT_REQUIRED: usize = 2781;
const NUMBERS_ONLY: usize = 2782;
const OVER_INPUT: usize = 2783;
const UNDER_INPUT: usize = 2784;
const SHORT_ZENY: usize = 2780;
const SHORT_CARRIED: usize = 2785;
const SHORT_FUNDS: usize = 2786;
/// The buttons' tooltips: Deposit, Withdraw, "+ 1z", "- 1z", Max.
const DEPOSIT_TIP: usize = 2774;
const WITHDRAW_TIP: usize = 2775;
const UP_TIP: usize = 2776;
const DOWN_TIP: usize = 2777;

pub struct Assets {
    background: Texture,
    /// Deposit, withdraw, the three quick amounts, Max, +1, -1: out, over, pressed.
    buttons: [[Texture; 3]; 8],
    field: Texture,
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |name: String| gpu.load_texture(grf, &format!("{UI}bank\\{name}.bmp"), false);
        let button = |name: &str| ["out", "over", "press"].map(|look| image(format!("btn_{name}_{look}")));
        Self {
            background: image("bg_bank".into()),
            buttons: ["deposit", "withdraw", "1000mil", "100mil", "10mil", "max", "upper", "buttom"].map(button),
            field: gpu.solid(FIELD),
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(TitleHit),
    Amount,
    /// The arrows: 0 adds one zeny, 1 takes one off.
    Arrow(usize),
    Max,
    /// The quick amounts in QUICK_AMOUNTS' order.
    Quick(usize),
    Deposit,
    Withdraw,
    Window,
}

#[derive(Debug, PartialEq)]
pub enum Action {
    Deposit(i32),
    Withdraw(i32),
    /// The client's own refusal, a chat line's message number.
    Refuse(usize),
    Close,
}

pub struct Bank {
    pub frame: Frame,
    /// What the account keeps.
    pub balance: i64,
    pub typed: String,
    pub typing: bool,
    /// The red line in the window after a refusal, a message number, until the next request goes out.
    pub status: Option<usize>,
    /// The Max button's word: with it in the box, Deposit means all carried and Withdraw all kept.
    max_word: String,
}

impl Bank {
    pub fn new(messages: &[String]) -> Self {
        let [width, height] = crate::screen();
        let mut frame = Frame::new([(width / 2.0 - 140.0).trunc(), (height / 2.0 - 77.0).trunc()], WIDTH);
        frame.open = true;
        let max_word = messages.get(MAX_WORD).cloned().unwrap_or_default();
        Self { frame, balance: 0, typed: String::new(), typing: false, status: None, max_word }
    }

    /// The balance's answer: opens the bank, or closes it when it is open (so the menu's bank button toggles it).
    pub fn answer(bank: &mut Option<Self>, balance: i64, messages: &[String]) {
        if bank.take().is_none() {
            let mut opened = Self::new(messages);
            opened.balance = balance;
            *bank = Some(opened);
        }
    }

    fn quick(place: usize) -> [f32; 2] {
        [QUICK[0] + place as f32 * (QUICK_SIZE[0] + 2.0), QUICK[1]]
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let local = self.frame.local(cursor, HEIGHT)?;
        // No minimize button: that spot is title bar.
        match self.frame.title_hit(local) {
            Some(TitleHit::Minimize) => return Some(Hit::Title(TitleHit::Title)),
            Some(hit) => return Some(Hit::Title(hit)),
            None => {}
        }
        let quick = (0..3).find(|&place| contains(Self::quick(place), QUICK_SIZE, local));
        let arrow = (0..2).find(|&place| contains(ARROWS[place].0, ARROWS[place].1, local));
        Some(match (quick, arrow) {
            (Some(place), _) => Hit::Quick(place),
            (_, Some(place)) => Hit::Arrow(place),
            _ if contains(MAX, QUICK_SIZE, local) => Hit::Max,
            _ if contains(AMOUNT, AMOUNT_SIZE, local) => Hit::Amount,
            _ if contains(DEPOSIT, BUTTON, local) => Hit::Deposit,
            _ if contains(WITHDRAW, BUTTON, local) => Hit::Withdraw,
            _ => Hit::Window,
        })
    }

    /// `busy`: a window that takes the bank's place is open (Laphine synthesis or upgrade, item reform).
    pub fn click(&mut self, hit: Hit, carried: u32, busy: bool) -> Option<Action> {
        // The box keeps the keyboard after a step button or Max.
        self.typing = matches!(hit, Hit::Amount | Hit::Arrow(_) | Hit::Max | Hit::Quick(_));
        // A box with no number (empty, or Max) steps from 0.
        let step = |typed: &str, by: i64| typed.parse::<i64>().unwrap_or(0).saturating_add(by).clamp(0, MOST).to_string();
        match hit {
            Hit::Title(TitleHit::Close) => return Some(Action::Close),
            Hit::Title(title) => self.frame.click_title(title),
            Hit::Arrow(place) => self.typed = step(&self.typed, if place == 0 { 1 } else { -1 }),
            Hit::Quick(place) => self.typed = step(&self.typed, QUICK_AMOUNTS[place.min(2)]),
            Hit::Max => self.typed.clone_from(&self.max_word),
            Hit::Deposit | Hit::Withdraw => return self.check(hit == Hit::Deposit, carried, busy),
            Hit::Amount | Hit::Window => {}
        }
        None
    }

    /// The client's checks before asking the server: the box is read and emptied; a refusal is a chat line and the
    /// status line; nothing goes out for Max that comes to 0.
    fn check(&mut self, deposit: bool, carried: u32, busy: bool) -> Option<Action> {
        if busy {
            return Some(Action::Refuse(BUSY));
        }
        let typed = std::mem::take(&mut self.typed);
        let carried = i64::from(carried);
        // What the giving side has and what the taking side can still hold.
        let (have, room) = if deposit { (carried, MOST - self.balance) } else { (self.balance, MOST - carried) };
        let amount = if !typed.is_empty() && typed == self.max_word {
            match (have, room) {
                (..=0, _) => Err((NO_ZENY, SHORT_ZENY)),
                (_, ..=0) if deposit => Err((MAX_ZENY, OVER_INPUT)),
                _ => Ok(have.min(room).max(0)),
            }
        } else if typed.is_empty() {
            Err((NOTHING_TYPED, INPUT_REQUIRED))
        } else {
            match typed.parse::<i64>() {
                Err(_) => Err((NOT_A_NUMBER, NUMBERS_ONLY)),
                Ok(amount) if amount > MOST => Err((OVER_MOST, OVER_INPUT)),
                Ok(amount) if amount < 1 => Err((UNDER_ONE, UNDER_INPUT)),
                Ok(amount) if amount > have => Err(if deposit { (NOT_CARRIED, SHORT_CARRIED) } else { (BALANCE_SHORT, SHORT_FUNDS) }),
                Ok(amount) if amount > room => Err(if deposit { (MAX_ZENY, OVER_INPUT) } else { (CANNOT_HOLD, MAX_ZENY) }),
                Ok(amount) => Ok(amount),
            }
        };
        match amount {
            Err((chat, status)) => {
                self.status = Some(status);
                Some(Action::Refuse(chat))
            }
            Ok(0) => None,
            Ok(amount) => {
                self.status = None;
                Some(if deposit { Action::Deposit(amount as i32) } else { Action::Withdraw(amount as i32) })
            }
        }
    }

    pub fn type_text(&mut self, text: &str) {
        let room = 10usize.saturating_sub(self.typed.len());
        self.typed.extend(text.chars().filter(char::is_ascii_digit).take(room));
    }

    /// The box, the buttons and the status line on screen, for the test driver.
    pub fn button_boxes(&self) -> Vec<(&'static str, [f32; 4])> {
        let place = |local: [f32; 2], size: [f32; 2]| {
            let [x, y] = self.frame.at(local);
            [x, y, size[0], size[1]]
        };
        let mut boxes = vec![
            ("bank_amount", place(AMOUNT, AMOUNT_SIZE)),
            ("bank_up", place(ARROWS[0].0, ARROWS[0].1)),
            ("bank_down", place(ARROWS[1].0, ARROWS[1].1)),
            ("bank_deposit", place(DEPOSIT, BUTTON)),
            ("bank_withdraw", place(WITHDRAW, BUTTON)),
            ("bank_max", place(MAX, QUICK_SIZE)),
            ("bank_close", place(self.frame.close_button(), BUTTON_SIZE)),
        ];
        boxes.extend(["bank_10m", "bank_1m", "bank_100k"].into_iter().enumerate().map(|(index, name)| (name, place(Self::quick(index), QUICK_SIZE))));
        boxes
    }

    /// The status line's row on screen, when there is one.
    pub fn status_box(&self) -> Option<[f32; 4]> {
        let [x, y] = self.frame.at([0.0, STATUS_Y]);
        self.status.map(|_| [x, y, WIDTH, 12.0])
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &self,
        layer: &mut Layer<'a>,
        tooltip: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        messages: &[String],
        font: &Font,
        pointer: ([f32; 2], bool),
        caret: bool,
        carried: u32,
    ) {
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        let at = |local: [f32; 2]| self.frame.at(local);
        self.frame.draw_title_no_minimize(layer, chrome, message(TITLE), pointer.0);
        let hit = self.hit(pointer.0);
        let look = |target: Hit| {
            let hovered = hit == Some(target);
            usize::from(hovered) + usize::from(hovered && pointer.1)
        };
        layer.sprites.push((&assets.background, at([0.0, TITLE_HEIGHT]), assets.background.size));
        // Both amounts start right after the longer label's room, "1,234 z".
        let value_x = font.width(message(IN_BANK)) + VALUE_GAP;
        for (label, value, place) in [(IN_BANK, self.balance.max(0), BALANCE), (ON_HAND, i64::from(carried), CARRIED)] {
            layer.texts.push((message(label).to_owned(), at(place), [0, 0, 0]));
            layer.texts.push((format!("{} z", group(value as u64)), at([value_x, place[1]]), [0, 0, 0]));
        }
        layer.sprites.push((&assets.field, at(AMOUNT), AMOUNT_SIZE));
        let typed = if self.typing && caret { format!("{}|", self.typed) } else { self.typed.clone() };
        layer.texts.push((typed.clone(), at([AMOUNT[0] + AMOUNT_SIZE[0] - 4.0 - font.width(&typed), AMOUNT[1] + 3.0]), [0, 0, 0]));
        for (place, (corner, size)) in ARROWS.into_iter().enumerate() {
            layer.sprites.push((&assets.buttons[6 + place][look(Hit::Arrow(place))], at(corner), size));
        }
        layer.sprites.push((&assets.buttons[5][look(Hit::Max)], at(MAX), QUICK_SIZE));
        for place in 0..3 {
            layer.sprites.push((&assets.buttons[2 + place][look(Hit::Quick(place))], at(Self::quick(place)), QUICK_SIZE));
        }
        layer.sprites.push((&assets.buttons[0][look(Hit::Deposit)], at(DEPOSIT), BUTTON));
        layer.sprites.push((&assets.buttons[1][look(Hit::Withdraw)], at(WITHDRAW), BUTTON));
        if let Some(status) = self.status.map(message) {
            layer.texts.push((status.to_owned(), at([((WIDTH - font.width(status)) / 2.0).trunc(), STATUS_Y]), [255, 0, 0]));
        }
        // Each button names itself, 20 px above it.
        let tip = match hit {
            Some(Hit::Deposit) => Some((message(DEPOSIT_TIP).to_owned(), DEPOSIT)),
            Some(Hit::Withdraw) => Some((message(WITHDRAW_TIP).to_owned(), WITHDRAW)),
            Some(Hit::Arrow(place)) => Some((message(if place == 0 { UP_TIP } else { DOWN_TIP }).to_owned(), ARROWS[place.min(1)].0)),
            Some(Hit::Max) => Some((message(MAX_WORD).to_owned(), MAX)),
            Some(Hit::Quick(place)) => Some((format!("+ {}", group(QUICK_AMOUNTS[place.min(2)] as u64)), Self::quick(place))),
            _ => None,
        };
        if let Some((text, corner)) = tip.filter(|(text, _)| !text.is_empty()) {
            crate::window::tooltip(tooltip, chrome, font, &text, at([corner[0], corner[1] - 20.0]), [255; 3]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn messages() -> Vec<String> {
        let mut messages = vec![String::new(); 3000];
        messages[MAX_WORD] = "Max".into();
        messages
    }

    fn bank(balance: i64) -> Bank {
        let mut bank = Bank::new(&messages());
        bank.frame.position = [0.0, 0.0];
        bank.balance = balance;
        bank
    }

    /// A press with `typed` in the box: the action, and the status line after it.
    fn press(bank: &mut Bank, typed: &str, deposit: bool, carried: u32) -> (Option<Action>, Option<usize>) {
        bank.typed = typed.into();
        let action = bank.click(if deposit { Hit::Deposit } else { Hit::Withdraw }, carried, false);
        assert!(bank.typed.is_empty(), "the box is emptied by a press");
        (action, bank.status)
    }

    #[test]
    fn buttons_sit_in_the_band_and_the_footer() {
        let bank = bank(0);
        let centre = |corner: [f32; 2], size: [f32; 2]| [corner[0] + size[0] / 2.0, corner[1] + size[1] / 2.0];
        assert_eq!(bank.hit(centre([78.0, 90.0], [99.0, 17.0])), Some(Hit::Amount));
        assert_eq!(bank.hit([186.0, 93.0]), Some(Hit::Arrow(0)));
        assert_eq!(bank.hit([186.0, 103.0]), Some(Hit::Arrow(1)));
        // Deposit over Withdraw, beside the box.
        assert_eq!(bank.hit(centre([209.0, 77.0], [42.0, 20.0])), Some(Hit::Deposit));
        assert_eq!(bank.hit(centre([209.0, 102.0], [42.0, 20.0])), Some(Hit::Withdraw));
        // The footer: Max first, then ten million, a million, a hundred thousand.
        for (x, hit) in [(45.0, Hit::Max), (93.0, Hit::Quick(0)), (141.0, Hit::Quick(1)), (189.0, Hit::Quick(2))] {
            assert_eq!(bank.hit(centre([x, 132.0], [46.0, 16.0])), Some(hit));
        }
        // No minimize button: its spot is title bar.
        assert_eq!(bank.hit([256.0, 8.0]), Some(Hit::Title(TitleHit::Title)));
        assert_eq!(bank.hit([270.0, 8.0]), Some(Hit::Title(TitleHit::Close)));
    }

    #[test]
    fn opens_at_the_middle() {
        let bank = Bank::new(&messages());
        let [width, height] = crate::screen();
        assert_eq!(bank.frame.position, [(width / 2.0 - 140.0).trunc(), (height / 2.0 - 77.0).trunc()]);
    }

    #[test]
    fn steps_and_max() {
        let mut bank = bank(1000);
        bank.click(Hit::Quick(2), 5000, false);
        bank.click(Hit::Quick(1), 5000, false);
        assert_eq!(bank.typed, "1100000");
        assert!(bank.typing, "the box keeps the keyboard after a step");
        bank.click(Hit::Max, 5000, false);
        assert_eq!(bank.typed, "Max");
        // Max counts as 0 for a step.
        bank.click(Hit::Arrow(0), 5000, false);
        assert_eq!(bank.typed, "1");
        bank.click(Hit::Arrow(1), 5000, false);
        bank.click(Hit::Arrow(1), 5000, false);
        assert_eq!(bank.typed, "0");
        bank.typed = "2147483647".into();
        bank.click(Hit::Quick(0), 5000, false);
        assert_eq!(bank.typed, "2147483647");
        // Max: all carried in, all kept out, each cut at what the other side can hold.
        assert_eq!(press(&mut bank, "Max", true, 5000).0, Some(Action::Deposit(5000)));
        assert_eq!(press(&mut bank, "Max", false, 5000).0, Some(Action::Withdraw(1000)));
        bank.balance = MOST - 10;
        assert_eq!(press(&mut bank, "Max", true, 5000).0, Some(Action::Deposit(10)));
        assert_eq!(press(&mut bank, "Max", false, 2_147_483_000).0, Some(Action::Withdraw(647)));
        assert_eq!(press(&mut bank, "Max", false, MOST as u32), (None, None));
    }

    #[test]
    fn refusals_give_a_chat_line_and_a_status_line() {
        let mut bank = bank(1000);
        assert_eq!(press(&mut bank, "", true, 5000), (Some(Action::Refuse(NOTHING_TYPED)), Some(INPUT_REQUIRED)));
        assert_eq!(press(&mut bank, "Max5", true, 5000), (Some(Action::Refuse(NOT_A_NUMBER)), Some(NUMBERS_ONLY)));
        assert_eq!(press(&mut bank, "3000000000", false, 5000), (Some(Action::Refuse(OVER_MOST)), Some(OVER_INPUT)));
        assert_eq!(press(&mut bank, "0", true, 5000), (Some(Action::Refuse(UNDER_ONE)), Some(UNDER_INPUT)));
        assert_eq!(press(&mut bank, "Max", true, 0), (Some(Action::Refuse(NO_ZENY)), Some(SHORT_ZENY)));
        assert_eq!(press(&mut bank, "6000", true, 5000), (Some(Action::Refuse(NOT_CARRIED)), Some(SHORT_CARRIED)));
        assert_eq!(press(&mut bank, "2000", false, 5000), (Some(Action::Refuse(BALANCE_SHORT)), Some(SHORT_FUNDS)));
        assert_eq!(press(&mut bank, "500", false, MOST as u32 - 100), (Some(Action::Refuse(CANNOT_HOLD)), Some(MAX_ZENY)));
        bank.balance = 0;
        assert_eq!(press(&mut bank, "Max", false, 5000), (Some(Action::Refuse(NO_ZENY)), Some(SHORT_ZENY)));
        bank.balance = MOST - 100;
        assert_eq!(press(&mut bank, "500", true, 5000), (Some(Action::Refuse(MAX_ZENY)), Some(OVER_INPUT)));
        bank.balance = MOST;
        assert_eq!(press(&mut bank, "Max", true, 5000), (Some(Action::Refuse(MAX_ZENY)), Some(OVER_INPUT)));
        // A crafting window open: the chat line only, the box and the status line as they were.
        bank.typed = "100".into();
        assert_eq!(bank.click(Hit::Deposit, 5000, true), Some(Action::Refuse(BUSY)));
        assert_eq!((bank.typed.as_str(), bank.status), ("100", Some(OVER_INPUT)));
        // A request that goes out clears the status line.
        bank.balance = 1000;
        assert_eq!(press(&mut bank, "1000", true, 5000), (Some(Action::Deposit(1000)), None));
        assert_eq!(bank.click(Hit::Title(TitleHit::Close), 5000, false), Some(Action::Close));
    }

    #[test]
    fn a_second_balance_answer_closes_it() {
        let mut bank = None;
        Bank::answer(&mut bank, 1234, &messages());
        assert_eq!(bank.as_ref().map(|bank| bank.balance), Some(1234));
        Bank::answer(&mut bank, 1234, &messages());
        assert!(bank.is_none());
    }
}
