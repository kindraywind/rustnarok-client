//! Which window is in front. The game keeps its windows in one list: a window that opens goes on top, and so does one
//! that's pressed. Message boxes, the menus, the tooltips and the world map stay over every window; the screen's own
//! things (the small minimap, the quest icon, the level-up buttons) stay under them.

use crate::Top;
use crate::escape::WindowId;

/// Where something drawn on the screen sits.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum Place {
    /// Under every window.
    #[default]
    Under,
    /// A window, in `Front`'s order. One key for all of a window's parts (the mail's three, every dragged-out member).
    Window(Top),
    /// Over every window.
    Over,
}

/// Where `top` sits.
pub fn place(top: Top) -> Place {
    use WindowId::*;
    match top {
        Top::Slot | Top::GuildReason | Top::TradeAsked | Top::BookingAsked | Top::SpecialPopup => Place::Over,
        Top::Esc(
            WorldMap | Menu | Notice | PartyAsked | SaveAsked | MemorialAsked | SkillApplyAsked | Amount | PartyInvite | AdoptAsked | QueueAsked | FriendAsked
            | DismissAsked | HomunculusAsked | LeaderAsked | BindAsked | ExpansionAsked | PetAsked | GuildInvite | GuildAsked | CashShopBuy,
        ) => Place::Over,
        Top::SmallMinimap | Top::QuestIcon | Top::LevelUp | Top::RouletteIcon => Place::Under,
        Top::Esc(MailWrite | MailRead) => Place::Window(Top::Esc(Mailbox)),
        Top::Esc(PartyCreate | PartyInviteName) => Place::Window(Top::Esc(PartySetup)),
        Top::PartyHp(_) => Place::Window(Top::PartyHp(0)),
        window => Place::Window(window),
    }
}

/// The windows from the bottom up.
#[derive(Debug, Default)]
pub struct Front(Vec<Top>);

impl Front {
    /// Drops the windows that closed and puts the ones newly shown on top, in the order they're drawn.
    pub fn sync(&mut self, shown: &[Top]) {
        self.0.retain(|key| shown.contains(key));
        for key in shown {
            if !self.0.contains(key) {
                self.0.push(*key);
            }
        }
    }

    /// Puts the window `top` is part of on top (anything else stays where it is).
    pub fn raise(&mut self, top: Top) {
        if let Place::Window(key) = place(top) {
            self.0.retain(|&other| other != key);
            self.0.push(key);
        }
    }

    /// How high the window is: higher is nearer the top, -1 for one not listed yet.
    pub fn rank(&self, key: Top) -> isize {
        self.0.iter().position(|&other| other == key).map_or(-1, |at| at as isize)
    }

    /// Where a press lands, among the things under it listed top first as they're drawn by default: the first that's
    /// over the windows, else the highest window (the first listed of equals), else the first under them.
    pub fn pick(&self, hits: impl IntoIterator<Item = Top>) -> Option<Top> {
        let (mut window, mut under) = (None::<(isize, Top)>, None);
        for hit in hits {
            match place(hit) {
                Place::Over => return Some(hit),
                Place::Window(key) => {
                    let rank = self.rank(key);
                    if window.is_none_or(|(best, _)| rank > best) {
                        window = Some((rank, hit));
                    }
                }
                Place::Under => {
                    under.get_or_insert(hit);
                }
            }
        }
        window.map(|(_, hit)| hit).or(under)
    }

    /// The order to draw pieces in: under first, then the windows bottom up, then what's over them; equals keep their
    /// order.
    pub fn draw_key(&self, place: Place) -> (u8, isize) {
        match place {
            Place::Under => (0, 0),
            Place::Window(key) => (1, self.rank(key)),
            Place::Over => (2, 0),
        }
    }

    /// The windows from the bottom up.
    pub fn windows(&self) -> &[Top] {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Panel;

    const INVENTORY: Top = Top::Panel(Panel::Inventory);
    const MAIL: Top = Top::Esc(WindowId::Mailbox);
    const BANK: Top = Top::Esc(WindowId::Bank);

    #[test]
    fn a_window_that_opens_or_is_pressed_goes_on_top() {
        let mut front = Front::default();
        front.sync(&[INVENTORY, MAIL]);
        assert_eq!(front.windows(), [INVENTORY, MAIL]);
        // The bank opens: on top; the inventory is pressed: on top of it.
        front.sync(&[INVENTORY, MAIL, BANK]);
        front.raise(INVENTORY);
        assert_eq!(front.windows(), [MAIL, BANK, INVENTORY]);
        // A part of the mail raises the whole mail; a message box raises nothing.
        front.raise(Top::Esc(WindowId::MailWrite));
        front.raise(Top::Esc(WindowId::SaveAsked));
        assert_eq!(front.windows(), [BANK, INVENTORY, MAIL]);
        // The bank closes and opens again: on top again.
        front.sync(&[INVENTORY, MAIL]);
        front.sync(&[INVENTORY, MAIL, BANK]);
        assert_eq!(front.windows(), [INVENTORY, MAIL, BANK]);
    }

    #[test]
    fn the_pickup_popup_is_a_window_in_the_order() {
        const HOTBAR: Top = Top::Panel(Panel::Hotkeys);
        let mut front = Front::default();
        front.sync(&[HOTBAR, INVENTORY]);
        // It shows: on top of the hotbar and the rest. A press on the hotbar after that puts the hotbar over it.
        front.sync(&[Top::Pickup, HOTBAR, INVENTORY]);
        assert_eq!(front.windows(), [HOTBAR, INVENTORY, Top::Pickup]);
        front.raise(HOTBAR);
        assert_eq!(front.windows(), [INVENTORY, Top::Pickup, HOTBAR]);
        // Its five seconds over, the next one is on top again.
        front.sync(&[HOTBAR, INVENTORY]);
        front.sync(&[Top::Pickup, HOTBAR, INVENTORY]);
        assert_eq!(front.windows(), [INVENTORY, HOTBAR, Top::Pickup]);
    }

    #[test]
    fn a_press_goes_to_the_highest_window_under_message_boxes() {
        let mut front = Front::default();
        front.sync(&[INVENTORY, MAIL]);
        // Listed as the default drawing has them (the mail over the inventory), but the inventory was raised.
        front.raise(INVENTORY);
        assert_eq!(front.pick([MAIL, INVENTORY, Top::SmallMinimap]), Some(INVENTORY));
        // A message box over both takes it; with no window there, what's under the windows does.
        assert_eq!(front.pick([MAIL, Top::Esc(WindowId::SaveAsked), INVENTORY]), Some(Top::Esc(WindowId::SaveAsked)));
        assert_eq!(front.pick([Top::SmallMinimap, Top::LevelUp]), Some(Top::SmallMinimap));
        // Windows not listed yet keep the default order: the first listed.
        assert_eq!(Front::default().pick([BANK, MAIL]), Some(BANK));
        // Member windows share one key, and keep their own account in the press.
        front.sync(&[INVENTORY, MAIL, Top::PartyHp(0)]);
        assert_eq!(front.pick([INVENTORY, Top::PartyHp(7)]), Some(Top::PartyHp(7)));
    }

    #[test]
    fn draws_under_then_windows_bottom_up_then_over() {
        let mut front = Front::default();
        front.sync(&[MAIL, INVENTORY]);
        let mut pieces = [Place::Over, Place::Window(INVENTORY), Place::Under, Place::Window(MAIL)];
        pieces.sort_by_key(|&place| front.draw_key(place));
        assert_eq!(pieces, [Place::Under, Place::Window(MAIL), Place::Window(INVENTORY), Place::Over]);
    }
}
