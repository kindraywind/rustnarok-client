//! The lists the server asks the player to pick from: what a card goes into, what a Magnifier appraises, what to
//! make, which spell to keep ready. Two looks: the icon list (a small window of four rows, each an item's or
//! skill's icon and name) and the text list (Repair Weapon, Upgrade Weapon, making).

use std::collections::HashMap;

use rustnarok_client::grf::Grf;
use rustnarok_client::lua::{ItemInfo, SkillInfo};
use rustnarok_client::net::InventoryItem;
use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};
use crate::window::{self, Chrome, Clicks, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI, contains, screen};
use crate::{inventory, npc_menu, skills};

// The icon lists (identify, card, arrows, cooking, spells, eggs): one 200 x 200 window with a bare title bar, four
// 32 px rows from y 34 with nothing between them, a scroll bar past four rows, OK and Cancel on a 29 px footer.
const ICON_SIZE: [f32; 2] = [200.0, 200.0];
const ICON_ROWS: usize = 4;
const ICON_TOP: f32 = 34.0;
/// A press picks a row from the first x up to the second.
const ICON_PRESS: [f32; 2] = [4.0, 187.0];
const ICON_AT: f32 = 8.0;
const ICON_NAME: f32 = 40.0;
const ICON_NAME_WIDTH: f32 = 150.0;
/// A skill's name sits this far under its row's top; an item's name at the top.
const SKILL_NAME_DOWN: f32 = 4.0;
/// A graded item's mark, this far under its row's top.
const GRADE_DOWN: f32 = 12.0;
/// The selected row's band from its row's top-left: opaque, the colour of colorchip.bmp's pixel (6, 2).
const ICON_BAND: [f32; 4] = [5.0, -3.0, 176.0, 30.0];
const ICON_BAND_FILL: [u8; 4] = [0x70, 0x9F, 0xED, 0xFF];
const ICON_OK: [f32; 2] = [108.0, 176.0];
const ICON_CANCEL: [f32; 2] = [154.0, 176.0];
const ICON_FOOTER: f32 = 29.0;
/// The scroll bar, while there are more than four rows: its place and height, and the rows a wheel notch moves.
const ICON_BAR: [f32; 2] = [185.0, 34.0];
const ICON_BAR_HEIGHT: f32 = 145.0;
const WHEEL: i32 = 3;
/// Arrow Crafting's and the cooking list's title: this text, not a message.
pub const LIST_TITLE: &str = "LIST";

/// Where an icon list first opens: (392, 320) at 1024 x 768.
pub fn icon_position(screen: [f32; 2]) -> [f32; 2] {
    [((screen[0] - 640.0) / 2.0).floor() + 200.0, (200.0 * screen[1] / 480.0).floor()]
}

/// A kept place (the identify and card lists'), or the first place when the window wouldn't be wholly on screen there.
pub fn restored_place(saved: [f32; 2], screen: [f32; 2]) -> [f32; 2] {
    let fits = (0..2).all(|axis| saved[axis] >= 0.0 && saved[axis] + ICON_SIZE[axis] <= screen[axis]);
    if fits { saved } else { icon_position(screen) }
}

/// The card list's title: "Insert Card (Hydra Card)".
pub fn card_title(insert: &str, card: &str) -> String {
    format!("{insert} ({card})")
}

const ROW: f32 = 32.0;
const BUTTON: [f32; 2] = [42.0, 20.0];

// The text list: 280 x 150 with a 27 px footer, 16 px rows in a #F0F0F0 box.
const WIDTH: f32 = 280.0;
const FOOTER: f32 = 27.0;

const TEXT_HEIGHT: f32 = 150.0;
const TEXT_LIST_AT: [f32; 2] = [12.0, 22.0];
const TEXT_LIST_SIZE: [f32; 2] = [256.0, 95.0];
const TEXT_ROW: f32 = 16.0;
const TEXT_ROWS: usize = 6;
const TEXT_BAND: f32 = 15.0;
const TEXT_X: f32 = 3.0;
const TEXT_OK: [f32; 2] = [189.0, 126.0];
const TEXT_CANCEL: [f32; 2] = [234.0, 126.0];

/// Where the 280×150 text list window sits at screen size `screen`.
pub fn text_position(screen: [f32; 2]) -> [f32; 2] {
    [((screen[0] - 640.0) / 2.0 + 185.0).floor(), (300.0 * screen[1] / 480.0 - 60.0).floor()]
}

/// The plain item name for the text lists: identified name without +N, grade, cards or slot count.
pub fn plain_item_name(items: &HashMap<u32, ItemInfo>, id: u32) -> String {
    items.get(&id).map_or_else(|| id.to_string(), |info| info.name.clone())
}

/// What a pick list answers on click.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum PickChoice {
    /// A row was picked.
    Pick(usize),
    /// Cancelled: send the cancel packet.
    Cancel,
    /// Closed without sending any packet.
    Close,
}

pub struct Assets {
    pub strip: Texture,
    /// OK, cancel and make: normal, hovered, pressed.
    pub buttons: [[Texture; 3]; 3],
    /// The icon list's band and the marks of grades 1 to 4.
    band: Texture,
    grades: [Texture; 4],
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |path: String| gpu.load_texture(grf, &path, false);
        let button = |name: &str| ["", "_a", "_b"].map(|suffix| image(format!("{UI}btn_{name}{suffix}.bmp")));
        Self {
            strip: image(format!("{UI}basic_interface\\itemwin_left.bmp")),
            buttons: [button("ok"), button("cancel"), button("make")],
            band: gpu.solid(band_fill(
                crate::render::load_image(grf, &format!("{UI}colorchip.bmp")).ok().and_then(|chip| chip.get_pixel_checked(6, 2).map(|pixel| pixel.0)),
            )),
            grades: [1, 2, 3, 4].map(|grade| image(format!("{UI}grade_enchant\\grade_icon{grade}.bmp"))),
        }
    }
}

/// The band's colour, given colorchip.bmp's pixel when there is one.
fn band_fill(chip: Option<[u8; 4]>) -> [u8; 4] {
    chip.map_or(ICON_BAND_FILL, |[red, green, blue, _]| [red, green, blue, 255])
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(TitleHit),
    Row(usize),
    Ok,
    Cancel,
    /// The icon list's scroll bar, which goes by presses.
    Bar,
    Window,
}

pub struct PickList {
    pub frame: Frame,
    /// The title's message number, or the title itself (`named`).
    title: usize,
    named: Option<String>,
    /// The items to pick from: carried ones, or (to make) just their IDs.
    pub items: Vec<InventoryItem>,
    /// Make rather than OK on the button.
    pub making: bool,
    /// The rows are skills (their IDs in `id`), not items.
    pub skills: bool,
    /// Plain text list (repair, upgrade, making): 280x150 window, 16px rows, #F0F0F0 list.
    pub text_list: bool,
    selected: Option<usize>,
    scroll: usize,
    clicks: Clicks,
    /// The scroll bar's thumb while held.
    thumb: window::Thumb,
}

impl PickList {
    /// An icon list under the message `title`, at its first place.
    pub fn new(title: usize, items: Vec<InventoryItem>) -> Self {
        let mut frame = Frame::new(icon_position(screen()), ICON_SIZE[0]);
        frame.open = true;
        Self {
            frame,
            title,
            named: None,
            items,
            making: false,
            skills: false,
            text_list: false,
            selected: None,
            scroll: 0,
            clicks: Clicks::default(),
            thumb: window::Thumb::default(),
        }
    }

    /// An icon list with a title of its own: "LIST", the card list's.
    pub fn named(title: String, items: Vec<InventoryItem>) -> Self {
        Self { named: Some(title), ..Self::new(0, items) }
    }

    /// The 280×150 text list window: repair 812, upgrade 910, making 425. It opens with its first row selected.
    pub fn text(title: usize, items: Vec<InventoryItem>) -> Self {
        let mut frame = Frame::new(text_position(screen()), WIDTH);
        frame.open = true;
        let making = title == crate::MAKING_TITLE;
        let selected = (!items.is_empty()).then_some(0);
        Self { frame, making, text_list: true, selected, ..Self::new(title, items) }
    }

    /// The icon list's look, not the text list's.
    fn icon_look(&self) -> bool {
        !self.text_list
    }

    fn height(&self) -> f32 {
        if self.text_list { TEXT_HEIGHT } else { ICON_SIZE[1] }
    }

    fn buttons(&self) -> ([f32; 2], [f32; 2]) {
        if self.text_list { (TEXT_OK, TEXT_CANCEL) } else { (ICON_OK, ICON_CANCEL) }
    }

    /// The most the rows scroll: the rows past the ones shown.
    fn most(&self) -> usize {
        self.items.len().saturating_sub(if self.text_list { TEXT_ROWS } else { ICON_ROWS })
    }

    /// An icon list's scroll bar: it shows past four rows.
    fn bar(&self) -> Option<window::ScrollBar> {
        self.icon_look().then(|| window::ScrollBar {
            at: self.frame.at(ICON_BAR),
            height: ICON_BAR_HEIGHT,
            shown: ICON_ROWS,
            rows: self.items.len(),
            first: self.scroll,
        })
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let local = self.frame.local(cursor, self.height())?;
        // No list's title bar has buttons: all of it is the bar.
        if local[1] < TITLE_HEIGHT {
            return Some(Hit::Title(TitleHit::Title));
        }
        let (ok, cancel) = self.buttons();
        if contains(ok, BUTTON, local) {
            return Some(Hit::Ok);
        }
        if contains(cancel, BUTTON, local) {
            return Some(Hit::Cancel);
        }
        if self.icon_look() {
            if self.bar().is_some_and(|bar| bar.hit(cursor).is_some()) {
                return Some(Hit::Bar);
            }
            let on_rows = (ICON_PRESS[0]..ICON_PRESS[1]).contains(&local[0]) && (ICON_TOP..ICON_TOP + ICON_ROWS as f32 * ROW).contains(&local[1]);
            let row = ((local[1] - ICON_TOP) / ROW) as usize + self.scroll;
            return Some(if on_rows && row < self.items.len() { Hit::Row(row) } else { Hit::Window });
        }
        if contains(TEXT_LIST_AT, TEXT_LIST_SIZE, local) {
            let row = ((local[1] - TEXT_LIST_AT[1]) / TEXT_ROW) as usize + self.scroll;
            if row < self.items.len() {
                return Some(Hit::Row(row));
            }
        }
        Some(Hit::Window)
    }

    pub fn covers(&self, cursor: [f32; 2]) -> bool {
        self.hit(cursor).is_some()
    }

    /// A press: the title bar drags the window, and a text list's body too, off its list box; the scroll bar steps,
    /// pages or has its thumb held.
    pub fn press(&mut self, cursor: [f32; 2]) {
        if let Some(first) = self.bar().and_then(|bar| bar.press(cursor, &mut self.thumb)) {
            self.scroll = first;
        } else if self.grips(cursor) {
            self.frame.start_drag(cursor);
        }
    }

    /// Whether a press at `cursor` drags the window.
    fn grips(&self, cursor: [f32; 2]) -> bool {
        let on_list = self.frame.local(cursor, self.height()).is_some_and(|local| contains(TEXT_LIST_AT, TEXT_LIST_SIZE, local));
        match self.hit(cursor) {
            Some(Hit::Title(_)) => true,
            Some(Hit::Window) => self.text_list && !on_list,
            _ => false,
        }
    }

    /// The pointer moved: a dragged window follows it, a held thumb scrolls in whole rows.
    pub fn drag_to(&mut self, cursor: [f32; 2]) {
        self.frame.drag_to(cursor, self.height());
        if let Some(first) = self.thumb.first(cursor[1]) {
            self.scroll = first;
        }
    }

    /// The button went up: whether a drag ended.
    pub fn drop_drag(&mut self) -> bool {
        self.thumb.stop() | self.frame.drop_drag()
    }

    /// What a click answers: Some(PickChoice) answers or closes, None is no answer yet.
    pub fn click(&mut self, hit: Hit) -> Option<PickChoice> {
        match hit {
            Hit::Title(_) => None,
            Hit::Cancel => Some(PickChoice::Cancel),
            Hit::Row(row) => {
                self.selected = Some(row);
                if self.clicks.double(row as u16) { Some(PickChoice::Pick(row)) } else { None }
            }
            Hit::Ok => {
                if let Some(row) = self.selected {
                    Some(PickChoice::Pick(row))
                } else if self.text_list {
                    if self.making {
                        Some(PickChoice::Close)
                    } else if self.items.is_empty() {
                        Some(PickChoice::Cancel)
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            Hit::Bar => None,
            // A press off the icon list's rows clears the selection.
            Hit::Window => {
                if self.icon_look() {
                    self.selected = None;
                }
                None
            }
        }
    }

    fn scroll_rows(&mut self, rows: i32) {
        self.scroll = (self.scroll as i32 + rows).clamp(0, self.most() as i32) as usize;
    }

    /// The wheel, `notches` down when positive: three rows a notch.
    pub fn scroll_by(&mut self, notches: i32) {
        self.scroll_rows(notches * WHEEL);
    }

    /// The rows on screen: each item's ID and its box.
    pub fn row_boxes(&self) -> Vec<(u32, [f32; 4])> {
        if !self.frame.open || self.frame.minimized {
            return Vec::new();
        }
        if self.text_list {
            let rows = self.items.iter().enumerate().skip(self.scroll).take(TEXT_ROWS);
            return rows
                .map(|(row, item)| {
                    let [x, y] = self.frame.at([TEXT_LIST_AT[0], TEXT_LIST_AT[1] + (row - self.scroll) as f32 * TEXT_ROW]);
                    (item.id, [x, y, TEXT_LIST_SIZE[0], TEXT_ROW])
                })
                .collect();
        }
        let rows = self.items.iter().enumerate().skip(self.scroll).take(ICON_ROWS);
        rows.map(|(row, item)| {
            let [x, y] = self.frame.at([ICON_PRESS[0], ICON_TOP + (row - self.scroll) as f32 * ROW]);
            (item.id, [x, y, ICON_PRESS[1] - ICON_PRESS[0], ROW])
        })
        .collect()
    }

    /// OK and Cancel on screen, named for scripted clicks: `ok`, `cancel`.
    pub fn button_boxes(&self) -> Vec<(String, [f32; 4])> {
        let (ok, cancel) = self.buttons();
        [("ok", ok), ("cancel", cancel)]
            .map(|(name, corner)| {
                let [x, y] = self.frame.at(corner);
                (name.to_owned(), [x, y, BUTTON[0], BUTTON[1]])
            })
            .to_vec()
    }

    pub fn icons(&self, items: &HashMap<u32, ItemInfo>, skill_info: &HashMap<u16, SkillInfo>) -> Vec<String> {
        if self.text_list {
            return Vec::new();
        }
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
        npc_menu: &'a npc_menu::Assets,
        icons: &'a HashMap<String, Option<Texture>>,
        items: &HashMap<u32, ItemInfo>,
        (cards, makers): inventory::Naming,
        skill_info: &HashMap<u16, SkillInfo>,
        messages: &[String],
        font: &Font,
        pointer: ([f32; 2], bool),
    ) {
        let frame = &self.frame;
        let title = self.named.as_deref().unwrap_or_else(|| messages.get(self.title).map_or("", String::as_str));
        if self.icon_look() {
            // A bare title bar; the spell list's has the base picture, which is no button.
            frame.draw_title_only(layer, chrome, title);
            if self.skills {
                let base = chrome.button(0, false);
                layer.sprites.push((base, frame.at([3.0, 3.0]), base.size));
            }
            chrome.draw_body(layer, frame.position, ICON_SIZE);
            chrome.draw_footer(layer, frame.at([0.0, ICON_SIZE[1]]), ICON_SIZE[0], ICON_FOOTER);
            for (row, item) in self.items.iter().enumerate().skip(self.scroll).take(ICON_ROWS) {
                let y = ICON_TOP + (row - self.scroll) as f32 * ROW;
                if self.selected == Some(row) {
                    layer.sprites.push((&assets.band, frame.at([ICON_BAND[0], y + ICON_BAND[1]]), [ICON_BAND[2], ICON_BAND[3]]));
                }
                if let Some(icon) = self.icon_path(item, items, skill_info).and_then(|path| icons.get(&path)).and_then(Option::as_ref) {
                    layer.sprites.push((icon, frame.at([ICON_AT, y]), icon.size));
                }
                let (name, down) = if self.skills {
                    (
                        u16::try_from(item.id).ok().and_then(|skill| skill_info.get(&skill)).map_or_else(|| item.id.to_string(), |info| info.name.clone()),
                        SKILL_NAME_DOWN,
                    )
                } else {
                    if let Some(mark) = usize::from(item.grade).checked_sub(1).and_then(|grade| assets.grades.get(grade)) {
                        layer.sprites.push((mark, frame.at([ICON_AT, y + GRADE_DOWN]), mark.size));
                    }
                    (inventory::full_name(items, cards, messages, makers, item, true), 0.0)
                };
                layer.texts.push((window::fit(font, &name, ICON_NAME_WIDTH), frame.at([ICON_NAME, y + down]), [0, 0, 0]));
            }
            if let Some(bar) = self.bar() {
                bar.draw(layer, chrome);
            }
            let hit = self.hit(pointer.0);
            for (target, place, images) in [(Hit::Ok, ICON_OK, &assets.buttons[0]), (Hit::Cancel, ICON_CANCEL, &assets.buttons[1])] {
                let look = usize::from(hit == Some(target)) + usize::from(hit == Some(target) && pointer.1);
                layer.sprites.push((&images[look], frame.at(place), BUTTON));
            }
            return;
        }
        // The bar, the title and the base picture, which is no button.
        frame.draw_title_only(layer, chrome, title);
        let base = chrome.button(0, false);
        layer.sprites.push((base, frame.at([3.0, 3.0]), base.size));
        chrome.draw_body(layer, frame.position, [WIDTH, TEXT_HEIGHT]);
        chrome.draw_footer(layer, frame.at([0.0, TEXT_HEIGHT]), WIDTH, FOOTER);
        layer.sprites.push((&npc_menu.list, frame.at(TEXT_LIST_AT), TEXT_LIST_SIZE));
        for (row, item) in self.items.iter().enumerate().skip(self.scroll).take(TEXT_ROWS) {
            let top = TEXT_LIST_AT[1] + (row - self.scroll) as f32 * TEXT_ROW;
            if self.selected == Some(row) {
                layer.sprites.push((&npc_menu.band, frame.at([TEXT_LIST_AT[0], top]), [TEXT_LIST_SIZE[0], TEXT_BAND]));
            }
            let name = plain_item_name(items, item.id);
            layer.texts.push((window::fit(font, &name, TEXT_LIST_SIZE[0] - TEXT_X * 2.0), frame.at([TEXT_LIST_AT[0] + TEXT_X, top]), [0, 0, 0]));
        }
        let hit = self.hit(pointer.0);
        let look = |target: Hit| {
            let hovered = hit == Some(target);
            usize::from(hovered) + usize::from(hovered && pointer.1)
        };
        layer.sprites.push((&assets.buttons[0][look(Hit::Ok)], frame.at(TEXT_OK), BUTTON));
        layer.sprites.push((&assets.buttons[1][look(Hit::Cancel)], frame.at(TEXT_CANCEL), BUTTON));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(count: u16) -> Vec<InventoryItem> {
        (0..count).map(|index| InventoryItem { index, id: 1201 + u32::from(index), identified: true, ..Default::default() }).collect()
    }

    #[test]
    fn picks_and_cancels() {
        let mut list = PickList::new(522, rows(2));
        let [x, y, ..] = list.row_boxes()[1].1;
        let row = list.hit([x + 40.0, y + 10.0]).unwrap();
        assert_eq!(row, Hit::Row(1));
        // OK picks the highlighted row; a double click picks at once; Cancel answers no.
        assert_eq!(list.click(Hit::Ok), None);
        assert_eq!(list.click(row), None);
        assert_eq!(list.click(Hit::Ok), Some(PickChoice::Pick(1)));
        assert_eq!(list.click(Hit::Row(0)), None);
        assert_eq!(list.click(Hit::Row(0)), Some(PickChoice::Pick(0)));
        assert_eq!(list.click(Hit::Cancel), Some(PickChoice::Cancel));
        // A press off the rows clears the selection: OK then answers nothing.
        assert_eq!(list.click(Hit::Row(1)), None);
        assert_eq!(list.click(Hit::Window), None);
        assert_eq!(list.click(Hit::Ok), None);
    }

    /// The icon list is the game's 200 x 200 window: its place, rows, band, buttons and bare title bar.
    #[test]
    fn icon_list_layout() {
        assert_eq!((icon_position([1024.0, 768.0]), icon_position([1280.0, 960.0])), ([392.0, 320.0], [520.0, 400.0]));
        let mut list = PickList::new(521, rows(4));
        list.frame.position = [100.0, 50.0];
        let at = |list: &PickList, x: f32, y: f32| list.hit([100.0 + x, 50.0 + y]);
        assert_eq!(list.height(), 200.0);
        assert_eq!((at(&list, 199.0, 199.0), at(&list, 200.0, 100.0), at(&list, 100.0, 200.0)), (Some(Hit::Window), None, None));
        // Four rows from y 34, 32 px apart, pressed from x 4 to 186.
        let boxes = list.row_boxes();
        assert_eq!(boxes.iter().map(|(_, area)| area[1] - 50.0).collect::<Vec<_>>(), [34.0, 66.0, 98.0, 130.0]);
        assert_eq!((at(&list, 4.0, 34.0), at(&list, 186.0, 161.0)), (Some(Hit::Row(0)), Some(Hit::Row(3))));
        assert_eq!([at(&list, 3.0, 40.0), at(&list, 187.0, 40.0), at(&list, 50.0, 33.0), at(&list, 50.0, 162.0)], [Some(Hit::Window); 4]);
        // The band behind the selected row; OK and Cancel on the footer.
        assert_eq!([ICON_BAND[0], ICON_TOP + ICON_BAND[1], ICON_BAND[2], ICON_BAND[3]], [5.0, 31.0, 176.0, 30.0]);
        assert_eq!(list.button_boxes().iter().map(|(_, area)| [area[0] - 100.0, area[1] - 50.0]).collect::<Vec<_>>(), [[108.0, 176.0], [154.0, 176.0]]);
        assert_eq!((at(&list, 108.0, 176.0), at(&list, 195.0, 195.0)), (Some(Hit::Ok), Some(Hit::Cancel)));
        // The title bar has no close or minimize button: all of it drags, and a click on it answers nothing.
        assert_eq!([at(&list, 190.0, 5.0), at(&list, 175.0, 5.0), at(&list, 50.0, 16.0)], [Some(Hit::Title(TitleHit::Title)); 3]);
        assert_eq!(list.click(Hit::Title(TitleHit::Title)), None);
        list.press([100.0 + 190.0, 50.0 + 5.0]);
        list.drag_to([100.0 + 200.0, 50.0 + 25.0]);
        assert!(list.drop_drag());
        assert_eq!(list.frame.position, [110.0, 70.0]);
    }

    #[test]
    fn icon_list_scrolls_past_four_rows() {
        // Four rows: no bar, and the wheel does nothing.
        let mut list = PickList::new(521, rows(4));
        list.frame.position = [0.0, 0.0];
        list.scroll_by(1);
        assert_eq!((list.bar().unwrap().boxes(), list.scroll, list.hit([190.0, 40.0])), (None, 0, Some(Hit::Window)));
        // Six rows: the bar at (185, 34), 13 x 145; the wheel moves three rows and stops at the last four.
        let mut list = PickList::new(521, rows(6));
        list.frame.position = [0.0, 0.0];
        let [up, down, thumb] = list.bar().unwrap().boxes().unwrap();
        assert_eq!((up, down), ([185.0, 34.0, 13.0, 13.0], [185.0, 166.0, 13.0, 13.0]));
        assert_eq!((list.hit([185.0, 34.0]), list.hit([197.0, 178.0]), list.hit([198.0, 100.0])), (Some(Hit::Bar), Some(Hit::Bar), Some(Hit::Window)));
        list.scroll_by(1);
        assert_eq!((list.scroll, list.row_boxes()[0].0, list.hit([50.0, 40.0])), (2, 1203, Some(Hit::Row(2))));
        list.scroll_by(-1);
        assert_eq!(list.scroll, 0);
        // A press on an arrow moves a row and on the track three; a click on the bar answers nothing and keeps the
        // selection; the held thumb scrolls whole rows.
        list.click(Hit::Row(1));
        list.press([190.0, 170.0]);
        assert_eq!((list.scroll, list.click(Hit::Bar), list.selected), (1, None, Some(1)));
        list.press([190.0, 160.0]);
        assert_eq!(list.scroll, 2);
        list.press([190.0, 40.0]);
        list.press([190.0, 40.0]);
        assert_eq!(list.scroll, 0);
        list.press([thumb[0] + 5.0, thumb[1] + 5.0]);
        list.drag_to([190.0, 300.0]);
        assert!(list.scroll == 2 && list.drop_drag() && !list.drop_drag());
    }

    #[test]
    fn icon_list_titles_and_kept_places() {
        assert_eq!((card_title("Insert Card", "Hydra Card"), LIST_TITLE), ("Insert Card (Hydra Card)".to_owned(), "LIST"));
        assert_eq!(PickList::named(LIST_TITLE.into(), rows(1)).named.as_deref(), Some("LIST"));
        // A kept place off the screen, even in part, is dropped for the first place.
        let screen = [1024.0, 768.0];
        assert_eq!((restored_place([10.0, 20.0], screen), restored_place([824.0, 568.0], screen)), ([10.0, 20.0], [824.0, 568.0]));
        assert_eq!([restored_place([825.0, 20.0], screen), restored_place([10.0, 569.0], screen), restored_place([-1.0, 20.0], screen)], [[392.0, 320.0]; 3]);
        assert_eq!(band_fill(None), ICON_BAND_FILL);
        assert_eq!(band_fill(Some([1, 2, 3, 0])), [1, 2, 3, 255]);
    }

    #[test]
    fn text_list_place_and_row_hit_test() {
        assert_eq!(text_position([1024.0, 768.0]), [377.0, 420.0]);
        let items: Vec<_> = (0..8).map(|i| InventoryItem { index: i, id: 1000 + i as u32, identified: true, ..Default::default() }).collect();
        let list = PickList::text(crate::REPAIR_TITLE, items);
        let [wx, wy] = list.frame.position;
        assert_eq!([wx, wy], [377.0, 420.0]);

        // 16-px rows from y 22:
        assert_eq!(list.hit([wx + 50.0, wy + 22.0]), Some(Hit::Row(0)));
        assert_eq!(list.hit([wx + 50.0, wy + 37.9]), Some(Hit::Row(0)));
        assert_eq!(list.hit([wx + 50.0, wy + 38.0]), Some(Hit::Row(1)));
        assert_eq!(list.hit([wx + 50.0, wy + 54.0]), Some(Hit::Row(2)));
        assert_eq!(list.hit([wx + 50.0, wy + 70.0]), Some(Hit::Row(3)));
        assert_eq!(list.hit([wx + 50.0, wy + 86.0]), Some(Hit::Row(4)));
        assert_eq!(list.hit([wx + 50.0, wy + 102.0]), Some(Hit::Row(5)));

        // Outside list area:
        assert_eq!(list.hit([wx + 50.0, wy + 21.9]), Some(Hit::Window));
        assert_eq!(list.hit([wx + 50.0, wy + 117.0]), Some(Hit::Window));
        assert_eq!(list.hit([wx + 11.9, wy + 30.0]), Some(Hit::Window));
        assert_eq!(list.hit([wx + 268.1, wy + 30.0]), Some(Hit::Window));

        // Buttons:
        assert_eq!(list.hit([wx + 189.0 + 5.0, wy + 126.0 + 5.0]), Some(Hit::Ok));
        assert_eq!(list.hit([wx + 234.0 + 5.0, wy + 126.0 + 5.0]), Some(Hit::Cancel));
    }

    #[test]
    fn text_list_answers() {
        let item = |index: u16, id: u32| InventoryItem { index, id, identified: true, ..Default::default() };

        // Each opens with its first row selected: OK answers it at once; a row picked first is answered; Cancel cancels.
        let mut repair = PickList::text(crate::REPAIR_TITLE, vec![item(1, 1201), item(2, 1202)]);
        assert_eq!(repair.click(Hit::Ok), Some(PickChoice::Pick(0)));
        assert_eq!(repair.click(Hit::Row(1)), None);
        assert_eq!(repair.click(Hit::Ok), Some(PickChoice::Pick(1)));
        assert_eq!(repair.click(Hit::Cancel), Some(PickChoice::Cancel));
        let mut upgrade = PickList::text(crate::UPGRADE_TITLE, vec![item(5, 1201)]);
        assert_eq!(upgrade.click(Hit::Ok), Some(PickChoice::Pick(0)));
        assert_eq!(upgrade.click(Hit::Cancel), Some(PickChoice::Cancel));
        // Upgrade's -1 bytes:
        assert_eq!(rustnarok_client::net::encode_upgrade(u32::MAX), [0x22, 0x02, 0xFF, 0xFF, 0xFF, 0xFF]);
        let mut make = PickList::text(crate::MAKING_TITLE, vec![item(0, 501)]);
        assert_eq!(make.click(Hit::Ok), Some(PickChoice::Pick(0)));
        assert_eq!(make.click(Hit::Cancel), Some(PickChoice::Cancel));

        // With no rows (the server sends none so): repair and upgrade cancel, making closes.
        let mut empty_repair = PickList::text(crate::REPAIR_TITLE, Vec::new());
        assert_eq!((empty_repair.selected, empty_repair.click(Hit::Ok)), (None, Some(PickChoice::Cancel)));
        let mut empty_upgrade = PickList::text(crate::UPGRADE_TITLE, Vec::new());
        assert_eq!(empty_upgrade.click(Hit::Ok), Some(PickChoice::Cancel));
        let mut empty_make = PickList::text(crate::MAKING_TITLE, Vec::new());
        assert_eq!(empty_make.click(Hit::Ok), Some(PickChoice::Close));
    }

    /// A text list's title bar has only the base picture; it drags by any part but its list box and buttons.
    #[test]
    fn text_list_has_no_title_buttons_and_drags_by_its_body() {
        let mut list = PickList::text(crate::REPAIR_TITLE, vec![InventoryItem { index: 1, id: 1201, identified: true, ..Default::default() }]);
        list.frame.position = [0.0, 0.0];
        assert_eq!([list.hit([270.0, 5.0]), list.hit([255.0, 5.0])], [Some(Hit::Title(TitleHit::Title)); 2]);
        assert_eq!(list.click(Hit::Title(TitleHit::Title)), None);
        assert!(list.frame.open && !list.frame.minimized);
        // The title bar and the body off the list drag; the list (a row or below the rows) and the buttons don't.
        assert!(list.grips([270.0, 5.0]) && list.grips([100.0, 122.0]) && list.grips([5.0, 60.0]));
        assert!(!list.grips([50.0, 25.0]) && !list.grips([50.0, 100.0]) && !list.grips([195.0, 130.0]) && !list.grips([240.0, 130.0]));
        list.press([100.0, 122.0]);
        list.drag_to([150.0, 172.0]);
        assert!(list.drop_drag());
        assert_eq!(list.frame.position, [50.0, 50.0]);
    }

    #[test]
    fn plain_row_name_for_carded_weapon() {
        let mut items = HashMap::new();
        items.insert(
            1201,
            ItemInfo {
                name: "Knife".into(),
                unidentified_name: "Knife".into(),
                resource: "칼".into(),
                unidentified_resource: "칼".into(),
                slots: 3,
                class_num: 1,
                description: Vec::new(),
                unidentified_description: Vec::new(),
                costume: false,
                effect_id: None,
            },
        );
        // A +7 carded weapon:
        let weapon = InventoryItem { id: 1201, refine: 7, cards: [4001, 0, 0, 0], grade: 1, identified: true, ..Default::default() };
        // Plain item name is simply "Knife" (no +7, no grade, no card words, no slot count):
        assert_eq!(plain_item_name(&items, weapon.id), "Knife");
    }
}
