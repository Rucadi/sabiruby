//! Temporary local tool: runs PROJECT headless for FRAMES frames, pressing keys
//! from a schedule, printing the scene class and dumping frames as PNG.
//! usage: rgss_drive PROJECT FRAMES OUT_DIR [frame:key:hold,...] [dump_every]
use sabiruby::Step;
use sabiruby_rgss_core::{Project, RgssRuntime};
use std::collections::HashMap;
#[path = "support/compose.rs"]
mod compose;

fn main() {
    let mut args = std::env::args().skip(1);
    let root = args.next().expect("usage");
    let frames: u64 = args.next().unwrap().parse().unwrap();
    let out = std::path::PathBuf::from(args.next().unwrap());
    let schedule: Vec<(u64, i32, u64)> = args
        .next()
        .unwrap_or_default()
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|item| {
            let parts: Vec<&str> = item.split(':').collect();
            (
                parts[0].parse().unwrap(),
                parts[1].parse().unwrap(),
                parts.get(2).map_or(4, |s| s.parse().unwrap()),
            )
        })
        .collect();
    let dump_every: u64 = args.next().map_or(100, |s| s.parse().unwrap());
    let mut runtime = RgssRuntime::open(Project::open(root).unwrap()).unwrap();
    // optional setup script: run every script but Main, the setup, then `$scene.main`
    if let Some(setup) = args.next() {
        runtime.run_scripts(Some(0)).unwrap();
        runtime
            .run_source("count.rb", b"print $RGSS_SCRIPTS.size")
            .unwrap();
        let total: usize = String::from_utf8(runtime.take_output())
            .unwrap()
            .parse()
            .unwrap();
        runtime.run_scripts(Some(total - 1)).unwrap();
        if let Err(error) = runtime.run_source("setup.rb", &std::fs::read(setup).unwrap()) {
            panic!("setup: {error}");
        }
        runtime.take_output();
        runtime.start_source("scene.rb", b"$scene.main").unwrap();
    } else {
        runtime.start_scripts(None).unwrap();
    }
    let mut cache = HashMap::new();
    let mut done = 0;
    let mut releases: Vec<(u64, i32)> = Vec::new();
    let mut typed = 0;
    let started = std::time::Instant::now();
    while done < frames {
        for &(at, key, hold) in &schedule {
            if at == done {
                // negative keys are raw SDL scancodes
                let key = if key < 0 {
                    sabiruby_rgss_core::RAW_KEY_BASE - key
                } else {
                    key
                };
                runtime.input_press(key);
                releases.push((
                    done + hold,
                    if key < 0 {
                        sabiruby_rgss_core::RAW_KEY_BASE - key
                    } else {
                        key
                    },
                ));
            }
        }
        releases.retain(|&(at, key)| {
            if at == done {
                runtime.input_release(key);
                false
            } else {
                true
            }
        });
        if runtime.text_input_enabled() && typed < 3 && done % 20 == 0 {
            runtime.input_text("Ash");
            typed += 1;
        }
        let before = runtime.completed_frames();
        match runtime.step(1_000_000) {
            Ok(Step::Finished(_)) => {
                println!("finished at frame {done}");
                break;
            }
            Ok(_) => {}
            Err(error) => {
                println!("error at frame {done}: {error}");
                break;
            }
        }
        if runtime.completed_frames() == before {
            continue;
        }
        done += 1;
        if done % dump_every == 0 {
            let scene = runtime.vm.global_get("$scene");
            let class = runtime.vm.class_name(runtime.vm.class_of(scene));
            let file = out.join(format!("frame{done:05}.png"));
            let drawn = compose::save_png(&runtime, &mut cache, &file);
            println!(
                "frame {done} ({:.1}s): scene={class} drawn={drawn}",
                started.elapsed().as_secs_f64()
            );
        }
    }
    print!("{}", String::from_utf8_lossy(&runtime.take_output()));
}
