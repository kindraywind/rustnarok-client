//! What the servers send: the login, char and map servers' events, entering and changing maps.

use super::*;

impl App {
    pub(crate) fn net_event(&mut self, event: NetEvent) {
        match event {
            // An answer after Cancel or the time limit.
            NetEvent::Login(_) if !matches!(self.scene, Scene::Waiting(Wait::Login, _)) => {}
            NetEvent::Login(Ok(accepted)) => {
                // Dev runs (RUSTNAROK_CLIENT_LOGIN) leave the saved ID alone.
                if std::env::var_os("RUSTNAROK_CLIENT_LOGIN").is_none() {
                    remember_id(self.save_id.then_some(self.fields[0].as_str()));
                }
                let selected = servers::clamped_row(saved_server(), accepted.char_servers.len());
                let scroll = servers::keep_visible(selected, 0);
                self.server_clicks = window::Clicks::default();
                self.server_list_opened = Instant::now();
                self.scene = Scene::ServerSelect { accepted, selected, scroll };
                if std::env::var_os("RUSTNAROK_CLIENT_AUTOSTART").is_some() {
                    self.enter_char_server();
                }
            }
            NetEvent::Login(Err(error)) => {
                let (text, back) = match &error {
                    LoginError::Refused(code, date) => {
                        (login_refusal_text(*code, date, &self.messages), if login_refusal_quits(*code) { Scene::Quit } else { Scene::Login })
                    }
                    LoginError::Notified(code) => (Some(self.message_text(ban_message(*code))), Scene::Login),
                    LoginError::Io(_) => (Some(self.message_text(CONNECT_FAILED)), Scene::Login),
                };
                match text {
                    Some(text) => self.message(text, back),
                    None => self.scene = back,
                }
            }
            // Events from a session already closed by going back to login.
            NetEvent::Char(CharEvent::Disconnected(_)) if self.char_session.is_none() => {}
            NetEvent::Char(_) if self.char_session.is_none() => {}
            NetEvent::Char(CharEvent::Characters { slots, characters }) => {
                let figures = characters.iter().map(|character| SpriteSet::player(&self.grf, &character_look(character), &self.look_tables)).collect();
                let mut select = char_select::Select::new(characters, figures, usize::from(slots));
                // A list sent again (after a deletion) keeps the cursor and PIN verification where they were.
                if let Scene::CharSelect(previous) = self.scene.base() {
                    select.selected = previous.selected;
                    select.pin_verified = previous.pin_verified;
                } else if let Some(saved) =
                    settings_path("char_slot").and_then(|path| std::fs::read_to_string(path).ok()).and_then(|text| text.trim().parse().ok())
                {
                    // The last card picked, kept between runs (the character just played, back from the game).
                    select.selected = char_select::opening_slot(saved, select.pages());
                }
                let autostart = std::env::var("RUSTNAROK_CLIENT_AUTOSTART").ok().and_then(|slot| slot.parse::<usize>().ok());
                // Only the first list: later ones (after a move or a rename) keep the player's selection.
                if let Some(slot) = autostart
                    && !self.scripted
                {
                    select.selected = slot;
                }
                let verified = select.pin_verified;
                match &mut self.scene {
                    // After a rename its message stays up over the new list.
                    Scene::Message(_, back) if matches!(**back, Scene::CharSelect(_)) => **back = Scene::CharSelect(Box::new(select)),
                    scene => *scene = Scene::CharSelect(Box::new(select)),
                }
                // Autostart and MAKE/DELETE wait for PIN state 0 (`finish_char_select_entry`).
                if verified {
                    self.finish_char_select_entry();
                }
            }
            NetEvent::Char(CharEvent::DeleteReserved { id, result, seconds }) => {
                self.settle();
                match (result, &mut self.scene) {
                    (1, Scene::CharSelect(select)) => select.set_deletion(id, Some(seconds)),
                    // Already reserved, a database error, in a guild, in a party, and the rest; 2 says nothing.
                    (code, _) => {
                        if let Some(line) = char_select::reserve_answer(code) {
                            let text = self.message_text(line);
                            let back = std::mem::replace(&mut self.scene, Scene::Login);
                            self.message(text, back);
                        }
                    }
                }
            }
            NetEvent::Char(CharEvent::DeleteCanceled { id, result }) => {
                self.settle();
                match (result, &mut self.scene) {
                    (1, Scene::CharSelect(select)) => select.set_deletion(id, None),
                    _ => {
                        let back = std::mem::replace(&mut self.scene, Scene::Login);
                        self.message(self.message_text(1817), back);
                    }
                }
            }
            NetEvent::Char(CharEvent::Moved { result, .. }) => {
                self.settle();
                let Scene::CharSelect(select) = &mut self.scene else { return };
                let to = select.moved_to.take();
                match (result, to) {
                    // The list that follows keeps the selection, so it follows the character there.
                    (0, Some(to)) => select.selected = to,
                    (0, None) => {}
                    _ => {
                        let back = std::mem::replace(&mut self.scene, Scene::Login);
                        self.message(self.message_text(MOVE_FAILED), back);
                    }
                }
            }
            NetEvent::Char(CharEvent::Renamed(result)) => {
                self.settle();
                // Renamed, renamed before, bad account, failed, name taken, in a guild, in a party, too long, symbols,
                // prohibited, unknown.
                let text = self.message_text(match result {
                    0 => 1214,
                    1 => 1218,
                    2 => 1219,
                    3 => 1215,
                    4 => 1220,
                    5 => 2022,
                    6 => 2023,
                    7 => 2027,
                    8 => 2028,
                    9 => 2029,
                    _ => 2024,
                });
                let back = std::mem::replace(&mut self.scene, Scene::Login);
                self.message(text, back);
            }
            // A deletion the server made comes after the new character list, which already replaced the wait.
            NetEvent::Char(CharEvent::Deleted { result: 1, .. }) => {}
            NetEvent::Char(CharEvent::Deleted { result, .. }) => {
                self.settle();
                let text = self.message_text(char_select::delete_answer(result));
                let back = std::mem::replace(&mut self.scene, Scene::Login);
                self.message(text, back);
            }
            NetEvent::Char(CharEvent::Made(character)) => {
                self.settle();
                if let Scene::CharMake(_, mut select) = std::mem::replace(&mut self.scene, Scene::Login) {
                    let figure = SpriteSet::player(&self.grf, &character_look(&character), &self.look_tables);
                    select.add(character, figure);
                    self.scene = Scene::CharSelect(select);
                }
            }
            NetEvent::Char(CharEvent::MakeRefused(code)) => {
                self.settle();
                // Name taken, underage, symbols in the name, slot not available, refused.
                let text = self.message_text(match code {
                    0 => 10,
                    1 => 298,
                    2 => 1272,
                    3 => 1355,
                    _ => 11,
                });
                let back = after_make_refused(std::mem::replace(&mut self.scene, Scene::Login));
                self.message(text, back);
            }
            NetEvent::Char(CharEvent::ZoneServer { map, ip, port, character_id }) => {
                self.char_session = None;
                let (Some(account), Some(_)) = (&self.account, &self.playing) else { return };
                let ticket = MapTicket { address: (ip, port), account_id: account.account_id, character_id, login_id1: account.login_id1, sex: account.sex };
                let proxy = self.proxy.clone();
                match MapSession::connect(&ticket, self.client_tick(), move |event| {
                    let _ = proxy.send_event(NetEvent::Map(event));
                }) {
                    Ok(session) => {
                        self.entering = Some((session, map));
                        self.early_events.clear();
                        // From here the map server's limit counts.
                        if let Scene::Waiting(wait, _) = &mut self.scene {
                            *wait = Wait::Map;
                        }
                        self.wait_began = Instant::now();
                    }
                    Err(_) => self.message("맵 서버에 접속할 수 없습니다.", Scene::Login),
                }
            }
            NetEvent::Char(CharEvent::Pin { seed, state }) => {
                self.settle();
                let Scene::CharSelect(select) = self.scene.base_mut() else { return };
                let outcome = pin::PinPad::apply_server(&mut select.pin_pad, seed, state);
                match outcome {
                    pin::PinServerOutcome::Passed { message } => {
                        select.pin_verified = true;
                        self.finish_char_select_entry();
                        if let Some(id) = message
                            && matches!(self.scene, Scene::CharSelect(_))
                        {
                            let text = self.message_text(id);
                            let back = std::mem::replace(&mut self.scene, Scene::Login);
                            self.message(text, back);
                        }
                    }
                    pin::PinServerOutcome::Prompt => {
                        select.pin_verified = false;
                        if state == 1 && !select.auto_pin_tried {
                            select.auto_pin_tried = true;
                            if let Ok(auto_pin) = std::env::var("RUSTNAROK_CLIENT_PIN")
                                && auto_pin.len() == 4
                                && auto_pin.bytes().all(|byte| byte.is_ascii_digit())
                                && let Some(pad) = &mut select.pin_pad
                            {
                                let mut action = pin::PinAction::None;
                                for digit in auto_pin.chars() {
                                    action = pad.push_digit(digit);
                                }
                                self.apply_pin_action(action);
                            }
                        }
                    }
                    pin::PinServerOutcome::Message { id, tries_left } => {
                        select.pin_verified = false;
                        let text = messages::fill(&self.message_text(id), &[&tries_left]);
                        let back = std::mem::replace(&mut self.scene, Scene::Login);
                        self.message(text, back);
                    }
                    pin::PinServerOutcome::LockedOut(id) => {
                        self.char_session = None;
                        let text = self.message_text(id);
                        self.message(text, Scene::Login);
                    }
                }
            }
            NetEvent::Char(CharEvent::Refused(code)) => {
                self.char_session = None;
                let (id, to_list) = char_refusal(code);
                let back = match (&self.account, to_list) {
                    (Some(accepted), true) => {
                        self.server_list_opened = Instant::now();
                        let selected = self.char_server.min(accepted.char_servers.len().saturating_sub(1));
                        Scene::ServerSelect { accepted: accepted.clone(), selected, scroll: servers::keep_visible(selected, 0) }
                    }
                    _ => Scene::Login,
                };
                self.message(self.message_text(id), back);
            }
            NetEvent::Char(CharEvent::Banned(code)) => {
                self.char_session = None;
                self.message(self.message_text(ban_message(code)), Scene::Login);
            }
            NetEvent::Char(CharEvent::Disconnected(_)) => {
                self.char_session = None;
                self.message(self.messages.get(DISCONNECTED_MESSAGE).cloned().unwrap_or_default(), Scene::Login);
            }
            NetEvent::Map(event) => self.map_event(event),
            NetEvent::Control(_) => unreachable!("user_event runs commands"),
        }
    }

    /// Loads the map the map server put us on and starts playing.
    pub(crate) fn enter_map(&mut self, position: [u16; 2], direction: u8) {
        let (Some((mut session, map)), Some(me), Some(account), Some(gpu)) = (self.entering.take(), self.playing.clone(), &self.account, &self.gpu) else {
            return;
        };
        match World::load(gpu, &self.grf, &map) {
            Ok(world) => {
                let sky = sky::for_map(&self.grf, &map, &world, position);
                let _ = session.loaded();
                // The player's own title comes with their own name tag.
                let _ = session.ask_name(account.account_id);
                let look = character_look(&me);
                let sprites = SpriteSet::player(&self.grf, &look, &self.look_tables);
                // The character list's speed until the map server sends one.
                let mut player = Actor::new(me.name.clone(), sprites, position.map(i32::from), direction, if me.speed > 0 { me.speed } else { 150 });
                player.set_map([world.gat.width, world.gat.height]);
                (player.kind, player.job, player.look) = (PLAYER, me.job, Some(look));
                player.sync_companions(me.option, &self.grf);
                let indoor = self.indoor.contains(&map_key(&map));
                let camera_goal = CameraGoal::arrive(indoor, self.viewpoints.get(&map_key(&map)).copied(), self.saved_view.of(indoor), zoom_out_saved());
                let camera = camera_goal.camera(world.cell_position(position.map(f32::from)));
                let now = Instant::now();
                let minimap = minimap::load_map(&self.grf, &map);
                // Until the server sends updates, the character list's values.
                let status = HashMap::from([
                    (net::STATUS_HP, me.hp),
                    (net::STATUS_MAX_HP, me.max_hp),
                    (net::STATUS_SP, me.sp),
                    (net::STATUS_MAX_SP, me.max_sp),
                    (net::STATUS_BASE_LEVEL, i64::from(me.base_level)),
                    (net::STATUS_JOB_LEVEL, i64::from(me.job_level)),
                    (net::STATUS_BASE_EXP, me.exp),
                    (net::STATUS_JOB_EXP, me.job_exp),
                    (net::STATUS_ZENY, i64::from(me.zeny)),
                ]);
                let minimap_map = map_key(&map);
                let (ambient, diffuse) = world.light_rgb();
                let scene = world::SceneLight::load(ambient, diffuse);
                self.scene = Scene::Game(Box::new(Game {
                    saved_places: String::new(),
                    session,
                    world,
                    map,
                    me,
                    account_id: account.account_id,
                    player,
                    entered_facing: direction,
                    units: HashMap::new(),
                    camera,
                    camera_goal,
                    minimap,
                    minimap_crop: None,
                    minimap_view: minimap::saved_view(),
                    minimap_zoom: settings_path("minimap_zoom")
                        .and_then(|path| std::fs::read_to_string(path).ok())
                        .and_then(|text| text.trim().parse::<f32>().ok())
                        .map_or(1.0, |zoom| zoom.clamp(1.0, minimap::MOST_ZOOM)),
                    navigation_route: false,
                    minimap_window: minimap_window::Window::new(gpu, &self.grf, &minimap_map),
                    guild_places: HashMap::new(),
                    party_colors: Vec::new(),
                    show_bars: true,
                    last_frame: now,
                    walk_clock: now,
                    chat: chat::Chat::default(),
                    entered: now,
                    last_keepalive: now,
                    shadow: SpriteSet::at(&self.grf, "data\\sprite\\shadow"),
                    emotion_sprite: SpriteSet::at(&self.grf, "data\\sprite\\이팩트\\emotion"),
                    emotions: HashMap::new(),
                    mvp_sprite: SpriteSet::at(&self.grf, "data\\sprite\\이팩트\\mvp"),
                    mvps: HashMap::new(),
                    mark_sprites: ["status-stun", "status-sleep", "status-curse", "얼음땡"]
                        .into_iter()
                        .filter_map(|name| Some((name, SpriteSet::at(&self.grf, &format!("data\\sprite\\이팩트\\{name}"))?)))
                        .collect(),
                    spirits: HashMap::new(),
                    sphere: gpu.try_load_texture(&self.grf, "data\\texture\\effect\\기구.bmp", true).ok(),
                    shield: render::load_image(&self.grf, "data\\texture\\effect\\shield.bmp").ok().map(|image| gpu.texture(&render::glow(image), true)),
                    charm: render::load_image(&self.grf, "data\\texture\\effect\\charm.bmp").ok().map(|image| gpu.texture(&render::glow(image), true)),
                    coin_sprite: SpriteSet::at(&self.grf, "data\\sprite\\이팩트\\동전"),
                    digits: Digits::load(gpu, &self.grf),
                    message_sprite: SpriteSet::at(&self.grf, "data\\sprite\\이팩트\\msg"),
                    dialog: None,
                    say: npc_say::SayWindow::default(),
                    special_popup: None,
                    npc_menu: npc_menu::frame(),
                    hit_boxes: Vec::new(),
                    skill_units: HashMap::new(),
                    ground: HashMap::new(),
                    item_sprites: HashMap::new(),
                    inventory: {
                        let mut inventory = Inventory::new(std::env::var_os("RUSTNAROK_CLIENT_INVENTORY").is_some());
                        inventory.apply_boxes(inventory::Boxes::load());
                        inventory.set_tab_labels(&self.messages, &self.font);
                        inventory
                    },
                    equipment: {
                        let mut equipment = Equipment::default();
                        equipment.frame.open = std::env::var_os("RUSTNAROK_CLIENT_EQUIPMENT").is_some();
                        equipment.set_tab_labels(&self.messages, &self.font);
                        equipment
                    },
                    basic_info: BasicInfo::load(screen()),
                    hotkeys: hotkeys::Hotkeys::default(),
                    dragging: None,
                    drag_cancelled: false,
                    skills: Vec::new(),
                    skill_window: {
                        let mut window = skills::SkillWindow::default();
                        window.set_simple(skills::load_view());
                        window.frame.open = std::env::var_os("RUSTNAROK_CLIENT_SKILLS").is_some();
                        window
                    },
                    targeting: None,
                    capturing: false,
                    slot_sprite: SpriteSet::at(&self.grf, "data\\sprite\\slotmachine"),
                    quest_npc: None,
                    slot: None,
                    casts: HashMap::new(),
                    progress: ProgressBar::default(),
                    opened_page: None,
                    cast_squares: HashMap::new(),
                    skill_lands: HashMap::new(),
                    // ponytail: on at the start by the user's choice; the game starts it off.
                    ground_cursor: true,
                    ground_mark: None,
                    combos: HashMap::new(),
                    locks: Vec::new(),
                    speech: HashMap::new(),
                    status_icons: Vec::new(),
                    status_tip: None,
                    cooldowns: skills::Cooldowns::default(),
                    talkie_text: None,
                    talkie_spots: HashMap::new(),
                    cooldown_line: self.messages.get(COOLDOWN_REFUSED).cloned().unwrap_or_default(),
                    effects: effects::Effects::default(),
                    deal_choice: None,
                    deal_frame: window::Frame::new(shop::deal_box_position(), shop::DEAL_SIZE[0]),
                    shop: None,
                    deal_confirm: None,
                    shop_asked: None,
                    storage: storage::Storage::default(),
                    cutin: None,
                    marks: Vec::new(),
                    quest_marks: Vec::new(),
                    quest_started: HashMap::new(),
                    store_mark: None,
                    amount: None,
                    escape: None,
                    settings_window: None,
                    reset_settings: false,
                    shortcuts_window: None,
                    world_map: None,
                    quests: Vec::new(),
                    quest_window: {
                        let mut window = quest::QuestWindow::load();
                        window.filters = self.quest_filters.clone();
                        window.tracker =
                            settings_path("quest_tracker").and_then(|path| std::fs::read_to_string(path).ok()).is_none_or(|text| text.trim() != "0");
                        window
                    },
                    achievement_totals: net::AchievementTotals::default(),
                    achievements: HashMap::new(),
                    achievement_window: achievement::AchievementWindow::default(),
                    item_window: None,
                    card_window: None,
                    card_place: iteminfo::CARD_PLACE,
                    card_lines: 0,
                    card_view: None,
                    item_lines: 0,
                    quick_moving: false,
                    item_place: iteminfo::DEFAULT_PLACE,
                    whispered: VecDeque::new(),
                    whisper_asked: VecDeque::new(),
                    pick: None,
                    forge: None,
                    card_asked: None,
                    item_skill: None,
                    trade: None,
                    refine: None,
                    refine_clicks: window::Clicks::default(),
                    grade: None,
                    synthesis: None,
                    upgrade: None,
                    reform: None,
                    enchant: None,
                    stylist: None,
                    materials: None,
                    roulette: None,
                    monster_window: None,
                    boss: None,
                    viewing: None,
                    bank: None,
                    cashshop: None,
                    cashshop_lists: vec![Vec::new(); 9],
                    cashshop_list_requested: false,
                    cashshop_buy_asked: false,
                    trade_asked: None,
                    booking: None,
                    booking_web: (
                        web_address(&self.login_server().0),
                        booking::Auth {
                            account: account.account_id,
                            token: account.web_token.clone(),
                            world: account.char_servers.get(self.char_server).map(|server| server.name.clone()).unwrap_or_default(),
                        },
                    ),
                    booking_answers: std::sync::mpsc::channel(),
                    booking_asked: None,
                    trade_with: String::new(),
                    shop_sizes: shop::default_sizes(),
                    no_trade: false,
                    trade_give: VecDeque::new(),
                    trade_give_zeny: None,
                    menu: None,
                    weather: weather::Weather::default(),
                    blind: effects::BlindView::default(),
                    deep_blind: false,
                    scene,
                    sky,
                    right_press: None,
                    party: None,
                    status_window: {
                        let mut window = status::StatusWindow::default();
                        window.frame.open = std::env::var_os("RUSTNAROK_CLIENT_STATUS").is_some();
                        window
                    },
                    status_height: 0.0,
                    party_window: {
                        let mut window = party::PartyWindow::default();
                        let order = std::env::var("RUSTNAROK_CLIENT_PARTY").unwrap_or_default();
                        window.friends = order.split(',').any(|part| part == "friends");
                        window.frame.open = window.friends || order.split(',').any(|part| part == "window");
                        window
                    },
                    member_windows: party::MemberWindows::default(),
                    party_invite: None,
                    partner: None,
                    adopt_asked: None,
                    friends: Vec::new(),
                    friend_asked: None,
                    expansion_asked: None,
                    switch_asked: None,
                    last_wear: None,
                    bind_asked: None,
                    broken_before: None,
                    pet_id: None,
                    pet: None,
                    pet_window: pet::PetWindow::default(),
                    pet_asked: None,
                    pet_hunger_before: 0,
                    pet_idle: pet::Idle::default(),
                    mercenary: None,
                    refusing_party: None,
                    announcements: announce::Bar::default(),
                    map_property: None,
                    dress_lists: lua::dress_lists(&self.grf)
                        .map(|[hair, hair_color, clothes_color]| dressroom::Lists { hair, hair_color, clothes_color })
                        .unwrap_or_default(),
                    dressroom: None,
                    ranking: None,
                    reputation: HashMap::new(),
                    reputation_window: None,
                    navigation: None,
                    navi_window: None,
                    catalog: None,
                    skill_description: None,
                    pickup_popup: None,
                    new_items: false,
                    level_ups: level_up::LevelUps::default(),
                    overweight: basic_info::OVERWEIGHT,
                    quest_icon: false,
                    banners: quest_notice::Banners::default(),
                    char_names: HashMap::new(),
                    names_asked: HashSet::new(),
                    dress_figure: None,
                    preview_figure: None,
                    head_emblems: Vec::new(),
                    bars_drawn: Vec::new(),
                    no_shift: settings_path("noshift").and_then(|path| std::fs::read_to_string(path).ok()).is_some_and(|text| text.trim() == "1"),
                    login_notices: settings_path("loginout").and_then(|path| std::fs::read_to_string(path).ok()).is_none_or(|text| text.trim() != "0"),
                    no_guild_talk: settings_path("notalkmsg2").and_then(|path| std::fs::read_to_string(path).ok()).is_some_and(|text| text.trim() == "1"),
                    show_emblems: settings_path("emblem").and_then(|path| std::fs::read_to_string(path).ok()).is_none_or(|text| text.trim() != "0"),
                    unknown_names: HashSet::new(),
                    hover_block: None,
                    skill_sent: None,
                    right_moved: false,
                    companion_mark: None,
                    camera_snap: settings_path("camerasnap").and_then(|path| std::fs::read_to_string(path).ok()).is_some_and(|text| text.trim() == "1"),
                    no_ctrl: settings_path("noctrl").and_then(|path| std::fs::read_to_string(path).ok()).is_none_or(|text| text.trim() != "0"),
                    skill_fail: settings_path("skillfail").and_then(|path| std::fs::read_to_string(path).ok()).is_none_or(|text| text.trim() != "0"),
                    snaps: Snaps::load(),
                    cursor_at: [-1.0; 2],
                    attack: None,
                    last_attack: None,
                    attack_was: None,
                    walk_cell: None,
                    last_chase_walk: None,
                    player_motion: (actor::IDLE, false),
                    riding: false,
                    chat_mode: battle_mode::Mode::load(),
                    map_type: None,
                    battle: battleground::Battle::default(),
                    queue_window: battle_queue::QueueWindow::default(),
                    memorial: memorial::Window::default(),
                    memorial_asked: false,
                    storage_log: None,
                    storage_log_heard: Vec::new(),
                    queued: None,
                    queue_asked: None,
                    camp_looks: None,
                    pvp_rank: None,
                    rank_font: None,
                    time_font: None,
                    time_attack: None,
                    last_rank: 0,
                    first_since: None,
                    top_ten: HashSet::new(),
                    blocked: HashSet::new(),
                    party_dialog: None,
                    leader_asked: None,
                    question_box: None,
                    row_press: None,
                    small_saved: String::new(),
                    talks: whisper::Talks::default(),
                    whisper_open: whisper::OpenSettings::load(),
                    friend_setup: None,
                    block_list: None,
                    away: whisper::Away::default(),
                    away_prompt: None,
                    mercenary_skills: Vec::new(),
                    mercenary_window: mercenary::Window::default(),
                    mercenary_skill_list: homunculus::SkillList::mercenary(),
                    steering: mercenary::Steering::default(),
                    dismiss_asked: false,
                    homunculus: None,
                    homunculus_id: None,
                    elemental: None,
                    homunculus_skills: Vec::new(),
                    homunculus_window: homunculus::Window::default(),
                    homunculus_skill_list: homunculus::SkillList::homunculus(),
                    homunculus_steering: mercenary::Steering::default(),
                    homunculus_asked: None,
                    homunculus_warned: None,
                    vend_setup: None,
                    vend_size: vend::GridSize::default(),
                    my_shop: None,
                    buying_setup: None,
                    my_buying_store: None,
                    mail: mail::Mail::default(),
                    cart: {
                        let mut cart = inventory::Inventory::cart();
                        cart.title = CART_TITLE;
                        cart.frame.position = CART_POSITION;
                        cart.frame.open = std::env::var_os("RUSTNAROK_CLIENT_CART").is_some();
                        cart
                    },
                    guild: None,
                    guild_window: guild::GuildWindow::default(),
                    guild_invite: None,
                    guild_asked: None,
                    reason_box: None,
                    guild_companion: guild_companion::Companion::default(),
                    guild_shown: false,
                    clan: None,
                    clan_window: clan::ClanWindow::default(),
                    emblems: HashMap::new(),
                    name_tags: HashMap::new(),
                    pointed: None,
                    signs: HashMap::new(),
                    sign_rects: Vec::new(),
                    chat_links: Vec::new(),
                    dialog_links: Vec::new(),
                    room: None,
                    room_make: None,
                    emotion_list: None,
                    macro_window: None,
                    macro_lines: macros::load(macros::path().as_deref()),
                    room_tried: false,
                    esc_order: Vec::new(),
                    save_asked: false,
                    apply_asked: false,
                    windows: vec![
                        Panel::Hotkeys,
                        Panel::BasicInfo,
                        Panel::Inventory,
                        Panel::Equipment,
                        Panel::Skills,
                        Panel::Storage,
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
                        Panel::Cutin,
                    ]
                    .into_iter()
                    .chain((0..storage::TABS).map(Panel::StorageCategory))
                    .chain([Panel::StorageSearch])
                    .collect(),
                    damages: Vec::new(),
                    status,
                    unit_hp: HashMap::new(),
                    auto_talk: std::env::var("RUSTNAROK_CLIENT_TALK").ok().map(|talk| talk.split(':').next().unwrap_or_default().to_owned()),
                    auto_next: std::env::var("RUSTNAROK_CLIENT_TALK").ok().and_then(|talk| talk.split(':').nth(1)?.parse().ok()).unwrap_or(0),
                    auto_menu: std::env::var("RUSTNAROK_CLIENT_MENU")
                        .unwrap_or_default()
                        .split(',')
                        .rev()
                        .filter_map(|choice| choice.trim().parse().ok())
                        .collect(),
                    auto_store: std::env::var("RUSTNAROK_CLIENT_STORE").ok(),
                    auto_attack: std::env::var_os("RUSTNAROK_CLIENT_ATTACK").is_some(),
                    auto_pick_up: std::env::var_os("RUSTNAROK_CLIENT_PICKUP").is_some(),
                    auto_wear: std::env::var_os("RUSTNAROK_CLIENT_WEAR").is_some(),
                    auto_keys: std::env::var("RUSTNAROK_CLIENT_HOTKEYS")
                        .unwrap_or_default()
                        .split(',')
                        .filter_map(|slot| slot.trim().parse().ok())
                        .enumerate()
                        .map(|(order, slot)| (Duration::from_secs(3 + order as u64), slot))
                        .collect(),
                    auto_skill: std::env::var("RUSTNAROK_CLIENT_SKILL").ok().and_then(|skill| {
                        let (id, level) = skill.split_once(':')?;
                        Some((id.parse().ok()?, level.parse().ok()?))
                    }),
                    attack_range: 1,
                    pending: None,
                    pending_walk: None,
                    pickups_asked: HashSet::new(),
                    auto_say: std::env::var("RUSTNAROK_CLIENT_SAY").ok(),
                    last_move: None,
                    walk_goal: None,
                    walks: 0,
                    walk_held: false,
                    last_walk_try: None,
                    stored_walk: StoredWalk::default(),
                    skill_info: self.skill_info.clone(),
                    location_radii: self.location_radii.clone(),
                    press_mark: None,
                    auto_walk: std::env::var("RUSTNAROK_CLIENT_WALK").ok().and_then(|cell| {
                        let (x, y) = cell.split_once(',')?;
                        Some([x.parse().ok()?, y.parse().ok()?])
                    }),
                }));
                if let Scene::Game(game) = &mut self.scene {
                    game.restore_places();
                    game.weather.begin_map();
                }
                for event in std::mem::take(&mut self.early_events) {
                    self.map_event(event);
                }
            }
            Err(error) => self.message(format!("맵을 불러올 수 없습니다: {map} ({error})"), Scene::Login),
        }
    }

    /// Shows a loading screen; the map loads once it's on screen.
    pub(crate) fn show_loading(&mut self, target: LoadTarget) {
        let Some(gpu) = &self.gpu else { return };
        let image = (!self.loading_images.is_empty()).then(|| {
            let pick =
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().subsec_nanos() as usize % self.loading_images.len();
            gpu.try_load_texture(&self.grf, &self.loading_images[pick], true).ok()
        });
        self.loading = Some(Loading { image: image.flatten(), shown: false, target });
        gpu.window.request_redraw();
    }

    /// Loads another map on the same map server.
    pub(crate) fn change_map(&mut self, map: String, position: [u16; 2]) {
        let (Scene::Game(game), Some(gpu)) = (&mut self.scene, &self.gpu) else { return };
        match World::load(gpu, &self.grf, &map) {
            Ok(world) => {
                let indoor = self.indoor.contains(&map_key(&map));
                game.camera_goal = CameraGoal::arrive(indoor, self.viewpoints.get(&map_key(&map)).copied(), self.saved_view.of(indoor), zoom_out_saved());
                game.camera = game.camera_goal.camera(world.cell_position(position.map(f32::from)));
                game.minimap = minimap::load_map(&self.grf, &map);
                game.minimap_crop = None;
                let image = game.minimap.clone();
                // ponytail: a warp onto a map with no big minimap closes an open one, and says so in the OK box.
                let shown = !self.minimap_exceptions.contains(&map_key(&map));
                if game.minimap_window.change_map(gpu, image.as_ref(), &map_key(&map), shown) {
                    game.question_box = Some(BoxQuestion::Notice(minimap_window::NO_PICTURE));
                    game.sync_esc();
                }
                game.guild_places.clear();
                game.player.set_map([world.gat.width, world.gat.height]);
                game.world = world;
                let (ambient, diffuse) = game.world.light_rgb();
                game.scene.reload(ambient, diffuse);
                game.map = map;
                // The sky starts again on every load, a same-map teleport too.
                game.sky = sky::for_map(&self.grf, &game.map, &game.world, position);
                game.units.clear();
                game.skill_units.clear();
                game.spirits.clear();
                game.ground.clear();
                game.marks.clear();
                // rathena sends the quest marks again on arrival.
                game.quest_marks.clear();
                game.quest_started.clear();
                game.store_mark = None;
                game.signs.clear();
                // The server ends the search on leaving the map.
                game.catalog = None;
                if let Some(shop) = &game.my_shop {
                    game.signs.insert(game.account_id, chatroom::Sign::shop(game.account_id, shop.name.clone()));
                }
                if let Some(store) = &game.my_buying_store {
                    game.signs.insert(game.account_id, chatroom::Sign::buying_store(game.account_id, store.name.clone()));
                }
                game.effects.clear();
                game.capturing = false;
                game.stored_walk.goal = None;
                // The server announces the pet again on load.
                (game.pet_id, game.pet) = (None, None);
                game.put_after_warp(position);
                let _ = game.session.loaded();
            }
            Err(error) => game.chat_line(format!("맵을 불러올 수 없습니다: {map} ({error})"), CHAT_ERROR),
        }
    }
}

impl App {
    pub(crate) fn map_event(&mut self, event: MapEvent) {
        if let MapEvent::Entered { position, direction, clock: (tick, at) } = event {
            self.server_clock.set(tick, at);
            self.map_loaded = true;
            self.show_loading(LoadTarget::Enter { position, direction });
            return;
        }
        // Read on this frame's clock.
        if let MapEvent::ServerTime(tick, _) = event {
            let at = Instant::now();
            if std::mem::take(&mut self.map_loaded) {
                self.server_clock.set(tick, at);
            } else {
                self.server_clock.heard(tick, at);
            }
            return;
        }

        let Scene::Game(game) = &mut self.scene else {
            if self.entering.is_some() {
                self.early_events.push(event);
            }
            return;
        };
        let (grf, job_names, look_tables, pets) = (&self.grf, &self.job_names, &self.look_tables, &self.pets);
        if frees_quick_move(&event) {
            game.quick_moving = false;
        }
        // Which kind of pick a making list is (read before the event is taken apart).
        let event_kind = match &event {
            MapEvent::MakeList(_) => Some(Picking::Make),
            MapEvent::CookList { kind, .. } => Some(Picking::Cook(*kind)),
            MapEvent::ArrowList(_) => Some(Picking::Arrow),
            _ => None,
        };
        let (gpu, table) = (self.gpu.as_ref(), &self.effect_table);
        // The effects a skill plays at a moment on a unit (`role` as in the client's skill effect table).
        let skill_effects = |game: &mut Game, skill: u16, role: &str, unit: u32| {
            if let Some(gpu) = gpu {
                for effect in table.skill(skill, role) {
                    game.play_effect(gpu, grf, table, effect, unit);
                }
            }
        };
        let map_size = [game.world.gat.width, game.world.gat.height];
        let new_actor = |unit: &net::Unit| {
            let sprites = match unit.kind {
                _ if drawn_as_player(unit.kind, unit.job) => SpriteSet::player(grf, &unit_look(unit), look_tables),
                PET => job_names.get(&unit.job).and_then(|name| pet_sprites(grf, name, pets.tables.accessory_acts.get(&unit.head_bottom))),
                MERCENARY if (6017..=6046).contains(&unit.job) => job_names.get(&unit.job).and_then(|name| SpriteSet::mercenary(grf, unit.job, name)),
                // Hidden NPCs (script triggers) have no sprite in the client either; a warp portal is only its rings.
                _ if HIDDEN_JOBS.contains(&unit.job) || unit.job == WARP_JOB => None,
                _ => job_names.get(&unit.job).and_then(|name| SpriteSet::single(grf, name, unit.kind)),
            };
            let mut actor = Actor::new(unit.name.clone(), sprites, unit.position.map(i32::from), unit.direction, unit.speed);
            actor.set_map(map_size);
            actor.kind = unit.kind;
            actor.job = unit.job;
            actor.level = unit.level;
            actor.look = drawn_as_player(unit.kind, unit.job).then(|| unit_look(unit));
            actor.hidden = HIDDEN_JOBS.contains(&unit.job) || unit.job == WARP_JOB;
            actor.cloaked = unit.option & actor::UNSEEN_OPTIONS != 0;
            actor.hide_option = unit.option;
            actor.set_head(unit.head_direction);
            actor.guild = unit.guild;
            actor
        };
        match npc_holds(&event) {
            Some((npc, true)) => game.dialog_for(npc).talking = true,
            Some((npc, false)) => {
                if let Some(dialog) = game.dialog.as_mut().filter(|dialog| dialog.npc == npc) {
                    dialog.talking = false;
                }
            }
            None => {}
        }
        match event {
            MapEvent::Unit(unit) => {
                // A warp portal shows its rings while it's there (EF_WARPZONE2).
                if unit.job == WARP_JOB
                    && !game.units.contains_key(&unit.id)
                    && let Some(gpu) = gpu
                {
                    let at = game.world.cell_position(unit.position.map(f32::from));
                    game.effects.play(gpu, grf, table, EF_WARPZONE2, effects::Target::Unit(Some(unit.id)), at, Instant::now());
                }
                // Back in sight while fading out, or a monster that respawned under its corpse's ID: a unit of its own
                // again, the old copy fading where it was.
                game.leave_behind(unit.id);
                let known = game.units.contains_key(&unit.id);
                // An NPC with a quest mark coming into view starts its mark's loop again.
                if !known && game.quest_marks.iter().any(|&(npc, ..)| npc == unit.id) {
                    game.quest_started.insert(unit.id, Instant::now());
                }
                let cue = arrival(unit.spawned, known, unit.id == game.account_id, unit.kind, unit.option);
                let actor = game.units.entry(unit.id).or_insert_with(|| new_actor(&unit));
                actor.body_state = unit.body_state;
                actor.health_state = unit.health_state;
                actor.level = unit.level;
                actor.stop_at(unit.position.map(i32::from));
                actor.set_direction(if !known && unit.kind == MONSTER && turned_round(unit.job) { (unit.direction + 4) % 8 } else { unit.direction });
                actor.set_head(unit.head_direction);
                take_posture(actor, unit.posture, control::now());
                if cue == Arrival::Fade {
                    actor.fade_in = Some(control::now());
                }
                // A player seen again may have changed clothes (or guilds) meanwhile.
                if let Some(look) = refreshed_look(actor, &unit, false) {
                    actor.sprites = SpriteSet::player(grf, &look, look_tables);
                    actor.look = Some(look);
                }
                actor.guild = unit.guild;
                actor.cloaked = unit.option & actor::UNSEEN_OPTIONS != 0;
                // Our own entry is the player actor. A copy in the unit list doesn't grow a second falcon or warg.
                if unit.id == game.account_id {
                    game.player.cloaked = actor.cloaked;
                    game.player.sync_companions(unit.option, grf);
                } else {
                    actor.sync_companions(unit.option, grf);
                }
                game.want_emblem(unit.guild, unit.emblem);
                game.monster_hp(&unit);
                if cue == Arrival::Rings
                    && let Some(gpu) = gpu
                {
                    game.play_effect(gpu, grf, table, EF_ENTRY2, unit.id);
                }
            }
            MapEvent::Turn { id, direction, head_direction } => {
                let actor = if id == game.account_id { Some(&mut game.player) } else { game.units.get_mut(&id) };
                if let Some(actor) = actor {
                    actor.turn(direction, head_direction, control::now());
                }
            }
            MapEvent::UnitWalk { unit, to, start_time, sub } => {
                game.leave_behind(unit.id);
                let known = game.units.contains_key(&unit.id);
                let actor = game.units.entry(unit.id).or_insert_with(|| new_actor(&unit));
                // Every move entry brings the unit's job and looks again.
                if let Some(look) = refreshed_look(actor, &unit, true) {
                    actor.job = unit.job;
                    actor.sprites = SpriteSet::player(grf, &look, look_tables);
                    actor.look = Some(look);
                } else if !drawn_as_player(unit.kind, unit.job) && take_job(actor, unit.job) {
                    actor.look = None;
                    actor.sprites = job_sprites(grf, job_names, unit.kind, unit.job);
                }
                actor.hide_option = unit.option;
                actor.body_state = unit.body_state;
                actor.health_state = unit.health_state;
                actor.level = unit.level;
                let from = unit.position.map(i32::from);
                actor.guild = unit.guild;
                // Every move entry carries the unit's speed.
                if unit.speed > 0 {
                    actor.step_ms = f32::from(unit.speed);
                }
                // On the server's clock, a little behind it; from where it really is in its cell. No path: a new unit
                // stands where it starts, a known one goes on as it was.
                let now = control::now();
                self.server_clock.heard(start_time, Instant::now());
                let begin = self.server_clock.local(start_time).map_or(now, |at| at + actor::REMOTE_DELAY);
                match world::find_unit_path(&game.world.gat, from, to.map(i32::from), unit.kind) {
                    Some(path) => actor.walk_from(from, sub, path, begin, now, known),
                    None if !known => actor.walk_from(from, sub, Vec::new(), begin, now, false),
                    None => {}
                }
                actor.cloaked = unit.option & actor::UNSEEN_OPTIONS != 0;
                // A walk into view is often the first packet. The falcon and warg come with it.
                if unit.id == game.account_id {
                    game.player.cloaked = actor.cloaked;
                    game.player.sync_companions(unit.option, grf);
                } else {
                    actor.sync_companions(unit.option, grf);
                }
                game.want_emblem(unit.guild, unit.emblem);
                game.monster_hp(&unit);
            }
            // The bank: open on the server's word, then ask the balance; refusals by rathena's reasons.
            // ponytail: which message goes with which reason is read off their words.
            MapEvent::BankOpened => {
                game.bank = Some(bank::Bank::new());
                let _ = game.session.check_bank(game.account_id);
            }
            MapEvent::BankBalance(balance) => {
                if let Some(bank) = &mut game.bank {
                    bank.balance = balance;
                }
            }
            MapEvent::BankMoved { deposit, reason, balance, zeny } => {
                if let Some(bank) = &mut game.bank {
                    bank.balance = balance;
                }
                game.status.insert(net::STATUS_ZENY, i64::from(zeny));
                let refusal = match (deposit, reason) {
                    (_, 0) => None,
                    (true, 2) => Some(BANK_NO_ZENY),
                    (true, 3) => Some(BANK_OVERFLOW),
                    (false, 1) => Some(BANK_SHORT),
                    _ => Some(BANK_DOWN),
                };
                if let Some(id) = refusal {
                    game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ERROR);
                }
            }
            MapEvent::CashShopOpen { cash, free, tab } => {
                let mut shop = cashshop::CashShop::new(cash, free);
                shop.tabs.clone_from(&game.cashshop_lists);
                if usize::try_from(tab).ok().is_some_and(|tab| tab < shop.tabs.len() && !shop.tabs[tab].is_empty()) {
                    shop.selected_tab = tab as usize;
                }
                game.cashshop = Some(shop);
                if !game.cashshop_list_requested {
                    game.cashshop_list_requested = true;
                    let _ = game.session.request_cashshop_list();
                }
            }
            MapEvent::CashShopList { tab, items } => {
                if let Some(list) = usize::try_from(tab).ok().and_then(|tab| game.cashshop_lists.get_mut(tab)) {
                    list.extend(items.iter().copied());
                }
                if let Some(shop) = &mut game.cashshop {
                    shop.append(tab, items);
                    if shop.tabs[shop.selected_tab].is_empty()
                        && let Some(first) = shop.tabs.iter().position(|items| !items.is_empty())
                    {
                        shop.selected_tab = first;
                    }
                }
            }
            MapEvent::CashShopBuyResult { result, cash, free, .. } => {
                if let Some(shop) = &mut game.cashshop {
                    shop.clear_cart_on_success(result, cash, free);
                }
                let id = cashshop::buy_result_message(result);
                game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), if result == 0 { CHAT_ITEM } else { CHAT_ERROR });
            }
            // Another player's equipment: their window with their figure, its weapon and shield from what they wear.
            MapEvent::OtherEquipment(equipped) => {
                let look = Look {
                    job: equipped.job,
                    sex: equipped.sex,
                    hair: equipped.hair,
                    hair_color: equipped.hair_color,
                    clothes_color: equipped.clothes_color,
                    robe: equipped.robe,
                    ..Look::default()
                };
                let mut window = equipment::Equipment::of(equipped.name, screen());
                window.set_tab_labels(&self.messages, &self.font);
                let views = equipped.items.iter().filter(|(item, _)| item.worn != 0).map(|&(ref item, view)| (item.worn, view)).collect();
                game.viewing = Some(Viewed { window, items: equipped.items.into_iter().map(|(item, _)| item).collect(), look, views, figure: None });
            }
            MapEvent::CostumesShown(shown) => game.equipment.costumes_shown = shown,
            MapEvent::ShowEquipment { shown, answer } => {
                game.equipment.shown = shown;
                if answer {
                    let id = if shown { EQUIPMENT_SHOWN } else { EQUIPMENT_HIDDEN };
                    game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ITEM);
                }
            }
            MapEvent::ExpansionAsked { result: 0, voucher } => game.expansion_asked = Some(voucher),
            // The results, done (0) or why not; the question's own result 0 is handled above.
            MapEvent::ExpansionAsked { result, .. } | MapEvent::ExpansionDone(result) => {
                let id = EXPANSION_RESULTS.get(usize::from(result)).copied().unwrap_or(EXPANSION_RESULTS[1]);
                game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), if result == 0 { CHAT_ITEM } else { CHAT_ERROR });
            }
            // Change Material or Four Spirit Analysis: the material window, with the inventory beside it.
            MapEvent::MaterialWindow(kind) => {
                game.materials = Some(material::Materials::new(kind));
                game.inventory.frame.open = true;
                game.raise(Panel::Inventory);
            }
            // The refine window opens with the inventory beside it.
            MapEvent::RefineOpen => {
                let window = refine::Refine::new(crate::screen(), control::now());
                // Opening shuts an open mail window and bank window, each with a red line.
                if game.mail.list.frame.open || game.mail.compose.is_some() || game.mail.letter.is_some() {
                    game.mail.list.frame.open = false;
                    game.mail.compose = None;
                    game.mail.letter = None;
                    game.chat_line(self.messages.get(REFINE_CLOSED_MAIL).cloned().unwrap_or_default(), CHAT_ERROR);
                }
                if game.bank.take().is_some() {
                    let _ = game.session.close_bank(game.account_id);
                    game.chat_line(self.messages.get(REFINE_CLOSED_BANK).cloned().unwrap_or_default(), CHAT_ERROR);
                }
                // The inventory goes right beside it, its bottom on the window's.
                game.inventory.frame.open = true;
                game.inventory.frame.position = window.inventory_place(game.inventory.height());
                game.refine = Some(window);
                game.raise(Panel::Inventory);
            }
            // Kind 8 is grade enchanting and opens the inventory beside it. Kind 1 is the stylist.
            MapEvent::UiOpen { kind, .. } => {
                if kind == grade::UI_KIND {
                    game.grade = Some(grade::GradeEnchant::new());
                    game.inventory.frame.open = true;
                    game.raise(Panel::Inventory);
                } else if kind == stylist::UI_KIND {
                    game.stylist = Some(stylist::Window::open());
                    let look = game.player.look.clone().unwrap_or_default();
                    if let Some(window) = &mut game.stylist {
                        dress_stylist(window, grf, look_tables, &look, &self.items, &self.styling_shop);
                    }
                }
            }
            MapEvent::GradeMaterials { index, chance, blessing, materials } => {
                if index < 0 {
                    game.chat_line(self.messages.get(grade::CANT_UPGRADE).cloned().unwrap_or_default(), CHAT_ERROR);
                    if let Some(window) = &mut game.grade {
                        window.clear();
                    }
                } else if let Some(window) = &mut game.grade {
                    window.listed(index as u16, chance, blessing, materials);
                }
            }
            // The item's new grade. Result 3 already deleted it. 0 and 1 ask for the list again.
            MapEvent::GradeResult { index, grade: next, result } => {
                if result != 3
                    && let Some(item) = game.inventory.items.iter_mut().find(|item| item.index == index)
                {
                    grade::apply_result(item, next, result);
                }
                if let Some(window) = &mut game.grade {
                    let again = window.finished(result);
                    if let Some(&id) = grade::RESULT_LINES.get(result as usize) {
                        let color = if result == 0 || result == 4 { CHAT_ITEM } else { CHAT_ERROR };
                        game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), color);
                    }
                    if again {
                        let _ = game.session.grade_select(index);
                    }
                }
            }
            MapEvent::SynthesisOpen(id) => match self.synthesis_boxes.get(&id) {
                Some(info) => {
                    game.synthesis = Some(laphine::Synthesis::new(id, info));
                    game.inventory.frame.open = true;
                    game.raise(Panel::Inventory);
                }
                None => {
                    let _ = game.session.close_synthesis();
                }
            },
            MapEvent::UpgradeOpen(id) => match self.upgrade_boxes.get(&id) {
                Some(info) => {
                    game.upgrade = Some(laphine::Upgrade::new(id, info));
                    game.inventory.frame.open = true;
                    game.raise(Panel::Inventory);
                }
                None => {
                    let _ = game.session.close_upgrade();
                }
            },
            // A reform item the client has no entries for: tell the server and open nothing.
            MapEvent::ReformOpen(item) => match self.reforms.get(&item).filter(|entries| !entries.is_empty()) {
                Some(entries) => {
                    game.reform = Some(reform::Reform::open(item, entries.clone()));
                    game.inventory.frame.open = true;
                    game.raise(Panel::Inventory);
                }
                None => {
                    game.reform = None;
                    let _ = game.session.close_reform();
                }
            },
            MapEvent::Synthesized(result) => {
                if result == 0
                    && let Some(window) = &mut game.synthesis
                {
                    window.clear();
                }
                let id = match result {
                    5 => Some(laphine::NOT_ENOUGH),
                    7 => Some(laphine::NOTHING),
                    _ => None,
                };
                if let Some(id) = id {
                    game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ERROR);
                }
            }
            MapEvent::LaphineUpgraded(result) => {
                // ponytail: 2971 and 2972 are guesses for the upgrade's result lines.
                let (id, color) = if result == 0 {
                    if let Some(window) = &mut game.upgrade {
                        window.clear();
                    }
                    (laphine::UPGRADE_OK, CHAT_ITEM)
                } else {
                    (laphine::UPGRADE_FAIL, CHAT_ERROR)
                };
                game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), color);
            }
            MapEvent::ItemPreview { index, cards, refine, broken, grade } => {
                let shown = game.inventory.items.iter_mut().find(|item| item.index == index).map(|item| {
                    item.cards = cards;
                    item.refine = refine;
                    item.grade = grade;
                    item.broken = broken;
                    iteminfo::Shown::of(item)
                });
                // ponytail: that the preview is the item info window is a guess.
                if let Some(shown) = shown {
                    game.show_item(shown);
                }
            }
            // Result 0: rathena has forgotten the window, so close it. The inventory updates arrived on their own.
            MapEvent::Reformed { result: 0, .. } => game.reform = None,
            MapEvent::Reformed { .. } => {}
            // A table this client doesn't have: say so, and tell rathena the window is closed.
            MapEvent::EnchantOpen(table) => match self.enchant_tables.get(&table).cloned() {
                Some(info) => {
                    game.enchant = Some(enchant::Enchant::open(table, info));
                    game.inventory.frame.open = true;
                    game.raise(Panel::Inventory);
                }
                None => {
                    game.chat_line(self.messages.get(enchant::NOT_FOUND).cloned().unwrap_or_default(), CHAT_ERROR);
                    let _ = game.session.close_enchant();
                }
            },
            MapEvent::Enchanted { message, item } => {
                let success = message == enchant::SUCCESS as u32;
                if let Some(window) = &game.enchant
                    && let Some(index) = window.item
                    && let Some(carried) = game.inventory.items.iter_mut().find(|carried| carried.index == index)
                {
                    carried.cards = window.cards_after(message, item, carried.cards);
                }
                let text = self.messages.get(message as usize).cloned().unwrap_or_default();
                game.chat_line(text, if success { CHAT_ITEM } else { CHAT_ERROR });
                game.enchant = None;
            }
            MapEvent::RefineMaterials { index, blessings, materials } => {
                // Nothing to refine it with: a timed notice in the window, which stays as it was.
                if let Some(window) = &mut game.refine {
                    if materials.is_empty() {
                        window.say(refine::CANT_REFINE, true, Some(control::now()));
                    } else {
                        window.listed(index, blessings, materials, control::now());
                    }
                }
            }
            // Any refine (the window, Upgrade Weapon, @refine): the item's new level; a broken one leaves the window.
            MapEvent::Refined { result, index, refine } => {
                if result != 1
                    && let Some(item) = game.inventory.items.iter_mut().find(|item| item.index == index)
                {
                    item.refine = refine as u8;
                }
                // The window hammers for 1.4 s, shows the result and says it in the chat after it.
                if let Some(window) = &mut game.refine {
                    window.refined(result, control::now());
                }
                let (effect, line) = non_window_refine_result(game.refine.is_some(), result);
                if let Some(effect) = effect
                    && let Some(gpu) = gpu
                {
                    game.play_effect(gpu, grf, table, effect, game.account_id);
                }
                if let Some(line) = line {
                    game.chat_line(self.messages.get(line).cloned().unwrap_or_default(), CHAT_ITEM);
                }
            }
            MapEvent::Spirits { id, count } => {
                change_orbs(&mut game.spirits, (id != game.account_id).then_some(id), |orbs| orbs.spheres = count);
            }
            // A Royal Guard's rage spheres come as shields too.
            MapEvent::Shields { id, count } => {
                let job = if id == game.account_id { Some(game.me.job) } else { game.units.get(&id).map(|actor| actor.job) };
                let guard = job.is_some_and(guard_job);
                change_orbs(&mut game.spirits, (id != game.account_id).then_some(id), |orbs| {
                    *if guard { &mut orbs.spheres } else { &mut orbs.shields } = count
                });
            }
            MapEvent::Charms { id, kind, count } => {
                change_orbs(&mut game.spirits, (id != game.account_id).then_some(id), |orbs| {
                    orbs.charms = if (1..=4).contains(&kind) { (kind, count) } else { (0, 0) }
                });
            }
            MapEvent::Souls { id, count } => change_orbs(&mut game.spirits, (id != game.account_id).then_some(id), |orbs| orbs.souls = count),
            MapEvent::Vanish { id, reason } => {
                if id != game.account_id {
                    game.spirits.remove(&Some(id));
                }
                lock_pair(&mut game.locks, id, id, false);
                game.hold_locked();
                // A body keeps what plays on it (the killing hit's effects) until it goes.
                if reason != 1 {
                    game.effects.forget(id);
                }
                // Reason 1: died. The body falls when the killing hit lands, then stays a moment.
                let key = (id != game.account_id).then_some(id);
                let lands = game.damages.iter().filter(|damage| damage.target == key).map(|damage| damage.start).fold(Instant::now(), Instant::max);
                let dying = if id == game.account_id { Some(&mut game.player) } else { game.units.get_mut(&id) };
                // Our own death brings up the option menu with the save point.
                if id == game.account_id && reason == 1 {
                    game.escape = Some(escape::Menu { dead: true });
                }
                // Teleporting and logging out leave rings and a sound on the spot (not for a GM hidden from sight).
                if id != game.account_id
                    && let (Some(gpu), Some(actor)) = (gpu, dying.as_deref())
                    && let Some(effect) = vanish_effect(reason, actor.hide_option)
                {
                    let at = game.world.cell_position(actor.position(Instant::now()));
                    game.effects.play(gpu, grf, table, effect, effects::Target::Ground, at, Instant::now());
                }
                let dying = if id == game.account_id { Some(&mut game.player) } else { game.units.get_mut(&id) };
                match (reason, dying) {
                    (1, Some(actor)) => fall(actor, lands),
                    // Out of sight: it fades out, its shadow gone at once.
                    (0, Some(actor)) if id != game.account_id => {
                        actor.fade_out(Instant::now(), OUT_OF_SIGHT_FADE_MS);
                        game.unit_hp.remove(&id);
                    }
                    _ => {
                        game.units.remove(&id);
                        game.unit_hp.remove(&id);
                        // Whoever comes back with this account may be another character.
                        game.name_tags.remove(&id);
                    }
                }
            }
            // Our own walk as the server has it, from its start cell and sub-cell at its tick, on the same shown clock as
            // other units; from where we're drawn when there's no way from there.
            MapEvent::PlayerMove { from, to, start_time, sub } => {
                let now = control::now();
                self.server_clock.heard(start_time, Instant::now());
                let begin = self.server_clock.local(start_time).map_or(now, |at| at + actor::REMOTE_DELAY);
                let (from, to) = (from.map(i32::from), to.map(i32::from));
                let gat = &game.world.gat;
                // No path from the server's source (or it's the destination): from the cell and sub-cell under where
                // we're drawn; nothing when that's no way either.
                let walk = world::find_unit_path(gat, from, to, PLAYER).map(|path| (from, sub, path)).or_else(|| {
                    let (cell, sub) = game.player.cell_and_sub();
                    world::find_unit_path(gat, cell, to, PLAYER).map(|path| (cell, sub, path))
                });
                if let Some((from, sub, path)) = walk {
                    game.player.walk_from(from, sub, path, begin, now, true);
                }
            }
            // A knockback puts the unit on its new cell at once; what it was doing goes on until the stop that follows.
            MapEvent::Knocked { id, position } => {
                let actor = if id == game.account_id { Some(&mut game.player) } else { game.units.get_mut(&id) };
                if let Some(actor) = actor {
                    actor.place_at(position.map(i32::from));
                }
            }
            // ZC_FASTMOVE slides the unit there; the stop that follows at the same cell leaves the slide running.
            // ponytail: the slide kept for ZC_FASTMOVE; only the knockback is known to be at once.
            MapEvent::Stop { id, position } | MapEvent::Slide { id, position } => {
                let cell = position.map(i32::from);
                let slide = matches!(event, MapEvent::Slide { .. });
                let now = control::now();
                let actor = if id == game.account_id { Some(&mut game.player) } else { game.units.get_mut(&id) };
                if let Some(actor) = actor {
                    if slide {
                        actor.slide_to(cell, now);
                        // Snap and the jumps blur a player for 100 ticks.
                        if actor.look.is_some() {
                            actor.snap();
                        }
                    } else if actor.sliding_to() != Some(cell) {
                        // Where it's drawn, unless it's drawn on another cell with a path to the stop cell.
                        let from = actor::drawn_cell(actor.position(now));
                        let path = world::find_path(&game.world.gat, from, cell).is_some();
                        actor.stop_drawn(cell, path, now);
                    }
                }
                if id == game.account_id
                    && let Some((_, arrived)) = &mut game.pending
                {
                    *arrived = Some(Instant::now());
                }
            }
            // In a chat room, talk goes to its window.
            MapEvent::Chat { id, message, .. } if game.room.is_some() => {
                game.room.as_mut().unwrap().lines.push((message, if id.is_none() { chatroom::OWN_LINE } else { [0, 0, 0] }));
            }
            MapEvent::Chat { id, message, color } => {
                game.say(id, message.clone(), color.unwrap_or(CHAT_WHITE));
                game.chat_line(message, color.unwrap_or(if id.is_none() { CHAT_SELF } else { CHAT_WHITE }));
            }
            MapEvent::MapChange { map, position } => {
                self.map_loaded = true;
                game.pickups_asked.clear();
                // A map change or teleport leaves the NPC behind: its windows and picture go, and nothing is sent.
                game.dismiss_dialog();
                game.special_popup = None;
                game.cutin = None;
                // The new map's weather comes once it has loaded, and the mercenary's news, its type and the PvP count.
                game.weather.clear();
                game.pvp_rank = None;
                game.map_type = None;
                game.battle = battleground::Battle::default();
                game.last_rank = 0;
                game.first_since = None;
                game.top_ten.clear();
                game.forget_mercenary(false);
                game.forget_homunculus(false);
                // Back at the save point: up again.
                if game.escape.as_ref().is_some_and(|menu| menu.dead) {
                    game.escape = None;
                }
                game.player.revive();
                // The NPC is left behind: its shop and its buy-or-sell question close (the server doesn't say so).
                game.close_shop();
                game.close_cashshop();
                // rathena forgets the refine window on a map change; the material window, the roulette and the reform
                // window go too. The reform window sends no close: the server already forgot it.
                // rathena forgets the refine window and the grade window on a map change (no packet); the material window and the roulette go too.
                game.refine = None;
                game.grade = None;
                // rathena forgets both Laphine windows, the enchant window and the stylist on a map change, without a close packet.
                game.synthesis = None;
                game.upgrade = None;
                game.reform = None;
                game.enchant = None;
                game.stylist = None;
                game.roulette = None;
                // The mirror's status ends with the warp.
                game.boss = None;
                game.materials = None;
                game.close_deal_choice();
                game.attack = None;
                // The merge window closes without a word.
                if matches!(game.pick, Some((_, Picking::Merge))) {
                    game.pick = None;
                }
                game.combos.clear();
                game.locks.clear();
                game.hold_locked();
                if map == game.map {
                    // A teleport on the same map: no loading, but the server takes the player off the map until
                    // the load is acknowledged (it drops everything else till then) and sends what's in view again.
                    game.put_after_warp(position);
                    game.units.clear();
                    game.skill_units.clear();
                    game.ground.clear();
                    let _ = game.session.loaded();
                } else {
                    self.show_loading(LoadTarget::Change { map, position });
                }
            }
            MapEvent::ComboWindow { id, ms } => {
                game.combos.insert((id != game.account_id).then_some(id), (control::now(), Duration::from_millis(u64::from(ms))));
            }
            // Locked units turn to face each other.
            // ponytail: the facing is an estimate.
            MapEvent::BladeStop { source, target, on } => {
                lock_pair(&mut game.locks, source, target, on);
                let me = game.account_id;
                let cell = |game: &Game, id: u32| if id == me { Some(game.player.cell) } else { game.units.get(&id).map(|actor| actor.cell) };
                if on && let (Some(from), Some(to)) = (cell(game, source), cell(game, target)) {
                    for (id, toward) in [(source, to), (target, from)] {
                        if let Some(actor) = if id == me { Some(&mut game.player) } else { game.units.get_mut(&id) } {
                            actor.face(toward);
                        }
                    }
                }
                game.hold_locked();
            }
            // Each text starts a line, and so does each LF in it; the first after Next wipes the old page.
            MapEvent::NpcSay { npc, text } => {
                let name_of = |item: &links::ItemLink| inventory::item_name(&self.items, item.id, true, item.refine);
                let text = links::mark(&npc_say::unescape(&text), &name_of);
                let dialog = game.dialog_for(npc);
                if std::mem::take(&mut dialog.wipe) {
                    dialog.lines.clear();
                    game.say.restart();
                }
                let dialog = game.dialog.as_mut().unwrap();
                dialog.lines.extend(text.split('\n').map(|line| line.replace('\r', "")));
            }
            MapEvent::NpcNext { npc } => game.dialog_for(npc).button = DialogButton::Next,
            MapEvent::NpcWindowSize { width, height } => game.say.resize(width, height),
            MapEvent::NpcWindowPos { x, y, percent } => game.say.move_to(x, y, percent),
            MapEvent::SpecialPopup(id) => game.special_popup = Some(id),
            // A script may end in `close` without having said anything (rathena's item scripts do): no box for
            // that, but answer it so the script doesn't keep the player.
            MapEvent::NpcClose { npc } if game.dialog.as_ref().is_none_or(|dialog| dialog.npc != npc) => {
                let _ = game.session.close_dialog(npc);
            }
            // The script is done: Close shows, and the player is free while it does.
            MapEvent::NpcClose { npc } => game.dialog_for(npc).button = DialogButton::Close,
            // The text goes and the view is back at the top; the window, its button, the menu and the cut-in stay.
            MapEvent::NpcClear { npc } => {
                if let Some(dialog) = game.dialog.as_mut().filter(|dialog| dialog.npc == npc) {
                    dialog.lines.clear();
                    dialog.wipe = false;
                    game.say.restart();
                }
            }
            MapEvent::NpcMenu { npc, items } => {
                // A new list replaces an open one, and the say window's Next goes.
                let items = items.into_iter().filter(|item| !item.is_empty()).collect();
                let dialog = game.dialog_for(npc);
                dialog.menu = Some(npc_menu::Menu::new(items));
                dialog.button = DialogButton::None;
            }
            // The box opens only for an NPC in view (a text one also for id 0).
            MapEvent::NpcInput { npc, text } => {
                if game.units.contains_key(&npc) || (text && npc == 0) {
                    game.dialog_for(npc).input = Some(npc_input::Input::new(text));
                }
            }
            // Where Warp Portal or Teleport goes, in the NPC menu's box: "Random Area" for Teleport's random spot, the
            // maps by their file names, then Cancel.
            MapEvent::WarpList { skill, maps } => {
                let message = |id: usize| self.messages.get(id).cloned().unwrap_or_default();
                let mut items: Vec<String> =
                    maps.iter().map(|map| if map.trim_end_matches(".gat") == "Random" { message(RANDOM_AREA) } else { map.clone() }).collect();
                items.push(message(WARP_CANCEL));
                game.dialog = Some(Dialog {
                    npc: 0,
                    talking: false,
                    lines: Vec::new(),
                    wipe: false,
                    button: DialogButton::None,
                    menu: Some(npc_menu::Menu::new(items)),
                    input: None,
                    warp: Some((skill, maps)),
                });
            }
            MapEvent::Act(act) => {
                // A landed hit sparks on whatever it hit, each hit as it lands.
                let attacker_job = if act.source == game.account_id { Some(game.player.job) } else { game.units.get(&act.source).map(|actor| actor.job) };
                let sparks = attack_sparks(act.damage, act.kind, attacker_job.is_some_and(|job| TAEKWON_LINE.contains(&job)));
                if let Some(gpu) = gpu {
                    let lands = Instant::now() + Duration::from_millis(act.source_speed.max(0) as u64);
                    // ponytail: the hits of a double attack 200 ms apart, as a skill's.
                    for hit in 0..u32::from(act.hits.max(1)) {
                        for &spark in &sparks {
                            game.play_effect_from(gpu, grf, table, spark, act.target, lands + Duration::from_millis(200) * hit);
                        }
                    }
                }
                if matches!(act.kind, 0 | 4 | 8 | 9 | 10 | 11 | 13) {
                    game.battle_lines(&act, &self.messages);
                }
                game.act(act, look_tables, grf);
            }
            MapEvent::Status { id, value } => {
                let before = game.status.insert(id, value);
                game.level_ups.changed(id, before, value);
                // Our walking speed. ponytail: a walk under way keeps its times; the next one goes at the new speed.
                if id == net::STATUS_SPEED && value > 0 {
                    game.player.step_ms = value as f32;
                }
            }
            MapEvent::Statuses(values) => game.status.extend(values),
            // The raise's answer, taken or refused, lets the next one go.
            MapEvent::RaiseAnswer(value) => {
                game.status_window.waiting = false;
                game.status.extend(value);
            }
            // -1 takes the bar away.
            MapEvent::UnitHp { id, hp, max_hp } if hp < 0 || max_hp <= 0 => {
                game.unit_hp.remove(&id);
            }
            MapEvent::UnitHp { id, hp, max_hp } => {
                game.unit_hp.insert(id, (hp, max_hp));
            }
            MapEvent::AttackRange(range) => game.attack_range = i32::from(range),
            MapEvent::GroundItem(item) => {
                let resource = ground_sprite(&self.items, item.item, item.identified).to_owned();
                game.item_sprites.entry(resource.clone()).or_insert_with(|| SpriteSet::at(grf, &format!("data\\sprite\\아이템\\{resource}")));
                let [x, y] = item.cell;
                // Sixteenths of a cell from its low corner (8 is the middle).
                let [sub_x, sub_y] = item.sub.map(|sub| f32::from(sub) / 16.0);
                game.ground.insert(
                    item.id,
                    FloorItem {
                        item: item.item,
                        identified: item.identified,
                        amount: item.amount,
                        at: [i32::from(x), i32::from(y)],
                        cell: [f32::from(x) - 0.5 + sub_x, f32::from(y) - 0.5 + sub_y],
                        seen: Instant::now(),
                        dropped: item.falling.then(Instant::now),
                    },
                );
                // The item's light pillar, from the fall packet's drop effect and the item info's own effect number.
                if let Some(gpu) = gpu {
                    let at = game.world.ground_under(game.world.cell_position([f32::from(x) - 0.5 + sub_x, f32::from(y) - 0.5 + sub_y]));
                    let info = self.items.get(&item.item).and_then(|info| info.effect_id);
                    game.effects.item_dropped(gpu, grf, table, item.id, info, (item.show_effect, item.effect_mode), at, Instant::now());
                }
            }
            MapEvent::ItemGone { id } => {
                game.effects.item_gone(id);
                game.ground.remove(&id);
                game.pickups_asked.remove(&id);
                if game.pending.is_some_and(|(pending, _)| pending == Pending::PickUp(id)) {
                    game.pending = None;
                }
            }
            MapEvent::PickedUp { result, item } => {
                game.pickups_asked.clear();
                let message = |id: usize| self.messages.get(id).cloned().unwrap_or_default();
                let held = game.inventory.get(item.index).map_or(0, |held| held.amount);
                // A forged or brewed item names its maker: asked for once.
                if let Some(maker) = inventory::maker(&item.cards)
                    && !game.char_names.contains_key(&maker)
                    && game.names_asked.insert(maker)
                {
                    let _ = game.session.ask_char_name(maker);
                }
                let name = |slots: bool| inventory::full_name(&self.items, &self.card_names, &self.messages, &game.char_names, &item, slots);
                match pickup::obtained(result, item.amount, held) {
                    Some(amount) => {
                        let line = messages::fill(&message(messages::GOT_ITEM), &[&name(true), &amount]);
                        // The popup: the name without its slot count, how many came, the icon.
                        let (short, maker) = inventory::full_name_maker(&self.items, &self.card_names, &self.messages, &game.char_names, &item, false);
                        let text = format!("{short}{}", messages::fill(&message(pickup::OBTAINED), &[&amount]));
                        let icon = self.items.get(&item.id).map(|info| inventory::icon_path(info, item.identified)).unwrap_or_default();
                        let mut popup = pickup::Popup::new(text, icon, self.font.width(&line), control::now());
                        popup.maker = maker;
                        game.pickup_popup = Some(popup);
                        game.new_items = true;
                        game.chat_line_kind(line, CHAT_ITEM, chat::Kind::ItemLog);
                        game.inventory.mark_new(item.index);
                        game.inventory.add(net::InventoryItem { amount, ..item });
                    }
                    None => {
                        if let Some(id) = pickup::failure_line(result) {
                            game.chat_line(message(id), CHAT_ERROR);
                        }
                    }
                }
            }
            // The inventory again: what broke since says so; the out pet's egg (marked broken) stays hidden.
            MapEvent::InventoryStart { kind: 0, .. } => {
                let had = !game.inventory.items.is_empty();
                game.broken_before = had.then(|| game.inventory.items.iter().filter(|item| item.broken).map(|item| item.index).collect());
                game.inventory.clear();
            }
            MapEvent::InventoryItems { kind: 0, items } => {
                // ponytail: rathena never sends ZC_EQUIPITEM_DAMAGED, so the client notices a break in the resent list.
                if let Some(before) = &game.broken_before {
                    let message = |id: usize| self.messages.get(id).map_or("", String::as_str);
                    let lines: Vec<String> = inventory::newly_broken(before, &items)
                        .into_iter()
                        .map(|item| {
                            let place = message(if item.kind == net::ITEM_WEAPON { DAMAGED_WEAPON } else { DAMAGED_ARMOR });
                            messages::fill(message(DAMAGED), &[&game.me.name, &place])
                        })
                        .collect();
                    for line in lines {
                        game.chat_line(line, CHAT_ERROR);
                    }
                }
                if let Some(egg) = items.iter().find(|item| inventory::hidden_egg(item)) {
                    game.pet_window.egg = Some(egg.id);
                }
                game.inventory.extend(items.into_iter().filter(|item| !inventory::hidden_egg(item)).collect());
            }
            MapEvent::BoundOnEquip(index) => {
                if let Some(item) = game.inventory.items.iter_mut().find(|item| item.index == index) {
                    item.bind = 2;
                    let name = inventory::item_name(&self.items, item.id, item.identified, item.refine);
                    game.chat_line(messages::fill(self.messages.get(BOUND).map_or("", String::as_str), &[&name]), CHAT_ITEM);
                }
            }
            // ponytail: no line fits a refusal; it says nothing.
            MapEvent::TookOffAll { .. } => {}
            MapEvent::SwitchSet(set) => {
                for item in &mut game.inventory.items {
                    item.switch = set.iter().find(|(index, _)| *index == item.index).map_or(0, |(_, places)| *places);
                }
            }
            // The player's own add and remove get a line.
            // ponytail: whether the client also says so for items a switch moves isn't knowable; it doesn't.
            MapEvent::SwitchAdded { index, places, result } => {
                let message = |id: usize| self.messages.get(id).cloned().unwrap_or_default();
                let asked = game.switch_asked.take_if(|asked| *asked == index).is_some();
                let name = game.inventory.items.iter_mut().find(|item| item.index == index && result == 0).map(|item| {
                    item.switch = places;
                    inventory::item_name(&self.items, item.id, item.identified, item.refine)
                });
                match result {
                    0 if asked => game.chat_line(format!("{}{}", name.unwrap_or_default(), message(SWITCH_ADDED)), CHAT_ITEM),
                    0 => {}
                    1 => game.chat_line(message(1773), CHAT_ERROR),
                    _ => game.chat_line(message(372), CHAT_ERROR),
                }
            }
            MapEvent::SwitchRemoved { index, .. } => {
                let asked = game.switch_asked.take_if(|asked| *asked == index).is_some();
                let name = game.inventory.items.iter_mut().find(|item| item.index == index).map(|item| {
                    item.switch = 0;
                    inventory::item_name(&self.items, item.id, item.identified, item.refine)
                });
                if let Some(name) = name.filter(|_| asked) {
                    game.chat_line(format!("{name}{}", self.messages.get(SWITCH_REMOVED).cloned().unwrap_or_default()), CHAT_ITEM);
                }
            }
            // The wear and take-off answers that follow move the items.
            MapEvent::Switched { .. } => {}
            MapEvent::FavoriteMoved { index, favorite } => {
                if let Some(item) = game.inventory.items.iter_mut().find(|item| item.index == index) {
                    item.favorite = favorite;
                }
            }
            // The storage opens under the server's name for it; the inventory stays as it was.
            MapEvent::InventoryStart { kind: 2, name } => {
                game.storage.open(name);
                game.raise(Panel::Storage);
            }
            MapEvent::InventoryItems { kind: 2, items } => game.storage.extend(items),
            MapEvent::InventoryStart { kind: 1, .. } => game.cart.clear(),
            MapEvent::InventoryItems { kind: 1, items } => game.cart.extend(items),
            MapEvent::InventoryStart { .. } | MapEvent::InventoryItems { .. } => {}
            MapEvent::Quests(quests) => game.quests = quests,
            // A new quest: its line and icon; one already known is refreshed in place.
            MapEvent::QuestAdded(quest) => {
                if quest_notice::add(&mut game.quests, quest) {
                    game.chat_line_kind(self.messages.get(quest_notice::RECEIVED).cloned().unwrap_or_default(), quest_notice::YELLOW, chat::Kind::Quest);
                    game.quest_icon = true;
                }
            }
            MapEvent::QuestRemoved(id) => {
                if game.quests.iter().any(|quest| quest.id == id) {
                    let title = self.quest_texts.get(&id).map_or_else(|| id.to_string(), |text| text.title.clone());
                    let line = messages::fill(self.messages.get(quest_notice::DELETED).map_or("", String::as_str), &[&title]);
                    game.chat_line_kind(line, quest_notice::YELLOW, chat::Kind::Quest);
                }
                game.quests.retain(|quest| quest.id != id);
                if game.quest_window.picked == Some(id) {
                    game.quest_window.picked = None;
                }
            }
            MapEvent::QuestActive { id, active } => {
                if let Some(quest) = game.quests.iter_mut().find(|quest| quest.id == id) {
                    quest.active = active;
                }
            }
            // A counted kill: the counts, a progress line each and its banner.
            MapEvent::QuestProgress(hunts) => {
                let now = control::now();
                for (id, count, needed) in hunts {
                    let Some(quest) = game.quests.iter_mut().find(|quest| quest.hunts.iter().any(|hunt| hunt.id == id)) else { continue };
                    let quest_id = quest.id;
                    let hunt = quest.hunts.iter_mut().find(|hunt| hunt.id == id).unwrap();
                    (hunt.count, hunt.needed) = (count, needed);
                    let hunt = hunt.clone();
                    let title = self.quest_texts.get(&quest_id).map_or_else(|| quest_id.to_string(), |text| text.title.clone());
                    game.chat_line_kind(quest_notice::progress_line(&self.messages, &title, &hunt), quest_notice::YELLOW, chat::Kind::Quest);
                    game.banners.show(quest_notice::banner_line(&self.messages, &hunt), now);
                }
            }
            MapEvent::QuestHunts(hunts) => {
                for (id, count, needed) in hunts {
                    if let Some(hunt) = game.quests.iter_mut().flat_map(|quest| &mut quest.hunts).find(|hunt| hunt.id == id) {
                        (hunt.count, hunt.needed) = (count, needed);
                    }
                }
            }
            // RODEX. Closing the window written in after sending lets the server know writing is over.
            event @ (MapEvent::MailUnread(_)
            | MapEvent::MailList(_)
            | MapEvent::MailRead { .. }
            | MapEvent::MailClaimed { .. }
            | MapEvent::MailDeleted { .. }
            | MapEvent::MailWrite { .. }
            | MapEvent::MailReceiver { .. }
            | MapEvent::MailItemAdded { .. }
            | MapEvent::MailItemRemoved { .. }
            | MapEvent::MailSent(_)) => {
                if matches!(event, MapEvent::MailSent(0)) {
                    let _ = game.session.cancel_mail();
                }
                if let Some((id, error)) = game.mail.event(event) {
                    game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), if error { CHAT_ERROR } else { CHAT_ITEM });
                }
            }
            // Announcements, and the client's own messages by number.
            // In the chat and the bar; a blocked speaker's not at all.
            MapEvent::Broadcast { text, color, kind, size } => {
                if let net::BroadcastKind::Speaker(name) = &kind
                    && game.blocked.contains(name)
                {
                    return;
                }
                game.chat_line_kind(text.clone(), color, chat::Kind::Broadcast);
                game.announcements.push(text, color, size.unwrap_or(announce::SIZE), control::now());
            }
            // ponytail: in the announcements' yellow; a monster's drop says the box's line with the monster's name, as
            // rathena's notes have it, unchecked against the real client.
            MapEvent::ItemObtained { item, player, box_item, monster } => {
                let name = |id: u32| inventory::item_name(&self.items, id, true, 0);
                let template = |id: usize| self.messages.get(id).map_or("", String::as_str);
                let text = match (box_item, monster) {
                    (Some(container), _) => messages::fill(template(ITEM_FROM_BOX), &[&player, &name(container), &name(item)]),
                    (_, Some(monster)) => messages::fill(template(ITEM_FROM_BOX), &[&player, &monster, &name(item)]),
                    _ => messages::fill(template(ITEM_FROM_NPC), &[&player, &name(item)]),
                };
                game.chat_line_kind(text, CHAT_BROADCAST, chat::Kind::Broadcast);
            }
            MapEvent::RefineAnnounced { player, item, level, success } => {
                let template = self.messages.get(if success { REFINE_SUCCEEDED } else { REFINE_FAILED }).map_or("", String::as_str);
                let text = messages::fill(template, &[&player, &level, &inventory::item_name(&self.items, item, true, 0)]);
                game.chat_line_kind(text.clone(), CHAT_REFINE_ANNOUNCEMENT, chat::Kind::Broadcast);
                game.announcements.push(text, CHAT_REFINE_ANNOUNCEMENT, announce::SIZE, control::now());
            }
            // The same yellow as a refine announcement. Refine 0 so the name has no +N.
            // ponytail: the grade's letter is a guess.
            MapEvent::GradeAnnounced { player, item, grade: next, success } => {
                let template = self.messages.get(if success { grade::ANNOUNCE_OK } else { grade::ANNOUNCE_FAIL }).map_or("", String::as_str);
                let name = inventory::item_name(&self.items, item, true, 0);
                game.chat_line_kind(messages::fill(template, &[&player, &grade::grade_letter(next), &name]), CHAT_BROADCAST, chat::Kind::Broadcast);
            }
            // The banner effect, and the MVP sprite over the winner.
            // ponytail: ZC_MVP starts the sprite and it plays once through, like an emotion; neither is pinned down.
            MapEvent::Mvp(id) => {
                if let Some(gpu) = gpu {
                    game.play_effect(gpu, grf, table, EF_MVP, id);
                }
                game.mvps.insert((id != game.account_id).then_some(id), control::now());
            }
            // ponytail: the reward lines in the items' cyan; the real client's color is unknown.
            MapEvent::MvpItem(item) => {
                let [before, after] = MVP_ITEM.map(|id| self.messages.get(id).cloned().unwrap_or_default());
                game.chat_line(format!("{before}{}{after}", inventory::item_name(&self.items, item, true, 0)), CHAT_ITEM);
            }
            MapEvent::MvpExp(exp) => {
                let [before, after] = MVP_EXP.map(|id| self.messages.get(id).cloned().unwrap_or_default());
                game.chat_line_kind(format!("{before}{exp}{after}"), CHAT_ITEM, chat::Kind::Experience);
            }
            MapEvent::OverweightPercent(percent) => game.overweight = percent,
            MapEvent::GuildMemberPlace { account, cell: Some(cell) } => {
                game.guild_places.insert(account, cell);
            }
            MapEvent::GuildMemberPlace { account, cell: None } => {
                game.guild_places.remove(&account);
            }
            MapEvent::MvpOverweight => game.chat_line(self.messages.get(MVP_OVERWEIGHT).cloned().unwrap_or_default(), CHAT_ERROR),
            MapEvent::HomunculusState { state: net::HOMUNCULUS_OWNED, id, .. } => game.homunculus_id = Some(id),
            MapEvent::HomunculusState { state: net::HOMUNCULUS_INTIMACY, value, .. } => {
                if let Some(homunculus) = &mut game.homunculus {
                    homunculus.intimacy = value as u16;
                }
            }
            MapEvent::HomunculusState { state: net::HOMUNCULUS_HUNGER, value, .. } => {
                // Only the full status says "starving".
                let hunger = value.min(u32::from(u16::MAX)) as u16;
                if let Some(homunculus) = &mut game.homunculus {
                    homunculus.hunger = hunger;
                }
            }
            MapEvent::HomunculusState { .. } => {}
            MapEvent::HomunculusInfo(info) => {
                if homunculus::starving_now(game.homunculus_warned, info.hunger) {
                    game.homunculus_warned = Some(info.hunger);
                    game.chat_line(self.messages.get(homunculus::STARVING).cloned().unwrap_or_default(), CHAT_ERROR);
                }
                game.homunculus = Some(info);
            }
            MapEvent::HomunculusAutoFeed(on) => {
                game.homunculus_window.auto_feed = on;
                let line = self.messages.get(if on { homunculus::AUTO_FEED_ON } else { homunculus::AUTO_FEED_OFF });
                game.chat_line(line.cloned().unwrap_or_default(), homunculus::AUTO_FEED_COLOR);
            }
            MapEvent::HomunculusChanged { kind, value } => {
                if let Some(homunculus) = &mut game.homunculus {
                    homunculus.set(kind, value);
                }
            }
            MapEvent::HomunculusSkills(list) => game.homunculus_skills = list,
            MapEvent::HomunculusSkill(update) => match game.homunculus_skills.iter_mut().find(|skill| skill.id == update.id) {
                Some(skill) => {
                    skill.level = update.level;
                    skill.sp = update.sp;
                    skill.range = update.range;
                    skill.upgradable = update.upgradable;
                }
                None => game.homunculus_skills.push(update),
            },
            // Fed: the food and how many are left (rathena takes it before it answers); not fed: what it eats.
            MapEvent::HomunculusFed { ok, food } => {
                let name = inventory::item_name(&self.items, food, true, 0);
                let message = |id: usize| self.messages.get(id).map_or("", String::as_str);
                if ok {
                    let left: u32 = game.inventory.items.iter().filter(|item| item.id == food).map(|item| u32::from(item.amount)).sum();
                    game.chat_line(messages::fill(message(homunculus::FED), &[&name, &left]), homunculus::FED_COLOR);
                } else {
                    game.chat_line(messages::fill(message(homunculus::NOT_FED), &[&name]), homunculus::NOT_FED_COLOR);
                }
            }
            // The contract's end, as a local date and time by the message table's pattern.
            MapEvent::MercenaryInfo(hired) => {
                let pattern = self.messages.get(mercenary::CONTRACT_PATTERN).map_or("", String::as_str);
                game.mercenary_window.contract = rustnarok_client::lua::local_date(pattern, i64::from(hired.contract_end));
                game.mercenary = Some(hired);
            }
            MapEvent::ElementalInfo { id, values } => game.elemental = Some((id, values)),
            MapEvent::ElementalChanged { kind, value } => {
                if let Some((_, values)) = &mut game.elemental
                    && let Some(slot) = kind.checked_sub(net::MERCENARY_HP).and_then(|at| values.get_mut(usize::from(at)))
                {
                    *slot = value;
                }
            }
            MapEvent::MercenarySkills(list) => game.mercenary_skills = list,
            MapEvent::MercenaryChanged { kind, value } => {
                if let Some(hired) = &mut game.mercenary {
                    hired.set(kind, value);
                }
            }
            MapEvent::Message { id, value, skill, color } => {
                // The contract is over, or the mercenary died: it's gone.
                if [mercenary::EXPIRED, mercenary::DIED].contains(&id) {
                    game.forget_mercenary(true);
                }
                let template = self.messages.get(id).map_or("", String::as_str);
                let text = match (value, skill) {
                    (Some(value), _) => messages::fill(template, &[&value]),
                    (_, Some(skill)) => format!("[{}] {template}", self.skill_info.get(&skill).map_or_else(|| skill.to_string(), |info| info.name.clone())),
                    _ => template.to_owned(),
                };
                // ponytail: plain ones in red, as nearly all of them are refusals; calibrate against the real client.
                game.chat_line_kind(
                    text,
                    color.unwrap_or(if skill.is_some() { CHAT_SKILL_MESSAGE } else { CHAT_ERROR }),
                    if skill.is_some() { chat::Kind::SkillFailure } else { chat::Kind::General },
                );
            }
            MapEvent::ActionFailure(kind) => {
                let id = ACTION_FAILURES.get(usize::from(kind)).copied().unwrap_or(ACTION_FAILURES[0]);
                game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), if kind == 3 { CHAT_ITEM } else { CHAT_ERROR });
            }
            // What an NPC's script plays: a sound from where the unit stands, music until the map changes.
            // ponytail: a repeated sound (act 1) plays once and a stop (act 2) is let go; rathena notes neither works.
            MapEvent::Sound { file, act: 0 | 1, unit } => {
                let me = game.player.cell;
                let at = if unit == game.account_id { Some(me) } else { game.units.get(&unit).map(|actor| actor.cell) };
                let distance = at.map_or(0.0, |[x, y]| f32::hypot((x - me[0]) as f32, (y - me[1]) as f32));
                if let Some(audio) = &mut self.audio {
                    audio.play(grf, &file, heard(distance));
                }
            }
            MapEvent::Sound { .. } => {}
            MapEvent::NpcMusic(file) => {
                if let Some(audio) = &mut self.audio {
                    audio.play_music(Some(&sound::music_path(&file)));
                }
            }
            MapEvent::MemoResult(result) => {
                let id = MEMO_RESULTS.get(usize::from(result)).copied().unwrap_or(MEMO_RESULTS[2]);
                game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), if result == 0 { CHAT_ITEM } else { CHAT_ERROR });
            }
            MapEvent::MapForbids(kind) => {
                if let Some(&id) = MAP_FORBIDS.get(usize::from(kind)) {
                    game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ERROR);
                }
            }
            // Vending: setting up, our shop, others' signs and goods.
            MapEvent::VendSetup(most) => {
                let mut setup = vend::Setup::new(most, game.vend_size);
                // Dev switch RUSTNAROK_CLIENT_VEND=<name>:<price>: everything in the cart at that price, and open.
                if let Some((name, price)) = std::env::var("RUSTNAROK_CLIENT_VEND")
                    .ok()
                    .and_then(|vend| vend.split_once(':').map(|(name, price)| (name.to_owned(), price.parse().unwrap_or(1))))
                {
                    setup.type_text(&name);
                    for item in game.cart.items.iter().take(usize::from(most)) {
                        setup.prices.insert(item.index, price);
                    }
                    let _ = game.session.open_shop(&name, true, &setup.goods(&game.cart.items));
                }
                game.vend_setup = Some(setup);
            }
            // Refused for a zero price (2); other refusals the server explains itself.
            MapEvent::VendOpened(2) => game.chat_line(self.messages.get(VEND_FAILED).cloned().unwrap_or_default(), CHAT_ERROR),
            MapEvent::VendOpened(_) => {}
            MapEvent::MyShop(goods) => {
                let name = game.vend_setup.take().map(|setup| setup.name).unwrap_or_default();
                game.signs.insert(game.account_id, chatroom::Sign::shop(game.account_id, name.clone()));
                game.my_shop = Some(vend::MyShop::new(name, goods, game.vend_size));
            }
            MapEvent::VendSold { index, amount, .. } => {
                let Some(shop) = &mut game.my_shop else { return };
                let name = shop
                    .goods
                    .iter()
                    .find(|good| good.index == index)
                    .map(|good| inventory::item_name(&self.items, good.item.id, good.item.identified, good.item.refine));
                let sold_out = shop.sold(index, amount);
                if let Some(name) = name {
                    game.chat_line(
                        rustnarok_client::messages::fill(self.messages.get(VEND_SOLD).map_or("", String::as_str), &[&name, &amount]),
                        VEND_SOLD_COLOR,
                    );
                }
                // Sold out: the client closes the shop itself.
                if sold_out {
                    game.chat_line(self.messages.get(VEND_SOLD_OUT).cloned().unwrap_or_default(), CHAT_SELF);
                    game.close_esc(escape::WindowId::MyShop);
                }
            }
            MapEvent::VendorSign { id, title } => {
                game.signs.insert(id, chatroom::Sign::shop(id, title));
            }
            MapEvent::VendorGone { id } => {
                game.signs.remove(&id);
            }
            MapEvent::VendorGoods { vendor, shop: shop_id, goods } => {
                // A store opened from the catalog may stand out of sight: its name as the search found it.
                let name = game.signs.get(&vendor).map(|sign| sign.title.clone()).unwrap_or_else(|| {
                    game.catalog
                        .as_ref()
                        .and_then(|catalog| catalog.rows.iter().find(|row| row.account == vendor))
                        .map(|row| row.shop.clone())
                        .unwrap_or_default()
                });
                let offers = goods
                    .into_iter()
                    .map(|good| shop::Offer {
                        item: good.item.id,
                        price: good.price,
                        index: good.index,
                        have: good.item.amount,
                        kind: good.item.kind,
                        identified: good.item.identified,
                        refine: good.item.refine,
                        ..Default::default()
                    })
                    .collect();
                let mut store = shop::Shop::new(false, offers);
                store.vendor = Some((vendor, shop_id, name));
                game.open_shop(store);
                game.auto_deal();
            }
            MapEvent::VendBought { result, .. } => {
                let id = match result {
                    0 => None,
                    1 => Some(55),
                    2 => Some(56),
                    4 => Some(VEND_SOLD_OUT),
                    _ => Some(57),
                };
                if let Some(id) = id {
                    game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ERROR);
                    game.shop = None;
                }
            }
            MapEvent::OpenBuyingStore { slots } => {
                game.buying_setup = Some(buying::Setup::new(slots));
            }
            MapEvent::BuyingStoreOpenFailed { result, .. } => {
                let id = match result {
                    1 => Some(1741),
                    2 => Some(1742),
                    8 => Some(1797),
                    _ => None,
                };
                if let Some(id) = id {
                    game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ERROR);
                }
            }
            MapEvent::MyBuyingStore { limit, items, .. } => {
                let name = game.buying_setup.take().map(|setup| setup.name).unwrap_or_default();
                game.signs.insert(game.account_id, chatroom::Sign::buying_store(game.account_id, name.clone()));
                game.my_buying_store = Some(buying::MyStore::new(name, limit, items));
            }
            MapEvent::BuyingStoreUpdate { item_id, amount, zeny, limit_left, .. } => {
                if let Some(store) = &mut game.my_buying_store {
                    store.bought(item_id, amount, limit_left);
                }
                // ponytail: 1736 reads like the owner's line for a purchase; which line the official client prints isn't knowable from rathena.
                let name = inventory::item_name(&self.items, item_id, true, 0);
                let paid = shop::group(u64::from(zeny));
                let line = rustnarok_client::messages::fill(self.messages.get(1736).map_or("", String::as_str), &[&name, &paid, &amount]);
                game.chat_line(line, CHAT_ITEM);
            }
            MapEvent::BuyingStoreBuyerFailed { result } => {
                game.my_buying_store = None;
                game.signs.remove(&game.account_id);
                let id = match result {
                    3 => Some(1743),
                    4 => Some(1744),
                    _ => None,
                };
                if let Some(id) = id {
                    game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ITEM);
                }
            }
            MapEvent::BuyingStoreSign { owner, title } => {
                game.signs.insert(owner, chatroom::Sign::buying_store(owner, title));
            }
            MapEvent::BuyingStoreSignGone { owner } => {
                game.signs.remove(&owner);
            }
            MapEvent::BuyingStoreItems { owner, store_id, limit, items } => {
                game.open_shop(shop::Shop::new_buying_store(owner, store_id, limit, &items, &game.inventory.items));
            }
            MapEvent::BuyingStoreSold { index, amount, price } => {
                let name = game.inventory.get(index).map(|item| inventory::item_name(&self.items, item.id, item.identified, item.refine)).unwrap_or_default();
                game.inventory.remove(index, amount);
                let total = u64::from(price) * u64::from(amount);
                let line = rustnarok_client::messages::fill(self.messages.get(1746).map_or("", String::as_str), &[&name, &amount, &total]);
                game.chat_line(line, CHAT_ITEM);
                // ponytail: whether the official client keeps the seller's window open isn't knowable.
                game.shop = None;
            }
            MapEvent::BuyingStoreSellerFailed { result, item_id } => {
                let line = match result {
                    5 => self.messages.get(57).cloned(),
                    6 => {
                        let name = inventory::item_name(&self.items, item_id, true, 0);
                        Some(rustnarok_client::messages::fill(self.messages.get(1747).map_or("", String::as_str), &[&name]))
                    }
                    7 => self.messages.get(1745).cloned(),
                    _ => self.messages.get(57).cloned(),
                };
                if let Some(line) = line {
                    game.chat_line(line, CHAT_ERROR);
                }
                // ponytail: whether the official client keeps the seller's window open isn't knowable.
                game.shop = None;
            }
            MapEvent::CartCount { most, weight, max_weight, .. } => game.cart.cart = Some((most, [weight, max_weight])),
            MapEvent::CartAdded(item) => game.cart.add(item),
            MapEvent::CartRemoved { index, amount } => game.cart.remove(index, amount.min(u32::from(u16::MAX)) as u16),
            MapEvent::StorageCount { amount, max } => {
                game.storage.count = [amount, max];
                game.auto_store();
            }
            MapEvent::StorageAdded(mut item) => {
                // Where equipment goes isn't sent with it; the inventory still holds it.
                if let Some(carried) = game.inventory.items.iter().find(|carried| carried.id == item.id) {
                    item.location = carried.location;
                }
                game.storage.add(item);
            }
            MapEvent::StorageRemoved { index, amount } => game.storage.remove(index, amount),
            // It only ends a quick move's wait (above).
            MapEvent::MoveFailed { .. } => {}
            MapEvent::StorageClosed => game.storage.close(),
            // Going back to character select: the map session closes and the char server lists the characters again.
            MapEvent::RestartAck(1) => {
                let Some(account) = self.account.clone() else { return };
                let proxy = self.proxy.clone();
                self.scene = Scene::Login;
                self.playing = None;
                self.returned = true;
                self.play_music(None);
                match CharSession::connect(&account.char_servers[self.char_server.min(account.char_servers.len() - 1)], &account, move |event| {
                    let _ = proxy.send_event(NetEvent::Char(event));
                }) {
                    Ok(session) => {
                        self.char_session = Some(session);
                        self.wait(Wait::CharServer);
                    }
                    Err(_) => self.message("서버에 접속할 수 없습니다.", Scene::Login),
                }
            }
            MapEvent::RestartAck(_) | MapEvent::QuitAck(1..) => game.chat_line(self.messages.get(LOGOUT_WAIT).cloned().unwrap_or_default(), CHAT_ERROR),
            MapEvent::QuitAck(_) => self.quitting = true,
            MapEvent::Emotion { id, kind } => {
                if let Some(action) = emotion::sprite_action(kind) {
                    game.emotions.insert((id != game.account_id).then_some(id), (action, Instant::now()));
                }
            }
            MapEvent::Revived { id } => {
                if id == game.account_id {
                    game.escape = None;
                    game.player.revive();
                } else if let Some(actor) = game.units.get_mut(&id) {
                    actor.revive();
                }
            }
            // Chat rooms: signs over heads, the one we're in.
            MapEvent::ChatRoom { owner, id, limit, users, kind, title } => {
                let sign = chatroom::Sign { id, limit, users, kind, title };
                if let Some(room) = game.room.as_mut().filter(|room| room.sign.id == id || owner == game.account_id) {
                    room.sign = chatroom::Sign { id: room.sign.id.max(id), ..sign.clone() };
                }
                if owner != game.account_id {
                    game.signs.insert(owner, sign);
                }
            }
            MapEvent::ChatRoomGone { id } => game.signs.retain(|_, sign| sign.id != id),
            MapEvent::ChatRoomMade(result) => {
                let message = |id: usize| self.messages.get(id).cloned().unwrap_or_default();
                match result {
                    0 => {
                        if let Some(make) = game.room_make.take() {
                            let sign =
                                chatroom::Sign { id: 0, limit: make.limit(), users: 1, kind: u8::from(make.public), title: make.title.trim().to_owned() };
                            game.room = Some(chatroom::Room::new(sign, vec![(game.me.name.clone(), true)]));
                        }
                        game.chat_line(message(64), CHAT_ITEM);
                    }
                    1 => game.chat_line(message(65), CHAT_ERROR),
                    _ => game.chat_line(message(66), CHAT_ERROR),
                }
            }
            MapEvent::ChatRoomRefused(result) => {
                let id = match result {
                    0 => Some(67),
                    1 => Some(7),
                    2 => Some(68),
                    4 => Some(55),
                    5 => Some(432),
                    6 => Some(433),
                    7 => Some(434),
                    _ => None,
                };
                if let Some(id) = id {
                    game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ERROR);
                }
            }
            MapEvent::ChatRoomEntered { id, members } => {
                let sign = game.signs.values().find(|sign| sign.id == id).cloned().unwrap_or(chatroom::Sign {
                    id,
                    limit: 0,
                    users: members.len() as u16,
                    kind: 1,
                    title: String::new(),
                });
                game.room = Some(chatroom::Room::new(chatroom::Sign { users: members.len() as u16, ..sign }, members));
            }
            MapEvent::ChatRoomJoined { users, name } => {
                if let Some(room) = &mut game.room {
                    room.sign.users = users;
                    room.lines.push((rustnarok_client::messages::fill(self.messages.get(179).map_or("", String::as_str), &[&name]), chatroom::JOIN_LINE));
                    room.members.push((name, false));
                }
            }
            MapEvent::ChatRoomLeft { users, name, kicked } => {
                if name == game.me.name {
                    game.room = None;
                    if kicked {
                        game.chat_line(self.messages.get(68).cloned().unwrap_or_default(), CHAT_ERROR);
                    }
                } else if let Some(room) = &mut game.room {
                    room.sign.users = users;
                    room.members.retain(|(member, _)| *member != name);
                    let line = rustnarok_client::messages::fill(self.messages.get(if kicked { 181 } else { 180 }).map_or("", String::as_str), &[&name]);
                    room.lines.push((line, chatroom::LEAVE_LINE));
                }
            }
            MapEvent::ChatRoomOwner { owner, name } => {
                for (member, owns) in game.room.iter_mut().flat_map(|room| room.members.iter_mut()) {
                    if *member == name {
                        *owns = owner;
                    }
                }
            }
            // The guild: what it is, its pages, its talk, and who comes and goes.
            MapEvent::Guild { id, name, master, emblem, mode } => {
                let guild = game.guild.get_or_insert_with(guild::Guild::default);
                guild.id = id;
                guild.name = name;
                guild.master = master;
                guild.emblem = emblem;
                guild.mode = mode;
                game.want_emblem(id, emblem);
            }
            MapEvent::GuildMenu(menu) => game.guild_window.menu = menu,
            MapEvent::GuildInfo(info) => {
                if let Some(guild) = &mut game.guild {
                    // We lead it when its master's name is ours, too.
                    if info.master == game.me.name {
                        guild.master = true;
                    }
                    guild.name.clone_from(&info.name);
                    guild.info = Some(info);
                }
            }
            MapEvent::GuildMembers(members) => {
                if let Some(guild) = &mut game.guild {
                    guild.members = members;
                }
            }
            MapEvent::GuildPositions(positions) => {
                if let Some(guild) = &mut game.guild {
                    guild.positions = positions.into_iter().collect();
                }
            }
            MapEvent::GuildPositionChanged(position, title) => {
                if let Some(guild) = &mut game.guild {
                    guild.change_position(position, title);
                }
            }
            MapEvent::GuildMemberPosition { character, position, .. } => {
                if let Some(guild) = &mut game.guild {
                    guild.move_member(character, position);
                }
            }
            // The line under a player's name; a tag not known yet waits for the whole of it.
            MapEvent::PositionTitle { account, title } => {
                if let Some(tag) = game.name_tags.get_mut(&account) {
                    tag.position = title;
                }
            }
            MapEvent::GuildPositionInfo(info) => {
                if let Some(guild) = &mut game.guild {
                    guild.position_info = info;
                }
            }
            MapEvent::GuildSkills { points, skills } => {
                if let Some(guild) = &mut game.guild {
                    guild.skill_points = points;
                    guild.skills = skills;
                }
            }
            MapEvent::GuildExpelled(expelled) => {
                if let Some(guild) = &mut game.guild {
                    guild.expelled = expelled;
                }
            }
            MapEvent::GuildEmblem { guild, version, data } => {
                if let Some(gpu) = gpu {
                    match guild::decode_emblem(&data) {
                        Some(image) => _ = game.emblems.insert(guild, (version, Some(gpu.texture(&image, false)))),
                        None => eprintln!("guild {guild}: emblem {version} isn't a compressed BMP"),
                    }
                }
            }
            MapEvent::GuildEmblemChanged { account, guild, emblem } => {
                // Joining a guild brings this (with its info) rather than ZC_UPDATE_GDID.
                if account == game.account_id {
                    if guild == 0 {
                        game.guild = None;
                        game.guild_window.frame.open = false;
                        game.guild_window.menu = guild::MENU_UNKNOWN;
                    } else if game.guild.as_ref().is_none_or(|ours| ours.id != guild) {
                        game.guild = Some(guild::Guild { id: guild, ..Default::default() });
                    }
                    if let Some(ours) = &mut game.guild {
                        ours.emblem = emblem;
                    }
                } else if let Some(actor) = game.units.get_mut(&account) {
                    actor.guild = guild;
                }
                game.want_emblem(guild, emblem);
            }
            MapEvent::NameTag(id, tag) => {
                if id == game.account_id {
                    game.equipment.title = tag.title;
                }
                game.name_tags.insert(id, tag);
            }
            // The notice: "[ subject ]" and "[ text ]" while /loginout is on; kept either way.
            MapEvent::GuildNotice { subject, text } => {
                if game.login_notices {
                    for line in guild::notice_lines(&subject, &text) {
                        game.chat_line(line, guild::NOTICE_LINE);
                    }
                }
                if let Some(guild) = &mut game.guild {
                    guild.notice = (subject, text);
                }
            }
            // Guild chat, unless /notalkmsg2 hides it.
            MapEvent::GuildChat(text) => {
                if !game.no_guild_talk {
                    game.chat_line(text, GUILD_CHAT);
                }
            }
            MapEvent::GuildMemberOnline { account, character, online } => {
                let member =
                    game.guild.as_mut().and_then(|guild| guild.members.iter_mut().find(|member| member.account == account && member.character == character));
                // A member logging in or out: a guild line in cyan while /loginout is on, when their name is known.
                if let Some(member) = member {
                    member.online = online;
                    let name = member.name.clone();
                    if game.login_notices && !name.is_empty() {
                        let template = self.messages.get(GUILD_ONLINE + usize::from(!online)).map_or("", String::as_str);
                        game.chat_line_kind(rustnarok_client::messages::fill(template, &[&name]), guild::LINE_CYAN, chat::Kind::Guild);
                    }
                }
            }
            // Made: the companion and its windows close.
            MapEvent::GuildMade(result) => {
                if let Some((id, color)) = guild_companion::make_result(result) {
                    game.chat_line_kind(self.messages.get(id).cloned().unwrap_or_default(), color, chat::Kind::Guild);
                }
                if result == 0 {
                    game.guild_companion.close();
                }
            }
            MapEvent::GuildInvite { id, name } => {
                // Dev switch: join.
                if std::env::var("RUSTNAROK_CLIENT_GUILD").is_ok_and(|answer| answer.split(',').any(|part| part == "accept")) {
                    let _ = game.session.answer_guild(id, true);
                    game.guild_companion.close();
                } else {
                    game.guild_invite = Some((id, name));
                }
            }
            MapEvent::GuildRelations(relations) => {
                if let Some(guild) = &mut game.guild {
                    guild.relations = relations;
                }
            }
            MapEvent::AllyAsked { id, guild } => game.guild_asked = Some(GuildAsked::Ally { id, guild }),
            MapEvent::AllyResult(result) | MapEvent::AntagonistResult(result) => {
                let (line, color) = if matches!(event, MapEvent::AllyResult(_)) { guild::ally_result(result) } else { guild::antagonist_result(result) };
                game.chat_line(self.messages.get(line).cloned().unwrap_or_default(), color);
            }
            MapEvent::GuildRelationEnded { id, antagonist } => {
                if let Some(guild) = &mut game.guild {
                    guild.relations.retain(|relation| (relation.id, relation.antagonist) != (id, antagonist));
                }
            }
            // Disbanded: the guild is gone, as if the player had left it.
            MapEvent::GuildDisbanded(result) => {
                if let Some((line, gone)) = guild_companion::disband_result(result) {
                    if gone {
                        game.guild = None;
                        game.guild_window.frame.open = false;
                        game.guild_window.menu = guild::MENU_UNKNOWN;
                    }
                    game.chat_line(self.messages.get(line).cloned().unwrap_or_default(), guild_companion::YELLOW);
                }
            }
            MapEvent::GuildInviteResult(result) => {
                if let Some((id, color)) = guild::invite_result_line(result) {
                    game.chat_line_kind(self.messages.get(id).cloned().unwrap_or_default(), color, chat::Kind::Guild);
                }
            }
            // Someone left or was expelled: us (the guild goes), or another (they leave the list).
            MapEvent::GuildLeft { character, reason, expelled } => {
                let us = character == game.me.id;
                let name =
                    game.guild.as_ref().and_then(|guild| guild.members.iter().find(|member| member.character == character)).map(|member| member.name.clone());
                if us {
                    game.guild = None;
                    game.guild_window.frame.open = false;
                    game.guild_window.menu = guild::MENU_UNKNOWN;
                } else if let Some(guild) = &mut game.guild {
                    guild.members.retain(|member| member.character != character);
                }
                let nameless = self.messages.get(581).cloned().unwrap_or_default();
                for (id, value, color) in guild::leaving_lines(us, expelled, name.as_deref(), &reason, &nameless) {
                    let text = messages::fill(self.messages.get(id).map_or("", String::as_str), &[&value]);
                    game.chat_line_kind(text, color, chat::Kind::Guild);
                }
            }
            // The clan: its info at login or on joining, how many are online, leaving, and its chat.
            MapEvent::ClanInfo { id, name, master, map, allies, hostile } => {
                let clan = game.clan.get_or_insert_with(clan::Clan::default);
                clan.id = id;
                clan.name = name;
                clan.master = master;
                clan.map = map;
                clan.allies = allies;
                clan.hostile = hostile;
            }
            // rathena sends the info (0x098A) before the first count; a count without a clan is ignored.
            MapEvent::ClanOnlineCount { online, max } => {
                if let Some(clan) = &mut game.clan {
                    clan.online = online;
                    clan.max = max;
                }
            }
            MapEvent::ClanLeft => {
                game.clan = None;
                game.clan_window.frame.open = false;
            }
            MapEvent::ClanChat(text) => game.chat_line(text, CLAN_CHAT),
            // Friends: the list, who's online (said in the chat), asking and answering.
            MapEvent::Friends(friends) => game.friends = friends,
            MapEvent::FriendOnline { account, character, online } => {
                if let Some(friend) = game.friends.iter_mut().find(|friend| friend.account == account && friend.character == character) {
                    friend.online = online;
                    let text =
                        rustnarok_client::messages::fill(self.messages.get(FRIEND_ONLINE + usize::from(!online)).map_or("", String::as_str), &[&friend.name]);
                    game.chat_line(text, CHAT_ITEM);
                }
            }
            MapEvent::FriendAsked { account, character, name } => {
                // Dev switch: say yes.
                if std::env::var("RUSTNAROK_CLIENT_FRIEND").is_ok_and(|answer| answer == "accept") {
                    let _ = game.session.answer_friend(account, character, true);
                } else {
                    game.friend_asked = Some((account, character, name));
                }
            }
            MapEvent::FriendAdded { result, friend } => {
                let id = match result {
                    0 => 821,
                    1 => 822,
                    2 => 819,
                    _ => 820,
                };
                let text = rustnarok_client::messages::fill(self.messages.get(id).map_or("", String::as_str), &[&friend.name]);
                game.chat_line(text, if result == 0 { CHAT_ITEM } else { CHAT_ERROR });
                if result == 0 && game.friends.iter().all(|known| known.character != friend.character) {
                    game.friends.push(friend);
                }
            }
            MapEvent::FriendRemoved { account, character } => {
                game.friends.retain(|friend| !(friend.account == account && friend.character == character));
                game.party_window.picked = None;
            }
            // The party: kept member by member, as the window and minimap show it.
            MapEvent::Party { name, members } => {
                let mut party = game.party.take().unwrap_or_default();
                let old = std::mem::take(&mut party.members);
                party.name = name;
                // The list rebuilds the colours too: each member's place in it.
                party.members = members
                    .into_iter()
                    .enumerate()
                    .map(|(place, info)| party::Member {
                        info: info.clone(),
                        color: place,
                        ..old.iter().find(|member| member.info.account == info.account).cloned().unwrap_or_default()
                    })
                    .collect();
                game.party = Some(party);
                game.member_windows.retain(game.party.as_ref());
                // The windows saved for this character come back for the members in the list.
                if let Some(party) = &game.party {
                    let saved = party::parse_saved(&read_small_windows(&game.me.name));
                    game.member_windows.restore(&saved, party, &party::Grid::new(screen()));
                }
            }
            MapEvent::PartyMember { party: name, member, cell, .. } => {
                let party = game.party.get_or_insert_with(party::Party::default);
                party.name = name;
                let account = member.account;
                party.upsert(member);
                if let Some(member) = party.member(account) {
                    member.cell = Some(cell.map(i32::from));
                }
            }
            MapEvent::PartyLeft { account, result, .. } => {
                let message = |id: usize| self.messages.get(id).cloned().unwrap_or_default();
                match result {
                    // Leaving isn't allowed here.
                    2 => game.chat_line(message(1872), CHAT_ERROR),
                    3 => game.chat_line(message(1873), CHAT_ERROR),
                    _ if account == game.account_id => {
                        game.party = None;
                        game.party_dialog = None;
                        game.member_windows.retain(None);
                        write_small_windows(&game.me.name, "");
                        game.small_saved.clear();
                        game.chat_line(message(PARTY_LEFT), CHAT_ITEM);
                    }
                    _ => {
                        if let Some(party) = &mut game.party {
                            party.members.retain(|member| member.info.account != account);
                        }
                        game.member_windows.retain(game.party.as_ref());
                        game.unit_hp.remove(&account);
                    }
                }
            }
            // Made: the party window opens on its party tab.
            MapEvent::PartyMade(result) => {
                if let Some((id, tone)) = party::made_line(result) {
                    game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), if tone == party::Tone::Good { CHAT_ITEM } else { CHAT_ERROR });
                }
                if result == 0 {
                    game.open_party_tab();
                }
            }
            MapEvent::PartyInvite { party, name } => {
                game.party_invite = Some((party, name));
                // Dev switch: join.
                if std::env::var("RUSTNAROK_CLIENT_PARTY").is_ok_and(|order| order.split(',').any(|part| part == "accept")) {
                    game.party_invite = None;
                    let _ = game.session.answer_party(party, true);
                    game.open_party_tab();
                }
            }
            // How an invitation went, in the client's words.
            MapEvent::PartyInviteResult { name, result } => {
                if let Some((id, tone)) = party::invite_result_line(result) {
                    let text = party::invite_result_text(self.messages.get(id).map_or("", String::as_str), &name);
                    game.chat_line(text, if tone == party::Tone::Good { CHAT_ITEM } else { CHAT_ERROR });
                }
            }
            // A map's property: its notices, then the "ready" the server waits for.
            MapEvent::MapProperty { property, flags } => {
                game.map_property = Some((property, flags));
                // Allies are spared on siege maps: their list comes with the guild's first page.
                if versus::siege(flags) && game.guild.is_some() {
                    let _ = game.session.guild_page(0);
                }
                for id in arrival_notices(flags, game.me.job) {
                    game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), MAP_NOTICE);
                }
                let _ = game.session.ready_to_play();
            }
            MapEvent::MapType(kind) => game.map_type = Some(kind),
            // A battleground: teammates' HP bars as party members', their dots, everyone's camp, the score, the team chat.
            MapEvent::BattleHp { account, hp, max } => {
                if account != game.account_id {
                    game.unit_hp.insert(account, (hp, max));
                }
            }
            MapEvent::BattlePosition { account, cell } => game.battle.place(account, cell),
            MapEvent::BattleCamp { account, camp } => {
                game.battle.camps.insert(account, camp);
                if game.camp_looks.is_none()
                    && let Some(gpu) = gpu
                {
                    let solid = |[r, g, b]: [u8; 3]| gpu.solid([r, g, b, 255]);
                    game.camp_looks = Some((solid(battleground::OUR_COLOR), [solid(battleground::OUR_COLOR), solid(battleground::THEIR_COLOR)]));
                }
            }
            MapEvent::BattleScore(score) => game.battle.score = Some(score),
            // The queue: applied (a line by result; 0 has rathena's own), how many wait, left, the battle ready.
            MapEvent::BattleQueue { result, name } => {
                if result == 1 {
                    game.queued = Some((name, None));
                }
                if let Some(id) = battle_queue::apply_result(result) {
                    game.chat_line_kind(
                        self.messages.get(id).cloned().unwrap_or_default(),
                        if result == 1 { CHAT_ITEM } else { CHAT_ERROR },
                        chat::Kind::Battlefield,
                    );
                }
            }
            MapEvent::QueueNumber { name, number } => game.queued = Some((name, Some(number))),
            MapEvent::QueueLeft { result, .. } => {
                if result == 1 {
                    game.queued = None;
                }
                let id = battle_queue::cancel_result(result);
                game.chat_line_kind(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ITEM, chat::Kind::Battlefield);
            }
            MapEvent::QueueReady { name } => {
                game.queue_asked = Some(QueueAsked::Lobby(name, Instant::now() + Duration::from_secs(battle_queue::LOBBY_SECONDS)));
            }
            // ponytail: no sound for it; which one rathena's "gong" is isn't knowable.
            MapEvent::QueueAccepted { result: 1, .. } => {
                game.chat_line_kind(self.messages.get(battle_queue::LOBBY_ACCEPTED).cloned().unwrap_or_default(), CHAT_ITEM, chat::Kind::Battlefield);
            }
            MapEvent::QueueAccepted { .. } => {}
            MapEvent::QueueReset => {
                game.queued = None;
                if matches!(game.queue_asked, Some(QueueAsked::Lobby(..))) {
                    game.queue_asked = None;
                }
            }
            MapEvent::BattleChat { text, .. } => game.chat_line(text, battleground::CHAT_COLOR),
            MapEvent::DressRoom => game.open_dressroom(),
            // The board opens (or shows the new ranking); the names it lacks are asked for once each.
            MapEvent::Ranking { kind, ids, points, mine } => {
                let frame = game.ranking.as_ref().map(|board| board.frame.position);
                let mut board = ranking::Board::new(kind, ids, points, mine);
                if let Some(position) = frame {
                    board.frame.position = position;
                }
                for id in ranking::missing_names(&ids, &game.char_names) {
                    if game.names_asked.insert(id) {
                        let _ = game.session.ask_char_name(id);
                    }
                }
                game.ranking = Some(board);
            }
            MapEvent::CharName { id, name } => {
                game.char_names.insert(id, name);
            }
            // ponytail: the point and mission lines' color is an estimate.
            MapEvent::RankingPoints { kind, added, total } => {
                let id = match kind {
                    0 => RANK_POINTS_BLACKSMITH,
                    1 => RANK_POINTS_ALCHEMIST,
                    2 => RANK_POINTS_TAEKWON,
                    _ => RANK_POINTS_SLAYER,
                };
                let text = rustnarok_client::messages::fill(self.messages.get(id).map_or("", String::as_str), &[&added, &total]);
                game.chat_line(text, CHAT_ITEM);
            }
            MapEvent::StarSkill { name, count, kind: net::STAR_MISSION, .. } => {
                let text = rustnarok_client::messages::fill(self.messages.get(MISSION_TARGET).map_or("", String::as_str), &[&name, &count]);
                game.chat_line(text, CHAT_ITEM);
            }
            // The Star Gladiator's Feeling and Hatred aren't shown yet.
            MapEvent::StarSkill { .. } => {}
            // Sent to the area: only our own counts. Rank 0 or no players hides the counter; a hidden player's (all
            // ones) shows nothing.
            // Only on a map that counts.
            MapEvent::PvpRank { .. } if game.map_property.is_none_or(|(_, flags)| flags & net::MAP_COUNT_PK == 0) => {}
            MapEvent::PvpRank { account, rank, players } => {
                // ponytail: what EF_TOPRANK looks like isn't in this data or rathena's sources ("PvP circle"); the effect
                // plays (nothing until effects.txt has it) and who carries it is kept.
                if (1..=10).contains(&rank) {
                    if game.top_ten.insert(account)
                        && let Some(gpu) = gpu
                    {
                        game.play_effect(gpu, grf, table, TOP_RANK_EFFECT, account);
                    }
                } else {
                    game.top_ten.remove(&account);
                }
                if account == game.account_id {
                    game.pvp_rank = (rank != 0 && rank != u32::MAX && players != 0).then_some((rank, players));
                    if rank != game.last_rank {
                        if let Some(id) = rank_line(game.last_rank, rank) {
                            let text = rustnarok_client::messages::fill(self.messages.get(id).map_or("", String::as_str), &[&game.me.name, &rank]);
                            game.chat_line(text, RANK_LINE);
                        }
                        if let Some(audio) = &mut self.audio {
                            audio.play(grf, "effect\\number_change.wav", 1.0);
                        }
                    }
                    game.first_since = match (rank, game.first_since) {
                        (1, Some(since)) => Some(since),
                        (1, None) => Some(control::now()),
                        _ => None,
                    };
                    game.last_rank = rank;
                }
            }
            // Party chat: a line, and the words over the speaker's head (ours over our own) while they're on screen.
            MapEvent::PartyChat { account, text } => {
                if account == game.account_id {
                    game.say(None, text.clone(), CHAT_WHITE);
                } else if game.units.contains_key(&account) {
                    game.say(Some(account), text.clone(), CHAT_WHITE);
                }
                game.chat_line(text, if account == game.account_id { PARTY_CHAT_OURS } else { PARTY_CHAT });
            }
            MapEvent::PartyPosition { account, cell } => {
                if let Some(member) = game.party.as_mut().and_then(|party| party.member(account)) {
                    member.cell = cell.map(|cell| cell.map(i32::from));
                }
            }
            // ponytail: the line and its color (the party chat's) for a member's pickup are estimates.
            MapEvent::PartyPickup { account, item, identified, refine } => {
                let member = game.party.as_mut().and_then(|party| party.member(account)).map(|member| member.info.name.clone());
                let name = inventory::item_name(&self.items, item, identified, refine);
                let text = rustnarok_client::messages::fill(self.messages.get(PARTY_PICKUP).map_or("", String::as_str), &[&member.unwrap_or_default(), &name]);
                game.chat_line_kind(text, PARTY_CHAT, chat::Kind::PartyItemLog);
            }
            // Every time it comes (each map load too): whether invitations are taken.
            MapEvent::PartyRefusing(refusing) => {
                game.chat_line_kind(
                    self.messages.get(if refusing { PARTY_REFUSING } else { PARTY_ALLOWING }).cloned().unwrap_or_default(),
                    PARTY_NOTICE,
                    chat::Kind::PartyConfig,
                );
                game.refusing_party = Some(refusing);
            }
            // Members' HP: in the window, and as a bar under them.
            MapEvent::PartyHp { account, hp, max } => {
                if let Some(member) = game.party.as_mut().and_then(|party| party.member(account)) {
                    member.hp = Some([hp, max]);
                }
                if account != game.account_id {
                    game.unit_hp.insert(account, (hp, max));
                }
            }
            // EXP 2: an even share was refused (the levels are too far apart); the item settings still change.
            MapEvent::PartyOptions { exp, pickup, share } => {
                if let Some(party) = &mut game.party {
                    if exp != 2 {
                        party.exp_shared = exp == 1;
                    }
                    party.pickup_shared = pickup;
                    party.item_shared = share;
                }
                // Every time it comes (at login and on each change): "Party Setup - <what> : <how>", three lines.
                let message = |id: usize| self.messages.get(id).map_or("", String::as_str).trim();
                for (setup, label, value, note) in party::setup_lines(exp, pickup, share) {
                    let note = note.map(|note| format!(" ({})", message(note))).unwrap_or_default();
                    game.chat_line_kind(
                        format!("{} - {} : {}{note}", message(setup), message(label), message(value)),
                        PARTY_SETUP_LINE,
                        chat::Kind::PartyConfig,
                    );
                }
            }
            MapEvent::PartyLeader { old, new } => {
                for member in game.party.iter_mut().flat_map(|party| party.members.iter_mut()) {
                    if member.info.account == old || member.info.account == new {
                        member.info.leader = member.info.account == new;
                    }
                }
                let leader = game.party.as_mut().and_then(|party| party.member(new)).map(|member| member.info.name.clone()).unwrap_or_default();
                let text = rustnarok_client::messages::fill(self.messages.get(PARTY_NEW_LEADER).map_or("", String::as_str), &[&leader]);
                game.chat_line_kind(text, PARTY_SETUP_LINE, chat::Kind::PartyConfig);
            }
            MapEvent::PartyMemberInfo { account, job, level } => {
                if let Some(member) = game.party.as_mut().and_then(|party| party.member(account)) {
                    member.info.job = job;
                    member.info.level = level;
                }
            }
            MapEvent::PartyDead { account, dead } => {
                if let Some(member) = game.party.as_mut().and_then(|party| party.member(account)) {
                    member.dead = dead;
                }
            }
            // Refused at once under /notrade: no window, no line.
            // Someone asks to join through our booking post: a question for the leader.
            MapEvent::BookingJoinAsked { character, account, name, level, job } => game.booking_asked = Some((character, account, name, level, job)),
            // ponytail: plain lines for the leader's answer; the client's own aren't identified.
            MapEvent::BookingJoinAnswered { leader, party, refused, .. } => {
                let line = if refused { format!("{leader} turned down your request to join {party}.") } else { format!("{leader} let you join {party}.") };
                game.chat_line(line, if refused { CHAT_ERROR } else { CHAT_ITEM });
            }
            MapEvent::TradeAsked { .. } if game.no_trade => {
                let _ = game.session.answer_trade(false);
            }
            MapEvent::TradeAsked { name, id, level } => {
                game.trade_with = name.clone();
                game.trade_asked = Some((name, id, level));
                // Dev switch: take it.
                if std::env::var("RUSTNAROK_CLIENT_TRADE").is_ok_and(|trade| trade == "accept") {
                    game.trade_asked = None;
                    let _ = game.session.answer_trade(true);
                }
            }
            // The deal is on: its window opens (the inventory stays as it is).
            MapEvent::TradeAnswer { result: 3, id, level } => {
                game.trade_asked = None;
                let caption = self.messages.get(TRADE_CAPTION).cloned().unwrap_or_default();
                game.trade = Some(trade::Trade::new(caption, format!("{} Lv{level} ({})", game.trade_with, trade::c_code(id)), screen()));
                game.auto_trade();
            }
            MapEvent::TradeAnswer { result, .. } => {
                if let Some(line) = trade::answer_line(result) {
                    let text = rustnarok_client::messages::fill(self.messages.get(line).map_or("", String::as_str), &[&game.trade_with]);
                    game.chat_line(text, CHAT_ERROR);
                }
            }
            MapEvent::TradeItem(item) => {
                if let Some(trade) = &mut game.trade {
                    trade.items[1].push(item);
                }
            }
            MapEvent::TradeZeny(zeny) => {
                if let Some(trade) = &mut game.trade {
                    trade.their_zeny = zeny;
                }
            }
            // Ours went in: it leaves the inventory until the deal ends (the server gives it back if cancelled).
            MapEvent::TradeAdded { index, result } => {
                if let Some(trade) = &mut game.trade
                    && let Some((_, amount)) = trade.pending.take_if(|(pending, _)| *pending == index)
                    && result == 0
                    && let Some(item) = game.inventory.get(index)
                {
                    trade.items[0].push(net::InventoryItem { amount: amount as u16, ..item.clone() });
                    game.inventory.remove(index, amount as u16);
                }
                let id = match result {
                    1 => Some(TRADE_HEAVY),
                    3 => Some(1938),
                    4 => Some(1939),
                    _ => None,
                };
                if let Some(id) = id {
                    game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ERROR);
                }
                if game.trade.is_some() {
                    game.auto_trade_next();
                }
            }
            MapEvent::TradeLocked { partner } => {
                if let Some(trade) = &mut game.trade {
                    trade.locked[usize::from(partner)] = true;
                    // Dev switch: exchange once both are in.
                    if trade.locked == [true; 2] && std::env::var_os("RUSTNAROK_CLIENT_TRADE_GIVE").is_some() {
                        let _ = game.session.finish_trade();
                    }
                }
            }
            // Cancelled: the deal and any request box close.
            MapEvent::TradeCancelled => {
                game.trade = None;
                game.trade_asked = None;
                game.chat_line(self.messages.get(TRADE_CANCELLED).cloned().unwrap_or_default(), CHAT_ERROR);
            }
            // 0 done (cyan), 1 failed (red); any other result says nothing.
            MapEvent::TradeDone { result } => {
                game.trade = None;
                let line = match result {
                    0 => Some((TRADE_DONE, CHAT_ITEM)),
                    1 => Some((TRADE_FAILED, CHAT_ERROR)),
                    _ => None,
                };
                if let Some((id, color)) = line {
                    game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), color);
                }
            }
            // Whispers, yellow both ways (the client's "(From name)" wording). A 1:1 window opens when that sender's box is on.
            MapEvent::Whisper { from, text } => game.receive_whisper(from, text),
            MapEvent::WhisperResult(result) => {
                let (to, text) = game.whispered.pop_front().unwrap_or_default();
                let line = match result {
                    0 => format!("( To {to} ) : {text}"),
                    1 => self.messages.get(WHISPER_FAILED).cloned().unwrap_or_default(),
                    _ => format!("{to}{}", self.messages.get(WHISPER_FAILED + usize::from(result) - 1).cloned().unwrap_or_default()),
                };
                game.chat_line_kind(line, WHISPER_COLOR, chat::Kind::Whisper);
            }
            MapEvent::ItemSkill(skill) => game.cast_item_skill(skill),
            // Eggs to hatch: the pick list; an empty one shows nothing.
            MapEvent::PetEggs(indexes) => {
                let eggs: Vec<_> = indexes.iter().filter_map(|&index| game.inventory.get(index).cloned()).collect();
                if !eggs.is_empty() {
                    game.pick = Some((pick_list::PickList::new(PET_EGGS, eggs), Picking::Hatch));
                }
            }
            MapEvent::PetState { kind: net::PET_OWNED, id, .. } => game.pet_id = Some(id),
            MapEvent::PetState { kind: net::PET_EGGS_DONE, .. } => {
                if matches!(game.pick, Some((_, Picking::Hatch))) {
                    game.pick = None;
                }
            }
            MapEvent::PetState { kind: net::PET_INTIMACY, value, .. } => {
                let job = game.me.job;
                if game.pet.as_ref().is_some_and(|pet| pet::neglected(job, value.clamp(0, 1000) as u16, pet.hunger)) {
                    game.chat_line(self.messages.get(pet::NEGLECTED).cloned().unwrap_or_default(), pet::NEGLECTED_COLOR);
                }
                if let Some(pet) = &mut game.pet {
                    pet.intimacy = value as u16;
                }
            }
            // The hunger before this update is what a feeding's line is about.
            MapEvent::PetState { kind: net::PET_HUNGER, value, .. } => {
                if let Some(pet) = &mut game.pet {
                    game.pet_hunger_before = pet.hunger;
                    pet.hunger = value as u16;
                }
            }
            // An accessory on or off: the pet moves by another ACT.
            MapEvent::PetState { kind: net::PET_ACCESSORY, id, value } => {
                if let Some(pet) = game.pet.as_mut().filter(|_| game.pet_id == Some(id)) {
                    pet.accessory = value as u16;
                }
                if let Some(actor) = game.units.get_mut(&id)
                    && let Some(name) = job_names.get(&actor.job)
                {
                    actor.sprites = pet_sprites(grf, name, pets.tables.accessory_acts.get(&(value as u16)));
                }
            }
            // ponytail: a performance plays action 4 + its number once (pets' sprites have more actions than plain
            // monsters'); which action the real client plays isn't known. Our pet says something about it.
            MapEvent::PetState { kind: net::PET_PERFORMANCE, id, value } => {
                let action = 4 + value.clamp(0, 4) as usize;
                if let Some(actor) = game.units.get_mut(&id)
                    && let Some(duration) =
                        actor.sprites.as_ref().filter(|sprites| sprites.frames(action * 8) > 0).and_then(|sprites| sprites.action_ms(action, actor.direction))
                {
                    actor.play(action, Instant::now(), duration, false);
                }
                if let Some(hunger) = game.pet.as_ref().filter(|_| game.pet_id == Some(id)).map(|pet| pet.hunger) {
                    let act = if value == 4 { pet::ACT_PERFORMED_SPECIAL } else { pet::ACT_PERFORMED + value.clamp(1, 3) as u8 - 1 };
                    game.pet_talk(act, hunger);
                }
            }
            MapEvent::PetState { .. } => {}
            MapEvent::PetAutoFeed(on) => game.pet_window.auto_feed = on,
            MapEvent::PetInfo(pet) => {
                // Its picture, loaded when it's another monster's.
                if game.pet_window.picture.as_ref().is_none_or(|(monster, _)| *monster != pet.monster) {
                    let path = pets.tables.illustrations.get(&pet.monster).map(|name| format!("{UI}illust\\{name}"));
                    let picture = gpu.zip(path).and_then(|(gpu, path)| gpu.try_load_texture(grf, &path, false).ok());
                    game.pet_window.picture = Some((pet.monster, picture));
                }
                game.pet = Some(pet);
            }
            MapEvent::PetFed { ok: false, food } => {
                let name = inventory::item_name(&self.items, food, true, 0);
                game.chat_line(messages::fill(self.messages.get(PET_NO_FOOD).map_or("", String::as_str), &[&name]), PET_NO_FOOD_COLOR);
            }
            // Fed (its hunger came just before): the food and how many are left, and the pet says so.
            MapEvent::PetFed { ok: true, food } => {
                let name = inventory::item_name(&self.items, food, true, 0);
                let left: u32 = game.inventory.items.iter().filter(|item| item.id == food).map(|item| u32::from(item.amount)).sum();
                game.chat_line(messages::fill(self.messages.get(PET_FED).map_or("", String::as_str), &[&name, &left]), PET_FED_COLOR);
                game.pet_talk(pet::ACT_FEEDING, game.pet_hunger_before);
            }
            MapEvent::PetAct { id, act: net::PetAct::Emotion(kind) } => {
                if let Some(action) = emotion::sprite_action(kind) {
                    game.emotions.insert(Some(id), (action, Instant::now()));
                }
            }
            // ponytail: the line goes to the chat where public talk goes.
            MapEvent::PetAct { id, act: net::PetAct::Talk { monster, act, hunger } } => {
                if let Some(line) = pets.line(monster, act, hunger) {
                    game.say(Some(id), line.clone(), CHAT_WHITE);
                    game.chat_line(line, PET_TALK_COLOR);
                }
            }
            MapEvent::UnitName { id, name } => {
                if let Some(actor) = game.units.get_mut(&id) {
                    actor.name = name;
                }
            }
            // A taming item: aim at the monster to tame.
            MapEvent::CaptureStart => {
                game.targeting = None;
                game.capturing = true;
            }
            // The reel slows and stops on the result; nothing in the chat.
            MapEvent::CaptureResult(caught) => {
                if let Some(machine) = &mut game.slot {
                    machine.answer(caught, Instant::now());
                }
            }
            // Achievements: the whole list, or one changed; a newly done one says so.
            MapEvent::Achievements { totals, list } => {
                game.achievement_totals = totals;
                game.achievements = list.into_iter().map(|achievement| (achievement.id, achievement)).collect();
                game.equipment.titles = earned_titles(&game.achievements, &self.achievement_info, &self.title_names);
            }
            MapEvent::AchievementUpdate { totals, achievement } => {
                game.achievement_totals = totals;
                if let Some(achievement) = achievement {
                    // ponytail: a chat line when one is done; how the real client tells it isn't known.
                    if achievement.completed && !game.achievements.get(&achievement.id).is_some_and(|known| known.completed) {
                        let info = self.achievement_info.get(&achievement.id);
                        let name = info.map_or_else(|| achievement.id.to_string(), |info| info.title.clone());
                        game.chat_line(messages::fill(self.messages.get(achievement::ACHIEVED).map_or("", String::as_str), &[&name]), CHAT_ITEM);
                    }
                    game.achievements.insert(achievement.id, achievement);
                    game.equipment.titles = earned_titles(&game.achievements, &self.achievement_info, &self.title_names);
                }
            }
            // A claim's result shows in the achievement's update.
            MapEvent::AchievementReward { .. } => {}
            MapEvent::TitleChanged { ok: true, title } => game.equipment.title = title,
            MapEvent::TitleChanged { .. } => {}
            // The Lucky Roulette: opened (asked or by @roulette), its prizes, a spin, a claim.
            MapEvent::RouletteOpen { prize, bonus, points } => {
                let window = game.roulette.get_or_insert_with(roulette::Roulette::new);
                (window.prize, window.bonus, window.points) = (prize, bonus, points);
                if window.prizes.is_empty() {
                    let _ = game.session.roulette(net::RouletteRequest::Prizes);
                }
            }
            MapEvent::RoulettePrizes(prizes) => {
                if let Some(window) = &mut game.roulette {
                    window.prizes = prizes;
                }
            }
            // ponytail: the wheel's sound plays as it starts; when the real client plays it isn't known.
            MapEvent::RouletteSpun { result, row, column, bonus, points } => {
                if let Some(window) = &mut game.roulette {
                    (window.bonus, window.points) = (bonus, points);
                    if result == 0 || result == 3 {
                        window.spun(row, column, control::now());
                        if let Some(audio) = &mut self.audio {
                            audio.play(grf, "roulette.wav", 1.0);
                        }
                    }
                }
                if let Some(id) = roulette::spin_message(result) {
                    game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ERROR);
                }
            }
            MapEvent::RouletteClaimed { result, bonus } => {
                if let Some(window) = &mut game.roulette {
                    window.bonus = bonus;
                    if result == 0 {
                        window.prize = None;
                    }
                }
                if let Some(id) = roulette::claim_message(result) {
                    game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ERROR);
                }
            }
            // Sense: the window, named after a unit on screen of that class (the packet has no name).
            // ponytail: where the real client takes the name from isn't knowable.
            MapEvent::MonsterInfo(info) => {
                let name = game.units.values().find(|actor| actor.job == info.class).map(|actor| actor.name.clone()).unwrap_or_default();
                game.monster_window = Some(monster_info::MonsterWindow::new(info, name));
            }
            // The Convex Mirror: none here; the boss found, moving; dead, back in a while.
            // ponytail: the lines in the server lines' yellow.
            MapEvent::BossInfo { kind, cell, hours, minutes, name } => {
                let message = |id: usize| self.messages.get(id).map_or("", String::as_str);
                match kind {
                    0 => game.chat_line(message(NO_BOSS).to_owned(), CHAT_BROADCAST),
                    2 => {
                        game.chat_line(messages::fill(message(BOSS_SHOWN), &[&name]), CHAT_BROADCAST);
                        game.boss = Some((name, cell));
                    }
                    1 => game.boss = Some((name, cell)),
                    _ => {
                        game.boss = None;
                        let line = messages::fill(message(monster_info::boss_line(hours, minutes)), &[&name, &hours, &minutes]);
                        game.chat_line(line, CHAT_BROADCAST);
                    }
                }
            }
            // A Talkie Box went off: its words over it, like speech.
            // ponytail: a box never seen (a hidden trap) puts them in the chat; where the real client shows them isn't known.
            MapEvent::TalkieBox { unit, text } => match game.effects.ground_unit_at(unit) {
                Some(at) => {
                    game.talkie_spots.insert(unit, at);
                    game.say(Some(unit), text, CHAT_WHITE);
                }
                None => game.chat_line(text, CHAT_WHITE),
            },
            MapEvent::IdentifyList(indexes) => {
                let items: Vec<_> = indexes.iter().filter_map(|&index| game.inventory.get(index).cloned()).collect();
                if items.is_empty() {
                    let _ = game.session.identify(None);
                } else {
                    game.pick = Some((pick_list::PickList::new(ITEM_APPRAISAL, items), Picking::Identify));
                }
            }
            // What to make, cook or craft: the pick list with the items by ID.
            MapEvent::MakeList(items) if items.is_empty() => {
                game.question_box = Some(BoxQuestion::Notice(NOTHING_TO_MAKE));
            }
            MapEvent::CookList { items, .. } | MapEvent::ArrowList(items) if items.is_empty() => {
                game.chat_line(self.messages.get(NOTHING_TO_MAKE).cloned().unwrap_or_default(), CHAT_ERROR);
                // An empty making list still waits for an answer.
                let _ = match event_kind {
                    Some(Picking::Cook(kind)) => game.session.cook(kind, 0),
                    _ => Ok(()),
                };
            }
            MapEvent::MakeList(items) => {
                let rows = items.into_iter().map(|id| net::InventoryItem { id, identified: true, ..Default::default() }).collect();
                game.pick = Some((pick_list::PickList::text(MAKING_TITLE, rows), Picking::Make));
            }
            MapEvent::CookList { items, .. } | MapEvent::ArrowList(items) => {
                let picking = event_kind.unwrap_or(Picking::Arrow);
                let rows = items.into_iter().map(|id| net::InventoryItem { id, identified: true, ..Default::default() }).collect();
                let mut list = pick_list::PickList::new(if matches!(picking, Picking::Arrow) { ARROW_TITLE } else { MAKING_TITLE }, rows);
                list.making = !matches!(picking, Picking::Arrow);
                game.pick = Some((list, picking));
            }
            // Blacksmith services: the target's broken items, the caster's weapons; an empty list opens nothing.
            // ponytail: the results in the chat; whether the real client uses a box isn't known.
            // Skills to pick (Hindsight's spells, Shadow Spell's copies): the pick list with skill rows.
            MapEvent::SpellList(skills) => {
                let rows = skills.into_iter().map(|id| net::InventoryItem { id, identified: true, ..Default::default() }).collect();
                let mut list = pick_list::PickList::new(SPELL_TITLE, rows);
                list.skills = true;
                game.pick = Some((list, Picking::Spell));
            }
            MapEvent::SkillSelect { why, skills } => {
                let rows = skills.into_iter().map(|id| net::InventoryItem { id: u32::from(id), identified: true, ..Default::default() }).collect();
                let mut list = pick_list::PickList::new(SPELL_TITLE, rows);
                list.skills = true;
                game.pick = Some((list, Picking::SkillSelect(why)));
            }
            MapEvent::RepairList(items) | MapEvent::UpgradeList(items) if items.is_empty() => {
                game.question_box = Some(BoxQuestion::Notice(NOTHING_TO_MAKE));
            }
            MapEvent::RepairList(items) => game.pick = Some((pick_list::PickList::text(REPAIR_TITLE, items), Picking::Repair)),
            MapEvent::MergeOpen(indexes) => {
                let stacks = indexes.iter().filter_map(|&index| game.inventory.get(index).cloned()).collect();
                game.pick = Some((pick_list::PickList::merging(stacks), Picking::Merge));
            }
            // Merged: the stack that kept everything has its new amount (the others' deletions came first).
            // ponytail: which line the official client prints for each result isn't knowable.
            MapEvent::Merged { index, amount, result } => {
                let message = |id: usize| self.messages.get(id).cloned().unwrap_or_default();
                if result == 0 {
                    if let Some(stack) = game.inventory.items.iter_mut().find(|item| item.index == index) {
                        stack.amount = amount;
                    }
                    game.chat_line(message(MERGED), CHAT_ITEM);
                    if matches!(game.pick, Some((_, Picking::Merge))) {
                        game.pick = None;
                    }
                } else {
                    game.chat_line(message(MERGE_TOO_MANY), CHAT_ERROR);
                }
            }
            MapEvent::UpgradeList(items) => game.pick = Some((pick_list::PickList::text(UPGRADE_TITLE, items), Picking::Upgrade)),
            MapEvent::Repaired { ok, .. } => {
                let (id, color) = if ok { (REPAIRED, CHAT_ITEM) } else { (NOT_REPAIRED, CHAT_ERROR) };
                game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), color);
            }
            MapEvent::Upgraded { result, item } => {
                let (id, color) = UPGRADE_RESULTS.get(result as usize).copied().unwrap_or(UPGRADE_RESULTS[1]);
                let name = inventory::item_name(&self.items, item, true, 0);
                game.chat_line(messages::fill(self.messages.get(id).map_or("", String::as_str), &[&name]), color);
            }
            MapEvent::Made { result, .. } => {
                let Some(effect) = forge::result_effect(result) else { return };
                if (2..=7).contains(&result) {
                    game.forget_effect(game.account_id, effect);
                }
                if let Some(gpu) = gpu {
                    game.play_effect(gpu, grf, table, effect, game.account_id);
                }
            }
            MapEvent::CardList(indexes) => {
                let items: Vec<_> = indexes.iter().filter_map(|&index| game.inventory.get(index).cloned()).collect();
                if let Some(card) = game.card_asked.take()
                    && !items.is_empty()
                {
                    game.pick = Some((pick_list::PickList::new(INSERT_CARD, items), Picking::Card(card)));
                }
            }
            // rathena takes the card without telling; it goes into the equipment's first empty slot.
            // ponytail: the result in the chat; whether the real client shows it in a box isn't known.
            MapEvent::CardInserted { equip, card, ok } => {
                if ok {
                    let id = game.inventory.get(card).map(|item| item.id);
                    game.inventory.remove(card, 1);
                    if let Some(id) = id
                        && let Some(slot) =
                            game.inventory.items.iter_mut().find(|item| item.index == equip).and_then(|item| item.cards.iter_mut().find(|slot| **slot == 0))
                    {
                        *slot = id;
                    }
                }
                let id = if ok { CARD_DONE } else { CARD_FAILED };
                game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), if ok { CHAT_ITEM } else { CHAT_ERROR });
            }
            MapEvent::Identified { index, ok } => {
                if ok && let Some(item) = game.inventory.items.iter_mut().find(|item| item.index == index) {
                    item.identified = true;
                }
            }
            // Replies to the client's own chat commands.
            MapEvent::UserCount(count) => {
                game.chat_line(rustnarok_client::messages::fill(self.messages.get(USER_COUNT).map_or("", String::as_str), &[&count]), CHAT_ITEM);
            }
            MapEvent::WhisperSetting { allow, result } => {
                let name = game.whisper_asked.pop_front().unwrap_or_default();
                if result == 0 {
                    if allow {
                        game.blocked.remove(&name);
                        if let Some(list) = &mut game.block_list {
                            list.remove(&name);
                        }
                    } else {
                        game.blocked.insert(name.clone());
                    }
                }
                let id = if allow { ALLOW_RESULTS } else { BLOCK_RESULTS } + usize::from(result.min(2));
                game.chat_line(format!("{name}{}", self.messages.get(id).cloned().unwrap_or_default()), if result == 0 { CHAT_ITEM } else { CHAT_ERROR });
            }
            MapEvent::WhisperList(names) => {
                game.blocked = names.iter().cloned().collect();
                let empty = self.messages.get(BLOCKED_NONE).cloned().unwrap_or_default();
                game.block_list = Some(whisper::BlockList::new(names, empty));
                game.sync_esc();
            }
            MapEvent::WhisperAll { allow, result } => {
                let id = if allow { ALLOW_ALL_RESULTS } else { BLOCK_ALL_RESULTS } + usize::from(result != 0);
                game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), if result == 0 { CHAT_ITEM } else { CHAT_ERROR });
            }
            // An NPC's picture: from illust (or cardbmp when it isn't there), a window over the ones open before it.
            // Type 255 closes it; the picture already showing stays where it is.
            MapEvent::Cutin { image, place } => {
                let showing = game.cutin.as_ref().map(|cutin| (cutin.name.as_str(), cutin.kind));
                if place == cutin::CLOSE || image.is_empty() {
                    game.cutin = None;
                } else if !cutin::same(showing, &image, place) {
                    let name = cutin::file_name(&image);
                    let picture = gpu.and_then(|gpu| {
                        ["illust", "cardbmp"].iter().find_map(|folder| gpu.try_load_texture(grf, &format!("{UI}{folder}\\{name}"), false).ok())
                    });
                    game.cutin = picture.map(|picture| cutin::Cutin::new(name, place, picture, screen()));
                    game.raise(Panel::Cutin);
                }
            }
            MapEvent::CatalogOpen { remote, uses } => game.catalog = Some(catalog::Catalog::new(remote, uses)),
            MapEvent::CatalogResults { first, more, uses, rows } => {
                if let Some(catalog) = &mut game.catalog {
                    catalog.page(first, more, uses, rows);
                }
            }
            MapEvent::CatalogFailed(reason) => {
                game.chat_line(self.messages.get(catalog::failure_message(reason)).cloned().unwrap_or_default(), CHAT_ERROR);
            }
            // Where the store picked stands: the store mark on the minimap (none: it's gone).
            MapEvent::CatalogMark(cell) => {
                if let Some(catalog) = &mut game.catalog {
                    catalog.mark = cell;
                }
                game.store_mark = cell.map(|cell| cell.map(i32::from));
            }
            // A quest mark: its loop starts again only for a new effect or colour.
            MapEvent::QuestMark { npc, cell, effect, color } => {
                let same = game.quest_marks.iter().any(|&(each, _, was_effect, was_color)| each == npc && was_effect == effect && was_color == color);
                if !same {
                    game.quest_started.insert(npc, Instant::now());
                }
                minimap::set_quest_mark(&mut game.quest_marks, npc, cell.map(i32::from), effect, color);
            }
            // A compass mark, by its id whichever NPC sent it.
            MapEvent::Mark { kind, cell, id, color, .. } => {
                if let Some(gpu) = gpu {
                    let until = (kind == 0).then(|| control::now() + Duration::from_secs(15));
                    let mark = (cell.map(i32::from), gpu.solid([color[0], color[1], color[2], 255]), until, control::now());
                    minimap::set_compass_mark(&mut game.marks, id, kind, mark);
                }
            }
            MapEvent::InventoryExpansion(slots) => game.inventory.expansion = slots,
            MapEvent::Hotkeys { tab, rotate, keys } => game.hotkeys.set_tab(tab, rotate, keys),
            MapEvent::Skills(skills) => game.skills = skills,
            MapEvent::SkillChanged(changed) => match game.skills.iter_mut().find(|skill| skill.id == changed.id) {
                // ZC_SKILLINFO_UPDATE doesn't say the kind.
                Some(skill) => *skill = net::Skill { kind: if changed.kind == u32::MAX { skill.kind } else { changed.kind }, ..changed },
                None => game.skills.push(net::Skill { kind: if changed.kind == u32::MAX { 0 } else { changed.kind }, ..changed }),
            },
            MapEvent::SkillRemoved(id) => game.skills.retain(|skill| skill.id != id),
            MapEvent::Casting { source, target, cell, skill, element, cast_ms } => {
                // A cast with a cast time glows under the caster in its skill's aura: the element's color (EF_BEGINSPELL*),
                // Bash's burst, a fixed effect or none (`Table::cast_aura`).
                if cast_ms > 0
                    && let Some(gpu) = gpu
                    && let Some(effect) = table.cast_aura(skill, CAST_AURAS.get(element as usize).copied().unwrap_or(CAST_AURAS[0]))
                {
                    game.play_cast_aura(gpu, grf, table, effect, source, cast_ms);
                    if effects::is_cast_aura(effect) {
                        game.effects.play_aura(gpu, grf, (source != game.account_id).then_some(source), effect, cast_ms, Instant::now());
                    }
                    // What the cast starts beside its aura (Elemental Action's crescents and flicker).
                    for extra in table.cast_extras(skill) {
                        game.play_effect(gpu, grf, table, extra, source);
                    }
                }
                let me = game.account_id;
                let goal = if target == me { Some(game.player.cell) } else { game.units.get(&target).map(|actor| actor.cell) };
                let caster = if source == me { Some(&mut game.player) } else { game.units.get_mut(&source) };
                if let Some(caster) = caster.filter(|_| skill_motions::cast_moves(cast_ms)) {
                    caster.face(goal.unwrap_or([i32::from(cell[0]), i32::from(cell[1])]));
                }
                if cast_ms > 0 {
                    let (start, length) = (Instant::now(), Duration::from_millis(u64::from(cast_ms)));
                    game.casts.insert((source != me).then_some(source), (start, length));
                    // A cast at a cell: the magic square there. The packet has no level: ours is the skill tree's.
                    // ponytail: our level is the highest learned, not the one picked; others' is level 10.
                    let level = (source == me).then(|| game.skills.iter().find(|known| known.id == skill).map(|known| known.level)).flatten();
                    match (target == 0).then(|| cast_square::Square::new(skill, level, cell, start, length)).flatten() {
                        Some(square) => game.cast_squares.insert((source != me).then_some(source), square),
                        None => game.cast_squares.remove(&(source != me).then_some(source)),
                    };
                }
                game.skill_motion(source, skill, 0, overhead::Shout::Cast, cast_ms as f32, &self.skill_info, look_tables, &self.messages);
                if skill_motions::cast_moves(cast_ms) {
                    skill_effects(game, skill, "beginCastEffectId", source);
                }
            }
            MapEvent::SkillHit { skill, level, act } => {
                game.casts.remove(&(act.source != game.account_id).then_some(act.source));
                let me = game.account_id;
                // Damage from a skill's unit on the ground (Storm Gust, Thunderstorm, Lord of Vermilion, Meteor Storm,
                // Heaven's Drive): no caster to move, its own hit effect, hits from now.
                if act.source != me && !game.units.contains_key(&act.source) && game.skill_units.contains_key(&act.source) {
                    if act.damage != 0 {
                        let gap = Duration::from_millis(skill_motions::UNIT_HIT_GAP_MS);
                        game.battle_lines(&act, &self.messages);
                        game.show_hit(&act, Some(0.0), gap, HitFrom::Unit);
                        if let (Some(gpu), Some(effect)) = (gpu, effects::unit_hit_effect(skill)) {
                            let now = Instant::now();
                            for hit in 0..u32::from(act.hits.max(1)) {
                                game.play_effect_from(gpu, grf, table, effect, act.target, now + gap * hit);
                            }
                        }
                    }
                    return;
                }
                let target_cell = if act.target == me { Some(game.player.cell) } else { game.units.get(&act.target).map(|actor| actor.cell) };
                // Blitz Beat's packet sends the caster's falcon at the target, once whatever the hit count.
                if skill == actor::BLITZ_BEAT {
                    game.dive_falcon(act.source, act.target);
                }
                let caster = if act.source == me { Some(&game.player) } else { game.units.get(&act.source) };
                let bow = caster.is_some_and(|caster| caster.holds_bow(look_tables));
                let scale = actor::attack_scale(act.source_speed, bow);
                let gap = Duration::from_millis(skill_motions::hit_gap_ms(skill));
                // A splash's other targets (Fire Ball's): the caster isn't touched again; one hit, no hit effects,
                // landing with the main target's.
                let splash = matches!(act.kind, 5 | 14) && !effects::SPLASH_FULL.contains(&skill);
                let hit_ms = if splash {
                    game.skill_lands
                        .get(&act.source)
                        .map_or(act.source_speed.max(0) as f32, |&lands| lands.saturating_duration_since(Instant::now()).as_secs_f32() * 1000.0)
                } else {
                    let source = if act.source == me { Some(&mut game.player) } else { game.units.get_mut(&act.source) };
                    if let Some(source) = source {
                        // The caster turns to its target (a player's swing not always: `skill_motions::turns`).
                        let mid_swing = source.playing(source.attack_motion(look_tables), Instant::now());
                        let turns = source.kind != PLAYER || skill_motions::turns(skill_motions::motion(skill), mid_swing);
                        if let Some(cell) = target_cell.filter(|_| act.source != act.target && turns) {
                            source.face(cell);
                        }
                        // Monsters swing as for an attack; players take the skill's motion (`skill_motion`).
                        if source.kind != PLAYER {
                            source.play_at_pace(actor::combat_actions(source.kind).0, Instant::now(), scale, false);
                        }
                    }
                    // Sonic Blow: a swing for each hit after the first, 200 ms apart.
                    if skill == skill_motions::SONIC_BLOW
                        && let Some(source) = if act.source == me { Some(&mut game.player) } else { game.units.get_mut(&act.source) }
                        && source.kind == PLAYER
                    {
                        let swing = source.attack_motion(look_tables);
                        let now = Instant::now();
                        for hit in 1..u32::from(act.hits) {
                            source.swing_later(swing, now + gap * hit, scale);
                        }
                        // ponytail: free to walk through the swings and half a second after the last starts.
                        source.flurry(now + gap * u32::from(act.hits) + Duration::from_millis(500));
                    }
                    let hit_ms = game.skill_motion(
                        act.source,
                        skill,
                        i32::from(level as i16),
                        overhead::Shout::Hit,
                        scale,
                        &self.skill_info,
                        look_tables,
                        &self.messages,
                    );
                    // Hits land on the motion's hit frame (else after the attack motion), each `hit_gap_ms` after the last;
                    // a swing with a bow shoots its arrow then, and its hits land with it.
                    let shoots = bow && skill_motions::motion(skill) == skill_motions::Motion::Attack;
                    if let Some(ms) = hit_ms.filter(|_| shoots) {
                        game.shoot_arrow(grf, act.source, act.target, ms);
                    }
                    // Arrow Shower: arrows at the target and the cells round it, twice for an endured hit.
                    if let Some(cell) = target_cell.filter(|_| skill == ARROW_SHOWER) {
                        for _ in 0..if act.kind == 14 { 2 } else { 1 } {
                            game.arrow_shower(grf, act.source, cell, Some(act.target));
                        }
                    }
                    let hit_ms = hit_ms.map_or(act.source_speed.max(0) as f32, |ms| actor::bow_flight(ms, shoots));
                    game.skill_lands.insert(act.source, Instant::now() + Duration::from_secs_f32(hit_ms / 1000.0));
                    hit_ms
                };
                if act.damage != 0 && !(act.source == act.target && act.kind == 6) {
                    // A splash skill's preamble on its first target (Grimtooth's −30000): the hit effects alone.
                    if !game::preamble(&act) {
                        let hits = net::Act { hits: if splash { 1 } else { act.hits }, ..act };
                        game.battle_lines(&hits, &self.messages);
                        game.show_hit(&hits, Some(hit_ms), gap, if splash { HitFrom::Splash } else { HitFrom::Skill });
                    }
                    // Hit by hit, as each lands: the skill's hit effects on the target; the second only with the first
                    // hit. A skill the table lacks: what comes at the target first (a bolt's arrows), then what shows
                    // as it lands.
                    if let Some(gpu) = gpu.filter(|_| !splash) {
                        let now = Instant::now();
                        let lands = now + Duration::from_secs_f32(hit_ms / 1000.0);
                        for hit in 0..u32::from(act.hits.max(1)) {
                            match table.hit(skill) {
                                Some([first, second]) => {
                                    for effect in first.into_iter().chain(second.filter(|_| hit == 0)) {
                                        game.play_effect_from(gpu, grf, table, effect, act.target, lands + gap * hit);
                                    }
                                }
                                None => {
                                    for effect in table.skill(skill, "beforeHitEffectId") {
                                        game.play_effect_from(gpu, grf, table, effect, act.target, now + gap * hit);
                                    }
                                    for effect in table.skill(skill, "hitEffectId") {
                                        game.play_effect_from(gpu, grf, table, effect, act.target, lands + gap * hit);
                                    }
                                }
                            }
                        }
                    }
                }
                // As the packet comes: the caster's effect (not for a splash's other targets), and the skill's own (on
                // the caster or the target).
                if let Some(gpu) = gpu {
                    if let Some(effect) = table.caster(skill).filter(|_| !splash) {
                        game.play_effect(gpu, grf, table, effect, act.source);
                        // A cast aura blooms on the caster again with the damage (Jupitel Thunder, Earth Spike).
                        if effects::is_cast_aura(effect) {
                            game.effects.bloom_aura(gpu, grf, (act.source != me).then_some(act.source), effect, Instant::now());
                        }
                    }
                    if skill == effects::CANNON_SPEAR {
                        game.cannon_spear(gpu, grf, table, act.source);
                    }
                    // Moonlight shows one, two or three slashes by the packet's hit count.
                    if skill == effects::MOONLIGHT_SKILL {
                        game.play_effect_hits(gpu, grf, table, effects::moonlight_effect(act.hits), act.target, Instant::now(), act.hits);
                    }
                    if let Some(struck) = table.struck(skill) {
                        // Told the hit count, as the game tells them: a bolt drops an arrow a hit.
                        for &(who, effect) in struck {
                            let effect = effects::rampage_effect(skill, effect, game.effects.min);
                            // What flies or runs from the caster to the target (Soul Strike's ghosts, Frost Diver's ice…).
                            if effects::AIMED.contains(&effect) {
                                // Duple Light's sphere always has 3 hits; the other counts follow the game's rule.
                                let hits = if effect == 265 && matches!(skill, 2055 | 2056) { 3 } else { effects::hit_count(effect, act.hits, act.kind) };
                                game.aimed(gpu, grf, table, effect, act.source, act.target, hits);
                                continue;
                            }
                            let unit = if who == effects::Who::Caster { act.source } else { act.target };
                            game.play_effect_hits(gpu, grf, table, effect, unit, Instant::now(), act.hits);
                        }
                    }
                }
                // A skill's own sound and shake with its damage (Crush Strike's, Venom Pressure's), once for the packet's units.
                if !splash {
                    game.effects.play_hit_sounds(table, skill, Instant::now());
                }
                if table.struck(skill).is_none() {
                    skill_effects(game, skill, "effectId", act.target);
                }
            }
            MapEvent::SkillUsed { skill, amount, target, source, ok } => {
                game.casts.remove(&(source != game.account_id).then_some(source));
                let me = game.account_id;
                let target_cell = if target == me { Some(game.player.cell) } else { game.units.get(&target).map(|actor| actor.cell) };
                if source != target
                    && let (Some(cell), Some(caster)) = (target_cell, if source == me { Some(&mut game.player) } else { game.units.get_mut(&source) })
                {
                    caster.face(cell);
                }
                game.skill_motion(source, skill, amount, overhead::Shout::Used, 1.0, &self.skill_info, look_tables, &self.messages);
                // A result of 0: only the motion and the name.
                if !ok {
                    return;
                }
                game.effects.play_skill_sounds(table, skill, Instant::now());
                // A Bard's Frost Joke and a Dancer's Scream say a line from the game's data in the chat.
                if let Some(line) = crate::talk::says(
                    skill,
                    source == game.account_id,
                    ok,
                    net::langtype(),
                    |path| grf.read(path).ok(),
                    |n| effects::pick(0, n.saturating_sub(1)),
                ) {
                    let _ = game.session.chat(&game.me.name, &line);
                }
                // The pitchers throw their item at the target (Potion Pitcher, Berserk Pitcher, Spore Explosion).
                if let (Some(effect), Some(gpu)) = (effects::thrown_item(skill), gpu) {
                    game.aimed(gpu, grf, table, effect, source, target, 1);
                }
                // Heals show their amount, green, and an effect by it; SP Recovery its amount, blue.
                let heal = HEAL_SKILLS.contains(&skill);
                if heal || skill == SP_RECOVERY {
                    game.damages.push(Damage {
                        target: (target != me).then_some(target),
                        amount,
                        start: Instant::now(),
                        kind: DamageKind::Heal { color: if !heal { HEAL_BLUE } else { HEAL_GREEN } },
                        flinch: None,
                        drift: [0.0; 2],
                        source: None,
                    });
                }
                match (heal, table.used(skill), gpu) {
                    (true, _, Some(gpu)) => game.play_effect(gpu, grf, table, heal_effect(amount), target),
                    (false, Some(used), Some(gpu)) => {
                        for &(who, effect) in used {
                            // What lands at the target (Pang Voice's ring, Wink of Charm's heart) is aimed from the caster.
                            if effects::AIMED.contains(&effect) {
                                game.aimed(gpu, grf, table, effect, source, target, 1);
                                continue;
                            }
                            let unit = if who == effects::Who::Caster { source } else { target };
                            // Escape's tint and copies are the unit's own look.
                            if effect == effects::ESCAPE_FX {
                                game.start_escape(unit);
                                continue;
                            }
                            if effect == effects::EF_FROSTDIVER3 {
                                game.jack_frost(gpu, grf, source, target);
                                continue;
                            }
                            game.play_effect(gpu, grf, table, effect, unit);
                            // A cast aura blooms again with the use (Arrullo's blue one, Change Material's green).
                            if effects::is_cast_aura(effect) {
                                game.effects.bloom_aura(gpu, grf, (unit != game.account_id).then_some(unit), effect, Instant::now());
                            }
                        }
                    }
                    (false, None, _) => {
                        skill_effects(game, skill, "effectId", target);
                        skill_effects(game, skill, "effectIdOnCaster", source);
                    }
                    _ => {}
                }
            }
            MapEvent::GroundSkill { skill, source, level, cell } => {
                game.casts.remove(&(source != game.account_id).then_some(source));
                // At the cell or on the caster; a skill the table lacks: its effect at the cell and on the caster.
                match (table.ground(skill), gpu) {
                    (Some(ground), Some(gpu)) => {
                        for &(who, effect) in ground {
                            let effect = effects::storm_gust_effect(skill, effect, game.effects.min);
                            match who {
                                effects::Who::Caster => {
                                    if effect == effects::EF_COLDTHROW {
                                        let aim = game.world.cell_position(cell.map(f32::from));
                                        game.cold_slower(gpu, grf, table, source, aim);
                                    } else {
                                        game.play_effect(gpu, grf, table, effect, source);
                                    }
                                }
                                _ => game.play_effect_at(gpu, grf, table, effect, cell),
                            }
                        }
                    }
                    (None, Some(gpu)) => {
                        for effect in table.skill(skill, "effectId") {
                            game.play_effect_at(gpu, grf, table, effect, cell);
                        }
                        skill_effects(game, skill, "effectIdOnCaster", source);
                    }
                    _ => {}
                }
                // The caster turns to the cell, but for Meteor Storm's meteors.
                let caster = if source == game.account_id { Some(&mut game.player) } else { game.units.get_mut(&source) };
                if let Some(caster) = caster.filter(|_| skill != skill_motions::METEOR_STORM) {
                    caster.face([i32::from(cell[0]), i32::from(cell[1])]);
                }
                game.skill_motion(source, skill, i32::from(level as i16), overhead::Shout::Ground, 1.0, &self.skill_info, look_tables, &self.messages);
                // Arrow Shower: arrows at the cells round the one picked (not a monster's).
                let monster = game.units.get(&source).is_some_and(|actor| actor.kind == MONSTER);
                if skill == ARROW_SHOWER && !monster {
                    game.arrow_shower(grf, source, [i32::from(cell[0]), i32::from(cell[1])], None);
                }
            }
            MapEvent::SkillUnit { id, kind, cell, range } => {
                game.skill_units.insert(id, (kind, cell));
                if let Some(gpu) = gpu {
                    let at = game.world.cell_position(cell.map(f32::from));
                    game.effects.play_unit(gpu, grf, table, kind, id, at, range, Instant::now());
                }
            }
            MapEvent::SkillUnitGone { id } => {
                game.skill_units.remove(&id);
                game.effects.forget(id);
            }
            // Cooldowns: kept across map changes (the server doesn't repeat them), sent again after a relog.
            MapEvent::SkillCooldown { skill, ms } => game.cooldowns.set(skill, ms, control::now()),
            MapEvent::SkillFailed { ok: true, .. } => {}
            // A cut-off cast: its bar and its magic square go.
            // Reputation points: the window fits the rows they show.
            MapEvent::Reputation(entries) => {
                game.reputation.extend(entries);
                if let Some(window) = &mut game.reputation_window {
                    window.refresh(&self.reputation_tables, &game.reputation);
                }
            }
            MapEvent::ReputationOpen { .. } => {
                if game.reputation_window.is_none() {
                    game.toggle_reputation(&self.reputation_tables);
                }
                game.sync_esc();
            }
            // The server starts a navigation: to a cell (kind 0), a map (1) or a monster's map (3).
            MapEvent::Navigate { kind, services, hide_window, map, cell, monster } => {
                let map = navigation::map_key(&map);
                let name = self.navi_tables.maps.get(&map).cloned().unwrap_or_else(|| map.clone());
                let target = navigation::Target { cell: (kind == 0).then_some(cell), monster: (kind == 3).then_some(monster), name, map };
                game.navigation = Some(navigation::Active::new(target, services));
                if !hide_window && game.navi_window.is_none() {
                    game.toggle_navigation();
                    game.sync_esc();
                }
            }
            // A script's progress bar is the player's cast bar; on another unit it's that unit's.
            MapEvent::Progress { seconds } => {
                let (start, length) = (Instant::now(), Duration::from_secs(u64::from(seconds)));
                game.casts.insert(None, (start, length));
                game.progress.start(start, length);
            }
            MapEvent::ProgressActor { unit, seconds } => {
                if game.units.contains_key(&unit) {
                    game.casts.insert(Some(unit), (Instant::now(), Duration::from_secs(u64::from(seconds))));
                }
            }
            MapEvent::ProgressCancel => {
                game.casts.remove(&None);
                game.effects.cut_twilight();
                game.player.stop_holding(SKILL_ACTION);
                game.end_progress(true);
            }
            MapEvent::CastCancelled(source) => {
                let caster = (source != game.account_id).then_some(source);
                game.effects.cut_twilight();
                game.casts.remove(&caster);
                game.cast_squares.remove(&caster);
            }
            // Why the server refused a skill, in red. The cast bar and the cast square stay.
            MapEvent::SkillFailed { skill, detail, item, cause, .. } => {
                let skill_name = self.skill_info.get(&skill).map_or_else(|| skill.to_string(), |info| info.name.clone());
                let item_name = self.items.get(&item).map_or_else(|| item.to_string(), |info| info.name(true).to_owned());
                for (id, fill) in skills::refusal_lines(skill, detail, cause, game.skill_fail) {
                    let Some(template) = self.messages.get(id).filter(|template| !template.is_empty()) else { continue };
                    let line = match fill {
                        skills::Fill::Nothing => template.clone(),
                        skills::Fill::Skill => messages::fill(template, &[&skill_name]),
                        skills::Fill::Item => messages::fill(template, &[&item_name]),
                        skills::Fill::ItemCount(count) => messages::fill(template, &[&item_name, &count]),
                        skills::Fill::Number(number) => messages::fill(template, &[&number]),
                    };
                    game.chat_line(line, CHAT_ERROR);
                }
            }
            // A pushcart comes and goes as a status, its type the status's value.
            MapEvent::StatusIcon { kind: actor::PUSH_CART, id, on, value, .. } => {
                let actor = if id == game.account_id { Some(&mut game.player) } else { game.units.get_mut(&id) };
                if let Some(actor) = actor {
                    let (sprite, act) = actor::cart_path(value, actor.job);
                    let sprites = SpriteSet::with_act(grf, &sprite, &act).filter(|_| on);
                    let leader = actor::world_xz(actor.cell.map(|value| value as f32));
                    actor.cart = sprites.map(|sprites| actor::Cart::behind(sprites, leader, actor.direction));
                }
                if id == game.account_id {
                    game.status_icons.retain(|(icon, ..)| *icon != actor::PUSH_CART);
                    if on && self.status_icon_files.contains_key(&actor::PUSH_CART) {
                        game.status_icons.push((actor::PUSH_CART, Instant::now(), None));
                    }
                }
            }
            MapEvent::StatusIcon { kind, id, on, left_ms, value, .. } => {
                game.status_loop(gpu, grf, table, id, kind, on, i64::from(value));
                // ChangeSize's percent multiplies the size table when the status is on.
                if kind == actor::CHANGE_SIZE || kind == actor::CHANGE_SIZE_MONSTER {
                    let actor = if id == game.account_id { Some(&mut game.player) } else { game.units.get_mut(&id) };
                    if let Some(actor) = actor {
                        // The short status packet carries no percent. 0 would shrink the sprite away.
                        actor.size_percent = (on && value != 0).then_some(value);
                    }
                }
                if kind == UNKNOWN_NAME {
                    if on {
                        game.unknown_names.insert(id);
                    } else {
                        game.unknown_names.remove(&id);
                    }
                }
                // The mirror's 10 minutes are over: the boss leaves the minimap.
                if id == game.account_id && kind == BOSS_ALARM && !on {
                    game.boss = None;
                }
                // A lock's status ending lets go of the unit's pairs too: rathena sends no ZC_BLADESTOP release for
                // Cursed Circle (its caster's count of caught units stays 0).
                if !on && LOCK_ICONS.contains(&kind) {
                    lock_pair(&mut game.locks, id, id, false);
                    game.hold_locked();
                }
                if id == game.account_id && kind == ALL_RIDING {
                    game.riding = on;
                }
                if id == game.account_id {
                    game.status_icons.retain(|(icon, ..)| *icon != kind);
                    // Clairvoyance has no icon in the built-in table; it is still kept so hidden units can be silhouettes.
                    if on && (self.status_icon_files.contains_key(&kind) || kind == actor::CLAIRVOYANCE) {
                        let now = Instant::now();
                        let ends = (left_ms > 0).then(|| now + Duration::from_millis(u64::from(left_ms)));
                        game.status_icons.push((kind, now, ends));
                    }
                }
            }
            // Potions and the like: green HP or blue SP over the player.
            MapEvent::Recovered { kind, amount } => {
                game.damages.push(Damage {
                    target: None,
                    amount,
                    start: Instant::now(),
                    kind: DamageKind::Heal { color: if kind == net::STATUS_SP { HEAL_BLUE } else { HEAL_GREEN } },
                    flinch: None,
                    drift: [0.0; 2],
                    source: None,
                });
            }
            // Mounting, dismounting or an outfit puts a player in another body. Body and health state colour it.
            MapEvent::OptionChange { id, option, body, health } => {
                let me = game.player.cell;
                let actor = if id == game.account_id { Some(&mut game.player) } else { game.units.get_mut(&id) };
                if let Some(actor) = actor {
                    actor.cloaked = option & actor::UNSEEN_OPTIONS != 0;
                    actor.hide_option = option;
                    let changed = state_sounds((actor.body_state, actor.health_state), (body, health));
                    if let Some(audio) = &mut self.audio {
                        let loudness = heard(f32::hypot((actor.cell[0] - me[0]) as f32, (actor.cell[1] - me[1]) as f32));
                        for sound in changed {
                            audio.play(grf, sound, loudness);
                        }
                    }
                    actor.body_state = body;
                    actor.health_state = health;
                }
                if id == game.account_id {
                    game.note_blind(control::now());
                }
                game.hold_locked();
                let actor = if id == game.account_id { Some(&mut game.player) } else { game.units.get_mut(&id) };
                if let Some(actor) = actor
                    && let Some(look) = &mut actor.look
                    && look.option != option & actor::BODY_OPTIONS
                {
                    look.option = option & actor::BODY_OPTIONS;
                    actor.sprites = SpriteSet::player(grf, look, look_tables);
                }
                let actor = if id == game.account_id { Some(&mut game.player) } else { game.units.get_mut(&id) };
                if let Some(actor) = actor {
                    actor.sync_companions(option, grf);
                }
            }
            // A unit takes another job's sprite at once (setnpcdisplay, a homunculus evolving).
            MapEvent::JobSprite { id, job } => {
                if let Some(actor) = game.units.get_mut(&id) {
                    change_job(actor, job as u16, grf, job_names, look_tables);
                }
            }
            MapEvent::LookChange { id, kind, value, extra } => {
                // A skill's ground unit becoming another (rathena's Warp Portal opens this way): the new one's effect. A trap
                // going off (becoming 0x8c) first shows its trigger at its cell.
                if kind == net::LOOK_JOB
                    && let Some(at) = game.effects.ground_unit_at(id)
                    && let Some(gpu) = gpu
                {
                    let trigger = game.effects.unit_kind(id).filter(|_| value == FIRED_TRAP).and_then(|old| table.trap(old));
                    game.effects.play_unit(gpu, grf, table, value, id, at, 0, Instant::now());
                    if let Some(effect) = trigger {
                        // Reverberation's tornado goes with the trap; the others stay where it was.
                        let target = if effect == effects::REVERBERATION { effects::Target::Kept(id) } else { effects::Target::Ground };
                        game.effects.play(gpu, grf, table, effect, target, at, Instant::now());
                    }
                }
                // A job change comes as the player's new look.
                if id == game.account_id && kind == net::LOOK_JOB {
                    game.me.job = value as u16;
                }
                let actor = if id == game.account_id { Some(&mut game.player) } else { game.units.get_mut(&id) };
                // A job change on any unit (rathena's `setunitdata` too); parts on anything drawn as a player. At once.
                if let Some(actor) = actor {
                    if kind == net::LOOK_JOB {
                        change_job(actor, value as u16, grf, job_names, look_tables);
                    } else if let Some(look) = &mut actor.look
                        && change_look(look, kind, value, extra)
                    {
                        actor.sprites = SpriteSet::player(grf, look, look_tables);
                    }
                }
                // The preview follows the real look once the server has applied it. Picks stay until the window closes.
                if id == game.account_id {
                    let look = game.player.look.clone().unwrap_or_default();
                    if let Some(window) = &mut game.stylist {
                        dress_stylist(window, grf, look_tables, &look, &self.items, &self.styling_shop);
                    }
                }
            }
            MapEvent::Equipped { index, location, result } => {
                let name = game.inventory.get(index).map(|item| inventory::item_name(&self.items, item.id, item.identified, item.refine)).unwrap_or_default();
                let message = |id: usize| self.messages.get(id).cloned().unwrap_or_default();
                match result {
                    0 => {
                        game.inventory.set_worn(index, location);
                        game.chat_line(format!("{name}{}", message(170)), CHAT_ITEM);
                    }
                    1 => game.chat_line(message(1773), CHAT_ERROR),
                    _ => game.chat_line(message(372), CHAT_ERROR),
                }
            }
            // A rental ended: its warning, its time up and gone from the inventory (the take-off and delete that may follow
            // find nothing then).
            // ponytail: rathena's comment says minutes; under a minute it's said in seconds.
            MapEvent::RentalTime { item, seconds } => {
                let name = inventory::item_name(&self.items, item, true, 0);
                let (id, count) = if seconds < 60 { (RENTAL_SECONDS, seconds) } else { (RENTAL_MINUTES, seconds / 60) };
                game.chat_line(messages::fill(self.messages.get(id).map_or("", String::as_str), &[&name, &count]).trim_start().to_owned(), CHAT_ITEM);
            }
            MapEvent::RentalExpired { index, item } => {
                let name = inventory::item_name(&self.items, item, true, 0);
                game.chat_line(messages::fill(self.messages.get(RENTAL_ENDED).map_or("", String::as_str), &[&name]).trim_start().to_owned(), CHAT_ITEM);
                game.inventory.remove(index, u16::MAX);
            }
            // An item no longer carried (a rental that ended) comes off without a word.
            MapEvent::Unequipped { index, failed: false, .. } if game.inventory.get(index).is_none() => {}
            MapEvent::Unequipped { index, failed: false, .. } => {
                let name = game.inventory.get(index).map(|item| inventory::item_name(&self.items, item.id, item.identified, item.refine)).unwrap_or_default();
                game.inventory.set_worn(index, 0);
                game.chat_line(format!("{name} {}", self.messages.get(171).cloned().unwrap_or_default()), CHAT_ERROR);
            }
            MapEvent::Unequipped { .. } => {}
            MapEvent::ItemRemoved { index, amount } => game.inventory.remove(index, amount),
            MapEvent::ItemUsed { index, item, user, remaining, ok } => {
                if ok && user == game.account_id {
                    game.inventory.set_amount(index, remaining);
                }
                if ok && let (Some(gpu), Some(effect)) = (gpu, table.item(item)) {
                    game.play_effect(gpu, grf, table, effect, user);
                }
            }
            // Weather comes as effects on the player: it shows until the map changes.
            // A sky effect on the player (the clouds, clouds2 and fog weather among them) builds its sky type's puffs.
            MapEvent::Effect { id, effect, .. } if id == game.account_id && sky::sky_type_of(effect).is_some() => {
                if weather::is_weather(effect) {
                    let at = game.world.cell_position(game.player.cell.map(|cell| cell as f32));
                    game.weather.start(effect, control::now(), at);
                }
                let player = game.world.cell_position(game.player.position(control::now()));
                let map = game.map.clone();
                if let Some(sky_type) = sky::sky_type_of(effect) {
                    game.sky.add(sky_type, &map, player, &sky::ground_of(&game.world.gat));
                }
            }
            MapEvent::Effect { id, effect, .. } if id == game.account_id && weather::is_weather(effect) => {
                let at = game.world.cell_position(game.player.cell.map(|cell| cell as f32));
                game.weather.start(effect, control::now(), at);
            }
            MapEvent::Effect { id, effect, num } => {
                let is_player = id == game.account_id;
                if !is_player && !game.units.contains_key(&id) {
                    return;
                }
                if let Some((target, amount, color)) = effect_number(id, effect, num, game.account_id, &|id| game.units.contains_key(&id)) {
                    game.damages.push(Damage {
                        target,
                        amount,
                        start: Instant::now(),
                        kind: DamageKind::Heal { color },
                        flinch: None,
                        drift: [0.0; 2],
                        source: None,
                    });
                }
                if !matches!(effect, 657..=664)
                    && let Some(gpu) = gpu
                {
                    if effects::aimed_at_centre(effect) {
                        game.aimed_at_centre(gpu, grf, table, effect, id);
                    } else if effect == 451 {
                        // Dark Strike's balls come only with the packet's own damage.
                    } else {
                        // The server's own effect has no hit count (Blitz Beat's makes no ring).
                        game.play_effect_hits(gpu, grf, table, effect, id, Instant::now(), 0);
                    }
                }
            }
            MapEvent::HatEffects { id, on, effects } => {
                let own = id == game.account_id;
                if let Some(gpu) = gpu
                    && (own || game.units.contains_key(&id))
                {
                    game.effects.hat_effects(gpu, grf, (!own).then_some(id), on, &effects);
                }
            }
            // Marriage: the wedding, the partner's name, divorce; adoption: being asked, and refusals.
            MapEvent::Congratulation(id) => {
                if let Some(gpu) = gpu {
                    game.play_effect(gpu, grf, table, WEDDING_EFFECT, id);
                }
            }
            // ponytail: the name comes to everyone near whoever called, without who; taken as ours while we have the
            // ring's skill, so another couple calling nearby overwrites it.
            MapEvent::PartnerName(name) => {
                if !name.is_empty() && game.skills.iter().any(|skill| skill.id == CALL_PARTNER) {
                    game.partner = Some(name);
                }
            }
            // ponytail: the line names the player, then the partner; the order is an estimate.
            MapEvent::Divorced(partner) => {
                let line = rustnarok_client::messages::fill(self.messages.get(DIVORCED).map_or("", String::as_str), &[&game.me.name, &partner]);
                game.chat_line(line, CHAT_WHITE);
                game.partner = None;
            }
            MapEvent::AdoptAsked { parent, partner, name } => game.adopt_asked = Some((parent, partner, name, false)),
            MapEvent::AdoptRefused(reason) => {
                if let Some(&id) = ADOPT_REFUSALS.get(reason as usize) {
                    game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), CHAT_ERROR);
                }
            }
            MapEvent::DealChoice { npc } => {
                game.dismiss_dialog();
                game.deal_choice = Some(npc);
                game.deal_frame.open = true;
                // Dev switch: answer at once.
                if let Ok(deal) = std::env::var("RUSTNAROK_CLIENT_DEAL") {
                    game.close_deal_choice();
                    let _ = game.session.choose_deal(npc, deal.starts_with("sell"));
                }
            }
            MapEvent::ShopGoods(goods) => {
                let offers = goods
                    .into_iter()
                    .map(|goods| shop::Offer {
                        item: goods.item,
                        price: goods.discounted,
                        listed: goods.price,
                        kind: goods.kind,
                        identified: true,
                        ..Default::default()
                    })
                    .collect();
                game.open_shop(shop::Shop::new(false, offers));
                game.auto_deal();
            }
            MapEvent::ShopWants(wants) => {
                let offers = wants
                    .into_iter()
                    .filter_map(|(index, listed, price)| {
                        let item = game.inventory.get(index)?;
                        Some(shop::Offer {
                            item: item.id,
                            price,
                            listed,
                            index,
                            have: item.amount,
                            kind: item.kind,
                            identified: item.identified,
                            refine: item.refine,
                            ..Default::default()
                        })
                    })
                    .collect();
                game.open_shop(shop::Shop::new(true, offers));
                game.auto_deal();
            }
            // Markets and barters: the plain shop's windows with stock and costs; they stay open after a purchase.
            MapEvent::Market { kind, goods } => {
                let offers = goods
                    .into_iter()
                    .map(|goods| {
                        // The inventory index of what it costs: the field's name says so; rathena doesn't read it.
                        let cost_index = goods
                            .costs
                            .first()
                            .and_then(|&(item, _)| game.inventory.items.iter().find(|carried| carried.id == item))
                            .map_or(0, |carried| carried.index);
                        shop::Offer {
                            item: goods.item,
                            price: goods.zeny,
                            kind: goods.kind,
                            identified: true,
                            stock: goods.stock,
                            costs: goods.costs,
                            shop_index: goods.index,
                            cost_index,
                            ..Default::default()
                        }
                    })
                    .collect();
                let mut store = shop::Shop::new(false, offers);
                store.market = Some(kind);
                game.open_shop(store);
            }
            MapEvent::MarketBought { ok, items } => {
                if ok && let Some(shop) = &mut game.shop {
                    shop.bought(&items.iter().map(|&(item, amount)| (item, u32::from(amount))).collect::<Vec<_>>());
                }
                // ponytail: rathena sends 0 or 0xFFFF only; the lines are the plain shop's.
                let (id, color) = if ok { (54, CHAT_ITEM) } else { (57, CHAT_ERROR) };
                game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), color);
            }
            // A barter's answer: its own lines (by their words), the window stays with what's left.
            MapEvent::Bought(result)
                if game.shop.as_ref().is_some_and(|shop| matches!(shop.market, Some(net::MarketKind::Barter | net::MarketKind::Expanded))) =>
            {
                let (id, color) = match result {
                    0 => (3555, CHAT_ITEM),
                    1 => (55, CHAT_ERROR),
                    2 => (56, CHAT_ERROR),
                    11 => (3554, CHAT_ERROR),
                    13 => (3557, CHAT_ERROR),
                    14 => (3556, CHAT_ERROR),
                    _ => (57, CHAT_ERROR),
                };
                if result == 0
                    && let Some(shop) = &mut game.shop
                {
                    let deal: Vec<_> = shop.deal.iter().map(|&(offer, amount)| (shop.offers[offer].item, amount)).collect();
                    shop.bought(&deal);
                }
                game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), color);
            }
            MapEvent::Bought(result) | MapEvent::Sold(result) => {
                // The client's words for how it went (some results say nothing).
                let sold = matches!(event, MapEvent::Sold(_));
                if let Some((id, color)) = deal_result(sold, result) {
                    game.chat_line(self.messages.get(id).cloned().unwrap_or_default(), color);
                }
                game.shop = None;
            }
            MapEvent::MiscEffect { id, kind } => {
                if let (Some(gpu), Some(effect)) = (gpu, table.misc(kind)) {
                    game.play_effect(gpu, grf, table, effect, id);
                }
            }
            // Out of reach after all: chase it again, as asked (kept up unless it was only while held).
            // ponytail: the chase starts from the client's positions, not the server's in the packet.
            MapEvent::TooFar { target, target_cell, cell, range } => {
                game.attack_range = i32::from(range);
                // Both where the server has them: one standing goes to its cell's centre, one walking walks on.
                // ponytail: a walking target keeps its own destination (a fast chase's aim) until its next move.
                if let Some(unit) = game.units.get_mut(&target).filter(|unit| !unit.moving()) {
                    unit.place_at(target_cell.map(i32::from));
                }
                if !game.player.moving() {
                    game.player.place_at(cell.map(i32::from));
                }
                game.walk_cell = None;
                // Chased again at once (`rearmed`); one on Ice Wall or a trap ends instead.
                if too_far_ends(game.attack, game.attack_was, target) {
                    game.attack = None;
                } else if !game.player.sitting && game.player.option & 0x2 == 0 {
                    let now = Instant::now();
                    game.attack = Some(rearmed(game.attack, game.attack_was, target));
                    game.last_chase_walk = Some(whole_ms(now).saturating_sub(ATTACK_GAP_MS));
                    game.attack_step(now);
                }
            }
            MapEvent::StyleChanged(false) => {
                game.stylist = None;
                game.chat_line(self.messages.get(stylist::DONE).cloned().unwrap_or_default(), CHAT_ITEM);
            }
            // ponytail: no client line fits a failed style change, so the result is printed plainly. The window stays.
            MapEvent::StyleChanged(true) => game.chat_line(stylist::FAILED_LINE.to_owned(), CHAT_ERROR),
            // The guild storage's log: pages gather until the last, which opens the window with all of them; no log
            // or a refusal says so in red and drops what was heard.
            MapEvent::GuildStorageLog { result, entries } => match result {
                0 => game.storage_log_heard.extend(entries),
                1 => {
                    game.storage_log_heard.extend(entries);
                    game.storage_log = Some(storage_log::Log::new(std::mem::take(&mut game.storage_log_heard), screen()));
                }
                _ => {
                    game.storage_log_heard.clear();
                    let line = if result == 2 { storage_log::NO_LOG } else { storage_log::LOG_FAILED };
                    game.chat_line(self.messages.get(line).cloned().unwrap_or_default(), CHAT_ERROR);
                }
            },
            MapEvent::InstanceCreate { name, place } => {
                game.memorial.instance = Some(memorial::Instance::waiting(name, place));
                game.show_memorial();
            }
            MapEvent::InstanceWait { place } => {
                if let Some(instance) = &mut game.memorial.instance {
                    instance.place = Some(place);
                }
            }
            MapEvent::InstanceStatus { name, time_limit, idle_limit } => {
                game.memorial.instance = Some(memorial::Instance::made(name, time_limit, idle_limit));
                game.show_memorial();
            }
            // 0 keeps the window and stores the new idle-limit Unix time. 1 through 4 close it.
            MapEvent::InstanceNotify { kind, time } => {
                if kind == memorial::IN_NOTIFY {
                    if let Some(instance) = &mut game.memorial.instance {
                        instance.idle_limit = Some(time);
                    }
                } else if memorial::closes(kind) {
                    let name = game.memorial.instance.as_ref().map(|instance| instance.name.clone()).unwrap_or_default();
                    if let Some(id) = memorial::notify_message(kind) {
                        let template = self.messages.get(id).cloned().unwrap_or_default();
                        let line = if kind == memorial::IN_CREATE_FAIL { messages::fill(&template, &[&name]) } else { template };
                        let color = if kind == memorial::IN_CREATE_FAIL { CHAT_ERROR } else { CHAT_WHITE };
                        game.chat_line(line, color);
                    }
                    game.end_memorial();
                }
            }
            MapEvent::Entered { .. } | MapEvent::ServerTime(..) => unreachable!("handled above"),
            MapEvent::Disconnected(_) => self.message(self.messages.get(DISCONNECTED_MESSAGE).cloned().unwrap_or_default(), Scene::Login),
        }
    }
}
