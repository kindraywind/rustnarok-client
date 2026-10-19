//! The graphics and shortcut settings, the shortcut window, and the option images the Game Settings window shares.

use std::path::{Path, PathBuf};

use rustnarok_client::grf::Grf;
use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};
use crate::window::{Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI};

/// The Game Settings tabs' and pages' edge.
const OUTLINE: [u8; 4] = [0xCE, 0xCE, 0xCE, 0xFF];
const CHECK_X: f32 = 12.0;

// ponytail: calibrate this window against the real client.
const SHORTCUTS_WIDTH: f32 = 280.0;
const SHORTCUTS_ROW: f32 = 22.0;
const SHORTCUTS_TOP: f32 = TITLE_HEIGHT + 8.0;
const SHORTCUTS_HEIGHT: f32 = SHORTCUTS_TOP + 9.0 * SHORTCUTS_ROW + 4.0;
const SHORTCUTS_POSITION: [f32; 2] = [372.0, 260.0];
// ponytail: check this message id against msgstringtable.
const SHORTCUTS_TITLE: usize = 1487;

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
}

impl Default for GraphicsSettings {
    fn default() -> Self {
        Self { effects_on: true, aura: true, aura_off: false, quake: true, min_effect: false, fullscreen: false }
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
        Self { effects_on: next(true), aura: next(true), aura_off: next(false), quake: next(true), min_effect: next(false), fullscreen: next(false) }
    }

    pub fn save_to(&self, path: &Path) {
        let text = [self.effects_on, self.aura, self.aura_off, self.quake, self.min_effect, self.fullscreen].map(|on| if on { "1" } else { "0" }).join(" ");
        let _ = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(path, text));
    }
}

/// Physical key bound to a hotkey-bar slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShortcutKey {
    NamedF(u8),
    Char(char),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ShortcutSettings {
    pub slots: [Option<ShortcutKey>; 36],
}

impl ShortcutSettings {
    pub fn default_bindings() -> Self {
        let mut slots = [None; 36];
        for number in 1..=9 {
            slots[usize::from(number - 1)] = Some(ShortcutKey::NamedF(number));
        }
        Self { slots }
    }

    fn path() -> Option<PathBuf> {
        crate::settings_path("shortcuts")
    }

    pub fn load() -> Self {
        Self::path().map_or_else(Self::default_bindings, |path| Self::load_from(&path))
    }

    pub fn save(&self) {
        if let Some(path) = Self::path() {
            self.save_to(&path);
        }
    }

    /// Which hotbar slot `key` presses. The window's tests use this; the shortcut layer fires the keys itself.
    #[allow(dead_code)]
    pub fn slot_for(&self, key: ShortcutKey) -> Option<usize> {
        self.slots.iter().position(|binding| *binding == Some(key))
    }

    pub fn bind(&mut self, slot: usize, key: Option<ShortcutKey>) {
        if slot >= self.slots.len() {
            return;
        }
        if let Some(key) = key {
            for binding in &mut self.slots {
                if *binding == Some(key) {
                    *binding = None;
                }
            }
        }
        self.slots[slot] = key;
    }

    pub fn load_from(path: &Path) -> Self {
        let Ok(text) = std::fs::read_to_string(path) else { return Self::default_bindings() };
        let bindings: Vec<_> = text.split_whitespace().map(parse_shortcut).collect();
        if bindings.len() != 36 {
            return Self::default_bindings();
        }
        let mut slots = [None; 36];
        slots.copy_from_slice(&bindings);
        Self { slots }
    }

    pub fn save_to(&self, path: &Path) {
        let text = self.slots.iter().map(|key| key.map_or_else(|| "-".into(), shortcut_token)).collect::<Vec<_>>().join(" ");
        let _ = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(path, text));
    }
}

fn parse_shortcut(token: &str) -> Option<ShortcutKey> {
    if token == "-" {
        None
    } else if let Some(number) = token.strip_prefix('f').and_then(|number| number.parse().ok()) {
        Some(ShortcutKey::NamedF(number))
    } else {
        let mut chars = token.chars();
        chars.next().filter(|_| chars.next().is_none()).map(ShortcutKey::Char)
    }
}

fn shortcut_token(key: ShortcutKey) -> String {
    match key {
        ShortcutKey::NamedF(number) => format!("f{number}"),
        ShortcutKey::Char(character) => character.to_string(),
    }
}

fn shortcut_name(key: Option<ShortcutKey>) -> String {
    key.map_or_else(
        || "-".into(),
        |key| match key {
            ShortcutKey::NamedF(number) => format!("F{number}"),
            ShortcutKey::Char(character) => character.to_uppercase().collect(),
        },
    )
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ShortcutsHit {
    Title(TitleHit),
    Row(usize),
    Window,
}

pub struct ShortcutsWindow {
    pub frame: Frame,
    pub listening: Option<usize>,
}

impl ShortcutsWindow {
    pub fn new() -> Self {
        let mut frame = Frame::new(SHORTCUTS_POSITION, SHORTCUTS_WIDTH);
        frame.open = true;
        Self { frame, listening: None }
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<ShortcutsHit> {
        let local = self.frame.local(cursor, SHORTCUTS_HEIGHT)?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(ShortcutsHit::Title(hit));
        }
        let row = ((local[1] - SHORTCUTS_TOP) / SHORTCUTS_ROW).floor();
        Some(if (0.0..9.0).contains(&row) { ShortcutsHit::Row(row as usize) } else { ShortcutsHit::Window })
    }

    pub fn press(&mut self, hit: ShortcutsHit, cursor: [f32; 2]) {
        if hit == ShortcutsHit::Title(TitleHit::Title) {
            self.frame.start_drag(cursor);
        }
    }

    pub fn click(&mut self, hit: ShortcutsHit) -> bool {
        match hit {
            ShortcutsHit::Title(TitleHit::Close) => self.frame.open = false,
            ShortcutsHit::Row(slot) => self.listening = Some(slot),
            ShortcutsHit::Title(_) | ShortcutsHit::Window => return false,
        }
        true
    }

    pub fn bind_key(&mut self, key: ShortcutKey, settings: &mut ShortcutSettings) -> bool {
        let Some(slot) = self.listening.take() else { return false };
        settings.bind(slot, Some(key));
        true
    }

    pub fn drag_to(&mut self, cursor: [f32; 2]) -> bool {
        self.frame.drag_to(cursor, SHORTCUTS_HEIGHT)
    }

    pub fn drop_drag(&mut self) -> bool {
        self.frame.drop_drag()
    }

    pub fn draw<'a>(&self, layer: &mut Layer<'a>, chrome: &'a Chrome, font: &Font, messages: &[String], settings: &ShortcutSettings, pointer: [f32; 2]) {
        let at = |local: [f32; 2]| self.frame.at(local);
        self.frame.draw_title(layer, chrome, messages.get(SHORTCUTS_TITLE).map_or("", String::as_str), pointer);
        if self.frame.minimized {
            return;
        }
        layer.sprites.push((&chrome.border, at([0.0, TITLE_HEIGHT]), [SHORTCUTS_WIDTH, SHORTCUTS_HEIGHT - TITLE_HEIGHT]));
        layer.sprites.push((&chrome.white, at([1.0, TITLE_HEIGHT]), [SHORTCUTS_WIDTH - 2.0, SHORTCUTS_HEIGHT - TITLE_HEIGHT - 1.0]));
        for slot in 0..9 {
            let top = SHORTCUTS_TOP + slot as f32 * SHORTCUTS_ROW;
            let marker = if self.listening == Some(slot) { "..." } else { "" };
            layer.texts.push((format!("Hotkey 1-{} {marker}", slot + 1), at([CHECK_X, top + 3.0]), [0, 0, 0]));
            let key = shortcut_name(settings.slots[slot]);
            layer.texts.push((key.clone(), at([SHORTCUTS_WIDTH - CHECK_X - font.width(&key), top + 3.0]), [0, 0, 0]));
        }
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
        let settings = GraphicsSettings { effects_on: false, aura: false, aura_off: true, quake: false, min_effect: true, fullscreen: true };
        settings.save_to(&path);
        assert_eq!(GraphicsSettings::load_from(&path), settings);
        // An older file with only the effects switch keeps the aura's and quake's defaults.
        std::fs::write(&path, "0").unwrap();
        assert_eq!(GraphicsSettings::load_from(&path), GraphicsSettings { effects_on: false, ..GraphicsSettings::default() });
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn shortcut_defaults_map_f_keys_to_first_row() {
        let settings = ShortcutSettings::default_bindings();
        for number in 1..=9 {
            assert_eq!(settings.slot_for(ShortcutKey::NamedF(number)), Some(usize::from(number - 1)));
        }
        assert!(settings.slots[9..].iter().all(Option::is_none));
    }

    #[test]
    fn shortcut_row_listens_and_binding_moves_key() {
        let mut window = ShortcutsWindow::new();
        let mut settings = ShortcutSettings::default_bindings();
        let at = |local: [f32; 2]| window.frame.at(local);
        assert!(window.frame.open);
        assert_eq!(window.hit(at([20.0, SHORTCUTS_TOP + SHORTCUTS_ROW / 2.0])), Some(ShortcutsHit::Row(0)));
        assert!(window.click(ShortcutsHit::Row(0)));
        assert_eq!(window.listening, Some(0));
        assert!(window.bind_key(ShortcutKey::NamedF(2), &mut settings));
        assert_eq!(window.listening, None);
        assert_eq!(settings.slots[0], Some(ShortcutKey::NamedF(2)));
        assert_eq!(settings.slots[1], None);
        assert_eq!(settings.slot_for(ShortcutKey::NamedF(2)), Some(0));
    }

    #[test]
    fn shortcut_settings_round_trip() {
        let path = temp_settings("shortcuts");
        let _ = std::fs::remove_file(&path);
        let mut settings = ShortcutSettings::default_bindings();
        settings.slots[0] = Some(ShortcutKey::Char('q'));
        settings.slots[35] = Some(ShortcutKey::NamedF(12));
        settings.save_to(&path);
        assert_eq!(ShortcutSettings::load_from(&path), settings);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn shortcuts_close_closes_window() {
        let mut window = ShortcutsWindow::new();
        assert!(window.click(ShortcutsHit::Title(TitleHit::Close)));
        assert!(!window.frame.open);
    }
}
