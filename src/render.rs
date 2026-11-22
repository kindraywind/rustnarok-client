//! 2D drawing on wgpu: textured rectangles in logical pixels, in order.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

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
    /// Where the image sits in the GPU texture it's bound with, in texture coordinates: left, top, width, height (the
    /// whole of it, 0 0 1 1, for an image with a texture of its own).
    rect: [f32; 4],
    /// Its place on an atlas page, given back when the last copy goes.
    _slot: Option<Rc<Slot>>,
}

impl Texture {
    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    /// The texture coordinates of the point `[u, v]` (0 to 1 across the image) in the GPU texture it's bound with.
    pub fn uv(&self, [u, v]: [f32; 2]) -> [f32; 2] {
        [self.rect[0] + u * self.rect[2], self.rect[1] + v * self.rect[3]]
    }
}

/// The side of an atlas page in pixels: 4 MB. 300 monsters of 15 kinds take about 0.64 M of its pixels, 180 players in
/// one look 0.26 M, so a crowd draws in a run or two.
// ponytail: a scene past one page spreads its units over two, which splits the runs between them; draws cost little
// next to the rest of a frame (measured on Metal), so pages stay this small rather than 16 MB each.
const PAGE: u32 = 1024;

/// Sprite frames packed into one large texture: neighbours on screen share its bind group and go in one draw, and a
/// crowd's hundreds of frames cost one GPU allocation instead of one each.
// ponytail: the shelf allocator merges freed shelves but never moves what's left, so a long stay can leave a page
// patchy; a new page takes what doesn't fit, and an emptied one is given back.
struct Page {
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    allocator: RefCell<etagere::AtlasAllocator>,
}

/// An image's place on a page, freed when dropped.
struct Slot {
    page: Rc<Page>,
    id: etagere::AllocId,
}

impl Drop for Slot {
    fn drop(&mut self) {
        self.page.allocator.borrow_mut().deallocate(self.id);
    }
}

/// `image` with a 1 px border repeating its edge pixels: the nearest sampler then reads past an edge (a scaled or turned
/// sprite's last pixels) as it would clamped to an image of its own, never the neighbour on the page.
// ponytail: 1 px is enough for the nearest sampler only; a smooth (linear) or mipmapped atlas would need a wider border.
fn padded(image: &RgbaImage) -> RgbaImage {
    let [width, height] = [image.width(), image.height()];
    RgbaImage::from_fn(width + 2, height + 2, |x, y| *image.get_pixel(x.saturating_sub(1).min(width - 1), y.saturating_sub(1).min(height - 1)))
}

/// A texture drawn at a position with a size, in logical pixels.
pub type Sprite<'a> = (&'a Texture, [f32; 2], [f32; 2]);
/// How opaque a sprite is drawn (0 to 1), and the rectangle on screen it is cut to, if any.
pub type Shade = (f32, Option<[f32; 4]>);

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
    /// The window was fully covered at the last draw, so nothing was presented and nothing waits for the display; and
    /// when that draw began. `request_frame` paces frames by them.
    occluded: bool,
    drawn_at: Instant,
    /// How long the last draw waited for the display to hand it a frame to draw into (not the client's own work).
    pub waited: Duration,
    /// How close two presented frames may come (`frame_floor`).
    frame_floor: Duration,
    /// Lines of text as textures, with the frame each was last drawn in; and the frame count.
    texts: RefCell<TextCache>,
    frame: Cell<u64>,
    /// The atlas pages sprite frames go on.
    pages: RefCell<Vec<Rc<Page>>>,
}

/// Lines of text (with their color and pixel size) as textures, with the frame each was last drawn in.
type TextCache = HashMap<(String, [u8; 3], u32), (Texture, u64)>;

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
        // Metal keeps latency + 1 drawables, so 1 saves a window-sized one; a frame takes ~1.5 ms, well inside 8.3 ms.
        // ponytail: macOS only, where frame pacing was measured; Vulkan and DX12 count frame latency differently.
        if cfg!(target_os = "macos") {
            config.desired_maximum_frame_latency = 1;
        }
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
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32],
                })],
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

        let frame_floor = frame_floor(&window);
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
            occluded: false,
            drawn_at: Instant::now(),
            waited: Duration::ZERO,
            frame_floor,
            texts: RefCell::new(HashMap::new()),
            frame: Cell::new(0),
            pages: RefCell::default(),
        }
    }

    /// Screen pixels per canvas pixel (2 on a Retina display, more in fullscreen).
    pub fn ui_scale(&self) -> f32 {
        viewport([self.config.width, self.config.height])[3] / screen()[1]
    }

    /// A line of UI text, rasterized at the screen's own pixel size so it stays sharp however the canvas is scaled,
    /// and kept while it's drawn.
    pub fn text(&self, font: &Font, line: &str, color: [u8; 3]) -> Texture {
        self.text_sized(font, line, color, text::SIZE)
    }

    /// The same at another pixel size (an announcement comes in the server's own).
    pub fn text_sized(&self, font: &Font, line: &str, color: [u8; 3], size: f32) -> Texture {
        let frame = self.frame.get();
        let mut texts = self.texts.borrow_mut();
        let key = (line.to_owned(), color, size.to_bits());
        if let Some((texture, drawn)) = texts.get_mut(&key) {
            *drawn = frame;
            return texture.clone();
        }
        let scale = self.ui_scale();
        let image = font.render_sized(line, color, size * scale);
        let mut texture = self.texture(&image, scale != 1.0);
        texture.size = [image.width() as f32 / scale, image.height() as f32 / scale];
        texts.insert(key, (texture.clone(), frame));
        texture
    }

    /// Drops the lines of text rasterized so far (another font).
    pub fn forget_texts(&self) {
        self.texts.borrow_mut().clear();
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

        Texture { bind_group, size: [image.width() as f32, image.height() as f32], rect: [0.0, 0.0, 1.0, 1.0], _slot: None }
    }

    /// A sprite frame, drawn with the nearest sampler: on an atlas page, or in a texture of its own when it's too big for
    /// one (over a quarter of a page's side).
    pub fn sprite_texture(&self, image: &RgbaImage) -> Texture {
        let size = [image.width() + 2, image.height() + 2];
        if image.width() == 0 || image.height() == 0 || size.iter().any(|&side| side > PAGE / 4) {
            return self.texture(image, false);
        }
        let mut pages = self.pages.borrow_mut();
        // A page emptied since (its units gone) gives its memory back; one is kept for what comes next.
        let mut kept = false;
        pages.retain(|page| !page.allocator.borrow().is_empty() || !std::mem::replace(&mut kept, true));
        let wanted = etagere::size2(size[0] as i32, size[1] as i32);
        let found = pages.iter().find_map(|page| Some((page.clone(), page.allocator.borrow_mut().allocate(wanted)?)));
        let (page, allocation) = match found {
            Some(found) => found,
            None => {
                let page = Rc::new(self.page());
                let allocation = page.allocator.borrow_mut().allocate(wanted).expect("a sprite frame fits an empty page");
                pages.push(page.clone());
                (page, allocation)
            }
        };
        let [x, y] = [allocation.rectangle.min.x as u32, allocation.rectangle.min.y as u32];
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &page.texture, mip_level: 0, origin: wgpu::Origin3d { x, y, z: 0 }, aspect: wgpu::TextureAspect::All },
            &padded(image),
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(size[0] * 4), rows_per_image: None },
            wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
        );
        let side = PAGE as f32;
        let [width, height] = [image.width() as f32, image.height() as f32];
        Texture {
            bind_group: page.bind_group.clone(),
            size: [width, height],
            rect: [(x + 1) as f32 / side, (y + 1) as f32 / side, width / side, height / side],
            _slot: Some(Rc::new(Slot { page, id: allocation.id })),
        }
    }

    /// An empty atlas page.
    fn page(&self) -> Page {
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("sprite atlas"),
            size: wgpu::Extent3d { width: PAGE, height: PAGE, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sprite atlas"),
            layout: &self.texture_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.pixel_sampler) },
            ],
        });
        Page { texture, bind_group, allocator: RefCell::new(etagere::AtlasAllocator::new(etagere::size2(PAGE as i32, PAGE as i32))) }
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

    /// Asks for the next frame once it's due, and says when to look again: when it's due, or a `FRAME_TIME` away.
    /// Presenting usually waits for the display, but a display gone to sleep waits for nothing, so presented frames are
    /// kept `frame_floor` apart too; a covered window presents nothing, and its frames are kept `COVERED_FRAME_TIME` apart.
    pub fn request_frame(&self) -> Instant {
        let due = self.drawn_at + frame_gap(self.occluded, crate::control::driven(), self.frame_floor);
        let now = Instant::now();
        if now < due {
            return due.min(now + crate::FRAME_TIME);
        }
        // The frame asked for wakes the loop itself; a wait already over would spin it until the frame is drawn.
        self.window.request_redraw();
        now + crate::FRAME_TIME
    }

    /// Whether a frame now would come too close to the last presented one. A packet, the pointer or a command asks for
    /// a frame of its own; a covered window still draws it (it presents nothing), else `request_frame` does.
    pub fn too_soon(&self) -> bool {
        !self.occluded && self.drawn_at.elapsed() < self.frame_floor
    }

    /// The window moved, maybe onto a display with another refresh rate.
    pub fn moved(&mut self) {
        self.frame_floor = frame_floor(&self.window);
    }

    /// Draws the world and then `sprites` in order, each at its `alphas` opacity and cut (opaque and whole past the list's end).
    pub fn draw(&mut self, world: Option<&dyn WorldPass>, sprites: &[Sprite], alphas: &[Shade]) {
        crate::control::on_draw(self, world, sprites, alphas);
        // The frame counts whether or not it shows: a covered window still lays out its text every frame, and the lines
        // it no longer draws must go then too (they piled up by the thousand behind other windows).
        let now = self.frame.get() + 1;
        self.frame.set(now);
        self.texts.borrow_mut().retain(|_, (_, drawn)| now - *drawn < TEXT_FRAMES);
        self.drawn_at = Instant::now();
        let acquired = self.surface.get_current_texture();
        self.waited = self.drawn_at.elapsed();
        let frame = match acquired {
            wgpu::CurrentSurfaceTexture::Success(frame) | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                // Covered: the uploads still go out, or they'd pile up (gigabytes in a minute) until it shows again.
                self.occluded = true;
                self.queue.submit([]);
                return;
            }
            _ => return self.resize(),
        };
        self.occluded = false;
        let size = [self.config.width, self.config.height];
        let encoder = self.encode(&frame.texture.create_view(&Default::default()), size, world, sprites, alphas);
        self.queue.submit([encoder.finish()]);
        self.queue.present(frame);
    }

    /// Whether the window was covered at the last draw.
    pub fn occluded(&self) -> bool {
        self.occluded
    }

    /// How many lines of text are rasterized now (the control socket's `memory.texts`).
    pub fn text_count(&self) -> usize {
        self.texts.borrow().len()
    }

    /// Renders one frame at 1x into a PNG, for checking output against the real client.
    pub fn capture(&self, world: Option<&dyn WorldPass>, sprites: &[Sprite], alphas: &[Shade], path: &str) {
        self.snapshot(world, sprites, alphas).save(path).expect("save screenshot");
    }

    /// Renders one frame at 1x.
    pub fn snapshot(&self, world: Option<&dyn WorldPass>, sprites: &[Sprite], alphas: &[Shade]) -> RgbaImage {
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

        let mapped = buffer.slice(..).get_mapped_range().expect("screenshot mapped");
        let bgra = matches!(self.config.format, wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb);
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for row in mapped.chunks(bytes_per_row as usize) {
            for pixel in row[..(width * 4) as usize].chunks(4) {
                rgba.extend_from_slice(&if bgra { [pixel[2], pixel[1], pixel[0], 255] } else { [pixel[0], pixel[1], pixel[2], 255] });
            }
        }
        RgbaImage::from_raw(width, height, rgba).expect("frame size")
    }

    fn encode(&self, view: &wgpu::TextureView, size: [u32; 2], world: Option<&dyn WorldPass>, sprites: &[Sprite], alphas: &[Shade]) -> wgpu::CommandEncoder {
        let [left, top, width, height] = viewport(size);
        // At a scale that isn't whole (a window under 1024×768, the Extras' UI scale) every edge goes to the target's
        // nearest pixel: text, rasterized at that scale, lands on its own pixels, and pictures side by side still meet.
        // ponytail: the pictures' own pixels then come out a target pixel wider or narrower than their neighbours; a
        // shader that blends only at their seams would even that out.
        let scale = width / screen()[0];
        let snap = |at: f32| if scale.fract() == 0.0 { at } else { (at * scale).round() / scale };
        let vertices: Vec<Vertex> = sprites
            .iter()
            .enumerate()
            .flat_map(|(index, &(texture, [x, y], [w, h]))| {
                let alpha = alphas.get(index).map_or(1.0, |shade| shade.0);
                let corner = |dx: f32, dy: f32| Vertex { pixel: [snap(x + dx * w), snap(y + dy * h)], uv: texture.uv([dx, dy]), alpha };
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
            pass.set_viewport(left, top, width, height, 0.0, 1.0);
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.screen_bind_group, &[]);
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            // A cut sprite draws only inside its rectangle, in the target's pixels.
            let target = |[x, y, w, h]: [f32; 4]| {
                let [x0, y0] = [(left + x * scale).round().clamp(0.0, size[0] as f32), (top + y * scale).round().clamp(0.0, size[1] as f32)];
                let [x1, y1] = [(left + (x + w) * scale).round().clamp(x0, size[0] as f32), (top + (y + h) * scale).round().clamp(y0, size[1] as f32)];
                [x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32]
            };
            let mut cut = None;
            // Neighbours on one texture under one cut go in one draw (a grid of slots, a line of the same icon).
            let mut run: Option<(&wgpu::BindGroup, std::ops::Range<u32>)> = None;
            let flush = |pass: &mut wgpu::RenderPass, run: &mut Option<(&wgpu::BindGroup, std::ops::Range<u32>)>| {
                if let Some((bind_group, range)) = run.take() {
                    pass.set_bind_group(1, bind_group, &[]);
                    pass.draw(range, 0..1);
                }
            };
            for (index, (texture, ..)) in sprites.iter().enumerate() {
                let clip = alphas.get(index).and_then(|shade| shade.1);
                let [x, y, w, h] = clip.map_or([0, 0, size[0], size[1]], target);
                if w == 0 || h == 0 {
                    continue;
                }
                if clip != cut {
                    flush(&mut pass, &mut run);
                    pass.set_scissor_rect(x, y, w, h);
                    cut = clip;
                }
                let first = index as u32 * 6;
                match &mut run {
                    Some((bind_group, range)) if *bind_group == &texture.bind_group && range.end == first => range.end += 6,
                    _ => {
                        flush(&mut pass, &mut run);
                        run = Some((&texture.bind_group, first..first + 6));
                    }
                }
            }
            flush(&mut pass, &mut run);
        }
        encoder
    }
}

/// A client image with its magenta background keyed out.
pub fn load_image(grf: &Grf, path: &str) -> Result<RgbaImage, String> {
    let bytes = grf.read(path).map_err(|error| format!("{path}: {error}"))?;
    Ok(key_out(decode_image(path, &bytes).map_err(|error| format!("{path}: {error}"))?.to_rgba8()))
}

/// Decodes an image file of the data by its contents, or, for TGA, which has no magic number to recognize it by, by
/// its name.
pub fn decode_image(path: &str, bytes: &[u8]) -> image::ImageResult<image::DynamicImage> {
    if path.to_lowercase().ends_with(".tga") { image::load_from_memory_with_format(bytes, image::ImageFormat::Tga) } else { image::load_from_memory(bytes) }
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
/// How often a covered or minimised window draws while nothing happens. Packets, input and control commands each ask
/// for a frame of their own, so they aren't held back; this only slows the idle animation no one can see.
const COVERED_FRAME_TIME: Duration = Duration::from_millis(250);

/// How long after a frame began the next may: `frame_floor` while frames are presented, else `COVERED_FRAME_TIME`. A
/// client driven over the control socket, often launched behind other windows, keeps a frame time: its scenarios' shots
/// follow the camera's easing frame by frame.
fn frame_gap(occluded: bool, driven: bool, floor: Duration) -> Duration {
    if !occluded {
        floor
    } else if driven {
        crate::FRAME_TIME
    } else {
        COVERED_FRAME_TIME
    }
}

/// How close presented frames may come: three quarters of a refresh of the window's display (6.25 ms at 120 Hz), or zero
/// when the system doesn't say. A display awake paces them itself; a whole refresh would let each frame begin a little
/// later than the last until one misses its refresh (108 of 120 fps). A display asleep paces nothing: about 150 fps
/// then, not as many as the GPU can draw.
fn frame_floor(window: &Window) -> Duration {
    window
        .current_monitor()
        .and_then(|monitor| monitor.refresh_rate_millihertz())
        .filter(|&millihertz| millihertz > 0)
        .map_or(Duration::ZERO, |millihertz| Duration::from_nanos(750_000_000_000 / u64::from(millihertz)))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    #[test]
    fn decodes_a_tga_by_its_name() {
        // An uncompressed 24-bit 1x1 TGA (the pixel stored blue, green, red): nothing in it says TGA.
        let tga = [0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 1, 0, 24, 0, 0x30, 0x20, 0x10];
        assert_eq!(super::decode_image("texture\\hero\\hero07.TGA", &tga).unwrap().to_rgba8().get_pixel(0, 0).0, [0x10, 0x20, 0x30, 255]);
        assert!(super::decode_image("hero07.bmp", &tga).is_err());
    }

    #[test]
    fn presented_frames_keep_a_floor_apart() {
        let floor = Duration::from_micros(6250);
        // Presenting: the floor, even while the display sleeps and waits for nothing; none known, none.
        assert_eq!(super::frame_gap(false, false, floor), floor);
        assert_eq!(super::frame_gap(false, true, floor), floor);
        assert_eq!(super::frame_gap(false, false, Duration::ZERO), Duration::ZERO);
        // Covered: `COVERED_FRAME_TIME`; driven over the control socket, a frame time, as before.
        assert_eq!(super::frame_gap(true, false, floor), super::COVERED_FRAME_TIME);
        assert_eq!(super::frame_gap(true, true, floor), crate::FRAME_TIME);
    }

    #[test]
    fn atlas_images_are_padded_with_their_own_edges() {
        // 2x2: red green over blue white. The border repeats the nearest edge pixel, corners included, so a sample past
        // the image reads it as a clamped texture of its own would.
        let [r, g, b, w] = [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255], [255; 4]];
        let image = image::RgbaImage::from_fn(2, 2, |x, y| image::Rgba([[r, g], [b, w]][y as usize][x as usize]));
        let rows: Vec<Vec<[u8; 4]>> = super::padded(&image).rows().map(|row| row.map(|pixel| pixel.0).collect()).collect();
        assert_eq!(rows, [[r, r, g, g], [r, r, g, g], [b, b, w, w], [b, b, w, w]]);
    }
}
