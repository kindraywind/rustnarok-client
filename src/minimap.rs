//! The minimap at the top right: the map's overview (see-through by default, Ctrl+Tab for opaque or hidden), zoomed
//! around the player, with the player's arrow, party and guild members, NPC marks, the boss and the coordinates.

use image::{Rgba, RgbaImage};
use rustnarok_client::grf::Grf;

use crate::render::{self, Gpu, Texture};
use crate::{Layer, UI, contains, screen};

const SIZE: f32 = 128.0;
/// The map square at (W - 144, 16); the bottom row at y 145.
fn origin() -> [f32; 2] {
    [screen()[0] - 144.0, 16.0]
}
const ROW: f32 = 145.0;
const BUTTON: f32 = 12.0;
/// The bottom row's buttons and their left edges from the screen's right: object, plus, minus, mini, view on.
const BUTTONS: [(&str, f32); 5] = [("object", 144.0), ("plus", 72.0), ("minus", 58.0), ("mini", 44.0), ("viewon", 30.0)];
/// The see-through overview's alpha (0xaa).
const SEE_THROUGH: u8 = 0xAA;
pub const MOST_ZOOM: f32 = 4.0;
/// The player arrow turned in this many steps around.
const TURNS: u32 = 32;
/// Party members' colours by their place in the party list (15 the last); past the table, grey.
const PARTY_COLORS: [u32; 16] = [
    0xff0a3278, 0xff007878, 0xff000000, 0xff780000, 0xff007800, 0xff787800, 0xff780078, 0xff783200, 0xff787878, 0xff000078, 0xff003232, 0xff320000, 0xff003200,
    0xff323200, 0xff320032, 0xff780000,
];
const PARTY_PAST: u32 = 0xff323232;

/// An ARGB colour as RGBA.
fn rgba(argb: u32) -> [u8; 4] {
    let [a, r, g, b] = argb.to_be_bytes();
    [r, g, b, a]
}

/// A party member's colour by their place in the party list.
pub fn party_color(place: usize) -> [u8; 4] {
    rgba(PARTY_COLORS.get(place).copied().unwrap_or(PARTY_PAST))
}

/// A compass mark shows half of each second.
pub fn blink_on(age_ms: u128) -> bool {
    age_ms % 1000 < 500
}

/// A mark set at `set_ms` shows at `now_ms`: on for 500ms from the moment it was set, then off, and so on.
pub fn mark_visible(set_ms: u128, now_ms: u128) -> bool {
    blink_on(now_ms.saturating_sub(set_ms))
}

/// Which of `i_<name>_1/2/3.bmp` to draw: 0 at rest, 1 under the pointer, 2 held down.
/// ponytail: `_1` at rest, `_2` under the pointer, `_3` pressed; a capture of the client would confirm which file is which.
pub fn button_frame(hovered: bool, pressed: bool) -> usize {
    if pressed {
        2
    } else if hovered {
        1
    } else {
        0
    }
}

/// `messages[2855]` through `[2859]`: object, plus, minus, mini, viewon.
pub fn tooltip_id(name: &str) -> usize {
    2855 + BUTTONS.iter().position(|(each, _)| *each == name).unwrap_or(0)
}

/// The pointer, for the bottom row's hover and press.
pub struct Pointer {
    pub at: [f32; 2],
    pub down: bool,
}

/// One zoom step in: z + (z/3 + z) * 0.25, at most 4.
pub fn zoom_in(zoom: f32) -> f32 {
    (zoom + (zoom / 3.0 + zoom) * 0.25).min(MOST_ZOOM)
}

/// One zoom step out: z * 0.75, at least 1.
pub fn zoom_out(zoom: f32) -> f32 {
    (zoom * 0.75).max(1.0)
}

/// A cell as a share of the overview (0 to 1 across and down; the longer side fills it, the other centred).
pub fn share(cell: [i32; 2], size: [u32; 2]) -> [f32; 2] {
    let longest = size[0].max(size[1]).max(1) as f32;
    let start = [(longest - size[0] as f32) / 2.0, (longest - size[1] as f32) / 2.0];
    [(start[0] + cell[0] as f32) / longest, 1.0 - (start[1] + cell[1] as f32) / longest]
}

/// The part of the overview showing: the player ± half of 1/zoom, shifted to stay inside it; its top left share.
pub fn window(player: [f32; 2], zoom: f32) -> [f32; 2] {
    let span = 1.0 / zoom.max(1.0);
    player.map(|at| (at - span / 2.0).clamp(0.0, 1.0 - span))
}

/// Where a share of the overview lands on screen through the window, or none outside it.
pub fn on_screen(at: [f32; 2], window: [f32; 2], zoom: f32) -> Option<[f32; 2]> {
    let local = [(at[0] - window[0]) * zoom, (at[1] - window[1]) * zoom];
    local.iter().all(|value| (0.0..=1.0).contains(value)).then(|| [origin()[0] + local[0] * SIZE, origin()[1] + local[1] * SIZE])
}

/// The overview's window as a picture: the overview's pixels it covers, at `alpha`.
pub fn crop(image: &RgbaImage, window: [f32; 2], zoom: f32, alpha: u8) -> RgbaImage {
    let [width, height] = [image.width() as f32, image.height() as f32];
    let span = 1.0 / zoom.max(1.0);
    let [x, y] = [(window[0] * width) as u32, (window[1] * height) as u32];
    let [w, h] = [((span * width) as u32).max(1).min(image.width() - x), ((span * height) as u32).max(1).min(image.height() - y)];
    let mut part = image::imageops::crop_imm(image, x, y, w, h).to_image();
    for pixel in part.pixels_mut() {
        pixel.0[3] = (u16::from(pixel.0[3]) * u16::from(alpha) / 255) as u8;
    }
    part
}

/// The cropped overview's key: the window in the picture's pixels and the view, so it's rebuilt only when they move.
pub fn crop_key(image: &RgbaImage, window: [f32; 2], zoom: f32, view: View) -> (u32, u32, u32, View) {
    ((window[0] * image.width() as f32) as u32, (window[1] * image.height() as f32) as u32, (zoom * 100.0) as u32, view)
}

/// The coordinates as the game writes them.
pub fn coordinates(cell: [i32; 2]) -> String {
    format!(" {}  {}", cell[0], cell[1])
}

/// A bottom-row button's box on screen (for the test driver).
pub fn button_box(name: &str) -> [f32; 4] {
    let from_right = BUTTONS.iter().find(|(each, _)| *each == name).map_or(0.0, |(_, from_right)| *from_right);
    [screen()[0] - from_right, ROW, BUTTON, BUTTON]
}

/// The bottom row's button under the pointer.
pub fn button_at(cursor: [f32; 2]) -> Option<&'static str> {
    BUTTONS.into_iter().find(|(_, from_right)| contains([screen()[0] - from_right, ROW], [BUTTON, BUTTON], cursor)).map(|(name, _)| name)
}

pub struct Assets {
    /// The player arrow turned TURNS ways.
    arrows: Vec<Texture>,
    /// Each bottom-row button at rest, under the pointer, and held (`i_<name>_1/2/3.bmp`).
    buttons: Vec<[Texture; 3]>,
    arrow_shadow: Texture,
    dot_back: Texture,
    guild: Texture,
    guild_ring: Texture,
    /// The map's boss, where the Convex Mirror shows it.
    boss: Texture,
    /// A navigation's trail dots and where it heads.
    trail: Texture,
    goal: Texture,
}

/// Turns an image by `angle` radians around its center (nearest neighbor), keeping its size.
fn rotate(image: &RgbaImage, angle: f32) -> RgbaImage {
    let (sin, cos) = angle.sin_cos();
    let [width, height] = [image.width(), image.height()];
    let [cx, cy] = [width as f32 / 2.0, height as f32 / 2.0];
    RgbaImage::from_fn(width, height, |x, y| {
        // Sample the source point that lands here.
        let [dx, dy] = [x as f32 + 0.5 - cx, y as f32 + 0.5 - cy];
        let [sx, sy] = [cos * dx + sin * dy + cx, -sin * dx + cos * dy + cy];
        if sx < 0.0 || sy < 0.0 || sx >= width as f32 || sy >= height as f32 { Rgba([0; 4]) } else { *image.get_pixel(sx as u32, sy as u32) }
    })
}

/// A filled circle of `radius` in `color`, on a square just big enough.
fn disc(radius: u32, color: [u8; 4]) -> RgbaImage {
    let side = radius * 2 + 1;
    RgbaImage::from_fn(side, side, |x, y| {
        let [dx, dy] = [x as i32 - radius as i32, y as i32 - radius as i32];
        if dx * dx + dy * dy <= (radius * radius) as i32 { Rgba(color) } else { Rgba([0; 4]) }
    })
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let arrow = render::load_image(grf, &format!("{UI}map\\map_arrow.bmp")).unwrap_or_else(|_| RgbaImage::new(1, 1));
        let image = |path: String| gpu.try_load_texture(grf, &path, false).unwrap_or_else(|_| gpu.solid([0, 0, 0, 0]));
        Self {
            arrows: (0..TURNS).map(|turn| gpu.texture(&rotate(&arrow, turn as f32 * std::f32::consts::TAU / TURNS as f32), false)).collect(),
            buttons: BUTTONS.iter().map(|(name, _)| [1, 2, 3].map(|state| image(format!("{UI}minimap\\i_{name}_{state}.bmp")))).collect(),
            arrow_shadow: gpu.solid(rgba(0x77333333)),
            dot_back: gpu.solid(rgba(0x99ffffff)),
            guild: gpu.texture(&disc(2, rgba(0xffffa6d2)), false),
            guild_ring: gpu.texture(&disc(3, rgba(0xaaffffff)), false),
            boss: image(format!("{UI}map\\bossmonster.bmp")),
            trail: gpu.texture(&disc(1, rgba(0xffff4040)), false),
            goal: gpu.texture(&disc(3, rgba(0xffff2020)), false),
        }
    }

    /// The player arrow for the camera's yaw (180° less the yaw, in TURNS steps).
    pub fn player_arrow(&self, yaw: f32) -> &Texture {
        let turn = ((180.0 - yaw).rem_euclid(360.0) / 360.0 * TURNS as f32).round() as usize % TURNS as usize;
        &self.arrows[turn]
    }

    pub fn arrow_shadow(&self) -> &Texture {
        &self.arrow_shadow
    }

    pub fn guild_ring(&self) -> &Texture {
        &self.guild_ring
    }

    pub fn guild_dot(&self) -> &Texture {
        &self.guild
    }

    pub fn party_back(&self) -> &Texture {
        &self.dot_back
    }

    pub fn boss_mark(&self) -> &Texture {
        &self.boss
    }
}

/// Ctrl+Tab walks the overview: opaque, see-through, hidden; it starts see-through.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Hash)]
pub enum View {
    Shown,
    #[default]
    Translucent,
    Hidden,
}

impl View {
    pub fn next(self) -> Self {
        match self {
            Self::Shown => Self::Translucent,
            Self::Translucent => Self::Hidden,
            Self::Hidden => Self::Shown,
        }
    }

    pub fn alpha(self) -> u8 {
        if self == Self::Shown { 255 } else { SEE_THROUGH }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Shown => "opaque",
            Self::Translucent => "see_through",
            Self::Hidden => "hidden",
        }
    }

    /// The text `save_view` writes. Anything else is left unread.
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim() {
            "opaque" => Some(Self::Shown),
            "see_through" => Some(Self::Translucent),
            "hidden" => Some(Self::Hidden),
            _ => None,
        }
    }
}

/// The see-through mode last saved (`minimap_view`, next to `minimap_zoom`), or see-through when there is none.
pub fn saved_view() -> View {
    crate::settings_path("minimap_view").and_then(|path| std::fs::read_to_string(path).ok()).and_then(|text| View::parse(&text)).unwrap_or_default()
}

/// Keeps the see-through mode with the settings, as the game keeps `M_MINIMAPARGB`.
pub fn save_view(view: View) {
    if let Some(path) = crate::settings_path("minimap_view") {
        let _ = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(path, view.name()));
    }
}

/// The overview picture of a map (data\texture\유저인터페이스\map\<map>.bmp), if the client has one.
pub fn load_map(grf: &Grf, map: &str) -> Option<RgbaImage> {
    let name = map.trim_end_matches(".gat").trim_end_matches(".rsw");
    render::load_image(grf, &format!("{UI}map\\{name}.bmp")).ok()
}

/// What the minimap draws besides the overview: the player's cell and the arrow's turn (180° less the camera's yaw),
/// NPC marks and party members (a cell and a texture of their colour), guild members, the boss, and a navigation's
/// trail (cells) and where it heads on this map.
pub struct Markers<'a> {
    pub cell: [i32; 2],
    pub yaw: f32,
    pub marks: &'a [([i32; 2], &'a Texture)],
    pub party: &'a [([i32; 2], &'a Texture)],
    pub guild: &'a [[i32; 2]],
    pub boss: Option<[i32; 2]>,
    pub trail: &'a [[i32; 2]],
    pub goal: Option<[i32; 2]>,
}

/// Draws the minimap: the overview's window (already cropped, `overview`), then the markers through it, then the
/// bottom row; nothing of the map while hidden. A button uses its hover image under `pointer` and its pressed image
/// while the button is held there.
pub fn draw<'a>(layer: &mut Layer<'a>, assets: &'a Assets, overview: Option<&'a Texture>, size: [u32; 2], zoom: f32, markers: &Markers<'a>, pointer: Pointer) {
    let player = share(markers.cell, size);
    let window = window(player, zoom);
    if let Some(overview) = overview {
        layer.sprites.push((overview, origin(), [SIZE, SIZE]));
        let at = |cell: [i32; 2]| on_screen(share(cell, size), window, zoom).map(|point| point.map(f32::round));
        // A navigation's trail, a dot every few cells, and a disc where it heads.
        // ponytail: the client's own route look on the minimap isn't known; red dots.
        for &cell in markers.trail.iter().step_by(crate::navigation::TRAIL_STEP) {
            let Some([x, y]) = at(cell) else { continue };
            layer.sprites.push((&assets.trail, [x - 1.0, y - 1.0], assets.trail.size));
        }
        if let Some([x, y]) = markers.goal.and_then(at) {
            layer.sprites.push((&assets.goal, [x - 3.0, y - 3.0], assets.goal.size));
        }
        // The cross: 8x2 and 2x8.
        for &(cell, color) in markers.marks {
            let Some([x, y]) = at(cell) else { continue };
            layer.sprites.push((color, [x - 1.0, y - 4.0], [2.0, 8.0]));
            layer.sprites.push((color, [x - 4.0, y - 1.0], [8.0, 2.0]));
        }
        // Guild members: a pink circle in a white ring.
        for &cell in markers.guild {
            let Some([x, y]) = at(cell) else { continue };
            layer.sprites.push((&assets.guild_ring, [x - 3.0, y - 3.0], assets.guild_ring.size));
            layer.sprites.push((&assets.guild, [x - 2.0, y - 2.0], assets.guild.size));
        }
        // Party members: 4x4 of their colour on a 6x6 white.
        for &(cell, color) in markers.party {
            let Some([x, y]) = at(cell) else { continue };
            layer.sprites.push((&assets.dot_back, [x - 3.0, y - 3.0], [6.0, 6.0]));
            layer.sprites.push((color, [x - 2.0, y - 2.0], [4.0, 4.0]));
        }
        // The boss, kept at the edge when outside.
        if let Some(boss) = markers.boss {
            let local = [(share(boss, size)[0] - window[0]) * zoom, (share(boss, size)[1] - window[1]) * zoom].map(|value| value.clamp(0.0, 1.0));
            let [x, y] = [origin()[0] + local[0] * SIZE, origin()[1] + local[1] * SIZE];
            layer.sprites.push((&assets.boss, [(x - 6.0).round(), (y - 6.0).round()], [12.0, 12.0]));
        }
        // The player: the arrow turned with the camera over its shadow.
        if let Some([x, y]) = at(markers.cell) {
            layer.sprites.push((&assets.arrow_shadow, [x - 7.0, y - 7.0], [14.0, 16.0]));
            let turn = ((180.0 - markers.yaw).rem_euclid(360.0) / 360.0 * TURNS as f32).round() as usize % TURNS as usize;
            let arrow = &assets.arrows[turn];
            layer.sprites.push((arrow, [x - 5.0, y - 5.0], [10.0, 10.0]));
        }
    }
    // The bottom row: the buttons and the coordinates, black under white.
    let over = button_at(pointer.at);
    for ((name, from_right), frames) in BUTTONS.iter().zip(&assets.buttons) {
        let hovered = over == Some(*name);
        let button = &frames[button_frame(hovered, hovered && pointer.down)];
        layer.sprites.push((button, [screen()[0] - from_right, ROW], [BUTTON, BUTTON]));
    }
    let text = coordinates(markers.cell);
    layer.texts.push((text.clone(), [screen()[0] - 126.0, ROW], [0, 0, 0]));
    layer.texts.push((text, [screen()[0] - 125.0, ROW], [255; 3]));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turns_around() {
        // A dot right of center goes below it after a quarter turn clockwise.
        let mut image = RgbaImage::new(9, 9);
        image.put_pixel(7, 4, Rgba([255; 4]));
        let turned = rotate(&image, std::f32::consts::FRAC_PI_2);
        assert_eq!(turned.get_pixel(4, 7).0, [255; 4]);
        assert_eq!(rotate(&image, 0.0), image);
    }

    #[test]
    fn zooms_by_the_games_steps() {
        assert_eq!(zoom_in(1.0), 1.0 + (1.0 / 3.0 + 1.0) * 0.25);
        let mut zoom = 1.0;
        for _ in 0..20 {
            zoom = zoom_in(zoom);
        }
        assert_eq!(zoom, MOST_ZOOM);
        assert_eq!(zoom_out(2.0), 1.5);
        assert_eq!(zoom_out(1.2), 1.0, "never under 1");
    }

    #[test]
    fn places_markers_through_the_window() {
        // A 400x300 map: the 300 is centred in the 400 square.
        assert_eq!(share([200, 150], [400, 300]), [0.5, 0.5]);
        assert_eq!(share([0, 0], [400, 300]), [0.0, 0.875]);
        // Zoomed ×2 the window is a half, around the player, kept inside.
        assert_eq!(window([0.5, 0.5], 2.0), [0.25, 0.25]);
        assert_eq!(window([0.1, 0.95], 2.0), [0.0, 0.5]);
        assert_eq!(window([0.3, 0.3], 1.0), [0.0, 0.0]);
        // The player at the middle of a ×2 window lands at the middle of the square; outside it, nowhere.
        assert_eq!(on_screen([0.5, 0.5], [0.25, 0.25], 2.0), Some([origin()[0] + 64.0, origin()[1] + 64.0]));
        assert_eq!(on_screen([0.9, 0.5], [0.25, 0.25], 2.0), None);
        assert_eq!(coordinates([156, 180]), " 156  180");
        assert_eq!(button_at([screen()[0] - 70.0, ROW + 5.0]), Some("plus"));
        assert_eq!(button_at([screen()[0] - 56.0, ROW + 5.0]), Some("minus"));
    }

    #[test]
    fn colours_party_members_and_blinks() {
        assert_eq!(party_color(0), [0x0a, 0x32, 0x78, 0xff]);
        assert_eq!(party_color(2), [0, 0, 0, 0xff]);
        assert_eq!(party_color(15), [0x78, 0, 0, 0xff]);
        assert_eq!(party_color(16), [0x32, 0x32, 0x32, 0xff]);
        assert!(blink_on(0) && blink_on(1499) && !blink_on(500) && !blink_on(1999));
        // Each mark keeps its own clock: one set at 0 is off at 600, one set at 500 is still on.
        assert!(mark_visible(0, 0) && mark_visible(0, 499) && !mark_visible(0, 500));
        assert!(mark_visible(500, 600) && !mark_visible(0, 600));
        assert!(mark_visible(0, 1100) && !mark_visible(500, 1100));
        assert_eq!((button_frame(false, false), button_frame(true, false), button_frame(true, true)), (0, 1, 2));
        assert_eq!((tooltip_id("object"), tooltip_id("plus"), tooltip_id("minus"), tooltip_id("mini"), tooltip_id("viewon")), (2855, 2856, 2857, 2858, 2859));
        // A see-through crop keeps two thirds of the alpha.
        let image = RgbaImage::from_pixel(8, 8, Rgba([1, 2, 3, 255]));
        let part = crop(&image, [0.5, 0.5], 2.0, SEE_THROUGH);
        assert_eq!((part.width(), part.height(), part.get_pixel(0, 0).0[3]), (4, 4, SEE_THROUGH));
    }

    #[test]
    fn the_see_through_mode_survives_a_reload() {
        assert_eq!(View::parse(View::Hidden.name()), Some(View::Hidden));
        assert_eq!(View::parse(View::Shown.name()), Some(View::Shown));
        assert_eq!(View::parse(View::Translucent.name()), Some(View::Translucent));
        assert_eq!(View::parse("nope"), None);
        let path = std::env::temp_dir().join(format!("rustnarok-minimap-view-{}", std::process::id()));
        std::fs::write(&path, View::Hidden.name()).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(View::parse(&text), Some(View::Hidden));
        let _ = std::fs::remove_file(&path);
    }
}
