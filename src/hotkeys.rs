//! The hotkey bar (F1-F9, F12 shows or hides it): items and skills kept at hand, rows of nine.

use std::collections::HashMap;

use rustnarok_client::grf::Grf;
use rustnarok_client::lua::{ItemInfo, SkillInfo};
use rustnarok_client::net::{Hotkey, InventoryItem};
use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};
use crate::window::{self, Chrome, Frame};
use crate::{Layer, UI, contains, inventory};

// ponytail: places over shortitem_bg.bmp; calibrate against the real client.
const WIDTH: f32 = 280.0;
const ROW_HEIGHT: f32 = 34.0;
pub const PER_ROW: usize = 9;
const ROWS: usize = 4;
const SLOT: [f32; 2] = [24.0, 23.0];
const SLOT_STEP: f32 = 29.0;
const FIRST_SLOT: [f32; 2] = [5.0, 5.0];
const ROW_NUMBER: [f32; 2] = [FIRST_SLOT[0] + PER_ROW as f32 * SLOT_STEP + 5.0, 13.0];
const CLOSE: [f32; 2] = [WIDTH - 13.0, 2.0];
const BUTTON_SIZE: [f32; 2] = [11.0, 11.0];
const DEFAULT_POSITION: [f32; 2] = [480.0, 0.0];

pub struct Assets {
    background: Texture,
    close: Texture,
    resize: Texture,
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |path: String| gpu.load_texture(grf, &path, false);
        Self {
            background: image(format!("{UI}basic_interface\\shortitem_bg.bmp")),
            close: image(format!("{UI}basic_interface\\sys_close_off.bmp")),
            resize: image(format!("{UI}btn_resize.bmp")),
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Close,
    Resize,
    Slot(usize),
    Window,
}

pub struct Hotkeys {
    pub frame: Frame,
    /// Every slot of the server's first tab (four rows of nine and two spare).
    pub keys: Vec<Option<Hotkey>>,
    /// How many rows show.
    rows: usize,
}

impl Default for Hotkeys {
    fn default() -> Self {
        let mut frame = Frame::new(DEFAULT_POSITION, WIDTH);
        frame.open = true;
        Self { frame, keys: vec![None; ROWS * PER_ROW], rows: 1 }
    }
}

impl Hotkeys {
    pub fn height(&self) -> f32 {
        self.rows as f32 * ROW_HEIGHT
    }

    /// The slot (row by row) under the pointer.
    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let local = self.frame.local(cursor, self.height())?;
        if contains(CLOSE, BUTTON_SIZE, local) {
            return Some(Hit::Close);
        }
        if contains([WIDTH - 14.0, self.height() - 14.0], [13.0, 13.0], local) {
            return Some(Hit::Resize);
        }
        Some((0..self.rows * PER_ROW).find(|&slot| contains(slot_corner(slot), SLOT, local)).map_or(Hit::Window, Hit::Slot))
    }

    pub fn click(&mut self, hit: Hit) {
        match hit {
            Hit::Close => self.frame.open = false,
            Hit::Resize => self.rows = self.rows % ROWS + 1,
            Hit::Slot(_) | Hit::Window => {}
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &self,
        layer: &mut Layer<'a>,
        tooltip: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        icons: &'a HashMap<String, Option<Texture>>,
        items: &HashMap<u32, ItemInfo>,
        skills: &HashMap<u16, SkillInfo>,
        inventory: &[InventoryItem],
        font: &Font,
        cursor: [f32; 2],
    ) {
        if !self.frame.open {
            return;
        }
        for row in 0..self.rows {
            let top = row as f32 * ROW_HEIGHT;
            layer.sprites.push((&assets.background, self.frame.at([0.0, top]), assets.background.size));
            let number = (row + 1).to_string();
            layer.texts.push((number, self.frame.at([ROW_NUMBER[0], top + ROW_NUMBER[1] - 4.0]), [0, 0, 0]));
        }
        layer.sprites.push((&assets.close, self.frame.at(CLOSE), assets.close.size));
        layer.sprites.push((&assets.resize, self.frame.at([WIDTH - 14.0, self.height() - 14.0]), assets.resize.size));
        for (slot, key) in self.keys.iter().enumerate().take(self.rows * PER_ROW) {
            let corner = self.frame.at(slot_corner(slot));
            // A skill: its icon, the name on hover.
            if let Some(Hotkey { skill: true, id, count }) = key
                && let Some(info) = u16::try_from(*id).ok().and_then(|id| skills.get(&id))
            {
                if let Some(Some(icon)) = icons.get(&crate::skills::icon_path(info)) {
                    layer.sprites.push((icon, corner, icon.size));
                }
                // The level it's used at, bottom right, as an item's amount.
                count_text(layer, font, corner, &count.to_string());
                if contains(corner, SLOT, cursor) {
                    window::tooltip(tooltip, chrome, font, &format!("{} Lv {count}", info.name), [corner[0], corner[1] + SLOT[1] + 2.0], [255; 3]);
                }
                continue;
            }
            let Some(Hotkey { skill: false, id, .. }) = key else { continue };
            let Some(info) = items.get(id) else { continue };
            if let Some(Some(icon)) = icons.get(&inventory::icon_path(info, true)) {
                layer.sprites.push((icon, corner, icon.size));
            }
            // How many are carried, bottom right.
            let amount: u32 = inventory.iter().filter(|item| item.id == *id && item.worn == 0).map(|item| u32::from(item.amount)).sum();
            count_text(layer, font, corner, &amount.to_string());
            if contains(corner, SLOT, cursor) {
                window::tooltip(tooltip, chrome, font, info.name(true), [corner[0], corner[1] + SLOT[1] + 2.0], [255; 3]);
            }
        }
    }
}

/// A number at a slot's bottom right: black on a white shadow.
fn count_text(layer: &mut Layer<'_>, font: &Font, corner: [f32; 2], text: &str) {
    let place = [corner[0] + SLOT[0] - 1.0 - font.width(text), corner[1] + 12.0];
    layer.texts.push((text.to_owned(), [place[0] + 1.0, place[1] + 1.0], [255; 3]));
    layer.texts.push((text.to_owned(), place, [0, 0, 0]));
}

/// A slot's top-left corner inside the bar.
fn slot_corner(slot: usize) -> [f32; 2] {
    [FIRST_SLOT[0] + (slot % PER_ROW) as f32 * SLOT_STEP, (slot / PER_ROW) as f32 * ROW_HEIGHT + FIRST_SLOT[1]]
}
