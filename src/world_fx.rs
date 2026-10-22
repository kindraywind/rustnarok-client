//! World objects that are not one STR file: small objects that build their pictures in code (quads, sprite cards, STR
//! parts they move themselves) and update them every 16 ms tick. This is their shared base: the registry with launch,
//! removal, tick, far rule and draw gate; the world quad; the sprite card; the draw lists by face flags; the STR part
//! with its 2-D and 3-D draws. The per-skill objects register their classes here.

// The per-skill objects come after this base; until then only the tests launch objects.
#![allow(dead_code)]

pub mod area4;
pub mod drops;
pub mod footprints;
pub mod heal;
pub mod jobs3;
pub mod shots;
pub mod status4;
pub mod summoner;

use std::collections::HashMap;
use std::rc::Rc;

use crate::actor::{ImageKey, Placed};
use crate::effects::{Loaded, str_corner};
use crate::world::{MeshVertex, Vector};
use crate::world_str::{self, Controls, Fired, Scene, Step, Timeline};

/// An object's length in ticks unless its class says.
pub const LENGTH: u32 = 60;
/// World units per sprite pixel at zoom 1 (and per STR pixel of a 2-D STR draw).
pub const PIXEL: f32 = 1.0 / 7.0;
/// The STR part's default step offset in STR px: its (320, 240) sits 80 px over the point.
pub const STR_OFFSET: [f32; 2] = [0.0, -80.0];
/// The opaque list drops texels with less alpha than this.
pub const OPAQUE_CUT: f32 = 207.0 / 255.0;
/// How much nearer each later layer of a sprite card sits.
const LAYER_BIAS: f32 = 1e-5;
// ponytail: a depth bias b pulls a face b × 1000 world units toward the camera; tune against a screenshot.
const BIAS_UNITS: f32 = 1000.0;

/// The face flags.
pub const FLAGS_EFFECT: u32 = 5;
pub const FLAGS_FLOOR: u32 = 0xd;
pub const FLAGS_TRANSLUCENT: u32 = 1 | 0x20;

/// The draw list a face goes to by its flags, in the order the lists are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum List {
    /// No blending, alpha-tested at `OPAQUE_CUT`, writes depth.
    Opaque,
    /// Under the unit sprites, unfogged, no depth write.
    Floor,
    /// With the unit sprites: their two passes at alpha 159.
    WithSprites,
    /// Alpha-blended, fogged, far to near.
    Translucent,
    /// The client's `meshes`: own blends, unfogged, no depth write, in insertion order.
    Effect,
    /// Over everything, adding, far to near.
    TopAdd,
    /// Over everything, blended, in insertion order after the others.
    Top,
}

impl List {
    pub fn of(flags: u32) -> Self {
        match flags {
            _ if flags & 1 == 0 => Self::Opaque,
            FLAGS_FLOOR => Self::Floor,
            0x101 => Self::WithSprites,
            1 | FLAGS_TRANSLUCENT => Self::Translucent,
            0x203 => Self::TopAdd,
            0x201 => Self::Top,
            // ponytail: 5, 5 | 0x20 and any flags not in the table.
            _ => Self::Effect,
        }
    }
}

/// Whether the opaque list keeps a texel of this alpha (0 … 1).
pub fn opaque_keeps(alpha: f32) -> bool {
    alpha >= OPAQUE_CUT
}

/// Where a face's texture comes from.
#[derive(Clone, Copy)]
pub enum Tex<'a> {
    /// A texture file, loaded by path.
    Path(&'a str),
    /// An STR's layer and texture.
    Str(&'a Loaded, usize, usize),
    /// A sprite's (by path) image.
    Sprite(&'a str, ImageKey),
}

/// A textured shape ready for its list: two triangles a quad.
pub struct Face<'a> {
    pub texture: Tex<'a>,
    pub vertices: Vec<MeshVertex>,
    pub additive: bool,
    pub flags: u32,
}

/// The faces of a frame, by list.
pub struct Lists<T> {
    pub opaque: Vec<T>,
    pub floor: Vec<T>,
    pub with_sprites: Vec<T>,
    pub translucent: Vec<T>,
    pub effect: Vec<T>,
    pub top_add: Vec<T>,
    pub top: Vec<T>,
}

impl<T> Default for Lists<T> {
    fn default() -> Self {
        Self {
            opaque: Vec::new(),
            floor: Vec::new(),
            with_sprites: Vec::new(),
            translucent: Vec::new(),
            effect: Vec::new(),
            top_add: Vec::new(),
            top: Vec::new(),
        }
    }
}

impl<'a> Lists<Face<'a>> {
    /// Puts each face in its list; the translucent and adding lists sort far to near by their first vertex's depth.
    pub fn sort(faces: Vec<Face<'a>>, depth_of: impl Fn(Vector) -> f32) -> Self {
        let mut lists = Self::default();
        for face in faces {
            let list = match List::of(face.flags) {
                List::Opaque => &mut lists.opaque,
                List::Floor => &mut lists.floor,
                List::WithSprites => &mut lists.with_sprites,
                List::Translucent => &mut lists.translucent,
                List::Effect => &mut lists.effect,
                List::TopAdd => &mut lists.top_add,
                List::Top => &mut lists.top,
            };
            list.push(face);
        }
        let depth = |face: &Face| face.vertices.first().map_or(0.0, |vertex| depth_of(vertex.position));
        for list in [&mut lists.translucent, &mut lists.top_add] {
            list.sort_by(|a, b| depth(b).total_cmp(&depth(a)));
        }
        lists
    }
}

impl<T> Lists<T> {
    pub fn map<U>(self, mut f: impl FnMut(T) -> Option<U>) -> Lists<U> {
        let mut each = |list: Vec<T>| -> Vec<U> { list.into_iter().filter_map(&mut f).collect() };
        Lists {
            opaque: each(self.opaque),
            floor: each(self.floor),
            with_sprites: each(self.with_sprites),
            translucent: each(self.translucent),
            effect: each(self.effect),
            top_add: each(self.top_add),
            top: each(self.top),
        }
    }
}

fn add(a: Vector, b: Vector) -> Vector {
    std::array::from_fn(|axis| a[axis] + b[axis])
}

fn scaled(v: Vector, by: f32) -> Vector {
    v.map(|value| value * by)
}

fn normalized(v: Vector) -> Vector {
    let length = v.iter().map(|value| value * value).sum::<f32>().sqrt();
    if length == 0.0 { v } else { scaled(v, 1.0 / length) }
}

/// The camera's axes in the world.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Eye {
    pub right: Vector,
    pub up: Vector,
    pub forward: Vector,
}

impl Eye {
    /// The axes at a latitude (degrees, looking down) and longitude (degrees), as `Camera::view` builds them.
    pub fn at(latitude: f32, longitude: f32) -> Self {
        let (sin_lat, cos_lat) = latitude.to_radians().sin_cos();
        let (sin_lon, cos_lon) = longitude.to_radians().sin_cos();
        Self { right: [cos_lon, 0.0, -sin_lon], up: [sin_lat * sin_lon, cos_lat, sin_lat * cos_lon], forward: [cos_lat * sin_lon, -sin_lat, cos_lat * cos_lon] }
    }

    /// The camera's right on the ground: it turns with the longitude only.
    pub fn ground_right(&self) -> Vector {
        normalized([self.right[0], 0.0, self.right[2]])
    }

    /// The camera's horizontal view direction.
    pub fn ground_forward(&self) -> Vector {
        normalized([self.forward[0], 0.0, self.forward[2]])
    }

    /// A point pulled toward the camera by a depth bias.
    fn nearer(&self, point: Vector, bias: f32) -> Vector {
        add(point, scaled(self.forward, -bias * BIAS_UNITS))
    }
}

/// A point moved by `right` along the camera's right axis and `up` along its up axis: the same on the screen at any
/// latitude.
pub fn view_offset(point: Vector, right: f32, up: f32, eye: &Eye) -> Vector {
    add(add(point, scaled(eye.right, right)), scaled(eye.up, up))
}

/// An offset (x right on the ground, y up, z along the view) turned with the camera's longitude about `point`, so a ring
/// of them keeps its pattern on the screen whatever the yaw.
// ponytail: z is taken toward the viewer; fix the sign with the first screenshot against a ring.
pub fn yawed(point: Vector, offset: Vector, eye: &Eye) -> Vector {
    let toward = scaled(eye.ground_forward(), -offset[2]);
    add(add(add(point, scaled(eye.ground_right(), offset[0])), [0.0, offset[1], 0.0]), toward)
}

/// The screen angle (degrees) from one projected point to another, both in screen px (y down): 0 to the right, 90 down,
/// 180 to the left, 270 up. The shot STRs point along +x, so this as a screen turn points one at its target.
pub fn aim_turn(from: [f32; 2], to: [f32; 2]) -> f32 {
    let [dx, dy] = [to[0] - from[0], to[1] - from[1]];
    let length = dx.hypot(dy);
    if length == 0.0 {
        return 0.0;
    }
    let theta = (dx / length).clamp(-1.0, 1.0).acos().to_degrees();
    if dy < 0.0 { 360.0 - theta } else { theta }
}

/// The part of a power-of-two surface (next power of two ≥ each side) an image of this size fills from its top left.
pub fn padded_ratio(size: [f32; 2]) -> [f32; 2] {
    size.map(|side| side / (side.max(1.0) as u32).next_power_of_two() as f32)
}

/// The "image-exact" texture rectangle (u0, u1, v0, v1) of an image on its power-of-two surface.
pub fn exact_uv(size: [f32; 2]) -> [f32; 4] {
    let [width, height] = size.map(|side| (side.max(1.0) as u32).next_power_of_two() as f32);
    [0.5 / width, (size[0] + 0.5) / width, 0.5 / height, (size[1] + 0.5) / height]
}

/// A square's corners v0 (−1, 1), v1 (1, 1), v2 (−1, −1), v3 (1, −1).
const SQUARE: [[f32; 2]; 4] = [[-1.0, 1.0], [1.0, 1.0], [-1.0, -1.0], [1.0, -1.0]];

/// Two triangles of corners v0 … v3 (v0 v1 v2, v2 v1 v3).
fn triangles(corners: [Vector; 4], uv: [[f32; 2]; 4], tint: [f32; 4]) -> Vec<MeshVertex> {
    [0, 1, 2, 2, 1, 3].into_iter().map(|corner| MeshVertex { position: corners[corner], uv: uv[corner], tint }).collect()
}

/// A textured square an object moves itself.
#[derive(Debug, Clone)]
pub struct Quad {
    /// The texture's path.
    pub texture: String,
    pub corners: [Vector; 4],
    /// The texture rectangle: u0, u1, v0, v1.
    pub uv: [f32; 4],
    pub color: [f32; 3],
    /// Only its low byte is drawn.
    pub alpha: i32,
    pub scale: f32,
    /// 5/2 (source alpha, one); else 5/6 (source alpha, inverse source alpha).
    pub additive: bool,
    /// Its own age, 0 at its first update.
    pub counter: u32,
    pub flags: u32,
}

impl Quad {
    pub fn new(texture: &str) -> Self {
        Self {
            texture: texture.to_owned(),
            corners: SQUARE.map(|[x, y]| [x, y, 0.0]),
            uv: [0.0, 1.0, 0.0, 1.0],
            color: [1.0; 3],
            alpha: 255,
            scale: 1.0,
            additive: true,
            counter: 0,
            flags: FLAGS_EFFECT,
        }
    }

    /// The alpha drawn, of 255.
    pub fn drawn_alpha(&self) -> u8 {
        (self.alpha & 0xff) as u8
    }

    /// v0 (u0, v0), v1 (u1, v0), v2 (u0, v1), v3 (u1, v1).
    pub fn uv_corners(&self) -> [[f32; 2]; 4] {
        let [u0, u1, v0, v1] = self.uv;
        [[u0, v0], [u1, v0], [u0, v1], [u1, v1]]
    }

    /// The square on two axes through `point`, of half-side `scale`.
    pub fn place(&mut self, point: Vector, x: Vector, y: Vector) {
        let half = self.scale;
        self.corners = SQUARE.map(|[cx, cy]| add(point, add(scaled(x, cx * half), scaled(y, cy * half))));
    }

    /// Facing the camera.
    pub fn billboard(&mut self, point: Vector, eye: &Eye) {
        self.place(point, eye.right, eye.up);
    }

    /// Standing up, turned with the camera's longitude only.
    pub fn upright(&mut self, point: Vector, eye: &Eye) {
        self.place(point, eye.ground_right(), [0.0, 1.0, 0.0]);
    }

    /// Lying on the ground along the cell grid.
    // ponytail: the square's up is world +z.
    pub fn flat(&mut self, point: Vector) {
        self.place(point, [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]);
    }

    /// Lying on the ground with its edges along the screen.
    pub fn flat_on_screen(&mut self, point: Vector, eye: &Eye) {
        self.place(point, eye.ground_right(), eye.ground_forward());
    }

    pub fn face(&self) -> Face<'_> {
        let tint = [self.color[0], self.color[1], self.color[2], f32::from(self.drawn_alpha()) / 255.0];
        Face { texture: Tex::Path(&self.texture), vertices: triangles(self.corners, self.uv_corners(), tint), additive: self.additive, flags: self.flags }
    }
}

/// What a sprite card does at its action's last frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum End {
    Loop,
    /// Sets the end mark.
    Stop,
    Hold,
    /// Never steps.
    Still,
}

/// One frame step of a sprite card: the frame goes up when the delay (cut to a whole number) is 0 or `e8` is a multiple
/// of it at least as big; at the last frame the end mode applies. Returns the frame and whether the card has ended.
pub fn card_step(frame: usize, frames: usize, e8: u32, delay: f32, mode: End) -> (usize, bool) {
    let delay = delay as u32;
    let due = delay == 0 || e8 >= delay && e8.is_multiple_of(delay);
    if mode == End::Still || !due {
        return (frame, false);
    }
    if frame + 1 < frames {
        return (frame + 1, false);
    }
    match mode {
        End::Loop => (0, false),
        End::Stop => (frame, true),
        End::Hold | End::Still => (frame, false),
    }
}

/// A card layer's colour: with layer colours on, the layer's RGB and the two alphas multiplied; off, the card's own.
pub fn card_color(layer_colours: bool, color: [f32; 3], alpha: f32, layer: [u8; 4]) -> [f32; 4] {
    if layer_colours {
        let alpha = (alpha / 255.0 * f32::from(layer[3]) / 255.0 * 255.0).trunc();
        [f32::from(layer[0]) / 255.0, f32::from(layer[1]) / 255.0, f32::from(layer[2]) / 255.0, alpha / 255.0]
    } else {
        [color[0], color[1], color[2], alpha / 255.0]
    }
}

/// A sprite (SPR + ACT) shown as a camera-facing card at a point.
#[derive(Debug, Clone)]
pub struct Card {
    /// The sprite's path, without extension.
    pub sprite: String,
    pub action: usize,
    pub frame: usize,
    pub end: End,
    /// Ticks a frame (cut to a whole number).
    pub delay: f32,
    pub ended: bool,
    pub layer_colours: bool,
    pub bias: f32,
    /// Screen px added with an owner unit.
    pub offset: [f32; 2],
    pub scale: f32,
    /// Degrees, the card turned in its plane.
    pub spin: f32,
    pub color: [f32; 3],
    /// Of 255.
    pub alpha: f32,
    pub flags: u32,
    /// Its own counter, +1 an update.
    pub e8: u32,
    /// The frame's layers and the action's frame count, from the last step.
    pub layers: Vec<Placed>,
    pub frames: usize,
}

impl Card {
    pub fn new(sprite: &str) -> Self {
        Self {
            sprite: sprite.to_owned(),
            action: 0,
            frame: 0,
            end: End::Loop,
            delay: 0.0,
            ended: false,
            layer_colours: true,
            bias: 0.0,
            offset: [0.0; 2],
            scale: 1.0,
            spin: 0.0,
            color: [1.0; 3],
            alpha: 255.0,
            flags: FLAGS_EFFECT,
            e8: 0,
            layers: Vec::new(),
            frames: 0,
        }
    }

    /// Once a drawn frame (a card not drawn never steps); reads the frame's layers.
    pub fn step(&mut self, assets: &mut dyn Assets) {
        if let Some((frames, _)) = assets.sprite(&self.sprite, self.action, self.frame) {
            (self.frame, self.ended) = match card_step(self.frame, frames, self.e8, self.delay, self.end) {
                (frame, true) => (frame, true),
                (frame, false) => (frame, self.ended),
            };
            self.frames = frames;
        }
        self.layers = assets.sprite(&self.sprite, self.action, self.frame).map(|(_, layers)| layers).unwrap_or_default();
    }

    /// The card's faces at `point`, `size` being the owner's size factor (1 with none).
    // ponytail: the screen offset is taken at 1/7 unit a px (zoom 1); the head point and lift options come with the
    // classes that ask for them.
    pub fn faces<'a>(&'a self, point: Vector, size: f32, owned: bool, eye: &Eye, out: &mut Vec<Face<'a>>) {
        if self.ended {
            return;
        }
        let unit = PIXEL * size * self.scale;
        let point = if owned { add(point, add(scaled(eye.right, self.offset[0] * PIXEL), scaled(eye.up, -self.offset[1] * PIXEL))) } else { point };
        for (index, &(key, offset, extent, degrees)) in self.layers.iter().enumerate() {
            let centre = [offset[0] + extent[0] / 2.0, offset[1] + extent[1] / 2.0];
            let (sin, cos) = (degrees + self.spin).to_radians().sin_cos();
            // The signed width mirrors the picture.
            let corner = |[u, v]: [f32; 2]| {
                let [x, y] = [(u - 0.5) * extent[0], (v - 0.5) * extent[1]];
                let [x, y] = [x * cos - y * sin + centre[0], x * sin + y * cos + centre[1]];
                add(point, add(scaled(eye.right, x * unit), scaled(eye.up, -y * unit)))
            };
            let uv = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]];
            let bias = self.bias + LAYER_BIAS * index as f32;
            let corners = uv.map(|at| eye.nearer(corner(at), bias));
            let tint = card_color(self.layer_colours, self.color, self.alpha, key.3);
            out.push(Face {
                texture: Tex::Sprite(&self.sprite, key),
                vertices: triangles(corners, uv, tint),
                additive: self.flags == FLAGS_EFFECT,
                flags: self.flags,
            });
        }
    }
}

/// `a\b\name.str` → `a\b\min_name.str`: the file `/mineffect` plays.
pub fn min_name(file: &str) -> String {
    match file.rfind(['\\', '/']) {
        Some(slash) => format!("{}min_{}", &file[..=slash], &file[slash + 1..]),
        None => format!("min_{file}"),
    }
}

/// An STR played as a piece of an object, which sets its point every tick and steps it.
pub struct StrPart {
    pub loaded: Option<Rc<Loaded>>,
    /// The world point it plays at.
    pub point: Vector,
    /// The step offset in STR px.
    pub offset: [f32; 2],
    pub flags: u32,
    /// Start delay, cut, loop and hold.
    pub timeline: Timeline,
    /// Scale (about the point), screen turn (positive only), mirror and the alpha multiplier.
    pub controls: Controls,
    pub bias: f32,
    /// Drawn in the world (`faces` 3-D) instead of on the screen.
    pub world: bool,
    shown: Option<i32>,
}

impl StrPart {
    /// A part playing `file` (its `min_` file under /mineffect); a file that can't load makes a part ended at once.
    // ponytail: the game keeps such an object alive and invisible.
    pub fn new(file: &str, min: bool, assets: &mut dyn Assets) -> Self {
        let file = if min { min_name(file) } else { file.to_owned() };
        Self {
            loaded: assets.str(&file),
            point: [0.0; 3],
            offset: STR_OFFSET,
            flags: FLAGS_EFFECT,
            timeline: Timeline::default(),
            controls: Controls::default(),
            bias: 0.0,
            world: false,
            shown: None,
        }
    }

    /// One STR step; false once the STR is over (the object then frees the part).
    pub fn step(&mut self) -> bool {
        let Some(loaded) = &self.loaded else { return false };
        match self.timeline.step(loaded.animation.last_key()) {
            Step::Over => false,
            Step::Show(frame) => {
                self.shown = Some(frame);
                true
            }
            Step::Wait | Step::Keep => true,
        }
    }

    /// Its layers' faces now (layer 0 is never drawn). On the screen each STR px is a sprite px about the point, moved by
    /// the step offset; in the world (3-D) a camera-facing quad whose STR px are `scale` world units, with real depth.
    pub fn faces<'a>(&'a self, eye: &Eye, out: &mut Vec<Face<'a>>) {
        let (Some(loaded), Some(frame)) = (&self.loaded, self.shown) else { return };
        // The 3-D draw's size is the scale in world units; the 2-D draw's is the scale in sprite px.
        let (unit, controls) = if self.world {
            (1.0, Controls { offset: [0.0; 2], ..self.controls })
        } else {
            (PIXEL, Controls { offset: [self.controls.offset[0] + self.offset[0], self.controls.offset[1] + self.offset[1]], ..self.controls })
        };
        let point = eye.nearer(self.point, self.bias);
        for (index, layer) in loaded.animation.layers.iter().enumerate().skip(1) {
            let Some(state) = layer.state(frame as f32) else { continue };
            let corner = |corner: usize| {
                let [x, y] = str_corner(state.position, [state.corners[corner], state.corners[4 + corner]], state.angle);
                let [x, y] = world_str::transform([x, y], &controls);
                add(point, add(scaled(eye.right, x * unit), scaled(eye.up, -y * unit)))
            };
            // STR corners go top left, top right, bottom right, bottom left; v0 … v3 are top left, top right, bottom left,
            // bottom right.
            let corners = [corner(0), corner(1), corner(3), corner(2)];
            let mut tint = state.color;
            tint[3] = (tint[3] * 255.0 * self.controls.alpha).trunc() / 255.0;
            let uv = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]];
            out.push(Face {
                texture: Tex::Str(loaded, index, state.texture),
                vertices: triangles(corners, uv, tint),
                additive: state.additive,
                flags: self.flags,
            });
        }
    }
}

impl StrPart {
    /// Its layers' faces lying on the ground: the 3-D draw's layers, colours, alphas and mirrors, with each corner placed in
    /// the ground's plane (the STR's x along the camera's right on the ground, its up along the viewing direction on the
    /// ground, `scale` units per STR px, turned by the controls' turn about the vertical) and set `FLAT_LIFT` over the
    /// ground under it, so the print follows the terrain.
    // ponytail: the handedness of the flat frame and the turn's sign are unchecked: look at one walk.
    pub fn flat_faces<'a>(&'a self, eye: &Eye, ground: &dyn Fn(f32, f32) -> f32, out: &mut Vec<Face<'a>>) {
        let (Some(loaded), Some(frame)) = (&self.loaded, self.shown) else { return };
        let controls = Controls { offset: [0.0; 2], ..self.controls };
        let (right, forward) = (eye.ground_right(), eye.ground_forward());
        for (index, layer) in loaded.animation.layers.iter().enumerate().skip(1) {
            let Some(state) = layer.state(frame as f32) else { continue };
            let corner = |corner: usize| {
                let [x, y] = str_corner(state.position, [state.corners[corner], state.corners[4 + corner]], state.angle);
                let [x, y] = world_str::transform([x, y], &controls);
                let mut at = add(self.point, add(scaled(right, x), scaled(forward, -y)));
                at[1] = ground(at[0], at[2]) + FLAT_LIFT;
                at
            };
            let corners = [corner(0), corner(1), corner(3), corner(2)];
            let mut tint = state.color;
            tint[3] = (tint[3] * 255.0 * self.controls.alpha).trunc() / 255.0;
            let uv = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]];
            out.push(Face {
                texture: Tex::Str(loaded, index, state.texture),
                vertices: triangles(corners, uv, tint),
                additive: state.additive,
                flags: FLAGS_EFFECT,
            });
        }
    }
}

/// A flat STR lies this far over the ground under each corner.
pub const FLAT_LIFT: f32 = 2.0;

/// What objects load: textures (their image size), STR files, sprite frames.
pub trait Assets {
    fn texture(&mut self, path: &str) -> Option<[f32; 2]>;
    fn str(&mut self, file: &str) -> Option<Rc<Loaded>>;
    /// The action's frame count and the frame's layers.
    fn sprite(&mut self, path: &str, action: usize, frame: usize) -> Option<(usize, Vec<Placed>)>;
}

/// A sound or a camera shake at an object's age.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// A file under data\wav\, at the object's place or (`listener`) at the own player's.
    Sound { file: String, listener: bool },
    /// The 650 ms camera shake.
    Quake,
    /// The owner starts leaving Kagemusya's afterimages (Shadow Exceed's end).
    Afterimage,
}

/// What every object has.
#[derive(Debug, Clone, PartialEq)]
pub struct Fx {
    pub id: u32,
    /// The unit it follows (`Some(None)` = the player), None for none.
    pub owner: Option<Option<u32>>,
    pub position: Vector,
    /// The target's position at launch.
    pub aim: Vector,
    pub target: Option<Option<u32>>,
    /// Its age in ticks, 0 at its first update.
    pub age: u32,
    pub length: u32,
    /// A level or a count, −1 when not given.
    pub a7: i32,
    /// A launch for the same owner and id returns this one.
    pub single: bool,
    /// What the launcher told this copy apart by (a hat effect's number), 0 for none.
    pub tag: u32,
    pub life: u32,
    pub far: bool,
    pub events: Vec<(u32, Event)>,
    /// The ground unit it stands for is gone: its pieces finish without new ones.
    pub released: bool,
    /// /mineffect was on at launch.
    pub min: bool,
    /// Its start delay in ms, for the classes that hold for one.
    pub delay: i32,
    /// Its first update has run.
    updated: bool,
}

impl Fx {
    /// A launch's object before its `begin`.
    fn of(launch: &Launch) -> Self {
        Self {
            id: launch.id,
            owner: launch.owner,
            position: launch.point,
            aim: launch.aim,
            target: launch.target,
            age: 0,
            length: LENGTH,
            a7: launch.a7.unwrap_or(-1),
            single: false,
            tag: launch.tag,
            life: launch.life,
            far: false,
            events: Vec::new(),
            released: false,
            min: launch.min,
            delay: launch.delay,
            updated: false,
        }
    }

    /// Alive while younger than its length.
    pub fn timed(&self) -> bool {
        self.age < self.length
    }

    /// Alive while it has no owner or the owner is there.
    pub fn owned(&self, scene: &Scene) -> bool {
        self.owner.is_none_or(|owner| (scene.feet)(owner).is_some())
    }

    /// Reads the target's position again.
    pub fn aim_at(&mut self, scene: &Scene) {
        if let Some(at) = self.target.and_then(|target| (scene.feet)(target)) {
            self.aim = at;
        }
    }
}

/// What a frame's draw needs to know.
pub struct Seen<'a> {
    pub eye: Eye,
    /// Whether an id draws (all of them, or with /effect off the force-drawn ones).
    pub shows: &'a dyn Fn(u32) -> bool,
    /// Whether a unit's body is hidden.
    pub hidden: &'a dyn Fn(Option<u32>) -> bool,
    /// A unit's size factor.
    pub size: &'a dyn Fn(Option<u32>) -> f32,
    /// The ground's height (world y) under a world x and z.
    pub ground: &'a dyn Fn(f32, f32) -> f32,
    /// On ecl_fild01, where the cut-down alphas of some objects apply.
    pub dim: bool,
}

/// One kind of world object.
pub trait Class {
    fn begin(&mut self, _fx: &mut Fx, _assets: &mut dyn Assets) {}
    fn update(&mut self, fx: &mut Fx, scene: &Scene, assets: &mut dyn Assets);
    /// Once a drawn frame, after the ticks: the sprite cards step here.
    fn frame(&mut self, _fx: &Fx, _assets: &mut dyn Assets) {}
    fn draw<'a>(&'a self, fx: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>);
    /// Timed unless the class says (`Fx::owned`, its STR parts playing, or its own rule).
    fn alive(&self, fx: &Fx, _scene: &Scene) -> bool {
        fx.timed()
    }
    /// The class itself, for a launcher that changes a running object (`WorldFx::class_mut`).
    fn any(&mut self) -> Option<&mut dyn std::any::Any> {
        None
    }
}

/// Makes a class's object.
pub type Make = fn() -> Box<dyn Class>;

/// A launch.
#[derive(Debug, Clone, PartialEq)]
pub struct Launch {
    pub id: u32,
    pub owner: Option<Option<u32>>,
    pub point: Vector,
    pub target: Option<Option<u32>>,
    /// The target's position now.
    pub aim: Vector,
    pub a7: Option<i32>,
    pub unique: bool,
    pub life: u32,
    pub tag: u32,
    /// /mineffect is on.
    pub min: bool,
    /// The start delay in ms.
    pub delay: i32,
}

impl Launch {
    pub fn at(id: u32, owner: Option<Option<u32>>, point: Vector) -> Self {
        Self { id, owner, point, target: None, aim: point, a7: None, unique: false, life: 0, tag: 0, min: false, delay: 0 }
    }
}

pub struct Object {
    pub serial: u32,
    pub fx: Fx,
    pub class: Box<dyn Class>,
}

/// The registry and the running objects.
pub struct WorldFx {
    classes: HashMap<u32, Make>,
    objects: Vec<Object>,
    serial: u32,
}

impl Default for WorldFx {
    /// An empty registry with the classes registered so far.
    fn default() -> Self {
        let mut fx = Self { classes: HashMap::new(), objects: Vec::new(), serial: 0 };
        area4::register(&mut fx);
        drops::register(&mut fx);
        footprints::register(&mut fx);
        heal::register(&mut fx);
        jobs3::register(&mut fx);
        shots::register(&mut fx);
        status4::register(&mut fx);
        summoner::register(&mut fx);
        fx
    }
}

impl WorldFx {
    pub fn register(&mut self, id: u32, make: Make) {
        self.classes.insert(id, make);
    }

    pub fn registered(&self, id: u32) -> bool {
        self.classes.contains_key(&id)
    }

    /// The class of the running copy of (owner, one of `ids`), when it is a `T`.
    pub fn class_mut<T: 'static>(&mut self, owner: Option<Option<u32>>, ids: &[u32]) -> Option<(&mut Fx, &mut T)> {
        let object = self.objects.iter_mut().find(|object| object.fx.owner == owner && ids.contains(&object.fx.id))?;
        Some((&mut object.fx, object.class.any()?.downcast_mut::<T>()?))
    }

    /// Whether a copy of (owner, id) is running.
    pub fn running(&self, owner: Option<Option<u32>>, id: u32) -> bool {
        self.objects.iter().any(|object| object.fx.owner == owner && object.fx.id == id)
    }

    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }

    pub fn get_mut(&mut self, serial: u32) -> Option<&mut Object> {
        self.objects.iter_mut().find(|object| object.serial == serial)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Object> {
        self.objects.iter()
    }

    /// Starts a registered id at its point and runs its `begin`, or returns the copy running for (owner, id) when the
    /// launch is unique or that copy is single. None for an id not registered.
    pub fn launch(&mut self, launch: &Launch, assets: &mut dyn Assets) -> Option<u32> {
        let make = *self.classes.get(&launch.id)?;
        if let Some(running) = self.objects.iter_mut().find(|object| object.fx.owner == launch.owner && object.fx.id == launch.id)
            && (launch.unique || running.fx.single)
        {
            // A ground unit seen again takes back its released copy.
            running.fx.released &= !launch.unique;
            return Some(running.serial);
        }
        let mut fx = Fx::of(launch);
        let mut class = make();
        class.begin(&mut fx, assets);
        self.serial += 1;
        self.objects.push(Object { serial: self.serial, fx, class });
        Some(self.serial)
    }

    /// Removes the running copy of (owner, id).
    pub fn remove(&mut self, owner: Option<Option<u32>>, id: u32) {
        self.objects.retain(|object| !(object.fx.owner == owner && object.fx.id == id));
    }

    /// Removes the running copy of (owner, id) that was launched with this tag.
    pub fn remove_tagged(&mut self, owner: Option<Option<u32>>, id: u32, tag: u32) {
        self.objects.retain(|object| !(object.fx.owner == owner && object.fx.id == id && object.fx.tag == tag));
    }

    /// The ground unit `owner` is gone: its objects let their pieces finish.
    pub fn release(&mut self, owner: Option<Option<u32>>) {
        self.objects.iter_mut().filter(|object| object.fx.owner == owner).for_each(|object| object.fx.released = true);
    }

    pub fn remove_all_of(&mut self, owner: Option<Option<u32>>) {
        self.objects.retain(|object| object.fx.owner != owner);
    }

    /// A map load.
    pub fn remove_all(&mut self) {
        self.objects.clear();
    }

    /// A status object: its status turning on starts it afresh, turning off removes it.
    pub fn status(&mut self, launch: &Launch, on: bool, assets: &mut dyn Assets) -> Option<u32> {
        self.remove(launch.owner, launch.id);
        if on { self.launch(launch, assets) } else { None }
    }

    /// One 16 ms tick, before the units move: (1) the dead go; then each (2) follows its owner, (3) stands still while far
    /// (130 units or more from the own player), (4) fires this age's events, (5) updates and (6) ages.
    pub fn tick(&mut self, scene: &Scene, assets: &mut dyn Assets) -> Vec<Fired> {
        let mut fired = Vec::new();
        self.objects.retain(|object| object.class.alive(&object.fx, scene));
        for Object { fx, class, .. } in &mut self.objects {
            if let Some(feet) = fx.owner.and_then(|owner| (scene.feet)(owner)) {
                fx.position = feet;
            }
            let distance = std::array::from_fn::<f32, 3, _>(|axis| fx.position[axis] - scene.player[axis]).map(|d| d * d).iter().sum::<f32>().sqrt();
            fx.far = distance >= world_str::FAR;
            if fx.far {
                continue;
            }
            for (age, event) in &fx.events {
                match event {
                    _ if *age != fx.age => {}
                    Event::Sound { file, listener } => fired.push(Fired::Sound(file.clone(), if *listener { scene.player } else { fx.position })),
                    Event::Quake => fired.push(Fired::Quake),
                    Event::Afterimage => fired.extend(fx.owner.map(Fired::Afterimage)),
                }
            }
            class.update(fx, scene, assets);
            fx.updated = true;
            fx.age += 1;
        }
        fired
    }

    /// Whether an object draws: updated once, near, shown under /effect, its owner's body not hidden.
    fn drawn(fx: &Fx, seen: &Seen) -> bool {
        fx.updated && !fx.far && (seen.shows)(fx.id) && !fx.owner.is_some_and(|owner| (seen.hidden)(owner))
    }

    /// Once a drawn frame: the drawn objects' frame step.
    pub fn frame(&mut self, seen: &Seen, assets: &mut dyn Assets) {
        for Object { fx, class, .. } in &mut self.objects {
            if Self::drawn(fx, seen) {
                class.frame(fx, assets);
            }
        }
    }

    /// The drawn objects' faces.
    pub fn faces<'a>(&'a self, seen: &Seen) -> Vec<Face<'a>> {
        let mut faces = Vec::new();
        for Object { fx, class, .. } in &self.objects {
            if Self::drawn(fx, seen) {
                class.draw(fx, seen, &mut faces);
            }
        }
        faces
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustnarok_client::effect::{Key, Layer, Str};
    use std::cell::RefCell;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    fn dot(a: Vector, b: Vector) -> f32 {
        a.iter().zip(b).map(|(a, b)| a * b).sum()
    }

    fn sub(a: Vector, b: Vector) -> Vector {
        std::array::from_fn(|axis| a[axis] - b[axis])
    }

    /// Loads nothing but what the test gives it.
    #[derive(Default)]
    struct Fake {
        strs: HashMap<String, Rc<Loaded>>,
        frames: usize,
    }

    impl Assets for Fake {
        fn texture(&mut self, _: &str) -> Option<[f32; 2]> {
            Some([64.0; 2])
        }
        fn str(&mut self, file: &str) -> Option<Rc<Loaded>> {
            self.strs.get(file).cloned()
        }
        fn sprite(&mut self, _: &str, _: usize, frame: usize) -> Option<(usize, Vec<Placed>)> {
            Some((self.frames, vec![((0, 0, frame, [255; 4], 0), [-32.0, -32.0], [64.0, 64.0], 0.0)]))
        }
    }

    thread_local! {
        /// What the test class did: (id, age at update).
        static UPDATES: RefCell<Vec<(u32, u32)>> = const { RefCell::new(Vec::new()) };
    }

    /// A class that draws one quad at its position and logs its updates.
    struct Dot {
        quad: Quad,
    }

    impl Class for Dot {
        fn begin(&mut self, fx: &mut Fx, _: &mut dyn Assets) {
            fx.single = fx.id == 2;
            fx.events.push((30, Event::Quake));
        }
        fn update(&mut self, fx: &mut Fx, _: &Scene, _: &mut dyn Assets) {
            UPDATES.with(|log| log.borrow_mut().push((fx.id, fx.age)));
            self.quad.flat(fx.position);
        }
        fn draw<'a>(&'a self, _: &Fx, _: &Seen, out: &mut Vec<Face<'a>>) {
            out.push(self.quad.face());
        }
    }

    fn make_dot() -> Box<dyn Class> {
        Box::new(Dot { quad: Quad::new("a.bmp") })
    }

    fn registry() -> WorldFx {
        let mut fx = WorldFx::default();
        for id in [1, 2, 228, 1088] {
            fx.register(id, make_dot);
        }
        fx
    }

    fn ticks(fx: &mut WorldFx, count: usize, player: Vector, units: &HashMap<u32, Vector>) -> Vec<Fired> {
        let feet = |unit: Option<u32>| match unit {
            None => Some(player),
            Some(id) => units.get(&id).copied(),
        };
        let body = |_: Option<u32>| None;
        let scene = Scene { player, feet: &feet, body: &body, longitude: 0.0, right: [1.0, 0.0, 0.0], up: [0.0, 1.0, 0.0], ground: &|_, _| 0.0 };
        (0..count).flat_map(|_| fx.tick(&scene, &mut Fake::default())).collect()
    }

    fn seen<'a>(shows: &'a dyn Fn(u32) -> bool, hidden: &'a dyn Fn(Option<u32>) -> bool) -> Seen<'a> {
        Seen { eye: Eye::at(45.0, 0.0), shows, hidden, size: &|_| 1.0, ground: &|_, _| 0.0, dim: false }
    }

    #[test]
    fn a_launch_starts_a_registered_id_at_its_point() {
        let mut fx = registry();
        let at = [3.0, 0.0, 4.0];
        let serial = fx.launch(&Launch::at(1, None, at), &mut Fake::default()).unwrap();
        let object = fx.get_mut(serial).unwrap();
        assert_eq!((object.fx.position, object.fx.length, object.fx.a7), (at, 60, -1));
        let given = Launch { a7: Some(5), ..Launch::at(1, None, at) };
        let serial = fx.launch(&given, &mut Fake::default()).unwrap();
        assert_eq!(fx.get_mut(serial).unwrap().fx.a7, 5);
        // Unregistered ids go the old way.
        assert_eq!(fx.launch(&Launch::at(3, None, at), &mut Fake::default()), None);
        assert!(!fx.registered(3));
    }

    #[test]
    fn unique_and_single_return_the_running_copy() {
        let mut fx = registry();
        let mut fake = Fake::default();
        let owner = Some(Some(7));
        let first = fx.launch(&Launch::at(1, owner, [0.0; 3]), &mut fake).unwrap();
        // Neither: a second copy.
        let second = fx.launch(&Launch::at(1, owner, [0.0; 3]), &mut fake).unwrap();
        assert_ne!(first, second);
        let unique = Launch { unique: true, ..Launch::at(1, owner, [0.0; 3]) };
        assert!(fx.launch(&unique, &mut fake).is_some_and(|serial| serial == first || serial == second));
        // Id 2's objects are single.
        let single = fx.launch(&Launch::at(2, owner, [0.0; 3]), &mut fake).unwrap();
        assert_eq!(fx.launch(&Launch::at(2, owner, [0.0; 3]), &mut fake), Some(single));
        // Another owner is independent.
        assert_ne!(fx.launch(&Launch::at(2, Some(Some(8)), [0.0; 3]), &mut fake), Some(single));
        assert_eq!(fx.iter().count(), 4);
    }

    #[test]
    fn removals() {
        let mut fx = registry();
        let mut fake = Fake::default();
        for (id, owner) in [(1, Some(Some(7))), (2, Some(Some(7))), (1, Some(Some(8))), (1, None)] {
            fx.launch(&Launch::at(id, owner, [0.0; 3]), &mut fake);
        }
        fx.remove(Some(Some(7)), 1);
        assert_eq!(fx.iter().count(), 3);
        fx.remove_all_of(Some(Some(7)));
        assert_eq!(fx.iter().map(|object| object.fx.owner).collect::<Vec<_>>(), [Some(Some(8)), None]);
        fx.remove_all();
        assert!(fx.is_empty());
        // A status turning on starts afresh, off removes it.
        let launch = Launch::at(1, Some(Some(7)), [0.0; 3]);
        let first = fx.status(&launch, true, &mut fake).unwrap();
        let again = fx.status(&launch, true, &mut fake).unwrap();
        assert!(again != first && fx.iter().count() == 1);
        assert_eq!(fx.status(&launch, false, &mut fake), None);
        assert!(fx.is_empty());
    }

    #[test]
    fn an_owner_follows_its_unit_and_owner_none_stays() {
        let mut fx = registry();
        let mut fake = Fake::default();
        let follower = fx.launch(&Launch::at(1, Some(Some(7)), [0.0; 3]), &mut fake).unwrap();
        let still = fx.launch(&Launch::at(1, None, [5.0, 0.0, 5.0]), &mut fake).unwrap();
        let mut units = HashMap::from([(7, [1.0, 0.0, 1.0])]);
        ticks(&mut fx, 1, [0.0; 3], &units);
        units.insert(7, [4.0, 2.0, 6.0]);
        ticks(&mut fx, 1, [0.0; 3], &units);
        assert_eq!(fx.get_mut(follower).unwrap().fx.position, [4.0, 2.0, 6.0]);
        assert_eq!(fx.get_mut(still).unwrap().fx.position, [5.0, 0.0, 5.0]);
    }

    #[test]
    fn the_dead_go_before_they_update() {
        let mut fx = registry();
        let serial = fx.launch(&Launch::at(1, None, [0.0; 3]), &mut Fake::default()).unwrap();
        UPDATES.with(|log| log.borrow_mut().clear());
        ticks(&mut fx, 61, [0.0; 3], &HashMap::new());
        // Ages 0 … 59 update; at 60 it is removed first.
        let updates = UPDATES.with(|log| log.borrow().clone());
        assert_eq!(updates.len(), 60);
        assert_eq!(updates.last(), Some(&(1, 59)));
        assert!(fx.get_mut(serial).is_none());
    }

    #[test]
    fn a_far_object_neither_ages_nor_updates_nor_fires() {
        let mut fx = registry();
        let serial = fx.launch(&Launch::at(1, None, [0.0; 3]), &mut Fake::default()).unwrap();
        let near = [129.9, 0.0, 0.0];
        ticks(&mut fx, 29, near, &HashMap::new());
        assert_eq!(fx.get_mut(serial).unwrap().fx.age, 29);
        UPDATES.with(|log| log.borrow_mut().clear());
        // At 130 it stands still, and its age-30 event waits.
        assert!(ticks(&mut fx, 2, [130.0, 0.0, 0.0], &HashMap::new()).is_empty());
        assert!(fx.get_mut(serial).unwrap().fx.far);
        assert_eq!(fx.get_mut(serial).unwrap().fx.age, 29);
        ticks(&mut fx, 1, near, &HashMap::new());
        assert_eq!(fx.get_mut(serial).unwrap().fx.age, 30);
        assert_eq!(ticks(&mut fx, 1, near, &HashMap::new()), vec![Fired::Quake]);
        assert_eq!(UPDATES.with(|log| log.borrow().clone()), [(1, 29), (1, 30)]);
    }

    #[test]
    fn the_draw_gate() {
        let mut fx = registry();
        let shows_all = |_: u32| true;
        let none_hidden = |_: Option<u32>| false;
        let mut fake = Fake::default();
        fx.launch(&Launch::at(228, None, [0.0; 3]), &mut fake);
        fx.launch(&Launch::at(1088, None, [0.0; 3]), &mut fake);
        // Nothing before the first update.
        assert!(fx.faces(&seen(&shows_all, &none_hidden)).is_empty());
        ticks(&mut fx, 1, [0.0; 3], &HashMap::new());
        assert_eq!(fx.faces(&seen(&shows_all, &none_hidden)).len(), 2);
        // /effect off: the force-drawn 228 draws, 1088 not.
        assert_eq!(fx.faces(&seen(&crate::effects::force_drawn, &none_hidden)).len(), 1);
        for id in [228, 735, 738, 1110, 1135, 1214, 1217, 1225, 1262, 1265, 1617, 1619] {
            assert!(crate::effects::force_drawn(id), "{id}");
        }
        assert!(!crate::effects::force_drawn(1088));
        // A hidden owner's object doesn't draw; far ones neither.
        fx.launch(&Launch::at(1, Some(Some(7)), [0.0; 3]), &mut fake);
        ticks(&mut fx, 1, [0.0; 3], &HashMap::from([(7, [0.0; 3])]));
        let hides_7 = |unit: Option<u32>| unit == Some(7);
        assert_eq!(fx.faces(&seen(&shows_all, &hides_7)).len(), 2);
        ticks(&mut fx, 1, [500.0, 0.0, 0.0], &HashMap::from([(7, [0.0; 3])]));
        assert!(fx.faces(&seen(&shows_all, &none_hidden)).is_empty());
    }

    #[test]
    fn events_fire_at_their_age_at_the_object_or_the_listener() {
        let mut fx = registry();
        let serial = fx.launch(&Launch::at(1, None, [3.0, 0.0, 0.0]), &mut Fake::default()).unwrap();
        let events = &mut fx.get_mut(serial).unwrap().fx.events;
        events.clear();
        events.push((2, Event::Sound { file: "a.wav".into(), listener: false }));
        events.push((2, Event::Sound { file: "b.wav".into(), listener: true }));
        assert!(ticks(&mut fx, 2, [1.0, 0.0, 0.0], &HashMap::new()).is_empty());
        let fired = ticks(&mut fx, 1, [1.0, 0.0, 0.0], &HashMap::new());
        assert_eq!(fired, [Fired::Sound("a.wav".into(), [3.0, 0.0, 0.0]), Fired::Sound("b.wav".into(), [1.0, 0.0, 0.0])]);
    }

    #[test]
    fn the_quad() {
        let mut quad = Quad::new("a.bmp");
        assert!(quad.additive);
        quad.alpha = 256;
        assert_eq!(quad.drawn_alpha(), 0);
        quad.alpha = 300;
        assert_eq!(quad.drawn_alpha(), 44);
        quad.uv = [0.1, 0.9, 0.2, 0.8];
        assert_eq!(quad.uv_corners(), [[0.1, 0.2], [0.9, 0.2], [0.1, 0.8], [0.9, 0.8]]);
        // A billboard of half-side 2 faces the camera.
        let eye = Eye::at(40.0, 30.0);
        quad.scale = 2.0;
        quad.billboard([1.0, 2.0, 3.0], &eye);
        let v1 = sub(quad.corners[1], [1.0, 2.0, 3.0]);
        assert!(close(dot(v1, eye.right), 2.0) && close(dot(v1, eye.up), 2.0) && close(dot(v1, eye.forward), 0.0));
        // Upright: vertical, along the ground right.
        quad.upright([0.0; 3], &eye);
        assert!(close(quad.corners[0][1], 2.0) && close(quad.corners[2][1], -2.0));
        // Flat: on the ground.
        quad.flat_on_screen([0.0; 3], &eye);
        assert!(quad.corners.iter().all(|corner| corner[1] == 0.0));
        let face = quad.face();
        assert_eq!(face.vertices.len(), 6);
        assert_eq!(face.vertices[0].uv, [0.1, 0.2]);
    }

    #[test]
    fn padded_surfaces() {
        assert_eq!(padded_ratio([24.0, 24.0]), [0.75, 0.75]);
        assert_eq!(padded_ratio([27.0, 27.0]), [0.84375, 0.84375]);
        assert_eq!(padded_ratio([128.0, 128.0]), [1.0, 1.0]);
        assert_eq!(exact_uv([40.0, 40.0]), [0.5 / 64.0, 40.5 / 64.0, 0.5 / 64.0, 40.5 / 64.0]);
    }

    #[test]
    fn a_ring_of_yawed_offsets_turns_with_the_camera() {
        for longitude in [0.0, 90.0, 217.0] {
            let eye = Eye::at(45.0, longitude);
            let at = yawed([10.0, 0.0, 10.0], [2.0, 1.0, 3.0], &eye);
            let moved = sub(at, [10.0, 0.0, 10.0]);
            assert!(close(dot(moved, eye.ground_right()), 2.0) && close(moved[1], 1.0) && close(dot(moved, eye.ground_forward()), -3.0));
        }
    }

    #[test]
    fn the_card_step() {
        let run = |delay: f32, mode: End, frames: usize, e8s: std::ops::Range<u32>| {
            let mut frame = 0;
            let mut out = Vec::new();
            for e8 in e8s {
                let (next, ended) = card_step(frame, frames, e8, delay, mode);
                frame = next;
                out.push((frame, ended));
            }
            out
        };
        // d = 4: up at e8 = 4, 8, 12.
        let frames: Vec<usize> = run(4.0, End::Loop, 10, 0..13).into_iter().map(|(frame, _)| frame).collect();
        assert_eq!(frames, [0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3]);
        // d = 0: every call.
        assert_eq!(run(0.0, End::Loop, 10, 0..3).into_iter().map(|(frame, _)| frame).collect::<Vec<_>>(), [1, 2, 3]);
        // d = 2.8 is cut to 2.
        assert_eq!(run(2.8, End::Loop, 10, 0..5).into_iter().map(|(frame, _)| frame).collect::<Vec<_>>(), [0, 0, 1, 1, 2]);
        // The end modes at the last of three frames.
        assert_eq!(run(0.0, End::Loop, 3, 0..4).into_iter().map(|(frame, _)| frame).collect::<Vec<_>>(), [1, 2, 0, 1]);
        assert_eq!(run(0.0, End::Stop, 3, 0..3), [(1, false), (2, false), (2, true)]);
        assert_eq!(run(0.0, End::Hold, 3, 0..4), [(1, false), (2, false), (2, false), (2, false)]);
        assert_eq!(run(0.0, End::Still, 3, 0..4), [(0, false); 4]);
    }

    #[test]
    fn an_undrawn_card_does_not_step() {
        struct Carded(Card);
        impl Class for Carded {
            fn update(&mut self, _: &mut Fx, _: &Scene, _: &mut dyn Assets) {
                self.0.e8 += 1;
            }
            fn frame(&mut self, _: &Fx, assets: &mut dyn Assets) {
                self.0.step(assets);
            }
            fn draw<'a>(&'a self, fx: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
                self.0.faces(fx.position, 1.0, false, &seen.eye, out);
            }
        }
        let mut fx = WorldFx::default();
        fx.register(1, || Box::new(Carded(Card { end: End::Stop, ..Card::new("x") })));
        let mut fake = Fake { frames: 2, ..Fake::default() };
        let serial = fx.launch(&Launch::at(1, None, [0.0; 3]), &mut fake).unwrap();
        ticks(&mut fx, 1, [0.0; 3], &HashMap::new());
        let off = |_: u32| false;
        let on = |_: u32| true;
        let none_hidden = |_: Option<u32>| false;
        for _ in 0..5 {
            fx.frame(&seen(&off, &none_hidden), &mut fake);
        }
        let card = |fx: &mut WorldFx| fx.get_mut(serial).unwrap().fx.clone();
        assert!(card(&mut fx).updated);
        assert_eq!(fx.faces(&seen(&on, &none_hidden)).len(), 0, "no layers read while undrawn");
        fx.frame(&seen(&on, &none_hidden), &mut fake);
        fx.frame(&seen(&on, &none_hidden), &mut fake);
        // Two drawn frames: 0 → 1, then the end at the last frame.
        assert!(fx.faces(&seen(&on, &none_hidden)).is_empty());
    }

    #[test]
    fn card_colours_and_sizes() {
        let color = card_color(true, [0.5; 3], 128.0, [255, 0, 51, 128]);
        assert!(close(color[0], 1.0) && close(color[1], 0.0) && close(color[2], 0.2) && close(color[3], 64.0 / 255.0));
        assert_eq!(card_color(false, [0.5; 3], 128.0, [255, 0, 51, 128]), [0.5, 0.5, 0.5, 128.0 / 255.0]);
        let eye = Eye::at(30.0, 0.0);
        let mut card = Card::new("x");
        card.layers = vec![((0, 0, 0, [255; 4], 0), [-32.0, -32.0], [64.0, 64.0], 0.0)];
        for (size, width) in [(1.0, 64.0 / 7.0), (1.5, 96.0 / 7.0)] {
            let mut faces = Vec::new();
            card.faces([0.0; 3], size, true, &eye, &mut faces);
            let vertices = &faces[0].vertices;
            // v0 → v1 spans the width, along the camera's right.
            assert!(close(dot(sub(vertices[1].position, vertices[0].position), eye.right), width));
        }
        // A mirrored layer (negative width) swaps left and right.
        card.layers[0].2 = [-64.0, 64.0];
        card.layers[0].1 = [32.0, -32.0];
        let mut faces = Vec::new();
        card.faces([0.0; 3], 1.0, false, &eye, &mut faces);
        assert!(close(dot(sub(faces[0].vertices[1].position, faces[0].vertices[0].position), eye.right), -64.0 / 7.0));
        // Flags 5 add; others blend.
        assert!(faces[0].additive);
    }

    #[test]
    fn faces_go_to_their_lists() {
        for (flags, list) in [
            (0, List::Opaque),
            (0x200, List::Opaque),
            (0xd, List::Floor),
            (0x101, List::WithSprites),
            (1, List::Translucent),
            (0x21, List::Translucent),
            (5, List::Effect),
            (0x25, List::Effect),
            (0x203, List::TopAdd),
            (0x201, List::Top),
        ] {
            assert_eq!(List::of(flags), list, "{flags:#x}");
        }
        assert!(!opaque_keeps(206.0 / 255.0) && opaque_keeps(207.0 / 255.0));
        let face =
            |flags: u32, z: f32| Face { texture: Tex::Path("a"), vertices: triangles([[0.0, 0.0, z]; 4], [[0.0; 2]; 4], [1.0; 4]), additive: false, flags };
        let faces = vec![face(5, 1.0), face(1, 1.0), face(5, 3.0), face(1, 3.0), face(5, 2.0), face(1, 2.0), face(0xd, 0.0)];
        let lists = Lists::sort(faces, |point| point[2]);
        let depths = |list: &[Face]| list.iter().map(|face| face.vertices[0].position[2]).collect::<Vec<_>>();
        // Insertion order for flags 5, far to near for 1.
        assert_eq!(depths(&lists.effect), [1.0, 3.0, 2.0]);
        assert_eq!(depths(&lists.translucent), [3.0, 2.0, 1.0]);
        assert_eq!(lists.floor.len(), 1);
    }

    fn key(frame: i32, position: [f32; 2], corners: [f32; 8]) -> Key {
        Key {
            frame,
            kind: 0,
            position,
            corners,
            texture: 0.0,
            texture_motion: 0,
            delay: 0.0,
            angle: 0.0,
            color: [1.0; 4],
            source_blend: 5,
            destination_blend: 2,
        }
    }

    /// An STR with layer 0 and one layer 100 × 50 px, centred 20 px right and 10 px above (320, 240).
    fn one_layer() -> Rc<Loaded> {
        let corners = [-50.0, 50.0, 50.0, -50.0, -25.0, -25.0, 25.0, 25.0];
        let layer = || Layer { textures: vec!["a.bmp".into()], keys: vec![key(0, [340.0, 230.0], corners), key(10, [340.0, 230.0], corners)] };
        Rc::new(Loaded { animation: Str { fps: 60, frames: 10, layers: vec![layer(), layer()] }, textures: vec![Vec::new(); 2] })
    }

    fn part(world: bool) -> StrPart {
        let mut fake = Fake::default();
        fake.strs.insert("a\\b".into(), one_layer());
        let mut part = StrPart::new("a\\b", false, &mut fake);
        part.world = world;
        part.step();
        part
    }

    #[test]
    fn str_parts() {
        assert_eq!(min_name("a\\b\\name.str"), "a\\b\\min_name.str");
        assert_eq!(min_name("name.str"), "min_name.str");
        // A missing file ends the part at once.
        let mut missing = StrPart::new("nothing", false, &mut Fake::default());
        assert!(!missing.step());
        // The floor flag routes its layers to the floor list.
        let mut floor = part(false);
        floor.flags = FLAGS_FLOOR;
        let mut faces = Vec::new();
        floor.faces(&Eye::at(45.0, 0.0), &mut faces);
        assert_eq!(faces.len(), 1);
        assert_eq!(Lists::sort(faces, |_| 0.0).floor.len(), 1);
    }

    #[test]
    fn view_offsets_look_the_same_at_any_latitude() {
        for latitude in [30.0, 60.0] {
            let eye = Eye::at(latitude, 20.0);
            let moved = sub(view_offset([1.0, 2.0, 3.0], 3.0, 4.0, &eye), [1.0, 2.0, 3.0]);
            assert!(close(dot(moved, eye.right), 3.0) && close(dot(moved, eye.up), 4.0) && close(dot(moved, eye.forward), 0.0));
        }
    }

    #[test]
    fn aim_turns() {
        for (to, degrees) in [([10.0, 0.0], 0.0), ([0.0, 10.0], 90.0), ([-10.0, 0.0], 180.0), ([0.0, -10.0], 270.0)] {
            assert!(close(aim_turn([0.0; 2], to), degrees), "{to:?}");
        }
        let (sin, cos) = 30f32.to_radians().sin_cos();
        assert!(close(aim_turn([5.0, 5.0], [5.0 + cos, 5.0 + sin]), 30.0));
    }

    #[test]
    fn the_3d_str_draw() {
        let eye = Eye::at(45.0, 0.0);
        let mut part = part(true);
        part.point = [10.0, 0.0, 10.0];
        part.controls.scale = 0.12;
        let mut faces = Vec::new();
        part.faces(&eye, &mut faces);
        let face = &faces[0];
        assert_eq!(face.flags, FLAGS_EFFECT);
        let local = |corner: usize| {
            let moved = sub(face.vertices[corner].position, [10.0, 0.0, 10.0]);
            [dot(moved, eye.right), dot(moved, eye.up)]
        };
        // Vertices 0, 1, 2 are v0, v1, v2; vertex 5 is v3.
        let [v0, v1, v2, v3] = [local(0), local(1), local(2), local(5)];
        assert!(close(v1[0] - v0[0], 12.0) && close(v0[1] - v2[1], 6.0));
        assert!(close((v0[0] + v3[0]) / 2.0, 2.4) && close((v0[1] + v3[1]) / 2.0, 1.2));
        // A mirror in x flips the offset; the alpha multiplier halves the alpha.
        part.controls.mirror_x = true;
        part.controls.alpha = 0.5;
        let mut faces = Vec::new();
        part.faces(&eye, &mut faces);
        let moved = sub(faces[0].vertices[0].position, [10.0, 0.0, 10.0]);
        let moved_v3 = sub(faces[0].vertices[5].position, [10.0, 0.0, 10.0]);
        assert!(close((dot(moved, eye.right) + dot(moved_v3, eye.right)) / 2.0, -2.4));
        assert!(close(faces[0].vertices[0].tint[3], 127.0 / 255.0));
    }

    #[test]
    fn the_ground_str_draw() {
        let eye = Eye::at(45.0, 0.0);
        let mut part = part(true);
        let at = [10.0, 0.0, 10.0];
        part.point = at;
        part.controls.scale = 0.12;
        let slope = |x: f32, _: f32| x * 0.5;
        let mut faces = Vec::new();
        part.flat_faces(&eye, &slope, &mut faces);
        let face = &faces[0];
        assert_eq!(face.flags, FLAGS_EFFECT);
        // Every corner is the ground's height + 2.
        for vertex in &face.vertices {
            assert!(close(vertex.position[1], vertex.position[0] * 0.5 + 2.0));
        }
        let (right, forward) = (eye.ground_right(), eye.ground_forward());
        let local = |faces: &[Face], corner: usize| {
            let moved = sub(faces[0].vertices[corner].position, at);
            [dot(moved, right), dot(moved, forward)]
        };
        // Vertices 0, 1, 2 are v0, v1, v2; vertex 5 is v3: a 12 × 6 layer, 2.4 right and 1.2 along the view from P.
        let [v0, v1, v2, v3] = [local(&faces, 0), local(&faces, 1), local(&faces, 2), local(&faces, 5)];
        assert!(close(v1[0] - v0[0], 12.0) && close((v0[1] - v2[1]).abs(), 6.0));
        assert!(close((v0[0] + v3[0]) / 2.0, 2.4) && close((v0[1] + v3[1]) / 2.0, 1.2));
        // A turn of 90 degrees turns it about the vertical: the centre goes from (2.4, 1.2) to (1.2, -2.4).
        part.controls.turn = 90.0;
        let mut turned = Vec::new();
        part.flat_faces(&eye, &slope, &mut turned);
        let [v0, v3] = [local(&turned, 0), local(&turned, 5)];
        assert!(close((v0[0] + v3[0]) / 2.0, 1.2) && close((v0[1] + v3[1]) / 2.0, -2.4));
    }
}
