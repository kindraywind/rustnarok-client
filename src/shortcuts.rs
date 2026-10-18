//! Shortcuts between keys and actions. Defaults come from `hotkey_v2.lub`; `UserKeys.lua` replaces a row.
//!
//! `action` is the whole check: one key event in, the action to fire or none.

use std::io;
use std::path::Path;

use rustnarok_client::grf::Grf;
use rustnarok_client::lua::{self, HotkeyRow};
use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};

/// The one modifier that counts. Alt is checked first, then Shift, then Ctrl.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Modifier {
    None,
    Alt,
    Shift,
    Ctrl,
    /// A KEY2 this client doesn't treat as a modifier, so the row never matches a press.
    Other,
}

/// A key a shortcut can be bound to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShortcutKey {
    Named(NamedKey),
    Char(char),
}

/// What a shortcut does. `Unused` is bound in the data and has no window here yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    BasicInfo,
    Equipment,
    Skills,
    Quests,
    Friends,
    Party,
    ChatRoom,
    Cart,
    WorldMap,
    Inventory,
    Guild,
    Emotions,
    Sit,
    Minimap,
    Macros,
    Commands,
    CloseWindows,
    HotkeyBar,
    Chat,
    ChatSize,
    HpSp,
    Screenshot,
    Memorial,
    Pet,
    Homunculus,
    Mercenary,
    HomunculusFollow,
    MercenaryFollow,
    Menu,
    GroundCursor,
    Clan,
    Bank,
    Status,
    RotateHotkeys,
    /// Hotbar 1 and 2 trade places.
    SwitchHotkeys,
    Tip,
    Costume,
    /// The reputation window.
    Reputation,
    PartyBoard,
    Hunting,
    Macro(u8),
    Hotbar {
        bar: u8,
        slot: u8,
    },
    Flag(u8),
    /// Enter toggles chat mode in the data. The chat line still takes Enter.
    ChatMode,
    Unused,
}

/// One bound (or unbound) shortcut.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binding {
    pub table: u8,
    pub index: u16,
    pub exe: String,
    pub action: Action,
    pub key: Option<ShortcutKey>,
    pub modifier: Modifier,
}

/// The shortcut list, table order, defaults then the player's rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shortcuts {
    bindings: Vec<Binding>,
}

/// A key press, as `action` sees it.
pub struct KeyEvent<'a> {
    pub key: &'a Key,
    pub physical: Option<PhysicalKey>,
    pub alt: bool,
    pub shift: bool,
    pub ctrl: bool,
    pub repeat: bool,
    /// A text box has the caret.
    pub text_focus: bool,
    /// The hotbar showing (1 or 2): only its slot keys fire.
    pub bar: u8,
}

/// The action this key press should fire, or none.
///
/// An auto-repeated key-down (`repeat`) never fires. While a text box has focus, only Tab, Enter,
/// Shift, Ctrl, Alt, Caps Lock, Insert, F1–F12, any Alt combination, and Ctrl combinations other
/// than Ctrl+A/C/V/X are considered. One modifier counts: Alt, then Shift, then Ctrl.
pub fn action(shortcuts: &Shortcuts, event: &KeyEvent<'_>) -> Option<Action> {
    if event.repeat || !reaches(event) {
        return None;
    }
    let modifier = counted_modifier(event.alt, event.shift, event.ctrl);
    let keys = event_keys(event);
    shortcuts
        .bindings
        .iter()
        .filter(|binding| !matches!(binding.action, Action::Hotbar { bar, .. } if bar != event.bar))
        .find(|binding| binding.modifier == modifier && binding.key.is_some_and(|key| keys.contains(&key)))
        .map(|binding| binding.action)
}

impl Shortcuts {
    /// Defaults from the grf, or the built-in copy of those defaults when the file isn't there or its tables aren't
    /// the ones this client knows (older data lays them out otherwise: no window opens the inventory),
    /// then `UserKeys.lua` from the save folder (`RUSTNAROK_CLIENT_USERKEYS` names another file).
    pub fn load(grf: &Grf) -> Self {
        let mut shortcuts = lua::hotkey_defaults(grf)
            .ok()
            .filter(|rows| !rows.is_empty())
            .map(|rows| Self::from_rows(&rows))
            .filter(Self::knows_the_tables)
            .unwrap_or_else(Self::builtin);
        if let Some(path) = userkeys_path() {
            let _ = shortcuts.apply_user_file(&path);
        }
        shortcuts
    }

    /// Whether the data's tables are laid out as this client reads them: something opens the inventory.
    fn knows_the_tables(&self) -> bool {
        self.bindings.iter().any(|binding| binding.action == Action::Inventory && binding.key.is_some())
    }

    /// The game's usual defaults, used when hotkey_v2.lub can't be read.
    pub fn builtin() -> Self {
        Self::from_lua(DEFAULTS).unwrap_or_else(|_| Self { bindings: Vec::new() })
    }

    pub fn from_lua(source: &str) -> io::Result<Self> {
        Ok(Self::from_rows(&lua::hotkey_rows(source)?))
    }

    fn from_rows(rows: &[HotkeyRow]) -> Self {
        let mut bindings: Vec<_> = rows.iter().filter(|row| !row.user).map(Binding::from_row).collect();
        bindings.sort_by_key(|binding| (binding.table, binding.index));
        let mut shortcuts = Self { bindings };
        let user: Vec<_> = rows.iter().filter(|row| row.user).cloned().collect();
        shortcuts.apply_user(&user);
        shortcuts
    }

    /// `USERKEY_<table>[index]` replaces that default. No KEY1 leaves the row unbound.
    pub fn apply_user(&mut self, rows: &[HotkeyRow]) {
        for row in rows {
            let key = row.key1.and_then(vk_to_key);
            let modifier = modifier_of(row.key2);
            let pos = self.bindings.iter().position(|binding| binding.table == row.table && binding.index == row.index);
            let pos = if let Some(pos) = pos {
                self.bindings[pos].key = key;
                self.bindings[pos].modifier = modifier;
                pos
            } else if key.is_some() {
                self.bindings.push(Binding::from_row(row));
                self.bindings.sort_by_key(|binding| (binding.table, binding.index));
                self.bindings.iter().position(|binding| binding.table == row.table && binding.index == row.index).unwrap_or(0)
            } else {
                continue;
            };
            // The player's chord wins: the same key on another row is cleared.
            if let Some(key) = key {
                self.clear_chord(pos, key, modifier);
            }
        }
    }

    /// Reads a UserKeys.lua file and applies it.
    pub fn apply_user_file(&mut self, path: &Path) -> io::Result<()> {
        let rows = lua::hotkey_rows(&std::fs::read_to_string(path)?)?;
        let user: Vec<_> = rows.into_iter().filter(|row| row.user).collect();
        self.apply_user(&user);
        Ok(())
    }

    /// One hotbar-1 slot, as the shortcut-settings window rebinds it.
    pub fn set_hotbar(&mut self, slot: usize, key: Option<ShortcutKey>) {
        let Some(pos) = self.bindings.iter().position(|binding| matches!(binding.action, Action::Hotbar { bar: 1, slot: bound } if usize::from(bound) == slot))
        else {
            return;
        };
        self.bindings[pos].key = key;
        self.bindings[pos].modifier = Modifier::None;
        if let Some(key) = key {
            self.clear_chord(pos, key, Modifier::None);
        }
    }

    /// Hotbar 1's keys from the shortcut-settings window, which writes its own file until it writes UserKeys.
    pub fn apply_bar_keys(&mut self, keys: [Option<ShortcutKey>; 36]) {
        for (slot, key) in keys.into_iter().enumerate() {
            let Some(pos) = self.bindings.iter().position(|binding| matches!(binding.action, Action::Hotbar { bar: 1, slot: bound } if bound as usize == slot))
            else {
                continue;
            };
            self.bindings[pos].key = key;
            self.bindings[pos].modifier = Modifier::None;
            if let Some(key) = key {
                self.clear_chord(pos, key, Modifier::None);
            }
        }
    }

    fn clear_chord(&mut self, skip: usize, key: ShortcutKey, modifier: Modifier) {
        // The two hotbars share keys: only the one showing fires.
        let bar_of = |action: Action| match action {
            Action::Hotbar { bar, .. } => Some(bar),
            _ => None,
        };
        let kept = bar_of(self.bindings[skip].action);
        for (index, binding) in self.bindings.iter_mut().enumerate() {
            let other_bar = kept.is_some_and(|kept| bar_of(binding.action).is_some_and(|bar| bar != kept));
            if index != skip && !other_bar && binding.key == Some(key) && binding.modifier == modifier {
                binding.key = None;
            }
        }
    }

    pub fn bindings(&self) -> &[Binding] {
        &self.bindings
    }

    /// Action name to the chord, for the control socket. Unbound rows are left out.
    pub fn labels(&self) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        for binding in self.bindings() {
            let Some(key) = binding.key else { continue };
            let name = match binding.action {
                Action::Unused => binding.exe.clone(),
                other => action_name(other),
            };
            map.entry(name).or_insert_with(|| chord(key, binding.modifier).into());
        }
        serde_json::Value::Object(map)
    }
}

impl Binding {
    fn from_row(row: &HotkeyRow) -> Self {
        Self {
            table: row.table,
            index: row.index,
            exe: row.exe.clone(),
            action: exe_action(&row.exe, row.table, row.index),
            key: row.key1.and_then(vk_to_key),
            modifier: modifier_of(row.key2),
        }
    }
}

fn userkeys_path() -> Option<std::path::PathBuf> {
    std::env::var_os("RUSTNAROK_CLIENT_USERKEYS").map(std::path::PathBuf::from).or_else(|| crate::settings_path("UserKeys.lua"))
}

/// F1 is 1. None past F12.
pub fn function_key(number: u8) -> Option<ShortcutKey> {
    const KEYS: [NamedKey; 12] = [
        NamedKey::F1,
        NamedKey::F2,
        NamedKey::F3,
        NamedKey::F4,
        NamedKey::F5,
        NamedKey::F6,
        NamedKey::F7,
        NamedKey::F8,
        NamedKey::F9,
        NamedKey::F10,
        NamedKey::F11,
        NamedKey::F12,
    ];
    KEYS.get(usize::from(number.checked_sub(1)?)).copied().map(ShortcutKey::Named)
}

/// Windows virtual-key code to a key. KEYNAME in the hotkey file names these (112 is F1, 18 is Alt).
pub fn vk_to_key(vk: u16) -> Option<ShortcutKey> {
    const FUNCTIONS: [NamedKey; 12] = [
        NamedKey::F1,
        NamedKey::F2,
        NamedKey::F3,
        NamedKey::F4,
        NamedKey::F5,
        NamedKey::F6,
        NamedKey::F7,
        NamedKey::F8,
        NamedKey::F9,
        NamedKey::F10,
        NamedKey::F11,
        NamedKey::F12,
    ];
    Some(match vk {
        8 => ShortcutKey::Named(NamedKey::Backspace),
        9 => ShortcutKey::Named(NamedKey::Tab),
        13 => ShortcutKey::Named(NamedKey::Enter),
        19 => ShortcutKey::Named(NamedKey::Pause),
        20 => ShortcutKey::Named(NamedKey::CapsLock),
        27 => ShortcutKey::Named(NamedKey::Escape),
        32 => ShortcutKey::Named(NamedKey::Space),
        33 => ShortcutKey::Named(NamedKey::PageUp),
        34 => ShortcutKey::Named(NamedKey::PageDown),
        35 => ShortcutKey::Named(NamedKey::End),
        36 => ShortcutKey::Named(NamedKey::Home),
        37 => ShortcutKey::Named(NamedKey::ArrowLeft),
        38 => ShortcutKey::Named(NamedKey::ArrowUp),
        39 => ShortcutKey::Named(NamedKey::ArrowRight),
        40 => ShortcutKey::Named(NamedKey::ArrowDown),
        44 => ShortcutKey::Named(NamedKey::PrintScreen),
        45 => ShortcutKey::Named(NamedKey::Insert),
        46 => ShortcutKey::Named(NamedKey::Delete),
        48..=57 => ShortcutKey::Char((b'0' + (vk as u8 - 48)) as char),
        65..=90 => ShortcutKey::Char((b'a' + (vk as u8 - 65)) as char),
        112..=123 => ShortcutKey::Named(FUNCTIONS[(vk - 112) as usize]),
        186 => ShortcutKey::Char(';'),
        187 => ShortcutKey::Char('='),
        188 => ShortcutKey::Char(','),
        189 => ShortcutKey::Char('-'),
        190 => ShortcutKey::Char('.'),
        191 => ShortcutKey::Char('/'),
        192 => ShortcutKey::Char('`'),
        219 => ShortcutKey::Char('['),
        220 => ShortcutKey::Char('\\'),
        221 => ShortcutKey::Char(']'),
        222 => ShortcutKey::Char('\''),
        // ponytail: the numpad and the mouse buttons aren't mapped; the defaults use the top row and F-keys.
        _ => return None,
    })
}

fn modifier_of(vk: u16) -> Modifier {
    match vk {
        0 => Modifier::None,
        16 => Modifier::Shift,
        17 => Modifier::Ctrl,
        18 => Modifier::Alt,
        _ => Modifier::Other,
    }
}

fn counted_modifier(alt: bool, shift: bool, ctrl: bool) -> Modifier {
    if alt {
        Modifier::Alt
    } else if shift {
        Modifier::Shift
    } else if ctrl {
        Modifier::Ctrl
    } else {
        Modifier::None
    }
}

/// With a text box focused, the keys that still reach shortcuts.
fn reaches(event: &KeyEvent<'_>) -> bool {
    if !event.text_focus {
        return true;
    }
    if event.alt {
        return true;
    }
    if event.ctrl {
        return !event_keys(event).iter().any(|key| matches!(key, ShortcutKey::Char('a' | 'c' | 'v' | 'x')));
    }
    // ponytail: Print Screen is let through while a text box has the caret, or the snapshot key never fires.
    matches!(
        event.key,
        Key::Named(
            NamedKey::Tab
                | NamedKey::Enter
                | NamedKey::Shift
                | NamedKey::Control
                | NamedKey::Alt
                | NamedKey::CapsLock
                | NamedKey::Insert
                | NamedKey::PrintScreen
                | NamedKey::F1
                | NamedKey::F2
                | NamedKey::F3
                | NamedKey::F4
                | NamedKey::F5
                | NamedKey::F6
                | NamedKey::F7
                | NamedKey::F8
                | NamedKey::F9
                | NamedKey::F10
                | NamedKey::F11
                | NamedKey::F12
        )
    )
}

fn event_keys(event: &KeyEvent<'_>) -> Vec<ShortcutKey> {
    let mut keys = Vec::new();
    match event.key {
        Key::Named(named) => keys.push(ShortcutKey::Named(*named)),
        Key::Character(text) => {
            let mut characters = text.chars();
            if let (Some(character), None) = (characters.next(), characters.next()) {
                // Shift types `~` for the backtick key.
                let character = if character == '~' { '`' } else { character.to_ascii_lowercase() };
                keys.push(ShortcutKey::Char(character));
            }
        }
        _ => {}
    }
    if let Some(PhysicalKey::Code(code)) = event.physical
        && let Some(key) = code_key(code)
        && !keys.contains(&key)
    {
        keys.push(key);
    }
    keys
}

fn code_key(code: KeyCode) -> Option<ShortcutKey> {
    const LETTERS: [KeyCode; 26] = [
        KeyCode::KeyA,
        KeyCode::KeyB,
        KeyCode::KeyC,
        KeyCode::KeyD,
        KeyCode::KeyE,
        KeyCode::KeyF,
        KeyCode::KeyG,
        KeyCode::KeyH,
        KeyCode::KeyI,
        KeyCode::KeyJ,
        KeyCode::KeyK,
        KeyCode::KeyL,
        KeyCode::KeyM,
        KeyCode::KeyN,
        KeyCode::KeyO,
        KeyCode::KeyP,
        KeyCode::KeyQ,
        KeyCode::KeyR,
        KeyCode::KeyS,
        KeyCode::KeyT,
        KeyCode::KeyU,
        KeyCode::KeyV,
        KeyCode::KeyW,
        KeyCode::KeyX,
        KeyCode::KeyY,
        KeyCode::KeyZ,
    ];
    const DIGITS: [KeyCode; 10] = [
        KeyCode::Digit0,
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
        KeyCode::Digit8,
        KeyCode::Digit9,
    ];
    const FUNCTIONS: [(KeyCode, NamedKey); 12] = [
        (KeyCode::F1, NamedKey::F1),
        (KeyCode::F2, NamedKey::F2),
        (KeyCode::F3, NamedKey::F3),
        (KeyCode::F4, NamedKey::F4),
        (KeyCode::F5, NamedKey::F5),
        (KeyCode::F6, NamedKey::F6),
        (KeyCode::F7, NamedKey::F7),
        (KeyCode::F8, NamedKey::F8),
        (KeyCode::F9, NamedKey::F9),
        (KeyCode::F10, NamedKey::F10),
        (KeyCode::F11, NamedKey::F11),
        (KeyCode::F12, NamedKey::F12),
    ];
    if let Some(index) = LETTERS.iter().position(|letter| *letter == code) {
        return Some(ShortcutKey::Char((b'a' + index as u8) as char));
    }
    if let Some(index) = DIGITS.iter().position(|digit| *digit == code) {
        return Some(ShortcutKey::Char((b'0' + index as u8) as char));
    }
    if let Some((_, named)) = FUNCTIONS.iter().find(|(function, _)| *function == code) {
        return Some(ShortcutKey::Named(*named));
    }
    match code {
        KeyCode::Backquote => Some(ShortcutKey::Char('`')),
        KeyCode::Tab => Some(ShortcutKey::Named(NamedKey::Tab)),
        KeyCode::Insert => Some(ShortcutKey::Named(NamedKey::Insert)),
        KeyCode::End => Some(ShortcutKey::Named(NamedKey::End)),
        KeyCode::Home => Some(ShortcutKey::Named(NamedKey::Home)),
        KeyCode::PrintScreen => Some(ShortcutKey::Named(NamedKey::PrintScreen)),
        KeyCode::Enter => Some(ShortcutKey::Named(NamedKey::Enter)),
        _ => None,
    }
}

fn exe_action(exe: &str, table: u8, index: u16) -> Action {
    if table == 1 {
        return Action::Hotbar { bar: 1, slot: u8::try_from(index).unwrap_or(u8::MAX) };
    }
    if table == 4 {
        return Action::Hotbar { bar: 2, slot: u8::try_from(index).unwrap_or(u8::MAX) };
    }
    if table == 3 {
        return if index < 10 { Action::Macro(index as u8) } else { Action::Flag(index.saturating_sub(10) as u8) };
    }
    // ponytail: a name the list doesn't have is matched by the words in it; anything left over stays unused.
    let name = exe.trim_start_matches("MSI_HK_");
    if name.contains("OPTION") || name.contains("REPLAY") || name.contains("NAVIGATION") || name.contains("CASHSHOP") || name.contains("BATTLEFIELD") {
        return Action::Unused;
    }
    if name.contains("BTN") && name.contains("BASIC") {
        return Action::Menu;
    }
    if name.contains("CHATMODE") {
        return Action::ChatMode;
    }
    if name.contains("CHAT") && name.contains("MAKE") {
        return Action::ChatRoom;
    }
    if name.contains("CHAT") && (name.contains("ZOOM") || name.contains("SIZE")) {
        return Action::ChatSize;
    }
    if name.contains("CHAT") {
        return Action::Chat;
    }
    if name.contains("ROTATE") {
        return Action::RotateHotkeys;
    }
    if name.contains("CHANGE_SKILLBAR") {
        return Action::SwitchHotkeys;
    }
    // ponytail: no equipment set swap yet; it mustn't fall to the equipment window.
    if name.contains("SWAP_EQUIP") {
        return Action::Unused;
    }
    if name.contains("COSEQUIP") {
        return Action::Costume;
    }
    if name.contains("SKILLBAR") || (name.contains("HOTKEY") && name.contains("ONOFF")) {
        return Action::HotkeyBar;
    }
    if name.contains("ALLWND") || name.contains("ALL_WND") {
        return Action::CloseWindows;
    }
    if name.contains("SNAPSHOT") || name.contains("SCREENSHOT") {
        return Action::Screenshot;
    }
    if name.contains("GAGE") || name.contains("HPSP") {
        return Action::HpSp;
    }
    if name.contains("MINIMAP") {
        return Action::Minimap;
    }
    if name.contains("SIT") {
        return Action::Sit;
    }
    if name.contains("GROUND") || name.contains("CURSOR") {
        return Action::GroundCursor;
    }
    if name.contains("HUNT") {
        return Action::Hunting;
    }
    if name.contains("MEMORIAL") {
        return Action::Memorial;
    }
    if name.contains("TIP") {
        return Action::Tip;
    }
    if name.contains("COSTUME") {
        return Action::Costume;
    }
    if name.contains("BANK") {
        return Action::Bank;
    }
    if name.contains("CLAN") {
        return Action::Clan;
    }
    if name.contains("STATUS") {
        return Action::Status;
    }
    if name.contains("FOLLOW") && name.contains("HOMUN") {
        return Action::HomunculusFollow;
    }
    if name.contains("FOLLOW") && name.contains("MER") {
        return Action::MercenaryFollow;
    }
    if name.contains("HOMUN") {
        return Action::Homunculus;
    }
    if name.contains("MER") {
        return Action::Mercenary;
    }
    if name.contains("PET") {
        return Action::Pet;
    }
    if name.contains("EMOTION") {
        return Action::Emotions;
    }
    if name.contains("MACRO") {
        return Action::Macros;
    }
    if name.contains("EXE") || name.contains("COMMAND") {
        return Action::Commands;
    }
    if name.contains("FRIEND") {
        return Action::Friends;
    }
    if name.contains("BOOKING") || name.contains("PARTYBOARD") || name.contains("PARTY_BOARD") {
        return Action::PartyBoard;
    }
    if name.contains("PARTY") {
        return Action::Party;
    }
    if name.contains("GUILD") {
        return Action::Guild;
    }
    if name.contains("REPUT") {
        return Action::Reputation;
    }
    if name.contains("QUEST") {
        return Action::Quests;
    }
    if name.contains("SKILL") {
        return Action::Skills;
    }
    if name.contains("EQUIP") {
        return Action::Equipment;
    }
    if name.contains("CART") {
        return Action::Cart;
    }
    if name.contains("MAP") {
        return Action::WorldMap;
    }
    if name.contains("ITEM") {
        return Action::Inventory;
    }
    if name.contains("BASIC") {
        return Action::BasicInfo;
    }
    Action::Unused
}

pub fn action_name(action: Action) -> String {
    match action {
        Action::BasicInfo => "basic_info",
        Action::Equipment => "equipment",
        Action::Skills => "skills",
        Action::Quests => "quests",
        Action::Friends => "friends",
        Action::Party => "party",
        Action::ChatRoom => "chat_room",
        Action::Cart => "cart",
        Action::WorldMap => "world_map",
        Action::Inventory => "inventory",
        Action::Guild => "guild",
        Action::Emotions => "emotions",
        Action::Sit => "sit",
        Action::Minimap => "minimap",
        Action::Macros => "macros",
        Action::Commands => "commands",
        Action::CloseWindows => "close_windows",
        Action::HotkeyBar => "hotkey_bar",
        Action::Chat => "chat",
        Action::ChatSize => "chat_size",
        Action::HpSp => "hp_sp",
        Action::Screenshot => "screenshot",
        Action::Memorial => "memorial",
        Action::Pet => "pet",
        Action::Homunculus => "homunculus",
        Action::Mercenary => "mercenary",
        Action::HomunculusFollow => "homunculus_follow",
        Action::MercenaryFollow => "mercenary_follow",
        Action::Menu => "menu",
        Action::GroundCursor => "ground_cursor",
        Action::Clan => "clan",
        Action::Bank => "bank",
        Action::Status => "status",
        Action::RotateHotkeys => "rotate_hotkeys",
        Action::SwitchHotkeys => "switch_hotkeys",
        Action::Tip => "tip",
        Action::Costume => "costume",
        Action::Reputation => "reputation",
        Action::PartyBoard => "party_board",
        Action::Hunting => "hunting",
        Action::Macro(line) => return format!("macro_{}", line + 1),
        Action::Hotbar { bar, slot } => return format!("hotbar_{bar}_{}", slot + 1),
        Action::Flag(flag) => return format!("flag_{}", flag + 1),
        Action::ChatMode => "chat_mode",
        Action::Unused => "unused",
    }
    .into()
}

fn chord(key: ShortcutKey, modifier: Modifier) -> String {
    let name = match key {
        ShortcutKey::Named(named) => match named {
            NamedKey::Tab => "Tab".into(),
            NamedKey::Enter => "Enter".into(),
            NamedKey::Insert => "Insert".into(),
            NamedKey::End => "End".into(),
            NamedKey::Home => "Home".into(),
            NamedKey::PrintScreen => "PrintScreen".into(),
            NamedKey::F1 => "F1".into(),
            NamedKey::F2 => "F2".into(),
            NamedKey::F3 => "F3".into(),
            NamedKey::F4 => "F4".into(),
            NamedKey::F5 => "F5".into(),
            NamedKey::F6 => "F6".into(),
            NamedKey::F7 => "F7".into(),
            NamedKey::F8 => "F8".into(),
            NamedKey::F9 => "F9".into(),
            NamedKey::F10 => "F10".into(),
            NamedKey::F11 => "F11".into(),
            NamedKey::F12 => "F12".into(),
            other => format!("{other:?}"),
        },
        ShortcutKey::Char(character) if character.is_ascii_alphabetic() => character.to_ascii_uppercase().to_string(),
        ShortcutKey::Char(character) => character.to_string(),
    };
    match modifier {
        Modifier::Alt => format!("Alt+{name}"),
        Modifier::Shift => format!("Shift+{name}"),
        Modifier::Ctrl => format!("Ctrl+{name}"),
        Modifier::None | Modifier::Other => name,
    }
}

/// The usual defaults, in the shape of hotkey_v2.lub, for a grf that doesn't have the file.
/// ponytail: the newer rows (clan, bank, status, rotate, tip, costume, party board, hunting) use the
/// MSI_HK_ names and the keys from the issue; a hotkey_v2.lub in the data replaces this whole list.
const DEFAULTS: &str = r#"
HOTKEY_1 = {
    [0] = { EXE = "MSI_HK_SKILLBAR1_1", KEY1 = 112 },
    [1] = { EXE = "MSI_HK_SKILLBAR1_2", KEY1 = 113 },
    [2] = { EXE = "MSI_HK_SKILLBAR1_3", KEY1 = 114 },
    [3] = { EXE = "MSI_HK_SKILLBAR1_4", KEY1 = 115 },
    [4] = { EXE = "MSI_HK_SKILLBAR1_5", KEY1 = 116 },
    [5] = { EXE = "MSI_HK_SKILLBAR1_6", KEY1 = 117 },
    [6] = { EXE = "MSI_HK_SKILLBAR1_7", KEY1 = 118 },
    [7] = { EXE = "MSI_HK_SKILLBAR1_8", KEY1 = 119 },
    [8] = { EXE = "MSI_HK_SKILLBAR1_9", KEY1 = 120 },
    [9] = { EXE = "MSI_HK_SKILLBAR2_1" },
    [10] = { EXE = "MSI_HK_SKILLBAR2_2" },
    [11] = { EXE = "MSI_HK_SKILLBAR2_3" },
    [12] = { EXE = "MSI_HK_SKILLBAR2_4" },
    [13] = { EXE = "MSI_HK_SKILLBAR2_5" },
    [14] = { EXE = "MSI_HK_SKILLBAR2_6" },
    [15] = { EXE = "MSI_HK_SKILLBAR2_7" },
    [16] = { EXE = "MSI_HK_SKILLBAR2_8" },
    [17] = { EXE = "MSI_HK_SKILLBAR2_9" },
    [18] = { EXE = "MSI_HK_SKILLBAR3_1" },
    [19] = { EXE = "MSI_HK_SKILLBAR3_2" },
    [20] = { EXE = "MSI_HK_SKILLBAR3_3" },
    [21] = { EXE = "MSI_HK_SKILLBAR3_4" },
    [22] = { EXE = "MSI_HK_SKILLBAR3_5" },
    [23] = { EXE = "MSI_HK_SKILLBAR3_6" },
    [24] = { EXE = "MSI_HK_SKILLBAR3_7" },
    [25] = { EXE = "MSI_HK_SKILLBAR3_8" },
    [26] = { EXE = "MSI_HK_SKILLBAR3_9" },
    [27] = { EXE = "MSI_HK_SKILLBAR4_1" },
    [28] = { EXE = "MSI_HK_SKILLBAR4_2" },
    [29] = { EXE = "MSI_HK_SKILLBAR4_3" },
    [30] = { EXE = "MSI_HK_SKILLBAR4_4" },
    [31] = { EXE = "MSI_HK_SKILLBAR4_5" },
    [32] = { EXE = "MSI_HK_SKILLBAR4_6" },
    [33] = { EXE = "MSI_HK_SKILLBAR4_7" },
    [34] = { EXE = "MSI_HK_SKILLBAR4_8" },
    [35] = { EXE = "MSI_HK_SKILLBAR4_9" },
}
HOTKEY_2 = {
    [0] = { EXE = "MSI_HK_BASICINFOWND_MINI", KEY1 = 86, KEY2 = 18 },
    [1] = { EXE = "MSI_HK_EQUIPWND_ONOFF", KEY1 = 81, KEY2 = 18 },
    [2] = { EXE = "MSI_HK_SKILLWND_ONOFF", KEY1 = 83, KEY2 = 18 },
    [3] = { EXE = "MSI_HK_QUESTWND_ONOFF", KEY1 = 85, KEY2 = 18 },
    [4] = { EXE = "MSI_HK_FRIENDWND_ONOFF", KEY1 = 72, KEY2 = 18 },
    [5] = { EXE = "MSI_HK_PARTYWND_ONOFF", KEY1 = 90, KEY2 = 18 },
    [6] = { EXE = "MSI_HK_CHATWND_MAKE", KEY1 = 67, KEY2 = 18 },
    [7] = { EXE = "MSI_HK_CARTWND_ONOFF", KEY1 = 87, KEY2 = 18 },
    [8] = { EXE = "MSI_HK_MAPWND_ONOFF", KEY1 = 192, KEY2 = 17 },
    [9] = { EXE = "MSI_HK_ITEMWND_ONOFF", KEY1 = 69, KEY2 = 18 },
    [10] = { EXE = "MSI_HK_GUILDWND_ONOFF", KEY1 = 71, KEY2 = 18 },
    [11] = { EXE = "MSI_HK_EMOTIONWND_ONOFF", KEY1 = 76, KEY2 = 18 },
    [12] = { EXE = "MSI_HK_SITORSTAND", KEY1 = 45 },
    [13] = { EXE = "MSI_HK_MINIMAP_ONOFF", KEY1 = 9, KEY2 = 17 },
    [14] = { EXE = "MSI_HK_MACROWND_ONOFF", KEY1 = 77, KEY2 = 18 },
    [15] = { EXE = "MSI_HK_EXEWND_ONOFF", KEY1 = 89, KEY2 = 18 },
    [16] = { EXE = "MSI_HK_ALLWND_OFF", KEY1 = 122 },
    [17] = { EXE = "MSI_HK_SKILLBAR_ONOFF", KEY1 = 123 },
    [18] = { EXE = "MSI_HK_CHATWND_ONOFF", KEY1 = 121, KEY2 = 18 },
    [19] = { EXE = "MSI_HK_CHATWND_ZOOM", KEY1 = 121 },
    [20] = { EXE = "MSI_HK_GAGE_ONOFF", KEY1 = 35, KEY2 = 18 },
    [21] = { EXE = "MSI_HK_SNAPSHOT", KEY1 = 44 },
    [22] = { EXE = "MSI_HK_MEMORIALWND_ONOFF", KEY1 = 66, KEY2 = 18 },
    [23] = { EXE = "MSI_HK_PETWND_ONOFF", KEY1 = 74, KEY2 = 18 },
    [24] = { EXE = "MSI_HK_HOMUNWND_ONOFF", KEY1 = 82, KEY2 = 18 },
    [25] = { EXE = "MSI_HK_MERWND_ONOFF", KEY1 = 82, KEY2 = 17 },
    [26] = { EXE = "MSI_HK_HOMUN_FOLLOW", KEY1 = 84, KEY2 = 18 },
    [27] = { EXE = "MSI_HK_MER_FOLLOW", KEY1 = 84, KEY2 = 17 },
    [32] = { EXE = "MSI_HK_BASICINFOWND_BTN_ONOFF", KEY1 = 192, KEY2 = 18 },
    [34] = { EXE = "MSI_HK_PARTY_BOOKING_LIST", KEY1 = 90, KEY2 = 17 },
    [35] = { EXE = "MSI_HK_GROUND_CURSOR_ONOFF", KEY1 = 36, KEY2 = 18 },
    [36] = { EXE = "MSI_HK_CLANWND_ONOFF", KEY1 = 71, KEY2 = 17 },
    [37] = { EXE = "MSI_HK_BANKWND_ONOFF", KEY1 = 66, KEY2 = 17 },
    [38] = { EXE = "MSI_HK_STATUSWND_ONOFF", KEY1 = 65, KEY2 = 18 },
    [39] = { EXE = "MSI_HK_SKILLBAR_ROTATE", KEY1 = 123, KEY2 = 17 },
    [40] = { EXE = "MSI_HK_TIPWND_ONOFF", KEY1 = 68, KEY2 = 18 },
    [41] = { EXE = "MSI_HK_COSTUMEWND_ONOFF", KEY1 = 88, KEY2 = 18 },
    [42] = { EXE = "MSI_HK_HUNTINGWND_ONOFF", KEY1 = 72, KEY2 = 17 },
}
HOTKEY_3 = {
    [0] = { EXE = "MSI_HK_MACRO_1", KEY1 = 49, KEY2 = 18 },
    [1] = { EXE = "MSI_HK_MACRO_2", KEY1 = 50, KEY2 = 18 },
    [2] = { EXE = "MSI_HK_MACRO_3", KEY1 = 51, KEY2 = 18 },
    [3] = { EXE = "MSI_HK_MACRO_4", KEY1 = 52, KEY2 = 18 },
    [4] = { EXE = "MSI_HK_MACRO_5", KEY1 = 53, KEY2 = 18 },
    [5] = { EXE = "MSI_HK_MACRO_6", KEY1 = 54, KEY2 = 18 },
    [6] = { EXE = "MSI_HK_MACRO_7", KEY1 = 55, KEY2 = 18 },
    [7] = { EXE = "MSI_HK_MACRO_8", KEY1 = 56, KEY2 = 18 },
    [8] = { EXE = "MSI_HK_MACRO_9", KEY1 = 57, KEY2 = 18 },
    [9] = { EXE = "MSI_HK_MACRO_10", KEY1 = 48, KEY2 = 18 },
    [10] = { EXE = "MSI_HK_FLAG_1", KEY1 = 49, KEY2 = 17 },
    [11] = { EXE = "MSI_HK_FLAG_2", KEY1 = 50, KEY2 = 17 },
    [12] = { EXE = "MSI_HK_FLAG_3", KEY1 = 51, KEY2 = 17 },
    [13] = { EXE = "MSI_HK_FLAG_4", KEY1 = 52, KEY2 = 17 },
    [14] = { EXE = "MSI_HK_FLAG_5", KEY1 = 53, KEY2 = 17 },
    [15] = { EXE = "MSI_HK_FLAG_6", KEY1 = 54, KEY2 = 17 },
    [16] = { EXE = "MSI_HK_FLAG_7", KEY1 = 55, KEY2 = 17 },
    [17] = { EXE = "MSI_HK_FLAG_8", KEY1 = 56, KEY2 = 17 },
    [18] = { EXE = "MSI_HK_FLAG_9", KEY1 = 57, KEY2 = 17 },
    [19] = { EXE = "MSI_HK_FLAG_10", KEY1 = 48, KEY2 = 17 },
}
HOTKEY_4 = {
    [0] = { EXE = "MSI_HK_SKILLBAR5_1" },
    [35] = { EXE = "MSI_HK_SKILLBAR8_9" },
}
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use winit::keyboard::SmolStr;

    fn sample() -> Shortcuts {
        Shortcuts::from_lua(
            r#"
            KEYNAME = { [112] = "F1", [86] = "V", [18] = "Alt", [9] = "Tab" }
            HOTKEY_1 = {
                [0] = { EXE = "MSI_HK_SKILLBAR1_1", KEY1 = 112 },
                [1] = { EXE = "MSI_HK_SKILLBAR1_2" },
            }
            HOTKEY_2 = {
                [0] = { EXE = MsgStrID.MSI_HK_BASICINFOWND_MINI, KEY1 = 86, KEY2 = 18 },
                [1] = { EXE = "MSI_HK_EQUIPWND_ONOFF", KEY1 = 81, KEY2 = 16 },
                [2] = { EXE = "MSI_HK_ITEMWND_ONOFF", KEY1 = 69, KEY2 = 17 },
                [3] = { EXE = "MSI_HK_FRIENDWND_ONOFF", KEY1 = 72, KEY2 = 18 },
                [4] = { EXE = "MSI_HK_ALLWND_OFF", KEY1 = 122 },
                [5] = { EXE = "MSI_HK_MINIMAP_ONOFF", KEY1 = 9, KEY2 = 17 },
                [6] = { EXE = "MSI_HK_LETTER", KEY1 = 76 },
            }
            HOTKEY_3 = {
                [0] = { EXE = "MSI_HK_MACRO_1", KEY1 = 49, KEY2 = 18 },
                [10] = { EXE = "MSI_HK_FLAG_1", KEY1 = 49, KEY2 = 17 },
            }
            HOTKEY_4 = {
                [0] = { EXE = "MSI_HK_SKILLBAR5_1", KEY1 = 112, KEY2 = 16 },
            }
            USERKEY_2 = {
                [2] = { EXE = "MSI_HK_ITEMWND_ONOFF", KEY1 = 73, KEY2 = 18 },
                [0] = { EXE = "MSI_HK_BASICINFOWND_MINI" },
            }
            "#,
        )
        .unwrap()
    }

    fn fire(shortcuts: &Shortcuts, key: &Key, alt: bool, shift: bool, ctrl: bool, repeat: bool, text_focus: bool) -> Option<Action> {
        fire_at(shortcuts, key, None, alt, shift, ctrl, repeat, text_focus)
    }

    fn fire_at(
        shortcuts: &Shortcuts,
        key: &Key,
        physical: Option<PhysicalKey>,
        alt: bool,
        shift: bool,
        ctrl: bool,
        repeat: bool,
        text_focus: bool,
    ) -> Option<Action> {
        action(shortcuts, &KeyEvent { key, physical, alt, shift, ctrl, repeat, text_focus, bar: 1 })
    }

    #[test]
    fn tells_costume_and_unbuilt_actions_from_their_neighbours() {
        assert_eq!(exe_action("MSI_HK_COSEQUIPWND_ONOFF", 0, 0), Action::Costume);
        assert_eq!(exe_action("MSI_HK_EQUIPWND_ONOFF", 0, 0), Action::Equipment);
        assert_eq!(exe_action("MSI_HK_CHANGE_SKILLBAR", 0, 0), Action::SwitchHotkeys);
        assert_eq!(exe_action("MSI_HK_SWAP_EQUIPMENT", 0, 0), Action::Unused);
        assert_eq!(exe_action("MSI_HK_SKILLBAR_ONOFF", 0, 0), Action::HotkeyBar);
    }

    #[test]
    fn parses_the_tables_and_index_zero() {
        let shortcuts = sample();
        let hotbar = shortcuts.bindings().iter().find(|binding| binding.table == 1 && binding.index == 0).unwrap();
        assert_eq!(hotbar.action, Action::Hotbar { bar: 1, slot: 0 });
        assert_eq!(hotbar.key, Some(ShortcutKey::Named(NamedKey::F1)));
        assert!(shortcuts.bindings().iter().any(|binding| binding.table == 1 && binding.index == 1 && binding.key.is_none()));
        assert!(shortcuts.bindings().iter().any(|binding| binding.action == Action::Macro(0)));
        assert!(shortcuts.bindings().iter().any(|binding| binding.action == Action::Flag(0)));
        assert!(shortcuts.bindings().iter().any(|binding| binding.action == Action::Hotbar { bar: 2, slot: 0 }));
        // MsgStrID.MSI_HK_… is the name, and [0] is read (a Lua sequence would skip it).
        assert!(shortcuts.bindings().iter().any(|binding| binding.exe == "MSI_HK_BASICINFOWND_MINI"));
    }

    #[test]
    fn maps_virtual_keys() {
        assert_eq!(vk_to_key(112), Some(ShortcutKey::Named(NamedKey::F1)));
        assert_eq!(vk_to_key(123), Some(ShortcutKey::Named(NamedKey::F12)));
        assert_eq!(vk_to_key(86), Some(ShortcutKey::Char('v')));
        assert_eq!(vk_to_key(192), Some(ShortcutKey::Char('`')));
        assert_eq!(vk_to_key(44), Some(ShortcutKey::Named(NamedKey::PrintScreen)));
        assert_eq!(vk_to_key(45), Some(ShortcutKey::Named(NamedKey::Insert)));
        assert_eq!(vk_to_key(9), Some(ShortcutKey::Named(NamedKey::Tab)));
        assert_eq!(vk_to_key(35), Some(ShortcutKey::Named(NamedKey::End)));
        assert_eq!(vk_to_key(36), Some(ShortcutKey::Named(NamedKey::Home)));
        assert_eq!(vk_to_key(48), Some(ShortcutKey::Char('0')));
        // Modifiers and the mouse buttons are not keys.
        assert_eq!(vk_to_key(18), None);
        assert_eq!(vk_to_key(1), None);
    }

    #[test]
    fn userkeys_override_rebinds_and_unbinds() {
        let shortcuts = sample();
        let labels = shortcuts.labels();
        assert_eq!(labels["inventory"], "Alt+I");
        assert!(labels.get("basic_info").is_none());
        let key = Key::Character(SmolStr::new("i"));
        assert_eq!(fire(&shortcuts, &key, true, false, false, false, false), Some(Action::Inventory));
        let old = Key::Character(SmolStr::new("e"));
        assert_eq!(fire(&shortcuts, &old, false, false, true, false, false), None);
    }

    #[test]
    fn userkeys_file_moves_inventory_to_alt_i() {
        let dir = std::env::temp_dir().join(format!("rustnarok-userkeys-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("UserKeys.lua");
        std::fs::write(
            &path,
            r#"
            USERKEY_2 = {
                [9] = { EXE = "MSI_HK_ITEMWND_ONOFF", KEY1 = 73, KEY2 = 18 },
            }
            "#,
        )
        .unwrap();
        let mut shortcuts = Shortcuts::builtin();
        shortcuts.apply_user_file(&path).unwrap();
        assert_eq!(shortcuts.labels()["inventory"], "Alt+I");
        let key = Key::Character(SmolStr::new("i"));
        assert_eq!(fire(&shortcuts, &key, true, false, false, false, true), Some(Action::Inventory));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn one_modifier_alt_then_shift_then_ctrl() {
        let shortcuts = sample();
        let vee = Key::Character(SmolStr::new("v"));
        // Alt+Shift+Ctrl+V is Alt: basic info was unbound by the sample's UserKeys, so nothing.
        assert_eq!(fire(&shortcuts, &vee, true, true, true, false, false), None);
        let queue = Key::Character(SmolStr::new("q"));
        // Shift wins over Ctrl.
        assert_eq!(fire(&shortcuts, &queue, false, true, true, false, false), Some(Action::Equipment));
        assert_eq!(fire(&shortcuts, &queue, false, false, true, false, false), None);
        let ee = Key::Character(SmolStr::new("e"));
        // Inventory moved to Alt+I, so Ctrl+E is free. Rebind a fresh list for Ctrl.
        let ctrl = Shortcuts::from_lua(
            r#"
            HOTKEY_2 = {
                [0] = { EXE = "MSI_HK_BASICINFOWND_MINI", KEY1 = 86, KEY2 = 18 },
                [1] = { EXE = "MSI_HK_EQUIPWND_ONOFF", KEY1 = 81, KEY2 = 16 },
                [2] = { EXE = "MSI_HK_ITEMWND_ONOFF", KEY1 = 69, KEY2 = 17 },
            }
            "#,
        )
        .unwrap();
        assert_eq!(fire(&ctrl, &vee, true, true, false, false, false), Some(Action::BasicInfo));
        assert_eq!(fire(&ctrl, &queue, false, true, true, false, false), Some(Action::Equipment));
        assert_eq!(fire(&ctrl, &ee, false, false, true, false, false), Some(Action::Inventory));
        assert_eq!(fire(&ctrl, &ee, false, true, false, false, false), None);
    }

    #[test]
    fn a_repeated_keydown_does_not_fire() {
        let shortcuts = Shortcuts::builtin();
        let f1 = Key::Named(NamedKey::F1);
        assert_eq!(fire(&shortcuts, &f1, false, false, false, true, false), None);
        assert_eq!(fire(&shortcuts, &f1, false, false, false, false, false), Some(Action::Hotbar { bar: 1, slot: 0 }));
        // Pressing again, not a repeat, fires at once.
        assert_eq!(fire(&shortcuts, &f1, false, false, false, false, true), Some(Action::Hotbar { bar: 1, slot: 0 }));
    }

    #[test]
    fn a_text_box_lets_through_only_some_keys() {
        let shortcuts = Shortcuts::builtin();
        let f1 = Key::Named(NamedKey::F1);
        let aitch = Key::Character(SmolStr::new("h"));
        let letter = Key::Character(SmolStr::new("v"));
        let ay = Key::Character(SmolStr::new("a"));
        let gee = Key::Character(SmolStr::new("g"));
        assert_eq!(fire(&shortcuts, &f1, false, false, false, false, true), Some(Action::Hotbar { bar: 1, slot: 0 }));
        assert_eq!(fire(&shortcuts, &aitch, true, false, false, false, true), Some(Action::Friends));
        // A bare letter stays in the text box. The same letter fires when nothing is focused.
        let bare = Shortcuts::from_lua(r#"HOTKEY_2 = { [0] = { EXE = "MSI_HK_BASICINFOWND_MINI", KEY1 = 86 } }"#).unwrap();
        assert_eq!(fire(&bare, &letter, false, false, false, false, true), None);
        assert_eq!(fire(&bare, &letter, false, false, false, false, false), Some(Action::BasicInfo));
        // Ctrl+A/C/V/X stay with the text box; another Ctrl chord does not.
        let ctrl_a = Shortcuts::from_lua(r#"HOTKEY_2 = { [0] = { EXE = "MSI_HK_STATUSWND_ONOFF", KEY1 = 65, KEY2 = 17 } }"#).unwrap();
        assert_eq!(fire(&ctrl_a, &ay, false, false, true, false, true), None);
        assert_eq!(fire(&ctrl_a, &ay, false, false, true, false, false), Some(Action::Status));
        assert_eq!(fire(&shortcuts, &gee, false, false, true, false, true), Some(Action::Clan));
        let tick = Key::Character(SmolStr::new("`"));
        assert_eq!(fire(&shortcuts, &tick, false, false, true, false, true), Some(Action::WorldMap));
        let insert = Key::Named(NamedKey::Insert);
        assert_eq!(fire(&shortcuts, &insert, false, false, false, false, true), Some(Action::Sit));
        let shot = Key::Named(NamedKey::PrintScreen);
        assert_eq!(fire(&shortcuts, &shot, false, false, false, false, true), Some(Action::Screenshot));
    }

    #[test]
    fn tells_tables_laid_out_otherwise() {
        assert!(Shortcuts::builtin().knows_the_tables());
        // Older data: its window table holds what this client takes for macros, and nothing opens the inventory.
        let older = Shortcuts { bindings: Shortcuts::builtin().bindings.into_iter().filter(|binding| binding.action != Action::Inventory).collect() };
        assert!(!older.knows_the_tables());
    }

    #[test]
    fn q_on_hotbar_row_2_slot_1_fires_only_when_the_line_is_closed() {
        // UserKeys binds Q (VK 81) to HOTKEY_1 index 9, which is hotbar row 2 slot 1. That row has no default key.
        // Battle mode leaves the line closed, so text_focus is false and Q fires. An open line keeps the letter.
        let mut shortcuts = Shortcuts::builtin();
        let rows = lua::hotkey_rows(r#"USERKEY_1 = { [9] = { EXE = "MSI_HK_SKILLBAR2_1", KEY1 = 81 } }"#).unwrap();
        shortcuts.apply_user(&rows);
        let queue = Key::Character(SmolStr::new("q"));
        assert_eq!(fire(&shortcuts, &queue, false, false, false, false, false), Some(Action::Hotbar { bar: 1, slot: 9 }));
        assert_eq!(fire(&shortcuts, &queue, false, false, false, false, true), None);
        let unbound = Shortcuts::builtin();
        assert_eq!(fire(&unbound, &queue, false, false, false, false, false), None);
    }

    #[test]
    fn the_two_hotbars_share_keys_and_the_showing_one_fires() {
        // The player's file binds Q to row 2 slot 1 of both bars; neither clears the other.
        let mut shortcuts = Shortcuts::builtin();
        let rows = lua::hotkey_rows(
            r#"USERKEY_1 = { [9] = { EXE = "Hotkey 2-1", KEY1 = 81 } }
               USERKEY_4 = { [9] = { EXE = "Hotkey 2 2-1", KEY1 = 81 } }
               USERKEY_2 = { [55] = { EXE = "Switch Hotkey Bar", KEY1 = 71, KEY2 = 17 } }"#,
        )
        .unwrap();
        shortcuts.apply_user(&rows);
        let queue = Key::Character(SmolStr::new("q"));
        let on = |bar: u8| {
            action(&shortcuts, &KeyEvent { key: &queue, physical: None, alt: false, shift: false, ctrl: false, repeat: false, text_focus: false, bar })
        };
        assert_eq!(on(1), Some(Action::Hotbar { bar: 1, slot: 9 }));
        assert_eq!(on(2), Some(Action::Hotbar { bar: 2, slot: 9 }));
        // The data's name for the bar switch isn't the bar's on/off.
        assert_eq!(exe_action("MSI_HK_CHANGE_SKILLBAR", 2, 55), Action::SwitchHotkeys);
        assert_eq!(exe_action("MSI_HK_SKILLBAR_ONOFF", 2, 17), Action::HotkeyBar);
    }

    #[test]
    fn builtin_defaults_cover_the_game_keys() {
        let shortcuts = Shortcuts::builtin();
        let labels = shortcuts.labels();
        assert_eq!(labels["friends"], "Alt+H");
        assert_eq!(labels["inventory"], "Alt+E");
        assert_eq!(labels["close_windows"], "F11");
        assert_eq!(labels["hotbar_1_1"], "F1");
        assert_eq!(labels["hotkey_bar"], "F12");
        assert_eq!(labels["minimap"], "Ctrl+Tab");
        assert_eq!(labels["world_map"], "Ctrl+`");
        assert_eq!(labels["macro_1"], "Alt+1");
        assert_eq!(labels["macro_10"], "Alt+0");
        assert_eq!(labels["sit"], "Insert");
        assert_eq!(labels["screenshot"], "PrintScreen");
        // Bound, and nothing to do yet.
        assert_eq!(labels["memorial"], "Alt+B");
        assert_eq!(labels["commands"], "Alt+Y");
        assert_eq!(labels["hunting"], "Ctrl+H");
        let f11 = Key::Named(NamedKey::F11);
        assert_eq!(fire(&shortcuts, &f11, false, false, false, false, true), Some(Action::CloseWindows));
    }
}
