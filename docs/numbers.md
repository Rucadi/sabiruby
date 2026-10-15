# Numbers

The defaults and limits the VM carries, each with where it is changed and where it came from
(`/home/kishima/book/CLAUDE.md`: a number is a default a user can change, and it says where it
came from). This list starts with 0.7.0; the older numbers are to be added (`docs/plans/`, R10).

| number | default | changed with | where it came from |
|---|---|---|---|
| `TASK_INDEX_FROM` — waiting tasks from which the scheduler indexes them (deadlines, awaited objects) instead of walking the waiting queue | 30 | `Vm::task_set_index_from` (`TaskState::index_from`) | Measured 2026-09-27 (`worklog/2026-09-27-release-0.7-bench.md`): two tasks playing queue ping-pong 200,000 times beside N tasks waiting on queues nobody pushes to, indexed against walking, 2 rounds of 5, best. Walking was 2.0% faster at N = 10, the same at 30 (+0.4%), 9.6% slower at 100, 52% at 300, 160% at 1000. |
| time limit per test file of `tools/edge_compare.sh` (the comparison with mruby/edge) — not the VM's, the tool's | 30 s | `tools/edge_compare.sh --timeout SECONDS` (`tools/edge-compare/run.py`) | Taken from mruby/edge so that both are cut off at the same point: `--timeout` of its `scripts/mruby_spec_report.py` defaults to 30 and its `tests/mruby_spec/mod.rs` kills the child after 30 seconds (`d15d933`, 2026-10-14). Not measured here; every file takes well under a second on SabiRuby (`docs/verification/edge-compare/`). |
