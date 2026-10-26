//! The party booking board: the level range, purpose and roles to post or search with, a note, the posts found (each
//! with Join), and our own post's Delete. The posts live on rathena's web server (`rustnarok_client::booking`).

use rustnarok_client::booking::{PURPOSES, Post};
use rustnarok_client::text::Font;

use crate::Layer;
use crate::window::{self, Chrome, Frame, TITLE_HEIGHT, TitleHit};

// ponytail: the window is ours over the client's chrome; the purposes' and roles' words are plain English.
const WIDTH: f32 = 340.0;
const ROW: f32 = 18.0;
const ROWS: usize = 8;
const LEFT: f32 = 8.0;
const LEVELS_TOP: f32 = TITLE_HEIGHT + 6.0;
const ROLES_TOP: f32 = LEVELS_TOP + ROW + 4.0;
const MEMO_TOP: f32 = ROLES_TOP + ROW + 4.0;
const BUTTONS_TOP: f32 = MEMO_TOP + ROW + 4.0;
const POSTS_TOP: f32 = BUTTONS_TOP + ROW + 8.0;
const STATUS_TOP: f32 = POSTS_TOP + ROWS as f32 * ROW + 4.0;
const HEIGHT: f32 = STATUS_TOP + ROW + 4.0;
/// The two level fields, the purpose, the roles and the buttons along their rows.
const LEVEL_FIELDS: [[f32; 2]; 2] = [[LEFT + 24.0, 40.0], [LEFT + 80.0, 40.0]];
const PURPOSE: [f32; 2] = [LEFT + 140.0, 120.0];
const ROLE_WIDTH: f32 = 70.0;
const BUTTON_WIDTH: f32 = 70.0;
const JOIN_WIDTH: f32 = 40.0;
/// The most a note and a level take.
const MEMO_MOST: usize = 40;
const LEVEL_MOST: u16 = 275;
const TEXT: [u8; 3] = [0, 0, 0];
const GREY: [u8; 3] = [0x80, 0x80, 0x80];
const ON: [u8; 3] = [0x30, 0x50, 0xC0];

pub const PURPOSE_NAMES: [&str; PURPOSES] = ["Any", "Quest", "Field", "Dungeon", "Memorial", "Paradise", "Other"];
pub const ROLE_NAMES: [&str; 4] = ["Tank", "Healer", "Dealer", "Assist"];
const BUTTONS: [&str; 3] = ["Search", "Post", "Delete"];

/// A field that takes typing.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Field {
    MinLevel,
    MaxLevel,
    Memo,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(TitleHit),
    Field(Field),
    Purpose,
    Role(usize),
    Search,
    Post,
    Delete,
    /// A post's row, or its Join.
    Row(usize),
    Join(usize),
    Window,
}

/// What a click asks of the board.
#[derive(Debug, PartialEq, Clone)]
pub enum Action {
    Search {
        min_level: u16,
        max_level: u16,
        purpose: u16,
        roles: [bool; 4],
    },
    /// Our post, without whose it is (the game fills that in).
    Post(Post),
    Delete,
    /// Ask this post's leader to let us join.
    Join(Post),
    Close,
}

pub struct BookingWindow {
    pub frame: Frame,
    pub levels: [String; 2],
    pub purpose: u16,
    pub roles: [bool; 4],
    pub memo: String,
    pub focus: Option<Field>,
    /// The posts found, and a line about the last request.
    pub posts: Vec<Post>,
    pub status: String,
    /// Our account, so our own post shows no Join.
    me: u32,
}

impl BookingWindow {
    pub fn new(me: u32, level: u16) -> Self {
        // ponytail: where it opens, and the level range it starts with (the player's level to the most).
        let mut frame = Frame::new([340.0, 140.0], WIDTH);
        frame.open = true;
        Self {
            frame,
            levels: [level.max(1).to_string(), LEVEL_MOST.to_string()],
            purpose: 0,
            roles: [false; 4],
            memo: String::new(),
            focus: None,
            posts: Vec::new(),
            status: String::new(),
            me,
        }
    }

    fn level(&self, which: usize) -> u16 {
        self.levels[which].parse::<u16>().unwrap_or(1).clamp(1, LEVEL_MOST)
    }

    /// The level range typed, the lower first.
    pub fn level_range(&self) -> (u16, u16) {
        let (a, b) = (self.level(0), self.level(1));
        (a.min(b), a.max(b))
    }

    fn role_box(index: usize) -> ([f32; 2], [f32; 2]) {
        ([LEFT + index as f32 * (ROLE_WIDTH + 6.0), ROLES_TOP], [ROLE_WIDTH, ROW])
    }

    fn button_box(index: usize) -> ([f32; 2], [f32; 2]) {
        ([LEFT + index as f32 * (BUTTON_WIDTH + 8.0), BUTTONS_TOP], [BUTTON_WIDTH, ROW])
    }

    fn join_box(row: usize) -> ([f32; 2], [f32; 2]) {
        ([WIDTH - LEFT - JOIN_WIDTH, POSTS_TOP + row as f32 * ROW], [JOIN_WIDTH, ROW])
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let local = self.frame.local(cursor, HEIGHT)?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(Hit::Title(hit));
        }
        let on = |(corner, size): ([f32; 2], [f32; 2])| crate::contains(corner, size, local);
        let fields = [(Field::MinLevel, LEVEL_FIELDS[0]), (Field::MaxLevel, LEVEL_FIELDS[1])];
        if let Some((field, _)) = fields.into_iter().find(|(_, [x, w])| on(([*x, LEVELS_TOP], [*w, ROW]))) {
            return Some(Hit::Field(field));
        }
        if on(([PURPOSE[0], LEVELS_TOP], [PURPOSE[1], ROW])) {
            return Some(Hit::Purpose);
        }
        if let Some(role) = (0..4).find(|&role| on(Self::role_box(role))) {
            return Some(Hit::Role(role));
        }
        if on(([LEFT, MEMO_TOP], [WIDTH - 2.0 * LEFT, ROW])) {
            return Some(Hit::Field(Field::Memo));
        }
        if let Some(button) = (0..BUTTONS.len()).find(|&button| on(Self::button_box(button))) {
            return Some([Hit::Search, Hit::Post, Hit::Delete][button]);
        }
        if let Some(row) = (0..self.posts.len().min(ROWS)).find(|&row| on(Self::join_box(row))) {
            return Some(if self.posts[row].account == self.me { Hit::Row(row) } else { Hit::Join(row) });
        }
        if (POSTS_TOP..POSTS_TOP + ROWS as f32 * ROW).contains(&local[1]) {
            let row = ((local[1] - POSTS_TOP) / ROW) as usize;
            if row < self.posts.len() {
                return Some(Hit::Row(row));
            }
        }
        Some(Hit::Window)
    }

    pub fn click(&mut self, hit: Hit) -> Option<Action> {
        self.focus = match hit {
            Hit::Field(field) => Some(field),
            _ => None,
        };
        let (min_level, max_level) = self.level_range();
        match hit {
            Hit::Title(TitleHit::Close) => return Some(Action::Close),
            Hit::Title(title) => self.frame.click_title(title),
            Hit::Purpose => self.purpose = (self.purpose + 1) % PURPOSES as u16,
            Hit::Role(role) => self.roles[role] = !self.roles[role],
            Hit::Search => return Some(Action::Search { min_level, max_level, purpose: self.purpose, roles: self.roles }),
            Hit::Post => {
                return Some(Action::Post(Post { purpose: self.purpose, roles: self.roles, min_level, max_level, memo: self.memo.clone(), ..Post::default() }));
            }
            Hit::Delete => return Some(Action::Delete),
            Hit::Join(row) => return self.posts.get(row).cloned().map(Action::Join),
            Hit::Field(_) | Hit::Row(_) | Hit::Window => {}
        }
        None
    }

    /// Typing into the field with the caret: digits into a level, text into the note.
    pub fn type_text(&mut self, text: &str) {
        match self.focus {
            Some(Field::Memo) => {
                let room = MEMO_MOST.saturating_sub(self.memo.chars().count());
                self.memo.extend(text.chars().filter(|character| !character.is_control()).take(room));
            }
            Some(field) => {
                let level = &mut self.levels[usize::from(field == Field::MaxLevel)];
                let room = 3usize.saturating_sub(level.len());
                level.extend(text.chars().filter(char::is_ascii_digit).take(room));
            }
            None => {}
        }
    }

    pub fn backspace(&mut self) {
        match self.focus {
            Some(Field::Memo) => drop(self.memo.pop()),
            Some(field) => drop(self.levels[usize::from(field == Field::MaxLevel)].pop()),
            None => {}
        }
    }

    /// The fields, toggles, buttons and Join buttons on screen (for the test driver).
    pub fn button_boxes(&self) -> Vec<(String, [f32; 4])> {
        if !self.frame.open || self.frame.minimized {
            return Vec::new();
        }
        let area = |(corner, size): ([f32; 2], [f32; 2])| {
            let [x, y] = self.frame.at(corner);
            [x, y, size[0], size[1]]
        };
        let mut boxes = vec![
            ("booking_min".to_owned(), area(([LEVEL_FIELDS[0][0], LEVELS_TOP], [LEVEL_FIELDS[0][1], ROW]))),
            ("booking_max".to_owned(), area(([LEVEL_FIELDS[1][0], LEVELS_TOP], [LEVEL_FIELDS[1][1], ROW]))),
            ("booking_purpose".to_owned(), area(([PURPOSE[0], LEVELS_TOP], [PURPOSE[1], ROW]))),
            ("booking_memo".to_owned(), area(([LEFT, MEMO_TOP], [WIDTH - 2.0 * LEFT, ROW]))),
        ];
        boxes.extend(ROLE_NAMES.iter().enumerate().map(|(role, name)| (format!("booking_role_{}", name.to_lowercase()), area(Self::role_box(role)))));
        boxes.extend(BUTTONS.iter().enumerate().map(|(button, name)| (format!("booking_{}", name.to_lowercase()), area(Self::button_box(button)))));
        boxes.extend(
            self.posts
                .iter()
                .take(ROWS)
                .enumerate()
                .filter(|(_, post)| post.account != self.me)
                .map(|(row, post)| (format!("booking_join:{}", post.name), area(Self::join_box(row)))),
        );
        boxes
    }

    pub fn draw<'a>(&self, layer: &mut Layer<'a>, chrome: &'a Chrome, font: &Font, pointer: [f32; 2], caret: bool) {
        if !self.frame.open {
            return;
        }
        let at = |local: [f32; 2]| self.frame.at(local);
        self.frame.draw_title(layer, chrome, "Party Booking", pointer);
        if self.frame.minimized {
            return;
        }
        layer.sprites.push((&chrome.white, at([0.0, TITLE_HEIGHT]), [WIDTH, HEIGHT - TITLE_HEIGHT]));
        for x in [0.0, WIDTH - 1.0] {
            layer.sprites.push((&chrome.border, at([x, TITLE_HEIGHT]), [1.0, HEIGHT - TITLE_HEIGHT]));
        }
        layer.sprites.push((&chrome.border, at([0.0, HEIGHT - 1.0]), [WIDTH, 1.0]));
        let framed = |layer: &mut Layer<'a>, corner: [f32; 2], size: [f32; 2], text: String, color: [u8; 3]| {
            let place = at(corner);
            layer.sprites.push((&chrome.border, place, size));
            layer.sprites.push((&chrome.white, [place[0] + 1.0, place[1] + 1.0], [size[0] - 2.0, size[1] - 2.0]));
            layer.texts.push((text, [place[0] + 3.0, place[1] + 2.0], color));
        };
        let typed = |field: Field, text: &str| if self.focus == Some(field) && caret { format!("{text}_") } else { text.to_owned() };
        layer.texts.push(("Lv".to_owned(), at([LEFT, LEVELS_TOP + 2.0]), TEXT));
        framed(layer, [LEVEL_FIELDS[0][0], LEVELS_TOP], [LEVEL_FIELDS[0][1], ROW], typed(Field::MinLevel, &self.levels[0]), TEXT);
        layer.texts.push(("~".to_owned(), at([LEVEL_FIELDS[0][0] + LEVEL_FIELDS[0][1] + 4.0, LEVELS_TOP + 2.0]), TEXT));
        framed(layer, [LEVEL_FIELDS[1][0], LEVELS_TOP], [LEVEL_FIELDS[1][1], ROW], typed(Field::MaxLevel, &self.levels[1]), TEXT);
        framed(layer, [PURPOSE[0], LEVELS_TOP], [PURPOSE[1], ROW], PURPOSE_NAMES[usize::from(self.purpose)].to_owned(), ON);
        for (role, name) in ROLE_NAMES.iter().enumerate() {
            let (corner, size) = Self::role_box(role);
            let mark = if self.roles[role] { "■" } else { "□" };
            framed(layer, corner, size, format!("{mark} {name}"), if self.roles[role] { ON } else { GREY });
        }
        let memo = if self.memo.is_empty() && self.focus != Some(Field::Memo) { "Note".to_owned() } else { typed(Field::Memo, &self.memo) };
        framed(layer, [LEFT, MEMO_TOP], [WIDTH - 2.0 * LEFT, ROW], memo, if self.memo.is_empty() { GREY } else { TEXT });
        for (button, name) in BUTTONS.iter().enumerate() {
            let (corner, size) = Self::button_box(button);
            framed(layer, corner, size, (*name).to_owned(), TEXT);
        }
        for (row, post) in self.posts.iter().take(ROWS).enumerate() {
            let top = POSTS_TOP + row as f32 * ROW;
            let line = format!(
                "{}  Lv{}~{}  {}  {}",
                post.name,
                post.min_level,
                post.max_level,
                PURPOSE_NAMES.get(usize::from(post.purpose)).unwrap_or(&""),
                post.memo
            );
            let room = WIDTH - 2.0 * LEFT - JOIN_WIDTH - 4.0;
            layer.texts.push((window::fit(font, &line, room), at([LEFT, top + 2.0]), if post.account == self.me { ON } else { TEXT }));
            if post.account != self.me {
                let (corner, size) = Self::join_box(row);
                framed(layer, corner, size, "Join".to_owned(), ON);
            }
        }
        layer.texts.push((window::fit(font, &self.status, WIDTH - 2.0 * LEFT), at([LEFT, STATUS_TOP + 2.0]), GREY));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_and_asks_the_board() {
        let mut window = BookingWindow::new(7, 40);
        let middle = |window: &BookingWindow, name: &str| {
            let [x, y, w, h] = window.button_boxes().into_iter().find(|(box_name, _)| box_name == name).unwrap().1;
            [x + w / 2.0, y + h / 2.0]
        };
        // The levels: typed digits, the range kept in order.
        let field = window.hit(middle(&window, "booking_min")).unwrap();
        window.click(field);
        window.backspace();
        window.backspace();
        window.type_text("9x9");
        assert_eq!(window.levels[0], "99");
        window.focus = Some(Field::MaxLevel);
        window.levels[1].clear();
        window.type_text("50");
        assert_eq!(window.level_range(), (50, 99));
        window.click(Hit::Purpose);
        window.click(Hit::Purpose);
        window.click(window.hit(middle(&window, "booking_role_healer")).unwrap());
        window.click(Hit::Field(Field::Memo));
        window.type_text(&"n".repeat(60));
        assert_eq!(window.memo.len(), MEMO_MOST);
        let search = window.click(window.hit(middle(&window, "booking_search")).unwrap());
        assert_eq!(search, Some(Action::Search { min_level: 50, max_level: 99, purpose: 2, roles: [false, true, false, false] }));
        let Some(Action::Post(post)) = window.click(Hit::Post) else { panic!() };
        assert_eq!((post.purpose, post.min_level, post.max_level, post.memo.len()), (2, 50, 99, MEMO_MOST));
        // Others' posts have Join; ours doesn't.
        window.posts = vec![Post { account: 7, name: "Me".into(), ..Post::default() }, Post { account: 8, name: "Bob".into(), ..Post::default() }];
        let names: Vec<String> = window.button_boxes().into_iter().map(|(name, _)| name).filter(|name| name.starts_with("booking_join")).collect();
        assert_eq!(names, ["booking_join:Bob"]);
        let join = window.click(window.hit(middle(&window, "booking_join:Bob")).unwrap());
        assert_eq!(join, Some(Action::Join(window.posts[1].clone())));
        assert_eq!(window.click(Hit::Title(TitleHit::Close)), Some(Action::Close));
    }
}
