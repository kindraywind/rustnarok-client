//! The game's option menu (Esc): back to character select, leave the game, or return to it; after dying it also
//! offers going back to the save point.

use roclient::grf::Grf;

use crate::render::{Gpu, Texture};
use crate::window::{Chrome, Frame};
use crate::{Layer, SCREEN, UI, contains};

// ponytail: 280px wide, buttons 221x20 three apart under a 20px top margin; calibrate against the real client.
const WIDTH: f32 = 280.0;
const BUTTON: [f32; 2] = [221.0, 20.0];
const TOP: f32 = 20.0;
const STEP: f32 = 23.0;
const BOTTOM: f32 = 6.0;
const TITLE_MESSAGE: usize = 1483;

/// The menu's buttons, by the bitmap they use (esc_0N).
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Choice {
    SavePoint,
    CharSelect,
    Graphics,
    Sound,
    Hotkeys,
    Exit,
    Cancel,
}

impl Choice {
    fn image(self) -> u8 {
        match self {
            Self::CharSelect => 1,
            Self::Cancel => 2,
            Self::Exit => 3,
            Self::SavePoint => 4,
            Self::Graphics => 6,
            Self::Sound => 7,
            Self::Hotkeys => 8,
        }
    }
}

pub struct Assets {
    title: Texture,
    /// Each button's normal, hovered and pressed images, by esc_0N.
    buttons: Vec<(Choice, [Texture; 3])>,
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |name: String| gpu.load_texture(grf, &name, false);
        let choices = [Choice::SavePoint, Choice::CharSelect, Choice::Graphics, Choice::Sound, Choice::Hotkeys, Choice::Exit, Choice::Cancel];
        Self {
            title: image(format!("{UI}basic_interface\\titlebar_fix.bmp")),
            buttons: choices.into_iter().map(|choice| (choice, ["a", "b", "c"].map(|look| image(format!("{UI}esc_0{}{look}.bmp", choice.image()))))).collect(),
        }
    }
}

pub struct Menu {
    /// Opened by dying: going back to the save point replaces the settings.
    pub dead: bool,
}

impl Menu {
    fn choices(&self) -> Vec<Choice> {
        if self.dead {
            vec![Choice::SavePoint, Choice::CharSelect, Choice::Exit, Choice::Cancel]
        } else {
            vec![Choice::CharSelect, Choice::Graphics, Choice::Sound, Choice::Hotkeys, Choice::Exit, Choice::Cancel]
        }
    }

    fn height(&self) -> f32 {
        TOP + self.choices().len() as f32 * STEP + BOTTOM
    }

    fn corner(&self) -> [f32; 2] {
        [((SCREEN[0] - WIDTH) / 2.0).floor(), ((SCREEN[1] - self.height()) / 2.0).floor()]
    }

    /// Over the menu: the button under the pointer, if any.
    pub fn hit(&self, cursor: [f32; 2]) -> Option<Option<Choice>> {
        let corner = self.corner();
        if !contains(corner, [WIDTH, self.height()], cursor) {
            return None;
        }
        Some(self.choices().into_iter().enumerate().find(|(row, _)| contains(self.button_corner(*row), BUTTON, cursor)).map(|(_, choice)| choice))
    }

    fn button_corner(&self, row: usize) -> [f32; 2] {
        let corner = self.corner();
        [corner[0] + ((WIDTH - BUTTON[0]) / 2.0).floor(), corner[1] + TOP + row as f32 * STEP]
    }

    pub fn draw<'a>(&self, layer: &mut Layer<'a>, chrome: &'a Chrome, assets: &'a Assets, messages: &[String], pointer: ([f32; 2], bool)) {
        let corner = self.corner();
        let height = self.height();
        layer.sprites.push((&chrome.white, corner, [WIDTH, height]));
        layer.sprites.push((&assets.title, corner, [WIDTH, assets.title.size[1]]));
        let mut frame = Frame::new(corner, WIDTH);
        frame.open = true;
        frame.draw_title(layer, chrome, messages.get(TITLE_MESSAGE).map_or("", String::as_str), pointer.0);
        let hovered = self.hit(pointer.0).flatten();
        for (row, choice) in self.choices().into_iter().enumerate() {
            let images = &assets.buttons.iter().find(|(which, _)| *which == choice).unwrap().1;
            let look = if hovered == Some(choice) { 1 + usize::from(pointer.1) } else { 0 };
            layer.sprites.push((&images[look], self.button_corner(row), BUTTON));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offers_the_save_point_after_dying() {
        let alive = Menu { dead: false };
        let dead = Menu { dead: true };
        assert_eq!(dead.choices()[0], Choice::SavePoint);
        assert!(!alive.choices().contains(&Choice::SavePoint));
        let first = dead.button_corner(0);
        assert_eq!(dead.hit([first[0] + 5.0, first[1] + 5.0]), Some(Some(Choice::SavePoint)));
        assert_eq!(dead.hit([0.0, 0.0]), None);
    }
}
