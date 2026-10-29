//! Windowed benchmark using the normal player path, enabled only for tooling.
use super::*;
use sabiruby_rgss_wgpu::RenderProfile;

pub struct Measurements {
    warmup: usize,
    samples: Vec<u64>,
    first_ns: u64,
    last_ns: u64,
    render: RenderProfile,
}

impl Measurements {
    fn new() -> Self {
        Self {
            warmup: 100,
            samples: Vec::new(),
            first_ns: 0,
            last_ns: 0,
            render: RenderProfile::default(),
        }
    }

    pub fn record(&mut self, start: u64, end: u64, stats: Option<&RenderProfile>) {
        if self.warmup > 0 {
            self.warmup -= 1;
            return;
        }
        if self.samples.is_empty() {
            self.first_ns = start;
        }
        self.last_ns = end;
        self.samples.push(end - start);
        if let Some(stats) = stats {
            self.render.acquire_ns += stats.acquire_ns;
            self.render.prepare_ns += stats.prepare_ns;
            self.render.geometry_ns += stats.geometry_ns;
            self.render.vertex_upload_ns += stats.vertex_upload_ns;
            self.render.encode_ns += stats.encode_ns;
            self.render.submit_ns += stats.submit_ns;
            self.render.present_ns += stats.present_ns;
            self.render.draw_calls += stats.draw_calls;
            self.render.sprites += stats.sprites;
            self.render.vertices += stats.vertices;
            self.render.static_vertices += stats.static_vertices;
            self.render.dynamic_vertices += stats.dynamic_vertices;
            self.render.static_rebuilds += stats.static_rebuilds;
            self.render.uploads += stats.uploads;
        }
    }

    fn report(mut self) {
        self.samples.sort_unstable();
        let n = self.samples.len();
        assert!(n > 0, "no benchmark frames presented");
        let divisor = n as f64 * 1000.0;
        println!(
            "window_frames={n} submitted_fps={:.1} active_mean_us={:.2} p50_us={:.2} p95_us={:.2} acquire_us={:.2} prepare_us={:.2} geometry_us={:.2} vertex_upload_us={:.2} encode_us={:.2} submit_us={:.2} present_us={:.2} sprites_per_frame={:.2} vertices_per_frame={:.2} static_vertices_per_frame={:.2} dynamic_vertices_per_frame={:.2} static_rebuilds={:.2} draws_per_frame={:.2} texture_uploads={}",
            n as f64 * 1e9 / (self.last_ns - self.first_ns) as f64,
            self.samples.iter().sum::<u64>() as f64 / divisor,
            self.samples[n / 2] as f64 / 1000.0,
            self.samples[(n * 95 / 100).min(n - 1)] as f64 / 1000.0,
            self.render.acquire_ns as f64 / divisor,
            self.render.prepare_ns as f64 / divisor,
            self.render.geometry_ns as f64 / divisor,
            self.render.vertex_upload_ns as f64 / divisor,
            self.render.encode_ns as f64 / divisor,
            self.render.submit_ns as f64 / divisor,
            self.render.present_ns as f64 / divisor,
            self.render.sprites as f64 / n as f64,
            self.render.vertices as f64 / n as f64,
            self.render.static_vertices as f64 / n as f64,
            self.render.dynamic_vertices as f64 / n as f64,
            self.render.static_rebuilds as f64 / n as f64,
            self.render.draw_calls as f64 / n as f64,
            self.render.uploads
        );
    }
}

pub fn run(runtime: RgssRuntime, frames: u64, zoom3: bool) -> Result<(), String> {
    let project = runtime.project.clone();
    let target = runtime.completed_frames() + frames + 100;
    let options = Options {
        root: project.root().to_path_buf(),
        fps_unlocked: true,
        fps_limit: None,
        render_scale: if zoom3 { 3 } else { 1 },
        frame_limit: Some(target),
        ..Options::default()
    };
    let mut app = App::new(
        runtime,
        project,
        options,
        "RGSS starting-house benchmark".into(),
    );
    app.benchmark = Some(Measurements::new());
    let event_loop = EventLoop::new().map_err(|e| e.to_string())?;
    event_loop.run_app(&mut app).map_err(|e| e.to_string())?;
    app.benchmark.take().unwrap().report();
    Ok(())
}
