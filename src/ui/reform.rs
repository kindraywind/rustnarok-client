//! The item reform window: a box or an NPC names a reform item, the player picks a carried base item, and Reform
//! asks the server to turn it into the result. The 440×260 picture is the window: the made item and the base item on
//! the cube's two circles, six material beds under it, the list of carried items on the right, the footer's notice
//! line and buttons.

use std::collections::HashMap;

use rustnarok_client::grf::Grf;
use rustnarok_client::lua::{ItemInfo, ReformInfo};
use rustnarok_client::net::InventoryItem;
use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};
use crate::window::{self, Chrome, Frame, TitleHit};
use crate::{Layer, UI, contains, inventory, iteminfo, offset, skill_descript};

pub const SIZE: [f32; 2] = [440.0, 260.0];
const BASE_BUTTON: [f32; 2] = [3.0, 3.0];
const LIST: [f32; 2] = [215.0, 21.0];
const LIST_HEIGHT: f32 = 192.0;
const ROW: [f32; 2] = [207.0, 32.0];
const VISIBLE: usize = 6;
const BAR_X: f32 = 209.0;
const ICON: f32 = 24.0;
const RESULT_AT: [f32; 2] = [54.0, 50.0];
const BASE_AT: [f32; 2] = [144.0, 50.0];
/// The made item's name, centred in (x, width) at this height.
const NAME: [f32; 2] = [6.0, 209.0];
const NAME_Y: f32 = 104.0;
const LINK: [f32; 2] = [36.0, 124.0];
const LINK_SIZE: [f32; 2] = [149.0, 14.0];
const NOTICE: [f32; 2] = [8.0, 240.0];
const NOTICE_COLOR: [u8; 3] = [255, 10, 10];
const BUTTON_WIDTH: f32 = 60.0;
const BUTTON_HEIGHT: f32 = 20.0;
const CANCEL_AT: [f32; 2] = [300.0, 236.0];
const REFORM_AT: [f32; 2] = [365.0, 236.0];
const COUNT_GREY: [u8; 3] = [60, 60, 60];
const COUNT_SHORT: [u8; 3] = [200, 30, 50];
/// The details popup: a white rounded panel, a grey rounded fill 2 px in, a pale fill 8 px in, the lines on it.
const POPUP_FILLS: [[u8; 4]; 3] = [[255, 255, 255, 255], [200, 200, 200, 255], [247, 247, 247, 255]];
const POPUP_PAD: f32 = 12.0;

/// The world effect a done reform plays on the player.
pub const EFFECT: u32 = 1608;
const CANCEL: usize = 218;
/// "Reform": the button.
const REFORM: usize = 3847;
/// "There are not enough materials for the Combination."
pub const SHORT: usize = 3848;
/// "Check Details": the link.
const DETAILS: usize = 3854;
/// "Please check the details."
pub const CHECK: usize = 3855;
/// "There are no items that can be combined."
pub const NOTHING: usize = 3856;

pub struct Assets {
    background: Texture,
    /// A list row: idle, hovered, picked.
    row: [Texture; 3],
    popup: [Texture; 3],
    underline: Texture,
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |name: &str| gpu.load_texture(grf, &format!("{UI}itemreform\\{name}.bmp"), false);
        Self {
            background: image("bg_reform"),
            row: [image("btn_reform_item"), image("btn_reform_item_over"), image("btn_reform_item_press")],
            popup: POPUP_FILLS.map(|fill| gpu.solid(fill)),
            underline: gpu.solid([0, 0, 0, 255]),
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(TitleHit),
    /// A listed item, by inventory index.
    Row(u16),
    Bar,
    Result,
    Base,
    Material(usize),
    Link,
    Cancel,
    Reform,
    Window,
}

#[derive(Debug, PartialEq)]
pub enum Action {
    /// Reform the item at this inventory index with the reform item the window was opened for.
    Reform {
        item: u32,
        index: u16,
    },
    Close,
}

pub struct Reform {
    pub frame: Frame,
    /// The reform item (the box, or what the NPC named).
    pub reform_item: u32,
    pub entries: Vec<ReformInfo>,
    /// The picked inventory index.
    pub picked: Option<u16>,
    scroll: usize,
    thumb: window::Thumb,
}

impl Reform {
    /// The window, centred.
    pub fn open(item: u32, entries: Vec<ReformInfo>) -> Self {
        let screen = crate::screen();
        let mut frame = Frame::new([((screen[0] - SIZE[0]) / 2.0).floor(), ((screen[1] - SIZE[1]) / 2.0).floor()], SIZE[0]);
        frame.open = true;
        Self { frame, reform_item: item, entries, picked: None, scroll: 0, thumb: window::Thumb::default() }
    }

    /// The title: the reform item's name.
    pub fn title<'a>(&self, items: &'a HashMap<u32, ItemInfo>) -> &'a str {
        items.get(&self.reform_item).map_or("", |info| info.name(true))
    }

    pub fn scroll_by(&mut self, rows: i32, carried: &[InventoryItem]) {
        let most = listed(&self.entries, carried).len().saturating_sub(VISIBLE);
        self.scroll = (self.scroll as i32 + rows).clamp(0, most as i32) as usize;
    }

    fn scroll(&self, count: usize) -> usize {
        self.scroll.min(count.saturating_sub(VISIBLE))
    }

    /// The list's scroll bar, at its right; shown only while more than six rows are listed.
    fn bar(&self, carried: &[InventoryItem]) -> window::ScrollBar {
        let rows = listed(&self.entries, carried).len();
        window::ScrollBar { at: self.frame.at([LIST[0] + BAR_X, LIST[1]]), height: LIST_HEIGHT, shown: VISIBLE, rows, first: self.scroll(rows) }
    }

    /// Listed rows on screen: the item and its top inside the window.
    fn placed<'a>(&self, carried: &'a [InventoryItem]) -> Vec<(&'a InventoryItem, f32)> {
        let rows = listed(&self.entries, carried);
        let scroll = self.scroll(rows.len());
        rows.into_iter().skip(scroll).take(VISIBLE).enumerate().map(|(row, item)| (item, LIST[1] + row as f32 * ROW[1])).collect()
    }

    /// The picked item and its entry, while it is still carried.
    fn chosen<'a>(&'a self, carried: &'a [InventoryItem]) -> Option<(&'a InventoryItem, &'a ReformInfo)> {
        let item = listed(&self.entries, carried).into_iter().find(|item| Some(item.index) == self.picked)?;
        Some((item, entry_for(&self.entries, item.id)?))
    }

    pub fn hit(&self, cursor: [f32; 2], carried: &[InventoryItem]) -> Option<Hit> {
        let local = self.frame.local(cursor, SIZE[1])?;
        // The picture's bar has the base and close buttons only: where others have minimize, it is plain title bar.
        match self.frame.title_hit(local) {
            Some(TitleHit::Minimize) => return Some(Hit::Title(TitleHit::Title)),
            Some(hit) => return Some(Hit::Title(hit)),
            None => {}
        }
        if self.bar(carried).hit(cursor).is_some() {
            return Some(Hit::Bar);
        }
        if let Some((item, _)) = self.placed(carried).into_iter().find(|(_, top)| contains([LIST[0], *top], ROW, local)) {
            return Some(Hit::Row(item.index));
        }
        let button = |at: [f32; 2]| contains(at, [BUTTON_WIDTH, BUTTON_HEIGHT], local);
        if button(CANCEL_AT) {
            return Some(Hit::Cancel);
        }
        if button(REFORM_AT) {
            return Some(Hit::Reform);
        }
        let Some((_, entry)) = self.chosen(carried) else { return Some(Hit::Window) };
        if contains(RESULT_AT, [ICON; 2], local) {
            return Some(Hit::Result);
        }
        if contains(BASE_AT, [ICON; 2], local) {
            return Some(Hit::Base);
        }
        if let Some(slot) = (0..entry.materials.len().min(6)).find(|&slot| contains(material_at(slot), [ICON; 2], local)) {
            return Some(Hit::Material(slot));
        }
        if !entry.information.is_empty() && contains(LINK, LINK_SIZE, local) {
            return Some(Hit::Link);
        }
        Some(Hit::Window)
    }

    pub fn click(&mut self, hit: Hit, carried: &[InventoryItem]) -> Option<Action> {
        match hit {
            Hit::Title(TitleHit::Close) | Hit::Cancel => Some(Action::Close),
            Hit::Title(title) => {
                self.frame.click_title(title);
                None
            }
            Hit::Row(index) => {
                self.picked = Some(index);
                None
            }
            Hit::Reform if self.ready(carried) => Some(Action::Reform { item: self.reform_item, index: self.picked? }),
            _ => None,
        }
    }

    /// A press: the scroll bar takes it, else anywhere off the list and the buttons grabs the window.
    pub fn press(&mut self, cursor: [f32; 2], carried: &[InventoryItem]) {
        let bar = self.bar(carried);
        if let Some(first) = bar.press(cursor, &mut self.thumb) {
            self.scroll = first;
            return;
        }
        if matches!(self.hit(cursor, carried), Some(Hit::Title(TitleHit::Title) | Hit::Window | Hit::Result | Hit::Base | Hit::Material(_))) {
            self.frame.start_drag(cursor);
        }
    }

    /// The pointer moved to height `y`: a held thumb takes the list along.
    pub fn drag_thumb(&mut self, y: f32) {
        if let Some(first) = self.thumb.first(y) {
            self.scroll = first;
        }
    }

    pub fn drop_thumb(&mut self) -> bool {
        self.thumb.stop()
    }

    /// The item under the pointer, for its description: the made item, the base item or a material.
    pub fn shown_at(&self, cursor: [f32; 2], carried: &[InventoryItem], items: &HashMap<u32, ItemInfo>) -> Option<iteminfo::Shown> {
        let (item, entry) = self.chosen(carried)?;
        match self.hit(cursor, carried)? {
            Hit::Result => {
                let cards = if entry.preserve_socket { item.cards } else { [0; 4] };
                Some(iteminfo::Shown::of_link(entry.result, made_refine(item, entry), cards, false, items))
            }
            Hit::Base => Some(iteminfo::Shown::of(item)),
            Hit::Material(slot) => entry.materials.get(slot).map(|&(id, _)| iteminfo::Shown::of_link(id, 0, [0; 4], false, items)),
            _ => None,
        }
    }

    /// The notice line: nothing to list, else for the picked entry the details hint or a short material.
    pub fn notice(&self, carried: &[InventoryItem]) -> Option<usize> {
        if listed(&self.entries, carried).is_empty() {
            return Some(NOTHING);
        }
        let (_, entry) = self.chosen(carried)?;
        if !entry.information.is_empty() { Some(CHECK) } else { entry.materials.iter().any(|&(id, need)| carried_of(carried, id) < need).then_some(SHORT) }
    }

    /// Rows on screen, as `pick_item` with the base item's id.
    pub fn row_boxes(&self, carried: &[InventoryItem]) -> Vec<(u32, [f32; 4])> {
        if !self.frame.open {
            return Vec::new();
        }
        self.placed(carried)
            .into_iter()
            .map(|(item, top)| {
                let [x, y] = self.frame.at([LIST[0], top]);
                (item.id, [x, y, ROW[0], ROW[1]])
            })
            .collect()
    }

    /// `reform`, `cancel` and the title's `close`.
    pub fn button_boxes(&self) -> Vec<(&'static str, [f32; 4])> {
        if !self.frame.open {
            return Vec::new();
        }
        let place = |local: [f32; 2], size: [f32; 2]| {
            let [x, y] = self.frame.at(local);
            [x, y, size[0], size[1]]
        };
        vec![
            ("close", place(self.frame.close_button(), window::BUTTON_SIZE)),
            ("cancel", place(CANCEL_AT, [BUTTON_WIDTH, BUTTON_HEIGHT])),
            ("reform", place(REFORM_AT, [BUTTON_WIDTH, BUTTON_HEIGHT])),
        ]
    }

    pub fn icons(&self, items: &HashMap<u32, ItemInfo>) -> Vec<String> {
        let mut ids: Vec<u32> = self
            .entries
            .iter()
            .flat_map(|entry| std::iter::once(entry.base).chain(std::iter::once(entry.result)).chain(entry.materials.iter().map(|(id, _)| *id)))
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids.into_iter().filter_map(|id| items.get(&id).map(|info| inventory::icon_path(info, true))).collect()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &self,
        layer: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        icons: &'a HashMap<String, Option<Texture>>,
        items: &HashMap<u32, ItemInfo>,
        carried: &[InventoryItem],
        messages: &[String],
        font: &Font,
        pointer: ([f32; 2], bool),
    ) {
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        let at = |local: [f32; 2]| self.frame.at(local);
        let icon = |id: u32| items.get(&id).and_then(|info| icons.get(&inventory::icon_path(info, true))).and_then(Option::as_ref);
        layer.sprites.push((&assets.background, at([0.0, 0.0]), SIZE));
        // The picture's bar: the base and close buttons and the reform item's name.
        let base = chrome.button(0, contains(at(BASE_BUTTON), window::BUTTON_SIZE, pointer.0));
        layer.sprites.push((base, at(BASE_BUTTON), base.size));
        self.frame.draw_bare_title(layer, chrome, self.title(items), pointer.0);
        let hit = self.hit(pointer.0, carried);
        // The list: the row's picture (picked wins over hovered), the icon and the carried item's name.
        for (item, top) in self.placed(carried) {
            let look = match hit {
                _ if self.picked == Some(item.index) => 2,
                Some(Hit::Row(index)) if index == item.index => 1,
                _ => 0,
            };
            layer.sprites.push((&assets.row[look], at([LIST[0], top]), ROW));
            if let Some(icon) = icon(item.id) {
                layer.sprites.push((icon, at([LIST[0] + 15.0, top + 4.0]), icon.size));
            }
            let name = inventory::item_name(items, item.id, true, item.refine);
            layer.texts.push((window::fit(font, &name, 160.0), at([LIST[0] + 42.0, top + 10.0]), [0; 3]));
        }
        self.bar(carried).draw(layer, chrome);
        // The picked entry: the made item and the base on the circles, the made item's name, the details link, and
        // the materials on their beds with "have/need" under each.
        if let Some((item, entry)) = self.chosen(carried) {
            for (id, place) in [(entry.result, RESULT_AT), (item.id, BASE_AT)] {
                if let Some(icon) = icon(id) {
                    layer.sprites.push((icon, at(place), [ICON; 2]));
                }
            }
            let name = window::fit(font, &inventory::item_name(items, entry.result, true, made_refine(item, entry)), 200.0);
            layer.texts.push((name.clone(), at([NAME[0] + ((NAME[1] - font.width(&name)) / 2.0).floor(), NAME_Y]), [0; 3]));
            if !entry.information.is_empty() {
                let link = message(DETAILS);
                let width = font.width(link);
                let x = LINK[0] + ((LINK_SIZE[0] - width) / 2.0).floor();
                layer.texts.push((link.to_owned(), at([x, LINK[1]]), [0; 3]));
                layer.sprites.push((&assets.underline, at([x, LINK[1] + 12.0]), [width, 1.0]));
            }
            for (slot, &(id, need)) in entry.materials.iter().take(6).enumerate() {
                let place = material_at(slot);
                if let Some(icon) = icon(id) {
                    layer.sprites.push((icon, at(place), [ICON; 2]));
                }
                let [have, slash, needed] = count_parts(carried_of(carried, id), need);
                let middle = place[0] + 12.0;
                let slash_x = middle - (font.width(&slash.0) / 2.0).floor();
                let y = place[1] + 27.0;
                layer.texts.push((have.0.clone(), at([slash_x - font.width(&have.0), y]), have.1));
                layer.texts.push((needed.0, at([slash_x + font.width(&slash.0), y]), needed.1));
                layer.texts.push((slash.0, at([slash_x, y]), slash.1));
            }
        }
        if let Some(id) = self.notice(carried) {
            layer.texts.push((window::fit(font, message(id), 280.0), at(NOTICE), NOTICE_COLOR));
        }
        let look = |target: Hit, enabled: bool| match () {
            _ if !enabled => 3,
            _ if hit == Some(target) => 1 + usize::from(pointer.1),
            _ => 0,
        };
        chrome.draw_text_button(layer, font, at(CANCEL_AT), BUTTON_WIDTH, message(CANCEL), look(Hit::Cancel, true));
        chrome.draw_text_button(layer, font, at(REFORM_AT), BUTTON_WIDTH, message(REFORM), look(Hit::Reform, self.ready(carried)));
        // The made item, the base or a material under the pointer: its name beside it, over the window's text.
        if let Some(shown) = self.shown_at(pointer.0, carried, items) {
            layer.window();
            let name = inventory::item_name(items, shown.item, true, shown.refine);
            window::tooltip(layer, chrome, font, &name, offset(pointer.0, [5.0, -5.0]), [255; 3]);
        }
        // The link under the pointer: the entry's details in a popup that follows it.
        if hit == Some(Hit::Link)
            && let Some((_, entry)) = self.chosen(carried)
        {
            layer.window();
            draw_popup(layer, assets, font, &entry.information, offset(pointer.0, [5.0, 5.0]));
        }
    }

    fn ready(&self, carried: &[InventoryItem]) -> bool {
        self.chosen(carried).is_some_and(|(item, entry)| can_reform(entry, item, carried))
    }
}

/// The details popup at `corner`: sized to its lines, which start on the pale fill.
// ponytail: the popup's size and how its lines are set aren't settled; the lines' <B> marks are dropped (one weight).
fn draw_popup<'a>(layer: &mut Layer<'a>, assets: &'a Assets, font: &Font, information: &[String], corner: [f32; 2]) {
    let lines: Vec<String> = information.iter().map(|line| line.replace("<B>", "").replace("</B>", "")).collect();
    let lines = skill_descript::colored(&lines);
    let width = lines.iter().map(|line| line.iter().map(|(_, run)| font.width(run)).sum::<f32>()).fold(0.0, f32::max);
    let size = [width.ceil() + 2.0 * POPUP_PAD, lines.len() as f32 * font.line_height() + 2.0 * POPUP_PAD];
    let corner = window::tooltip_corner(corner, size, crate::screen());
    layer.sprites.push((&assets.popup[0], corner, size));
    for (fill, inset) in [(&assets.popup[1], 2.0), (&assets.popup[2], 8.0)] {
        for [x, y, w, h] in window::rounded_fill([size[0] - 2.0 * inset, size[1] - 2.0 * inset]) {
            layer.sprites.push((fill, offset(corner, [inset + x, inset + y]), [w, h]));
        }
    }
    for (row, line) in lines.iter().enumerate() {
        let mut x = POPUP_PAD;
        for (color, run) in line {
            layer.texts.push((run.clone(), offset(corner, [x, POPUP_PAD + row as f32 * font.line_height()]), *color));
            x += font.width(run);
        }
    }
}

/// Material bed `slot`: three to a row, 69 px apart, the second row 50 px down.
fn material_at(slot: usize) -> [f32; 2] {
    [30.0 + 69.0 * (slot % 3) as f32, 140.0 + 50.0 * (slot / 3) as f32]
}

/// A material's count, as its three parts: the carried number, "/", the needed number; the numbers grey, red when
/// too few are carried, the "/" black.
fn count_parts(have: u32, need: u32) -> [(String, [u8; 3]); 3] {
    let color = if have < need { COUNT_SHORT } else { COUNT_GREY };
    [(have.to_string(), color), ("/".to_owned(), [0; 3]), (need.to_string(), color)]
}

/// The made item's refine: the base's plus the entry's change.
fn made_refine(item: &InventoryItem, entry: &ReformInfo) -> u8 {
    (i32::from(item.refine) + entry.change_refine).clamp(0, i32::from(u8::MAX)) as u8
}

/// Carried items this reform item can turn into something else: identified, not worn, one row each.
pub fn listed<'a>(entries: &[ReformInfo], carried: &'a [InventoryItem]) -> Vec<&'a InventoryItem> {
    carried.iter().filter(|item| item.identified && item.worn == 0 && entry_for(entries, item.id).is_some()).collect()
}

pub fn entry_for(entries: &[ReformInfo], item: u32) -> Option<&ReformInfo> {
    entries.iter().find(|entry| entry.base == item)
}

/// How many of `id` are carried, stacks added together.
pub fn carried_of(carried: &[InventoryItem], id: u32) -> u32 {
    carried.iter().filter(|item| item.id == id).map(|item| u32::from(item.amount)).sum()
}

/// Refine inside the range, no cards when the sockets must be empty, and every material carried.
/// Random options are not tracked, so `need_options` is not checked.
pub fn can_reform(entry: &ReformInfo, item: &InventoryItem, carried: &[InventoryItem]) -> bool {
    let refine = u16::from(item.refine);
    if refine < entry.refine_min || refine > entry.refine_max {
        return false;
    }
    if entry.empty_socket && item.cards.iter().any(|card| *card != 0) {
        return false;
    }
    entry.materials.iter().all(|&(id, need)| carried_of(carried, id) >= need)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use rustnarok_client::lua::{self, ItemInfo, ReformInfo};
    use rustnarok_client::net::InventoryItem;

    use super::*;

    fn names_and_list() -> HashMap<u32, Vec<ReformInfo>> {
        let names = lua::item_db_names(b"ItemDBNameTbl = { Box = 100, Sword = 200, Shard = 300, Stone = 301, Result = 400 }".to_vec()).unwrap();
        lua::reform_table(
            br#"
            ReformItemList = { Box = { 1, 2 } }
            ReformInfo = {
                [1] = {
                    BaseItem = "Sword", ResultItem = "Result",
                    Material = { Shard = 100, Stone = 1 },
                    NeedRefineMin = 9, NeedRefineMax = 20,
                    IsEmptySocket = true, PreserveSocketItem = false
                },
                [2] = {
                    BaseItem = "Stone", ResultItem = "Result", NeedRefineMin = 0, NeedRefineMax = 20, IsEmptySocket = false,
                    InformationString = { "<B>About</B>", "1) Refine: ^4D4DFF+1^000000" }
                }
            }
            "#
            .to_vec(),
            &names,
        )
        .unwrap()
    }

    fn item(index: u16, id: u32, refine: u8, cards: [u32; 4], amount: u16, worn: u32, identified: bool) -> InventoryItem {
        InventoryItem { index, id, refine, cards, amount, worn, identified, ..Default::default() }
    }

    fn on(window: &Reform, local: [f32; 2]) -> [f32; 2] {
        window.frame.at(local)
    }

    #[test]
    fn lists_only_identified_unworn_base_items() {
        let reforms = names_and_list();
        let entries = &reforms[&100];
        let carried = vec![
            item(1, 200, 9, [0; 4], 1, 0, true),
            item(2, 200, 9, [0; 4], 1, 2, true),
            item(3, 200, 9, [0; 4], 1, 0, false),
            item(4, 999, 9, [0; 4], 1, 0, true),
            item(5, 301, 0, [0; 4], 1, 0, true),
        ];
        assert_eq!(listed(entries, &carried).iter().map(|item| item.index).collect::<Vec<_>>(), [1, 5]);
        assert_eq!(entries[1].information, ["<B>About</B>", "1) Refine: ^4D4DFF+1^000000"]);
        assert!(entries[0].information.is_empty());
    }

    #[test]
    fn the_pieces_sit_on_the_picture() {
        let reforms = names_and_list();
        let mut window = Reform::open(100, reforms[&100].clone());
        let screen = crate::screen();
        assert_eq!(window.frame.position, [((screen[0] - 440.0) / 2.0).floor(), ((screen[1] - 260.0) / 2.0).floor()]);
        let items = HashMap::from([(100, ItemInfo { name: "Thanos Upgrade Box".into(), ..ItemInfo::default() })]);
        assert_eq!(window.title(&items), "Thanos Upgrade Box");
        let carried = vec![item(1, 200, 9, [0; 4], 1, 0, true), item(2, 300, 0, [0; 4], 100, 0, true), item(5, 301, 0, [0; 4], 1, 0, true)];
        let hit = |window: &Reform, local: [f32; 2]| window.hit(on(window, local), &carried);
        // Rows from (215, 21), 207 × 32.
        assert_eq!(hit(&window, [215.0, 21.0]), Some(Hit::Row(1)));
        assert_eq!(hit(&window, [421.0, 52.9]), Some(Hit::Row(1)));
        assert_eq!(hit(&window, [215.0, 53.0]), Some(Hit::Row(5)));
        assert_eq!(hit(&window, [215.0, 85.0]), Some(Hit::Window));
        assert_eq!(window.row_boxes(&carried)[1].1, [on(&window, [215.0, 53.0])[0], on(&window, [215.0, 53.0])[1], 207.0, 32.0]);
        assert_eq!((hit(&window, [300.0, 236.0]), hit(&window, [359.0, 255.0])), (Some(Hit::Cancel), Some(Hit::Cancel)));
        assert_eq!((hit(&window, [365.0, 236.0]), hit(&window, [424.0, 255.0])), (Some(Hit::Reform), Some(Hit::Reform)));
        assert_eq!(hit(&window, [377.0, 5.0]), Some(Hit::Title(TitleHit::Title)));
        assert_eq!(hit(&window, [427.0, 5.0]), Some(Hit::Title(TitleHit::Close)));
        // Nothing picked: the circles and beds are plain window.
        assert_eq!(hit(&window, [54.0, 50.0]), Some(Hit::Window));
        window.click(Hit::Row(1), &carried);
        assert_eq!((hit(&window, [54.0, 50.0]), hit(&window, [77.0, 73.0])), (Some(Hit::Result), Some(Hit::Result)));
        assert_eq!(hit(&window, [144.0, 50.0]), Some(Hit::Base));
        assert_eq!((material_at(0), material_at(2), material_at(3), material_at(5)), ([30.0, 140.0], [168.0, 140.0], [30.0, 190.0], [168.0, 190.0]));
        assert_eq!((hit(&window, [30.0, 140.0]), hit(&window, [99.0, 140.0])), (Some(Hit::Material(0)), Some(Hit::Material(1))));
        assert_eq!(hit(&window, [168.0, 140.0]), Some(Hit::Window));
        // The link only for an entry with details.
        assert_eq!(hit(&window, [40.0, 126.0]), Some(Hit::Window));
        window.click(Hit::Row(5), &carried);
        assert_eq!(hit(&window, [40.0, 126.0]), Some(Hit::Link));
        let names: Vec<_> = window.button_boxes().into_iter().map(|(name, _)| name).collect();
        assert_eq!(names, ["close", "cancel", "reform"]);
    }

    #[test]
    fn counts_notices_and_the_button() {
        assert_eq!(count_parts(100, 100), [("100".into(), COUNT_GREY), ("/".into(), [0; 3]), ("100".into(), COUNT_GREY)]);
        assert_eq!(count_parts(40, 100), [("40".into(), COUNT_SHORT), ("/".into(), [0; 3]), ("100".into(), COUNT_SHORT)]);
        let reforms = names_and_list();
        let mut window = Reform::open(100, reforms[&100].clone());
        assert_eq!(window.notice(&[]), Some(NOTHING));
        let carried = |shards| vec![item(1, 200, 9, [0; 4], 1, 0, true), item(2, 300, 0, [0; 4], shards, 0, true), item(5, 301, 0, [0; 4], 1, 0, true)];
        assert_eq!(window.notice(&carried(100)), None);
        assert!(!window.ready(&carried(100)));
        window.click(Hit::Row(1), &carried(100));
        assert_eq!(window.notice(&carried(100)), None);
        assert!(window.ready(&carried(100)));
        assert_eq!(window.notice(&carried(99)), Some(SHORT));
        assert!(!window.ready(&carried(99)));
        // An entry with details says so, short or not.
        window.click(Hit::Row(5), &carried(99));
        assert_eq!(window.notice(&carried(99)), Some(CHECK));
        assert!(window.ready(&carried(99)));
        assert_eq!(window.click(Hit::Reform, &carried(99)), Some(Action::Reform { item: 100, index: 5 }));
        assert_eq!(window.click(Hit::Cancel, &carried(99)), Some(Action::Close));
        assert_eq!(window.click(Hit::Title(TitleHit::Close), &carried(99)), Some(Action::Close));
    }

    #[test]
    fn reform_button_follows_refine_cards_and_materials() {
        let reforms = names_and_list();
        let entries = &reforms[&100];
        let sword = entries.iter().find(|entry| entry.base == 200).unwrap();
        let carried = |refine, cards, shards| {
            vec![
                item(1, 200, refine, cards, 1, 0, true),
                item(2, 300, 0, [0; 4], shards, 0, true),
                item(3, 300, 0, [0; 4], 40, 0, true),
                item(4, 301, 0, [0; 4], 1, 0, true),
            ]
        };
        // 40 + 60 of shard 300 meets 100; 40 + 50 does not. Stone 301 is its own stack of 1.
        assert!(can_reform(sword, &carried(9, [0; 4], 60)[0], &carried(9, [0; 4], 60)));
        assert!(can_reform(sword, &carried(20, [0; 4], 60)[0], &carried(20, [0; 4], 60)));
        assert!(!can_reform(sword, &carried(8, [0; 4], 60)[0], &carried(8, [0; 4], 60)));
        assert!(!can_reform(sword, &carried(21, [0; 4], 60)[0], &carried(21, [0; 4], 60)));
        assert!(!can_reform(sword, &carried(9, [4001, 0, 0, 0], 60)[0], &carried(9, [4001, 0, 0, 0], 60)));
        assert!(!can_reform(sword, &carried(9, [0; 4], 50)[0], &carried(9, [0; 4], 50)));
        // Sockets may hold cards on the other entry.
        let stone = entries.iter().find(|entry| entry.base == 301).unwrap();
        let with_card = item(5, 301, 0, [7, 0, 0, 0], 1, 0, true);
        assert!(can_reform(stone, &with_card, std::slice::from_ref(&with_card)));

        let mut window = Reform::open(100, entries.clone());
        let have = carried(9, [0; 4], 60);
        assert_eq!(window.click(Hit::Reform, &have), None);
        assert_eq!(window.click(Hit::Row(1), &have), None);
        assert_eq!(window.click(Hit::Reform, &have), Some(Action::Reform { item: 100, index: 1 }));
        let short = carried(9, [0; 4], 50);
        window.picked = Some(1);
        assert_eq!(window.click(Hit::Reform, &short), None);
        assert_eq!(made_refine(&have[0], sword), 9);
        assert_eq!(made_refine(&have[0], &ReformInfo { change_refine: -12, ..sword.clone() }), 0);
    }
}
