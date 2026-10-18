//! Battlegrounds: which camp each player fights for, the score, teammates' places on the minimap, and the team's
//! battle chat. rathena sends it all again on arriving on a map, so a map change forgets it.

use std::collections::HashMap;

/// rathena's map type for a battleground (`MAPTYPE_BATTLEFIELD`).
pub const MAP_TYPE: u16 = 19;
/// "Battlefield Chat has been activated." / "... deactivated."
pub const CHAT_ON: usize = 1365;
pub const CHAT_OFF: usize = 1366;

// ponytail: nothing says how camps look; ours blue, the other red, the battle chat in orange.
pub const OUR_COLOR: [u8; 3] = [0x30, 0x90, 0xFF];
pub const THEIR_COLOR: [u8; 3] = [0xFF, 0x40, 0x30];
pub const CHAT_COLOR: [u8; 3] = [0xFF, 0xB0, 0x40];

#[derive(Default)]
pub struct Battle {
    /// Each player's camp (rathena's team ID), by account, the player's own included.
    pub camps: HashMap<u32, u16>,
    /// Camp A's points and camp B's.
    pub score: Option<[u16; 2]>,
    /// Teammates' cells for the minimap, by account.
    pub dots: HashMap<u32, [u16; 2]>,
    /// Typed lines go to the team.
    pub chat: bool,
}

impl Battle {
    pub fn camp(&self, me: u32) -> Option<u16> {
        self.camps.get(&me).copied()
    }

    /// Teammates (our camp, not us), once our camp is known.
    pub fn teammates(&self, me: u32) -> Vec<u32> {
        let Some(ours) = self.camp(me) else { return Vec::new() };
        let mut team: Vec<u32> = self.camps.iter().filter(|&(&account, &camp)| account != me && camp == ours).map(|(&account, _)| account).collect();
        team.sort_unstable();
        team
    }

    /// ZC_BATTLEFIELD_NOTIFY_POSITION: a teammate's cell, or none when they left.
    pub fn place(&mut self, account: u32, cell: Option<[u16; 2]>) {
        match cell {
            Some(cell) => {
                self.dots.insert(account, cell);
            }
            None => {
                self.dots.remove(&account);
            }
        }
    }

    /// /battlechat: on or off; the line it shows.
    pub fn toggle_chat(&mut self) -> usize {
        self.chat = !self.chat;
        if self.chat { CHAT_ON } else { CHAT_OFF }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_camps_dots_and_chat() {
        let mut battle = Battle::default();
        battle.camps.extend([(1, 7), (2, 7), (3, 8)]);
        assert_eq!(battle.camp(1), Some(7));
        assert_eq!(battle.teammates(1), [2]);
        assert!(battle.teammates(4).is_empty(), "our own camp not known yet");
        battle.place(2, Some([62, 119]));
        battle.place(2, Some([63, 119]));
        assert_eq!(battle.dots.get(&2), Some(&[63, 119]));
        battle.place(2, None);
        assert!(battle.dots.is_empty());
        assert_eq!(battle.toggle_chat(), CHAT_ON);
        assert_eq!(battle.toggle_chat(), CHAT_OFF);
    }
}
