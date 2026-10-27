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
use crate::{Layer, UI, contains, screen};
use crate::{inventory, npc_menu, skills};

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

/// Item merge: its title, the hint over the rows (two lines), and its refusals.
// ponytail: data.grf has no picture for this window; the lines next to its title in msgstringtable.
pub const MERGE_TITLE: usize = 2169;
const MERGE_HINT: usize = 2170;
const HINT: f32 = 28.0;
const ONE_KIND: usize = 2172;

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
    /// Item merge: several rows picked (in the order picked), each with its amount, under a hint.
    pub merging: bool,
    /// Plain text list (repair, upgrade, making): 280x150 window, 16px rows, #F0F0F0 list.
    pub text_list: bool,
    picked: Vec<usize>,
    selected: Option<usize>,
    scroll: usize,
    clicks: Clicks,
}

impl PickList {
    pub fn new(title: usize, items: Vec<InventoryItem>) -> Self {
        let height = TITLE_HEIGHT + PADDING * 2.0 + ROWS as f32 * ROW + FOOTER;
        let mut frame = Frame::new([((screen()[0] - WIDTH) / 2.0).floor(), ((screen()[1] - height) / 2.0).floor()], WIDTH);
        frame.open = true;
        Self {
            frame,
            title,
            items,
            making: false,
            skills: false,
            merging: false,
            text_list: false,
            picked: Vec::new(),
            selected: None,
            scroll: 0,
            clicks: Clicks::default(),
        }
    }

    /// The 280×150 text list window: repair 812, upgrade 910, making 425.
    pub fn text(title: usize, items: Vec<InventoryItem>) -> Self {
        let mut frame = Frame::new(text_position(screen()), WIDTH);
        frame.open = true;
        let making = title == crate::MAKING_TITLE;
        Self {
            frame,
            title,
            items,
            making,
            skills: false,
            merging: false,
            text_list: true,
            picked: Vec::new(),
            selected: None,
            scroll: 0,
            clicks: Clicks::default(),
        }
    }

    /// The item merge window: the stacks sorted by item, then by index.
    pub fn merging(mut items: Vec<InventoryItem>) -> Self {
        items.sort_by_key(|item| (item.id, item.index));
        Self { merging: true, ..Self::new(MERGE_TITLE, items) }
    }

    /// Where the rows begin.
    fn top(&self) -> f32 {
        TITLE_HEIGHT + PADDING + if self.merging { HINT } else { 0.0 }
    }

    fn height(&self) -> f32 {
        if self.text_list {
            return TEXT_HEIGHT;
        }
        self.top() + PADDING + ROWS as f32 * ROW + FOOTER
    }

    fn buttons(&self) -> ([f32; 2], [f32; 2]) {
        if self.text_list {
            return (TEXT_OK, TEXT_CANCEL);
        }
        let top = self.height() - FOOTER + 4.0;
        ([WIDTH - 62.0 - BUTTON[0], top], [WIDTH - 15.0 - BUTTON[0], top])
    }

    /// Item merge: picks a row or unpicks it; a stack of another item than the picked ones is refused (its line).
    pub fn toggle(&mut self, row: usize) -> Result<(), usize> {
        if let Some(at) = self.picked.iter().position(|&picked| picked == row) {
            self.picked.remove(at);
            return Ok(());
        }
        let id = |row: usize| self.items.get(row).map(|item| item.id);
        if self.picked.first().is_some_and(|&first| id(first) != id(row)) {
            return Err(ONE_KIND);
        }
        self.picked.push(row);
        Ok(())
    }

    /// Item merge: the picked stacks' indexes to send (the first picked first), or the hint's line when fewer than
    /// two are picked.
    pub fn merge_request(&self) -> Result<Vec<u16>, usize> {
        let indexes = self.picked_indexes();
        if indexes.len() < 2 { Err(MERGE_HINT) } else { Ok(indexes) }
    }

    pub fn picked_indexes(&self) -> Vec<u16> {
        self.picked.iter().filter_map(|&row| self.items.get(row)).map(|item| item.index).collect()
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let local = self.frame.local(cursor, self.height())?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(Hit::Title(hit));
        }
        let (ok, cancel) = self.buttons();
        if contains(ok, BUTTON, local) {
            return Some(Hit::Ok);
        }
        if contains(cancel, BUTTON, local) {
            return Some(Hit::Cancel);
        }
        if self.text_list {
            if contains(TEXT_LIST_AT, TEXT_LIST_SIZE, local) {
                let row = ((local[1] - TEXT_LIST_AT[1]) / TEXT_ROW) as usize + self.scroll;
                if row < self.items.len() {
                    return Some(Hit::Row(row));
                }
            }
            return Some(Hit::Window);
        }
        let top = self.top();
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

    /// What a click answers: Some(PickChoice) answers or closes, None is no answer yet.
    pub fn click(&mut self, hit: Hit) -> Option<PickChoice> {
        match hit {
            Hit::Title(TitleHit::Close) | Hit::Cancel => Some(PickChoice::Cancel),
            Hit::Title(title) => {
                self.frame.click_title(title);
                None
            }
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
            Hit::Window => None,
        }
    }

    pub fn scroll_by(&mut self, rows: i32) {
        if self.text_list {
            let most = self.items.len().saturating_sub(TEXT_ROWS);
            self.scroll = (self.scroll as i32 + rows * 3).clamp(0, most as i32) as usize;
            return;
        }
        let most = self.items.len().saturating_sub(ROWS);
        self.scroll = (self.scroll as i32 + rows).clamp(0, most as i32) as usize;
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
        let top = self.top();
        let rows = self.items.iter().enumerate().skip(self.scroll).take(ROWS);
        rows.map(|(row, item)| {
            let [x, y] = self.frame.at([LEFT, top + (row - self.scroll) as f32 * ROW]);
            (item.id, [x, y, WIDTH - LEFT - 1.0, ROW])
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
        frame.draw_title(layer, chrome, messages.get(self.title).map_or("", String::as_str), pointer.0);
        if frame.minimized {
            return;
        }
        if self.text_list {
            let body_height = TEXT_HEIGHT - TITLE_HEIGHT - FOOTER;
            layer.sprites.push((&chrome.white, frame.at([0.0, TITLE_HEIGHT]), [WIDTH, body_height]));
            layer.sprites.push((&chrome.border, frame.at([0.0, TITLE_HEIGHT]), [1.0, body_height]));
            layer.sprites.push((&chrome.border, frame.at([WIDTH - 1.0, TITLE_HEIGHT]), [1.0, body_height]));
            layer.sprites.push((&assets.bar, frame.at([0.0, TEXT_HEIGHT - FOOTER]), [WIDTH, FOOTER]));
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
            return;
        }
        let height = self.height();
        let list_height = height - TITLE_HEIGHT - FOOTER;
        layer.sprites.push((&chrome.white, frame.at([0.0, TITLE_HEIGHT]), [WIDTH, list_height]));
        layer.sprites.push((&assets.strip, frame.at([0.0, TITLE_HEIGHT]), [assets.strip.size[0], list_height]));
        layer.sprites.push((&chrome.border, frame.at([WIDTH - 1.0, TITLE_HEIGHT]), [1.0, list_height]));
        if self.merging {
            let hint = window::wrap(font, messages.get(MERGE_HINT).map_or("", String::as_str), WIDTH - LEFT - 8.0, 2);
            for (line, text) in hint.into_iter().enumerate() {
                layer.texts.push((text, frame.at([LEFT + 4.0, TITLE_HEIGHT + PADDING + line as f32 * 13.0]), [0, 0, 0]));
            }
        }
        let top = self.top();
        for (row, item) in self.items.iter().enumerate().skip(self.scroll).take(ROWS) {
            let y = top + (row - self.scroll) as f32 * ROW;
            if self.selected == Some(row) && !self.merging || self.picked.contains(&row) {
                layer.sprites.push((&assets.selected, frame.at([LEFT, y]), [WIDTH - LEFT - 1.0, ROW]));
            }
            let icon = self.icon_path(item, items, skill_info).and_then(|path| icons.get(&path)).and_then(Option::as_ref);
            if let Some(icon) = icon {
                layer.sprites.push((icon, frame.at([LEFT + 4.0, y + 4.0]), icon.size));
            }
            let name = if self.skills {
                u16::try_from(item.id).ok().and_then(|skill| skill_info.get(&skill)).map_or_else(|| item.id.to_string(), |info| info.name.clone())
            } else {
                inventory::full_name(items, cards, messages, makers, item, true)
            };
            layer.texts.push((window::fit(font, &name, WIDTH - LEFT - 80.0), frame.at([LEFT + 32.0, y + 10.0]), [0, 0, 0]));
            if self.merging {
                let amount = item.amount.to_string();
                layer.texts.push((amount.clone(), frame.at([WIDTH - 10.0 - font.width(&amount), y + 10.0]), [0, 0, 0]));
            }
        }
        let footer = height - FOOTER;
        layer.sprites.push((&assets.bar, frame.at([0.0, footer]), [WIDTH, FOOTER]));
        let hit = self.hit(pointer.0);
        let look = |target: Hit| {
            let hovered = hit == Some(target);
            usize::from(hovered) + usize::from(hovered && pointer.1)
        };
        let (ok, cancel) = self.buttons();
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
        assert_eq!(list.click(Hit::Ok), Some(PickChoice::Pick(1)));
        assert_eq!(list.click(Hit::Row(0)), None);
        assert_eq!(list.click(Hit::Row(0)), Some(PickChoice::Pick(0)));
        assert_eq!(list.click(Hit::Cancel), Some(PickChoice::Cancel));
        assert_eq!(list.click(Hit::Title(TitleHit::Close)), Some(PickChoice::Cancel));
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

        // 1. Repair: OK with row -> Pick(row), OK nothing selected -> None, OK no rows -> Cancel, Cancel -> Cancel
        let mut repair = PickList::text(crate::REPAIR_TITLE, vec![item(1, 1201), item(2, 1202)]);
        assert_eq!(repair.click(Hit::Ok), None);
        assert_eq!(repair.click(Hit::Row(0)), None);
        assert_eq!(repair.click(Hit::Ok), Some(PickChoice::Pick(0)));
        assert_eq!(repair.click(Hit::Cancel), Some(PickChoice::Cancel));
        let mut empty_repair = PickList::text(crate::REPAIR_TITLE, Vec::new());
        assert_eq!(empty_repair.click(Hit::Ok), Some(PickChoice::Cancel));

        // 2. Upgrade: OK with row -> Pick(row), OK nothing selected -> None, OK no rows -> Cancel, Cancel -> Cancel (-1 bytes)
        let mut upgrade = PickList::text(crate::UPGRADE_TITLE, vec![item(5, 1201)]);
        assert_eq!(upgrade.click(Hit::Ok), None);
        assert_eq!(upgrade.click(Hit::Row(0)), None);
        assert_eq!(upgrade.click(Hit::Ok), Some(PickChoice::Pick(0)));
        assert_eq!(upgrade.click(Hit::Cancel), Some(PickChoice::Cancel));
        let mut empty_upgrade = PickList::text(crate::UPGRADE_TITLE, Vec::new());
        assert_eq!(empty_upgrade.click(Hit::Ok), Some(PickChoice::Cancel));
        // Upgrade's -1 bytes:
        assert_eq!(rustnarok_client::net::encode_upgrade(u32::MAX), [0x22, 0x02, 0xFF, 0xFF, 0xFF, 0xFF]);

        // 3. Making: OK with row -> Pick(row), OK nothing selected -> Close, OK no rows -> Close, Cancel -> Cancel
        let mut make = PickList::text(crate::MAKING_TITLE, vec![item(0, 501)]);
        assert_eq!(make.click(Hit::Ok), Some(PickChoice::Close));
        assert_eq!(make.click(Hit::Row(0)), None);
        assert_eq!(make.click(Hit::Ok), Some(PickChoice::Pick(0)));
        assert_eq!(make.click(Hit::Cancel), Some(PickChoice::Cancel));
        let mut empty_make = PickList::text(crate::MAKING_TITLE, Vec::new());
        assert_eq!(empty_make.click(Hit::Ok), Some(PickChoice::Close));
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

    #[test]
    fn merges_stacks_of_one_item() {
        let stack = |index: u16, id: u32, amount: u16| InventoryItem { index, id, amount, identified: true, ..Default::default() };
        let mut list = PickList::merging(vec![stack(9, 16395, 3), stack(4, 16393, 1), stack(2, 16395, 2), stack(7, 16393, 1)]);
        // By item, then by index; the rows sit under the hint.
        assert_eq!(list.items.iter().map(|item| item.index).collect::<Vec<_>>(), [4, 7, 2, 9]);
        let [x, y, ..] = list.row_boxes()[2].1;
        assert_eq!(list.hit([x + 40.0, y + 10.0]), Some(Hit::Row(2)));
        // One picked isn't enough; another item is refused; a second click unpicks.
        assert_eq!(list.toggle(3), Ok(()));
        assert_eq!(list.merge_request(), Err(MERGE_HINT));
        assert_eq!(list.toggle(0), Err(ONE_KIND));
        assert_eq!(list.picked_indexes(), [9]);
        assert_eq!(list.toggle(2), Ok(()));
        assert_eq!(list.merge_request(), Ok(vec![9, 2]));
        assert_eq!(list.toggle(3), Ok(()));
        assert_eq!(list.picked_indexes(), [2]);
        let [x, y, ..] = list.button_boxes()[0].1;
        assert_eq!(list.hit([x + 5.0, y + 5.0]), Some(Hit::Ok));
    }
}
