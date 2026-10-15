#!/bin/bash
# Run mruby/edge's compatibility report on this machine, as its README says
# (`python3 scripts/mruby_spec_report.py --format json`, default features), in a container
# that has libclang (mrubyedge-runner.Dockerfile). The checkout is not changed except for its
# target/ (the build and the report).
#   tools/edge-compare/run-mrubyedge.sh <mruby/edge checkout>
#   e.g. git -C ../../ref/mrubyedge worktree add --detach <scratch>/mrubyedge origin/master
# Writes docs/verification/edge-compare/mrubyedge.json; the logs stay in
# target/edge-compare/mrubyedge/ (paths in the JSON are relative to that directory).
set -eu
SRC=$(cd "$1" && pwd)
cd "$(dirname "$0")/../.."
HERE=$PWD
OUT=$HERE/target/edge-compare/mrubyedge
mkdir -p "$OUT"
rm -rf "$OUT"/mruby-spec-logs-*
GITDIR=$(cd "$SRC" && cd "$(git rev-parse --git-common-dir)" && pwd)
RUST_VER=$(rustc --version | awk '{print $2}')
docker build -q --build-arg RUST_VER="$RUST_VER" -f tools/edge-compare/mrubyedge-runner.Dockerfile \
  -t sabiruby/mrubyedge-runner:"$RUST_VER" tools/edge-compare
# the same absolute paths inside, so that a worktree's `.git` file still points at its
# repository (mounted read-only; GIT_OPTIONAL_LOCKS=0 keeps `git status` from writing)
docker run --rm --user "$(id -u):$(id -g)" \
  -e HOME=/tmp -e CARGO_HOME="$SRC/target/cargo-home" -e GIT_OPTIONAL_LOCKS=0 \
  -e GIT_CONFIG_COUNT=1 -e GIT_CONFIG_KEY_0=safe.directory -e GIT_CONFIG_VALUE_0='*' \
  -v "$SRC:$SRC" -v "$GITDIR:$GITDIR:ro" -v "$OUT:$OUT" -w "$SRC" \
  sabiruby/mrubyedge-runner:"$RUST_VER" \
  /bin/sh -c "rustc --version > '$OUT/rustc-version.txt'; python3 scripts/mruby_spec_report.py --format json --output '$OUT/mruby-spec-report.json'; echo \$? > '$OUT/exit-status.txt'"
echo "exit status of the report: $(cat "$OUT/exit-status.txt") (0: all passed, 1: some failed, 2: build or report error)"
[ "$(cat "$OUT/exit-status.txt")" = 2 ] && exit 1
mkdir -p docs/verification/edge-compare
cp "$OUT/mruby-spec-report.json" docs/verification/edge-compare/mrubyedge.json
git -C "$SRC" log -1 --format='mruby/edge %H %ci' > "$OUT/commit.txt"
cat "$OUT/commit.txt"
