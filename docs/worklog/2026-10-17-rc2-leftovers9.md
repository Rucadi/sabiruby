# 2026-10-17 rc2-leftovers9 — rc2-leftovers8 で見つかった 5 件

計画: `../plans/rc2-leftovers9-plan.md`（V1、V2）。base は `e3f57db`。正解は 4.1.0-rc2（Docker
`kishima/mruby:4.1.0-rc2`、ソースは `/home/kishima/book/ref/mruby` の 4.1.0-rc2、HEAD は動かしていない）。
試しのスクリプトはスクラッチパッドの `l9/` に置いた（repo には入れていない）。

## V1-1. rc2 に無いメソッドを消す

### 洗い出し

メソッドの「ある・なし」を、定義されたクラスではなく**呼べるかどうか**で比べた。最初は
`instance_methods(false)` を両方で並べたが、これは持ち主の違い（rc2 は `Integer#<` を
`integer_rom_entries` に、SabiRuby は `Numeric#<` に置く）まで差に出して、本当に呼べないものが埋もれた。
そこで `public_instance_methods(true)`・`private_instance_methods(true)`・`singleton_methods(true)` を
26 のクラス・モジュールで書き出し（`l9/ml2.rb`）、各クラスの一覧から Object の一覧を引いてから両者を
引き算した（`l9/cmp.rb`、CRuby で集計）。Kernel の差が全部のクラスに出るのを消すためである。

数の 4 クラス（Comparable・Numeric・Integer・Float）で SabiRuby にだけあったもの:

| メソッド | SabiRuby の定義 | rc2 |
|---|---|---|
| `Numeric#to_int`（`1.0.to_int`、`Rational(1, 2).to_int` もこれ） | `numeric.rs` の Numeric の表 | Integer にだけある（`integer_rom_entries` の `mrb_obj_itself`） |
| `Float#to_int` | Float の表の `("to_int", float_to_i)` | 無い |
| `Numeric#nan?`（`1.nan?`、`Rational#nan?`） | Numeric の表 | Float にだけある（`flo_nan_p`、`src/numeric.c:2563`） |
| `Numeric#step` | Numeric の表。中身は `NotImplementedError` を投げるだけ | Integer と Float に mrblib（`mrblib/numeric.rb:103,135`） |
| `Integer#pred` | Integer の表 | 無い（mruby-numeric-ext にも無い） |
| `Integer#__num_to_a` | Integer の表。nil を返すだけ | Range のもの（`src/range.c:586`） |
| `Float#__coerce_step_counter` | Float の表。self を返すだけ | Integer にだけある（`src/numeric.c:2528`）。rc2 の `Float#step` は使わない（`i = self`） |

Comparable の差は無かった。rc2 にあって SabiRuby に無いのは `Integer.new`・`Float.new` が
`singleton_methods` に出ること（下の表）だけで、呼べないことは同じ。

消す前の grep: sabiruby の `src`・`tests`・`bench`・`compiler`・`cli`、rubevy、rubevy_games、
sabiruby-playground の `*.rb`・`*.rs`・`*.html`・`*.js` を `\.pred\b|to_int\b|__num_to_a|__coerce_step_counter|nan\?`
で探した。当たったのは Float の `nan?`（mrbtest の `float.rb`、`assert.rb` の `assert_float` — `exp.to_f` の
あとなので Float）、`to_int` を定義する側のテスト（`rc2_leftovers3.rb` など）、playground の
`corelib_mrblib_internal.rb`（`[Integer, :__coerce_step_counter]` — Integer の分は残る）だけで、消すものを
呼ぶ所は無かった。Rust の側も `"to_int"`・`"pred"`・`"nan?"` を名前で呼ぶ所は無い（定義の行だけ）。
book_mruby3 も grep だけした（`corelib.re:91,93` は Range と Integer の分で、残る方）。

### 直し

`src/builtins/numeric.rs` から 6 行を消し、`nan?` を Float の表に移した（`flo_nan_p` とコメント）。
`argc!(vm, a, 0)` は rc2-leftovers8 で付けたものをそのまま持っていった。

確かめ（`l9/n.rb` を両方で）: 消したものは rc2 と同じ NoMethodError の文言になり、`1.0.respond_to?(:to_int)`
が false、`Float.instance_method(:nan?).owner` が Float になった。`1.step(2, 0.5)`、`1.0.step(2.0, 0.5)`、
`(1..3).to_a`、`[1, 2][1.0]` は前と同じ答え。`tests/custom/rc2_leftovers9.rb` の前半がこれで、`.expected` は
`.rc.out` そのまま。

最初の確かめで 1 度「何も変わっていない」と読んだ: `cargo build --release` はワークスペースの根で走らせると
`sabiruby` の lib だけを建て、`target/release/sabiruby`（`sabiruby-cli`）は古いままだった。`-p sabiruby-cli` を
付けて建て直したら差が消えた。

### 数の 4 クラスの外（消していない。backlog へ）

同じ引き算で出た残り。持ち主だけが違うものは除いてある。

| クラス | SabiRuby にだけある | rc2 にだけある | 見立て |
|---|---|---|---|
| Rational | `truncate` | | `ext_rational.rs:281`。rc2 の mruby-rational には無い（`1r.truncate` が NoMethodError） |
| Kernel | `require`、`load`、`__require_*` 6 つ、`__exec_file`、`__file_exist?`、`__load_file`、`__printstr__`（特異も）、`sleep`／`usleep`／`sleep_ms`（`Kernel.sleep` も） | `gets`、`open`、`readline`、`readlines`、`` Kernel.` `` | SabiRuby の require と sleep（`docs/design/gems.md`）、rc2 の mruby-io。意図した差 |
| Module・Class | `public_method_defined?`、`private_method_defined?`、`protected_method_defined?`、`initialize_copy`、`Module.new`（特異） | | rc2 の既定の gembox に `*_method_defined?` は無い |
| String | `__aref`、`__aset` | | `ext_regexp.rs:1141` が `[]`／`[]=` を包むときの元の名前 |
| Symbol | `id2name` | `Symbol.new` | |
| Array | `__ary_cmp`、`__ary_eq`、`__ary_index`、`to_ary` | | `array.rs:276-285`。rc2 の mrblib はこれらを呼ばない |
| Hash | `__update` | | `hash.rs:208` |
| Proc | `()`、`initialize` | | `proc_.rs:36` が `"()"` という名前のメソッドを置いている |
| Exception | `full_message` | | |
| Time | `strftime` | | `ext_strftime.rs`（SabiRuby の gem） |
| Range | `Range.new`（特異） | | 持ち主の違い |
| Integer・Float・NilClass・TrueClass・FalseClass・Symbol | | `.new`（`singleton_methods` に出る） | rc2 の `mrb_undef_class_method` は特異クラスに「undef」の項目を置くので一覧に出る。呼ぶと両方 NoMethodError だが、文言が rc2 は `undefined method 'new' for Class`、SabiRuby は `... for Integer` |

## V1-2. ネイティブの `arity` — 見積もって止めた

### rc2 の規則

`mrb_proc_arity`（`src/proc.c:423`）は、C 関数の Proc では `p->flags` の圧縮した aspec
（`MRB_PROC_CASPEC_MASK`、`include/mruby/proc.h:134`。`mrb_proc_set_cfunc_aspec` が `MRB_ARGS_*` の宣言から作る）
を読み、`ra || op ? -(ma + pa + 1) : ma + pa` を返す。aspec が 0（`MRB_ARGS_NONE`）なら `MRB_PROC_NOARG` が立って
0、どちらも無い（`attr_writer` のように aspec を持たない cfunc）なら -1。`Method#arity`
（mruby-method `method_arity`）はこれをそのまま返す。つまり答えは各ネイティブの `MRB_MT_ENTRY`／
`mrb_define_method` の第 4 引数で決まる。rc2 の VM はこの aspec で引数の数の検査もする（`src/vm.c:1610`
`check_argument_count`、`NOARG` なら 1 つでも渡すと誤り）。

### SabiRuby の今

ネイティブ（`NativeFn`）は宣言を持たない。引数の数は各クロージャの中の `argc!` で見る（391 か所）。
`Method#arity` は `ext_method.rs:119` の `native_arity` が `vm.native_arity`（関数のアドレスと arity の組）を引き、
無ければ -1。その表は `ext_method.rs:279` に手で書いた 37 組だけある（「全部 -1」は正確ではなかった）。

### 差の大きさ

両方で `ObjectSpace.each_object(Module)` の全メソッドの `instance_method(m).arity` を書き出して突き合わせた
（`l9/ar2.rb`）。両方にある 1098 個のうち **612 個が違い、全部 SabiRuby が -1**:

| rc2 | SabiRuby | 数 |
|---|---|---:|
| 0 | -1 | 283 |
| 1 | -1 | 268 |
| -2 | -1 | 36 |
| 2 | -1 | 19 |
| 3 | -1 | 5 |
| -3 | -1 | 1 |

（ここには mrblib の Ruby のメソッドも入るが、そちらは両方とも irep から出すので差に出ていない。）

### 要る変更の見積もり

- ネイティブの定義 約 930（`define_methods` の表の行 864、`define_method` 65）に aspec を持たせる。
  `define_methods(&[(&str, NativeFn)])` を `(&str, Aspec, NativeFn)` にするか、表を別に持つ。値は rc2 の
  `MRB_MT_ENTRY`／`mrb_define_method` から 1 つずつ写す（ROM の表は機械で読めるが、`mrb_define_method` の
  呼び出しと SabiRuby の名前・クラスの対応は手で合わせる所が残る）。
- 宣言を持たせるなら、rc2 と同じく呼ぶ側で数を検査して各 `argc!` を消すのが筋になる。それはネイティブの
  呼び出しの道（`call_native_direct` など）に検査を 1 つ足すことで、`bench_ab` で全体を測る仕事になる。
  arity の答えだけなら `native_arity` の表を 37 組から約 600 組に増やす形でも出せるが、宣言と検査が 2 か所に
  分かれ、`Closure` は別（`NativeClosure::arity`）のまま。
- 計画書の「大きな仕事なら止めて報告」に当たるので、ここで止めた。どちらの形にするかは著者の判断。

## V1-3. 引数の数の誤りの文言

`OP_ENTER` の strict の検査（`src/vm.c:2622-2627`）は `argnum_error(mrb, m1+m2)` で、省略できる引数や
残りの引数があっても必要な数だけを言う。速い道（`src/vm.c:2552-2558`、必須の引数だけ）も `argnum_error(mrb, m1)`。
SabiRuby の `op_enter`（`src/vm.rs`）は CRuby と同じく `1..2`・`1+` を組み立てていた。

直し: 文言の組み立てを `#[cold] #[inline(never)] fn enter_argnum_error(given, required)` に出し、速い道と
遅い道の 2 か所から `m1` と `m1 + m2` を渡す。`format!` はもともと誤りの枝の中にあったので、正しい呼び出しの
道は何も増えていない（枝の中身が関数呼び出しに替わっただけ）。

確かめ（`l9/ae.rb`、36 通り）: 必要な数の部分は rc2 と全部同じになった（`def f(a, b = nil)` に 3 つ → "given 3,
expected 1"、`*r` つき → "expected 1"、`def f(a, b, c = 1, d = 2, e)` に 1 つ → "expected 3"、lambda、
`yield` に lambda を渡したもの、`1.clamp(1, 2, 3)`）。

### 合わせなかった所: "given" の数（著者に聞く）

rc2 の `argnum_error` は "given" を `ci->n`（位置引数の数）から数え、`argc == 0 && ci->kw` でキーワードの
Hash が空でなければ 1 足す（`src/vm.c:2154-2170`）。これで次の 4 つが SabiRuby と違う:

| 呼び出し | rc2 | SabiRuby |
|---|---|---|
| `def f1(a)`; `f1(1, k: 1)` | given 1, expected 1 | given 2, expected 1 |
| `def f5(a, k: 1)`; `f5(k: 2)` | given 1, expected 1 | given 0, expected 1 |
| `def f6(a, b = 1, k: 1)`; `f6(k: 1)` | given 1, expected 1 | given 0, expected 1 |
| `def f8(a, **kw)`; `f8(k: 1)` | given 1, expected 1 | given 0, expected 1 |

1 行目は速い道で、そこは `argc + ci->kw != m1` で数を比べるのに文言では `ci->kw` を数えない（遅い道の
`f2(1, 2, k: 3)` は Hash を位置引数に入れてから比べるので "given 3"）。2〜4 行目は、キーワードを受けるメソッドで
Hash を位置引数に入れないのに "given" に数える。どちらも "given 1, expected 1" と自分で矛盾した文言になる。
方針の「本家の中で食い違う」に当たるか、文言の違い（例外にしない）かが分からないので、著者の判断として残した。
SabiRuby は今の数（CRuby と同じ）のまま。custom にはこの 4 行を入れていない。

## V1-4. 捕まえない例外の CLI の表示

rc2 の `mruby` は `mrb_print_error` → `print_backtrace`（`src/backtrace.c:222`）で標準エラーに書く:

```
trace (most recent call last):
	[2] u1.rb:8
	[1] u1.rb:2:in a
u1.rb:5:in b: boom (ArgumentError)
```

- backtrace が 1 つでもあれば見出しを書き、外側（配列の最後）から 2 番目までを `\t[i] 行`、最初の項目を
  `行: ` で書いてから `mrb_exc_get_output`（`src/error.c:143`）。それは message と `(クラス)`、message が nil か
  空ならクラスだけ（`raise ""` → `RuntimeError`）。
- backtrace が空（`set_backtrace([])`）なら見出しなしで `(unknown):0: `。
- backtrace は例外が持つもの（`set_backtrace` で与えた配列ならそれ）で、`backtrace` を上書きしたメソッドは
  呼ばない（`v5.rb`: `def backtrace; ["x"]; end` は効かない）。
- ファイル名は渡したパスのまま（`sub/u1.rb`、`/w/u1.rb`）、`-e` は `-e:1`、標準入力は `-:1`。`-g` なしで
  作った `.mrb` は `(unknown):0: boom (ArgumentError)`。
- `exit` と `require` は rc2 の既定のビルドに無い（NoMethodError）。

SabiRuby の CLI は `vm.describe_error(&e)`（message とクラスだけ）を書いていた。VM の `describe_error` は
custom のテスト（`<error: ...>`）、`edge_spec`、ホストが使うので触らず、CLI（`cli/src/main.rs`）に
`uncaught_report` を足した。backtrace は Exception のネイティブ `backtrace`（`find_method(core.exception, ...)`
で見つけた `Method::Native` を直接呼ぶ）から取るので、利用者の上書きは見ない。message は内部の `mesg` を読み、
String でなければ `Exception#to_s` と同じく `as_string` で文字にする（rc2 は `exc_initialize` で文字にして持つ。
`RuntimeError.new(:sym)` は両方 `sym (RuntimeError)`）。プログラムの失敗と `-r` の失敗の 2 か所で使う。

確かめ（`l9/ux/` の 21 本、標準エラーだけを比べた）: 18 本が 1 文字も同じ。残りの 3 本:

- `require "nope"`: rc2 には `require` が無い。SabiRuby の require の LoadError で、意図した差。
- `Class.new(StandardError)`: `#<Class:0x…>` のアドレス。
- `1 + nil`（メソッドの中）: rc2 は backtrace の先頭に `u2.rb:3:in +` があり、SabiRuby には無い。CLI ではなく
  VM の `Exception#backtrace` の違いで、`1.+(nil)`、`1.0 * nil`、`1 < nil` も同じ（演算子は `OP_ADD` などで
  呼ばれ、ネイティブの名前が backtrace に入らない）。`1.send(:+, nil)` は rc2 が `in +`、SabiRuby が `in send`。
  既知の 7 つ（`worklog/2026-09-26-release-0.7-b.md` S5「残った違い」、`1.method(:+).call(nil)` が `in call`）と
  同じ根の別の例。VM の仕事なので backlog に足した。

もう 1 つ形の違い: rc2 は標準出力を C の stdio で溜め、終わるときに書くので、`2>&1` でパイプに流すと
`before` が trace の後に来る（端末では前）。SabiRuby は出力を最後にまとめて書いてから標準エラーを書くので、
いつも前。これはバッファの都合なので合わせていない。

`cli/tests/cli.rs` の `errors_look_like_mrubys` の期待を rc2 の出力に替え、入れ子のメソッド、空の message、
`set_backtrace([])`、`set_backtrace([...])` の 4 つを足した（どれも rc2 で出した文字列）。

## V2. `Integer#infinite?`（記録だけ）

rc2: `num_infinite_p`（`src/numeric.c:2454`、Numeric の表）は `return mrb_false_value()`、`flo_infinite_p`
（`:775`）は有限なら nil。master（`origin/master` と 891d1d9d）も同じ。`1.infinite?`、`(2**70).infinite?`、
`Rational(1, 3).infinite?` が rc2 で false、SabiRuby で nil、CRuby 3.2 で nil（ホストの `ruby -e` で確かめた）。
計画どおり nil のまま、方針の記録の形で:

1. README "Known deviations" の「Kept by the author's decision (2026-10-17) where mruby is inconsistent
   within itself」に 1 項。
2. `tests/custom/integer_infinite.rb`: 12 行。`.expected` は SabiRuby の答え（nil）、`.rc.out` に rc2 の false。
   見出しに出どころと rc2 の答えを書いた。
3. 本の notes には書いていない（book_mruby3 には触らない）。書く案は報告に入れた。

## 確かめ

（数値は下の「数値」）

- `cargo test --workspace --release`: 275 passed、0 failed。`cargo test --workspace`（debug、CI と同じ）: 275、0。
- `cargo test -p sabiruby --no-default-features --features "std utf8"`: 161、0。`--features "std regexp"`: 165、0。
- `cargo test -p sabiruby-cli --release`: 13 passed（`errors_look_like_mrubys` に 4 つ足した）。
- `tools/check_no_std.sh --features regexp`／`--features utf8`: no_std OK。`cargo build -p sabiruby --lib --target
  wasm32-unknown-unknown`、`cargo doc --no-deps --workspace --lib`、`cargo check --workspace --all-targets`、
  feature の組 `std`／`std regexp`／`std utf8`: 警告なし。`cargo doc -p sabiruby-cli --bins` は
  「output filename collision」の警告が出るが、base（`e3f57db` の worktree）でも同じに出る。
- `tools/mrbtest.sh` の 3 ビルド: 2346/2508、2269/2453、1981/2012。base と同じ。生成した md は日付の行だけが
  変わったので戻した。
- unsafe は増やしていない（差分の `+` 行に `unsafe` は 0）。

## 数値

ベンチは `tools/bench_ab.sh --runs 5 --core 2`。A = base `e3f57db` を `git worktree add --detach` した所で別の
`CARGO_TARGET_DIR` に建てた release、B = この変更の release、A/A は A の写し。`md5sum` は A `28cfb486…`、
B `c3f91291…`、A の写しは A と同じ。始める前の `vmstat` は idle 99〜100%、隣に sabiruby・cargo・mruby の実行は無し。

| 巡 | 組 | 27 本の合計 | 1 本ずつの幅 |
|---|---|---:|---|
| 1 | A/B | +0.1% | −1.5%（`mem_short_lived`）〜+2.2%（`ds_hash`） |
| 1 | A/A | +0.4% | −1.7%（`ds_hash`）〜+1.9%（`mem_retained`） |
| 2 | A/B | +0.1% | −2.2%（`loop_while_add`）〜+1.5%（`app_tak`） |
| 2 | A/A | −0.2% | −1.4%（`call_fiber`）〜+3.3%（`bm_mandel_term`） |
| 3 | B/A（新を A に） | +0.1% | −2.7%（`vmo_objects`）〜+1.1%（`loop_while_add`） |

A/B（+0.1%、+0.1%、新を A にした巡で −0.1% 相当）は A/A（+0.4%、−0.2%）の幅の中で、ぶれの範囲。呼び出しの
多いもの（`call_args`、`call_kwargs`、`call_block_yield`）も巡ごとに符号が変わる。`OP_ENTER` の正しい呼び出しの
道は変えていない（誤りの枝の中身だけ）ので、そう読むのが合う。

## 残ったこと

- ネイティブの arity（V1-2、上の見積もり）: 著者の判断待ち。
- "given" の数の 4 つ（V1-3）: 著者の判断待ち。
- 演算子のネイティブが backtrace に名前を残さない（V1-4）: backlog。
- 数の 4 クラスの外の差の表（V1-1）: backlog。

## 気づいた点

- **`define_method` の block が strict にならない**（VM、バグ）: rc2 は `define_method(:g) { |a, b = 1| }` を lambda の
  形で走らせ、`C.new.g(1, 2, 3)` は "wrong number of arguments (given 3, expected 1)"、`g()` も誤り、`arity` は -2。
  SabiRuby は `[1, 2]` を返し（余りを捨てる）、`g()` は `[nil, 1]`、`arity` は 1。`define_singleton_method` も同じ。
  `method(:g).to_proc.lambda?` は両方 true なので、Proc の strict の印が呼び出しで使われていない。再現: `l9/dm.rb`
  （`class C; define_method(:g) { |a, b = 1| [a, b] }; end; p C.new.g(1, 2, 3)`）。計画の外なので直していない。
- **ネイティブの arity の表**（VM、`ext_method.rs:279`）: 37 組を手で書いた表があり、計画書の「全部 -1」は正確では
  なかった。その表も `1.5.method(:to_i)` などは持たない。V1-2 の見積もり。
- **"given" の数**（VM、`op_enter`）: V1-3 の 4 行。著者の判断。
- **演算子のネイティブの名前が backtrace に無い**（VM）: V1-4。backlog に足した。
- **`Integer.new` の文言**（VM）: rc2 は `undefined method 'new' for Class`（undef の項目を特異クラスに置き、
  `singleton_methods` にも `:new` が出る）、SabiRuby は `... for Integer`。NilClass・TrueClass・FalseClass・Symbol・
  Float も同じ。合わせるかは決めていない。
- **`Float#-@`・`Integer#-@` が Float・Integer の表にある**（VM、`numeric.rs`）: 呼べることは rc2 と同じだが、
  rc2 で持ち主がどこかは確かめていない（V1-1 の引き算は「呼べるか」だけを見た）。持ち主の違いの一覧（`Numeric#<`
  など）は作っていない。
- **本の素材**: rc2 の `Method#arity` は C 関数でも答えを持つ。`MRB_ARGS_*` の宣言を RProc の空いたビット 13 本に
  詰め（req・opt は 7 まで、post・key は 3 まで、超えると rest に倒す）、`mrb_proc_arity` がそれを広げて読む。
  同じ宣言で VM が引数の数を検査する（`check_argument_count`）ので、宣言と検査がずれない。
