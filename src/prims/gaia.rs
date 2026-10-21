//! Power of Gaia's nine boxes of yellow flame walls (1088) over its skill unit, a 16 ms tick at a time.
//!
//! A square's age counts from 0 on the tick its copy is made, before that tick's step.

use std::time::Instant;

use super::TICK;
use super::walls::{RING_YELLOW, Wall};
use crate::world::{MeshVertex, Vector};

pub const TEXTURE: &str = RING_YELLOW;
/// A new copy every 1 999 ticks while the unit stands; a copy updates and draws only within 130 of the player.
const PERIOD: u32 = 1999;
const NEAR: f32 = 130.0;
/// The squares' height, alpha, size (a corner's distance from its cell's middle) and the ages they rise in and fade out at.
const HEIGHT: f32 = 16.0;
const ALPHA: f32 = 120.0;
const SIZE: f32 = 3.5;
const RISE: u32 = 90;
const HELD: u32 = 1880;
/// A copy is invisible from its tick 1 940.
const FADED: u32 = HELD + 60;

/// One corner of a box: its cell's offset, its angle and its two draws.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Square {
    offset: [f32; 2],
    angle: f32,
    phase: u32,
    blue: u32,
}

/// One copy of the 45 squares; `steps` is the ticks it has stepped, `clock` the last tick it looked at.
struct Copy {
    squares: Vec<Square>,
    steps: u32,
    clock: Instant,
    near: bool,
}

/// Power of Gaia's unit `id` at `at`, its next copy due at `next`.
struct Field {
    id: u32,
    at: Vector,
    next: Instant,
    copies: Vec<Copy>,
}

#[derive(Default)]
pub struct Gaia {
    fields: Vec<Field>,
    restarts: Vec<Vector>,
}

/// The 45 squares in the game's order (dz, then dx, then θ), each with its draws from `rand(n)`: the phase, then the blue base.
fn squares(mut rand: impl FnMut(u32) -> u32) -> Vec<Square> {
    let mut squares = Vec::with_capacity(45);
    for dz in [-5.0, 0.0, 5.0] {
        for dx in [-5.0, 0.0, 5.0] {
            for angle in [45.0, 135.0, 225.0, 315.0, 405.0] {
                let phase = rand(360);
                let blue = 100 + rand(50);
                squares.push(Square { offset: [dx, dz], angle, phase, blue });
            }
        }
    }
    squares
}

/// A wall's top at `age` for a square whose phase is now `phase`: 10.4 to 21.6, rising in over the first 90 ticks.
pub fn top(phase: u32, age: u32) -> f32 {
    let top = HEIGHT * (phase as f32).to_radians().sin() * 0.35 + HEIGHT * 0.65;
    if age < RISE { top * (age as f32).to_radians().sin() } else { top }
}

/// The alpha after the step at `age`: 120 held, then 2 less a tick to 0 at 1 940.
pub fn alpha(age: u32) -> f32 {
    (ALPHA - 2.0 * age.saturating_sub(HELD) as f32).max(0.0)
}

/// The blue channel at the colour phase `phase` for the base `base`.
pub fn blue(phase: u32, base: u32) -> f32 {
    ((phase as f32).to_radians().sin() * 100.0 + base as f32).trunc().clamp(0.0, 255.0)
}

/// A square's corner at angle `angle` round its cell's middle, on the ground.
fn corner(middle: Vector, angle: f32) -> Vector {
    let (sin, cos) = (angle % 360.0).to_radians().sin_cos();
    [middle[0] + SIZE * cos, middle[1], middle[2] + SIZE * sin]
}

impl Copy {
    fn new(now: Instant, rand: impl FnMut(u32) -> u32) -> Self {
        Self { squares: squares(rand), steps: 0, clock: now - TICK, near: false }
    }

    /// The 36 walls: square j's wall runs from square j − 1's corner to its own, both in square j's cell; a cell's first is a sliver
    /// and is skipped.
    fn walls(&self, at: Vector) -> Vec<Wall> {
        let Some(age) = self.steps.checked_sub(1) else { return Vec::new() };
        let alpha = alpha(age);
        let mut walls = Vec::with_capacity(36);
        for j in (1..self.squares.len()).filter(|j| j % 5 != 0) {
            let (before, square) = (self.squares[j - 1], self.squares[j]);
            let middle = [at[0] + square.offset[0], at[1], at[2] + square.offset[1]];
            let (from, to) = (corner(middle, before.angle), corner(middle, square.angle));
            let (from_top, to_top) = [before, square].map(|square| top((square.phase + self.steps) % 360, age)).into();
            let tint = [1.0, 1.0, blue(self.steps % 360, square.blue) / 255.0, alpha / 255.0];
            let vertex = |position: Vector, uv: [f32; 2]| MeshVertex { position, uv, tint };
            let a = vertex(from, [0.0, 1.0]);
            let b = vertex(to, [1.0, 1.0]);
            let c = vertex([to[0], to[1] + to_top, to[2]], [1.0, 0.0]);
            let d = vertex([from[0], from[1] + from_top, from[2]], [0.0, 0.0]);
            walls.push(Wall { texture: TEXTURE, vertices: vec![a, b, c, a, c, d], additive: true, forced: false });
        }
        walls
    }
}

impl Gaia {
    /// Power of Gaia over unit `id`'s cell centre `at`: the first copy now, the choices from `rand(n)` (0 to n − 1).
    pub fn keep(&mut self, id: u32, at: Vector, now: Instant, rand: impl FnMut(u32) -> u32) {
        self.fields.push(Field { id, at, next: now + TICK * PERIOD, copies: vec![Copy::new(now, rand)] });
    }

    pub fn forget(&mut self, id: u32) {
        self.fields.retain(|field| field.id != id);
    }

    /// Where a copy started since this was last asked (its sound).
    pub fn take_restarts(&mut self) -> Vec<Vector> {
        std::mem::take(&mut self.restarts)
    }

    /// Starts the copies that are due and steps each one tick by tick while it is within 130 of `player`.
    pub fn advance(&mut self, now: Instant, player: Option<Vector>, mut rand: impl FnMut(u32) -> u32) {
        for field in &mut self.fields {
            while field.next <= now {
                field.copies.push(Copy::new(field.next, &mut rand));
                self.restarts.push(field.at);
                field.next += TICK * PERIOD;
            }
            let near = player.is_some_and(|player| (0..3).map(|i| (player[i] - field.at[i]).powi(2)).sum::<f32>().sqrt() <= NEAR);
            for copy in &mut field.copies {
                while copy.clock + TICK <= now {
                    copy.clock += TICK;
                    copy.steps += u32::from(near);
                }
                copy.near = near;
            }
            // ponytail: a copy stays as long as its unit in the game; one faded out is dropped here, as nothing shows it again.
            field.copies.retain(|copy| copy.steps <= FADED);
        }
    }

    /// Every wall to draw now.
    pub fn meshes(&self) -> Vec<Wall> {
        self.fields.iter().flat_map(|field| field.copies.iter().filter(|copy| copy.near).flat_map(|copy| copy.walls(field.at))).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(got: f32, want: f32) -> bool {
        (got - want).abs() < 0.01
    }

    #[test]
    fn power_of_gaia_makes_45_squares_in_the_games_order() {
        let mut draws = Vec::new();
        let squares = squares(|n| {
            draws.push(n);
            draws.len() as u32
        });
        assert_eq!(squares.len(), 45);
        assert_eq!(draws, [360, 50].repeat(45), "two draws a square: the phase, then the blue base");
        assert_eq!((squares[0].phase, squares[0].blue, squares[1].phase), (1, 102, 3));
        assert_eq!(squares[..5].iter().map(|square| square.angle).collect::<Vec<_>>(), [45.0, 135.0, 225.0, 315.0, 405.0]);
        assert_eq!([0, 5, 10, 15, 44].map(|j| squares[j].offset), [[-5.0, -5.0], [0.0, -5.0], [5.0, -5.0], [-5.0, 0.0], [5.0, 5.0]]);
    }

    #[test]
    fn power_of_gaias_walls_rise_sway_pulse_and_fade() {
        // At phase 90: 16·0.35 + 16·0.65 = 16, times sin(age) while it rises.
        assert_eq!([0, 1, 45, 89, 90].map(|age| near(top(90, age), 16.0 * (age.min(90) as f32).to_radians().sin())), [true; 5]);
        assert!(near(top(270, 120), 16.0 * 0.3) && near(top(0, 120), 10.4));
        assert_eq!([0, 1880, 1881, 1939, 1940, 2000].map(alpha), [120.0, 120.0, 118.0, 2.0, 0.0, 0.0]);
        assert_eq!([270, 0, 90].map(|phase| blue(phase, 100)), [0.0, 100.0, 200.0]);
    }

    #[test]
    fn power_of_gaia_draws_36_walls_round_nine_cells() {
        let start = Instant::now();
        let mut gaia = Gaia::default();
        let at = [10.0, 2.0, 20.0];
        gaia.keep(7, at, start, |_| 0);
        gaia.advance(start, Some(at), |_| 0);
        let walls = gaia.meshes();
        assert_eq!(walls.len(), 36);
        assert!(walls.iter().all(|wall| wall.texture == TEXTURE && wall.additive && !wall.forced && wall.vertices.len() == 6));
        // Square 1's wall spans corners 45° and 135° of the (−5, −5) cell: a side of 4.95, on the ground at age 0.
        let [a, b, ..] = [0, 1, 2].map(|i| walls[0].vertices[i].position);
        let middle = [5.0, 2.0, 15.0];
        assert!(near(a[0], middle[0] + 3.5 * 45.0_f32.to_radians().cos()) && near(a[2], middle[2] + 3.5 * 45.0_f32.to_radians().sin()));
        assert!(near(b[0], middle[0] + 3.5 * 135.0_f32.to_radians().cos()) && near(f32::hypot(a[0] - b[0], a[2] - b[2]), 4.95));
        assert!(walls[0].vertices.iter().all(|vertex| near(vertex.position[1], 2.0)));
        // The 405° corner is at 45°: square 4's wall ends where square 1's begins.
        assert!(near(walls[3].vertices[1].position[0], a[0]) && near(walls[3].vertices[1].position[2], a[2]));
        // Alpha 120, the colour (255, 255, base + sin·100) with the colour phase at 1 after the first step.
        let tint = walls[0].vertices[0].tint;
        assert!(tint[..2] == [1.0, 1.0] && near(tint[2], (1.0_f32.to_radians().sin() * 100.0 + 100.0).trunc() / 255.0) && near(tint[3], 120.0 / 255.0));
        // A tick on, the tops have risen.
        gaia.advance(start + TICK, Some(at), |_| 0);
        assert!(gaia.meshes()[0].vertices[2].position[1] > 2.0);
    }

    #[test]
    fn power_of_gaia_starts_a_copy_beside_the_old_every_1999_ticks() {
        let start = Instant::now();
        let mut gaia = Gaia::default();
        let (at, far) = ([0.0; 3], Some([500.0, 0.0, 0.0]));
        gaia.keep(7, at, start, |_| 0);
        // Away from the player neither copy ages, so the relaunch stands beside the first.
        gaia.advance(start + TICK * 1998, far, |_| 0);
        assert!(gaia.fields[0].copies.len() == 1 && gaia.take_restarts().is_empty());
        gaia.advance(start + TICK * 1999, far, |_| 0);
        assert_eq!(gaia.fields[0].copies.len(), 2);
        assert_eq!(gaia.take_restarts(), [at]);
        gaia.advance(start + TICK * 2000, Some(at), |_| 0);
        assert_eq!(gaia.meshes().len(), 72);
        // Near the player a copy has faded at its tick 1 940 and goes; the unit going takes the rest.
        gaia.advance(start + TICK * (2000 + 1939), Some(at), |_| 0);
        assert_eq!(gaia.fields[0].copies.len(), 2);
        gaia.advance(start + TICK * (2000 + 1940), Some(at), |_| 0);
        assert!(gaia.fields[0].copies.is_empty());
        gaia.keep(8, at, start, |_| 0);
        gaia.forget(8);
        assert!(gaia.fields.len() == 1 && gaia.meshes().is_empty());
    }

    #[test]
    fn power_of_gaia_stands_still_away_from_the_player() {
        let start = Instant::now();
        let mut gaia = Gaia::default();
        gaia.keep(7, [0.0; 3], start, |_| 0);
        gaia.advance(start + TICK * 10, Some([131.0, 0.0, 0.0]), |_| 0);
        assert!(gaia.meshes().is_empty() && gaia.fields[0].copies[0].steps == 0);
        gaia.advance(start + TICK * 12, Some([130.0, 0.0, 0.0]), |_| 0);
        assert_eq!(gaia.fields[0].copies[0].steps, 2);
        assert_eq!(gaia.meshes().len(), 36);
        gaia.advance(start + TICK * 13, None, |_| 0);
        assert!(gaia.meshes().is_empty());
    }
}
