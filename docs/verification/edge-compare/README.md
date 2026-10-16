# SabiRuby on mruby's test/t, under mruby/edge's driver

`index.html` is SabiRuby's mruby compatibility report: the 48 files of mruby's `test/t` (commit
`891d1d9d`, the files of [mruby/edge](https://github.com/mrubyedge/mrubyedge)'s `SHA256SUMS`),
counted by mruby/edge's driver (`helpers.rb`, unmodified), one process and one fresh VM per file,
30 s per file, in two columns: SabiRuby, and SabiRuby with its ports of the C test fixtures. It is
the page published at https://sabiruby.github.io/sabiruby/mruby-compatibility/ (with the two SabiRuby
JSON files) and does not show mruby/edge's results; it links to mruby/edge's own report
(https://mrubyedge.github.io/mrubyedge/mruby-spec/) instead (the author's decision, 2026-10-16).
`compare.html` is the development record that puts SabiRuby, mruby/edge and the fixtures column
side by side; it is not published. Both are self-contained (no external resources); open them in
a browser. SabiRuby implements the behaviour of
mruby 4.1.0-rc2 (the stable branch); tests in `891d1d9d` that check behaviour changed on master after
the stable branch point (`4e2bc5c2b`) fail by design (`docs/worklog/2026-10-16-follow-master.md`).

| file | what | made by |
|---|---|---|
| `sabiruby.json` | SabiRuby's run, in the shape of mruby/edge's `--format json`, plus per file the counts under mruby's own `assert.rb` (`reference`) | `tools/edge_compare.sh` |
| `sabiruby-fixtures.json` | the same run with SabiRuby's ports of the C fixtures of mruby's test driver defined as well (`sabiruby edge-spec --with-fixtures`, the ones `sabiruby mrbtest` defines); mruby/edge has none, so this column is not under its conditions | `tools/edge_compare.sh` |
| `mrubyedge.json` | mruby/edge's own report (`scripts/mruby_spec_report.py --format json`), run on the same PC on the same day; read only by `compare.html`, not published | `tools/edge-compare/run-mrubyedge.sh <checkout>` |
| `index.html` | SabiRuby's compatibility report (published): `sabiruby.json` and `sabiruby-fixtures.json` | `tools/edge-compare/report.py` (also called by `tools/edge_compare.sh`) |
| `compare.html` | the side-by-side page with the mruby/edge column (development record, not published); written when `mrubyedge.json` is there | `tools/edge-compare/report.py` |

The logs and event files the JSON names are under `target/edge-compare/` and are not kept here.
The hand-written reasons in the per-file table are `tests/edge-spec/notes.tsv`. Images: the
reference `mrbc` is `kishima/mruby:891d1d9d` (`tools/edge-compare/build-image.sh`), mruby/edge
builds in `mrubyedge-runner.Dockerfile` (its compiler crate needs libclang). How it was set up
and what was decided: `docs/worklog/2026-10-15-edge-compare.md` (Japanese).
