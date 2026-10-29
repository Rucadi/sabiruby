# RGSS performance

Last updated: 2026-10-29. This is the RGSS-specific performance record. The VM-wide numbers live in
[`performance.md`](performance.md) and [`../verification/bench.md`](../verification/bench.md);
the frame-trace format and the raw captures are described in
[`../verification/rgss-profiling.md`](../verification/rgss-profiling.md).

The rule used here is the same one the VM work uses: change one thing, profile
the same workload before and after, and keep the change only when the release
build shows an improvement. Game-specific scripts are not patched in the
engine; optimizations belong in `rgss/core`, `rgss/wgpu`, or the player.

## How the RGSS work is measured

* Build with the repository Nix shell and the release profile
  (`codegen-units = 1` is the repository default since `96221fb`).
* CPU work: `benchmark_map PROJECT MAP FRAMES [TRACE]` from
  `rgss/player/examples/benchmark_map.rs`. It starts the real archive, drives
  the intro, waits for the requested map, warms up 120 frames, then measures
  `runtime.step` and `render_snapshot` for the requested number of frames. It
  can also run the windowed player with `--gpu`.
* VM attribution: build the player with `--features profiling` and pass a trace
  path. The profiler samples Ruby stacks by instruction count and records
  native inclusive/exclusive time and call sites. Native times include Ruby
  callbacks and nested natives; do not sum inclusive times.
* Host attribution: the trace summary has `vm.frame`, `snapshot`, renderer
  `acquire`/`prepare`/`encode`/`submit`/`present`, and counters such as sprite
  count, draw calls and texture uploads. Renderer times are CPU/API timings,
  not GPU timestamps.
* A profiled run is slower than an unprofiled one. Compare profiled runs with
  profiled runs, and use the unprofiled number only as the headline throughput.
* Treat one run as a direction, not a guarantee. The VM notes explain the
  machine's ±4–12% code-layout noise; the RGSS map workload is heavier, but
  still use repeated runs or alternating A/B before making a 1% claim.

## What was done, and why

### Parsed-font cache (`c661fbd`)

Controlled `profile_workload` draws and measures text in 600-sprite frames.
Before the cache, every text operation read and parsed the same TTF. Measured
over 120 frames:

| phase | before | after |
|---|---:|---:|
| text drawing | 46.88 ms | 7.15 ms |
| text measurement | 39.14 ms | 0.25 ms |
| median total, unprofiled | 147.290 ms | 62.204 ms |

The final comparison was **2.37× faster on this controlled CPU workload**.

**Why:** `fontdue` parsing and the file read are much more expensive than
rasterizing one string, and the game uses only a handful of font families.

**Pros:** removes repeated I/O and parsing from every text draw; parsed fonts
are immutable and shared with `Arc`.

**Cons:** the cache is bounded at 64 font-name keys and clears wholesale on
overflow rather than using an LRU; parsed fonts occupy memory; editing a font
file while the game is running is not picked up. These are acceptable for a
game session but are not a general font-service design.

### Effect values compare by component (`c661fbd`)

`Tone`, `Color` and `Rect` now compare their components instead of falling back
to Ruby object identity. In a five-second Anil diagnostic capture, identity
comparison had caused **3,301,740 tile `tone=` calls**: cloned equal tones
looked different, so the renderer propagated effects to every tile again.

**Why:** RGSS/rsxp treat equal component values as equal; the renderer's
`@old_tone != @tone` checks and the game's caches rely on that.

**Pros:** removes mass tile effect writes and clone-driven work; matches the
reference behaviour; cheap per comparison.

**Cons:** an `==` now reads four ivars and performs numeric coercion instead of
answering by identity; the coercion rules have to stay aligned with RGSS.
Differential fixtures cover the comparison and the tile path.

### Inline attribute dispatch for fixed RGSS properties (`18a0a0a`)

`Sprite`, `Plane`, `Viewport`, `Window`, `Color`, `Tone` and `Rect` get
`AttrReader`/`AttrWriter` methods for their fixed RGSS properties. The prelude
installs them only when the property is not already defined, so game-defined
getters still win.

**Why:** these properties were going through `method_missing`, which allocated
a property-name string and an argument list for every read and write. Sprites
and game UI code read and write many properties per frame.

**Pros:** the VM's inline attribute dispatch replaces a native call and
`method_missing` lookup; fewer temporary strings and argument arrays.

**Cons:** the property list is an explicit compatibility surface that must be
kept in sync with RGSS; the attributes expose ivars directly, so a later Ruby
`attr_*`/custom getter is what decides the behaviour (the installer does not
overwrite one). This is a good fit for fixed engine properties, not for
arbitrary game objects.

### Render snapshot: pre-resolved symbols and shared projections (`18a0a0a`)

`RenderSnapshot` construction used to look up every sprite property by string
and recompute viewport and bitmap geometry per sprite. `RenderReader` now
resolves the ivar symbols once per snapshot, and the snapshot memoizes viewport
and bitmap information. Neutral viewport color/tone overlays are skipped.

**Why:** the snapshot runs every presented frame and touches every registered
sprite; repeated symbol lookups and per-sprite geometry allocations were pure
overhead in a read-only projection.

**Pros:** no per-property symbol lookup; each bitmap's width/height/path and
each viewport's clip/offset are computed once per snapshot instead of once per
sprite.

**Cons:** the snapshot still allocates a `Vec` and two per-frame `HashMap`s; the
projection assumes Ruby state does not change during the snapshot (it is a
frame-boundary read, so this is intentional). The memoization is per snapshot,
not a persistent cache, so it does not help a frame where only one sprite
changed.

### Sprite registry deduplication and disposal pruning (`18a0a0a`)

`Sprite.new`/`initialize` registers a renderable, and `dispose` removes it from
`Sprite.@__sabiruby_instances`. A `@__sabiruby_registered` marker prevents a
re-`initialize` from adding a duplicate.

**Why:** repeated initialization could register the same object twice, and
disposed sprites stayed in the registry forever. Both made the render snapshot
scan objects that could never draw.

**Pros:** the registry is bounded by live sprites rather than by all sprites
ever created; the snapshot no longer filters a growing graveyard; duplicate
draws are impossible.

**Cons:** pruning relies on the game calling `dispose`; an abandoned sprite
still stays registered until GC or dispose. Removing from the Ruby array during
a native `dispose` is a mutation the registry test now covers. The registry
array is rooted through the `Sprite` class, so it must stay reachable until the
class itself is collected.

### WGPU frame batching (`18a0a0a`)

The renderer used to allocate a vertex buffer and a bind group per tile or
sprite. It now appends all visible quads to one reusable `Vertex` vector, grows
one vertex buffer to a power-of-two capacity, caches one bind group per texture,
and coalesces consecutive quads that share a scissor rectangle and asset into
one draw range. Empty source rectangles are skipped.

**Why:** a tilemap produces hundreds of quads per frame; creating a buffer and
bind group per quad spends CPU in WGPU allocation and submission rather than
drawing. Adjacent tiles in the same tileset can share one bind group and one
scissor.

**Pros:** measured map 28 frames use **14–16 draw calls** instead of one per
tile; steady-state texture uploads are zero; draw order is preserved; no
per-tile buffer/bind-group creation.

**Cons:** the CPU still rebuilds and uploads the frame's vertices every frame
(`write_buffer`); the buffer only shrinks by growing the next power of two;
batching stops at scissor or texture changes, not at every possible merge;
there is no instancing. This is an efficient immediate path, not a retained
scene graph.

### Per-VM compiled regexp cache (`18a0a0a`)

`src/regexp/cache.rs` keeps up to 128 compiled engines keyed by
`(source, flags, binary)`. Sources up to 4096 bytes are cached; eviction uses a
use counter and a linear scan of the (small) table. `Regexp` objects and their
capture tables remain independent: only the immutable matcher is shared.

**Why:** Anil constructs literal regexps in hot loops; the previous RGSS
capture recorded roughly 62,000–70,000 `Regexp#initialize` calls and 1.76–1.93 s
inclusive. Compiling the same pattern repeatedly is avoidable because the
compiled matcher is immutable.

**Pros:** repeated patterns reuse the expensive compile; the cache is per VM
and bounded, so one embedding cannot grow it without limit; Ruby object
identity and `MatchData` semantics are unchanged.

**Cons:** the cache still allocates one `Regexp` object and runs
`Regexp#initialize` per literal. The current map capture still shows about
**84 `Regexp` objects per frame** (~25 ms over 400 frames including
initialization), so this cache removed the engine compile, not the per-literal
object cost. The capacity eviction scans up to 128 entries on a miss; patterns
larger than 4 KiB are never cached, and an insert at capacity evicts the
least-recently-used entry.

### Unchanged-frame fast path for the standard tile renderer (`18a0a0a`, repaired 2026-10-29)

`TilemapRenderer#update` is replaced by a small Ruby dispatcher:

```ruby
def update
  return false if __sabiruby_tiles_unchanged?(System.uptime)
  result = __sabiruby_original_tile_update
  __sabiruby_record_tiles
  result
end
```

The native check records a signature of the renderer state: the map and its
`Table` revision, display offsets, viewport and clip rect, tone/color,
tilesets/autotiles (including animation frames), the game map/factory, and each
tile sprite's `tile_id`/`need_refresh`. It also checks that the specific
renderer helpers, `Game_Map` accessors and factory method are still the methods
seen at install time. Changed frames call the original Ruby method, so
`Graphics.update` keeps its normal suspension behavior; changed maps, effects,
scrolling, animation, method redefinitions and singleton overrides fall back.

**Why:** the Ruby traversal reads every visible tile cell every frame
(`Table#[]`, `Game_Map#width`, array indexing, comparisons). A 512×384 screen
with the renderer's off-screen margin is roughly 600 tiles; most overworld
frames change none of their tile ids.

**The bug found in this round:** the first version was installed but rarely
used. `Heap::method_serial` changes on any runtime `def` or class allocation,
so normal menus/events/plugins disabled it permanently. Exact class checks also
rejected `$game_map` after the footprints plugin ran
`@character.map.instance_eval { ... }`; `instance_eval` gives the object a
singleton class even though `Game_Map#class` still prints `Game_Map`. The
current version drops the global serial, checks helpers against the actual
receiver/tileset/autotile/map/factory class, accepts kind-of/subclasses, and
guards `Game_Map#tileset_id`, `autotile_names`, `priorities` and
`terrain_tags` as well. Tile `@bitmap`/`@animated` are no longer recorded
separately because the standard renderer writes them together with `@tile_id`.

**Pros:** the largest measured win in the game workload; it preserves the Ruby
implementation for all changed frames and does not require a native tilemap
renderer. It is also robust to the game's normal runtime method definitions and
singleton use.

**Cons:** the unchanged check still reads every tile sprite: about 41 ms over
400 frames (~103 µs/frame) in the current capture. It only covers the standard
Essentials `TilemapRenderer` shape. The signature has a safety contract: code
that changes a tile's `@bitmap`/`@animated` directly without changing its
`@tile_id` would not be seen, because the standard renderer writes those three
together. The record path builds a Ruby array on changed frames. Any new
`TilemapRenderer` helper or map accessor needs a guard and a differential
fixture; otherwise it can be skipped incorrectly.

### Related correctness fixes

These were required for the measurements to be meaningful, but they are not
performance changes:

* `File#read(length)` returns `nil` at EOF (and `File.read(path, length,
  offset)` does too). Save-file copy loops (`s = r.read(4096); break if !s`)
  previously looped forever on the empty string.
* `RgssRuntime` installs a real `Time.now` wall clock. Save timestamps were
  written as 1970, so Multi Save's newest-slot selection failed.
* `File.mtime` reads the VFS modification time, and the VFS reports it for the
  OS backend. Multi Save uses it to label and rotate slots.

## Measured result of the tile repair

Same command, same map, same scripted onboarding inputs, release with
`benchmark,profiling`: `benchmark_map anil 28 400 TRACE` (120-frame warmup).

| metric | before | after |
|---|---:|---:|
| VM mean / frame | 5151 µs | **2441 µs** |
| snapshot mean / frame | 262 µs | 228 µs* |
| total mean / frame | 5413 µs | **2669 µs** |
| p95 frame (CPU) | 6745 µs | 4332 µs |
| CPU frames/sec | 184.7 | **374.7** |
| instructions / frame | 98,781 | **40,017** |
| GC time / 400 frames | 250.7 ms | 81.5 ms |

*The snapshot code did not change in this repair; the 262 → 228 µs difference
is run noise.

The unprofiled build measured **1.95 ms VM / 2.16 ms total / 462 fps**. A
windowed `benchmark_map ... --gpu` run measured **429.9 fps**, 2.30 ms active
mean, 2.37 ms p95, 16 draw calls/frame and no texture uploads. This is a
**2.0× improvement in this map CPU workload**, not a whole-game FPS claim.

## Pros and cons at a glance

| optimization | strongest argument for | main cost / risk |
|---|---|---|
| font cache | 2.37× on the controlled text workload | memory, wholesale eviction, stale font files |
| component equality | removes millions of redundant tile effect writes | numeric coercion must match RGSS |
| inline RGSS attributes | removes `method_missing` from every sprite read | fixed property list, direct ivar semantics |
| snapshot symbol/precomputed projections | keeps snapshot construction cheap as sprite count grows | per-frame HashMaps; state must be stable at the boundary |
| registry pruning | registry tracks live sprites, not history | relies on `dispose` |
| WGPU batching | 14–16 draws and zero uploads in steady state | CPU vertex rebuild/upload each frame |
| regexp engine cache | repeated literals skip the compile | still allocates a Regexp per literal |
| tile fast path | 2.0× on map 28; preserves Ruby fallback | ~103 µs/frame check; standard-renderer contract |

## What the current profile still says

Map 28, 400 profiled frames, most expensive natives:

| native | calls | inclusive | note |
|---|---:|---:|---|
| `TilemapRenderer#__sabiruby_tiles_unchanged?` | 400 | 41.2 ms | the fast-path state walk |
| `Regexp#new` | 33,600 | 16.4 ms | literal construction, engine cache hit |
| `String#[]` | 23,200 | 14.4 ms | game UI/plugin loops |
| `RPG::Event#name` | 43,600 | 11.5 ms | `Game_Event` wrapper through data `method_missing` |
| `String#match` | 10,400 | 9.8 ms | mostly the Auto Weather event-name scan |
| `Array#length` / `Array#[]` | 260k / 158k | 8.6 / 6.6 ms | plugin loops over events |
| `Regexp#initialize` / `Regexp#match` | 33.6k / 10.4k | 8.3 / 8.1 ms | object construction and matching |

The sampled Ruby stacks put most remaining VM samples in the game's per-frame
plugins (`Barras entrenadores`, `Following Pokemon EX`, `Marin's Footprints`,
panorama/field-move updates) and `Scene_Map#updateSpritesets`; those are game
code, not RGSS core. Snapshot construction is about 228 µs/frame and GC about
81 ms per 400 frames (two collections, roughly 40 ms each) in this capture.

## Next steps, in order

1. **Skip the per-tile walk when the guarded renderer methods determine state.**
   The fast path still reads roughly 600 tiles' `tile_id`/`need_refresh` every
   frame. Either keep a tile revision counter updated by the renderer's own
   setters (and a fallback for direct ivar writes), or omit the tile walk under
   the guarded-helper contract. Measure the native's 103 µs/frame, p95 and GC;
   keep the regression test that mutates tiles and scrolls.
2. **Attack literal regexp object construction.** The engine cache removed the
   compile but not the `Regexp` object/`initialize` per literal. A call-site or
   irep literal cache needs compiler/VM cooperation and must preserve Ruby
   object identity. A cheaper first experiment is to measure how much of the
   33,600/400 comes from a single game caller and whether a public, opt-in
   literal cache is acceptable; do not silently change `Regexp.new` identity.
3. **Snapshot dirty tracking.** 228 µs/frame for about 339 sprites. A
   per-sprite property version or a previous-snapshot diff could avoid
   rebuilding unchanged projections, but the snapshot is a read-only frame
   boundary and must not miss Ruby-side mutations. Measure before changing the
   data flow.
4. **GC pauses.** Two collections cost ~81 ms in 400 frames; the 45 ms maximum
   frame in the capture is the same order. Reducing per-frame regexp/snapshot
   allocations and reviewing the GC interval/pause target are separate
   experiments; report p95/p99, not only the mean.
5. **Broaden the profiles.** Map 28 is an overworld CPU workload. Profile a
   battle, a menu with many windows/text draws, a map transition, an indoor map
   and a large/complex map before making claims about the whole game. The
   `benchmark_map` harness only covers the map loop; extending it to load a
   save and script scene transitions is the next tooling step.
6. **Renderer only if GPU-bound.** The current windowed run submits 16 draws and
   spends ~49 µs presenting; the CPU is the limiter. Instancing/atlasing or GPU
   timestamps are only worth doing after a GPU-bound capture.
7. **Add a repeatable performance gate.** Record the map-28 command, trace
   settings, scripted inputs (or the save, once the harness can load one),
   build features and machine in the verification file, and compare an
   alternating A/B round rather than a single run. The current before/after
   table is one run per side; the direction is robust at 2×, but future 5–10%
   candidates need the VM's alternating methodology.

## Reproduction

```sh
nix-shell shell.nix

# Headless CPU profile and summary
cargo build --release -p sabiruby-rgss-player \
  --features benchmark,profiling --example benchmark_map
./target/release/examples/benchmark_map anil 28 400 /tmp/anil-map28.json

# Unprofiled headline
./target/release/examples/benchmark_map anil 28 400

# Windowed CPU/API phases (no GPU timestamps)
./target/release/examples/benchmark_map anil 28 300 --gpu

# Focused tests
cargo test -p sabiruby-rgss-core
cargo test -p sabiruby-rgss-core tile_idle -- --ignored
```

The fast path is a compatibility-sensitive optimization. Every change to it
should keep the differential fixture, the monkey-patch fallback test, the
singleton/runtime-`def` regression and the real map profile green at the same
time.
