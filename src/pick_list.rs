//! The lists the server asks the player to pick from: what a card goes into, what a Magnifier appraises, what to
//! make, which spell to keep ready. A titled window of rows (the item's or skill's icon and name), OK (or Make) and
//! Cancel.

use std::collections::HashMap;

use rustnarok_client::grf::Grf;
use rustnarok_client::lua::{ItemInfo, SkillInfo};
use rustnarok_client::net::InventoryItem;
use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};
use crate::window::{self, Chrome, Clicks, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, SCREEN, UI, contains};
use crate::{inventory, skills};

// ponytail: the shop's look (280px, 32px rows), five rows, in the middle of the screen; calibrate against the real
// client's lists, and give rows other than items (skills, things to make) when a list needs them.
const WIDTH: f32 = 280.0;
const ROW: f32 = 32.0;
const ROWS: usize = 5;
const PADDING: f32 = 5.0;
const LEFT: f32 = 16.0;
const FOOTER: f32 = 27.0;
const BUTTON: [f32; 2] = [42.0, 20.0];
const SELECTED: [u8; 4] = [0x34, 0x6A, 0xE1, 0x80];

pub struct Assets {
    pub strip: Texture,
    pub bar: Texture,
    /// OK, cancel and make: normal, hovered, pressed.
    pub buttons: [[Texture; 3]; 3],
    selected: Texture,
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |path: String| gpu.load_texture(grf, &path, false);
        let button = |name: &str| ["", "_a", "_b"].map(|suffix| image(format!("{UI}btn_{name}{suffix}.bmp")));
        Self {
            strip: image(format!("{UI}basic_interface\\itemwin_left.bmp")),
            bar: image(format!("{UI}basic_interface\\btnbar_mid2.bmp")),
            buttons: [button("ok"), button("cancel"), button("make")],
            selected: gpu.solid(SELECTED),
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(TitleHit),
    Row(usize),
    Ok,
    Cancel,
    Window,
}

pub struct PickList {
    pub frame: Frame,
    /// The title's message number.
    title: usize,
    /// The items to pick from: carried ones, or (to make) just their IDs.
    pub items: Vec<InventoryItem>,
    /// Make rather than OK on the button.
    pub making: bool,
    /// The rows are skills (their IDs in `id`), not items.
    pub skills: bool,
    selected: Option<usize>,
    scroll: usize,
    clicks: Clicks,
}

impl PickList {
    pub fn new(title: usize, items: Vec<InventoryItem>) -> Self {
        let mut frame = Frame::new([((SCREEN[0] - WIDTH) / 2.0).floor(), ((SCREEN[1] - Self::height()) / 2.0).floor()], WIDTH);
        frame.open = true;
        Self { frame, title, items, making: false, skills: false, selected: None, scroll: 0, clicks: Clicks::default() }
    }

    fn height() -> f32 {
        TITLE_HEIGHT + PADDING * 2.0 + ROWS as f32 * ROW + FOOTER
    }

    fn buttons() -> ([f32; 2], [f32; 2]) {
        let top = Self::height() - FOOTER + 4.0;
        ([WIDTH - 62.0 - BUTTON[0], top], [WIDTH - 15.0 - BUTTON[0], top])
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
        let top = TITLE_HEIGHT + PADDING;
        if local[1] >= top && local[1] < top + ROWS as f32 * ROW {
            let row = ((local[1] - top) / ROW) as usize + self.scroll;
            if row < self.items.len() {
                return Some(Hit::Row(row));
            }
        }
        Some(Hit::Window)
    }

    pub fn covers(&self, cursor: [f32; 2]) -> bool {
        self.hit(cursor).is_some()
    }

    /// What a click answers: Some(Some(row)) picks that row's item, Some(None) cancels, None is no answer yet.
    pub fn click(&mut self, hit: Hit) -> Option<Option<usize>> {
        match hit {
            Hit::Title(TitleHit::Close) | Hit::Cancel => return Some(None),
            Hit::Title(title) => self.frame.click_title(title),
            Hit::Row(row) => {
                self.selected = Some(row);
                if self.clicks.double(row as u16) {
                    return Some(Some(row));
                }
            }
            Hit::Ok => return self.selected.map(Some),
            Hit::Window => {}
        }
        None
    }

    pub fn scroll_by(&mut self, rows: i32) {
        let most = self.items.len().saturating_sub(ROWS);
        self.scroll = (self.scroll as i32 + rows).clamp(0, most as i32) as usize;
    }

    /// The rows on screen: each item's ID and its box.
    pub fn row_boxes(&self) -> Vec<(u32, [f32; 4])> {
        if !self.frame.open || self.frame.minimized {
            return Vec::new();
        }
        let top = TITLE_HEIGHT + PADDING;
        let rows = self.items.iter().enumerate().skip(self.scroll).take(ROWS);
        rows.map(|(row, item)| {
            let [x, y] = self.frame.at([LEFT, top + (row - self.scroll) as f32 * ROW]);
            (item.id, [x, y, WIDTH - LEFT - 1.0, ROW])
        })
        .collect()
    }

    pub fn icons(&self, items: &HashMap<u32, ItemInfo>, skill_info: &HashMap<u16, SkillInfo>) -> Vec<String> {
        self.items.iter().filter_map(|row| self.icon_path(row, items, skill_info)).collect()
    }

    fn icon_path(&self, row: &InventoryItem, items: &HashMap<u32, ItemInfo>, skill_info: &HashMap<u16, SkillInfo>) -> Option<String> {
        if self.skills {
            return u16::try_from(row.id).ok().and_then(|skill| skill_info.get(&skill)).map(skills::icon_path);
        }
        items.get(&row.id).map(|info| inventory::icon_path(info, row.identified))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &self,
        layer: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        icons: &'a HashMap<String, Option<Texture>>,
        items: &HashMap<u32, ItemInfo>,
        skill_info: &HashMap<u16, SkillInfo>,
        messages: &[String],
        font: &Font,
        pointer: ([f32; 2], bool),
    ) {
        let frame = &self.frame;
        frame.draw_title(layer, chrome, messages.get(self.title).map_or("", String::as_str), pointer.0);
        if frame.minimized {
            return;
        }
        let height = Self::height();
        let list_height = height - TITLE_HEIGHT - FOOTER;
        layer.sprites.push((&chrome.white, frame.at([0.0, TITLE_HEIGHT]), [WIDTH, list_height]));
        layer.sprites.push((&assets.strip, frame.at([0.0, TITLE_HEIGHT]), [assets.strip.size[0], list_height]));
        layer.sprites.push((&chrome.border, frame.at([WIDTH - 1.0, TITLE_HEIGHT]), [1.0, list_height]));
        let top = TITLE_HEIGHT + PADDING;
        for (row, item) in self.items.iter().enumerate().skip(self.scroll).take(ROWS) {
            let y = top + (row - self.scroll) as f32 * ROW;
            if self.selected == Some(row) {
                layer.sprites.push((&assets.selected, frame.at([LEFT, y]), [WIDTH - LEFT - 1.0, ROW]));
            }
            let icon = self.icon_path(item, items, skill_info).and_then(|path| icons.get(&path)).and_then(Option::as_ref);
            if let Some(icon) = icon {
                layer.sprites.push((icon, frame.at([LEFT + 4.0, y + 4.0]), icon.size));
            }
            let name = if self.skills {
                u16::try_from(item.id).ok().and_then(|skill| skill_info.get(&skill)).map_or_else(|| item.id.to_string(), |info| info.name.clone())
            } else {
                inventory::item_name(items, item.id, item.identified, item.refine)
            };
            layer.texts.push((window::fit(font, &name, WIDTH - LEFT - 40.0), frame.at([LEFT + 32.0, y + 10.0]), [0, 0, 0]));
        }
        let footer = height - FOOTER;
        layer.sprites.push((&assets.bar, frame.at([0.0, footer]), [WIDTH, FOOTER]));
        let hit = self.hit(pointer.0);
        let look = |target: Hit| {
            let hovered = hit == Some(target);
            usize::from(hovered) + usize::from(hovered && pointer.1)
        };
        let (ok, cancel) = Self::buttons();
        layer.sprites.push((&assets.buttons[if self.making { 2 } else { 0 }][look(Hit::Ok)], frame.at(ok), BUTTON));
        layer.sprites.push((&assets.buttons[1][look(Hit::Cancel)], frame.at(cancel), BUTTON));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_and_cancels() {
        let item = |index: u16, id: u32| InventoryItem { index, id, identified: true, ..Default::default() };
        let mut list = PickList::new(522, vec![item(4, 1201), item(7, 2301)]);
        let [x, y, ..] = list.row_boxes()[1].1;
        let row = list.hit([x + 40.0, y + 10.0]).unwrap();
        assert_eq!(row, Hit::Row(1));
        // OK picks the highlighted row; a double click picks at once; Cancel answers no.
        assert_eq!(list.click(Hit::Ok), None);
        assert_eq!(list.click(row), None);
        assert_eq!(list.click(Hit::Ok), Some(Some(1)));
        assert_eq!(list.click(Hit::Row(0)), None);
        assert_eq!(list.click(Hit::Row(0)), Some(Some(0)));
        assert_eq!(list.click(Hit::Cancel), Some(None));
        assert_eq!(list.click(Hit::Title(TitleHit::Close)), Some(None));
    }
}
