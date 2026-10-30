//! Temporary local harness: loads the first manual save and walks in a fixed
//! pattern, timing VM + snapshot per completed frame. CPU-only.
//! usage: bench_save PROJECT FRAMES ZOOM [WARMUP] [TRACE.json]
#[cfg(feature = "profiling")]
#[path = "../src/profile.rs"]
mod profile;

use sabiruby::Step;
use sabiruby_rgss_core::{Project, RgssRuntime};
use std::collections::HashMap;
use std::time::Instant;
#[path = "support/compose.rs"]
mod compose;

#[derive(Default)]
struct Stats {
    done: u64,
    vm_ns: u64,
    snap_ns: u64,
    samples: Vec<u64>,
    current: Option<i32>,
    sprites: u64,
    visible: u64,
}

fn drive(runtime: &mut RgssRuntime, first: u64, frames: u64, s: &mut Stats, record: bool) {
    let directions = [2, 6, 8, 4];
    let mut frame_ns = 0u64;
    let mut done = 0;
    let mut instr0 = runtime.vm.instructions;
    let mut gc0 = runtime.vm.gc_count;
    let mut miss0 = runtime
        .vm
        .host_state::<sabiruby_rgss_core::RgssState>()
        .unwrap()
        .tile_fast_misses;
    while done < frames {
        let index = first + done;
        let next = directions[(index / 120) as usize % directions.len()];
        if s.current != Some(next) {
            if let Some(previous) = s.current {
                runtime.input_release(previous);
            }
            runtime.input_press(next);
            s.current = Some(next);
        }
        let before = runtime.completed_frames();
        let at = Instant::now();
        let result = runtime.step(250_000).unwrap();
        let e = at.elapsed().as_nanos() as u64;
        frame_ns += e;
        if record {
            s.vm_ns += e;
        }
        if runtime.completed_frames() > before {
            let at = Instant::now();
            let snapshot = runtime.render_snapshot();
            std::hint::black_box(&snapshot);
            let e = at.elapsed().as_nanos() as u64;
            if record {
                s.sprites += snapshot.sprites.len() as u64;
                s.visible += snapshot
                    .sprites
                    .iter()
                    .filter(|x| x.visible && x.opacity > 0 && x.src_width > 0 && x.src_height > 0)
                    .count() as u64;
            }
            frame_ns += e;
            if record {
                s.snap_ns += e;
                s.samples.push(frame_ns);
                s.done += 1;
            }
            if record && frame_ns > 15_000_000 {
                let st = runtime
                    .vm
                    .host_state::<sabiruby_rgss_core::RgssState>()
                    .unwrap();
                eprintln!(
                    "slow frame {index}: {:.1} ms instr={} gc={} tile_misses={}",
                    frame_ns as f64 / 1e6,
                    runtime.vm.instructions - instr0,
                    runtime.vm.gc_count - gc0,
                    st.tile_fast_misses - miss0
                );
            }
            frame_ns = 0;
            done += 1;
            instr0 = runtime.vm.instructions;
            gc0 = runtime.vm.gc_count;
            miss0 = runtime
                .vm
                .host_state::<sabiruby_rgss_core::RgssState>()
                .unwrap()
                .tile_fast_misses;
        }
        assert!(!matches!(result, Step::Finished(_)), "game finished");
    }
}

#[inline(never)]
fn measured(runtime: &mut RgssRuntime, first: u64, frames: u64, s: &mut Stats) {
    drive(runtime, first, frames, s, true);
}

fn main() {
    let mut args = std::env::args().skip(1);
    let root = args
        .next()
        .expect("usage: bench_save PROJECT FRAMES ZOOM [WARMUP] [SETUP.rb|-] [TRACE]");
    let frames: u64 = args.next().map(|s| s.parse().unwrap()).unwrap_or(1440);
    let zoom: u32 = args.next().map(|s| s.parse().unwrap()).unwrap_or(3);
    let warmup: u64 = args.next().map(|s| s.parse().unwrap()).unwrap_or(120);
    let setup = args.next().filter(|path| path != "-");
    let dump = args.next().filter(|path| path != "-");
    let _trace = args.next();
    let project = Project::open(root).unwrap();
    let mut runtime = RgssRuntime::open(project).unwrap();
    runtime
        .run_source("count.rb", br#"puts load_data("Data/Scripts.rxdata").size"#)
        .unwrap();
    let total: usize = String::from_utf8(runtime.take_output())
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    runtime.run_scripts(Some(total.saturating_sub(1))).unwrap();
    let anil_boot: &[u8] = br#"
MessageTypes.load_default_messages if FileTest.exist?("Data/messages_core.dat")
PluginManager.runPlugins
Compiler.main
Game.initialize
Game.set_up_system
slot = SaveData.respond_to?(:get_manual_slots) ? SaveData.get_manual_slots.first : nil
slot ||= SaveData.get_newest_save_slot if SaveData.respond_to?(:get_newest_save_slot)
path = slot ? SaveData.get_full_path(slot) : SaveData::FILE_PATH
Game.load(SaveData.read_from_file(path))
"#;
    let boot = setup.map_or_else(|| anil_boot.to_vec(), |path| std::fs::read(path).unwrap());
    if let Err(error) = runtime.run_source("__boot.rb", &boot) {
        panic!(
            "boot: {error}\n{}",
            String::from_utf8_lossy(&runtime.take_output())
        );
    }
    print!("{}", String::from_utf8_lossy(&runtime.take_output()));
    runtime.set_render_size(
        runtime.project.screen_width.saturating_mul(zoom),
        runtime.project.screen_height.saturating_mul(zoom),
    );
    runtime.start_source("__scene.rb", b"$scene.main").unwrap();
    let mut s = Stats::default();
    drive(&mut runtime, 0, warmup, &mut s, false);
    #[cfg(feature = "profiling")]
    let mut capture = _trace.map(|t| profile::Capture::new(&mut runtime, t.into(), 4096, 500_000));
    let start_instr = runtime.vm.instructions;
    let start_gc = runtime.vm.gc_time_ns;
    let (h0, s0, m0) = {
        let st = runtime
            .vm
            .host_state::<sabiruby_rgss_core::RgssState>()
            .unwrap();
        (st.tile_fast_hits, st.tile_shift_hits, st.tile_fast_misses)
    };
    if let Some(dir) = &dump {
        // frames to look at, outside the measured run
        let mut cache = HashMap::new();
        for shot in 0..4 {
            compose::save_png(
                &runtime,
                &mut cache,
                &std::path::Path::new(dir).join(format!("bench{shot}.png")),
            );
            drive(
                &mut runtime,
                warmup + shot * 60,
                60,
                &mut Stats::default(),
                false,
            );
        }
    }
    measured(&mut runtime, warmup, frames, &mut s);
    #[cfg(feature = "profiling")]
    if let Some(c) = &mut capture {
        c.completed_frames += s.done;
    }
    if let Some(player) = runtime.vm.global_get("$game_player").obj() {
        println!(
            "player at {:?},{:?}",
            runtime.vm.ivar_get(player, "@x"),
            runtime.vm.ivar_get(player, "@y")
        );
    }
    let done = s.done;
    s.samples.sort_unstable();
    let p =
        |n: usize| s.samples[(s.samples.len() * n / 100).min(s.samples.len() - 1)] as f64 / 1000.0;
    let st = runtime
        .vm
        .host_state::<sabiruby_rgss_core::RgssState>()
        .unwrap();
    let shifted = st.tile_shift_hits - s0;
    println!(
        "frames={done} vm_us={:.1} snap_us={:.1} total_us={:.1} p50={:.1} p95={:.1} p99={:.1} max={:.1} fps={:.1} instr/f={:.0} gc_ms={:.1} tiles unchanged={} shifted={} full={}",
        s.vm_ns as f64 / done as f64 / 1000.0,
        s.snap_ns as f64 / done as f64 / 1000.0,
        (s.vm_ns + s.snap_ns) as f64 / done as f64 / 1000.0,
        p(50),
        p(95),
        p(99),
        p(100),
        done as f64 * 1e9 / (s.vm_ns + s.snap_ns) as f64,
        (runtime.vm.instructions - start_instr) as f64 / done as f64,
        (runtime.vm.gc_time_ns - start_gc) as f64 / 1e6,
        (st.tile_fast_hits - h0).saturating_sub(shifted),
        shifted,
        st.tile_fast_misses - m0,
    );
    println!(
        "live_after_gc={} heap_slots={} gc_count={} alloc_threshold={}",
        runtime.vm.live_after_gc,
        runtime.vm.heap.live_count(),
        runtime.vm.gc_count,
        runtime.vm.heap.alloc_threshold
    );
    println!(
        "sprites/f={} drawable/f={}",
        s.sprites / done,
        s.visible / done
    );
    #[cfg(feature = "profiling")]
    if let Some(c) = capture {
        c.finish(&runtime).unwrap();
    }
}
