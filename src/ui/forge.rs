//! Forging material window for element stones and Star Crumbs.

use std::collections::HashMap;

use rustnarok_client::lua::ItemInfo;
use rustnarok_client::net::InventoryItem;
use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};
use crate::window::{Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI, contains, inventory};

pub const WIDTH: f32 = 280.0;
pub const HEIGHT: f32 = 150.0;
const FOOTER: f32 = 29.0;
pub const BUTTON: [f32; 2] = [42.0, 20.0];
pub const OK: [f32; 2] = [189.0, 126.0];
pub const CANCEL: [f32; 2] = [234.0, 126.0];

pub const SLOT_Y: f32 = 89.0;
pub const SLOT_X: [f32; 3] = [4.0, 36.0, 68.0];
pub const SLOT_SIZE: [f32; 2] = [32.0, 32.0];

pub const CREATE_MSG: usize = 426;
pub const REQUIRED_MATERIALS_MSG: usize = 427;

pub struct Assets {
    pub bar: Texture,
    pub cell: Texture,
    pub buttons: [[Texture; 3]; 2],
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &rustnarok_client::grf::Grf) -> Self {
        let image = |path: String| gpu.load_texture(grf, &path, false);
        let button = |name: &str| ["", "_a", "_b"].map(|suffix| image(format!("{UI}btn_{name}{suffix}.bmp")));
        Self {
            bar: image(format!("{UI}basic_interface\\btnbar_mid2.bmp")),
            cell: image(format!("{UI}basic_interface\\itemwin_mid.bmp")),
            buttons: [button("ok"), button("cancel")],
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(TitleHit),
    Slot(usize),
    Ok,
    Cancel,
    Window,
}

#[derive(Debug, PartialEq, Clone)]
pub enum Action {
    Ok([u32; 3]),
    Cancel,
    Take(usize),
}

/// Whether the item is a smelting recipe (items 994..=1000).
pub fn is_smelting(item: u32) -> bool {
    (994..=1000).contains(&item)
}

/// The Rune Knight / Dragon Knight jobs that skip the material window.
pub fn skips_material_window(job: u16) -> bool {
    matches!(job, 4054 | 4060 | 4096 | 4252)
}

/// The effect ID played on the player for each making result.
pub fn result_effect(result: u16) -> Option<u32> {
    match result {
        0 => Some(154),
        1 => Some(155),
        2 => Some(305),
        3 => Some(306),
        4 => Some(1015),
        5 => Some(1016),
        6 => Some(1017),
        7 => Some(1018),
        _ => None,
    }
}

pub struct Forge {
    pub frame: Frame,
    pub item: u32,
    pub slots: [Option<InventoryItem>; 3],
}

impl Forge {
    pub fn new(position: [f32; 2], item: u32) -> Self {
        let mut frame = Frame::new(position, WIDTH);
        frame.open = true;
        Self { frame, item, slots: [None, None, None] }
    }

    pub fn slots_drawn(&self) -> bool {
        !is_smelting(self.item)
    }

    pub fn can_drop(&self, item_id: u32) -> bool {
        if is_smelting(self.item) {
            return false;
        }
        let is_star_crumb = item_id == 1000;
        let is_element_stone = (994..=997).contains(&item_id);
        if !is_star_crumb && !is_element_stone {
            return false;
        }
        if is_element_stone && self.slots.iter().filter_map(|s| s.as_ref()).any(|item| (994..=997).contains(&item.id)) {
            return false;
        }
        self.slots.iter().any(|s| s.is_none())
    }

    pub fn put_item(&mut self, item: InventoryItem) -> bool {
        if !self.can_drop(item.id) {
            return false;
        }
        if let Some(slot) = self.slots.iter_mut().find(|s| s.is_none()) {
            let mut single = item;
            single.amount = 1;
            *slot = Some(single);
            true
        } else {
            false
        }
    }

    pub fn take_slot(&mut self, slot_idx: usize) -> Option<InventoryItem> {
        if slot_idx < self.slots.len() { self.slots[slot_idx].take() } else { None }
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let local = self.frame.local(cursor, HEIGHT)?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(Hit::Title(hit));
        }
        if contains(OK, BUTTON, local) {
            return Some(Hit::Ok);
        }
        if contains(CANCEL, BUTTON, local) {
            return Some(Hit::Cancel);
        }
        if self.slots_drawn() {
            for (i, &slot_x) in SLOT_X.iter().enumerate() {
                if contains([slot_x, SLOT_Y], SLOT_SIZE, local) {
                    return Some(Hit::Slot(i));
                }
            }
        }
        Some(Hit::Window)
    }

    pub fn covers(&self, cursor: [f32; 2]) -> bool {
        self.hit(cursor).is_some()
    }

    pub fn click(&mut self, hit: Hit) -> Option<Action> {
        match hit {
            Hit::Title(TitleHit::Close) | Hit::Cancel => Some(Action::Cancel),
            Hit::Title(title) => {
                self.frame.click_title(title);
                None
            }
            Hit::Slot(i) if self.slots.get(i).is_some_and(Option::is_some) => Some(Action::Take(i)),
            Hit::Slot(_) => None,
            Hit::Ok => {
                let materials =
                    [self.slots[0].as_ref().map_or(0, |i| i.id), self.slots[1].as_ref().map_or(0, |i| i.id), self.slots[2].as_ref().map_or(0, |i| i.id)];
                Some(Action::Ok(materials))
            }
            Hit::Window => None,
        }
    }

    pub fn button_boxes(&self) -> Vec<(String, [f32; 4])> {
        if self.frame.minimized {
            return Vec::new();
        }
        let mut buttons = [("btn_ok", OK), ("btn_cancel", CANCEL)]
            .map(|(name, corner)| {
                let [x, y] = self.frame.at(corner);
                (name.to_owned(), [x, y, BUTTON[0], BUTTON[1]])
            })
            .to_vec();
        if self.slots_drawn() {
            for (i, &slot_x) in SLOT_X.iter().enumerate() {
                let [x, y] = self.frame.at([slot_x, SLOT_Y]);
                buttons.push((format!("forge_slot:{i}"), [x, y, SLOT_SIZE[0], SLOT_SIZE[1]]));
            }
        }
        buttons
    }

    pub fn icons(&self, items: &HashMap<u32, ItemInfo>) -> Vec<String> {
        self.slots.iter().filter_map(|slot| slot.as_ref()).filter_map(|item| items.get(&item.id).map(|info| inventory::icon_path(info, true))).collect()
    }

    pub fn draw<'a>(
        &self,
        layer: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        icons: &'a HashMap<String, Option<Texture>>,
        items: &HashMap<u32, ItemInfo>,
        messages: &[String],
        font: &Font,
        pointer: ([f32; 2], bool),
    ) {
        if !self.frame.open {
            return;
        }
        // Minimized: the title bar alone, so its buttons can bring it back.
        if self.frame.minimized {
            self.frame.draw_title(layer, chrome, "", pointer.0);
            return;
        }
        let pos = self.frame.position;
        let body_height = HEIGHT - TITLE_HEIGHT - FOOTER;
        layer.sprites.push((&chrome.white, self.frame.at([0.0, TITLE_HEIGHT]), [WIDTH, body_height]));
        layer.sprites.push((&chrome.border, self.frame.at([0.0, TITLE_HEIGHT]), [1.0, body_height]));
        layer.sprites.push((&chrome.border, self.frame.at([WIDTH - 1.0, TITLE_HEIGHT]), [1.0, body_height]));
        self.frame.draw_title(layer, chrome, "", pointer.0);

        // Footer strip:
        layer.sprites.push((&assets.bar, [pos[0], pos[1] + HEIGHT - FOOTER], [WIDTH, FOOTER]));

        // Buttons:
        let (cursor, pressed) = pointer;
        let local = self.frame.local(cursor, HEIGHT);
        let ok_hover = local.is_some_and(|l| contains(OK, BUTTON, l));
        let cancel_hover = local.is_some_and(|l| contains(CANCEL, BUTTON, l));
        let button_img = |assets_idx: usize, hover: bool| {
            let state = if hover && pressed {
                2
            } else if hover {
                1
            } else {
                0
            };
            &assets.buttons[assets_idx][state]
        };
        layer.sprites.push((button_img(0, ok_hover), self.frame.at(OK), BUTTON));
        layer.sprites.push((button_img(1, cancel_hover), self.frame.at(CANCEL), BUTTON));

        // Text:
        let item_name = items.get(&self.item).map_or_else(|| self.item.to_string(), |info| info.name.clone());
        let create_msg = messages.get(CREATE_MSG).map_or("", String::as_str);
        let req_msg = messages.get(REQUIRED_MATERIALS_MSG).map_or("", String::as_str);
        let name_w = font.width(&item_name);

        // Line 1 at (16, 2):
        layer.texts.push((item_name.clone(), self.frame.at([16.0, 2.0]), [0, 0, 0]));
        let x1 = 16.0 + name_w;
        layer.texts.push((create_msg.to_owned(), self.frame.at([x1 + 1.0, 3.0]), [255, 255, 255]));
        layer.texts.push((create_msg.to_owned(), self.frame.at([x1, 2.0]), [0, 0, 0]));

        // Line 2 at (16, 22):
        layer.texts.push((item_name.clone(), self.frame.at([16.0, 22.0]), [0, 0, 0]));
        let x2 = 16.0 + name_w;
        layer.texts.push((req_msg.to_owned(), self.frame.at([x2 + 1.0, 23.0]), [255, 255, 255]));
        layer.texts.push((req_msg.to_owned(), self.frame.at([x2, 22.0]), [0, 0, 0]));

        // Slots:
        if self.slots_drawn() {
            for (i, &slot_x) in SLOT_X.iter().enumerate() {
                layer.sprites.push((&assets.cell, self.frame.at([slot_x, SLOT_Y]), SLOT_SIZE));
                if let Some(item) = &self.slots[i]
                    && let Some(Some(icon)) = items.get(&item.id).and_then(|info| icons.get(&inventory::icon_path(info, true)))
                {
                    layer.sprites.push((icon, self.frame.at([slot_x + 4.0, SLOT_Y + 4.0]), icon.size));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: u32) -> InventoryItem {
        InventoryItem { id, identified: true, amount: 1, ..Default::default() }
    }

    #[test]
    fn drop_rules_and_smelting() {
        let mut forge = Forge::new([100.0, 100.0], 1163); // Two-handed Sword
        assert!(forge.slots_drawn());

        // Each allowed id:
        for allowed in [1000, 994, 995, 996, 997] {
            assert!(forge.can_drop(allowed));
        }

        // Refused item:
        assert!(!forge.can_drop(501));
        assert!(!forge.can_drop(909));
        assert!(!forge.can_drop(1201));

        // Putting an element stone (994 Flame Heart):
        assert!(forge.put_item(item(994)));
        assert_eq!(forge.slots[0].as_ref().unwrap().id, 994);

        // A second element stone is refused:
        assert!(!forge.can_drop(994));
        assert!(!forge.can_drop(995));
        assert!(!forge.can_drop(996));
        assert!(!forge.can_drop(997));
        assert!(!forge.put_item(item(995)));

        // Star Crumb (1000) is still allowed:
        assert!(forge.can_drop(1000));
        assert!(forge.put_item(item(1000)));
        assert_eq!(forge.slots[1].as_ref().unwrap().id, 1000);

        // A second Star Crumb is also allowed:
        assert!(forge.can_drop(1000));
        assert!(forge.put_item(item(1000)));
        assert_eq!(forge.slots[2].as_ref().unwrap().id, 1000);

        // Window is full (all 3 slots occupied) -> refused:
        assert!(!forge.can_drop(1000));
        assert!(!forge.put_item(item(1000)));

        // Take one out:
        let taken = forge.take_slot(1);
        assert_eq!(taken.unwrap().id, 1000);
        assert!(forge.slots[1].is_none());
        assert!(forge.can_drop(1000));

        // Smelting recipes (994..=1000):
        for recipe in 994..=1000 {
            let smelting = Forge::new([100.0, 100.0], recipe);
            assert!(!smelting.slots_drawn());
            assert!(!smelting.can_drop(1000));
            assert!(!smelting.can_drop(994));
        }
    }

    #[test]
    fn job_skipping() {
        assert!(skips_material_window(4054)); // Rune Knight
        assert!(skips_material_window(4060)); // Transcendent Rune Knight
        assert!(skips_material_window(4096)); // Baby Rune Knight
        assert!(skips_material_window(4252)); // Dragon Knight

        // Other jobs do not skip:
        assert!(!skips_material_window(0)); // Novice
        assert!(!skips_material_window(10)); // Blacksmith
        assert!(!skips_material_window(18)); // Alchemist
        assert!(!skips_material_window(4011)); // Whitesmith
        assert!(!skips_material_window(4019)); // Creator
        assert!(!skips_material_window(4066)); // Mechanic
    }

    #[test]
    fn click_actions() {
        let mut forge = Forge::new([100.0, 100.0], 1163);
        forge.put_item(item(1000));
        forge.put_item(item(994));

        assert_eq!(forge.click(Hit::Cancel), Some(Action::Cancel));
        assert_eq!(forge.click(Hit::Title(TitleHit::Close)), Some(Action::Cancel));
        assert_eq!(forge.click(Hit::Ok), Some(Action::Ok([1000, 994, 0])));

        // Take slot 0:
        assert_eq!(forge.click(Hit::Slot(0)), Some(Action::Take(0)));
        // Slot 2 is empty, click gives None:
        assert_eq!(forge.click(Hit::Slot(2)), None);
    }

    #[test]
    fn result_effect_table() {
        assert_eq!(result_effect(0), Some(154));
        assert_eq!(result_effect(1), Some(155));
        assert_eq!(result_effect(2), Some(305));
        assert_eq!(result_effect(3), Some(306));
        assert_eq!(result_effect(4), Some(1015));
        assert_eq!(result_effect(5), Some(1016));
        assert_eq!(result_effect(6), Some(1017));
        assert_eq!(result_effect(7), Some(1018));
        assert_eq!(result_effect(8), None);
    }
}
