//! Controlled bitmap/text/sprite workload. Use release mode; the second argument
//! is a trace path or `off` for an uninstrumented timing baseline.
#[path = "../src/profile.rs"]
mod profile;

use sabiruby::Step;
use sabiruby_rgss_core::{Project, RgssRuntime};
use std::time::Instant;

fn main() {
    let mut args = std::env::args().skip(1);
    let root = args
        .next()
        .expect("usage: profile_workload PROJECT [TRACE.json|off]");
    let trace = args
        .next()
        .unwrap_or_else(|| "/tmp/sabiruby-workload.json".into());
    let mut runtime = RgssRuntime::open(Project::open(root).unwrap()).unwrap();
    let mut capture =
        (trace != "off").then(|| profile::Capture::new(&mut runtime, trace.into(), 4096, 100_000));
    runtime
        .start_source(
            "profile-workload.rb",
            br#"
      atlas = Bitmap.new(256, 2048)
      600.times do |i|
        tile = Sprite.new
        tile.bitmap = atlas
        tile.src_rect.set((i % 8) * 32, (i % 64) * 32, 32, 32)
        tile.x = (i % 17) * 32
        tile.y = (i / 17) * 32
      end
      ui = Bitmap.new(512, 96)
      ui.font.size = 22
      120.times do
        ui.clear
        ui.draw_text(0, 0, 512, 48, "Pokemon: frame profiling 0123456789")
        ui.text_size("Pokemon: frame profiling 0123456789")
        Graphics.update
      end
    "#,
        )
        .unwrap();
    let start = Instant::now();
    loop {
        let before = runtime.completed_frames();
        let at = capture.as_ref().map(|_| profile::clock_ns());
        let result = runtime.step(250_000).unwrap();
        if let (Some(capture), Some(at)) = (&mut capture, at) {
            capture.span("vm.step", at);
        }
        if runtime.completed_frames() > before {
            let at = capture.as_ref().map(|_| profile::clock_ns());
            std::hint::black_box(runtime.render_snapshot());
            if let (Some(capture), Some(at)) = (&mut capture, at) {
                capture.span("snapshot", at);
                capture.completed_frames += 1;
                capture.counters(serde_json::json!({"sprites":600}));
            }
        } else if let Some(capture) = &mut capture {
            capture.budget_pauses += 1;
        }
        if matches!(result, Step::Finished(_)) {
            break;
        }
    }
    println!(
        "120 frames, 600 sprites, text draw + measurement per frame: {:.3} ms total",
        start.elapsed().as_secs_f64() * 1000.0
    );
    if let Some(capture) = capture {
        capture.finish(&runtime).unwrap();
    }
}
