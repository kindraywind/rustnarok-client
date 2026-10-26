//! The grade enchant window: an item dropped on it, the materials that can raise its grade, blessing steps, and Start.
//!
//! Textures live under `data\texture\유저인터페이스\grade_enchant\`. The title bar has no text. Result effects
//! (`.str`) are left out: when they play isn't knowable.

use std::collections::HashMap;

use rustnarok_client::grf::Grf;
use rustnarok_client::lua::ItemInfo;
use rustnarok_client::net::{GradeBlessing, GradeMaterial, InventoryItem};
use rustnarok_client::text::Font;

use crate::inventory;
use crate::render::{Gpu, Texture};
use crate::shop::group;
use crate::window::{self, BUTTON_SIZE, Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI, contains, offset};

// ponytail: where each piece sits, the text colors, and which grade image is which are estimates.
const POSITION: [f32; 2] = [300.0, 100.0];
const BACKGROUND: [f32; 2] = [400.0, 330.0];
pub const HEIGHT: f32 = TITLE_HEIGHT + BACKGROUND[1];
const WIDTH: f32 = BACKGROUND[0];
const ITEM_FRAME: [f32; 2] = [104.0, 208.0];
const ITEM_AT: [f32; 2] = [12.0, TITLE_HEIGHT + 28.0];
const ROW_SIZE: [f32; 2] = [235.0, 37.0];
const ROW_AT: [f32; 2] = [150.0, TITLE_HEIGHT + 20.0];
const ROW_STEP: f32 = 44.0;
const STEP_SIZE: [f32; 2] = [32.0, 15.0];
const DOWN_AT: [f32; 2] = [150.0, TITLE_HEIGHT + 250.0];
const UP_AT: [f32; 2] = [190.0, TITLE_HEIGHT + 250.0];
const START_SIZE: [f32; 2] = [82.0, 28.0];
const START_AT: [f32; 2] = [300.0, TITLE_HEIGHT + 286.0];
const CAUTION: [f32; 2] = [12.0, 10.0];
const GRADE_ICON: [f32; 2] = [12.0, 12.0];
const WHEEL: [f32; 2] = [40.0, 23.0];
/// No row, too little zeny, too few materials or blessings.
const NO_ROW: usize = 3707;
const NO_ZENY: usize = 2968;
const NO_MATERIALS: usize = 3709;
/// Can break, the refine goes down, or neither.
const CAN_BREAK: usize = 3712;
const LOWERS_REFINE: usize = 3713;
const WONT_BREAK: usize = 3822;
/// ponytail: whether a line says the refine is too low, and which, isn't knowable. 3821 is a guess.
const REFINE_TOO_LOW: usize = 3821;
/// Raised, failed, refine down, destroyed, protected.
pub const RESULT_LINES: [usize; 5] = [3702, 3703, 3704, 3705, 3706];
pub const CANT_UPGRADE: usize = 3720;
pub const ANNOUNCE_OK: usize = 3718;
pub const ANNOUNCE_FAIL: usize = 3719;
/// ZC_UI_OPEN's kind for this window.
pub const UI_KIND: u8 = 8;
const TEXT: [u8; 3] = [0, 0, 0];
const WARN: [u8; 3] = [0xC0, 0x40, 0x20];

pub struct Assets {
    background: Texture,
    /// One frame per grade, none through A.
    frames: [Texture; 5],
    /// A material row: plain, hovered, picked.
    material: [Texture; 3],
    /// Start: plain, hovered, pressed, off.
    start: [Texture; 4],
    up: [Texture; 3],
    down: [Texture; 3],
    /// Grade marks D through A, and the three wheels.
    icons: [Texture; 4],
    wheels: [Texture; 3],
    caution: Texture,
    close: Texture,
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |name: &str| gpu.load_texture(grf, &format!("{UI}grade_enchant\\{name}.bmp"), false);
        let row = |name: &str| ["", "_over", "_pick"].map(|suffix| image(&format!("{name}{suffix}")));
        Self {
            background: image("bg_grade_enchant"),
            frames: [1, 2, 3, 4, 5].map(|grade| image(&format!("bg_gradeitem{grade}"))),
            material: row("btn_material"),
            start: ["", "_over", "_press", "_disable"].map(|suffix| image(&format!("btn_start{suffix}"))),
            up: row("btn_chance_up"),
            down: row("btn_chance_down"),
            icons: [1, 2, 3, 4].map(|grade| image(&format!("grade_icon{grade}"))),
            wheels: [1, 2, 3].map(|grade| image(&format!("gradewheel{grade}"))),
            caution: image("caution"),
            close: image("btn_close_a"),
            // btn_unbroken is the protection mark. This version doesn't use it.
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(TitleHit),
    Material(usize),
    BlessingUp,
    BlessingDown,
    Start,
    Window,
}

#[derive(Debug, PartialEq)]
pub enum Action {
    /// Grade the item with this material row and this many blessing steps (0: blessings off).
    Enchant {
        index: u16,
        material: u32,
        steps: u32,
    },
    /// The client's own refusal, a message number.
    Refuse(usize),
    Close,
}

/// What Start does: send the try, refuse with a message, or stay off.
#[derive(Debug, PartialEq)]
pub enum Start {
    Go { material: u32, steps: u32 },
    Refuse(usize),
    Off,
}

/// The chance on screen: the list's plus one share per step, and never past 100.
pub fn shown_chance(list: u32, steps: u32, per_step: u32) -> u32 {
    list.saturating_add(steps.saturating_mul(per_step)).min(100)
}

/// Start's answer. A list chance of 0 leaves it off: the server would ignore the request, blessings included.
/// The material number is the row. With steps, blessings are required too, and when the blessing item is the
/// material both amounts are required together.
pub fn start_request(
    picked: Option<usize>,
    materials: &[GradeMaterial],
    list_chance: u32,
    steps: u32,
    blessing_item: u32,
    per_step: u32,
    carried: impl Fn(u32) -> u32,
    zeny: u32,
) -> Start {
    if list_chance == 0 {
        return Start::Off;
    }
    let Some(row) = picked.filter(|&row| row < materials.len()) else {
        return Start::Refuse(NO_ROW);
    };
    let material = &materials[row];
    if zeny < material.zeny {
        return Start::Refuse(NO_ZENY);
    }
    let blessings = steps.saturating_mul(per_step);
    let mut need = material.amount;
    if steps > 0 {
        if blessing_item == material.item {
            need = need.saturating_add(blessings);
        } else if carried(blessing_item) < blessings {
            return Start::Refuse(NO_MATERIALS);
        }
    }
    if carried(material.item) < need {
        return Start::Refuse(NO_MATERIALS);
    }
    Start::Go { material: row as u32, steps: if steps > 0 { steps } else { 0 } }
}

/// What the item becomes after a try. Result 3 means it is already gone. Result 0 sets the refine to 0.
/// ponytail: result 2's downgrade isn't in the packet, and rathena's data never sends one, so the refine stays.
pub fn apply_result(item: &mut InventoryItem, grade: u16, result: u32) -> bool {
    if result == 3 {
        return false;
    }
    item.grade = u8::try_from(grade).unwrap_or(u8::MAX);
    if result == 0 {
        item.refine = 0;
    }
    true
}

/// ponytail: the grade's letter as its text is a guess (none, D, C, B, A).
pub fn grade_letter(grade: u16) -> &'static str {
    match grade {
        1 => "D",
        2 => "C",
        3 => "B",
        4 => "A",
        _ => "",
    }
}

pub struct GradeEnchant {
    pub frame: Frame,
    /// The item in the window, once the server lists its materials.
    item: Option<u16>,
    /// The list's chance, percent, before blessing steps.
    chance: u32,
    blessing: GradeBlessing,
    materials: Vec<GradeMaterial>,
    steps: u32,
    picked: Option<usize>,
}

impl GradeEnchant {
    pub fn new() -> Self {
        let mut frame = Frame::new(POSITION, WIDTH);
        frame.open = true;
        Self { frame, item: None, chance: 0, blessing: GradeBlessing::default(), materials: Vec::new(), steps: 0, picked: None }
    }

    /// The server's list for an item. The same item keeps the row, with the steps clamped to the new most.
    pub fn listed(&mut self, index: u16, chance: u32, blessing: GradeBlessing, materials: Vec<GradeMaterial>) {
        let same = self.item == Some(index);
        self.item = Some(index);
        self.chance = chance;
        self.blessing = blessing;
        self.materials = materials;
        if same {
            if self.picked.is_some_and(|row| row >= self.materials.len()) {
                self.picked = None;
            }
            self.steps = self.steps.min(self.blessing.most);
        } else {
            self.picked = None;
            self.steps = 0;
        }
    }

    pub fn clear(&mut self) {
        self.item = None;
        self.picked = None;
        self.steps = 0;
        self.chance = 0;
        self.materials.clear();
        self.blessing = GradeBlessing::default();
    }

    /// The try's result. 3 empties the window. 0 and 1 empty it too: the caller asks for the list again, and when
    /// none comes (the top grade) the window stays empty.
    pub fn finished(&mut self, result: u32) -> bool {
        match result {
            3 => {
                self.clear();
                false
            }
            0 | 1 => {
                self.clear();
                true
            }
            _ => false,
        }
    }

    pub fn chance_shown(&self) -> u32 {
        shown_chance(self.chance, self.steps, self.blessing.chance)
    }

    /// Start sends nothing while the list's chance is 0.
    pub fn start_off(&self) -> bool {
        self.chance == 0
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let local = self.frame.local(cursor, HEIGHT)?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(Hit::Title(hit));
        }
        let row = (0..self.materials.len()).find(|&row| contains(offset(ROW_AT, [0.0, row as f32 * ROW_STEP]), ROW_SIZE, local));
        Some(match row {
            Some(row) => Hit::Material(row),
            None if contains(UP_AT, STEP_SIZE, local) => Hit::BlessingUp,
            None if contains(DOWN_AT, STEP_SIZE, local) => Hit::BlessingDown,
            None if contains(START_AT, START_SIZE, local) => Hit::Start,
            None => Hit::Window,
        })
    }

    /// Reacts to a click. `carried` counts an item, `zeny` is the zeny carried.
    pub fn click(&mut self, hit: Hit, carried: impl Fn(u32) -> u32, zeny: u32) -> Option<Action> {
        match hit {
            Hit::Title(TitleHit::Close) => return Some(Action::Close),
            Hit::Title(title) => self.frame.click_title(title),
            Hit::Material(row) if row < self.materials.len() => self.picked = Some(row),
            Hit::BlessingUp => self.steps = (self.steps + 1).min(self.blessing.most),
            Hit::BlessingDown => self.steps = self.steps.saturating_sub(1),
            Hit::Start => {
                let index = self.item?;
                return match start_request(self.picked, &self.materials, self.chance, self.steps, self.blessing.item, self.blessing.per_step, carried, zeny) {
                    Start::Go { material, steps } => Some(Action::Enchant { index, material, steps }),
                    Start::Refuse(id) => Some(Action::Refuse(id)),
                    Start::Off => None,
                };
            }
            Hit::Material(_) | Hit::Window => {}
        }
        None
    }

    /// The buttons on screen, named for scripted clicks.
    pub fn button_boxes(&self) -> Vec<(&'static str, [f32; 4])> {
        if !self.frame.open {
            return Vec::new();
        }
        let place = |local: [f32; 2], size: [f32; 2]| {
            let [x, y] = self.frame.at(local);
            [x, y, size[0], size[1]]
        };
        let mut boxes = vec![("close", place(self.frame.close_button(), BUTTON_SIZE))];
        if self.frame.minimized {
            return boxes;
        }
        for row in 0..self.materials.len().min(2) {
            let name = if row == 0 { "material_0" } else { "material_1" };
            boxes.push((name, place(offset(ROW_AT, [0.0, row as f32 * ROW_STEP]), ROW_SIZE)));
        }
        boxes.push(("blessing_down", place(DOWN_AT, STEP_SIZE)));
        boxes.push(("blessing_up", place(UP_AT, STEP_SIZE)));
        boxes.push(("start", place(START_AT, START_SIZE)));
        boxes
    }

    pub fn icons(&self, items: &HashMap<u32, ItemInfo>, carried: &[InventoryItem]) -> Vec<String> {
        let item = self.item.and_then(|index| carried.iter().find(|item| item.index == index)).map(|item| item.id);
        item.into_iter()
            .chain(self.materials.iter().map(|row| row.item))
            .chain((self.blessing.item != 0).then_some(self.blessing.item))
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
        // The bar has no title in the message table.
        self.frame.draw_title(layer, chrome, "", pointer.0);
        layer.sprites.push((&assets.close, at(self.frame.close_button()), BUTTON_SIZE));
        if self.frame.minimized {
            return;
        }
        let local = [pointer.0[0] - self.frame.position[0], pointer.0[1] - self.frame.position[1]];
        let over = |place: [f32; 2], size: [f32; 2]| contains(place, size, local);
        let icon = |id: u32| items.get(&id).and_then(|info| icons.get(&inventory::icon_path(info, true))).and_then(Option::as_ref);
        let count = |id: u32| carried.iter().filter(|item| item.id == id).map(|item| u32::from(item.amount)).sum::<u32>();
        layer.sprites.push((&assets.background, at([0.0, TITLE_HEIGHT]), BACKGROUND));
        let item = self.item.and_then(|index| carried.iter().find(|item| item.index == index));
        if let Some(item) = item {
            // ponytail: frame 1 is grade none, then one frame per grade.
            let frame = &assets.frames[usize::from(item.grade).min(assets.frames.len() - 1)];
            layer.sprites.push((frame, at(ITEM_AT), ITEM_FRAME));
            if let Some(texture) = icon(item.id) {
                let center = [ITEM_AT[0] + ITEM_FRAME[0] / 2.0, ITEM_AT[1] + 78.0];
                layer.sprites.push((texture, at([center[0] - texture.size[0] / 2.0, center[1] - texture.size[1] / 2.0]), texture.size));
            }
            let name = inventory::item_name(items, item.id, true, item.refine);
            let name = window::fit(font, &name, ITEM_FRAME[0] - 8.0);
            layer.texts.push((name.clone(), at([ITEM_AT[0] + 4.0, ITEM_AT[1] + ITEM_FRAME[1] - 18.0]), TEXT));
            if (1..=4).contains(&item.grade) {
                layer.sprites.push((&assets.icons[usize::from(item.grade - 1)], at([ITEM_AT[0] + 8.0, ITEM_AT[1] + 8.0]), GRADE_ICON));
                // ponytail: three wheels, one each for D and C and one shared by B and A.
                let wheel = match item.grade {
                    1 => 0,
                    2 => 1,
                    _ => 2,
                };
                layer.sprites.push((&assets.wheels[wheel], at([ITEM_AT[0] + 28.0, ITEM_AT[1] + 6.0]), WHEEL));
                let letter = grade_letter(u16::from(item.grade));
                layer.texts.push((letter.to_owned(), at([ITEM_AT[0] + 72.0, ITEM_AT[1] + 8.0]), TEXT));
            }
        }
        let shown = self.chance_shown();
        layer.texts.push((format!("{shown}%"), at([ITEM_AT[0] + 8.0, ITEM_AT[1] + ITEM_FRAME[1] + 6.0]), TEXT));
        if self.item.is_some() && self.chance == 0 {
            layer.texts.push((window::fit(font, message(REFINE_TOO_LOW), 200.0), at([ITEM_AT[0], TITLE_HEIGHT + BACKGROUND[1] - 36.0]), WARN));
        }
        for (row, material) in self.materials.iter().enumerate() {
            let place = offset(ROW_AT, [0.0, row as f32 * ROW_STEP]);
            let face = if self.picked == Some(row) {
                2
            } else if over(place, ROW_SIZE) {
                1
            } else {
                0
            };
            layer.sprites.push((&assets.material[face], at(place), ROW_SIZE));
            if let Some(texture) = icon(material.item) {
                layer.sprites.push((texture, at(offset(place, [4.0, 6.0])), texture.size));
            }
            let cost = format!("{}  {} z", material.amount, group(u64::from(material.zeny)));
            let short = count(material.item) < material.amount;
            layer.texts.push((cost, at(offset(place, [32.0, 4.0])), if short { WARN } else { TEXT }));
            let warning = if material.can_break {
                CAN_BREAK
            } else if material.lowers_refine {
                LOWERS_REFINE
            } else {
                WONT_BREAK
            };
            if material.can_break || material.lowers_refine {
                layer.sprites.push((&assets.caution, at(offset(place, [32.0, 20.0])), CAUTION));
            }
            let line = window::fit(font, message(warning), ROW_SIZE[0] - 52.0);
            layer.texts.push((line, at(offset(place, [46.0, 18.0])), if material.can_break || material.lowers_refine { WARN } else { TEXT }));
        }
        if self.blessing.item != 0 {
            if let Some(texture) = icon(self.blessing.item) {
                layer.sprites.push((texture, at([12.0, TITLE_HEIGHT + 246.0]), texture.size));
            }
            let label = format!("{}  {} / {}", inventory::item_name(items, self.blessing.item, true, 0), self.steps, self.blessing.most);
            layer.texts.push((window::fit(font, &label, 120.0), at([40.0, TITLE_HEIGHT + 252.0]), TEXT));
        }
        let down = if over(DOWN_AT, STEP_SIZE) { if pointer.1 { 2 } else { 1 } } else { 0 };
        let up = if over(UP_AT, STEP_SIZE) { if pointer.1 { 2 } else { 1 } } else { 0 };
        layer.sprites.push((&assets.down[down], at(DOWN_AT), STEP_SIZE));
        layer.sprites.push((&assets.up[up], at(UP_AT), STEP_SIZE));
        let start = if self.start_off() {
            3
        } else if over(START_AT, START_SIZE) {
            if pointer.1 { 2 } else { 1 }
        } else {
            0
        };
        layer.sprites.push((&assets.start[start], at(START_AT), START_SIZE));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blessing(item: u32, most: u32, chance: u32) -> GradeBlessing {
        GradeBlessing { item, per_step: 1, most, chance }
    }

    fn row(item: u32, amount: u32, zeny: u32) -> GradeMaterial {
        GradeMaterial { item, amount, zeny, lowers_refine: false, can_break: true }
    }

    fn carried(have: &[(u32, u32)]) -> impl Fn(u32) -> u32 + '_ {
        |id| have.iter().find(|&&(item, _)| item == id).map_or(0, |&(_, amount)| amount)
    }

    #[test]
    fn chance_adds_steps_and_stops_at_100() {
        assert_eq!(shown_chance(80, 0, 1), 80);
        assert_eq!(shown_chance(80, 3, 1), 83);
        assert_eq!(shown_chance(10, 2, 5), 20);
        assert_eq!(shown_chance(80, 30, 1), 100);
        assert_eq!(shown_chance(99, 5, 1), 100);
        assert_eq!(shown_chance(0, 10, 1), 10);
    }

    #[test]
    fn start_refuses_and_stays_off() {
        let rows = [row(1_000_325, 1, 175_000), row(1_000_325, 5, 875_000)];
        let have = [(1_000_325, 1), (1_000_337, 2)];
        let ask = |picked, steps, blessing_item, zeny| start_request(picked, &rows, 80, steps, blessing_item, 1, carried(&have), zeny);
        assert_eq!(ask(None, 0, 1_000_337, 175_000), Start::Refuse(NO_ROW));
        assert_eq!(ask(Some(0), 0, 1_000_337, 174_999), Start::Refuse(NO_ZENY));
        assert_eq!(ask(Some(1), 0, 1_000_337, 875_000), Start::Refuse(NO_MATERIALS));
        assert_eq!(ask(Some(0), 3, 1_000_337, 175_000), Start::Refuse(NO_MATERIALS));
        // The blessing item is the material: both amounts together.
        assert_eq!(start_request(Some(0), &rows, 80, 2, 1_000_325, 1, carried(&[(1_000_325, 2)]), 175_000), Start::Refuse(NO_MATERIALS));
        assert_eq!(start_request(Some(0), &rows, 80, 2, 1_000_325, 1, carried(&[(1_000_325, 3)]), 175_000), Start::Go { material: 0, steps: 2 });
        assert_eq!(ask(Some(0), 0, 1_000_337, 175_000), Start::Go { material: 0, steps: 0 });
        assert_eq!(ask(Some(0), 2, 1_000_337, 175_000), Start::Go { material: 0, steps: 2 });
        // A chance of 0 keeps Start off, even with steps that would show a number.
        assert_eq!(start_request(Some(0), &rows, 0, 10, 1_000_337, 1, carried(&have), 175_000), Start::Off);
        assert_eq!(start_request(None, &rows, 0, 0, 0, 0, carried(&have), 0), Start::Off);
    }

    #[test]
    fn the_item_after_each_result() {
        let mut raised = InventoryItem { refine: 20, grade: 0, ..InventoryItem::default() };
        assert!(apply_result(&mut raised, 1, 0));
        assert_eq!((raised.grade, raised.refine), (1, 0));

        let mut failed = InventoryItem { refine: 20, grade: 1, ..InventoryItem::default() };
        assert!(apply_result(&mut failed, 1, 1));
        assert_eq!((failed.grade, failed.refine), (1, 20));

        // ponytail: result 2 keeps the old refine; the downgrade amount isn't sent.
        let mut down = InventoryItem { refine: 15, grade: 2, ..InventoryItem::default() };
        assert!(apply_result(&mut down, 2, 2));
        assert_eq!((down.grade, down.refine), (2, 15));

        let mut gone = InventoryItem { refine: 10, grade: 1, ..InventoryItem::default() };
        assert!(!apply_result(&mut gone, 0, 3));
        assert_eq!((gone.grade, gone.refine), (1, 10));
    }

    #[test]
    fn the_window_steps_and_answers() {
        let mut window = GradeEnchant::new();
        let rows = vec![row(1_000_325, 1, 175_000), row(1_000_325, 5, 875_000)];
        window.listed(4, 80, blessing(1_000_337, 10, 1), rows);
        let have = carried(&[(1_000_325, 1), (1_000_337, 2)]);
        assert_eq!(window.click(Hit::Start, &have, 175_000), Some(Action::Refuse(NO_ROW)));
        window.click(Hit::Material(0), &have, 175_000);
        window.click(Hit::BlessingUp, &have, 175_000);
        window.click(Hit::BlessingUp, &have, 175_000);
        assert_eq!(window.steps, 2);
        assert_eq!(window.chance_shown(), 82);
        assert_eq!(window.click(Hit::Start, &have, 175_000), Some(Action::Enchant { index: 4, material: 0, steps: 2 }));
        window.click(Hit::BlessingDown, &have, 175_000);
        assert_eq!(window.click(Hit::Start, &have, 175_000), Some(Action::Enchant { index: 4, material: 0, steps: 1 }));
        // Ten is the most.
        window.steps = 10;
        window.click(Hit::BlessingUp, &have, 175_000);
        assert_eq!(window.steps, 10);
        assert_eq!(window.chance_shown(), 90);
        // A chance of 0: the button stays off.
        window.chance = 0;
        assert!(window.start_off());
        assert_eq!(window.click(Hit::Start, &have, 175_000), None);

        assert!(window.finished(0));
        assert_eq!(window.item, None);
        window.listed(4, 100, blessing(1_000_337, 10, 1), vec![row(1_000_325, 1, 175_000)]);
        assert!(!window.finished(2));
        assert_eq!(window.item, Some(4));
        assert!(!window.finished(3));
        assert_eq!(window.item, None);

        let names: Vec<_> = {
            let mut listed = GradeEnchant::new();
            listed.listed(4, 100, blessing(1_000_337, 10, 1), vec![row(1_000_325, 1, 175_000), row(1_000_325, 5, 875_000)]);
            listed.button_boxes().into_iter().map(|(name, _)| name).collect()
        };
        assert_eq!(names, ["close", "material_0", "material_1", "blessing_down", "blessing_up", "start"]);
        assert_eq!(grade_letter(1), "D");
        assert_eq!(grade_letter(4), "A");
    }
}
