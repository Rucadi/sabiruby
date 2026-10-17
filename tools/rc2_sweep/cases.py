"""The case table of the method sweep (docs/plans/rc2-sweep-plan.md X1).

A case is one call: a receiver of prelude.rb, a method both VMs can call on it, the indices of
its arguments in SW_VALS, whether a keyword argument (k: 1) is passed and whether a block is.
Which argument counts are tried comes from the reference's own declaration of the method
(tools/rc2-aspec/rc2-aspec.tsv, the MRB_ARGS_* bits rc2's VM checks the count against), so a
call is not spent on a count the VM refuses before the method runs; that refusal was compared
on its own (tools/rc2_aspec.sh --compare, 989 of 989 the same).
"""
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
NVALS = 22          # len(SW_VALS) in prelude.rb: the values the plan lists
MAXARGS = 3         # the plan: "引数 0〜3 個"

# Left out, and why (the plan's list first). A name here is left out on every receiver.
SKIP = {
    # stops or leaves the process, or waits for input / another process
    "exit": "leaves", "exit!": "leaves", "abort": "leaves", "sleep": "waits", "usleep": "waits",
    "sleep_ms": "waits", "loop": "does not end", "gets": "reads stdin", "readline": "reads stdin",
    "readlines": "reads stdin", "system": "runs a process", "`": "runs a process",
    "exec": "runs a process", "fork": "runs a process", "spawn": "runs a process",
    "trap": "signals", "open": "opens a file (mruby-io, reference only)",
    # reads other files
    "require": "reads a file", "require_relative": "reads a file", "load": "reads a file",
    "__load_file": "reads a file", "__exec_file": "reads a file",
    # eval and the block-running forms (the plan: eval 系)
    "eval": "eval", "instance_eval": "eval", "class_eval": "eval", "module_eval": "eval",
    "instance_exec": "eval", "class_exec": "eval", "module_exec": "eval",
    # writes to the output the sweep reads its results from (X2 compares output)
    "p": "writes stdout", "print": "writes stdout", "puts": "writes stdout",
    "printf": "writes stdout", "putc": "writes stdout", "pp": "writes stdout",
    "display": "writes stdout", "__printstr__": "writes stdout", "warn": "writes stderr",
    # answers with what is in the heap
    "each_object": "walks the heap", "count_objects": "counts the heap",
    # the sweep's own helpers
    "sw_esc": "the sweep's own", "sw_insp": "the sweep's own",
}

OBJECT_OWNERS = {"Kernel", "BasicObject", "Object"}


def load_aspec():
    spec = {}
    for line in (ROOT / "tools/rc2-aspec/rc2-aspec.tsv").read_text().splitlines():
        key, kind, a = line.split("\t")
        spec[key] = None if a == "-" else int(a)
    return spec


def decode(a):
    """MRB_ARGS_* (include/mruby.h): req<<18, opt<<13, rest 1<<12, post<<7, key<<2, kdict 2, block 1."""
    return {"req": (a >> 18) & 0x1f, "opt": (a >> 13) & 0x1f, "rest": (a >> 12) & 1,
            "post": (a >> 7) & 0x1f, "key": (a >> 2) & 0x1f, "kdict": (a >> 1) & 1, "block": a & 1}


def from_params(params):
    """Method#parameters when the table has no spec (a Proc-defined method: Proc#call, Proc#[])."""
    d = {"req": 0, "opt": 0, "rest": 0, "post": 0, "key": 0, "kdict": 0, "block": 0}
    for p in params:
        t = p[0]
        if t == "req": d["req"] += 1
        elif t == "opt": d["opt"] += 1
        elif t == "rest": d["rest"] = 1
        elif t in ("key", "keyreq"): d["key"] += 1
        elif t == "keyrest": d["kdict"] = 1
        elif t == "block": d["block"] = 1
    return d


def counts(d):
    lo = d["req"] + d["post"]
    hi = MAXARGS if d["rest"] else min(MAXARGS, lo + d["opt"])
    return list(range(lo, hi + 1)) if lo <= MAXARGS else []


def arg_rows(n):
    """0 and 1 argument: every value. 2: every pair. 3: every pair of values in every pair of
    positions (a, b, (a + b) mod 22, an orthogonal Latin square): as many rows as two
    arguments, where the full product would be 22 times more."""
    r = range(NVALS)
    if n == 0: return [()]
    if n == 1: return [(a,) for a in r]
    if n == 2: return [(a, b) for a in r for b in r]
    return [(a, b, (a + b) % NVALS) for a in r for b in r]


def owner_key(owner, name):
    if owner.startswith("#<Class:") and owner.endswith(">"):
        return f"{owner[8:-1]}.{name}"
    return f"{owner}#{name}"


def read_receivers(path):
    out = {}
    for line in Path(path).read_text().splitlines():
        rk, name, owner, params = line.split("\t")
        out.setdefault(rk, {})[name] = (owner, params)
    return out


def build(ref_path, sabi_path):
    """Returns (cases, skipped, one_side): cases as tuples (recv, name, args, kw, blk)."""
    ref, sabi = read_receivers(ref_path), read_receivers(sabi_path)
    spec = load_aspec()
    cases, skipped, one_side = [], [], []
    for rk in ref:
        rm, sm = ref[rk], sabi.get(rk, {})
        for name in sorted(set(rm) | set(sm)):
            if name not in rm or name not in sm:
                one_side.append((rk, name, "reference" if name in rm else "sabiruby",
                                 (rm.get(name) or sm.get(name))[0]))
                continue
            owner, params = rm[name]
            selfr = rk.startswith("self:")
            if selfr and owner != f"#<Class:{rk[5:]}>":
                continue        # a class's Module/Class/Kernel methods: on Module.new/Class.new
            if not selfr and rk != "Object" and owner in OBJECT_OWNERS:
                continue        # Kernel and BasicObject: on Object.new
            if name in SKIP:
                skipped.append((rk, name, SKIP[name]))
                continue
            a = spec.get(owner_key(owner, name))
            if a is not None:
                d = decode(a)
            else:
                d = from_params([[t] for t in re.findall(r"\[:(\w+)", params)])
            ns = counts(d)
            if not ns:
                continue
            keys = d["key"] or d["kdict"]
            for n in ns:
                for args in arg_rows(n):
                    cases.append((rk, name, args, False, False))
                    if d["block"] or n == ns[0]:
                        cases.append((rk, name, args, False, True))
                    if keys or n == ns[0]:
                        cases.append((rk, name, args, True, False))
    return cases, skipped, one_side
