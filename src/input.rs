//! The app's input: mouse buttons, the wheel, the pointer, keys and shortcuts, on every scene.

use super::*;

impl App {
    pub(crate) fn click(&mut self, _event_loop: &ActiveEventLoop) {
        let Some(assets) = &self.assets else { return };
        let cursor = self.cursor;
        let shift = self.modifiers.shift_key();
        let hit = |position: [f32; 2], images: &ButtonImages| contains(position, images[0].size, cursor);
        // A click anywhere but the storage's search field takes the caret from it.
        if let Scene::Game(game) = &mut self.scene {
            game.storage.typing = false;
            game.pet_window.typing = false;
            game.homunculus_window.typing = false;
            game.quest_window.typing = false;
            if let Some(trade) = &mut game.trade {
                trade.typing = false;
            }
            if let Some(bank) = &mut game.bank {
                bank.typing = false;
            }
            // A double click on a speaker's line in the log: whisper them (the click still reaches the world).
            if let Some(name) = game.chat.log_click(cursor) {
                game.chat.whisper_to(&name);
            }
        }
        match &mut self.scene {
            Scene::Login => {
                let local = offset(cursor, login_position().map(|value| -value));
                let part = login_part(local, self.account_list);
                self.account_list = false;
                match part {
                    Some(LoginPart::Start) => self.submit_login(),
                    // The close X keeps the Save ID choice, then asks before quitting.
                    Some(LoginPart::Exit) => {
                        if std::env::var_os("RUSTNAROK_CLIENT_LOGIN").is_none() {
                            remember_id(self.save_id.then_some(self.fields[0].as_str()));
                        }
                        self.ask(QUIT_MESSAGE, Then::Quit);
                    }
                    Some(LoginPart::Save) => self.save_id = !self.save_id,
                    Some(LoginPart::Drop) => self.account_list = true,
                    Some(LoginPart::Account(row)) => self.account_kind = row,
                    Some(LoginPart::Field(field)) => {
                        self.focus = field;
                        self.blink_start = Instant::now();
                    }
                    // ponytail: a rathena server has no page for Opening or Sign Up to open.
                    Some(LoginPart::Opening | LoginPart::SignUp) | None => {}
                }
            }
            // The server list: pick one, OK goes on to the login window, Cancel leaves the game.
            Scene::Connections { selected } => {
                let service = service_position(&assets.service);
                let list = offset(service, SERVER_LIST);
                if let Some(row) = (0..self.connections.len()).find(|&row| contains(offset(list, [0.0, row as f32 * SERVER_ROW[1]]), SERVER_ROW, cursor)) {
                    *selected = row;
                } else if hit(offset(service, LEFT_BUTTON), &assets.ok) {
                    let picked = *selected;
                    self.choose_connection(picked);
                } else if hit(offset(service, RIGHT_BUTTON), &assets.cancel) {
                    self.quitting = true;
                }
            }
            Scene::ServerSelect { accepted, selected, scroll } => {
                let place = servers::place(screen());
                let list = offset(place, servers::LIST);
                let local = [cursor[0] - place[0], cursor[1] - place[1]];
                let frame = window::Frame::new(place, servers::WIDTH);
                // ponytail: the shared title bar's close does what Cancel does; minimize does nothing on this step.
                if contains([0.0, 0.0], [servers::WIDTH, window::TITLE_HEIGHT], local) {
                    if frame.title_hit(local) == Some(window::TitleHit::Close) {
                        self.scene = Scene::Login;
                    }
                } else if let Some(row) = servers::shown(*scroll, accepted.char_servers.len())
                    .find(|&row| contains(offset(list, [0.0, (row - *scroll) as f32 * servers::ROW]), [servers::LIST_SIZE[0], servers::ROW], cursor))
                {
                    let enter = self.server_clicks.double(row as u16);
                    *selected = row;
                    if enter {
                        self.confirm_server();
                    }
                } else if hit(offset(place, servers::OK), &assets.ok) {
                    self.confirm_server();
                } else if hit(offset(place, servers::CANCEL), &assets.cancel) {
                    self.scene = Scene::Login;
                }
            }
            Scene::CharSelect(select) => {
                if let Some(pad) = &mut select.pin_pad {
                    if let Some(button) = pad.hit(cursor) {
                        let action = pad.press(button);
                        self.apply_pin_action(action);
                    }
                } else if select.unlocked() {
                    let action = if select.menu.is_some() { select.click_menu(cursor) } else { select.hit(cursor).and_then(|hit| select.click(hit)) };
                    if let Some(action) = action {
                        self.select_action(action);
                    }
                } else if select.hit(cursor) == Some(char_select::Hit::Close) {
                    self.select_action(char_select::Action::Close);
                }
            }
            Scene::CharMake(make, _) => {
                if let Some(action) = make.hit(cursor).and_then(|hit| make.click(hit, &self.grf, &self.look_tables)) {
                    self.make_action(action);
                }
            }
            // A link in the chat log or the NPC window: an item's information, or a navigation to the place.
            Scene::Game(game) if game.link_at(cursor).is_some() => match game.link_at(cursor).cloned().unwrap() {
                links::Target::Item(item) => {
                    let card = self.card_pictures.name(item.id).is_some();
                    game.show_item(iteminfo::Shown::of_link(item.id, item.refine, item.cards, card, &self.items));
                }
                links::Target::Navi { map, cell } => {
                    let map = navigation::map_key(&map);
                    let name = self.navi_tables.maps.get(&map).cloned().unwrap_or_else(|| map.clone());
                    let target = navigation::Target { cell: (cell != [0, 0]).then_some(cell), monster: None, name, map };
                    game.navigation = Some(navigation::Active::new(target, NAVI_SERVICES));
                }
                links::Target::Url(address) => {
                    if let Some(page) = links::page(&address) {
                        open_page(game, page);
                    }
                }
                // The quest window at that quest, or a notice that no list has it.
                links::Target::Quest(id) => {
                    if !game.quest_window.show_quest(id, &game.quests, &self.quest_texts) {
                        game.question_box = Some(BoxQuestion::Said(format!("Not found Quest Info = {id}")));
                    }
                }
                links::Target::Tip => {}
            },
            Scene::Game(game) if game.deal_choice.is_some() => {
                let npc = game.deal_choice.unwrap();
                let dialog = deal_box_position();
                let pressed = DEAL_BUTTONS.iter().position(|&place| hit(offset(dialog, place), assets.shop_buttons()[0]));
                match pressed {
                    Some(choice @ 0..=1) => {
                        game.deal_choice = None;
                        let _ = game.session.choose_deal(npc, choice == 1);
                    }
                    Some(_) => game.deal_choice = None,
                    None => {}
                }
            }
            // The world map covers the screen; its close button closes it.
            Scene::Game(game) if game.world_map.is_some() => {
                if game.world_map.as_ref().unwrap().closes(cursor) {
                    game.world_map = None;
                }
            }
            // The skill window's Apply asks first: OK sends the pending levels; Cancel keeps them pending.
            Scene::Game(game) if game.apply_asked && contains(dialog_position(&assets.message), assets.message.size, cursor) => {
                let dialog = dialog_position(&assets.message);
                for (place, yes) in [(LEFT_BUTTON, true), (RIGHT_BUTTON, false)] {
                    if hit(offset(dialog, place), if yes { &assets.ok } else { &assets.cancel }) {
                        game.answer_apply(yes);
                    }
                }
            }
            // Destroy asks first. OK sends the command; Cancel stays in the dungeon window.
            Scene::Game(game) if game.memorial_asked && contains(dialog_position(&assets.message), assets.message.size, cursor) => {
                let dialog = dialog_position(&assets.message);
                for (place, yes) in [(LEFT_BUTTON, true), (RIGHT_BUTTON, false)] {
                    if hit(offset(dialog, place), if yes { &assets.ok } else { &assets.cancel }) {
                        game.memorial_asked = false;
                        if yes {
                            let _ = game.session.memorial_command(net::COMMAND_MEMORIALDUNGEON_DESTROY_FORCE);
                        }
                    }
                }
            }
            // Save point asks first. OK goes; Cancel stays.
            Scene::Game(game) if game.save_asked && contains(dialog_position(&assets.message), assets.message.size, cursor) => {
                let dialog = dialog_position(&assets.message);
                for (place, yes) in [(LEFT_BUTTON, true), (RIGHT_BUTTON, false)] {
                    if hit(offset(dialog, place), if yes { &assets.ok } else { &assets.cancel }) {
                        game.save_asked = false;
                        if yes {
                            game.escape = None;
                            let _ = game.session.restart(false);
                        }
                    }
                }
            }
            // Confirm Deal: Buy sends the vendor's deal; either button closes it.
            Scene::Game(game) if game.deal_confirm.as_ref().is_some_and(|confirm| confirm.hit(cursor).is_some()) => {
                if let Some(buy) = game.deal_confirm.as_ref().unwrap().hit(cursor).unwrap() {
                    let confirm = game.deal_confirm.take().unwrap();
                    if buy {
                        let _ = confirm.shop.send(&mut game.session);
                    }
                }
            }
            // The message box, over every window: OK (alone when it only tells) or Cancel.
            Scene::Game(game) if game.question_box.is_some() && contains(dialog_position(&assets.message), assets.message.size, cursor) => {
                let dialog = dialog_position(&assets.message);
                let asks = game.question_box.as_ref().is_some_and(BoxQuestion::asks);
                for (place, yes) in box_buttons(asks) {
                    if hit(offset(dialog, place), if yes { &assets.ok } else { &assets.cancel }) {
                        game.answer_box(yes, &self.messages);
                    }
                }
            }
            // The option menu keeps the pointer until it's closed, but for the settings windows it opened over it and the
            // menu buttons' "option", which shuts it.
            // ponytail: a click off them does nothing.
            Scene::Game(game) if game.escape.is_some() && !game.settings_at(cursor) && game.basic_info.hit(cursor) != Some(basic_info::Hit::Menu("option")) => {
                match game.escape.as_ref().unwrap().hit(cursor, game.can_resurrect()).flatten() {
                    Some(escape::Choice::SavePoint) => game.save_asked = true,
                    Some(escape::Choice::CharSelect) => {
                        game.escape = None;
                        let _ = game.session.restart(true);
                    }
                    Some(escape::Choice::Exit) => {
                        game.escape = None;
                        let _ = game.session.quit();
                    }
                    Some(escape::Choice::Return) => game.escape = None,
                    Some(escape::Choice::Resurrect) => {
                        game.escape = None;
                        let _ = game.session.standing_resurrection();
                    }
                    // Game settings and shortcut settings open or shut their window; the menu stays up.
                    Some(escape::Choice::GameSettings) => {
                        game.graphics_window = toggled(game.graphics_window.take(), options::GraphicsWindow::new);
                        game.sync_esc();
                    }
                    Some(escape::Choice::Shortcuts) => {
                        game.shortcuts_window = toggled(game.shortcuts_window.take(), options::ShortcutsWindow::new);
                        game.sync_esc();
                    }
                    None => {}
                }
            }
            Scene::Game(game) if game.whisper_at(cursor) => {
                game.whisper_click(cursor);
            }
            // The audio settings: checkboxes and arrows (the sliders go on press).
            Scene::Game(game) if game.audio_window.as_ref().is_some_and(|window| window.hit(cursor).is_some()) => {
                let window = game.audio_window.as_mut().unwrap();
                let hit = window.hit(cursor).unwrap();
                if window.click(hit, &mut self.sound) {
                    if let Some(audio) = &mut self.audio {
                        audio.set(&self.sound);
                    }
                    self.sound.save();
                }
                if !window.frame.open {
                    game.audio_window = None;
                }
            }
            Scene::Game(game) if game.graphics_window.as_ref().is_some_and(|window| window.hit(cursor).is_some()) => {
                let window = game.graphics_window.as_mut().unwrap();
                let hit = window.hit(cursor).unwrap();
                if window.click(hit, &mut self.graphics) {
                    self.graphics.save();
                }
                if !window.frame.open {
                    game.graphics_window = None;
                }
            }
            Scene::Game(game) if game.shortcuts_window.as_ref().is_some_and(|window| window.hit(cursor).is_some()) => {
                let window = game.shortcuts_window.as_mut().unwrap();
                let hit = window.hit(cursor).unwrap();
                window.click(hit);
                if !window.frame.open {
                    game.shortcuts_window = None;
                }
            }
            // The quest window: tabs, quests, their locks, and the picked one's details.
            Scene::Game(game) if game.achievement_window.covers(cursor) => {
                let hit = game.achievement_window.hit(cursor, &self.achievement_info, &game.achievements).unwrap();
                if let Some(id) = game.achievement_window.click(hit) {
                    let _ = game.session.claim_achievement(id);
                }
            }
            // The quest details' NPC picture: navigation to it, as a <NAVI> link.
            Scene::Game(game) if game.quest_window.hit(cursor, &game.quests, &self.quest_texts) == Some(quest::Hit::DetailNpc) => {
                if let Some((map, cell)) = game.quest_window.npc_place(&self.quest_texts) {
                    let map = navigation::map_key(&map);
                    let name = self.navi_tables.maps.get(&map).cloned().unwrap_or_else(|| map.clone());
                    let cell = (cell != [0, 0]).then(|| cell.map(|value| value.clamp(0, i32::from(u16::MAX)) as u16));
                    game.navigation = Some(navigation::Active::new(navigation::Target { cell, monster: None, name, map }, NAVI_SERVICES));
                }
            }
            Scene::Game(game) if game.quest_window.covers(cursor) => {
                let hit = game.quest_window.hit(cursor, &game.quests, &self.quest_texts).unwrap();
                let tracker = game.quest_window.tracker;
                if let Some((id, active)) = game.quest_window.click(hit, &game.quests, &self.quest_texts) {
                    let _ = game.session.set_quest_active(id, active);
                }
                // The tracker's checkbox is kept with the settings.
                if game.quest_window.tracker != tracker
                    && let Some(path) = settings_path("quest_tracker")
                {
                    let _ = path
                        .parent()
                        .map_or(Ok(()), std::fs::create_dir_all)
                        .and_then(|()| std::fs::write(path, if game.quest_window.tracker { "1" } else { "0" }));
                }
            }
            // The mail windows: the mailbox, a mail read, one written, and the new-mail icon.
            Scene::Game(game) if game.mail.hit(cursor).is_some() => {
                let hit = game.mail.hit(cursor).unwrap();
                if let Some(action) = game.mail.click(hit)
                    && let Some(id) = game.mail_action(action)
                {
                    game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ERROR);
                }
            }
            // Setting up a shop: its name, its goods and prices, OK or cancel.
            Scene::Game(game) if game.vend_setup.as_ref().is_some_and(|setup| setup.hit(cursor, &game.cart.items).is_some()) => {
                let setup = game.vend_setup.as_mut().unwrap();
                let hit = setup.hit(cursor, &game.cart.items).unwrap();
                match setup.click(hit, &game.cart.items) {
                    Some(vend::Action::Price(index)) => game.amount = Some(window::Amount::vend_price(Asked::VendPrice(index))),
                    Some(vend::Action::Open) => game.open_vending(),
                    Some(vend::Action::AskFree) => game.question_box = Some(BoxQuestion::FreeGoods),
                    Some(vend::Action::Refused(message)) => game.question_box = Some(BoxQuestion::Notice(message)),
                    Some(vend::Action::Cancel) => {
                        let _ = game.session.open_shop("", false, &[]);
                        game.vend_setup = None;
                    }
                    None => {}
                }
            }
            // Our open shop: closing it closes the shop.
            Scene::Game(game) if game.my_shop.as_ref().is_some_and(|shop| shop.hit(cursor).is_some()) => {
                if game.my_shop.as_ref().unwrap().hit(cursor) == Some(true) {
                    let _ = game.session.close_shop();
                    game.my_shop = None;
                    game.signs.remove(&game.account_id);
                }
            }
            // Setting up a buying store: its name, zeny limit, wanted items, OK or cancel.
            Scene::Game(game)
                if game.amount.is_none() && game.buying_setup.as_ref().is_some_and(|setup| setup.hit(cursor, &game.inventory.items).is_some()) =>
            {
                let setup = game.buying_setup.as_mut().unwrap();
                let hit = setup.hit(cursor, &game.inventory.items).unwrap();
                match setup.click(hit, &game.inventory.items) {
                    Some(buying::Action::Price(index, item_id)) => {
                        game.amount = Some(window::Amount::price(Asked::BuyingPrice(index, item_id)));
                    }
                    Some(buying::Action::Open) => {
                        let zeny = game.status.get(&net::STATUS_ZENY).copied().unwrap_or(0).max(0) as u32;
                        match setup.validate(&game.inventory.items, zeny) {
                            Ok((limit, items)) => {
                                let name = setup.name.clone();
                                let _ = game.session.open_buying_store(&name, limit, true, &items);
                            }
                            Err(Some((msg_id, Some(item_id)))) => {
                                let item_name = inventory::item_name(&self.items, item_id, true, 0);
                                let line = rustnarok_client::messages::fill(self.messages.get(msg_id).map_or("", String::as_str), &[&item_name]);
                                game.chat_line(line, CHAT_ERROR);
                            }
                            Err(Some((msg_id, None))) => {
                                game.chat_line(self.messages.get(msg_id).cloned().unwrap_or_default(), CHAT_ERROR);
                            }
                            Err(None) => {}
                        }
                    }
                    Some(buying::Action::Cancel) => {
                        let _ = game.session.open_buying_store("", 0, false, &[]);
                        game.buying_setup = None;
                    }
                    None => {}
                }
            }
            // Our open buying store: closing it closes the store and drops the sign.
            Scene::Game(game) if game.amount.is_none() && game.my_buying_store.as_ref().is_some_and(|store| store.hit(cursor).is_some()) => {
                if game.my_buying_store.as_ref().unwrap().hit(cursor) == Some(true) {
                    let _ = game.session.close_buying_store();
                    game.my_buying_store = None;
                    game.signs.remove(&game.account_id);
                }
            }
            // A card's picture: its close button. A click anywhere on it stays there.
            Scene::Game(game) if game.card_view.as_ref().is_some_and(|view| view.hit(cursor).is_some()) => {
                if game.card_view.as_ref().unwrap().hit(cursor) == Some(iteminfo::ViewHit::Close) {
                    game.card_view = None;
                }
            }
            // A card's own window, over the description: its close button or its picture.
            Scene::Game(game) if game.card_window.as_ref().is_some_and(|window| window.hit(cursor).is_some()) => {
                match game.card_window.as_ref().unwrap().hit(cursor) {
                    Some(iteminfo::Hit::Close) => game.card_window = None,
                    Some(iteminfo::Hit::View) => {
                        let card = game.card_window.as_ref().unwrap().shown.item;
                        game.toggle_card_view(card, &self.card_pictures, &self.items);
                    }
                    _ => {}
                }
            }
            // The item description window: its close button or a card's picture (a click on a card in the strip does nothing).
            Scene::Game(game) if game.item_window.as_ref().is_some_and(|window| window.hit(cursor).is_some()) => {
                match game.item_window.as_ref().unwrap().hit(cursor) {
                    Some(iteminfo::Hit::Close) => {
                        game.item_window = None;
                        game.card_view = None;
                    }
                    Some(iteminfo::Hit::View) => {
                        let item = game.item_window.as_ref().unwrap().shown.item;
                        game.toggle_card_view(item, &self.card_pictures, &self.items);
                    }
                    _ => {}
                }
            }
            // The right-click menu takes the next click, wherever it lands.
            Scene::Game(game) if game.menu.is_some() => match game.menu.take().unwrap().hit(cursor).flatten() {
                // Not while talking to an NPC or in its shop.
                Some(Choice::Trade(_)) if game.dialog.is_some() || game.shop.is_some() => {
                    game.chat_line(self.messages.get(TASK_IN_PROGRESS).cloned().unwrap_or_default(), CHAT_WHITE);
                }
                Some(Choice::Trade(id)) => {
                    game.trade_with = game.units.get(&id).map(|actor| actor.name.clone()).unwrap_or_default();
                    let _ = game.session.request_trade(id);
                }
                Some(Choice::ViewEquipment(id)) => {
                    let _ = game.session.view_equipment(id);
                }
                // Their name in the chat's name box, as the friend list's whisper does.
                Some(Choice::Uninstall(kind)) => drop(game.session.uninstall(kind)),
                Some(Choice::Whisper(id)) => {
                    if let Some(actor) = game.units.get(&id) {
                        game.chat.whisper = actor.name.clone();
                        game.chat.naming = false;
                    }
                }
                Some(Choice::Mail(id)) => {
                    if let Some(name) = game.units.get(&id).map(|actor| actor.name.clone()) {
                        let _ = game.session.start_mail(&name);
                    }
                }
                // As /ex: their whispers blocked.
                Some(Choice::Block(id)) => {
                    if let Some(name) = game.units.get(&id).map(|actor| actor.name.clone())
                        && game.session.set_whisper(&name, false).is_ok()
                    {
                        game.whisper_asked.push_back(name);
                    }
                }
                Some(Choice::HomunculusStatus) => {
                    let _ = game.session.homunculus_command(net::HOMUNCULUS_STATUS);
                    game.homunculus_window.frame.toggle();
                    game.raise(Panel::Homunculus);
                    game.sync_esc();
                }
                Some(Choice::HomunculusFeed) => game.homunculus_asked = Some(HomunculusAsked::Feed),
                Some(Choice::HomunculusStandBy) => game.homunculus_steering.following = !game.homunculus_steering.following,
                Some(Choice::MercenaryStatus) => {
                    game.mercenary_window.frame.toggle();
                    game.raise(Panel::Mercenary);
                    game.sync_esc();
                }
                // As Ctrl+T.
                Some(Choice::MercenaryStandBy) => game.steering.following = !game.steering.following,
                Some(Choice::CloseStall) => {
                    let _ = game.session.close_shop();
                    game.my_shop = None;
                    game.signs.remove(&game.account_id);
                }
                Some(Choice::Invite(id)) => {
                    if let Some(actor) = game.units.get(&id) {
                        let _ = game.session.invite_to_party(&actor.name.clone());
                    }
                }
                Some(Choice::Adopt(id)) => {
                    let _ = game.session.ask_adoption(id);
                }
                // Sent: "Send (name) a Guild Invitation" in cyan.
                Some(Choice::GuildInvite(id)) => {
                    if let Some(name) = game.units.get(&id).map(|actor| actor.name.clone())
                        && game.session.invite_to_guild(&name).is_ok()
                    {
                        let line = rustnarok_client::messages::fill(self.messages.get(GUILD_CHOICE).map_or("", String::as_str), &[&name]);
                        game.chat_line(line, [0x00, 0xFF, 0xFF]);
                    }
                }
                Some(Choice::Ally(id)) => {
                    let _ = game.session.ask_alliance(id, game.account_id, game.me.id);
                }
                Some(Choice::Antagonist(id)) => {
                    let _ = game.session.set_antagonist(id);
                }
                // Feeding asks first; the status opens the pet window.
                Some(Choice::Pet(net::PET_FEED)) => game.ask_feed(&self.messages),
                // Check Pet Status asks for the news and opens or shuts the window.
                Some(Choice::Pet(net::PET_INFO)) => {
                    let _ = game.session.pet_command(net::PET_INFO);
                    game.pet_window.frame.toggle();
                    game.raise(Panel::Pet);
                }
                Some(Choice::Pet(command)) => game.pet_command(command),
                Some(Choice::Member(choice, account)) => game.member_choice(choice, account, &self.messages),
                Some(Choice::RoomOwner(row)) => {
                    if let Some(name) = game.room.as_ref().and_then(|room| room.member_choices(row, &game.me.name)) {
                        let _ = game.session.hand_over_chat_room(&name);
                    }
                }
                Some(Choice::RoomKick(row)) => {
                    if let Some(name) = game.room.as_ref().and_then(|room| room.member_choices(row, &game.me.name)) {
                        let _ = game.session.kick_from_chat_room(&name);
                    }
                }
                // The details alone, beside the list when it's open, else in the middle.
                Some(Choice::QuestInfo(id)) => {
                    game.quest_window.pick(id, screen());
                    game.sync_esc();
                }
                Some(Choice::QuestAside(id)) => {
                    let _ = game.session.set_quest_active(id, false);
                }
                Some(Choice::EndRelation(id, antagonist)) => {
                    let _ = game.session.end_relation(id, antagonist);
                }
                // A guild skill's description, where the menu was.
                Some(Choice::GuildSkillInfo(id)) => {
                    let lines = self.skill_descriptions.get(&id).cloned().unwrap_or_default();
                    let active = game.skill_active(id);
                    game.skill_description =
                        Some(skill_descript::SkillDescription::new(id, &lines, true, active, |height| skill_descript::at_window(cursor, height)));
                    game.sync_esc();
                }
                Some(Choice::GuildMember(choice, index)) => {
                    if let Some(member) = game.guild.as_ref().and_then(|guild| guild.members.get(index)).cloned() {
                        match choice {
                            guild::MemberChoice::Leave => game.reason_box = Some(guild::ReasonBox::new(guild::Reasoned::Leave)),
                            guild::MemberChoice::Expel => {
                                game.reason_box = Some(guild::ReasonBox::new(guild::Reasoned::Expel(member.account, member.character)))
                            }
                            guild::MemberChoice::Assign => {
                                game.question_box = Some(BoxQuestion::GuildLeader { account: member.account, character: member.character, name: member.name })
                            }
                            guild::MemberChoice::PartyInvite => {
                                let _ = game.session.invite_to_party(&member.name);
                            }
                            guild::MemberChoice::Whisper => game.whisper_member(&member.name),
                            guild::MemberChoice::Mail => {
                                let _ = game.session.start_mail(&member.name);
                            }
                        }
                    }
                }
                Some(Choice::Befriend(id)) => {
                    if let Some(actor) = game.units.get(&id) {
                        let _ = game.session.add_friend(&actor.name.clone());
                    }
                }
                None => {}
            },
            // The room maker's fields and buttons.
            // The emotion list: a click shows that emotion.
            Scene::Game(game) if game.emotion_list.as_ref().is_some_and(|list| list.hit(cursor).is_some()) => {
                match game.emotion_list.as_ref().unwrap().hit(cursor) {
                    Some(emotion_list::Hit::Title(TitleHit::Close)) => game.emotion_list = None,
                    Some(emotion_list::Hit::Emotion(kind)) => {
                        let _ = game.session.emote(kind);
                    }
                    _ => {}
                }
            }
            // The macro window: a line's field takes the typing.
            Scene::Game(game) if game.macro_window.as_ref().is_some_and(|window| window.hit(cursor).is_some()) => {
                match game.macro_window.as_ref().unwrap().hit(cursor) {
                    Some(macros::Hit::Title(TitleHit::Close)) => game.toggle_macro_window(),
                    Some(macros::Hit::Line(line)) => game.macro_window.as_mut().unwrap().editing = Some(line),
                    _ => game.macro_window.as_mut().unwrap().editing = None,
                }
            }
            Scene::Game(game) if game.room_make.as_ref().is_some_and(|make| make.hit(cursor).is_some()) => {
                let make = game.room_make.as_mut().unwrap();
                let hit = make.hit(cursor).unwrap();
                if make.click(hit) {
                    game.make_room();
                } else if hit == chatroom::MakeHit::Cancel {
                    game.room_make = None;
                }
            }
            // The room we're in: closing it leaves.
            Scene::Game(game) if game.room.as_ref().is_some_and(|room| room.hit(cursor, &game.me.name).is_some()) => {
                let room = game.room.as_mut().unwrap();
                match room.hit(cursor, &game.me.name) {
                    Some(chatroom::RoomHit::Title(TitleHit::Close)) => {
                        let _ = game.session.leave_chat_room();
                    }
                    Some(chatroom::RoomHit::Title(title)) => room.frame.click_title(title),
                    // The owner's Room Setup: the make window, filled from the room.
                    Some(chatroom::RoomHit::Setup) => game.room_make = Some(chatroom::Make::from_sign(&room.sign)),
                    _ => {}
                }
            }
            // A chat room's sign: open rooms let us in, locked ones ask for the password.
            Scene::Game(game)
                if game.room.is_none()
                    && game.sign_rects.iter().any(|(rect, ..)| contains([rect[0], rect[1]], [rect[2] - rect[0], rect[3] - rect[1]], cursor)) =>
            {
                let &(_, id, kind) =
                    game.sign_rects.iter().rev().find(|(rect, ..)| contains([rect[0], rect[1]], [rect[2] - rect[0], rect[3] - rect[1]], cursor)).unwrap();
                let now = crate::control::now();
                let open = game.shop.as_ref().and_then(|shop| shop.vendor.as_ref().map(|vendor| vendor.0).or(shop.buying_store.map(|store| store.0)));
                let since = game.shop_asked.map(|at| now.saturating_duration_since(at));
                let _ = match kind {
                    // A vendor's or buyer's sign opens their shop.
                    chatroom::SHOP | chatroom::BUYING_STORE if !vend::may_ask(id, game.account_id, game.player.dead, open, since) => Ok(()),
                    chatroom::SHOP => {
                        game.shop_asked = Some(now);
                        game.session.browse_vendor(id)
                    }
                    chatroom::BUYING_STORE => {
                        game.shop_asked = Some(now);
                        game.session.click_buying_store(id)
                    }
                    0 => {
                        game.amount = Some(window::Amount::name(Asked::RoomPassword(id), ROOM_PASSWORD));
                        Ok(())
                    }
                    _ => game.session.enter_chat_room(id, ""),
                };
            }
            // A guild invites us: OK or cancel.
            // At the party invitation's place; accepting closes the Guild Companion.
            Scene::Game(game) if game.guild_invite.is_some() && contains(party::invitation_at(screen()), assets.message.size, cursor) => {
                let dialog = party::invitation_at(screen());
                for (place, accept) in [(LEFT_BUTTON, true), (RIGHT_BUTTON, false)] {
                    if hit(offset(dialog, place), if accept { &assets.ok } else { &assets.cancel }) {
                        let (guild, _) = game.guild_invite.take().unwrap();
                        let _ = game.session.answer_guild(guild, accept);
                        if accept {
                            game.guild_companion.close();
                        }
                    }
                }
            }
            // A guild question: OK or cancel. Whoever is asked for an alliance answers either way.
            Scene::Game(game) if game.guild_asked.is_some() && contains(dialog_position(&assets.message), assets.message.size, cursor) => {
                let dialog = dialog_position(&assets.message);
                for (place, yes) in [(LEFT_BUTTON, true), (RIGHT_BUTTON, false)] {
                    if hit(offset(dialog, place), if yes { &assets.ok } else { &assets.cancel }) {
                        let GuildAsked::Ally { id, .. } = game.guild_asked.take().unwrap();
                        let _ = game.session.answer_alliance(id, yes);
                    }
                }
            }
            // Wearing an item that binds: OK wears it.
            Scene::Game(game) if game.bind_asked.is_some() && contains(dialog_position(&assets.message), assets.message.size, cursor) => {
                let dialog = dialog_position(&assets.message);
                for (place, yes) in [(LEFT_BUTTON, true), (RIGHT_BUTTON, false)] {
                    if hit(offset(dialog, place), if yes { &assets.ok } else { &assets.cancel }) {
                        game.answer_bind(yes);
                    }
                }
            }
            // Growing the inventory: OK or cancel.
            Scene::Game(game) if game.expansion_asked.is_some() && contains(dialog_position(&assets.message), assets.message.size, cursor) => {
                let dialog = dialog_position(&assets.message);
                for (place, yes) in [(LEFT_BUTTON, true), (RIGHT_BUTTON, false)] {
                    if hit(offset(dialog, place), if yes { &assets.ok } else { &assets.cancel }) {
                        game.expansion_asked = None;
                        let _ = game.session.answer_expansion(yes);
                    }
                }
            }
            // A queue question: OK applies, leaves the queue or enters the battle; Cancel doesn't (or declines entering).
            Scene::Game(game) if game.queue_asked.is_some() && contains(dialog_position(&assets.message), assets.message.size, cursor) => {
                let dialog = dialog_position(&assets.message);
                for (place, yes) in [(LEFT_BUTTON, true), (RIGHT_BUTTON, false)] {
                    if hit(offset(dialog, place), if yes { &assets.ok } else { &assets.cancel }) {
                        let _ = match game.queue_asked.take().unwrap() {
                            QueueAsked::Apply(kind, name) if yes => game.session.apply_to_queue(kind, &name),
                            QueueAsked::Cancel(name) if yes => game.session.leave_queue(&name),
                            QueueAsked::Lobby(name, _) => game.session.answer_lobby(yes, &name),
                            _ => Ok(()),
                        };
                    }
                }
            }
            // The memorial window: drag is a press; a click closes, or asks before Destroy.
            Scene::Game(game) if game.memorial.frame.open && game.memorial.hit(cursor).is_some() => match game.memorial.hit(cursor) {
                Some(memorial::Hit::Title(title)) => {
                    game.memorial.frame.click_title(title);
                    if !game.memorial.frame.open {
                        game.memorial_asked = false;
                        escape::remove(&mut game.esc_order, escape::WindowId::Memorial);
                    }
                }
                Some(memorial::Hit::Destroy) => game.memorial_asked = true,
                _ => {}
            },
            // The queue window: a battleground picked, applying, cancelling.
            Scene::Game(game) if game.queue_window.frame.open && game.queue_window.hit(cursor, &self.battlegrounds, game.queued.is_some()).is_some() => {
                match game.queue_window.hit(cursor, &self.battlegrounds, game.queued.is_some()) {
                    Some(battle_queue::Hit::Title(title)) => game.queue_window.frame.click_title(title),
                    Some(battle_queue::Hit::Row(row)) => game.queue_window.picked = Some(row),
                    Some(battle_queue::Hit::Apply(kind)) => {
                        if let Some(entry) = game.queue_window.picked.and_then(|row| self.battlegrounds.get(row)) {
                            game.queue_asked = Some(QueueAsked::Apply(kind, entry.name.clone()));
                        }
                    }
                    Some(battle_queue::Hit::Cancel) => {
                        if let Some((name, _)) = &game.queued {
                            game.queue_asked = Some(QueueAsked::Cancel(name.clone()));
                        }
                    }
                    _ => {}
                }
            }
            // The big minimap: a click on it, before the small one's buttons (it can cover them).
            Scene::Game(game) if game.minimap_window.covers(cursor) => {
                let size = [game.world.gat.width as u32, game.world.gat.height as u32];
                let quests = game.big_minimap_quests();
                let pins = minimap_window::Pins { player: game.player.cell, party: &[], guild: &[], boss: None, quests: &quests };
                match game.minimap_window.hit(cursor, size, &pins) {
                    // A quest NPC: navigation to its cell on this map.
                    Some(minimap_window::Hit::Quest(cell)) => {
                        let map = navigation::map_key(&game.map);
                        let name = self.navi_tables.maps.get(&map).cloned().unwrap_or_else(|| map.clone());
                        let target = navigation::Target { cell: Some(cell.map(|value| value as u16)), monster: None, name, map };
                        game.navigation = Some(navigation::Active::new(target, NAVI_SERVICES));
                    }
                    Some(hit) => {
                        game.minimap_window.click(hit);
                        game.sync_esc();
                    }
                    None => {}
                }
            }
            // The minimap's bottom row: zoom (kept with the settings), the big map, the route flag.
            // A click elsewhere on the minimap does nothing.
            Scene::Game(game) if game.window_at(cursor).is_none() && game.minimap_shown() && minimap::covers(cursor) => match minimap::button_at(cursor) {
                Some(button @ ("plus" | "minus")) => {
                    game.minimap_zoom = if button == "plus" { minimap::zoom_in(game.minimap_zoom) } else { minimap::zoom_out(game.minimap_zoom) };
                    if let Some(path) = settings_path("minimap_zoom") {
                        let _ = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(path, game.minimap_zoom.to_string()));
                    }
                }
                Some("mini") => {
                    if let Some(gpu) = &self.gpu {
                        toggle_big_minimap(game, gpu, &self.grf, &self.minimap_exceptions);
                    }
                }
                // ponytail: #276 draws the navigation route when this is on. The route itself is not drawn here.
                Some("object") => game.navigation_route = !game.navigation_route,
                // View On: the world map, open or closed, as its shortcut does.
                Some("viewon") => toggle_world_map(game, self.gpu.as_ref(), &self.grf, &self.worlds),
                _ => {}
            },
            // The new-quest icon: the quest window.
            Scene::Game(game) if game.quest_icon && quest_notice::on_icon(cursor) && game.window_at(cursor).is_none() => {
                game.quest_window.open();
                game.quest_icon = false;
                game.sync_esc();
            }
            // A level up button: the status window (base level) or the skill window (job level), restored if minimised.
            Scene::Game(game) if game.level_ups.hit(cursor).is_some() && game.window_at(cursor).is_none() => {
                match game.level_ups.hit(cursor).unwrap() {
                    level_up::Button::Base => {
                        game.status_window.frame.open = true;
                        game.status_window.frame.minimized = false;
                        game.raise(Panel::Status);
                    }
                    level_up::Button::Job => {
                        game.skill_window.frame.open = true;
                        game.skill_window.frame.minimized = false;
                        game.raise(Panel::Skills);
                    }
                }
                game.sync_esc();
            }
            // A skill's description: its delay button opens or closes the delays beside it, its close button closes it;
            // a click elsewhere on it does nothing.
            Scene::Game(game) if game.skill_description.as_ref().is_some_and(|window| window.contains(cursor)) => {
                if let Some(window) = game.skill_description.as_mut().filter(|window| window.on_delay(cursor)) {
                    window.delays = !window.delays;
                    game.sync_esc();
                } else if game.skill_description.as_ref().is_some_and(|window| window.on_close(cursor)) {
                    game.skill_description = None;
                    game.sync_esc();
                }
            }
            // The store search: the kind, the fields, searching, paging, and a row twice to find its store.
            // A store it opened stands over it.
            Scene::Game(game)
                if game.catalog.as_ref().and_then(|catalog| catalog.hit(cursor)).is_some() && !game.shop.as_ref().is_some_and(|shop| shop.covers(cursor)) =>
            {
                let hit = game.catalog.as_ref().and_then(|catalog| catalog.hit(cursor));
                game.click_catalog(hit.unwrap(), &self.items, &self.messages);
            }
            // The ranking board: another ranking, or closing it.
            Scene::Game(game) if game.ranking.as_ref().and_then(|board| board.hit(cursor)).is_some() => {
                match game.ranking.as_ref().and_then(|board| board.hit(cursor)) {
                    Some(ranking::Hit::Kind(kind)) => {
                        let _ = game.session.ask_ranking(kind);
                    }
                    Some(ranking::Hit::Title(TitleHit::Close)) => game.ranking = None,
                    Some(ranking::Hit::Title(title)) => game.ranking.as_mut().unwrap().frame.click_title(title),
                    _ => {}
                }
            }
            // The party booking board.
            Scene::Game(game) if game.booking.as_ref().and_then(|window| window.hit(cursor)).is_some() => {
                let window = game.booking.as_mut().unwrap();
                let hit = window.hit(cursor).unwrap();
                if let Some(action) = window.click(hit) {
                    game.booking_action(action);
                }
            }
            // Someone asks to join through our post: OK lets them in.
            Scene::Game(game) if game.booking_asked.is_some() && contains(dialog_position(&assets.message), assets.message.size, cursor) => {
                let dialog = dialog_position(&assets.message);
                for (place, accept) in [(LEFT_BUTTON, true), (RIGHT_BUTTON, false)] {
                    if hit(offset(dialog, place), if accept { &assets.ok } else { &assets.cancel }) {
                        let (character, account, ..) = game.booking_asked.take().unwrap();
                        let _ = game.session.answer_join(character, account, accept);
                    }
                }
            }
            // The navigation window: its field, a result to go to, stopping.
            Scene::Game(game) if game.navi_window.as_ref().and_then(|window| window.hit(cursor)).is_some() => {
                let window = game.navi_window.as_mut().unwrap();
                match window.hit(cursor).unwrap() {
                    navigation::Hit::Title(TitleHit::Close) => game.navi_window = None,
                    navigation::Hit::Title(title) => window.frame.click_title(title),
                    navigation::Hit::Field => window.typing = true,
                    navigation::Hit::Result(index) => {
                        let target = window.results[index].clone();
                        window.typing = false;
                        game.navigation = Some(navigation::Active::new(target, NAVI_SERVICES));
                    }
                    navigation::Hit::Stop => {
                        window.typing = false;
                        game.navigation = None;
                    }
                    navigation::Hit::Window => window.typing = false,
                }
            }
            // The reputation window: its title bar.
            Scene::Game(game) if game.reputation_window.as_ref().and_then(|window| window.hit(cursor)).is_some() => {
                match game.reputation_window.as_ref().and_then(|window| window.hit(cursor)) {
                    Some(reputation::Hit::Title(TitleHit::Close)) => game.reputation_window = None,
                    Some(reputation::Hit::Title(title)) => game.reputation_window.as_mut().unwrap().frame.click_title(title),
                    _ => {}
                }
            }
            // The dress room: pickers, turning, closing.
            Scene::Game(game) if game.dressroom.as_ref().and_then(|room| room.hit(cursor)).is_some() => {
                let room = game.dressroom.as_mut().unwrap();
                if room.hit(cursor).is_some_and(|hit| room.click(hit, &game.dress_lists)) {
                    game.dressroom = None;
                    game.dress_figure = None;
                }
            }
            // Handing the party over: OK asks the server.
            Scene::Game(game) if game.leader_asked.is_some() && contains(dialog_position(&assets.message), assets.message.size, cursor) => {
                let dialog = dialog_position(&assets.message);
                for (place, yes) in [(LEFT_BUTTON, true), (RIGHT_BUTTON, false)] {
                    if hit(offset(dialog, place), if yes { &assets.ok } else { &assets.cancel }) {
                        let account = game.leader_asked.take().unwrap();
                        if yes {
                            let _ = game.session.change_party_leader(account);
                        }
                    }
                }
            }
            // The Guild Companion and its windows; the create (or disband) window's OK checks the name.
            Scene::Game(game) if game.guild_companion.hit(cursor).is_some() => match game.guild_companion.hit(cursor) {
                Some(guild_companion::Hit::CreateOk) => game.guild_name_ok(&self.messages),
                Some(hit) => game.guild_companion.click(hit),
                None => {}
            },
            // The guild's reason box: OK sends, Cancel closes.
            Scene::Game(game) if game.reason_box.as_ref().is_some_and(|reason| reason.hit(cursor, screen()).is_some()) => {
                match game.reason_box.as_ref().and_then(|reason| reason.hit(cursor, screen())) {
                    Some(guild::ReasonHit::Ok) => game.answer_reason(true),
                    Some(guild::ReasonHit::Cancel) => game.answer_reason(false),
                    _ => {}
                }
            }
            // The party's docked windows (setup, create, invite): choices, the edit box, OK and Cancel.
            Scene::Game(game) if game.party_dialog.as_ref().and_then(|dialog| dialog.hit(cursor)).is_some() => {
                let dialog = game.party_dialog.as_mut().unwrap();
                let hit = dialog.hit(cursor);
                dialog.typing = hit == Some(party::DialogHit::Edit) || (dialog.typing && hit != Some(party::DialogHit::Window));
                let action = hit.and_then(|hit| dialog.click(hit));
                if let Some(action) = action {
                    game.party_dialog_action(action, &self.messages);
                }
            }
            // Dismissing the mercenary: OK lets it go.
            Scene::Game(game) if game.dismiss_asked && contains(dialog_position(&assets.message), assets.message.size, cursor) => {
                let dialog = dialog_position(&assets.message);
                for (place, yes) in [(LEFT_BUTTON, true), (RIGHT_BUTTON, false)] {
                    if hit(offset(dialog, place), if yes { &assets.ok } else { &assets.cancel }) {
                        game.answer_dismiss(yes, &self.messages);
                    }
                }
            }
            // A homunculus question: OK feeds, deletes or renames it.
            Scene::Game(game) if game.homunculus_asked.is_some() && contains(dialog_position(&assets.message), assets.message.size, cursor) => {
                let dialog = dialog_position(&assets.message);
                for (place, yes) in [(LEFT_BUTTON, true), (RIGHT_BUTTON, false)] {
                    if hit(offset(dialog, place), if yes { &assets.ok } else { &assets.cancel }) {
                        game.answer_homunculus(yes, &self.messages);
                    }
                }
            }
            // A pet question: OK feeds it or renames it.
            Scene::Game(game) if game.pet_asked.is_some() && contains(dialog_position(&assets.message), assets.message.size, cursor) => {
                let dialog = dialog_position(&assets.message);
                for (place, yes) in [(LEFT_BUTTON, true), (RIGHT_BUTTON, false)] {
                    if hit(offset(dialog, place), if yes { &assets.ok } else { &assets.cancel }) {
                        game.answer_pet(yes, &self.pets, &self.items, &self.messages);
                    }
                }
            }
            // A cash shop purchase: OK sends the cart.
            Scene::Game(game) if game.cashshop_buy_asked && contains(dialog_position(&assets.message), assets.message.size, cursor) => {
                let dialog = dialog_position(&assets.message);
                for (place, accept) in [(LEFT_BUTTON, true), (RIGHT_BUTTON, false)] {
                    if hit(offset(dialog, place), if accept { &assets.ok } else { &assets.cancel }) {
                        game.cashshop_buy_asked = false;
                        if let Some((free, items)) = game.cashshop.as_ref().and_then(|shop| shop.buy_request(accept)) {
                            let _ = game.session.buy_cashshop(free, &items);
                        }
                    }
                }
            }
            // Someone wants to be friends: OK or cancel.
            Scene::Game(game) if game.friend_asked.is_some() && contains(dialog_position(&assets.message), assets.message.size, cursor) => {
                let dialog = dialog_position(&assets.message);
                for (place, accept) in [(LEFT_BUTTON, true), (RIGHT_BUTTON, false)] {
                    if hit(offset(dialog, place), if accept { &assets.ok } else { &assets.cancel }) {
                        let (account, character, _) = game.friend_asked.take().unwrap();
                        let _ = game.session.answer_friend(account, character, accept);
                    }
                }
            }
            // Asked to be adopted: OK asks again, then accepts; Cancel at either step refuses.
            Scene::Game(game) if game.adopt_asked.is_some() && contains(dialog_position(&assets.message), assets.message.size, cursor) => {
                let dialog = dialog_position(&assets.message);
                for (place, yes) in [(LEFT_BUTTON, true), (RIGHT_BUTTON, false)] {
                    if hit(offset(dialog, place), if yes { &assets.ok } else { &assets.cancel }) {
                        let (parent, partner, name, again) = game.adopt_asked.take().unwrap();
                        if yes && !again {
                            // What a baby class gives up doesn't fit the box: the whole of it in the chat, the box asks.
                            game.chat_line(self.messages.get(ADOPT_ASKS_AGAIN).cloned().unwrap_or_default(), CHAT_ERROR);
                            game.adopt_asked = Some((parent, partner, name, true));
                        } else {
                            let _ = game.session.answer_adoption(parent, partner, yes);
                        }
                    }
                }
            }
            // A party invites us: OK or cancel.
            // Yes joins and opens the party window on its party tab.
            Scene::Game(game) if game.party_invite.is_some() && contains(party::invitation_at(screen()), assets.message.size, cursor) => {
                let dialog = party::invitation_at(screen());
                for (place, accept) in [(LEFT_BUTTON, true), (RIGHT_BUTTON, false)] {
                    if hit(offset(dialog, place), if accept { &assets.ok } else { &assets.cancel }) {
                        let (party, _) = game.party_invite.take().unwrap();
                        let _ = game.session.answer_party(party, accept);
                        if accept {
                            game.open_party_tab();
                        }
                    }
                }
            }
            // Someone asks for a deal: OK or cancel.
            Scene::Game(game) if game.trade_asked.is_some() && contains(dialog_position(&assets.message), assets.message.size, cursor) => {
                let dialog = dialog_position(&assets.message);
                for (place, accept) in [(LEFT_BUTTON, true), (RIGHT_BUTTON, false)] {
                    if hit(offset(dialog, place), if accept { &assets.ok } else { &assets.cancel }) {
                        game.trade_asked = None;
                        let _ = game.session.answer_trade(accept);
                    }
                }
            }
            Scene::Game(game) if game.bank.as_ref().is_some_and(|bank| bank.hit(cursor).is_some()) => {
                let carried = game.status.get(&net::STATUS_ZENY).copied().unwrap_or(0).max(0) as u32;
                let bank = game.bank.as_mut().unwrap();
                let hit = bank.hit(cursor).unwrap();
                let account = game.account_id;
                match bank.click(hit, carried) {
                    Some(bank::Action::Deposit(zeny)) => {
                        let _ = game.session.move_zeny(true, account, zeny);
                    }
                    Some(bank::Action::Withdraw(zeny)) => {
                        let _ = game.session.move_zeny(false, account, zeny);
                    }
                    Some(bank::Action::Refuse(id)) => game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ERROR),
                    Some(bank::Action::Close) => {
                        let _ = game.session.close_bank(account);
                        game.bank = None;
                    }
                    None => {}
                }
            }
            Scene::Game(game) if game.cashshop.as_ref().is_some_and(|shop| shop.hit(cursor).is_some()) => {
                let shop = game.cashshop.as_mut().unwrap();
                let hit = shop.hit(cursor).unwrap();
                match shop.click(hit) {
                    Some(cashshop::Action::Buy) => game.cashshop_buy_asked = true,
                    Some(cashshop::Action::Close) => game.close_cashshop(),
                    None => {}
                }
            }
            Scene::Game(game) if game.viewing.as_ref().is_some_and(|viewed| viewed.window.hit(cursor, &viewed.items).is_some()) => {
                let viewed = game.viewing.as_mut().unwrap();
                match viewed.window.hit(cursor, &viewed.items).unwrap() {
                    equipment::Hit::Title(TitleHit::Close) => game.viewing = None,
                    hit => {
                        viewed.window.click(hit);
                    }
                }
            }
            // Sense's window: its close button.
            Scene::Game(game) if game.monster_window.as_ref().is_some_and(|window| window.hit(cursor).is_some()) => {
                if game.monster_window.as_ref().unwrap().hit(cursor) == Some(true) {
                    game.monster_window = None;
                }
            }
            // The Lucky Roulette: START spins, Get Prize claims (neither while the wheel spins), close lets it go.
            Scene::Game(game) if game.roulette.as_ref().is_some_and(|window| window.hit(cursor).is_some()) => {
                let window = game.roulette.as_mut().unwrap();
                let hit = window.hit(cursor).unwrap();
                match window.click(hit, control::now()) {
                    Some(roulette::Action::Spin) => _ = game.session.roulette(net::RouletteRequest::Spin),
                    Some(roulette::Action::Claim) => _ = game.session.roulette(net::RouletteRequest::Claim),
                    Some(roulette::Action::Busy) => game.chat_line(self.messages.get(roulette::SPINNING).cloned().unwrap_or_default(), CHAT_ERROR),
                    Some(roulette::Action::Close) => game.close_roulette(),
                    None => {}
                }
            }
            // Its icon asks for it.
            Scene::Game(game) if roulette::icon_hit(cursor) => {
                if game.roulette.is_none() {
                    let _ = game.session.roulette(net::RouletteRequest::Open);
                }
            }
            // The material window: OK sends what's in; Cancel and the close button send nothing in (rathena forgets the
            // window either way).
            Scene::Game(game) if game.materials.as_ref().is_some_and(|window| window.hit(cursor).is_some()) => {
                let window = game.materials.as_mut().unwrap();
                let hit = window.hit(cursor).unwrap();
                if let Some(ok) = window.click(hit) {
                    let entries = if ok { window.entries.clone() } else { Vec::new() };
                    let _ = game.session.send_materials(window.kind, ok, &entries);
                    game.materials = None;
                }
            }
            Scene::Game(game) if game.synthesis.as_ref().is_some_and(|window| window.hit(cursor).is_some()) => {
                let hit = game.synthesis.as_ref().unwrap().hit(cursor).unwrap();
                match game.synthesis.as_mut().unwrap().click(hit) {
                    Some(laphine::Action::Make) => {
                        let (box_id, entries) = {
                            let window = game.synthesis.as_ref().unwrap();
                            (window.box_id, window.entries())
                        };
                        let _ = game.session.synthesize(box_id, &entries);
                    }
                    Some(laphine::Action::Close) => {
                        let _ = game.session.close_synthesis();
                        game.synthesis = None;
                    }
                    None => {}
                }
            }
            Scene::Game(game) if game.upgrade.as_ref().is_some_and(|window| window.hit(cursor).is_some()) => {
                let hit = game.upgrade.as_ref().unwrap().hit(cursor).unwrap();
                match game.upgrade.as_mut().unwrap().click(hit) {
                    Some(laphine::Action::Make) => {
                        if let Some(index) = game.upgrade.as_ref().unwrap().index() {
                            let box_id = game.upgrade.as_ref().unwrap().box_id;
                            let _ = game.session.upgrade_item(box_id, index);
                        }
                    }
                    Some(laphine::Action::Close) => {
                        let _ = game.session.close_upgrade();
                        game.upgrade = None;
                    }
                    None => {}
                }
            }
            Scene::Game(game) if game.reform.as_ref().is_some_and(|window| window.hit(cursor, &game.inventory.items).is_some()) => {
                let hit = game.reform.as_ref().unwrap().hit(cursor, &game.inventory.items).unwrap();
                match game.reform.as_mut().unwrap().click(hit, &game.inventory.items) {
                    Some(reform::Action::Reform { item, index }) => {
                        let _ = game.session.reform(item, index);
                    }
                    // Until the server hears it, it holds the player and refuses NPC clicks.
                    Some(reform::Action::Close) => {
                        let _ = game.session.close_reform();
                        game.reform = None;
                    }
                    None => {}
                }
            }
            Scene::Game(game) if game.enchant.as_ref().is_some_and(|window| window.hit(cursor).is_some()) => {
                let zeny = game.status.get(&net::STATUS_ZENY).copied().unwrap_or(0).max(0) as u32;
                let action = {
                    let carried = &game.inventory.items;
                    let count = |id: u32| carried.iter().filter(|item| item.id == id).map(|item| u32::from(item.amount)).sum::<u32>();
                    let index = game.enchant.as_ref().and_then(|window| window.item);
                    let item = index.and_then(|index| carried.iter().find(|item| item.index == index)).cloned();
                    let window = game.enchant.as_mut().unwrap();
                    let hit = window.hit(cursor).unwrap();
                    window.click(hit, item.as_ref(), count, zeny)
                };
                match action {
                    Some(enchant::Action::Send(request)) => {
                        let result = match request {
                            enchant::Request::Random { table, index } => game.session.enchant_random(table, index),
                            enchant::Request::Perfect { table, index, item } => game.session.enchant_perfect(table, index, item),
                            enchant::Request::Upgrade { table, index, slot } => game.session.enchant_upgrade(table, index, slot),
                            enchant::Request::Reset { table, index } => game.session.enchant_reset(table, index),
                        };
                        // Nothing went out, so the window must not sit waiting for an answer that never comes.
                        if result.is_err()
                            && let Some(window) = &mut game.enchant
                        {
                            window.forget_request();
                        }
                    }
                    Some(enchant::Action::Refuse(id)) => game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ERROR),
                    // Until the server hears the close, it holds the player and ignores the next request.
                    Some(enchant::Action::Close) => {
                        let _ = game.session.close_enchant();
                        game.enchant = None;
                    }
                    None => {}
                }
            }
            Scene::Game(game)
                if game.stylist.as_ref().is_some_and(|window| {
                    let job = game.player.look.as_ref().map(|look| look.job).unwrap_or(game.me.job);
                    window.hit(cursor, &self.styling_shop, job).is_some()
                }) =>
            {
                let job = game.player.look.as_ref().map(|look| look.job).unwrap_or(game.me.job);
                let hair = game.player.look.as_ref().map(|look| look.hair).unwrap_or(0);
                let zeny = game.status.get(&net::STATUS_ZENY).copied().unwrap_or(0);
                let carried = &game.inventory.items;
                let has = |id: u32| carried.iter().filter(|item| item.id == id).map(|item| u32::from(item.amount)).sum::<u32>();
                let window = game.stylist.as_mut().unwrap();
                let hit = window.hit(cursor, &self.styling_shop, job).unwrap();
                match window.click(hit, &self.styling_shop, job, hair, zeny, has) {
                    Some(stylist::Action::Buy(picks)) => {
                        let _ = game.session.buy_style(picks);
                    }
                    Some(stylist::Action::Refuse(id)) => game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ERROR),
                    Some(stylist::Action::Close) => {
                        let _ = game.session.close_stylist();
                        game.stylist = None;
                    }
                    None => {}
                }
                let look = game.player.look.clone().unwrap_or_default();
                if let Some(window) = &mut game.stylist {
                    dress_stylist(window, &self.grf, &self.look_tables, &look, &self.items, &self.styling_shop);
                }
            }
            Scene::Game(game) if game.refine.as_ref().is_some_and(|window| window.hit(cursor).is_some()) => {
                let zeny = game.status.get(&net::STATUS_ZENY).copied().unwrap_or(0).max(0) as u32;
                let carried = &game.inventory.items;
                let count = |id: u32| carried.iter().filter(|item| item.id == id).map(|item| u32::from(item.amount)).sum::<u32>();
                let window = game.refine.as_mut().unwrap();
                let hit = window.hit(cursor).unwrap();
                match window.click(hit, count, zeny) {
                    Some(refine::Action::Refine { index, ore, blessings }) => {
                        let _ = game.session.refine(index, ore, blessings);
                    }
                    Some(refine::Action::Refuse(id)) => game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ERROR),
                    // Until the server hears it, it won't open the window again and holds the player.
                    Some(refine::Action::Close) => {
                        let _ = game.session.close_refine();
                        game.refine = None;
                    }
                    None => {}
                }
            }
            // Grade enchanting: Start sends the try, a refusal is the client's own line, close tells the server.
            Scene::Game(game) if game.grade.as_ref().is_some_and(|window| window.hit(cursor).is_some()) => {
                let zeny = game.status.get(&net::STATUS_ZENY).copied().unwrap_or(0).max(0) as u32;
                let carried = &game.inventory.items;
                let count = |id: u32| carried.iter().filter(|item| item.id == id).map(|item| u32::from(item.amount)).sum::<u32>();
                let window = game.grade.as_mut().unwrap();
                let hit = window.hit(cursor).unwrap();
                match window.click(hit, count, zeny) {
                    Some(grade::Action::Enchant { index, material, steps }) => {
                        let _ = game.session.grade_enchant(index, material, steps);
                    }
                    Some(grade::Action::Refuse(id)) => game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ERROR),
                    Some(grade::Action::Close) => {
                        let _ = game.session.close_grade();
                        game.grade = None;
                    }
                    None => {}
                }
            }
            Scene::Game(game) if game.trade.as_ref().is_some_and(|trade| trade.hit(cursor).is_some()) => {
                let trade = game.trade.as_mut().unwrap();
                let hit = trade.hit(cursor).unwrap();
                match trade.click(hit) {
                    Some(trade::Action::Lock(zeny)) => game.lock_trade(zeny),
                    Some(trade::Action::Exchange) => {
                        let _ = game.session.finish_trade();
                    }
                    // The window stays until the server's 0x00ee.
                    Some(trade::Action::Cancel) => {
                        let _ = game.session.cancel_trade();
                    }
                    None => {}
                }
            }
            // The box asking how many keeps the pointer until it's answered.
            Scene::Game(game) if game.amount.is_some() => {
                if let Some(Some(ok)) = window::Amount::<Asked>::hit(cursor, assets.message.size, assets.ok[0].size) {
                    game.answer_amount(ok, &self.messages);
                }
            }
            Scene::Game(game) if game.pick.as_ref().is_some_and(|(list, _)| list.covers(cursor)) => {
                let (list, picking) = game.pick.as_mut().unwrap();
                let hit = list.hit(cursor).unwrap();
                // Item merge: rows pick and unpick, OK sends the picked stacks and waits for the answer.
                let refused = match (*picking, hit) {
                    (Picking::Merge, pick_list::Hit::Row(row)) => list.toggle(row).err(),
                    (Picking::Merge, pick_list::Hit::Ok) => match list.merge_request() {
                        Ok(indexes) => {
                            let _ = game.session.merge_items(&indexes);
                            None
                        }
                        Err(line) => Some(line),
                    },
                    _ => {
                        if let Some(answer) = list.click(hit) {
                            game.answer_pick(answer);
                        }
                        None
                    }
                };
                if let Some(line) = refused {
                    game.chat_line(self.messages.get(line).cloned().unwrap_or_default(), CHAT_ERROR);
                }
            }
            Scene::Game(game) if game.shop.as_ref().is_some_and(|shop| shop.covers(cursor)) => {
                let shop = game.shop.as_mut().unwrap();
                let hit = shop.hit(cursor).unwrap();
                match shop.click(hit) {
                    // A vendor's deal asks "Confirm Deal" first, its windows closed.
                    Some(shop::Action::Deal) if shop.vendor.is_some() => {
                        if !shop.deal.is_empty() {
                            game.deal_confirm = Some(shop::Confirm::new(game.shop.take().unwrap(), screen()));
                        }
                    }
                    Some(shop::Action::Deal) => {
                        let _ = shop.send(&mut game.session);
                        // An NPC shop closes as the deal goes.
                        if shop.plain() {
                            game.shop = None;
                        }
                    }
                    Some(shop::Action::Close) => game.close_shop(),
                    Some(shop::Action::Ask(row, _)) => game.amount = Some(window::Amount::new(Asked::Shop(row), None)),
                    Some(shop::Action::AskOut(..) | shop::Action::Say(..)) | None => {}
                }
            }
            // A small party window: a click on it was a drag (handled on press).
            Scene::Game(game) if game.party_member_at(cursor).is_some() => {}
            Scene::Game(game) if game.window_at(cursor).is_some() => {
                let id = game.window_at(cursor).unwrap();
                game.raise(id);
                match id {
                    Panel::Cutin => {
                        if game.cutin.as_ref().is_some_and(|cutin| cutin.on_close(cursor, &assets.cutin)) {
                            game.cutin = None;
                        }
                    }
                    Panel::Hotkeys => {
                        let hit = game.hotkeys.hit(cursor).unwrap();
                        game.hotkeys.click(hit);
                        if let hotkeys::Hit::Slot(slot) = hit
                            && game.hotkeys.clicks.double(slot as u16)
                        {
                            game.press_hotkey(slot, &self.messages);
                        }
                    }
                    Panel::BasicInfo => {
                        let hit = game.basic_info.hit(cursor).unwrap();
                        match game.basic_info.click(hit) {
                            Some("equip") => {
                                game.equipment.toggle_general();
                                game.raise(Panel::Equipment);
                            }
                            Some("item") => {
                                game.inventory.frame.toggle();
                                game.raise(Panel::Inventory);
                            }
                            Some("skill") => {
                                game.skill_window.frame.toggle();
                                game.raise(Panel::Skills);
                            }
                            Some("party") => {
                                game.party_window.frame.toggle();
                                game.raise(Panel::Party);
                            }
                            Some("status") => {
                                game.status_window.frame.toggle();
                                game.raise(Panel::Status);
                            }
                            // Without a guild, the Guild Companion.
                            Some("guild") if game.guild.is_none() => game.guild_companion.toggle(screen()),
                            Some("guild") => {
                                game.guild_window.frame.toggle();
                                game.raise(Panel::Guild);
                                if game.guild_window.frame.open {
                                    game.ask_guild_pages();
                                }
                            }
                            // The bank opens when the server says so: it answers in its own words where banking is off.
                            Some("bank") => {
                                let _ = game.session.open_bank(game.account_id);
                            }
                            Some("achievement") => game.achievement_window.frame.toggle(),
                            Some("mail") => {
                                let action = game.mail.toggle();
                                game.mail_action(action);
                            }
                            Some("quest") => {
                                game.quest_window.toggle();
                                game.sync_esc();
                            }
                            // The game menu, as Esc opens it (the dead layout when dead); the shortcut settings.
                            Some("option") => {
                                game.escape = if game.escape.is_some() { None } else { Some(escape::Menu { dead: game.player.dead }) };
                            }
                            Some("keyboard") => {
                                game.shortcuts_window = if game.shortcuts_window.is_some() { None } else { Some(options::ShortcutsWindow::new()) };
                                game.sync_esc();
                            }
                            Some("repute") => {
                                game.toggle_reputation(&self.reputation_tables);
                                game.sync_esc();
                            }
                            Some("navigation") => {
                                game.toggle_navigation();
                                game.sync_esc();
                            }
                            Some("map") => {
                                game.world_map = match (game.world_map.take(), &self.gpu) {
                                    (None, Some(gpu)) => worldmap::WorldMap::open(gpu, &self.grf, &self.worlds.0, self.worlds.1, &map_key(&game.map)),
                                    _ => None,
                                };
                            }
                            _ => {}
                        }
                    }
                    Panel::Inventory => {
                        let hit = game.inventory.hit(cursor).unwrap();
                        // Shift + click puts the item's link on the chat line.
                        if shift
                            && let inventory::Hit::Item(index) = hit
                            && let Some(item) = game.inventory.get(index)
                        {
                            let name = inventory::item_name(&self.items, item.id, item.identified, item.refine);
                            game.chat.type_whole(&item_link(item.id, &name));
                        } else if hit == inventory::Hit::Expand {
                            // The same question a voucher opens comes back from the server.
                            let _ = game.session.ask_expansion();
                        } else if let Some(index) = game.inventory.click(hit) {
                            game.use_item(index, &self.messages);
                        }
                    }
                    Panel::Equipment => {
                        let hit = game.equipment.hit(cursor, &game.inventory.items).unwrap();
                        // The box changes when the server answers.
                        if hit == equipment::Hit::ShowBox {
                            let _ = game.session.show_equipment(!game.equipment.shown);
                        }
                        // The title changes when the server answers.
                        if let equipment::Hit::WearTitle(title) = hit {
                            let _ = game.session.change_title(title);
                        }
                        if hit == equipment::Hit::ViewCostume {
                            let _ = game.session.show_costumes(!game.equipment.costumes_shown);
                        }
                        if hit == equipment::Hit::TakeOffAll {
                            match game.equipment.unequip_all(&game.inventory.items) {
                                Some(equipment::UnequipAll::General) => drop(game.session.take_off_all()),
                                Some(equipment::UnequipAll::Pieces(pieces)) => {
                                    for index in pieces {
                                        let _ = game.session.take_off(index);
                                    }
                                }
                                None => {}
                            }
                        }
                        // One thing to take off goes at once; a cart and a mount ask which.
                        if hit == equipment::Hit::Dismount {
                            match game.equipment.dismounts()[..] {
                                [(kind, _)] => drop(game.session.uninstall(kind)),
                                ref both => {
                                    let lines =
                                        both.iter().map(|&(kind, id)| Some((self.messages.get(id).cloned().unwrap_or_default(), Choice::Uninstall(kind))));
                                    game.menu = Some(menu::Menu::new(offset(cursor, [1.0, 1.0]), lines.collect(), &self.font));
                                }
                            }
                        }
                        if hit == equipment::Hit::CartItems {
                            game.cart.frame.toggle();
                            game.raise(Panel::Cart);
                        }
                        if hit == equipment::Hit::SwitchAll {
                            match game.equipment.switch_all(&game.inventory.items, control::now()) {
                                Ok(true) => drop(game.session.switch_all()),
                                Ok(false) => {}
                                Err(id) => game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ERROR),
                            }
                        }
                        // In the swap view, a double click takes the item out of the set.
                        match game.equipment.click(hit) {
                            Some(index) if game.equipment.swapping => {
                                game.switch_asked = Some(index);
                                let _ = game.session.switch_remove(index);
                            }
                            Some(_) if trade::refusal(game.trade.is_some(), trade::ItemAction::Unequip).is_some() => {
                                game.chat_line(self.messages.get(trade::NO_USE).cloned().unwrap_or_default(), CHAT_ERROR);
                            }
                            Some(index) if !game.store_open() => drop(game.session.take_off(index)),
                            _ => {}
                        }
                    }
                    Panel::Cart => {
                        let hit = game.cart.hit(cursor).unwrap();
                        game.cart.click(hit);
                    }
                    Panel::Guild => match game.guild.as_ref().and_then(|guild| game.guild_window.hit(cursor, guild)) {
                        Some(guild::Hit::Title(title)) => game.guild_window.frame.click_title(title),
                        // The Announcement tab is the master's, with the notice as it is to start from.
                        Some(guild::Hit::Tab(guild::NOTICE)) if !game.guild.as_ref().is_some_and(|guild| guild.master) => {}
                        Some(guild::Hit::Tab(tab)) => {
                            game.guild_window.show_tab(tab);
                            if tab == guild::NOTICE
                                && let Some(guild) = &game.guild
                            {
                                game.guild_window.notice_draft = guild.notice.clone();
                            }
                            // Each page but the notice is asked for when shown.
                            if tab <= 4 {
                                let _ = game.session.guild_page(tab as u32);
                            }
                        }
                        // Disband: the warning box, and the disband window asking the guild's name.
                        Some(guild::Hit::Disband) => {
                            game.question_box = Some(BoxQuestion::Notice(guild_companion::DISBAND_WARNING));
                            game.guild_companion.open_disband(screen());
                        }
                        Some(guild::Hit::LevelUp(skill)) => {
                            let _ = game.session.skill_up(skill);
                        }
                        // The skills page: a row selects, Use uses the selected learned skill at its level, Close closes.
                        Some(guild::Hit::Skill(id)) => game.guild_window.skill = Some(id),
                        Some(guild::Hit::SkillUse) => {
                            let level = game
                                .guild
                                .as_ref()
                                .and_then(|guild| guild.skills.iter().find(|skill| Some(skill.id) == game.guild_window.skill))
                                .map(|skill| (skill.id, skill.level));
                            if let Some((id, level)) = level.filter(|&(_, level)| level > 0) {
                                game.use_skill(id, level);
                            }
                        }
                        Some(guild::Hit::SkillClose) => game.guild_window.frame.open = false,
                        Some(guild::Hit::Relation(id, antagonist)) => game.guild_window.selected = Some((id, antagonist)),
                        // The master's positions page: edits wait for OK, which sends them all; a row selects.
                        Some(hit @ (guild::Hit::Permission(..) | guild::Hit::PositionTitle(_) | guild::Hit::Tax(_) | guild::Hit::ListRow(_))) => {
                            if let Some(guild) = &game.guild {
                                game.guild_window.click_position(hit, guild);
                            }
                        }
                        Some(guild::Hit::PositionsOk) => {
                            if let Some(edits) = game.guild_window.take_position_edits(Instant::now()) {
                                let _ = game.session.change_positions(&edits);
                            }
                        }
                        // The Announcement tab: the boxes take the caret; Send writes the notice and closes the window.
                        Some(guild::Hit::NoticeSubject) => game.guild_window.typing = Some(guild::GuildTyping::NoticeSubject),
                        Some(guild::Hit::NoticeText) => game.guild_window.typing = Some(guild::GuildTyping::NoticeText),
                        Some(guild::Hit::NoticeSend) => {
                            if let Some(guild) = &game.guild {
                                let (subject, text) = &game.guild_window.notice_draft;
                                let _ = game.session.set_guild_notice(guild.id, subject, text);
                            }
                            game.guild_window.typing = None;
                            game.guild_window.frame.open = false;
                        }
                        // The members page: select, sort, online first.
                        // The master's moves: OK sends them all at once.
                        Some(guild::Hit::MovesOk) => {
                            if let Some(moves) = game.guild_window.take_moves(Instant::now()) {
                                let _ = game.session.move_members(&moves);
                            }
                        }
                        Some(
                            hit @ (guild::Hit::Member(_)
                            | guild::Hit::SortBy(_)
                            | guild::Hit::LoginStatus
                            | guild::Hit::MemberPage
                            | guild::Hit::Drop(_)
                            | guild::Hit::PickPosition(_)),
                        ) => {
                            if let Some(guild) = &game.guild {
                                game.guild_window.click_members(hit, guild);
                            }
                        }
                        _ => {}
                    },
                    Panel::Clan => {
                        if let Some(clan::Hit::Title(title)) = game.clan.as_ref().and_then(|_| game.clan_window.hit(cursor)) {
                            game.clan_window.frame.click_title(title);
                        }
                    }
                    Panel::Mercenary => match game.mercenary_window.hit(cursor) {
                        Some(mercenary::Hit::Close) => game.mercenary_window.frame.open = false,
                        Some(mercenary::Hit::Skills) => {
                            game.mercenary_skill_list.frame.toggle();
                            game.raise(Panel::MercenarySkills);
                        }
                        Some(mercenary::Hit::Dismiss) => game.dismiss_asked = true,
                        _ => {}
                    },
                    // A press on a row selects it; Use uses the selected skill.
                    Panel::MercenarySkills => match game.mercenary_skill_list.hit(cursor, &game.mercenary_skills, 0) {
                        Some(homunculus::SkillHit::Close) => game.mercenary_skill_list.frame.open = false,
                        Some(homunculus::SkillHit::Row(row)) => game.mercenary_skill_list.selected = game.mercenary_skills.get(row).map(|skill| skill.id),
                        Some(homunculus::SkillHit::Use) => {
                            let selected = game.mercenary_skill_list.selected;
                            if let Some(skill) = game.mercenary_skills.iter().find(|skill| Some(skill.id) == selected && skill.level > 0) {
                                let id = skill.id;
                                game.use_mercenary_skill(id, u16::MAX);
                            }
                        }
                        _ => {}
                    },
                    Panel::Homunculus => match game.homunculus.as_ref().and_then(|_| game.homunculus_window.hit(cursor)) {
                        Some(homunculus::Hit::Close) => game.homunculus_window.frame.open = false,
                        Some(homunculus::Hit::Feed) => game.homunculus_asked = Some(HomunculusAsked::Feed),
                        Some(homunculus::Hit::Delete) => game.homunculus_asked = Some(HomunculusAsked::Delete),
                        Some(homunculus::Hit::Skills) => {
                            game.homunculus_skill_list.frame.toggle();
                            game.raise(Panel::HomunculusSkills);
                        }
                        Some(homunculus::Hit::Rename) => game.ask_homunculus_rename(),
                        Some(homunculus::Hit::NameBox) => {
                            let name = game.homunculus.as_ref().map(|homunculus| game.homunculus_window.name(homunculus).to_owned()).unwrap_or_default();
                            game.homunculus_window.name_text = Some(name);
                            game.homunculus_window.typing = true;
                        }
                        // The box follows the server's answer.
                        Some(homunculus::Hit::AutoFeed) => drop(game.session.homunculus_auto_feed(!game.homunculus_window.auto_feed)),
                        Some(homunculus::Hit::Window) | None => {}
                    },
                    Panel::HomunculusSkills => {
                        let points = game.homunculus.as_ref().map_or(0, |homunculus| homunculus.skill_points);
                        match game.homunculus_skill_list.hit(cursor, &game.homunculus_skills, points) {
                            Some(homunculus::SkillHit::Close) => game.homunculus_skill_list.frame.open = false,
                            Some(homunculus::SkillHit::Row(row)) => {
                                game.homunculus_skill_list.selected = game.homunculus_skills.get(row).map(|skill| skill.id);
                            }
                            Some(homunculus::SkillHit::Use) => {
                                let selected = game.homunculus_skill_list.selected;
                                if let Some(skill) = game.homunculus_skills.iter().find(|skill| Some(skill.id) == selected && skill.level > 0) {
                                    let id = skill.id;
                                    game.use_skill(id, u16::MAX);
                                }
                            }
                            Some(homunculus::SkillHit::Raise(id)) => {
                                let _ = game.session.skill_up(id);
                            }
                            _ => {}
                        }
                    }
                    Panel::Pet => {
                        let hit = game.pet.as_ref().and_then(|_| game.pet_window.hit(cursor));
                        // Any click but the arrow's shuts the command list.
                        if hit != Some(pet::Hit::OpenList) {
                            game.pet_window.list_open = false;
                        }
                        match hit {
                            Some(pet::Hit::Close) => game.pet_window.frame.open = false,
                            Some(pet::Hit::Rename) => game.ask_rename(),
                            Some(pet::Hit::NameBox) => {
                                let name = game.pet.as_ref().map(|pet| game.pet_window.name(pet).to_owned()).unwrap_or_default();
                                game.pet_window.name_text = Some(name);
                                game.pet_window.typing = true;
                            }
                            Some(pet::Hit::OpenList) => game.pet_window.list_open = !game.pet_window.list_open,
                            Some(pet::Hit::Command(row)) => match pet::LIST.get(row).map(|&(_, command)| command) {
                                Some(net::PET_FEED) => game.ask_feed(&self.messages),
                                Some(command) => game.pet_command(command),
                                None => {}
                            },
                            // The box follows the server's answer.
                            Some(pet::Hit::AutoFeed) => drop(game.session.pet_auto_feed(!game.pet_window.auto_feed)),
                            Some(pet::Hit::Window) | None => {}
                        }
                    }
                    Panel::Status => {
                        let hit = game.status_window.hit(cursor, &game.status);
                        if let Some(stat) = hit.and_then(|hit| game.status_window.click(hit)) {
                            // A trait (4th jobs) has its own request.
                            if (status::TRAIT_POW..status::TRAIT_POW + 6).contains(&stat) {
                                let _ = game.session.raise_trait(stat);
                            } else {
                                let _ = game.session.raise_stat(stat);
                            }
                        }
                    }
                    Panel::Party => match game.party_window.hit(cursor, game.party.as_ref(), &game.friends) {
                        Some(party::Hit::Title(title)) => game.party_window.click_title(title),
                        Some(party::Hit::Tab(friends)) => {
                            game.party_window.friends = friends;
                            game.party_window.picked = None;
                        }
                        Some(party::Hit::Friend(row)) => game.party_window.picked = Some(row),
                        // A friend picked: whisper them (the name goes into the chat's name box), or drop them.
                        Some(party::Hit::Button(party::Button::Whisper)) => {
                            if let Some(friend) = game.party_window.picked.and_then(|row| game.friends.get(row)) {
                                game.chat.whisper = friend.name.clone();
                                game.chat.naming = false;
                            }
                        }
                        Some(party::Hit::Button(party::Button::Delete)) => {
                            if let Some(friend) = game.party_window.picked.and_then(|row| game.friends.get(row)) {
                                let _ = game.session.remove_friend(friend.account, friend.character);
                            }
                        }
                        // With the lock on, only the lock and the agency take a press.
                        Some(party::Hit::Button(button)) if !game.party_window.takes(button) => {}
                        Some(party::Hit::Button(party::Button::Lock(locked))) => game.party_window.locked = !locked,
                        Some(party::Hit::Button(party::Button::Sort)) => game.member_windows.sort(&party::Grid::new(screen())),
                        // ponytail: the agency does nothing yet.
                        Some(party::Hit::Button(party::Button::Agency)) => {}
                        Some(party::Hit::Button(party::Button::Create)) => game.toggle_party_dialog(party::DialogKind::Create),
                        // The leader's press opens or closes the invite window; anyone else is told only the leader invites.
                        Some(party::Hit::Button(party::Button::Invite)) => {
                            if game.party.as_ref().and_then(party::Party::leader) != Some(game.account_id) {
                                game.question_box = Some(BoxQuestion::OnlyLeader);
                            } else {
                                game.toggle_party_dialog(party::DialogKind::Invite);
                            }
                        }
                        Some(party::Hit::Button(party::Button::Leave)) => game.question_box = Some(BoxQuestion::Leave),
                        Some(party::Hit::Button(party::Button::Setup)) => {
                            if game.party.is_some() {
                                game.toggle_party_dialog(party::DialogKind::Setup);
                            }
                        }
                        Some(party::Hit::Button(party::Button::FriendSetup)) => game.open_friend_setup(),
                        _ => {}
                    },
                    Panel::Storage => {
                        let names = (&self.messages[..], &self.card_names, &game.char_names);
                        let hit = game.storage.hit(cursor, &assets.storage, &self.items, names);
                        if hit.is_some_and(|hit| game.storage.click(hit, &self.items, names)) {
                            let _ = game.session.close_storage();
                        }
                        // The search button with text: the Search window comes up with it, in front.
                        if hit == Some(storage::Hit::Find) && game.storage.search() {
                            game.raise(Panel::StorageSearch);
                        }
                        game.sync_esc();
                    }
                    // Their presses act on the press.
                    Panel::StorageCategory(_) | Panel::StorageSearch => {}
                    Panel::Skills => {
                        let jobs = skills::tabs(&self.skill_trees, game.me.job, &game.skills);
                        let points = game.skill_points();
                        let skill_info = game.skill_info.clone();
                        let sheet = skills::Sheet { trees: &self.skill_trees, jobs: &jobs, skills: &game.skills.clone(), info: &skill_info, points };
                        let hit = game.skill_window.hit(cursor, &sheet);
                        match hit.and_then(|hit| game.skill_window.click(hit)) {
                            Some(skills::Action::Use(id)) => {
                                let learned = game.skills.iter().find(|skill| skill.id == id).map_or(0, |skill| skill.level);
                                let level = if learned > 0 { game.skill_window.use_level(id, learned) } else { 0 };
                                if level > 0 {
                                    game.use_skill(id, level);
                                }
                            }
                            // A click (let go, not dragged off): a level goes in pending, with what it needs first; a
                            // prerequisite the points can't reach is named in red.
                            Some(skills::Action::LevelUp(id)) => {
                                if let Err(short) = game.skill_window.raise(&sheet, id) {
                                    let name = skill_info.get(&short).map_or_else(|| short.to_string(), |info| info.name.clone());
                                    let line = rustnarok_client::messages::fill(self.messages.get(skills::CANNOT_LEARN).map_or("", String::as_str), &[&name]);
                                    game.chat_line(line, CHAT_ERROR);
                                }
                            }
                            Some(skills::Action::Apply) => game.apply_asked = true,
                            Some(skills::Action::View(simple)) => skills::save_view(simple),
                            None => {}
                        }
                    }
                }
            }
            // The chat tab settings: a kind of line on or off for the tab.
            Scene::Game(game) if game.chat.settings.as_ref().and_then(|settings| settings.hit(cursor)).is_some() => {
                match game.chat.settings.as_ref().and_then(|settings| settings.hit(cursor)).unwrap() {
                    chat::SettingsHit::Title(TitleHit::Close) => game.chat.settings = None,
                    chat::SettingsHit::Title(title) => game.chat.settings.as_mut().unwrap().frame.click_title(title),
                    chat::SettingsHit::Kind(kind) => game.chat.toggle_kind(kind),
                    chat::SettingsHit::Window => {}
                }
            }
            // A click on the chat window: the battle-mode icon opens or closes the line (the focus stays); the rest
            // focuses the boxes but doesn't open the line.
            Scene::Game(game) if game.chat.on_list(cursor) || game.chat.blocks(cursor) => {
                if game.chat.hit(cursor) == Some(chat::Hit::ModeIcon) {
                    game.chat_mode.toggle_line();
                    game.chat.mode_icon = game.chat_mode.icon();
                } else if let Some(id) = game.chat.click(cursor) {
                    game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ERROR);
                }
            }
            Scene::Game(game) if game.dialog.is_some() && (game.on_npc_window(cursor) || game.talk_blocks()) => {
                // NPC windows take the click; the map doesn't react while talking.
                let dialog = game.dialog.as_mut().unwrap();
                if dialog.input.as_ref().is_some_and(|input| hit(input.ok_on(screen()), &assets.ok)) {
                    game.submit_input();
                } else if dialog.button != DialogButton::None && dialog.warp.is_none() && game.say.on_button(cursor) {
                    game.press_dialog_button(true);
                } else if let Some(menu) = &dialog.menu {
                    // Rows go on the press; the buttons on the click.
                    let [ok, cancel] = npc_menu::buttons(menu.items.len()).map(|place| game.npc_menu.at(place));
                    if hit(ok, &assets.ok) {
                        game.menu_ok();
                    } else if hit(cancel, &assets.cancel) {
                        game.choose(true);
                    }
                }
            }
            // The slot machine: the first press on it makes the try.
            Scene::Game(game) if game.slot.is_some() && contains(taming::place(screen()), taming::SIZE, cursor) => {
                let machine = game.slot.as_mut().unwrap();
                if machine.press(Instant::now()) {
                    let _ = game.session.try_capture(machine.monster);
                }
            }
            // Taming: a press on a unit opens the slot machine for it (rathena checks the rest); a player or our own pet,
            // nothing, and it keeps aiming.
            Scene::Game(game) if game.capturing => {
                if let Some(Owner::Unit(id)) = hovered_unit(&game.hit_boxes, cursor, false)
                    && let Some(actor) = game.units.get(&id)
                    && taming::tameable(actor.kind == PLAYER, id, game.pet_id)
                {
                    game.capturing = false;
                    game.slot = Some(taming::Machine::new(id, Instant::now()));
                }
            }
            Scene::Game(game) => {
                if game.store_open() || game.trade_or_storage() {
                    return;
                }
                // Whatever is under the cursor (see `picking::pick`): talk to NPCs, attack monsters, pick items up. The
                // ground walked when the button went down (`press`).
                match game.world_click(cursor, shift) {
                    // Started on the press.
                    WorldClick::Attack(_) => {}
                    WorldClick::Swing(id) => drop(game.session.act(id, net::ACT_ATTACK)),
                    // The rest went with the press.
                    WorldClick::Talk(_) | WorldClick::PickUp(_) | WorldClick::Warp | WorldClick::Ground => {}
                }
            }
            Scene::Message(..) => {
                if hit(offset(dialog_position(&assets.message), RIGHT_BUTTON), &assets.ok) {
                    self.close_message();
                }
            }
            Scene::Confirm(..) | Scene::Prompt { .. } => {
                let dialog = dialog_position(&assets.message);
                if hit(offset(dialog, LEFT_BUTTON), &assets.ok) {
                    self.answer();
                } else if hit(offset(dialog, RIGHT_BUTTON), &assets.cancel) {
                    self.dismiss();
                }
            }
            // The wait box's Cancel.
            Scene::Waiting(wait, _) => {
                if wait.message().is_some() && hit(offset(login_position(), WAIT_CANCEL), &assets.cancel) {
                    self.give_up();
                }
            }
            Scene::Quit => {}
        }
    }

    /// The pointer moved: windows and items being dragged follow it.
    pub(crate) fn pointer_moved(&mut self) {
        if let Scene::Game(game) = &mut self.scene {
            let cursor = self.cursor;
            if let Some((_, from, moved)) = &mut game.press_mark {
                *moved |= left_pixel(*from, cursor);
            }
            if game.right_press.is_some_and(|(from, _)| moved_off(from, cursor)) {
                game.right_moved = true;
            }
            let (inventory_height, equipment_height, info_height) = (game.inventory.height(), game.equipment.height(), game.basic_info.height());
            game.basic_info.frame.drag_to(self.cursor, info_height);
            game.chat.drag_to(self.cursor);
            let hotkeys_height = game.hotkeys.height();
            game.hotkeys.frame.drag_to(self.cursor, hotkeys_height);
            game.inventory.frame.drag_to(self.cursor, inventory_height);
            game.inventory.resize_to(self.cursor);
            game.cart.resize_to(self.cursor);
            game.storage.resize_to(self.cursor);
            for category in &mut game.storage.categories {
                category.resize_to(self.cursor);
                let height = category.height();
                category.frame.drag_to(self.cursor, height);
            }
            game.skill_window.resize_to(self.cursor);
            if let Some(setup) = &mut game.vend_setup
                && setup.resize_to(self.cursor)
            {
                game.vend_size = setup.size;
            }
            if let Some(shop) = &mut game.my_shop
                && shop.resize_to(self.cursor)
            {
                game.vend_size = shop.size;
            }
            game.equipment.frame.drag_to(self.cursor, equipment_height);
            let skills_height = game.skill_window.height();
            game.skill_window.frame.drag_to(self.cursor, skills_height);
            let storage_height = game.storage.height();
            game.storage.frame.drag_to(self.cursor, storage_height);
            if let Some(trade) = &mut game.trade {
                let height = trade.height();
                trade.frame.drag_to(self.cursor, height);
            }
            if let Some(shop) = &mut game.shop {
                shop.drag_to(self.cursor);
            }
            let party_height = game.party_window.height();
            game.party_window.frame.drag_to(self.cursor, party_height);
            game.member_windows.drag_to(self.cursor);
            let status_height = game.status_height;
            game.status_window.frame.drag_to(self.cursor, status_height);
            let guild_height = game.guild_window.height();
            game.guild_window.frame.drag_to(self.cursor, guild_height);
            let clan_height = game.clan_window.height();
            game.clan_window.frame.drag_to(self.cursor, clan_height);
            let cart_height = game.cart.height();
            game.cart.frame.drag_to(self.cursor, cart_height);
            game.pet_window.frame.drag_to(self.cursor, pet::PetWindow::height());
            game.mercenary_window.frame.drag_to(self.cursor, mercenary::Window::height());
            game.mercenary_skill_list.drag_to(self.cursor, game.mercenary_skills.len());
            game.homunculus_window.frame.drag_to(self.cursor, homunculus::Window::height());
            game.homunculus_skill_list.drag_to(self.cursor, game.homunculus_skills.len());
            game.talks.drag_to(self.cursor);
            if let Some(setup) = &mut game.friend_setup {
                setup.frame.drag_to(self.cursor, whisper::FriendSetup::height());
            }
            if let Some(list) = &mut game.block_list {
                let height = list.height();
                list.frame.drag_to(self.cursor, height);
            }
            if let Some(view) = &mut game.card_view {
                view.drag_to(self.cursor);
            }
            if let Some(window) = &mut game.item_window {
                window.drag_to(self.cursor);
            }
            if let Some(window) = &mut game.card_window {
                window.drag_to(self.cursor);
            }
            if let Some(room) = &mut game.room {
                let height = room.height();
                room.frame.drag_to(self.cursor, height);
            }
            game.mail.drag_to(self.cursor);
            game.quest_window.frame.drag_to(self.cursor, 466.0);
            game.quest_window.detail.drag_to(self.cursor, quest::DETAIL_HEIGHT);
            game.minimap_window.frame.drag_to(self.cursor, minimap_window::HEIGHT);
            game.say.frame.drag_to(self.cursor, npc_say::SIZE[1]);
            if let Some(cutin) = &mut game.cutin {
                let height = cutin.height();
                cutin.frame.drag_to(self.cursor, height);
            }
            let menu_height = game.dialog.as_ref().and_then(|dialog| dialog.menu.as_ref()).map_or(0.0, |menu| menu.size()[1]);
            game.npc_menu.drag_to(self.cursor, menu_height);
            game.achievement_window.frame.drag_to(self.cursor, achievement::HEIGHT);
            if let Some(window) = &mut game.roulette {
                window.frame.drag_to(self.cursor, roulette::HEIGHT);
            }
            if let Some(window) = &mut game.grade {
                window.frame.drag_to(self.cursor, grade::HEIGHT);
            }
            if let Some(window) = &mut game.synthesis {
                window.frame.drag_to(self.cursor, laphine::Synthesis::height());
            }
            if let Some(window) = &mut game.upgrade {
                window.frame.drag_to(self.cursor, laphine::Upgrade::height());
            }
            if let Some(window) = &mut game.reform {
                window.frame.drag_to(self.cursor, reform::HEIGHT);
            }
            if let Some(window) = &mut game.enchant {
                window.frame.drag_to(self.cursor, enchant::HEIGHT);
            }
            if let Some(window) = &mut game.monster_window {
                window.frame.drag_to(self.cursor, monster_info::MonsterWindow::height());
            }
            let memorial_height = game.memorial.height();
            game.memorial.frame.drag_to(self.cursor, memorial_height);
            if let Some(log) = &mut game.storage_log {
                log.drag_to(self.cursor);
            }
            if let Some(window) = &mut game.stylist {
                window.frame.drag_to(self.cursor, stylist::HEIGHT);
            }
            if let Some(window) = &mut game.audio_window
                && window.drag_to(self.cursor, &mut self.sound)
                && let Some(audio) = &mut self.audio
            {
                audio.set(&self.sound);
            }
            if let Some(window) = &mut game.graphics_window {
                window.drag_to(self.cursor);
            }
            if let Some(window) = &mut game.shortcuts_window {
                window.drag_to(self.cursor);
            }
            game.camera_goal.pointer_moved(cursor);
            game.camera_goal.drag(self.cursor, self.modifiers);
            if let Some((_, start, moved)) = &mut game.dragging {
                *moved |= (self.cursor[0] - start[0]).abs() + (self.cursor[1] - start[1]).abs() > 4.0;
            }
        }
    }

    /// The right button: Alt moves stacks to storage, a click on a player opens their menu, a press aims the camera.
    pub(crate) fn right_button(&mut self, state: ElementState) {
        self.right_down = state == ElementState::Pressed;
        // A right press lets go of a held drag and closes the box asking how many of an item.
        if let Scene::Game(game) = &mut self.scene
            && state == ElementState::Pressed
        {
            let dropped = game.dragging.take().is_some();
            game.drag_cancelled |= dropped;
            let closed = game.amount.take_if(|amount| amount.what.moves_items()).is_some();
            if dropped || closed {
                return;
            }
        }
        // Alt + right press orders the homunculus (when it's out), off the windows.
        if let (Scene::Game(game), Some(assets)) = (&mut self.scene, &self.assets)
            && state == ElementState::Pressed
            && self.modifiers.alt_key()
            && !game.on_ui(self.cursor, assets.message.size)
            && game.companion_press(true, self.cursor, self.modifiers.shift_key())
        {
            return;
        }
        // On the character select, a card's menu of moving and renaming (only once the PIN check has passed).
        if let Scene::CharSelect(select) = &mut self.scene
            && state == ElementState::Pressed
            && select.unlocked()
        {
            select.open_menu(self.cursor, &self.font, &self.messages);
        }
        // The room owner's right click on another member's name: hand the room over, or kick them out.
        if let Scene::Game(game) = &mut self.scene
            && state == ElementState::Released
            && let Some((start, None)) = game.right_press
            && (start[0] - self.cursor[0]).abs() + (start[1] - self.cursor[1]).abs() < 4.0
            && let Some(room) = &game.room
            && let Some(chatroom::RoomHit::Member(row)) = room.hit(self.cursor, &game.me.name)
            && room.member_choices(row, &game.me.name).is_some()
        {
            game.right_press = None;
            let label = |message: usize| self.messages.get(message).cloned().unwrap_or_default();
            let choices = vec![Some((label(ROOM_HAND_OVER), Choice::RoomOwner(row))), Some((label(ROOM_KICK), Choice::RoomKick(row)))];
            game.menu = Some(menu::Menu::new(self.cursor, choices, &self.font));
            return;
        }
        // A right click (not a drag) on a member's row in the party window, or on their small window, unlocked: the
        // member's menu.
        if let Scene::Game(game) = &mut self.scene
            && state == ElementState::Released
            && let Some((start, None)) = game.right_press
            && (start[0] - self.cursor[0]).abs() + (start[1] - self.cursor[1]).abs() < 4.0
            && !game.party_window.locked
        {
            let small = game.party_member_at(self.cursor);
            let row = match game.party_window.hit(self.cursor, game.party.as_ref(), &game.friends) {
                Some(party::Hit::Member(account)) if game.window_at(self.cursor) == Some(Panel::Party) => Some(account),
                _ => None,
            };
            if let Some((account, small)) = small.map(|account| (account, true)).or(row.map(|account| (account, false))) {
                game.right_press = None;
                game.member_menu(account, small, self.cursor, &self.messages, &self.font);
                return;
            }
        }
        // A right click on a tracked quest's title: its details, or putting it aside.
        if let Scene::Game(game) = &mut self.scene
            && state == ElementState::Released
            && let Some((start, None)) = game.right_press
            && (start[0] - self.cursor[0]).abs() + (start[1] - self.cursor[1]).abs() < 4.0
            && game.quest_window.tracker
            && game.window_at(self.cursor).is_none()
            && let Some(id) = quest_tracker::title_at(
                &quest_tracker::blocks(&game.quests, &self.quest_texts, &game.quest_window.hidden, &self.messages),
                &self.font,
                self.cursor,
            )
        {
            game.right_press = None;
            let label = |message: usize| self.messages.get(message).cloned().unwrap_or_default();
            let choices = vec![Some((label(QUEST_INFO_CHOICE), Choice::QuestInfo(id))), Some((label(QUEST_DELETE_CHOICE), Choice::QuestAside(id)))];
            game.menu = Some(menu::Menu::new(self.cursor, choices, &self.font));
            return;
        }
        // A right click on a skill in the skill window or the guild window's skills: its description (the hotkey bar's
        // opens on the press), the same skill again closes it. Only the guild's has a close button; Esc closes either.
        if let Scene::Game(game) = &mut self.scene
            && state == ElementState::Released
            && let Some((start, None)) = game.right_press
            && (start[0] - self.cursor[0]).abs() + (start[1] - self.cursor[1]).abs() < 4.0
        {
            let cursor = self.cursor;
            let opened = match game.window_at(cursor) {
                Some(Panel::Skills) => {
                    let jobs = skills::tabs(&self.skill_trees, game.me.job, &game.skills);
                    let sheet =
                        skills::Sheet { trees: &self.skill_trees, jobs: &jobs, skills: &game.skills, info: &game.skill_info, points: game.skill_points() };
                    // The skill's prerequisites stay lit until another right click.
                    let skill = match game.skill_window.hit(cursor, &sheet) {
                        Some(skills::Hit::Skill(id)) => Some(id),
                        _ => None,
                    };
                    game.skill_window.pin(skill);
                    skill.map(|id| (id, false))
                }
                // A guild skill: a one-entry menu, "View Skill Info".
                Some(Panel::Guild) => {
                    if let Some(guild::Hit::Skill(id)) = game.guild.as_ref().and_then(|guild| game.guild_window.hit(cursor, guild)) {
                        game.right_press = None;
                        let label = self.messages.get(guild::VIEW_SKILL_INFO).cloned().unwrap_or_default();
                        game.menu = Some(menu::Menu::new(cursor, vec![Some((label, Choice::GuildSkillInfo(id)))], &self.font));
                        return;
                    }
                    None
                }
                _ => None,
            };
            if let Some((id, closable)) = opened {
                game.right_press = None;
                if game.skill_description.as_ref().is_some_and(|window| window.skill == id) {
                    game.skill_description = None;
                } else {
                    let lines = self.skill_descriptions.get(&id).cloned().unwrap_or_default();
                    let active = game.skill_active(id);
                    game.skill_description =
                        Some(skill_descript::SkillDescription::new(id, &lines, closable, active, |height| skill_descript::at_window(cursor, height)));
                }
                game.sync_esc();
                return;
            }
        }
        // A right click on a row of the guild's members page: the master moves someone else, a member leaves.
        if let Scene::Game(game) = &mut self.scene
            && state == ElementState::Released
            && let Some((start, None)) = game.right_press
            && (start[0] - self.cursor[0]).abs() + (start[1] - self.cursor[1]).abs() < 4.0
            && game.window_at(self.cursor) == Some(Panel::Guild)
            && let Some(guild) = &game.guild
            && game.guild_window.tab == guild::MEMBERS
            && let Some(index) = game.guild_window.member.and_then(|character| guild.members.iter().position(|member| member.character == character))
        {
            // The menu is for the row selected by the last left press.
            let name = guild.members[index].name.clone();
            let invite_party = game
                .party
                .as_ref()
                .is_some_and(|party| party.leader() == Some(game.account_id) && !party.members.iter().any(|member| member.info.name == name));
            let choices = guild.member_menu(game.me.id, index, invite_party);
            if !choices.is_empty() {
                game.right_press = None;
                let entries = choices
                    .into_iter()
                    .map(|choice| Some((self.messages.get(choice.message()).cloned().unwrap_or_default(), Choice::GuildMember(choice, index))))
                    .collect();
                game.menu = Some(menu::Menu::new(self.cursor, entries, &self.font));
                return;
            }
        }
        // The master's right click on an ally or antagonist: delete it.
        if let Scene::Game(game) = &mut self.scene
            && state == ElementState::Released
            && let Some((start, None)) = game.right_press
            && (start[0] - self.cursor[0]).abs() + (start[1] - self.cursor[1]).abs() < 4.0
            && game.window_at(self.cursor) == Some(Panel::Guild)
            && let Some(guild) = &game.guild
            && guild.master
            && let Some(guild::Hit::Relation(id, antagonist)) = game.guild_window.hit(self.cursor, guild)
        {
            game.right_press = None;
            let label = self.messages.get(RELATION_DELETE).cloned().unwrap_or_default();
            game.menu = Some(menu::Menu::new(self.cursor, vec![Some((label, Choice::EndRelation(id, antagonist)))], &self.font));
            return;
        }
        if let Scene::Game(game) = &mut self.scene {
            let assets = self.assets.as_ref();
            // A right press lets go of a skill being aimed.
            if state == ElementState::Pressed {
                game.targeting = None;
            }
            // A right press on an NPC talks too.
            if state == ElementState::Pressed
                && let Some(assets) = assets
                && !game.on_ui(self.cursor, assets.message.size)
                && let Some(Owner::Unit(id)) = hovered_unit(&game.hit_boxes, self.cursor, false)
                && game.units.get(&id).is_some_and(|actor| actor.kind == NPC && actor.job != WARP_JOB)
            {
                game.talk_to(id, &self.messages);
            }
            // A right click on an inventory item with a modifier: Alt sends the stack to what's open, Shift moves it
            // into the favourites or out, Ctrl identifies it with a Magnifier.
            let (alt, shift, ctrl) = (self.modifiers.alt_key(), self.modifiers.shift_key(), self.modifiers.control_key());
            if state == ElementState::Pressed
                && game.window_at(self.cursor) == Some(Panel::Inventory)
                && let Some(inventory::Hit::Item(index)) = game.inventory.hit(self.cursor)
                && let Some(item) = game.inventory.get(index).cloned()
                && let Some(action) = inventory_right(alt, shift, ctrl)
            {
                let line = match action {
                    InventoryRight::QuickMove => game.quick_move(index),
                    InventoryRight::Favorite => game.session.move_favorite(index, item.favorite).err().and(None),
                    InventoryRight::Identify => {
                        let magnifier = game.inventory.items.iter().any(|carried| MAGNIFIERS.contains(&carried.id));
                        let (warn, ask) = inventory::ctrl_identify(game.inventory.open_tab(), item.identified, magnifier);
                        if ask {
                            let _ = game.session.identify_one(index);
                        }
                        warn.then_some(NO_MAGNIFIER)
                    }
                };
                if let Some(line) = line {
                    game.chat_line(self.messages.get(line).cloned().unwrap_or_default(), CHAT_ERROR);
                }
                return;
            }
            // A right press on a skill on the hotkey bar: its description under or over that row (the same skill again
            // closes it).
            if state == ElementState::Pressed
                && game.window_at(self.cursor) == Some(Panel::Hotkeys)
                && let Some(hotkeys::Hit::Slot(slot)) = game.hotkeys.hit(self.cursor)
                && let Some(key) = game.hotkeys.keys.get(slot).copied().flatten().filter(|key| key.skill)
                && let Some(row) = game.hotkeys.slot_row(slot)
            {
                let id = key.id as u16;
                if game.skill_description.as_ref().is_some_and(|window| window.skill == id) {
                    game.skill_description = None;
                } else {
                    let lines = self.skill_descriptions.get(&id).cloned().unwrap_or_default();
                    let (pointer_x, bar_y) = (self.cursor[0], game.hotkeys.frame.position[1]);
                    let active = game.skill_active(id);
                    game.skill_description = Some(skill_descript::SkillDescription::new(id, &lines, true, active, |height| {
                        skill_descript::at_hotbar(pointer_x, bar_y, row, height)
                    }));
                }
                game.sync_esc();
                return;
            }
            // Alt + right click takes a whole stack out of the open storage.
            let names = (&self.messages[..], &self.card_names, &game.char_names);
            let moved = match game.window_at(self.cursor) {
                _ if state != ElementState::Pressed || !self.modifiers.alt_key() || !game.storage.frame.open => None,
                Some(Panel::Storage) => match assets.and_then(|assets| game.storage.hit(self.cursor, &assets.storage, &self.items, names)) {
                    Some(storage::Hit::Item(index)) => game.storage.get(index).map(|item| (Asked::Take(index), item.amount)),
                    _ => None,
                },
                Some(panel @ (Panel::StorageCategory(_) | Panel::StorageSearch)) => {
                    match storage_list(panel).and_then(|which| game.storage.category_hit(which, self.cursor, &self.items, names)) {
                        Some(storage::Hit::Item(index)) => game.storage.get(index).map(|item| (Asked::Take(index), item.amount)),
                        _ => None,
                    }
                }
                _ => None,
            };
            // A right click (not a drag) opens the menu for what's under the pointer, by the usual ranks: another
            // player, our pet or homunculus, ourselves with our stall open. A monster or an item over them wins.
            let ourselves =
                || picking::pick(game.hit_boxes.iter().copied().filter(|target| target.owner == Owner::Player), self.cursor, false, screen_middle());
            let picked = hovered(&game.hit_boxes, self.cursor, false).or_else(|| game.my_shop.as_ref().and_then(|_| ourselves()));
            let is_player = |id: u32| game.units.get(&id).is_some_and(|actor| actor.kind == PLAYER);
            let mercenary = game.mercenary.as_ref().map(|hired| hired.id);
            let on_unit = match menu_for(picked, is_player, game.pet_id, game.homunculus_id, mercenary, game.my_shop.is_some()) {
                _ if game.window_at(self.cursor).is_some() || game.chat.blocks(self.cursor) => None,
                target => target,
            };
            if state == ElementState::Pressed {
                game.menu = None;
                game.right_press = Some((self.cursor, on_unit));
                game.right_moved = false;
            } else if let Some((_, Some(target))) = game.right_press.take()
                && menu_opens(game.right_moved, self.modifiers.shift_key(), self.mouse_down, false)
            {
                let name = match target {
                    MenuFor::Player(id) | MenuFor::Pet(id) | MenuFor::Homunculus(id) | MenuFor::Mercenary(id) => {
                        game.units.get(&id).map(|actor| actor.name.clone()).unwrap_or_default()
                    }
                    MenuFor::Stall => game.me.name.clone(),
                };
                let fill = |id: usize| rustnarok_client::messages::fill(self.messages.get(id).map_or("", String::as_str), &[&name]);
                let choices: Vec<(usize, Choice)> = match target {
                    // Our pet: its five commands.
                    MenuFor::Pet(_) => PET_CHOICES.into_iter().map(|(label, command)| (label, Choice::Pet(command))).collect(),
                    MenuFor::Homunculus(_) => HOMUNCULUS_CHOICES.to_vec(),
                    MenuFor::Mercenary(_) => MERCENARY_CHOICES.to_vec(),
                    MenuFor::Stall => vec![(CLOSE_STALL_CHOICE, Choice::CloseStall)],
                    MenuFor::Player(id) => {
                        let guild = game.guild.as_ref().filter(|guild| guild.master);
                        let their_guild = game.units.get(&id).map_or(0, |actor| actor.guild);
                        let in_party = |party: &party::Party| party.members.iter().any(|member| member.info.account == id);
                        // Married (wearing the ring): a party member can be asked to be adopted.
                        let married = game.inventory.items.iter().any(|item| item.worn != 0 && WEDDING_RINGS.contains(&item.id));
                        let ties = PlayerTies {
                            trading: game.trade.is_some(),
                            party_invite: game.party.as_ref().is_some_and(|party| party.leader() == Some(game.account_id) && !in_party(party)),
                            guild_invite: guild::offers_invite(game.guild.as_ref(), their_guild),
                            guild_relation: guild.is_some_and(|guild| their_guild != 0 && their_guild != guild.id),
                            friend: game.friends.iter().any(|friend| friend.account == id),
                            adoptable: married && game.party.as_ref().is_some_and(in_party),
                            in_our_room: game.room.as_ref().and_then(|room| {
                                let row = room.members.iter().position(|(member, _)| *member == name)?;
                                room.member_choices(row, &game.me.name).map(|_| row)
                            }),
                        };
                        player_choices(id, ties)
                    }
                };
                let choices = choices.into_iter().map(|(label, choice)| Some((fill(label), choice))).collect();
                game.menu = Some(menu::Menu::new(self.cursor, choices, &self.font));
            }
            // A right press on a card in a description's strip: the card's own window (the description stays).
            let card =
                if state == ElementState::Pressed { game.item_window.as_ref().and_then(|window| window.card_under(self.cursor, &self.items)) } else { None };
            if let Some((card, enchant)) = card {
                game.show_card(card, enchant);
            }
            // A right click on an item (inventory, storage, what's worn, a shop's row) describes it.
            let shop_row = match game.shop.as_ref().and_then(|shop| shop.hit(self.cursor)) {
                Some(shop::Hit::Row(window, row)) => game.shop.as_ref().and_then(|shop| shop.offer_at(window, row)),
                _ => None,
            };
            // Another player's equipment window, over the others: what they wear too.
            let viewed = game.viewing.as_ref().and_then(|viewed| match viewed.window.hit(self.cursor, &viewed.items) {
                Some(equipment::Hit::Slot(index) | equipment::Hit::Name(index)) => viewed.items.iter().find(|item| item.index == index).cloned(),
                _ => None,
            });
            let names = (&self.messages[..], &self.card_names, &game.char_names);
            let described = viewed.or_else(|| match game.window_at(self.cursor) {
                _ if state != ElementState::Pressed || self.modifiers.alt_key() || card.is_some() => None,
                _ if shop_row.is_some() => shop_row.map(|offer| net::InventoryItem {
                    id: offer.item,
                    kind: offer.kind,
                    amount: 1,
                    identified: offer.identified,
                    refine: offer.refine,
                    ..Default::default()
                }),
                Some(Panel::Inventory) => match game.inventory.hit(self.cursor) {
                    Some(inventory::Hit::Item(index)) => game.inventory.get(index).cloned(),
                    _ => None,
                },
                Some(Panel::Storage) => match assets.and_then(|assets| game.storage.hit(self.cursor, &assets.storage, &self.items, names)) {
                    Some(storage::Hit::Item(index)) => game.storage.get(index).cloned(),
                    _ => None,
                },
                Some(panel @ (Panel::StorageCategory(_) | Panel::StorageSearch)) => {
                    match storage_list(panel).and_then(|which| game.storage.category_hit(which, self.cursor, &self.items, names)) {
                        Some(storage::Hit::Item(index)) => game.storage.get(index).cloned(),
                        _ => None,
                    }
                }
                Some(Panel::Cart) => match game.cart.hit(self.cursor) {
                    Some(inventory::Hit::Item(index)) => game.cart.get(index).cloned(),
                    _ => None,
                },
                Some(Panel::Equipment) => match game.equipment.hit(self.cursor, &game.inventory.items) {
                    Some(equipment::Hit::Slot(index) | equipment::Hit::Name(index)) => game.inventory.get(index).cloned(),
                    _ => None,
                },

                _ => None,
            });
            if let Some(item) = described {
                let shown = iteminfo::Shown::of(&item);
                // The same item again closes its description; another one takes its place.
                if game.item_window.as_ref().is_some_and(|window| window.shown.same_item(&shown)) {
                    game.item_window = None;
                    game.card_view = None;
                } else {
                    game.show_item(shown);
                }
            } else if let Some((asked, amount)) = moved {
                game.give(asked, u32::from(amount));
            } else if state == ElementState::Pressed && (game.targeting.take().is_some() | std::mem::take(&mut game.capturing)) {
            } else if state == ElementState::Pressed {
                game.camera_goal.press(self.cursor, Instant::now(), self.modifiers.control_key(), self.modifiers.shift_key());
            } else {
                game.camera_goal.dragging = None;
            }
        }
    }

    /// The left button went down or up: a press grabs, a release drops or clicks.
    /// The wheel turned `rows` notches (positive: toward the player): it scrolls what the pointer is over, or zooms.
    pub(crate) fn wheel(&mut self, rows: i32) {
        // Over a window the wheel zooms only with Ctrl (and never while aiming).
        if let (Scene::Game(game), Some(assets)) = (&mut self.scene, &self.assets)
            && self.modifiers.control_key()
            && game.targeting.is_none()
            && game.on_ui(self.cursor, assets.message.size)
        {
            game.camera_goal.wheel(rows, Instant::now(), true, self.modifiers.shift_key());
            return;
        }
        if let Scene::Game(game) = &mut self.scene {
            // Over the inventory the wheel scrolls it; elsewhere it zooms (away from you zooms out).
            let on_shop = game.shop.as_ref().is_some_and(|shop| shop.covers(self.cursor));
            let on_pick = game.pick.as_ref().is_some_and(|(list, _)| list.covers(self.cursor));
            let on_item = game.item_window.as_ref().is_some_and(|window| window.hit(self.cursor).is_some());
            let on_card = game.card_window.as_ref().is_some_and(|window| window.hit(self.cursor).is_some());
            let on_setup = game.vend_setup.as_ref().is_some_and(|setup| setup.hit(self.cursor, &game.cart.items).is_some());
            let on_buying_setup = game.buying_setup.as_ref().is_some_and(|setup| setup.hit(self.cursor, &game.inventory.items).is_some());
            let on_cashshop = game.cashshop.as_ref().is_some_and(|shop| shop.hit(self.cursor).is_some());
            let on_synthesis = game.synthesis.as_ref().is_some_and(|window| window.hit(self.cursor).is_some());
            let on_upgrade = game.upgrade.as_ref().is_some_and(|window| window.hit(self.cursor).is_some());
            let on_reform = game.reform.as_ref().is_some_and(|window| window.hit(self.cursor, &game.inventory.items).is_some());
            let on_npc_menu =
                game.dialog.as_ref().and_then(|dialog| dialog.menu.as_ref()).is_some_and(|menu| game.npc_menu.local(self.cursor, menu.size()[1]).is_some());
            match game.window_at(self.cursor) {
                // Aiming, the wheel picks the level (up raises it), wherever the pointer is.
                _ if game.targeting.is_some() => {
                    let (id, level) = game.targeting.unwrap();
                    let separate = self.skill_info.get(&id).is_some_and(|info| info.separate_level);
                    let learned = game.skills.iter().find(|skill| skill.id == id).map_or(level, |skill| skill.level);
                    game.targeting = Some((id, aim_level(level, learned, rows, separate)));
                }
                _ if on_synthesis => game.synthesis.as_mut().unwrap().scroll_by(rows),
                _ if on_upgrade => game.upgrade.as_mut().unwrap().scroll_by(rows),
                _ if on_npc_menu => game.dialog.as_mut().and_then(|dialog| dialog.menu.as_mut()).unwrap().wheel(rows),
                _ if game.dialog.as_ref().is_some_and(|dialog| dialog.warp.is_none()) && game.say.contains(self.cursor) => {
                    game.say.scroll_by(rows as f32 * npc_say::SCROLL_STEP);
                }
                _ if game.enchant.as_ref().is_some_and(|window| window.hit(self.cursor).is_some()) => {
                    let index = game.enchant.as_ref().and_then(|window| window.item);
                    let item = index.and_then(|index| game.inventory.items.iter().find(|item| item.index == index)).cloned();
                    game.enchant.as_mut().unwrap().scroll_by(rows, item.as_ref());
                }
                _ if on_cashshop => game.cashshop.as_mut().unwrap().scroll_at(self.cursor, rows),
                _ if on_reform => game.reform.as_mut().unwrap().scroll_by(rows, &game.inventory.items),
                _ if on_setup => {
                    let count = game.cart.items.len();
                    game.vend_setup.as_mut().unwrap().scroll_by(rows, count);
                }
                _ if on_buying_setup => {
                    let inv = &game.inventory.items;
                    game.buying_setup.as_mut().unwrap().scroll_by(rows, inv);
                }
                _ if game.card_view.as_ref().is_some_and(|view| view.hit(self.cursor).is_some()) => {}
                _ if on_card => {
                    let lines = game.card_lines;
                    game.card_window.as_mut().unwrap().scroll_by(rows, lines);
                }
                _ if on_item => {
                    let lines = game.item_lines;
                    game.item_window.as_mut().unwrap().scroll_by(rows, lines);
                }
                _ if on_shop => game.shop.as_mut().unwrap().scroll_by(self.cursor, rows),
                _ if game.skill_description.as_ref().is_some_and(|window| window.contains(self.cursor)) => {
                    game.skill_description.as_mut().unwrap().scroll_by(rows);
                }
                _ if on_pick => game.pick.as_mut().unwrap().0.scroll_by(rows),
                _ if game.achievement_window.covers(self.cursor) => game.achievement_window.scroll_by(rows, &self.achievement_info),
                _ if game.quest_window.on_details(self.cursor) => game.quest_window.scroll_details(rows, self.cursor, &game.quests, &self.quest_texts),
                _ if game.quest_window.covers(self.cursor) => game.quest_window.scroll_by(rows, &game.quests, &self.quest_texts),
                Some(Panel::Inventory) => game.inventory.wheel(rows),
                Some(Panel::Cart) => game.cart.wheel(rows),
                Some(Panel::HomunculusSkills) => game.homunculus_skill_list.scroll_by(rows, game.homunculus_skills.len()),
                Some(Panel::MercenarySkills) => game.mercenary_skill_list.scroll_by(rows, game.mercenary_skills.len()),
                Some(Panel::Skills) => {
                    let jobs = skills::tabs(&self.skill_trees, game.me.job, &game.skills);
                    let sheet =
                        skills::Sheet { trees: &self.skill_trees, jobs: &jobs, skills: &game.skills, info: &game.skill_info, points: game.skill_points() };
                    game.skill_window.scroll_by(&sheet, rows);
                }
                Some(Panel::Guild) => {
                    if let Some(guild) = &game.guild {
                        game.guild_window.scroll_by(rows, guild);
                    }
                }
                Some(Panel::Storage) => game.storage.scroll_by(rows, &self.items, (&self.messages[..], &self.card_names, &game.char_names)),
                Some(panel @ (Panel::StorageCategory(_) | Panel::StorageSearch)) => {
                    if let Some(which) = storage_list(panel) {
                        game.storage.category_scroll_by(which, rows, &self.items, (&self.messages[..], &self.card_names, &game.char_names));
                    }
                }
                Some(Panel::Party) => {
                    let count = if game.party_window.friends { game.friends.len() } else { game.party.as_ref().map_or(0, |party| party.members.len()) };
                    game.party_window.scroll_by(rows, count);
                }
                Some(_) => {}
                // Three rows a notch.
                None if game.chat.contains(self.cursor) => game.chat.scroll_by(rows * 3),
                None => game.camera_goal.wheel(rows, Instant::now(), self.modifiers.control_key(), self.modifiers.shift_key()),
            }
        }
    }

    pub(crate) fn left_button(&mut self, event_loop: &ActiveEventLoop, state: ElementState) {
        self.mouse_down = state == ElementState::Pressed;
        if let Scene::Game(game) = &mut self.scene
            && state == ElementState::Released
        {
            game.walk_held = false;
            // An attack held down stops when let go, once it went out.
            if let Some(attack) = &mut game.attack
                && attack.release() == Release::Cancel
            {
                game.attack = None;
                let _ = game.session.stop_attack();
            }
        }
        let message_size = self.assets.as_ref().map_or([0.0; 2], |assets| assets.message.size);
        let dragged = match &mut self.scene {
            Scene::Game(game) if state == ElementState::Released => {
                let item = game.dragging.take();
                // An item, a skill or a hotkey let go over a hotkey slot goes there (`hotkeys::drop_on`).
                if let Some((dragged, _, true)) = item
                    && let Some(hotkeys::Hit::Slot(slot)) = game.hotkeys.hit(self.cursor)
                {
                    let key = match dragged {
                        Dragged::Hotkey(from) if from != slot => game.hotkeys.keys[from],
                        Dragged::Item(index) => game.inventory.get(index).map(|item| net::Hotkey { skill: false, id: item.id, count: item.amount as i16 }),
                        // At the level picked in the skill window.
                        Dragged::Skill(id) => game.skills.iter().find(|skill| skill.id == id).map(|skill| net::Hotkey {
                            skill: true,
                            id: u32::from(id),
                            count: game.skill_window.use_level(id, skill.level) as i16,
                        }),
                        Dragged::Stored(_) | Dragged::Carted(_) | Dragged::Hotkey(_) | Dragged::Worn(_) | Dragged::ShopOffer(_) | Dragged::ShopDeal(_) => None,
                    };
                    if let Some(key) = key {
                        for (slot, key) in hotkeys::drop_on(&game.hotkeys.keys, key, slot) {
                            game.hotkeys.keys[slot] = key;
                            let _ = game.session.set_hotkey(game.hotkeys.tab(), slot as u16, key);
                        }
                    }
                }
                // A skill hotkey let go off the slots, on the bar or over the world, is taken off. An item hotkey stays:
                // over the world it can't be dropped. Over another window nothing happens.
                if let Some((Dragged::Hotkey(from), _, true)) = item
                    && let Some(key) = game.hotkeys.keys[from]
                {
                    let on_bar = game.hotkeys.hit(self.cursor);
                    let elsewhere = on_bar.is_none() && game.window_at(self.cursor).is_some();
                    match on_bar {
                        Some(hotkeys::Hit::Slot(_)) => {}
                        _ if elsewhere => {}
                        _ if key.skill => {
                            game.hotkeys.keys[from] = None;
                            let _ = game.session.set_hotkey(game.hotkeys.tab(), from as u16, None);
                        }
                        Some(_) => {}
                        None => game.chat_line(self.messages.get(HOTKEY_ITEM_STAYS).cloned().unwrap_or_default(), CHAT_ERROR),
                    }
                }
                // A shop's offer let go over its deal goes in; a deal row let go over its goods comes out.
                if let Some((Dragged::ShopOffer(row) | Dragged::ShopDeal(row), _, true)) = item
                    && game.trade.is_none()
                    && let Some(shop) = &mut game.shop
                {
                    let zeny = game.status.get(&net::STATUS_ZENY).copied().unwrap_or(0).max(0) as u64;
                    shop.zeny = zeny;
                    let action = match (item.map(|(dragged, ..)| dragged), shop.window_at(self.cursor)) {
                        (Some(Dragged::ShopOffer(_)), Some(1)) => shop.drop_offer(row),
                        (Some(Dragged::ShopDeal(_)), Some(0)) => shop.drop_deal(row),
                        _ => None,
                    };
                    match action {
                        Some(shop::Action::Ask(row, fill)) => game.amount = Some(window::Amount::new(Asked::Shop(row), (fill > 0).then_some(fill))),
                        Some(shop::Action::AskOut(row, fill)) => game.amount = Some(window::Amount::new(Asked::ShopOut(row), Some(fill))),
                        Some(shop::Action::Say(message, arg)) => {
                            let text = self.messages.get(message).map_or("", String::as_str);
                            let line = arg.map_or_else(|| text.to_owned(), |arg| rustnarok_client::messages::fill(text, &[&arg]));
                            game.chat_line(line, CHAT_ERROR);
                        }
                        _ => {}
                    }
                }
                // An item let go over the storage goes in; a stored one let go over the inventory comes out.
                let on_trade = game.trade.as_ref().is_some_and(|trade| trade.hit(self.cursor).is_some());
                let on_mail = matches!(game.mail.hit(self.cursor), Some(mail::Hit::Compose(_)));
                let on_refine = game.refine.as_ref().is_some_and(|window| window.hit(self.cursor).is_some());
                let on_grade = game.grade.as_ref().is_some_and(|window| window.hit(self.cursor).is_some());
                let on_enchant = game.enchant.as_ref().is_some_and(|window| window.hit(self.cursor).is_some());
                let on_materials = game.materials.as_ref().is_some_and(|window| window.hit(self.cursor).is_some());
                let on_synthesis = game.synthesis.as_ref().is_some_and(|window| window.hit(self.cursor).is_some());
                let on_upgrade = game.upgrade.as_ref().is_some_and(|window| window.hit(self.cursor).is_some());
                match (item, game.window_at(self.cursor)) {
                    (Some((Dragged::Item(index), _, true)), _) if on_synthesis || on_upgrade => {
                        if let Some(item) = game.inventory.get(index).cloned() {
                            let said = if on_synthesis {
                                game.synthesis.as_mut().and_then(|window| match window.put(&item) {
                                    laphine::Put::Say(id) => Some(id),
                                    _ => None,
                                })
                            } else {
                                game.upgrade.as_mut().and_then(|window| match window.put(&item) {
                                    laphine::Put::Say(id) => Some(id),
                                    _ => None,
                                })
                            };
                            if let Some(id) = said {
                                game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ERROR);
                            }
                        }
                    }
                    (Some((Dragged::Item(index), _, true)), _) if on_mail => game.move_item(Asked::Mail(index)),
                    (Some((Dragged::Item(index), _, true)), _) if on_materials => game.move_item(Asked::Material(index)),
                    // Into the refine machine: the server lists what refines it.
                    (Some((Dragged::Item(index), _, true)), _) if on_refine => {
                        let _ = game.session.refine_select(index);
                    }
                    // Onto the grade window: the server lists what raises it.
                    (Some((Dragged::Item(index), _, true)), _) if on_grade => {
                        let _ = game.session.grade_select(index);
                    }
                    // Into the enchant window: it stays here until a request; the server hears the item with that.
                    (Some((Dragged::Item(index), _, true)), _) if on_enchant => {
                        let slots = game.inventory.get(index).and_then(|item| self.items.get(&item.id)).map(|info| info.slots).unwrap_or(0);
                        if let Some(item) = game.inventory.get(index).cloned()
                            && let Some(window) = &mut game.enchant
                            && let Err(Some(id)) = window.try_item(&item, slots)
                        {
                            game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ERROR);
                        }
                    }
                    (Some((Dragged::Item(index), _, true)), _) if on_trade => {
                        if let Some(id) = game.offer(index, false) {
                            game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ERROR);
                        }
                    }
                    // While trading, nothing goes into the storage, the cart, the equipment window or an NPC's list.
                    (
                        Some((Dragged::Item(_) | Dragged::Worn(_), _, true)),
                        Some(Panel::Storage | Panel::StorageCategory(_) | Panel::Cart | Panel::Equipment),
                    ) if trade::refusal(game.trade.is_some(), trade::ItemAction::DropOn).is_some() => {}
                    (Some((Dragged::Item(index), _, true)), Some(Panel::Storage | Panel::StorageCategory(_))) => game.move_item(Asked::Store(index)),
                    // Let go over the world: dropped at the player's feet, asking how many of a stack. The drop lock holds it.
                    (Some((Dragged::Item(index), _, true)), None)
                        if inventory::may_ground_drop(game.inventory.lock_drop) && !game.on_ui(self.cursor, message_size) && !game.store_open() =>
                    {
                        game.drop_on_ground(index, &self.messages)
                    }
                    // Onto the equipment window, wherever: worn. Off it onto the inventory: taken off. Not while trading.
                    (Some((Dragged::Item(index), _, true)), Some(Panel::Equipment))
                        if !game.equipment.swapping
                            && game.equipment.owner.is_none()
                            && game.trade.is_none()
                            && game.inventory.get(index).is_some_and(|item| item.location != 0 && item.worn == 0) =>
                    {
                        game.use_item(index, &self.messages)
                    }
                    (Some((Dragged::Worn(index), _, true)), Some(Panel::Inventory)) if game.trade.is_none() => {
                        let _ = game.session.take_off(index);
                    }
                    (Some((Dragged::Item(index), _, true)), Some(Panel::Cart)) => game.move_item(Asked::ToCart(index)),
                    (Some((Dragged::Carted(index), _, true)), Some(Panel::Inventory)) => game.move_item(Asked::FromCart(index)),
                    (Some((Dragged::Stored(index), _, true)), Some(Panel::Inventory)) => game.move_item(Asked::Take(index)),
                    // Onto the swap view: into the Swap Equipment set, where it would be worn.
                    (Some((Dragged::Item(index), _, true)), Some(Panel::Equipment)) if game.equipment.swapping => {
                        if let Some(item) = game.inventory.get(index) {
                            game.switch_asked = Some(index);
                            let _ = game.session.switch_add(index, item.location);
                        }
                    }
                    _ => {}
                }
                let trade_moved = game.chat.drop_drag()
                    | game.trade.as_mut().is_some_and(|trade| trade.frame.drop_drag())
                    | game.shop.as_mut().is_some_and(|shop| shop.frames.iter_mut().fold(false, |moved, frame| frame.drop_drag() | moved))
                    | game.room.as_mut().is_some_and(|room| room.frame.drop_drag())
                    | game.card_view.as_mut().is_some_and(|view| view.drop_drag())
                    | game.item_window.as_mut().is_some_and(|window| window.drop_drag())
                    | game.card_window.as_mut().is_some_and(|window| window.drop_drag())
                    | game.mail.drop_drag()
                    | game.quest_window.frame.drop_drag()
                    | game.quest_window.detail.drop_drag()
                    | game.achievement_window.frame.drop_drag()
                    | game.synthesis.as_mut().is_some_and(|window| window.frame.drop_drag())
                    | game.upgrade.as_mut().is_some_and(|window| window.frame.drop_drag())
                    | game.roulette.as_mut().is_some_and(|window| window.frame.drop_drag())
                    | game.reform.as_mut().is_some_and(|window| window.frame.drop_drag())
                    | game.grade.as_mut().is_some_and(|window| window.frame.drop_drag())
                    | game.enchant.as_mut().is_some_and(|window| window.frame.drop_drag())
                    | game.monster_window.as_mut().is_some_and(|window| window.frame.drop_drag())
                    | game.stylist.as_mut().is_some_and(|window| window.frame.drop_drag())
                    | game.memorial.frame.drop_drag()
                    | game.storage_log.as_mut().is_some_and(storage_log::Log::drop_drag);
                // A slider let go keeps its volume for next time.
                let slid = game.audio_window.as_mut().is_some_and(options::AudioWindow::drop_drag);
                if slid {
                    self.sound.save();
                }
                let option_windows = game.graphics_window.as_mut().is_some_and(options::GraphicsWindow::drop_drag)
                    | game.shortcuts_window.as_mut().is_some_and(options::ShortcutsWindow::drop_drag);
                let outside = game.party_window.hit(self.cursor, game.party.as_ref(), &game.friends).is_none();
                let member_held = game.member_windows.release(outside, &party::Grid::new(screen()));
                // Let go outside the used cells: it went back, and the box says why.
                if std::mem::take(&mut game.member_windows.last_row) {
                    game.question_box = Some(BoxQuestion::LastRow);
                }
                let windows = trade_moved
                    | slid
                    | option_windows
                    | member_held
                    | game.party_window.frame.drop_drag()
                    | game.status_window.frame.drop_drag()
                    | game.guild_window.frame.drop_drag()
                    | game.clan_window.frame.drop_drag()
                    | game.cart.frame.drop_drag()
                    | game.pet_window.frame.drop_drag()
                    | game.mercenary_window.frame.drop_drag()
                    | game.mercenary_skill_list.drop_drag()
                    | game.homunculus_window.frame.drop_drag()
                    | game.homunculus_skill_list.drop_drag()
                    | game.storage.frame.drop_drag()
                    | game.storage.categories.iter_mut().fold(false, |any, category| category.frame.drop_drag() | category.stop_resize() | any)
                    | game.inventory.frame.drop_drag()
                    | game.inventory.stop_resize()
                    | game.cart.stop_resize()
                    | game.storage.stop_resize()
                    | game.skill_window.stop_resize()
                    | game.vend_setup.as_mut().is_some_and(vend::Setup::stop_resize)
                    | game.my_shop.as_mut().is_some_and(vend::MyShop::stop_resize)
                    | game.shop.as_mut().is_some_and(shop::Shop::stop_resize)
                    | game.equipment.frame.drop_drag()
                    | game.basic_info.frame.drop_drag()
                    | game.hotkeys.frame.drop_drag()
                    | game.skill_window.frame.drop_drag()
                    | game.talks.drop_drag()
                    | game.friend_setup.as_mut().is_some_and(|setup| setup.frame.drop_drag())
                    | game.block_list.as_mut().is_some_and(|list| list.frame.drop_drag())
                    | game.minimap_window.frame.drop_drag()
                    | game.say.frame.drop_drag()
                    | game.cutin.as_mut().is_some_and(|cutin| cutin.frame.drop_drag())
                    | game.npc_menu.drop_drag();
                windows || item.is_some_and(|(_, _, moved)| moved) || std::mem::take(&mut game.drag_cancelled)
            }
            _ => false,
        };
        if state == ElementState::Pressed {
            self.press();
        } else if !dragged {
            self.click(event_loop);
        }
    }

    /// Dev switch `RUSTNAROK_CLIENT_CLICKS=x,y[>x,y][;...]`: clicks, or drags to the second point, one a second from
    /// `RUSTNAROK_CLIENT_CLICKS_AT` seconds (4 by default) after entering the map.
    pub(crate) fn auto_input(&mut self, event_loop: &ActiveEventLoop) {
        let Scene::Game(game) = &self.scene else { return };
        let elapsed = game.entered.elapsed();
        while let Some(&(at, right, from, to)) = self.auto_input.last()
            && elapsed >= at
        {
            self.auto_input.pop();
            self.cursor = from;
            self.pointer_moved();
            let button = |app: &mut Self, state| if right { app.right_button(state) } else { app.left_button(event_loop, state) };
            button(self, ElementState::Pressed);
            // Held over a frame, so the world reads the press.
            self.read_button();
            if let Some(to) = to {
                self.cursor = to;
                self.pointer_moved();
            }
            button(self, ElementState::Released);
        }
    }

    /// Whether a left press at the pointer is the world's: not over a window, nor while dragging, aiming or capturing,
    /// nor with a store open.
    fn world_takes_press(&self) -> bool {
        let (Scene::Game(game), Some(assets)) = (&self.scene, &self.assets) else { return false };
        game.dragging.is_none() && !game.on_ui(self.cursor, assets.message.size) && !game.store_open() && game.targeting.is_none() && !game.capturing
    }

    /// Whether a world press does anything: not with the trade or storage window open (it's taken, and dropped).
    fn world_acts(&self) -> bool {
        matches!(&self.scene, Scene::Game(game) if !game.trade_or_storage())
    }

    /// The left button read once a frame (`ButtonRead`): a press since the last read, still down, acts in the world
    /// at the pointer now.
    pub(crate) fn read_button(&mut self) {
        if self.button.read(self.mouse_down) && self.world_takes_press() && self.world_acts() {
            self.world_press();
        }
    }

    /// A press on the ground walks there (in the frame that reads it), and holding the button keeps following the pointer. Shift turns
    /// to face it, Ctrl does nothing, and sitting only turns. A second press within DOUBLE_PRESS with the pointer
    /// still is ignored. Units and items are the click's (on release).
    fn world_press(&mut self) {
        let cursor = self.cursor;
        let Scene::Game(game) = &mut self.scene else { return };
        let now = Instant::now();
        let (double, mark) = press_pair(game.press_mark, now, cursor);
        game.press_mark = mark;
        let (shift, ctrl) = (self.modifiers.shift_key(), self.modifiers.control_key());
        // Alt + left press orders the mercenary (when it's out).
        if !double && self.modifiers.alt_key() && game.companion_press(false, cursor, shift) {
            return;
        }
        // One pending action at a time (`press_ends`).
        let click = game.world_click(cursor, shift);
        let (ends_attack, drops_pending) = press_ends(&click, double);
        if ends_attack {
            game.attack = None;
        }
        if drops_pending {
            game.pending = None;
            game.pending_walk = None;
        }
        // Every press but an attack starts the hold (a double press leaves the try time alone); each held try then
        // decides again.
        if press_holds(&click) {
            game.walk_held = true;
            if !double {
                game.last_walk_try = Some(whole_ms(now));
            }
        }
        let in_room = game.room.is_some();
        match click {
            // A double press does nothing in the world.
            _ if double => {}
            // Sitting or hidden (option 0x2): an attack click does nothing.
            WorldClick::Attack(_) if game.player.sitting || game.player.option & 0x2 != 0 => {}
            // A monster with Shift (without /noshift): nothing.
            WorldClick::Attack(id) if shift && !game.no_shift && game.units.get(&id).is_some_and(|actor| actor.kind == MONSTER) => {}
            // An attack: kept up with Ctrl or /noctrl (on someone else's homunculus always), otherwise while the
            // button is held.
            WorldClick::Attack(id) => {
                let homunculus = game.units.get(&id).is_some_and(|actor| actor.kind == HOMUNCULUS);
                let weight = |id: u16| game.status.get(&id).copied().unwrap_or(0);
                match attack_refused(weight(net::STATUS_WEIGHT), weight(net::STATUS_MAX_WEIGHT), game.riding) {
                    Some(line) => game.chat_line(self.messages.get(line).cloned().unwrap_or_default(), CHAT_ERROR),
                    None => {
                        game.attack = Some(game.new_attack(id, ctrl || game.no_ctrl || homunculus));
                        game.attack_step(now);
                    }
                }
            }
            WorldClick::Talk(id) => game.talk_to(id, &self.messages),
            // Held, the button follows the pointer again once the item is picked up.
            WorldClick::PickUp(id) if !game.player.sitting => game.pick_up(id),
            // A warp is ground: the cell under the pointer, not the portal's own.
            WorldClick::Warp | WorldClick::Ground => {
                let cell = game.ground_cell(cursor);
                match (cell, ground_press(ctrl, shift, game.player.sitting, in_room, self.right_down, cell.is_some())) {
                    (Some(cell), GroundAct::Walk) => drop(game.walk_toward(cell, now)),
                    (Some(cell), GroundAct::Turn) => game.turn_to(cell),
                    _ => {}
                }
            }
            _ => {}
        }
    }

    /// A mouse press: grabs a window by its title bar, and notes it for Esc.
    pub(crate) fn press(&mut self) {
        let cursor = self.cursor;
        // A press on the right-click menu is the menu's (its choice is taken on the release), not the window's under it.
        if let Scene::Game(game) = &self.scene
            && game.menu.as_ref().is_some_and(|menu| menu.hit(cursor).is_some())
        {
            return;
        }
        // So is a press on the message box or Confirm Deal: nothing under it starts a drag, so its release still clicks.
        if let (Scene::Game(game), Some(assets)) = (&self.scene, &self.assets)
            && (game.question_box.is_some() && contains(dialog_position(&assets.message), assets.message.size, cursor)
                || game.deal_confirm.as_ref().is_some_and(|confirm| confirm.hit(cursor).is_some()))
        {
            return;
        }
        // Aiming a skill: the press on its target uses it; any other press lets go of the aim. On the world that's
        // all it does (no walk); on a window the window gets the press too.
        if let (Scene::Game(game), Some(assets)) = (&mut self.scene, &self.assets)
            && game.targeting.is_some()
        {
            // ponytail: a click on the member's window casts on their account id (a player's unit id). Range is the
            // same as aiming at them in the world; the close button still closes.
            // A member's small window or row in the party window: their skill if the rule takes it, else the aim ends.
            let row = match game.party_window.hit(cursor, game.party.as_ref(), &game.friends) {
                Some(party::Hit::Member(account)) if game.window_at(cursor) == Some(Panel::Party) => Some(account),
                _ => None,
            };
            if let Some(account) = game.party_member_at(cursor).or(row) {
                if game.row_takes_aim(account, self.modifiers.shift_key()) {
                    game.aim_skill(SkillTarget::Unit(account));
                } else {
                    game.targeting = None;
                }
                return;
            }
            let on_ui = game.on_ui(cursor, assets.message.size);
            let shift = self.modifiers.shift_key();
            if on_ui {
                game.targeting = None;
            } else {
                // An NPC is talked to; a unit the press doesn't take, the ground under a unit skill, an item or a warp:
                // the aim just ends. Over 90 % weight a unit skill is refused.
                let weight = |id: u16| game.status.get(&id).copied().unwrap_or(0);
                let heavy = attack_refused(weight(net::STATUS_WEIGHT), weight(net::STATUS_MAX_WEIGHT), false);
                match game.aim_target(cursor, game.players_first(shift)) {
                    Some(SkillTarget::Unit(id)) if game.units.get(&id).is_some_and(|actor| actor.kind == NPC && actor.job != WARP_JOB) => {
                        game.targeting = None;
                        game.talk_to(id, &self.messages);
                    }
                    Some(SkillTarget::Unit(id)) if game.aim_takes_unit(id, shift) => match heavy {
                        Some(line) => {
                            game.targeting = None;
                            game.chat_line(self.messages.get(line).cloned().unwrap_or_default(), CHAT_ERROR);
                        }
                        None => game.aim_skill(SkillTarget::Unit(id)),
                    },
                    Some(target @ SkillTarget::Cell(_)) => game.aim_skill(target),
                    _ => game.targeting = None,
                }
                return;
            }
        }
        // A press the world takes is read with the frame (`ButtonRead`); the windows under it don't get it.
        if self.world_takes_press() {
            self.button.due = true;
            return;
        }
        // The big minimap is above the other windows, under the world map.
        if let Scene::Game(game) = &mut self.scene
            && game.world_map.is_none()
            && game.minimap_window.covers(cursor)
        {
            let size = [game.world.gat.width as u32, game.world.gat.height as u32];
            let pins = minimap_window::Pins { player: game.player.cell, party: &[], guild: &[], boss: None, quests: &[] };
            if game.minimap_window.hit(cursor, size, &pins) == Some(minimap_window::Hit::Title(TitleHit::Title)) {
                game.minimap_window.frame.start_drag(cursor);
            }
            game.press_esc(cursor);
            return;
        }
        if let Scene::Game(game) = &mut self.scene {
            // Windows opened since the last Esc join the list before this press moves one to the front.
            game.sync_esc();
            // A button on the game menu is not a press on the window under it.
            let on_menu = game.escape.as_ref().is_some_and(|menu| menu.hit(cursor, game.can_resurrect()).is_some());
            if !on_menu && game.card_view.as_ref().is_some_and(|view| view.hit(cursor).is_some()) {
                escape::touch(&mut game.esc_order, escape::WindowId::CardPicture);
            } else if !on_menu && game.card_window.as_ref().is_some_and(|window| window.hit(cursor).is_some()) {
                escape::touch(&mut game.esc_order, escape::WindowId::CardInfo);
            } else if !on_menu && game.item_window.as_ref().is_some_and(|window| window.hit(cursor).is_some()) {
                escape::touch(&mut game.esc_order, escape::WindowId::ItemInfo);
            } else if !on_menu {
                game.press_esc(cursor);
            }
        }
        // The guild storage log: its tabs and arrows on the press, Close too, and anywhere else drags it.
        if let Scene::Game(game) = &mut self.scene
            && let Some(log) = &mut game.storage_log
            && let Some(hit) = log.hit(cursor)
        {
            if log.press(hit, cursor) {
                game.storage_log = None;
            }
            return;
        }
        if let Scene::Game(game) = &mut self.scene
            && game.memorial.hit(cursor).is_some()
        {
            if game.memorial.hit(cursor) == Some(memorial::Hit::Title(TitleHit::Title)) {
                game.memorial.frame.start_drag(cursor);
            }
            return;
        }
        if let Scene::Game(game) = &mut self.scene {
            let job = game.player.look.as_ref().map(|look| look.job).unwrap_or(game.me.job);
            let hit = game.stylist.as_ref().and_then(|window| window.hit(cursor, &self.styling_shop, job));
            if let Some(hit) = hit {
                if hit == stylist::Hit::Title(TitleHit::Title) {
                    game.stylist.as_mut().unwrap().frame.start_drag(cursor);
                }
                return;
            }
        }
        // A card's picture sits over the item info.
        if let Scene::Game(game) = &mut self.scene
            && let Some(view) = &mut game.card_view
            && let Some(hit) = view.hit(cursor)
        {
            if hit == iteminfo::ViewHit::Grip {
                view.start_drag(cursor);
            }
            return;
        }
        // The chat window goes by its tab strip.
        if let Scene::Game(game) = &mut self.scene
            && game.chat.press(cursor)
        {
            return;
        }
        // The item description window goes by its top band, and its height by the grip (the NPC window's links are
        // over it).
        if let Scene::Game(game) = &mut self.scene
            && links::at(&game.dialog_links, cursor).is_none()
            && let Some((window, hit)) =
                [&mut game.card_window, &mut game.item_window].into_iter().flatten().find_map(|window| window.hit(cursor).map(|hit| (window, hit)))
        {
            match hit {
                iteminfo::Hit::Grip => window.start_drag(cursor),
                iteminfo::Hit::Resize => window.start_resize(cursor),
                _ => {}
            }
            return;
        }
        // The NPC menu, over the say window: a press on a row selects it (a double click picks it); any part but the
        // list and the buttons drags it.
        if let Scene::Game(game) = &mut self.scene
            && let Some(menu) = game.dialog.as_mut().and_then(|dialog| dialog.menu.as_mut())
            && let Some(local) = game.npc_menu.local(cursor, menu.size()[1])
        {
            if menu.grips(local) {
                game.npc_menu.start_drag(cursor);
            } else if menu.press(local, Instant::now()) {
                game.menu_ok();
            }
            return;
        }
        // The NPC say window goes by any part but its button, links and scrollbar.
        if let Scene::Game(game) = &mut self.scene
            && game.dialog.as_ref().is_some_and(|dialog| dialog.warp.is_none())
            && game.say.contains(cursor)
        {
            let on_button = game.dialog.as_ref().is_some_and(|dialog| dialog.button != DialogButton::None) && game.say.on_button(cursor);
            if game.say.scrollbar_box().is_some_and(|[x, y, width, height]| contains([x, y], [width, height], cursor)) {
                game.say.press_scrollbar(cursor);
            } else if !on_button && links::at(&game.dialog_links, cursor).is_none() {
                game.say.frame.start_drag(cursor);
            }
            return;
        }
        // The chat room's window is over the others too.
        if let Scene::Game(game) = &mut self.scene
            && let Some(room) = &mut game.room
            && let Some(hit) = room.hit(cursor, &game.me.name)
        {
            if hit == chatroom::RoomHit::Title(TitleHit::Title) {
                room.frame.start_drag(cursor);
            }
            return;
        }
        // The deal's window is over the others.
        if let Scene::Game(game) = &mut self.scene
            && let Some(trade) = &mut game.trade
            && let Some(hit) = trade.hit(cursor)
        {
            // Any part that isn't a control moves it.
            if hit == trade::Hit::Window {
                trade.frame.start_drag(cursor);
            }
            return;
        }
        // The vending windows' corner grips.
        if let Scene::Game(game) = &mut self.scene {
            if let Some(setup) = &mut game.vend_setup
                && setup.hit(cursor, &game.cart.items) == Some(vend::Hit::Resize)
            {
                setup.start_resize(cursor);
                return;
            }
            if let Some(shop) = &mut game.my_shop
                && shop.on_grip(cursor)
            {
                shop.start_resize(cursor);
                return;
            }
        }
        // A shop's row starts a drag at once; its title bar moves the window.
        if let Scene::Game(game) = &mut self.scene
            && let Some(hit) = game.shop.as_ref().and_then(|shop| shop.hit(cursor))
        {
            match hit {
                shop::Hit::Row(0, row) => game.dragging = Some((Dragged::ShopOffer(row), cursor, false)),
                shop::Hit::Row(_, row) => game.dragging = Some((Dragged::ShopDeal(row), cursor, false)),
                shop::Hit::Title(window, TitleHit::Title) => game.shop.as_mut().unwrap().frames[window].start_drag(cursor),
                shop::Hit::Resize(_) => {
                    game.shop.as_mut().unwrap().start_resize(cursor);
                }
                _ => {}
            }
            return;
        }
        // So are the mail windows, and the audio settings.
        if let Scene::Game(game) = &mut self.scene
            && game.mail.hit(cursor).is_some()
        {
            game.mail.start_drag(cursor);
            return;
        }
        if let Scene::Game(game) = &mut self.scene
            && let Some(window) = &mut game.monster_window
            && window.hit(cursor) == Some(false)
        {
            if window.frame.local(cursor, monster_info::MonsterWindow::height()).and_then(|local| window.frame.title_hit(local)) == Some(TitleHit::Title) {
                window.frame.start_drag(cursor);
            }
            return;
        }
        if let Scene::Game(game) = &mut self.scene
            && let Some(hit) = game.synthesis.as_ref().and_then(|window| window.hit(cursor))
        {
            if hit == laphine::Hit::Title(TitleHit::Title) {
                game.synthesis.as_mut().unwrap().frame.start_drag(cursor);
            }
            return;
        }
        if let Scene::Game(game) = &mut self.scene
            && let Some(hit) = game.upgrade.as_ref().and_then(|window| window.hit(cursor))
        {
            if hit == laphine::Hit::Title(TitleHit::Title) {
                game.upgrade.as_mut().unwrap().frame.start_drag(cursor);
            }
            return;
        }
        if let Scene::Game(game) = &mut self.scene
            && let Some(hit) = game.enchant.as_ref().and_then(|window| window.hit(cursor))
        {
            if hit == enchant::Hit::Title(TitleHit::Title) {
                game.enchant.as_mut().unwrap().frame.start_drag(cursor);
            }
            return;
        }
        if let Scene::Game(game) = &mut self.scene
            && let Some(hit) = game.roulette.as_ref().and_then(|window| window.hit(cursor))
        {
            if hit == roulette::Hit::Title(TitleHit::Title) {
                game.roulette.as_mut().unwrap().frame.start_drag(cursor);
            }
            return;
        }
        if let Scene::Game(game) = &mut self.scene
            && let Some(hit) = game.reform.as_ref().and_then(|window| window.hit(cursor, &game.inventory.items))
        {
            if hit == reform::Hit::Title(TitleHit::Title) {
                game.reform.as_mut().unwrap().frame.start_drag(cursor);
            }
            return;
        }
        if let Scene::Game(game) = &mut self.scene
            && let Some(hit) = game.grade.as_ref().and_then(|window| window.hit(cursor))
        {
            if hit == grade::Hit::Title(TitleHit::Title) {
                game.grade.as_mut().unwrap().frame.start_drag(cursor);
            }
            return;
        }
        if let Scene::Game(game) = &mut self.scene
            && game.achievement_window.covers(cursor)
        {
            if game.achievement_window.hit(cursor, &self.achievement_info, &game.achievements) == Some(achievement::Hit::Title(TitleHit::Title)) {
                game.achievement_window.frame.start_drag(cursor);
            }
            return;
        }
        if let Scene::Game(game) = &mut self.scene
            && let Some(hit) = game.quest_window.hit(cursor, &game.quests, &self.quest_texts)
        {
            // Each drags from anywhere that isn't a control (the details stay put when the list moves).
            if hit == quest::Hit::Window && game.quest_window.frame.local(cursor, 466.0).is_some() {
                game.quest_window.frame.start_drag(cursor);
            }
            if hit == quest::Hit::Detail {
                game.quest_window.detail.start_drag(cursor);
            }
            return;
        }
        if let Scene::Game(game) = &mut self.scene
            && let Some(window) = &mut game.audio_window
            && let Some(hit) = window.hit(cursor)
        {
            if window.press(hit, cursor, &mut self.sound)
                && let Some(audio) = &mut self.audio
            {
                audio.set(&self.sound);
            }
            return;
        }
        if let Scene::Game(game) = &mut self.scene
            && let Some(window) = &mut game.graphics_window
            && let Some(hit) = window.hit(cursor)
        {
            window.press(hit, cursor);
            return;
        }
        if let Scene::Game(game) = &mut self.scene
            && let Some(window) = &mut game.shortcuts_window
            && let Some(hit) = window.hit(cursor)
        {
            window.press(hit, cursor);
            return;
        }
        // A small party window: a press anywhere on it grabs it, unless the party window's lock is on.
        if let Scene::Game(game) = &mut self.scene
            && let Some(account) = game.party_member_at(cursor)
        {
            if !game.party_window.locked {
                game.member_windows.press_window(account, cursor, &party::Grid::new(screen()));
            }
            return;
        }
        if let Scene::Game(game) = &mut self.scene
            && let Some(id) = game.window_at(cursor)
        {
            game.raise(id);
            if let (Panel::Inventory, Some(inventory::Hit::Item(index))) = (id, game.inventory.hit(cursor)) {
                game.dragging = Some((Dragged::Item(index), cursor, false));
            }
            if let (Panel::Hotkeys, Some(hotkeys::Hit::Slot(slot))) = (id, game.hotkeys.hit(cursor))
                && game.hotkeys.keys.get(slot).is_some_and(Option::is_some)
            {
                game.dragging = Some((Dragged::Hotkey(slot), cursor, false));
            }
            if let (Panel::Cart, Some(inventory::Hit::Item(index))) = (id, game.cart.hit(cursor)) {
                game.dragging = Some((Dragged::Carted(index), cursor, false));
            }
            if let (Panel::Equipment, Some(equipment::Hit::Slot(index))) = (id, game.equipment.hit(cursor, &game.inventory.items))
                && game.equipment.owner.is_none()
                && !game.equipment.swapping
            {
                game.dragging = Some((Dragged::Worn(index), cursor, false));
            }
            if id == Panel::Storage
                && let Some(assets) = &self.assets
            {
                match game.storage.hit(cursor, &assets.storage, &self.items, (&self.messages[..], &self.card_names, &game.char_names)) {
                    Some(storage::Hit::Item(index)) => game.dragging = Some((Dragged::Stored(index), cursor, false)),
                    Some(storage::Hit::Title(TitleHit::Title)) => game.storage.frame.start_drag(cursor),
                    Some(storage::Hit::Resize) => game.storage.start_resize(cursor),
                    // A category's box toggles on the press; one turned on comes to the front.
                    Some(storage::Hit::Box(tab)) => {
                        game.storage.toggle_category(tab);
                        game.raise(Panel::StorageCategory(tab));
                        game.sync_esc();
                    }
                    _ => {}
                }
            }
            if let Some(which) = storage_list(id) {
                match game.storage.category_hit(which, cursor, &self.items, (&self.messages[..], &self.card_names, &game.char_names)) {
                    Some(storage::Hit::Item(index)) => game.dragging = Some((Dragged::Stored(index), cursor, false)),
                    // The Search window's close button shuts it.
                    Some(storage::Hit::Title(TitleHit::Close)) => {
                        game.storage.search_window.frame.open = false;
                        game.sync_esc();
                    }
                    Some(storage::Hit::Title(_)) => game.storage.list_window_mut(which).frame.start_drag(cursor),
                    Some(storage::Hit::Resize) => game.storage.list_window_mut(which).start_resize(cursor),
                    _ => {}
                }
            }
            if id == Panel::Skills {
                let jobs = skills::tabs(&self.skill_trees, game.me.job, &game.skills);
                let sheet = skills::Sheet { trees: &self.skill_trees, jobs: &jobs, skills: &game.skills, info: &game.skill_info, points: game.skill_points() };
                match game.skill_window.hit(cursor, &sheet) {
                    // Not while levels are pending.
                    Some(skills::Hit::Skill(skill))
                        if game.skill_window.draggable() && game.skills.iter().any(|known| known.id == skill && known.level > 0 && known.kind != 0) =>
                    {
                        game.dragging = Some((Dragged::Skill(skill), cursor, false));
                    }
                    Some(skills::Hit::Title(TitleHit::Title)) => game.skill_window.frame.start_drag(cursor),
                    Some(skills::Hit::Resize) => game.skill_window.start_resize(cursor),
                    _ => {}
                }
            }
            match id {
                Panel::BasicInfo if game.basic_info.hit(cursor) == Some(basic_info::Hit::Title) => game.basic_info.frame.start_drag(cursor),
                Panel::Hotkeys if game.hotkeys.grabbed(cursor) => game.hotkeys.frame.start_drag(cursor),
                Panel::Inventory if game.inventory.hit(cursor) == Some(inventory::Hit::Title(TitleHit::Title)) => game.inventory.frame.start_drag(cursor),
                // A member's row, unlocked: dragged out it makes a small window; a second press on another member's row
                // soon after opens the 1:1 chat with them.
                Panel::Party if matches!(game.party_window.hit(cursor, game.party.as_ref(), &game.friends), Some(party::Hit::Member(_))) => {
                    if let Some(party::Hit::Member(account)) = game.party_window.hit(cursor, game.party.as_ref(), &game.friends)
                        && !game.party_window.locked
                    {
                        let now = Instant::now();
                        let double = game.row_press.is_some_and(|(at, whose)| whose == account && now.saturating_duration_since(at) < DOUBLE_PRESS);
                        game.row_press = (!double).then_some((now, account));
                        if double && account != game.account_id {
                            let name =
                                game.party.as_ref().and_then(|party| party.members.iter().find(|m| m.info.account == account)).map(|m| m.info.name.clone());
                            if let Some(name) = name {
                                game.whisper_member(&name);
                            }
                        } else {
                            game.member_windows.press_row(account);
                        }
                    }
                }
                Panel::Party if game.party_window.hit(cursor, game.party.as_ref(), &game.friends) == Some(party::Hit::Title(TitleHit::Title)) => {
                    game.party_window.frame.start_drag(cursor)
                }
                Panel::Status
                    if game.status_window.frame.local(cursor, game.status_height).and_then(|local| game.status_window.frame.title_hit(local))
                        == Some(TitleHit::Title) =>
                {
                    game.status_window.frame.start_drag(cursor)
                }
                Panel::Guild if game.guild.as_ref().and_then(|guild| game.guild_window.hit(cursor, guild)) == Some(guild::Hit::Title(TitleHit::Title)) => {
                    game.guild_window.frame.start_drag(cursor)
                }
                Panel::Clan if game.clan.is_some() && game.clan_window.hit(cursor) == Some(clan::Hit::Title(TitleHit::Title)) => {
                    game.clan_window.frame.start_drag(cursor)
                }
                Panel::Cart if game.cart.hit(cursor) == Some(inventory::Hit::Title(TitleHit::Title)) => game.cart.frame.start_drag(cursor),
                // The corner grip resizes.
                Panel::Inventory if game.inventory.hit(cursor) == Some(inventory::Hit::Resize) => game.inventory.start_resize(cursor),
                Panel::Cart if game.cart.hit(cursor) == Some(inventory::Hit::Resize) => game.cart.start_resize(cursor),
                Panel::Pet if game.pet.is_some() && game.pet_window.hit(cursor) == Some(pet::Hit::Window) => game.pet_window.frame.start_drag(cursor),
                Panel::Homunculus if game.homunculus.is_some() && game.homunculus_window.hit(cursor) == Some(homunculus::Hit::Window) => {
                    game.homunculus_window.frame.start_drag(cursor)
                }
                Panel::HomunculusSkills => {
                    let points = game.homunculus.as_ref().map_or(0, |homunculus| homunculus.skill_points);
                    match game.homunculus_skill_list.hit(cursor, &game.homunculus_skills, points) {
                        Some(homunculus::SkillHit::Title) => game.homunculus_skill_list.frame.start_drag(cursor),
                        Some(homunculus::SkillHit::Resize) => game.homunculus_skill_list.start_resize(cursor),
                        _ => {}
                    }
                }
                Panel::Mercenary if game.mercenary_window.hit(cursor) == Some(mercenary::Hit::Window) => game.mercenary_window.frame.start_drag(cursor),
                Panel::MercenarySkills => match game.mercenary_skill_list.hit(cursor, &game.mercenary_skills, 0) {
                    Some(homunculus::SkillHit::Title) => game.mercenary_skill_list.frame.start_drag(cursor),
                    Some(homunculus::SkillHit::Resize) => game.mercenary_skill_list.start_resize(cursor),
                    _ => {}
                },
                Panel::Equipment if game.equipment.hit(cursor, &game.inventory.items) == Some(equipment::Hit::Title(TitleHit::Title)) => {
                    game.equipment.frame.start_drag(cursor)
                }
                // Any part of an NPC's picture but its close button drags it.
                Panel::Cutin if self.assets.as_ref().is_some_and(|assets| !game.cutin.as_ref().is_some_and(|cutin| cutin.on_close(cursor, &assets.cutin))) => {
                    game.cutin.as_mut().unwrap().frame.start_drag(cursor)
                }
                _ => {}
            }
        }
    }

    pub(crate) fn close_message(&mut self) {
        if let Scene::Message(_, back) = std::mem::replace(&mut self.scene, Scene::Login) {
            self.quitting |= matches!(*back, Scene::Quit);
            self.scene = *back;
        }
    }

    /// Sends what's on the chat's input line, as Enter does: the client's sound commands (/bgm, /sound, /bv, /v),
    /// else `Game::send_chat` (party with `party`, guild with `guild`).
    pub(crate) fn send_chat_input(&mut self, party: bool, guild: bool) {
        let Scene::Game(game) = &mut self.scene else { return };
        let music_was_on = self.sound.on[0];
        if let Some(shown) = self.sound.command(game.chat.input.trim()) {
            game.chat.input.clear();
            if let Some(audio) = &mut self.audio {
                audio.set(&self.sound);
            }
            self.sound.save();
            if let Some(id) = shown {
                game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ITEM);
            }
            // /bgm on: the map's song from its beginning.
            if self.sound.on[0] && !music_was_on {
                let map = game.map.clone();
                self.play_music(Some(&map));
            }
        } else {
            let before = self.graphics;
            // The receiving group button's choice, or the key held.
            let (party, guild) = (party || game.chat.send_to == chat::SendTo::Party, guild || game.chat.send_to == chat::SendTo::Guild);
            if let Some(id) = game.send_chat(party, guild, &self.messages, &mut self.graphics) {
                game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ERROR);
            }
            if before != self.graphics {
                self.graphics.save();
            }
        }
    }

    /// Alt+1 to Alt+0: sends macro line `line` as if typed and sent with Enter, keeping what the player was typing.
    pub(crate) fn send_macro(&mut self, line: usize) {
        let Scene::Game(game) = &mut self.scene else { return };
        let text = game.macro_lines[line].clone();
        if text.trim().is_empty() {
            return;
        }
        let typing = std::mem::replace(&mut game.chat.input, text);
        self.send_chat_input(false, false);
        if let Scene::Game(game) = &mut self.scene {
            game.chat.input = typing;
        }
    }

    /// One Esc in the game: the battle-mode line, a focused text box, a question's No, the newest window, or the menu.
    pub(crate) fn game_escape(&mut self) {
        let Scene::Game(game) = &mut self.scene else { return };
        game.sync_esc();
        let view = escape::EscView {
            // The always-on line is not this. An open battle-mode line is the first Esc.
            battle_line: game.chat_mode.battle_line_open(),
            whisper_list: game.chat.whisper_list_open(),
            focused: game.focused_text(),
            conversation: game.dialog.as_ref().is_some_and(|dialog| dialog.warp.is_none()),
            question: game.question_open(),
            // Esc never closes the own shop or buying store.
            newest: game.esc_order.iter().copied().find(|id| !matches!(id, escape::WindowId::MyShop | escape::WindowId::MyBuyingStore)),
        };
        let act = escape::decide(false, view);
        let Scene::Game(game) = &mut self.scene else { return };
        match act {
            Some(escape::EscAct::CloseWhisperList) => game.chat.close_whisper_list(),
            Some(escape::EscAct::CloseBattleChat) => {
                game.chat.close_battle_line();
                game.chat_mode.close_line();
            }
            Some(escape::EscAct::Cancel(id) | escape::EscAct::Close(id)) => game.close_esc(id),
            Some(escape::EscAct::ShopCancel) => game.close_esc(escape::WindowId::Shop),
            Some(escape::EscAct::AnswerNo) => game.answer_no(&self.messages),
            Some(escape::EscAct::ToggleMenu) => {
                game.escape = if game.escape.is_some() { None } else { Some(escape::Menu { dead: game.player.dead }) };
            }
            None => {}
        }
    }

    /// The next frame goes to ScreenShot/screen<server><NNN>.jpg in the folder the client runs in.
    // ponytail: "next to the client" taken as the folder it runs in (where its data is), not the executable's.
    pub(crate) fn request_screenshot(&mut self) {
        let server = self.account.as_ref().and_then(|account| account.char_servers.get(self.char_server)).map(|server| server.name.clone()).unwrap_or_default();
        let folder = std::path::Path::new(screenshot::FOLDER);
        let _ = std::fs::create_dir_all(folder);
        self.print_screen = Some(screenshot::next_path(folder, &server));
    }

    /// The shortcut this key fires, if it has one. A repeated key-down does not.
    pub(crate) fn fire_shortcut(&mut self, event: &control::KeyPress) -> bool {
        let Scene::Game(game) = &self.scene else { return false };
        let text_focus = game.text_box_focused();
        let fired = shortcuts::action(
            &self.keybinds,
            &shortcuts::KeyEvent {
                key: &event.logical_key,
                physical: Some(event.physical_key),
                alt: self.modifiers.alt_key(),
                shift: self.modifiers.shift_key(),
                ctrl: self.modifiers.control_key(),
                tab: self.tab_held && event.logical_key != Key::Named(NamedKey::Tab),
                repeat: event.repeat,
                text_focus,
                bar: if game.hotkeys.tab() == 0 { 1 } else { 2 },
            },
        );
        fired.is_some_and(|fired| self.perform_shortcut(fired))
    }

    /// Runs a shortcut. False leaves the key for whatever else is listening (Enter still sends chat).
    pub(crate) fn perform_shortcut(&mut self, fired: shortcuts::Action) -> bool {
        if matches!(fired, shortcuts::Action::ChatMode | shortcuts::Action::Unused) {
            return false;
        }
        if let shortcuts::Action::Macro(line) = fired {
            self.send_macro(usize::from(line));
            return true;
        }
        if fired == shortcuts::Action::Screenshot {
            self.request_screenshot();
            return true;
        }
        // Ctrl+Tab: nothing indoors or while the world map is open.
        if fired == shortcuts::Action::Minimap {
            if let Scene::Game(game) = &mut self.scene
                && !game.camera_goal.indoor
                && game.world_map.is_none()
            {
                game.minimap_view = game.minimap_view.next();
                minimap::save_view(game.minimap_view);
            }
            return true;
        }
        // The big minimap: nothing indoors.
        if fired == shortcuts::Action::MinimapWindow {
            if let (Scene::Game(game), Some(gpu)) = (&mut self.scene, &self.gpu)
                && !game.camera_goal.indoor
            {
                toggle_big_minimap(game, gpu, &self.grf, &self.minimap_exceptions);
            }
            return true;
        }
        let Scene::Game(game) = &mut self.scene else { return true };
        match fired {
            shortcuts::Action::BasicInfo => {
                game.basic_info.frame.toggle();
                game.raise(Panel::BasicInfo);
            }
            shortcuts::Action::Equipment => {
                game.equipment.toggle_general();
                game.raise(Panel::Equipment);
            }
            shortcuts::Action::Skills => {
                game.skill_window.frame.toggle();
                game.raise(Panel::Skills);
            }
            shortcuts::Action::Quests => game.quest_window.toggle(),
            shortcuts::Action::Friends => {
                if game.party_window.frame.open && game.party_window.friends {
                    game.party_window.frame.open = false;
                } else {
                    game.party_window.friends = true;
                    game.party_window.picked = None;
                    game.party_window.frame.open = true;
                    game.raise(Panel::Party);
                }
            }
            shortcuts::Action::FriendSetup => game.open_friend_setup(),
            shortcuts::Action::Party => {
                game.party_window.frame.toggle();
                game.raise(Panel::Party);
            }
            shortcuts::Action::ChatRoom => {
                game.room_make = if game.room_make.is_some() || game.room.is_some() { None } else { Some(chatroom::Make::default()) };
            }
            shortcuts::Action::Cart if game.cart.cart.is_some() => {
                game.cart.frame.toggle();
                game.raise(Panel::Cart);
            }
            shortcuts::Action::Cart => {}
            shortcuts::Action::WorldMap => toggle_world_map(game, self.gpu.as_ref(), &self.grf, &self.worlds),
            shortcuts::Action::Inventory => {
                game.inventory.frame.toggle();
                game.raise(Panel::Inventory);
            }
            shortcuts::Action::Guild if game.guild.is_none() => game.guild_companion.toggle(screen()),
            shortcuts::Action::Guild => {
                game.guild_window.frame.toggle();
                game.raise(Panel::Guild);
                if game.guild_window.frame.open {
                    game.ask_guild_pages();
                }
            }
            shortcuts::Action::Emotions => game.toggle_emotion_list(),
            shortcuts::Action::Sit => game.toggle_sit(),
            shortcuts::Action::Macros => game.toggle_macro_window(),
            shortcuts::Action::CloseWindows => game.close_windows(),
            shortcuts::Action::HotkeyBar => game.hotkeys.frame.toggle(),
            shortcuts::Action::Chat => game.chat.toggle_hidden(),
            shortcuts::Action::ChatSize => game.chat.cycle_size(),
            shortcuts::Action::HpSp => game.show_bars = !game.show_bars,
            // Alt+J opens or shuts the window and asks nothing.
            shortcuts::Action::Pet if game.pet.is_some() => {
                game.pet_window.frame.toggle();
                game.raise(Panel::Pet);
            }
            shortcuts::Action::Pet => {}
            shortcuts::Action::Homunculus if game.homunculus.is_some() => {
                game.homunculus_window.frame.toggle();
                game.raise(Panel::Homunculus);
            }
            shortcuts::Action::Homunculus => {}
            shortcuts::Action::Mercenary if game.mercenary.is_some() => {
                game.mercenary_window.frame.toggle();
                game.raise(Panel::Mercenary);
            }
            shortcuts::Action::Mercenary => {}
            shortcuts::Action::HomunculusFollow if game.homunculus.is_some() => {
                game.homunculus_steering.following = !game.homunculus_steering.following;
            }
            shortcuts::Action::HomunculusFollow => {}
            shortcuts::Action::MercenaryFollow if game.mercenary.is_some() => game.steering.following = !game.steering.following,
            shortcuts::Action::MercenaryFollow => {}
            shortcuts::Action::Menu => {
                if !game.basic_info.frame.open {
                    game.basic_info.frame.open = true;
                    game.basic_info.menu_open = true;
                    game.raise(Panel::BasicInfo);
                } else {
                    game.basic_info.menu_open = !game.basic_info.menu_open;
                }
            }
            shortcuts::Action::Clan if game.clan.is_some() => {
                game.clan_window.frame.toggle();
                game.raise(Panel::Clan);
            }
            shortcuts::Action::Clan => {}
            shortcuts::Action::Bank => {
                if game.bank.is_some() {
                    let _ = game.session.close_bank(game.account_id);
                    game.bank = None;
                } else {
                    let _ = game.session.open_bank(game.account_id);
                }
            }
            shortcuts::Action::Status => {
                game.status_window.frame.toggle();
                game.raise(Panel::Status);
            }
            shortcuts::Action::PartyBoard => game.toggle_booking(),
            shortcuts::Action::Reputation => {
                game.toggle_reputation(&self.reputation_tables);
                game.sync_esc();
            }
            // The other equipment set, as the equipment window's swap button.
            shortcuts::Action::SwapEquipment => match game.equipment.switch_all(&game.inventory.items, control::now()) {
                Ok(true) => drop(game.session.switch_all()),
                Ok(false) => {}
                Err(id) => game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ERROR),
            },
            shortcuts::Action::Achievements => game.achievement_window.frame.toggle(),
            shortcuts::Action::Mail => {
                let action = game.mail.toggle();
                game.mail_action(action);
            }
            shortcuts::Action::RotateHotkeys => {
                let (tab, rowshift) = game.hotkeys.rotate();
                let _ = game.session.rotate_hotkeys(tab, rowshift);
            }
            shortcuts::Action::SwitchHotkeys => game.hotkeys.switch(),
            shortcuts::Action::Costume => {
                if game.equipment.frame.open && game.equipment.showing_costume() {
                    game.equipment.frame.open = false;
                } else {
                    game.equipment.show_costume();
                    game.equipment.frame.open = true;
                    game.raise(Panel::Equipment);
                }
            }
            // Only the showing bar's keys come here (see shortcuts::action).
            shortcuts::Action::Hotbar { slot, .. } => game.press_hotkey(usize::from(slot), &self.messages),
            shortcuts::Action::Memorial => {
                if game.memorial.frame.open {
                    game.memorial.frame.open = false;
                    game.memorial_asked = false;
                    escape::remove(&mut game.esc_order, escape::WindowId::Memorial);
                } else {
                    game.show_memorial();
                }
            }
            // Off clears the square too.
            // ponytail: the game leaves it where it was; nobody wants a square stuck on the ground.
            shortcuts::Action::GroundCursor => {
                game.ground_cursor = !game.ground_cursor;
                game.ground_mark = game.ground_mark.filter(|_| game.ground_cursor);
            }
            // ponytail: no window yet (commands, tip, hunting, flags). The key is eaten.
            shortcuts::Action::Commands | shortcuts::Action::Tip | shortcuts::Action::Hunting | shortcuts::Action::Flag(_) => {}
            shortcuts::Action::Macro(_)
            | shortcuts::Action::Screenshot
            | shortcuts::Action::Minimap
            | shortcuts::Action::MinimapWindow
            | shortcuts::Action::ChatMode
            | shortcuts::Action::Unused => {}
        }
        true
    }

    pub(crate) fn key(&mut self, event: &control::KeyPress) {
        // Tab held counts as a modifier for the keys pressed meanwhile.
        if event.logical_key == Key::Named(NamedKey::Tab) {
            self.tab_held = event.state == ElementState::Pressed;
        }
        // Print Screen on its release (Windows sends no press for it), Scroll Lock, and on Macs Cmd+Shift+S (they have
        // no Print Screen; a ponytail choice): a screenshot.
        let pressed = event.state == ElementState::Pressed && !event.repeat;
        let shot = match &event.logical_key {
            Key::Named(NamedKey::PrintScreen) => event.state == ElementState::Released,
            Key::Named(NamedKey::ScrollLock) => pressed,
            _ => {
                cfg!(target_os = "macos")
                    && pressed
                    && self.modifiers.super_key()
                    && self.modifiers.shift_key()
                    && event.physical_key == PhysicalKey::Code(KeyCode::KeyS)
            }
        };
        if shot {
            self.request_screenshot();
            return;
        }
        if event.state != ElementState::Pressed || event.logical_key == Key::Named(NamedKey::PrintScreen) {
            return;
        }
        let key = &event.logical_key;
        if let Scene::Game(game) = &mut self.scene
            && game.shortcuts_window.as_ref().is_some_and(|window| window.listening.is_some())
        {
            if event.repeat {
                return;
            }
            if let Some(key) = shortcut_key(key) {
                let slot = game.shortcuts_window.as_ref().unwrap().listening;
                if game.shortcuts_window.as_mut().unwrap().bind_key(key, &mut self.shortcuts) {
                    self.shortcuts.save();
                    if let Some(slot) = slot {
                        self.keybinds.set_hotbar(slot, shortcut_to_bind(key));
                    }
                }
                return;
            }
            if *key == Key::Named(NamedKey::Escape) {
                game.shortcuts_window.as_mut().unwrap().listening = None;
                return;
            }
        }
        // Ctrl+Cmd+F, as Macs have it: fullscreen (the canvas widened to the screen's shape), or back. Alt+Enter is no
        // shortcut in the game: it reaches it as Enter with Alt held.
        // ponytail: no fullscreen key off the Mac; the graphics settings are where one would go.
        let fullscreen =
            cfg!(target_os = "macos") && self.modifiers.super_key() && self.modifiers.control_key() && event.physical_key == PhysicalKey::Code(KeyCode::KeyF);
        if fullscreen && let Some(gpu) = &self.gpu {
            gpu.window.set_fullscreen(gpu.window.fullscreen().is_none().then_some(winit::window::Fullscreen::Borderless(None)));
            return;
        }
        if self.fire_shortcut(event) {
            return;
        }
        match &mut self.scene {
            Scene::Login => {
                self.blink_start = Instant::now();
                match key {
                    Key::Named(NamedKey::Tab) => self.focus ^= 1,
                    Key::Named(NamedKey::Enter) => self.submit_login(),
                    Key::Named(NamedKey::Escape) => self.escape_before_map(),
                    Key::Named(NamedKey::Backspace) => {
                        self.fields[self.focus].pop();
                    }
                    _ => {
                        let field = &mut self.fields[self.focus];
                        for character in event.text.as_deref().unwrap_or_default().chars().filter(char::is_ascii_graphic) {
                            if field.len() < NAME_LENGTH - 1 {
                                field.push(character);
                            }
                        }
                    }
                }
            }
            Scene::Connections { selected } => match key {
                Key::Named(NamedKey::ArrowUp) => *selected = selected.saturating_sub(1),
                Key::Named(NamedKey::ArrowDown) => *selected = (*selected + 1).min(self.connections.len().saturating_sub(1)),
                Key::Named(NamedKey::Enter) => {
                    let picked = *selected;
                    self.choose_connection(picked);
                }
                Key::Named(NamedKey::Escape) => self.escape_before_map(),
                _ => {}
            },
            Scene::ServerSelect { accepted, selected, scroll } => match key {
                Key::Named(NamedKey::ArrowUp) => {
                    *selected = selected.saturating_sub(1);
                    *scroll = servers::keep_visible(*selected, *scroll);
                }
                Key::Named(NamedKey::ArrowDown) => {
                    *selected = (*selected + 1).min(accepted.char_servers.len().saturating_sub(1));
                    *scroll = servers::keep_visible(*selected, *scroll);
                }
                Key::Named(NamedKey::Enter) => self.confirm_server(),
                Key::Named(NamedKey::Escape) => self.escape_before_map(),
                _ => {}
            },
            Scene::CharSelect(select) if select.pin_pad.is_some() => {
                let pad = select.pin_pad.as_mut().unwrap();
                // ponytail: the official pad is mouse-only (message 1906); also accept digit keys so scenarios can `type` a PIN.
                let action = match key {
                    Key::Named(NamedKey::Backspace) => {
                        pad.backspace();
                        pin::PinAction::None
                    }
                    Key::Named(NamedKey::Enter) => pad.press(pin::PinButton::Ok),
                    Key::Named(NamedKey::Escape) => pad.press(pin::PinButton::Cancel),
                    _ => event
                        .text
                        .as_deref()
                        .and_then(|text| text.chars().next())
                        .filter(char::is_ascii_digit)
                        .map_or(pin::PinAction::None, |digit| pad.push_digit(digit)),
                };
                self.apply_pin_action(action);
            }
            Scene::CharSelect(select) => match key {
                Key::Named(NamedKey::ArrowLeft) => select.step(-1),
                Key::Named(NamedKey::ArrowRight) => select.step(1),
                Key::Named(NamedKey::ArrowUp) => select.step(-char_select::Select::ROW),
                Key::Named(NamedKey::ArrowDown) => select.step(char_select::Select::ROW),
                Key::Named(NamedKey::Enter) => {
                    if let Some(action) = select.enter() {
                        self.select_action(action);
                    }
                }
                // Escape closes the card's menu or stops picking where to move first.
                Key::Named(NamedKey::Escape) if select.menu.is_some() || select.moving.is_some() => {
                    select.menu = None;
                    select.moving = None;
                }
                Key::Named(NamedKey::Escape) => self.escape_before_map(),
                _ => {}
            },
            Scene::CharMake(make, _) => match key {
                Key::Named(NamedKey::Enter) => {
                    if let Some(action) = make.click(char_make::Hit::Make, &self.grf, &self.look_tables) {
                        self.make_action(action);
                    }
                }
                Key::Named(NamedKey::Escape) => self.escape_before_map(),
                Key::Named(NamedKey::Backspace) => {
                    make.name.pop();
                }
                _ => make.type_text(event.text.as_deref().unwrap_or_default()),
            },
            // Esc, one window per press, and never on a key that is auto-repeating.
            Scene::Game(_) if *key == Key::Named(NamedKey::Escape) => {
                if !event.repeat {
                    self.game_escape();
                }
            }
            Scene::Game(game) if game.dialog.is_some() => {
                let dialog = game.dialog.as_mut().unwrap();
                match (key, &mut dialog.input, &mut dialog.menu) {
                    (Key::Named(NamedKey::Enter), Some(_), _) => game.submit_input(),
                    (Key::Named(NamedKey::Backspace), Some(input), _) => input.backspace(),
                    (_, Some(input), _) => input.type_text(event.text.as_deref().unwrap_or_default()),
                    (Key::Named(NamedKey::ArrowUp), _, Some(menu)) => menu.step(false),
                    (Key::Named(NamedKey::ArrowDown), _, Some(menu)) => menu.step(true),
                    (Key::Named(NamedKey::Enter), _, Some(_)) => game.menu_ok(),
                    // Without a menu Up and Down scroll the text, and Enter (or Space with the chat line closed)
                    // presses Next or Close.
                    (Key::Named(NamedKey::ArrowUp), None, None) => game.say.scroll_by(-npc_say::SCROLL_STEP),
                    (Key::Named(NamedKey::ArrowDown), None, None) => game.say.scroll_by(npc_say::SCROLL_STEP),
                    (Key::Named(NamedKey::Enter), None, None) => game.press_dialog_button(false),
                    (Key::Named(NamedKey::Space), None, None) if !game.chat_mode.chat_line_open() => game.press_dialog_button(false),
                    _ => {}
                }
            }
            // The question holds the player busy until it's answered.
            Scene::Game(game) if game.bind_asked.is_some() && matches!(key, Key::Named(NamedKey::Enter | NamedKey::Escape)) => {
                game.answer_bind(*key == Key::Named(NamedKey::Enter));
            }
            Scene::Game(game) if game.expansion_asked.is_some() && matches!(key, Key::Named(NamedKey::Enter | NamedKey::Escape)) => {
                game.expansion_asked = None;
                let _ = game.session.answer_expansion(*key == Key::Named(NamedKey::Enter));
            }
            Scene::Game(game) if game.apply_asked && matches!(key, Key::Named(NamedKey::Enter | NamedKey::Escape)) => {
                game.answer_apply(*key == Key::Named(NamedKey::Enter));
            }
            // Tab over the skill window: its next tab, round to the first.
            // ponytail: with the pointer over the window; the game's own focus rule isn't known.
            Scene::Game(game) if *key == Key::Named(NamedKey::Tab) && game.window_at(self.cursor) == Some(Panel::Skills) => {
                let jobs = skills::tabs(&self.skill_trees, game.me.job, &game.skills);
                let sheet = skills::Sheet {
                    trees: &self.skill_trees,
                    jobs: &jobs,
                    skills: &game.skills.clone(),
                    info: &game.skill_info.clone(),
                    points: game.skill_points(),
                };
                game.skill_window.next_tab(&sheet);
            }
            Scene::Game(game) if game.leader_asked.is_some() && matches!(key, Key::Named(NamedKey::Enter | NamedKey::Escape)) => {
                let account = game.leader_asked.take().unwrap();
                if *key == Key::Named(NamedKey::Enter) {
                    let _ = game.session.change_party_leader(account);
                }
            }
            Scene::Game(game) if game.question_box.is_some() && matches!(key, Key::Named(NamedKey::Enter | NamedKey::Escape)) => {
                game.answer_box(*key == Key::Named(NamedKey::Enter), &self.messages);
            }
            // The guild window's text boxes: Enter or Esc leaves the box.
            Scene::Game(game) if game.guild_window.typing.is_some() && game.reason_box.is_none() => match key {
                Key::Named(NamedKey::Enter | NamedKey::Escape) => game.guild_window.typing = None,
                Key::Named(NamedKey::Backspace) => {
                    if let Some(guild) = &game.guild {
                        game.guild_window.backspace(guild);
                    }
                }
                _ => {
                    if let Some(guild) = &game.guild {
                        game.guild_window.type_text(guild, event.text.as_deref().unwrap_or_default());
                    }
                }
            },
            // The create (or disband) window's box: Enter is OK, Esc closes the window.
            Scene::Game(game) if game.guild_companion.typing() => match key {
                Key::Named(NamedKey::Enter) => game.guild_name_ok(&self.messages),
                Key::Named(NamedKey::Escape) => game.guild_companion.create = None,
                Key::Named(NamedKey::Backspace) => drop(game.guild_companion.create.as_mut().unwrap().name.pop()),
                _ => game.guild_companion.create.as_mut().unwrap().type_text(event.text.as_deref().unwrap_or_default()),
            },
            // The guild's reason box: Enter is OK, Esc cancels.
            Scene::Game(game) if game.reason_box.is_some() => match key {
                Key::Named(NamedKey::Enter) => game.answer_reason(true),
                Key::Named(NamedKey::Escape) => game.answer_reason(false),
                Key::Named(NamedKey::Backspace) => drop(game.reason_box.as_mut().unwrap().text.pop()),
                _ => game.reason_box.as_mut().unwrap().type_text(event.text.as_deref().unwrap_or_default()),
            },
            // The master's pending guild moves: Enter is OK.
            Scene::Game(game) if game.guild_window.frame.open && !game.guild_window.pending.is_empty() && *key == Key::Named(NamedKey::Enter) => {
                if let Some(moves) = game.guild_window.take_moves(Instant::now()) {
                    let _ = game.session.move_members(&moves);
                }
            }
            // A docked party window's edit box: Enter is OK, Esc closes the window.
            Scene::Game(game) if game.party_dialog.as_ref().is_some_and(|dialog| dialog.typing) => {
                let dialog = game.party_dialog.as_mut().unwrap();
                match key {
                    Key::Named(NamedKey::Enter) => {
                        let action = dialog.ok();
                        game.party_dialog_action(action, &self.messages);
                    }
                    Key::Named(NamedKey::Escape) => game.party_dialog = None,
                    Key::Named(NamedKey::Backspace) => drop(dialog.name.pop()),
                    _ => dialog.type_text(event.text.as_deref().unwrap_or_default()),
                }
            }
            Scene::Game(game) if game.party_dialog.is_some() && *key == Key::Named(NamedKey::Escape) => game.party_dialog = None,
            Scene::Game(game) if game.ranking.is_some() && *key == Key::Named(NamedKey::Escape) => game.ranking = None,
            Scene::Game(game) if game.dressroom.is_some() && *key == Key::Named(NamedKey::Escape) => {
                game.dressroom = None;
                game.dress_figure = None;
            }
            Scene::Game(game) if game.dismiss_asked && matches!(key, Key::Named(NamedKey::Enter | NamedKey::Escape)) => {
                game.answer_dismiss(*key == Key::Named(NamedKey::Enter), &self.messages);
            }
            Scene::Game(game) if game.homunculus_asked.is_some() && matches!(key, Key::Named(NamedKey::Enter | NamedKey::Escape)) => {
                game.answer_homunculus(*key == Key::Named(NamedKey::Enter), &self.messages);
            }
            Scene::Game(game) if game.pet_asked.is_some() && matches!(key, Key::Named(NamedKey::Enter | NamedKey::Escape)) => {
                game.answer_pet(*key == Key::Named(NamedKey::Enter), &self.pets, &self.items, &self.messages);
            }
            Scene::Game(game) if game.cashshop_buy_asked && matches!(key, Key::Named(NamedKey::Enter | NamedKey::Escape)) => {
                game.cashshop_buy_asked = false;
                if let Some((free, items)) = game.cashshop.as_ref().and_then(|shop| shop.buy_request(*key == Key::Named(NamedKey::Enter))) {
                    let _ = game.session.buy_cashshop(free, &items);
                }
            }
            Scene::Game(game) if game.amount.is_some() => match key {
                Key::Named(NamedKey::Enter) => game.answer_amount(true, &self.messages),
                Key::Named(NamedKey::Escape) => game.answer_amount(false, &self.messages),
                Key::Named(NamedKey::Backspace) => {
                    game.amount.as_mut().unwrap().typed.pop();
                }
                _ => game.amount.as_mut().unwrap().type_text(event.text.as_deref().unwrap_or_default()),
            },
            Scene::Game(game) if game.vend_setup.as_ref().is_some_and(|setup| setup.typing) && game.amount.is_none() => {
                let setup = game.vend_setup.as_mut().unwrap();
                match key {
                    Key::Named(NamedKey::Enter | NamedKey::Escape) => setup.typing = false,
                    Key::Named(NamedKey::Backspace) => {
                        setup.name.pop();
                    }
                    _ => setup.type_text(event.text.as_deref().unwrap_or_default()),
                }
            }
            Scene::Game(game) if game.buying_setup.as_ref().is_some_and(|setup| setup.focus.is_some()) && game.amount.is_none() => {
                let setup = game.buying_setup.as_mut().unwrap();
                match key {
                    Key::Named(NamedKey::Enter | NamedKey::Escape) => setup.focus = None,
                    Key::Named(NamedKey::Tab) => {
                        setup.focus = match setup.focus {
                            Some(buying::Focus::Name) => Some(buying::Focus::Limit),
                            _ => Some(buying::Focus::Name),
                        };
                    }
                    Key::Named(NamedKey::Backspace) => setup.backspace(),
                    _ => setup.type_text(event.text.as_deref().unwrap_or_default()),
                }
            }
            Scene::Game(game) if game.mail.typing() && game.amount.is_none() => match key {
                Key::Named(NamedKey::Escape) => game.mail.stop_typing(),
                Key::Named(NamedKey::Backspace) => game.mail.backspace(),
                Key::Named(NamedKey::Enter) => game.mail.enter(false),
                Key::Named(NamedKey::Tab) => game.mail.enter(true),
                _ => game.mail.type_text(event.text.as_deref().unwrap_or_default()),
            },
            // A macro line being edited takes the typing; Enter or Esc ends it.
            Scene::Game(game) if game.macro_window.as_ref().is_some_and(|window| window.editing.is_some()) => {
                let window = game.macro_window.as_mut().unwrap();
                match key {
                    Key::Named(NamedKey::Escape | NamedKey::Enter) => {
                        window.editing = None;
                        macros::save(macros::path().as_deref(), &game.macro_lines);
                    }
                    Key::Named(NamedKey::Backspace) => window.backspace(&mut game.macro_lines),
                    _ => window.type_text(&mut game.macro_lines, event.text.as_deref().unwrap_or_default()),
                }
            }
            Scene::Game(game) if game.room_make.is_some() => {
                let make = game.room_make.as_mut().unwrap();
                match key {
                    Key::Named(NamedKey::Escape) => game.room_make = None,
                    Key::Named(NamedKey::Enter) => game.make_room(),
                    Key::Named(NamedKey::Tab) => make.on_password = !make.on_password && !make.public,
                    Key::Named(NamedKey::Backspace) => make.backspace(),
                    _ => make.type_text(event.text.as_deref().unwrap_or_default()),
                }
            }
            // In a chat room, typing goes to its line; Enter says it there.
            Scene::Game(game) if game.room.is_some() && game.amount.is_none() => {
                let room = game.room.as_mut().unwrap();
                match key {
                    Key::Named(NamedKey::Enter) => {
                        let text = std::mem::take(&mut room.input);
                        if !text.trim().is_empty() {
                            let _ = game.session.chat(&game.me.name, text.trim());
                        }
                    }
                    Key::Named(NamedKey::Backspace) => {
                        room.input.pop();
                    }
                    Key::Named(NamedKey::Escape) => room.input.clear(),
                    _ if !self.modifiers.alt_key() && !self.modifiers.super_key() && !self.modifiers.control_key() => {
                        room.input.extend(event.text.as_deref().unwrap_or_default().chars().filter(|character| !character.is_control()));
                    }
                    _ => {}
                }
            }
            Scene::Game(game) if game.cashshop.as_ref().is_some_and(|shop| shop.typing) => {
                let shop = game.cashshop.as_mut().unwrap();
                match key {
                    Key::Named(NamedKey::Enter | NamedKey::Escape) => shop.typing = false,
                    Key::Named(NamedKey::Backspace) => shop.backspace(),
                    _ => shop.type_text(event.text.as_deref().unwrap_or_default()),
                }
            }
            Scene::Game(game) if game.bank.as_ref().is_some_and(|bank| bank.typing) => {
                let bank = game.bank.as_mut().unwrap();
                match key {
                    Key::Named(NamedKey::Enter | NamedKey::Escape) => bank.typing = false,
                    Key::Named(NamedKey::Backspace) => {
                        bank.typed.pop();
                    }
                    _ => bank.type_text(event.text.as_deref().unwrap_or_default()),
                }
            }
            Scene::Game(game) if game.trade.as_ref().is_some_and(|trade| trade.typing) => {
                let trade = game.trade.as_mut().unwrap();
                match key {
                    Key::Named(NamedKey::Enter | NamedKey::Escape) => trade.typing = false,
                    Key::Named(NamedKey::Backspace) => {
                        trade.typed.pop();
                    }
                    _ => {
                        if let Some(id) = trade.type_text(event.text.as_deref().unwrap_or_default()) {
                            let line = rustnarok_client::messages::fill(self.messages.get(id).map_or("", String::as_str), &[&"1,000,000,000"]);
                            game.chat_line(line, CHAT_ERROR);
                        }
                    }
                }
            }
            // The booking board's fields.
            Scene::Game(game) if game.booking.as_ref().is_some_and(|window| window.focus.is_some()) => {
                let window = game.booking.as_mut().unwrap();
                match key {
                    Key::Named(NamedKey::Enter | NamedKey::Escape) => window.focus = None,
                    Key::Named(NamedKey::Backspace) => window.backspace(),
                    _ => window.type_text(event.text.as_deref().unwrap_or_default()),
                }
            }
            // The navigation window's search field: each change searches again.
            Scene::Game(game) if game.navi_window.as_ref().is_some_and(|window| window.typing) => {
                let window = game.navi_window.as_mut().unwrap();
                match key {
                    Key::Named(NamedKey::Enter | NamedKey::Escape) => window.typing = false,
                    Key::Named(NamedKey::Backspace) => {
                        window.text.pop();
                    }
                    _ => window.text.push_str(event.text.as_deref().unwrap_or_default()),
                }
                window.search(&self.navi_tables);
            }
            // The quest search box: Enter applies it, Esc leaves it.
            Scene::Game(game) if game.quest_window.typing => match key {
                Key::Named(NamedKey::Enter) => game.quest_window.apply_search(),
                Key::Named(NamedKey::Escape) => game.quest_window.typing = false,
                Key::Named(NamedKey::Backspace) => game.quest_window.backspace(),
                _ => game.quest_window.type_text(event.text.as_deref().unwrap_or_default()),
            },
            // The pet's name box: Enter keeps what's typed, Esc puts the name back.
            Scene::Game(game) if game.pet_window.typing => match key {
                Key::Named(NamedKey::Enter) => game.pet_window.typing = false,
                Key::Named(NamedKey::Escape) => {
                    game.pet_window.name_text = None;
                    game.pet_window.typing = false;
                }
                Key::Named(NamedKey::Backspace) => {
                    game.pet_window.name_text.get_or_insert_with(String::new).pop();
                }
                _ => {
                    let typed: String = event.text.as_deref().unwrap_or_default().chars().filter(|character| !character.is_control()).collect();
                    game.pet_window.name_text.get_or_insert_with(String::new).push_str(&typed);
                }
            },
            // The homunculus's name box, the same.
            Scene::Game(game) if game.homunculus_window.typing => match key {
                Key::Named(NamedKey::Enter) => game.homunculus_window.typing = false,
                Key::Named(NamedKey::Escape) => {
                    game.homunculus_window.name_text = None;
                    game.homunculus_window.typing = false;
                }
                Key::Named(NamedKey::Backspace) => {
                    game.homunculus_window.name_text.get_or_insert_with(String::new).pop();
                }
                _ => {
                    let typed: String = event.text.as_deref().unwrap_or_default().chars().filter(|character| !character.is_control()).collect();
                    game.homunculus_window.name_text.get_or_insert_with(String::new).push_str(&typed);
                }
            },
            Scene::Game(game) if game.storage.typing => match key {
                Key::Named(NamedKey::Enter) => game.storage.typing = false,
                Key::Named(NamedKey::Escape) => {
                    game.storage.search.clear();
                    game.storage.typing = false;
                }
                Key::Named(NamedKey::Backspace) => {
                    game.storage.search.pop();
                }
                _ => game.storage.type_text(event.text.as_deref().unwrap_or_default()),
            },
            Scene::Game(game) if game.catalog.is_some() && matches!(key, Key::Named(NamedKey::Escape)) => game.close_catalog(),
            Scene::Game(game) if game.catalog.as_ref().is_some_and(|catalog| catalog.focus.is_some()) => {
                let catalog = game.catalog.as_mut().unwrap();
                match key {
                    Key::Named(NamedKey::Enter) => game.click_catalog(catalog::Hit::Search, &self.items, &self.messages),
                    Key::Named(NamedKey::Tab) => {
                        catalog.focus = Some(match catalog.focus {
                            Some(catalog::Field::Name) => catalog::Field::Min,
                            Some(catalog::Field::Min) => catalog::Field::Max,
                            _ => catalog::Field::Name,
                        })
                    }
                    Key::Named(NamedKey::Backspace) => catalog.backspace(),
                    _ => catalog.type_text(event.text.as_deref().unwrap_or_default()),
                }
            }
            // Naming a memo on the big minimap. Esc cancels it (game_escape) before this.
            Scene::Game(game) if game.minimap_window.naming() => match key {
                Key::Named(NamedKey::Enter) => {
                    game.minimap_window.store();
                }
                Key::Named(NamedKey::Backspace) => game.minimap_window.backspace(),
                _ => game.minimap_window.type_text(event.text.as_deref().unwrap_or_default()),
            },
            Scene::Game(game) if game.deal_choice.is_some() && matches!(key, Key::Named(NamedKey::Escape)) => game.deal_choice = None,
            // Enter is the deal-type box's default button: Buy.
            Scene::Game(game) if game.deal_choice.is_some() && matches!(key, Key::Named(NamedKey::Enter)) => {
                let npc = game.deal_choice.take().unwrap();
                let _ = game.session.choose_deal(npc, false);
            }
            // Earlier arms are windows that want the key. Esc is game_escape, which closes an open battle-mode
            // line first. Here the chat line has the key. Off: the line is always open and letters type. On: Enter
            // opens it, Enter on an empty line closes it, and a closed line sends letters and digits to shortcuts.
            Scene::Game(game) if game.away_prompt.is_some() => match key {
                Key::Named(NamedKey::Enter) => {
                    let text = game.away_prompt.take().map(|prompt| prompt.typed).unwrap_or_default();
                    game.finish_away(&text);
                }
                Key::Named(NamedKey::Backspace) => {
                    if let Some(prompt) = &mut game.away_prompt {
                        prompt.typed.pop();
                    }
                }
                _ if !self.modifiers.alt_key() && !self.modifiers.super_key() && !self.modifiers.control_key() => {
                    if let Some(prompt) = &mut game.away_prompt {
                        prompt.type_text(event.text.as_deref().unwrap_or_default());
                    }
                }
                _ => {}
            },
            Scene::Game(game) if game.talks.typing() => match key {
                Key::Named(NamedKey::Enter) => game.send_focused_talk(),
                Key::Named(NamedKey::Backspace) => game.talks.backspace(),
                _ if !self.modifiers.alt_key() && !self.modifiers.super_key() && !self.modifiers.control_key() => {
                    game.talks.type_text(event.text.as_deref().unwrap_or_default());
                }
                _ => {}
            },
            Scene::Game(game) => {
                let kind = battle_key(key);
                // Only a box with nothing in it is empty: spaces are text.
                let take = game.chat_mode.take_key(kind, game.chat.input.is_empty(), game.chat.naming);
                match take {
                    battle_mode::Take::Opened => game.chat.reopened(),
                    battle_mode::Take::Closed => game.chat.close_battle_line(),
                    battle_mode::Take::Shortcut | battle_mode::Take::Type => {
                        let typing = take == battle_mode::Take::Type;
                        // A key on the closed line that no shortcut took empties what Esc left in the box.
                        if !typing && !matches!(key, Key::Named(NamedKey::Shift | NamedKey::Control | NamedKey::Alt | NamedKey::Super)) {
                            game.chat.drop_stale();
                        }
                        // shortcuts::action already ran. A letter it fired never reaches here. An unbound letter
                        // while the line is closed is not typed either.
                        if typing || !matches!(kind, battle_mode::KeyKind::Char(_)) {
                            match key {
                                Key::Named(NamedKey::Tab) if typing => game.chat.naming = !game.chat.naming,
                                // Shift+Insert pastes, Shift+Delete cuts; Ctrl (or Cmd) with A, C, X, V edit.
                                Key::Named(NamedKey::Insert) if typing && self.modifiers.shift_key() => game.chat.type_text(&clipboard_line()),
                                Key::Named(NamedKey::Delete) if typing && self.modifiers.shift_key() => set_clipboard(&game.chat.cut()),
                                Key::Character(letter)
                                    if typing
                                        && (self.modifiers.control_key() || self.modifiers.super_key())
                                        && matches!(letter.to_lowercase().as_str(), "a" | "c" | "x" | "v") =>
                                {
                                    match letter.to_lowercase().as_str() {
                                        "a" => game.chat.select_all(),
                                        "c" => set_clipboard(&game.chat.selected()),
                                        "x" => set_clipboard(&game.chat.cut()),
                                        _ => game.chat.type_text(&clipboard_line()),
                                    }
                                }
                                Key::Named(NamedKey::Insert) => game.toggle_sit(),
                                // The caret: Left, Right, Home, End (Shift selects); Up / Down recall.
                                Key::Named(named @ (NamedKey::ArrowLeft | NamedKey::ArrowRight | NamedKey::Home | NamedKey::End)) if typing => {
                                    let to = match named {
                                        NamedKey::ArrowLeft => line_edit::Move::Left,
                                        NamedKey::ArrowRight => line_edit::Move::Right,
                                        NamedKey::Home => line_edit::Move::Home,
                                        _ => line_edit::Move::End,
                                    };
                                    game.chat.go(to, self.modifiers.shift_key());
                                }
                                Key::Named(named @ (NamedKey::ArrowUp | NamedKey::ArrowDown)) if typing => game.chat.recall(*named == NamedKey::ArrowUp),
                                Key::Named(NamedKey::Delete) if typing => game.chat.delete(),
                                // PageUp / PageDown page the log; with Ctrl, to its top or bottom.
                                Key::Named(key @ (NamedKey::PageUp | NamedKey::PageDown)) if game.chat_mode.chat_line_open() => {
                                    game.chat.page(*key == NamedKey::PageDown, self.modifiers.control_key());
                                }
                                // Enter sends from either box.
                                Key::Named(NamedKey::Enter) if typing => self.send_chat_input(self.modifiers.control_key(), self.modifiers.alt_key()),
                                Key::Named(NamedKey::Backspace) if typing => game.chat.backspace(),
                                _ if typing && !self.modifiers.alt_key() && !self.modifiers.super_key() && !self.modifiers.control_key() => {
                                    let typed: String = event.text.as_deref().unwrap_or_default().chars().filter(|character| !character.is_control()).collect();
                                    game.chat.type_text(&typed);
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
            Scene::Message(..) => {
                if matches!(key, Key::Named(NamedKey::Enter | NamedKey::Escape)) {
                    self.close_message();
                }
            }
            Scene::Prompt { input, then, .. } if !matches!(key, Key::Named(NamedKey::Enter | NamedKey::Escape)) => {
                if matches!(key, Key::Named(NamedKey::Backspace)) {
                    input.pop();
                }
                let text = event.text.as_deref().unwrap_or_default();
                // A new name takes any letters; the birthdate only digits.
                if let Then::ConfirmName(_) = then {
                    let room = NAME_MOST - input.chars().count().min(NAME_MOST);
                    input.extend(text.chars().filter(|character| !character.is_control()).take(room));
                } else {
                    input.extend(text.chars().filter(char::is_ascii_digit).take(PROMPT_LENGTH - input.len().min(PROMPT_LENGTH)));
                }
            }
            Scene::Confirm(..) | Scene::Prompt { .. } => match key {
                Key::Named(NamedKey::Enter) => self.answer(),
                Key::Named(NamedKey::Escape) => self.dismiss(),
                _ => {}
            },
            Scene::Waiting(..) | Scene::Quit => {}
        }
    }
}
