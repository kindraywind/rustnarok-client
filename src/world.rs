//! The 3D world: GND ground (textures, lightmaps, map light), RSM models, sprite billboards and the camera.
//!
//! World space: X east, Y up, Z north; one GND cube is `gnd.zoom` (10) units, one GAT cell half of that.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io;
use std::ops::Range;

use image::{Rgba, RgbaImage};
use rustnarok_client::grf::Grf;
use rustnarok_client::map::{Gat, Gnd, Rsw};
use rustnarok_client::model::{self, Rsm};
use wgpu::util::DeviceExt;

use crate::render::{Gpu, Texture, WorldPass};

/// World units per GAT cell.
pub const CELL: f32 = 5.0;
// Camera and sprite scale as tuned against the official client (vFov 15, zoom 125 = 62.5 cells).
/// World units per sprite pixel (about 35 sprite pixels per cell).
const SPRITE_SCALE: f32 = CELL / 35.0;
/// The logical screen's height in pixels (`screen()` in main.rs): the field of view is vertical, so a screen pixel is
/// this fraction of the view's height at any aspect.
const SCREEN_HEIGHT: f32 = 768.0;
const FIELD_OF_VIEW: f32 = 15.0;
const NEAR: f32 = 10.0;
const FAR: f32 = 1500.0;
/// How far above the ground under it the eye stays.
const EYE_CLEARANCE: f32 = 2.0;

/// The eye's height, kept above the ground under it.
pub fn above_ground(eye: f32, ground: f32) -> f32 {
    eye.max(ground + EYE_CLEARANCE)
}
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// The projection puts view depth z in the depth buffer as `DEPTH_SCALE · (1 − NEAR / z)`.
const DEPTH_SCALE: f32 = FAR / (FAR - NEAR);

pub type Vector = [f32; 3];
/// Column-major, as WGSL reads it.
type Matrix = [[f32; 4]; 4];

fn subtract(a: Vector, b: Vector) -> Vector {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: Vector, b: Vector) -> Vector {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn dot(a: Vector, b: Vector) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn normalize(a: Vector) -> Vector {
    let length = dot(a, a).sqrt().max(f32::EPSILON);
    [a[0] / length, a[1] / length, a[2] / length]
}

fn multiply(a: &Matrix, b: &Matrix) -> Matrix {
    let mut result = [[0.0; 4]; 4];
    for column in 0..4 {
        for row in 0..4 {
            result[column][row] = (0..4).map(|k| a[k][row] * b[column][k]).sum();
        }
    }
    result
}

pub struct Camera {
    pub target: Vector,
    pub distance: f32,
    /// Radians above the horizon.
    pub pitch: f32,
    /// Radians; 0 looks north.
    pub yaw: f32,
    /// The ground's height under the eye (f32::MIN: not known).
    pub ground: f32,
}

/// The cell where a ray from `eye` first meets the ground (each cell's corner heights blended), up to the far plane.
/// None past the map's edge.
// ponytail: steps of a quarter cell, then halving; exact per-cell tests if cliffs ever pick wrong.
fn ground_hit(gat: &Gat, eye: Vector, direction: Vector) -> Option<[i32; 2]> {
    let direction = normalize(direction);
    let at = |t: f32| [0, 1, 2].map(|axis| eye[axis] + direction[axis] * t);
    let under = |point: Vector| point[1] < -gat.height_at([point[0] / CELL, point[2] / CELL]);
    let step = CELL / 4.0;
    let mut t = 0.0;
    while t < FAR {
        if under(at(t + step)) {
            let (mut above, mut below) = (t, t + step);
            for _ in 0..12 {
                let middle = (above + below) / 2.0;
                if under(at(middle)) {
                    below = middle;
                } else {
                    above = middle;
                }
            }
            let hit = at(below);
            let cell = [(hit[0] / CELL).floor() as i32, (hit[2] / CELL).floor() as i32];
            return (cell[0] >= 0 && cell[1] >= 0 && (cell[0] as usize) < gat.width && (cell[1] as usize) < gat.height).then_some(cell);
        }
        t += step;
    }
    None
}

/// A frame's camera: view-projection plus the basis billboards and picking need.
pub struct View {
    pub view_projection: Matrix,
    pub eye: Vector,
    pub forward: Vector,
    pub right: Vector,
    pub up: Vector,
    pub aspect: f32,
    /// The camera's distance and pitch (radians), for the sprites' depth.
    pub distance: f32,
    pub pitch: f32,
}

impl Camera {
    pub fn view(&self, aspect: f32) -> View {
        let horizontal = self.distance * self.pitch.cos();
        let eye = [
            self.target[0] - horizontal * self.yaw.sin(),
            above_ground(self.target[1] + self.distance * self.pitch.sin(), self.ground),
            self.target[2] - horizontal * self.yaw.cos(),
        ];
        // Left-handed: right = up x forward, depth along forward.
        let forward = normalize(subtract(self.target, eye));
        let right = normalize(cross([0.0, 1.0, 0.0], forward));
        let up = cross(forward, right);
        let view = [
            [right[0], up[0], forward[0], 0.0],
            [right[1], up[1], forward[1], 0.0],
            [right[2], up[2], forward[2], 0.0],
            [-dot(right, eye), -dot(up, eye), -dot(forward, eye), 1.0],
        ];
        let focal = 1.0 / (FIELD_OF_VIEW.to_radians() / 2.0).tan();
        let projection = [[focal / aspect, 0.0, 0.0, 0.0], [0.0, focal, 0.0, 0.0], [0.0, 0.0, DEPTH_SCALE, 1.0], [0.0, 0.0, -NEAR * DEPTH_SCALE, 0.0]];
        View { view_projection: multiply(&projection, &view), eye, forward, right, up, aspect, distance: self.distance, pitch: self.pitch }
    }
}

impl View {
    /// The ray from the eye through a point on screen (`0..1` each axis, y down), one unit of view depth long.
    fn ray(&self, screen: [f32; 2]) -> Vector {
        let tangent = (FIELD_OF_VIEW.to_radians() / 2.0).tan();
        let x = (screen[0] * 2.0 - 1.0) * tangent * self.aspect;
        let y = (1.0 - screen[1] * 2.0) * tangent;
        [0, 1, 2].map(|axis| self.forward[axis] + self.right[axis] * x + self.up[axis] * y)
    }

    /// The cell drawn under a point on screen (`0..1` each axis, y down); see `ground_hit`.
    pub fn pick_ground(&self, screen: [f32; 2], gat: &Gat) -> Option<[i32; 2]> {
        ground_hit(gat, self.eye, self.ray(screen))
    }

    /// Screen rectangle (`0..1`, y down: left, top, right, bottom) of a sprite-pixel box anchored at `feet`.
    pub fn sprite_rect(&self, feet: Vector, top_left: [f32; 2], bottom_right: [f32; 2]) -> Option<[f32; 4]> {
        let (a, b) = (self.sprite_point(feet, top_left)?, self.sprite_point(feet, bottom_right)?);
        Some([a[0].min(b[0]), a[1].min(b[1]), a[0].max(b[0]), a[1].max(b[1])])
    }

    /// Screen position (`0..1`, y down) of a point `offset` sprite pixels from `feet`.
    fn sprite_point(&self, feet: Vector, [x, y]: [f32; 2]) -> Option<[f32; 2]> {
        self.project([0, 1, 2].map(|axis| feet[axis] + self.right[axis] * x * SPRITE_SCALE - self.up[axis] * y * SPRITE_SCALE))
    }

    /// How far in front of the eye a point is, along the view.
    fn depth_of(&self, point: Vector) -> f32 {
        dot(subtract(point, self.eye), self.forward)
    }

    /// Screen position (`0..1`, y down) of a world point, if it's in front of the camera.
    pub fn project(&self, point: Vector) -> Option<[f32; 2]> {
        let m = &self.view_projection;
        let clip = [0, 1, 2, 3].map(|row| m[0][row] * point[0] + m[1][row] * point[1] + m[2][row] * point[2] + m[3][row]);
        (clip[3] > 0.0).then(|| [(clip[0] / clip[3] + 1.0) / 2.0, (1.0 - clip[1] / clip[3]) / 2.0])
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GroundVertex {
    position: [f32; 3],
    normal: [f32; 3],
    uv: [f32; 2],
    lightmap_uv: [f32; 2],
    color: [f32; 4],
}

const SPRITE_ATTRIBUTES: [wgpu::VertexAttribute; 6] =
    wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32x2, 3 => Float32x4, 4 => Float32x2, 5 => Float32x3];
const MESH_ATTRIBUTES: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32x4];

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct SpriteVertex {
    feet: [f32; 3],
    offset: [f32; 2],
    uv: [f32; 2],
    tint: [f32; 4],
    /// Screen pixels to move the corner by, after projecting.
    pixel: [f32; 2],
    /// The sprite's depth plane (see `plane`).
    plane: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Globals {
    view_projection: Matrix,
    light_direction: [f32; 4],
    ambient: [f32; 4],
    diffuse: [f32; 4],
    camera_right: [f32; 4],
    /// w: world units per sprite pixel.
    camera_up: [f32; 4],
    /// One screen pixel's width and height in clip space; z and w: `DEPTH_SCALE` and `NEAR`.
    pixel: [f32; 4],
}

const SHADER: &str = r"
struct Globals {
    view_projection: mat4x4<f32>,
    light_direction: vec4<f32>,
    ambient: vec4<f32>,
    diffuse: vec4<f32>,
    camera_right: vec4<f32>,
    camera_up: vec4<f32>,
    pixel: vec4<f32>,
};
@group(0) @binding(0) var<uniform> globals: Globals;
@group(1) @binding(0) var image: texture_2d<f32>;
@group(1) @binding(1) var image_sampler: sampler;
@group(2) @binding(0) var lightmap: texture_2d<f32>;
@group(2) @binding(1) var lightmap_sampler: sampler;

struct GroundOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) lightmap_uv: vec2<f32>,
    @location(2) normal: vec3<f32>,
    @location(3) color: vec4<f32>,
};

@vertex fn ground_vs(@location(0) position: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) uv: vec2<f32>,
                     @location(3) lightmap_uv: vec2<f32>, @location(4) color: vec4<f32>) -> GroundOutput {
    var out: GroundOutput;
    out.position = globals.view_projection * vec4(position, 1.0);
    out.uv = uv;
    out.lightmap_uv = lightmap_uv;
    out.normal = normal;
    out.color = color;
    return out;
}

@fragment fn ground_fs(in: GroundOutput) -> @location(0) vec4<f32> {
    let texel = textureSample(image, image_sampler, in.uv);
    let light_map = textureSample(lightmap, lightmap_sampler, in.lightmap_uv);
    if (texel.a < 0.5) { discard; }
    let weight = max(dot(normalize(in.normal), globals.light_direction.xyz), 0.0);
    let light = clamp(globals.ambient.rgb + globals.diffuse.rgb * weight, vec3(0.0), vec3(1.0));
    return vec4(texel.rgb * in.color.rgb * light * light_map.a + light_map.rgb, 1.0);
}

@fragment fn water_fs(in: GroundOutput) -> @location(0) vec4<f32> {
    let texel = textureSample(image, image_sampler, in.uv);
    return vec4(texel.rgb, 0.6);
}

@fragment fn model_fs(in: GroundOutput) -> @location(0) vec4<f32> {
    let texel = textureSample(image, image_sampler, in.uv);
    if (texel.a < 0.5) { discard; }
    // Faces are drawn from both sides, so light them from whichever side faces the light.
    let weight = abs(dot(normalize(in.normal), globals.light_direction.xyz));
    let light = clamp(globals.ambient.rgb + globals.diffuse.rgb * weight, vec3(0.0), vec3(1.0));
    return vec4(texel.rgb * in.color.rgb * light, 1.0);
}

// Shadows and effects take their feet's `depth`, flat. Sprites work theirs out per pixel from their `plane` (the
// feet's view depth and screen y, depth gained per screen pixel down, bias) and `nudge`: worked out per corner, depth
// differed between layers by rounding, so a headgear or head could fall behind the layer under it in some frames.
struct SpriteOutput { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32>, @location(1) tint: vec4<f32>,
                      @location(2) @interpolate(flat) depth: f32, @location(3) @interpolate(flat) plane: vec4<f32>,
                      @location(4) @interpolate(flat) nudge: f32, @location(5) @interpolate(linear) screen_y: f32 };
struct SpriteFragment { @location(0) color: vec4<f32>, @builtin(frag_depth) depth: f32 };

@vertex fn sprite_vs(@location(0) feet: vec3<f32>, @location(1) offset: vec2<f32>, @location(2) uv: vec2<f32>,
                     @location(3) tint: vec4<f32>, @location(4) pixel: vec2<f32>, @location(5) plane: vec3<f32>) -> SpriteOutput {
    var out = sprite_vertex(feet, offset, uv, tint, pixel);
    let feet_clip = globals.view_projection * vec4(feet, 1.0);
    out.plane = vec4(feet_clip.w, feet_clip.y / feet_clip.w, plane.x, plane.y);
    out.nudge = plane.z;
    out.screen_y = out.position.y / out.position.w;
    return out;
}

fn sprite_vertex(feet: vec3<f32>, offset: vec2<f32>, uv: vec2<f32>, tint: vec4<f32>, pixel: vec2<f32>) -> SpriteOutput {
    let scale = globals.camera_up.w;
    let world = feet + globals.camera_right.xyz * offset.x * scale - globals.camera_up.xyz * offset.y * scale;
    var clip = globals.view_projection * vec4(world, 1.0);
    clip.x += pixel.x * globals.pixel.x * clip.w;
    clip.y -= pixel.y * globals.pixel.y * clip.w;
    // The whole sprite takes its feet's depth, pulled slightly forward so it stands on the ground.
    let feet_clip = globals.view_projection * vec4(feet, 1.0);
    let depth = feet_clip.z / feet_clip.w - 0.0005;
    clip.z = depth * clip.w;
    var out: SpriteOutput;
    out.position = clip;
    out.uv = uv;
    out.tint = tint;
    out.depth = depth;
    return out;
}

@fragment fn sprite_fs(in: SpriteOutput) -> SpriteFragment {
    let texel = textureSample(image, image_sampler, in.uv);
    // Shadows and RGBA sprite images are translucent; palette sprites are fully opaque or clear.
    if (texel.a < 0.02) { discard; }
    return SpriteFragment(texel * in.tint, in.depth);
}

// A sprite pixel's depth, as `sprite_depth` works it out.
fn tilted_depth(in: SpriteOutput) -> f32 {
    let below = (in.plane.y - in.screen_y) / globals.pixel.y;
    let z = max(in.plane.x + in.plane.z * below, globals.pixel.w);
    return clamp(globals.pixel.z * (1.0 - globals.pixel.w * (1.0 / z + in.plane.w)) - in.nudge, 0.0, 1.0);
}

// Sprites draw twice: their solid pixels (alpha 159 and up) writing depth, then their soft ones blending over without.
const SOLID: f32 = 159.0 / 255.0;

@fragment fn solid_fs(in: SpriteOutput) -> SpriteFragment {
    let color = textureSample(image, image_sampler, in.uv) * in.tint;
    if (color.a < SOLID) { discard; }
    return SpriteFragment(color, tilted_depth(in));
}

@fragment fn soft_fs(in: SpriteOutput) -> SpriteFragment {
    let color = textureSample(image, image_sampler, in.uv) * in.tint;
    if (color.a < 0.02 || color.a >= SOLID) { discard; }
    return SpriteFragment(color, tilted_depth(in));
}

// Effect shapes placed in the world (cylinders): texture repeats across, colored and faded by the vertex.
struct MeshOutput { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32>, @location(1) tint: vec4<f32> };

@vertex fn mesh_vs(@location(0) position: vec3<f32>, @location(1) uv: vec2<f32>, @location(2) tint: vec4<f32>) -> MeshOutput {
    var out: MeshOutput;
    out.position = globals.view_projection * vec4(position, 1.0);
    out.uv = uv;
    out.tint = tint;
    return out;
}

@fragment fn mesh_fs(in: MeshOutput) -> @location(0) vec4<f32> {
    let texel = textureSample(image, image_sampler, vec2(fract(in.uv.x), in.uv.y));
    return texel * in.tint;
}

// Effects stand in front of what they play on, pulled toward the camera.
@vertex fn effect_vs(@location(0) feet: vec3<f32>, @location(1) offset: vec2<f32>, @location(2) uv: vec2<f32>,
                     @location(3) tint: vec4<f32>) -> SpriteOutput {
    var out = sprite_vertex(feet, offset, uv, tint, vec2(0.0));
    out.position.z = max(out.position.z - 0.01 * out.position.w, 0.0);
    out.depth = max(out.depth - 0.01, 0.0);
    return out;
}
";

/// A sprite corner (`uv` 0 or 1 each way): its offset from the feet in sprite pixels, and how many screen pixels it
/// moves after projecting. The image turns clockwise on screen about its middle; its right and bottom edges reach one
/// screen pixel further, and the whole quad sits half a pixel right and down of the ground under it.
pub fn sprite_corner([x, y]: [f32; 2], [w, h]: [f32; 2], angle: f32, [u, v]: [f32; 2]) -> ([f32; 2], [f32; 2]) {
    let (sin, cos) = angle.to_radians().sin_cos();
    let turn = |[dx, dy]: [f32; 2]| [dx * cos - dy * sin, dx * sin + dy * cos];
    let [dx, dy] = turn([(u - 0.5) * w, (v - 0.5) * h]);
    let [px, py] = turn([u - 0.5, v - 0.5]);
    ([x + w / 2.0 + dx, y + h / 2.0 + dy], [1.0 + px, 1.0 + py])
}

/// A sprite image in the world: texture, feet position, top-left offset from the feet and size in sprite pixels
/// (negative width = mirrored), a color to multiply it by (damage colors, fading), degrees to turn it clockwise
/// about its middle, and its depth layer.
pub type Billboard<'a> = (&'a Texture, Vector, [f32; 2], [f32; 2], [f32; 4], f32, DepthLayer);

/// Where a sprite image stands in depth. It takes a tilted depth from its feet, its top nearer the camera (farther
/// for one lying `flat`: the dead), biased nearer the more images of its sprite are under it (`index`); `nudge` steps
/// it nearer still without leaving its plane, as a player's head and gear do on the body's.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DepthLayer {
    pub index: f32,
    pub nudge: f32,
    pub flat: bool,
}

/// A layer's depth plane, for feet at view depth `feet`: depth gained per screen pixel down it, bias nearer (in
/// 1/depth: about 1.2 cells at the feet with the default camera) and nudge nearer (in the depth buffer).
fn plane(layer: DepthLayer, feet: f32, pitch: f32, distance: f32) -> [f32; 3] {
    let per_pixel = 0.16 * 730.0 / SCREEN_HEIGHT;
    let slope = if layer.flat { -per_pixel / pitch.tan() } else { per_pixel * pitch.tan() };
    let near = 300.0 / distance;
    let per_layer = 9e-6 * near * pitch.sin() / std::f32::consts::FRAC_PI_4.sin();
    // ponytail: the unit's perspective scale as the camera's distance over its feet's depth.
    let scale = distance / feet.max(NEAR);
    [slope, (3.0 + layer.index) * per_layer + scale * 3.6e-5 * near, layer.nudge * 1e-6]
}

/// The depth buffer's value for a sprite pixel `below` screen pixels down from its feet at view depth `feet`.
fn sprite_depth(feet: f32, below: f32, [slope, bias, nudge]: [f32; 3]) -> f32 {
    let z = (feet + slope * below).max(NEAR);
    (DEPTH_SCALE * (1.0 - NEAR * (1.0 / z + bias)) - nudge).clamp(0.0, 1.0)
}

/// An effect's quad: texture, where it plays, its corners' offsets in sprite pixels (top left, top right, bottom right,
/// bottom left; y down), a color to multiply it by, and whether it adds to what's behind instead of covering it.
pub type EffectQuad<'a> = (&'a Texture, Vector, [[f32; 2]; 4], [f32; 4], bool);

/// An effect shape in the world: texture, triangles (position, texture coordinates, color) and whether it adds on.
pub type EffectMesh<'a> = (&'a Texture, Vec<MeshVertex>, bool);

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MeshVertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
    pub tint: [f32; 4],
}

pub struct World {
    pub gat: Gat,
    /// One RGB light per GAT cell, from the ground lightmaps.
    cell_light: Vec<[f32; 3]>,
    /// The ground tiles' corner heights (south-west, south-east, north-west, north-east; up is +), and tiles a row.
    tile_heights: (Vec<[f32; 4]>, usize),
    /// RSM 2 instances whose tracks play, and the buffer they're posed into each frame.
    animated: Vec<Animated>,
    animated_buffer: Option<wgpu::Buffer>,
    /// How many RSM 2 model instances stand on the map.
    pub rsm_2_models: usize,
    /// The map's ambient sound sources.
    pub ambient: crate::ambient::Ambient,
    textures: Vec<Texture>,
    lightmap: Texture,
    vertices: wgpu::Buffer,
    /// Ground triangles per texture: texture index, vertex range.
    draws: Vec<(usize, Range<u32>)>,
    model_textures: Vec<Texture>,
    model_vertices: Option<wgpu::Buffer>,
    model_draws: Vec<(usize, Range<u32>)>,
    /// Water plane and its animation frames, with frames per texture change (at 60 per second).
    water: Option<(wgpu::Buffer, Vec<Texture>, u32)>,
    light: (Vector, [f32; 3], [f32; 3]),
    globals: wgpu::Buffer,
    globals_bind_group: wgpu::BindGroup,
    ground_pipeline: wgpu::RenderPipeline,
    model_pipeline: wgpu::RenderPipeline,
    water_pipeline: wgpu::RenderPipeline,
    sprite_pipeline: wgpu::RenderPipeline,
    /// Shadows: drawn like sprites, but without writing depth.
    shadow_pipeline: wgpu::RenderPipeline,
    /// Sprites' soft pixels: blended over without writing depth.
    soft_pipeline: wgpu::RenderPipeline,
    /// Effects blended over, and added on; the same for effect shapes.
    effect_pipelines: [wgpu::RenderPipeline; 2],
    mesh_pipelines: [wgpu::RenderPipeline; 2],
    depth: RefCell<Option<([u32; 2], wgpu::TextureView)>>,
}

impl World {
    pub fn load(gpu: &Gpu, grf: &Grf, map: &str) -> io::Result<Self> {
        let map = map.trim_end_matches(".gat").trim_end_matches(".rsw");
        let rsw = Rsw::parse(&grf.read(&format!("data\\{map}.rsw"))?)?;
        let gnd = Gnd::parse(&grf.read(&format!("data\\{}", rsw.ground_file))?)?;
        let gat = Gat::parse(&grf.read(&format!("data\\{}", rsw.altitude_file))?)?;

        let clamp = |filter| gpu.device.create_sampler(&wgpu::SamplerDescriptor { mag_filter: filter, min_filter: filter, ..Default::default() });
        let smooth = clamp(wgpu::FilterMode::Linear);
        let textures = gnd
            .textures
            .iter()
            .map(|name| {
                let image = grf
                    .read(&format!("data\\texture\\{name}"))
                    .ok()
                    .and_then(|bytes| image::load_from_memory(&bytes).ok())
                    .map(|image| image.to_rgba8())
                    .unwrap_or_else(|| RgbaImage::from_pixel(1, 1, Rgba([128, 128, 128, 255])));
                gpu.texture_with_sampler(&key_out_magenta(image), &smooth)
            })
            .collect();
        let (atlas, columns) = lightmap_atlas(&gnd);
        let lightmap = gpu.texture_with_sampler(&atlas, &smooth);

        let (vertices, draws) = ground_mesh(&gnd, columns, [atlas.width() as f32, atlas.height() as f32]);
        let vertices = gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ground"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let (model_vertices, model_draws, model_texture_names, rsm_2_models, mut animated) = model_mesh(grf, &rsw, &gnd);
        // The moving models' buffer: each instance's triangles in their order, its draws by texture.
        let mut moving = 0u32;
        for instance in &mut animated {
            instance.draws = std::mem::take(&mut instance.runs)
                .into_iter()
                .map(|(texture, count)| {
                    let range = moving..moving + count;
                    moving += count;
                    (texture, range)
                })
                .collect();
        }
        let animated_buffer = (moving > 0).then(|| {
            gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("moving models"),
                size: u64::from(moving) * std::mem::size_of::<GroundVertex>() as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        });
        let model_textures = model_texture_names
            .iter()
            .map(|name| {
                let image = grf
                    .read(&format!("data\\texture\\{name}"))
                    .ok()
                    .and_then(|bytes| image::load_from_memory(&bytes).ok())
                    .map(|image| image.to_rgba8())
                    .unwrap_or_else(|| RgbaImage::from_pixel(1, 1, Rgba([128, 128, 128, 255])));
                gpu.texture_with_sampler(&key_out_magenta(image), &smooth)
            })
            .collect();
        let model_vertices = (!model_vertices.is_empty()).then(|| {
            gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("models"),
                contents: bytemuck::cast_slice(&model_vertices),
                usage: wgpu::BufferUsages::VERTEX,
            })
        });

        // RSW light angles: longitude around the vertical axis, latitude from the zenith.
        let (longitude, latitude) = ((rsw.light.longitude as f32).to_radians(), (rsw.light.latitude as f32).to_radians());
        let direction = normalize([-longitude.cos() * latitude.sin(), latitude.cos(), -longitude.sin() * latitude.sin()]);

        let globals = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("world globals"),
            size: size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let globals_layout = gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            }],
        });
        let globals_bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &globals_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: globals.as_entire_binding() }],
        });

        let shader = gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("world"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
        let layout = gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&globals_layout), Some(&gpu.texture_layout), Some(&gpu.texture_layout)],
            immediate_size: 0,
        });
        let pipeline =
            |entry: &str, fragment: &str, buffer: wgpu::VertexBufferLayout, blend: Option<wgpu::BlendState>, depth_write: bool| -> wgpu::RenderPipeline {
                gpu.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(entry),
                    layout: Some(&layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some(&format!("{entry}_vs")),
                        compilation_options: Default::default(),
                        buffers: &[buffer],
                    },
                    primitive: Default::default(),
                    depth_stencil: Some(wgpu::DepthStencilState {
                        format: DEPTH_FORMAT,
                        depth_write_enabled: Some(depth_write),
                        depth_compare: Some(wgpu::CompareFunction::LessEqual),
                        stencil: Default::default(),
                        bias: Default::default(),
                    }),
                    multisample: Default::default(),
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some(fragment),
                        compilation_options: Default::default(),
                        targets: &[Some(wgpu::ColorTargetState { format: gpu.config.format, blend, write_mask: wgpu::ColorWrites::ALL })],
                    }),
                    multiview_mask: None,
                    cache: None,
                })
            };
        const GROUND_ATTRIBUTES: [wgpu::VertexAttribute; 5] =
            wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32x2, 4 => Float32x4];
        let ground_layout = || wgpu::VertexBufferLayout {
            array_stride: size_of::<GroundVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &GROUND_ATTRIBUTES,
        };
        let ground_pipeline = pipeline("ground", "ground_fs", ground_layout(), None, true);
        let model_pipeline = pipeline("ground", "model_fs", ground_layout(), None, true);
        let water_pipeline = pipeline("ground", "water_fs", ground_layout(), Some(wgpu::BlendState::ALPHA_BLENDING), false);
        let sprite_buffer_layout = || wgpu::VertexBufferLayout {
            array_stride: size_of::<SpriteVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &SPRITE_ATTRIBUTES,
        };
        let sprite_pipeline = pipeline("sprite", "solid_fs", sprite_buffer_layout(), Some(wgpu::BlendState::ALPHA_BLENDING), true);
        let soft_pipeline = pipeline("sprite", "soft_fs", sprite_buffer_layout(), Some(wgpu::BlendState::ALPHA_BLENDING), false);
        // A shadow and its sprite share a depth; rounding in the two quads' divides left the shadow nearer in places,
        // so a shadow writing depth cut holes in its own sprite. Shadows lie on what's under them and hide nothing.
        let shadow_pipeline = pipeline("sprite", "sprite_fs", sprite_buffer_layout(), Some(wgpu::BlendState::ALPHA_BLENDING), false);
        let additive = wgpu::BlendState {
            color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::SrcAlpha, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
            alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::Zero, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
        };
        let effect_pipelines = [
            pipeline("effect", "sprite_fs", sprite_buffer_layout(), Some(wgpu::BlendState::ALPHA_BLENDING), false),
            pipeline("effect", "sprite_fs", sprite_buffer_layout(), Some(additive), false),
        ];
        let mesh_buffer_layout =
            || wgpu::VertexBufferLayout { array_stride: size_of::<MeshVertex>() as u64, step_mode: wgpu::VertexStepMode::Vertex, attributes: &MESH_ATTRIBUTES };
        let mesh_pipelines = [
            pipeline("mesh", "mesh_fs", mesh_buffer_layout(), Some(wgpu::BlendState::ALPHA_BLENDING), false),
            pipeline("mesh", "mesh_fs", mesh_buffer_layout(), Some(additive), false),
        ];

        // ponytail: flat water; the client also waves it (wave height/speed/pitch from the RSW).
        let water = rsw.water.as_ref().map(|water| {
            let repeat = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
                address_mode_u: wgpu::AddressMode::Repeat,
                address_mode_v: wgpu::AddressMode::Repeat,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            });
            let frames = (0..32)
                .filter_map(|frame| {
                    let bytes = grf.read(&format!("data\\texture\\워터\\water{}{frame:02}.jpg", water.kind)).ok()?;
                    Some(gpu.texture_with_sampler(&image::load_from_memory(&bytes).ok()?.to_rgba8(), &repeat))
                })
                .collect();
            let (width, depth) = (gnd.width as f32 * gnd.zoom, gnd.height as f32 * gnd.zoom);
            // One texture repeat per two cubes.
            let tile = gnd.zoom * 2.0;
            let corner = |x: f32, z: f32| GroundVertex {
                position: [x, -water.level, z],
                normal: [0.0, 1.0, 0.0],
                uv: [x / tile, z / tile],
                lightmap_uv: [0.0; 2],
                color: [1.0; 4],
            };
            let quad = [corner(0.0, 0.0), corner(width, 0.0), corner(width, depth), corner(0.0, 0.0), corner(width, depth), corner(0.0, depth)];
            let buffer = gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("water"),
                contents: bytemuck::cast_slice(&quad),
                usage: wgpu::BufferUsages::VERTEX,
            });
            (buffer, frames, water.animation_speed.max(1))
        });

        // Sound sources sit, like models, relative to the map's center.
        let half = [gnd.width as f32 * gnd.zoom / 2.0, gnd.height as f32 * gnd.zoom / 2.0];
        let ambient = crate::ambient::Ambient::new(rsw.sounds.iter().map(|source| crate::ambient::Source::placed(source, half)).collect());
        let cell_light = cell_lights(&gnd, &gat);
        let tile_heights = (gnd.cubes.iter().map(|cube| cube.heights.map(|height| -height)).collect(), gnd.width);
        Ok(Self {
            animated,
            animated_buffer,
            rsm_2_models,
            ambient,
            gat,
            cell_light,
            tile_heights,
            textures,
            lightmap,
            vertices,
            draws,
            model_textures,
            model_vertices,
            model_draws,
            water,
            light: (direction, rsw.light.ambient, rsw.light.diffuse),
            globals,
            globals_bind_group,
            ground_pipeline,
            model_pipeline,
            water_pipeline,
            sprite_pipeline,
            shadow_pipeline,
            soft_pipeline,
            effect_pipelines,
            mesh_pipelines,
            depth: RefCell::new(None),
        })
    }

    /// The ground under a world point: the same x and z, at the ground's height there.
    pub fn ground_under(&self, point: Vector) -> Vector {
        [point[0], -self.gat.height_at([point[0] / CELL, point[2] / CELL]), point[2]]
    }

    /// World position of a (fractional) cell's center on the ground.
    pub fn cell_position(&self, cell: [f32; 2]) -> Vector {
        let height = self.gat.height_at([cell[0] + 0.5, cell[1] + 0.5]);
        [(cell[0] + 0.5) * CELL, -height, (cell[1] + 0.5) * CELL]
    }

    /// A quad over a GAT cell at the corner heights of the whole ground tile it's a quarter of (as the ground cursor
    /// draws it): corners south-west, south-east, north-west, north-east.
    pub fn cell_quad(&self, cell: [i32; 2]) -> Option<[Vector; 4]> {
        cell_quad(&self.tile_heights.0, self.tile_heights.1, cell)
    }

    /// The map light on a unit drawn at `feet`: that GAT cell's triple, or full bright off the map.
    pub fn sprite_light(&self, feet: Vector) -> [f32; 3] {
        light_at(&self.cell_light, self.gat.width, self.gat.height, feet)
    }
}

/// A GAT cell's quad at the corner heights of its ground tile (`tiles`, `wide` a row), or None off the map.
fn cell_quad(tiles: &[[f32; 4]], wide: usize, [x, y]: [i32; 2]) -> Option<[Vector; 4]> {
    let tile = usize::try_from(y / 2).ok()? * wide + usize::try_from(x / 2).ok().filter(|&column| column < wide)?;
    let [south_west, south_east, north_west, north_east] = *tiles.get(tile)?;
    let (x0, x1, z0, z1) = (x as f32 * CELL, (x + 1) as f32 * CELL, y as f32 * CELL, (y + 1) as f32 * CELL);
    (x >= 0 && y >= 0).then_some([[x0, south_west, z0], [x1, south_east, z0], [x0, north_west, z1], [x1, north_east, z1]])
}

/// The cell and its four edge neighbours. Light changes in a step at the cell border, with no blend.
const LIGHT_SAMPLES: [(i32, i32); 5] = [(0, 0), (0, -1), (0, 1), (1, 0), (-1, 0)];

/// One RGB triple per GAT cell. A cell more than 2.5 units above its GND tile stays full bright.
fn cell_lights(gnd: &Gnd, gat: &Gat) -> Vec<[f32; 3]> {
    let mut lights = Vec::with_capacity(gat.width.saturating_mul(gat.height));
    for y in 0..gat.height {
        for x in 0..gat.width {
            lights.push(if above_mesh(gnd, gat, x, y) { [1.0; 3] } else { sampled_light(gnd, x, y) });
        }
    }
    lights
}

/// The mean of the cell's four GAT corners (world Y, up) is more than 2.5 above the GND tile's highest corner.
fn above_mesh(gnd: &Gnd, gat: &Gat, x: usize, y: usize) -> bool {
    let (tile_x, tile_y) = (x / 2, y / 2);
    if tile_x >= gnd.width || tile_y >= gnd.height {
        return false;
    }
    let Some(cube) = gnd.cubes.get(tile_y * gnd.width + tile_x) else { return false };
    let Some(&(corners, _)) = gat.cells.get(y * gat.width + x) else { return false };
    // File heights are negative-up. The highest corner is the greatest world Y.
    let mesh = cube.heights.into_iter().map(|height| -height).fold(f32::NEG_INFINITY, f32::max);
    let cell = corners.into_iter().map(|height| -height).sum::<f32>() / 4.0;
    cell > mesh + 2.5
}

/// light_c = mean(shadow) / 255 + mean(channel) / 128, then divided by the largest channel when that is above 1.
fn sampled_light(gnd: &Gnd, x: usize, y: usize) -> [f32; 3] {
    let mut shadow = 0.0;
    let mut color = [0.0; 3];
    for (dx, dy) in LIGHT_SAMPLES {
        let [alpha, red, green, blue] = light_sample(gnd, x as i32 + dx, y as i32 + dy);
        shadow += f32::from(alpha);
        color[0] += f32::from(red);
        color[1] += f32::from(green);
        color[2] += f32::from(blue);
    }
    let count = LIGHT_SAMPLES.len() as f32;
    let mut light = color.map(|channel| shadow / count / 255.0 + channel / count / 128.0);
    let peak = light[0].max(light[1]).max(light[2]);
    if peak > 1.0 {
        light = light.map(|channel| channel / peak);
    }
    light
}

/// Shadow and RGB of one GAT cell's quarter of its GND tile's top lightmap. Off the map, or a tile with no top, is 255.
fn light_sample(gnd: &Gnd, x: i32, y: i32) -> [u8; 4] {
    const FULL: [u8; 4] = [255; 4];
    if x < 0 || y < 0 {
        return FULL;
    }
    let (x, y) = (x as usize, y as usize);
    let (tile_x, tile_y) = (x / 2, y / 2);
    if tile_x >= gnd.width || tile_y >= gnd.height {
        return FULL;
    }
    let Some(cube) = gnd.cubes.get(tile_y * gnd.width + tile_x) else { return FULL };
    if cube.top < 0 {
        return FULL;
    }
    let Some(surface) = usize::try_from(cube.top).ok().and_then(|index| gnd.surfaces.get(index)) else { return FULL };
    let Some(lightmap) = gnd.lightmaps.get(usize::from(surface.lightmap)) else { return FULL };
    let [width, height] = gnd.lightmap_size;
    // West-south (1, 1), east-south (5, 1), west-north (1, 5), east-north (5, 5), row 0 on the south edge.
    // ponytail: a lightmap that is not 8×8 has no such quarter, so the sample stays full bright.
    let column = if x % 2 == 0 { 1 } else { 5 };
    let row = if y % 2 == 0 { 1 } else { 5 };
    if column >= width || row >= height {
        return FULL;
    }
    let texel = row * width + column;
    let Some(&shadow) = lightmap.get(texel) else { return FULL };
    let color = width * height + texel * 3;
    let Some(rgb) = lightmap.get(color..color + 3) else { return FULL };
    [shadow, rgb[0], rgb[1], rgb[2]]
}

/// The GAT cell under a world point, or full bright off the table. No blending across the border.
fn light_at(table: &[[f32; 3]], width: usize, height: usize, [x, _, z]: Vector) -> [f32; 3] {
    let (cell_x, cell_z) = ((x / CELL).floor(), (z / CELL).floor());
    if cell_x < 0.0 || cell_z < 0.0 {
        return [1.0; 3];
    }
    let (cell_x, cell_z) = (cell_x as usize, cell_z as usize);
    if cell_x >= width || cell_z >= height {
        return [1.0; 3];
    }
    table.get(cell_z * width + cell_x).copied().unwrap_or([1.0; 3])
}

/// Everything one frame of the world needs.
pub struct WorldFrame<'a> {
    pub world: &'a World,
    pub view: &'a View,
    pub billboards: &'a [Billboard<'a>],
    /// How many of the billboards, first, are shadows.
    pub shadows: usize,
    pub effects: &'a [EffectQuad<'a>],
    pub meshes: &'a [EffectMesh<'a>],
    /// Seconds since the map was entered, for animated textures.
    pub time: f32,
}

impl WorldPass for WorldFrame<'_> {
    fn encode(&self, gpu: &Gpu, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView, size: [u32; 2]) {
        let world = self.world;
        let (direction, ambient, diffuse) = world.light;
        let globals = Globals {
            view_projection: self.view.view_projection,
            light_direction: [direction[0], direction[1], direction[2], 0.0],
            ambient: [ambient[0], ambient[1], ambient[2], 1.0],
            diffuse: [diffuse[0], diffuse[1], diffuse[2], 1.0],
            camera_right: [self.view.right[0], self.view.right[1], self.view.right[2], 0.0],
            camera_up: [self.view.up[0], self.view.up[1], self.view.up[2], SPRITE_SCALE],
            pixel: [2.0 / (SCREEN_HEIGHT * self.view.aspect), 2.0 / SCREEN_HEIGHT, DEPTH_SCALE, NEAR],
        };
        gpu.queue.write_buffer(&world.globals, 0, bytemuck::bytes_of(&globals));

        let mut depth = world.depth.borrow_mut();
        if depth.as_ref().is_none_or(|(depth_size, _)| *depth_size != size) {
            let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("depth"),
                size: wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            *depth = Some((size, texture.create_view(&Default::default())));
        }
        let depth_view = &depth.as_ref().unwrap().1;

        let view = self.view;
        let planes: Vec<[f32; 3]> = self.billboards.iter().map(|&(_, feet, .., layer)| plane(layer, view.depth_of(feet), view.pitch, view.distance)).collect();
        let sprite_vertices: Vec<SpriteVertex> = self
            .billboards
            .iter()
            .zip(&planes)
            .flat_map(|(&(_, feet, offset, size, tint, angle, _), &plane)| {
                let corner = |u: f32, v: f32| {
                    let (offset, pixel) = sprite_corner(offset, size, angle, [u, v]);
                    SpriteVertex { feet, offset, uv: [u, v], tint, pixel, plane }
                };
                [corner(0., 0.), corner(1., 0.), corner(0., 1.), corner(0., 1.), corner(1., 0.), corner(1., 1.)]
            })
            .collect();
        // Sprites back to front by the depth of their images' top edges.
        let top_depths: Vec<f32> = self
            .billboards
            .iter()
            .zip(&planes)
            .map(|(&(_, feet, offset, size, _, angle, _), &plane)| {
                let Some(base) = view.project(feet) else { return 1.0 };
                let corners = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]].map(|uv| view.sprite_point(feet, sprite_corner(offset, size, angle, uv).0));
                let top = corners.iter().flatten().map(|corner| corner[1]).reduce(f32::min).unwrap_or(base[1]);
                sprite_depth(view.depth_of(feet), (top - base[1]) * SCREEN_HEIGHT, plane)
            })
            .collect();
        let mut order: Vec<usize> = (self.shadows..self.billboards.len()).collect();
        order.sort_by(|&a, &b| top_depths[b].total_cmp(&top_depths[a]));
        let effect_vertices: Vec<SpriteVertex> = self
            .effects
            .iter()
            .flat_map(|&(_, feet, corners, tint, _)| {
                let corner = |index: usize, uv: [f32; 2]| SpriteVertex { feet, offset: corners[index], uv, tint, pixel: [0.0; 2], plane: [0.0; 3] };
                let [top_left, top_right, bottom_right, bottom_left] =
                    [corner(0, [0.0, 0.0]), corner(1, [1.0, 0.0]), corner(2, [1.0, 1.0]), corner(3, [0.0, 1.0])];
                [top_left, top_right, bottom_left, bottom_left, top_right, bottom_right]
            })
            .collect();
        let effect_buffer = (!effect_vertices.is_empty()).then(|| {
            gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("effects"),
                contents: bytemuck::cast_slice(&effect_vertices),
                usage: wgpu::BufferUsages::VERTEX,
            })
        });
        let mesh_vertices: Vec<MeshVertex> = self.meshes.iter().flat_map(|(_, vertices, _)| vertices.iter().copied()).collect();
        let mesh_buffer = (!mesh_vertices.is_empty()).then(|| {
            gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("effect shapes"),
                contents: bytemuck::cast_slice(&mesh_vertices),
                usage: wgpu::BufferUsages::VERTEX,
            })
        });
        let sprite_buffer = (!sprite_vertices.is_empty()).then(|| {
            gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("sprites"),
                contents: bytemuck::cast_slice(&sprite_vertices),
                usage: wgpu::BufferUsages::VERTEX,
            })
        });

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("world"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_view,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        let [x, y, width, height] = crate::render::viewport(size);
        pass.set_viewport(x, y, width, height, 0.0, 1.0);
        pass.set_bind_group(0, &world.globals_bind_group, &[]);
        pass.set_bind_group(2, world.lightmap.bind_group(), &[]);
        pass.set_pipeline(&world.ground_pipeline);
        pass.set_vertex_buffer(0, world.vertices.slice(..));
        for (texture, range) in &world.draws {
            pass.set_bind_group(1, world.textures[*texture].bind_group(), &[]);
            pass.draw(range.clone(), 0..1);
        }
        if let Some(buffer) = &world.model_vertices {
            pass.set_pipeline(&world.model_pipeline);
            pass.set_vertex_buffer(0, buffer.slice(..));
            for (texture, range) in &world.model_draws {
                pass.set_bind_group(1, world.model_textures[*texture].bind_group(), &[]);
                pass.draw(range.clone(), 0..1);
            }
        }
        if let Some(buffer) = &world.animated_buffer {
            pass.set_pipeline(&world.model_pipeline);
            pass.set_vertex_buffer(0, buffer.slice(..));
            for (texture, range) in world.animated.iter().flat_map(|instance| &instance.draws) {
                pass.set_bind_group(1, world.model_textures[*texture].bind_group(), &[]);
                pass.draw(range.clone(), 0..1);
            }
        }
        if let Some((buffer, frames, frames_per_change)) = &world.water
            && !frames.is_empty()
        {
            let frame = (self.time * 60.0 / *frames_per_change as f32) as usize % frames.len();
            pass.set_pipeline(&world.water_pipeline);
            pass.set_vertex_buffer(0, buffer.slice(..));
            pass.set_bind_group(1, frames[frame].bind_group(), &[]);
            pass.draw(0..6, 0..1);
        }
        if let Some(buffer) = &sprite_buffer {
            pass.set_vertex_buffer(0, buffer.slice(..));
            let draw = |pass: &mut wgpu::RenderPass, index: usize| {
                let first = index as u32 * 6;
                pass.set_bind_group(1, self.billboards[index].0.bind_group(), &[]);
                pass.draw(first..first + 6, 0..1);
            };
            pass.set_pipeline(&world.shadow_pipeline);
            (0..self.shadows).for_each(|index| draw(&mut pass, index));
            for pipeline in [&world.sprite_pipeline, &world.soft_pipeline] {
                pass.set_pipeline(pipeline);
                order.iter().for_each(|&index| draw(&mut pass, index));
            }
        }
        if let Some(buffer) = &mesh_buffer {
            pass.set_vertex_buffer(0, buffer.slice(..));
            let mut first = 0u32;
            for (texture, vertices, additive) in self.meshes {
                let count = vertices.len() as u32;
                pass.set_pipeline(&world.mesh_pipelines[usize::from(*additive)]);
                pass.set_bind_group(1, texture.bind_group(), &[]);
                pass.draw(first..first + count, 0..1);
                first += count;
            }
        }
        if let Some(buffer) = &effect_buffer {
            pass.set_vertex_buffer(0, buffer.slice(..));
            for (index, (texture, .., additive)) in self.effects.iter().enumerate() {
                let first = index as u32 * 6;
                pass.set_pipeline(&world.effect_pipelines[usize::from(*additive)]);
                pass.set_bind_group(1, texture.bind_group(), &[]);
                pass.draw(first..first + 6, 0..1);
            }
        }
    }
}

/// Map textures use the same magenta color key as UI bitmaps.
fn key_out_magenta(mut image: RgbaImage) -> RgbaImage {
    for pixel in image.pixels_mut() {
        let [r, g, b, _] = pixel.0;
        if r >= 0xFE && g <= 0x03 && b >= 0xFE {
            pixel.0 = [0; 4];
        }
    }
    image
}

/// Packs the 8x8 lightmaps into one texture (RGB light color, alpha shadow); returns it and its column count.
fn lightmap_atlas(gnd: &Gnd) -> (RgbaImage, usize) {
    let [width, height] = gnd.lightmap_size;
    let count = gnd.lightmaps.len().max(1);
    let columns = count.min(2048 / width.max(1));
    let rows = count.div_ceil(columns);
    let mut atlas = RgbaImage::from_pixel((columns * width) as u32, (rows * height) as u32, Rgba([0, 0, 0, 255]));
    for (index, lightmap) in gnd.lightmaps.iter().enumerate() {
        let (left, top) = ((index % columns) * width, (index / columns) * height);
        for y in 0..height {
            for x in 0..width {
                let shadow = lightmap[y * width + x];
                let color = &lightmap[width * height + (y * width + x) * 3..][..3];
                atlas.put_pixel((left + x) as u32, (top + y) as u32, Rgba([color[0], color[1], color[2], shadow]));
            }
        }
    }
    (atlas, columns)
}

/// Ground triangles grouped by texture: cube tops, plus north and east walls down to the neighbor.
fn ground_mesh(gnd: &Gnd, columns: usize, atlas_size: [f32; 2]) -> (Vec<GroundVertex>, Vec<(usize, Range<u32>)>) {
    let [lightmap_width, lightmap_height] = gnd.lightmap_size.map(|size| size as f32);
    let mut per_texture: Vec<Vec<GroundVertex>> = vec![Vec::new(); gnd.textures.len()];
    let zoom = gnd.zoom;

    // Corners in surface order (u[0..4]/v[0..4]): the lightmap's inner 6x6 pixels map onto the quad.
    let mut push_quad = |surface_index: i32, corners: [Vector; 4]| {
        let Some(surface) = usize::try_from(surface_index).ok().and_then(|index| gnd.surfaces.get(index)) else { return };
        let Some(texture) = usize::try_from(surface.texture).ok().filter(|&texture| texture < per_texture.len()) else { return };
        let lightmap = usize::from(surface.lightmap);
        let (left, top) = ((lightmap % columns) as f32 * lightmap_width, (lightmap / columns) as f32 * lightmap_height);
        let lightmap_uv = |corner: [f32; 2]| {
            [(left + 1.0 + corner[0] * (lightmap_width - 2.0)) / atlas_size[0], (top + 1.0 + corner[1] * (lightmap_height - 2.0)) / atlas_size[1]]
        };
        let normal = normalize(cross(subtract(corners[2], corners[0]), subtract(corners[1], corners[0])));
        let [b, g, r, a] = surface.color;
        let color = [r, g, b, a].map(|channel| f32::from(channel) / 255.0);
        let vertex = |corner: usize, lightmap_corner: [f32; 2]| GroundVertex {
            position: corners[corner],
            normal,
            uv: [surface.u[corner], surface.v[corner]],
            lightmap_uv: lightmap_uv(lightmap_corner),
            color: [color[0], color[1], color[2], 1.0],
        };
        let quad = [vertex(0, [0.0, 0.0]), vertex(1, [1.0, 0.0]), vertex(2, [0.0, 1.0]), vertex(3, [1.0, 1.0])];
        per_texture[texture].extend([quad[0], quad[1], quad[3], quad[0], quad[3], quad[2]]);
    };

    for y in 0..gnd.height {
        for x in 0..gnd.width {
            let cube = &gnd.cubes[y * gnd.width + x];
            let [south_west, south_east, north_west, north_east] = cube.heights.map(|height| -height);
            let (x0, x1, z0, z1) = (x as f32 * zoom, (x + 1) as f32 * zoom, y as f32 * zoom, (y + 1) as f32 * zoom);
            push_quad(cube.top, [[x0, south_west, z0], [x1, south_east, z0], [x0, north_west, z1], [x1, north_east, z1]]);
            if let Some(north) = gnd.cubes.get((y + 1) * gnd.width + x).filter(|_| y + 1 < gnd.height) {
                let [next_south_west, next_south_east, ..] = north.heights.map(|height| -height);
                push_quad(cube.north, [[x0, north_west, z1], [x1, north_east, z1], [x0, next_south_west, z1], [x1, next_south_east, z1]]);
            }
            if let Some(east) = gnd.cubes.get(y * gnd.width + x + 1).filter(|_| x + 1 < gnd.width) {
                let [next_south_west, _, next_north_west, _] = east.heights.map(|height| -height);
                push_quad(cube.east, [[x1, north_east, z1], [x1, south_east, z0], [x1, next_north_west, z1], [x1, next_south_west, z0]]);
            }
        }
    }

    let mut vertices = Vec::new();
    let mut draws = Vec::new();
    for (texture, triangles) in per_texture.into_iter().enumerate() {
        if !triangles.is_empty() {
            let start = vertices.len() as u32;
            vertices.extend(triangles);
            draws.push((texture, start..vertices.len() as u32));
        }
    }
    (vertices, draws)
}

/// Every RSW model instance baked into world space, grouped by texture; returns vertices, draws and texture names.
/// Models' vertices, the ranges of them each texture draws, and the textures.
/// A model read once for a map, with its triangles at rest.
type LoadedModel = (std::rc::Rc<Rsm>, Vec<model::Triangle>);

/// Also how many RSM 2 instances were placed, and those that move, apart.
type ModelMesh = (Vec<GroundVertex>, Vec<(usize, Range<u32>)>, Vec<String>, usize, Vec<Animated>);

/// An RSM 2 instance whose tracks play: its model, where it stands, its clock, and its triangles' order (by texture)
/// and draws in the animated buffer.
pub struct Animated {
    rsm: std::rc::Rc<Rsm>,
    placement: model::Matrix,
    color: [f32; 4],
    anim_type: i32,
    speed: f32,
    start: Option<std::time::Instant>,
    order: Vec<usize>,
    /// The texture and vertex count of each run of its ordered triangles, then those runs' places in the buffer.
    runs: Vec<(usize, u32)>,
    draws: Vec<(usize, Range<u32>)>,
}

impl World {
    /// How many RSM 2 instances move on this map.
    pub fn rsm_2_moving(&self) -> usize {
        self.animated.len()
    }

    /// Poses the moving RSM 2 models for `now`: each on its own clock (started the first time it's drawn), instances of
    /// a model at the same frame posed once.
    pub fn animate(&mut self, gpu: &Gpu, now: std::time::Instant) {
        let Some(buffer) = &self.animated_buffer else { return };
        let mut vertices = Vec::new();
        let mut posed: HashMap<(*const Rsm, u32), Vec<model::Triangle>> = HashMap::new();
        for instance in &mut self.animated {
            let start = *instance.start.get_or_insert(now);
            let elapsed = now.saturating_duration_since(start).as_millis() as u64;
            let (frame, again) = model::clock(elapsed, instance.speed, instance.anim_type, instance.rsm.frames, instance.rsm.fps);
            if again {
                instance.start = Some(now);
            }
            let triangles = posed.entry((std::rc::Rc::as_ptr(&instance.rsm), frame.to_bits())).or_insert_with(|| instance.rsm.triangles_at(frame));
            for &triangle in &instance.order {
                if let Some(triangle) = triangles.get(triangle) {
                    vertices.extend(placed_vertices(&instance.placement, instance.color, triangle));
                }
            }
        }
        gpu.queue.write_buffer(buffer, 0, bytemuck::cast_slice(&vertices));
    }
}

/// Where an RSW model instance stands: its scale, then Ry, Rx and Rz, then its position (moved by `half` from the
/// map's centre). RSM 2's y flip and RSM 1's centring come before, in the model's triangles.
fn placement(instance: &rustnarok_client::map::Model, half: [f32; 2]) -> model::Matrix {
    let [rx, ry, rz] = instance.rotation.map(f32::to_radians);
    [
        model::translation([instance.position[0] + half[0], instance.position[1], instance.position[2] + half[1]]),
        model::rotation([0.0, 0.0, 1.0], rz),
        model::rotation([1.0, 0.0, 0.0], rx),
        model::rotation([0.0, 1.0, 0.0], ry),
        model::scaling(instance.scale),
    ]
    .iter()
    .fold(model::IDENTITY, |matrix, next| model::multiply(&matrix, next))
}

fn model_color([r, g, b]: [u8; 3]) -> [f32; 4] {
    [f32::from(r) / 255.0, f32::from(g) / 255.0, f32::from(b) / 255.0, 1.0]
}

/// A triangle's three vertices placed in the world (y up), with its face normal and the instance's colour.
fn placed_vertices(placement: &model::Matrix, color: [f32; 4], triangle: &model::Triangle) -> [GroundVertex; 3] {
    // Native space has Y down; the world has Y up.
    let positions = triangle.positions.map(|position| {
        let [x, y, z] = model::transform(placement, position);
        [x, -y, z]
    });
    let normal = normalize(cross(subtract(positions[1], positions[0]), subtract(positions[2], positions[0])));
    std::array::from_fn(|corner| GroundVertex { position: positions[corner], normal, uv: triangle.uvs[corner], lightmap_uv: [0.0; 2], color })
}

fn model_mesh(grf: &Grf, rsw: &Rsw, gnd: &Gnd) -> ModelMesh {
    let mut texture_index: HashMap<String, usize> = HashMap::new();
    let mut per_texture: Vec<Vec<GroundVertex>> = Vec::new();
    let mut models: HashMap<&str, Option<LoadedModel>> = HashMap::new();
    // RSW positions are relative to the map's center.
    let half = [gnd.width as f32 * gnd.zoom / 2.0, gnd.height as f32 * gnd.zoom / 2.0];
    let mut rsm_2 = 0;
    let mut animated = Vec::new();

    for instance in &rsw.models {
        let entry = models.entry(instance.file.as_str()).or_insert_with(|| {
            let rsm = Rsm::parse(&grf.read(&format!("data\\model\\{}", instance.file)).ok()?).ok()?;
            let triangles = rsm.triangles();
            Some((std::rc::Rc::new(rsm), triangles))
        });
        let Some((rsm, triangles)) = entry else { continue };
        rsm_2 += usize::from(rsm.version >= (2, 0));

        let placement = placement(instance, half);
        let color = model_color(instance.tint);

        let mut texture_of = |name: &String, per_texture: &mut Vec<Vec<GroundVertex>>| {
            *texture_index.entry(name.clone()).or_insert_with(|| {
                per_texture.push(Vec::new());
                per_texture.len() - 1
            })
        };
        // RSM 2 that plays (a type and tracks that move) is posed again each frame, in a buffer of its own.
        if instance.anim_type != 0 && rsm.animated() {
            let textures: Vec<usize> = rsm.textures.iter().map(|name| texture_of(name, &mut per_texture)).collect();
            let mut order: Vec<usize> = (0..triangles.len()).collect();
            order.sort_by_key(|&triangle| textures[triangles[triangle].texture]);
            let mut runs: Vec<(usize, u32)> = Vec::new();
            for &triangle in &order {
                let texture = textures[triangles[triangle].texture];
                match runs.last_mut() {
                    Some((last, count)) if *last == texture => *count += 3,
                    _ => runs.push((texture, 3)),
                }
            }
            animated.push(Animated {
                rsm: rsm.clone(),
                placement,
                color,
                anim_type: instance.anim_type,
                speed: instance.anim_speed,
                start: None,
                order,
                runs,
                draws: Vec::new(),
            });
            continue;
        }
        for triangle in triangles.iter() {
            let texture = texture_of(&rsm.textures[triangle.texture], &mut per_texture);
            per_texture[texture].extend(placed_vertices(&placement, color, triangle));
        }
    }

    let mut names = vec![String::new(); texture_index.len()];
    for (name, index) in texture_index {
        names[index] = name;
    }
    let mut vertices = Vec::new();
    let mut draws = Vec::new();
    for (texture, triangles) in per_texture.into_iter().enumerate() {
        let start = vertices.len() as u32;
        vertices.extend(triangles);
        draws.push((texture, start..vertices.len() as u32));
    }
    (vertices, draws, names, rsm_2, animated)
}

/// A walk's path: the search with the game's 149-node limit.
pub fn find_path(gat: &Gat, from: [i32; 2], to: [i32; 2]) -> Option<Vec<[i32; 2]>> {
    search(gat, from, to, Some(WALK_NODES), None)
}

/// A unit's walk heard from the server, by its object type: an NPC with a monster's look (12) may step onto type-5
/// cells.
pub fn find_unit_path(gat: &Gat, from: [i32; 2], to: [i32; 2], kind: u8) -> Option<Vec<[i32; 2]>> {
    search(gat, from, to, Some(WALK_NODES), Some(kind))
}

/// rathena's object type for an NPC whose look is a monster class (`clif_bl_type`).
const MONSTER_LOOK: u8 = 12;

/// The navigation trail's path: the same search with no limit.
pub fn find_trail(gat: &Gat, from: [i32; 2], to: [i32; 2]) -> Option<Vec<[i32; 2]>> {
    search(gat, from, to, None, None)
}

/// The most cells a walk's search reaches: the start is the first, and the 150th ends it with no path.
const WALK_NODES: usize = 149;

/// Cells walked from `from` to `to` (excluding `from`), 8-directional; diagonals need both sides open. The server's
/// search, step for step, so both walk the same cells: A* at 10 a straight step and 14 a diagonal, with 10 times the
/// Manhattan distance left as the estimate, neighbours tried from south-east round anticlockwise, and a binary heap on
/// that sum alone, ties falling where the heap leaves them. A walk across open ground takes its diagonal steps first.
/// At most `limit` cells are reached; there's no path from a cell to itself. The walker's object type (`kind`) lets an
/// NPC with a monster's look step onto type-5 cells; a diagonal's side cells still block on 1 and 5.
fn search(gat: &Gat, from: [i32; 2], to: [i32; 2], limit: Option<usize>, kind: Option<u8>) -> Option<Vec<[i32; 2]>> {
    use std::collections::HashMap;

    struct Node {
        cell: [i32; 2],
        parent: Option<usize>,
        cost: i32,
        sum: i32,
        open: bool,
    }
    if from == to {
        return None;
    }
    let monster_look = kind == Some(MONSTER_LOOK);
    if !gat.walkable_by(to, monster_look) {
        return None;
    }
    let estimate = |cell: [i32; 2]| 10 * ((cell[0] - to[0]).abs() + (cell[1] - to[1]).abs());
    let mut nodes = vec![Node { cell: from, parent: None, cost: 0, sum: estimate(from), open: true }];
    let mut known = HashMap::from([(from, 0)]);
    let mut heap = vec![0];
    while !heap.is_empty() {
        let current = heap_pop(&mut heap, &nodes, |node| node.sum);
        nodes[current].open = false;
        let (cell, spent) = (nodes[current].cell, nodes[current].cost);
        if cell == to {
            let mut path = vec![];
            let mut at = current;
            while let Some(parent) = nodes[at].parent {
                path.push(nodes[at].cell);
                at = parent;
            }
            path.reverse();
            return Some(path);
        }
        // A diagonal's side cells block on 1 and 5; the cell stepped onto takes the walker's own test.
        let open = |dx: i32, dy: i32| gat.walkable([cell[0] + dx, cell[1] + dy]);
        let onto = |dx: i32, dy: i32| gat.walkable_by([cell[0] + dx, cell[1] + dy], monster_look);
        let (north, south, east, west) = (open(0, 1), open(0, -1), open(1, 0), open(-1, 0));
        let ways = [
            (1, -1, south && east),
            (1, 0, true),
            (1, 1, north && east),
            (0, 1, true),
            (-1, 1, north && west),
            (-1, 0, true),
            (-1, -1, south && west),
            (0, -1, true),
        ];
        for (dx, dy, sides) in ways {
            if !sides || !onto(dx, dy) {
                continue;
            }
            let diagonal = dx != 0 && dy != 0;
            let next = [cell[0] + dx, cell[1] + dy];
            let cost = spent + if diagonal { 14 } else { 10 };
            match known.get(&next) {
                Some(&index) if cost < nodes[index].cost => {
                    let node = &mut nodes[index];
                    (node.cost, node.parent, node.sum) = (cost, Some(current), cost + estimate(next));
                    if node.open {
                        let at = heap.iter().position(|&queued| queued == index).expect("an open node is queued");
                        heap_rise(&mut heap, &nodes, |node| node.sum, 0, at);
                        heap_sink(&mut heap, &nodes, |node| node.sum, at);
                    } else {
                        node.open = true;
                        heap_push(&mut heap, &nodes, |node| node.sum, index);
                    }
                }
                Some(_) => {}
                // A cell reached the first time is a new node; one past the limit ends the search with no path.
                None => {
                    if limit.is_some_and(|limit| nodes.len() >= limit) {
                        return None;
                    }
                    known.insert(next, nodes.len());
                    nodes.push(Node { cell: next, parent: Some(current), cost, sum: cost + estimate(next), open: true });
                    heap_push(&mut heap, &nodes, |node| node.sum, nodes.len() - 1);
                }
            }
        }
    }
    None
}

// The server's binary heap of indices into `items`, smallest `key` on top; its exact moves decide which of equal keys
// comes out first.
fn heap_push<T>(heap: &mut Vec<usize>, items: &[T], key: impl Fn(&T) -> i32, item: usize) {
    heap.push(item);
    let last = heap.len() - 1;
    heap_rise(heap, items, key, 0, last);
}

/// Takes the top off: the last item goes on top and sinks to a leaf, then rises.
fn heap_pop<T>(heap: &mut Vec<usize>, items: &[T], key: impl Fn(&T) -> i32) -> usize {
    let top = heap.swap_remove(0);
    if !heap.is_empty() {
        heap_sink(heap, items, key, 0);
    }
    top
}

/// Moves the item at `at` up while its parent's key is larger, no higher than `top`.
fn heap_rise<T>(heap: &mut [usize], items: &[T], key: impl Fn(&T) -> i32, top: usize, mut at: usize) {
    while at > top {
        let parent = (at - 1) / 2;
        if key(&items[heap[parent]]) <= key(&items[heap[at]]) {
            break;
        }
        heap.swap(parent, at);
        at = parent;
    }
}

/// Moves the item at `from` down to a leaf, always by the smaller child (the right one on a tie), then back up.
fn heap_sink<T>(heap: &mut [usize], items: &[T], key: impl Fn(&T) -> i32, from: usize) {
    let mut at = from;
    while 2 * at + 1 < heap.len() {
        let (left, right) = (2 * at + 1, 2 * at + 2);
        let child = if right >= heap.len() || key(&items[heap[left]]) < key(&items[heap[right]]) { left } else { right };
        heap.swap(at, child);
        at = child;
    }
    heap_rise(heap, items, key, from, at);
}

/// Whether a player at `from` can hit `to` with `range` (rathena's `check_distance_client`: circular, and a cell longer on straight lines).
pub fn in_reach(from: [i32; 2], to: [i32; 2], range: i32) -> bool {
    let (dx, dy) = ((from[0] - to[0]) as f32, (from[1] - to[1]) as f32);
    ((dx * dx + dy * dy).sqrt() - 0.1) as i32 <= range
}

/// Where a click on `target` walks from `from` (the cell the player is drawn on): the target when it's `free` (in the
/// map, walkable, not blocked by a unit) and `reachable` (the path search); otherwise the game's fallback line. Along
/// the major axis (x on a tie) the major coordinate steps from the click's toward `from`'s, giving up on reaching it;
/// the minor one is `minor_click + (m − m_click) · d_minor / d_major`, truncated. At each step the first free cell of
/// the 3×3, in a fixed order, is the only candidate: the goal when reachable, else no walk.
pub fn walk_goal(target: [i32; 2], from: [i32; 2], free: impl Fn([i32; 2]) -> bool, reachable: impl Fn([i32; 2]) -> bool) -> Option<[i32; 2]> {
    const AROUND: [[i32; 2]; 9] = [[0, 0], [1, 0], [1, 1], [1, -1], [-1, 0], [-1, 1], [-1, -1], [0, 1], [0, -1]];
    if free(target) && reachable(target) {
        return Some(target);
    }
    let delta = [from[0] - target[0], from[1] - target[1]];
    let major = usize::from(delta[0].abs() < delta[1].abs());
    let minor = 1 - major;
    if delta[major] == 0 {
        return None;
    }
    let step = delta[major].signum();
    let mut m = target[major];
    while m != from[major] {
        let mut at = [0; 2];
        at[major] = m;
        at[minor] = target[minor] + (m - target[major]) * delta[minor] / delta[major];
        if let Some(cell) = AROUND.iter().map(|around| [at[0] + around[0], at[1] + around[1]]).find(|&cell| free(cell)) {
            return reachable(cell).then_some(cell);
        }
        m += step;
    }
    None
}

/// RO's direction index (0 south, then clockwise: south-west, west, ... south-east) for a step.
pub fn direction(step: [i32; 2]) -> u8 {
    match (step[0].signum(), step[1].signum()) {
        (0, -1) => 0,
        (-1, -1) => 1,
        (-1, 0) => 2,
        (-1, 1) => 3,
        (0, 1) => 4,
        (1, 1) => 5,
        (1, 0) => 6,
        _ => 7,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn places_an_rsm_2_vertex_without_centring() {
        // One vertex 1 up (model y is down, so -1) and 2 east, in a file whose origin is the vertex's base: the y flip
        // makes it 1 down in native space, then the instance doubles it, turns it a quarter about y and moves it.
        let instance = rustnarok_client::map::Model {
            name: String::new(),
            file: "a.rsm2".into(),
            position: [10.0, 0.0, 20.0],
            rotation: [0.0, 90.0, 0.0],
            scale: [2.0; 3],
            anim_type: 0,
            anim_speed: 1.0,
            tint: [0x33, 0x66, 0x99],
        };
        let matrix = placement(&instance, [100.0, 200.0]);
        let flipped = [2.0, 1.0, 0.0];
        let [x, y, z] = model::transform(&matrix, flipped);
        assert!((x - 110.0).abs() < 1e-3 && (y - 2.0).abs() < 1e-3 && (z - 216.0).abs() < 1e-3, "{x} {y} {z}");
        // In the world (y up) it stands 2 below the native y, as every model does, and takes the instance's tint.
        let triangle = model::Triangle { texture: 0, positions: [flipped; 3], uvs: [[0.0; 2]; 3] };
        let vertex = placed_vertices(&matrix, model_color(instance.tint), &triangle)[0];
        assert_eq!(vertex.position[1], -y);
        assert_eq!(vertex.color, [0x33 as f32 / 255.0, 0x66 as f32 / 255.0, 0x99 as f32 / 255.0, 1.0]);
    }

    #[test]
    fn walks_as_close_as_it_can() {
        // Open ground from 0 to 19 each way, with a wall from x 10 to 12 for y 10 to 12.
        let wall = |[x, y]: [i32; 2]| (10..=12).contains(&x) && (10..=12).contains(&y);
        let open = |cell: [i32; 2]| cell.iter().all(|value| (0..20).contains(value)) && !wall(cell);
        let any = |_: [i32; 2]| true;
        assert_eq!(walk_goal([5, 9], [0, 0], open, any), Some([5, 9]));
        // A click on the wall's edge: its own 3×3, (1,0) first.
        assert_eq!(walk_goal([12, 11], [0, 0], open, any), Some([13, 11]));
        // The middle of the wall has nothing open around it: one step back toward the player, (1,-1) is.
        assert_eq!(walk_goal([11, 11], [0, 0], open, any), Some([11, 9]));
        // An island that can't be reached (x from 15): the first free cell is the only candidate, so nothing.
        assert_eq!(walk_goal([18, 5], [0, 0], open, |cell: [i32; 2]| cell[0] < 15), None);
        // Nothing free on the line: it gives up only on reaching the player's major coordinate.
        let nothing = |_: [i32; 2]| false;
        assert_eq!(walk_goal([18, 5], [0, 5], nothing, any), None);
        // The player's own cell: no path to itself.
        let path = |from: [i32; 2]| move |cell: [i32; 2]| cell != from;
        assert_eq!(walk_goal([3, 3], [3, 3], open, path([3, 3])), None);
    }

    #[test]
    fn falls_back_along_the_games_line() {
        fn grid(width: i32, wall: impl Fn([i32; 2]) -> bool) -> impl Fn([i32; 2]) -> bool {
            move |cell: [i32; 2]| cell.iter().all(|value| (0..width).contains(value)) && !wall(cell)
        }
        // Walls x 9–11 × y 9–11: from (20, 15), a click on (10, 10) → (12, 10) (the minor truncates).
        let free = grid(40, |[x, y]: [i32; 2]| (9..=11).contains(&x) && (9..=11).contains(&y));
        let open = Gat { width: 40, height: 40, cells: (0..1600).map(|index| ([0.0; 4], u8::from(!free([index % 40, index / 40])))).collect() };
        let reach = |from: [i32; 2]| move |cell: [i32; 2]| find_path(&open, from, cell).is_some();
        assert_eq!(walk_goal([10, 10], [20, 15], &free, reach([20, 15])), Some([12, 10]));
        // Walls x 10–14 × y 13–17: from (3, 15), a click on (12, 15) on the player's row goes on along it → (9, 15).
        let free = grid(30, |[x, y]: [i32; 2]| (10..=14).contains(&x) && (13..=17).contains(&y));
        assert_eq!(walk_goal([12, 15], [3, 15], free, |_: [i32; 2]| true), Some([9, 15]));
        // Walls (6, 5), (7, 4), (7, 5), (7, 6): from (5, 5), a click on (6, 5): the candidate is the player's own cell.
        let free = grid(20, |cell: [i32; 2]| [[6, 5], [7, 4], [7, 5], [7, 6]].contains(&cell));
        assert_eq!(walk_goal([6, 5], [5, 5], free, |cell: [i32; 2]| cell != [5, 5]), None);
        // A wall on x = 10 for every y: from (5, 15), a click on (12, 18): the first free cell is behind the wall.
        let free = grid(30, |[x, _]: [i32; 2]| x == 10);
        assert_eq!(walk_goal([12, 18], [5, 15], free, |cell: [i32; 2]| cell[0] < 10), None);
        // 149 nodes: from (20, 20), (67, 20) is a goal and (68, 20) gives nothing.
        let field = Gat { width: 100, height: 100, cells: vec![([0.0; 4], 0); 10_000] };
        let free = |cell: [i32; 2]| field.walkable(cell);
        let reach = |cell: [i32; 2]| find_path(&field, [20, 20], cell).is_some();
        assert_eq!(walk_goal([67, 20], [20, 20], free, reach), Some([67, 20]));
        assert_eq!(walk_goal([68, 20], [20, 20], free, reach), None);
        assert_eq!(walk_goal([20, 20], [20, 20], free, reach), None);
    }

    #[test]
    fn turns_sprite_corners_and_nudges_them_by_screen_pixels() {
        let close = |a: [f32; 2], b: [f32; 2]| (a[0] - b[0]).abs() < 1e-4 && (a[1] - b[1]).abs() < 1e-4;
        // Unturned: corners where the layer says; the right and bottom edges one pixel further, all half a pixel down
        // and right.
        assert_eq!(sprite_corner([-5.0, -4.0], [10.0, 8.0], 0.0, [0.0, 0.0]), ([-5.0, -4.0], [0.5, 0.5]));
        assert_eq!(sprite_corner([-5.0, -4.0], [10.0, 8.0], 0.0, [1.0, 1.0]), ([5.0, 4.0], [1.5, 1.5]));
        // Mirrored (negative width): the image's left edge is on the right.
        assert_eq!(sprite_corner([5.0, -4.0], [-10.0, 8.0], 0.0, [1.0, 0.0]), ([-5.0, -4.0], [1.5, 0.5]));
        // A quarter turn clockwise about the middle: the top-left corner goes to the top right.
        let (offset, pixel) = sprite_corner([-5.0, -5.0], [10.0, 10.0], 90.0, [0.0, 0.0]);
        assert!(close(offset, [5.0, -5.0]), "{offset:?}");
        assert!(close(pixel, [1.5, 0.5]), "{pixel:?}");
    }

    #[test]
    fn melee_reach_matches_the_server() {
        assert!(in_reach([5, 5], [5, 5], 1));
        assert!(in_reach([5, 5], [6, 6], 1));
        assert!(in_reach([5, 5], [7, 5], 1));
        assert!(!in_reach([5, 5], [7, 6], 1));
        assert!(in_reach([0, 0], [10, 1], 9));
    }

    #[test]
    fn paths_avoid_walls_and_cut_no_corners() {
        // 0 walkable, 1 blocked; y grows upward in GAT, rows listed bottom first.
        let rows = ["00000", "01110", "00010", "00000"];
        let cells = rows.iter().flat_map(|row| row.bytes().map(|byte| ([0.0; 4], if byte == b'0' { 0 } else { 1 }))).collect();
        let gat = Gat { width: 5, height: 4, cells };
        let path = find_path(&gat, [0, 0], [2, 2]).unwrap();
        assert_eq!(path.last(), Some(&[2, 2]));
        assert!(path.iter().all(|&cell| gat.walkable(cell)));
        let mut previous = [0, 0];
        for &cell in &path {
            let step = [cell[0] - previous[0], cell[1] - previous[1]];
            assert!(step[0].abs() <= 1 && step[1].abs() <= 1);
            if step[0] != 0 && step[1] != 0 {
                assert!(gat.walkable([previous[0] + step[0], previous[1]]) && gat.walkable([previous[0], previous[1] + step[1]]));
            }
            previous = cell;
        }
        assert!(find_path(&gat, [0, 0], [1, 1]).is_none());
        assert_eq!(direction([0, 1]), 4);
        assert_eq!(direction([1, -1]), 7);
    }

    #[test]
    fn monster_looking_npcs_step_onto_type_5() {
        // Cell (x, y) is cells[y * W + x]; the listed cells are type 5, the rest 0.
        let gat = |width: usize, height: usize, five: &[[usize; 2]]| {
            let mut cells = vec![([0.0; 4], 0); width * height];
            for [x, y] in five {
                cells[y * width + x].1 = 5;
            }
            Gat { width, height, cells }
        };
        let across = gat(3, 1, &[[1, 0]]);
        assert_eq!(find_unit_path(&across, [0, 0], [2, 0], 12), Some(vec![[1, 0], [2, 0]]));
        assert_eq!(find_unit_path(&across, [0, 0], [2, 0], 6), None);
        let goal = gat(3, 1, &[[2, 0]]);
        assert_eq!(find_unit_path(&goal, [0, 0], [2, 0], 12), Some(vec![[1, 0], [2, 0]]));
        assert_eq!(find_unit_path(&goal, [0, 0], [2, 0], 6), None);
        // The diagonal past a type-5 side cell stays shut, so kind 12 goes through it and kind 6 round it.
        let corner = gat(2, 2, &[[1, 0]]);
        assert_eq!(find_unit_path(&corner, [0, 0], [1, 1], 12), Some(vec![[1, 0], [1, 1]]));
        assert_eq!(find_unit_path(&corner, [0, 0], [1, 1], 6), Some(vec![[0, 1], [1, 1]]));
        // Type 1 blocks everyone; clicks and the trail (no kind) treat 5 as blocked.
        let mut wall = gat(3, 1, &[]);
        wall.cells[1].1 = 1;
        assert_eq!(find_unit_path(&wall, [0, 0], [2, 0], 12), None);
        assert_eq!(find_path(&across, [0, 0], [2, 0]), None);
    }

    #[test]
    fn walk_searches_stop_at_149_cells() {
        let open = Gat { width: 120, height: 120, cells: vec![([0.0; 4], 0); 120 * 120] };
        assert_eq!(find_path(&open, [60, 60], [107, 60]).map(|path| path.len()), Some(47));
        assert_eq!(find_path(&open, [60, 60], [108, 60]), None);
        assert_eq!(find_path(&open, [40, 40], [69, 69]).map(|path| path.len()), Some(29));
        assert_eq!(find_path(&open, [40, 40], [70, 70]), None);
        // The trail has no limit.
        assert_eq!(find_trail(&open, [60, 60], [108, 60]).map(|path| path.len()), Some(48));
        // A wall across rows: round it is too far for a walk, not for the trail.
        let mut cells = vec![([0.0; 4], 0); 27 * 12];
        for x in 3..=23 {
            cells[5 * 27 + x].1 = 1;
        }
        let walled = Gat { width: 27, height: 12, cells };
        assert_eq!(find_path(&walled, [13, 3], [13, 7]), None);
        assert!(find_trail(&walled, [13, 3], [13, 7]).is_some_and(|path| path.last() == Some(&[13, 7])));
        // No path from a cell to itself.
        assert_eq!(find_path(&open, [5, 5], [5, 5]), None);
        assert_eq!(find_trail(&open, [5, 5], [5, 5]), None);
    }

    #[test]
    fn the_ground_cursor_takes_its_tiles_heights() {
        // Two tiles a row: cell (3, 1) is the north-east quarter of tile (1, 0) but keeps that tile's whole slope.
        let tiles = [[0.0; 4], [1.0, 2.0, 3.0, 4.0], [0.0; 4], [0.0; 4]];
        let quad = cell_quad(&tiles, 2, [3, 1]).unwrap();
        assert_eq!(quad, [[15.0, 1.0, 5.0], [20.0, 2.0, 5.0], [15.0, 3.0, 10.0], [20.0, 4.0, 10.0]]);
        assert!(cell_quad(&tiles, 2, [4, 0]).is_none() && cell_quad(&tiles, 2, [0, 4]).is_none() && cell_quad(&tiles, 2, [-1, 0]).is_none());
    }

    #[test]
    fn paths_are_the_servers() {
        // The server's own search gives these (rows listed bottom first): on open ground the diagonal steps come
        // first, and round a wall the ties fall as its heap leaves them.
        let gat = |rows: &[&str]| Gat {
            width: rows[0].len(),
            height: rows.len(),
            cells: rows.iter().flat_map(|row| row.bytes().map(|byte| ([0.0; 4], if byte == b'0' { 0 } else { 1 }))).collect(),
        };
        let open = gat(&["0000000000"; 8]);
        assert_eq!(find_path(&open, [1, 1], [7, 5]).unwrap(), [[2, 2], [3, 3], [4, 4], [5, 5], [6, 5], [7, 5]]);
        let walled = gat(&["0000000000", "0000000000", "0001110000", "0000010000", "0000010000", "0000000000", "0000000000", "0000000000"]);
        assert_eq!(find_path(&walled, [1, 1], [8, 4]).unwrap(), [[2, 1], [3, 1], [4, 1], [5, 1], [6, 1], [7, 2], [8, 3], [8, 4]]);
    }

    #[test]
    fn sprites_tilt_their_depth_toward_the_camera() {
        let pitch = 45f32.to_radians();
        // The view depth a depth buffer value stands for, and a pixel's at `below` screen pixels down from feet as far
        // away as the camera.
        let view_depth = |value: f32| NEAR / (1.0 - value / DEPTH_SCALE);
        let at = |distance: f32, layer: DepthLayer, below: f32| view_depth(sprite_depth(distance, below, plane(layer, distance, pitch, distance)));
        let near = |a: f32, b: f32| (a - b).abs() < 0.05;
        let standing = DepthLayer::default();
        // Default camera: the feet about 1.1 cells nearer than they are, the top 0.152 nearer per pixel up.
        let (top, feet, bottom) = (at(300.0, standing, -100.0), at(300.0, standing, 0.0), at(300.0, standing, 20.0));
        assert!(near(feet, 294.44) && near(top, 279.77) && near(bottom, 297.37), "{top} {feet} {bottom}");
        // Far camera: less bias.
        let (top, feet) = (at(400.0, standing, -100.0), at(400.0, standing, 0.0));
        assert!(near(feet, 392.58) && near(top, 377.92), "{top} {feet}");
        // Each image of a monster sits a layer nearer; a player's share the body's plane, a step nearer each.
        assert!(at(300.0, DepthLayer { index: 1.0, ..standing }, 0.0) < feet - 0.5);
        let player = |nudge: f32, below: f32| sprite_depth(300.0, below, plane(DepthLayer { nudge, ..standing }, 300.0, pitch, 300.0));
        for below in [-80.0, 0.0, 10.0] {
            assert!(((player(0.0, below) - player(3.0, below)) - 3e-6).abs() < 2e-7);
        }
        // The dead lie flat: their top counts as farther.
        let flat = DepthLayer { flat: true, ..standing };
        assert!(near(at(300.0, flat, 0.0), 294.44) && near(at(300.0, flat, -100.0), 309.07), "{}", at(300.0, flat, -100.0));
    }

    #[test]
    fn camera_projects_its_target_to_the_center() {
        let camera = Camera { target: [100.0, 0.0, 100.0], distance: 200.0, pitch: 0.8, yaw: 0.0, ground: f32::MIN };
        let view = camera.view(4.0 / 3.0);
        let center = view.project(camera.target).unwrap();
        assert!((center[0] - 0.5).abs() < 1e-4 && (center[1] - 0.5).abs() < 1e-4, "{center:?}");
        // East is to the right, north is up the screen.
        assert!(view.project([110.0, 0.0, 100.0]).unwrap()[0] > 0.5);
        assert!(view.project([100.0, 0.0, 110.0]).unwrap()[1] < 0.5);
        // The middle of the screen picks the target's cell on flat ground.
        let flat = Gat { width: 40, height: 40, cells: vec![([0.0; 4], 0); 1600] };
        assert_eq!(view.pick_ground([0.5, 0.5], &flat), Some([20, 20]));
    }

    #[test]
    fn picks_the_ground_drawn_under_the_pointer() {
        // Ten by ten cells of flat ground; a pit at (5, 5), ten deep (negative heights are up); a block at (5, 6),
        // twenty high.
        let ground = |block: bool| {
            let mut gat = Gat { width: 10, height: 10, cells: vec![([0.0; 4], 0); 100] };
            gat.cells[5 * 10 + 5].0 = [10.0; 4];
            if block {
                gat.cells[6 * 10 + 5].0 = [-20.0; 4];
            }
            gat
        };
        let (flat, gat) = (ground(false), ground(true));
        let toward = |from: Vector, to: Vector| subtract(to, from);
        let eye = [25.0, 50.0, -20.0];
        // Flat ground: where the ray meets y = 0.
        assert_eq!(ground_hit(&flat, eye, toward(eye, [37.0, 0.0, 41.0])), Some([7, 8]));
        // The block stops a ray bound for the cell behind it, at its near side.
        assert_eq!(ground_hit(&flat, eye, toward(eye, [25.0, 0.0, 35.0])), Some([5, 7]));
        assert_eq!(ground_hit(&gat, eye, toward(eye, [25.0, 0.0, 35.0])), Some([5, 6]));
        // A steep ray over the pit reaches its floor.
        let above = [27.5, 100.0, 20.0];
        assert_eq!(ground_hit(&gat, above, toward(above, [27.5, -10.0, 27.5])), Some([5, 5]));
        // Off the map's edge: none.
        assert_eq!(ground_hit(&gat, eye, toward(eye, [25.0, 0.0, -10.0])), None);
    }

    use rustnarok_client::map::{Cube, Surface};

    fn paint(texel: impl Fn(usize, usize) -> [u8; 4]) -> Vec<u8> {
        let mut bytes = vec![0; 8 * 8 * 4];
        for row in 0..8 {
            for column in 0..8 {
                let [shadow, red, green, blue] = texel(column, row);
                let index = row * 8 + column;
                bytes[index] = shadow;
                let color = 64 + index * 3;
                bytes[color] = red;
                bytes[color + 1] = green;
                bytes[color + 2] = blue;
            }
        }
        bytes
    }

    fn flat_light(shadow: u8, rgb: [u8; 3]) -> Vec<u8> {
        paint(|_, _| [shadow, rgb[0], rgb[1], rgb[2]])
    }

    fn ground(width: usize, height: usize, heights: [f32; 4], top: i32, lightmap: Vec<u8>) -> Gnd {
        Gnd {
            width,
            height,
            zoom: 10.0,
            textures: vec!["ground.bmp".into()],
            lightmap_size: [8, 8],
            lightmaps: vec![lightmap],
            surfaces: vec![Surface { u: [0.0; 4], v: [0.0; 4], texture: 0, lightmap: 0, color: [255; 4] }],
            cubes: vec![Cube { heights, top, north: -1, east: -1 }; width * height],
        }
    }

    fn altitude(width: usize, height: usize, corners: [f32; 4]) -> Gat {
        Gat { width, height, cells: vec![(corners, 0); width * height] }
    }

    fn near(light: [f32; 3], expected: [f32; 3]) {
        assert!(light.iter().zip(expected).all(|(channel, want)| (channel - want).abs() < 1e-5), "{light:?} {expected:?}");
    }

    fn lit_cell(lights: &[[f32; 3]], width: usize, x: usize, y: usize) -> [f32; 3] {
        lights[y * width + x]
    }

    #[test]
    fn full_light_is_white() {
        let gnd = ground(2, 2, [0.0; 4], 0, flat_light(255, [0, 0, 0]));
        let gat = altitude(4, 4, [0.0; 4]);
        let lights = cell_lights(&gnd, &gat);
        near(lit_cell(&lights, 4, 1, 1), [1.0; 3]);
    }

    #[test]
    fn half_shadow_is_half_the_shadow_byte() {
        // A 128 and colour 0 is 128/255, the half shadow the light formula calls 0.50.
        let gnd = ground(2, 2, [0.0; 4], 0, flat_light(128, [0, 0, 0]));
        let gat = altitude(4, 4, [0.0; 4]);
        let lights = cell_lights(&gnd, &gat);
        near(lit_cell(&lights, 4, 1, 1), [128.0 / 255.0; 3]);
    }

    #[test]
    fn a_coloured_lightmap_is_divided_by_its_largest_channel() {
        // 200/128, 100/128 and 0, then divided by 200/128.
        let gnd = ground(2, 2, [0.0; 4], 0, flat_light(0, [200, 100, 0]));
        let gat = altitude(4, 4, [0.0; 4]);
        let lights = cell_lights(&gnd, &gat);
        near(lit_cell(&lights, 4, 1, 1), [1.0, 0.5, 0.0]);
    }

    #[test]
    fn off_map_and_missing_tops_sample_full_bright() {
        // One neighbour of an edge cell is off the map (shadow and colour 255); the rest of the tile is black.
        let gnd = ground(2, 2, [0.0; 4], 0, flat_light(0, [0, 0, 0]));
        let gat = altitude(4, 4, [0.0; 4]);
        let lights = cell_lights(&gnd, &gat);
        let mixed = 51.0 / 255.0 + 51.0 / 128.0;
        near(lit_cell(&lights, 4, 0, 1), [mixed; 3]);
        // A tile with no top ignores its lightmap.
        let open = ground(2, 2, [0.0; 4], -1, flat_light(0, [0, 0, 0]));
        let lights = cell_lights(&open, &gat);
        near(lit_cell(&lights, 4, 1, 1), [1.0; 3]);
    }

    #[test]
    fn reads_the_quarter_texel_of_the_8x8_lightmap() {
        // Only west-south (1, 1) is dark. Cell (2, 2) is that quarter; its four neighbours read bright texels.
        let gnd = ground(2, 2, [0.0; 4], 0, paint(|column, row| if (column, row) == (1, 1) { [0, 0, 0, 0] } else { [255, 0, 0, 0] }));
        let gat = altitude(4, 4, [0.0; 4]);
        let lights = cell_lights(&gnd, &gat);
        near(lit_cell(&lights, 4, 2, 2), [204.0 / 255.0; 3]);
    }

    #[test]
    fn a_cell_more_than_2_5_above_the_mesh_stays_bright() {
        let dark = flat_light(0, [0, 0, 0]);
        let level = ground(2, 2, [0.0; 4], 0, dark.clone());
        // Exactly 2.5 above stays on the lightmap. More than that is a bridge or a model.
        let on_it = cell_lights(&level, &altitude(4, 4, [-2.5; 4]));
        near(lit_cell(&on_it, 4, 1, 1), [0.0; 3]);
        let bridge = cell_lights(&level, &altitude(4, 4, [-3.0; 4]));
        near(lit_cell(&bridge, 4, 1, 1), [1.0; 3]);
        // The highest GND corner wins: file height -10 is world Y 10, so 2.5 above it is file height -12.5.
        let raised = ground(2, 2, [0.0, 0.0, 0.0, -10.0], 0, dark);
        near(lit_cell(&cell_lights(&raised, &altitude(4, 4, [-12.5; 4])), 4, 1, 1), [0.0; 3]);
        near(lit_cell(&cell_lights(&raised, &altitude(4, 4, [-13.0; 4])), 4, 1, 1), [1.0; 3]);
    }

    #[test]
    fn light_steps_at_the_cell_border() {
        let table = [[0.1, 0.2, 0.3], [0.4, 0.5, 0.6], [0.7, 0.8, 0.9]];
        assert_eq!(light_at(&table, 3, 1, [9.999, 8.0, 0.1]), [0.4, 0.5, 0.6]);
        assert_eq!(light_at(&table, 3, 1, [10.0, 8.0, 0.1]), [0.7, 0.8, 0.9]);
        let rows = [[0.2, 0.0, 0.0], [0.8, 0.0, 0.0]];
        assert_eq!(light_at(&rows, 1, 2, [0.1, 0.0, 4.999]), [0.2, 0.0, 0.0]);
        assert_eq!(light_at(&rows, 1, 2, [0.1, 0.0, 5.0]), [0.8, 0.0, 0.0]);
        assert_eq!(light_at(&table, 3, 1, [-0.01, 0.0, 0.0]), [1.0; 3]);
        assert_eq!(light_at(&table, 3, 1, [0.0, 0.0, -0.01]), [1.0; 3]);
        assert_eq!(light_at(&table, 3, 1, [15.0, 0.0, 0.0]), [1.0; 3]);
    }
}
