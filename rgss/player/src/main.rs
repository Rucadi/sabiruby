//! Thin desktop adapter for the reusable RGSS core.
//!
//! The VM and RGSS compatibility layer do not know about WGPU, winit, or an
//! audio device. This binary owns those services and advances the suspended
//! game script from the window event loop.

use std::collections::{HashMap, HashSet};
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
use winit::event::{ElementState, Ime, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

mod profile;
use profile::Capture;

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
    profile_path: Option<PathBuf>,
    profile_seconds: Option<u64>,
    profile_sample_instructions: u64,
    profile_event_limit: usize,
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
            profile_path: None,
            profile_seconds: None,
            profile_sample_instructions: 4096,
            profile_event_limit: 100_000,
        }
    }
}

fn parse_options() -> Result<Options, String> {
    parse_options_from(env::args_os().skip(1))
}

fn parse_options_from(
    mut args: impl Iterator<Item = std::ffi::OsString>,
) -> Result<Options, String> {
    let mut options = Options::default();
    let mut mode_explicit = false;
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
            "--profile" => {
                options.profile_path = Some(parse_next::<String>(&mut args, "--profile")?.into())
            }
            "--profile-seconds" => {
                options.profile_seconds =
                    Some(parse_next::<u64>(&mut args, "--profile-seconds")?.max(1))
            }
            "--profile-sample-instructions" => {
                options.profile_sample_instructions =
                    parse_next::<u64>(&mut args, "--profile-sample-instructions")?.max(1)
            }
            "--profile-event-limit" => {
                options.profile_event_limit = parse_next(&mut args, "--profile-event-limit")?
            }
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
    if options.profile_seconds.is_some() && options.profile_path.is_none() {
        return Err("--profile-seconds requires --profile PATH".to_owned());
    }
    if options.profile_path.is_some() && !cfg!(feature = "profiling") {
        return Err("rebuild with cargo run --release -p sabiruby-rgss-player --features profiling to use --profile".to_owned());
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
              --step-budget N --fps N --fps-unlocked --auto-z\n\
              --profile PATH --profile-seconds N\n\
              --profile-sample-instructions N --profile-event-limit N"
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
    let mut capture = start_capture(runtime, options);
    let result = (|| {
        let load_start = capture.as_ref().map(|_| profile::clock_ns());
        let count = runtime
            .start_scripts(options.script_limit)
            .map_err(|error| format!("boot failed: {error}"))?;
        if let (Some(capture), Some(start)) = (&mut capture, load_start) {
            capture.span("scripts.load", start);
        }
        let target = options
            .frame_limit
            .unwrap_or(if options.profile_seconds.is_some() {
                u64::MAX
            } else {
                1
            });
        while runtime.completed_frames() < target {
            if profile_expired(capture.as_ref(), options) {
                break;
            }
            let before = runtime.completed_frames();
            let start = capture.as_ref().map(|_| profile::clock_ns());
            let result = runtime.step(options.step_budget);
            if let (Some(capture), Some(start)) = (&mut capture, start) {
                capture.span("vm.step", start);
                if runtime.completed_frames() > before {
                    capture.completed_frames += runtime.completed_frames() - before;
                    let start = profile::clock_ns();
                    let snapshot = runtime.render_snapshot();
                    capture.span("snapshot", start);
                    capture.sprites += snapshot.sprites.len() as u64;
                } else {
                    capture.budget_pauses += 1;
                }
            }
            match result.map_err(|error| format!("game stopped: {error}"))? {
                Step::Paused if runtime.completed_frames() > before => {
                    apply_auto_z(runtime, options)
                }
                Step::Paused => {}
                Step::Finished(_) => break,
            }
        }
        Ok(count)
    })();
    if let Some(capture) = capture {
        capture.finish(runtime)?;
    }
    result
}

fn start_capture(runtime: &mut RgssRuntime, options: &Options) -> Option<Capture> {
    if !cfg!(feature = "profiling") {
        return None;
    }
    options.profile_path.as_ref().map(|path| {
        Capture::new(
            runtime,
            path.clone(),
            options.profile_sample_instructions,
            options.profile_event_limit,
        )
    })
}

fn profile_expired(capture: Option<&Capture>, options: &Options) -> bool {
    capture
        .zip(options.profile_seconds)
        .is_some_and(|(capture, seconds)| capture.elapsed() >= Duration::from_secs(seconds))
}

fn apply_auto_z(runtime: &mut RgssRuntime, options: &Options) {
    if !options.auto_z {
        return;
    }
    let frame = runtime.frame_count();
    if frame % 45 == 1 {
        runtime.input_press(13);
    } else if frame % 45 == 2 {
        runtime.input_release(13);
    }
}

fn exit_with_error(error: impl std::fmt::Display) -> ! {
    eprintln!("{error}");
    std::process::exit(1);
}

fn print_output(runtime: &mut RgssRuntime) {
    print!("{}", String::from_utf8_lossy(&runtime.take_output()));
}

fn run_windowed(runtime: RgssRuntime, project: Project, options: Options) -> Result<(), String> {
    let title = project.title.clone();
    let mut app = App::new(runtime, project, options, title);
    let start = app.capture.as_ref().map(|_| profile::clock_ns());
    app.runtime
        .start_scripts(app.options.script_limit)
        .map_err(|error| format!("boot failed: {error}"))?;
    if let (Some(capture), Some(start)) = (&mut app.capture, start) {
        capture.span("scripts.load", start);
    }
    let event_loop = EventLoop::new().map_err(|error| format!("event loop: {error}"))?;
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
    held_keys: HashSet<PhysicalKey>,
    text_input_active: bool,
    last_key_text: Option<String>,
    capture: Option<Capture>,
    profile_frame_start: Option<u64>,
}

impl Drop for App {
    fn drop(&mut self) {
        if let Some(capture) = self.capture.take() {
            if let Err(error) = capture.finish(&self.runtime) {
                eprintln!("profile write failed: {error}");
            }
        }
    }
}

impl App {
    fn new(mut runtime: RgssRuntime, project: Project, options: Options, title: String) -> Self {
        let now = Instant::now();
        let capture = start_capture(&mut runtime, &options);
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
            held_keys: HashSet::new(),
            text_input_active: false,
            last_key_text: None,
            capture,
            profile_frame_start: None,
        }
    }

    fn sprite_frames(&mut self, snapshot: &RenderSnapshot) -> Result<Vec<SpriteFrame>, String> {
        let mut frames = Vec::new();
        for sprite in &snapshot.sprites {
            let (asset_key, image, raw_rgba) = if let Some(path) = sprite.bitmap_path.as_ref() {
                if !self.asset_cache.contains_key(path) {
                    let Some(bytes) = self.runtime.read_asset(path) else {
                        return Err(format!("render asset not found: {path}"));
                    };
                    self.asset_cache
                        .insert(path.clone(), Arc::from(bytes.into_boxed_slice()));
                }
                let Some(image) = self.asset_cache.get(path).cloned() else {
                    return Err(format!("render asset cache entry disappeared: {path}"));
                };
                (path.clone(), image, None)
            } else if let Some(image) = sprite.bitmap_rgba.as_ref() {
                (
                    format!("bitmap:{}:{}", sprite.bitmap_id, sprite.bitmap_revision),
                    image.clone(),
                    Some((sprite.bitmap_width, sprite.bitmap_height)),
                )
            } else {
                return Err(format!(
                    "sprite {} has no bitmap data or asset path",
                    sprite.bitmap_id
                ));
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
                clip_rect: sprite.clip_rect,
            });
        }
        if let Some(movie) = snapshot.movie.as_ref() {
            let ratio = (f64::from(snapshot.width.max(1)) / f64::from(movie.width.max(1)))
                .min(f64::from(snapshot.height.max(1)) / f64::from(movie.height.max(1)));
            let display_width = f64::from(movie.width) * ratio;
            let display_height = f64::from(movie.height) * ratio;
            frames.push(SpriteFrame {
                asset_key: format!("movie:{}:{}", movie.id, movie.revision),
                image: movie.rgba.clone(),
                raw_rgba: Some((movie.width, movie.height)),
                bitmap_width: movie.width,
                bitmap_height: movie.height,
                src_x: 0,
                src_y: 0,
                src_width: i64::from(movie.width),
                src_height: i64::from(movie.height),
                x: ((f64::from(snapshot.width) - display_width) / 2.0).round() as i64,
                y: ((f64::from(snapshot.height) - display_height) / 2.0).round() as i64,
                ox: 0,
                oy: 0,
                zoom_x: ratio,
                zoom_y: ratio,
                opacity: 255,
                angle: 0.0,
                mirror: false,
                color: [255, 255, 255, 0],
                tone: [0, 0, 0, 0],
                blend_type: 0,
                visible: true,
                z: i64::MAX,
                tile: false,
                clip_rect: None,
            });
        }
        Ok(frames)
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
        let mut renderer = if self.options.fps_unlocked {
            pollster::block_on(Renderer::new_unlocked(window.clone()))?
        } else {
            pollster::block_on(Renderer::new(window.clone()))?
        };
        renderer.set_profiling(self.capture.is_some());
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
        if profile_expired(self.capture.as_ref(), &self.options) {
            event_loop.exit();
            return;
        }
        let previous_frame = self.runtime.completed_frames();
        let step_start = self.capture.as_ref().map(|_| profile::clock_ns());
        if self.profile_frame_start.is_none() {
            self.profile_frame_start = step_start;
        }
        let result = self.runtime.step(self.options.step_budget);
        if let (Some(capture), Some(start)) = (&mut self.capture, step_start) {
            capture.span("vm.step", start);
        }
        if let Err(error) = result {
            print_output(&mut self.runtime);
            eprintln!("game stopped: {error}");
            event_loop.exit();
            return;
        }
        let text_input = self.runtime.text_input_enabled();
        if text_input != self.text_input_active {
            if let Some(window) = &self.window {
                window.set_ime_allowed(text_input);
            }
            self.text_input_active = text_input;
            self.last_key_text = None;
        }
        if matches!(result, Ok(Step::Finished(_))) {
            print_output(&mut self.runtime);
            event_loop.exit();
            return;
        }
        // A budget pause only lets the host process events. Presenting here
        // would expose partially disposed/rebuilt maps during a transfer.
        if self.runtime.completed_frames() == previous_frame {
            if let Some(capture) = &mut self.capture {
                capture.budget_pauses += 1;
            }
            return;
        }
        apply_auto_z(&mut self.runtime, &self.options);
        let start = self.capture.as_ref().map(|_| profile::clock_ns());
        let snapshot = self.runtime.render_snapshot();
        if let (Some(capture), Some(start)) = (&mut self.capture, start) {
            capture.span("snapshot", start);
            capture.completed_frames += self.runtime.completed_frames() - previous_frame;
            capture.sprites += snapshot.sprites.len() as u64;
            let mut bitmaps = HashSet::new();
            let bytes: u64 = snapshot
                .sprites
                .iter()
                .filter(|sprite| bitmaps.insert(sprite.bitmap_id))
                .filter_map(|sprite| sprite.bitmap_rgba.as_ref())
                .map(|pixels| pixels.len() as u64)
                .sum();
            capture.bitmap_bytes += bytes;
            capture.counters(serde_json::json!({"sprites":snapshot.sprites.len(),"unique_bitmaps":bitmaps.len(),
                "bitmap_bytes":bytes,"instructions":self.runtime.vm.instructions,"gc_count":self.runtime.vm.gc_count,
                "gc_ns":self.runtime.vm.gc_time_ns}));
        }
        let start = self.capture.as_ref().map(|_| profile::clock_ns());
        let sprites = match self.sprite_frames(&snapshot) {
            Ok(sprites) => sprites,
            Err(error) => {
                print_output(&mut self.runtime);
                eprintln!("rendering stopped: {error}");
                event_loop.exit();
                return;
            }
        };
        if let (Some(capture), Some(start)) = (&mut self.capture, start) {
            capture.span("sprite.prepare", start);
        }
        let start = self.capture.as_ref().map(|_| profile::clock_ns());
        self.dispatch_audio();
        if let (Some(capture), Some(start)) = (&mut self.capture, start) {
            capture.span("audio.dispatch", start);
        }
        let start = self.capture.as_ref().map(|_| profile::clock_ns());
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
            if let (Some(capture), Some(stats)) = (&mut self.capture, renderer.take_frame_profile())
            {
                let mut at = start.unwrap_or(0);
                for (name, ns) in [
                    ("render.acquire", stats.acquire_ns),
                    ("render.prepare_upload", stats.prepare_ns),
                    ("render.encode", stats.encode_ns),
                    ("render.submit", stats.submit_ns),
                    ("render.present", stats.present_ns),
                ] {
                    capture.duration(name, at, ns);
                    at += ns;
                }
                capture.counters(serde_json::json!({"draw_calls":stats.draw_calls,"texture_uploads":stats.uploads,"upload_bytes":stats.upload_bytes}));
            }
        }
        if let (Some(capture), Some(start)) = (&mut self.capture, start) {
            capture.span("render.total", start);
        }
        if let (Some(capture), Some(start)) = (&mut self.capture, self.profile_frame_start.take()) {
            capture.span("frame.active_wall", start);
        }
        if self
            .options
            .frame_limit
            .is_some_and(|limit| self.runtime.completed_frames() >= limit)
        {
            event_loop.exit();
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

    fn handle_text(&mut self, text: &str, ime_commit: bool) {
        if !self.runtime.text_input_enabled() {
            self.last_key_text = None;
            return;
        }
        // Some platforms report the same committed text in both key and IME
        // events. Consume that pair once, while retaining repeated keystrokes.
        if ime_commit {
            if self.last_key_text.as_deref() != Some(text) {
                self.runtime.input_text(text);
            }
            self.last_key_text = None;
        } else {
            self.runtime.input_text(text);
            self.last_key_text = Some(text.to_owned());
        }
    }

    fn release_all_keys(&mut self) {
        self.last_key_text = None;
        let keys: Vec<_> = self.held_keys.iter().copied().collect();
        for key in keys {
            self.handle_key(ElementState::Released, key);
        }
    }

    fn handle_key(&mut self, state: ElementState, physical_key: PhysicalKey) {
        match state {
            ElementState::Pressed => {
                self.held_keys.insert(physical_key);
            }
            ElementState::Released => {
                self.held_keys.remove(&physical_key);
            }
        }
        for code in rgss_key_codes(physical_key) {
            // Multiple physical keys may hold the same RGSS action.
            if self
                .held_keys
                .iter()
                .any(|key| rgss_key_codes(*key).contains(&code))
            {
                self.runtime.input_press(code);
            } else {
                self.runtime.input_release(code);
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
                if event.state == ElementState::Pressed {
                    self.last_key_text = None;
                    if let Some(text) = event.text.as_deref() {
                        self.handle_text(text, false);
                    }
                }
                self.handle_key(event.state, event.physical_key);
            }
            WindowEvent::Ime(Ime::Commit(text)) => self.handle_text(&text, true),
            WindowEvent::Ime(Ime::Preedit(_, _)) => {
                self.last_key_text = None;
            }
            WindowEvent::Focused(false) => {
                self.release_all_keys();
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

// Raw queries use SDL scancodes in a separate namespace from RGSS actions.
fn rgss_key_codes(key: PhysicalKey) -> Vec<i32> {
    let PhysicalKey::Code(key) = key else {
        return Vec::new();
    };
    let (scan, actions): (i32, &[i32]) = match key {
        KeyCode::KeyA => (4, &[14]),
        KeyCode::KeyB => (5, &[]),
        KeyCode::KeyC => (6, &[]),
        KeyCode::KeyD => (7, &[16]),
        KeyCode::KeyE => (8, &[]),
        KeyCode::KeyF => (9, &[]),
        KeyCode::KeyG => (10, &[]),
        KeyCode::KeyH => (11, &[]),
        KeyCode::KeyI => (12, &[]),
        KeyCode::KeyJ => (13, &[]),
        KeyCode::KeyK => (14, &[]),
        KeyCode::KeyL => (15, &[]),
        KeyCode::KeyM => (16, &[]),
        KeyCode::KeyN => (17, &[]),
        KeyCode::KeyO => (18, &[]),
        KeyCode::KeyP => (19, &[]),
        KeyCode::KeyQ => (20, &[17]),
        KeyCode::KeyR => (21, &[]),
        KeyCode::KeyS => (22, &[15]),
        KeyCode::KeyT => (23, &[]),
        KeyCode::KeyU => (24, &[]),
        KeyCode::KeyV => (25, &[]),
        KeyCode::KeyW => (26, &[18]),
        KeyCode::KeyX => (27, &[12]),
        KeyCode::KeyY => (28, &[]),
        KeyCode::KeyZ => (29, &[13]),
        KeyCode::Digit1 => (30, &[]),
        KeyCode::Digit2 => (31, &[]),
        KeyCode::Digit3 => (32, &[]),
        KeyCode::Digit4 => (33, &[]),
        KeyCode::Digit5 => (34, &[]),
        KeyCode::Digit6 => (35, &[]),
        KeyCode::Digit7 => (36, &[]),
        KeyCode::Digit8 => (37, &[]),
        KeyCode::Digit9 => (38, &[]),
        KeyCode::Digit0 => (39, &[]),
        KeyCode::Enter => (40, &[13]),
        KeyCode::Escape => (41, &[12]),
        KeyCode::Backspace => (42, &[]),
        KeyCode::Tab => (43, &[]),
        KeyCode::Space => (44, &[13]),
        KeyCode::Backquote => (53, &[]),
        KeyCode::F1 => (58, &[]),
        KeyCode::F2 => (59, &[]),
        KeyCode::F3 => (60, &[]),
        KeyCode::F4 => (61, &[]),
        KeyCode::F5 => (62, &[25]),
        KeyCode::F6 => (63, &[26]),
        KeyCode::F7 => (64, &[27]),
        KeyCode::F8 => (65, &[28]),
        KeyCode::F9 => (66, &[29]),
        KeyCode::F10 => (67, &[]),
        KeyCode::F11 => (68, &[]),
        KeyCode::F12 => (69, &[]),
        KeyCode::Home => (74, &[]),
        KeyCode::PageUp => (75, &[17]),
        KeyCode::Delete => (76, &[]),
        KeyCode::End => (77, &[]),
        KeyCode::PageDown => (78, &[18]),
        KeyCode::ArrowRight => (79, &[6]),
        KeyCode::ArrowLeft => (80, &[4]),
        KeyCode::ArrowDown => (81, &[2]),
        KeyCode::ArrowUp => (82, &[8]),
        KeyCode::NumpadSubtract => (86, &[]),
        KeyCode::NumpadAdd => (87, &[]),
        KeyCode::NumpadEnter => (88, &[13]),
        KeyCode::ControlLeft => (224, &[22]),
        KeyCode::ShiftLeft => (225, &[11, 21]),
        KeyCode::AltLeft => (226, &[23]),
        KeyCode::ControlRight => (228, &[22]),
        KeyCode::ShiftRight => (229, &[11, 21]),
        KeyCode::AltRight => (230, &[23]),
        _ => return Vec::new(),
    };
    let mut codes = actions.to_vec();
    codes.push(sabiruby_rgss_core::RAW_KEY_BASE + scan);
    codes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "profiling")]
    fn profile_options_are_explicit_and_do_not_enable_capture_by_default() {
        let options = parse_options_from(
            [
                "--headless",
                "--profile",
                "/tmp/trace.json",
                "--profile-seconds",
                "5",
                "--profile-sample-instructions",
                "2048",
                "--profile-event-limit",
                "200",
            ]
            .into_iter()
            .map(Into::into),
        )
        .unwrap();
        assert!(!options.window);
        assert_eq!(options.profile_path, Some(PathBuf::from("/tmp/trace.json")));
        assert_eq!(options.profile_seconds, Some(5));
        assert_eq!(options.profile_sample_instructions, 2048);
        assert_eq!(options.profile_event_limit, 200);
        assert!(
            parse_options_from(["--profile-seconds", "5"].into_iter().map(Into::into)).is_err()
        );
        assert!(Options::default().profile_path.is_none());
    }

    #[test]
    fn text_events_reach_ruby_once_and_only_in_text_mode() {
        let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
        let runtime = RgssRuntime::open(project.clone()).unwrap();
        let mut app = App::new(runtime, project, Options::default(), String::new());
        app.handle_text("ignored", false);
        app.runtime
            .run_source("text-mode.rb", b"Input.text_input = true")
            .unwrap();
        app.handle_text("A", false);
        app.handle_text("A", true); // Paired key/IME events must not insert twice.
        app.handle_text("ñ", false);
        app.handle_text("i", false);
        app.handle_text("l", false);
        app.last_key_text = None; // IME preedit clears an earlier key event.
        app.handle_text("界", true);
        app.handle_text("!", false);
        app.handle_text("!", false); // Repeated ordinary typing remains valid.
        app.runtime
            .run_source(
                "typed.rb",
                b"puts Input.gets; puts Input.gets.empty?; Input.text_input = false",
            )
            .unwrap();
        assert_eq!(
            String::from_utf8(app.runtime.take_output()).unwrap(),
            "Añil界!!\ntrue\n"
        );
        app.handle_text("ignored", true);
        app.runtime
            .run_source("disabled.rb", b"puts Input.gets.empty?")
            .unwrap();
        assert_eq!(app.runtime.take_output(), b"true\n");
    }

    #[test]
    fn default_keyboard_bindings_match_rgss_and_keep_raw_keys_separate() {
        for (key, actions) in [
            (KeyCode::ArrowDown, vec![2]),
            (KeyCode::ArrowLeft, vec![4]),
            (KeyCode::ArrowRight, vec![6]),
            (KeyCode::ArrowUp, vec![8]),
            (KeyCode::KeyZ, vec![13]),
            (KeyCode::Enter, vec![13]),
            (KeyCode::Space, vec![13]),
            (KeyCode::KeyX, vec![12]),
            (KeyCode::Escape, vec![12]),
            (KeyCode::ShiftLeft, vec![11, 21]),
            (KeyCode::KeyA, vec![14]),
            (KeyCode::KeyS, vec![15]),
            (KeyCode::KeyD, vec![16]),
            (KeyCode::KeyQ, vec![17]),
            (KeyCode::KeyW, vec![18]),
            (KeyCode::KeyC, vec![]),
        ] {
            let codes = rgss_key_codes(PhysicalKey::Code(key));
            assert!(codes.last().unwrap() >= &sabiruby_rgss_core::RAW_KEY_BASE);
            assert_eq!(&codes[..codes.len() - 1], actions, "{key:?}");
        }
    }

    #[test]
    fn shared_bindings_stay_pressed_until_all_keys_release_and_focus_loss_clears_them() {
        let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
        let runtime = RgssRuntime::open(project.clone()).unwrap();
        let mut app = App::new(runtime, project, Options::default(), String::new());
        app.handle_key(ElementState::Pressed, PhysicalKey::Code(KeyCode::KeyZ));
        app.handle_key(ElementState::Pressed, PhysicalKey::Code(KeyCode::Enter));
        app.runtime.run_source("held.rb", b"Input.update").unwrap();
        app.handle_key(ElementState::Released, PhysicalKey::Code(KeyCode::KeyZ));
        app.runtime
            .run_source(
                "held.rb",
                b"Input.update; puts Input.press?(13); puts Input.release?(13)",
            )
            .unwrap();
        assert_eq!(app.runtime.take_output(), b"true\nfalse\n");
        app.release_all_keys();
        app.runtime.run_source("focus.rb", b"Input.update; puts Input.press?(13); puts Input.release?(13); puts Input.pressex?(:RETURN)").unwrap();
        assert_eq!(app.runtime.take_output(), b"false\ntrue\nfalse\n");
        assert!(app.held_keys.is_empty());
    }
}
