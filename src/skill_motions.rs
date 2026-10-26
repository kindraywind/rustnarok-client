//! How a caster moves when its skill goes off, by skill: the per-skill motion table (ZC_NOTIFY_SKILL, ZC_USE_SKILL and
//! the ground packet all use it), the cast motion table, and the action a player plays for each of the game's motions.

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
    /// Another of the game's motions (a song, a dance, a kick, a shot…): this action, never the job's ready stance.
    Action(usize),
}

const ATTACK: [u16; 105] = [
    5, 7, 42, 46, 47, 56, 57, 58, 61, 62, 109, 212, 214, 219, 253, 266, 267, 338, 366, 367, 368, 370, 372, 379, 382, 397, 398, 399, 406, 484, 499, 502, 503,
    512, 513, 514, 515, 518, 519, 520, 661, 674, 712, 728, 732, 740, 1001, 1005, 1009, 1016, 2002, 2004, 2005, 2006, 2017, 2022, 2023, 2029, 2030, 2031, 2036,
    2037, 2233, 2236, 2259, 2261, 2279, 2280, 2284, 2288, 2298, 2314, 2320, 2324, 2342, 2476, 5245, 5248, 5249, 5250, 5277, 5283, 5287, 5289, 5294, 5295, 5296,
    5314, 5315, 5316, 5320, 5322, 5344, 5345, 5346, 5348, 5382, 8202, 8601, 8602, 8603, 8604, 8605, 2055, 136,
];
const THROW: [u16; 18] = [59, 149, 152, 229, 230, 231, 232, 250, 251, 480, 1004, 2278, 2480, 2493, 5340, 5341, 5342, 5343];
const THIRD_ATTACK: [u16; 9] = [316, 324, 394, 530, 2258, 2307, 2308, 2312, 5330];
const PICK_UP: [u16; 23] = [115, 116, 117, 118, 119, 120, 121, 122, 123, 124, 125, 688, 1013, 2238, 2239, 2249, 2250, 2251, 2252, 2253, 2254, 2267, 5335];
const KEEP: [u16; 73] = [
    29, 151, 196, 209, 249, 252, 256, 257, 261, 268, 269, 271, 272, 273, 304, 349, 350, 356, 369, 371, 380, 384, 387, 401, 411, 444, 446, 478, 490, 690, 730,
    1015, 2028, 2215, 2218, 2219, 2220, 2221, 2240, 2241, 2242, 2263, 2264, 2265, 2334, 2415, 2416, 2481, 2482, 2484, 2485, 2486, 2490, 2516, 2520, 5219, 5223,
    5224, 5226, 5231, 5236, 5243, 5244, 263, 143, 2317, 2326, 2328, 2329, 2579, 8204, 8219, 8220,
];
/// Cast without the cast pose (Blessing, the potion and chemical skills, Steel Body, Full Protection…).
// The game's motions past the swing, throw, third swing, pick up and skill pose, by skill: dances 16, songs 17, readied
// shots 18, Taekwon and Star Gladiator 20–34, ninja 35–37, gunslinger 38–40, Madogear 43–44, Doram 49, …
const STATES: [(u16, u8); 149] = [
    (306, 16),
    (307, 16),
    (308, 16),
    (309, 16),
    (310, 16),
    (311, 16),
    (312, 16),
    (313, 16),
    (317, 17),
    (319, 17),
    (320, 17),
    (321, 17),
    (322, 17),
    (325, 16),
    (327, 16),
    (328, 16),
    (329, 16),
    (330, 16),
    (395, 16),
    (396, 16),
    (412, 21),
    (413, 30),
    (414, 22),
    (416, 23),
    (417, 32),
    (418, 24),
    (419, 26),
    (420, 25),
    (421, 27),
    (426, 20),
    (428, 34),
    (429, 34),
    (430, 34),
    (431, 33),
    (432, 33),
    (433, 33),
    (445, 23),
    (447, 23),
    (448, 23),
    (449, 23),
    (450, 23),
    (451, 23),
    (452, 23),
    (453, 23),
    (454, 23),
    (455, 23),
    (456, 23),
    (457, 23),
    (458, 23),
    (460, 23),
    (461, 23),
    (488, 16),
    (494, 23),
    (500, 39),
    (501, 38),
    (504, 38),
    (505, 38),
    (506, 38),
    (508, 40),
    (516, 38),
    (517, 38),
    (521, 40),
    (523, 36),
    (524, 36),
    (525, 36),
    (526, 36),
    (527, 35),
    (528, 35),
    (529, 35),
    (531, 36),
    (532, 35),
    (534, 36),
    (535, 35),
    (536, 35),
    (537, 35),
    (538, 35),
    (539, 36),
    (540, 36),
    (541, 36),
    (542, 36),
    (543, 36),
    (572, 23),
    (573, 23),
    (574, 23),
    (575, 23),
    (707, 11),
    (1011, 16),
    (2003, 11),
    (2011, 11),
    (2015, 21),
    (2018, 24),
    (2256, 39),
    (2257, 39),
    (2260, 39),
    (2268, 43),
    (2269, 25),
    (2270, 44),
    (2271, 39),
    (2272, 39),
    (2273, 43),
    (2274, 43),
    (2275, 43),
    (2285, 21),
    (2311, 32),
    (2323, 25),
    (2327, 18),
    (2330, 18),
    (2333, 23),
    (2336, 27),
    (2337, 30),
    (2340, 23),
    (2343, 24),
    (2344, 32),
    (2345, 32),
    (2346, 32),
    (2347, 32),
    (2348, 32),
    (2445, 39),
    (2478, 22),
    (2576, 30),
    (2580, 51),
    (2581, 20),
    (2593, 32),
    (2596, 23),
    (2597, 23),
    (2598, 23),
    (2599, 23),
    (3004, 9),
    (3005, 36),
    (3006, 36),
    (3007, 36),
    (3008, 36),
    (3009, 36),
    (3011, 37),
    (3013, 37),
    (3014, 37),
    (3015, 37),
    (3016, 37),
    (3017, 37),
    (3018, 37),
    (3020, 36),
    (3021, 37),
    (3022, 36),
    (3023, 36),
    (3026, 37),
    (3027, 37),
    (3029, 37),
    (5021, 49),
    (5023, 20),
];
/// The cast motion where it isn't the cast pose held: 0 and −1 none (cast standing), 7 the skill pose, 2 the swing, 12 the
/// throw, and the game's other motions.
const CAST: [(u16, i8); 292] = [
    (5, 2),
    (7, 2),
    (29, 7),
    (34, 0),
    (42, 2),
    (46, 2),
    (47, 18),
    (56, 2),
    (57, 2),
    (58, 2),
    (59, 12),
    (61, 11),
    (62, 2),
    (110, 2),
    (115, 5),
    (116, 5),
    (117, 5),
    (118, 5),
    (119, 5),
    (120, 5),
    (121, 5),
    (122, 5),
    (123, 5),
    (124, 5),
    (125, 5),
    (136, 7),
    (149, 11),
    (151, 5),
    (152, 11),
    (196, 0),
    (209, 0),
    (230, 0),
    (231, 0),
    (232, 0),
    (234, 0),
    (235, 0),
    (236, 0),
    (237, 0),
    (243, 0),
    (244, 0),
    (247, 0),
    (249, 0),
    (268, 0),
    (270, 0),
    (349, 0),
    (382, 18),
    (389, 0),
    (394, 18),
    (397, 11),
    (425, 0),
    (426, 19),
    (446, 12),
    (475, 0),
    (478, 28),
    (479, 0),
    (490, 28),
    (493, 0),
    (496, 7),
    (497, 7),
    (498, 7),
    (499, 2),
    (503, 41),
    (504, 41),
    (505, 41),
    (512, 41),
    (513, 41),
    (514, 41),
    (515, 41),
    (517, 41),
    (518, 42),
    (519, 42),
    (520, 42),
    (521, 42),
    (525, 37),
    (532, 37),
    (534, 37),
    (535, 37),
    (536, 37),
    (537, 37),
    (538, 37),
    (539, 37),
    (540, 37),
    (541, 37),
    (542, 37),
    (543, 37),
    (688, 5),
    (689, 0),
    (690, 7),
    (708, 7),
    (720, 7),
    (728, 7),
    (735, -1),
    (736, -1),
    (737, 7),
    (739, -1),
    (742, -1),
    (744, -1),
    (745, -1),
    (1001, 0),
    (1004, 11),
    (1009, 2),
    (1010, 0),
    (1011, 16),
    (1014, 0),
    (2002, 2),
    (2022, 7),
    (2055, 2),
    (2056, 7),
    (2201, 7),
    (2202, 7),
    (2203, 7),
    (2204, 7),
    (2205, 7),
    (2206, 7),
    (2207, 7),
    (2209, 7),
    (2210, 7),
    (2211, 7),
    (2212, 7),
    (2213, 7),
    (2214, 7),
    (2216, 7),
    (2217, 7),
    (2230, 7),
    (2231, 7),
    (2233, 7),
    (2236, 7),
    (2237, 7),
    (2239, 5),
    (2240, 7),
    (2241, -1),
    (2242, -1),
    (2243, 7),
    (2244, 7),
    (2246, 7),
    (2253, 5),
    (2254, 5),
    (2256, -1),
    (2258, -1),
    (2259, 2),
    (2260, -1),
    (2261, 2),
    (2262, -1),
    (2267, -1),
    (2268, -1),
    (2270, -1),
    (2271, -1),
    (2272, -1),
    (2273, -1),
    (2274, -1),
    (2275, -1),
    (2280, 2),
    (2286, 7),
    (2287, 7),
    (2288, 18),
    (2290, 0),
    (2292, 7),
    (2293, 7),
    (2294, 7),
    (2295, 7),
    (2296, 7),
    (2297, 7),
    (2298, 7),
    (2304, 0),
    (2313, 7),
    (2314, 7),
    (2315, 7),
    (2316, 7),
    (2317, 7),
    (2318, 7),
    (2319, 7),
    (2320, 7),
    (2321, 7),
    (2322, 7),
    (2323, 7),
    (2324, 7),
    (2325, 7),
    (2326, -1),
    (2327, -1),
    (2328, -1),
    (2329, -1),
    (2330, -1),
    (2332, -1),
    (2338, -1),
    (2340, -1),
    (2341, -1),
    (2343, -1),
    (2344, -1),
    (2345, -1),
    (2346, -1),
    (2347, -1),
    (2348, -1),
    (2350, 16),
    (2351, 16),
    (2352, 16),
    (2381, 16),
    (2382, 16),
    (2383, 16),
    (2413, 16),
    (2414, 16),
    (2417, 16),
    (2418, 16),
    (2419, 16),
    (2420, 16),
    (2421, 16),
    (2422, 16),
    (2423, 16),
    (2424, 16),
    (2425, 16),
    (2426, 16),
    (2427, 16),
    (2428, 16),
    (2429, 16),
    (2430, 16),
    (2431, 16),
    (2432, 16),
    (2433, 16),
    (2434, 16),
    (2443, -1),
    (2444, -1),
    (2446, -1),
    (2447, -1),
    (2448, -1),
    (2449, -1),
    (2450, -1),
    (2451, -1),
    (2452, -1),
    (2453, -1),
    (2454, 7),
    (2455, -1),
    (2456, -1),
    (2457, -1),
    (2458, -1),
    (2459, -1),
    (2460, -1),
    (2476, -1),
    (2477, -1),
    (2478, -1),
    (2479, 7),
    (2480, -1),
    (2481, 12),
    (2482, 7),
    (2483, 7),
    (2485, 12),
    (2486, 7),
    (2490, 7),
    (2492, -1),
    (2494, -1),
    (2495, -1),
    (2496, -1),
    (2497, -1),
    (2517, -1),
    (2518, -1),
    (2520, -1),
    (2581, 19),
    (3006, 36),
    (3009, 36),
    (3011, 37),
    (3013, 37),
    (3014, 37),
    (3015, 37),
    (3016, 37),
    (3017, 37),
    (3018, 37),
    (3021, 37),
    (3026, 37),
    (3027, 37),
    (3029, 37),
    (5023, 19),
    (5026, 50),
    (5027, 50),
    (5032, 50),
    (5033, 50),
    (5035, 50),
    (5036, 50),
    (5038, 50),
    (5040, 50),
    (5043, 50),
    (5078, 7),
    (5321, -1),
    (8202, 2),
    (8220, 0),
    (8401, 7),
    (8425, 7),
    (8426, 7),
    (8428, 7),
    (8430, 7),
    (8431, 7),
    (8433, 7),
    (8434, 7),
    (8435, 7),
    (8442, 7),
];

pub fn motion(skill: u16) -> Motion {
    if let Some(action) = STATES.iter().find(|&&(id, _)| id == skill).and_then(|&(_, state)| state_action(state)) {
        Motion::Action(action)
    } else if ATTACK.contains(&skill) {
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

/// A player's motion while casting the skill: None for the cast pose held (most skills), Keep for none (Blessing, the
/// potions, most Sura, Mechanic, Sorcerer and Genetic skills…), else the motion played once.
pub fn cast_motion(skill: u16) -> Option<Motion> {
    let state = CAST.iter().find(|&&(id, _)| id == skill)?.1;
    Some(match state {
        0 | -1 => Motion::Keep,
        2 => Motion::Attack,
        7 => Motion::Skill,
        12 => Motion::Throw,
        state => Motion::Action(state_action(u8::try_from(state).ok()?)?),
    })
}

/// The action a player plays for one of the game's motions (actor states): None for those the map leaves out.
// ponytail: state 26 (Counter Kick) is the third swing for a few jobs (Soul Linker, Soul Reaper…); the first for all here.
pub fn state_action(state: u8) -> Option<usize> {
    match state {
        5 | 20 | 25 => Some(3),
        11 => Some(4),
        19 | 26 | 28 | 40 | 42 | 50 | 51 => Some(5),
        43 | 44 => Some(10),
        9 | 18 | 32 | 49 => Some(11),
        16 | 17 | 21..=24 | 27 | 30 | 33..=39 | 41 => Some(12),
        _ => None,
    }
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

/// Whether a cast moves its caster (turn, cast pose, cast effects): not without cast time (Bash, Double Strafe), where
/// only the name shows and the skill's own packet moves it; Asura Strike's always does (in a combo it has none).
pub fn cast_moves(cast_ms: u32, skill: u16) -> bool {
    cast_ms > 0 || skill == ASURA_STRIKE
}

/// Asura Strike (MO_EXTREMITYFIST).
pub const ASURA_STRIKE: u16 = 271;

/// Whether a player's skill packet turns it to the target: the swing (motion `Attack`) only as it starts one, not
/// mid-swing (Bowling Bash's later packets, Arrow Shower's after its ground swing); other motions always.
pub fn turns(motion: Motion, mid_swing: bool) -> bool {
    motion != Motion::Attack || !mid_swing
}

/// Sonic Blow: the caster swings once a hit.
// ponytail: the game's table gives one skill pose; the swing a hit (hits 200 ms apart) is how it plays for the project's lead.
pub const SONIC_BLOW: u16 = 136;

/// Meteor Storm: each meteor's ground packet leaves its caster unturned.
pub const METEOR_STORM: u16 = 83;

/// Meteor Storm and the skills like it (NPC and Doram meteors, the Arch Mage's falling skills): each ground packet
/// shows the caster on the third swing's last frame, unturned, with no motion.
pub fn meteor_like(skill: u16) -> bool {
    [METEOR_STORM, 769, 774, 5028, 5042, 5219, 5223, 5224, 5231, 5236].contains(&skill)
}

/// Time between the hits of damage from a skill's ground unit (Storm Gust's, say), from the packet on.
pub const UNIT_HIT_GAP_MS: u64 = 200;

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
        assert_eq!(motion(SONIC_BLOW), Motion::Attack, "Sonic Blow swings");
        assert_eq!([motion(29), motion(272)], [Motion::Keep, Motion::Keep], "Increase AGI stands, a combo keeps going");
        assert_eq!((cast_motion(34), cast_motion(19)), (Some(Motion::Keep), None), "Blessing cast standing, Fire Bolt in the cast pose");
        assert!(ready_instead(14) && ready_instead(4047) && !ready_instead(7));
        assert_eq!((hit_gap_ms(56), hit_gap_ms(5)), (280, 200));
    }

    #[test]
    fn casts_and_turns_as_the_game() {
        // Bash and Double Strafe come with cast time 0: no motion from the cast; Holy Light's second does.
        assert!(!cast_moves(0, 5) && cast_moves(1000, 5));
        // Asura Strike's cast moves its caster even at once (in a combo).
        assert!(cast_moves(0, ASURA_STRIKE));
        // Bash and Double Strafe turn as their swing starts, not mid-swing (Bowling Bash's later packets).
        assert!(turns(motion(5), false) && turns(motion(46), false) && !turns(motion(62), true));
        // The skill pose always turns (Holy Light), mid-motion or not.
        assert!(turns(motion(156), true));
    }

    #[test]
    fn the_games_other_motions_play_their_own_actions() {
        // Lullaby (a dance) and Assassin Cross of Sunset (a song): the skill pose's action, for a Bard or Dancer too.
        assert_eq!((motion(306), motion(320)), (Motion::Action(12), Motion::Action(12)));
        // Taekwon: Turn Kick the third swing, Tumbling the pick-up, Storm Kick the skill pose.
        assert_eq!((motion(417), motion(420), motion(413)), (Motion::Action(11), Motion::Action(3), Motion::Action(12)));
        // Gunslinger: Cracker the first swing; Madogear's Shape Shift the second; Sky Net Blow the third.
        assert_eq!((motion(508), motion(2268), motion(2327)), (Motion::Action(5), Motion::Action(10), Motion::Action(11)));
        // Overbrand and Dragon Combo keep the motion going at their damage.
        assert_eq!((motion(2317), motion(2326)), (Motion::Keep, Motion::Keep));
        // Casting: Sharp Shooting draws the bow (the third swing), Acid Demonstration throws, Sura skills stand.
        assert_eq!(cast_motion(382), Some(Motion::Action(11)));
        assert_eq!((cast_motion(490), cast_motion(2330)), (Some(Motion::Action(5)), Some(Motion::Keep)));
        assert_eq!(cast_motion(2201), Some(Motion::Skill), "Warlock skills: the skill pose, played once");
        assert!(meteor_like(METEOR_STORM) && !meteor_like(19));
        // Every table entry has an action.
        assert!(STATES.iter().all(|&(_, state)| state_action(state).is_some()));
        assert!(CAST.iter().all(|&(skill, _)| cast_motion(skill).is_some()));
    }
}
