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
/// The player arrow turned in whole degrees around.
const TURNS: u32 = 360;
const ARROW_SHADOW: u32 = 0x77333333;
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

/// The clockwise angle in whole degrees for the player's facing `rot` in degrees (`trunc(180 - rot)`).
pub fn arrow_angle(rot: f32) -> i32 {
    (180.0 - rot).trunc() as i32
}

/// The arrow's step in `0..360` for the player's facing `rot` in degrees.
pub fn arrow_turn(rot: f32) -> usize {
    arrow_angle(rot).rem_euclid(TURNS as i32) as usize
}

/// The 10×10 box `(x - 5, y - 5)..(x + 5, y + 5)` for the player arrow.
pub fn arrow_box([x, y]: [f32; 2]) -> ([f32; 2], [f32; 2]) {
    ([x - 5.0, y - 5.0], [10.0, 10.0])
}

/// The 14×16 box `(x - 7, y - 7)..(x + 7, y + 9)` for the arrow's shadow (centred 1 px below the arrow).
pub fn shadow_box([x, y]: [f32; 2]) -> ([f32; 2], [f32; 2]) {
    ([x - 7.0, y - 7.0], [14.0, 16.0])
}

/// Tints an arrow texel with `0x77333333` (RGB × #333333, alpha × 0x77 / 255).
pub fn shadow_pixel(pixel: [u8; 4]) -> [u8; 4] {
    let tint = rgba(ARROW_SHADOW);
    std::array::from_fn(|i| (u16::from(pixel[i]) * u16::from(tint[i]) / 255) as u8)
}

/// The arrow bitmap tinted with `0x77333333` for its shadow.
pub fn shadow_image(image: &RgbaImage) -> RgbaImage {
    RgbaImage::from_fn(image.width(), image.height(), |x, y| Rgba(shadow_pixel(image.get_pixel(x, y).0)))
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

/// Where a share of the overview lands on screen through the window, in or out of the square.
fn through(at: [f32; 2], window: [f32; 2], zoom: f32) -> [f32; 2] {
    [origin()[0] + (at[0] - window[0]) * zoom * SIZE, origin()[1] + (at[1] - window[1]) * zoom * SIZE]
}

/// A mark's box at screen point `at` with half-size `half`: (x − s, y − s) to (x + s, y + s), corners truncated; as
/// [left, top, right, bottom].
pub fn mark_box([x, y]: [f32; 2], half: f32) -> [f32; 4] {
    [(x - half).trunc(), (y - half).trunc(), (x + half).trunc(), (y + half).trunc()]
}

/// Whether all of a box is inside the square: a plain mark (party, guild, compass) is drawn whole or not at all.
pub fn inside([left, top, right, bottom]: [f32; 4]) -> bool {
    let [x, y] = origin();
    left >= x && top >= y && right <= x + SIZE && bottom <= y + SIZE
}

/// A box pushed back inside the square, and the side it touched: left 1, right 2, top 3, bottom 4 (left or right wins
/// over top or bottom); 0 when it was inside.
pub fn push_in(area: [f32; 4]) -> ([f32; 4], u8) {
    let [x, y] = origin();
    let [left, top, right, bottom] = area;
    let [width, height] = [right - left, bottom - top];
    let side = if left < x {
        1
    } else if right > x + SIZE {
        2
    } else if top < y {
        3
    } else if bottom > y + SIZE {
        4
    } else {
        0
    };
    let left = left.clamp(x, x + SIZE - width);
    let top = top.clamp(y, y + SIZE - height);
    ([left, top, left + width, top + height], side)
}

/// An NPC's quest mark from ZC_QUEST_NOTIFY_EFFECT: one per NPC, replaced by the next; effect 9999 (QTYPE_NONE) takes
/// it away. Colour 0 is kept but draws nothing.
pub fn set_quest_mark(marks: &mut Vec<(u32, [i32; 2], u16, u16)>, npc: u32, cell: [i32; 2], effect: u16, color: u16) {
    marks.retain(|(each, ..)| *each != npc);
    if effect != 9999 {
        marks.push((npc, cell, effect, color));
    }
}

/// A compass mark (ZC_COMPASS) by its id alone: a new one replaces the mark with that id whichever NPC sent it; type 2
/// only removes it.
pub fn set_compass_mark<T>(marks: &mut Vec<(u8, T)>, id: u8, kind: u32, mark: T) {
    marks.retain(|(each, _)| *each != id);
    if kind != 2 {
        marks.push((id, mark));
    }
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

/// The window, 128×140 at (W − 145, 17): a press anywhere on it is the minimap's, never the world's.
pub fn window_box() -> [f32; 4] {
    [screen()[0] - 145.0, 17.0, 128.0, 140.0]
}

pub fn covers(cursor: [f32; 2]) -> bool {
    let [x, y, w, h] = window_box();
    contains([x, y], [w, h], cursor)
}

/// Whether the minimap is there at all: not on an indoor map, not hidden.
pub fn shown(view: View, indoor: bool) -> bool {
    !indoor && view != View::Hidden
}

/// The bottom row's button under the pointer.
pub fn button_at(cursor: [f32; 2]) -> Option<&'static str> {
    BUTTONS.into_iter().find(|(_, from_right)| contains([screen()[0] - from_right, ROW], [BUTTON, BUTTON], cursor)).map(|(name, _)| name)
}

pub struct Assets {
    /// The player arrow turned TURNS ways (whole degrees).
    arrows: Vec<Texture>,
    /// The arrow's shadow (`map_arrow.bmp` tinted `0x77333333`) turned the same ways.
    arrow_shadows: Vec<Texture>,
    /// Each bottom-row button at rest, under the pointer, and held (`i_<name>_1/2/3.bmp`).
    buttons: Vec<[Texture; 3]>,
    dot_back: Texture,
    guild: Texture,
    guild_ring: Texture,
    /// The map's boss, where the Convex Mirror shows it.
    boss: Texture,
    /// A navigation's trail dots and where it heads.
    trail: Texture,
    goal: Texture,
    /// Quest marks by colour (1 yellow, 2 green, 3 purple), the store search mark, and the edge arrows by side (left,
    /// right, top, bottom).
    quests: [Texture; 3],
    store: Texture,
    edge_arrows: [Texture; 4],
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
        let shadow = shadow_image(&arrow);
        let image = |path: String| gpu.try_load_texture(grf, &path, false).unwrap_or_else(|_| gpu.solid([0, 0, 0, 0]));
        let turns = |source: &RgbaImage| (0..TURNS).map(|turn| gpu.texture(&rotate(source, (turn as f32).to_radians()), false)).collect();
        Self {
            arrows: turns(&arrow),
            arrow_shadows: turns(&shadow),
            buttons: BUTTONS.iter().map(|(name, _)| [1, 2, 3].map(|state| image(format!("{UI}minimap\\i_{name}_{state}.bmp")))).collect(),
            dot_back: gpu.solid(rgba(0x99ffffff)),
            guild: gpu.texture(&disc(2, rgba(0xffffa6d2)), false),
            guild_ring: gpu.texture(&disc(3, rgba(0xaaffffff)), false),
            boss: image(format!("{UI}map\\bossmonster.bmp")),
            trail: gpu.texture(&disc(1, rgba(0xffff4040)), false),
            goal: gpu.texture(&disc(3, rgba(0xffff2020)), false),
            quests: [1, 2, 3].map(|color| image(format!("{UI}basic_interface\\quest_{color}.bmp"))),
            store: image(format!("{UI}basic_interface\\search_store.bmp")),
            edge_arrows: [1, 2, 3, 4].map(|side| image(format!("{UI}basic_interface\\quest_arrow{side}.bmp"))),
        }
    }

    /// The player arrow for the player's facing `rot` in degrees (`trunc(180 - rot)` whole degrees clockwise).
    pub fn player_arrow(&self, rot: f32) -> &Texture {
        &self.arrows[arrow_turn(rot)]
    }

    /// The arrow's shadow for the player's facing `rot` in degrees.
    pub fn arrow_shadow(&self, rot: f32) -> &Texture {
        &self.arrow_shadows[arrow_turn(rot)]
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

    /// The mode the next run starts in: opaque stays, see-through and hidden come back see-through.
    pub fn kept(self) -> Self {
        if self == Self::Shown { Self::Shown } else { Self::Translucent }
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

/// The mode last saved (`minimap_view`, next to `minimap_zoom`), as `View::kept`; see-through when there is none.
pub fn saved_view() -> View {
    crate::settings_path("minimap_view").and_then(|path| std::fs::read_to_string(path).ok()).and_then(|text| View::parse(&text)).unwrap_or_default().kept()
}

/// Keeps the mode with the settings, as the game keeps `M_MINIMAPARGB`: hidden is never saved.
pub fn save_view(view: View) {
    if let Some(path) = crate::settings_path("minimap_view") {
        let _ = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(path, view.kept().name()));
    }
}

/// The overview picture of a map (data\texture\유저인터페이스\map\<map>.bmp), if the client has one.
pub fn load_map(grf: &Grf, map: &str) -> Option<RgbaImage> {
    let name = map.trim_end_matches(".gat").trim_end_matches(".rsw");
    render::load_image(grf, &format!("{UI}map\\{name}.bmp")).ok()
}

/// What the minimap draws besides the overview: the player's cell and facing in degrees (`yaw`, `Actor::rot`),
/// NPC marks and party members (a cell and a texture of their colour), guild members, the boss, and a navigation's
/// trail (cells) and where it heads on this map.
pub struct Markers<'a> {
    pub cell: [i32; 2],
    pub yaw: f32,
    pub marks: &'a [([i32; 2], &'a Texture)],
    /// NPCs' quest marks (cell, colour) and the store search mark.
    pub quests: &'a [([i32; 2], u16)],
    pub store: Option<[i32; 2]>,
    pub party: &'a [([i32; 2], &'a Texture)],
    pub guild: &'a [[i32; 2]],
    pub boss: Option<[i32; 2]>,
    pub trail: &'a [[i32; 2]],
    pub goal: Option<[i32; 2]>,
}

/// Draws the minimap: the overview's window (already cropped, `overview`; none when the map has no picture), then the
/// markers through it, then the bottom row. Indoors or hidden it isn't drawn at all (`shown`). A button uses its hover
/// image under `pointer` and its pressed image while the button is held there.
pub fn draw<'a>(layer: &mut Layer<'a>, assets: &'a Assets, overview: Option<&'a Texture>, size: [u32; 2], zoom: f32, markers: &Markers<'a>, pointer: Pointer) {
    let player = share(markers.cell, size);
    let window = window(player, zoom);
    if let Some(overview) = overview {
        layer.sprites.push((overview, origin(), [SIZE, SIZE]));
    }
    // A cell's screen point; a cell with x or y under 0 is never drawn.
    let point = |cell: [i32; 2]| (cell[0] >= 0 && cell[1] >= 0).then(|| through(share(cell, size), window, zoom));
    let place = |area: [f32; 4]| ([area[0], area[1]], [area[2] - area[0], area[3] - area[1]]);
    // A plain mark: drawn whole inside the square or not at all.
    let plain = |cell: [i32; 2], half: f32| point(cell).map(|at| mark_box(at, half)).filter(|area| inside(*area));
    // A navigation's trail, a dot every few cells, and a disc where it heads.
    // ponytail: the client's own route look on the minimap isn't known; red dots.
    for &cell in markers.trail.iter().step_by(crate::navigation::TRAIL_STEP) {
        if let Some(area) = plain(cell, 1.0) {
            layer.sprites.push((&assets.trail, [area[0], area[1]], assets.trail.size));
        }
    }
    if let Some(area) = markers.goal.and_then(|cell| plain(cell, 3.0)) {
        layer.sprites.push((&assets.goal, [area[0], area[1]], assets.goal.size));
    }
    // The player: the arrow turned by the player's facing in whole degrees, over its arrow-shaped shadow, never left out.
    if let Some(point) = point(markers.cell).map(|at| at.map(f32::round)) {
        let (shadow_at, shadow_size) = shadow_box(point);
        layer.sprites.push((assets.arrow_shadow(markers.yaw), shadow_at, shadow_size));
        let (arrow_at, arrow_size) = arrow_box(point);
        layer.sprites.push((assets.player_arrow(markers.yaw), arrow_at, arrow_size));
    }
    // The boss, all of its 12×12 kept inside the square.
    if let Some(at) = markers.boss.and_then(point) {
        let (area, _) = push_in(mark_box(at, 6.0));
        let (corner, extent) = place(area);
        layer.sprites.push((&assets.boss, corner, extent));
    }
    // Party members: their colour on a white back.
    for &(cell, color) in markers.party {
        if let Some(area) = plain(cell, 2.0) {
            layer.sprites.push((&assets.dot_back, [area[0] - 1.0, area[1] - 1.0], [6.0, 6.0]));
            layer.sprites.push((color, [area[0], area[1]], [4.0, 4.0]));
        }
    }
    // Guild members: a pink circle in a white ring.
    for &cell in markers.guild {
        if let Some(area) = plain(cell, 2.0) {
            layer.sprites.push((&assets.guild_ring, [area[0] - 1.0, area[1] - 1.0], assets.guild_ring.size));
            layer.sprites.push((&assets.guild, [area[0], area[1]], assets.guild.size));
        }
    }
    // Quest marks and the store mark: kept at the edge, as an arrow toward them once pushed.
    let edge = |picture: &'a Texture, at: [f32; 2], layer: &mut Layer<'a>| {
        let (area, side) = push_in(mark_box(at, 4.0));
        let image = if side == 0 { picture } else { &assets.edge_arrows[usize::from(side) - 1] };
        let (corner, extent) = place(area);
        layer.sprites.push((image, corner, extent));
    };
    for &(cell, color) in markers.quests {
        if let (Some(at), 1..=3) = (point(cell), color) {
            edge(&assets.quests[usize::from(color) - 1], at, layer);
        }
    }
    if let Some(at) = markers.store.and_then(point) {
        edge(&assets.store, at, layer);
    }
    // Compass marks: a cross, 8×2 and 2×8.
    for &(cell, color) in markers.marks {
        if let Some(area) = plain(cell, 1.0) {
            let [x, y] = [area[0] + 1.0, area[1] + 1.0];
            layer.sprites.push((color, [x - 1.0, y - 4.0], [2.0, 8.0]));
            layer.sprites.push((color, [x - 4.0, y - 1.0], [8.0, 2.0]));
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
    fn turns_the_player_arrow_by_facing_in_whole_degrees_over_its_shadow() {
        // Facing north (180) -> 0°, east (90) -> 90°, south (0) -> 180°, west (270) -> -90° (turn 270).
        assert_eq!((arrow_angle(180.0), arrow_turn(180.0)), (0, 0));
        assert_eq!((arrow_angle(90.0), arrow_turn(90.0)), (90, 90));
        assert_eq!((arrow_angle(0.0), arrow_turn(0.0)), (180, 180));
        assert_eq!((arrow_angle(270.0), arrow_turn(270.0)), (-90, 270));
        // Truncation to whole degrees: 180 - 179.6 = 0.4 -> 0°, 180 - 90.4 = 89.6 -> 89°, 180 - 270.5 = -90.5 -> -90°.
        assert_eq!((arrow_angle(179.6), arrow_turn(179.6)), (0, 0));
        assert_eq!((arrow_angle(90.4), arrow_turn(90.4)), (89, 89));
        assert_eq!((arrow_angle(270.5), arrow_turn(270.5)), (-90, 270));
        assert_eq!(TURNS, 360);
        // An upward tip at (4, 1) on a 9×9 arrow lands at the top for north (180), right for east (90),
        // bottom for south (0), and left for west (270).
        let mut arrow = RgbaImage::new(9, 9);
        arrow.put_pixel(4, 1, Rgba([255; 4]));
        let turned = |rot: f32| rotate(&arrow, (arrow_turn(rot) as f32).to_radians());
        assert_eq!(turned(180.0).get_pixel(4, 1).0, [255; 4]);
        assert_eq!(turned(90.0).get_pixel(7, 4).0, [255; 4]);
        assert_eq!(turned(0.0).get_pixel(4, 7).0, [255; 4]);
        assert_eq!(turned(270.0).get_pixel(1, 4).0, [255; 4]);
        // The shadow is the same arrow bitmap tinted 0x77333333 in (x - 7, y - 7)..(x + 7, y + 9), 1 px below the 10×10 arrow.
        assert_eq!(arrow_box([100.0, 80.0]), ([95.0, 75.0], [10.0, 10.0]));
        assert_eq!(shadow_box([100.0, 80.0]), ([93.0, 73.0], [14.0, 16.0]));
        assert_eq!(shadow_pixel([255, 255, 255, 255]), [0x33, 0x33, 0x33, 0x77]);
        assert_eq!(shadow_pixel([200, 100, 50, 255]), [40, 20, 10, 0x77]);
        assert_eq!(shadow_pixel([255, 0, 255, 0]), [0x33, 0, 0x33, 0]);
        let shadow = rotate(&shadow_image(&arrow), (arrow_turn(90.0) as f32).to_radians());
        assert_eq!(shadow.get_pixel(7, 4).0, [0x33, 0x33, 0x33, 0x77]);
        assert_eq!(shadow.get_pixel(0, 0).0, [0, 0, 0, 0]);
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
        // The player at the middle of a ×2 window lands at the middle of the square; a mark past it isn't inside.
        assert_eq!(through([0.5, 0.5], [0.25, 0.25], 2.0), [origin()[0] + 64.0, origin()[1] + 64.0]);
        assert!(!inside(mark_box(through([0.9, 0.5], [0.25, 0.25], 2.0), 2.0)));
        assert_eq!(coordinates([156, 180]), " 156  180");
        assert_eq!(button_at([screen()[0] - 70.0, ROW + 5.0]), Some("plus"));
        assert_eq!(button_at([screen()[0] - 56.0, ROW + 5.0]), Some("minus"));
    }

    #[test]
    fn keeps_marks_whole_or_at_the_edge() {
        let [x, y] = origin();
        // A box with truncated corners; inside whole, or not.
        assert_eq!(mark_box([x + 10.7, y + 20.2], 2.0), [x + 8.0, y + 18.0, x + 12.0, y + 22.0].map(f32::trunc));
        assert!(inside(mark_box([x + 2.0, y + 64.0], 2.0)));
        assert!(!inside(mark_box([x + 1.5, y + 64.0], 2.0)), "a dot crossing the edge is gone whole");
        // Pushed back in: the side, and a corner where left wins over top.
        let (area, side) = push_in(mark_box([x - 30.0, y + 50.0], 4.0));
        assert_eq!((area[0], area[2] - area[0], side), (x, 8.0, 1));
        assert_eq!(push_in(mark_box([x + 200.0, y + 50.0], 4.0)).1, 2);
        assert_eq!(push_in(mark_box([x + 50.0, y - 9.0], 4.0)).1, 3);
        assert_eq!(push_in(mark_box([x + 50.0, y + 300.0], 4.0)), ([x + 46.0, y + 120.0, x + 54.0, y + 128.0], 4));
        assert_eq!(push_in(mark_box([x - 5.0, y - 5.0], 4.0)), ([x, y, x + 8.0, y + 8.0], 1));
        assert_eq!(push_in(mark_box([x + 50.0, y + 50.0], 4.0)).1, 0);
    }

    #[test]
    fn keys_quest_and_compass_marks() {
        let mut quests = Vec::new();
        set_quest_mark(&mut quests, 7, [150, 170], 1, 2);
        set_quest_mark(&mut quests, 8, [10, 10], 1, 0);
        set_quest_mark(&mut quests, 7, [151, 171], 2, 3);
        assert_eq!(quests, [(8, [10, 10], 1, 0), (7, [151, 171], 2, 3)]);
        set_quest_mark(&mut quests, 7, [151, 171], 9999, 3);
        assert_eq!(quests, [(8, [10, 10], 1, 0)]);
        // Compass marks by id alone: another NPC's mark with the same id replaces it; type 2 removes it.
        let mut compass = Vec::new();
        set_compass_mark(&mut compass, 1, 0, "npc a");
        set_compass_mark(&mut compass, 2, 1, "npc a");
        set_compass_mark(&mut compass, 1, 1, "npc b");
        assert_eq!(compass, [(2, "npc a"), (1, "npc b")]);
        set_compass_mark(&mut compass, 2, 2, "npc b");
        assert_eq!(compass, [(1, "npc b")]);
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
        // Opaque is kept; see-through and hidden come back see-through, and hidden is never written.
        assert_eq!([View::Shown, View::Translucent, View::Hidden].map(View::kept), [View::Shown, View::Translucent, View::Translucent]);
        assert_eq!(View::default(), View::Translucent);
        assert!(shown(View::Shown, false) && shown(View::Translucent, false));
        assert!(!shown(View::Hidden, false) && !shown(View::Shown, true));
        let [x, y, ..] = window_box();
        assert_eq!([x, y], [screen()[0] - 145.0, 17.0]);
        assert!(covers([x + 1.0, y + 1.0]) && covers([x + 127.0, y + 139.0]) && !covers([x + 129.0, y + 50.0]) && !covers([x + 5.0, y + 141.0]));
        let path = std::env::temp_dir().join(format!("rustnarok-minimap-view-{}", std::process::id()));
        std::fs::write(&path, View::Hidden.name()).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(View::parse(&text), Some(View::Hidden));
        let _ = std::fs::remove_file(&path);
    }
}
