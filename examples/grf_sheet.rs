//! Dev tool: `cargo run --example grf_sheet -- <data.grf> <out.png> <text>...` lays out every image whose path contains
//! one of the `text`s on one sheet (2x, or GRF_SCALE, in order) and prints each one's number, size, path and the
//! box its visible pixels fill (left, top, right, bottom).

use image::{Rgba, RgbaImage, imageops};
use roclient::grf;

const GAP: u32 = 6;
const SHEET_WIDTH: u32 = 1400;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let [_, grf_path, output, texts @ ..] = &args[..] else {
        panic!("usage: grf_sheet <data.grf> <out.png> <text>...");
    };
    let scale: u32 = std::env::var("GRF_SCALE").ok().and_then(|scale| scale.parse().ok()).unwrap_or(2);
    let grf = grf::Grf::open(grf_path).unwrap();
    let mut paths: Vec<&str> = grf
        .paths()
        .filter(|path| path.ends_with(".bmp") || path.ends_with(".tga") || path.ends_with(".png") || path.ends_with(".jpg"))
        .filter(|path| texts.iter().any(|text| path.contains(&text.replace('/', "\\").to_lowercase())))
        .collect();
    paths.sort_unstable();

    // Shelf packing: left to right, wrapping at the sheet width.
    let mut placed = Vec::new();
    let (mut x, mut y, mut row_height) = (GAP, GAP, 0);
    for (number, path) in paths.iter().enumerate() {
        let bytes = grf.read(path).unwrap();
        let decoded =
            if path.ends_with(".tga") { image::load_from_memory_with_format(&bytes, image::ImageFormat::Tga) } else { image::load_from_memory(&bytes) };
        let Ok(image) = decoded else { continue };
        let mut image = image.to_rgba8();
        // The client treats magenta as transparent.
        for pixel in image.pixels_mut() {
            if pixel.0[0] > 0xf0 && pixel.0[1] < 0x10 && pixel.0[2] > 0xf0 {
                *pixel = Rgba([0, 0, 0, 0]);
            }
        }
        let visible: Vec<(u32, u32)> = image.enumerate_pixels().filter(|(.., pixel)| pixel.0[3] > 0).map(|(x, y, _)| (x, y)).collect();
        let bounds = |pick: fn(&(u32, u32)) -> u32| (visible.iter().map(pick).min().unwrap_or(0), visible.iter().map(pick).max().unwrap_or(0));
        let ((left, right), (top, bottom)) = (bounds(|point| point.0), bounds(|point| point.1));
        println!("{number:3} {:4}x{:<4} {path}  visible {left},{top}..{right},{bottom}", image.width(), image.height());
        // GRF_PIXELS=x,y,w,h prints that region's colors (RRGGBBAA).
        if let Ok(region) = std::env::var("GRF_PIXELS") {
            let [x, y, w, h] = <[u32; 4]>::try_from(region.split(',').map(|n| n.parse().unwrap()).collect::<Vec<u32>>()).unwrap();
            for row in y..(y + h).min(image.height()) {
                let line: Vec<String> =
                    (x..(x + w).min(image.width())).map(|column| image.get_pixel(column, row).0.iter().map(|c| format!("{c:02x}")).collect()).collect();
                println!("    {row:3}: {}", line.join(" "));
            }
        }
        let image = imageops::resize(&image, image.width() * scale, image.height() * scale, imageops::FilterType::Nearest);
        if x + image.width() > SHEET_WIDTH && x > GAP {
            (x, y, row_height) = (GAP, y + row_height + GAP, 0);
        }
        placed.push((x, y, image));
        let last = &placed.last().unwrap().2;
        x += last.width() + GAP;
        row_height = row_height.max(last.height());
    }
    let mut sheet = RgbaImage::from_pixel(SHEET_WIDTH, y + row_height + GAP, Rgba([96, 112, 128, 255]));
    for (x, y, image) in &placed {
        imageops::overlay(&mut sheet, image, i64::from(*x), i64::from(*y));
    }
    sheet.save(output).unwrap();
}
