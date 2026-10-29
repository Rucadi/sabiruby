//! CPU-side timeline and VM report writer. Timestamps share one monotonic clock.
#![cfg_attr(not(feature = "profiling"), allow(dead_code, unused_imports))]
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use sabiruby_rgss_core::RgssRuntime;
use serde_json::{Value, json};

pub fn clock_ns() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START
        .get_or_init(Instant::now)
        .elapsed()
        .as_nanos()
        .min(u128::from(u64::MAX)) as u64
}

#[derive(Default)]
struct Stage {
    calls: u64,
    total_ns: u64,
    max_ns: u64,
    // Bounded logarithmic histogram; percentiles are upper bounds in microseconds.
    histogram: [u64; 32],
}

impl Stage {
    fn add(&mut self, ns: u64) {
        self.calls += 1;
        self.total_ns = self.total_ns.saturating_add(ns);
        self.max_ns = self.max_ns.max(ns);
        let us = (ns / 1000).max(1);
        let bucket = (64 - (us - 1).leading_zeros() as usize).min(31);
        self.histogram[bucket] += 1;
    }

    fn percentile_us(&self, percentile: u64) -> u64 {
        let target = (self.calls * percentile).div_ceil(100);
        let mut count = 0;
        for (bucket, samples) in self.histogram.iter().enumerate() {
            count += samples;
            if count >= target {
                return 1 << bucket;
            }
        }
        0
    }
}

pub struct Capture {
    path: PathBuf,
    started_ns: u64,
    limit: usize,
    events: Vec<Value>,
    stages: HashMap<&'static str, Stage>,
    dropped: u64,
    pub completed_frames: u64,
    pub budget_pauses: u64,
    pub sprites: u64,
    pub bitmap_bytes: u64,
    initial_instructions: u64,
    #[cfg(feature = "profiling")]
    initial_op_counts: [u64; sabiruby::opcode::OP_COUNT],
    initial_gc_count: u64,
    initial_gc_ns: u64,
}

impl Capture {
    pub fn new(runtime: &mut RgssRuntime, path: PathBuf, interval: u64, limit: usize) -> Self {
        let started_ns = clock_ns();
        #[cfg(feature = "profiling")]
        {
            runtime.vm.gc_clock = Some(clock_ns);
            runtime.vm.set_op_counting(true);
            runtime.vm.profile = Some(sabiruby::profile::Profile::new(clock_ns, interval, limit));
        }
        #[cfg(not(feature = "profiling"))]
        let _ = interval;
        Self {
            path,
            started_ns,
            limit,
            events: Vec::new(),
            stages: HashMap::new(),
            dropped: 0,
            completed_frames: 0,
            budget_pauses: 0,
            sprites: 0,
            bitmap_bytes: 0,
            initial_instructions: runtime.vm.instructions,
            #[cfg(feature = "profiling")]
            initial_op_counts: runtime.vm.op_counts,
            initial_gc_count: runtime.vm.gc_count,
            initial_gc_ns: runtime.vm.gc_time_ns,
        }
    }

    pub fn elapsed(&self) -> Duration {
        Duration::from_nanos(clock_ns() - self.started_ns)
    }

    pub fn span(&mut self, name: &'static str, start_ns: u64) {
        self.duration(name, start_ns, clock_ns().saturating_sub(start_ns));
    }

    pub fn duration(&mut self, name: &'static str, start_ns: u64, duration_ns: u64) {
        self.stages.entry(name).or_default().add(duration_ns);
        if self.events.len() < self.limit {
            self.events
                .push(json!({"name":name,"cat":"host.cpu","ph":"X",
                "ts":start_ns as f64 / 1000.0,"dur":duration_ns as f64 / 1000.0,"pid":1,"tid":1}));
        } else {
            self.dropped += 1;
        }
    }

    pub fn counters(&mut self, args: Value) {
        if self.events.len() < self.limit {
            self.events
                .push(json!({"name":"frame","cat":"counters","ph":"C",
                "ts":clock_ns() as f64 / 1000.0,"pid":1,"tid":1,"args":args}));
        } else {
            self.dropped += 1;
        }
    }

    #[cfg(not(feature = "profiling"))]
    pub fn finish(self, _: &RgssRuntime) -> Result<(), String> {
        Err("rebuild the player with --features profiling".to_owned())
    }

    #[cfg(feature = "profiling")]
    pub fn finish(mut self, runtime: &RgssRuntime) -> Result<(), String> {
        let Some(profile) = runtime.vm.profile.as_ref() else {
            return Ok(());
        };
        for span in &profile.spans {
            self.events.push(json!({"name":profile.natives[span.native].name,"cat":"vm.native.inclusive",
                "ph":"X","ts":span.start_ns as f64/1000.0,"dur":span.duration_ns as f64/1000.0,"pid":1,"tid":1,
                "args":{"caller":span.caller.map(|(irep,pc)|caller_name(runtime,irep,pc))}}));
        }
        let mut folded = HashMap::<String, u64>::new();
        for sample in &profile.samples {
            let stack: Vec<_> = sample
                .stack
                .iter()
                .map(|id| profile.frames[*id].as_str())
                .collect();
            *folded.entry(stack.join(";")).or_default() += 1;
            self.events.push(json!({"name":"Ruby instruction sample","cat":"vm.ruby.sample",
                "ph":"i","s":"t","ts":sample.timestamp_ns as f64/1000.0,"pid":1,"tid":1,"args":{"stack":stack}}));
        }
        self.events.sort_by(|a, b| {
            a["ts"]
                .as_f64()
                .unwrap_or(0.0)
                .total_cmp(&b["ts"].as_f64().unwrap_or(0.0))
        });
        let mut natives: Vec<_> = profile.natives.iter().collect();
        natives.sort_by_key(|native| std::cmp::Reverse(native.inclusive_ns));
        let natives: Vec<_> = natives.into_iter().map(|native| {
            let mut callers: Vec<_> = native.callers.iter().collect();
            callers.sort_by_key(|(_,stats)|std::cmp::Reverse(stats.inclusive_ns));
            let callers: Vec<_> = callers.into_iter().take(20).map(|((irep,pc),stats)|json!({
                "source":caller_name(runtime,*irep,*pc),"calls":stats.calls,"inclusive_ms":stats.inclusive_ns as f64/1e6
            })).collect();
            json!({
            "name":native.name,"calls":native.calls,"inclusive_ms":native.inclusive_ns as f64/1e6,
            "excluding_nested_natives_ms":native.exclusive_native_ns as f64/1e6,"max_ms":native.max_ns as f64/1e6,
            "top_callers":callers,
        })}).collect();
        let stages: serde_json::Map<_,_> = self.stages.iter().map(|(name,stage)| ((*name).into(),json!({
            "calls":stage.calls,"total_ms":stage.total_ns as f64/1e6,
            "mean_ms":stage.total_ns as f64/1e6/stage.calls.max(1) as f64,"max_ms":stage.max_ns as f64/1e6,
            "p50_upper_us":stage.percentile_us(50),"p95_upper_us":stage.percentile_us(95),"p99_upper_us":stage.percentile_us(99)
        }))).collect();
        let summary_path = self.path.with_extension("summary.json");
        let folded_path = self.path.with_extension("stacks.folded");
        let mut opcodes: Vec<_> = runtime
            .vm
            .op_counts
            .iter()
            .enumerate()
            .filter_map(|(index, count)| {
                let count = count.saturating_sub(self.initial_op_counts[index]);
                (count > 0)
                    .then(|| json!({"opcode": sabiruby::opcode::OP_NAMES[index], "count": count}))
            })
            .collect();
        opcodes.sort_by_key(|entry| std::cmp::Reverse(entry["count"].as_u64().unwrap()));
        let summary = json!({
            "elapsed_ms":self.elapsed().as_secs_f64()*1000.0,"completed_frames":self.completed_frames,
            "budget_pauses":self.budget_pauses,"instructions":runtime.vm.instructions.saturating_sub(self.initial_instructions),
            "gc_count":runtime.vm.gc_count.saturating_sub(self.initial_gc_count),"gc_ms":runtime.vm.gc_time_ns.saturating_sub(self.initial_gc_ns) as f64/1e6,
            "sprite_snapshots":self.sprites,"bitmap_bytes_referenced":self.bitmap_bytes,
            "dropped_host_events":self.dropped,"dropped_vm_events":profile.dropped_events,
            "sample_count":profile.samples.len(),"stages":stages,"natives":natives,"opcodes":opcodes,
            "notes":["Native times include Ruby callbacks and overlap host VM slices; do not sum inclusive times.",
                "Ruby stacks are sampled by instruction count, not CPU-time percentages.",
                "Opcode counts are frequencies, not timings.",
                "Renderer timings measure CPU/API calls, not asynchronous GPU execution.",
                "Calls sharing a native implementation and receiver class are grouped under the first observed name, including aliases and dynamic properties.",
                "Stage percentiles are logarithmic histogram upper bounds; nested stages overlap."]
        });
        write_json(
            &self.path,
            &json!({"traceEvents":self.events,"displayTimeUnit":"ms"}),
        )?;
        write_json(&summary_path, &summary)?;
        let mut file = BufWriter::new(File::create(&folded_path).map_err(|e| e.to_string())?);
        let mut folded: Vec<_> = folded.into_iter().collect();
        folded.sort();
        for (stack, count) in folded {
            writeln!(file, "{stack} {count}").map_err(|e| e.to_string())?;
        }
        file.flush().map_err(|e| e.to_string())?;
        eprintln!(
            "profile: {} (summary: {}, stacks: {})",
            self.path.display(),
            summary_path.display(),
            folded_path.display()
        );
        for native in summary["natives"].as_array().unwrap().iter().take(10) {
            eprintln!(
                "  {:>9.2} ms {:>9} calls {}",
                native["inclusive_ms"].as_f64().unwrap(),
                native["calls"],
                native["name"].as_str().unwrap()
            );
        }
        Ok(())
    }
}

fn write_json(path: &Path, value: &Value) -> Result<(), String> {
    let mut file =
        BufWriter::new(File::create(path).map_err(|e| format!("{}: {e}", path.display()))?);
    serde_json::to_writer(&mut file, value).map_err(|e| e.to_string())?;
    file.flush().map_err(|e| e.to_string())
}

#[cfg(feature = "profiling")]
fn caller_name(runtime: &RgssRuntime, irep: usize, pc: usize) -> String {
    runtime
        .vm
        .ireps
        .get(irep)
        .map(|irep| {
            format!(
                "{}:{}",
                irep.filename.as_deref().unwrap_or("<bytecode>"),
                irep.line_of(pc.saturating_sub(1)).unwrap_or(0)
            )
        })
        .unwrap_or_else(|| "<unloaded>".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn histograms_report_upper_bounds_without_retaining_frame_samples() {
        let mut stage = Stage::default();
        for ns in [1000, 2000, 3000, 5000, 10000] {
            stage.add(ns);
        }
        assert_eq!(stage.calls, 5);
        assert_eq!(stage.percentile_us(50), 4);
        assert_eq!(stage.percentile_us(95), 16);
        assert_eq!(stage.max_ns, 10000);
    }

    #[test]
    #[cfg(feature = "profiling")]
    fn capture_writes_valid_bounded_trace_and_summary_files() {
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil");
        let mut runtime =
            RgssRuntime::open(sabiruby_rgss_core::Project::open(root).unwrap()).unwrap();
        let path = std::env::temp_dir().join(format!(
            "sabiruby-profile-{}-{}.json",
            std::process::id(),
            clock_ns()
        ));
        let mut capture = Capture::new(&mut runtime, path.clone(), 1, 2);
        let start = clock_ns();
        runtime
            .run_source("capture.rb", b"3.times { Graphics.update }")
            .unwrap();
        for _ in 0..4 {
            capture.span("vm.step", start);
        }
        capture.counters(json!({"sprites":0}));
        capture.finish(&runtime).unwrap();
        let trace: Value = serde_json::from_reader(File::open(&path).unwrap()).unwrap();
        assert!(trace["traceEvents"].as_array().unwrap().len() <= 4);
        let summary: Value =
            serde_json::from_reader(File::open(path.with_extension("summary.json")).unwrap())
                .unwrap();
        assert_eq!(summary["stages"]["vm.step"]["calls"], 4);
        let opcode_total: u64 = summary["opcodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["count"].as_u64().unwrap())
            .sum();
        assert_eq!(opcode_total, summary["instructions"].as_u64().unwrap());
        assert!(summary["dropped_host_events"].as_u64().unwrap() > 0);
        assert!(
            summary["natives"]
                .as_array()
                .unwrap()
                .iter()
                .any(|n| n["name"].as_str().unwrap().contains("Graphics#update"))
        );
        for path in [
            &path,
            &path.with_extension("summary.json"),
            &path.with_extension("stacks.folded"),
        ] {
            std::fs::remove_file(path).unwrap();
        }
    }
}
