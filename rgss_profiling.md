# Profiling RGSS games

Use this guide to find where a game spends time, compare optimizations, and
separate Ruby/VM work from rendering and frame pacing. Run the commands from the
repository root inside its Nix shell:

```sh
nix-shell shell.nix
```

Use release builds for timing. Keep the game project, scene, player position,
window size, input, frame limit, and machine conditions consistent when
comparing results. Repeat runs: a single run can include compilation, operating
system scheduling, asset loading, or other unrelated work. Save traces outside
the repository, for example under `/tmp`.

## Live profiling with Tracy

Tracy provides a live, interactive timeline of CPU zones and frame boundaries.
The integration is opt-in at both build time and runtime. From the repository
Nix shell, start the player with release debug information for useful native
symbols:

```sh
cargo run --release --config 'profile.release.debug=1' \
  -p sabiruby-rgss-player --features tracy -- anil --fps-unlocked --tracy
```

In another terminal, open the matching Nix-provided Tracy viewer:

```sh
nix-shell -p tracy --run tracy
```

Connect to `127.0.0.1` on Tracy's default port, 8086. The client uses
on-demand capture and listens only on localhost, with network discovery
disabled: the timeline begins when the viewer connects. The pinned client
bundles Tracy 0.13.1, matching the current Nix viewer; a viewer with a different
Tracy protocol version may refuse the connection.

For process-wide Rust allocation/free events, add the separate
`tracy-memory` feature. It installs Tracy's global profiled allocator in the
desktop player (and in the benchmark example), so allocation stacks include
the VM, snapshot projection, WGPU preparation and audio adapters:

```sh
cargo build --release --config 'profile.release.debug=1' \
  -p sabiruby-rgss-player --features benchmark,profiling,tracy-memory \
  --example benchmark_map
./target/release/examples/benchmark_map anil 28 1000 --tracy
```

This captures every allocation, free and reallocation with a deep native
callstack and can be substantially slower and noisier than ordinary Tracy
spans. It is intentionally separate from `tracy`; use it for short targeted
captures, not throughput measurements. The allocator starts the Tracy client
on the first allocation, so `--tracy` is optional for allocation capture.

Look for the named `rgss.frame` begin/end markers, then inspect `vm.*`,
`snapshot`, `sprite.prepare`, `audio.dispatch`, and the `render.*` zones inside
that frame. The VM emits Ruby method messages such as `ruby method update` and
`ruby native []`; these are intentionally diagnostic and can be numerous. The
VM plots are sampled at the frame boundary, including live/free heap slots,
stack slots, call-frame depth, allocation counters, GC state and frame number.
The `render.*` zones split surface acquisition, preparation/upload, encoding,
submission and presentation. These renderer zones measure CPU/API time, not GPU
timestamps. The client enables native CPU sampling; whether samples are
available depends on the operating system's profiler permissions and support.
Tracy's native stacks do not identify Ruby methods by themselves; use the JSON
and sampled Ruby stacks below for deeper attribution.

To open Tracy during the scripted house benchmark, run:

```sh
cargo run --release --config 'profile.release.debug=1' \
  -p sabiruby-rgss-player --features tracy,benchmark \
  --example benchmark_map -- anil 28 4000 --gpu --tracy
```

Connect while it runs and inspect frames after the reported warmup. To save a
capture without opening the viewer, run this while the player is running:

```sh
nix-shell -p tracy --run 'tracy-capture -a 127.0.0.1 -s 10 -o /tmp/house.tracy'
```

Open the resulting `.tracy` file in the viewer. The extra
instrumentation and sampling can change throughput, so compare performance
with a separate build/run without Tracy connected. Without `--tracy`, even a
Tracy-enabled build does not start the profiler; ordinary builds do not link
it. The existing JSON capture can be combined with Tracy by building with
`--features tracy,profiling` and supplying both `--tracy` and `--profile PATH`.

## Profile the running game

Build the player with profiling enabled and specify an output path:

```sh
cargo run --release -p sabiruby-rgss-player --features profiling -- anil \
  --fps-unlocked --profile /tmp/anil.json --profile-seconds 120
```

Play through the scene you want to study during the capture. Close the window
to finish a capture early. `--fps-unlocked` is useful for throughput; omit it
when investigating a player's normal capped experience. A profiling build with
no `--profile` path does not collect a capture. Ordinary builds compile out
the VM profiling hooks.

The capture produces three files:

- `/tmp/anil.json`: Chrome trace events, suitable for opening in Perfetto or
  another Chrome-trace viewer. Use the timeline to correlate long frames,
  collections, and rendering phases.
- `/tmp/anil.summary.json`: aggregate timings, calls, counters, GC statistics,
  expensive native call sites, and executed opcode counts.
- `/tmp/anil.stacks.folded`: sampled Ruby stacks, suitable for flame-graph
  tooling or text analysis. Each sample is weighted by executed instructions.

The default sampling interval is 4,096 VM instructions. Increase it with
`--profile-sample-instructions N` to reduce sampling overhead. Host and VM trace
buffers each default to 100,000 events; `--profile-event-limit N` changes that
limit. The summary reports dropped events. Since the buffers retain the first
events, a long startup can use the available timeline space before the scene of
interest; increase the limit or use the targeted map benchmark below. Native
spans of at least 100 microseconds appear in the timeline, while native totals
include shorter calls too.

For a bounded boot investigation without a window, GPU, or audio device:

```sh
cargo run --release -p sabiruby-rgss-player --features profiling -- anil \
  --headless --auto-z --profile /tmp/boot.json --profile-seconds 10
```

A headless capture measures VM and snapshot work, not window presentation or
GPU behavior. Both modes execute the script archive named by `Scripts=` in the
project's `Game.ini`.

## Benchmark the starting house

The map benchmark drives the real game's startup, waits for the normal
`Scene_Map` loop on the requested map, releases its simulated input, and warms
up before measuring. The onboarding clock is accelerated to reach the scene;
the clock runs at normal speed during warmup and measurement. For Anil, map 28
is the starting house. The optional project must exist locally.

```sh
# CPU: VM execution plus construction of the render snapshot.
cargo run --release -p sabiruby-rgss-player --features benchmark \
  --example benchmark_map -- anil 28 1000

# Windowed: the normal player and WGPU rendering path, unlocked.
cargo run --release -p sabiruby-rgss-player --features benchmark \
  --example benchmark_map -- anil 28 1000 --gpu

# CPU attribution for the same map. Profiling adds overhead.
cargo run --release -p sabiruby-rgss-player --features benchmark,profiling \
  --example benchmark_map -- anil 28 500 /tmp/house.json
```

The CPU result reports mean VM and snapshot times, frame percentiles, VM
instructions per frame, and `cpu_fps`, the reciprocal of their mean combined
elapsed time. It does not include the window, GPU, audio, or event loop. The
windowed result reports submitted FPS over the measured interval, active frame
latency, renderer phase means, draw calls, and texture uploads. Verify the
reported frame count matches the requested count: closing the benchmark window
early makes its result a shorter measurement. Submitted FPS is not a count of
frames actually displayed by the monitor, and renderer API timings are not GPU
execution timestamps.

For a 1,000 FPS throughput target, the entire frame must average no more than
1,000 microseconds, including VM, snapshot, event-loop, and presentation
costs. Compare unprofiled release runs for the throughput number and use a
separate profiled run to identify where to work.

## What to investigate

1. **Frame pacing:** Look at `frame.active_wall`, its p95/p99 and maximum, and
   the timeline for individual stalls. A good mean can hide visible pauses.
   The phase covers completed-frame work and can include OS scheduling, but
   excludes frame-cap waiting.
2. **VM and Ruby:** Compare VM slice time with the whole frame. Use the folded
   stacks to find frequently executed Ruby paths, then inspect the source and
   native call sites in the summary. Instruction samples show execution
   frequency, not CPU-time percentages, and can miss long native calls. The
   summary’s `opcodes` list counts VM instructions during the capture; it helps
   identify frequent dispatch paths, but it also does not measure time.
3. **Native operations and GC:** Examine native call counts, inclusive times,
   maxima, and their Ruby callers. GC time overlaps VM time. Native calls can
   contain nested native calls and Ruby callbacks, so do not add inclusive
   totals together or add them to VM time.
4. **Snapshot construction:** Compare snapshot time with sprite and bitmap
   counts. Bitmap bytes referenced across frames are not allocation totals or
   process-memory measurements. Look for repeated property projection, sorting,
   bitmap copies, and disposed objects that remain in the render registry.
5. **Renderer and presentation:** Compare surface acquire, preparation, command
   encoding, submit, and present times with draw calls and uploads. These are
   CPU/API timings; GPU submission is asynchronous, and acquisition/present
   can include driver or vsync waits. Use a GPU profiler for GPU execution time.
6. **Allocation and resource churn:** Correlate GC pauses, texture uploads,
   bitmap revisions, repeated regular-expression compilation, and asset loads
   with the relevant call sites. Measure a representative changed scene as well
   as an unchanged room, so caching does not hide invalidation or transfer costs.

Native `Graphics.update` advances media and suspends the VM for a frame. Ruby
map traversal, snapshot construction, and the renderer are separate costs;
profile those phases before choosing an implementation to optimize. Preserve
resumable `Graphics.update` behavior and verify movement, map changes, animated
tiles, and scene transitions after a change.

For details on trace fields, measurement caveats, previous captures, and a
controlled synthetic workload, see [the RGSS profiling record](docs/verification/rgss-profiling.md).
For design tradeoffs and prior measurements, see [RGSS performance notes](docs/design/rgss_performance.md).

## Example investigation: Anil's starting house

A 700-frame capture on 2026-10-29 found approximately 40,000 VM instructions
per frame. The most frequent opcodes were `GETIV`, `JMPNOT`, `MOVE`, and
`SEND0`; their counts describe frequency rather than time. The trace attributed
substantial native time to the unchanged-tile check, repeated regexp creation,
string operations, and `RPG::Event` property reads. Four garbage collections
accounted for roughly 190 ms of the profiled interval. Other work remained in
Ruby event, character, picture, and plugin updates.

A bounded cache of already-interned RGSS data property names reduced the
profiled `RPG::Event#name` total from 22.8 ms to 5.3 ms over 700 frames on this
machine. Three alternating unprofiled runs of 1,500 house frames had median CPU
throughputs of 469 FPS before and 475 FPS after. That whole-frame difference
is small relative to run-to-run noise. A separate 1,000-frame windowed run
submitted approximately 375 FPS with 16 draw calls per frame and no texture
uploads. These figures are workload- and machine-specific; they do not meet the
1,000 FPS goal.

The captured [timeline](/tmp/rgss-house-after.json),
[summary](/tmp/rgss-house-after.summary.json), and
[folded stacks](/tmp/rgss-house-after.stacks.folded) are temporary local files.
The trace was captured in an isolated copy because an existing edit in the
working tree left `File.mtime` registered without its implementation. That copy
used the committed VFS implementation and the working tree's tile-renderer
changes. Reproduce the measurement on the final build after the working tree
builds successfully.
