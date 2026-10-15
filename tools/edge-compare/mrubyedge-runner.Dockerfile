# What mruby/edge's report (`scripts/mruby_spec_report.py`) needs to build and run, for
# tools/edge-compare/run-mrubyedge.sh: its compiler crate (mruby-compiler2-sys) runs bindgen,
# which needs libclang, and this machine has none (worklog/2026-09-16-rbs-study.md). Local only.
# The Rust version is the host's (`rustc --version` on 2026-10-15), so that the build is the
# one a `cargo` on this machine would make.
ARG RUST_VER=1.97.0
FROM rust:${RUST_VER}-bookworm
RUN apt-get update && apt-get install -y --no-install-recommends libclang-dev clang python3 git \
 && rm -rf /var/lib/apt/lists/*
