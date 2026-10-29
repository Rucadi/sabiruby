# SabiRuby repository instructions

## Git

Never commit, amend, squash, rebase, or otherwise rewrite history unless the user
explicitly asks for it in that turn. Leave changes staged or unstaged as they are
and report what was modified. A previous request to commit does not authorize
later commits.

## Toolchain

Use the repository's Nix shell for Rust and C tooling. The host may not expose
`cargo` directly:

```sh
nix-shell shell.nix
cargo check --workspace
cargo test -p sabiruby-rgss-core
```

Do not install a second Rust toolchain into the user environment. The shell
contains Cargo, rustc, rustfmt, Clippy, GCC, CMake and pkg-config. The
`sabiruby-compiler` crate builds vendored mruby C sources through the `cc` crate.

## Project structure

`sabiruby` is the reusable `no_std + alloc` mruby-compatible VM. Keep windowing,
WGPU, audio devices, OS input and game-specific code out of it. The RGSS runtime
lives under `rgss/core`; reusable desktop surface/audio adapters live under
`rgss/wgpu`; and the player under `rgss/player` is a thin host. Anil is a local
compatibility target and its assets must not become a dependency of published
crates.

## Validation

Run focused checks while iterating and then the workspace checks before handing
off:

```sh
cargo fmt -p sabiruby-rgss-core -p sabiruby-rgss-player -- --check
cargo check --workspace
cargo test -p sabiruby-rgss-core
```

RGSS work must not run `cargo fmt --all` or otherwise reformat unrelated
SabiRuby crates. Format only the RGSS packages and explicitly edited shared
files; preserve existing formatting everywhere else.

When a change affects VM semantics, also run the existing SabiRuby tests and
`tools/check_no_std.sh`. Use release builds for performance measurements.

## RGSS implementation rules

The embeddable core must accept host-owned rendering, timing, input and audio
services. It must not create a window or GPU device as a side effect of library
construction. Keep frame suspension/resumption explicit and resumable through
`Graphics.update`; do not emulate it with a worker thread or by swallowing Ruby
exceptions. Preserve the current VM compatibility policy and add differential
fixtures for RGSS behavior.

The player accepts the project path, an optional numeric script limit, and an
optional headless frame limit as explicit command-line arguments. Avoid
environment variables for runtime configuration. The player runtime entrypoint
is the Marshal/zlib archive named by `Scripts=` in `Game.ini`; source files
under `Data/Scripts` are diagnostics/tooling only and must never be loaded as a
second entrypoint.

The compiled main scripts are read through the VFS from the archive named by
`Game.ini`. Plugin scripts are supplied to the Ruby PluginManager through a
generic archive-shaped loader, so dependency and evaluation order remain
Ruby-owned; never hardcode a particular utility path.

RGSS file access and directory views must go through VirtualFileSystem; its
ordered `read_dir` result preserves MRI-style `Dir.entries`/`Dir.foreach`
behavior, while `Dir.glob` sorts its matches as MRI does. `OsVfs` is the
default desktop backend, uses ordinary OS operations, and resolves paths
case-insensitively by default because RPG Maker XP projects assume that
behavior. Embedders should use `RgssRuntime::open_with_vfs` to provide an
archive, memory, or sandboxed filesystem without changing Ruby bindings.
