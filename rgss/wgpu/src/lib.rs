//! Optional desktop services for the headless RGSS core.
//!
//! This crate deliberately contains no Ruby or game policy. It only wraps a
//! WGPU surface and a CPAL output stream so an embedding application can own
//! the event loop, pacing, input mapping, and audio mixer.

use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Instant;

use bytemuck::{Pod, Zeroable};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};
use lewton::inside_ogg::OggStreamReader;
use sabiruby_rgss_core::{RenderCache, RenderSprite};
use wgpu::CurrentSurfaceTexture;
use winit::dpi::PhysicalSize;
use winit::window::Window;

mod paging;

/// An asset key without its page suffix: the bitmap revision a page belongs to.
fn page_base(key: &str) -> &str {
    key.split_once(paging::PAGE_SUFFIX)
        .map_or(key, |(base, _)| base)
}

#[cfg(feature = "tracy")]
macro_rules! tracy_zone {
    ($name:literal) => {
        tracy_client::Client::running()
            .map(|client| client.span(tracy_client::span_location!($name), 0))
    };
}

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    size: PhysicalSize<u32>,
    pipeline: wgpu::RenderPipeline,
    screen_buffer: wgpu::Buffer,
    screen_bind_group: wgpu::BindGroup,
    static_screen_buffer: wgpu::Buffer,
    static_screen_bind_group: wgpu::BindGroup,
    texture_layout: wgpu::BindGroupLayout,
    assets: HashMap<Arc<str>, TextureAsset>,
    profiling: bool,
    frame_profile: Option<RenderProfile>,
    vertices: Vec<Vertex>,
    vertex_buffers: [Option<wgpu::Buffer>; 3],
    vertex_buffer_index: usize,
    vertex_capacity: usize,
    static_vertices: Vec<Vertex>,
    static_buffer: Option<wgpu::Buffer>,
    static_capacity: usize,
    render_cache: RenderCache,
    static_entries: Vec<Option<StaticEntry>>,
    /// Pages of bitmaps taller than `max_texture_dimension_2d` (`paging`).
    pages: paging::PageCache,
}

/// CPU/API timings only. Submission does not wait for GPU execution.
#[derive(Clone, Debug, Default)]
pub struct RenderProfile {
    pub acquire_ns: u64,
    pub prepare_ns: u64,
    pub geometry_ns: u64,
    pub vertex_upload_ns: u64,
    pub encode_ns: u64,
    pub submit_ns: u64,
    pub present_ns: u64,
    pub draw_calls: u64,
    pub sprites: u64,
    pub vertices: u64,
    pub static_vertices: u64,
    pub dynamic_vertices: u64,
    pub static_rebuilds: u64,
    pub uploads: u64,
    pub upload_bytes: u64,
}

#[derive(Clone)]
struct StaticEntry {
    bind_group: wgpu::BindGroup,
    range: core::ops::Range<u32>,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ScreenUniform {
    size: [f32; 2],
    offset: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 2],
    uv: [f32; 2],
    alpha: f32,
    color: [f32; 4],
    tone: [f32; 4],
}

impl Vertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![
        0 => Float32x2,
        1 => Float32x2,
        2 => Float32,
        3 => Float32x4,
        4 => Float32x4
    ];

    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

struct TextureAsset {
    _texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
}

impl Renderer {
    pub async fn new(window: Arc<Window>) -> Result<Self, String> {
        Self::new_with_present_mode(window, None).await
    }

    /// Construct a renderer whose surface requests the no-vsync automatic
    /// mode. The adapter can still fall back when a platform cannot provide it.
    pub async fn new_unlocked(window: Arc<Window>) -> Result<Self, String> {
        Self::new_with_present_mode(window, Some(wgpu::PresentMode::AutoNoVsync)).await
    }

    async fn new_with_present_mode(
        window: Arc<Window>,
        present_mode: Option<wgpu::PresentMode>,
    ) -> Result<Self, String> {
        let size = window.inner_size();
        let instance = wgpu::Instance::default();
        let surface = instance
            .create_surface(window)
            .map_err(|error| format!("surface: {error}"))?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                ..Default::default()
            })
            .await
            .map_err(|error| format!("adapter: {error}"))?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("sabiruby-rgss-device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::Off,
            })
            .await
            .map_err(|error| format!("device: {error}"))?;
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .ok_or_else(|| "surface has no supported formats".to_owned())?;
        if let Some(mode) = present_mode {
            config.present_mode = mode;
        }
        surface.configure(&device, &config);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sabiruby-rgss-sprite-shader"),
            source: wgpu::ShaderSource::Wgsl(SPRITE_SHADER.into()),
        });
        let screen_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sabiruby-rgss-screen-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sabiruby-rgss-texture-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sabiruby-rgss-pipeline-layout"),
            bind_group_layouts: &[Some(&screen_layout), Some(&texture_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sabiruby-rgss-sprite-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(Vertex::layout())],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let screen_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sabiruby-rgss-screen-buffer"),
            size: std::mem::size_of::<ScreenUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let screen_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sabiruby-rgss-screen-bind-group"),
            layout: &screen_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: screen_buffer.as_entire_binding(),
            }],
        });
        let static_screen_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sabiruby-rgss-static-screen-buffer"),
            size: std::mem::size_of::<ScreenUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let static_screen_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sabiruby-rgss-static-screen-bind-group"),
            layout: &screen_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: static_screen_buffer.as_entire_binding(),
            }],
        });
        Ok(Self {
            surface,
            device,
            queue,
            config,
            size,
            pipeline,
            screen_buffer,
            screen_bind_group,
            static_screen_buffer,
            static_screen_bind_group,
            texture_layout,
            assets: HashMap::new(),
            profiling: false,
            frame_profile: None,
            vertices: Vec::new(),
            vertex_buffers: [None, None, None],
            vertex_buffer_index: 0,
            vertex_capacity: 0,
            static_vertices: Vec::new(),
            static_buffer: None,
            static_capacity: 0,
            render_cache: RenderCache::default(),
            static_entries: Vec::new(),
            pages: paging::PageCache::default(),
        })
    }

    pub fn size(&self) -> PhysicalSize<u32> {
        self.size
    }

    pub fn set_profiling(&mut self, enabled: bool) {
        self.profiling = enabled;
    }

    pub fn take_frame_profile(&mut self) -> Option<RenderProfile> {
        self.frame_profile.take()
    }

    pub fn resize(&mut self, size: PhysicalSize<u32>) {
        self.size = size;
        if size.width == 0 || size.height == 0 {
            return;
        }
        self.config.width = size.width;
        self.config.height = size.height;
        self.surface.configure(&self.device, &self.config);
    }

    pub fn render(&mut self) -> Result<(), RenderError> {
        self.render_frame(&[])
    }

    /// Draw one RGSS frame.  Bitmap decoding is cached by `asset_key`; only
    /// sprite geometry and changed textures are submitted each frame.
    pub fn render_frame(&mut self, sprites: &[RenderSprite]) -> Result<(), RenderError> {
        self.render_frame_sized(sprites, self.size.width, self.size.height)
    }

    /// Draw using the game's logical RGSS resolution. The surface may be
    /// larger after a window resize; Ruby coordinates remain unchanged.
    pub fn render_frame_sized(
        &mut self,
        sprites: &[RenderSprite],
        logical_width: u32,
        logical_height: u32,
    ) -> Result<(), RenderError> {
        self.frame_profile = self.profiling.then(RenderProfile::default);
        self.render_frame_sized_impl(sprites, logical_width, logical_height)
    }

    fn render_frame_sized_impl(
        &mut self,
        sprites: &[RenderSprite],
        logical_width: u32,
        logical_height: u32,
    ) -> Result<(), RenderError> {
        let mut phase = self.profiling.then(Instant::now);
        if self.size.width == 0 || self.size.height == 0 {
            return Ok(());
        }
        let max_dimension = self.device.limits().max_texture_dimension_2d;
        let paged = self.pages.page(sprites, max_dimension);
        let sprites: &[RenderSprite] = &paged;
        #[cfg(feature = "tracy")]
        let acquire_zone = tracy_zone!("render.acquire");
        let output = self.surface.get_current_texture();
        #[cfg(feature = "tracy")]
        drop(acquire_zone);
        if let (Some(stats), Some(start)) = (&mut self.frame_profile, phase) {
            stats.acquire_ns = start.elapsed().as_nanos() as u64;
        }
        if self.profiling {
            phase = Some(Instant::now());
        }
        let output = match output {
            CurrentSurfaceTexture::Success(texture)
            | CurrentSurfaceTexture::Suboptimal(texture) => texture,
            CurrentSurfaceTexture::Timeout => {
                return Err(RenderError::Recoverable(RenderStatus::Timeout));
            }
            CurrentSurfaceTexture::Occluded => {
                return Err(RenderError::Recoverable(RenderStatus::Occluded));
            }
            CurrentSurfaceTexture::Outdated => {
                return Err(RenderError::Recoverable(RenderStatus::Outdated));
            }
            CurrentSurfaceTexture::Lost => {
                return Err(RenderError::Recoverable(RenderStatus::Lost));
            }
            CurrentSurfaceTexture::Validation => {
                return Err(RenderError::Fatal("surface validation failure".to_owned()));
            }
        };
        #[cfg(feature = "tracy")]
        let prepare_zone = tracy_zone!("render.prepare_upload");
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("sabiruby-rgss-clear"),
            });
        self.queue.write_buffer(
            &self.screen_buffer,
            0,
            bytemuck::bytes_of(&ScreenUniform {
                size: [logical_width.max(1) as f32, logical_height.max(1) as f32],
                offset: [0.0, 0.0],
            }),
        );
        #[cfg(feature = "tracy")]
        let geometry_zone = tracy_zone!("render.prepare.geometry");
        let geometry_start = self.profiling.then(Instant::now);
        self.vertices.clear();
        let cache_plan = self
            .render_cache
            .prepare(sprites, logical_width, logical_height);
        let static_reset = cache_plan.static_rebuild;
        if static_reset {
            self.static_vertices.clear();
            self.static_entries.clear();
            self.static_entries.resize_with(sprites.len(), || None);
            for (index, sprite) in sprites.iter().enumerate() {
                // UI and dialogue sprites are layer-sensitive.  Keep them in
                // the ordinary ordered stream so a retained map buffer can
                // never hide a message window or reorder it relative to a
                // picture.  Map sprites use a clipped viewport and low z.
                // Character sprites (player, followers, and spawned wild
                // Pokémon) move and are created/disposed independently of
                // the tilemap.  Retain only actual tilemap planes here.
                let cacheable = sprite.tile && sprite.clip_rect.is_some() && sprite.z < 10_000;
                if !sprite.visible
                    || sprite.opacity == 0
                    || sprite.src_width <= 0
                    || sprite.src_height <= 0
                    || !cacheable
                {
                    continue;
                }
                let Ok(bind_group) = self.ensure_asset(sprite) else {
                    continue;
                };
                let first = self.static_vertices.len() as u32;
                append_vertices(
                    &mut self.static_vertices,
                    sprite,
                    logical_width,
                    logical_height,
                );
                let end = self.static_vertices.len() as u32;
                self.static_entries[index] = Some(StaticEntry {
                    bind_group,
                    range: first..end,
                });
            }
        }
        self.queue.write_buffer(
            &self.static_screen_buffer,
            0,
            bytemuck::bytes_of(&ScreenUniform {
                size: [logical_width.max(1) as f32, logical_height.max(1) as f32],
                offset: [
                    cache_plan.static_offset[0] as f32,
                    cache_plan.static_offset[1] as f32,
                ],
            }),
        );
        let mut draws: Vec<(wgpu::BindGroup, [u32; 4], core::ops::Range<u32>, bool, &str)> =
            Vec::with_capacity(sprites.len());
        let mut last_asset_key: Option<&str> = None;
        let mut last_bind_group: Option<wgpu::BindGroup> = None;
        let mut last_clip_rect: Option<Option<[i64; 4]>> = None;
        let mut last_scissor = None;
        let mut static_drawn_vertices = 0usize;
        for (index, sprite) in sprites.iter().enumerate().filter(|(_, sprite)| {
            sprite.visible && sprite.opacity > 0 && sprite.src_width > 0 && sprite.src_height > 0
        }) {
            let scissor = if last_clip_rect == Some(sprite.clip_rect) {
                last_scissor
            } else {
                let value = viewport_scissor(
                    sprite.clip_rect,
                    logical_width,
                    logical_height,
                    self.config.width,
                    self.config.height,
                );
                last_clip_rect = Some(sprite.clip_rect);
                last_scissor = value;
                value
            };
            let Some(scissor) = scissor else {
                continue;
            };
            let asset_key: &str = &sprite.asset_key;
            let cacheable = sprite.tile && sprite.clip_rect.is_some() && sprite.z < 10_000;
            let (bind_group, range, is_static) = if cacheable
                && cache_plan.static_eligible.get(index) == Some(&true)
                && self
                    .static_entries
                    .get(index)
                    .and_then(Option::as_ref)
                    .is_some()
            {
                let Some(entry) = self.static_entries.get(index).and_then(Option::as_ref) else {
                    continue;
                };
                (entry.bind_group.clone(), entry.range.clone(), true)
            } else {
                let bind_group = if last_asset_key == Some(asset_key) {
                    last_bind_group
                        .as_ref()
                        .expect("cached asset key has a bind group")
                        .clone()
                } else {
                    let asset = match self.ensure_asset(sprite) {
                        Ok(asset) => asset,
                        Err(error) => {
                            eprintln!("skipping sprite {}: {error}", sprite.asset_key);
                            last_asset_key = None;
                            last_bind_group = None;
                            continue;
                        }
                    };
                    last_asset_key = Some(asset_key);
                    last_bind_group = Some(asset.clone());
                    asset
                };
                let first = self.vertices.len() as u32;
                append_vertices(&mut self.vertices, sprite, logical_width, logical_height);
                let end = self.vertices.len() as u32;
                (bind_group, first..end, false)
            };
            if is_static {
                static_drawn_vertices += (range.end - range.start) as usize;
            }
            if let Some(last) = draws
                .last_mut()
                .filter(|last| last.3 == is_static && last.1 == scissor && last.4 == asset_key)
            {
                last.2.end = range.end;
            } else {
                draws.push((bind_group, scissor, range, is_static, asset_key));
            }
        }
        #[cfg(feature = "tracy")]
        drop(geometry_zone);
        if let (Some(stats), Some(start)) = (&mut self.frame_profile, geometry_start) {
            stats.geometry_ns = start.elapsed().as_nanos() as u64;
        }
        #[cfg(feature = "tracy")]
        let vertex_upload_zone = tracy_zone!("render.prepare.vertex_upload");
        let vertex_upload_start = self.profiling.then(Instant::now);
        if static_reset && !self.static_vertices.is_empty() {
            if self.static_vertices.len() > self.static_capacity {
                self.static_capacity = self.static_vertices.len().next_power_of_two();
                self.static_buffer = Some(self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("sabiruby-rgss-static-vertices"),
                    size: (self.static_capacity * core::mem::size_of::<Vertex>()) as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }));
            }
            if let Some(buffer) = &self.static_buffer {
                self.queue
                    .write_buffer(buffer, 0, bytemuck::cast_slice(&self.static_vertices));
            }
        }
        self.vertex_buffer_index = (self.vertex_buffer_index + 1) % self.vertex_buffers.len();
        if self.vertices.len() > self.vertex_capacity {
            self.vertex_capacity = self.vertices.len().next_power_of_two();
            for buffer in &mut self.vertex_buffers {
                *buffer = None;
            }
        }
        if !self.vertices.is_empty() && self.vertex_buffers[self.vertex_buffer_index].is_none() {
            self.vertex_buffers[self.vertex_buffer_index] =
                Some(self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("sabiruby-rgss-frame-vertices"),
                    size: (self.vertex_capacity * core::mem::size_of::<Vertex>()) as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }));
        }
        if let Some(buffer) = &self.vertex_buffers[self.vertex_buffer_index] {
            if !self.vertices.is_empty() {
                self.queue
                    .write_buffer(buffer, 0, bytemuck::cast_slice(&self.vertices));
            }
        }
        #[cfg(feature = "tracy")]
        drop(vertex_upload_zone);
        if let (Some(stats), Some(start)) = (&mut self.frame_profile, vertex_upload_start) {
            stats.vertex_upload_ns = start.elapsed().as_nanos() as u64;
        }
        #[cfg(feature = "tracy")]
        drop(prepare_zone);
        if let (Some(stats), Some(start)) = (&mut self.frame_profile, phase) {
            stats.prepare_ns = start.elapsed().as_nanos() as u64;
            stats.draw_calls = draws.len() as u64;
            stats.sprites = sprites.len() as u64;
            stats.vertices = (self.vertices.len() + static_drawn_vertices) as u64;
            stats.static_vertices = static_drawn_vertices as u64;
            stats.dynamic_vertices = self.vertices.len() as u64;
            stats.static_rebuilds = u64::from(static_reset);
        }
        if self.profiling {
            phase = Some(Instant::now());
        }
        #[cfg(feature = "tracy")]
        let encode_zone = tracy_zone!("render.encode");
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("sabiruby-rgss-frame"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            for (bind_group, scissor, range, is_static, _) in &draws {
                let buffer = if *is_static {
                    self.static_buffer.as_ref()
                } else {
                    self.vertex_buffers[self.vertex_buffer_index].as_ref()
                };
                let Some(buffer) = buffer else {
                    continue;
                };
                pass.set_vertex_buffer(0, buffer.slice(..));
                pass.set_bind_group(
                    0,
                    if *is_static {
                        &self.static_screen_bind_group
                    } else {
                        &self.screen_bind_group
                    },
                    &[],
                );
                pass.set_scissor_rect(scissor[0], scissor[1], scissor[2], scissor[3]);
                pass.set_bind_group(1, bind_group, &[]);
                pass.draw(range.clone(), 0..1);
            }
        }
        #[cfg(feature = "tracy")]
        drop(encode_zone);
        if let (Some(stats), Some(start)) = (&mut self.frame_profile, phase) {
            stats.encode_ns = start.elapsed().as_nanos() as u64;
        }
        if self.profiling {
            phase = Some(Instant::now());
        }
        #[cfg(feature = "tracy")]
        let submit_zone = tracy_zone!("render.submit");
        self.queue.submit(Some(encoder.finish()));
        #[cfg(feature = "tracy")]
        drop(submit_zone);
        if let (Some(stats), Some(start)) = (&mut self.frame_profile, phase) {
            stats.submit_ns = start.elapsed().as_nanos() as u64;
        }
        if self.profiling {
            phase = Some(Instant::now());
        }
        #[cfg(feature = "tracy")]
        let present_zone = tracy_zone!("render.present");
        self.queue.present(output);
        #[cfg(feature = "tracy")]
        drop(present_zone);
        if let (Some(stats), Some(start)) = (&mut self.frame_profile, phase) {
            stats.present_ns = start.elapsed().as_nanos() as u64;
        }
        #[cfg(feature = "tracy")]
        if let Some(client) = tracy_client::Client::running() {
            client.plot(
                tracy_client::plot_name!("render.draw_calls"),
                draws.len() as f64,
            );
            client.plot(
                tracy_client::plot_name!("render.vertices"),
                self.vertices.len() as f64,
            );
            if let Some(stats) = self.frame_profile.as_ref() {
                client.plot(
                    tracy_client::plot_name!("render.texture_uploads"),
                    stats.uploads as f64,
                );
                client.plot(
                    tracy_client::plot_name!("render.upload_bytes"),
                    stats.upload_bytes as f64,
                );
            }
        }
        Ok(())
    }

    fn ensure_asset(&mut self, sprite: &RenderSprite) -> Result<wgpu::BindGroup, String> {
        if let Some(asset) = self.assets.get(sprite.asset_key.as_ref()) {
            return Ok(asset.bind_group.clone());
        }
        if self.assets.get(sprite.asset_key.as_ref()).is_none() {
            #[cfg(feature = "tracy")]
            let decode_zone = tracy_zone!("render.asset.decode");
            let (decoded, width, height) = if let Some((width, height)) = sprite.raw_rgba {
                if sprite.image.len()
                    < (width as usize)
                        .saturating_mul(height as usize)
                        .saturating_mul(4)
                {
                    return Err(format!(
                        "raw RGBA asset {} is truncated ({} bytes for {}x{})",
                        sprite.asset_key,
                        sprite.image.len(),
                        width,
                        height
                    ));
                }
                (sprite.image.to_vec(), width, height)
            } else {
                let decoded = image::load_from_memory(&sprite.image)
                    .map_err(|error| {
                        format!("asset {} could not be decoded: {error}", sprite.asset_key)
                    })?
                    .to_rgba8();
                let (width, height) = decoded.dimensions();
                (decoded.into_raw(), width, height)
            };
            #[cfg(feature = "tracy")]
            drop(decode_zone);
            if width == 0 || height == 0 {
                return Err(format!(
                    "asset {} has invalid dimensions {}x{}",
                    sprite.asset_key, width, height
                ));
            }
            let max_dimension = self.device.limits().max_texture_dimension_2d;
            if width > max_dimension || height > max_dimension {
                return Err(format!(
                    "asset {} dimensions {}x{} exceed GPU limit {}",
                    sprite.asset_key, width, height, max_dimension
                ));
            }
            // Raw RGSS bitmaps use a revision in their cache key so a changed
            // frame is visible immediately. Retain only the current revision
            // for that bitmap; keeping every animation frame's texture alive
            // grows GPU memory without bound during long sessions.
            // The pages of one revision (`paging`) are kept together.
            if let Some(prefix) = paging::bitmap_prefix(&sprite.asset_key) {
                let revision = page_base(&sprite.asset_key);
                self.assets
                    .retain(|key, _| !key.starts_with(prefix) || page_base(key) == revision);
            }
            #[cfg(feature = "tracy")]
            let upload_zone = tracy_zone!("render.asset.upload");
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("sabiruby-rgss-bitmap"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &decoded,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * width),
                    rows_per_image: Some(height),
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
            if let Some(stats) = &mut self.frame_profile {
                stats.uploads += 1;
                stats.upload_bytes += decoded.len() as u64;
            }
            #[cfg(feature = "tracy")]
            drop(upload_zone);
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("sabiruby-rgss-bitmap-bind-group"),
                layout: &self.texture_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                }],
            });
            self.assets.insert(
                sprite.asset_key.clone(),
                TextureAsset {
                    _texture: texture,
                    bind_group,
                },
            );
        }
        self.assets
            .get(sprite.asset_key.as_ref())
            .map(|asset| asset.bind_group.clone())
            .ok_or_else(|| {
                format!(
                    "render asset {} was not inserted into the texture cache",
                    sprite.asset_key
                )
            })
    }
}

fn viewport_scissor(
    rect: Option<[i64; 4]>,
    logical_width: u32,
    logical_height: u32,
    output_width: u32,
    output_height: u32,
) -> Option<[u32; 4]> {
    let [x, y, width, height] = rect.unwrap_or([0, 0, logical_width.into(), logical_height.into()]);
    if width <= 0 || height <= 0 {
        return None;
    }
    let left = x.clamp(0, logical_width.into());
    let top = y.clamp(0, logical_height.into());
    let right = x.saturating_add(width).clamp(0, logical_width.into());
    let bottom = y.saturating_add(height).clamp(0, logical_height.into());
    if right <= left || bottom <= top {
        return None;
    }
    let sx = f64::from(output_width) / f64::from(logical_width.max(1));
    let sy = f64::from(output_height) / f64::from(logical_height.max(1));
    let x = (left as f64 * sx).floor() as u32;
    let y = (top as f64 * sy).floor() as u32;
    let right = ((right as f64 * sx).ceil() as u32).min(output_width);
    let bottom = ((bottom as f64 * sy).ceil() as u32).min(output_height);
    (right > x && bottom > y).then_some([x, y, right.saturating_sub(x), bottom.saturating_sub(y)])
}

fn append_vertices(vertices: &mut Vec<Vertex>, sprite: &RenderSprite, width: u32, height: u32) {
    if sprite.tile {
        for (x, y) in tile_positions(sprite, width, height) {
            vertices.extend_from_slice(&quad_vertices_at(sprite, x, y));
        }
    } else {
        vertices.extend_from_slice(&quad_vertices_at(
            sprite,
            sprite.x - sprite.ox,
            sprite.y - sprite.oy,
        ));
    }
}

fn tile_positions(
    sprite: &RenderSprite,
    logical_width: u32,
    logical_height: u32,
) -> Vec<(i64, i64)> {
    if !sprite.tile {
        return vec![(sprite.x - sprite.ox, sprite.y - sprite.oy)];
    }
    let tile_width = (sprite.src_width.max(1) as f64 * sprite.zoom_x.abs()).round() as i64;
    let tile_height = (sprite.src_height.max(1) as f64 * sprite.zoom_y.abs()).round() as i64;
    if tile_width <= 0 || tile_height <= 0 {
        return Vec::new();
    }
    let origin_x = sprite.x - sprite.ox;
    let origin_y = sprite.y - sprite.oy;
    let first_x = origin_x.rem_euclid(tile_width) - tile_width;
    let first_y = origin_y.rem_euclid(tile_height) - tile_height;
    let end_x = i64::from(logical_width).saturating_add(tile_width);
    let end_y = i64::from(logical_height).saturating_add(tile_height);
    let mut positions = Vec::new();
    let mut y = first_y;
    while y < end_y {
        let mut x = first_x;
        while x < end_x {
            positions.push((x, y));
            x = x.saturating_add(tile_width);
        }
        y = y.saturating_add(tile_height);
    }
    positions
}

fn quad_vertices_at(sprite: &RenderSprite, x: i64, y: i64) -> [Vertex; 6] {
    let x = x as f32;
    let y = y as f32;
    let width = (sprite.src_width.max(0) as f64 * sprite.zoom_x) as f32;
    let height = (sprite.src_height.max(0) as f64 * sprite.zoom_y) as f32;
    let bitmap_width = sprite.bitmap_width.max(1) as f32;
    let bitmap_height = sprite.bitmap_height.max(1) as f32;
    let mut u0 = sprite.src_x.max(0) as f32 / bitmap_width;
    let v0 = sprite.src_y.max(0) as f32 / bitmap_height;
    let mut u1 = (sprite.src_x + sprite.src_width).max(0) as f32 / bitmap_width;
    let v1 = (sprite.src_y + sprite.src_height).max(0) as f32 / bitmap_height;
    if sprite.mirror {
        std::mem::swap(&mut u0, &mut u1);
    }
    let alpha = f32::from(sprite.opacity) / 255.0;
    let color = sprite.color.map(|component| f32::from(component) / 255.0);
    let tone = sprite.tone.map(|component| f32::from(component) / 255.0);
    let origin_x = x + sprite.ox as f32;
    let origin_y = y + sprite.oy as f32;
    let angle = (sprite.angle as f32).to_radians();
    let (sin, cos) = angle.sin_cos();
    let rotate = |local_x: f32, local_y: f32| {
        let translated_x = local_x - sprite.ox as f32;
        let translated_y = local_y - sprite.oy as f32;
        [
            origin_x + translated_x * cos - translated_y * sin,
            origin_y + translated_x * sin + translated_y * cos,
        ]
    };
    let top_left = rotate(0.0, 0.0);
    let top_right = rotate(width, 0.0);
    let bottom_right = rotate(width, height);
    let bottom_left = rotate(0.0, height);
    [
        Vertex {
            position: top_left,
            uv: [u0, v0],
            alpha,
            color,
            tone,
        },
        Vertex {
            position: top_right,
            uv: [u1, v0],
            alpha,
            color,
            tone,
        },
        Vertex {
            position: bottom_right,
            uv: [u1, v1],
            alpha,
            color,
            tone,
        },
        Vertex {
            position: top_left,
            uv: [u0, v0],
            alpha,
            color,
            tone,
        },
        Vertex {
            position: bottom_right,
            uv: [u1, v1],
            alpha,
            color,
            tone,
        },
        Vertex {
            position: bottom_left,
            uv: [u0, v1],
            alpha,
            color,
            tone,
        },
    ]
}

const SPRITE_SHADER: &str = r#"
struct Screen { size: vec2<f32>, offset: vec2<f32> };
@group(0) @binding(0) var<uniform> screen: Screen;
@group(1) @binding(0) var sprite_texture: texture_2d<f32>;

struct VertexInput {
  @location(0) position: vec2<f32>,
  @location(1) uv: vec2<f32>,
  @location(2) alpha: f32,
  @location(3) color: vec4<f32>,
  @location(4) tone: vec4<f32>,
};
struct VertexOutput {
  @builtin(position) position: vec4<f32>,
  @location(0) uv: vec2<f32>,
  @location(1) alpha: f32,
  @location(2) color: vec4<f32>,
  @location(3) tone: vec4<f32>,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
  var output: VertexOutput;
  let ndc = vec2<f32>(
    (input.position.x + screen.offset.x) / screen.size.x * 2.0 - 1.0,
    1.0 - (input.position.y + screen.offset.y) / screen.size.y * 2.0
  );
  output.position = vec4<f32>(ndc, 0.0, 1.0);
  output.uv = input.uv;
  output.alpha = input.alpha;
  output.color = input.color;
  output.tone = input.tone;
  return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
  let dimensions = textureDimensions(sprite_texture);
  let max_x = i32(dimensions.x) - 1;
  let max_y = i32(dimensions.y) - 1;
  let texel = vec2<i32>(
    clamp(i32(floor(input.uv.x * f32(dimensions.x))), 0, max_x),
    clamp(i32(floor(input.uv.y * f32(dimensions.y))), 0, max_y)
  );
  let sampled = textureLoad(sprite_texture, texel, 0);
  let color_factor = clamp(input.color.a, 0.0, 1.0);
  var rgb = mix(sampled.rgb, input.color.rgb, color_factor);
  let tone_rgb = input.tone.rgb;
  rgb = clamp(rgb + tone_rgb, vec3<f32>(0.0), vec3<f32>(1.0));
  let gray = dot(rgb, vec3<f32>(0.299, 0.587, 0.114));
  rgb = mix(rgb, vec3<f32>(gray), clamp(input.tone.a, 0.0, 1.0));
  return vec4<f32>(rgb, sampled.a * input.alpha);
}
"#;

#[derive(Debug)]
pub enum RenderError {
    Recoverable(RenderStatus),
    Fatal(String),
}

#[derive(Clone, Copy, Debug)]
pub enum RenderStatus {
    Lost,
    Outdated,
    Timeout,
    Occluded,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioChannel {
    Bgm,
    Bgs,
    Me,
    Se,
}

pub struct AudioCommand {
    pub channel: AudioChannel,
    pub data: Arc<[u8]>,
    pub volume: u8,
    pub pitch: u16,
}

struct Playback {
    samples: Arc<[f32]>,
    position: f64,
    step: f64,
    volume: f32,
    looping: bool,
}

impl Playback {
    fn new(
        samples: Vec<f32>,
        source_rate: u32,
        output_rate: u32,
        volume: u8,
        pitch: u16,
        looping: bool,
    ) -> Self {
        Self {
            samples: Arc::from(samples.into_boxed_slice()),
            position: 0.0,
            step: f64::from(source_rate) / f64::from(output_rate)
                * (f64::from(pitch.max(1)) / 100.0),
            volume: f32::from(volume) / 100.0,
            looping,
        }
    }
}

#[derive(Default)]
struct Mixer {
    bgm: Option<Playback>,
    bgs: Option<Playback>,
    me: Option<Playback>,
    se: Option<Playback>,
}

impl Mixer {
    fn stop(&mut self, channel: AudioChannel) {
        match channel {
            AudioChannel::Bgm => self.bgm = None,
            AudioChannel::Bgs => self.bgs = None,
            AudioChannel::Me => self.me = None,
            AudioChannel::Se => self.se = None,
        }
    }

    fn play(&mut self, channel: AudioChannel, playback: Playback) {
        match channel {
            AudioChannel::Bgm => self.bgm = Some(playback),
            AudioChannel::Bgs => self.bgs = Some(playback),
            AudioChannel::Me => self.me = Some(playback),
            AudioChannel::Se => self.se = Some(playback),
        }
    }

    fn next_from(slot: &mut Option<Playback>) -> [f32; 2] {
        let Some(playback) = slot.as_mut() else {
            return [0.0; 2];
        };
        // Decoders normalize mono/stereo assets to interleaved stereo. Position
        // and step count frames, so consuming L and R cannot double duration.
        let frames = playback.samples.len() / 2;
        if frames == 0 {
            *slot = None;
            return [0.0; 2];
        }
        if playback.position >= frames as f64 {
            if playback.looping {
                playback.position %= frames as f64;
            } else {
                *slot = None;
                return [0.0; 2];
            }
        }
        let index = playback.position as usize * 2;
        let frame = [
            playback.samples[index] * playback.volume,
            playback.samples[index + 1] * playback.volume,
        ];
        playback.position += playback.step;
        frame
    }

    fn next_frame(&mut self) -> [f32; 2] {
        let mut frame = [0.0f32; 2];
        for source in [
            Self::next_from(&mut self.bgm),
            Self::next_from(&mut self.bgs),
            Self::next_from(&mut self.me),
            Self::next_from(&mut self.se),
        ] {
            for channel in 0..2 {
                frame[channel] += source[channel];
            }
        }
        frame.map(|sample| sample.clamp(-1.0, 1.0))
    }
}

pub struct AudioOutput {
    _stream: cpal::Stream,
    mixer: Arc<Mutex<Mixer>>,
    sample_rate: u32,
}

impl AudioOutput {
    /// Open the default output and connect a small decoded-file mixer.  The
    /// device stays in this crate; the RGSS core only emits portable commands.
    pub fn open() -> Result<Self, String> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| "no default output device".to_owned())?;
        let supported = device
            .default_output_config()
            .map_err(|error| format!("default output config: {error}"))?;
        let format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();
        let sample_rate = config.sample_rate;
        let channels = usize::from(config.channels);
        let mixer = Arc::new(Mutex::new(Mixer::default()));
        let callback_mixer = mixer.clone();
        let stream = match format {
            SampleFormat::F32 => device.build_output_stream(
                config.clone(),
                move |data: &mut [f32], info| write_mixed(data, info, &callback_mixer, channels),
                audio_error,
                None,
            ),
            SampleFormat::I16 => device.build_output_stream(
                config.clone(),
                move |data: &mut [i16], info| write_mixed(data, info, &callback_mixer, channels),
                audio_error,
                None,
            ),
            SampleFormat::U16 => device.build_output_stream(
                config.clone(),
                move |data: &mut [u16], info| write_mixed(data, info, &callback_mixer, channels),
                audio_error,
                None,
            ),
            SampleFormat::I8 => device.build_output_stream(
                config.clone(),
                move |data: &mut [i8], info| write_mixed(data, info, &callback_mixer, channels),
                audio_error,
                None,
            ),
            SampleFormat::U8 => device.build_output_stream(
                config,
                move |data: &mut [u8], info| write_mixed(data, info, &callback_mixer, channels),
                audio_error,
                None,
            ),
            format => return Err(format!("unsupported output sample format {format}")),
        }
        .map_err(|error| format!("build output stream: {error}"))?;
        stream
            .play()
            .map_err(|error| format!("start output stream: {error}"))?;
        Ok(Self {
            _stream: stream,
            mixer,
            sample_rate,
        })
    }

    pub fn submit(&self, command: AudioCommand) {
        if command.data.is_empty() {
            if let Ok(mut mixer) = self.mixer.lock() {
                mixer.stop(command.channel);
            }
            return;
        }
        #[cfg(feature = "tracy")]
        let _decode_zone = tracy_zone!("audio.decode");
        let Some((samples, source_rate)) = decode_audio(&command.data) else {
            eprintln!("audio: unsupported or invalid asset");
            return;
        };
        let playback = Playback::new(
            samples,
            source_rate,
            self.sample_rate,
            command.volume,
            command.pitch,
            matches!(command.channel, AudioChannel::Bgm | AudioChannel::Bgs),
        );
        if let Ok(mut mixer) = self.mixer.lock() {
            mixer.play(command.channel, playback);
        }
    }
}

fn write_mixed<T: SizedSample + FromSample<f32>>(
    data: &mut [T],
    _: &cpal::OutputCallbackInfo,
    mixer: &Arc<Mutex<Mixer>>,
    channels: usize,
) {
    #[cfg(feature = "tracy")]
    let _zone = tracy_zone!("audio.callback");
    mix_into(data, mixer, channels);
}

fn mix_into<T: SizedSample + FromSample<f32>>(
    data: &mut [T],
    mixer: &Arc<Mutex<Mixer>>,
    channels: usize,
) {
    let mut guard = mixer.lock().ok();
    let channels = channels.max(1);
    for frame in data.chunks_mut(channels) {
        let stereo = guard.as_mut().map_or([0.0; 2], |mixer| mixer.next_frame());
        if channels == 1 {
            frame[0] = T::from_sample((stereo[0] + stereo[1]) * 0.5);
        } else {
            for (channel, output) in frame.iter_mut().enumerate() {
                *output = T::from_sample(stereo.get(channel).copied().unwrap_or(0.0));
            }
        }
    }
}

fn audio_error(error: cpal::Error) {
    eprintln!("audio stream: {error}");
}

fn decode_audio(bytes: &[u8]) -> Option<(Vec<f32>, u32)> {
    if bytes.starts_with(b"OggS") {
        let mut reader = OggStreamReader::new(Cursor::new(bytes)).ok()?;
        let sample_rate = reader.ident_hdr.audio_sample_rate;
        let channels = usize::from(reader.ident_hdr.audio_channels.max(1));
        let mut samples = Vec::new();
        while let Some(packet) = reader.read_dec_packet_itl().ok()? {
            for frame in packet.chunks(channels) {
                let left = f32::from(frame[0]) / 32_768.0;
                let right = if channels > 1 {
                    f32::from(frame[1]) / 32_768.0
                } else {
                    left
                };
                samples.push(left);
                samples.push(right);
            }
        }
        return Some((samples, sample_rate));
    }
    decode_wav(bytes)
}

fn decode_wav(bytes: &[u8]) -> Option<(Vec<f32>, u32)> {
    if bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }
    let mut cursor = 12usize;
    let mut channels = 0u16;
    let mut sample_rate = 0u32;
    let mut bits = 0u16;
    let mut data = &[][..];
    while cursor + 8 <= bytes.len() {
        let size = u32::from_le_bytes(bytes[cursor + 4..cursor + 8].try_into().ok()?) as usize;
        let start = cursor + 8;
        let end = start.checked_add(size)?;
        if end > bytes.len() {
            return None;
        }
        match &bytes[cursor..cursor + 4] {
            b"fmt " if size >= 16 => {
                let format = u16::from_le_bytes(bytes[start..start + 2].try_into().ok()?);
                channels = u16::from_le_bytes(bytes[start + 2..start + 4].try_into().ok()?);
                sample_rate = u32::from_le_bytes(bytes[start + 4..start + 8].try_into().ok()?);
                bits = u16::from_le_bytes(bytes[start + 14..start + 16].try_into().ok()?);
                if format != 1 {
                    return None;
                }
            }
            b"data" => data = &bytes[start..end],
            _ => {}
        }
        cursor = end + (size & 1);
    }
    if channels == 0 || sample_rate == 0 || data.is_empty() || bits != 16 {
        return None;
    }
    let channels = usize::from(channels);
    let mut samples = Vec::with_capacity(data.len() / 2 / channels * 2);
    for frame in data.chunks_exact(2 * channels) {
        let left = f32::from(i16::from_le_bytes([frame[0], frame[1]])) / 32_768.0;
        let right = if channels > 1 {
            f32::from(i16::from_le_bytes([frame[2], frame[3]])) / 32_768.0
        } else {
            left
        };
        samples.push(left);
        samples.push(right);
    }
    Some((samples, sample_rate))
}

#[cfg(test)]
mod audio_tests {
    use super::*;

    #[test]
    fn viewport_clip_scales_to_the_surface_and_intersects_screen_bounds() {
        assert_eq!(
            viewport_scissor(Some([10, 20, 30, 40]), 512, 384, 1024, 768),
            Some([20, 40, 60, 80])
        );
        assert_eq!(
            viewport_scissor(Some([-10, -20, 30, 40]), 512, 384, 1024, 768),
            Some([0, 0, 40, 40])
        );
        assert_eq!(
            viewport_scissor(None, 512, 384, 1024, 768),
            Some([0, 0, 1024, 768])
        );
        assert_eq!(
            viewport_scissor(Some([600, 20, 30, 40]), 512, 384, 1024, 768),
            None
        );
        assert_eq!(
            viewport_scissor(Some([10, 20, 0, 40]), 512, 384, 1024, 768),
            None
        );
    }

    #[test]
    fn stereo_channels_share_a_frame_and_keep_the_original_duration() {
        let mut slot = Some(Playback::new(
            vec![0.25, -0.5, 0.75, -1.0],
            48_000,
            48_000,
            100,
            100,
            false,
        ));
        assert_eq!(Mixer::next_from(&mut slot), [0.25, -0.5]);
        assert_eq!(Mixer::next_from(&mut slot), [0.75, -1.0]);
        assert_eq!(Mixer::next_from(&mut slot), [0.0, 0.0]);
        assert!(slot.is_none());
    }

    #[test]
    fn resampling_and_rgss_pitch_preserve_frame_duration() {
        for (output_rate, pitch, expected_frames) in [
            (48_000, 100, 8),
            (24_000, 100, 4),
            (48_000, 200, 4),
            (48_000, 50, 16),
        ] {
            let mut slot = Some(Playback::new(
                vec![0.5; 8],
                24_000,
                output_rate,
                100,
                pitch,
                false,
            ));
            for _ in 0..expected_frames {
                assert_eq!(Mixer::next_from(&mut slot), [0.5; 2]);
            }
            assert_eq!(Mixer::next_from(&mut slot), [0.0; 2]);
            assert!(slot.is_none());
        }
    }

    #[test]
    fn looping_stereo_restarts_on_a_frame_boundary() {
        let mut slot = Some(Playback::new(
            vec![0.25, -0.5, 0.75, -1.0],
            48_000,
            48_000,
            50,
            100,
            true,
        ));
        for _ in 0..3 {
            assert_eq!(Mixer::next_from(&mut slot), [0.125, -0.25]);
            assert_eq!(Mixer::next_from(&mut slot), [0.375, -0.5]);
        }
    }

    #[test]
    fn output_keeps_stereo_and_downmixes_for_mono_devices() {
        let mixer = Arc::new(Mutex::new(Mixer::default()));
        for channels in [1, 2, 6] {
            mixer.lock().unwrap().play(
                AudioChannel::Se,
                Playback::new(vec![0.75, -0.25], 48_000, 48_000, 100, 100, false),
            );
            let mut output = vec![99.0f32; channels * 2];
            mix_into(&mut output, &mixer, channels);
            if channels == 1 {
                assert_eq!(output[0], 0.25);
            } else {
                assert_eq!(&output[..2], &[0.75, -0.25]);
            }
            assert!(output[2..].iter().all(|sample| *sample == 0.0));
            assert_eq!(output[channels], 0.0);
        }
    }

    #[test]
    fn mono_and_stereo_wav_decode_to_stereo_frames() {
        for channels in [1u16, 2] {
            let values: Vec<i16> = if channels == 1 {
                vec![8192, -16384]
            } else {
                vec![8192, -8192, 16384, -16384]
            };
            let len = (values.len() * 2) as u32;
            let mut wav = b"RIFF".to_vec();
            wav.extend_from_slice(&(36 + len).to_le_bytes());
            wav.extend_from_slice(b"WAVEfmt ");
            wav.extend_from_slice(&16u32.to_le_bytes());
            wav.extend_from_slice(&1u16.to_le_bytes());
            wav.extend_from_slice(&channels.to_le_bytes());
            wav.extend_from_slice(&24_000u32.to_le_bytes());
            wav.extend_from_slice(&(24_000u32 * u32::from(channels) * 2).to_le_bytes());
            wav.extend_from_slice(&(channels * 2).to_le_bytes());
            wav.extend_from_slice(&16u16.to_le_bytes());
            wav.extend_from_slice(b"data");
            wav.extend_from_slice(&len.to_le_bytes());
            for value in values {
                wav.extend_from_slice(&value.to_le_bytes());
            }
            let (samples, rate) = decode_audio(&wav).unwrap();
            assert_eq!(rate, 24_000);
            assert_eq!(
                samples,
                if channels == 1 {
                    vec![0.25, 0.25, -0.5, -0.5]
                } else {
                    vec![0.25, -0.25, 0.5, -0.5]
                }
            );
        }
    }
}
