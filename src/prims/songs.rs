//! Song and alchemy effects built in code a 16 ms tick at a time: Twilight Alchemy's 320 drifting icons (573 to 575), Shrink's
//! yellow flashes (599), Close Confine's red blinking and shaking (602), and the performers' four cones of red bubbles (840).

use std::time::Instant;

use super::passes;
use super::sage::{ring_point, ring_quads};
use super::walls::Wall;
use crate::world::{MeshVertex, Vector};

pub const HERB: &str = "data\\texture\\유저인터페이스\\item\\하얀허브.bmp";
pub const WHITE_SLIM_POTION: &str = "data\\texture\\유저인터페이스\\item\\화이트슬림포션.bmp";
/// The three pictures of Twilight Alchemy 3: a fire bottle, an acid bottle and alcohol.
pub const MIXED: [&str; 3] =
    ["data\\texture\\유저인터페이스\\item\\화염병.bmp", "data\\texture\\유저인터페이스\\item\\염산병.bmp", "data\\texture\\유저인터페이스\\item\\알코올.bmp"];
/// The cone's bubble pictures, W_bubble01 to W_bubble27.
pub const BUBBLES: [&str; 27] = [
    "data\\texture\\effect\\W_bubble01.tga",
    "data\\texture\\effect\\W_bubble02.tga",
    "data\\texture\\effect\\W_bubble03.tga",
    "data\\texture\\effect\\W_bubble04.tga",
    "data\\texture\\effect\\W_bubble05.tga",
    "data\\texture\\effect\\W_bubble06.tga",
    "data\\texture\\effect\\W_bubble07.tga",
    "data\\texture\\effect\\W_bubble08.tga",
    "data\\texture\\effect\\W_bubble09.tga",
    "data\\texture\\effect\\W_bubble10.tga",
    "data\\texture\\effect\\W_bubble11.tga",
    "data\\texture\\effect\\W_bubble12.tga",
    "data\\texture\\effect\\W_bubble13.tga",
    "data\\texture\\effect\\W_bubble14.tga",
    "data\\texture\\effect\\W_bubble15.tga",
    "data\\texture\\effect\\W_bubble16.tga",
    "data\\texture\\effect\\W_bubble17.tga",
    "data\\texture\\effect\\W_bubble18.tga",
    "data\\texture\\effect\\W_bubble19.tga",
    "data\\texture\\effect\\W_bubble20.tga",
    "data\\texture\\effect\\W_bubble21.tga",
    "data\\texture\\effect\\W_bubble22.tga",
    "data\\texture\\effect\\W_bubble23.tga",
    "data\\texture\\effect\\W_bubble24.tga",
    "data\\texture\\effect\\W_bubble25.tga",
    "data\\texture\\effect\\W_bubble26.tga",
    "data\\texture\\effect\\W_bubble27.tga",
];

/// 80 prims of 4 icons.
const ICONS: usize = 320;
/// The cone's life: drawn on ticks 0 to 118.
const CONE_TICKS: u32 = 119;
/// Shrink's flashes and the sound at its tick 5; Close Confine's from tick 21 to 59.
const SHRINK_TICKS: u32 = 200;
const CONFINE_TICKS: u32 = 60;

struct Icon {
    picture: &'static str,
    spin: f32,
    /// Where it starts, from the player, and how high above the ground.
    offset: [f32; 2],
    rise: f32,
    /// The two drift angles and where each is heading (degrees).
    drift: [(f32, f32); 2],
    size: f32,
    wobble: f32,
    /// Where it is now (once its cloud has the player's place).
    at: Vector,
}

/// Twilight Alchemy's cloud round the own player.
struct Cloud {
    length: u32,
    start: Instant,
    ticks: u32,
    settled: bool,
    icons: Vec<Icon>,
}

/// A body flashing or blinking: Shrink on its caster, Close Confine on its target.
struct Body {
    unit: Option<u32>,
    start: Instant,
}

/// A performer's cone of bubbles.
struct Cone {
    unit: Option<u32>,
    start: Instant,
    length: u32,
}

/// One icon to draw: its picture, place, spin (degrees), half-diagonal (world units) and alpha (0 to 255).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IconDraw {
    pub picture: &'static str,
    pub at: Vector,
    pub spin: f32,
    pub half: f32,
    pub alpha: f32,
}

#[derive(Default)]
pub struct Songs {
    clouds: Vec<Cloud>,
    shrinks: Vec<Body>,
    confines: Vec<Body>,
    cones: Vec<Cone>,
}

/// An icon's alpha at its n-th tick (from 1) in a cloud of `length` ticks: 5 a tick to 245, held, then 5 less a tick over the last 50.
pub fn icon_alpha(length: u32, n: u32) -> f32 {
    let fade = n.saturating_sub(length.saturating_sub(50));
    (5 * n.min(49)) as f32 - (5 * fade) as f32
}

/// One drift angle `c` heading for `t`: a degree a tick, taking a new target on passing it. `rand(n)` is below n.
pub fn drift(c: f32, t: f32, mut rand: impl FnMut(u32) -> u32) -> (f32, f32) {
    if t > c {
        let c = (c + 1.0).min(359.0);
        let whole = c.floor() as u32;
        (c, if c > t { if whole == 0 { 0.0 } else { rand(whole) as f32 } } else { t })
    } else {
        let c = (c - 1.0).max(0.0);
        let whole = c.floor() as u32;
        (c, if c <= t { if whole == 0 { 359.0 } else { 359.0 - rand(whole) as f32 } } else { t })
    }
}

impl Icon {
    fn step(&mut self, mut rand: impl FnMut(u32) -> u32) {
        self.at[0] += self.drift[0].0.to_radians().sin() * 0.1;
        self.at[2] += self.drift[1].0.to_radians().sin() * 0.1;
        for (c, t) in &mut self.drift {
            (*c, *t) = drift(*c, *t, &mut rand);
        }
        self.wobble = (self.wobble + 1.0) % 360.0;
        self.spin = (self.spin + 1.0) % 360.0;
    }
}

/// Close Confine's body colour at tick e: #FA3232 on the even ticks 22 to 58 and white on the odd ones, from tick 21.
pub fn confine_tint(e: u32) -> Option<[u8; 3]> {
    ((21..CONFINE_TICKS).contains(&e) && e.is_multiple_of(2)).then_some([0xFA, 0x32, 0x32])
}

/// Shrink's body at tick e: added on in #FAFA64 on the even ticks 6 to 24.
pub fn shrink_glow(e: u32) -> Option<(u32, Option<[u8; 3]>)> {
    ((6..=24).contains(&e) && e.is_multiple_of(2)).then_some((crate::actor::GLOW_ADDITIVE, Some([0xFA, 0xFA, 0x64])))
}

/// A cone's alpha at tick a: 3 a tick to 87, held, then from tick 74 2 less a tick.
pub fn cone_alpha(a: u32) -> f32 {
    let age = a + 1;
    match age {
        0..=29 => 3.0 * age as f32,
        30..=74 => 87.0,
        _ => (87.0 - 2.0 * (age - 74) as f32).max(0.0),
    }
}

/// Band k's turn at tick a, in degrees, and its radius.
pub fn cone_turn(k: usize, a: u32) -> f32 {
    ([180.0, 270.0, 0.0, 90.0][k] + 3.0 * (a + 1) as f32) % 360.0
}

pub fn cone_radius(k: usize, a: u32) -> f32 {
    4.3 + cone_turn(k, a).to_radians().sin()
}

/// The cone's height at point s of band k at tick a: a wave round the rim.
pub fn cone_profile(k: usize, s: u32, a: u32) -> f32 {
    let age = a + 1;
    let rim = if k < 2 { (age + 90 * k as u32) % 360 } else { (2 * (age + 45 * k as u32)) % 360 };
    (9.0 * s as f32).to_radians().sin() * 23.0 * 0.3 * (rim as f32).to_radians().sin() + 23.0
}

/// The picture of the cone at tick a: W_bubble02 first, then in turn.
pub fn cone_picture(a: u32) -> &'static str {
    BUBBLES[((a + 1) % 27) as usize]
}

const LEANS: [f32; 4] = [75.0, 63.0, 52.0, 40.0];

impl Cone {
    fn walls(&self, feet: Vector, a: u32) -> [Wall; 2] {
        let alpha = cone_alpha(a);
        let vertices = |rgb: [f32; 3], alpha: f32| -> Vec<MeshVertex> {
            (0..4)
                .flat_map(|k| {
                    let (turn, radius) = (cone_turn(k, a), cone_radius(k, a));
                    let points: Vec<_> = (0..=20).map(|s| ring_point(feet, turn + 18.0 * s as f32, radius, cone_profile(k, s, a), LEANS[k])).collect();
                    ring_quads(&points, alpha / 255.0).into_iter().map(move |(position, uv, alpha)| MeshVertex {
                        position,
                        uv,
                        tint: [rgb[0], rgb[1], rgb[2], alpha],
                    })
                })
                .collect()
        };
        let texture = cone_picture(a);
        [
            Wall { texture, vertices: vertices([1.0; 3], alpha.trunc()), additive: false, forced: false },
            Wall { texture, vertices: vertices([1.0, 0.0, 0.0], (3.0 * alpha).min(255.0).trunc()), additive: true, forced: false },
        ]
    }
}

impl Songs {
    /// A Twilight Alchemy cloud (573 to 575) lasting `length` ticks, replacing the last; each icon's choices from `rand(n)`
    /// (0 to n − 1) in the order the game draws them.
    pub fn twilight(&mut self, effect: u32, length: u32, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        let mut icons = Vec::with_capacity(ICONS);
        let mut picture = match effect {
            573 => HERB,
            574 => WHITE_SLIM_POTION,
            _ => MIXED[0],
        };
        for index in 0..ICONS {
            // Each prim of four icons picks one of the three pictures, for Twilight Alchemy 3.
            if index % 4 == 0 && effect == 575 {
                picture = MIXED[rand(3) as usize];
            }
            let spin = rand(360) as f32;
            let offset = [rand(160) as f32 - 80.0, rand(160) as f32 - 80.0];
            let rise = 5.0 + rand(66) as f32;
            let drift = [(rand(360) as f32, rand(360) as f32), (rand(360) as f32, rand(360) as f32)];
            let size = rand(11) as f32 * 0.1 + 3.0;
            let wobble = rand(360) as f32;
            icons.push(Icon { picture, spin, offset, rise, drift, size, wobble, at: [0.0; 3] });
        }
        self.clouds.clear();
        self.clouds.push(Cloud { length, start: now, ticks: 0, settled: false, icons });
    }

    /// A cast cut off: the cloud goes.
    pub fn cut_twilight(&mut self) {
        self.clouds.clear();
    }

    /// Shrink's flashes on `unit`.
    pub fn shrink(&mut self, unit: Option<u32>, now: Instant) {
        self.shrinks.push(Body { unit, start: now });
    }

    /// Close Confine's blinking and shaking on `unit`.
    pub fn confine(&mut self, unit: Option<u32>, now: Instant) {
        self.confines.push(Body { unit, start: now });
    }

    /// A performer's cone of bubbles on `unit`, `length` ticks (at least 120).
    pub fn cone(&mut self, unit: Option<u32>, length: u32, now: Instant) {
        self.cones.push(Cone { unit, start: now, length: length.max(120) });
    }

    /// Puts the clouds' icons round the player (`player`, once known) over the ground (`ground_at(x, z)`) and runs them up to
    /// `now` a tick at a time; drops what ended.
    pub fn advance(&mut self, now: Instant, player: Option<Vector>, ground_at: impl Fn(f32, f32) -> f32, mut rand: impl FnMut(u32) -> u32) {
        for cloud in &mut self.clouds {
            if !cloud.settled
                && let Some(player) = player
            {
                for icon in &mut cloud.icons {
                    let (x, z) = (player[0] + icon.offset[0], player[2] + icon.offset[1]);
                    icon.at = [x, ground_at(x, z) + icon.rise, z];
                }
                cloud.settled = true;
            }
            while cloud.settled && cloud.ticks < passes(cloud.start, now) {
                cloud.icons.iter_mut().for_each(|icon| icon.step(&mut rand));
                cloud.ticks += 1;
            }
        }
        self.clouds.retain(|cloud| cloud.ticks < cloud.length + 1);
        self.shrinks.retain(|body| passes(body.start, now) <= SHRINK_TICKS);
        self.confines.retain(|body| passes(body.start, now) <= CONFINE_TICKS);
        self.cones.retain(|cone| passes(cone.start, now) <= cone.length);
    }

    /// The cloud's icons at `now`.
    pub fn icons(&self) -> Vec<IconDraw> {
        let mut draws = Vec::new();
        for cloud in self.clouds.iter().filter(|cloud| cloud.settled && cloud.ticks > 0 && cloud.ticks < cloud.length) {
            let alpha = icon_alpha(cloud.length, cloud.ticks);
            for icon in &cloud.icons {
                let half = icon.size + icon.size * icon.wobble.to_radians().sin() * 0.05;
                draws.push(IconDraw { picture: icon.picture, at: icon.at, spin: icon.spin, half, alpha });
            }
        }
        draws
    }

    /// The body flags and colour Shrink gives `unit` at `now`.
    pub fn glow(&self, unit: Option<u32>, now: Instant) -> Option<(u32, Option<[u8; 3]>)> {
        self.shrinks.iter().filter(|body| body.unit == unit).find_map(|body| shrink_glow(passes(body.start, now).checked_sub(1)?))
    }

    /// The colour Close Confine gives `unit`'s body at `now`.
    pub fn body_tint(&self, unit: Option<u32>, now: Instant) -> Option<[u8; 3]> {
        self.confines.iter().filter(|body| body.unit == unit).find_map(|body| confine_tint(passes(body.start, now).checked_sub(1)?))
    }

    /// Whether `unit`'s parts shake now: Close Confine's, from tick 21 to 59.
    pub fn shakes(&self, unit: Option<u32>, now: Instant) -> bool {
        self.confines
            .iter()
            .filter(|body| body.unit == unit)
            .any(|body| passes(body.start, now).checked_sub(1).is_some_and(|e| (21..CONFINE_TICKS).contains(&e)))
    }

    /// The cones' walls at `now`; `position_of` gives a unit's feet.
    pub fn meshes(&self, now: Instant, position_of: impl Fn(Option<u32>) -> Option<Vector>) -> Vec<Wall> {
        let mut walls = Vec::new();
        for cone in &self.cones {
            let (Some(feet), Some(a)) = (position_of(cone.unit), passes(cone.start, now).checked_sub(1)) else { continue };
            if a < CONE_TICKS.min(cone.length - 1) {
                walls.extend(cone.walls(feet, a));
            }
        }
        walls
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prims::TICK;

    fn near(got: f32, want: f32) -> bool {
        (got - want).abs() < 0.01
    }

    #[test]
    fn twilight_scatters_320_icons_that_fade_in_and_out() {
        let start = Instant::now();
        let mut songs = Songs::default();
        songs.twilight(573, 187, start, |_| 0);
        assert_eq!(songs.clouds[0].icons.len(), 320);
        // Round the player (once known), 5 to 70 up from the ground, within ±80 of the player.
        let mut rolls = [80, 79, 65].into_iter().cycle();
        let mut spread = Songs::default();
        spread.twilight(573, 187, start, |_| rolls.next().unwrap());
        spread.advance(start, Some([100.0, 0.0, 200.0]), |_, _| 3.0, |_| 0);
        let icon = &spread.clouds[0].icons[0];
        assert_eq!(icon.picture, HERB);
        assert!((spread.clouds[0].icons[0].at[0] - 100.0 - 0.0).abs() <= 80.0);
        // Alpha for a cast of 187 ticks at n = 1, 49, 137, 186; for 300 at n = 250, 299.
        assert_eq!([1, 49, 137, 186].map(|n| icon_alpha(187, n)), [5.0, 245.0, 245.0, 0.0]);
        assert_eq!([250, 299].map(|n| icon_alpha(300, n)), [245.0, 0.0]);
        // The drift pairs: (10, 20) steps to 11 and keeps its target; (20, 19) steps to 19 and takes 359 − R(19); (359, 400) stays.
        assert_eq!(drift(10.0, 20.0, |_| unreachable!()), (11.0, 20.0));
        assert_eq!(drift(20.0, 19.0, |n| n - 1), (19.0, 359.0 - 18.0));
        assert_eq!(drift(359.0, 400.0, |_| unreachable!()), (359.0, 400.0));
        assert_eq!(drift(0.0, 0.0, |_| unreachable!()), (0.0, 359.0));
        assert_eq!(drift(5.0, 10.0, |n| n - 1).0, 6.0);
    }

    #[test]
    fn twilight_places_each_icon_round_the_player_and_runs_it_a_tick_at_a_time() {
        let start = Instant::now();
        let mut songs = Songs::default();
        // Every draw 0: spin 0, 80 west and north, 5 up, drifts (0, 0), size 3.0, wobble 0.
        songs.twilight(574, 187, start, |_| 0);
        assert!(songs.icons().is_empty(), "nothing before the player's place is known");
        songs.advance(start, Some([100.0, 0.0, 200.0]), |_, _| 3.0, |_| 0);
        assert_eq!(songs.clouds[0].icons[0].at, [20.0, 8.0, 120.0]);
        // The first tick: n = 1, alpha 5, the drift angles at 0 going to 359 afterwards.
        let icons = songs.icons();
        assert_eq!(icons.len(), 320);
        assert!(icons[0].alpha == 5.0 && icons[0].picture == WHITE_SLIM_POTION && near(icons[0].half, 3.0));
        // Per tick x moves sin(drift) · 0.1: 0 at first (angle 0), and the wobble and spin go up one.
        songs.advance(start + TICK * 9, Some([100.0, 0.0, 200.0]), |_, _| 3.0, |_| 0);
        assert_eq!(songs.clouds[0].ticks, 10);
        assert_eq!(songs.icons()[0].alpha, 50.0);
        assert_eq!(songs.clouds[0].icons[0].spin, 10.0);
        assert!(songs.clouds[0].icons[0].at[0] >= 20.0);
        // Gone with its cast, or cut.
        songs.advance(start + TICK * 200, Some([0.0; 3]), |_, _| 0.0, |_| 0);
        assert!(songs.clouds.is_empty());
        songs.twilight(573, 187, start, |_| 0);
        songs.cut_twilight();
        assert!(songs.clouds.is_empty());
        // Twilight Alchemy 3: each prim of four takes one of the three pictures with its first draw.
        let mut rolls = (0..).map(|n: u32| n % 3);
        let mut mixed = Songs::default();
        mixed.twilight(575, 100, start, |bound| if bound == 3 { rolls.next().unwrap() } else { 0 });
        let pictures: Vec<&str> = mixed.clouds[0].icons.iter().map(|icon| icon.picture).collect();
        assert_eq!(pictures[0..4], [MIXED[0]; 4]);
        assert_eq!(pictures[4..8], [MIXED[1]; 4]);
        assert_eq!(pictures[8..12], [MIXED[2]; 4]);
    }

    #[test]
    fn shrink_flashes_yellow_and_close_confine_blinks_red() {
        let start = Instant::now();
        let mut songs = Songs::default();
        songs.shrink(Some(1), start);
        songs.confine(Some(2), start);
        // Shrink: even ticks 6…24 added on in #FAFA64 (ten of them), nothing at 0…5 and from 25.
        let flashes: Vec<u32> = (0..60).filter(|&e| songs.glow(Some(1), start + TICK * e).is_some()).collect();
        assert_eq!(flashes, [6, 8, 10, 12, 14, 16, 18, 20, 22, 24]);
        assert_eq!(songs.glow(Some(1), start + TICK * 6), Some((crate::actor::GLOW_ADDITIVE, Some([0xFA, 0xFA, 0x64]))));
        assert!(songs.glow(Some(2), start + TICK * 6).is_none());
        // Close Confine: nothing before tick 21; #FA3232 on the even ticks to 58 and white on the odd.
        let blinks: Vec<u32> = (0..70).filter(|&e| songs.body_tint(Some(2), start + TICK * e).is_some()).collect();
        assert_eq!(blinks.first(), Some(&22));
        assert_eq!(blinks.last(), Some(&58));
        assert_eq!(songs.body_tint(Some(2), start + TICK * 22), Some([0xFA, 0x32, 0x32]));
        assert_eq!(songs.body_tint(Some(2), start + TICK * 23), None);
        // It shakes on ticks 21…59 only.
        let shaking: Vec<u32> = (0..70).filter(|&e| songs.shakes(Some(2), start + TICK * e)).collect();
        assert_eq!((shaking.first(), shaking.last(), shaking.len()), (Some(&21), Some(&59), 39));
        assert!(!songs.shakes(Some(1), start + TICK * 30));
        songs.advance(start + TICK * 300, None, |_, _| 0.0, |_| 0);
        assert!(songs.shrinks.is_empty() && songs.confines.is_empty());
    }

    #[test]
    fn four_cones_of_red_bubbles_turn_round_the_performer() {
        let start = Instant::now();
        let mut songs = Songs::default();
        songs.cone(None, 0, start);
        assert_eq!(songs.cones[0].length, 120);
        // α at a = 0, 28, 29, 73, 74, 116, 117: 3, 87, 87, 87, 85, 1, 0.
        assert_eq!([0, 28, 29, 73, 74, 116, 117].map(cone_alpha), [3.0, 87.0, 87.0, 87.0, 85.0, 1.0, 0.0]);
        // The turn of band 1 at a = 0 is 273°; the radius of band 0 is 4.3 + sin 183°; p at s = 10, band 0, a = 0.
        assert_eq!(cone_turn(1, 0), 273.0);
        assert!(near(cone_radius(0, 0), 4.3 + 183.0_f32.to_radians().sin()) && near(cone_radius(0, 0), 4.248));
        assert!(
            near(cone_profile(0, 10, 0), 23.0 + 6.9 * 1.0_f32.to_radians().sin()) && near(cone_profile(0, 0, 0), 23.0) && near(cone_profile(0, 20, 0), 23.0)
        );
        // Picture 2 at a = 0 and picture 1 at a = 26.
        assert_eq!((cone_picture(0), cone_picture(26)), (BUBBLES[1], BUBBLES[0]));
        // Two passes of four bands of 20 quads: blended white and added red at three times the alpha, 255 at most.
        let feet = [0.0, 0.0, 0.0];
        let walls = songs.meshes(start, |_| Some(feet));
        assert_eq!(walls.len(), 2);
        assert!(walls.iter().all(|wall| wall.vertices.len() == 4 * 20 * 6));
        assert!(!walls[0].additive && walls[1].additive);
        assert_eq!(walls[1].vertices[0].tint[..3], [1.0, 0.0, 0.0]);
        let pass_two = |a: u32| (songs.meshes(start + TICK * a, |_| Some(feet))[1].vertices[0].tint[3] * 255.0).round();
        assert_eq!([28, 74, 75].map(pass_two), [255.0, 255.0, 249.0]);
        // Drawn on ticks 0…118, not at 119.
        assert_eq!(songs.meshes(start + TICK * 118, |_| Some(feet)).len(), 2);
        assert!(songs.meshes(start + TICK * 119, |_| Some(feet)).is_empty());
        // It follows its performer.
        assert!(songs.meshes(start, |_| None).is_empty());
        songs.advance(start + TICK * 122, None, |_, _| 0.0, |_| 0);
        assert!(songs.cones.is_empty());
    }
}
