//! The item reform window: a box or an NPC names a reform item, the player picks a carried base item, and Reform
//! asks the server to turn it into the result.

use std::collections::HashMap;

use rustnarok_client::grf::Grf;
use rustnarok_client::lua::{ItemInfo, ReformInfo};
use rustnarok_client::net::InventoryItem;
use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};
use crate::window::{self, BUTTON_SIZE, Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI, contains, inventory, offset};

// ponytail: where the rows, the detail and the button sit over bg_reform.bmp, and the text colors, aren't knowable.
const WIDTH: f32 = 440.0;
const BG: [f32; 2] = [440.0, 260.0];
pub const HEIGHT: f32 = TITLE_HEIGHT + BG[1];
const POSITION: [f32; 2] = [290.0, 140.0];
const ROW_LEFT: f32 = 12.0;
const ROW_TOP: f32 = TITLE_HEIGHT + 8.0;
const ROW_SIZE: [f32; 2] = [207.0, 32.0];
const VISIBLE: usize = 6;
const DETAIL: [f32; 2] = [232.0, TITLE_HEIGHT + 8.0];
const DETAIL_SIZE: [f32; 2] = [196.0, 200.0];
const BUTTON: [f32; 2] = [96.0, 24.0];
const REFORM_BUTTON: [f32; 2] = [WIDTH - 16.0 - BUTTON[0], TITLE_HEIGHT + BG[1] - 40.0];
const TEXT: [u8; 3] = [0, 0, 0];
const GREY: [u8; 3] = [0x90, 0x90, 0x90];
/// "Reform", also the window's title.
const TITLE: usize = 3847;
/// "Equipped Cards and Enchantments will be lost."
const LOST: usize = 3849;
/// "Cards, Random Options and Enchantments are maintained."
const KEPT: usize = 3871;
/// "A card is inserted or it's enchanted."
const CARDED: usize = 3646;

pub struct Assets {
    background: Texture,
    /// A list row: idle, hovered, pressed.
    row: [Texture; 3],
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |name: &str| gpu.load_texture(grf, &format!("{UI}itemreform\\{name}.bmp"), false);
        Self { background: image("bg_reform"), row: [image("btn_reform_item"), image("btn_reform_item_over"), image("btn_reform_item_press")] }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(TitleHit),
    /// A listed item, by inventory index.
    Row(u16),
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
}

impl Reform {
    pub fn open(item: u32, entries: Vec<ReformInfo>) -> Self {
        let mut frame = Frame::new(POSITION, WIDTH);
        frame.open = true;
        Self { frame, reform_item: item, entries, picked: None, scroll: 0 }
    }

    pub fn scroll_by(&mut self, rows: i32, carried: &[InventoryItem]) {
        let most = listed(&self.entries, carried).len().saturating_sub(VISIBLE);
        self.scroll = (self.scroll as i32 + rows).clamp(0, most as i32) as usize;
    }

    fn scroll(&self, count: usize) -> usize {
        self.scroll.min(count.saturating_sub(VISIBLE))
    }

    /// Listed rows on screen: the item and its top inside the window.
    fn placed<'a>(&self, carried: &'a [InventoryItem]) -> Vec<(&'a InventoryItem, f32)> {
        let rows = listed(&self.entries, carried);
        let scroll = self.scroll(rows.len());
        rows.into_iter().skip(scroll).take(VISIBLE).enumerate().map(|(row, item)| (item, ROW_TOP + row as f32 * ROW_SIZE[1])).collect()
    }

    pub fn hit(&self, cursor: [f32; 2], carried: &[InventoryItem]) -> Option<Hit> {
        let local = self.frame.local(cursor, HEIGHT)?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(Hit::Title(hit));
        }
        if self.frame.minimized {
            return Some(Hit::Window);
        }
        for (item, top) in self.placed(carried) {
            if contains([ROW_LEFT, top], ROW_SIZE, local) {
                return Some(Hit::Row(item.index));
            }
        }
        if contains(REFORM_BUTTON, BUTTON, local) {
            return Some(Hit::Reform);
        }
        Some(Hit::Window)
    }

    pub fn click(&mut self, hit: Hit, carried: &[InventoryItem]) -> Option<Action> {
        match hit {
            Hit::Title(TitleHit::Close) => Some(Action::Close),
            Hit::Title(title) => {
                self.frame.click_title(title);
                None
            }
            Hit::Row(index) => {
                self.picked = Some(index);
                None
            }
            Hit::Reform => {
                let index = self.picked?;
                let item = listed(&self.entries, carried).into_iter().find(|item| item.index == index)?;
                let entry = entry_for(&self.entries, item.id)?;
                can_reform(entry, item, carried).then_some(Action::Reform { item: self.reform_item, index })
            }
            Hit::Window => None,
        }
    }

    /// Rows on screen, as `pick_item` with the base item's id.
    pub fn row_boxes(&self, carried: &[InventoryItem]) -> Vec<(u32, [f32; 4])> {
        if !self.frame.open || self.frame.minimized {
            return Vec::new();
        }
        self.placed(carried)
            .into_iter()
            .map(|(item, top)| {
                let [x, y] = self.frame.at([ROW_LEFT, top]);
                (item.id, [x, y, ROW_SIZE[0], ROW_SIZE[1]])
            })
            .collect()
    }

    /// `reform` and the title's `close`.
    pub fn button_boxes(&self) -> Vec<(&'static str, [f32; 4])> {
        if !self.frame.open {
            return Vec::new();
        }
        let place = |local: [f32; 2], size: [f32; 2]| {
            let [x, y] = self.frame.at(local);
            [x, y, size[0], size[1]]
        };
        let mut boxes = vec![("close", place(self.frame.close_button(), BUTTON_SIZE))];
        if !self.frame.minimized {
            boxes.push(("reform", place(REFORM_BUTTON, BUTTON)));
        }
        boxes
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
        self.frame.draw_title(layer, chrome, message(TITLE), pointer.0);
        if self.frame.minimized {
            return;
        }
        layer.sprites.push((&assets.background, at([0.0, TITLE_HEIGHT]), BG));
        let hit = self.hit(pointer.0, carried);
        for (item, top) in self.placed(carried) {
            let entry = entry_for(&self.entries, item.id);
            let look = match hit {
                Some(Hit::Row(index)) if index == item.index && pointer.1 => 2,
                Some(Hit::Row(index)) if index == item.index => 1,
                _ if self.picked == Some(item.index) => 2,
                _ => 0,
            };
            layer.sprites.push((&assets.row[look], at([ROW_LEFT, top]), ROW_SIZE));
            if let Some(icon) = items.get(&item.id).and_then(|info| icons.get(&inventory::icon_path(info, true))).and_then(Option::as_ref) {
                layer.sprites.push((icon, at([ROW_LEFT + 4.0, top + 4.0]), icon.size));
            }
            let name = inventory::item_name(items, item.id, true, item.refine);
            layer.texts.push((window::fit(font, &name, 78.0), at([ROW_LEFT + 32.0, top + 8.0]), TEXT));
            if let Some(entry) = entry {
                let result = inventory::item_name(items, entry.result, true, 0);
                layer.texts.push((window::fit(font, &result, 84.0), at([ROW_LEFT + 116.0, top + 8.0]), TEXT));
            }
        }
        // A white panel so the lines stay readable over the background.
        layer.sprites.push((&chrome.white, at(DETAIL), DETAIL_SIZE));
        layer.sprites.push((&chrome.border, at(DETAIL), [1.0, DETAIL_SIZE[1]]));
        let mut y = DETAIL[1] + 6.0;
        let line = |layer: &mut Layer<'a>, text: String, y: f32, color: [u8; 3]| {
            layer.texts.push((text, at([DETAIL[0] + 8.0, y]), color));
        };
        if let Some(item) = self.picked.and_then(|index| carried.iter().find(|item| item.index == index))
            && let Some(entry) = entry_for(&self.entries, item.id)
        {
            let result = inventory::item_name(items, entry.result, true, 0);
            line(layer, result, y, TEXT);
            y += font.line_height();
            if entry.change_refine != 0 {
                line(layer, format!("{:+}", entry.change_refine), y, TEXT);
                y += font.line_height();
            }
            if entry.refine_min > 0 || entry.refine_max < 20 {
                line(layer, format!("+{} ~ +{}", entry.refine_min, entry.refine_max), y, TEXT);
                y += font.line_height();
            }
            for &(id, need) in &entry.materials {
                if let Some(icon) = items.get(&id).and_then(|info| icons.get(&inventory::icon_path(info, true))).and_then(Option::as_ref) {
                    layer.sprites.push((icon, at([DETAIL[0] + 8.0, y]), [24.0, 24.0]));
                }
                let name = inventory::item_name(items, id, true, 0);
                let have = carried_of(carried, id);
                let text = format!("{}  {have} / {need}", window::fit(font, &name, 100.0));
                layer.texts.push((text, at([DETAIL[0] + 36.0, y + 4.0]), if have >= need { TEXT } else { GREY }));
                y += 26.0;
            }
            // ponytail: which of these lines shows, and when, isn't knowable.
            let mut notes = vec![if entry.preserve_socket { KEPT } else { LOST }];
            if entry.empty_socket && item.cards.iter().any(|card| *card != 0) {
                notes.push(CARDED);
            }
            for id in notes {
                for wrapped in window::wrap(font, message(id), DETAIL_SIZE[0] - 16.0, 2) {
                    line(layer, wrapped, y, TEXT);
                    y += font.line_height();
                }
            }
        }
        let ready = self.ready(carried);
        layer.sprites.push((&chrome.border, at(REFORM_BUTTON), BUTTON));
        layer.sprites.push((&chrome.white, at(offset(REFORM_BUTTON, [1.0, 1.0])), [BUTTON[0] - 2.0, BUTTON[1] - 2.0]));
        let label = message(TITLE);
        let label_x = REFORM_BUTTON[0] + (BUTTON[0] - font.width(label)) / 2.0;
        layer.texts.push((label.to_owned(), at([label_x, REFORM_BUTTON[1] + 4.0]), if ready { TEXT } else { GREY }));
    }

    fn ready(&self, carried: &[InventoryItem]) -> bool {
        let Some(index) = self.picked else { return false };
        let Some(item) = listed(&self.entries, carried).into_iter().find(|item| item.index == index) else { return false };
        entry_for(&self.entries, item.id).is_some_and(|entry| can_reform(entry, item, carried))
    }
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

    use rustnarok_client::lua::{self, ReformInfo};
    use rustnarok_client::net::InventoryItem;

    use super::{Action, Hit, Reform, can_reform, listed};
    use crate::window::TitleHit;

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
                [2] = { BaseItem = "Stone", ResultItem = "Result", NeedRefineMin = 0, NeedRefineMax = 20, IsEmptySocket = false }
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
        assert_eq!(window.click(Hit::Title(TitleHit::Close), &have), Some(Action::Close));
        assert!(window.button_boxes().iter().any(|(name, _)| *name == "reform"));
        assert!(window.row_boxes(&have).iter().any(|(id, _)| *id == 200));
    }
}
