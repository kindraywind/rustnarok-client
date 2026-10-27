//! The game's tick: what moves on between frames besides the drawing. It runs right before each frame is drawn.

use super::*;

impl App {
    /// The game's work at `now` that needs no GPU: sounds, companions, saved window places, units that leave, hits that
    /// land, and the player's walk. Drawing reads what this leaves.
    pub(crate) fn tick(&mut self, now: Instant) {
        let Scene::Game(game) = &mut self.scene else { return };
        let (grf, messages, audio) = (&self.grf, &self.messages, &mut self.audio);
        // A player's name block is asked for each time the pointer comes onto them; the last frame found what it's on.
        if game.pointed != game.last_pointed {
            game.last_pointed = game.pointed;
            let actor = |id| if id == game.account_id { Some(&game.player) } else { game.units.get(&id) };
            if let Some(id) = game.pointed.filter(|&id| actor(id).is_some_and(|actor| actor.kind == PLAYER)) {
                let _ = game.session.ask_name(id);
            }
        }
        game.poll_booking();
        // The makers of forged and brewed items anywhere on screen: each one's name asked for once, as the game does.
        let shown = (game.inventory.items.iter().chain(&game.cart.items).chain(&game.storage.items))
            .chain(game.trade.iter().flat_map(|trade| trade.items.iter().flatten()))
            .chain(game.my_shop.iter().flat_map(|shop| shop.goods.iter().map(|good| &good.item)))
            .chain(game.shop.iter().flat_map(|shop| shop.offers.iter().filter_map(|offer| offer.whole.as_ref())))
            .chain(game.mail.letter.iter().flat_map(|letter| &letter.items));
        let makers: Vec<u32> = shown.filter_map(|item| inventory::maker(&item.cards)).filter(|maker| !game.char_names.contains_key(maker)).collect();
        for maker in makers {
            if game.names_asked.insert(maker) {
                let _ = game.session.ask_char_name(maker);
            }
        }
        // The refine window's hammering: its sounds as the frames pass, and at its end the result's sound and chat line.
        let zeny = game.status.get(&net::STATUS_ZENY).copied().unwrap_or(0).max(0) as u32;
        let carried = &game.inventory.items;
        let carried_of = |id: u32| carried.iter().filter(|item| item.id == id).map(|item| u32::from(item.amount)).sum::<u32>();
        let cues = game.refine.as_mut().map(|window| window.advance(control::now(), carried_of, zeny)).unwrap_or_default();
        for cue in cues {
            match cue {
                refine::Cue::Sound(file) => {
                    if let Some(audio) = audio.as_mut() {
                        audio.play(grf, file, 1.0);
                    }
                }
                refine::Cue::Chat(id) => game.chat_line(messages.get(id).cloned().unwrap_or_default(), CHAT_ITEM),
            }
        }
        // The map's ambient sounds, each on its cycle.
        if let Some(audio) = audio.as_mut() {
            let at = actor::world_xz(game.player.position(now));
            for (file, volume) in game.world.ambient.due(at, now) {
                audio.play(grf, &file, volume);
            }
        }
        // The mercenary and the homunculus follow the player unless told otherwise.
        if let Some(id) = game.mercenary.as_ref().map(|hired| hired.id) {
            game.follow_player(id, false, now);
        }
        if let Some(id) = game.homunculus_id {
            game.follow_player(id, true, now);
        }
        // Our pet left standing may say so.
        if let Some(actor) = game.pet_id.and_then(|id| game.units.get(&id))
            && game.pet_idle.due(actor.moving(), now)
            && crate::effects::pick(0, 3) == 0
            && let Some(hunger) = game.pet.as_ref().map(|pet| pet.hunger)
        {
            game.pet_talk(pet::ACT_STAND, hunger);
        }

        // Where the windows were left is kept for next time.
        game.save_places();
        game.save_small_windows();

        // Bodies that finished dying leave; hits that land make their target flinch.
        game.units.retain(|_, actor| actor.remove_at.is_none_or(|at| at > now));
        // A hit's sound when it lands, at the target.
        let mut hit_sounds = Vec::new();
        for damage in &mut game.damages {
            if damage.start <= now
                && let Some(source) = damage.source.take()
            {
                let target = damage.target.map_or(Some(&game.player), |id| game.units.get(&id));
                let attacker = if source == game.account_id { Some(&game.player) } else { game.units.get(&source) };
                if let Some(target) = target {
                    let lineage = (target.kind == PLAYER).then(|| self.skill_trees.lineage(target.job));
                    let weapon = attacker.and_then(|actor| actor.look.as_ref()).map(|look| self.look_tables.weapon_type(look.weapon));
                    let roll = damage.start.duration_since(game.entered).subsec_nanos() >> 8;
                    let at = target.position(now);
                    let me = game.player.position(now);
                    hit_sounds.push((hit_sound(lineage.as_deref(), weapon, roll), heard(f32::hypot(at[0] - me[0], at[1] - me[1]))));
                }
            }
            if damage.start <= now && damage.flinch.is_some() {
                let motion = damage.flinch.take();
                let target = match damage.target {
                    None => Some(&mut game.player),
                    Some(id) => game.units.get_mut(&id),
                };
                // The flinch stops a walk once its step is done and gets a sitting player up.
                if let Some(target) =
                    target.filter(|target| target.remove_at.is_none() && !target.dead && actor::flinches(target.kind, target.animation(now).0))
                    && let Some(motion) = motion
                {
                    let (_, flinch, _) = actor::combat_actions(target.kind);
                    target.flinch(flinch, now, motion);
                    target.stop_after_step();
                    target.sitting = false;
                    target.stand_ready();
                }
            }
        }
        game.damages.retain(|damage| now < damage.start || now.duration_since(damage.start).as_secs_f32() < damage.kind.duration(damage.amount));
        if let Some(audio) = audio {
            for (sound, volume) in hit_sounds {
                audio.play(grf, &sound, volume);
            }
        }

        // The player walks, and what goes with it follows.
        let walking = game.player.moving();
        game.player.casting = game.casts.contains_key(&None);
        // The test driver's clock can run back on a resume after a step: walks never see a frame earlier than the last,
        // so only a change of the server's clock moves them back.
        let walk_now = now.max(game.walk_clock);
        game.walk_clock = walk_now;
        let player_cell = game.player.update(walk_now);
        game.player.step_companions(actor::world_xz(player_cell), now);
        if walking
            && !game.player.moving()
            && let Some((_, arrived)) = &mut game.pending
        {
            *arrived = Some(now);
        }
    }
}
