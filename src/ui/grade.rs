//! The grade enchant window: an item dropped on the machine, the materials that can raise its grade, blessings, and
//! Start.
//!
//! Textures live under `data\texture\유저인터페이스\grade_enchant\`. The window is its background picture, which has
//! its own title strip; no title text.

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
/// Start with no blessing chosen asks first.
pub const ASKS_WITHOUT_BLESSING: usize = 3823;
/// Another row picked with blessings chosen: they go back to 0.
const BLESSINGS_RESET: usize = 3831;
/// The up button's tooltip: the blessings one step takes.
const PER_REQUEST: usize = 3976;
/// The item goes in when let go in this box, and a double click in it takes it out; a press in the other picks it up.
const DROP_BOX: [f32; 2] = [53.0, 114.0];
const GRAB_BOX: [f32; 2] = [64.0, 125.0];
const ITEM_BOX: [f32; 2] = [46.0, 46.0];
/// The blessing's icon, for its description.
const BLESSING_HOVER: [f32; 2] = [24.0, 24.0];
/// The result lines' colour, and the "can't be upgraded" line's.
pub const RESULT_COLOR: [u8; 3] = [0x00, 0xFF, 0xFF];
pub const CANT_COLOR: [u8; 3] = [0xFF, 0xB0, 0x62];
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
    /// The box round the item's icon on the machine.
    Item,
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
    /// Ask before a try with no blessing (`ASKS_WITHOUT_BLESSING`); yes sends what `go` gives.
    Ask,
    /// A notice in the message box, OK only.
    Notice(usize),
    Close,
}

/// The chance on screen: the list's plus one share per step, and never past 100.
pub fn shown_chance(list: u32, steps: u32, per_step: u32) -> u32 {
    list.saturating_add(steps.saturating_mul(per_step)).min(100)
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

/// The effects a result plays on the player at once: the machine's, then success or failure.
pub fn result_effects(result: u32) -> [u32; 2] {
    [1607, if result == 0 { 1608 } else { 1609 }]
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
    /// The blessing items chosen: steps of the amount per step.
    chosen: u32,
    picked: Option<usize>,
    /// A try is out and its answer hasn't come.
    waiting: bool,
    clicks: window::Clicks,
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
        Self {
            frame,
            item: None,
            chance: 0,
            blessing: GradeBlessing::default(),
            materials: Vec::new(),
            chosen: 0,
            picked: None,
            waiting: false,
            clicks: window::Clicks::default(),
            result: None,
            opened: now,
        }
    }

    /// Where the inventory goes beside it: right of the window, its bottom on the window's.
    pub fn inventory_place(&self, inventory_height: f32) -> [f32; 2] {
        [self.frame.position[0] + SIZE[0], self.frame.position[1] + SIZE[1] - inventory_height]
    }

    /// The server's list for an item: it's in, with no row picked, no blessing and no try waiting.
    pub fn listed(&mut self, index: u16, chance: u32, blessing: GradeBlessing, materials: Vec<GradeMaterial>) {
        self.clear();
        self.item = Some(index);
        self.chance = chance;
        self.blessing = blessing;
        self.materials = materials;
    }

    /// Empties the window: the three fixed notice lines again.
    pub fn clear(&mut self) {
        self.item = None;
        self.picked = None;
        self.chosen = 0;
        self.waiting = false;
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

    /// Blessing steps: the items chosen over the amount per step.
    fn steps(&self) -> u32 {
        self.chosen.checked_div(self.blessing.per_step).unwrap_or(0)
    }

    pub fn chance_shown(&self) -> u32 {
        shown_chance(self.chance, self.steps(), self.blessing.chance)
    }

    /// Start can go: no try waiting or playing, an item in, a row picked with its material carried and its zeny, and a
    /// chance.
    pub fn start_ready(&self, carried: impl Fn(u32) -> u32, zeny: u32) -> bool {
        let row = self.picked.and_then(|row| self.materials.get(row));
        !self.waiting
            && self.result.is_none()
            && self.item.is_some()
            && self.chance != 0
            && row.is_some_and(|row| carried(row.item) >= row.amount && zeny >= row.zeny)
    }

    /// The try Start sends, now waited on.
    pub fn go(&mut self) -> Option<Action> {
        let index = self.item?;
        let material = self.picked? as u32;
        self.waiting = true;
        Some(Action::Enchant { index, material, steps: self.steps() })
    }

    /// An inventory item let go at `cursor` goes in: inside the box round the machine's icon, not while a result plays.
    pub fn takes_drop(&self, cursor: [f32; 2]) -> bool {
        self.result.is_none() && self.frame.local(cursor, HEIGHT).is_some_and(|local| contains(DROP_BOX, ITEM_BOX, local))
    }

    /// A press at `cursor` that picks the item up to take it out: its index.
    pub fn grab(&self, cursor: [f32; 2]) -> Option<u16> {
        self.item.filter(|_| self.result.is_none() && self.frame.local(cursor, HEIGHT).is_some_and(|local| contains(GRAB_BOX, ITEM_BOX, local)))
    }

    /// The item under the pointer, for its description: a material row's, the blessing's, or the one in.
    pub fn hovered(&self, cursor: [f32; 2], carried: &[InventoryItem]) -> Option<InventoryItem> {
        let local = self.frame.local(cursor, HEIGHT)?;
        let plain = |id| Some(InventoryItem { id, identified: true, ..Default::default() });
        if let Some(row) = (0..self.materials.len().min(ROWS)).find(|&row| contains(row_at(row), ROW_SIZE, local)) {
            return plain(self.materials[row].item);
        }
        if self.item.is_some() && self.blessing.item != 0 && contains(BLESSING_ICON, BLESSING_HOVER, local) {
            return plain(self.blessing.item);
        }
        let index = self.item.filter(|_| contains(DROP_BOX, ITEM_BOX, local))?;
        carried.iter().find(|item| item.index == index).cloned()
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
            None if contains(DROP_BOX, ITEM_BOX, local) => Hit::Item,
            None => Hit::Window,
        })
    }

    /// Reacts to a click. `carried` counts an item, `zeny` is the zeny carried.
    pub fn click(&mut self, hit: Hit, carried: impl Fn(u32) -> u32, zeny: u32) -> Option<Action> {
        let per_step = self.blessing.per_step;
        match hit {
            Hit::Title(TitleHit::Close) => return Some(Action::Close),
            // Another row resets the blessings, saying so.
            Hit::Material(row) if row < self.materials.len() && self.picked != Some(row) => {
                self.picked = Some(row);
                if self.chosen > 0 {
                    self.chosen = 0;
                    return Some(Action::Notice(BLESSINGS_RESET));
                }
            }
            // Up while under the most and enough are carried; down to 0.
            Hit::BlessingUp
                if self.blessing.item != 0
                    && per_step > 0
                    && self.chosen < self.blessing.most.saturating_mul(per_step)
                    && carried(self.blessing.item) >= self.chosen + per_step =>
            {
                self.chosen += per_step;
            }
            Hit::BlessingDown if self.blessing.item != 0 => self.chosen = self.chosen.saturating_sub(per_step),
            Hit::Start if self.start_ready(carried, zeny) => return if self.chosen == 0 { Some(Action::Ask) } else { self.go() },
            // A double click on the item takes it out; nothing is sent.
            Hit::Item if self.item.is_some() && self.result.is_none() && self.clicks.double(0) => self.clear(),
            Hit::Title(_) | Hit::Material(_) | Hit::BlessingUp | Hit::BlessingDown | Hit::Start | Hit::Item | Hit::Window => {}
        }
        None
    }

    /// The buttons on screen, named for scripted clicks: Start is `start_off` while it can't go.
    pub fn button_boxes(&self, carried: &[InventoryItem], zeny: u32) -> Vec<(&'static str, [f32; 4])> {
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
        boxes.push((if self.start_ready(count_in(carried), zeny) { "start" } else { "start_off" }, place(START_AT, START_SIZE)));
        boxes.push(("item", place(DROP_BOX, ITEM_BOX)));
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
        tooltip: &mut Layer<'a>,
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
        let count = count_in(carried);
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
            outlined(layer, &amount_of(self.blessing.item, self.chosen), at(BLESSING_TEXT));
        }
        layer.sprites.push((&assets.up[held(UP_AT, STEP_SIZE)], at(UP_AT), STEP_SIZE));
        layer.sprites.push((&assets.down[held(DOWN_AT, STEP_SIZE)], at(DOWN_AT), STEP_SIZE));
        let start = if self.start_ready(&count, zeny) { held(START_AT, START_SIZE) } else { 3 };
        if item.is_some() && over(UP_AT, STEP_SIZE) {
            let text = rustnarok_client::messages::fill(message(PER_REQUEST), &[&self.blessing.per_step]);
            window::tooltip(tooltip, chrome, font, &text, at(offset(UP_AT, [0.0, -20.0])), WHITE);
        }
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

/// How many of an item are carried, over every stack.
fn count_in(carried: &[InventoryItem]) -> impl Fn(u32) -> u32 + '_ {
    |id| carried.iter().filter(|item| item.id == id).map(|item| u32::from(item.amount)).sum()
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
    fn start_goes_only_when_it_can_and_asks_without_blessings() {
        let mut window = opened();
        let rows = vec![row(1_000_325, 1, 175_000), row(1_000_325, 5, 875_000)];
        window.listed(4, 80, blessing(1_000_337, 10, 1), rows);
        let have = carried(&[(1_000_325, 1), (1_000_337, 2)]);
        // No row, too few materials, too little zeny: Start is off and a press does nothing.
        assert!(!window.start_ready(&have, 175_000));
        assert_eq!(window.click(Hit::Start, &have, 175_000), None);
        window.click(Hit::Material(1), &have, 875_000);
        assert!(!window.start_ready(&have, 875_000));
        window.click(Hit::Material(0), &have, 175_000);
        assert!(!window.start_ready(&have, 174_999));
        assert!(window.start_ready(&have, 175_000));
        // No blessing: it asks; yes sends and waits, which turns Start off.
        assert_eq!(window.click(Hit::Start, &have, 175_000), Some(Action::Ask));
        assert_eq!(window.go(), Some(Action::Enchant { index: 4, material: 0, steps: 0 }));
        assert!(!window.start_ready(&have, 175_000));
        // A new list: no row, no blessing, nothing waiting.
        window.listed(4, 80, blessing(1_000_337, 10, 1), vec![row(1_000_325, 1, 175_000)]);
        assert_eq!((window.picked, window.chosen, window.waiting), (None, 0, false));
        window.click(Hit::Material(0), &have, 175_000);
        // With blessings it sends at once, steps = chosen / per step.
        window.click(Hit::BlessingUp, &have, 175_000);
        assert_eq!(window.click(Hit::Start, &have, 175_000), Some(Action::Enchant { index: 4, material: 0, steps: 1 }));
        // A chance of 0: off.
        window.listed(4, 0, blessing(1_000_337, 10, 1), vec![row(1_000_325, 1, 175_000)]);
        window.click(Hit::Material(0), &have, 175_000);
        assert!(!window.start_ready(&have, 175_000));
    }

    #[test]
    fn blessings_stop_at_what_is_carried_and_reset_with_another_row() {
        let mut window = opened();
        window.listed(4, 80, GradeBlessing { item: 1_000_337, per_step: 2, most: 3, chance: 1 }, vec![row(1_000_325, 1, 1); 2]);
        window.click(Hit::Material(0), carried(&[]), 1);
        let have = carried(&[(1_000_337, 5)]);
        for _ in 0..4 {
            window.click(Hit::BlessingUp, &have, 1);
        }
        // Two steps of 2: a third would need 6 carried.
        assert_eq!((window.chosen, window.steps(), window.chance_shown()), (4, 2, 82));
        let plenty = carried(&[(1_000_337, 99)]);
        for _ in 0..4 {
            window.click(Hit::BlessingUp, &plenty, 1);
        }
        // The most is 3 steps.
        assert_eq!(window.chosen, 6);
        for _ in 0..4 {
            window.click(Hit::BlessingDown, &plenty, 1);
        }
        assert_eq!(window.chosen, 0);
        window.click(Hit::BlessingUp, &plenty, 1);
        // The same row again changes nothing; another resets the blessings and says so.
        assert_eq!(window.click(Hit::Material(0), &plenty, 1), None);
        assert_eq!(window.chosen, 2);
        assert_eq!(window.click(Hit::Material(1), &plenty, 1), Some(Action::Notice(BLESSINGS_RESET)));
        assert_eq!((window.chosen, window.picked), (0, Some(1)));
        assert_eq!(window.click(Hit::Material(0), &plenty, 1), None);
        // No blessing item in the list: neither button does anything.
        window.listed(4, 80, GradeBlessing::default(), vec![row(1_000_325, 1, 1)]);
        window.click(Hit::BlessingUp, &plenty, 1);
        assert_eq!(window.chosen, 0);
    }

    #[test]
    fn the_item_goes_in_on_its_box_and_comes_out_by_double_click() {
        let mut window = opened();
        let at = |window: &GradeEnchant, local: [f32; 2]| offset(window.frame.position, local);
        assert!(window.takes_drop(at(&window, [53.0, 114.0])));
        assert!(window.takes_drop(at(&window, [98.0, 159.0])));
        assert!(!window.takes_drop(at(&window, [52.0, 130.0])));
        assert!(!window.takes_drop(at(&window, [80.0, 160.0])));
        assert_eq!(window.grab(at(&window, [70.0, 130.0])), None);
        window.listed(4, 80, blessing(1_000_337, 10, 1), vec![row(1_000_325, 1, 175_000)]);
        assert_eq!(window.grab(at(&window, [64.0, 125.0])), Some(4));
        assert_eq!(window.grab(at(&window, [109.0, 170.0])), Some(4));
        assert_eq!(window.grab(at(&window, [63.0, 130.0])), None);
        assert_eq!(window.grab(at(&window, [80.0, 171.0])), None);
        let item = InventoryItem { index: 4, id: 15282, ..Default::default() };
        assert_eq!(window.hovered(at(&window, [60.0, 120.0]), std::slice::from_ref(&item)).map(|item| item.id), Some(15282));
        assert_eq!(window.hovered(at(&window, [200.0, 110.0]), &[]).map(|item| item.id), Some(1_000_325));
        assert_eq!(window.hovered(at(&window, [196.0, 229.0]), &[]).map(|item| item.id), Some(1_000_337));
        // One click does nothing; two empty it, and nothing is sent.
        let hit = window.hit(at(&window, [70.0, 130.0])).unwrap();
        assert_eq!(hit, Hit::Item);
        assert_eq!(window.click(hit, carried(&[]), 0), None);
        assert_eq!(window.item, Some(4));
        assert_eq!(window.click(hit, carried(&[]), 0), None);
        assert_eq!((window.item, window.materials.len()), (None, 0));
        // While a result plays nothing goes in or out.
        window.listed(4, 80, blessing(1_000_337, 10, 1), vec![row(1_000_325, 1, 175_000)]);
        window.finished(0, Instant::now());
        assert!(!window.takes_drop(at(&window, [70.0, 130.0])));
        assert_eq!(window.grab(at(&window, [70.0, 130.0])), None);
    }

    #[test]
    fn a_result_plays_its_effects_then_empties_the_window() {
        assert_eq!([0, 1, 2, 3, 4].map(result_effects), [[1607, 1608], [1607, 1609], [1607, 1609], [1607, 1609], [1607, 1609]]);
        let mut window = opened();
        let have = carried(&[(1_000_325, 1)]);
        window.listed(4, 100, blessing(1_000_337, 10, 1), vec![row(1_000_325, 1, 175_000)]);
        window.click(Hit::Material(0), &have, 175_000);
        let now = Instant::now();
        window.finished(2, now);
        assert!(!window.start_ready(&have, 175_000));
        assert_eq!(window.result_due(now + Duration::from_millis(2999)), None);
        assert_eq!(window.item, Some(4));
        assert_eq!(window.result_due(now + RESULT_DELAY), Some(2));
        assert_eq!(window.item, None);
        assert_eq!(window.result_due(now + RESULT_DELAY), None);
        let names: Vec<_> = {
            let mut listed = opened();
            listed.listed(4, 100, blessing(1_000_337, 10, 1), vec![row(1_000_325, 1, 175_000), row(1_000_325, 5, 875_000)]);
            listed.button_boxes(&[], 0).into_iter().map(|(name, _)| name).collect()
        };
        assert_eq!(names, ["close", "close_text", "material_0", "material_1", "blessing_down", "blessing_up", "start_off", "item"]);
        assert_eq!(grade_letter(1), "D");
        assert_eq!(grade_letter(4), "A");
    }
}
