//! Shortcut Settings (1494): every row of the shortcut tables on four tabs (Hotkey Bar, Hotkey Bar 2, Interface,
//! Macros), 36 to a page in two columns of 18. A row pressed takes the next key; changes wait in a draft for OK, which
//! writes UserKeys.lua. "Enable Battle Mode" applies on the click.

use rustnarok_client::grf::Grf;
use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};
use crate::shortcuts::{self, Action, Modifier, ShortcutKey, Shortcuts};
use crate::window::{self, Chrome, Frame, TITLE_HEIGHT};
use crate::{Layer, UI, contains};

// ponytail: the game's size comes from GetWindowInfo(0x9c), which this data lacks; 436 fits the layout.
const SIZE: [f32; 2] = [436.0, 432.0];
const TITLE: usize = 1494;
/// The tabs at (3, 20): the table each lists, its label, and words for an empty label.
const TABS: [(u8, usize, &str); 4] = [(1, 1491, "Hotkey Bar"), (4, 3595, "Hotkey Bar 2"), (2, 1492, "Interface"), (3, 1493, "Macros")];
const TABS_AT: [f32; 2] = [3.0, 20.0];
// ponytail: the tabs 70 × 17.
const TAB: [f32; 2] = [70.0, 17.0];
/// Rows 20 apart from y 40, 18 a column, two columns.
const ROWS_TOP: f32 = 40.0;
const ROW: f32 = 20.0;
const PER_COLUMN: usize = 18;
const PER_PAGE: usize = 36;
/// The key box, 80 × 19 at the cell's right end.
const KEY_BOX: [f32; 2] = [80.0, 19.0];
/// Key boxes by tab, the picked row's cell, and the lines.
const BOX_COLORS: [[u8; 4]; 4] = [[0xD6, 0xEF, 0xFF, 0xFF], [0xFF, 0xD6, 0xEF, 0xFF], [0xFE, 0xFF, 0xDF, 0xFF], [0xFE, 0xFF, 0xDF, 0xFF]];
const PICKED: [u8; 4] = [0xFF, 0xE1, 0xE1, 0xFF];
const LINE: [u8; 4] = [0xC0, 0xC0, 0xC0, 0xFF];
/// "Press a key to assign…", "Unable to specify a single key.", "Unable to specify the key assigned.", the question
/// when the key is taken, "Not Assigned", and OK's chat line.
const ASSIGN: usize = 1486;
const SINGLE_KEY: usize = 1487;
const TAKEN_KEY: usize = 1488;
pub const ASKS_REBIND: usize = 1489;
const NOT_ASSIGNED: (usize, &str) = (1518, "Not Assigned");
pub const SAVED: usize = 3572;
const BATTLE: (usize, &str) = (1775, "Enable Battle Mode");
/// The page arrows at (w − 80, 25) and (w − 20, 25); the page in #FF3C7F, " / n" in #666666.
const PREV: [f32; 2] = [SIZE[0] - 80.0, 25.0];
const NEXT: [f32; 2] = [SIZE[0] - 20.0, 25.0];
const PAGE_COLOR: [u8; 3] = [0xFF, 0x3C, 0x7F];
const PAGES_COLOR: [u8; 3] = [0x66, 0x66, 0x66];
/// OK, Cancel, Reset and Close, 42 × 20 at y h − 25; the battle-mode box at (80, h − 22).
const BUTTONS: [f32; 4] = [SIZE[0] - 139.0, SIZE[0] - 92.0, 5.0, SIZE[0] - 46.0];
const BUTTON: [f32; 2] = [42.0, 20.0];
const BUTTON_TOP: f32 = SIZE[1] - 25.0;
const BATTLE_AT: [f32; 2] = [80.0, SIZE[1] - 22.0];

pub struct Assets {
    arrows: [Texture; 2],
    buttons: [[Texture; 3]; 4],
    fills: [Texture; 4],
    picked: Texture,
    line: Texture,
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |name: String| gpu.load_texture(grf, &format!("{UI}{name}.bmp"), false);
        let button = |name: &str| ["", "_a", "_b"].map(|look| image(format!("btn_{name}{look}")));
        Self {
            arrows: ["left", "right"].map(|side| image(format!("basic_interface\\arw_{side}"))),
            buttons: [button("ok"), button("cancel"), button("reset"), button("close")],
            fills: BOX_COLORS.map(|color| gpu.solid(color)),
            picked: gpu.solid(PICKED),
            line: gpu.solid(LINE),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Hit {
    Close,
    Title,
    Tab(usize),
    /// A row, by its place in the shortcut list.
    Row(usize),
    Prev,
    Next,
    Ok,
    Cancel,
    Reset,
    CloseButton,
    Battle,
    Window,
}

/// What a click asks the client to do.
#[derive(Debug, PartialEq)]
pub enum Click {
    /// OK: the draft becomes the shortcuts and is written, 3572 is said, and the window closes.
    Save,
    /// Reset: the draft goes back to the data's keys (nothing saved until OK).
    Reset,
    BattleMode,
    Close,
}

/// A key pressed while a row is picked.
#[derive(Debug, PartialEq)]
pub enum Pressed {
    /// The row took it, or lost its key (Esc).
    Bound,
    /// Refused: the notice says why.
    Refused,
    /// Another row has it: ask (1489, with that row's name).
    Asks(String),
}

pub struct ShortcutWindow {
    pub frame: Frame,
    pub tab: usize,
    page: usize,
    /// Every change waits here for OK.
    pub draft: Shortcuts,
    /// The picked row, and the notice over it: 1486, or a refusal in yellow.
    pub picked: Option<usize>,
    notice: (usize, bool),
    /// A chord another row has, waiting on the question: the row, the chord.
    asking: Option<(usize, ShortcutKey, Modifier)>,
}

impl ShortcutWindow {
    // ponytail: centred; the game's place comes from GetWindowInfo.
    pub fn new(shortcuts: &Shortcuts, screen: [f32; 2]) -> Self {
        let mut frame = Frame::new([((screen[0] - SIZE[0]) / 2.0).floor(), ((screen[1] - SIZE[1]) / 2.0).floor()], SIZE[0]);
        frame.open = true;
        Self { frame, tab: 0, page: 0, draft: shortcuts.clone(), picked: None, notice: (ASSIGN, false), asking: None }
    }

    /// The open tab's rows, by place in the shortcut list.
    fn rows(&self) -> Vec<usize> {
        let table = TABS[self.tab].0;
        self.draft.bindings().iter().enumerate().filter(|(_, binding)| binding.table == table).map(|(pos, _)| pos).collect()
    }

    fn pages(&self) -> usize {
        self.rows().len().div_ceil(PER_PAGE).max(1)
    }

    /// The cell of the `place`th row on the page: its corner and width.
    fn cell(place: usize) -> ([f32; 2], f32) {
        let (column, row) = (place / PER_COLUMN, place % PER_COLUMN);
        let half = SIZE[0] / 2.0;
        let left = if column == 0 { 3.0 } else { half };
        ([left, ROWS_TOP + row as f32 * ROW], half - 3.0)
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let local = self.frame.local(cursor, SIZE[1])?;
        if contains(self.frame.close_button(), window::BUTTON_SIZE, local) {
            return Some(Hit::Close);
        }
        if local[1] < TITLE_HEIGHT {
            return Some(Hit::Title);
        }
        for (tab, _) in TABS.iter().enumerate() {
            if contains([TABS_AT[0] + tab as f32 * TAB[0], TABS_AT[1]], TAB, local) {
                return Some(Hit::Tab(tab));
            }
        }
        if self.pages() > 1 {
            if contains(PREV, [13.0, 13.0], local) {
                return Some(Hit::Prev);
            }
            if contains(NEXT, [13.0, 13.0], local) {
                return Some(Hit::Next);
            }
        }
        for (x, hit) in BUTTONS.into_iter().zip([Hit::Ok, Hit::Cancel, Hit::Reset, Hit::CloseButton]) {
            if contains([x, BUTTON_TOP], BUTTON, local) {
                return Some(hit);
            }
        }
        if contains(BATTLE_AT, [150.0, 12.0], local) {
            return Some(Hit::Battle);
        }
        let rows = self.rows();
        for (place, &pos) in rows.iter().skip(self.page * PER_PAGE).take(PER_PAGE).enumerate() {
            let (corner, width) = Self::cell(place);
            if contains(corner, [width, ROW], local) {
                return Some(Hit::Row(pos));
            }
        }
        Some(Hit::Window)
    }

    pub fn press(&mut self, hit: Hit, cursor: [f32; 2]) {
        if hit == Hit::Title {
            self.frame.start_drag(cursor);
        }
    }

    /// A click; Cancel drops the draft's changes (back to `shortcuts`) and stays open.
    pub fn click(&mut self, hit: Hit, shortcuts: &Shortcuts) -> Option<Click> {
        match hit {
            Hit::Close | Hit::CloseButton => {
                self.frame.open = false;
                return Some(Click::Close);
            }
            Hit::Tab(tab) => {
                (self.tab, self.page, self.picked) = (tab, 0, None);
            }
            Hit::Row(pos) => {
                self.picked = Some(pos);
                self.notice = (ASSIGN, false);
            }
            Hit::Prev => self.page = self.page.saturating_sub(1),
            Hit::Next => self.page = (self.page + 1).min(self.pages() - 1),
            Hit::Ok => return Some(Click::Save),
            Hit::Cancel => {
                self.draft = shortcuts.clone();
                self.picked = None;
            }
            Hit::Reset => return Some(Click::Reset),
            Hit::Battle => return Some(Click::BattleMode),
            Hit::Title | Hit::Window => {}
        }
        None
    }

    /// A key while a row is picked: Esc unbinds it, a modifier alone (`chord` None) or Tab with Alt or Shift is
    /// refused, a chord another row has asks first, else the row takes it.
    pub fn key(&mut self, escape: bool, chord: Option<(ShortcutKey, Modifier)>, messages: &[String]) -> Option<Pressed> {
        let pos = self.picked?;
        if escape {
            self.draft.bind(pos, None, Modifier::None);
            self.picked = None;
            return Some(Pressed::Bound);
        }
        let Some((key, modifier)) = chord else {
            self.notice = (SINGLE_KEY, true);
            return Some(Pressed::Refused);
        };
        if key == ShortcutKey::Named(winit::keyboard::NamedKey::Tab) && matches!(modifier, Modifier::Alt | Modifier::Shift) {
            self.notice = (TAKEN_KEY, true);
            return Some(Pressed::Refused);
        }
        if let Some(other) = self.draft.holder(pos, key, modifier) {
            self.asking = Some((pos, key, modifier));
            return Some(Pressed::Asks(row_name(&self.draft.bindings()[other], messages)));
        }
        self.draft.bind(pos, Some(key), modifier);
        self.picked = None;
        Some(Pressed::Bound)
    }

    /// The question's answer: yes takes the chord from the other row.
    pub fn confirm(&mut self, yes: bool) {
        if let Some((pos, key, modifier)) = self.asking.take().filter(|_| yes) {
            self.draft.bind(pos, Some(key), modifier);
            self.picked = None;
        }
    }

    pub fn drag_to(&mut self, cursor: [f32; 2]) -> bool {
        self.frame.drag_to(cursor, SIZE[1])
    }

    pub fn drop_drag(&mut self) -> bool {
        self.frame.drop_drag()
    }

    /// The tabs, rows (by action name) and buttons on screen (for the test driver).
    pub fn button_boxes(&self) -> Vec<(String, [f32; 4])> {
        let place = |[x, y]: [f32; 2], [width, height]: [f32; 2]| {
            let [x, y] = self.frame.at([x, y]);
            [x, y, width, height]
        };
        let mut boxes: Vec<_> =
            (0..TABS.len()).map(|tab| (format!("shortcut_tab:{tab}"), place([TABS_AT[0] + tab as f32 * TAB[0], TABS_AT[1]], TAB))).collect();
        let rows = self.rows();
        boxes.extend(rows.iter().skip(self.page * PER_PAGE).take(PER_PAGE).enumerate().map(|(place_on_page, &pos)| {
            let (corner, width) = Self::cell(place_on_page);
            (format!("shortcut_row:{}", target_name(&self.draft.bindings()[pos])), place(corner, [width, ROW]))
        }));
        let names = ["shortcut_ok", "shortcut_cancel", "shortcut_reset", "shortcut_close"];
        boxes.extend(names.into_iter().zip(BUTTONS).map(|(name, x)| (name.to_owned(), place([x, BUTTON_TOP], BUTTON))));
        boxes.push(("battle_mode".to_owned(), place(BATTLE_AT, [150.0, 12.0])));
        boxes
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &self,
        layer: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        check: &'a [Texture; 2],
        font: &Font,
        messages: &[String],
        battle_on: bool,
        pointer: ([f32; 2], bool),
    ) {
        let at = |local: [f32; 2]| self.frame.at(local);
        let worded = |(id, words): (usize, &'static str)| {
            Some(messages.get(id).map_or("", String::as_str)).filter(|text| !text.trim().is_empty()).unwrap_or(words).to_owned()
        };
        let black = [0, 0, 0];
        let [width, height] = SIZE;
        self.frame.draw_bar(layer, chrome);
        self.frame.draw_bare_title(layer, chrome, &worded((TITLE, "Shortcut Settings")), pointer.0);
        layer.sprites.push((&chrome.white, at([0.0, TITLE_HEIGHT]), [width, height - TITLE_HEIGHT]));
        // The tabs.
        for (tab, &(_, label, words)) in TABS.iter().enumerate() {
            let corner = [TABS_AT[0] + tab as f32 * TAB[0], TABS_AT[1]];
            let fill = if tab == self.tab { &assets.fills[tab] } else { &chrome.white };
            layer.sprites.push((&assets.line, at(corner), TAB));
            layer.sprites.push((fill, at([corner[0] + 1.0, corner[1] + 1.0]), [TAB[0] - 2.0, TAB[1] - 1.0]));
            let text = window::fit(font, &worded((label, words)), TAB[0] - 6.0);
            layer.texts.push((text.clone(), at([corner[0] + ((TAB[0] - font.width(&text)) / 2.0).floor(), corner[1] + 3.0]), black));
        }
        // The list's outline and divider.
        let bottom = BUTTON_TOP - 6.0;
        layer.sprites.push((&assets.line, at([3.0, ROWS_TOP - 1.0]), [width - 6.0, 1.0]));
        layer.sprites.push((&assets.line, at([3.0, bottom]), [width - 6.0, 1.0]));
        layer.sprites.push((&assets.line, at([3.0, ROWS_TOP - 1.0]), [1.0, bottom - ROWS_TOP + 1.0]));
        layer.sprites.push((&assets.line, at([width - 4.0, ROWS_TOP - 1.0]), [1.0, bottom - ROWS_TOP + 1.0]));
        layer.sprites.push((&assets.line, at([width / 2.0, ROWS_TOP - 1.0]), [1.0, bottom - ROWS_TOP + 1.0]));
        // The rows: the name centred left of the key box, the key in it, a line under.
        let rows = self.rows();
        let mut notice_at = None;
        for (place, &pos) in rows.iter().skip(self.page * PER_PAGE).take(PER_PAGE).enumerate() {
            let binding = &self.draft.bindings()[pos];
            let ([x, y], cell_width) = Self::cell(place);
            let label_width = cell_width - KEY_BOX[0];
            let box_x = x + cell_width - KEY_BOX[0];
            if self.picked == Some(pos) {
                layer.sprites.push((&assets.picked, at([x, y]), [cell_width, ROW]));
                notice_at = Some([x, y - ROW]);
            } else {
                layer.sprites.push((&assets.fills[self.tab], at([box_x, y]), KEY_BOX));
            }
            layer.sprites.push((&assets.line, at([box_x, y]), [1.0, ROW]));
            layer.sprites.push((&assets.line, at([x, y + ROW - 1.0]), [cell_width, 1.0]));
            let name = window::fit(font, &row_name(binding, messages), label_width.min(130.0));
            layer.texts.push((name.clone(), at([x + ((label_width - font.width(&name)) / 2.0).floor(), y + 5.0]), black));
            let key = binding.key.map_or_else(|| worded(NOT_ASSIGNED), |key| shortcuts::chord(key, binding.modifier));
            let key = window::fit(font, &key, KEY_BOX[0] - 4.0);
            layer.texts.push((key.clone(), at([box_x + ((KEY_BOX[0] - font.width(&key)) / 2.0).floor(), y + 5.0]), black));
        }
        // Pages.
        if self.pages() > 1 {
            let [left, right] = &assets.arrows;
            layer.sprites.push((left, at(PREV), left.size));
            layer.sprites.push((right, at(NEXT), right.size));
            let (page, of) = (format!("{}", self.page + 1), format!(" / {}", self.pages()));
            let x = PREV[0] + 18.0;
            layer.texts.push((page.clone(), at([x, PREV[1]]), PAGE_COLOR));
            layer.texts.push((of, at([x + font.width(&page), PREV[1]]), PAGES_COLOR));
        }
        // The battle-mode box and the buttons.
        let box_image = &check[usize::from(battle_on)];
        layer.sprites.push((box_image, at(BATTLE_AT), box_image.size));
        layer.texts.push((worded(BATTLE).trim().to_owned(), at([BATTLE_AT[0] + 14.0, BATTLE_AT[1] - 1.0]), black));
        let hit = self.hit(pointer.0);
        for ((images, x), target) in assets.buttons.iter().zip(BUTTONS).zip([Hit::Ok, Hit::Cancel, Hit::Reset, Hit::CloseButton]) {
            let look = if hit == Some(target) { 1 + usize::from(pointer.1) } else { 0 };
            layer.sprites.push((&images[look], at([x, BUTTON_TOP]), BUTTON));
        }
        // The notice over the picked row: white, or yellow for a refusal.
        if let Some(corner) = notice_at {
            let (id, refused) = self.notice;
            let color = if refused { [0xFF, 0xFF, 0x00] } else { [0xFF; 3] };
            window::tooltip(layer, chrome, font, messages.get(id).map_or("", String::as_str), at(corner), color);
        }
    }
}

/// A row's name: a hotbar slot by its bar and number, a macro or flag by its number, else what the row opens.
// ponytail: the game names rows from its message table by the row's MSI id, which this data doesn't map.
pub fn row_name(binding: &shortcuts::Binding, messages: &[String]) -> String {
    match binding.action {
        Action::Hotbar { bar, slot } => {
            let label = TABS.iter().find(|(table, ..)| *table == binding.table).map(|&(_, id, words)| (id, words)).unwrap_or((0, ""));
            let label = Some(messages.get(label.0).map_or("", String::as_str)).filter(|text| !text.trim().is_empty()).unwrap_or(label.1);
            format!("{label} {bar}-{}", slot + 1)
        }
        Action::Macro(line) => format!("Macro {}", line + 1),
        Action::Flag(flag) => format!("Flag {}", flag + 1),
        Action::Unused => binding.exe.trim_start_matches("MSI_HK_").replace('_', " "),
        action => {
            let name = shortcuts::action_name(action).replace('_', " ");
            let mut letters = name.chars();
            letters.next().map(|first| first.to_uppercase().chain(letters).collect()).unwrap_or_default()
        }
    }
}

/// A row's name for the test driver: the action's name, or the EXE for one this client doesn't use.
fn target_name(binding: &shortcuts::Binding) -> String {
    match binding.action {
        Action::Unused => binding.exe.clone(),
        action => shortcuts::action_name(action),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window() -> ShortcutWindow {
        ShortcutWindow::new(&Shortcuts::builtin(), [1024.0, 768.0])
    }

    #[test]
    fn lists_a_tab_in_two_columns_and_binds_a_picked_row() {
        let mut window = window();
        let corner = window.frame.position;
        let at = |local: [f32; 2]| [corner[0] + local[0], corner[1] + local[1]];
        assert_eq!(window.hit(at([3.0 + 70.0 * 2.0 + 5.0, 25.0])), Some(Hit::Tab(2)));
        let shortcuts = Shortcuts::builtin();
        window.click(Hit::Tab(2), &shortcuts);
        let rows = window.rows();
        assert!(rows.iter().all(|&pos| window.draft.bindings()[pos].table == 2));
        // The 19th row is the second column's first.
        if rows.len() > PER_COLUMN {
            assert_eq!(window.hit(at([SIZE[0] / 2.0 + 10.0, ROWS_TOP + 5.0])), Some(Hit::Row(rows[PER_COLUMN])));
        }
        let inventory = rows.iter().copied().find(|&pos| window.draft.bindings()[pos].action == Action::Inventory).unwrap();
        window.click(Hit::Row(inventory), &shortcuts);
        // A modifier alone is refused; Tab with Alt is refused.
        assert_eq!(window.key(false, None, &[]), Some(Pressed::Refused));
        assert_eq!(window.key(false, Some((ShortcutKey::Named(winit::keyboard::NamedKey::Tab), Modifier::Alt)), &[]), Some(Pressed::Refused));
        // A free chord binds at once; one another row has asks, and yes moves it.
        let free = ('a'..='z').map(ShortcutKey::Char).find(|&key| window.draft.holder(inventory, key, Modifier::Ctrl).is_none()).unwrap();
        assert_eq!(window.key(false, Some((free, Modifier::Ctrl)), &[]), Some(Pressed::Bound));
        assert_eq!(window.draft.bindings()[inventory].key, Some(free));
        let equipment = window.draft.bindings().iter().position(|binding| binding.action == Action::Equipment).unwrap();
        let (taken, modifier) = (window.draft.bindings()[equipment].key.unwrap(), window.draft.bindings()[equipment].modifier);
        window.click(Hit::Row(inventory), &shortcuts);
        assert!(matches!(window.key(false, Some((taken, modifier)), &[]), Some(Pressed::Asks(name)) if name == "Equipment"));
        window.confirm(true);
        assert_eq!(window.draft.bindings()[inventory].key, Some(taken));
        assert_eq!(window.draft.bindings()[equipment].key, None);
        // Esc unbinds; Cancel drops the draft; OK, Reset, battle mode and Close are the app's.
        window.click(Hit::Row(inventory), &shortcuts);
        assert_eq!(window.key(true, None, &[]), Some(Pressed::Bound));
        assert_eq!(window.draft.bindings()[inventory].key, None);
        assert_eq!(window.click(Hit::Cancel, &shortcuts), None);
        assert_eq!(window.draft, shortcuts);
        assert_eq!(window.click(Hit::Ok, &shortcuts), Some(Click::Save));
        assert_eq!(window.click(Hit::Battle, &shortcuts), Some(Click::BattleMode));
        assert_eq!(window.click(Hit::Reset, &shortcuts), Some(Click::Reset));
        assert_eq!(window.click(Hit::CloseButton, &shortcuts), Some(Click::Close));
    }

    #[test]
    fn pages_a_long_tab() {
        let mut window = window();
        let shortcuts = Shortcuts::builtin();
        let tab = (0..TABS.len()).max_by_key(|&tab| {
            window.tab = tab;
            window.rows().len()
        });
        window.click(Hit::Tab(tab.unwrap()), &shortcuts);
        let pages = window.pages();
        window.click(Hit::Next, &shortcuts);
        assert_eq!(window.page, 1.min(pages - 1));
        window.click(Hit::Next, &shortcuts);
        window.click(Hit::Next, &shortcuts);
        assert!(window.page < pages);
        window.click(Hit::Prev, &shortcuts);
        window.click(Hit::Prev, &shortcuts);
        window.click(Hit::Prev, &shortcuts);
        assert_eq!(window.page, 0);
    }
}
