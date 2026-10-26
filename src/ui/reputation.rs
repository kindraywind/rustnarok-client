//! The reputation window: the player's standing with each faction, by group, from the client's content tables
//! (data\contentdata\repute\*.bson) and the points the server sends (ZC_REPUTE_INFO).

use std::collections::HashMap;

use rustnarok_client::bson::{self, Value};
use rustnarok_client::grf::Grf;
use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};
use crate::window::{Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI};

/// "Reputation Status".
const TITLE: usize = 3827;
/// Lights each side of the middle: a level every fifth of the most points that side.
// ponytail: the tables give each reputation's most points but no levels; five a side is an estimate.
pub const LIGHTS: i64 = 5;

// ponytail: layout over the client's chrome; reputation\bg_info.bmp and the lights are the data's.
const WIDTH: f32 = 300.0;
const LEFT: f32 = 10.0;
const GROUP_ROW: f32 = 20.0;
const ROW: f32 = 36.0;
const LIGHT: f32 = 12.0;
const LIGHTS_LEFT: f32 = 150.0;
const BAR: [f32; 2] = [2.0 * LIGHTS as f32 * LIGHT, 3.0];
const BOTTOM: f32 = 8.0;
const TEXT: [u8; 3] = [0, 0, 0];
const GREY: [u8; 3] = [0x80, 0x80, 0x80];
const BAR_FILL: [u8; 4] = [0x40, 0x80, 0xE0, 0xFF];
const BAR_BACK: [u8; 4] = [0xD8, 0xD8, 0xD8, 0xFF];

/// When a reputation shows (the table's `Invisible`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shown {
    Always,
    Never,
    /// Once it has points.
    WithPoints,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Repute {
    pub id: u64,
    pub name: String,
    pub most_negative: i64,
    pub most_positive: i64,
    pub shown: Shown,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub name: String,
    pub reputes: Vec<u64>,
}

/// The client's reputation tables: groups in order, reputations by id.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Tables {
    pub groups: Vec<Group>,
    pub reputes: HashMap<u64, Repute>,
}

impl Tables {
    pub fn load(grf: &Grf) -> Self {
        let read = |name: &str| grf.read(&format!("data\\contentdata\\repute\\{name}.bson")).ok().and_then(|bytes| bson::parse(&bytes).ok());
        match (read("reputegroupdata"), read("reputeinfodata")) {
            (Some(groups), Some(infos)) => Self::from_values(&groups, &infos),
            _ => Self::default(),
        }
    }

    /// From reputegroupdata's `ReputeGroup` and reputeinfodata's `reputeInfo`, each keyed by number.
    pub fn from_values(groups: &Value, infos: &Value) -> Self {
        let number = |key: &str| key.parse::<u64>().ok();
        let mut ordered: Vec<(u64, Group)> = groups
            .get("ReputeGroup")
            .map(Value::entries)
            .unwrap_or_default()
            .iter()
            .filter_map(|(key, group)| {
                let reputes = group.get("ReputeList")?.entries().iter().filter_map(|(_, id)| u64::try_from(id.as_int()?).ok()).collect();
                Some((number(key)?, Group { name: group.get("Name").and_then(Value::as_str).unwrap_or_default().to_owned(), reputes }))
            })
            .collect();
        ordered.sort_by_key(|(key, _)| *key);
        let reputes = infos
            .get("reputeInfo")
            .map(Value::entries)
            .unwrap_or_default()
            .iter()
            .filter_map(|(key, info)| {
                let id = number(key)?;
                let most = |field: &str| info.get(field).and_then(Value::as_int).unwrap_or(0);
                let shown = match info.get("Invisible").and_then(Value::as_str) {
                    Some("VISIBLE_FALSE") => Shown::Never,
                    Some("VISIBLE_EXIST") => Shown::WithPoints,
                    _ => Shown::Always,
                };
                let name = info.get("Name").and_then(Value::as_str).unwrap_or_default().to_owned();
                Some((id, Repute { id, name, most_negative: most("MaxPoint_Negative"), most_positive: most("MaxPoint_Positive"), shown }))
            })
            .collect();
        Self { groups: ordered.into_iter().map(|(_, group)| group).collect(), reputes }
    }
}

/// The level for `points`: -LIGHTS..=LIGHTS, a level each fifth of the most that side (toward zero), and how far the
/// points are to the next level away from zero (0..1; 1 at the most).
pub fn level(points: i64, most_negative: i64, most_positive: i64) -> (i64, f32) {
    let most = if points < 0 { most_negative } else { most_positive }.max(1);
    let step = (most / LIGHTS).max(1);
    let size = points.abs().min(most);
    let level = (size / step).min(LIGHTS);
    let share = if level == LIGHTS { 1.0 } else { (size % step) as f32 / step as f32 };
    (if points < 0 { -level } else { level }, share)
}

/// The rows shown: each group with a reputation to show, and its reputations with their points.
pub fn rows<'a>(tables: &'a Tables, points: &HashMap<u64, i64>) -> Vec<(&'a Group, Vec<(&'a Repute, i64)>)> {
    tables
        .groups
        .iter()
        .filter_map(|group| {
            let reputes: Vec<(&Repute, i64)> = group
                .reputes
                .iter()
                .filter_map(|id| tables.reputes.get(id))
                .map(|repute| (repute, points.get(&repute.id).copied().unwrap_or(0)))
                .filter(|(repute, points)| match repute.shown {
                    Shown::Always => true,
                    Shown::Never => false,
                    Shown::WithPoints => *points != 0,
                })
                .collect();
            (!reputes.is_empty()).then_some((group, reputes))
        })
        .collect()
}

pub struct Assets {
    background: Texture,
    blue: Texture,
    red: Texture,
    empty: Texture,
    bar_fill: Texture,
    bar_back: Texture,
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |name: &str| gpu.load_texture(grf, &format!("{UI}reputation\\{name}.bmp"), false);
        Self {
            background: image("bg_info"),
            blue: image("light_blue"),
            red: image("light_red"),
            empty: image("light_empty"),
            bar_fill: gpu.solid(BAR_FILL),
            bar_back: gpu.solid(BAR_BACK),
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(TitleHit),
    Window,
}

pub struct Window {
    pub frame: Frame,
    /// How tall the rows shown make it (set by `refresh`).
    height: f32,
}

impl Window {
    pub fn new(tables: &Tables, points: &HashMap<u64, i64>) -> Self {
        let mut frame = Frame::new([360.0, 120.0], WIDTH);
        frame.open = true;
        Self { frame, height: Self::height(tables, points) }
    }

    /// Fits the window to the rows shown, after the points change.
    pub fn refresh(&mut self, tables: &Tables, points: &HashMap<u64, i64>) {
        self.height = Self::height(tables, points);
    }

    fn height(tables: &Tables, points: &HashMap<u64, i64>) -> f32 {
        let rows = rows(tables, points);
        TITLE_HEIGHT + 6.0 + rows.iter().map(|(_, reputes)| GROUP_ROW + reputes.len() as f32 * ROW).sum::<f32>() + BOTTOM
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let local = self.frame.local(cursor, self.height)?;
        Some(self.frame.title_hit(local).map_or(Hit::Window, Hit::Title))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &self,
        layer: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        font: &Font,
        messages: &[String],
        tables: &Tables,
        points: &HashMap<u64, i64>,
        pointer: [f32; 2],
    ) {
        if !self.frame.open {
            return;
        }
        let at = |local: [f32; 2]| self.frame.at(local);
        self.frame.draw_title(layer, chrome, messages.get(TITLE).map_or("Reputation Status", String::as_str), pointer);
        if self.frame.minimized {
            return;
        }
        let height = self.height;
        layer.sprites.push((&chrome.white, at([0.0, TITLE_HEIGHT]), [WIDTH, height - TITLE_HEIGHT]));
        for x in [0.0, WIDTH - 1.0] {
            layer.sprites.push((&chrome.border, at([x, TITLE_HEIGHT]), [1.0, height - TITLE_HEIGHT]));
        }
        layer.sprites.push((&chrome.border, at([0.0, height - 1.0]), [WIDTH, 1.0]));
        let mut y = TITLE_HEIGHT + 6.0;
        for (group, reputes) in rows(tables, points) {
            layer.texts.push((group.name.clone(), at([LEFT, y + 3.0]), GREY));
            y += GROUP_ROW;
            for (repute, value) in reputes {
                layer.sprites.push((&assets.background, at([LEFT - 4.0, y]), [WIDTH - 2.0 * (LEFT - 4.0), ROW - 4.0]));
                layer.texts.push((crate::window::fit(font, &repute.name, LIGHTS_LEFT - LEFT - 6.0), at([LEFT, y + 3.0]), TEXT));
                let most = if value < 0 { repute.most_negative } else { repute.most_positive };
                layer.texts.push((format!("{value} / {}{most}", if value < 0 { "-" } else { "" }), at([LEFT, y + 18.0]), GREY));
                // Five lights a side of the middle: red ones lit out to the left for a bad standing, blue to the
                // right for a good one.
                let (level, share) = level(value, repute.most_negative, repute.most_positive);
                for light in 0..2 * LIGHTS {
                    let side = light - LIGHTS;
                    let texture = if level < 0 && side < 0 && side >= level {
                        &assets.red
                    } else if level > 0 && side >= 0 && side < level {
                        &assets.blue
                    } else {
                        &assets.empty
                    };
                    layer.sprites.push((texture, at([LIGHTS_LEFT + light as f32 * LIGHT, y + 4.0]), [LIGHT - 2.0, LIGHT - 2.0]));
                }
                // The bar: how far to the next light.
                let bar = at([LIGHTS_LEFT, y + 4.0 + LIGHT + 4.0]);
                layer.sprites.push((&assets.bar_back, bar, BAR));
                layer.sprites.push((&assets.bar_fill, bar, [(BAR[0] * share).round(), BAR[1]]));
                y += ROW;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(value: &str) -> Value {
        Value::Text(value.into())
    }

    fn tables() -> Tables {
        let list = |ids: &[i64]| Value::Document(ids.iter().enumerate().map(|(index, id)| (index.to_string(), Value::Int(*id))).collect());
        let group = |name: &str, ids: &[i64]| Value::Document(vec![("Name".into(), text(name)), ("ReputeList".into(), list(ids))]);
        let groups =
            Value::Document(vec![("ReputeGroup".into(), Value::Document(vec![("2".into(), group("Second", &[3])), ("1".into(), group("Monsters", &[1, 2]))]))]);
        let info = |name: &str, most: i64, shown: &str| {
            Value::Document(vec![
                ("Invisible".into(), text(shown)),
                ("MaxPoint_Negative".into(), Value::Int(most)),
                ("MaxPoint_Positive".into(), Value::Int(most)),
                ("Name".into(), text(name)),
            ])
        };
        let infos = Value::Document(vec![(
            "reputeInfo".into(),
            Value::Document(vec![
                ("1".into(), info("Orcs", 3000, "VISIBLE_TRUE")),
                ("2".into(), info("Goblins", 3000, "VISIBLE_FALSE")),
                ("3".into(), info("Wolves", 5000, "VISIBLE_EXIST")),
            ]),
        )]);
        Tables::from_values(&groups, &infos)
    }

    #[test]
    fn reads_the_tables() {
        let tables = tables();
        assert_eq!(tables.groups.iter().map(|group| group.name.as_str()).collect::<Vec<_>>(), ["Monsters", "Second"]);
        assert_eq!(tables.groups[0].reputes, [1, 2]);
        assert_eq!(tables.reputes[&3], Repute { id: 3, name: "Wolves".into(), most_negative: 5000, most_positive: 5000, shown: Shown::WithPoints });
    }

    #[test]
    fn levels_from_points() {
        // A level every 600 of 3000: 0, part way, a level, the most, and past it.
        assert_eq!(level(0, 3000, 3000), (0, 0.0));
        assert_eq!(level(300, 3000, 3000), (0, 0.5));
        assert_eq!(level(1500, 3000, 3000), (2, 0.5));
        assert_eq!(level(3000, 3000, 3000), (5, 1.0));
        assert_eq!(level(9999, 3000, 3000), (5, 1.0));
        // Below zero the levels go the other way, by the negative side's most.
        assert_eq!(level(-1200, 3000, 6000), (-2, 0.0));
    }

    #[test]
    fn shows_what_the_tables_allow() {
        let tables = tables();
        let mut points = HashMap::new();
        // Goblins never show; Wolves only with points, so the second group is left out until then.
        let shown = |points: &HashMap<u64, i64>| {
            rows(&tables, points)
                .iter()
                .map(|(group, reputes)| (group.name.clone(), reputes.iter().map(|(repute, value)| (repute.id, *value)).collect()))
                .collect::<Vec<(String, Vec<(u64, i64)>)>>()
        };
        assert_eq!(shown(&points), [("Monsters".to_owned(), vec![(1, 0)])]);
        points.insert(3, -250);
        points.insert(1, 700);
        assert_eq!(shown(&points), [("Monsters".to_owned(), vec![(1, 700)]), ("Second".to_owned(), vec![(3, -250)])]);
    }
}
