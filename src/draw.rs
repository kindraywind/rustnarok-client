//! Drawing a frame: the loading screen, the scenes before the map, and the game's world and windows.

use super::*;

/// A unit sprite's vertex RGB is its tint times the map light. Alpha stays the fade.
fn lit_tint(tint: [f32; 4], light: [f32; 3]) -> [f32; 4] {
    [tint[0] * light[0], tint[1] * light[1], tint[2] * light[2], tint[3]]
}

impl App {
    /// The loading screen: the picture over everything and an empty progress bar.
    pub(crate) fn draw_loading(&mut self, event_loop: &ActiveEventLoop) {
        let (Some(gpu), Some(assets), Some(loading)) = (&mut self.gpu, &self.assets, &mut self.loading) else { return };
        let mut layer = Layer::default();
        if let Some(image) = &loading.image {
            let (at, size) = picture_place();
            layer.sprites.push((image, at, size));
        }
        let [width, height] = LOADING_BAR;
        let corner = [((screen()[0] - width) / 2.0).floor(), (screen()[1] * 0.75).floor()];
        layer.sprites.push((&assets.loading_bar[0], corner, LOADING_BAR));
        layer.sprites.push((&assets.loading_bar[1], offset(corner, [1.0, 1.0]), [width - 2.0, height - 2.0]));
        let percent = "0%";
        layer.texts.push((percent.to_owned(), [((screen()[0] - self.font.width(percent)) / 2.0).floor(), corner[1] + 1.0], [255, 255, 0]));
        let composed = compose(gpu, &self.font, vec![layer]);
        let (sprites, alphas) = flatten(&composed);
        // Screenshots wait for the game itself, unless RUSTNAROK_CLIENT_SHOT_LOADING asks for this screen.
        match &self.screenshot {
            Some(path) if std::env::var_os("RUSTNAROK_CLIENT_SHOT_LOADING").is_some() => {
                gpu.capture(None, &sprites, &alphas, path);
                event_loop.exit();
            }
            _ => gpu.draw(None, &sprites, &alphas),
        }
        loading.shown = true;
    }
}

impl App {
    pub(crate) fn redraw(&mut self, event_loop: &ActiveEventLoop) {
        // The map packets since the last frame first, before anything moves.
        for event in std::mem::take(&mut self.frame_events) {
            let event = NetEvent::Map(event);
            control::observe(&event);
            self.net_event(event);
        }
        if self.loading.is_some() {
            return self.draw_loading(event_loop);
        }
        if matches!(self.scene, Scene::Game(_)) {
            return self.redraw_game(event_loop);
        }
        let blink_phase = self.blink_phase();
        let (Some(gpu), Some(assets)) = (&mut self.gpu, &self.assets) else { return };
        let font = &self.font;
        // Covered scenes don't react to the pointer.
        let interactive =
            matches!(self.scene, Scene::Connections { .. } | Scene::Login | Scene::ServerSelect { .. } | Scene::CharSelect(_) | Scene::CharMake(..));
        let base_pointer = if interactive { (self.cursor, self.mouse_down) } else { ([-1.0; 2], false) };

        // Character figures decode their textures first (mutably), then the frame borrows them.
        let poses = match self.scene.base_mut() {
            Scene::CharSelect(select) => {
                // Each change of the selection is kept, for the next time character select opens.
                if let Some(slot) = select.changed_selection()
                    && let Some(path) = settings_path("char_slot")
                {
                    let _ = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(path, slot.to_string()));
                }
                for path in select.job_icons() {
                    self.icons.entry(path.clone()).or_insert_with(|| gpu.try_load_texture(&self.grf, &path, false).ok());
                }
                select.pose(gpu)
            }
            _ => Vec::new(),
        };
        let make_poses = match self.scene.base_mut() {
            Scene::CharMake(make, _) => make.pose(gpu),
            _ => [None, None, None],
        };

        let mut base = Layer::default();
        let (at, size) = picture_place();
        base.sprites.push((&assets.background, at, size));
        let mut caret = None;
        // The login window and the server list close while the wait box is up.
        let boxed = matches!(&self.scene, Scene::Waiting(wait, _) if wait.message().is_some());
        let shown = match self.scene.base() {
            Scene::Login | Scene::ServerSelect { .. } if boxed => &Scene::Quit,
            scene => scene,
        };
        match shown {
            Scene::Login => {
                let login = login_position();
                let message = |id: usize| self.messages.get(id).map_or("", String::as_str);
                base.sprites.push((&assets.login, login, assets.login.size));
                base.sprites.push(button_sprite(base_pointer, offset(login, LOGIN_START), &assets.connect));
                base.sprites.push(button_sprite(base_pointer, offset(login, LOGIN_EXIT.0), &assets.exit));
                let save = &assets.save[usize::from(self.save_id)];
                base.sprites.push((save, offset(login, LOGIN_SAVE.0), save.size));
                let label_top = |(corner, size): ([f32; 2], [f32; 2])| corner[1] + ((size[1] - font.line_height()) / 2.0).floor();
                base.texts.push((message(SAVE_ID_LABEL).to_owned(), [login[0] + LOGIN_SAVE_LABEL.0[0], login[1] + LOGIN_SAVE_LABEL.0[1]], SAVE_ID_COLOR));
                // Opening and Sign Up, their labels centred.
                for ((corner, size), images, label) in [(LOGIN_OPENING, &assets.opening, OPENING_LABEL), (LOGIN_SIGN_UP, &assets.sign_up, SIGN_UP_LABEL)] {
                    base.sprites.push(button_sprite(base_pointer, offset(login, corner), images));
                    let text = message(label);
                    let left = corner[0] + ((size[0] - font.width(text)) / 2.0).floor();
                    base.texts.push((text.to_owned(), offset(login, [left, label_top((corner, size))]), LOGIN_BUTTON_COLOR));
                }
                // The account drop-down shows its pick; open, its two accounts under it.
                let (corner, size) = LOGIN_DROP;
                base.texts.push((message(LOGIN_ACCOUNTS[self.account_kind]).to_owned(), offset(login, [corner[0] + 5.0, label_top(LOGIN_DROP)]), TEXT_COLOR));
                if self.account_list {
                    for (row, &id) in LOGIN_ACCOUNTS.iter().enumerate() {
                        let at = offset(login, account_row(row));
                        base.sprites.push((&assets.chrome.border, at, size));
                        base.sprites.push((&assets.chrome.white, offset(at, [1.0, 1.0]), [size[0] - 2.0, size[1] - 2.0]));
                        base.texts.push((message(id).to_owned(), offset(at, [5.0, ((size[1] - font.line_height()) / 2.0).floor()]), TEXT_COLOR));
                    }
                }
                for (index, value) in self.fields.iter().enumerate() {
                    let shown = if index == 1 { "*".repeat(value.len()) } else { value.clone() };
                    // Long input scrolls: keep the tail that fits the box.
                    let mut start = 0;
                    while font.width(&shown[start..]) > LOGIN_FIELD_SIZE[0] - 2.0 * FIELD_PADDING {
                        start += 1;
                    }
                    // Centered in the field.
                    let field = offset(login, LOGIN_FIELDS[index]);
                    let left = ((LOGIN_FIELD_SIZE[0] - font.width(&shown[start..])) / 2.0).floor();
                    let position = [field[0] + left, field[1] + ((LOGIN_FIELD_SIZE[1] - font.line_height()) / 2.0).floor()];
                    if index == self.focus && matches!(self.scene, Scene::Login) && blink_phase.is_multiple_of(2) {
                        caret = Some([position[0] + font.width(&shown[start..]).round(), position[1] + 1.0]);
                    }
                    base.texts.push((shown[start..].to_owned(), position, TEXT_COLOR));
                }
            }
            Scene::Connections { selected } => {
                let service = service_position(&assets.service);
                let list = offset(service, SERVER_LIST);
                base.sprites.push((&assets.service, service, assets.service.size));
                base.sprites.push((&assets.selection, offset(list, [0.0, *selected as f32 * SERVER_ROW[1]]), SERVER_ROW));
                base.sprites.push(button_sprite(base_pointer, offset(service, LEFT_BUTTON), &assets.ok));
                base.sprites.push(button_sprite(base_pointer, offset(service, RIGHT_BUTTON), &assets.cancel));
                for (row, connection) in self.connections.iter().enumerate() {
                    base.texts.push((connection.display.clone(), offset(list, [4.0, row as f32 * SERVER_ROW[1] + 3.0]), TEXT_COLOR));
                }
            }
            Scene::ServerSelect { accepted, selected, scroll } => {
                let place = servers::place(screen());
                let body = servers::HEIGHT - window::TITLE_HEIGHT;
                // Grey edge, white body, then the bottom bar over the foot of it.
                base.sprites.push((&assets.chrome.border, offset(place, [0.0, window::TITLE_HEIGHT]), [servers::WIDTH, body]));
                base.sprites.push((&assets.chrome.white, offset(place, [1.0, window::TITLE_HEIGHT]), [servers::WIDTH - 2.0, body - 1.0]));
                let footer = servers::HEIGHT - servers::FOOTER;
                base.sprites.push((&assets.server_bar, offset(place, [0.0, footer]), [servers::WIDTH, servers::FOOTER]));
                let list = offset(place, servers::LIST);
                base.sprites.push((&assets.server_fill, list, servers::LIST_SIZE));
                if servers::shown(*scroll, accepted.char_servers.len()).contains(selected) {
                    let band = (*selected - *scroll) as f32 * servers::ROW;
                    base.sprites.push((&assets.white, offset(list, [0.0, band]), servers::BAND));
                }
                let mut frame = window::Frame::new(place, servers::WIDTH);
                frame.open = true;
                let title = self.messages.get(servers::TITLE).map(String::as_str).unwrap_or("");
                frame.draw_title(&mut base, &assets.chrome, title, base_pointer.0);
                base.sprites.push(button_sprite(base_pointer, offset(place, servers::OK), &assets.ok));
                base.sprites.push(button_sprite(base_pointer, offset(place, servers::CANCEL), &assets.cancel));
                let words = servers::Words::from_messages(&self.messages);
                let text_y = ((servers::ROW - font.line_height()).max(0.0) / 2.0).floor();
                for row in servers::shown(*scroll, accepted.char_servers.len()) {
                    let server = &accepted.char_servers[row];
                    let text = servers::row(&server.name, server.users, server.kind, server.new, &words);
                    let at = offset(list, [servers::TEXT_X, (row - *scroll) as f32 * servers::ROW + text_y]);
                    if text.crowd.is_empty() {
                        base.texts.push((text.rest, at, TEXT_COLOR));
                    } else {
                        base.texts.push((text.crowd.clone(), at, text.color));
                        if !text.rest.is_empty() {
                            base.texts.push((text.rest, [at[0] + font.width(&text.crowd), at[1]], TEXT_COLOR));
                        }
                    }
                }
            }
            Scene::CharSelect(select) => {
                let select_pointer = if select.pin_pad.is_some() { ([-1.0; 2], false) } else { base_pointer };
                select.draw(&mut base, &assets.char_select, font, &self.messages, &self.pc_job_names, &self.map_names, &self.icons, select_pointer, &poses);
                if let Some(pad) = &select.pin_pad {
                    pad.draw(&mut base, &assets.chrome, &assets.pin, font, &self.messages, base_pointer);
                }
            }
            Scene::CharMake(make, _) => {
                let caret = matches!(self.scene, Scene::CharMake(..)) && blink_phase.is_multiple_of(2);
                make.draw(&mut base, &assets.char_make, font, &self.messages, base_pointer, &make_poses, caret)
            }
            Scene::Quit => {}
            Scene::Game(_) | Scene::Waiting(..) | Scene::Message(..) | Scene::Confirm(..) | Scene::Prompt { .. } => unreachable!("handled elsewhere"),
        }

        let mut overlay = Layer::default();
        let pointer = (self.cursor, self.mouse_down);
        let dialog = dialog_position(&assets.message);
        let (text, buttons) = match &self.scene {
            Scene::Message(text, _) => (Some(text), vec![(RIGHT_BUTTON, &assets.ok)]),
            Scene::Confirm(text, ..) | Scene::Prompt { text, .. } => (Some(text), vec![(LEFT_BUTTON, &assets.ok), (RIGHT_BUTTON, &assets.cancel)]),
            _ => (None, Vec::new()),
        };
        if let Some(text) = text {
            overlay.sprites.push((&assets.message, dialog, assets.message.size));
            for (place, images) in buttons {
                overlay.sprites.push(button_sprite(pointer, offset(dialog, place), images));
            }
            let lines = message_lines(font, text, MESSAGE_WIDTH);
            let top = if matches!(self.scene, Scene::Prompt { .. }) { MESSAGE_TEXT[1] - 16.0 } else { MESSAGE_TEXT[1] };
            for (row, line) in lines.into_iter().enumerate() {
                overlay.texts.push((line.trim_start().to_owned(), offset(dialog, [MESSAGE_TEXT[0], top + row as f32 * MESSAGE_LINE]), TEXT_COLOR));
            }
        }
        // The wait box: its line, and Cancel.
        if let Scene::Waiting(wait, _) = &self.scene
            && let Some(id) = wait.message()
        {
            let at = login_position();
            overlay.sprites.push((&assets.message, at, assets.message.size));
            overlay.sprites.push(button_sprite(pointer, offset(at, WAIT_CANCEL), &assets.cancel));
            let text = self.messages.get(id).map_or("", String::as_str);
            for (row, line) in message_lines(font, text, WAIT_TEXT_WIDTH).into_iter().enumerate() {
                overlay.texts.push((line.trim_start().to_owned(), offset(at, [WAIT_TEXT[0], WAIT_TEXT[1] + row as f32 * MESSAGE_LINE]), TEXT_COLOR));
            }
        }
        if let Scene::Prompt { input, .. } = &self.scene {
            let field = offset(dialog, PROMPT_FIELD);
            overlay.sprites.push((&assets.grey, offset(field, [-1.0, -1.0]), [PROMPT_FIELD_SIZE[0] + 2.0, PROMPT_FIELD_SIZE[1] + 2.0]));
            overlay.sprites.push((&assets.white, field, PROMPT_FIELD_SIZE));
            let at = [field[0] + FIELD_PADDING, field[1] + ((PROMPT_FIELD_SIZE[1] - font.line_height()) / 2.0).floor()];
            overlay.texts.push((input.clone(), at, TEXT_COLOR));
            if blink_phase.is_multiple_of(2) {
                caret = Some([at[0] + font.width(input).round(), at[1] + 1.0]);
            }
        }

        // A character card's menu, over the cards.
        let mut card_menu = Layer::default();
        if let Scene::CharSelect(select) = &self.scene {
            select.draw_menu(&mut card_menu, &assets.context_menu, font, self.cursor);
        }
        let composed = compose(gpu, font, vec![base, card_menu, overlay]);
        let (mut sprites, alphas) = flatten(&composed);
        if let Some(position) = caret {
            sprites.push((&assets.black, position, [1.0, font.line_height() - 2.0]));
        }
        self.cursor_look.set(cursor::Kind::Default);
        sprites.extend(self.cursor_look.draw(gpu, self.cursor));

        // With RUSTNAROK_CLIENT_AUTOSTART=<slot> the shot waits for the game; any other value stops at character select.
        let autostart = std::env::var("RUSTNAROK_CLIENT_AUTOSTART").ok();
        let playing_slot = autostart.as_deref().is_some_and(|value| value.parse::<usize>().is_ok());
        let settled = !matches!(self.scene, Scene::Waiting(..))
            && match autostart {
                None => true,
                // Back from the game (the option menu's character select) counts too.
                Some(_) if playing_slot => matches!(self.scene, Scene::Message(..)) || (self.returned && matches!(self.scene, Scene::CharSelect(_))),
                Some(_) => matches!(self.scene, Scene::CharSelect(_) | Scene::CharMake(..) | Scene::Message(..) | Scene::Confirm(..) | Scene::Prompt { .. }),
            };
        match &self.screenshot {
            Some(path) if settled => {
                gpu.capture(None, &sprites, &alphas, path);
                event_loop.exit();
            }
            _ => gpu.draw(None, &sprites, &alphas),
        }
        self.drawn_blink_phase = blink_phase;
    }

    pub(crate) fn redraw_game(&mut self, event_loop: &ActiveEventLoop) {
        let (Some(gpu), Some(assets), Scene::Game(game)) = (&mut self.gpu, &self.assets, &mut self.scene) else { return };
        // ponytail: the client's own line for a saved screenshot isn't in the data; a plain one.
        if let Some(name) = self.shot_saved.take() {
            game.chat_line(format!("Screenshot saved: {name}"), CHAT_ITEM);
        }
        let (grf, items, messages, icons, cursor_look, pc_job_names) =
            (&self.grf, &self.items, &self.messages, &mut self.icons, &mut self.cursor_look, &self.pc_job_names);
        let (audio, sound) = (&mut self.audio, &self.sound);
        let (worlds, map_names, quest_texts) = (&self.worlds, &self.map_names, &self.quest_texts);
        let card_names = &self.card_names;
        let (achievement_info, title_names) = (&self.achievement_info, &self.title_names);
        let (skill_info, skill_trees, status_icon_files) = (&self.skill_info, &self.skill_trees, &self.status_icon_files);
        let state_infos = &self.state_infos;
        let monster_sizes = &self.monster_sizes;
        let shadow_factors = &self.shadow_factors;
        let font = &self.font;
        let now = control::now();
        let pointer = (self.cursor, self.mouse_down);
        let pet_window = &mut game.pet_window;
        pet_window.auto_label = font.width(messages.get(pet::AUTO_FEED_LABEL).map_or("", String::as_str));
        // ponytail: the box only hides; rathena itself refuses feeding by itself to a pet it doesn't allow.
        pet_window.auto_allowed = pet_window.egg.is_some_and(|egg| self.pets.auto_eggs.contains(&egg));
        let players_first = game.players_first(self.modifiers.shift_key());
        let message = |id: usize| messages.get(id).map_or("", String::as_str);

        // The map's moving RSM 2 models, posed for now.
        game.world.animate(gpu, now);
        game.poll_booking();
        // The map's ambient sounds, each on its cycle.
        if let Some(audio) = audio.as_mut() {
            let at = actor::world_xz(game.player.position(now));
            for (file, volume) in game.world.ambient.due(at, now) {
                audio.play(grf, &file, volume);
            }
        }
        // The party's docked window stays right of the party window.
        if game.party_window.frame.open
            && let Some(dialog) = &mut game.party_dialog
        {
            dialog.frame.position = party::docked_at(Some((game.party_window.frame.position, game.party_window.size()[0])), screen());
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
        // Fireworks weather sets off fireworks around the player.
        for [dx, dy] in game.weather.fireworks_due(now) {
            let cell = [game.player.cell[0] + dx, game.player.cell[1] + dy].map(|value| value.max(0) as u16);
            game.play_effect_at(gpu, grf, &self.effect_table, weather::FIREWORK, cell);
        }

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

        // Move everyone, follow the player, then decode the sprite frames (mutably) before borrowing them.
        let walking = game.player.moving();
        game.player.casting = game.casts.contains_key(&None);
        // The test driver's clock can run back on a resume after a step: walks never see a frame earlier than the last,
        // so only a change of the server's clock moves them back.
        let walk_now = now.max(game.walk_clock);
        game.walk_clock = walk_now;
        let player_cell = game.player.update(walk_now);
        // ponytail: footprints alternate 326 and 327, one on each cell just left, only while we are chase-walking.
        // Others don't see the walker, so they don't see our prints. A new chase starts again at 326.
        if game.player.hide_option & actor::OPTION_CHASEWALK != 0 {
            let stepped = game.player.steps_left().to_vec();
            for cell in stepped {
                let effect = actor::chase_footprint(game.player.footprints);
                game.player.footprints = game.player.footprints.wrapping_add(1);
                let at = [cell[0].max(0) as u16, cell[1].max(0) as u16];
                game.play_effect_at(gpu, grf, &self.effect_table, effect, at);
            }
        } else {
            game.player.footprints = 0;
        }
        game.player.step_companions(actor::world_xz(player_cell), now);
        if walking
            && !game.player.moving()
            && let Some((_, arrived)) = &mut game.pending
        {
            *arrived = Some(now);
        }
        let player_feet = game.world.cell_position(player_cell);
        game.camera_goal.drag_frame();
        game.camera_goal.ease(&mut game.camera, now.duration_since(game.last_frame), player_feet, game.camera_snap);
        // An earthquake shakes the view sideways, in and out and in tilt, the other way each frame, and snaps back when it ends.
        let quake = game.effects.quake(now).filter(|_| self.graphics.quake);
        game.camera_goal.shake(&mut game.camera, player_feet, quake);
        // The eye stays above the ground under it.
        let horizontal = game.camera.distance * game.camera.pitch.cos();
        let eye = [game.camera.target[0] - horizontal * game.camera.yaw.sin(), game.camera.target[2] - horizontal * game.camera.yaw.cos()];
        game.camera.ground = -game.world.gat.height(eye.map(|axis| (axis / world::CELL).floor() as i32));
        game.last_frame = now;
        let view = game.camera.view(screen()[0] / screen()[1]);
        let yaw = game.camera.yaw;
        let mut frames = Vec::new();
        let mut sounds: Vec<(String, f32)> = game.player.sounds_now(now).into_iter().map(|sound| (sound, 1.0)).chain(hit_sounds).collect();
        let (action, timing) = game.player.animation(now);
        let head_direction = game.player.head_direction;
        let player_seen = seen_direction(game.player.rot, yaw, rounds_facing(PLAYER, game.player.job, action));
        // Pushcarts, apart from the bodies (their textures are the cart's, and they cast no shadow of their own).
        // A cart's ACT has one action per direction; its frames turn the wheels, so they run only while walking.
        // ponytail: the cart's anchor half a cell behind its owner; place it against the real client.
        // A cart stands on the ground where its rope has it, facing its owner, its wheels at the distance rolled. Its
        // ACT has one action per direction.
        let world = &game.world;
        let cart_frame = |cart: &mut actor::Cart| {
            let seen = usize::from(seen_direction(cart.rot, yaw, true));
            let frame = cart.frame(cart.sprites.frames(seen));
            let count = cart.sprites.frames(seen).max(1) as f32;
            let placed = cart.sprites.frame_at(gpu, seen, 0, Timing::Span((frame as f32 + 0.5) / count));
            (world.ground_under([cart.at[0], 0.0, cart.at[1]]), placed)
        };
        let mut carts = Vec::new();
        let mut companion_draws: Vec<CompanionDraw> = Vec::new();
        // ponytail: Clairvoyance is status 184 on our own character.
        let clairvoyance = game.status_icons.iter().any(|(kind, ..)| *kind == actor::CLAIRVOYANCE);
        if let Some(cart) = &mut game.player.cart
            && actor::seen(game.player.hide_option, true, clairvoyance) == actor::Seen::Whole
        {
            let (feet, placed) = cart_frame(cart);
            carts.push((Owner::Player, feet, placed));
        }
        let player_tint = game.player.body_tint();
        let player_scale = unit_draw_scale(monster_sizes, game.player.kind, game.player.job, game.player.size_percent);
        // Our own character in Hiding shows its shadow alone (a frame with no sprite); cloaked, chase-walking or
        // GM-hidden, nothing. We are never drawn as a silhouette.
        match actor::seen(game.player.hide_option, true, clairvoyance) {
            actor::Seen::Whole => {
                if let Some(sprites) = &mut game.player.sprites {
                    sprites.set_tint(player_tint);
                    let mut placed = sprites.frame(gpu, action, player_seen, head_direction, timing);
                    scale_frame(&mut placed, player_scale);
                    game.player.leave_ghost(now, player_feet, &placed);
                    frames.push((Owner::Player, player_feet, placed));
                }
            }
            actor::Seen::Shadow => frames.push((Owner::Player, player_feet, Vec::new())),
            actor::Seen::Nothing | actor::Seen::Silhouette => {}
        }
        if !game.player.cloaked {
            if let Some(falcon) = &mut game.player.falcon {
                push_companion(falcon, false, Owner::Player, gpu, &game.world, yaw, now, &mut companion_draws);
            }
            if let Some(warg) = &mut game.player.warg {
                push_companion(warg, true, Owner::Player, gpu, &game.world, yaw, now, &mut companion_draws);
            }
        }
        // The dress room's figure: rebuilt when a pick changes, standing the way it's turned.
        if let Some(room) = &mut game.dressroom
            && room.changed
        {
            game.dress_figure = SpriteSet::player(grf, &room.look(&character_look(&game.me), &game.dress_lists), &self.look_tables);
            room.changed = false;
        }
        let dress_placements = match (&game.dressroom, &mut game.dress_figure) {
            (Some(room), Some(figure)) => figure.flat_frame(gpu, actor::IDLE, room.direction, 0, Timing::Loop(0.0)),
            _ => Vec::new(),
        };
        // The equipment window shows the character standing, facing front.
        let preview_feet = game.equipment.preview_feet();
        let viewed_feet = game.viewing.as_ref().and_then(|viewed| viewed.window.preview_feet());
        let viewed_placements = match (viewed_feet, game.viewing.as_mut().and_then(|viewed| viewed.sprites.as_mut())) {
            (Some(_), Some(sprites)) => sprites.flat_frame(gpu, actor::IDLE, 0, 0, Timing::Loop(0.0)),
            _ => Vec::new(),
        };
        let preview_placements = match (preview_feet, &mut game.player.sprites) {
            (Some(_), Some(sprites)) => {
                // The equipment figure keeps the sprite's own colours.
                sprites.set_tint(None);
                sprites.flat_frame(gpu, actor::IDLE, 0, 0, Timing::Loop(0.0))
            }
            _ => Vec::new(),
        };
        let stylist_placements = game.stylist.as_mut().map(|window| window.pose(gpu)).unwrap_or_default();
        let mut labels = Vec::new();
        // Whose names show grey (ours) and against what level monsters' are judged.
        let own_level = game.status.get(&net::STATUS_BASE_LEVEL).copied().unwrap_or(1);
        let our_summons = [game.pet_id, game.homunculus_id, game.mercenary.as_ref().map(|mercenary| mercenary.id)];
        // Talkie Boxes saying their words stand where their traps were.
        let mut feet_of = game.talkie_spots.clone();
        for (&id, actor) in &mut game.units {
            actor.casting = game.casts.contains_key(&Some(id));
            let cell = actor.update(walk_now);
            actor.step_companions(actor::world_xz(cell), now);
            // Hidden units aren't drawn, except as black silhouettes while we have Clairvoyance. GM hide stays hidden.
            let show = actor::seen(actor.hide_option, false, clairvoyance);
            if show == actor::Seen::Nothing {
                continue;
            }
            let silhouette = show == actor::Seen::Silhouette;
            let feet = game.world.cell_position(cell);
            feet_of.insert(id, feet);
            if !silhouette {
                let loudness = heard(f32::hypot(cell[0] - player_cell[0], cell[1] - player_cell[1]));
                sounds.extend(actor.sounds_now(now).into_iter().map(|sound| (sound, loudness)));
            }
            let (action, timing) = actor.animation(now);
            let head_direction = actor.head_direction;
            let seen = seen_direction(actor.rot, yaw, rounds_facing(actor.kind, actor.job, action));
            // NPC names show under them; players' and monsters' on hover (below).
            if !silhouette && !actor.hidden && (actor.kind == NPC || actor.sprites.is_none()) {
                labels.push((actor.name.clone(), feet, name_color(actor.kind, actor.job, actor.level, our_summons.contains(&Some(id)), own_level)));
            }
            if !silhouette && let Some(cart) = &mut actor.cart {
                let (feet, placed) = cart_frame(cart);
                carts.push((Owner::Unit(id), feet, placed));
            }
            // A silhouette multiplies the sprite's RGB by 0 and keeps its alpha. It casts no shadow.
            let tint = if silhouette { Some([0, 0, 0]) } else { actor.body_tint() };
            let scale = unit_draw_scale(monster_sizes, actor.kind, actor.job, actor.size_percent);
            if let Some(sprites) = &mut actor.sprites {
                sprites.set_tint(tint);
                let mut placed = sprites.frame(gpu, action, seen, head_direction, timing);
                scale_frame(&mut placed, scale);
                actor.leave_ghost(now, feet, &placed);
                frames.push((Owner::Unit(id), feet, placed));
            }
            if !silhouette && !actor.cloaked {
                if let Some(falcon) = &mut actor.falcon {
                    push_companion(falcon, false, Owner::Unit(id), gpu, &game.world, yaw, now, &mut companion_draws);
                }
                if let Some(warg) = &mut actor.warg {
                    push_companion(warg, true, Owner::Unit(id), gpu, &game.world, yaw, now, &mut companion_draws);
                }
            }
        }
        if let Some(audio) = audio {
            for (sound, volume) in sounds {
                audio.play(grf, &sound, volume);
            }
        }
        // Items on the ground; just-dropped ones fall into place.
        for (&id, item) in &game.ground {
            let Some(Some(sprites)) = game.item_sprites.get_mut(ground_sprite(items, item.item, item.identified)) else { continue };
            let mut feet = game.world.ground_under(game.world.cell_position(item.cell));
            if let Some(dropped) = item.dropped {
                feet[1] += item_fall_height(now.saturating_duration_since(dropped).as_secs_f32() / 0.024);
            }
            frames.push((Owner::Item(id), feet, sprites.frame(gpu, 0, 0, 0, Timing::Loop(0.0))));
        }

        // Clickable areas: each unit's and item's sprite bounds on screen, in logical pixels, and how far along the
        // camera's view it stands. The dead and the hidden can't be clicked.
        let to_screen = |[x, y]: [f32; 2]| [x * screen()[0], y * screen()[1]];
        let siege_map = game.map_property.is_some_and(|(_, flags)| versus::siege(flags));
        let our_guild = game.guild.as_ref().map_or(0, |guild| guild.id);
        game.hit_boxes = frames
            .iter()
            .filter(|(owner, ..)| !matches!(owner, Owner::Unit(id) if game.units.get(id).is_some_and(|actor| actor.remove_at.is_some() || actor.hidden)))
            .filter(|(_, _, placements)| !placements.is_empty())
            .filter_map(|(owner, feet, placements)| {
                let (mut low, mut high) = ([f32::MAX; 2], [f32::MIN; 2]);
                for (_, position, size, _) in placements {
                    for axis in 0..2 {
                        let (a, b) = (position[axis], position[axis] + size[axis]);
                        low[axis] = low[axis].min(a.min(b));
                        high[axis] = high[axis].max(a.max(b));
                    }
                }
                let [left, top, right, bottom] = view.sprite_rect(*feet, low, high)?;
                let sprite = [left * screen()[0], top * screen()[1], right * screen()[0], bottom * screen()[1]];
                let depth = (0..3).map(|axis| (feet[axis] - view.eye[axis]) * view.forward[axis]).sum::<f32>();
                let kind = match owner {
                    Owner::Player => picking::Kind::Player,
                    Owner::Item(_) => picking::Kind::Item,
                    Owner::SkillUnit(_) => picking::Kind::SkillUnit,
                    Owner::Unit(id) => match game.units.get(id).map(|actor| actor.kind) {
                        Some(MONSTER) => picking::Kind::Monster,
                        Some(NPC) => picking::Kind::Npc,
                        Some(HOMUNCULUS) => picking::Kind::Homunculus,
                        // On a siege map players outside our guild rank with monsters.
                        Some(PLAYER) if siege_map && game.units.get(id).is_some_and(|actor| our_guild == 0 || actor.guild != our_guild) => picking::Kind::Foe,
                        Some(PLAYER) => picking::Kind::Player,
                        _ => picking::Kind::Other,
                    },
                };
                let min = if kind == picking::Kind::Item { picking::ITEM_MIN } else { picking::UNIT_MIN };
                Some(picking::Target { owner: *owner, kind, sprite, area: picking::area(sprite, min), depth })
            })
            .collect();

        // Ice Wall and the traps a hand can break: their cell, grown to an item's least box.
        // ponytail: drawn as effects, they have no sprite to pick by; the cell is the box, as for warps.
        for (&id, &(_, cell)) in game.skill_units.iter().filter(|(_, (kind, _))| attackable_ground_unit(*kind)) {
            let feet = game.world.cell_position(cell.map(f32::from));
            if let Some([left, top, right, bottom]) = warp_pick_rect(&view, feet) {
                let sprite = picking::bounds(&[to_screen([left, top]), to_screen([right, bottom])]);
                let depth = (0..3).map(|axis| (feet[axis] - view.eye[axis]) * view.forward[axis]).sum::<f32>();
                game.hit_boxes.push(picking::Target {
                    owner: Owner::SkillUnit(id),
                    kind: picking::Kind::SkillUnit,
                    sprite,
                    area: picking::area(sprite, picking::ITEM_MIN),
                    depth,
                });
            }
        }

        // Warp portals have no sprite: their pick box is drawn around the feet. They rank as NPCs.
        for (&id, actor) in game.units.iter().filter(|(_, actor)| actor.job == WARP_JOB) {
            let feet = game.world.cell_position(actor.cell.map(|value| value as f32));
            if let Some([left, top, right, bottom]) = warp_pick_rect(&view, feet) {
                let cell = picking::bounds(&[to_screen([left, top]), to_screen([right, bottom])]);
                let depth = (0..3).map(|axis| (feet[axis] - view.eye[axis]) * view.forward[axis]).sum::<f32>();
                game.hit_boxes.push(picking::Target {
                    owner: Owner::Unit(id),
                    kind: picking::Kind::Npc,
                    sprite: cell,
                    area: picking::area(cell, picking::UNIT_MIN),
                    depth,
                });
            }
        }

        // The message sprite's frames for critical bursts and Lucky, posed while the sprite can still decode them.
        let message_frames: Vec<Vec<actor::Placed>> = match &mut game.message_sprite {
            Some(sprite) => game
                .damages
                .iter()
                .map(|damage| {
                    let elapsed = now.saturating_duration_since(damage.start).as_secs_f32();
                    match damage.kind {
                        _ if damage.start > now => Vec::new(),
                        DamageKind::Critical => {
                            let frames = sprite.frames(actor::BURST_ACTION).max(1) as f32;
                            sprite.frame_at(gpu, actor::BURST_ACTION, 0, Timing::Span((elapsed / 0.024 / 2.0).floor() / frames))
                        }
                        DamageKind::Lucky => sprite.frame_at(gpu, actor::LUCKY_ACTION, 0, Timing::Once(elapsed * 1000.0, 1.0)),
                        _ => Vec::new(),
                    }
                })
                .collect(),
            None => Vec::new(),
        };

        // Shadows first, so characters standing at the same depth draw over them.
        let shadow_placements = game.shadow.as_mut().map(|shadow| shadow.frame(gpu, 0, 0, 0, Timing::Loop(0.0))).unwrap_or_default();
        let mut billboards: Vec<Billboard> = Vec::new();
        if let Some(shadow) = &game.shadow {
            // On the ground under each unit, even one in the air (a falling item).
            let cast = |actor: &actor::Actor, scale: f32| {
                let factor = actor::shadow_factor(shadow_factors, actor.kind == PLAYER, actor.job, actor.option, actor.sitting, actor.dead);
                factor * scale
            };
            for (owner, feet, _) in &frames {
                let dead = matches!(owner, Owner::Unit(id) if game.units.get(id).is_some_and(|actor| actor.shadowless(now)));
                let scale = match owner {
                    Owner::Item(_) => ITEM_SHADOW,
                    Owner::SkillUnit(_) => 0.0,
                    Owner::Player => cast(&game.player, player_scale),
                    Owner::Unit(id) => match game.units.get(id) {
                        Some(actor) if actor::seen(actor.hide_option, false, clairvoyance) == actor::Seen::Silhouette => 0.0,
                        Some(actor) => cast(actor, unit_draw_scale(monster_sizes, actor.kind, actor.job, actor.size_percent)),
                        None => 1.0,
                    },
                };
                if !dead && scale > 0.0 {
                    let ground = game.world.ground_under(*feet);
                    billboards.extend(shadow_placements.iter().map(|(key, position, size, angle)| {
                        (
                            shadow.texture(*key),
                            ground,
                            position.map(|value| value * scale),
                            size.map(|value| value * scale),
                            [1.0; 4],
                            *angle,
                            DepthLayer::default(),
                        )
                    }));
                }
            }
            // A cart's, a body's.
            for (_, feet, _) in &carts {
                billboards.extend(
                    shadow_placements
                        .iter()
                        .map(|(key, position, size, angle)| (shadow.texture(*key), *feet, *position, *size, [1.0; 4], *angle, DepthLayer::default())),
                );
            }
            // The warg's shadow is twice a body's (a baby's a body's). The falcon, up in the air, casts none.
            for draw in companion_draws.iter().filter(|draw| draw.shadow > 0.0) {
                let scale = draw.shadow;
                billboards.extend(shadow_placements.iter().map(|(key, position, size, angle)| {
                    (
                        shadow.texture(*key),
                        draw.feet,
                        position.map(|value| value * scale),
                        size.map(|value| value * scale),
                        [1.0; 4],
                        *angle,
                        DepthLayer::default(),
                    )
                }));
            }
        }
        let shadows = billboards.len();
        let sprites_of = |owner: &Owner| match owner {
            Owner::Player => game.player.sprites.as_ref(),
            Owner::Unit(id) => game.units[id].sprites.as_ref(),
            Owner::SkillUnit(_) => None,
            Owner::Item(id) => {
                let item = &game.ground[id];
                game.item_sprites.get(ground_sprite(items, item.item, item.identified)).and_then(Option::as_ref)
            }
        };
        // Spirit spheres, or coins for the Gunslinger line, circling the body at chest height: the far half of the
        // circle behind the body, the near half in front.
        let turned = now.duration_since(game.entered).as_secs_f32();
        let coin = game.coin_sprite.as_mut().map(|sprite| sprite.frame_at(gpu, 0, 0, Timing::Loop(turned * 1000.0))).unwrap_or_default();
        let mut spirits: [Vec<Billboard>; 2] = [Vec::new(), Vec::new()];
        let orbit_layer = |near: bool| DepthLayer { nudge: if near { IN_FRONT } else { -1.0 }, ..DepthLayer::default() };
        for (unit, orbs) in &game.spirits {
            let (feet, job) = match unit {
                None => (player_feet, game.me.job),
                Some(id) => match (feet_of.get(id), game.units.get(id)) {
                    (Some(feet), Some(actor)) => (*feet, actor.job),
                    _ => continue,
                },
            };
            // Shields at the waist, souls over the spheres, charms over the head.
            let (element, charms) = orbs.charms;
            let pictures = [
                (game.shield.as_ref(), orbs.shields, SHIELD_HEIGHT, SHIELD_SCALE, [1.0; 4]),
                (game.sphere.as_ref(), orbs.souls, SOUL_HEIGHT, SPHERE_SCALE, SOUL_TINT),
                (game.charm.as_ref(), charms, CHARM_HEIGHT, CHARM_SCALE, CHARM_TINTS[usize::from(element.clamp(1, 4) - 1)]),
            ];
            for (picture, count, height, scale, tint) in pictures {
                let Some(picture) = picture else { continue };
                let size = picture.size.map(|side| side * scale);
                for (center, near) in orbit(count, height, turned) {
                    spirits[usize::from(near)].push((
                        picture,
                        feet,
                        [center[0] - size[0] / 2.0, center[1] - size[1] / 2.0],
                        size,
                        tint,
                        0.0,
                        orbit_layer(near),
                    ));
                }
            }
            for (center, near) in orbit(orbs.spheres, SPIRIT_HEIGHT, turned) {
                let side = &mut spirits[usize::from(near)];
                if coin_job(job) {
                    if let Some(sprite) = &game.coin_sprite {
                        side.extend(coin.iter().map(|(key, position, size, angle)| {
                            (sprite.texture(*key), feet, [center[0] + position[0], center[1] + position[1]], *size, [1.0; 4], *angle, orbit_layer(near))
                        }));
                    }
                } else if let Some(sphere) = &game.sphere {
                    let size = sphere.size.map(|side| side * SPHERE_SCALE);
                    side.push((sphere, feet, [center[0] - size[0] / 2.0, center[1] - size[1] / 2.0], size, [1.0; 4], 0.0, orbit_layer(near)));
                }
            }
        }
        let [behind, in_front] = spirits;
        billboards.extend(behind);
        // Carts before the bodies, so the one pulling a cart draws over it.
        let cart_of = |owner: &Owner| match owner {
            Owner::Player => game.player.cart.as_ref().map(|cart| &cart.sprites),
            Owner::Unit(id) => game.units.get(id).and_then(|actor| actor.cart.as_ref()).map(|cart| &cart.sprites),
            Owner::Item(_) | Owner::SkillUnit(_) => None,
        };
        billboards.extend(carts.iter().flat_map(|(owner, feet, placements)| {
            let cart = cart_of(owner).unwrap();
            placements.iter().enumerate().map(move |(index, (key, position, size, angle))| {
                (cart.texture(*key), *feet, *position, *size, [1.0; 4], *angle, sprite_layer(index, false, false))
            })
        }));
        // The dead lie flat.
        let stand_of = |owner: &Owner| match owner {
            Owner::Player => (true, game.player.dead),
            Owner::Unit(id) => game.units.get(id).map_or((false, false), |actor| (actor.kind == PLAYER, actor.dead || actor.remove_at.is_some())),
            Owner::Item(_) | Owner::SkillUnit(_) => (false, false),
        };
        billboards.extend(frames.iter().flat_map(|(owner, feet, placements)| {
            let sprites = sprites_of(owner).unwrap();
            let tint = match owner {
                Owner::Unit(id) => [1.0, 1.0, 1.0, game.units.get(id).map_or(1.0, |actor| actor.alpha(now))],
                Owner::Item(id) if game.ground.get(id).is_some_and(|item| item_glints(now.saturating_duration_since(item.seen).as_secs_f32() / 0.024)) => {
                    [1.0, 1.0, 0.0, 1.0]
                }
                _ => [1.0; 4],
            };
            // Layer colour and status tints are already in the sprite image. Shadows, items and skill units stay unlit.
            let tint = match owner {
                Owner::Player | Owner::Unit(_) => lit_tint(tint, game.world.sprite_light(*feet)),
                _ => tint,
            };
            let (player, flat) = stand_of(owner);
            placements.iter().enumerate().map(move |(index, (key, position, size, angle))| {
                (sprites.texture(*key), *feet, *position, *size, tint, *angle, sprite_layer(index, player, flat))
            })
        }));
        // The copies left behind units that blur (the quickens), fading out.
        let blurring = std::iter::once((Owner::Player, &game.player)).chain(game.units.iter().map(|(&id, actor)| (Owner::Unit(id), actor)));
        for (owner, actor) in blurring.filter(|(_, actor)| !actor.ghosts.is_empty()) {
            let Some(sprites) = actor.sprites.as_ref() else { continue };
            let (player, flat) = stand_of(&owner);
            for ghost in &actor.ghosts {
                let alpha = actor::ghost_alpha(now.saturating_duration_since(ghost.left));
                let tint = lit_tint([1.0, 1.0, 1.0, alpha], game.world.sprite_light(ghost.feet));
                billboards.extend(ghost.placed.iter().enumerate().filter_map(|(index, (key, position, size, angle))| {
                    Some((sprites.texture_of(*key)?, ghost.feet, *position, *size, tint, *angle, sprite_layer(index, player, flat)))
                }));
            }
        }
        for draw in &companion_draws {
            let actor = match draw.owner {
                Owner::Player => &game.player,
                Owner::Unit(id) => match game.units.get(&id) {
                    Some(actor) => actor,
                    None => continue,
                },
                Owner::Item(_) | Owner::SkillUnit(_) => continue,
            };
            let Some(sprites) = (if draw.warg { actor.warg.as_ref() } else { actor.falcon.as_ref() }).and_then(actor::Companion::sprites) else { continue };
            billboards.extend(draw.placed.iter().enumerate().map(|(index, (key, position, size, angle))| {
                (sprites.texture(*key), draw.feet, *position, *size, [1.0; 4], *angle, sprite_layer(index, false, false))
            }));
        }
        billboards.extend(in_front);
        // The emotion list's pictures, posed while the sprite can still decode them.
        let emotion_icons = match (&game.emotion_list, &mut game.emotion_sprite) {
            (Some(list), Some(sprite)) => list.icons(gpu, sprite),
            _ => Vec::new(),
        };
        // Taming's slot machine: its result played once, then held a moment.
        // The PvP counter while the map counts and a rank came, the first-place clock while we're first among more than
        // one, and the banner its first 3 s: each glyph's frame at its anchor.
        let counting = game.map_property.is_some_and(|(_, flags)| flags & net::MAP_COUNT_PK != 0);
        let glyph_frames =
            |font: &mut Option<Option<SpriteSet>>, path: &str, glyphs: Vec<(usize, [f32; 2])>, timing: Timing| -> Vec<(Vec<actor::Placed>, [f32; 2])> {
                match font.get_or_insert_with(|| SpriteSet::at(grf, path)) {
                    Some(font) => glyphs.into_iter().map(|(action, at)| (font.frame_at(gpu, action, 0, timing), at)).collect(),
                    None => Vec::new(),
                }
            };
        let counter = match game.pvp_rank.filter(|_| counting) {
            Some((rank, players)) => {
                glyph_frames(&mut game.rank_font, "data\\sprite\\이팩트\\rankfont", counter_glyphs(rank, players, screen()), Timing::Span(0.0))
            }
            None => Vec::new(),
        };
        let first_place = game.first_since.filter(|_| counting && game.pvp_rank.is_some_and(|(_, players)| players > 1));
        let clock = match first_place {
            Some(since) => {
                let glyphs = clock_glyphs(now.saturating_duration_since(since).as_secs());
                let left = screen()[0] / 2.0 - 20.0 * glyphs.len() as f32;
                let placed = glyphs.into_iter().enumerate().map(|(n, glyph)| (glyph, [left + n as f32 * 40.0, 40.0])).collect();
                glyph_frames(&mut game.time_font, "data\\sprite\\이팩트\\timefont", placed, Timing::Span(0.0))
            }
            None => Vec::new(),
        };
        let banner = match first_place.map(|since| now.saturating_duration_since(since)).filter(|elapsed| *elapsed < FIRST_PLACE_BANNER) {
            Some(elapsed) => glyph_frames(
                &mut game.time_attack,
                "data\\sprite\\이팩트\\timeattack",
                vec![(0, [screen()[0] / 2.0, 120.0])],
                Timing::Loop(elapsed.as_secs_f32() * 1000.0),
            ),
            None => Vec::new(),
        };
        // The quest details: the description wrapped for its panel; the NPC picture's sprite, loaded once per name.
        game.quest_window.lay_out(font, quest_texts);
        let quest_npc = game.quest_window.npc_sprite(quest_texts, &game.quests);
        if let Some((name, _)) = &quest_npc
            && game.quest_npc.as_ref().is_none_or(|(loaded, _)| loaded != name)
        {
            game.quest_npc = Some((name.clone(), SpriteSet::at(grf, &format!("data\\sprite\\npc\\{name}"))));
        }
        let quest_npc_frame = match (quest_npc, game.quest_npc.as_mut()) {
            (Some((_, feet)), Some((_, Some(sprite)))) => sprite.frame(gpu, 0, 0, 0, Timing::Span(0.0)).into_iter().map(|placed| (placed, feet)).collect(),
            _ => Vec::new(),
        };
        let slot_frame = match (&mut game.slot_sprite, game.slot.as_ref()) {
            (Some(sprite), Some(machine)) => match machine.frame(now, [0, 1].map(|action| sprite.frames(action))) {
                Some((action, frame)) => {
                    let count = sprite.frames(action).max(1);
                    sprite.frame_at(gpu, action, 0, Timing::Span((frame as f32 + 0.5) / count as f32))
                }
                None => {
                    game.slot = None;
                    Vec::new()
                }
            },
            _ => Vec::new(),
        };
        // Emotions play once over heads.
        let mut shown = Vec::new();
        if let Some(sprite) = &mut game.emotion_sprite {
            game.emotions.retain(|_, (action, start)| head_marks::playing(now.duration_since(*start), sprite.index_ms(*action).unwrap_or(emotion::SHOWN_MS)));
            for (unit, (action, start)) in &game.emotions {
                let feet = match unit {
                    None => player_feet,
                    Some(id) => match feet_of.get(id) {
                        Some(feet) => *feet,
                        None => continue,
                    },
                };
                let elapsed = now.duration_since(*start).as_secs_f32() * 1000.0;
                shown.push((feet, sprite.frame_at(gpu, *action, 0, Timing::Loop(elapsed))));
            }
        }
        // NPCs' quest marks loop over their heads while they're in view.
        let mut quest_shown = Vec::new();
        if let Some(sprite) = &mut game.emotion_sprite {
            for &(npc, _, effect, _) in &game.quest_marks {
                let (Some(action), Some(feet)) = (head_marks::quest_action(effect), feet_of.get(&npc)) else { continue };
                let started = game.quest_started.get(&npc).copied().unwrap_or(now);
                let elapsed = now.saturating_duration_since(started).as_secs_f32() * 1000.0;
                quest_shown.push((head_marks::quest_anchor(*feet), sprite.frame_at(gpu, action, 0, Timing::Loop(elapsed))));
            }
        }
        // The MVP sprite plays once over the winner, where emotions hang, following them.
        let mut mvp_shown = Vec::new();
        if let Some(sprite) = &mut game.mvp_sprite {
            let length = sprite.index_ms(0).unwrap_or(emotion::SHOWN_MS);
            game.mvps.retain(|_, start| head_marks::playing(now.duration_since(*start), length));
            for (unit, start) in &game.mvps {
                let Some(feet) = unit.map_or(Some(player_feet), |id| feet_of.get(&id).copied()) else { continue };
                mvp_shown.push((feet, sprite.frame_at(gpu, 0, 0, Timing::Loop(now.duration_since(*start).as_secs_f32() * 1000.0))));
            }
        }
        // Status marks: stun stars, sleep, curse and silence's "..." over heads, the ice block at a frozen unit's feet.
        let looped = now.duration_since(game.entered).as_secs_f32() * 1000.0;
        let mut marked = Vec::new();
        let states = std::iter::once((player_feet, game.player.body_state, game.player.health_state))
            .chain(game.units.iter().filter_map(|(id, actor)| feet_of.get(id).map(|&feet| (feet, actor.body_state, actor.health_state))));
        for (feet, body, health) in states {
            for mark in head_marks::marks(body, health) {
                let anchor = head_marks::anchor(feet, mark.height());
                match mark.sprite() {
                    Some(name) => {
                        if let Some(sprite) = game.mark_sprites.get_mut(name) {
                            marked.push((Some(name), anchor, sprite.frame_at(gpu, 0, 0, Timing::Loop(looped))));
                        }
                    }
                    None => {
                        if let (Some(sprite), Some(action)) = (&mut game.emotion_sprite, emotion::sprite_action(head_marks::SILENCE_EMOTION)) {
                            marked.push((None, anchor, sprite.frame_at(gpu, action, 0, Timing::Loop(looped))));
                        }
                    }
                }
            }
        }
        if let Some(sprite) = &game.emotion_sprite {
            billboards.extend(shown.into_iter().flat_map(|(feet, placements)| {
                let anchor = head_marks::anchor(feet, head_marks::ANCHOR);
                placements.into_iter().map(move |(key, position, size, angle)| {
                    (sprite.texture(key), anchor, position, size, [1.0; 4], angle, DepthLayer { nudge: IN_FRONT, ..DepthLayer::default() })
                })
            }));
        }
        if let Some(sprite) = &game.emotion_sprite {
            billboards.extend(quest_shown.into_iter().flat_map(|(anchor, placements)| {
                placements.into_iter().map(move |(key, position, size, angle)| {
                    (sprite.texture(key), anchor, position, size, [1.0; 4], angle, DepthLayer { nudge: IN_FRONT, ..DepthLayer::default() })
                })
            }));
        }
        if let Some(sprite) = &game.mvp_sprite {
            billboards.extend(mvp_shown.into_iter().flat_map(|(feet, placements)| {
                let anchor = head_marks::anchor(feet, head_marks::ANCHOR);
                placements.into_iter().map(move |(key, position, size, angle)| {
                    (sprite.texture(key), anchor, position, size, [1.0; 4], angle, DepthLayer { nudge: IN_FRONT, ..DepthLayer::default() })
                })
            }));
        }
        for (name, anchor, placements) in marked {
            let sprite = match name {
                Some(name) => game.mark_sprites.get(name),
                None => game.emotion_sprite.as_ref(),
            };
            if let Some(sprite) = sprite {
                billboards.extend(placements.into_iter().map(|(key, position, size, angle)| {
                    (sprite.texture(key), anchor, position, size, [1.0; 4], angle, DepthLayer { nudge: IN_FRONT, ..DepthLayer::default() })
                }));
            }
        }

        // Numbers over units: hits arc and drift away from where the target faces, shrinking and fading; MISS rises
        // over the attacker; heals rise slowly; a multi-hit's total pops up high (the newest only); a critical's burst
        // follows its number; Lucky plays once over the dodger.
        if let Some(digits) = &game.digits {
            let mut newest_combo: HashMap<Option<u32>, Instant> = HashMap::new();
            for damage in game.damages.iter().filter(|damage| damage.start <= now && damage.kind == DamageKind::Combo) {
                let newest = newest_combo.entry(damage.target).or_insert(damage.start);
                *newest = (*newest).max(damage.start);
            }
            let message_sprite = game.message_sprite.as_ref();
            for (index, damage) in game.damages.iter().enumerate().filter(|(_, damage)| damage.start <= now) {
                let Some(feet) = damage.target.map_or(Some(player_feet), |id| feet_of.get(&id).copied()) else { continue };
                let elapsed = now.duration_since(damage.start).as_secs_f32();
                let posed = message_frames.get(index).map(Vec::as_slice).unwrap_or_default();
                if elapsed >= damage.kind.duration(damage.amount) {
                    continue;
                }
                let ticks = elapsed / 0.024;
                let at = |motion: &actor::NumberMotion| {
                    let [east, north] = damage.drift.map(|sign| sign * motion.drift);
                    [feet[0] + east, feet[1] + motion.height, feet[2] + north]
                };
                // Digits stand a step in front of a critical's burst behind them.
                let digits_layer = DepthLayer { nudge: 1.0, ..DepthLayer::default() };
                let number = |motion: actor::NumberMotion, color: [f32; 3]| {
                    let tint = [color[0], color[1], color[2], motion.alpha];
                    let anchor = at(&motion);
                    digits
                        .place(damage.amount, motion.scale)
                        .into_iter()
                        .map(move |(texture, position, size)| (texture, anchor, position, size, tint, 0.0, digits_layer))
                };
                match damage.kind {
                    DamageKind::Hit { taken, sp } => {
                        let color = if sp {
                            [0x1E as f32 / 255.0, 0x90 as f32 / 255.0, 1.0]
                        } else if taken {
                            [1.0, 0.0, 0.0]
                        } else {
                            [1.0; 3]
                        };
                        billboards.extend(number(actor::hit_number(ticks, false), color));
                    }
                    DamageKind::Critical => {
                        if let Some(sprite) = message_sprite {
                            let motion = actor::burst(ticks);
                            let tint = [0xAA as f32 / 255.0, 0xAA as f32 / 255.0, 0xAA as f32 / 255.0, motion.alpha];
                            let anchor = at(&motion);
                            billboards.extend(posed.iter().map(|&(key, position, size, angle)| {
                                (
                                    sprite.texture(key),
                                    anchor,
                                    position.map(|value| value * motion.scale),
                                    size.map(|value| value * motion.scale),
                                    tint,
                                    angle,
                                    DepthLayer::default(),
                                )
                            }));
                        }
                        billboards.extend(number(actor::hit_number(ticks, true), [1.0, 1.0, 0.0]));
                    }
                    DamageKind::Miss { red } => {
                        let motion = actor::miss_number(ticks);
                        let size = digits.miss.size.map(|value| value * motion.scale);
                        let color = if red { [1.0, 0.0, 0.0, motion.alpha] } else { [1.0, 1.0, 1.0, motion.alpha] };
                        billboards.push((&digits.miss, at(&motion), [-size[0] / 2.0, -size[1] / 2.0], size, color, 0.0, DepthLayer::default()));
                    }
                    DamageKind::Lucky => {
                        if let Some(sprite) = message_sprite {
                            let anchor = [feet[0], feet[1] + actor::LUCKY_HEIGHT, feet[2]];
                            billboards.extend(
                                posed
                                    .iter()
                                    .map(|&(key, position, size, angle)| (sprite.texture(key), anchor, position, size, [1.0; 4], angle, DepthLayer::default())),
                            );
                        }
                    }
                    DamageKind::Heal { sp } => billboards.extend(number(actor::heal_number(ticks), if sp { [0.0, 0.0, 1.0] } else { [0.0, 1.0, 0.0] })),
                    DamageKind::Combo => {
                        if newest_combo.get(&damage.target) == Some(&damage.start) {
                            billboards.extend(number(actor::total_number(ticks), [1.0, 1.0, 0.0]));
                        }
                    }
                }
            }
        }

        let viewed_preview: Vec<Sprite> = match (viewed_feet, game.viewing.as_ref().and_then(|viewed| viewed.sprites.as_ref())) {
            (Some(feet), Some(sprites)) => {
                viewed_placements.iter().map(|(key, position, size, _)| (sprites.texture(*key), offset(feet, *position), *size)).collect()
            }
            _ => Vec::new(),
        };
        let preview: Vec<Sprite> = match (preview_feet, &game.player.sprites) {
            (Some(feet), Some(sprites)) => {
                preview_placements.iter().map(|(key, position, size, _)| (sprites.texture(*key), offset(feet, *position), *size)).collect()
            }
            _ => Vec::new(),
        };

        // A monster or another player under the pointer shows its name at its feet (NPCs show theirs always). A
        // player's is a block: `[Title] Name (Party)` over `Guild [Position]`, their guild's (or our clan's) emblem left
        // of both lines (drawn with the names below); the server tells those each time the pointer comes onto them, as
        // what it sends unasked can be stale.
        let mut name_block = None;
        let pointed = match hovered(&game.hit_boxes, pointer.0, players_first) {
            Some(Owner::Unit(id)) if game.units.get(&id).is_some_and(|actor| !actor.hidden && actor.kind != NPC && actor.sprites.is_some()) => Some(id),
            _ => None,
        };
        if let Some(id) = pointed
            && let Some(actor) = game.units.get(&id)
            && let Some(&feet) = feet_of.get(&id)
        {
            let tag = game.name_tags.get(&id).filter(|_| actor.kind == PLAYER);
            let color = name_color(actor.kind, actor.job, actor.level, our_summons.contains(&Some(id)), own_level);
            let siege = game.map_property.is_some_and(|(_, flags)| versus::siege(flags));
            let shown_name = if overhead::name_hidden(game.map_type, siege, game.unknown_names.contains(&id)) { HIDDEN_NAME } else { &actor.name };
            match tag {
                Some(tag) => {
                    let title = title_names.get(&tag.title).map(String::as_str);
                    let lines = overhead::name_lines(shown_name, title, &tag.party, &tag.guild, &tag.position);
                    // A guild's emblem; without a guild, rathena tells a clan as the position with no guild name, and
                    // only our own clan's emblem is known.
                    // ponytail: a clanmate is told apart by our clan's name in that field.
                    let emblem = if !tag.guild.is_empty() {
                        game.emblems.get(&actor.guild).and_then(|(_, emblem)| emblem.as_ref())
                    } else {
                        game.clan.as_ref().filter(|clan| !tag.position.is_empty() && clan.name == tag.position).and_then(|clan| assets.clan.emblem(clan.id))
                    };
                    name_block = Some((lines, feet, color, emblem.filter(|_| game.show_emblems), siege));
                }
                None => labels.push((shown_name.to_owned(), feet, color)),
            }
            if actor.kind == PLAYER && game.pointed != pointed {
                let _ = game.session.ask_name(id);
            }
        }
        game.pointed = pointed;
        // The item under the pointer shows its name and amount.
        if let Some(Owner::Item(id)) = hovered(&game.hit_boxes, pointer.0, players_first)
            && let Some(item) = game.ground.get(&id)
        {
            let name = inventory::item_name(items, item.item, item.identified, 0);
            labels.push((messages::fill(message(messages::ITEM_AMOUNT), &[&name, &item.amount]), game.world.cell_position(item.cell), ITEM_NAME_COLOR));
        }

        // In-world overlays: under the feet (in sprite pixels, the way sprites are drawn) HP bars and names under them;
        // over the head cast bars and what's said.
        let sprite_point = |feet: [f32; 3], y: f32| view.sprite_rect(feet, [0.0, y], [0.0, y]).map(|[x, y, ..]| [x * screen()[0], y * screen()[1]]);
        let mut base = Layer::default();
        for (name, feet, color) in &labels {
            let name = name.split('#').next().unwrap_or_default();
            if let Some(point) = sprite_point(*feet, overhead::NAME_BELOW) {
                let [x, y] = overhead::label_corner(point, font.width(name), font.line_height(), screen());
                base.outlined_in(name, [x, y + overhead::NAME_TEXT_TOP], *color);
            }
        }
        // The hovered player's block: the emblem (and on siege maps its frame) at the left, centred on the lines, the
        // text after it.
        // ponytail: each line centred in the text area; the siege frame always drawn (the game's option for it is
        // unknown).
        game.hover_block = None;
        if let Some((lines, feet, color, emblem, siege)) = &name_block
            && let Some(point) = sprite_point(*feet, overhead::NAME_BELOW)
        {
            let frame = assets.guild.emblem_frame();
            let layout = overhead::block_layout(emblem.is_some(), siege.then_some(frame.size[0]));
            let text_width = lines.iter().map(|line| font.width(line)).fold(0.0, f32::max);
            let height = overhead::NAME_TEXT_TOP + overhead::NAME_LINE * lines.len() as f32;
            let [left, top] = overhead::label_corner(point, layout.text_x + text_width, height, screen());
            for (row, line) in lines.iter().enumerate() {
                let x = left + layout.text_x + ((text_width - font.width(line)) / 2.0).floor();
                base.outlined_in(line, [x, top + overhead::NAME_TEXT_TOP + overhead::NAME_LINE * row as f32], *color);
            }
            if let Some(frame_x) = layout.frame_x {
                base.sprites.push((frame, [left + frame_x, top + overhead::centred_on_lines(lines.len(), frame.size[1])], frame.size));
            }
            let emblem_box = emblem.zip(layout.emblem_x).map(|(emblem, x)| {
                let corner = [left + x, top + overhead::centred_on_lines(lines.len(), guild::EMBLEM_SIZE[1])];
                base.sprites.push((emblem, corner, guild::EMBLEM_SIZE));
                [corner[0], corner[1], guild::EMBLEM_SIZE[0], guild::EMBLEM_SIZE[1]]
            });
            game.hover_block = Some((lines.clone(), emblem_box));
        }
        // ponytail: nothing says how camps look; a square in the camp's color over each player's head, and the score
        // as "A : B" under the minimap (the top middle is the announcements').
        if let (Some(ours), Some((_, marks))) = (game.battle.camp(game.account_id), &game.camp_looks) {
            for (id, camp) in &game.battle.camps {
                let Some(above) = feet_of.get(id).and_then(|&feet| sprite_point(feet, -100.0)) else { continue };
                let mark = &marks[usize::from(*camp != ours)];
                base.sprites.push((mark, [above[0] - 4.0, above[1] - 12.0], [8.0, 8.0]));
            }
        }
        if let Some([a, b]) = game.battle.score {
            let score = format!("{a} : {b}");
            base.outlined(&score, [(screen()[0] - 12.0 - font.width(&score)).floor(), 166.0]);
        }
        // On a siege map (not a battleground) other players in a guild, and the guardians, carry its emblem over their
        // heads; it follows the unit but doesn't grow with the zoom.
        game.head_emblems.clear();
        if game.map_property.is_some_and(|(_, flags)| versus::siege(flags)) {
            for (id, actor) in &game.units {
                let wears = guild::wears_head_emblem(actor.kind == PLAYER, actor.job, actor.guild, actor.cloaked || actor.hidden, *id == game.account_id);
                let Some(&feet) = feet_of.get(id).filter(|_| wears) else { continue };
                let (Some(emblem), Some(at_feet), Some(above)) =
                    (game.emblems.get(&actor.guild).and_then(|(_, emblem)| emblem.as_ref()), sprite_point(feet, 0.0), sprite_point(feet, -100.0))
                else {
                    continue;
                };
                // The unit's perspective factor, from the screen pixels a sprite pixel takes at its feet.
                let perspective = guild::perspective((at_feet[1] - above[1]) / 100.0, screen()[1]);
                let [left, top, right, bottom] = guild::head_emblem_box(at_feet, screen(), perspective);
                base.sprites.push((emblem, [left.round(), top.round()], guild::EMBLEM_SIZE));
                game.head_emblems.push((actor.name.clone(), actor.guild, [left, top, right, bottom]));
            }
        }
        let status = |id| game.status.get(&id).copied().unwrap_or(0);
        let [border, background, green, red, blue, hunger, third] = &assets.bars;
        // Each bar: whose, where, its rows (value, maximum, color, low color), its border and background.
        let mut bars = Vec::new();
        if game.show_bars {
            let mut rows =
                vec![(status(net::STATUS_HP), status(net::STATUS_MAX_HP), green, red), (status(net::STATUS_SP), status(net::STATUS_MAX_SP), blue, blue)];
            // ponytail: a 4th job's third row taken as its AP.
            if basic_info::fourth_job(game.me.job) {
                rows.push((status(net::STATUS_AP), status(net::STATUS_MAX_AP), third, third));
            }
            bars.push(("player".to_string(), player_feet, rows, [border, background], overhead::bar_fill as overhead::Fill));
        }
        // The player's own summons while in sight: HP, SP, and a homunculus's or pet's hunger.
        // ponytail: hunger fills over a maximum of 100.
        let pair = |value: u32, max: u32| (i64::from(value), i64::from(max));
        let hp_sp = |[hp, max_hp, sp, max_sp]: [u32; 4]| {
            let ((hp, max_hp), (sp, max_sp)) = (pair(hp, max_hp), pair(sp, max_sp));
            vec![(hp, max_hp, green, red), (sp, max_sp, blue, blue)]
        };
        let mut summons = Vec::new();
        if let (Some(id), Some(homunculus)) = (game.homunculus_id, &game.homunculus) {
            let mut rows = hp_sp([homunculus.hp, homunculus.max_hp, homunculus.sp, homunculus.max_sp]);
            rows.push((i64::from(homunculus.hunger), 100, hunger, red));
            summons.push(("homunculus", id, rows));
        }
        if let Some(hired) = &game.mercenary {
            summons.push(("mercenary", hired.id, hp_sp([hired.hp, hired.max_hp, hired.sp, hired.max_sp])));
        }
        if let Some((id, values)) = game.elemental {
            summons.push(("elemental", id, hp_sp(values)));
        }
        if let (Some(id), Some(pet)) = (game.pet_id, &game.pet) {
            summons.push(("pet", id, vec![(i64::from(pet.hunger), 100, hunger, red)]));
        }
        for (kind, id, rows) in summons {
            if let Some(&feet) = feet_of.get(&id) {
                bars.push((kind.to_string(), feet, rows, [border, background], overhead::bar_fill));
            }
        }
        // Monsters' bars in their own colors; a dying unit's bar shows empty until it's gone. Party members and
        // battleground teammates fill by whole percents.
        bars.extend(game.unit_hp.iter().filter_map(|(id, &(hp, max))| {
            let actor = game.units.get(id)?;
            let hp = if actor.remove_at.is_some() { 0 } else { hp };
            let monster = actor.kind == MONSTER;
            let [border, background, fill, low] = if monster { assets.monster_bars.each_ref() } else { [border, background, green, red] };
            let filled: overhead::Fill = if monster { overhead::bar_fill } else { overhead::party_fill };
            Some((id.to_string(), *feet_of.get(id)?, vec![(i64::from(hp), i64::from(max), fill, low)], [border, background], filled))
        }));
        game.bars_drawn.clear();
        for (whose, feet, rows, [border, background], filled) in bars {
            let Some(point) = sprite_point(feet, overhead::BAR_BELOW) else { continue };
            let [x, y] = [point[0].trunc() - overhead::BAR_WIDTH / 2.0, point[1].trunc()];
            let rects = overhead::gage(&rows, border, background, filled);
            game.bars_drawn.push((whose, [x, y, rects[0].1[2], rects[0].1[3]], rows.iter().map(|&(value, max, ..)| filled(value, max)).collect()));
            for (texture, [left, top, width, height]) in rects {
                base.sprites.push((texture, [x + left, y + top], [width, height]));
            }
        }

        // Over heads: cast bars filling green, and the words said there.
        game.casts.retain(|_, (start, length)| now.duration_since(*start) < *length);
        game.speech.retain(|_, (.., said)| now.duration_since(*said) < SPEECH_TIME);
        game.talkie_spots.retain(|id, _| game.speech.contains_key(&Some(*id)));
        let head = |unit: &Option<u32>| -> Option<[f32; 2]> {
            let feet = match unit {
                None => player_feet,
                Some(id) => *feet_of.get(id)?,
            };
            sprite_point(feet, -overhead::HEAD).map(|point| point.map(f32::round))
        };
        let [border, background, ..] = &assets.bars;
        for (unit, (start, length)) in &game.casts {
            let feet = match unit {
                None => player_feet,
                Some(id) => match feet_of.get(id) {
                    Some(&feet) => feet,
                    None => continue,
                },
            };
            let Some(top) = sprite_point(feet, -overhead::CAST_ABOVE) else { continue };
            let [left, y, width, height] = overhead::cast_rect(top);
            let done = now.duration_since(*start).as_secs_f32() / length.as_secs_f32();
            base.sprites.push((border, [left, y], [width, height]));
            base.sprites.push((background, [left + 1.0, y + 1.0], [width - 2.0, height - 2.0]));
            base.sprites.push((&assets.cast_fill, [left + 1.0, y + 1.0], [overhead::cast_fill(done), height - 2.0]));
        }
        // Combo windows: a bar under the cast bar's place, emptying as the window runs out.
        // ponytail: how the official client shows a combo window isn't knowable from rathena.
        game.combos.retain(|_, (start, length)| now.duration_since(*start) < *length);
        for (unit, &(start, length)) in &game.combos {
            let Some([x, y]) = head(unit) else { continue };
            let [left, top] = [x - overhead::CAST_BAR[0] / 2.0, y + overhead::CAST_BAR[1] + 1.0];
            base.sprites.push((border, [left, top], overhead::CAST_BAR));
            base.sprites.push((background, [left + 1.0, top + 1.0], [overhead::CAST_BAR[0] - 2.0, overhead::CAST_BAR[1] - 2.0]));
            base.sprites.push((
                &assets.combo_fill,
                [left + 1.0, top + 1.0],
                [((overhead::CAST_BAR[0] - 2.0) * share_left(start, length, now)).round(), overhead::CAST_BAR[1] - 2.0],
            ));
        }
        // Navigation: there is the end of it; else the trail to where it heads on this map, and the arrow over the head.
        let here = navigation::map_key(&game.map);
        if game.navigation.as_ref().is_some_and(|active| navigation::arrived(&here, game.player.cell, &active.target)) {
            game.navigation = None;
        }
        let navi_next = game.navigation.as_ref().and_then(|active| navigation::next_cell(&self.navi_tables, &here, &active.target, active.services));
        if let Some(active) = &mut game.navigation {
            let gat = &game.world.gat;
            active.update_trail(&here, game.player.cell, navi_next, |from, to| world::find_trail(gat, from, to));
        }
        if let Some(next) = navi_next {
            // A point a few cells toward it, so a far goal off the screen still turns the arrow.
            let toward = [f32::from(next[0]) - game.player.cell[0] as f32, f32::from(next[1]) - game.player.cell[1] as f32];
            let length = toward[0].hypot(toward[1]).max(1.0);
            let near = [0, 1].map(|axis| game.player.cell[axis] as f32 + toward[axis] / length * NAVI_ARROW_LOOK.min(length));
            let goal = game.world.cell_position(near);
            if let (Some(from), Some(to), Some([x, y])) = (sprite_point(player_feet, 0.0), sprite_point(goal, 0.0), head(&None)) {
                let arrow = &assets.navi_arrows[navigation::arrow([to[0] - from[0], to[1] - from[1]])];
                base.sprites.push((arrow, [x - arrow.size[0] / 2.0, y - NAVI_ARROW_RISE - arrow.size[1]], arrow.size));
            }
        }
        // Chat room signs over their owners' heads, like speech (not the room we're in).
        game.sign_rects.clear();
        for (owner, sign) in &game.signs {
            if game.room.as_ref().is_some_and(|room| room.sign.id == sign.id) {
                continue;
            }
            let unit = (*owner != game.account_id).then_some(*owner);
            let feet = match unit {
                None => player_feet,
                Some(id) => match feet_of.get(&id) {
                    Some(&feet) => feet,
                    None => continue,
                },
            };
            let Some(point) = sprite_point(feet, -chatroom::SIGN_ABOVE) else { continue };
            // ponytail: the one yellow shop board is the vendor whose shop window is open.
            let picked = game.shop.as_ref().and_then(|shop| shop.vendor.as_ref()).is_some_and(|(vendor, ..)| vendor == owner);
            let rect = chatroom::draw_sign(&mut base, &assets.chatroom, font, sign, point.map(f32::round), picked);
            game.sign_rects.push((rect, sign.id, sign.kind));
        }
        for (unit, (text, color, _)) in &game.speech {
            let Some(anchor) = head(unit) else { continue };
            let lines = overhead::lines(text);
            let size = overhead::size(lines.iter().map(|line| font.width(line)).fold(0.0, f32::max), lines.len());
            let corner = overhead::corner(anchor, size, screen());
            base.sprites.push((&assets.speech_fill, corner, size));
            for (edge, piece) in overhead::border(size) {
                base.sprites.push((&assets.speech_border, offset(corner, edge), piece));
            }
            for (row, line) in lines.into_iter().enumerate() {
                let place = offset(corner, [overhead::TEXT_AT[0], overhead::TEXT_AT[1] + row as f32 * overhead::LINE_STEP]);
                base.texts.push((line.clone(), offset(place, [1.0, 1.0]), [0, 0, 0]));
                base.texts.push((line, place, *color));
            }
        }

        // NPC windows on top.
        let mut overlay = Layer::default();
        let mut dialog_links = Vec::new();
        if let Some(dialog) = &game.dialog {
            // A warp list is the menu alone.
            if dialog.warp.is_none() {
                let lines: links::LinkedLines =
                    dialog.lines.iter().flat_map(|line| wrap_linked_by(font, line, npc_say::WRAP_WIDTH, true, links::Target::npc_color)).collect();
                let pitch = font.line_height() + npc_say::LINE_GAP;
                game.say.text_height = lines.len() as f32 * pitch;
                game.say.scroll_by(0.0);
                game.say.draw(&mut overlay, &assets.chrome, &assets.pickup, &assets.say);
                for (row, pieces) in lines.iter().enumerate() {
                    let Some([mut x, y]) = game.say.line_at(row, pitch, font.line_height()) else { continue };
                    for (color, piece, item) in pieces {
                        let width = font.width(piece);
                        overlay.texts.push((piece.clone(), [x, y], *color));
                        // Links underlined in their colour, and clickable.
                        if let Some(item) = item {
                            let line = if matches!(item, links::Target::Url(_)) { &assets.url_line } else { &assets.link_line };
                            overlay.sprites.push((line, [x, y + font.line_height() - 1.0], [width, 1.0]));
                            dialog_links.push(([x, y, width, font.line_height()], item.clone()));
                        }
                        x += width;
                    }
                }
                let [x, y, _, _] = game.say.button_box();
                match dialog.button {
                    DialogButton::Next => overlay.sprites.push(button_sprite(pointer, [x, y], &assets.next)),
                    DialogButton::Close => overlay.sprites.push(button_sprite(pointer, [x, y], &assets.close)),
                    DialogButton::None => {}
                }
            }
            if let Some(menu) = &dialog.menu {
                let corner = game.npc_menu.position;
                menu.draw(overlay.window(), &assets.chrome, &assets.pickup, &assets.npc_menu, corner, font);
                let [ok, cancel] = npc_menu::buttons(menu.items.len()).map(|place| offset(corner, place));
                overlay.sprites.push(button_sprite(pointer, ok, &assets.ok));
                overlay.sprites.push(button_sprite(pointer, cancel, &assets.cancel));
            }
            if let Some(input) = &dialog.input {
                input.draw(overlay.window(), &assets.chrome, &assets.pickup, &assets.npc_input, screen());
                overlay.sprites.push(button_sprite(pointer, input.ok_on(screen()), &assets.ok));
            }
        }

        // Windows over the world, NPC conversations over windows, tooltips over everything.
        let shown_items = game
            .inventory
            .items
            .iter()
            .chain(game.cart.items.iter().filter(|_| game.cart.frame.open))
            .map(|item| (item.id, item.identified))
            .chain(game.hotkeys.keys.iter().flatten().filter(|key| !key.skill).map(|key| (key.id, true)))
            .chain(game.viewing.iter().flat_map(|viewed| viewed.items.iter()).map(|item| (item.id, item.identified)));
        for (id, identified) in shown_items {
            if let Some(info) = items.get(&id) {
                let path = inventory::icon_path(info, identified);
                icons.entry(path.clone()).or_insert_with(|| gpu.try_load_texture(grf, &path, false).ok());
            }
        }
        let jobs = skills::tabs(skill_trees, game.me.job, &game.skills);
        let points = game.skill_points();
        let sheet = skills::Sheet { trees: skill_trees, jobs: &jobs, skills: &game.skills, info: skill_info, points };
        let skill_icons = game
            .skill_window
            .icons(&sheet)
            .into_iter()
            .chain(game.guild.as_ref().map(|guild| game.guild_window.icons(guild, skill_info)).unwrap_or_default())
            .chain(mercenary::SkillList::icons(&game.mercenary_skills, skill_info))
            .chain(homunculus::SkillList::icons(&game.homunculus_skills, skill_info))
            .chain(game.hotkeys.keys.iter().flatten().filter(|key| key.skill).filter_map(|key| skill_info.get(&(key.id as u16)).map(skills::icon_path)));
        let status_paths =
            game.status_icons.iter().filter_map(|(kind, ..)| status_icon_files.get(kind).map(|(_, file)| format!("data\\texture\\effect\\{file}")));
        let mut shop_icons = game.shop.as_ref().map(|shop| shop.icons(items)).unwrap_or_default();
        shop_icons.extend(game.cashshop.as_ref().map(|shop| shop.icons(items)).unwrap_or_default());
        let mut pick_icons = game.pick.as_ref().map(|(list, _)| list.icons(items, skill_info)).unwrap_or_default();
        pick_icons.extend(game.refine.as_ref().map(|window| window.icons(items, &game.inventory.items)).unwrap_or_default());
        pick_icons.extend(game.reform.as_ref().map(|window| window.icons(items)).unwrap_or_default());
        pick_icons.extend(game.grade.as_ref().map(|window| window.icons(items, &game.inventory.items)).unwrap_or_default());
        pick_icons.extend(game.enchant.as_ref().map(|window| window.icons(items, &game.inventory.items)).unwrap_or_default());
        pick_icons.extend(game.materials.as_ref().map(|window| window.icons(items, &game.inventory.items)).unwrap_or_default());
        pick_icons.extend(game.synthesis.as_ref().map(|window| window.icons(items, &game.inventory.items)).unwrap_or_default());
        pick_icons.extend(game.upgrade.as_ref().map(|window| window.icons(items, &game.inventory.items)).unwrap_or_default());
        pick_icons.extend(game.achievement_window.icons(achievement_info));
        pick_icons.extend(game.roulette.as_ref().map(|window| window.icons(items)).unwrap_or_default());
        // With compare on, the description's item next to what's worn in its slot.
        let compared = game
            .item_window
            .as_ref()
            .filter(|window| game.inventory.compare && !window.compare)
            .and_then(|window| Some((window.position, iteminfo::compared(&window.shown, &game.inventory.items)?)))
            .map(|(position, worn)| {
                let mut window = iteminfo::ItemWindow::new(iteminfo::Shown::of(worn));
                window.compare = true;
                window.position = iteminfo::compare_place(position);
                window
            });
        let mut trade_icons = game.trade.as_ref().map(|trade| trade.icons(items)).unwrap_or_default();
        trade_icons.extend(game.item_window.as_ref().map(|window| window.images(items)).unwrap_or_default());
        trade_icons.extend(game.card_window.as_ref().map(|window| window.images(items)).unwrap_or_default());
        trade_icons.extend(compared.iter().flat_map(|window| window.images(items)));
        trade_icons.extend(game.card_view.iter().map(|view| view.path.clone()));
        trade_icons.extend(game.quest_window.icons(&game.quests, quest_texts, items));
        if game.vend_setup.is_some() {
            trade_icons.extend(vend::icons(items, game.cart.items.iter().map(|item| (item.id, item.identified))));
        }
        if let Some(shop) = &game.my_shop {
            trade_icons.extend(vend::icons(items, shop.goods.iter().map(|good| (good.item.id, good.item.identified))));
        }
        if game.buying_setup.is_some() {
            trade_icons.extend(vend::icons(items, game.inventory.items.iter().map(|item| (item.id, item.identified))));
        }
        if let Some(store) = &game.my_buying_store {
            trade_icons.extend(vend::icons(items, store.items.iter().map(|item| (item.item_id, true))));
        }
        trade_icons.extend(game.mail.icons(items));
        trade_icons.extend(game.pickup_popup.as_ref().map(|popup| popup.icon.clone()).filter(|icon| !icon.is_empty()));
        trade_icons.extend(game.catalog.as_ref().map(|catalog| catalog.icons(items)).unwrap_or_default());
        for path in skill_icons.chain(status_paths).chain(shop_icons).chain(pick_icons).chain(game.storage.icons(items)).chain(trade_icons) {
            icons.entry(path.clone()).or_insert_with(|| gpu.try_load_texture(grf, &path, false).ok());
        }
        party::load_icons(icons, gpu, grf, game.party.as_ref());
        // Online guild members' heads, for the member list.
        if game.guild_window.frame.open {
            for member in game.guild.iter().flat_map(|guild| guild.members.iter()).filter(|member| member.online) {
                icons.entry(guild::head_key(member)).or_insert_with(|| {
                    let look = actor::Look { job: member.job, sex: member.sex as u8, hair: member.hair, hair_color: member.hair_color, ..Default::default() };
                    actor::head_image(grf, &look).map(|image| gpu.texture(&image, false))
                });
            }
        }
        let mut tooltip = Layer::default();
        let mut chat_layer = Layer::default();
        let open = game.chat_mode.chat_line_open();
        let caret = open && (game.entered.elapsed().as_millis() / CARET_BLINK.as_millis()).is_multiple_of(2);
        let name_of = |item: &links::ItemLink| inventory::item_name(items, item.id, true, item.refine);
        // The basic info and chat windows are kept as they're left.
        game.basic_info.save_if_changed();
        game.chat.save_if_changed();
        game.chat.layout(&|text| font.width(text));
        game.chat.mode_icon = game.chat_mode.icon();
        game.chat_links = game.chat.draw(&mut chat_layer, &assets.chat, font, messages, open, caret, &name_of);
        // The input bar buttons' tooltips, over the pointer.
        if let Some(text) = game.chat.tooltip(pointer.0).and_then(|id| messages.get(id)) {
            let size = [font.width(text) + 8.0, font.line_height() + 4.0];
            let corner = [pointer.0[0] - size[0] / 2.0, pointer.0[1] - size[1] - 6.0];
            tooltip.sprites.push((&assets.speech_fill, corner, size));
            for (edge, piece) in overhead::border(size) {
                tooltip.sprites.push((&assets.speech_border, offset(corner, edge), piece));
            }
            tooltip.texts.push((text.clone(), offset(corner, [4.0, 2.0]), [0, 0, 0]));
        }
        let size = [game.world.gat.width as u32, game.world.gat.height as u32];
        // NPC marks last until they're taken down. Each blinks from the moment it was set.
        game.marks.retain(|(_, (_, _, until, _))| until.is_none_or(|until| until > now));
        // The pickup popup's five seconds; an open inventory has been seen.
        if game.pickup_popup.as_ref().is_some_and(|popup| !popup.alive(control::now())) {
            game.pickup_popup = None;
        }
        if game.inventory.frame.open {
            game.new_items = false;
        }
        game.inventory.note_open();
        // What the equipment window's dismount and Cart Items buttons go by.
        game.equipment.option = game.player.hide_option;
        game.equipment.job = game.me.job;
        game.equipment.cart = game.cart.cart.is_some();
        if game.quest_window.frame.open {
            game.quest_icon = false;
        }
        game.basic_info.fourth = basic_info::fourth_job(game.me.job);
        game.basic_info.doram = basic_info::doram(game.me.job);
        game.banners.current(control::now());
        game.level_ups.seen(level_up::opened(&game.status_window.frame), level_up::opened(&game.skill_window.frame));
        let marks: Vec<_> = game
            .marks
            .iter()
            .filter(|(_, (_, _, _, set))| minimap::mark_visible(0, now.saturating_duration_since(*set).as_millis()))
            .map(|(_, (cell, color, _, _))| (*cell, color))
            .collect();
        // Party members on this map, in their colours (their places in the list when they were added).
        if game.party_colors.is_empty() {
            game.party_colors = (0..17).map(|place| gpu.solid(minimap::party_color(place))).collect();
        }
        let here = game.map.trim_end_matches(".gat").to_owned();
        let members: Vec<_> = game
            .party
            .iter()
            .flat_map(|party| party.members.iter())
            .filter(|member| member.info.account != game.account_id && member.info.online && member.info.map.trim_end_matches(".gat") == here)
            .filter_map(|member| Some((member.color, member.info.account, member.cell?)))
            .collect();
        // On a battleground, teammates in our camp's color.
        let mut dots: Vec<_> = members.iter().map(|&(place, _, cell)| (cell, &game.party_colors[place.min(16)])).collect();
        if let Some((team_dot, _)) = &game.camp_looks {
            dots.extend(game.battle.dots.values().map(|&[x, y]| ([i32::from(x), i32::from(y)], team_dot)));
        }
        // Guild members not in the party.
        let in_party = |account: &u32| members.iter().any(|(_, member, _)| member == account);
        let guild: Vec<[i32; 2]> =
            game.guild_places.iter().filter(|(account, _)| !in_party(account) && **account != game.account_id).map(|(_, cell)| *cell).collect();
        let boss = game.boss.as_ref().map(|(_, cell)| *cell);
        // The overview's window, cut again only when the zoom, the window or the view changed.
        let overview = match (&game.minimap, game.minimap_view) {
            (_, minimap::View::Hidden) | (None, _) => None,
            (Some(image), view) => {
                let window = minimap::window(minimap::share(game.player.cell, size), game.minimap_zoom);
                let key = minimap::crop_key(image, window, game.minimap_zoom, view);
                if game.minimap_crop.as_ref().is_none_or(|(cut, _)| *cut != key) {
                    let part = minimap::crop(image, window, game.minimap_zoom, view.alpha());
                    game.minimap_crop = Some((key, gpu.texture(&part, true)));
                }
                game.minimap_crop.as_ref().map(|(_, texture)| texture)
            }
        };
        let trail = game.navigation.as_ref().map_or(&[][..], |active| active.trail.as_slice());
        let goal = navi_next.map(|cell| cell.map(i32::from));
        let markers = minimap::Markers {
            cell: game.player.cell,
            yaw: game.player.rot,
            marks: &marks,
            quests: &game.quest_marks.iter().map(|&(_, cell, _, color)| (cell, color)).collect::<Vec<_>>(),
            store: game.store_mark,
            party: &dots,
            guild: &guild,
            boss,
            trail,
            goal,
        };
        if game.minimap_shown() {
            minimap::draw(&mut chat_layer, &assets.minimap, overview, size, game.minimap_zoom, &markers, minimap::Pointer { at: pointer.0, down: pointer.1 });
        }
        if !game.minimap_window.covers(pointer.0)
            && game.window_at(pointer.0).is_none()
            && let Some(name) = minimap::button_at(pointer.0)
        {
            let text = messages.get(minimap::tooltip_id(name)).map(String::as_str).unwrap_or("");
            if !text.is_empty() {
                window::tooltip(&mut tooltip, &assets.chrome, font, text, offset(pointer.0, [12.0, 12.0]), [255; 3]);
            }
        }
        // Status icons down the right, each with the clock of its time (orange in the last minute).
        // In the game's five priority passes, 42 as 12.
        let started: Vec<(u16, (Instant, Option<Instant>))> = game.status_icons.iter().map(|&(kind, start, end)| (kind, (start, end))).collect();
        let order = status_tip::placed(&started, |kind| status_icon_files.get(&kind).map(|(priority, _)| *priority));
        let shown = order.iter().filter_map(|(kind, (start, end))| {
            let (_, file) = status_icon_files.get(kind)?;
            Some((*kind, icons.get(&format!("data\\texture\\effect\\{file}"))?.as_ref()?, start, end))
        });
        game.status_tip = None;
        for (order, (kind, icon, start, end)) in shown.enumerate() {
            let corner = status_tip::icon_corner(order, screen());
            // Pointed at: what it is and the time it has left, in a box at its left, rebuilt each frame.
            if status_tip::on_icon(corner, pointer.0)
                && let Some(info) = state_infos.get(&kind)
            {
                let left = end.filter(|_| info.time_limit).map(|end| end.saturating_duration_since(now).as_secs());
                let lines = status_tip::lines(info, left, messages);
                let widest = lines.iter().map(|(line, _)| font.width(line)).fold(0.0, f32::max);
                let (box_corner, size) = status_tip::place(corner, widest, lines.len(), font.line_height());
                tooltip.sprites.push((&assets.speech_fill, box_corner, size));
                for (edge, piece) in overhead::border(size) {
                    tooltip.sprites.push((&assets.speech_border, offset(box_corner, edge), piece));
                }
                for (row, (line, color)) in lines.iter().enumerate() {
                    let at = status_tip::line_at(box_corner, row, font.line_height());
                    tooltip.texts.push((line.clone(), offset(at, [1.0, 1.0]), [0, 0, 0]));
                    tooltip.texts.push((line.clone(), at, *color));
                }
                game.status_tip = Some(lines.into_iter().map(|(line, _)| line).collect());
            }
            let scale = (STATUS_ICON / icon.size[0].max(icon.size[1])).min(1.0);
            let size = icon.size.map(|value| value * scale);
            chat_layer.sprites.push((icon, offset(corner, [(STATUS_ICON - size[0]) / 2.0, (STATUS_ICON - size[1]) / 2.0]), size));
            if let Some(end) = end {
                let left = end.saturating_duration_since(now).as_secs_f32();
                let (color, share) = if left < 60.0 {
                    (1, 1.0 - left / 60.0)
                } else {
                    (0, now.duration_since(*start).as_secs_f32() / (end.duration_since(*start).as_secs_f32() - 60.0).max(1.0))
                };
                let step = ((share.clamp(0.0, 1.0) * CLOCK_STEPS as f32) as usize).min(CLOCK_STEPS);
                chat_layer.sprites.push((&assets.clocks[color][step], corner, [STATUS_ICON; 2]));
            }
        }
        // The quest tracker, under the minimap.
        if game.quest_window.tracker {
            quest_tracker::draw(&mut chat_layer, &quest_tracker::blocks(&game.quests, quest_texts, &game.quest_window.hidden, messages));
        }
        game.status_height = game.status_window.height();
        // Our own HP in the party window comes from our status.
        let own_hp = [net::STATUS_HP, net::STATUS_MAX_HP].map(|kind| game.status.get(&kind).copied().unwrap_or(0) as i32);
        let account = game.account_id;
        if let Some(member) = game.party.as_mut().and_then(|party| party.member(account)) {
            member.hp = Some(own_hp);
        }
        // Announcements under the windows.
        game.announcements.expire(now);
        game.announcements.prepare(gpu, font);
        let mut announce_layer = Layer::default();
        game.announcements.draw(&mut announce_layer);
        game.level_ups.draw(&mut announce_layer, &assets.level_up, pointer);
        // ponytail: the new-quest icon covers the base level button when both are up.
        if game.quest_icon {
            quest_notice::draw_icon(&mut announce_layer, &assets.quest_notice, pointer);
        }
        game.banners.draw(&mut announce_layer, &assets.quest_notice, font);
        // The pickup popup takes no clicks.
        if let Some(popup) = &game.pickup_popup {
            let icon = icons.get(&popup.icon).and_then(Option::as_ref);
            popup.draw(&mut announce_layer, &assets.chrome, &assets.pickup, icon, font);
        }
        // Weather over the map, under the names and windows.
        let mut weather_layer = Layer::default();
        if self.graphics.effects_on {
            game.weather.draw(&mut weather_layer, &assets.weather, now);
        }
        let mut layers = vec![weather_layer, base, announce_layer, chat_layer];
        let mut preview = Some(preview);
        let cooling = skills::Cooling { cooldowns: &game.cooldowns, fan: &assets.fan, now: control::now() };
        // An inventory item being dragged: where it could be worn, for the equipment window's ovals.
        let dropping = match game.dragging {
            Some((Dragged::Item(index), _, true)) => game.inventory.get(index).map(|item| item.location),
            _ => None,
        };
        for &id in &game.windows {
            let mut layer = Layer::default();
            match id {
                Panel::Hotkeys => game.hotkeys.draw(
                    &mut layer,
                    &mut tooltip,
                    &assets.chrome,
                    &assets.hotkeys,
                    icons,
                    items,
                    skill_info,
                    &game.inventory.items,
                    font,
                    pointer.0,
                    &cooling,
                ),
                Panel::Skills => {
                    game.skill_window.draw(&mut layer, &mut tooltip, &assets.chrome, &assets.skills, icons, font, messages, &sheet, pointer, &cooling)
                }
                Panel::BasicInfo => {
                    let info = basic_info::Info {
                        name: &game.me.name,
                        job: pc_job_names.get(&game.me.job).map_or("", String::as_str),
                        status: &game.status,
                        unread_mail: game.mail.unread,
                        reward_waiting: game.achievements.values().any(|achievement| achievement::claimable(Some(achievement))),
                        new_items: game.new_items,
                        new_level: game.level_ups.base,
                        new_job_level: game.level_ups.job,
                        overweight: game.overweight,
                    };
                    game.basic_info.draw(&mut layer, &assets.chrome, &assets.basic_info, font, messages, &info, pointer.0);
                }
                Panel::Inventory => game.inventory.draw(
                    &mut layer,
                    &mut tooltip,
                    &assets.chrome,
                    &assets.inventory,
                    icons,
                    items,
                    messages,
                    font,
                    pointer.0,
                    (card_names, &game.char_names),
                ),
                Panel::Cart if game.cart.cart.is_some() => game.cart.draw(
                    &mut layer,
                    &mut tooltip,
                    &assets.chrome,
                    &assets.inventory,
                    icons,
                    items,
                    messages,
                    font,
                    pointer.0,
                    (card_names, &game.char_names),
                ),
                Panel::Cart => {}
                Panel::Pet => {
                    if let Some(pet) = &game.pet {
                        let blink = (game.entered.elapsed().as_millis() / CARET_BLINK.as_millis()).is_multiple_of(2);
                        game.pet_window.draw(&mut layer, &assets.chrome, &assets.pet, font, messages, pet, pointer, blink);
                    }
                }
                Panel::Mercenary => {
                    if let Some(hired) = &game.mercenary {
                        let job = game.units.get(&hired.id).map_or(0, |actor| actor.job);
                        let unix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.as_secs());
                        game.mercenary_window.draw(&mut layer, &assets.chrome, &assets.mercenary, font, messages, hired, job, unix, pointer);
                    }
                }
                Panel::MercenarySkills => {
                    if game.mercenary.is_some() {
                        let list = &game.mercenary_skill_list;
                        list.draw(&mut layer, &assets.chrome, &assets.skills, icons, font, messages, &game.mercenary_skills, skill_info, pointer.0);
                    }
                }
                Panel::Homunculus => {
                    if let Some(homunculus) = &game.homunculus {
                        let blink = (game.entered.elapsed().as_millis() / CARET_BLINK.as_millis()).is_multiple_of(2);
                        game.homunculus_window.draw(&mut layer, &assets.chrome, &assets.homunculus, font, messages, homunculus, pointer, blink);
                    }
                }
                Panel::HomunculusSkills => {
                    if let Some(homunculus) = &game.homunculus {
                        let list = &game.homunculus_skill_list;
                        let skills = &game.homunculus_skills;
                        list.draw(&mut layer, &assets.chrome, &assets.skills, icons, font, messages, skills, homunculus.skill_points, skill_info, pointer);
                    }
                }
                Panel::Cutin => {
                    if let Some(cutin) = &game.cutin {
                        cutin.draw(&mut layer, &assets.chrome, &assets.cutin, pointer.0);
                    }
                }
                Panel::Status => {
                    let guild = game.guild.as_ref().map_or("", |guild| guild.name.as_str());
                    game.status_window.draw(&mut layer, &mut tooltip, &assets.chrome, &assets.status, font, messages, &game.status, guild, pointer);
                }
                Panel::Guild => {
                    if let Some(guild) = &game.guild {
                        let emblem = game.emblems.get(&guild.id).and_then(|(_, emblem)| emblem.as_ref());
                        game.guild_window.draw(
                            &mut layer,
                            &assets.chrome,
                            &assets.guild,
                            icons,
                            font,
                            messages,
                            guild,
                            (pc_job_names, skill_info),
                            emblem,
                            pointer,
                        );
                    }
                }
                Panel::Clan => {
                    if let Some(clan) = &game.clan {
                        game.clan_window.draw(&mut layer, &assets.chrome, &assets.clan, messages, clan, pointer.0);
                    }
                }
                Panel::Party => {
                    let stem = |map: &str| map.trim_end_matches(".gat").to_owned();
                    let our_hp = [net::STATUS_HP, net::STATUS_MAX_HP].map(|id| game.status.get(&id).copied().unwrap_or(0) as i32);
                    let on_screen = |account: u32| game.units.contains_key(&account);
                    game.party_window.draw(
                        &mut layer,
                        &mut tooltip,
                        &assets.chrome,
                        &assets.party,
                        icons,
                        font,
                        messages,
                        game.party.as_ref(),
                        &game.friends,
                        (game.account_id, our_hp, &on_screen, &stem),
                        &game.member_windows.open.iter().map(|window| window.account).collect::<Vec<_>>(),
                        pointer,
                    );
                }
                Panel::Storage => {
                    let names = (&messages[..], card_names, &game.char_names);
                    game.storage.draw(&mut layer, &mut tooltip, &assets.chrome, &assets.storage, icons, items, names, font, pointer, caret);
                }
                Panel::Equipment => game.equipment.draw(
                    &mut layer,
                    &mut tooltip,
                    &assets.chrome,
                    &assets.equipment,
                    icons,
                    items,
                    messages,
                    font,
                    pointer.0,
                    &game.inventory.items,
                    preview.take().unwrap_or_default(),
                    (card_names, &game.char_names),
                    dropping,
                ),
            }
            layer.faded = game.panel_faded(id);
            layers.push(layer);
        }
        // Party members dragged out of the list, over the other windows.
        let mut member_layer = Layer::default();
        let mut member_tooltip = Layer::default();
        {
            let stem = |map: &str| self.map_names.get(map.trim_end_matches(".gat")).cloned().unwrap_or_else(|| map.trim_end_matches(".gat").to_owned());
            let our_hp = [net::STATUS_HP, net::STATUS_MAX_HP].map(|id| game.status.get(&id).copied().unwrap_or(0) as i32);
            let on_screen = |account: u32| game.units.contains_key(&account);
            game.member_windows.draw(
                &mut member_layer,
                &mut member_tooltip,
                &assets.chrome,
                &assets.party,
                icons,
                font,
                game.party.as_ref(),
                (game.account_id, our_hp, &on_screen, &stem),
                &party::Grid::new(screen()),
                pointer.0,
            );
        }
        layers.push(member_layer);
        layers.push(member_tooltip);
        // A dragged item's or skill's icon follows the pointer.
        let dragged_icon = match game.dragging {
            Some((Dragged::Item(index) | Dragged::Worn(index), _, true)) => {
                game.inventory.get(index).and_then(|item| items.get(&item.id).map(|info| inventory::icon_path(info, item.identified)))
            }
            Some((Dragged::Skill(id), _, true)) => skill_info.get(&id).map(skills::icon_path),
            Some((Dragged::Stored(index), _, true)) => {
                game.storage.get(index).and_then(|item| items.get(&item.id).map(|info| inventory::icon_path(info, item.identified)))
            }
            Some((Dragged::ShopOffer(row), _, true)) => game
                .shop
                .as_ref()
                .and_then(|shop| shop.offer_at(0, row))
                .and_then(|offer| items.get(&offer.item).map(|info| inventory::icon_path(info, offer.identified))),
            Some((Dragged::ShopDeal(row), _, true)) => game
                .shop
                .as_ref()
                .and_then(|shop| shop.offer_at(1, row))
                .and_then(|offer| items.get(&offer.item).map(|info| inventory::icon_path(info, offer.identified))),
            Some((Dragged::Carted(index), _, true)) => {
                game.cart.get(index).and_then(|item| items.get(&item.id).map(|info| inventory::icon_path(info, item.identified)))
            }
            Some((Dragged::Hotkey(slot), _, true)) => match game.hotkeys.keys.get(slot).copied().flatten() {
                Some(net::Hotkey { skill: true, id, .. }) => u16::try_from(id).ok().and_then(|id| skill_info.get(&id)).map(skills::icon_path),
                Some(net::Hotkey { id, .. }) => items.get(&id).map(|info| inventory::icon_path(info, true)),
                None => None,
            },
            _ => None,
        };
        if let Some(Some(icon)) = dragged_icon.and_then(|path| icons.get(&path)) {
            tooltip.sprites.push((icon, [pointer.0[0] - icon.size[0] / 2.0, pointer.0[1] - icon.size[1] / 2.0], icon.size));
        }

        // An aimed skill: its name in a balloon at the top, "select a target" in one at the bottom, centred. Taming has
        // no skill: the bottom one only.
        let mut aim_lines = Vec::new();
        if let Some((id, _)) = game.targeting {
            aim_lines.push((skill_info.get(&id).map_or_else(|| id.to_string(), |info| info.name.clone()), screen()[1] * 46.0 / 480.0));
        }
        if game.targeting.is_some() || game.capturing {
            aim_lines.push((message(TARGET_MESSAGE).to_owned(), screen()[1] * 400.0 / 480.0));
        }
        for (text, top) in aim_lines {
            let size = overhead::size(font.width(&text), 1);
            let corner = [((screen()[0] - size[0]) / 2.0).floor(), top.floor()];
            tooltip.sprites.push((&assets.speech_fill, corner, size));
            for (edge, piece) in overhead::border(size) {
                tooltip.sprites.push((&assets.speech_border, offset(corner, edge), piece));
            }
            let place = offset(corner, overhead::TEXT_AT);
            tooltip.texts.push((text.clone(), offset(place, [1.0, 1.0]), [0, 0, 0]));
            tooltip.texts.push((text, place, AIM_TEXT));
        }
        // The store search under the shops it opens from afar.
        let mut catalog_layer = Layer::default();
        if let Some(catalog) = &game.catalog {
            catalog.draw(&mut catalog_layer, &assets.chrome, &assets.catalog, icons, items, font, messages, (pointer.0, pointer.1, caret));
        }
        // A shop's windows over the others; its question in a message box.
        let mut shop_layer = Layer::default();
        if let Some(settings) = &game.chat.settings {
            settings.draw(shop_layer.window(), &assets.chrome, messages, &game.chat.tabs[settings.tab].kinds, pointer.0);
        }
        // A skill's description, over the windows.
        if let Some(window) = &game.skill_description {
            window.draw(shop_layer.window(), &assets.chrome, &assets.skill_descript, font, pointer.0, &cooling, skill_info);
        }
        if let Some((list, _)) = &game.pick {
            list.draw(shop_layer.window(), &assets.chrome, &assets.pick_list, icons, items, skill_info, messages, font, pointer);
        }
        if let Some(shop) = &game.shop {
            shop.draw(shop_layer.window(), &assets.chrome, &assets.shop, icons, items, messages, font, pointer);
        }
        if game.deal_choice.is_some() {
            let dialog = deal_box_position();
            shop_layer.window().sprites.push((&assets.message, dialog, assets.message.size));
            for (images, place) in assets.shop_buttons().into_iter().zip(DEAL_BUTTONS) {
                shop_layer.sprites.push(button_sprite(pointer, offset(dialog, place), images));
            }
            shop_layer.texts.push((message(DEAL_MESSAGE).to_owned(), offset(dialog, MESSAGE_TEXT), TEXT_COLOR));
        }
        if let Some(room) = &game.room {
            room.draw(shop_layer.window(), &assets.chrome, font, messages, &game.me.name, pointer.0, caret);
        }
        if let Some(setup) = &game.vend_setup {
            setup.draw(shop_layer.window(), &assets.chrome, &assets.vend, icons, items, &game.cart.items, font, messages, pointer, caret);
        }
        if let Some(shop) = &game.my_shop {
            shop.draw(shop_layer.window(), &assets.chrome, &assets.vend, icons, items, font, messages, pointer);
        }
        if let Some(setup) = &game.buying_setup {
            setup.draw(shop_layer.window(), &assets.chrome, &assets.vend, icons, items, &game.inventory.items, font, messages, pointer, caret);
        }
        if let Some(store) = &game.my_buying_store {
            store.draw(shop_layer.window(), &assets.chrome, &assets.vend, icons, items, font, messages, pointer);
        }
        game.quest_window.draw(
            shop_layer.window(),
            &mut tooltip,
            &assets.chrome,
            &assets.quest,
            icons,
            items,
            font,
            messages,
            &game.quests,
            quest_texts,
            pointer,
        );
        game.achievement_window.draw(
            shop_layer.window(),
            &assets.chrome,
            &assets.achievement,
            icons,
            font,
            messages,
            achievement_info,
            &game.achievements,
            &game.achievement_totals,
            title_names,
            items,
            pointer,
        );
        // ponytail: the roulette's icon stands left of the minimap; where the real client puts it isn't known.
        roulette::draw_icon(shop_layer.window(), &mut tooltip, &assets.chrome, &assets.roulette, font, messages, pointer.0);
        if let Some(window) = &game.monster_window {
            window.draw(shop_layer.window(), &assets.chrome, font, messages, pointer.0);
        }
        if let Some(window) = &game.roulette {
            window.draw(shop_layer.window(), &mut tooltip, &assets.chrome, &assets.roulette, icons, items, font, messages, control::now(), pointer);
        }
        if let Some(list) = &game.emotion_list {
            list.draw(shop_layer.window(), &assets.chrome, font, messages, game.emotion_sprite.as_ref(), &emotion_icons, pointer.0);
        }
        if let Some(window) = &game.macro_window {
            window.draw(shop_layer.window(), &assets.chrome, font, messages, &game.macro_lines, pointer.0, caret);
        }
        game.mail.draw(shop_layer.window(), &mut tooltip, &assets.chrome, &assets.mail, icons, items, font, messages, pc_job_names, pointer, caret);
        if let Some(make) = &game.room_make {
            make.draw(shop_layer.window(), &assets.chrome, &assets.chatroom, messages, pointer, caret);
        }
        if let Some(bank) = &game.bank {
            let carried = game.status.get(&net::STATUS_ZENY).copied().unwrap_or(0).max(0) as u32;
            bank.draw(shop_layer.window(), &assets.chrome, &assets.bank, messages, font, pointer, caret, carried);
        }
        if let Some(shop) = &game.cashshop {
            shop.draw(shop_layer.window(), &assets.chrome, &assets.cashshop, icons, items, messages, font, pointer, caret);
        }
        if let Some(viewed) = &game.viewing {
            let window = &viewed.window;
            window.draw(
                shop_layer.window(),
                &mut tooltip,
                &assets.chrome,
                &assets.equipment,
                icons,
                items,
                messages,
                font,
                pointer.0,
                &viewed.items,
                viewed_preview,
                (card_names, &game.char_names),
                None,
            );
        }
        if let Some(window) = &game.refine {
            window.draw(shop_layer.window(), &assets.chrome, &assets.refine, icons, items, &game.inventory.items, messages, font, pointer);
        }
        if let Some(window) = &game.reform {
            window.draw(shop_layer.window(), &assets.chrome, &assets.reform, icons, items, &game.inventory.items, messages, font, pointer);
        }
        if let Some(window) = &game.grade {
            window.draw(shop_layer.window(), &assets.chrome, &assets.grade, icons, items, &game.inventory.items, messages, font, pointer);
        }
        if let Some(window) = &game.enchant {
            window.draw(shop_layer.window(), &assets.chrome, &assets.enchant, icons, items, &game.inventory.items, messages, font, pointer);
        }
        if let Some(window) = &game.stylist {
            let job = game.player.look.as_ref().map(|look| look.job).unwrap_or(game.me.job);
            window.draw(shop_layer.window(), &assets.chrome, &assets.stylist, items, &self.styling_shop, job, messages, font, pointer, &stylist_placements);
        }
        if let Some(window) = &game.materials {
            window.draw(shop_layer.window(), &assets.chrome, &assets.pick_list, icons, items, &game.inventory.items, messages, font, pointer);
        }
        if let Some(window) = &game.synthesis {
            window.draw(shop_layer.window(), &assets.chrome, &assets.laphine, icons, items, &game.inventory.items, messages, font, pointer);
        }
        if let Some(window) = &game.upgrade {
            window.draw(shop_layer.window(), &assets.chrome, &assets.laphine, icons, items, &game.inventory.items, messages, font, pointer);
        }
        if let Some(trade) = &game.trade {
            trade.draw(shop_layer.window(), &mut tooltip, &assets.chrome, &assets.trade, icons, items, font, pointer, caret);
        }
        // Someone asks to join through our booking post: who, their level and job.
        // ponytail: the question's words are plain English; the client's own aren't identified.
        if let Some((_, _, name, level, job)) = &game.booking_asked {
            let dialog = dialog_position(&assets.message);
            shop_layer.window().sprites.push((&assets.message, dialog, assets.message.size));
            shop_layer.sprites.push(button_sprite(pointer, offset(dialog, LEFT_BUTTON), &assets.ok));
            shop_layer.sprites.push(button_sprite(pointer, offset(dialog, RIGHT_BUTTON), &assets.cancel));
            let job_name = self.pc_job_names.get(job).cloned().unwrap_or_default();
            let lines = [format!("{name} asks to join your party."), format!("Lv.{level} {job_name}")];
            for (row, line) in lines.into_iter().enumerate() {
                shop_layer.texts.push((line, offset(dialog, [MESSAGE_TEXT[0], MESSAGE_TEXT[1] + row as f32 * font.line_height()]), TEXT_COLOR));
            }
        }
        // Someone asks for a deal: who, their C-Code and level, OK or cancel.
        if let Some((name, id, level)) = &game.trade_asked {
            let dialog = dialog_position(&assets.message);
            shop_layer.window().sprites.push((&assets.message, dialog, assets.message.size));
            shop_layer.sprites.push(button_sprite(pointer, offset(dialog, LEFT_BUTTON), &assets.ok));
            shop_layer.sprites.push(button_sprite(pointer, offset(dialog, RIGHT_BUTTON), &assets.cancel));
            let lines = [format!("({name}){}", message(TRADE_ASKS)), format!("C-Code : {}     Lv.{level}", trade::c_code(*id))];
            for (row, line) in lines.into_iter().enumerate() {
                shop_layer.texts.push((line, offset(dialog, [MESSAGE_TEXT[0], MESSAGE_TEXT[1] + row as f32 * font.line_height()]), TEXT_COLOR));
            }
        }
        if game.cashshop_buy_asked
            && let Some(shop) = &game.cashshop
        {
            let dialog = dialog_position(&assets.message);
            shop_layer.window().sprites.push((&assets.message, dialog, assets.message.size));
            shop_layer.sprites.push(button_sprite(pointer, offset(dialog, LEFT_BUTTON), &assets.ok));
            shop_layer.sprites.push(button_sprite(pointer, offset(dialog, RIGHT_BUTTON), &assets.cancel));
            let cash = shop.cart_total().saturating_sub(shop.free_to_spend);
            let question = messages::fill(message(1312), &[&cash, &shop.free_to_spend]);
            for (row, line) in message_lines(font, &question, MESSAGE_WIDTH).into_iter().enumerate() {
                shop_layer.texts.push((line, offset(dialog, [MESSAGE_TEXT[0], MESSAGE_TEXT[1] + row as f32 * font.line_height()]), TEXT_COLOR));
            }
        }
        // A guild invites us, asks for an alliance, or the player is about to leave.
        let guild_question = match (&game.guild_invite, &game.guild_asked) {
            (Some((_, name)), _) => Some(format!("({name}){}", message(GUILD_INVITED))),
            (_, Some(GuildAsked::Ally { guild, .. })) => Some(format!("({guild}){}", message(ALLY_ASKS))),
            _ => None,
        };
        if let Some(question) = guild_question {
            // The invitation sits where the party's does.
            let dialog = if game.guild_invite.is_some() { party::invitation_at(screen()) } else { dialog_position(&assets.message) };
            shop_layer.window().sprites.push((&assets.message, dialog, assets.message.size));
            shop_layer.sprites.push(button_sprite(pointer, offset(dialog, LEFT_BUTTON), &assets.ok));
            shop_layer.sprites.push(button_sprite(pointer, offset(dialog, RIGHT_BUTTON), &assets.cancel));
            for (row, line) in message_lines(font, &question, MESSAGE_WIDTH).into_iter().enumerate() {
                shop_layer.texts.push((line, offset(dialog, [MESSAGE_TEXT[0], MESSAGE_TEXT[1] + row as f32 * font.line_height()]), TEXT_COLOR));
            }
        }
        // Growing the inventory (which voucher, from how many slots to how many) or a pet question, in the message's
        // colors.
        let colored_question = match (game.expansion_asked, &game.pet_asked) {
            _ if game.dismiss_asked => Some(message(mercenary::DISMISS_QUESTION).to_owned()),
            _ if game.homunculus_asked.is_some() => Some(match game.homunculus_asked.as_ref().unwrap() {
                HomunculusAsked::Feed => message(homunculus::FEED_ASKS).to_owned(),
                HomunculusAsked::Delete => message(homunculus::DELETE_ASKS).to_owned(),
                HomunculusAsked::Rename(name) => messages::fill(message(homunculus::RENAME_ASKS), &[name]),
                HomunculusAsked::Refused(why) => message(*why).to_owned(),
            }),
            _ if game.leader_asked.is_some() => Some(message(LEADER_QUESTION).to_owned()),
            _ if game.bind_asked.is_some() => Some(message(BIND_ASKS).to_owned()),
            (Some(voucher), _) => {
                let now = inventory::BASE_SLOTS + usize::from(game.inventory.expansion);
                let name = inventory::item_name(items, voucher, true, 0);
                Some(messages::fill(message(EXPANSION_ASKS), &[&name, &now, &(now + VOUCHER_SLOTS)]))
            }
            (None, Some(PetAsked::Feed)) => Some(message(PET_FEED_ASKS).to_owned()),
            (None, Some(PetAsked::Rename(name))) => Some(messages::fill(message(PET_RENAME_ASKS), &[name])),
            (None, Some(PetAsked::Refused(why))) => Some(message(*why).to_owned()),
            (None, None) => None,
        };
        if let Some(question) = colored_question {
            let dialog = dialog_position(&assets.message);
            shop_layer.window().sprites.push((&assets.message, dialog, assets.message.size));
            shop_layer.sprites.push(button_sprite(pointer, offset(dialog, LEFT_BUTTON), &assets.ok));
            shop_layer.sprites.push(button_sprite(pointer, offset(dialog, RIGHT_BUTTON), &assets.cancel));
            for (row, pieces) in wrap_colored(font, &question, MESSAGE_WIDTH).into_iter().enumerate() {
                let mut x = dialog[0] + MESSAGE_TEXT[0];
                for (color, piece) in pieces {
                    let width = font.width(&piece);
                    // The message's black is the box's text color.
                    let color = if color == [0, 0, 0] { TEXT_COLOR } else { color };
                    shop_layer.texts.push((piece, [x, dialog[1] + MESSAGE_TEXT[1] + row as f32 * font.line_height()], color));
                    x += width;
                }
            }
        }
        // Someone asks to be friends.
        if let Some((_, _, name)) = &game.friend_asked {
            let dialog = dialog_position(&assets.message);
            shop_layer.window().sprites.push((&assets.message, dialog, assets.message.size));
            shop_layer.sprites.push(button_sprite(pointer, offset(dialog, LEFT_BUTTON), &assets.ok));
            shop_layer.sprites.push(button_sprite(pointer, offset(dialog, RIGHT_BUTTON), &assets.cancel));
            let question = messages::fill(message(FRIEND_ASKS), &[name]);
            for (row, line) in message_lines(font, &question, MESSAGE_WIDTH).into_iter().enumerate() {
                shop_layer.texts.push((line, offset(dialog, [MESSAGE_TEXT[0], MESSAGE_TEXT[1] + row as f32 * font.line_height()]), TEXT_COLOR));
            }
        }
        if game.memorial.frame.open {
            game.memorial.draw(shop_layer.window(), &assets.chrome, font, messages, pointer.0, memorial::unix_now());
        }
        // The battleground queue, and its question: applying, leaving, or entering with the time left.
        if game.queue_window.frame.open {
            game.queue_window.draw(
                shop_layer.window(),
                &assets.chrome,
                &assets.battle_queue,
                font,
                messages,
                &self.battlegrounds,
                game.queued.as_ref(),
                pointer,
            );
        }
        let queue_question = match &game.queue_asked {
            Some(QueueAsked::Apply(kind, _)) => {
                let index = [net::QUEUE_ALONE, net::QUEUE_PARTY, net::QUEUE_GUILD].iter().position(|each| each == kind).unwrap_or(0);
                Some(message(battle_queue::APPLY_ASKS[index]).to_owned())
            }
            Some(QueueAsked::Cancel(_)) => Some(message(battle_queue::CANCEL_ASKS).to_owned()),
            Some(QueueAsked::Lobby(_, until)) => {
                let left = until.saturating_duration_since(Instant::now()).as_secs();
                let note = battle_queue::LOBBY_NOTE.iter().map(|&id| message(id)).collect::<Vec<_>>().join(" ");
                Some(format!("{} {note} {}", message(battle_queue::LOBBY_ASKS), messages::fill(message(battle_queue::LOBBY_TIME), &[&left])))
            }
            None => None,
        };
        if let Some(question) = queue_question {
            let dialog = dialog_position(&assets.message);
            shop_layer.window().sprites.push((&assets.message, dialog, assets.message.size));
            shop_layer.sprites.push(button_sprite(pointer, offset(dialog, LEFT_BUTTON), &assets.ok));
            shop_layer.sprites.push(button_sprite(pointer, offset(dialog, RIGHT_BUTTON), &assets.cancel));
            for (row, line) in message_lines(font, &question, MESSAGE_WIDTH).into_iter().enumerate() {
                shop_layer.texts.push((line, offset(dialog, [MESSAGE_TEXT[0], MESSAGE_TEXT[1] + row as f32 * font.line_height()]), TEXT_COLOR));
            }
        }
        // Asked to be adopted: who asks, then whether they're sure (what a baby class gives up is in the chat).
        // ponytail: the second question (888) is an estimate, and so is splitting it: the box holds three lines, so it
        // shows the last sentence.
        if let Some((_, _, name, again)) = &game.adopt_asked {
            let question = if *again { last_sentence(message(ADOPT_ASKS_AGAIN)).to_owned() } else { format!("({name}){}", message(ADOPT_ASKS)) };
            let dialog = dialog_position(&assets.message);
            shop_layer.window().sprites.push((&assets.message, dialog, assets.message.size));
            shop_layer.sprites.push(button_sprite(pointer, offset(dialog, LEFT_BUTTON), &assets.ok));
            shop_layer.sprites.push(button_sprite(pointer, offset(dialog, RIGHT_BUTTON), &assets.cancel));
            for (row, line) in message_lines(font, &question, MESSAGE_WIDTH).into_iter().enumerate() {
                shop_layer.texts.push((line, offset(dialog, [MESSAGE_TEXT[0], MESSAGE_TEXT[1] + row as f32 * font.line_height()]), TEXT_COLOR));
            }
        }
        // The Guild Companion and its windows.
        {
            let caret = (now.saturating_duration_since(game.entered).as_millis() / CARET_BLINK.as_millis()).is_multiple_of(2);
            game.guild_companion.draw(shop_layer.window(), &assets.chrome, &assets.guild_companion, font, messages, caret, pointer);
        }
        // The guild's reason box.
        if let Some(reason) = &game.reason_box {
            let caret = (now.saturating_duration_since(game.entered).as_millis() / CARET_BLINK.as_millis()).is_multiple_of(2);
            reason.draw(shop_layer.window(), &assets.chrome, &assets.guild, (&assets.ok, &assets.cancel), font, messages, screen(), pointer, caret);
        }
        // The message box: its words, OK and Cancel (OK alone when it only tells).
        if let Some(asked) = &game.question_box {
            let dialog = dialog_position(&assets.message);
            shop_layer.window().sprites.push((&assets.message, dialog, assets.message.size));
            for (place, yes) in box_buttons(asked.asks()) {
                shop_layer.sprites.push(button_sprite(pointer, offset(dialog, place), if yes { &assets.ok } else { &assets.cancel }));
            }
            for (row, line) in message_lines(font, &asked.text(messages), MESSAGE_WIDTH).into_iter().enumerate() {
                shop_layer.texts.push((line, offset(dialog, [MESSAGE_TEXT[0], MESSAGE_TEXT[1] + row as f32 * font.line_height()]), TEXT_COLOR));
            }
        }
        // A party invites us: its name and the question, OK or cancel.
        if let Some((_, name)) = &game.party_invite {
            let dialog = party::invitation_at(screen());
            shop_layer.window().sprites.push((&assets.message, dialog, assets.message.size));
            shop_layer.sprites.push(button_sprite(pointer, offset(dialog, LEFT_BUTTON), &assets.ok));
            shop_layer.sprites.push(button_sprite(pointer, offset(dialog, RIGHT_BUTTON), &assets.cancel));
            let question = party::invitation_text(name, message(PARTY_INVITED));
            for (row, line) in message_lines(font, &question, MESSAGE_WIDTH).into_iter().enumerate() {
                shop_layer.texts.push((line, offset(dialog, [MESSAGE_TEXT[0], MESSAGE_TEXT[1] + row as f32 * font.line_height()]), TEXT_COLOR));
            }
        }
        if let Some(amount) = &game.amount {
            amount.draw(shop_layer.window(), &assets.chrome, &assets.message, [&assets.ok, &assets.cancel], messages, pointer);
        }
        if let Some(window) = &mut game.item_window {
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |time| time.as_secs());
            let lines = window.lines(items, messages, now);
            window.fit(lines.len());
            game.item_lines = lines.len();
            let names = (card_names, &messages[..], &game.char_names);
            window.draw(shop_layer.window(), &mut tooltip, &assets.chrome, &assets.item_window, icons, items, names, font, &lines, pointer.0);
        }
        let unix_now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |time| time.as_secs());
        if let Some(mut window) = compared {
            let lines = window.lines(items, messages, unix_now);
            window.fit(lines.len());
            let names = (card_names, &messages[..], &game.char_names);
            window.draw(shop_layer.window(), &mut tooltip, &assets.chrome, &assets.item_window, icons, items, names, font, &lines, pointer.0);
        }
        if let Some(window) = &mut game.card_window {
            let lines = window.lines(items, messages, unix_now);
            window.fit(lines.len());
            game.card_lines = lines.len();
            let names = (card_names, &messages[..], &game.char_names);
            window.draw(shop_layer.window(), &mut tooltip, &assets.chrome, &assets.item_window, icons, items, names, font, &lines, pointer.0);
        }
        if let Some(view) = &game.card_view {
            view.draw(shop_layer.window(), &assets.chrome, &assets.item_window, icons, pointer.0);
        }
        if game.apply_asked {
            let dialog = dialog_position(&assets.message);
            let question = messages.get(skills::APPLY_QUESTION).map_or("", String::as_str);
            shop_layer.window().sprites.push((&assets.message, dialog, assets.message.size));
            shop_layer.sprites.push(button_sprite(pointer, offset(dialog, LEFT_BUTTON), &assets.ok));
            shop_layer.sprites.push(button_sprite(pointer, offset(dialog, RIGHT_BUTTON), &assets.cancel));
            for (row, line) in message_lines(font, question, MESSAGE_WIDTH).into_iter().enumerate() {
                shop_layer.texts.push((line, offset(dialog, [MESSAGE_TEXT[0], MESSAGE_TEXT[1] + row as f32 * font.line_height()]), TEXT_COLOR));
            }
        }
        if game.memorial_asked {
            let dialog = dialog_position(&assets.message);
            // ponytail: no msgstringtable id was given for this question, so the Destroy button's label asks it.
            let question = messages.get(memorial::DESTROY_ASK).map_or("Destroy instance", String::as_str);
            shop_layer.window().sprites.push((&assets.message, dialog, assets.message.size));
            shop_layer.sprites.push(button_sprite(pointer, offset(dialog, LEFT_BUTTON), &assets.ok));
            shop_layer.sprites.push(button_sprite(pointer, offset(dialog, RIGHT_BUTTON), &assets.cancel));
            for (row, line) in message_lines(font, question, MESSAGE_WIDTH).into_iter().enumerate() {
                shop_layer.texts.push((line, offset(dialog, [MESSAGE_TEXT[0], MESSAGE_TEXT[1] + row as f32 * font.line_height()]), TEXT_COLOR));
            }
        }
        if game.save_asked {
            let dialog = dialog_position(&assets.message);
            let question = messages.get(escape::SAVE_QUESTION).map_or("Do you really want to go back to your savepoint?", String::as_str);
            shop_layer.window().sprites.push((&assets.message, dialog, assets.message.size));
            shop_layer.sprites.push(button_sprite(pointer, offset(dialog, LEFT_BUTTON), &assets.ok));
            shop_layer.sprites.push(button_sprite(pointer, offset(dialog, RIGHT_BUTTON), &assets.cancel));
            for (row, line) in message_lines(font, question, MESSAGE_WIDTH).into_iter().enumerate() {
                shop_layer.texts.push((line, offset(dialog, [MESSAGE_TEXT[0], MESSAGE_TEXT[1] + row as f32 * font.line_height()]), TEXT_COLOR));
            }
        }
        if let Some(menu) = &game.escape {
            menu.draw(shop_layer.window(), &assets.chrome, &assets.escape, messages, game.me.id, pointer, game.can_resurrect());
        }
        // The party's docked window, half see-through while the party window is dragged.
        if let Some(dialog) = &game.party_dialog {
            let mut docked = Layer { faded: game.party_window.frame.faded(), ..Layer::default() };
            dialog.draw(&mut docked, &assets.chrome, &assets.party, (&assets.ok, &assets.cancel), font, messages, caret, pointer);
            layers.push(docked);
        }
        game.talks.draw(shop_layer.window(), &assets.chrome, font, messages, pointer);
        if let Some(setup) = &game.friend_setup {
            setup.draw(shop_layer.window(), &assets.chrome, messages, pointer.0);
        }
        if let Some(list) = &game.block_list {
            list.draw(shop_layer.window(), &assets.chrome, pointer.0);
        }
        if let Some(prompt) = &game.away_prompt {
            prompt.draw(shop_layer.window(), &assets.chrome, messages, pointer);
        }
        if let Some(board) = &game.ranking {
            board.draw(shop_layer.window(), &assets.chrome, font, messages, &game.char_names, pointer.0);
        }
        if let Some(window) = &game.booking {
            let caret = (game.entered.elapsed().as_millis() / CARET_BLINK.as_millis()).is_multiple_of(2);
            window.draw(shop_layer.window(), &assets.chrome, font, pointer.0, caret);
        }
        if let Some(window) = &game.navi_window {
            let caret = (game.entered.elapsed().as_millis() / CARET_BLINK.as_millis()).is_multiple_of(2);
            window.draw(shop_layer.window(), &assets.chrome, font, messages, game.navigation.as_ref().map(|active| &active.target), pointer.0, caret);
        }
        if let Some(window) = &game.reputation_window {
            window.draw(shop_layer.window(), &assets.chrome, &assets.reputation, font, messages, &self.reputation_tables, &game.reputation, pointer.0);
        }
        if let Some(room) = &game.dressroom {
            let feet = room.feet();
            let figure = match &game.dress_figure {
                Some(sprites) => dress_placements.iter().map(|(key, position, size, _)| (sprites.texture(*key), offset(feet, *position), *size)).collect(),
                None => Vec::new(),
            };
            room.draw(shop_layer.window(), &assets.chrome, &assets.dressroom, font, messages, &game.dress_lists, figure, pointer);
        }
        if let Some(window) = &game.audio_window {
            window.draw(shop_layer.window(), &assets.chrome, &assets.options, font, messages, sound, pointer.0);
        }
        if let Some(window) = &game.graphics_window {
            window.draw(shop_layer.window(), &assets.chrome, &assets.options, font, messages, &self.graphics, pointer.0);
        }
        if let Some(window) = &game.shortcuts_window {
            window.draw(shop_layer.window(), &assets.chrome, font, messages, &self.shortcuts, pointer.0);
        }
        let mut menu_layer = Layer::default();
        if let Some(menu) = &game.menu {
            menu.draw(&mut menu_layer, &assets.context_menu, font, pointer.0);
        }
        let mut world_layer = Layer::default();
        if let Some(world_map) = &game.world_map {
            world_map.draw(&mut world_layer, &assets.chrome, font, &worlds.0, map_names, &map_key(&game.map), pointer.0);
        }
        game.dialog_links = dialog_links;
        let mut minimap_layer = Layer::default();
        let mut minimap_tooltip = Layer::default();
        {
            minimap_layer.faded = game.minimap_window.frame.faded();
            let map = map_key(&game.map);
            let title = minimap_window::title(map_names.get(&map).map_or(map.as_str(), String::as_str), &map, game.player.cell);
            let naming_caret = (now.saturating_duration_since(game.entered).as_millis() / CARET_BLINK.as_millis()).is_multiple_of(2);
            let party_cells: Vec<[i32; 2]> = dots.iter().map(|(cell, _)| *cell).collect();
            let quests = game.big_minimap_quests();
            let pins = minimap_window::Pins { player: game.player.cell, party: &party_cells, guild: &guild, boss, quests: &quests };
            game.minimap_window.draw(&mut minimap_layer, &mut minimap_tooltip, &assets.chrome, font, messages, &title, size, &pins, pointer, naming_caret);
        }
        layers.extend([catalog_layer, shop_layer, minimap_layer, overlay, menu_layer, tooltip, minimap_tooltip, world_layer]);
        let composed = compose(gpu, font, layers);
        let (mut sprites, alphas) = flatten(&composed);

        // The cursor shows what a click would do.
        let on_window = game.on_ui(pointer.0, assets.message.size);
        let ground = view.pick_ground([pointer.0[0].floor() / screen()[0], pointer.0[1].floor() / screen()[1]], &game.world.gat);
        let (shift, alt) = (self.modifiers.shift_key(), self.modifiers.alt_key());
        let look = if turning_cursor(game.camera_goal.dragging.is_some(), shift, alt) {
            cursor::Kind::Rotate
        } else if pointer.1 && game.attack.is_some_and(|attack| attack.held) {
            cursor::Kind::Attack
        } else if (game.targeting.is_some() || game.capturing) && !on_window {
            if game.aim_valid(pointer.0, self.modifiers.shift_key()) { cursor::Kind::TargetValid } else { cursor::Kind::Target }
        } else if game.link_at(pointer.0).is_some() {
            cursor::Kind::Clickable
        } else if on_window {
            if game.on_control(pointer.0) { cursor::Kind::Clickable } else { cursor::Kind::Default }
        } else if game.companion_mark.is_some_and(|(_, target)| hovered(&game.hit_boxes, pointer.0, false) == Some(Owner::Unit(target))) {
            cursor::Kind::TargetValid
        } else {
            match hovered(&game.hit_boxes, pointer.0, players_first) {
                Some(Owner::Unit(id)) => match game.units.get(&id).map(|actor| actor.kind) {
                    Some(MONSTER) => monster_cursor(shift, game.no_shift),
                    Some(NPC) => npc_cursor(game.units[&id].job),
                    Some(PLAYER) if game.opponent(id).is_some_and(|other| game.side().attack_cursor(&other, players_first, false)) => cursor::Kind::Attack,
                    Some(HOMUNCULUS | MERCENARY | ELEMENTAL) if versus::companion_cursor(game.side().flags) => cursor::Kind::Attack,
                    _ => cursor::Kind::Default,
                },
                Some(Owner::Item(_)) => cursor::Kind::Pick,
                Some(Owner::SkillUnit(_)) => cursor::Kind::Attack,
                // Past the map's edge counts as somewhere that can't be walked.
                _ => match ground {
                    Some(cell) if game.world.gat.walkable(cell) => cursor::Kind::Default,
                    _ => cursor::Kind::NoWalk,
                },
            }
        };
        cursor_look.set(look);
        // The ground cursor follows the pointer while a click would walk there: the walkable cell under it, nothing
        // over one that can't be walked. Over a window or a unit, or with Ctrl held, it stays where it was.
        if game.ground_cursor
            && !on_window
            && !self.modifiers.control_key()
            && matches!(look, cursor::Kind::Default | cursor::Kind::NoWalk | cursor::Kind::Warp)
        {
            game.ground_mark = ground.filter(|&cell| game.world.gat.walkable(cell));
        }
        for (font, frames) in [(&game.rank_font, &counter), (&game.time_font, &clock), (&game.time_attack, &banner)] {
            if let Some(Some(font)) = font {
                for (placed, anchor) in frames {
                    sprites.extend(placed.iter().map(|(key, at, size, _)| (font.texture(*key), [anchor[0] + at[0], anchor[1] + at[1]], *size)));
                }
            }
        }
        // ponytail: over everything, as the slot machine.
        if let Some((_, Some(sprite))) = &game.quest_npc {
            sprites.extend(quest_npc_frame.into_iter().map(|((key, at, size, _), feet)| (sprite.texture(key), [feet[0] + at[0], feet[1] + at[1]], size)));
        }
        if let Some(sprite) = &game.slot_sprite {
            let middle = [screen()[0] / 2.0, screen()[1] / 2.0];
            sprites.extend(slot_frame.into_iter().map(|(key, offset, size, _)| (sprite.texture(key), [middle[0] + offset[0], middle[1] + offset[1]], size)));
        }
        // The cursor drawn at the middle of what it snaps to (clicks still go by the pointer).
        let aiming_unit = game.targeting.is_some_and(|(id, _)| {
            game.skills.iter().chain(&game.item_skill).find(|skill| skill.id == id).is_none_or(|skill| skill.kind & net::Skill::GROUND == 0)
        });
        let picked =
            if game.targeting.is_some() { hovered_unit(&game.hit_boxes, pointer.0, players_first) } else { hovered(&game.hit_boxes, pointer.0, players_first) };
        let under = picked.filter(|_| !on_window).and_then(|owner| {
            let area = game.hit_boxes.iter().find(|target| target.owner == owner)?.area;
            let kind = match owner {
                Owner::Item(_) => SnapTo::Item,
                Owner::Unit(id) => match game.units.get(&id).map(|actor| actor.kind) {
                    Some(MONSTER) => SnapTo::Monster,
                    Some(HOMUNCULUS) if game.homunculus_id == Some(id) => SnapTo::OurHomunculus,
                    _ => SnapTo::Other,
                },
                // Ice Wall and traps are attacked like monsters.
                Owner::SkillUnit(_) => SnapTo::Monster,
                Owner::Player => SnapTo::Other,
            };
            Some((kind, area))
        });
        let pitcher = game.targeting.is_some_and(|(id, _)| id == POTION_PITCHER);
        game.cursor_at = snap_point(under, aiming_unit, pitcher, game.snaps).unwrap_or(pointer.0);
        cursor_look.press(pointer.1);
        sprites.extend(cursor_look.draw(gpu, game.cursor_at));

        game.effects.advance(gpu, now, yaw);
        // With /effect off, effects still play on, unseen, and their sounds are still heard.
        let effects_on = self.graphics.effects_on;
        if let Some(audio) = audio {
            for (file, at) in game.effects.due_sounds(now) {
                audio.play(grf, &file, heard(f32::hypot(at[0] - player_feet[0], at[2] - player_feet[2]) / world::CELL));
            }
        }
        let feet_of_unit = |unit: Option<u32>| match unit {
            None => Some(player_feet),
            Some(id) => feet_of.get(&id).copied(),
        };
        let (mut effect_quads, mut effect_meshes) =
            if effects_on { (game.effects.quads(now, feet_of_unit), game.effects.meshes(now, feet_of_unit)) } else { (Vec::new(), Vec::new()) };
        let screen_shapes = if effects_on { game.effects.screen_shapes(now, feet_of_unit) } else { Vec::new() };
        // Max-level auras under the feet of those in sight, with the effects (unless /aura2 has them off).
        if effects_on && !self.graphics.aura_off {
            let seconds = now.duration_since(game.entered).as_secs_f32();
            let own_level = game.status.get(&net::STATUS_BASE_LEVEL).copied().unwrap_or(0).clamp(0, i64::from(u16::MAX)) as u16;
            let own_job = game.player.look.as_ref().map_or(game.me.job, |look| look.job);
            let player = (!game.player.cloaked).then_some((game.account_id, own_job, own_level, player_feet));
            let others = game.units.iter().filter(|(_, actor)| !actor.cloaked && actor.remove_at.is_none()).filter_map(|(&id, actor)| {
                let feet = *feet_of.get(&id)?;
                let near = f32::hypot(feet[0] - player_feet[0], feet[2] - player_feet[2]) <= aura::SEEN_CELLS * world::CELL;
                near.then_some((id, actor.job, actor.level, feet))
            });
            for (id, job, level, feet) in player.into_iter().chain(others) {
                if let Some(kind) = aura::kind(job, level) {
                    aura::draw(&assets.aura, kind, feet, seconds, id, self.graphics.aura, &mut effect_meshes, &mut effect_quads);
                }
            }
        }
        // Magic squares under ground casts, for as long as their casts last; flat on the ground, a little over it.
        game.cast_squares.retain(|caster, square| square.alive(now) && game.casts.contains_key(caster));
        if let Some(texture) = &assets.magic_target {
            for square in game.cast_squares.values() {
                let steps = square.width.ceil() as usize;
                let vertices = square
                    .triangles(now, steps)
                    .into_iter()
                    .map(|(cell, uv)| {
                        let ground = game.world.cell_position(cell);
                        world::MeshVertex {
                            position: [cell[0] * world::CELL + world::CELL / 2.0, ground[1] + CAST_SQUARE_LIFT, cell[1] * world::CELL + world::CELL / 2.0],
                            uv,
                            tint: [1.0; 4],
                        }
                    })
                    .collect();
                effect_meshes.push((texture, vertices, true));
            }
        }
        // The ground cursor: its square over the cell, at the corner heights of the ground tile it's part of.
        if let (Some(texture), Some(corners)) = (&assets.ground_grid, game.ground_mark.and_then(|cell| game.world.cell_quad(cell))) {
            let vertex = |corner: usize, uv| {
                let [x, y, z] = corners[corner];
                world::MeshVertex { position: [x, y + CAST_SQUARE_LIFT, z], uv, tint: GROUND_CURSOR_TINT }
            };
            let [a, b, c, d] = [vertex(0, [0.0, 1.0]), vertex(1, [1.0, 1.0]), vertex(2, [0.0, 0.0]), vertex(3, [1.0, 0.0])];
            effect_meshes.push((texture, vec![a, b, d, a, d, c], false));
        }
        let frame = WorldFrame {
            world: &game.world,
            view: &view,
            billboards: &billboards,
            shadows,
            effects: &effect_quads,
            meshes: &effect_meshes,
            screen: &screen_shapes,
            time: now.duration_since(game.entered).as_secs_f32(),
            blind: game.player.health_state & 0x10 != 0,
        };
        if let Some(path) = self.print_screen.take() {
            let _ = path.parent().map(std::fs::create_dir_all);
            let shot = image::DynamicImage::ImageRgba8(gpu.snapshot(Some(&frame), &sprites, &alphas)).to_rgb8();
            if shot.save(&path).is_ok() {
                self.shot_saved = path.file_name().map(|name| name.to_string_lossy().into_owned());
            }
        }
        let shot_delay = std::env::var("RUSTNAROK_CLIENT_SHOT_DELAY").ok().and_then(|seconds| seconds.parse().ok()).unwrap_or(2.0);
        match &self.screenshot {
            Some(path) if game.entered.elapsed().as_secs_f32() > shot_delay => {
                gpu.capture(Some(&frame), &sprites, &alphas, path);
                event_loop.exit();
            }
            _ => gpu.draw(Some(&frame), &sprites, &alphas),
        }
    }
}
