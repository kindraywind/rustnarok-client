//! The equipment window (Alt+Q): what the player wears on the client's slot bitmap, with the character in the middle;
//! and the titles the player's achievements gave, to wear one.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use image::imageops;
use rustnarok_client::grf::Grf;
use rustnarok_client::lua::ItemInfo;
use rustnarok_client::net::InventoryItem;
use rustnarok_client::text::Font;

use crate::inventory::{self, UNIDENTIFIED};
use crate::render::{self, Gpu, Sprite, Texture};
use crate::window::{self, Chrome, Clicks, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI, contains, offset};

const WIDTH: f32 = 280.0;
/// The player's own window (equipwin_bg2.bmp's footer at the bottom, its last 7 rows cut), and another player's.
const HEIGHT: f32 = 187.0;
const OTHERS_HEIGHT: f32 = 167.0;
// ponytail: the swap view keeps its own layout and height; the game grows the window to 280×345 with the set's
// 280×179 panel at (0, 167) under the slots.
const SWAP_HEIGHT: f32 = 194.0;
/// The slot panel, under the tab strip; the Costume tab's from 3 higher.
const PANEL_TOP: f32 = 37.0;
const PANEL_HEIGHT: f32 = 130.0;
const OWN_PANEL_HEIGHT: u32 = 150;
const COSTUME_TOP: f32 = 34.0;
/// The body's edge (bottom corners cut) and the tabs' outline, also the line on from the strip at y 34.
const EDGE: [u8; 4] = [0xC8, 0xC8, 0xC8, 0xFF];
const TAB_LINE: [u8; 4] = [0xC3, 0xC3, 0xC3, 0xFF];
/// The tab strip at (0, 17): tabs 17 tall, each its label's width + 15.
const TAB_HEIGHT: f32 = 17.0;
const TAB_PADDING: f32 = 15.0;
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
/// Where it first opens.
const DEFAULT_POSITION: [f32; 2] = [185.0, 300.0];
/// General, Costume (costumes and shadow gear) and Title (the player's own window only), and their tooltips.
const TAB_LABELS: [usize; 3] = [1805, 2419, 2685];
const TAB_TOOLTIPS: [usize; 3] = [3158, 3159, 3160];
/// Tab widths until the labels are measured (`set_tab_labels`).
const TAB_WIDTHS: [f32; 3] = [60.0, 62.0, 42.0];
const TITLES_TAB: usize = 2;
/// "Disable Title", first in the titles' list, and the list's rows.
const NO_TITLE: usize = 2686;
const TITLE_ROW: f32 = 18.0;
/// The Title tab: white over the panel, a #646464 frame round the list, the worn title centred at y 40, the list.
const TITLES_WHITE: [f32; 4] = [1.0, 35.0, WIDTH - 2.0, 131.0];
const TITLES_FRAME: [f32; 4] = [2.0, 58.0, WIDTH - 4.0, 107.0];
const TITLES_FRAME_COLOR: [u8; 4] = [0x64, 0x64, 0x64, 0xFF];
const WORN_TITLE_TOP: f32 = 40.0;
const TITLES_LIST: [f32; 4] = [3.0, 59.0, WIDTH - 6.0, 105.0];
/// Take Off All, in the footer's right corner, and its name.
const TAKE_OFF: [f32; 2] = [40.0, 20.0];
const TAKE_OFF_NAME: usize = 3955;
const SHOW_EQUIPMENT_MESSAGE: usize = 1362;
/// "%s's Equipment": another player's window.
const OTHERS_TITLE: usize = 1361;
const TITLE_MESSAGE: usize = 104;
/// The Swap Equipment view: its title, and the switch button's label and size.
const SWAP_TITLE: usize = 2986;
const CHANGE_LABEL: usize = 3111;
const CHANGE: [f32; 2] = [118.0, 20.0];
/// "There is no item to replace.": the switch refused for an empty set.
pub const NOTHING_TO_SWITCH: usize = 3009;
/// ALL_EQSWITCH's cooldown (skill_db.yml): rathena drops a switch asked sooner.
const SWITCH_COOLDOWN: Duration = Duration::from_secs(10);
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
    /// The one-piece title bar.
    title: Texture,
    /// The player's own panel (equipwin_bg2.bmp, cut at the window's bottom), another player's, the Costume tab's.
    panels: [Texture; 3],
    edge: Texture,
    tab_line: Texture,
    titles_frame: Texture,
    /// The Show Equipment box: off, on.
    checkbox: [Texture; 2],
    /// Swap equipment and take everything off; Take Off All pointed at.
    buttons: [Texture; 2],
    take_off_hover: Texture,
    /// The Swap Equipment view: its general and special panels (with their own strip and footer), the swap button
    /// pointed at, and the switch button: normal, pointed at, greyed.
    swap_panels: [Texture; 2],
    swap_hover: Texture,
    change: [Texture; 3],
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let basic = format!("{UI}basic_interface\\");
        let image = |path: String| gpu.load_texture(grf, &path, false);
        let own = render::load_image(grf, &format!("{basic}equipwin_bg2.bmp"))
            .map(|image| imageops::crop_imm(&image, 0, 0, image.width(), OWN_PANEL_HEIGHT.min(image.height())).to_image());
        Self {
            title: image(format!("{basic}titlebar_fix.bmp")),
            panels: [
                own.map_or_else(|_| gpu.solid([0xF0; 4]), |own| gpu.texture(&own, false)),
                image(format!("{basic}equipwin_bg.bmp")),
                image(format!("{basic}equipwin_special.bmp")),
            ],
            edge: gpu.solid(EDGE),
            tab_line: gpu.solid(TAB_LINE),
            titles_frame: gpu.solid(TITLES_FRAME_COLOR),
            checkbox: ["0", "1"].map(|look| image(format!("{UI}checkbox_{look}.bmp"))),
            buttons: [image(format!("{basic}btn_e_change_a.bmp")), image(format!("{basic}btn_e_off_a.bmp"))],
            take_off_hover: image(format!("{basic}btn_e_off_b.bmp")),
            swap_panels: ["bg2", "special"].map(|panel| image(format!("{UI}swap_equipment\\equipwin_{panel}_change.bmp"))),
            swap_hover: image(format!("{basic}btn_e_change_b.bmp")),
            change: ["normal", "over", "disable"].map(|look| image(format!("{UI}swap_equipment\\btn_change2_{look}.bmp"))),
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
    /// The swap button: the Swap Equipment view and back.
    Swap,
    /// The view's switch button.
    SwitchAll,
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
    /// The Swap Equipment view: the slots show the set instead of what's worn.
    pub swapping: bool,
    /// When the last switch was asked.
    switched: Option<Instant>,
    /// Each tab's width: its label's + 15.
    tab_widths: [f32; 3],
}

impl Default for Equipment {
    fn default() -> Self {
        Self {
            frame: Frame::new(DEFAULT_POSITION, WIDTH).collapsing(),
            tab: 0,
            clicks: Clicks::default(),
            owner: None,
            shown: false,
            titles: Vec::new(),
            title: 0,
            swapping: false,
            switched: None,
            tab_widths: TAB_WIDTHS,
        }
    }
}

/// Take Off All's corner inside the window.
fn take_off_at() -> [f32; 2] {
    [WIDTH - 45.0, PANEL_TOP + PANEL_HEIGHT]
}

/// The swap button's corner, and the switch button's (in the view's footer).
// ponytail: the switch button's place isn't knowable; it takes the Show Equipment box's.
fn swap_at() -> [f32; 2] {
    [WIDTH - 90.0, PANEL_TOP + PANEL_HEIGHT]
}

/// The tabs' boxes in the window: side by side from (0, 17), each as wide as given.
fn tab_boxes(widths: &[f32]) -> Vec<[f32; 4]> {
    let mut x = 0.0;
    widths
        .iter()
        .map(|&width| {
            x += width;
            [x - width, TITLE_HEIGHT, width, TAB_HEIGHT]
        })
        .collect()
}

fn change_at() -> [f32; 2] {
    [6.0, PANEL_TOP + PANEL_HEIGHT + 4.0]
}

/// A slot's top-left corner and size inside the window.
fn slot_area(column: usize, row: usize) -> ([f32; 2], [f32; 2]) {
    let x = if column == 0 { 0.0 } else { WIDTH - COLUMN_WIDTH };
    ([x, PANEL_TOP + row as f32 * ROW], [COLUMN_WIDTH, ROW])
}

impl Equipment {
    /// Another player's equipment, open, at the game's place for the screen size.
    pub fn of(name: String, screen: [f32; 2]) -> Self {
        let mut frame = Frame::new(crate::escape::menu_origin(screen), WIDTH);
        frame.open = true;
        Self { frame, owner: Some(name), ..Self::default() }
    }

    /// Measures the tabs' labels: each tab is its label's width + 15.
    pub fn set_tab_labels(&mut self, messages: &[String], font: &Font) {
        self.tab_widths = TAB_LABELS.map(|id| font.width(messages.get(id).map_or("", String::as_str)) + TAB_PADDING);
    }

    pub fn height(&self) -> f32 {
        if self.frame.minimized {
            TITLE_HEIGHT
        } else if self.swapping {
            SWAP_HEIGHT
        } else if self.owner.is_some() {
            OTHERS_HEIGHT
        } else {
            HEIGHT
        }
    }

    /// Alt+Q and the basic info window's Equip button: open on General; open on another tab (or the swap view), back
    /// to General; open on General, closed.
    pub fn toggle_general(&mut self) {
        if self.frame.open && self.tab == 0 && !self.swapping {
            self.frame.open = false;
        } else {
            self.frame.open = true;
            self.tab = 0;
            self.swapping = false;
        }
    }

    /// The item in each slot of the current tab, with the slot's place: what's worn, or the swap set's.
    fn slots<'i>(&self, items: &'i [InventoryItem]) -> impl Iterator<Item = (usize, usize, &'i InventoryItem)> {
        let swapping = self.swapping;
        let place = move |item: &InventoryItem| if swapping { item.switch } else { item.worn };
        SLOTS
            .get(self.tab)
            .into_iter()
            .flatten()
            .filter_map(move |&(location, column, row)| Some((column, row, items.iter().find(|item| place(item) & location != 0)?)))
    }

    /// Whether a switch was asked within the cooldown.
    fn cooling(&self, now: Instant) -> bool {
        self.switched.is_some_and(|at| now.duration_since(at) < SWITCH_COOLDOWN)
    }

    /// The switch button pressed: true to send the switch (the cooldown starts), false within the cooldown, and
    /// NOTHING_TO_SWITCH for an empty set.
    pub fn switch_all(&mut self, items: &[InventoryItem], now: Instant) -> Result<bool, usize> {
        if items.iter().all(|item| item.switch == 0) {
            return Err(NOTHING_TO_SWITCH);
        }
        if self.cooling(now) {
            return Ok(false);
        }
        self.switched = Some(now);
        Ok(true)
    }

    pub fn hit(&self, cursor: [f32; 2], items: &[InventoryItem]) -> Option<Hit> {
        let local = self.frame.local(cursor, self.height())?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(Hit::Title(hit));
        }
        if let Some(tab) = self.tab_at(local) {
            return Some(Hit::Tab(tab));
        }
        // The titles: "none" first, then each one.
        let [x, y, width, height] = TITLES_LIST;
        if self.tab == TITLES_TAB && contains([x, y], [width, height], local) {
            let row = ((local[1] - y) / TITLE_ROW) as usize;
            let picked = std::iter::once(0).chain(self.titles.iter().map(|(id, _)| *id)).nth(row);
            return Some(picked.map_or(Hit::Window, Hit::WearTitle));
        }
        // ponytail: the box's label is taken to run 120px from the box.
        let footer_top = PANEL_TOP + PANEL_HEIGHT;
        if self.owner.is_none() && contains(swap_at(), TAKE_OFF, local) {
            return Some(Hit::Swap);
        }
        if self.swapping && contains(change_at(), CHANGE, local) {
            return Some(Hit::SwitchAll);
        }
        if self.owner.is_none() && !self.swapping && contains([6.0, footer_top + 6.0], [120.0, 14.0], local) {
            return Some(Hit::ShowBox);
        }
        if self.owner.is_none() && !self.swapping && contains(take_off_at(), TAKE_OFF, local) {
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

    /// Reacts to a click; returns the item double-clicked: to take off, or in the swap view to take out of the set.
    pub fn click(&mut self, hit: Hit) -> Option<u16> {
        match hit {
            Hit::Title(hit) => self.frame.click_title(hit),
            Hit::Tab(tab) => self.tab = tab,
            // Someone else's items stay on.
            Hit::Slot(index) if self.owner.is_none() => return self.clicks.double(index).then_some(index),
            // The set has no titles.
            Hit::Swap => {
                self.swapping = !self.swapping;
                self.tab = self.tab.min(self.tabs() - 1);
            }
            Hit::Slot(_) | Hit::ShowBox | Hit::WearTitle(_) | Hit::TakeOffAll | Hit::SwitchAll | Hit::Window => {}
        }
        None
    }

    /// The special tab (costumes and shadow gear), not the swap view.
    pub fn showing_costume(&self) -> bool {
        self.tab == 1 && !self.swapping
    }

    /// Opens the special tab.
    pub fn show_costume(&mut self) {
        self.swapping = false;
        self.tab = 1;
    }

    /// How many tabs show: the titles only on the player's own window, out of the swap view.
    fn tabs(&self) -> usize {
        if self.owner.is_none() && !self.swapping { TAB_LABELS.len() } else { TITLES_TAB }
    }

    /// The tab under a point in the window.
    fn tab_at(&self, local: [f32; 2]) -> Option<usize> {
        tab_boxes(&self.tab_widths[..self.tabs()]).into_iter().position(|[x, y, width, height]| contains([x, y], [width, height], local))
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
        let tabs = tab_boxes(&self.tab_widths[..self.tabs()]).into_iter().enumerate();
        let mut boxes: Vec<_> = tabs.map(|(tab, [x, y, width, height])| (format!("equipment_tab:{tab}"), place([x, y], [width, height]))).collect();
        if self.owner.is_none() {
            boxes.push(("swap".to_owned(), place(swap_at(), TAKE_OFF)));
        }
        if self.owner.is_none() && !self.swapping {
            boxes.push(("take_off_all".to_owned(), place(take_off_at(), TAKE_OFF)));
        }
        if self.swapping {
            boxes.push(("switch_all".to_owned(), place(change_at(), CHANGE)));
        }
        if self.tab == TITLES_TAB {
            let ids = std::iter::once(0).chain(self.titles.iter().map(|(id, _)| *id));
            let [x, y, width, height] = TITLES_LIST;
            let rows = (height / TITLE_ROW) as usize;
            boxes.extend(ids.take(rows).enumerate().map(|(row, id)| (format!("title:{id}"), place([x, y + row as f32 * TITLE_ROW], [width, TITLE_ROW]))));
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
        names: (&inventory::CardNames, &HashMap<u32, Option<String>>),
    ) {
        if !self.frame.open {
            return;
        }
        let at = |local: [f32; 2]| self.frame.at(local);
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        let title = match &self.owner {
            Some(name) => rustnarok_client::messages::fill(message(OTHERS_TITLE), &[name]),
            None if self.swapping => message(SWAP_TITLE).to_owned(),
            None => message(TITLE_MESSAGE).to_owned(),
        };
        self.frame.draw_fixed_title(layer, chrome, &assets.title, &title, cursor);
        if self.frame.minimized {
            return;
        }

        // The body: a #C8C8C8 edge with its bottom corners cut, white inside.
        let height = self.height();
        layer.sprites.push((&assets.edge, at([0.0, TITLE_HEIGHT]), [WIDTH, height - TITLE_HEIGHT - 1.0]));
        layer.sprites.push((&assets.edge, at([1.0, height - 1.0]), [WIDTH - 2.0, 1.0]));
        layer.sprites.push((&chrome.white, at([1.0, TITLE_HEIGHT]), [WIDTH - 2.0, height - 19.0]));
        // The panel and the character standing on its shadow: the player's own (its footer at the bottom), another
        // player's, and the Costume tab's over it. The swap view's pink panel comes with its own strip and footer.
        // ponytail: the view takes the window's place; whether the real one opens beside it isn't knowable.
        let swap_panel = assets.swap_panels.get(self.tab).filter(|_| self.swapping);
        if let Some(panel) = swap_panel {
            layer.sprites.push((panel, at([0.0, TITLE_HEIGHT]), panel.size));
        } else {
            let panel = &assets.panels[usize::from(self.owner.is_some())];
            layer.sprites.push((panel, at([0.0, PANEL_TOP]), panel.size));
            if self.tab == 1 {
                layer.sprites.push((&assets.panels[2], at([0.0, COSTUME_TOP]), assets.panels[2].size));
            }
        }
        // The tabs: white with a #C3C3C3 outline, the label centred; a line on from them to the right edge.
        // ponytail: the open tab is told by its black label and no line under it, so it joins the panel.
        let boxes = tab_boxes(&self.tab_widths[..self.tabs()]);
        for (tab, &[x, y, width, height]) in boxes.iter().enumerate() {
            let open = tab == self.tab;
            layer.sprites.push((&assets.tab_line, at([x, y]), [width, height + 1.0]));
            layer.sprites.push((&chrome.white, at([x + 1.0, y + 1.0]), [width - 2.0, height - if open { 0.0 } else { 1.0 }]));
            let label = window::fit(font, message(TAB_LABELS[tab]), width - 4.0);
            let left = x + ((width - font.width(&label)) / 2.0).floor();
            layer.texts.push((label, at([left, y + 2.0]), if open { [0, 0, 0] } else { GREY }));
            if contains(at([x, y]), [width, height], cursor) {
                window::tooltip(tooltip, chrome, font, message(TAB_TOOLTIPS[tab]), offset(cursor, [12.0, 12.0]), [255; 3]);
            }
        }
        let end = boxes.last().map_or(0.0, |[x, _, width, _]| x + width);
        layer.sprites.push((&assets.tab_line, at([end, TITLE_HEIGHT + TAB_HEIGHT]), [WIDTH - end, 1.0]));
        layer.sprites.extend(preview);
        // The titles: white over the panel, the worn one's name centred on top, and the list in its frame: none, then
        // each earned one; the worn one black, the others grey.
        if self.tab == TITLES_TAB {
            let [x, y, width, height] = TITLES_WHITE;
            layer.sprites.push((&chrome.white, at([x, y]), [width, height]));
            let [x, y, width, height] = TITLES_FRAME;
            for (corner, size) in
                [([x, y], [width, 1.0]), ([x, y + height - 1.0], [width, 1.0]), ([x, y], [1.0, height]), ([x + width - 1.0, y], [1.0, height])]
            {
                layer.sprites.push((&assets.titles_frame, at(corner), size));
            }
            if let Some((_, worn)) = self.titles.iter().find(|(id, _)| *id == self.title) {
                let worn = window::fit(font, worn, WIDTH - 2.0);
                layer.texts.push((worn.clone(), at([1.0 + ((WIDTH - 2.0 - font.width(&worn)) / 2.0).floor(), WORN_TITLE_TOP]), [0, 0, 0]));
            }
            let [x, y, width, height] = TITLES_LIST;
            let rows = std::iter::once((0, message(NO_TITLE).to_owned())).chain(self.titles.iter().cloned());
            for (row, (id, name)) in rows.enumerate().take((height / TITLE_ROW) as usize) {
                let color = if id == self.title { [0, 0, 0] } else { GREY };
                layer.texts.push((window::fit(font, &name, width - 10.0), at([x + 5.0, y + 2.0 + row as f32 * TITLE_ROW]), color));
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
            let name = inventory::full_name(items, names.0, messages, names.1, item, false);
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
        let footer_top = PANEL_TOP + PANEL_HEIGHT;
        if self.owner.is_some() {
            return;
        }
        let [swap, take_off] = &assets.buttons;
        let swap = if contains(at(swap_at()), TAKE_OFF, cursor) { &assets.swap_hover } else { swap };
        layer.sprites.push((swap, at(swap_at()), swap.size));
        // The swap view: the switch button, greyed for the cooldown.
        if self.swapping {
            let [normal, over, greyed] = &assets.change;
            let pointed = contains(at(change_at()), CHANGE, cursor);
            let button = if self.cooling(crate::control::now()) {
                greyed
            } else if pointed {
                over
            } else {
                normal
            };
            layer.sprites.push((button, at(change_at()), CHANGE));
            let label = message(CHANGE_LABEL);
            let x = change_at()[0] + (CHANGE[0] - font.width(label)) / 2.0;
            layer.texts.push((label.to_owned(), at([x, change_at()[1] + 3.0]), [0, 0, 0]));
            return;
        }
        let checkbox = &assets.checkbox[usize::from(self.shown)];
        layer.sprites.push((checkbox, at([6.0, footer_top + 8.0]), checkbox.size));
        layer.texts.push((message(SHOW_EQUIPMENT_MESSAGE).to_owned(), at([19.0, footer_top + 6.0]), [0, 0, 0]));
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
        let other = Equipment::of("Someone".into(), [1024.0, 768.0]);
        assert_ne!(other.hit(other.frame.at(offset(take_off_at(), [20.0, 10.0])), &[]), Some(Hit::TakeOffAll));
    }

    #[test]
    fn opens_at_the_games_size_and_place() {
        let mut window = Equipment::default();
        assert_eq!(window.frame.position, [185.0, 300.0]);
        assert_eq!(window.height(), 187.0);
        window.frame.minimized = true;
        assert_eq!(window.height(), 17.0);
        let other = Equipment::of("Someone".into(), [1024.0, 768.0]);
        assert_eq!((other.frame.position, other.height()), ([377.0, 480.0], 167.0));
        assert_eq!(Equipment::of("Someone".into(), [800.0, 600.0]).frame.position, [265.0, 375.0]);
        // Another player's window has General and Costume only.
        assert_eq!(other.tabs(), 2);
    }

    #[test]
    fn lays_tabs_side_by_side_from_their_labels() {
        assert_eq!(tab_boxes(&[60.0, 62.0, 42.0]), [[0.0, 17.0, 60.0, 17.0], [60.0, 17.0, 62.0, 17.0], [122.0, 17.0, 42.0, 17.0]]);
        let mut window = Equipment::default();
        window.frame.open = true;
        window.tab_widths = [50.0, 70.0, 40.0];
        let [x, y] = window.frame.position;
        assert_eq!(window.hit([x + 55.0, y + 20.0], &[]), Some(Hit::Tab(1)));
        assert_eq!(window.hit([x + 165.0, y + 20.0], &[]), Some(Hit::Window));
        assert_eq!(window.button_boxes()[2], ("equipment_tab:2".to_owned(), [x + 120.0, y + 17.0, 40.0, 17.0]));
        // The titles' list rows from (3, 59), 18 apart.
        window.tab = TITLES_TAB;
        window.titles = vec![(1000, "First".into())];
        assert_eq!(window.hit([x + 10.0, y + 60.0], &[]), Some(Hit::WearTitle(0)));
        assert_eq!(window.hit([x + 10.0, y + 78.0], &[]), Some(Hit::WearTitle(1000)));
        assert_eq!(window.hit([x + 10.0, y + 96.0], &[]), Some(Hit::Window));
    }

    #[test]
    fn alt_q_goes_back_to_general_before_closing() {
        let mut window = Equipment::default();
        window.toggle_general();
        assert!(window.frame.open && window.tab == 0);
        window.tab = TITLES_TAB;
        window.toggle_general();
        assert!(window.frame.open && window.tab == 0);
        window.toggle_general();
        assert!(!window.frame.open);
        // Closed on Costume: opens on General.
        window.tab = 1;
        window.toggle_general();
        assert!(window.frame.open && window.tab == 0);
        window.swapping = true;
        window.toggle_general();
        assert!(window.frame.open && !window.swapping);
    }

    #[test]
    fn swaps_the_set_in() {
        let knife = InventoryItem { index: 3, id: 1201, location: 2, worn: 2, ..Default::default() };
        let sword = InventoryItem { index: 4, id: 1101, location: 2, switch: 2, ..Default::default() };
        let mut window = Equipment { tab: TITLES_TAB, ..Equipment::default() };
        window.frame.open = true;
        let items = [knife.clone(), sword.clone()];
        let [x, y] = window.frame.at(offset(swap_at(), [20.0, 10.0]));
        assert_eq!(window.hit([x, y], &items), Some(Hit::Swap));
        window.click(Hit::Swap);
        // The swap view shows the set; it has no titles tab.
        assert!(window.swapping);
        assert_eq!(window.tab, 1);
        window.tab = 0;
        let shown: Vec<u16> = window.slots(&items).map(|(.., item)| item.index).collect();
        assert_eq!(shown, [4]);
        assert!(window.button_boxes().iter().any(|(name, _)| name == "switch_all"));
        assert!(!window.button_boxes().iter().any(|(name, _)| name == "take_off_all"));
        let [x, y] = window.frame.at(offset(change_at(), [50.0, 10.0]));
        assert_eq!(window.hit([x, y], &items), Some(Hit::SwitchAll));
        // An empty set is refused; a switch blocks the next for 10 seconds.
        let now = Instant::now();
        assert_eq!(window.switch_all(&[knife], now), Err(NOTHING_TO_SWITCH));
        assert_eq!(window.switch_all(&items, now), Ok(true));
        assert_eq!(window.switch_all(&items, now + Duration::from_secs(9)), Ok(false));
        assert_eq!(window.switch_all(&items, now + Duration::from_secs(10)), Ok(true));
        // Back to what's worn.
        window.click(Hit::Swap);
        assert_eq!(window.slots(&items).map(|(.., item)| item.index).collect::<Vec<_>>(), [3]);
    }
}
