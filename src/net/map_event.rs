//! What the map server tells the client: a `MapEvent` per packet, decoded in `decode.rs`.

use super::*;

#[derive(Debug)]
pub enum MapEvent {
    Entered {
        position: [u16; 2],
        direction: u8,
        /// The server's tick, and when it came.
        clock: (u32, Instant),
    },
    /// The server's tick, answering ours, and when it came.
    ServerTime(u32, Instant),
    /// A unit is standing in view (appeared or was already there).
    Unit(Unit),
    /// A unit walking in view; its `position` is where the walk starts.
    UnitWalk {
        unit: Unit,
        to: [u16; 2],
        start_time: u32,
        /// Where in its start cell it is, in sixteenths (8: the centre).
        sub: [u8; 2],
    },
    Vanish {
        id: u32,
        reason: u8,
    },
    PlayerMove {
        from: [u16; 2],
        to: [u16; 2],
        start_time: u32,
        /// Where in its start cell the walk starts, in sixteenths (8: the centre).
        sub: [u8; 2],
    },
    Stop {
        id: u32,
        position: [u16; 2],
    },
    /// A unit is knocked back to a cell (ZC_HIGHJUMP, rathena's `clif_blown`).
    Knocked {
        id: u32,
        position: [u16; 2],
    },
    /// A unit is at a cell now, moved there at once (Taekwon Jump, Snap, a pet put by its owner).
    Slide {
        id: u32,
        position: [u16; 2],
    },
    /// A unit may use its next combo skill for this long.
    ComboWindow {
        id: u32,
        ms: u32,
    },
    /// Blade Stop or Cursed Circle locks two units together, or lets go.
    BladeStop {
        source: u32,
        target: u32,
        on: bool,
    },
    /// A unit turned its body (a sprite direction) or (players) its head.
    Turn {
        id: u32,
        direction: u8,
        head_direction: u16,
    },
    /// Chat from another unit, or our own (id None); ZC_NPC_CHAT's comes in a color.
    Chat {
        id: Option<u32>,
        message: String,
        color: Option<[u8; 3]>,
    },
    MapChange {
        map: String,
        position: [u16; 2],
    },
    /// NPC dialog text (one line; the box keeps appending until cleared).
    NpcSay {
        npc: u32,
        text: String,
    },
    /// Show the dialog's "next" button.
    NpcNext {
        npc: u32,
    },
    /// The say window's size in pixels (ZC_DIALOG_WINDOW_SIZE). A 0 axis is the default size.
    NpcWindowSize {
        width: i32,
        height: i32,
    },
    /// Where the say window sits: pixels, or a percent of the free screen (ZC_DIALOG_WINDOW_POS2).
    NpcWindowPos {
        x: i32,
        y: i32,
        percent: bool,
    },
    /// A special popup by id (ZC_SPECIALPOPUP). The picture and lines are System/spopup.lub's.
    SpecialPopup(i32),
    /// Show the dialog's "close" button.
    NpcClose {
        npc: u32,
    },
    /// Close every dialog window of this NPC.
    NpcClear {
        npc: u32,
    },
    /// A script's progress bar on the player, this many seconds (its colour isn't used).
    Progress {
        seconds: u32,
    },
    /// The same bar on another unit.
    ProgressActor {
        unit: u32,
        seconds: u32,
    },
    /// The player's progress bar is taken away.
    ProgressCancel,
    NpcMenu {
        npc: u32,
        items: Vec<String>,
    },
    /// Ask for a number, or text when `text` is set.
    NpcInput {
        npc: u32,
        text: bool,
    },
    /// An attack (or other action) between units (ZC_NOTIFY_ACT).
    Act(Act),
    /// One of the player's own status values changed (HP, SP, ...).
    Status {
        id: u16,
        value: i64,
    },
    /// A monster's HP, sent while it's being fought.
    UnitHp {
        id: u32,
        hp: i32,
        max_hp: i32,
    },
    /// The player's attack reach in cells.
    AttackRange(u16),
    /// An attack request was out of reach: walk closer first.
    TooFar {
        target: u32,
        target_cell: [u16; 2],
        cell: [u16; 2],
        range: u16,
    },
    GroundItem(GroundItem),
    /// An item on the ground was picked up or went away.
    ItemGone {
        id: u32,
    },
    /// The result of picking something up: the inventory entry it went into, unless `result` is nonzero.
    PickedUp {
        result: u8,
        item: InventoryItem,
    },
    /// The server starts listing an inventory (`kind` 0 is the player's own), with its name (the storage's title).
    InventoryStart {
        kind: u8,
        name: String,
    },
    InventoryItems {
        kind: u8,
        items: Vec<InventoryItem>,
    },
    /// Inventory slots beyond the base 100.
    InventoryExpansion(u16),
    /// Fewer of an inventory item: dropped, used up or taken.
    ItemRemoved {
        index: u16,
        amount: u16,
    },
    /// Someone used an item; for the player, how many are left.
    ItemUsed {
        index: u16,
        item: u32,
        user: u32,
        remaining: u16,
        ok: bool,
    },
    /// A unit shows an effect by its ID (ZC_NOTIFY_EFFECT2/3).
    Effect {
        id: u32,
        effect: u32,
    },
    /// A level up, refining result and the like on a unit (ZC_NOTIFY_EFFECT): which kind.
    MiscEffect {
        id: u32,
        kind: u32,
    },
    /// An NPC shop asks whether to buy or sell (ZC_SELECT_DEALTYPE).
    DealChoice {
        npc: u32,
    },
    /// What the shop sells (ZC_PC_PURCHASE_ITEMLIST).
    ShopGoods(Vec<ShopGoods>),
    /// What the shop would buy from the inventory: index, price and the price it pays (ZC_PC_SELL_ITEMLIST).
    ShopWants(Vec<(u16, u32, u32)>),
    /// How buying or selling went: 0 done, else why not (ZC_PC_PURCHASE_RESULT, ZC_PC_SELL_RESULT).
    Bought(u8),
    Sold(u8),
    /// How full the storage is (ZC_NOTIFY_STOREITEM_COUNTINFO).
    StorageCount {
        amount: u16,
        max: u16,
    },
    /// An item went into storage (ZC_ADD_ITEM_TO_STORE); a stack already there grows.
    StorageAdded(InventoryItem),
    /// Fewer of a stored item (ZC_DELETE_ITEM_FROM_STORE).
    StorageRemoved {
        index: u16,
        amount: u32,
    },
    /// A move between the inventory, the storage or the cart that the server turned down.
    MoveFailed {
        index: u16,
        amount: u16,
    },
    StorageClosed,
    /// A unit shows an emotion (the client's ET_* number).
    Emotion {
        id: u32,
        kind: u8,
    },
    /// Several status values at once (rathena's SP_* IDs, bonuses under STATUS_BONUS).
    Statuses(Vec<(u16, i64)>),
    /// Going back to character select: 1 yes, 0 not yet (wait 10 seconds).
    RestartAck(u8),
    /// Leaving the game: 0 go, 1 not yet.
    QuitAck(u16),
    /// A unit came back to life.
    Revived {
        id: u32,
    },
    /// A chat room over its owner's head, new or changed (ZC_ROOM_NEWENTRY, ZC_CHANGE_CHATROOM): its ID, size, how
    /// many are in, kind (0 password, 1 open, 2 an NPC's waiting room) and title.
    ChatRoom {
        owner: u32,
        id: u32,
        limit: u16,
        users: u16,
        kind: u8,
        title: String,
    },
    ChatRoomGone {
        id: u32,
    },
    /// Making a room: 0 made, 1 too many rooms, 2 the same room exists.
    ChatRoomMade(u8),
    /// Why a room can't be entered (0 full, 1 wrong password, 2 kicked out before, 4 not enough zeny, 5-7 level or job).
    ChatRoomRefused(u8),
    /// In a room: its ID, and who's there (and whether they own it).
    ChatRoomEntered {
        id: u32,
        members: Vec<(String, bool)>,
    },
    ChatRoomJoined {
        users: u16,
        name: String,
    },
    ChatRoomLeft {
        users: u16,
        name: String,
        kicked: bool,
    },
    /// Someone becomes (or stops being) the room's owner.
    ChatRoomOwner {
        owner: bool,
        name: String,
    },
    /// The vending skill went off: set up a shop of up to this many items (ZC_OPENSTORE).
    VendSetup(u16),
    /// Our shop opened (0) or didn't, and what it sells.
    VendOpened(u8),
    MyShop(Vec<VendItem>),
    /// Someone bought from our shop: which item, how many, for how much in all.
    VendSold {
        index: u16,
        amount: u16,
        zeny: i32,
    },
    /// A vendor's sign over their head, and its going away.
    VendorSign {
        id: u32,
        title: String,
    },
    VendorGone {
        id: u32,
    },
    /// What a vendor sells (their ID, the shop's ID to buy with, the goods).
    VendorGoods {
        vendor: u32,
        shop: u32,
        goods: Vec<VendItem>,
    },
    /// How buying an item from a vendor went: 0 bought, 1 not enough zeny, 2 too heavy, 4 out of stock, ...
    VendBought {
        index: u16,
        amount: u16,
        result: u8,
    },
    /// A buying store license or skill went off: set up a buying store of up to this many items (ZC_OPEN_BUYING_STORE).
    OpenBuyingStore {
        slots: u8,
    },
    /// Opening our buying store failed (ZC_FAILED_OPEN_BUYING_STORE_TO_BUYER).
    BuyingStoreOpenFailed {
        result: u16,
        weight: u32,
    },
    /// Our buying store opened: our account, the zeny limit, and the wanted items (ZC_MYITEMLIST_BUYING_STORE).
    MyBuyingStore {
        account: u32,
        limit: u32,
        items: Vec<BuyingItem>,
    },
    /// A buying store's sign over its owner's head, and its going away (ZC_BUYING_STORE_ENTRY, ZC_DISAPPEAR_BUYING_STORE_ENTRY).
    BuyingStoreSign {
        owner: u32,
        title: String,
    },
    BuyingStoreSignGone {
        owner: u32,
    },
    /// What another player's buying store wants to buy (ZC_ACK_ITEMLIST_BUYING_STORE).
    BuyingStoreItems {
        owner: u32,
        store_id: u32,
        limit: u32,
        items: Vec<BuyingItem>,
    },
    /// The server closed our buying store after a trade or limit check (ZC_FAILED_TRADE_BUYING_STORE_TO_BUYER).
    BuyingStoreBuyerFailed {
        result: u16,
    },
    /// An item we sold into a buying store left our inventory (ZC_ITEM_DELETE_BUYING_STORE).
    BuyingStoreSold {
        index: u16,
        amount: u16,
        price: u32,
    },
    /// Selling into a buying store failed (ZC_FAILED_TRADE_BUYING_STORE_TO_SELLER).
    BuyingStoreSellerFailed {
        result: u16,
        item_id: u32,
    },
    /// Someone sold an item into our buying store (ZC_UPDATE_ITEM_FROM_BUYING_STORE2).
    BuyingStoreUpdate {
        item_id: u32,
        amount: u16,
        zeny: u32,
        limit_left: u32,
        char_id: u32,
        time: u32,
    },
    /// An announcement: plain ones yellow (or blue when the server asks), ZC_BROADCAST2 in its own color.
    /// The map's property (rathena's `map_property`) and its flags (MAP_*), at every load and flag change.
    MapProperty {
        property: u16,
        flags: u32,
    },
    /// A script opens the dress room (`opendressroom`).
    DressRoom,
    /// A Universal Catalog opens the store search: `remote` opens a store found (effect 1), else it shows where it
    /// is; the searches left.
    CatalogOpen {
        remote: bool,
        uses: u8,
    },
    /// A page of results: the first replaces the list, later ones add to it; `more`: a next page exists.
    CatalogResults {
        first: bool,
        more: bool,
        uses: u8,
        rows: Vec<CatalogRow>,
    },
    /// A search failed: 0 nothing found, 1 too many, 2 no searches left, 3 too soon, 4 the store's gone.
    CatalogFailed(u8),
    /// Where a store found stands on this map; none when it's on another.
    CatalogMark(Option<[i16; 2]>),
    /// A ranking's top ten (0 Blacksmith, 1 Alchemist, 2 Taekwon, 3 Killer): character IDs (0: an empty place) and
    /// points, and the player's own points.
    Ranking {
        kind: u16,
        ids: [u32; 10],
        points: [u32; 10],
        mine: u32,
    },
    /// A character's name by ID (none when the server has none).
    CharName {
        id: u32,
        name: Option<String>,
    },
    /// Rank points the player earned (or lost), and the new total.
    RankingPoints {
        kind: u16,
        added: i32,
        total: u32,
    },
    /// A Star Gladiator's or Taekwon's target (STAR_MISSION: the Taekwon Mission, `count` kills of 100 done).
    StarSkill {
        name: String,
        monster: u32,
        count: u8,
        kind: u8,
    },
    /// The map's type (rathena's `e_map_type`; it only sends 19, a battleground).
    MapType(u16),
    /// The wedding effect on a unit (ZC_CONGRATULATION).
    Congratulation(u32),
    /// Someone nearby called their partner: the partner's name, empty when offline (ZC_COUPLENAME).
    PartnerName(String),
    /// Divorced from this partner (ZC_DIVORCE).
    Divorced(String),
    /// A parent (account) asks, with their partner (account), to adopt the player (ZC_REQ_BABY).
    AdoptAsked {
        parent: u32,
        partner: u32,
        name: String,
    },
    /// Asking to adopt, refused (ZC_BABYMSG): 0 a child already, 1 under level 70, 2 the one asked is married.
    AdoptRefused(u32),
    /// A teammate's HP on a battleground (ZC_BATTLEFIELD_NOTIFY_HP2).
    BattleHp {
        account: u32,
        hp: i32,
        max: i32,
    },
    /// A teammate's cell on the minimap (ZC_BATTLEFIELD_NOTIFY_POSITION); none when they left.
    BattlePosition {
        account: u32,
        cell: Option<[u16; 2]>,
    },
    /// A player's camp, rathena's team ID (ZC_BATTLEFIELD_NOTIFY_CAMPINFO).
    BattleCamp {
        account: u32,
        camp: u16,
    },
    /// Applying to a battleground's queue, answered (ZC_ACK_ENTRY_QUEUE_APPLY): rathena's `e_bg_queue_apply_ack`.
    BattleQueue {
        result: u8,
        name: String,
    },
    /// How many are queued for it (ZC_NOTIFY_ENTRY_QUEUE_APPLY).
    QueueNumber {
        name: String,
        number: u32,
    },
    /// Leaving the queue, answered (ZC_ACK_ENTRY_QUEUE_CANCEL): 1 left, 0 wasn't in it.
    QueueLeft {
        result: u8,
        name: String,
    },
    /// Both teams are full: enter? (ZC_NOTIFY_LOBBY_ADMISSION)
    QueueReady {
        name: String,
    },
    /// Entering, answered (ZC_REPLY_ACK_LOBBY_ADMISSION).
    QueueAccepted {
        result: u8,
        name: String,
    },
    /// Forget the queue: declined, expired, or gone in (ZC_ENTRY_QUEUE_INIT).
    QueueReset,
    /// Camp A's points and camp B's (ZC_BATTLEFIELD_NOTIFY_POINT).
    BattleScore([u16; 2]),
    /// A line of the team's chat, whole ("Name : hello"), and who said it (account 0: the server).
    BattleChat {
        account: u32,
        text: String,
    },
    /// A player's PvP rank among the players on the map (rank 0 or 0 players: no counter).
    PvpRank {
        account: u32,
        rank: u32,
        players: u32,
    },
    /// A server announcement, its color resolved; `size` is ZC_BROADCAST2's font size.
    Broadcast {
        text: String,
        color: [u8; 3],
        kind: BroadcastKind,
        size: Option<u16>,
    },
    /// Someone got a special item: out of a box (its item), from a monster (its name), or from an NPC (neither).
    ItemObtained {
        item: u32,
        player: String,
        box_item: Option<u32>,
        monster: Option<String>,
    },
    /// Someone's refine at a high level: the new level, or the one before a failure.
    RefineAnnounced {
        player: String,
        item: u32,
        level: u8,
        success: bool,
    },
    /// A boss's MVP: the player (account ID) who did the most against it.
    Mvp(u32),
    /// The MVP's reward: an item, or EXP; or a reward too heavy to take.
    MvpItem(u32),
    MvpExp(u32),
    MvpOverweight,
    /// A guild member's cell on this map, for the minimap; none when they left it (ZC_NOTIFY_POSITION_TO_GUILDM).
    GuildMemberPlace {
        account: u32,
        cell: Option<[i32; 2]>,
    },
    /// The weight at which the player counts as overweight, in percent of the limit (ZC_WEIGHT_INFO).
    OverweightPercent(u32),
    /// The carried equipment a card can go into (inventory indexes).
    CardList(Vec<u16>),
    /// A card put into equipment, or not; rathena takes the card away without saying so.
    CardInserted {
        equip: u16,
        card: u16,
        ok: bool,
    },
    /// How many spirit spheres (or coins) a unit carries; 0 takes them away.
    Spirits {
        id: u32,
        count: u16,
    },
    /// How many Millennium Shield shields (a Royal Guard's: rage spheres) a unit carries.
    Shields {
        id: u32,
        count: u16,
    },
    /// A unit's charms: their element (0 none, 1 water, 2 earth, 3 fire, 4 wind) and how many.
    Charms {
        id: u32,
        kind: u16,
        count: u16,
    },
    /// How many souls a unit carries.
    Souls {
        id: u32,
        count: u16,
    },
    /// The bank opens (rathena answers in its own words instead where banking is off).
    BankOpened,
    /// What the account keeps.
    BankBalance(i64),
    /// A deposit's or withdrawal's answer: rathena's reason (0 done), the balance and the zeny carried after.
    BankMoved {
        deposit: bool,
        reason: u16,
        balance: i64,
        zeny: i32,
    },
    CashShopOpen {
        cash: u32,
        free: u32,
        tab: u32,
    },
    CashShopList {
        tab: i16,
        items: Vec<(u32, u32)>,
    },
    CashShopBuyResult {
        item: u32,
        result: u16,
        cash: u32,
        free: u32,
    },
    /// Another player's equipment, as asked for.
    OtherEquipment(Equipped),
    /// The pet window's automatic feeding box, as the server says.
    PetAutoFeed(bool),
    /// The homunculus window's automatic feeding box, as the server says.
    HomunculusAutoFeed(bool),
    /// The Show Equipment box: the server's answer to a click, or the saved state at a map change.
    ShowEquipment {
        shown: bool,
        answer: bool,
    },
    /// The View Costume box: whether costumes show, as the server answered.
    CostumesShown(bool),
    /// May the inventory grow: 0 ask (with the voucher it would use), 1 failed, 2 busy, 3 no voucher, 4 at the most.
    ExpansionAsked {
        result: u8,
        voucher: u32,
    },
    /// The inventory grew (0), or why not (as above).
    ExpansionDone(u8),
    /// A market or barter opens with what it offers.
    Market {
        kind: MarketKind,
        goods: Vec<Bartered>,
    },
    /// A market purchase: done, with what was bought (item, amount), or failed.
    MarketBought {
        ok: bool,
        items: Vec<(u32, u16)>,
    },
    /// What can be made (Pharmacy, forging, rune stones...): the items' IDs.
    MakeList(Vec<u32>),
    /// A cooking kit's (or another) making list: its type and the items.
    CookList {
        kind: u16,
        items: Vec<u32>,
    },
    /// Arrow Crafting's (or a converter's, a poison's) list: the items it can use.
    ArrowList(Vec<u32>),
    /// Something made, or not: even results succeeded, odd ones failed.
    Made {
        result: u16,
        item: u32,
    },
    /// Repair Weapon's list: the target's broken items, indexed by rathena's own slots in the target's inventory.
    RepairList(Vec<InventoryItem>),
    /// An item repaired, or not.
    Repaired {
        index: u16,
        ok: bool,
    },
    /// Upgrade Weapon's list: the caster's weapons it can refine.
    UpgradeList(Vec<InventoryItem>),
    /// Upgrade Weapon's result (0 refined, 1 failed, 2 skill too low, 3 ore missing) with the item it names.
    Upgraded {
        result: u32,
        item: u32,
    },
    /// Hindsight's spells to pick from.
    SpellList(Vec<u32>),
    /// Skills to pick from for a reason (1: Shadow Spell), which goes back with the answer.
    SkillSelect {
        why: u32,
        skills: Vec<u16>,
    },
    /// The stylist's answer (ZC_STYLE_CHANGE_RES): true when the change failed.
    StyleChanged(bool),
    /// The refine window opens.
    RefineOpen,
    /// The enchant window opens on this enchant table.
    EnchantOpen(u64),
    /// An enchant's answer: a message number, and the enchant put in (0 on a failure and after a reset).
    Enchanted {
        message: u32,
        item: u32,
    },
    /// What an item in the refine window can be refined with: ores (item, chance %, zeny), and how many Blacksmith's
    /// Blessings protect it (0: they can't).
    RefineMaterials {
        index: u16,
        blessings: u8,
        materials: Vec<(u32, u8, u32)>,
    },
    /// A refine: 0 done, 1 failed and broke, 2 failed and went down, 3 failed; the item's level after.
    Refined {
        result: u16,
        index: u16,
        refine: u16,
    },
    /// ZC_UI_OPEN: a window by kind (1 is the stylist, 8 is grade enchanting) and the data it carries.
    UiOpen {
        kind: u8,
        data: u32,
    },
    /// What grades an item, or index −1 when its level has none. Chance is a percent. Blessing steps add `chance`
    /// each, up to `most`. Each material row is one option, in the order the request numbers them.
    GradeMaterials {
        index: i16,
        chance: u32,
        blessing: GradeBlessing,
        materials: Vec<GradeMaterial>,
    },
    /// A grade try: the item's grade after it, and the result (0 raised, 1 failed, 2 refine down, 3 destroyed, 4 protected).
    GradeResult {
        index: u16,
        grade: u16,
        result: u32,
    },
    /// Someone's grade try, told to every player: success raised it, failure did not.
    GradeAnnounced {
        player: String,
        item: u32,
        grade: u16,
        success: bool,
    },
    /// A Laphine synthesis box opened (the box's item ID).
    SynthesisOpen(u32),
    /// Synthesis result: 0 done, 5 an amount is wrong, 7 an item isn't wanted.
    Synthesized(u16),
    /// A Laphine upgrade box opened (the box's item ID).
    UpgradeOpen(u32),
    /// Upgrade result: 0 done, 1 refused. `Upgraded` is already Upgrade Weapon.
    LaphineUpgraded(u16),
    /// The item an upgrade just rewrote: its cards, refine and grade. Random options aren't tracked.
    ItemPreview {
        index: u16,
        cards: [u32; 4],
        refine: u8,
        grade: u8,
        broken: bool,
    },
    /// The item reform window opens for this reform item (the box, or the item an NPC named).
    ReformOpen(u32),
    /// A reform finished: result 0 means done, and the client closes the window.
    Reformed {
        index: u16,
        result: u8,
    },
    /// Eggs that can hatch, by inventory index.
    PetEggs(Vec<u16>),
    /// A pet's state changed: `kind` is one of the PET_* states above.
    PetState {
        kind: u8,
        id: u32,
        value: i32,
    },
    /// What the player's pet is like now.
    PetInfo(Pet),
    /// The player's mercenary: hired, or on a map load.
    MercenaryInfo(Mercenary),
    /// The mercenary's skills.
    MercenarySkills(Vec<Skill>),
    /// One of the mercenary's values changed (a MERCENARY_* kind).
    MercenaryChanged {
        kind: u16,
        value: u32,
    },
    /// The player's elemental, summoned or on a map load: its unit, HP, max HP, SP and max SP.
    ElementalInfo {
        id: u32,
        values: [u32; 4],
    },
    /// One of the elemental's values changed: MERCENARY_HP to MERCENARY_MAX_SP.
    ElementalChanged {
        kind: u16,
        value: u32,
    },
    /// ZC_CHANGESTATE_MER: a HOMUNCULUS_* state of the homunculus `id`.
    HomunculusState {
        state: u8,
        id: u32,
        value: u32,
    },
    /// What the player's homunculus is like now.
    HomunculusInfo(Homunculus),
    /// One of the homunculus's values changed (a HOMUNCULUS_* kind).
    HomunculusChanged {
        kind: u16,
        value: u64,
    },
    /// The homunculus's skills.
    HomunculusSkills(Vec<Skill>),
    /// One of the homunculus's skills changed; its kind isn't in the packet (0).
    HomunculusSkill(Skill),
    /// The homunculus was fed (or there was no food), and its food.
    HomunculusFed {
        ok: bool,
        food: u32,
    },
    /// The pet was fed (or there was no food), and its food.
    PetFed {
        ok: bool,
        food: u32,
    },
    /// A pet emotes or talks.
    PetAct {
        id: u32,
        act: PetAct,
    },
    /// A unit's new name (a renamed pet).
    UnitName {
        id: u32,
        name: String,
    },
    /// A taming item was used: pick the monster to tame.
    CaptureStart,
    /// The monster was tamed (its egg comes next), or not.
    CaptureResult(bool),
    /// The material window opens: 0 for Change Material, 1 or 2 for Four Spirit Analysis at that level.
    MaterialWindow(u32),
    /// All the player's achievements, with the totals (at login, and after a reward that gives a title).
    Achievements {
        totals: AchievementTotals,
        list: Vec<Achievement>,
    },
    /// One achievement added or changed (none: the totals only), with the totals.
    AchievementUpdate {
        totals: AchievementTotals,
        achievement: Option<Achievement>,
    },
    /// A reward claim taken, or refused.
    AchievementReward {
        ok: bool,
        id: u32,
    },
    /// The title put on (0: taken off), or refused.
    TitleChanged {
        ok: bool,
        title: u32,
    },
    /// A skill can't be used again for this long from now (0: it can again).
    SkillCooldown {
        skill: u16,
        ms: u32,
    },
    /// The roulette window opens: a prize waiting to be claimed (row, column), the bonus item, and the gold, silver
    /// and bronze points.
    RouletteOpen {
        prize: Option<(u8, u8)>,
        bonus: u32,
        points: [i32; 3],
    },
    /// The roulette's prizes.
    RoulettePrizes(Vec<RoulettePrize>),
    /// A spin's result (0 won, 1 failed, 2 not enough points, 3 lost) with where it stopped, the bonus and the points.
    RouletteSpun {
        result: u8,
        row: u8,
        column: u8,
        bonus: u32,
        points: [i32; 3],
    },
    /// A claim's result (0 claimed, 1 failed, 2 too many items, 3 too heavy) and the next bonus item.
    RouletteClaimed {
        result: u8,
        bonus: u32,
    },
    /// A bind-on-equip item (inventory index) was put on: it's bound to the character now.
    BoundOnEquip(u16),
    /// Take Off All was done, or refused (the player can't act).
    TookOffAll {
        refused: bool,
    },
    /// The Swap Equipment set: inventory index and the places it goes in.
    SwitchSet(Vec<(u16, u32)>),
    /// An item went into the set (result 0), or didn't: 1 level too low, 2 can't.
    SwitchAdded {
        index: u16,
        places: u32,
        result: u16,
    },
    /// An item left the set.
    SwitchRemoved {
        index: u16,
        places: u32,
    },
    /// The item merge window opens with the stacks (inventory indexes) that can merge.
    MergeOpen(Vec<u16>),
    /// Stacks merged into the one at `index`, now `amount` (result 0), or not: 2 the total is too big.
    Merged {
        index: u16,
        amount: u16,
        result: u8,
    },
    /// An item went into the favourites, or out.
    FavoriteMoved {
        index: u16,
        favorite: bool,
    },
    /// The set is switched with what's worn now.
    Switched {
        failed: bool,
    },
    /// A rental (by item ID) has this many seconds left.
    RentalTime {
        item: u32,
        seconds: u32,
    },
    /// The rental at an inventory index has ended.
    RentalExpired {
        index: u16,
        item: u32,
    },
    /// Sense's report on a monster.
    MonsterInfo(MonsterInfo),
    /// The Convex Mirror on the map's boss: 0 none here, 2 alive (first), 1 alive (moved), 3 dead (back in hours and
    /// minutes).
    BossInfo {
        kind: u8,
        cell: [i32; 2],
        hours: u16,
        minutes: u16,
        name: String,
    },
    /// A Talkie Box went off: its ground unit, and the message it held.
    TalkieBox {
        unit: u32,
        text: String,
    },
    /// One of the client's messages, by number: with a number for its `%d`, a skill for its `%s`, in a color.
    Message {
        id: usize,
        value: Option<i32>,
        skill: Option<u16>,
        color: Option<[u8; 3]>,
    },
    /// Ammunition or weight trouble: 0 no ammunition, 1 and 2 too heavy, 3 ammunition equipped.
    ActionFailure(u16),
    /// A sound a script plays (`data\wav\<file>`) from where a unit stands: act 0 once, 1 on repeat, 2 stops it.
    Sound {
        file: String,
        act: u8,
        unit: u32,
    },
    /// Music a script plays instead of the map's (a BGM file, maybe without ".mp3").
    NpcMusic(String),
    /// Where Warp Portal or Teleport may take the player: map names with ".gat" (Teleport's first is "Random").
    WarpList {
        skill: u16,
        maps: Vec<String>,
    },
    /// Saving a memo point: 0 saved, 1 Warp Portal's level is too low, 2 it isn't learned.
    MemoResult(u8),
    /// What this map doesn't allow: 0 teleporting, 1 memo points, 2 skills, 3 items.
    MapForbids(u16),
    /// Whether unread mail waits (ZC_NOTIFY_UNREADMAIL).
    MailUnread(bool),
    /// The mailbox, newest first.
    MailList(Vec<MailHeader>),
    /// A page of the guild storage's log: result 0 more to come, 1 the last, 2 no log, else refused.
    GuildStorageLog {
        result: u16,
        entries: Vec<GuildStorageEntry>,
    },
    /// A mail opened: its text, and the zeny and items still in it.
    MailRead {
        kind: u8,
        id: u64,
        text: String,
        zeny: u64,
        items: Vec<InventoryItem>,
    },
    /// Taking a mail's zeny (or its items): 0 done, 1 failed (or too much zeny), 2 too heavy or no room.
    MailClaimed {
        id: u64,
        zeny: bool,
        result: u8,
    },
    MailDeleted {
        id: u64,
    },
    /// Whether writing a mail (to that name) may start.
    MailWrite {
        to: String,
        ok: bool,
    },
    /// Whose name it is: their character ID (0 for nobody), job and level.
    MailReceiver {
        character: u32,
        job: u16,
        level: u16,
        name: String,
    },
    /// An item went in the mail being written (result 0; 1 too heavy, 2 failed, 3 no room, 4 can't be mailed),
    /// with what the attachments weigh now.
    MailItemAdded {
        result: u8,
        item: InventoryItem,
        weight: u16,
    },
    MailItemRemoved {
        ok: bool,
        index: u16,
        amount: u16,
        weight: u16,
    },
    /// How sending went: 0 sent, 1 failed, 2 too many today, 3 an item can't go, 4 no such receiver.
    MailSent(u8),
    /// The quest log at login, a quest taken, and quests leaving it.
    Quests(Vec<Quest>),
    QuestAdded(Quest),
    QuestRemoved(u32),
    /// A quest put aside or taken up again.
    QuestActive {
        id: u32,
        active: bool,
    },
    /// Hunts' counts: objective ID, how many so far, how many in all.
    QuestHunts(Vec<(u32, u16, u16)>),
    /// Hunts that progressed with a kill (ZC_UPDATE_MISSION_HUNT_EX), as `QuestHunts`.
    QuestProgress(Vec<(u32, u16, u16)>),
    /// The cart: how many kinds it holds and can, its weight and most weight.
    CartCount {
        count: u16,
        most: u16,
        weight: i32,
        max_weight: i32,
    },
    CartAdded(InventoryItem),
    CartRemoved {
        index: u16,
        amount: u32,
    },
    /// The player's guild: its ID, name, whether we lead it, its emblem's version (ZC_UPDATE_GDID).
    Guild {
        id: u32,
        name: String,
        master: bool,
        emblem: u32,
        /// Our position's rights (invite 0x01, expel 0x10).
        mode: u32,
    },
    GuildInfo(GuildInfo),
    /// The guild window tabs the player may open: rathena's menu flag (0x57 members, 0xd7 the master).
    GuildMenu(i32),
    /// A stat or trait raise answered (ZC_STATUS_CHANGE_ACK): its new value when it went up, none when refused.
    // ponytail: 0x0B25 would answer a trait too, but it isn't in this server's version.
    RaiseAnswer(Option<(u16, i64)>),
    GuildMembers(Vec<GuildMember>),
    /// Position names, by position ID.
    GuildPositions(Vec<(u32, String)>),
    /// Positions' permissions and tax (ZC_POSITION_INFO).
    GuildPositionInfo(Vec<GuildPosition>),
    /// A position changed (ZC_ACK_CHANGE_GUILD_POSITIONINFO): its permissions and tax, and its title.
    GuildPositionChanged(GuildPosition, String),
    /// A member moved to another position (ZC_ACK_REQ_CHANGE_MEMBERS).
    GuildMemberPosition {
        account: u32,
        character: u32,
        position: u32,
    },
    /// The position title under a player's name (ZC_GUILD_POSITION); empty without a guild.
    PositionTitle {
        account: u32,
        title: String,
    },
    /// The guild's skills and the points left to put in them.
    GuildSkills {
        points: u16,
        skills: Vec<Skill>,
    },
    /// Who was expelled from the guild (name), and why.
    GuildExpelled(Vec<(String, String)>),
    /// A guild's emblem: a zlib-compressed 24x24 BMP.
    GuildEmblem {
        guild: u32,
        version: u32,
        data: Vec<u8>,
    },
    /// A player's guild or its emblem changed (ZC_CHANGE_GUILD).
    GuildEmblemChanged {
        account: u32,
        guild: u32,
        emblem: u32,
    },
    /// A player's party, guild and position, shown with their name (ZC_ACK_REQNAMEALL), by account ID.
    NameTag(u32, NameTag),
    GuildNotice {
        subject: String,
        text: String,
    },
    GuildChat(String),
    /// A member logged in or out.
    GuildMemberOnline {
        account: u32,
        character: u32,
        online: bool,
    },
    /// Making a guild: 0 made, 1 already in one, 2 the name's taken, 3 no Emperium.
    GuildMade(u8),
    /// A guild invites the player.
    GuildInvite {
        id: u32,
        name: String,
    },
    /// How an invitation went: 0 they're in a guild, 1 refused, 2 joined, 3 the guild is full.
    GuildInviteResult(u8),
    /// The guild's allies and antagonists.
    GuildRelations(Vec<GuildRelation>),
    /// Another guild (its name) asks ours for an alliance, through the player who asked (account ID).
    AllyAsked {
        id: u32,
        guild: String,
    },
    /// How an alliance request went (0 already allied, 1 they're answering another, 2 allied, 3 refused or they
    /// have too many, 4 we have too many, 5 not allowed).
    AllyResult(u8),
    /// How setting an antagonist went: 0 set, 1 too many, 2 already one.
    AntagonistResult(u8),
    /// A relation with a guild ended.
    GuildRelationEnded {
        id: u32,
        antagonist: bool,
    },
    /// Disbanding the guild: 0 done, 1 wrong name, 2 members are left.
    GuildDisbanded(u32),
    /// Someone (by character ID) left or was expelled, and why.
    GuildLeft {
        character: u32,
        reason: String,
        expelled: bool,
    },
    /// The character's clan, at login or upon joining (`ZC_CLANINFO`).
    ClanInfo {
        id: u32,
        name: String,
        master: String,
        map: String,
        allies: Vec<String>,
        hostile: Vec<String>,
    },
    /// Members online and the clan's capacity (`ZC_NOTIFY_CLAN_CONNECTINFO`).
    ClanOnlineCount {
        online: i16,
        max: i16,
    },
    /// The character left their clan (`ZC_ACK_CLAN_LEAVE`).
    ClanLeft,
    /// A line of clan chat (`ZC_NOTIFY_CLAN_CHAT`).
    ClanChat(String),
    /// The friend list, at login (everyone offline until told otherwise).
    Friends(Vec<Friend>),
    /// A friend logged in or out.
    FriendOnline {
        account: u32,
        character: u32,
        online: bool,
    },
    /// Someone asks to be friends.
    FriendAsked {
        account: u32,
        character: u32,
        name: String,
    },
    /// How asking went: 0 friends now, 1 refused, 2 our list is full, 3 theirs is.
    FriendAdded {
        result: u16,
        friend: Friend,
    },
    FriendRemoved {
        account: u32,
        character: u32,
    },
    /// The whole party (ZC_GROUP_LIST), at login and when it changes.
    Party {
        name: String,
        members: Vec<PartyMember>,
    },
    /// Someone joined or logged in or out (ZC_ADD_MEMBER_TO_GROUP); where they are, and the item rules.
    PartyMember {
        party: String,
        member: PartyMember,
        cell: [u16; 2],
        pickup: bool,
        share: bool,
    },
    /// Someone left (0) or was expelled (1); 2 and 3 mean leaving isn't allowed here.
    PartyLeft {
        account: u32,
        name: String,
        result: u8,
    },
    /// How making a party went: 0 made, 1 the name is taken, 2 already in one, 3 not on this map.
    PartyMade(u8),
    /// Someone asks to join the party through its booking post: their character and account, name, base level and
    /// job.
    BookingJoinAsked {
        character: u32,
        account: u32,
        name: String,
        level: u16,
        job: u16,
    },
    /// A booking post's leader answered the player's request (their name, the party's, their account).
    BookingJoinAnswered {
        leader: String,
        party: String,
        account: u32,
        refused: bool,
    },
    /// A party asks the player to join.
    PartyInvite {
        party: u32,
        name: String,
    },
    /// How an invitation went (`name` is who was asked); see the client's messages 80-83 and on.
    PartyInviteResult {
        name: String,
        result: u32,
    },
    PartyChat {
        account: u32,
        text: String,
    },
    /// A member's place on the minimap; none when they left the map.
    PartyPosition {
        account: u32,
        cell: Option<[u16; 2]>,
    },
    /// A member (by account) picked up an item.
    PartyPickup {
        account: u32,
        item: u32,
        identified: bool,
        refine: u8,
    },
    /// Whether the player refuses party invitations.
    PartyRefusing(bool),
    PartyHp {
        account: u32,
        hp: i32,
        max: i32,
    },
    /// Shared experience (0 no, 1 yes, 2 can't change) and the item rules.
    PartyOptions {
        exp: u32,
        pickup: bool,
        share: bool,
    },
    PartyLeader {
        old: u32,
        new: u32,
    },
    PartyMemberInfo {
        account: u32,
        job: u16,
        level: u16,
    },
    PartyDead {
        account: u32,
        dead: bool,
    },
    /// Someone asks the player for a deal: their name, character ID and level.
    TradeAsked {
        name: String,
        id: u32,
        level: u16,
    },
    /// How asking for a deal went: 0 too far, 1 no such character, 2 they're in another deal, 3 it's on, 4 refused,
    /// 5 busy.
    TradeAnswer {
        result: u8,
        id: u32,
        level: u16,
    },
    /// The partner put in an item, or zeny.
    TradeItem(InventoryItem),
    TradeZeny(u32),
    /// Whether an item went in: 0 yes, 1 too heavy for them, 2 the deal was cancelled, 3 their inventory is full,
    /// 4 too many of it.
    TradeAdded {
        index: u16,
        result: u8,
    },
    /// One side locked in (OK).
    TradeLocked {
        partner: bool,
    },
    TradeCancelled,
    /// The deal's end: 0 done, 1 failed.
    TradeDone {
        result: u8,
    },
    /// Someone whispered to the player.
    Whisper {
        from: String,
        text: String,
    },
    /// How a whisper went: 0 sent, 1 nobody of that name online, 2 ignored by them, 3 they ignore everyone.
    WhisperResult(u8),
    /// An item casts a skill the player may not know (a Magnifier's Item Appraisal, a scroll's bolt): cast it now.
    ItemSkill(Skill),
    /// The inventory indexes a Magnifier can identify; the server waits for a pick (or a cancel) before anything else.
    IdentifyList(Vec<u16>),
    /// How identifying went.
    Identified {
        index: u16,
        ok: bool,
    },
    /// How many players are online (/who).
    UserCount(u32),
    /// How blocking (`allow` false) or allowing one name's whispers went: 0 done, 1 failed, 2 the list is full.
    WhisperSetting {
        allow: bool,
        result: u8,
    },
    /// The names whose whispers are blocked.
    WhisperList(Vec<String>),
    /// How blocking (`allow` false) or allowing everyone's whispers went: 0 done, else failed.
    WhisperAll {
        allow: bool,
        result: u8,
    },
    /// An NPC's illustration (`illust\<image>`): where (0 bottom left, 1 bottom middle, 2 bottom right, 3 and 4
    /// the middle of the screen; anything else, or no image, takes it away).
    Cutin {
        image: String,
        place: u8,
    },
    /// An NPC marks a cell on the minimap: `kind` 0 for 15 seconds, 1 until taken away, 2 takes it away.
    /// An NPC's quest mark (ZC_QUEST_NOTIFY_EFFECT): where it stands, the effect (9999 QTYPE_NONE: none) and the mark's
    /// colour (0 none, 1 yellow, 2 green, 3 purple).
    QuestMark {
        npc: u32,
        cell: [u16; 2],
        effect: u16,
        color: u16,
    },
    Mark {
        npc: u32,
        kind: u32,
        cell: [u16; 2],
        id: u8,
        color: [u8; 3],
    },
    /// A unit's body state, health state and option bits changed (ZC_STATE_CHANGE).
    OptionChange {
        id: u32,
        /// rathena's opt1.
        body: u16,
        /// rathena's opt2.
        health: u16,
        option: u32,
    },
    /// A unit's looks changed (`kind` is a `LOOK_*`); for weapons `extra` is the shield.
    /// A unit takes another job's sprite (rathena's `setnpcdisplay`, a homunculus evolving or mutating).
    JobSprite {
        id: u32,
        job: u32,
    },
    LookChange {
        id: u32,
        kind: u8,
        value: u32,
        extra: u32,
    },
    /// Putting on equipment: where it went, or a nonzero `result` (1 level too low, 2 can't).
    Equipped {
        index: u16,
        location: u32,
        result: u8,
    },
    /// Taking off equipment, unless `failed`.
    Unequipped {
        index: u16,
        location: u32,
        failed: bool,
    },
    /// The hotkeys of one tab (`rotate`: the row shown first).
    Hotkeys {
        tab: u16,
        rotate: u8,
        keys: Vec<Option<Hotkey>>,
    },
    /// The player's skills (ZC_SKILLINFO_LIST).
    Skills(Vec<Skill>),
    /// A skill learned or changed (level, SP, range, whether it can go up).
    SkillChanged(Skill),
    SkillRemoved(u16),
    /// Reputation points, by reputation type (ZC_REPUTE_INFO): all of them on entering the map, one when it changes.
    Reputation(Vec<(u64, i64)>),
    /// The server opens the reputation window (ZC_REPUTE_OPEN) at a group and a reputation (0: none).
    ReputationOpen {
        group: u64,
        repute: u32,
    },
    /// The server starts a navigation (ZC_NAVIGATION_ACTIVE): to a cell (kind 0), a map (1) or a monster (3) on
    /// `map`; `services` are the digits of rathena's NAV_ flags (ones the airship, tens scrolls, hundreds Kafra);
    /// `hide_window` keeps the navigation window shut.
    Navigate {
        kind: u8,
        services: u8,
        hide_window: bool,
        map: String,
        cell: [u16; 2],
        monster: u16,
    },
    /// A unit's cast was cut off before the skill went (ZC_DISPEL).
    CastCancelled(u32),
    /// A unit starts a skill (ZC_USESKILL_ACK) at a unit or a cell, casting for `cast_ms` first.
    Casting {
        source: u32,
        target: u32,
        cell: [u16; 2],
        skill: u16,
        element: u32,
        cast_ms: u32,
    },
    /// A skill hit (ZC_NOTIFY_SKILL); `act.kind` 8 splits the damage into hits.
    SkillHit {
        skill: u16,
        level: u16,
        act: Act,
    },
    /// A skill without damage, like a heal or a buff (ZC_USE_SKILL); `amount` is how much a heal healed.
    SkillUsed {
        skill: u16,
        amount: i32,
        target: u32,
        source: u32,
        ok: bool,
    },
    /// A skill aimed at a cell (ZC_NOTIFY_GROUNDSKILL).
    GroundSkill {
        skill: u16,
        source: u32,
        level: u16,
        cell: [u16; 2],
    },
    /// A skill's unit on a cell comes into sight (ZC_SKILL_ENTRY): a Fire Wall, a warp portal, a trap. `kind` is
    /// rathena's UNT_ number.
    SkillUnit {
        id: u32,
        kind: u32,
        cell: [u16; 2],
    },
    /// A skill's unit is gone or out of sight (ZC_SKILL_DISAPPEAR).
    SkillUnitGone {
        id: u32,
    },
    /// The player's skill didn't go off (ZC_ACK_TOUSESKILL) unless `ok`: rathena's `useskill_fail_cause`, a
    /// skill-specific detail (`btype`) and the item it's about.
    SkillFailed {
        skill: u16,
        detail: i32,
        item: u32,
        ok: bool,
        cause: u8,
    },
    /// HP (5) or SP (7) restored (ZC_RECOVERY).
    Recovered {
        kind: u16,
        amount: i32,
    },
    /// ZC instance create (0x02CB): the name and the place in the waiting line.
    InstanceCreate {
        name: String,
        place: u16,
    },
    /// ZC instance changewait (0x02CC): the new place in line.
    InstanceWait {
        place: u16,
    },
    /// ZC instance status (0x02CD): the name and the time-limit and idle-limit Unix times.
    InstanceStatus {
        name: String,
        time_limit: u32,
        idle_limit: u32,
    },
    /// ZC instance changestatus (0x02CE): rathena `e_instance_notify` and a Unix time (the idle limit on notify 0).
    InstanceNotify {
        kind: u32,
        time: u32,
    },
    /// A status effect began or ended on a unit (ZC_MSG_STATE_CHANGE): the client's EFST id and the time left.
    StatusIcon {
        kind: u16,
        id: u32,
        on: bool,
        total_ms: u32,
        left_ms: u32,
        /// The status's first value where the server sends one (a pushcart's type), else 0.
        value: i32,
    },
    Disconnected(io::Error),
}
