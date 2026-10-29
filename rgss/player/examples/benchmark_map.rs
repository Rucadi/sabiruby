//! CPU benchmark of an actual archive scene. The accelerated onboarding clock
//! returns to normal speed before warmup; this does not measure GPU presentation.
#[allow(dead_code)]
#[path = "../src/main.rs"]
mod player;
#[cfg(feature = "profiling")]
#[path = "../src/profile.rs"]
mod profile;

use sabiruby::{Step, Value, Vm, VmError};
use sabiruby_rgss_core::{Project, RAW_KEY_BASE, RgssRuntime};
use std::sync::{
    OnceLock,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, Instant};

static CLOCK: OnceLock<Instant> = OnceLock::new();
static NORMAL_FROM: AtomicU64 = AtomicU64::new(0);

fn onboarding_clock(_: &mut Vm, _: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    let ns = CLOCK.get_or_init(Instant::now).elapsed().as_nanos() as u64;
    let normal = NORMAL_FROM.load(Ordering::Relaxed);
    let ns = if normal == 0 {
        ns * 10
    } else {
        normal * 10 + ns.saturating_sub(normal)
    };
    Ok(Value::Float(ns as f64 / 1e9))
}

fn percentile(samples: &[u64], n: usize) -> f64 {
    samples[(samples.len() * n / 100).min(samples.len() - 1)] as f64 / 1000.0
}

fn at_map_loop(runtime: &RgssRuntime) -> bool {
    let scene = runtime.vm.global_get("$scene");
    runtime
        .vm
        .ci
        .iter()
        .rev()
        .find(|ci| {
            ci.mid.is_some() && runtime.vm.stack.get(ci.base).map(|slot| slot.get()) == Some(scene)
        })
        .and_then(|ci| ci.mid)
        .is_some_and(|mid| runtime.vm.sym_name(mid) == "main")
}

fn main() {
    let mut args = std::env::args().skip(1);
    let root = args
        .next()
        .expect("usage: benchmark_map PROJECT MAP_ID [FRAMES] [TRACE.json]");
    let map_id: i64 = args.next().expect("MAP_ID").parse().unwrap();
    let frames: usize = args.next().map(|s| s.parse().unwrap()).unwrap_or(1000);
    assert!(frames > 0);
    let mut gpu = false;
    let mut trace = None;
    for arg in args {
        if arg == "--gpu" {
            gpu = true;
        } else {
            trace = Some(arg);
        }
    }
    #[cfg(not(feature = "profiling"))]
    assert!(trace.is_none(), "a trace requires --features profiling");
    let mut runtime = RgssRuntime::open(Project::open(root).unwrap()).unwrap();
    runtime.start_scripts(None).unwrap();
    let system = runtime.vm.const_path_get("System").unwrap().obj().unwrap();
    runtime
        .vm
        .define_module_function(system, "uptime", onboarding_clock)
        .unwrap();
    let started = Instant::now();
    let mut warmup = None;
    let mut ready_frames = 0;
    let mut frames_done = 0;
    let mut vm_ns = 0;
    let mut snapshot_ns = 0;
    let mut samples = Vec::with_capacity(frames);
    let mut frame_vm_ns = 0;
    let mut start_instructions = 0;
    let mut start_gc_ns = 0;
    #[cfg(feature = "profiling")]
    let mut capture = None;
    while frames_done < frames {
        assert!(
            started.elapsed() < Duration::from_secs(120),
            "benchmark timed out before completing the target scene"
        );
        let previous = runtime.completed_frames();
        let at = Instant::now();
        let result = runtime.step(250_000).unwrap();
        let elapsed = at.elapsed().as_nanos() as u64;
        if warmup == Some(0) {
            frame_vm_ns += elapsed;
        }
        assert!(
            !matches!(result, Step::Finished(_)),
            "game ended before the benchmark"
        );
        if runtime.completed_frames() == previous {
            continue;
        }
        let current_map = runtime
            .vm
            .global_get("$game_map")
            .obj()
            .map(|id| runtime.vm.ivar_get(id, "@map_id"));
        if warmup.is_none() {
            let frame = runtime.completed_frames();
            if runtime.text_input_enabled() {
                runtime.input_text("Alex");
            }
            for code in [6, 13, RAW_KEY_BASE + 40] {
                runtime.input_release(code);
            }
            if frame % 40 == 1 && current_map != Some(Value::Int(map_id)) {
                runtime.input_press(6);
            }
            if frame % 40 == 5 && ready_frames == 0 {
                runtime.input_press(13);
                runtime.input_press(RAW_KEY_BASE + 40);
            }
            if current_map == Some(Value::Int(map_id)) {
                let snapshot = runtime.render_snapshot();
                if at_map_loop(&runtime)
                    && snapshot.sprites.iter().filter(|s| s.visible).count() > 100
                    && snapshot.sprites.iter().all(|s| s.tone[0] != -255)
                {
                    ready_frames += 1;
                    if ready_frames < 120 {
                        continue;
                    }
                    for code in [6, 13, RAW_KEY_BASE + 40] {
                        runtime.input_release(code);
                    }
                    NORMAL_FROM.store(
                        CLOCK.get().unwrap().elapsed().as_nanos() as u64,
                        Ordering::Relaxed,
                    );
                    warmup = Some(120);
                    eprintln!("map {map_id} reached; warming 120 frames");
                } else {
                    ready_frames = 0;
                }
            }
            continue;
        }
        assert_eq!(current_map, Some(Value::Int(map_id)), "left benchmark map");
        assert!(
            at_map_loop(&runtime),
            "benchmark left the regular Scene_Map loop: {:?}",
            runtime.vm.backtrace(None)
        );
        if let Some(left) = warmup.as_mut().filter(|left| **left > 0) {
            *left -= 1;
            if *left == 0 {
                if gpu {
                    player::benchmark::run(runtime, frames as u64).unwrap();
                    return;
                }
                start_instructions = runtime.vm.instructions;
                start_gc_ns = runtime.vm.gc_time_ns;
                #[cfg(feature = "profiling")]
                if let Some(trace) = &trace {
                    capture = Some(profile::Capture::new(
                        &mut runtime,
                        trace.into(),
                        4096,
                        100_000,
                    ));
                }
            }
            continue;
        }
        let at = Instant::now();
        let snapshot = runtime.render_snapshot();
        std::hint::black_box(&snapshot);
        let snapshot_elapsed = at.elapsed().as_nanos() as u64;
        vm_ns += frame_vm_ns;
        snapshot_ns += snapshot_elapsed;
        samples.push(frame_vm_ns + snapshot_elapsed);
        #[cfg(feature = "profiling")]
        if let Some(capture) = &mut capture {
            let end = profile::clock_ns();
            capture.duration(
                "vm.frame",
                end - snapshot_elapsed - frame_vm_ns,
                frame_vm_ns,
            );
            capture.duration("snapshot", end - snapshot_elapsed, snapshot_elapsed);
            capture.completed_frames += 1;
            capture.sprites += snapshot.sprites.len() as u64;
            capture.counters(serde_json::json!({"map":map_id,"sprites":snapshot.sprites.len()}));
        }
        frame_vm_ns = 0;
        frames_done += 1;
    }
    samples.sort_unstable();
    println!(
        "map={map_id} frames={frames} vm_mean_us={:.2} snapshot_mean_us={:.2} total_mean_us={:.2} p50_us={:.2} p95_us={:.2} cpu_fps={:.1} instructions_per_frame={:.0} gc_ms={:.2}",
        vm_ns as f64 / frames as f64 / 1000.0,
        snapshot_ns as f64 / frames as f64 / 1000.0,
        (vm_ns + snapshot_ns) as f64 / frames as f64 / 1000.0,
        percentile(&samples, 50),
        percentile(&samples, 95),
        frames as f64 * 1e9 / (vm_ns + snapshot_ns) as f64,
        (runtime.vm.instructions - start_instructions) as f64 / frames as f64,
        (runtime.vm.gc_time_ns - start_gc_ns) as f64 / 1e6
    );
    #[cfg(feature = "profiling")]
    if let Some(capture) = capture {
        capture.finish(&runtime).unwrap();
    }
}
