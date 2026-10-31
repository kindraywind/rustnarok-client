//! The bank (the basic information menu's bank button): what the account keeps, what's carried, and an amount to put
//! in or take out.

use rustnarok_client::grf::Grf;
use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};
use crate::shop::group;
use crate::window::{Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI, contains, offset};

// ponytail: places over bg_bank.bmp (280x136: the money bag in a grey band) are estimates; calibrate against the
// real client.
const WIDTH: f32 = 280.0;
const HEIGHT: f32 = TITLE_HEIGHT + 136.0;
pub const POSITION: [f32; 2] = [372.0, 250.0];
const BALANCE: [f32; 2] = [10.0, TITLE_HEIGHT + 12.0];
const CARRIED: [f32; 2] = [10.0, TITLE_HEIGHT + 30.0];
const AMOUNT: [f32; 2] = [80.0, TITLE_HEIGHT + 61.0];
const AMOUNT_SIZE: [f32; 2] = [150.0, 18.0];
/// The +1 and -1 zeny arrows beside the amount.
const ARROWS: [[f32; 2]; 2] = [[236.0, TITLE_HEIGHT + 60.0], [236.0, TITLE_HEIGHT + 70.0]];
const ARROW: [f32; 2] = [18.0, 9.0];
/// 10만, 100만 and 1000만 zeny (a hundred thousand, a million, ten million) added to the amount (the English
/// pictures say "+100TSN"), then Max (all carried).
const QUICK: [f32; 2] = [80.0, TITLE_HEIGHT + 86.0];
const QUICK_SIZE: [f32; 2] = [46.0, 16.0];
const QUICK_AMOUNTS: [u64; 3] = [100_000, 1_000_000, 10_000_000];
const DEPOSIT: [f32; 2] = [WIDTH - 98.0, HEIGHT - 23.0];
const WITHDRAW: [f32; 2] = [WIDTH - 52.0, HEIGHT - 23.0];
const BUTTON: [f32; 2] = [42.0, 20.0];
const FIELD: [u8; 4] = [0xFF, 0xFF, 0xFF, 0xFF];
/// The most zeny anyone can hold, and so move at once.
const MOST: u64 = 2_147_483_647;
// ponytail: "Bank" (3135) over "Zeny Storage" (2771); which the real client uses isn't knowable.
const TITLE: usize = 3135;
const IN_BANK: usize = 2772;
const ON_HAND: usize = 2773;
/// The client's own refusals: nothing typed, under 1, over the most, no zeny, more than carried, more than kept.
const NOTHING_TYPED: usize = 2779;
const UNDER_ONE: usize = 2769;
const OVER_MOST: usize = 2768;
const NO_ZENY: usize = 2770;
const TOO_MUCH: usize = 2780;
const BALANCE_SHORT: usize = 2455;
/// The buttons' tooltips: Deposit, Withdraw, "+ 1z", "- 1z", Max.
const DEPOSIT_TIP: usize = 2774;
const WITHDRAW_TIP: usize = 2775;
const UP_TIP: usize = 2776;
const DOWN_TIP: usize = 2777;
const MAX_TIP: usize = 2778;

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
            buttons: ["deposit", "withdraw", "10mil", "100mil", "1000mil", "max", "upper", "buttom"].map(button),
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
    /// The quick amounts, 3 being Max.
    Quick(usize),
    Deposit,
    Withdraw,
    Window,
}

#[derive(Debug, PartialEq)]
pub enum Action {
    Deposit(i32),
    Withdraw(i32),
    /// The client's own refusal, a message number.
    Refuse(usize),
    Close,
}

pub struct Bank {
    pub frame: Frame,
    /// What the account keeps.
    pub balance: i64,
    pub typed: String,
    pub typing: bool,
}

impl Bank {
    pub fn new() -> Self {
        let mut frame = Frame::new(POSITION, WIDTH);
        frame.open = true;
        Self { frame, balance: 0, typed: String::new(), typing: false }
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let local = self.frame.local(cursor, HEIGHT)?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(Hit::Title(hit));
        }
        let quick = (0..4).find(|&place| contains(offset(QUICK, [place as f32 * (QUICK_SIZE[0] + 2.0), 0.0]), QUICK_SIZE, local));
        let arrow = (0..2).find(|&place| contains(ARROWS[place], ARROW, local));
        Some(match (quick, arrow) {
            (Some(place), _) => Hit::Quick(place),
            (_, Some(place)) => Hit::Arrow(place),
            _ if contains(AMOUNT, AMOUNT_SIZE, local) => Hit::Amount,
            _ if contains(DEPOSIT, BUTTON, local) => Hit::Deposit,
            _ if contains(WITHDRAW, BUTTON, local) => Hit::Withdraw,
            _ => Hit::Window,
        })
    }

    pub fn click(&mut self, hit: Hit, carried: u32) -> Option<Action> {
        self.typing = hit == Hit::Amount;
        let amount = self.typed.parse::<u64>().unwrap_or(0);
        match hit {
            Hit::Title(TitleHit::Close) => return Some(Action::Close),
            Hit::Title(title) => self.frame.click_title(title),
            Hit::Arrow(0) => self.typed = (amount + 1).min(MOST).to_string(),
            Hit::Arrow(_) => self.typed = amount.saturating_sub(1).to_string(),
            Hit::Quick(place) => self.typed = QUICK_AMOUNTS.get(place).map_or(u64::from(carried), |more| (amount + more).min(MOST)).to_string(),
            Hit::Deposit | Hit::Withdraw => return Some(self.check(hit == Hit::Deposit, carried)),
            Hit::Amount | Hit::Window => {}
        }
        None
    }

    /// The client's checks before asking the server.
    fn check(&self, deposit: bool, carried: u32) -> Action {
        let Ok(amount) = self.typed.parse::<u64>() else { return Action::Refuse(NOTHING_TYPED) };
        let refusal = match amount {
            0 => Some(UNDER_ONE),
            amount if amount > MOST => Some(OVER_MOST),
            _ if deposit && carried == 0 => Some(NO_ZENY),
            amount if deposit && amount > u64::from(carried) => Some(TOO_MUCH),
            amount if !deposit && amount as i64 > self.balance => Some(BALANCE_SHORT),
            _ => None,
        };
        match refusal {
            Some(message) => Action::Refuse(message),
            None if deposit => Action::Deposit(amount as i32),
            None => Action::Withdraw(amount as i32),
        }
    }

    pub fn type_text(&mut self, text: &str) {
        let room = 10usize.saturating_sub(self.typed.len());
        self.typed.extend(text.chars().filter(char::is_ascii_digit).take(room));
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
        self.frame.draw_title(layer, chrome, message(TITLE), pointer.0);
        if self.frame.minimized {
            return;
        }
        let hit = self.hit(pointer.0);
        let look = |target: Hit| {
            let hovered = hit == Some(target);
            usize::from(hovered) + usize::from(hovered && pointer.1)
        };
        layer.sprites.push((&assets.background, at([0.0, TITLE_HEIGHT]), assets.background.size));
        for (label, value, place) in [(IN_BANK, self.balance.max(0) as u64, BALANCE), (ON_HAND, u64::from(carried), CARRIED)] {
            layer.texts.push((message(label).to_owned(), at(place), [0, 0, 0]));
            let amount = format!("{} z", group(value));
            layer.texts.push((amount.clone(), at([WIDTH - 12.0 - font.width(&amount), place[1]]), [0, 0, 0]));
        }
        layer.sprites.push((&assets.field, at(AMOUNT), AMOUNT_SIZE));
        let typed = if self.typing && caret { format!("{}|", self.typed) } else { self.typed.clone() };
        layer.texts.push((typed.clone(), at([AMOUNT[0] + AMOUNT_SIZE[0] - 4.0 - font.width(&typed), AMOUNT[1] + 3.0]), [0, 0, 0]));
        for (place, corner) in ARROWS.into_iter().enumerate() {
            layer.sprites.push((&assets.buttons[6 + place][look(Hit::Arrow(place))], at(corner), ARROW));
        }
        for place in 0..4 {
            let corner = offset(QUICK, [place as f32 * (QUICK_SIZE[0] + 2.0), 0.0]);
            layer.sprites.push((&assets.buttons[2 + place][look(Hit::Quick(place))], at(corner), QUICK_SIZE));
        }
        layer.sprites.push((&assets.buttons[0][look(Hit::Deposit)], at(DEPOSIT), BUTTON));
        layer.sprites.push((&assets.buttons[1][look(Hit::Withdraw)], at(WITHDRAW), BUTTON));
        // Each button names itself, 20 px above it.
        let quick = |place: usize| offset(QUICK, [place as f32 * (QUICK_SIZE[0] + 2.0), 0.0]);
        let tip = match self.hit(pointer.0) {
            Some(Hit::Deposit) => Some((message(DEPOSIT_TIP).to_owned(), DEPOSIT)),
            Some(Hit::Withdraw) => Some((message(WITHDRAW_TIP).to_owned(), WITHDRAW)),
            Some(Hit::Arrow(place)) => Some((message(if place == 0 { UP_TIP } else { DOWN_TIP }).to_owned(), ARROWS[place.min(1)])),
            Some(Hit::Quick(place)) => {
                let text = QUICK_AMOUNTS.get(place).map_or_else(|| message(MAX_TIP).to_owned(), |&amount| format!("+ {}", crate::shop::group(amount)));
                Some((text, quick(place)))
            }
            _ => None,
        };
        if let Some((text, corner)) = tip.filter(|(text, _)| !text.is_empty()) {
            crate::window::tooltip(tooltip, chrome, font, &text, at(offset(corner, [0.0, -20.0])), [255; 3]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_amounts_before_asking() {
        let mut bank = Bank::new();
        bank.balance = 1000;
        assert_eq!(bank.click(Hit::Deposit, 5000), Some(Action::Refuse(NOTHING_TYPED)));
        bank.type_text("0");
        assert_eq!(bank.click(Hit::Deposit, 5000), Some(Action::Refuse(UNDER_ONE)));
        bank.typed = "3000000000".into();
        assert_eq!(bank.click(Hit::Withdraw, 5000), Some(Action::Refuse(OVER_MOST)));
        bank.typed = "6000".into();
        assert_eq!(bank.click(Hit::Deposit, 5000), Some(Action::Refuse(TOO_MUCH)));
        assert_eq!(bank.click(Hit::Deposit, 0), Some(Action::Refuse(NO_ZENY)));
        assert_eq!(bank.click(Hit::Withdraw, 5000), Some(Action::Refuse(BALANCE_SHORT)));
        // The quick amounts add to what's typed, the arrows step it; Max is all carried.
        bank.typed.clear();
        bank.click(Hit::Quick(0), 5000);
        bank.click(Hit::Quick(1), 5000);
        assert_eq!(bank.typed, "1100000");
        bank.click(Hit::Quick(3), 5000);
        bank.click(Hit::Arrow(1), 5000);
        assert_eq!(bank.typed, "4999");
        assert_eq!(bank.click(Hit::Deposit, 5000), Some(Action::Deposit(4999)));
        bank.typed = "1000".into();
        assert_eq!(bank.click(Hit::Withdraw, 5000), Some(Action::Withdraw(1000)));
        assert_eq!(bank.click(Hit::Title(TitleHit::Close), 5000), Some(Action::Close));
    }
}
