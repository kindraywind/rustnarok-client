//! The STR world effects: the client's STR player with a start delay, loop, hold, repeats, cut, screen scale, mirror and
//! turn and a head-height rule, started by id from `world_str.txt`. A copy ticks by its own 16 ms counter, so a far one
//! stands still.

use std::collections::HashMap;
use std::rc::Rc;

use crate::effects::Loaded;
use crate::world::Vector;

/// A copy this far from the own player (3D, in world units) is far: it neither ages nor draws.
pub const FAR: f32 = 130.0;
/// An STR step is 16 ms: the delay and the cut count down by this much a tick.
const TICK_MS: i32 = 16;
/// The full-screen black fill grows up to this much alpha.
const TINT_MAX: u32 = 170;
/// The STR's (0, -80) px anchor over the feet.
const RISE: f32 = crate::effects::STR_RISE;
/// The id whose STR sits on the point itself, with no rise.
const NO_RISE: u32 = 1412;
/// The id whose head height starts from 18.
const LOW_HEAD: u32 = 1817;

/// Where a row's file comes from.
#[derive(Debug, Clone, PartialEq)]
pub enum Source {
    /// Given at the launch (`-`).
    Launch,
    /// Named by the hat tables (`lua`).
    Hat,
    /// A file under data\texture\effect\, forward slashes, no .str; `%d` is a number from 1 to 3.
    Named(String),
}

/// A row's head rule.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Head {
    /// The STR sits 80 px over the feet.
    Feet,
    /// From the hat tables.
    Hat,
    /// `dx,dy_adjust` of the head-height rule.
    At { dx: f32, adjust: f32 },
    /// `view:dx,dy`: the feet moved dx units along the camera's right and dy along its up, as they are.
    View { dx: f32, dy: f32 },
}

/// A sound or a camera shake at the copy's age in ticks.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// A file under data\wav\.
    Sound(u32, String),
    Quake(u32),
}

#[derive(Debug, Clone)]
pub struct Row {
    pub file: Source,
    /// The file `/mineffect` plays instead.
    pub min: Option<String>,
    pub single: bool,
    pub looped: bool,
    pub owner: bool,
    pub head: Head,
    /// How much alpha a frame the black fill gains (0 none).
    pub tint: u32,
    pub events: Vec<Event>,
    /// The start delay in ms every launch gets (effects.txt's `wdelay`).
    pub delay: i32,
}

/// The ids' data (world_str.txt).
pub struct Table(HashMap<u32, Row>);

fn dash(field: &str) -> Option<&str> {
    (field != "-").then_some(field)
}

impl Table {
    pub fn load() -> Self {
        let mut rows = HashMap::new();
        for line in include_str!("world_str.txt").lines().filter(|line| !line.is_empty() && !line.starts_with('#')) {
            let fields: Vec<&str> = line.split('\t').collect();
            let [id, file, min, flags, head, tint, events] = fields[..] else { continue };
            let Ok(id) = id.parse() else { continue };
            let flag = |name: &str| flags.split(',').any(|each| each == name);
            let head = match head {
                "-" => Head::Feet,
                "lua" => Head::Hat,
                view if view.starts_with("view:") => match view[5..].split_once(',').and_then(|(dx, dy)| Some((dx.parse().ok()?, dy.parse().ok()?))) {
                    Some((dx, dy)) => Head::View { dx, dy },
                    None => Head::Feet,
                },
                pair => match pair.split_once(',').and_then(|(dx, adjust)| Some((dx.parse().ok()?, adjust.parse().ok()?))) {
                    Some((dx, adjust)) => Head::At { dx, adjust },
                    None => Head::Feet,
                },
            };
            let events = dash(events)
                .into_iter()
                .flat_map(|events| events.split(';'))
                .filter_map(|event| match event.strip_prefix('w') {
                    Some(sound) => sound.split_once(':').and_then(|(tick, file)| Some(Event::Sound(tick.parse().ok()?, file.to_owned()))),
                    None => event.strip_prefix('q')?.parse().ok().map(Event::Quake),
                })
                .collect();
            let file = match file {
                "-" => Source::Launch,
                "lua" => Source::Hat,
                file => Source::Named(file.to_owned()),
            };
            let row = Row {
                file,
                min: dash(min).map(str::to_owned),
                single: flag("single"),
                looped: flag("loop"),
                owner: flag("owner"),
                head,
                tint: tint.parse().unwrap_or(0),
                events,
                delay: 0,
            };
            rows.insert(id, row);
        }
        Self(rows)
    }

    pub fn get(&self, id: u32) -> Option<&Row> {
        self.0.get(&id)
    }

    /// Gives every launch of `id` a start delay of `ms`.
    pub fn set_delay(&mut self, id: u32, ms: i32) {
        if let Some(row) = self.0.get_mut(&id) {
            row.delay = ms;
        }
    }

    /// Drops the ids another table already plays.
    pub fn remove(&mut self, id: u32) {
        self.0.remove(&id);
    }

    #[cfg(test)]
    fn rows(&self) -> &HashMap<u32, Row> {
        &self.0
    }
}

/// What one STR step did.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Step {
    /// The STR has ended.
    Over,
    /// Nothing is drawn: the start delay.
    Wait,
    /// The last state stays drawn (hold).
    Keep,
    /// Draw the layers at this frame.
    Show(i32),
}

/// The STR step's controls and counters.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Timeline {
    /// Start delay in ms.
    pub delay: i32,
    /// Cut in ms (0 none).
    pub cut: i32,
    armed: bool,
    pub frame: i32,
    pub looped: bool,
    pub hold: bool,
    /// Extra plays after the first.
    pub repeats: u32,
    /// The frame an STR must have reached before it may end or repeat.
    pub minimum: i32,
}

impl Timeline {
    /// One step of an STR whose last key is at `last_key`.
    pub fn step(&mut self, last_key: i32) -> Step {
        if self.delay > 0 {
            self.delay -= TICK_MS;
            return Step::Wait;
        }
        if self.cut != 0 {
            self.armed = true;
        }
        if self.armed && self.frame - self.cut >= 0 {
            return Step::Over;
        }
        if self.frame > last_key {
            if self.hold {
                return Step::Keep;
            }
            if !self.looped && self.frame >= self.minimum && self.frame >= self.cut {
                if self.repeats == 0 {
                    return Step::Over;
                }
                self.repeats -= 1;
            }
            self.frame = 0;
        }
        let shown = self.frame;
        self.frame += 1;
        if self.armed {
            self.cut -= TICK_MS;
        }
        Step::Show(shown)
    }
}

/// What the head rule needs to know of a unit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Body {
    /// The head's height over the feet.
    pub height: f32,
    /// The unit's size factor.
    pub scale: f32,
    pub rides: bool,
    pub doram: bool,
    /// The way it faces, in degrees (`Actor::rot`: 0 south, 90 east).
    pub rot: f32,
}

// ponytail: the exact head height and size factor are not known. The height is 4 - 0.29·L, L being the y offset in
// sprite pixels (negative up) of the body sprite's first layer at action 0, frame 0 (30 when it has none); the factor is
// the unit's draw scale. A skill unit's are 30 and 1.
impl Body {
    pub fn from_sprite(offset: Option<f32>, scale: f32, rides: bool, doram: bool) -> Self {
        Self { height: 4.0 - 0.29 * offset.unwrap_or(30.0), scale, rides, doram, rot: 0.0 }
    }

    pub fn skill_unit() -> Self {
        Self { height: 30.0, scale: 1.0, rides: false, doram: false, rot: 0.0 }
    }
}

/// How far right and up of the feet the head rule puts the STR, in world units; None when it moves nothing.
pub fn head_offset(id: u32, dx: f32, adjust: f32, body: Body) -> Option<[f32; 2]> {
    let base = if id == LOW_HEAD || body.doram && body.rides {
        18.0
    } else if body.rides {
        23.0
    } else {
        body.height + 18.0
    };
    let dy = base + adjust - 7.0;
    let (right, up) = (dx * body.scale, dy * body.scale);
    (right >= 0.0 || up >= 0.0).then_some([right, up])
}

/// The black fill's alpha (of 255) at an STR frame.
pub fn tint_alpha(rate: u32, frame: u32) -> u32 {
    (rate * frame).min(TINT_MAX)
}

/// What a launcher sets on a copy.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Controls {
    /// Turns the picture on the screen, in degrees.
    pub turn: f32,
    pub scale: f32,
    pub alpha: f32,
    /// Added to every vertex, in px.
    pub offset: [f32; 2],
    pub mirror_x: bool,
    pub mirror_y: bool,
    /// The screen turn follows the camera's turn.
    pub follow_camera: bool,
    /// The distance from the player at which the copy is far.
    pub far: f32,
}

impl Default for Controls {
    fn default() -> Self {
        Self { turn: 0.0, scale: 1.0, alpha: 1.0, offset: [0.0; 2], mirror_x: false, mirror_y: false, follow_camera: false, far: FAR }
    }
}

/// A point of the picture, in px from the point the copy stands at (the feet, or the head point): scaled, mirrored and
/// turned about it, then moved by the extra offset.
// ponytail: in that order.
pub fn transform(point: [f32; 2], controls: &Controls) -> [f32; 2] {
    let [mut x, mut y] = point.map(|value| value * controls.scale);
    if controls.mirror_x {
        x = -x;
    }
    if controls.mirror_y {
        y = -y;
    }
    if controls.turn > 0.0 {
        let (sin, cos) = controls.turn.to_radians().sin_cos();
        [x, y] = [x * cos - y * sin, x * sin + y * cos];
    }
    [x + controls.offset[0], y + controls.offset[1]]
}

/// One layer's quad of a copy, ready for the renderer.
pub struct Drawn {
    pub layer: usize,
    pub texture: usize,
    /// The world point the corners hang from (the renderer lifts it 80 px).
    pub at: Vector,
    pub corners: [[f32; 2]; 4],
    pub color: [f32; 4],
    pub additive: bool,
}

/// A launch.
pub struct Launch {
    pub id: u32,
    /// The unit it follows (`Some(None)` = the player), or None for a cell.
    pub owner: Option<Option<u32>>,
    pub point: Vector,
    pub unique: bool,
    /// A timed launch, in ticks of the copy's own counter (0 none).
    pub life: u32,
    /// `/mineffect` is on.
    pub min: bool,
    /// A random whole number 0 … 2, for a `%d` file.
    pub roll: u32,
}

/// The world a tick sees.
pub struct Scene<'a> {
    /// The own player's feet.
    pub player: Vector,
    /// A unit's feet (None = the player), or None for a unit that is gone.
    pub feet: &'a dyn Fn(Option<u32>) -> Option<Vector>,
    pub body: &'a dyn Fn(Option<u32>) -> Option<Body>,
    /// The camera's longitude in degrees.
    pub longitude: f32,
    /// The camera's right and up axes in the world.
    pub right: Vector,
    pub up: Vector,
    /// The ground's height (world y) under a world x and z.
    pub ground: &'a dyn Fn(f32, f32) -> f32,
}

/// A sound or a shake a tick fired.
#[derive(Debug, Clone, PartialEq)]
pub enum Fired {
    Sound(String, Vector),
    Quake,
    /// A unit (None = the player) starts leaving Kagemusya's afterimages.
    Afterimage(Option<u32>),
}

pub struct World {
    pub serial: u32,
    pub id: u32,
    pub owner: Option<Option<u32>>,
    /// The status (EFST) that started it, if one did: it goes when that status goes off.
    pub status: Option<u16>,
    row: Row,
    pub loaded: Option<Rc<Loaded>>,
    pub timeline: Timeline,
    pub controls: Controls,
    /// Where it stands (the owner's feet, or the cell).
    pub position: Vector,
    /// The STR's world point and whether its anchor is lifted 80 px.
    point: Vector,
    lifted: bool,
    pub age: u32,
    life: u32,
    over: bool,
    pub far: bool,
    shown: Option<i32>,
    longitude: Option<f32>,
}

impl World {
    /// The black fill's alpha of 255 now, when the row has one and a frame is showing.
    pub fn tint(&self) -> Option<u32> {
        if self.row.tint == 0 || self.far {
            return None;
        }
        Some(tint_alpha(self.row.tint, self.shown?.max(0) as u32))
    }

    fn last_key(&self) -> i32 {
        self.loaded.as_ref().map_or(0, |loaded| loaded.animation.last_key())
    }

    /// Runs the STR step, `steps` times for a fast-forward.
    pub fn run(&mut self, steps: u32) {
        let last = self.last_key();
        for _ in 0..steps {
            match self.timeline.step(last) {
                Step::Over => self.over = true,
                Step::Show(frame) => self.shown = Some(frame),
                Step::Wait | Step::Keep => {}
            }
        }
    }

    /// Fast-forward x: `trunc(x · frames)` steps at once.
    // The launchers' control; the per-skill issues come after this one.
    #[allow(dead_code)]
    pub fn fast_forward(&mut self, x: f32) {
        let frames = self.last_key() + 1;
        self.run((x * frames as f32) as u32);
    }

    fn dead(&self, scene: &Scene) -> bool {
        if self.life > 0 && self.age >= self.life {
            return true;
        }
        match self.owner {
            Some(unit) if self.row.owner => (scene.feet)(unit).is_none(),
            _ => self.over && !self.row.looped,
        }
    }

    /// One 16 ms tick: (2) the position, (3) the far rule, (4) the events, (5) the update, (6) the age.
    fn tick(&mut self, scene: &Scene, fired: &mut Vec<Fired>) {
        if let Some(feet) = self.owner.and_then(|unit| (scene.feet)(unit)) {
            self.position = feet;
        }
        let distance = std::array::from_fn::<f32, 3, _>(|axis| self.position[axis] - scene.player[axis]).map(|d| d * d).iter().sum::<f32>().sqrt();
        self.far = distance >= self.controls.far;
        if self.far {
            return;
        }
        for event in &self.row.events {
            match event {
                Event::Sound(tick, file) if *tick == self.age => fired.push(Fired::Sound(file.clone(), self.position)),
                Event::Quake(tick) if *tick == self.age => fired.push(Fired::Quake),
                _ => {}
            }
        }
        if self.controls.follow_camera
            && let Some(before) = self.longitude.filter(|before| *before != scene.longitude)
        {
            self.controls.turn = (self.controls.turn + before - scene.longitude).rem_euclid(360.0);
        }
        self.longitude = Some(scene.longitude);
        let head = match (self.row.head, self.owner) {
            (Head::At { dx, adjust }, Some(unit)) => (scene.body)(unit).and_then(|body| head_offset(self.id, dx, adjust, body)),
            (Head::View { dx, dy }, _) => Some([dx, dy]),
            _ => None,
        };
        self.point = match head {
            Some([right, up]) => std::array::from_fn(|axis| self.position[axis] + right * scene.right[axis] + up * scene.up[axis]),
            None => self.position,
        };
        self.lifted = head.is_none() && self.id != NO_RISE;
        self.run(1);
        self.age += 1;
    }

    /// The layers' quads now: layer 0 is never drawn, a far copy not at all.
    pub fn drawn(&self) -> Vec<Drawn> {
        let (Some(loaded), Some(frame), false) = (&self.loaded, self.shown, self.far) else { return Vec::new() };
        // The picture is moved about the point it stands at (`lift` up from it for the plain anchor), then brought down by
        // the `RISE` the renderer adds to every STR quad.
        let lift = if self.lifted { -RISE } else { 0.0 };
        let mut quads = Vec::new();
        for (index, layer) in loaded.animation.layers.iter().enumerate().skip(1) {
            let Some(state) = layer.state(frame as f32) else { continue };
            let corner = |corner: usize| {
                let [x, y] = crate::effects::str_corner(state.position, [state.corners[corner], state.corners[4 + corner]], state.angle);
                let [x, y] = transform([x, y + lift], &self.controls);
                [x, y + RISE]
            };
            let mut color = state.color;
            color[3] = (color[3] * 255.0 * self.controls.alpha).trunc() / 255.0;
            quads.push(Drawn {
                layer: index,
                texture: state.texture,
                at: self.point,
                corners: [corner(0), corner(1), corner(2), corner(3)],
                color,
                additive: state.additive,
            });
        }
        quads
    }
}

/// The running copies.
#[derive(Default)]
pub struct Worlds {
    copies: Vec<World>,
    serial: u32,
}

impl Worlds {
    pub fn is_empty(&self) -> bool {
        self.copies.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &World> {
        self.copies.iter()
    }

    pub fn get_mut(&mut self, serial: u32) -> Option<&mut World> {
        self.copies.iter_mut().find(|copy| copy.serial == serial)
    }

    /// The running copy of an owner's id.
    pub fn running(&self, owner: Option<Option<u32>>, id: u32) -> Option<u32> {
        self.copies.iter().find(|copy| copy.owner == owner && copy.id == id).map(|copy| copy.serial)
    }

    /// Whether an owner's id is still playing (running and not over).
    pub fn playing(&self, owner: Option<Option<u32>>, id: u32) -> bool {
        self.copies.iter().any(|copy| copy.owner == owner && copy.id == id && !copy.over)
    }

    /// The file a launch plays: the row's `min` file under /mineffect, a `%d` one with its number, else the row's.
    pub fn file_of(row: &Row, min: bool, roll: u32) -> Option<String> {
        match (&row.min, &row.file) {
            (Some(file), _) if min => Some(file.clone()),
            (_, Source::Named(file)) => Some(file.replace("%d", &(roll + 1).to_string())),
            _ => None,
        }
    }

    /// Starts a copy, or returns the running one for a `single` row or a `unique` launch; a timed launch refuses a second
    /// one (None). `load` reads the chosen file.
    pub fn launch(&mut self, row: &Row, launch: &Launch, load: impl FnOnce(&str) -> Option<Rc<Loaded>>) -> Option<u32> {
        let timed = launch.life > 0;
        if (row.single || launch.unique || timed)
            && let Some(serial) = self.running(launch.owner, launch.id)
        {
            return (!timed).then_some(serial);
        }
        let mut row = row.clone();
        if timed {
            row.single = true;
            row.looped = true;
        }
        let loaded = Self::file_of(&row, launch.min, launch.roll).and_then(|file| load(&file));
        self.serial += 1;
        let timeline = Timeline { looped: row.looped, delay: row.delay, ..Timeline::default() };
        self.copies.push(World {
            serial: self.serial,
            id: launch.id,
            owner: launch.owner,
            status: None,
            row,
            loaded,
            timeline,
            controls: Controls::default(),
            position: launch.point,
            point: launch.point,
            lifted: launch.id != NO_RISE,
            age: 0,
            life: launch.life,
            over: false,
            far: false,
            shown: None,
            longitude: None,
        });
        Some(self.serial)
    }

    /// Removes the copy of an owner's id.
    pub fn remove(&mut self, owner: Option<Option<u32>>, id: u32) {
        self.copies.retain(|copy| !(copy.owner == owner && copy.id == id));
    }

    /// Removes all of an owner's.
    pub fn remove_owner(&mut self, owner: Option<u32>) {
        self.copies.retain(|copy| copy.owner != Some(owner));
    }

    /// Removes the copies a unit's status started: that status went off, or (`status` None) the unit went.
    pub fn remove_status(&mut self, unit: Option<u32>, status: Option<u16>) {
        self.copies.retain(|copy| !(copy.owner == Some(unit) && copy.status.is_some() && (status.is_none() || copy.status == status)));
    }

    /// One 16 ms tick for every copy, before the units: (1) the dead go, then each copy runs its steps.
    pub fn tick(&mut self, scene: &Scene) -> Vec<Fired> {
        let mut fired = Vec::new();
        self.copies.retain(|copy| !copy.dead(scene));
        for copy in &mut self.copies {
            copy.tick(scene, &mut fired);
        }
        fired
    }

    /// The owners whose head rule a tick needs.
    pub fn head_owners(&self) -> Vec<Option<u32>> {
        self.copies.iter().filter(|copy| matches!(copy.row.head, Head::At { .. })).filter_map(|copy| copy.owner).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustnarok_client::effect::{Key, Layer, Str};

    fn table() -> Table {
        Table::load()
    }

    #[test]
    fn the_table_has_its_rows() {
        let table = table();
        let rows = table.rows();
        assert_eq!(rows.len(), 1312);
        let lines: Vec<&str> = include_str!("world_str.txt").lines().filter(|line| !line.starts_with('#') && !line.is_empty()).collect();
        let ids: Vec<u32> = lines.iter().map(|line| line.split('\t').next().unwrap().parse().unwrap()).collect();
        assert!(ids.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(lines.iter().all(|line| line.split('\t').count() == 7));
        // ponytail: the issue counts 265 rows of `single,loop,owner`; the data it gives has 360.
        let flags = |row: &Row| row.single && row.looped && row.owner;
        assert_eq!(rows.values().filter(|row| flags(row)).count(), 360);
        assert_eq!(rows.values().filter(|row| row.head == Head::Hat).count(), 264);
        assert!(table.get(1566).unwrap().min.is_some() && table.get(1654).unwrap().min.is_some());
        assert!(flags(table.get(1654).unwrap()) && table.get(1654).unwrap().head == Head::At { dx: 0.0, adjust: -3.0 });
        assert_eq!(table.get(1035).unwrap().head, Head::At { dx: 5.0, adjust: 5.0 });
        assert!(flags(table.get(1012).unwrap()));
        assert!(table.get(1817).is_some());
        assert_eq!(table.get(1163).unwrap().events, vec![Event::Sound(0, "effect\\wizard_meteor.wav".into()), Event::Quake(40)]);
        assert_eq!(table.get(141).unwrap().file, Source::Named("Pneuma%d".into()));
    }

    #[test]
    fn the_old_str_rows_are_gone() {
        let effects = crate::effects::Table::load();
        for id in [1062, 1873, 1874, 1875, 1877, 1878, 1879, 1880, 1881, 1882, 1883] {
            assert!(!effects.has_str(id), "{id}");
            assert!(effects.world(id).is_some(), "{id}");
        }
        // The ten ids with an `effects.txt` row keep playing as they do.
        for id in [141, 204, 205, 206, 207, 208, 209, 210, 211, 1094] {
            assert!(effects.has_str(id), "{id}");
            assert!(effects.world(id).is_none(), "{id}");
        }
    }

    /// Plays an STR whose last key is `last`, step by step.
    fn steps(timeline: &mut Timeline, last: i32, ticks: usize) -> Vec<Step> {
        (0..ticks).map(|_| timeline.step(last)).collect()
    }

    fn show(frames: &[i32]) -> Vec<Step> {
        frames.iter().map(|&frame| Step::Show(frame)).collect()
    }

    #[test]
    fn an_str_shows_its_frames_and_ends() {
        let mut timeline = Timeline::default();
        let mut expected = show(&[0, 1, 2, 3, 4, 5]);
        expected.push(Step::Over);
        assert_eq!(steps(&mut timeline, 5, 7), expected);
        // Looping starts again at once.
        let mut timeline = Timeline { looped: true, ..Timeline::default() };
        assert_eq!(steps(&mut timeline, 5, 8), show(&[0, 1, 2, 3, 4, 5, 0, 1]));
        let mut timeline = Timeline { looped: true, ..Timeline::default() };
        let run = steps(&mut timeline, 100, 203);
        assert_eq!((run[100], run[101], run[202]), (Step::Show(100), Step::Show(0), Step::Show(0)));
        // Hold keeps the last frame.
        let mut timeline = Timeline { hold: true, ..Timeline::default() };
        let run = steps(&mut timeline, 5, 9);
        assert_eq!(&run[6..], &[Step::Keep; 3]);
    }

    #[test]
    fn repeats_play_it_again() {
        let mut timeline = Timeline { repeats: 2, ..Timeline::default() };
        let run = steps(&mut timeline, 2, 10);
        let mut expected = show(&[0, 1, 2, 0, 1, 2, 0, 1, 2]);
        expected.push(Step::Over);
        assert_eq!(run, expected);
    }

    #[test]
    fn a_cut_ends_it_early() {
        for (cut, frame) in [(100, 6), (17, 1), (34, 2), (35, 3)] {
            let mut timeline = Timeline { cut, ..Timeline::default() };
            let run = steps(&mut timeline, 50, 20);
            let ended = run.iter().position(|step| *step == Step::Over).unwrap();
            assert_eq!(ended, frame, "{cut} ms");
        }
    }

    #[test]
    fn a_start_delay_holds_the_first_step() {
        let mut timeline = Timeline { delay: 100, ..Timeline::default() };
        let run = steps(&mut timeline, 5, 9);
        assert!(run[..7].iter().all(|step| *step == Step::Wait));
        assert_eq!(&run[7..], &show(&[0, 1]));
    }

    fn key(frame: i32, kind: u32) -> Key {
        Key {
            frame,
            kind,
            position: [320.0, 240.0],
            corners: [-10.0, 10.0, 10.0, -10.0, -10.0, -10.0, 10.0, 10.0],
            texture: 0.0,
            texture_motion: 0,
            delay: 0.0,
            angle: 0.0,
            color: [1.0; 4],
            source_blend: 5,
            destination_blend: 6,
        }
    }

    /// A small in-memory STR: layer 0 (never drawn) and layers 1, 2 showing from frame 0 to `last`.
    fn small(last: i32) -> Rc<Loaded> {
        let layer = || Layer { textures: vec!["a.bmp".into()], keys: vec![key(0, 0), key(last, 0)] };
        Rc::new(Loaded { animation: Str { fps: 60, frames: last as u32, layers: vec![layer(), layer(), layer()] }, textures: vec![Vec::new(); 3] })
    }

    fn row(flags: &str, events: Vec<Event>) -> Row {
        let has = |name: &str| flags.split(',').any(|each| each == name);
        Row {
            file: Source::Named("a/b".into()),
            min: None,
            single: has("single"),
            looped: has("loop"),
            owner: has("owner"),
            head: Head::Feet,
            tint: 0,
            events,
            delay: 0,
        }
    }

    fn launch(id: u32, owner: Option<Option<u32>>) -> Launch {
        Launch { id, owner, point: [0.0; 3], unique: false, life: 0, min: false, roll: 0 }
    }

    /// Ticks `worlds` `count` times with the player at `player` and these units' feet.
    fn run(worlds: &mut Worlds, count: usize, player: Vector, units: &HashMap<u32, Vector>) -> Vec<Fired> {
        let feet = |unit: Option<u32>| match unit {
            None => Some(player),
            Some(id) => units.get(&id).copied(),
        };
        let body = |_: Option<u32>| None;
        let scene = Scene { player, feet: &feet, body: &body, longitude: 0.0, right: [1.0, 0.0, 0.0], up: [0.0, 1.0, 0.0], ground: &|_, _| 0.0 };
        (0..count).flat_map(|_| worlds.tick(&scene)).collect()
    }

    #[test]
    fn the_events_fire_at_their_age_whatever_the_delay() {
        let mut worlds = Worlds::default();
        let row = row("", vec![Event::Sound(30, "effect\\a.wav".into()), Event::Quake(30)]);
        let serial = worlds.launch(&row, &launch(1, None), |_| Some(small(100))).unwrap();
        worlds.get_mut(serial).unwrap().timeline.delay = 500;
        assert!(run(&mut worlds, 30, [0.0; 3], &HashMap::new()).is_empty());
        let fired = run(&mut worlds, 1, [0.0; 3], &HashMap::new());
        assert_eq!(fired, vec![Fired::Sound("effect\\a.wav".into(), [0.0; 3]), Fired::Quake]);
        assert!(run(&mut worlds, 5, [0.0; 3], &HashMap::new()).is_empty());
    }

    #[test]
    fn a_far_copy_stands_still() {
        let mut worlds = Worlds::default();
        let serial = worlds.launch(&row("loop", Vec::new()), &launch(1, None), |_| Some(small(100))).unwrap();
        let near = [129.9, 0.0, 0.0];
        run(&mut worlds, 3, near, &HashMap::new());
        assert_eq!(worlds.get_mut(serial).unwrap().age, 3);
        let at = [130.0, 0.0, 0.0];
        run(&mut worlds, 3, at, &HashMap::new());
        let copy = worlds.get_mut(serial).unwrap();
        assert_eq!(copy.age, 3);
        assert!(copy.far && copy.drawn().is_empty());
        run(&mut worlds, 2, near, &HashMap::new());
        let copy = worlds.get_mut(serial).unwrap();
        assert_eq!(copy.age, 5);
        assert!(!copy.far && !copy.drawn().is_empty());
    }

    #[test]
    fn a_far_copy_fires_no_events() {
        let mut worlds = Worlds::default();
        let row = row("", vec![Event::Quake(0)]);
        worlds.launch(&row, &launch(1, None), |_| Some(small(100)));
        assert!(run(&mut worlds, 2, [200.0, 0.0, 0.0], &HashMap::new()).is_empty());
        assert_eq!(run(&mut worlds, 1, [0.0; 3], &HashMap::new()), vec![Fired::Quake]);
    }

    #[test]
    fn single_and_unique_return_the_running_copy() {
        let mut worlds = Worlds::default();
        let single = row("single", Vec::new());
        let first = worlds.launch(&single, &launch(1, Some(Some(7))), |_| None).unwrap();
        assert_eq!(worlds.launch(&single, &launch(1, Some(Some(7))), |_| panic!("loaded again")), Some(first));
        let second = worlds.launch(&single, &launch(1, Some(Some(8))), |_| None).unwrap();
        assert_ne!(first, second);
        // A plain row makes a copy each launch, unless the launch is unique.
        let plain = row("", Vec::new());
        let a = worlds.launch(&plain, &launch(2, None), |_| None).unwrap();
        let b = worlds.launch(&plain, &launch(2, None), |_| None).unwrap();
        assert_ne!(a, b);
        let unique = Launch { unique: true, ..launch(2, None) };
        assert!(worlds.launch(&plain, &unique, |_| None).is_some_and(|serial| serial == a || serial == b));
        // A timed launch makes the row single and loop, and refuses a second.
        let timed = Launch { life: 10, ..launch(3, None) };
        let serial = worlds.launch(&plain, &timed, |_| None).unwrap();
        assert!(worlds.get_mut(serial).unwrap().row.looped && worlds.get_mut(serial).unwrap().row.single);
        assert_eq!(worlds.launch(&plain, &timed, |_| None), None);
    }

    #[test]
    fn a_launch_picks_its_file() {
        let mut named = row("", Vec::new());
        named.file = Source::Named("Pneuma%d".into());
        named.min = Some("small".into());
        let files: Vec<String> = (0..3).map(|roll| Worlds::file_of(&named, false, roll).unwrap()).collect();
        assert_eq!(files, ["Pneuma1", "Pneuma2", "Pneuma3"]);
        assert_eq!(Worlds::file_of(&named, true, 0).as_deref(), Some("small"));
        named.file = Source::Launch;
        named.min = None;
        assert_eq!(Worlds::file_of(&named, false, 0), None);
    }

    #[test]
    fn a_follower_moves_with_its_owner_and_a_cell_copy_stays() {
        let mut worlds = Worlds::default();
        let follower = worlds.launch(&row("loop", Vec::new()), &launch(1, Some(Some(7))), |_| None).unwrap();
        let cell = worlds.launch(&row("loop", Vec::new()), &Launch { point: [5.0, 0.0, 5.0], ..launch(2, None) }, |_| None).unwrap();
        let mut units = HashMap::from([(7, [1.0, 2.0, 3.0])]);
        run(&mut worlds, 1, [0.0; 3], &units);
        units.insert(7, [4.0, 2.0, 6.0]);
        run(&mut worlds, 1, [0.0; 3], &units);
        assert_eq!(worlds.get_mut(follower).unwrap().position, [4.0, 2.0, 6.0]);
        assert_eq!(worlds.get_mut(cell).unwrap().position, [5.0, 0.0, 5.0]);
    }

    #[test]
    fn lifetimes() {
        let units = HashMap::from([(7, [0.0; 3])]);
        let mut worlds = Worlds::default();
        // An owner's copy outlives its STR and ends when the unit goes.
        let owned = worlds.launch(&row("owner", Vec::new()), &launch(1, Some(Some(7))), |_| Some(small(2))).unwrap();
        // The other kind is removed one tick after its STR is over; a looping one is not.
        let plain = worlds.launch(&row("", Vec::new()), &launch(2, None), |_| Some(small(2))).unwrap();
        let looping = worlds.launch(&row("loop", Vec::new()), &launch(3, None), |_| Some(small(2))).unwrap();
        run(&mut worlds, 3, [0.0; 3], &units);
        assert!(worlds.get_mut(plain).is_some());
        run(&mut worlds, 1, [0.0; 3], &units);
        assert!(worlds.get_mut(plain).is_some(), "over on its fourth step");
        run(&mut worlds, 1, [0.0; 3], &units);
        assert!(worlds.get_mut(plain).is_none());
        assert!(worlds.get_mut(owned).is_some() && worlds.get_mut(looping).is_some());
        run(&mut worlds, 20, [0.0; 3], &units);
        assert!(worlds.get_mut(owned).is_some() && worlds.get_mut(looping).is_some());
        run(&mut worlds, 1, [0.0; 3], &HashMap::new());
        assert!(worlds.get_mut(owned).is_none() && worlds.get_mut(looping).is_some());
        // A timed copy lives its `life`.
        let timed = worlds.launch(&row("", Vec::new()), &Launch { life: 3, ..launch(4, None) }, |_| Some(small(100))).unwrap();
        run(&mut worlds, 3, [0.0; 3], &units);
        assert!(worlds.get_mut(timed).is_some());
        run(&mut worlds, 1, [0.0; 3], &units);
        assert!(worlds.get_mut(timed).is_none());
    }

    #[test]
    fn removals() {
        let mut worlds = Worlds::default();
        worlds.launch(&row("single", Vec::new()), &launch(1, Some(Some(7))), |_| None);
        worlds.launch(&row("single", Vec::new()), &launch(2, Some(Some(7))), |_| None);
        worlds.launch(&row("single", Vec::new()), &launch(1, Some(Some(8))), |_| None);
        worlds.remove(Some(Some(7)), 1);
        assert_eq!(worlds.iter().count(), 2);
        worlds.remove_owner(Some(7));
        assert_eq!(worlds.iter().map(|copy| copy.owner).collect::<Vec<_>>(), vec![Some(Some(8))]);
    }

    #[test]
    fn a_status_copy_goes_with_its_status_or_its_unit() {
        let mut worlds = Worlds::default();
        let on = |worlds: &mut Worlds, id: u32, unit: Option<u32>, status: u16| {
            let serial = worlds.launch(&row("loop", Vec::new()), &Launch { unique: true, ..launch(id, Some(unit)) }, |_| None).unwrap();
            worlds.get_mut(serial).unwrap().status = Some(status);
        };
        on(&mut worlds, 1, Some(7), 100);
        on(&mut worlds, 2, Some(7), 200);
        on(&mut worlds, 1, None, 100);
        // Not from a status: stays.
        worlds.launch(&row("loop", Vec::new()), &launch(3, Some(Some(7))), |_| None);
        // A status sent again keeps its one copy.
        on(&mut worlds, 1, Some(7), 100);
        assert_eq!(worlds.iter().count(), 4);
        worlds.remove_status(Some(7), Some(100));
        assert_eq!(worlds.iter().map(|copy| (copy.id, copy.owner)).collect::<Vec<_>>(), [(2, Some(Some(7))), (1, Some(None)), (3, Some(Some(7)))]);
        worlds.remove_status(Some(7), None);
        assert_eq!(worlds.iter().map(|copy| (copy.id, copy.owner)).collect::<Vec<_>>(), [(1, Some(None)), (3, Some(Some(7)))]);
    }

    #[test]
    fn the_picture_scales_mirrors_and_turns_about_its_point() {
        let controls = Controls { scale: 2.0, ..Controls::default() };
        assert_eq!(transform([3.0, -4.0], &controls), [6.0, -8.0]);
        let controls = Controls { mirror_x: true, ..Controls::default() };
        assert_eq!(transform([3.0, -4.0], &controls), [-3.0, -4.0]);
        let controls = Controls { mirror_y: true, ..Controls::default() };
        assert_eq!(transform([3.0, -4.0], &controls), [3.0, 4.0]);
        let controls = Controls { turn: 90.0, ..Controls::default() };
        let [x, y] = transform([1.0, 0.0], &controls);
        assert!(x.abs() < 1e-6 && (y - 1.0).abs() < 1e-6);
        let controls = Controls { offset: [2.0, 3.0], ..Controls::default() };
        assert_eq!(transform([1.0, 1.0], &controls), [3.0, 4.0]);
    }

    #[test]
    fn the_alpha_multiplier_scales_the_colour() {
        let mut worlds = Worlds::default();
        let serial = worlds.launch(&row("loop", Vec::new()), &launch(1, None), |_| Some(small(10))).unwrap();
        worlds.get_mut(serial).unwrap().controls.alpha = 0.5;
        run(&mut worlds, 1, [0.0; 3], &HashMap::new());
        let drawn = worlds.get_mut(serial).unwrap().drawn();
        // Layers 1 and 2 draw, layer 0 never.
        assert_eq!(drawn.iter().map(|quad| quad.layer).collect::<Vec<_>>(), vec![1, 2]);
        assert_eq!((drawn[0].color[3] * 255.0).round(), 127.0);
    }

    #[test]
    fn a_scaled_copy_grows_about_its_feet() {
        let mut worlds = Worlds::default();
        let serial = worlds.launch(&row("loop", Vec::new()), &launch(1, None), |_| Some(small(10))).unwrap();
        run(&mut worlds, 1, [0.0; 3], &HashMap::new());
        let plain = worlds.get_mut(serial).unwrap().drawn();
        worlds.get_mut(serial).unwrap().controls.scale = 2.0;
        let scaled = worlds.get_mut(serial).unwrap().drawn();
        // About the feet, which sit 80 px under the STR's anchor (the renderer's lift is added back to every corner).
        for (before, after) in plain[0].corners.iter().zip(&scaled[0].corners) {
            assert!((after[0] - before[0] * 2.0).abs() < 1e-4 && (after[1] - RISE - (before[1] - RISE) * 2.0).abs() < 1e-4, "{before:?} {after:?}");
        }
    }

    #[test]
    fn the_camera_turns_a_following_copy() {
        let mut worlds = Worlds::default();
        let serial = worlds.launch(&row("loop", Vec::new()), &launch(1, None), |_| None).unwrap();
        worlds.get_mut(serial).unwrap().controls.follow_camera = true;
        let feet = |_: Option<u32>| Some([0.0; 3]);
        let body = |_: Option<u32>| None;
        for longitude in [10.0, 30.0, 5.0] {
            let scene = Scene { player: [0.0; 3], feet: &feet, body: &body, longitude, right: [1.0, 0.0, 0.0], up: [0.0, 1.0, 0.0], ground: &|_, _| 0.0 };
            worlds.tick(&scene);
        }
        // 10 → 30 turns by -20, 30 → 5 by +25.
        assert_eq!(worlds.get_mut(serial).unwrap().controls.turn, 5.0);
    }

    #[test]
    fn the_head_rule() {
        let body = Body { height: 27.2, scale: 1.0, rides: false, doram: false, rot: 0.0 };
        let [right, up] = head_offset(1035, 5.0, 5.0, body).unwrap();
        assert!((right - 5.0).abs() < 1e-5 && (up - 43.2).abs() < 1e-4);
        let rider = Body { rides: true, ..body };
        assert!((head_offset(1035, 5.0, 5.0, rider).unwrap()[1] - (23.0 + 5.0 - 7.0)).abs() < 1e-4);
        let doram = Body { rides: true, doram: true, ..body };
        assert!((head_offset(1035, 0.0, 0.0, doram).unwrap()[1] - 11.0).abs() < 1e-4);
        assert!((head_offset(1817, 0.0, 0.0, body).unwrap()[1] - 11.0).abs() < 1e-4);
        // Both negative: nothing moves.
        let low = Body { height: -40.0, ..body };
        assert_eq!(head_offset(1035, -1.0, 0.0, low), None);
        // The size factor scales the offset.
        let big = Body { scale: 2.0, ..body };
        assert!((head_offset(1035, 5.0, 5.0, big).unwrap()[1] - 86.4).abs() < 1e-3);
        // The estimate of the head height.
        assert!((Body::from_sprite(Some(-80.0), 1.0, false, false).height - 27.2).abs() < 1e-4);
        assert!((Body::from_sprite(None, 1.0, false, false).height - -4.7).abs() < 1e-4);
    }

    #[test]
    fn the_head_rule_lifts_the_point_along_the_camera() {
        let mut head = row("loop", Vec::new());
        head.head = Head::At { dx: 5.0, adjust: 5.0 };
        let mut worlds = Worlds::default();
        let serial = worlds.launch(&head, &launch(1035, Some(Some(7))), |_| Some(small(10))).unwrap();
        let feet = |_: Option<u32>| Some([10.0, 0.0, 10.0]);
        let body = |_: Option<u32>| Some(Body { height: 27.2, scale: 1.0, rides: false, doram: false, rot: 0.0 });
        let scene =
            Scene { player: [10.0, 0.0, 10.0], feet: &feet, body: &body, longitude: 0.0, right: [0.0, 0.0, 1.0], up: [0.0, 1.0, 0.0], ground: &|_, _| 0.0 };
        worlds.tick(&scene);
        let copy = worlds.get_mut(serial).unwrap();
        assert!((copy.point[2] - 15.0).abs() < 1e-4 && (copy.point[1] - 43.2).abs() < 1e-3 && copy.point[0] == 10.0);
        assert!(!copy.lifted);
    }

    #[test]
    fn the_view_head_moves_the_point_along_the_camera() {
        let table = table();
        assert_eq!(table.get(10010).unwrap().head, Head::View { dx: 0.0, dy: 10.0 });
        let mut worlds = Worlds::default();
        let serial = worlds.launch(table.get(10010).unwrap(), &launch(10010, Some(Some(7))), |_| Some(small(29))).unwrap();
        let feet = |_: Option<u32>| Some([10.0, 0.0, 10.0]);
        // No body is needed: no base, adjustment or size factor.
        let body = |_: Option<u32>| None;
        let scene =
            Scene { player: [10.0, 0.0, 10.0], feet: &feet, body: &body, longitude: 0.0, right: [0.0, 0.0, 1.0], up: [0.0, 0.6, 0.8], ground: &|_, _| 0.0 };
        worlds.tick(&scene);
        let copy = worlds.get_mut(serial).unwrap();
        assert_eq!(copy.point, [10.0, 6.0, 18.0]);
        // The anchor is the point itself, with no 80 px rise.
        assert!(!copy.lifted);
        // dx goes along the camera's right.
        let mut side = row("", Vec::new());
        side.head = Head::View { dx: 4.0, dy: 0.0 };
        let serial = worlds.launch(&side, &launch(1, Some(Some(7))), |_| Some(small(29))).unwrap();
        worlds.tick(&scene);
        assert_eq!(worlds.get_mut(serial).unwrap().point, [10.0, 0.0, 14.0]);
        assert!(!worlds.head_owners().contains(&Some(7)));
    }

    #[test]
    fn a_row_delay_holds_every_launch() {
        let mut delayed = row("", Vec::new());
        delayed.delay = 100;
        let mut worlds = Worlds::default();
        let serial = worlds.launch(&delayed, &launch(1, None), |_| Some(small(13))).unwrap();
        assert_eq!(worlds.get_mut(serial).unwrap().timeline.delay, 100);
    }

    #[test]
    fn the_tint_grows_to_its_cap() {
        assert_eq!(tint_alpha(2, 0), 0);
        assert_eq!(tint_alpha(2, 50), 100);
        assert_eq!(tint_alpha(2, 85), 170);
        assert_eq!(tint_alpha(2, 200), 170);
        assert_eq!(tint_alpha(16, 10), 160);
        assert_eq!(tint_alpha(16, 11), 170);
        let mut tinted = row("loop", Vec::new());
        tinted.tint = 16;
        let mut worlds = Worlds::default();
        let serial = worlds.launch(&tinted, &launch(1, None), |_| Some(small(50))).unwrap();
        assert_eq!(worlds.get_mut(serial).unwrap().tint(), None);
        run(&mut worlds, 12, [0.0; 3], &HashMap::new());
        assert_eq!(worlds.get_mut(serial).unwrap().tint(), Some(170));
    }

    #[test]
    fn a_fast_forward_runs_steps_at_once() {
        let mut worlds = Worlds::default();
        let serial = worlds.launch(&row("loop", Vec::new()), &launch(1, None), |_| Some(small(9))).unwrap();
        let copy = worlds.get_mut(serial).unwrap();
        copy.fast_forward(0.5);
        assert_eq!(copy.shown, Some(4));
    }
}
