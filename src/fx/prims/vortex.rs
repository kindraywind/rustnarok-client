//! Recognized Spell's skill icons (803) and Tetra Vortex's cards and light columns (804), a 16 ms tick at a time.
//!
//! A shape's own age n counts from 1 on its first tick; e is the ticks since the effect started.

use std::time::Instant;

use super::chi::tube;
use super::circle;
use super::passes;
use super::walls::Wall;
use crate::render::Texture;
use crate::world::{EffectQuad, Vector};

pub const RING_RED: &str = "data\\texture\\effect\\ring_red.tga";

pub const TV_TEXTURES: [&str; 8] = [
    "data\\texture\\effect\\tv-1.bmp",
    "data\\texture\\effect\\tv-2.bmp",
    "data\\texture\\effect\\tv-3.bmp",
    "data\\texture\\effect\\tv-4.bmp",
    "data\\texture\\effect\\tv-5.bmp",
    "data\\texture\\effect\\tv-6.bmp",
    "data\\texture\\effect\\tv-7.bmp",
    "data\\texture\\effect\\tv-8.bmp",
];

pub const MG_ICONS: [&str; 12] = [
    "data\\texture\\유저인터페이스\\item\\MG_SRECOVERY.bmp",
    "data\\texture\\유저인터페이스\\item\\MG_SIGHT.bmp",
    "data\\texture\\유저인터페이스\\item\\MG_NAPALMBEAT.bmp",
    "data\\texture\\유저인터페이스\\item\\MG_SAFETYWALL.bmp",
    "data\\texture\\유저인터페이스\\item\\MG_SOULSTRIKE.bmp",
    "data\\texture\\유저인터페이스\\item\\MG_COLDBOLT.bmp",
    "data\\texture\\유저인터페이스\\item\\MG_FROSTDIVER.bmp",
    "data\\texture\\유저인터페이스\\item\\MG_STONECURSE.bmp",
    "data\\texture\\유저인터페이스\\item\\MG_FIREBALL.bmp",
    "data\\texture\\유저인터페이스\\item\\MG_FIREWALL.bmp",
    "data\\texture\\유저인터페이스\\item\\MG_FIREBOLT.bmp",
    "data\\texture\\유저인터페이스\\item\\MG_LIGHTNINGBOLT.bmp",
];

pub const WZ_ICONS: [&str; 13] = [
    "data\\texture\\유저인터페이스\\item\\WZ_FIREPILLAR.bmp",
    "data\\texture\\유저인터페이스\\item\\WZ_SIGHTRASHER.bmp",
    "data\\texture\\유저인터페이스\\item\\WZ_FIREIVY.bmp",
    "data\\texture\\유저인터페이스\\item\\WZ_METEOR.bmp",
    "data\\texture\\유저인터페이스\\item\\WZ_JUPITEL.bmp",
    "data\\texture\\유저인터페이스\\item\\WZ_VERMILION.bmp",
    "data\\texture\\유저인터페이스\\item\\WZ_WATERBALL.bmp",
    "data\\texture\\유저인터페이스\\item\\WZ_ICEWALL.bmp",
    "data\\texture\\유저인터페이스\\item\\WZ_FROSTNOVA.bmp",
    "data\\texture\\유저인터페이스\\item\\WZ_STORMGUST.bmp",
    "data\\texture\\유저인터페이스\\item\\WZ_EARTHSPIKE.bmp",
    "data\\texture\\유저인터페이스\\item\\WZ_HEAVENDRIVE.bmp",
    "data\\texture\\유저인터페이스\\item\\WZ_QUAGMIRE.bmp",
];

pub const PILLAR_COLORS: [[f32; 3]; 5] = [
    [100.0 / 255.0, 100.0 / 255.0, 1.0], // 4: #6464FF
    [1.0, 1.0, 1.0],                     // 5: #FFFFFF
    [1.0, 100.0 / 255.0, 100.0 / 255.0], // 6: #FF6464
    [100.0 / 255.0, 1.0, 100.0 / 255.0], // 7: #64FF64
    [1.0, 1.0, 170.0 / 255.0],           // 8: #FFFFAA
];

pub const PILLAR_RED: [f32; 3] = [1.0, 10.0 / 255.0, 10.0 / 255.0]; // #FF0A0A
pub const PIXELS_PER_UNIT: f32 = 7.0;

const SPELL_LIFE: u32 = 500;
const TETRA_LIFE: u32 = 600;

/// The skill id picked by `branch` (0 = Magician, 1 = Wizard) and `pick` (0..12 or 0..13).
pub fn skill_id(branch: u32, pick: u32) -> u16 {
    if branch == 0 { 9 + (pick % 12) as u16 } else { 80 + (pick % 13) as u16 }
}

/// The icon texture path for `id`.
pub fn skill_icon(id: u16) -> &'static str {
    match id {
        9..=20 => MG_ICONS[(id - 9) as usize],
        80..=92 => WZ_ICONS[(id - 80) as usize],
        _ => MG_ICONS[0],
    }
}

/// The corners at the angles 120°, 60°, 300°, 240° at radius `r` in the camera's plane.
pub fn card_corners(r: f32) -> [[f32; 2]; 4] {
    [120.0_f32, 60.0, 300.0, 240.0].map(|angle| {
        let (sin, cos) = angle.to_radians().sin_cos();
        [r * cos, r * sin]
    })
}

/// The column's colour for colour index `c` (4..=8).
pub fn pillar_color(c: u32) -> [f32; 3] {
    match c {
        4 => PILLAR_COLORS[0],
        5 => PILLAR_COLORS[1],
        6 => PILLAR_COLORS[2],
        7 => PILLAR_COLORS[3],
        8 => PILLAR_COLORS[4],
        _ => PILLAR_COLORS[0],
    }
}

/// A light column's profile height at age `n` with height `h`.
pub fn pillar_profile(h: f32, n: u32) -> f32 {
    match n {
        0..=99 => 0.0,
        100..=115 => h * (6.0 * (n - 100) as f32).to_radians().sin(),
        _ => h,
    }
}

/// One of Recognized Spell's skill icons: its skill id, age n, half-diagonal size, and ten alphas a₀ … a₉.
#[derive(Clone, Debug)]
pub struct Icon {
    pub skill: u16,
    pub age: u32,
    pub size: f32,
    pub alphas: [i32; 10],
}

impl Icon {
    pub fn new(skill: u16) -> Self {
        Self { skill, age: 0, size: 5.0, alphas: [0; 10] }
    }

    pub fn step(&mut self) {
        self.age += 1;
        let n = self.age;
        if n < 20 {
            for (i, alpha) in self.alphas.iter_mut().enumerate() {
                *alpha = (*alpha + 13).min(70 + 20 * i as i32);
            }
        } else {
            for alpha in &mut self.alphas[..9] {
                *alpha = (*alpha - 5).max(0);
            }
            if n > 25 {
                self.alphas[9] = (self.alphas[9] - 5).max(0);
                if self.alphas[9] > 0 {
                    self.size *= 1.01;
                }
            }
        }
    }
}

/// One of Tetra Vortex's flip-book cards.
#[derive(Clone, Debug)]
pub struct Card {
    pub age: u32,
    pub index: usize,
    pub alpha: i32,
    pub place: Vector,
}

impl Card {
    pub fn new(place: Vector) -> Self {
        Self { age: 0, index: 0, alpha: 200, place }
    }

    pub fn step(&mut self) {
        self.age += 1;
        let n = self.age;
        if n.is_multiple_of(3) {
            self.index += 1;
            if self.index >= 8 {
                self.index = 7;
                self.alpha = (self.alpha - 15).max(0);
            }
        }
    }
}

/// One of Tetra Vortex's light columns: age, turn, current height, alpha, radius, offset from target, and colour.
#[derive(Clone, Debug)]
pub struct Column {
    pub age: u32,
    pub turn: f32,
    pub height: f32,
    pub alpha: f32,
    pub radius: f32,
    pub offset: Vector,
    pub color: [f32; 3],
}

impl Column {
    pub fn step(&mut self) {
        self.turn = (self.turn + 1.0) % 360.0;
        self.age += 1;
        let n = self.age;
        if n >= 125 {
            self.alpha = (self.alpha - 15.0).max(0.0);
            self.height = (self.height - 5.0).max(0.0);
        }
    }

    pub fn profile(&self) -> f32 {
        pillar_profile(self.height, self.age)
    }
}

/// An icon quad to draw: texture, world centre, half-diagonal in world units, and vertex alpha (0..255).
#[derive(Clone, Debug, PartialEq)]
pub struct IconDraw {
    pub texture: &'static str,
    pub at: Vector,
    pub half: f32,
    pub alpha: f32,
}

/// A card quad to draw: texture, world centre, corner offsets in world units, and vertex alpha (0..255).
#[derive(Clone, Debug, PartialEq)]
pub struct CardDraw {
    pub texture: &'static str,
    pub at: Vector,
    pub corners: [[f32; 2]; 4],
    pub alpha: f32,
}

struct Spell {
    unit: Option<u32>,
    start: Instant,
    ticks: u32,
    icons: Vec<Icon>,
}

struct Tetra {
    aim: Vector,
    start: Instant,
    ticks: u32,
    cards: Vec<Card>,
    columns: Vec<Column>,
}

#[derive(Default)]
pub struct Vortex {
    spells: Vec<Spell>,
    tetras: Vec<Tetra>,
}

impl Vortex {
    pub fn recognized(&mut self, unit: Option<u32>, now: Instant) {
        self.spells.push(Spell { unit, start: now, ticks: 0, icons: Vec::new() });
    }

    pub fn tetra(&mut self, aim: Vector, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        let mut columns = Vec::with_capacity(7 * 4);
        for _ in 0..7 {
            let color = pillar_color(4 + rand(5));
            for k in 0..4 {
                let age = rand(100);
                let q = rand(41) as f32;
                let a = (rand(360) as f32).to_radians();
                let r = 0.15 + 0.015 * rand(11) as f32;
                let offset = [q * a.cos(), 0.0, q * a.sin()];
                columns.push(Column { age, turn: 90.0 * k as f32, height: 120.0, alpha: 150.0, radius: r, offset, color });
            }
        }
        self.tetras.push(Tetra { aim, start: now, ticks: 0, cards: Vec::new(), columns });
    }

    pub fn advance(&mut self, now: Instant, mut rand: impl FnMut(u32) -> u32) {
        for spell in &mut self.spells {
            while spell.ticks < passes(spell.start, now).min(SPELL_LIFE) {
                let e = spell.ticks;
                spell.ticks += 1;
                for icon in &mut spell.icons {
                    icon.step();
                }
                if e <= 271 && e % 30 == 1 {
                    let branch = rand(2);
                    let pick = if branch == 0 { rand(12) } else { rand(13) };
                    let skill = skill_id(branch, pick);
                    let mut icon = Icon::new(skill);
                    icon.step();
                    spell.icons.push(icon);
                }
            }
        }
        for tetra in &mut self.tetras {
            while tetra.ticks < passes(tetra.start, now).min(TETRA_LIFE) {
                let e = tetra.ticks;
                tetra.ticks += 1;
                for card in &mut tetra.cards {
                    card.step();
                }
                if e <= 80 && e % 25 == 0 {
                    let mut card = Card::new([tetra.aim[0], tetra.aim[1] + 35.0, tetra.aim[2]]);
                    card.step();
                    tetra.cards.push(card);
                }
                for column in &mut tetra.columns {
                    column.step();
                }
            }
        }
        self.spells.retain(|spell| passes(spell.start, now) <= SPELL_LIFE);
        self.tetras.retain(|tetra| passes(tetra.start, now) <= TETRA_LIFE);
    }

    pub fn icon_draws(&self, yaw: f32, position_of: impl Fn(Option<u32>) -> Option<Vector>) -> Vec<IconDraw> {
        let mut draws = Vec::new();
        let (sin, cos) = yaw.to_radians().sin_cos();
        for spell in &self.spells {
            let Some(feet) = position_of(spell.unit) else { continue };
            let at = [feet[0] + cos, feet[1] + 40.0, feet[2] + sin];
            for icon in &spell.icons {
                for j in 0..10 {
                    let alpha = icon.alphas[9 - j];
                    if alpha > 0 {
                        draws.push(IconDraw { texture: skill_icon(icon.skill), at, half: icon.size + j as f32, alpha: alpha as f32 });
                    }
                }
            }
        }
        draws
    }

    pub fn card_draws(&self) -> Vec<CardDraw> {
        let mut draws = Vec::new();
        for tetra in &self.tetras {
            for card in &tetra.cards {
                if card.alpha > 0 {
                    draws.push(CardDraw { texture: TV_TEXTURES[card.index], at: card.place, corners: card_corners(30.0), alpha: card.alpha as f32 });
                }
            }
        }
        draws
    }

    pub fn quads<'a>(
        &self,
        now: Instant,
        yaw: f32,
        texture_of: impl Fn(&str) -> Option<&'a Texture>,
        position_of: &impl Fn(Option<u32>) -> Option<Vector>,
    ) -> Vec<EffectQuad<'a>> {
        let _ = now;
        let mut quads = Vec::new();
        for draw in self.icon_draws(yaw, position_of) {
            let Some(texture) = texture_of(draw.texture) else { continue };
            let corners = circle::corners(135.0, draw.half * PIXELS_PER_UNIT);
            quads.push((texture, draw.at, corners, [1.0, 1.0, 1.0, draw.alpha / 255.0], false, false));
        }
        for draw in self.card_draws() {
            let Some(texture) = texture_of(draw.texture) else { continue };
            let corners = draw.corners.map(|[x, y]| [x * PIXELS_PER_UNIT, y * PIXELS_PER_UNIT]);
            quads.push((texture, draw.at, corners, [1.0, 1.0, 1.0, draw.alpha / 255.0], false, false));
        }
        quads
    }

    pub fn meshes(&self) -> Vec<Wall> {
        let (mut core, mut glow) = (Vec::new(), Vec::new());
        for tetra in &self.tetras {
            for col in &tetra.columns {
                let p = col.profile();
                if col.alpha <= 0.0 || p <= 0.0 {
                    continue;
                }
                let foot = [tetra.aim[0] + col.offset[0], tetra.aim[1], tetra.aim[2] + col.offset[2]];
                core.extend(tube(foot, p, col.turn, col.radius, [PILLAR_RED[0], PILLAR_RED[1], PILLAR_RED[2], (col.alpha / 5.0).floor() / 255.0], None));
                glow.extend(tube(foot, p, col.turn + 45.0, 1.5 * col.radius, [col.color[0], col.color[1], col.color[2], col.alpha / 255.0], Some(21.0)));
            }
        }
        let mut walls = Vec::new();
        if !core.is_empty() {
            walls.push(Wall { texture: RING_RED, vertices: core, additive: false, forced: false });
        }
        if !glow.is_empty() {
            walls.push(Wall { texture: RING_RED, vertices: glow, additive: true, forced: false });
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

    fn pillar_alpha(n: u32) -> f32 {
        (150.0 - 15.0 * n.saturating_sub(124) as f32).max(0.0)
    }

    #[test]
    fn recognized_spell_icons_timing_and_draws() {
        let start = Instant::now();
        let mut vortex = Vortex::default();
        vortex.recognized(Some(1), start);

        // One prim on e = 1, 31 … 271 (10) and none on other ticks.
        let mut prim_counts = Vec::new();
        let mut total_draws = 0;
        for e in 0..=350 {
            let before = vortex.spells[0].icons.len();
            vortex.advance(start + TICK * e, |_n| {
                total_draws += 1;
                0
            });
            let after = vortex.spells[0].icons.len();
            if after > before {
                prim_counts.push((e, after));
            }
        }
        let expected_ticks: Vec<u32> = (0..10).map(|i| 30 * i + 1).collect();
        assert_eq!(prim_counts.iter().map(|&(e, _)| e).collect::<Vec<_>>(), expected_ticks);
        assert_eq!(vortex.spells[0].icons.len(), 10);
        assert_eq!(total_draws, 20); // 2 draws per icon * 10 icons

        // The skill id from R(2) and the branch's draw:
        // R(2) = 0 with R(12) = 0 -> 9, R(12) = 11 -> 20
        assert_eq!(skill_id(0, 0), 9);
        assert_eq!(skill_id(0, 11), 20);
        // R(2) = 1 with R(13) = 0 -> 80, 12 -> 92
        assert_eq!(skill_id(1, 0), 80);
        assert_eq!(skill_id(1, 12), 92);
        assert_eq!(skill_icon(9), "data\\texture\\유저인터페이스\\item\\MG_SRECOVERY.bmp");
        assert_eq!(skill_icon(20), "data\\texture\\유저인터페이스\\item\\MG_LIGHTNINGBOLT.bmp");
        assert_eq!(skill_icon(80), "data\\texture\\유저인터페이스\\item\\WZ_FIREPILLAR.bmp");
        assert_eq!(skill_icon(82), "data\\texture\\유저인터페이스\\item\\WZ_FIREIVY.bmp");
        assert_eq!(skill_icon(92), "data\\texture\\유저인터페이스\\item\\WZ_QUAGMIRE.bmp");

        // The table's a₉, a₀ and size at n = 1, 6, 10, 19, 25, 40, 50, 65, 74, 75
        let mut icon = Icon::new(9);
        let mut table_values = Vec::new();
        for _ in 1..=75 {
            icon.step();
            if [1, 6, 10, 19, 25, 40, 50, 65, 74, 75].contains(&icon.age) {
                table_values.push((icon.age, icon.alphas[9], icon.alphas[0], icon.size));
            }
        }
        assert_eq!(table_values[0], (1, 13, 13, 5.0));
        assert_eq!(table_values[1], (6, 78, 70, 5.0));
        assert_eq!(table_values[2], (10, 130, 70, 5.0));
        assert_eq!(table_values[3], (19, 247, 70, 5.0));
        assert_eq!(table_values[4], (25, 247, 40, 5.0));
        assert!(table_values[5].0 == 40 && table_values[5].1 == 172 && table_values[5].2 == 0 && near(table_values[5].3, 5.81));
        assert!(table_values[6].0 == 50 && table_values[6].1 == 122 && table_values[6].2 == 0 && near(table_values[6].3, 6.41));
        assert!(table_values[7].0 == 65 && table_values[7].1 == 47 && table_values[7].2 == 0 && near(table_values[7].3, 7.44));
        assert!(table_values[8].0 == 74 && table_values[8].1 == 2 && table_values[8].2 == 0 && near(table_values[8].3, 8.14));
        assert_eq!(table_values[9], (75, 0, 0, table_values[8].3));

        // Ten copies of half-diagonal size + 0 … 9 drawn in order with the alphas a₉ … a₀
        let mut fresh = Vortex::default();
        fresh.recognized(Some(1), start);
        fresh.advance(start + TICK, |_| 0); // e = 1: one icon created, n = 1
        let feet = [10.0, 20.0, 30.0];
        let draws = fresh.icon_draws(0.0, |_| Some(feet));
        assert_eq!(draws.len(), 10);
        for (j, draw) in draws.iter().enumerate() {
            assert_eq!(draw.half, 5.0 + j as f32);
            assert_eq!(draw.alpha, 13.0); // at n = 1, all alphas are 13
        }

        // The centre 40 up and 1.0 sideways
        assert_eq!(draws[0].at, [feet[0] + 1.0, feet[1] + 40.0, feet[2]]);
    }

    #[test]
    fn tetra_vortex_cards_and_columns() {
        let start = Instant::now();
        let mut vortex = Vortex::default();
        let aim = [100.0, 10.0, 200.0];

        // 7 prims * 4 bands = 28 made at e = 0 and none later
        // 17 draws a prim in the order R(5) then R(100), R(41), R(360), R(11) four times
        let mut draw_caps = Vec::new();
        vortex.tetra(aim, start, |cap| {
            draw_caps.push(cap);
            0
        });
        assert_eq!(draw_caps.len(), 7 * 17);
        for chunk in draw_caps.chunks(17) {
            assert_eq!(chunk[0], 5);
            for band in chunk[1..].chunks(4) {
                assert_eq!(band, [100, 41, 360, 11]);
            }
        }
        assert_eq!(vortex.tetras[0].columns.len(), 28);

        // The colour for each c = 4 … 8
        assert_eq!(pillar_color(4), [100.0 / 255.0, 100.0 / 255.0, 1.0]);
        assert_eq!(pillar_color(5), [1.0, 1.0, 1.0]);
        assert_eq!(pillar_color(6), [1.0, 100.0 / 255.0, 100.0 / 255.0]);
        assert_eq!(pillar_color(7), [100.0 / 255.0, 1.0, 100.0 / 255.0]);
        assert_eq!(pillar_color(8), [1.0, 1.0, 170.0 / 255.0]);

        // Cards on e = 0, 25, 50, 75 and none on 100
        let mut card_e = Vec::new();
        for e in 0..=100 {
            let before = vortex.tetras[0].cards.len();
            vortex.advance(start + TICK * e, |_| 0);
            let after = vortex.tetras[0].cards.len();
            if after > before {
                card_e.push(e);
            }
        }
        assert_eq!(card_e, [0, 25, 50, 75]);

        // The card's picture index at n = 1, 2, 3, 5, 6, 21, 23 (0, 0, 1, 1, 2, 7, 7)
        // alpha at n = 23, 24, 27, 60, 62, 63 (200, 185, 170, 5, 5, 0: gone)
        let mut card = Card::new(aim);
        let mut indices = Vec::new();
        let mut alphas = Vec::new();
        for _ in 1..=63 {
            card.step();
            if [1, 2, 3, 5, 6, 21, 23].contains(&card.age) {
                indices.push((card.age, card.index));
            }
            if [23, 24, 27, 60, 62, 63].contains(&card.age) {
                alphas.push((card.age, card.alpha));
            }
        }
        assert_eq!(indices.into_iter().map(|(_, idx)| idx).collect::<Vec<_>>(), [0, 0, 1, 1, 2, 7, 7]);
        assert_eq!(alphas.into_iter().map(|(_, a)| a).collect::<Vec<_>>(), [200, 185, 170, 5, 5, 0]);

        // The corners (±15, ±25.98) and 35 up from A
        let corners = card_corners(30.0);
        assert!(near(corners[0][0], -15.0) && near(corners[0][1], 25.98));
        assert!(near(corners[1][0], 15.0) && near(corners[1][1], 25.98));
        assert!(near(corners[2][0], 15.0) && near(corners[2][1], -25.98));
        assert!(near(corners[3][0], -15.0) && near(corners[3][1], -25.98));
        let card_draws = vortex.card_draws();
        assert!(!card_draws.is_empty());
        assert_eq!(card_draws[0].at, [aim[0], aim[1] + 35.0, aim[2]]);

        // A band with age start 0 first drawn at n = 101,
        // its p at n = 100, 108, 115, 116 (0, 120·sin 48°, 120, 120),
        // its alpha at n = 124, 125, 133, 134 (150, 135, 15, 0)
        let mut col = Column { age: 0, turn: 0.0, height: 120.0, alpha: 150.0, radius: 0.2, offset: [0.0; 3], color: [1.0; 3] };
        let mut p_vals = Vec::new();
        let mut a_vals = Vec::new();
        let mut first_drawn = None;
        for _ in 1..=140 {
            col.step();
            let p = col.profile();
            if col.alpha > 0.0 && p > 0.0 && first_drawn.is_none() {
                first_drawn = Some(col.age);
            }
            if [100, 108, 115, 116].contains(&col.age) {
                p_vals.push((col.age, p));
            }
            if [124, 125, 133, 134].contains(&col.age) {
                a_vals.push((col.age, col.alpha));
            }
        }
        assert_eq!(first_drawn, Some(101));
        assert_eq!(p_vals[0].1, 0.0);
        assert!(near(p_vals[1].1, 120.0 * 48.0_f32.to_radians().sin()));
        assert_eq!(p_vals[2].1, 120.0);
        assert_eq!(p_vals[3].1, 120.0);
        assert_eq!(a_vals.into_iter().map(|(_, a)| a).collect::<Vec<_>>(), [150.0, 135.0, 15.0, 0.0]);
        assert_eq!([124, 125, 133, 134].map(pillar_alpha), [150.0, 135.0, 15.0, 0.0]);

        // The columns' offsets within 40 of A
        let mut offsets_vortex = Vortex::default();
        offsets_vortex.tetra(aim, start, |cap| cap - 1);
        for col in &offsets_vortex.tetras[0].columns {
            let dist = f32::hypot(col.offset[0], col.offset[2]);
            assert!(dist <= 40.0, "offset {dist} exceeds 40");
        }
    }
}
