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
use wgpu::CurrentSurfaceTexture;
use wgpu::util::DeviceExt;
use winit::dpi::PhysicalSize;
use winit::window::Window;

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    size: PhysicalSize<u32>,
    pipeline: wgpu::RenderPipeline,
    screen_buffer: wgpu::Buffer,
    screen_bind_group: wgpu::BindGroup,
    texture_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    assets: HashMap<String, TextureAsset>,
    profiling: bool,
    frame_profile: Option<RenderProfile>,
}

/// CPU/API timings only. Submission does not wait for GPU execution.
#[derive(Clone, Debug, Default)]
pub struct RenderProfile {
    pub acquire_ns: u64,
    pub prepare_ns: u64,
    pub encode_ns: u64,
    pub submit_ns: u64,
    pub present_ns: u64,
    pub draw_calls: u64,
    pub uploads: u64,
    pub upload_bytes: u64,
}

/// A renderer-owned projection of an RGSS Sprite.  The core does not depend
/// on WGPU; the player converts its `RenderSnapshot` into these values.
#[derive(Clone, Debug)]
pub struct SpriteFrame {
    pub asset_key: String,
    pub image: Arc<[u8]>,
    /// `Some` means `image` is already tightly packed RGBA8 rather than an
    /// encoded PNG/GIF/JPEG asset.
    pub raw_rgba: Option<(u32, u32)>,
    pub bitmap_width: u32,
    pub bitmap_height: u32,
    pub src_x: i64,
    pub src_y: i64,
    pub src_width: i64,
    pub src_height: i64,
    pub x: i64,
    pub y: i64,
    pub ox: i64,
    pub oy: i64,
    pub zoom_x: f64,
    pub zoom_y: f64,
    pub opacity: u8,
    pub angle: f64,
    pub mirror: bool,
    pub color: [u8; 4],
    pub tone: [i16; 4],
    pub blend_type: i64,
    pub visible: bool,
    pub z: i64,
    pub tile: bool,
    pub clip_rect: Option<[i64; 4]>,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ScreenUniform {
    size: [f32; 2],
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
    view: wgpu::TextureView,
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
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("sabiruby-rgss-sprite-sampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
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
            texture_layout,
            sampler,
            assets: HashMap::new(),
            profiling: false,
            frame_profile: None,
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
    pub fn render_frame(&mut self, sprites: &[SpriteFrame]) -> Result<(), RenderError> {
        self.render_frame_sized(sprites, self.size.width, self.size.height)
    }

    /// Draw using the game's logical RGSS resolution. The surface may be
    /// larger after a window resize; Ruby coordinates remain unchanged.
    pub fn render_frame_sized(
        &mut self,
        sprites: &[SpriteFrame],
        logical_width: u32,
        logical_height: u32,
    ) -> Result<(), RenderError> {
        self.frame_profile = self.profiling.then(RenderProfile::default);
        let mut phase = self.profiling.then(Instant::now);
        if self.size.width == 0 || self.size.height == 0 {
            return Ok(());
        }
        let output = self.surface.get_current_texture();
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
            }),
        );
        let mut draws = Vec::new();
        for sprite in sprites
            .iter()
            .filter(|sprite| sprite.visible && sprite.opacity > 0)
        {
            let Some(scissor) = viewport_scissor(
                sprite.clip_rect,
                logical_width,
                logical_height,
                self.config.width,
                self.config.height,
            ) else {
                continue;
            };
            let asset_view = match self.ensure_asset(sprite) {
                Ok(asset) => asset,
                Err(error) => {
                    // A malformed or unsupported bitmap must not terminate the
                    // game while transferring to a new map.  Keep rendering
                    // the remaining sprites and leave a diagnostic for the
                    // offending asset.
                    eprintln!("skipping sprite {}: {error}", sprite.asset_key);
                    continue;
                }
            };
            for (x, y) in tile_positions(sprite, logical_width, logical_height) {
                let vertices = quad_vertices_at(sprite, x, y);
                let vertex_buffer = device_buffer(&self.device, &vertices);
                let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("sabiruby-rgss-sprite-bind-group"),
                    layout: &self.texture_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&asset_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&self.sampler),
                        },
                    ],
                });
                draws.push((vertex_buffer, bind_group, scissor));
            }
        }
        if let (Some(stats), Some(start)) = (&mut self.frame_profile, phase) {
            stats.prepare_ns = start.elapsed().as_nanos() as u64;
            stats.draw_calls = draws.len() as u64;
        }
        if self.profiling {
            phase = Some(Instant::now());
        }
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
            pass.set_bind_group(0, &self.screen_bind_group, &[]);
            for (vertex_buffer, bind_group, scissor) in &draws {
                pass.set_scissor_rect(scissor[0], scissor[1], scissor[2], scissor[3]);
                pass.set_bind_group(1, bind_group, &[]);
                pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                pass.draw(0..6, 0..1);
            }
        }
        if let (Some(stats), Some(start)) = (&mut self.frame_profile, phase) {
            stats.encode_ns = start.elapsed().as_nanos() as u64;
        }
        if self.profiling {
            phase = Some(Instant::now());
        }
        self.queue.submit(Some(encoder.finish()));
        if let (Some(stats), Some(start)) = (&mut self.frame_profile, phase) {
            stats.submit_ns = start.elapsed().as_nanos() as u64;
        }
        if self.profiling {
            phase = Some(Instant::now());
        }
        self.queue.present(output);
        if let (Some(stats), Some(start)) = (&mut self.frame_profile, phase) {
            stats.present_ns = start.elapsed().as_nanos() as u64;
        }
        Ok(())
    }

    fn ensure_asset(&mut self, sprite: &SpriteFrame) -> Result<wgpu::TextureView, String> {
        if !self.assets.contains_key(&sprite.asset_key) {
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
            if let Some(bitmap_id) = sprite
                .asset_key
                .strip_prefix("bitmap:")
                .and_then(|key| key.split_once(':').map(|(id, _)| id))
            {
                let prefix = format!("bitmap:{bitmap_id}:");
                self.assets
                    .retain(|key, _| !key.starts_with(prefix.as_str()));
            }
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
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            self.assets.insert(
                sprite.asset_key.clone(),
                TextureAsset {
                    _texture: texture,
                    view,
                },
            );
        }
        self.assets
            .get(&sprite.asset_key)
            .map(|asset| asset.view.clone())
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

fn device_buffer(device: &wgpu::Device, vertices: &[Vertex]) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("sabiruby-rgss-sprite-vertices"),
        contents: bytemuck::cast_slice(vertices),
        usage: wgpu::BufferUsages::VERTEX,
    })
}

fn tile_positions(
    sprite: &SpriteFrame,
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

fn quad_vertices_at(sprite: &SpriteFrame, x: i64, y: i64) -> [Vertex; 6] {
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
struct Screen { size: vec2<f32> };
@group(0) @binding(0) var<uniform> screen: Screen;
@group(1) @binding(0) var sprite_texture: texture_2d<f32>;
@group(1) @binding(1) var sprite_sampler: sampler;

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
    input.position.x / screen.size.x * 2.0 - 1.0,
    1.0 - input.position.y / screen.size.y * 2.0
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
  let sampled = textureSample(sprite_texture, sprite_sampler, input.uv);
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
