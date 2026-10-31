//! The player's homunculus: its window (Alt+R), its skill list, the words for its intimacy, when it says it's
//! starving.

use std::collections::HashMap;

use rustnarok_client::grf::Grf;
use rustnarok_client::lua::SkillInfo;
use rustnarok_client::net::{Homunculus, Skill};
use rustnarok_client::text::Font;

use crate::mercenary::draw_values;
use crate::render::{Gpu, Texture};
use crate::window::{self, Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI, contains, messages, skills};

const TITLE: usize = 936;
const SKILLS_TITLE: usize = 937;
const NAME: usize = 407;
const SKILL_POINTS: usize = 284;
const LEVEL: usize = 408;
const HUNGER: usize = 585;
const INTIMACY: usize = 586;
const EXP: usize = 1019;
/// "receives 10% of master's experience", red in the white band.
const NOTE: usize = 3284;
const NOTE_COLOR: [u8; 3] = [0xFF, 0, 0];
/// The automatic feeding box's two-line label.
const AUTO_FEED: usize = 3279;
/// The starving line.
pub const STARVING: usize = 1018;
/// A new name: too long, or holding an item link (boxes); else asked with the name.
pub const NAME_TOO_LONG: usize = 938;
pub const RENAME_ASKS: usize = 2977;
pub const FEED_ASKS: usize = 601;
pub const DELETE_ASKS: usize = 955;
/// Feeding refused while trading.
pub const FEED_WHILE_TRADING: usize = 3551;
/// Fed: "[food] - n remaining", cyan; not fed, red.
pub const FED: usize = 3281;
pub const FED_COLOR: [u8; 3] = [0x64, 0xFF, 0xFF];
pub const NOT_FED: usize = 591;
pub const NOT_FED_COLOR: [u8; 3] = [0xF5, 0x1E, 0x1E];
/// The server's word on automatic feeding: on, off.
pub const AUTO_FEED_ON: usize = 3282;
pub const AUTO_FEED_OFF: usize = 3283;
pub const AUTO_FEED_COLOR: [u8; 3] = [0xFF, 0xFF, 0x64];

/// The message naming how fond it is (rathena's `intimacy_grades`, divided by 100).
pub fn intimacy_word(intimacy: u16) -> usize {
    match intimacy {
        911.. => 675,
        751..=910 => 674,
        251..=750 => 669,
        101..=250 => 673,
        11..=100 => 672,
        4..=10 => 1021,
        _ => 1022,
    }
}

/// A message's lines: the table writes its breaks as "\n".
fn text_lines(text: &str) -> impl Iterator<Item = &str> {
    text.split("\\n").flat_map(str::lines)
}

/// Whether the homunculus's full status says "starving": hunger at exactly 1, 5 or 10, and not the hunger last
/// warned about.
pub fn starving_now(warned: Option<u16>, hunger: u16) -> bool {
    matches!(hunger, 1 | 5 | 10) && warned != Some(hunger)
}

/// Places from the window's corner (its title bar included).
const WIDTH: f32 = 280.0;
const HEIGHT: f32 = 208.0;
const POSITION: [f32; 2] = [150.0, 100.0];
/// The white band under the background, and the note in it.
const BAND_TOP: f32 = 168.0;
const NOTE_AT: [f32; 2] = [8.0, 180.0];
const LABEL: f32 = 100.0;
const VALUE: f32 = 135.0;
const NAME_TOP: f32 = 24.0;
const NAME_BOX: [f32; 4] = [135.0, 22.0, 80.0, 16.0];
const LEVEL_TOP: f32 = 47.0;
const BUTTON: [f32; 2] = [42.0, 20.0];
const RENAME: [f32; 2] = [232.0, 21.0];
const DELETE: [f32; 2] = [187.0, 44.0];
const SKILLS: [f32; 2] = [232.0, 44.0];
const FEED: [f32; 2] = [210.0, 105.0];
const FEED_SIZE: [f32; 2] = [64.0, 20.0];
/// HP and SP: 157 × 9 each; the fill's left end is 4 px, its middle starts there.
const HP_BAR: [f32; 2] = [117.0, 73.0];
const SP_BAR: [f32; 2] = [117.0, 88.0];
const BAR: [f32; 2] = [157.0, 9.0];
const FILL_END: f32 = 4.0;
const EXP_TOP: f32 = 105.0;
const EXP_RIGHT: f32 = 205.0;
const EXP_BAR: [f32; 4] = [101.0, 119.0, 102.0, 6.0];
const HUNGER_TOP: f32 = 131.0;
const HUNGER_RIGHT: f32 = 200.0;
const HUNGER_BAR: [f32; 4] = [101.0, 145.0, 102.0, 6.0];
const AUTO_BOX: [f32; 2] = [210.0, 142.0];
const CHECK: [f32; 2] = [12.0, 12.0];
const AUTO_LABEL: [f32; 2] = [225.0, 136.0];
const INTIMACY_TOP: f32 = 159.0;
const COLON: f32 = 140.0;
const WORD: f32 = 150.0;
const TEXT: [u8; 3] = [0, 0, 0];
const BAR_FRAME: [u8; 4] = [0xAF, 0xAF, 0xAF, 0xFF];
const BAR_BLUE: [u8; 4] = [0x56, 0x7F, 0xCF, 0xFF];
const BAR_RED: [u8; 4] = [0xFF, 0x00, 0x00, 0xFF];
/// The exp a level with no next one asks for.
const NO_NEXT: i64 = 99_999_999;

pub struct Assets {
    background: Option<Texture>,
    /// The HP and SP bars' track, blue and red fills: left, middle, right.
    track: [Texture; 3],
    fills: [[Texture; 3]; 2],
    /// Normal, hovered, pressed.
    rename: [Texture; 3],
    delete: [Texture; 3],
    skill: [Texture; 3],
    feed: [Texture; 3],
    check: [Texture; 2],
    bar_frame: Texture,
    bar_blue: Texture,
    bar_red: Texture,
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let basic = |name: &str| gpu.load_texture(grf, &format!("{UI}basic_interface\\{name}.bmp"), false);
        let button = |name: &str| ["", "_a", "_b"].map(|suffix| gpu.load_texture(grf, &format!("{UI}btn_{name}{suffix}.bmp"), false));
        let pieces = |color: &str| ["left", "mid", "right"].map(|part| basic(&format!("gze{color}_{part}")));
        Self {
            background: gpu.try_load_texture(grf, &format!("{UI}basic_interface\\homuninfo_bg.bmp"), false).ok(),
            track: ["gze_bg_left", "gze_bg_mid", "gze_bg_right"].map(basic),
            fills: [pieces("blue"), pieces("red")],
            rename: button("rewrite"),
            delete: button("del"),
            skill: button("skill"),
            feed: button("feed"),
            check: ["checkbox_0", "checkbox_1"].map(|name| gpu.load_texture(grf, &format!("{UI}{name}.bmp"), false)),
            bar_frame: gpu.solid(BAR_FRAME),
            bar_blue: gpu.solid(BAR_BLUE),
            bar_red: gpu.solid(BAR_RED),
        }
    }
}

/// An HP or SP bar: nothing past its maximum; else the track, and unless the maximum or the value is 0, the fill's
/// middle width and whether it's red (under a quarter).
#[derive(Debug, PartialEq)]
pub enum Gauge {
    Hidden,
    Empty,
    Filled(f32, bool),
}

pub fn gauge(value: u32, most: u32, width: f32) -> Gauge {
    if value > most {
        Gauge::Hidden
    } else if value == 0 || most == 0 {
        Gauge::Empty
    } else {
        let middle = ((width - 2.0 * FILL_END) * value as f32 / most as f32).trunc();
        Gauge::Filled(middle, u64::from(value) * 4 < u64::from(most))
    }
}

/// The numbers over an HP or SP bar: three wide, two spaces each side of the slash.
pub fn gauge_text(value: u32, most: u32) -> String {
    format!("{value:>3}  /  {most:>3}")
}

/// The exp bar's fill width, or None when there's no bar: exp past the next level's, or no next level.
pub fn exp_fill(exp: i64, next: i64) -> Option<f32> {
    (exp <= next && next != 0 && next != NO_NEXT).then(|| ((EXP_BAR[2] - 2.0) * exp.max(0) as f32 / next as f32).trunc())
}

/// The hunger bar's fill width and whether it's red (under 25), or None past 100.
pub fn hunger_fill(hunger: u16) -> Option<(f32, bool)> {
    (hunger <= 100).then(|| (((HUNGER_BAR[2] - 2.0) * f32::from(hunger) / 100.0).trunc(), hunger < 25))
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Close,
    Rename,
    /// The name box: typing goes to it.
    NameBox,
    Delete,
    Skills,
    Feed,
    AutoFeed,
    /// Anywhere else: the window drags from it.
    Window,
}

pub struct Window {
    pub frame: Frame,
    /// The name box's text once it's been typed in (None: the homunculus's name), and whether typing goes to it.
    pub name_text: Option<String>,
    pub typing: bool,
    /// The automatic feeding box, as the server last said.
    pub auto_feed: bool,
}

impl Default for Window {
    fn default() -> Self {
        Self { frame: Frame::new(POSITION, WIDTH), name_text: None, typing: false, auto_feed: false }
    }
}

impl Window {
    pub fn height() -> f32 {
        HEIGHT
    }

    /// The name the box shows: what's typed, else the homunculus's.
    pub fn name<'a>(&'a self, homunculus: &'a Homunculus) -> &'a str {
        self.name_text.as_deref().unwrap_or(&homunculus.name)
    }

    const PARTS: [(Hit, [f32; 2], [f32; 2]); 6] = [
        (Hit::Rename, RENAME, BUTTON),
        (Hit::NameBox, [NAME_BOX[0], NAME_BOX[1]], [NAME_BOX[2], NAME_BOX[3]]),
        (Hit::Delete, DELETE, BUTTON),
        (Hit::Skills, SKILLS, BUTTON),
        (Hit::Feed, FEED, FEED_SIZE),
        (Hit::AutoFeed, AUTO_BOX, CHECK),
    ];

    /// The buttons and boxes on screen (for the test driver).
    pub fn button_boxes(&self) -> Vec<(&'static str, [f32; 4])> {
        if !self.frame.open {
            return Vec::new();
        }
        let names = ["rename", "homunculus_name", "delete", "skill", "feed", "homunculus_auto_feed"];
        names
            .into_iter()
            .zip(Self::PARTS)
            .map(|(name, (_, at, size))| {
                let [x, y] = self.frame.at(at);
                (name, [x, y, size[0], size[1]])
            })
            .collect()
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let local = self.frame.local(cursor, HEIGHT)?;
        if self.frame.title_hit(local) == Some(TitleHit::Close) {
            return Some(Hit::Close);
        }
        Some(Self::PARTS.iter().find(|(_, at, size)| contains(*at, *size, local)).map_or(Hit::Window, |&(hit, ..)| hit))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &'a self,
        layer: &mut Layer<'a>,
        tooltip: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        font: &Font,
        messages: &[String],
        homunculus: &Homunculus,
        pointer: ([f32; 2], bool),
        blink: bool,
    ) {
        if !self.frame.open {
            return;
        }
        let frame = &self.frame;
        let at = |local: [f32; 2]| frame.at(local);
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        let text = |layer: &mut Layer<'a>, text: String, place: [f32; 2]| layer.texts.push((text, at(place), TEXT));
        let right = |layer: &mut Layer<'a>, value: String, edge: f32, top: f32| {
            let x = (edge - font.width(&value)).floor();
            layer.texts.push((value, at([x, top]), TEXT));
        };
        let hit = self.hit(pointer.0);
        let look = |target: Hit| {
            let hovered = hit == Some(target);
            usize::from(hovered) + usize::from(hovered && pointer.1)
        };
        // The white band with its red note, the background (bringing its title strip) over it.
        layer.sprites.push((&chrome.white, at([0.0, BAND_TOP]), [WIDTH, HEIGHT - BAND_TOP]));
        match &assets.background {
            Some(background) => layer.sprites.push((background, at([0.0, 0.0]), background.size)),
            None => chrome.draw_title_bar(layer, at([0.0, 0.0]), WIDTH),
        }
        for (row, line) in text_lines(message(NOTE)).enumerate() {
            layer.texts.push((line.to_owned(), at([NOTE_AT[0], NOTE_AT[1] + row as f32 * font.line_height()]), NOTE_COLOR));
        }
        frame.draw_bare_title(layer, chrome, message(TITLE), pointer.0);
        let h = homunculus;
        draw_values(layer, frame, font, [h.atk, h.matk, h.hit, h.critical, h.def, h.mdef, h.flee], h.aspd);
        // Name (edited in place) and level, the buttons.
        text(layer, message(NAME).to_owned(), [LABEL, NAME_TOP]);
        let [x, y, w, h_box] = NAME_BOX;
        let shown = if self.typing && blink { format!("{}|", self.name(h)) } else { self.name(h).to_owned() };
        text(layer, window::fit(font, &shown, w - 4.0), [x + 2.0, y + ((h_box - font.line_height()) / 2.0).floor()]);
        text(layer, message(LEVEL).to_owned(), [LABEL, LEVEL_TOP]);
        text(layer, h.level.to_string(), [VALUE, LEVEL_TOP]);
        for (images, target, place, size) in [
            (&assets.rename, Hit::Rename, RENAME, BUTTON),
            (&assets.delete, Hit::Delete, DELETE, BUTTON),
            (&assets.skill, Hit::Skills, SKILLS, BUTTON),
            (&assets.feed, Hit::Feed, FEED, FEED_SIZE),
        ] {
            layer.sprites.push((&images[look(target)], at(place), size));
        }
        // HP and SP.
        for (place, (value, most)) in [(HP_BAR, (h.hp, h.max_hp)), (SP_BAR, (h.sp, h.max_sp))] {
            let gauge = gauge(value, most, BAR[0]);
            if gauge == Gauge::Hidden {
                continue;
            }
            let [left, middle, end] = &assets.track;
            layer.sprites.push((left, at(place), [left.size[0], BAR[1]]));
            layer.sprites.push((middle, at([place[0] + left.size[0], place[1]]), [BAR[0] - left.size[0] - end.size[0], BAR[1]]));
            layer.sprites.push((end, at([place[0] + BAR[0] - end.size[0], place[1]]), [end.size[0], BAR[1]]));
            if let Gauge::Filled(width, red) = gauge {
                let [left, middle, end] = &assets.fills[usize::from(red)];
                layer.sprites.push((left, at(place), [FILL_END, BAR[1]]));
                layer.sprites.push((middle, at([place[0] + FILL_END, place[1]]), [width, BAR[1]]));
                layer.sprites.push((end, at([place[0] + FILL_END + width, place[1]]), [end.size[0], BAR[1]]));
                // ponytail: centred on the bar.
                let numbers = gauge_text(value, most);
                let x = place[0] + ((BAR[0] - font.width(&numbers)) / 2.0).floor();
                text(layer, numbers, [x, place[1] - 2.0]);
            }
        }
        // Exp: the next level's total, its bar.
        text(layer, message(EXP).to_owned(), [LABEL, EXP_TOP]);
        right(layer, h.next_exp.to_string(), EXP_RIGHT, EXP_TOP - 1.0);
        self.small_bar(layer, assets, chrome, EXP_BAR, exp_fill(h.exp, h.next_exp).map(|width| (width, false)));
        // Hunger: "n / 100", its bar.
        text(layer, message(HUNGER).to_owned(), [LABEL, HUNGER_TOP]);
        right(layer, format!("{} / 100", h.hunger), HUNGER_RIGHT, HUNGER_TOP - 1.0);
        if let Some(fill) = hunger_fill(h.hunger) {
            self.small_bar(layer, assets, chrome, HUNGER_BAR, Some(fill));
        }
        // Resting on the exp or hunger bar shows how full it is, 16 px above its corner.
        let tips = [
            (EXP_BAR, crate::basic_info::exp_tip(h.exp, h.next_exp)),
            (HUNGER_BAR, crate::basic_info::exp_tip(i64::from(h.hunger), 100)),
        ];
        for ([x, y, width, height], tip) in tips {
            if let Some(tip) = tip.filter(|_| contains(at([x, y]), [width, height], pointer.0)) {
                crate::window::tooltip(tooltip, chrome, font, &tip, at([x, y - 16.0]), [255; 3]);
            }
        }
        // Automatic feeding.
        let check = &assets.check[usize::from(self.auto_feed)];
        layer.sprites.push((check, at(AUTO_BOX), CHECK));
        for (row, line) in text_lines(message(AUTO_FEED)).enumerate() {
            text(layer, line.to_owned(), [AUTO_LABEL[0], AUTO_LABEL[1] + row as f32 * font.line_height()]);
        }
        // Intimacy (the word keeps its leading space).
        text(layer, message(INTIMACY).to_owned(), [LABEL, INTIMACY_TOP]);
        text(layer, ":".to_owned(), [COLON, INTIMACY_TOP]);
        text(layer, message(intimacy_word(h.intimacy)).trim().to_owned(), [WORD, INTIMACY_TOP]);
    }

    /// A thin bar: the grey frame, white inside, the fill (`None`: no bar).
    fn small_bar<'a>(&self, layer: &mut Layer<'a>, assets: &'a Assets, chrome: &'a Chrome, [x, y, w, h]: [f32; 4], fill: Option<(f32, bool)>) {
        let Some((width, red)) = fill else { return };
        layer.sprites.push((&assets.bar_frame, self.frame.at([x, y]), [w, h]));
        layer.sprites.push((&chrome.white, self.frame.at([x + 1.0, y + 1.0]), [w - 2.0, h - 2.0]));
        if width > 0.0 {
            layer.sprites.push((if red { &assets.bar_red } else { &assets.bar_blue }, self.frame.at([x + 1.0, y + 1.0]), [width, h - 2.0]));
        }
    }
}

/// The homunculus's and the mercenary's skill lists: one column of 32-px rows with the level and SP, a band on the
/// selected row, Use and Close at the bottom, a corner grip for the height. Places from the list's corner, title
/// bar included.
const LIST_WIDTH: f32 = 280.0;
const LIST_POSITION: [f32; 2] = [360.0, 200.0];
/// Heights: 120 + 32k, up to 280; 216 at first.
const LEAST_HEIGHT: f32 = 120.0;
const MOST_STEPS: usize = 5;
const FIRST_HEIGHT: f32 = 216.0;
const LIST_ROW: f32 = 32.0;
/// In row r (y = 32r): the icon, its level up button, the band, the name, "Lv : n", "Sp : n".
const ROW_ICON: [f32; 2] = [44.0, 21.0];
const ROW_UP: [f32; 2] = [78.0, 21.0];
const ROW_BAND: [f32; 4] = [104.0, 19.0, 164.0, 28.0];
const ROW_NAME: [f32; 2] = [112.0, 22.0];
const ROW_LEVEL: [f32; 2] = [113.0, 34.0];
const ROW_SP: [f32; 2] = [212.0, 34.0];
const ICON_SIZE: f32 = 24.0;
// ponytail: the name stops at the band's right edge.
const NAME_END: f32 = 268.0;
const UNLEARNED: [u8; 3] = [0x78, 0x78, 0x78];
/// The band: learned, not learned, at 0 SP.
const BANDS: [[u8; 4]; 3] = [[0x70, 0x9F, 0xED, 0xFF], [0xB4, 0xB4, 0xB4, 0xFF], [0x70, 0xD7, 0xED, 0xFF]];
const PASSIVE: usize = 286;
const TITLE_SHADOW: [f32; 2] = [17.0, 4.0];
const TITLE_AT: [f32; 2] = [16.0, 3.0];
/// From the bottom right: Use, Close; "Skill Point: n" from the bottom left.
const USE_FROM: [f32; 2] = [107.0, 24.0];
const CLOSE_FROM: [f32; 2] = [61.0, 24.0];
const POINTS_AT: [f32; 2] = [16.0, 20.0];

pub struct SkillAssets {
    bands: [Texture; 3],
    /// Normal, hovered, pressed.
    use_button: [Texture; 3],
    close: [Texture; 3],
    resize: Texture,
}

impl SkillAssets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let button = |name: &str| ["", "_a", "_b"].map(|suffix| gpu.load_texture(grf, &format!("{UI}btn_{name}{suffix}.bmp"), false));
        Self {
            bands: BANDS.map(|color| gpu.solid(color)),
            use_button: button("use"),
            close: button("close"),
            resize: gpu.load_texture(grf, &format!("{UI}btn_resize.bmp"), false),
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum SkillHit {
    /// The title bar (it drags), its close button.
    Title,
    Close,
    /// A row, by its place among all the skills.
    Row(usize),
    Raise(u16),
    Use,
    Resize,
    Window,
}

pub struct SkillList {
    pub frame: Frame,
    /// Its title (`messages`), and whether it has skill points to show (the homunculus's).
    title: usize,
    points_line: bool,
    height: f32,
    grip: window::Grip,
    scroll: usize,
    /// The selected skill.
    pub selected: Option<u16>,
}

impl SkillList {
    pub fn homunculus() -> Self {
        Self::new(SKILLS_TITLE, true)
    }

    pub fn mercenary() -> Self {
        Self::new(crate::mercenary::SKILLS_TITLE, false)
    }

    fn new(title: usize, points_line: bool) -> Self {
        let frame = Frame::new(LIST_POSITION, LIST_WIDTH);
        Self { frame, title, points_line, height: FIRST_HEIGHT, grip: window::Grip::default(), scroll: 0, selected: None }
    }

    pub fn height(&self) -> f32 {
        self.height
    }

    /// Sets the height (kept or asked by the grip): 120 to 280 in 32-px steps.
    pub fn set_height(&mut self, asked: f32) {
        self.height = LEAST_HEIGHT + LIST_ROW * window::steps(asked, LEAST_HEIGHT, LIST_ROW, MOST_STEPS) as f32;
    }

    /// How many rows show: (h − 38) / 32.
    pub fn rows(&self) -> usize {
        ((self.height - 38.0) / LIST_ROW) as usize
    }

    /// Skills shown, with their place among all and their row's top.
    fn shown<'s>(&self, skills: &'s [Skill]) -> impl Iterator<Item = (usize, &'s Skill, f32)> {
        let scroll = self.scroll;
        skills.iter().enumerate().skip(scroll).take(self.rows()).map(move |(index, skill)| (index, skill, (index - scroll) as f32 * LIST_ROW))
    }

    fn use_at(&self) -> [f32; 2] {
        [LIST_WIDTH - USE_FROM[0], self.height - USE_FROM[1]]
    }

    fn close_at(&self) -> [f32; 2] {
        [LIST_WIDTH - CLOSE_FROM[0], self.height - CLOSE_FROM[1]]
    }

    pub fn hit(&self, cursor: [f32; 2], skills: &[Skill], points: u16) -> Option<SkillHit> {
        let local = self.frame.local(cursor, self.height)?;
        match self.frame.title_hit(local) {
            Some(TitleHit::Close) => return Some(SkillHit::Close),
            Some(_) => return Some(SkillHit::Title),
            None => {}
        }
        if window::on_grip([LIST_WIDTH, self.height], local) {
            return Some(SkillHit::Resize);
        }
        if contains(self.use_at(), BUTTON, local) {
            return Some(SkillHit::Use);
        }
        if contains(self.close_at(), BUTTON, local) {
            return Some(SkillHit::Close);
        }
        for (index, skill, y) in self.shown(skills) {
            if points > 0 && skill.upgradable && contains([ROW_UP[0], ROW_UP[1] + y], [ICON_SIZE; 2], local) {
                return Some(SkillHit::Raise(skill.id));
            }
            if contains([0.0, ROW_BAND[1] + y], [LIST_WIDTH, LIST_ROW], local) {
                return Some(SkillHit::Row(index));
            }
        }
        Some(SkillHit::Window)
    }

    /// A press on the grip holds it; whether it did.
    pub fn start_resize(&mut self, cursor: [f32; 2]) {
        self.grip.start(cursor, [LIST_WIDTH, self.height]);
    }

    /// The title bar held moves the list; the grip held sets its height.
    pub fn drag_to(&mut self, cursor: [f32; 2], count: usize) {
        if let Some([_, asked]) = self.grip.asked(cursor) {
            self.set_height(asked);
            self.scroll = self.scroll.min(count.saturating_sub(self.rows()));
        }
        self.frame.drag_to(cursor, self.height);
    }

    pub fn drop_drag(&mut self) -> bool {
        self.grip.stop() | self.frame.drop_drag()
    }

    /// The wheel: a row a notch, while there are more skills than rows.
    pub fn scroll_by(&mut self, rows: i32, count: usize) {
        let most = count.saturating_sub(self.rows());
        self.scroll = (self.scroll as i32 + rows).clamp(0, most as i32) as usize;
    }

    /// Each shown skill's row on screen (for the test driver).
    pub fn skill_boxes(&self, skills: &[Skill]) -> Vec<(u16, [f32; 4])> {
        if !self.frame.open {
            return Vec::new();
        }
        self.shown(skills)
            .map(|(_, skill, y)| {
                let [x, top] = self.frame.at([ROW_BAND[0], ROW_BAND[1] + y]);
                (skill.id, [x, top, ROW_BAND[2], ROW_BAND[3]])
            })
            .collect()
    }

    /// The level up buttons on screen: only with points to spend, on skills that can rise.
    pub fn raise_boxes(&self, skills: &[Skill], points: u16) -> Vec<(u16, [f32; 4])> {
        if !self.frame.open || points == 0 {
            return Vec::new();
        }
        self.shown(skills)
            .filter(|(_, skill, _)| skill.upgradable)
            .map(|(_, skill, y)| {
                let [x, top] = self.frame.at([ROW_UP[0], ROW_UP[1] + y]);
                (skill.id, [x, top, ICON_SIZE, ICON_SIZE])
            })
            .collect()
    }

    /// Use, Close and the grip on screen (for the test driver), named after the list (`name`).
    pub fn button_boxes(&self, name: &str) -> Vec<(String, [f32; 4])> {
        if !self.frame.open {
            return Vec::new();
        }
        let place = |at: [f32; 2], size: [f32; 2]| {
            let [x, y] = self.frame.at(at);
            [x, y, size[0], size[1]]
        };
        vec![
            (format!("{name}_use"), place(self.use_at(), BUTTON)),
            (format!("{name}_close"), place(self.close_at(), BUTTON)),
            (format!("{name}_resize"), place([LIST_WIDTH - window::GRIP, self.height - window::GRIP], [window::GRIP; 2])),
        ]
    }

    pub fn icons(skills: &[Skill], info: &HashMap<u16, SkillInfo>) -> Vec<String> {
        skills.iter().filter_map(|skill| info.get(&skill.id).map(skills::icon_path)).collect()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &'a self,
        layer: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a SkillAssets,
        skill_assets: &'a skills::Assets,
        icons: &'a HashMap<String, Option<Texture>>,
        font: &Font,
        messages: &[String],
        skills: &[Skill],
        points: u16,
        info: &HashMap<u16, SkillInfo>,
        pointer: ([f32; 2], bool),
    ) {
        if !self.frame.open {
            return;
        }
        let frame = &self.frame;
        let at = |local: [f32; 2]| frame.at(local);
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        let hit = self.hit(pointer.0, skills, points);
        let look = |target: SkillHit| {
            let hovered = hit == Some(target);
            usize::from(hovered) + usize::from(hovered && pointer.1)
        };
        // The title bar with its close button alone, white below.
        chrome.draw_title_bar(layer, at([0.0, 0.0]), LIST_WIDTH);
        let close = chrome.button(2, contains(at(frame.close_button()), [11.0, 11.0], pointer.0));
        layer.sprites.push((close, at(frame.close_button()), close.size));
        layer.texts.push((message(self.title).to_owned(), at(TITLE_SHADOW), [255; 3]));
        layer.texts.push((message(self.title).to_owned(), at(TITLE_AT), TEXT));
        layer.sprites.push((&chrome.white, at([0.0, TITLE_HEIGHT]), [LIST_WIDTH, self.height - TITLE_HEIGHT]));
        layer.sprites.push((&chrome.border, at([0.0, TITLE_HEIGHT]), [1.0, self.height - TITLE_HEIGHT]));
        layer.sprites.push((&chrome.border, at([LIST_WIDTH - 1.0, TITLE_HEIGHT]), [1.0, self.height - TITLE_HEIGHT]));
        layer.sprites.push((&chrome.border, at([0.0, self.height - 1.0]), [LIST_WIDTH, 1.0]));
        for (_, skill, y) in self.shown(skills) {
            let learned = skill.level > 0;
            if self.selected == Some(skill.id) {
                let [x, top, w, h] = ROW_BAND;
                layer.sprites.push((&assets.bands[band(skill)], at([x, top + y]), [w, h]));
            }
            let skill_info = info.get(&skill.id);
            if let Some(Some(icon)) = skill_info.map(skills::icon_path).and_then(|path| icons.get(&path)) {
                layer.sprites.push((icon, at([ROW_ICON[0], ROW_ICON[1] + y]), [ICON_SIZE; 2]));
            }
            if points > 0 && skill.upgradable {
                let up = &skill_assets.up[look(SkillHit::Raise(skill.id))];
                layer.sprites.push((up, at([ROW_UP[0], ROW_UP[1] + y]), [ICON_SIZE; 2]));
            }
            let name = skill_info.map_or_else(|| skill.id.to_string(), |info| info.name.clone());
            let name = window::fit(font, &name, NAME_END - ROW_NAME[0]);
            layer.texts.push((name, at([ROW_NAME[0], ROW_NAME[1] + y]), if learned { TEXT } else { UNLEARNED }));
            if learned {
                layer.texts.push((format!("Lv : {}", skill.level), at([ROW_LEVEL[0], ROW_LEVEL[1] + y]), TEXT));
                let sp = if skill.kind == 0 { message(PASSIVE).to_owned() } else { format!("Sp : {}", skill.sp) };
                layer.texts.push((sp, at([ROW_SP[0], ROW_SP[1] + y]), TEXT));
            }
        }
        if self.points_line {
            let line = messages::fill(message(SKILL_POINTS), &[&points]);
            layer.texts.push((line, at([POINTS_AT[0], self.height - POINTS_AT[1]]), TEXT));
        }
        layer.sprites.push((&assets.use_button[look(SkillHit::Use)], at(self.use_at()), BUTTON));
        layer.sprites.push((&assets.close[look(SkillHit::Close)], at(self.close_at()), BUTTON));
        layer.sprites.push((&assets.resize, at([LIST_WIDTH - window::GRIP, self.height - window::GRIP]), assets.resize.size));
    }
}

/// The selected row's band: blue, grey while not learned, light blue at 0 SP.
fn band(skill: &Skill) -> usize {
    if skill.level == 0 {
        1
    } else if skill.sp == 0 {
        2
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skill(id: u16, level: u16, upgradable: bool) -> Skill {
        Skill { id, kind: 0, level, sp: 0, range: 0, upgradable }
    }

    fn centre(rect: [f32; 4]) -> [f32; 2] {
        [rect[0] + rect[2] / 2.0, rect[1] + rect[3] / 2.0]
    }

    fn open_window() -> Window {
        let mut window = Window::default();
        window.frame.open = true;
        window
    }

    fn open_list() -> SkillList {
        let mut list = SkillList::homunculus();
        list.frame.open = true;
        list
    }

    #[test]
    fn names_intimacy_and_says_starving() {
        assert_eq!(
            [0, 3, 4, 10, 11, 100, 101, 250, 251, 750, 751, 910, 911, 1000].map(intimacy_word),
            [1022, 1022, 1021, 1021, 672, 672, 673, 673, 669, 669, 674, 674, 675, 675]
        );
        // At exactly 1, 5 or 10, and not the hunger last warned about.
        assert_eq!([0, 1, 2, 5, 9, 10, 11].map(|hunger| starving_now(None, hunger)), [false, true, false, true, false, true, false]);
        assert!(!starving_now(Some(10), 10));
        assert!(starving_now(Some(10), 5));
        assert_eq!(text_lines("Auto \\nFeeding").collect::<Vec<_>>(), ["Auto ", "Feeding"]);
    }

    #[test]
    fn hits_the_window_buttons() {
        let window = open_window();
        let boxes = window.button_boxes();
        let expected = [Hit::Rename, Hit::NameBox, Hit::Delete, Hit::Skills, Hit::Feed, Hit::AutoFeed];
        for ((name, rect), hit) in boxes.iter().zip(expected) {
            assert_eq!(window.hit(centre(*rect)), Some(hit), "{name}");
        }
        // At (150, 100): Rename (382, 121), Delete (337, 144), Skills (382, 144), Feed (360, 205).
        let corner = |name: &str| boxes.iter().find(|(found, _)| *found == name).map(|(_, [x, y, ..])| [*x, *y]).unwrap();
        assert_eq!([corner("rename"), corner("delete"), corner("skill"), corner("feed")], [[382.0, 121.0], [337.0, 144.0], [382.0, 144.0], [360.0, 205.0]]);
        // Only the close button on the title bar; the rest drags.
        let [x, y] = window.frame.position;
        assert_eq!(window.hit([x + 270.0, y + 8.0]), Some(Hit::Close));
        assert_eq!(window.hit([x + 255.0, y + 8.0]), Some(Hit::Window));
        assert_eq!(window.hit([x + 20.0, y + 190.0]), Some(Hit::Window));
        assert_eq!(window.hit([x + 20.0, y + 208.0]), None);
        assert!(Window::default().button_boxes().is_empty());
        // A new name: too long, then an item link, then asked.
        assert_eq!(crate::pet::rename_refusal(&"a".repeat(24), NAME_TOO_LONG), Some(938));
        assert_eq!(crate::pet::rename_refusal("<ITEML>", NAME_TOO_LONG), Some(2812));
    }

    #[test]
    fn fills_the_bars() {
        // HP and SP: the middle over 149 px, red under a quarter, nothing past the maximum, no fill at 0.
        assert_eq!(gauge(100, 100, BAR[0]), Gauge::Filled(149.0, false));
        assert_eq!(gauge(25, 100, BAR[0]), Gauge::Filled(37.0, false));
        assert_eq!(gauge(24, 100, BAR[0]), Gauge::Filled(35.0, true));
        assert_eq!(gauge(101, 100, BAR[0]), Gauge::Hidden);
        assert_eq!([gauge(0, 100, BAR[0]), gauge(0, 0, BAR[0])], [Gauge::Empty, Gauge::Empty]);
        assert_eq!(gauge_text(20, 150), " 20  /  150");
        // Exp: none past the next level's or without one.
        assert_eq!(exp_fill(50, 100), Some(50.0));
        assert_eq!([exp_fill(101, 100), exp_fill(0, 0), exp_fill(5, NO_NEXT)], [None, None, None]);
        // Hunger: red under 25, nothing past 100, only the frame at 0.
        assert_eq!([hunger_fill(20), hunger_fill(25), hunger_fill(0)], [Some((20.0, true)), Some((25.0, false)), Some((0.0, true))]);
        assert_eq!(hunger_fill(101), None);
    }

    #[test]
    fn lists_skills_in_rows() {
        let mut list = open_list();
        // 216 tall: 5 rows; the grip goes 120 to 280 in 32-px steps.
        assert_eq!((list.height(), list.rows()), (216.0, 5));
        for (asked, height, rows) in [(100.0, 120.0, 2), (151.0, 120.0, 2), (152.0, 152.0, 3), (300.0, 280.0, 7)] {
            list.set_height(asked);
            assert_eq!((list.height(), list.rows()), (height, rows), "{asked}");
        }
        list.set_height(216.0);
        let skills = [skill(8001, 3, true), skill(8002, 1, false), skill(8003, 0, true), skill(8004, 1, false)];
        // Rows at (360, 200): the band at (104, 19 + 32r).
        let boxes = list.skill_boxes(&skills);
        assert_eq!(boxes.iter().map(|(id, [x, y, ..])| (*id, [*x, *y])).collect::<Vec<_>>()[..2], [(8001, [464.0, 219.0]), (8002, [464.0, 251.0])]);
        assert_eq!(list.hit(centre(boxes[2].1), &skills, 0), Some(SkillHit::Row(2)));
        // Level up buttons at x 78 only with points, on the skills that can go up.
        assert!(list.raise_boxes(&skills, 0).is_empty());
        let raises = list.raise_boxes(&skills, 1);
        assert_eq!(raises.iter().map(|(id, [x, ..])| (*id, *x)).collect::<Vec<_>>(), [(8001, 438.0), (8003, 438.0)]);
        assert_eq!(list.hit(centre(raises[0].1), &skills, 1), Some(SkillHit::Raise(8001)));
        // Use and Close at the bottom right; the title bar has the close button alone.
        let buttons = list.button_boxes("homunculus_skills");
        assert_eq!(buttons.iter().map(|(_, [x, y, ..])| [*x, *y]).collect::<Vec<_>>()[..2], [[533.0, 392.0], [579.0, 392.0]]);
        assert_eq!(list.hit(centre(buttons[0].1), &skills, 0), Some(SkillHit::Use));
        assert_eq!(list.hit(centre(buttons[1].1), &skills, 0), Some(SkillHit::Close));
        assert_eq!(list.hit(centre(buttons[2].1), &skills, 0), Some(SkillHit::Resize));
        assert_eq!(list.hit([630.0, 208.0], &skills, 0), Some(SkillHit::Close));
        assert_eq!(list.hit([600.0, 208.0], &skills, 0), Some(SkillHit::Title));
        assert_eq!(list.hit([-100.0, -100.0], &skills, 1), None);
    }

    #[test]
    fn bands_and_scrolls() {
        // Blue, grey while not learned, light blue at 0 SP.
        let mut learned = skill(8001, 3, true);
        learned.sp = 10;
        assert_eq!([band(&learned), band(&skill(8003, 0, true)), band(&skill(8002, 1, false))], [0, 1, 2]);
        let mut list = open_list();
        let skills: Vec<Skill> = (0..7).map(|n| skill(8001 + n, 1, false)).collect();
        list.scroll_by(5, skills.len());
        assert_eq!(list.skill_boxes(&skills)[0].0, 8003, "two more than it shows");
    }
}
