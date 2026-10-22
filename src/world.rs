//! The 3D world: GND ground (textures, lightmaps, map light), RSM models, waving water, sprite billboards and the camera.
//!
//! World space: X east, Y up, Z north; one GND cube is `gnd.zoom` (10) units, one GAT cell half of that.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::io;
use std::ops::Range;
use std::time::Duration;

use image::{Rgba, RgbaImage};
use rustnarok_client::grf::Grf;
use rustnarok_client::map::{Cube, Gat, Gnd, Rsw, WaterZone, corner_wave, rsw_zone, step_water, tile_under_water, water_uv, zone_index};
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
pub const SCREEN_HEIGHT: f32 = 768.0;
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

/// No hit yet, and the reach of every box: the game's "far".
const PICK_FAR: f32 = 999_999.0;

/// The cell a ray from `eye` meets, as the game picks it: in map-centred GAT space (X = x − W·5, Z = z − H·5 with W, H
/// half the GAT's size, Y = −y so heights are the GAT's own), two flat triangles per GAT cell and no step faces, the
/// cells listed by a five-level quadtree (`pick_leaves`), the lowest t winning (the first tested on a tie). Only
/// in-map cells; None when nothing is hit.
fn ground_hit(gat: &Gat, eye: Vector, direction: Vector) -> Option<[i32; 2]> {
    let direction = normalize(direction);
    let [half_w, half_h] = [gat.width as i32 / 2, gat.height as i32 / 2];
    let origin = [eye[0] - (half_w * 5) as f32, -eye[1], eye[2] - (half_h * 5) as f32];
    let direction = [direction[0], -direction[1], direction[2]];
    let mut leaves = Vec::new();
    let [lo_x, lo_z] = [half_w, half_h].map(|half| -((half / 2) * 10) as f32);
    let [hi_x, hi_z] = [half_w, half_h].map(|half| ((half - half / 2) * 10 - 1) as f32);
    pick_leaves([lo_x, -PICK_FAR, lo_z], [hi_x, PICK_FAR, hi_z], 0, origin, direction, &mut leaves);
    let column = |x: f32| (x / 5.0).floor() as i32 + 2 * (gat.width as i32 / 4);
    let row = |z: f32| (z / 5.0).floor() as i32 + 2 * (gat.height as i32 / 4);
    let (mut best, mut picked) = (PICK_FAR, None);
    for (lo, hi) in leaves {
        for cy in row(lo[2])..=row(hi[2]) {
            for cx in column(lo[0])..=column(hi[0]) {
                if cx < 0 || cy < 0 || cx as usize >= gat.width || cy as usize >= gat.height {
                    continue;
                }
                let t = cell_t(gat, [half_w, half_h], [cx, cy], origin, direction);
                if best > t {
                    (best, picked) = (t, Some([cx, cy]));
                }
            }
        }
    }
    picked.filter(|_| best < PICK_FAR)
}

/// The quadtree's leaves a ray passes, in order: each node's four children (west-south, east-south, west-north,
/// east-north), five levels down from the root, which itself isn't tested.
fn pick_leaves(lo: Vector, hi: Vector, depth: u32, origin: Vector, direction: Vector, leaves: &mut Vec<(Vector, Vector)>) {
    let mid = [(lo[0] + hi[0]) * 0.5, 0.0, (lo[2] + hi[2]) * 0.5];
    let children = [
        ([lo[0], lo[1], lo[2]], [mid[0], hi[1], mid[2]]),
        ([mid[0], lo[1], lo[2]], [hi[0], hi[1], mid[2]]),
        ([lo[0], lo[1], mid[2]], [mid[0], hi[1], hi[2]]),
        ([mid[0], lo[1], mid[2]], [hi[0], hi[1], hi[2]]),
    ];
    for (lo, hi) in children {
        if !ray_meets_box(lo, hi, origin, direction) {
            continue;
        }
        if depth + 1 == 5 {
            leaves.push((lo, hi));
        } else {
            pick_leaves(lo, hi, depth + 1, origin, direction, leaves);
        }
    }
}

/// The game's slab test of a ray against a box. An axis the ray is parallel to (|d| ≤ 1e-5) passes only through the
/// middle 70 % of the box.
fn ray_meets_box(lo: Vector, hi: Vector, origin: Vector, direction: Vector) -> bool {
    let (mut near, mut far) = (-PICK_FAR, PICK_FAR);
    for axis in 0..3 {
        let centre = (hi[axis] + lo[axis]) * 0.5;
        let half = hi[axis] - centre;
        let relative = centre - origin[axis];
        if direction[axis].abs() > 1e-5 {
            let inverse = 1.0 / direction[axis];
            let (t1, t2) = ((relative + half) * inverse, (relative - half) * inverse);
            near = near.max(t1.min(t2));
            far = far.min(t1.max(t2));
            if near > far || far < 0.0 {
                return false;
            }
        } else if relative.abs() > 0.7 * half {
            return false;
        }
    }
    if near > 0.0 { near <= PICK_FAR } else { far <= PICK_FAR }
}

/// Where a ray meets a GAT cell's two triangles, (SW, SE, NW) then (SE, NE, NW): the lower t, PICK_FAR for none.
fn cell_t(gat: &Gat, [half_w, half_h]: [i32; 2], [cx, cy]: [i32; 2], origin: Vector, direction: Vector) -> f32 {
    let [south_west, south_east, north_west, north_east] = gat.cells[cy as usize * gat.width + cx as usize].0;
    let [x0, x1] = [(cx - half_w) * 5, (cx + 1 - half_w) * 5].map(|value| value as f32);
    let [z0, z1] = [(cy - half_h) * 5, (cy + 1 - half_h) * 5].map(|value| value as f32);
    let [sw, se, nw, ne] = [[x0, south_west, z0], [x1, south_east, z0], [x0, north_west, z1], [x1, north_east, z1]];
    let mut t = PICK_FAR;
    if let Some(hit) = triangle_t(origin, direction, sw, se, nw) {
        t = hit.min(PICK_FAR);
    }
    if let Some(hit) = triangle_t(origin, direction, se, ne, nw) {
        t = hit.min(t);
    }
    t
}

/// The game's ray–triangle test, in its order of operations; no t ≥ 0 test and no far limit.
fn triangle_t(origin: Vector, direction: Vector, v0: Vector, v1: Vector, v2: Vector) -> Option<f32> {
    let gat_dot = |a: Vector, b: Vector| (a[1] * b[1] + a[0] * b[0]) + a[2] * b[2];
    let (edge1, edge2) = (subtract(v1, v0), subtract(v2, v0));
    let p = cross(direction, edge2);
    let determinant = gat_dot(p, edge1);
    if -1e-5 < determinant && determinant < 1e-5 {
        return None;
    }
    let inverse = 1.0 / determinant;
    let s = subtract(origin, v0);
    let u = gat_dot(s, p) * inverse;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = cross(s, edge1);
    let v = gat_dot(q, direction) * inverse;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    Some(gat_dot(q, edge2) * inverse)
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
    pub fn depth_of(&self, point: Vector) -> f32 {
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
    /// Fog colour. Unused when `fog_range.y` is not past `fog_range.x`.
    fog_color: [f32; 4],
    /// x: depth where fog starts, y: depth where it is solid.
    fog_range: [f32; 4],
    /// rgb: the scene tint. w is 1 while the player is blind, and models then use the ambient instead.
    tint: [f32; 4],
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
    fog_color: vec4<f32>,
    fog_range: vec4<f32>,
    tint: vec4<f32>,
};
@group(0) @binding(0) var<uniform> globals: Globals;
@group(1) @binding(0) var image: texture_2d<f32>;
@group(1) @binding(1) var image_sampler: sampler;
@group(2) @binding(0) var lightmap: texture_2d<f32>;
@group(2) @binding(1) var lightmap_sampler: sampler;

fn fog_of(depth: f32) -> f32 {
    let span = globals.fog_range.y - globals.fog_range.x;
    if (span <= 0.0) { return 0.0; }
    return clamp((depth - globals.fog_range.x) / span, 0.0, 1.0);
}

fn apply_fog(color: vec3<f32>, amount: f32) -> vec3<f32> {
    return color * (1.0 - amount) + globals.fog_color.rgb * amount;
}

struct GroundOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) lightmap_uv: vec2<f32>,
    @location(2) normal: vec3<f32>,
    @location(3) color: vec4<f32>,
    @location(4) fog: f32,
};

@vertex fn ground_vs(@location(0) position: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) uv: vec2<f32>,
                     @location(3) lightmap_uv: vec2<f32>, @location(4) color: vec4<f32>) -> GroundOutput {
    var out: GroundOutput;
    out.position = globals.view_projection * vec4(position, 1.0);
    out.uv = uv;
    out.lightmap_uv = lightmap_uv;
    out.normal = normal;
    out.color = color;
    out.fog = fog_of(out.position.w);
    return out;
}

@fragment fn ground_fs(in: GroundOutput) -> @location(0) vec4<f32> {
    let texel = textureSample(image, image_sampler, in.uv);
    let light_map = textureSample(lightmap, lightmap_sampler, in.lightmap_uv);
    if (texel.a < 207.0 / 255.0) { discard; }
    // The sun is already in the vertex colour. The scene tint multiplies that, then the lightmap adds its colour.
    let lit = light_map.rgb + light_map.a * texel.rgb * in.color.rgb * globals.tint.rgb;
    return vec4(apply_fog(saturate(lit), in.fog), 1.0);
}

@fragment fn water_fs(in: GroundOutput) -> @location(0) vec4<f32> {
    let tinted = textureSample(image, image_sampler, in.uv).rgb * in.color.rgb;
    // Opaque lava is the vertex colour itself. Water keeps its texture.
    let color = select(tinted, in.color.rgb, in.color.a >= 240.0 / 255.0);
    return vec4(apply_fog(color, in.fog), in.color.a);
}

// Blind replaces the scene tint with the ambient. The light is already in the vertex colour.
fn scene_shade() -> vec3<f32> {
    return select(globals.tint.rgb, globals.ambient.rgb, globals.tint.w > 0.5);
}

@fragment fn model_fs(in: GroundOutput) -> @location(0) vec4<f32> {
    let texel = textureSample(image, image_sampler, in.uv);
    if (texel.a < 207.0 / 255.0) { discard; }
    // The light is baked into the corners' colour at load. The scene tint multiplies that.
    return vec4(apply_fog(texel.rgb * in.color.rgb * scene_shade(), in.fog), 1.0);
}

// Translucent model textures, blended: texels of 240 and up, those below, or any but none.
fn model_blended(in: GroundOutput, texel: vec4<f32>) -> vec4<f32> {
    return vec4(apply_fog(texel.rgb * in.color.rgb * scene_shade(), in.fog), texel.a);
}

@fragment fn model_solid_fs(in: GroundOutput) -> @location(0) vec4<f32> {
    let texel = textureSample(image, image_sampler, in.uv);
    if (texel.a < 240.0 / 255.0) { discard; }
    return model_blended(in, texel);
}

@fragment fn model_soft_fs(in: GroundOutput) -> @location(0) vec4<f32> {
    let texel = textureSample(image, image_sampler, in.uv);
    if (texel.a >= 240.0 / 255.0 || texel.a < 1.0 / 255.0) { discard; }
    return model_blended(in, texel);
}

@fragment fn model_any_fs(in: GroundOutput) -> @location(0) vec4<f32> {
    let texel = textureSample(image, image_sampler, in.uv);
    if (texel.a < 1.0 / 255.0) { discard; }
    return model_blended(in, texel);
}

// Shadows and effects take their feet's `depth`, flat. Sprites work theirs out per pixel from their `plane` (the
// feet's view depth and screen y, depth gained per screen pixel down, bias) and `nudge`: worked out per corner, depth
// differed between layers by rounding, so a headgear or head could fall behind the layer under it in some frames.
struct SpriteOutput { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32>, @location(1) tint: vec4<f32>,
                      @location(2) @interpolate(flat) depth: f32, @location(3) @interpolate(flat) plane: vec4<f32>,
                      @location(4) @interpolate(flat) nudge: f32, @location(5) @interpolate(linear) screen_y: f32,
                      @location(6) fog: f32 };
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
    out.fog = fog_of(clip.w);
    return out;
}

@fragment fn sprite_fs(in: SpriteOutput) -> SpriteFragment {
    let texel = textureSample(image, image_sampler, in.uv);
    // Shadows and RGBA sprite images are translucent; palette sprites are fully opaque or clear.
    if (texel.a < 0.02) { discard; }
    let color = texel * in.tint;
    return SpriteFragment(vec4(apply_fog(color.rgb, in.fog), color.a), in.depth);
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
    return SpriteFragment(vec4(apply_fog(color.rgb, in.fog), color.a), tilted_depth(in));
}

@fragment fn soft_fs(in: SpriteOutput) -> SpriteFragment {
    let color = textureSample(image, image_sampler, in.uv) * in.tint;
    if (color.a < 0.02 || color.a >= SOLID) { discard; }
    return SpriteFragment(vec4(apply_fog(color.rgb, in.fog), color.a), tilted_depth(in));
}

// Effect shapes placed in the world (cylinders): texture repeats across, colored and faded by the vertex.
struct MeshOutput { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32>, @location(1) tint: vec4<f32>, @location(2) fog: f32 };

fn mesh_vertex(position: vec3<f32>, uv: vec2<f32>, tint: vec4<f32>) -> MeshOutput {
    var out: MeshOutput;
    out.position = globals.view_projection * vec4(position, 1.0);
    out.uv = uv;
    out.tint = tint;
    out.fog = fog_of(out.position.w);
    return out;
}

@vertex fn mesh_vs(@location(0) position: vec3<f32>, @location(1) uv: vec2<f32>, @location(2) tint: vec4<f32>) -> MeshOutput {
    return mesh_vertex(position, uv, tint);
}

// Additive shapes are not fogged.
@vertex fn mesh_add_vs(@location(0) position: vec3<f32>, @location(1) uv: vec2<f32>, @location(2) tint: vec4<f32>) -> MeshOutput {
    var out = mesh_vertex(position, uv, tint);
    out.fog = 0.0;
    return out;
}

@fragment fn mesh_fs(in: MeshOutput) -> @location(0) vec4<f32> {
    let color = textureSample(image, image_sampler, vec2(fract(in.uv.x), in.uv.y)) * in.tint;
    return vec4(apply_fog(color.rgb, in.fog), color.a);
}

// The world objects' opaque faces: clamped, texels under alpha 207 dropped, unblended.
@fragment fn mesh_cut_fs(in: MeshOutput) -> @location(0) vec4<f32> {
    let texel = textureSample(image, image_sampler, clamp(in.uv, vec2(0.0), vec2(1.0)));
    if (texel.a < 207.0 / 255.0) { discard; }
    return vec4(apply_fog(texel.rgb * in.tint.rgb, in.fog), 1.0);
}

// The world objects' faces drawn with the unit sprites, in the sprites' two passes.
@fragment fn mesh_solid_fs(in: MeshOutput) -> @location(0) vec4<f32> {
    let color = textureSample(image, image_sampler, in.uv) * in.tint;
    if (color.a < SOLID) { discard; }
    return vec4(apply_fog(color.rgb, in.fog), color.a);
}

@fragment fn mesh_soft_fs(in: MeshOutput) -> @location(0) vec4<f32> {
    let color = textureSample(image, image_sampler, in.uv) * in.tint;
    if (color.a < 0.02 || color.a >= SOLID) { discard; }
    return vec4(apply_fog(color.rgb, in.fog), color.a);
}

// Effects stand in front of what they play on, pulled toward the camera.
fn effect_vertex(feet: vec3<f32>, offset: vec2<f32>, uv: vec2<f32>, tint: vec4<f32>, pixel: vec2<f32>) -> SpriteOutput {
    var out = sprite_vertex(feet, offset, uv, tint, pixel);
    out.position.z = max(out.position.z - 0.01 * out.position.w, 0.0);
    out.depth = max(out.depth - 0.01, 0.0);
    return out;
}

@vertex fn effect_vs(@location(0) feet: vec3<f32>, @location(1) offset: vec2<f32>, @location(2) uv: vec2<f32>,
                     @location(3) tint: vec4<f32>, @location(4) pixel: vec2<f32>) -> SpriteOutput {
    return effect_vertex(feet, offset, uv, tint, pixel);
}

// Additive effects are not fogged.
@vertex fn effect_add_vs(@location(0) feet: vec3<f32>, @location(1) offset: vec2<f32>, @location(2) uv: vec2<f32>,
                         @location(3) tint: vec4<f32>, @location(4) pixel: vec2<f32>) -> SpriteOutput {
    var out = effect_vertex(feet, offset, uv, tint, pixel);
    out.fog = 0.0;
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
    let per_pixel = 0.16 * 480.0 / SCREEN_HEIGHT;
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

/// A hit number part's draw order (far first): its anchor's depth, nudged.
fn number_depth(feet: f32, plane: [f32; 3]) -> f32 {
    sprite_depth(feet, 0.0, plane)
}

/// An effect's quad: texture, where it plays, its corners' offsets in sprite pixels (top left, top right, bottom right,
/// bottom left; y down), a color to multiply it by, whether it adds to what's behind instead of covering it, and whether
/// it's an STR's: `STR_RISE` screen pixels up, drawn last over everything with no depth test.
pub type EffectQuad<'a> = (&'a Texture, Vector, [[f32; 2]; 4], [f32; 4], bool, bool);

/// A camera-facing quad drawn last in the world, unfogged, with no depth test. Corners are sprite-pixel offsets
/// (y down) with their own texture coordinates, and a colour to multiply the texture by.
pub type CoverQuad<'a> = (&'a Texture, Vector, [[f32; 2]; 4], [[f32; 2]; 4], [f32; 4]);

/// A shape drawn flat on the screen over a point in the world (the game's screen prims, COLDHIT's shards): texture, the
/// point, and its triangles' corners in screen pixels from the point's projection (y down), texture coordinates and
/// colors. Drawn over everything, blended.
pub type ScreenShape<'a> = (&'a Texture, Vector, Vec<([f32; 2], [f32; 2], [f32; 4])>);

/// A unit's part drawn again for a status (its body flags): texture, feet, corners in sprite pixels (top left, top
/// right, bottom right, bottom left), colour, whether it adds on, and the screen pixels it's grown by on every side.
/// Drawn after the units with a depth test and no depth write.
pub type Glow<'a> = (&'a Texture, Vector, [[f32; 2]; 4], [f32; 4], bool, f32);

/// An effect shape in the world: texture, triangles (position, texture coordinates, color) and whether it adds on.
pub type EffectMesh<'a> = (&'a Texture, Vec<MeshVertex>, bool);

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MeshVertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
    pub tint: [f32; 4],
}

/// tint_c = 1 − (1 − L_c)(1 − ambient_c). Sprites, water, effects, the background and the fog do not use it.
pub fn scene_tint(l: [f32; 3], ambient: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|channel| 1.0 - (1.0 - l[channel]) * (1.0 - ambient[channel]))
}

/// Soul Link, rathena's night, and the soul statuses that fade L toward dusk.
pub fn is_dusk_status(status: u16) -> bool {
    matches!(status, 149 | 160 | 1056..=1059)
}

/// What models are multiplied by: the tint, or the ambient while the player is blind. The model shader does this.
#[cfg(test)]
pub fn model_shade(tint: [f32; 3], ambient: [f32; 3], blind: bool) -> [f32; 3] {
    if blind { ambient } else { tint }
}

/// The tint as thousandths, for the control state.
pub fn tint_milli(tint: [f32; 3]) -> [i32; 3] {
    tint.map(|channel| (channel * 1000.0).round() as i32)
}

/// L, the map's diffuse and ambient, and whether dusk is fading it. A map load sets L back to the diffuse.
#[derive(Clone, Debug)]
pub struct SceneLight {
    color: [f32; 3],
    target: [f32; 3],
    ambient: [f32; 3],
    /// Statuses 149, 160 and 1056–1059 currently on the player.
    statuses: Vec<u16>,
    carried: Duration,
}

impl SceneLight {
    pub fn load(ambient: [f32; 3], diffuse: [f32; 3]) -> Self {
        Self { color: diffuse, target: diffuse, ambient, statuses: Vec::new(), carried: Duration::ZERO }
    }

    /// A map load: L and its target become the new diffuse. A dusk status that is still on fades it down again.
    pub fn reload(&mut self, ambient: [f32; 3], diffuse: [f32; 3]) {
        self.color = diffuse;
        self.target = diffuse;
        self.ambient = ambient;
        self.carried = Duration::ZERO;
    }

    /// The player's dusk statuses. Another unit's status changes nothing.
    pub fn note(&mut self, own: bool, status: u16, on: bool) {
        if !own || !is_dusk_status(status) {
            return;
        }
        if on {
            if !self.statuses.contains(&status) {
                self.statuses.push(status);
            }
        } else {
            self.statuses.retain(|each| *each != status);
        }
    }

    pub fn dusk(&self) -> bool {
        !self.statuses.is_empty()
    }

    #[cfg(test)]
    pub fn color(&self) -> [f32; 3] {
        self.color
    }

    pub fn tint(&self) -> [f32; 3] {
        scene_tint(self.color, self.ambient)
    }

    /// One dusk step: on, R and G fall 0.005 while above 0.5 and B stays; off, each channel rises 0.005 while below 1 and its target.
    pub fn step(&mut self) {
        for channel in 0..3 {
            if self.dusk() {
                if channel < 2 && self.color[channel] > 0.5 {
                    self.color[channel] = (self.color[channel] - 0.005).max(0.5);
                }
            } else {
                let cap = self.target[channel].min(1.0);
                if self.color[channel] < cap {
                    self.color[channel] = (self.color[channel] + 0.005).min(cap);
                }
            }
        }
    }

    /// Steps of 1/60 s that fit in `dt`.
    pub fn advance(&mut self, dt: Duration) {
        // ponytail: a stalled frame fades at most two seconds, so a long pause doesn't spin this loop.
        self.carried += dt.min(Duration::from_secs(2));
        let step = Duration::from_secs(1) / 60;
        while self.carried >= step {
            self.carried -= step;
            self.step();
        }
    }
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
    /// The map's own effects (torches, fireflies…) and their timers.
    pub map_effects: crate::map_effects::MapEffects,
    textures: Vec<Texture>,
    lightmap: Texture,
    vertices: wgpu::Buffer,
    /// Ground triangles per texture: texture index, vertex range.
    draws: Vec<(usize, Range<u32>)>,
    model_textures: Vec<Texture>,
    model_vertices: Option<wgpu::Buffer>,
    model_draws: Vec<ModelDraw>,
    /// Which model textures have their own alpha (blended after the sprites), and the static batches that use them.
    model_translucent: Vec<bool>,
    translucent_buffer: Option<wgpu::Buffer>,
    translucent_batches: Vec<TranslucentBatch>,
    /// The RSW's translucency byte is 1: translucent texels of 240 and up write depth.
    translucency: bool,
    /// Waving water, one quad per tile that dips under it.
    water: Option<MapWater>,
    light: (Vector, [f32; 3], [f32; 3]),
    /// The colour cleared behind the world, and the same without the fog mixed in (/fog off).
    clear_color: [f32; 3],
    clear_color_bare: [f32; 3],
    /// Fog colour, and the view depths where it starts and becomes solid. End at or before the start means no fog.
    fog_color: [f32; 3],
    fog_range: [f32; 2],
    globals: wgpu::Buffer,
    globals_bind_group: wgpu::BindGroup,
    ground_pipeline: wgpu::RenderPipeline,
    model_pipelines: [wgpu::RenderPipeline; 2],
    /// Translucent model passes (`BlendPass`), one-sided then two-sided.
    model_blend_pipelines: [[wgpu::RenderPipeline; 2]; 4],
    /// Water blended over, then the same with depth write for the lava pass.
    water_pipelines: [wgpu::RenderPipeline; 2],
    sprite_pipeline: wgpu::RenderPipeline,
    /// Shadows: drawn like sprites, but without writing depth.
    shadow_pipeline: wgpu::RenderPipeline,
    /// Sprites' soft pixels: blended over without writing depth.
    soft_pipeline: wgpu::RenderPipeline,
    /// Hit numbers: the sprite pipelines with no depth test, so they draw over walls and the ground.
    // ponytail: a no-depth twin of the sprite pipelines, rather than the STR quads.
    number_pipelines: [wgpu::RenderPipeline; 2],
    /// Effects blended over, and added on; the same for effect shapes.
    effect_pipelines: [wgpu::RenderPipeline; 2],
    /// STRs: no depth test, over everything.
    str_pipelines: [wgpu::RenderPipeline; 2],
    /// The blind overlay: blended, unfogged, no depth test, drawn last.
    cover_pipeline: wgpu::RenderPipeline,
    mesh_pipelines: [wgpu::RenderPipeline; 2],
    top_mesh_pipelines: [wgpu::RenderPipeline; 2],
    /// The world objects' lists (`world_fx::List`): opaque; the floor's, blended and added; with the sprites, solid and
    /// soft. Their translucent list uses the blended mesh pipeline.
    fx_opaque_pipeline: wgpu::RenderPipeline,
    fx_floor_pipelines: [wgpu::RenderPipeline; 2],
    fx_sprite_pipelines: [wgpu::RenderPipeline; 2],
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

        // Toward the light: x east, y up, z north. Latitude is measured from the zenith. Models use this same vector.
        let direction = light_direction(rsw.light.longitude as f32, rsw.light.latitude as f32);
        let (vertices, draws) = ground_mesh(&gnd, columns, [atlas.width() as f32, atlas.height() as f32], direction, rsw.light.ambient, rsw.light.diffuse);
        let vertices = gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ground"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        // Each model texture decoded once: its image keyed, and whether it has an alpha channel of its own.
        let mut images: HashMap<String, (RgbaImage, bool)> = HashMap::new();
        let mut alpha_of = |name: &str| images.entry(name.to_owned()).or_insert_with(|| model_image(grf, name)).1;
        let (model_vertices, model_draws, model_texture_names, rsm_2_models, mut animated, translucent) = model_mesh(grf, &rsw, &gnd, &mut alpha_of);
        // The moving models' buffer: each instance's triangles in their order, its draws by texture.
        let mut moving = 0u32;
        for instance in &mut animated {
            instance.draws = std::mem::take(&mut instance.runs)
                .into_iter()
                .map(|(texture, two_sided, count)| {
                    let range = moving..moving + count;
                    moving += count;
                    (texture, two_sided, range)
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
        let model_translucent: Vec<bool> = model_texture_names.iter().map(|name| images.get(name).is_some_and(|(_, alpha)| *alpha)).collect();
        let model_textures = model_texture_names
            .iter()
            .map(|name| {
                let (image, _) = images.remove(name).unwrap_or_else(|| model_image(grf, name));
                gpu.texture_with_sampler(&image, &smooth)
            })
            .collect();
        // The translucent batches in one buffer, each with the place of its first vertex for sorting.
        let mut translucent_vertices = Vec::new();
        let translucent_batches: Vec<TranslucentBatch> = translucent
            .into_iter()
            .map(|(texture, two_sided, depth_flag, vertices)| {
                let start = translucent_vertices.len() as u32;
                let first = vertices.first().map_or([0.0; 3], |vertex| vertex.position);
                translucent_vertices.extend(vertices);
                TranslucentBatch { texture, two_sided, depth_flag, range: start..translucent_vertices.len() as u32, first }
            })
            .collect();
        let translucent_buffer = (!translucent_vertices.is_empty()).then(|| {
            gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("translucent models"),
                contents: bytemuck::cast_slice(&translucent_vertices),
                usage: wgpu::BufferUsages::VERTEX,
            })
        });
        let model_vertices = (!model_vertices.is_empty()).then(|| {
            gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("models"),
                contents: bytemuck::cast_slice(&model_vertices),
                usage: wgpu::BufferUsages::VERTEX,
            })
        });

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
        let pipeline_culled = |entry: &str,
                               fragment: &str,
                               buffer: wgpu::VertexBufferLayout,
                               blend: Option<wgpu::BlendState>,
                               depth_write: bool,
                               compare: wgpu::CompareFunction,
                               cull: Option<wgpu::Face>|
         -> wgpu::RenderPipeline {
            gpu.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(&format!("{entry}_vs")),
                    compilation_options: Default::default(),
                    buffers: &[buffer],
                },
                primitive: wgpu::PrimitiveState { cull_mode: cull, front_face: wgpu::FrontFace::Ccw, ..Default::default() },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(depth_write),
                    depth_compare: Some(compare),
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
        let pipeline_compared =
            |entry: &str,
             fragment: &str,
             buffer: wgpu::VertexBufferLayout,
             blend: Option<wgpu::BlendState>,
             depth_write: bool,
             compare: wgpu::CompareFunction| { pipeline_culled(entry, fragment, buffer, blend, depth_write, compare, None) };
        let pipeline = |entry: &str, fragment: &str, buffer: wgpu::VertexBufferLayout, blend: Option<wgpu::BlendState>, depth_write: bool| {
            pipeline_compared(entry, fragment, buffer, blend, depth_write, wgpu::CompareFunction::LessEqual)
        };
        const GROUND_ATTRIBUTES: [wgpu::VertexAttribute; 5] =
            wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32x2, 4 => Float32x4];
        let ground_layout = || wgpu::VertexBufferLayout {
            array_stride: size_of::<GroundVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &GROUND_ATTRIBUTES,
        };
        let ground_pipeline = pipeline("ground", "ground_fs", ground_layout(), None, true);
        // One-sided model faces show only their front (counter-clockwise on screen); two-sided ones both sides.
        let model_pipelines = [Some(wgpu::Face::Back), None]
            .map(|cull| pipeline_culled("ground", "model_fs", ground_layout(), None, true, wgpu::CompareFunction::LessEqual, cull));
        let blend = Some(wgpu::BlendState::ALPHA_BLENDING);
        let model_blend_pipelines = [BlendPass::Solid, BlendPass::Soft, BlendPass::Any, BlendPass::AnyDepth].map(|pass| {
            let (fragment, depth_write) = pass.look();
            [Some(wgpu::Face::Back), None]
                .map(|cull| pipeline_culled("ground", fragment, ground_layout(), blend, depth_write, wgpu::CompareFunction::LessEqual, cull))
        });
        let water_blend = Some(wgpu::BlendState::ALPHA_BLENDING);
        let water_pipelines =
            [pipeline("ground", "water_fs", ground_layout(), water_blend, false), pipeline("ground", "water_fs", ground_layout(), water_blend, true)];
        let sprite_buffer_layout = || wgpu::VertexBufferLayout {
            array_stride: size_of::<SpriteVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &SPRITE_ATTRIBUTES,
        };
        let sprite_pipeline = pipeline("sprite", "solid_fs", sprite_buffer_layout(), Some(wgpu::BlendState::ALPHA_BLENDING), true);
        let soft_pipeline = pipeline("sprite", "soft_fs", sprite_buffer_layout(), Some(wgpu::BlendState::ALPHA_BLENDING), false);
        // ponytail: hit numbers use this twin (no depth test, no depth write) so a wall doesn't hide them.
        let number_pipelines = [true, false].map(|solid| {
            pipeline_compared(
                "sprite",
                if solid { "solid_fs" } else { "soft_fs" },
                sprite_buffer_layout(),
                Some(wgpu::BlendState::ALPHA_BLENDING),
                false,
                wgpu::CompareFunction::Always,
            )
        });
        // A shadow and its sprite share a depth; rounding in the two quads' divides left the shadow nearer in places,
        // so a shadow writing depth cut holes in its own sprite. Shadows lie on what's under them and hide nothing.
        let shadow_pipeline = pipeline("sprite", "sprite_fs", sprite_buffer_layout(), Some(wgpu::BlendState::ALPHA_BLENDING), false);
        let additive = wgpu::BlendState {
            color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::SrcAlpha, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
            alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::Zero, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
        };
        let effect_pipelines = [
            pipeline("effect", "sprite_fs", sprite_buffer_layout(), Some(wgpu::BlendState::ALPHA_BLENDING), false),
            pipeline("effect_add", "sprite_fs", sprite_buffer_layout(), Some(additive), false),
        ];
        let str_pipelines = [
            pipeline_compared("effect", "sprite_fs", sprite_buffer_layout(), Some(wgpu::BlendState::ALPHA_BLENDING), false, wgpu::CompareFunction::Always),
            pipeline_compared("effect_add", "sprite_fs", sprite_buffer_layout(), Some(additive), false, wgpu::CompareFunction::Always),
        ];
        // Same unfogged vertex shader as additive effects, blended, and drawn over everything.
        let cover_pipeline =
            pipeline_compared("effect_add", "sprite_fs", sprite_buffer_layout(), Some(wgpu::BlendState::ALPHA_BLENDING), false, wgpu::CompareFunction::Always);
        let mesh_buffer_layout =
            || wgpu::VertexBufferLayout { array_stride: size_of::<MeshVertex>() as u64, step_mode: wgpu::VertexStepMode::Vertex, attributes: &MESH_ATTRIBUTES };
        let mesh_pipelines = [
            pipeline("mesh", "mesh_fs", mesh_buffer_layout(), Some(wgpu::BlendState::ALPHA_BLENDING), false),
            pipeline("mesh_add", "mesh_fs", mesh_buffer_layout(), Some(additive), false),
        ];
        let top_mesh_pipelines = [
            pipeline_compared("mesh", "mesh_fs", mesh_buffer_layout(), Some(wgpu::BlendState::ALPHA_BLENDING), false, wgpu::CompareFunction::Always),
            pipeline_compared("mesh", "mesh_fs", mesh_buffer_layout(), Some(additive), false, wgpu::CompareFunction::Always),
        ];
        let fx_opaque_pipeline = pipeline("mesh", "mesh_cut_fs", mesh_buffer_layout(), None, true);
        let fx_floor_pipelines = [
            pipeline("mesh_add", "mesh_fs", mesh_buffer_layout(), Some(wgpu::BlendState::ALPHA_BLENDING), false),
            pipeline("mesh_add", "mesh_fs", mesh_buffer_layout(), Some(additive), false),
        ];
        let fx_sprite_pipelines = [
            pipeline("mesh", "mesh_solid_fs", mesh_buffer_layout(), Some(wgpu::BlendState::ALPHA_BLENDING), true),
            pipeline("mesh", "mesh_soft_fs", mesh_buffer_layout(), Some(wgpu::BlendState::ALPHA_BLENDING), false),
        ];

        let water = MapWater::load(gpu, grf, &gnd, &rsw);

        // Sound sources sit, like models, relative to the map's center.
        let half = [gnd.width as f32 * gnd.zoom / 2.0, gnd.height as f32 * gnd.zoom / 2.0];
        let ambient = crate::ambient::Ambient::new(rsw.sounds.iter().map(|source| crate::ambient::Source::placed(source, half)).collect());
        let map_effects = crate::map_effects::MapEffects::new(&rsw.effects, half, || crate::effects::pick(0, u32::MAX - 1));
        let cell_light = cell_lights(&gnd, &gat);
        let (clear_color, clear_color_bare, fog_color, fog_range) = atmosphere(grf, map);
        let tile_heights = (gnd.cubes.iter().map(|cube| cube.heights.map(|height| -height)).collect(), gnd.width);
        Ok(Self {
            animated,
            animated_buffer,
            rsm_2_models,
            ambient,
            map_effects,
            gat,
            cell_light,
            tile_heights,
            textures,
            lightmap,
            vertices,
            draws,
            model_textures,
            model_translucent,
            translucent_buffer,
            translucent_batches,
            translucency: rsw.water_depth,
            model_vertices,
            model_draws,
            water,
            light: (direction, rsw.light.ambient, rsw.light.diffuse),
            clear_color,
            clear_color_bare,
            fog_color,
            fog_range,
            globals,
            globals_bind_group,
            ground_pipeline,
            model_pipelines,
            model_blend_pipelines,
            water_pipelines,
            sprite_pipeline,
            shadow_pipeline,
            soft_pipeline,
            number_pipelines,
            effect_pipelines,
            str_pipelines,
            cover_pipeline,
            mesh_pipelines,
            top_mesh_pipelines,
            fx_opaque_pipeline,
            fx_floor_pipelines,
            fx_sprite_pipelines,
            depth: RefCell::new(None),
        })
    }

    /// The ground under a world point: the same x and z, at the ground's height there.
    pub fn ground_under(&self, point: Vector) -> Vector {
        [point[0], -self.gat.height_at([point[0] / CELL, point[2] / CELL]), point[2]]
    }

    /// World position of a (fractional) cell's center on the ground.
    /// The map's light: the direction toward it, then its ambient and diffuse colours.
    pub fn light(&self) -> (Vector, [f32; 3], [f32; 3]) {
        self.light
    }

    pub fn cell_position(&self, cell: [f32; 2]) -> Vector {
        let height = self.gat.height_at([cell[0] + 0.5, cell[1] + 0.5]);
        [(cell[0] + 0.5) * CELL, -height, (cell[1] + 0.5) * CELL]
    }

    /// A quad over a GAT cell at the corner heights of the whole ground tile it's a quarter of (as the ground cursor
    /// draws it): corners south-west, south-east, north-west, north-east.
    pub fn cell_quad(&self, cell: [i32; 2]) -> Option<[Vector; 4]> {
        cell_quad(&self.tile_heights.0, self.tile_heights.1, cell)
    }

    /// The RSW light's ambient and diffuse.
    pub fn light_rgb(&self) -> ([f32; 3], [f32; 3]) {
        (self.light.1, self.light.2)
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

/// One fog-table row. Density is kept and not used.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Fog {
    near: f32,
    far: f32,
    /// 0xAARRGGBB.
    color: u32,
    density: f32,
}

impl Fog {
    fn rgb(self) -> [f32; 3] {
        let [_, red, green, blue] = self.color.to_be_bytes();
        [red, green, blue].map(|channel| f32::from(channel) / 255.0)
    }

    fn channels(self) -> ([u8; 3], u8) {
        let [alpha, red, green, blue] = self.color.to_be_bytes();
        ([red, green, blue], alpha)
    }

    fn range(self) -> [f32; 2] {
        [10.0 + 1490.0 * self.near, 10.0 + 1490.0 * self.far]
    }
}

/// Clear colour, fog colour and the depths where fog starts and is solid.
fn atmosphere(grf: &Grf, map: &str) -> ([f32; 3], [f32; 3], [f32; 3], [f32; 2]) {
    let looked_up = instance_map(map);
    let fog = fog_for(&parse_fog_table(&grf_text(grf, "data\\fogParameterTable.txt")), &looked_up);
    let sky = crate::lua::sky_for(grf, &looked_up);
    let clear = crate::lua::clear_color(sky, fog.map(Fog::channels));
    let bare = crate::lua::clear_color(sky, None);
    match fog {
        Some(fog) => (clear, bare, fog.rgb(), fog.range()),
        None => (clear, bare, [0.0; 3], [1.0, 0.0]),
    }
}

fn grf_text(grf: &Grf, path: &str) -> String {
    grf.read(path).map(|bytes| encoding_rs::EUC_KR.decode_without_bom_handling(&bytes).0.into_owned()).unwrap_or_default()
}

/// `0001@tower` loses the three characters that start four before `@`, so it is looked up as `1@tower`.
fn instance_map(map: &str) -> String {
    let map = map.trim_end_matches(".gat").trim_end_matches(".rsw");
    let Some(at) = map.rfind('@') else { return map.to_string() };
    if at < 4 {
        return map.to_string();
    }
    let start = at - 4;
    let mut name = String::with_capacity(map.len() - 3);
    name.push_str(&map[..start]);
    name.push_str(&map[start + 3..]);
    name
}

/// Tokens end with `#`. Five make an entry: `map.rsw`, near, far, `0xAARRGGBB`, density. `//` lines are comments.
fn parse_fog_table(text: &str) -> HashMap<String, Fog> {
    let mut tokens = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        tokens.extend(line.split('#').map(str::trim).filter(|token| !token.is_empty()).map(str::to_string));
    }
    let mut table = HashMap::new();
    for entry in tokens.chunks(5) {
        let [name, near, far, color, density] = entry else { break };
        let Some(color) = parse_fog_color(color) else { continue };
        let (Some(near), Some(far), Some(density)) = (near.parse().ok(), far.parse().ok(), density.parse().ok()) else { continue };
        table.insert(name.to_lowercase(), Fog { near, far, color, density });
    }
    table
}

/// A hex colour. A 7-digit value is padded on the left, so `0xff89003` is `0x0FF89003`.
fn parse_fog_color(token: &str) -> Option<u32> {
    let hex = token.trim().trim_start_matches('#').trim_start_matches("0x").trim_start_matches("0X");
    if hex.is_empty() || !hex.chars().all(|digit| digit.is_ascii_hexdigit()) {
        return None;
    }
    let padded = if hex.len() % 2 == 1 { format!("0{hex}") } else { hex.to_string() };
    u32::from_str_radix(&padded, 16).ok()
}

fn fog_for(table: &HashMap<String, Fog>, map: &str) -> Option<Fog> {
    let name = instance_map(map).to_lowercase();
    table.get(&format!("{name}.rsw")).copied()
}

/// How much fog covers a view depth: 0 at `10 + 1490 · near`, 1 at `10 + 1490 · far`. The shader uses the same ramp.
#[cfg(test)]
fn fog_amount(near: f32, far: f32, depth: f32) -> f32 {
    let start = 10.0 + 1490.0 * near;
    let end = 10.0 + 1490.0 * far;
    let span = end - start;
    // ponytail: a row whose far is not past its near draws no fog.
    if span <= 0.0 {
        return 0.0;
    }
    ((depth - start) / span).clamp(0.0, 1.0)
}

/// The colour after linear fog.
#[cfg(test)]
fn fogged(color: [f32; 3], fog: [f32; 3], amount: f32) -> [f32; 3] {
    [0, 1, 2].map(|index| color[index] * (1.0 - amount) + fog[index] * amount)
}

/// One water zone's clock. `counter` and `phase` advance one tick per 16 ms.
struct RunningZone {
    zone: WaterZone,
    counter: u32,
    phase: f32,
}

/// A GND tile the water may cover: its four top corner file heights, and whether it has a top.
struct WaterTile {
    heights: [f32; 4],
    top: bool,
}

/// One texture's water triangles for this frame.
struct WaterBatch {
    texture: Texture,
    /// Opaque lava writes depth when the RSW flag asks for it.
    depth_write: bool,
    buffer: wgpu::Buffer,
    count: u32,
}

/// Waving water: one quad per tile that dips under its zone, animated from the water textures.
struct MapWater {
    zones: RefCell<Vec<RunningZone>>,
    /// The last elapsed tick (`time / 0.016`) the clocks were stepped to.
    tick: Cell<u64>,
    split: [usize; 2],
    size: [usize; 2],
    zoom: f32,
    tiles: Vec<WaterTile>,
    /// Up to 32 frames per water type, indexed by the frame number in the file name.
    frames: HashMap<i32, Vec<Option<Texture>>>,
    /// The default zone is type 4: lava, coloured by the map ambient unless the player is blind.
    lava: bool,
    ambient: [f32; 3],
    /// RSW 2.2 flag: opaque water writes depth.
    depth_pass: bool,
}

impl MapWater {
    /// GND 1.8+ zones when the ground has them, otherwise the one zone from the RSW.
    fn load(gpu: &Gpu, grf: &Grf, gnd: &Gnd, rsw: &Rsw) -> Option<Self> {
        let (default, zones, split) = if let Some(water) = gnd.water.as_ref().filter(|water| !water.zones.is_empty()) {
            (water.default, water.zones.clone(), [water.split[0].max(1) as usize, water.split[1].max(1) as usize])
        } else {
            let zone = rsw_zone(rsw.water.as_ref()?);
            (zone, vec![zone], [1, 1])
        };
        let repeat = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let mut kinds: Vec<i32> = zones.iter().map(|zone| zone.kind).collect();
        kinds.sort_unstable();
        kinds.dedup();
        let mut frames = HashMap::new();
        for kind in kinds {
            let loaded: Vec<Option<Texture>> = (0..32)
                .map(|frame| {
                    let bytes = grf.read(&format!("data\\texture\\워터\\water{kind}{frame:02}.jpg")).ok()?;
                    Some(gpu.texture_with_sampler(&image::load_from_memory(&bytes).ok()?.to_rgba8(), &repeat))
                })
                .collect();
            if loaded.iter().any(Option::is_some) {
                frames.insert(kind, loaded);
            }
        }
        let tiles = gnd.cubes.iter().map(|cube| WaterTile { heights: cube.heights, top: cube.top >= 0 }).collect();
        let zones = zones.into_iter().map(|zone| RunningZone { zone, counter: 0, phase: 0.0 }).collect();
        Some(Self {
            zones: RefCell::new(zones),
            tick: Cell::new(0),
            split,
            size: [gnd.width, gnd.height],
            zoom: gnd.zoom,
            tiles,
            frames,
            lava: default.kind == 4,
            ambient: rsw.light.ambient,
            depth_pass: rsw.water_depth,
        })
    }

    /// Steps each zone once per 16 ms since the previous frame.
    fn advance(&self, time: f32) {
        let tick = (time.max(0.0) / 0.016) as u64;
        let due = tick.saturating_sub(self.tick.get());
        // ponytail: a long hitch steps at most 4096 ticks; the rest catch up on later frames.
        let steps = due.min(4096);
        if steps == 0 {
            return;
        }
        let mut zones = self.zones.borrow_mut();
        for _ in 0..steps {
            for running in zones.iter_mut() {
                let (counter, phase, _) = step_water(running.counter, running.phase, running.zone.animation_speed, running.zone.wave_speed);
                running.counter = counter;
                running.phase = phase;
            }
        }
        self.tick.set(self.tick.get() + steps);
    }

    /// The water triangles for this frame, grouped by type and texture frame.
    fn batches(&self, gpu: &Gpu, time: f32, eye_y: f32, blind: bool) -> Vec<WaterBatch> {
        self.advance(time);
        let zones = self.zones.borrow();
        // ponytail: a blind player sees lava as ordinary water. The client drops the lava colour while blind.
        let lava = self.lava && !blind;
        let color = if lava { [self.ambient[0], self.ambient[1], self.ambient[2], 1.0] } else { [1.0, 1.0, 1.0, 144.0 / 255.0] };
        // ponytail: lava is the map's default type, so every tile is opaque or none of them are. One pass, not two.
        let depth_write = self.depth_pass && color[3] >= 240.0 / 255.0;
        let [width, height] = self.size;
        let mut grouped: HashMap<(i32, u32), Vec<GroundVertex>> = HashMap::new();
        for j in 0..height {
            for i in 0..width {
                let Some(tile) = self.tiles.get(j * width + i) else { continue };
                if !tile.top {
                    continue;
                }
                let Some(running) = zones.get(zone_index(i, j, self.size, self.split)) else { continue };
                let surface = corner_wave(&running.zone, running.phase, i as i32 + 1, j as i32);
                if !tile_under_water(tile.heights, surface) {
                    continue;
                }
                // The eye under this corner's water is looking up through the surface.
                if eye_y < -surface {
                    continue;
                }
                let zone = &running.zone;
                let frame = (running.counter / zone.animation_speed.max(1) as u32) % 32;
                let (uv0, uv1) = water_uv(i, j);
                let uvs = [[uv0[0], uv0[1]], [uv1[0], uv0[1]], [uv0[0], uv1[1]], [uv1[0], uv1[1]]];
                // ponytail: a corner shared by two zones takes this tile's zone, so a seam can step where they disagree.
                let quad = [0, 1, 2, 3].map(|corner| {
                    let (gx, gz) = (i as i32 + corner as i32 % 2, j as i32 + corner as i32 / 2);
                    GroundVertex {
                        position: [gx as f32 * self.zoom, -corner_wave(zone, running.phase, gx, gz), gz as f32 * self.zoom],
                        normal: [0.0, 1.0, 0.0],
                        uv: uvs[corner],
                        lightmap_uv: [0.0; 2],
                        color,
                    }
                });
                grouped.entry((zone.kind, frame)).or_default().extend([quad[0], quad[1], quad[3], quad[0], quad[3], quad[2]]);
            }
        }
        grouped
            .into_iter()
            .filter_map(|((kind, frame), vertices)| {
                let texture = self.frames.get(&kind)?.get(frame as usize)?.as_ref()?;
                let count = vertices.len() as u32;
                let buffer = gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("water"),
                    contents: bytemuck::cast_slice(&vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });
                Some(WaterBatch { texture: texture.clone(), depth_write, buffer, count })
            })
            .collect()
    }
}

/// Everything one frame of the world needs.
pub struct WorldFrame<'a> {
    pub world: &'a World,
    pub view: &'a View,
    pub billboards: &'a [Billboard<'a>],
    /// How many of the billboards, first, are shadows.
    pub shadows: usize,
    /// Hit numbers, MISS, Lucky and totals: drawn like sprites, with no depth test, after the ground and models.
    pub numbers: &'a [Billboard<'a>],
    pub effects: &'a [EffectQuad<'a>],
    pub glows: &'a [Glow<'a>],
    pub meshes: &'a [EffectMesh<'a>],
    pub screen: &'a [ScreenShape<'a>],
    /// The blind overlay, drawn last with no depth test.
    pub cover: &'a [CoverQuad<'a>],
    /// Shapes over everything, with no depth test, after the STRs.
    pub top_meshes: &'a [EffectMesh<'a>],
    /// The world objects' other lists: opaque, floor and with the sprites, and translucent before the meshes.
    pub fx: &'a crate::world_fx::Lists<EffectMesh<'a>>,
    /// Seconds since the map was entered, for animated textures.
    pub time: f32,
    /// The player is blind (health bit 0x10 or status 1205): lava draws as ordinary water, and models use the ambient.
    pub blind: bool,
    /// The scene tint for the ground and models this frame.
    pub tint: [f32; 3],
    /// The map's depth fog shows (/fog).
    pub fog: bool,
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
            fog_color: [world.fog_color[0], world.fog_color[1], world.fog_color[2], 0.0],
            // An end at or before the start is no fog.
            fog_range: if self.fog { [world.fog_range[0], world.fog_range[1], 0.0, 0.0] } else { [1.0, 0.0, 0.0, 0.0] },
            tint: [self.tint[0], self.tint[1], self.tint[2], f32::from(u8::from(self.blind))],
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
            .flat_map(|&(_, feet, corners, tint, _, on_top)| {
                let pixel = if on_top { [0.0, -crate::effects::STR_RISE] } else { [0.0; 2] };
                let corner = |index: usize, uv: [f32; 2]| SpriteVertex { feet, offset: corners[index], uv, tint, pixel, plane: [0.0; 3] };
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
        // Screen shapes: every corner on its point, moved by its screen pixels after projecting.
        let screen_vertices: Vec<SpriteVertex> = self
            .screen
            .iter()
            .flat_map(|(_, feet, corners)| {
                corners.iter().map(move |&(pixel, uv, tint)| SpriteVertex { feet: *feet, offset: [0.0; 2], uv, tint, pixel, plane: [0.0; 3] })
            })
            .collect();
        let screen_buffer = (!screen_vertices.is_empty()).then(|| {
            gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("screen shapes"),
                contents: bytemuck::cast_slice(&screen_vertices),
                usage: wgpu::BufferUsages::VERTEX,
            })
        });
        let cover_vertices: Vec<SpriteVertex> = self
            .cover
            .iter()
            .flat_map(|&(_, feet, corners, uvs, tint)| {
                let corner = |index: usize| SpriteVertex { feet, offset: corners[index], uv: uvs[index], tint, pixel: [0.0; 2], plane: [0.0; 3] };
                [corner(0), corner(1), corner(3), corner(3), corner(1), corner(2)]
            })
            .collect();
        let cover_buffer = (!cover_vertices.is_empty()).then(|| {
            gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("blind overlay"),
                contents: bytemuck::cast_slice(&cover_vertices),
                usage: wgpu::BufferUsages::VERTEX,
            })
        });
        let glow_vertices: Vec<SpriteVertex> = self
            .glows
            .iter()
            .flat_map(|&(_, feet, corners, tint, _, grow)| {
                let corner = |index: usize, uv: [f32; 2], pixel: [f32; 2]| SpriteVertex { feet, offset: corners[index], uv, tint, pixel, plane: [0.0; 3] };
                let [top_left, top_right, bottom_right, bottom_left] = [
                    corner(0, [0.0, 0.0], [-grow, -grow]),
                    corner(1, [1.0, 0.0], [grow, -grow]),
                    corner(2, [1.0, 1.0], [grow, grow]),
                    corner(3, [0.0, 1.0], [-grow, grow]),
                ];
                [top_left, top_right, bottom_left, bottom_left, top_right, bottom_right]
            })
            .collect();
        let glow_buffer = (!glow_vertices.is_empty()).then(|| {
            gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("glows"),
                contents: bytemuck::cast_slice(&glow_vertices),
                usage: wgpu::BufferUsages::VERTEX,
            })
        });
        // One buffer for every effect shape: the world objects' opaque, floor, with-sprites and translucent lists, the meshes,
        // then the shapes over everything; `mesh_starts` is where each begins.
        let mesh_lists: [&[EffectMesh]; 6] = [&self.fx.opaque, &self.fx.floor, &self.fx.with_sprites, &self.fx.translucent, self.meshes, self.top_meshes];
        let mut mesh_starts = [0u32; 6];
        for index in 1..6 {
            mesh_starts[index] = mesh_starts[index - 1] + mesh_lists[index - 1].iter().map(|(_, vertices, _)| vertices.len() as u32).sum::<u32>();
        }
        let mesh_vertices: Vec<MeshVertex> = mesh_lists.iter().flat_map(|list| list.iter()).flat_map(|(_, vertices, _)| vertices.iter().copied()).collect();
        let mesh_buffer = (!mesh_vertices.is_empty()).then(|| {
            gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("effect shapes"),
                contents: bytemuck::cast_slice(&mesh_vertices),
                usage: wgpu::BufferUsages::VERTEX,
            })
        });
        let number_planes: Vec<[f32; 3]> =
            self.numbers.iter().map(|&(_, feet, .., layer)| plane(layer, view.depth_of(feet), view.pitch, view.distance)).collect();
        let number_vertices: Vec<SpriteVertex> = self
            .numbers
            .iter()
            .zip(&number_planes)
            .flat_map(|(&(_, feet, offset, size, tint, angle, _), &plane)| {
                let corner = |u: f32, v: f32| {
                    let (offset, pixel) = sprite_corner(offset, size, angle, [u, v]);
                    SpriteVertex { feet, offset, uv: [u, v], tint, pixel, plane }
                };
                [corner(0., 0.), corner(1., 0.), corner(0., 1.), corner(0., 1.), corner(1., 0.), corner(1., 1.)]
            })
            .collect();
        // By where each part is anchored, not its top: a critical's burst, scaled up and taller than its digits in the
        // first frames, would otherwise sort in front of them. Parts of one number share the anchor; the nudge orders them.
        let number_depths: Vec<f32> = number_planes.iter().zip(self.numbers).map(|(&plane, &(_, feet, ..))| number_depth(view.depth_of(feet), plane)).collect();
        let mut number_order: Vec<usize> = (0..self.numbers.len()).collect();
        number_order.sort_by(|&a, &b| number_depths[b].total_cmp(&number_depths[a]));
        let number_buffer = (!number_vertices.is_empty()).then(|| {
            gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("hit numbers"),
                contents: bytemuck::cast_slice(&number_vertices),
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
        let water_batches = world.water.as_ref().map(|water| water.batches(gpu, self.time, self.view.eye[1], self.blind));

        let clear = if self.fog { world.clear_color } else { world.clear_color_bare };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("world"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color { r: clear[0] as f64, g: clear[1] as f64, b: clear[2] as f64, a: 1.0 }),
                    store: wgpu::StoreOp::Store,
                },
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
        let moving = world.animated.iter().flat_map(|instance| &instance.draws);
        for (buffer, draws) in [(&world.model_vertices, world.model_draws.iter().collect::<Vec<_>>()), (&world.animated_buffer, moving.collect())] {
            let Some(buffer) = buffer else { continue };
            pass.set_vertex_buffer(0, buffer.slice(..));
            for (texture, two_sided, range) in draws.into_iter().filter(|(texture, ..)| !world.model_translucent[*texture]) {
                pass.set_pipeline(&world.model_pipelines[usize::from(*two_sided)]);
                pass.set_bind_group(1, world.model_textures[*texture].bind_group(), &[]);
                pass.draw(range.clone(), 0..1);
            }
        }
        // The world objects' opaque faces with the models, and their floor faces under the units.
        if let Some(buffer) = &mesh_buffer {
            pass.set_vertex_buffer(0, buffer.slice(..));
            draw_meshes(&mut pass, &self.fx.opaque, mesh_starts[0], |_| &world.fx_opaque_pipeline);
            draw_meshes(&mut pass, &self.fx.floor, mesh_starts[1], |additive| &world.fx_floor_pipelines[usize::from(additive)]);
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
        // The world objects' faces with the sprites, in their two passes.
        if let Some(buffer) = &mesh_buffer
            && !self.fx.with_sprites.is_empty()
        {
            pass.set_vertex_buffer(0, buffer.slice(..));
            for pipeline in &world.fx_sprite_pipelines {
                draw_meshes(&mut pass, &self.fx.with_sprites, mesh_starts[2], |_| pipeline);
            }
        }
        // The parts drawn again for statuses, over the units.
        if let Some(buffer) = &glow_buffer {
            pass.set_vertex_buffer(0, buffer.slice(..));
            for (index, (texture, .., additive, _)) in self.glows.iter().enumerate() {
                let first = index as u32 * 6;
                pass.set_pipeline(&world.effect_pipelines[usize::from(*additive)]);
                pass.set_bind_group(1, texture.bind_group(), &[]);
                pass.draw(first..first + 6, 0..1);
            }
        }
        // Translucent model textures after the sprites: the static batches back to front by their first vertex's depth,
        // then the moving models' (as they come), each in its passes for the map's translucency byte.
        if let Some(buffer) = &world.translucent_buffer {
            let mut order: Vec<&TranslucentBatch> = world.translucent_batches.iter().collect();
            back_to_front(&mut order, |batch| view.depth_of(batch.first));
            pass.set_vertex_buffer(0, buffer.slice(..));
            for batch in order {
                pass.set_bind_group(1, world.model_textures[batch.texture].bind_group(), &[]);
                for &step in blend_passes(world.translucency, batch.depth_flag) {
                    pass.set_pipeline(&world.model_blend_pipelines[step as usize][usize::from(batch.two_sided)]);
                    pass.draw(batch.range.clone(), 0..1);
                }
            }
        }
        if let Some(buffer) = &world.animated_buffer {
            pass.set_vertex_buffer(0, buffer.slice(..));
            for instance in &world.animated {
                for (texture, two_sided, range) in instance.draws.iter().filter(|(texture, ..)| world.model_translucent[*texture]) {
                    pass.set_bind_group(1, world.model_textures[*texture].bind_group(), &[]);
                    for &step in blend_passes(world.translucency, instance.depth_flag) {
                        pass.set_pipeline(&world.model_blend_pipelines[step as usize][usize::from(*two_sided)]);
                        pass.draw(range.clone(), 0..1);
                    }
                }
            }
        }
        // After the sprites, so the surface covers the part of a wading unit under it, and before effects.
        if let Some(batches) = &water_batches {
            for batch in batches {
                pass.set_pipeline(&world.water_pipelines[usize::from(batch.depth_write)]);
                pass.set_vertex_buffer(0, batch.buffer.slice(..));
                pass.set_bind_group(1, batch.texture.bind_group(), &[]);
                pass.draw(0..batch.count, 0..1);
            }
        }
        if let Some(buffer) = &mesh_buffer {
            pass.set_vertex_buffer(0, buffer.slice(..));
            // The world objects' translucent faces, blended, then the effect shapes.
            draw_meshes(&mut pass, &self.fx.translucent, mesh_starts[3], |_| &world.mesh_pipelines[0]);
            draw_meshes(&mut pass, self.meshes, mesh_starts[4], |additive| &world.mesh_pipelines[usize::from(additive)]);
        }
        if let Some(buffer) = &effect_buffer {
            pass.set_vertex_buffer(0, buffer.slice(..));
            // The STRs last, in the order they came, over everything.
            for top in [false, true] {
                for (index, (texture, .., additive, on_top)) in self.effects.iter().enumerate().filter(|(_, quad)| quad.5 == top) {
                    let first = index as u32 * 6;
                    let pipelines = if *on_top { &world.str_pipelines } else { &world.effect_pipelines };
                    pass.set_pipeline(&pipelines[usize::from(*additive)]);
                    pass.set_bind_group(1, texture.bind_group(), &[]);
                    pass.draw(first..first + 6, 0..1);
                }
            }
        }
        if let Some(buffer) = &screen_buffer {
            pass.set_vertex_buffer(0, buffer.slice(..));
            pass.set_pipeline(&world.str_pipelines[0]);
            let mut first = 0u32;
            for (texture, _, corners) in self.screen {
                let count = corners.len() as u32;
                pass.set_bind_group(1, texture.bind_group(), &[]);
                pass.draw(first..first + count, 0..1);
                first += count;
            }
        }

        // Hit numbers over the ground, walls, models and the effects (an Ice Wall's spikes, the STRs), as the game draws them
        // with no depth test after its STR and wall list. Digits sort in front of their burst.
        if let Some(buffer) = &number_buffer {
            pass.set_vertex_buffer(0, buffer.slice(..));
            let draw = |pass: &mut wgpu::RenderPass, index: usize| {
                let first = index as u32 * 6;
                pass.set_bind_group(1, self.numbers[index].0.bind_group(), &[]);
                pass.draw(first..first + 6, 0..1);
            };
            for pipeline in &world.number_pipelines {
                pass.set_pipeline(pipeline);
                number_order.iter().for_each(|&index| draw(&mut pass, index));
            }
        }

        // Then the shapes over everything, after the other shapes in the same buffer.
        if let Some(buffer) = &mesh_buffer {
            pass.set_vertex_buffer(0, buffer.slice(..));
            draw_meshes(&mut pass, self.top_meshes, mesh_starts[5], |additive| &world.top_mesh_pipelines[usize::from(additive)]);
        }
        // Last in the world, over the ground, models, units and effects, and not fogged.
        if let Some(buffer) = &cover_buffer {
            pass.set_vertex_buffer(0, buffer.slice(..));
            pass.set_pipeline(&world.cover_pipeline);
            for (index, (texture, ..)) in self.cover.iter().enumerate() {
                let first = index as u32 * 6;
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
        if [r, g, b] == [0xFF, 0x00, 0xFF] {
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

/// Toward the light. Longitude turns around the vertical axis, latitude is measured from the zenith, both in degrees.
fn light_direction(longitude: f32, latitude: f32) -> Vector {
    let (longitude, latitude) = (longitude.to_radians(), latitude.to_radians());
    normalize([-latitude.sin() * longitude.sin(), latitude.cos(), -latitude.sin() * longitude.cos()])
}

/// The sun baked into a corner: `trunc(colour · min(1, ambient + diffuse · max(0, n · light)))` per channel, as 0..1.
fn baked_color(color: [u8; 3], normal: Vector, light: Vector, ambient: [f32; 3], diffuse: [f32; 3]) -> [f32; 4] {
    let weight = dot(normal, light).max(0.0);
    let rgb = [0, 1, 2].map(|index| {
        let lit = (ambient[index] + diffuse[index] * weight).min(1.0);
        (f32::from(color[index]) * lit).trunc() / 255.0
    });
    [rgb[0], rgb[1], rgb[2], 1.0]
}

fn cube_at(gnd: &Gnd, x: i32, y: i32) -> Option<&Cube> {
    (x >= 0 && y >= 0 && (x as usize) < gnd.width && (y as usize) < gnd.height).then(|| &gnd.cubes[y as usize * gnd.width + x as usize])
}

/// RGB of the top surface of the tile that starts at (x, y). No top, or off the map, is white.
fn top_color(gnd: &Gnd, x: i32, y: i32) -> [u8; 3] {
    let Some(cube) = cube_at(gnd, x, y) else { return [255; 3] };
    let Some(surface) = usize::try_from(cube.top).ok().and_then(|index| gnd.surfaces.get(index)) else { return [255; 3] };
    let [b, g, r, _] = surface.color;
    [r, g, b]
}

/// World positions of a tile's four corners: south-west, south-east, north-west, north-east.
fn tile_corners(gnd: &Gnd, x: i32, y: i32) -> Option<[Vector; 4]> {
    let cube = cube_at(gnd, x, y)?;
    let (x0, z0) = (x as f32 * gnd.zoom, y as f32 * gnd.zoom);
    let (x1, z1) = (x0 + gnd.zoom, z0 + gnd.zoom);
    let [sw, se, nw, ne] = cube.heights.map(|height| -height);
    Some([[x0, sw, z0], [x1, se, z0], [x0, nw, z1], [x1, ne, z1]])
}

/// Face normal of triangle (a, b, c), pointing up for a flat tile.
fn face_normal(a: Vector, b: Vector, c: Vector) -> Vector {
    let normal = cross(subtract(c, a), subtract(b, a));
    if dot(normal, normal) < 1e-12 { [0.0, 1.0, 0.0] } else { normalize(normal) }
}

/// The one triangle a corner contributes. South-west, south-east and north-west sit on (v0, v1, v2); north-east only on the other.
fn corner_face(corners: [Vector; 4], corner: usize) -> Vector {
    // ponytail: a corner on both triangles contributes (v0, v1, v2). North-east is only on (v2, v1, v3).
    if corner == 3 { face_normal(corners[2], corners[1], corners[3]) } else { face_normal(corners[0], corners[1], corners[2]) }
}

/// Normal at world corner (x, z): the tiles that meet there at exactly `height` (file height, positive down).
fn corner_normal(gnd: &Gnd, x: i32, z: i32, height: f32) -> Vector {
    let mut sum = [0.0; 3];
    // This corner is south-west of (x, z), south-east of (x-1, z), north-west of (x, z-1), north-east of (x-1, z-1).
    for (tile_x, tile_z, local) in [(x, z, 0usize), (x - 1, z, 1), (x, z - 1, 2), (x - 1, z - 1, 3)] {
        let Some(cube) = cube_at(gnd, tile_x, tile_z) else { continue };
        if cube.top < 0 || cube.heights[local] != height {
            continue;
        }
        let Some(corners) = tile_corners(gnd, tile_x, tile_z) else { continue };
        let normal = corner_face(corners, local);
        sum = [sum[0] + normal[0], sum[1] + normal[1], sum[2] + normal[2]];
    }
    if dot(sum, sum) < 1e-12 { [0.0, 1.0, 0.0] } else { normalize(sum) }
}

/// Ground triangles grouped by texture: cube tops, plus north and east walls down to the neighbor.
fn ground_mesh(
    gnd: &Gnd,
    columns: usize,
    atlas_size: [f32; 2],
    light: Vector,
    ambient: [f32; 3],
    diffuse: [f32; 3],
) -> (Vec<GroundVertex>, Vec<(usize, Range<u32>)>) {
    let [lightmap_width, lightmap_height] = gnd.lightmap_size.map(|size| size as f32);
    let mut per_texture: Vec<Vec<GroundVertex>> = vec![Vec::new(); gnd.textures.len()];
    let zoom = gnd.zoom;

    // Corners in surface order (u[0..4]/v[0..4]): the lightmap's inner 6x6 pixels map onto the quad.
    let mut push_quad = |surface_index: i32, corners: [Vector; 4], colors: [[u8; 3]; 4], normals: [Vector; 4]| {
        let Some(surface) = usize::try_from(surface_index).ok().and_then(|index| gnd.surfaces.get(index)) else { return };
        let Some(texture) = usize::try_from(surface.texture).ok().filter(|&texture| texture < per_texture.len()) else { return };
        let lightmap = usize::from(surface.lightmap);
        let (left, top) = ((lightmap % columns) as f32 * lightmap_width, (lightmap / columns) as f32 * lightmap_height);
        let lightmap_uv = |corner: [f32; 2]| {
            [(left + 1.0 + corner[0] * (lightmap_width - 2.0)) / atlas_size[0], (top + 1.0 + corner[1] * (lightmap_height - 2.0)) / atlas_size[1]]
        };
        let vertex = |corner: usize, lightmap_corner: [f32; 2]| GroundVertex {
            position: corners[corner],
            normal: normals[corner],
            uv: [surface.u[corner], surface.v[corner]],
            lightmap_uv: lightmap_uv(lightmap_corner),
            color: baked_color(colors[corner], normals[corner], light, ambient, diffuse),
        };
        let quad = [vertex(0, [0.0, 0.0]), vertex(1, [1.0, 0.0]), vertex(2, [0.0, 1.0]), vertex(3, [1.0, 1.0])];
        // (v0, v1, v2) and (v2, v1, v3): the top folds along (i+1, j)–(i, j+1).
        per_texture[texture].extend([quad[0], quad[1], quad[2], quad[2], quad[1], quad[3]]);
    };

    for y in 0..gnd.height {
        for x in 0..gnd.width {
            let cube = &gnd.cubes[y * gnd.width + x];
            let [south_west, south_east, north_west, north_east] = cube.heights.map(|height| -height);
            let (x0, x1, z0, z1) = (x as f32 * zoom, (x + 1) as f32 * zoom, y as f32 * zoom, (y + 1) as f32 * zoom);
            let (ix, iy) = (x as i32, y as i32);
            let top_colors = [top_color(gnd, ix, iy), top_color(gnd, ix + 1, iy), top_color(gnd, ix, iy + 1), top_color(gnd, ix + 1, iy + 1)];
            let top_normals = [
                corner_normal(gnd, ix, iy, cube.heights[0]),
                corner_normal(gnd, ix + 1, iy, cube.heights[1]),
                corner_normal(gnd, ix, iy + 1, cube.heights[2]),
                corner_normal(gnd, ix + 1, iy + 1, cube.heights[3]),
            ];
            push_quad(cube.top, [[x0, south_west, z0], [x1, south_east, z0], [x0, north_west, z1], [x1, north_east, z1]], top_colors, top_normals);
            if let Some(north) = cube_at(gnd, ix, iy + 1) {
                let [next_south_west, next_south_east, ..] = north.heights.map(|height| -height);
                // A larger file height is lower. The wall faces that side: north when the neighbor's south-west is lower than our north-west.
                let facing = if north.heights[0] > cube.heights[2] { [0.0, 0.0, 1.0] } else { [0.0, 0.0, -1.0] };
                let colors = [top_color(gnd, ix, iy + 1), top_color(gnd, ix + 1, iy + 1), top_color(gnd, ix, iy + 1), top_color(gnd, ix + 1, iy + 1)];
                push_quad(cube.north, [[x0, north_west, z1], [x1, north_east, z1], [x0, next_south_west, z1], [x1, next_south_east, z1]], colors, [facing; 4]);
            }
            if let Some(east) = cube_at(gnd, ix + 1, iy) {
                let [next_south_west, _, next_north_west, _] = east.heights.map(|height| -height);
                // Compared with this tile's north-west (h2), not its south-east.
                let facing = if east.heights[0] > cube.heights[2] { [1.0, 0.0, 0.0] } else { [-1.0, 0.0, 0.0] };
                let colors = [top_color(gnd, ix + 1, iy + 1), top_color(gnd, ix + 1, iy), top_color(gnd, ix + 1, iy + 1), top_color(gnd, ix + 1, iy)];
                push_quad(cube.east, [[x1, north_east, z1], [x1, south_east, z0], [x1, next_north_west, z1], [x1, next_south_west, z0]], colors, [facing; 4]);
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
type ModelMesh = (Vec<GroundVertex>, Vec<ModelDraw>, Vec<String>, usize, Vec<Animated>, Vec<Translucent>);

/// An instance's triangles with one translucent texture: the texture, two-sided, the instance's 2.6 flag, its vertices.
type Translucent = (usize, bool, bool, Vec<GroundVertex>);

/// A static translucent batch in the buffer: its texture, sidedness, 2.6 flag, range and first vertex's place.
struct TranslucentBatch {
    texture: usize,
    two_sided: bool,
    depth_flag: bool,
    range: Range<u32>,
    first: Vector,
}

/// A translucent model pass: texels of 240 and up writing depth, those below without, or any (alpha 1 and up)
/// without or with depth.
#[derive(Clone, Copy, PartialEq, Debug)]
enum BlendPass {
    Solid,
    Soft,
    Any,
    AnyDepth,
}

impl BlendPass {
    /// Its fragment shader and whether it writes depth.
    fn look(self) -> (&'static str, bool) {
        match self {
            Self::Solid => ("model_solid_fs", true),
            Self::Soft => ("model_soft_fs", false),
            Self::Any => ("model_any_fs", false),
            Self::AnyDepth => ("model_any_fs", true),
        }
    }
}

/// A translucent batch's passes, by the map's translucency byte and the model's 2.6 flag: byte 1 draws its texels of
/// 240 and up with depth, then the rest without (with the flag, every visible texel with depth); byte 0 one pass
/// without depth (with the flag, with).
fn blend_passes(translucency: bool, depth_flag: bool) -> &'static [BlendPass] {
    match (translucency, depth_flag) {
        (true, false) => &[BlendPass::Solid, BlendPass::Soft],
        (false, false) => &[BlendPass::Any],
        (_, true) => &[BlendPass::AnyDepth],
    }
}

/// Puts things back to front: the deepest first.
/// Draws effect shapes from the bound buffer, the first at vertex `first`, each with the pipeline for its blending.
fn draw_meshes<'a>(pass: &mut wgpu::RenderPass, meshes: &[EffectMesh], mut first: u32, pipeline: impl Fn(bool) -> &'a wgpu::RenderPipeline) {
    for (texture, vertices, additive) in meshes {
        let count = vertices.len() as u32;
        pass.set_pipeline(pipeline(*additive));
        pass.set_bind_group(1, texture.bind_group(), &[]);
        pass.draw(first..first + count, 0..1);
        first += count;
    }
}

fn back_to_front<T>(items: &mut [T], depth: impl Fn(&T) -> f32) {
    items.sort_by(|a, b| depth(b).total_cmp(&depth(a)));
}

/// A model texture keyed (#FF00FF clear) and whether its file has an alpha channel of its own; grey when missing.
fn model_image(grf: &Grf, name: &str) -> (RgbaImage, bool) {
    let decoded = grf.read(&format!("data\\texture\\{name}")).ok().and_then(|bytes| image::load_from_memory(&bytes).ok());
    match decoded {
        Some(image) => {
            let alpha = image.color().has_alpha();
            (key_out_magenta(image.to_rgba8()), alpha)
        }
        None => (RgbaImage::from_pixel(1, 1, Rgba([128, 128, 128, 255])), false),
    }
}

/// A run of model vertices with one texture: the texture, whether its faces are drawn from both sides, the range.
type ModelDraw = (usize, bool, Range<u32>);

/// An RSM 2 instance whose tracks play: its model, where it stands, its clock, and its triangles' order (by texture)
/// and draws in the animated buffer.
pub struct Animated {
    rsm: std::rc::Rc<Rsm>,
    placement: model::Matrix,
    mirror: bool,
    /// Its RSW 2.6 flag: its translucent texels all write depth.
    depth_flag: bool,
    anim_type: i32,
    speed: f32,
    start: Option<std::time::Instant>,
    /// RSM 1: its frame and the 16 ms tick it was last stepped at; its ordered triangles' corner colours, baked at
    /// frame 0 (its light isn't redone while it moves).
    frame: i32,
    tick: Option<u64>,
    colors: Vec<[[f32; 4]; 3]>,
    order: Vec<usize>,
    /// The texture, sidedness and vertex count of each run of its ordered triangles, then those runs' draws.
    runs: Vec<(usize, bool, u32)>,
    draws: Vec<ModelDraw>,
}

impl World {
    /// How many RSM 2 instances move on this map.
    pub fn rsm_2_moving(&self) -> usize {
        self.animated.iter().filter(|instance| instance.rsm.version >= (2, 0)).count()
    }

    /// How many RSM 1 instances move on this map.
    pub fn rsm_1_moving(&self) -> usize {
        self.animated.len() - self.rsm_2_moving()
    }

    /// Poses the moving models for `now`, instances of a model at the same frame posed once. RSM 2 runs on its own
    /// clock (started the first time it's drawn); RSM 1 steps its frame once per drawn frame in which a 16 ms tick
    /// passed (`model::rsm1_step`) and keeps its frame-0 light.
    pub fn animate(&mut self, gpu: &Gpu, now: std::time::Instant) {
        let Some(buffer) = &self.animated_buffer else { return };
        let mut vertices = Vec::new();
        let mut posed: HashMap<(*const Rsm, u32), Vec<model::Triangle>> = HashMap::new();
        let tick = crate::whole_ms(now) / 16;
        for instance in &mut self.animated {
            let frame = if instance.rsm.version >= (2, 0) {
                let start = *instance.start.get_or_insert(now);
                let elapsed = now.saturating_duration_since(start).as_millis() as u64;
                let (frame, again) = model::clock(elapsed, instance.speed, instance.anim_type, instance.rsm.frames, instance.rsm.fps);
                if again {
                    instance.start = Some(now);
                }
                frame
            } else {
                if instance.tick.is_some_and(|last| tick > last) {
                    instance.frame = model::rsm1_step(instance.frame, instance.speed, instance.anim_type, instance.rsm.frames);
                }
                instance.tick = Some(tick);
                instance.frame as f32
            };
            let triangles = posed.entry((std::rc::Rc::as_ptr(&instance.rsm), frame.to_bits())).or_insert_with(|| instance.rsm.triangles_at(frame));
            for (place, &triangle) in instance.order.iter().enumerate() {
                if let (Some(triangle), Some(&colors)) = (triangles.get(triangle), instance.colors.get(place)) {
                    vertices.extend(placed_vertices(&instance.placement, colors, triangle, instance.mirror));
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

/// The sun in model space (y down): (sin lat · sin lon, cos lat, sin lat · cos lon), with the map's diffuse and
/// ambient.
#[derive(Clone, Copy)]
struct ModelLight {
    direction: Vector,
    diffuse: [f32; 3],
    ambient: [f32; 3],
}

impl ModelLight {
    fn of(light: &rustnarok_client::map::Light) -> Self {
        let (longitude, latitude) = ((light.longitude as f32).to_radians(), (light.latitude as f32).to_radians());
        let direction = [latitude.sin() * longitude.sin(), latitude.cos(), latitude.sin() * longitude.cos()];
        Self { direction, diffuse: light.diffuse, ambient: light.ambient }
    }
}

/// A triangle's corner colours, baked once: shade 0 the tint; else d = max(0, n′ · L) with n′ the corner's normal under
/// the instance (normalized), c = min(1, d · diffuse + ambient) and trunc(min(255, c · tint)) per channel. A corner
/// turned away gets the ambient only.
fn corner_colors(shade: i32, triangle: &model::Triangle, placement: &model::Matrix, light: ModelLight, tint: [u8; 3]) -> [[f32; 4]; 3] {
    triangle.normals.map(|normal| {
        let rgb: [f32; 3] = std::array::from_fn(|channel| {
            let tint = f32::from(tint[channel]);
            if shade == 0 {
                return tint / 255.0;
            }
            let turned = normalize(model::direction(placement, normal));
            let d = dot(turned, light.direction).max(0.0);
            let c = (d * light.diffuse[channel] + light.ambient[channel]).min(1.0);
            (c * tint).min(255.0).trunc() / 255.0
        });
        [rgb[0], rgb[1], rgb[2], 1.0]
    })
}

/// Whether an instance is mirrored: an odd number of its scale's components is negative.
fn mirrored(scale: [f32; 3]) -> bool {
    scale.iter().filter(|&&value| value < 0.0).count() % 2 == 1
}

/// A triangle's three vertices placed in the world (y up), with its face normal and baked corner colours. A mirrored
/// instance's corners go the other way round, so its one-sided faces still show their front.
fn placed_vertices(placement: &model::Matrix, colors: [[f32; 4]; 3], triangle: &model::Triangle, mirror: bool) -> [GroundVertex; 3] {
    // Native space has Y down; the world has Y up.
    let positions = triangle.positions.map(|position| {
        let [x, y, z] = model::transform(placement, position);
        [x, -y, z]
    });
    let normal = normalize(cross(subtract(positions[1], positions[0]), subtract(positions[2], positions[0])));
    let corners = if mirror { [0, 2, 1] } else { [0, 1, 2] };
    corners.map(|corner| GroundVertex { position: positions[corner], normal, uv: triangle.uvs[corner], lightmap_uv: [0.0; 2], color: colors[corner] })
}

fn model_mesh(grf: &Grf, rsw: &Rsw, gnd: &Gnd, alpha_of: &mut impl FnMut(&str) -> bool) -> ModelMesh {
    let mut texture_index: HashMap<String, usize> = HashMap::new();
    // Per texture, its one-sided and its two-sided faces' vertices.
    let mut per_texture: Vec<[Vec<GroundVertex>; 2]> = Vec::new();
    let mut models: HashMap<&str, Option<LoadedModel>> = HashMap::new();
    // RSW positions are relative to the map's center.
    let half = [gnd.width as f32 * gnd.zoom / 2.0, gnd.height as f32 * gnd.zoom / 2.0];
    let light = ModelLight::of(&rsw.light);
    let mut rsm_2 = 0;
    let mut animated = Vec::new();
    let mut translucent: Vec<Translucent> = Vec::new();

    for instance in &rsw.models {
        let entry = models.entry(instance.file.as_str()).or_insert_with(|| {
            let rsm = Rsm::parse(&grf.read(&format!("data\\model\\{}", instance.file)).ok()?).ok()?;
            let triangles = rsm.triangles();
            Some((std::rc::Rc::new(rsm), triangles))
        });
        let Some((rsm, triangles)) = entry else { continue };
        rsm_2 += usize::from(rsm.version >= (2, 0));

        let placement = placement(instance, half);
        let mirror = mirrored(instance.scale);

        let mut texture_of = |name: &String, per_texture: &mut Vec<[Vec<GroundVertex>; 2]>| {
            *texture_index.entry(name.clone()).or_insert_with(|| {
                per_texture.push([Vec::new(), Vec::new()]);
                per_texture.len() - 1
            })
        };
        // A model that plays (a type and something that moves) is posed again each frame, in a buffer of its own; its
        // light stays as at frame 0.
        if instance.anim_type != 0 && rsm.animated() {
            let textures: Vec<usize> = rsm.textures.iter().map(|name| texture_of(name, &mut per_texture)).collect();
            let key = |triangle: &model::Triangle| (textures[triangle.texture], triangle.two_sided);
            let mut order: Vec<usize> = (0..triangles.len()).collect();
            order.sort_by_key(|&triangle| key(&triangles[triangle]));
            let mut runs: Vec<(usize, bool, u32)> = Vec::new();
            for &triangle in &order {
                let (texture, two_sided) = key(&triangles[triangle]);
                match runs.last_mut() {
                    Some((last, sided, count)) if *last == texture && *sided == two_sided => *count += 3,
                    _ => runs.push((texture, two_sided, 3)),
                }
            }
            let still = if rsm.version >= (2, 0) { rsm.triangles() } else { rsm.triangles_at(0.0) };
            let colors = order
                .iter()
                .filter_map(|&triangle| still.get(triangle))
                .map(|triangle| corner_colors(rsm.shade, triangle, &placement, light, instance.tint))
                .collect();
            animated.push(Animated {
                rsm: rsm.clone(),
                placement,
                mirror,
                depth_flag: instance.depth_flag,
                anim_type: instance.anim_type,
                speed: instance.anim_speed,
                start: None,
                frame: 0,
                tick: None,
                colors,
                order,
                runs,
                draws: Vec::new(),
            });
            continue;
        }
        // Triangles with a translucent texture go in this instance's own batch of that texture.
        let mut own: HashMap<(usize, bool), Vec<GroundVertex>> = HashMap::new();
        for triangle in triangles.iter() {
            let name = &rsm.textures[triangle.texture];
            let texture = texture_of(name, &mut per_texture);
            let colors = corner_colors(rsm.shade, triangle, &placement, light, instance.tint);
            let placed = placed_vertices(&placement, colors, triangle, mirror);
            if alpha_of(name) {
                own.entry((texture, triangle.two_sided)).or_default().extend(placed);
            } else {
                per_texture[texture][usize::from(triangle.two_sided)].extend(placed);
            }
        }
        let mut own: Vec<_> = own.into_iter().collect();
        own.sort_by_key(|(key, _)| *key);
        translucent.extend(own.into_iter().map(|((texture, two_sided), vertices)| (texture, two_sided, instance.depth_flag, vertices)));
    }

    let mut names = vec![String::new(); texture_index.len()];
    for (name, index) in texture_index {
        names[index] = name;
    }
    let mut vertices = Vec::new();
    let mut draws = Vec::new();
    for (texture, sides) in per_texture.into_iter().enumerate() {
        for (two_sided, triangles) in [false, true].into_iter().zip(sides) {
            if triangles.is_empty() {
                continue;
            }
            let start = vertices.len() as u32;
            vertices.extend(triangles);
            draws.push((texture, two_sided, start..vertices.len() as u32));
        }
    }
    (vertices, draws, names, rsm_2, animated, translucent)
}

/// The walk search lives in the library; the binary keeps calling it from here.
pub use rustnarok_client::pathfinding::{find_path, find_unit_path, find_unlimited_path};

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
/// A drawn position (a fractional cell from `Actor::position`, cell centres on whole numbers) as a point on the
/// ground plane in world units: (p + 0.5) × 5 on each axis.
// ponytail: measured from the map's corner; the game measures from its centre, which gives the same cells on even-sized
// maps (all of them) and can differ only by f32 rounding exactly on a cell edge.
pub fn drawn_point(position: [f32; 2]) -> [f32; 2] {
    position.map(|value| (value + 0.5) * CELL)
}

/// The cell a ground point is over.
pub fn floor_cell(point: [f32; 2]) -> [i32; 2] {
    point.map(|value| (value / CELL).floor() as i32)
}

/// How far an attack reaches across between two ground points: the flat distance, then with the difference of the
/// ground heights under them.
pub fn attack_distance(gat: &Gat, from: [f32; 2], to: [f32; 2]) -> f32 {
    let [dx, dz] = [to[0] - from[0], to[1] - from[1]];
    let flat = (dx * dx + dz * dz).sqrt();
    let dy = gat.height_at(to.map(|value| value / CELL)) - gat.height_at(from.map(|value| value / CELL));
    (flat * flat + dy * dy).sqrt()
}

/// In reach to attack at `range` cells: range × 5 + 5.5 world units or less (a NaN is out).
pub fn in_attack_reach(distance: f32, range: i32) -> bool {
    (range * 5) as f32 + 5.5 >= distance
}

/// Where a chase walks: range × 5 world units short of the aim point, on the line from the player's point; off the
/// map or not walkable, the first walkable neighbour (the `pickup_spot` order) of that cell, else the last one tried.
pub fn chase_cell(gat: &Gat, from: [f32; 2], aim: [f32; 2], range: i32) -> [i32; 2] {
    const NEXT: [[i32; 2]; 8] = [[1, 0], [1, 1], [1, -1], [-1, 0], [-1, 1], [-1, -1], [0, 1], [0, -1]];
    let away = [from[0] - aim[0], from[1] - aim[1]];
    let inverse = 1.0 / (away[1] * away[1] + away[0] * away[0]).sqrt();
    let reach = (range * 5) as f32;
    let cell = floor_cell([aim[0] + away[0] * inverse * reach, aim[1] + away[1] * inverse * reach]);
    if gat.walkable(cell) {
        return cell;
    }
    let tried = NEXT.map(|[x, y]| [cell[0] + x, cell[1] + y]);
    tried.into_iter().find(|&next| gat.walkable(next)).unwrap_or(tried[7])
}

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
    fn a_criticals_digits_draw_over_its_burst() {
        // The burst and its digits share an anchor; the digits' one-step nudge puts them later (nearer) in the order,
        // however tall the burst is.
        let at = |nudge: f32| number_depth(400.0, plane(DepthLayer { nudge, ..DepthLayer::default() }, 400.0, 0.8, 300.0));
        assert!(at(1.0) < at(0.0));
        // A number nearer the camera still draws after one further away.
        let plain = plane(DepthLayer::default(), 400.0, 0.8, 300.0);
        assert!(number_depth(300.0, plain) < number_depth(400.0, plain));
    }

    #[test]
    fn chases_to_range_short_of_the_target() {
        let flat = |wall: Option<[i32; 2]>| Gat {
            width: 64,
            height: 64,
            cells: (0..64 * 64).map(|index| ([0.0; 4], u8::from(Some([index % 64, index / 64]) == wall))).collect(),
        };
        let open = flat(None);
        let chase = |gat: &Gat, from: [f32; 2], aim: [f32; 2], range| chase_cell(gat, drawn_point(from), drawn_point(aim), range);
        let target = [40.0, 32.0];
        assert_eq!(chase(&open, [34.0, 32.0], target, 1), [39, 32]);
        assert_eq!(chase(&open, [35.0, 27.0], target, 1), [39, 31]);
        assert_eq!(chase(&open, [34.0, 30.0], target, 3), [37, 31]);
        assert_eq!(chase(&open, [29.0, 25.0], target, 9), [32, 27]);
        // The cell is a wall: its first neighbour, the target's own cell.
        assert_eq!(chase(&flat(Some([39, 32])), [34.0, 32.0], target, 1), [40, 32]);
        // A slow target drawn halfway to (41, 33).
        assert_eq!(chase(&open, [34.0, 32.0], [40.5, 32.5], 1), [40, 32]);
    }

    #[test]
    fn reaches_across_the_drawn_distance_with_height() {
        let gat = |rise: f32| Gat { width: 16, height: 16, cells: (0..256).map(|index| ([if index % 16 >= 3 { rise } else { 0.0 }; 4], 0)).collect() };
        let reach =
            |gat: &Gat, to: [f32; 2], range| in_attack_reach(attack_distance(gat, drawn_point([1.0, 1.0]), drawn_point([1.0 + to[0], 1.0 + to[1]])), range);
        let flat = gat(0.0);
        assert!(reach(&flat, [2.0, 0.0], 1));
        assert!(!reach(&flat, [2.0, 1.0], 1));
        // Two cells straight with the target's ground higher: 3.2 units in, 3.3 out.
        assert!(reach(&gat(-3.2), [2.0, 0.0], 1));
        assert!(!reach(&gat(-3.3), [2.0, 0.0], 1));
        assert!(reach(&flat, [10.0, 0.0], 9) && reach(&flat, [7.0, 7.0], 9) && !reach(&flat, [7.0, 8.0], 9));
        assert!(!in_attack_reach(f32::NAN, 9));
    }

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
            depth_flag: false,
        };
        let matrix = placement(&instance, [100.0, 200.0]);
        let flipped = [2.0, 1.0, 0.0];
        let [x, y, z] = model::transform(&matrix, flipped);
        assert!((x - 110.0).abs() < 1e-3 && (y - 2.0).abs() < 1e-3 && (z - 216.0).abs() < 1e-3, "{x} {y} {z}");
        // In the world (y up) it stands 2 below the native y, as every model does, and takes the instance's tint.
        let triangle = model::Triangle { texture: 0, positions: [flipped; 3], uvs: [[0.0; 2]; 3], normals: [[0.0, 1.0, 0.0]; 3], two_sided: false };
        let light = ModelLight { direction: [0.0, 1.0, 0.0], diffuse: [1.0; 3], ambient: [0.3; 3] };
        let vertex = placed_vertices(&matrix, corner_colors(0, &triangle, &matrix, light, instance.tint), &triangle, false)[0];
        assert_eq!(vertex.position[1], -y);
        assert_eq!(vertex.color, [0x33 as f32 / 255.0, 0x66 as f32 / 255.0, 0x99 as f32 / 255.0, 1.0]);
    }

    #[test]
    fn bakes_model_corners_by_shade_and_light() {
        let light = ModelLight { direction: [0.0, 1.0, 0.0], diffuse: [0.5; 3], ambient: [0.3; 3] };
        let triangle = |normal: Vector| model::Triangle { texture: 0, positions: [[0.0; 3]; 3], uvs: [[0.0; 2]; 3], normals: [normal; 3], two_sided: false };
        let byte = |colors: [[f32; 4]; 3]| (colors[0][0] * 255.0).round() as u8;
        // Shade 0: the tint, no light.
        assert_eq!(byte(corner_colors(0, &triangle([0.0, -1.0, 0.0]), &model::IDENTITY, light, [200; 3])), 200);
        // Facing the light: min(1, 0.5 + 0.3) × 255 = 204; turned away: the ambient only, trunc(0.3 × 255) = 76.
        assert_eq!(byte(corner_colors(1, &triangle([0.0, 2.0, 0.0]), &model::IDENTITY, light, [255; 3])), 204);
        assert_eq!(byte(corner_colors(2, &triangle([0.0, -1.0, 0.0]), &model::IDENTITY, light, [255; 3])), 76);
        // The tint scales it: trunc(0.8 × 100) = 80.
        assert_eq!(byte(corner_colors(1, &triangle([0.0, 1.0, 0.0]), &model::IDENTITY, light, [100; 3])), 80);
        // Mirrored by an odd count of negative scales; its corners go the other way round.
        assert!(mirrored([-1.0, 1.0, 1.0]) && !mirrored([-1.0, -1.0, 1.0]) && !mirrored([1.0; 3]) && mirrored([-1.0; 3]));
        let mut corners = triangle([0.0, 1.0, 0.0]);
        corners.positions = [[0.0; 3], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]];
        let colors = [[0.0; 4]; 3];
        let ([_, plain, _], [_, flipped, _]) =
            (placed_vertices(&model::IDENTITY, colors, &corners, false), placed_vertices(&model::IDENTITY, colors, &corners, true));
        assert_eq!((plain.position, flipped.position), ([1.0, 0.0, 0.0], [0.0, 0.0, 1.0]));
    }

    #[test]
    fn blends_textures_with_their_own_alpha_after_the_sprites() {
        // Only exactly #FF00FF is keyed, whatever its alpha.
        let keyed =
            key_out_magenta(RgbaImage::from_fn(3, 1, |x, _| Rgba([[0xFF, 0x00, 0xFF, 0x80], [0xFE, 0x00, 0xFE, 0xFF], [0xFF, 0x03, 0xFF, 0xFF]][x as usize])));
        assert_eq!(keyed.pixels().map(|pixel| pixel.0[3]).collect::<Vec<_>>(), [0, 0xFF, 0xFF]);
        // Translucent when the image has an alpha channel of its own (a TGA), opaque otherwise (a BMP).
        assert!(image::DynamicImage::new_rgba8(1, 1).color().has_alpha() && !image::DynamicImage::new_rgb8(1, 1).color().has_alpha());
        // The passes by the translucency byte and the 2.6 flag.
        assert_eq!(blend_passes(true, false), [BlendPass::Solid, BlendPass::Soft]);
        assert_eq!(blend_passes(true, true), [BlendPass::AnyDepth]);
        assert_eq!(blend_passes(false, false), [BlendPass::Any]);
        assert_eq!(blend_passes(false, true), [BlendPass::AnyDepth]);
        assert_eq!([BlendPass::Solid, BlendPass::Soft].map(|pass| pass.look().1), [true, false]);
        // Back to front by depth.
        let mut depths = vec![3.0, 9.0, 1.0, 5.0];
        back_to_front(&mut depths, |depth| *depth);
        assert_eq!(depths, [9.0, 5.0, 3.0, 1.0]);
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
    fn steps_face_their_direction() {
        assert_eq!(direction([0, 1]), 4);
        assert_eq!(direction([1, -1]), 7);
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
    fn sprites_tilt_their_depth_toward_the_camera() {
        let pitch = 45f32.to_radians();
        // The view depth a depth buffer value stands for, and a pixel's at `below` screen pixels down from feet as far
        // away as the camera.
        let view_depth = |value: f32| NEAR / (1.0 - value / DEPTH_SCALE);
        let at = |distance: f32, layer: DepthLayer, below: f32| view_depth(sprite_depth(distance, below, plane(layer, distance, pitch, distance)));
        let near = |a: f32, b: f32| (a - b).abs() < 0.05;
        let standing = DepthLayer::default();
        assert!((plane(standing, 300.0, pitch, 300.0)[0] - 0.1).abs() < 1e-6);
        // Default camera: the feet about 1.1 cells nearer than they are, the top 0.1 nearer per pixel up.
        let (top, feet, bottom) = (at(300.0, standing, -100.0), at(300.0, standing, 0.0), at(300.0, standing, 20.0));
        assert!(near(feet, 294.44) && near(top, 284.80) && near(bottom, 296.36), "{top} {feet} {bottom}");
        // Far camera: less bias.
        let (top, feet) = (at(400.0, standing, -100.0), at(400.0, standing, 0.0));
        assert!(near(feet, 392.58) && near(top, 382.94), "{top} {feet}");
        // Each image of a monster sits a layer nearer; a player's share the body's plane, a step nearer each.
        assert!(at(300.0, DepthLayer { index: 1.0, ..standing }, 0.0) < feet - 0.5);
        let player = |nudge: f32, below: f32| sprite_depth(300.0, below, plane(DepthLayer { nudge, ..standing }, 300.0, pitch, 300.0));
        for below in [-80.0, 0.0, 10.0] {
            assert!(((player(0.0, below) - player(3.0, below)) - 3e-6).abs() < 2e-7);
        }
        // The dead lie flat: their top counts as farther.
        let flat = DepthLayer { flat: true, ..standing };
        assert!(near(at(300.0, flat, 0.0), 294.44) && near(at(300.0, flat, -100.0), 304.06), "{}", at(300.0, flat, -100.0));
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
        // A pixel right of the middle picks the target's cell on flat ground. The middle column itself is a ray parallel
        // to x at turn 0, and like the game's it picks nothing there for this cell.
        let flat = Gat { width: 40, height: 40, cells: vec![([0.0; 4], 0); 1600] };
        assert_eq!(view.pick_ground([0.5 + 1.0 / 1024.0, 0.5], &flat), Some([20, 20]));
        assert_eq!(view.pick_ground([0.5, 0.5], &flat), None);
    }

    #[test]
    fn picks_the_ground_drawn_under_the_pointer() {
        let flat = |width: usize, height: usize| Gat { width, height, cells: vec![([0.0; 4], 0); width * height] };
        // 20 × 20, cell (10, 10) raised 10: a ray that reaches its rising face passes under it.
        let mut raised = flat(20, 20);
        raised.cells[10 * 20 + 10].0 = [-10.0; 4];
        let slant = [0.05, -1.0, 1.0];
        assert_eq!(ground_hit(&raised, [51.0, 33.0, 20.0], slant), None);
        assert_eq!(ground_hit(&raised, [51.0, 37.0, 20.0], slant), Some([10, 11]));
        assert_eq!(ground_hit(&flat(20, 20), [51.0, 33.0, 20.0], slant), Some([10, 10]));
        assert_eq!(ground_hit(&flat(20, 20), [51.0, 37.0, 20.0], slant), Some([10, 11]));
        // From outside the map: the row y = 0 raised 40, hit from beyond the south edge.
        let mut wall = flat(20, 20);
        for x in 0..20 {
            wall.cells[x].0 = [-40.0; 4];
        }
        assert_eq!(ground_hit(&wall, [52.5, 60.0, -30.0], slant), Some([11, 6]));
        // A ray parallel to x: only the middle 70 % of each quadtree box passes.
        let picked: Vec<i32> = (0..20).filter(|&x| ground_hit(&flat(20, 20), [x as f32 * 5.0 + 2.5, 30.0, 22.0], [0.0, -1.0, 1.0]) == Some([x, 10])).collect();
        assert_eq!(picked, [1, 3, 6, 11, 16]);
        assert!((0..20).all(|x| ground_hit(&flat(20, 20), [x as f32 * 5.0 + 2.5, 30.0, 22.0], [0.01, -1.0, 1.0]) == Some([x, 10])));
        // An odd half width lists one cell west of the box.
        assert_eq!(ground_hit(&flat(42, 40), [1.25, 30.0, 22.0], [0.3, -1.0, 1.0]), None);
        assert_eq!(ground_hit(&flat(40, 40), [1.25, 30.0, 22.0], [0.3, -1.0, 1.0]), Some([2, 10]));
    }

    #[test]
    fn the_quadtree_picks_as_testing_every_cell() {
        let gat = Gat { width: 40, height: 40, cells: vec![([0.0; 4], 0); 1600] };
        let half = [20, 20];
        let mut seed = 12345u32;
        let mut random = || {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
            (seed >> 8) as f32 / (1u32 << 24) as f32
        };
        for _ in 0..500 {
            let eye = [random() * 200.0, 20.0 + random() * 60.0, random() * 200.0];
            let direction = normalize([random() - 0.5, -0.3 - random(), random() - 0.5]);
            let origin = [eye[0] - 100.0, -eye[1], eye[2] - 100.0];
            let flipped = [direction[0], -direction[1], direction[2]];
            let (mut best, mut every) = (PICK_FAR, None);
            for cy in 0..40 {
                for cx in 0..40 {
                    let t = cell_t(&gat, half, [cx, cy], origin, flipped);
                    if best > t {
                        (best, every) = (t, Some([cx, cy]));
                    }
                }
            }
            assert_eq!(ground_hit(&gat, eye, direction), every.filter(|_| best < PICK_FAR), "{eye:?} {direction:?}");
        }
    }

    #[test]
    fn a_cell_is_two_flat_triangles() {
        // Corners (SW, SE, NW, NE) = (0, 0, 0, −10): flat in the first triangle, 5 up at three quarters across both ways.
        let gat = Gat { width: 2, height: 2, cells: vec![([0.0, 0.0, 0.0, -10.0], 0); 4] };
        let height_at = |fx: f32, fz: f32| {
            let origin = [(fx - 1.0) * 5.0, -100.0, (fz - 1.0) * 5.0];
            100.0 - cell_t(&gat, [1, 1], [0, 0], origin, [0.0, 1.0, 0.0])
        };
        assert!(height_at(0.25, 0.25).abs() < 1e-4);
        assert!((height_at(0.75, 0.75) - 5.0).abs() < 1e-4);
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
            water: None,
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

    #[test]
    fn parses_a_fog_table_and_cuts_an_instance_prefix() {
        let text = "
            // skipped
            prontera.rsw#
            0.23#
            0.9#
            0xFFFFFFFF#
            0.4#
            hu_fild01.rsw#
            0.2#
            0.6#
            0xFFDDD7CC#
            0#
            mag_dun01.rsw#
            0.0#
            0.5#
            0xff89003#
            1.5#
            1@tower.rsw#
            0.1#
            0.2#
            0xFF000000#
            0#
        ";
        let table = parse_fog_table(text);
        let prontera = table["prontera.rsw"];
        assert_eq!((prontera.near, prontera.far, prontera.color, prontera.density), (0.23, 0.9, 0xFFFF_FFFF, 0.4));
        assert_eq!(table["mag_dun01.rsw"].color, 0x0FF8_9003);
        assert!(!table.contains_key("izlude.rsw"));
        assert_eq!(instance_map("0001@tower"), "1@tower");
        assert_eq!(instance_map("prontera"), "prontera");
        assert_eq!(instance_map("6@tower"), "6@tower");
        assert_eq!(fog_for(&table, "0001@tower").unwrap().color, 0xFF00_0000);
    }

    #[test]
    fn fog_runs_from_the_start_depth_to_the_end() {
        // hu_fild01: near 0.2, far 0.6 → depth 308 to 904.
        let (start, end) = (10.0 + 1490.0 * 0.2, 10.0 + 1490.0 * 0.6);
        assert!((start - 308.0_f32).abs() < 1e-3 && (end - 904.0_f32).abs() < 1e-3, "{start} {end}");
        assert_eq!(fog_amount(0.2, 0.6, start), 0.0);
        assert!((fog_amount(0.2, 0.6, (start + end) / 2.0) - 0.5).abs() < 1e-5);
        assert_eq!(fog_amount(0.2, 0.6, end), 1.0);
        assert_eq!(fog_amount(0.2, 0.6, 0.0), 0.0);
        assert_eq!(fog_amount(0.2, 0.6, 2000.0), 1.0);
        let white = [1.0; 3];
        let ground = [0.0, 0.5, 1.0];
        assert_eq!(fogged(ground, white, 0.0), ground);
        assert_eq!(fogged(ground, white, 1.0), white);
        assert_eq!(fogged(ground, white, 0.5), [0.5, 0.75, 1.0]);
    }

    fn bgra(r: u8, g: u8, b: u8) -> [u8; 4] {
        [b, g, r, 255]
    }

    fn flat_surface(color: [u8; 4]) -> Surface {
        Surface { u: [0.0; 4], v: [0.0; 4], texture: 0, lightmap: 0, color }
    }

    fn plot(width: usize, height: usize, cubes: Vec<Cube>, surfaces: Vec<Surface>) -> Gnd {
        Gnd {
            width,
            height,
            zoom: 10.0,
            textures: vec!["ground.bmp".into()],
            lightmap_size: [8, 8],
            lightmaps: vec![vec![255; 256]],
            surfaces,
            cubes,
            water: None,
        }
    }

    fn baked_mesh(gnd: &Gnd, light: Vector, ambient: [f32; 3], diffuse: [f32; 3]) -> Vec<GroundVertex> {
        ground_mesh(gnd, 1, [8.0, 8.0], light, ambient, diffuse).0
    }

    #[test]
    fn light_comes_from_the_south_west_at_45_and_not_the_mirror_at_70() {
        near(light_direction(45.0, 45.0), [-0.5, 2.0_f32.sqrt() / 2.0, -0.5]);
        // izlude is longitude 70, latitude 20. The old vector swapped sin and cos of the longitude.
        near(light_direction(70.0, 20.0), [-0.3213938, 0.9396926, -0.1169778]);
    }

    #[test]
    fn a_top_folds_along_the_south_east_to_north_west_diagonal() {
        let gnd = plot(1, 1, vec![Cube { heights: [0.0; 4], top: 0, north: -1, east: -1 }], vec![flat_surface([255; 4])]);
        let positions: Vec<_> = baked_mesh(&gnd, [0.0, 1.0, 0.0], [1.0; 3], [0.0; 3]).into_iter().map(|vertex| vertex.position).collect::<Vec<_>>();
        assert_eq!(positions.as_slice(), [[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 0.0, 10.0], [0.0, 0.0, 10.0], [10.0, 0.0, 0.0], [10.0, 0.0, 10.0]]);
    }

    #[test]
    fn a_corner_takes_the_top_colour_of_the_tile_that_starts_there() {
        let surfaces = vec![flat_surface(bgra(255, 0, 0)), flat_surface(bgra(0, 255, 0)), flat_surface(bgra(0, 0, 255)), flat_surface(bgra(255, 255, 0))];
        let tile = |top, north, east| Cube { heights: [0.0; 4], top, north, east };
        // (0,0) red, with yellow wall surfaces that must not colour the walls. (1,0) green, (0,1) blue, (1,1) no top.
        let cubes = vec![tile(0, 3, 3), tile(1, -1, -1), tile(2, -1, -1), tile(-1, -1, -1)];
        let vertices = baked_mesh(&plot(2, 2, cubes, surfaces), [0.0, 1.0, 0.0], [1.0; 3], [0.0; 3]);
        let rgb = |index: usize| vertices[index].color.map(|channel| (channel * 255.0).round() as u8);
        assert_eq!((rgb(0), rgb(1), rgb(2), rgb(5)), ([255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255], [255, 255, 255, 255]));
        // North wall: the tile to the north, and the one past its east corner (white, it has no top).
        assert_eq!((rgb(6), rgb(7), rgb(8)), ([0, 0, 255, 255], [255, 255, 255, 255], [0, 0, 255, 255]));
        // East wall: white from the missing top, green from the east tile.
        assert_eq!((rgb(12), rgb(13)), ([255, 255, 255, 255], [0, 255, 0, 255]));
    }

    #[test]
    fn equal_heights_share_a_normal_and_a_cliff_does_not() {
        let flat = Cube { heights: [0.0; 4], top: 0, north: -1, east: -1 };
        let slope = Cube { heights: [0.0, 0.0, -4.0, -4.0], top: 0, north: -1, east: -1 };
        let vertices = baked_mesh(&plot(2, 1, vec![flat, slope], vec![flat_surface([255; 4])]), [0.0, 1.0, 0.0], [1.0; 3], [0.0; 3]);
        near(vertices[0].normal, [0.0, 1.0, 0.0]);
        assert!(vertices[1].normal[2] < -0.05, "{:?}", vertices[1].normal);
        let cliff = Cube { heights: [5.0; 4], top: 0, north: -1, east: -1 };
        let vertices = baked_mesh(
            &plot(2, 1, vec![Cube { heights: [0.0; 4], top: 0, north: -1, east: -1 }, cliff], vec![flat_surface([255; 4])]),
            [0.0, 1.0, 0.0],
            [1.0; 3],
            [0.0; 3],
        );
        near(vertices[1].normal, [0.0, 1.0, 0.0]);
    }

    #[test]
    fn a_wall_faces_the_lower_side() {
        let facing = |north_height: f32| {
            let cubes = vec![Cube { heights: [0.0; 4], top: 0, north: 0, east: -1 }, Cube { heights: [north_height; 4], top: 0, north: -1, east: -1 }];
            baked_mesh(&plot(1, 2, cubes, vec![flat_surface([255; 4])]), [0.0, 1.0, 0.0], [1.0; 3], [0.0; 3])[6].normal
        };
        // File height 5 is lower than 0, so the wall faces north. A higher neighbor faces it south.
        assert_eq!(facing(5.0), [0.0, 0.0, 1.0]);
        assert_eq!(facing(-3.0), [0.0, 0.0, -1.0]);
        // The east wall compares the neighbor's south-west with this tile's north-west, not its south-east (50).
        let cubes =
            vec![Cube { heights: [0.0, 50.0, 0.0, 0.0], top: 0, north: -1, east: 0 }, Cube { heights: [4.0, 0.0, 0.0, 0.0], top: 0, north: -1, east: -1 }];
        let normal = baked_mesh(&plot(2, 1, cubes, vec![flat_surface([255; 4])]), [0.0, 1.0, 0.0], [1.0; 3], [0.0; 3])[6].normal;
        assert_eq!(normal, [1.0, 0.0, 0.0]);
    }

    #[test]
    fn the_sun_is_baked_into_the_vertex_colour() {
        let gnd = plot(1, 1, vec![Cube { heights: [0.0; 4], top: 0, north: -1, east: -1 }], vec![flat_surface(bgra(200, 200, 200))]);
        let vertices = baked_mesh(&gnd, [0.0, 1.0, 0.0], [0.25; 3], [0.25; 3]);
        near([vertices[0].color[0], vertices[0].color[1], vertices[0].color[2]], [100.0 / 255.0; 3]);
        // At 45/45 a north or east wall gets the ambient, and a south or west wall ambient plus half the diffuse.
        let sun = light_direction(45.0, 45.0);
        let baked = |normal: Vector| {
            let color = baked_color([200, 200, 200], normal, sun, [0.25; 3], [0.5; 3]);
            [color[0], color[1], color[2]]
        };
        near(baked([0.0, 0.0, 1.0]), [50.0 / 255.0; 3]);
        near(baked([1.0, 0.0, 0.0]), [50.0 / 255.0; 3]);
        near(baked([0.0, 0.0, -1.0]), [100.0 / 255.0; 3]);
        near(baked([-1.0, 0.0, 0.0]), [100.0 / 255.0; 3]);
    }

    fn close(have: [f32; 3], want: [f32; 3]) {
        for channel in 0..3 {
            assert!((have[channel] - want[channel]).abs() < 1e-4, "{have:?} != {want:?}");
        }
    }

    #[test]
    fn scene_tint_follows_the_diffuse_and_the_ambient() {
        // prontera: white diffuse, tint 1 whatever the ambient.
        close(scene_tint([1.0, 1.0, 1.0], [0.55, 0.5, 0.5]), [1.0, 1.0, 1.0]);
        // izlude and mag_dun01's diffuses. With no ambient the tint is the diffuse.
        close(scene_tint([0.993, 0.979, 1.0], [0.0; 3]), [0.993, 0.979, 1.0]);
        close(scene_tint([0.79, 0.60, 0.52], [0.0; 3]), [0.79, 0.60, 0.52]);
        // Dusk ends: L's R and G at 0.5, B still the diffuse. prontera's ambient, then ambient 0.3.
        close(scene_tint([0.5, 0.5, 1.0], [0.55, 0.5, 0.5]), [0.775, 0.75, 1.0]);
        close(scene_tint([0.5, 0.5, 1.0], [0.3; 3]), [0.65, 0.65, 1.0]);
        assert_eq!(tint_milli(scene_tint([0.5, 0.5, 1.0], [0.55, 0.5, 0.5])), [775, 750, 1000]);
        assert_eq!(tint_milli(scene_tint([0.5, 0.5, 1.0], [0.3; 3])), [650, 650, 1000]);
    }

    #[test]
    fn dusk_steps_toward_half_and_back_to_the_target() {
        let mut light = SceneLight::load([0.55, 0.5, 0.5], [1.0, 1.0, 1.0]);
        light.note(true, 160, true);
        light.step();
        close(light.color(), [0.995, 0.995, 1.0]);
        for _ in 0..99 {
            light.step();
        }
        close(light.color(), [0.5, 0.5, 1.0]);
        light.step();
        close(light.color(), [0.5, 0.5, 1.0]);
        light.note(true, 160, false);
        light.step();
        close(light.color(), [0.505, 0.505, 1.0]);
        for _ in 0..200 {
            light.step();
        }
        close(light.color(), [1.0, 1.0, 1.0]);
        // A dimmer diffuse is the cap: nothing rises past it.
        let mut dim = SceneLight::load([0.0; 3], [0.79, 0.60, 0.52]);
        dim.note(true, 149, true);
        for _ in 0..80 {
            dim.step();
        }
        close(dim.color(), [0.5, 0.5, 0.52]);
        dim.note(true, 149, false);
        for _ in 0..200 {
            dim.step();
        }
        close(dim.color(), [0.79, 0.60, 0.52]);
    }

    #[test]
    fn dusk_statuses_are_the_players_and_a_map_load_starts_again() {
        for status in [149, 160, 1056, 1057, 1058, 1059] {
            assert!(is_dusk_status(status), "{status}");
        }
        assert!(!is_dusk_status(148) && !is_dusk_status(161) && !is_dusk_status(1055) && !is_dusk_status(1060));
        let mut light = SceneLight::load([0.55, 0.5, 0.5], [1.0, 1.0, 1.0]);
        light.note(false, 149, true);
        assert!(!light.dusk());
        light.note(true, 149, true);
        light.note(true, 1056, true);
        light.note(true, 149, false);
        assert!(light.dusk());
        light.note(true, 1056, false);
        assert!(!light.dusk());
        light.note(true, 160, true);
        for _ in 0..40 {
            light.step();
        }
        assert!(light.color()[0] < 1.0);
        light.reload([0.3, 0.3, 0.3], [0.8, 0.9, 0.7]);
        close(light.color(), [0.8, 0.9, 0.7]);
        assert!(light.dusk());
        light.step();
        assert!(light.color()[0] < 0.8 && light.color()[1] < 0.9 && light.color()[2] == 0.7);
    }

    #[test]
    fn a_blind_player_shades_models_with_the_ambient() {
        let tint = scene_tint([0.5, 0.5, 1.0], [0.55, 0.5, 0.5]);
        let ambient = [0.55, 0.5, 0.5];
        close(model_shade(tint, ambient, false), tint);
        close(model_shade(tint, ambient, true), ambient);
    }
}
