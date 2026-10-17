//! The refine window (2016+ refiners, `@refineui`): an item dropped on the machine, the ores it can be refined with
//! (chance, zeny), Blacksmith's Blessings, Refine and Back.

use std::collections::HashMap;

use rustnarok_client::grf::Grf;
use rustnarok_client::lua::ItemInfo;
use rustnarok_client::net::InventoryItem;
use rustnarok_client::text::Font;

use crate::inventory;
use crate::render::{Gpu, Texture};
use crate::shop::group;
use crate::window::{self, Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI, contains, offset};

// ponytail: the refining\ set (not refining_renewal\), stills only, and every place over its pictures is an estimate;
// calibrate against the real client.
const MACHINE: [f32; 2] = [256.0, 344.0];
const LIST_WIDTH: f32 = 199.0;
const WIDTH: f32 = MACHINE[0] + LIST_WIDTH;
const FOOTER: f32 = 60.0;
const HEIGHT: f32 = TITLE_HEIGHT + MACHINE[1] + FOOTER;
const POSITION: [f32; 2] = [280.0, 120.0];
/// The item's slot in the machine's middle, its name in the bar above, the picked ore in the lower left gear.
const ITEM_CENTER: [f32; 2] = [127.0, TITLE_HEIGHT + 116.0];
const NAME_CENTER: [f32; 2] = [127.0, TITLE_HEIGHT + 26.0];
const ORE_CENTER: [f32; 2] = [65.0, TITLE_HEIGHT + 225.0];
/// The ore rows down the list.
const ROWS: usize = 5;
const ROW_TOP: f32 = TITLE_HEIGHT + 12.0;
const ROW: f32 = 62.0;
const ROW_SIZE: [f32; 2] = [188.0, 60.0];
const ROW_LEFT: f32 = MACHINE[0] + 6.0;
const BLESSING: [f32; 2] = [10.0, TITLE_HEIGHT + MACHINE[1] + 6.0];
const BUTTON: [f32; 2] = [96.0, 54.0];
const BACK: [f32; 2] = [MACHINE[0] + 4.0, TITLE_HEIGHT + MACHINE[1] + 3.0];
const REFINE: [f32; 2] = [MACHINE[0] + 101.0, TITLE_HEIGHT + MACHINE[1] + 3.0];
const GREY: [u8; 3] = [0x90, 0x90, 0x90];
const TITLE: usize = 2966;
const BACK_LABEL: usize = 2973;
const BLESSING_LABEL: usize = 2967;
/// Blacksmith's Blessing.
pub const BLESSING_ITEM: u32 = 6635;
/// The client's own refusals: too little zeny, too few blessings.
const NO_ZENY: usize = 2968;
const NO_BLESSINGS: usize = 2969;

pub struct Assets {
    /// The machine, idle and lit (an item in it).
    machine: [Texture; 2],
    list: Texture,
    selected: Texture,
    /// Refine: on, off; Back.
    refine: [Texture; 2],
    back: Texture,
    checkbox: [Texture; 2],
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |name: &str| gpu.load_texture(grf, &format!("{UI}refining\\{name}.bmp"), false);
        Self {
            machine: [image("bg_refining"), image("bg_refining_on")],
            list: image("bg_list"),
            selected: image("slot_select_00"),
            refine: [image("bt_refining"), image("bt_refining_off")],
            back: image("bt_refining_back"),
            checkbox: ["0", "1"].map(|look| gpu.load_texture(grf, &format!("{UI}checkbox_{look}.bmp"), false)),
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(TitleHit),
    Ore(usize),
    Blessing,
    Back,
    Refine,
    Window,
}

#[derive(Debug, PartialEq)]
pub enum Action {
    /// Refine the item with this ore, with or without blessings.
    Refine {
        index: u16,
        ore: u32,
        blessings: bool,
    },
    /// The client's own refusal, a message number.
    Refuse(usize),
    Close,
}

pub struct Refine {
    pub frame: Frame,
    /// The item in the machine (inventory index), once the server lists its ores.
    pub item: Option<u16>,
    /// Ores: item ID, chance (percent), zeny.
    pub materials: Vec<(u32, u8, u32)>,
    /// How many Blacksmith's Blessings protect this attempt (0: none can).
    pub blessings: u8,
    pub use_blessings: bool,
    pub picked: Option<usize>,
}

impl Refine {
    pub fn new() -> Self {
        let mut frame = Frame::new(POSITION, WIDTH);
        frame.open = true;
        Self { frame, item: None, materials: Vec::new(), blessings: 0, use_blessings: false, picked: None }
    }

    /// The server's list for an item: its ores, and how many blessings it takes.
    pub fn listed(&mut self, index: u16, blessings: u8, materials: Vec<(u32, u8, u32)>) {
        // The same item again (after a refine) keeps the ore picked, if it's still offered.
        let ore = (self.item == Some(index)).then(|| self.picked.and_then(|row| self.materials.get(row)).map(|&(ore, ..)| ore)).flatten();
        self.picked = ore.and_then(|ore| materials.iter().position(|&(listed, ..)| listed == ore));
        (self.item, self.blessings, self.materials) = (Some(index), blessings, materials);
        self.use_blessings &= blessings > 0;
    }

    /// Back: the machine empties.
    pub fn clear(&mut self) {
        (self.item, self.picked, self.use_blessings) = (None, None, false);
        self.materials.clear();
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let local = self.frame.local(cursor, HEIGHT)?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(Hit::Title(hit));
        }
        let row = (0..ROWS.min(self.materials.len())).find(|&row| contains([ROW_LEFT, ROW_TOP + row as f32 * ROW], ROW_SIZE, local));
        Some(match row {
            Some(row) => Hit::Ore(row),
            None if contains(BLESSING, [200.0, 16.0], local) => Hit::Blessing,
            None if contains(BACK, BUTTON, local) => Hit::Back,
            None if contains(REFINE, BUTTON, local) => Hit::Refine,
            None => Hit::Window,
        })
    }

    /// Reacts to a click; `count` tells how many of an item are carried, `zeny` the zeny carried.
    pub fn click(&mut self, hit: Hit, count: impl Fn(u32) -> u32, zeny: u32) -> Option<Action> {
        match hit {
            Hit::Title(TitleHit::Close) => return Some(Action::Close),
            Hit::Title(title) => self.frame.click_title(title),
            // Ores not carried can't be picked.
            Hit::Ore(row) if count(self.materials[row].0) > 0 => self.picked = Some(row),
            Hit::Blessing if self.blessings > 0 => self.use_blessings = !self.use_blessings,
            Hit::Back => self.clear(),
            Hit::Refine => {
                let (index, &(ore, _, price)) = (self.item?, self.materials.get(self.picked?)?);
                // The ore picked may be used up (a refine leaves it picked).
                if count(ore) == 0 {
                    return None;
                }
                if zeny < price {
                    return Some(Action::Refuse(NO_ZENY));
                }
                if self.use_blessings && count(BLESSING_ITEM) < u32::from(self.blessings) {
                    return Some(Action::Refuse(NO_BLESSINGS));
                }
                return Some(Action::Refine { index, ore, blessings: self.use_blessings });
            }
            Hit::Ore(_) | Hit::Blessing | Hit::Window => {}
        }
        None
    }

    pub fn icons(&self, items: &HashMap<u32, ItemInfo>, carried: &[InventoryItem]) -> Vec<String> {
        let item = self.item.and_then(|index| carried.iter().find(|item| item.index == index)).map(|item| item.id);
        item.into_iter()
            .chain(self.materials.iter().map(|&(ore, ..)| ore))
            .filter_map(|id| items.get(&id).map(|info| inventory::icon_path(info, true)))
            .collect()
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
        let icon = |id: u32| items.get(&id).and_then(|info| icons.get(&inventory::icon_path(info, true))).and_then(Option::as_ref);
        let centered = |center: [f32; 2], size: [f32; 2]| at([center[0] - size[0] / 2.0, center[1] - size[1] / 2.0]);
        let count = |id: u32| carried.iter().filter(|item| item.id == id).map(|item| u32::from(item.amount)).sum::<u32>();
        let item = self.item.and_then(|index| carried.iter().find(|item| item.index == index));
        layer.sprites.push((&assets.machine[usize::from(item.is_some())], at([0.0, TITLE_HEIGHT]), MACHINE));
        layer.sprites.push((&assets.list, at([MACHINE[0], TITLE_HEIGHT]), assets.list.size));
        layer.sprites.push((&chrome.white, at([0.0, TITLE_HEIGHT + MACHINE[1]]), [WIDTH, FOOTER]));
        if let Some(item) = item {
            if let Some(texture) = icon(item.id) {
                layer.sprites.push((texture, centered(ITEM_CENTER, texture.size), texture.size));
            }
            let name = window::fit(font, &inventory::item_name(items, item.id, true, item.refine), 120.0);
            layer.texts.push((name.clone(), at([NAME_CENTER[0] - font.width(&name) / 2.0, NAME_CENTER[1] - 6.0]), [255; 3]));
        }
        if let Some(texture) = self.picked.and_then(|row| self.materials.get(row)).and_then(|&(ore, ..)| icon(ore)) {
            layer.sprites.push((texture, centered(ORE_CENTER, texture.size), texture.size));
        }
        // The ores: icon, name, chance and price; greyed when none is carried.
        for (row, &(ore, chance, price)) in self.materials.iter().enumerate().take(ROWS) {
            let top = [ROW_LEFT, ROW_TOP + row as f32 * ROW];
            if self.picked == Some(row) {
                layer.sprites.push((&assets.selected, at(top), assets.selected.size));
            }
            if let Some(texture) = icon(ore) {
                layer.sprites.push((texture, centered(offset(top, [22.0, 30.0]), texture.size), texture.size));
            }
            let color = if count(ore) > 0 { [0, 0, 0] } else { GREY };
            layer.texts.push((window::fit(font, &inventory::item_name(items, ore, true, 0), 130.0), at(offset(top, [48.0, 14.0])), color));
            layer.texts.push((format!("{chance}%  {} z", group(u64::from(price))), at(offset(top, [48.0, 32.0])), color));
        }
        // Blessings: the box, off and greyed when none can be used.
        layer.sprites.push((&assets.checkbox[usize::from(self.use_blessings)], at(offset(BLESSING, [0.0, 2.0])), assets.checkbox[0].size));
        let label = format!("{} ({})", message(BLESSING_LABEL), self.blessings);
        layer.texts.push((label, at(offset(BLESSING, [14.0, 0.0])), if self.blessings > 0 { [0, 0, 0] } else { GREY }));
        // Back, and Refine with the price in its field (off until an ore is picked).
        layer.sprites.push((&assets.back, at(BACK), BUTTON));
        let back = message(BACK_LABEL);
        layer.texts.push((back.to_owned(), at([BACK[0] + (BUTTON[0] - font.width(back)) / 2.0, BACK[1] + 20.0]), [0, 0, 0]));
        let price = self.picked.and_then(|row| self.materials.get(row)).map(|&(_, _, price)| price);
        layer.sprites.push((&assets.refine[usize::from(price.is_none())], at(REFINE), BUTTON));
        if let Some(price) = price {
            let text = group(u64::from(price));
            layer.texts.push((text.clone(), at([REFINE[0] + 70.0 - font.width(&text), REFINE[1] + 30.0]), [255; 3]));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refines_with_a_carried_ore() {
        let mut window = Refine::new();
        window.listed(4, 0, vec![(1010, 100, 50), (6240, 100, 5000)]);
        let carried = |id: u32| if id == 1010 { 1 } else { 0 };
        // Nothing picked: Refine does nothing; an ore not carried can't be picked.
        assert_eq!(window.click(Hit::Refine, carried, 1000), None);
        window.click(Hit::Ore(1), carried, 1000);
        assert_eq!(window.picked, None);
        window.click(Hit::Ore(0), carried, 1000);
        assert_eq!(window.click(Hit::Refine, carried, 10), Some(Action::Refuse(NO_ZENY)));
        assert_eq!(window.click(Hit::Refine, carried, 1000), Some(Action::Refine { index: 4, ore: 1010, blessings: false }));
        // No blessings for this level: the box stays off. One that takes 2 refuses with none carried.
        window.click(Hit::Blessing, carried, 1000);
        assert!(!window.use_blessings);
        window.listed(4, 2, vec![(1010, 90, 50)]);
        assert_eq!(window.picked, Some(0), "the same ore stays picked after a refine");
        window.click(Hit::Blessing, carried, 1000);
        assert_eq!(window.click(Hit::Refine, carried, 1000), Some(Action::Refuse(NO_BLESSINGS)));
        window.click(Hit::Back, carried, 1000);
        assert_eq!((window.item, window.materials.len()), (None, 0));
    }
}
