# PERF_AI handoff: RGSS savedata / 3× movement profiling

Written 2026-10-30 by the model that did the work. This file is deliberately
detailed: it records the reasoning, the failed attempts, the measurements, and
the exact state of the working tree so a later model can continue without
re-deriving everything.

Repository: `/home/rucadi/i/sabiruby`
HEAD at the time of this work: `03d2cbb fix(rgss): match mkxp-z runtime semantics for Essentials plugins`
All work is **uncommitted**. Do not commit unless the user asks in that turn.

---

## 1. TL;DR

The user asked to profile the first manual save at 3× zoom while moving, find
the worst offenders, and fix them.

The two main wins already in the working tree are:

1. **Snapshot construction caches** in `rgss/core/src/lib.rs`: pre-interned
   property symbols, one projection per bitmap and viewport per snapshot.
2. **A native pixel-shift fast path for the Essentials `TilemapRenderer`** in
   `rgss/core/src/rgss_optimizations/tile_idle.rs`: while the camera moves
   inside a source-tile boundary, move the already-rendered tile sprites
   natively instead of re-running the whole Ruby renderer.

A/B on the same fixed save, 1,440 frames, CPU-only, 1536×1152 (3×) render size:

| metric | shift disabled | shift enabled |
|---|---:|---:|
| elapsed | 28.27 s | **21.12 s** |
| VM instructions | 512.7 M | **275.0 M** |
| `vm.step` total | 24.54 s | **17.19 s** |
| GC time | 3.09 s | **1.67 s** |

That is **25% less elapsed time**, **46% fewer instructions**, and **46% less GC
time** on this workload. The snapshot stage stayed in the same 2.3–2.5 ms/frame
range. This is profiled CPU time, not a whole-game FPS claim.

Also fixed along the way:

- The shift path originally moved every tile stack uniformly. The Ruby renderer
  visits only stacks overlapping a map and clears the rest, leaving their stale
  coordinates. A differential test caught this at the map edge (frame 43).
  The current path reproduces the visited set and only moves visited stacks;
  it falls back when an uncovered stack has not already been cleared or a
  covered stack still needs a refresh.
- `Sprite#dispose` registry pruning had been dropped in commit `97e5ba1`.
  The test for it existed but was never compiled because
  `rgss/core/src/tests/tile_idle.rs` was not included from `lib.rs`. Restoring
  the pruning and activating the test module fixed the regression and keeps
  disposed sprites out of the snapshot scan.
- Ruby's `@screen_moved` / `@screen_moved_vertically` state is mirrored by the
  shift path so scripts that read those ivars see the same values.

---

## 2. Current worktree state

Modified:

```text
 M docs/design/rgss_performance.md
 M rgss/core/src/lib.rs
 M rgss/core/src/rgss_optimizations/tile_idle.rs
 M rgss/core/src/tests/tile_idle.rs
```

Deleted (was a temporary untracked example):

```text
 rgss/player/examples/profile_save.rs
```

The profiler source is preserved in the appendix of this file so it can be
recreated if further measurements are needed. It was removed from the repo
because it hardcodes Essentials/Anil save APIs (`SaveData`, `$scene.main`) and
is not a general player example.

No commit has been made.

---

## 3. The profile workload

Windowed player profiling was attempted first, but this environment has a
Wayland session and the release player could not create an X11/WGPU surface
under Xvfb (missing/incorrect X libraries; winit fell back to Wayland and then
failed to load X11). See section 12. The work switched to a CPU-only harness
that still executes the real game scripts and constructs real render snapshots,
but does not create a window or GPU device.

Workload definition:

- Project: isolated copy/symlink tree of `anil` at `/tmp/opencode/anil-profile`,
  with the first manual save copied to a fixed path.
- Entry point: the compiled `Data/Scripts.rxdata` archive, as the real player
  does.
- Boot: `MessageTypes.load_default_messages`, `PluginManager.runPlugins`,
  `Compiler.main`, `Game.initialize`, `Game.set_up_system`, then load the first
  manual save.
- Scene: `$scene.main`.
- Render size: `project.screen_width * 3` by `project.screen_height * 3`
  (1536×1152 for Anil's 512×384 logical screen).
- Input: hold Down for 120 completed frames, then Right, Up, Left, repeating.
  Directions are changed only when a frame completes.
- Frames: 1,440 completed frames per run.
- Trace capture: `profile::Capture` with a 500,000 VM/host event limit and
  4,096-instruction sampling (the profiling feature defaults).
- No audio device, window, or GPU.

Isolated project setup used:

```sh
rm -rf /tmp/opencode/anil-profile
cp -as /home/rucadi/i/sabiruby/anil /tmp/opencode/anil-profile
rm -f '/tmp/opencode/anil-profile/Data/Partida 1.rxdata' \
      '/tmp/opencode/anil-profile/Data/Partida 1.rxdata.bak'
cp '/home/rucadi/i/sabiruby/anil/Data/Partida 1.rxdata' \
   '/tmp/opencode/anil-profile/Data/Partida 1.rxdata'
```

`cp -as` creates a symlink farm, so graphics/audio/scripts are shared with the
real project; only the save file is a real copy. This keeps repeated runs on
the same save even if the user plays the real project.

Rebuild/run command (after recreating the harness from the appendix):

```sh
nix-shell shell.nix --run \
  'cargo run --release -q -p sabiruby-rgss-player --features profiling \
   --example profile_save -- /tmp/opencode/anil-profile \
   /tmp/opencode/rgss-save-zoom3-moving-current.json 1440'
```

Trace files are in `/tmp/opencode` and may not survive a reboot. Names used:

- `rgss-save-zoom3-moving-before.json` — early capture (old code)
- `rgss-save-zoom3-moving-baseline-temp.json` — caches, shift disabled
- `rgss-save-zoom3-moving-cache.json` — caches + early, buggy shift
- `rgss-save-zoom3-moving-shift-fixed.json` — shift + guard-root fix, no
  coverage guard
- `rgss-save-zoom3-moving-final.json` / `final2.json` — full-coverage guard
- `rgss-save-zoom3-moving-partial.json` — current partial-coverage logic
- `rgss-save-zoom3-moving-noshift.json` — A/B with the shift call disabled

Each has a `.summary.json` and `.stacks.folded` sibling.

---

## 4. Where the time went before the changes

The early capture showed:

- ~516 M VM instructions over 1,440 frames.
- Snapshot construction was ~7.4 s total (~5.15 ms/frame) before the snapshot
  caches, with very poor percentiles.
- The biggest native was `TilemapRenderer#__sabiruby_tiles_unchanged?`, followed
  by a large amount of Ruby work inside `TilemapRenderer#update`:
  `Table#[]`, `RPG::Map#tileset_id`, `Array#length`, `Array#[]`,
  `Array#__svalue`, `Integer#!=`, etc.
- The game's tile renderer loops over every visible grid cell and three layers,
  reads the map `Table` for every cell, and only changes pixel coordinates on
  most movement frames. It performs dozens of Ruby calls per tile per frame just
  to conclude that tile ids did not change.
- Sampled Ruby stacks were dominated by `TilemapRenderer`, generic bytecode
  (`each`, `each_with_index`), `Game_Character`, and the Marin Side Stairs
  plugin's per-frame event scan. The latter is game code and was not modified.
- `Regexp#new`/`Regexp#initialize` was ~0.3 s over the run. The engine already
  caches compiled patterns; the remaining cost is one Regexp object per literal
  evaluation. This was deliberately not changed (see section 10).

The tile renderer's hot loop is in
`anil/Data/Scripts/016_Map renderer/002_TilemapRenderer.rb` (`update`,
`check_if_screen_moved`, `refresh_tile_coordinates`, `refresh_tile_z`). The
source is read-only for this work; all fixes are in `rgss/core`.

---

## 5. Change 1: snapshot construction caches

File: `rgss/core/src/lib.rs`, around lines 1120–1275 and 5124 onward.

### 5.1 Pre-interned symbols

`Vm::ivar_get(object, "@name")` looks the name up in the symbol table on every
call. A snapshot touches every registered sprite and many bitmap/viewport
ivars, so that lookup happened tens of thousands of times per frame.

`SnapshotKeys` now holds `Sym` values for:

```text
@disposed @bitmap @width @height @path @src_rect @viewport @visible
@rect @ox @oy @x @y @z @color @tone @red @green @blue @alpha @gray
@mirror @opacity @zoom_x @zoom_y @angle @blend_type @__sabiruby_plane
```

- `SnapshotKeys::new(vm)` interns all names. It is called by both public RGSS
  installers: `install_rgss` and `install_rgss_with_vfs`.
- `SnapshotKeys::lookup(vm)` is called once per `render_snapshot`; it uses
  `vm.syms.lookup_str` and `expect`s success because the installers interned
  the names. If a future embedding bypasses both installers, this must be
  revisited.
- `color_components` / `tone_components` now take the key table and read the
  four components directly instead of allocating a `Vec` through an iterator
  and `try_into`.

**Why both installers:** `install_rgss_with_vfs` was patched first, then review
found `install_rgss` calls `rgss_interface::install` directly and would panic
in `lookup`. Both now call `SnapshotKeys::new`. This is an easy thing to miss.

### 5.2 Per-snapshot bitmap cache

`SnapshotBitmap` stores `{ width, height, path, pixels, revision }`. Keyed by
`ObjId` in a per-snapshot `HashMap<ObjId, SnapshotBitmap>`.

- Width/height/path no longer read once per sprite using the same bitmap.
- `snapshot_pixels` is still the authoritative `Arc<[u8]>` and is cloned per
  snapshot, not copied.
- `bitmap_revision` is carried through for the renderer's texture cache.
- Path is computed only when the bitmap has no pixel data, matching the old
  path/`AnimatedBitmap` fallback behavior.

The cache is **per snapshot**, not persistent across frames. This was chosen to
avoid stale cached state if Ruby mutates a bitmap between frames; the
correctness argument is the same as before: the snapshot is a read-only
frame-boundary projection. Persistent generation-stamped caches are listed as
future work.

### 5.3 Per-snapshot viewport cache

`SnapshotViewport` stores `{ z, clip_rect, dx, dy, color, tone }`, keyed by
`ObjId` in `HashMap<ObjId, Option<SnapshotViewport>>`.

- `None` means the viewport is disposed or invisible; sprites in it are skipped
  exactly as before.
- Clip rect and dx/dy are computed once per viewport instead of once per child.
- Viewport color/tone overlay is computed once and applied to each child with
  `overlay_colors` / `add_tones`.
- Panoramic mode still forces the clip rect to the full logical size when no
  viewport exists, matching the old code.
- Sort order is unchanged: `(viewport z or sprite z, viewport identity, z,
  creation order)`.
- Numeric behavior is unchanged (`value_i64`/`value_f64` defaults, clamps, and
  src-rect clipping are all the same).

### 5.4 Result

Snapshot total dropped from ~7.4 s to ~3.1–3.7 s over 1,440 frames depending on
run (roughly 5.15 ms → 2.1–2.6 ms/frame). The exact figure varies with CPU load
and with how many tiles are in the registry; do not compare cross-run values
too precisely.

The design doc (`docs/design/rgss_performance.md`) already had a section for
this; it says the cache still allocates a `Vec` and two `HashMap`s per frame.
That is still true.

---

## 6. Change 2: pixel-shift tile fast path

File: `rgss/core/src/rgss_optimizations/tile_idle.rs`.

### 6.1 Existing unchanged path

The existing optimization installs a Ruby dispatcher when the standard
Essentials `TilemapRenderer` shape is detected:

```ruby
class TilemapRenderer
  def update
    now = System.uptime
    return false if __sabiruby_tiles_unchanged?(now)
    return false if __sabiruby_shift_tiles(now)
    result = __sabiruby_original_tile_update
    __sabiruby_record_tiles
    result
  end
end
```

- `__sabiruby_tiles_unchanged?` records a signature of renderer state and tile
  state. If nothing changed, it returns true and no Ruby renderer code runs.
- On a real change, the original Ruby `update` runs outside the native call.
  This matters: `Graphics.update` inside the renderer/plugin stack must remain
  resumable and must not be swallowed by a native frame.
- After the Ruby update, `__sabiruby_record_tiles` records the new signature.

The unchanged path was already present, but it only helps frames with **no**
movement. Movement frames still fell through to the full Ruby traversal.

### 6.2 Why a second path is needed

During movement, `Game_Map#display_x/y` changes almost every frame. Most changes
do not cross a 32-pixel source-tile boundary: the camera's pixel offset changes
but the tile offset stays the same and the set of map cells represented by each
tile stack is unchanged. In that case the Ruby renderer only needs to run
`refresh_tile_coordinates` for every visited stack; tile bitmaps/ids/z values
are unchanged.

The native path detects exactly that case and applies the coordinate formula:

```text
tile.x = column_index * DISPLAY_TILE_WIDTH  - @pixel_offset_x
tile.y = row_index    * DISPLAY_TILE_HEIGHT - @pixel_offset_y
```

This avoids thousands of `Table#[]`, `Array#[]`, `tileset_id`, `Integer#!=`,
and Ruby `each` operations per movement frame.

### 6.3 Guard setup

The installer already verifies the standard renderer source and installs method
guards for renderer helpers, tileset/autotile updaters, map accessors, and the
map factory. For the shift path:

- `Guard` became `Copy`, so the same guard list can be shared by the unchanged
  and shift closures.
- Guard class resolution and validation were factored into `guard_class` and
  `guards_valid` (around lines 84–124).
- `constant_i64` reads the geometry constants once at install:
  `Game_Map::X_SUBPIXELS`, `Y_SUBPIXELS`,
  `TilemapRenderer::SOURCE_TILE_WIDTH/HEIGHT`,
  `DISPLAY_TILE_WIDTH/HEIGHT`, `ZOOM_X/Y`.

**Bug found during development:** the shift closure originally checked
`@__sabiruby_tile_guards` on the renderer *instance*. The array is stored on
the `TilemapRenderer` class. The check always failed. `SABIRUBY_TILE_DEBUG`
prints showed `tile shift fallback: guards nil` on every moved frame. Fixed by
reading the class ivar (`shift_keys.get(vm, class, ...)`).

### 6.4 Shift signature

`__sabiruby_record_tiles` now records three signatures:

1. `@__sabiruby_tile_signature` — full state including view position
   (`include_view_position = true`, tile state included). Used by the unchanged
   path.
2. `@__sabiruby_tile_guard_signature` — guard-relevant state without the tile
   walk (`include_tile_state = false`, `include_view_position = false`). Used to
   cheaply decide whether the unchanged path is safe.
3. `@__sabiruby_tile_shift_signature` — tile state only
   (`include_tile_state = true`, `include_view_position = false`). Used by the
   shift path.

The shift signature deliberately excludes `@tile_offset_x/y`,
`@pixel_offset_x/y`, `@old_viewport_ox/oy`, and the map display positions,
because those are exactly what changes during a pixel shift. It still includes:

- Renderer layout/class checks.
- `@need_refresh`, tile array identities, horizontal/vertical counts, current
  map id, bridge state.
- Screen width/height.
- Tone/old tone/color/old color components.
- Viewport identity, ox/oy, disposed, rect.
- `$PokemonGlobal.bridge` and `$PokemonSystem.autotile_animations`.
- `$game_map` identity/map id and, for every factory map, map identity, data
  map identity, width/height/tileset id, `Table` identity and revision.
- Tilesets/autotiles objects, bitmap entries, animation frame counters,
  durations, current frames, wraps, timer, and the animation-frame check at
  `System.uptime`.
- Every tile sprite's `@need_refresh` and `@tile_id`.

`visit` returns `None` (fallback) if any expected value cannot be read or if
the animation frame implied by `now` does not match the recorded frame.

### 6.5 `__sabiruby_shift_tiles` algorithm

Order matters; all validation happens before mutating renderer state.

1. Receiver must be an object.
2. Guard root must exist on the class and `guards_valid` must pass.
3. The stored shift signature array must exist.
4. Run `visit` with current `System.uptime`, tile state, and
   `include_view_position = false`; compare every emitted value with the stored
   signature in order; require equal length.
5. Viewport must exist; `viewport.ox/oy` must equal the renderer's
   `@old_viewport_ox/oy`. Viewport scrolling forces the Ruby full refresh.
6. `$game_map` must exist.
7. Geometry constants must be positive.
8. Compute current map display in Ruby units:
   - `map_display_x = round(display_x / X_SUBPIXELS)`
   - same for y. Rust `f64::round()` rounds halves away from zero, matching
     Ruby `Float#round`.
9. Tile offsets must be unchanged:
   - `tile_offset_x = floor_div(map_display_x, SOURCE_TILE_WIDTH) * ZOOM_X`
   - `div_euclid` matches Ruby integer `/` floor semantics for negative values.
   - Compare with `@tile_offset_x/y`. A change means a tile boundary was
     crossed / stacks must rotate; fall back to Ruby.
10. Pixel offsets:
    - `pixel_offset_x = rem_euclid(map_display_x, SOURCE_TILE_WIDTH) * ZOOM_X`
    - same for y.
    - If both are unchanged, this is not a pixel-shift frame; fall back (the
      unchanged path or Ruby state handles it).
11. Read `@tiles`, column count, and row count. All columns must have the same
    row count; otherwise fall back.
12. Compute the renderer's visited set for every map in `$map_factory.maps`:
    - per-map display in Ruby units, with the ZOOM adjustment from the Ruby
      source when `ZOOM_X/Y != 1`:
      `((display + Graphics.width/2) * ZOOM) - Graphics.width/2`
    - `display_x_tile = floor_div(display, DISPLAY_TILE_WIDTH)`
    - `start_x = max(-display_x_tile, 0)`
    - `end_x = min(column_count - 1, map_width - display_x_tile - 1)`
    - skip a map if `start > end`, or either end is negative
    - mark `visited[i * row_count + j] = true` for the inclusive rectangle.
    - `screen_width/height` come from `RgssState` (the same values backing
      `Graphics.width/height`).
    - This mirrors `TilemapRenderer#update`'s visited array.
13. Validate every tile stack before mutating anything:
    - covered stack: every tile must have `@need_refresh` false. The previous
      Ruby update rendered it; a pending refresh means coverage grew and Ruby
      must render the newly visible tile.
    - uncovered stack: every tile must have `@need_refresh` true and
      `@tile_id == 0`. This is the state left by Ruby's
      `tile.set_bitmap("", 0, false, false, 0, nil)` clear. If not, fall back
      so Ruby performs the clear.
14. Write `@pixel_offset_x/y`.
15. Write `@screen_moved = true` and
    `@screen_moved_vertically = (new_pixel_y != old_pixel_y)`, matching the
    Ruby `check_if_screen_moved` result for this frame.
16. Move only covered stacks: set `x`/`y` on each tile in covered cells.
    Uncovered stacks keep their stale coordinates, exactly like Ruby.
17. Increment `tile_fast_hits` and `tile_shift_hits` in `RgssState`.

### 6.6 Why partial coverage is safe

Ruby's update loop:

```ruby
start_x = [-map_display_x_tile, 0].max
...
@tiles[i][j].each do |tile|
  ...
  refresh_tile_coordinates(tile, i, j) if @screen_moved
  ...
end
visited[i][j] = true
...
@tiles.each_with_index do |col, i|
  col.each_with_index do |coord, j|
    next if visited[i][j]
    coord.each do |tile|
      tile.set_bitmap("", 0, false, false, 0, nil)
      tile.shows_reflection = false
      tile.bridge = false
    end
  end
end
```

The native path leaves uncovered tiles alone rather than clearing them again.
This is only allowed if the previous Ruby frame already cleared them, which the
`need_refresh == true && tile_id == 0` check proves. The clear is idempotent in
terms of the state the snapshot reads (`bitmap`, `tile_id`,
`need_refresh`, coordinates). It does replace `src_rect` with a fresh `Rect`
and writes `visible`, `zoom`, filename, flags, etc. Those are not part of the
tile signature and are not observed by the render snapshot for a cleared tile
(no bitmap), but a script that holds a `src_rect` reference and mutates it while
the tile is outside every map could observe a difference. This is an accepted
trade-off and a test gap; see section 10.

If coverage shrinks and an uncovered tile still has `need_refresh == false` or a
non-zero tile id, the path bails and Ruby clears it. If coverage grows and a
covered tile still has `need_refresh == true`, the path bails and Ruby renders
it.

### 6.7 Map-edge debugging story

This is the most important correctness lesson from the work.

The first version moved every tile stack in the grid. The differential test
`pixel_movement_matches_reference_across_tile_boundaries` failed at frame 43:

- Reference third column: `[-31, -21, ...]`
- Optimized third column: `[64, -22, ...]`

The map in the fixture is 3 tiles wide and the viewport grid is 3 tiles wide
(`Graphics.resize_screen(64, 64)` → `ceil(64/32)+1 = 3`). At frame 43 the map
has scrolled so that `map_display_x_tile = 1`; only grid columns 0 and 1
overlap the map. Ruby clears column 2 and leaves its old x/y. The naive native
path happily moved column 2 to the new uniform coordinate. Hence the mismatch.

The first fix was to require full coverage and fall back whenever any stack was
unvisited. That is correct and was measured (21.76 s vs 21.12 s for the partial
logic). The final logic allows partial coverage when the uncovered stacks are
already in the cleared state, recovering some of the lost frames.

Frames where the camera crosses a source-tile boundary already fall back for a
different reason: `@tile_offset_x/y` changes and Ruby rotates the stacks and
marks entire rows/columns `need_refresh`.

---

## 7. Change 3: restore render-registry pruning

`Sprite.new`/`initialize` registers the object in
`Sprite.@__sabiruby_instances`. The design doc and commit `18a0a0a` say
`dispose` removes it. Commit `97e5ba1` ("optimizations") accidentally dropped
that block from `object_dispose`.

The registry test existed in `rgss/core/src/tests/tile_idle.rs` but the module
was never included from `lib.rs`, so the regression was invisible. Adding

```rust
#[path = "tile_idle.rs"]
mod tile_idle;
```

to `mod tests` activated it and it failed with `501 / 501 / 501` instead of
`1 / 0 / 1`.

Restored block in `object_dispose` (`rgss/core/src/lib.rs`, around line 4928):

- If `@__sabiruby_registered` is truthy, find the `Sprite` class, get its
  registry array, remove the receiver by identity, and set the marker false.
- Re-`initialize` registers again because the marker is false.
- This is O(n) per disposal. The test disposes 500 sprites and passes. Mass
  disposal could be O(n²); a future index map would need to preserve registry
  order because equal-z sprites use creation order.

This is both a correctness fix and a performance help: disposed sprites no
longer stay in the array scanned by `render_snapshot`.

---

## 8. Measurements (all known runs)

All are the same CPU-only savedata/3×/movement workload unless noted. Do not
sum inclusive native times. Times are wall-clock from the profiling capture.

| run | elapsed | instructions | GC | vm.step | snapshot | tile counts |
|---|---:|---:|---:|---:|---:|---|
| early `before` | 25.48 s | 381.4 M | 1.96 s | 17.75 s | 7.42 s | n/a |
| caches, shift disabled (`baseline-temp`) | 28.94 s | 516.4 M | 2.96 s | 24.81 s | 3.72 s | unchanged 968, full 115 |
| caches + early buggy shift (`cache`) | 18.72 s | 282.7 M | 1.49 s | 15.40 s | 3.08 s | unchanged 657, shifted 634, full 74 |
| shift fixed, no coverage guard | 20.07 s | 293.4 M | 1.60 s | 16.27 s | 3.55 s | unchanged 591, shifted 683, full 79 |
| full-coverage guard (`final`) | 21.76 s | 280.2 M | 1.81 s | 17.54 s | 3.96 s | unchanged 891, shifted 395, full 85 |
| full-coverage guard repeat (`final2`) | 21.77 s | 284.1 M | 1.76 s | 17.75 s | 3.75 s | unchanged 775, shifted 514, full 87 |
| partial coverage, current (`partial`) | **21.12 s** | **275.0 M** | **1.67 s** | **17.19 s** | 3.66 s | unchanged 829, shifted 465, full 84 |
| shift disabled, contemporaneous A/B (`noshift`) | 28.27 s | 512.7 M | 3.09 s | 24.54 s | 3.33 s | unchanged 958, full 112 |

Caveats:

- The early `before` row is **not** a clean baseline for the final code. It was
  measured before several code changes and its instruction count is much lower
  than the later "caches, shift disabled" run, which is counter-intuitive. The
  likely causes are different code at the time, different input timing, and
  profiler/allocator variance. Use `noshift` vs `partial` as the A/B.
- Tile counts do not sum to 1,440 because the renderer update is not called on
  every completed frame in some scene paths and because input direction changes
  are tied to completed frames, which shifts the composition of the run.
- The buggy-shift 18.72 s row is not a valid target: it produced wrong output at
  map edges. The speed difference versus 21.12 s is partly the missing
  correctness work and partly run-to-run variation.
- Snapshot totals fluctuate 3.1–4.0 s between runs even with the same code.
  Do not claim snapshot improvements from cross-run deltas.
- The machine has demonstrated ±4–12% variance in VM benchmarks; repeat runs or
  alternate A/B before claiming 1% differences.
- The `partial` row was measured just before the final two `@screen_moved` ivar
  writes and the `tile_fast_hits` inclusion change. Both are negligible (two
  extra ivar writes per shifted frame and one counter increment), but rerun if
  exact numbers are needed.

Profiler counters are in `tile_fast_hits`, `tile_shift_hits`, and
`tile_fast_misses` in `RgssState`. Note that `tile_fast_hits` now includes
shift hits, so:

```text
unchanged = tile_fast_hits - tile_shift_hits
shifted   = tile_shift_hits
full      = tile_fast_misses
```

`RgssRuntime::tile_fast_path_stats()` still returns `(hits, misses)`; hits now
cover both native paths.

---

## 9. Tests and validation performed

Passing:

```sh
nix-shell shell.nix --run 'cargo fmt -p sabiruby-rgss-core -p sabiruby-rgss-player -- --check'
nix-shell shell.nix --run 'cargo check --workspace'
nix-shell shell.nix --run 'cargo test -p sabiruby-rgss-core'
nix-shell shell.nix --run 'cargo test -p sabiruby-rgss-core tests::tile_idle -- --ignored'
```

Results:

- `cargo test -p sabiruby-rgss-core`: **64 passed, 0 failed, 5 ignored**.
- `cargo test -p sabiruby-rgss-core tests::tile_idle -- --ignored`: **5 passed**
  (all tile fixtures).
- `cargo check --workspace`: passes.
- Formatting check: passes.

Not run successfully:

- `tools/check_no_std.sh`: the Nix toolchain does not have the
  `thumbv7em-none-eabi` target installed (`can't find crate for core`). The
  change does not touch the no_std VM (`src/` unchanged), but this must be
  verified in an environment with the target before claiming the full checklist.
- `cargo clippy -p sabiruby-rgss-core --lib -- -D warnings`: fails on many
  pre-existing warnings in `sabiruby` (92 errors) and in `rgss-core` (372
  errors, mostly missing `# Errors` docs and `must_use`). These are not caused
  by this work. If clippy is wanted, run `--no-deps` and do not use
  `-D warnings` unless the pre-existing backlog is addressed.

Tests added:

`rgss/core/src/tests/tile_idle.rs::pixel_movement_matches_reference_across_tile_boundaries`

- Builds the same fixture twice: one runtime with the optimizer installed,
  one without.
- Creates a `Viewport`, `TilemapRenderer`, tileset and autotile bitmap, then
  runs 50 frames of `display_x = i * 3`, `display_y = i * 2`.
- Prints the complete tile grid state
  `[tile.x, tile.y, tile.z, tile.tile_id, tile.need_refresh]` plus
  `@screen_moved`/`@screen_moved_vertically` each frame.
- Compares line by line and reports the first differing frame, avoiding the
  enormous raw output from the initial `assert_eq!`.
- Asserts 50 lines and that `tile_shift_hits > 0`.

There is no non-ignored test for the shift path because the fixture needs the
local Anil project. CI without Anil will only run the registry test.

---

## 10. Open issues / next steps, in priority order

1. **Negative scrolling coverage.** The differential test only scrolls
   right/down from zero. The shift path uses `div_euclid`/`rem_euclid` and the
   visited-set formulas for negative display offsets, but there is no fixture
   for scrolling left/up across the map origin or across negative tile offsets.
   Add a second segment to the test (or a separate test) before trusting the
   path for all directions. The real savedata profile does move Left/Up later,
   but the differential test does not.
2. **Strengthen the edge-state comparison.** The movement test compares
   x/y/z/tile_id/need_refresh and the two `screen_moved` flags, but not
   `src_rect`, `visible`, `filename`, `is_autotile`, `animated`, `priority`,
   `shows_reflection`, or `bridge`. The skipped idempotent clear only changes
   those. Extend the fixture to compare them if practical.
3. **Signature-walk cost.** `TilemapRenderer#__sabiruby_tiles_unchanged?` was
   still the top native in the savedata profile: 1.48 s inclusive over 2,197
   calls (~0.67 ms/call) with the shift path active. It walks every tile and
   compares two ivars. Ideas: hash the signature instead of storing/comparing
   per-value slots; track a renderer-owned tile revision; or omit the tile walk
   under a stronger helper contract with a fallback for direct ivar mutation.
   Do not remove the differential tile tests.
4. **Regexp literal construction.** `Regexp#new` + `Regexp#initialize` still
   costs ~0.3 s and ~386k calls in this workload. The compiler emits
   `Regexp.compile` on every literal evaluation (mruby-style). MRI caches one
   Regexp per literal site. Changing this is a compiler/VM semantic change and
   would diverge from mruby/mkxp; the design doc explicitly says not to silently
   change `Regexp.new` identity. Decide with the user before touching it.
5. **Tile-boundary frames.** 84 full Ruby frames remain. Extending the native
   path to rotate stacks and refresh the entering row/column is much more
   complex (`refresh_tile_bitmap`, terrain tags, autotile selection, tileset
   wrappers). Do not attempt without a strong differential fixture.
6. **Snapshot dirty tracking / persistent caches.** Snapshot is still
   ~2.3–2.5 ms/frame. Per-frame HashMaps allocate. A generation-stamped
   bitmap/viewport cache in `RgssState`, or a previous-snapshot diff, could
   help, but the snapshot must not miss Ruby mutations at the frame boundary.
   Measure before and after; do not assume the cache is the bottleneck.
7. **Registry pruning complexity.** O(n) removal preserves creation order. If a
   game mass-disposes thousands of sprites, consider an index map keyed by
   object id while preserving order. The current test only covers 500.
8. **Full player measurements.** The CPU-only harness excludes WGPU, window,
   presentation, and audio. Once X11/WGPU works in this environment, run the
   windowed player on the same savedata scene and compare frame pacing, not
   just CPU work. The failed attempt is documented below.

---

## 11. Do-not-do list

- Do not patch game scripts or add Anil-specific compatibility code to the
  engine. The user has rejected endless game-specific fixes.
- Do not run `cargo fmt --all`; format only the RGSS packages and explicitly
  edited files.
- Do not commit, amend, rebase, or squash unless the user asks in that turn.
- Do not change `Regexp.new`/literal identity without an explicit decision;
  it is a VM semantic change.
- Do not move `Graphics.update` work into a native call or swallow Ruby
  exceptions. `Graphics.update` must remain resumable.
- Do not make a native tilemap renderer out of the fast path; keep Ruby
  authoritative for changed content.
- Do not trust a single profiled run for a small delta. Repeat or alternate
  A/B.

---

## 12. Debugging log: things that failed or wasted time

1. **Windowed profiler under Xvfb.**
   - `DISPLAY=:0` exists but the session is Wayland.
   - `xvfb-run` was not on PATH; started `Xvfb :99`.
   - winit tried Wayland and failed (`wayland library could not be loaded`),
     then X11 failed (`Failed to load one of xlib's shared libraries`).
   - Built an xlibs environment with `buildEnv` and set `LD_LIBRARY_PATH`;
     WGPU then failed with `Failed to create surface for any enabled backend`.
   - Conclusion: CPU-only `profile_save` harness is the reliable path here.
2. **xdotool window id was empty** because the window never appeared; the
   script waited and the player exited/failed. Do not spend more time on Xvfb
   unless the environment changes.
3. **Huge test output.** The first differential `assert_eq!` printed entire
   serialized arrays for every frame and was unreadable. Compare per-line and
   report the first differing frame.
4. **`$tile_renderer` was nil in the new test.** The fixture setup does not
   create a renderer; only `tile_scene_steps.rb` does. The movement test must
   create its own `Viewport`/`TilemapRenderer`/bitmaps.
5. **Guard root on the wrong object.** The shift closure checked
   `@__sabiruby_tile_guards` on the renderer instance; the installer stores it
   on the class. Debug output `tile shift fallback: guards nil` led to the fix.
6. **Temporary debug instrumentation.** A `SABIRUBY_TILE_DEBUG` env-var path
   printed fallback reasons and guard changes. It was removed before final
   validation. If resurrected, remember `cargo run` must rebuild after edits.
7. **Sed-based rename bug.** A scripted `sed` replaced `keys.` with
   `shift_keys.` inside the line that declared `let shift_keys = keys.clone()`,
   producing `let shift_keys = shift_keys.clone();`. Fixed manually.
8. **`Arc` moved into a closure.** The shift closure moved `keys` (an `Arc`)
   while the record closure also needed it. Fixed by cloning the `Arc` for the
   shift closure (`let shift_keys = keys.clone()`).
9. **Coverage guard first attempt** required full coverage and was correct but
   lost ~3 s. The partial-coverage/idempotent-clear logic recovered most of it.
10. **Do not use `RgssState.width` before checking host state exists.** The
    shift closure reads host state for the zoom formula and bails if absent.

---

## 13. File and symbol map

`rgss/core/src/rgss_optimizations/tile_idle.rs`

- `keys!` macro / `Keys` — interned ivar names, layouts, tile fields.
- `array` — read an array's slots.
- `plain_reader`, `method_matches` — method guard helpers.
- `GuardTarget`, `Guard`, `guard_class`, `guards_valid` — install-time method
  guards.
- `constant_i64` — reads constants at install.
- `install` — validates the standard renderer, aliases `update`, installs
  closures, records roots/guards/signatures.
- `__sabiruby_tiles_unchanged?` — unchanged frame path.
- `__sabiruby_shift_tiles` — pixel movement path (lines ~347–605).
- `__sabiruby_record_tiles` — records full/guard/shift signatures.
- `visit` — shared state walk (line ~664).
- `DISPATCH` — Ruby dispatcher source (line ~909).
- `tile_fast_hits`, `tile_shift_hits`, `tile_fast_misses` in `RgssState`.

`rgss/core/src/lib.rs`

- `SnapshotKeys`, `SnapshotBitmap`, `SnapshotViewport` (~1120–1248).
- `color_components`, `tone_components` (~1250–1275).
- `object_dispose` registry pruning (~4928).
- `install_rgss` / `install_rgss_with_vfs` call `SnapshotKeys::new` (~4994).
- `tile_fast_path_stats` (~5115).
- `render_snapshot` (~5124).
- `mod tile_idle` test inclusion (~5562).

`rgss/core/src/tests/tile_idle.rs`

- `runtime(optimized)` — opens Anil, loads fixture setup and renderer, installs
  the optimizer and dispatcher when requested.
- `pixel_movement_matches_reference_across_tile_boundaries` (~54).
- `render_registry_prunes_disposed_objects_without_duplicate_registration`
  (~187).
- Other ignored tests cover edits, scrolling, effects, animation, GC, patched
  helpers, singleton helpers, and unrelated runtime defs.

`docs/design/rgss_performance.md`

- Snapshot cache section.
- New `Pixel-shift fast path for moving tiles (2026-10-30)` section with the
  A/B table.
- Pros/cons table updated with the pixel-shift row.

`/tmp/opencode/anil-profile` — isolated project copy.
`/tmp/opencode/rgss-save-zoom3-moving-*.json` — traces and summaries.

---

## 14. Appendix: temporary profiler harness source

Recreate `rgss/player/examples/profile_save.rs` with this content if profiling
is needed again. It was deleted from the repo because it assumes Essentials
`SaveData`/`$scene` APIs. The tile counter print has been adjusted for the new
meaning of `tile_fast_hits`:

```rust
//! Profiles the first manual save on the current map with panoramic zoom and
//! scripted movement. CPU-only: it advances the VM and builds render snapshots
//! but does not create a window or GPU device.
#[path = "../src/profile.rs"]
mod profile;

use sabiruby::Step;
use sabiruby_rgss_core::{Project, RgssRuntime};

fn main() {
    let mut args = std::env::args().skip(1);
    let root = args
        .next()
        .expect("usage: profile_save PROJECT [TRACE.json] [FRAMES]");
    let trace = args
        .next()
        .unwrap_or_else(|| "/tmp/opencode/rgss-save-zoom3-moving.json".into());
    let frames: u64 = args.next().map(|s| s.parse().unwrap()).unwrap_or(1440);
    let project = Project::open(root).unwrap();
    let mut runtime = RgssRuntime::open(project).unwrap();
    runtime
        .run_source(
            "count-scripts.rb",
            br#"puts load_data("Data/Scripts.rxdata").size"#,
        )
        .unwrap();
    let total: usize = String::from_utf8(runtime.take_output())
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    runtime.run_scripts(Some(total.saturating_sub(1))).unwrap();
    runtime
        .run_source(
            "__profile_save_boot.rb",
            br#"
MessageTypes.load_default_messages if FileTest.exist?("Data/messages_core.dat")
PluginManager.runPlugins
Compiler.main
Game.initialize
Game.set_up_system
slot = SaveData.respond_to?(:get_manual_slots) ? SaveData.get_manual_slots.first : nil
slot ||= SaveData.get_newest_save_slot if SaveData.respond_to?(:get_newest_save_slot)
path = slot ? SaveData.get_full_path(slot) : SaveData::FILE_PATH
Game.load(SaveData.read_from_file(path))
"#,
        )
        .unwrap();
    runtime.set_render_size(
        runtime.project.screen_width.saturating_mul(3),
        runtime.project.screen_height.saturating_mul(3),
    );
    let mut capture = profile::Capture::new(&mut runtime, trace.into(), 4096, 500_000);
    runtime
        .start_source("__profile_save_scene.rb", b"$scene.main")
        .unwrap();

    let directions = [2, 6, 8, 4];
    let mut current = None;
    let mut done = 0;
    while done < frames {
        if done % 120 == 0 {
            if let Some(previous) = current {
                runtime.input_release(previous);
            }
            let next = directions[(done / 120) as usize % directions.len()];
            runtime.input_press(next);
            current = Some(next);
        }
        let before = runtime.completed_frames();
        let start = profile::clock_ns();
        let result = runtime.step(250_000).unwrap();
        capture.span("vm.step", start);
        if runtime.completed_frames() > before {
            let start = profile::clock_ns();
            let snapshot = runtime.render_snapshot();
            capture.span("snapshot", start);
            capture.completed_frames += runtime.completed_frames() - before;
            capture.sprites += snapshot.sprites.len() as u64;
            capture.counters(serde_json::json!({
                "sprites": snapshot.sprites.len(),
                "zoom": 3,
                "direction": current,
            }));
            done += 1;
        } else {
            capture.budget_pauses += 1;
        }
        if matches!(result, Step::Finished(_)) {
            break;
        }
    }
    if let Some(direction) = current {
        runtime.input_release(direction);
    }
    if let Some(state) = runtime
        .vm
        .host_state::<sabiruby_rgss_core::RgssState>()
    {
        let shifted = state.tile_shift_hits;
        eprintln!(
            "tile paths: unchanged={} shifted={} full={}",
            state.tile_fast_hits.saturating_sub(shifted),
            shifted,
            state.tile_fast_misses
        );
    }
    capture.finish(&runtime).unwrap();
}
```

Notes for re-running:

- The harness requires the `profiling` feature and the player's existing
  `profile.rs` module.
- `runtime.input_press` numbers follow RGSS: 2 = down, 4 = left, 6 = right,
  8 = up.
- 250,000 instructions is the VM budget per `step`; a budget pause does not
  complete a frame. The harness only counts completed frames and only renders
  a snapshot then.
- `capture.span("vm.step", start)` wraps the whole budget slice, including
  frames that did not complete; that is why `vm.step.calls` is larger than
  1,440 in the summaries.
- The first manual save is assumed to be slot 1 / the first result of
  `SaveData.get_manual_slots`. Change this if the save layout differs.
- If the game has no `SaveData`, this harness is not applicable; use
  `benchmark_map` or a generic headless boot instead.

---

## 15. Quick continuation checklist

1. Run `git status` and read this file; do not assume the tree is clean.
2. Run the four validation commands in section 9 (tile ignored tests included).
3. If changing the tile path, run
   `cargo test -p sabiruby-rgss-core tests::tile_idle -- --ignored` and add a
   differential frame comparison for the new case.
4. If changing snapshot code, run the core tests that cover
   `render_snapshot`/sprites and check `docs/design/rgss_performance.md`.
5. If measuring, recreate the harness from section 14 and the isolated project
   from section 3; use the same save, render size, input sequence, and frame
   count.
6. Do not commit unless the user explicitly asks.

---

## 16. Session 2 (2026-10-30): Anil + Pokémon Z (`pz/`)

All uncommitted. Validation at the end: every crate's tests pass (VM 171, compiler 20,
rgss-core 70 + 5 ignored tile fixtures, wgpu 10, player 6, cli 13, serde 47, macros 29);
`cargo check --workspace` clean; RGSS packages formatted. `tools/check_no_std.sh` still
cannot run here (no thumbv7em core in the Nix toolchain).

### Performance changes (both games)
- `Cargo.toml` release profile: `lto = "fat"` (codegen-units = 1 was already set).
- Snapshot: one pass over each sprite's ivars (`SnapshotKeys::sprite_fields`), Color/Tone
  in one pass, transparent overlays skipped, sort of `(key, index)` pairs, last frame's
  draw order reused when still sorted (`RgssState::snapshot_order`), pre-sized output.
- GC: the byte axis scales with the live heap (`Heap::malloc_limit`, docs/design/gc.md).
  Anil collected every ~60 frames (38 ms each); now every ~120+.
- VM: constant cache for `const_lookup_slow` (`Vm::const_cache`, keyed by
  class/proc/name, `method_serial` + cleared on GC); `Array#length/size/[]` answered in
  the send when the resolved method is the builtin (`Vm::op_send_leaf`); `Op::from_u8`
  `#[inline(always)]` (it was not inlined under fat LTO).
- Regexp: `SABIRUBY_REGEXP_TRACE` read once (getenv on every match before).
- `Bitmap#blt`: clipped row copies, source borrowed in place (was a full source clone
  and per-pixel bounds checks). Pokémon Z's Ruby `Plane` emulation spent ~1 ms/frame here.
- `OsVfs`: per-directory lowercase index for case-insensitive misses, validated by mtime
  and dropped on VFS writes (each miss re-read whole directories).
- tile_idle `Keys::get` `#[inline(always)]` (its `&str` match was evaluated at runtime).

Numbers (CPU harness, noisy machine): Anil save walk 1×: ~178 → ~206 fps, p99 42 → ~10 ms;
3×: ~100 → ~135 fps. Pokémon Z new-game walk (map 59): ~343 → ~365 fps; deterministic
x86 instructions/frame −6.6%. Pokémon Z user save (map 2): ~310 → ~520 fps.

### Pokémon Z compatibility (RGSS1, Ruby 1.8; `Project::ruby18`)
`ruby18` = Game.ini `Library=RGSS1*` and no bundled modern Ruby DLL (Anil ships
mkxp-z's `x64-msvcrt-ruby310.dll`, so it stays modern). `rgss_interface/ruby18.rs`:
`String#[]` with an Integer → byte; `Object#id`; `Kernel#proc` → lambda with loose arity
(`ProcData::loose_arity`, VM); `IO#getc` → Integer. Compile fallbacks (`compile_rgss`):
`when x:` → `then` and `retry` in a `loop` block → `next`, only at positions prism rejects.
General fixes: tolerant Win32API signatures (`'%w(l p)'`), real Zlib inflate/deflate,
`save_data`, `Thread.critical`, `ObjectSpace.define_finalizer/_id2ref` (registry; ids are
heap slots), Ruby 1.8 Marshal float mantissa bytes, `File.extname`, `File.open` read modes
raise ENOENT/EISDIR, IO `getbyte/gets/each_byte/each_line/readlines/...` with a per-File
buffer, `ENV` (only TEMP/TMP = "."), fonts matched by TrueType family name, compiler long
jumps for >32 KiB methods (VENDOR.md third patch), wgpu paging of bitmaps taller than the
GPU texture limit (`rgss/wgpu/src/paging.rs`, Pokémon Z's 256×30004 tileset).

### Temporary local tools (rgss/player/examples, not general examples)
`bench_save.rs` (args: PROJECT FRAMES ZOOM [WARMUP] [SETUP.rb|-] [DUMP_DIR|-] [TRACE]),
`rgss_probe.rs`, `rgss_drive.rs`, `support/compose.rs` (CPU compositor, no blend modes),
and a temporary `image` dev-dependency in rgss/player/Cargo.toml for frame dumps. Setup
scripts used: Pokémon Z new game on map 59 / continue from `pz/Game.rxdata` (run on a
symlinked copy so the save is never written).

### Open items
- Regexp literals still allocate a Regexp per evaluation (mruby semantics; MRI caches per
  literal). ~140–260 per frame in these games; needs a decision (VM semantic change).
- `Array#each` and blocks are mrblib bytecode: per iteration a `BLKCALL` and two sends
  (the sends are now inline). Biggest remaining generic VM cost (~22% of instructions).
- GC is stop-the-world: 33–50 ms pauses, now rarer. Incremental/generational is the fix.
- Anil 3×: tile-boundary frames still run the full Ruby `TilemapRenderer#update`
  (~700k instructions each at 3×).
- Not verified on GPU here: the paging fix (user saw the 256×30004 error in the windowed
  player). Rendering of Pokémon Z was only checked through the CPU compositor.
