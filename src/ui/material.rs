//! The material window (Change Material, Four Spirit Analysis): inventory items dragged in, each with an amount, and
//! sent all at once with OK. A click on a row takes it out again; Cancel and the close button send nothing in.

use std::collections::HashMap;

use rustnarok_client::lua::ItemInfo;
use rustnarok_client::net::InventoryItem;
use rustnarok_client::text::Font;

use crate::render::Texture;
use crate::window::{self, Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, contains, inventory, pick_list};

// ponytail: the pick list's look (280px, 32px rows, the hint above them, OK and Cancel below), six rows and no scrolling
// (Four Spirit Analysis takes four kinds of item); data.grf's changematerial\ pictures have search and sort buttons
// this window has no use for. Calibrate against the real client.
const WIDTH: f32 = 280.0;
const POSITION: [f32; 2] = [300.0, 172.0];
const HINT: f32 = 22.0;
const ROW: f32 = 32.0;
const ROWS: usize = 6;
const LEFT: f32 = 16.0;
const FOOTER: f32 = 27.0;
const BUTTON: [f32; 2] = [42.0, 20.0];
const TITLE: usize = 1568;
const HINT_MESSAGE: usize = 1569;

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(TitleHit),
    Row(usize),
    Ok,
    Cancel,
    Window,
}

pub struct Materials {
    pub frame: Frame,
    /// What the window is for, sent back as it came: 0 Change Material, 1 or 2 Four Spirit Analysis.
    pub kind: u32,
    /// What's in: inventory index and amount, in the order put in.
    pub entries: Vec<(u16, u16)>,
}

impl Materials {
    pub fn new(kind: u32) -> Self {
        let mut frame = Frame::new(POSITION, WIDTH);
        frame.open = true;
        Self { frame, kind, entries: Vec::new() }
    }

    fn height() -> f32 {
        TITLE_HEIGHT + HINT + ROWS as f32 * ROW + FOOTER
    }

    fn buttons() -> ([f32; 2], [f32; 2]) {
        let top = Self::height() - FOOTER + 4.0;
        ([WIDTH - 62.0 - BUTTON[0], top], [WIDTH - 15.0 - BUTTON[0], top])
    }

    /// Puts `amount` of an item in; more of one already in adds up, to no more than the `carried`.
    pub fn add(&mut self, index: u16, amount: u16, carried: u16) {
        match self.entries.iter_mut().find(|(put, _)| *put == index) {
            Some((_, have)) => *have = have.saturating_add(amount).min(carried),
            None if amount > 0 => self.entries.push((index, amount.min(carried))),
            None => {}
        }
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let local = self.frame.local(cursor, Self::height())?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(Hit::Title(hit));
        }
        let (ok, cancel) = Self::buttons();
        if contains(ok, BUTTON, local) {
            return Some(Hit::Ok);
        }
        if contains(cancel, BUTTON, local) {
            return Some(Hit::Cancel);
        }
        let top = TITLE_HEIGHT + HINT;
        if local[1] >= top && local[1] < top + ROWS as f32 * ROW {
            let row = ((local[1] - top) / ROW) as usize;
            if row < self.entries.len() {
                return Some(Hit::Row(row));
            }
        }
        Some(Hit::Window)
    }

    /// What a click answers: Some(true) sends what's in, Some(false) lets the window go, None keeps it open.
    pub fn click(&mut self, hit: Hit) -> Option<bool> {
        match hit {
            Hit::Title(TitleHit::Close) | Hit::Cancel => return Some(false),
            Hit::Title(title) => self.frame.click_title(title),
            Hit::Row(row) => _ = self.entries.remove(row),
            Hit::Ok if !self.entries.is_empty() => return Some(true),
            Hit::Ok | Hit::Window => {}
        }
        None
    }

    pub fn icons(&self, items: &HashMap<u32, ItemInfo>, carried: &[InventoryItem]) -> Vec<String> {
        self.entries
            .iter()
            .filter_map(|(index, _)| carried.iter().find(|item| item.index == *index))
            .filter_map(|item| items.get(&item.id).map(|info| inventory::icon_path(info, item.identified)))
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &self,
        layer: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a pick_list::Assets,
        icons: &'a HashMap<String, Option<Texture>>,
        items: &HashMap<u32, ItemInfo>,
        carried: &[InventoryItem],
        messages: &[String],
        font: &Font,
        pointer: ([f32; 2], bool),
    ) {
        let frame = &self.frame;
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        frame.draw_title(layer, chrome, message(TITLE), pointer.0);
        if frame.minimized {
            return;
        }
        let height = Self::height();
        let body = height - TITLE_HEIGHT - FOOTER;
        layer.sprites.push((&chrome.white, frame.at([0.0, TITLE_HEIGHT]), [WIDTH, body]));
        layer.sprites.push((&assets.strip, frame.at([0.0, TITLE_HEIGHT]), [assets.strip.size[0], body]));
        layer.sprites.push((&chrome.border, frame.at([WIDTH - 1.0, TITLE_HEIGHT]), [1.0, body]));
        layer.texts.push((message(HINT_MESSAGE).to_owned(), frame.at([LEFT + 4.0, TITLE_HEIGHT + 5.0]), [0x60, 0x60, 0x60]));
        let top = TITLE_HEIGHT + HINT;
        for (row, &(index, amount)) in self.entries.iter().enumerate().take(ROWS) {
            let Some(item) = carried.iter().find(|item| item.index == index) else { continue };
            let y = top + row as f32 * ROW;
            let icon = items.get(&item.id).and_then(|info| icons.get(&inventory::icon_path(info, item.identified))).and_then(Option::as_ref);
            if let Some(icon) = icon {
                layer.sprites.push((icon, frame.at([LEFT + 4.0, y + 4.0]), icon.size));
            }
            let name = inventory::item_name(items, item.id, item.identified, item.refine);
            let count = amount.to_string();
            layer.texts.push((window::fit(font, &name, WIDTH - LEFT - 90.0), frame.at([LEFT + 32.0, y + 10.0]), [0, 0, 0]));
            layer.texts.push((count.clone(), frame.at([WIDTH - 12.0 - font.width(&count), y + 10.0]), [0, 0, 0]));
        }
        layer.sprites.push((&assets.bar, frame.at([0.0, height - FOOTER]), [WIDTH, FOOTER]));
        let hit = self.hit(pointer.0);
        let look = |target: Hit| {
            let hovered = hit == Some(target);
            usize::from(hovered) + usize::from(hovered && pointer.1)
        };
        let (ok, cancel) = Self::buttons();
        layer.sprites.push((&assets.buttons[0][look(Hit::Ok)], frame.at(ok), BUTTON));
        layer.sprites.push((&assets.buttons[1][look(Hit::Cancel)], frame.at(cancel), BUTTON));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn puts_items_in_and_takes_them_out() {
        let mut window = Materials::new(1);
        // Nothing in: OK does nothing.
        assert_eq!(window.click(Hit::Ok), None);
        window.add(5, 4, 10);
        window.add(5, 9, 10);
        window.add(7, 3, 3);
        window.add(8, 0, 3);
        assert_eq!(window.entries, [(5, 10), (7, 3)]);
        assert_eq!(window.click(Hit::Row(0)), None);
        assert_eq!(window.entries, [(7, 3)]);
        assert_eq!(window.click(Hit::Ok), Some(true));
        assert_eq!(window.click(Hit::Cancel), Some(false));
        assert_eq!(window.click(Hit::Title(TitleHit::Close)), Some(false));
    }
}
