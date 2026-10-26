//! The interface for external plugins (an auto hunt, say), accessibility tools and automated debugging, in a client
//! shared with an agent (mcp.rs). The vanilla client has no auto hunt and shouldn't get one: hunt logic belongs in the
//! plugin, a program of its own that plays over the control socket, so changing it needs no client build. Here is what a
//! plugin reads (`hunt_state`, `hunt_gat`), what it does (`hunt_do`: the game's own actions, so no packets of its own),
//! and the news a state can't show: a typed `/hunt`, a monster's hit on the player, a skill the server refused, a monster's
//! cast at the player or at a cell near it.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use rustnarok_client::lua::ItemInfo;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::chat::Kind;
use crate::{Game, MONSTER, SkillTarget, control, net};

const HUNT_LINE: [u8; 3] = [200, 170, 255];
/// News kept for the hunt program between two polls; more is dropped.
const MOST_NEWS: usize = 500;

struct News {
    items: Vec<Value>,
    polled: Option<Instant>,
}

static NEWS: Mutex<News> = Mutex::new(News { items: Vec::new(), polled: None });

/// Whether the hunt program polled lately (news is kept only then).
fn listening(news: &News) -> bool {
    news.polled.is_some_and(|at| at.elapsed() < Duration::from_secs(5))
}

fn tell(item: Value) {
    let mut news = NEWS.lock().unwrap();
    if listening(&news) && news.items.len() < MOST_NEWS {
        news.items.push(item);
    }
}

fn is_hunt(text: &str) -> bool {
    text == "/hunt" || text.starts_with("/hunt ")
}

/// A typed `/hunt` line, when the client is shared: passed on to the hunt program. Whether it was one.
pub fn command(game: &mut Game, text: &str) -> bool {
    if !crate::mcp::shared() || !is_hunt(text) {
        return false;
    }
    let mut news = NEWS.lock().unwrap();
    if listening(&news) {
        news.items.push(json!({"kind": "command", "text": text}));
    } else {
        drop(news);
        say(game, "no hunt program is running");
    }
    true
}

/// A hit in the game (`events.rs`): one a monster makes on the player, by a normal attack or a skill (`skill`), and its
/// damage, so a plugin can learn how hard each skill hits.
pub fn hit(game: &Game, act: &net::Act, skill: Option<u16>) {
    let by_monster = game.units.get(&act.source).is_some_and(|unit| unit.kind == MONSTER);
    if act.target == game.account_id && act.source != act.target && by_monster {
        tell(json!({"kind": "hit", "source": act.source, "damage": act.damage, "skill": skill}));
    }
}

/// How near the player a monster's cast at a cell is still told (cells): the widest ground skills (Meteor Storm) reach about
/// half of that from their cell.
const CAST_NEAR: i32 = 10;

/// A monster's cast with a cast time (`events.rs`): at the player, or at a cell within `CAST_NEAR` of it. A plugin has the
/// cast time to step out of a ground skill's area or put up a defense.
pub fn cast(game: &Game, source: u32, target: u32, cell: [u16; 2], skill: u16, cast_ms: u32) {
    let by_monster = game.units.get(&source).is_some_and(|unit| unit.kind == MONSTER);
    if let Some(news) = cast_news(by_monster, game.account_id, game.player.cell, source, target, cell, skill, cast_ms) {
        tell(news);
    }
}

/// The news for a cast, when it is one a plugin needs: by a monster, with a cast time, at the player or at a cell near it.
#[allow(clippy::too_many_arguments)]
fn cast_news(by_monster: bool, me: u32, at: [i32; 2], source: u32, target: u32, cell: [u16; 2], skill: u16, cast_ms: u32) -> Option<Value> {
    if !by_monster || cast_ms == 0 || source == me {
        return None;
    }
    let spot = [i32::from(cell[0]), i32::from(cell[1])];
    let at_me = target == me;
    let near = target == 0 && (spot[0] - at[0]).abs().max((spot[1] - at[1]).abs()) <= CAST_NEAR;
    (at_me || near).then(|| json!({"kind": "cast", "source": source, "skill": skill, "ms": cast_ms, "at_me": at_me, "cell": (!at_me).then_some(spot)}))
}

/// A skill the server refused (`events.rs`), with its reason.
pub fn refused(skill: u16, cause: u8) {
    tell(json!({"kind": "refused", "skill": skill, "cause": cause}));
}

fn say(game: &mut Game, text: impl Into<String>) {
    game.chat_line_kind(format!("hunt: {}", text.into()), HUNT_LINE, Kind::Mcp);
}

/// `hunt_state`: what the hunt decides on, and the news since the last call.
pub fn state(game: &Game, items: &HashMap<u32, ItemInfo>) -> Value {
    let news = {
        let mut news = NEWS.lock().unwrap();
        news.polled = Some(Instant::now());
        std::mem::take(&mut news.items)
    };
    let clock = control::now();
    let now = Instant::now();
    let status = |id: u16, login: i64| game.status.get(&id).copied().unwrap_or(login);
    let player = &game.player;
    let name = |id: u32| crate::inventory::item_name(items, id, true, 0);
    let units: Vec<Value> = game
        .units
        .iter()
        .map(|(id, unit)| {
            json!({"id": id, "kind": unit.kind, "job": unit.job, "name": unit.name, "level": unit.level, "cell": unit.cell, "dead": unit.dead,
                   "gone": unit.remove_at.is_some(), "hidden": unit.hidden, "hp": game.unit_hp.get(id)})
        })
        .collect();
    let ground: Vec<Value> = game
        .ground
        .iter()
        .map(|(id, item)| json!({"id": id, "item": item.item, "amount": item.amount, "at": item.at, "name": name(item.item), "asked": game.pickups_asked.contains(id)}))
        .collect();
    let skills: Vec<Value> = game
        .skills
        .iter()
        .map(|skill| {
            let skill_name = game.skill_info.get(&skill.id).map_or_else(|| skill.id.to_string(), |info| info.name.clone());
            json!({"id": skill.id, "level": skill.level, "kind": skill.kind, "sp": skill.sp, "range": skill.range, "name": skill_name})
        })
        .collect();
    let cooldowns: HashMap<String, u64> = game.cooldowns.blocked(clock).map(|(id, left)| (id.to_string(), left.as_millis() as u64)).collect();
    let icons: Vec<Value> =
        game.status_icons.iter().map(|&(id, _, ends)| json!([id, ends.map(|end| end.saturating_duration_since(now).as_millis() as u64)])).collect();
    let inventory: Vec<Value> = game
        .inventory
        .items
        .iter()
        .map(|item| json!({"index": item.index, "id": item.id, "amount": item.amount, "worn": item.worn, "name": name(item.id)}))
        .collect();
    let party: Vec<u32> = game.party.as_ref().map(|party| party.members.iter().map(|member| member.info.account).collect()).unwrap_or_default();
    json!({
        "map": game.map,
        "account_id": game.account_id,
        "cell": player.cell,
        "dead": player.dead,
        "sitting": player.sitting,
        "busy": player.busy(clock),
        "motion": format!("{:?}", game.player_motion(clock)).to_lowercase(),
        "pending": game.pending.is_some(),
        "attack": game.attack.map(|attack| attack.target),
        "store_open": game.store_open(),
        "hp": status(net::STATUS_HP, game.me.hp),
        "max_hp": status(net::STATUS_MAX_HP, game.me.max_hp),
        "sp": status(net::STATUS_SP, game.me.sp),
        "max_sp": status(net::STATUS_MAX_SP, game.me.max_sp),
        "exp": status(net::STATUS_BASE_EXP, 0),
        "next_exp": status(net::STATUS_NEXT_BASE_EXP, 0),
        "zeny": status(net::STATUS_ZENY, 0),
        "units": units,
        "ground": ground,
        "skills": skills,
        "cooldowns": cooldowns,
        "icons": icons,
        "inventory": inventory,
        "party": party,
        "news": news,
    })
}

/// `hunt_gat`: the map's walkable cells, a row after another from the south-west, '1' walkable.
pub fn gat(game: &Game) -> Value {
    let gat = &game.world.gat;
    let cells: String =
        (0..gat.height as i32).flat_map(|y| (0..gat.width as i32).map(move |x| [x, y])).map(|cell| if gat.walkable(cell) { '1' } else { '0' }).collect();
    json!({"map": game.map, "width": gat.width, "height": gat.height, "cells": cells})
}

/// One thing the hunt program does, as the player would.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(tag = "do", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    Attack {
        id: u32,
    },
    StopAttack,
    ClearPending,
    PickUp {
        id: u32,
    },
    Walk {
        cell: [i32; 2],
    },
    UseItem {
        index: u16,
    },
    UseSkill {
        id: u16,
        level: u16,
    },
    /// An enemy skill at a unit, or a ground skill at a cell.
    AimSkill {
        id: u16,
        level: u16,
        unit: Option<u32>,
        cell: Option<[i32; 2]>,
    },
    /// A line in the chat's MCP tab, after "hunt: ".
    Say {
        text: String,
    },
}

/// `hunt_do`: the actions in order; whether each went (a walk can find no way).
pub fn perform(game: &mut Game, actions: Vec<Action>) -> Value {
    let done: Vec<bool> = actions
        .into_iter()
        .map(|action| match action {
            Action::Attack { id } => {
                game.attack(id);
                true
            }
            Action::StopAttack => {
                game.attack = None;
                true
            }
            Action::ClearPending => {
                game.pending = None;
                true
            }
            Action::PickUp { id } => {
                game.pick_up(id);
                true
            }
            Action::Walk { cell } => game.walk_toward(cell, Instant::now()),
            Action::UseItem { index } => {
                game.use_item(index, &[]);
                true
            }
            Action::UseSkill { id, level } => {
                game.use_skill(id, level);
                true
            }
            Action::AimSkill { id, level, unit, cell } => {
                let target = match (unit, cell) {
                    (Some(unit), _) => SkillTarget::Unit(unit),
                    (None, Some(cell)) => SkillTarget::Cell(cell),
                    (None, None) => return false,
                };
                game.targeting = Some((id, level));
                game.aim_skill(target);
                true
            }
            Action::Say { text } => {
                say(game, text);
                true
            }
        })
        .collect();
    json!({"ok": true, "done": done})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_hunt_actions() {
        let action: Action = serde_json::from_str(r#"{"do": "aim_skill", "id": 5, "level": 10, "unit": 7}"#).unwrap();
        assert_eq!(action, Action::AimSkill { id: 5, level: 10, unit: Some(7), cell: None });
        assert!(is_hunt("/hunt 8 poring") && is_hunt("/hunt") && !is_hunt("/hunted"));
    }

    #[test]
    fn tells_casts_at_the_player_or_near_it() {
        let news = |by_monster, target, cell: [u16; 2], ms| cast_news(by_monster, 7, [100, 100], 9, target, cell, 731, ms);
        // At the player, and at a cell 3 away: told, with the cell for the ground one.
        assert_eq!(news(true, 7, [0, 0], 3000).unwrap()["at_me"], json!(true));
        assert_eq!(news(true, 0, [103, 100], 3000).unwrap()["cell"], json!([103, 100]));
        // Far off, at another unit, instant, or not by a monster: not told.
        assert!(news(true, 0, [150, 100], 3000).is_none());
        assert!(news(true, 8, [0, 0], 3000).is_none());
        assert!(news(true, 7, [0, 0], 0).is_none());
        assert!(news(false, 7, [0, 0], 3000).is_none());
    }
}
