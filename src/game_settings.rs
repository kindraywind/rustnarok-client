//! Game Settings (2374): one 400×350 window, centred and made fresh each time it opens, with five tabs. Basic has the
//! music and effect sound rows, Effect the effect switches this client has, Graphics the screen mode; Control and
//! Other stay empty here.

use rustnarok_client::grf::Grf;
use rustnarok_client::text::Font;

use crate::options::GraphicsSettings;
use crate::render::{Gpu, Texture};
use crate::sound::{MAX_VOLUME, Settings};
use crate::window::{self, Chrome, Frame, TITLE_HEIGHT};
use crate::{Layer, UI, contains};

const SIZE: [f32; 2] = [400.0, 350.0];
const TITLE: usize = 2374;
/// The tabs, 72×18 at y 17 and overlapping by a pixel: Basic, Effect, Control, Graphics, Other.
const TABS: [(f32, usize); 5] = [(0.0, 4142), (71.0, 4143), (142.0, 4144), (213.0, 4145), (284.0, 4222)];
const TAB: [f32; 2] = [72.0, 18.0];
const TAB_TOP: f32 = 17.0;
pub const BASIC: usize = 0;
pub const EFFECT: usize = 1;
pub const GRAPHICS: usize = 3;
/// The pages: 400×288 at (0, 34), outlined in #CECECE (`options::OUTLINE`).
const PAGE_TOP: f32 = 34.0;
const PAGE_HEIGHT: f32 = 288.0;
/// Reset, Apply and Close, 42×20 at y 326.
const BUTTONS: [f32; 3] = [5.0, 301.0, 348.0];
const BUTTON: [f32; 2] = [42.0, 20.0];
const BUTTON_TOP: f32 = 326.0;
/// Every group's heading at (19, 12) inside it.
const HEADING: [f32; 2] = [19.0, 12.0];
/// The sound group, page y 36: "Audio Settings"; rows at y 12 and 36, each with its label (BGM, SFX) at x 110, a
/// slider 174×13 at (152, y − 1), the Off box at (334, y + 1) and "Off" at (348, y).
const SOUND_TOP: f32 = 36.0;
const SOUND_TITLE: usize = 1485;
const SOUND_ROWS: [f32; 2] = [12.0, 36.0];
const SOUND_LABELS: [usize; 2] = [1495, 1496];
const LABEL_X: f32 = 110.0;
const SLIDER_X: f32 = 152.0;
const SLIDER: f32 = 174.0;
const ARROW: f32 = 13.0;
// ponytail: a 24 px thumb with the volume on it; the game's is sized by the bar's page.
const THUMB: f32 = 24.0;
const OFF_BOX: f32 = 334.0;
const OFF_X: f32 = 348.0;
const OFF: usize = 4148;
/// The Effect page's rows, 387×26 from page y 1: the switch at (8, 5), the title at (47, 6), the description 252
/// wide at (138, 6).
const ROW: f32 = 26.0;
const SWITCH: [f32; 2] = [8.0, 5.0];
const ROW_TITLE: f32 = 47.0;
const ROW_TEXT: f32 = 138.0;
const ROW_TEXT_WIDTH: f32 = 252.0;
// ponytail: the game's rows come from GameSettings.lub, which this data lacks; these are the switches this client
// has, titled and described by their messages, with checkboxes for the game's toggle images.
/// Effects (/effect), the aura shown (/aura2), the mini aura (/aura), the screen shaking (/quake).
const SWITCHES: [(usize, usize); 4] = [(3172, 37), (4160, 3162), (4163, 710), (2805, 3161)];
/// The screen mode group, Graphics page y 160: "Screen mode", the Full screen and Window radios at (150, 12) and
/// (270, 12), their labels 17 px right.
const SCREEN_TOP: f32 = 160.0;
const SCREEN_TITLE: usize = 4183;
const SCREEN_RADIOS: [(f32, usize); 2] = [(150.0, 4184), (270.0, 4185)];
/// Reset asks first.
pub const RESET_ASKS: usize = 3166;

pub struct Assets {
    radio: [Texture; 2],
    reset: [Texture; 3],
    apply: [Texture; 3],
    close: [Texture; 3],
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |name: String| gpu.load_texture(grf, &format!("{UI}{name}.bmp"), false);
        let button = |name: &str| ["", "_a", "_b"].map(|look| image(format!("btn_{name}{look}")));
        Self { radio: ["radiobtn_off", "radiobtn_on"].map(|name| image(name.into())), reset: button("reset"), apply: button("apply"), close: button("close") }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Hit {
    Close,
    Tab(usize),
    Reset,
    Apply,
    CloseButton,
    /// A sound row's left or right arrow, its track at a volume, its Off box.
    Less(usize),
    More(usize),
    Track(usize, u8),
    Off(usize),
    /// An Effect page switch.
    Switch(usize),
    /// A screen mode radio: full screen or not.
    Screen(bool),
    Title,
    Window,
}

/// What a click asks the client to do.
#[derive(Debug, PartialEq)]
pub enum Action {
    /// The sound settings changed.
    Sound,
    /// The effect switches changed.
    Graphics,
    /// Reset asks first (3166).
    AskReset,
    /// Apply: full screen or not.
    Screen(bool),
    Close,
}

pub struct GameSettings {
    pub frame: Frame,
    pub tab: usize,
    /// The slider whose thumb is held.
    dragging: Option<usize>,
    /// The screen mode picked on the Graphics page, until Apply.
    pub fullscreen: bool,
}

impl GameSettings {
    /// Opens centred on Basic, the screen mode as it is.
    pub fn new(screen: [f32; 2], fullscreen: bool) -> Self {
        let mut frame = Frame::new([((screen[0] - SIZE[0]) / 2.0).floor(), ((screen[1] - SIZE[1]) / 2.0).floor()], SIZE[0]);
        frame.open = true;
        Self { frame, tab: BASIC, dragging: None, fullscreen }
    }

    /// The volume at `x` across the window (the thumb's middle there).
    fn volume_at(x: f32) -> u8 {
        let along = ((x - SLIDER_X - ARROW - THUMB / 2.0) / (SLIDER - 2.0 * ARROW - THUMB)).clamp(0.0, 1.0);
        (along * f32::from(MAX_VOLUME)).round() as u8
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let local = self.frame.local(cursor, SIZE[1])?;
        if contains(self.frame.close_button(), window::BUTTON_SIZE, local) {
            return Some(Hit::Close);
        }
        if local[1] < TITLE_HEIGHT {
            return Some(Hit::Title);
        }
        if (TAB_TOP..PAGE_TOP).contains(&local[1]) {
            // The later tab is on top where they overlap.
            return Some(TABS.iter().rposition(|&(x, _)| (x..x + TAB[0]).contains(&local[0])).map_or(Hit::Window, Hit::Tab));
        }
        for (button, hit) in BUTTONS.into_iter().zip([Hit::Reset, Hit::Apply, Hit::CloseButton]) {
            if contains([button, BUTTON_TOP], BUTTON, local) {
                return Some(hit);
            }
        }
        let [x, y] = [local[0], local[1] - PAGE_TOP];
        Some(match self.tab {
            BASIC => SOUND_ROWS
                .iter()
                .position(|&row| (SOUND_TOP + row - 1.0..SOUND_TOP + row + ARROW).contains(&y))
                .and_then(|row| match x {
                    x if (SLIDER_X..SLIDER_X + ARROW).contains(&x) => Some(Hit::Less(row)),
                    x if (SLIDER_X + SLIDER - ARROW..SLIDER_X + SLIDER).contains(&x) => Some(Hit::More(row)),
                    x if (SLIDER_X..SLIDER_X + SLIDER).contains(&x) => Some(Hit::Track(row, Self::volume_at(x))),
                    x if (OFF_BOX..SIZE[0]).contains(&x) => Some(Hit::Off(row)),
                    _ => None,
                })
                .unwrap_or(Hit::Window),
            EFFECT => {
                let row = ((y - 1.0) / ROW).floor();
                if (0.0..SWITCHES.len() as f32).contains(&row) && x < ROW_TITLE { Hit::Switch(row as usize) } else { Hit::Window }
            }
            GRAPHICS if (SCREEN_TOP..SCREEN_TOP + 32.0).contains(&y) => {
                SCREEN_RADIOS.iter().position(|&(radio, _)| (radio..radio + 90.0).contains(&x)).map_or(Hit::Window, |radio| Hit::Screen(radio == 0))
            }
            _ => Hit::Window,
        })
    }

    /// A press: the title bar grabs the window, a track the thumb there. Returns whether the sound changed.
    pub fn press(&mut self, hit: Hit, cursor: [f32; 2], sound: &mut Settings) -> bool {
        match hit {
            Hit::Title => self.frame.start_drag(cursor),
            // An Off sound doesn't move.
            Hit::Track(row, volume) if sound.on[row] => {
                self.dragging = Some(row);
                sound.volume[row] = volume;
                return true;
            }
            _ => {}
        }
        false
    }

    /// A click. The sound and the switches change at once; the screen mode waits for Apply.
    pub fn click(&mut self, hit: Hit, sound: &mut Settings, graphics: &mut GraphicsSettings) -> Option<Action> {
        match hit {
            Hit::Close | Hit::CloseButton => {
                self.frame.open = false;
                Some(Action::Close)
            }
            Hit::Tab(tab) => {
                self.tab = tab;
                None
            }
            Hit::Reset => Some(Action::AskReset),
            Hit::Apply if self.tab == GRAPHICS => Some(Action::Screen(self.fullscreen)),
            Hit::Less(row) if sound.on[row] => {
                sound.volume[row] = sound.volume[row].saturating_sub(1);
                Some(Action::Sound)
            }
            Hit::More(row) if sound.on[row] => {
                sound.volume[row] = (sound.volume[row] + 1).min(MAX_VOLUME);
                Some(Action::Sound)
            }
            Hit::Off(row) => {
                sound.on[row] = !sound.on[row];
                Some(Action::Sound)
            }
            Hit::Switch(row) => {
                let on = !switch_on(graphics, row);
                set_switch(graphics, row, on);
                Some(Action::Graphics)
            }
            Hit::Screen(fullscreen) => {
                self.fullscreen = fullscreen;
                None
            }
            _ => None,
        }
    }

    /// The held thumb (or the window) follows the pointer; returns whether the sound changed.
    pub fn drag_to(&mut self, cursor: [f32; 2], sound: &mut Settings) -> bool {
        self.frame.drag_to(cursor, SIZE[1]);
        let Some(row) = self.dragging else { return false };
        let volume = Self::volume_at(cursor[0] - self.frame.position[0]);
        std::mem::replace(&mut sound.volume[row], volume) != volume
    }

    /// Lets go of the thumb or the window; returns whether a thumb was held.
    pub fn drop_drag(&mut self) -> bool {
        self.frame.drop_drag();
        self.dragging.take().is_some()
    }

    /// The tabs and buttons on screen (for the test driver).
    pub fn button_boxes(&self) -> Vec<(String, [f32; 4])> {
        let place = |[x, y]: [f32; 2], [width, height]: [f32; 2]| {
            let [x, y] = self.frame.at([x, y]);
            [x, y, width, height]
        };
        let mut boxes: Vec<_> =
            TABS.iter().enumerate().map(|(tab, &(x, _))| (format!("settings_tab:{tab}"), place([x + 1.0, TAB_TOP], [TAB[0] - 2.0, TAB[1]]))).collect();
        boxes.extend(
            ["settings_reset", "settings_apply", "settings_close"].into_iter().zip(BUTTONS).map(|(name, x)| (name.to_owned(), place([x, BUTTON_TOP], BUTTON))),
        );
        match self.tab {
            BASIC => boxes.extend(
                SOUND_ROWS.iter().enumerate().map(|(row, &y)| (format!("sound_off:{row}"), place([OFF_BOX, PAGE_TOP + SOUND_TOP + y + 1.0], [10.0, 10.0]))),
            ),
            EFFECT => boxes.extend(
                (0..SWITCHES.len()).map(|row| (format!("switch:{row}"), place([SWITCH[0], PAGE_TOP + 1.0 + row as f32 * ROW + SWITCH[1]], [20.0, 15.0]))),
            ),
            GRAPHICS => boxes.extend(
                SCREEN_RADIOS.iter().enumerate().map(|(radio, &(x, _))| (format!("screen:{radio}"), place([x, PAGE_TOP + SCREEN_TOP + 12.0], [12.0, 12.0]))),
            ),
            _ => {}
        }
        boxes
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &self,
        layer: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        options: &'a crate::options::Assets,
        font: &Font,
        messages: &[String],
        (sound, graphics): (&Settings, &GraphicsSettings),
        pointer: ([f32; 2], bool),
    ) {
        let at = |local: [f32; 2]| self.frame.at(local);
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        let black = [0, 0, 0];
        self.frame.draw_bar(layer, chrome);
        self.frame.draw_bare_title(layer, chrome, message(TITLE), pointer.0);
        let [width, height] = SIZE;
        layer.sprites.push((&chrome.white, at([0.0, TITLE_HEIGHT]), [width, height - TITLE_HEIGHT]));
        // The tabs: white with a #CECECE edge, the open one joined to its page.
        let outline = &options.outline;
        for (tab, &(x, label)) in TABS.iter().enumerate() {
            let open = tab == self.tab;
            layer.sprites.push((outline, at([x, TAB_TOP]), [TAB[0], 1.0]));
            layer.sprites.push((outline, at([x, TAB_TOP]), [1.0, TAB[1]]));
            layer.sprites.push((outline, at([x + TAB[0] - 1.0, TAB_TOP]), [1.0, TAB[1]]));
            layer.sprites.push((&chrome.white, at([x + 1.0, TAB_TOP + 1.0]), [TAB[0] - 2.0, TAB[1] - 1.0 + f32::from(u8::from(open))]));
            let text = window::fit(font, message(label), TAB[0] - 6.0);
            let color = if open { black } else { [0x70, 0x70, 0x70] };
            layer.texts.push((text.clone(), at([x + ((TAB[0] - font.width(&text)) / 2.0).floor(), TAB_TOP + 3.0]), color));
        }
        // The page's outline, under the open tab left open.
        let page = |local: [f32; 2]| at([local[0], PAGE_TOP + local[1]]);
        let open_x = TABS[self.tab].0;
        layer.sprites.push((outline, page([0.0, 0.0]), [open_x, 1.0]));
        layer.sprites.push((outline, page([open_x + TAB[0] - 1.0, 0.0]), [width - open_x - TAB[0] + 1.0, 1.0]));
        layer.sprites.push((outline, page([0.0, 0.0]), [1.0, PAGE_HEIGHT]));
        layer.sprites.push((outline, page([width - 1.0, 0.0]), [1.0, PAGE_HEIGHT]));
        layer.sprites.push((outline, page([0.0, PAGE_HEIGHT - 1.0]), [width, 1.0]));
        let hit = self.hit(pointer.0);
        match self.tab {
            BASIC => {
                let group = |local: [f32; 2]| page([local[0], SOUND_TOP + local[1]]);
                layer.texts.push((message(SOUND_TITLE).to_owned(), group(HEADING), black));
                for (row, (&y, &label)) in SOUND_ROWS.iter().zip(&SOUND_LABELS).enumerate() {
                    layer.texts.push((message(label).trim().to_owned(), group([LABEL_X, y]), black));
                    let [less, more] = &options.arrows;
                    layer.sprites.push((less, group([SLIDER_X, y - 1.0]), less.size));
                    layer.sprites.push((&options.track, group([SLIDER_X + ARROW, y - 1.0]), [SLIDER - 2.0 * ARROW, ARROW]));
                    layer.sprites.push((more, group([SLIDER_X + SLIDER - ARROW, y - 1.0]), more.size));
                    // The thumb, placed by the volume, with the volume on it.
                    let left = SLIDER_X + ARROW + (SLIDER - 2.0 * ARROW - THUMB) * f32::from(sound.volume[row]) / f32::from(MAX_VOLUME);
                    let [start, body, end] = &options.thumb;
                    layer.sprites.push((start, group([left, y - 1.0]), start.size));
                    layer.sprites.push((body, group([left + start.size[0], y - 1.0]), [THUMB - start.size[0] - end.size[0], body.size[1]]));
                    layer.sprites.push((end, group([left + THUMB - end.size[0], y - 1.0]), end.size));
                    let volume = sound.volume[row].to_string();
                    layer.texts.push((volume.clone(), group([left + ((THUMB - font.width(&volume)) / 2.0).floor(), y]), black));
                    let check = &options.check[usize::from(!sound.on[row])];
                    layer.sprites.push((check, group([OFF_BOX, y + 1.0]), check.size));
                    layer.texts.push((message(OFF).to_owned(), group([OFF_X, y]), black));
                }
            }
            EFFECT => {
                for (row, &(title, text)) in SWITCHES.iter().enumerate() {
                    let top = 1.0 + row as f32 * ROW;
                    let check = &options.check[usize::from(switch_on(graphics, row))];
                    layer.sprites.push((check, page([SWITCH[0], top + SWITCH[1] + 2.0]), check.size));
                    let title = message(title).trim_end_matches(" ON").to_owned();
                    layer.texts.push((window::fit(font, &title, ROW_TEXT - ROW_TITLE - 4.0), page([ROW_TITLE, top + 6.0]), black));
                    layer.texts.push((window::fit(font, message(text), ROW_TEXT_WIDTH), page([ROW_TEXT, top + 6.0]), black));
                }
            }
            GRAPHICS => {
                let group = |local: [f32; 2]| page([local[0], SCREEN_TOP + local[1]]);
                layer.texts.push((message(SCREEN_TITLE).to_owned(), group(HEADING), black));
                for (radio, &(x, label)) in SCREEN_RADIOS.iter().enumerate() {
                    let image = &assets.radio[usize::from(self.fullscreen == (radio == 0))];
                    layer.sprites.push((image, group([x, 12.0]), image.size));
                    layer.texts.push((message(label).to_owned(), group([x + 17.0, 12.0]), black));
                }
            }
            _ => {}
        }
        for ((images, x), target) in [&assets.reset, &assets.apply, &assets.close].into_iter().zip(BUTTONS).zip([Hit::Reset, Hit::Apply, Hit::CloseButton]) {
            let look = if hit == Some(target) { 1 + usize::from(pointer.1) } else { 0 };
            layer.sprites.push((&images[look], at([x, BUTTON_TOP]), BUTTON));
        }
    }
}

/// Reset: the sound and the effect switches back to their defaults (the screen mode stays).
pub fn reset(sound: &mut Settings, graphics: &mut GraphicsSettings) {
    *sound = Settings::default();
    *graphics = GraphicsSettings { fullscreen: graphics.fullscreen, ..GraphicsSettings::default() };
}

/// Whether an Effect page switch is on: effects shown, the aura shown, the mini aura, the screen shaking.
pub fn switch_on(graphics: &GraphicsSettings, row: usize) -> bool {
    match row {
        0 => graphics.effects_on,
        1 => !graphics.aura_off,
        2 => !graphics.aura,
        _ => graphics.quake,
    }
}

fn set_switch(graphics: &mut GraphicsSettings, row: usize, on: bool) {
    match row {
        0 => graphics.effects_on = on,
        1 => graphics.aura_off = !on,
        2 => graphics.aura = !on,
        _ => graphics.quake = on,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_centred_with_five_tabs_and_three_buttons() {
        let window = GameSettings::new([1024.0, 768.0], false);
        assert_eq!(window.frame.position, [312.0, 209.0]);
        let corner = window.frame.position;
        let at = |local: [f32; 2]| [corner[0] + local[0], corner[1] + local[1]];
        assert_eq!(window.hit(at([386.0 + 5.0, 8.0])), Some(Hit::Close));
        assert_eq!(window.hit(at([71.5, 20.0])), Some(Hit::Tab(1)), "the later tab on top");
        assert_eq!(window.hit(at([300.0, 25.0])), Some(Hit::Tab(4)));
        assert_eq!(window.hit(at([10.0, 330.0])), Some(Hit::Reset));
        assert_eq!(window.hit(at([305.0, 330.0])), Some(Hit::Apply));
        assert_eq!(window.hit(at([350.0, 330.0])), Some(Hit::CloseButton));
        assert_eq!(window.hit([0.0, 0.0]), None);
    }

    #[test]
    fn sound_rows_step_by_one_and_turn_off() {
        let mut window = GameSettings::new([1024.0, 768.0], false);
        let (mut sound, mut graphics) = (Settings::default(), GraphicsSettings::default());
        let corner = window.frame.position;
        let at = |local: [f32; 2]| [corner[0] + local[0], corner[1] + local[1]];
        let row = |row: usize| PAGE_TOP + SOUND_TOP + SOUND_ROWS[row] + 4.0;
        assert_eq!(window.hit(at([SLIDER_X + 2.0, row(0)])), Some(Hit::Less(0)));
        assert_eq!(window.hit(at([SLIDER_X + SLIDER - 2.0, row(1)])), Some(Hit::More(1)));
        assert_eq!(window.hit(at([SLIDER_X + ARROW + 1.0, row(0)])), Some(Hit::Track(0, 0)));
        assert_eq!(window.hit(at([SLIDER_X + SLIDER - ARROW - 1.0, row(0)])), Some(Hit::Track(0, MAX_VOLUME)));
        assert_eq!(window.hit(at([OFF_BOX + 3.0, row(1)])), Some(Hit::Off(1)));
        assert_eq!(window.click(Hit::More(0), &mut sound, &mut graphics), Some(Action::Sound));
        assert_eq!(sound.volume[0], 101);
        assert_eq!(window.click(Hit::Off(0), &mut sound, &mut graphics), Some(Action::Sound));
        assert!(!sound.on[0]);
        // Off, the slider doesn't move.
        assert_eq!(window.click(Hit::Less(0), &mut sound, &mut graphics), None);
        assert!(!window.press(Hit::Track(0, 5), at([0.0, 0.0]), &mut sound));
        assert!(window.press(Hit::Track(1, 20), at([0.0, 0.0]), &mut sound));
        assert!(window.drag_to(at([SLIDER_X + SLIDER / 2.0, 0.0]), &mut sound));
        assert_eq!(sound.volume[1], 64);
        assert!(window.drop_drag());
    }

    #[test]
    fn switches_effects_and_waits_for_apply_on_the_screen_mode() {
        let mut window = GameSettings::new([1024.0, 768.0], false);
        let (mut sound, mut graphics) = (Settings::default(), GraphicsSettings::default());
        window.click(Hit::Tab(EFFECT), &mut sound, &mut graphics);
        let corner = window.frame.position;
        let at = |local: [f32; 2]| [corner[0] + local[0], corner[1] + local[1]];
        assert_eq!(window.hit(at([12.0, PAGE_TOP + 1.0 + ROW + 8.0])), Some(Hit::Switch(1)));
        assert_eq!(window.click(Hit::Switch(0), &mut sound, &mut graphics), Some(Action::Graphics));
        assert!(!graphics.effects_on);
        window.click(Hit::Switch(1), &mut sound, &mut graphics);
        assert!(graphics.aura_off);
        window.click(Hit::Switch(2), &mut sound, &mut graphics);
        assert!(!graphics.aura && switch_on(&graphics, 2));
        // Apply does nothing but on the Graphics page, where it sets the screen mode picked.
        assert_eq!(window.click(Hit::Apply, &mut sound, &mut graphics), None);
        window.click(Hit::Tab(GRAPHICS), &mut sound, &mut graphics);
        let corner = window.frame.position;
        let at = |local: [f32; 2]| [corner[0] + local[0], corner[1] + local[1]];
        assert_eq!(window.hit(at([152.0, PAGE_TOP + SCREEN_TOP + 14.0])), Some(Hit::Screen(true)));
        window.click(Hit::Screen(true), &mut sound, &mut graphics);
        assert_eq!(window.click(Hit::Apply, &mut sound, &mut graphics), Some(Action::Screen(true)));
        assert_eq!(window.click(Hit::Reset, &mut sound, &mut graphics), Some(Action::AskReset));
        assert_eq!(window.click(Hit::CloseButton, &mut sound, &mut graphics), Some(Action::Close));
        assert!(!window.frame.open);
    }
}
