//! The ninja jobs' objects: the charm orbs circling a Kagerou or Oboro (228, 735 – 738) and the fog puffs of Moonlight
//! Fantasy (1005), Swirling Petal's kunai ring (984), cloud ribbons (986, 1000) and petals (1002), Kunai Explosion's burst
//! (990), Shadow Trampling's square (991) and Makibishi's caltrops (1007).

use super::{Assets, Card, Class, Eye, FLAGS_EFFECT, Face, Fx, Launch, Seen, Tex, WorldFx, padded_ratio, triangles, yawed};
use crate::world::Vector;
use crate::world_str::Scene;

/// The charm orbs: thunder (kind 5), fire, wind, water, earth.
pub const THUNDER: u32 = 228;
pub const FIRE: u32 = 735;
pub const WIND: u32 = 736;
pub const WATER: u32 = 737;
pub const EARTH: u32 = 738;
/// Moonlight Fantasy's ten fog puffs.
pub const FOG: u32 = 1005;
pub const KUNAI_RING: u32 = 984;
pub const DARK_RIBBONS: u32 = 986;
pub const BLUE_RIBBONS: u32 = 1000;
pub const PETALS: u32 = 1002;
pub const BURST: u32 = 990;
pub const SQUARE: u32 = 991;
pub const CALTROPS: u32 = 1007;

const ICON: &str = "data\\texture\\유저인터페이스\\item\\쿠나이_독.bmp";
const CLOUD: &str = "data\\texture\\effect\\cloud11.tga";
const FRAME: &str = "data\\texture\\effect\\SquareRange.tga";
const CALTROP: &str = "data\\texture\\effect\\makibishi.bmp";
const PETAL: &str = "data\\sprite\\이팩트\\sakura01";
const FLAME: &str = "data\\sprite\\이팩트\\m_ef03";

/// Registers the ninja objects.
pub fn register(fx: &mut WorldFx) {
    for id in [THUNDER, FIRE, WIND, WATER, EARTH, FOG] {
        fx.register(id, || Box::new(Orbs::new(pick)));
    }
    fx.register(KUNAI_RING, || Box::new(Ring::default()));
    for id in [DARK_RIBBONS, BLUE_RIBBONS] {
        fx.register(id, || Box::new(Ribbons::new(pick)));
    }
    fx.register(PETALS, || Box::new(Petals::new(pick)));
    fx.register(BURST, || Box::new(Flame::default()));
    fx.register(SQUARE, || Box::new(Frame));
    fx.register(CALTROPS, || Box::new(Caltrops::default()));
}

/// One random whole number below `n`.
fn pick(n: u32) -> u32 {
    crate::effects::pick(0, n.saturating_sub(1))
}

/// The charm orbs a charm packet starts by its kind: 1 water, 2 earth, 3 fire, 4 wind, 5 thunder.
pub fn charm_effect(kind: u16) -> Option<u32> {
    match kind {
        1 => Some(WATER),
        2 => Some(EARTH),
        3 => Some(FIRE),
        4 => Some(WIND),
        5 => Some(THUNDER),
        _ => None,
    }
}

/// Whether `effect` is one of the charm orbs, which only a charm packet starts.
pub fn is_charm(effect: u32) -> bool {
    matches!(effect, THUNDER | FIRE..=EARTH)
}

/// A status object's launch: Moonlight Fantasy's fog starts afresh each time its status comes on.
pub fn restarts_on_status(effect: u32) -> bool {
    effect == FOG
}

/// A charm packet for `owner` (`Some(None)` = the player) standing at `at` (None: not in sight): the old ring goes (not the
/// thunder one), and `count` orbs of the packet's kind take its place unless the unit is hidden or cloaked.
pub fn charms(fx: &mut WorldFx, owner: Option<Option<u32>>, (kind, count): (u16, u16), at: Option<Vector>, seen: bool, assets: &mut dyn Assets) {
    for id in [WATER, EARTH, FIRE, WIND] {
        fx.remove(owner, id);
    }
    if let (true, true, Some(at), Some(id)) = (count > 0, seen, at, charm_effect(kind)) {
        fx.launch(&Launch { a7: Some(i32::from(count)), ..Launch::at(id, owner, at) }, assets);
    }
}

fn sin_cos(degrees: f32) -> (f32, f32) {
    degrees.to_radians().sin_cos()
}

/// A square of half-side `half` on the axes `x` and `y` through `centre`, holding a picture that fills `ratio` of its
/// power-of-two surface from the top left: the whole picture on that part of the square.
fn padded(texture: &'static str, centre: Vector, (x, y): (Vector, Vector), half: f32, ratio: [f32; 2], tint: [f32; 4], additive: bool) -> Face<'static> {
    let [right, bottom] = [-1.0 + 2.0 * ratio[0], 1.0 - 2.0 * ratio[1]];
    let corner = |cx: f32, cy: f32| -> Vector { std::array::from_fn(|axis| centre[axis] + x[axis] * cx * half + y[axis] * cy * half) };
    Face {
        texture: Tex::Path(texture),
        vertices: triangles(
            [corner(-1.0, 1.0), corner(right, 1.0), corner(-1.0, bottom), corner(right, bottom)],
            [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]],
            tint,
        ),
        additive,
        flags: FLAGS_EFFECT,
    }
}

/// The camera's latitude in degrees (looking down is positive).
fn latitude(eye: &Eye) -> f32 {
    (-eye.forward[1]).clamp(-1.0, 1.0).asin().to_degrees()
}

/// The camera's longitude in degrees.
fn longitude(eye: &Eye) -> f32 {
    (-eye.right[2]).atan2(eye.right[0]).to_degrees()
}

// The charm orbs (228, 735 – 738) and the fog puffs (1005)

/// What tells one orb id from another.
struct Look {
    texture: &'static str,
    /// The half-side scale, the turn a tick is five times `speed` degrees, the orbit's radius and height.
    s: f32,
    speed: f32,
    radius: f32,
    height: f32,
    /// The two quads of an orb: colour and blend.
    colours: [[f32; 3]; 2],
    additive: [bool; 2],
}

fn look(id: u32) -> Look {
    let orb = |texture, s| Look { texture, s, speed: 0.15, radius: 8.0, height: 12.0, colours: [[1.0; 3]; 2], additive: [false, true] };
    match id {
        THUNDER => Look { colours: [[0.0, 0.0, 1.0], [1.0, 1.0, 0.0]], ..orb("data\\texture\\effect\\thunder_center.bmp", 0.8) },
        FIRE => orb("data\\texture\\effect\\FireOrb.bmp", 2.3),
        WIND => orb("data\\texture\\effect\\LightningOrb.bmp", 2.3),
        WATER => orb("data\\texture\\effect\\WaterOrb.bmp", 1.3),
        EARTH => orb("data\\texture\\effect\\StoneOrb.bmp", 2.3),
        _ => Look { speed: 0.05, radius: 4.0, height: 8.0, additive: [false; 2], ..orb("data\\texture\\effect\\fog1.tga", 6.0) },
    }
}

/// One of an orb's two quads.
#[derive(Debug, Clone, PartialEq)]
struct Piece {
    /// The orbit angle in degrees, and the bob's phase in whole degrees and its step.
    theta: f32,
    p: i32,
    b: i32,
    /// Its own counter, +1 an update.
    c: u32,
    alpha: i32,
    half: f32,
    /// From the feet, in the camera-yawed frame.
    offset: Vector,
}

impl Piece {
    /// One update, for the orb's quad `k`.
    fn step(&mut self, look: &Look, k: usize) {
        if self.c <= 200 {
            self.alpha += 1;
        }
        self.theta += 5.0 * look.speed;
        if self.theta > 360.0 {
            self.theta -= 360.0;
        }
        self.p = (self.p + self.b) % 360;
        let sin = sin_cos(self.p as f32).0;
        self.half = look.s * (if k == 0 { 1.1 } else { 1.0 } + 0.1 * sin);
        let radius = look.radius + 0.6 * sin;
        let height = look.height - 0.6 * self.b as f32 * sin;
        let (sin_theta, cos_theta) = sin_cos(self.theta);
        self.offset = [radius * cos_theta, height, radius * sin_theta];
        self.c += 1;
    }
}

/// Orbs hovering round the unit's feet, bobbing and turning: two quads each.
struct Orbs {
    rand: fn(u32) -> u32,
    look: Option<Look>,
    ratio: [f32; 2],
    orbs: Vec<[Piece; 2]>,
}

impl Orbs {
    fn new(rand: fn(u32) -> u32) -> Self {
        Self { rand, look: None, ratio: [1.0; 2], orbs: Vec::new() }
    }

    /// Makes the orbs a count of `n` lacks, spread over the circle: each draws its phase, then its step (the fog's is 0, the
    /// draw is still made). It never takes an orb away.
    fn set_count(&mut self, n: usize, fog: bool) {
        let apart = if n == 0 { 0 } else { 360 / n as i32 };
        for index in self.orbs.len()..n {
            let p = (self.rand)(360) as i32;
            let b = (self.rand)(3) as i32;
            let b = if fog { 0 } else { b };
            let piece = Piece { theta: (apart * index as i32) as f32, p, b, c: 0, alpha: 0, half: 0.0, offset: [0.0; 3] };
            self.orbs.push([piece.clone(), piece]);
        }
    }
}

impl Class for Orbs {
    fn begin(&mut self, fx: &mut Fx, assets: &mut dyn Assets) {
        let look = look(fx.id);
        self.ratio = assets.texture(look.texture).map_or([1.0; 2], padded_ratio);
        self.look = Some(look);
        let n = if fx.id == FOG { 10 } else { fx.a7.max(0) as usize };
        self.set_count(n, fx.id == FOG);
    }

    fn update(&mut self, _: &mut Fx, _: &Scene, _: &mut dyn Assets) {
        let Some(look) = &self.look else { return };
        for orb in &mut self.orbs {
            for (k, piece) in orb.iter_mut().enumerate() {
                piece.step(look, k);
            }
        }
    }

    fn draw<'a>(&'a self, fx: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        let Some(look) = &self.look else { return };
        for orb in &self.orbs {
            for (k, piece) in orb.iter().enumerate() {
                let centre = yawed(fx.position, piece.offset, &seen.eye);
                let [r, g, b] = look.colours[k];
                let tint = [r, g, b, f32::from((piece.alpha & 0xff) as u8) / 255.0];
                out.push(padded(look.texture, centre, (seen.eye.right, seen.eye.up), piece.half, self.ratio, tint, look.additive[k]));
            }
        }
    }

    fn alive(&self, fx: &Fx, scene: &Scene) -> bool {
        fx.owned(scene)
    }

    fn any(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}

// Swirling Petal: the kunai ring (984), the cloud ribbons (986, 1000) and the petals (1002)

/// A picture standing in the world like an item's: tipped by the camera's latitude (degrees), turned about the vertical by
/// `spin` (degrees), one unit behind its point, upside down when face-on; a square of side 2 · `scale` of which the picture
/// fills `ratio` from the top left.
fn item_corners(at: Vector, spin: f32, scale: f32, latitude: f32, ratio: [f32; 2]) -> [(Vector, [f32; 2]); 4] {
    let (s, c) = sin_cos(-latitude);
    let (sigma_sin, sigma_cos) = sin_cos(spin);
    let [right, bottom] = [-1.0 + 2.0 * ratio[0], 1.0 - 2.0 * ratio[1]];
    [((-1.0, 1.0), [0.0, 0.0]), ((right, 1.0), [1.0, 0.0]), ((-1.0, bottom), [0.0, 1.0]), ((right, bottom), [1.0, 1.0])].map(|((lx, ly), uv)| {
        let w = scale * ly * s + c;
        ([at[0] + scale * lx * sigma_cos + w * sigma_sin, at[1] + s - scale * ly * c, at[2] - scale * lx * sigma_sin + w * sigma_cos], uv)
    })
}

/// The ring's alphas: the lead and its three ghosts.
const RING_ALPHAS: [f32; 4] = [255.0, 105.0, 80.0, 55.0];
const RING_RADIUS: f32 = 13.0;
const RING_HEIGHT: f32 = 8.0;
const RING_SCALE: f32 = 2.5;
/// The ring shows nothing for its first updates.
const RING_WAIT: u32 = 5;

/// Four poison kunai circling the unit, a lead and three fading copies behind it.
struct Ring {
    /// The angle of the lead and of each ghost, in whole degrees.
    angles: [i32; 4],
    ratio: [f32; 2],
    shown: bool,
}

impl Default for Ring {
    fn default() -> Self {
        Self { angles: [360, 0, 0, 0], ratio: [1.0; 2], shown: false }
    }
}

impl Class for Ring {
    fn begin(&mut self, _: &mut Fx, assets: &mut dyn Assets) {
        self.ratio = assets.texture(ICON).map_or([1.0; 2], padded_ratio);
    }

    fn update(&mut self, fx: &mut Fx, _: &Scene, _: &mut dyn Assets) {
        if fx.age < RING_WAIT {
            return;
        }
        self.shown = true;
        self.angles[0] = (0.4 * 45.0 + self.angles[0] as f32).trunc() as i32 % 360;
        for ghost in 1..4 {
            self.angles[ghost] = (self.angles[ghost - 1] - 5).rem_euclid(360);
        }
    }

    fn draw<'a>(&'a self, fx: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        if !self.shown {
            return;
        }
        let (latitude, longitude) = (latitude(&seen.eye), longitude(&seen.eye));
        for (angle, alpha) in self.angles.into_iter().zip(RING_ALPHAS) {
            let (sin, cos) = sin_cos(angle as f32);
            let centre = yawed(fx.position, [RING_RADIUS * cos, RING_HEIGHT, RING_RADIUS * sin], &seen.eye);
            // The spin is against the camera: the world's adds its longitude.
            let spin = ((angle + 45) % 360) as f32 + longitude;
            let tint = [1.0, 200.0 / 255.0, 1.0, alpha / 255.0];
            let corners = item_corners(centre, spin, RING_SCALE, latitude, self.ratio);
            out.push(Face {
                texture: Tex::Path(ICON),
                vertices: triangles(corners.map(|(position, _)| position), corners.map(|(_, uv)| uv), tint),
                additive: false,
                flags: FLAGS_EFFECT,
            });
        }
    }

    fn any(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}

const ROWS: usize = 4;
const SEGMENTS: usize = 21;

/// One point of a ribbon: its two edges from the feet and the quad's alpha.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Edge {
    inner: Vector,
    outer: Vector,
    alpha: i32,
}

/// The ribbon's angle, span (degrees), radius and alpha at update `m` for row `r`, segment `j`; `wide` is the blue one.
fn ribbon_state(wide: bool, r: usize, j: usize, m: u32) -> (i32, i32, f32, i32) {
    let (m, j, r) = (m as i32, j as i32, r as i32);
    let angle = (90 * r + 5 * (m + 1) + 5 * j) % 360;
    let span = (30 + 3 * (m + 1) + 3 * j).min(180);
    let start = (7 - r) as f32 + if wide { 3.0 } else { 0.0 };
    let radius = if m <= 12 { start } else { start + 0.1 * (m - 12) as f32 + 0.1 * j as f32 };
    let alpha = match m {
        ..=12 => 51 + m + j,
        13..=50 => 63 + j,
        _ => (113 - m - j).max(0),
    };
    (angle, span, radius, alpha)
}

/// The edges of ribbon segment `j` of row `r`, leaning at `lean` degrees.
fn ribbon_edge(wide: bool, r: usize, j: usize, m: u32, lean: f32) -> Edge {
    let (angle, span, radius, alpha) = ribbon_state(wide, r, j, m);
    let theta = ((span / 21) * j as i32 + angle) % 360;
    let (sin, cos) = sin_cos(theta as f32);
    let (lean_sin, lean_cos) = sin_cos(lean);
    let height = 28.0 - 5.0 * r as f32;
    Edge { inner: [radius * cos, height, radius * sin], outer: [(radius + lean_cos) * cos, height - 3.0 + lean_sin, (radius + lean_cos) * sin], alpha }
}

/// Four rows of a 20-segment cloud ribbon whirling round the unit: dark grey (986) or blue and adding (1000).
struct Ribbons {
    rand: fn(u32) -> u32,
    wide: bool,
    /// Each row's lean in degrees.
    lean: [f32; ROWS],
    rows: Vec<[Edge; SEGMENTS]>,
}

impl Ribbons {
    fn new(rand: fn(u32) -> u32) -> Self {
        Self { rand, wide: false, lean: [80.0; ROWS], rows: vec![[Edge::default(); SEGMENTS]; ROWS] }
    }
}

impl Class for Ribbons {
    fn begin(&mut self, fx: &mut Fx, assets: &mut dyn Assets) {
        assets.texture(CLOUD);
        self.wide = fx.id == BLUE_RIBBONS;
        for lean in &mut self.lean {
            *lean = 80.0 + (self.rand)(21) as f32;
        }
    }

    fn update(&mut self, fx: &mut Fx, _: &Scene, _: &mut dyn Assets) {
        for (r, row) in self.rows.iter_mut().enumerate() {
            for (j, edge) in row.iter_mut().enumerate() {
                *edge = ribbon_edge(self.wide, r, j, fx.age, self.lean[r]);
            }
        }
    }

    /// Segment j is the strip between points j − 1 and j; the outer edge is v 0, the inner edge v 1.
    fn draw<'a>(&'a self, fx: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        let colour = if self.wide { [100.0 / 255.0, 100.0 / 255.0, 1.0] } else { [50.0 / 255.0; 3] };
        let at = |offset: Vector| yawed(fx.position, offset, &seen.eye);
        for row in &self.rows {
            for j in 1..SEGMENTS {
                let (before, edge) = (row[j - 1], row[j]);
                let (u0, u1) = ((j - 1) as f32 / SEGMENTS as f32, j as f32 / SEGMENTS as f32);
                out.push(Face {
                    texture: Tex::Path(CLOUD),
                    vertices: triangles(
                        [at(before.outer), at(edge.outer), at(before.inner), at(edge.inner)],
                        [[u0, 0.0], [u1, 0.0], [u0, 1.0], [u1, 1.0]],
                        [colour[0], colour[1], colour[2], f32::from((edge.alpha & 0xff) as u8) / 255.0],
                    ),
                    additive: self.wide,
                    flags: FLAGS_EFFECT,
                });
            }
        }
    }

    fn any(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}

const PETAL_COUNT: usize = 50;

/// One falling petal: its card, fall speed, heading in degrees and offset from the base (the y is how far down).
struct Petal {
    card: Card,
    fall: f32,
    heading: f32,
    off: Vector,
}

/// Fifty cherry petals tumbling out and down from 40 units over the point, fading.
struct Petals {
    rand: fn(u32) -> u32,
    petals: Vec<Petal>,
}

impl Petals {
    fn new(rand: fn(u32) -> u32) -> Self {
        Self { rand, petals: Vec::new() }
    }
}

impl Class for Petals {
    fn begin(&mut self, fx: &mut Fx, _: &mut dyn Assets) {
        fx.length = 300;
        for _ in 0..PETAL_COUNT {
            let scale = ((self.rand)(6) + 22) as f32 / 70.0;
            let fall = ((self.rand)(3) + 2) as f32 * 0.15;
            let spin = (self.rand)(360) as f32;
            let heading = (self.rand)(360) as f32;
            let mut card = Card::new(PETAL);
            card.delay = fall;
            card.flags = 1;
            card.layer_colours = false;
            card.alpha = 250.0;
            card.scale = scale;
            card.spin = spin;
            self.petals.push(Petal { card, fall, heading, off: [0.0; 3] });
        }
    }

    fn update(&mut self, _: &mut Fx, _: &Scene, _: &mut dyn Assets) {
        for petal in &mut self.petals {
            petal.card.alpha = (petal.card.alpha - 3.0).max(0.0);
            let (sin, cos) = sin_cos(petal.heading);
            petal.off = [petal.off[0] + 0.2 * cos, petal.off[1] + petal.fall, petal.off[2] + 0.2 * sin];
        }
    }

    /// The cards step once a drawn frame.
    fn frame(&mut self, _: &Fx, assets: &mut dyn Assets) {
        for petal in &mut self.petals {
            petal.card.step(assets);
        }
    }

    fn draw<'a>(&'a self, fx: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        for petal in &self.petals {
            let point = [fx.position[0] + petal.off[0], fx.position[1] + 40.0 - petal.off[1], fx.position[2] + petal.off[2]];
            petal.card.faces(point, 1.0, false, &seen.eye, out);
        }
    }

    fn any(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}

// Kunai Explosion's burst (990)

/// A fire burst at 1.5 times its size on the target, four ticks a frame, for 100 ticks.
struct Flame {
    card: Card,
}

impl Default for Flame {
    fn default() -> Self {
        let mut card = Card::new(FLAME);
        card.delay = 4.0;
        card.scale = 1.5;
        card.flags = 0x200;
        Self { card }
    }
}

impl Class for Flame {
    fn begin(&mut self, fx: &mut Fx, _: &mut dyn Assets) {
        fx.length = 100;
    }

    /// The frame steps once a tick here, not once a drawn frame.
    fn update(&mut self, _: &mut Fx, _: &Scene, assets: &mut dyn Assets) {
        self.card.step(assets);
        self.card.e8 += 1;
    }

    fn draw<'a>(&'a self, fx: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        self.card.faces(fx.position, 1.0, false, &seen.eye, out);
    }

    fn any(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}

// Shadow Trampling's square (991)

const SQUARE_HALF: f32 = 17.5;
const SQUARE_LIFT: f32 = 1.0;
const SQUARE_COLOUR: f32 = 92.0 / 255.0;

/// A dark translucent 7 × 7 cell square lying under the caster, its edges along the screen, for 300 ticks.
#[derive(Default)]
struct Frame;

impl Class for Frame {
    fn begin(&mut self, fx: &mut Fx, assets: &mut dyn Assets) {
        fx.length = 300;
        assets.texture(FRAME);
    }

    fn update(&mut self, _: &mut Fx, _: &Scene, _: &mut dyn Assets) {}

    fn draw<'a>(&'a self, fx: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        let centre = [fx.position[0], fx.position[1] + SQUARE_LIFT, fx.position[2]];
        let axes = (seen.eye.ground_right(), seen.eye.ground_forward());
        out.push(padded(FRAME, centre, axes, SQUARE_HALF, [1.0; 2], [SQUARE_COLOUR, SQUARE_COLOUR, SQUARE_COLOUR, 1.0], false));
    }
}

// Makibishi's caltrops (1007)

/// How far over the ground the caltrops lie.
const CALTROP_LIFT: f32 = 1.2;
const CALTROP_HALF: f32 = 2.5;

/// A caltrop picture on each cell of the unit's area, facing the camera, while the unit stands.
#[derive(Default)]
struct Caltrops {
    ratio: [f32; 2],
    started: bool,
    /// The cells' offsets in cells, row i then column j, and where each caltrop is now.
    cells: Vec<[i32; 2]>,
    points: Vec<Vector>,
}

impl Class for Caltrops {
    fn begin(&mut self, _: &mut Fx, assets: &mut dyn Assets) {
        self.ratio = assets.texture(CALTROP).map_or([1.0; 2], padded_ratio);
    }

    fn update(&mut self, fx: &mut Fx, scene: &Scene, _: &mut dyn Assets) {
        // The unit's range byte is the launch's value: a unit not found, or no range given, shows none.
        if !self.started {
            self.started = true;
            let found = fx.owner.is_some_and(|owner| (scene.feet)(owner).is_some());
            if found && fx.a7 >= 0 {
                self.cells = (-fx.a7..=fx.a7).flat_map(|i| (-fx.a7..=fx.a7).map(move |j| [i, j])).collect();
            }
        }
        self.points = self
            .cells
            .iter()
            .map(|&[i, j]| {
                let (x, z) = (fx.position[0] + 5.0 * j as f32, fx.position[2] + 5.0 * i as f32);
                [x, (scene.ground)(x, z) + CALTROP_LIFT, z]
            })
            .collect();
    }

    fn draw<'a>(&'a self, _: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        for &point in &self.points {
            out.push(padded(CALTROP, point, (seen.eye.right, seen.eye.up), CALTROP_HALF, self.ratio, [1.0; 4], false));
        }
    }

    fn alive(&self, fx: &Fx, scene: &Scene) -> bool {
        fx.owned(scene)
    }

    fn any(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}

#[cfg(test)]
mod tests {
    use super::super::List;
    use super::*;
    use crate::actor::Placed;
    use crate::effects::Loaded;
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::rc::Rc;

    const UNIT: u32 = 0x700;
    const FEET: Vector = [30.0, 4.0, 50.0];

    /// Textures of the sizes the data has; sprites of 24 frames.
    #[derive(Default)]
    struct Fake;

    impl Assets for Fake {
        fn texture(&mut self, path: &str) -> Option<[f32; 2]> {
            Some(match path {
                ICON | CALTROP => [24.0; 2],
                "data\\texture\\effect\\FireOrb.bmp" => [39.0; 2],
                _ => [64.0; 2],
            })
        }
        fn str(&mut self, _: &str) -> Option<Rc<Loaded>> {
            None
        }
        fn sprite(&mut self, _: &str, _: usize, frame: usize) -> Option<(usize, Vec<Placed>)> {
            Some((24, vec![((0, 0, frame, [255; 4], 0), [-32.0, -32.0], [64.0, 64.0], 0.0)]))
        }
    }

    thread_local! {
        /// The numbers the next draws give (0 when it runs out), and the `n` of every draw.
        static VALUES: RefCell<VecDeque<u32>> = const { RefCell::new(VecDeque::new()) };
        static DRAWS: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) };
    }

    fn scripted(n: u32) -> u32 {
        DRAWS.with(|draws| draws.borrow_mut().push(n));
        VALUES.with(|values| values.borrow_mut().pop_front()).unwrap_or(0)
    }

    fn script(values: &[u32]) {
        VALUES.with(|queue| *queue.borrow_mut() = values.iter().copied().collect());
        DRAWS.with(|draws| draws.borrow_mut().clear());
    }

    fn drawn() -> Vec<u32> {
        DRAWS.with(|draws| draws.borrow().clone())
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    fn close3(a: Vector, b: Vector) -> bool {
        (0..3).all(|axis| close(a[axis], b[axis]))
    }

    /// `count` ticks with the unit at `at` (None: gone) and the ground `ground` high.
    fn ticks(fx: &mut WorldFx, count: usize, at: Option<Vector>, ground: f32) {
        let feet = |unit: Option<u32>| match unit {
            Some(UNIT) => at,
            Some(_) => None,
            None => Some(FEET),
        };
        let body = |_: Option<u32>| None;
        let eye = Eye::at(45.0, 0.0);
        let scene = Scene { player: FEET, feet: &feet, body: &body, longitude: 0.0, right: eye.right, up: eye.up, ground: &move |_, _| ground };
        for _ in 0..count {
            fx.tick(&scene, &mut Fake);
        }
    }

    fn launched(id: u32, a7: Option<i32>) -> WorldFx {
        let mut fx = WorldFx::default();
        fx.launch(&Launch { a7, ..Launch::at(id, Some(Some(UNIT)), FEET) }, &mut Fake).unwrap();
        fx
    }

    fn seen(eye: Eye, shows: &dyn Fn(u32) -> bool) -> Seen<'_> {
        Seen { eye, shows, hidden: &|_| false, size: &|_| 1.0, ground: &|_, _| 0.0, dim: false }
    }

    fn all(_: u32) -> bool {
        true
    }

    fn faces_at(fx: &WorldFx, longitude: f32) -> Vec<Face<'_>> {
        fx.faces(&seen(Eye::at(45.0, longitude), &all))
    }

    fn class<T: 'static>(fx: &mut WorldFx, id: u32) -> &mut T {
        fx.class_mut::<T>(Some(Some(UNIT)), &[id]).unwrap().1
    }

    fn centre(face: &Face) -> Vector {
        std::array::from_fn(|axis| face.vertices.iter().map(|vertex| vertex.position[axis]).sum::<f32>() / 6.0)
    }

    fn alpha(face: &Face) -> f32 {
        (face.vertices[0].tint[3] * 255.0).round()
    }

    #[test]
    fn orbs_are_two_quads_each_spread_over_the_circle() {
        for (n, thetas) in [(3, vec![0.0, 120.0, 240.0]), (7, (0..7).map(|i| (51 * i) as f32).collect())] {
            script(&[10, 1, 20, 2, 30, 0]);
            let mut orbs = Orbs::new(scripted);
            orbs.set_count(n, false);
            assert_eq!(orbs.orbs.len(), n);
            assert_eq!(orbs.orbs.iter().map(|orb| orb[0].theta).collect::<Vec<_>>(), thetas);
            // Both quads of an orb start alike.
            assert!(orbs.orbs.iter().all(|orb| orb[0] == orb[1]));
            // Per orb in order: the phase below 360, then the step below 3.
            assert_eq!(drawn(), [360, 3].repeat(n));
            if n == 3 {
                let phases: Vec<(i32, i32)> = orbs.orbs.iter().map(|orb| (orb[0].p, orb[0].b)).collect();
                assert_eq!(phases, [(10, 1), (20, 2), (30, 0)]);
            }
        }
        // More only appends; the fog's step is 0 though the draw is made.
        script(&[5, 2, 6, 2, 7, 2, 8, 2]);
        let mut orbs = Orbs::new(scripted);
        orbs.set_count(2, true);
        orbs.set_count(2, true);
        orbs.set_count(4, true);
        assert_eq!(orbs.orbs.len(), 4);
        assert_eq!(drawn().len(), 8);
        assert!(orbs.orbs.iter().all(|orb| orb[0].b == 0));
        assert_eq!(orbs.orbs[3][0].theta, 270.0);
        // A count that is lower takes none away.
        orbs.set_count(1, true);
        assert_eq!(orbs.orbs.len(), 4);
    }

    #[test]
    fn an_orb_fades_in_turns_and_bobs() {
        let l = look(FIRE);
        let mut piece = Piece { theta: 0.0, p: 0, b: 1, c: 0, alpha: 0, half: 0.0, offset: [0.0; 3] };
        piece.step(&l, 1);
        assert_eq!((piece.alpha, piece.c, piece.p), (1, 1, 1));
        assert!(close(piece.theta, 0.75));
        for _ in 0..199 {
            piece.step(&l, 1);
        }
        assert_eq!((piece.c, piece.alpha), (200, 200));
        piece.step(&l, 1);
        assert_eq!(piece.alpha, 201);
        piece.step(&l, 1);
        assert_eq!((piece.c, piece.alpha), (202, 201));
        // The turn wraps above 360; the fog turns a third as fast.
        let mut piece = Piece { theta: 359.5, ..piece };
        piece.step(&l, 1);
        assert!(close(piece.theta, 0.25));
        let mut fog = Piece { theta: 10.0, ..piece };
        fog.step(&look(FOG), 1);
        assert!(close(fog.theta, 10.25));
        // The size, radius and height after the step to p = 0 and 90, with b = 1 and 2; quad 0 is a tenth larger.
        for (b, p, size, radius, height) in [(1, 0, 2.3, 8.0, 12.0), (1, 90, 2.3 * 1.1, 8.6, 11.4), (2, 90, 2.3 * 1.1, 8.6, 10.8), (2, 0, 2.3, 8.0, 12.0)] {
            for k in 0..2 {
                let mut piece = Piece { theta: 0.0, p: (p + 360 - b) % 360, b, c: 0, alpha: 0, half: 0.0, offset: [0.0; 3] };
                piece.step(&l, k);
                assert_eq!(piece.p, p);
                assert!(close(piece.half, if k == 0 { size + 2.3 * 0.1 } else { size }), "{b} {p} {k}: {}", piece.half);
                assert!(close(piece.offset[1], height) && close(piece.offset[0].hypot(piece.offset[2]), radius), "{b} {p}: {:?}", piece.offset);
            }
        }
    }

    #[test]
    fn each_id_has_its_picture_size_colours_and_blends() {
        let rows = [
            (THUNDER, "thunder_center.bmp", 0.8, 8.0, 12.0, [[0.0, 0.0, 1.0], [1.0, 1.0, 0.0]], [false, true]),
            (FIRE, "FireOrb.bmp", 2.3, 8.0, 12.0, [[1.0; 3]; 2], [false, true]),
            (WIND, "LightningOrb.bmp", 2.3, 8.0, 12.0, [[1.0; 3]; 2], [false, true]),
            (WATER, "WaterOrb.bmp", 1.3, 8.0, 12.0, [[1.0; 3]; 2], [false, true]),
            (EARTH, "StoneOrb.bmp", 2.3, 8.0, 12.0, [[1.0; 3]; 2], [false, true]),
            (FOG, "fog1.tga", 6.0, 4.0, 8.0, [[1.0; 3]; 2], [false, false]),
        ];
        for (id, file, s, radius, height, colours, additive) in rows {
            let l = look(id);
            assert_eq!(l.texture, format!("data\\texture\\effect\\{file}"));
            assert_eq!((l.s, l.radius, l.height, l.colours, l.additive), (s, radius, height, colours, additive), "{id}");
            assert_eq!(l.speed, if id == FOG { 0.05 } else { 0.15 });
            // In the object: its two faces per orb, the first drawn first.
            let mut fx = launched(id, Some(2));
            ticks(&mut fx, 1, Some(FEET), 0.0);
            let faces = faces_at(&fx, 0.0);
            assert_eq!(faces.len(), if id == FOG { 20 } else { 4 }, "{id}");
            for (k, face) in faces.iter().take(2).enumerate() {
                assert_eq!((face.additive, face.flags), (additive[k], FLAGS_EFFECT), "{id} {k}");
                assert_eq!(&face.vertices[0].tint[..3], &colours[k][..], "{id} {k}");
                assert_eq!(alpha(face), 1.0);
            }
        }
        // The picture fills the top left of its surface: 39 of 64.
        let mut fx = launched(FIRE, Some(1));
        ticks(&mut fx, 1, Some(FEET), 0.0);
        let faces = faces_at(&fx, 0.0);
        let width = faces[1].vertices[1].position[0] - faces[1].vertices[0].position[0];
        let half = class::<Orbs>(&mut fx, FIRE).orbs[0][1].half;
        assert!(close(width, 2.0 * half * 39.0 / 64.0), "{width}");
    }

    #[test]
    fn a_charm_packet_replaces_the_ring() {
        let owner = Some(Some(UNIT));
        let mut fx = WorldFx::default();
        for id in [THUNDER, FIRE, WIND, WATER, EARTH] {
            fx.launch(&Launch::at(id, owner, FEET), &mut Fake).unwrap();
        }
        let ids = |fx: &WorldFx| {
            let mut ids: Vec<u32> = fx.iter().map(|object| object.fx.id).collect();
            ids.sort_unstable();
            ids
        };
        // Count 0 removes the four ring kinds and starts nothing; 228 stays.
        charms(&mut fx, owner, (1, 0), Some(FEET), true, &mut Fake);
        assert_eq!(ids(&fx), [THUNDER]);
        // Kinds 1 – 4 start 737, 738, 735, 736; kind 5 starts 228.
        for (kind, id) in [(1, WATER), (2, EARTH), (3, FIRE), (4, WIND)] {
            charms(&mut fx, owner, (kind, 3), Some(FEET), true, &mut Fake);
            assert_eq!(ids(&fx), [THUNDER, id].iter().copied().collect::<std::collections::BTreeSet<_>>().into_iter().collect::<Vec<_>>(), "{kind}");
            assert_eq!(class::<Orbs>(&mut fx, id).orbs.len(), 3);
        }
        // Another count replaces the ring.
        charms(&mut fx, owner, (4, 5), Some(FEET), true, &mut Fake);
        assert_eq!(fx.iter().filter(|object| object.fx.id == WIND).count(), 1);
        assert_eq!(class::<Orbs>(&mut fx, WIND).orbs.len(), 5);
        // A hidden unit or one out of sight, a kind not known: none.
        charms(&mut fx, owner, (3, 2), Some(FEET), false, &mut Fake);
        charms(&mut fx, owner, (3, 2), None, true, &mut Fake);
        charms(&mut fx, owner, (9, 2), Some(FEET), true, &mut Fake);
        assert_eq!(ids(&fx), [THUNDER]);
        charms(&mut fx, owner, (5, 4), Some(FEET), true, &mut Fake);
        assert_eq!(fx.iter().filter(|object| object.fx.id == THUNDER).count(), 2);
        // The packet is for its own unit only.
        charms(&mut fx, Some(Some(UNIT + 1)), (1, 1), Some(FEET), true, &mut Fake);
        assert_eq!(fx.iter().filter(|object| object.fx.id == WATER).count(), 1);
        charms(&mut fx, owner, (1, 0), Some(FEET), true, &mut Fake);
        assert_eq!(fx.iter().filter(|object| object.fx.id == WATER).count(), 1);
    }

    #[test]
    fn moonlight_fantasy_makes_ten_fog_puffs_and_the_charms_draw_with_effects_off() {
        let owner = Launch::at(FOG, Some(Some(UNIT)), FEET);
        let mut fx = WorldFx::default();
        fx.status(&owner, true, &mut Fake).unwrap();
        assert_eq!(class::<Orbs>(&mut fx, FOG).orbs.len(), 10);
        // Sent again, a status starts afresh.
        fx.status(&owner, true, &mut Fake).unwrap();
        assert_eq!(fx.iter().count(), 1);
        assert!(restarts_on_status(FOG) && !restarts_on_status(THUNDER));
        assert_eq!(fx.status(&owner, false, &mut Fake), None);
        assert!(fx.is_empty());
        // /effect off: the charms draw, the fog does not.
        for id in [THUNDER, FIRE, WIND, WATER, EARTH] {
            assert!(crate::effects::force_drawn(id), "{id}");
        }
        assert!(!crate::effects::force_drawn(FOG));
        for (id, shown) in [(WATER, 2), (FOG, 0)] {
            let mut fx = launched(id, Some(1));
            ticks(&mut fx, 1, Some(FEET), 0.0);
            assert_eq!(fx.faces(&seen(Eye::at(45.0, 0.0), &crate::effects::force_drawn)).len(), shown, "{id}");
        }
        assert!(is_charm(THUNDER) && is_charm(738) && !is_charm(FOG) && !is_charm(739));
    }

    #[test]
    fn an_orb_follows_the_feet() {
        let mut fx = launched(EARTH, Some(1));
        ticks(&mut fx, 1, Some(FEET), 0.0);
        // Hovering about 12 up and 8 out (the bob and the cut picture move the middle a few units).
        let near =
            |faces: Vec<Face>, at: Vector| faces.iter().map(centre).all(|c| (c[1] - (at[1] + 12.0)).abs() < 4.0 && (c[0] - at[0]).hypot(c[2] - at[2]) < 14.0);
        assert!(near(faces_at(&fx, 0.0), FEET));
        let moved_to = [40.0, 6.0, 55.0];
        let mut moved = launched(EARTH, Some(1));
        ticks(&mut moved, 1, Some(moved_to), 0.0);
        assert!(near(faces_at(&moved, 0.0), moved_to));
        assert_eq!(moved.iter().next().unwrap().fx.position, moved_to);
        ticks(&mut fx, 1, None, 0.0);
        assert!(fx.is_empty());
    }

    /// The facing of a ring quad's top edge, the spin in degrees.
    fn spin_of(face: &Face) -> f32 {
        let (v0, v1) = (face.vertices[0].position, face.vertices[1].position);
        (-(v1[2] - v0[2])).atan2(v1[0] - v0[0]).to_degrees().rem_euclid(360.0)
    }

    #[test]
    fn the_kunai_ring_waits_five_updates_then_turns_18_degrees_a_tick() {
        let mut fx = launched(KUNAI_RING, None);
        for count in 1..=5 {
            ticks(&mut fx, 1, Some(FEET), 0.0);
            assert!(faces_at(&fx, 0.0).is_empty(), "update {count}");
        }
        let mut lead = Vec::new();
        for _ in 0..21 {
            ticks(&mut fx, 1, Some(FEET), 0.0);
            lead.push(class::<Ring>(&mut fx, KUNAI_RING).angles);
        }
        assert_eq!(lead[0], [18, 13, 8, 3]);
        assert_eq!(lead[1], [36, 31, 26, 21]);
        assert_eq!(lead[18][0], 342);
        assert_eq!(lead[19][0], 0);
        assert_eq!(lead[20][0], 18);
        // A ghost that would go below 0 wraps.
        assert_eq!(lead[19], [0, 355, 350, 345]);
        let faces = faces_at(&fx, 0.0);
        assert_eq!(faces.len(), 4);
        assert_eq!(faces.iter().map(alpha).collect::<Vec<_>>(), [255.0, 105.0, 80.0, 55.0]);
        assert!(faces.iter().all(|face| !face.additive && face.vertices[0].tint[..3] == [1.0, 200.0 / 255.0, 1.0]));
        // Radius 13, 8 up (the quad is tipped, so its middle is within a unit and a half of its point), spun ang + 45 to the camera.
        let angle = lead[20][0];
        for (face, ghost) in faces.iter().zip(lead[20]) {
            let c = centre(face);
            assert!((c[1] - (FEET[1] + 8.0)).abs() <= 1.5 && ((c[0] - FEET[0]).hypot(c[2] - FEET[2]) - 13.0).abs() <= 1.5, "{ghost}: {c:?}");
            assert!(close(spin_of(face), ((ghost + 45) % 360) as f32), "{ghost} {} vs {}", spin_of(face), ghost + 45);
        }
        assert_eq!(angle, 18);
        // The world spin adds the camera's longitude.
        let turned = fx.faces(&seen(Eye::at(45.0, 30.0), &all));
        assert!(close(spin_of(&turned[0]), 63.0 + 30.0));
        // The picture fills 24 of 32: 3/4 of the 5 unit square.
        let (v0, v1) = (faces[0].vertices[0].position, faces[0].vertices[1].position);
        let width = (0..3).map(|axis| (v1[axis] - v0[axis]).powi(2)).sum::<f32>().sqrt();
        assert!(close(width, 5.0 * 0.75), "{width}");
        // It ends at e = 60.
        ticks(&mut fx, 34, Some(FEET), 0.0);
        assert_eq!(fx.get_mut(1).map(|object| object.fx.age), Some(60));
        ticks(&mut fx, 1, Some(FEET), 0.0);
        assert!(fx.is_empty());
    }

    #[test]
    fn the_ribbons_state_by_update() {
        // (m, j) → alpha: 51 + m + j up to 12, 63 + j to 50, then 113 − m − j.
        let alpha_at = |m, j| ribbon_state(false, 0, j, m).3;
        assert_eq!(
            [alpha_at(0, 0), alpha_at(12, 5), alpha_at(13, 5), alpha_at(20, 5), alpha_at(50, 20), alpha_at(51, 0), alpha_at(59, 0), alpha_at(59, 20)],
            [51, 68, 68, 68, 83, 62, 54, 34]
        );
        assert_eq!(alpha_at(100, 20), 0);
        // Angle and span grow 5 and 3 a segment and an update; the span stops at 180.
        let (angle, span, radius, _) = ribbon_state(false, 2, 7, 4);
        assert_eq!((angle, span, radius), (180 + 25 + 35, 30 + 15 + 21, 5.0));
        assert_eq!(ribbon_state(false, 0, 20, 100).1, 180);
        // The radius is R0 to m = 12, then 0.1 more an update and a segment; the blue one 3 wider.
        assert_eq!(ribbon_state(false, 0, 0, 12).2, 7.0);
        assert!(close(ribbon_state(false, 0, 10, 22).2, 7.0 + 1.0 + 1.0));
        assert_eq!(ribbon_state(true, 3, 0, 0).2, 4.0 + 3.0);
        // θ = span div 21 · j + angle; the edges lean by t.
        let edge = ribbon_edge(true, 1, 5, 0, 90.0);
        let theta = (2.0 * 5.0 + 120.0f32).to_radians();
        assert!(close3(edge.inner, [9.0 * theta.cos(), 23.0, 9.0 * theta.sin()]), "{:?}", edge.inner);
        assert!(close3(edge.outer, [9.0 * theta.cos(), 21.0, 9.0 * theta.sin()]), "{:?}", edge.outer);
        let flat = ribbon_edge(false, 0, 0, 0, 80.0);
        let (c, s) = (80.0f32.to_radians().cos(), 80.0f32.to_radians().sin());
        // m = 0, j = 0, row 0: angle 5, span 33 (33 div 21 = 1, times 0), so θ = 5.
        let t5 = 5.0f32.to_radians();
        assert!(close3(flat.inner, [7.0 * t5.cos(), 28.0, 7.0 * t5.sin()]));
        assert!(close3(flat.outer, [(7.0 + c) * t5.cos(), 25.0 + s, (7.0 + c) * t5.sin()]));
    }

    #[test]
    fn the_ribbons_are_four_rows_of_twenty_strips() {
        script(&[0, 5, 10, 20, 99]);
        let mut ribbons = Ribbons::new(scripted);
        let mut fx = Fx::of(&Launch::at(BLUE_RIBBONS, Some(Some(UNIT)), FEET));
        ribbons.begin(&mut fx, &mut Fake);
        // The leans: four draws, one a row in order, then none.
        assert_eq!(drawn(), [21, 21, 21, 21]);
        assert_eq!(ribbons.lean, [80.0, 85.0, 90.0, 100.0]);
        assert!(ribbons.wide);
        for id in [DARK_RIBBONS, BLUE_RIBBONS] {
            let mut fx = launched(id, None);
            ticks(&mut fx, 1, Some(FEET), 0.0);
            let faces = faces_at(&fx, 0.0);
            assert_eq!(faces.len(), 80);
            let blue = id == BLUE_RIBBONS;
            assert!(faces.iter().all(|face| face.additive == blue));
            let colour = if blue { [100.0 / 255.0, 100.0 / 255.0, 1.0] } else { [50.0 / 255.0; 3] };
            assert!(faces.iter().all(|face| face.vertices[0].tint[..3] == colour));
            // Strip 1 of row 0: u from 0 to 1/21, v 0 on the outer edge and 1 on the inner edge; alpha 51 + 1.
            let uv = |index: usize| faces[0].vertices[index].uv;
            assert_eq!([uv(0), uv(1), uv(2), uv(5)], [[0.0, 0.0], [1.0 / 21.0, 0.0], [0.0, 1.0], [1.0 / 21.0, 1.0]]);
            assert_eq!(alpha(&faces[0]), 52.0);
            assert_eq!(alpha(&faces[19]), 71.0);
            // Each row starts 90 degrees on: the strips of rows 0 and 1 are 90 turns apart about the unit.
            let at = |face: &Face| {
                let p = face.vertices[4].position;
                (p[2] - FEET[2]).atan2(p[0] - FEET[0]).to_degrees()
            };
            let turn = (at(&faces[20]) - at(&faces[0])).rem_euclid(360.0);
            assert!((turn - 90.0).abs() < 30.0 || (turn - 270.0).abs() < 30.0, "{turn}");
        }
        // The last update is e = 59: the alpha is 54 − j.
        let mut fx = launched(DARK_RIBBONS, None);
        ticks(&mut fx, 60, Some(FEET), 0.0);
        assert_eq!(alpha(&faces_at(&fx, 0.0)[0]), 53.0);
        ticks(&mut fx, 1, Some(FEET), 0.0);
        assert!(fx.is_empty());
    }

    #[test]
    fn fifty_petals_fall_and_fade() {
        script(&[3, 2, 90, 90].repeat(50));
        let mut petals = Petals::new(scripted);
        let mut fx = Fx::of(&Launch::at(PETALS, None, FEET));
        petals.begin(&mut fx, &mut Fake);
        assert_eq!(fx.length, 300);
        // Per card in order: the size, the fall, the spin and the heading.
        assert_eq!(drawn(), [6, 3, 360, 360].repeat(50));
        assert_eq!(petals.petals.len(), 50);
        let card = &petals.petals[0].card;
        assert!(close(card.scale, 25.0 / 70.0) && close(petals.petals[0].fall, 0.6) && card.spin == 90.0);
        assert_eq!((card.flags, card.alpha, card.layer_colours, card.end), (1, 250.0, false, super::super::End::Loop));
        // The delay is the fall cut to a whole number: 0, so a frame every drawn frame.
        assert!(petals.petals.iter().all(|petal| petal.card.delay as u32 == 0));
        // In the registry: alpha 247 at e = 0, the card 39.4 up; at e = 66 it is under the point; 0 alpha at e = 83.
        let mut fx = launched(PETALS, None);
        ticks(&mut fx, 1, Some(FEET), 0.0);
        let petals = class::<Petals>(&mut fx, PETALS);
        assert_eq!(petals.petals.len(), 50);
        assert!(petals.petals.iter().all(|petal| petal.card.alpha == 247.0));
        ticks(&mut fx, 82, Some(FEET), 0.0);
        let petals = class::<Petals>(&mut fx, PETALS);
        assert!(petals.petals.iter().all(|petal| petal.card.alpha == 1.0));
        ticks(&mut fx, 1, Some(FEET), 0.0);
        assert!(class::<Petals>(&mut fx, PETALS).petals.iter().all(|petal| petal.card.alpha == 0.0));
        // Each drawn frame gives a frame.
        let (assets, eye) = (&mut Fake, Eye::at(45.0, 0.0));
        let seen = seen(eye, &all);
        for _ in 0..3 {
            fx.frame(&seen, assets);
        }
        assert_eq!(class::<Petals>(&mut fx, PETALS).petals[0].card.frame, 3);
        assert_eq!(fx.faces(&seen).len(), 50);
        // A 0.6 petal heading 90 goes 0.2 along z a tick and is at 40 − 0.6 · 67 = −0.2 at e = 66.
        script(&[0, 2, 0, 90]);
        let mut one = Petals::new(scripted);
        one.petals.clear();
        let mut fx2 = Fx::of(&Launch::at(PETALS, None, FEET));
        one.begin(&mut fx2, &mut Fake);
        one.petals.truncate(1);
        let scene = {
            let feet = |_: Option<u32>| Some(FEET);
            let body = |_: Option<u32>| None;
            let eye = Eye::at(45.0, 0.0);
            move |f: &mut dyn FnMut(&Scene)| {
                f(&Scene { player: FEET, feet: &feet, body: &body, longitude: 0.0, right: eye.right, up: eye.up, ground: &|_, _| 0.0 })
            }
        };
        for _ in 0..67 {
            scene(&mut |scene| one.update(&mut fx2, scene, &mut Fake));
        }
        assert!(close(one.petals[0].off[1], 0.6 * 67.0) && close(40.0 - one.petals[0].off[1], -0.2));
        assert!(close(one.petals[0].off[2], 0.2 * 67.0) && close(one.petals[0].off[0], 0.0));
    }

    #[test]
    fn the_burst_is_an_opaque_card_that_follows_its_target() {
        let mut fx = launched(BURST, None);
        assert_eq!(fx.iter().next().unwrap().fx.length, 100);
        // One copy a packet.
        fx.launch(&Launch::at(BURST, Some(Some(UNIT)), FEET), &mut Fake).unwrap();
        assert_eq!(fx.iter().count(), 2);
        ticks(&mut fx, 1, Some(FEET), 0.0);
        let flame = class::<Flame>(&mut fx, BURST);
        assert_eq!((flame.card.scale, flame.card.delay, flame.card.flags), (1.5, 4.0, 0x200));
        assert!(flame.card.layer_colours);
        assert_eq!(List::of(0x200), List::Opaque);
        let faces = faces_at(&fx, 0.0);
        assert_eq!(faces.len(), 2);
        assert!(faces.iter().all(|face| !face.additive && List::of(face.flags) == List::Opaque));
        // The frame goes up every fourth tick.
        ticks(&mut fx, 3, Some(FEET), 0.0);
        assert_eq!(class::<Flame>(&mut fx, BURST).card.frame, 0);
        ticks(&mut fx, 1, Some(FEET), 0.0);
        assert_eq!(class::<Flame>(&mut fx, BURST).card.frame, 1);
        // It follows the target and ends after 100 ticks.
        let moved = [40.0, 4.0, 55.0];
        ticks(&mut fx, 1, Some(moved), 0.0);
        assert_eq!(fx.iter().next().unwrap().fx.position, moved);
        ticks(&mut fx, 94, Some(moved), 0.0);
        assert_eq!(fx.iter().next().unwrap().fx.age, 100);
        ticks(&mut fx, 1, Some(moved), 0.0);
        assert!(fx.is_empty());
    }

    #[test]
    fn the_square_lies_35_units_wide_one_up_and_follows_the_caster() {
        let mut fx = launched(SQUARE, None);
        assert_eq!(fx.iter().next().unwrap().fx.length, 300);
        ticks(&mut fx, 1, Some(FEET), 0.0);
        for longitude in [0.0, 40.0] {
            let faces = faces_at(&fx, longitude);
            assert_eq!(faces.len(), 1);
            let face = &faces[0];
            assert!(!face.additive && List::of(face.flags) == List::Effect);
            assert_eq!(&face.vertices[0].tint[..], &[92.0 / 255.0, 92.0 / 255.0, 92.0 / 255.0, 1.0]);
            let positions: Vec<Vector> = face.vertices.iter().map(|vertex| vertex.position).collect();
            assert!(positions.iter().all(|p| close(p[1], FEET[1] + 1.0)));
            // 35 units a side, edges along the screen, so a corner is 17.5 along the ground right and forward.
            let eye = Eye::at(45.0, longitude);
            let (right, forward) = (eye.ground_right(), eye.ground_forward());
            let along = |p: Vector, axis: Vector| (p[0] - FEET[0]) * axis[0] + (p[2] - FEET[2]) * axis[2];
            // Both ground axes span 35: a corner is 17.5 along the ground right and forward.
            for axis in [right, forward] {
                let span = positions.iter().map(|&p| along(p, axis)).fold((f32::MAX, f32::MIN), |(lo, hi), x| (lo.min(x), hi.max(x)));
                assert!(close(span.1 - span.0, 35.0) && close(span.0, -17.5));
            }
        }
        // It goes with the caster's place, not the camera's.
        ticks(&mut fx, 1, Some([40.0, 6.0, 55.0]), 0.0);
        assert!(close3(centre(&faces_at(&fx, 0.0)[0]), [40.0, 7.0, 55.0]));
    }

    #[test]
    fn makibishi_lays_a_caltrop_on_each_cell_of_its_area() {
        for (range, count) in [(0, 1), (1, 9), (2, 25)] {
            let mut fx = launched(CALTROPS, Some(range));
            ticks(&mut fx, 1, Some(FEET), 10.0);
            let points = class::<Caltrops>(&mut fx, CALTROPS).points.clone();
            let faces = faces_at(&fx, 0.0);
            assert_eq!(faces.len(), count, "r {range}");
            // Row i outer, column j: 5 j along x, 5 i along z from the unit, 1.2 over the ground.
            let r = range;
            let mut expected = Vec::new();
            for i in -r..=r {
                for j in -r..=r {
                    expected.push([FEET[0] + 5.0 * j as f32, 11.2, FEET[2] + 5.0 * i as f32]);
                }
            }
            assert_eq!(points, expected);
            assert!(faces.iter().all(|face| !face.additive && alpha(face) == 255.0));
            // The picture fills 24 of 32 of a 5 unit square, facing the camera.
            let width = faces[0].vertices[1].position[0] - faces[0].vertices[0].position[0];
            assert!(close(width, 5.0 * 0.75), "{width}");
        }
        // No range given, or no unit at the start: none.
        for (a7, here) in [(None, Some(FEET)), (Some(2), None)] {
            let mut fx = launched(CALTROPS, a7);
            ticks(&mut fx, 1, here, 0.0);
            assert!(faces_at(&fx, 0.0).is_empty());
        }
        // It ends with the unit, and follows its place.
        let mut fx = launched(CALTROPS, Some(0));
        ticks(&mut fx, 1, Some(FEET), 0.0);
        ticks(&mut fx, 1, Some([40.0, 4.0, 60.0]), 0.0);
        assert_eq!(class::<Caltrops>(&mut fx, CALTROPS).points, [[40.0, 1.2, 60.0]]);
        ticks(&mut fx, 1, None, 0.0);
        assert!(fx.is_empty());
    }

    #[test]
    fn every_class_is_registered() {
        let fx = WorldFx::default();
        for id in [THUNDER, FIRE, WIND, WATER, EARTH, FOG, KUNAI_RING, DARK_RIBBONS, BLUE_RIBBONS, PETALS, BURST, SQUARE, CALTROPS] {
            assert!(fx.registered(id), "{id}");
        }
        assert_eq!([1, 2, 3, 4, 5, 0, 6].map(charm_effect), [Some(WATER), Some(EARTH), Some(FIRE), Some(WIND), Some(THUNDER), None, None]);
    }
}
