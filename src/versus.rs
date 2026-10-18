//! Fighting other players on PvP, GvG and War of Emperium maps: who the attack cursor shows over, and when a click on
//! a player attacks. The map's flags (ZC_MAPPROPERTY_R2) decide; rathena refuses an attack the rules don't allow, so
//! these only shape what the client offers.

use rustnarok_client::net::{MAP_BATTLEFIELD, MAP_GVG, MAP_NO_LOCK_ON, MAP_PVP, MAP_SIEGE};

/// Where the player stands and with whom: the map's flags, the online members of our party (accounts), our guild
/// (0: none) and its allies.
pub struct Side {
    pub flags: u32,
    pub party: Vec<u32>,
    pub guild: u32,
    pub allies: Vec<u32>,
}

/// Another player as these rules need them.
pub struct Other {
    pub account: u32,
    pub guild: u32,
    pub dead: bool,
}

/// A siege map: guild emblems over heads, guildmates and allies spared (battlegrounds go by camp, not here).
pub fn siege(flags: u32) -> bool {
    flags & MAP_SIEGE != 0 && flags & MAP_BATTLEFIELD == 0
}

impl Side {
    fn party_member(&self, account: u32) -> bool {
        self.party.contains(&account)
    }

    fn friend(&self, guild: u32) -> bool {
        guild != 0 && (guild == self.guild || self.allies.contains(&guild))
    }

    /// Whether another player counts as an enemy, as the attack cursor shows. `wants`: Shift held, or its opposite while
    /// aiming a skill for players (`Game::players_first`); `aiming`: a skill waits for a target.
    pub fn enemy(&self, other: &Other, wants: bool, aiming: bool) -> bool {
        let mut enemy = wants;
        // The party flag first, else the guild flag.
        if self.flags & MAP_PVP != 0 {
            enemy |= !self.party_member(other.account);
        } else if self.flags & MAP_GVG != 0 {
            enemy |= self.guild == 0 || other.guild != self.guild;
        }
        if siege(self.flags) {
            // Guildmates and allies never, whatever came before; a skill for players still reaches them by Shift.
            enemy = !self.friend(other.guild) || (aiming && wants);
        } else if self.flags & MAP_NO_LOCK_ON != 0 {
            enemy |= !self.party_member(other.account);
        }
        enemy
    }

    /// The attack cursor over another player: an enemy who isn't dead.
    pub fn attack_cursor(&self, other: &Other, wants: bool, aiming: bool) -> bool {
        self.enemy(other, wants, aiming) && !other.dead
    }
}

// ponytail: someone's homunculus, mercenary or elemental doesn't say whose it is, so on a siege map (where a
// guildmate's or ally's is spared) it's taken as an owner out of sight: no attack cursor. Elsewhere it's an enemy
// wherever bit 4 is set.
/// The attack cursor over another player's companion (not a pet).
pub fn companion_cursor(flags: u32) -> bool {
    !siege(flags) && flags & MAP_NO_LOCK_ON != 0
}

/// How a left click on another player goes.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Click {
    /// Walk there, as in a town.
    Walk,
    /// Attack, walking into reach first, and keep at it.
    Attack,
    /// One swing (siege maps): each needs its own click.
    Swing,
}

/// A left click on another player (never ourselves): it attacks with Shift (or its opposite while aiming at players),
/// with /noshift on, or where bit 4 says players fight; else it walks.
pub fn click(flags: u32, wants: bool, no_shift: bool, me: u32, account: u32) -> Click {
    if account == me || !(wants || no_shift || flags & MAP_NO_LOCK_ON != 0) {
        Click::Walk
    } else if siege(flags) {
        Click::Swing
    } else {
        Click::Attack
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // rathena's stock maps: prontera, pvp_y_1-2, guild_vs1, a castle outside and during WoE.
    const TOWN: u32 = 0x0600;
    const PVP: u32 = 0x0731;
    const GVG: u32 = 0x061E;
    const CASTLE: u32 = 0x060E;
    const WOE: u32 = 0x061E;

    fn other(account: u32, guild: u32) -> Other {
        Other { account, guild, dead: false }
    }

    #[test]
    fn finds_enemies_by_the_maps_rules() {
        let side = |flags| Side { flags, party: vec![7], guild: 10, allies: vec![30] };
        let stranger = other(5, 20);
        let party_member = other(7, 20);
        let guildmate = other(8, 10);
        let ally = other(9, 30);
        let none = other(6, 0);
        // A town: no one, unless Shift.
        assert!(!side(TOWN).attack_cursor(&stranger, false, false));
        assert!(side(TOWN).attack_cursor(&stranger, true, false));
        // PvP: everyone outside the party, guildmates too.
        assert!(side(PVP).attack_cursor(&stranger, false, false));
        assert!(side(PVP).attack_cursor(&guildmate, false, false));
        assert!(!side(PVP).attack_cursor(&party_member, false, false));
        // GvG (guild_vs1 is a siege map too): outside our guild and allies.
        assert!(side(GVG).attack_cursor(&stranger, false, false));
        assert!(side(GVG).attack_cursor(&none, false, false));
        assert!(!side(GVG).attack_cursor(&guildmate, false, false));
        assert!(!side(GVG).attack_cursor(&ally, false, false));
        assert!(!side(GVG).attack_cursor(&guildmate, true, false), "Shift doesn't make a guildmate an enemy");
        assert!(side(GVG).attack_cursor(&guildmate, true, true), "a skill aimed at players still reaches them");
        // Without a guild, everyone on a GvG map is outside ours.
        let alone = Side { flags: GVG, party: Vec::new(), guild: 0, allies: Vec::new() };
        assert!(alone.attack_cursor(&none, false, false));
        // A castle outside WoE shows the cursor on other guilds (the click won't attack); during WoE the same.
        assert!(side(CASTLE).attack_cursor(&stranger, false, false));
        assert!(side(WOE).attack_cursor(&stranger, false, false));
        // The dead never.
        assert!(!side(PVP).attack_cursor(&Other { dead: true, ..other(5, 20) }, false, false));
    }

    #[test]
    fn clicks_attack_where_players_fight() {
        assert_eq!(click(TOWN, false, false, 1, 2), Click::Walk);
        assert_eq!(click(TOWN, true, false, 1, 2), Click::Attack, "Shift");
        assert_eq!(click(TOWN, false, true, 1, 2), Click::Attack, "/noshift");
        assert_eq!(click(PVP, false, false, 1, 2), Click::Attack);
        assert_eq!(click(CASTLE, false, false, 1, 2), Click::Walk, "a castle outside WoE");
        assert_eq!(click(WOE | MAP_NO_LOCK_ON, false, false, 1, 2), Click::Swing, "a castle during WoE: one swing a click");
        assert_eq!(click(PVP, true, true, 1, 1), Click::Walk, "never ourselves");
        assert!(companion_cursor(PVP) && !companion_cursor(TOWN) && !companion_cursor(GVG | MAP_NO_LOCK_ON));
    }
}
