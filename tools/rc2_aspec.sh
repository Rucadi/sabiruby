#!/bin/bash
# The argument spec (MRB_ARGS_*) of every method of the reference mruby 4.1.0-rc2, and how
# SabiRuby's natives compare with it.
#   tools/rc2_aspec.sh            # collect: write tools/rc2-aspec/rc2-aspec.tsv (Docker)
#   tools/rc2_aspec.sh --compare  # compare: Method#arity of every method both have, and the
#                                 # error of a call with one argument too many or too few
#
# Collect: the image's source tree is copied inside a throwaway container, built with the
# default gembox plus tools/rc2-aspec/gem (Kernel#__aspec), and tools/rc2-aspec/dump.rb asks the
# VM for the flags of each method. The values are what the reference's VM checks the count
# against (src/vm.c check_argument_count) and reads arity from (src/proc.c mrb_proc_arity),
# not a reading of its sources; the method list is the same as the image's own `mruby`. The
# "@" lines (which owner a class reaches for each name, on the reference) go to
# target/rc2-aspec/owners.tsv: they are what matched a SabiRuby native defined higher or lower
# in the chain with the reference's (docs/worklog/2026-10-17-rc2-leftovers10.md).
#
# Compare: tools/rc2-aspec/arity.rb and argcount.rb run on both; the counts are printed and
# the differing lines are left in target/rc2-aspec/.
set -eu
cd "$(dirname "$0")/.."
IMG=kishima/mruby:4.1.0-rc2
W=target/rc2-aspec
mkdir -p $W
if [ "${1:-}" = "--compare" ]; then
  cargo build --release -q -p sabiruby-cli
  cp tools/rc2-aspec/arity.rb tools/rc2-aspec/argcount.rb $W/
  for s in arity argcount; do
    ./target/release/sabiruby $W/$s.rb 2>/dev/null | sed -E 's/0x[0-9a-f]+/0x/g' | sort > $W/$s.sabiruby
    docker run --rm -v "$PWD/$W:/w" $IMG mruby /w/$s.rb 2>/dev/null | sed -E 's/0x[0-9a-f]+/0x/g' | sort > $W/$s.reference
  done
  # arity: "Owner#name arity"; compare the names both have
  awk 'NR==FNR { k=$0; sub(/ [^ ]*$/, "", k); a[k]=$NF; next }
       { k=$0; sub(/ [^ ]*$/, "", k); if (k in a) { n++; if (a[k] != $NF) { d++; print k, "reference", $NF, "sabiruby", a[k] > "'$W'/arity.diff" } } }
       END { printf "arity: %d methods both have, %d differ\n", n, d }' $W/arity.sabiruby $W/arity.reference
  # argcount: "Owner#name n => result" for the methods both have an arity for
  awk 'FILENAME ~ /arity/ { k=$0; sub(/ [^ ]*$/, "", k); c[k]++; next }
       FILENAME ~ /sabiruby$/ { split($0, p, " => "); s[p[1]]=substr($0, length(p[1]) + 5); next }
       { split($0, p, " => "); split(p[1], q, " "); if (c[q[1]] == 2 && (p[1] in s)) { n++; r=substr($0, length(p[1]) + 5); if (r == s[p[1]]) same++; else print p[1], "reference:", r, "sabiruby:", s[p[1]] > "'$W'/argcount.diff" } }
       END { printf "wrong argument count: %d calls on both, %d answer the same\n", n, same }' \
    $W/arity.sabiruby $W/arity.reference $W/argcount.sabiruby $W/argcount.reference
  exit 0
fi
docker run --rm -v "$PWD/tools/rc2-aspec:/aspec:ro" $IMG sh -c '
  set -e
  cp -r /usr/src/mruby /tmp/m && cp /aspec/build_config.rb /tmp/build_config.rb && cd /tmp/m
  MRUBY_CONFIG=/tmp/build_config.rb rake -j"$(nproc)" >/tmp/build.log 2>&1 || { tail -40 /tmp/build.log >&2; exit 1; }
  build/host/bin/mruby /aspec/dump.rb
' > $W/dump.tsv
grep -v '^@' $W/dump.tsv > tools/rc2-aspec/rc2-aspec.tsv
grep '^@' $W/dump.tsv > $W/owners.tsv
wc -l tools/rc2-aspec/rc2-aspec.tsv $W/owners.tsv
