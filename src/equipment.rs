//! The equipment window (Alt+Q): what the player wears on the client's slot bitmap, with the character in the middle;
//! and the titles the player's achievements gave, to wear one.

use std::collections::HashMap;

use image::imageops;
use rustnarok_client::grf::Grf;
use rustnarok_client::lua::ItemInfo;
use rustnarok_client::net::InventoryItem;
use rustnarok_client::text::Font;

use crate::inventory::{self, UNIDENTIFIED};
use crate::render::{self, Gpu, Sprite, Texture};
use crate::window::{self, Chrome, Clicks, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI, contains, offset};

// ponytail: layout over the bitmaps; calibrate against the real client.
const WIDTH: f32 = 280.0;
const TABS_HEIGHT: f32 = 20.0;
const PANEL_TOP: f32 = TITLE_HEIGHT + TABS_HEIGHT;
const PANEL_HEIGHT: f32 = 130.0;
/// The striped bottom of equipwin_bg2.bmp.
const FOOTER: [u32; 2] = [130, 27];
const HEIGHT: f32 = PANEL_TOP + PANEL_HEIGHT + FOOTER[1] as f32;
const ROW: f32 = 26.0;
const ICON: f32 = 24.0;
/// Icons sit 4px in from either side; names fill up to 80px beside them, two lines at most.
const ICON_MARGIN: f32 = 4.0;
const NAME_WIDTH: f32 = 80.0;
const NAME_LINE: f32 = 12.0;
/// Each side's slots take this much of the width; the character stands between them.
const COLUMN_WIDTH: f32 = 115.0;
/// Where the character stands: the shadow baked into equipwin_bg.bmp.
const PREVIEW_FEET: [f32; 2] = [140.0, PANEL_TOP + 114.0];
/// Where the client first opens it.
const DEFAULT_POSITION: [f32; 2] = [480.0, 200.0];
/// The general and the special (costume and shadow gear) tabs, and the titles (the player's own window only).
// ponytail: equipwin_bg3.bmp draws two tabs; the third's label goes beside them.
const TAB_LABELS: [usize; 3] = [3158, 3159, 3160];
const TAB_TEXT: [[f32; 2]; 3] = [[8.0, TITLE_HEIGHT + 3.0], [90.0, TITLE_HEIGHT + 3.0], [172.0, TITLE_HEIGHT + 3.0]];
const TAB_WIDTH: [f32; 3] = [80.0, 80.0, 80.0];
const TITLES_TAB: usize = 2;
/// "Disable Title", first in the titles' list, and the list's rows.
const NO_TITLE: usize = 2686;
const TITLE_ROW: f32 = 18.0;
/// Take Off All, in the footer's right corner, and its name.
const TAKE_OFF: [f32; 2] = [40.0, 20.0];
const TAKE_OFF_NAME: usize = 3955;
const SHOW_EQUIPMENT_MESSAGE: usize = 1362;
/// "%s's Equipment": another player's window.
const OTHERS_TITLE: usize = 1361;
/// Where another player's window opens.
// ponytail: beside the player's own; the real client's place isn't known.
const OTHERS_POSITION: [f32; 2] = [500.0, 220.0];
const TITLE_MESSAGE: usize = 104;
const GREY: [u8; 3] = [0x80, 0x80, 0x80];

/// Equip location bits (rathena's equip_pos) and where each shows: column (0 left, 1 right) and row.
const SLOTS: [[(u32, usize, usize); 10]; 2] = [
    [
        (256, 0, 0), // top headgear
        (512, 1, 0), // middle headgear
        (1, 0, 1),   // lower headgear
        (16, 1, 1),  // armor
        (2, 0, 2),   // weapon
        (32, 1, 2),  // shield
        (4, 0, 3),   // garment
        (64, 1, 3),  // shoes
        (8, 0, 4),   // accessory
        (128, 1, 4), // accessory
    ],
    [
        (1024, 0, 0),    // costume top
        (2048, 1, 0),    // costume middle
        (4096, 0, 1),    // costume lower
        (65536, 1, 1),   // shadow armor
        (131072, 0, 2),  // shadow weapon
        (262144, 1, 2),  // shadow shield
        (8192, 0, 3),    // costume garment
        (524288, 1, 3),  // shadow shoes
        (1048576, 0, 4), // shadow accessory
        (2097152, 1, 4), // shadow accessory
    ],
];

pub struct Assets {
    panels: [Texture; 2],
    tabs: Texture,
    footer: Texture,
    /// The Show Equipment box: off, on.
    checkbox: [Texture; 2],
    /// Swap equipment and take everything off; Take Off All pointed at.
    buttons: [Texture; 2],
    take_off_hover: Texture,
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let basic = format!("{UI}basic_interface\\");
        let image = |path: String| gpu.load_texture(grf, &path, false);
        let footer = render::load_image(grf, &format!("{basic}equipwin_bg2.bmp"))
            .map(|image| imageops::crop_imm(&image, 0, FOOTER[0], image.width(), FOOTER[1]).to_image());
        Self {
            panels: [image(format!("{basic}equipwin_bg.bmp")), image(format!("{basic}equipwin_special.bmp"))],
            tabs: image(format!("{basic}equipwin_bg3.bmp")),
            footer: footer.map_or_else(|_| gpu.solid([0xF0; 4]), |footer| gpu.texture(&footer, false)),
            checkbox: ["0", "1"].map(|look| image(format!("{UI}checkbox_{look}.bmp"))),
            buttons: [image(format!("{basic}btn_e_change_a.bmp")), image(format!("{basic}btn_e_off_a.bmp"))],
            take_off_hover: image(format!("{basic}btn_e_off_b.bmp")),
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Hit {
    Title(TitleHit),
    Tab(usize),
    /// A worn item, by inventory index.
    Slot(u16),
    /// The Show Equipment box and its label.
    ShowBox,
    /// A title to wear (0: none).
    WearTitle(u32),
    TakeOffAll,
    Window,
}

pub struct Equipment {
    pub frame: Frame,
    tab: usize,
    clicks: Clicks,
    /// Another player's window: their name (for the title; no footer). None for the player's own.
    pub owner: Option<String>,
    /// Whether others may see what the player wears (the Show Equipment box), as the server last said.
    pub shown: bool,
    /// The titles the player's achievements gave (ID, name), and the one worn (0: none).
    pub titles: Vec<(u32, String)>,
    pub title: u32,
}

impl Default for Equipment {
    fn default() -> Self {
        Self { frame: Frame::new(DEFAULT_POSITION, WIDTH), tab: 0, clicks: Clicks::default(), owner: None, shown: false, titles: Vec::new(), title: 0 }
    }
}

/// Take Off All's corner inside the window.
fn take_off_at() -> [f32; 2] {
    [WIDTH - 45.0, PANEL_TOP + PANEL_HEIGHT + 4.0]
}

/// A slot's top-left corner and size inside the window.
fn slot_area(column: usize, row: usize) -> ([f32; 2], [f32; 2]) {
    let x = if column == 0 { 0.0 } else { WIDTH - COLUMN_WIDTH };
    ([x, PANEL_TOP + row as f32 * ROW], [COLUMN_WIDTH, ROW])
}

impl Equipment {
    /// Another player's equipment, open.
    pub fn of(name: String) -> Self {
        let mut frame = Frame::new(OTHERS_POSITION, WIDTH);
        frame.open = true;
        Self { frame, owner: Some(name), ..Self::default() }
    }

    pub fn height(&self) -> f32 {
        HEIGHT
    }

    /// The worn item in each slot of the current tab, with the slot's place.
    fn slots<'i>(&self, worn: &'i [InventoryItem]) -> impl Iterator<Item = (usize, usize, &'i InventoryItem)> {
        SLOTS
            .get(self.tab)
            .into_iter()
            .flatten()
            .filter_map(move |&(location, column, row)| Some((column, row, worn.iter().find(|item| item.worn & location != 0)?)))
    }

    pub fn hit(&self, cursor: [f32; 2], items: &[InventoryItem]) -> Option<Hit> {
        let local = self.frame.local(cursor, HEIGHT)?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(Hit::Title(hit));
        }
        // ponytail: the tab strip's hit areas are guessed from equipwin_bg3.bmp's divider.
        if local[1] < PANEL_TOP {
            return Some(
                (0..self.tabs())
                    .find(|&tab| contains([TAB_TEXT[tab][0] - 4.0, TITLE_HEIGHT], [TAB_WIDTH[tab], TABS_HEIGHT], local))
                    .map_or(Hit::Window, Hit::Tab),
            );
        }
        // The titles: "none" first, then each one.
        if self.tab == TITLES_TAB && local[1] < PANEL_TOP + PANEL_HEIGHT {
            let row = ((local[1] - PANEL_TOP - 4.0) / TITLE_ROW).max(0.0) as usize;
            let picked = std::iter::once(0).chain(self.titles.iter().map(|(id, _)| *id)).nth(row);
            return Some(picked.map_or(Hit::Window, Hit::WearTitle));
        }
        // ponytail: the box's label is taken to run 120px from the box.
        let footer_top = PANEL_TOP + PANEL_HEIGHT;
        if self.owner.is_none() && contains([6.0, footer_top + 6.0], [120.0, 14.0], local) {
            return Some(Hit::ShowBox);
        }
        if self.owner.is_none() && contains(take_off_at(), TAKE_OFF, local) {
            return Some(Hit::TakeOffAll);
        }
        Some(
            self.slots(items)
                .find(|&(column, row, _)| {
                    let (corner, size) = slot_area(column, row);
                    contains(corner, size, local)
                })
                .map_or(Hit::Window, |(.., item)| Hit::Slot(item.index)),
        )
    }

    /// Reacts to a click; returns the item to take off on a double click.
    pub fn click(&mut self, hit: Hit) -> Option<u16> {
        match hit {
            Hit::Title(hit) => self.frame.click_title(hit),
            Hit::Tab(tab) => self.tab = tab,
            // Someone else's items stay on.
            Hit::Slot(index) if self.owner.is_none() => return self.clicks.double(index).then_some(index),
            Hit::Slot(_) | Hit::ShowBox | Hit::WearTitle(_) | Hit::TakeOffAll | Hit::Window => {}
        }
        None
    }

    /// How many tabs show: the titles only on the player's own window.
    fn tabs(&self) -> usize {
        if self.owner.is_none() { TAB_LABELS.len() } else { TITLES_TAB }
    }

    /// The tabs and the titles on screen, named for scripted clicks: `equipment_tab:<n>`, `title:<id>` (0: none).
    pub fn button_boxes(&self) -> Vec<(String, [f32; 4])> {
        if !self.frame.open || self.frame.minimized {
            return Vec::new();
        }
        let place = |local: [f32; 2], size: [f32; 2]| {
            let [x, y] = self.frame.at(local);
            [x, y, size[0], size[1]]
        };
        let tabs = (0..self.tabs()).map(|tab| (format!("equipment_tab:{tab}"), place([TAB_TEXT[tab][0] - 4.0, TITLE_HEIGHT], [TAB_WIDTH[tab], TABS_HEIGHT])));
        let mut boxes: Vec<_> = tabs.collect();
        if self.owner.is_none() {
            boxes.push(("take_off_all".to_owned(), place(take_off_at(), TAKE_OFF)));
        }
        if self.tab == TITLES_TAB {
            let ids = std::iter::once(0).chain(self.titles.iter().map(|(id, _)| *id));
            boxes.extend(ids.enumerate().map(|(row, id)| (format!("title:{id}"), place([0.0, PANEL_TOP + 4.0 + row as f32 * TITLE_ROW], [WIDTH, TITLE_ROW]))));
        }
        boxes
    }

    /// Where the character preview stands on screen, while it shows.
    pub fn preview_feet(&self) -> Option<[f32; 2]> {
        (self.frame.open && !self.frame.minimized && self.tab == 0).then(|| self.frame.at(PREVIEW_FEET))
    }

    /// Draws the window into `layer` (with the character's `preview` sprites) and the hovered slot into `tooltip`.
    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &self,
        layer: &mut Layer<'a>,
        tooltip: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        icons: &'a HashMap<String, Option<Texture>>,
        items: &HashMap<u32, ItemInfo>,
        messages: &[String],
        font: &Font,
        cursor: [f32; 2],
        worn: &[InventoryItem],
        preview: Vec<Sprite<'a>>,
    ) {
        if !self.frame.open {
            return;
        }
        let at = |local: [f32; 2]| self.frame.at(local);
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        let title = match &self.owner {
            Some(name) => rustnarok_client::messages::fill(message(OTHERS_TITLE), &[name]),
            None => message(TITLE_MESSAGE).to_owned(),
        };
        self.frame.draw_title(layer, chrome, &title, cursor);
        if self.frame.minimized {
            return;
        }

        // Tabs, the slot panel and the character standing on its shadow.
        layer.sprites.push((&assets.tabs, at([0.0, TITLE_HEIGHT]), assets.tabs.size));
        for (tab, &label) in TAB_LABELS.iter().enumerate().take(self.tabs()) {
            // Kept inside its tab, however long the translation's title is.
            let title = window::fit(font, message(label), TAB_WIDTH[tab] - 6.0);
            layer.texts.push((title, at(TAB_TEXT[tab]), if tab == self.tab { [0, 0, 0] } else { GREY }));
        }
        match assets.panels.get(self.tab) {
            Some(panel) => layer.sprites.push((panel, at([0.0, PANEL_TOP]), panel.size)),
            None => layer.sprites.push((&chrome.white, at([0.0, PANEL_TOP]), [WIDTH, PANEL_HEIGHT])),
        }
        layer.sprites.extend(preview);
        // The titles: none, then each earned one; the worn one black, the others grey.
        if self.tab == TITLES_TAB {
            let rows = std::iter::once((0, message(NO_TITLE).to_owned())).chain(self.titles.iter().cloned());
            for (row, (id, name)) in rows.enumerate().take((PANEL_HEIGHT / TITLE_ROW) as usize) {
                let color = if id == self.title { [0, 0, 0] } else { GREY };
                layer.texts.push((window::fit(font, &name, WIDTH - 20.0), at([10.0, PANEL_TOP + 6.0 + row as f32 * TITLE_ROW]), color));
            }
        }

        for (column, row, item) in self.slots(worn) {
            let (corner, size) = slot_area(column, row);
            let top = PANEL_TOP + row as f32 * ROW + 1.0;
            let icon_x = if column == 0 { ICON_MARGIN } else { WIDTH - ICON_MARGIN - ICON };
            let info = items.get(&item.id);
            if let Some(Some(icon)) = info.and_then(|info| icons.get(&inventory::icon_path(info, item.identified))) {
                layer.sprites.push((icon, at([icon_x, top]), icon.size));
            }
            let name = inventory::item_name(items, item.id, item.identified, item.refine);
            let lines = window::wrap(font, &name, NAME_WIDTH, 2);
            let first_line = top + (ICON - lines.len() as f32 * NAME_LINE) / 2.0 - 2.0;
            for (number, line) in lines.iter().enumerate() {
                let x = if column == 0 { icon_x + ICON + 2.0 } else { icon_x - 2.0 - font.width(line) };
                let place = at([x, first_line + number as f32 * NAME_LINE]);
                layer.texts.push((line.clone(), offset(place, [1.0, 1.0]), [255; 3]));
                layer.texts.push((line.clone(), place, [0, 0, 0]));
            }
            if contains(at(corner), size, cursor) {
                window::tooltip(tooltip, chrome, font, &name, offset(cursor, [12.0, 12.0]), if item.identified { [255; 3] } else { UNIDENTIFIED });
            }
        }

        // Footer: the "show my equipment" box and the two buttons, on the player's own window.
        // ponytail: someone else's window shows the footer empty; what the real client shows there isn't known.
        let footer_top = PANEL_TOP + PANEL_HEIGHT;
        layer.sprites.push((&assets.footer, at([0.0, footer_top]), assets.footer.size));
        if self.owner.is_some() {
            return;
        }
        let checkbox = &assets.checkbox[usize::from(self.shown)];
        layer.sprites.push((checkbox, at([6.0, footer_top + 8.0]), checkbox.size));
        layer.texts.push((message(SHOW_EQUIPMENT_MESSAGE).to_owned(), at([19.0, footer_top + 6.0]), [0, 0, 0]));
        let [swap, take_off] = &assets.buttons;
        layer.sprites.push((swap, at([WIDTH - 90.0, footer_top + 4.0]), swap.size));
        // ponytail: _a and _b read as normal and pointed at; the button's look isn't otherwise knowable.
        let pointed = contains(at(take_off_at()), TAKE_OFF, cursor);
        layer.sprites.push((if pointed { &assets.take_off_hover } else { take_off }, at(take_off_at()), TAKE_OFF));
        if pointed {
            window::tooltip(tooltip, chrome, font, message(TAKE_OFF_NAME), offset(cursor, [12.0, 12.0]), [255; 3]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_off_all_from_its_button() {
        let mut window = Equipment::default();
        window.frame.open = true;
        let [x, y] = window.frame.at(take_off_at());
        assert_eq!(window.hit([x + 20.0, y + 10.0], &[]), Some(Hit::TakeOffAll));
        assert!(window.button_boxes().iter().any(|(name, _)| name == "take_off_all"));
        // Someone else's window has no such button.
        let other = Equipment::of("Someone".into());
        assert_ne!(other.hit(other.frame.at(offset(take_off_at(), [20.0, 10.0])), &[]), Some(Hit::TakeOffAll));
    }
}
