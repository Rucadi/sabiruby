# 2026-10-16 4.1.0-rc2 との残りの差、その 2（M1〜M4）

計画: `docs/plans/rc2-leftovers2-plan.md`。L1〜L5（`2026-10-16-rc2-leftovers.md`）の作業の気づいた点に
残した 4 つ。著者の判断は「悪影響あまりなさそうで直しやすそうなら直してみてほしい」で、計画の
「直すかどうかの線」（悪影響、直しやすさ、4.1 の挙動か）を 1 件ずつ見てから手を付けた。結論は 4 つ
とも直した。

進め方は前回と同じ。スクラッチに断片（`m2/a.rb`、`m2/b.rb`。49 行と 9 行）を書き、rc2 の `mruby`
（Docker `kishima/mruby:4.1.0-rc2`）と SabiRuby の出力を並べ、rc2 のソース（イメージの
`/usr/src/mruby` を取り出したもの）でその答えを作る関数を読んで写す。行番号は rc2 のツリーのもの。
最後に断片を `tests/custom/rc2_leftovers2.rb` にまとめた（期待値は rc2 の出力そのもの）。

直す前の断片の比較は、`a.rb` 49 行のうち 30 行、`b.rb` 9 行のうち 7 行が違った。

## M1. クラスを受け手にした `Foo::String`

### rc2 の作り

`OP_GETMCNST`（`src/vm.c:3382`）は `mrb_const_get(mrb, regs[a], sym)`（`src/variable.c:1346`）を呼び、
それは `mod_const_check` の後に `const_get(mrb, c, sym, skip = FALSE)` を呼ぶ。`const_get_nohook`
（`src/variable.c:1278`）は:

```c
  if (skip) c = c->super;
L_RETRY:
  while (c) {
    if (... iv_get(mrb, class_iv_ptr(c), sym, &v)) return v;
    c = c->super;
    if (!skip && c == mrb->object_class) break;
  }
```

`skip` が偽のときは、受け手とその祖先を見るが、**次が Object になったところで止まる**。受け手が
Object そのものなら最初に Object の表を見るので `Object::String` は見つかる。`Foo < Object` なら
`Foo` を見て、次が Object なので止まり、`Foo::String` は NameError（`const_missing` が呼ばれる）。
include したモジュールの include クラスは Object の手前にあるので見る（`Sub::IX` は見つかる）。
特異クラス `#<Class:Foo>` は `#<Class:Object>`、`#<Class:BasicObject>`、Class、Module と上って
Object の手前で止まる。BasicObject の子は Object を通らないので最後まで見る（Object に無いので同じ
こと）。モジュールは `super` が NULL なので前から Object を見ない（`skip` が真の `OP_GETCONST` の
道だけが、モジュールのとき Object からもう一度見る）。

`Module#const_get`（`mrb_mod_const_get`、`src/class.c:4023`）も、Symbol でも "A::B" の文字列でも、
名前ごとに `mrb_const_get_sym` → `mrb_const_get` を通るので同じ範囲。一方 `defined?(Foo::String)` は
別の道（`const_defined` 系）で、rc2 でも `"constant"` を答える。`Foo.const_defined?(:String)` も
true。この 2 つは今のままでよい。

master（`../ref/mruby` の `891d1d9d`）の `const_get_nohook` も同じ形（`src/variable.c:1311` の
`break`）なので、4.1 の挙動で、master でだけ変わったものではない。

### 悪影響の grep

直す前に、`X::Y` と書いて `Y` が Object の定数（クラスを越して読んでいる）書き方を探した。
SabiRuby の `Object.constants`（`target/release/sabiruby` で出した 81 個）を並べた正規表現
`[A-Za-z0-9_]::(ARGV|ArgumentError|Array|…|JSON)\b` で、

- rc2 の `mrblib/` と `mrbgems/*/mrblib/`（SabiRuby の `src/mrblib/` の元）
- rubevy、rubevy_games、rubevy_games-wt-factory、journey、sabiruby-playground
- この repo の `bench/`、`examples/`、`src/`

の `.rb` を探して、**0 件**。`const_get` を Ruby から呼んでいる所も、上の場所には 0 件（tests の
外では rubevy の Rust 側の `vm.const_get(...)` だけで、これは Rust の公開 API なので変えない。下）。

`X::Y` の形を全部（repo ごとに `[A-Z]\w*(::[A-Z]\w*)+` を数えて）並べて目で見たものでは、Object の
定数をクラス越しに読んでいるのは tests の中の、その差そのものを確かめる行だけだった:
`tests/edge-spec/upstream/t/syntax.rb:2151-2155` と `tests/custom/defined_paths.rb:19`
（`defined?(String::String)` などで、`defined?` の道なので変わらない）、mrbtest の
`syntax.rb:409` の `assert_raise(NameError) { Syntax4Const::Object }`（モジュールなので前から
NameError）。rubevy_games の `Creature::CRUISE` は `Creature` の中で定義した定数。fixtures の
`Dog::COUNT_LIMIT` は親の `Animal` の定数で、Object の手前なので見つかる。

Rust の文字列に埋めた Ruby（rubevy の `tests/*.rs` の `r#"…"#`）は、`[A-Z]\w*::(Object|String|…)`
で `.rs` を探すと Rust の enum（`Value::Float`、`Op::String` など）しか出なかった。

### 直し

`Vm::const_get`（`pub`）は rubevy と VM の中の 30 か所ほどが `vm.const_get(vm.core.object, …)` の
形で使っていて、Rust から「この名前の定数」を引く口なので意味を変えない。表を歩く本体を
`Vm::const_walk(class, name, stop_at_object)` に出し、`const_get` は今までどおり最後まで、
新しい `Vm::const_get_scoped`（`pub(crate)`）は rc2 の `skip = FALSE` の形（`super` に進んだ先が
Object なら止まる）にした。`OP_GETMCNST` と `Module#const_get` を `const_get_scoped` に替えた。

`Module#const_get` の "A::B" の道は、前は途中の名前が見つからないと `const_missing` の答えをそのまま
返していた（rc2 はその答えを次の名前の受け手にして続ける）のと、「最後の名前か」を文字列の比較
（`seg == path.split("::").last()`）で見ていた（`"A::X::X"` のように同じ名前が並ぶと途中で返る）
ので、rc2 の `mrb_mod_const_get` のループの形に書き直した: 受け手（`Value`）を持って回り、毎回
クラスかを確かめてから引く。

## M2. クラスでない受け手の TypeError

rc2 には 2 つの文言がある:

- `mod_const_check`（`src/variable.c:1264`）: `constant look-up for non class/module`。
  `mrb_const_get`（`1::X`、`const_get("A::B")` の途中）と `mrb_const_set`（`x::Y = 2`）から。
- `check_if_class_or_module`（`src/class.c:439`）: `%!v is not a class/module`（`inspect` した値）。
  `class x::Foo`／`module x::Mm` の外側（`src/class.c:492`、`651`）から。

SabiRuby はどれも `not a class/module`（`const_get` の文字列の道だけ `[] is not a class/module`）
だった。前者を `vm::NON_CLASS_LOOKUP`（文言 1 つの `&str`）にして `OP_GETMCNST`、`OP_SETMCNST`、
`Module#const_get` で使い、`OP_CLASS`／`OP_MODULE` の外側は `inspect_str` で後者の形にした。
`OP_EXEC` と `OP_DEF` の `not a class/module` は Ruby から届かない（コンパイラがクラスしか置かない）
ので触っていない。`instance_eval`／`class_eval` の文字列（`ext_eval.rs:231`）も別の話なので残した。

## M3. `Array#[]=` の Range の Float の端

`mrb_ary_aset`（`src/array.c:1435`）は引数 2 つなら `mrb_range_beg_len(…, trunc = FALSE)`
（`src/range.c:537`）に渡し、そこは端を `mrb_as_int`（`include/mruby.h:1662`、
`mrb_ensure_int_type`）で取る。Float は切り捨て（NaN と範囲外は `mrb_float_to_integer` の
FloatDomainError／RangeError）、Rational と Complex も整数にし、ほかは
`mrb_ensure_integer_type`（`src/object.c:631`）の `%Y cannot be converted to Integer`。
`a[(1.5..2.7)] = 9` は `1..2` になる。SabiRuby は端が Integer か nil でなければ
`no implicit conversion into Integer` だった。

`numeric.rs` に `as_int`（`mrb_as_int` の写し）を足した。Integer はそのまま、Float・Rational・
Complex・大きい Integer は `Vm::expect_int`（前から `mrb_ensure_int_type` の写し）に任せ、
ほかは同じファイルの `not_int`（前から `mrb_as_int` の文言を作っていたもの）で TypeError。
`Vm::expect_int` の最後の腕の文言（`no implicit conversion of nil into Integer`）は CRuby の形だが、
多くの所から呼ばれているので変えていない（気づいた点）。

## M4. `a[nil] = 1` の文言

添字が Range でないとき、rc2 は `aget_index`（`src/array.c:1316`）で Integer か Float（C の
キャスト）をそのまま使い、ほかは `mrb_get_args(mrb, "i*!", &i, …)` で取る。`'i'` は
`mrb_as_int(mrb, argv[i++])`（`src/class.c:1449`）なので、文言は M3 と同じ
`nil cannot be converted to Integer`（nil、true、false 以外はクラス名）。`ary_aset` の、Range で
ない添字と長さを `as_int` に通した。

ここで rc2 の癖が 1 つ見つかった。`aget_index` が見る `mrb_get_args` は**メソッドの引数の頭から**
読むので、長さ（2 つ目）が数でないとき、長さではなく 1 つ目の添字を変換する。`a = [1];
a[0, nil] = 1` は rc2 では例外にならず、長さ 0 として `[1, 1]` になる（答えは 1）。master
`891d1d9d` の `aget_index` も同じ。本家の誤りと読み、真似ていない（長さそのものを変換して
`nil cannot be converted to Integer`）。`ary_aset` のコメントにそう書いた。

## テストの期待値

新しく `tests/custom/rc2_leftovers2.rb`（M1 24 行、M2 8 行、M3 10 行、M4 7 行）。`tools/custom.sh
rc2_leftovers2` が `.expected` を rc2 の出力から作り、`reference output agrees with .expected`。
最初は M2 の断片で `ARGV::X` を使っていて、`cargo test` の runner には `ARGV` が無く NameError に
なったので、ファイルの中で定義した `NotAMod = [1]` に替えた。

`Array#[]`（読む側）の Range の Float の端と `a[nil]` の文言、上の `a[0, nil] = 1` は、計画の外か
真似ないものなので入れていない。

## 性能

`OP_GETCONST`（L1 の `const_lookup` の速い道）には触れていない。触れたのは命令ループの
`OP_GETMCNST`（呼ぶ関数を `const_get_scoped` に）、`OP_SETMCNST` と `OP_CLASS`／`OP_MODULE` の
例外の腕。定数の探索に触れたので、計画どおり A/B を取った。base は `f4746ae`
（`git worktree add --detach` の別ディレクトリ、別の `CARGO_TARGET_DIR` で建てた。md5
`8dd19106…`）。`tools/bench_ab.sh --core 2 --runs 5`、向きを入れ替えて 2 巡。表の値は「新しい方が
何 % 遅いか」に直したもの（2 巡目は base が B の側に来るので、出力の符号を返した）。

最初の形（v1、md5 `f3aa398b…`）は `bm_so_mandelbrot` +13.6%／+12.2%、`bm_mandel_term`
+8.2%／+7.5% と、2 巡とも同じ向きに大きく出た（全体 +1.5%／+0.9%）。mandelbrot は `X::Y` も
`class x::Foo` も通らない。L1 の worklog の「命令ループの腕の形」と同じ種類で、`OP_CLASS` の腕に
`inspect_str` と `format!` をその場で書いたので、ループの関数が大きくなったと読んだ。

その文言を作る所を `#[cold] #[inline(never)]` の `Vm::not_class_or_module` に出して、腕を呼び出し
1 つにした形（v2、md5 `e549f39f…`、これをコミットした）:

| benchmark | A/A | v1 1 巡 | v1 2 巡 | v2 1 巡 | v2 2 巡 |
|---|---:|---:|---:|---:|---:|
| app_json_hash | +0.4% | +1.2% | +1.4% | -0.4% | -0.1% |
| app_robot | -0.6% | +1.2% | +1.0% | +0.4% | +0.5% |
| app_tak | -1.0% | +0.8% | +1.0% | +0.2% | +0.3% |
| bm_ao_render | +1.0% | +1.2% | +2.6% | -0.8% | -0.9% |
| bm_fib | -1.0% | +0.7% | +0.6% | +0.6% | -2.1% |
| bm_mandel_term | +1.2% | +8.2% | +7.5% | -1.1% | -2.2% |
| bm_so_lists | +0.1% | +2.7% | -0.4% | +0.3% | -0.2% |
| bm_so_mandelbrot | +0.6% | +13.6% | +12.2% | -0.7% | -0.7% |
| call_args | +0.9% | +0.9% | -0.5% | +0.4% | -0.2% |
| call_block_yield | +0.8% | +1.3% | +0.3% | +3.3% | +3.1% |
| call_fiber | -0.3% | +1.1% | +1.0% | +0.4% | -0.0% |
| call_kwargs | -0.5% | +0.3% | -1.7% | -3.4% | -4.6% |
| ds_array | -0.5% | +2.9% | +0.9% | +2.8% | +2.0% |
| ds_hash | +0.2% | -2.8% | +2.1% | +0.7% | +2.1% |
| ds_string | +0.2% | +0.3% | +2.0% | +0.2% | +0.5% |
| gc_churn | -0.4% | -0.4% | +2.2% | -0.1% | +0.2% |
| loop_if_branch | +0.4% | -2.4% | +1.0% | +0.3% | -0.5% |
| loop_times | +0.2% | +4.8% | -0.8% | +0.9% | +3.0% |
| loop_while_add | +1.5% | +7.4% | +0.6% | +0.2% | +0.4% |
| mem_retained | +0.8% | -3.5% | -1.9% | -1.9% | -1.0% |
| mem_short_lived | +0.2% | -0.8% | -1.0% | +0.9% | +0.1% |
| vm_optimization_bench | -0.3% | +3.2% | +0.3% | +1.1% | +0.1% |
| vmo_arith | +1.5% | -3.8% | +0.2% | +0.4% | -0.8% |
| vmo_calls | -0.3% | +0.8% | +0.5% | +2.9% | +3.1% |
| vmo_dispatch | +1.3% | +0.8% | -0.2% | +2.2% | -0.2% |
| vmo_index | -0.0% | +1.2% | +1.8% | +4.3% | +0.9% |
| vmo_objects | -0.3% | -0.4% | -2.4% | +0.8% | -0.2% |
| **all** | +0.1% | +1.5% | +0.9% | +0.7% | -0.1% |


v2 で mandelbrot 2 本は線の内に戻り、全体も A/A と同じ幅になった。2 巡とも同じ向きに A/A の幅
（この日の 1 回、1 本ごとに ±1.5%）を越えているのは `call_block_yield` +3.3%／+3.1%、`vmo_calls`
+2.9%／+3.1%、`ds_array` +2.8%／+2.0%（遅い側）と `call_kwargs` -3.4%／-4.6%（速い側）。どれも
`X::Y` も `class x::Foo` も `Array#[]=` のこの道も通らない（`ds_array` は `[]=` を Integer の添字で
呼ぶ。その道は変換の腕の形が変わっただけで、比較は前と同じ `Value::Int` の一致 1 つ）。L1 で
見た、`codegen-units = 1` での配置の揺れ（呼ばれない所を変えても数 % 動く）と同じものと読み、
ここで追うのを止めた（L1〜L5 のときと同じく原因は特定していない）。

## 確かめたこと

（下で埋める）

## 気づいた点

（下で埋める）
