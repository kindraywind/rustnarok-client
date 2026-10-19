//! The player's pet: the pet window (Alt+J) with its picture, name, level, hunger, intimacy and accessory and the
//! rename and feed buttons; the words for its hunger and intimacy; and what pets say (data\pettalktable.xml).

use std::collections::HashSet;

use rustnarok_client::grf::Grf;
use rustnarok_client::lua::{self, PetTables};
use rustnarok_client::net::{self, Pet};
use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};
use crate::window::{self, Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI, contains};

/// "Pet Info", "Name", "Level", "Hunger", "Intimacy", "Accessory", "Equipped", "Unequipped", "Automatic Feeding".
const TITLE: usize = 584;
const NAME: usize = 407;
const LEVEL: usize = 408;
const HUNGER: usize = 585;
const INTIMACY: usize = 586;
const ACCESSORY: usize = 597;
const EQUIPPED: usize = 598;
const UNEQUIPPED: usize = 600;
const AUTO_FEED: usize = 2577;
/// " Very Hungry" to " Stuffed", by hunger band; "Unknown" past the bands.
const HUNGER_WORDS: usize = 667;
const UNKNOWN: usize = 676;
/// The intimacy words, each up to its bound: Awkward, Shy, Neutral, Cordial, Loyal.
const INTIMACY_WORDS: [(u16, usize); 5] = [(100, 672), (250, 673), (750, 669), (900, 674), (1000, 675)];

/// What the talk table calls the hunger bands and the acts (`(monster − 100) × 100 + act × 10 + hunger`).
const HUNGERS: [&str; 5] = ["hungry", "bit_hungry", "noting", "full", "so_full"];
const ACTS: [&str; 10] = ["feeding", "hunting", "danger", "dead", "stand", "perfor_s", "levelup", "perfor_1", "perfor_2", "perfor_3"];
/// The acts the client says: after eating, after a performance (special, then 1 to 3).
pub const ACT_FEEDING: u8 = 0;
pub const ACT_PERFORMED_SPECIAL: u8 = 5;
pub const ACT_PERFORMED: u8 = 7;
pub const ACT_STAND: u8 = 4;
/// Our pet talks only this fond of us.
pub const TALKS_FROM: u16 = 900;
/// Standing still this long, it may talk (a quarter of the time), once a stand.
const IDLE_TALK: std::time::Duration = std::time::Duration::from_millis(3000);
/// "Your pet is starving and neglected" (red): only Gangsi, Death Knight and Dark Collector get it, when a fall
/// brings intimacy under 100 while hunger is under 30.
pub const NEGLECTED: usize = 1188;
pub const NEGLECTED_COLOR: [u8; 3] = [0xFF, 0x32, 0x32];

/// What our pet says for an act at a hunger: a line of its talk table, when it's fond enough of us to talk.
pub fn talk(pet: &Pet, act: u8, hunger: u16) -> Option<net::PetAct> {
    (pet.intimacy >= TALKS_FROM).then(|| net::PetAct::Talk { monster: pet.monster, act, hunger: hunger_band(hunger) })
}

pub fn neglected(job: u16, intimacy: u16, hunger: u16) -> bool {
    matches!(job, 4050..=4052) && intimacy < 100 && hunger < 30
}

/// Our pet standing still: since when, and whether this stand has had its roll.
#[derive(Default)]
pub struct Idle {
    since: Option<std::time::Instant>,
    rolled: bool,
}

impl Idle {
    /// Whether to roll for a stand line now: once it has stood 3000 ms, then not again until it has moved.
    pub fn due(&mut self, moving: bool, now: std::time::Instant) -> bool {
        if moving {
            *self = Self::default();
            return false;
        }
        let since = *self.since.get_or_insert(now);
        let due = !self.rolled && now.duration_since(since) >= IDLE_TALK;
        self.rolled |= due;
        due
    }
}

/// 280 × 180, white, the close button alone on the title bar; from the window's corner, title bar included.
const WIDTH: f32 = 280.0;
const HEIGHT: f32 = 180.0;
const POSITION: [f32; 2] = [150.0, 100.0];
const TITLE_TEXT: [f32; 2] = [5.0, 2.0];
const CORNER: f32 = 7.0;
const PICTURE: [f32; 2] = [5.0, 21.0];
/// The rows: labels at x 100; the name box, its rename button, the level, the command box; the words at x 145.
const LABEL: f32 = 100.0;
const NAME_BOX: [f32; 4] = [135.0, 24.0, 80.0, 16.0];
const NAME_FILL: [u8; 4] = [0xE8, 0xE8, 0xE8, 0xFF];
const RENAME: [f32; 2] = [230.0, 21.0];
const RENAME_SIZE: [f32; 2] = [42.0, 20.0];
const NAME_TOP: f32 = 24.0;
const LEVEL_TOP: f32 = 44.0;
const VALUE: f32 = 135.0;
const COMMAND_BOX: [f32; 4] = [160.0, 44.0, 110.0, 18.0];
const COMMAND_FILL: [u8; 4] = [0xE6, 0xE6, 0xFA, 0xFF];
const COMMAND_TEXT: &str = "< Command >";
const COMMAND_TEXT_AT: [f32; 2] = [206.0, 48.0];
/// A press here or on the arrow opens the command list.
const COMMAND_AREA: [f32; 4] = [150.0, 44.0, 120.0, 18.0];
const WORD: f32 = 145.0;
const HUNGER_TOP: f32 = 69.0;
const INTIMACY_TOP: f32 = 89.0;
const BAR: [f32; 4] = [145.0, 105.0, 120.0, 9.0];
const ACCESSORY_TOP: f32 = 119.0;
const EQUIPPED_TOP: f32 = 139.0;
const AUTO_TOP: f32 = 159.0;
const AUTO_BOX: [f32; 2] = [12.0, 12.0];
/// The command list under the box: Feed, Performance, Unequip Accessory, Return to Egg.
// ponytail: drawn like the right-click menu, 16 px rows.
const LIST_ROW: f32 = 16.0;
pub const LIST: [(usize, u8); 4] = [(592, net::PET_FEED), (593, net::PET_PERFORM), (595, net::PET_UNEQUIP), (594, net::PET_RETURN)];
const LIST_LINE: [u8; 4] = [0xCE, 0xCE, 0xCE, 0xFF];
const LIST_HOVER: [u8; 4] = [0x73, 0x9C, 0xEF, 0xFF];
const TEXT: [u8; 3] = [0, 0, 0];
/// A name may not be this long (bytes), nor hold an item link.
const NAME_BYTES: usize = 24;
pub const NAME_TOO_LONG: usize = 682;
pub const NAME_HAS_LINK: usize = 2812;

pub struct Assets {
    /// Normal, hovered, pressed.
    rename: [Texture; 3],
    /// The command box's arrow: normal, hovered, pressed.
    arrow: [Texture; 3],
    corners: [Texture; 2],
    no_picture: Texture,
    /// The bar's track (left, middle, right) and its blue fill.
    track: [Texture; 3],
    fill: [Texture; 3],
    check: [Texture; 2],
    name_fill: Texture,
    command_fill: Texture,
    line: Texture,
    hover: Texture,
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |path: String| gpu.load_texture(grf, &path, false);
        let basic = |name: &str| image(format!("{UI}basic_interface\\{name}.bmp"));
        Self {
            rename: ["", "_a", "_b"].map(|suffix| image(format!("{UI}btn_rewrite{suffix}.bmp"))),
            arrow: ["txtbox_btn_a", "txtbox_btn_b", "txtbox_btn_c"].map(basic),
            corners: ["sysboxs_ld", "sysboxs_rd"].map(basic),
            no_picture: image("data\\texture\\userinterface\\illust\\pet_noimage.bmp".to_owned()),
            track: ["gze_bg_left", "gze_bg_mid", "gze_bg_right"].map(basic),
            fill: ["gzeblue_left", "gzeblue_mid", "gzeblue_right"].map(basic),
            check: ["checkbox_0", "checkbox_1"].map(|name| image(format!("{UI}{name}.bmp"))),
            name_fill: gpu.solid(NAME_FILL),
            command_fill: gpu.solid(COMMAND_FILL),
            line: gpu.solid(LIST_LINE),
            hover: gpu.solid(LIST_HOVER),
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Close,
    Rename,
    /// The name box: typing goes to it.
    NameBox,
    /// The command box or its arrow: the list opens.
    OpenList,
    /// A command of the open list, by its row.
    Command(usize),
    AutoFeed,
    /// Anywhere else: the window drags from it.
    Window,
}

pub struct PetWindow {
    pub frame: Frame,
    /// The picture, and the monster it's of.
    pub picture: Option<(u16, Option<Texture>)>,
    /// The name box's text once it's been typed in (None: the pet's name), and whether typing goes to it.
    pub name_text: Option<String>,
    pub typing: bool,
    /// The command list is open.
    pub list_open: bool,
    /// The automatic feeding box, as the server last said; whether it shows (the pet's egg allows it); its label's
    /// width.
    pub auto_feed: bool,
    pub auto_allowed: bool,
    pub auto_label: f32,
    /// The egg the pet hatched from (its item ID).
    pub egg: Option<u32>,
}

impl Default for PetWindow {
    fn default() -> Self {
        Self {
            frame: Frame::new(POSITION, WIDTH),
            picture: None,
            name_text: None,
            typing: false,
            list_open: false,
            auto_feed: false,
            auto_allowed: false,
            auto_label: 0.0,
            egg: None,
        }
    }
}

/// rathena's hunger bands (`pet_hungry_val`): 0 very hungry to 4 stuffed.
pub fn hunger_band(hunger: u16) -> u8 {
    match hunger {
        91.. => 4,
        76..=90 => 3,
        26..=75 => 2,
        11..=25 => 1,
        _ => 0,
    }
}

/// The message naming how hungry the pet is: by rathena's bands, "Unknown" past 100.
pub fn hunger_word(hunger: u16) -> usize {
    if hunger > 100 { UNKNOWN } else { HUNGER_WORDS + usize::from(hunger_band(hunger)) }
}

/// The message naming how fond the pet is: each word up to its bound, "Unknown" past 1000.
pub fn intimacy_word(intimacy: u16) -> usize {
    INTIMACY_WORDS.iter().find(|&&(bound, _)| intimacy <= bound).map_or(UNKNOWN, |&(_, word)| word)
}

/// How far into its word's band the intimacy is, 0 to 100 (0 outside 0–1000).
pub fn intimacy_bar(intimacy: u16) -> u32 {
    let Some(band) = INTIMACY_WORDS.iter().position(|&(bound, _)| intimacy <= bound) else { return 0 };
    let low = if band == 0 { 0 } else { INTIMACY_WORDS[band - 1].0 };
    let high = INTIMACY_WORDS[band].0;
    100 * u32::from(intimacy.saturating_sub(low)) / u32::from(high - low)
}

/// The blue middle's width for a bar value, between the fill's two ends.
fn bar_middle(value: u32, ends: f32) -> f32 {
    ((BAR[2] - ends) * value as f32 / 100.0).trunc()
}

/// What a new name the rename button reads from the box is refused for: too long, or an item link in it.
pub fn rename_refusal(name: &str) -> Option<usize> {
    if name.len() >= NAME_BYTES {
        Some(NAME_TOO_LONG)
    } else if name.contains("<ITEML>") || name.contains("</ITEML>") {
        Some(NAME_HAS_LINK)
    } else {
        None
    }
}

/// Feeding, before the question: refused while a mail is written ("can't while writing a mail").
pub const FEED_WHILE_MAIL: usize = 2985;
/// After yes: refused while trading, or said there's no food (no packet).
pub const FEED_WHILE_TRADING: usize = 3550;

/// What a yes to feeding does: refused while trading, no food in the bag, or sent.
#[derive(Debug, PartialEq)]
pub enum Feeding {
    Trading,
    NoFood,
    Send,
}

pub fn feeding(trading: bool, has_food: bool) -> Feeding {
    if trading {
        Feeding::Trading
    } else if has_food {
        Feeding::Send
    } else {
        Feeding::NoFood
    }
}

/// The insides of each element named `name` (lowercase) in `lower[from]`, the XML lowercased, by byte range.
fn elements(lower: &str, from: std::ops::Range<usize>, name: &str) -> Vec<std::ops::Range<usize>> {
    let (open, close) = (format!("<{name}>"), format!("</{name}>"));
    let mut found = Vec::new();
    let mut at = from.start;
    while let Some(start) = lower[at..from.end].find(&open) {
        let start = at + start + open.len();
        let Some(length) = lower[start..from.end].find(&close) else { break };
        found.push(start..start + length);
        at = start + length + close.len();
    }
    found
}

/// What a pet (by its talk table name) can say for an act at a hunger band.
pub fn talk_lines<'a>(xml: &'a str, pet: &str, hunger: u8, act: u8) -> Vec<&'a str> {
    let (Some(hunger), Some(act)) = (HUNGERS.get(usize::from(hunger)), ACTS.get(usize::from(act))) else { return Vec::new() };
    // Lowercasing ASCII keeps every byte where it was: the ranges fit `xml` too.
    let lower = xml.to_ascii_lowercase();
    let Some(pet) = elements(&lower, 0..xml.len(), &pet.to_ascii_lowercase()).into_iter().next() else { return Vec::new() };
    let Some(band) = elements(&lower, pet, hunger).into_iter().next() else { return Vec::new() };
    elements(&lower, band, act).into_iter().map(|range| xml[range].trim()).collect()
}

/// What the client's data says about pets: its tables, and what they say.
#[derive(Default)]
pub struct Data {
    pub tables: PetTables,
    talk: String,
    /// The eggs whose pets may eat by themselves.
    pub auto_eggs: HashSet<u32>,
}

impl Data {
    pub fn load(grf: &Grf) -> Self {
        let tables = lua::pet_tables(grf).unwrap_or_else(|error| {
            eprintln!("no pet tables: {error}");
            PetTables::default()
        });
        let talk = grf.read("data\\pettalktable.xml").map(|bytes| encoding_rs::EUC_KR.decode_without_bom_handling(&bytes).0.into_owned());
        Self { tables, talk: talk.unwrap_or_default(), auto_eggs: auto_feeding_eggs(grf) }
    }

    /// One of the lines the pet (a monster ID) has for the act at the hunger band, picked at random.
    pub fn line(&self, monster: u16, act: u8, hunger: u8) -> Option<String> {
        let lines = talk_lines(&self.talk, self.tables.names.get(&monster)?, hunger, act);
        let last = u32::try_from(lines.len()).ok()?.checked_sub(1)?;
        lines.get(crate::effects::pick(0, last) as usize).map(|line| (*line).to_owned())
    }
}

impl PetWindow {
    pub fn height() -> f32 {
        HEIGHT
    }

    /// The list's row under a window-local point, while it's open.
    fn list_row(&self, local: [f32; 2]) -> Option<usize> {
        let [x, y, width, _] = COMMAND_BOX;
        let top = y + COMMAND_BOX[3];
        (self.list_open && contains([x, top], [width, LIST.len() as f32 * LIST_ROW], local)).then(|| ((local[1] - top) / LIST_ROW) as usize)
    }

    /// The automatic feeding box's corner, after its label.
    fn auto_box(&self) -> [f32; 2] {
        [LABEL + 10.0 + self.auto_label, AUTO_TOP]
    }

    /// The name the box shows: what's typed, else the pet's.
    pub fn name<'a>(&'a self, pet: &'a Pet) -> &'a str {
        self.name_text.as_deref().unwrap_or(&pet.name)
    }

    /// The buttons and boxes on screen (for the test driver): "rename", "pet_name", "pet_command", "pet_command:<n>",
    /// "pet_auto_feed".
    pub fn button_boxes(&self) -> Vec<(String, [f32; 4])> {
        if !self.frame.open {
            return Vec::new();
        }
        let place = |at: [f32; 2], size: [f32; 2]| {
            let [x, y] = self.frame.at(at);
            [x, y, size[0], size[1]]
        };
        let [x, y, w, h] = NAME_BOX;
        let [cx, cy, cw, ch] = COMMAND_AREA;
        let mut boxes = vec![
            ("rename".to_owned(), place(RENAME, RENAME_SIZE)),
            ("pet_name".to_owned(), place([x, y], [w, h])),
            ("pet_command".to_owned(), place([cx, cy], [cw, ch])),
        ];
        if self.list_open {
            let [lx, ly, lw, lh] = COMMAND_BOX;
            boxes.extend((0..LIST.len()).map(|row| (format!("pet_command:{row}"), place([lx, ly + lh + row as f32 * LIST_ROW], [lw, LIST_ROW]))));
        }
        if self.auto_allowed {
            boxes.push(("pet_auto_feed".to_owned(), place(self.auto_box(), AUTO_BOX)));
        }
        boxes
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let local = self.frame.local(cursor, HEIGHT)?;
        if let Some(row) = self.list_row(local) {
            return Some(Hit::Command(row));
        }
        Some(match self.frame.title_hit(local) {
            Some(TitleHit::Close) => Hit::Close,
            Some(_) => Hit::Window,
            None if contains(RENAME, RENAME_SIZE, local) => Hit::Rename,
            None if contains([NAME_BOX[0], NAME_BOX[1]], [NAME_BOX[2], NAME_BOX[3]], local) => Hit::NameBox,
            None if contains([COMMAND_AREA[0], COMMAND_AREA[1]], [COMMAND_AREA[2], COMMAND_AREA[3]], local) => Hit::OpenList,
            None if self.auto_allowed && contains(self.auto_box(), AUTO_BOX, local) => Hit::AutoFeed,
            None => Hit::Window,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &'a self,
        layer: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        font: &Font,
        messages: &[String],
        pet: &Pet,
        pointer: ([f32; 2], bool),
        blink: bool,
    ) {
        if !self.frame.open {
            return;
        }
        let frame = &self.frame;
        let at = |local: [f32; 2]| frame.at(local);
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        let hit = self.hit(pointer.0);
        let look = |target: Hit| {
            let hovered = hit == Some(target);
            usize::from(hovered) + usize::from(hovered && pointer.1)
        };
        // The title bar with the close button alone, white below, the bottom corners rounded.
        chrome.draw_title_bar(layer, at([0.0, 0.0]), WIDTH);
        let close = chrome.button(2, hit == Some(Hit::Close));
        layer.sprites.push((close, at(frame.close_button()), close.size));
        layer.texts.push((message(TITLE).to_owned(), at(TITLE_TEXT), TEXT));
        layer.sprites.push((&chrome.white, at([0.0, TITLE_HEIGHT]), [WIDTH, HEIGHT - TITLE_HEIGHT]));
        let [left, right] = &assets.corners;
        layer.sprites.push((left, at([0.0, HEIGHT - CORNER]), left.size));
        layer.sprites.push((right, at([WIDTH - CORNER, HEIGHT - CORNER]), right.size));
        let picture = self.picture.as_ref().and_then(|(_, picture)| picture.as_ref()).unwrap_or(&assets.no_picture);
        layer.sprites.push((picture, at(PICTURE), picture.size));
        // Name (edited in place), its rename button, level.
        layer.texts.push((message(NAME).to_owned(), at([LABEL, NAME_TOP]), TEXT));
        let [x, y, w, h] = NAME_BOX;
        layer.sprites.push((&assets.name_fill, at([x, y]), [w, h]));
        let shown = if self.typing && blink { format!("{}|", self.name(pet)) } else { self.name(pet).to_owned() };
        layer.texts.push((window::fit(font, &shown, w - 4.0), at([x + 2.0, y + ((h - font.line_height()) / 2.0).floor()]), TEXT));
        layer.sprites.push((&assets.rename[look(Hit::Rename)], at(RENAME), RENAME_SIZE));
        layer.texts.push((message(LEVEL).to_owned(), at([LABEL, LEVEL_TOP]), TEXT));
        layer.texts.push((pet.level.to_string(), at([VALUE, LEVEL_TOP]), TEXT));
        // The command box and its arrow.
        let [x, y, w, h] = COMMAND_BOX;
        layer.sprites.push((&assets.command_fill, at([x, y]), [w, h]));
        layer.texts.push((COMMAND_TEXT.to_owned(), at([COMMAND_TEXT_AT[0] - (font.width(COMMAND_TEXT) / 2.0).floor(), COMMAND_TEXT_AT[1]]), TEXT));
        let arrow = &assets.arrow[look(Hit::OpenList)];
        layer.sprites.push((arrow, at([x + w - arrow.size[0], y]), arrow.size));
        // Hunger and intimacy (the words keep their leading space), the intimacy bar, the accessory.
        layer.texts.push((message(HUNGER).to_owned(), at([LABEL, HUNGER_TOP]), TEXT));
        layer.texts.push((message(hunger_word(pet.hunger)).to_owned(), at([WORD, HUNGER_TOP]), TEXT));
        layer.texts.push((message(INTIMACY).to_owned(), at([LABEL, INTIMACY_TOP]), TEXT));
        layer.texts.push((message(intimacy_word(pet.intimacy)).to_owned(), at([WORD, INTIMACY_TOP]), TEXT));
        let [x, y, w, h] = BAR;
        let [track_left, track_middle, track_right] = &assets.track;
        layer.sprites.push((track_left, at([x, y]), [track_left.size[0], h]));
        layer.sprites.push((track_middle, at([x + track_left.size[0], y]), [w - track_left.size[0] - track_right.size[0], h]));
        layer.sprites.push((track_right, at([x + w - track_right.size[0], y]), [track_right.size[0], h]));
        let value = intimacy_bar(pet.intimacy);
        if value > 0 {
            let [fill_left, fill_middle, fill_right] = &assets.fill;
            let middle = bar_middle(value, fill_left.size[0] + fill_right.size[0]);
            layer.sprites.push((fill_left, at([x, y]), [fill_left.size[0], h]));
            layer.sprites.push((fill_middle, at([x + fill_left.size[0], y]), [middle, h]));
            layer.sprites.push((fill_right, at([x + fill_left.size[0] + middle, y]), [fill_right.size[0], h]));
        }
        layer.texts.push((message(ACCESSORY).to_owned(), at([LABEL, ACCESSORY_TOP]), TEXT));
        layer.texts.push((message(if pet.accessory != 0 { EQUIPPED } else { UNEQUIPPED }).to_owned(), at([LABEL, EQUIPPED_TOP]), TEXT));
        // Automatic feeding, when the pet's egg allows it.
        if self.auto_allowed {
            layer.texts.push((message(AUTO_FEED).to_owned(), at([LABEL, AUTO_TOP]), TEXT));
            let check = &assets.check[usize::from(self.auto_feed)];
            layer.sprites.push((check, at(self.auto_box()), AUTO_BOX));
        }
        // The open command list over everything, the window's text included.
        if self.list_open {
            let layer = layer.window();
            let [x, y, w, h] = COMMAND_BOX;
            let top = y + h;
            layer.sprites.push((&assets.line, at([x, top]), [w, LIST.len() as f32 * LIST_ROW + 1.0]));
            layer.sprites.push((&chrome.white, at([x + 1.0, top]), [w - 2.0, LIST.len() as f32 * LIST_ROW]));
            for (row, (label, _)) in LIST.iter().enumerate() {
                let row_top = top + row as f32 * LIST_ROW;
                if hit == Some(Hit::Command(row)) {
                    layer.sprites.push((&assets.hover, at([x + 1.0, row_top]), [w - 2.0, LIST_ROW]));
                }
                layer.texts.push((message(*label).to_owned(), at([x + 4.0, row_top + ((LIST_ROW - font.line_height()) / 2.0).floor()]), TEXT));
            }
        }
    }
}

/// The eggs whose pets may eat by themselves: `InsertPetAutoFeeding(egg)` calls in the client's pet evolution list.
pub fn auto_feeding_eggs(grf: &Grf) -> HashSet<u32> {
    lua::pet_auto_feeding(grf).unwrap_or_default()
}

/// The automatic feeding label's message, for its width.
pub const AUTO_FEED_LABEL: usize = AUTO_FEED;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::offset;

    #[test]
    fn names_hunger_and_intimacy() {
        assert_eq!([0, 10, 11, 25, 26, 75, 76, 90, 91, 100].map(hunger_band), [0, 0, 1, 1, 2, 2, 3, 3, 4, 4]);
        assert_eq!([0, 10, 11, 90, 91, 100, 101].map(hunger_word), [667, 667, 668, 670, 671, 671, 676]);
        let edges = [0, 100, 101, 250, 251, 750, 751, 900, 901, 1000, 1001];
        assert_eq!(edges.map(intimacy_word), [672, 672, 673, 673, 669, 669, 674, 674, 675, 675, 676]);
        // How far into its band: full at 100, empty again at 101; nothing past 1000.
        assert_eq!(edges.map(intimacy_bar), [0, 100, 0, 100, 0, 100, 0, 100, 1, 100, 0]);
        assert_eq!([intimacy_bar(175), intimacy_bar(905)], [50, 5]);
        // The blue middle between ends 3 px wide each: 114 px at 100.
        assert_eq!([bar_middle(100, 6.0), bar_middle(50, 6.0), bar_middle(1, 6.0)], [114.0, 57.0, 1.0]);
    }

    #[test]
    fn talks_when_fond_and_standing() {
        let start = std::time::Instant::now();
        let at = |ms: u64| start + std::time::Duration::from_millis(ms);
        let mut idle = Idle::default();
        assert!(!idle.due(false, at(0)));
        assert!(!idle.due(false, at(2999)));
        assert!(idle.due(false, at(3000)));
        assert!(!idle.due(false, at(9000)), "once a stand");
        assert!(!idle.due(true, at(9100)));
        assert!(!idle.due(false, at(9200)));
        assert!(idle.due(false, at(12_200)));
        // Only from intimacy 900, about the hunger it's given (a feeding's: from before the meal).
        let poring = |intimacy| Pet { intimacy, monster: 1002, ..Pet::default() };
        assert_eq!(talk(&poring(899), ACT_FEEDING, 20), None);
        assert_eq!(talk(&poring(900), ACT_FEEDING, 20).map(|act| act.value()), Some(1_002_010));
        // The neglect line: three jobs, under 100 intimacy, under 30 hunger.
        assert!(neglected(4050, 99, 29) && neglected(4052, 0, 0));
        assert!(!neglected(1, 99, 0) && !neglected(4051, 100, 0) && !neglected(4051, 50, 30));
    }

    #[test]
    fn checks_feeding_and_renaming_in_order() {
        assert_eq!([feeding(true, true), feeding(false, false), feeding(false, true)], [Feeding::Trading, Feeding::NoFood, Feeding::Send]);
        assert_eq!(rename_refusal(&"a".repeat(24)), Some(NAME_TOO_LONG));
        assert_eq!(rename_refusal("<ITEML>x"), Some(NAME_HAS_LINK));
        assert_eq!(rename_refusal("x</ITEML>"), Some(NAME_HAS_LINK));
        assert_eq!(rename_refusal(&"a".repeat(23)), None);
    }

    #[test]
    fn hits_the_pet_windows_parts() {
        let mut window = PetWindow { auto_label: 90.0, ..PetWindow::default() };
        window.frame.open = true;
        assert_eq!(window.frame.position, [150.0, 100.0]);
        let at = |local: [f32; 2]| offset([150.0, 100.0], local);
        assert_eq!(window.hit(at([270.0, 5.0])), Some(Hit::Close));
        assert_eq!(window.hit(at([100.0, 5.0])), Some(Hit::Window));
        assert_eq!(window.hit(at([231.0, 22.0])), Some(Hit::Rename));
        assert_eq!(window.hit(at([136.0, 25.0])), Some(Hit::NameBox));
        assert_eq!(window.hit(at([151.0, 45.0])), Some(Hit::OpenList));
        assert_eq!(window.hit(at([20.0, 150.0])), Some(Hit::Window), "it drags from the picture");
        // The list under the command box: Feed, Performance, Unequip, Return.
        window.list_open = true;
        assert_eq!(window.hit(at([170.0, 63.0])), Some(Hit::Command(0)));
        assert_eq!(window.hit(at([170.0, 62.0 + 3.0 * 16.0 + 1.0])), Some(Hit::Command(3)));
        assert_eq!(LIST.map(|(_, command)| command), [net::PET_FEED, net::PET_PERFORM, net::PET_UNEQUIP, net::PET_RETURN]);
        // The automatic feeding box after its label, only when the egg allows it.
        window.list_open = false;
        let corner = window.auto_box();
        assert_eq!(corner, [200.0, 159.0]);
        assert_eq!(window.hit(at(offset(corner, [1.0, 1.0]))), Some(Hit::Window));
        window.auto_allowed = true;
        assert_eq!(window.hit(at(offset(corner, [1.0, 1.0]))), Some(Hit::AutoFeed));
        // The box shows the typed name, else the pet's.
        let pet = Pet { name: "Poring".into(), ..Pet::default() };
        assert_eq!(window.name(&pet), "Poring");
        window.name_text = Some("Pori".into());
        assert_eq!(window.name(&pet), "Pori");
    }

    #[test]
    fn lists_the_eggs_that_feed_themselves() {
        let source = b"InsertPetAutoFeeding(9001)\nInsertEvolutionRecipeLGU(9001, 9002, 1, 2)\nInsertPetAutoFeeding(9002)\n".to_vec();
        assert_eq!(lua::auto_feeding(source).unwrap(), HashSet::from([9001, 9002]));
    }

    #[test]
    fn picks_what_a_pet_says() {
        let xml = "<?xml version=\"1.0\" ?>\n<monster_talk_table>\n\t<Poring>\n\t\t<bit_hungry>\n\t\t\t<feeding>\tYum.\t</feeding>\n\
                   \t\t</bit_hungry>\n\t\t<hungry>\n\t\t\t<feeding>\tMore!\t</feeding>\n\t\t\t<feeding>Again</feeding>\n\
                   \t\t\t<perfor_1>Ta-da</perfor_1>\n\t\t</hungry>\n\t</Poring>\n\t<lunatic><hungry><feeding>Carrot</feeding></hungry></lunatic>\n\
                   </monster_talk_table>";
        assert_eq!(talk_lines(xml, "poring", 0, ACT_FEEDING), ["More!", "Again"]);
        assert_eq!(talk_lines(xml, "poring", 1, ACT_FEEDING), ["Yum."]);
        assert_eq!(talk_lines(xml, "poring", 0, ACT_PERFORMED), ["Ta-da"]);
        assert_eq!(talk_lines(xml, "lunatic", 0, ACT_FEEDING), ["Carrot"]);
        assert!(talk_lines(xml, "poring", 3, ACT_FEEDING).is_empty());
        assert!(talk_lines(xml, "drops", 0, ACT_FEEDING).is_empty());
    }
}
