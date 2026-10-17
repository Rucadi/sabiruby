"""The causes the differing calls of the method sweep are grouped by (compare.py).

A rule names one cause and says which rows are its own; the first rule that takes a row wins.
A row no rule takes is grouped by a key made from the answers themselves (AUTO): for an
exception, its class and its message with the values written out of it (both sides); for a
value, the method. The rules were written after reading the largest of those automatic groups
(docs/worklog/2026-10-18-rc2-sweep.md), and each was checked on a one-line program on both VMs.
"""
import re

WRONG_NUM = "wrong number of arguments"


def _raise(f, cls=None):
    return f[0] == "raise" and (cls is None or f[1] == cls)


def template(s):
    s = re.sub(r"'[^']*'", "'X'", s)
    s = re.sub(r'"[^"]*"', '"X"', s)
    s = re.sub(r"-?\d+(\.\d+)?(e[+-]?\d+)?", "N", s)
    s = re.sub(r"\b(nil|true|false|NaN|Infinity|-Infinity)\b", "V", s)
    s = re.sub(r":\w+[?!=]?", "S", s)
    s = re.sub(r"\[[^\]]*\]|\{[^}]*\}", "C", s)
    s = re.sub(r"\b(Integer|Float|String|Symbol|Array|Hash|Range|Proc|SW_S|NilClass|TrueClass|FalseClass)\b", "T", s)
    return s[:120]


def drop_empty(s):
    """An inspect with the empty items of its argument list taken out (what join-empty leaves)."""
    prev = None
    while prev != s:
        prev, s = s, s.replace("(, ", "(").replace(", )", ")").replace(", , ", ", ")
    return s


def floats_close(a, b):
    try:
        x, y = float(a), float(b)
    except ValueError:
        return False
    return x != y and abs(x - y) <= 4 * 2.220446049250313e-16 * max(abs(x), abs(y))  # a few ulps


# (id, what it is, predicate(row)) — row: dict(cls, rk, name, args, kw, blk, R, S)
RULES = [
    ("crash", "SabiRuby stops the process (a Rust panic or an abort) where rc2 answers",
     lambda r: r["S"][0] == "crash"),
    ("ref-crash", "rc2 stops the process where SabiRuby answers",
     lambda r: r["R"][0] == "crash"),
    ("timeout", "one side runs past the time limit and the other answers",
     lambda r: "timeout" in (r["R"][0], r["S"][0])),
    ("internal-helper", "a helper whose name starts with __ (each VM's own, called by a method written in Ruby "
     "with arguments it makes itself): the public method is swept on its own",
     lambda r: internal(r["name"])),
    ("kw-dropped", "a keyword argument to a method that takes none: rc2 drops the keyword Hash (it answers as "
     "it does without k: 1), SabiRuby passes it on as the last positional argument (the root of the "
     "recorded push(1, k: 2) deviation)",
     lambda r: r["kw"] and r["R_nokw"] is not None and r["R"] == r["R_nokw"] and r["S"] != r["S_nokw"]),
    ("bytesplice", "String#bytesplice takes its arguments another way: rc2 lets any count through and says "
     "'wrong number of arumgnts' (a misspelling in rc2) only after it has looked at the values, reads two "
     "arguments as (range, str) and converts before it checks the bounds; SabiRuby checks the count, then "
     "the bounds (IndexError), then converts",
     lambda r: r["name"] == "bytesplice"),
    ("argc-splat", "String#chomp, chomp!, to_i and alias_method, which rc2 declares MRB_ARGS_ANY and counts in "
     "mrb_get_args: given their arguments through a splat (the sweep calls __send__(m, *a)), rc2 converts "
     "the first argument before it counts them and raises TypeError; a literal call (\"abc\".chomp(nil, nil)) "
     "is ArgumentError on both. SabiRuby counts first in both forms",
     lambda r: (r["rk"].split(":")[0], r["name"]) in (("String", "chomp"), ("String", "chomp!"),
     ("String", "to_i"), ("Class", "alias_method"), ("Module", "alias_method"))
     and _raise(r["S"], "ArgumentError") and WRONG_NUM in r["S"][2]
     and not (_raise(r["R"], "ArgumentError") and WRONG_NUM in r["R"][2])),
    ("argc-narrow", "SabiRuby refuses an argument count that rc2's declaration (MRB_ARGS_*) accepts "
     "and its body reads", lambda r: _raise(r["S"], "ArgumentError") and WRONG_NUM in r["S"][2]
     and not (_raise(r["R"], "ArgumentError") and WRONG_NUM in r["R"][2])),
    ("argc-wide", "rc2's body refuses a count its declaration lets through (ANY/opt), SabiRuby takes it",
     lambda r: _raise(r["R"], "ArgumentError") and WRONG_NUM in r["R"][2]
     and not (_raise(r["S"], "ArgumentError") and WRONG_NUM in r["S"][2])),
    ("conv-float-msg", "the TypeError of a value that is no Float: rc2 'X cannot be converted to Float', "
     "SabiRuby 'can't convert X into Float' / 'X can't be coerced into Float'",
     lambda r: r["cls"] == "m" and "cannot be converted to Float" in r["R"][2]
     and ("can't convert" in r["S"][2] or "can't be coerced" in r["S"][2])),
    ("conv-int-msg", "the TypeError of a value that is no Integer: other wording, or SabiRuby names Float "
     "as the target where rc2 names Integer (Integer#div, &, |, ^, allbits?, pow ...)",
     lambda r: r["cls"] == "m" and r["R"][1] == "TypeError" and
     ("into Integer" in r["R"][2] + r["S"][2] or "to Integer" in r["R"][2] + r["S"][2])),
    ("type-nan-name", "a type error names a NaN/Infinity by its value where rc2 names the class Float",
     lambda r: r["cls"] == "m" and "Float" in r["R"][2] and re.search(r"\b(NaN|Infinity)\b", r["S"][2])
     and not re.search(r"\b(NaN|Infinity)\b", r["R"][2])),
    ("module-required", "a value that is no Module: rc2 'wrong argument type X (expected Module)' or "
     "'X is not class/module', SabiRuby 'class or module required'",
     lambda r: "class or module required" in r["S"][2] and r["S"] != r["R"]),
    ("method-missing-name", "method_missing called with a first argument that is no Symbol: rc2 raises TypeError "
     "'X is not a symbol nor a string' (a String: NoMethodError naming it), SabiRuby a NoMethodError that names '?' "
     "(CRuby ArgumentError 'method name must be a Symbol')",
     lambda r: r["name"] == "method_missing" and "'?'" in r["S"][2]),
    ("not-a-symbol", "a value that is no name: rc2 'nil is not a symbol nor a string', SabiRuby 'NilClass is not a "
     "symbol nor a string' (Binding#local_variable_*)", lambda r: r["cls"] in ("m", "e") and
     ("not a symbol" in r["S"][2]) != ("not a symbol" in r["R"][2]) or
     (r["cls"] == "m" and "not a symbol" in r["S"][2] and r["S"][2] != r["R"][2])),
    ("conv-order", "both raise, one RangeError ('out of range') and the other TypeError: the two VMs "
     "convert the arguments in another order (rc2 often the later one first)",
     lambda r: r["cls"] == "e" and {r["R"][1], r["S"][1]} == {"RangeError", "TypeError"}),
    ("frozen-set", "rc2 lets a frozen Set change; SabiRuby raises FrozenError",
     lambda r: r["rk"] == "Set:frozen" and r["R"][0] == "ok" and _raise(r["S"], "FrozenError")),
    ("frozen", "FrozenError on one side only",
     lambda r: _raise(r["S"], "FrozenError") != _raise(r["R"], "FrozenError")),
    ("kw-as-hash", "a keyword argument to a method that takes none: rc2 drops the keyword Hash, SabiRuby "
     "passes it as the last positional argument (the root of the recorded push(1, k: 2) deviation)",
     lambda r: r["kw"] and "{k: 1}" in r["S"][1] + r["S"][3] and "{k: 1}" not in r["R"][1] + r["R"][3]),
    ("unknown-keyword", "'unknown keyword: k' (rc2) and 'unknown keyword: :k' (SabiRuby)",
     lambda r: "unknown keyword" in r["R"][2] and "unknown keyword" in r["S"][2]),
    ("join-empty", "Array#join leaves out an empty nested Array: [[], 1].join(', ') is '1' (rc2 and CRuby: ', 1');"
     " seen through Enumerator#inspect, which joins the arguments ('step(1, )' on rc2, 'step(1)' here)",
     lambda r: r["cls"] == "v" and "Enumerator" in r["R"][1]
     and drop_empty(r["R"][1]) == drop_empty(r["S"][1])),
    ("enum-args-kw", "Enumerator#inspect of a call given a keyword Hash: rc2 shows it, SabiRuby leaves it out",
     lambda r: r["cls"] == "v" and "Enumerator" in r["R"][1] and "{k: 1}" in r["R"][1]),
    ("fiber-inspect", "Fiber#inspect: rc2 writes where the Fiber's block is (file:line), SabiRuby does not",
     lambda r: r["rk"].startswith("Fiber") and "#<Fiber:0x " in r["R"][1] + r["R"][3]
     and r["S"][1:] != r["R"][1:] and re.sub(r"#<Fiber:0x [^ >]+ ", "#<Fiber:0x ", r["R"][1] + r["R"][3])
     == r["S"][1] + r["S"][3]),
    ("binding-inspect", "Binding#inspect: SabiRuby writes the Binding's insides (proc, env, recv, pc)",
     lambda r: "#<Binding:0x proc=" in r["S"][1] + r["S"][3]),
    ("set-order", "the order of a Set's elements: rc2 does not keep the order they were added in",
     lambda r: r["rk"].startswith("Set") and r["cls"] == "v" and r["R"][1].startswith("Set[")
     and sorted(r["R"][1][4:-1].split(", ")) == sorted(r["S"][1][4:-1].split(", "))),
    ("float-ulp", "a Float differs in its last digits (rc2 calls the C library: pow, cosh, ...)",
     lambda r: r["cls"] == "v" and floats_close(r["R"][1], r["S"][1])),
    ("negative-zero", "-0.0 where rc2 answers 0.0 (Float#% / modulo)",
     lambda r: r["cls"] == "v" and {r["R"][1], r["S"][1]} == {"0.0", "-0.0"}),
    ("reinitialize", "calling initialize again on a live object: Array#initialize keeps the elements on rc2, "
     "String#initialize empties it, Class/Module#initialize answer self, Range#initialize says 'bad value'",
     lambda r: r["name"] == "initialize" and r["rk"].split(":")[0] in
     ("Array", "String", "Class", "Module", "Range")),
    ("data-init", "Data#initialize with positional arguments: rc2 'missing keyword: x', SabiRuby a count error",
     lambda r: r["rk"] == "Data" and r["name"] == "initialize"),
    ("bigint-float", "an Integer past 64 bits with a Float or a Rational: rc2 answers a Float where SabiRuby "
     "keeps a Rational or an Integer (quo, remainder, divmod, Rational(2**70, 1.5))",
     lambda r: r["cls"] == "v" and 5 in r["args"] + ([5] if r["rk"] == "Integer:big" else [])
     and r["name"] in ("quo", "remainder", "divmod", "Rational", "fdiv", "/", "div", "modulo", "%")),
    ("int-div-float", "Integer#div / ceildiv by a Float: rc2 floors after a Float division differently "
     "((-3).div(1.5) is -3 on rc2, -2 here)", lambda r: r["cls"] == "v" and r["name"] in ("div", "ceildiv")),
    ("time-now", "Time.new / Time#initialize without arguments: the current time, a second apart",
     lambda r: r["rk"].startswith("Time") and r["name"] == "initialize" and not r["args"]),
    ("time-year", "Time#inspect/to_s of a year below 1000: rc2 writes the year as it is ('0-01-01', "
     "'-1-01-01'), SabiRuby pads it to 3 or 4 digits ('0000-01-01', '-001-01-01'; CRuby '-0001')",
     lambda r: r["rk"].startswith(("Time", "self:Time")) and r["cls"] == "v"
     and re.search(r"(^|\b|-)-?\d{1,3}-01-01", r["R"][1] + r["R"][3])
     and re.search(r"-?0\d{2,3}-01-01", r["S"][1] + r["S"][3])),
    ("gc-setter", "the value GC's setters answer (generational_mode=, interval_ratio=, ...)",
     lambda r: r["rk"] == "self:GC" and r["cls"] == "v"),
    ("method-lists", "the lists of names (methods, constants, global_variables) differ by more than order "
     "(the gems each build carries)", lambda r: r["cls"] == "v" and r["name"] in (
         "methods", "public_methods", "private_methods", "protected_methods", "singleton_methods",
         "instance_methods", "public_instance_methods", "private_instance_methods",
         "protected_instance_methods", "constants", "global_variables")),
    ("sum-block", "Array#sum ignores its block ([1, 2, 3].sum { |x| x * 10 } is 6; rc2 60)",
     lambda r: r["name"] == "sum" and r["blk"] and r["cls"] == "v"),
    ("split-empty", "''.split(sep, -1) is [''] (rc2 and CRuby: [])",
     lambda r: r["name"] in ("split", "__split") and r["rk"] == "String:empty" and r["cls"] == "v"),
    # --- the rules below were written in the second pass (the automatic groups of 2 calls and more)
    ("bigint-to-float", "an Integer past 64 bits where a Float is wanted: rc2 converts it (Math.cos(2**70), "
     "Float#remainder, Complex.polar), SabiRuby raises 'can't convert Integer into Float'; and Integer "
     "past 64 bits with a Float in &, |, ^, allbits?: rc2 truncates the Float, SabiRuby 'can't convert "
     "Float into Float'",
     lambda r: "can't convert" in r["S"][2] and (5 in r["args"] or r["rk"] == "Integer:big")
     and not (r["R"][0] == "raise" and r["R"][1] == "TypeError")),
    ("math-nan-domain", "Math.sqrt / log10 / acos ... of NaN: rc2 answers NaN, SabiRuby raises Math::DomainError",
     lambda r: r["rk"] == "self:Math" and 7 in r["args"] and _raise(r["S"], "Math::DomainError")
     and r["R"][0] == "ok"),
    ("visibility-args", "public/private/protected/module_function (:sym, 0): rc2 takes the names one by one and "
     "fails on the first missing method (NameError), SabiRuby checks every argument's type first (TypeError)",
     lambda r: r["name"] in ("public", "private", "protected", "module_function") and r["cls"] == "e"),
    ("int-div-nonfinite", "Integer#div / ceildiv by NaN, Infinity or 2**70: rc2 raises RangeError "
     "(float NaN out of range / integer out of range), SabiRuby answers (0, -1; CRuby: FloatDomainError for "
     "NaN, -1 for (-3).div(2**70))",
     lambda r: r["name"] in ("div", "ceildiv") and r["cls"] == "e" and _raise(r["R"], "RangeError")),
    ("regexp-match-nil", "Regexp#match(nil, pos): rc2 converts pos before it looks at the nil, SabiRuby answers nil",
     lambda r: r["rk"].startswith("Regexp") and r["name"] == "match" and r["args"][:1] == [0]
     and r["S"][:2] == ["ok", "nil"]),
    ("merge-partial", "Hash#merge! / update with a later argument that is no Hash: rc2 has merged the earlier "
     "ones when it raises (as CRuby), SabiRuby checks them all first and leaves the receiver as it was",
     lambda r: r["name"] in ("merge!", "update") and r["cls"] == "v" and r["R"][0] == "raise"),
    ("fill-empty-range", "[].fill(1..3) (a Range past the end of an empty Array): rc2 says 'array size too big' and "
     "hands the block an uninitialised value (an rc2 bug: [].__fill_parse_arg answers garbage); SabiRuby "
     "answers [] (CRuby [nil, ...] filled)",
     lambda r: r["name"] == "fill" and r["rk"] == "Array:empty"),
    ("set-reinit", "Set#initialize on a live Set: rc2 'already initialized set' (RuntimeError), SabiRuby starts it again",
     lambda r: r["rk"].startswith("Set") and r["name"] == "initialize"),
    ("set-uninit", "Set.new (no argument) | other: rc2 says 'uninitialized Set', SabiRuby answers the union",
     lambda r: r["rk"].startswith("Set") and "uninitialized Set" in r["R"][2]),
    ("proc-init-copy", "Proc#initialize_copy(non-Proc): rc2 ArgumentError 'not a proc', SabiRuby TypeError",
     lambda r: r["rk"].startswith("Proc") and r["name"] == "initialize_copy"),
    ("rational-minus", "Rational#- with a value that is no number: rc2 TypeError (cannot be converted to Float), "
     "SabiRuby calls the argument's own - (NoMethodError) or coerces the other way",
     lambda r: r["rk"].startswith("Rational") and r["name"] == "-" and r["cls"] in ("e", "m")),
    ("lenient-arg", "rc2 converts an argument SabiRuby does not look at and raises TypeError: "
     "Hash#initialize_copy(0), String#chomp(nil) (CRuby answers the String), Hash#initialize_copy(nil)",
     lambda r: r["cls"] == "e" and _raise(r["R"], "TypeError") and "converted to" in r["R"][2]
     and r["S"][0] == "ok"),
    ("struct-const-name", "Struct.new('abc'): the NameError writes the name with quotes on SabiRuby, without on rc2",
     lambda r: r["cls"] == "m" and "needs to be constant" in r["R"][2]),
    ("class-new-super", "Class.new(non-Class): rc2 'X is not class/module', SabiRuby 'superclass must be a Class "
     "(X given)' (CRuby's wording)",
     lambda r: r["cls"] == "m" and r["rk"] == "self:Class" and "superclass must be" in r["S"][2]),
    ("conv-msg-other", "another wording of a TypeError for a value that cannot be converted ('can't convert X "
     "into T', 'X cannot be converted to T', 'X can't be coerced into T', 'expected T', 'wrong argument "
     "type nil' / 'NilClass')", lambda r: r["cls"] == "m" and r["R"][1] == "TypeError" and
     all(re.search(r"can't convert|cannot be convert|can't be coerced|expected|wrong argument type", x)
         for x in (r["R"][2], r["S"][2]))),
    ("msg-other", "the same exception class, another wording, in no group above (read one by one in x1-causes.tsv)",
     lambda r: r["cls"] == "m"),
]


def internal(name):
    return name.startswith("__") and name not in ("__send__", "__id__", "__method__")


def auto_key(r):
    R, S = r["R"], r["S"]
    if r["cls"] == "v":
        return f"v {r['rk'].split(':')[0]}#{r['name']}"
    a = f"{R[0]} {R[1]}: {template(R[2])}" if R[0] == "raise" else f"{R[0]}"
    b = f"{S[0]} {S[1]}: {template(S[2])}" if S[0] == "raise" else f"{S[0]}"
    return f"{r['cls']} rc2 {a} / sabiruby {b}"


def cause(r):
    for cid, _, pred in RULES:
        try:
            if pred(r):
                return cid
        except (IndexError, KeyError):
            pass
    return "auto: " + auto_key(r)


DESCRIPTIONS = {cid: d for cid, d, _ in RULES}


# The verdict under docs/design/compatibility.md, the size of a fix and whether the book's text is
# touched (grep of book_mruby3/contents/*.re and data/code), one line per cause; the reasons are
# in docs/verification/rc2-sweep/README.md. Verdicts: A follow rc2 (value or exception class),
# B wording only, C rc2 is clearly wrong (keep SabiRuby, record), D recorded already or decided by
# how each VM is built, ? the author decides. Size: S small (one method or one helper), M medium
# (one mechanism used by many methods), - nothing to fix.
VERDICTS = {
    "crash": ("A", "S", ""),
    "timeout": ("D", "-", ""),
    "internal-helper": ("D", "-", ""),
    "argc-splat": ("C", "-", ""),
    "conv-float-msg": ("B", "M", "corelib.re:442, extension.re:474, data/code/corelib_error_messages.rb"),
    "module-required": ("B", "S", ""),
    "bytesplice": ("A", "M", ""),
    "reinitialize": ("A", "S", ""),
    "argc-wide": ("A", "S", ""),
    "conv-int-msg": ("B", "M", ""),
    "argc-narrow": ("A", "S", ""),
    "frozen-set": ("C", "-", ""),
    "frozen": ("A", "S", ""),
    "fiber-inspect": ("A", "S", ""),
    "join-empty": ("A", "S", ""),
    "method-missing-name": ("A", "S", ""),
    "set-order": ("D", "-", ""),
    "binding-inspect": ("A", "S", ""),
    "not-a-symbol": ("B", "S", ""),
    "type-nan-name": ("B", "S", ""),
    "kw-dropped": ("?", "M", ""),
    "conv-msg-other": ("B", "S", ""),
    "conv-order": ("A", "S", ""),
    "visibility-args": ("A", "S", ""),
    "merge-partial": ("A", "S", ""),
    "bigint-to-float": ("A", "M", ""),
    "method-lists": ("D", "-", "corelib.re:133-155 (Kernel.instance_methods(false).size)"),
    "lenient-arg": ("A", "S", ""),
    "set-reinit": ("A", "S", ""),
    "time-year": ("A", "S", ""),
    "fill-empty-range": ("?", "S", ""),
    "proc-init-copy": ("A", "S", ""),
    "regexp-match-nil": ("A", "S", ""),
    "sum-block": ("A", "S", ""),
    "rational-minus": ("A", "S", ""),
    "gc-setter": ("D", "-", "gc.re:716-789, corelib.re:141 (GC.generational_mode)"),
    "class-new-super": ("B", "S", ""),
    "int-div-nonfinite": ("?", "S", ""),
    "set-uninit": ("C", "-", ""),
    "struct-const-name": ("B", "S", ""),
    "msg-other": ("B", "S", ""),
    "bigint-float": ("?", "S", ""),
    "float-ulp": ("D", "-", ""),
    "math-nan-domain": ("A", "S", ""),
    "int-div-float": ("C", "-", ""),
    "unknown-keyword": ("B", "S", ""),
    "enum-args-kw": ("A", "S", ""),
    "negative-zero": ("A", "S", ""),
    "split-empty": ("A", "S", ""),
    "data-init": ("B", "S", ""),
    "time-now": ("D", "-", ""),
}


def verdict(cid):
    """(verdict, size, book) of a cause; a recorded difference (known: ...) is D, an automatic group ''."""
    if cid.startswith("known: "):
        return ("D", "-", "")
    return VERDICTS.get(cid, ("", "", ""))
