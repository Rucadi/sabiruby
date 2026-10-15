# mruby/edge's compatibility suite, for SabiRuby

What `tools/edge_compare.sh` runs (`docs/plans/edge-compare-plan.md`): the same 48 files of
mruby's `test/t`, counted by the same Ruby, as mruby/edge's `tests/mruby_spec/`.

- `upstream/t/*.rb`, `upstream/assert.rb`, `upstream/LICENSE`: mruby
  [`891d1d9dc90f1578eedd1003d645250d94eb07e9`](https://github.com/mruby/mruby/tree/891d1d9dc90f1578eedd1003d645250d94eb07e9/test)
  (2026-10-12), taken with `git archive` from that commit, byte for byte. MIT,
  Copyright (c) 2010- mruby developers (`upstream/LICENSE`).
- `upstream/SHA256SUMS`: mruby/edge's own list (`mrubyedge/tests/mruby_spec/upstream/SHA256SUMS`
  at `d15d933`), copied unchanged. `tools/edge_compare.sh` checks the files against it before
  every run, so that these are the files mruby/edge runs.
- `helpers.rb`: mruby/edge's assertion driver (`mrubyedge/tests/mruby_spec/helpers.rb` at
  [`d15d933`](https://github.com/mrubyedge/mrubyedge/tree/d15d933/mrubyedge/tests/mruby_spec),
  2026-10-14, last changed in `f0d64ef`), **not modified**. BSD-3-Clause, Copyright (c) 2024 -
  Uchio Kondo (`LICENSE-mrubyedge`).

The natives `helpers.rb` calls (`t_print`, `_str_match?`, `_spec_event`) are in the command,
`cli/src/edge_spec.rs` (`sabiruby edge-spec`), not in the VM crate. `upstream/assert.rb` is
mruby's own driver, run for the reference column only.

To verify: `cd upstream && sha256sum -c SHA256SUMS`.
