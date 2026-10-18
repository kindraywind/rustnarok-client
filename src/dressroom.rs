//! The dress room (/dressroom, ZC_DRESSROOM_OPEN): the player's figure on a stage, trying other hairstyles and colors.
//! A preview only: nothing goes to the server and the player's real look never changes.

use rustnarok_client::grf::Grf;
use rustnarok_client::text::Font;

use crate::actor::Look;
use crate::render::{Gpu, Texture};
use crate::window::{Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI, contains, offset, screen};

/// The values the pickers step through (data\luafiles514\lua files\dressroom\), and the stages.
#[derive(Debug, Clone, PartialEq)]
pub struct Lists {
    pub hair: Vec<u16>,
    pub hair_color: Vec<u16>,
    pub clothes_color: Vec<u16>,
}

impl Default for Lists {
    /// What the data's lists hold, for when they can't be read.
    fn default() -> Self {
        Self { hair: (1..=29).collect(), hair_color: (0..=8).collect(), clothes_color: (0..=3).collect() }
    }
}

const STAGES: usize = 5;
const HAIR_STYLE: usize = 2764;
const HAIR_COLOR: usize = 2763;
const TURN_LEFT: usize = 2174;
const TURN_RIGHT: usize = 2175;

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Picker {
    Hair,
    HairColor,
    Clothes,
    Stage,
}

const PICKERS: [Picker; 4] = [Picker::Hair, Picker::HairColor, Picker::Clothes, Picker::Stage];

impl Picker {
    fn name(self) -> &'static str {
        match self {
            Picker::Hair => "hair",
            Picker::HairColor => "hair_color",
            Picker::Clothes => "clothes",
            Picker::Stage => "stage",
        }
    }

    /// Its label; the clothes color and the stage have none in the msgstringtable: their value shows alone.
    fn label(self) -> Option<usize> {
        match self {
            Picker::Hair => Some(HAIR_STYLE),
            Picker::HairColor => Some(HAIR_COLOR),
            Picker::Clothes | Picker::Stage => None,
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(TitleHit),
    /// A picker's arrow: false back, true forward.
    Step(Picker, bool),
    /// A rotate button: true left.
    Turn(bool),
    Window,
}

/// One step through `count` choices from `index`, wrapping around.
pub fn step(index: usize, count: usize, forward: bool) -> usize {
    if count == 0 {
        0
    } else if forward {
        (index + 1) % count
    } else {
        (index + count - 1) % count
    }
}

// ponytail: the whole layout is an estimate: the 200x180 stage in a window, the pickers under it, the rotate
// buttons on its lower corners.
const SIDE: f32 = 6.0;
const STAGE: [f32; 2] = [200.0, 180.0];
const STAGE_TOP: f32 = TITLE_HEIGHT + 4.0;
const WIDTH: f32 = STAGE[0] + 2.0 * SIDE;
/// Where the figure stands: the stage's lit spot, bottom middle.
const FEET: [f32; 2] = [100.0, 158.0];
const TURN: [f32; 2] = [20.0, 20.0];
const ROWS_TOP: f32 = STAGE_TOP + STAGE[1] + 6.0;
const ROW: f32 = 22.0;
const ARROW: [f32; 2] = [13.0, 13.0];
const BACK_X: f32 = 110.0;
const VALUE_X: f32 = 132.0;
const NEXT_X: f32 = 186.0;
const HEIGHT: f32 = ROWS_TOP + PICKERS.len() as f32 * ROW + 6.0;
const TEXT: [u8; 3] = [0, 0, 0];

pub struct Assets {
    stages: Vec<Texture>,
    arrows: [Texture; 2],
    /// Normal, hovered, pressed: left, then right.
    turn: [[Texture; 3]; 2],
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let basic = format!("{UI}basic_interface\\");
        let image = |path: String| gpu.load_texture(grf, &path, false);
        let turn = |side: &str| ["out", "over", "press"].map(|look| image(format!("{UI}equip_preview\\chr_arrow_rotate_{side}_{look}.bmp")));
        Self {
            stages: (0..STAGES).filter_map(|stage| gpu.try_load_texture(grf, &format!("{basic}dressroomback_{stage}.bmp"), false).ok()).collect(),
            arrows: ["arw_left", "arw_right"].map(|name| image(format!("{basic}{name}.bmp"))),
            turn: [turn("l"), turn("r")],
        }
    }
}

/// The dress room open: the picks (indexes into the lists), the stage and which way the figure faces.
pub struct Room {
    pub frame: Frame,
    pub hair: usize,
    pub hair_color: usize,
    pub clothes: usize,
    pub stage: usize,
    /// RO's direction, 0 facing south.
    pub direction: u8,
    /// The look changed since the figure was built.
    pub changed: bool,
}

impl Room {
    /// Opens on the player's own hair and colors (the first choice for a value the lists don't have) and stage 0.
    pub fn new(look: &Look, lists: &Lists) -> Self {
        let at = |list: &[u16], value: u16| list.iter().position(|&choice| choice == value).unwrap_or(0);
        let mut frame = Frame::new([((screen()[0] - WIDTH) / 2.0).floor(), 120.0], WIDTH);
        frame.open = true;
        Self {
            frame,
            hair: at(&lists.hair, look.hair),
            hair_color: at(&lists.hair_color, look.hair_color),
            clothes: at(&lists.clothes_color, look.clothes_color),
            stage: 0,
            direction: 0,
            changed: true,
        }
    }

    /// The figure's look: the player's, with the picks put on.
    pub fn look(&self, base: &Look, lists: &Lists) -> Look {
        let pick = |list: &[u16], index: usize, own: u16| list.get(index).copied().unwrap_or(own);
        Look {
            hair: pick(&lists.hair, self.hair, base.hair),
            hair_color: pick(&lists.hair_color, self.hair_color, base.hair_color),
            clothes_color: pick(&lists.clothes_color, self.clothes, base.clothes_color),
            ..base.clone()
        }
    }

    fn row_top(row: usize) -> f32 {
        ROWS_TOP + row as f32 * ROW
    }

    fn turn_at(left: bool) -> [f32; 2] {
        let x = if left { SIDE + 4.0 } else { SIDE + STAGE[0] - TURN[0] - 4.0 };
        [x, STAGE_TOP + STAGE[1] - TURN[1] - 4.0]
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let local = self.frame.local(cursor, HEIGHT)?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(Hit::Title(hit));
        }
        for left in [true, false] {
            if contains(Self::turn_at(left), TURN, local) {
                return Some(Hit::Turn(left));
            }
        }
        for (row, picker) in PICKERS.into_iter().enumerate() {
            let top = Self::row_top(row) + 3.0;
            if contains([BACK_X, top], ARROW, local) {
                return Some(Hit::Step(picker, false));
            }
            if contains([NEXT_X, top], ARROW, local) {
                return Some(Hit::Step(picker, true));
            }
        }
        Some(Hit::Window)
    }

    /// A click: a picker steps (wrapping), a rotate button turns the figure; true when the window closes.
    pub fn click(&mut self, hit: Hit, lists: &Lists) -> bool {
        match hit {
            Hit::Title(TitleHit::Close) => return true,
            Hit::Title(title) => self.frame.click_title(title),
            Hit::Step(picker, forward) => {
                match picker {
                    Picker::Hair => self.hair = step(self.hair, lists.hair.len(), forward),
                    Picker::HairColor => self.hair_color = step(self.hair_color, lists.hair_color.len(), forward),
                    Picker::Clothes => self.clothes = step(self.clothes, lists.clothes_color.len(), forward),
                    Picker::Stage => self.stage = step(self.stage, STAGES, forward),
                }
                self.changed |= picker != Picker::Stage;
            }
            // RO's directions go counterclockwise from south: turning left is one more.
            Hit::Turn(left) => self.direction = if left { (self.direction + 1) % 8 } else { (self.direction + 7) % 8 },
            Hit::Window => {}
        }
        false
    }

    /// Where the figure's feet are on screen.
    pub fn feet(&self) -> [f32; 2] {
        self.frame.at([SIDE + FEET[0], STAGE_TOP + FEET[1]])
    }

    /// The arrows and rotate buttons on screen (for the test driver): `dress_<picker>_prev` / `_next`,
    /// `dress_turn_left` / `_right`.
    pub fn button_boxes(&self) -> Vec<(String, [f32; 4])> {
        let place = |at: [f32; 2], size: [f32; 2]| {
            let [x, y] = self.frame.at(at);
            [x, y, size[0], size[1]]
        };
        let mut boxes: Vec<_> = PICKERS
            .into_iter()
            .enumerate()
            .flat_map(|(row, picker)| {
                let top = Self::row_top(row) + 3.0;
                [(format!("dress_{}_prev", picker.name()), place([BACK_X, top], ARROW)), (format!("dress_{}_next", picker.name()), place([NEXT_X, top], ARROW))]
            })
            .collect();
        boxes.push(("dress_turn_left".into(), place(Self::turn_at(true), TURN)));
        boxes.push(("dress_turn_right".into(), place(Self::turn_at(false), TURN)));
        boxes
    }

    /// The values the pickers show.
    fn value(&self, picker: Picker, lists: &Lists) -> String {
        match picker {
            Picker::Hair => lists.hair.get(self.hair).map_or_else(String::new, u16::to_string),
            Picker::HairColor => lists.hair_color.get(self.hair_color).map_or_else(String::new, u16::to_string),
            Picker::Clothes => lists.clothes_color.get(self.clothes).map_or_else(String::new, u16::to_string),
            Picker::Stage => self.stage.to_string(),
        }
    }

    /// The window; the figure's sprites (posed by the caller at `feet`) go in between the stage and the buttons.
    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &self,
        layer: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        font: &Font,
        messages: &[String],
        lists: &Lists,
        figure: Vec<crate::render::Sprite<'a>>,
        pointer: ([f32; 2], bool),
    ) {
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        let frame = &self.frame;
        // The client has no "dress room" line: the title stays empty.
        frame.draw_title(layer, chrome, "", pointer.0);
        if frame.minimized {
            return;
        }
        layer.sprites.push((&chrome.white, frame.at([0.0, TITLE_HEIGHT]), [WIDTH, HEIGHT - TITLE_HEIGHT]));
        for x in [0.0, WIDTH - 1.0] {
            layer.sprites.push((&chrome.border, frame.at([x, TITLE_HEIGHT]), [1.0, HEIGHT - TITLE_HEIGHT]));
        }
        layer.sprites.push((&chrome.border, frame.at([0.0, HEIGHT - 1.0]), [WIDTH, 1.0]));
        if let Some(stage) = assets.stages.get(self.stage) {
            layer.sprites.push((stage, frame.at([SIDE, STAGE_TOP]), STAGE));
        }
        // ponytail: basic_interface\dressroom.bmp (45x26) isn't the figure's shadow (it's a blue blob on white, nothing
        // keyed out); what it is, and whether the stage has a shadow, isn't known.
        layer.sprites.extend(figure);
        let hit = self.hit(pointer.0);
        for (index, left) in [true, false].into_iter().enumerate() {
            let look = usize::from(hit == Some(Hit::Turn(left))) + usize::from(hit == Some(Hit::Turn(left)) && pointer.1);
            layer.sprites.push((&assets.turn[index][look], frame.at(Self::turn_at(left)), TURN));
        }
        if let Some(Hit::Turn(left)) = hit {
            let words = message(if left { TURN_LEFT } else { TURN_RIGHT });
            layer.texts.push((words.to_owned(), offset(pointer.0, [12.0, 12.0]), TEXT));
        }
        for (row, picker) in PICKERS.into_iter().enumerate() {
            let top = Self::row_top(row);
            if let Some(label) = picker.label() {
                layer.texts.push((message(label).to_owned(), frame.at([SIDE + 4.0, top + 3.0]), TEXT));
            }
            layer.sprites.push((&assets.arrows[0], frame.at([BACK_X, top + 3.0]), ARROW));
            layer.sprites.push((&assets.arrows[1], frame.at([NEXT_X, top + 3.0]), ARROW));
            let value = self.value(picker, lists);
            let x = VALUE_X + ((NEXT_X - 4.0 - VALUE_X - font.width(&value)) / 2.0).floor();
            layer.texts.push((value, frame.at([x, top + 3.0]), TEXT));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_and_wraps() {
        let lists = Lists::default();
        assert_eq!(step(28, lists.hair.len(), true), 0, "hairstyle 29 to 1");
        assert_eq!(step(0, lists.hair_color.len(), false), 8, "hair color 0 back to 8");
        assert_eq!(step(2, 4, true), 3);
        assert_eq!(step(0, 0, true), 0);
    }

    #[test]
    fn builds_the_look_from_the_picks() {
        let lists = Lists::default();
        let base = Look { hair: 5, hair_color: 2, clothes_color: 1, job: 4001, ..Look::default() };
        let mut room = Room::new(&base, &lists);
        assert_eq!(room.look(&base, &lists), base, "it opens on the player's own");
        room.click(Hit::Step(Picker::Hair, true), &lists);
        room.click(Hit::Step(Picker::HairColor, false), &lists);
        room.click(Hit::Step(Picker::Stage, false), &lists);
        let look = room.look(&base, &lists);
        assert_eq!((look.hair, look.hair_color, look.clothes_color, look.job), (6, 1, 1, 4001));
        assert_eq!(room.stage, 4, "the stage wraps too");
        room.click(Hit::Turn(true), &lists);
        assert_eq!(room.direction, 1);
        room.click(Hit::Turn(false), &lists);
        room.click(Hit::Turn(false), &lists);
        assert_eq!(room.direction, 7);
        assert!(room.click(Hit::Title(TitleHit::Close), &lists));
    }
}
