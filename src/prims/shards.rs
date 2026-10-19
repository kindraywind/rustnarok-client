//! Intimidate's shards (EF_INTIMIDATE): 80 white triangles in four bands of twenty, each waiting 70 to 99 ticks, then
//! growing out from the spot the caster stood on, spinning, and fading. They stay in the world.

use std::time::Instant;

use super::passes;
use crate::world::{MeshVertex, Vector};

pub const TEXTURE: &str = "data\\texture\\effect\\white01.bmp";
/// 20 prims of 4 bands.
const BANDS: usize = 80;
/// Half the diagonal of a shard, and where the bands start out and how far they grow a tick.
const SIZE: f32 = 3.5;
const START_RADIUS: f32 = 12.0;
const GROWTH: f32 = 1.5;
/// The longest wait is 99 ticks (70 + 29) and a band is shown for 46 after it: nothing shows after tick 144.
const LAST_TICK: u32 = 144;
/// The shards' colour, #6496FF.
const COLOUR: [f32; 3] = [100.0 / 255.0, 150.0 / 255.0, 1.0];

/// One band's choices, all in whole degrees but the wait (ticks).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Band {
    wait: u32,
    azimuth: f32,
    elevation: f32,
    turn: f32,
    spin: f32,
}

struct Burst {
    origin: Vector,
    start: Instant,
    bands: Vec<Band>,
}

#[derive(Default)]
pub struct Shards {
    list: Vec<Burst>,
}

/// A band's age n after the tick t (shown for 1 to 46): its first tick is 70 + wait.
fn age(tick: u32, wait: u32) -> Option<u32> {
    tick.checked_sub(69 + wait).filter(|n| (1..=46).contains(n))
}

/// A band's alpha (out of 255) at age n: 15 a tick to 150, held to 25, then 7 less a tick.
pub fn alpha(n: u32) -> f32 {
    match n {
        1..=10 => 15.0 * n as f32,
        11..=25 => 150.0,
        26..=46 => 150.0 - 7.0 * (n - 25) as f32,
        _ => 0.0,
    }
}

/// How far a band is from the centre at age n.
pub fn radius(n: u32) -> f32 {
    START_RADIUS + GROWTH * n as f32
}

impl Band {
    /// Its six vertices over `origin` at tick `tick` of its burst, once it shows.
    fn mesh(&self, origin: Vector, tick: u32) -> Option<Vec<MeshVertex>> {
        let n = age(tick, self.wait)?;
        let r = radius(n);
        let (x1, y1) = (r * self.elevation.to_radians().cos(), r * self.elevation.to_radians().sin());
        let (sin, cos) = self.azimuth.to_radians().sin_cos();
        let centre = [origin[0] + x1 * cos, origin[1] - y1, origin[2] + x1 * sin];
        // Spinning 5° a tick from the first tick, the waiting ones too.
        let (spin_sin, spin_cos) = ((self.spin + 5.0 * (tick + 1) as f32) % 360.0).to_radians().sin_cos();
        let tint = [COLOUR[0], COLOUR[1], COLOUR[2], alpha(n) / 255.0];
        let corner = |j: u32, uv: [f32; 2]| {
            let (sin, cos) = (self.turn + 90.0 * j as f32).to_radians().sin_cos();
            let (across, up) = (SIZE * cos, -SIZE * sin);
            MeshVertex { position: [centre[0] + across * spin_cos, centre[1] + up, centre[2] + across * spin_sin], uv, tint }
        };
        let [a, b, c, d] = [corner(0, [0.0, 1.0]), corner(1, [1.0, 1.0]), corner(2, [1.0, 0.0]), corner(3, [0.0, 0.0])];
        Some(vec![a, b, c, d, c, a])
    }
}

impl Shards {
    /// Intimidate's burst round `origin`, its choices from `rand(n)` (0 to n − 1): per band the wait, then the azimuth,
    /// the elevation, the turn and the first spin.
    pub fn burst(&mut self, origin: Vector, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        let bands = (0..BANDS)
            .map(|_| {
                let wait = rand(30);
                let [azimuth, elevation, turn, spin] = [(); 4].map(|()| rand(360) as f32);
                Band { wait, azimuth, elevation, turn, spin }
            })
            .collect();
        self.list.push(Burst { origin, start: now, bands });
    }

    pub fn advance(&mut self, now: Instant) {
        self.list.retain(|burst| passes(burst.start, now) <= LAST_TICK + 1);
    }

    /// Every shard showing at `now`.
    pub fn mesh(&self, now: Instant) -> Vec<MeshVertex> {
        let mut vertices = Vec::new();
        for burst in &self.list {
            let Some(tick) = passes(burst.start, now).checked_sub(1) else { continue };
            vertices.extend(burst.bands.iter().filter_map(|band| band.mesh(burst.origin, tick)).flatten());
        }
        vertices
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prims::TICK;

    fn near(got: f32, want: f32) -> bool {
        (got - want).abs() < 0.01
    }

    fn burst(draws: impl FnMut(u32) -> u32) -> (Shards, Instant) {
        let start = Instant::now();
        let mut shards = Shards::default();
        shards.burst([10.0, 2.0, 20.0], start, draws);
        (shards, start)
    }

    #[test]
    fn eighty_shards_wait_then_grow_and_fade() {
        // Every draw 0: wait 0, so a band shows on ticks 70…115 (n = 1…46); hidden at 69.
        let (shards, start) = burst(|_| 0);
        assert_eq!(shards.list[0].bands.len(), 80);
        let at = |tick: u32| shards.mesh(start + TICK * tick);
        assert!(at(0).is_empty() && at(69).is_empty());
        assert_eq!(at(70).len(), 80 * 6);
        // Alpha 15 at n = 1, 150 at 10 and 25, 143 at 26, 3 at 46; the radius 13.5 and 81.
        let alpha_at = |tick: u32| (at(tick)[0].tint[3] * 255.0).round();
        assert_eq!([70, 79, 94, 95, 115].map(alpha_at), [15.0, 150.0, 150.0, 143.0, 3.0]);
        assert!(near(radius(1), 13.5) && near(radius(46), 81.0));
        assert!(at(116).is_empty());
        // A wait of 29 shows on ticks 99…144 and the burst is gone after.
        let (late, late_start) = burst(|n| if n == 30 { 29 } else { 0 });
        assert!(late.mesh(late_start + TICK * 98).is_empty());
        assert_eq!(late.mesh(late_start + TICK * 99).len(), 80 * 6);
        assert_eq!(late.mesh(late_start + TICK * 144).len(), 80 * 6);
        assert!(late.mesh(late_start + TICK * 145).is_empty());
        let mut late = late;
        late.advance(late_start + TICK * 146);
        assert!(late.list.is_empty());
    }

    #[test]
    fn shards_spin_five_degrees_a_tick_and_turn_in_their_plane() {
        // Draws 0 except the azimuth, 0: a shard at turn 0 has corners (+3.5, 0), (0, +3.5) …, horizontal along (cos a3, sin a3)
        // and, up being minus the game's y, (0, −3.5) for corner 1.
        let (shards, start) = burst(|_| 0);
        let vertices = shards.mesh(start + TICK * 70);
        // a3 = 5 (70 + 1) = 355°: the horizontal offset of corner 0 is 3.5 along (cos 355°, sin 355°).
        let centre = [10.0 + radius(1), 2.0, 20.0];
        let offset = |vertex: &MeshVertex| [vertex.position[0] - centre[0], vertex.position[1] - centre[1], vertex.position[2] - centre[2]];
        let corner0 = offset(&vertices[0]);
        assert!(near(corner0[0], 3.5 * 355.0_f32.to_radians().cos()) && near(corner0[2], 3.5 * 355.0_f32.to_radians().sin()) && near(corner0[1], 0.0));
        let corner1 = offset(&vertices[1]);
        assert!(near(corner1[0], 0.0) && near(corner1[2], 0.0) && near(corner1[1], -3.5));
        let corner2 = offset(&vertices[2]);
        assert!(near(corner2[0], -3.5 * 355.0_f32.to_radians().cos()) && near(corner2[1], 0.0));
        // uv (0, 1), (1, 1), (1, 0), (0, 0); triangles (0, 1, 2) and (3, 2, 0).
        assert_eq!([0, 1, 2, 3, 4, 5].map(|k| vertices[k].uv), [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]);
        // The spin goes on in the waiting ticks: after tick 0, spin0 + 5.
        let mut draws = [0, 0, 0, 0, 90].into_iter().cycle();
        let (turned, turned_start) = burst(|_| draws.next().unwrap());
        assert_eq!(turned.list[0].bands[0].spin, 90.0);
        let spun = turned.list[0].bands[0];
        assert!(near((spun.spin + 5.0 * 1.0) % 360.0, 95.0));
        // A band over the spot's height goes up with the elevation: below the horizon, under the ground.
        let mut tilted = Shards::default();
        let mut tilted_draws = [0, 0, 90, 0, 0].into_iter().cycle();
        tilted.burst([0.0; 3], turned_start, |_| tilted_draws.next().unwrap());
        let up = tilted.mesh(turned_start + TICK * 70);
        assert!(up.iter().all(|vertex| vertex.position[1] < 0.0), "elevation 90 is r below, the game's y down being up here");
    }
}
