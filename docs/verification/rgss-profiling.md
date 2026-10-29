# RGSS performance profiling

Run inside the repository Nix shell, using a release build:

```sh
nix-shell shell.nix
cargo run --release -p sabiruby-rgss-player --features profiling -- anil \
  --profile /tmp/anil-trace.json --profile-seconds 120
```

Play through the name screen, enter the overworld, walk, and transfer maps.
Closing the window also writes the capture. For throughput measurements add
`--fps-unlocked`; keep the normal frame cap when investigating visible stutter.
Use the same save, actions, frame cap, and build for comparisons.

The capture writes:

| File | Contents |
| --- | --- |
| `anil-trace.json` | Chrome trace events; open with a trace viewer such as Perfetto |
| `anil-trace.summary.json` | Native totals/counts/maxima, host phase statistics, GC and workload counters |
| `anil-trace.stacks.folded` | Folded Ruby stacks weighted by instruction samples |

`--profile-sample-instructions N` defaults to 4096. Increase it to reduce stack
sampling overhead and trace size. `--profile-event-limit N` defaults to 100000
for each of the host and VM buffers. Totals continue after buffers fill;
the summary reports dropped trace events. These are bounded first-event buffers,
so a longer capture may need a larger limit to retain the later gameplay.
Native spans of at least 100 microseconds are retained in the timeline;
aggregate native counts and totals include every call.
Normal player builds compile out the VM profiling hooks. Build with
`--features profiling` to enable the CLI capture option; even in that build,
no profiler clocks, samples, or output files are used unless `--profile` is set.

A bounded headless CLI check, without a GPU or audio device:

```sh
cargo run --release -p sabiruby-rgss-player --features profiling -- anil --headless --auto-z \
  --profile /tmp/boot.json --profile-seconds 10
```

Headless captures measure VM and snapshot work, not renderer/device performance.
The runtime entrypoint remains the archive named by `Game.ini`.

## Reading the data

The host measures script loading, VM slices, render snapshot construction,
sprite preparation, audio dispatch, and renderer API phases. The renderer splits
surface acquisition, preparation/texture uploads, command encoding, queue
submission, and presentation. Counters include sprite/bitmap counts, bytes
referenced by snapshots, draw calls, and texture upload counts/bytes.
`frame.active_wall` covers all VM budget slices through presentation for one
completed frame, excluding the frame-cap wait before work begins. It can include
OS/event-loop scheduling between slices, so it is elapsed work latency rather
than thread CPU time.

Renderer timings are CPU/API durations. Submission is asynchronous; these are
not GPU timestamp measurements. Surface acquisition or presentation can include
driver/vsync waiting. Bitmap bytes referenced across frames are not allocation
or process-memory measurements.

Native timings include Ruby callbacks and nested native calls. Do not add
inclusive totals together or to `vm.step`. The
`excluding_nested_natives_ms` column subtracts nested natives but still includes
callback bytecode. Calls sharing one native implementation and receiver class
(including aliases and dynamic property handlers) are grouped under the first
name observed (for example, a game's alias of
`Graphics.update`). GC time is measured separately and overlaps the VM phases.
Each native summary includes its 20 most expensive Ruby call sites by inclusive
time, with source file, line, call count and duration. Native timeline spans also
carry the caller location. Use these to attribute repeated regexp or bitmap work
to the scripts responsible.

Ruby stacks include source file, line, and method, sampled at instruction
intervals. They identify frequently executed paths; their counts are **not CPU
time percentages**. Native durations supply the missing evidence for calls
that spend a long time in Rust between instruction samples. The root VM's
optional `profiling` feature uses only `alloc` and a host-supplied monotonic
nanosecond clock; ordinary builds without that feature compile out the hooks.

Host phase p50/p95/p99 values are logarithmic histogram upper bounds in
microseconds. Maxima and totals use the measured nanoseconds. Nested phases
overlap. Compare profiled and unprofiled release runs to assess instrumentation
overhead; profiling many tiny native calls can substantially affect throughput.

## Initial findings: 2026-10-29

Native `Graphics.update` currently advances media and requests frame suspension.
Map traversal and tile updates happen in Ruby; snapshot construction and WGPU
submission happen in the host. Replacing `Graphics.update` alone would not
remove those costs.

The controlled `profile_workload` example creates 600 sprites sharing an atlas,
then performs text drawing, text measurement, `Graphics.update`, and snapshot
construction for 120 frames:

```sh
cargo run --release -p sabiruby-rgss-player --features profiling --example profile_workload -- \
  anil /tmp/workload.json
cargo run --release -p sabiruby-rgss-player --features profiling --example profile_workload -- \
  anil off
```

Before caching parsed fonts, its profiled text drawing took 46.88 ms and text
measurement 39.14 ms. Native `Graphics.update` took 0.06 ms across all 120 calls.
The same font was being read and parsed again for every text operation.

After a bounded font cache, drawing took 7.15 ms, measurement 0.25 ms, and
`Graphics.update` 0.03 ms in the comparison capture. Five **unprofiled** runs
before/after produced median total times of **147.290 ms / 62.204 ms**: a
**2.37× improvement in this controlled CPU workload**, not a whole-game FPS claim.
The remaining snapshot cost was about 0.5 ms per 600-sprite frame.

A separate diagnostic run of Anil's actual archive reached Map028 and exercised
the overworld for five seconds. Its intro clock was accelerated in the probe;
these captures are useful for call attribution, not normal gameplay FPS.
The trace revealed **3,301,740 tile `tone=` calls**: the RGSS value classes were
using object identity instead of component equality. Consequently, cloned
unchanged tones/colors looked different and the Ruby tile renderer propagated
effects to every tile again. Component comparisons matching rsxp removed those
tile `tone=` calls in the repeat capture. A differential fixture covers these
comparisons and the unchanged-tile fast path.

Other substantial costs in the archive capture remain:

| Candidate | Measured evidence | Next investigation |
| --- | --- | --- |
| Repeated regexp construction | About 62000–70000 `Regexp#initialize` calls; 1.76–1.93 s inclusive | Attribute repeated patterns to Ruby callers; consider sharing compiled regexp engines while preserving Ruby object identity |
| Bitmap blits | About 35000 `blt` calls / 1.14 s, 3113 `stretch_blt` calls / 1.08 s | Measure source pixel copies, clipping and compositing before changing algorithms |
| Ruby tile traversal | Frequent `TilemapRenderer` stack samples around lines 572–603 | After equality fixes, inspect remaining repeated map/table/property lookups before implementing a native renderer |
| GC | About 1.4 s in the repeat diagnostic capture | Correlate collections with Ruby temporary values and discarded UI/bitmap resources |
| GPU/driver costs | Not measured by the headless archive probe | Capture the windowed player and compare draw calls, uploads, acquire/submit/present phases |

Preserve the host-owned rendering boundary and explicit `Graphics.update`
suspension when optimizing. A rewrite should reproduce RGSS behavior and show
a release improvement on the same workload before replacing the current path.
