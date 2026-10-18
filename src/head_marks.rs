//! Marks over heads: where emotions and status sprites hang, and which status sprites a unit's states show.

/// How far over the feet emotions and status marks hang, in world units (straight up); the curse mark hangs lower.
pub const ANCHOR: f32 = 20.0;
pub const CURSE_ANCHOR: f32 = 15.0;

/// rathena's OPT1_* body states (status.hpp) that have a mark.
const OPT1_FREEZE: u16 = 2;
const OPT1_STUN: u16 = 3;
const OPT1_SLEEP: u16 = 4;
/// rathena's OPT2_* health bits (status.hpp) that have a mark.
const OPT2_CURSE: u16 = 0x0002;
const OPT2_SILENCE: u16 = 0x0004;

/// The "..." emotion (rathena's ET_THINK) stands for silence.
pub const SILENCE_EMOTION: u8 = 9;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    Stun,
    Sleep,
    Curse,
    Silence,
    /// The block of ice a frozen unit sits in.
    Freeze,
}

impl Mark {
    /// The sprite under data\sprite\이팩트\, or None for silence (the emotion sprite's "...").
    pub fn sprite(self) -> Option<&'static str> {
        match self {
            Mark::Stun => Some("status-stun"),
            Mark::Sleep => Some("status-sleep"),
            Mark::Curse => Some("status-curse"),
            Mark::Silence => None,
            Mark::Freeze => Some("얼음땡"),
        }
    }

    /// How far over the feet it hangs, in world units: the ice block sits at the feet.
    pub fn height(self) -> f32 {
        match self {
            Mark::Curse => CURSE_ANCHOR,
            Mark::Freeze => 0.0,
            _ => ANCHOR,
        }
    }
}

/// The marks for a unit's body state and health bits, the body's first.
pub fn marks(body: u16, health: u16) -> Vec<Mark> {
    let mut marks = Vec::new();
    match body {
        OPT1_STUN => marks.push(Mark::Stun),
        OPT1_SLEEP => marks.push(Mark::Sleep),
        OPT1_FREEZE => marks.push(Mark::Freeze),
        _ => {}
    }
    if health & OPT2_CURSE != 0 {
        marks.push(Mark::Curse);
    }
    if health & OPT2_SILENCE != 0 {
        marks.push(Mark::Silence);
    }
    marks
}

/// The world point `height` units straight over `feet`.
pub fn anchor(feet: [f32; 3], height: f32) -> [f32; 3] {
    [feet[0], feet[1] + height, feet[2]]
}

/// The emotion sprite's action for an NPC's quest effect (ZC_QUEST_NOTIFY_EFFECT): 0–5 are actions 80–85, 6–10 are
/// 93–97; any other effect has no head mark.
pub fn quest_action(effect: u16) -> Option<usize> {
    match effect {
        0..=5 => Some(80 + usize::from(effect)),
        6..=10 => Some(93 + usize::from(effect - 6)),
        _ => None,
    }
}

/// Where a quest mark hangs: where emotions do, 3 units toward world −x, so its offset on screen turns with the camera.
pub fn quest_anchor(feet: [f32; 3]) -> [f32; 3] {
    let [x, y, z] = anchor(feet, ANCHOR);
    [x - 3.0, y, z]
}

/// Whether a sprite played once over a head (an emotion, the MVP sprite) still shows `elapsed` after it started,
/// lasting its action's `length_ms`.
pub fn playing(elapsed: std::time::Duration, length_ms: f32) -> bool {
    elapsed.as_secs_f32() * 1000.0 < length_ms
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_quest_marks_action() {
        assert_eq!((0..=5).map(quest_action).collect::<Vec<_>>(), (80..=85).map(Some).collect::<Vec<_>>());
        assert_eq!((6..=10).map(quest_action).collect::<Vec<_>>(), (93..=97).map(Some).collect::<Vec<_>>());
        assert_eq!([11, 9999].map(quest_action), [None, None]);
        assert_eq!(quest_anchor([10.0, 1.0, 5.0]), [7.0, 21.0, 5.0]);
    }

    #[test]
    fn hangs_marks_over_the_feet() {
        assert_eq!(anchor([10.0, 2.0, 30.0], ANCHOR), [10.0, 22.0, 30.0]);
        assert_eq!([Mark::Stun.height(), Mark::Sleep.height(), Mark::Silence.height()], [20.0; 3]);
        assert_eq!((Mark::Curse.height(), Mark::Freeze.height()), (15.0, 0.0));
    }

    #[test]
    fn plays_sprites_over_heads_once() {
        // The MVP sprite hangs 20 units over the winner's feet, for its action's frames × delay × 24 ms.
        assert_eq!(anchor([5.0, 1.0, 7.0], ANCHOR), [5.0, 21.0, 7.0]);
        let length = 10.0 * 4.0 * 24.0;
        assert!(playing(std::time::Duration::from_millis(959), length));
        assert!(!playing(std::time::Duration::from_millis(960), length));
    }

    #[test]
    fn picks_a_mark_per_state() {
        assert_eq!(marks(0, 0), []);
        assert_eq!(marks(3, 0), [Mark::Stun]);
        assert_eq!(marks(4, 0), [Mark::Sleep]);
        assert_eq!(marks(2, 0), [Mark::Freeze]);
        // Stone, petrifying and burning have no mark (their colour is the tint's).
        assert_eq!([marks(1, 0), marks(6, 0), marks(7, 0)], [vec![], vec![], vec![]]);
        assert_eq!(marks(0, 0x0002), [Mark::Curse]);
        assert_eq!(marks(0, 0x0004), [Mark::Silence]);
        assert_eq!(marks(3, 0x0006 | 0x0001), [Mark::Stun, Mark::Curse, Mark::Silence]);
        assert_eq!(
            [Mark::Stun, Mark::Sleep, Mark::Curse, Mark::Silence, Mark::Freeze].map(Mark::sprite),
            [Some("status-stun"), Some("status-sleep"), Some("status-curse"), None, Some("얼음땡")]
        );
    }
}
