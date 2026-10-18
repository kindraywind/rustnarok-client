//! The monster info window (Sense): a monster's level, HP, size, race, DEF, MDEF and element, then how much each
//! element's attacks hurt it. And the Convex Mirror's lines about the map's boss.

use rustnarok_client::net::MonsterInfo;
use rustnarok_client::text::Font;

use crate::window::{Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, SCREEN};

const TITLE: usize = 406;
/// Name, Level, HP, Size, Race, DEF, MDEF, Element.
const LABELS: [usize; 8] = [407, 408, 409, 410, 411, 270, 412, 413];
/// "Neutral" to "Undead", in rathena's element order.
const ELEMENTS: usize = 414;
const SIZES: usize = 443;
/// The navigation window's race names: "Formless", and "Undead" to "Dragon" for 1 to 9.
const FORMLESS: usize = 2285;
const RACES: usize = 2275;
/// The boss is back in some hours and minutes, or within a minute.
const BOSS_IN: usize = 1224;
const BOSS_SOON: usize = 1261;

// ponytail: data.grf has no picture for this window: a plain one, a label and a value a row; calibrate against the
// real client.
const WIDTH: f32 = 220.0;
const ROW: f32 = 16.0;
const TOP: f32 = 6.0;
const LABEL: f32 = 10.0;
const VALUE: f32 = 100.0;
const ROWS: usize = LABELS.len() + 9;
const HEIGHT: f32 = TITLE_HEIGHT + TOP * 2.0 + ROWS as f32 * ROW;

pub fn size_message(size: u16) -> usize {
    SIZES + usize::from(size.min(2))
}

// ponytail: the race labels are the navigation window's; the real window's own words aren't knowable.
pub fn race_message(race: u16) -> usize {
    if race == 0 { FORMLESS } else { RACES + usize::from(race.min(9)) }
}

pub fn element_message(element: u16) -> usize {
    ELEMENTS + usize::from(element.min(9))
}

/// The line for a boss that's dead: back in hours and minutes, or within a minute.
// ponytail: rathena rounds up a minute; at most one minute left reads as "within 1 minute".
pub fn boss_line(hours: u16, minutes: u16) -> usize {
    if hours == 0 && minutes <= 1 { BOSS_SOON } else { BOSS_IN }
}

pub struct MonsterWindow {
    pub frame: Frame,
    pub info: MonsterInfo,
    /// A unit on screen of that class gives the name; the packet has none.
    pub name: String,
}

impl MonsterWindow {
    pub fn new(info: MonsterInfo, name: String) -> Self {
        let mut frame = Frame::new([((SCREEN[0] - WIDTH) / 2.0).floor(), 140.0], WIDTH);
        frame.open = true;
        Self { frame, info, name }
    }

    /// Over the window: Some(true) on its close button.
    pub fn hit(&self, cursor: [f32; 2]) -> Option<bool> {
        let local = self.frame.local(cursor, HEIGHT)?;
        Some(self.frame.title_hit(local) == Some(TitleHit::Close))
    }

    pub fn height() -> f32 {
        HEIGHT
    }

    /// The rows: each label's message and its value.
    fn rows(&self, messages: &[String]) -> Vec<(String, String)> {
        let message = |id: usize| messages.get(id).map_or("", String::as_str).to_owned();
        let info = &self.info;
        let values = [
            self.name.clone(),
            info.level.to_string(),
            info.hp.to_string(),
            message(size_message(info.size)),
            message(race_message(info.race)),
            info.def.to_string(),
            info.mdef.to_string(),
            message(element_message(info.element)),
        ];
        let mut rows: Vec<(String, String)> = LABELS.iter().zip(values).map(|(&label, value)| (message(label), value)).collect();
        // Water to undead: how much each element's attacks hurt it.
        rows.extend(info.resists.iter().enumerate().map(|(element, percent)| (message(ELEMENTS + 1 + element), format!("{percent}%"))));
        rows
    }

    pub fn draw<'a>(&self, layer: &mut Layer<'a>, chrome: &'a Chrome, font: &Font, messages: &[String], pointer: [f32; 2]) {
        let frame = &self.frame;
        frame.draw_title(layer, chrome, messages.get(TITLE).map_or("", String::as_str), pointer);
        if frame.minimized {
            return;
        }
        let body = HEIGHT - TITLE_HEIGHT;
        layer.sprites.push((&chrome.white, frame.at([0.0, TITLE_HEIGHT]), [WIDTH, body]));
        layer.sprites.push((&chrome.border, frame.at([0.0, TITLE_HEIGHT]), [1.0, body]));
        layer.sprites.push((&chrome.border, frame.at([WIDTH - 1.0, TITLE_HEIGHT]), [1.0, body]));
        layer.sprites.push((&chrome.border, frame.at([0.0, HEIGHT - 1.0]), [WIDTH, 1.0]));
        for (row, (label, value)) in self.rows(messages).into_iter().enumerate() {
            let y = TITLE_HEIGHT + TOP + row as f32 * ROW;
            layer.texts.push((label, frame.at([LABEL, y]), [0x40, 0x40, 0x40]));
            layer.texts.push((crate::window::fit(font, &value, WIDTH - VALUE - 8.0), frame.at([VALUE, y]), [0, 0, 0]));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_what_sense_tells() {
        assert_eq!([0, 1, 2].map(size_message), [443, 444, 445]);
        assert_eq!([0, 1, 3, 9].map(race_message), [2285, 2276, 2278, 2284]);
        assert_eq!([0, 1, 9].map(element_message), [414, 415, 423]);
        assert_eq!([(0, 1), (0, 0), (0, 2), (1, 0)].map(|(hours, minutes)| boss_line(hours, minutes)), [1261, 1261, 1224, 1224]);
        // A Poring: the rows carry its values and the nine element percentages.
        let info =
            MonsterInfo { class: 1002, level: 1, size: 1, hp: 55, def: 2, race: 3, mdef: 5, element: 1, resists: [25, 100, 90, 175, 100, 100, 100, 100, 100] };
        let messages: Vec<String> = (0..2300).map(|id| format!("m{id}")).collect();
        let rows = MonsterWindow::new(info, "Poring".into()).rows(&messages);
        assert_eq!(rows.len(), 17);
        assert_eq!(rows[0], ("m407".into(), "Poring".into()));
        assert_eq!(rows[3], ("m410".into(), "m444".into()));
        assert_eq!(rows[4], ("m411".into(), "m2278".into()));
        assert_eq!(rows[8], ("m415".into(), "25%".into()));
        assert_eq!(rows[16], ("m423".into(), "100%".into()));
    }
}
