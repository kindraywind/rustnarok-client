//! The graphics settings, and the option images the Game Settings and Shortcut Settings windows share.

use std::path::{Path, PathBuf};

use rustnarok_client::grf::Grf;
use rustnarok_client::text::DEFAULT as DEFAULT_FONT;

use crate::UI;
use crate::render::{Gpu, Texture};

/// The Game Settings tabs' and pages' edge.
const OUTLINE: [u8; 4] = [0xCE, 0xCE, 0xCE, 0xFF];
/// The sizes the Extras' UI scale can be set to, the smallest interface first: the rows of the 4:3 screen the
/// interface is as large as on, 1920×1440 down to the classic 1024×768 (`crate::canvas`).
pub const UI_SIZES: [u16; 6] = [1440, 1200, 1050, 960, 864, 768];

/// The UI size `by` steps after `rows` (a bigger interface is a step up), stopping at the ends.
pub fn ui_size_stepped(rows: u16, by: isize) -> u16 {
    let last = UI_SIZES.len() - 1;
    let index = UI_SIZES.iter().position(|&size| size == rows).unwrap_or(last);
    UI_SIZES[(index as isize + by).clamp(0, last as isize) as usize]
}

#[derive(Clone, Debug, PartialEq)]
pub struct GraphicsSettings {
    pub effects_on: bool,
    /// The max-level aura in full (`/aura`), else simplified; and none at all (`/aura2`).
    pub aura: bool,
    pub aura_off: bool,
    pub quake: bool,
    /// /mineffect: lighter effects.
    pub min_effect: bool,
    /// Full screen (the Graphics page's screen mode).
    pub fullscreen: bool,
    /// /fog: the map's depth fog.
    pub fog: bool,
    /// /mable: Spirit Marble's spirits drawn.
    pub mable: bool,
    /// The game's extended damage numbers (on at first): a million and up in thousands with K, a
    /// hundred million and up in millions with M; off, numbers stop at 999,999.
    pub extended_damage: bool,
    /// The game's Quick Slots (Control tab, on at first): the quick slot window shows.
    pub quick_slot: bool,
    /// This client's extras, none of them in the game and all off at first (the Extras tab): a monster's HP on its
    /// bar, its level after its name, the camera going out much further, an item's random options in its inventory
    /// name box, the seconds left on a cooling skill's hotkey, the exp bar's hover to a hundredth of a percent, a
    /// kill's base and job exp over where the monster died, and the interface growing with the window (`crate::canvas`).
    pub monster_hp: bool,
    pub monster_level: bool,
    pub far_zoom: bool,
    pub item_options: bool,
    pub cooldown_numbers: bool,
    pub exp_hundredths: bool,
    pub kill_exp: bool,
    pub ui_scale: bool,
    /// Which of the eight sets of navigation arrows lies on the ground (the Navigation window's picker), the first at
    /// first.
    pub navi_icons: u8,
    /// How large the UI scale makes the interface: one of `UI_SIZES`, the classic 768 rows (the largest) at first.
    pub ui_size: u16,
    /// The font the text is drawn in (Extras; `text::choices`), Tahoma at first.
    pub font: String,
}

impl Default for GraphicsSettings {
    fn default() -> Self {
        Self {
            effects_on: true,
            aura: true,
            aura_off: false,
            quake: true,
            min_effect: false,
            fullscreen: false,
            fog: true,
            mable: true,
            extended_damage: true,
            quick_slot: true,
            monster_hp: false,
            monster_level: false,
            far_zoom: false,
            item_options: false,
            cooldown_numbers: false,
            exp_hundredths: false,
            kill_exp: false,
            ui_scale: false,
            navi_icons: 1,
            ui_size: 768,
            font: DEFAULT_FONT.into(),
        }
    }
}

impl GraphicsSettings {
    /// The rows the canvas is fitted to, with the Extras' UI scale on (`crate::canvas`).
    pub fn ui_rows(&self) -> Option<f32> {
        self.ui_scale.then_some(f32::from(self.ui_size))
    }

    fn path() -> Option<PathBuf> {
        crate::settings_path("graphics")
    }

    pub fn load() -> Self {
        Self::path().map_or_else(Self::default, |path| Self::load_from(&path))
    }

    /// Saved for next time; not by clients driven over the control socket (scenarios) unless given their own folder.
    pub fn save(&self) {
        let own_folder = std::env::var_os("RUSTNAROK_CLIENT_SETTINGS").is_some_and(|folder| !folder.is_empty());
        if let Some(path) = Self::path().filter(|_| own_folder || !crate::control::driven()) {
            self.save_to(&path);
        }
    }

    pub fn load_from(path: &Path) -> Self {
        let text = std::fs::read_to_string(path).unwrap_or_default();
        // The switches, the UI size and the navigation's arrow set, then the font's name on a line of its own. (A file
        // from when a switch followed the size reads as the first set.)
        let (switches, font) = text.split_once('\n').unwrap_or((&text, ""));
        let mut words = switches.split_whitespace();
        let mut next = |default: bool| words.next().map_or(default, |value| value != "0");
        Self {
            effects_on: next(true),
            aura: next(true),
            aura_off: next(false),
            quake: next(true),
            min_effect: next(false),
            fullscreen: next(false),
            fog: next(true),
            mable: next(true),
            monster_hp: next(false),
            monster_level: next(false),
            far_zoom: next(false),
            item_options: next(false),
            cooldown_numbers: next(false),
            extended_damage: next(true),
            quick_slot: next(true),
            exp_hundredths: next(false),
            kill_exp: next(false),
            ui_scale: next(false),
            ui_size: words.next().and_then(|rows| rows.parse().ok()).filter(|rows| UI_SIZES.contains(rows)).unwrap_or(768),
            navi_icons: words.next().and_then(|set| set.parse().ok()).filter(|set| (1..=8).contains(set)).unwrap_or(1),
            font: Some(font.trim()).filter(|font| !font.is_empty()).unwrap_or(DEFAULT_FONT).to_owned(),
        }
    }

    pub fn save_to(&self, path: &Path) {
        let text = [
            self.effects_on,
            self.aura,
            self.aura_off,
            self.quake,
            self.min_effect,
            self.fullscreen,
            self.fog,
            self.mable,
            self.monster_hp,
            self.monster_level,
            self.far_zoom,
            self.item_options,
            self.cooldown_numbers,
            self.extended_damage,
            self.quick_slot,
            self.exp_hundredths,
            self.kill_exp,
            self.ui_scale,
        ]
        .map(|on| if on { "1" } else { "0" })
        .join(" ")
            + " "
            + &self.ui_size.to_string()
            + " "
            + &self.navi_icons.to_string()
            + "\n"
            + &self.font;
        let _ = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(path, text));
    }
}

pub struct Assets {
    pub check: [Texture; 2],
    pub arrows: [Texture; 2],
    pub track: Texture,
    /// The thumb's left end, middle and right end.
    pub thumb: [Texture; 3],
    /// The Game Settings tabs' and pages' #CECECE edge.
    pub outline: Texture,
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |name: &str| gpu.load_texture(grf, &format!("{UI}{name}.bmp"), false);
        Self {
            check: ["checkbox_0", "checkbox_1"].map(image),
            arrows: ["scroll1left", "scroll1right"].map(image),
            track: image("scroll1mid"),
            thumb: ["scroll1bar_left", "scroll1bar_mid", "scroll1bar_right"].map(image),
            outline: gpu.solid(OUTLINE),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_settings(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("rustnarok-options-{}-{name}.txt", std::process::id()))
    }

    #[test]
    fn graphics_settings_round_trip() {
        let path = temp_settings("graphics");
        let _ = std::fs::remove_file(&path);
        let settings = GraphicsSettings {
            effects_on: false,
            aura: false,
            aura_off: true,
            quake: false,
            min_effect: true,
            fullscreen: true,
            fog: false,
            mable: false,
            monster_hp: true,
            monster_level: true,
            far_zoom: true,
            item_options: true,
            cooldown_numbers: true,
            extended_damage: false,
            quick_slot: false,
            exp_hundredths: true,
            kill_exp: true,
            ui_scale: true,
            navi_icons: 5,
            ui_size: 960,
            font: "MS Gothic (Japan)".into(),
        };
        settings.save_to(&path);
        assert_eq!(GraphicsSettings::load_from(&path), settings);
        // An older file with only the effects switch keeps the aura's and quake's defaults.
        std::fs::write(&path, "0").unwrap();
        assert_eq!(GraphicsSettings::load_from(&path), GraphicsSettings { effects_on: false, ..GraphicsSettings::default() });
        // A file from before the font choice draws in Tahoma.
        std::fs::write(&path, "1 1 0 1 0 0 1 1 1 0 0 0 0 1 1").unwrap();
        assert_eq!(GraphicsSettings::load_from(&path), GraphicsSettings { monster_hp: true, ..GraphicsSettings::default() });
        // One from before the UI size (the switch alone) keeps the classic 768 rows; a size not on the list does too.
        std::fs::write(&path, "1 1 0 1 0 0 1 1 0 0 0 0 0 1 1 0 0 1\nTahoma").unwrap();
        assert_eq!(GraphicsSettings::load_from(&path), GraphicsSettings { ui_scale: true, ..GraphicsSettings::default() });
        std::fs::write(&path, "1 1 0 1 0 0 1 1 0 0 0 0 0 1 1 0 0 1 900\nTahoma").unwrap();
        assert_eq!(GraphicsSettings::load_from(&path).ui_size, 768);
        // One from when a switch followed the size: the first arrow set.
        std::fs::write(&path, "1 1 0 1 0 0 1 1 0 0 0 0 0 1 1 0 0 1 960 1\nTahoma").unwrap();
        assert_eq!(GraphicsSettings::load_from(&path), GraphicsSettings { ui_scale: true, ui_size: 960, ..GraphicsSettings::default() });
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn ui_sizes_step_up_to_the_classic_768_rows_and_stop_at_the_ends() {
        assert_eq!(ui_size_stepped(768, -1), 864);
        assert_eq!(ui_size_stepped(864, 1), 768);
        assert_eq!(ui_size_stepped(768, 1), 768, "the largest interface");
        assert_eq!(ui_size_stepped(1440, -1), 1440, "the smallest");
        assert_eq!(ui_size_stepped(1050, -1), 1200);
        // Off, no rows; on, the size's.
        let on = GraphicsSettings { ui_scale: true, ui_size: 960, ..GraphicsSettings::default() };
        assert_eq!((GraphicsSettings::default().ui_rows(), on.ui_rows()), (None, Some(960.0)));
    }
}
