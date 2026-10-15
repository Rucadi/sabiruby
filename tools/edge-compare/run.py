#!/usr/bin/env python3
"""Run mruby's test/t on SabiRuby under mruby/edge's driver and write its JSON report.

Called by tools/edge_compare.sh, which compiles the files first. One child process
(`sabiruby edge-spec`) per file with a fresh VM and a time limit, events appended to a file,
the file's status read off the events, the exit status and the log: the same rules as
mruby/edge's scripts/mruby_spec_report.py, whose `summarize`, `read_events`, `STATUSES` and
`NOTE` below are copied unchanged (BSD-3-Clause, Copyright (c) 2024 - Uchio Kondo; the licence
is tests/edge-spec/LICENSE-mrubyedge). Python standard library only.

Besides mruby/edge's fields, every file carries `reference`: the counts of the same file under
mruby's own test/assert.rb (`sabiruby mrbtest`, which also installs the C fixtures of the
reference's test driver), for a column the HTML shows as a reference.
"""
import argparse
from collections import Counter
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import platform
import re
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[2]

# ---- from mrubyedge scripts/mruby_spec_report.py (d15d933), unchanged ----
STATUSES = ("pass", "fail", "error", "warn", "skip", "stress", "interrupted")
NOTE = ("Counts include only reached assert blocks (including nested blocks), not individual "
        "assert_equal calls. Interrupted files may have further unevaluated tests; their count "
        "is unknown. Conditional tests not entered by this build are not counted as skips.")


def read_events(path):
    events = []
    if path.exists():
        for line in path.read_text(errors="replace").splitlines():
            try:
                events.append(json.loads(line))
            except json.JSONDecodeError:
                # A killed process can leave its last record partially written.
                break
    return events


def summarize(events, returncode, timed_out, log):
    assertions, by_id, stack = [], {}, []
    phase = None
    completed = False
    for event in events:
        kind = event[0]
        if kind == "phase":
            phase = event[1]
            completed |= phase == "completed"
        elif kind == "start":
            item = {"id": event[1], "name": event[2], "parent_id": stack[-1] if stack else None,
                    "status": "interrupted", "details": []}
            assertions.append(item)
            by_id[item["id"]] = item
            stack.append(item["id"])
        elif kind == "check_failure":
            if stack:
                detail = " ".join(str(v) for v in event[2:] if v is not None)
                by_id[stack[-1]]["details"].append(detail or "assertion failed")
        elif kind in STATUSES and event[1] in by_id:
            item = by_id[event[1]]
            item["status"] = kind
            if len(event) > 2 and event[2] is not None:
                item["details"].append(str(event[2]))
            if event[1] in stack:
                stack.remove(event[1])
    if timed_out:
        status = "timeout"
    elif returncode is not None and returncode < 0:
        status = "aborted"
    elif returncode == 0 and completed and not stack:
        status = "passed"
    elif completed and any(a["status"] in ("fail", "error") for a in assertions):
        status = "failed"
    elif phase == "compile":
        status = "compile_error"
    elif phase == "returned":
        status = "premature_return"
    elif phase == "runtime_error":
        status = "runtime_error"
    elif "panicked at" in log:
        status = "panic"
    else:
        status = "incomplete"
    counts = Counter(a["status"] for a in assertions)
    return {"status": status, "completed": completed, "returncode": returncode,
            "counts": {s: counts[s] for s in STATUSES}, "assertions": assertions}
# ---- end of the copy ----


REF_FIELDS = ("total", "ok", "ko", "crash", "warn", "skip")


def reference_counts(binary, work, name, timeout):
    """One file under mruby's assert.rb: the row `sabiruby mrbtest` prints for it."""
    assert_mrb, mrb = work / "ref/assert.mrb", work / f"ref/{name}.mrb"
    if not mrb.exists():
        return {"status": "compile_error"}
    try:
        out = subprocess.run([binary, "mrbtest", str(assert_mrb), str(mrb)], cwd=ROOT,
                             stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
                             errors="replace", timeout=timeout).stdout
    except subprocess.TimeoutExpired:
        return {"status": "timeout"}
    for line in out.splitlines():
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if cells and cells[0] == name and len(cells) >= 8:
            row = {k: int(v) for k, v in zip(REF_FIELDS, cells[1:7])}
            row["note"] = cells[7]
            row["status"] = "ran"
            return row
    return {"status": "no_report", "output_tail": out[-500:]}


def git(*cmd):
    return subprocess.check_output(["git", *cmd], cwd=ROOT, text=True).strip()


def cpu_model():
    try:
        for line in Path("/proc/cpuinfo").read_text().splitlines():
            if line.startswith("model name"):
                return line.split(":", 1)[1].strip()
    except OSError:
        pass
    return platform.processor()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--bin", required=True)
    p.add_argument("--work", type=Path, required=True)
    p.add_argument("--spec", type=Path, required=True)
    p.add_argument("--upstream-revision", required=True)
    p.add_argument("--compiler", required=True)
    p.add_argument("--output", type=Path, required=True)
    p.add_argument("--file", action="append", help="this upstream file only (repeatable, .rb optional)")
    # mruby/edge's default (`--timeout`, and the 30 s of mod.rs `run`); docs/numbers.md
    p.add_argument("--timeout", type=float, default=30, help="seconds per file (default: 30)")
    p.add_argument("--with-fixtures", action="store_true",
                   help="run with the ports of the reference driver's C fixtures (`sabiruby edge-spec --with-fixtures`)")
    args = p.parse_args()
    if not 0 < args.timeout < float("inf"):
        p.error("--timeout must be finite and positive")
    binary = str(Path(args.bin).resolve())
    work = args.work.resolve()
    files = sorted(f.name for f in (args.spec / "upstream/t").glob("*.rb"))
    if args.file:
        selected = {f if f.endswith(".rb") else f + ".rb" for f in args.file}
        unknown = selected - set(files)
        if unknown:
            p.error("unknown files: " + ", ".join(sorted(unknown)))
        files = [f for f in files if f in selected]
    output = args.output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    logs = work / ("logs-fixtures" if args.with_fixtures else "logs")
    logs.mkdir(parents=True, exist_ok=True)
    version = subprocess.run([binary, "--version"], stdout=subprocess.PIPE, text=True).stdout.strip()
    mrbc = (work / "mrbc-version.txt").read_text().strip() if (work / "mrbc-version.txt").exists() else ""
    report = {"schema_version": 1, "metadata": {
        "commit": git("rev-parse", "HEAD"), "dirty": bool(git("status", "--porcelain", "--untracked-files=no", "--", ".", ":!docs/verification/edge-compare")),
        "generated_at": datetime.now(timezone.utc).isoformat(), "features": "",
        "default_features": True, "upstream_revision": args.upstream_revision,
        "timeout_seconds": args.timeout, "note": NOTE,
        # not in mruby/edge's metadata
        "vm": "SabiRuby", "vm_version": version, "cargo_features": "utf8, regexp (the default)",
        "compiler": args.compiler, "compiler_version": mrbc,
        "machine": f"{cpu_model()}, {platform.system()} {platform.release()}",
        "c_fixtures": args.with_fixtures,
    }, "files": []}
    for filename in files:
        name = filename[:-3]
        events = logs / (filename + ".jsonl")
        log = logs / (filename + ".log")
        events.unlink(missing_ok=True)
        events.touch()
        mrb = work / f"mrb/{name}.mrb"
        start = time.monotonic()
        timed_out = False
        with log.open("w") as stream:
            # what mrbc said (the compile phase is its, before the VM is involved)
            mrbc_log = work / f"mrb/{name}.mrbc.log"
            if mrbc_log.exists() and mrbc_log.read_text().strip():
                stream.write("mrbc: " + mrbc_log.read_text())
                stream.flush()
            try:
                result = subprocess.run([binary, "edge-spec", "--events", str(events), "--label", filename,
                                         *(["--with-fixtures"] if args.with_fixtures else []),
                                         str(work / "mrb/helpers.mrb"), str(mrb)],
                                        cwd=ROOT, stdout=stream, stderr=subprocess.STDOUT, timeout=args.timeout)
                returncode = result.returncode
            except subprocess.TimeoutExpired:
                timed_out, returncode = True, None
        item = summarize(read_events(events), returncode, timed_out, log.read_text(errors="replace"))
        item.update(file=filename, elapsed_seconds=round(time.monotonic() - start, 3),
                    log=os.path.relpath(log, ROOT), events=os.path.relpath(events, ROOT),
                    source=os.path.relpath(args.spec.resolve() / "upstream/t" / filename, ROOT))
        item["reference"] = reference_counts(binary, work, name, args.timeout)
        report["files"].append(item)
        print(f"{filename}: {item['status']}", file=sys.stderr)
    report["file_counts"] = dict(Counter(f["status"] for f in report["files"]))
    report["assert_counts"] = {s: sum(f["counts"][s] for f in report["files"]) for s in STATUSES}
    ref = [f["reference"] for f in report["files"] if f["reference"].get("status") == "ran"]
    report["reference_counts"] = {k: sum(r[k] for r in ref) for k in REF_FIELDS}
    report["reference_counts"]["files_ran"] = len(ref)
    output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"Report: {output}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
