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
const FIELD_OF_VIEW: f32 = 15.0;
const NEAR: f32 = 10.0;
const FAR: f32 = 4000.0;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

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
}

/// A frame's camera: view-projection plus the basis billboards and picking need.
pub struct View {
    pub view_projection: Matrix,
    pub eye: Vector,
    pub forward: Vector,
    pub right: Vector,
    pub up: Vector,
    pub aspect: f32,
}

impl Camera {
    pub fn view(&self, aspect: f32) -> View {
        let horizontal = self.distance * self.pitch.cos();
        let eye =
            [self.target[0] - horizontal * self.yaw.sin(), self.target[1] + self.distance * self.pitch.sin(), self.target[2] - horizontal * self.yaw.cos()];
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
        let depth = FAR / (FAR - NEAR);
        let projection = [[focal / aspect, 0.0, 0.0, 0.0], [0.0, focal, 0.0, 0.0], [0.0, 0.0, depth, 1.0], [0.0, 0.0, -NEAR * depth, 0.0]];
        View { view_projection: multiply(&projection, &view), eye, forward, right, up, aspect }
    }
}

impl View {
    /// Where the ray through a point on screen (`0..1` each axis, y down) meets the horizontal plane at `height`.
    pub fn pick(&self, screen: [f32; 2], height: f32) -> Option<Vector> {
        let tangent = (FIELD_OF_VIEW.to_radians() / 2.0).tan();
        let x = (screen[0] * 2.0 - 1.0) * tangent * self.aspect;
        let y = (1.0 - screen[1] * 2.0) * tangent;
        let direction = [0, 1, 2].map(|axis| self.forward[axis] + self.right[axis] * x + self.up[axis] * y);
        let distance = (height - self.eye[1]) / direction[1];
        (distance > 0.0).then(|| [0, 1, 2].map(|axis| self.eye[axis] + direction[axis] * distance))
    }

    /// Screen rectangle (`0..1`, y down: left, top, right, bottom) of a sprite-pixel box anchored at `feet`.
    pub fn sprite_rect(&self, feet: Vector, top_left: [f32; 2], bottom_right: [f32; 2]) -> Option<[f32; 4]> {
        let corner = |[x, y]: [f32; 2]| self.project([0, 1, 2].map(|axis| feet[axis] + self.right[axis] * x * SPRITE_SCALE - self.up[axis] * y * SPRITE_SCALE));
        let (a, b) = (corner(top_left)?, corner(bottom_right)?);
        Some([a[0].min(b[0]), a[1].min(b[1]), a[0].max(b[0]), a[1].max(b[1])])
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

const SPRITE_ATTRIBUTES: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32x2, 3 => Float32x4];
const MESH_ATTRIBUTES: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32x4];

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct SpriteVertex {
    feet: [f32; 3],
    offset: [f32; 2],
    uv: [f32; 2],
    tint: [f32; 4],
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
}

const SHADER: &str = r"
struct Globals {
    view_projection: mat4x4<f32>,
    light_direction: vec4<f32>,
    ambient: vec4<f32>,
    diffuse: vec4<f32>,
    camera_right: vec4<f32>,
    camera_up: vec4<f32>,
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
    return vec4(texel.rgb * light, 1.0);
}

struct SpriteOutput { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32>, @location(1) tint: vec4<f32> };

@vertex fn sprite_vs(@location(0) feet: vec3<f32>, @location(1) offset: vec2<f32>, @location(2) uv: vec2<f32>,
                     @location(3) tint: vec4<f32>) -> SpriteOutput {
    return sprite_vertex(feet, offset, uv, tint);
}

fn sprite_vertex(feet: vec3<f32>, offset: vec2<f32>, uv: vec2<f32>, tint: vec4<f32>) -> SpriteOutput {
    let scale = globals.camera_up.w;
    let world = feet + globals.camera_right.xyz * offset.x * scale - globals.camera_up.xyz * offset.y * scale;
    var clip = globals.view_projection * vec4(world, 1.0);
    // The whole sprite takes its feet's depth, pulled slightly forward so it stands on the ground.
    let feet_clip = globals.view_projection * vec4(feet, 1.0);
    clip.z = (feet_clip.z / feet_clip.w - 0.0005) * clip.w;
    var out: SpriteOutput;
    out.position = clip;
    out.uv = uv;
    out.tint = tint;
    return out;
}

@fragment fn sprite_fs(in: SpriteOutput) -> @location(0) vec4<f32> {
    let texel = textureSample(image, image_sampler, in.uv);
    // Shadows and RGBA sprite images are translucent; palette sprites are fully opaque or clear.
    if (texel.a < 0.02) { discard; }
    return texel * in.tint;
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
    var out = sprite_vertex(feet, offset, uv, tint);
    out.position.z = max(out.position.z - 0.01 * out.position.w, 0.0);
    return out;
}
";

/// A sprite image in the world: texture, feet position, top-left offset from the feet and size in sprite pixels
/// (negative width = mirrored), and a color to multiply it by (damage colors, fading).
pub type Billboard<'a> = (&'a Texture, Vector, [f32; 2], [f32; 2], [f32; 4]);

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

        let (model_vertices, model_draws, model_texture_names) = model_mesh(grf, &rsw, &gnd);
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
        let sprite_pipeline = pipeline(
            "sprite",
            "sprite_fs",
            wgpu::VertexBufferLayout {
                array_stride: size_of::<SpriteVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32x2, 3 => Float32x4],
            },
            Some(wgpu::BlendState::ALPHA_BLENDING),
            true,
        );
        let sprite_buffer_layout = || wgpu::VertexBufferLayout {
            array_stride: size_of::<SpriteVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &SPRITE_ATTRIBUTES,
        };
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

        Ok(Self {
            gat,
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
            effect_pipelines,
            mesh_pipelines,
            depth: RefCell::new(None),
        })
    }

    /// World position of a (fractional) cell's center on the ground.
    pub fn cell_position(&self, cell: [f32; 2]) -> Vector {
        let height = self.gat.height([cell[0].round() as i32, cell[1].round() as i32]);
        [(cell[0] + 0.5) * CELL, -height, (cell[1] + 0.5) * CELL]
    }
}

/// Everything one frame of the world needs.
pub struct WorldFrame<'a> {
    pub world: &'a World,
    pub view: &'a View,
    pub billboards: &'a [Billboard<'a>],
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

        let sprite_vertices: Vec<SpriteVertex> = self
            .billboards
            .iter()
            .flat_map(|&(_, feet, [x, y], [w, h], tint)| {
                let corner = |dx: f32, dy: f32| SpriteVertex { feet, offset: [x + dx * w, y + dy * h], uv: [dx, dy], tint };
                [corner(0., 0.), corner(1., 0.), corner(0., 1.), corner(0., 1.), corner(1., 0.), corner(1., 1.)]
            })
            .collect();
        let effect_vertices: Vec<SpriteVertex> = self
            .effects
            .iter()
            .flat_map(|&(_, feet, corners, tint, _)| {
                let corner = |index: usize, uv: [f32; 2]| SpriteVertex { feet, offset: corners[index], uv, tint };
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
            pass.set_pipeline(&world.sprite_pipeline);
            pass.set_vertex_buffer(0, buffer.slice(..));
            for (index, (texture, ..)) in self.billboards.iter().enumerate() {
                let first = index as u32 * 6;
                pass.set_bind_group(1, texture.bind_group(), &[]);
                pass.draw(first..first + 6, 0..1);
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
type ModelMesh = (Vec<GroundVertex>, Vec<(usize, Range<u32>)>, Vec<String>);

fn model_mesh(grf: &Grf, rsw: &Rsw, gnd: &Gnd) -> ModelMesh {
    let mut texture_index: HashMap<String, usize> = HashMap::new();
    let mut per_texture: Vec<Vec<GroundVertex>> = Vec::new();
    let mut models: HashMap<&str, Option<(Rsm, Vec<model::Triangle>)>> = HashMap::new();
    // RSW positions are relative to the map's center.
    let half = [gnd.width as f32 * gnd.zoom / 2.0, gnd.height as f32 * gnd.zoom / 2.0];

    for instance in &rsw.models {
        let entry = models.entry(instance.file.as_str()).or_insert_with(|| {
            let rsm = Rsm::parse(&grf.read(&format!("data\\model\\{}", instance.file)).ok()?).ok()?;
            let triangles = rsm.triangles();
            Some((rsm, triangles))
        });
        let Some((rsm, triangles)) = entry else { continue };

        let [rx, ry, rz] = instance.rotation.map(f32::to_radians);
        let placement = [
            model::translation([instance.position[0] + half[0], instance.position[1], instance.position[2] + half[1]]),
            model::rotation([0.0, 0.0, 1.0], rz),
            model::rotation([1.0, 0.0, 0.0], rx),
            model::rotation([0.0, 1.0, 0.0], ry),
            model::scaling(instance.scale),
        ]
        .iter()
        .fold(model::IDENTITY, |matrix, next| model::multiply(&matrix, next));

        for triangle in triangles.iter() {
            let name = &rsm.textures[triangle.texture];
            let texture = *texture_index.entry(name.clone()).or_insert_with(|| {
                per_texture.push(Vec::new());
                per_texture.len() - 1
            });
            // Native space has Y down; the world has Y up.
            let positions = triangle.positions.map(|position| {
                let [x, y, z] = model::transform(&placement, position);
                [x, -y, z]
            });
            let normal = normalize(cross(subtract(positions[1], positions[0]), subtract(positions[2], positions[0])));
            for (position, uv) in positions.into_iter().zip(triangle.uvs) {
                per_texture[texture].push(GroundVertex { position, normal, uv, lightmap_uv: [0.0; 2], color: [1.0; 4] });
            }
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
    (vertices, draws, names)
}

/// Cells walked from `from` to `to` (excluding `from`), 8-directional; diagonals need both sides open, as on the server.
pub fn find_path(gat: &Gat, from: [i32; 2], to: [i32; 2]) -> Option<Vec<[i32; 2]>> {
    use std::cmp::Reverse;
    use std::collections::{BinaryHeap, HashMap};

    if from == to {
        return Some(Vec::new());
    }
    if !gat.walkable(to) {
        return None;
    }
    let estimate = |cell: [i32; 2]| {
        let (dx, dy) = ((cell[0] - to[0]).abs(), (cell[1] - to[1]).abs());
        10 * dx.max(dy) + 4 * dx.min(dy)
    };
    let mut open = BinaryHeap::from([Reverse((estimate(from), 0, from))]);
    let mut came_from: HashMap<[i32; 2], [i32; 2]> = HashMap::new();
    let mut cost: HashMap<[i32; 2], i32> = HashMap::from([(from, 0)]);
    // rathena caps searches too; this keeps a click across the map from stalling a frame.
    let mut budget = 20_000;
    while let Some(Reverse((_, spent, cell))) = open.pop() {
        if cell == to {
            let mut path = vec![to];
            while let Some(&previous) = came_from.get(path.last().unwrap()) {
                if previous == from {
                    break;
                }
                path.push(previous);
            }
            path.reverse();
            return Some(path);
        }
        budget -= 1;
        if budget == 0 || spent > cost[&cell] {
            if budget == 0 {
                return None;
            }
            continue;
        }
        for dx in -1..=1 {
            for dy in -1..=1 {
                let next = [cell[0] + dx, cell[1] + dy];
                let diagonal = dx != 0 && dy != 0;
                if (dx == 0 && dy == 0)
                    || !gat.walkable(next)
                    || (diagonal && !(gat.walkable([cell[0] + dx, cell[1]]) && gat.walkable([cell[0], cell[1] + dy])))
                {
                    continue;
                }
                let next_cost = spent + if diagonal { 14 } else { 10 };
                if cost.get(&next).is_none_or(|&known| next_cost < known) {
                    cost.insert(next, next_cost);
                    came_from.insert(next, cell);
                    open.push(Reverse((next_cost + estimate(next), next_cost, next)));
                }
            }
        }
    }
    None
}

/// Whether a player at `from` can hit `to` with `range` (rathena's `check_distance_client`: circular, and a cell longer on straight lines).
pub fn in_reach(from: [i32; 2], to: [i32; 2], range: i32) -> bool {
    let (dx, dy) = ((from[0] - to[0]) as f32, (from[1] - to[1]) as f32);
    ((dx * dx + dy * dy).sqrt() - 0.1) as i32 <= range
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
    fn camera_projects_its_target_to_the_center() {
        let camera = Camera { target: [100.0, 0.0, 100.0], distance: 200.0, pitch: 0.8, yaw: 0.0 };
        let view = camera.view(4.0 / 3.0);
        let center = view.project(camera.target).unwrap();
        assert!((center[0] - 0.5).abs() < 1e-4 && (center[1] - 0.5).abs() < 1e-4, "{center:?}");
        // East is to the right, north is up the screen.
        assert!(view.project([110.0, 0.0, 100.0]).unwrap()[0] > 0.5);
        assert!(view.project([100.0, 0.0, 110.0]).unwrap()[1] < 0.5);
        let picked = view.pick([0.5, 0.5], 0.0).unwrap();
        assert!((picked[0] - 100.0).abs() < 1e-2 && (picked[2] - 100.0).abs() < 1e-2, "{picked:?}");
    }
}
