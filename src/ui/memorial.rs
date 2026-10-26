//! The memorial dungeon window: the waiting line, then the two time limits and Destroy.
//!
//! rathena `clif_instance_create` / `_changewait` / `_status` / `_changestatus`. Destroy sends
//! CZ_MEMORIALDUNGEON_COMMAND 3 only after the message box's yes.

use rustnarok_client::messages;
use rustnarok_client::text::Font;

use crate::status_tip;
use crate::window::{Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, contains, screen};

pub const TITLE: usize = 1336;
/// "%s in Standby".
pub const STANDBY: usize = 1337;
/// "%s in Progress".
pub const PROGRESS: usize = 1339;
/// "Your Standby Priority: ^ff0000%d^000000".
pub const PLACE: usize = 1342;
/// Entry time: "The requested dungeon will be removed if you do not enter within ^ff0000%s^000000."
pub const ENTRY: usize = 1343;
pub const MISSION: usize = 1344;
pub const LIVE_EXPIRED: usize = 1346;
pub const ENTER_EXPIRED: usize = 1347;
pub const REMOVED: usize = 1348;
// ponytail: rathena's create-failure (notify 4) has no id in the spec. 1323 is the table's "Failed to create
// Memorial Dungeon, '%s'."
pub const CREATE_FAIL: usize = 1323;
pub const DESTROY: usize = 3270;
// ponytail: no msgstringtable id was given for the destroy question. The button's own label is the question.
pub const DESTROY_ASK: usize = 3270;

/// rathena `e_instance_notify`. 0 updates the idle limit and leaves the window up.
pub const IN_NOTIFY: u32 = 0;
pub const IN_DESTROY_LIVE_TIMEOUT: u32 = 1;
pub const IN_DESTROY_ENTER_TIMEOUT: u32 = 2;
pub const IN_DESTROY_USER_REQUEST: u32 = 3;
pub const IN_CREATE_FAIL: u32 = 4;

const WIDTH: f32 = 280.0;
const WAIT_HEIGHT: f32 = 72.0;
const PROGRESS_HEIGHT: f32 = 160.0;
const EMPTY_HEIGHT: f32 = 36.0;
const LINE: f32 = 16.0;
const TEXT_X: f32 = 8.0;
const TEXT_TOP: f32 = TITLE_HEIGHT + 6.0;
const BUTTON: [f32; 2] = [120.0, 20.0];
const TEXT: [u8; 3] = [0, 0, 0];
/// "hour" / "minute" / "second" (newer data: "%d hour(s)" and so on). The status tooltip uses the same lines.
const UNITS: [usize; 3] = [1697, 1807, 1808];

/// Seconds from `now` until a Unix `deadline`. 0 once the deadline has passed.
pub fn seconds_left(deadline: u32, now: u64) -> u64 {
    u64::from(deadline).saturating_sub(now)
}

/// The countdown the window shows for a Unix `deadline`.
// ponytail: the memorial window's exact digits are not in the message table. This is the status tooltip's
// hour/minute/second lines, and 0 still says the seconds.
pub fn countdown_text(messages: &[String], deadline: u32, now: u64) -> String {
    let left = seconds_left(deadline, now);
    if left == 0 {
        return unit_text(messages, 0, UNITS[2]);
    }
    status_tip::time_text(messages, left)
}

fn unit_text(messages: &[String], value: u64, id: usize) -> String {
    let template = messages.get(id).map_or("", String::as_str);
    let text = if template.contains("%d") {
        messages::fill(template, &[&value])
    } else if template.is_empty() {
        value.to_string()
    } else {
        format!("{value} {}", template.trim())
    };
    text.trim().to_owned()
}

/// `^RRGGBB` taken out. Dialog lines drawn through `message_lines` drop these the same way.
pub fn plain(text: &str) -> String {
    let mut plain = String::new();
    let mut rest = text;
    while !rest.is_empty() {
        if let Some(code) =
            rest.strip_prefix('^').filter(|code| code.len() >= 6 && code.is_char_boundary(6) && code[..6].chars().all(|digit| digit.is_ascii_hexdigit()))
        {
            rest = &code[6..];
            continue;
        }
        let character = rest.chars().next().unwrap();
        plain.push(character);
        rest = &rest[character.len_utf8()..];
    }
    plain
}

/// The chat line for a closing notify, if it closes. Create failure is 1323 (`%s` is the name).
pub fn notify_message(kind: u32) -> Option<usize> {
    match kind {
        IN_DESTROY_LIVE_TIMEOUT => Some(LIVE_EXPIRED),
        IN_DESTROY_ENTER_TIMEOUT => Some(ENTER_EXPIRED),
        IN_DESTROY_USER_REQUEST => Some(REMOVED),
        IN_CREATE_FAIL => Some(CREATE_FAIL),
        _ => None,
    }
}

/// Notify 1 through 4 close the window. 0 only updates the idle limit.
pub fn closes(kind: u32) -> bool {
    notify_message(kind).is_some()
}

/// Unix time, for the countdown. The deadlines in the packets are Unix times.
pub fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |time| time.as_secs())
}

/// One memorial dungeon. `place` while waiting; both deadlines once it is made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Instance {
    pub name: String,
    pub place: Option<u16>,
    pub time_limit: Option<u32>,
    pub idle_limit: Option<u32>,
}

impl Instance {
    pub fn waiting(name: String, place: u16) -> Self {
        Self { name, place: Some(place), time_limit: None, idle_limit: None }
    }

    pub fn made(name: String, time_limit: u32, idle_limit: u32) -> Self {
        Self { name, place: None, time_limit: Some(time_limit), idle_limit: Some(idle_limit) }
    }

    /// 0x02CD has arrived: the timers and Destroy show.
    pub fn made_up(&self) -> bool {
        self.time_limit.is_some()
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(TitleHit),
    Destroy,
    Window,
}

pub struct Window {
    pub frame: Frame,
    pub instance: Option<Instance>,
}

impl Default for Window {
    fn default() -> Self {
        Self { frame: Frame::new([((screen()[0] - WIDTH) / 2.0).floor(), 160.0], WIDTH), instance: None }
    }
}

impl Window {
    pub fn height(&self) -> f32 {
        match &self.instance {
            Some(instance) if instance.made_up() => PROGRESS_HEIGHT,
            Some(_) => WAIT_HEIGHT,
            None => EMPTY_HEIGHT,
        }
    }

    pub fn covers(&self, cursor: [f32; 2]) -> bool {
        self.frame.local(cursor, self.height()).is_some()
    }

    fn destroy_at() -> [f32; 2] {
        [((WIDTH - BUTTON[0]) / 2.0).floor(), PROGRESS_HEIGHT - BUTTON[1] - 8.0]
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let local = self.frame.local(cursor, self.height())?;
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(Hit::Title(hit));
        }
        if self.instance.as_ref().is_some_and(Instance::made_up) && contains(Self::destroy_at(), BUTTON, local) {
            return Some(Hit::Destroy);
        }
        Some(Hit::Window)
    }

    /// The Destroy button, for the test driver. Empty unless the instance is made and the window is up.
    pub fn button_boxes(&self) -> Vec<(String, [f32; 4])> {
        if !self.frame.open || self.frame.minimized || !self.instance.as_ref().is_some_and(Instance::made_up) {
            return Vec::new();
        }
        let [x, y] = self.frame.at(Self::destroy_at());
        vec![("destroy".to_owned(), [x, y, BUTTON[0], BUTTON[1]])]
    }

    pub fn draw<'a>(&self, layer: &mut Layer<'a>, chrome: &'a Chrome, font: &Font, messages: &[String], pointer: [f32; 2], now: u64) {
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        let at = |local: [f32; 2]| self.frame.at(local);
        self.frame.draw_title(layer, chrome, message(TITLE), pointer);
        if self.frame.minimized {
            return;
        }
        let height = self.height();
        layer.sprites.push((&chrome.white, at([0.0, TITLE_HEIGHT]), [WIDTH, height - TITLE_HEIGHT]));
        for x in [0.0, WIDTH - 1.0] {
            layer.sprites.push((&chrome.border, at([x, TITLE_HEIGHT]), [1.0, height - TITLE_HEIGHT]));
        }
        layer.sprites.push((&chrome.border, at([0.0, height - 1.0]), [WIDTH, 1.0]));
        // ponytail: no message id was given for the empty window, so the body stays blank.
        let Some(instance) = &self.instance else { return };
        let mut row = 0.0;
        let line = |layer: &mut Layer<'a>, text: String, row: f32| {
            layer.texts.push((text, at([TEXT_X, TEXT_TOP + row * LINE]), TEXT));
        };
        if instance.made_up() {
            line(layer, plain(&messages::fill(message(PROGRESS), &[&instance.name])), row);
            row += 1.0;
            line(layer, message(MISSION).to_owned(), row);
            row += 1.0;
            if let Some(deadline) = instance.time_limit {
                line(layer, countdown_text(messages, deadline, now), row);
                row += 1.0;
            }
            if let Some(deadline) = instance.idle_limit {
                let entry = plain(&messages::fill(message(ENTRY), &[&countdown_text(messages, deadline, now)]));
                for wrapped in wrap(font, &entry, WIDTH - TEXT_X * 2.0) {
                    line(layer, wrapped, row);
                    row += 1.0;
                }
            }
            let corner = Self::destroy_at();
            layer.sprites.push((&chrome.white, at(corner), BUTTON));
            layer.sprites.push((&chrome.border, at(corner), [BUTTON[0], 1.0]));
            layer.sprites.push((&chrome.border, at([corner[0], corner[1] + BUTTON[1] - 1.0]), [BUTTON[0], 1.0]));
            layer.sprites.push((&chrome.border, at(corner), [1.0, BUTTON[1]]));
            layer.sprites.push((&chrome.border, at([corner[0] + BUTTON[0] - 1.0, corner[1]]), [1.0, BUTTON[1]]));
            let label = message(DESTROY);
            let label_x = corner[0] + ((BUTTON[0] - font.width(label)) / 2.0).max(4.0);
            // ponytail: the official Destroy bitmap isn't identified, so a framed label stands in, with no hover look.
            layer.texts.push((label.to_owned(), at([label_x, corner[1] + 3.0]), TEXT));
            return;
        }
        line(layer, plain(&messages::fill(message(STANDBY), &[&instance.name])), row);
        if let Some(place) = instance.place {
            line(layer, plain(&messages::fill(message(PLACE), &[&place])), row + 1.0);
        }
    }
}

fn wrap(font: &Font, text: &str, width: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_inclusive(' ') {
        let next = format!("{line}{word}");
        if !line.is_empty() && font.width(&next) > width {
            lines.push(line.trim_end().to_owned());
            line = word.trim_start().to_owned();
        } else {
            line = next;
        }
    }
    if !line.trim().is_empty() {
        lines.push(line.trim_end().to_owned());
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn messages() -> Vec<String> {
        let mut messages = vec![String::new(); 3271];
        messages[1697] = "%d hour(s)".into();
        messages[1807] = " %d minute(s)".into();
        messages[1808] = " %d second(s)".into();
        messages[PLACE] = "Your Standby Priority: ^ff0000%d^000000".into();
        messages[ENTRY] = "within ^ff0000%s^000000.".into();
        messages
    }

    #[test]
    fn counts_down_to_a_deadline() {
        let messages = messages();
        assert_eq!(seconds_left(5_000, 1_275), 3_725);
        assert_eq!(countdown_text(&messages, 5_000, 1_275), "1 hour(s) 2 minute(s) 5 second(s)");
        assert_eq!(countdown_text(&messages, 1_000, 1_000), "0 second(s)");
        assert_eq!(countdown_text(&messages, 1_000, 1_050), "0 second(s)");
        let mut older = messages.clone();
        older[1697] = "hour".into();
        older[1807] = "minute".into();
        older[1808] = "second".into();
        assert_eq!(countdown_text(&older, 4_660, 1_000), "1 hour 1 minute");
        assert_eq!(countdown_text(&older, 30, 30), "0 second");
    }

    #[test]
    fn drops_colour_codes_and_names_the_closing_lines() {
        assert_eq!(plain("Your Standby Priority: ^ff000012^000000"), "Your Standby Priority: 12");
        assert_eq!(plain("a^12zz not a code"), "a^12zz not a code");
        assert_eq!([0, 1, 2, 3, 4, 9].map(notify_message), [None, Some(LIVE_EXPIRED), Some(ENTER_EXPIRED), Some(REMOVED), Some(CREATE_FAIL), None]);
        assert!(!closes(IN_NOTIFY));
        assert!(closes(IN_CREATE_FAIL));
    }

    #[test]
    fn hits_destroy_only_once_the_instance_is_made() {
        let mut window = Window::default();
        window.frame.open = true;
        let at = |window: &Window, local: [f32; 2]| window.frame.at(local);
        assert_eq!(window.hit(at(&window, [20.0, 8.0])), Some(Hit::Title(TitleHit::Title)));
        window.instance = Some(Instance::waiting("Endless Tower".into(), 3));
        assert_eq!(window.hit(at(&window, [20.0, 40.0])), Some(Hit::Window));
        // Destroy sits below the shorter waiting window.
        assert_eq!(window.hit(at(&window, Window::destroy_at())), None);
        assert!(window.button_boxes().is_empty());
        window.instance = Some(Instance::made("Endless Tower".into(), 2_000, 1_300));
        assert_eq!(window.hit(at(&window, Window::destroy_at())), Some(Hit::Destroy));
        assert_eq!(window.button_boxes().len(), 1);
        assert_eq!(window.button_boxes()[0].0, "destroy");
        window.frame.minimized = true;
        assert!(window.button_boxes().is_empty());
        assert_eq!(window.hit(at(&window, Window::destroy_at())), None);
    }
}
