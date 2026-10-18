# Every difference from mruby 4.1.0-rc2, by cause (2026-10-18)

A survey only: no code of the VM was changed for it. Plan:
[`../../plans/rc2-sweep-plan.md`](../../plans/rc2-sweep-plan.md); the record of how it was done, what
went wrong and what was decided: [`../../worklog/2026-10-18-rc2-sweep.md`](../../worklog/2026-10-18-rc2-sweep.md)
(Japanese). SabiRuby main `f009316` (`target/release/sabiruby`) against Docker
`kishima/mruby:4.1.0-rc2-utf8` (the reference built with `MRB_UTF8_STRING`, the reading SabiRuby's
default build has); CRuby 3.2.6 where a column says CRuby.

- **X1** calls every method both VMs have on 86 receivers with fixed arguments (0–3 of 22 values,
  a keyword `k: 1`, a block) and compares the value, the receiver afterwards, what the block was
  given, the exception class and its message.
- **X2** runs 502 Ruby programs on both (the book's `data/code`, `tests/custom`, the porting kit's
  fixtures and samples, mruby's own tests assertion by assertion) and compares stdout, stderr,
  the exit code and each assertion.

Verdicts follow [`../../design/compatibility.md`](../../design/compatibility.md):
**A** follow rc2 (a value or an exception class differs); **B** only the wording differs;
**C** rc2 is clearly wrong (undefined behaviour, inconsistent within mruby, or fixed on master), keep
SabiRuby's answer and record it; **D** recorded already (k), or decided by how each VM is built
(hash values, addresses, the gems in the build, the C library's last digit); **?** the author
decides (the reason is in the row).

## Summary for the author

**Dangerous first.**

- **A Rust panic**: `"abc".bytesplice(0, 1)` — `String#bytesplice` with two numbers, on any string —
  stops the process (`src/builtins/string.rs:541`, index out of bounds). rc2 raises TypeError. 51
  calls, cause `crash` (A, small). The `sabiruby` command keeps its output until the end, so the lines
  printed before the panic are lost too (worklog, 気づいた点).
- **A frozen class takes a constant**: `class A; end; A.freeze; A::Y = 2` assigns on SabiRuby;
  rc2 and CRuby raise FrozenError (`OP_SETMCNST`). The book says SETMCNST checks it as SETCONST does
  (`opcodes.re:2056`) and has the program (`data/code/exp_GETCONST_frozen.rb`). Cause
  `setmcnst-frozen` (A, small; found by X2).
- **A `def` with no class to go to**: `1.instance_eval { def x; end }` defines a method on SabiRuby;
  rc2 says TypeError "no class/module to add method" (`opcodes.re:5511`,
  `data/code/exp_TDEF_target_class.rb`). Cause `tdef-no-target` (A; X2).
- **Never ends where rc2 stops**: `(1..).drop(0)`, `grep`, `grep_v` run until the time limit on SabiRuby
  while rc2 says "array size too big" at once (CRuby runs forever too). D, but on a host without a
  memory limit it grows until the allocation fails. Cause `timeout`.

**By verdict** (X1: calls are combinations of receiver, method and arguments, not separate bugs —
one wrong `inspect` of a receiver shows up after every call on it, see `fiber-inspect` and
`binding-inspect`; "methods" is the number of distinct Class#method):

| verdict | X1 causes | X1 calls | X1 methods | X2-only causes |
|---|---:|---:|---:|---:|
| A follow rc2 | 25 | 20,954 | 112 | 3 (`string-add-redefine`, `setmcnst-frozen`, `tdef-no-target`) |
| B wording | 11 | 15,693 | 119 | 1 (`eval-error-line`) |
| C keep SabiRuby, record | 4 | 13,335 | 24 | 1 (`eval-scope-custom-only`) |
| ? the author | 4 | 853 | 36 | 1 (`method-owner`) |
| D build / design | 7 | 16,332 | 52 | 5 (`gem-set`, `one-side-methods`, `gc-numbers`, `list-order`, `harness`) |
| D recorded (k) | 10 | 1,292 | 37 | (custom tests and mruby's tests, below) |
| not classified | 31 groups | 124 | 32 | — |
| **all** | | **68,583** | | |

**Size of the fixes** (S: one method or one helper; M: one mechanism many methods go through):
A — 23 S, 2 M (`bytesplice`, `bigint-to-float`), plus the 3 X2 ones (S; `string-add-redefine` and
`tdef-no-target` are in the VM's opcodes); B — 9 S, 2 M (`conv-float-msg`, `conv-int-msg`: the
conversion messages of `convert.rs` and the numeric methods); ? — 3 S, 1 M (`kw-dropped`: how a
keyword Hash reaches a native that takes none). C and D need no code (C needs the record:
README "Known deviations" and a `tests/custom` case).

**Where the book's text disagrees with SabiRuby** (grep of `book_mruby3/contents/*.re` and
`data/code`; the book describes rc2, so these are places where SabiRuby cannot be used to check it):
`conv-float-msg` (corelib.re:442, extension.re:474 print "String cannot be converted to Float"),
`string-add-redefine` (overview.re:446-451: redefining `String#+` is not seen), `setmcnst-frozen`
(opcodes.re:2056), `tdef-no-target` (opcodes.re:5511), `one-side-methods` (corelib.re:381: "no
`full_message`"; corelib.re:133-155 the count of Kernel's methods), `method-owner` (corelib.re:137,
349: `between?` is Comparable's), `gc-setter` and `gc-numbers` (gc.re, corelib.re:141: the
generational mode and the numbers `GC.stat` gives).

**For the author to decide** (the `?` rows, reasons in the tables):
`kw-dropped`, `fill-empty-range`, `int-div-nonfinite`, `bigint-float`, `method-owner`.

**Where the classifying stopped.** After two passes over the automatic groups (largest first, each
rule checked on a one-line program on both VMs), the largest group left has 22 calls; the 31 groups
left hold 124 calls, 0.18% of the 68,583 that differ — below the "a few percent" the work was asked
to reach, so they are listed in `x1-causes.tsv` as `auto:` rows with their counts and not read one by
one. Not compared at all: 4,345 calls of the receiver `(1..)` (`zip`, `reduce`, `take` …), whose
re-run was cut off with the session; most of them never end, and at 5 s a call a re-run could take
six hours, so it was not repeated (worklog).

## Tools

`tools/rc2_sweep/` (Docker for the reference side; outputs under `target/rc2-sweep/`, not in git):

```
cargo build --release -p sabiruby-cli
python3 tools/rc2_sweep/sweep.py lists                # method lists from both VMs
python3 tools/rc2_sweep/sweep.py cases                # the call table and its size
python3 tools/rc2_sweep/sweep.py run --timeout 5 --only '^(?!Range:endless$)'
                                                      # all calls but (1..)'s (--only REGEX: some receivers)
python3 tools/rc2_sweep/compare.py                    # classify; writes the x1-* files here
python3 tools/rc2_sweep/programs.py run --timeout 60  # X2 on both VMs
python3 tools/rc2_sweep/programs.py compare           # writes x2-programs.tsv, x2-asserts.tsv
python3 tools/rc2_sweep/compare.py diff               # against the tables committed at HEAD
```

**The reference** is `--image` (`sweep.py lists` and `run`, `programs.py run`; default
`kishima/mruby:4.1.0-rc2-utf8`) and `--bytes-image` (`programs.py run`, the tests/custom cases
without `# utf8-only:`; default `kishima/mruby:4.1.0-rc2`). For 4.1.0 or master, pass that image's
name to `lists` and `run` (the same one to both) and to `programs.py run`; nothing else names it.

**Against the last run**: `compare.py diff [--base REV|DIR]` reads the tables as `compare.py` and
`programs.py compare` have just written them and the ones committed at `REV` (default `HEAD`), or
in a directory. It prints, for X1, the calls of each cause up or down (v/e/m/k too), the causes
that are new and the ones that are gone, the totals of `x1-counts.md` and the `x1-methods.tsv`
rows that changed; for X2, the `x2-programs.tsv` and `x2-asserts.tsv` rows added, removed or
changed. It ends with `no change` (exit 0) or `changed` (exit 1). The example columns are not
compared: rc2's answer there can be an address or uninitialised memory that changes every run.

**What moves between runs** with no change on either side (two runs on main `e70ed76`, the
worklog's Y0): `Time.new`/`Time#initialize` without arguments (cause `time-now`, 2 calls) when
the two sides' calls fall on either side of a second; the output length of
`book/vm_task_sleep_switching.rb`, which prints wall-clock milliseconds. A `diff` that shows only
these has not changed.

**`(1..)`** is left out of `run`: of its 16,388 calls, 4,345 never had an answer (they do not end
and take the 5 s limit each, hours in all), and the ones that had come from the 2026-10-18 survey.
Keep that receiver's `target/rc2-sweep/raw/{ref,sabi}/Range_endless.tsv` from an earlier run, or
`compare.py` counts its calls as without an answer.

**When to run it**: at the end of a stage of `plans/rc2-sweep-fixes-plan.md` (and of later work that
changes what methods answer), and when following a new version of mruby (with its image as
`--image`). Not on every commit: it needs Docker and each run writes the tables in git. Time on the
24-CPU machine beside other work (2026-10-18, 2 runs each): `lists` 1 s, `cases` 8 s, X1 `run`
72–75 s, `compare.py` 8 s; X2 `run` 72 s (60 s of it is waiting for `test_NOP_empty_loop.rb`,
which does not end, to reach the limit), `programs.py compare` a few seconds. About 3 minutes in all.
The tool's numbers (the limits, the processes at a time, the memory) are in
[`../../numbers.md`](../../numbers.md).

`causes.py` holds the rules that give each differing call its cause (first rule that takes it
wins), and the verdict, size and book reference of each cause. Changing a rule needs only
`compare.py` again (seconds), not a re-run.

| file | what |
|---|---|
| `x1-counts.md` | counts by class (v/e/m/k) and the 40 largest causes (generated) |
| `x1-causes.tsv` | one row per cause: verdict, size, book, description, counts, an example with both answers, the methods (generated) |
| `x1-methods.tsv` | one row per Class#method that differs, with its main cause (generated) |
| `x1-one-side.tsv`, `x1-one-side-callable.tsv` | methods one VM has and the other does not, by owner and by receiver |
| `x1-left-out.tsv` | the (receiver, method) pairs not called (they stop, leave the process, read files, eval, print) |
| `x2-programs.tsv`, `x2-asserts.tsv` | X2 per program and per differing assertion (generated) |
| `x2-causes.tsv` | X2's differences by cause (written by hand) |
| `target/rc2-sweep/x1-diffs.tsv` | every differing call (16 MB, not kept) |

## Counts

X1: 977,521 calls compared (4,345 without an answer on a side, above), 908,938 the same.

| class | v value | e exception class | m message | k recorded |
|---|---:|---:|---:|---:|
| **all** | 4,866 | 36,166 | 26,259 | 1,292 |
| String | 256 | 17,993 | 3,188 | 10 |
| Range | 12 | 4,618 | 14 | 11 |
| Array | 300 | 3,244 | 4,688 | 1,030 |
| Module | 4 | 1,624 | 2,216 | 0 |
| Class | 76 | 1,604 | 2,171 | 0 |
| Math (singleton) | 12 | 156 | 3,331 | 0 |
| Set | 907 | 1,968 | 0 | 4 |
| Integer | 215 | 148 | 2,078 | 170 |
| Object | 250 | 884 | 1,592 | 4 |
| Struct | 6 | 344 | 1,760 | 4 |
| Binding | 790 | 0 | 872 | 0 |
| Hash | 266 | 1,420 | 40 | 9 |
| Float | 176 | 8 | 1,304 | 8 |
| Regexp | 0 | 266 | 992 | 4 |
| Fiber | 1,068 | 0 | 0 | 3 |
| Data | 0 | 0 | 1,038 | 0 |
| Enumerator | 329 | 680 | 16 | 9 |
| Module (singleton) | 3 | 968 | 0 | 0 |
| others (19) | | | | see `x1-counts.md` |

X2: 502 programs, 424 the same. Differ: book 21 (19 of them also in the kit's samples, the same
files), custom 18, mruby's tests 20 files with 250 assertions (127 the regexp engine, 116 gems the rc2
image does not carry, 7 others).

## Causes

`calls` is X1 (v/e/m in brackets); "X2" marks a cause found only by the programs. Repros were run on
both VMs (and CRuby) on 2026-10-18; a call written with `__send__` reaches a private method.

### A — follow rc2

| cause | calls | repro | rc2 | SabiRuby | CRuby | size, where | book |
|---|---:|---|---|---|---|---|---|
| `crash` | 51 (e) | `"".bytesplice(0, 0)` | TypeError: Integer cannot be converted to String | **panic** (string.rs:541) | TypeError (expected Range) | S, `string.rs` bytesplice | — |
| `bytesplice` | 4,434 (e 1,511, m 2,923) | `"abc".bytesplice(0, -1)` | TypeError: Integer cannot be converted to String | IndexError: index out of string | TypeError (expected Range) | M, `string.rs`: rc2 reads two arguments as (range, str), converts before the bounds, says "wrong number of arumgnts" (sic) | — |
| `reinitialize` | 3,970 (v 152, e 3,818) | `(1..3).__send__(:initialize, 0, "")`; `[3, 1, 2].__send__(:initialize)` | ArgumentError: bad value for range; `[3, 1, 2]` | NameError: 'initialize' called twice; `[]` | FrozenError; `[]` | S each, `range.rs`, `array.rs`, `string.rs`, Class/Module (`"abc"` → rc2 `""`, `Class#initialize` → rc2 self) | — |
| `argc-wide` | 3,104 (e) | `Class.new.instance_methods(nil, nil)` | ArgumentError (given 2, expected 0..1) | `[]` | ArgumentError | S, the `Aspec` of `instance_methods`, `Module.constants`, … | — |
| `argc-narrow` | 2,619 (e 1,629, m 990) | `[3, 1, 2].sample(nil, nil)` | TypeError: nil cannot be converted to Integer (declared opt 2) | ArgumentError (given 2, expected 0..1) | ArgumentError | S, the `Aspec` of `Array#sample`, `Class#initialize` (rc2 opt 1), `Data#initialize` | — |
| `frozen` | 1,136 (e) | `[3, 1, 2].freeze.delete(nil)` | nil | FrozenError | nil | S, where the frozen check stands (`delete`, `chomp!`, `merge!`, `Set#replace`, …) | — |
| `fiber-inspect` | 1,068 (v) | `Fiber.new { }.inspect` | `#<Fiber:0x… q.rb:24 (created)>` | `#<Fiber:0x… (created)>` | as rc2 | S, `fiber.rs` (one method; the count is the receiver's inspect after every call) | — |
| `join-empty` | 1,036 (v) | `[[], 1].join(", ")` | `", 1"` | `"1"` | `", 1"` | S, `array.rs` join (also seen in `Enumerator#inspect` of `[]` arguments) | — |
| `method-missing-name` | 946 (e 766, m 180) | `Object.new.__send__(:method_missing, 0)` | TypeError: 0 is not a symbol nor a string | NoMethodError: undefined method '?' | ArgumentError | S | — |
| `binding-inspect` | 854 (v 798, m 56) | `binding.inspect` | `#<Binding:0x…>` | `#<Binding:0x… proc=…, env=…, recv=main, pc=9>` | as rc2 | S, `ext_binding.rs` (one method, counted through the receiver) | — |
| `conv-order` | 420 (e) | `(2**70).pow(2**70)` | TypeError: too big power | RangeError: integer out of range | Infinity | S, the order arguments are converted in (`Integer#pow`, bit operations with a wide Integer, `Regexp#match`, `caller`, `Math.ldexp`) | — |
| `visibility-args` | 301 (e) | `Class.new.__send__(:public, :sym, 0)` | NameError: undefined method 'sym' | TypeError: wrong argument type Integer (expected Symbol) | NameError | S, public/private/protected/module_function take names one by one | — |
| `merge-partial` | 172 (v) | `h = {}; (h.merge!({a: 1}, nil) rescue nil); h` | `{a: 1}` | `{}` | `{a: 1}` | S, `hash.rs` | — |
| `bigint-to-float` | 152 (e) | `Math.cos(2**70)` | 0.060314849224819785 | TypeError: can't convert Integer into Float | as rc2 | M, the Integer→Float conversion of `Math`, `Float#remainder`, `Complex.polar`; and `(2**70) & 1.5`: rc2 0 (truncates), SabiRuby "can't convert Float into Float", CRuby TypeError | — |
| `lenient-arg` | 129 (e) | `"abc".chomp(nil)`; `{}.__send__(:initialize_copy, 0)` | TypeError; TypeError | `"abc"`; `{}` | `"abc"`; TypeError | S, `string.rs`, `hash.rs` | — |
| `set-reinit` | 128 (e) | `Set.new.__send__(:initialize)` | RuntimeError: already initialized set | `Set[]` | nil | S, `ext_set.rs` | — |
| `time-year` | 94 (v) | `Time.gm(-1).inspect` | `"-1-01-01 00:00:00 UTC"` | `"-001-01-01 00:00:00 UTC"` | `"-0001-01-01 …"` | S, `ext_time.rs` (year 0: `0-01-01` / `0000-01-01`) | — |
| `proc-init-copy` | 84 (e) | `proc { }.__send__(:initialize_copy, 0)` | ArgumentError: not a proc | TypeError: initialize_copy should take same class object | as SabiRuby | S, `proc_.rs` | — |
| `regexp-match-nil` | 74 (e) | `/x/.match(nil, "")` | TypeError | nil | TypeError | S, `ext_regexp.rs` | — |
| `sum-block` | 62 (v) | `[1, 2, 3].sum { \|x\| x * 10 }` | 60 | 6 | 60 | S, `array.rs:313` (the block is not called) | — |
| `rational-minus` | 60 (e 48, m 12) | `Rational(1, 2) - ""` | TypeError: String cannot be converted to Float | NoMethodError: undefined method '-' for String | TypeError | S, `ext_rational.rs` | — |
| `math-nan-domain` | 16 (e) | `Math.sqrt(Float::NAN)` | NaN | Math::DomainError | NaN | S, `ext_math.rs` | — |
| `enum-args-kw` | 9 (v) | `"".gsub(k: 1).inspect` | `#<Enumerator: "":gsub({k: 1})>` | `#<Enumerator: "":gsub>` | `gsub(k: 1)` | S | — |
| `negative-zero` | 8 (v) | `(-3).modulo(1.5)` | 0.0 | -0.0 | -0.0 | S, `numeric.rs` | — |
| `split-empty` | 3 (v) | `"".split("abc", -1)` | `[]` | `[""]` | `[]` | S, `string.rs` | — |
| `string-add-redefine` | X2 | `class String; def +(o); "r"; end; end; "a" + "b"` | `"ab"` | `"r"` | `"r"` | S, `vm.rs` (`OP_ADD` for two Strings looks up the redefinition; rc2 does not) | overview.re:446-451, `data/code/overview_diff.rb` |
| `setmcnst-frozen` | X2 | `class A; end; A.freeze; A::Y = 2` | FrozenError | assigns | FrozenError | S, `vm.rs` `OP_SETMCNST` | opcodes.re:2056, `exp_GETCONST_frozen.rb` |
| `tdef-no-target` | X2 | `1.instance_eval { def x; end }` | TypeError: no class/module to add method | defines it | TypeError: can't define singleton | S, `vm.rs` (`def`, `alias`, `undef` in `instance_eval` of a value that has no singleton class) | opcodes.re:5511, `exp_TDEF_target_class.rb` |

### B — wording only

| cause | calls | repro | rc2 | SabiRuby | CRuby | size, where | book |
|---|---:|---|---|---|---|---|---|
| `conv-float-msg` | 5,648 | `0.quo("")` | String cannot be converted to Float | can't convert String into Float | String can't be coerced into Rational | M, `convert.rs` and the numeric methods (52) | corelib.re:442, extension.re:474, `corelib_error_messages.rb` |
| `module-required` | 5,175 | `Object.new.is_a?(0)` | 0 is not class/module | class or module required | as SabiRuby | S (10 methods) | — |
| `conv-int-msg` | 2,781 | `0.div("")` | String cannot be converted to Integer | can't convert String into Float (names Float) | String can't be coerced into Integer | M (18 methods) | — |
| `not-a-symbol` | 816 | `binding.local_variable_get(nil)` | nil is not a symbol nor a string | NilClass is not a symbol nor a string | — | S | — |
| `type-nan-name` | 707 | `[].product(Float::NAN)` | wrong argument type Float (expected Array) | wrong argument type NaN (expected Array) | no implicit conversion of Float into Array | S | — |
| `conv-msg-other` | 452 | `0.0.quo(nil)` | can't convert nil into Float | NilClass can't be coerced into Float | nil can't be coerced into Float | S (`1 + nil` is the same on both) | — |
| `class-new-super` | 45 | `Class.new(0)` | 0 is not class/module | superclass must be a Class (0 given) | superclass must be an instance of Class … | S | — |
| `struct-const-name` | 30 | `Struct.new("abc")` | identifier abc needs to be constant | identifier "abc" needs to be constant | as rc2 | S | — |
| `msg-other` | 26 | `Rational(1, 2) / 0` | divided by 0 | divided by 0 in rational | divided by 0 | S (also `caller`'s RangeErrors, `lazy.sort`'s SystemStackError) | — |
| `unknown-keyword` | 11 | `[].sample(k: 1)` | unknown keyword: k | unknown keyword: :k | as SabiRuby | S (natives only; a Ruby method says `k` on both) | — |
| `data-init` | 2 | `D = Data.define(:x, :y); D.new(x: 1, y: 2).__send__(:initialize)` | missing keyword: x | missing keywords: :x, :y | FrozenError | S | — |
| `eval-error-line` | X2 | `eval("alias $a $b")` | SyntaxError … line 0 … | … line 1 …, and the generator error also written to stderr | — | S, the compiler crate's eval. Y4: fixed. The stderr half by `quiet_errors`; the line is the first diagnostic's (0 for a generator error, the line within the string for a syntax error, not the `eval` line argument), handed back by `Host::compile_eval` (the author's choice of a default method, worklog Y4) | `overview_diff.rb` |

### C — rc2 is clearly wrong: keep SabiRuby, record (none is recorded yet)

| cause | calls | repro | rc2 | SabiRuby | CRuby | why it is a defect |
|---|---:|---|---|---|---|---|
| `argc-splat` | 12,042 (e) | `"abc".chomp(*[nil, nil])` | TypeError: nil cannot be converted to String | ArgumentError (given 2, expected 0..1) | ArgumentError | inconsistent: rc2 declares `chomp`, `chomp!`, `to_i`, `alias_method` MRB_ARGS_ANY and counts in `mrb_get_args`, which converts the first argument before counting when the arguments come through a splat; the literal call `"abc".chomp(nil, nil)` is ArgumentError on rc2 too. The sweep calls `__send__(m, *a)`, so this cause is large |
| `frozen-set` | 1,251 (e) | `Set[1].freeze << 2` | `Set[1, 2]` | FrozenError | FrozenError | a frozen object changes (mruby-set's mutators do not check; every other collection does) |
| `set-uninit` | 30 (e) | `Set.new \| [1, 2]` | RuntimeError: uninitialized Set | `Set[1, 2]` | `{1, 2}` | an empty `Set.new` refuses `\|`/`union`/`+` until something is added (after `add(1)` it works) |
| `int-div-float` | 12 (v) | `7.div(1.5)` | 7 | 4 | 4 | the Float is truncated to 1 before dividing (`(-3).div(1.5)` is -3); the same slip for a wide receiver is already recorded (`docs/design/gems.md`, mruby-bigint) |
| `eval-scope-custom-only` | X2 | `tests/custom/eval_class_eval_scope.rb`, `eval_outer_scope.rb` | `def` after a string `class_eval` lands in the class; `eval "x"` in a method is a SyntaxError | as CRuby | — | already kept (master 300cc9532 fixed the first), but recorded only in the tests' headers, not in README "Known deviations" |
| `exc-copy` (found in Y4; C by the policy, 2026-10-18) | not swept | `RuntimeError.new("a").dup.message`; `e.exception("b").backtrace` of a raised `e` | "RuntimeError"; nil | "a"; the backtrace | "a"; the backtrace | a value silently loses its contents: rc2's `init_copy` (src/class.c) copies an exception's instance variables only, not the message and the backtrace it keeps in `RException`, so `dup`, `clone` and `exception(msg)` lose them |
| `stack-error-gc` (found in Y4; C by the policy, 2026-10-18) | 2 (m, in `msg-other`) | `(1..3).lazy.sort` | SystemStackError: SystemStackError | SystemStackError: stack level too deep | — | the answer depends on the collector's state: rc2's SystemStackError is one preallocated object whose message the collector clears (`clear_error_object`, src/gc.c) once a GC has run; `def f; f; end; f` says "stack level too deep" on rc2 too |

### ? — the author decides

| cause | calls | repro | rc2 | SabiRuby | CRuby | why it is not decided here |
|---|---:|---|---|---|---|---|
| `kw-dropped` | 706 (v 47, e 490, m 169) | `[1].first(k: 1)` | 1 (the keyword Hash is dropped) | TypeError: Hash cannot be converted to Integer | TypeError | the policy says A, but the author kept `[].push(1, k: 2)` as `[1, {k: 2}]` (README) and this is the same root in 26 methods (`first`, `[]`, `[]=`, `to_s`, `pow`, `Struct#a`, `Method#call` …). README gives as the reason that mruby otherwise hands the keyword Hash on as a positional argument; these calls show rc2 dropping it. M, `vm.rs` (how a keyword Hash reaches a native that takes none) |
| `fill-empty-range` | 87 (e) | `[].fill(0, 1..3)` | `[nil, nil, nil, 0, 0, 0, 0, 0, 0, 0]` (and "array size too big" with a block, which is given uninitialised values) | `[]` | `[nil, 0, 0, 0]` | rc2 reads memory it never set (C: undefined behaviour), but SabiRuby's `[]` is not CRuby's answer either; taking CRuby's is a fix (S, `array.rs` fill) plus a record |
| `int-div-nonfinite` | 36 (e) | `0.div(Float::NAN)`; `(-3).div(2**70)` | RangeError: float NaN out of range; RangeError: integer out of range | 0; -1 | FloatDomainError; -1 | two halves: NaN/Infinity is A (SabiRuby's 0 is nobody's answer); a wide divisor looks like C (rc2 cannot divide by it, CRuby -1). S, `numeric.rs` |
| `bigint-float` | 24 (v) | `0.quo(2**70)` | 0.0 | (0/1) | (0/1) | rc2 answers a Rational for a small divisor (`1.quo(2)` is `(1/2)`) and a Float for a wide one; inconsistent (C), or mruby-bigint's design (A). rc2 also answers `(2**70).quo(3)` with `(0/1)` (not in the sweep's values) |
| `method-owner` | X2 | `1.method(:between?).owner` | Comparable | Numeric | Comparable | the owner differences were tabled in rc2-leftovers9 V1-1 and not decided; the book prints this one (corelib.re:137, 349) |

### D — decided by how each VM is built, or recorded already

| cause | calls | repro | rc2 | SabiRuby | note |
|---|---:|---|---|---|---|
| `internal-helper` | 15,178 | `"".__split(0)` | TypeError: expected String | TypeError: Integer cannot be converted to String | helpers named `__…` (20) that a Ruby-written method calls with arguments it makes itself; the public methods are swept on their own |
| `set-order` | 900 (v) | `Set[3, 1, 2].add_all(0, -1)` | `Set[0, 3, -1, 2, 1]` | `Set[3, 1, 2, 0, -1]` | rc2's table does not keep the order elements were added in; SabiRuby keeps it, as CRuby does |
| `method-lists` | 150 (v) | `Module.constants.size` | 97 | 75 | the gems each build carries (IO, Socket, Errno::* on rc2; Task, CMath on SabiRuby); book corelib.re:133-155 |
| `gc-setter` | 54 (v) | `GC.generational_mode`; `GC.__send__(:interval_ratio=, 100)` | true; nil | false; 100 | SabiRuby's collector is not generational (`../../design/gc.md`); book gc.re:716-789, corelib.re:141 |
| `timeout` | 28 (e) | `(1..).drop(0)` | ArgumentError: array size too big | runs past the limit | rc2 caps an Array's size; CRuby runs forever too. Dangerous on a host without a memory limit (above) |
| `float-ulp` | 20 (v) | `7.pow(1.5)` | 18.520259177452136 | 18.520259177452132 | the C library against `libm`; CRuby as rc2 |
| `time-now` | 2 (v) | `Time.at(0).__send__(:initialize)` | the time now | a second apart | the clock |
| known (10 rules) | 1,292 | — | — | — | README "Known deviations": a Float that stands for no Integer as an index (729) or a `rand` limit (8), `a[i, len] = v` (288), `Integer#to_s` radix (144), `hash` values (90), `Integer#infinite?` (12), `sort`'s pairs (5), `push` with keywords (3), NaN identity (3); `docs/design/gems.md` mruby-bigint (10). The rules are `known()` in `compare.py` |
| `gem-set` | X2 | `Task.new`, `$stdout`, `require`, `MRUBY_PLATFORM`, `strftime`, `sleep`, `CMath` | — | — | gems in one build and not the other: `exp_AREF_redefine.rb` stops on SabiRuby (`$stdout` is nil, no mruby-io), the `vm_task_*`/`task_sample` programs and 116 assertions of mruby's tests need gems the rc2 image does not have; `ObjectSpace` finds 31/27 exception classes |
| `one-side-methods` | X2 | `RuntimeError.new.respond_to?(:full_message)`; `Kernel.instance_methods(false).size` | false; 44 | true; 47 | listed in `x1-one-side.tsv` and `docs/backlog.md`; book corelib.re:381 says mruby has no `full_message` |
| `gc-numbers` | X2 | `GC.stat` | `{live: 882, debt: -143, …}` | `{live: …, debt: 0, generational: 0, full: 0, dynamic_symbol_count: 0}` | the heap and the collector differ; `debt`, `state`, `generational`, `full`, `dynamic_symbol_count` are fixed values (`src/builtins/object.rs:332`), so `gc_symbols.rb` prints 0 where rc2 prints 100; book gc.re:812, 879-884 |
| `list-order` | X2 | `NilClass.instance_methods(false)` | method table order | definition order | X1 sorts such lists before comparing |
| `harness` | X2 | `test_NOP_empty_loop.rb` | killed (137) | killed (-9) | both stop at the time limit on purpose; only the runner's way of writing the kill differs |
| custom tests | X2 | 16 of `tests/custom` | `.rc.out` | `.expected` | each is a recorded deviation (README "Known deviations", `design/gems.md`, `design/utf8.md`), except the two under C |
| mruby's tests | X2 | 134 assertions | pass | fail | the regexp engine (127, `design/gems.md` mruby-regexp), NaN identity (4), the hash pattern whose key moves the subject (1), backticks (2: rc2's image runs the command) |
