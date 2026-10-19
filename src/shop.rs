//! NPC shops as the client shows them: a window of what the shop sells (or would buy), and one of the deal being
//! made with its total, side by side.

use std::collections::HashMap;

use rustnarok_client::grf::Grf;
use rustnarok_client::lua::ItemInfo;
use rustnarok_client::net::{BuyingItem, ITEM_ARMOR, ITEM_PET_ARMOR, ITEM_PET_EGG, ITEM_WEAPON, InventoryItem, MapSession, MarketKind};
use rustnarok_client::text::Font;

use crate::inventory;
use crate::render::{Gpu, Texture};
use crate::window::{self, Chrome, Clicks, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI, contains, offset};

const WIDTH: f32 = 280.0;
const ROW: f32 = 32.0;
/// Rows each window shows at first: the goods (280×312) and the deal (280×152).
pub const ROWS: [usize; 2] = [8, 3];
/// The corner grips: the goods both ways, 248, 280 or 312 wide (232 to 320 asked, stepping toward 280); either window
/// 120 to 312 high, a row per 32 px (2 to 8 rows here). A player's shop and a buying store have none.
const LEAST_ROWS: usize = 2;
const MOST_ROWS: usize = 8;

/// The goods' width for an asked one: 248, 280 or 312.
pub fn snap_width(asked: f32) -> f32 {
    WIDTH + ROW * ((asked.clamp(232.0, 320.0) - WIDTH) / ROW).trunc()
}

/// The windows' sizes before any are kept: the goods' width and both windows' rows.
pub fn default_sizes() -> (f32, [usize; 2]) {
    (WIDTH, ROWS)
}

/// A window's height with `rows` rows.
fn height(rows: usize) -> f32 {
    TITLE_HEIGHT + PADDING * 2.0 + rows as f32 * ROW + FOOTER
}
/// The goods on the inventory's spot, the deal beside and under it.
const POSITIONS: [[f32; 2]; 2] = [[0.0, 120.0], [280.0, 280.0]];
/// A market's goods: where the icon and name go, and how wide the name may be.
const MARKET_ICON: f32 = 34.0;
const MARKET_NAME: f32 = 70.0;
const MARKET_NAME_WIDTH: f32 = 120.0;
/// Where prices end, from the right edge.
const PRICE_RIGHT: f32 = 18.0;
const PADDING: f32 = 5.0;
const LEFT: f32 = 16.0;
/// In a row (from its top, 17 + 32 per row): the icon, the name, and the selected row's box (28 high, to 17 short of
/// the right edge).
const ICON: [f32; 2] = [44.0, 4.0];
const NAME: [f32; 2] = [76.0, 12.0];
const SELECTED_BOX: [f32; 2] = [72.0, 2.0];
const SELECTED_HEIGHT: f32 = 28.0;
/// The tooltip: 16 px above the row, stepping 32 px with the pointer from x 40.
const TIP_ABOVE: f32 = 16.0;
const TIP_STEP: [f32; 2] = [40.0, 32.0];
/// "Toggle Item Amount.": selling to an NPC, a whole stack moves at once. The box at (12, h − 20) of the goods.
const WHOLE_MESSAGE: usize = 295;
const WHOLE_BOX: [f32; 2] = [200.0, 12.0];
const WHOLE_UP: f32 = 20.0;
const FOOTER: f32 = 27.0;
const BUTTON: [f32; 2] = [42.0, 20.0];
const SELECTED: [u8; 4] = [0x34, 0x6A, 0xE1, 0x80];
/// Titles: the goods when buying and selling, the deal when buying and selling.
const TITLES: [[usize; 2]; 2] = [[186, 185], [166, 168]];
/// "합계 : %d z": a market's total.
const TOTAL_MESSAGE: usize = 3699;
/// Barters: "%s %d ea" (an item it costs), "money" (zeny it costs), and the deal window's title.
const COSTS_MESSAGE: usize = 3553;
const MONEY_MESSAGE: usize = 3552;
const EXCHANGE_TITLE: usize = 3689;
const LIMIT_MESSAGE: usize = 1735;
const WANTED_TITLE: usize = 1737;
pub const OVER_WANTED_MESSAGE: usize = 1739;
pub const OVER_LIMIT_MESSAGE: usize = 1740;
const GREY: [u8; 3] = [0x70, 0x70, 0x70];
/// Buying from an NPC: "Please avoid buying 2 of the same items at one time.", "It is not possible to purchase the
/// same item more than 30000 pieces at a time", "You do not have enough zeny."
pub const SAME_EQUIPMENT: usize = 119;
pub const OVER_30000: usize = 1704;
pub const NOT_ENOUGH_ZENY: usize = 55;

pub struct Assets {
    strip: Texture,
    bar: Texture,
    /// Buy, sell, cancel and exchange: normal, hovered, pressed.
    buttons: [[Texture; 3]; 4],
    selected: Texture,
    /// The corner grip.
    resize: Texture,
    check: [Texture; 2],
}

impl Assets {
    /// Buy, sell and cancel.
    pub fn buttons(&self) -> [&[Texture; 3]; 3] {
        [&self.buttons[0], &self.buttons[1], &self.buttons[2]]
    }

    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |path: String| gpu.load_texture(grf, &path, false);
        let button = |name: &str| ["", "_a", "_b"].map(|suffix| image(format!("{UI}btn_{name}{suffix}.bmp")));
        Self {
            strip: image(format!("{UI}basic_interface\\itemwin_left.bmp")),
            bar: image(format!("{UI}basic_interface\\btnbar_mid2.bmp")),
            buttons: [button("buy"), button("sell"), button("cancel"), button("exchange")],
            selected: gpu.solid(SELECTED),
            resize: image(format!("{UI}btn_resize.bmp")),
            check: ["0", "1"].map(|look| image(format!("{UI}checkbox_{look}.bmp"))),
        }
    }
}

/// Something the shop sells, or something carried it would buy.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Offer {
    pub item: u32,
    pub price: u32,
    /// The price before a discount (buying) or overcharge (selling); 0 or `price` when there's none.
    pub listed: u32,
    /// Selling: the inventory entry and how many are carried.
    pub index: u16,
    pub have: u16,
    pub kind: u8,
    pub identified: bool,
    pub refine: u8,
    /// Markets and barters: how many are left (None: no end).
    pub stock: Option<u32>,
    /// Barters: the items it costs and how many (`price` is the zeny it costs too).
    pub costs: Vec<(u32, u32)>,
    /// Barters: the shop's own index for it, and the inventory index of the item it costs.
    pub shop_index: u32,
    pub cost_index: u16,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(usize, TitleHit),
    Row(usize, usize),
    Deal,
    Cancel,
    /// A window's corner grip.
    Resize(usize),
    /// The "Toggle Item Amount." box.
    Whole,
    Window(usize),
}

/// What a click or a drop asks the client to do.
#[derive(Debug, PartialEq)]
pub enum Action {
    /// Buy or sell what's in the deal.
    Deal,
    Close,
    /// Ask how many of an offer to put in the deal, the box filled with this many.
    Ask(usize, u32),
    /// Ask how many of a deal row to take out, the box filled with all of them.
    AskOut(usize, u32),
    /// Say a message (with a number in it): a refused addition.
    Say(usize, Option<u32>),
}

pub struct Shop {
    pub selling: bool,
    /// A player's shop: the vendor, their shop's ID, and its name (the first window's title).
    pub vendor: Option<(u32, u32, String)>,
    /// A player's buying store we are selling into: `(owner, store_id, zeny_limit)`.
    pub buying_store: Option<(u32, u32, u32)>,
    /// A market or barter rather than a plain NPC shop.
    pub market: Option<MarketKind>,
    pub offers: Vec<Offer>,
    /// Offers in the deal, and how many.
    pub deal: Vec<(usize, u32)>,
    pub frames: [Frame; 2],
    /// Each window's rows, the goods' width, and the corner grips while held.
    rows: [usize; 2],
    width: f32,
    grips: [window::Grip; 2],
    scroll: [usize; 2],
    selected: [Option<usize>; 2],
    clicks: Clicks,
    /// The zeny carried, for what buying from an NPC may add.
    pub zeny: u64,
    /// "Toggle Item Amount." is ticked: off at start, not kept.
    whole: bool,
}

impl Shop {
    pub fn new(selling: bool, offers: Vec<Offer>) -> Self {
        let frames = POSITIONS.map(|position| {
            let mut frame = Frame::new(position, WIDTH);
            frame.open = true;
            frame
        });
        Self {
            selling,
            vendor: None,
            buying_store: None,
            market: None,
            offers,
            deal: Vec::new(),
            frames,
            rows: ROWS,
            width: WIDTH,
            grips: [window::Grip::default(); 2],
            scroll: [0; 2],
            selected: [None; 2],
            clicks: Clicks::default(),
            zeny: u64::MAX,
            whole: false,
        }
    }

    /// A plain NPC shop (not a player's shop or store, a market or a barter).
    pub fn plain(&self) -> bool {
        self.vendor.is_none() && self.buying_store.is_none() && self.market.is_none()
    }

    /// Windows held by their title bars follow the pointer; one held by its corner grip takes the size it asks
    /// for.
    pub fn drag_to(&mut self, cursor: [f32; 2]) {
        for window in 0..2 {
            if let Some([width, asked]) = self.grips[window].asked(cursor) {
                let rows = LEAST_ROWS + window::steps(asked, height(LEAST_ROWS), ROW, MOST_ROWS - LEAST_ROWS);
                self.set_size(window, (window == 0).then_some(width), rows);
            }
            let height = self.height(window);
            self.frames[window].drag_to(cursor, height);
        }
    }

    /// Whether the windows have corner grips: an NPC's shop or a market, not a player's shop or buying store.
    fn resizable(&self) -> bool {
        self.vendor.is_none() && self.buying_store.is_none()
    }

    /// Sizes a window: the goods' width (snapped) and the rows.
    pub fn set_size(&mut self, window: usize, width: Option<f32>, rows: usize) {
        if let Some(width) = width.filter(|_| window == 0) {
            self.width = snap_width(width);
            self.frames[0].set_width(self.width);
        }
        self.rows[window] = rows.clamp(LEAST_ROWS, MOST_ROWS);
        let most = self.rows(window).len().saturating_sub(self.rows[window]);
        self.scroll[window] = self.scroll[window].min(most);
    }

    /// The goods' width and both windows' rows, for keeping them (None for a player's shop or store).
    pub fn sizes(&self) -> Option<(f32, [usize; 2])> {
        self.resizable().then_some((self.width, self.rows))
    }

    /// Puts back kept sizes.
    pub fn apply_sizes(&mut self, (width, rows): (f32, [usize; 2])) {
        if !self.resizable() {
            return;
        }
        self.set_size(0, Some(width), rows[0]);
        self.set_size(1, None, rows[1]);
    }

    /// A press on a window's corner grip holds it; whether it did.
    pub fn start_resize(&mut self, cursor: [f32; 2]) -> bool {
        let Some(Hit::Resize(window)) = self.hit(cursor) else { return false };
        let size = [self.window_width(window), self.height(window)];
        self.grips[window].start(cursor, size);
        true
    }

    /// Lets go of the grips; whether one was held.
    pub fn stop_resize(&mut self) -> bool {
        self.grips.iter_mut().fold(false, |held, grip| grip.stop() | held)
    }

    fn window_width(&self, window: usize) -> f32 {
        if window == 0 { self.width } else { WIDTH }
    }

    /// Which of its windows is under the pointer: 0 the goods, 1 the deal.
    pub fn window_at(&self, cursor: [f32; 2]) -> Option<usize> {
        match self.hit(cursor)? {
            Hit::Title(window, _) | Hit::Row(window, _) | Hit::Resize(window) | Hit::Window(window) => Some(window),
            Hit::Deal | Hit::Cancel => Some(1),
            Hit::Whole => Some(0),
        }
    }

    /// The offer a row of a window shows (window 1: a deal row).
    pub fn offer_at(&self, window: usize, row: usize) -> Option<&Offer> {
        let offer = if window == 0 { row } else { self.deal.get(row)?.0 };
        self.offers.get(offer)
    }

    /// An offer dragged onto the deal: equipment (and a single one carried) goes in at once, the rest asks how many,
    /// the box filled with 1 when buying and with the whole stack when selling.
    pub fn drop_offer(&mut self, row: usize) -> Option<Action> {
        let offer = self.offers.get(row)?;
        if self.buying_store.is_some() {
            return (offer.have > 0).then_some(Action::Ask(row, u32::from(offer.have)));
        }
        if self.whole_moves() {
            return self.add_checked(row, u32::from(offer.have)).map(|(message, arg)| Action::Say(message, arg));
        }
        let selling = self.selling || self.vendor.is_some();
        let single = matches!(offer.kind, ITEM_WEAPON | ITEM_ARMOR | ITEM_PET_EGG | ITEM_PET_ARMOR) || (selling && offer.have == 1);
        if single {
            return self.add_checked(row, 1).map(|(message, arg)| Action::Say(message, arg));
        }
        Some(Action::Ask(row, if self.selling { u32::from(offer.have) } else { 1 }))
    }

    /// A deal row dragged back onto the goods: one (or with the box ticked, all) comes out at once, more asks how
    /// many.
    pub fn drop_deal(&mut self, row: usize) -> Option<Action> {
        let &(_, amount) = self.deal.get(row)?;
        if amount == 1 || self.whole_moves() {
            self.take_out(row, amount);
            return None;
        }
        Some(Action::AskOut(row, amount))
    }

    /// Takes `amount` of a deal row out of the deal (all of them: the row goes).
    pub fn take_out(&mut self, row: usize, amount: u32) {
        let Some((_, had)) = self.deal.get_mut(row) else { return };
        if amount >= *had {
            self.deal.remove(row);
            self.selected[1] = None;
        } else {
            *had -= amount;
        }
    }

    /// Opens a seller's window for another player's buying store (0x0818): skips wanted rows at 0 and matches
    /// carried stacks that are identified, not worn, and have no cards.
    pub fn new_buying_store(owner: u32, store_id: u32, limit: u32, wanted: &[BuyingItem], inventory: &[InventoryItem]) -> Self {
        let offers = wanted
            .iter()
            .filter(|item| item.amount > 0)
            .map(|item| {
                let carried = inventory.iter().find(|inv| inv.id == item.item_id && inv.worn == 0 && inv.identified && inv.cards == [0; 4]);
                Offer {
                    item: item.item_id,
                    price: item.price,
                    index: carried.map_or(0, |inv| inv.index),
                    have: carried.map_or(0, |inv| inv.amount),
                    kind: item.kind,
                    identified: true,
                    stock: Some(u32::from(item.amount)),
                    ..Offer::default()
                }
            })
            .collect();
        let mut shop = Self::new(true, offers);
        shop.buying_store = Some((owner, store_id, limit));
        shop
    }

    fn height(&self, window: usize) -> f32 {
        height(self.rows[window])
    }

    /// Selling to an NPC with "Toggle Item Amount." ticked: stacks move whole, without asking.
    fn whole_moves(&self) -> bool {
        self.whole && self.selling && self.plain()
    }

    /// Where the box is, when it shows: selling to an NPC, in the goods' footer.
    fn whole_box(&self) -> Option<[f32; 2]> {
        (self.selling && self.plain()).then(|| [12.0, self.height(0) - WHOLE_UP])
    }

    /// The rows a window lists: offers, or the deal's offers with their amounts.
    fn rows(&self, window: usize) -> Vec<(usize, Option<u32>)> {
        if window == 0 {
            (0..self.offers.len()).map(|offer| (offer, None)).collect()
        } else {
            self.deal.iter().map(|&(offer, amount)| (offer, Some(amount))).collect()
        }
    }

    pub fn total(&self) -> u64 {
        self.deal.iter().map(|&(offer, amount)| u64::from(self.offers[offer].price) * u64::from(amount)).sum()
    }

    fn deal_button(&self, window: usize) -> ([f32; 2], [f32; 2]) {
        let top = self.height(window) - FOOTER + 4.0;
        if window == 1 { ([WIDTH - 62.0 - BUTTON[0], top], [WIDTH - 15.0 - BUTTON[0], top]) } else { ([-100.0; 2], [-100.0; 2]) }
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        for window in [1, 0] {
            let Some(local) = self.frames[window].local(cursor, self.height(window)) else { continue };
            if let Some(hit) = self.frames[window].title_hit(local) {
                return Some(Hit::Title(window, hit));
            }
            if self.resizable() && !self.frames[window].minimized && window::on_grip([self.window_width(window), self.height(window)], local) {
                return Some(Hit::Resize(window));
            }
            let (deal, cancel) = self.deal_button(window);
            if contains(deal, BUTTON, local) {
                return Some(Hit::Deal);
            }
            if contains(cancel, BUTTON, local) {
                return Some(Hit::Cancel);
            }
            if window == 0 && self.whole_box().is_some_and(|at| contains(at, WHOLE_BOX, local)) {
                return Some(Hit::Whole);
            }
            let top = TITLE_HEIGHT;
            if local[1] >= top && local[1] < top + self.rows[window] as f32 * ROW {
                let row = ((local[1] - top) / ROW) as usize + self.scroll[window];
                if row < self.rows(window).len() {
                    return Some(Hit::Row(window, row));
                }
            }
            return Some(Hit::Window(window));
        }
        None
    }

    pub fn click(&mut self, hit: Hit) -> Option<Action> {
        match hit {
            // Closing either window ends the deal.
            Hit::Title(_, TitleHit::Close) => return Some(Action::Close),
            Hit::Title(window, title) => self.frames[window].click_title(title),
            Hit::Row(window, row) => {
                self.selected[window] = Some(row);
                // Rows of both windows share the double click tracker: tell them apart. A plain NPC shop moves items
                // by dragging, and by double clicks only selling with the box ticked.
                if self.clicks.double((window * 1000 + row) as u16) && (!self.plain() || self.whole_moves()) {
                    return self.pick(window, row);
                }
            }
            // An NPC shop's Buy or Sell with nothing in the deal just closes it.
            Hit::Deal if self.deal.is_empty() && self.plain() => return Some(Action::Close),
            Hit::Deal => return (!self.deal.is_empty()).then_some(Action::Deal),
            Hit::Cancel => return Some(Action::Close),
            Hit::Whole => self.whole = !self.whole,
            Hit::Resize(_) | Hit::Window(_) => {}
        }
        None
    }

    /// A double click: an offer goes into the deal (asking how many when there can be more than one), or a deal
    /// row comes out.
    fn pick(&mut self, window: usize, row: usize) -> Option<Action> {
        if window == 1 {
            self.deal.remove(row);
            self.selected[1] = None;
            return None;
        }
        let offer = &self.offers[row];
        if self.buying_store.is_some() {
            return (offer.have > 0).then_some(Action::Ask(row, 0));
        }
        if self.whole_moves() {
            self.add(row, u32::from(offer.have));
            return None;
        }
        let single =
            matches!(offer.kind, ITEM_WEAPON | ITEM_ARMOR | ITEM_PET_EGG | ITEM_PET_ARMOR) || ((self.selling || self.vendor.is_some()) && offer.have == 1);
        if single {
            self.add(row, 1);
            return None;
        }
        Some(Action::Ask(row, 0))
    }

    /// Puts `amount` of an offer in the deal. For a buying store, checks the wanted amount (1739) and the buyer's
    /// zeny limit (1740), returning `Some((message_id, arg))` when refused.
    pub fn add_checked(&mut self, offer: usize, amount: u32) -> Option<(usize, Option<u32>)> {
        if amount == 0 {
            return None;
        }
        if let Some((_, _, zeny_limit)) = self.buying_store {
            if self.offers[offer].have == 0 {
                return None;
            }
            let wanted = self.offers[offer].stock.unwrap_or(0);
            let already = self.deal.iter().find(|(row, _)| *row == offer).map_or(0, |&(_, had)| had);
            if already + amount > wanted {
                return Some((OVER_WANTED_MESSAGE, Some(wanted)));
            }
            let new_amount = (already + amount).min(u32::from(self.offers[offer].have));
            if new_amount == already {
                return None;
            }
            let added_cost = u64::from(new_amount - already) * u64::from(self.offers[offer].price);
            if self.total() + added_cost > u64::from(zeny_limit) {
                return Some((OVER_LIMIT_MESSAGE, None));
            }
            match self.deal.iter_mut().find(|(row, _)| *row == offer) {
                Some((_, had)) => *had = new_amount,
                None => self.deal.push((offer, new_amount)),
            }
            return None;
        }
        let single = matches!(self.offers[offer].kind, ITEM_WEAPON | ITEM_ARMOR | ITEM_PET_EGG | ITEM_PET_ARMOR);
        // Buying from an NPC: one piece of each equipment, up to 30000 of a thing, no more than the zeny carried.
        if self.plain() && !self.selling {
            let already = self.deal.iter().find(|(row, _)| *row == offer).map_or(0, |&(_, had)| had);
            if single && already > 0 {
                return Some((SAME_EQUIPMENT, None));
            }
            let wanted = if single { 1 } else { amount };
            let amount = wanted.min(30_000 - already.min(30_000));
            if self.total() + u64::from(amount) * u64::from(self.offers[offer].price) > self.zeny {
                return Some((NOT_ENOUGH_ZENY, None));
            }
            match self.deal.iter_mut().find(|(row, _)| *row == offer) {
                Some((_, had)) => *had += amount,
                None if amount > 0 => self.deal.push((offer, amount)),
                None => {}
            }
            return (amount < wanted).then_some((OVER_30000, Some(30_000)));
        }
        // Selling, or buying from a player: no more than there are.
        let limit = if self.selling || self.vendor.is_some() {
            u32::from(self.offers[offer].have)
        } else if single {
            1
        } else {
            30_000
        };
        // A market or barter has no more than its stock.
        let limit = self.offers[offer].stock.map_or(limit, |stock| limit.min(stock));
        match self.deal.iter_mut().find(|(row, _)| *row == offer) {
            Some((_, had)) => *had = (*had + amount).min(limit),
            None => self.deal.push((offer, amount.min(limit))),
        }
        None
    }

    /// Puts `amount` of an offer in the deal (selling: no more than carried; equipment: once).
    pub fn add(&mut self, offer: usize, amount: u32) {
        let _ = self.add_checked(offer, amount);
    }

    /// Asks the server for the deal.
    pub fn send(&self, session: &mut MapSession) -> std::io::Result<()> {
        let deal = self.deal.iter().map(|&(offer, amount)| (&self.offers[offer], amount));
        match self.market {
            Some(MarketKind::Market) => return session.market_buy(&deal.map(|(offer, amount)| (offer.item, amount)).collect::<Vec<_>>()),
            Some(MarketKind::Barter) => {
                return session.barter_buy(&deal.map(|(offer, amount)| (offer.item, amount, offer.cost_index, offer.shop_index)).collect::<Vec<_>>());
            }
            Some(MarketKind::Expanded) => {
                return session.expanded_barter_buy(&deal.map(|(offer, amount)| (offer.item, offer.shop_index, amount)).collect::<Vec<_>>());
            }
            None => {}
        }
        if let Some((owner, store_id, _)) = self.buying_store {
            let items: Vec<_> = self.deal.iter().map(|&(offer, amount)| (self.offers[offer].index, self.offers[offer].item, amount as u16)).collect();
            return session.sell_to_buying_store(owner, store_id, &items);
        }
        if let Some((vendor, shop, _)) = &self.vendor {
            let items: Vec<_> = self.deal.iter().map(|&(offer, amount)| (amount as u16, self.offers[offer].index)).collect();
            return session.buy_from_vendor(*vendor, *shop, &items);
        }
        if self.selling {
            session.sell(&self.deal.iter().map(|&(offer, amount)| (self.offers[offer].index, amount as u16)).collect::<Vec<_>>())
        } else {
            session.buy(&self.deal.iter().map(|&(offer, amount)| (self.offers[offer].item, amount as u16)).collect::<Vec<_>>())
        }
    }

    /// After a purchase went through: what was bought leaves the stock, and the deal empties.
    pub fn bought(&mut self, items: &[(u32, u32)]) {
        for &(item, amount) in items {
            if let Some(stock) = self.offers.iter_mut().find(|offer| offer.item == item).and_then(|offer| offer.stock.as_mut()) {
                *stock = stock.saturating_sub(amount);
            }
        }
        self.deal.clear();
        self.selected[1] = None;
    }

    /// What a barter's deal costs, or a row's: "Ore 20 ea, ... money".
    fn costs_text(&self, items: &HashMap<u32, ItemInfo>, messages: &[String], rows: &[(usize, u32)]) -> String {
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        let mut costs: Vec<(u32, u64)> = Vec::new();
        let mut zeny = 0u64;
        for &(offer, amount) in rows {
            let offer = &self.offers[offer];
            zeny += u64::from(offer.price) * u64::from(amount);
            for &(item, each) in &offer.costs {
                match costs.iter_mut().find(|(cost, _)| *cost == item) {
                    Some((_, total)) => *total += u64::from(each) * u64::from(amount),
                    None => costs.push((item, u64::from(each) * u64::from(amount))),
                }
            }
        }
        let mut parts: Vec<String> = costs
            .into_iter()
            .map(|(item, total)| rustnarok_client::messages::fill(message(COSTS_MESSAGE), &[&inventory::item_name(items, item, true, 0), &total]))
            .collect();
        if zeny > 0 {
            parts.push(format!("{} {}", group(zeny), message(MONEY_MESSAGE)));
        }
        parts.join(", ")
    }

    /// Whether the pointer is on the shop's windows.
    pub fn covers(&self, cursor: [f32; 2]) -> bool {
        self.hit(cursor).is_some()
    }

    pub fn scroll_by(&mut self, cursor: [f32; 2], rows: i32) {
        for (window, frame) in self.frames.iter().enumerate() {
            if frame.local(cursor, self.height(window)).is_some() {
                let most = self.rows(window).len().saturating_sub(self.rows[window]);
                // As the inventory: up to 3 rows a notch, one less than it shows.
                let step = self.rows[window].saturating_sub(1).clamp(1, 3) as i32;
                self.scroll[window] = (self.scroll[window] as i32 + rows * step).clamp(0, most as i32) as usize;
            }
        }
    }

    pub fn icons(&self, items: &HashMap<u32, ItemInfo>) -> Vec<String> {
        self.offers.iter().filter_map(|offer| items.get(&offer.item).map(|info| inventory::icon_path(info, offer.identified))).collect()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &self,
        layer: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        icons: &'a HashMap<String, Option<Texture>>,
        items: &HashMap<u32, ItemInfo>,
        messages: &[String],
        font: &Font,
        pointer: ([f32; 2], bool),
    ) {
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        let hit = self.hit(pointer.0);
        let look = |target: Hit| {
            let hovered = hit == Some(target);
            usize::from(hovered) + usize::from(hovered && pointer.1)
        };
        for (window, titles) in TITLES.iter().enumerate() {
            let frame = &self.frames[window];
            if !frame.open {
                continue;
            }
            let barter = matches!(self.market, Some(MarketKind::Barter | MarketKind::Expanded));
            let title = match &self.vendor {
                Some((.., name)) if window == 0 => name.as_str(),
                _ if self.buying_store.is_some() && window == 0 => message(WANTED_TITLE),
                _ if barter && window == 1 => message(EXCHANGE_TITLE),
                _ => message(titles[usize::from(self.selling)]),
            };
            frame.draw_title(layer, chrome, title, pointer.0);
            if frame.minimized {
                continue;
            }
            let height = self.height(window);
            let width = self.window_width(window);
            let list_height = height - TITLE_HEIGHT - FOOTER;
            layer.sprites.push((&chrome.white, frame.at([0.0, TITLE_HEIGHT]), [width, list_height]));
            layer.sprites.push((&assets.strip, frame.at([0.0, TITLE_HEIGHT]), [assets.strip.size[0], list_height]));
            layer.sprites.push((&chrome.border, frame.at([width - 1.0, TITLE_HEIGHT]), [1.0, list_height]));
            for (place, (offer_index, amount)) in self.rows(window).into_iter().enumerate().skip(self.scroll[window]).take(self.rows[window]) {
                let top = TITLE_HEIGHT + (place - self.scroll[window]) as f32 * ROW;
                let offer = &self.offers[offer_index];
                if self.selected[window] == Some(place) {
                    let [x, y] = SELECTED_BOX;
                    layer.sprites.push((&assets.selected, frame.at([x, top + y]), [width - x - 17.0, SELECTED_HEIGHT]));
                }
                // A market's goods sit further left, the name wider.
                let market_goods = window == 0 && self.market == Some(MarketKind::Market);
                let (icon_x, name_x) = if market_goods { (MARKET_ICON, MARKET_NAME) } else { (ICON[0], NAME[0]) };
                let icon = items.get(&offer.item).and_then(|info| icons.get(&inventory::icon_path(info, offer.identified))).and_then(Option::as_ref);
                if let Some(icon) = icon {
                    layer.sprites.push((icon, frame.at([icon_x, top + ICON[1]]), icon.size));
                }
                if let Some(count) = self.icon_count(window, offer, amount) {
                    let text = count.to_string();
                    // From 1000 up the number starts at the icon's left.
                    let place = frame.at([icon_x + if count < 1000 { 14.0 } else { 0.0 }, top + ICON[1] + 14.0]);
                    layer.texts.push((text.clone(), offset(place, [-1.0, -1.0]), [255; 3]));
                    layer.texts.push((text, place, [0, 0, 0]));
                }
                let name = inventory::item_name(items, offer.item, offer.identified, offer.refine);
                // ponytail: a barter's costs go on a second line, in grey; where the real client shows them isn't known.
                if barter {
                    layer.texts.push((window::fit(font, &name, width - LEFT - 40.0), frame.at([LEFT + 32.0, top + 3.0]), [0, 0, 0]));
                    let costs = self.costs_text(items, messages, &[(offer_index, amount.unwrap_or(1))]);
                    layer.texts.push((window::fit(font, &costs, width - LEFT - 40.0), frame.at([LEFT + 32.0, top + 17.0]), GREY));
                    continue;
                }
                let (price, name_width) = price_text(offer.listed, offer.price);
                let name_width = if market_goods { MARKET_NAME_WIDTH } else { name_width };
                layer.texts.push((window::fit(font, &name, name_width), frame.at([name_x, top + NAME[1]]), [0, 0, 0]));
                layer.texts.push((price.clone(), frame.at([width - PRICE_RIGHT - font.width(&price), top + NAME[1]]), [0, 0, 0]));
            }
            // Footer: the deal's total and buttons.
            let footer = height - FOOTER;
            layer.sprites.push((&assets.bar, frame.at([0.0, footer]), [width, FOOTER]));
            if self.resizable() {
                layer.sprites.push((&assets.resize, frame.at([width - window::GRIP, height - window::GRIP]), assets.resize.size));
            }
            if let (Some(at), 0) = (self.whole_box(), window) {
                let check = &assets.check[usize::from(self.whole)];
                layer.sprites.push((check, frame.at(at), check.size));
                layer.texts.push((message(WHOLE_MESSAGE).to_owned(), frame.at([at[0] + check.size[0] + 4.0, at[1] - 1.0]), [0, 0, 0]));
            }
            if let (Some((_, _, limit)), 0) = (self.buying_store, window) {
                let text = rustnarok_client::messages::fill(message(LIMIT_MESSAGE), &[&group(u64::from(limit))]);
                layer.texts.push((window::fit(font, &text, width - 20.0), frame.at([10.0, footer + 7.0]), [0, 0, 0]));
            }
            if window == 1 {
                let total = if barter {
                    window::fit(font, &self.costs_text(items, messages, &self.deal.iter().map(|&(offer, amount)| (offer, amount)).collect::<Vec<_>>()), 150.0)
                } else if self.market.is_some() {
                    rustnarok_client::messages::fill(message(TOTAL_MESSAGE), &[&group(self.total())])
                } else {
                    format!("Total : {} Zeny", group(self.total()))
                };
                layer.texts.push((total, frame.at([12.0, height - 21.0]), [0, 0, 0]));
                let (deal, cancel) = self.deal_button(1);
                let deal_images = &assets.buttons[if barter { 3 } else { usize::from(self.selling) }];
                layer.sprites.push((&deal_images[look(Hit::Deal)], frame.at(deal), BUTTON));
                layer.sprites.push((&assets.buttons[2][look(Hit::Cancel)], frame.at(cancel), BUTTON));
            }
        }
        // An NPC shop's row under the pointer: its name and price over it.
        if let (Some(Hit::Row(window, row)), true) = (hit, self.plain()) {
            let (Some(offer), frame) = (self.offer_at(window, row), &self.frames[window]) else { return };
            let count = if window == 0 { u32::from(offer.have) } else { self.deal[row].1 };
            let item = InventoryItem { id: offer.item, kind: offer.kind, identified: offer.identified, refine: offer.refine, ..Default::default() };
            let text = tooltip_text(&inventory::item_name(items, offer.item, offer.identified, offer.refine), offer, count, self.selling);
            let local = pointer.0[0] - frame.position[0];
            let x = TIP_STEP[0] + TIP_STEP[1] * ((local - TIP_STEP[0]) / TIP_STEP[1]).floor();
            let top = TITLE_HEIGHT + (row - self.scroll[window]) as f32 * ROW - TIP_ABOVE;
            window::tooltip(layer.window(), chrome, font, &text, frame.at([x, top]), inventory::name_color(&item, items));
        }
    }

    /// The number on a row's icon: how many are in the deal, carried (selling, a vendor's shop) or left (a market or
    /// buying store); none for 0.
    fn icon_count(&self, window: usize, offer: &Offer, amount: Option<u32>) -> Option<u32> {
        let count = amount.or(if window != 0 {
            None
        } else if self.buying_store.is_some() || self.market == Some(MarketKind::Market) {
            offer.stock
        } else if self.selling || self.vendor.is_some() {
            Some(u32::from(offer.have))
        } else {
            None
        });
        count.filter(|&count| count > 0)
    }

    pub fn button_boxes(&self) -> Vec<(&'static str, [f32; 4])> {
        let (deal, cancel) = self.deal_button(1);
        let [deal, cancel] = [self.frames[1].at(deal), self.frames[1].at(cancel)];
        let mut boxes = vec![("shop_deal", [deal[0], deal[1], BUTTON[0], BUTTON[1]]), ("shop_cancel", [cancel[0], cancel[1], BUTTON[0], BUTTON[1]])];
        if let Some([x, y]) = self.whole_box().map(|at| self.frames[0].at(at)) {
            boxes.push(("shop_whole", [x, y, WHOLE_BOX[0], WHOLE_BOX[1]]));
        }
        if self.resizable() {
            for (window, name) in ["shop_resize_0", "shop_resize_1"].into_iter().enumerate() {
                let [x, y] = self.frames[window].at([self.window_width(window) - window::GRIP, self.height(window) - window::GRIP]);
                boxes.push((name, [x, y, window::GRIP, window::GRIP]));
            }
        }
        boxes
    }

    pub fn row_boxes(&self) -> Vec<(u32, [f32; 4])> {
        self.rows(0)
            .into_iter()
            .enumerate()
            .skip(self.scroll[0])
            .take(self.rows[0])
            .map(|(place, (offer, _))| {
                let top = self.frames[0].at([0.0, TITLE_HEIGHT + (place - self.scroll[0]) as f32 * ROW]);
                (self.offers[offer].item, [top[0], top[1], self.width, ROW])
            })
            .collect()
    }
}

/// An NPC shop row's tooltip: buying "name 1,200 Zeny" or "name 1500 -> 1,200 Zeny"; selling the count carried (or
/// in the deal) after the name, "name 5 ea 1,200 Zeny".
// ponytail: the spacing follows the random-option forms (2758–2761); those forms aren't used, offers carry no options.
fn tooltip_text(name: &str, offer: &Offer, count: u32, selling: bool) -> String {
    let price = group(u64::from(offer.price));
    let price = if offer.listed == 0 || offer.listed == offer.price { price } else { format!("{} -> {price}", offer.listed) };
    if selling { format!("{name} {count} ea {price} Zeny") } else { format!("{name} {price} Zeny") }
}

/// A row's unit price, " 1,234 Z", or with a discount or overcharge " 1500 -> 1,200 Z" (the original not grouped); and
/// how wide its name may be: 100 px, narrower for a longer original price.
fn price_text(listed: u32, price: u32) -> (String, f32) {
    if listed == 0 || listed == price {
        return (format!(" {} Z", group(u64::from(price))), 100.0);
    }
    let width = match listed.to_string().len() {
        0..=2 => 80.0,
        3 => 70.0,
        4 => 55.0,
        _ => 45.0,
    };
    (format!(" {listed} -> {} Z", group(u64::from(price))), width)
}

/// A number with thousands separated by commas, as the client shows prices.
pub fn group(number: u64) -> String {
    let digits = number.to_string();
    let mut grouped = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    grouped
}

/// "Confirm Deal" (2326): Buy on a vendor's shop closes it and its deal waits here, the total shown, for Buy (0x0801)
/// or Cancel; either closes it.
pub struct Confirm {
    pub shop: Shop,
    frame: Frame,
}

const CONFIRM_SIZE: [f32; 2] = [300.0, 150.0];
const CONFIRM_TITLE: usize = 2326;
/// "Below is the total cost:", "<total> Zeny", "Press buy to confirm." at (10, 20 + 20k).
const CONFIRM_LINES: [usize; 3] = [2327, 2328, 2329];
/// Buy and Cancel.
const CONFIRM_BUTTONS: [[f32; 2]; 2] = [[194.0, 126.0], [239.0, 126.0]];

impl Confirm {
    // ponytail: in the middle of the screen; where the game puts it isn't known.
    pub fn new(shop: Shop, screen: [f32; 2]) -> Self {
        let mut frame = Frame::new([((screen[0] - CONFIRM_SIZE[0]) / 2.0).floor(), ((screen[1] - CONFIRM_SIZE[1]) / 2.0).floor()], CONFIRM_SIZE[0]);
        frame.open = true;
        Self { shop, frame }
    }

    /// Over the box: Some(true) on Buy, Some(false) on Cancel.
    pub fn hit(&self, cursor: [f32; 2]) -> Option<Option<bool>> {
        let local = self.frame.local(cursor, CONFIRM_SIZE[1])?;
        Some(CONFIRM_BUTTONS.iter().position(|&button| contains(button, BUTTON, local)).map(|button| button == 0))
    }

    pub fn lines(&self, messages: &[String]) -> [String; 3] {
        let message = |id: usize| messages.get(id).cloned().unwrap_or_default();
        let [cost, zeny, press] = CONFIRM_LINES;
        [message(cost), format!("{} {}", group(self.shop.total()), message(zeny)), message(press)]
    }

    /// Buy and Cancel on screen (for the test driver).
    pub fn button_boxes(&self) -> Vec<(&'static str, [f32; 4])> {
        ["confirm_buy", "confirm_cancel"]
            .into_iter()
            .zip(CONFIRM_BUTTONS)
            .map(|(name, place)| {
                let [x, y] = self.frame.at(place);
                (name, [x, y, BUTTON[0], BUTTON[1]])
            })
            .collect()
    }

    pub fn draw<'a>(&self, layer: &mut Layer<'a>, chrome: &'a Chrome, assets: &'a Assets, messages: &[String], pointer: ([f32; 2], bool)) {
        let frame = &self.frame;
        frame.draw_bar(layer, chrome);
        let title = messages.get(CONFIRM_TITLE).cloned().unwrap_or_default();
        layer.texts.push((title.clone(), frame.at([17.0, 3.0]), [255; 3]));
        layer.texts.push((title, frame.at([16.0, 2.0]), [0, 0, 0]));
        let [width, height] = CONFIRM_SIZE;
        layer.sprites.push((&chrome.white, frame.at([0.0, TITLE_HEIGHT]), [width, height - TITLE_HEIGHT - FOOTER]));
        layer.sprites.push((&assets.bar, frame.at([0.0, height - FOOTER]), [width, FOOTER]));
        for (row, line) in self.lines(messages).into_iter().enumerate() {
            layer.texts.push((line, frame.at([10.0, 20.0 + 20.0 * row as f32]), [0, 0, 0]));
        }
        let hit = self.hit(pointer.0).flatten();
        for (images, place, buy) in [(&assets.buttons[0], CONFIRM_BUTTONS[0], true), (&assets.buttons[2], CONFIRM_BUTTONS[1], false)] {
            let look = if hit == Some(buy) { 1 + usize::from(pointer.1) } else { 0 };
            layer.sprites.push((&images[look], frame.at(place), BUTTON));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resizes_by_the_corner_grips() {
        // 232 to 320 asked, toward 280: 248, 280 or 312.
        assert_eq!([100.0, 248.0, 249.0, 311.0, 312.0, 400.0].map(snap_width), [248.0, 248.0, 280.0, 280.0, 312.0, 312.0]);
        let mut shop = Shop::new(false, vec![offer(501, 50, 0, 0)]);
        let grip = |shop: &Shop, window: usize| {
            let [x, y, w, h] = shop.button_boxes().into_iter().find(|(name, _)| *name == ["shop_resize_0", "shop_resize_1"][window]).unwrap().1;
            [x + w / 2.0, y + h / 2.0]
        };
        // The goods: both ways.
        let corner = grip(&shop, 0);
        assert_eq!(shop.hit(corner), Some(Hit::Resize(0)));
        assert!(shop.start_resize(corner));
        shop.drag_to([corner[0] - 40.0, corner[1] - 6.0 * ROW]);
        assert_eq!(shop.sizes(), Some((248.0, [2, 3])));
        assert!(shop.stop_resize());
        // The deal: only its height, up to 8 rows.
        let corner = grip(&shop, 1);
        assert!(shop.start_resize(corner));
        shop.drag_to([corner[0] + 100.0, corner[1] + 20.0 * ROW]);
        assert_eq!(shop.sizes(), Some((248.0, [2, MOST_ROWS])));
        shop.stop_resize();
        // A player's shop has no grips and keeps nothing.
        let mut vendor = Shop::new(false, vec![offer(501, 50, 0, 0)]);
        vendor.vendor = Some((1, 2, "Shop".into()));
        vendor.apply_sizes((312.0, [8, 8]));
        assert_eq!((vendor.sizes(), vendor.rows), (None, ROWS));
        assert!(!vendor.button_boxes().iter().any(|(name, _)| name.starts_with("shop_resize")));
    }

    fn offer(item: u32, price: u32, have: u16, kind: u8) -> Offer {
        Offer { item, price, index: 2, have, kind, identified: true, ..Offer::default() }
    }

    #[test]
    fn a_vendors_deal_waits_for_confirm_deal() {
        let mut shop = Shop::new(false, vec![offer(501, 2_500_000, 3, 0)]);
        shop.vendor = Some((1, 2, "Shop".into()));
        shop.add(0, 2);
        let confirm = Confirm::new(shop, [1024.0, 768.0]);
        let messages: Vec<String> = (0..=2329).map(|id| format!("m{id}")).collect();
        assert_eq!(confirm.lines(&messages), ["m2327", "5,000,000 m2328", "m2329"]);
        let [(_, buy), (_, cancel)] = confirm.button_boxes()[..] else { panic!() };
        assert_eq!(buy, [556.0, 435.0, 42.0, 20.0]);
        assert_eq!(confirm.hit([buy[0] + 1.0, buy[1] + 1.0]), Some(Some(true)));
        assert_eq!(confirm.hit([cancel[0] + 1.0, cancel[1] + 1.0]), Some(Some(false)));
        assert_eq!(confirm.hit([buy[0] - 100.0, buy[1]]), Some(None));
        assert_eq!(confirm.hit([0.0, 0.0]), None);
    }

    #[test]
    fn makes_deals() {
        let mut shop = Shop::new(false, vec![offer(501, 50, 0, 0), offer(1201, 100, 0, ITEM_WEAPON)]);
        shop.add(0, 3);
        shop.add(0, 2);
        shop.add(1, 5);
        // Five potions; one knife however many are asked.
        assert_eq!(shop.deal, [(0, 5), (1, 1)]);
        assert_eq!(shop.total(), 350);
        let mut selling = Shop::new(true, vec![offer(909, 3, 4, 3)]);
        selling.add(0, 10);
        assert_eq!(selling.deal, [(0, 4)]);
        assert_eq!(group(1_234_567), "1,234,567");
        // A market takes no more than its stock, and what's bought leaves it.
        let mut market = Shop::new(false, vec![Offer { stock: Some(3), ..offer(1010, 200, 0, 3) }]);
        market.market = Some(MarketKind::Market);
        market.add(0, 5);
        assert_eq!(market.deal, [(0, 3)]);
        market.bought(&[(1010, 2)]);
        assert_eq!((market.offers[0].stock, market.deal.is_empty()), (Some(1), true));
        // What's left shows on the goods' icon, the amount on the deal's, and nothing once it's sold out.
        let left = market.offers[0].clone();
        assert_eq!((market.icon_count(0, &left, None), market.icon_count(1, &left, Some(1))), (Some(1), Some(1)));
        assert_eq!(market.icon_count(0, &Offer { stock: Some(0), ..left }, None), None);
    }

    #[test]
    fn shows_unit_and_discounted_prices() {
        assert_eq!(price_text(0, 1200), (" 1,200 Z".to_owned(), 100.0));
        assert_eq!(price_text(1200, 1200), (" 1,200 Z".to_owned(), 100.0));
        assert_eq!(price_text(1500, 1200), (" 1500 -> 1,200 Z".to_owned(), 55.0));
        assert_eq!(price_text(40, 30).1, 80.0);
        // An NPC shop's Buy with nothing in the deal closes it.
        let mut shop = Shop::new(false, vec![offer(501, 50, 0, 0)]);
        assert_eq!(shop.click(Hit::Deal), Some(Action::Close));
    }

    #[test]
    fn drags_in_and_out_of_the_deal() {
        let mut shop = Shop::new(false, vec![offer(501, 50, 0, 0), offer(1201, 100, 0, ITEM_WEAPON)]);
        // Buying: a potion asks how many (1 in the box); a knife goes in at once, and a second one is refused.
        assert_eq!(shop.drop_offer(0), Some(Action::Ask(0, 1)));
        assert_eq!(shop.drop_offer(1), None);
        assert_eq!(shop.drop_offer(1), Some(Action::Say(SAME_EQUIPMENT, None)));
        assert_eq!(shop.deal, [(1, 1)]);
        // Not more than the zeny carried; past 30000 of a thing it's cut, and says so.
        shop.zeny = 1_000;
        assert_eq!(shop.add_checked(0, 20), Some((NOT_ENOUGH_ZENY, None)));
        shop.zeny = u64::MAX;
        assert_eq!(shop.add_checked(0, 29_999), None);
        assert_eq!(shop.add_checked(0, 5), Some((OVER_30000, Some(30_000))));
        assert_eq!(shop.deal, [(1, 1), (0, 30_000)]);
        // Back out: one at once, more asks with the whole row in the box.
        assert_eq!(shop.drop_deal(0), None);
        assert_eq!(shop.drop_deal(0), Some(Action::AskOut(0, 30_000)));
        shop.take_out(0, 10_000);
        assert_eq!(shop.deal, [(0, 20_000)]);
        // Selling: the box holds the whole stack.
        let mut selling = Shop::new(true, vec![offer(909, 3, 40, 3)]);
        assert_eq!(selling.drop_offer(0), Some(Action::Ask(0, 40)));
    }

    #[test]
    fn says_name_and_price_over_a_row() {
        let plain = offer(501, 1200, 5, 0);
        assert_eq!(tooltip_text("Red Potion", &plain, 5, false), "Red Potion 1,200 Zeny");
        let discounted = Offer { listed: 1500, ..plain.clone() };
        assert_eq!(tooltip_text("Red Potion", &discounted, 5, false), "Red Potion 1500 -> 1,200 Zeny");
        assert_eq!(tooltip_text("Red Potion", &plain, 5, true), "Red Potion 5 ea 1,200 Zeny");
    }

    #[test]
    fn lays_rows_out_from_the_title() {
        let mut shop = Shop::new(false, (0..10).map(|n| offer(501 + n, 50, 0, 0)).collect());
        let [x, y] = shop.frames[0].position;
        // Row r is 32 px from y 17: the band's first and last pixel.
        assert_eq!(shop.hit([x + 100.0, y + 17.0]), Some(Hit::Row(0, 0)));
        assert_eq!(shop.hit([x + 100.0, y + 48.9]), Some(Hit::Row(0, 0)));
        assert_eq!(shop.hit([x + 100.0, y + 49.0]), Some(Hit::Row(0, 1)));
        assert_eq!(shop.row_boxes()[1].1[1], y + 49.0);
        // The wheel: 3 rows a notch with 8 showing, 1 with 2.
        shop.scroll_by([x + 100.0, y + 50.0], 1);
        assert_eq!(shop.scroll[0], 2, "only 2 more than it shows");
        shop.set_size(0, None, 2);
        shop.scroll_by([x + 100.0, y + 50.0], -1);
        assert_eq!(shop.scroll[0], 1);
    }

    #[test]
    fn moves_whole_stacks_with_the_box_ticked() {
        let mut shop = Shop::new(true, vec![offer(909, 3, 40, 3), offer(910, 5, 7, 3)]);
        let whole = |shop: &Shop| shop.button_boxes().into_iter().find(|(name, _)| *name == "shop_whole").map(|(_, [x, y, ..])| [x + 1.0, y + 1.0]);
        // Off at start: a drop asks, a double click does nothing.
        assert_eq!(shop.drop_offer(0), Some(Action::Ask(0, 40)));
        let corner = whole(&shop).unwrap();
        assert_eq!(shop.hit(corner), Some(Hit::Whole));
        shop.click(Hit::Whole);
        // Ticked: the stack goes in whole, by a drop or a double click, and comes back out whole.
        assert_eq!(shop.drop_offer(0), None);
        assert_eq!(shop.deal, [(0, 40)]);
        assert_eq!(shop.drop_deal(0), None);
        assert!(shop.deal.is_empty());
        shop.click(Hit::Row(0, 1));
        shop.click(Hit::Row(0, 1));
        assert_eq!(shop.deal, [(1, 7)]);
        // Buying has no box, and a double click does nothing.
        let mut buying = Shop::new(false, vec![offer(501, 50, 0, 0)]);
        assert!(whole(&buying).is_none());
        buying.click(Hit::Row(0, 0));
        assert_eq!(buying.click(Hit::Row(0, 0)), None);
        assert!(buying.deal.is_empty());
    }

    #[test]
    fn sells_into_buying_store() {
        let wanted = [BuyingItem { price: 10, amount: 0, kind: 3, item_id: 501 }, BuyingItem { price: 300, amount: 5, kind: 3, item_id: 909 }];
        let inv = [
            // Worn or carded stacks are ignored.
            InventoryItem { index: 2, id: 909, amount: 5, identified: true, worn: 1, ..Default::default() },
            InventoryItem { index: 3, id: 909, amount: 5, identified: true, cards: [4001, 0, 0, 0], ..Default::default() },
            InventoryItem { index: 4, id: 909, amount: 5, identified: true, ..Default::default() },
        ];
        let mut shop = Shop::new_buying_store(2000001, 42, 1000, &wanted, &inv);
        assert_eq!(shop.offers.len(), 1);
        assert_eq!((shop.offers[0].item, shop.offers[0].index, shop.offers[0].have, shop.offers[0].stock), (909, 4, 5, Some(5)));

        // Over wanted amount (6 > 5) -> 1739 with 5.
        assert_eq!(shop.add_checked(0, 6), Some((1739, Some(5))));
        assert!(shop.deal.is_empty());

        // Within wanted amount (4 <= 5), but 4 * 300 = 1200 > 1000 zeny limit -> 1740.
        assert_eq!(shop.add_checked(0, 4), Some((1740, None)));
        assert!(shop.deal.is_empty());

        // 3 * 300 = 900 <= 1000 zeny limit.
        assert_eq!(shop.add_checked(0, 3), None);
        assert_eq!(shop.deal, [(0, 3)]);
        assert_eq!(shop.total(), 900);
    }
}
