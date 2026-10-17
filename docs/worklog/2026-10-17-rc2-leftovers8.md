# 2026-10-17 rc2-leftovers8: Class の module_function、hex/oct、引数を取らない数のメソッド、sort の順

計画は `../plans/rc2-leftovers8-plan.md`（U1、U2）。元は `2026-10-17-rc2-leftovers7.md` の「気づいた点」。base は
`47279c2`。正解は Docker `kishima/mruby:4.1.0-rc2`、ソースは `/home/kishima/book/ref/mruby`（HEAD は 4.1.0-rc2 の
`c17ffcc24` のまま、動かしていない）。プローブはスクラッチの `l8/p/`。

## U1. rc2 に合わせる 3 件

### Class の module_function

rc2 の `mrb_init_class`（`src/class.c:4761`）は Module の ROM 表を入れた後に
`mrb_undef_method_id(mrb, cls, MRB_SYM(module_function))` を呼ぶ。SabiRuby は `object.rs` の Class の
`inherited` の後に同じ `vm.undef_method(c.class, :module_function)` を置いた。`Vm::undef_method`（`vm.rs:1732`）は
rc2 の `undef_method` と同じく `Method::Undef` を書き込んで `method_undefined` を呼ぶ（起動時なので既定の
何もしない hook）。確かめたこと（プローブ `hex.rb` の後半、rc2 と同じ答え）: `Class.new { module_function }`、
`class_eval { module_function :to_s }`、`send(:module_function)` が "undefined method 'module_function' for
Class"、`respond_to?(:module_function, true)` が Class で false・Module で true、
`Class.private_instance_methods(false)` に無く Module のには有る、モジュールの中の `module_function :y` は今まで
どおり。

### String#hex／oct

rc2 の `str_hex`／`str_oct`（mruby-string-ext `string.c:859-869`）は `mrb_str_to_integer(mrb, self, 16, FALSE)` と
`(…, 8, FALSE)`。rc2-leftovers7 の worklog は oct を「-8（接頭辞を読む）」と書いていたが、ソースは 8 だった。
基数 8 の `mrb_str_len_to_integer` は `0o` だけを接頭辞として読み、`0x` は読まない（`"0xff".oct` は 0）。
SabiRuby の `ext_string.rs` は `string::to_i`（文字ごとに走査して `from_str_radix`）を呼んでいたので、rc2-leftovers7
で `String#to_i` に入れた写し `ext_kernel::str_to_integer`（`ext_kernel.rs:46`）を 16／8、`badcheck = false` で
呼ぶようにした。どちらも `MRB_ARGS_NONE` なので `argc!(vm, a, 0)` も足した。`string::to_i` は使われなくなった
ので消した（dead_code 警告）。

違いがどこに出るかを先に手で探した 46 個の文字列（接頭辞、`_`、符号、空白、NUL、`i64` の境目と超える値）では、
**base の SabiRuby も rc2 と全部同じ答え**だった。そこで `0178afgxXoObBdD_ +-\t` の 21 文字から長さ 0〜6 の
文字列を 3000 個（乱数の種 8）作って `hex`・`oct` を比べた（`l8/p/fz.rb`）。base は 11 個が違い、どれも
**符号の直後の `_`**（`"+_ff".hex` が rc2 で 0、base で 255、`"-_17".oct` が 0 と -15）。rc2 の走査は符号の後の
`_` を数字の前の `_` として止まる（`p == n || s[p] == '_'` で 0）が、base の `to_i` は `_` の前の文字が `_` で
なければ許していた。直した後は 3000 個すべて rc2 と同じ。custom にこの 2 つを足した。

### 引数を取らない Float／Integer のメソッド

`Float#to_i` だけでなく同じ作り（引数を見ない closure）が他にもあるかを、Float・Integer・Numeric・Comparable の
`instance_methods(false)` の全部を引数 0〜3 で呼んで rc2 と比べて探した（`l8/p/num.rb`、副作用のある
`instance_eval` などは外した）。rc2 が "wrong number of arguments (given 1, expected 0)" と言い SabiRuby が通して
いたのは次の 21:

- Float: `-@`、`abs`、`finite?`、`infinite?`、`nan?`、`to_f`、`to_i`、`to_r`、`to_s`
- Integer: `-@`、`~`、`bit_length`、`even?`、`odd?`、`finite?`、`infinite?`（この 2 つは Numeric）、`size`、
  `to_f`、`to_i`、`to_int`、`to_r`

どれも rc2 の表で `MRB_ARGS_NONE`（`src/numeric.c:2472-2563`、mruby-numeric-ext、mruby-rational）。rc2 は
`MRB_ARGS_NONE` のネイティブに引数が来ると呼ぶ前に誤りにする。SabiRuby は closure の頭に `argc!(vm, a, 0)` を
足した（`numeric.rs`、`ext_numeric.rs`、`ext_rational.rs`）。`float_to_i` は `to_i` と SabiRuby だけにある
`Float#to_int` の両方なので、`to_int` も引数を拒むようになった。Numeric の `nan?`（これも SabiRuby だけにある）
も同じ並びで足した。`argc!` は `check_argc` の比較 1 つで、速い道（`OP_*` の Int の腕）には触れていない。直した
後に同じプローブを流すと、残った違いは引数の数ではない 5 件だけ（下の「気づいた点」）。

### tests/custom

`rc2_leftovers8.rb`（98 行、`.expected` = `.rc.out`）。base の SabiRuby で流すと 98 行のうち 31 行が違う
（module_function の 4、hex/oct の引数 2 と `_` の 2、数のメソッドの 23）。`7.infinite?` の値の行だけは、rc2 が
false（`num_infinite_p`）、SabiRuby が nil で違うので、引数なしの呼び出しから外し、テストの中に理由を書いた。
これは計画の外（下の「気づいた点」）。

## U2. sort の組の順と、等しい要素の並ぶ順（記録だけ）

rc2 の `mrb_ary_sort_bang`（`src/array.c:2492`）は Integer だけ・String だけの Array を専用の比較で、それ以外を
`sort_cmp` で並べる。どれも 16 要素まで（`SMALL_ARRAY_SORT_THRESHOLD`、`:2137`）は挿入ソートで、それより多いと
ヒープソート（Floyd の削除）。挿入ソートは安定、ヒープソートは安定でない。SabiRuby の `sort_values`
（`array.rs:82`）はマージソート（安定）。block なしの Integer だけの Array は `sort_unstable` だが、等しい Integer は
同じ値なので見分けられない。

確かめ（`l8/p/stab2.rb`）: `(0...n).map { |i| [i % 3, i] }.sort { |x, y| x[0] <=> y[0] }.map(&:last)` は、n = 8 と
16 で rc2 と SabiRuby が同じ（どちらも安定）、**n = 17 から違う**。

```
17 rc2:       [0, 9, 3, 6, 15, 12, 13, 10, 4, 16, 7, 1, 14, 11, 5, 2, 8]
17 SabiRuby:  [0, 3, 6, 9, 12, 15, 1, 4, 7, 10, 13, 16, 2, 5, 8, 11, 14]
20 rc2:       [18, 12, 6, 0, 3, 9, 15, 13, 10, 19, 4, 1, 16, 7, 14, 11, 5, 2, 17, 8]
```

キーが全部同じ 20 要素（`["k", i]`）では rc2 が `[2, 4, 5, 6, 8, …, 19, 15, 7, 3, 1, 0]`、SabiRuby が元の順。
`sort_by` は rc2 でも mruby-enum-ext の mrblib が `[key, index]` の組を `sort!` するので安定で、両方同じ
（`[0, 3, 6, …]`）。CRuby 3.2 は 10 要素で両方と同じ（小さい Array なので）。

README の Known deviations の `instance_variables` と同じ段落に 1 文（例は 17 要素の頭 3 つ）、backlog の
「著者が決めた本家との差」に 1 行、CHANGELOG に 1 項。

## 確かめ

（数値は下の「数値」）

- `cargo test --workspace --release`: 275 passed、0 failed。`cargo test --workspace`（debug、CI と同じ）: 275、0。
- `cargo test -p sabiruby --no-default-features --features "std utf8"`（CI の regexp なし）: 161、0。
  `--features "std regexp"`（bytes）: 165、0。
- `tools/check_no_std.sh --features regexp`／`--features utf8`: no_std OK。`cargo build -p sabiruby --lib --target
  wasm32-unknown-unknown`、`cargo doc --no-deps --workspace --lib`、`cargo doc -p sabiruby-cli --bins`、
  `cargo check --workspace --all-targets`、feature の組 `std`／`std regexp`／`std utf8`: 警告なし。
- `tools/mrbtest.sh` の 3 ビルド: 2346/2508、2269/2453、1981/2012。base と同じ。生成した md は日付の行だけが
  変わったので戻した。
- unsafe は増やしていない（`src/` の差分に `unsafe` は 0 行）。

## 数値

ベンチは `tools/bench_ab.sh --runs 5 --core 2`。A = base `47279c2` を `git worktree add --detach` した所で別の
`CARGO_TARGET_DIR` に建てた release、B = U1 の release、A/A は A の写し。`md5sum` は A `a3fa2486…`、B
`c4f437ec…`、A の写しは A と同じ。

| 巡 | 組 | 27 本の合計 | 1 本ずつの幅 | 備考 |
|---|---|---:|---|---|
| 1 | A/B | +1.0% | −7.8〜+12.5%（`mem_retained`、`loop_times`、`loop_while_add`） | 途中で custom.sh の docker を回してしまった。読まない |
| 1 | A/A | −0.2% | −8.4〜+3.0% | 同じく揺れている |
| 2 | A/B | +0.2% | −1.6〜+3.5%（`vmo_arith`） | 静か |
| 2 | A/A | −0.1% | −2.8〜+1.9% | 静か |
| 3 | B/A（新を A に） | +0.0% | −1.0〜+3.2% | `vmo_arith` +0.1% |
| 4 | A/B | +0.1% | −1.2〜+2.2% | `vmo_arith` +0.1% |
| 4 | A/A | +0.0% | −1.4〜+2.4% | |

静かな巡の A/B（+0.2%、+0.0%、+0.1%）は A/A（−0.1%、+0.0%）と同じ幅で、ぶれの範囲。2 巡目の `vmo_arith` +3.5% は
3・4 巡目で +0.1% になったので、`argc!` の分ではない（`vmo_arith` の足し算・掛け算は `OP_ADD` などの速い道で、
今回の closure を通らない）。

`tools/edge_compare.sh`（`32d0b15`、dirty: false）: ファイル 30 passed／17 failed／1 runtime_error、assert 859 pass／32 fail／
34 error、mruby 自身の `assert.rb` の列 884 ok／42 ko。どれも前（`f410baf`）と同じ。edge の test/t は master の
もので、今回の直しでは数が動かなかった（どの assert が今回の口を通るかは 1 件ずつ確かめていない）。

## 気づいた点

- **`Integer#infinite?` が rc2 では false**（VM、`numeric.rs` の Numeric の `infinite?`）: rc2 の `num_infinite_p`
  （`src/numeric.c:2473`、Numeric）は false を返し、`flo_infinite_p` は有限なら nil。SabiRuby はどちらも nil（CRuby と
  同じ）。本家の中で Float と Integer が食い違うので、方針の「本家の中で食い違う」に当たるか著者が決める話。
  custom ではこの 1 行を外した。
- **rc2 に無いメソッドが SabiRuby にある**（VM、`numeric.rs:523-524`、`:578` 付近）: `Float#to_int`、
  `Integer#pred`、Numeric の `nan?`（`1.nan?`）、`Integer#__num_to_a` は rc2 で NameError（`l8/p/own.rb`）。
  `Float#to_int` は CRuby にはある。消すか残すかは計画の外。
- **ネイティブの `arity` が -1**（VM、`Method#arity`）: rc2 は `MRB_ARGS_NONE` のネイティブに 0 を答える
  （`1.5.method(:to_i).arity` が 0）。SabiRuby はネイティブに引数の宣言を持たないので全部 -1。
- **Ruby で書いたメソッドの引数の数の文言**（VM、`vm.rs:5680`）: rc2 の `OP_ENTER` は省略できる引数があっても
  `argnum_error(mrb, m1+m2)`（`src/vm.c:2625`）で "expected 1"、SabiRuby は CRuby と同じ "expected 1..2"
  （`def f(a, b = nil); end; f(1, 2, 3)`、`1.clamp(1, 2, 3)`）。mrbtest は文言を見ていないので出ていない。
  rc2 に揃えるなら 1 行だが、全部の Ruby メソッドに効くので著者に聞く話。
- **捕まえない例外の出力**（CLI）: rc2 の `mruby` は `trace (most recent call last):` と行番号つきで書き、SabiRuby の
  CLI は文言だけ（`hex.rb` の最後の行）。既知かどうか確かめていない。
- **計画書の前提**: rc2-leftovers7 の worklog は `String#oct` を「-8（接頭辞を読む）」と書いていたが、rc2 の
  `str_oct` は基数 8。また hex/oct の違いは「接頭辞の扱い」ではなく、符号の直後の `_` だけだった。backlog には
  U1 の 3 件の行がもともと無かった（worklog の「気づいた点」だけにあった）ので、消した行は無い。
- **本の素材**: rc2 の `sort` は 16 要素を境に挿入ソートとヒープソートを切り替えるので、同じ比較でも 16 要素と
  17 要素で等しい要素の並びの性質（安定かどうか）が変わる。
