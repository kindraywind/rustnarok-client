//! The graphics settings, and the option images the Game Settings and Shortcut Settings windows share.

use std::path::{Path, PathBuf};

use rustnarok_client::grf::Grf;

use crate::UI;
use crate::render::{Gpu, Texture};

/// The Game Settings tabs' and pages' edge.
const OUTLINE: [u8; 4] = [0xCE, 0xCE, 0xCE, 0xFF];

#[derive(Clone, Copy, Debug, PartialEq)]
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
    /// name box, and the seconds left on a cooling skill's hotkey.
    pub monster_hp: bool,
    pub monster_level: bool,
    pub far_zoom: bool,
    pub item_options: bool,
    pub cooldown_numbers: bool,
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
        }
    }
}

impl GraphicsSettings {
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
        let mut words = text.split_whitespace();
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
        ]
        .map(|on| if on { "1" } else { "0" })
        .join(" ");
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
        };
        settings.save_to(&path);
        assert_eq!(GraphicsSettings::load_from(&path), settings);
        // An older file with only the effects switch keeps the aura's and quake's defaults.
        std::fs::write(&path, "0").unwrap();
        assert_eq!(GraphicsSettings::load_from(&path), GraphicsSettings { effects_on: false, ..GraphicsSettings::default() });
        let _ = std::fs::remove_file(path);
    }
}
