//! The minimap at the top right: the map's overview picture, the player's arrow and coordinates.

use image::{Rgba, RgbaImage};
use rustnarok_client::grf::Grf;

use crate::render::{self, Gpu, Texture};
use crate::{Layer, SCREEN, UI};

// ponytail: the whole map at full opacity; zoom levels need sub-image drawing.
const SIZE: f32 = 128.0;
const MARGIN: f32 = 16.0;
const BUTTON: f32 = 12.0;
const BUTTONS: [&str; 5] = ["object", "plus", "minus", "mini", "viewon"];

pub struct Assets {
    /// The player arrow turned to each of the 8 directions.
    arrows: Vec<Texture>,
    buttons: Vec<Texture>,
    black: Texture,
}

/// Turns an image a whole number of eighths around its center (nearest neighbor), keeping its size.
fn rotate(image: &RgbaImage, eighths: u32) -> RgbaImage {
    let angle = eighths as f32 * std::f32::consts::FRAC_PI_4;
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

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let arrow = render::load_image(grf, &format!("{UI}map\\map_arrow.bmp")).unwrap_or_else(|_| RgbaImage::new(1, 1));
        Self {
            arrows: (0..8).map(|eighths| gpu.texture(&rotate(&arrow, eighths), false)).collect(),
            buttons: BUTTONS.iter().map(|name| gpu.load_texture(grf, &format!("{UI}minimap\\i_{name}_1.bmp"), false)).collect(),
            black: gpu.solid([0, 0, 0, 255]),
        }
    }
}

/// The overview picture of a map (data\texture\유저인터페이스\map\<map>.bmp), if the client has one.
pub fn load_map(gpu: &Gpu, grf: &Grf, map: &str) -> Option<Texture> {
    let name = map.trim_end_matches(".gat").trim_end_matches(".rsw");
    gpu.try_load_texture(grf, &format!("{UI}map\\{name}.bmp"), true).ok()
}

/// Draws the minimap: the map scaled into 128x128 (keeping its proportions), the arrow and the coordinates.
/// NPC marks (`marks`: a cell and a texture of its color) show as small crosses, party members as dots.
#[allow(clippy::too_many_arguments)]
pub fn draw<'a>(
    layer: &mut Layer<'a>,
    assets: &'a Assets,
    map: Option<&'a Texture>,
    size: [u32; 2],
    cell: [i32; 2],
    direction: u8,
    marks: &[([i32; 2], &'a Texture)],
    party: &[([i32; 2], &'a Texture)],
) {
    let origin = [SCREEN[0] - MARGIN - SIZE, MARGIN];
    let Some(map) = map else { return };
    layer.sprites.push((map, origin, [SIZE, SIZE]));
    // Cells to minimap pixels: the longer side fills the square; the picture is drawn north up.
    let longest = size[0].max(size[1]) as f32;
    let scale = SIZE / longest;
    let start = [(longest - size[0] as f32) / 2.0 * scale, (size[1] as f32 - longest) / 2.0 * scale];
    let project = |cell: [i32; 2]| [origin[0] + start[0] + cell[0] as f32 * scale, origin[1] + start[1] + SIZE - cell[1] as f32 * scale];
    // The cross: 8x2 and 2x8.
    for &(cell, color) in marks {
        let [x, y] = project(cell).map(f32::round);
        layer.sprites.push((color, [x - 1.0, y - 4.0], [2.0, 8.0]));
        layer.sprites.push((color, [x - 4.0, y - 1.0], [8.0, 2.0]));
    }
    // A member's dot: 4x4 of the member's color in a black 6x6.
    for &(cell, color) in party {
        let [x, y] = project(cell).map(f32::round);
        layer.sprites.push((&assets.black, [x - 3.0, y - 3.0], [6.0, 6.0]));
        layer.sprites.push((color, [x - 2.0, y - 2.0], [4.0, 4.0]));
    }
    let point = project(cell);
    let arrow = &assets.arrows[usize::from((direction + 4) % 8)];
    layer.sprites.push((arrow, [(point[0] - arrow.size[0] / 2.0).round(), (point[1] - arrow.size[1] / 2.0).round()], arrow.size));

    // Below it: the buttons and the coordinates, white with a black shadow.
    let row = origin[1] + SIZE + 1.0;
    layer.sprites.push((&assets.buttons[0], [origin[0], row], [BUTTON, BUTTON]));
    for (place, button) in assets.buttons[1..].iter().enumerate() {
        layer.sprites.push((button, [origin[0] + SIZE - (4 - place) as f32 * BUTTON, row], [BUTTON, BUTTON]));
    }
    let coordinates = format!("{} {}", cell[0], cell[1]);
    layer.texts.push((coordinates.clone(), [origin[0] + BUTTON + 3.0, row - 1.0], [0, 0, 0]));
    layer.texts.push((coordinates, [origin[0] + BUTTON + 2.0, row - 2.0], [255; 3]));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turns_by_eighths() {
        // A dot right of center goes below it after a quarter turn clockwise.
        let mut image = RgbaImage::new(9, 9);
        image.put_pixel(7, 4, Rgba([255; 4]));
        let turned = rotate(&image, 2);
        assert_eq!(turned.get_pixel(4, 7).0, [255; 4]);
        assert_eq!(rotate(&image, 0), image);
    }
}
