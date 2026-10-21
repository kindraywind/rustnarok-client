//! The equipment window (Alt+Q): what the player wears on the client's slot bitmap, with the character in the middle;
//! and the titles the player's achievements gave, to wear one.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use image::imageops;
use rustnarok_client::grf::Grf;
use rustnarok_client::lua::ItemInfo;
use rustnarok_client::net::InventoryItem;
use rustnarok_client::text::Font;

use crate::actor::Look;
use crate::inventory;
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
/// A slot's icon at x 6 or 251, top 38 + 26r; its name box 70 wide at x 33 or 172, two lines at most, centred on the row's
/// middle 51 + 26r (one line's top is 45 + 26r, also the drag oval's).
const ICON_X: [f32; 2] = [6.0, 251.0];
const ICON_TOP: f32 = 38.0;
const NAME_X: [f32; 2] = [33.0, 172.0];
const NAME_TOP: f32 = 45.0;
const NAME_MID: f32 = 51.0;
const NAME_BOX: f32 = 70.0;
/// The slot rows: from y 37, 26 apart, above y 165. A press takes the icon only, a hover or a right press the icon
/// and the name: x from and to, left and right.
const ROWS_TOP: f32 = 37.0;
const ROWS_END: f32 = 165.0;
const PRESS_X: [[f32; 2]; 2] = [[6.0, 30.0], [251.0, 275.0]];
const HOVER_X: [[f32; 2]; 2] = [[6.0, 106.0], [170.0, 274.0]];
/// The ammo (General, the player's own window): its icon at (115, 64), taken on 115 < x < 139, 64 < y < 88, and its
/// name under it, 40 wide.
const AMMO: u32 = 0x8000;
const AMMO_ICON: [f32; 2] = [115.0, 64.0];
const AMMO_NAME: [f32; 2] = [115.0, 88.0];
const AMMO_NAME_WIDTH: f32 = 40.0;
/// Where a dragged item may go: an oval (28×15) at x 4 or 248, top 45 + 26r, for each slot its location names.
const OVAL_X: [f32; 2] = [4.0, 248.0];
/// The grade's mark (12×12) under the icon's top.
const GRADE_AT: [f32; 2] = [0.0, 12.0];
/// One enchant's name colour here; more take the inventory's tiers.
const ONE_ENCHANT: [u8; 3] = [0x66, 0x99, 0x00];
/// A stack's tooltip, "%s: %d ea." (the ammo).
const STACK_MESSAGE: usize = 183;
/// A name's lines, this far apart.
const NAME_LINE: f32 = 12.0;
/// Where the character stands: the shadow baked into equipwin_bg.bmp.
const PREVIEW_FEET: [f32; 2] = [141.0, 149.0];
/// The slots the preview dresses from, by tab (top, mid and lower headgear, garment): General, then Costume.
const PREVIEW_SLOTS: [[u32; 4]; 2] = [[0x100, 0x200, 0x001, 0x004], [0x400, 0x800, 0x1000, 0x2000]];
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
/// The footer on the player's own window, none of it on the Title tab: the Show Equipment box on General and the View
/// Costume box in its place on Costume (80×12 click areas), Swap and Unequip All on both, and their names.
const BOX: [f32; 2] = [10.0, 172.0];
const BOX_AREA: [f32; 2] = [80.0, 12.0];
const BOX_LABEL: [f32; 2] = [23.0, 170.0];
const SWAP: [f32; 2] = [193.0, 167.0];
const UNEQUIP: [f32; 2] = [235.0, 167.0];
const TAKE_OFF: [f32; 2] = [40.0, 20.0];
const TAKE_OFF_NAME: usize = 3955;
const SHOW_EQUIPMENT_MESSAGE: usize = 1362;
const VIEW_COSTUME_MESSAGE: usize = 4103;
// ponytail: older message tables end before 4103; the box keeps a label.
const VIEW_COSTUME_FALLBACK: &str = "View Costume";
/// What Unequip All takes off: General's gear, or Costume's costume and shadow pieces.
/// rathena's equip_pos bits. General is head, garment, accessories, armor, both hands and shoes. Costume is the
/// four costume slots and the six shadow slots. Ammunition stays on.
pub(crate) const GENERAL_PIECES: u32 = 0x3FF;
pub(crate) const COSTUME_PIECES: u32 = 0x3C00 | 0x3F_0000;
/// The middle column on General: the dismount (or cart off) button and Cart Items, with their names.
const DISMOUNT: [f32; 2] = [122.0, 127.0];
const DISMOUNT_SIZE: [f32; 2] = [36.0, 36.0];
const CART_ITEMS: [f32; 2] = [126.0, 107.0];
const CART_ITEMS_SIZE: [f32; 2] = [30.0, 20.0];
const CART_ITEMS_MESSAGE: usize = 237;
const CART_OFF_MESSAGE: usize = 223;
const DISMOUNT_BOTH_MESSAGE: usize = 3636;
/// CZ_UNINSTALLATION's kinds.
pub const UNINSTALL_DRAGON: u8 = 1;
pub const UNINSTALL_MADOGEAR: u8 = 3;
pub const UNINSTALL_MOUNT: u8 = 4;
pub const UNINSTALL_FALCON: u8 = 5;
pub const UNINSTALL_CART: u8 = 6;
/// rathena's option bits for what can be taken off, and the gryphon riders' jobs.
const OPTION_FALCON: u32 = 0x10;
const OPTION_RIDING: u32 = 0x20;
const OPTION_DRAGON: u32 = 0x8_0000 | 0x80_0000 | 0x100_0000 | 0x200_0000 | 0x400_0000;
const OPTION_MADOGEAR: u32 = 0x40_0000;
const GRYPHON_JOBS: [u16; 4] = [4082, 4083, 4110, 4281];
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
    /// Swap equipment and take everything off; Take Off All pointed at and greyed.
    buttons: [Texture; 2],
    take_off_hover: Texture,
    take_off_greyed: Texture,
    /// The dismount button, and Cart Items: off, on.
    dismount: Texture,
    cart_items: [Texture; 2],
    /// The Swap Equipment view: its general and special panels (with their own strip and footer), the swap button
    /// pointed at, and the switch button: normal, pointed at, greyed.
    swap_panels: [Texture; 2],
    swap_hover: Texture,
    change: [Texture; 3],
    /// The drop oval, and the grades' marks (1 to 4).
    oval: Texture,
    grades: Vec<Option<Texture>>,
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
            take_off_greyed: image(format!("{basic}btn_e_off_c.bmp")),
            dismount: image(format!("{basic}btn_off.bmp")),
            cart_items: ["off", "on"].map(|look| image(format!("{basic}btn_items_{look}.bmp"))),
            swap_panels: ["bg2", "special"].map(|panel| image(format!("{UI}swap_equipment\\equipwin_{panel}_change.bmp"))),
            swap_hover: image(format!("{basic}btn_e_change_b.bmp")),
            change: ["normal", "over", "disable"].map(|look| image(format!("{UI}swap_equipment\\btn_change2_{look}.bmp"))),
            oval: image(format!("{basic}item_invert.bmp")),
            grades: (1..=4).map(|grade| gpu.try_load_texture(grf, &format!("{UI}grade_enchant\\grade_icon{grade}.bmp"), false).ok()).collect(),
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Hit {
    Title(TitleHit),
    Tab(usize),
    /// A worn item's icon (or the ammo), by inventory index: presses, drags, double clicks.
    Slot(u16),
    /// A worn item's name: hovers and right presses only.
    Name(u16),
    /// The Show Equipment box and its label.
    ShowBox,
    /// The View Costume box and its label.
    ViewCostume,
    /// The dismount (or cart off) button.
    Dismount,
    /// Cart Items: the cart window.
    CartItems,
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
    /// Costumes show on the player's sprite (the View Costume box), as the server last said.
    // ponytail: this server's version sends no state at login, so it starts ticked.
    pub costumes_shown: bool,
    /// What was worn when Unequip All was last pressed: it stays greyed until that changes.
    unequipped: Option<Vec<(u16, u32)>>,
    /// The player's option bits, job and cart, for the dismount and Cart Items buttons.
    pub option: u32,
    pub job: u16,
    pub cart: bool,
}

/// Unequip All: General's request, or Costume's pieces one by one.
#[derive(Debug, PartialEq)]
pub enum UnequipAll {
    General,
    /// Costume (and shadow) pieces, one CZ_REQ_TAKEOFF_EQUIP each. PACKETVER 20230906 sends COSTUME_PIECES instead.
    Pieces(Vec<u16>),
}

/// The mount or falcon to take off, by option bits: its CZ_UNINSTALLATION kind and its line.
fn mount(option: u32, job: u16) -> Option<(u8, usize)> {
    if option & OPTION_DRAGON != 0 {
        Some((UNINSTALL_DRAGON, 1463))
    } else if option & OPTION_MADOGEAR != 0 {
        Some((UNINSTALL_MADOGEAR, 1464))
    } else if option & OPTION_RIDING != 0 {
        Some((UNINSTALL_MOUNT, if GRYPHON_JOBS.contains(&job) { 2096 } else { 442 }))
    } else if option & OPTION_FALCON != 0 {
        Some((UNINSTALL_FALCON, 441))
    } else {
        None
    }
}

/// What is worn, piece by piece.
fn worn_now(items: &[InventoryItem]) -> Vec<(u16, u32)> {
    items.iter().filter(|item| item.worn != 0).map(|item| (item.index, item.worn)).collect()
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
            costumes_shown: true,
            unequipped: None,
            option: 0,
            job: 0,
            cart: false,
        }
    }
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

/// The switch button's corner (in the swap view's footer).
// ponytail: the switch button's place isn't knowable; it takes the Show Equipment box's.
fn change_at() -> [f32; 2] {
    [6.0, PANEL_TOP + PANEL_HEIGHT + 4.0]
}

/// The slot row under a window-local y.
fn slot_row(y: f32) -> Option<usize> {
    (ROWS_TOP..ROWS_END).contains(&y).then(|| ((y - ROWS_TOP) / ROW) as usize)
}

/// Whether a window-local point is on the ammo.
fn on_ammo([x, y]: [f32; 2]) -> bool {
    x > AMMO_ICON[0] && x < AMMO_ICON[0] + ICON && y > AMMO_ICON[1] && y < AMMO_ICON[1] + ICON
}

/// A slot's tooltip corner for the window-local pointer: x 6 left, 175 right, 120 between (the ammo); y one 26 px step
/// per 24 px from y 37, plus 17 (so low in a row it lands a step lower).
fn tooltip_at([x, y]: [f32; 2]) -> [f32; 2] {
    let left = if x < 106.0 {
        6.0
    } else if x >= 170.0 {
        175.0
    } else {
        WIDTH / 2.0 - 20.0
    };
    [left, ROW * ((y - ROWS_TOP) / 24.0).floor() + 17.0]
}

/// A worn item's name colour in its box: black, or by its enchants (#669900 for one, then the inventory's tiers).
pub fn slot_name_color(item: &InventoryItem, items: &HashMap<u32, ItemInfo>) -> [u8; 3] {
    match inventory::enchants(item, items) {
        0 => [0, 0, 0],
        1 => ONE_ENCHANT,
        count => inventory::ENCHANTED[count.min(4) - 1],
    }
}

/// A worn name in its box: the first line breaks at any character, the second is cut with "..." when the rest doesn't fit.
fn name_lines(name: &str, width: f32, measure: impl Fn(&str) -> f32) -> Vec<String> {
    let Some(cut) = name.char_indices().skip(1).find(|&(at, character)| measure(&name[..at + character.len_utf8()]) > width).map(|(at, _)| at) else {
        return vec![name.to_owned()];
    };
    let rest = name[cut..].trim_start();
    let mut second = rest.to_owned();
    if measure(rest) > width {
        while !second.is_empty() && measure(&format!("{second}...")) > width {
            second.pop();
        }
        second.push_str("...");
    }
    vec![name[..cut].to_owned(), second]
}

/// The General slots a dragged item's location (bits 0–9) names: column and row, up to three for a headgear.
fn drop_slots(location: u32) -> Vec<(usize, usize)> {
    SLOTS[0].iter().filter(|(bit, ..)| location & 0x3FF & bit != 0).map(|&(_, column, row)| (column, row)).collect()
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
        let footer = self.owner.is_none() && self.tab != TITLES_TAB;
        if footer && contains(SWAP, TAKE_OFF, local) {
            return Some(Hit::Swap);
        }
        if self.swapping && contains(change_at(), CHANGE, local) {
            return Some(Hit::SwitchAll);
        }
        let own = footer && !self.swapping;
        if own && contains(BOX, BOX_AREA, local) {
            return Some(if self.tab == 0 { Hit::ShowBox } else { Hit::ViewCostume });
        }
        if own && contains(UNEQUIP, TAKE_OFF, local) {
            return Some(Hit::TakeOffAll);
        }
        if own && self.tab == 0 && !self.dismounts().is_empty() && contains(DISMOUNT, DISMOUNT_SIZE, local) {
            return Some(Hit::Dismount);
        }
        if own && self.tab == 0 && self.cart && contains(CART_ITEMS, CART_ITEMS_SIZE, local) {
            return Some(Hit::CartItems);
        }
        if let Some(ammo) = self.ammo(items).filter(|_| on_ammo(local)) {
            return Some(Hit::Slot(ammo.index));
        }
        let within = |[from, to]: [f32; 2]| (from..to).contains(&local[0]);
        let slot = slot_row(local[1]).and_then(|row| {
            let column = (0..2).find(|&column| within(HOVER_X[column]))?;
            let item = self.slots(items).find(|&(each, at, _)| each == column && at == row)?.2;
            Some(if within(PRESS_X[column]) { Hit::Slot(item.index) } else { Hit::Name(item.index) })
        });
        Some(slot.unwrap_or(Hit::Window))
    }

    /// The worn ammo, shown on General in the player's own window.
    fn ammo<'i>(&self, items: &'i [InventoryItem]) -> Option<&'i InventoryItem> {
        (self.tab == 0 && self.owner.is_none() && !self.swapping).then(|| items.iter().find(|item| item.worn & AMMO != 0)).flatten()
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
            Hit::Slot(_)
            | Hit::Name(_)
            | Hit::ShowBox
            | Hit::ViewCostume
            | Hit::Dismount
            | Hit::CartItems
            | Hit::WearTitle(_)
            | Hit::TakeOffAll
            | Hit::SwitchAll
            | Hit::Window => {}
        }
        None
    }

    /// What Unequip All takes off on the open tab: the worn pieces' indices.
    fn unequip_targets(&self, items: &[InventoryItem]) -> Vec<u16> {
        let pieces = if self.tab == 1 { COSTUME_PIECES } else { GENERAL_PIECES };
        items.iter().filter(|item| item.worn & pieces != 0).map(|item| item.index).collect()
    }

    /// Unequip All works while something on the tab is worn, and not again until what is worn changes.
    fn unequip_enabled(&self, items: &[InventoryItem]) -> bool {
        !self.unequip_targets(items).is_empty() && self.unequipped.as_ref() != Some(&worn_now(items))
    }

    /// Unequip All pressed: what to send, if it's enabled; then it greys.
    pub fn unequip_all(&mut self, items: &[InventoryItem]) -> Option<UnequipAll> {
        if !self.unequip_enabled(items) {
            return None;
        }
        self.unequipped = Some(worn_now(items));
        Some(if self.tab == 1 { UnequipAll::Pieces(self.unequip_targets(items)) } else { UnequipAll::General })
    }

    /// What the dismount button takes off, each with its line: the cart, then the mount or falcon. Empty: no button.
    pub fn dismounts(&self) -> Vec<(u8, usize)> {
        let mut found: Vec<_> = self.cart.then_some((UNINSTALL_CART, CART_OFF_MESSAGE)).into_iter().collect();
        found.extend(mount(self.option, self.job));
        found
    }

    /// The dismount button's name: the one thing it takes off, or "cart and mount" for both.
    fn dismount_name(&self) -> Option<usize> {
        match self.dismounts()[..] {
            [] => None,
            [(_, message)] => Some(message),
            _ => Some(DISMOUNT_BOTH_MESSAGE),
        }
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
        let footer = self.owner.is_none() && self.tab != TITLES_TAB;
        if footer {
            boxes.push(("swap".to_owned(), place(SWAP, TAKE_OFF)));
        }
        if footer && !self.swapping {
            boxes.push(("take_off_all".to_owned(), place(UNEQUIP, TAKE_OFF)));
            boxes.push((if self.tab == 0 { "show_box" } else { "view_costume" }.to_owned(), place(BOX, BOX_AREA)));
        }
        if footer && !self.swapping && self.tab == 0 && !self.dismounts().is_empty() {
            boxes.push(("dismount".to_owned(), place(DISMOUNT, DISMOUNT_SIZE)));
        }
        if footer && !self.swapping && self.tab == 0 && self.cart {
            boxes.push(("cart_items".to_owned(), place(CART_ITEMS, CART_ITEMS_SIZE)));
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
        (self.frame.open && !self.frame.minimized && self.tab != TITLES_TAB).then(|| self.frame.at(PREVIEW_FEET))
    }

    /// The preview's look on this tab (`preview_look`).
    pub fn preview_look(&self, base: &Look, worn: &[(u32, u16)]) -> Look {
        preview_look(base, self.tab == 1, worn)
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
        dropping: Option<u32>,
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

        // While an inventory item is dragged over General: an oval on each slot it would go to, under the icons.
        if let Some(location) = dropping.filter(|_| self.tab == 0 && contains(at([0.0, 0.0]), [WIDTH, self.height()], cursor)) {
            for (column, row) in drop_slots(location) {
                layer.sprites.push((&assets.oval, at([OVAL_X[column], NAME_TOP + row as f32 * ROW]), assets.oval.size));
            }
        }
        let hovered = self.frame.local(cursor, self.height()).filter(|_| self.frame.open);
        let pointed = match hovered.and_then(|_| self.hit(cursor, worn)) {
            Some(Hit::Slot(index) | Hit::Name(index)) => Some(index),
            _ => None,
        };
        let icon = |layer: &mut Layer<'a>, item: &InventoryItem, corner: [f32; 2]| {
            if let Some(Some(icon)) = items.get(&item.id).and_then(|info| icons.get(&inventory::icon_path(info, item.identified))) {
                layer.sprites.push((icon, at(corner), icon.size));
            }
            if let Some(Some(mark)) = usize::from(item.grade).checked_sub(1).and_then(|grade| assets.grades.get(grade)) {
                layer.sprites.push((mark, at(offset(corner, GRADE_AT)), mark.size));
            }
        };
        let shown = self.slots(worn).map(|(column, row, item)| (Some((column, row)), item)).chain(self.ammo(worn).map(|item| (None, item)));
        for (place, item) in shown {
            let name = inventory::full_name(items, names.0, messages, names.1, item, false);
            match place {
                // Left-aligned in both columns, as the game's.
                Some((column, row)) => {
                    icon(layer, item, [ICON_X[column], ICON_TOP + row as f32 * ROW]);
                    let color = slot_name_color(item, items);
                    let lines = name_lines(&name, NAME_BOX, |text| font.width(text));
                    let top = NAME_MID + row as f32 * ROW - lines.len() as f32 * NAME_LINE / 2.0;
                    for (number, line) in lines.into_iter().enumerate() {
                        layer.texts.push((line, at([NAME_X[column], top + number as f32 * NAME_LINE]), color));
                    }
                }
                None => {
                    icon(layer, item, AMMO_ICON);
                    layer.texts.push((window::fit(font, &name, AMMO_NAME_WIDTH), at(AMMO_NAME), slot_name_color(item, items)));
                }
            }
            // The tooltip at its fixed place: the name with its slots, or "name: n ea." for the ammo's stack.
            if pointed == Some(item.index)
                && let Some(local) = hovered
            {
                let text = if place.is_none() {
                    rustnarok_client::messages::fill(
                        message(STACK_MESSAGE),
                        &[&inventory::full_name(items, names.0, messages, names.1, item, false), &item.amount],
                    )
                } else {
                    inventory::full_name(items, names.0, messages, names.1, item, true)
                };
                window::tooltip(tooltip, chrome, font, &text, at(tooltip_at(local)), inventory::name_color(item, items));
            }
        }

        // The player's own window, off the Title tab: Swap, and in the swap view the switch button; else the box,
        // Unequip All and on General the middle buttons.
        if self.owner.is_some() || self.tab == TITLES_TAB {
            return;
        }
        let name = |tooltip: &mut Layer<'a>, place: [f32; 2], size: [f32; 2], id: usize| {
            if contains(at(place), size, cursor) {
                window::tooltip(tooltip, chrome, font, message(id), offset(cursor, [12.0, 12.0]), [255; 3]);
            }
        };
        let [swap, take_off] = &assets.buttons;
        let swap = if contains(at(SWAP), TAKE_OFF, cursor) { &assets.swap_hover } else { swap };
        layer.sprites.push((swap, at(SWAP), swap.size));
        name(tooltip, SWAP, TAKE_OFF, SWAP_TITLE);
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
        let (ticked, label) = if self.tab == 0 { (self.shown, SHOW_EQUIPMENT_MESSAGE) } else { (self.costumes_shown, VIEW_COSTUME_MESSAGE) };
        let checkbox = &assets.checkbox[usize::from(ticked)];
        layer.sprites.push((checkbox, at(BOX), checkbox.size));
        let label = match message(label) {
            "" if label == VIEW_COSTUME_MESSAGE => VIEW_COSTUME_FALLBACK,
            text => text,
        };
        layer.texts.push((label.to_owned(), at(BOX_LABEL), [0, 0, 0]));
        // Unequip All: _a, _b pointed at, _c greyed.
        let button = if !self.unequip_enabled(worn) {
            &assets.take_off_greyed
        } else if contains(at(UNEQUIP), TAKE_OFF, cursor) {
            &assets.take_off_hover
        } else {
            take_off
        };
        layer.sprites.push((button, at(UNEQUIP), TAKE_OFF));
        name(tooltip, UNEQUIP, TAKE_OFF, TAKE_OFF_NAME);
        if self.tab != 0 {
            return;
        }
        if let Some(id) = self.dismount_name() {
            layer.sprites.push((&assets.dismount, at(DISMOUNT), DISMOUNT_SIZE));
            name(tooltip, DISMOUNT, DISMOUNT_SIZE, id);
        }
        // ponytail: Cart Items shows "on" while pointed at; when the game shows which isn't known.
        if self.cart {
            let button = &assets.cart_items[usize::from(contains(at(CART_ITEMS), CART_ITEMS_SIZE, cursor))];
            layer.sprites.push((button, at(CART_ITEMS), CART_ITEMS_SIZE));
            name(tooltip, CART_ITEMS, CART_ITEMS_SIZE, CART_ITEMS_MESSAGE);
        }
    }
}

/// The equipment window's figure: `base` (job, sex, hair and colours) dressed from the tab's headgear and garment, each
/// by its item's view in `worn` (where it's worn, view), with no weapon, shield or mount. A headgear filling several
/// slots shows once, in its highest; an empty costume slot shows nothing.
pub fn preview_look(base: &Look, costume: bool, worn: &[(u32, u16)]) -> Look {
    let [top, mid, low, robe] = PREVIEW_SLOTS[usize::from(costume)];
    let view = |slot: u32, above: u32| worn.iter().find(|&&(at, _)| at & slot != 0 && at & above == 0).map_or(0, |&(_, view)| view);
    Look {
        weapon: 0,
        shield: 0,
        option: 0,
        head_top: view(top, 0),
        head_mid: view(mid, top),
        head_bottom: view(low, top | mid),
        robe: view(robe, 0),
        ..base.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_worn_names_by_enchants() {
        let items: HashMap<u32, ItemInfo> = [2301, 4700].into_iter().map(|id| (id, ItemInfo { slots: 1, ..ItemInfo::default() })).collect();
        let shirt = |cards: [u32; 4]| InventoryItem { id: 2301, identified: true, cards, ..Default::default() };
        // The cards past its one slot: none black, one #669900, two #00BFFF, three and four the further tiers.
        assert_eq!(slot_name_color(&shirt([0; 4]), &items), [0, 0, 0]);
        assert_eq!(slot_name_color(&shirt([0, 4700, 0, 0]), &items), [0x66, 0x99, 0x00]);
        assert_eq!(slot_name_color(&shirt([0, 4700, 4700, 0]), &items), [0x00, 0xBF, 0xFF]);
        assert_eq!(slot_name_color(&shirt([4700, 4700, 4700, 4700]), &items), [0xFF, 0xA5, 0x00]);
        // The inventory's (the tooltip's) form says #ADFF2F for one; a maker's or a pet's item has none.
        assert_eq!(inventory::name_color(&shirt([0, 4700, 0, 0]), &items), [0xAD, 0xFF, 0x2F]);
        assert_eq!([0x00FF, 0x00FE, 0x0100].map(|mark| slot_name_color(&shirt([mark, 4700, 4700, 0]), &items)), [[0, 0, 0]; 3]);
    }

    #[test]
    fn breaks_worn_names_anywhere_and_cuts_the_second_line() {
        let width = |text: &str| text.chars().count() as f32 * 5.3;
        assert_eq!(name_lines("Fairy Lamp", 70.0, width), ["Fairy Lamp"]);
        assert_eq!(name_lines("+9 Khalitzburg Knight Armor", 70.0, width), ["+9 Khalitzbur", "g Knight A..."]);
        assert_eq!(name_lines("+9 Half Neutral Queen Mask Of Doom", 70.0, width), ["+9 Half Neutr", "al Queen M..."]);
        assert_eq!(name_lines("Resist Stun Monocle", 70.0, width), ["Resist Stun M", "onocle"]);
        assert_eq!(name_lines("+9 Half Neutral Queen Mask", 70.0, width), ["+9 Half Neutr", "al Queen Mask"]);
    }

    #[test]
    fn takes_presses_on_icons_and_hovers_on_names() {
        let mut window = Equipment::default();
        window.frame.open = true;
        let armor = InventoryItem { index: 7, id: 2301, location: 16, worn: 16, ..Default::default() };
        let arrows = InventoryItem { index: 8, id: 1750, location: AMMO, worn: AMMO, amount: 100, ..Default::default() };
        let items = [armor, arrows];
        let at = |x: f32, y: f32| window.frame.at([x, y]);
        // The armor: right column, row 1 (y 63–88). Its icon takes presses, its name only hovers.
        assert_eq!(window.hit(at(260.0, 70.0), &items), Some(Hit::Slot(7)));
        assert_eq!(window.hit(at(200.0, 70.0), &items), Some(Hit::Name(7)));
        assert_eq!(window.hit(at(160.0, 70.0), &items), Some(Hit::Window));
        // Rows end at y 165; the left column's empty slot is the window.
        assert_eq!(window.hit(at(20.0, 70.0), &items), Some(Hit::Window));
        // The ammo, strictly inside 115–139 × 64–88, on General in the player's own window only.
        assert_eq!(window.hit(at(120.0, 70.0), &items), Some(Hit::Slot(8)));
        assert_eq!(window.hit(at(115.0, 70.0), &items), Some(Hit::Window));
        window.tab = 1;
        assert_eq!(window.hit(at(120.0, 70.0), &items), Some(Hit::Window));
        // The tooltip: x 6, 175 or 120 by the pointer; y a 26 px step per 24 px, so low in the last row it's 147.
        assert_eq!([tooltip_at([50.0, 40.0]), tooltip_at([200.0, 70.0]), tooltip_at([120.0, 70.0])], [[6.0, 17.0], [175.0, 43.0], [120.0, 43.0]]);
        assert_eq!(tooltip_at([50.0, 161.0]), [6.0, 147.0]);
        assert_eq!(tooltip_at([50.0, 150.0]), [6.0, 121.0]);
        // The drop ovals: a top headgear one slot, a full headgear three, a weapon its own.
        assert_eq!(drop_slots(256), [(0, 0)]);
        assert_eq!(drop_slots(256 | 512 | 1), [(0, 0), (1, 0), (0, 1)]);
        assert_eq!(drop_slots(2 | AMMO), [(0, 2)]);
    }

    #[test]
    fn takes_off_all_from_its_button() {
        let mut window = Equipment::default();
        window.frame.open = true;
        let [x, y] = window.frame.at(UNEQUIP);
        assert_eq!(window.hit([x + 20.0, y + 10.0], &[]), Some(Hit::TakeOffAll));
        assert!(window.button_boxes().iter().any(|(name, _)| name == "take_off_all"));
        // Someone else's window has no such button.
        let other = Equipment::of("Someone".into(), [1024.0, 768.0]);
        assert_ne!(other.hit(other.frame.at(offset(UNEQUIP, [20.0, 10.0])), &[]), Some(Hit::TakeOffAll));
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
    fn shows_the_footer_and_middle_buttons_by_tab() {
        let names =
            |window: &Equipment| window.button_boxes().into_iter().map(|(name, _)| name).filter(|name| !name.starts_with("equipment_tab")).collect::<Vec<_>>();
        let mut window = Equipment::default();
        window.frame.open = true;
        assert_eq!(names(&window), ["swap", "take_off_all", "show_box"]);
        let [x, y] = window.frame.position;
        assert_eq!(window.button_boxes().iter().find(|(name, _)| name == "show_box").unwrap().1, [x + 10.0, y + 172.0, 80.0, 12.0]);
        assert_eq!(window.button_boxes().iter().find(|(name, _)| name == "swap").unwrap().1, [x + 193.0, y + 167.0, 40.0, 20.0]);
        assert_eq!(window.button_boxes().iter().find(|(name, _)| name == "take_off_all").unwrap().1, [x + 235.0, y + 167.0, 40.0, 20.0]);
        // A cart and a Madogear: both middle buttons, and the dismount button asks which.
        window.cart = true;
        window.option = OPTION_MADOGEAR;
        assert_eq!(names(&window), ["swap", "take_off_all", "show_box", "dismount", "cart_items"]);
        assert_eq!(window.dismounts(), [(UNINSTALL_CART, CART_OFF_MESSAGE), (UNINSTALL_MADOGEAR, 1464)]);
        assert_eq!(window.dismount_name(), Some(DISMOUNT_BOTH_MESSAGE));
        assert_eq!(window.hit([x + 130.0, y + 140.0], &[]), Some(Hit::Dismount));
        assert_eq!(window.hit([x + 130.0, y + 110.0], &[]), Some(Hit::CartItems));
        // Costume: the View Costume box in the Show Equipment box's place, no middle buttons.
        window.tab = 1;
        assert_eq!(names(&window), ["swap", "take_off_all", "view_costume"]);
        assert_eq!(window.hit([x + 20.0, y + 176.0], &[]), Some(Hit::ViewCostume));
        // Title: none of them; another player's window: none either.
        window.tab = TITLES_TAB;
        assert!(names(&window).iter().all(|name| name.starts_with("title:")));
        let other = Equipment::of("Someone".into(), [1024.0, 768.0]);
        assert!(names(&other).is_empty());
    }

    #[test]
    fn names_what_the_dismount_button_takes_off() {
        assert_eq!(mount(0, 0), None);
        assert_eq!(mount(OPTION_FALCON, 4012), Some((UNINSTALL_FALCON, 441)));
        assert_eq!(mount(OPTION_RIDING, 7), Some((UNINSTALL_MOUNT, 442)));
        assert_eq!(mount(OPTION_RIDING, 4082), Some((UNINSTALL_MOUNT, 2096)));
        assert_eq!(mount(0x80_0000, 4060), Some((UNINSTALL_DRAGON, 1463)));
        assert_eq!(mount(OPTION_MADOGEAR, 4086), Some((UNINSTALL_MADOGEAR, 1464)));
        let window = Equipment { cart: true, ..Equipment::default() };
        assert_eq!((window.dismounts(), window.dismount_name()), (vec![(UNINSTALL_CART, CART_OFF_MESSAGE)], Some(CART_OFF_MESSAGE)));
    }

    #[test]
    fn unequips_the_tabs_pieces_once_until_that_changes() {
        let item = |index, worn| InventoryItem { index, worn, ..Default::default() };
        // A Hat (top headgear) and a costume circlet (costume top) worn.
        let mut items = vec![item(1, 256), item(2, 1024), item(3, 0)];
        let mut window = Equipment::default();
        assert_eq!(window.unequip_all(&items), Some(UnequipAll::General));
        // Greyed until what is worn changes.
        assert!(!window.unequip_enabled(&items));
        assert_eq!(window.unequip_all(&items), None);
        items[0].worn = 0;
        // Costume: only the circlet; nothing worn there, nothing to do.
        window.tab = 1;
        assert_eq!(window.unequip_all(&items), Some(UnequipAll::Pieces(vec![2])));
        items[1].worn = 0;
        assert!(!window.unequip_enabled(&items));
        assert_eq!(window.unequip_all(&items), None);
    }

    #[test]
    fn swaps_the_set_in() {
        let knife = InventoryItem { index: 3, id: 1201, location: 2, worn: 2, ..Default::default() };
        let sword = InventoryItem { index: 4, id: 1101, location: 2, switch: 2, ..Default::default() };
        let mut window = Equipment { tab: 1, ..Equipment::default() };
        window.frame.open = true;
        let items = [knife.clone(), sword.clone()];
        let [x, y] = window.frame.at(offset(SWAP, [20.0, 10.0]));
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

    #[test]
    fn dresses_the_preview_from_the_tabs_own_gear() {
        let base = Look { job: 1, sex: 1, hair: 5, weapon: 1101, shield: 2101, head_top: 9, robe: 3, option: 0x10, ..Look::default() };
        // A Hat (view 16) on top, a two-slot mid and lower headgear (view 20), a cape (view 1), a costume circlet (view 77).
        let worn = [(0x100, 16), (0x201, 20), (0x004, 1), (0x400, 77), (0x002, 1101), (0x020, 2101)];
        let general = preview_look(&base, false, &worn);
        assert_eq!((general.head_top, general.head_mid, general.head_bottom, general.robe), (16, 20, 0, 1), "the two-slot one once");
        assert_eq!((general.weapon, general.shield, general.option), (0, 0, 0));
        assert_eq!((general.job, general.sex, general.hair), (1, 1, 5));
        let costume = preview_look(&base, true, &worn);
        assert_eq!((costume.head_top, costume.head_mid, costume.head_bottom, costume.robe), (77, 0, 0, 0), "empty costume slots stay empty");
        // A top-mid-lower helm shows once, on top.
        assert_eq!(preview_look(&base, false, &[(0x301, 30)]).head_mid, 0);
        // The feet, on General and Costume but not Title.
        let mut window = Equipment::default();
        window.frame.open = true;
        assert_eq!(window.preview_feet(), Some(window.frame.at([141.0, 149.0])));
        window.tab = 1;
        assert!(window.preview_feet().is_some());
        window.tab = TITLES_TAB;
        assert_eq!(window.preview_feet(), None);
    }
}
