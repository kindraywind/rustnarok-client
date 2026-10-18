//! How a caster moves when its skill goes off, by skill: the client's per-skill motion table (ZC_NOTIFY_SKILL,
//! ZC_USE_SKILL and the ground packet all use it), and which casts are made standing.

/// The caster's motion for a skill.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Motion {
    /// The attack swing for the weapon.
    Attack,
    /// The first attack action (a throw).
    Throw,
    /// The third attack action (Arrow Vulcan, Musical Strike).
    ThirdAttack,
    PickUp,
    /// Nothing changes (standing, or a combo going on).
    Keep,
    /// The skill pose (the ready stance for some jobs).
    Skill,
}

// ponytail: the dance, song, Taekwon, gun and ninja motions of the table play the skill pose until those actions exist.
const ATTACK: [u16; 105] = [
    5, 7, 42, 46, 47, 56, 57, 58, 61, 62, 109, 212, 214, 219, 253, 266, 267, 338, 366, 367, 368, 370, 372, 379, 382, 397, 398, 399, 406, 484, 499, 502, 503,
    512, 513, 514, 515, 518, 519, 520, 661, 674, 712, 728, 732, 740, 1001, 1005, 1009, 1016, 2002, 2004, 2005, 2006, 2017, 2022, 2023, 2029, 2030, 2031, 2036,
    2037, 2233, 2236, 2259, 2261, 2279, 2280, 2284, 2288, 2298, 2314, 2317, 2320, 2324, 2342, 2476, 5245, 5248, 5249, 5250, 5277, 5283, 5287, 5289, 5294, 5295,
    5296, 5314, 5315, 5316, 5320, 5322, 5344, 5345, 5346, 5348, 5382, 8202, 8601, 8602, 8603, 8604, 8605, 2055,
];
const THROW: [u16; 18] = [59, 149, 152, 229, 230, 231, 232, 250, 251, 480, 1004, 2278, 2480, 2493, 5340, 5341, 5342, 5343];
const THIRD_ATTACK: [u16; 9] = [316, 324, 394, 530, 2258, 2307, 2308, 2312, 5330];
const PICK_UP: [u16; 23] = [115, 116, 117, 118, 119, 120, 121, 122, 123, 124, 125, 688, 1013, 2238, 2239, 2249, 2250, 2251, 2252, 2253, 2254, 2267, 5335];
const KEEP: [u16; 64] = [
    29, 151, 196, 209, 249, 252, 256, 257, 261, 268, 269, 271, 272, 273, 304, 349, 350, 356, 369, 371, 380, 384, 387, 401, 411, 444, 446, 478, 490, 690, 730,
    1015, 2028, 2215, 2218, 2219, 2220, 2221, 2240, 2241, 2242, 2263, 2264, 2265, 2334, 2415, 2416, 2481, 2482, 2484, 2485, 2486, 2490, 2516, 2520, 5219, 5223,
    5224, 5226, 5231, 5236, 5243, 5244, 263,
];
/// Cast without the cast pose (Blessing, the potion and chemical skills, Steel Body, Full Protection…).
const CAST_STANDING: [u16; 20] = [34, 196, 209, 230, 231, 232, 234, 235, 236, 237, 243, 244, 247, 249, 268, 270, 349, 479, 689, 8220];

pub fn motion(skill: u16) -> Motion {
    if ATTACK.contains(&skill) {
        Motion::Attack
    } else if THROW.contains(&skill) {
        Motion::Throw
    } else if THIRD_ATTACK.contains(&skill) {
        Motion::ThirdAttack
    } else if PICK_UP.contains(&skill) {
        Motion::PickUp
    } else if KEEP.contains(&skill) {
        Motion::Keep
    } else {
        Motion::Skill
    }
}

pub fn cast_standing(skill: u16) -> bool {
    CAST_STANDING.contains(&skill)
}

/// Jobs that take the ready stance instead of the skill pose: Crusader, Bard, Dancer, Gunslinger, Taekwon and Star
/// Gladiator lines.
// ponytail: rathena's job ids for those lines and their mounts, transcendent, baby and 3rd forms.
pub fn ready_instead(job: u16) -> bool {
    const JOBS: [u16; 31] = [
        14, 19, 20, 21, 24, 4015, 4020, 4021, 4022, 4037, 4041, 4042, 4044, 4046, 4047, 4048, 4066, 4068, 4069, 4073, 4075, 4076, 4082, 4083, 4102, 4104, 4105,
        4110, 4215, 4239, 4240,
    ];
    JOBS.contains(&job)
}

/// Time between a multi-hit's hits: Pierce's are 280 ms apart, the rest 200.
pub fn hit_gap_ms(skill: u16) -> u64 {
    if skill == 56 { 280 } else { 200 }
}

/// A skill's hit comes this much later than its motion's hit frame: Turn Undead's by 500 ms.
pub fn hit_extra_ms(skill: u16) -> f32 {
    if skill == 77 { 500.0 } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_by_skill() {
        assert_eq!(
            [motion(5), motion(59), motion(394), motion(123), motion(34), motion(19)],
            [Motion::Attack, Motion::Throw, Motion::ThirdAttack, Motion::PickUp, Motion::Skill, Motion::Skill]
        );
        assert_eq!([motion(29), motion(272)], [Motion::Keep, Motion::Keep], "Increase AGI stands, a combo keeps going");
        assert!(cast_standing(34) && !cast_standing(19));
        assert!(ready_instead(14) && ready_instead(4047) && !ready_instead(7));
        assert_eq!((hit_gap_ms(56), hit_gap_ms(5)), (280, 200));
    }
}
