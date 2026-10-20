//! Spikes the game makes in code (its prim kind 9): open four-sided pyramids that rise out of the ground along their
//! own axis. Earth Spike's (EF_EARTHSPIKE 79), Heaven's Drive's field (EF_HEAVENSDRIVE 142), Grimtooth's train
//! (EF_GRIMTOOTH 123), Frost Diver's ice (EF_FROSTDIVER 27, EF_FROSTDIVER2 28) and Ice Wall's cells (EF_ICEWALL 74).

use std::time::Instant;

use super::{ICE_TEXTURE, TICK, passes};
use crate::effects::pick;
use crate::world::{MeshVertex, Vector};

/// What the stone spikes are made of (Frost Diver's are `ICE_TEXTURE`).
pub const STONE: &str = "data\\texture\\effect\\stone.bmp";

/// Earth Spike's and Heaven's Drive's lengths in ticks (Heaven's Drive raises its 250 by 50 as it starts).
const EARTH_SPIKE_TICKS: u32 = 250;
const HEAVENS_DRIVE_TICKS: u32 = 300;
/// Grimtooth's length: its point walks the whole of it, leaving a spike every 3rd tick.
const TRAIN_TICKS: u32 = 250;
const TRAIN_EVERY: u32 = 3;
/// How far Grimtooth's point walks a tick, and how near the caster must stand to the target for the train to stop
/// after its first spike (world units).
const TRAIN_STEP: f32 = 2.0;
const TRAIN_STOP: f32 = 2.5;
/// Frost Diver's line: a spike a tick from 5 units out toward the target, 2 units further each tick, the last within
/// 2.5 of the target (or on the effect's 150th tick if it never gets there). Its spikes stand 30 units under the
/// ground and live 40 ticks.
const FROST_FIRST: f32 = 5.0;
const FROST_STEP: f32 = 2.0;
const FROST_REACH: f32 = 2.5;
const FROST_TICKS: u32 = 150;
const FROST_LIFE: u32 = 40;
const FROST_DEPTH: f32 = 30.0;
/// Ice Wall's cell: three spikes a cell, 200 alpha, standing for the effect's whole 9999 ticks (or until their unit goes).
const ICE_WALL_TICKS: u32 = 9999;
const ICE_WALL_ALPHA: f32 = 200.0;
const ICE_WALL_HALF: f32 = 2.5;
/// Storm Gust's crystals stand until the effect's tick 215, 20 tall, from 20 under the ground.
const STORM_GUST_END: u32 = 215;
const STORM_GUST_HEIGHT: f32 = 20.0;
const STORM_GUST_DEPTH: f32 = 20.0;

/// A spike: an open pyramid `half` wide each way from its middle at the base and `height` from the base to the apex
/// along its axis (world units), moving along that axis.
struct Spike {
    texture: &'static str,
    born: Instant,
    /// Ticks run, ticks it lives, and ticks it's drawn: its effect ends sooner for one made late (a train's last).
    age: u32,
    life: u32,
    ends: u32,
    /// Where its base started, and its axes: across, across, and up its length (turned by its yaw and pitch).
    base: Vector,
    axes: [Vector; 3],
    half: f32,
    height: f32,
    /// How far it has moved along its axis, its speed a tick and the speed's change a tick.
    rise: f32,
    speed: f32,
    change: f32,
    /// At this age the speed becomes this and stops changing (Earth Spike's centre, Heaven's Drive's: it settles).
    turn: Option<(u32, f32)>,
    /// At this age it stops for good.
    stop: u32,
    /// None: it fades out from age `fade` (alpha 0–255). Some: it never fades, and 30 ticks before its end takes this
    /// speed and change (Heaven's Drive's sink back into the ground).
    sink: Option<(f32, f32)>,
    alpha: f32,
    alpha_step: f32,
    fade: u32,
    /// The skill unit on the ground it stands for: it goes with it.
    unit: Option<u32>,
    /// Gravitation Field's: every 6 ticks it bobs (the speed is −1.2 at age 2 and 1.18 at age 5 of each cycle).
    cycle: bool,
}

/// The axes of a spike turned `yaw` about the vertical, then pitched `pitch` from lying along it (90: upright), in
/// degrees: the game's x-turn then y-turn, its y down flipped up.
// ponytail: that a spike moves up its own axis (rises) is how the game's numbers read, not seen in it.
fn axes(yaw: f32, pitch: f32) -> [Vector; 3] {
    let (sy, cy) = yaw.to_radians().sin_cos();
    let (sp, cp) = pitch.to_radians().sin_cos();
    [[cy, 0.0, -sy], [sp * sy, -cp, sp * cy], [cp * sy, sp, cp * cy]]
}

/// A random whole number below `n`, as a float.
fn below(n: u32) -> f32 {
    pick(0, n - 1) as f32
}

impl Spike {
    /// A stone spike born at `born`, its base `down` units under `at`, turned `[yaw, pitch]`, `[half, height]` big,
    /// living `life` ticks in an effect that lets it be drawn `left` more; standing still and opaque until told otherwise.
    fn new(born: Instant, at: Vector, down: f32, [yaw, pitch]: [f32; 2], [half, height]: [f32; 2], life: u32, left: u32) -> Self {
        // One that would outlive its effect fades out over what the effect has left, from the start.
        let alpha_step = if life > left + 1 { -254.0 / left.max(1) as f32 } else { 0.0 };
        Self {
            texture: STONE,
            born,
            age: 0,
            life,
            ends: life.min(left),
            base: [at[0], at[1] - down, at[2]],
            axes: axes(yaw, pitch),
            half,
            height,
            rise: 0.0,
            speed: 0.0,
            change: 0.0,
            turn: None,
            stop: 0,
            sink: None,
            alpha: 254.0,
            alpha_step,
            fade: life - 10,
            unit: None,
            cycle: false,
        }
    }

    /// One tick: the speed changes and it moves on its axis; then it stops, sinks or fades as told.
    fn step(&mut self) {
        if self.cycle {
            match self.age % 6 {
                2 => (self.speed, self.change) = (-1.2, 0.0),
                5 => (self.speed, self.change) = (1.18, 0.01),
                _ => {}
            }
        }
        let speed = match self.turn {
            Some((age, speed)) if age == self.age => {
                self.change = 0.0;
                speed
            }
            _ => self.speed,
        };
        self.speed = speed + self.change;
        self.rise += self.speed;
        if self.age == self.stop {
            self.speed = 0.0;
            self.change = 0.0;
        }
        match self.sink {
            Some((speed, change)) if self.age + 30 == self.life => {
                self.speed = speed;
                self.change = change;
            }
            Some(_) => {}
            None => {
                if self.age == self.fade {
                    self.alpha_step = -self.alpha / (self.life - self.age) as f32;
                }
                self.alpha = (self.alpha + self.alpha_step).clamp(0.0, 254.0);
            }
        }
        self.age += 1;
    }

    /// A Frost Diver ice spike born at `born`, its base `around` units off `at` the way `turn` points: it rises 3 units
    /// the first tick, slowing by 0.15 a tick, and stops after age 20 (28.35 up); it fades over its last 10 ticks.
    #[allow(clippy::too_many_arguments)]
    fn ice(born: Instant, at: Vector, around: f32, turn: f32, size: [f32; 2], angles: [f32; 2], left: u32, alpha: f32) -> Self {
        let (sin, cos) = turn.to_radians().sin_cos();
        let spike = Spike::new(born, [at[0] + around * sin, at[1], at[2] - around * cos], FROST_DEPTH, angles, size, FROST_LIFE, left);
        Spike { texture: ICE_TEXTURE, speed: 3.0, change: -0.15, stop: 20, alpha, ..spike }
    }

    /// Its four faces: base corner, apex, base corner, with the texture across them a fifth at a time.
    fn faces(&self) -> [MeshVertex; 12] {
        let [across, side, up] = self.axes;
        let point =
            |x: f32, y: f32, z: f32| -> Vector { std::array::from_fn(|axis| self.base[axis] + across[axis] * x + side[axis] * y + up[axis] * (self.rise + z)) };
        // ponytail: a spike that never fades draws fully opaque (the game leaves its colour white, alpha 255).
        let alpha = if self.sink.is_some() { 1.0 } else { self.alpha / 255.0 };
        let tint = [1.0, 1.0, 1.0, alpha];
        let [s, h] = [self.half, self.height];
        let apex = point(0.0, 0.0, h);
        let sides = [[[-s, -s], [s, -s]], [[-s, -s], [-s, s]], [[-s, s], [s, s]], [[s, s], [s, -s]]];
        let mut faces = [MeshVertex { position: apex, uv: [0.0; 2], tint }; 12];
        for (face, [[ax, ay], [cx, cy]]) in sides.into_iter().enumerate() {
            let u = face as f32 * 0.2;
            faces[face * 3] = MeshVertex { position: point(ax, ay, 0.0), uv: [u, 1.0], tint };
            faces[face * 3 + 1].uv = [u, 0.0];
            faces[face * 3 + 2] = MeshVertex { position: point(cx, cy, 0.0), uv: [u + 0.2, 1.0], tint };
        }
        faces
    }
}

/// Grimtooth's train: from where the caster stood, a point walks toward where the target stood and on past it.
struct Train {
    start: Instant,
    /// Ticks run.
    ticks: u32,
    from: Vector,
    /// The way it walks across the ground.
    way: [f32; 2],
    /// Stops after its first spike (the caster stood on the target).
    short: bool,
}

/// The spikes standing and the trains leaving them.
#[derive(Default)]
pub struct Stone {
    trains: Vec<Train>,
    spikes: Vec<Spike>,
}

impl Stone {
    /// Earth Spike at `at` from `now`: a centre spike that rises and settles back a little, never fading, and six
    /// leaning outer ones round it that push up a little and fade over their last 10 ticks. All stand 240 ticks.
    pub fn earth_spike(&mut self, at: Vector, now: Instant) {
        self.spike_field(at, now, STONE);
    }

    /// Spear of Ice at `at` from `now`: Earth Spike's seven spikes in ice.
    pub fn ice_spike(&mut self, at: Vector, now: Instant) {
        self.spike_field(at, now, ICE_TEXTURE);
    }

    fn spike_field(&mut self, at: Vector, now: Instant, texture: &'static str) {
        // ponytail: the spikes stand where the target stood; whether the game's follow it isn't known.
        let left = EARTH_SPIKE_TICKS - 1;
        let size = [3.0 + below(50) / 100.0, 18.0];
        let centre = Spike::new(now, at, 10.0, [below(360), 80.0 + below(20)], size, 240, left);
        self.spikes.push(Spike { texture, speed: 1.0, change: 0.01, turn: Some((12, -1.2)), stop: 15, sink: Some((0.0, 0.0)), ..centre });
        // Each at a random way out, its lean in a widening random range.
        for k in 1..=6 {
            let (sin, cos) = below(360).to_radians().sin_cos();
            let at = [at[0] + 3.5 * sin, at[1], at[2] - 3.5 * cos];
            let yaw = ((60 * (k - 1)) as f32 + below(60 * k)).min(360.0);
            let outer = Spike::new(now, at, 20.0, [yaw, 100.0], [4.0 + below(100) / 100.0, 20.0], 240, left);
            self.spikes.push(Spike { texture, speed: 1.5, stop: 2, alpha: 250.0, ..outer });
        }
    }

    /// Gravitation Field at `at` from `now`: 100 faint spikes on a 5 by 5 field, four a cell (two stone, two ice) 10 under the ground,
    /// bobbing together for 250 ticks; those of a skill unit `unit` go with it.
    pub fn gravitation(&mut self, at: Vector, now: Instant, unit: Option<u32>) {
        for z in [-10.0, -5.0, 0.0, 5.0, 10.0] {
            for x in [-10.0, -5.0, 0.0, 5.0, 10.0] {
                for i in 0..4 {
                    // Two draws thrown away, then the size, the yaw and the pitch.
                    let _ = (below(360), below(2));
                    let half = (300.0 + below(50)) / 100.0;
                    let angles = [below(360), below(40) + 60.0];
                    let spike = Spike::new(now, [at[0] + x, at[1], at[2] + z], 10.0, angles, [half, 18.0], 250, 249);
                    let texture = if i < 2 { STONE } else { ICE_TEXTURE };
                    self.spikes.push(Spike { texture, speed: 1.0, change: 0.01, stop: u32::MAX, cycle: true, alpha: 20.0, fade: 240, unit, ..spike });
                }
            }
        }
    }

    /// Heaven's Drive at `at` from `now`: 25 spikes on a 5 by 5 grid a cell apart, rising for 11 ticks and settling
    /// back a little, standing unfaded and sinking into the ground over their last 30 ticks of 290 to 319 (the effect
    /// ends at 299).
    pub fn heavens_drive(&mut self, at: Vector, now: Instant) {
        // ponytail: the effect lives its 300 ticks; whether the game's launcher (260 ticks) cuts it short isn't known.
        for x in [-10.0, -5.0, 0.0, 5.0, 10.0] {
            for z in [-10.0, -5.0, 0.0, 5.0, 10.0] {
                let size = [3.0 + below(50) / 100.0, 9.0 + (below(60) + 1.0) / 10.0];
                let life = HEAVENS_DRIVE_TICKS - 10 + pick(0, 29);
                let spike = Spike::new(now, [at[0] + x, at[1], at[2] + z], 10.0, [below(360), 80.0 + below(20)], size, life, HEAVENS_DRIVE_TICKS - 1);
                self.spikes.push(Spike { speed: 1.0, change: 0.01, turn: Some((11, -1.2)), stop: 14, sink: Some((-1.0, -0.01)), ..spike });
            }
        }
    }

    /// Grimtooth from `from` toward `to` from `now`: a spike every 3rd tick along the way, 6 units apart, for the
    /// effect's whole 250 ticks, on past the target; just one when the caster stands within 2.5 units of it.
    pub fn grimtooth(&mut self, from: Vector, to: Vector, now: Instant) {
        // ponytail: running about 500 units past the target is how the game's code reads, not seen in it; straight at
        // the target, where the game's turn truncates to whole degrees, 1–2° off.
        let [dx, dz] = [to[0] - from[0], to[2] - from[2]];
        let distance = f32::hypot(dx, dz);
        let way = if distance > 0.0 { [dx / distance, dz / distance] } else { [0.0, 1.0] };
        self.trains.push(Train { start: now, ticks: 0, from, way, short: distance <= TRAIN_STOP });
    }

    /// Frost Diver (EF_FROSTDIVER) from `from` at `to` from `now`: one ice spike a tick in a line toward the target,
    /// its random choices from `rand(n)` (0 to n − 1).
    pub fn frost_diver(&mut self, from: Vector, to: Vector, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        // Along the whole degree before the bearing's (ψ), 1° to 2° off the line toward smaller bearings; 0 below 1°.
        let heading = crate::actor::step_rot(to[0] - from[0], to[2] - from[2]).trunc();
        let (sin, cos) = (if heading == 0.0 { 0.0 } else { heading - 1.0 }).to_radians().sin_cos();
        let way = [sin, -cos];
        for tick in 0..FROST_TICKS {
            let out = FROST_FIRST + FROST_STEP * tick as f32;
            let at = [from[0] + way[0] * out, from[1], from[2] + way[1] * out];
            let [turn, around, half, height, yaw, pitch] = [rand(360), rand(10), rand(40), rand(30), rand(360), rand(30)];
            let (around, half, height) = ((around + 5) as f32 / 10.0, (half + 60) as f32 / 100.0, (height + 150) as f32 / 10.0);
            let angles = [yaw as f32, (pitch + 75) as f32];
            self.spikes.push(Spike::ice(now + TICK * tick, at, around, turn as f32, [half, height], angles, FROST_TICKS - tick - 1, 254.0));
            if f32::hypot(to[0] - at[0], to[2] - at[2]) <= FROST_REACH {
                break;
            }
        }
    }

    /// Frost Diver's second hit effect (EF_FROSTDIVER2) at `at` from `now`: eight bigger ice spikes round the target at
    /// once, 200 alpha.
    pub fn frost_hit(&mut self, at: Vector, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        for _ in 0..8 {
            let [turn, around, half, height, yaw, pitch] = [rand(360), rand(4), rand(250), rand(100), rand(360), rand(20)];
            let (around, half, height) = ((around + 1) as f32 / 10.0, (half + 100) as f32 / 100.0, (height + 200) as f32 / 10.0);
            let angles = [yaw as f32, (pitch + 80) as f32];
            self.spikes.push(Spike::ice(now, at, around, turn as f32, [half, height], angles, FROST_LIFE, 200.0));
        }
    }

    /// Ice Wall's cell (EF_ICEWALL) for ground unit `unit` at `at` from `now`: three ice spikes at i = −2, 0 and 2, each
    /// −i·cos r along z from the cell's centre, 30 units under it, rising 28.35 in 20 ticks and standing with the unit.
    /// The draws, per spike: r (360, a different one for each), one unused (40), height (100), yaw (360), pitch (6).
    pub fn ice_wall(&mut self, unit: u32, at: Vector, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        // ponytail: the packet's own 74 at the wall's centre cell draws nothing; the units' cells show the wall.
        let mut turns = Vec::new();
        for i in [-2.0_f32, 0.0, 2.0] {
            let mut turn = rand(360);
            // The game's three differ; how it keeps them apart isn't known: draw again.
            for _ in 0..16 {
                if !turns.contains(&turn) {
                    break;
                }
                turn = rand(360);
            }
            turns.push(turn);
            let _unused = rand(40);
            let [height, yaw, pitch] = [rand(100), rand(360), rand(6)];
            let spot = [at[0], at[1], at[2] - i * (turn as f32).to_radians().cos()];
            let spike = Spike::new(
                now,
                spot,
                FROST_DEPTH,
                [yaw as f32, (pitch + 87) as f32],
                [ICE_WALL_HALF, (height + 300) as f32 / 10.0],
                ICE_WALL_TICKS,
                ICE_WALL_TICKS - 1,
            );
            self.spikes.push(Spike {
                texture: ICE_TEXTURE,
                speed: 3.0,
                change: -0.15,
                stop: 20,
                alpha: ICE_WALL_ALPHA,
                fade: u32::MAX,
                unit: Some(unit),
                ..spike
            });
        }
    }

    /// Takes down what ground unit `unit` stood with.
    pub fn forget(&mut self, unit: u32) {
        self.spikes.retain(|spike| spike.unit != Some(unit));
    }

    /// Storm Gust's ice crystals (EF_STORMGUST) round `at` from `now`: two at each of the effect's ticks 58, 87, 116 and 145,
    /// each an open pyramid 20 tall and pitched 100° rising 1.5 a tick for four ticks from 20 under the ground, standing until
    /// the effect's tick 215. Its choices from `rand(n)`: yaw, distance, bearing and half-width, per crystal.
    pub fn storm_gust(&mut self, at: Vector, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        for tick in [58u32, 87, 116, 145] {
            for _ in 0..2 {
                let yaw = rand(360) as f32;
                let distance = 3.0 + rand(25) as f32;
                let bearing = rand(360) as f32;
                let half = (400 + rand(100)) as f32 / 100.0;
                let (sin, cos) = bearing.to_radians().sin_cos();
                let spot = [at[0] + distance * sin, at[1], at[2] - distance * cos];
                let life = STORM_GUST_END - tick;
                let spike = Spike::new(now + TICK * tick, spot, STORM_GUST_DEPTH, [yaw, 100.0], [half, STORM_GUST_HEIGHT], life, life);
                self.spikes.push(Spike { texture: ICE_TEXTURE, speed: 1.5, stop: 3, alpha: 250.0, ..spike });
            }
        }
    }

    /// Stops the lines of spikes still to come (Frost Diver's, under /mineffect): the ones not yet born.
    pub fn stop_lines(&mut self, now: Instant) {
        self.spikes.retain(|spike| spike.born <= now);
    }

    /// Runs the trains and the spikes up to `now`, a tick at a time.
    pub fn advance(&mut self, now: Instant) {
        for train in &mut self.trains {
            let due = passes(train.start, now).min(TRAIN_TICKS);
            for age in train.ticks..due {
                if age % TRAIN_EVERY != 0 || (train.short && age > 0) {
                    continue;
                }
                // The point has walked a step more than the ticks run.
                let out = TRAIN_STEP * (age + 1) as f32;
                let at = [train.from[0] + train.way[0] * out, train.from[1], train.from[2] + train.way[1] * out];
                let size = [0.6 + below(40) / 100.0, 10.0];
                let spike = Spike::new(train.start + TICK * age, at, 20.0, [below(360), 75.0 + below(30)], size, 40, TRAIN_TICKS - age - 1);
                self.spikes.push(Spike { speed: 3.0, change: -0.15, stop: 6, ..spike });
            }
            train.ticks = train.ticks.max(due);
        }
        self.trains.retain(|train| train.ticks < TRAIN_TICKS && !(train.short && train.ticks > 0));
        for spike in &mut self.spikes {
            while spike.age < passes(spike.born, now).min(spike.ends) {
                spike.step();
            }
        }
        self.spikes.retain(|spike| passes(spike.born, now) <= spike.ends);
    }

    /// The triangles of every spike showing in `texture`.
    pub fn mesh(&self, texture: &str) -> Vec<MeshVertex> {
        self.spikes.iter().filter(|spike| spike.age > 0 && spike.texture == texture).flat_map(Spike::faces).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn earth_spike_stands_seven_spikes_240_ticks() {
        let start = Instant::now();
        let mut spikes = Stone::default();
        spikes.earth_spike([0.0; 3], start);
        spikes.advance(start);
        assert_eq!(spikes.spikes.len(), 7);
        assert_eq!(spikes.mesh(STONE).len(), 7 * 12);
        // The centre rises 12 ticks (1.01 to 1.12 a tick), settles 1.2 a tick for 4 and stands, never fading.
        spikes.advance(start + TICK * 30);
        let centre = &spikes.spikes[0];
        assert!((centre.rise - (12.78 - 4.8)).abs() < 1e-3, "{}", centre.rise);
        assert_eq!(centre.alpha, 254.0);
        // The outer ones push up 1.5 a tick for 3 ticks, then fade over their last 10.
        let outer = &spikes.spikes[1];
        assert!((outer.rise - 4.5).abs() < 1e-4 && outer.alpha == 250.0);
        for k in 1..=6 {
            let pitch = spikes.spikes[k].axes[2][1].asin().to_degrees();
            assert!((pitch - 80.0).abs() < 1e-3, "outer {k} leans 10°: {pitch}");
        }
        spikes.advance(start + TICK * 235);
        assert!((spikes.spikes[1].alpha - 250.0 * 4.0 / 10.0).abs() < 1e-3, "{}", spikes.spikes[1].alpha);
        spikes.advance(start + TICK * 239);
        assert_eq!(spikes.spikes.len(), 7);
        spikes.advance(start + TICK * 240);
        assert!(spikes.spikes.is_empty());
    }

    #[test]
    fn spear_of_ice_stands_earth_spikes_seven_spikes_in_ice() {
        let start = Instant::now();
        let mut spikes = Stone::default();
        spikes.ice_spike([1.0, 2.0, 3.0], start);
        spikes.advance(start);
        assert_eq!(spikes.spikes.len(), 7);
        assert!(spikes.spikes.iter().all(|spike| spike.texture == ICE_TEXTURE));
        assert_eq!((spikes.mesh(ICE_TEXTURE).len(), spikes.mesh(STONE).len()), (7 * 12, 0));
        // The same rise, settling and fade as Earth Spike's: the centre at 7.98 up, the stubs 4.5, ending at tick 240.
        spikes.advance(start + TICK * 30);
        assert!((spikes.spikes[0].rise - (12.78 - 4.8)).abs() < 1e-3 && (spikes.spikes[1].rise - 4.5).abs() < 1e-4);
        assert_eq!(spikes.spikes[0].alpha, 254.0);
        spikes.advance(start + TICK * 240);
        assert!(spikes.spikes.is_empty());
        // Earth Spike's stay in stone.
        let mut earth = Stone::default();
        earth.earth_spike([0.0; 3], start);
        assert!(earth.spikes.iter().all(|spike| spike.texture == STONE));
    }

    #[test]
    fn heavens_drive_raises_25_spikes_and_sinks_them() {
        let start = Instant::now();
        let mut spikes = Stone::default();
        spikes.heavens_drive([0.0; 3], start);
        spikes.advance(start + TICK * 20);
        assert_eq!(spikes.spikes.len(), 25);
        // A cell apart, x and z from −10 to 10.
        let corners: Vec<[f32; 2]> = spikes.spikes.iter().map(|spike| [spike.base[0], spike.base[2]]).collect();
        assert_eq!((corners[0], corners[1], corners[24]), ([-10.0, -10.0], [-10.0, -5.0], [10.0, 10.0]));
        // Up 11 ticks (1.01 to 1.11), back 1.2 a tick for 4 (stopped at 14, 176 ms in), then standing.
        for spike in &spikes.spikes {
            assert!((spike.rise - (11.66 - 4.8)).abs() < 1e-3, "{}", spike.rise);
            assert!((290..320).contains(&spike.life) && (9.1..=15.0).contains(&spike.height));
        }
        // 30 ticks before its end each starts to sink, faster each tick; the last go with the effect at 299 ticks.
        let life = spikes.spikes[0].life;
        spikes.advance(start + TICK * (life - 30));
        let sinking = spikes.spikes[0].rise;
        spikes.advance(start + TICK * (life - 28));
        assert!((spikes.spikes[0].rise - (sinking - 1.01 - 1.02)).abs() < 1e-3);
        spikes.advance(start + TICK * 298);
        assert!(spikes.spikes.iter().all(|spike| spike.life >= 299));
        spikes.advance(start + TICK * 299);
        assert!(spikes.spikes.is_empty());
    }

    #[test]
    fn frost_diver_lays_spikes_to_the_target() {
        // A target 45 units off: spikes 5, 7, … 43 units out, 20 of them, one a tick.
        let start = Instant::now();
        let mut spikes = Stone::default();
        spikes.frost_diver([0.0; 3], [0.0, 0.0, 45.0], start, |_| 0);
        assert_eq!(spikes.spikes.len(), 20);
        // (each a little off its point: half a unit the way its random turn points)
        // (along 179°, 1° off the line: the last, 43 out, lies 0.75 east of it)
        assert_eq!(spikes.spikes[1].born - start, TICK);
        assert!((spikes.spikes[19].base[0] - 0.75).abs() < 0.01 && (spikes.spikes[19].base[2] - 42.5).abs() < 0.05, "{:?}", spikes.spikes[19].base);
        // A spike rises 2.85 the first tick and settles 28.35 up after tick 20, 30 under where it stands at first.
        spikes.advance(start);
        assert!((spikes.spikes[0].rise - 2.85).abs() < 1e-4, "{}", spikes.spikes[0].rise);
        assert_eq!(spikes.spikes[0].base[1], -FROST_DEPTH);
        spikes.advance(start + TICK * 5);
        assert_eq!((spikes.mesh(ICE_TEXTURE).len(), spikes.mesh(STONE).len()), (6 * 12, 0));
        spikes.advance(start + TICK * 20);
        let settled = spikes.spikes[0].rise;
        assert!((settled - 28.35).abs() < 1e-4, "{settled}");
        // Straight up at pitch 90; four faces of three corners; fading over its last 10 ticks, from tick 30.
        let up = axes(0.0, 90.0)[2];
        assert!((up[1] - 1.0).abs() < 1e-6 && up[0].abs() < 1e-6 && up[2].abs() < 1e-6);
        assert_eq!(spikes.spikes[0].faces().len(), 12);
        spikes.advance(start + TICK * 29);
        assert_eq!(spikes.spikes[0].alpha, 254.0);
        spikes.advance(start + TICK * 35);
        assert!((spikes.spikes[0].alpha - 254.0 * 4.0 / 10.0).abs() < 1e-3, "{}", spikes.spikes[0].alpha);
        spikes.advance(start + TICK * 39);
        assert_eq!(spikes.spikes[0].rise, settled);
        // Its hit: eight spikes at once, 200 alpha.
        spikes.frost_hit([0.0; 3], start, |_| 0);
        assert_eq!(spikes.spikes.len(), 28);
        assert_eq!(spikes.spikes[27].alpha, 200.0);
        spikes.advance(start + TICK * 100);
        assert!(spikes.spikes.is_empty());
    }

    #[test]
    fn ice_wall_raises_three_spikes_a_cell() {
        let start = Instant::now();
        let mut spikes = Stone::default();
        // r 0, 90 and 180 degrees (cos 1, 0, −1), the unused draw 39, heights 300 + 0, 50 and 99, pitches 87 to 92.
        let mut draws = [0, 39, 0, 10, 0, 90, 39, 50, 20, 3, 180, 39, 99, 30, 5].into_iter();
        spikes.ice_wall(7, [10.0, 0.0, 20.0], start, |_| draws.next().unwrap());
        assert_eq!(spikes.spikes.len(), 3);
        // Offsets −i·cos r along z from the cell's centre: i = −2 gives +2·1, 0 gives 0, 2 gives −2·−1 = +2.
        let spots: Vec<[f32; 2]> = spikes.spikes.iter().map(|spike| [spike.base[0], spike.base[2]]).collect();
        assert!(spots[0] == [10.0, 22.0] && (spots[1][1] - 20.0).abs() < 1e-3 && (spots[2][1] - 22.0).abs() < 1e-3, "{spots:?}");
        // The half-base is 2.5 whatever the unused draw gives; height (R + 300) / 10; 30 under the cell, 200 alpha.
        for (spike, height) in spikes.spikes.iter().zip([30.0, 35.0, 39.9]) {
            assert_eq!((spike.half, spike.base[1], spike.alpha, spike.texture), (2.5, -FROST_DEPTH, 200.0, ICE_TEXTURE));
            assert!((spike.height - height).abs() < 1e-3);
        }
        // Rising 2.85 the first tick, 28.5 at tick 19, 28.35 from tick 20, standing on.
        spikes.advance(start);
        assert!((spikes.spikes[0].rise - 2.85).abs() < 1e-4);
        spikes.advance(start + TICK * 19);
        assert!((spikes.spikes[0].rise - 28.5).abs() < 1e-3, "{}", spikes.spikes[0].rise);
        spikes.advance(start + TICK * 20);
        assert!((spikes.spikes[0].rise - 28.35).abs() < 1e-3);
        spikes.advance(start + TICK * 2000);
        assert!((spikes.spikes[0].rise - 28.35).abs() < 1e-3 && spikes.spikes[0].alpha == 200.0);
        assert_eq!(spikes.mesh(ICE_TEXTURE).len(), 3 * 12);
        // They go with their unit, and no other.
        spikes.ice_wall(8, [0.0; 3], start, |n| n / 3);
        assert_eq!(spikes.spikes.len(), 6);
        spikes.forget(7);
        assert_eq!(spikes.spikes.len(), 3);
        assert!(spikes.spikes.iter().all(|spike| spike.unit == Some(8)));
    }

    #[test]
    fn a_frost_diver_line_stops_under_mineffect() {
        let start = Instant::now();
        let mut spikes = Stone::default();
        spikes.frost_diver([0.0; 3], [0.0, 0.0, 45.0], start, |_| 0);
        assert_eq!(spikes.spikes.len(), 20);
        // Turned on three ticks in, the line stops: the spikes already born stand, the rest never come.
        spikes.stop_lines(start + TICK * 3);
        assert_eq!(spikes.spikes.len(), 4);
        spikes.frost_hit([0.0; 3], start + TICK * 3, |_| 0);
        spikes.stop_lines(start + TICK * 3);
        assert_eq!(spikes.spikes.len(), 12, "the hit's own spikes aren't a line's");
    }

    #[test]
    fn storm_gust_pops_pairs_of_ice_crystals() {
        let start = Instant::now();
        let mut spikes = Stone::default();
        // Every draw 0: yaw 0, distance 3, bearing 0, half-width 4.0.
        spikes.storm_gust([10.0, 0.0, 20.0], start, |_| 0);
        assert_eq!(spikes.spikes.len(), 8);
        assert_eq!(spikes.spikes.iter().map(|spike| (spike.born - start).as_millis() / 16).collect::<Vec<_>>(), [58, 58, 87, 87, 116, 116, 145, 145]);
        let spike = &spikes.spikes[0];
        assert_eq!((spike.texture, spike.half, spike.height), (ICE_TEXTURE, 4.0, 20.0));
        // The base 3 from the cell on bearing 0 (toward −z), 20 under the ground.
        assert!((spike.base[0] - 10.0).abs() < 1e-4 && (spike.base[2] - 17.0).abs() < 1e-4 && spike.base[1] == -20.0);
        // Lives 157, 128, 99, 70 ticks; alpha 250 until 10 before its end.
        assert_eq!([0, 2, 4, 6].map(|index| spikes.spikes[index].life), [157, 128, 99, 70]);
        // The axis for a yaw: pitch 100°.
        let axis = spike.axes[2];
        assert!((axis[1] - 0.9848).abs() < 1e-3 && (axis[0]).abs() < 1e-3 && (axis[2] + 0.1736).abs() < 1e-3);
        let turned = axes(90.0, 100.0)[2];
        assert!((turned[0] + 0.1736).abs() < 1e-3 && turned[2].abs() < 1e-3);
        // It rises 1.5 on each of its first four ticks (6.0), then stays, and the ground hides most of it.
        spikes.advance(start + TICK * 58);
        assert!((spikes.spikes[0].rise - 1.5).abs() < 1e-4);
        spikes.advance(start + TICK * 70);
        assert!((spikes.spikes[0].rise - 6.0).abs() < 1e-4);
        // The alpha 250 to age 147, 225 … 0 at its last ticks; gone by tick 215.
        spikes.advance(start + TICK * (58 + 146));
        assert_eq!(spikes.spikes[0].alpha, 250.0);
        spikes.advance(start + TICK * (58 + 147));
        assert!((spikes.spikes[0].alpha - 225.0).abs() < 1e-3, "{}", spikes.spikes[0].alpha);
        spikes.advance(start + TICK * 215);
        assert!(spikes.spikes.is_empty());
    }

    #[test]
    fn grimtooth_walks_a_spike_train_past_the_target() {
        let start = Instant::now();
        let mut spikes = Stone::default();
        spikes.grimtooth([0.0; 3], [0.0, 0.0, 30.0], start);
        // Spikes at ticks 0, 3, 6: 2, 8 and 14 units out, 20 under the ground, rising 6 ticks to 16.8.
        spikes.advance(start + TICK * 6);
        let out: Vec<f32> = spikes.spikes.iter().map(|spike| spike.base[2]).collect();
        assert_eq!(out, [2.0, 8.0, 14.0]);
        assert_eq!(spikes.spikes[0].base[1], -20.0);
        assert!((spikes.spikes[0].rise - 16.8).abs() < 1e-3, "{}", spikes.spikes[0].rise);
        // Each lives 40 ticks, fading over its last 10.
        spikes.advance(start + TICK * 35);
        assert!((spikes.spikes[0].alpha - 254.0 * 4.0 / 10.0).abs() < 1e-3, "{}", spikes.spikes[0].alpha);
        // It walks the effect's whole 250 ticks, on past the target: the spikes made in its last 40 fade out with it,
        // the last drawn 494 units out, 3 ticks; the one made on its last tick never shows.
        spikes.advance(start + TICK * 248);
        assert_eq!(spikes.spikes.len(), 13);
        let last = spikes.spikes.last().unwrap();
        assert_eq!((last.base[2], last.ends), (494.0, 3));
        assert!(last.alpha < 254.0 * 2.0 / 3.0 + 1e-3, "{}", last.alpha);
        spikes.advance(start + TICK * 249);
        assert!(spikes.spikes.is_empty() && spikes.trains.is_empty());
        // A caster on the target: one spike.
        spikes.grimtooth([0.0; 3], [0.0, 0.0, 2.0], start);
        spikes.advance(start + TICK * 30);
        assert_eq!(spikes.spikes.len(), 1);
    }

    #[test]
    fn gravitation_field_bobs_100_faint_spikes() {
        let start = Instant::now();
        let mut spikes = Stone::default();
        spikes.gravitation([0.0, 2.0, 0.0], start, None);
        spikes.gravitation([50.0, 2.0, 0.0], start, Some(9));
        assert_eq!(spikes.spikes.len(), 200);
        // 4 per cell over 25 cells: stone for i = 0, 1 and ice for 2, 3.
        let first: Vec<&Spike> = spikes.spikes[..100].iter().collect();
        assert_eq!(first.iter().filter(|spike| spike.texture == STONE).count(), 50);
        assert_eq!(first.iter().filter(|spike| spike.texture == ICE_TEXTURE).count(), 50);
        assert!(first[0].alpha == 20.0 && first[0].half >= 3.0 && first[0].half < 3.5 && first[0].height == 18.0);
        // The travel after ages 0…7: 1.01, 2.03, 0.83, −0.37, −1.57, −0.38, 0.82, 2.03.
        let mut spike = Spike { ..spikes.spikes.remove(0) };
        let mut travel = Vec::new();
        for _ in 0..8 {
            spike.step();
            travel.push((spike.rise * 100.0).round() / 100.0);
        }
        assert_eq!(travel, [1.01, 2.03, 0.83, -0.37, -1.57, -0.38, 0.82, 2.03]);
        // Alpha 20 until age 240, 18 after it, 0 at 249; the base is 10 under the ground.
        let mut fading = Spike::new(start, [0.0; 3], 10.0, [0.0, 90.0], [3.0, 18.0], 250, 249);
        fading.alpha = 20.0;
        fading.fade = 240;
        for _ in 0..240 {
            fading.step();
        }
        assert_eq!(fading.alpha, 20.0);
        fading.step();
        assert!((fading.alpha - 18.0).abs() < 1e-4);
        for _ in 0..9 {
            fading.step();
        }
        assert!(fading.alpha.abs() < 1e-3);
        assert_eq!(fading.base[1], -10.0);
        // The unit's copies go with the unit; the packet's run their ticks.
        spikes.forget(9);
        assert_eq!(spikes.spikes.len(), 99);
        spikes.advance(start + TICK * 300);
        assert!(spikes.spikes.is_empty());
    }
}
