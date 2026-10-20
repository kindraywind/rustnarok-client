//! Experiment: a Diablo IV style HUD. The hotbar is drawn as the action bar at the bottom middle: the life orb (HP) on
//! its left, the resource orb (SP) on its right, nine sockets for its first row between them and the experience bar
//! under them. A vignette closes in the screen's edges, the world is graded darker (world.rs `grade`) and the monster
//! pointed at or fought gets a bar at the top middle. Every texture here is painted by code; none come from the data.

use std::collections::HashMap;
use std::f32::consts::PI;
use std::sync::OnceLock;

use image::{Rgba, RgbaImage};
use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};
use crate::window::{self, Chrome};
use crate::{Layer, contains, screen};

/// Textures are painted at twice their size on screen: sharp on a Retina display.
const SS: f32 = 2.0;

pub const WIDTH: f32 = 670.0;
pub const HEIGHT: f32 = 128.0;
/// Each orb's box: the left one at x 0, the right one at WIDTH - ORB_BOX. The liquid is a circle inside the ring.
const ORB_BOX: f32 = 128.0;
const ORB_RADIUS: f32 = 50.0;
const RING_OUTER: f32 = 62.0;
/// The plate behind the sockets (x, y, width, height), its top corners cut.
const PANEL: [f32; 4] = [112.0, 46.0, 446.0, 82.0];
const CHAMFER: f32 = 18.0;
pub const SOCKET: f32 = 42.0;
const SOCKET_STEP: f32 = 47.0;
const FIRST_SOCKET: [f32; 2] = [126.0, 58.0];
pub const ICON: f32 = 34.0;
const XP_BAR: [f32; 4] = [140.0, 109.0, 390.0, 7.0];
/// The enemy bar at the top middle of the screen: its size and top.
const ENEMY_BAR: [f32; 2] = [340.0, 12.0];
const ENEMY_TOP: f32 = 34.0;

pub(crate) const IRON: [f32; 3] = [0.11, 0.10, 0.095];
pub(crate) const BRONZE: [f32; 3] = [0.54, 0.40, 0.23];
pub(crate) const BRONZE_LIGHT: [f32; 3] = [0.84, 0.67, 0.41];
/// The liquids' deep and lit colours.
const LIFE: ([f32; 3], [f32; 3]) = ([0.28, 0.0, 0.02], [0.86, 0.10, 0.07]);
const MANA: ([f32; 3], [f32; 3]) = ([0.03, 0.06, 0.30], [0.22, 0.45, 0.95]);
pub(crate) const TEXT: [u8; 3] = [232, 222, 205];
pub(crate) const LABEL: [u8; 3] = [214, 178, 120];
/// The serif the bar's words are set in, then fallbacks (Hangul names included).
const TITLE_FONTS: [&str; 4] = [
    "/System/Library/Fonts/Supplemental/Copperplate.ttc",
    "/System/Library/Fonts/Supplemental/Georgia Bold.ttf",
    "/System/Library/Fonts/Supplemental/Tahoma.ttf",
    "/System/Library/Fonts/Supplemental/AppleGothic.ttf",
];

fn title_font() -> Option<&'static Font> {
    static FONT: OnceLock<Option<Font>> = OnceLock::new();
    FONT.get_or_init(|| Font::from_files(&TITLE_FONTS)).as_ref()
}

/// Words in the title font: text, pixel size and colour.
pub type Word = (String, u8, [u8; 3]);

pub fn label_word(text: &str) -> Word {
    (text.to_owned(), 12, LABEL)
}

pub fn number_word([value, max]: [i64; 2]) -> Word {
    (format!("{value} / {max}"), 13, TEXT)
}

pub fn name_word(name: &str) -> Word {
    (name.to_uppercase(), 17, TEXT)
}

pub struct Assets {
    ring: Texture,
    glass: Texture,
    panel: Texture,
    socket: Texture,
    glow: Texture,
    xp_back: Texture,
    xp_fill: Texture,
    pub vignette: Texture,
    enemy_back: Texture,
    enemy_fill: Texture,
}

impl Assets {
    pub fn new(gpu: &Gpu) -> Self {
        let texture = |image: RgbaImage| gpu.texture(&image, true);
        Self {
            ring: texture(paint([ORB_BOX; 2], ring)),
            glass: texture(paint([ORB_RADIUS * 2.0; 2], glass)),
            panel: texture(paint([PANEL[2], PANEL[3]], panel)),
            socket: texture(paint([SOCKET; 2], |x, y| socket(x, y, false))),
            glow: texture(paint([SOCKET; 2], |x, y| socket(x, y, true))),
            xp_back: texture(paint([XP_BAR[2] + 4.0, XP_BAR[3] + 4.0], |x, y| frame_bar([XP_BAR[2] + 4.0, XP_BAR[3] + 4.0], x, y))),
            xp_fill: texture(paint([1.0, XP_BAR[3]], |_, y| gradient(y / XP_BAR[3], [1.0, 0.86, 0.48], [0.62, 0.40, 0.10]))),
            vignette: texture(paint([128.0, 96.0], vignette)),
            enemy_back: texture(paint([ENEMY_BAR[0] + 24.0, ENEMY_BAR[1] + 8.0], enemy_back)),
            enemy_fill: texture(paint([1.0, ENEMY_BAR[1]], |_, y| gradient(y / ENEMY_BAR[1], [0.85, 0.12, 0.08], [0.38, 0.02, 0.02]))),
        }
    }
}

/// The orbs' liquid, painted again as it moves (20 times a second) or the share changes, and the words drawn this frame
/// (each with its black for the outline).
#[derive(Default)]
pub struct State {
    made: [Option<((u32, u32), Texture)>; 2],
    words: HashMap<Word, [Texture; 2]>,
}

impl State {
    /// Rasterizes the words this frame needs (twice their size, for Retina) and forgets the others.
    pub fn prepare(&mut self, gpu: &Gpu, wanted: Vec<Word>) {
        self.words.retain(|word, _| wanted.contains(word));
        let Some(font) = title_font() else { return };
        for word in wanted {
            if let std::collections::hash_map::Entry::Vacant(entry) = self.words.entry(word) {
                let (text, size, color) = entry.key();
                let made = [*color, [0, 0, 0]].map(|color| gpu.texture(&font.render_sized(text, color, f32::from(*size) * SS), true));
                entry.insert(made);
            }
        }
    }

    /// A word's width on screen; without the title font, the UI font's.
    fn width(&self, word: &Word, font: &Font) -> f32 {
        self.words.get(word).map_or_else(|| font.width(&word.0), |[text, _]| (text.size[0] / SS).round())
    }

    /// A word with its top left at `at`, outlined in black.
    fn put<'a>(&'a self, layer: &mut Layer<'a>, word: &Word, at: [f32; 2]) {
        let Some([text, black]) = self.words.get(word) else { return layer.outlined_in(&word.0, at, word.2) };
        let size = text.size.map(|value| value / SS);
        for [dx, dy] in [[-1.0, 0.0], [1.0, 0.0], [0.0, -1.0], [0.0, 1.0], [1.0, 1.0]] {
            layer.sprites.push((black, [at[0] + dx, at[1] + dy], size));
        }
        layer.sprites.push((text, at, size));
    }

    /// `shares`: life and resource left, 0 to 1; `seconds`: the animation's clock.
    pub fn update(&mut self, gpu: &Gpu, shares: [f32; 2], seconds: f32) {
        let step = (seconds * 20.0) as u32;
        for (made, (share, colors)) in self.made.iter_mut().zip(shares.into_iter().zip([LIFE, MANA])) {
            let share = share.clamp(0.0, 1.0);
            let key = ((share * ORB_RADIUS * 2.0 * SS) as u32, step);
            if made.as_ref().is_none_or(|(made_for, _)| *made_for != key) {
                *made = Some((key, gpu.texture(&paint([ORB_RADIUS * 2.0; 2], |x, y| liquid(colors, share, step as f32 / 20.0, x, y)), true)));
            }
        }
    }
}

/// What the bar shows besides the hotkeys.
pub struct Hud<'a> {
    pub state: &'a State,
    pub hp: [i64; 2],
    pub sp: [i64; 2],
    pub exp: [i64; 2],
    pub level: i64,
    /// The keys of the nine sockets, F1 to F9 by default.
    pub labels: Vec<String>,
}

/// True the first time it's asked: the first map since the client started.
pub fn first_map() -> bool {
    static SEEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    !SEEN.swap(true, std::sync::atomic::Ordering::Relaxed)
}

/// The bar's corner on a screen `width` wide: bottom middle.
pub fn corner(width: f32) -> [f32; 2] {
    [((width - WIDTH) / 2.0).round(), 768.0 - HEIGHT]
}

/// Socket `n`'s corner in the bar.
pub fn socket_corner(n: usize) -> [f32; 2] {
    [FIRST_SOCKET[0] + n as f32 * SOCKET_STEP, FIRST_SOCKET[1]]
}

/// The icon's corner in socket `n`.
pub fn icon_corner(n: usize) -> [f32; 2] {
    let [x, y] = socket_corner(n);
    let inset = (SOCKET - ICON) / 2.0;
    [x + inset, y + inset]
}

fn orb_centers() -> [[f32; 2]; 2] {
    [[ORB_BOX / 2.0, ORB_BOX / 2.0], [WIDTH - ORB_BOX / 2.0, ORB_BOX / 2.0]]
}

/// Whether a point in the bar is on it (an orb or the plate) rather than on the world seen past its corners.
pub fn covers(local: [f32; 2]) -> bool {
    let on_orb = orb_centers().iter().any(|[x, y]| (local[0] - x).hypot(local[1] - y) <= RING_OUTER);
    on_orb || in_panel(local[0] - PANEL[0], local[1] - PANEL[1])
}

fn in_panel(x: f32, y: f32) -> bool {
    let [w, h] = [PANEL[2], PANEL[3]];
    (0.0..w).contains(&x) && (0.0..h).contains(&y) && y >= CHAMFER - x.min(w - x)
}

/// The plate, sockets, experience bar and orbs, under the hotkeys' icons. `at` puts a bar point on the screen.
#[allow(clippy::too_many_arguments)]
pub fn draw<'a>(
    layer: &mut Layer<'a>,
    tooltip: &mut Layer<'a>,
    assets: &'a Assets,
    chrome: &'a Chrome,
    font: &Font,
    hud: &Hud<'a>,
    at: impl Fn([f32; 2]) -> [f32; 2],
    cursor: [f32; 2],
) {
    layer.sprites.push((&assets.panel, at([PANEL[0], PANEL[1]]), [PANEL[2], PANEL[3]]));
    for (n, label) in hud.labels.iter().enumerate().take(crate::hotkeys::PER_ROW) {
        let corner = at(socket_corner(n));
        layer.sprites.push((&assets.socket, corner, [SOCKET; 2]));
        if contains(corner, [SOCKET; 2], cursor) {
            layer.sprites.push((&assets.glow, corner, [SOCKET; 2]));
        }
        let word = label_word(label);
        hud.state.put(layer, &word, [(corner[0] + (SOCKET - hud.state.width(&word, font)) / 2.0).round(), corner[1] - 13.0]);
    }
    // Experience: a gold line filling its frame.
    let back = at([XP_BAR[0] - 2.0, XP_BAR[1] - 2.0]);
    layer.sprites.push((&assets.xp_back, back, [XP_BAR[2] + 4.0, XP_BAR[3] + 4.0]));
    let share = share(hud.exp);
    if share > 0.0 {
        layer.sprites.push((&assets.xp_fill, at([XP_BAR[0], XP_BAR[1]]), [XP_BAR[2] * share, XP_BAR[3]]));
    }
    if contains(back, [XP_BAR[2] + 4.0, XP_BAR[3] + 4.0], cursor) {
        let text = format!("Level {}  -  {:.1}% to the next", hud.level, share * 100.0);
        window::tooltip(tooltip, chrome, font, &text, [cursor[0] - font.width(&text) / 2.0, back[1] - 28.0], [255, 220, 150]);
    }
    // The orbs: liquid, the glass over it, the ring around both; their numbers when pointed at.
    for ((center, liquid), values) in orb_centers().into_iter().zip(&hud.state.made).zip([hud.hp, hud.sp]) {
        let corner = at([center[0] - ORB_RADIUS, center[1] - ORB_RADIUS]);
        if let Some((_, texture)) = liquid {
            layer.sprites.push((texture, corner, [ORB_RADIUS * 2.0; 2]));
        }
        layer.sprites.push((&assets.glass, corner, [ORB_RADIUS * 2.0; 2]));
        let box_corner = at([center[0] - ORB_BOX / 2.0, center[1] - ORB_BOX / 2.0]);
        layer.sprites.push((&assets.ring, box_corner, [ORB_BOX; 2]));
        let middle = at(center);
        if (cursor[0] - middle[0]).hypot(cursor[1] - middle[1]) <= RING_OUTER {
            let word = number_word(values);
            hud.state.put(layer, &word, [(middle[0] - hud.state.width(&word, font) / 2.0).round(), middle[1] - 8.0]);
        }
    }
}

/// The monster pointed at or fought: its name over a bar at the top middle.
pub fn draw_enemy<'a>(layer: &mut Layer<'a>, assets: &'a Assets, state: &'a State, font: &Font, name: &str, hp: Option<(i32, i32)>) {
    let x = ((screen()[0] - ENEMY_BAR[0]) / 2.0).round();
    layer.sprites.push((&assets.enemy_back, [x - 12.0, ENEMY_TOP - 4.0], [ENEMY_BAR[0] + 24.0, ENEMY_BAR[1] + 8.0]));
    let share = hp.map_or(1.0, |(hp, max)| share([i64::from(hp), i64::from(max)]));
    layer.sprites.push((&assets.enemy_fill, [x, ENEMY_TOP], [ENEMY_BAR[0] * share, ENEMY_BAR[1]]));
    let word = name_word(name);
    state.put(layer, &word, [(screen()[0] / 2.0 - state.width(&word, font) / 2.0).round(), ENEMY_TOP - 24.0]);
}

/// value / max, 0 to 1.
pub fn share([value, max]: [i64; 2]) -> f32 {
    if max <= 0 { 0.0 } else { (value as f32 / max as f32).clamp(0.0, 1.0) }
}

// Painting. Each painter takes a point in the texture's on-screen units and gives straight RGBA, 0 to 1.

pub(crate) fn paint(size: [f32; 2], pixel: impl Fn(f32, f32) -> [f32; 4]) -> RgbaImage {
    let [width, height] = size.map(|value| (value * SS).round() as u32);
    RgbaImage::from_fn(width, height, |x, y| {
        let color = pixel((x as f32 + 0.5) / SS, (y as f32 + 0.5) / SS);
        Rgba(color.map(|value| (value.clamp(0.0, 1.0) * 255.0).round() as u8))
    })
}

pub(crate) fn smooth(from: f32, to: f32, x: f32) -> f32 {
    let t = ((x - from) / (to - from)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t)
}

pub(crate) fn scale(color: [f32; 3], by: f32) -> [f32; 3] {
    color.map(|value| value * by)
}

pub(crate) fn rgba([r, g, b]: [f32; 3], a: f32) -> [f32; 4] {
    [r, g, b, a]
}

fn gradient(t: f32, top: [f32; 3], bottom: [f32; 3]) -> [f32; 4] {
    rgba(mix(top, bottom, t), 1.0)
}

/// Grain for metal and stone, 0 to 1.
fn grain(x: f32, y: f32) -> f32 {
    let n = ((x * 12.9898 + y * 78.233).sin() * 43758.547).fract();
    n.abs()
}

/// Light from the top left on a surface tilted toward (nx, ny): 0 to 1.
fn lit(nx: f32, ny: f32) -> f32 {
    (0.55 + 0.45 * (nx * -0.6 + ny * -0.8)).clamp(0.0, 1.0)
}

fn ring(x: f32, y: f32) -> [f32; 4] {
    let (dx, dy) = (x - ORB_BOX / 2.0, y - ORB_BOX / 2.0);
    let d = dx.hypot(dy).max(0.001);
    let inner = ORB_RADIUS - 1.0;
    let alpha = smooth(inner - 1.0, inner, d) * (1.0 - smooth(RING_OUTER - 0.75, RING_OUTER + 0.25, d));
    if alpha <= 0.0 {
        return [0.0; 4];
    }
    let t = ((d - inner) / (RING_OUTER - inner)).clamp(0.0, 1.0);
    // A rounded band: its inner side faces the middle, its outer side away.
    let tilt = (t * PI).cos();
    let light = lit(-dx / d * tilt, -dy / d * tilt);
    let mut color = scale(IRON, 0.7 + 1.6 * light + 0.25 * grain(x, y));
    // Bronze bands near both edges.
    if (0.10..0.24).contains(&t) || (0.80..0.92).contains(&t) {
        color = scale(BRONZE, 0.4 + 0.7 * light);
    }
    // Rivets around the middle of the band.
    let middle = (inner + RING_OUTER) / 2.0;
    for k in 0..8 {
        let angle = k as f32 * PI / 4.0 + PI / 8.0;
        let (rx, ry) = (x - (ORB_BOX / 2.0 + angle.cos() * middle), y - (ORB_BOX / 2.0 + angle.sin() * middle));
        let r = rx.hypot(ry);
        if r < 2.8 {
            let z = (1.0 - (r / 2.8).powi(2)).sqrt();
            color = scale(BRONZE_LIGHT, 0.35 + 0.65 * lit(rx / 2.8, ry / 2.8) * z + 0.1);
        }
    }
    rgba(color, alpha)
}

fn glass(x: f32, y: f32) -> [f32; 4] {
    let (u, v) = (x / ORB_RADIUS - 1.0, y / ORB_RADIUS - 1.0);
    let r = u.hypot(v);
    if r > 1.0 {
        return [0.0; 4];
    }
    // A soft crescent of window light near the top left, and the rim catching light at the bottom right.
    let ellipse = ((u + 0.22) / 0.55).powi(2) + ((v + 0.52) / 0.30).powi(2);
    let inner = ((u + 0.16) / 0.55).powi(2) + ((v + 0.40) / 0.30).powi(2);
    let shine = (1.0 - smooth(0.4, 1.0, ellipse)) * smooth(0.6, 1.2, inner) * 0.35 + smooth(0.82, 0.98, r) * (u * 0.4 + v * 0.8).max(0.0) * 0.3;
    let shadow = smooth(0.62, 1.0, r) * 0.65;
    let alpha = shine + shadow * (1.0 - shine);
    let white = if alpha > 0.0 { shine / alpha } else { 0.0 };
    rgba([white; 3], alpha * (1.0 - smooth(0.985, 1.0, r)))
}

fn liquid((deep, bright): ([f32; 3], [f32; 3]), share: f32, t: f32, x: f32, y: f32) -> [f32; 4] {
    let (u, v) = (x / ORB_RADIUS - 1.0, y / ORB_RADIUS - 1.0);
    let r2 = u * u + v * v;
    if r2 > 1.0 {
        return [0.0; 4];
    }
    let edge = 1.0 - smooth(0.97, 1.0, r2.sqrt());
    let z = (1.0 - r2).sqrt();
    let wave = if share > 0.0 && share < 1.0 { 0.035 * (u * 5.0 + t * 2.2).sin() + 0.02 * (u * 11.0 - t * 3.3).sin() } else { 0.0 };
    let surface = 1.0 - 2.0 * share + wave;
    if share <= 0.0 || v < surface {
        // Empty: dark, smoky glass with a hint of the colour.
        return rgba(scale(deep, 0.22 + 0.25 * z), edge);
    }
    let swirl = (u * 6.0 + (v * 4.0 + t * 0.8).sin() * 1.6 + t * 0.5).sin() * (v * 5.0 - t * 0.6 + (u * 3.0).cos()).sin();
    let light = (0.25 + 0.75 * z) * (0.85 + 0.22 * swirl);
    let mut color = mix(scale(deep, 0.6), bright, light.clamp(0.0, 1.0));
    // A lit line along the surface.
    let foam = 1.0 - smooth(0.0, 0.06, v - surface);
    color = mix(color, mix(bright, [1.0; 3], 0.35), foam * 0.7 * f32::from(share < 1.0));
    rgba(color, edge)
}

fn panel(x: f32, y: f32) -> [f32; 4] {
    let [w, h] = [PANEL[2], PANEL[3]];
    if !in_panel(x, y) {
        return [0.0; 4];
    }
    // Distance to the top edge, chamfers included, for the bronze trim along it.
    let top = (y - (CHAMFER - x.min(w - x)).max(0.0)).min(x).min(w - x);
    let stone = scale(IRON, 1.25 - 0.55 * (y / h) + 0.35 * grain(x, y));
    let color = if top < 1.0 {
        scale(BRONZE_LIGHT, 0.9)
    } else if top < 3.0 {
        scale(BRONZE, 0.75)
    } else if top < 4.5 {
        scale(IRON, 0.4)
    } else {
        stone
    };
    rgba(color, 0.97)
}

fn socket(x: f32, y: f32, glow: bool) -> [f32; 4] {
    // Distance from the nearest edge.
    let edge = x.min(y).min(SOCKET - x).min(SOCKET - y);
    if glow {
        let alpha = (1.0 - smooth(0.0, 6.0, edge)) * 0.9;
        return rgba([1.0, 0.78, 0.38], alpha);
    }
    let light = 1.0 - y / SOCKET;
    let color = if edge < 1.0 {
        [0.02; 3]
    } else if edge < 3.0 {
        scale(BRONZE, 0.55 + 0.7 * light)
    } else {
        // The hollow, darker toward its top edge where the lip shades it.
        scale([0.035, 0.03, 0.03], 0.6 + 0.8 * smooth(3.0, 10.0, y.min(x)))
    };
    rgba(color, 1.0)
}

fn frame_bar(size: [f32; 2], x: f32, y: f32) -> [f32; 4] {
    let edge = x.min(y).min(size[0] - x).min(size[1] - y);
    let color = if edge < 1.0 { scale(BRONZE, 0.8) } else { [0.03, 0.025, 0.02] };
    rgba(color, 1.0)
}

fn vignette(x: f32, y: f32) -> [f32; 4] {
    let (u, v) = (x / 64.0 - 1.0, y / 48.0 - 1.0);
    rgba([0.03, 0.015, 0.015], smooth(0.55, 1.3, u.hypot(v)) * 0.85)
}

/// A long plate with pointed ends.
fn enemy_back(x: f32, y: f32) -> [f32; 4] {
    let [w, h] = [ENEMY_BAR[0] + 24.0, ENEMY_BAR[1] + 8.0];
    let (dx, dy) = ((x - w / 2.0).abs(), (y - h / 2.0).abs());
    let inside = w / 2.0 - dy - dx;
    if inside < 0.0 {
        return [0.0; 4];
    }
    let rim = inside.min(h / 2.0 - dy);
    let color = if rim < 1.5 { scale(BRONZE_LIGHT, 0.85) } else { [0.05, 0.03, 0.03] };
    rgba(color, smooth(0.0, 0.8, inside))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sockets_sit_on_the_plate_between_the_orbs() {
        let first = socket_corner(0);
        let last = socket_corner(crate::hotkeys::PER_ROW - 1);
        assert!(first[0] >= ORB_BOX / 2.0 + RING_OUTER);
        assert!(last[0] + SOCKET <= WIDTH - ORB_BOX / 2.0 - RING_OUTER);
        assert!(covers([first[0] + 1.0, first[1] + 1.0]) && covers([last[0] + SOCKET - 1.0, last[1] + SOCKET - 1.0]));
        // The bar is centred: the plate and experience bar share its middle.
        assert_eq!(PANEL[0] + PANEL[2] / 2.0, WIDTH / 2.0);
        assert_eq!(XP_BAR[0] + XP_BAR[2] / 2.0, WIDTH / 2.0);
        assert_eq!(first[0] - PANEL[0], PANEL[0] + PANEL[2] - last[0] - SOCKET);
    }

    #[test]
    fn the_world_shows_past_the_bars_top_corners() {
        assert!(!covers([WIDTH / 2.0, 10.0]));
        assert!(covers([ORB_BOX / 2.0, 10.0]));
        assert!(!covers([PANEL[0] + 1.0, PANEL[1] + 1.0]) || ORB_BOX / 2.0 + RING_OUTER > PANEL[0] + 1.0);
    }

    #[test]
    fn shares_clamp() {
        assert_eq!(share([5, 10]), 0.5);
        assert_eq!(share([5, 0]), 0.0);
        assert_eq!(share([20, 10]), 1.0);
    }
}
