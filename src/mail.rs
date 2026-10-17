//! RODEX mail: the mailbox (its tabs, a page of mail, refresh, write and search), a mail being read (its text, the
//! zeny and items to take, reply and delete) and one being written (the receiver checked by name, title, text, zeny
//! and items), over the client's own renewal backgrounds, and the icon that says mail came.

use std::collections::HashMap;

use roclient::grf::Grf;
use roclient::lua::ItemInfo;
use roclient::messages::fill;
use roclient::net::{InventoryItem, MAIL_ITEM, MAIL_NPC, MAIL_ZENY, MailHeader, MapEvent};
use roclient::text::Font;

use crate::inventory;
use crate::render::{Gpu, Texture};
use crate::shop::group;
use crate::window::{self, Chrome, Frame, TitleHit};
use crate::{Layer, SCREEN, UI, contains, offset};

// ponytail: the rows, slots and boxes are read off bg_rodex_list.bmp and bg_rodex_read.bmp; what sits where over
// them (buttons, labels, the icon on screen) is a guess to calibrate against the real client.
const LIST_POSITION: [f32; 2] = [140.0, 90.0];
const LETTER_POSITION: [f32; 2] = [460.0, 100.0];
const COMPOSE_POSITION: [f32; 2] = [490.0, 140.0];
/// The backgrounds' sizes (bg_rodex_list.bmp, bg_rodex_read.bmp) and the new-mail icon's.
const LIST_SIZE: [f32; 2] = [309.0, 416.0];
const LETTER_SIZE: [f32; 2] = [300.0, 400.0];
const NOTICE_SIZE: [f32; 2] = [43.0, 43.0];
const ROWS: usize = 6;
const ROW_TOP: f32 = 113.0;
const ROW: f32 = 44.0;
const TAB: [f32; 2] = [68.0, 28.0];
const TABS_AT: [f32; 2] = [18.0, 85.0];
const ICON_BUTTON: [f32; 2] = [24.0, 26.0];
const REFRESH: [f32; 2] = [243.0, 26.0];
const WRITE: [f32; 2] = [271.0, 26.0];
const SEARCH_FIELD: [f32; 4] = [136.0, 66.0, 127.0, 16.0];
const SEARCH_BUTTON: [f32; 2] = [264.0, 65.0];
const SEARCH_BUTTON_SIZE: [f32; 2] = [32.0, 18.0];
/// The checkboxes picking what the search looks at: titles, senders.
const SEARCH_BY: [[f32; 2]; 2] = [[19.0, 69.0], [72.0, 69.0]];
const CHECKBOX: [f32; 2] = [10.0, 10.0];
const PAGE_BUTTON: [f32; 2] = [24.0, 24.0];
const PREV: [f32; 2] = [110.0, 389.0];
const NEXT: [f32; 2] = [175.0, 389.0];
/// A letter: its text's box, its item slots (centers), the zeny box and the buttons.
const TEXT_BOX: [f32; 4] = [9.0, 71.0, 282.0, 220.0];
const SLOT_CENTERS: [f32; 5] = [41.5, 75.5, 109.5, 143.5, 177.5];
const SLOT_TOP: f32 = 303.0;
const ITEM_ICON: [f32; 2] = [24.0, 24.0];
const ZENY_BOX: [f32; 4] = [19.0, 343.0, 262.0, 24.0];
const RECEIVE: [f32; 2] = [26.0, 20.0];
const CLAIM_ITEMS: [f32; 2] = [258.0, 315.0];
const CLAIM_ZENY: [f32; 2] = [251.0, 345.0];
const BUTTON: [f32; 2] = [70.0, 18.0];
const REPLY_BUTTON: [f32; 2] = [70.0, 376.0];
const DELETE_BUTTON: [f32; 2] = [160.0, 376.0];
const SEND_BUTTON: [f32; 2] = [115.0, 376.0];
const TO_FIELD: [f32; 4] = [60.0, 21.0, 150.0, 16.0];
const TITLE_FIELD: [f32; 4] = [60.0, 43.0, 226.0, 16.0];
const VALIDATE: [f32; 2] = [216.0, 20.0];
const LINE: f32 = 16.0;
const NOTICE: [f32; 2] = [SCREEN[0] - 16.0 - 43.0, 152.0];
const MAX_ITEMS: usize = 5;
/// Messages: the windows' names, the tabs, the labels and the buttons.
const LIST_TITLE: usize = 2727;
const SEND_TITLE: usize = 2728;
const READ_TITLE: usize = 2729;
const TABS: [usize; 4] = [3546, 3547, 3548, 3549];
pub const TAB_SEARCH: usize = 3;
const TITLE_LABEL: usize = 3192;
const SENDER_LABEL: usize = 3193;
const TO_LABEL: usize = 1099;
const ZENY_LABEL: usize = 2464;
const DELETE_LABEL: usize = 3542;
const REPLY_LABEL: usize = 3543;
const SEND_LABEL: usize = 3544;
const VALIDATE_LABEL: usize = 3545;
/// Time left: "%d minute", "%d hour", "%d day" in the list, "in %d minutes" and so on in a letter.
const LEFT_SHORT: [usize; 3] = [2850, 2851, 2852];
const LEFT_LONG: [usize; 3] = [2608, 2609, 2610];
/// What the chat says: no receiver, a bad title, attachments still in, no such name, deleted.
const NO_RECEIVER: usize = 2595;
const BAD_TITLE: usize = 2596;
const CLAIM_FIRST: usize = 2612;
const NO_SUCH_NAME: usize = 2615;
const DELETED: usize = 2907;
const GREY: [u8; 3] = [0x80, 0x80, 0x80];
const TEXT: [u8; 3] = [0, 0, 0];
const FIELD_BORDER: [u8; 4] = [0xAD, 0xAD, 0xAD, 0xFF];

pub struct Assets {
    list: Texture,
    letter: Texture,
    /// Out, over and pressed looks of the buttons.
    tab: [Texture; 3],
    button: [Texture; 3],
    refresh: [Texture; 3],
    write: [Texture; 3],
    search: [Texture; 3],
    prev: [Texture; 3],
    next: [Texture; 3],
    receive: [Texture; 3],
    /// Read, unread and returned mail.
    status: [Texture; 3],
    /// Zeny, items, both.
    holds: [Texture; 3],
    check: [Texture; 2],
    balloon: Texture,
    slot: Texture,
    notice: Texture,
    field: Texture,
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |name: &str| gpu.load_texture(grf, &format!("{UI}basic_interface\\rodexsystem\\renewal\\{name}.bmp"), false);
        let looks = |name: &str| ["out", "over", "press"].map(|look| image(&format!("{name}_{look}")));
        Self {
            list: image("bg_rodex_list"),
            letter: image("bg_rodex_read"),
            tab: looks("btn_listtab_empty"),
            button: looks("btn_confirm_id_empty"),
            refresh: looks("btn_refresh"),
            write: looks("btn_write"),
            search: looks("btn_search"),
            prev: looks("btn_prev"),
            next: looks("btn_next"),
            receive: looks("btn_receive"),
            status: ["icon_status_mail_read", "icon_status_mail_received", "icon_status_mail_returned"].map(image),
            holds: ["icon_zeny", "icon_item", "icon_zeny_n_item"].map(image),
            check: ["checkbox_search_off", "checkbox_search_on"].map(image),
            balloon: image("bg_id_balloon"),
            slot: image("shadow_item_on"),
            notice: image("icon_noti_gotmail"),
            field: gpu.solid(FIELD_BORDER),
        }
    }
}

/// Which tab a mail shows under: NPC mail under notices, returned mail under returns, the rest under mail.
fn tab_of(mail: &MailHeader) -> usize {
    if mail.kind == 2 {
        2
    } else if mail.flags & MAIL_NPC != 0 {
        0
    } else {
        1
    }
}

/// Time left the way the client words it, in minutes, hours or days.
fn time_left(messages: &[String], seconds: u32, words: [usize; 3]) -> String {
    let (word, value) = match seconds {
        86_400.. => (words[2], seconds / 86_400),
        3_600.. => (words[1], seconds / 3_600),
        _ => (words[0], seconds / 60),
    };
    fill(messages.get(word).map_or("", String::as_str), &[&value])
}

/// A button's look: out, over, or pressed.
fn look(hovered: bool, pressed: bool) -> usize {
    if hovered { 1 + usize::from(pressed) } else { 0 }
}

fn inside(corner: [f32; 2], size: [f32; 2], local: [f32; 2]) -> bool {
    contains(corner, size, local)
}

fn field_hit(field: [f32; 4], local: [f32; 2]) -> bool {
    contains([field[0], field[1]], [field[2], field[3]], local)
}

/// Where a letter's item slot (`slot` 0-4) puts its icon.
fn slot_corner(slot: usize) -> [f32; 2] {
    [SLOT_CENTERS[slot] - ITEM_ICON[0] / 2.0, SLOT_TOP]
}

fn slot_hit(local: [f32; 2], count: usize) -> Option<usize> {
    (0..count.min(MAX_ITEMS)).find(|&slot| inside(slot_corner(slot), ITEM_ICON, local))
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum ListHit {
    Title(TitleHit),
    Refresh,
    Write,
    Search,
    SearchField,
    /// Search senders (or titles).
    SearchBy(bool),
    Tab(usize),
    /// A mail, by its place among those the tab shows.
    Row(usize),
    Prev,
    Next,
    Window,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum LetterHit {
    Title(TitleHit),
    ClaimItems,
    ClaimZeny,
    Reply,
    Delete,
    Item(usize),
    Window,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Field {
    To,
    Title,
    Text,
    Zeny,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum ComposeHit {
    Title(TitleHit),
    Field(Field),
    Validate,
    Item(usize),
    Send,
    Window,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    /// The icon saying mail came.
    Notice,
    List(ListHit),
    Letter(LetterHit),
    Compose(ComposeHit),
}

/// What a click asks of the server, or has the chat say.
#[derive(Debug, PartialEq)]
pub enum Action {
    Open,
    Close,
    Refresh,
    Read(u8, u64),
    /// Start writing, to that name (or nobody yet).
    Write(String),
    Claim {
        kind: u8,
        id: u64,
        zeny: bool,
    },
    Delete(u8, u64),
    /// Ask whose that name is.
    Check(String),
    /// Take an attached item (index, amount) back out.
    Detach(u16, u16),
    Send,
    /// Stop writing.
    Cancel,
    /// A message (by number) for the chat.
    Say(usize),
}

/// The mailbox window.
pub struct MailBox {
    pub frame: Frame,
    pub mails: Vec<MailHeader>,
    pub tab: usize,
    page: usize,
    pub search: String,
    pub typing: bool,
    by_sender: bool,
}

impl Default for MailBox {
    fn default() -> Self {
        Self { frame: Frame::new(LIST_POSITION, LIST_SIZE[0]), mails: Vec::new(), tab: 1, page: 0, search: String::new(), typing: false, by_sender: false }
    }
}

impl MailBox {
    /// The mail the open tab shows, in order.
    fn shown(&self) -> Vec<&MailHeader> {
        let search = self.search.to_lowercase();
        self.mails
            .iter()
            .filter(|mail| match self.tab {
                TAB_SEARCH => !search.is_empty() && if self.by_sender { &mail.sender } else { &mail.title }.to_lowercase().contains(&search),
                tab => tab_of(mail) == tab,
            })
            .collect()
    }

    fn pages(&self) -> usize {
        self.shown().len().div_ceil(ROWS).max(1)
    }

    fn hit(&self, cursor: [f32; 2]) -> Option<ListHit> {
        let local = self.frame.local(cursor, LIST_SIZE[1])?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(ListHit::Title(hit));
        }
        let shown = self.shown().len();
        let places = [
            (REFRESH, ICON_BUTTON, ListHit::Refresh),
            (WRITE, ICON_BUTTON, ListHit::Write),
            (SEARCH_BUTTON, SEARCH_BUTTON_SIZE, ListHit::Search),
            ([SEARCH_FIELD[0], SEARCH_FIELD[1]], [SEARCH_FIELD[2], SEARCH_FIELD[3]], ListHit::SearchField),
            (SEARCH_BY[0], [50.0, CHECKBOX[1]], ListHit::SearchBy(false)),
            (SEARCH_BY[1], [55.0, CHECKBOX[1]], ListHit::SearchBy(true)),
            (PREV, PAGE_BUTTON, ListHit::Prev),
            (NEXT, PAGE_BUTTON, ListHit::Next),
        ];
        if let Some((.., hit)) = places.into_iter().find(|&(corner, size, _)| inside(corner, size, local)) {
            return Some(hit);
        }
        if let Some(tab) = (0..TABS.len()).find(|&tab| inside([TABS_AT[0] + tab as f32 * TAB[0], TABS_AT[1]], TAB, local)) {
            return Some(ListHit::Tab(tab));
        }
        let row = ((local[1] - ROW_TOP) / ROW).floor();
        let place = self.page * ROWS + row as usize;
        Some(if local[1] >= ROW_TOP && row < ROWS as f32 && place < shown { ListHit::Row(place) } else { ListHit::Window })
    }
}

/// A mail being read.
pub struct Letter {
    pub frame: Frame,
    pub mail: MailHeader,
    /// Empty until the server sends it.
    pub text: String,
    pub zeny: u64,
    pub items: Vec<InventoryItem>,
}

impl Letter {
    fn hit(&self, cursor: [f32; 2]) -> Option<LetterHit> {
        let local = self.frame.local(cursor, LETTER_SIZE[1])?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(LetterHit::Title(hit));
        }
        Some(if !self.items.is_empty() && inside(CLAIM_ITEMS, RECEIVE, local) {
            LetterHit::ClaimItems
        } else if self.zeny > 0 && inside(CLAIM_ZENY, RECEIVE, local) {
            LetterHit::ClaimZeny
        } else if inside(REPLY_BUTTON, BUTTON, local) {
            LetterHit::Reply
        } else if inside(DELETE_BUTTON, BUTTON, local) {
            LetterHit::Delete
        } else if let Some(slot) = slot_hit(local, self.items.len()) {
            LetterHit::Item(slot)
        } else {
            LetterHit::Window
        })
    }
}

/// A mail being written.
pub struct Compose {
    pub frame: Frame,
    pub to: String,
    pub title: String,
    pub text: String,
    pub zeny: String,
    /// Where typing goes.
    pub field: Option<Field>,
    /// Who the name was checked to be: character ID, job and level.
    pub receiver: Option<(u32, u16, u16)>,
    pub items: Vec<InventoryItem>,
    pub weight: u16,
}

impl Compose {
    /// Room in each field: a name's 23 bytes, a title's 39, the text's 499 and ten digits of zeny.
    const LIMITS: [usize; 4] = [23, 39, 499, 10];

    pub fn new(to: &str) -> Self {
        let mut frame = Frame::new(COMPOSE_POSITION, LETTER_SIZE[0]);
        frame.open = true;
        let field = Some(if to.is_empty() { Field::To } else { Field::Title });
        Self { frame, to: to.to_owned(), title: String::new(), text: String::new(), zeny: String::new(), field, receiver: None, items: Vec::new(), weight: 0 }
    }

    pub fn zeny(&self) -> u64 {
        self.zeny.parse().unwrap_or(0)
    }

    fn typed(&mut self, field: Field) -> &mut String {
        match field {
            Field::To => &mut self.to,
            Field::Title => &mut self.title,
            Field::Text => &mut self.text,
            Field::Zeny => &mut self.zeny,
        }
    }

    fn type_text(&mut self, field: Field, text: &str) {
        let limit = Self::LIMITS[field as usize];
        let typed = self.typed(field);
        let room = limit.saturating_sub(typed.chars().count());
        typed.extend(
            text.chars()
                .filter(
                    |&character| {
                        if field == Field::Zeny { character.is_ascii_digit() } else { !character.is_control() || (field == Field::Text && character == '\n') }
                    },
                )
                .take(room),
        );
        if field == Field::To {
            self.receiver = None;
        }
    }

    fn hit(&self, cursor: [f32; 2]) -> Option<ComposeHit> {
        let local = self.frame.local(cursor, LETTER_SIZE[1])?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(ComposeHit::Title(hit));
        }
        Some(if inside(VALIDATE, BUTTON, local) {
            ComposeHit::Validate
        } else if inside(SEND_BUTTON, BUTTON, local) {
            ComposeHit::Send
        } else if field_hit(TO_FIELD, local) {
            ComposeHit::Field(Field::To)
        } else if field_hit(TITLE_FIELD, local) {
            ComposeHit::Field(Field::Title)
        } else if field_hit(TEXT_BOX, local) {
            ComposeHit::Field(Field::Text)
        } else if field_hit(ZENY_BOX, local) {
            ComposeHit::Field(Field::Zeny)
        } else if let Some(slot) = slot_hit(local, self.items.len()) {
            ComposeHit::Item(slot)
        } else {
            ComposeHit::Window
        })
    }
}

/// All of RODEX on screen.
#[derive(Default)]
pub struct Mail {
    pub unread: bool,
    pub list: MailBox,
    pub letter: Option<Letter>,
    pub compose: Option<Compose>,
}

impl Mail {
    /// What's under the pointer; the window written in is on top, then the one read, then the mailbox.
    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        if let Some(hit) = self.compose.as_ref().and_then(|compose| compose.hit(cursor)) {
            return Some(Hit::Compose(hit));
        }
        if let Some(hit) = self.letter.as_ref().and_then(|letter| letter.hit(cursor)) {
            return Some(Hit::Letter(hit));
        }
        if let Some(hit) = self.list.hit(cursor) {
            return Some(Hit::List(hit));
        }
        (self.unread && !self.list.frame.open && contains(NOTICE, NOTICE_SIZE, cursor)).then_some(Hit::Notice)
    }

    /// Opens the mailbox (the list comes from the server), or closes it with what's read in it.
    pub fn toggle(&mut self) -> Action {
        self.list.frame.toggle();
        if self.list.frame.open {
            Action::Open
        } else {
            self.letter = None;
            self.list.typing = false;
            Action::Close
        }
    }

    pub fn click(&mut self, hit: Hit) -> Option<Action> {
        let (search, field) = match hit {
            Hit::List(ListHit::SearchField) => (true, None),
            Hit::Compose(ComposeHit::Field(field)) => (false, Some(field)),
            _ => (false, None),
        };
        self.list.typing = search;
        if let Some(compose) = &mut self.compose {
            compose.field = field;
        }
        match hit {
            Hit::Notice => return Some(self.toggle()),
            Hit::List(hit) => return self.click_list(hit),
            Hit::Letter(LetterHit::Title(TitleHit::Close)) => self.letter = None,
            Hit::Letter(hit) => {
                let letter = self.letter.as_ref()?;
                let (kind, id) = (letter.mail.kind, letter.mail.id);
                return match hit {
                    LetterHit::ClaimItems => Some(Action::Claim { kind, id, zeny: false }),
                    LetterHit::ClaimZeny => Some(Action::Claim { kind, id, zeny: true }),
                    LetterHit::Reply => Some(Action::Write(letter.mail.sender.clone())),
                    LetterHit::Delete if letter.zeny > 0 || !letter.items.is_empty() => Some(Action::Say(CLAIM_FIRST)),
                    LetterHit::Delete => Some(Action::Delete(kind, id)),
                    LetterHit::Title(_) | LetterHit::Item(_) | LetterHit::Window => None,
                };
            }
            Hit::Compose(ComposeHit::Title(TitleHit::Close)) => {
                self.compose = None;
                return Some(Action::Cancel);
            }
            Hit::Compose(hit) => {
                let compose = self.compose.as_ref()?;
                return match hit {
                    ComposeHit::Validate if compose.to.trim().is_empty() => Some(Action::Say(NO_RECEIVER)),
                    ComposeHit::Validate => Some(Action::Check(compose.to.trim().to_owned())),
                    ComposeHit::Item(slot) => compose.items.get(slot).map(|item| Action::Detach(item.index, item.amount)),
                    ComposeHit::Send if compose.to.trim().is_empty() => Some(Action::Say(NO_RECEIVER)),
                    ComposeHit::Send if compose.title.trim().chars().count() < 4 => Some(Action::Say(BAD_TITLE)),
                    ComposeHit::Send => Some(Action::Send),
                    ComposeHit::Title(_) | ComposeHit::Field(_) | ComposeHit::Window => None,
                };
            }
        }
        None
    }

    fn click_list(&mut self, hit: ListHit) -> Option<Action> {
        let list = &mut self.list;
        match hit {
            ListHit::Title(TitleHit::Close) => return Some(self.toggle()),
            ListHit::Refresh => return Some(Action::Refresh),
            ListHit::Write if self.compose.is_none() => return Some(Action::Write(String::new())),
            ListHit::Search => {
                list.tab = TAB_SEARCH;
                list.page = 0;
            }
            ListHit::SearchBy(sender) => list.by_sender = sender,
            ListHit::Tab(tab) => {
                list.tab = tab;
                list.page = 0;
            }
            ListHit::Row(place) => {
                let mail = (*list.shown().get(place)?).clone();
                let action = Action::Read(mail.kind, mail.id);
                let mut frame = Frame::new(self.letter.as_ref().map_or(LETTER_POSITION, |letter| letter.frame.position), LETTER_SIZE[0]);
                frame.open = true;
                self.letter = Some(Letter { frame, mail, text: String::new(), zeny: 0, items: Vec::new() });
                return Some(action);
            }
            ListHit::Prev => list.page = list.page.saturating_sub(1),
            ListHit::Next => list.page = (list.page + 1).min(list.pages() - 1),
            ListHit::Title(_) | ListHit::Write | ListHit::SearchField | ListHit::Window => {}
        }
        None
    }

    /// Whether typing goes to a mail window.
    pub fn typing(&self) -> bool {
        self.list.typing || self.compose.as_ref().is_some_and(|compose| compose.field.is_some())
    }

    pub fn type_text(&mut self, text: &str) {
        if let Some(compose) = &mut self.compose
            && let Some(field) = compose.field
        {
            compose.type_text(field, text);
        } else if self.list.typing {
            let room = 23usize.saturating_sub(self.list.search.chars().count());
            self.list.search.extend(text.chars().filter(|character| !character.is_control()).take(room));
        }
    }

    pub fn backspace(&mut self) {
        if let Some(compose) = &mut self.compose
            && let Some(field) = compose.field
        {
            compose.typed(field).pop();
            if field == Field::To {
                compose.receiver = None;
            }
        } else if self.list.typing {
            self.list.search.pop();
        }
    }

    /// Enter: a new line in the text, the search done in the mailbox, the next field elsewhere (as Tab).
    pub fn enter(&mut self, tab: bool) {
        if let Some(compose) = &mut self.compose
            && let Some(field) = compose.field
        {
            if field == Field::Text && !tab {
                compose.type_text(field, "\n");
            } else {
                compose.field = Some(match field {
                    Field::To => Field::Title,
                    Field::Title => Field::Text,
                    Field::Text => Field::Zeny,
                    Field::Zeny => Field::To,
                });
            }
        } else if self.list.typing && !tab {
            self.list.typing = false;
            self.list.tab = TAB_SEARCH;
            self.list.page = 0;
        }
    }

    pub fn stop_typing(&mut self) {
        self.list.typing = false;
        if let Some(compose) = &mut self.compose {
            compose.field = None;
        }
    }

    /// Takes in what the server says about mail; returns a message (by number) for the chat, and whether it's an
    /// error.
    pub fn event(&mut self, event: MapEvent) -> Option<(usize, bool)> {
        match event {
            MapEvent::MailUnread(unread) => self.unread = unread,
            MapEvent::MailList(mails) => {
                self.list.mails = mails;
                self.list.page = self.list.page.min(self.list.pages() - 1);
            }
            MapEvent::MailRead { id, text, zeny, items, .. } => {
                if let Some(letter) = self.letter.as_mut().filter(|letter| letter.mail.id == id) {
                    (letter.text, letter.zeny, letter.items) = (text, zeny, items);
                }
            }
            MapEvent::MailClaimed { id, zeny, result } => {
                if result == 0 {
                    let flag = if zeny { MAIL_ZENY } else { MAIL_ITEM };
                    if let Some(mail) = self.list.mails.iter_mut().find(|mail| mail.id == id) {
                        mail.flags &= !flag;
                    }
                    if let Some(letter) = self.letter.as_mut().filter(|letter| letter.mail.id == id) {
                        if zeny {
                            letter.zeny = 0;
                        } else {
                            letter.items.clear();
                        }
                    }
                }
                // Zeny taken, too much zeny, failed; items moved, too heavy or no room, failed.
                return Some(match (zeny, result) {
                    (true, 0) => (2591, false),
                    (true, 1) => (2593, true),
                    (true, _) => (2592, true),
                    (false, 0) => (2588, false),
                    (false, 2) => (2590, true),
                    (false, _) => (2589, true),
                });
            }
            MapEvent::MailDeleted { id } => {
                self.list.mails.retain(|mail| mail.id != id);
                if self.letter.as_ref().is_some_and(|letter| letter.mail.id == id) {
                    self.letter = None;
                }
                return Some((DELETED, false));
            }
            // Couldn't start writing: "You cannot open the mail."
            MapEvent::MailWrite { ok: false, .. } => return Some((2604, true)),
            MapEvent::MailWrite { to, ok: true } => {
                if self.compose.is_none() {
                    self.compose = Some(Compose::new(&to));
                }
            }
            MapEvent::MailReceiver { character: 0, .. } => return Some((NO_SUCH_NAME, true)),
            MapEvent::MailReceiver { character, job, level, name } => {
                if let Some(compose) = &mut self.compose {
                    compose.to = name;
                    compose.receiver = Some((character, job, level));
                }
            }
            MapEvent::MailItemAdded { result: 0, item, weight } => {
                if let Some(compose) = &mut self.compose {
                    match compose.items.iter_mut().find(|attached| attached.index == item.index) {
                        Some(attached) => attached.amount = item.amount,
                        None => compose.items.push(item),
                    }
                    compose.weight = weight;
                }
            }
            // Too heavy, no room, can't be mailed, failed.
            MapEvent::MailItemAdded { result, .. } => {
                return Some((
                    match result {
                        1 => 2630,
                        3 => 2697,
                        4 => 2791,
                        _ => 2629,
                    },
                    true,
                ));
            }
            MapEvent::MailItemRemoved { ok, index, amount, weight } => {
                if let Some(compose) = self.compose.as_mut().filter(|_| ok) {
                    if let Some(attached) = compose.items.iter_mut().find(|attached| attached.index == index) {
                        attached.amount = attached.amount.saturating_sub(amount);
                    }
                    compose.items.retain(|attached| attached.amount > 0);
                    compose.weight = weight;
                }
            }
            MapEvent::MailSent(0) => {
                self.compose = None;
                return Some((2587, false));
            }
            // Failed, too many today, an item can't go, no such receiver.
            MapEvent::MailSent(result) => {
                return Some((
                    match result {
                        2 => 2603,
                        3 => 2598,
                        4 => 2599,
                        _ => 2597,
                    },
                    true,
                ));
            }
            _ => {}
        }
        None
    }

    /// Starts or ends a drag on whichever window's title bar is under the pointer.
    pub fn start_drag(&mut self, cursor: [f32; 2]) -> bool {
        let frame = match self.hit(cursor) {
            Some(Hit::Compose(ComposeHit::Title(TitleHit::Title))) => &mut self.compose.as_mut().unwrap().frame,
            Some(Hit::Letter(LetterHit::Title(TitleHit::Title))) => &mut self.letter.as_mut().unwrap().frame,
            Some(Hit::List(ListHit::Title(TitleHit::Title))) => &mut self.list.frame,
            _ => return false,
        };
        frame.start_drag(cursor);
        true
    }

    pub fn drag_to(&mut self, cursor: [f32; 2]) {
        self.list.frame.drag_to(cursor, LIST_SIZE[1]);
        for frame in self.letter.iter_mut().map(|letter| &mut letter.frame).chain(self.compose.iter_mut().map(|compose| &mut compose.frame)) {
            frame.drag_to(cursor, LETTER_SIZE[1]);
        }
    }

    pub fn drop_drag(&mut self) -> bool {
        self.list.frame.drop_drag()
            | self.letter.as_mut().is_some_and(|letter| letter.frame.drop_drag())
            | self.compose.as_mut().is_some_and(|compose| compose.frame.drop_drag())
    }

    /// The item icons the windows show.
    pub fn icons(&self, items: &HashMap<u32, ItemInfo>) -> Vec<String> {
        let letter = self.letter.iter().flat_map(|letter| &letter.items);
        let compose = self.compose.iter().flat_map(|compose| &compose.items);
        letter.chain(compose).filter_map(|item| items.get(&item.id).map(|info| inventory::icon_path(info, item.identified))).collect()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &self,
        layer: &mut Layer<'a>,
        tooltip: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        icons: &'a HashMap<String, Option<Texture>>,
        items: &HashMap<u32, ItemInfo>,
        font: &Font,
        messages: &[String],
        job_names: &HashMap<u16, String>,
        pointer: ([f32; 2], bool),
        caret: bool,
    ) {
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        let hit = self.hit(pointer.0);
        if self.unread && !self.list.frame.open {
            layer.sprites.push((&assets.notice, NOTICE, assets.notice.size));
        }
        if self.list.frame.open {
            self.draw_list(layer, chrome, assets, font, messages, hit, pointer, caret);
        }
        // Items on a letter (or in one being written): icon, how many, and the name when pointed at.
        let draw_items = |layer: &mut Layer<'a>, tooltip: &mut Layer<'a>, frame: &Frame, attached: &[InventoryItem], hovered: Option<usize>| {
            for (slot, item) in attached.iter().take(MAX_ITEMS).enumerate() {
                let corner = frame.at(slot_corner(slot));
                if hovered == Some(slot) {
                    layer.sprites.push((&assets.slot, frame.at([SLOT_CENTERS[slot] - 16.0, SLOT_TOP + 15.0]), assets.slot.size));
                }
                if let Some(Some(icon)) = items.get(&item.id).and_then(|info| icons.get(&inventory::icon_path(info, item.identified))) {
                    layer.sprites.push((icon, corner, icon.size));
                }
                let amount = item.amount.to_string();
                let place = offset(corner, [ITEM_ICON[0] - font.width(&amount), 12.0]);
                layer.texts.push((amount.clone(), offset(place, [-1.0, -1.0]), [255; 3]));
                layer.texts.push((amount, place, TEXT));
                if hovered == Some(slot) {
                    let name = inventory::item_name(items, item.id, item.identified, item.refine);
                    window::tooltip(tooltip, chrome, font, &name, offset(pointer.0, [12.0, 12.0]), [255; 3]);
                }
            }
        };
        let text_lines = |text: &str| text.split('\n').flat_map(|paragraph| window::wrap(font, paragraph, TEXT_BOX[2] - 12.0, usize::MAX)).collect::<Vec<_>>();
        let text_button = |layer: &mut Layer<'a>, frame: &Frame, corner: [f32; 2], label: &str, hovered: bool| {
            let image = &assets.button[look(hovered, pointer.1)];
            layer.sprites.push((image, frame.at(corner), image.size));
            let text = window::fit(font, label, BUTTON[0] - 6.0);
            let x = corner[0] + ((BUTTON[0] - font.width(&text)) / 2.0).floor();
            layer.texts.push((text, frame.at([x, corner[1] + ((BUTTON[1] - font.line_height()) / 2.0).floor()]), TEXT));
        };

        if let Some(letter) = &self.letter {
            let frame = &letter.frame;
            let at = |local: [f32; 2]| frame.at(local);
            let hovered = |target: LetterHit| hit == Some(Hit::Letter(target));
            layer.sprites.push((&assets.letter, frame.position, assets.letter.size));
            frame.draw_bare_title(layer, chrome, message(READ_TITLE), pointer.0);
            layer.texts.push((window::fit(font, &letter.mail.title, 272.0), at([14.0, 23.0]), TEXT));
            layer.texts.push((message(SENDER_LABEL).to_owned(), at([14.0, 45.0]), GREY));
            layer.texts.push((window::fit(font, &letter.mail.sender, 130.0), at([64.0, 45.0]), TEXT));
            let left = time_left(messages, letter.mail.expires, LEFT_LONG);
            layer.texts.push((left.clone(), at([286.0 - font.width(&left), 45.0]), GREY));
            for (row, line) in text_lines(&letter.text).into_iter().take(((TEXT_BOX[3] - 12.0) / LINE) as usize).enumerate() {
                layer.texts.push((line, at([TEXT_BOX[0] + 6.0, TEXT_BOX[1] + 6.0 + row as f32 * LINE]), TEXT));
            }
            let item_hit = match hit {
                Some(Hit::Letter(LetterHit::Item(slot))) => Some(slot),
                _ => None,
            };
            draw_items(layer, tooltip, frame, &letter.items, item_hit);
            if !letter.items.is_empty() {
                let image = &assets.receive[look(hovered(LetterHit::ClaimItems), pointer.1)];
                layer.sprites.push((image, at(CLAIM_ITEMS), image.size));
            }
            layer.texts.push((message(ZENY_LABEL).to_owned(), at([ZENY_BOX[0] + 8.0, ZENY_BOX[1] + 5.0]), GREY));
            let zeny = format!("{} z", group(letter.zeny));
            layer.texts.push((zeny.clone(), at([CLAIM_ZENY[0] - 6.0 - font.width(&zeny), ZENY_BOX[1] + 5.0]), TEXT));
            if letter.zeny > 0 {
                let image = &assets.receive[look(hovered(LetterHit::ClaimZeny), pointer.1)];
                layer.sprites.push((image, at(CLAIM_ZENY), image.size));
            }
            text_button(layer, frame, REPLY_BUTTON, message(REPLY_LABEL), hovered(LetterHit::Reply));
            text_button(layer, frame, DELETE_BUTTON, message(DELETE_LABEL), hovered(LetterHit::Delete));
        }

        if let Some(compose) = &self.compose {
            let frame = &compose.frame;
            let at = |local: [f32; 2]| frame.at(local);
            let hovered = |target: ComposeHit| hit == Some(Hit::Compose(target));
            layer.sprites.push((&assets.letter, frame.position, assets.letter.size));
            frame.draw_bare_title(layer, chrome, message(SEND_TITLE), pointer.0);
            let typed = |field: Field, text: &str| if caret && compose.field == Some(field) { format!("{text}|") } else { text.to_owned() };
            // The receiver's and title's fields, with their labels.
            for (label, field, place) in [(TO_LABEL, Field::To, TO_FIELD), (TITLE_LABEL, Field::Title, TITLE_FIELD)] {
                layer.texts.push((message(label).to_owned(), at([14.0, place[1] + 1.0]), GREY));
                layer.sprites.push((&assets.field, at([place[0] - 1.0, place[1] - 1.0]), [place[2] + 2.0, place[3] + 2.0]));
                layer.sprites.push((&chrome.white, at([place[0], place[1]]), [place[2], place[3]]));
                let text = typed(field, if field == Field::To { &compose.to } else { &compose.title });
                layer.texts.push((window::fit(font, &text, place[2] - 6.0), at([place[0] + 3.0, place[1] + 1.0]), TEXT));
            }
            text_button(layer, frame, VALIDATE, message(VALIDATE_LABEL), hovered(ComposeHit::Validate));
            // Whose the checked name is, in a balloon off the window's right side.
            if let Some((_, job, level)) = compose.receiver {
                let corner = at([LETTER_SIZE[0] - 6.0, 8.0]);
                layer.sprites.push((&assets.balloon, corner, assets.balloon.size));
                let job = job_names.get(&job).cloned().unwrap_or_else(|| job.to_string());
                layer.texts.push((window::fit(font, &format!("Lv. {level} {job}"), 110.0), offset(corner, [14.0, 13.0]), TEXT));
            }
            let lines = text_lines(&typed(Field::Text, &compose.text));
            let shown = ((TEXT_BOX[3] - 12.0) / LINE) as usize;
            for (row, line) in lines.iter().skip(lines.len().saturating_sub(shown)).enumerate() {
                layer.texts.push((line.clone(), at([TEXT_BOX[0] + 6.0, TEXT_BOX[1] + 6.0 + row as f32 * LINE]), TEXT));
            }
            let item_hit = match hit {
                Some(Hit::Compose(ComposeHit::Item(slot))) => Some(slot),
                _ => None,
            };
            draw_items(layer, tooltip, frame, &compose.items, item_hit);
            layer.texts.push((fill(message(2627), &[&compose.weight, &2000]), at([210.0, SLOT_TOP + 8.0]), GREY));
            layer.texts.push((message(ZENY_LABEL).to_owned(), at([ZENY_BOX[0] + 8.0, ZENY_BOX[1] + 5.0]), GREY));
            let zeny = typed(Field::Zeny, &compose.zeny);
            layer.texts.push((zeny.clone(), at([ZENY_BOX[0] + ZENY_BOX[2] - 10.0 - font.width(&zeny), ZENY_BOX[1] + 5.0]), TEXT));
            text_button(layer, frame, SEND_BUTTON, message(SEND_LABEL), hovered(ComposeHit::Send));
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_list<'a>(
        &self,
        layer: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        font: &Font,
        messages: &[String],
        hit: Option<Hit>,
        pointer: ([f32; 2], bool),
        caret: bool,
    ) {
        let list = &self.list;
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        let at = |local: [f32; 2]| list.frame.at(local);
        let hovered = |target: ListHit| hit == Some(Hit::List(target));
        layer.sprites.push((&assets.list, list.frame.position, assets.list.size));
        list.frame.draw_bare_title(layer, chrome, message(LIST_TITLE), pointer.0);
        for (images, place, target) in
            [(&assets.refresh, REFRESH, ListHit::Refresh), (&assets.write, WRITE, ListHit::Write), (&assets.search, SEARCH_BUTTON, ListHit::Search)]
        {
            let image = &images[look(hovered(target), pointer.1)];
            layer.sprites.push((image, at(place), image.size));
        }
        // What the search looks at, and what's typed for it.
        for (by_sender, (place, label)) in [false, true].into_iter().zip(SEARCH_BY.into_iter().zip([TITLE_LABEL, SENDER_LABEL])) {
            let check = &assets.check[usize::from(list.by_sender == by_sender)];
            layer.sprites.push((check, at(place), check.size));
            layer.texts.push((message(label).to_owned(), at([place[0] + 13.0, place[1] - 2.0]), TEXT));
        }
        let search = if list.typing && caret { format!("{}|", list.search) } else { list.search.clone() };
        layer.texts.push((window::fit(font, &search, SEARCH_FIELD[2] - 6.0), at([SEARCH_FIELD[0] + 3.0, SEARCH_FIELD[1] + 1.0]), TEXT));
        for (tab, &label) in TABS.iter().enumerate() {
            let corner = [TABS_AT[0] + tab as f32 * TAB[0], TABS_AT[1]];
            let image = &assets.tab[if tab == list.tab { 2 } else { usize::from(hovered(ListHit::Tab(tab))) }];
            layer.sprites.push((image, at(corner), image.size));
            let text = window::fit(font, message(label), TAB[0] - 6.0);
            let x = corner[0] + ((TAB[0] - font.width(&text)) / 2.0).floor();
            layer.texts.push((text, at([x, corner[1] + ((TAB[1] - font.line_height()) / 2.0).floor()]), if tab == list.tab { TEXT } else { GREY }));
        }
        // A page of mail: title, sender, what it holds, read or not, and time left.
        for (row, mail) in list.shown().into_iter().skip(list.page * ROWS).take(ROWS).enumerate() {
            let top = ROW_TOP + row as f32 * ROW;
            layer.texts.push((window::fit(font, &mail.title, 150.0), at([24.0, top + 9.0]), TEXT));
            layer.texts.push((window::fit(font, &mail.sender, 150.0), at([24.0, top + 25.0]), GREY));
            let holds = match (mail.flags & MAIL_ZENY != 0, mail.flags & MAIL_ITEM != 0) {
                (true, true) => Some(2),
                (false, true) => Some(1),
                (true, false) => Some(0),
                (false, false) => None,
            };
            if let Some(holds) = holds {
                layer.sprites.push((&assets.holds[holds], at([182.0, top + 10.0]), assets.holds[holds].size));
            }
            let status = &assets.status[if mail.kind == 2 { 2 } else { usize::from(!mail.read) }];
            layer.sprites.push((status, at([221.0, top + 12.0]), status.size));
            let left = time_left(messages, mail.expires, LEFT_SHORT);
            layer.texts.push((left.clone(), at([272.0 - (font.width(&left) / 2.0).floor(), top + 17.0]), GREY));
        }
        for (images, place, target) in [(&assets.prev, PREV, ListHit::Prev), (&assets.next, NEXT, ListHit::Next)] {
            let image = &images[look(hovered(target), pointer.1)];
            layer.sprites.push((image, at(place), image.size));
        }
        let page = format!("{} / {}", list.page + 1, list.pages());
        layer.texts.push((page.clone(), at([(PREV[0] + PAGE_BUTTON[0] + NEXT[0]) / 2.0 - (font.width(&page) / 2.0).floor(), PREV[1] + 5.0]), TEXT));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mail(id: u64, kind: u8, flags: u8, title: &str) -> MailHeader {
        MailHeader { id, kind, flags, title: title.to_owned(), sender: "Tester".to_owned(), ..Default::default() }
    }

    #[test]
    fn sorts_mail_into_tabs_and_searches() {
        let mut list = MailBox { mails: vec![mail(1, 0, 0, "Hello"), mail(2, 0, MAIL_NPC, "Notice"), mail(3, 2, 0, "Back")], ..Default::default() };
        assert_eq!(list.shown().iter().map(|mail| mail.id).collect::<Vec<_>>(), [1]);
        list.tab = 0;
        assert_eq!(list.shown().iter().map(|mail| mail.id).collect::<Vec<_>>(), [2]);
        list.tab = 2;
        assert_eq!(list.shown().iter().map(|mail| mail.id).collect::<Vec<_>>(), [3]);
        list.tab = TAB_SEARCH;
        list.search = "hel".to_owned();
        assert_eq!(list.shown().iter().map(|mail| mail.id).collect::<Vec<_>>(), [1]);
        list.by_sender = true;
        list.search = "tester".to_owned();
        assert_eq!(list.shown().len(), 3);
    }

    #[test]
    fn reads_claims_and_deletes() {
        let mut mail = Mail::default();
        assert_eq!(mail.toggle(), Action::Open);
        mail.event(MapEvent::MailList(vec![self::mail(7, 0, MAIL_ZENY, "Hello")]));
        assert_eq!(mail.click(Hit::List(ListHit::Row(0))), Some(Action::Read(0, 7)));
        mail.event(MapEvent::MailRead { kind: 0, id: 7, text: "Hi".to_owned(), zeny: 100, items: Vec::new() });
        assert_eq!(mail.click(Hit::Letter(LetterHit::Delete)), Some(Action::Say(CLAIM_FIRST)), "zeny still in it");
        assert_eq!(mail.event(MapEvent::MailClaimed { id: 7, zeny: true, result: 0 }), Some((2591, false)));
        assert_eq!(mail.list.mails[0].flags, 0);
        assert_eq!(mail.click(Hit::Letter(LetterHit::Delete)), Some(Action::Delete(0, 7)));
        mail.event(MapEvent::MailDeleted { id: 7 });
        assert!(mail.letter.is_none() && mail.list.mails.is_empty());
        assert_eq!(mail.toggle(), Action::Close);
    }

    #[test]
    fn writes_with_a_checked_name() {
        let mut mail = Mail::default();
        mail.event(MapEvent::MailWrite { to: String::new(), ok: true });
        mail.type_text("tester2");
        assert_eq!(mail.click(Hit::Compose(ComposeHit::Send)), Some(Action::Say(BAD_TITLE)));
        assert!(!mail.typing(), "a click off the fields ends typing");
        mail.click(Hit::Compose(ComposeHit::Field(Field::Title)));
        mail.type_text("Hello");
        mail.enter(true);
        mail.type_text("one");
        mail.enter(false);
        mail.type_text("two");
        let compose = mail.compose.as_ref().unwrap();
        assert_eq!((compose.to.as_str(), compose.title.as_str(), compose.text.as_str()), ("tester2", "Hello", "one\ntwo"));
        mail.event(MapEvent::MailReceiver { character: 150004, job: 5, level: 31, name: "Tester2".to_owned() });
        assert_eq!(mail.compose.as_ref().unwrap().receiver, Some((150004, 5, 31)));
        mail.backspace();
        assert_eq!(mail.compose.as_ref().unwrap().text, "one\ntw");
        mail.enter(true);
        mail.type_text("12a3");
        assert_eq!(mail.compose.as_ref().unwrap().zeny(), 123);
        assert_eq!(mail.click(Hit::Compose(ComposeHit::Send)), Some(Action::Send));
        assert_eq!(mail.event(MapEvent::MailSent(0)), Some((2587, false)));
        assert!(mail.compose.is_none());
    }
}
