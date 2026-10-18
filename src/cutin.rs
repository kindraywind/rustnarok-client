//! An NPC's picture (ZC_SHOW_IMAGE): a window of its own, drawn 1:1 and placed by its type; types 3 and 4 have a
//! close button, type 3 a title bar above the picture too. Any other part drags it.

use rustnarok_client::grf::Grf;

use crate::render::{Gpu, Texture};
use crate::window::{Chrome, Frame};
use crate::{Layer, UI, contains};

/// The type that closes the picture.
pub const CLOSE: u8 = 255;
/// Types 0 and 2 centre the picture this far from the left or right edge.
const EDGE: f32 = 200.0;
/// Type 3's title bar.
const TITLE: f32 = 17.0;
/// The close button: this far left of the window's right edge, 3 down.
const CLOSE_LEFT: f32 = 16.0;
const CLOSE_TOP: f32 = 3.0;

/// The picture's file: lower-cased, ".bmp" added unless the name has ".bmp" or ".png".
pub fn file_name(image: &str) -> String {
    let name = image.to_lowercase();
    if name.contains(".bmp") || name.contains(".png") { name } else { format!("{name}.bmp") }
}

/// Whether `image` of `kind` is the picture already showing (`showing`: its file and type): then nothing changes.
pub fn same(showing: Option<(&str, u8)>, image: &str, kind: u8) -> bool {
    showing == Some((file_name(image).as_str(), kind))
}

/// Where a `kind` picture `size` big goes on a `screen`: its top-left (type 3's title bar is above it).
pub fn place(kind: u8, [width, height]: [f32; 2], [screen_width, screen_height]: [f32; 2]) -> [f32; 2] {
    let x = match kind {
        0 => EDGE - width / 2.0,
        2 => screen_width - EDGE - width / 2.0,
        _ => screen_width / 2.0 - width / 2.0,
    };
    let y = if kind >= 3 { screen_height / 2.0 - height / 2.0 } else { screen_height - height };
    [x.floor(), y.floor()]
}

/// The close button's two looks.
pub struct Assets {
    close: [Texture; 2],
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |name: &str| gpu.load_texture(grf, &format!("{UI}basic_interface\\{name}.bmp"), false);
        Self { close: [image("sys_close_off"), image("sys_close_on")] }
    }
}

/// The picture showing: its file, type and texture, and where its window is.
pub struct Cutin {
    pub name: String,
    pub kind: u8,
    picture: Texture,
    pub frame: Frame,
}

impl Cutin {
    /// The picture `name` of `kind` (a texture already loaded), placed for this screen.
    pub fn new(name: String, kind: u8, picture: Texture, screen: [f32; 2]) -> Self {
        let [x, y] = place(kind, picture.size, screen);
        let mut frame = Frame::new([x, y - Self::title(kind)], picture.size[0]);
        frame.open = true;
        Self { name, kind, picture, frame }
    }

    fn title(kind: u8) -> f32 {
        if kind == 3 { TITLE } else { 0.0 }
    }

    /// The window's height: the picture's and type 3's title bar.
    pub fn height(&self) -> f32 {
        self.picture.size[1] + Self::title(self.kind)
    }

    pub fn contains(&self, cursor: [f32; 2]) -> bool {
        contains(self.frame.position, [self.picture.size[0], self.height()], cursor)
    }

    /// The close button's corner on screen, for types 3 and 4.
    fn close_at(&self) -> Option<[f32; 2]> {
        matches!(self.kind, 3 | 4).then(|| self.frame.at([self.picture.size[0] - CLOSE_LEFT, CLOSE_TOP]))
    }

    /// The close button's box on screen, for types 3 and 4.
    pub fn close_box(&self, assets: &Assets) -> Option<[f32; 4]> {
        let [width, height] = assets.close[0].size;
        self.close_at().map(|[x, y]| [x, y, width, height])
    }

    pub fn on_close(&self, cursor: [f32; 2], assets: &Assets) -> bool {
        self.close_box(assets).is_some_and(|[x, y, width, height]| contains([x, y], [width, height], cursor))
    }

    pub fn draw<'a>(&'a self, layer: &mut Layer<'a>, chrome: &'a Chrome, assets: &'a Assets, cursor: [f32; 2]) {
        if self.kind == 3 {
            self.frame.draw_bar(layer, chrome);
        }
        layer.sprites.push((&self.picture, self.frame.at([0.0, Self::title(self.kind)]), self.picture.size));
        if let Some(at) = self.close_at() {
            let image = &assets.close[usize::from(contains(at, assets.close[0].size, cursor))];
            layer.sprites.push((image, at, image.size));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn places_each_type() {
        let picture = [300.0, 400.0];
        for (screen, expected) in [
            ([1024.0, 768.0], [[50.0, 368.0], [362.0, 368.0], [674.0, 368.0], [362.0, 184.0], [362.0, 184.0]]),
            ([800.0, 600.0], [[50.0, 200.0], [250.0, 200.0], [450.0, 200.0], [250.0, 100.0], [250.0, 100.0]]),
        ] {
            assert_eq!([0, 1, 2, 3, 4].map(|kind| place(kind, picture, screen)), expected);
        }
        // Wider than 400 px: off the left edge.
        assert_eq!(place(0, [500.0, 400.0], [1024.0, 768.0])[0], -50.0);
    }

    #[test]
    fn names_the_file() {
        assert_eq!(file_name("Kafra_01"), "kafra_01.bmp");
        assert_eq!(file_name("Kafra_01.BMP"), "kafra_01.bmp");
        assert_eq!(file_name("event.png"), "event.png");
        assert_eq!(file_name("v1.2_face"), "v1.2_face.bmp");
    }

    #[test]
    fn skips_the_picture_showing() {
        assert!(same(Some(("kafra_01.bmp", 2)), "Kafra_01", 2));
        assert!(!same(Some(("kafra_01.bmp", 2)), "Kafra_01", 0));
        assert!(!same(Some(("kafra_01.bmp", 2)), "Kafra_02", 2));
        assert!(!same(None, "Kafra_01", 2));
    }
}
