#!/usr/bin/env python3
"""Ruby programs on both VMs (docs/plans/rc2-sweep-plan.md X2).

  tools/rc2_sweep/programs.py run --timeout S   # every program on both VMs (Docker for the reference)
  tools/rc2_sweep/programs.py compare           # docs/verification/rc2-sweep/x2-*.tsv

The programs:
  book      book_mruby3/data/code/*.rb (read in place, never written)
  custom    tests/custom/*.rb
  kit       mruby-porting-kit fixtures/*.rb and samples/*.rb (read in place)
  mrbtest   mruby's test/t and the bundled gems' test/*.rb, as tools/mrbtest.sh copies them into
            tests/mrbtest/src/, each run as one program: test/assert.rb, assert_patch.rb (the C
            driver's t_print, _str_match? and FLOAT_TOLERANCE in Ruby, and one line per
            assertion), the regexp gem's shared helper (prelude.rb), the file
A program runs in its own directory, with the same path inside the container (each source
directory is mounted read-only at its own path), so `__FILE__` and backtraces name the same
files. Compared: standard output, standard error, the exit status; for mrbtest, assertion by
assertion. The reference image: kishima/mruby:4.1.0-rc2-utf8 (SabiRuby's default build), except
for tests/custom, where tools/custom.sh's rule is kept (the `# utf8-only:` cases on the utf8
image, the others on kishima/mruby:4.1.0-rc2, which is what their .rc.out files come from).
"""
import argparse
import json
import os
import re
import subprocess
import sys
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
HERE = ROOT / "tools/rc2_sweep"
WORK = ROOT / "target/rc2-sweep/x2"
OUT = ROOT / "docs/verification/rc2-sweep"
SABI = ROOT / "target/release/sabiruby"
BOOK = (ROOT / "../../book_mruby3/data/code").resolve()
KIT = (ROOT / "../mruby-porting-kit").resolve()
IMAGES = {"utf8": "kishima/mruby:4.1.0-rc2-utf8", "bytes": "kishima/mruby:4.1.0-rc2"}
# tools/mrbtest.sh OTHER_BUILD: the pair of regexp test files that belongs to the byte-string build
BYTES_ONLY = {"gem_ascii_case", "gem_ascii_ctype"}
ADDR = re.compile(rb"0x[0-9a-f]+")


def programs():
    out = []
    for p in sorted(BOOK.glob("*.rb")):
        out.append(("book", p, "utf8"))
    for p in sorted((ROOT / "tests/custom").glob("*.rb")):
        img = "utf8" if "\n# utf8-only:" in "\n" + p.read_text(errors="replace") else "bytes"
        out.append(("custom", p, img))
    for d in ("fixtures", "samples"):
        for p in sorted((KIT / d).glob("*.rb")):
            out.append((f"kit-{d}", p, "utf8"))
    src = ROOT / "tests/mrbtest/src"
    asm = WORK / "mrbtest"
    asm.mkdir(parents=True, exist_ok=True)
    head = (src / "assert.rb").read_text() + (HERE / "assert_patch.rb").read_text() + \
        (src / "prelude.rb").read_text()
    for p in sorted(src.glob("*.rb")):
        if p.stem in ("assert", "prelude") or p.stem in BYTES_ONLY:
            continue
        t = asm / p.name
        t.write_text(head + "\n" + p.read_text() + "\nprint \"\\x01END\\t#{$ok_test}\\t#{$ko_test}\\t"
                     "#{$kill_test}\\t#{$skip_test}\\t#{$warning_test}\\n\"\n")
        out.append(("mrbtest", t, "utf8"))
    return out


def container(img):
    return f"rc2-sweep-x2-{img}"


def start_containers():
    mounts = []
    for d in (WORK, BOOK, KIT, ROOT / "tests"):
        mounts += ["-v", f"{d}:{d}" + ("" if d == WORK else ":ro")]
    for k, img in IMAGES.items():
        subprocess.run(["docker", "rm", "-f", container(k)], capture_output=True)
        subprocess.run(["docker", "run", "-d", "--name", container(k), "-e", "TZ=UTC", *mounts, img,
                        "sleep", "infinity"], check=True, capture_output=True)


def stop_containers():
    for k in IMAGES:
        subprocess.run(["docker", "rm", "-f", container(k)], capture_output=True)


def run_one(side, group, path, img, limit):
    d = str(path.parent)
    if side == "ref":
        cmd = ["docker", "exec", "-w", d, container(img), "timeout", "-s", "KILL", str(limit),
               "mruby", path.name]
    else:
        cmd = ["timeout", "-s", "KILL", str(limit), str(SABI), path.name]
    t = time.time()
    p = subprocess.run(cmd, capture_output=True, cwd=d, stdin=subprocess.DEVNULL,
                       env={**os.environ, "TZ": "UTC"})
    return {"rc": p.returncode, "out": p.stdout.decode("utf-8", "replace"),
            "err": p.stderr.decode("utf-8", "replace"), "time": time.time() - t}


def key(group, path):
    return f"{group}/{path.name}"


def cmd_run(a):
    progs = programs()
    jobs = a.jobs or max(1, (os.cpu_count() or 2) // 4)
    print(f"{len(progs)} programs, {jobs} at a time per side, {a.timeout} s each", flush=True)
    start_containers()
    res = {}
    try:
        work = [(s, g, p, i) for g, p, i in progs for s in ("ref", "sabi")]
        with ThreadPoolExecutor(jobs * 2) as ex:
            for (s, g, p, i), r in zip(work, ex.map(lambda w: run_one(w[0], w[1], w[2], w[3], a.timeout), work)):
                res.setdefault(key(g, p), {"group": g, "path": str(p), "image": i})[s] = r
    finally:
        stop_containers()
    (WORK / "results.json").write_text(json.dumps(res))
    slow = max((r[s]["time"], k, s) for k, r in res.items() for s in ("ref", "sabi") if r[s]["rc"] != -9)
    kills = [(k, s) for k, r in res.items() for s in ("ref", "sabi") if r[s]["rc"] == -9 or r[s]["rc"] == 137]
    print(f"slowest that finished: {slow[0]:.2f} s ({slow[1]} on {slow[2]}); killed at the limit: {kills}")


def norm(s, path):
    s = ADDR.sub(b"0x", s.encode()).decode()
    return s


def asserts(out):
    rows, end = [], None
    for line in out.split("\n"):
        if line.startswith("\x01A\t"):
            f = line[3:].split("\t")
            f += [""] * (5 - len(f))
            rows.append(f)
        elif line.startswith("\x01END\t"):
            end = line[5:].split("\t")
    return rows, end


def cmd_compare(a):
    res = json.loads((WORK / "results.json").read_text())
    OUT.mkdir(parents=True, exist_ok=True)
    files, arows = [], []
    for k, r in sorted(res.items()):
        R, S = r["ref"], r["sabi"]
        g = r["group"]
        if g == "mrbtest":
            ra, rend = asserts(norm(R["out"], k))
            sa, send = asserts(norm(S["out"], k))
            rm = {(x[0], x[2]): x for x in ra}
            sm = {(x[0], x[2]): x for x in sa}
            nd = 0
            for kk in sorted(set(rm) | set(sm), key=lambda x: int(x[0])):
                x, y = rm.get(kk), sm.get(kk)
                if x and y and x[3] == y[3] and x[4] == y[4]:
                    continue
                if x is None or y is None:
                    cls = "missing"
                elif x[3] != y[3]:
                    cls = "result"
                else:
                    cls = "message"
                nd += 1
                arows.append((k, kk[0], (x or y)[1], kk[1], cls, x[3] if x else "-", y[3] if y else "-",
                              x[4] if x else "", y[4] if y else ""))
            same = nd == 0 and R["rc"] == S["rc"]
            files.append((g, k, "same" if same else f"{nd} assertions differ", R["rc"], S["rc"],
                          len(ra), len(sa), "|".join(rend or []), "|".join(send or []), "", ""))
            continue
        ro, so = norm(R["out"], k), norm(S["out"], k)
        re_, se = norm(R["err"], k), norm(S["err"], k)
        what = [n for n, x, y in (("stdout", ro, so), ("stderr", re_, se), ("exit", R["rc"], S["rc"])) if x != y]
        files.append((g, k, ",".join(what) or "same", R["rc"], S["rc"], len(ro), len(so), "", "",
                      first_diff(ro, so), first_diff(re_, se)))
    with open(OUT / "x2-programs.tsv", "w") as f:
        f.write("group\tprogram\tdiffers\tref_exit\tsabi_exit\tref_out_len\tsabi_out_len\t"
                "ref_counts\tsabi_counts\tfirst_stdout_diff\tfirst_stderr_diff\n")
        for row in files:
            f.write("\t".join(str(x).replace("\t", " ").replace("\n", "\\n") for x in row) + "\n")
    with open(OUT / "x2-asserts.tsv", "w") as f:
        f.write("program\tseq\tdepth\tassertion\tdiffers\tref\tsabi\tref_message\tsabi_message\n")
        for row in arows:
            f.write("\t".join(str(x).replace("\t", " ") for x in row) + "\n")
    from collections import Counter
    c = Counter((x[0], x[2] == "same") for x in files)
    for g in sorted({x[0] for x in files}):
        print(g, "same", c[(g, True)], "differ", c[(g, False)])
    print("assertions that differ:", len(arows), Counter(x[4] for x in arows))


def first_diff(a, b):
    if a == b:
        return ""
    al, bl = a.split("\n"), b.split("\n")
    for i in range(max(len(al), len(bl))):
        x = al[i] if i < len(al) else "<none>"
        y = bl[i] if i < len(bl) else "<none>"
        if x != y:
            return f"line {i + 1}: ref {x[:200]!r} / sabi {y[:200]!r}"
    return ""


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    sp = ap.add_subparsers(dest="cmd", required=True)
    r = sp.add_parser("run")
    r.add_argument("--timeout", type=int, required=True, help="seconds a program may run")
    r.add_argument("--jobs", type=int, default=0, help="programs at a time per side (default: a quarter of the CPUs)")
    sp.add_parser("compare")
    a = ap.parse_args()
    {"run": cmd_run, "compare": cmd_compare}[a.cmd](a)
