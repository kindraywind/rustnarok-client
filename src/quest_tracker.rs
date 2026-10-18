//! The quest tracker: active quests on screen under the minimap, each its title in gold, its summary, and how its
//! hunts stand, so the quest window needn't be opened.

use std::collections::HashMap;

use rustnarok_client::net::{Hunt, Quest};
use rustnarok_client::text::Font;

use crate::quest::QuestText;
use crate::{Layer, contains, screen};

/// The tracker's area: 250 by 400 at (W - 320, 150); no frame, no title.
pub fn corner() -> [f32; 2] {
    [screen()[0] - 320.0, 150.0]
}
pub const SIZE: [f32; 2] = [250.0, 400.0];
/// At most this many quests.
const MOST: usize = 5;
/// "Complete", for a hunt done.
const COMPLETE: usize = 2030;
const GOLD: [u8; 3] = [225, 214, 0];
const WHITE: [u8; 3] = [255, 255, 255];
const SHADOW: [u8; 3] = [0, 0, 0];
/// The summary keeps 30 bytes (as the game counts them, two for a Korean letter).
const SUMMARY_BYTES: usize = 30;

/// The quests it shows: active, in ID order, at most five.
pub fn tracked(quests: &[Quest]) -> Vec<&Quest> {
    let mut active: Vec<&Quest> = quests.iter().filter(|quest| quest.active).collect();
    active.sort_by_key(|quest| quest.id);
    active.truncate(MOST);
    active
}

/// Each block's top: 40 for one with no hunts shown (none, or more than three), 50 for one hunt, 80 for two or three.
pub fn block_tops(hunts: &[usize]) -> Vec<f32> {
    let mut top = 0.0;
    hunts
        .iter()
        .map(|&count| {
            let this = top;
            top += match count {
                1 => 50.0,
                2 | 3 => 80.0,
                _ => 40.0,
            };
            this
        })
        .collect()
}

/// The summary: its colour codes taken out, cut to 30 bytes (whole letters) with "..." when longer.
pub fn summary(text: &str) -> String {
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
    let bytes = |character: char| if character.is_ascii() { 1 } else { 2 };
    if plain.chars().map(bytes).sum::<usize>() <= SUMMARY_BYTES {
        return plain;
    }
    let mut cut = String::new();
    let mut used = 0;
    for character in plain.chars() {
        if used + bytes(character) > SUMMARY_BYTES {
            break;
        }
        used += bytes(character);
        cut.push(character);
    }
    cut + "..."
}

/// A hunt's line: its monster (with the level range), and how many of how many, or done; none when it needs none.
pub fn hunt_line(hunt: &Hunt, messages: &[String]) -> Option<String> {
    if hunt.needed == 0 {
        return None;
    }
    let name = match hunt.levels {
        [0, 0] => hunt.name.clone(),
        [0, most] => format!("{} Lv.{most}▼", hunt.name),
        [least, 0] => format!("{} Lv.{least}▲", hunt.name),
        [least, most] => format!("{} Lv.{least}~Lv.{most}", hunt.name),
    };
    Some(if hunt.count >= hunt.needed {
        format!("{name} ( {} )", messages.get(COMPLETE).map_or("", String::as_str))
    } else {
        format!("{name} ( {} / {} )", hunt.count, hunt.needed)
    })
}

/// A quest as the tracker draws it: its ID, title, summary and hunt lines (only with one to three hunts).
pub struct Block {
    pub id: u32,
    pub title: String,
    pub summary: String,
    pub hunts: Vec<String>,
    pub top: f32,
}

pub fn blocks(quests: &[Quest], texts: &HashMap<u32, QuestText>, messages: &[String]) -> Vec<Block> {
    let shown = tracked(quests);
    let tops = block_tops(&shown.iter().map(|quest| quest.hunts.len()).collect::<Vec<_>>());
    shown
        .into_iter()
        .zip(tops)
        .map(|(quest, top)| {
            let text = texts.get(&quest.id);
            let hunts =
                if (1..=3).contains(&quest.hunts.len()) { quest.hunts.iter().filter_map(|hunt| hunt_line(hunt, messages)).collect() } else { Vec::new() };
            Block {
                id: quest.id,
                title: text.map_or_else(|| quest.id.to_string(), |text| text.title.clone()),
                summary: text.map(|text| summary(&text.summary)).unwrap_or_default(),
                hunts,
                top,
            }
        })
        .collect()
}

/// Whether the pointer is in the tracker's area.
pub fn covers(cursor: [f32; 2]) -> bool {
    contains(corner(), SIZE, cursor)
}

/// The quest whose title is under the pointer.
pub fn title_at(blocks: &[Block], font: &Font, cursor: [f32; 2]) -> Option<u32> {
    blocks.iter().find(|block| contains([corner()[0], corner()[1] + block.top], [font.width(&block.title).max(1.0), 14.0], cursor)).map(|block| block.id)
}

pub fn draw(layer: &mut Layer<'_>, blocks: &[Block]) {
    let at = |x: f32, y: f32| [corner()[0] + x, corner()[1] + y];
    let mut shadowed = |text: &str, place: [f32; 2], color: [u8; 3]| {
        layer.texts.push((text.to_owned(), [place[0] + 1.0, place[1] + 1.0], SHADOW));
        layer.texts.push((text.to_owned(), place, color));
    };
    for block in blocks {
        shadowed(&block.title, at(0.0, block.top), GOLD);
        shadowed(&block.summary, at(0.0, block.top + 15.0), WHITE);
        for (row, line) in block.hunts.iter().enumerate() {
            shadowed(line, at(10.0, block.top + 28.0 + 13.0 * row as f32), WHITE);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hunt(count: u16, needed: u16, levels: [u16; 2]) -> Hunt {
        Hunt { id: 1, name: "Poring".into(), count, needed, levels }
    }

    #[test]
    fn stacks_blocks_by_their_hunts() {
        // No hunts, one, three, then more than three (none shown): 40, 50, 80, 40.
        assert_eq!(block_tops(&[0, 1, 3, 4, 2]), [0.0, 40.0, 90.0, 170.0, 210.0]);
    }

    #[test]
    fn keeps_five_active_quests_in_order() {
        let quest = |id, active| Quest { id, active, ..Default::default() };
        let quests = [quest(9, true), quest(3, true), quest(5, false), quest(1, true), quest(7, true), quest(8, true), quest(2, true)];
        assert_eq!(tracked(&quests).iter().map(|quest| quest.id).collect::<Vec<_>>(), [1, 2, 3, 7, 8]);
    }

    #[test]
    fn cuts_summaries_and_says_hunts() {
        assert_eq!(summary("^777777Bring^000000 apples"), "Bring apples");
        assert_eq!(summary("Bring ten jellopies to the old man in Payon"), "Bring ten jellopies to the old...");
        // Korean letters count two bytes: fifteen fit.
        assert_eq!(summary(&"가".repeat(16)), format!("{}...", "가".repeat(15)));
        let mut messages = vec![String::new(); 2031];
        messages[2030] = "Complete".into();
        assert_eq!(hunt_line(&hunt(3, 10, [0, 0]), &messages).unwrap(), "Poring ( 3 / 10 )");
        assert_eq!(hunt_line(&hunt(10, 10, [0, 0]), &messages).unwrap(), "Poring ( Complete )");
        assert_eq!(hunt_line(&hunt(0, 5, [0, 20]), &messages).unwrap(), "Poring Lv.20▼ ( 0 / 5 )");
        assert_eq!(hunt_line(&hunt(0, 5, [10, 0]), &messages).unwrap(), "Poring Lv.10▲ ( 0 / 5 )");
        assert_eq!(hunt_line(&hunt(0, 5, [10, 20]), &messages).unwrap(), "Poring Lv.10~Lv.20 ( 0 / 5 )");
        assert_eq!(hunt_line(&hunt(0, 0, [0, 0]), &messages), None);
    }
}
