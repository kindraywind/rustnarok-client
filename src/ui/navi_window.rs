//! The Navigation window: a search over the navigation tables, what it found in two lists (rows of a name on the left,
//! the places of the picked row on the right), and Find, which starts a navigation to the place picked. While one runs
//! it shows the route view instead (the map's picture with the trail on it), and it has a small form that says where
//! the navigation heads.

use rustnarok_client::grf::Grf;
use rustnarok_client::lua::NaviTables;
use rustnarok_client::text::Font;

use crate::navigation::{self, Found, Target};
use crate::render::{Gpu, Texture};
use crate::window::{Chrome, Frame, TITLE_HEIGHT, TitleHit};
use crate::{Layer, UI, contains, screen};

pub const SIZE: [f32; 2] = [272.0, 338.0];
/// "Navigation"; "Search result[%d]"; "Failed to set info for location!" (Find with nothing picked).
const TITLE: usize = 2221;
const FOUND: usize = 3210;
pub const NOTHING_PICKED: usize = 2297;
/// The type box and its four lines: "All", "Map", "Npc", "Mob".
const FILTER: [f32; 4] = [29.0, 21.0, 61.0, 18.0];
const FILTERS: [usize; 4] = [2206, 2207, 2208, 2209];
/// The search box (30 characters at most) and the Search button beside it.
const FIELD: [f32; 4] = [94.0, 21.0, 113.0, 18.0];
const MOST: usize = 30;
const SEARCH: [f32; 4] = [207.0, 21.0, 25.0, 18.0];
const FOUND_AT: [f32; 2] = [20.0, 48.0];
/// The two lists, in rows of 16: fifteen on the left, five on the right.
const ROW: f32 = 16.0;
const GROUPS: [f32; 4] = [1.0, 70.0, 138.0, 240.0];
const MEMBERS: [f32; 4] = [139.0, 166.0, 132.0, 80.0];
/// A list's scrollbar: this wide, at its right edge.
const BAR: f32 = 7.0;
/// The picked place's name and its map's picture.
const NAME_AT: [f32; 2] = [141.0, 46.0];
const PICTURE: [f32; 4] = [155.0, 60.0, 100.0, 100.0];
/// The "service" box (its label to the right), and Find.
const SERVICE: [f32; 4] = [146.0, 247.0, 110.0, 12.0];
const FIND: [f32; 4] = [207.0, 263.0, 58.0, 20.0];
const BOTTOM: [f32; 2] = [0.0, 314.0];
/// The button that turns between the search and the route view, and the "Minimize" box: ticked, a click on the
/// ground makes the window small.
const BACK: [f32; 4] = [4.0, 21.0, 22.0, 18.0];
const AUTO: [f32; 4] = [9.0, 321.0, 110.0, 12.0];
/// The route view: a grey square, the map's picture in it, and its three lines; "Destination".
const SQUARE: [f32; 4] = [1.0, 44.0, 270.0, 270.0];
const MAP: [f32; 4] = [3.0, 46.0, 266.0, 266.0];
const ROUTE_LINES: [[f32; 2]; 3] = [[10.0, 48.0], [10.0, 68.0], [10.0, 296.0]];
const DESTINATION: usize = 3585;
/// The small form: its size, its black box, its two lines, and its maximize and close buttons from the right edge.
pub const MINI: [f32; 2] = [134.0, 61.0];
const MINI_BOX: [f32; 4] = [1.0, 17.0, 132.0, 43.0];
const MINI_LINES: [[f32; 2]; 2] = [[5.0, 21.0], [5.0, 40.0]];
const MINI_BUTTON: [f32; 2] = [14.0, 14.0];
const MINI_MAXIMIZE: f32 = 32.0;
const MINI_CLOSE: f32 = 15.0;
/// How see-through the small form is (alpha 170).
const MINI_FADE: f32 = 1.0 - 170.0 / 255.0;
const WHITE: [u8; 3] = [255, 255, 255];
/// The title's help button, and the bottom bar's two: the route list and the arrow sets.
const HELP_BUTTON: [f32; 4] = [230.0, 3.0, 11.0, 11.0];
const LIST_BUTTON: [f32; 4] = [203.0, 317.0, 26.0, 18.0];
const SETS_BUTTON: [f32; 4] = [237.0, 317.0, 26.0, 18.0];
/// The help window ("How to use the Navigation"): where it opens, its text box, the messages in it and a line's height.
const HELP_SIZE: [f32; 2] = [310.0, 200.0];
const HELP_AT: [f32; 2] = [100.0, 100.0];
const HELP_TITLE: usize = 2240;
const HELP_BOX: [f32; 4] = [5.0, 22.0, 300.0, 173.0];
const HELP_TEXT: [usize; 19] = [2241, 2242, 2243, 2244, 2245, 2246, 2247, 2248, 2249, 2250, 2251, 2252, 2253, 2254, 2255, 2256, 3216, 3217, 3218];
const HELP_LINE: f32 = 14.0;
/// The arrow sets' window ("Navigation Icons"): where it opens from the main window's corner, and the eight sets'
/// boxes on its picture, two rows of four.
const SETS_SIZE: [f32; 2] = [158.0, 102.0];
const SETS_FROM: [f32; 2] = [113.0, 211.0];
const SETS_TITLE: usize = 3203;
const SET_COLUMNS: [f32; 4] = [11.0, 47.0, 83.0, 119.0];
const SET_ROWS: [[f32; 2]; 2] = [[25.0, 55.0], [63.0, 91.0]];
/// The route list ("Navigation Info"), at the main window's right edge: its list of rows.
const LIST_SIZE: [f32; 2] = [150.0, 338.0];
const LIST_TITLE: usize = 3204;
const LIST_BOX: [f32; 4] = [2.0, 20.0, 146.0, 293.0];
const GREY: [u8; 3] = [0xCC, 0xCC, 0xCC];
const INK: [u8; 3] = [0, 0, 0];
const PICKED: [u8; 3] = [0x30, 0x50, 0xC0];

pub struct Assets {
    /// The type box and its arrow; the search box; the Search button and Find (normal, under the pointer).
    filter: [Texture; 2],
    field: Texture,
    search: [Texture; 2],
    find: [Texture; 2],
    /// A row's icon: a map, an NPC, a monster.
    icons: [Texture; 3],
    check: [Texture; 2],
    bottom: Texture,
    grey: Texture,
    black: Texture,
    picked: Texture,
    /// The view button (normal, under the pointer); the small form's maximize and close.
    back: [Texture; 2],
    maximize: [Texture; 2],
    close: [Texture; 2],
    /// On the route view's map: a trail dot, where the trail ends, where the player is.
    dot: Texture,
    destination: Texture,
    location: Texture,
    /// The help button (off, on); the route list's button and each arrow set's (normal, under the pointer).
    help: [Texture; 2],
    list_button: [Texture; 2],
    set_buttons: Vec<[Texture; 2]>,
    /// The arrow sets' picture and the frame round the one in use; the help's box; the route list's row icons (the
    /// first way, a way on, the target).
    sets: Texture,
    set_frame: Texture,
    help_box: Texture,
    steps: [Texture; 3],
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |name: &str| gpu.load_texture(grf, &format!("{UI}navigation_interface3\\{name}.bmp"), false);
        Self {
            filter: ["bg_combo", "dropdown_on"].map(image),
            field: image("bg_search"),
            search: ["btn_searchbar_normal", "btn_searchbar_over"].map(image),
            find: ["btn_find_normal", "btn_find_over"].map(|name| gpu.load_texture(grf, &format!("{UI}private_airplane\\{name}.bmp"), false)),
            icons: ["icon_list_map", "icon_list_npc", "icon_list_mob"].map(image),
            check: ["checkbox_0", "checkbox_1"].map(|name| gpu.load_texture(grf, &format!("{UI}{name}.bmp"), false)),
            bottom: image("bg_down"),
            grey: gpu.solid([GREY[0], GREY[1], GREY[2], 0xFF]),
            black: gpu.solid([0, 0, 0, 0xFF]),
            picked: gpu.solid([0xD8, 0xE0, 0xF8, 0xFF]),
            back: ["bt_back", "bt_back_on"].map(image),
            maximize: ["btn_maximize_normal", "btn_maximize_over"].map(image),
            close: ["btn_close_normal", "btn_close_over"].map(image),
            dot: gpu.solid([0x0A, 0xF0, 0xF0, 0xFF]),
            destination: image("ico_destination"),
            location: image("ico_location"),
            help: ["NaviHelp_off", "NaviHelp_on"].map(|name| gpu.load_texture(grf, &format!("{UI}navigation_interface\\{name}.bmp"), false)),
            list_button: ["btn_roadlistwnd_normal", "btn_roadlistwnd_over"].map(image),
            set_buttons: (1..=8).map(|set| ["normal", "over"].map(|state| image(&format!("btn_roadIocn_select{set}_{state}")))).collect(),
            sets: image("bg_roadIocn_select"),
            set_frame: image("img_roadIocn_bg_select"),
            help_box: gpu.solid([0xD2, 0xD2, 0xD2, 0xFF]),
            steps: ["icon_list_Start", "icon_list_Riute", "icon_list_Target"].map(image),
        }
    }
}

/// A map's picture for the window: the map's own, else the one for insides.
// ponytail: the game picks by the map's type in the tables; here it's whichever picture the data has.
pub fn picture(gpu: &Gpu, grf: &Grf, map: &str) -> Option<Texture> {
    let own = gpu.try_load_texture(grf, &format!("{UI}map\\{map}.bmp"), true);
    own.or_else(|_| gpu.try_load_texture(grf, &format!("{UI}navigation_interface\\Interior.bmp"), true)).ok()
}

/// What the window shows of a navigation under way: the map the player is on (as the tables name it) and its size in
/// cells, the player's cell, the trail on this map, the cell headed for on it, and the target.
pub struct Guide<'a> {
    pub map: &'a str,
    pub size: [usize; 2],
    pub player: [i32; 2],
    pub trail: &'a [[i32; 2]],
    pub next: Option<[u16; 2]>,
    pub target: &'a Target,
    /// The route's ways from this map on: each its map and cell.
    pub steps: &'a [(String, [u16; 2])],
}

/// The route list's rows: each way of the route ("map (x y)"; the first marked as the start), then the target ("map
/// (x y)", or the map alone for a map or a monster). The mark: 0 the start, 1 a way on, 2 the target.
pub fn route_rows(guide: &Guide) -> Vec<(usize, String)> {
    let ways = guide.steps.iter().enumerate().map(|(index, (map, [x, y]))| (index.min(1), format!("{map} ({x} {y})")));
    let target = guide.target.cell.map_or(guide.target.map.clone(), |[x, y]| format!("{} ({x} {y})", guide.target.map));
    ways.chain([(2, target)]).collect()
}

/// The help's lines: its messages one under the other, each wrapped to the box, an empty line between two.
fn help_lines(font: &Font, messages: &[String]) -> Vec<String> {
    let texts = HELP_TEXT.iter().map(|&id| messages.get(id).map_or("", String::as_str));
    texts.flat_map(|text| crate::window::wrap(font, text, HELP_BOX[2] - 12.0, 8).into_iter().chain([String::new()])).collect()
}

/// One of the windows that go with the main one: the help, the arrow sets, the route list.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Side {
    Help,
    Sets,
    List,
}

/// What a press on one of them is on: its close button, its title, an arrow set (1 to 8), or the rest.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum SideHit {
    Close,
    Title,
    Set(u8),
    Body,
}

impl Side {
    const ALL: [Side; 3] = [Side::Help, Side::Sets, Side::List];

    fn size(self) -> [f32; 2] {
        match self {
            Side::Help => HELP_SIZE,
            Side::Sets => SETS_SIZE,
            Side::List => LIST_SIZE,
        }
    }
}

/// A cell's point on a square picture of a map `side` wide, its shorter side centred: whole pixels, y down from `top`.
fn on_picture(cell: [i32; 2], [wide, high]: [usize; 2], side: f32, top: f32) -> [f32; 2] {
    let scale = side / wide.max(high).max(1) as f32;
    let centred = |short: usize| ((wide.max(high) - short) as f32 * 0.5 * scale).trunc();
    [(cell[0] as f32 * scale + centred(wide)).trunc(), (top - (cell[1] as f32 * scale + centred(high))).trunc()]
}

/// The trail's dots on the route view's map, window-local: each cell's point on a 127-px picture, a point once,
/// drawn 266/128 times as far apart.
pub fn dots(trail: &[[i32; 2]], size: [usize; 2]) -> Vec<[f32; 2]> {
    let mut points: Vec<[f32; 2]> = trail.iter().map(|&cell| on_picture(cell, size, 127.0, 126.0)).collect();
    points.dedup();
    points.into_iter().map(|[x, y]| [x * 2.078_125 + MAP[0], y * 2.078_125 + MAP[1]]).collect()
}

/// Where the player's mark goes on the route view's map (its corner, window-local): its bottom middle on the cell.
pub fn location(cell: [i32; 2], size: [usize; 2]) -> [f32; 2] {
    let [x, y] = on_picture(cell, size, MAP[2], MAP[2]);
    [MAP[0] + x - 9.0, MAP[1] + y - 17.0]
}

/// The route view's three lines: the shown name of the map being walked, where it heads on it ("map x y"), and the
/// target ("[Destination] -> map x y"; a map or a monster: "[Destination] map").
pub fn route_lines(tables: &NaviTables, messages: &[String], guide: &Guide) -> [String; 3] {
    let heads = guide.next.map_or(String::new(), |[x, y]| format!("{} {x} {y}", guide.map));
    let destination = messages.get(DESTINATION).map_or("", String::as_str);
    let target = match guide.target.cell {
        Some([x, y]) => format!("[{destination}] -> {} {x} {y}", guide.target.map),
        None => format!("[{destination}] {}", guide.target.map),
    };
    [tables.maps.get(guide.map).cloned().unwrap_or_default(), heads, target]
}

/// The small form's two lines, when the navigation heads somewhere on this map: "map x y..." and the target
/// (">.map x y", or ">.map").
pub fn mini_lines(guide: &Guide) -> Option<[String; 2]> {
    let [x, y] = guide.next?;
    let target = guide.target.cell.map_or(format!(">.{}", guide.target.map), |[x, y]| format!(">.{} {x} {y}", guide.target.map));
    Some([format!("{} {x} {y}...", guide.map), target])
}

/// A control or a row on screen, for the test driver: its name and box, and a row's words and place.
pub type Button<'a> = (String, [f32; 4], Option<String>, Option<&'a Found>);

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Hit {
    Title(TitleHit),
    Filter,
    /// A line of the open type box.
    FilterLine(usize),
    Field,
    Search,
    /// A row of the left list, of the right list: by its place in the whole list.
    Group(usize),
    Member(usize),
    Service,
    Find,
    /// The view button; the "Minimize" box; the small form's maximize and close.
    Back,
    Auto,
    Maximize,
    Close,
    /// The title's help button, the bottom bar's route list and arrow sets buttons, and a press on one of the windows
    /// they open.
    HelpButton,
    ListButton,
    SetsButton,
    Side(Side, SideHit),
    Window,
}

/// The navigation window: the search's text and type, what it found and the rows it's shown in, the row and the place
/// picked, and whether routes may take paid services.
pub struct Window {
    pub frame: Frame,
    pub text: String,
    pub typing: bool,
    pub filter: usize,
    pub filter_open: bool,
    pub found: Vec<Found>,
    pub rows: Vec<Vec<usize>>,
    pub row: Option<usize>,
    pub member: usize,
    /// The first row each list shows.
    pub first: [usize; 2],
    pub service: bool,
    /// The picked place's map and its picture, loaded when the pick changes.
    pub picture: Option<(String, Option<Texture>)>,
    /// Whether a navigation runs (kept by the drawing, a frame behind at most), and whether it's shown as the route
    /// view (a start turns that on) or left for the search.
    pub guiding: bool,
    pub route_view: bool,
    /// The small form, and the "Minimize" box.
    pub mini: bool,
    pub auto_mini: bool,
    /// The map the player is on and its picture, for the route view.
    pub here: Option<(String, Option<Texture>)>,
    /// The help, the arrow sets and the route list while they're open (by `Side`'s order), the help's first line shown,
    /// and how many ways the route has (the list opens only with some).
    pub sides: [Option<Frame>; 3],
    pub help_first: usize,
    pub steps: usize,
}

impl Window {
    /// In the middle of the screen, with the service box as the navigation under way has it. The search box takes the
    /// keys only once the player asks for the window or presses the box (`typing`).
    pub fn new(service: bool) -> Self {
        let mut frame = Frame::new([((screen()[0] - SIZE[0]) / 2.0).floor(), ((screen()[1] - SIZE[1]) / 2.0).floor()], SIZE[0]);
        frame.open = true;
        Self {
            frame,
            text: String::new(),
            typing: false,
            filter: 0,
            filter_open: false,
            found: Vec::new(),
            rows: Vec::new(),
            row: None,
            member: 0,
            first: [0, 0],
            service,
            picture: None,
            guiding: false,
            route_view: true,
            mini: false,
            auto_mini: false,
            here: None,
            sides: [None, None, None],
            help_first: 0,
            steps: 0,
        }
    }

    pub fn side(&self, side: Side) -> Option<&Frame> {
        self.sides[side as usize].as_ref()
    }

    /// Opens one of the side windows, or closes it: the help where it always opens, the arrow sets over the main
    /// window's corner, the route list at its right edge (only while the route has ways).
    fn toggle_side(&mut self, side: Side) {
        let open = match side {
            _ if self.side(side).is_some() => None,
            Side::Help => Some(HELP_AT),
            Side::Sets => Some(self.frame.at(SETS_FROM)),
            Side::List => (self.steps > 0).then(|| self.frame.at([SIZE[0], 0.0])),
        };
        self.sides[side as usize] = open.map(|at| {
            let mut frame = Frame::new(at, side.size()[0]);
            frame.open = true;
            frame
        });
        self.help_first = 0;
    }

    /// Closes the side window opened first in `Side`'s order, if one is open (Esc closes them before the main one).
    pub fn close_side(&mut self) -> bool {
        let open = self.sides.iter_mut().find(|side| side.is_some());
        open.map(|side| *side = None).is_some()
    }

    /// A press that holds a window by its title (or the main one by any spot that's no control) starts moving it.
    pub fn start_drag(&mut self, cursor: [f32; 2]) {
        match self.hit(cursor) {
            Some(Hit::Side(side, SideHit::Title)) => self.sides[side as usize].as_mut().unwrap().start_drag(cursor),
            Some(Hit::Title(TitleHit::Title) | Hit::Window) => self.frame.start_drag(cursor),
            _ => {}
        }
    }

    pub fn drag_to(&mut self, cursor: [f32; 2]) {
        let height = self.size()[1];
        self.frame.drag_to(cursor, height);
        for side in Side::ALL {
            if let Some(frame) = &mut self.sides[side as usize] {
                frame.drag_to(cursor, side.size()[1]);
            }
        }
    }

    pub fn drop_drag(&mut self) -> bool {
        let sides = self.sides.iter_mut().flatten().fold(false, |moved, frame| frame.drop_drag() | moved);
        self.frame.drop_drag() | sides
    }

    pub fn size(&self) -> [f32; 2] {
        if self.mini { MINI } else { SIZE }
    }

    /// How see-through it's drawn: the small form more than a little, a held one by half.
    pub fn fade(&self) -> f32 {
        self.frame.faded().max(if self.mini { MINI_FADE } else { 0.0 })
    }

    /// Small or full, where it stands.
    pub fn set_mini(&mut self, mini: bool) {
        self.mini = mini;
        self.frame.set_width(self.size()[0]);
        (self.typing, self.filter_open) = (false, false);
    }

    /// A click on the ground: with the "Minimize" box ticked the window goes small.
    pub fn ground_clicked(&mut self) {
        if self.auto_mini && !self.mini {
            self.set_mini(true);
        }
    }

    /// Whether the route view shows: a navigation runs and the view button hasn't turned to the search.
    pub fn routing(&self) -> bool {
        self.guiding && self.route_view
    }

    /// Keeps what the drawing needs of the game: whether a navigation runs, and the picture of the map the player is on.
    pub fn follow(&mut self, gpu: &Gpu, grf: &Grf, map: &str, guiding: bool, steps: usize) {
        (self.guiding, self.steps) = (guiding, steps);
        // The route list goes with the navigation.
        if !guiding {
            self.sides[Side::List as usize] = None;
        }
        if self.here.as_ref().map(|(shown, _)| shown.as_str()) != Some(map) {
            self.here = Some((map.to_owned(), picture(gpu, grf, map)));
        }
    }

    /// Searches for what's typed (nothing typed: nothing happens) and shows the rows from the top, none picked, in the
    /// search view.
    pub fn search(&mut self, tables: &NaviTables) {
        if self.text.trim().is_empty() || self.mini {
            return;
        }
        self.found = navigation::search(tables, &self.text);
        self.regroup();
        self.route_view = false;
    }

    /// The rows again, by the type box as it stands.
    fn regroup(&mut self) {
        self.rows = navigation::groups(&self.found, self.filter);
        (self.row, self.member, self.first) = (None, 0, [0, 0]);
    }

    pub fn type_text(&mut self, text: &str) {
        let room = MOST.saturating_sub(self.text.chars().count());
        self.text.extend(text.chars().filter(|character| !character.is_control()).take(room));
    }

    /// The place picked: a row's place on the right list.
    pub fn picked(&self) -> Option<&Found> {
        self.rows.get(self.row?).and_then(|row| row.get(self.member)).map(|&index| &self.found[index])
    }

    /// The picked place as a navigation's target, or the message that says there is none.
    pub fn target(&self, tables: &NaviTables) -> Result<Target, Option<usize>> {
        let place = self.picked().ok_or(Some(NOTHING_PICKED))?;
        navigation::target_of(tables, &place.map, place.kind, place.cell, place.monster)
    }

    /// The services a route from here may take: the Kafra and the airship with the box ticked, else none.
    pub fn services(&self) -> u8 {
        if self.service { 101 } else { 0 }
    }

    fn shown(list: [f32; 4]) -> usize {
        (list[3] / ROW) as usize
    }

    /// The rows each list holds: the left one's, the picked row's places.
    fn lengths(&self) -> [usize; 2] {
        [self.rows.len(), self.row.and_then(|row| self.rows.get(row)).map_or(0, Vec::len)]
    }

    /// The wheel over a list scrolls it, a row a notch.
    pub fn scroll_at(&mut self, cursor: [f32; 2], rows: i32) -> bool {
        // The help's text scrolls by lines; how far is the drawing's to hold (`draw` keeps it in its lines).
        if self.side(Side::Help).is_some_and(|frame| frame.local(cursor, HELP_SIZE[1]).is_some()) {
            self.help_first = (self.help_first as i32 + rows).max(0) as usize;
            return true;
        }
        let Some(local) = self.frame.local(cursor, self.size()[1]) else { return false };
        let lists = !self.mini && !self.routing();
        for (list, area) in [GROUPS, MEMBERS].into_iter().enumerate().filter(|_| lists) {
            if contains([area[0], area[1]], [area[2], area[3]], local) {
                let last = self.lengths()[list].saturating_sub(Self::shown(area));
                self.first[list] = (self.first[list] as i32 + rows).clamp(0, last as i32) as usize;
            }
        }
        true
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        // The side windows lie over the main one.
        for side in Side::ALL {
            if let Some(local) = self.side(side).and_then(|frame| frame.local(cursor, side.size()[1])) {
                let frame = self.side(side).unwrap();
                let set = || {
                    let column = SET_COLUMNS.iter().position(|&left| (left..left + 30.0).contains(&local[0]))?;
                    let row = SET_ROWS.iter().position(|&[top, bottom]| (top..bottom).contains(&local[1]))?;
                    Some((row * 4 + column + 1) as u8)
                };
                return Some(Hit::Side(
                    side,
                    match frame.title_hit(local) {
                        Some(TitleHit::Close) => SideHit::Close,
                        Some(_) => SideHit::Title,
                        None => set().filter(|_| side == Side::Sets).map_or(SideHit::Body, SideHit::Set),
                    },
                ));
            }
        }
        let local = self.frame.local(cursor, self.size()[1])?;
        let on = |area: [f32; 4]| contains([area[0], area[1]], [area[2], area[3]], local);
        // The small form: its two buttons, and the rest to hold it by.
        if self.mini {
            let button = |from_right: f32| on([MINI[0] - from_right, 2.0, MINI_BUTTON[0], MINI_BUTTON[1]]);
            return Some(if button(MINI_CLOSE) {
                Hit::Close
            } else if button(MINI_MAXIMIZE) {
                Hit::Maximize
            } else {
                Hit::Window
            });
        }
        if on(HELP_BUTTON) {
            return Some(Hit::HelpButton);
        }
        if let Some(hit) = self.frame.title_hit(local) {
            return Some(Hit::Title(hit));
        }
        let row_of = |list: usize, area: [f32; 4]| {
            let row = self.first[list] + ((local[1] - area[1]) / ROW) as usize;
            (on([area[0], area[1], area[2] - BAR, area[3]]) && row < self.lengths()[list]).then_some(row)
        };
        // The open type box's lines lie over what's under them.
        let lines = [FILTER[0], FILTER[1] + FILTER[3], FILTER[2], ROW * FILTERS.len() as f32];
        Some(if self.filter_open && on(lines) {
            Hit::FilterLine(((local[1] - lines[1]) / ROW) as usize)
        } else if on(BACK) {
            Hit::Back
        } else if on(FILTER) {
            Hit::Filter
        } else if on(FIELD) {
            Hit::Field
        } else if on(SEARCH) {
            Hit::Search
        } else if on(AUTO) {
            Hit::Auto
        } else if on(LIST_BUTTON) {
            Hit::ListButton
        } else if on(SETS_BUTTON) {
            Hit::SetsButton
        } else if self.routing() {
            // The route view has no lists, no service box and no Find.
            Hit::Window
        } else if let Some(row) = row_of(0, GROUPS) {
            Hit::Group(row)
        } else if let Some(row) = row_of(1, MEMBERS) {
            Hit::Member(row)
        } else if on(SERVICE) {
            Hit::Service
        } else if on(FIND) {
            Hit::Find
        } else {
            Hit::Window
        })
    }

    /// A click: everything but the title's buttons, the search and Find, which are the caller's (they need the tables
    /// and the game). Whether the pick changed.
    pub fn click(&mut self, hit: Hit) -> bool {
        self.typing = hit == Hit::Field;
        let open = std::mem::take(&mut self.filter_open);
        match hit {
            Hit::Filter => self.filter_open = !open,
            Hit::FilterLine(line) if line < FILTERS.len() => {
                self.filter = line;
                self.regroup();
                return true;
            }
            Hit::Group(row) => {
                (self.row, self.member, self.first[1]) = (Some(row), 0, 0);
                return true;
            }
            Hit::Member(member) => {
                self.member = member;
                return true;
            }
            Hit::Service => self.service = !self.service,
            Hit::Back => self.route_view = !self.route_view,
            Hit::Auto => self.auto_mini = !self.auto_mini,
            Hit::Maximize => self.set_mini(false),
            Hit::Title(TitleHit::Minimize) => self.set_mini(true),
            Hit::HelpButton => self.toggle_side(Side::Help),
            Hit::SetsButton => self.toggle_side(Side::Sets),
            Hit::ListButton => self.toggle_side(Side::List),
            Hit::Side(side, SideHit::Close) => self.sides[side as usize] = None,
            _ => {}
        }
        false
    }

    /// The controls and the rows on screen, by name (for the test driver): a row comes with its words and the place it
    /// stands for (a left row: its first).
    pub fn button_boxes(&self, tables: &NaviTables, messages: &[String]) -> Vec<Button<'_>> {
        if !self.frame.open {
            return Vec::new();
        }
        let area = |[x, y, w, h]: [f32; 4]| {
            let [x, y] = self.frame.at([x, y]);
            [x, y, w, h]
        };
        let plain = |name: &str, at: [f32; 4]| (name.to_owned(), area(at), None, None);
        // The side windows' close buttons and the arrow sets' boxes.
        let mut sides = Vec::new();
        for (side, name) in Side::ALL.into_iter().zip(["help", "sets", "list"]) {
            let Some(frame) = self.side(side) else { continue };
            let within = |[x, y, w, h]: [f32; 4]| {
                let [x, y] = frame.at([x, y]);
                [x, y, w, h]
            };
            let [x, y] = frame.close_button();
            sides.push((format!("navi_{name}_close"), within([x, y, crate::window::BUTTON_SIZE[0], crate::window::BUTTON_SIZE[1]]), None, None));
            if side == Side::Sets {
                sides.extend((0..8).map(|set| {
                    let [top, bottom] = SET_ROWS[set / 4];
                    (format!("navi_set_{}", set + 1), within([SET_COLUMNS[set % 4], top, 30.0, bottom - top]), None, None)
                }));
            }
        }
        if self.mini {
            let button = |from_right: f32| [MINI[0] - from_right, 2.0, MINI_BUTTON[0], MINI_BUTTON[1]];
            sides.extend([plain("navi_maximize", button(MINI_MAXIMIZE)), plain("navi_close", button(MINI_CLOSE))]);
            return sides;
        }
        let [minimize_x, minimize_y] = self.frame.minimize_button();
        let mut boxes = vec![
            plain("navi_back", BACK),
            plain("navi_filter", FILTER),
            plain("navi_field", FIELD),
            plain("navi_search", SEARCH),
            plain("navi_auto", AUTO),
            plain("navi_minimize", [minimize_x, minimize_y, crate::window::BUTTON_SIZE[0], crate::window::BUTTON_SIZE[1]]),
            plain("navi_help", HELP_BUTTON),
            plain("navi_list", LIST_BUTTON),
            plain("navi_sets", SETS_BUTTON),
        ];
        boxes.extend(sides);
        if self.routing() {
            return boxes;
        }
        boxes.extend([plain("navi_service", SERVICE), plain("navi_find", FIND)]);
        if self.filter_open {
            boxes.extend(
                (0..FILTERS.len()).map(|line| plain(&format!("navi_filter_{line}"), [FILTER[0], FILTER[1] + FILTER[3] + line as f32 * ROW, FILTER[2], ROW])),
            );
        }
        let rows = |list: usize, at: [f32; 4]| {
            (self.first[list]..self.lengths()[list].min(self.first[list] + Self::shown(at)))
                .map(move |row| (row, [at[0], at[1] + (row - self.first[list]) as f32 * ROW, at[2] - BAR, ROW]))
        };
        boxes.extend(rows(0, GROUPS).map(|(row, at)| {
            (format!("navi_group_{row}"), area(at), Some(navigation::group_label(&self.found, &self.rows[row])), Some(&self.found[self.rows[row][0]]))
        }));
        if let Some(picked) = self.row.and_then(|row| self.rows.get(row)) {
            boxes.extend(rows(1, MEMBERS).map(|(row, at)| {
                (
                    format!("navi_member_{row}"),
                    area(at),
                    Some(navigation::member_label(tables, messages, &self.found, picked, row)),
                    Some(&self.found[picked[row]]),
                )
            }));
        }
        boxes
    }

    /// The window, then the side windows over it. `set` is the arrow set in use (1 to 8).
    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &'a self,
        layer: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        font: &Font,
        tables: &NaviTables,
        messages: &[String],
        pointer: [f32; 2],
        caret: bool,
        guide: Option<&Guide>,
        set: u8,
    ) {
        if !self.frame.open {
            return;
        }
        self.draw_main(layer, chrome, assets, font, tables, messages, pointer, caret, guide, set);
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        for (side, title) in Side::ALL.into_iter().zip([HELP_TITLE, SETS_TITLE, LIST_TITLE]) {
            let Some(frame) = self.side(side) else { continue };
            let size = side.size();
            let fill = |layer: &mut Layer<'a>, texture: &'a Texture, [x, y, w, h]: [f32; 4]| layer.sprites.push((texture, frame.at([x, y]), [w, h]));
            fill(layer, &chrome.border, [0.0, TITLE_HEIGHT, size[0], size[1] - TITLE_HEIGHT]);
            fill(layer, if side == Side::List { &assets.grey } else { &chrome.white }, [1.0, TITLE_HEIGHT, size[0] - 2.0, size[1] - TITLE_HEIGHT - 1.0]);
            frame.draw_bar(layer, chrome);
            frame.draw_bare_title(layer, chrome, message(title), pointer);
            match side {
                // Its messages in a grey box, from the line the wheel scrolled to.
                Side::Help => {
                    fill(layer, &assets.help_box, HELP_BOX);
                    let lines = help_lines(font, messages);
                    let shown = (HELP_BOX[3] / HELP_LINE) as usize;
                    let first = self.help_first.min(lines.len().saturating_sub(shown));
                    for (row, line) in lines.into_iter().skip(first).take(shown).enumerate() {
                        layer.texts.push((line, frame.at([HELP_BOX[0] + 4.0, HELP_BOX[1] + 3.0 + row as f32 * HELP_LINE]), INK));
                    }
                }
                // The eight sets' picture, a frame round the one in use.
                Side::Sets => {
                    layer.sprites.push((&assets.sets, frame.at([0.0, TITLE_HEIGHT]), assets.sets.size));
                    let index = usize::from(set.clamp(1, 8)) - 1;
                    let mark = [36.0 * (index % 4 + 1) as f32 - 26.0, if index < 4 { 24.0 } else { 60.0 }];
                    layer.sprites.push((&assets.set_frame, frame.at(mark), assets.set_frame.size));
                }
                // A row a way of the route, then the target.
                Side::List => {
                    fill(layer, &chrome.white, LIST_BOX);
                    let rows = guide.map(route_rows).unwrap_or_default();
                    for (row, (mark, words)) in rows.into_iter().take((LIST_BOX[3] / ROW) as usize).enumerate() {
                        let top = LIST_BOX[1] + row as f32 * ROW;
                        layer.sprites.push((&assets.steps[mark], frame.at([LIST_BOX[0] + 2.0, top + 2.0]), assets.steps[mark].size));
                        layer.texts.push((crate::window::fit(font, &words, LIST_BOX[2] - 18.0), frame.at([LIST_BOX[0] + 14.0, top + 2.0]), INK));
                    }
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_main<'a>(
        &'a self,
        layer: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        font: &Font,
        tables: &NaviTables,
        messages: &[String],
        pointer: [f32; 2],
        caret: bool,
        guide: Option<&Guide>,
        set: u8,
    ) {
        let at = |local: [f32; 2]| self.frame.at(local);
        let message = |id: usize| messages.get(id).map_or("", String::as_str);
        let title = messages.get(TITLE).map_or("Navigation", String::as_str);
        let over = |area: [f32; 4]| self.frame.local(pointer, self.size()[1]).is_some_and(|local| contains([area[0], area[1]], [area[2], area[3]], local));
        let fill = |layer: &mut Layer<'a>, texture: &'a Texture, [x, y, w, h]: [f32; 4]| layer.sprites.push((texture, at([x, y]), [w, h]));
        // The small form: a white box with its title and two buttons, and in its black box where the navigation heads
        // and its target.
        if self.mini {
            fill(layer, &chrome.border, [0.0, 0.0, MINI[0], MINI[1]]);
            fill(layer, &chrome.white, [1.0, 1.0, MINI[0] - 2.0, MINI[1] - 2.0]);
            fill(layer, &assets.black, MINI_BOX);
            layer.texts.push((title.to_owned(), at([3.0, 3.0]), INK));
            for (images, from_right) in [(&assets.maximize, MINI_MAXIMIZE), (&assets.close, MINI_CLOSE)] {
                let corner = [MINI[0] - from_right, 2.0];
                let image = &images[usize::from(over([corner[0], corner[1], MINI_BUTTON[0], MINI_BUTTON[1]]))];
                layer.sprites.push((image, at(corner), image.size));
            }
            if let Some(lines) = guide.and_then(mini_lines) {
                layer.sprites.push((&assets.destination, at([2.0, 18.0]), assets.destination.size));
                for (line, place) in lines.into_iter().zip(MINI_LINES) {
                    layer.texts.push((crate::window::fit(font, &line, MINI_BOX[2] - 6.0), at(place), WHITE));
                }
            }
            return;
        }
        self.frame.draw_title(layer, chrome, title, pointer);
        // The body, its border, the line under the search row, and the lines between the lists and the picked place.
        fill(layer, &chrome.white, [0.0, TITLE_HEIGHT, SIZE[0], SIZE[1] - TITLE_HEIGHT]);
        for x in [0.0, SIZE[0] - 1.0] {
            fill(layer, &chrome.border, [x, TITLE_HEIGHT, 1.0, SIZE[1] - TITLE_HEIGHT]);
        }
        fill(layer, &chrome.border, [0.0, SIZE[1] - 1.0, SIZE[0], 1.0]);
        fill(layer, &assets.grey, [0.0, 43.0, SIZE[0], 1.0]);
        layer.sprites.push((&assets.bottom, at(BOTTOM), assets.bottom.size));
        // The view button, and the "Minimize" box on the bottom bar.
        let back = &assets.back[usize::from(over(BACK))];
        layer.sprites.push((back, at([BACK[0], BACK[1]]), back.size));
        let auto = &assets.check[usize::from(self.auto_mini)];
        layer.sprites.push((auto, at([AUTO[0], AUTO[1] + 1.0]), auto.size));
        layer.texts.push(("Minimize".to_owned(), at([AUTO[0] + auto.size[0] + 3.0, AUTO[1]]), INK));
        // The help button on the title (lit while the help is open), and the bottom bar's route list and arrow sets
        // buttons (the set in use on its own).
        let help = &assets.help[usize::from(self.side(Side::Help).is_some())];
        layer.sprites.push((help, at([HELP_BUTTON[0], HELP_BUTTON[1]]), help.size));
        let list = &assets.list_button[usize::from(over(LIST_BUTTON))];
        layer.sprites.push((list, at([LIST_BUTTON[0], LIST_BUTTON[1]]), list.size));
        let sets = &assets.set_buttons[usize::from(set.clamp(1, 8)) - 1][usize::from(over(SETS_BUTTON))];
        layer.sprites.push((sets, at([SETS_BUTTON[0], SETS_BUTTON[1]]), sets.size));
        // The type box with its arrow, the search box with what's typed, the Search button.
        layer.sprites.push((&assets.filter[0], at([FILTER[0], FILTER[1]]), assets.filter[0].size));
        layer.sprites.push((&assets.filter[1], at([FILTER[0] + FILTER[2] - assets.filter[1].size[0], FILTER[1]]), assets.filter[1].size));
        layer.texts.push((message(FILTERS[self.filter]).to_owned(), at([FILTER[0] + 4.0, FILTER[1] + 3.0]), INK));
        layer.sprites.push((&assets.field, at([FIELD[0], FIELD[1]]), assets.field.size));
        let typed = if self.typing && caret { format!("{}_", self.text) } else { self.text.clone() };
        layer.texts.push((crate::window::fit(font, &typed, FIELD[2] - 6.0), at([FIELD[0] + 3.0, FIELD[1] + 3.0]), INK));
        let search = &assets.search[usize::from(over(SEARCH))];
        layer.sprites.push((search, at([SEARCH[0], SEARCH[1]]), search.size));
        // The route view: the map the player is on, the trail's dots on it with a mark where it ends, the player's
        // mark, and the three lines.
        if let Some(guide) = guide.filter(|_| self.routing()) {
            fill(layer, &assets.grey, SQUARE);
            fill(layer, &assets.black, MAP);
            if let Some((_, Some(picture))) = &self.here {
                layer.sprites.push((picture, at([MAP[0], MAP[1]]), [MAP[2], MAP[3]]));
            }
            for (line, place) in route_lines(tables, messages, guide).into_iter().zip(ROUTE_LINES) {
                layer.texts.push((crate::window::fit(font, &line, MAP[2] - 14.0), at(place), WHITE));
            }
            let points = dots(guide.trail, guide.size);
            for &point in &points {
                layer.sprites.push((&assets.dot, at(point), [2.0, 2.0]));
            }
            if let Some(&[x, y]) = points.last() {
                layer.sprites.push((&assets.destination, at([x - 8.0, y - 21.0]), assets.destination.size));
            }
            layer.sprites.push((&assets.location, at(location(guide.player, guide.size)), assets.location.size));
            return;
        }
        for line in [[138.0, 44.0, 1.0, 270.0], [139.0, 242.0, 132.0, 1.0]] {
            fill(layer, &assets.grey, line);
        }
        layer.texts.push((rustnarok_client::messages::fill(message(FOUND), &[&self.rows.len()]), at(FOUND_AT), INK));
        // The lists: a row's icon and words, the picked one on its bar; a bar at the right when there's more.
        let lengths = self.lengths();
        let list = |layer: &mut Layer<'a>, which: usize, area: [f32; 4], picked: Option<usize>, row: &dyn Fn(usize) -> (Option<u8>, String)| {
            let shown = Self::shown(area);
            for index in self.first[which]..lengths[which].min(self.first[which] + shown) {
                let top = area[1] + (index - self.first[which]) as f32 * ROW;
                if picked == Some(index) {
                    fill(layer, &assets.picked, [area[0], top, area[2] - BAR, ROW]);
                }
                let (kind, words) = row(index);
                let left = match kind.and_then(|kind| assets.icons.get(usize::from(kind).wrapping_sub(1))) {
                    Some(icon) => {
                        layer.sprites.push((icon, at([area[0] + 2.0, top + 2.0]), icon.size));
                        14.0
                    }
                    None => 3.0,
                };
                layer.texts.push((crate::window::fit(font, &words, area[2] - BAR - left - 2.0), at([area[0] + left, top + 2.0]), INK));
            }
            // ponytail: a plain track and thumb; the game's bar has pictures of its own.
            if lengths[which] > shown {
                let (track, thumb) = (area[3], (area[3] * shown as f32 / lengths[which] as f32).max(8.0));
                let top = area[1] + (track - thumb) * self.first[which] as f32 / (lengths[which] - shown) as f32;
                fill(layer, &assets.grey, [area[0] + area[2] - BAR, area[1], BAR, track]);
                fill(layer, &chrome.border, [area[0] + area[2] - BAR + 1.0, top, BAR - 2.0, thumb]);
            }
        };
        list(layer, 0, GROUPS, self.row, &|row| (Some(self.found[self.rows[row][0]].kind), navigation::group_label(&self.found, &self.rows[row])));
        if let Some(picked) = self.row.and_then(|row| self.rows.get(row)) {
            list(layer, 1, MEMBERS, Some(self.member), &|member| (None, navigation::member_label(tables, messages, &self.found, picked, member)));
        }
        // The picked place: its name over its map's picture.
        if let Some(place) = self.picked() {
            fill(layer, &assets.black, PICTURE);
            if let Some((_, Some(picture))) = &self.picture {
                layer.sprites.push((picture, at([PICTURE[0], PICTURE[1]]), [PICTURE[2], PICTURE[3]]));
            }
            layer.texts.push((crate::window::fit(font, &place.name, SIZE[0] - NAME_AT[0] - 4.0), at(NAME_AT), PICKED));
        }
        // The service box, and Find.
        let check = &assets.check[usize::from(self.service)];
        layer.sprites.push((check, at([SERVICE[0], SERVICE[1] + 1.0]), check.size));
        layer.texts.push(("service".to_owned(), at([SERVICE[0] + check.size[0] + 3.0, SERVICE[1]]), INK));
        let find = &assets.find[usize::from(over(FIND))];
        layer.sprites.push((find, at([FIND[0], FIND[1]]), find.size));
        // The open type box's lines, over the rest.
        if self.filter_open {
            let top = FILTER[1] + FILTER[3];
            fill(layer, &chrome.border, [FILTER[0], top, FILTER[2], ROW * FILTERS.len() as f32 + 1.0]);
            fill(layer, &chrome.white, [FILTER[0] + 1.0, top, FILTER[2] - 2.0, ROW * FILTERS.len() as f32]);
            for (line, id) in FILTERS.into_iter().enumerate() {
                let line_top = top + line as f32 * ROW;
                if over([FILTER[0], line_top, FILTER[2], ROW]) {
                    fill(layer, &assets.picked, [FILTER[0] + 1.0, line_top, FILTER[2] - 2.0, ROW]);
                }
                layer.texts.push((message(id).to_owned(), at([FILTER[0] + 4.0, line_top + 2.0]), INK));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustnarok_client::lua::{NaviMonster, NaviNpc};

    fn tables() -> NaviTables {
        NaviTables {
            maps: [("prt_fild08", "Prontera Field"), ("prontera", "Prontera")].map(|(map, shown)| (map.to_owned(), shown.to_owned())).into_iter().collect(),
            npcs: vec![NaviNpc { map: "prontera".into(), name: "Tool Dealer".into(), cell: [30, 40] }],
            monsters: vec![NaviMonster { map: "prt_fild08".into(), id: 1002, name: "Poring".into(), spawns: 70 }],
            ..NaviTables::default()
        }
    }

    #[test]
    fn turns_between_the_search_and_the_route_and_goes_small() {
        let mut window = Window::new(false);
        let corner = window.frame.position;
        let at = |area: [f32; 4]| [corner[0] + area[0] + 3.0, corner[1] + area[1] + 3.0];
        // No navigation: the search view whatever the view button says. One runs: the route view, with no lists.
        window.rows = vec![vec![0]];
        window.found = navigation::search(&tables(), "prontera");
        assert_eq!((window.routing(), window.hit(at(GROUPS))), (false, Some(Hit::Group(0))));
        window.guiding = true;
        assert_eq!((window.routing(), window.hit(at(GROUPS)), window.hit(at(FIND))), (true, Some(Hit::Window), Some(Hit::Window)));
        // The view button turns to the search and back; a search turns to the search view by itself.
        assert_eq!(window.hit(at(BACK)), Some(Hit::Back));
        window.click(Hit::Back);
        assert_eq!((window.routing(), window.hit(at(GROUPS))), (false, Some(Hit::Group(0))));
        window.click(Hit::Back);
        assert!(window.routing());
        window.type_text("pront");
        window.search(&tables());
        assert!(!window.routing());
        // The title's minimize button: the small form, where the window stands, with two buttons; maximize brings the
        // full one back.
        window.click(Hit::Title(TitleHit::Minimize));
        assert_eq!((window.mini, window.size(), window.frame.position, window.fade()), (true, MINI, corner, MINI_FADE));
        assert_eq!(window.hit([corner[0] + MINI[0] - MINI_CLOSE + 2.0, corner[1] + 4.0]), Some(Hit::Close));
        assert_eq!(window.hit([corner[0] + MINI[0] - MINI_MAXIMIZE + 2.0, corner[1] + 4.0]), Some(Hit::Maximize));
        assert_eq!((window.hit([corner[0] + 5.0, corner[1] + 30.0]), window.hit([corner[0] + 200.0, corner[1] + 30.0])), (Some(Hit::Window), None));
        window.click(Hit::Maximize);
        assert_eq!((window.mini, window.size(), window.fade()), (false, SIZE, 0.0));
        // The "Minimize" box: ticked, a click on the ground makes it small; not ticked, nothing.
        window.ground_clicked();
        assert!(!window.mini);
        assert_eq!(window.hit(at(AUTO)), Some(Hit::Auto));
        window.click(Hit::Auto);
        window.ground_clicked();
        assert!(window.mini);
    }

    #[test]
    fn says_where_the_navigation_heads_and_marks_the_map() {
        let tables = tables();
        let messages = {
            let mut messages = vec![String::new(); DESTINATION + 1];
            messages[DESTINATION] = "Destination".into();
            messages
        };
        let cell = Target { map: "prontera".into(), cell: Some([150, 150]), monster: None, name: String::new() };
        let trail = [[10, 10], [11, 10], [12, 10], [13, 10]];
        let guide =
            |map: &'static str, next, target: &'static Target| Guide { map, size: [400, 200], player: [10, 10], trail: &trail, next, target, steps: &[] };
        let cell: &'static Target = Box::leak(Box::new(cell));
        let field: &'static Target = Box::leak(Box::new(Target { map: "prt_fild08".into(), cell: None, monster: Some(1002), name: String::new() }));
        // On the way: this map's shown name, the warp walked to, the target and its cell.
        let on_the_way = guide("prt_fild08", Some([170, 375]), cell);
        assert_eq!(route_lines(&tables, &messages, &on_the_way), ["Prontera Field", "prt_fild08 170 375", "[Destination] -> prontera 150 150"]);
        assert_eq!(mini_lines(&on_the_way), Some(["prt_fild08 170 375...".into(), ">.prontera 150 150".into()]));
        // A monster's map as the target: its name alone. Nowhere to head on this map: the small form says nothing.
        assert_eq!(route_lines(&tables, &messages, &guide("prontera", Some([156, 22]), field))[2], "[Destination] prt_fild08");
        assert_eq!(mini_lines(&guide("prontera", Some([156, 22]), field)).unwrap()[1], ">.prt_fild08");
        assert_eq!(mini_lines(&guide("prontera", None, cell)), None);
        // A 400 x 200 map on the square picture: the short side centred; a dot a cell, twice as far apart on the
        // window, the same point once.
        assert_eq!(on_picture([0, 0], [400, 200], 127.0, 126.0), [0.0, 95.0]);
        assert_eq!(on_picture([400, 200], [400, 200], 127.0, 126.0), [127.0, 31.0]);
        assert_eq!(on_picture([100, 100], [200, 400], 266.0, 266.0), [132.0, 199.0]);
        let points = dots(&trail, [400, 200]);
        assert_eq!(points, [[3.0 * 2.078_125 + MAP[0], 91.0 * 2.078_125 + MAP[1]], [4.0 * 2.078_125 + MAP[0], 91.0 * 2.078_125 + MAP[1]]]);
        // The player's mark: its bottom middle on the cell.
        assert_eq!(location([200, 100], [400, 200]), [MAP[0] + 133.0 - 9.0, MAP[1] + 133.0 - 17.0]);
    }

    #[test]
    fn opens_the_help_the_arrow_sets_and_the_route_list() {
        let mut window = Window::new(false);
        let corner = window.frame.position;
        let at = |area: [f32; 4]| [corner[0] + area[0] + 3.0, corner[1] + area[1] + 3.0];
        assert_eq!(
            [window.hit(at(HELP_BUTTON)), window.hit(at(LIST_BUTTON)), window.hit(at(SETS_BUTTON))],
            [Some(Hit::HelpButton), Some(Hit::ListButton), Some(Hit::SetsButton)]
        );
        // The help opens where it always does; the arrow sets over the main window's corner; the route list only while
        // the route has ways, at the right edge.
        window.click(Hit::HelpButton);
        window.click(Hit::SetsButton);
        window.click(Hit::ListButton);
        assert_eq!(window.side(Side::Help).map(|frame| frame.position), Some(HELP_AT));
        assert_eq!(window.side(Side::Sets).map(|frame| frame.position), Some([corner[0] + 113.0, corner[1] + 211.0]));
        assert!(window.side(Side::List).is_none());
        window.steps = 2;
        window.click(Hit::ListButton);
        assert_eq!(window.side(Side::List).map(|frame| frame.position), Some([corner[0] + SIZE[0], corner[1]]));
        // A press on one is its own: its title, its close, an arrow set by its box (two rows of four).
        let sets = window.side(Side::Sets).unwrap().position;
        let set = |x: f32, y: f32| window.hit([sets[0] + x, sets[1] + y]);
        assert_eq!(
            [set(12.0, 26.0), set(120.0, 54.0), set(48.0, 64.0), set(148.0, 90.0)].map(|hit| hit.unwrap()),
            [1, 4, 6, 8].map(|set| Hit::Side(Side::Sets, SideHit::Set(set)))
        );
        assert_eq!(
            [set(43.0, 30.0), set(12.0, 58.0), set(50.0, 5.0)].map(|hit| hit.unwrap()),
            [SideHit::Body, SideHit::Body, SideHit::Title].map(|hit| Hit::Side(Side::Sets, hit))
        );
        let close = window.side(Side::Help).unwrap().close_button();
        assert_eq!(window.hit([HELP_AT[0] + close[0] + 2.0, HELP_AT[1] + close[1] + 2.0]), Some(Hit::Side(Side::Help, SideHit::Close)));
        // The wheel over the help scrolls its lines.
        assert!(window.scroll_at([HELP_AT[0] + 50.0, HELP_AT[1] + 50.0], 3));
        assert_eq!(window.help_first, 3);
        // Esc closes them one at a time before the main window; the button closes its own.
        window.click(Hit::Side(Side::Help, SideHit::Close));
        assert!(window.side(Side::Help).is_none() && window.close_side() && window.side(Side::Sets).is_none());
        window.click(Hit::ListButton);
        assert!(window.side(Side::List).is_none() && !window.close_side());
    }

    #[test]
    fn lists_the_route_way_by_way() {
        let steps = vec![("prt_fild08".to_owned(), [170, 375]), ("prontera".to_owned(), [100, 50])];
        let target = Target { map: "prt_in".into(), cell: Some([30, 40]), monster: None, name: String::new() };
        let guide = Guide { map: "prt_fild08", size: [400, 400], player: [1, 1], trail: &[], next: Some([170, 375]), target: &target, steps: &steps };
        // The first way is the start, the others ways on, the target last.
        assert_eq!(route_rows(&guide), [(0, "prt_fild08 (170 375)".to_owned()), (1, "prontera (100 50)".to_owned()), (2, "prt_in (30 40)".to_owned())]);
        let field = Target { map: "prt_fild08".into(), cell: None, monster: Some(1002), name: String::new() };
        assert_eq!(route_rows(&Guide { target: &field, steps: &[], ..guide }), [(2, "prt_fild08".to_owned())]);
    }

    fn window(text: &str) -> Window {
        let mut window = Window::new(false);
        window.type_text(text);
        window.search(&tables());
        window
    }

    #[test]
    fn types_thirty_characters_at_most() {
        let mut window = window("");
        assert_eq!((window.typing, window.rows.len()), (false, 0));
        window.type_text(&"a".repeat(40));
        assert_eq!(window.text.len(), 30);
    }

    #[test]
    fn picks_a_row_then_a_place_and_starts_nothing_by_itself() {
        let mut window = window("pront");
        // Two maps: the field with its Poring, and the town.
        assert_eq!(window.rows, [vec![0, 2], vec![1]]);
        assert_eq!(window.target(&tables()), Err(Some(NOTHING_PICKED)));
        let corner = window.frame.position;
        let center = |area: [f32; 4]| [corner[0] + area[0] + 5.0, corner[1] + area[1] + 5.0];
        let (first_row, second_place) = (center(GROUPS), center([MEMBERS[0], MEMBERS[1] + ROW, 0.0, 0.0]));
        assert_eq!(window.hit(first_row), Some(Hit::Group(0)));
        // A row picks its first place; no third row to press.
        assert!(window.click(Hit::Group(0)));
        assert_eq!(window.target(&tables()).map(|target| target.kind()), Ok("map"));
        assert_eq!((window.hit(second_place), window.hit(center([GROUPS[0], GROUPS[1] + 2.0 * ROW, 0.0, 0.0]))), (Some(Hit::Member(1)), Some(Hit::Window)));
        assert!(window.click(Hit::Member(1)));
        assert_eq!(window.target(&tables()).map(|target| (target.kind(), target.monster)), Ok(("monster", Some(1002))));
        // The service box: none of the paid ways, or the Kafra and the airship.
        assert_eq!(window.services(), 0);
        assert!(!window.click(Hit::Service));
        assert_eq!(window.services(), 101);
    }

    #[test]
    fn the_type_box_shows_the_rows_of_one_type() {
        let mut window = window("o");
        assert_eq!(window.rows.len(), 4);
        window.click(Hit::Group(0));
        // It opens on a press and closes on the next, whatever that's on; a line of it sets the type and unpicks.
        window.click(Hit::Filter);
        assert!(window.filter_open);
        let line = [window.frame.position[0] + FILTER[0] + 5.0, window.frame.position[1] + FILTER[1] + FILTER[3] + 2.0 * ROW + 5.0];
        assert_eq!(window.hit(line), Some(Hit::FilterLine(2)));
        assert!(window.click(Hit::FilterLine(2)));
        assert_eq!((window.filter, window.filter_open, window.rows.len(), window.row), (2, false, 1, None));
        assert_eq!(navigation::group_label(&window.found, &window.rows[0]), "[1]Tool Dealer");
        // Closed, the same spot is the left list's.
        assert_ne!(window.hit(line), Some(Hit::FilterLine(2)));
    }

    #[test]
    fn the_wheel_scrolls_a_list_that_has_more() {
        let mut tables = tables();
        tables.npcs.extend((0..20).map(|index| NaviNpc { map: "prontera".into(), name: format!("Guard {index:02}"), cell: [index, 1] }));
        let mut window = Window::new(false);
        window.type_text("guard");
        window.search(&tables);
        let over = [window.frame.position[0] + GROUPS[0] + 5.0, window.frame.position[1] + GROUPS[1] + 5.0];
        // Twenty rows, fifteen shown: five to scroll, no further.
        assert!(window.scroll_at(over, 3));
        assert_eq!(window.first[0], 3);
        window.scroll_at(over, 9);
        assert_eq!((window.first[0], window.hit(over)), (5, Some(Hit::Group(5))));
        window.scroll_at(over, -9);
        assert_eq!(window.first[0], 0);
        assert!(!window.scroll_at([0.0, 0.0], 1));
    }
}
