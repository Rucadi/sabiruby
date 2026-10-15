#!/bin/bash
# SabiRuby under mruby/edge's compatibility driver, on the same 48 files of mruby's test/t
# (docs/plans/edge-compare-plan.md). Writes docs/verification/edge-compare/sabiruby.json in
# the shape of mruby/edge's `scripts/mruby_spec_report.py --format json`, plus, per file, the
# counts of mruby's own `assert.rb` (the `mrbtest` subcommand) as a reference column.
#   tools/edge_compare.sh                  # compile (Docker, reference mrbc) + run + JSON
#   tools/edge_compare.sh --timeout 60     # seconds per file (default: mruby/edge's, docs/numbers.md)
#   tools/edge_compare.sh --file array     # some files only (repeatable)
#   tools/edge_compare.sh --no-compile     # reuse target/edge-compare/mrb
# The test files and helpers.rb are tests/edge-spec (byte-for-byte copies; see its README).
# Logs and events go to target/edge-compare/logs, not to docs/.
set -eu
cd "$(dirname "$0")/.."
REV=891d1d9dc90f1578eedd1003d645250d94eb07e9
IMG=kishima/mruby:${REV:0:8}   # tools/edge-compare/build-image.sh
SPEC=tests/edge-spec
WORK=target/edge-compare
COMPILE=1
ARGS=()
for a in "$@"; do
  if [ "$a" = "--no-compile" ]; then COMPILE=0; else ARGS+=("$a"); fi
done
(cd $SPEC/upstream && sha256sum --quiet -c SHA256SUMS)
if [ $COMPILE = 1 ]; then
  rm -rf $WORK/src $WORK/mrb
  mkdir -p $WORK/src $WORK/mrb $WORK/ref
  # mruby/edge compiles `"#{source}\n$spec_completed = true\n"` (mod.rs `execute`): the same
  # bytes are written here, so that the last line tells a finished file from a VM that returned
  python3 - "$SPEC/upstream/t" "$WORK/src" <<'EOF'
import pathlib, sys
src, dst = map(pathlib.Path, sys.argv[1:])
for f in sorted(src.glob('*.rb')):
    (dst / f.name).write_bytes(f.read_bytes() + b"\n$spec_completed = true\n")
EOF
  cp $SPEC/helpers.rb $WORK/src/helpers.rb
  # -g as tools/mrbtest.sh: the reference test driver compiles from source, so line numbers
  # and local variable names are there (`local_variables`, `Proc#parameters`). A file mrbc
  # cannot compile leaves no .mrb and its message in <name>.mrbc.log (status compile_error).
  # ref/: the unmarked files and mruby's assert.rb, for the reference column.
  docker run --rm -v "$PWD/$WORK:/w" -v "$PWD/$SPEC/upstream:/u:ro" $IMG /bin/sh -c '
    cd /w/src && for rb in *.rb; do b=${rb%.rb}
      mrbc -g -o /w/mrb/$b.mrb $rb > /w/mrb/$b.mrbc.log 2>&1 || { echo "compile failed: $b"; rm -f /w/mrb/$b.mrb; }; done
    cd /u && mrbc -g -o /w/ref/assert.mrb assert.rb
    cd /u/t && for rb in *.rb; do b=${rb%.rb}
      mrbc -g -o /w/ref/$b.mrb $rb || echo "compile failed (reference column): $b"; done
    mrbc --version > /w/mrbc-version.txt 2>&1 || true'
fi
cargo build --release -q -p sabiruby-cli
python3 tools/edge-compare/run.py --bin target/release/sabiruby --work $WORK --spec $SPEC \
  --upstream-revision $REV --compiler "reference mrbc $REV (Docker $IMG), -g" \
  --output docs/verification/edge-compare/sabiruby.json "${ARGS[@]}"
# the same files with the ports of the reference driver's C fixtures defined as well, which
# mruby/edge does not have (a column of its own on the page, not under mruby/edge's conditions)
python3 tools/edge-compare/run.py --bin target/release/sabiruby --work $WORK --spec $SPEC \
  --upstream-revision $REV --compiler "reference mrbc $REV (Docker $IMG), -g" --with-fixtures \
  --output docs/verification/edge-compare/sabiruby-fixtures.json "${ARGS[@]}"
# the page, once mruby/edge's JSON is there too (tools/edge-compare/run-mrubyedge.sh)
if [ -f docs/verification/edge-compare/mrubyedge.json ]; then python3 tools/edge-compare/report.py; fi
