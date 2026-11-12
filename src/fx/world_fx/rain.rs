//! The STR rains: objects that drop many small STRs on random spots round a unit or a cell, again and again while their
//! skill lasts. Severe Rainstorm (1276), Rain of Crystal (1752), Diamond Storm (2026), the Sky Emperor's Star Burst
//! (2348), Twinkling Galaxy (2349) and Star Cannon (2350), the Night Watch's Grenades Dropping (2351) and Mission
//! Bombard (2354), and Cross Rain (2462).

use super::area4::Rand;
use super::{Assets, Class, Event, Eye, FLAGS_EFFECT, FLAGS_FLOOR, Face, Fx, Seen, StrPart, WorldFx, view_offset};
use crate::world::Vector;
use crate::world_str::Scene;

pub const SEVERE_RAINSTORM: u32 = 1276;
pub const RAIN_OF_CRYSTAL: u32 = 1752;
pub const DIAMOND_STORM: u32 = 2026;
pub const STAR_BURST: u32 = 2348;
pub const TWINKLING_GALAXY: u32 = 2349;
pub const STAR_CANNON: u32 = 2350;
pub const GRENADES_DROPPING: u32 = 2351;
pub const MISSION_BOMBARD: u32 = 2354;
pub const CROSS_RAIN: u32 = 2462;

const RAINSTORM: &str = "new_severerainstorm\\new_severerainstorm\\new_severerainstorm";
const RAINSTORM_MIN: &str = "mineffect\\new_severerainstorm\\new_severerainstorm\\new_severerainstorm";
const CRYSTAL_BOTTOM: &str = "rain_of_crystal\\rain_of_crystal_attack_bottom\\rain_of_crystal_attack_bottom";
const CRYSTAL: &str = "rain_of_crystal\\rain_of_crystal_attack\\rain_of_crystal_attack";
const DIAMOND: &str = "diamond_storm\\diamond_storm\\diamond_storm";
const GALAXY_BOTTOM: &str = "SKY_EMPEROR\\SKE_TWINKLING_GALAXY\\twinkling_galaxy_bottom\\ske_twinkling_galaxy_bottom";
/// Followed by the star's number, 00 … 03.
const GALAXY: &str = "SKY_EMPEROR\\SKE_TWINKLING_GALAXY\\twinkling_galaxy\\ske_twinkling_galaxy";
const CANNON_BOTTOM: &str = "SKY_EMPEROR\\SKE_STAR_CANNON\\star_cannon_bottom\\ske_star_cannon_bottom";
const CANNON: &str = "SKY_EMPEROR\\SKE_STAR_CANNON\\star_cannon\\ske_star_cannon";
const GRENADE_BOTTOM: &str = "NIGHT_WATCH\\NW_GRENADES_DROPPING\\grenades_dropping_bottom\\grenades_dropping_bottom";
const GRENADE: &str = "NIGHT_WATCH\\NW_GRENADES_DROPPING\\grenades_dropping\\grenades_dropping";
const BOMBARD: &str = "NIGHT_WATCH\\NW_MISSION_BOMBARD\\mission_bombard\\mission_bombard_hit";
const CROSS: &str = "IMPERIAL_GUARD\\IG_CROSS_RAIN\\new_crossrain\\new_cross_rain\\new_cross_rain";

const CRYSTAL_SOUND: &str = "effect\\AG_RAIN_OF_CRYSTAL.wav";
const STAR_SOUND: &str = "effect\\SKY_EMPEROR\\ske_star_cannon.wav";

/// Severe Rainstorm and Diamond Storm end once their counter passes these.
const RAINSTORM_TICKS: u32 = 240;
const DIAMOND_TICKS: u32 = 60;

pub fn register(fx: &mut WorldFx) {
    for id in [SEVERE_RAINSTORM, RAIN_OF_CRYSTAL, DIAMOND_STORM, STAR_BURST, TWINKLING_GALAXY, STAR_CANNON, GRENADES_DROPPING, MISSION_BOMBARD, CROSS_RAIN] {
        fx.register(id, || Box::new(Rain::new(Box::new(pick))));
    }
}

/// One random whole number below `n`.
fn pick(n: u32) -> u32 {
    crate::effects::pick(0, n.saturating_sub(1))
}

/// The ticks between spawns at `level` (1-based); None when the level has no entry.
fn period(id: u32, level: i32) -> Option<u32> {
    match id {
        SEVERE_RAINSTORM => Some(3),
        RAIN_OF_CRYSTAL => (level >= 1).then(|| (4 - (level + 1) / 2).max(1) as u32),
        DIAMOND_STORM => Some(5),
        STAR_BURST | GRENADES_DROPPING | MISSION_BOMBARD => Some(15),
        TWINKLING_GALAXY | STAR_CANNON => Some(18),
        CROSS_RAIN => Some((9 - level.clamp(1, 10) / 3) as u32),
        _ => None,
    }
}

/// W of a spawn's draws at `level` (a spot is up to W / 20 units each way, W = 50·n); None when the level has no entry,
/// and for the two that draw their own way.
fn spread(id: u32, level: i32) -> Option<u32> {
    let by_level = |table: &[u32]| usize::try_from(level - 1).ok().and_then(|index| table.get(index)).copied();
    let n = match id {
        RAIN_OF_CRYSTAL if level > 5 => Some(19),
        RAIN_OF_CRYSTAL => by_level(&[11, 13, 15, 17, 19]),
        DIAMOND_STORM | MISSION_BOMBARD => Some(9),
        TWINKLING_GALAXY | STAR_CANNON => by_level(&[13, 13, 11, 11, 9]),
        GRENADES_DROPPING => by_level(&[11, 11, 11, 9, 9]),
        CROSS_RAIN => [3, 3, 3, 5, 5, 5, 7, 7, 7, 9].get(level.clamp(1, 10) as usize - 1).copied(),
        _ => None,
    };
    n.map(|n| 50 * n)
}

/// How far up from the ground point its parts are drawn.
fn lift(id: u32) -> f32 {
    match id {
        TWINKLING_GALAXY | STAR_CANNON | GRENADES_DROPPING | MISSION_BOMBARD => 15.0,
        CROSS_RAIN => 10.0,
        _ => 0.0,
    }
}

/// A spot `lift` units over a point up to W / 20 units from `at` each way: `(R(W + 1) − W / 2) · 0.1`, x drawn before z.
fn spot(at: Vector, w: u32, lift: f32, rand: &mut Rand) -> Vector {
    let mut draw = || (rand(w + 1) as f32 - (w / 2) as f32) * 0.1;
    let dx = draw();
    let dz = draw();
    [at[0] + dx, at[1] + lift, at[2] + dz]
}

/// Severe Rainstorm's offset on one axis: `R(251) · 0.1`, + when a first draw `R(10)` is 5 or more, else −.
fn signed(rand: &mut Rand) -> f32 {
    let sign = if rand(10) >= 5 { 1.0 } else { -1.0 };
    sign * rand(251) as f32 * 0.1
}

/// 0.2 units right of `feet` and 13.5 up, on the screen.
fn over_head(feet: Vector, scene: &Scene) -> Vector {
    view_offset(feet, 0.2, 13.5, &Eye { right: scene.right, up: scene.up, forward: [0.0; 3] })
}

/// A part playing `file` (its `min_` file when `min`) at `point`: under the unit sprites when the name ends in `_bottom`.
fn part(file: &str, min: bool, point: Vector, assets: &mut dyn Assets) -> StrPart {
    let mut part = StrPart::new(file, min, assets);
    part.flags = if file.ends_with("_bottom") { FLAGS_FLOOR } else { FLAGS_EFFECT };
    part.point = point;
    part
}

/// The STR spawner: every period it drops parts on spots round its point, steps them, and frees those that ended.
struct Rain {
    rand: Rand,
    parts: Vec<StrPart>,
}

impl Rain {
    fn new(rand: Rand) -> Self {
        Self { rand, parts: Vec::new() }
    }

    /// This tick's parts, and their sounds a tick on at their spot.
    fn spawn(&mut self, fx: &mut Fx, scene: &Scene, assets: &mut dyn Assets) {
        let Self { rand, parts } = self;
        let (id, e, at, min, level) = (fx.id, fx.age, fx.position, fx.min, fx.a7);
        let mut sound = |file: &str, point: Vector| fx.events.push((e + 1, Event::SoundAt(file.to_owned(), point)));
        match id {
            SEVERE_RAINSTORM => {
                let dx = signed(rand);
                let dz = 10.0 + signed(rand);
                parts.push(part(if min { RAINSTORM_MIN } else { RAINSTORM }, false, [at[0] + dx, at[1], at[2] + dz], assets));
            }
            STAR_BURST => {
                let point = over_head(at, scene);
                for file in [GALAXY_BOTTOM.to_owned(), format!("{GALAXY}{:02}", (e / 15) & 3)] {
                    parts.push(part(&file, min, point, assets));
                }
                sound(STAR_SOUND, point);
            }
            _ => {
                let Some(w) = spread(id, level) else { return };
                let (files, min, count) = match id {
                    RAIN_OF_CRYSTAL => (vec![CRYSTAL_BOTTOM.to_owned(), CRYSTAL.to_owned()], min, 1),
                    DIAMOND_STORM => (vec![DIAMOND.to_owned()], false, 1),
                    TWINKLING_GALAXY => (vec![GALAXY_BOTTOM.to_owned(), format!("{GALAXY}03")], false, 1),
                    STAR_CANNON => (vec![CANNON_BOTTOM.to_owned(), CANNON.to_owned()], false, 1),
                    GRENADES_DROPPING => (vec![GRENADE_BOTTOM.to_owned(), GRENADE.to_owned()], false, (level + 1) / 2),
                    MISSION_BOMBARD => (vec![BOMBARD.to_owned()], false, 3),
                    _ => (vec![CROSS.to_owned()], min, 1),
                };
                for _ in 0..count {
                    let point = spot(at, w, lift(id), rand);
                    for file in &files {
                        let mut part = part(file, min, point, assets);
                        // Each blast its own size, drawn after its spot.
                        if id == MISSION_BOMBARD {
                            part.controls.scale = (rand(51) + 50) as f32 * 0.01;
                        }
                        parts.push(part);
                    }
                    match id {
                        RAIN_OF_CRYSTAL if rand(7) == 0 => sound(CRYSTAL_SOUND, point),
                        TWINKLING_GALAXY | STAR_CANNON => sound(STAR_SOUND, point),
                        _ => {}
                    }
                }
            }
        }
    }
}

impl Class for Rain {
    fn begin(&mut self, fx: &mut Fx, _: &mut dyn Assets) {
        fx.single = fx.id == STAR_BURST;
    }

    /// The spawn when the counter is a multiple of the period, then every part steps (Star Burst's twice, kept over the
    /// head); the registry then counts on.
    fn update(&mut self, fx: &mut Fx, scene: &Scene, assets: &mut dyn Assets) {
        let e = fx.age;
        fx.events.retain(|(age, _)| *age > e);
        let spawns = !fx.released && (fx.id != STAR_BURST || fx.owned(scene));
        if spawns && period(fx.id, fx.a7).is_some_and(|period| e.is_multiple_of(period)) {
            self.spawn(fx, scene, assets);
        }
        if fx.id == STAR_BURST {
            let point = over_head(fx.position, scene);
            for part in &mut self.parts {
                part.point = point;
            }
            self.parts.retain_mut(|part| part.step() && part.step());
        } else {
            self.parts.retain_mut(StrPart::step);
        }
    }

    fn draw<'a>(&'a self, _: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        for part in &self.parts {
            part.faces(&seen.eye, out);
        }
    }

    /// Severe Rainstorm and Diamond Storm by their counter (cutting their parts), Rain of Crystal with its unit, Star Burst
    /// with its unit and status and then its parts; the rest until their unit goes and their parts finish.
    fn alive(&self, fx: &Fx, scene: &Scene) -> bool {
        match fx.id {
            SEVERE_RAINSTORM => fx.age <= RAINSTORM_TICKS,
            DIAMOND_STORM => fx.age <= DIAMOND_TICKS,
            RAIN_OF_CRYSTAL => !fx.released,
            STAR_BURST => !self.parts.is_empty() || (!fx.released && fx.owned(scene)),
            _ => !fx.released || !self.parts.is_empty(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::Launch;
    use super::super::area4::tests::{FEET, Fake, HERE, Solo, UNIT, close3, scripted, top};
    use super::*;

    /// A copy of `id` at FEET.
    fn copy(id: u32, owner: Option<Option<u32>>, level: i32, min: bool, rand: Rand, assets: &mut Fake) -> Solo<Rain> {
        Solo::new(Rain::new(rand), Launch { a7: Some(level), min, ..Launch::at(id, owner, FEET) }, assets)
    }

    /// Ticks while alive, at most `limit` times: each counter value that spawned, with how many parts.
    fn spawned(solo: &mut Solo<Rain>, here: &[(u32, Vector)], assets: &mut Fake, limit: u32) -> Vec<(u32, usize)> {
        let mut spawns = Vec::new();
        for _ in 0..limit {
            if !solo.alive(here) {
                break;
            }
            let (e, before) = (solo.fx.age, assets.strs.len());
            solo.tick(here, assets);
            if assets.strs.len() > before {
                spawns.push((e, assets.strs.len() - before));
            }
        }
        spawns
    }

    fn up(at: Vector, by: f32) -> Vector {
        [at[0], at[1] + by, at[2]]
    }

    #[test]
    fn a_spot_draws_x_then_z_up_to_2_5_n_each_way() {
        for n in [3, 9, 11, 19] {
            let w = 50 * n;
            let (mut rand, asked) = scripted(&[0, w]);
            let half = 2.5 * n as f32;
            assert!(close3(spot(FEET, w, 15.0, &mut rand), [FEET[0] - half, FEET[1] + 15.0, FEET[2] + half]));
            assert_eq!(*asked.borrow(), [w + 1, w + 1]);
        }
    }

    #[test]
    fn diamond_storm_drops_13_crystals_and_is_cut_at_61() {
        let mut assets = Fake::default();
        let mut solo = copy(DIAMOND_STORM, None, -1, true, top(), &mut assets);
        assert_eq!(spawned(&mut solo, &[], &mut assets, 1000), (0..=60).step_by(5).map(|e| (e, 1)).collect::<Vec<_>>());
        assert_eq!(solo.fx.age, 61);
        assert!(!solo.alive(&[]) && !solo.class.parts.is_empty());
        // No `min_` swap; the top draws are 22.5 units out on the ground.
        assert!(assets.strs.iter().all(|file| file == DIAMOND));
        assert!(close3(solo.class.parts[0].point, [FEET[0] + 22.5, FEET[1], FEET[2] + 22.5]));
        assert!(solo.class.parts.iter().all(|part| part.flags == FLAGS_EFFECT && part.offset == super::super::STR_OFFSET));
    }

    #[test]
    fn severe_rainstorm_drops_81_streaks_round_a_point_10_units_along_z() {
        let mut assets = Fake::default();
        let mut solo = copy(SEVERE_RAINSTORM, None, -1, false, top(), &mut assets);
        let spawns = spawned(&mut solo, &[], &mut assets, 1000);
        assert_eq!(spawns, (0..=240).step_by(3).map(|e| (e, 1)).collect::<Vec<_>>());
        assert_eq!(solo.fx.age, 241);
        assert!(assets.strs.iter().all(|file| file == RAINSTORM));
        // R(10) ≥ 5 is +: x +25, z 10 − 25; then x +0, z 10 − 0.3.
        let (rand, asked) = scripted(&[5, 250, 4, 250, 9, 0, 0, 3]);
        let mut solo = copy(SEVERE_RAINSTORM, None, -1, true, rand, &mut assets);
        solo.tick(&[], &mut assets);
        assert!(close3(solo.class.parts[0].point, [FEET[0] + 25.0, FEET[1], FEET[2] - 15.0]));
        assert_eq!(*asked.borrow(), [10, 251, 10, 251]);
        for _ in 0..3 {
            solo.tick(&[], &mut assets);
        }
        assert!(close3(solo.class.parts[1].point, [FEET[0], FEET[1], FEET[2] + 9.7]));
        assert_eq!(assets.strs.last().map(String::as_str), Some(RAINSTORM_MIN));
        assert!(solo.sounds().is_empty());
    }

    #[test]
    fn rain_of_crystal_by_level() {
        assert_eq!([1, 2, 3, 4, 5, 6, 7].map(|level| period(RAIN_OF_CRYSTAL, level)), [3, 3, 2, 2, 1, 1, 1].map(Some));
        assert_eq!([1, 2, 3, 4, 5, 6].map(|level| spread(RAIN_OF_CRYSTAL, level).unwrap() as f32 / 20.0), [27.5, 32.5, 37.5, 42.5, 47.5, 47.5]);
        assert_eq!((period(RAIN_OF_CRYSTAL, 0), spread(RAIN_OF_CRYSTAL, 0)), (None, None));
        let mut assets = Fake::default();
        // x, z, R(7): the sound a tick on only for a 0.
        let (rand, asked) = scripted(&[0, 550, 0, 550, 550, 1]);
        let mut solo = copy(RAIN_OF_CRYSTAL, Some(Some(UNIT)), 1, true, rand, &mut assets);
        solo.tick(&HERE, &mut assets);
        let point = [FEET[0] - 27.5, FEET[1], FEET[2] + 27.5];
        let parts: Vec<_> = solo.class.parts.iter().map(|part| part.flags).collect();
        assert_eq!(parts, [FLAGS_FLOOR, FLAGS_EFFECT]);
        assert!(solo.class.parts.iter().all(|part| close3(part.point, point)));
        assert_eq!(
            assets.strs,
            [
                "rain_of_crystal\\rain_of_crystal_attack_bottom\\min_rain_of_crystal_attack_bottom",
                "rain_of_crystal\\rain_of_crystal_attack\\min_rain_of_crystal_attack"
            ]
        );
        assert_eq!(solo.fx.events, [(1, Event::SoundAt(CRYSTAL_SOUND.into(), point))]);
        for _ in 0..3 {
            solo.tick(&HERE, &mut assets);
        }
        assert_eq!(*asked.borrow(), [551, 551, 7, 551, 551, 7]);
        assert!(close3(solo.class.parts[2].point, [FEET[0] + 27.5, FEET[1], FEET[2] + 27.5]));
        assert!(solo.fx.events.is_empty());
        // Periods 3, 2 and 1 by level.
        for (level, every) in [(2, 3), (4, 2), (5, 1)] {
            let mut solo = copy(RAIN_OF_CRYSTAL, Some(Some(UNIT)), level, false, top(), &mut assets);
            let ticks: Vec<u32> = spawned(&mut solo, &HERE, &mut assets, 7).into_iter().map(|(e, _)| e).collect();
            assert_eq!(ticks, (0..7).step_by(every).collect::<Vec<_>>(), "{level}");
        }
        // It ends with its unit, parts and all.
        solo.fx.released = true;
        assert!(!solo.alive(&HERE) && !solo.class.parts.is_empty());
    }

    #[test]
    fn star_burst_cycles_four_stars_over_the_head_at_double_speed() {
        let mut assets = Fake::default();
        let mut solo = copy(STAR_BURST, Some(Some(UNIT)), -1, true, top(), &mut assets);
        assert!(solo.fx.single);
        assert_eq!(spawned(&mut solo, &HERE, &mut assets, 61), [0, 15, 30, 45, 60].map(|e| (e, 2)));
        let bottom = "SKY_EMPEROR\\SKE_TWINKLING_GALAXY\\twinkling_galaxy_bottom\\min_ske_twinkling_galaxy_bottom";
        assert!(assets.strs.iter().step_by(2).all(|file| file == bottom));
        let stars: Vec<_> = assets.strs.iter().skip(1).step_by(2).map(|file| file.rsplit('\\').next().unwrap()).collect();
        assert_eq!(
            stars,
            [
                "min_ske_twinkling_galaxy00",
                "min_ske_twinkling_galaxy01",
                "min_ske_twinkling_galaxy02",
                "min_ske_twinkling_galaxy03",
                "min_ske_twinkling_galaxy00"
            ]
        );
        // Every part sits over the moving unit's head.
        let eye = Eye::at(45.0, 0.0);
        let moved = [FEET[0] + 5.0, FEET[1], FEET[2]];
        solo.tick(&[(UNIT, moved)], &mut assets);
        assert!(solo.class.parts.iter().all(|part| close3(part.point, view_offset(moved, 0.2, 13.5, &eye))));
        assert_eq!(solo.class.parts.iter().map(|part| part.flags).take(2).collect::<Vec<_>>(), [FLAGS_FLOOR, FLAGS_EFFECT]);
        // The sound a tick on at the spot; two steps a tick, so a 50-frame star is freed at its 26th update; the status
        // going off lets it finish.
        let mut solo = copy(STAR_BURST, Some(Some(UNIT)), -1, false, top(), &mut assets);
        solo.tick(&HERE, &mut assets);
        assert_eq!(solo.fx.events, [(1, Event::SoundAt(STAR_SOUND.into(), view_offset(FEET, 0.2, 13.5, &eye)))]);
        solo.fx.released = true;
        assert_eq!(1 + solo.run(&HERE, &mut assets, 1000), 26);
        // A unit gone spawns no more; its parts finish.
        let mut solo = copy(STAR_BURST, Some(Some(UNIT)), -1, false, top(), &mut assets);
        solo.tick(&HERE, &mut assets);
        assert_eq!(spawned(&mut solo, &[], &mut assets, 1000), []);
        assert!(solo.class.parts.is_empty() && !solo.alive(&[]));
    }

    #[test]
    fn twinkling_galaxy_and_star_cannon_drop_a_star_every_18_ticks() {
        for (id, files) in [(TWINKLING_GALAXY, [GALAXY_BOTTOM.to_owned(), format!("{GALAXY}03")]), (STAR_CANNON, [CANNON_BOTTOM.to_owned(), CANNON.to_owned()])]
        {
            for (level, half) in [(1, 32.5), (2, 32.5), (3, 27.5), (4, 27.5), (5, 22.5)] {
                let mut assets = Fake::default();
                let (rand, asked) = scripted(&[]);
                let mut solo = copy(id, Some(Some(UNIT)), level, true, rand, &mut assets);
                solo.tick(&HERE, &mut assets);
                let point = [FEET[0] - half, FEET[1] + 15.0, FEET[2] - half];
                assert!(solo.class.parts.iter().all(|part| close3(part.point, point)));
                assert_eq!(asked.borrow()[0], (20.0 * half) as u32 + 1);
                assert_eq!(solo.fx.events, [(1, Event::SoundAt(STAR_SOUND.into(), point))]);
                // No `min_` swap.
                assert_eq!(assets.strs, files);
                assert_eq!(spawned(&mut solo, &HERE, &mut assets, 36), [(18, 2), (36, 2)]);
            }
            // No entry for level 6.
            let mut assets = Fake::default();
            let mut solo = copy(id, Some(Some(UNIT)), 6, false, top(), &mut assets);
            assert_eq!(spawned(&mut solo, &HERE, &mut assets, 40), []);
            // The unit gone, its stars finish: a 50-frame part is freed at its 51st step.
            let mut solo = copy(id, Some(Some(UNIT)), 1, false, top(), &mut assets);
            solo.tick(&HERE, &mut assets);
            solo.fx.released = true;
            assert_eq!(1 + solo.run(&HERE, &mut assets, 1000), 51);
        }
    }

    #[test]
    fn grenades_drop_in_pairs_by_level() {
        for (level, pairs, half) in [(1, 1, 27.5), (2, 1, 27.5), (3, 2, 27.5), (4, 2, 22.5), (5, 3, 22.5)] {
            let mut assets = Fake::default();
            let (rand, asked) = scripted(&[]);
            let mut solo = copy(GRENADES_DROPPING, Some(Some(UNIT)), level, false, rand, &mut assets);
            assert_eq!(spawned(&mut solo, &HERE, &mut assets, 16), [(0, 2 * pairs), (15, 2 * pairs)]);
            assert_eq!(*asked.borrow(), vec![(20.0 * half) as u32 + 1; 4 * pairs]);
            assert!(close3(solo.class.parts[0].point, [FEET[0] - half, FEET[1] + 15.0, FEET[2] - half]));
            assert_eq!(solo.class.parts.iter().map(|part| part.flags).take(2).collect::<Vec<_>>(), [FLAGS_FLOOR, FLAGS_EFFECT]);
            assert!(solo.sounds().is_empty());
        }
        assert_eq!(spread(GRENADES_DROPPING, 6), None);
    }

    #[test]
    fn mission_bombard_drops_three_blasts_of_random_size() {
        let mut assets = Fake::default();
        let (rand, asked) = scripted(&[0, 450, 0, 450, 0, 50, 225, 225, 25]);
        let mut solo = copy(MISSION_BOMBARD, Some(Some(UNIT)), 1, true, rand, &mut assets);
        assert_eq!(spawned(&mut solo, &HERE, &mut assets, 16), [(0, 3), (15, 3)]);
        assert_eq!(asked.borrow()[..9], [451, 451, 51, 451, 451, 51, 451, 451, 51]);
        let parts: Vec<_> = solo.class.parts.iter().take(3).map(|part| (part.point, part.controls.scale)).collect();
        let want = [([-22.5, 15.0, 22.5], 0.5), ([22.5, 15.0, -22.5], 1.0), ([0.0, 15.0, 0.0], 0.75)];
        for ((point, scale), (offset, size)) in parts.into_iter().zip(want) {
            assert!(close3(point, [FEET[0] + offset[0], FEET[1] + offset[1], FEET[2] + offset[2]]) && (scale - size).abs() < 1e-6, "{point:?} {scale}");
        }
        assert!(assets.strs.iter().all(|file| file == BOMBARD));
    }

    #[test]
    fn cross_rain_by_level() {
        assert_eq!((1..=10).map(|level| period(CROSS_RAIN, level).unwrap()).collect::<Vec<_>>(), [9, 9, 8, 8, 8, 7, 7, 7, 6, 6]);
        let halves: Vec<f32> = (1..=10).map(|level| spread(CROSS_RAIN, level).unwrap() as f32 / 20.0).collect();
        assert_eq!(halves, [7.5, 7.5, 7.5, 12.5, 12.5, 12.5, 17.5, 17.5, 17.5, 22.5]);
        assert_eq!((spread(CROSS_RAIN, 11), period(CROSS_RAIN, 0)), (Some(450), Some(9)));
        let mut assets = Fake::default();
        let mut solo = copy(CROSS_RAIN, Some(Some(UNIT)), 1, true, top(), &mut assets);
        assert_eq!(spawned(&mut solo, &HERE, &mut assets, 19), [(0, 1), (9, 1), (18, 1)]);
        assert!(assets.strs.iter().all(|file| file == "IMPERIAL_GUARD\\IG_CROSS_RAIN\\new_crossrain\\new_cross_rain\\min_new_cross_rain"));
        assert!(close3(solo.class.parts[0].point, up([FEET[0] + 7.5, FEET[1], FEET[2] + 7.5], 10.0)));
        assert!(solo.sounds().is_empty());
        // The unit gone: no more crosses; those falling finish.
        solo.fx.released = true;
        assert!(solo.alive(&HERE));
        assert_eq!(spawned(&mut solo, &HERE, &mut assets, 1000), []);
        assert!(!solo.alive(&HERE));
    }

    #[test]
    fn the_registry_runs_them_and_removing_one_cuts_its_parts() {
        let mut fx = WorldFx::default();
        let mut assets = Fake::default();
        for id in [SEVERE_RAINSTORM, RAIN_OF_CRYSTAL, DIAMOND_STORM, STAR_BURST, TWINKLING_GALAXY, STAR_CANNON, GRENADES_DROPPING, MISSION_BOMBARD, CROSS_RAIN]
        {
            assert!(fx.registered(id), "{id}");
        }
        fx.launch(&Launch::at(DIAMOND_STORM, None, FEET), &mut assets);
        super::super::area4::tests::scene(&[], |scene| fx.tick(scene, &mut assets));
        assert_eq!(assets.strs, [DIAMOND]);
        fx.remove(None, DIAMOND_STORM);
        assert!(fx.is_empty());
        // Star Burst's status going off: the copy stays to let its parts finish.
        fx.launch(&Launch::at(STAR_BURST, Some(Some(UNIT)), FEET), &mut assets);
        fx.release_one(Some(Some(UNIT)), STAR_BURST);
        assert!(fx.iter().all(|object| object.fx.released));
    }
}
