//! Chat rooms: the sign over a room owner's head (title and how full), the room window once inside (talk on the left,
//! who's there on the right), and the window that makes one (Alt+C).

use rustnarok_client::grf::Grf;
use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};
use crate::window::{self, Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, SCREEN, UI, contains, offset};

// ponytail: places are estimates; calibrate against the real client.
/// The board over an owner's head, and where its text starts inside.
pub const SIGN: [f32; 2] = [140.0, 34.0];
const SIGN_TEXT: [f32; 2] = [31.0, 8.0];
/// How far over the feet a shop board's bottom sits, in sprite pixels (the head height); a chat room's sits 17 screen
/// pixels lower.
pub const SIGN_ABOVE: f32 = 96.0;
const ROOM_DROP: f32 = 17.0;
/// A shop board: grey frame, white, and a yellow band inside.
const SHOP_FRAME: [u8; 4] = [0xC3, 0xC3, 0xC3, 0xFF];
const SHOP_BAND: [u8; 4] = [0xFF, 0xFA, 0x7D, 0xFF];
/// The system box's corner and edge pieces, and the chat room board's tail under its middle.
const PIECE: f32 = 14.0;
const TAIL: usize = 6;
const TAIL_EDGE: [u8; 4] = [0x8C, 0x8C, 0x8C, 0xFF];
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
    frame: Texture,
    band: Texture,
    tail_edge: Texture,
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
            frame: gpu.solid(SHOP_FRAME),
            band: gpu.solid(SHOP_BAND),
            tail_edge: gpu.solid(TAIL_EDGE),
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

/// The tail's rows under a chat room board with its box at `rect`: each row's left, top and width, narrowing to a
/// point under the middle.
// ponytail: the tail's size and look; the data has no bitmap for it.
fn tail_rows(rect: [f32; 4]) -> Vec<[f32; 3]> {
    let middle = ((rect[0] + rect[2]) / 2.0).floor();
    (0..TAIL).map(|row| [middle - (TAIL - 1 - row) as f32, rect[3] - 1.0 + row as f32, (2 * (TAIL - row) - 1) as f32]).collect()
}

/// Draws a sign over the unit whose head point is `head`, and gives its box.
pub fn draw_sign<'a>(layer: &mut Layer<'a>, assets: &'a Assets, font: &Font, sign: &Sign, head: [f32; 2]) -> [f32; 4] {
    let rect = sign_rect(sign.kind, head);
    let [x, y] = [rect[0], rect[1]];
    let [w, h] = SIGN;
    if is_shop(sign.kind) {
        // ponytail: the band's place: inside a white margin of 2 within the frame.
        layer.sprites.push((&assets.frame, [x, y], SIGN));
        layer.sprites.push((&assets.white, [x + 1.0, y + 1.0], [w - 2.0, h - 2.0]));
        layer.sprites.push((&assets.band, [x + 3.0, y + 3.0], [w - 6.0, h - 6.0]));
    } else {
        let [lu, mu, ru, lm, rm, ld, md, rd] = &assets.pieces;
        let middle = [w - 2.0 * PIECE, h - 2.0 * PIECE];
        layer.sprites.push((&assets.white, [x + PIECE, y + PIECE], middle));
        layer.sprites.push((lu, [x, y], [PIECE; 2]));
        layer.sprites.push((mu, [x + PIECE, y], [middle[0], PIECE]));
        layer.sprites.push((ru, [x + w - PIECE, y], [PIECE; 2]));
        layer.sprites.push((lm, [x, y + PIECE], [PIECE, middle[1]]));
        layer.sprites.push((rm, [x + w - PIECE, y + PIECE], [PIECE, middle[1]]));
        layer.sprites.push((ld, [x, y + h - PIECE], [PIECE; 2]));
        layer.sprites.push((md, [x + PIECE, y + h - PIECE], [middle[0], PIECE]));
        layer.sprites.push((rd, [x + w - PIECE, y + h - PIECE], [PIECE; 2]));
        for [left, top, width] in tail_rows(rect) {
            layer.sprites.push((&assets.tail_edge, [left, top], [width, 1.0]));
            if width > 2.0 {
                layer.sprites.push((&assets.white, [left + 1.0, top], [width - 2.0, 1.0]));
            }
        }
    }
    let icon = &assets.icons[match sign.kind {
        0 => 1,
        SHOP => 2,
        BUYING_STORE => 3,
        _ => 0,
    }];
    // ponytail: the icon's place, left of the text and centred down the board.
    layer.sprites.push((icon, [x + 3.0, y + ((h - icon.size[1]) / 2.0).floor()], icon.size));
    let text = window::fit(font, &sign.text(), w - SIGN_TEXT[0] - 4.0);
    layer.texts.push((text, [x + SIGN_TEXT[0], y + SIGN_TEXT[1]], [0, 0, 0]));
    rect
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

    pub fn hit(&self, cursor: [f32; 2]) -> Option<RoomHit> {
        let local = self.frame.local(cursor, self.height())?;
        Some(self.frame.title_hit(local).map_or(RoomHit::Window, RoomHit::Title))
    }

    pub fn draw<'a>(&self, layer: &mut Layer<'a>, chrome: &'a Chrome, font: &Font, pointer: [f32; 2], caret: bool) {
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
        Self { title: String::new(), limit: 0, public: true, password: String::new(), on_password: false }
    }
}

impl Make {
    fn corner() -> [f32; 2] {
        [((SCREEN[0] - MAKE_SIZE[0]) / 2.0).floor(), ((SCREEN[1] - MAKE_SIZE[1]) / 2.0).floor()]
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
        Some(if contains([MAKE_SIZE[0] - 110.0, footer + 4.0], BUTTON, local) {
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
        frame.draw_title(layer, chrome, message(MAKE_TITLE), pointer.0);
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
    fn places_the_boards() {
        // A head point at (500, 300): both 140x34 and centred, a shop's bottom on it, a chat room's 17 lower.
        assert_eq!(sign_rect(SHOP, [500.0, 300.0]), [430.0, 266.0, 570.0, 300.0]);
        assert_eq!(sign_rect(BUYING_STORE, [500.0, 300.0]), [430.0, 266.0, 570.0, 300.0]);
        assert_eq!(sign_rect(1, [500.0, 300.0]), [430.0, 283.0, 570.0, 317.0]);
        assert_eq!(sign_rect(0, [500.5, 300.0]), [431.0, 283.0, 571.0, 317.0]);
        // The tail narrows to one pixel under the middle, from the box's last row down.
        let tail = tail_rows([430.0, 283.0, 570.0, 317.0]);
        assert_eq!(tail.first(), Some(&[495.0, 316.0, 11.0]));
        assert_eq!(tail.last(), Some(&[500.0, 321.0, 1.0]));
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
    }
}
