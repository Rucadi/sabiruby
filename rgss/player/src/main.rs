//! Thin desktop adapter for the reusable RGSS core.
//!
//! The VM and RGSS compatibility layer do not know about WGPU, winit, or an
//! audio device. This binary owns those services and advances the suspended
//! game script from the window event loop.

use std::collections::HashMap;
use std::env;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use sabiruby::Step;
use sabiruby_rgss_core::{
    AudioChannel as CoreAudioChannel, AudioCommand as CoreAudioCommand, Project, RenderSnapshot,
    RgssRuntime,
};
use sabiruby_rgss_wgpu::{
    AudioChannel, AudioCommand, AudioOutput, RenderError, RenderStatus, Renderer, SpriteFrame,
};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

#[derive(Clone, Debug)]
struct Options {
    root: PathBuf,
    script_limit: Option<usize>,
    frame_limit: Option<u64>,
    step_budget: u64,
    window: bool,
    fps_unlocked: bool,
    fps_limit: Option<u32>,
    auto_z: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            root: PathBuf::from("anil"),
            script_limit: None,
            frame_limit: None,
            step_budget: 250_000,
            window: true,
            fps_unlocked: false,
            fps_limit: Some(60),
            auto_z: false,
        }
    }
}

fn parse_options() -> Result<Options, String> {
    let mut options = Options::default();
    let mut mode_explicit = false;
    let mut args = env::args_os().skip(1);
    let mut positional = Vec::new();
    while let Some(arg) = args.next() {
        let text = arg
            .into_string()
            .map_err(|_| "arguments must be valid UTF-8".to_owned())?;
        match text.as_str() {
            "--window" => {
                options.window = true;
                mode_explicit = true;
            }
            "--headless" => {
                options.window = false;
                mode_explicit = true;
            }
            "--fps-unlocked" => options.fps_unlocked = true,
            "--auto-z" => options.auto_z = true,
            "--script-limit" => {
                options.script_limit = Some(parse_next(&mut args, "--script-limit")?);
            }
            "--frame-limit" => {
                options.frame_limit = Some(parse_next(&mut args, "--frame-limit")?);
            }
            "--step-budget" => {
                options.step_budget = parse_next::<u64>(&mut args, "--step-budget")?.max(1);
            }
            "--fps" => {
                options.fps_limit = Some(parse_next::<u32>(&mut args, "--fps")?.max(1));
            }
            "--help" | "-h" => return Err(usage().to_owned()),
            value if value.starts_with('-') => {
                return Err(format!("unknown option {value}\n\n{}", usage()));
            }
            value => positional.push(value.to_owned()),
        }
    }
    if let Some(root) = positional.first() {
        options.root = PathBuf::from(root);
    }
    // Keep the original probe's positional form working: ROOT SCRIPT_LIMIT FRAME_LIMIT.
    if let Some(value) = positional.get(1) {
        options.script_limit = Some(
            value
                .parse()
                .map_err(|_| format!("invalid script limit: {value}"))?,
        );
    }
    if let Some(value) = positional.get(2) {
        options.frame_limit = Some(
            value
                .parse()
                .map_err(|_| format!("invalid frame limit: {value}"))?,
        );
    }
    if !mode_explicit && positional.len() > 1 {
        options.window = false;
    }
    Ok(options)
}

fn parse_next<T: std::str::FromStr>(
    args: &mut impl Iterator<Item = std::ffi::OsString>,
    name: &str,
) -> Result<T, String> {
    let value = args
        .next()
        .ok_or_else(|| format!("{name} needs a value\n\n{}", usage()))?;
    let value = value
        .into_string()
        .map_err(|_| format!("{name} value must be valid UTF-8"))?;
    value
        .parse()
        .map_err(|_| format!("invalid value for {name}: {value}"))
}

fn usage() -> &'static str {
    "usage: sabiruby-rgss-player [ROOT] [SCRIPT_LIMIT] [FRAME_LIMIT] [options]\n\
     windowed mode is the default; use --headless for a bounded boot probe\n\
     options: --window --headless --script-limit N --frame-limit N\n\
              --step-budget N --fps N --fps-unlocked --auto-z"
}

fn main() {
    let options = match parse_options() {
        Ok(options) => options,
        Err(error) if error == usage() => {
            println!("{error}");
            return;
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    let project = match Project::open(&options.root) {
        Ok(project) => project,
        Err(error) => exit_with_error(error),
    };
    eprintln!(
        "SabiRuby RGSS player: {} ({}x{})",
        project.title, project.screen_width, project.screen_height
    );
    let mut runtime = match RgssRuntime::open(project.clone()) {
        Ok(runtime) => runtime,
        Err(error) => exit_with_error(error),
    };

    if options.window {
        if let Err(error) = run_windowed(runtime, project, options) {
            exit_with_error(error);
        }
    } else {
        match run_headless(&mut runtime, &options) {
            Ok(count) => {
                print_output(&mut runtime);
                eprintln!("executed {count} scripts");
            }
            Err(error) => {
                print_output(&mut runtime);
                exit_with_error(error);
            }
        }
    }
}

fn run_headless(runtime: &mut RgssRuntime, options: &Options) -> Result<usize, String> {
    let count = runtime
        .start_scripts(options.script_limit)
        .map_err(|error| format!("boot failed: {error}"))?;
    let target = options.frame_limit.unwrap_or(1);
    while runtime.frame_count() < target {
        match runtime
            .step(options.step_budget)
            .map_err(|error| format!("game stopped: {error}"))?
        {
            Step::Paused => {}
            Step::Finished(_) => break,
        }
    }
    Ok(count)
}

fn exit_with_error(error: impl std::fmt::Display) -> ! {
    eprintln!("{error}");
    std::process::exit(1);
}

fn print_output(runtime: &mut RgssRuntime) {
    print!("{}", String::from_utf8_lossy(&runtime.take_output()));
}

fn run_windowed(
    mut runtime: RgssRuntime,
    project: Project,
    options: Options,
) -> Result<(), String> {
    runtime
        .start_scripts(options.script_limit)
        .map_err(|error| format!("boot failed: {error}"))?;
    let title = project.title.clone();
    let event_loop = EventLoop::new().map_err(|error| format!("event loop: {error}"))?;
    let mut app = App::new(runtime, project, options, title);
    event_loop
        .run_app(&mut app)
        .map_err(|error| format!("event loop: {error}"))
}

struct App {
    runtime: RgssRuntime,
    project: Project,
    options: Options,
    title: String,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    audio: Option<AudioOutput>,
    next_frame: Instant,
    fps_window: Instant,
    rendered_frames: u32,
    asset_cache: HashMap<String, Arc<[u8]>>,
}

impl App {
    fn new(runtime: RgssRuntime, project: Project, options: Options, title: String) -> Self {
        let now = Instant::now();
        Self {
            runtime,
            project,
            options,
            title,
            window: None,
            renderer: None,
            audio: None,
            next_frame: now,
            fps_window: now,
            rendered_frames: 0,
            asset_cache: HashMap::new(),
        }
    }

    fn sprite_frames(&mut self, snapshot: &RenderSnapshot) -> Vec<SpriteFrame> {
        let mut frames = Vec::new();
        for sprite in &snapshot.sprites {
            let (asset_key, image, raw_rgba) = if let Some(path) = sprite.bitmap_path.as_ref() {
                if !self.asset_cache.contains_key(path) {
                    let Some(bytes) = self.runtime.read_asset(path) else {
                        continue;
                    };
                    self.asset_cache
                        .insert(path.clone(), Arc::from(bytes.into_boxed_slice()));
                }
                let Some(image) = self.asset_cache.get(path).cloned() else {
                    continue;
                };
                (path.clone(), image, None)
            } else if let Some(image) = sprite.bitmap_rgba.as_ref() {
                (
                    format!("bitmap:{}:{}", sprite.bitmap_id, sprite.bitmap_revision),
                    image.clone(),
                    Some((sprite.bitmap_width, sprite.bitmap_height)),
                )
            } else {
                continue;
            };
            frames.push(SpriteFrame {
                asset_key,
                image,
                raw_rgba,
                bitmap_width: sprite.bitmap_width,
                bitmap_height: sprite.bitmap_height,
                src_x: sprite.src_x,
                src_y: sprite.src_y,
                src_width: sprite.src_width,
                src_height: sprite.src_height,
                x: sprite.x,
                y: sprite.y,
                ox: sprite.ox,
                oy: sprite.oy,
                zoom_x: sprite.zoom_x,
                zoom_y: sprite.zoom_y,
                opacity: sprite.opacity,
                angle: sprite.angle,
                mirror: sprite.mirror,
                color: sprite.color,
                tone: sprite.tone,
                blend_type: sprite.blend_type,
                visible: sprite.visible,
                z: sprite.z,
                tile: sprite.tile,
            });
        }
        frames
    }

    fn dispatch_audio(&mut self) {
        let commands = self.runtime.take_audio_commands();
        let Some(audio) = self.audio.as_ref() else {
            return;
        };
        for command in commands {
            let CoreAudioCommand {
                channel,
                path,
                volume,
                pitch,
            } = command;
            let data = if path.is_empty() {
                Arc::<[u8]>::from([])
            } else {
                let Some(bytes) = self.runtime.read_asset(&path) else {
                    eprintln!("audio asset not found: {path}");
                    continue;
                };
                Arc::from(bytes.into_boxed_slice())
            };
            audio.submit(AudioCommand {
                channel: match channel {
                    CoreAudioChannel::Bgm => AudioChannel::Bgm,
                    CoreAudioChannel::Bgs => AudioChannel::Bgs,
                    CoreAudioChannel::Me => AudioChannel::Me,
                    CoreAudioChannel::Se => AudioChannel::Se,
                },
                data,
                volume,
                pitch,
            });
        }
    }

    fn create_window(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        let attributes = Window::default_attributes()
            .with_title(self.title.clone())
            .with_inner_size(LogicalSize::new(
                f64::from(self.project.screen_width),
                f64::from(self.project.screen_height),
            ));
        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .map_err(|error| format!("window: {error}"))?,
        );
        let renderer = if self.options.fps_unlocked {
            pollster::block_on(Renderer::new_unlocked(window.clone()))?
        } else {
            pollster::block_on(Renderer::new(window.clone()))?
        };
        self.audio = match AudioOutput::open() {
            Ok(audio) => Some(audio),
            Err(error) => {
                eprintln!("audio disabled: {error}");
                None
            }
        };
        self.renderer = Some(renderer);
        self.window = Some(window);
        Ok(())
    }

    fn step_and_render(&mut self, event_loop: &ActiveEventLoop) {
        let result = self.runtime.step(self.options.step_budget);
        if let Err(error) = result {
            print_output(&mut self.runtime);
            eprintln!("game stopped: {error}");
            event_loop.exit();
            return;
        }
        if self.options.auto_z {
            let frame = self.runtime.frame_count();
            if frame % 45 == 1 {
                self.runtime.input_press(1);
                self.runtime.input_press(24);
            } else if frame % 45 == 2 {
                self.runtime.input_release(1);
                self.runtime.input_release(24);
            }
        }
        if matches!(result, Ok(Step::Finished(_))) {
            print_output(&mut self.runtime);
            event_loop.exit();
            return;
        }
        let snapshot = self.runtime.render_snapshot();
        let sprites = self.sprite_frames(&snapshot);
        self.dispatch_audio();
        if let Some(renderer) = self.renderer.as_mut() {
            match renderer.render_frame_sized(&sprites, snapshot.width, snapshot.height) {
                Ok(()) => {}
                Err(RenderError::Recoverable(RenderStatus::Lost | RenderStatus::Outdated)) => {
                    renderer.resize(renderer.size());
                }
                Err(RenderError::Recoverable(RenderStatus::Timeout | RenderStatus::Occluded)) => {}
                Err(RenderError::Fatal(error)) => {
                    eprintln!("rendering stopped: {error}");
                    event_loop.exit();
                    return;
                }
            }
        }
        self.rendered_frames = self.rendered_frames.saturating_add(1);
        let now = Instant::now();
        if now.duration_since(self.fps_window) >= Duration::from_secs(1) {
            if let Some(window) = self.window.as_ref() {
                let fps = self.rendered_frames;
                window.set_title(&format!("{} — {fps} FPS", self.title));
            }
            self.rendered_frames = 0;
            self.fps_window = now;
        }
        if let Some(limit) = self.options.fps_limit {
            self.next_frame = now + Duration::from_secs_f64(1.0 / f64::from(limit));
        }
    }

    fn handle_key(&mut self, state: ElementState, physical_key: PhysicalKey) {
        let Some(codes) = rgss_key_codes(physical_key) else {
            return;
        };
        for code in codes {
            match state {
                ElementState::Pressed => self.runtime.input_press(*code),
                ElementState::Released => self.runtime.input_release(*code),
            }
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        if let Err(error) = self.create_window(event_loop) {
            eprintln!("startup failed: {error}");
            event_loop.exit();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(window) = self.window.as_ref() else {
            return;
        };
        if window.id() != window_id {
            return;
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.resize(size);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                self.handle_key(event.state, event.physical_key);
            }
            WindowEvent::RedrawRequested => self.step_and_render(event_loop),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(window) = self.window.as_ref() {
            if self.options.fps_unlocked || self.options.fps_limit.is_none() {
                event_loop.set_control_flow(ControlFlow::Poll);
                window.request_redraw();
            } else {
                event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame));
                if Instant::now() >= self.next_frame {
                    window.request_redraw();
                }
            }
        }
    }
}

fn rgss_key_codes(key: PhysicalKey) -> Option<&'static [i32]> {
    match key {
        // Essentials aliases USE/BACK/ACTION/SPECIAL to the legacy RGSS
        // constants C/B/A/Z in Data/Scripts/003_Technical/005_Input.rb.
        // Send both numeric aliases so legacy Essentials code and scripts
        // polling the extended RGSS names (:RETURN, :ESCAPE, USE, ...) see
        // the same physical key.
        PhysicalKey::Code(KeyCode::KeyZ) => Some(&[1, 24]),
        PhysicalKey::Code(KeyCode::KeyX) => Some(&[2, 21]),
        PhysicalKey::Code(KeyCode::KeyC) => Some(&[3, 23]),
        PhysicalKey::Code(KeyCode::KeyA) => Some(&[6, 22]),
        PhysicalKey::Code(KeyCode::KeyS) => Some(&[4]),
        PhysicalKey::Code(KeyCode::KeyD) => Some(&[5]),
        PhysicalKey::Code(KeyCode::KeyQ) => Some(&[6]),
        PhysicalKey::Code(KeyCode::KeyW) => Some(&[7]),
        PhysicalKey::Code(KeyCode::ArrowUp) => Some(&[9]),
        PhysicalKey::Code(KeyCode::ArrowDown) => Some(&[10]),
        PhysicalKey::Code(KeyCode::ArrowLeft) => Some(&[11]),
        PhysicalKey::Code(KeyCode::ArrowRight) => Some(&[12]),
        PhysicalKey::Code(KeyCode::AltLeft | KeyCode::AltRight) => Some(&[13]),
        PhysicalKey::Code(KeyCode::ControlLeft | KeyCode::ControlRight) => Some(&[14]),
        PhysicalKey::Code(KeyCode::ShiftLeft | KeyCode::ShiftRight) => Some(&[15]),
        PhysicalKey::Code(KeyCode::F8) => Some(&[16]),
        PhysicalKey::Code(KeyCode::F9) => Some(&[17]),
        PhysicalKey::Code(KeyCode::F3) => Some(&[0x103]),
        PhysicalKey::Code(KeyCode::F5) => Some(&[0x105]),
        PhysicalKey::Code(KeyCode::KeyF) => Some(&[0x146]),
        PhysicalKey::Code(KeyCode::Tab) => Some(&[0x109]),
        PhysicalKey::Code(KeyCode::Delete) => Some(&[0x10a]),
        PhysicalKey::Code(KeyCode::Home) => Some(&[0x10b]),
        PhysicalKey::Code(KeyCode::End) => Some(&[0x10c]),
        PhysicalKey::Code(KeyCode::NumpadSubtract) => Some(&[0x10d]),
        PhysicalKey::Code(KeyCode::NumpadAdd) => Some(&[0x10e]),
        PhysicalKey::Code(KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space) => Some(&[1, 23]),
        PhysicalKey::Code(KeyCode::Escape | KeyCode::Backspace) => Some(&[2, 21]),
        _ => None,
    }
}
