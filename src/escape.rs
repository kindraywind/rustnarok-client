//! The game menu (Esc), and which window Esc closes.
//!
//! Esc follows one rule, the first that applies, and never on a key that is auto-repeating. The windows it can
//! close are kept in a `Vec` on the game, newest at the front.

use rustnarok_client::grf::Grf;

use crate::render::{Gpu, Texture};
use crate::window::{Chrome, Frame};
use crate::{Layer, UI, contains, screen};

const WIDTH: f32 = 280.0;
const BUTTON: [f32; 2] = [221.0, 20.0];
/// Buttons start here, under the 17px title.
const BUTTON_X: f32 = 29.0;
const BUTTON_Y: f32 = 22.0;
const STEP: f32 = 23.0;
// ponytail: the gap under the last button; the spec places the buttons, not the frame's bottom.
const BOTTOM: f32 = 6.0;
const TITLE_MESSAGE: usize = 1483;
/// "Do you really want to go back to your savepoint?"
pub const SAVE_QUESTION: usize = 1548;

/// rathena `IG_TOKEN_OF_SIEGFRIED`: Token, F, E and DF of Siegfried.
const SIEGFRIED: [u32; 4] = [7621, 6293, 6316, 25310];
/// EFST_LIGHT_OF_REGENE. `EFST_BLANK` is -1, so this is its place in status.hpp.
const LIGHT_OF_REGENE: u16 = 580;

/// A window Esc can close. The basic info window, the hotbar, the minimap, the chat window and the status icons
/// are not here: they stay up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowId {
    Inventory,
    Equipment,
    Skills,
    Storage,
    Party,
    Status,
    Guild,
    Clan,
    Cart,
    Pet,
    Mercenary,
    MercenarySkills,
    Homunculus,
    HomunculusSkills,
    NpcDialog,
    WarpList,
    /// An NPC shop: Esc presses its Cancel, which tells the server.
    Shop,
    Refine,
    GradeEnchant,
    LaphineSynthesis,
    LaphineUpgrade,
    ItemReform,
    Enchant,
    Stylist,
    Materials,
    PartySetup,
    DressRoom,
    Ranking,
    Reputation,
    Navigation,
    Catalog,
    /// A skill's description (a right click on a skill).
    SkillDescription,
    Roulette,
    MonsterInfo,
    Achievements,
    OtherEquipment,
    Bank,
    CashShop,
    PickList,
    Trade,
    ItemInfo,
    /// A card's picture.
    CardPicture,
    ChatRoom,
    RoomMake,
    EmotionList,
    Macros,
    VendSetup,
    MyShop,
    BuyingStoreSetup,
    MyBuyingStore,
    Mailbox,
    MailRead,
    MailWrite,
    AudioSettings,
    GraphicsSettings,
    ShortcutSettings,
    WorldMap,
    Quests,
    Menu,
    Capturing,
    /// Questions. Esc answers them No before it would close a window (rule 3); they are still listed so `windows` shows them.
    Amount,
    SaveAsked,
    BindAsked,
    ExpansionAsked,
    LeaderAsked,
    DismissAsked,
    HomunculusRename,
    PetAsked,
    CashShopBuy,
    GuildAsked,
    GuildInvite,
    PartyInvite,
    FriendAsked,
    TradeAsked,
    DealChoice,
    /// The battleground list.
    BattleQueue,
    /// Asked to adopt a baby.
    AdoptAsked,
    /// A queue question: applying, leaving, or the battle lobby.
    QueueAsked,
    /// A 1:1 whisper window. Several can be open; Esc closes the front one.
    Whisper,
    /// The two checkboxes that open 1:1 windows.
    FriendSetup,
    /// /ex: the names whose whispers are blocked.
    BlockList,
    /// The away-message box.
    AwayMessage,
}

impl WindowId {
    /// The name `windows` uses in the control state.
    pub fn name(self) -> &'static str {
        match self {
            Self::Inventory => "Inventory",
            Self::Equipment => "Equipment",
            Self::Skills => "Skills",
            Self::Storage => "Storage",
            Self::Party => "Party",
            Self::Status => "Status",
            Self::Guild => "Guild",
            Self::Clan => "Clan",
            Self::Cart => "Cart",
            Self::Pet => "Pet",
            Self::Mercenary => "Mercenary",
            Self::MercenarySkills => "MercenarySkills",
            Self::Homunculus => "Homunculus",
            Self::HomunculusSkills => "HomunculusSkills",
            Self::NpcDialog => "NpcDialog",
            Self::WarpList => "WarpList",
            Self::Shop => "Shop",
            Self::Refine => "Refine",
            Self::GradeEnchant => "GradeEnchant",
            Self::LaphineSynthesis => "LaphineSynthesis",
            Self::LaphineUpgrade => "LaphineUpgrade",
            Self::ItemReform => "ItemReform",
            Self::Enchant => "Enchant",
            Self::Stylist => "Stylist",
            Self::Materials => "Materials",
            Self::PartySetup => "PartySetup",
            Self::DressRoom => "DressRoom",
            Self::Ranking => "Ranking",
            Self::Reputation => "Reputation",
            Self::Navigation => "Navigation",
            Self::Catalog => "Catalog",
            Self::SkillDescription => "SkillDescription",
            Self::Roulette => "Roulette",
            Self::MonsterInfo => "MonsterInfo",
            Self::Achievements => "Achievements",
            Self::OtherEquipment => "OtherEquipment",
            Self::Bank => "Bank",
            Self::CashShop => "CashShop",
            Self::PickList => "PickList",
            Self::Trade => "Trade",
            Self::ItemInfo => "ItemInfo",
            Self::CardPicture => "CardPicture",
            Self::ChatRoom => "ChatRoom",
            Self::RoomMake => "RoomMake",
            Self::EmotionList => "EmotionList",
            Self::Macros => "Macros",
            Self::VendSetup => "VendSetup",
            Self::MyShop => "MyShop",
            Self::BuyingStoreSetup => "BuyingStoreSetup",
            Self::MyBuyingStore => "MyBuyingStore",
            Self::Mailbox => "Mailbox",
            Self::MailRead => "MailRead",
            Self::MailWrite => "MailWrite",
            Self::AudioSettings => "AudioSettings",
            Self::GraphicsSettings => "GraphicsSettings",
            Self::ShortcutSettings => "ShortcutSettings",
            Self::WorldMap => "WorldMap",
            Self::Quests => "Quests",
            Self::Menu => "Menu",
            Self::Capturing => "Capturing",
            Self::Amount => "Amount",
            Self::SaveAsked => "SaveAsked",
            Self::BindAsked => "BindAsked",
            Self::ExpansionAsked => "InventoryExpansion",
            Self::LeaderAsked => "LeaderAsked",
            Self::DismissAsked => "DismissAsked",
            Self::HomunculusRename => "HomunculusRename",
            Self::PetAsked => "PetAsked",
            Self::CashShopBuy => "CashShopBuy",
            Self::GuildAsked => "GuildAsked",
            Self::GuildInvite => "GuildInvite",
            Self::PartyInvite => "PartyInvite",
            Self::FriendAsked => "FriendAsked",
            Self::TradeAsked => "TradeAsked",
            Self::DealChoice => "DealChoice",
            Self::BattleQueue => "BattleQueue",
            Self::AdoptAsked => "AdoptAsked",
            Self::QueueAsked => "QueueAsked",
            Self::Whisper => "Whisper",
            Self::FriendSetup => "FriendSetup",
            Self::BlockList => "BlockList",
            Self::AwayMessage => "AwayMessage",
        }
    }
}

/// Puts `id` at the front (just opened, or pressed). A second press of the same window moves it.
pub fn touch(order: &mut Vec<WindowId>, id: WindowId) {
    order.retain(|&window| window != id);
    order.insert(0, id);
}

/// Drops `id` (the window closed).
pub fn remove(order: &mut Vec<WindowId>, id: WindowId) {
    order.retain(|&window| window != id);
}

/// Drops windows that are gone and puts newly opened ones at the front, in the order `open` lists them
/// (the later new one ends up newest).
pub fn sync(order: &mut Vec<WindowId>, open: &[WindowId]) {
    order.retain(|id| open.contains(id));
    let fresh: Vec<WindowId> = open.iter().copied().filter(|id| !order.contains(id)).collect();
    for id in fresh {
        order.insert(0, id);
    }
}

/// What one Esc press does. A repeating key does nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EscAct {
    /// Battle-mode chat (#179): the whisper-name list was open.
    CloseWhisperList,
    /// Battle-mode chat (#179): close the line.
    CloseBattleChat,
    /// A text box in this window has focus: the window cancels.
    Cancel(WindowId),
    /// A question is up: answer it No.
    AnswerNo,
    /// Close this window.
    Close(WindowId),
    /// The NPC shop: press Cancel instead of hiding it.
    ShopCancel,
    /// Nothing closed: open or close the game menu.
    ToggleMenu,
}

/// What Esc looks at. `battle_line` is the battle-mode chat line from #179; the always-on chat line is not it,
/// so a closed line leaves this false and Esc moves on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EscView {
    pub battle_line: bool,
    pub whisper_list: bool,
    pub focused: Option<WindowId>,
    /// An NPC's text window or choice menu is up.
    pub conversation: bool,
    pub question: bool,
    pub newest: Option<WindowId>,
}

/// The first rule that applies. `repeat` is a key auto-repeat: Esc ignores those.
pub fn decide(repeat: bool, view: EscView) -> Option<EscAct> {
    if repeat {
        return None;
    }
    // 1. Battle-mode chat line. Closed until #179 opens it; this arm stays first.
    if view.battle_line {
        return Some(if view.whisper_list { EscAct::CloseWhisperList } else { EscAct::CloseBattleChat });
    }
    // 2. A text box in a window, not the chat line.
    if let Some(id) = view.focused {
        return Some(EscAct::Cancel(id));
    }
    // 3. A question: No.
    if view.question {
        return Some(EscAct::AnswerNo);
    }
    // 4. The newest window. An NPC shop's Cancel tells the server.
    if let Some(id) = view.newest {
        return Some(if id == WindowId::Shop { EscAct::ShopCancel } else { EscAct::Close(id) });
    }
    // 5. The game menu, but not over a conversation with an NPC.
    (!view.conversation).then_some(EscAct::ToggleMenu)
}

/// Token of Siegfried (and the other tokens in that group) or Light of Regene: the player can stand up here.
pub fn can_stand_up(items: impl IntoIterator<Item = u32>, icons: impl IntoIterator<Item = u16>) -> bool {
    items.into_iter().any(|id| SIEGFRIED.contains(&id)) || icons.into_iter().any(|id| id == LIGHT_OF_REGENE)
}

/// The menu's top-left at this screen size: x = (W − 640) / 2 + 185, y = 300·H/480.
pub fn menu_origin(screen: [f32; 2]) -> [f32; 2] {
    [((screen[0] - 640.0) / 2.0 + 185.0).floor(), (300.0 * screen[1] / 480.0).floor()]
}

/// The menu's buttons.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Choice {
    CharSelect,
    /// The graphics settings window (#68).
    GameSettings,
    /// The shortcut settings window (#68).
    Shortcuts,
    Exit,
    Return,
    SavePoint,
    Resurrect,
}

impl Choice {
    fn stem(self) -> &'static str {
        match self {
            Self::CharSelect => "esc_01",
            Self::GameSettings => "esc_001",
            Self::Shortcuts => "esc_08",
            Self::Exit => "esc_09",
            Self::Return => "esc_02",
            Self::SavePoint => "esc_10",
            Self::Resurrect => "esc_05",
        }
    }

    pub fn button(self) -> &'static str {
        match self {
            Self::CharSelect => "char_select",
            Self::GameSettings => "settings",
            Self::Shortcuts => "shortcuts",
            Self::Exit => "exit",
            Self::Return => "return",
            Self::SavePoint => "save_point",
            Self::Resurrect => "resurrect",
        }
    }
}

pub struct Assets {
    title: Texture,
    /// Each button's normal, hovered and pressed images (`esc_NNa/b/c.bmp`).
    buttons: Vec<(Choice, [Texture; 3])>,
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |name: String| gpu.load_texture(grf, &name, false);
        let choices = [Choice::CharSelect, Choice::GameSettings, Choice::Shortcuts, Choice::Exit, Choice::Return, Choice::SavePoint, Choice::Resurrect];
        Self {
            title: image(format!("{UI}basic_interface\\titlebar_fix.bmp")),
            buttons: choices.into_iter().map(|choice| (choice, ["a", "b", "c"].map(|look| image(format!("{UI}{}{look}.bmp", choice.stem()))))).collect(),
        }
    }
}

pub struct Menu {
    /// Opened because the player died.
    pub dead: bool,
}

impl Menu {
    /// Alive: character select, game settings, shortcut settings, exit, return.
    /// Dead: save point and return; resurrect here when something can stand the player up.
    pub fn choices(&self, resurrect: bool) -> Vec<Choice> {
        if self.dead {
            let mut choices = Vec::new();
            if resurrect {
                choices.push(Choice::Resurrect);
            }
            choices.push(Choice::SavePoint);
            choices.push(Choice::Return);
            choices
        } else {
            vec![Choice::CharSelect, Choice::GameSettings, Choice::Shortcuts, Choice::Exit, Choice::Return]
        }
    }

    fn height(&self, resurrect: bool) -> f32 {
        BUTTON_Y + self.choices(resurrect).len() as f32 * STEP + BOTTOM
    }

    fn corner(&self) -> [f32; 2] {
        menu_origin(screen())
    }

    pub fn button_corner(&self, row: usize) -> [f32; 2] {
        self.button_corner_at(row, screen())
    }

    pub fn button_corner_at(&self, row: usize, screen: [f32; 2]) -> [f32; 2] {
        let corner = menu_origin(screen);
        [corner[0] + BUTTON_X, corner[1] + BUTTON_Y + row as f32 * STEP]
    }

    /// The buttons, for the control socket: name and box.
    pub fn button_boxes(&self, resurrect: bool) -> Vec<(&'static str, [f32; 4])> {
        self.choices(resurrect)
            .into_iter()
            .enumerate()
            .map(|(row, choice)| {
                let corner = self.button_corner(row);
                (choice.button(), [corner[0], corner[1], BUTTON[0], BUTTON[1]])
            })
            .collect()
    }

    /// Over the menu: the button under the pointer, if any. `Some(None)` is the menu, off a button.
    pub fn hit(&self, cursor: [f32; 2], resurrect: bool) -> Option<Option<Choice>> {
        let corner = self.corner();
        if !contains(corner, [WIDTH, self.height(resurrect)], cursor) {
            return None;
        }
        Some(self.choices(resurrect).into_iter().enumerate().find(|(row, _)| contains(self.button_corner(*row), BUTTON, cursor)).map(|(_, choice)| choice))
    }

    pub fn draw<'a>(&self, layer: &mut Layer<'a>, chrome: &'a Chrome, assets: &'a Assets, messages: &[String], pointer: ([f32; 2], bool), resurrect: bool) {
        let corner = self.corner();
        let height = self.height(resurrect);
        layer.sprites.push((&chrome.white, corner, [WIDTH, height]));
        layer.sprites.push((&assets.title, corner, [WIDTH, assets.title.size[1]]));
        let mut frame = Frame::new(corner, WIDTH);
        frame.open = true;
        frame.draw_title(layer, chrome, messages.get(TITLE_MESSAGE).map_or("", String::as_str), pointer.0);
        let hovered = self.hit(pointer.0, resurrect).flatten();
        for (row, choice) in self.choices(resurrect).into_iter().enumerate() {
            let images = &assets.buttons.iter().find(|(which, _)| *which == choice).unwrap().1;
            let look = if hovered == Some(choice) { 1 + usize::from(pointer.1) } else { 0 };
            layer.sprites.push((&images[look], self.button_corner(row), BUTTON));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn esc_list_is_newest_open_or_pressed_first() {
        let mut order = Vec::new();
        touch(&mut order, WindowId::Inventory);
        touch(&mut order, WindowId::Skills);
        assert_eq!(order, [WindowId::Skills, WindowId::Inventory]);
        // A press inside the inventory puts it back in front.
        touch(&mut order, WindowId::Inventory);
        assert_eq!(order, [WindowId::Inventory, WindowId::Skills]);
        remove(&mut order, WindowId::Inventory);
        assert_eq!(order, [WindowId::Skills]);
        remove(&mut order, WindowId::Skills);
        assert!(order.is_empty());

        let mut order = vec![WindowId::Skills];
        sync(&mut order, &[WindowId::Skills, WindowId::Shop]);
        assert_eq!(order, [WindowId::Shop, WindowId::Skills]);
        sync(&mut order, &[WindowId::Skills]);
        assert_eq!(order, [WindowId::Skills]);
    }

    #[test]
    fn esc_follows_the_first_rule() {
        let window =
            EscView { battle_line: false, whisper_list: false, focused: None, conversation: false, question: false, newest: Some(WindowId::Inventory) };
        // A repeating key does nothing, even with a window up.
        assert_eq!(decide(true, window), None);
        // A closed battle-mode line is not the first case: the window closes.
        assert_eq!(decide(false, window), Some(EscAct::Close(WindowId::Inventory)));
        // An open battle-mode line is first: the whisper list, else the line. Windows and questions wait.
        let battle = EscView {
            battle_line: true,
            whisper_list: true,
            focused: Some(WindowId::Mailbox),
            conversation: false,
            question: true,
            newest: Some(WindowId::Skills),
        };
        assert_eq!(decide(false, battle), Some(EscAct::CloseWhisperList));
        assert_eq!(decide(false, EscView { whisper_list: false, ..battle }), Some(EscAct::CloseBattleChat));
        // A text box beats a question and the newest window.
        let typing = EscView {
            battle_line: false,
            whisper_list: false,
            focused: Some(WindowId::VendSetup),
            conversation: false,
            question: true,
            newest: Some(WindowId::Shop),
        };
        assert_eq!(decide(false, typing), Some(EscAct::Cancel(WindowId::VendSetup)));
        // A question beats the newest window.
        let question = EscView { focused: None, ..typing };
        assert_eq!(decide(false, question), Some(EscAct::AnswerNo));
        // The newest window. A shop presses Cancel.
        assert_eq!(decide(false, EscView { question: false, newest: Some(WindowId::Shop), ..question }), Some(EscAct::ShopCancel));
        assert_eq!(decide(false, EscView { question: false, newest: Some(WindowId::Skills), ..question }), Some(EscAct::Close(WindowId::Skills)));
        // Nothing open: the menu.
        let idle = EscView { newest: None, question: false, ..question };
        assert_eq!(decide(false, idle), Some(EscAct::ToggleMenu));
        // Talking to an NPC: nothing, unless another window is newer (it closes) or its text box has the caret
        // (it's cancelled). The warp list isn't a conversation: the menu toggles over it.
        let talking = EscView { conversation: true, ..idle };
        assert_eq!(decide(false, talking), None);
        assert_eq!(decide(false, EscView { newest: Some(WindowId::Inventory), ..talking }), Some(EscAct::Close(WindowId::Inventory)));
        assert_eq!(decide(false, EscView { focused: Some(WindowId::NpcDialog), ..talking }), Some(EscAct::Cancel(WindowId::NpcDialog)));
    }

    #[test]
    fn menu_buttons_and_place() {
        let alive = Menu { dead: false };
        assert_eq!(alive.choices(false), [Choice::CharSelect, Choice::GameSettings, Choice::Shortcuts, Choice::Exit, Choice::Return]);
        assert_eq!(alive.choices(true), alive.choices(false), "a living player has no resurrect button");
        let dead = Menu { dead: true };
        assert_eq!(dead.choices(false), [Choice::SavePoint, Choice::Return]);
        assert_eq!(dead.choices(true), [Choice::Resurrect, Choice::SavePoint, Choice::Return]);

        assert_eq!(menu_origin([1024.0, 768.0]), [377.0, 480.0]);
        assert_eq!(menu_origin([1280.0, 720.0]), [505.0, 450.0]);
        assert_eq!(alive.button_corner_at(0, [1024.0, 768.0]), [377.0 + 29.0, 480.0 + 22.0]);
        assert_eq!(alive.button_corner_at(1, [1280.0, 720.0]), [505.0 + 29.0, 450.0 + 22.0 + 23.0]);
        let first = alive.button_corner(0);
        assert_eq!(alive.hit([first[0] + 5.0, first[1] + 5.0], false), Some(Some(Choice::CharSelect)));
        assert_eq!(alive.hit([0.0, 0.0], false), None);
        let save = dead.button_corner(0);
        assert_eq!(dead.hit([save[0] + 5.0, save[1] + 5.0], false), Some(Some(Choice::SavePoint)));
        assert_eq!(dead.hit([save[0] + 5.0, save[1] + 5.0], true), Some(Some(Choice::Resurrect)));
    }

    #[test]
    fn standing_up_needs_a_token_or_light_of_regene() {
        assert!(can_stand_up([7621], []));
        assert!(can_stand_up([6293, 501], []));
        assert!(can_stand_up([], [LIGHT_OF_REGENE]));
        assert!(!can_stand_up([501], [1]));
    }
}
