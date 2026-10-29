use sabiruby_rgss_core::{Project, RgssRuntime};
use std::time::Instant;

fn main() {
    let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
    let mut runtime = RgssRuntime::open(project).unwrap();
    runtime
        .run_source(
            "snapshot-probe.rb",
            br#"
      bitmap = Bitmap.new(256, 2048)
      128.times do |i|
        sprite = Sprite.new
        sprite.bitmap = bitmap
        sprite.src_rect.set(0, (i % 64) * 32, 32, 32)
      end
    "#,
        )
        .unwrap();
    let start = Instant::now();
    for _ in 0..10 {
        std::hint::black_box(runtime.render_snapshot());
    }
    println!(
        "128 tiles, one 2 MiB bitmap: {:.3} ms/snapshot",
        start.elapsed().as_secs_f64() * 100.0
    );
}
