//! The grade enchant window: an item dropped on it, the materials that can raise its grade, blessing steps, and Start.
//!
//! Textures live under `data\texture\유저인터페이스\grade_enchant\`. The window is its background picture, which has
//! its own title strip; no title text. Result effects (`.str`) are left out: when they play isn't knowable.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use rustnarok_client::grf::Grf;
use rustnarok_client::lua::ItemInfo;
use rustnarok_client::net::{GradeBlessing, GradeMaterial, InventoryItem};
use rustnarok_client::text::Font;

use crate::inventory;
use crate::render::{Gpu, Texture};
use crate::window::{self, BUTTON_SIZE, Chrome, Frame, TitleHit};
use crate::{Layer, UI, contains, offset};

// Places are the game's, on the 400×330 picture.
const SIZE: [f32; 2] = [400.0, 330.0];
pub const HEIGHT: f32 = SIZE[1];
/// The picture's title strip: a press right of x 141 and above y 38 drags the window.
const DRAG_STRIP: [f32; 2] = [141.0, 38.0];
const CLOSE_AT: [f32; 2] = [384.0, 20.0];
/// The "Close" text button.
const CLOSE_TEXT_AT: [f32; 2] = [345.0, 299.0];
const CLOSE_TEXT_SIZE: [f32; 2] = [42.0, 20.0];
const CLOSE_LABEL: usize = 1299;
/// The machine, with the item's icon (and grade mark) on it and the chance above.
const MACHINE_AT: [f32; 2] = [24.0, 54.0];
const MACHINE_SIZE: [f32; 2] = [104.0, 208.0];
const ITEM_ICON: [f32; 2] = [64.0, 125.0];
const GRADE_ICON_AT: [f32; 2] = [64.0, 137.0];
const GRADE_ICON: [f32; 2] = [12.0, 12.0];
/// The chance, centred in the 74 px from x 39.
const CHANCE_AT: [f32; 2] = [39.0, 88.0];
const CHANCE_WIDTH: f32 = 74.0;
const CHANCE_SIZE: f32 = 28.0;
const WHEEL_AT: [f32; 2] = [56.0, 268.0];
const WHEEL: [f32; 2] = [40.0, 23.0];
/// The three material rows on their beds, 40 apart; each with an icon and "name amount ea".
const ROWS: usize = 3;
const ROW_AT: [f32; 2] = [158.0, 102.0];
const ROW_STEP: f32 = 40.0;
const ROW_SIZE: [f32; 2] = [235.0, 37.0];
const ROW_ICON: [f32; 2] = [12.0, 7.0];
const ROW_TEXT: [f32; 2] = [45.0, 16.0];
/// "%s %d ea".
const AMOUNT_OF: usize = 3553;
const UP_AT: [f32; 2] = [158.0, 225.0];
const DOWN_AT: [f32; 2] = [158.0, 240.0];
const STEP_SIZE: [f32; 2] = [32.0, 15.0];
/// The blessing's box, its icon and "name amount ea" beside it.
const BLESSING_AT: [f32; 2] = [192.0, 225.0];
const BLESSING_SIZE: [f32; 2] = [30.0, 30.0];
const BLESSING_ICON: [f32; 2] = [195.0, 228.0];
const BLESSING_TEXT: [f32; 2] = [228.0, 237.0];
const START_AT: [f32; 2] = [35.0, 291.0];
const START_SIZE: [f32; 2] = [82.0, 28.0];
/// The zeny in the TAX box, ending at x 307.
const ZENY_AT: [f32; 2] = [307.0, 303.0];
const ZENY_SIZE: f32 = 16.0;
/// Up to three notice lines, a caution mark before each.
const NOTICE_AT: [f32; 2] = [152.0, 46.0];
const NOTICE_TEXT: f32 = 14.0;
const NOTICE_STEP: f32 = 16.0;
const NOTICE_WIDTH: f32 = 228.0;
const CAUTION: [f32; 2] = [12.0, 10.0];
/// The machine's frames step every 200 ms (all five, four while a result plays); the wheel's every 100 ms while a
/// result plays, else its first.
const MACHINE_STEP: Duration = Duration::from_millis(200);
const WHEEL_STEP: Duration = Duration::from_millis(100);

/// No row, too few materials, too little zeny, heavy.
const NO_ROW: usize = 3707;
const SHORT_MATERIAL: usize = 3708;
const NO_ZENY: usize = 2968;
const NO_MATERIALS: usize = 3709;
const HEAVY: usize = 3710;
/// Can break, the refine goes down, or won't break.
const CAN_BREAK: usize = 3712;
const LOWERS_REFINE: usize = 3713;
const WONT_BREAK: usize = 3822;
/// The list's chance is 0.
const REFINE_TOO_LOW: usize = 3821;
/// The empty window's three lines.
const EMPTY_NOTICES: [(usize, bool); 3] = [(3870, false), (3871, true), (3872, true)];
/// Raised, failed, refine down, destroyed, protected.
pub const RESULT_LINES: [usize; 5] = [3702, 3703, 3704, 3705, 3706];
/// A try's line comes this long after its result (the result's animation), and the window empties then.
const RESULT_DELAY: Duration = Duration::from_secs(3);
pub const CANT_UPGRADE: usize = 3720;
pub const ANNOUNCE_OK: usize = 3718;
pub const ANNOUNCE_FAIL: usize = 3719;
/// ZC_UI_OPEN's kind for this window.
pub const UI_KIND: u8 = 8;
const TEXT: [u8; 3] = [0, 0, 0];
const WHITE: [u8; 3] = [255, 255, 255];
const RED: [u8; 3] = [0xDC, 0x0A, 0x0A];
const GREEN: [u8; 3] = [0x00, 0xEB, 0x5A];

pub struct Assets {
    background: Texture,
    /// The machine's animation, five frames.
    frames: [Texture; 5],
    /// A material row: plain, hovered, picked.
    material: [Texture; 3],
    /// Start: plain, hovered, pressed, off.
    start: [Texture; 4],
    up: [Texture; 3],
    down: [Texture; 3],
    /// Grade marks D through A.
    icons: [Texture; 4],
    /// The wheel's spin, three frames.
    wheels: [Texture; 3],
    blessing: Texture,
    caution: Texture,
    /// "0" to "9" and "%": the chance's, white and its black outline, and the zeny's, black.
    chance_digits: [Texture; 11],
    chance_outline: [Texture; 11],
    zeny_digits: [Texture; 11],
}

const DIGITS: &str = "0123456789%";
/// Where a bold text's black outline goes round it, a pixel out.
const OUTLINE: [[f32; 2]; 6] = [[-1.0, 0.0], [2.0, 0.0], [0.0, -1.0], [0.0, 1.0], [1.0, -1.0], [1.0, 1.0]];

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf, font: &Font) -> Self {
        let image = |name: &str| gpu.load_texture(grf, &format!("{UI}grade_enchant\\{name}.bmp"), false);
        let row = |name: &str| ["", "_over", "_pick"].map(|suffix| image(&format!("{name}{suffix}")));
        let digit = |index: usize, color: [u8; 3], size: f32| {
            let character = DIGITS.chars().nth(index).unwrap_or('0');
            gpu.texture(&font.render_sized(&character.to_string(), color, size), false)
        };
        Self {
            background: image("bg_grade_enchant"),
            frames: [1, 2, 3, 4, 5].map(|frame| image(&format!("bg_gradeitem{frame}"))),
            material: row("btn_material"),
            start: ["", "_over", "_press", "_disable"].map(|suffix| image(&format!("btn_start{suffix}"))),
            up: row("btn_chance_up"),
            down: row("btn_chance_down"),
            icons: [1, 2, 3, 4].map(|grade| image(&format!("grade_icon{grade}"))),
            wheels: [1, 2, 3].map(|frame| image(&format!("gradewheel{frame}"))),
            blessing: image("btn_unbroken"),
            caution: image("caution"),
            chance_digits: std::array::from_fn(|index| digit(index, WHITE, CHANCE_SIZE)),
            chance_outline: std::array::from_fn(|index| digit(index, TEXT, CHANCE_SIZE)),
            zeny_digits: std::array::from_fn(|index| digit(index, TEXT, ZENY_SIZE)),
        }
    }
}

/// A number from pre-drawn digits, bold (each drawn twice, a pixel apart); `outline` under it when given.
fn draw_digits<'a>(layer: &mut Layer<'a>, digits: &'a [Texture; 11], outline: Option<&'a [Texture; 11]>, text: &str, at: [f32; 2]) {
    let glyphs: Vec<(usize, f32)> = text
        .chars()
        .filter_map(|character| DIGITS.find(character))
        .scan(at[0], |x, glyph| {
            let left = *x;
            *x += digits[glyph].size[0];
            Some((glyph, left))
        })
        .collect();
    // The whole outline first, so no glyph's outline covers the one before it.
    if let Some(outline) = outline {
        for &(glyph, x) in &glyphs {
            for step in OUTLINE {
                layer.sprites.push((&outline[glyph], [x + step[0], at[1] + step[1]], digits[glyph].size));
            }
        }
    }
    for &(glyph, x) in &glyphs {
        for bold in [0.0, 1.0] {
            layer.sprites.push((&digits[glyph], [x + bold, at[1]], digits[glyph].size));
        }
    }
}

/// How wide `draw_digits` draws a number, its bold pixel included.
fn digits_width(digits: &[Texture; 11], text: &str) -> f32 {
    text.chars().filter_map(|character| DIGITS.find(character)).map(|glyph| digits[glyph].size[0]).sum::<f32>() + 1.0
}

/// Text drawn bold: twice, a pixel apart.
fn bold(layer: &mut Layer, text: &str, at: [f32; 2], color: [u8; 3]) {
    layer.texts.push((text.to_owned(), at, color));
    layer.texts.push((text.to_owned(), offset(at, [1.0, 0.0]), color));
}

/// Text drawn bold in white over a black outline a pixel round it.
fn outlined(layer: &mut Layer, text: &str, at: [f32; 2]) {
    for step in OUTLINE {
        layer.texts.push((text.to_owned(), offset(at, step), TEXT));
    }
    bold(layer, text, at, WHITE);
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

/// The notice box's lines, at most three, each a message and whether it's green (else red). Empty while a result
/// plays; the three fixed lines with no item in; with one, why it can't go and what may happen, in the game's order.
pub fn notices(
    item_in: bool,
    working: bool,
    list_chance: u32,
    row: Option<&GradeMaterial>,
    carried: u32,
    zeny: u32,
    heavy: bool,
    shown: u32,
) -> Vec<(usize, bool)> {
    if working {
        return Vec::new();
    }
    if !item_in {
        return EMPTY_NOTICES.to_vec();
    }
    if list_chance == 0 {
        return vec![(REFINE_TOO_LOW, false)];
    }
    let mut lines = Vec::new();
    match row {
        Some(row) if !row.can_break => lines.push((WONT_BREAK, true)),
        None => lines.push((NO_ROW, false)),
        Some(_) => {}
    }
    if let Some(row) = row {
        if carried < row.amount {
            lines.push((SHORT_MATERIAL, false));
        }
        if zeny < row.zeny {
            lines.push((NO_MATERIALS, false));
        }
    }
    if heavy {
        lines.push((HEAVY, false));
    }
    if let Some(row) = row {
        if row.can_break {
            lines.push(if shown < 100 { (CAN_BREAK, false) } else { (WONT_BREAK, true) });
        }
        if row.lowers_refine {
            lines.push((LOWERS_REFINE, false));
        }
    }
    lines.truncate(3);
    lines
}

/// The machine's frame and the wheel's, `elapsed` into the window's state: 0-based.
pub fn frames(elapsed: Duration, working: bool) -> (usize, usize) {
    let machine = (elapsed.as_millis() / MACHINE_STEP.as_millis()) as usize % if working { 4 } else { 5 };
    let wheel = if working { (elapsed.as_millis() / WHEEL_STEP.as_millis()) as usize % 3 } else { 0 };
    (machine, wheel)
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
    /// A try's result waiting for its line, and when the line comes.
    result: Option<(u32, Instant)>,
    /// When it opened: the machine's animation counts from then.
    opened: Instant,
}

impl GradeEnchant {
    /// The window on a `screen`-sized screen: 100 px left of centre.
    pub fn new(screen: [f32; 2], now: Instant) -> Self {
        let mut frame = Frame::new([((screen[0] - SIZE[0]) / 2.0 - 100.0).floor(), ((screen[1] - SIZE[1]) / 2.0).floor()], SIZE[0]);
        frame.open = true;
        Self { frame, item: None, chance: 0, blessing: GradeBlessing::default(), materials: Vec::new(), steps: 0, picked: None, result: None, opened: now }
    }

    /// Where the inventory goes beside it: right of the window, its bottom on the window's.
    pub fn inventory_place(&self, inventory_height: f32) -> [f32; 2] {
        [self.frame.position[0] + SIZE[0], self.frame.position[1] + SIZE[1] - inventory_height]
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

    /// A try's result: its line, and the window emptied, come `RESULT_DELAY` later (`result_due`). Nothing is sent: the
    /// item is asked about again only when one is put in.
    pub fn finished(&mut self, result: u32, now: Instant) {
        self.result = Some((result, now + RESULT_DELAY));
    }

    /// The result whose time has come: the window empties, and the caller prints its line.
    pub fn result_due(&mut self, now: Instant) -> Option<u32> {
        let (result, _) = self.result.filter(|&(_, at)| now >= at)?;
        self.clear();
        self.result = None;
        Some(result)
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
        let row = (0..self.materials.len().min(ROWS)).find(|&row| contains(row_at(row), ROW_SIZE, local));
        Some(match row {
            _ if contains(CLOSE_AT, BUTTON_SIZE, local) || contains(CLOSE_TEXT_AT, CLOSE_TEXT_SIZE, local) => Hit::Title(TitleHit::Close),
            _ if local[0] > DRAG_STRIP[0] && local[1] < DRAG_STRIP[1] => Hit::Title(TitleHit::Title),
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
            Hit::Material(row) if row < self.materials.len() => self.picked = Some(row),
            Hit::BlessingUp => self.steps = (self.steps + 1).min(self.blessing.most),
            Hit::BlessingDown => self.steps = self.steps.saturating_sub(1),
            Hit::Start => {
                // ponytail: no try while the last one's result plays out.
                let index = self.item.filter(|_| self.result.is_none())?;
                return match start_request(self.picked, &self.materials, self.chance, self.steps, self.blessing.item, self.blessing.per_step, carried, zeny) {
                    Start::Go { material, steps } => Some(Action::Enchant { index, material, steps }),
                    Start::Refuse(id) => Some(Action::Refuse(id)),
                    Start::Off => None,
                };
            }
            Hit::Title(_) | Hit::Material(_) | Hit::Window => {}
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
        let mut boxes = vec![("close", place(CLOSE_AT, BUTTON_SIZE)), ("close_text", place(CLOSE_TEXT_AT, CLOSE_TEXT_SIZE))];
        for (row, name) in ["material_0", "material_1", "material_2"].into_iter().enumerate().take(self.materials.len()) {
            boxes.push((name, place(row_at(row), ROW_SIZE)));
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

    /// `heavy`: the weight is past 70 % of the limit.
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
        (zeny, heavy, now): (u32, bool, Instant),
    ) {
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        let at = |local: [f32; 2]| self.frame.at(local);
        let local = [pointer.0[0] - self.frame.position[0], pointer.0[1] - self.frame.position[1]];
        let over = |place: [f32; 2], size: [f32; 2]| contains(place, size, local);
        let held = |place: [f32; 2], size: [f32; 2]| if over(place, size) { 1 + usize::from(pointer.1) } else { 0 };
        let icon = |id: u32| items.get(&id).and_then(|info| icons.get(&inventory::icon_path(info, true))).and_then(Option::as_ref);
        let count = |id: u32| carried.iter().filter(|item| item.id == id).map(|item| u32::from(item.amount)).sum::<u32>();
        let amount_of =
            |id: u32, amount: u32| rustnarok_client::messages::fill(message(AMOUNT_OF), &[&inventory::item_name(items, id, true, 0), &amount.to_string()]);
        layer.sprites.push((&assets.background, at([0.0, 0.0]), SIZE));
        layer.sprites.push((chrome.button(2, over(CLOSE_AT, BUTTON_SIZE)), at(CLOSE_AT), BUTTON_SIZE));
        chrome.draw_text_button(layer, font, at(CLOSE_TEXT_AT), CLOSE_TEXT_SIZE[0], message(CLOSE_LABEL), held(CLOSE_TEXT_AT, CLOSE_TEXT_SIZE));

        let item = self.item.and_then(|index| carried.iter().find(|item| item.index == index));
        let working = self.result.map(|(_, due)| due - RESULT_DELAY);
        let (machine, wheel) = frames(now.saturating_duration_since(working.unwrap_or(self.opened)), working.is_some());
        layer.sprites.push((&assets.wheels[wheel], at(WHEEL_AT), WHEEL));
        let picked = self.picked.and_then(|row| self.materials.get(row));
        if let Some(item) = item {
            layer.sprites.push((&assets.frames[machine], at(MACHINE_AT), MACHINE_SIZE));
            if let Some(texture) = icon(item.id) {
                layer.sprites.push((texture, at(ITEM_ICON), [24.0, 24.0]));
            }
            if (1..=4).contains(&item.grade) {
                layer.sprites.push((&assets.icons[usize::from(item.grade - 1)], at(GRADE_ICON_AT), GRADE_ICON));
            }
            let chance = format!("{}%", self.chance_shown());
            let left = CHANCE_AT[0] + ((CHANCE_WIDTH - digits_width(&assets.chance_digits, &chance)) / 2.0).floor();
            draw_digits(layer, &assets.chance_digits, Some(&assets.chance_outline), &chance, at([left, CHANCE_AT[1]]));
            layer.sprites.push((&assets.blessing, at(BLESSING_AT), BLESSING_SIZE));
            let zeny = picked.map_or(0, |row| row.zeny).to_string();
            let width = digits_width(&assets.zeny_digits, &zeny);
            draw_digits(layer, &assets.zeny_digits, None, &zeny, at([ZENY_AT[0] - width, ZENY_AT[1]]));
        }
        for row in 0..ROWS {
            let place = row_at(row);
            let look = if item.is_none() {
                0
            } else if self.picked == Some(row) {
                2
            } else {
                usize::from(over(place, ROW_SIZE))
            };
            layer.sprites.push((&assets.material[look], at(place), ROW_SIZE));
            let Some(material) = self.materials.get(row) else { continue };
            if let Some(texture) = icon(material.item) {
                layer.sprites.push((texture, at(offset(place, ROW_ICON)), [24.0, 24.0]));
            }
            layer.texts.push((amount_of(material.item, material.amount), at(offset(place, ROW_TEXT)), TEXT));
        }
        if item.is_some() && self.blessing.item != 0 {
            if let Some(texture) = icon(self.blessing.item) {
                layer.sprites.push((texture, at(BLESSING_ICON), [24.0, 24.0]));
            }
            outlined(layer, &amount_of(self.blessing.item, self.steps * self.blessing.per_step), at(BLESSING_TEXT));
        }
        layer.sprites.push((&assets.up[held(UP_AT, STEP_SIZE)], at(UP_AT), STEP_SIZE));
        layer.sprites.push((&assets.down[held(DOWN_AT, STEP_SIZE)], at(DOWN_AT), STEP_SIZE));
        let start = if self.start_off() { 3 } else { held(START_AT, START_SIZE) };
        layer.sprites.push((&assets.start[start], at(START_AT), START_SIZE));

        let carried_of_row = picked.map_or(0, |row| count(row.item));
        let lines = notices(item.is_some(), working.is_some(), self.chance, picked, carried_of_row, zeny, heavy, self.chance_shown());
        for (line, (id, green)) in lines.into_iter().enumerate() {
            let place = offset(NOTICE_AT, [0.0, NOTICE_STEP * line as f32]);
            layer.sprites.push((&assets.caution, at(place), CAUTION));
            let text = window::fit(font, message(id), NOTICE_WIDTH);
            bold(layer, &text, at(offset(place, [NOTICE_TEXT, 0.0])), if green { GREEN } else { RED });
        }
    }
}

fn row_at(row: usize) -> [f32; 2] {
    [ROW_AT[0], ROW_AT[1] + ROW_STEP * row as f32]
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

    fn opened() -> GradeEnchant {
        GradeEnchant::new([1024.0, 768.0], Instant::now())
    }

    #[test]
    fn opens_left_of_centre_with_the_inventory_beside_it() {
        assert_eq!(opened().frame.position, [212.0, 219.0]);
        let big = GradeEnchant::new([1280.0, 1024.0], Instant::now());
        assert_eq!(big.frame.position, [340.0, 347.0]);
        assert_eq!(big.inventory_place(200.0), [740.0, 477.0]);
    }

    #[test]
    fn each_piece_takes_its_own_press() {
        let mut window = opened();
        window.listed(4, 80, blessing(1_000_337, 10, 1), vec![row(1_000_325, 1, 175_000); 3]);
        let hit = |local: [f32; 2]| window.hit(offset(window.frame.position, local));
        assert_eq!(hit([384.0, 20.0]), Some(Hit::Title(TitleHit::Close)));
        assert_eq!(hit([386.0, 318.0]), Some(Hit::Title(TitleHit::Close)));
        // The picture's title strip drags it; left of it, or below, it doesn't.
        assert_eq!(hit([142.0, 37.0]), Some(Hit::Title(TitleHit::Title)));
        assert_eq!(hit([141.0, 30.0]), Some(Hit::Window));
        assert_eq!(hit([300.0, 38.0]), Some(Hit::Window));
        for row in 0..3 {
            let top = 102.0 + 40.0 * row as f32;
            assert_eq!(hit([158.0, top]), Some(Hit::Material(row)));
            assert_eq!(hit([392.0, top + 36.0]), Some(Hit::Material(row)));
        }
        assert_eq!(hit([300.0, 140.0]), Some(Hit::Window));
        assert_eq!(hit([158.0, 225.0]), Some(Hit::BlessingUp));
        assert_eq!(hit([189.0, 239.0]), Some(Hit::BlessingUp));
        assert_eq!(hit([158.0, 240.0]), Some(Hit::BlessingDown));
        assert_eq!(hit([35.0, 291.0]), Some(Hit::Start));
        assert_eq!(hit([116.0, 318.0]), Some(Hit::Start));
        assert_eq!(hit([117.0, 291.0]), Some(Hit::Window));
        // A row the list doesn't have takes nothing.
        window.listed(4, 80, blessing(1_000_337, 10, 1), vec![row(1_000_325, 1, 175_000)]);
        assert_eq!(window.hit(offset(window.frame.position, [200.0, 150.0])), Some(Hit::Window));
    }

    #[test]
    fn the_notice_box_says_why_and_what_may_happen() {
        let breaks = row(1, 1, 100);
        let lowers = GradeMaterial { lowers_refine: true, can_break: false, ..breaks.clone() };
        let note = |row: Option<&GradeMaterial>, carried, zeny, heavy, shown| notices(true, false, 80, row, carried, zeny, heavy, shown);
        assert_eq!(notices(false, false, 0, None, 0, 0, false, 0), [(3870, false), (3871, true), (3872, true)]);
        assert_eq!(notices(true, true, 80, Some(&breaks), 1, 100, false, 80), []);
        assert_eq!(notices(true, false, 0, Some(&breaks), 0, 0, true, 0), [(3821, false)]);
        assert_eq!(note(None, 0, 0, false, 80), [(3707, false)]);
        assert_eq!(note(Some(&breaks), 0, 100, false, 80), [(3708, false), (3712, false)]);
        assert_eq!(note(Some(&breaks), 1, 99, false, 80), [(3709, false), (3712, false)]);
        assert_eq!(note(Some(&breaks), 1, 100, false, 100), [(3822, true)]);
        assert_eq!(note(Some(&lowers), 1, 100, false, 80), [(3822, true), (3713, false)]);
        // Short of everything: the first three.
        assert_eq!(note(Some(&lowers), 0, 0, true, 80), [(3822, true), (3708, false), (3709, false)]);
    }

    #[test]
    fn the_machine_and_the_wheel_turn() {
        let ms = Duration::from_millis;
        assert_eq!(frames(ms(0), false), (0, 0));
        assert_eq!(frames(ms(199), false), (0, 0));
        assert_eq!(frames(ms(850), false), (4, 0));
        assert_eq!(frames(ms(1000), false), (0, 0));
        // A result: the machine's short loop, the wheel spinning.
        assert_eq!(frames(ms(650), true), (3, 0));
        assert_eq!(frames(ms(850), true), (0, 2));
        assert_eq!(frames(ms(150), true), (0, 1));
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
        let mut window = opened();
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

        // A result: its line and the window emptied 3 s on, and no try meanwhile.
        window.listed(4, 100, blessing(1_000_337, 10, 1), vec![row(1_000_325, 1, 175_000)]);
        window.click(Hit::Material(0), &have, 175_000);
        let now = Instant::now();
        window.finished(2, now);
        assert_eq!(window.click(Hit::Start, &have, 175_000), None);
        assert_eq!(window.result_due(now + Duration::from_millis(2999)), None);
        assert_eq!(window.item, Some(4));
        assert_eq!(window.result_due(now + RESULT_DELAY), Some(2));
        assert_eq!(window.item, None);
        assert_eq!(window.result_due(now + RESULT_DELAY), None);

        let names: Vec<_> = {
            let mut listed = opened();
            listed.listed(4, 100, blessing(1_000_337, 10, 1), vec![row(1_000_325, 1, 175_000), row(1_000_325, 5, 875_000)]);
            listed.button_boxes().into_iter().map(|(name, _)| name).collect()
        };
        assert_eq!(names, ["close", "close_text", "material_0", "material_1", "blessing_down", "blessing_up", "start"]);
        assert_eq!(grade_letter(1), "D");
        assert_eq!(grade_letter(4), "A");
    }
}
