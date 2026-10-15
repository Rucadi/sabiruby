#!/bin/bash
# Build the reference mruby image the comparison with mruby/edge compiles with (local only).
#   tools/edge-compare/build-image.sh [commit]
# Default commit: the one mruby/edge's tests/mruby_spec/upstream was copied from
# (its README, "The full source revision is ..."). Produces kishima/mruby:<first 8 of commit>.
# kishima/mruby:4.1.0-rc2 (tools/mrbtest.sh) is left as it is.
set -eu
REV="${1:-891d1d9dc90f1578eedd1003d645250d94eb07e9}"
cd "$(dirname "$0")"
docker build --platform linux/amd64 \
  --build-arg MRUBY_VER="$REV" \
  -f mruby-image.Dockerfile \
  -t "kishima/mruby:${REV:0:8}" .
docker run --rm "kishima/mruby:${REV:0:8}" /bin/sh -c 'mruby --version && git rev-parse HEAD'
