# Embeddable RGSS player: Anil first

Date: 2026-10-28. Status: implementation proposal, based on repository inspection;
no claim that Anil currently boots or that performance targets have been met.

## Outcome and boundaries

Run the bundled Pokémon Anil with SabiRuby in place of mkxp-z. Deliver a reusable
RGSS1 library and WGPU renderer, plus a small desktop test player with display,
sound, controls, FPS statistics and an unlock switch. Embedding applications must
be able to supply their existing GPU, render target, clock, inputs, filesystem and
audio output without adopting our window or event loop.

Keep the existing `sabiruby` crate as the general-purpose `no_std + alloc` VM.
Put game runtime behavior, native RGSS implementations and their optimizations
in library crates. The desktop player contains only platform setup and wiring.
Anil-specific accommodations belong in an explicit game profile, not in the VM.

“No performance penalty” means no avoidable embedding tax relative to running
the same library in our own player. It cannot honestly mean zero GPU/API cost,
or faster than mkxp-z before measurement. Benchmark embedding overhead separately
from total game performance; both are release gates below.

Initial target: desktop Linux in this workspace, with portable Windows/macOS
interfaces. RGSS2/3, arbitrary CRuby native extensions, browser deployment and a C
ABI are later scopes. Standard RGSS1 remains an explicit compatibility target:
Anil passing alone does not establish support for all RPG Maker XP games.

## What is already here

| Evidence | Consequence |
| --- | --- |
| Root `Cargo.toml`, `src/lib.rs` | VM already is a library; do not turn it into a desktop engine. |
| `src/vm.rs`: `define_method`, host state, Data handles, `gc_register`, `start`/`step` | Reuse native registration, rooted Ruby references and stepped execution. |
| `src/host.rs`, `compiler/` | Compiler is separate; source and dynamic `eval` can compile to RITE. |
| `src/builtins/ext_require.rs`, `src/mrblib/require.rb` | File loading hooks exist, but these are not full Ruby IO/File/Dir implementations. |
| README compatibility policy | VM targets mruby 4.1.0-rc2, not the full CRuby environment used by the game. |
| README performance section | Existing VM benchmarks do not establish game performance or parity with mkxp-z. |
| `anil/Game.ini` | RGSS1 DLL name, `Data\\Scripts.rxdata` entry point. |
| `anil/Data/Scripts/001_Settings/002_Settings.rb` | Game version `4.1.1`, Essentials `21.1`, expected mkxp-z `2.4.2/d13f35c`. |
| `anil/mkxp.json` | Initial screen 512×384, vsync/refresh synchronization, bundled MIDI soundfont. |
| `anil/Data/Scripts/003_Technical/002_MKXP_Compatibility.rb` | Needs Encoding setters, Font, Bitmap text, System and Graphics extensions at startup. |
| `anil/Data/Scripts/003_Technical/006_PluginManager.rb` | Plugins load through Marshal, zlib and `eval(code, TOPLEVEL_BINDING, filename)`. |
| `anil/Data/Scripts/999_Main/999_Main.rb` | Startup includes plugins, compiler/data checks and Game initialization before the title scene. |
| `anil/Data/Scripts/067_Scripts de la base/022_Turbo.rb` | Existing turbo replaces `System.uptime`, aliases unscaled time, and changes input handling. |
| `anil/Plugins/Discord RPC API/script.rb` | MiniFFI DLL integration and a wrapper around `Graphics.update`; needs a portable policy. |

Static scan found 356 Ruby files under `Data/Scripts` and 328 under `Plugins`.
These are source inventory counts, not the number of enabled or executed scripts.
`Scripts.rxdata` starts with Marshal 4.8 bytes (`04 08`); both it and the plugin
archive exist. The player decodes and inflates this archive in order; extracted
source files are not a second runtime entrypoint. Do not assume an extraction
utility's order is the current contents of `Scripts.rxdata`.

Static references include `Graphics.delta`, `average_frame_rate`, screenshots,
resize/fullscreen/scale, raw key queries, text input, clipboard, mouse, audio
position, `System.data_directory`, `HTTPLite::JSON` and HTTP utilities. References
alone do not prove reachability. Audio assets include OGG, WAV, MP3 and MIDI;
bitmap resolution searches PNG and GIF. Existing scripts and plugins also use
pixel reads/writes and custom `TilemapRenderer` behavior.

The README currently lists `$!` and non-UTF-8 encodings as missing. The game's
error handling uses `$!`, and startup actively uses Encoding. Treat those as
compatibility work, rather than assuming a modern parser makes the game work.

The bundled game is ignored by Git. Keep inventories derived from it reproducible
locally; do not make public CI or crate packages depend on distributing its assets.

## Library architecture

Proposed directories and crate names:

| Crate | Responsibility | Explicit exclusions |
| --- | --- | --- |
| `sabiruby` (existing) | Ruby VM, GC, general native API, necessary execution/continuation improvements | WGPU, windowing, audio devices, game rules |
| `sabiruby-compiler` (existing) | Source/eval compilation and diagnostics | RGSS state, device setup |
| `rgss/core` → `sabiruby-rgss-core` | RGSS objects and bindings, loader, compatibility profile, assets, timing/input semantics, audio mixer/decoders, render preparation | Window, surface acquisition, OS event loop, audio device |
| `rgss/wgpu` → `sabiruby-rgss-wgpu` | Reusable native WGPU implementation: textures, bitmap operations, text uploads, sprite/tile composition, effects | Window ownership, private GPU device, presentation loop |
| `rgss/player` → `sabiruby-rgss-player` | Window/events, GPU/surface setup, audio output, controller adapter, CLI and FPS display | Game logic, RGSS implementation, optimizations needed by embedders |

Dependency direction: player → core + WGPU adapter; WGPU adapter → core; core →
VM and optional compiler. VM never depends on RGSS. Use small typed interfaces
for assets, clock/input and rendering services. Shared internal implementation is
fine; do not add an abstraction or queue around every sprite property access.
Core can initially require `std`; preserving VM `no_std` is the requirement.

Expose an Anil profile and a stock RGSS1 profile. Install optional Ruby libraries
and CRuby/RGSS compatibility classes during runtime construction. Language fixes
consistent with the VM's compatibility policy can live in the VM; intentional
semantic differences need an explicit profile/design decision and separate tests.
Do not silently replace default mruby behavior globally.

### Host-facing contract

Design an API with the following operations; names here are illustrative, not
existing APIs or compilable Rust:

1. Construct runtime with game root/provider, profile, compiler/cache and services.
2. Construct renderer using host-supplied WGPU Device/Queue handles and target
   format/capabilities; do not request another adapter/device.
3. Supply timestamped input and monotonic time; resume until a semantic frame
   boundary, another host request, termination, error or execution budget limit.
4. Prepare pending resource updates, then encode rendering into a host-supplied
   `CommandEncoder` and `TextureView`, with explicit extent and load/store policy.
5. Host finishes/submits commands and presents if it owns a window surface.
6. Drain audio into host-provided interleaved PCM buffers at the negotiated sample
   rate/channel layout. Host chooses its output device or routes audio elsewhere.
7. Read lightweight statistics; explicitly reset or shut down the runtime.

Device/Queue ownership can use cloned handles or host-owned shared handles as
supported by the pinned WGPU release. The renderer must not hold borrowed host
resources in `Vm::host_state`: that slot requires `Any + Send + Sync + 'static`.
Keep frame-local encoder/view borrows outside the VM; native methods mutate
owned resource state or issue resumable service requests.

The embedder owns event pumping, surface acquisition/reconfiguration, submission,
presentation and pacing. Headless/offscreen operation must use the same core and
renderer. Document texture formats, alpha/color space, sample count, feature and
limit requirements, device loss and reset behavior. Use one public WGPU version
across the workspace; assess its Rust MSRV before changing the current 1.85 floor.

### The first technical gate: suspension

RGSS scripts own the scene loop. `Graphics.update` must yield control to the host
without discarding Ruby frames, running past that frame, unwinding `ensure`, or
becoming a Ruby exception that `rescue` catches. Instruction-budget stepping alone
does not provide this contract. `Graphics.wait`, transitions, fades and reset also
need defined resumable behavior.

Prototype an explicit host suspension outcome and a resumable native continuation
mechanism. On resume, complete the suspended native exactly once, including its
Ruby return value, before executing the following Ruby instruction. Keep budget
exhaustion distinguishable from frame completion. Carry roots and continuation
state through GC, errors, reset, nested eval and fibers.

Audit every actual path into the game's main loop: archive/script evaluation,
plugin eval, `require`/`load`, native callbacks and block iteration. Current
`__exec_file` calls `run_irep` synchronously, and some native-to-Ruby callbacks
cannot wait according to the README. A flag checked after one native returns is
insufficient when that native is still running the entire Ruby scene loop.
Convert the relevant entry points to resumable VM frames/continuations; establish
the supported callback contract before committing to the embedding API.

Reject a prototype that only works with rewritten top-level game loops. Do not
default to a VM worker thread plus blocking messages for every API call. If some
callback classes must remain synchronous, enumerate them and prove the supported
game paths never need suspension there. Execution budgets cannot preempt arbitrary
long-running synchronous native code; implement long operations in resumable chunks
where responsiveness requires it.

## Compatibility implementation

### Loading, Ruby services and persisted data

- Decode Marshal 4.8 and zlib script archives; preserve script IDs, names, order,
  `$RGSS_SCRIPTS`, filenames and source line diagnostics. Execute the selected
  entry point without double-loading extracted files or plugins.
- Implement Marshal load **and dump**, object/symbol links, cycles, binary strings,
  ivars, named classes, user hooks (`_load`/`_dump`, `marshal_load`/`marshal_dump`),
  and RGSS Table/Color/Tone payloads. Derive supported tags from maps, databases,
  plugin archives and saves. Loading maps requires `RPG::*` schemas before decode;
  loading saved game objects requires the game's classes to be defined first.
- Add `load_data`/`save_data`, IO/File/FileTest/Dir operations actually used, block
  and EOF behavior, glob/mtime/path helpers, errors, and scoped working-directory
  behavior. Avoid changing process cwd when hosting multiple runtimes.
- Use one asset provider for RGSS reads and Ruby file APIs. Handle separators,
  optional Windows case-insensitive lookup, Unicode filenames, extension probing,
  game/user directories and atomic saves. Detect ambiguous case-folded paths.
- Provide Zlib inflate/deflate, required Encoding/String behavior, top-level
  binding/eval semantics, exception globals/backtraces and required library APIs.
  Audit regexp, pack/unpack, Time and standard-library dependencies against actual
  scripts rather than loading the bundled CRuby library directory wholesale.
- Cache RITE by source hash, compiler/options/VM bytecode version and compatibility
  profile. Dynamic eval still needs the compiler; precompiling initial scripts
  does not eliminate it. Invalidate caches when plugins or bindings change.
- Test bundled-save import using a copy and save round trips back to mkxp-z.
  Deep-copy uses of Marshal must preserve semantics independently of disk saves.

### Native RGSS state and rendering

- Native Rect, Color, Tone and Table, with correct mutation/aliasing and binary
  formats; Bitmap and Font; Sprite, Viewport, Plane, Window, Tilemap; Graphics,
  Input and Audio modules; Ruby `RPG::*` classes required by the data.
- Store heavy resources in typed native arenas behind generation-checked handles.
  Define explicit `dispose`/`disposed?`, idempotent cleanup, Ruby-visible identity,
  GC ownership and GPU resource retirement. Root Ruby references held by native
  state; avoid cloning a scene into a fresh structure every frame.
- RGSS draw order, viewport clipping/effects, origin/zoom/rotation, opacity,
  blend modes, tone/color, bush depth, flash, planes, window skins/cursors and
  tile priorities/autotiles. Preserve ordering while batching compatible draws.
- Bitmap blits/stretch, clear/fill, pixel reads/writes, text metrics/rasterization,
  hue and other reached operations. Preserve immediate Ruby-visible results and
  self-blit semantics. Track CPU/GPU authority and dirty regions; CPU mirrors for
  edited bitmaps may avoid per-pixel GPU readback. Define explicit synchronization
  for screenshots and reads of GPU-generated content.
- Font selection/fallback using bundled fonts, Spanish glyphs, layout/outline/shadow
  and measured text behavior. Cache glyphs and text runs without changing metrics.
- PNG and reached GIF/animation support; atlas/standalone texture selection for
  large images, cache limits and hot asset reuse. Audit mkxp-z bitmap extensions.
- Graphics freeze/transition/snapshot and resize behavior, render-to-texture and
  presentation scaling. Readbacks remain explicit slow paths, not normal rendering.
- First support Anil's Ruby custom `TilemapRenderer` with correct primitives.
  Profile before substituting a native fast path; preserve Ruby reopen/alias hooks
  and retain a fallback when the optimized code's assumptions no longer hold.

### Sound, input and platform extensions

- Core owns mixing, decoding, channel state, BGM/BGS/ME/SE ordering, fades, loops,
  volume/pitch, playback position and ME interruption/resumption semantics.
  Stream music and cache short effects. Cover reached OGG/WAV/MP3 and MIDI using
  the supplied soundfont. Player only opens the device and feeds PCM buffers.
- Decode outside the realtime callback; callback must not invoke the VM, allocate,
  block on filesystem IO or lock a renderer. Define single-consumer mixer ownership
  and bounded command handoff from the VM thread; report underruns.
- Core owns RGSS press/trigger/repeat/release and dir4/dir8 semantics; platform
  adapter supplies keyboard/controller/mouse/text events. Preserve update-boundary
  latching, repeat cadence, raw key codes, focus loss and gamepad mappings.
- Implement reached System/Graphics/Input extensions through the same services.
  Return truthful runtime identity plus explicit compatibility capabilities; handle
  the game's mkxp-z version check through its profile instead of pretending to be
  that binary everywhere.
- Inventory optional HTTP/JSON, sockets/trading, clipboard, launcher/updater and
  MiniFFI calls. JSON is used by local options and may be necessary offline.
  Provide a documented disabled Discord capability when DLLs are unavailable;
  verify the plugin's guarded fallback. Required gameplay features cannot be
  silently stubbed. Separate optional online parity from the offline milestone.

## FPS and timing policy

Expose three separate controls: presentation/vsync, RGSS update pacing, and an
explicit game speed multiplier. These cannot be merged into one “FPS” setting.

Default mode preserves the profile's game timing and `Graphics.frame_rate` /
`frame_count` semantics. Stock XP timing and Anil's refresh-synchronized mkxp-z
configuration must be measured separately. Report measured presentation FPS and
completed RGSS updates/sec independently, alongside CPU/GPU time and frame percentiles.

Presentation-unlocked mode removes the display cap where the surface supports
it, while preserving game pacing. Re-presenting the last state does not magically
create smoother game animation. Full benchmark mode also removes update waits;
this may accelerate frame-count-based gameplay and must be labeled accordingly.
Do not claim arbitrary unlocked RGSS updates preserve every game's speed.

Give Anil's existing turbo ownership of its scaled timeline initially. Supply a
monotonic unscaled clock and avoid applying another multiplier behind it. Verify
`System.uptime`, `real_uptime`, `Graphics.delta`, waits, input repeat, battle effects
and audio at every turbo stage. Deterministic replays need an injected clock and
controlled RNG; timed native code must use the same service.

Player switches should cover game path/profile, FPS overlay, vsync, update cap,
uncapped benchmark mode, mute and trace/profile output. Keep shortcuts configurable
and avoid collisions with Anil's ALT/AUX1 turbo controls.

## Ordered delivery plan and acceptance gates

| Stage | Deliverable | Gate before moving on |
| --- | --- | --- |
| 0 — inventory/reference | Reproducible local archive + API audit, enabled plugin list, asset formats, startup trace; capture matching mkxp-z reference runs | Know authoritative script order, first startup blockers and representative scenes; store hashes/reference settings. |
| 1 — suspension spike | Tiny RGSS fixture and second host drive the same suspended VM | `Graphics.update` resumes correctly through real loader/eval paths, aliases, blocks, fibers, exceptions and GC; host stays responsive. Resolve callback limits here. |
| 2 — loader/data/Ruby | Core crate, profile, Marshal/zlib, IO/encoding and compiler cache | Decode real scripts/maps/plugin archives; compile all enabled sources; execute boot progressively with explicit missing-API reports. |
| 3 — WGPU vertical slice | Native bitmap/font/sprite/viewport plus minimal window/audio/input wiring | Actual boot reaches a rendered title frame with controls and required sound, using the same renderer offscreen. Pull prerequisites from stage 4 as observed. |
| 4 — playable Anil | Reached RGSS/extensions, custom tile rendering, saves, full audio/input | New game, walking/transfers, dialogue, menu, battle, party/storage and copied save load/save pass against reference. |
| 5 — embedding release | Second small host uses its own device, encoder, target, events and audio sink; two-instance lifecycle fixture | No private window/device, mandatory extra thread or presentation ownership; resets/disposal/multiple instances work without global state leakage. |
| 6 — optimization | Profile-guided native/VM/render changes, caches and batching | Visual/gameplay parity maintained; embedding overhead and total performance gates pass. |
| 7 — RGSS1 breadth | Stock XP fixtures and another legally available XP project | Publish an explicit supported API/behavior matrix and documented optional gaps. |

Stages are dependency milestones, not calendar promises. Full mkxp-z replacement
is a substantial compatibility/rendering project; estimate time after stage 0 and
the suspension spike expose the real blockers. Do not start with an aggressive VM
representation rewrite or a native rewrite of the game's battle logic.

### Verification and performance gates

- Preserve existing workspace tests, VM reference baselines and `tools/check_no_std.sh`.
  Add differential fixtures against stock RGSS1 behavior and Anil's pinned mkxp-z
  environment; use CRuby for the relevant Marshal/IO/library behavior. Keep each
  reference's scope explicit.
- Compile survey is syntax coverage only. Track boot execution, enabled plugin
  coverage and replayed gameplay separately; dynamically generated eval and event
  commands need runtime coverage. Missing required APIs fail with script/line info.
- Use deterministic input/time replays for title, crowded overworld, animated
  dialogue/menu, battle effects, darkness, transfer and turbo. Compare image output,
  draw order, state checkpoints and audio behavior. Font differences get explicit
  tolerances rather than hiding arbitrary rendering errors.
- Measure optimized builds on the same machine/resolution/scenes/settings with
  instrumentation settings matched. Record CPU frame p50/p95/p99, GPU time where
  supported, VM/GC time, allocations, draw calls, upload/readback bytes, memory,
  startup, shader warm-up and audio underruns. Separate cold and warm runs and
  bounded/update-unlocked/presentation-unlocked modes.
- Proposed embedding gate: at most 1% median CPU overhead compared with the direct
  player/library path, no statistically resolved p95 regression, no extra per-frame
  image copies/readbacks/submissions, and no additional steady-state allocations
  introduced by the wrapper. Report variance; tiny workloads need absolute timing.
  This is an acceptance target, not an existing result.
- Proposed game gate: maintain the selected game's update budget with headroom
  (16.67 ms at 60 updates/sec; measure CPU and GPU separately), no audio underruns
  during the representative run, and no material p95 regression versus mkxp-z.
  Record startup/memory tradeoffs. If VM time misses the target, optimize measured
  call/allocation/GC hotspots before considering specialization or JIT work.
- Normal render paths must have no full-frame CPU staging/readback. Ordered
  resource operations may require extra passes or submissions; count and justify
  them rather than promising every RGSS effect fits one draw/pass/submission.

Completion means a working player **and** an independently embeddable library,
verified gameplay/save behavior, a compatibility matrix and reproducible performance
evidence. A renderer demo or title screen alone is an intermediate milestone.

## External references checked for this design

- [mkxp-z upstream](https://github.com/mkxp-z/mkxp-z): reference player and source
  for targeted extension behavior; pin the game's referenced revision for tests.
- [Ruby Marshal](https://docs.ruby-lang.org/en/master/Marshal.html): format
  versioning and serialization hooks; game-specific payloads still need fixtures.
- [WGPU Device](https://docs.rs/wgpu/latest/wgpu/struct.Device.html),
  [TextureView](https://wgpu.rs/doc/wgpu/struct.TextureView.html) and
  [CommandEncoder](https://docs.rs/wgpu/latest/wgpu/struct.CommandEncoder.html):
  the host-provided GPU/target/encoding boundary. These are moving documentation;
  pin dependencies and MSRV during implementation.
- [WGPU PresentMode](https://docs.rs/wgpu/latest/wgpu/enum.PresentMode.html):
  display synchronization modes depend on surface support; uncapped presentation
  must report its actual selected mode/fallback.
