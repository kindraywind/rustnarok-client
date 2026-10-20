//! UI text, rasterized from a TrueType font into RGBA images.

use image::{Rgba, RgbaImage};

/// The official client draws its UI text at 12px.
pub const SIZE: f32 = 12.0;
// ponytail: fontdue doesn't hint, so small text only reads well antialiased.
/// Set to false (thresholded coverage) once a hinted bitmap-friendly font like Gulim is in use.
const ANTIALIAS: bool = true;

/// The font (`RUSTNAROK_CLIENT_FONT`, Tahoma by default), then the fallbacks.
pub struct Font(Vec<fontdue::Font>);

/// Tahoma, then a font for what it lacks (Tahoma has no Hangul), from the system's fonts.
fn system_fonts() -> Vec<String> {
    if cfg!(target_os = "macos") {
        vec!["/System/Library/Fonts/Supplemental/Tahoma.ttf".into(), "/System/Library/Fonts/Supplemental/AppleGothic.ttf".into()]
    } else if cfg!(windows) {
        vec![r"C:\Windows\Fonts\tahoma.ttf".into(), r"C:\Windows\Fonts\malgun.ttf".into()]
    } else {
        // Fontconfig's pick for Tahoma (whatever stands in for it), then for Korean.
        ["Tahoma", ":lang=ko"]
            .into_iter()
            .filter_map(|pattern| String::from_utf8(std::process::Command::new("fc-match").args(["-f", "%{file}", pattern]).output().ok()?.stdout).ok())
            .collect()
    }
}

impl Font {
    pub fn load() -> Self {
        let paths: Vec<String> = std::env::var("RUSTNAROK_CLIENT_FONT").into_iter().chain(system_fonts()).collect();
        let fonts: Vec<_> =
            paths.iter().filter_map(|path| fontdue::Font::from_bytes(std::fs::read(path).ok()?, fontdue::FontSettings::default()).ok()).collect();
        assert!(!fonts.is_empty(), "no font at {paths:?}");
        Self(fonts)
    }

    /// A font from these files (each a fallback for the ones before), for text set apart from the UI's.
    pub fn from_files(paths: &[&str]) -> Option<Self> {
        let fonts: Vec<_> =
            paths.iter().filter_map(|path| fontdue::Font::from_bytes(std::fs::read(path).ok()?, fontdue::FontSettings::default()).ok()).collect();
        (!fonts.is_empty()).then_some(Self(fonts))
    }

    /// The first font that has the character.
    fn face(&self, character: char) -> &fontdue::Font {
        self.0.iter().find(|font| font.lookup_glyph_index(character) != 0).unwrap_or(&self.0[0])
    }

    pub fn line_height(&self) -> f32 {
        self.0[0].horizontal_line_metrics(SIZE).map_or(SIZE, |line| line.new_line_size.ceil())
    }

    pub fn width(&self, text: &str) -> f32 {
        text.chars().map(|character| self.face(character).metrics(character, SIZE).advance_width).sum()
    }

    /// One line of text, `line_height` tall and at least 1px wide.
    pub fn render(&self, text: &str, color: [u8; 3]) -> RgbaImage {
        self.render_sized(text, color, SIZE)
    }

    /// One line at another pixel size (the client draws a few labels larger).
    pub fn render_sized(&self, text: &str, color: [u8; 3], size: f32) -> RgbaImage {
        let line = self.0[0].horizontal_line_metrics(size);
        let ascent = line.map_or(size, |line| line.ascent).round() as i32;
        let width: f32 = text.chars().map(|character| self.face(character).metrics(character, size).advance_width).sum();
        let height = line.map_or(size, |line| line.new_line_size.ceil());
        let mut image = RgbaImage::new(width.ceil().max(1.0) as u32, height as u32);
        let mut pen = 0.0f32;
        for character in text.chars() {
            let (metrics, coverage) = self.face(character).rasterize(character, size);
            let left = pen.round() as i32 + metrics.xmin;
            let top = ascent - metrics.height as i32 - metrics.ymin;
            for (index, &value) in coverage.iter().enumerate() {
                let alpha = if ANTIALIAS {
                    value
                } else if value >= 128 {
                    255
                } else {
                    0
                };
                let x = left + (index % metrics.width) as i32;
                let y = top + (index / metrics.width) as i32;
                if alpha > 0 && x >= 0 && y >= 0 && (x as u32) < image.width() && (y as u32) < image.height() {
                    image.put_pixel(x as u32, y as u32, Rgba([color[0], color[1], color[2], alpha]));
                }
            }
            pen += metrics.advance_width;
        }
        image
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn renders_hangul_and_latin_inside_the_line() {
        let font = super::Font::load();
        let image = font.render("아이디 Ag", [0, 0, 0]);
        assert_eq!(image.height() as f32, font.line_height());
        assert!(image.width() as f32 >= font.width("아이디 Ag").floor());
        let inked = image.pixels().filter(|pixel| pixel.0[3] > 0).count();
        assert!(inked > 40, "only {inked} pixels drawn");
        assert!(super::ANTIALIAS || image.pixels().all(|pixel| pixel.0[3] == 0 || pixel.0[3] == 255));
    }
}
