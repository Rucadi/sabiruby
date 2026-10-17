"""The method sweep's comparison (docs/plans/rc2-sweep-plan.md X1): reads target/rc2-sweep/raw,
writes target/rc2-sweep/x1-diffs.tsv (every differing call) and the reduced tables in
docs/verification/rc2-sweep/.

Normalisation, applied to both sides before they are compared:
  - an address (0x followed by hex digits) is written 0x: it is where the object happens to be;
  - the Integer `object_id`, `__id__` answer is written <id>: it is the heap's numbering;
  - the names `sabiruby edge-spec` defines on Object (`_spec_event`, `t_print`, `_str_match?`,
    the harness the SabiRuby side runs under, sweep.py) are taken out of a list of names;
  - the answer of a method that lists names (LISTS) is compared as a set, sorted: its order is
    the order of the VM's method or variable table (a hash table in rc2).
The values `hash` answers are not normalised: they differ, and that is a recorded deviation (k).

Classes (the plan's):
  v  both answered, and the value, the receiver afterwards or what the block was given differ
  e  the exception class differs, or one side raised (or stopped the process: crash, timeout)
     and the other did not
  m  the same exception class, another message (same class and message, but the receiver or the
     block differs afterwards: v)
  k  a difference already recorded: README "Known deviations", docs/backlog.md ("合わせない差",
     "著者が決めた本家との差"), or docs/design/gems.md "Deviations kept"; known() says which rule
     took it (each names its record)
"""
import re
import sys
from collections import Counter, defaultdict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import cases as C  # noqa: E402
import causes as K  # noqa: E402

ROOT = Path(__file__).resolve().parents[2]
WORK = ROOT / "target/rc2-sweep"
OUT = ROOT / "docs/verification/rc2-sweep"
HERE = ROOT / "tools/rc2_sweep"

ADDR = re.compile(r"0x[0-9a-f]+")
NAN = 7                     # SW_VALS index of Float::NAN
INF = 8                     # SW_VALS index of Float::INFINITY
INT_VALS = {2, 3, 4, 5}     # 0, -1, 1, 2**70: the Integer values


def recv_sources():
    """The receivers' Ruby source, read off prelude.rb, for the one-line repro."""
    src = {}
    for m in re.finditer(r'"([^"]+)" => -> \{ (.*?) \}(?=,|\s*$)', (HERE / "prelude.rb").read_text(), re.M):
        src[m.group(1)] = m.group(2)
    return src


def val_sources():
    text = (HERE / "prelude.rb").read_text()
    body = text[text.index("SW_VALS = ["):text.index("]\n\n", text.index("SW_VALS = ["))]
    return [m.group(1) for m in re.finditer(r"-> \{ (.*?) \}(?=,)", body)]


RSRC, VSRC = None, None


def repro(c):
    rk, name, args, kw, blk = c
    r = RSRC.get(rk) or (rk[5:] if rk.startswith("self:") else rk)
    r = r.lstrip("+")
    a = [VSRC[i].replace("SW_S", "Struct.new(:a, :b)") for i in args]
    if kw:
        a.append("k: 1")
    if re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*[?!=]?", name) and not name.endswith("="):
        call = f"({r}).{name}" + (f"({', '.join(a)})" if a else "")
    else:
        call = f"({r}).__send__(:\"{name}\"" + "".join(", " + x for x in a) + ")"
    if blk:
        call += " { |*x| x[0] }"
    return call


def parse(path):
    out = {}
    for line in path.read_bytes().split(b"\n"):
        if not line.startswith(b"\x01"):
            continue
        f = line[1:].decode("utf-8", "replace").split("\t")
        f += [""] * (6 - len(f))
        out[int(f[0])] = f[1:6]
    return out


LISTS = {"methods", "public_methods", "private_methods", "protected_methods", "singleton_methods",
         "instance_methods", "public_instance_methods", "private_instance_methods",
         "protected_instance_methods", "constants", "global_variables", "local_variables",
         "class_variables", "undefined_instance_methods"}
HARNESS = re.compile(r"(, )?:(_spec_event|t_print|_str_match\?)(?=, |\])")


def sort_list(s):
    if not (s.startswith("[") and s.endswith("]")) or s == "[]":
        return s
    items = s[1:-1].split(", ")
    return "[" + ", ".join(sorted(items)) + "]"


def norm(c, f):
    f = [ADDR.sub("0x", x) for x in f]
    if f[0] == "ok" and ("_spec_event" in f[1] or "t_print" in f[1] or "_str_match" in f[1]):
        f[1] = HARNESS.sub("", f[1]).replace("[, ", "[")
    if c[1] in LISTS and f[0] == "ok":
        f[1] = sort_list(f[1])
    if c[1] in ("object_id", "__id__") and f[0] == "ok" and re.fullmatch(r"-?\d+", f[1]):
        f[1] = "<id>"
    return f


def klass(rf, sf):
    if rf == sf:
        return None
    if rf[0] == "raise" and sf[0] == "raise":
        if rf[1] != sf[1]:
            return "e"
        return "m" if rf[2] != sf[2] else "v"   # same exception: the receiver or the block differs
    if rf[0] in ("ok", "stopped") and sf[0] == rf[0]:
        return "v"
    return "e"


IDENTITY = {"equal?", "eql?", "==", "===", "!=", "include?", "member?", "index", "find_index",
            "rindex", "count", "delete", "uniq", "uniq!", "-", "&", "|", "difference", "union",
            "intersection", "intersect?", "key?", "has_key?", "value?", "has_value?", "assoc",
            "rassoc", "key", "tally", "delete_if", "subtract", "<=>", "eql?", "hash"}


def known(c, rf, sf, cls):
    """The rule of README "Known deviations" / docs/backlog.md a difference falls under, or None."""
    rk, name, args, kw, blk = c
    rclass = rk.split(":")[0]
    if name == "hash":
        return "hash values are SabiRuby's own (README)"
    if name == "instance_variables" and rk != "Object" and rf[0] == sf[0] == "ok":
        if sorted(rf[1].strip("[]").split(", ")) == sorted(sf[1].strip("[]").split(", ")):
            return "instance_variables order outside a plain Object (README, backlog)"
    if name in ("sort", "sort!", "sort_by", "min", "max", "minmax") and blk and rf[:4] == sf[:4]:
        return "sort hands its block other pairs (README, backlog)"
    if NAN in args and name in IDENTITY and cls == "v":
        return "a NaN has no identity (README)"
    if rclass == "Array" and name == "[]=" and len(args) == 3 and args[1] not in INT_VALS | {6}:
        return "a[i, len] = v with a length that is not a number (README)"
    if rclass == "Array" and name in ("[]", "[]=", "slice") and (NAN in args or INF in args or 5 in args[:1]):
        return "a Float that stands for no Integer as an Array index (README)"
    if name == "rand" and (NAN in args or INF in args):
        return "a Float that stands for no Integer as a rand limit (README)"
    if rclass == "Integer" and name in ("to_s", "inspect") and args and args[0] not in INT_VALS:
        return "Integer#to_s with a radix that is not an Integer (README)"
    if rk == "Integer:big" and name in ("~", ">>", "div", "%", "modulo", "divmod", "dup"):
        return "mruby-bigint's slips (~, >>, div and % by a Float, dup) kept (docs/design/gems.md)"
    if name == "infinite?" and rclass in ("Integer", "Rational"):
        return "Integer#infinite? answers nil (README)"
    if name == "define_singleton_method" and len(args) == 2:
        return "define_singleton_method(name, a_proc) (README)"
    if name == "push" and kw:
        return "[].push(1, k: 2) keeps the keyword Hash (README)"
    if cls == "m" and kw and rf[1] == "ArgumentError" and "given" in rf[2] and "given" in sf[2]:
        return "the given count with keywords (README)"
    return None


def ruby_class(rk):
    if rk.startswith("self:"):
        return rk[5:] + " (singleton)"
    return {"nil": "NilClass", "true": "TrueClass", "false": "FalseClass"}.get(rk, rk.split(":")[0])


def main():
    global RSRC, VSRC
    RSRC, VSRC = recv_sources(), val_sources()
    cases, skipped, one_side = C.build(WORK / "receivers.ref.tsv", WORK / "receivers.sabi.tsv")
    by = defaultdict(list)
    for c in cases:
        by[c[0]].append(c)
    from sweep import chunk_name
    rows, counts, per_class, missing = [], Counter(), defaultdict(Counter), 0
    per_method = defaultdict(Counter)
    compared = 0
    for rk, cs in by.items():
        n = chunk_name(rk)
        rp, sp = WORK / "raw/ref" / f"{n}.tsv", WORK / "raw/sabi" / f"{n}.tsv"
        if not rp.exists() or not sp.exists():
            missing += len(cs)
            continue
        R, S = parse(rp), parse(sp)
        # the same call without the keyword, for the cause "kw-dropped" (causes.py)
        plain = {(c[1], tuple(c[2]), c[4]): j for j, c in enumerate(cs) if not c[3]}
        for i, c in enumerate(cs):
            if i not in R or i not in S:
                missing += 1
                continue
            compared += 1
            per_method[(ruby_class(rk), c[1])]["calls"] += 1
            rf, sf = norm(c, R[i]), norm(c, S[i])
            cls = klass(rf, sf)
            if cls is None:
                counts["same"] += 1
                continue
            k = known(c, rf, sf, cls)
            sub = cls
            if k:
                cls = "k"
            counts[cls] += 1
            per_class[ruby_class(rk)][cls] += 1
            per_method[(ruby_class(rk), c[1])][cls] += 1
            j = plain.get((c[1], tuple(c[2]), c[4])) if c[3] else None
            nokw = (norm(c, R[j]), norm(c, S[j])) if j is not None and j in R and j in S else (None, None)
            cz = "known: " + k if k else K.cause({"cls": sub, "rk": rk, "name": c[1], "args": list(c[2]),
                                                  "kw": c[3], "blk": c[4], "R": rf, "S": sf,
                                                  "R_nokw": nokw[0], "S_nokw": nokw[1]})
            rows.append((cls, sub, cz, rk, c[1], ",".join(map(str, c[2])), int(c[3]), int(c[4]),
                         repro(c), *rf, *sf))
    OUT.mkdir(parents=True, exist_ok=True)
    head = ("class\tfirst_class\tcause\treceiver\tmethod\targs\tkw\tblock\trepro\t"
            "ref_status\tref_value\tref_message\tref_receiver\tref_block\t"
            "sabi_status\tsabi_value\tsabi_message\tsabi_receiver\tsabi_block\n")
    with open(WORK / "x1-diffs.tsv", "w") as f:   # the full table stays out of git (MBs)
        f.write(head)
        for r in sorted(rows, key=lambda r: (r[0], r[3], r[4])):
            f.write("\t".join(str(x) for x in r) + "\n")
    write_reduced(rows, counts, per_class, per_method, compared, missing, skipped, one_side)
    print(f"compared {compared} calls ({missing} without an answer on a side)")
    print(dict(counts))
    for k in sorted(per_class):
        print(k, dict(per_class[k]))
    return rows, counts, per_class, compared, missing, skipped, one_side


def answer(st, val, msg):
    return f"{val}" if st == "ok" else f"{val}: {msg}" if st == "raise" else f"{st} {val} {msg}".strip()


def write_reduced(rows, counts, per_class, per_method, compared, missing, skipped, one_side):
    """The tables that are committed: one row per cause, one per method, the one-side lists."""
    groups = defaultdict(list)
    for r in rows:
        groups[r[2]].append(r)
    with open(OUT / "x1-causes.tsv", "w") as f:
        f.write("cause\tverdict\tsize\tbook\tdescription\tcalls\tv\te\tm\tk\tinternal_calls\tmethods\texample\trc2\t"
                "sabiruby\tmethod_list\n")
        for cz, rs in sorted(groups.items(), key=lambda kv: -len(kv[1])):
            c = Counter(r[0] for r in rs)
            ms = sorted({f"{ruby_class(r[3])}#{r[4]}" for r in rs})
            ex = min(rs, key=lambda r: (K.internal(r[4]), len(r[8])))
            desc = K.DESCRIPTIONS.get(cz, "")
            f.write("\t".join(str(x) for x in (
                cz, *K.verdict(cz), desc, len(rs), c["v"], c["e"], c["m"], c["k"], sum(K.internal(r[4]) for r in rs),
                len(ms), ex[8], answer(ex[9], ex[10], ex[11]), answer(ex[14], ex[15], ex[16]),
                " ".join(ms))) + "\n")
    bym = defaultdict(list)
    for r in rows:
        bym[(ruby_class(r[3]), r[4])].append(r)
    with open(OUT / "x1-methods.tsv", "w") as f:
        f.write("class\tmethod\tinternal\tcalls\tdiffer\tv\te\tm\tk\tmain_cause\texample\trc2\tsabiruby\n")
        for key in sorted(per_method):
            pm = per_method[key]
            rs = bym.get(key, [])
            if not rs:
                continue
            main = Counter(r[2] for r in rs).most_common(1)[0][0]
            ex = min((r for r in rs if r[2] == main), key=lambda r: len(r[8]))
            f.write("\t".join(str(x) for x in (
                key[0], key[1], int(K.internal(key[1])), pm["calls"], len(rs), pm["v"], pm["e"], pm["m"],
                pm["k"], main, ex[8], answer(ex[9], ex[10], ex[11]), answer(ex[14], ex[15], ex[16]))) + "\n")
    with open(OUT / "x1-left-out.tsv", "w") as f:
        f.write("receiver\tmethod\twhy\n")
        for r in sorted(set(skipped)):
            f.write("\t".join(r) + "\n")
    # methods one VM has and the other does not, by owner (methods.rb), as rc2-leftovers9 V1-1 listed them
    def owners(side):
        out = set()
        for line in (WORK / f"methods.{side}.tsv").read_text().splitlines():
            k, o, n = line.split("\t")
            if k != "M":
                out.add((o, n, k))
        return out
    ro, so = owners("ref"), owners("sabi")
    rcls = {o for o, n, k in ro}
    scls = {o for o, n, k in so}
    rn = {(o, n) for o, n, k in ro}
    sn = {(o, n) for o, n, k in so}
    with open(OUT / "x1-one-side.tsv", "w") as f:
        f.write("side\towner\tmethod\tkind\tcallable_on_the_other_side\n")
        for side, a, bn, other_cls in (("rc2 only", ro, sn, scls), ("SabiRuby only", so, rn, rcls)):
            for o, n, k in sorted(a):
                if o not in other_cls or (o, n) in bn:
                    continue
                f.write(f"{side}\t{o}\t{n}\t{k}\t\n")
        for side, a, b in (("rc2 only class", rcls, scls), ("SabiRuby only class", scls, rcls)):
            for o in sorted(a - b):
                f.write(f"{side}\t{o}\t\t\t\n")
    # callable on a receiver on one side only (whatever the owner): the sweep's own list
    with open(OUT / "x1-one-side-callable.tsv", "w") as f:
        f.write("receiver\tmethod\tonly_on\towner\n")
        for r in sorted(set(one_side)):
            f.write("\t".join(r) + "\n")
    with open(OUT / "x1-counts.md", "w") as f:
        f.write("# X1 counts (generated by tools/rc2_sweep/compare.py; do not edit)\n\n")
        f.write(f"Calls compared: {compared}; without an answer on a side: {missing}.\n\n")
        f.write("| | same | v | e | m | k |\n|---|---:|---:|---:|---:|---:|\n")
        f.write(f"| all | {counts['same']} | {counts['v']} | {counts['e']} | {counts['m']} | {counts['k']} |\n")
        for k in sorted(per_class):
            pc = per_class[k]
            f.write(f"| {k} | | {pc['v']} | {pc['e']} | {pc['m']} | {pc['k']} |\n")
        f.write("\nCalls by cause (the 40 largest; every cause is in x1-causes.tsv):\n\n")
        f.write("| cause | calls | v | e | m | k | methods |\n|---|---:|---:|---:|---:|---:|---:|\n")
        for cz, rs in sorted(groups.items(), key=lambda kv: -len(kv[1]))[:40]:
            c = Counter(r[0] for r in rs)
            ms = len({(r[3].split(':')[0], r[4]) for r in rs})
            f.write(f"| {cz.replace('|', '¦')} | {len(rs)} | {c['v']} | {c['e']} | {c['m']} | {c['k']} | {ms} |\n")


if __name__ == "__main__":
    main()
