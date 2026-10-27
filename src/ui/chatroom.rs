//! Chat rooms: the sign over a room owner's head (title and how full), the room window once inside (talk on the left,
//! who's there on the right), and the window that makes one (Alt+C).

use rustnarok_client::grf::Grf;
use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};
use crate::window::{self, Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI, contains, offset, screen};

// ponytail: places are estimates; calibrate against the real client.
/// The board over an owner's head, and where its text starts inside.
pub const SIGN: [f32; 2] = [140.0, 34.0];
const SIGN_TEXT: [f32; 2] = [31.0, 8.0];
/// How far over the feet a shop board's bottom sits, in sprite pixels (the head height); a chat room's sits 17 screen
/// pixels lower.
pub const SIGN_ABOVE: f32 = 96.0;
const ROOM_DROP: f32 = 17.0;
/// A shop board: a white box over all but its bottom 6 px (yellow for the one picked), a grey line inset 2 inside it.
const SHOP_LINE: [u8; 4] = [0xC3, 0xC3, 0xC3, 0xFF];
const SHOP_PICKED: [u8; 4] = [0xFF, 0xFA, 0x7D, 0xFF];
const SHOP_BOTTOM: f32 = 6.0;
const SHOP_INSET: f32 = 2.0;
/// The icon's corner on both boards.
const ICON: [f32; 2] = [3.0, 3.0];
/// The chat room board: the system box's 14-px pieces, the edges tiled every 12 px from 12, the bottom row's top at 28.
const PIECE: f32 = 14.0;
const TILE: f32 = 12.0;
const BOTTOM_ROW: f32 = 28.0;
const ROOM_WIDTH: f32 = 279.0;
const ROOM_LIST: f32 = 120.0;
const ROOM_SEND: f32 = 30.0;
const MESSAGES_WIDTH: f32 = 195.0;
const LINE: f32 = 14.0;
const ROOM_POSITION: [f32; 2] = [100.0, 100.0];
pub const OWN_LINE: [u8; 3] = [0x31, 0x59, 0x37];
pub const JOIN_LINE: [u8; 3] = [0x41, 0x6D, 0x6C];
pub const LEAVE_LINE: [u8; 3] = [0x48, 0x1B, 0x22];
const OWNER: [u8; 3] = [0x36, 0x71, 0x6D];
const MAKE_SIZE: [f32; 2] = [280.0, 120.0];
const MAKE_FOOTER: f32 = 27.0;
const BUTTON: [f32; 2] = [42.0, 20.0];
/// Room sizes the limit choice cycles through.
const LIMITS: [u16; 5] = [20, 12, 8, 4, 2];
/// "Create Chat Room", "Private", "Public", "Chat Room".
const MAKE_TITLE: usize = 125;
/// "Room Setup": the owner's button, and the make window's title when it changes the room.
const ROOM_SETUP: usize = 126;
/// The owner's button in the room window, at the right of the line.
// ponytail: the button's look and place.
const SETUP_BUTTON: ([f32; 2], [f32; 2]) = ([ROOM_WIDTH - 82.0, TITLE_HEIGHT + ROOM_LIST + 5.0], [78.0, 20.0]);
const PRIVATE: usize = 200;
const PUBLIC: usize = 201;

/// A chat room seen over its owner's head.
#[derive(Debug, Clone, PartialEq)]
pub struct Sign {
    pub id: u32,
    pub limit: u16,
    pub users: u16,
    /// 0 needs a password, 1 is open, 2 an NPC's waiting room, 3 shows no count; SHOP is a vendor's.
    pub kind: u8,
    pub title: String,
}

/// A vendor's sign, not a room's.
pub const SHOP: u8 = 100;
/// A buying store's sign.
pub const BUYING_STORE: u8 = 101;

impl Sign {
    /// The words on the sign: the title and how full it is (a shop or buying store just its name).
    pub fn text(&self) -> String {
        if matches!(self.kind, 3 | SHOP | BUYING_STORE) { self.title.clone() } else { format!("{} ({}/{})", self.title, self.users, self.limit) }
    }

    /// A vendor's sign for a shop's name.
    pub fn shop(vendor: u32, title: String) -> Self {
        Self { id: vendor, limit: 0, users: 0, kind: SHOP, title }
    }

    /// A buying store's sign for a store's name.
    pub fn buying_store(owner: u32, title: String) -> Self {
        Self { id: owner, limit: 0, users: 0, kind: BUYING_STORE, title }
    }
}

pub struct Assets {
    /// Open and password-locked room icons, a vendor's shop, and a buying store.
    icons: [Texture; 4],
    white: Texture,
    /// A shop board's grey line, and the picked shop's yellow.
    line: Texture,
    picked: Texture,
    /// The chat room board's tail.
    tail: Texture,
    /// The system box: corners, edges (lu, mu, ru, lm, rm, ld, md, rd).
    pieces: [Texture; 8],
    ok: [Texture; 3],
    cancel: [Texture; 3],
    footer: Texture,
    radio: [Texture; 2],
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |name: &str| gpu.load_texture(grf, &format!("{UI}{name}.bmp"), false);
        Self {
            icons: [image("chat_open"), image("chat_close"), image("shop"), image("buyingshop")],
            white: gpu.solid([255; 4]),
            line: gpu.solid(SHOP_LINE),
            picked: gpu.solid(SHOP_PICKED),
            tail: image("sysbox_arr_l"),
            pieces: ["lu", "mu", "ru", "lm", "rm", "ld", "md", "rd"].map(|name| image(&format!("sysbox_{name}"))),
            ok: ["btn_ok", "btn_ok_a", "btn_ok_b"].map(image),
            cancel: ["btn_cancel", "btn_cancel_a", "btn_cancel_b"].map(image),
            footer: image("basic_interface\\btnbar_mid2"),
            radio: [image("radiobtn_off"), image("radiobtn_on")],
        }
    }
}

/// Whether a sign is a shop's board (a vendor's or a buying store's), not a chat room's.
fn is_shop(kind: u8) -> bool {
    matches!(kind, SHOP | BUYING_STORE)
}

/// The board's box for a unit whose head point (`SIGN_ABOVE` over its feet) is `head`: centred on it, a shop's bottom
/// there and a chat room's 17 lower.
pub fn sign_rect(kind: u8, head: [f32; 2]) -> [f32; 4] {
    let bottom = head[1] + if is_shop(kind) { 0.0 } else { ROOM_DROP };
    let corner = [(head[0] - SIGN[0] / 2.0).round(), (bottom - SIGN[1]).round()];
    [corner[0], corner[1], corner[0] + SIGN[0], corner[1] + SIGN[1]]
}

/// A shop board's pieces inside the board: the filled box, the grey line's four sides, the icon's corner.
fn shop_pieces() -> ([f32; 4], [[f32; 4]; 4], [f32; 2]) {
    let [w, h] = SIGN;
    let fill = [0.0, 0.0, w, h - SHOP_BOTTOM];
    let [x, y, lw, lh] = [SHOP_INSET, SHOP_INSET, w - 2.0 * SHOP_INSET, h - SHOP_BOTTOM - 2.0 * SHOP_INSET];
    let line = [[x, y, lw, 1.0], [x, y + lh - 1.0, lw, 1.0], [x, y, 1.0, lh], [x + lw - 1.0, y, 1.0, lh]];
    (fill, line, ICON)
}

/// Where the chat room board's tiled edge pieces go across, inside the board: every 12 px from 12 while a piece fits
/// before the right corner.
fn edge_tiles() -> Vec<f32> {
    let last = SIGN[0] - PIECE;
    (1..).map(|step| step as f32 * TILE).take_while(|&x| x < last).collect()
}

/// Draws a sign over the unit whose head point is `head`, and gives its box. A `picked` shop board is yellow.
pub fn draw_sign<'a>(layer: &mut Layer<'a>, assets: &'a Assets, font: &Font, sign: &Sign, head: [f32; 2], picked: bool) -> [f32; 4] {
    let rect = sign_rect(sign.kind, head);
    let [x, y] = [rect[0], rect[1]];
    let at = |local: [f32; 2]| [x + local[0], y + local[1]];
    let w = SIGN[0];
    if is_shop(sign.kind) {
        let (fill, line, _) = shop_pieces();
        layer.sprites.push((if picked { &assets.picked } else { &assets.white }, at([fill[0], fill[1]]), [fill[2], fill[3]]));
        for [lx, ly, lw, lh] in line {
            layer.sprites.push((&assets.line, at([lx, ly]), [lw, lh]));
        }
    } else {
        let [lu, mu, ru, lm, rm, ld, md, rd] = &assets.pieces;
        let piece = [PIECE; 2];
        // The middle band between the rows, then the rows over it; the tail over the bottom row's middle.
        layer.sprites.push((lm, at([0.0, PIECE]), [PIECE, BOTTOM_ROW - PIECE]));
        layer.sprites.push((&assets.white, at([PIECE, PIECE]), [w - 2.0 * PIECE, BOTTOM_ROW - PIECE]));
        layer.sprites.push((rm, at([w - PIECE, PIECE]), [PIECE, BOTTOM_ROW - PIECE]));
        for (row, [left, middle, right]) in [(0.0, [lu, mu, ru]), (BOTTOM_ROW, [ld, md, rd])] {
            for tile in edge_tiles() {
                layer.sprites.push((middle, at([tile, row]), piece));
            }
            layer.sprites.push((left, at([0.0, row]), piece));
            layer.sprites.push((right, at([w - PIECE, row]), piece));
        }
        layer.sprites.push((&assets.tail, at([((w - assets.tail.size[0]) / 2.0).floor(), BOTTOM_ROW]), assets.tail.size));
    }
    let icon = &assets.icons[match sign.kind {
        0 => 1,
        SHOP => 2,
        BUYING_STORE => 3,
        _ => 0,
    }];
    layer.sprites.push((icon, at(ICON), icon.size));
    layer.texts.push((shown_text(font, sign), at(SIGN_TEXT), [0, 0, 0]));
    rect
}

/// The words as the sign's text strip fits them, cut with "..." when too long.
// ponytail: the client's 12-px font for the board isn't ours; the text keeps the window font in its box.
fn shown_text(font: &Font, sign: &Sign) -> String {
    window::fit(font, &sign.text(), SIGN[0] - SIGN_TEXT[0] - 3.0)
}

/// The text strip of the sign in `rect`: (31, 8), 34 px narrower than the board, 15 high. With the pointer on it, cut
/// words show whole.
pub fn text_strip(rect: [f32; 4]) -> [f32; 4] {
    [rect[0] + SIGN_TEXT[0], rect[1] + SIGN_TEXT[1], rect[2] - rect[0] - 34.0, 15.0]
}

/// The sign's whole words, when its strip cuts them.
pub fn whole_text(font: &Font, sign: &Sign) -> Option<String> {
    let text = sign.text();
    (shown_text(font, sign) != text).then_some(text)
}

/// The room the player is in.
pub struct Room {
    pub frame: Frame,
    pub sign: Sign,
    /// Who's in, and whether they own it.
    pub members: Vec<(String, bool)>,
    pub lines: Vec<(String, [u8; 3])>,
    /// What's typed into its line.
    pub input: String,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum RoomHit {
    Title(TitleHit),
    /// The owner's Room Setup button.
    Setup,
    /// A name in the member list, by its row.
    Member(usize),
    Window,
}

impl Room {
    pub fn new(sign: Sign, members: Vec<(String, bool)>) -> Self {
        let mut frame = Frame::new(ROOM_POSITION, ROOM_WIDTH);
        frame.open = true;
        Self { frame, sign, members, lines: Vec::new(), input: String::new() }
    }

    pub fn height(&self) -> f32 {
        TITLE_HEIGHT + ROOM_LIST + ROOM_SEND
    }

    /// Whether `name` owns the room.
    pub fn owned_by(&self, name: &str) -> bool {
        self.members.iter().any(|(member, owner)| member == name && *owner)
    }

    /// What `cursor` is on; the setup button only for the owner (`me`).
    pub fn hit(&self, cursor: [f32; 2], me: &str) -> Option<RoomHit> {
        let local = self.frame.local(cursor, self.height())?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(RoomHit::Title(hit));
        }
        if self.owned_by(me) && contains(SETUP_BUTTON.0, SETUP_BUTTON.1, local) {
            return Some(RoomHit::Setup);
        }
        let row = ((local[1] - TITLE_HEIGHT - 5.0) / LINE).floor();
        if local[0] >= MESSAGES_WIDTH && row >= 0.0 && (row as usize) < self.members.len() && local[1] < TITLE_HEIGHT + ROOM_LIST {
            return Some(RoomHit::Member(row as usize));
        }
        Some(RoomHit::Window)
    }

    /// The owner's choices for member `row`: hand over, kick (none for the owner's own name or a non-owner).
    pub fn member_choices(&self, row: usize, me: &str) -> Option<String> {
        let (name, _) = self.members.get(row)?;
        (self.owned_by(me) && name != me).then(|| name.clone())
    }

    /// The setup button and the member rows on screen (for the test driver).
    pub fn button_boxes(&self, me: &str) -> Vec<(String, [f32; 4])> {
        if !self.frame.open || self.frame.minimized {
            return Vec::new();
        }
        let mut boxes = Vec::new();
        if self.owned_by(me) {
            let [x, y] = self.frame.at(SETUP_BUTTON.0);
            boxes.push(("room_setup".to_owned(), [x, y, SETUP_BUTTON.1[0], SETUP_BUTTON.1[1]]));
        }
        for (row, (name, _)) in self.members.iter().enumerate() {
            let [x, y] = self.frame.at([MESSAGES_WIDTH + 2.0, TITLE_HEIGHT + 5.0 + row as f32 * LINE]);
            boxes.push((format!("room_member_{name}"), [x, y, ROOM_WIDTH - MESSAGES_WIDTH - 4.0, LINE]));
        }
        boxes
    }

    pub fn draw<'a>(&self, layer: &mut Layer<'a>, chrome: &'a Chrome, font: &Font, messages: &[String], me: &str, pointer: [f32; 2], caret: bool) {
        let at = |local: [f32; 2]| self.frame.at(local);
        self.frame.draw_title(layer, chrome, &self.sign.text(), pointer);
        if self.frame.minimized {
            return;
        }
        let body = self.height() - TITLE_HEIGHT;
        layer.sprites.push((&chrome.white, at([0.0, TITLE_HEIGHT]), [ROOM_WIDTH, body]));
        layer.sprites.push((&chrome.border, at([MESSAGES_WIDTH, TITLE_HEIGHT]), [1.0, ROOM_LIST]));
        layer.sprites.push((&chrome.border, at([0.0, TITLE_HEIGHT + ROOM_LIST]), [ROOM_WIDTH, 1.0]));
        // The newest lines that fit, wrapped to the talk column.
        let rows = (ROOM_LIST / LINE) as usize - 1;
        let wrapped: Vec<_> = self
            .lines
            .iter()
            .flat_map(|(line, color)| window::wrap(font, line, MESSAGES_WIDTH - 8.0, usize::MAX).into_iter().map(move |piece| (piece, *color)))
            .collect();
        for (row, (line, color)) in wrapped.iter().skip(wrapped.len().saturating_sub(rows)).enumerate() {
            layer.texts.push((line.clone(), at([4.0, TITLE_HEIGHT + 5.0 + row as f32 * LINE]), *color));
        }
        for (row, (name, owner)) in self.members.iter().enumerate().take(rows) {
            let name = window::fit(font, name, ROOM_WIDTH - MESSAGES_WIDTH - 6.0);
            layer.texts.push((name, at([MESSAGES_WIDTH + 4.0, TITLE_HEIGHT + 5.0 + row as f32 * LINE]), if *owner { OWNER } else { [0, 0, 0] }));
        }
        let typed = if caret { format!("{}|", self.input) } else { self.input.clone() };
        layer.texts.push((typed, at([5.0, TITLE_HEIGHT + ROOM_LIST + ((ROOM_SEND - font.line_height()) / 2.0).floor()]), [0, 0, 0]));
        // The owner's Room Setup button: a framed label.
        if self.owned_by(me) {
            let ([x, y], [w, h]) = SETUP_BUTTON;
            for (corner, size) in [([x, y], [w, 1.0]), ([x, y + h - 1.0], [w, 1.0]), ([x, y], [1.0, h]), ([x + w - 1.0, y], [1.0, h])] {
                layer.sprites.push((&chrome.border, at(corner), size));
            }
            let label = messages.get(ROOM_SETUP).map_or("Room Setup", String::as_str);
            layer.texts.push((label.to_owned(), at([x + ((w - font.width(label)) / 2.0).round(), y + ((h - font.line_height()) / 2.0).floor()]), [0, 0, 0]));
        }
    }
}

/// The window making a room: title, size, open or with a password.
pub struct Make {
    pub title: String,
    limit: usize,
    pub public: bool,
    pub password: String,
    /// Typing goes to the password, not the title.
    pub on_password: bool,
    /// It changes the room the player owns, not a new one.
    pub editing: bool,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum MakeHit {
    Title,
    Limit,
    Public(bool),
    Password,
    Ok,
    Cancel,
    Window,
}

impl Default for Make {
    fn default() -> Self {
        Self { title: String::new(), limit: 0, public: true, password: String::new(), on_password: false, editing: false }
    }
}

impl Make {
    /// The window filled from the room's sign, to change it (the password isn't known: it starts empty).
    pub fn from_sign(sign: &Sign) -> Self {
        let limit = LIMITS.iter().position(|&limit| limit == sign.limit).unwrap_or(0);
        Self { title: sign.title.clone(), limit, public: sign.kind != 0, editing: true, ..Self::default() }
    }

    fn corner() -> [f32; 2] {
        [((screen()[0] - MAKE_SIZE[0]) / 2.0).floor(), ((screen()[1] - MAKE_SIZE[1]) / 2.0).floor()]
    }

    pub fn limit(&self) -> u16 {
        LIMITS[self.limit]
    }

    /// The rows: title, then the limit, then open or private and the password.
    fn places() -> [[f32; 2]; 3] {
        [[7.0, TITLE_HEIGHT + 6.0], [7.0, TITLE_HEIGHT + 28.0], [7.0, TITLE_HEIGHT + 50.0]]
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<MakeHit> {
        let local = offset(cursor, Self::corner().map(|value| -value));
        if !contains([0.0; 2], MAKE_SIZE, local) {
            return None;
        }
        let [title, limit, mode] = Self::places();
        let footer = MAKE_SIZE[1] - MAKE_FOOTER;
        // The title bar's close is Cancel.
        let close = Frame::new([0.0; 2], MAKE_SIZE[0]).close_button();
        Some(if contains(close, window::BUTTON_SIZE, local) {
            MakeHit::Cancel
        } else if contains([MAKE_SIZE[0] - 110.0, footer + 4.0], BUTTON, local) {
            MakeHit::Ok
        } else if contains([MAKE_SIZE[0] - 60.0, footer + 4.0], BUTTON, local) {
            MakeHit::Cancel
        } else if contains(offset(title, [60.0, 0.0]), [200.0, 16.0], local) {
            MakeHit::Title
        } else if contains(offset(limit, [60.0, 0.0]), [40.0, 16.0], local) {
            MakeHit::Limit
        } else if contains(offset(mode, [60.0, 0.0]), [50.0, 16.0], local) {
            MakeHit::Public(true)
        } else if contains(offset(mode, [110.0, 0.0]), [50.0, 16.0], local) {
            MakeHit::Public(false)
        } else if contains(offset(mode, [165.0, 0.0]), [95.0, 16.0], local) && !self.public {
            MakeHit::Password
        } else {
            MakeHit::Window
        })
    }

    /// A click; true when OK asks to make the room.
    pub fn click(&mut self, hit: MakeHit) -> bool {
        match hit {
            MakeHit::Title => self.on_password = false,
            MakeHit::Password => self.on_password = true,
            MakeHit::Limit => self.limit = (self.limit + 1) % LIMITS.len(),
            MakeHit::Public(public) => {
                self.public = public;
                self.on_password = !public;
            }
            MakeHit::Ok => return !self.title.trim().is_empty(),
            MakeHit::Cancel | MakeHit::Window => {}
        }
        false
    }

    pub fn type_text(&mut self, text: &str) {
        let (field, most) = if self.on_password { (&mut self.password, 8) } else { (&mut self.title, 32) };
        let room = most - field.chars().count().min(most);
        field.extend(text.chars().filter(|character| !character.is_control()).take(room));
    }

    pub fn backspace(&mut self) {
        if self.on_password {
            self.password.pop()
        } else {
            self.title.pop()
        };
    }

    pub fn draw<'a>(&self, layer: &mut Layer<'a>, chrome: &'a Chrome, assets: &'a Assets, messages: &[String], pointer: ([f32; 2], bool), caret: bool) {
        let corner = Self::corner();
        let at = |local: [f32; 2]| offset(corner, local);
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        let frame = {
            let mut frame = Frame::new(corner, MAKE_SIZE[0]);
            frame.open = true;
            frame
        };
        frame.draw_title(layer, chrome, message(if self.editing { ROOM_SETUP } else { MAKE_TITLE }), pointer.0);
        layer.sprites.push((&chrome.white, at([0.0, TITLE_HEIGHT]), [MAKE_SIZE[0], MAKE_SIZE[1] - TITLE_HEIGHT - MAKE_FOOTER]));
        let [title, limit, mode] = Self::places();
        let field = |layer: &mut Layer<'a>, place: [f32; 2], width: f32, text: String| {
            layer.sprites.push((&chrome.border, at(offset(place, [-1.0, -1.0])), [width + 2.0, 18.0]));
            layer.sprites.push((&chrome.white, at(place), [width, 16.0]));
            layer.texts.push((text, at(offset(place, [3.0, 1.0])), [0, 0, 0]));
        };
        let typed = |text: &str, on: bool| if on && caret { format!("{text}|") } else { text.to_owned() };
        layer.texts.push(("Title :".to_owned(), at(offset(title, [0.0, 1.0])), [0, 0, 0]));
        field(layer, offset(title, [60.0, 0.0]), 200.0, typed(&self.title, !self.on_password));
        layer.texts.push(("Limit :".to_owned(), at(offset(limit, [0.0, 1.0])), [0, 0, 0]));
        field(layer, offset(limit, [60.0, 0.0]), 40.0, self.limit().to_string());
        layer.texts.push((message(130).to_owned(), at(offset(limit, [115.0, 1.0])), [0, 0, 0]));
        for (public, x, label) in [(true, 60.0, PUBLIC), (false, 110.0, PRIVATE)] {
            let radio = &assets.radio[usize::from(self.public == public)];
            layer.sprites.push((radio, at(offset(mode, [x, 2.0])), radio.size));
            layer.texts.push((message(label).to_owned(), at(offset(mode, [x + 15.0, 1.0])), [0, 0, 0]));
        }
        if !self.public {
            field(layer, offset(mode, [165.0, 0.0]), 90.0, typed(&"*".repeat(self.password.chars().count()), self.on_password));
        }
        let footer = MAKE_SIZE[1] - MAKE_FOOTER;
        layer.sprites.push((&assets.footer, at([0.0, footer]), [MAKE_SIZE[0], MAKE_FOOTER]));
        let hit = self.hit(pointer.0);
        for (images, place, target) in
            [(&assets.ok, [MAKE_SIZE[0] - 110.0, footer + 4.0], MakeHit::Ok), (&assets.cancel, [MAKE_SIZE[0] - 60.0, footer + 4.0], MakeHit::Cancel)]
        {
            let look = if hit == Some(target) { 1 + usize::from(pointer.1) } else { 0 };
            layer.sprites.push((&images[look], at(place), BUTTON));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tells_a_cut_sign_its_whole_words() {
        let font = Font::load();
        assert_eq!(whole_text(&font, &Sign::shop(7, "Cheap pots".to_owned())), None);
        let long = Sign::shop(7, "Selling cheap Jellopy for every adve".to_owned());
        assert!(shown_text(&font, &long).ends_with("..."));
        assert_eq!(whole_text(&font, &long).as_deref(), Some(long.title.as_str()));
        // The strip on a board at (430, 266): (461, 274), 106 x 15.
        assert_eq!(text_strip([430.0, 266.0, 570.0, 300.0]), [461.0, 274.0, 106.0, 15.0]);
    }

    #[test]
    fn places_the_boards() {
        // A head point at (500, 300): both 140x34 and centred, a shop's bottom on it, a chat room's 17 lower.
        assert_eq!(sign_rect(SHOP, [500.0, 300.0]), [430.0, 266.0, 570.0, 300.0]);
        assert_eq!(sign_rect(BUYING_STORE, [500.0, 300.0]), [430.0, 266.0, 570.0, 300.0]);
        assert_eq!(sign_rect(1, [500.0, 300.0]), [430.0, 283.0, 570.0, 317.0]);
        assert_eq!(sign_rect(0, [500.5, 300.0]), [431.0, 283.0, 571.0, 317.0]);
        // A shop board: white over 140 x 28, the grey line 136 x 24 at (2, 2), the icon at (3, 3).
        let (fill, line, icon) = shop_pieces();
        assert_eq!(fill, [0.0, 0.0, 140.0, 28.0]);
        assert_eq!(line, [[2.0, 2.0, 136.0, 1.0], [2.0, 25.0, 136.0, 1.0], [2.0, 2.0, 1.0, 24.0], [137.0, 2.0, 1.0, 24.0]]);
        assert_eq!(icon, [3.0, 3.0]);
        // The chat room board's edges: tiles every 12 px from 12 up to the right corner at 126; the bottom row at 28.
        assert_eq!(edge_tiles(), (1..=10).map(|step| step as f32 * 12.0).collect::<Vec<_>>());
        assert_eq!(BOTTOM_ROW, SIGN[1] - 6.0);
    }

    #[test]
    fn gives_the_owner_its_tools() {
        let sign = Sign { id: 7, limit: 12, users: 2, kind: 0, title: "QA room".into() };
        let room = Room::new(sign.clone(), vec![("LaneA1".into(), true), ("LaneA2".into(), false)]);
        assert!(room.owned_by("LaneA1") && !room.owned_by("LaneA2"));
        // The owner's menu is for the others; the others have none.
        assert_eq!(room.member_choices(1, "LaneA1"), Some("LaneA2".into()));
        assert_eq!([room.member_choices(0, "LaneA1"), room.member_choices(0, "LaneA2")], [None, None]);
        // The setup button and the member rows hit; the button only for the owner.
        let boxes = room.button_boxes("LaneA1");
        let middle = |area: [f32; 4]| [area[0] + area[2] / 2.0, area[1] + area[3] / 2.0];
        assert_eq!(room.hit(middle(boxes[0].1), "LaneA1"), Some(RoomHit::Setup));
        assert_eq!(room.hit(middle(boxes[0].1), "LaneA2"), Some(RoomHit::Window));
        assert_eq!((boxes[2].0.as_str(), room.hit(middle(boxes[2].1), "LaneA1")), ("room_member_LaneA2", Some(RoomHit::Member(1))));
        // Room Setup starts from the room: its title, size and privacy.
        let make = Make::from_sign(&sign);
        assert_eq!((make.title.as_str(), make.limit(), make.public, make.editing), ("QA room", 12, false, true));
    }

    #[test]
    fn signs_count_and_makes_cycle() {
        let sign = Sign { id: 1, limit: 20, users: 3, kind: 1, title: "Hi".into() };
        assert_eq!(sign.text(), "Hi (3/20)");
        assert_eq!(Sign { kind: 3, ..sign }.text(), "Hi");
        assert_eq!(Sign::buying_store(7, "Buying Jellopy".into()).text(), "Buying Jellopy");
        let mut make = Make::default();
        make.type_text("Trade\u{8} room");
        assert_eq!(make.title, "Trade room");
        assert!(!make.click(MakeHit::Limit));
        assert_eq!(make.limit(), 12);
        make.click(MakeHit::Public(false));
        make.type_text("123456789");
        assert_eq!(make.password, "12345678");
        assert!(make.click(MakeHit::Ok));
        // The title bar's close is Cancel.
        let close = offset(Make::corner(), [MAKE_SIZE[0] - 14.0 + 5.0, 3.0 + 5.0]);
        assert_eq!(make.hit(close), Some(MakeHit::Cancel));
    }
}
