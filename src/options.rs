//! The option menu's audio, graphics and shortcut settings.

use std::path::{Path, PathBuf};

use rustnarok_client::grf::Grf;
use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};
use crate::sound::{MAX_VOLUME, Settings};
use crate::window::{Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI};

// ponytail: a 280px window with a row per setting; calibrate against the real client.
const WIDTH: f32 = 280.0;
const ROW: f32 = 22.0;
const TOP: f32 = TITLE_HEIGHT + 8.0;
const HEIGHT: f32 = TOP + 2.0 * ROW + 4.0;
const CHECK_X: f32 = 12.0;
const SLIDER_X: f32 = 80.0;
const ARROW: f32 = 13.0;
/// The track between the arrows, and the thumb on it.
const TRACK: f32 = 174.0;
const THUMB: f32 = 24.0;
const STEP: u8 = 8;
// The menu no longer opens this window (its buttons are the spec's). The window stays.
#[allow(dead_code)]
const POSITION: [f32; 2] = [372.0, 300.0];
/// "Audio Settings", then "    BGM" and "    SFX" (their spaces leave room for the checkbox).
const TITLE: usize = 1485;
const LABELS: [usize; 2] = [1495, 1496];

// ponytail: calibrate these windows against the real client.
const GRAPHICS_WIDTH: f32 = 280.0;
const GRAPHICS_ROW: f32 = 22.0;
const GRAPHICS_TOP: f32 = TITLE_HEIGHT + 8.0;
const GRAPHICS_HEIGHT: f32 = GRAPHICS_TOP + GRAPHICS_ROW + 4.0;
const GRAPHICS_POSITION: [f32; 2] = [372.0, 300.0];
const SHORTCUTS_WIDTH: f32 = 280.0;
const SHORTCUTS_ROW: f32 = 22.0;
const SHORTCUTS_TOP: f32 = TITLE_HEIGHT + 8.0;
const SHORTCUTS_HEIGHT: f32 = SHORTCUTS_TOP + 9.0 * SHORTCUTS_ROW + 4.0;
const SHORTCUTS_POSITION: [f32; 2] = [372.0, 260.0];
// ponytail: check these neighboring message ids against msgstringtable.
const GRAPHICS_TITLE: usize = 1486;
const SHORTCUTS_TITLE: usize = 1487;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GraphicsSettings {
    pub effects_on: bool,
}

impl Default for GraphicsSettings {
    fn default() -> Self {
        Self { effects_on: true }
    }
}

impl GraphicsSettings {
    fn path() -> Option<PathBuf> {
        crate::settings_path("graphics")
    }

    pub fn load() -> Self {
        Self::path().map_or_else(Self::default, |path| Self::load_from(&path))
    }

    pub fn save(&self) {
        if let Some(path) = Self::path() {
            self.save_to(&path);
        }
    }

    pub fn load_from(path: &Path) -> Self {
        let text = std::fs::read_to_string(path).unwrap_or_default();
        Self { effects_on: text.split_whitespace().next().is_none_or(|value| value != "0") }
    }

    pub fn save_to(&self, path: &Path) {
        let _ = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(path, if self.effects_on { "1" } else { "0" }));
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GraphicsHit {
    Title(TitleHit),
    Check,
    Window,
}

pub struct GraphicsWindow {
    pub frame: Frame,
}

impl GraphicsWindow {
    pub fn new() -> Self {
        let mut frame = Frame::new(GRAPHICS_POSITION, GRAPHICS_WIDTH);
        frame.open = true;
        Self { frame }
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<GraphicsHit> {
        let local = self.frame.local(cursor, GRAPHICS_HEIGHT)?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(GraphicsHit::Title(hit));
        }
        Some(if (GRAPHICS_TOP..GRAPHICS_TOP + GRAPHICS_ROW).contains(&local[1]) && local[0] < SLIDER_X { GraphicsHit::Check } else { GraphicsHit::Window })
    }

    pub fn press(&mut self, hit: GraphicsHit, cursor: [f32; 2]) {
        if hit == GraphicsHit::Title(TitleHit::Title) {
            self.frame.start_drag(cursor);
        }
    }

    pub fn click(&mut self, hit: GraphicsHit, settings: &mut GraphicsSettings) -> bool {
        match hit {
            GraphicsHit::Title(TitleHit::Close) => self.frame.open = false,
            GraphicsHit::Check => settings.effects_on = !settings.effects_on,
            GraphicsHit::Title(_) | GraphicsHit::Window => return false,
        }
        true
    }

    pub fn drag_to(&mut self, cursor: [f32; 2]) -> bool {
        self.frame.drag_to(cursor, GRAPHICS_HEIGHT)
    }

    pub fn drop_drag(&mut self) -> bool {
        self.frame.drop_drag()
    }

    pub fn draw<'a>(
        &self,
        layer: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        font: &Font,
        messages: &[String],
        settings: &GraphicsSettings,
        pointer: [f32; 2],
    ) {
        let at = |local: [f32; 2]| self.frame.at(local);
        self.frame.draw_title(layer, chrome, messages.get(GRAPHICS_TITLE).map_or("", String::as_str), pointer);
        if self.frame.minimized {
            return;
        }
        layer.sprites.push((&chrome.border, at([0.0, TITLE_HEIGHT]), [GRAPHICS_WIDTH, GRAPHICS_HEIGHT - TITLE_HEIGHT]));
        layer.sprites.push((&chrome.white, at([1.0, TITLE_HEIGHT]), [GRAPHICS_WIDTH - 2.0, GRAPHICS_HEIGHT - TITLE_HEIGHT - 1.0]));
        let middle = |height: f32| GRAPHICS_TOP + ((GRAPHICS_ROW - height) / 2.0).floor();
        let check = &assets.check[usize::from(settings.effects_on)];
        layer.sprites.push((check, at([CHECK_X, middle(check.size[1])]), check.size));
        // ponytail: hard-coded until the Effects line in msgstringtable is confirmed.
        layer.texts.push(("Effects".into(), at([CHECK_X + 20.0, middle(font.line_height())]), [0, 0, 0]));
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
    check: [Texture; 2],
    arrows: [Texture; 2],
    track: Texture,
    /// The thumb's left end, middle and right end.
    thumb: [Texture; 3],
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |name: &str| gpu.load_texture(grf, &format!("{UI}{name}.bmp"), false);
        Self {
            check: ["checkbox_0", "checkbox_1"].map(image),
            arrows: ["scroll1left", "scroll1right"].map(image),
            track: image("scroll1mid"),
            thumb: ["scroll1bar_left", "scroll1bar_mid", "scroll1bar_right"].map(image),
        }
    }
}

/// What's under the pointer; settings by number, 0 music and 1 effects.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(TitleHit),
    Check(usize),
    Less(usize),
    More(usize),
    /// The track, and the volume at that point.
    Track(usize, u8),
    Window,
}

pub struct AudioWindow {
    pub frame: Frame,
    /// The slider whose thumb is held.
    dragging: Option<usize>,
}

impl AudioWindow {
    // The menu no longer opens this window. It stays, with the audio settings it already draws.
    #[allow(dead_code)]
    pub fn new() -> Self {
        let mut frame = Frame::new(POSITION, WIDTH);
        frame.open = true;
        Self { frame, dragging: None }
    }

    fn row_top(kind: usize) -> f32 {
        TOP + kind as f32 * ROW
    }

    /// The volume at `x` across the window (the thumb's middle there).
    fn volume_at(x: f32) -> u8 {
        let along = ((x - SLIDER_X - ARROW - THUMB / 2.0) / (TRACK - THUMB)).clamp(0.0, 1.0);
        (along * f32::from(MAX_VOLUME)).round() as u8
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let local = self.frame.local(cursor, HEIGHT)?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(Hit::Title(hit));
        }
        let row = ((local[1] - TOP) / ROW).floor();
        if !(0.0..2.0).contains(&row) {
            return Some(Hit::Window);
        }
        let (kind, x) = (row as usize, local[0]);
        Some(if x < SLIDER_X {
            Hit::Check(kind)
        } else if x < SLIDER_X + ARROW {
            Hit::Less(kind)
        } else if x < SLIDER_X + ARROW + TRACK {
            Hit::Track(kind, Self::volume_at(x))
        } else if x < SLIDER_X + 2.0 * ARROW + TRACK {
            Hit::More(kind)
        } else {
            Hit::Window
        })
    }

    /// A press: the track grabs the thumb there, the title bar the window. Returns whether the settings changed.
    pub fn press(&mut self, hit: Hit, cursor: [f32; 2], settings: &mut Settings) -> bool {
        match hit {
            Hit::Title(TitleHit::Title) => self.frame.start_drag(cursor),
            Hit::Track(kind, volume) => {
                self.dragging = Some(kind);
                settings.volume[kind] = volume;
                return true;
            }
            _ => {}
        }
        false
    }

    /// A click: the checkboxes turn a setting on or off, the arrows step its volume. Returns whether the settings
    /// changed.
    pub fn click(&mut self, hit: Hit, settings: &mut Settings) -> bool {
        match hit {
            Hit::Title(TitleHit::Close) => self.frame.open = false,
            Hit::Check(kind) => settings.on[kind] = !settings.on[kind],
            Hit::Less(kind) => settings.volume[kind] = settings.volume[kind].saturating_sub(STEP),
            Hit::More(kind) => settings.volume[kind] = (settings.volume[kind] + STEP).min(MAX_VOLUME),
            Hit::Title(_) | Hit::Track(..) | Hit::Window => return false,
        }
        true
    }

    /// The held thumb (or the window) follows the pointer; returns whether the settings changed.
    pub fn drag_to(&mut self, cursor: [f32; 2], settings: &mut Settings) -> bool {
        self.frame.drag_to(cursor, HEIGHT);
        let Some(kind) = self.dragging else { return false };
        let volume = Self::volume_at(cursor[0] - self.frame.position[0]);
        std::mem::replace(&mut settings.volume[kind], volume) != volume
    }

    /// Lets go of the thumb or the window; returns whether one was held.
    pub fn drop_drag(&mut self) -> bool {
        self.dragging.take().is_some() | self.frame.drop_drag()
    }

    pub fn draw<'a>(
        &self,
        layer: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        font: &Font,
        messages: &[String],
        settings: &Settings,
        pointer: [f32; 2],
    ) {
        let at = |local: [f32; 2]| self.frame.at(local);
        self.frame.draw_title(layer, chrome, messages.get(TITLE).map_or("", String::as_str), pointer);
        if self.frame.minimized {
            return;
        }
        layer.sprites.push((&chrome.border, at([0.0, TITLE_HEIGHT]), [WIDTH, HEIGHT - TITLE_HEIGHT]));
        layer.sprites.push((&chrome.white, at([1.0, TITLE_HEIGHT]), [WIDTH - 2.0, HEIGHT - TITLE_HEIGHT - 1.0]));
        for (kind, &label) in LABELS.iter().enumerate() {
            let top = Self::row_top(kind);
            let middle = |height: f32| top + ((ROW - height) / 2.0).floor();
            let check = &assets.check[usize::from(settings.on[kind])];
            layer.sprites.push((check, at([CHECK_X, middle(check.size[1])]), check.size));
            layer.texts.push((messages.get(label).cloned().unwrap_or_default(), at([CHECK_X, middle(font.line_height())]), [0, 0, 0]));
            let [less, more] = &assets.arrows;
            layer.sprites.push((less, at([SLIDER_X, middle(ARROW)]), less.size));
            layer.sprites.push((&assets.track, at([SLIDER_X + ARROW, middle(ARROW)]), [TRACK, ARROW]));
            layer.sprites.push((more, at([SLIDER_X + ARROW + TRACK, middle(ARROW)]), more.size));
            // The thumb: its ends around a stretched middle, placed by the volume.
            let left = SLIDER_X + ARROW + (TRACK - THUMB) * f32::from(settings.volume[kind]) / f32::from(MAX_VOLUME);
            let [start, body, end] = &assets.thumb;
            let top = middle(start.size[1]);
            layer.sprites.push((start, at([left, top]), start.size));
            layer.sprites.push((body, at([left + start.size[0], top]), [THUMB - start.size[0] - end.size[0], body.size[1]]));
            layer.sprites.push((end, at([left + THUMB - end.size[0], top]), end.size));
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
    fn graphics_checkbox_toggles_effects() {
        let mut window = GraphicsWindow::new();
        let mut settings = GraphicsSettings::default();
        let at = |local: [f32; 2]| window.frame.at(local);
        assert!(window.frame.open);
        assert!(settings.effects_on);
        assert_eq!(window.hit(at([20.0, GRAPHICS_TOP + GRAPHICS_ROW / 2.0])), Some(GraphicsHit::Check));
        assert!(window.click(GraphicsHit::Check, &mut settings));
        assert!(!settings.effects_on);
    }

    #[test]
    fn graphics_close_closes_window() {
        let mut window = GraphicsWindow::new();
        let mut settings = GraphicsSettings::default();
        assert!(window.click(GraphicsHit::Title(TitleHit::Close), &mut settings));
        assert!(!window.frame.open);
    }

    #[test]
    fn graphics_settings_round_trip() {
        let path = temp_settings("graphics");
        let _ = std::fs::remove_file(&path);
        let settings = GraphicsSettings { effects_on: false };
        settings.save_to(&path);
        assert_eq!(GraphicsSettings::load_from(&path), settings);
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

    #[test]
    fn checks_steps_and_slides() {
        let mut window = AudioWindow::new();
        let mut settings = Settings::default();
        let corner = window.frame.position;
        let at = |local: [f32; 2]| [corner[0] + local[0], corner[1] + local[1]];
        let row = |kind: usize| AudioWindow::row_top(kind) + ROW / 2.0;
        assert_eq!(window.hit(at([20.0, row(1)])), Some(Hit::Check(1)));
        assert!(window.click(Hit::Check(1), &mut settings));
        assert_eq!(window.hit(at([SLIDER_X + 2.0, row(0)])), Some(Hit::Less(0)));
        assert_eq!(window.hit(at([SLIDER_X + ARROW + 1.0, row(0)])), Some(Hit::Track(0, 0)), "the thumb's middle can't go past the track's end");
        assert_eq!(window.hit(at([SLIDER_X + ARROW + TRACK - 1.0, row(0)])), Some(Hit::Track(0, MAX_VOLUME)));
        window.click(Hit::More(0), &mut settings);
        assert_eq!(settings, Settings { on: [true, false], volume: [108, 100] });
        assert!(window.press(Hit::Track(0, 20), at([0.0, 0.0]), &mut settings));
        let middle = window.frame.position[0] + SLIDER_X + ARROW + TRACK / 2.0;
        assert!(window.drag_to([middle, 0.0], &mut settings));
        assert_eq!(settings.volume[0], 64);
        assert!(window.drop_drag());
    }
}
