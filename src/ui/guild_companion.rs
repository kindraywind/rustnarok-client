//! Without a guild: the Guild Companion (Alt+G), its tip window and the create window; with one, the same window for
//! disbanding it. The checks and the result lines are plain functions.

use rustnarok_client::grf::Grf;
use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};
use crate::window::{self, Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI, contains, offset};

/// "Guild Companion", "Join a guild or start your own!", "Create Guild", "Guild Name", "Disband the Guild", "Enter Guild
/// Name".
const COMPANION_TITLE: usize = 2074;
const COMPANION_TEXT: usize = 2075;
const CREATE_TITLE: usize = 2076;
const CREATE_LABEL: usize = 2077;
const DISBAND_TITLE: usize = 2088;
const DISBAND_LABEL: usize = 2089;
/// The lines and boxes the checks give.
pub const NAME_EMPTY: usize = 2080;
pub const NAME_TAG: usize = 2812;
pub const NAME_SPACE: usize = 2427;
pub const NAME_TOO_LONG: usize = 531;
pub const IN_A_CLAN: usize = 2605;
pub const DISBAND_FAILED: usize = 366;
/// "If you are using a guild storage, all items inside it will disappear.", shown when Disband is pressed.
pub const DISBAND_WARNING: usize = 2564;

pub const RED: [u8; 3] = [0xFF, 0x00, 0x00];
pub const ORANGE: [u8; 3] = [0xFF, 0x96, 0x6E];
pub const CYAN: [u8; 3] = [0x00, 0xFF, 0xFF];
pub const YELLOW: [u8; 3] = [0xFF, 0xFF, 0x00];

const COMPANION_SIZE: [f32; 2] = [200.0, 100.0];
const CREATE_SIZE: [f32; 2] = [150.0, 100.0];
const TIP_SIZE: [f32; 2] = [330.0, 230.0];
const FOOTER: f32 = 29.0;
const BUTTON: [f32; 2] = [42.0, 20.0];
const HELP_AT: ([f32; 2], [f32; 2]) = ([8.0, 76.0], [54.0, 20.0]);
const CREATE_AT: ([f32; 2], [f32; 2]) = ([70.0, 76.0], [71.0, 20.0]);
const COMPANION_OK: [f32; 2] = [148.0, 76.0];
const EDIT: ([f32; 2], [f32; 2]) = ([13.0, 42.0], [124.0, 20.0]);
const CREATE_OK: [f32; 2] = [58.0, 76.0];
const CREATE_CANCEL: [f32; 2] = [104.0, 76.0];
const NAME_MOST: usize = 23;
const EDIT_FILL: [u8; 4] = [0xE6, 0xE6, 0xE6, 0xFF];

/// The companion's first place: ((W − 640)/2 + 220, (H − 480)/2 + 120).
pub fn companion_at(screen: [f32; 2]) -> [f32; 2] {
    [(screen[0] - 640.0) / 2.0 + 220.0, (screen[1] - 480.0) / 2.0 + 120.0]
}

/// The create (or disband) window alone: ((W − 640)/2 + 200, (H − 480)/2 + 120).
pub fn create_alone_at(screen: [f32; 2]) -> [f32; 2] {
    [(screen[0] - 640.0) / 2.0 + 200.0, (screen[1] - 480.0) / 2.0 + 120.0]
}

/// Docked 2 px right of the companion.
pub fn create_docked_at(companion: [f32; 2]) -> [f32; 2] {
    [companion[0] + COMPANION_SIZE[0] + 2.0, companion[1]]
}

/// What OK in the create window comes to: a line (message, colour), a box (message), or the name to send.
#[derive(Debug, PartialEq, Clone)]
pub enum Checked {
    Line(usize, [u8; 3]),
    Box(usize),
    Send(String),
}

/// Whether a name holds the item link tag.
fn tagged(name: &str) -> bool {
    name.contains("</ITEML>") || name.contains("</iteml>")
}

/// A new guild's name, checked in order: empty, the tag, a full-width space at either end, 24 bytes or more, in a clan.
pub fn check_create(name: &str, in_clan: bool) -> Checked {
    if name.is_empty() {
        Checked::Line(NAME_EMPTY, RED)
    } else if tagged(name) {
        Checked::Line(NAME_TAG, RED)
    } else if name.starts_with('\u{3000}') || name.ends_with('\u{3000}') {
        Checked::Line(NAME_SPACE, ORANGE)
    } else if encoding_rs::EUC_KR.encode(name).0.len() >= 24 {
        Checked::Box(NAME_TOO_LONG)
    } else if in_clan {
        Checked::Line(IN_A_CLAN, ORANGE)
    } else {
        Checked::Send(name.to_owned())
    }
}

/// The guild's name typed to disband it: empty, the tag, then the box "You have failed to disband the guild." unless it
/// is the guild's own name.
pub fn check_disband(name: &str, guild: &str) -> Checked {
    if name.is_empty() {
        Checked::Line(NAME_EMPTY, RED)
    } else if tagged(name) {
        Checked::Line(NAME_TAG, RED)
    } else if name != guild {
        Checked::Box(DISBAND_FAILED)
    } else {
        Checked::Send(name.to_owned())
    }
}

/// ZC_RESULT_MAKE_GUILD's line, all guild lines: 0 374 in cyan; 1 375, 2 376, 3 405, 4 2687 in yellow; others none.
pub fn make_result(result: u8) -> Option<(usize, [u8; 3])> {
    Some(match result {
        0 => (374, CYAN),
        1 => (375, YELLOW),
        2 => (376, YELLOW),
        3 => (405, YELLOW),
        4 => (2687, YELLOW),
        _ => return None,
    })
}

/// ZC_ACK_DISORGANIZE_GUILD_RESULT's line, in yellow, and whether the guild is gone: 0 400, 1 401, 2 402; others none.
pub fn disband_result(result: u32) -> Option<(usize, bool)> {
    Some(match result {
        0 => (400, true),
        1 => (401, false),
        2 => (402, false),
        _ => return None,
    })
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(TitleHit),
    Help,
    Create,
    CompanionOk,
    /// The create (or disband) window's box, OK and Cancel; the tip window's title bar.
    Edit,
    CreateOk,
    CreateCancel,
    TipTitle(TitleHit),
    CreateTitle(TitleHit),
    Window,
}

/// The create window, or the same window disbanding the guild.
pub struct CreateWindow {
    pub disband: bool,
    pub frame: Frame,
    pub name: String,
}

impl CreateWindow {
    pub fn new(disband: bool, at: [f32; 2]) -> Self {
        let mut frame = Frame::new(at, CREATE_SIZE[0]);
        frame.open = true;
        Self { disband, frame, name: String::new() }
    }

    /// Typing into the box: up to 23 characters.
    pub fn type_text(&mut self, text: &str) {
        let room = NAME_MOST.saturating_sub(self.name.chars().count());
        self.name.extend(text.chars().filter(|character| !character.is_control()).take(room));
    }
}

/// The Guild Companion, its tip and its create window.
pub struct Companion {
    pub frame: Frame,
    pub tip: Option<Frame>,
    pub create: Option<CreateWindow>,
    placed: bool,
}

impl Default for Companion {
    fn default() -> Self {
        Self { frame: Frame::new([0.0, 0.0], COMPANION_SIZE[0]), tip: None, create: None, placed: false }
    }
}

impl Companion {
    /// Alt+G without a guild: open (first at its place, then where it was left) or close.
    pub fn toggle(&mut self, screen: [f32; 2]) {
        if self.frame.open {
            self.close();
        } else {
            if !self.placed {
                self.frame.position = companion_at(screen);
                self.placed = true;
            }
            self.frame.open = true;
            self.frame.minimized = false;
        }
    }

    /// The companion, the create window and the tip all close (made a guild, joined one, its OK).
    pub fn close(&mut self) {
        self.frame.open = false;
        self.tip = None;
        self.create = None;
    }

    /// The disband window, alone.
    pub fn open_disband(&mut self, screen: [f32; 2]) {
        self.create = Some(CreateWindow::new(true, create_alone_at(screen)));
    }

    pub fn typing(&self) -> bool {
        self.create.is_some()
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        if let Some(create) = &self.create
            && let Some(local) = create.frame.local(cursor, CREATE_SIZE[1])
        {
            return Some(if let Some(title) = create.frame.title_hit(local) {
                Hit::CreateTitle(title)
            } else if contains(EDIT.0, EDIT.1, local) {
                Hit::Edit
            } else if contains(CREATE_OK, BUTTON, local) {
                Hit::CreateOk
            } else if contains(CREATE_CANCEL, BUTTON, local) {
                Hit::CreateCancel
            } else {
                Hit::Window
            });
        }
        if let Some(tip) = &self.tip
            && let Some(local) = tip.local(cursor, TIP_SIZE[1])
        {
            return Some(tip.title_hit(local).map_or(Hit::Window, Hit::TipTitle));
        }
        if !self.frame.open {
            return None;
        }
        let local = self.frame.local(cursor, COMPANION_SIZE[1])?;
        Some(if let Some(title) = self.frame.title_hit(local) {
            Hit::Title(title)
        } else if contains(HELP_AT.0, HELP_AT.1, local) {
            Hit::Help
        } else if contains(CREATE_AT.0, CREATE_AT.1, local) {
            Hit::Create
        } else if contains(COMPANION_OK, BUTTON, local) {
            Hit::CompanionOk
        } else {
            Hit::Window
        })
    }

    /// A click: Help toggles the tip, Create the docked create window, OK closes everything; a title's close closes its
    /// window. The create window's OK is the caller's (it checks the name).
    pub fn click(&mut self, hit: Hit) {
        match hit {
            Hit::Title(TitleHit::Close) | Hit::CompanionOk => self.close(),
            Hit::Title(title) => self.frame.click_title(title),
            Hit::Help => {
                self.tip = if self.tip.is_some() {
                    None
                } else {
                    let mut tip = Frame::new(offset(self.frame.position, [0.0, COMPANION_SIZE[1] + 2.0]), TIP_SIZE[0]);
                    tip.open = true;
                    Some(tip)
                }
            }
            Hit::Create => {
                self.create = if self.create.is_some() { None } else { Some(CreateWindow::new(false, create_docked_at(self.frame.position))) };
            }
            Hit::TipTitle(TitleHit::Close) => self.tip = None,
            Hit::CreateTitle(TitleHit::Close) | Hit::CreateCancel => self.create = None,
            _ => {}
        }
    }

    /// The buttons, for the test driver.
    pub fn button_boxes(&self) -> Vec<(&'static str, [f32; 4])> {
        let place = |frame: &Frame, (corner, size): ([f32; 2], [f32; 2])| {
            let [x, y] = frame.at(corner);
            [x, y, size[0], size[1]]
        };
        let mut boxes = Vec::new();
        if self.frame.open && !self.frame.minimized {
            boxes.push(("companion_help", place(&self.frame, HELP_AT)));
            boxes.push(("companion_create", place(&self.frame, CREATE_AT)));
            boxes.push(("companion_ok", place(&self.frame, (COMPANION_OK, BUTTON))));
        }
        if let Some(create) = &self.create {
            boxes.push(("guild_name_box", place(&create.frame, EDIT)));
            boxes.push(("guild_name_ok", place(&create.frame, (CREATE_OK, BUTTON))));
            boxes.push(("guild_name_cancel", place(&create.frame, (CREATE_CANCEL, BUTTON))));
        }
        boxes
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &'a self,
        layer: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        font: &Font,
        messages: &[String],
        caret: bool,
        pointer: ([f32; 2], bool),
    ) {
        let message = |id: usize| messages.get(id).map_or("", String::as_str).trim_end().to_owned();
        let hit = self.hit(pointer.0);
        let look = |target: Hit| if hit == Some(target) { 1 + usize::from(pointer.1) } else { 0 };
        let body = |layer: &mut Layer<'a>, frame: &Frame, size: [f32; 2], footer: bool| {
            layer.sprites.push((&chrome.white, frame.at([0.0, TITLE_HEIGHT]), [size[0], size[1] - TITLE_HEIGHT]));
            if footer {
                let [left, middle, right] = &assets.footer;
                let top = size[1] - FOOTER;
                layer.sprites.push((left, frame.at([0.0, top]), [left.size[0], FOOTER]));
                layer.sprites.push((middle, frame.at([left.size[0], top]), [size[0] - left.size[0] - right.size[0], FOOTER]));
                layer.sprites.push((right, frame.at([size[0] - right.size[0], top]), [right.size[0], FOOTER]));
            }
        };
        // Minimized: the title bar alone.
        if self.frame.open && self.frame.minimized {
            self.frame.draw_title(layer, chrome, &message(COMPANION_TITLE), pointer.0);
        } else if self.frame.open {
            body(layer, &self.frame, COMPANION_SIZE, true);
            self.frame.draw_title(layer, chrome, &message(COMPANION_TITLE), pointer.0);
            layer.texts.push((message(COMPANION_TEXT), self.frame.at([15.0, 32.0]), [0, 0, 0]));
            layer.sprites.push((&assets.help[look(Hit::Help)], self.frame.at(HELP_AT.0), HELP_AT.1));
            layer.sprites.push((&assets.create[look(Hit::Create)], self.frame.at(CREATE_AT.0), CREATE_AT.1));
            layer.sprites.push((&assets.ok[look(Hit::CompanionOk)], self.frame.at(COMPANION_OK), BUTTON));
        }
        // ponytail: the tip's text is GuildTip.txt, which this data doesn't have: the window is empty.
        if let Some(tip) = &self.tip {
            body(layer, tip, TIP_SIZE, false);
            tip.draw_title(layer, chrome, "", pointer.0);
        }
        if let Some(create) = &self.create {
            let layer = layer.window();
            body(layer, &create.frame, CREATE_SIZE, true);
            let (title, label) = if create.disband { (DISBAND_TITLE, DISBAND_LABEL) } else { (CREATE_TITLE, CREATE_LABEL) };
            create.frame.draw_title(layer, chrome, &message(title), pointer.0);
            layer.texts.push((message(label), create.frame.at([16.0, 23.0]), [0, 0, 0]));
            layer.sprites.push((&assets.edit, create.frame.at(EDIT.0), EDIT.1));
            let shown = window::fit(font, &create.name, EDIT.1[0] - 6.0);
            let caret_x = font.width(&shown);
            layer.texts.push((shown, create.frame.at([EDIT.0[0] + 3.0, EDIT.0[1] + 3.0]), [0, 0, 0]));
            if caret {
                layer.sprites.push((&chrome.border, create.frame.at([EDIT.0[0] + 3.0 + caret_x, EDIT.0[1] + 4.0]), [1.0, 12.0]));
            }
            layer.sprites.push((&assets.ok[look(Hit::CreateOk)], create.frame.at(CREATE_OK), BUTTON));
            layer.sprites.push((&assets.cancel[look(Hit::CreateCancel)], create.frame.at(CREATE_CANCEL), BUTTON));
        }
    }
}

pub struct Assets {
    footer: [Texture; 3],
    help: [Texture; 3],
    create: [Texture; 3],
    ok: [Texture; 3],
    cancel: [Texture; 3],
    edit: Texture,
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |name: &str| gpu.try_load_texture(grf, &format!("{UI}{name}.bmp"), false).unwrap_or_else(|_| gpu.solid([0, 0, 0, 0]));
        let states = |name: &str| ["", "_a", "_b"].map(|look| image(&format!("{name}{look}")));
        Self {
            footer: ["basic_interface\\btnbar_left2", "basic_interface\\btnbar_mid2", "basic_interface\\btnbar_right2"].map(image),
            help: states("guild_helper\\btn_help"),
            create: states("guild_helper\\create_guild"),
            ok: states("btn_ok"),
            cancel: states("btn_cancel"),
            edit: gpu.solid(EDIT_FILL),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_a_new_guilds_name_in_order() {
        assert_eq!(check_create("", false), Checked::Line(NAME_EMPTY, RED));
        assert_eq!(check_create("a</ITEML>", false), Checked::Line(NAME_TAG, RED));
        assert_eq!(check_create("x</iteml>\u{3000}", false), Checked::Line(NAME_TAG, RED), "the tag first");
        assert_eq!(check_create("\u{3000}Crabs", false), Checked::Line(NAME_SPACE, ORANGE));
        assert_eq!(check_create("Crabs\u{3000}", false), Checked::Line(NAME_SPACE, ORANGE));
        assert_eq!(check_create("Crab Shack", false), Checked::Send("Crab Shack".into()), "a plain space is fine");
        assert_eq!(check_create(&"x".repeat(24), false), Checked::Box(NAME_TOO_LONG));
        assert_eq!(check_create(&"가".repeat(12), false), Checked::Box(NAME_TOO_LONG), "24 CP949 bytes");
        assert_eq!(check_create(&"x".repeat(23), true), Checked::Line(IN_A_CLAN, ORANGE));
        assert_eq!(check_create(&"x".repeat(23), false), Checked::Send("x".repeat(23)));
        // Disbanding: only the guild's own name is sent.
        assert_eq!(check_disband("", "Crabs"), Checked::Line(NAME_EMPTY, RED));
        assert_eq!(check_disband("Crab", "Crabs"), Checked::Box(DISBAND_FAILED));
        assert_eq!(check_disband("Crabs", "Crabs"), Checked::Send("Crabs".into()));
    }

    #[test]
    fn words_the_results_and_places_the_windows() {
        assert_eq!(
            (0..=5).map(make_result).collect::<Vec<_>>(),
            [Some((374, CYAN)), Some((375, YELLOW)), Some((376, YELLOW)), Some((405, YELLOW)), Some((2687, YELLOW)), None,]
        );
        assert_eq!([0, 1, 2, 9].map(disband_result), [Some((400, true)), Some((401, false)), Some((402, false)), None]);
        assert_eq!(companion_at([1024.0, 768.0]), [412.0, 264.0]);
        assert_eq!(create_docked_at([412.0, 264.0]), [614.0, 264.0]);
        assert_eq!(create_alone_at([1024.0, 768.0]), [392.0, 264.0]);
        let mut companion = Companion::default();
        companion.toggle([1024.0, 768.0]);
        assert_eq!((companion.frame.open, companion.frame.position), (true, [412.0, 264.0]));
        companion.click(Hit::Create);
        companion.click(Hit::Help);
        assert_eq!(companion.create.as_ref().map(|create| create.frame.position), Some([614.0, 264.0]));
        assert!(companion.tip.is_some());
        let at = |local: [f32; 2]| [614.0 + local[0], 264.0 + local[1]];
        assert_eq!(companion.hit(at([20.0, 50.0])), Some(Hit::Edit));
        assert_eq!(companion.hit(at([60.0, 80.0])), Some(Hit::CreateOk));
        companion.create.as_mut().unwrap().type_text(&"y".repeat(40));
        assert_eq!(companion.create.as_ref().unwrap().name.len(), 23);
        companion.click(Hit::CompanionOk);
        assert!(!companion.frame.open && companion.tip.is_none() && companion.create.is_none());
    }
}
