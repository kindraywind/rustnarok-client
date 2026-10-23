//! How far a driven client may go beyond a player (#1095). Every MCP tool and `state` field has a grade, and
//! `RUSTNAROK_CLIENT_PLAY` (fair, assist or all, all when unset) is the highest grade a session may use.

use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Grade {
    /// What a player has: the same input, and what the screen shows.
    Fair,
    /// Nothing a player couldn't see or do, handed over in an easier form, or making up for a mouse driven over a
    /// socket (the round trip, no eyes on a moving sprite).
    Assist,
    /// Beyond a player: for tests and debugging.
    Test,
}

impl Grade {
    pub fn parse(mode: &str) -> Option<Self> {
        match mode {
            "fair" => Some(Self::Fair),
            "assist" => Some(Self::Assist),
            "all" => Some(Self::Test),
            _ => None,
        }
    }

    /// The play mode that allows up to this grade.
    pub fn mode(self) -> &'static str {
        ["fair", "assist", "all"][self as usize]
    }

    /// The tag a tool's description starts with.
    pub fn tag(self) -> &'static str {
        ["[fair]", "[assist]", "[test]"][self as usize]
    }
}

/// The session's play mode, from `RUSTNAROK_CLIENT_PLAY`: everything when unset or unknown.
pub fn mode() -> Grade {
    static MODE: OnceLock<Grade> = OnceLock::new();
    *MODE.get_or_init(|| std::env::var("RUSTNAROK_CLIENT_PLAY").ok().and_then(|mode| Grade::parse(&mode)).unwrap_or(Grade::Test))
}
