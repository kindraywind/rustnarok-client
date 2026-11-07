//! UI text, rasterized from a TrueType font into RGBA images.

use image::{Rgba, RgbaImage};

/// The official client draws its UI text at 12px.
pub const SIZE: f32 = 12.0;
// ponytail: fontdue doesn't hint, so small text only reads well antialiased.
/// Set to false (thresholded coverage) once a hinted bitmap-friendly font like Gulim is in use.
const ANTIALIAS: bool = true;

/// The font (`RUSTNAROK_CLIENT_FONT`, else the one picked in Game Settings, Tahoma by default), then the fallbacks. A
/// fallback is read the first time a character needs it: fontdue builds every glyph's outline up front, and
/// AppleGothic's Hangul alone is ~185 MB.
pub struct Font {
    first: fontdue::Font,
    fallbacks: Vec<Fallback>,
    /// The choice it was loaded for (`choices`).
    name: String,
}

/// The font a player can pick by name: Tahoma, the default (no path: `system_fonts`), or a font file.
#[derive(Debug, PartialEq)]
pub struct Choice {
    pub name: String,
    path: Option<String>,
}

pub const DEFAULT: &str = "Tahoma";

/// The font each region's version of the game draws its text in (Thailand's Tahoma is the default), and the file it
/// comes in; AppleGothic is macOS's own Korean face.
const FACES: [(&str, &str); 7] = [
    ("Gulim (Korea)", "gulim.ttc"),
    ("Arial (English)", "arial.ttf"),
    ("MS Gothic (Japan)", "msgothic.ttc"),
    ("SimSun (China)", "simsun.ttc"),
    ("MingLiU (Taiwan)", "mingliu.ttc"),
    ("Verdana", "verdana.ttf"),
    ("AppleGothic", "applegothic.ttf"),
];

/// The fonts a player can pick (Game Settings' Extras), found once: Tahoma, then each face of `FACES` whose file is in
/// the game folder's `System/Font` (the data folder, `RUSTNAROK_CLIENT_DATA`) or the system's fonts, then the game
/// folder's other fonts by file name.
pub fn choices() -> &'static [Choice] {
    static CHOICES: std::sync::OnceLock<Vec<Choice>> = std::sync::OnceLock::new();
    CHOICES.get_or_init(|| {
        let root = std::env::var("RUSTNAROK_CLIENT_DATA").ok().filter(|root| root != "none").unwrap_or_else(|| ".".into());
        let game = ["System/Font", "SystemEN/Font"].map(|folder| std::path::Path::new(&root).join(folder));
        choices_from(&files_in(&game), &system_font_files())
    })
}

fn choices_from(game: &[String], system: &[String]) -> Vec<Choice> {
    let file = |path: &String| std::path::Path::new(path).file_name().map(|name| name.to_string_lossy().to_lowercase());
    let mut choices = vec![Choice { name: DEFAULT.into(), path: None }];
    for (name, wanted) in FACES {
        if let Some(path) = game.iter().chain(system).find(|path| file(path).as_deref() == Some(wanted)) {
            choices.push(Choice { name: name.into(), path: Some(path.clone()) });
        }
    }
    for path in game.iter().filter(|path| !FACES.iter().any(|(_, wanted)| file(path).as_deref() == Some(wanted))) {
        let name = std::path::Path::new(path).file_stem().map_or_else(|| path.clone(), |stem| stem.to_string_lossy().into_owned());
        choices.push(Choice { name, path: Some(path.clone()) });
    }
    choices
}

/// Where `name` is among the choices; an unknown name (a font since removed) is Tahoma.
pub fn index_of(choices: &[Choice], name: &str) -> usize {
    choices.iter().position(|choice| choice.name == name).unwrap_or(0)
}

/// The choice `by` steps after `name`, round the ends.
pub fn stepped<'a>(choices: &'a [Choice], name: &str, by: isize) -> &'a str {
    &choices[(index_of(choices, name) as isize + by).rem_euclid(choices.len() as isize) as usize].name
}

/// The font files (TrueType, collections, OpenType) in these folders, sorted by path.
fn files_in(folders: &[std::path::PathBuf]) -> Vec<String> {
    let mut files: Vec<String> = folders
        .iter()
        .filter_map(|folder| std::fs::read_dir(folder).ok())
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| ["ttf", "ttc", "otf"].contains(&extension.to_string_lossy().to_lowercase().as_str())))
        .map(|path| path.to_string_lossy().into_owned())
        .collect();
    files.sort();
    files
}

/// Every font file the system has.
fn system_font_files() -> Vec<String> {
    let home = |under: &str| std::env::var_os("HOME").map(|home| std::path::Path::new(&home).join(under));
    if cfg!(target_os = "macos") {
        let folders =
            ["/System/Library/Fonts", "/System/Library/Fonts/Supplemental", "/Library/Fonts"].map(Into::into).into_iter().chain(home("Library/Fonts"));
        files_in(&folders.collect::<Vec<_>>())
    } else if cfg!(windows) {
        let windows = std::env::var_os("WINDIR").map_or_else(|| r"C:\Windows".into(), std::path::PathBuf::from).join("Fonts");
        let own = std::env::var_os("LOCALAPPDATA").map(|local| std::path::Path::new(&local).join(r"Microsoft\Windows\Fonts"));
        files_in(&std::iter::once(windows).chain(own).collect::<Vec<_>>())
    } else {
        // Fontconfig lists every font file.
        std::process::Command::new("fc-list")
            .args(["-f", "%{file}\n"])
            .output()
            .ok()
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .map_or_else(Vec::new, |text| text.lines().map(Into::into).collect())
    }
}

struct Fallback {
    path: String,
    font: std::sync::OnceLock<Option<fontdue::Font>>,
}

fn read(path: &str) -> Option<fontdue::Font> {
    fontdue::Font::from_bytes(std::fs::read(path).ok()?, fontdue::FontSettings::default()).ok()
}

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
    /// Tahoma (or `RUSTNAROK_CLIENT_FONT`).
    pub fn load() -> Self {
        Self::named(DEFAULT)
    }

    /// The choice of that name (`choices`), with the system's fonts behind it for what it lacks; Tahoma when there's
    /// no such choice.
    pub fn named(name: &str) -> Self {
        let chosen = choices().iter().find(|choice| choice.name == name).and_then(|choice| choice.path.clone());
        let paths: Vec<String> = std::env::var("RUSTNAROK_CLIENT_FONT").into_iter().chain(chosen).chain(system_fonts()).collect();
        Self { name: name.to_owned(), ..Self::from_paths(&paths) }
    }

    /// The choice it was loaded for.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The first path that reads is the font; the paths after it are fallbacks, read when first needed.
    fn from_paths(paths: &[String]) -> Self {
        let (index, first) = paths.iter().enumerate().find_map(|(index, path)| Some((index, read(path)?))).unwrap_or_else(|| panic!("no font at {paths:?}"));
        let fallbacks = paths[index + 1..].iter().map(|path| Fallback { path: path.clone(), font: std::sync::OnceLock::new() }).collect();
        Self { first, fallbacks, name: String::new() }
    }

    /// The first font that has the character, else the font's own missing glyph.
    fn face(&self, character: char) -> &fontdue::Font {
        if self.first.lookup_glyph_index(character) != 0 {
            return &self.first;
        }
        self.fallbacks
            .iter()
            .filter_map(|fallback| fallback.font.get_or_init(|| read(&fallback.path)).as_ref())
            .find(|font| font.lookup_glyph_index(character) != 0)
            .unwrap_or(&self.first)
    }

    pub fn line_height(&self) -> f32 {
        self.first.horizontal_line_metrics(SIZE).map_or(SIZE, |line| line.new_line_size.ceil())
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
        let line = self.first.horizontal_line_metrics(size);
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
    /// The first font that reads is used; a missing fallback is only touched for a character the font lacks, and then
    /// leaves the font's own glyph, as before.
    #[test]
    fn reads_a_fallback_only_when_a_character_needs_it() {
        let Some(first) = super::system_fonts().into_iter().find(|path| super::read(path).is_some()) else { return };
        let paths = ["/no/such/font.ttf".to_string(), first, "/no/such/fallback.ttf".to_string()];
        let font = super::Font::from_paths(&paths);
        assert_eq!(font.fallbacks.len(), 1);
        font.width("Prontera 123");
        assert!(font.fallbacks[0].font.get().is_none(), "Latin text read the fallback");
        assert!(font.width("\u{10FFFD}") >= 0.0);
        assert!(matches!(font.fallbacks[0].font.get(), Some(None)), "the missing fallback was not tried");
    }

    /// Tahoma first, then the faces whose files are found (the game folder's copy before the system's), then the game
    /// folder's other fonts; stepping goes round.
    #[test]
    fn offers_the_faces_found_then_the_game_folders_own() {
        let game = ["System/Font/SCDream4.otf".to_string(), "System/Font/GULIM.TTC".into()];
        let system = ["/fonts/Arial.ttf".to_string(), "/fonts/Tahoma.ttf".into(), "/fonts/gulim.ttc".into(), "/fonts/Comic Sans MS.ttf".into()];
        let choices = super::choices_from(&game, &system);
        let names: Vec<&str> = choices.iter().map(|choice| choice.name.as_str()).collect();
        assert_eq!(names, ["Tahoma", "Gulim (Korea)", "Arial (English)", "SCDream4"]);
        assert_eq!(choices[0].path, None, "Tahoma is the system's");
        assert_eq!(choices[1].path.as_deref(), Some("System/Font/GULIM.TTC"), "the game folder's copy first");
        assert_eq!(super::stepped(&choices, "Tahoma", 1), "Gulim (Korea)");
        assert_eq!(super::stepped(&choices, "Tahoma", -1), "SCDream4");
        assert_eq!(super::stepped(&choices, "SCDream4", 1), "Tahoma");
        assert_eq!(super::stepped(&choices, "Removed font", 1), "Gulim (Korea)", "an unknown name counts as Tahoma");
        assert_eq!(super::stepped(&choices[..1], "Tahoma", 1), "Tahoma");
    }

    /// A picked font draws the text; a name with no font behind it draws in Tahoma.
    #[test]
    fn a_named_font_draws_and_keeps_its_name() {
        let tahoma = super::Font::load();
        let missing = super::Font::named("No such font");
        assert_eq!(missing.name(), "No such font");
        assert_eq!(missing.width("Prontera 123"), tahoma.width("Prontera 123"));
        if super::choices().iter().any(|choice| choice.name == "Arial (English)") {
            assert_ne!(super::Font::named("Arial (English)").width("Prontera 123"), tahoma.width("Prontera 123"));
        }
    }

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
