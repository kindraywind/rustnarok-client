//! The game scene's own actions on its state (`Game` in main.rs): NPC dialogs, attacking and walking, skills, chat,
//! items and trades, companions, and opening and closing windows.

use super::*;

/// Whether a unit blocks a click on the cell it's drawn on: not while walking, not while hiding (option 0x2), never a
/// warp or an NPC of job 111 or 139 (job 104 does). A dead one blocks until its fade ends: it stays in the units till then.
// ponytail: derived from each unit's state at the click rather than kept as a table updated on each stand, swing,
// flinch, sit and cast; the cells come out the same, except a swing or flinch while walking.
pub(crate) fn unit_blocks(job: u16, walking: bool, hide_option: u32) -> bool {
    !walking && hide_option & 0x2 == 0 && !matches!(job, 45 | 111 | 139)
}

/// The cells units block for a click: each blocking unit's drawn cell, and an Ice Wall's. Every other ground skill unit
/// (Safety Wall, Pneuma, Sanctuary, traps, portals, the unseen damage-only ones) can be stood on.
pub(crate) fn blocked_cells(units: &HashMap<u32, Actor>, skill_units: &HashMap<u32, (u32, [u16; 2])>, now: Instant) -> std::collections::HashSet<[i32; 2]> {
    let standing = units.values().filter(|actor| unit_blocks(actor.job, actor.moving(), actor.hide_option)).map(|actor| actor::drawn_cell(actor.position(now)));
    standing.chain(skill_units.values().filter(|(kind, _)| *kind == ICE_WALL_UNIT).map(|(_, cell)| cell.map(i32::from))).collect()
}

/// rathena's UNT_ICEWALL: the one ground skill unit whose cell the server makes unwalkable.
const ICE_WALL_UNIT: u32 = 0x8d;

impl Game {
    pub(crate) fn dialog_for(&mut self, npc: u32) -> &mut Dialog {
        if self.dialog.as_ref().is_none_or(|dialog| dialog.npc != npc) {
            self.dialog = Some(Dialog { npc, talking: false, lines: Vec::new(), wipe: false, button: DialogButton::None, menu: None, input: None, warp: None });
            self.say.restart();
        }
        self.dialog.as_mut().unwrap()
    }

    /// The dialog's next/close button, `clicked` or pressed by key: Next and a clicked Close wait out the throttle. A
    /// key's Next that comes too soon goes through when the wait ends (`queued_dialog_press`); a click's is dropped.
    pub(crate) fn press_dialog_button(&mut self, clicked: bool) {
        let Some(dialog) = &self.dialog else { return };
        if dialog.button == DialogButton::None {
            return;
        }
        if dialog.button == DialogButton::Next || clicked {
            let now = Instant::now();
            let through = if clicked { self.say.throttle.pass(now) } else { self.say.throttle.press(now, npc_say::Queued::Next) };
            if !through {
                return;
            }
        }
        self.dialog_button();
    }

    /// Next or Close, now.
    fn dialog_button(&mut self) {
        let Some(dialog) = &mut self.dialog else { return };
        match dialog.button {
            DialogButton::Next => {
                let _ = self.session.next(dialog.npc);
                // The text stays until the next page replaces it.
                dialog.wipe = true;
                dialog.button = DialogButton::None;
            }
            DialogButton::Close => {
                let _ = self.session.close_dialog(dialog.npc);
                self.dismiss_dialog();
            }
            DialogButton::None => {}
        }
    }

    /// Picks the highlighted menu item, or cancels the conversation: the menu answers 255, the talk is closed and
    /// the say window, the menu and the cut-in go. A menu without items cancels.
    pub(crate) fn choose(&mut self, cancel: bool) {
        let Some(dialog) = &mut self.dialog else { return };
        let Some(menu) = dialog.menu.take() else { return };
        let selected = menu.selected;
        // Warp Portal and Teleport: the map as it came; the row after the maps (Cancel) and Esc cancel.
        if let Some((skill, maps)) = dialog.warp.take() {
            let _ = self.session.select_warp_point(skill, maps.get(selected).filter(|_| !cancel).map_or("cancel", String::as_str));
            self.dismiss_dialog();
            return;
        }
        let cancel = cancel || menu.items.is_empty();
        let _ = self.session.choose(dialog.npc, if cancel { 255 } else { selected as u8 + 1 });
        if cancel {
            // ponytail: the say window counts as open whenever there's a talk; CZ_CLOSE_DIALOG for a menu-only talk
            // is harmless to rathena.
            let _ = self.session.close_dialog(dialog.npc);
            self.dismiss_dialog();
            self.cutin = None;
        }
    }

    /// The NPC talk is over: the window goes, and a script-set size returns to the default. Where it was left stays.
    pub(crate) fn dismiss_dialog(&mut self) {
        self.dialog = None;
        self.say.end_talk();
    }

    /// The menu's OK, on the say window's 300 ms timer: by `key` (Enter, Space) one that comes too soon goes through
    /// when the wait ends (`queued_dialog_press`), a click's is dropped.
    pub(crate) fn menu_ok(&mut self, key: bool) {
        let now = Instant::now();
        let through = if key { self.say.throttle.press(now, npc_say::Queued::MenuOk) } else { self.say.throttle.pass(now) };
        if through {
            self.choose(false);
        }
    }

    /// Each frame: a key's Next or menu OK that came inside the 300 ms wait goes through once the wait is over, if
    /// what it pressed is still there.
    pub(crate) fn queued_dialog_press(&mut self, now: Instant) {
        let Some(queued) = self.say.throttle.queued() else { return };
        let still = self.dialog.as_ref().is_some_and(|dialog| match queued {
            npc_say::Queued::Next => dialog.button == DialogButton::Next,
            npc_say::Queued::MenuOk => dialog.menu.is_some(),
        });
        if !still {
            self.say.throttle.forget();
            return;
        }
        match self.say.throttle.due(now) {
            Some(npc_say::Queued::Next) => self.dialog_button(),
            Some(npc_say::Queued::MenuOk) => self.choose(false),
            None => {}
        }
    }

    /// The NPC dialog's buttons as the control socket names them, with the boxes their clicks test: `npc_next` or
    /// `npc_close`, each menu row shown as `npc_menu:<index>`, `npc_menu_ok` and `npc_menu_cancel`, and `npc_input_ok`.
    /// `ok` and `cancel` are the buttons' image sizes.
    pub(crate) fn dialog_button_boxes(&self, ok: [f32; 2], cancel: [f32; 2]) -> Vec<(String, [f32; 4])> {
        let Some(dialog) = &self.dialog else { return Vec::new() };
        let mut boxes = Vec::new();
        if let Some(input) = &dialog.input {
            let [x, y] = input.ok_on(screen());
            boxes.push(("npc_input_ok".to_owned(), [x, y, ok[0], ok[1]]));
        }
        let name = match dialog.button {
            DialogButton::Next => Some("npc_next"),
            DialogButton::Close => Some("npc_close"),
            DialogButton::None => None,
        };
        if let Some(name) = name.filter(|_| dialog.warp.is_none()) {
            boxes.push((name.to_owned(), self.say.button_box()));
        }
        if let Some(menu) = &dialog.menu {
            for (index, [x, y, width, height]) in menu.row_boxes() {
                let [x, y] = self.npc_menu.at([x, y]);
                boxes.push((format!("npc_menu:{index}"), [x, y, width, height]));
            }
            let [ok_at, cancel_at] = npc_menu::buttons(menu.items.len()).map(|place| self.npc_menu.at(place));
            boxes.push(("npc_menu_ok".to_owned(), [ok_at[0], ok_at[1], ok[0], ok[1]]));
            boxes.push(("npc_menu_cancel".to_owned(), [cancel_at[0], cancel_at[1], cancel[0], cancel[1]]));
        }
        boxes
    }

    /// The input box's OK: nothing for an empty field; otherwise the answer goes to an NPC still in view (a text
    /// one also to id 0) and the box closes.
    pub(crate) fn submit_input(&mut self) {
        let Some(dialog) = &mut self.dialog else { return };
        let Some(answer) = dialog.input.as_ref().and_then(npc_input::Input::answer) else { return };
        dialog.input = None;
        let in_view = self.units.contains_key(&dialog.npc);
        let _ = match answer {
            npc_input::Answer::Number(value) if in_view => self.session.input_number(dialog.npc, value),
            npc_input::Answer::Text(text) if in_view || dialog.npc == 0 => self.session.input_text(dialog.npc, &text),
            _ => Ok(()),
        };
    }

    /// Whether players win over monsters under the pointer: Shift held, or aiming a skill for players, but not both.
    // ponytail: "a skill for players" is one the server flags as for friends (Heal, Blessing); a skill table may list
    // more.
    /// Opens the dress room on the player's own look.
    pub(crate) fn open_dressroom(&mut self) {
        self.dressroom = Some(dressroom::Room::new(&character_look(&self.me), &self.dress_lists));
    }

    /// A click in the store search window.
    pub(crate) fn click_catalog(&mut self, hit: catalog::Hit, items: &HashMap<u32, ItemInfo>, messages: &[String]) {
        let Some(catalog) = &mut self.catalog else { return };
        match hit {
            catalog::Hit::Title(TitleHit::Close) | catalog::Hit::Close => self.close_catalog(),
            catalog::Hit::Title(title) => catalog.frame.click_title(title),
            catalog::Hit::Kind(buying) => catalog.buying = buying,
            catalog::Hit::Field(field) => catalog.focus = Some(field),
            catalog::Hit::Search => match catalog::search(&catalog.name, &catalog.min, &catalog.max, items) {
                Ok((ids, max, min)) => {
                    let _ = self.session.search_stores(catalog.buying, max, min, &ids, &[]);
                }
                Err(id) => self.chat_line(messages.get(id).cloned().unwrap_or_default(), CHAT_ERROR),
            },
            catalog::Hit::Next => {
                let _ = self.session.next_store_page();
            }
            catalog::Hit::Row(index) => {
                if catalog.click_row(index) {
                    let row = &catalog.rows[index];
                    let _ = self.session.pick_store(row.account, row.store, row.item);
                }
            }
            catalog::Hit::Window => catalog.focus = None,
        }
    }

    /// Closes the store search, telling the server (CZ_CLOSE_SEARCH_STORE_INFO).
    pub(crate) fn close_catalog(&mut self) {
        if self.catalog.take().is_some() {
            let _ = self.session.close_store_search();
            self.store_mark = None;
        }
    }

    /// Who the player sides with on this map, for `versus`: the map's flags, our party's online members, our guild and
    /// its allies.
    pub(crate) fn side(&self) -> versus::Side {
        let party = self.party.iter().flat_map(|party| &party.members).filter(|member| member.info.online).map(|member| member.info.account).collect();
        let flags = self.map_property.map_or(0, |(_, flags)| flags);
        let allies = self.guild.iter().flat_map(|guild| &guild.relations).filter(|relation| !relation.antagonist).map(|relation| relation.id).collect();
        versus::Side { flags, party, guild: self.guild.as_ref().map_or(0, |guild| guild.id), allies, camp: self.battle.camp(self.account_id) }
    }

    /// Another player as `versus` sees them.
    pub(crate) fn opponent(&self, id: u32) -> Option<versus::Other> {
        let actor = self.units.get(&id).filter(|actor| actor.kind == PLAYER)?;
        Some(versus::Other { account: id, guild: actor.guild, dead: actor.dead || actor.remove_at.is_some(), camp: self.battle.camps.get(&id).copied() })
    }

    pub(crate) fn players_first(&self, shift: bool) -> bool {
        let support =
            self.targeting.and_then(|(id, _)| self.skills.iter().find(|skill| skill.id == id)).is_some_and(|skill| skill.kind & net::Skill::FRIEND != 0);
        shift != support
    }

    /// A monster coming into view brings its HP bar if it's hurt, and loses it if not.
    pub(crate) fn monster_hp(&mut self, unit: &net::Unit) {
        if unit.kind != MONSTER {
            return;
        }
        match unit.hp {
            Some([hp, max]) => {
                self.unit_hp.insert(unit.id, (hp, max));
            }
            None => {
                self.unit_hp.remove(&unit.id);
            }
        }
    }

    /// Attacks `target` and keeps at it, walking into reach first.
    pub(crate) fn attack(&mut self, target: u32) {
        self.attack = Some(self.new_attack(target, true));
        self.attack_step(Instant::now());
    }

    /// An attack on `target` as clicked: fast or slow, and whether it's a skill unit on the ground.
    pub(crate) fn new_attack(&self, target: u32, continuous: bool) -> Attack {
        let ground = !self.units.contains_key(&target) && self.skill_units.contains_key(&target);
        Attack { fast: self.fast_target(target), ground, ..Attack::new(target, continuous) }
    }

    /// What the player is doing, for a pending attack (`attack_works`).
    pub(crate) fn player_motion(&self, now: Instant) -> PlayerMotion {
        let player = &self.player;
        if player.sitting || player.dead {
            return PlayerMotion::Other;
        }
        if self.casts.contains_key(&None) {
            return PlayerMotion::Cast;
        }
        let (action, timing) = player.animation(now);
        let (swing, flinch, _) = actor::combat_actions(player.kind);
        match action {
            actor::WALK => PlayerMotion::Walk,
            _ if !matches!(timing, actor::Timing::Once(..) | actor::Timing::Span(_)) => PlayerMotion::Stand,
            _ if action == flinch => PlayerMotion::Flinch,
            _ if action == swing || action == actor::PICKUP => PlayerMotion::Other,
            _ => PlayerMotion::Skill,
        }
    }

    /// Whether an attack on `target` chases it as a fast one (`slow_target`); ground skill units are.
    pub(crate) fn fast_target(&self, target: u32) -> bool {
        self.units.get(&target).is_none_or(|unit| !slow_target(self.player.step_ms, unit.step_ms))
    }

    /// Where the attack's target is drawn and where a chase aims, as ground points: a unit's drawn position, aimed at
    /// the centre of the cell it walks to when fast; a ground skill unit's cell centre. None when it's gone.
    fn attack_points(&self, attack: Attack, now: Instant) -> Option<([f32; 2], [f32; 2])> {
        let cell = attack_goal(&self.units, &self.skill_units, attack.target)?;
        let Some(unit) = self.units.get(&attack.target) else {
            let centre = world::drawn_point(cell.map(|value| value as f32));
            return Some((centre, centre));
        };
        let drawn = world::drawn_point(unit.position(now));
        let aim = if attack.fast { world::drawn_point(unit.destination().map(|value| value as f32)) } else { drawn };
        Some((drawn, aim))
    }

    /// Moves the attack along, from where the player and the target are drawn: in reach (`world::in_attack_reach`,
    /// with heights) or arrived on the walk cell, the one attack packet goes out, no sooner than ATTACK_GAP after the
    /// last; out of reach, a walk to the chase cell (`world::chase_cell`) when it isn't the walk cell and the target's
    /// gap has passed (`chase_walk`). A target gone (or dying) ends it; after the packet the server keeps at it (or
    /// swings once).
    pub(crate) fn attack_step(&mut self, now: Instant) {
        let Some(mut attack) = self.attack else { return };
        let clock = control::now();
        let Some((drawn, aim)) = self.attack_points(attack, clock) else {
            self.attack = None;
            return;
        };
        if attack.sent || self.store_open() || !attack_works(self.player_motion(clock), self.player.job, attack.ground) {
            return;
        }
        let from = world::drawn_point(self.player.position(clock));
        let gat = &self.world.gat;
        let arrived = self.walk_cell == Some(world::floor_cell(from));
        // Ice Wall and traps: their own range, flat, and a walk whenever the chase cell changes (no timer).
        let (in_reach, range) = if attack.ground {
            ground_unit_reach(from, drawn, self.attack_range, attack.told)
        } else {
            let job = self.units.get(&attack.target).map_or(0, |unit| unit.job);
            let distance = reach_to(world::attack_distance(gat, from, drawn), job, self.location_radius(job));
            (world::in_attack_reach(distance, self.attack_range), self.attack_range)
        };
        if arrived || in_reach {
            match attack_send(&attack, self.last_attack, whole_ms(now)) {
                AttackSend::Send => {
                    let _ = self.session.act(attack.target, attack.action());
                    self.last_attack = Some(whole_ms(Instant::now()));
                    attack.sent = true;
                }
                AttackSend::Drop => {
                    self.attack_was = Some(attack);
                    self.attack = None;
                    return;
                }
                AttackSend::Wait => {}
            }
        } else {
            let cell = world::chase_cell(gat, from, aim, range);
            let gap = if attack.fast { CHASE_GAP_MS } else { ATTACK_GAP_MS };
            let asked = if attack.ground {
                (self.walk_cell != Some(cell)).then(|| *self.walk_cell.insert(cell))
            } else {
                chase_walk(cell, &mut self.walk_cell, &mut self.last_chase_walk, gap, whole_ms(now))
            };
            if let Some(cell) = asked
                && may_move(self.last_move, whole_ms(now))
            {
                self.last_move = Some(whole_ms(Instant::now()));
                let _ = self.session.walk(cell.map(|value| value as u16));
            }
        }
        self.attack_was = Some(attack);
        // A one swing that went out is over; so is one let go of.
        self.attack = Some(attack).filter(|attack| !(attack.sent && !attack.continuous && !attack.held));
    }

    /// The GAT cell the game picks under a point on screen (`world::ground_hit`), if any.
    pub(crate) fn ground_cell(&self, cursor: [f32; 2]) -> Option<[i32; 2]> {
        let view = self.camera.view(screen()[0] / screen()[1]);
        // Through the pointer's whole canvas pixel (its corner), as the game picks.
        view.pick_ground([cursor[0].floor() / screen()[0], cursor[1].floor() / screen()[1]], &self.world.gat)
    }

    /// Walks to a cell, or as close to it as can be walked (`world::walk_goal`), judged from the cell the player is
    /// drawn on. A goal ends the pending pickup or skill; while the player is busy (`Actor::busy`) it's stored and
    /// goes out when that ends, otherwise it's sent by `send_walk`. Whether a walk was sent.
    pub(crate) fn walk_toward(&mut self, cell: [i32; 2], now: Instant) -> bool {
        let clock = control::now();
        let from = actor::drawn_cell(self.player.position(clock));
        let taken = blocked_cells(&self.units, &self.skill_units, clock);
        let gat = &self.world.gat;
        let free = |cell: [i32; 2]| gat.walkable(cell) && !taken.contains(&cell);
        let reachable = |cell: [i32; 2]| world::find_path(gat, from, cell).is_some();
        let Some(goal) = world::walk_goal(cell, from, free, reachable) else { return false };
        let busy = self.player.busy(clock);
        self.stored_walk.click(goal, busy, &mut self.pending).is_some_and(|goal| self.send_walk(goal, now))
    }

    /// Sends CZ_REQUEST_MOVE to `goal` when more than MOVE_GAP_MS whole milliseconds passed since the last one; sooner
    /// it's dropped. With a Laphine or reform window open it counts as sent but nothing goes out (rathena refuses it
    /// then). Whether it counted.
    pub(crate) fn send_walk(&mut self, goal: [i32; 2], now: Instant) -> bool {
        if !may_move(self.last_move, whole_ms(now)) {
            return false;
        }
        self.last_move = Some(whole_ms(Instant::now()));
        if self.synthesis.is_some() || self.upgrade.is_some() || self.reform.is_some() {
            return true;
        }
        self.walk_goal = Some(goal);
        self.walks += 1;
        let _ = self.session.walk(goal.map(|value| value as u16));
        true
    }

    /// Turns toward a cell's centre from where the player is drawn, telling the server even when nothing turns. Only
    /// standing, sitting or flinching; see `turned`, counted from the body's exact facing.
    pub(crate) fn turn_to(&mut self, cell: [i32; 2]) {
        let now = control::now();
        if self.player.moving() || self.player.swinging(now) {
            return;
        }
        let target = direction_toward(self.player.position(now), cell);
        // The sprites count directions the other way round from rathena: body = (4 - ours) mod 8 and back.
        let body = (12 - self.player.direction % 8) % 8;
        let (body, head) = turned(sectors_from(self.player.rot, target), body, self.player.head_direction as u8, target);
        let _ = self.session.change_direction(body, head);
        self.player.set_direction((12 - body) % 8);
        self.player.set_head(u16::from(head));
    }

    /// What a left click on the world does: talk to an NPC, attack, swing once at a player, pick up an item, or
    /// (the ground, anything else) walk.
    pub(crate) fn world_click(&self, cursor: [f32; 2], shift: bool) -> WorldClick {
        match hovered(&self.hit_boxes, cursor, shift) {
            Some(Owner::Unit(id)) => match self.units.get(&id).map(|actor| actor.kind) {
                // A warp portal: walk onto it.
                Some(NPC) if self.units[&id].job == WARP_JOB => WorldClick::Warp,
                Some(NPC) => WorldClick::Talk(id),
                Some(MONSTER) => WorldClick::Attack(id),
                // Players: an attack where the map (or Shift, or /noshift) says so; one swing a click on a siege map.
                Some(PLAYER) => match versus::click(self.side().flags, self.players_first(shift), self.no_shift, self.account_id, id) {
                    versus::Click::Attack => WorldClick::Attack(id),
                    versus::Click::Swing => WorldClick::Swing(id),
                    versus::Click::Walk => WorldClick::Ground,
                },
                // Someone else's homunculus, mercenary or elemental where the map lets them be fought.
                Some(kind @ (HOMUNCULUS | MERCENARY | ELEMENTAL)) => {
                    let ours = self.homunculus_id == Some(id) || self.mercenary.as_ref().is_some_and(|mercenary| mercenary.id == id);
                    match versus::companion_click(self.side().flags, kind, self.players_first(shift), self.no_shift, ours) {
                        versus::Click::Attack => WorldClick::Attack(id),
                        versus::Click::Swing => WorldClick::Swing(id),
                        versus::Click::Walk => WorldClick::Ground,
                    }
                }
                _ => WorldClick::Ground,
            },
            Some(Owner::Item(id)) => WorldClick::PickUp(id),
            // Ice Wall or a trap: attacked like a monster.
            Some(Owner::SkillUnit(id)) => WorldClick::Attack(id),
            _ => WorldClick::Ground,
        }
    }

    /// A shop's windows open at the sizes the last one was left at (an NPC's shop or a market; the others keep
    /// theirs).
    pub(crate) fn open_shop(&mut self, mut shop: shop::Shop) {
        shop.apply_sizes(self.shop_sizes);
        self.shop = Some(shop);
    }

    pub(crate) fn store_open(&self) -> bool {
        self.my_shop.is_some() || self.my_buying_store.is_some()
    }

    /// What a press would aim the skill being aimed at: the cell under the pointer for ground skills (units there
    /// don't count), the unit under it (or the player) for the others.
    pub(crate) fn aim_target(&self, cursor: [f32; 2], players_first: bool) -> Option<SkillTarget> {
        let (id, _) = self.targeting?;
        let ground = self.skills.iter().chain(&self.item_skill).find(|skill| skill.id == id).is_some_and(|skill| skill.kind & net::Skill::GROUND != 0);
        let unit = match hovered_unit(&self.hit_boxes, cursor, players_first) {
            Some(Owner::Unit(unit)) => Some(unit),
            Some(Owner::Player) => Some(self.account_id),
            _ => None,
        };
        aim_use(ground, unit, if ground { self.ground_cell(cursor) } else { None })
    }

    /// A party member's small window under the pointer, for aiming a skill.
    pub(crate) fn party_member_at(&self, cursor: [f32; 2]) -> Option<u32> {
        self.member_windows.hit(cursor, &party::Grid::new(screen()))
    }

    /// The small party windows, saved for the character when they change while in a party.
    // ponytail: kept on each change, so the file is the same as one written when the game ends.
    pub(crate) fn save_small_windows(&mut self) {
        if self.party.is_none() {
            return;
        }
        let saved = self.member_windows.saved();
        if saved != self.small_saved {
            write_small_windows(&self.me.name, &saved);
            self.small_saved = saved;
        }
    }

    /// A unit fading out whose ID comes back: its fading copy moves aside (`leave_behind`).
    pub(crate) fn leave_behind(&mut self, id: u32) {
        leave_behind(&mut self.units, id);
    }

    /// After a warp (to this map or another) the character stands on the cell's centre, facing the way it did on
    /// entering the map server.
    pub(crate) fn put_after_warp(&mut self, position: [u16; 2]) {
        after_warp(&mut self.player, position, self.entered_facing);
    }

    /// Talks to an NPC, turned toward the cell where it's drawn; only standing or sitting (`may_talk`), else nothing at
    /// all. rathena answers when it's too far. While a say window is open another talk is refused with a chat line.
    pub(crate) fn talk_to(&mut self, id: u32, messages: &[String]) {
        let now = control::now();
        if !may_talk(self.player_motion(now), self.player.sitting) {
            return;
        }
        if let Some(cell) = self.units.get(&id).map(|actor| actor.position(now).map(|value| (value + 0.5).floor() as i32)) {
            self.turn_to(cell);
        }
        if self.dialog.as_ref().is_some_and(|dialog| dialog.warp.is_none()) {
            if let Some(line) = messages.get(TALK_REFUSED) {
                self.chat_line(line.clone(), CHAT_ERROR);
            }
            return;
        }
        let _ = self.session.talk(id);
    }

    /// The trade or storage window is open: the world takes no presses and the held button makes no tries.
    pub(crate) fn trade_or_storage(&self) -> bool {
        self.trade.is_some() || self.storage.frame.open
    }

    pub(crate) fn talk_blocks(&self) -> bool {
        talk_holds(self.dialog.as_ref().is_some_and(|dialog| dialog.talking), self.casts.contains_key(&None))
    }

    /// The pointer is on an NPC window: the say window, the menu or an input box.
    pub(crate) fn on_npc_window(&self, cursor: [f32; 2]) -> bool {
        let Some(dialog) = &self.dialog else { return false };
        (dialog.warp.is_none() && self.say.contains(cursor))
            || dialog.menu.as_ref().is_some_and(|menu| self.npc_menu.local(cursor, menu.size()[1]).is_some())
            || dialog.input.as_ref().is_some_and(|input| input.contains(screen(), cursor))
    }

    /// CZ_PROGRESS when the player's progress bar ends now: its time ran out, or the server took it away.
    pub(crate) fn end_progress(&mut self, cancelled: bool) {
        if self.progress.ends(Instant::now(), cancelled) {
            let _ = self.session.progress_done();
        }
    }

    /// A click on an item: the pickup is pending; nothing turns and nothing goes out yet (`pending_step` works on it
    /// once the player stands, after a walk under way).
    pub(crate) fn pick_up(&mut self, id: u32) {
        if self.store_open() || self.player.sitting || self.player.hidden || !self.ground.contains_key(&id) {
            return;
        }
        self.pending = Some((Pending::PickUp(id), None));
    }

    /// A pickup or skill waiting on a walk, each frame. A pickup goes out as soon as the player's cell is next to the
    /// item, still walking; a walk the move limit held back is asked for again.
    pub(crate) fn pending_step(&mut self, now: Instant) {
        match self.pending {
            Some((Pending::PickUp(id), _)) => {
                let Some(at) = self.ground.get(&id).map(|item| item.at) else {
                    self.pending = None;
                    return;
                };
                let spot = pickup_spot(self.player.cell, at, |cell| self.world.gat.walkable(cell));
                let motion = self.player_motion(control::now());
                match pickup_step(motion, self.player.cell, at, self.pickups_asked.contains(&id), self.walk_cell, spot) {
                    PickStep::Wait => {}
                    // Turned toward it first; one request per item, until an answer or the next map.
                    PickStep::PickUp => {
                        self.pending = None;
                        self.pickups_asked.insert(id);
                        self.turn_to(at);
                        let _ = self.session.pick_up(id);
                    }
                    PickStep::Drop => self.pending = None,
                    // The walk cell is set even when the move limit holds the packet back.
                    PickStep::Walk => {
                        self.walk_cell = Some(spot);
                        if may_move(self.last_move, whole_ms(now)) {
                            self.last_move = Some(whole_ms(Instant::now()));
                            let _ = self.session.walk(spot.map(|value| value as u16));
                        }
                    }
                }
            }
            // A skill: sent the moment it's in reach (walking or not) or on arriving on the walk cell; out of reach, a
            // walk to the chase cell toward where the target is drawn (a cell's centre on the ground) whenever it
            // isn't the walk cell, with no path needed. Only while the player stands, walks or flinches; a target
            // whose unit is gone ends it (a dying one is still there).
            Some((Pending::Skill { id, level, target }, _)) => {
                let clock = control::now();
                if !matches!(self.player_motion(clock), PlayerMotion::Stand | PlayerMotion::Walk | PlayerMotion::Flinch) {
                    return;
                }
                let from = world::drawn_point(self.player.position(clock));
                let range = self.skill_range(id, level);
                let gat = &self.world.gat;
                // Where it aims, the reach distance, and a big monster's job and radius (the sent cell shifts).
                let toward = match target {
                    SkillTarget::Unit(unit) if unit == self.account_id => Some((from, 0.0, None)),
                    SkillTarget::Unit(unit) => self.units.get(&unit).map(|actor| {
                        let drawn = world::drawn_point(actor.position(clock));
                        let radius = self.location_radius(actor.job);
                        (drawn, reach_to(world::attack_distance(gat, from, drawn), actor.job, radius), Some((actor.job, radius)))
                    }),
                    SkillTarget::Cell(cell) => {
                        let centre = world::drawn_point(cell.map(|value| value as f32));
                        Some((centre, (centre[0] - from[0]).hypot(centre[1] - from[1]), None))
                    }
                };
                let Some((aim, distance, big)) = toward else {
                    self.pending = None;
                    return;
                };
                if skill_in_reach(distance, range) || self.walk_cell == Some(world::floor_cell(from)) {
                    self.pending = None;
                    self.pending_walk = None;
                    self.send_skill(id, level, target);
                    return;
                }
                let cell = world::chase_cell(gat, from, aim, range as i32);
                if self.walk_cell != Some(cell) {
                    self.walk_cell = Some(cell);
                    if may_move(self.last_move, whole_ms(now)) {
                        let sent = big.map_or(cell, |(job, radius)| big_walk_cell(cell, job, radius, from, aim));
                        self.last_move = Some(whole_ms(Instant::now()));
                        let _ = self.session.walk(sent.map(|value| value as u16));
                    }
                }
            }
            _ => {}
        }
    }

    /// The topmost window under the pointer.
    pub(crate) fn window_at(&self, cursor: [f32; 2]) -> Option<Panel> {
        self.windows.iter().rev().copied().find(|&id| match id {
            Panel::Hotkeys => self.hotkeys.hit(cursor).is_some(),
            Panel::BasicInfo => self.basic_info.hit(cursor).is_some(),
            Panel::Inventory => self.inventory.hit(cursor).is_some(),
            Panel::Equipment => self.equipment.hit(cursor, &self.inventory.items).is_some(),
            Panel::Skills => self.skill_window.frame.local(cursor, self.skill_window.height()).is_some(),
            Panel::Storage => self.storage.frame.open && self.storage.frame.local(cursor, self.storage.height()).is_some(),
            Panel::StorageCategory(_) | Panel::StorageSearch => {
                let window = storage_list(id).map(|which| self.storage.list_window(which));
                window.is_some_and(|window| window.frame.open && window.frame.local(cursor, window.height()).is_some())
            }
            Panel::Party => self.party_window.frame.open && self.party_window.hit(cursor, self.party.as_ref(), &self.friends).is_some(),
            Panel::Status => self.status_window.frame.open && self.status_window.frame.local(cursor, self.status_height).is_some(),
            Panel::Guild => self.guild.is_some() && self.guild_window.frame.local(cursor, self.guild_window.height()).is_some(),
            Panel::Clan => self.clan.is_some() && self.clan_window.frame.local(cursor, self.clan_window.height()).is_some(),
            Panel::Cart => self.cart.cart.is_some() && self.cart.hit(cursor).is_some(),
            Panel::Pet => self.pet.is_some() && self.pet_window.hit(cursor).is_some(),
            Panel::Mercenary => self.mercenary.is_some() && self.mercenary_window.hit(cursor).is_some(),
            Panel::MercenarySkills => self.mercenary.is_some() && self.mercenary_skill_list.hit(cursor, &self.mercenary_skills, 0).is_some(),
            Panel::Homunculus => self.homunculus.is_some() && self.homunculus_window.hit(cursor).is_some(),
            Panel::HomunculusSkills => self
                .homunculus
                .as_ref()
                .is_some_and(|homunculus| self.homunculus_skill_list.hit(cursor, &self.homunculus_skills, homunculus.skill_points).is_some()),
            Panel::Cutin => self.cutin.as_ref().is_some_and(|cutin| cutin.contains(cursor)),
        })
    }

    /// How see-through a window is drawn: half while it's held.
    pub(crate) fn panel_faded(&self, panel: Panel) -> f32 {
        match panel {
            Panel::Hotkeys => self.hotkeys.frame.faded(),
            Panel::BasicInfo => self.basic_info.frame.faded(),
            _ => self.titled_frame(panel).map_or(0.0, window::Frame::faded),
        }
    }

    /// Every window's frame, by name, for saving where each was left. `minimap` is the game's MINIMAPWNDINFO.
    pub(crate) fn panel_frames(&mut self) -> [(&'static str, &mut window::Frame); 28] {
        let [used, cash, armors, weapons, costumes, ammo, card, etc] = &mut self.storage.categories;
        [
            ("hotkeys", &mut self.hotkeys.frame),
            ("basic_info", &mut self.basic_info.frame),
            ("inventory", &mut self.inventory.frame),
            ("equipment", &mut self.equipment.frame),
            ("skills", &mut self.skill_window.frame),
            ("storage", &mut self.storage.frame),
            ("storage_use", &mut used.frame),
            ("storage_cash", &mut cash.frame),
            ("storage_armors", &mut armors.frame),
            ("storage_weapons", &mut weapons.frame),
            ("storage_costumes", &mut costumes.frame),
            ("storage_ammo", &mut ammo.frame),
            ("storage_card", &mut card.frame),
            ("storage_etc", &mut etc.frame),
            ("storage_search", &mut self.storage.search_window.frame),
            ("party", &mut self.party_window.frame),
            ("status", &mut self.status_window.frame),
            ("guild", &mut self.guild_window.frame),
            ("clan", &mut self.clan_window.frame),
            ("cart", &mut self.cart.frame),
            ("pet", &mut self.pet_window.frame),
            ("mercenary", &mut self.mercenary_window.frame),
            ("mercenary_skills", &mut self.mercenary_skill_list.frame),
            ("homunculus", &mut self.homunculus_window.frame),
            ("homunculus_skills", &mut self.homunculus_skill_list.frame),
            ("minimap", &mut self.minimap_window.frame),
            ("npc_say", &mut self.say.frame),
            ("npc_menu", &mut self.npc_menu),
        ]
    }

    /// Writes where the windows are and which are collapsed, when that changed and none is being dragged. A client
    /// driven over the control socket (scenarios) neither saves nor restores them, so each run starts the same.
    pub(crate) fn save_places(&mut self) {
        if let Some(window) = &self.item_window {
            self.item_place = window.position;
        }
        if let Some(window) = &self.card_window {
            self.card_place = window.position;
        }
        if control::driven() {
            return;
        }
        let frames = self.panel_frames();
        if frames.iter().any(|(_, frame)| frame.dragging()) {
            return;
        }
        let mut places = window_places(frames.iter().map(|(name, frame)| (*name, frame.position, frame.minimized)));
        // Sizes ride along as lines of their own ("inventory_size columns rows 0", "shop_size width rows 0").
        places += &format!("{PARTY_MODE} 0 {} 0\n", self.party_window.mode);
        let [mode_0, mode_1] = self.party_window.grip_rows();
        places += &format!("{PARTY_SIZE} {mode_0} {mode_1} 0\n");
        places += &minimap_window::checks_line(self.minimap_window.checks);
        if let Some(sizes) = self.shop.as_ref().and_then(shop::Shop::sizes) {
            self.shop_sizes = sizes;
        }
        places += &size_lines(self.inventory.size(), self.cart.size(), self.storage.rows(), self.shop_sizes);
        places += &format!("{STORAGE_CATEGORIES} 0 {} 0\n", self.storage.mask);
        places += &format!("{VEND_SIZE} {} {} 0\n{SKILL_LIST_SIZE} 0 {} 0\n", self.vend_size.width, self.vend_size.rows, self.skill_window.list_rows());
        places += &format!("{ITEM_PLACE} {} {} 0\n{CARD_PLACE} {} {} 0\n", self.item_place[0], self.item_place[1], self.card_place[0], self.card_place[1]);
        places += &format!("{COMPANION_SKILLS_SIZE} {} {} 0\n", self.homunculus_skill_list.height(), self.mercenary_skill_list.height());
        let quest = self.quest_window.frame.position;
        places += &format!("{QUEST_PLACE} {} {} 0\n", quest[0], quest[1]);
        if places != self.saved_places {
            if let Some(path) = settings_path("windows") {
                let _ = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(path, &places));
            }
            self.saved_places = places;
        }
    }

    /// A card from a description's strip, in its own window where that was last (the description stays).
    pub(crate) fn show_card(&mut self, card: u32, enchant: bool) {
        if let Some(window) = &self.card_window {
            self.card_place = window.position;
        }
        let mut window = iteminfo::ItemWindow::new(iteminfo::Shown::card(card, enchant));
        window.position = self.card_place;
        self.card_window = Some(window);
    }

    /// A card's picture button: its picture, or shut again when it's that card's already (another card's replaces it).
    pub(crate) fn toggle_card_view(&mut self, card: u32, pictures: &iteminfo::CardPictures, items: &HashMap<u32, ItemInfo>) {
        if self.card_view.as_ref().is_some_and(|view| view.card == card) {
            self.card_view = None;
            return;
        }
        if let Some(name) = pictures.name(card) {
            let title = items.get(&card).map_or_else(String::new, |info| info.name.clone());
            self.card_view = Some(iteminfo::CardView::new(iteminfo::cardbmp_path(name), card, title, screen()));
            escape::touch(&mut self.esc_order, escape::WindowId::CardPicture);
        }
    }

    /// Opens an item's description where the window was last, in place of the one shown.
    pub(crate) fn show_item(&mut self, shown: iteminfo::Shown) {
        if let Some(window) = &self.item_window {
            self.item_place = window.position;
        }
        let mut window = iteminfo::ItemWindow::new(shown);
        window.position = self.item_place;
        self.item_window = Some(window);
    }

    /// Puts the windows where they were left last time.
    pub(crate) fn restore_places(&mut self) {
        if control::driven() {
            return;
        }
        let Some(text) = settings_path("windows").and_then(|path| std::fs::read_to_string(path).ok()) else { return };
        let saved = parse_window_places(&text);
        for (name, frame) in self.panel_frames() {
            if let Some(&(position, minimized)) = saved.get(name) {
                frame.position = window::dragged_to(position, [0.0, 0.0]);
                frame.minimized = minimized;
            }
        }
        if let Some(&([columns, rows], _)) = saved.get(INVENTORY_SIZE) {
            self.inventory.set_size([columns as usize, rows as usize]);
        }
        if let Some(&([columns, rows], _)) = saved.get(CART_SIZE) {
            self.cart.set_size([columns as usize, rows as usize]);
        }
        if let Some(&checks) = saved.get(minimap_window::CHECKS_LINE) {
            self.minimap_window.checks = minimap_window::checks_from(checks);
        }
        if let Some(&([mode_0, mode_1], _)) = saved.get(PARTY_SIZE) {
            self.party_window.set_grip_rows([mode_0 as usize, mode_1 as usize]);
        }
        if let Some(&([_, mode], _)) = saved.get(PARTY_MODE) {
            self.party_window.set_mode(mode as usize);
        }
        if let Some(&([_, rows], _)) = saved.get(STORAGE_SIZE) {
            self.storage.set_rows(rows as usize);
        }
        if let Some(&([_, mask], _)) = saved.get(STORAGE_CATEGORIES) {
            self.storage.mask = mask as u8;
        }
        if let Some(&([_, rows], _)) = saved.get(SKILL_LIST_SIZE) {
            self.skill_window.set_list_rows(rows as usize);
        }
        if let Some(&([width, rows], _)) = saved.get(VEND_SIZE) {
            self.vend_size = vend::GridSize::clamped(width, rows as usize);
        }
        if let Some(&([homunculus, mercenary], _)) = saved.get(COMPANION_SKILLS_SIZE) {
            self.homunculus_skill_list.set_height(homunculus);
            self.mercenary_skill_list.set_height(mercenary);
        }
        if let Some(&(place, _)) = saved.get(ITEM_PLACE) {
            self.item_place = iteminfo::restored_place(place, screen());
        }
        if let Some(&(place, _)) = saved.get(QUEST_PLACE) {
            self.quest_window.frame.position = quest::restored_place(place, screen());
        }
        if let Some(&(place, _)) = saved.get(CARD_PLACE) {
            self.card_place = iteminfo::restored_card_place(place, screen());
        }
        if let (Some(&([width, goods], _)), Some(&([_, deal], _))) = (saved.get(SHOP_SIZE), saved.get(SHOP_DEAL_SIZE)) {
            self.shop_sizes = (shop::snap_width(width), [goods as usize, deal as usize]);
        }
        self.saved_places = text;
    }

    /// The frame of a window with a title bar.
    pub(crate) fn titled_frame(&self, panel: Panel) -> Option<&window::Frame> {
        Some(match panel {
            Panel::Hotkeys | Panel::BasicInfo => return None,
            Panel::Inventory => &self.inventory.frame,
            Panel::Equipment => &self.equipment.frame,
            Panel::Skills => &self.skill_window.frame,
            Panel::Storage => &self.storage.frame,
            Panel::StorageCategory(tab) => &self.storage.categories[tab].frame,
            Panel::StorageSearch => &self.storage.search_window.frame,
            Panel::Party => &self.party_window.frame,
            Panel::Status => &self.status_window.frame,
            Panel::Guild => &self.guild_window.frame,
            Panel::Clan => &self.clan_window.frame,
            Panel::Cart => &self.cart.frame,
            Panel::Pet => &self.pet_window.frame,
            Panel::Mercenary => &self.mercenary_window.frame,
            Panel::MercenarySkills => &self.mercenary_skill_list.frame,
            Panel::Homunculus => &self.homunculus_window.frame,
            Panel::HomunculusSkills => &self.homunculus_skill_list.frame,
            Panel::Cutin => return self.cutin.as_ref().map(|cutin| &cutin.frame),
        })
    }

    /// Whether the pointer is on a window's control: a title bar's close or minimize button, the equipment window's
    /// buttons, a pick list's OK or Cancel, a corner grip.
    // ponytail: tabs, scroll arrows and the minimap aren't told apart yet.
    /// Opens the navigation window, or closes it.
    /// Sends a request to the party booking board off the frame; its answer comes back to `poll_booking`.
    pub(crate) fn booking_request(&mut self, request: booking::Request) {
        if let Some(window) = &mut self.booking {
            window.status = "...".to_owned();
        }
        let (address, auth) = self.booking_web.clone();
        let answers = self.booking_answers.0.clone();
        std::thread::spawn(move || {
            let reply = booking::send(&address, &auth, &request);
            let _ = answers.send((request, reply));
        });
    }

    /// Opens the party booking board (listing what's posted), or closes it.
    pub(crate) fn toggle_booking(&mut self) {
        if self.booking.take().is_none() {
            let level = self.status.get(&net::STATUS_BASE_LEVEL).copied().unwrap_or(1).clamp(1, i64::from(u16::MAX)) as u16;
            self.booking = Some(booking_window::BookingWindow::new(self.account_id, level));
            self.booking_request(booking::Request::List);
        }
        self.sync_esc();
    }

    /// The board's answers: posts found fill the window; a post added or deleted lists the board again.
    pub(crate) fn poll_booking(&mut self) {
        while let Ok((request, reply)) = self.booking_answers.1.try_recv() {
            let Some(window) = &mut self.booking else { continue };
            // ponytail: the board's lines are plain English; the client's own aren't identified.
            match (request, reply) {
                (_, Err(error)) => window.status = format!("The board didn't answer: {error}"),
                (_, Ok(booking::Reply::Posts(posts))) => {
                    window.status = format!("{} posts", posts.len());
                    window.posts = posts;
                }
                (booking::Request::Add(_), Ok(_)) => {
                    window.status = "Posted.".to_owned();
                    self.booking_request(booking::Request::List);
                }
                (_, Ok(_)) => {
                    window.status = "Deleted.".to_owned();
                    self.booking_request(booking::Request::List);
                }
            }
        }
    }

    /// What a click on the board asks.
    pub(crate) fn booking_action(&mut self, action: booking_window::Action) {
        match action {
            booking_window::Action::Close => {
                self.booking = None;
                self.sync_esc();
            }
            booking_window::Action::Search { min_level, max_level, purpose, roles } => {
                self.booking_request(booking::Request::Search { min_level, max_level, purpose, roles });
            }
            booking_window::Action::Post(post) => {
                let post = booking::Post { account: self.account_id, character: self.me.id, name: self.me.name.clone(), ..post };
                self.booking_request(booking::Request::Add(post));
            }
            booking_window::Action::Delete => self.booking_request(booking::Request::Delete),
            booking_window::Action::Join(post) => {
                let _ = self.session.ask_to_join(post.character, post.account);
                if let Some(window) = &mut self.booking {
                    window.status = format!("Asked {} to join.", post.name);
                }
            }
        }
    }

    /// The reason box answered: OK sends the reason as typed (empty is fine), Cancel only closes.
    /// OK in the create (or disband) window: the name's checks, then CZ_REQ_MAKE_GUILD (the window stays open) or,
    /// for the guild's own name, CZ_REQ_DISORGANIZE_GUILD (the window closes).
    pub(crate) fn guild_name_ok(&mut self, messages: &[String]) {
        let Some(create) = &self.guild_companion.create else { return };
        let checked = if create.disband {
            guild_companion::check_disband(&create.name, self.guild.as_ref().map_or("", |guild| guild.name.as_str()))
        } else {
            guild_companion::check_create(&create.name, self.clan.is_some())
        };
        let disband = create.disband;
        match checked {
            guild_companion::Checked::Line(message, color) => self.chat_line(messages.get(message).cloned().unwrap_or_default(), color),
            guild_companion::Checked::Box(message) => {
                if disband {
                    self.guild_companion.create = None;
                }
                self.question_box = Some(BoxQuestion::Notice(message));
            }
            guild_companion::Checked::Send(name) if disband => {
                self.guild_companion.create = None;
                let _ = self.session.disband_guild(&name);
            }
            guild_companion::Checked::Send(name) => {
                let _ = self.session.make_guild(self.me.id, &name);
            }
        }
    }

    pub(crate) fn answer_reason(&mut self, ok: bool) {
        let Some(asked) = self.reason_box.take() else { return };
        let Some(guild) = &self.guild else { return };
        if !ok {
            return;
        }
        let _ = match asked.kind {
            guild::Reasoned::Leave => self.session.leave_guild(guild.id, self.account_id, self.me.id, &asked.text),
            guild::Reasoned::Expel(account, character) if guild.can_expel() => self.session.expel_from_guild(guild.id, account, character, &asked.text),
            guild::Reasoned::Expel(..) => Ok(()),
        };
    }

    /// What a docked party window's click or Enter came to.
    pub(crate) fn party_dialog_action(&mut self, action: party::DialogAction, messages: &[String]) {
        let message = |id: usize| messages.get(id).cloned().unwrap_or_default();
        match action {
            party::DialogAction::Close => self.party_dialog = None,
            party::DialogAction::Send(settings) => {
                self.party_dialog = None;
                let _ = self.session.party_options(settings.exp_even, settings.pickup, settings.share);
            }
            party::DialogAction::LeaderOnly => self.chat_line(message(party::LEADER_ONLY), PARTY_LEADER_ONLY_LINE),
            party::DialogAction::EmptyName(line) => self.chat_line(message(line), CHAT_ERROR),
            party::DialogAction::Make(name, pickup, share) => {
                self.party_dialog = None;
                let _ = self.session.make_party(&name, pickup, share);
            }
            party::DialogAction::Invite(name) => {
                self.party_dialog = None;
                if self.session.invite_to_party(&name).is_ok() {
                    self.chat_line(rustnarok_client::messages::fill(&message(party::INVITATION_SENT), &[&name]), CHAT_ITEM);
                }
            }
        }
    }

    /// Opens a docked party window (closing any other), right of the party window, or closes it when it's the one
    /// open.
    pub(crate) fn toggle_party_dialog(&mut self, kind: party::DialogKind) {
        if self.party_dialog.as_ref().is_some_and(|dialog| dialog.kind == kind) {
            self.party_dialog = None;
            return;
        }
        let leader = self.party.as_ref().is_some_and(|party| party.leader() == Some(self.account_id));
        let at = party::docked_at(self.party_window.frame.open.then_some((self.party_window.frame.position, self.party_window.size()[0])), screen());
        self.party_dialog = Some(party::PartyDialog::new(kind, self.party.as_ref(), leader, at));
    }

    /// The message box answered (OK: `yes`).
    pub(crate) fn answer_box(&mut self, yes: bool, messages: &[String]) {
        let Some(asked) = self.question_box.take() else { return };
        if let BoxQuestion::RebindKey(_) = asked {
            if let Some(window) = &mut self.shortcuts_window {
                window.confirm(yes);
            }
            return;
        }
        if !yes {
            return;
        }
        match asked {
            BoxQuestion::Leave => match self.party.as_ref().map(|party| party::leave_decision(party, self.account_id, &self.map)) {
                Some(party::Leaving::HandOver(account)) => {
                    let _ = self.session.change_party_leader(account);
                    self.leave_party();
                }
                Some(party::Leaving::Delegate) => self.chat_line(messages.get(party::LEAVE_AFTER_DELEGATION).cloned().unwrap_or_default(), CHAT_ERROR),
                Some(party::Leaving::AskDisband) => self.question_box = Some(BoxQuestion::Disband),
                Some(party::Leaving::Leave) | None => self.leave_party(),
            },
            BoxQuestion::Disband => self.leave_party(),
            BoxQuestion::Expel(account, name) => {
                let _ = self.session.expel_from_party(account, &name);
            }
            BoxQuestion::GuildLeader { account, character, .. } => {
                let _ = self.session.move_members(&[(account, character, 0)]);
            }
            BoxQuestion::FreeGoods => self.open_vending(),
            BoxQuestion::ResetSettings => self.reset_settings = true,
            BoxQuestion::RebindKey(_) | BoxQuestion::OnlyLeader | BoxQuestion::LastRow | BoxQuestion::Notice(_) | BoxQuestion::Said(_) => {}
        }
    }

    /// The vending setup's OK went through: CZ_REQ_OPENSTORE2 with its name and goods.
    pub(crate) fn open_vending(&mut self) {
        if let Some(setup) = &self.vend_setup {
            let _ = self.session.open_shop(&setup.name, true, &setup.goods(&self.cart.items));
        }
    }

    /// CZ_REQ_LEAVE_GROUP; the invite window closes.
    pub(crate) fn leave_party(&mut self) {
        let _ = self.session.leave_party();
        if self.party_dialog.as_ref().is_some_and(|dialog| dialog.kind == party::DialogKind::Invite) {
            self.party_dialog = None;
        }
    }

    /// A member's menu entry chosen (`member` by account).
    pub(crate) fn member_choice(&mut self, choice: party::MemberChoice, member: u32, messages: &[String]) {
        let Some(info) = self.party.as_ref().and_then(|party| party.members.iter().find(|m| m.info.account == member)).map(|m| m.info.clone()) else {
            return;
        };
        match choice {
            party::MemberChoice::Mail => {
                let _ = self.session.start_mail(&info.name);
            }
            party::MemberChoice::Leave => self.question_box = Some(BoxQuestion::Leave),
            party::MemberChoice::Whisper => self.whisper_member(&info.name),
            party::MemberChoice::Expel => self.question_box = Some(BoxQuestion::Expel(info.account, info.name)),
            party::MemberChoice::Assign if !info.online => self.chat_line(messages.get(LEADER_OFFLINE).cloned().unwrap_or_default(), CHAT_ERROR),
            party::MemberChoice::Assign => self.leader_asked = Some(info.account),
            party::MemberChoice::AddSmall => self.member_windows.add(info.account, &party::Grid::new(screen())),
            party::MemberChoice::RemoveSmall => self.member_windows.close(info.account),
        }
    }

    /// "Open 1:1 Chat" with a member: their name in the chat's name box, as the player menu's.
    pub(crate) fn whisper_member(&mut self, name: &str) {
        self.chat.whisper = name.to_owned();
        self.chat.naming = false;
    }

    /// The menu on a party member (`small`: on their small window), at the pointer + 1.
    pub(crate) fn member_menu(&mut self, account: u32, small: bool, cursor: [f32; 2], messages: &[String], font: &Font) {
        let leader = self.party.as_ref().is_some_and(|party| party.leader() == Some(self.account_id));
        let online = self.party.as_ref().and_then(|party| party.members.iter().find(|m| m.info.account == account)).is_some_and(|m| m.info.online);
        let choices = party::member_menu(self.account_id, account, leader, small, online)
            .into_iter()
            .map(|choice| Some((messages.get(choice.message()).cloned().unwrap_or_default(), Choice::Member(choice, account))))
            .collect();
        self.menu = Some(menu::Menu::new(offset(cursor, [1.0, 1.0]), choices, font));
    }

    /// Whether a press on this member's row or small window casts the skill being aimed on them.
    pub(crate) fn row_takes_aim(&self, account: u32, shift: bool) -> bool {
        let Some((id, _)) = self.targeting else { return false };
        let friend = self.skills.iter().chain(&self.item_skill).find(|skill| skill.id == id).is_some_and(|skill| skill.kind & net::Skill::FRIEND != 0);
        let online = self.party.as_ref().and_then(|party| party.members.iter().find(|m| m.info.account == account)).is_some_and(|m| m.info.online);
        party::row_takes_skill(id, friend, shift || self.no_shift, account == self.account_id, online)
    }

    /// The party window opens on its party tab (made or joined a party).
    pub(crate) fn open_party_tab(&mut self) {
        self.party_window.friends = false;
        self.party_window.picked = None;
        self.party_window.frame.open = true;
        self.raise(Panel::Party);
    }

    /// The quest NPCs the big minimap shows: those with a mark colour (cell, effect, colour).
    pub(crate) fn big_minimap_quests(&self) -> Vec<([i32; 2], u16, u16)> {
        self.quest_marks.iter().filter(|(.., color)| *color != 0).map(|&(_, cell, effect, color)| (cell, effect, color)).collect()
    }

    /// Whether the small minimap is there: outdoors and not hidden.
    pub(crate) fn minimap_shown(&self) -> bool {
        minimap::shown(self.minimap_view, self.camera_goal.indoor)
    }

    pub(crate) fn toggle_navigation(&mut self) {
        self.navi_window = if self.navi_window.is_some() { None } else { Some(navigation::Window::new()) };
    }

    /// Opens the reputation window, or closes it.
    pub(crate) fn toggle_reputation(&mut self, tables: &reputation::Tables) {
        self.reputation_window = match self.reputation_window.take() {
            Some(_) => None,
            None => Some(reputation::Window::new(tables, &self.reputation)),
        };
    }

    /// The item link under the pointer: the NPC window's, else the chat log's where no window covers it.
    pub(crate) fn link_at(&self, cursor: [f32; 2]) -> Option<&links::Target> {
        if self.minimap_window.covers(cursor) {
            return None;
        }
        links::at(&self.dialog_links, cursor).or_else(|| self.window_at(cursor).is_none().then(|| links::at(&self.chat_links, cursor)).flatten())
    }

    pub(crate) fn on_control(&self, cursor: [f32; 2]) -> bool {
        let inside = |area: [f32; 4]| contains([area[0], area[1]], [area[2], area[3]], cursor);
        let title = self
            .window_at(cursor)
            .and_then(|panel| self.titled_frame(panel))
            .and_then(|frame| frame.title_hit([cursor[0] - frame.position[0], cursor[1] - frame.position[1]]));
        matches!(title, Some(TitleHit::Close | TitleHit::Minimize))
            || self.equipment.button_boxes().into_iter().any(|(_, area)| inside(area))
            || self.pick.as_ref().is_some_and(|(list, _)| list.button_boxes().into_iter().any(|(_, area)| inside(area)))
            || self.forge.as_ref().is_some_and(|forge| forge.button_boxes().into_iter().any(|(_, area)| inside(area)))
            || self.on_grip(cursor)
    }

    /// Whether the pointer is on a window's corner grip (or one is held).
    pub(crate) fn on_grip(&self, cursor: [f32; 2]) -> bool {
        let top = self.window_at(cursor);
        (top == Some(Panel::Inventory) && self.inventory.hit(cursor) == Some(inventory::Hit::Resize))
            || (top == Some(Panel::Cart) && self.cart.hit(cursor) == Some(inventory::Hit::Resize))
            || (top == Some(Panel::Storage) && self.storage.on_grip(cursor))
            || (top == Some(Panel::Party) && self.party_window.on_grip(cursor))
            || top.and_then(storage_list).is_some_and(|which| self.storage.list_window(which).on_grip(cursor))
            || (top == Some(Panel::Skills) && self.skill_window.grip_box().is_some_and(|[x, y, w, h]| contains([x, y], [w, h], cursor)))
            || matches!(self.shop.as_ref().and_then(|shop| shop.hit(cursor)), Some(shop::Hit::Resize(_)))
            || self.vend_setup.as_ref().is_some_and(|setup| setup.hit(cursor, &self.cart.items) == Some(vend::Hit::Resize))
            || self.my_shop.as_ref().is_some_and(|shop| shop.on_grip(cursor))
    }

    /// Whether the aim cursor shows "valid" here: over a unit a press would take; never for a ground skill.
    pub(crate) fn aim_valid(&self, cursor: [f32; 2], shift: bool) -> bool {
        matches!(self.aim_target(cursor, self.players_first(shift)), Some(SkillTarget::Unit(unit)) if self.aim_takes_unit(unit, shift))
    }

    /// Whether a press on `unit` takes the skill being aimed (`aim_takes`). A homunculus takes only Potion Pitcher;
    /// NPCs and the like none.
    pub(crate) fn aim_takes_unit(&self, unit: u32, shift: bool) -> bool {
        let Some((id, _)) = self.targeting else { return false };
        let support = self.skills.iter().chain(&self.item_skill).find(|skill| skill.id == id).is_some_and(|skill| skill.kind == net::Skill::FRIEND);
        let pvp = self.side().flags & net::MAP_NO_LOCK_ON != 0;
        let at = match self.units.get(&unit).map(|actor| actor.kind) {
            _ if unit == self.account_id => AimAt::Me,
            Some(MONSTER) => AimAt::Monster,
            Some(HOMUNCULUS) => return id == POTION_PITCHER,
            Some(NPC) | None => return false,
            Some(_) => AimAt::Player,
        };
        aim_takes(at, support, shift, self.no_shift, pvp)
    }

    pub(crate) fn skill_points(&self) -> i64 {
        self.status.get(&net::STATUS_SKILL_POINT).copied().unwrap_or(0)
    }

    /// Starts using a skill, ending any aim first: by its exact type a self skill goes off at once, an enemy, ground or
    /// support skill is aimed, anything else (passives, traps, mixed types) does nothing. A level above the learned one
    /// does nothing; an aimed skill without a level to pick goes at the learned level.
    pub(crate) fn use_skill(&mut self, id: u16, level: u16) {
        self.targeting = None;
        let Some(&net::Skill { kind, level: learned, .. }) = self.skills.iter().find(|skill| skill.id == id) else {
            return self.use_companion_skill(id, level);
        };
        // With a store open only the mercenary's skills go.
        if self.store_open() {
            return;
        }
        if level == 0 || level > learned {
            return;
        }
        match kind {
            net::Skill::SELF if self.cooled_down(id) => {
                let _ = self.session.use_skill(id, level, self.account_id);
            }
            net::Skill::SELF => {}
            net::Skill::ENEMY | net::Skill::GROUND | net::Skill::FRIEND => {
                let separate = self.skill_info.get(&id).is_some_and(|info| info.separate_level);
                self.targeting = Some((id, if skills::level_pickable(learned, separate) { level } else { learned }));
            }
            // ponytail: trap skills (32) aim at ground skill units in the game; not made yet.
            _ => {}
        }
    }

    /// An item casts a skill: on oneself at once, otherwise aimed as a known one would be.
    pub(crate) fn cast_item_skill(&mut self, skill: net::Skill) {
        if self.store_open() {
            return;
        }
        if skill.kind & net::Skill::SELF != 0 {
            let _ = self.session.use_skill(skill.id, skill.level, self.account_id);
        } else if skill.kind & (net::Skill::ENEMY | net::Skill::GROUND | net::Skill::FRIEND) != 0 {
            self.targeting = Some((skill.id, skill.level));
            self.item_skill = Some(skill);
        }
    }

    /// Uses the aimed skill on what was clicked, walking into its range first.
    pub(crate) fn aim_skill(&mut self, target: SkillTarget) {
        // A Talkie Box asks for its message first.
        if let (Some((TALKIE_BOX, level)), SkillTarget::Cell(cell), None) = (self.targeting, target, &self.talkie_text) {
            self.targeting = None;
            self.amount = Some(window::Amount::name(Asked::TalkieBox { level, cell }, TALKIE_ASKS));
            return;
        }
        let Some((id, level)) = self.targeting.take() else { return };
        // The mercenary and the homunculus cast their own skills from where they stand; out of its range the server does nothing.
        // With a store open the server still takes them on a unit but refuses every skill on the ground.
        if self.mercenary_skills.iter().chain(&self.homunculus_skills).any(|skill| skill.id == id) {
            let _ = match target {
                SkillTarget::Unit(unit) => self.session.use_skill(id, level, unit),
                SkillTarget::Cell(_) if self.store_open() => return,
                SkillTarget::Cell(cell) => self.session.use_skill_at(id, level, cell.map(|value| value.max(0) as u16)),
            };
            return;
        }
        if self.store_open() {
            return;
        }
        // One pending action: the skill takes the attack's place.
        self.attack = None;
        // Out of reach it walks there first: `pending_step` checks every frame.
        self.pending = Some((Pending::Skill { id, level, target }, None));
        self.pending_walk = None;
        self.pending_step(Instant::now());
    }

    /// Whether a skill's cooldown is over (or under 100 ms is left); if not, says so.
    pub(crate) fn cooled_down(&mut self, id: u16) -> bool {
        if self.cooldowns.left(id, control::now()).is_none_or(|left| left < COOLDOWN_GRACE) {
            return true;
        }
        let name = self.skill_info.get(&id).map_or_else(|| id.to_string(), |info| info.name.clone());
        self.chat_line(rustnarok_client::messages::fill(&self.cooldown_line, &[&name]), CHAT_ERROR);
        false
    }

    /// The range in cells of skill `id` at `level` (`skill_range`): 5 for skill 10000, 1 for one the player doesn't
    /// have (not listed, not an item's).
    pub(crate) fn skill_range(&self, id: u16, level: u16) -> f32 {
        if id == 10000 {
            return 5.0;
        }
        let Some(known) = self.skills.iter().chain(&self.item_skill).find(|skill| skill.id == id) else { return 1.0 };
        let info = self.skill_info.get(&id);
        skill_range(info.map_or(&[][..], |info| &info.attack_range), info.and_then(|info| info.max_lv), level, known.level, known.range)
    }

    /// The radius a unit's job stands on (`lua::location_radii`), 0 for most.
    pub(crate) fn location_radius(&self, job: u16) -> f32 {
        self.location_radii.get(&job).copied().unwrap_or(0.0)
    }

    /// Sends a skill walked into range (refused while its cooldown runs).
    pub(crate) fn send_skill(&mut self, id: u16, level: u16, target: SkillTarget) {
        if !self.cooled_down(id) {
            return;
        }
        self.skill_sent = Some((id, level, target));
        let _ = match target {
            SkillTarget::Unit(unit) => self.session.use_skill(id, level, unit),
            SkillTarget::Cell(cell) => self.send_skill_at(id, level, cell.map(|value| value as u16)),
        };
    }

    /// Plays an effect on a unit (by server ID; the player's own works too) where it stands.
    pub(crate) fn play_effect(&mut self, gpu: &Gpu, grf: &Grf, table: &effects::Table, effect: u32, unit: u32) {
        self.play_effect_from(gpu, grf, table, effect, unit, Instant::now());
    }

    /// A cast's aura `effect` on `unit` for a cast of `cast_ms` (`Effects::play_cast_aura`).
    pub(crate) fn play_cast_aura(&mut self, gpu: &Gpu, grf: &Grf, table: &effects::Table, effect: u32, unit: u32, cast_ms: u32) {
        let key = (unit != self.account_id).then_some(unit);
        let cell = match key {
            None => Some(self.player.cell),
            Some(id) => self.units.get(&id).map(|actor| actor.cell),
        };
        if let Some(cell) = cell {
            let at = self.world.cell_position(cell.map(|value| value as f32));
            self.effects.play_cast_aura(gpu, grf, table, effect, key, at, cast_ms, Instant::now());
        }
    }

    /// Escape is used by `unit`: it is tinted and leaves its copies (`Actor::escape`).
    pub(crate) fn start_escape(&mut self, unit: u32) {
        let actor = if unit == self.account_id { Some(&mut self.player) } else { self.units.get_mut(&unit) };
        if let Some(actor) = actor {
            actor.escape();
        }
    }

    /// Jack Frost (WL_JACKFROST / NPC_JACKFROST, effect 801): ten lines of dark blue ice spikes in a star round `source`.
    pub(crate) fn jack_frost(&mut self, gpu: &Gpu, grf: &Grf, source: u32, target: u32) {
        let now = control::now();
        if let (Some(from), Some(_)) = (self.feet(source, now), self.feet(target, now)) {
            self.effects.jack_frost(gpu, grf, from, Instant::now(), |n| effects::pick(0, n.saturating_sub(1)));
        }
    }

    /// Cannon Spear's damage packet from `source` (`Effects::cannon_spear`): its blade points the way it faces on the
    /// screen, after its swing.
    pub(crate) fn cannon_spear(&mut self, gpu: &Gpu, grf: &Grf, table: &effects::Table, source: u32) {
        let key = (source != self.account_id).then_some(source);
        let Some(actor) = (match key {
            None => Some(&self.player),
            Some(id) => self.units.get(&id),
        }) else {
            return;
        };
        let phi = effects::screen_phi(self.camera.yaw.to_degrees(), actor.rot);
        let delay = effects::cannon_delay_ms(actor.sprites.as_ref().map_or(4.0, actor::SpriteSet::current_delay));
        let at = self.world.cell_position(actor.cell.map(|value| value as f32));
        self.effects.cannon_spear(gpu, grf, table, key, at, phi, delay);
    }

    /// A Dragon Breath damage packet of `skill` from `source` on `target` (`Effects::dragon_breath`); nothing when the
    /// caster isn't known.
    pub(crate) fn dragon_breath(&mut self, gpu: &Gpu, grf: &Grf, table: &effects::Table, skill: u16, source: u32, target: u32) {
        let unit = |id: u32| (id != self.account_id).then_some(id);
        let actor = |key: Option<u32>| match key {
            None => Some(&self.player),
            Some(id) => self.units.get(&id),
        };
        let Some(caster) = actor(unit(source)) else { return };
        let phi = effects::screen_phi(self.camera.yaw.to_degrees(), caster.rot);
        let place = |actor: &actor::Actor| self.world.cell_position(actor.cell.map(|value| value as f32));
        let caster_at = place(caster);
        let target_at = actor(unit(target)).map(place);
        self.effects.dragon_breath(gpu, grf, table, skill, (unit(source), caster_at), (unit(target), target_at), phi, Instant::now());
    }

    /// A Silvervine Stem Spear or Spirit of Savage damage packet from `source` on `target` (`Effects::summoner_shot`);
    /// nothing unless both are in sight.
    pub(crate) fn summoner_shot(&mut self, gpu: &Gpu, grf: &Grf, table: &effects::Table, skill: u16, source: u32, target: u32) {
        let now = control::now();
        let unit = |id: u32| (id != self.account_id).then_some(id);
        if let (Some(from), Some(to)) = (self.feet(source, now), self.feet(target, now)) {
            self.effects.summoner_shot(gpu, grf, table, skill, (unit(source), from), (unit(target), to));
        }
    }

    /// Lunatic Carrot Beat's carrots (`effect`) round `target` at `level`; nothing for a target not in sight.
    pub(crate) fn carrots(&mut self, gpu: &Gpu, grf: &Grf, effect: u32, target: u32, level: i32) {
        if let Some(at) = self.feet(target, control::now()) {
            self.effects.carrots(gpu, grf, effect, at, level);
        }
    }

    /// A cast start of `skill` by `source` with a cast of `cast_ms`: Competentia's and Nipelheim Requiem's objects; nothing for
    /// a caster not in sight.
    pub(crate) fn fourth_job_cast(&mut self, gpu: &Gpu, grf: &Grf, table: &effects::Table, skill: u16, source: u32, cast_ms: u32) {
        if let Some(at) = self.feet(source, control::now()) {
            self.effects.fourth_job_cast(gpu, grf, table, skill, (source != self.account_id).then_some(source), at, cast_ms);
        }
    }

    /// A use packet of Petitio (or its damage) or Gale Storm at `level` on `target`; nothing for a target not in sight.
    pub(crate) fn fourth_job_used(&mut self, gpu: &Gpu, grf: &Grf, skill: u16, target: u32, level: i32) {
        if let Some(at) = self.feet(target, control::now()) {
            self.effects.fourth_job_used(gpu, grf, skill, (target != self.account_id).then_some(target), at, level);
        }
    }

    /// Frigg's Song's use packet at `level` from `source`; nothing for a caster not in sight.
    pub(crate) fn frigg_song(&mut self, gpu: &Gpu, grf: &Grf, source: u32, level: i32) {
        if let Some(at) = self.feet(source, control::now()) {
            self.effects.frigg_song(gpu, grf, at, level);
        }
    }

    /// A use packet of Cart Cannon or Shield Shooting from `source` on `target`; nothing unless both are in sight.
    pub(crate) fn thrown(&mut self, gpu: &Gpu, grf: &Grf, table: &effects::Table, skill: u16, source: u32, target: u32) {
        let now = control::now();
        let unit = |id: u32| (id != self.account_id).then_some(id);
        if let (Some(from), Some(to)) = (self.feet(source, now), self.feet(target, now)) {
            self.effects.thrown(gpu, grf, table, skill, (unit(source), from), (unit(target), to), Instant::now());
        }
    }

    /// A damage packet of Triangle Shot (its arrow after the caster's shot) or Axe Boomerang from `source` on `target`;
    /// nothing unless both are in sight.
    pub(crate) fn third_job_hit(&mut self, gpu: &Gpu, grf: &Grf, table: &effects::Table, skill: u16, source: u32, target: u32) {
        let now = control::now();
        let unit = |id: u32| (id != self.account_id).then_some(id);
        let (Some(from), Some(to)) = (self.feet(source, now), self.feet(target, now)) else { return };
        if skill == crate::world_fx::jobs3::AXE_SKILL {
            self.effects.axe_boomerang(gpu, grf, (unit(source), from), (unit(target), to));
            return;
        }
        let caster = if source == self.account_id { Some(&self.player) } else { self.units.get(&source) };
        let delay = effects::cannon_delay_ms(caster.and_then(|actor| actor.sprites.as_ref()).map_or(4.0, actor::SpriteSet::current_delay));
        self.effects.triangle_shot(gpu, grf, table, from, (unit(target), to), delay);
    }

    /// Fallen Angel's cast by `source`: its feathers start where it is drawn; nothing for a caster not in sight.
    pub(crate) fn fallen_angel(&mut self, gpu: &Gpu, grf: &Grf, source: u32) {
        if let Some(at) = self.feet(source, control::now()) {
            self.effects.fallen_angel(gpu, grf, (source != self.account_id).then_some(source), at);
        }
    }

    /// The spirit count of `unit`: a Rebellion's or a Night Watch's coins, none while it is out of sight.
    pub(crate) fn coins(&mut self, gpu: &Gpu, grf: &Grf, unit: u32, count: u16) {
        if let Some((_, seen, at)) = self.spirit_owner(unit, crate::world_fx::jobs3::object_coins) {
            self.effects.coins(gpu, grf, (unit != self.account_id).then_some(unit), count, at, seen);
        }
    }

    /// The spirit count of `unit`: the Abyss Chaser's shadow balls or the Dragon Knight's swords.
    pub(crate) fn spirit_objects(&mut self, gpu: &Gpu, grf: &Grf, unit: u32, count: u16) {
        if let Some((job, seen, at)) = self.spirit_owner(unit, crate::world_fx::status4::object_spheres) {
            self.effects.spirit_objects(gpu, grf, (unit != self.account_id).then_some(unit), job, count, seen, at);
        }
    }

    /// A unit in sight whose job `shows` its spirits as world objects: its job, whether it is seen (not hidden or cloaked)
    /// and its feet.
    fn spirit_owner(&self, unit: u32, shows: fn(u16) -> bool) -> Option<(u16, bool, world::Vector)> {
        let actor = if unit == self.account_id { Some(&self.player) } else { self.units.get(&unit) };
        let job = if unit == self.account_id { Some(self.me.job) } else { actor.map(|actor| actor.job) };
        let job = job.filter(|&job| shows(job))?;
        let seen = actor.is_some_and(|actor| actor.hide_option & actor::UNSEEN_OPTIONS == 0);
        Some((job, seen, self.feet(unit, control::now())?))
    }

    /// The use packet of Eternal Slash or Shadow Exceed from `source`; nothing for a caster not in sight.
    pub(crate) fn status4_used(&mut self, gpu: &Gpu, grf: &Grf, skill: u16, source: u32) {
        if let Some(at) = self.feet(source, control::now()) {
            self.effects.status4_used(gpu, grf, skill, (source != self.account_id).then_some(source), at);
        }
    }

    /// A damage packet of Attack Machine from `source` on `target`: the caster's drone fires, the hit sounds at the target.
    pub(crate) fn a_machine_hit(&mut self, gpu: &Gpu, grf: &Grf, source: u32, target: u32) {
        if let Some(at) = self.feet(target, control::now()) {
            self.effects.a_machine_hit(gpu, grf, (source != self.account_id).then_some(source), at, Instant::now());
        }
    }

    /// The players whose Shadow Exceed double just ended start leaving Kagemusya's afterimages, unless they already do or
    /// the status is already over.
    // ponytail: a monster's Shadow Exceed status is never sent, so nothing would end its afterimages: monsters leave none.
    // Kagemusya's own status going off ends these afterimages too: they share the unit's one switch.
    pub(crate) fn exceed_afterimages(&mut self) {
        for key in self.effects.take_afterimages().into_iter().filter(|key| self.exceed_on.contains(key)) {
            let actor = if key.is_none() { Some(&mut self.player) } else { key.and_then(|id| self.units.get_mut(&id)) };
            if let Some(actor) = actor.filter(|actor| !actor.shadowing() && !actor::is_monster_job(actor.job)) {
                actor.shadow(true);
                self.exceed_shadows.insert(key);
            }
        }
    }

    /// Cold Slower (791): three prims of four snowflakes thrown from caster's feet toward `aim`.
    pub(crate) fn cold_slower(&mut self, gpu: &Gpu, grf: &Grf, table: &effects::Table, source: u32, aim: world::Vector) {
        let key = (source != self.account_id).then_some(source);
        let feet = self.feet(source, control::now()).unwrap_or_else(|| {
            let cell = key.map_or(self.player.cell, |id| self.units.get(&id).map_or([0, 0], |actor| actor.cell));
            self.world.cell_position(cell.map(|v| v as f32))
        });
        self.effects.cold_slower(gpu, grf, table, key, feet, aim, Instant::now());
    }

    /// Plays an effect on a unit from a moment on (a hit when it lands).
    pub(crate) fn play_effect_from(&mut self, gpu: &Gpu, grf: &Grf, table: &effects::Table, effect: u32, unit: u32, start: Instant) {
        self.play_effect_hits(gpu, grf, table, effect, unit, start, 1);
    }

    /// A Blitz Beat's damage packet sends `caster`'s falcon, if it has one, diving at where `target`'s feet are drawn.
    pub(crate) fn dive_falcon(&mut self, caster: u32, target: u32) {
        let Some(feet) = self.feet(target, control::now()) else { return };
        let actor = if caster == self.account_id { Some(&mut self.player) } else { self.units.get_mut(&caster) };
        if let Some(falcon) = actor.and_then(|actor| actor.falcon.as_mut()) {
            let ground = crate::companion_feet(&self.world, falcon.at, 0.0)[1];
            falcon.dive(feet, ground);
        }
    }

    /// Where a unit's feet are drawn now.
    fn feet(&self, unit: u32, now: Instant) -> Option<world::Vector> {
        let actor = if unit == self.account_id { Some(&self.player) } else { self.units.get(&unit) }?;
        Some(self.world.cell_position(actor.position(now)))
    }

    /// A bow's arrow from over `archer` at `target`, leaving after `wait_ms` (`arrows`).
    pub(crate) fn shoot_arrow(&mut self, grf: &Grf, archer: u32, target: u32, wait_ms: f32) {
        let now = Instant::now();
        let Some(from) = self.feet(archer, now) else { return };
        let aim = arrows::Aim::Unit((target != self.account_id).then_some(target));
        self.effects.shoot_arrow(grf, from, aim, now, Duration::from_secs_f32(wait_ms.max(0.0) / 1000.0));
    }

    /// Arrow Shower's arrows from `archer` at points: the middles of the eight cells round `cell`, and `unit`'s feet
    /// when one is hit there.
    pub(crate) fn arrow_shower(&mut self, grf: &Grf, archer: u32, cell: [i32; 2], unit: Option<u32>) {
        let now = Instant::now();
        let Some(from) = self.feet(archer, now) else { return };
        let round = [[-1, -1], [0, -1], [1, -1], [-1, 0], [1, 0], [-1, 1], [0, 1], [1, 1]]
            .map(|step| self.world.cell_position([cell[0] + step[0], cell[1] + step[1]].map(|value| value as f32)));
        let points: Vec<_> = unit.and_then(|unit| self.feet(unit, now)).into_iter().chain(round).collect();
        for point in points {
            self.effects.shoot_arrow(grf, from, arrows::Aim::Point(point), now, arrows::POINT_WAIT);
        }
    }

    /// An effect from `caster` aimed at where `target` stands (`effects::AIMED`: Soul Strike's ghosts, Fire Ball's
    /// balls…), told the skill's hit count.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn aimed(&mut self, gpu: &Gpu, grf: &Grf, table: &effects::Table, effect: u32, caster: u32, target: u32, hits: u16) {
        // From and to are the feet as drawn, not the cells' centres.
        let now = control::now();
        if let (Some(from), Some(to)) = (self.feet(caster, now), self.feet(target, now)) {
            self.effects.aimed(
                gpu,
                grf,
                table,
                effect,
                ((caster != self.account_id).then_some(caster), target, self.account_id),
                from,
                to,
                hits,
                Instant::now(),
            );
        }
    }

    /// An effect the server sent with no target (`effects::aimed_at_centre`): from `unit`'s feet now, at the map's centre.
    pub(crate) fn aimed_at_centre(&mut self, gpu: &Gpu, grf: &Grf, table: &effects::Table, effect: u32, unit: u32) {
        let now = control::now();
        let Some(from) = self.feet(unit, now) else { return };
        let to = [self.world.gat.width as f32 * world::CELL / 2.0, 0.0, self.world.gat.height as f32 * world::CELL / 2.0];
        self.effects.aimed(gpu, grf, table, effect, ((unit != self.account_id).then_some(unit), 0, self.account_id), from, to, 0, Instant::now());
    }

    /// A heal object on `unit`, standing where the unit is.
    pub(crate) fn heal_object(&mut self, gpu: &Gpu, grf: &Grf, table: &effects::Table, effect: u32, unit: u32) {
        let key = (unit != self.account_id).then_some(unit);
        let cell = match key {
            None => Some(self.player.cell),
            Some(id) => self.units.get(&id).map(|actor| actor.cell),
        };
        if let Some(cell) = cell {
            let at = self.world.cell_position(cell.map(|value| value as f32));
            self.effects.heal_object(gpu, grf, table, key, effect, at, Instant::now());
        }
    }

    /// Plays an effect on a unit told the skill's hit count (a bolt drops an arrow a hit).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn play_effect_hits(&mut self, gpu: &Gpu, grf: &Grf, table: &effects::Table, effect: u32, unit: u32, start: Instant, hits: u16) {
        let key = (unit != self.account_id).then_some(unit);
        // Resurrection's ring is for a player character, not a monster or an NPC.
        if effect == effects::REVIVE_FX && key.is_some_and(|id| self.units.get(&id).is_some_and(|actor| actor.look.is_none())) {
            return;
        }
        let cell = match key {
            None => Some(self.player.cell),
            Some(id) => self.units.get(&id).map(|actor| actor.cell),
        };
        if let Some(cell) = cell {
            let at = self.world.cell_position(cell.map(|value| value as f32));
            self.effects.play_hits(gpu, grf, table, effect, effects::Target::Unit(key), at, start, hits);
            // Blitz Beat's streaks and ring: one ring from the hit effect, none from the server's own.
            if effect == effects::BLITZ_BEAT {
                let rot = key.map_or(Some(self.player.rot), |id| self.units.get(&id).map(|actor| actor.rot));
                self.effects.blitz_beat(gpu, grf, key, rot.unwrap_or(0.0), u32::from(hits > 0), start);
            }
            // EF_HITDARK is EF_HIT1 with near-black sparks.
            if effect == effects::HIT_DARK {
                let rot = key.map_or(Some(self.player.rot), |id| self.units.get(&id).map(|actor| actor.rot));
                self.effects.play_hit_dark(gpu, grf, key, rot.unwrap_or(0.0), start);
            }
            // EF_HIT1 is made in code, flying the exact way the target faces.
            if effect == EF_HIT1 {
                let rot = key.map_or(Some(self.player.rot), |id| self.units.get(&id).map(|actor| actor.rot));
                self.effects.play_hit(gpu, grf, key, rot.unwrap_or(0.0), start);
            }
        }
    }

    /// Blind (health bit 0x10) or Deep Blind on the player: one overlay, ended the tick both are off.
    pub(crate) fn note_blind(&mut self, now: Instant) {
        let on = self.player.health_state & effects::BLIND_BIT != 0 || self.deep_blind;
        self.blind.update(true, on, now);
    }

    /// Starts or stops what a status keeps showing on its unit (the player's own works too): its looping effects, the
    /// copies left behind it (the quickens' afterimage) and Cursed Circle's dark body.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn status_loop(&mut self, gpu: Option<&Gpu>, grf: &Grf, table: &effects::Table, unit: u32, status: u16, on: bool, value: i64) {
        let key = (unit != self.account_id).then_some(unit);
        self.scene.note(key.is_none(), status, on);
        // Deep Blind is the player's overlay. A repeat while it shows does not start the fade again.
        if key.is_none() && status == effects::DEEP_BLIND {
            self.deep_blind = on;
            self.note_blind(control::now());
        }
        // A Warlock's spheres: one a slot, by the element its value names.
        if let (Some(slot), Some(gpu)) = (crate::world_fx::jobs3::sphere_slot(status), gpu) {
            self.effects.sphere(gpu, grf, key, slot, on, value as i32);
        }
        // The fourth-job status objects take the status's value; frostbite thaws as it ends.
        // A unit not in sight starts none; going off only removes.
        if let Some(gpu) = gpu {
            let at = self.feet(unit, control::now());
            if let Some(at) = at.or((!on).then_some([0.0; 3])) {
                self.effects.status_object(gpu, grf, key, at, status, on, value as i32);
            }
            if let Some(at) = at.filter(|_| !on) {
                for &effect in crate::world_fx::status4::end_effects(status) {
                    self.effects.play(gpu, grf, table, effect, effects::Target::Unit(key), at, Instant::now());
                }
            }
        }
        // Shadow Exceed's afterimages last as long as its status.
        if status == crate::world_fx::status4::SHADOW_EXCEED_STATUS {
            if on {
                self.exceed_on.insert(key);
            } else {
                self.exceed_on.remove(&key);
                if self.exceed_shadows.remove(&key) {
                    let actor = if key.is_none() { Some(&mut self.player) } else { key.and_then(|id| self.units.get_mut(&id)) };
                    if let Some(actor) = actor {
                        actor.shadow(false);
                    }
                }
            }
        }
        // Song of Mana's and Wug Dance's circle runs out its ticks when the status goes off.
        if effects::circle_ignores(status, on, value) {
            return;
        }
        let shown = table.status(status);
        self.effects.forget_status(key, status);
        if !on {
            self.effects.status_objects_off(key, shown);
        }
        let actor = match key {
            None => Some(&mut self.player),
            Some(id) => self.units.get_mut(&id),
        };
        let Some(actor) = actor else { return };
        if shown.contains(&actor::AFTERIMAGE) {
            actor.blur(status, on);
        }
        if shown.contains(&actor::HALLUCINATION_WALK) {
            actor.hallucination(on);
        }
        // Cloud Kill's poison greys the unit; the player's statuses sent again after a map load start no second copy.
        if shown.contains(&actor::CLOUD_POISON) {
            let replay = key.is_none() && actor.loaded_at.is_some_and(|at| at.elapsed() < actor::REPLAY_WINDOW);
            actor.cloud_poison(on, replay);
        }
        // Striking leaves white copies behind a player, standing or not.
        if shown.contains(&actor::STRIKING) {
            actor.strike(on && actor.look.is_some());
        }
        // Kagemusya leaves a grey copy behind it every tick.
        if shown.contains(&actor::KAGEMUSYA) {
            actor.shadow(on);
        }
        if actor::CURSED_CIRCLE.contains(&status) {
            actor.cursed = on;
        }
        actor.glow(status, shown, on);
        let job = actor.job;
        let at = self.world.cell_position(actor.cell.map(|value| value as f32));
        let monster = actor::is_monster_job(job);
        if let Some(gpu) = gpu.filter(|_| on) {
            for &effect in shown
                .iter()
                .filter(|&&effect| !matches!(effect, actor::AFTERIMAGE | actor::STRIKING | actor::HALLUCINATION_WALK | actor::KAGEMUSYA | actor::CLOUD_POISON))
                .filter(|&&effect| !crate::world_fx::status4::is_class(effect))
            {
                // Manhole's body is for the other units only; a monster in the hole only sounds.
                if effect == effects::MANHOLE_BODY && monster {
                    self.effects.sound_of(table, effect, at, Instant::now());
                    continue;
                }
                self.effects.play(gpu, grf, table, effect, effects::Target::Status(key, status), at, Instant::now());
            }
        }
    }

    /// Plays an effect at a cell.
    pub(crate) fn play_effect_at(&mut self, gpu: &Gpu, grf: &Grf, table: &effects::Table, effect: u32, cell: [u16; 2]) {
        let at = self.world.cell_position(cell.map(f32::from));
        self.effects.play(gpu, grf, table, effect, effects::Target::Ground, at, Instant::now());
    }

    /// Alt+G and the menu's guild button: the guild window shut, or open (on its last tab, at its last place) with its
    /// pages asked for.
    pub(crate) fn toggle_guild_window(&mut self) {
        if self.guild_window.frame.open {
            self.guild_window.frame.open = false;
            return;
        }
        self.guild_window.open(screen());
        self.raise(Panel::Guild);
        self.ask_guild_pages();
    }

    /// The guild window's pages: the guild's info and its members, and the open tab's (positions, skills, the expel
    /// list; the notice comes by itself). Which tabs open is asked for first, until the server has said.
    pub(crate) fn ask_guild_pages(&mut self) {
        if self.guild.is_some() && self.guild_window.menu == guild::MENU_UNKNOWN {
            let _ = self.session.guild_menu();
        }
        if self.guild.is_some() {
            let tab = self.guild_window.tab as u32;
            for page in [0, 1].into_iter().chain((2..=4).contains(&tab).then_some(tab)) {
                let _ = self.session.guild_page(page);
            }
        }
    }

    /// Asks for a guild's emblem unless that version is here or asked for already.
    pub(crate) fn want_emblem(&mut self, guild: u32, version: u32) {
        if guild == 0 || version == 0 {
            return;
        }
        let known = self.emblems.entry(guild).or_insert((0, None));
        if known.0 != version {
            known.0 = version;
            let _ = self.session.guild_emblem(guild);
        }
    }

    /// Sits down, or stands up.
    pub(crate) fn toggle_sit(&mut self) {
        let _ = self.session.act(0, if self.player.sitting { net::ACT_STAND } else { net::ACT_SIT });
    }

    /// Sends what's typed: whispered when the name box has a name, to the party with a leading % (or `party`, Ctrl
    /// held). Returns a message to show when it can't.
    pub(crate) fn send_chat(&mut self, party: bool, guild: bool, messages: &[String], graphics: &mut options::GraphicsSettings) -> Option<usize> {
        let typed = std::mem::take(&mut self.chat.input);
        if typed.is_empty() {
            return None;
        }
        self.chat.sent(&typed);
        // A line of only spaces is text, and goes as it is.
        let text = if typed.trim().is_empty() { typed.as_str() } else { typed.trim() };
        // In the MCP tab a line is for the agent, except a / command, which is the game's as everywhere.
        if self.chat.on_mcp_tab() && crate::mcp::for_agent(text) {
            crate::mcp::typed(self, text);
            return None;
        }
        let to = self.chat.whisper.trim().to_owned();
        if let Some(command) = chat::command(text) {
            self.client_command(command, messages, graphics);
            return None;
        }
        // /sit sits down or stands up; /! /? /ho and the rest show an emotion.
        if text == "/sit" {
            self.toggle_sit();
            return None;
        }
        if let Some(kind) = emotion::by_command(text) {
            let _ = self.session.emote(kind);
            return None;
        }
        // The client's party commands (messages 138-140).
        if let Some(name) = text.strip_prefix("/organize ") {
            let _ = self.session.make_party(name.trim().trim_matches('"'), false, false);
            return None;
        }
        if text == "/leave" {
            let _ = self.session.leave_party();
            return None;
        }
        // ponytail: /refuse and /accept are rathena's names for them (clif_parse_PartyTick).
        if text == "/refuse" || text == "/accept" {
            let _ = self.session.refuse_parties(text == "/refuse");
            return None;
        }
        if let Some(name) = text.strip_prefix("/expel ") {
            let member = self.party.as_ref().and_then(|party| party.members.iter().find(|member| member.info.name == name.trim()));
            if let Some(member) = member {
                let _ = self.session.expel_from_party(member.info.account, &member.info.name);
            }
            return None;
        }
        if let Some(name) = text.strip_prefix("/guild ") {
            let _ = self.session.make_guild(self.me.id, name.trim().trim_matches('"'));
            return None;
        }
        // The guild's name is the key; rathena does nothing for a wrong one.
        // ponytail: /breakguild takes the disband window's name check.
        if let Some(name) = text.strip_prefix("/breakguild ") {
            let name = name.trim().trim_matches('"');
            match guild_companion::check_disband(name, self.guild.as_ref().map_or("", |guild| guild.name.as_str())) {
                guild_companion::Checked::Send(name) => drop(self.session.disband_guild(&name)),
                guild_companion::Checked::Box(message) => self.question_box = Some(BoxQuestion::Notice(message)),
                guild_companion::Checked::Line(..) => {}
            }
            return None;
        }
        // ponytail: /cl sends to the clan until the chat window has the send-to choice (messages 85, 86, 437, 2361).
        if text == "/cl" {
            return None;
        }
        if let Some(message) = text.strip_prefix("/cl ") {
            let message = message.trim();
            if !message.is_empty() {
                let _ = self.session.clan_chat(&self.me.name, message);
            }
            return None;
        }
        // With /battlechat on, a line goes to the battleground team (not to the party, the guild or a whisper).
        if self.battle.chat {
            let _ = self.session.battle_chat(&self.me.name, text);
            return None;
        }
        if to.is_empty() && (guild || text.starts_with('$')) {
            let _ = self.session.guild_chat(&self.me.name, text.trim_start_matches('$'));
            return None;
        }
        if to.is_empty() && (party || text.starts_with('%')) {
            let _ = self.session.party_chat(&self.me.name, text.trim_start_matches('%'));
            return None;
        }
        // The client's own commands (message 136): /deal, /거래 or /교환 and a name ask for a deal.
        if let Some(name) = ["/deal ", "/거래 ", "/교환 "].iter().find_map(|command| text.strip_prefix(command)) {
            return (!self.ask_trade(name.trim())).then_some(TRADE_NO_ONE);
        }
        // A / line nothing above knows: "Invalid Command", and nothing goes out (not as chat, not as a whisper).
        if text.starts_with('/') {
            self.chat_line(messages.get(INVALID_COMMAND).cloned().unwrap_or_default(), CHAT_BROADCAST);
            return None;
        }
        if to.is_empty() {
            let _ = self.session.chat(&self.me.name, text);
        } else {
            let _ = self.session.whisper(&to, text);
            self.whispered.push_back((to.clone(), text.to_owned()));
            self.chat.note_recent(&to);
            self.talks.push_line(&to, format!("(To {to}) : {text}"));
        }
        None
    }

    /// An incoming whisper: always a main-chat line, a 1:1 window when that sender's box is on, and one away reply.
    pub(crate) fn receive_whisper(&mut self, from: String, text: String) {
        let line = format!("(From {from}) : {text}");
        self.chat_line_kind(format!("( From {from} ) : {text}"), WHISPER_COLOR, chat::Kind::Whisper);
        self.chat.whispered_by(&from);
        let friend = self.friends.iter().any(|friend| friend.name.eq_ignore_ascii_case(&from));
        if self.whisper_open.opens(friend) {
            self.talks.ensure(&from);
            self.sync_esc();
        }
        self.talks.push_line(&from, line);
        if let Some(reply) = self.away.reply(&from) {
            let _ = self.session.whisper(&from, &reply);
            self.whispered.push_back((from.clone(), reply.clone()));
            self.chat.note_recent(&from);
            self.talks.push_line(&from, format!("(To {from}) : {reply}"));
            // ponytail: a plain line; the client's wording for an automatic answer isn't identified.
            self.chat_line_kind(format!("Away: answered {from}."), WHISPER_COLOR, chat::Kind::Whisper);
        }
    }

    /// Typing in a 1:1 window whispers that partner, the same packet as the chat's name box.
    pub(crate) fn send_focused_talk(&mut self) {
        let Some((partner, text)) = self.talks.take_input() else { return };
        let _ = self.session.whisper(&partner, &text);
        self.whispered.push_back((partner.clone(), text.clone()));
        self.chat.note_recent(&partner);
        self.talks.push_line(&partner, format!("(To {partner}) : {text}"));
    }

    pub(crate) fn open_friend_setup(&mut self) {
        if self.friend_setup.take().is_some() {
            self.sync_esc();
            return;
        }
        self.friend_setup = Some(whisper::FriendSetup::new(self.whisper_open, self.party_window.frame.position));
        self.sync_esc();
    }

    pub(crate) fn apply_whisper_open(&mut self, settings: whisper::OpenSettings) {
        self.whisper_open = settings;
        // A driven client (a scenario) doesn't keep the boxes, so the next run still starts with both off.
        if !control::driven() {
            settings.save();
        }
    }

    /// OK on the away box sets the message (empty clears it). Cancel leaves the old one.
    pub(crate) fn finish_away(&mut self, text: &str) {
        self.away_prompt = None;
        self.away.set(text);
        // ponytail: the client's line for setting an away message isn't identified.
        let line = match self.away.text() {
            Some(message) => format!("Away message: {message}"),
            None => "Away message off.".to_owned(),
        };
        self.chat_line(line, CHAT_ITEM);
        self.sync_esc();
    }

    /// The pointer is on a whisper window, the friend setup, the block list or the away box.
    pub(crate) fn whisper_at(&self, cursor: [f32; 2]) -> bool {
        self.away_prompt.as_ref().is_some_and(|prompt| prompt.hit(cursor).is_some())
            || self.talks.hit(cursor).is_some()
            || self.block_list.as_ref().is_some_and(|list| list.hit(cursor).is_some())
            || self.friend_setup.as_ref().is_some_and(|setup| setup.hit(cursor).is_some())
    }

    /// A click on a whisper window, the friend setup, the block list or the away box.
    pub(crate) fn whisper_click(&mut self, cursor: [f32; 2]) -> bool {
        if let Some(hit) = self.away_prompt.as_ref().and_then(|prompt| prompt.hit(cursor)) {
            match hit {
                whisper::AwayHit::Ok => {
                    let text = self.away_prompt.take().map(|prompt| prompt.typed).unwrap_or_default();
                    self.finish_away(&text);
                }
                whisper::AwayHit::Cancel => {
                    self.away_prompt = None;
                    self.sync_esc();
                }
                whisper::AwayHit::Box => {}
            }
            return true;
        }
        if let Some((index, hit)) = self.talks.hit(cursor) {
            match self.talks.click(index, hit, cursor) {
                whisper::TalkClick::Save(partner, lines) => match whisper::save_log(std::path::Path::new(screenshot::FOLDER), &partner, &lines) {
                    Ok(path) => self.chat_line(format!("Chat saved: {}", path.display()), CHAT_ITEM),
                    Err(_) => self.chat_line("Chat was not saved.".into(), CHAT_ERROR),
                },
                whisper::TalkClick::Friend(partner) => {
                    let _ = self.session.add_friend(&partner);
                }
                whisper::TalkClick::Closed => self.sync_esc(),
                whisper::TalkClick::None => {}
            }
            return true;
        }
        if let Some(hit) = self.block_list.as_ref().and_then(|list| list.hit(cursor)) {
            match hit {
                whisper::BlockHit::Allow(name) => {
                    if self.session.set_whisper(&name, true).is_ok() {
                        self.whisper_asked.push_back(name);
                    }
                }
                whisper::BlockHit::Title(TitleHit::Close) => {
                    self.block_list = None;
                    self.sync_esc();
                }
                whisper::BlockHit::Title(TitleHit::Title) => {
                    if let Some(list) = &mut self.block_list {
                        list.frame.start_drag(cursor);
                    }
                }
                whisper::BlockHit::Title(title) => {
                    if let Some(list) = &mut self.block_list {
                        list.frame.click_title(title);
                        if !list.frame.open {
                            self.block_list = None;
                            self.sync_esc();
                        }
                    }
                }
                whisper::BlockHit::Window => {}
            }
            return true;
        }
        if let Some(hit) = self.friend_setup.as_ref().and_then(|setup| setup.hit(cursor)) {
            match hit {
                whisper::SetupHit::Title(TitleHit::Title) => {
                    if let Some(setup) = &mut self.friend_setup {
                        setup.frame.start_drag(cursor);
                    }
                }
                whisper::SetupHit::Title(title) => {
                    let close = title == TitleHit::Close;
                    if let Some(setup) = &mut self.friend_setup {
                        setup.frame.click_title(title);
                        if close || !setup.frame.open {
                            self.friend_setup = None;
                            self.sync_esc();
                        }
                    }
                }
                other => {
                    let changed = self.friend_setup.as_mut().and_then(|setup| setup.click(other));
                    if let Some(whisper::SetupClick::Changed(settings)) = changed {
                        self.apply_whisper_open(settings);
                    }
                }
            }
            return true;
        }
        false
    }

    /// Answers a chat command the client handles itself; replies from the server come back as map events.
    pub(crate) fn client_command(&mut self, command: chat::Command, messages: &[String], graphics: &mut options::GraphicsSettings) {
        let message = |id: usize| messages.get(id).cloned().unwrap_or_default();
        let _ = match command {
            chat::Command::Help => {
                for id in std::iter::once(HELP_TITLE).chain(HELP_LINES) {
                    self.chat_line(message(id), CHAT_ITEM);
                }
                Ok(())
            }
            chat::Command::Who => self.session.user_count(),
            chat::Command::Memo => self.session.remember_warp_point(),
            chat::Command::Ranking(kind) => self.session.ask_ranking(kind),
            chat::Command::BattleChat => {
                let id = self.battle.toggle_chat();
                self.chat_line(message(id), CHAT_ITEM);
                Ok(())
            }
            // On leaves the line open with an empty box; off opens it. The window shows either way.
            chat::Command::BattleMode => {
                let id = self.chat_mode.toggle();
                self.chat_mode.save();
                self.chat.hidden = false;
                self.chat.mode_icon = self.chat_mode.icon();
                self.chat_line(message(id), [255, 255, 100]);
                Ok(())
            }
            chat::Command::DressRoom => {
                self.open_dressroom();
                Ok(())
            }
            chat::Command::CashShop => {
                let _ = self.session.open_cashshop(0);
                Ok(())
            }
            // The battleground queue; while queued, how many wait is asked again.
            chat::Command::BattleQueue => {
                self.queue_window.frame.toggle();
                if self.queue_window.frame.open
                    && let Some((name, _)) = &self.queued
                {
                    let _ = self.session.ask_queue_number(name);
                }
                Ok(())
            }
            chat::Command::Navigation => {
                self.toggle_navigation();
                self.sync_esc();
                Ok(())
            }
            // ponytail: the client's lines for /camera speak of zooming; they're used for on and off.
            chat::Command::Camera => {
                self.camera_snap = !self.camera_snap;
                if let Some(path) = settings_path("camerasnap") {
                    let _ = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(path, if self.camera_snap { "1" } else { "0" }));
                }
                let line = messages.get(if self.camera_snap { CAMERA_ON } else { CAMERA_OFF }).cloned().unwrap_or_default();
                self.chat_line(line, CHAT_ITEM);
                Ok(())
            }
            chat::Command::Zoom => {
                let on = !self.camera_goal.zoom_out;
                if let Some(path) = settings_path("zoomout") {
                    let _ = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(path, if on { "1" } else { "0" }));
                }
                self.camera_goal.zoom_out = on;
                let distance = self.camera_goal.distance();
                self.camera_goal.set_distance(distance);
                let line = messages.get(if on { ZOOM_OUT_ON } else { ZOOM_OUT_OFF }).cloned().unwrap_or_default();
                self.chat_line(line, CHAT_ITEM);
                Ok(())
            }
            chat::Command::NoCtrl => {
                self.no_ctrl = !self.no_ctrl;
                if let Some(path) = settings_path("noctrl") {
                    let _ = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(path, if self.no_ctrl { "1" } else { "0" }));
                }
                let line = messages.get(if self.no_ctrl { NO_CTRL_ON } else { NO_CTRL_OFF }).cloned().unwrap_or_default();
                self.chat_line(line, CHAT_ITEM);
                Ok(())
            }
            chat::Command::SkillFail => {
                self.skill_fail = !self.skill_fail;
                if let Some(path) = settings_path("skillfail") {
                    let _ = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(path, if self.skill_fail { "1" } else { "0" }));
                }
                let line = messages.get(if self.skill_fail { SKILL_FAIL_ON } else { SKILL_FAIL_OFF }).cloned().unwrap_or_default();
                self.chat_line(line, CHAT_ITEM);
                Ok(())
            }
            // ponytail: no line of the client's own for /notrade is known; a plain one, as /noshift prints.
            chat::Command::NoTrade => {
                self.no_trade = !self.no_trade;
                self.chat_line(format!("notrade: {}", if self.no_trade { "On" } else { "Off" }), CHAT_ITEM);
                Ok(())
            }
            // ponytail: this data has no line (or help line) of the client's own for /emblem; a plain one.
            chat::Command::Emblem => {
                self.show_emblems = !self.show_emblems;
                if let Some(path) = settings_path("emblem") {
                    let _ =
                        path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(path, if self.show_emblems { "1" } else { "0" }));
                }
                self.chat_line(format!("emblem: {}", if self.show_emblems { "On" } else { "Off" }), CHAT_ITEM);
                Ok(())
            }
            // ponytail: the client's own line for /noshift isn't in this data; a plain one.
            chat::Command::NoShift => {
                self.no_shift = !self.no_shift;
                if let Some(path) = settings_path("noshift") {
                    let _ = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(path, if self.no_shift { "1" } else { "0" }));
                }
                self.chat_line(format!("noshift: {}", if self.no_shift { "On" } else { "Off" }), CHAT_ITEM);
                Ok(())
            }
            // ponytail: plain on/off lines, as /noshift prints.
            chat::Command::LoginOut | chat::Command::NoGuildTalk => {
                let (name, on) = match command {
                    chat::Command::LoginOut => ("loginout", &mut self.login_notices),
                    _ => ("notalkmsg2", &mut self.no_guild_talk),
                };
                *on = !*on;
                let on = *on;
                if let Some(path) = settings_path(name) {
                    let _ = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(path, if on { "1" } else { "0" }));
                }
                self.chat_line(format!("{name}: {}", if on { "On" } else { "Off" }), CHAT_ITEM);
                Ok(())
            }
            chat::Command::Snap | chat::Command::ItemSnap | chat::Command::SkillSnap => {
                let (name, on) = match command {
                    chat::Command::Snap => ("snap", &mut self.snaps.snap),
                    chat::Command::ItemSnap => ("itemsnap", &mut self.snaps.item),
                    _ => ("skillsnap", &mut self.snaps.skill),
                };
                *on = !*on;
                let on = *on;
                if let Some(path) = settings_path(name) {
                    let _ = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(path, if on { "1" } else { "0" }));
                }
                self.chat_line(format!("{name}: {}", if on { "On" } else { "Off" }), CHAT_ITEM);
                Ok(())
            }
            chat::Command::Where => {
                // ponytail: the map and the cell, plainly; match the official client's wording from a screenshot.
                let [x, y] = self.player.cell;
                self.chat_line(format!("{} {x} {y}", map_key(&self.map)), CHAT_ITEM);
                Ok(())
            }
            chat::Command::Whispers { name, allow } => {
                let sent = self.session.set_whisper(&name, allow);
                if sent.is_ok() {
                    self.whisper_asked.push_back(name);
                }
                sent
            }
            chat::Command::BlockList => self.session.whisper_list(),
            chat::Command::Away => {
                let typed = self.away.text().unwrap_or("").to_owned();
                self.away_prompt = Some(whisper::AwayPrompt::new(typed));
                self.sync_esc();
                Ok(())
            }
            chat::Command::AllWhispers { allow } => self.session.set_whisper_all(allow),
            chat::Command::Effect => {
                graphics.effects_on = !graphics.effects_on;
                self.chat_line(message(if graphics.effects_on { EFFECTS_ON } else { EFFECTS_OFF }), CHAT_ITEM);
                Ok(())
            }
            // ponytail: /quake's chat lines use the message table's 2805 ("Screen shake ON") and 2806 ("Screen shake OFF").
            chat::Command::Quake => {
                graphics.quake = !graphics.quake;
                self.chat_line(message(if graphics.quake { QUAKE_ON } else { QUAKE_OFF }), CHAT_ITEM);
                Ok(())
            }
            chat::Command::MinEffect => {
                graphics.min_effect = !graphics.min_effect;
                self.chat_line(message(if graphics.min_effect { MIN_EFFECT_ON } else { MIN_EFFECT_OFF }), CHAT_ITEM);
                Ok(())
            }
            // ponytail: the game's /fog prints no line we know of; ours reads like /snap's.
            chat::Command::Fog => {
                graphics.fog = !graphics.fog;
                self.chat_line(format!("fog: {}", if graphics.fog { "On" } else { "Off" }), CHAT_ITEM);
                Ok(())
            }
            // ponytail: /mable's chat line is unknown; ours reads like /fog's.
            chat::Command::Mable => {
                graphics.mable = !graphics.mable;
                self.chat_line(format!("mable: {}", if graphics.mable { "On" } else { "Off" }), CHAT_ITEM);
                Ok(())
            }
            // /aura: the max-level aura in full or simplified (its motes only); not while /aura2 has it off.
            chat::Command::Aura => {
                graphics.aura = !graphics.aura;
                let line = match (graphics.aura_off, graphics.aura) {
                    (true, _) => AURA_OFF_FIRST,
                    (false, true) => SIMPLIFIED_AURA_OFF,
                    (false, false) => SIMPLIFIED_AURA_ON,
                };
                self.chat_line(message(line), [255, 255, 100]);
                Ok(())
            }
            // /aura2: no max-level aura at all.
            chat::Command::AuraOff => {
                graphics.aura_off = !graphics.aura_off;
                self.chat_line(message(if graphics.aura_off { AURA_EFFECT_OFF } else { AURA_EFFECT_ON }), CHAT_ITEM);
                Ok(())
            }
            chat::Command::Achievements => {
                self.achievement_window.frame.toggle();
                Ok(())
            }
            chat::Command::EmotionList => {
                self.toggle_emotion_list();
                Ok(())
            }
        };
    }

    pub(crate) fn toggle_emotion_list(&mut self) {
        self.emotion_list = if self.emotion_list.is_some() { None } else { Some(emotion_list::List::default()) };
        if self.emotion_list.is_some() {
            escape::touch(&mut self.esc_order, escape::WindowId::EmotionList);
        } else {
            escape::remove(&mut self.esc_order, escape::WindowId::EmotionList);
        }
    }

    /// Opens the macro window, or closes it keeping what was typed.
    pub(crate) fn toggle_macro_window(&mut self) {
        if self.macro_window.take().is_none() {
            self.macro_window = Some(macros::Window::default());
            escape::touch(&mut self.esc_order, escape::WindowId::Macros);
        } else {
            macros::save(macros::path().as_deref(), &self.macro_lines);
            escape::remove(&mut self.esc_order, escape::WindowId::Macros);
        }
    }

    /// A text box has the caret. The chat line does while it is open, and so does a window's field while it is typed in.
    pub(crate) fn text_box_focused(&self) -> bool {
        self.dialog.as_ref().is_some_and(|dialog| dialog.input.is_some())
            || self.amount.is_some()
            || self.vend_setup.as_ref().is_some_and(|setup| setup.typing)
            || self.buying_setup.as_ref().is_some_and(|setup| setup.focus.is_some())
            || self.mail.typing()
            || self.macro_window.as_ref().is_some_and(|window| window.editing.is_some())
            || self.room_make.is_some()
            || self.room.is_some()
            || self.cashshop.as_ref().is_some_and(|shop| shop.typing)
            || self.bank.as_ref().is_some_and(|bank| bank.typing)
            || self.trade.as_ref().is_some_and(|trade| trade.typing)
            || self.booking.as_ref().is_some_and(|window| window.focus.is_some())
            || self.party_dialog.as_ref().is_some_and(|dialog| dialog.typing)
            || self.reason_box.is_some()
            || self.guild_companion.typing()
            || self.guild_window.typing.is_some()
            || self.storage.typing
            || self.pet_window.typing
            || self.homunculus_window.typing
            || self.quest_window.typing
            || self.catalog.as_ref().is_some_and(|catalog| catalog.focus.is_some())
            || self.talks.typing()
            || self.away_prompt.is_some()
            || self.minimap_window.naming()
            // Battle mode off: the line is always open, so letters stay out of the shortcut layer.
            || self.chat_mode.chat_line_open()
    }

    /// Opens the memorial window and puts it first for Esc.
    pub(crate) fn show_memorial(&mut self) {
        self.memorial.frame.open = true;
        self.memorial.frame.minimized = false;
        escape::touch(&mut self.esc_order, escape::WindowId::Memorial);
    }

    /// The instance is gone: the window and its question go with it.
    pub(crate) fn end_memorial(&mut self) {
        self.memorial.instance = None;
        self.memorial.frame.open = false;
        self.memorial_asked = false;
        escape::remove(&mut self.esc_order, escape::WindowId::Memorial);
        escape::remove(&mut self.esc_order, escape::WindowId::MemorialAsked);
    }

    /// F11 closes the player's windows. A talk with an NPC stays until the server ends it.
    pub(crate) fn close_windows(&mut self) {
        for frame in [
            &mut self.hotkeys.frame,
            &mut self.basic_info.frame,
            &mut self.inventory.frame,
            &mut self.equipment.frame,
            &mut self.skill_window.frame,
            &mut self.storage.frame,
            &mut self.party_window.frame,
            &mut self.status_window.frame,
            &mut self.guild_window.frame,
            &mut self.clan_window.frame,
            &mut self.cart.frame,
            &mut self.pet_window.frame,
            &mut self.mercenary_window.frame,
            &mut self.mercenary_skill_list.frame,
            &mut self.homunculus_window.frame,
            &mut self.homunculus_skill_list.frame,
            &mut self.quest_window.frame,
            &mut self.achievement_window.frame,
            &mut self.mail.list.frame,
            &mut self.queue_window.frame,
            &mut self.memorial.frame,
        ] {
            frame.open = false;
        }
        self.memorial_asked = false;
        self.basic_info.menu_open = false;
        self.equipment.swapping = false;
        if self.macro_window.take().is_some() {
            macros::save(macros::path().as_deref(), &self.macro_lines);
        }
        self.emotion_list = None;
        self.world_map = None;
        self.minimap_window.close();
        self.settings_window = None;
        self.shortcuts_window = None;
        self.item_window = None;
        self.card_window = None;
        self.card_view = None;
        self.monster_window = None;
        self.ranking = None;
        self.dressroom = None;
        self.dress_figure = None;
        self.party_dialog = None;
        self.talks = whisper::Talks::default();
        self.friend_setup = None;
        self.block_list = None;
        self.away_prompt = None;
        self.escape = None;
        self.room_make = None;
        self.viewing = None;
        self.mail.letter = None;
        self.mail.compose = None;
        if self.bank.is_some() {
            let _ = self.session.close_bank(self.account_id);
            self.bank = None;
        }
        self.close_catalog();
        self.cancel_forge();
    }

    /// Whether the pointer is over the interface (a window, the chat, a dialog) rather than the world.
    /// Whether the pointer is on a window, or something holds the clicks (a dialog, a menu, a question in the message
    /// box of this size).
    /// The game settings or shortcut settings window under the pointer.
    pub(crate) fn settings_at(&self, cursor: [f32; 2]) -> bool {
        self.settings_window.as_ref().is_some_and(|window| window.hit(cursor).is_some())
            || self.shortcuts_window.as_ref().is_some_and(|window| window.hit(cursor).is_some())
    }

    pub(crate) fn on_ui(&self, cursor: [f32; 2], message: [f32; 2]) -> bool {
        let asking = self.guild_invite.is_some()
            || self.guild_asked.is_some()
            || self.bind_asked.is_some()
            || self.expansion_asked.is_some()
            || self.leader_asked.is_some()
            || self.question_box.is_some()
            || self.deal_confirm.as_ref().is_some_and(|confirm| confirm.hit(cursor).is_some())
            || self.dismiss_asked
            || self.homunculus_asked.is_some()
            || self.pet_asked.is_some()
            || self.friend_asked.is_some()
            || self.party_invite.is_some()
            || self.trade_asked.is_some()
            || self.booking_asked.is_some()
            || self.queue_asked.is_some()
            || self.adopt_asked.is_some()
            || self.memorial_asked;
        self.window_at(cursor).is_some()
            || (self.special_popup.is_some() && special_popup::covers(cursor))
            || self.catalog.as_ref().and_then(|catalog| catalog.hit(cursor)).is_some()
            || self.queue_window.covers(cursor)
            || self.memorial.covers(cursor)
            || self.storage_log.as_ref().is_some_and(|log| log.covers(cursor))
            || self.skill_description.as_ref().is_some_and(|window| window.contains(cursor))
            || self.level_ups.hit(cursor).is_some()
            || (self.quest_icon && quest_notice::on_icon(cursor))
            || (self.minimap_shown() && minimap::covers(cursor))
            || self.reason_box.as_ref().is_some_and(|reason| reason.hit(cursor, screen()).is_some())
            || self.minimap_window.covers(cursor)
            || (asking && contains(dialog_position_of(screen()), message, cursor))
            || self.on_npc_window(cursor)
            || self.talk_blocks()
            || self.amount.is_some()
            || self.escape.is_some()
            || self.world_map.is_some()
            || (self.deal_choice.is_some() && contains(self.deal_frame.position, shop::DEAL_SIZE, cursor))
            || self.menu.is_some()
            || self.pick.as_ref().is_some_and(|(list, _)| list.covers(cursor))
            || self.forge.as_ref().is_some_and(|forge| forge.covers(cursor))
            || self.shop.as_ref().is_some_and(|shop| shop.covers(cursor))
            || self.refine.as_ref().is_some_and(|window| window.hit(cursor).is_some())
            || self.enchant.as_ref().is_some_and(|window| window.hit(cursor).is_some())
            || self.materials.as_ref().is_some_and(|window| window.hit(cursor).is_some())
            || self.bank.as_ref().is_some_and(|bank| bank.hit(cursor).is_some())
            || self.viewing.as_ref().is_some_and(|viewed| viewed.window.hit(cursor, &viewed.items).is_some())
            || self.ranking.as_ref().and_then(|board| board.hit(cursor)).is_some()
            || self.reputation_window.as_ref().and_then(|window| window.hit(cursor)).is_some()
            || self.navi_window.as_ref().and_then(|window| window.hit(cursor)).is_some()
            || self.booking.as_ref().and_then(|window| window.hit(cursor)).is_some()
            || self.dressroom.as_ref().and_then(|room| room.hit(cursor)).is_some()
            || self.party_dialog.as_ref().and_then(|dialog| dialog.hit(cursor)).is_some()
            || self.talks.hit(cursor).is_some()
            || self.friend_setup.as_ref().is_some_and(|setup| setup.hit(cursor).is_some())
            || self.block_list.as_ref().is_some_and(|list| list.hit(cursor).is_some())
            || self.away_prompt.as_ref().is_some_and(|prompt| prompt.hit(cursor).is_some())
            || self.chat.on_list(cursor)
            || self.party_member_at(cursor).is_some()
            || self.guild_companion.hit(cursor).is_some()
            || (self.room.is_none() && self.sign_rects.iter().any(|(rect, ..)| contains([rect[0], rect[1]], [rect[2] - rect[0], rect[3] - rect[1]], cursor)))
            || self.settings_window.as_ref().is_some_and(|window| window.hit(cursor).is_some())
            || self.cashshop.as_ref().is_some_and(|shop| shop.hit(cursor).is_some())
            || self.cashshop_buy_asked
            || self.shortcuts_window.as_ref().is_some_and(|window| window.hit(cursor).is_some())
            || self.card_view.as_ref().is_some_and(|view| view.hit(cursor).is_some())
            || self.item_window.as_ref().is_some_and(|window| window.hit(cursor).is_some())
            || self.card_window.as_ref().is_some_and(|window| window.hit(cursor).is_some())
            || self.menu.as_ref().is_some_and(|menu| menu.hit(cursor).is_some())
            || self.trade.as_ref().is_some_and(|trade| trade.hit(cursor).is_some())
            || self.room.as_ref().is_some_and(|room| room.hit(cursor, &self.me.name).is_some())
            || self.room_make.as_ref().is_some_and(|make| make.hit(cursor).is_some())
            || self.vend_setup.as_ref().is_some_and(|setup| setup.hit(cursor, &self.cart.items).is_some())
            || self.my_shop.as_ref().is_some_and(|shop| shop.hit(cursor).is_some())
            || self.buying_setup.as_ref().is_some_and(|setup| setup.hit(cursor, &self.inventory.items).is_some())
            || self.my_buying_store.as_ref().is_some_and(|store| store.hit(cursor).is_some())
            || self.mail.hit(cursor).is_some()
            || self.quest_window.covers(cursor)
            || self.achievement_window.covers(cursor)
            || self.synthesis.as_ref().is_some_and(|window| window.hit(cursor).is_some())
            || self.upgrade.as_ref().is_some_and(|window| window.hit(cursor).is_some())
            || self.roulette.as_ref().is_some_and(|window| window.hit(cursor).is_some())
            || self.reform.as_ref().is_some_and(|window| window.hit(cursor, &self.inventory.items).is_some())
            || self.grade.as_ref().is_some_and(|window| window.hit(cursor).is_some())
            || roulette::icon_hit(cursor)
            || self.monster_window.as_ref().is_some_and(|window| window.hit(cursor).is_some())
            || self.stylist.as_ref().is_some_and(|window| window.covers(cursor))
            || self.chat.blocks(cursor)
            || self.chat.settings.as_ref().and_then(|settings| settings.hit(cursor)).is_some()
            || self.link_at(cursor).is_some()
            || self.emotion_list.as_ref().is_some_and(|list| list.hit(cursor).is_some())
            || self.macro_window.as_ref().is_some_and(|window| window.hit(cursor).is_some())
    }

    /// Moves an item between the inventory and storage, asking how many when there's more than one.
    pub(crate) fn move_item(&mut self, asked: Asked) {
        let amount = match asked {
            Asked::Store(index) | Asked::Drop(index) | Asked::Trade(index) | Asked::ToCart(index) | Asked::Mail(index) | Asked::Material(index) => {
                self.inventory.get(index).map(|item| item.amount)
            }
            Asked::FromCart(index) | Asked::CartToStore(index) => self.cart.get(index).map(|item| item.amount),
            Asked::Take(index) | Asked::StoreToCart(index) => self.storage.get(index).map(|item| item.amount),
            Asked::Shop(_)
            | Asked::ShopOut(_)
            | Asked::RoomPassword(_)
            | Asked::VendPrice(_)
            | Asked::BuyingPrice(..)
            | Asked::BuyingAmount(_)
            | Asked::TalkieBox { .. } => None,
        };
        match amount {
            Some(1) => self.give(asked, 1),
            Some(amount) => self.amount = Some(window::Amount::new(asked, Some(u32::from(amount)))),
            None => {}
        }
    }

    /// The box asking how many (or for a name) is answered.
    pub(crate) fn answer_amount(&mut self, ok: bool, messages: &[String]) {
        let Some(amount) = self.amount.take().filter(|_| ok) else { return };
        let name = amount.typed.trim();
        let _ = match amount.what {
            Asked::RoomPassword(id) => self.session.enter_chat_room(id, name),
            // The message is in: the box goes where it was aimed (walking there first if need be).
            Asked::TalkieBox { level, cell } => {
                self.talkie_text = Some(name.to_owned());
                self.targeting = Some((TALKIE_BOX, level));
                self.aim_skill(SkillTarget::Cell(cell));
                Ok(())
            }
            Asked::BuyingPrice(index, item_id) => {
                if let Some(setup) = &mut self.buying_setup {
                    setup.set_price(index, item_id, amount.value());
                    self.amount = Some(window::Amount::new(Asked::BuyingAmount(index), None));
                }
                Ok(())
            }
            Asked::BuyingAmount(index) => {
                if let Some(setup) = &mut self.buying_setup {
                    setup.set_amount(index, amount.value().min(u32::from(u16::MAX)) as u16);
                }
                Ok(())
            }
            Asked::ShopOut(row) => {
                if let Some(shop) = &mut self.shop
                    && amount.value() > 0
                {
                    shop.take_out(row, amount.value());
                }
                Ok(())
            }
            Asked::Shop(row) if amount.value() > 0 => {
                let zeny = self.status.get(&net::STATUS_ZENY).copied().unwrap_or(0).max(0) as u64;
                if let Some(shop) = &mut self.shop
                    && let Some((msg_id, arg)) = {
                        shop.zeny = zeny;
                        shop.add_checked(row, amount.value())
                    }
                {
                    let text = messages.get(msg_id).map_or("", String::as_str);
                    let line = match arg {
                        Some(limit) => rustnarok_client::messages::fill(text, &[&limit]),
                        None => text.to_owned(),
                    };
                    self.chat_line(line, CHAT_ERROR);
                }
                Ok(())
            }
            // A vending price may be 0.
            what @ Asked::VendPrice(_) => {
                self.give(what, amount.value());
                Ok(())
            }
            // None or less into storage or the cart is one.
            what @ (Asked::Store(_) | Asked::ToCart(_) | Asked::StoreToCart(_) | Asked::CartToStore(_)) => {
                self.give(what, amount.value().max(1));
                Ok(())
            }
            what if amount.value() > 0 => {
                self.give(what, amount.value());
                Ok(())
            }
            _ => Ok(()),
        };
    }

    /// Does what was asked with `amount` (no more than there is).
    pub(crate) fn give(&mut self, asked: Asked, amount: u32) {
        let _ = match asked {
            Asked::Shop(row) => {
                if let Some(shop) = &mut self.shop {
                    shop.add(row, amount);
                }
                Ok(())
            }
            Asked::ShopOut(row) => {
                if let Some(shop) = &mut self.shop {
                    shop.take_out(row, amount);
                }
                Ok(())
            }
            Asked::Store(index) => {
                let have = self.inventory.get(index).map_or(0, |item| u32::from(item.amount));
                self.session.put_in_storage(index, amount.min(have))
            }
            Asked::Take(index) => {
                let have = self.storage.get(index).map_or(0, |item| u32::from(item.amount));
                self.session.take_from_storage(index, amount.min(have))
            }
            // More than there is drops nothing.
            Asked::Drop(index) => match self.inventory.get(index).map_or(0, |item| u32::from(item.amount)) {
                have if amount > have => Ok(()),
                _ => self.session.drop_item(index, amount as u16),
            },
            Asked::RoomPassword(_) | Asked::BuyingPrice(..) | Asked::BuyingAmount(_) | Asked::TalkieBox { .. } => Ok(()),
            // ponytail: past the most, a notice; the game asks yes or no, and either way holds it there.
            Asked::VendPrice(index) => {
                if self.vend_setup.as_mut().is_some_and(|setup| setup.set_price(index, amount)) {
                    self.question_box = Some(BoxQuestion::Notice(vend::PRICE_HELD));
                }
                Ok(())
            }
            Asked::ToCart(index) => {
                let have = self.inventory.get(index).map_or(0, |item| u32::from(item.amount));
                self.session.put_in_cart(index, amount.min(have))
            }
            Asked::FromCart(index) => {
                let have = self.cart.get(index).map_or(0, |item| u32::from(item.amount));
                self.session.take_from_cart(index, amount.min(have))
            }
            Asked::StoreToCart(index) => {
                let have = self.storage.get(index).map_or(0, |item| u32::from(item.amount));
                self.session.storage_to_cart(index, between_one_and(amount, have))
            }
            Asked::CartToStore(index) => {
                let have = self.cart.get(index).map_or(0, |item| u32::from(item.amount));
                self.session.cart_to_storage(index, between_one_and(amount, have))
            }
            // Nothing goes to the server until the window's OK.
            Asked::Material(index) => {
                let have = self.inventory.get(index).map_or(0, |item| item.amount);
                if let Some(window) = &mut self.materials {
                    window.add(index, amount.min(u32::from(have)) as u16, have);
                }
                Ok(())
            }
            Asked::Mail(index) => {
                let have = self.inventory.get(index).map_or(0, |item| item.amount);
                self.session.attach_to_mail(index, (amount.min(u32::from(have))) as u16, true)
            }
            Asked::Trade(index) => {
                let have = self.inventory.get(index).map_or(0, |item| u32::from(item.amount));
                match &mut self.trade {
                    Some(trade) => {
                        trade.pending = Some((index, amount.min(have)));
                        self.session.add_to_trade(index, amount.min(have))
                    }
                    None => Ok(()),
                }
            }
        };
    }

    /// Carries out what a click on the mail windows asks; returns a message (by number) for the chat.
    pub(crate) fn mail_action(&mut self, action: mail::Action) -> Option<usize> {
        let _ = match action {
            mail::Action::Open => self.session.open_mailbox(false),
            mail::Action::Close => self.session.close_mailbox(),
            mail::Action::Refresh => self.session.open_mailbox(true),
            mail::Action::Read(kind, id) => self.session.read_mail(kind, id),
            mail::Action::Write(to) => self.session.start_mail(&to),
            mail::Action::Claim { kind, id, zeny } => self.session.claim_mail(kind, id, zeny),
            mail::Action::Delete(kind, id) => self.session.delete_mail(kind, id),
            mail::Action::Check(name) => self.session.check_receiver(&name),
            mail::Action::Detach(index, amount) => self.session.attach_to_mail(index, amount, false),
            mail::Action::Send => {
                let Some(compose) = &self.mail.compose else { return None };
                let receiver = compose.receiver.map_or(0, |(character, ..)| character);
                self.session.send_mail(&compose.to, &self.me.name, &compose.title, &compose.text, compose.zeny(), receiver)
            }
            mail::Action::Cancel => self.session.cancel_mail(),
            mail::Action::Say(id) => return Some(id),
        };
        None
    }

    /// An inventory item let go on the deal: it goes in unless the deal is locked, holds it already, is full or still
    /// waits on the last add.
    pub(crate) fn offer(&mut self, index: u16, all: bool) -> Option<usize> {
        let trade = self.trade.as_ref()?;
        if !trade.open_for_items() {
            return None;
        }
        if let Some(refused) = trade.refuse_offer(index) {
            return refused;
        }
        match self.inventory.get(index).map(|item| u32::from(item.amount)) {
            Some(amount) if all => self.give(Asked::Trade(index), amount),
            _ => self.move_item(Asked::Trade(index)),
        }
        None
    }

    /// Alt + right click on an inventory item: the whole stack goes to the first that's open of the storage, a trade
    /// and the cart. A refused trade says why.
    pub(crate) fn quick_move(&mut self, index: u16) -> Option<usize> {
        let amount = u32::from(self.inventory.get(index)?.amount);
        match quick_target(self.storage.frame.open, self.trade.is_some(), self.cart.cart.is_some() && self.cart.frame.open)? {
            // One quick move with the storage at a time, until the server answers.
            QuickTo::Storage if self.quick_moving => {}
            QuickTo::Storage => {
                self.quick_moving = true;
                self.give(Asked::Store(index), amount);
            }
            QuickTo::Trade => return self.offer(index, true),
            QuickTo::Cart => self.give(Asked::ToCart(index), amount),
        }
        None
    }

    /// The room maker's OK: opens the room (the window stays until the server says).
    pub(crate) fn make_room(&mut self) {
        if let Some(make) = &self.room_make
            && !make.title.trim().is_empty()
        {
            if make.editing {
                // The server's ZC_CHANGE_CHATROOM brings the changed sign.
                let _ = self.session.change_chat_room(make.title.trim(), make.limit(), make.public, &make.password);
                self.room_make = None;
            } else {
                let _ = self.session.make_chat_room(make.title.trim(), make.limit(), make.public, &make.password);
            }
        }
    }

    /// Locks our side of the deal in, with the zeny typed (no more than carried; 0 too).
    pub(crate) fn lock_trade(&mut self, zeny: u32) {
        let carried = self.status.get(&net::STATUS_ZENY).copied().unwrap_or(0).max(0) as u32;
        let zeny = zeny.min(carried);
        if let Some(trade) = &mut self.trade {
            trade.typed = zeny.to_string();
        }
        let _ = self.session.add_to_trade(0, zeny);
        let _ = self.session.lock_trade();
    }

    /// Dev switch `RUSTNAROK_CLIENT_TRADE_GIVE`: once a deal opens, puts those in (one add at a time) and locks.
    pub(crate) fn auto_trade(&mut self) {
        let Ok(give) = std::env::var("RUSTNAROK_CLIENT_TRADE_GIVE") else { return };
        let mut zeny = 0;
        self.trade_give.clear();
        for part in give.split(',') {
            if let Some(amount) = part.strip_prefix('z') {
                zeny = amount.parse().unwrap_or(0);
            } else if let Some((id, amount)) = part.split_once(':')
                && let (Ok(id), Ok(amount)) = (id.parse::<u32>(), amount.parse::<u32>())
                && let Some(index) = self.inventory.items.iter().find(|item| item.id == id && item.worn == 0).map(|item| item.index)
            {
                self.trade_give.push_back((index, amount));
            }
        }
        self.trade_give_zeny = Some(zeny);
        self.auto_trade_next();
    }

    /// The dev switch's next add, or the lock once they're all in.
    pub(crate) fn auto_trade_next(&mut self) {
        if let Some((index, amount)) = self.trade_give.pop_front() {
            self.give(Asked::Trade(index), amount);
        } else if let Some(zeny) = self.trade_give_zeny.take() {
            self.lock_trade(zeny);
        }
    }

    /// Asks a player seen by name for a deal (`/deal name`), or says there's no one of that name.
    pub(crate) fn ask_trade(&mut self, name: &str) -> bool {
        let found = self.units.iter().find(|(_, actor)| actor.kind == PLAYER && actor.name == name).map(|(&id, _)| id);
        if let Some(id) = found {
            self.trade_with = name.to_owned();
            let _ = self.session.request_trade(id);
        }
        found.is_some()
    }

    /// Dev switch `RUSTNAROK_CLIENT_STORE=put|take:<item>:<amount>`, once storage has opened.
    pub(crate) fn auto_store(&mut self) {
        let Some(order) = self.auto_store.take() else { return };
        let mut parts = order.split(':');
        let (Some(way), Some(Ok(id)), Some(Ok(amount))) = (parts.next(), parts.next().map(str::parse::<u32>), parts.next().map(str::parse::<u32>)) else {
            return;
        };
        let asked = if way == "take" {
            self.storage.items.iter().find(|item| item.id == id).map(|item| Asked::Take(item.index))
        } else {
            self.inventory.items.iter().find(|item| item.id == id && item.worn == 0).map(|item| Asked::Store(item.index))
        };
        if let Some(asked) = asked {
            self.give(asked, amount);
        }
    }

    /// Dev switch `RUSTNAROK_CLIENT_DEAL=buy|sell:<row>:<amount>`: puts a row of the shop in the deal and makes it.
    pub(crate) fn auto_deal(&mut self) {
        let Ok(deal) = std::env::var("RUSTNAROK_CLIENT_DEAL") else { return };
        let mut parts = deal.split(':').skip(1).map(str::parse::<u32>);
        let (Some(Ok(row)), Some(Ok(amount)), Some(shop)) = (parts.next(), parts.next(), self.shop.as_mut()) else { return };
        if (row as usize) < shop.offers.len() {
            shop.add(row as usize, amount);
            let _ = shop.send(&mut self.session);
        }
    }

    /// Words over a unit's head (None = the player); an item link shows as its words.
    pub(crate) fn say(&mut self, unit: Option<u32>, text: String, color: [u8; 3]) {
        let text = links::split(&links::mark(&text, &|_| String::new())).0;
        self.speech.insert(unit, (text, color, Instant::now()));
    }

    /// A unit starts or uses a skill: players strike the skill pose, and units call its name (see `overhead::shouts`).
    /// A player caster's motion for a skill packet (`skill_motions`) and its name shouted. A cast holds the cast pose
    /// for `pace` ms (none for skills cast standing); the others play their motion at `pace` times its ACT pace.
    /// Returns when the hit lands, in ms from now: the swing's "atk" frame, the skill pose's frame 6.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn skill_motion(
        &mut self,
        source: u32,
        skill: u16,
        level: i32,
        packet: overhead::Shout,
        pace: f32,
        info: &HashMap<u16, SkillInfo>,
        tables: &LookTables,
        messages: &[String],
    ) -> Option<f32> {
        let me = self.account_id;
        let actor = (if source == me { Some(&mut self.player) } else { self.units.get_mut(&source) })?;
        if actor.remove_at.is_some() {
            return None;
        }
        let (job, kind) = (actor.job, actor.kind);
        let mut hit_ms = None;
        // A cast without cast time, and Meteor Storm's meteors, leave the caster as it is.
        let still = match packet {
            overhead::Shout::Cast => !skill_motions::cast_moves(pace as u32),
            overhead::Shout::Ground => skill == skill_motions::METEOR_STORM,
            _ => false,
        };
        if actor.kind == PLAYER && !still {
            let now = Instant::now();
            let action = match (packet, skill_motions::motion(skill)) {
                (overhead::Shout::Cast, _) if skill_motions::cast_standing(skill) => None,
                (overhead::Shout::Cast, _) => {
                    actor.hold_for(SKILL_ACTION, now, pace);
                    None
                }
                (_, skill_motions::Motion::Keep) => None,
                (_, skill_motions::Motion::Attack) => Some(actor.attack_motion(tables)),
                (_, skill_motions::Motion::Throw) => Some(5),
                (_, skill_motions::Motion::ThirdAttack) => Some(11),
                (_, skill_motions::Motion::PickUp) => Some(actor::PICKUP),
                (_, skill_motions::Motion::Skill) if skill_motions::ready_instead(actor.job) => Some(actor::READY),
                (_, skill_motions::Motion::Skill) => Some(SKILL_ACTION),
            };
            if let Some(action) = action {
                actor.play_at_pace(action, now, pace, false);
                let index = action * 8 + usize::from(actor.direction % 8);
                hit_ms = actor.sprites.as_ref().and_then(|sprites| match action {
                    SKILL_ACTION => sprites.frame_ms(index, 6, pace),
                    _ => sprites.hit_ms(index, pace),
                });
            }
            actor.stand_ready();
        }
        let partner = self.partner.as_deref().filter(|_| source == me);
        let protect_words = messages.get(PROTECT_WORDS).map_or("", String::as_str);
        if overhead::shouts(skill, level, job, kind, packet)
            && let Some(text) = overhead::shout_text(skill, |id| info.get(&id).map(|info| info.name.clone()), partner, protect_words)
        {
            let unit = (source != me).then_some(source);
            self.say(unit, text, CHAT_WHITE);
        }
        hit_ms.map(|ms| ms + skill_motions::hit_extra_ms(skill))
    }

    /// Puts a window in front of the others, and at the front of the Esc list when it is open.
    pub(crate) fn raise(&mut self, id: Panel) {
        self.windows.retain(|&window| window != id);
        self.windows.push(id);
        if let Some(esc) = esc_panel(id) {
            if self.panel_open(id) {
                escape::touch(&mut self.esc_order, esc);
            } else {
                escape::remove(&mut self.esc_order, esc);
            }
        }
    }

    pub(crate) fn panel_open(&self, id: Panel) -> bool {
        match id {
            Panel::Hotkeys => self.hotkeys.frame.open,
            Panel::BasicInfo => self.basic_info.frame.open,
            Panel::Inventory => self.inventory.frame.open,
            Panel::Equipment => self.equipment.frame.open,
            Panel::Skills => self.skill_window.frame.open,
            Panel::Storage => self.storage.frame.open,
            Panel::StorageCategory(tab) => self.storage.categories[tab].frame.open,
            Panel::StorageSearch => self.storage.search_window.frame.open,
            Panel::Party => self.party_window.frame.open,
            Panel::Status => self.status_window.frame.open,
            Panel::Guild => self.guild.is_some() && self.guild_window.frame.open,
            Panel::Clan => self.clan.is_some() && self.clan_window.frame.open,
            Panel::Cart => self.cart.cart.is_some() && self.cart.frame.open,
            Panel::Pet => self.pet.is_some() && self.pet_window.frame.open,
            Panel::Mercenary => self.mercenary.is_some() && self.mercenary_window.frame.open,
            Panel::MercenarySkills => self.mercenary.is_some() && self.mercenary_skill_list.frame.open,
            Panel::Homunculus => self.homunculus.is_some() && self.homunculus_window.frame.open,
            Panel::HomunculusSkills => self.homunculus.is_some() && self.homunculus_skill_list.frame.open,
            Panel::Cutin => self.cutin.is_some(),
        }
    }

    /// Token of Siegfried (or Light of Regene): the dead menu offers standing up here.
    pub(crate) fn can_resurrect(&self) -> bool {
        escape::can_stand_up(self.inventory.items.iter().map(|item| item.id), self.status_icons.iter().map(|(id, ..)| *id))
    }

    /// Every window Esc may close that is open now, questions included.
    pub(crate) fn esc_open(&self) -> Vec<escape::WindowId> {
        use escape::WindowId::*;
        let mut open = Vec::new();
        let mut push = |id: escape::WindowId, shown: bool| {
            if shown {
                open.push(id);
            }
        };
        for panel in [
            Panel::Inventory,
            Panel::Equipment,
            Panel::Skills,
            Panel::Party,
            Panel::Status,
            Panel::Guild,
            Panel::Clan,
            Panel::Cart,
            Panel::Pet,
            Panel::Mercenary,
            Panel::MercenarySkills,
            Panel::Homunculus,
            Panel::HomunculusSkills,
        ] {
            if let Some(id) = esc_panel(panel) {
                push(id, self.panel_open(panel));
            }
        }
        for (tab, category) in self.storage.categories.iter().enumerate() {
            push(StorageCategory(tab), category.frame.open);
        }
        push(StorageSearch, self.storage.search_window.frame.open);
        // An NPC's window and the warp list aren't Esc's to close: they end with their own buttons.
        // An NPC shop isn't Esc's to close.
        push(Shop, self.shop.as_ref().is_some_and(|shop| !shop.plain()));
        push(GradeEnchant, self.grade.is_some());
        push(LaphineSynthesis, self.synthesis.is_some());
        push(LaphineUpgrade, self.upgrade.is_some());
        push(ItemReform, self.reform.is_some());
        push(Enchant, self.enchant.is_some());
        push(Stylist, self.stylist.is_some());
        push(Materials, self.materials.is_some());
        let docked = self.party_dialog.as_ref().map(|dialog| dialog.kind);
        push(PartySetup, docked == Some(party::DialogKind::Setup));
        push(PartyCreate, docked == Some(party::DialogKind::Create));
        push(PartyInviteName, docked == Some(party::DialogKind::Invite));
        push(Whisper, !self.talks.is_empty());
        push(FriendSetup, self.friend_setup.is_some());
        push(BlockList, self.block_list.is_some());
        push(AwayMessage, self.away_prompt.is_some());
        push(DressRoom, self.dressroom.is_some());
        push(Ranking, self.ranking.is_some());
        push(Reputation, self.reputation_window.is_some());
        push(Navigation, self.navi_window.is_some());
        push(Catalog, self.catalog.is_some());
        push(SkillDescription, self.skill_description.is_some());
        push(SkillDelay, self.skill_description.as_ref().is_some_and(|window| window.delays));
        push(Roulette, self.roulette.is_some());
        push(MonsterInfo, self.monster_window.is_some());
        push(Achievements, self.achievement_window.frame.open);
        push(OtherEquipment, self.viewing.is_some());
        push(Bank, self.bank.is_some());
        push(CashShop, self.cashshop.is_some());
        push(PickList, self.pick.as_ref().is_some_and(|(_, picking)| picking.escapable()));
        push(CardPicture, self.card_view.is_some());
        push(ItemInfo, self.item_window.is_some());
        push(CardInfo, self.card_window.is_some());
        push(ChatRoom, self.room.is_some());
        push(RoomMake, self.room_make.is_some());
        push(EmotionList, self.emotion_list.is_some());
        push(Macros, self.macro_window.is_some());
        push(VendSetup, self.vend_setup.is_some());
        push(MyShop, self.my_shop.is_some());
        push(BuyingStoreSetup, self.buying_setup.is_some());
        push(MyBuyingStore, self.my_buying_store.is_some());
        push(Mailbox, self.mail.list.frame.open);
        push(MailRead, self.mail.letter.is_some());
        push(MailWrite, self.mail.compose.is_some());
        push(GameSettings, self.settings_window.is_some());
        push(ShortcutSettings, self.shortcuts_window.is_some());
        push(WorldMap, self.world_map.is_some());
        push(Minimap, self.minimap_window.frame.open);
        push(Quests, self.quest_window.frame.open || self.quest_window.picked.is_some());
        push(Menu, self.menu.is_some());
        push(Capturing, self.capturing);
        push(Amount, self.amount.is_some());
        push(SaveAsked, self.save_asked);
        push(SkillApplyAsked, self.apply_asked);
        push(BindAsked, self.bind_asked.is_some());
        push(ExpansionAsked, self.expansion_asked.is_some());
        push(LeaderAsked, self.leader_asked.is_some());
        push(GuildCompanion, self.guild_companion.frame.open);
        push(GuildCreate, self.guild_companion.create.as_ref().is_some_and(|create| !create.disband));
        push(GuildDisband, self.guild_companion.create.as_ref().is_some_and(|create| create.disband));
        push(GuildTip, self.guild_companion.tip.is_some());
        push(PartyAsked, self.question_box.as_ref().is_some_and(|asked| !matches!(asked, BoxQuestion::Notice(_) | BoxQuestion::Said(_))));
        push(Notice, matches!(self.question_box, Some(BoxQuestion::Notice(_) | BoxQuestion::Said(_))));
        push(DismissAsked, self.dismiss_asked);
        push(HomunculusAsked, self.homunculus_asked.is_some());
        push(PetAsked, self.pet_asked.is_some());
        push(CashShopBuy, self.cashshop_buy_asked);
        push(GuildAsked, self.guild_asked.is_some());
        push(GuildInvite, self.guild_invite.is_some());
        push(PartyInvite, self.party_invite.is_some());
        push(FriendAsked, self.friend_asked.is_some());
        push(DealChoice, self.deal_choice.is_some());
        push(BattleQueue, self.queue_window.frame.open);
        push(Memorial, self.memorial.frame.open);
        push(MemorialAsked, self.memorial_asked);
        push(AdoptAsked, self.adopt_asked.is_some());
        push(QueueAsked, self.queue_asked.is_some());
        open
    }

    /// Brings the Esc list in line with what's open.
    /// Whether a skill is used rather than passive (an unknown one counts as used).
    pub(crate) fn skill_active(&self, id: u16) -> bool {
        let guild = self.guild.iter().flat_map(|guild| &guild.skills);
        self.skills.iter().chain(guild).find(|skill| skill.id == id).is_none_or(|skill| skill.kind != 0)
    }

    pub(crate) fn sync_esc(&mut self) {
        let open = self.esc_open();
        escape::sync(&mut self.esc_order, &open);
    }

    /// `windows` in the control state: the Esc order, newest first, plus a few second names for the same window.
    pub(crate) fn esc_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.esc_order.iter().map(|id| id.name().to_owned()).collect();
        if self.equipment.frame.open && self.equipment.swapping {
            names.push("SwapEquipment".into());
        }
        if self.pick.as_ref().is_some_and(|(list, _)| list.merging) {
            names.push("ItemMerge".into());
        }
        if self.pick.as_ref().is_some_and(|(_, picking)| !picking.escapable()) {
            names.push("PickList".into());
        }
        if self.forge.as_ref().is_some_and(|forge| forge.frame.open) {
            names.push("MakingMaterial".into());
        }
        if self.reason_box.is_some() {
            names.push("GuildReason".into());
        }
        if self.deal_confirm.is_some() {
            names.push("ConfirmDeal".into());
        }
        // Not Esc's to close, but open.
        if self.storage.frame.open {
            names.push("Storage".into());
        }
        if matches!(self.queue_asked, Some(QueueAsked::Lobby(..))) {
            names.push("BattleLobby".into());
        }
        // Esc reaches neither the deal nor its request.
        if self.trade.is_some() {
            names.push("Trade".into());
        }
        if self.trade_asked.is_some() {
            names.push("TradeAsked".into());
        }
        if self.booking.is_some() {
            names.push("PartyBooking".into());
        }
        if self.booking_asked.is_some() {
            names.push("BookingAsked".into());
        }
        names.extend(self.talks.window_names());
        if let Some(dialog) = &self.dialog {
            names.push(if dialog.warp.is_some() { "WarpList" } else { "NpcDialog" }.into());
            if dialog.input.is_some() {
                names.push("NpcInput".into());
            }
        }
        names
    }

    /// A text box inside a window has the caret. The always-on chat line is not one of these.
    pub(crate) fn focused_text(&self) -> Option<escape::WindowId> {
        use escape::WindowId::*;
        if self.away_prompt.is_some() {
            return Some(AwayMessage);
        }
        if self.talks.typing() {
            return Some(Whisper);
        }
        if self.dialog.as_ref().is_some_and(|dialog| dialog.input.is_some()) {
            return Some(NpcInput);
        }
        if self.vend_setup.as_ref().is_some_and(|setup| setup.typing) {
            return Some(VendSetup);
        }
        if self.buying_setup.as_ref().is_some_and(|setup| setup.focus.is_some()) {
            return Some(BuyingStoreSetup);
        }
        if self.mail.compose.as_ref().is_some_and(|compose| compose.field.is_some()) {
            return Some(MailWrite);
        }
        if self.mail.list.typing {
            return Some(Mailbox);
        }
        if self.macro_window.as_ref().is_some_and(|window| window.editing.is_some()) {
            return Some(Macros);
        }
        if self.room_make.is_some() {
            return Some(RoomMake);
        }
        if self.room.is_some() {
            return Some(ChatRoom);
        }
        if self.cashshop.as_ref().is_some_and(|shop| shop.typing) {
            return Some(CashShop);
        }
        if self.bank.as_ref().is_some_and(|bank| bank.typing) {
            return Some(Bank);
        }
        if self.trade.as_ref().is_some_and(|trade| trade.typing) {
            return Some(Trade);
        }
        if self.storage.typing {
            return Some(Storage);
        }
        if self.catalog.as_ref().is_some_and(|catalog| catalog.focus.is_some()) {
            return Some(Catalog);
        }
        if self.minimap_window.naming() {
            return Some(Minimap);
        }
        None
    }

    /// A question is up. An NPC's menu (and the warp list) isn't one: Esc leaves those alone.
    pub(crate) fn question_open(&self) -> bool {
        self.save_asked
            || self.apply_asked
            || self.bind_asked.is_some()
            || self.expansion_asked.is_some()
            || self.leader_asked.is_some()
            || self.question_box.is_some()
            || self.deal_confirm.is_some()
            || self.dismiss_asked
            || self.homunculus_asked.is_some()
            || self.pet_asked.is_some()
            || self.cashshop_buy_asked
            || self.guild_asked.is_some()
            || self.guild_invite.is_some()
            || self.party_invite.is_some()
            || self.friend_asked.is_some()
            || self.deal_choice.is_some()
            || self.amount.is_some()
            || self.adopt_asked.is_some()
            || self.queue_asked.is_some()
            || self.booking_asked.is_some()
            || self.memorial_asked
    }

    /// The Apply question's answer: yes sends each pending level, what it needs first; no keeps them pending.
    pub(crate) fn answer_apply(&mut self, yes: bool) {
        self.apply_asked = false;
        if yes {
            for skill in self.skill_window.take_pending() {
                let _ = self.session.skill_up(skill);
            }
        }
    }

    /// Answers the open question with No.
    pub(crate) fn answer_no(&mut self, messages: &[String]) {
        if let Some((character, account, ..)) = self.booking_asked.take() {
            let _ = self.session.answer_join(character, account, false);
            return;
        }
        if self.save_asked {
            self.save_asked = false;
            return;
        }
        if self.memorial_asked {
            self.memorial_asked = false;
            return;
        }
        if self.apply_asked {
            self.apply_asked = false;
            return;
        }
        if self.amount.is_some() {
            self.answer_amount(false, messages);
            return;
        }
        if self.bind_asked.is_some() {
            self.answer_bind(false);
            return;
        }
        if self.expansion_asked.is_some() {
            self.expansion_asked = None;
            let _ = self.session.answer_expansion(false);
            return;
        }
        if self.leader_asked.take().is_some() {
            return;
        }
        if self.question_box.take().is_some() || self.deal_confirm.take().is_some() {
            return;
        }
        if self.dismiss_asked {
            self.answer_dismiss(false, messages);
            return;
        }
        if self.homunculus_asked.is_some() {
            self.decline_homunculus();
            return;
        }
        if self.pet_asked.is_some() {
            self.decline_pet();
            return;
        }
        if self.cashshop_buy_asked {
            self.cashshop_buy_asked = false;
            if let Some((free, items)) = self.cashshop.as_ref().and_then(|shop| shop.buy_request(false)) {
                let _ = self.session.buy_cashshop(free, &items);
            }
            return;
        }
        if let Some(GuildAsked::Ally { id, .. }) = self.guild_asked.take() {
            let _ = self.session.answer_alliance(id, false);
            return;
        }
        if self.reason_box.take().is_some() {
            return;
        }
        if let Some((guild, _)) = self.guild_invite.take() {
            let _ = self.session.answer_guild(guild, false);
            return;
        }
        if let Some((party, _)) = self.party_invite.take() {
            let _ = self.session.answer_party(party, false);
            return;
        }
        if let Some((account, character, _)) = self.friend_asked.take() {
            let _ = self.session.answer_friend(account, character, false);
            return;
        }
        if let Some((parent, partner, _, _)) = self.adopt_asked.take() {
            let _ = self.session.answer_adoption(parent, partner, false);
            return;
        }
        if let Some(asked) = self.queue_asked.take() {
            if let QueueAsked::Lobby(name, _) = asked {
                let _ = self.session.answer_lobby(false, &name);
            }
            return;
        }
        self.close_deal_choice();
    }

    /// Closes one window the way its Cancel or close button does, and drops it from the Esc list.
    pub(crate) fn close_esc(&mut self, id: escape::WindowId) {
        use escape::WindowId::*;
        match id {
            Inventory => self.inventory.frame.open = false,
            Equipment => self.equipment.frame.open = false,
            Skills => self.skill_window.frame.open = false,
            StorageCategory(tab) => self.storage.close_category(tab),
            StorageSearch => self.storage.search_window.frame.open = false,
            Party => self.party_window.frame.open = false,
            Status => self.status_window.frame.open = false,
            Guild => self.guild_window.frame.open = false,
            Clan => self.clan_window.frame.open = false,
            Cart => self.cart.frame.open = false,
            Pet => self.pet_window.frame.open = false,
            Mercenary => self.mercenary_window.frame.open = false,
            MercenarySkills => self.mercenary_skill_list.frame.open = false,
            Homunculus => self.homunculus_window.frame.open = false,
            HomunculusSkills => self.homunculus_skill_list.frame.open = false,
            // Esc never gets here for the input box, nor for the storage (its search box keeps Esc too).
            NpcInput | Storage => {}
            Shop => self.close_shop(),
            GradeEnchant => {
                let _ = self.session.close_grade();
                self.grade = None;
            }
            LaphineSynthesis => {
                let _ = self.session.close_synthesis();
                self.synthesis = None;
            }
            LaphineUpgrade => {
                let _ = self.session.close_upgrade();
                self.upgrade = None;
            }
            ItemReform => {
                let _ = self.session.close_reform();
                self.reform = None;
            }
            Enchant => {
                let _ = self.session.close_enchant();
                self.enchant = None;
            }
            Stylist => {
                let _ = self.session.close_stylist();
                self.stylist = None;
            }
            Materials => self.materials = None,
            PartySetup | PartyCreate | PartyInviteName => self.party_dialog = None,
            DressRoom => {
                self.dressroom = None;
                self.dress_figure = None;
            }
            Ranking => self.ranking = None,
            Reputation => self.reputation_window = None,
            Navigation => self.navi_window = None,
            Catalog => self.close_catalog(),
            SkillDescription => self.skill_description = None,
            SkillDelay => {
                if let Some(window) = &mut self.skill_description {
                    window.delays = false;
                }
            }
            Roulette => self.close_roulette(),
            MonsterInfo => self.monster_window = None,
            Achievements => self.achievement_window.frame.open = false,
            OtherEquipment => self.viewing = None,
            Bank => {
                let _ = self.session.close_bank(self.account_id);
                self.bank = None;
            }
            CashShop => self.close_cashshop(),
            PickList => self.answer_pick(pick_list::PickChoice::Cancel),
            // Only while typing the zeny: Esc stops that, the deal stays.
            Trade => {
                if let Some(trade) = &mut self.trade {
                    trade.typing = false;
                }
            }
            ItemInfo => {
                self.item_window = None;
                self.card_view = None;
            }
            CardPicture => self.card_view = None,
            CardInfo => self.card_window = None,
            ChatRoom => {
                let _ = self.session.leave_chat_room();
            }
            RoomMake => self.room_make = None,
            EmotionList => self.emotion_list = None,
            Macros => {
                if self.macro_window.take().is_some() {
                    macros::save(macros::path().as_deref(), &self.macro_lines);
                }
            }
            VendSetup => {
                let _ = self.session.open_shop("", false, &[]);
                self.vend_setup = None;
            }
            MyShop => {
                let _ = self.session.close_shop();
                self.my_shop = None;
                self.signs.remove(&self.account_id);
            }
            BuyingStoreSetup => {
                let _ = self.session.open_buying_store("", 0, false, &[]);
                self.buying_setup = None;
            }
            MyBuyingStore => {
                let _ = self.session.close_buying_store();
                self.my_buying_store = None;
                self.signs.remove(&self.account_id);
            }
            Mailbox => {
                self.mail.list.frame.open = false;
                let _ = self.session.close_mailbox();
            }
            MailRead => self.mail.letter = None,
            MailWrite => self.mail.compose = None,
            GameSettings => self.settings_window = None,
            ShortcutSettings => self.shortcuts_window = None,
            WorldMap => self.world_map = None,
            Minimap => {
                if self.minimap_window.naming() {
                    self.minimap_window.cancel_name();
                } else {
                    self.minimap_window.close();
                }
            }
            Quests => self.quest_window.close(),
            Menu => self.menu = None,
            Capturing => self.capturing = false,
            Amount => self.answer_amount(false, &[]),
            SaveAsked => self.save_asked = false,
            SkillApplyAsked => self.apply_asked = false,
            BindAsked => self.answer_bind(false),
            ExpansionAsked => {
                self.expansion_asked = None;
                let _ = self.session.answer_expansion(false);
            }
            LeaderAsked => {
                self.leader_asked = None;
            }
            PartyAsked | Notice => self.question_box = None,
            GuildCompanion => self.guild_companion.close(),
            GuildCreate | GuildDisband => self.guild_companion.create = None,
            GuildTip => self.guild_companion.tip = None,
            DismissAsked => self.answer_dismiss(false, &[]),
            HomunculusAsked => self.decline_homunculus(),
            PetAsked => self.decline_pet(),
            CashShopBuy => self.cashshop_buy_asked = false,
            GuildAsked => {
                self.guild_asked = None;
            }
            GuildInvite => {
                if let Some((guild, _)) = self.guild_invite.take() {
                    let _ = self.session.answer_guild(guild, false);
                }
            }
            PartyInvite => {
                if let Some((party, _)) = self.party_invite.take() {
                    let _ = self.session.answer_party(party, false);
                }
            }
            FriendAsked => {
                if let Some((account, character, _)) = self.friend_asked.take() {
                    let _ = self.session.answer_friend(account, character, false);
                }
            }
            DealChoice => self.close_deal_choice(),
            BattleQueue => self.queue_window.frame.open = false,
            Memorial => {
                self.memorial.frame.open = false;
                self.memorial_asked = false;
                escape::remove(&mut self.esc_order, escape::WindowId::MemorialAsked);
            }
            MemorialAsked => self.memorial_asked = false,
            AdoptAsked => {
                if let Some((parent, partner, _, _)) = self.adopt_asked.take() {
                    let _ = self.session.answer_adoption(parent, partner, false);
                }
            }
            QueueAsked => {
                if let Some(crate::QueueAsked::Lobby(name, _)) = self.queue_asked.take() {
                    let _ = self.session.answer_lobby(false, &name);
                }
            }
            Whisper => {
                if self.talks.typing() {
                    self.talks.blur();
                } else {
                    self.talks.close_front();
                }
            }
            FriendSetup => self.friend_setup = None,
            BlockList => self.block_list = None,
            AwayMessage => self.away_prompt = None,
        }
        escape::remove(&mut self.esc_order, id);
        if id == escape::WindowId::Whisper && !self.talks.is_empty() {
            escape::touch(&mut self.esc_order, escape::WindowId::Whisper);
        }
    }

    /// The window under a press, so Esc closes the one the player just touched.
    pub(crate) fn press_esc(&mut self, cursor: [f32; 2]) {
        if let Some(id) = self.esc_at(cursor) {
            escape::touch(&mut self.esc_order, id);
        }
    }

    pub(crate) fn esc_at(&self, cursor: [f32; 2]) -> Option<escape::WindowId> {
        use escape::WindowId::*;
        if self.menu.is_some() {
            return Some(Menu);
        }
        // An NPC shop isn't Esc's to close.
        if self.shop.as_ref().is_some_and(|shop| shop.covers(cursor)) {
            return self.shop.as_ref().filter(|shop| !shop.plain()).map(|_| Shop);
        }
        if self.world_map.is_some() {
            return Some(WorldMap);
        }
        if self.away_prompt.as_ref().is_some_and(|prompt| prompt.hit(cursor).is_some()) {
            return Some(AwayMessage);
        }
        if self.talks.hit(cursor).is_some() {
            return Some(Whisper);
        }
        if self.block_list.as_ref().is_some_and(|list| list.hit(cursor).is_some()) {
            return Some(BlockList);
        }
        if self.friend_setup.as_ref().is_some_and(|setup| setup.hit(cursor).is_some()) {
            return Some(FriendSetup);
        }
        if self.minimap_window.covers(cursor) {
            return Some(Minimap);
        }
        if self.memorial.covers(cursor) {
            return Some(Memorial);
        }
        self.window_at(cursor).and_then(esc_panel)
    }

    /// Asks to wear an item.
    pub(crate) fn wear(&mut self, index: u16, location: u32) {
        self.last_wear = Some(control::now());
        let _ = self.session.wear(index, location);
    }

    /// An inventory item let go over the world: dropped, unless something open or a fresh wear holds it back.
    pub(crate) fn drop_on_ground(&mut self, index: u16, messages: &[String]) {
        let since_wear = self.last_wear.map(|at| control::now().saturating_duration_since(at));
        // Nothing is dropped on the ground while the refine window is open.
        if self.refine.is_some() {
            return;
        }
        let busy = self.trade.is_some() || self.talk_blocks() || self.shop.is_some();
        let mail = self.mail.list.frame.open || self.mail.compose.is_some() || self.mail.letter.is_some();
        let ammo = self.inventory.get(index).is_some_and(|item| item.kind == net::ITEM_AMMO && item.worn != 0);
        match ground_drop_refused(since_wear, busy, self.equipment.frame.open, self.storage.frame.open, mail, ammo) {
            None => self.move_item(Asked::Drop(index)),
            Some(Some(id)) => self.chat_line(messages.get(id).cloned().unwrap_or_default(), CHAT_ERROR),
            Some(None) => {}
        }
    }

    /// Uses an inventory item: consumables are used, equipment is put on.
    pub(crate) fn use_item(&mut self, index: u16, messages: &[String]) {
        if self.store_open() {
            return;
        }
        if let Some(Some(id)) = trade::refusal(self.trade.is_some(), trade::ItemAction::Use) {
            self.chat_line(messages.get(id).cloned().unwrap_or_default(), CHAT_ERROR);
            return;
        }
        let Some(item) = self.inventory.get(index) else { return };
        // While the refine window is open, a double click on equipment that is not worn puts it in (an unidentified one is
        // refused with a timed notice) instead of wearing it.
        if self.refine.is_some() && matches!(item.kind, net::ITEM_WEAPON | net::ITEM_ARMOR | net::ITEM_SHADOW_GEAR) && item.location != 0 && item.worn == 0 {
            let identified = item.identified;
            if let Some(window) = &mut self.refine
                && window.put(identified, control::now())
            {
                let _ = self.session.refine_select(index);
            }
            return;
        }
        // ponytail: a voucher asks to grow the inventory; how the real client lets the player ask isn't known.
        if VOUCHERS.contains(&item.id) {
            let _ = self.session.ask_expansion();
            return;
        }
        match item.kind {
            // A reform box is a delayed consume: using it only opens the window.
            net::ITEM_HEALING | net::ITEM_USABLE | net::ITEM_DELAY_CONSUME | net::ITEM_CASH => {
                let _ = self.session.use_item(index, self.account_id);
            }
            // An egg (armor worn nowhere) hatches from the egg list only.
            net::ITEM_ARMOR if item.location == 0 => {}
            // An item that binds once worn asks first.
            net::ITEM_WEAPON | net::ITEM_ARMOR | net::ITEM_SHADOW_GEAR if item.identified && item.bind == 1 => self.bind_asked = Some(index),
            net::ITEM_WEAPON | net::ITEM_ARMOR | net::ITEM_SHADOW_GEAR | net::ITEM_PET_ARMOR | net::ITEM_AMMO if item.identified => {
                let location = item.location;
                self.wear(index, location);
            }
            // A card asks what it can go into.
            net::ITEM_CARD => {
                self.card_asked = Some(index);
                let _ = self.session.card_list(index);
            }
            _ => {}
        }
    }

    /// Closes the shop's windows; a market or barter tells the server, which keeps the player busy until it hears.
    /// The shop's windows close without a deal: a market or barter says so, and so does an NPC shop.
    pub(crate) fn close_shop(&mut self) {
        let _ = match self.shop.take() {
            Some(shop) if shop.plain() => self.session.quit_npc_trade(),
            Some(shop) => shop.market.map_or(Ok(()), |kind| self.session.close_market(kind)),
            None => Ok(()),
        };
    }

    pub(crate) fn close_cashshop(&mut self) {
        if self.cashshop.take().is_some() {
            let _ = self.session.close_cashshop();
        }
        self.cashshop_buy_asked = false;
    }

    /// Closes the deal-type question and drops its drag.
    pub(crate) fn close_deal_choice(&mut self) {
        self.deal_choice = None;
        self.deal_frame.open = false;
        self.deal_frame.drop_drag();
    }

    /// The answer to a pick list: the row picked, Cancel, or Close.
    pub(crate) fn answer_pick(&mut self, choice: pick_list::PickChoice) {
        let Some((list, picking)) = self.pick.take() else { return };
        if matches!(choice, pick_list::PickChoice::Close) {
            return;
        }
        let picked = match choice {
            pick_list::PickChoice::Pick(row) => list.items.get(row),
            _ => None,
        };
        let _ = match (picking, picked) {
            (Picking::Card(card), Some(equip)) => self.session.insert_card(card, equip.index),
            // rathena keeps nothing for a card's list, nor for Arrow Crafting's.
            (Picking::Card(_) | Picking::Arrow, None) => Ok(()),
            // The server waits for the Magnifier's answer, a cancel too.
            (Picking::Identify, item) => self.session.identify(item.map(|item| item.index)),
            // A making list unanswered isn't sent again: a cancel answers item 0.
            (Picking::Make, Some(item)) => {
                if forge::skips_material_window(self.me.job) {
                    self.session.make(item.id, [0, 0, 0])
                } else {
                    self.forge = Some(forge::Forge::new(list.frame.position, item.id));
                    Ok(())
                }
            }
            (Picking::Make, None) => self.session.make(0, [0, 0, 0]),
            (Picking::Cook(kind), item) => self.session.cook(kind, item.map_or(0, |item| item.id)),
            (Picking::Arrow, Some(item)) => self.session.make_arrow(item.id),
            // Both lists end with an answer, a cancel too.
            (Picking::Repair, item) => self.session.repair(item),
            // Both hold the player until answered; 0 starts nothing.
            (Picking::Spell, skill) => self.session.select_autospell(skill.map_or(0, |skill| skill.id)),
            (Picking::SkillSelect(why), skill) => self.session.select_skill(why, skill.map_or(0, |skill| skill.id as u16)),
            (Picking::Upgrade, item) => self.session.upgrade_weapon(item.map_or(u32::MAX, |item| u32::from(item.index))),
            // An egg list left unanswered is let go (rathena drops it on the next skill).
            (Picking::Hatch, Some(egg)) => {
                self.pet_window.egg = Some(egg.id);
                self.session.hatch(egg.index)
            }
            (Picking::Hatch, None) => Ok(()),
            (Picking::Merge, _) => self.session.cancel_merge(),
        };
    }

    pub(crate) fn forget_effect(&mut self, unit: u32, effect: u32) {
        let key = (unit != self.account_id).then_some(unit);
        self.effects.forget_effect(key, effect);
    }

    pub(crate) fn cancel_forge(&mut self) {
        if let Some(mut forge) = self.forge.take() {
            for slot in &mut forge.slots {
                if let Some(item) = slot.take() {
                    self.inventory.extend(vec![item]);
                }
            }
            let _ = self.session.make(0, [0, 0, 0]);
        }
    }

    pub(crate) fn ok_forge(&mut self) {
        if let Some(forge) = self.forge.take() {
            let materials =
                [forge.slots[0].as_ref().map_or(0, |i| i.id), forge.slots[1].as_ref().map_or(0, |i| i.id), forge.slots[2].as_ref().map_or(0, |i| i.id)];
            let _ = self.session.make(forge.item, materials);
        }
    }

    pub(crate) fn take_forge_slot(&mut self, slot: usize) {
        if let Some(forge) = &mut self.forge
            && let Some(item) = forge.take_slot(slot)
        {
            self.inventory.extend(vec![item]);
        }
    }

    /// Answers the bind question: OK wears the item.
    pub(crate) fn answer_bind(&mut self, yes: bool) {
        let Some(index) = self.bind_asked.take().filter(|_| yes) else { return };
        if let Some(location) = self.inventory.get(index).map(|item| item.location) {
            self.wear(index, location);
        }
    }

    /// A ground skill goes out; a Talkie Box carries the message typed for it.
    pub(crate) fn send_skill_at(&mut self, id: u16, level: u16, cell: [u16; 2]) -> std::io::Result<()> {
        match self.talkie_text.take().filter(|_| id == TALKIE_BOX) {
            Some(text) => self.session.use_skill_at_with_text(id, level, cell, &text),
            None => self.session.use_skill_at(id, level, cell),
        }
    }

    /// Closes the roulette's window; the server answers nothing.
    pub(crate) fn close_roulette(&mut self) {
        let _ = self.session.roulette(net::RouletteRequest::Close);
        self.roulette = None;
    }

    /// A command to the pet; sent back to its egg, it's gone and its window closes.
    pub(crate) fn pet_command(&mut self, command: u8) {
        let _ = self.session.pet_command(command);
        if command == net::PET_RETURN {
            (self.pet_id, self.pet) = (None, None);
            self.pet_window.frame.open = false;
        }
    }

    /// Feeding, from the command list or the menu: not while a mail is written, else asked first.
    pub(crate) fn ask_feed(&mut self, messages: &[String]) {
        if self.mail.compose.is_some() {
            self.chat_line(messages.get(pet::FEED_WHILE_MAIL).cloned().unwrap_or_default(), CHAT_ERROR);
        } else {
            self.pet_asked = Some(PetAsked::Feed);
        }
    }

    /// The pet window's rename button, on what the name box holds: turned down, or asked.
    pub(crate) fn ask_rename(&mut self) {
        let Some(pet) = &self.pet else { return };
        let name = self.pet_window.name(pet).to_owned();
        self.pet_window.typing = false;
        self.pet_asked = Some(match pet::rename_refusal(&name, pet::NAME_TOO_LONG) {
            Some(why) => PetAsked::Refused(why),
            None => PetAsked::Rename(name),
        });
    }

    /// Answers the pet question. Feeding: not while trading; without the pet's food in the bag it says so and sends
    /// nothing. Renaming: sent, the window shows the new name at once.
    pub(crate) fn answer_pet(&mut self, yes: bool, pets: &pet::Data, items: &HashMap<u32, ItemInfo>, messages: &[String]) {
        if !yes {
            return self.decline_pet();
        }
        match self.pet_asked.take() {
            Some(PetAsked::Feed) => {
                let food = self.pet.as_ref().and_then(|pet| pets.tables.food.get(&pet.monster)).copied();
                // ponytail: a pet the food table leaves out is fed, and the server decides.
                let has_food = food.is_none_or(|food| self.inventory.items.iter().any(|item| item.id == food));
                match pet::feeding(self.trade.is_some(), has_food) {
                    pet::Feeding::Trading => self.chat_line(messages.get(pet::FEED_WHILE_TRADING).cloned().unwrap_or_default(), CHAT_ERROR),
                    pet::Feeding::NoFood => {
                        let name = inventory::item_name(items, food.unwrap_or_default(), true, 0);
                        self.chat_line(rustnarok_client::messages::fill(messages.get(PET_NO_FOOD).map_or("", String::as_str), &[&name]), PET_NO_FOOD_COLOR);
                    }
                    pet::Feeding::Send => drop(self.session.pet_command(net::PET_FEED)),
                }
            }
            Some(PetAsked::Rename(name)) => {
                let _ = self.session.rename_pet(&name);
                if let Some(pet) = &mut self.pet {
                    pet.name = name;
                }
                self.pet_window.name_text = None;
            }
            Some(PetAsked::Refused(_)) | None => {}
        }
    }

    /// Our pet says a line for an act at a hunger, when it's fond enough of us to talk.
    pub(crate) fn pet_talk(&mut self, act: u8, hunger: u16) {
        if let Some(talk) = self.pet.as_ref().and_then(|pet| pet::talk(pet, act, hunger)) {
            let _ = self.session.pet_act(&talk);
        }
    }

    /// No to the pet question: a refused rename puts the old name back in the box.
    pub(crate) fn decline_pet(&mut self) {
        if let Some(PetAsked::Rename(_)) = self.pet_asked.take() {
            self.pet_window.name_text = None;
        }
    }

    /// The mercenary is gone (dismissed, its contract over, dead): its windows close. On a map change the server sends
    /// it again, so they stay as they were.
    pub(crate) fn forget_mercenary(&mut self, close: bool) {
        self.mercenary = None;
        self.mercenary_skills.clear();
        self.steering = mercenary::Steering::default();
        self.dismiss_asked = false;
        if close {
            self.mercenary_window.frame.open = false;
            self.mercenary_skill_list.frame.open = false;
        }
    }

    /// Answers the dismiss question: OK lets the mercenary go.
    pub(crate) fn answer_dismiss(&mut self, yes: bool, messages: &[String]) {
        self.dismiss_asked = false;
        if yes && self.mercenary.is_some() {
            let _ = self.session.mercenary_command(net::MERCENARY_DISMISS);
            self.forget_mercenary(true);
            // ponytail: rathena answers nothing; when the official client prints this isn't knowable.
            self.chat_line(messages.get(mercenary::RELEASED).cloned().unwrap_or_default(), CHAT_ITEM);
        }
    }

    /// Sends the mercenary or homunculus `id` at the monster under the cursor, or to the cell there; the order it now
    /// follows, if any.
    /// An Alt press for a companion (the mercenary, or the homunculus): on a monster it attacks it, at once with
    /// Shift, otherwise the first press marks it and a second on the same one sends the attack; on the ground it goes
    /// there. False when that companion isn't out (Alt does nothing special then).
    pub(crate) fn companion_press(&mut self, homunculus: bool, cursor: [f32; 2], shift: bool) -> bool {
        let id = if homunculus { self.homunculus_id.filter(|id| self.units.contains_key(id)) } else { self.mercenary.as_ref().map(|hired| hired.id) };
        let Some(id) = id else { return false };
        let monster = match hovered(&self.hit_boxes, cursor, false) {
            Some(Owner::Unit(target)) if self.units.get(&target).is_some_and(|actor| actor.kind == MONSTER) => Some(target),
            _ => None,
        };
        let order = match monster {
            Some(target) => {
                let (mark, attack) = companion_mark(self.companion_mark, id, target, shift);
                self.companion_mark = mark;
                // ponytail: rathena reads non-zero as "keep attacking" and says the client sends 0; 1 keeps it at it.
                attack.then(|| {
                    let _ = self.session.act_npc(id, target, 1);
                    mercenary::Order::Attack(target)
                })
            }
            None => {
                self.companion_mark = None;
                self.ground_cell(cursor).filter(|&cell| self.world.gat.walkable(cell)).map(|cell| {
                    let _ = self.session.move_npc(id, cell.map(|value| value as u16));
                    mercenary::Order::Move(self.player.cell)
                })
            }
        };
        if let Some(order) = order {
            let steering = if homunculus { &mut self.homunculus_steering } else { &mut self.steering };
            steering.order = Some(order);
        }
        true
    }

    /// Calls the mercenary or homunculus `id` back to the player when `steering` says so; an attack order lasts while
    /// its target is there.
    pub(crate) fn follow_player(&mut self, id: u32, homunculus: bool, now: Instant) {
        let Some(cell) = self.units.get(&id).map(|actor| actor.cell) else { return };
        let steering = if homunculus { &mut self.homunculus_steering } else { &mut self.steering };
        if let Some(mercenary::Order::Attack(target)) = steering.order
            && self.units.get(&target).is_none_or(|actor| actor.remove_at.is_some())
        {
            steering.order = None;
        }
        if steering.should_call(cell, self.player.cell, now) {
            let _ = self.session.move_to_owner(id);
        }
    }

    /// Uses a skill of the mercenary's or the homunculus's.
    pub(crate) fn use_companion_skill(&mut self, id: u16, level: u16) {
        if self.homunculus_skills.iter().any(|skill| skill.id == id) {
            self.use_homunculus_skill(id, level);
        } else {
            self.use_mercenary_skill(id, level);
        }
    }

    /// Uses one of the homunculus's skills: on the homunculus at once when it's a self skill, otherwise aimed.
    pub(crate) fn use_homunculus_skill(&mut self, id: u16, level: u16) {
        let (Some(unit), Some(skill)) = (self.homunculus_id, self.homunculus_skills.iter().find(|skill| skill.id == id)) else { return };
        let level = level.clamp(1, skill.level.max(1));
        if skill.kind & net::Skill::SELF != 0 {
            let _ = self.session.use_skill(id, level, unit);
        } else if skill.kind & (net::Skill::ENEMY | net::Skill::GROUND | net::Skill::FRIEND) != 0 {
            self.targeting = Some((id, level));
        }
    }

    /// The homunculus is gone: its windows close when asked to. On a map change the server sends it again, so they
    /// stay as they were.
    pub(crate) fn forget_homunculus(&mut self, close: bool) {
        self.homunculus = None;
        self.homunculus_id = None;
        self.homunculus_skills.clear();
        self.homunculus_steering = mercenary::Steering::default();
        self.homunculus_asked = None;
        self.homunculus_warned = None;
        self.homunculus_window.name_text = None;
        self.homunculus_window.typing = false;
        if close {
            self.homunculus_window.frame.open = false;
            self.homunculus_skill_list.frame.open = false;
        }
    }

    /// The homunculus window's rename button, on what the name box holds: turned down, or asked.
    pub(crate) fn ask_homunculus_rename(&mut self) {
        let Some(homunculus) = &self.homunculus else { return };
        let name = self.homunculus_window.name(homunculus).to_owned();
        self.homunculus_window.typing = false;
        self.homunculus_asked = Some(match pet::rename_refusal(&name, homunculus::NAME_TOO_LONG) {
            Some(why) => HomunculusAsked::Refused(why),
            None => HomunculusAsked::Rename(name),
        });
    }

    /// Answers the homunculus question. Feeding: not while trading. Deleting: it's gone. Renaming: sent (rathena turns
    /// down a second one).
    pub(crate) fn answer_homunculus(&mut self, yes: bool, messages: &[String]) {
        if !yes {
            return self.decline_homunculus();
        }
        match self.homunculus_asked.take() {
            Some(HomunculusAsked::Feed) if self.trade.is_some() => {
                self.chat_line(messages.get(homunculus::FEED_WHILE_TRADING).cloned().unwrap_or_default(), CHAT_ERROR);
            }
            Some(HomunculusAsked::Feed) => drop(self.session.homunculus_command(net::HOMUNCULUS_FEED)),
            Some(HomunculusAsked::Delete) => {
                let _ = self.session.homunculus_command(net::HOMUNCULUS_DELETE);
                self.forget_homunculus(true);
            }
            Some(HomunculusAsked::Rename(name)) => {
                let _ = self.session.rename_homunculus(&name);
                self.homunculus_window.name_text = None;
            }
            Some(HomunculusAsked::Refused(_)) | None => {}
        }
    }

    /// No to the homunculus question: a refused rename puts the old name back in the box.
    pub(crate) fn decline_homunculus(&mut self) {
        if let Some(HomunculusAsked::Rename(_)) = self.homunculus_asked.take() {
            self.homunculus_window.name_text = None;
        }
    }

    /// Uses one of the mercenary's skills: on the mercenary at once when it's a self skill, otherwise aimed.
    pub(crate) fn use_mercenary_skill(&mut self, id: u16, level: u16) {
        let (Some(mercenary), Some(skill)) = (&self.mercenary, self.mercenary_skills.iter().find(|skill| skill.id == id)) else { return };
        let level = level.clamp(1, skill.level.max(1));
        if skill.kind & net::Skill::SELF != 0 {
            let _ = self.session.use_skill(id, level, mercenary.id);
        } else if skill.kind & (net::Skill::ENEMY | net::Skill::GROUND | net::Skill::FRIEND) != 0 {
            self.targeting = Some((id, level));
        }
    }

    /// Presses a hotkey slot: uses the item it holds (the first stack of it carried).
    pub(crate) fn press_hotkey(&mut self, slot: usize, messages: &[String]) {
        if let Some(Some(net::Hotkey { skill: true, id, count })) = self.hotkeys.keys.get(slot).copied() {
            return self.use_skill(id as u16, count.max(1) as u16);
        }
        if let Some(Some(net::Hotkey { skill: false, id, .. })) = self.hotkeys.keys.get(slot).copied()
            && let Some(index) = self.inventory.items.iter().find(|item| item.id == id && item.worn == 0).map(|item| item.index)
        {
            self.use_item(index, messages);
        }
    }

    /// Units locked together, turned to stone or frozen hold still on the frame they had when caught; the others move
    /// again.
    pub(crate) fn hold_locked(&mut self) {
        let now = control::now();
        let me = self.account_id;
        let locks = &self.locks;
        let actors = std::iter::once((me, &mut self.player)).chain(self.units.iter_mut().map(|(id, actor)| (*id, actor)));
        for (id, actor) in actors {
            actor.held = (locked_with(locks, id) > 0 || actor::stilled(actor.body_state)).then(|| actor.held.unwrap_or(now));
        }
    }

    pub(crate) fn chat_line(&mut self, text: String, color: [u8; 3]) {
        self.chat.push(text, color);
    }

    /// A chat line of a kind its colour doesn't tell (whispers share the broadcasts' yellow).
    pub(crate) fn chat_line_kind(&mut self, text: String, color: [u8; 3], kind: chat::Kind) {
        self.chat.push_kind(text, color, kind);
    }

    /// Starts the attacker's swing now; the number, the flinch and the sparks come when the hit lands, which is when
    /// this returns (now, for what isn't a hit).
    /// The numbers shown on the player in the last 3 s: amount, kind and how long ago (for the test driver).
    pub(crate) fn hits_taken(&self, now: Instant) -> Vec<(i32, &'static str, u64)> {
        crate::hits_taken(&self.damages, now)
    }

    pub(crate) fn act(&mut self, act: net::Act, tables: &LookTables, grf: &Grf) -> Instant {
        // Sitting down and standing up.
        if matches!(act.kind, 2 | 3) {
            let who = if act.source == self.account_id { Some(&mut self.player) } else { self.units.get_mut(&act.source) };
            if let Some(actor) = who {
                actor.sitting = act.kind == 2;
            }
            return Instant::now();
        }
        // Picking an item up: bend toward it.
        if act.kind == 1 {
            let item = self.ground.get(&act.target).map(|item| item.at);
            let picker = if act.source == self.account_id { Some(&mut self.player) } else { self.units.get_mut(&act.source) };
            if let Some(picker) = picker.filter(|actor| actor.kind == PLAYER) {
                if let Some(cell) = item {
                    picker.face(cell);
                }
                let duration = picker.sprites.as_ref().and_then(|sprites| sprites.action_ms(actor::PICKUP, picker.direction)).unwrap_or(500.0);
                picker.play(actor::PICKUP, Instant::now(), duration, false);
            }
            return Instant::now();
        }
        // Only hits animate (normal, endure, multi-hit, critical, lucky dodge, ...).
        if !matches!(act.kind, 0 | 4 | 8 | 9 | 10 | 11 | 13) {
            return Instant::now();
        }
        let me = self.account_id;
        let target_cell = if act.target == me { Some(self.player.cell) } else { self.units.get(&act.target).map(|actor| actor.cell) };
        let source = if act.source == me { Some(&mut self.player) } else { self.units.get_mut(&act.source) };
        let mut hit_ms = None;
        let mut bow = false;
        if let Some(source) = source {
            if let Some(cell) = target_cell {
                source.face(cell);
            }
            source.note_hit(&act, crate::control::now());
            let attack = source.attack_motion(tables);
            bow = source.holds_bow(tables);
            let scale = actor::attack_scale(act.source_speed, bow);
            source.play_at_pace(attack, Instant::now(), scale, false);
            source.stand_ready();
            hit_ms = source.sprites.as_ref().and_then(|sprites| sprites.hit_ms(attack * 8 + usize::from(source.direction % 8), scale));
        }
        // A bow shoots its arrow at the swing's hit frame; the hit lands with it.
        if let Some(ms) = hit_ms.filter(|_| bow) {
            self.shoot_arrow(grf, act.source, act.target, ms);
        }
        let hit_ms = hit_ms.map(|ms| actor::bow_flight(ms, bow));
        self.show_hit(&act, hit_ms, MULTI_HIT_DELAY, HitFrom::Attack);
        hit_lands(hit_ms, act.source_speed)
    }

    /// A unit's name for a battle message: ours, the one the map server told, else "Someone".
    fn battle_name(&self, id: u32, messages: &[String]) -> String {
        let name = if id == self.account_id { self.me.name.as_str() } else { self.units.get(&id).map_or("", |actor| actor.name.as_str()) };
        // Names come as "name#party": only the name shows.
        let name = name.split('#').next().unwrap_or_default();
        if name.is_empty() { messages.get(BATTLE_SOMEONE).map_or("Someone", String::as_str).trim_end_matches('#').to_owned() } else { name.to_owned() }
    }

    /// The battle messages of a hit (categories 16 and 17): see `chat::battle_line`.
    pub(crate) fn battle_lines(&mut self, act: &net::Act, messages: &[String]) {
        let me = self.account_id;
        let role = |id: u32| {
            if id == me {
                chat::Role::Me
            } else if self.party.as_ref().is_some_and(|party| party.members.iter().any(|member| member.info.account == id)) {
                chat::Role::Party
            } else {
                chat::Role::Other
            }
        };
        let (source, target) = ((role(act.source), self.battle_name(act.source, messages)), (role(act.target), self.battle_name(act.target, messages)));
        if let Some((text, color, kind)) = chat::battle_line(messages, (source.0, &source.1), (target.0, &target.1), act.damage + act.left_damage) {
            self.chat_line_kind(text, color, kind);
        }
    }

    /// The numbers of a hit as it lands: split over a multi-hit's hits (with the running total over monsters),
    /// yellow for a critical, "Lucky" for a lucky dodge.
    /// The numbers (and the flinch) of a hit, landing `hit_ms` after now (the attacker's "atk" frame), or after its
    /// attack motion when its sprite doesn't say.
    pub(crate) fn show_hit(&mut self, act: &net::Act, hit_ms: Option<f32>, gap: Duration, from: HitFrom) {
        let me = self.account_id;
        let target = (act.target != me).then_some(act.target);
        let hit = match target {
            None => Some(&self.player),
            Some(id) => self.units.get(&id),
        };
        let on_player = red_number(hit.map(|actor| actor.job), target.is_some_and(|id| self.skill_units.contains_key(&id)));
        // Hits drift diagonally away from where the target faces.
        let drift = hit.map_or([1.0, 1.0], |actor| actor::number_drift(actor.rot, actor.facing_from_server));
        // A swing's left-hand damage (a katar's, the second weapon's) is a number of its own after the right hand's.
        let attacker = if act.source == me { Some(&self.player) } else { self.units.get(&act.source) };
        let left = left_hand_number(from, act.left_damage, attacker.map(|actor| actor.job));
        let amount = act.damage;
        let lands = hit_lands(hit_ms, act.source_speed);
        let flinch = (amount > 0 && !endured(act.kind, from)).then_some(act.target_speed as f32);
        let kind = match act.kind {
            11 => DamageKind::Lucky,
            10 | 13 => DamageKind::Critical,
            _ if amount + left.unwrap_or(0) <= 0 => {
                // MISS shows over the attacker: white when it's a monster or an NPC, red otherwise.
                let attacker = if act.source == me { Some(&self.player) } else { self.units.get(&act.source) };
                let red = attacker.is_none_or(|actor| !(actor::is_monster_job(actor.job) || actor::is_npc_job(actor.job)));
                let over = (act.source != me).then_some(act.source);
                // A skill's multi-hit misses each of its hits.
                let misses = if from == HitFrom::Skill && matches!(act.kind, 8 | 9 | 13) { u32::from(act.hits.max(1)) } else { 1 };
                for miss in 0..misses {
                    self.damages.push(Damage {
                        target: over,
                        amount,
                        start: lands + gap * miss,
                        kind: DamageKind::Miss { red },
                        flinch: None,
                        drift,
                        source: None,
                    });
                }
                return;
            }
            _ => DamageKind::Hit { taken: on_player, sp: act.sp },
        };
        let hits = if amount > 0 { i32::from(act.hits.max(1)) } else { 1 };
        // A swing's critical has its own sound (EF_HIT2's) in place of the weapon's hit sound.
        let heard = (from != HitFrom::Attack || kind != DamageKind::Critical).then_some(act.source);
        for hit in 0..hits {
            let start = lands + gap * hit as u32;
            self.damages.push(Damage { target, amount: amount / hits, start, kind, flinch, drift, source: heard.filter(|_| amount > 0) });
            if shows_total(hits, from) && !on_player {
                self.damages.push(Damage { target, amount: amount / hits * (hit + 1), start, kind: DamageKind::Combo, flinch: None, drift, source: None });
            }
        }
        // ponytail: a multi-hit swing's left hand (no total with it); a katar never double-attacks.
        if let Some(left) = left {
            self.damages.push(Damage { target, amount: left, start: lands + LEFT_HAND_DELAY, kind, flinch, drift, source: heard });
        }
    }
}

/// When a hit lands: `hit_ms` from now, else after the swing's `speed`.
fn hit_lands(hit_ms: Option<f32>, speed: i32) -> Instant {
    Instant::now() + Duration::from_secs_f32(hit_ms.unwrap_or(speed.max(0) as f32) / 1000.0)
}

/// Whether a hit's number is red: on anything but monsters, NPCs and skill units on the ground (Ice Wall, traps), so on
/// players (us too), homunculi and mercenaries. `job` is the hit unit's, None when it isn't a unit seen.
// ponytail: a skill unit's numbers take a monster's white.
fn red_number(job: Option<u16>, ground_unit: bool) -> bool {
    !ground_unit && job.is_none_or(|job| !(actor::is_monster_job(job) || actor::is_npc_job(job)))
}

/// A splash skill's preamble on its first target (Grimtooth's damage −30000, type 6): its hit effects, and no number,
/// MISS or flinch.
pub(crate) fn preamble(act: &net::Act) -> bool {
    act.kind == 6 && act.damage < 0
}

/// Whether a hit doesn't flinch its target: misses aside, an endured hit or a lucky dodge. A skill's endured multi-hit
/// does flinch (each hit comes with a running total, which no longer counts as endured), and so does every hit from a
/// skill's ground unit.
/// A splash's other target doesn't flinch when it endures (type 14).
fn endured(kind: u8, from: HitFrom) -> bool {
    match from {
        HitFrom::Attack => matches!(kind, 4 | 9 | 11),
        HitFrom::Skill => matches!(kind, 4 | 11),
        HitFrom::Splash => kind == 14,
        HitFrom::Unit => false,
    }
}

/// Whether a monster's hits show the running total too: a multi-hit's, and every hit from a skill's ground unit.
fn shows_total(hits: i32, from: HitFrom) -> bool {
    hits > 1 || from == HitFrom::Unit
}

/// How long after a swing's first number its left-hand number comes.
const LEFT_HAND_DELAY: Duration = Duration::from_millis(350);

/// A swing's left-hand number, when its attacker (by job) shows one: the Assassin (12), every job past 4000, and the
/// NPC (45 – 999) and 10001 – 19998 ranges. Other attackers' left-hand damage isn't shown.
fn left_hand_number(from: HitFrom, damage: i32, attacker: Option<u16>) -> Option<i32> {
    let shows = |job: u16| job == 12 || job > 4000 || (45..1000).contains(&job) || (10001..=19998).contains(&job);
    (from == HitFrom::Attack && damage > 0 && attacker.is_some_and(shows)).then_some(damage)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hits_on_ice_wall_show_white() {
        // A player, a monster (Poring), an Ice Wall's unit, and an unknown target.
        assert!(red_number(Some(0), false));
        assert!(!red_number(Some(1002), false));
        assert!(!red_number(None, true));
        assert!(red_number(None, false));
    }
    use rustnarok_client::map::Gat;

    #[test]
    fn a_swings_left_hand_is_a_number_of_its_own() {
        // An Assassin's and an Assassin Cross's katar; a Kagerou's second dagger.
        assert_eq!(left_hand_number(HitFrom::Attack, 80, Some(12)), Some(80));
        assert_eq!(left_hand_number(HitFrom::Attack, 80, Some(4013)), Some(80));
        assert_eq!(left_hand_number(HitFrom::Attack, 80, Some(4211)), Some(80));
        // Not for a Knight, a Rogue or a monster, without left-hand damage, or for a skill.
        assert_eq!(left_hand_number(HitFrom::Attack, 80, Some(7)), None);
        assert_eq!(left_hand_number(HitFrom::Attack, 80, Some(17)), None);
        assert_eq!(left_hand_number(HitFrom::Attack, 80, Some(1002)), None);
        assert_eq!(left_hand_number(HitFrom::Attack, 0, Some(12)), None);
        assert_eq!(left_hand_number(HitFrom::Skill, 80, Some(12)), None);
        assert_eq!(left_hand_number(HitFrom::Attack, 80, None), None);
        assert_eq!(LEFT_HAND_DELAY, MULTI_HIT_DELAY + MULTI_HIT_DELAY / 4 + MULTI_HIT_DELAY / 2);
    }

    #[test]
    fn skill_hits_flinch_and_total_as_the_game() {
        // An endured hit: an attack's double and a skill's single don't flinch; a skill's multi-hit and a ground unit's do.
        assert!(endured(9, HitFrom::Attack) && endured(4, HitFrom::Skill));
        assert!(!endured(9, HitFrom::Skill) && !endured(4, HitFrom::Unit) && !endured(9, HitFrom::Unit));
        // A splash's other target: endured (14) it doesn't flinch, else (5) it does; on Napalm Beat's whole path, 14 does.
        assert!(endured(14, HitFrom::Splash) && !endured(5, HitFrom::Splash) && !endured(14, HitFrom::Skill));
        // A ground unit's single hit shows the total; a skill's single hit doesn't.
        assert!(shows_total(1, HitFrom::Unit) && !shows_total(1, HitFrom::Skill) && shows_total(2, HitFrom::Skill));
        // Grimtooth's preamble (type 6, −30000) shows its hit effects alone; a real miss (0) and a hit aren't one.
        let act = |kind, damage| net::Act { source: 1, target: 2, source_speed: 432, target_speed: 288, damage, sp: false, hits: 1, kind, left_damage: 0 };
        assert!(preamble(&act(6, -30000)) && !preamble(&act(8, 0)) && !preamble(&act(8, 120)) && !preamble(&act(6, 50)));
    }

    #[test]
    fn units_block_where_they_stand() {
        let gat = Gat { width: 30, height: 30, cells: vec![([0.0; 4], 0); 900] };
        let now = Instant::now();
        let mut units = HashMap::new();
        let mut monster = Actor::new("Poring".into(), None, [10, 10], 0, 150);
        monster.job = 1002;
        units.insert(1, monster);
        let skill_units = HashMap::new();
        let goal = |units: &HashMap<u32, Actor>, from: [i32; 2], click: [i32; 2]| {
            let taken = blocked_cells(units, &skill_units, now);
            world::walk_goal(click, from, |cell| gat.walkable(cell) && !taken.contains(&cell), |cell| world::find_path(&gat, from, cell).is_some())
        };
        // A standing monster on (10, 10): a click there from (15, 12) goes to (11, 10).
        assert_eq!(goal(&units, [15, 12], [10, 10]), Some([11, 10]));
        // Walking, it blocks nothing.
        units.get_mut(&1).unwrap().walk_from([10, 10], [8, 8], vec![[10, 11], [10, 12]], now, now, true);
        assert_eq!(goal(&units, [15, 12], [10, 10]), Some([10, 10]));
        // Stopped on (12, 10), it blocks that cell.
        units.get_mut(&1).unwrap().stop_at([12, 10]);
        assert!(blocked_cells(&units, &skill_units, now).contains(&[12, 10]));
        // A dead one still blocks while it fades.
        units.get_mut(&1).unwrap().remove_at = Some(now + std::time::Duration::from_secs(1));
        assert!(blocked_cells(&units, &skill_units, now).contains(&[12, 10]));
        // An Ice Wall blocks its cell; a dummy unit (0x86), Safety Wall (0x7e), Pneuma (0x85) and a trap (0x8a) don't.
        let mut skill_units = HashMap::new();
        for (id, (kind, cell)) in [(0x8d, [3, 3]), (0x86, [4, 4]), (0x7e, [5, 5]), (0x85, [6, 6]), (0x8a, [7, 7])].into_iter().enumerate() {
            skill_units.insert(id as u32, (kind, cell));
        }
        let taken = blocked_cells(&HashMap::new(), &skill_units, now);
        assert_eq!(taken, std::collections::HashSet::from([[3, 3]]));
        // A click on a Safety Wall's cell walks onto it.
        let walk = world::walk_goal([5, 5], [9, 5], |cell| gat.walkable(cell) && !taken.contains(&cell), |cell| world::find_path(&gat, [9, 5], cell).is_some());
        assert_eq!(walk, Some([5, 5]));
        // A hiding player doesn't; a job-104 NPC does; a warp doesn't.
        assert!(!unit_blocks(4001, false, 0x2) && unit_blocks(4001, false, 0x4));
        assert!(unit_blocks(104, false, 0) && !unit_blocks(45, false, 0) && !unit_blocks(111, false, 0) && !unit_blocks(139, false, 0));
    }

    #[test]
    fn judges_clicks_from_the_drawn_cell() {
        // Walking (10, 10) → (11, 10) → (12, 11) at 150 ms a cell: at 100 ms it's on (11, 10), at 300 ms on (12, 11).
        let start = Instant::now();
        let mut player = Actor::new("Me".into(), None, [10, 10], 0, 150);
        player.step_ms = 150.0;
        player.walk_from([10, 10], [8, 8], vec![[11, 10], [12, 11]], start, start, false);
        let at = |ms: u64| start + std::time::Duration::from_millis(ms);
        player.update(at(100));
        assert_eq!(actor::drawn_cell(player.position(at(100))), [11, 10]);
        player.update(at(300));
        assert_eq!(actor::drawn_cell(player.position(at(300))), [12, 11]);
        // A click on the cell it's drawn on sends nothing: no path to itself.
        let gat = Gat { width: 30, height: 30, cells: vec![([0.0; 4], 0); 900] };
        let from = [11, 10];
        assert_eq!(world::walk_goal(from, from, |cell| gat.walkable(cell), |cell| world::find_path(&gat, from, cell).is_some()), None);
    }
}
