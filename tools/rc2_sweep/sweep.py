#!/usr/bin/env python3
"""The method sweep against mruby 4.1.0-rc2 (docs/plans/rc2-sweep-plan.md X1).

  tools/rc2_sweep/sweep.py lists     # both VMs' methods and the receivers' callable methods
  tools/rc2_sweep/sweep.py cases     # the case table and its size (no VM is run)
  tools/rc2_sweep/sweep.py run       # every case on both VMs (Docker for the reference)
  tools/rc2_sweep/sweep.py compare   # the differences, classified, into docs/verification/rc2-sweep/

The reference is the image kishima/mruby:4.1.0-rc2-utf8 (SabiRuby's default build has strings
as characters, which is that image's MRB_UTF8_STRING; tools/custom.sh and fixtures.sh take the
same image for such cases). One container runs for the whole sweep and each chunk is a
`docker exec` in it; the work directory is mounted at the same path inside, so the file names a
backtrace or `caller` prints are the same on both sides. The work is in target/rc2-sweep/.
Python standard library only.
"""
import argparse
import json
import os
import queue
import re
import resource
import subprocess
import sys
import threading
import time
from collections import Counter
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import cases as C  # noqa: E402

ROOT = Path(__file__).resolve().parents[2]
HERE = ROOT / "tools/rc2_sweep"
WORK = ROOT / "target/rc2-sweep"
IMG = "kishima/mruby:4.1.0-rc2-utf8"
SABI = ROOT / "target/release/sabiruby"
CONTAINER = "rc2-sweep"
SIDES = ("ref", "sabi")


def sh(cmd, **kw):
    return subprocess.run(cmd, check=True, **kw)


def start_container():
    subprocess.run(["docker", "rm", "-f", CONTAINER], capture_output=True)
    sh(["docker", "run", "-d", "--name", CONTAINER, "-e", "TZ=UTC", "-v", f"{WORK}:{WORK}",
        IMG, "sleep", "infinity"], capture_output=True)


def stop_container():
    subprocess.run(["docker", "rm", "-f", CONTAINER], capture_output=True)


def run_once(side, rb, args=(), timeout=None):
    if side == "ref":
        cmd = ["docker", "exec", CONTAINER, "mruby", str(rb), *args]
    else:
        cmd = [str(SABI), str(rb), *args]
    return subprocess.run(cmd, capture_output=True, timeout=timeout, env={**os.environ, "TZ": "UTC"})


def cmd_lists(a):
    WORK.mkdir(parents=True, exist_ok=True)
    prelude = (HERE / "prelude.rb").read_text()
    (WORK / "methods.rb").write_text((HERE / "methods.rb").read_text())
    (WORK / "receivers.rb").write_text(prelude + (HERE / "receivers.rb").read_text())
    start_container()
    try:
        for side in SIDES:
            for name in ("methods", "receivers"):
                p = run_once(side, WORK / f"{name}.rb")
                if p.returncode != 0:
                    sys.exit(f"{side} {name}: exit {p.returncode}\n{p.stderr.decode()}")
                (WORK / f"{name}.{side}.tsv").write_bytes(p.stdout)
    finally:
        stop_container()
    print("wrote", *(f"{n}.{s}.tsv" for n in ("methods", "receivers") for s in SIDES))


def load_cases():
    return C.build(WORK / "receivers.ref.tsv", WORK / "receivers.sabi.tsv")


def chunk_name(rk):
    return re.sub(r"[^A-Za-z0-9]+", "_", rk).strip("_")


def case_line(c):
    rk, name, args, kw, blk = c
    return f"{rk}\t{name}\t{','.join(map(str, args))}\t{int(kw)}\t{int(blk)}"


# A pool string's length is written in 16 bits (mruby-compiler codegen.c new_lit_str refuses
# more), so the case table goes in as pieces no longer than that, joined at run time.
STR_MAX = 0xffff


def rb_string(s):
    """A single-quoted Ruby literal (an Array literal of one element per case is more than the
    code generator's jump offsets reach)."""
    return "'" + s.replace("\\", "\\\\").replace("'", "\\'") + "'"


def cmd_cases(a):
    cases, skipped, one_side = load_cases()
    by = {}
    for c in cases:
        by.setdefault(c[0], []).append(c)
    cdir = WORK / "chunks"
    cdir.mkdir(parents=True, exist_ok=True)
    prelude = (HERE / "prelude.rb").read_text()
    runner = (HERE / "runner.rb").read_text()
    index = []
    for rk, cs in by.items():
        n = chunk_name(rk)
        pieces, cur = [], []
        for c in cs:
            line = case_line(c)
            if cur and len("\n".join(cur + [line]).encode()) > STR_MAX:
                pieces.append("\n".join(cur))
                cur = []
            cur.append(line)
        pieces.append("\n".join(cur))
        body = "SW_CASES_SRC = [\n" + ",\n".join(map(rb_string, pieces)) + "\n].join(\"\\n\")\n"
        (cdir / f"{n}.rb").write_text(prelude + body + runner)
        sh([str(SABI), "compile", "-g", "-o", str(cdir / f"{n}.mrb"), str(cdir / f"{n}.rb")])
        index.append((n, rk, len(cs)))
    (WORK / "chunks.json").write_text(json.dumps(index))
    with open(WORK / "skipped.tsv", "w") as f:
        for r in skipped:
            f.write("\t".join(r) + "\n")
    with open(WORK / "one_side.tsv", "w") as f:
        for r in one_side:
            f.write("\t".join(r) + "\n")
    pairs = {(c[0], c[1]) for c in cases}
    print(f"{len(cases)} calls, {len(pairs)} receiver/method pairs, {len(by)} receivers, "
          f"{len(skipped)} pairs left out, {len(one_side)} receiver/method pairs on one side only")
    print("by argument count:", dict(sorted(Counter(len(c[2]) for c in cases).items())))


class Chunk:
    """One chunk on one side: started again after a case that hung or crashed."""

    def __init__(self, side, name, total, limit, mem):
        self.side, self.name, self.total, self.limit, self.mem = side, name, total, limit, mem
        self.rb = WORK / "chunks" / f"{name}.rb"
        self.mrb = WORK / "chunks" / f"{name}.mrb"
        self.out = WORK / "raw" / side / f"{name}.tsv"

    def spawn(self, start):
        ul = f"ulimit -v {self.mem // 1024}; "
        if self.side == "ref":
            cmd = ["docker", "exec", "-e", "TZ=UTC", CONTAINER, "sh", "-c",
                   ul + f"echo $$; exec mruby {self.rb} {start}"]
        else:
            st = WORK / "start" / f"{self.name}.{start}.mrb"
            st.parent.mkdir(exist_ok=True)
            src = st.with_suffix(".rb")
            src.write_text(f"SW_START = {start}\n")
            sh([str(SABI), "compile", "-g", "-o", str(st), str(src)], capture_output=True)
            cmd = ["sh", "-c", ul + f"echo $$; exec {SABI} edge-spec {st} {self.mrb}"]
        p = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                             env={**os.environ, "TZ": "UTC"})
        pid = int(p.stdout.readline())
        return p, pid

    def kill(self, p, pid):
        if self.side == "ref":
            subprocess.run(["docker", "exec", CONTAINER, "kill", "-9", str(pid)], capture_output=True)
        else:
            try:
                os.kill(pid, 9)
            except ProcessLookupError:
                pass
        p.kill()
        p.wait()

    def run(self):
        self.out.parent.mkdir(parents=True, exist_ok=True)
        f = open(self.out, "wb")
        start, restarts = 0, 0
        self.slowest = (0.0, None)   # the longest wait for one case's line, and its index
        while start < self.total:
            p, pid = self.spawn(start)
            q = queue.Queue()
            err = []

            def reader():
                for line in p.stdout:
                    q.put(line)
                q.put(None)

            def ereader():
                for line in p.stderr:
                    if len(err) < 20:
                        err.append(line)

            threading.Thread(target=reader, daemon=True).start()
            et = threading.Thread(target=ereader, daemon=True)
            et.start()
            nxt, done, ready = start, False, False
            last = time.time()
            while True:
                try:
                    line = q.get(timeout=self.limit)
                    now = time.time()
                    if now - last > self.slowest[0] and nxt > start:
                        self.slowest = (now - last, nxt)
                    last = now
                except queue.Empty:
                    self.kill(p, pid)
                    f.write(f"\x01{nxt}\ttimeout\t\t\t\t\n".encode())
                    break
                if line is None:
                    rc = p.wait()
                    et.join(1)
                    if not ready:
                        e = b"".join(err).decode(errors="replace").strip()
                        raise RuntimeError(f"{self.side} {self.name}: did not start: {e}")
                    if nxt < self.total:
                        e = [x for x in b"".join(err).decode(errors="replace").strip().splitlines()
                             if not x.startswith("note: run with")]
                        tail = e[-1] if e else ""
                        f.write(f"\x01{nxt}\tcrash\texit {rc}\t{tail.replace(chr(9), ' ')}\t\t\n".encode())
                    else:
                        done = True
                    break
                if not line.startswith(b"\x01"):
                    continue
                if line.startswith(b"\x01ready"):
                    ready = True
                    continue
                if line.startswith(b"\x01end"):
                    nxt = self.total
                    continue
                f.write(line if line.endswith(b"\n") else line + b"\n")
                nxt = int(line[1:line.index(b"\t")]) + 1
            if done:
                break
            start = nxt + 1 if nxt < self.total else self.total
            restarts += 1
        f.close()
        return restarts


def cmd_run(a):
    index = json.loads((WORK / "chunks.json").read_text())
    if a.only:
        index = [x for x in index if re.search(a.only, x[1])]
    jobs = a.jobs or max(1, (os.cpu_count() or 2) // 4)
    if a.mem:
        mem = a.mem
    else:
        avail = next(int(l.split()[1]) * 1024 for l in open("/proc/meminfo")
                     if l.startswith("MemAvailable"))
        mem = avail // 2 // (2 * jobs)
    print(f"{len(index)} chunks, {jobs} at a time per side, a case may run {a.timeout} s, "
          f"{mem >> 20} MiB of address space per process", flush=True)
    start_container()
    t0 = time.time()
    try:
        work = [(s, n, total) for n, rk, total in index for s in SIDES]
        work.sort(key=lambda w: -w[2])

        def go(w):
            s, n, total = w
            t = time.time()
            ch = Chunk(s, n, total, a.timeout, mem)
            r = ch.run()
            print(f"{s} {n}: {total} cases, {r} restarts, {time.time() - t:.0f} s, "
                  f"slowest case {ch.slowest[0]:.3f} s (#{ch.slowest[1]})", flush=True)
            return r

        with ThreadPoolExecutor(jobs * 2) as ex:
            restarts = sum(ex.map(go, work))
    finally:
        stop_container()
    print(f"done in {time.time() - t0:.0f} s, {restarts} restarts")


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    sp = ap.add_subparsers(dest="cmd", required=True)
    sp.add_parser("lists")
    sp.add_parser("cases")
    r = sp.add_parser("run")
    r.add_argument("--timeout", type=float, default=None,
                   help="seconds a case may run before it is counted as a hang (worklog: how it was chosen)")
    r.add_argument("--jobs", type=int, default=0, help="chunks at a time per side (default: a quarter of the CPUs)")
    r.add_argument("--mem", type=int, default=0, help="address space per process in bytes "
                   "(default: half of MemAvailable shared by every process running)")
    r.add_argument("--only", help="a regular expression on the receiver names")
    sp.add_parser("compare")
    a = ap.parse_args()
    if a.cmd == "run" and a.timeout is None:
        sys.exit("--timeout is required (docs/verification/rc2-sweep/README.md says how it was measured)")
    {"lists": cmd_lists, "cases": cmd_cases, "run": cmd_run,
     "compare": lambda a: __import__("compare").main()}[a.cmd](a)
