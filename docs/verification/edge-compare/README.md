# SabiRuby and mruby/edge on mruby's test/t

`index.html` puts SabiRuby next to [mruby/edge](https://github.com/mrubyedge/mrubyedge) on the
same 48 files of mruby's `test/t` (commit `891d1d9d`), counted by the same driver (mruby/edge's
`helpers.rb`, unmodified), one process and one fresh VM per file, 30 s per file. It is
self-contained (no external resources); open it in a browser.

| file | what | made by |
|---|---|---|
| `sabiruby.json` | SabiRuby's run, in the shape of mruby/edge's `--format json`, plus per file the counts under mruby's own `assert.rb` (`reference`) | `tools/edge_compare.sh` |
| `mrubyedge.json` | mruby/edge's own report (`scripts/mruby_spec_report.py --format json`), run on the same PC on the same day | `tools/edge-compare/run-mrubyedge.sh <checkout>` |
| `index.html` | the comparison page | `tools/edge-compare/report.py` (also called by `tools/edge_compare.sh`) |

The logs and event files the JSON names are under `target/edge-compare/` and are not kept here.
The hand-written reasons in the per-file table are `tests/edge-spec/notes.tsv`. Images: the
reference `mrbc` is `kishima/mruby:891d1d9d` (`tools/edge-compare/build-image.sh`), mruby/edge
builds in `mrubyedge-runner.Dockerfile` (its compiler crate needs libclang). How it was set up
and what was decided: `docs/worklog/2026-10-15-edge-compare.md` (Japanese).
