//! Character creation of 2018+ clients (make_character_ver2): race, gender, hairstyle and hair color with the
//! character turning on its pedestal, and the name; behind it, the client's blurred picture of the select screen.

use rustnarok_client::grf::Grf;
use rustnarok_client::lua::LookTables;
use rustnarok_client::text::Font;

use crate::actor::{Look, Placed, SpriteSet, Timing};
use crate::render::{Gpu, Texture};
use crate::{Layer, SCREEN, UI, contains, offset};

// ponytail: places come from the bitmap's own boxes (name field, scroll bar, color box, pedestals) where it has
// them and are estimated otherwise; calibrate against the real client.
const WINDOW: [f32; 2] = [794.0, 422.0];
/// bg_back2.tga is laid where the select screen's window was.
const BACKDROP: [f32; 2] = [997.0, 626.0];
const TITLE: [f32; 2] = [45.0, 15.0];
const CLOSE: [f32; 2] = [767.0, 9.0];
const RACES: [[f32; 2]; 2] = [[20.0, 33.0], [20.0, 202.0]];
const RACE_SIZE: [f32; 2] = [364.0, 166.0];
/// Inside a race panel: its name on the chip, the description, and where its figure stands.
const RACE_TITLE: [f32; 2] = [45.0, 22.0];
const RACE_TEXT: [f32; 2] = [110.0, 22.0];
const RACE_TEXT_WIDTH: f32 = 230.0;
const RACE_FEET: [f32; 2] = [60.0, 136.0];
const GENDERS: [[f32; 2]; 2] = [[435.0, 55.0], [498.0, 55.0]];
/// The preview stands on the pedestal in the middle of the gradient, between the turn buttons.
const MODEL_FEET: [f32; 2] = [498.0, 235.0];
const TURNS: [[f32; 2]; 2] = [[426.0, 224.0], [548.0, 224.0]];
const NAME_FIELD: [f32; 2] = [421.0, 259.0];
const NAME_SIZE: [f32; 2] = [158.0, 34.0];
const CHECK: [f32; 2] = [421.0, 296.0];
const HAIR_TITLE: [f32; 2] = [612.0, 42.0];
const HAIR_GRID: [f32; 2] = [612.0, 60.0];
const HAIR_CELL: [f32; 2] = [36.0, 37.0];
const HAIR_COLUMNS: usize = 4;
const COLOR_TITLE: [f32; 2] = [609.0, 283.0];
const COLORS: [f32; 2] = [613.0, 304.0];
const COLOR_STEP: f32 = 26.0;
const COLOR_COLUMNS: usize = 5;
const BACK: [f32; 2] = [25.0, 378.0];
const MAKE: [f32; 2] = [617.0, 378.0];
const WHITE: [u8; 3] = [255, 255, 255];
const BLACK: [u8; 3] = [0, 0, 0];
const DESCRIPTION: [u8; 3] = [0x55, 0x55, 0x55];
const RACE_JOBS: [u16; 2] = [0, 4218];
const HAIRSTYLES: [usize; 2] = [23, 6];
pub const HAIR_COLORS: usize = 9;
/// rathena takes names of up to 23 bytes (EUC-KR).
const NAME_BYTES: usize = 23;

const TITLE_MESSAGE: usize = 3355;
const RACE_MESSAGES: [usize; 2] = [3016, 3018];
const RACE_DESCRIPTIONS: [usize; 2] = [3020, 3021];
const HAIR_MESSAGE: usize = 3346;
const COLOR_MESSAGE: usize = 3347;
const MAKE_MESSAGE: usize = 3345;
const BACK_MESSAGE: usize = 3351;
const CHECK_MESSAGE: usize = 3348;
pub const SHORT_NAME_MESSAGE: usize = 21;

pub struct Assets {
    backdrop: Texture,
    window: Texture,
    /// Human and doram panels, off and on.
    races: [[Texture; 2]; 2],
    /// Male and female: off, on, over, press.
    genders: [[Texture; 4]; 2],
    /// Left and right: normal, over, press.
    turns: [[Texture; 3]; 2],
    /// A hairstyle cell: normal, over, press, selected.
    hair_cell: [Texture; 4],
    /// Hairstyle pictures: human male, human female, doram male, doram female.
    hairstyles: [Vec<Texture>; 4],
    /// Each color: off, on, over, press.
    colors: Vec<[Texture; 4]>,
    button: [Texture; 3],
    check: [Texture; 3],
    close: [Texture; 3],
    black: Texture,
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |name: &str| gpu.load_texture(grf, &format!("{UI}make_character_ver2\\{name}"), false);
        let pressable = |name: &str| ["normal", "over", "press"].map(|state| image(&format!("{name}_{state}.bmp")));
        let lit = |name: &str| ["off", "on", "over", "press"].map(|state| image(&format!("{name}_{state}.bmp")));
        let pictures = |prefix: &str, count: usize| (1..=count).map(|style| image(&format!("img_hairstyle{prefix}{style:02}.bmp"))).collect();
        Self {
            backdrop: image("bg_back2.tga"),
            window: image("bg_makebg.bmp"),
            races: [["img_human_off.bmp", "img_human_on.bmp"].map(image), ["img_doram_off.bmp", "img_doram_on.bmp"].map(image)],
            genders: [lit("bt_male"), lit("bt_female")],
            turns: [pressable("bt_leftturn"), pressable("bt_rightturn")],
            hair_cell: ["normal", "over", "press", "select"].map(|state| image(&format!("bt_hairstyle_{state}.bmp"))),
            hairstyles: [
                pictures("", HAIRSTYLES[0]),
                pictures("_girl", HAIRSTYLES[0]),
                pictures("_doramboy", HAIRSTYLES[1]),
                pictures("_doramgirl", HAIRSTYLES[1]),
            ],
            colors: (1..=HAIR_COLORS).map(|color| lit(&format!("color{color:02}"))).collect(),
            button: pressable("bt_make"),
            check: pressable("bt_doublecheck"),
            close: pressable("bt_close"),
            black: gpu.solid([0, 0, 0, 255]),
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Race(usize),
    /// 0 male, 1 female.
    Gender(usize),
    Turn(i8),
    Hair(usize),
    Color(usize),
    Name,
    Check,
    Make,
    Back,
    Close,
}

/// What a click asks the client to do.
#[derive(Debug, PartialEq)]
pub enum Action {
    /// Send CH_MAKE_CHAR.
    Make,
    /// The name is too short to check or make.
    ShortName,
    Back,
}

pub struct Make {
    pub slot: u8,
    pub doram: bool,
    pub male: bool,
    /// Hairstyle (from 1) and color (from 0).
    pub hair: u16,
    pub color: u16,
    pub name: String,
    /// The syllable the input method is still composing.
    pub preedit: String,
    /// Which way the preview faces (0 the viewer, turning by eighths).
    direction: u8,
    /// The race panels' figures and the preview, rebuilt when the look changes.
    figures: [Option<SpriteSet>; 3],
}

impl Make {
    pub fn new(grf: &Grf, tables: &LookTables, slot: u8, male: bool) -> Self {
        let mut make =
            Self { slot, doram: false, male, hair: 1, color: 0, name: String::new(), preedit: String::new(), direction: 0, figures: [None, None, None] };
        make.dress(grf, tables);
        make
    }

    pub fn job(&self) -> u16 {
        RACE_JOBS[usize::from(self.doram)]
    }

    pub fn sex(&self) -> u8 {
        u8::from(self.male)
    }

    fn look(&self, job: u16, hair: u16, color: u16) -> Look {
        Look { job, sex: self.sex(), hair, hair_color: color, ..Look::default() }
    }

    /// Rebuilds the figures for the current choices.
    fn dress(&mut self, grf: &Grf, tables: &LookTables) {
        self.figures = [
            SpriteSet::player(grf, &self.look(RACE_JOBS[0], 1, 0), tables),
            SpriteSet::player(grf, &self.look(RACE_JOBS[1], 1, 0), tables),
            SpriteSet::player(grf, &self.look(self.job(), self.hair, self.color), tables),
        ];
    }

    fn origin() -> [f32; 2] {
        [((SCREEN[0] - WINDOW[0]) / 2.0).floor(), ((SCREEN[1] - WINDOW[1]) / 2.0).floor()]
    }

    fn at(local: [f32; 2]) -> [f32; 2] {
        offset(Self::origin(), local)
    }

    fn styles(&self) -> usize {
        HAIRSTYLES[usize::from(self.doram)]
    }

    fn hair_cell(style: usize) -> [f32; 2] {
        Self::at([HAIR_GRID[0] + (style % HAIR_COLUMNS) as f32 * HAIR_CELL[0], HAIR_GRID[1] + (style / HAIR_COLUMNS) as f32 * HAIR_CELL[1]])
    }

    fn color_cell(color: usize) -> [f32; 2] {
        Self::at([COLORS[0] + (color % COLOR_COLUMNS) as f32 * COLOR_STEP, COLORS[1] + (color / COLOR_COLUMNS) as f32 * COLOR_STEP])
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let inside = |place: [f32; 2], size: [f32; 2]| contains(Self::at(place), size, cursor);
        if let Some(race) = (0..2).find(|&race| inside(RACES[race], RACE_SIZE)) {
            return Some(Hit::Race(race));
        }
        if let Some(gender) = (0..2).find(|&gender| inside(GENDERS[gender], [62.0, 25.0])) {
            return Some(Hit::Gender(gender));
        }
        if let Some(turn) = (0..2).find(|&turn| inside(TURNS[turn], [22.0, 23.0])) {
            return Some(Hit::Turn(if turn == 0 { 1 } else { -1 }));
        }
        if let Some(style) = (0..self.styles()).find(|&style| contains(Self::hair_cell(style), HAIR_CELL, cursor)) {
            return Some(Hit::Hair(style));
        }
        if let Some(color) = (0..HAIR_COLORS).find(|&color| contains(Self::color_cell(color), [22.0, 22.0], cursor)) {
            return Some(Hit::Color(color));
        }
        let hit = [
            (NAME_FIELD, NAME_SIZE, Hit::Name),
            (CHECK, [157.0, 24.0], Hit::Check),
            (MAKE, [152.0, 27.0], Hit::Make),
            (BACK, [152.0, 27.0], Hit::Back),
            (CLOSE, [17.0, 16.0], Hit::Close),
        ];
        hit.into_iter().find(|&(place, size, _)| inside(place, size)).map(|(.., hit)| hit)
    }

    pub fn click(&mut self, hit: Hit, grf: &Grf, tables: &LookTables) -> Option<Action> {
        match hit {
            Hit::Race(race) if self.doram != (race == 1) => {
                self.doram = race == 1;
                self.hair = 1;
                self.color = 0;
            }
            Hit::Gender(gender) => self.male = gender == 0,
            Hit::Turn(step) => {
                self.direction = (self.direction as i8 + step).rem_euclid(8) as u8;
                return None;
            }
            Hit::Hair(style) => self.hair = style as u16 + 1,
            Hit::Color(color) => self.color = color as u16,
            Hit::Check => return (!self.name_long_enough()).then_some(Action::ShortName),
            Hit::Make => return Some(if self.name_long_enough() { Action::Make } else { Action::ShortName }),
            Hit::Back | Hit::Close => return Some(Action::Back),
            Hit::Race(_) | Hit::Name => return None,
        }
        self.dress(grf, tables);
        None
    }

    // ponytail: rathena has no packet to check a new name, so the check only looks at its length.
    /// The client's rule: at least 2 Hangul or 4 Latin letters (4 bytes in EUC-KR).
    fn name_long_enough(&self) -> bool {
        encoding_rs::EUC_KR.encode(self.name.trim()).0.len() >= 4
    }

    /// Typed text, kept within what the server takes.
    pub fn type_text(&mut self, text: &str) {
        for character in text.chars().filter(|character| !character.is_control()) {
            let longer = format!("{}{character}", self.name);
            if encoding_rs::EUC_KR.encode(&longer).0.len() <= NAME_BYTES {
                self.name = longer;
            }
        }
    }

    /// Decodes the figures' frames: the race panels' and the preview's.
    pub fn pose(&mut self, gpu: &Gpu) -> [Option<Vec<Placed>>; 3] {
        let direction = self.direction;
        let [human, doram, model] = &mut self.figures;
        [
            human.as_mut().map(|figure| figure.frame(gpu, crate::actor::IDLE, 0, 0, Timing::Loop(0.0))),
            doram.as_mut().map(|figure| figure.frame(gpu, crate::actor::IDLE, 0, 0, Timing::Loop(0.0))),
            model.as_mut().map(|figure| figure.frame(gpu, crate::actor::IDLE, direction, 0, Timing::Loop(0.0))),
        ]
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &'a self,
        layer: &mut Layer<'a>,
        assets: &'a Assets,
        font: &Font,
        messages: &[String],
        pointer: ([f32; 2], bool),
        poses: &[Option<Vec<Placed>>; 3],
        caret: bool,
    ) {
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        let (cursor, down) = pointer;
        let hit = self.hit(cursor);
        let look = |target: Hit| {
            let hovered = hit == Some(target);
            usize::from(hovered) + usize::from(hovered && down)
        };
        let backdrop = [((SCREEN[0] - BACKDROP[0]) / 2.0).floor(), ((SCREEN[1] - BACKDROP[1]) / 2.0).floor()];
        layer.sprites.push((&assets.backdrop, backdrop, assets.backdrop.size));
        layer.sprites.push((&assets.window, Self::at([0.0; 2]), assets.window.size));
        layer.texts.push((message(TITLE_MESSAGE).to_owned(), Self::at(TITLE), BLACK));

        // The races, each with its figure standing on the panel's pedestal.
        for race in 0..2 {
            let panel = &assets.races[race][usize::from(self.doram == (race == 1))];
            layer.sprites.push((panel, Self::at(RACES[race]), panel.size));
            layer.texts.push((message(RACE_MESSAGES[race]).to_owned(), Self::at(offset(RACES[race], RACE_TITLE)), WHITE));
            let lines = crate::window::wrap(font, message(RACE_DESCRIPTIONS[race]), RACE_TEXT_WIDTH, 4);
            for (row, line) in lines.into_iter().enumerate() {
                layer.texts.push((line.trim_start().to_owned(), Self::at(offset(RACES[race], [RACE_TEXT[0], RACE_TEXT[1] + row as f32 * 15.0])), DESCRIPTION));
            }
        }
        let figure_sprites = |index: usize, feet: [f32; 2]| {
            let figure = self.figures[index].as_ref();
            poses[index].iter().flatten().filter_map(move |(key, position, size)| Some((figure?.texture(*key), offset(feet, *position), *size)))
        };
        layer.sprites.extend(figure_sprites(0, Self::at(offset(RACES[0], RACE_FEET))));
        layer.sprites.extend(figure_sprites(1, Self::at(offset(RACES[1], RACE_FEET))));

        // Gender, the turning preview and the name.
        for gender in 0..2 {
            let chosen = self.male == (gender == 0);
            let state = if chosen { 1 } else { [0, 2, 3][look(Hit::Gender(gender))] };
            let image = &assets.genders[gender][state];
            layer.sprites.push((image, Self::at(GENDERS[gender]), image.size));
        }
        layer.sprites.extend(figure_sprites(2, Self::at(MODEL_FEET)));
        for (turn, step) in [(0, 1), (1, -1)] {
            let image = &assets.turns[turn][look(Hit::Turn(step))];
            layer.sprites.push((image, Self::at(TURNS[turn]), image.size));
        }
        let text_top = NAME_FIELD[1] + ((NAME_SIZE[1] - font.line_height()) / 2.0).floor();
        let name_place = Self::at([NAME_FIELD[0] + 5.0, text_top]);
        let typed = format!("{}{}", self.name, self.preedit);
        layer.texts.push((typed.clone(), name_place, BLACK));
        if caret {
            layer.sprites.push((&assets.black, [name_place[0] + font.width(&typed).round(), name_place[1] + 1.0], [1.0, font.line_height() - 2.0]));
        }
        button(layer, font, &assets.check[look(Hit::Check)], Self::at(CHECK), message(CHECK_MESSAGE));

        // Hairstyles and colors.
        layer.texts.push((message(HAIR_MESSAGE).to_owned(), Self::at(HAIR_TITLE), BLACK));
        let pictures = &assets.hairstyles[usize::from(self.doram) * 2 + usize::from(!self.male)];
        for (style, picture) in pictures.iter().enumerate() {
            let cell = Self::hair_cell(style);
            let state = if usize::from(self.hair) == style + 1 { 3 } else { look(Hit::Hair(style)) };
            layer.sprites.push((&assets.hair_cell[state], cell, assets.hair_cell[state].size));
            layer.sprites.push((picture, cell, picture.size));
        }
        layer.texts.push((message(COLOR_MESSAGE).to_owned(), Self::at(COLOR_TITLE), BLACK));
        for (color, images) in assets.colors.iter().enumerate() {
            let state = if usize::from(self.color) == color { 1 } else { [0, 2, 3][look(Hit::Color(color))] };
            layer.sprites.push((&images[state], Self::color_cell(color), images[state].size));
        }

        button(layer, font, &assets.button[look(Hit::Back)], Self::at(BACK), message(BACK_MESSAGE));
        button(layer, font, &assets.button[look(Hit::Make)], Self::at(MAKE), message(MAKE_MESSAGE));
        let close = &assets.close[look(Hit::Close)];
        layer.sprites.push((close, Self::at(CLOSE), close.size));
    }
}

/// A button bitmap with its label centered.
fn button<'a>(layer: &mut Layer<'a>, font: &Font, image: &'a Texture, place: [f32; 2], label: &str) {
    layer.sprites.push((image, place, image.size));
    let text = [place[0] + ((image.size[0] - font.width(label)) / 2.0).floor(), place[1] + ((image.size[1] - font.line_height()) / 2.0).floor()];
    layer.texts.push((label.to_owned(), text, BLACK));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_names_within_the_servers_limit() {
        let mut make = Make {
            slot: 2,
            doram: false,
            male: true,
            hair: 1,
            color: 0,
            name: String::new(),
            preedit: String::new(),
            direction: 0,
            figures: [None, None, None],
        };
        make.type_text("ab");
        assert!(!make.name_long_enough());
        make.type_text("가나다라마바사아자차카타");
        // Two Latin bytes and ten 2-byte Hangul syllables fill 22 of the 23 bytes; the rest don't fit.
        assert_eq!(make.name, "ab가나다라마바사아자차");
        assert!(make.name_long_enough());
    }
}
