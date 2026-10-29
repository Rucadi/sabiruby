//! Optional desktop services for the headless RGSS core.
//!
//! This crate deliberately contains no Ruby or game policy. It only wraps a
//! WGPU surface and a CPAL output stream so an embedding application can own
//! the event loop, pacing, input mapping, and audio mixer.

use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Arc;
use std::sync::Mutex;

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
        })
    }

    pub fn size(&self) -> PhysicalSize<u32> {
        self.size
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
        if self.size.width == 0 || self.size.height == 0 {
            return Ok(());
        }
        let output = match self.surface.get_current_texture() {
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
            if self.ensure_asset(sprite).is_none() {
                continue;
            }
            let Some(asset) = self.assets.get(&sprite.asset_key) else {
                continue;
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
                            resource: wgpu::BindingResource::TextureView(&asset.view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&self.sampler),
                        },
                    ],
                });
                draws.push((vertex_buffer, bind_group));
            }
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("sabiruby-rgss-frame"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.025,
                            g: 0.035,
                            b: 0.055,
                            a: 1.0,
                        }),
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
            for (vertex_buffer, bind_group) in &draws {
                pass.set_bind_group(1, bind_group, &[]);
                pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                pass.draw(0..6, 0..1);
            }
        }
        self.queue.submit(Some(encoder.finish()));
        self.queue.present(output);
        Ok(())
    }

    fn ensure_asset(&mut self, sprite: &SpriteFrame) -> Option<&TextureAsset> {
        if !self.assets.contains_key(&sprite.asset_key) {
            let (decoded, width, height) = if let Some((width, height)) = sprite.raw_rgba {
                if sprite.image.len()
                    < (width as usize)
                        .saturating_mul(height as usize)
                        .saturating_mul(4)
                {
                    return None;
                }
                (sprite.image.to_vec(), width, height)
            } else {
                let decoded = image::load_from_memory(&sprite.image).ok()?.to_rgba8();
                let (width, height) = decoded.dimensions();
                (decoded.into_raw(), width, height)
            };
            if width == 0 || height == 0 {
                return None;
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
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            self.assets.insert(
                sprite.asset_key.clone(),
                TextureAsset {
                    _texture: texture,
                    view,
                },
            );
        }
        self.assets.get(&sprite.asset_key)
    }
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

    fn next_from(slot: &mut Option<Playback>) -> f32 {
        let Some(playback) = slot.as_mut() else {
            return 0.0;
        };
        if playback.samples.is_empty() {
            *slot = None;
            return 0.0;
        }
        if playback.position >= playback.samples.len() as f64 {
            if playback.looping {
                playback.position %= playback.samples.len() as f64;
            } else {
                *slot = None;
                return 0.0;
            }
        }
        let index = playback.position as usize;
        let sample = playback.samples[index] * playback.volume;
        playback.position += playback.step;
        sample
    }

    fn next_sample(&mut self) -> f32 {
        (Self::next_from(&mut self.bgm)
            + Self::next_from(&mut self.bgs)
            + Self::next_from(&mut self.me)
            + Self::next_from(&mut self.se))
        .clamp(-1.0, 1.0)
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
        let pitch = f64::from(command.pitch.max(1)) / 100.0;
        let playback = Playback {
            samples: Arc::from(samples.into_boxed_slice()),
            position: 0.0,
            step: (f64::from(source_rate) / f64::from(self.sample_rate)) * pitch,
            volume: f32::from(command.volume) / 100.0,
            looping: matches!(command.channel, AudioChannel::Bgm | AudioChannel::Bgs),
        };
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
    let mut guard = mixer.lock().ok();
    let channels = channels.max(1);
    for frame in data.chunks_mut(channels) {
        let sample = guard.as_mut().map_or(0.0, |mixer| mixer.next_sample());
        for output in frame {
            *output = T::from_sample(sample);
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
