//! The player's pet: the pet window (Alt+J) with its picture, name, level, hunger, intimacy and accessory and the
//! rename and feed buttons; the words for its hunger and intimacy; and what pets say (data\pettalktable.xml).

use rustnarok_client::grf::Grf;
use rustnarok_client::lua::{self, PetTables};
use rustnarok_client::net::Pet;
use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};
use crate::window::{self, Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, SCREEN, UI, contains};

const TITLE: usize = 584;
const NAME: usize = 282;
const LEVEL: usize = 408;
const HUNGER: usize = 585;
const INTIMACY: usize = 586;
const ACCESSORY: usize = 597;
const EQUIPPED: usize = 598;
const UNEQUIPPED: usize = 600;
/// " Very Hungry" to " Stuffed", by hunger band.
const HUNGER_WORDS: usize = 667;
/// The intimacy words by rathena's bands (`pet_get_card3_intimacy`): below each bound, its word.
const INTIMACY_WORDS: [(u16, usize); 4] = [(100, 672), (250, 673), (750, 669), (910, 674)];
const LOYAL: usize = 675;

/// What the talk table calls the hunger bands and the acts (`(monster − 100) × 100 + act × 10 + hunger`).
const HUNGERS: [&str; 5] = ["hungry", "bit_hungry", "noting", "full", "so_full"];
const ACTS: [&str; 10] = ["feeding", "hunting", "danger", "dead", "stand", "perfor_s", "levelup", "perfor_1", "perfor_2", "perfor_3"];
/// The acts the client says: after eating, after a performance (special, then 1 to 3).
pub const ACT_FEEDING: u8 = 0;
pub const ACT_PERFORMED_SPECIAL: u8 = 5;
pub const ACT_PERFORMED: u8 = 7;

// ponytail: data.grf has no pet window; a plain window, the picture on the left and the rows beside it.
const WIDTH: f32 = 300.0;
const BODY: f32 = 144.0;
const FOOTER: f32 = 27.0;
const PICTURE: [f32; 2] = [8.0, 5.0];
const LABEL: f32 = 108.0;
const VALUE: f32 = 170.0;
const FIRST_ROW: f32 = 14.0;
const ROW: f32 = 24.0;
const RENAME: [f32; 2] = [42.0, 20.0];
const FEED: [f32; 2] = [64.0, 20.0];
const TEXT: [u8; 3] = [0, 0, 0];

pub struct Assets {
    /// Normal, hovered, pressed.
    rename: [Texture; 3],
    feed: [Texture; 3],
    bar: Texture,
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let button = |name: &str| ["", "_a", "_b"].map(|suffix| gpu.load_texture(grf, &format!("{UI}btn_{name}{suffix}.bmp"), false));
        Self { rename: button("rewrite"), feed: button("feed"), bar: gpu.load_texture(grf, &format!("{UI}basic_interface\\btnbar_mid2.bmp"), false) }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(TitleHit),
    Rename,
    Feed,
    Window,
}

pub struct PetWindow {
    pub frame: Frame,
    /// The picture, and the monster it's of.
    pub picture: Option<(u16, Option<Texture>)>,
}

impl Default for PetWindow {
    fn default() -> Self {
        Self { frame: Frame::new([((SCREEN[0] - WIDTH) / 2.0).floor(), 160.0], WIDTH), picture: None }
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

/// The message naming how fond the pet is.
pub fn intimacy_word(intimacy: u16) -> usize {
    INTIMACY_WORDS.iter().find(|&&(below, _)| intimacy < below).map_or(LOYAL, |&(_, word)| word)
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
}

impl Data {
    pub fn load(grf: &Grf) -> Self {
        let tables = lua::pet_tables(grf).unwrap_or_else(|error| {
            eprintln!("no pet tables: {error}");
            PetTables::default()
        });
        let talk = grf.read("data\\pettalktable.xml").map(|bytes| encoding_rs::EUC_KR.decode_without_bom_handling(&bytes).0.into_owned());
        Self { tables, talk: talk.unwrap_or_default() }
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
        TITLE_HEIGHT + BODY + FOOTER
    }

    fn rename_at() -> [f32; 2] {
        [WIDTH - RENAME[0] - 8.0, TITLE_HEIGHT + FIRST_ROW - 4.0]
    }

    fn feed_at() -> [f32; 2] {
        [WIDTH - FEED[0] - 15.0, TITLE_HEIGHT + BODY + 4.0]
    }

    /// The buttons on screen: rename (until it's been renamed) and feed.
    pub fn button_boxes(&self, pet: &Pet) -> Vec<(&'static str, [f32; 4])> {
        if !self.frame.open || self.frame.minimized {
            return Vec::new();
        }
        let place = |at: [f32; 2], size: [f32; 2]| {
            let [x, y] = self.frame.at(at);
            [x, y, size[0], size[1]]
        };
        let mut boxes = vec![("feed", place(Self::feed_at(), FEED))];
        if !pet.renamed {
            boxes.push(("rename", place(Self::rename_at(), RENAME)));
        }
        boxes
    }

    pub fn hit(&self, cursor: [f32; 2], pet: &Pet) -> Option<Hit> {
        let local = self.frame.local(cursor, Self::height())?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(Hit::Title(hit));
        }
        if !pet.renamed && contains(Self::rename_at(), RENAME, local) {
            return Some(Hit::Rename);
        }
        if contains(Self::feed_at(), FEED, local) {
            return Some(Hit::Feed);
        }
        Some(Hit::Window)
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
    ) {
        if !self.frame.open {
            return;
        }
        let frame = &self.frame;
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        frame.draw_title(layer, chrome, message(TITLE), pointer.0);
        if frame.minimized {
            return;
        }
        layer.sprites.push((&chrome.white, frame.at([0.0, TITLE_HEIGHT]), [WIDTH, BODY]));
        layer.sprites.push((&chrome.border, frame.at([0.0, TITLE_HEIGHT]), [1.0, BODY]));
        layer.sprites.push((&chrome.border, frame.at([WIDTH - 1.0, TITLE_HEIGHT]), [1.0, BODY]));
        if let Some((_, Some(picture))) = &self.picture {
            layer.sprites.push((picture, frame.at([PICTURE[0], TITLE_HEIGHT + PICTURE[1]]), picture.size));
        }
        let accessory = message(if pet.accessory != 0 { EQUIPPED } else { UNEQUIPPED });
        let rows = [
            (NAME, pet.name.clone()),
            (LEVEL, pet.level.to_string()),
            (HUNGER, message(HUNGER_WORDS + usize::from(hunger_band(pet.hunger))).trim().to_owned()),
            (INTIMACY, message(intimacy_word(pet.intimacy)).trim().to_owned()),
            (ACCESSORY, accessory.to_owned()),
        ];
        let name_room = WIDTH - VALUE - if pet.renamed { 8.0 } else { RENAME[0] + 12.0 };
        for (row, (label, value)) in rows.into_iter().enumerate() {
            let y = TITLE_HEIGHT + FIRST_ROW + row as f32 * ROW;
            layer.texts.push((message(label).to_owned(), frame.at([LABEL, y]), TEXT));
            let value = if row == 0 { window::fit(font, &value, name_room) } else { value };
            layer.texts.push((value, frame.at([VALUE, y]), TEXT));
        }
        layer.sprites.push((&assets.bar, frame.at([0.0, TITLE_HEIGHT + BODY]), [WIDTH, FOOTER]));
        let hit = self.hit(pointer.0, pet);
        let look = |target: Hit| {
            let hovered = hit == Some(target);
            usize::from(hovered) + usize::from(hovered && pointer.1)
        };
        if !pet.renamed {
            layer.sprites.push((&assets.rename[look(Hit::Rename)], frame.at(Self::rename_at()), RENAME));
        }
        layer.sprites.push((&assets.feed[look(Hit::Feed)], frame.at(Self::feed_at()), FEED));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_hunger_and_intimacy() {
        assert_eq!([0, 10, 11, 25, 26, 75, 76, 90, 91, 100].map(hunger_band), [0, 0, 1, 1, 2, 2, 3, 3, 4, 4]);
        assert_eq!([0, 99, 100, 249, 250, 749, 750, 909, 910, 1000].map(intimacy_word), [672, 672, 673, 673, 669, 669, 674, 674, 675, 675]);
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
