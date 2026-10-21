//! 2D drawing on wgpu: textured rectangles in logical pixels, in order.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::Arc;

use image::RgbaImage;
use rustnarok_client::grf::Grf;
use rustnarok_client::text::{self, Font};
use wgpu::util::DeviceExt;
use winit::window::Window;

use crate::screen;

const SHADER: &str = r"
struct Screen { size: vec2<f32> };
@group(0) @binding(0) var<uniform> screen: Screen;
@group(1) @binding(0) var image: texture_2d<f32>;
@group(1) @binding(1) var image_sampler: sampler;

struct VertexOutput { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32>, @location(1) alpha: f32 };

@vertex fn vs(@location(0) pixel: vec2<f32>, @location(1) uv: vec2<f32>, @location(2) alpha: f32) -> VertexOutput {
    var out: VertexOutput;
    out.position = vec4(pixel.x / screen.size.x * 2.0 - 1.0, 1.0 - pixel.y / screen.size.y * 2.0, 0.0, 1.0);
    out.uv = uv;
    out.alpha = alpha;
    return out;
}

@fragment fn fs(in: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(image, image_sampler, in.uv);
    return vec4(color.rgb, color.a * in.alpha);
}
";

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    pixel: [f32; 2],
    uv: [f32; 2],
    alpha: f32,
}

#[derive(Clone)]
pub struct Texture {
    bind_group: wgpu::BindGroup,
    pub size: [f32; 2],
}

impl Texture {
    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }
}

/// A texture drawn at a position with a size, in logical pixels.
pub type Sprite<'a> = (&'a Texture, [f32; 2], [f32; 2]);

pub struct Gpu {
    pub window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    pub config: wgpu::SurfaceConfiguration,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    screen_buffer: wgpu::Buffer,
    screen_bind_group: wgpu::BindGroup,
    /// Texture + sampler at bindings 0 and 1; shared by UI images, ground textures, lightmaps and sprites.
    pub texture_layout: wgpu::BindGroupLayout,
    pixel_sampler: wgpu::Sampler,
    smooth_sampler: wgpu::Sampler,
    /// Frames aren't presented: a client driven over the control socket opens behind the window in use, and waiting
    /// for a drawable there can block for good. It presents while it has the focus (click it to watch).
    pub hidden: bool,
    /// Lines of text as textures, with the frame each was last drawn in; and the frame count.
    texts: RefCell<TextCache>,
    frame: Cell<u64>,
}

/// Lines of text (and their color) as textures, with the frame each was last drawn in.
type TextCache = HashMap<(String, [u8; 3]), (Texture, u64)>;

/// How many frames a line of text stays rasterized without being drawn.
const TEXT_FRAMES: u64 = 120;

/// Where the canvas goes in a target of `size` pixels: at `screen_scale`, or smaller to fit (a 1x screenshot), centered
/// (left, top, width, height).
pub fn viewport(size: [u32; 2]) -> [f32; 4] {
    let [width, height] = size.map(|side| side as f32);
    let scale = crate::screen_scale().min(width / screen()[0]).min(height / screen()[1]);
    let [canvas_width, canvas_height] = [screen()[0] * scale, screen()[1] * scale];
    [((width - canvas_width) / 2.0).floor(), ((height - canvas_height) / 2.0).floor(), canvas_width, canvas_height]
}

/// Something drawn before the UI (the 3D world), into a target of the given pixel size.
pub trait WorldPass {
    fn encode(&self, gpu: &Gpu, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView, size: [u32; 2]);
}

impl Gpu {
    pub fn new(window: Arc<Window>) -> Self {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let surface = instance.create_surface(window.clone()).expect("create surface");
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions { compatible_surface: Some(&surface), ..Default::default() }))
            .expect("no GPU adapter");
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).expect("request device");

        let size = window.inner_size();
        let mut config = surface.get_default_config(&adapter, size.width, size.height).expect("surface unsupported");
        // Original assets are sRGB; keep them unchanged on screen.
        config.format = config.format.add_srgb_suffix();
        surface.configure(&device, &config);
        crate::fit_screen([config.width, config.height], window.scale_factor());

        let screen_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            }],
        });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let screen_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&screen()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let screen_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &screen_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: screen_buffer.as_entire_binding() }],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: None, source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&screen_layout), Some(&texture_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32],
                }],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        let pixel_sampler = device.create_sampler(&wgpu::SamplerDescriptor::default());
        let smooth_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        Self {
            window,
            surface,
            config,
            device,
            queue,
            pipeline,
            screen_buffer,
            screen_bind_group,
            texture_layout,
            pixel_sampler,
            smooth_sampler,
            hidden: crate::control::driven(),
            texts: RefCell::new(HashMap::new()),
            frame: Cell::new(0),
        }
    }

    /// Screen pixels per canvas pixel (2 on a Retina display, more in fullscreen).
    pub fn ui_scale(&self) -> f32 {
        viewport([self.config.width, self.config.height])[3] / screen()[1]
    }

    /// A line of UI text, rasterized at the screen's own pixel size so it stays sharp however the canvas is scaled,
    /// and kept while it's drawn.
    pub fn text(&self, font: &Font, line: &str, color: [u8; 3]) -> Texture {
        let frame = self.frame.get();
        let mut texts = self.texts.borrow_mut();
        if let Some((texture, drawn)) = texts.get_mut(&(line.to_owned(), color)) {
            *drawn = frame;
            return texture.clone();
        }
        let scale = self.ui_scale();
        let image = font.render_sized(line, color, text::SIZE * scale);
        let mut texture = self.texture(&image, scale != 1.0);
        texture.size = [image.width() as f32 / scale, image.height() as f32 / scale];
        texts.insert((line.to_owned(), color), (texture.clone(), frame));
        texture
    }

    /// Decodes an image from the GRF; magenta (the client's color key) becomes transparent.
    pub fn load_texture(&self, grf: &Grf, path: &str, smooth: bool) -> Texture {
        self.try_load_texture(grf, path, smooth).unwrap_or_else(|error| panic!("{path}: {error}"))
    }

    /// A client image with its magenta background keyed out, or why it couldn't be loaded.
    pub fn try_load_texture(&self, grf: &Grf, path: &str, smooth: bool) -> Result<Texture, String> {
        Ok(self.texture(&load_image(grf, path)?, smooth))
    }

    pub fn texture(&self, image: &RgbaImage, smooth: bool) -> Texture {
        self.texture_with_sampler(image, if smooth { &self.smooth_sampler } else { &self.pixel_sampler })
    }

    pub fn texture_with_sampler(&self, image: &RgbaImage, sampler: &wgpu::Sampler) -> Texture {
        let texture = self.device.create_texture_with_data(
            &self.queue,
            &wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d { width: image.width(), height: image.height(), depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            image,
        );
        let view = texture.create_view(&Default::default());
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.texture_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
            ],
        });

        Texture { bind_group, size: [image.width() as f32, image.height() as f32] }
    }

    /// A 1x1 texture, stretched to draw solid rectangles.
    pub fn solid(&self, rgba: [u8; 4]) -> Texture {
        self.texture(&RgbaImage::from_pixel(1, 1, image::Rgba(rgba)), false)
    }

    pub fn resize(&mut self) {
        let size = self.window.inner_size();
        self.config.width = size.width.max(1);
        self.config.height = size.height.max(1);
        self.surface.configure(&self.device, &self.config);
        crate::fit_screen([self.config.width, self.config.height], self.window.scale_factor());
        self.queue.write_buffer(&self.screen_buffer, 0, bytemuck::cast_slice(&screen()));
        // Text rasterized for the old size would be blurry or wasteful now.
        self.texts.borrow_mut().clear();
    }

    /// Draws the world and then `sprites` in order, each at its `alphas` opacity (1 past the list's end).
    pub fn draw(&mut self, world: Option<&dyn WorldPass>, sprites: &[Sprite], alphas: &[f32]) {
        crate::control::on_draw(self, world, sprites, alphas);
        if self.hidden {
            // The frame's texture uploads still go out, or they'd pile up until the next screenshot.
            self.queue.submit([]);
            return;
        }
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => return,
            _ => return self.resize(),
        };
        let size = [self.config.width, self.config.height];
        let encoder = self.encode(&frame.texture.create_view(&Default::default()), size, world, sprites, alphas);
        self.queue.submit([encoder.finish()]);
        frame.present();
        let now = self.frame.get() + 1;
        self.frame.set(now);
        self.texts.borrow_mut().retain(|_, (_, drawn)| now - *drawn < TEXT_FRAMES);
    }

    /// Renders one frame at 1x into a PNG, for checking output against the real client.
    pub fn capture(&self, world: Option<&dyn WorldPass>, sprites: &[Sprite], alphas: &[f32], path: &str) {
        self.snapshot(world, sprites, alphas).save(path).expect("save screenshot");
    }

    /// Renders one frame at 1x.
    pub fn snapshot(&self, world: Option<&dyn WorldPass>, sprites: &[Sprite], alphas: &[f32]) -> RgbaImage {
        let [width, height] = screen().map(|value| value as u32);
        let size = wgpu::Extent3d { width, height, depth_or_array_layers: 1 };
        let target = self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let bytes_per_row = (width * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: u64::from(bytes_per_row * height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = self.encode(&target.create_view(&Default::default()), [width, height], world, sprites, alphas);
        encoder.copy_texture_to_buffer(
            target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(bytes_per_row), rows_per_image: Some(height) },
            },
            size,
        );
        self.queue.submit([encoder.finish()]);
        buffer.slice(..).map_async(wgpu::MapMode::Read, |result| result.expect("map screenshot"));
        self.device.poll(wgpu::PollType::wait_indefinitely()).expect("poll");

        let mapped = buffer.slice(..).get_mapped_range();
        let bgra = matches!(self.config.format, wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb);
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for row in mapped.chunks(bytes_per_row as usize) {
            for pixel in row[..(width * 4) as usize].chunks(4) {
                rgba.extend_from_slice(&if bgra { [pixel[2], pixel[1], pixel[0], 255] } else { [pixel[0], pixel[1], pixel[2], 255] });
            }
        }
        RgbaImage::from_raw(width, height, rgba).expect("frame size")
    }

    fn encode(&self, view: &wgpu::TextureView, size: [u32; 2], world: Option<&dyn WorldPass>, sprites: &[Sprite], alphas: &[f32]) -> wgpu::CommandEncoder {
        let vertices: Vec<Vertex> = sprites
            .iter()
            .enumerate()
            .flat_map(|(index, &(_, [x, y], [w, h]))| {
                let alpha = alphas.get(index).copied().unwrap_or(1.0);
                let corner = |dx: f32, dy: f32| Vertex { pixel: [x + dx * w, y + dy * h], uv: [dx, dy], alpha };
                [corner(0., 0.), corner(1., 0.), corner(0., 1.), corner(0., 1.), corner(1., 0.), corner(1., 1.)]
            })
            .collect();
        let vertex_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let mut encoder = self.device.create_command_encoder(&Default::default());
        if let Some(world) = world {
            world.encode(self, &mut encoder, view, size);
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: if world.is_some() { wgpu::LoadOp::Load } else { wgpu::LoadOp::Clear(wgpu::Color::BLACK) },
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            let [x, y, width, height] = viewport(size);
            pass.set_viewport(x, y, width, height, 0.0, 1.0);
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.screen_bind_group, &[]);
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            for (index, (texture, ..)) in sprites.iter().enumerate() {
                let first = index as u32 * 6;
                pass.set_bind_group(1, &texture.bind_group, &[]);
                pass.draw(first..first + 6, 0..1);
            }
        }
        encoder
    }
}

/// A client image with its magenta background keyed out.
pub fn load_image(grf: &Grf, path: &str) -> Result<RgbaImage, String> {
    let bytes = grf.read(path).map_err(|error| format!("{path}: {error}"))?;
    // TGA has no magic number to recognize it by.
    let decoded = if path.to_lowercase().ends_with(".tga") {
        image::load_from_memory_with_format(&bytes, image::ImageFormat::Tga)
    } else {
        image::load_from_memory(&bytes)
    };
    Ok(key_out(decoded.map_err(|error| format!("{path}: {error}"))?.to_rgba8()))
}

/// A glow drawn on black (effect pictures): black becomes transparent, the brightest channel the opacity.
pub fn glow(mut image: RgbaImage) -> RgbaImage {
    for pixel in image.pixels_mut() {
        let [r, g, b, _] = pixel.0;
        let alpha = r.max(g).max(b);
        let lift = |channel: u8| (u16::from(channel) * 255 / u16::from(alpha.max(1))) as u8;
        pixel.0 = [lift(r), lift(g), lift(b), alpha];
    }
    image
}

/// STR textures are loaded with a black key too: a texel whose colour is 0 in the 16-bit format the game keeps the
/// picture in becomes transparent. A BMP is A1R5G5B5 (r, g and b all below 8), a TGA A4R4G4B4 (all below 16).
pub fn key_out_black(mut image: RgbaImage, tga: bool) -> RgbaImage {
    let below = if tga { 16 } else { 8 };
    for pixel in image.pixels_mut() {
        if pixel.0[..3].iter().all(|&channel| channel < below) {
            pixel.0 = [0; 4];
        }
    }
    image
}

/// Magenta, the client's color key, becomes transparent.
pub fn key_out(mut image: RgbaImage) -> RgbaImage {
    for pixel in image.pixels_mut() {
        let [r, g, b, _] = pixel.0;
        if r >= 0xFE && g <= 0x03 && b >= 0xFE {
            pixel.0 = [0; 4];
        }
    }
    image
}
