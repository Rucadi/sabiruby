# 2026-10-16 `Array#[]` の Float の端と、方針に沿った 2 つの差の記録（N1・N2）

計画: `docs/plans/rc2-leftovers3-plan.md`。方針: `docs/design/compatibility.md`（本家に合わせる。
明らかな不具合だけ正しい方を採り、3 か所に記録。迷ったら止めて報告）。前の作業は
`2026-10-16-rc2-leftovers2.md`（気づいた点 1・2）と `2026-10-16-follow-master.md`（M1 の節）。

結論: N1 は直した（`Array#[]` を String と分けて rc2 の形に。String 側も同じ入力で rc2 と違って
いたので、共有していた読み方を rc2 の `str_convert_range` の形に直した）。N2 の `a[0, nil] = 1`
は方針の 3 か所に記録した。N2 の Hash の vacated は、master と CRuby の答えが違うので直さずに
止めた（下に案）。

進め方はこれまでと同じ。スクラッチに断片を書き、rc2 の `mruby`（Docker
`kishima/mruby:4.1.0-rc2` と `-utf8`）と SabiRuby の出力を並べ、rc2 のソース
（`../../ref/mruby` の `git show 4.1.0-rc2:<path>`）でその答えを作る関数を読んで写した。

## N1. `Array#[]` の Float の端と、数でない添字の文言

### 直す前の比較

断片（スクラッチの `n1/a.rb`、Array と String のそれぞれに、1 引数 29 種・2 引数 20 種を
`[]`、`slice`、`slice!` で）を rc2 の 2 つのイメージで走らせると、bytes と utf8 の出力は同じ
（違うのは匿名オブジェクトのアドレスだけ）。SabiRuby との差は 248 行のうち 100 行を超えた。
種類は:

1. Range の端が Float・Rational・大きい Integer: rc2 は切り捨てて答える（`[1,2,3,4,5][1.0..2.0]`
   は `[2, 3]`、`"abcde"[1.5..2.7]` は `"bc"`）か、`mrb_as_int` の RangeError。SabiRuby は
   Array も String も `no implicit conversion into Integer`。
2. 数でない添字（nil、true、Symbol、String、Array、`to_int` を持つ物、文字列の Range）: rc2 は
   `nil cannot be converted to Integer`（nil・true・false 以外はクラス名）。SabiRuby は `[]`
   で `no implicit conversion into Integer`、`slice!` で CRuby の `no implicit conversion of nil
   into Integer`。
3. Rational の添字（`a[1r]`）: rc2 は 1 として読む。SabiRuby は TypeError。
4. **String の Float の添字**: rc2 の `String#[]` は `mrb_as_int` なので `"abcde"[Float::NAN]` は
   RangeError `float NaN out of range`。SabiRuby は `as i64`（NaN は 0）で `"a"`、
   `"abcde"[1, Float::INFINITY]` は `"bcde"`。
5. `slice!`（両方）の NaN・無限大・範囲外の Float: rc2 は RangeError `float NaN out of range`、
   `integer out of range`。SabiRuby は `Vm::expect_int` の FloatDomainError と
   `float 1.0e+30 out of range of integer`（CRuby の形）。

Array の Float の添字そのもの（`a[Float::NAN]` が nil）は follow-master で rc2 の C のキャストに
してあったので一致していた。

### rc2 の作り

`mrb_ary_aget`（`src/array.c:1363`）:

- 引数 1 つで Range なら `mrb_range_beg_len(..., TRUE)`（`src/range.c:537`）。端は
  `mrb_as_int`。Integer ならそのまま `mrb_ary_ref`。それ以外は `aget_index`（`src/array.c:1316`）:
  Float は `(mrb_int)` のキャスト、それ以外は `mrb_get_args(mrb, "i*!", &i, ...)`、つまり
  メソッドの第 1 引数を `mrb_as_int`。`Array#[]` では第 1 引数が添字そのものなので、ここでは
  癖が表に出ない。
- 引数 2 つなら `mrb_get_args(mrb, "oi", &index, &len)`。**長さが先に** `mrb_as_int` され、その後
  `aget_index(index)`。`[1,2][nil, "x"]` は rc2 で `String cannot be converted to Integer`
  （長さの方の文言）。

`String#[]` は `str_convert_range`（`src/string.c:1862`）: 長さがあれば添字、長さの順に
`mrb_as_int`。1 つなら String は部分文字列の探索、Range は `mrb_range_beg_len(..., TRUE)`、
それ以外（Float も）は `mrb_ensure_int_type`（＝`mrb_as_int`）。`String#[]=`（`mrb_str_aset`）と
`byteslice`（`src/string.c:4252`）も同じ読み方。

`slice!` は Array が `ary_slice_bang`（mruby-array-ext）、String が `mrb_str_slice_bang`
（mruby-string-ext）で、どちらも Range は `mrb_range_beg_len`、それ以外は `mrb_as_int`
（Array の 1 引数は `mrb_ary_delete_at` に任せ、そこが `mrb_as_int`）。

`mrb_as_int` の Float は `mrb_float_to_integer`（`src/numeric.c:1855`）: NaN と無限大は
RangeError `float %f out of range`（`NaN`、`Infinity`、`-Infinity`）、それ以外は `flo_to_i`。
範囲外の Float は `flo_to_i` で bigint になり、`mrb_ensure_int_type` の `mrb_bint_as_int` で
`integer out of range`。

つまり Array と String は同じ形の引数を違う形で読む: Float の添字は Array ではキャスト、String
では `mrb_as_int`。2 引数の変換の順も逆。共有していた `string::index_args` ひとつでは両方の答えに
ならないので、計画の「共有を分ける」を取った。

### 直し

- `numeric::as_int`（M3・M4 で作った `mrb_as_int` の写し）の Float の腕を、`Vm::expect_int` に
  任せるのをやめて `float_as_int`（`mrb_float_to_integer` の写し）にした。`expect_int` は
  多くの所から呼ばれていて、Float の腕は CRuby の FloatDomainError なので変えていない（前回の
  気づいた点 3 と同じ理由）。文言を作る所は `#[cold] #[inline(never)]` の
  `float_out_of_range` に出した。
- `ext_array::range_beg_len`（`mrb_range_beg_len` の写し。Array・String の `slice!`、
  `values_at`、`fill`、`Struct#values_at`、`MatchData` の `[]`／`values_at`、`caller` が使う）の
  端を `expect_int` から `as_int` に。Integer と nil は前と同じ腕で先に取る。
- `array.rs` に `aget_index` を足し、`ary_aref` を `mrb_ary_aget` の形に書き直した。Integer の
  添字の道は前と同じ（配列を写さない）。前は入口で `float_indexes` が毎回 `Vec` を作っていたが、
  なくなった。Range は `range_beg_len(..., true)`。2 引数は長さ、添字の順。
- `string::index_args`（String の `[]`、`slice`、`[]=`、`byteslice`、`bytesplice` が使う）を
  `str_convert_range` の読み方に: Range は `range_beg_len(..., true)`、それ以外は添字、長さの順に
  `as_int`。前は入口で全引数の `Vec` を作り Float を `as i64` にしていた。Range の端の計算を
  自前で持っていたのも `range_beg_len` に任せた（同じ答えになることを、`mrb_range_beg_len` の
  `trunc` の分岐と前の式を端の場合ごとに並べて確かめた）。
- `String#[]=` の数の添字の道（`str_aset`）: 長さを先に `expect_int` し、Float の添字は
  `as i64` だった。`str_convert_range` どおり添字、長さの順に `as_int` にした。
- `slice!`（両方）の `expect_int` を `as_int` に。

`array.rs` の `slice!`（`index_args` を呼ぶ）は `ext_array` の `slice!` が常に上書きするので
（`builtins/mod.rs` の init の順）走らない。触っていない（気づいた点）。

### 確かめたこと

断片 `n1/a.rb`（248 行）、`n1/b.rb`（24 行、順と `-Infinity`、`[]=`、`values_at`、
`byteslice`）、`n1/c.rb`（72 行、`String#[]=` と `Array#[]=` と `byteslice` を 24 通りの引数で）、
`n1/d.rb`（`fill`、`Struct#values_at`、`MatchData`、`caller` の Range）を rc2 と比べ、残った差は
次の 3 つだけ（どれも計画の外。気づいた点）:

- `[1].freeze.slice!(nil)`: rc2 `can't modify frozen Array`、SabiRuby `can't modify frozen
  Array: [1]`。
- `s = "abcde"; s["z"] = "X"`: rc2 `string not matched`、SabiRuby `string not matched: "z"`。
- `a[1, nil] = 9`: N2 の 1 つ目（rc2 の誤りを真似ていない差）。

`tests/custom/rc2_leftovers3.rb`（新規、206 行の出力）にまとめた。期待値は rc2 の出力そのもの
（`tools/custom.sh` が `.expected` を作り、`reference output agrees with .expected`）。直す前の
SabiRuby（`2080743`）ではこのうち 126 行が違った。

## N2. 既に決めていた 2 つの差を方針に沿って記録する

### `a[0, nil] = 1`

rc2-leftovers2 の M4 で見つけ、真似なかったもの。`mrb_ary_aset`（`src/array.c:1435`）の
`a[i, len] = v` は `mrb_ary_splice(mrb, self, aget_index(mrb, v1), aget_index(mrb, v2), v3)`
（`src/array.c:1461`）で、長さ `v2` が数でないと `aget_index` の `mrb_get_args(mrb, "i*!")` が
**メソッドの第 1 引数**（添字 `v1`）を変換して返す。長さは見られない。

| 入力 | rc2 | 891d1d9d | 158760853 | CRuby 3.2.6 / 3.4 | SabiRuby |
|---|---|---|---|---|---|
| `a = [1]; a[0, nil] = 1` | `[1, 1]` | 同じ | 同じ | TypeError | TypeError |
| `[1,2,3]` に `a[1, nil] = 9` | `[1, 9, 3]` | 同じ | 同じ | TypeError | TypeError |
| `a = [1]; a[0, "x"] = 1` | `[1, 1]` | 同じ | 同じ | TypeError | TypeError |
| `[1,2,3]` に `a[1.5, nil] = 1` | `[1, 1, 3]` | 同じ | 同じ | TypeError | TypeError |
| `a[nil, 1] = 1`、`a[nil, nil] = 1` | TypeError | 同じ | 同じ | TypeError | TypeError |

方針の「mruby の中で食い違う」に当たる（方針の文書がこの例を挙げている）: 同じ引数の
`Array#[]` は `"oi"` で長さを変換して TypeError、`String#[]=` も TypeError。SabiRuby の答えは
前から TypeError（`nil cannot be converted to Integer`、mruby の文言）で、コードは変えていない。
記録した 3 か所:

1. `README.md` "Known deviations" に、何が違うか、rc2 の該当コード、CRuby の答え、採った理由。
   前は 1 段落だけだったので、その下に「方針の例外として採ったもの」の箇条を作った。
2. `tests/custom/ary_aset_length_not_number.rb`（新規）。期待値は CRuby の出力（3.2.6 で作り、
   `ruby:3.4` の出力と同じことを確かめた）。文言は CRuby 独自（`no implicit conversion from nil
   to integer`）なので例外のクラスだけを出す。rc2 の答えはヘッダのコメントと `.rc.out`。
3. `book_mruby3/docs/notes/values-and-collections.md` に 1 行（下の Float の読み方の違いと一緒の節）。

### Hash の探索中に `eql?` が一致したエントリを空ける場合 — 直さずに止めた

follow-master の M1 で rc2 の見える答えを写したもの（rc2 は空いたスロットを読む）。master の
直しは `a54353ecf`（`entry_key_eql`: 比べた後にエントリのキーが変わっていたら RuntimeError
`hash modified`）、`08a0432d1`（`assoc`、`rassoc`、`==`、`eql?` の走査にも）、`eb7693857`
（`inspect`、`__except`、`rehash` にも）。どれもコミットの文が解放後の読み出し（CWE-416）を
直すと言っている。

891d1d9d の `test/t/hash.rb` の「Hash lookup with the matched entry vacated by an eql? callback」
と同じ形（`eql?` の中で一致したキーを消して `:added` を足し、真を返す）を、HT（20 個）、
先頭に穴のある AR（12 個、0 を消す）、穴のない AR（12 個）で、`delete`・`[]`・`[]=`・`key?` に
かけた（スクラッチ `n2/h.rb`）:

| 形 | rc2 | 891d1d9d・158760853 | CRuby 3.2.6・3.4 | SabiRuby（今） |
|---|---|---|---|---|
| HT `delete` | 9 を返す、size 19、`keys` と `key?` が食い違う | RuntimeError `hash modified` | nil（見つからない）、size 20 | 9、size 20 |
| HT `[]` | 9 | RuntimeError | nil | 9 |
| HT `[]=` | 書いた先が無く size 20 | RuntimeError | 新しいエントリとして足す、size 21 | size 20 |
| HT `key?` | true | RuntimeError | false | true |
| 穴のある AR | HT と同じ形（1、size 10 ほか） | RuntimeError | HT と同じ形（nil、false、足す） | 1、size 11 ほか |
| 穴のない AR | RuntimeError `hash modified`（4 つとも） | RuntimeError | nil、false、足す | RuntimeError |

rc2 の答えが「明らかな不具合」なのは確か（空いたスロットを読む＝方針の 1 つ目の例、本家も
バグとして直した）。しかし「正しい方」が 2 つある:

- **master**: RuntimeError `hash modified`。rc2 自身が穴のない AR では同じ例外を出している
  （`H_CHECK_MODIFIED` が AR の先頭のキーの変化に気づく）ので、mruby の中での一貫性はこちら。
- **CRuby**: 一致しなかったものとして続ける（`delete` は nil、`[]=` は新しいエントリ）。

計画の「master の答えと CRuby の答えが違う場合」に当たるので、直さずに止めた。案:

- **案 A（master の答え）**: HT と AR の探索で、`eql?` が真を返した後にエントリのキーが比べた
  ものと同じ（同一）か見直し、違えば RuntimeError `hash modified`。`hashtable.rs` の
  `Found::Vacated` の道を例外に替えるだけで、探索の形は変わらない。master の 08a0432d1・
  eb7693857 の走査（`assoc`、`rassoc`、`==`、`eql?`、`inspect`、`__except`、`rehash`）も同じ
  見直しを足すかどうかは別に決める（今の SabiRuby は Rust の `Vec` なので範囲外は読まず、違うのは
  答えだけ）。891d1d9d の `hash.rb` の vacated の 3 つの assert はこれで通るはず（edge_compare の
  数が動く）。
- **案 B（CRuby の答え）**: 一致しなかったものとして探索を続ける。SabiRuby の今の作りでは、
  空いたことは分かる（`Found::Vacated`）ので書けるが、本家のどの版とも違う答えになり、穴のない
  AR で rc2 自身が例外を出すこととも食い違う。
- **案 C（今のまま）**: rc2 の見える答えを写したまま。方針の例外には当たるが、記録だけする。

気持ちとしては案 A が方針の「mruby itself fixed it later as a bug」と「穴のない AR の rc2 と
揃う」の両方に合うと思うが、決めるのは著者。

## 守ること・数

`unsafe` は増やしていない。`no_std + alloc`（下の `check_no_std.sh`）。新しい数は
`float_as_int` の `LIMIT`（2^63、`i64` の範囲そのもの。`array.rs` の `float_index` と同じ式）
だけ。

## 性能

base は `2080743`（`git worktree add --detach` の別ディレクトリ、別の `CARGO_TARGET_DIR`。md5
`9249a6ee…`）、new は作業ツリー（md5 `04e55fa7…`）、A/A は new とそのコピー。
`tools/bench_ab.sh --core 2 --runs 5`、27 本、向きを入れ替えて 2 巡。表は「new が base より何 %
遅いか」に直したもの（2 巡目は new が A の側なので出力の符号を返した）。始める前の `vmstat` は
idle 99〜100%、ほかの cargo・mruby は無し（別のセッションの sleep のループだけ）。

| benchmark | A/A | 1 巡 | 2 巡 |
|---|---:|---:|---:|
| app_json_hash | +0.9% | -0.1% | -1.4% |
| app_robot | -0.3% | -1.4% | -2.2% |
| app_tak | -1.3% | +0.0% | +1.0% |
| bm_ao_render | +0.4% | -0.7% | -0.8% |
| bm_fib | -0.5% | +0.1% | -0.2% |
| bm_mandel_term | +0.1% | -0.2% | -0.7% |
| bm_so_lists | +0.7% | -0.6% | +0.6% |
| bm_so_mandelbrot | +0.0% | +1.1% | -0.6% |
| call_args | -0.6% | +0.0% | +0.2% |
| call_block_yield | +0.2% | -2.3% | -2.1% |
| call_fiber | +1.7% | -0.1% | +0.1% |
| call_kwargs | +1.2% | +0.6% | +0.0% |
| ds_array | +0.0% | -2.5% | -2.6% |
| ds_hash | +0.4% | -0.2% | -0.1% |
| ds_string | +0.5% | -1.2% | -1.6% |
| gc_churn | -0.1% | +0.4% | -1.3% |
| loop_if_branch | -0.6% | -1.5% | -0.3% |
| loop_times | -0.6% | -0.3% | +0.0% |
| loop_while_add | +0.2% | -0.7% | +0.0% |
| mem_retained | -0.6% | -1.5% | -1.6% |
| mem_short_lived | +0.7% | +1.0% | -1.0% |
| vm_optimization_bench | -0.5% | -1.2% | -1.3% |
| vmo_arith | +0.2% | -0.1% | -1.0% |
| vmo_calls | -0.7% | -3.5% | -3.0% |
| vmo_dispatch | +0.2% | -0.3% | -0.8% |
| vmo_index | -0.9% | -0.7% | +0.2% |
| vmo_objects | -3.9% | +1.4% | -2.7% |
| **all** | -0.4% | -0.7% | -1.0% |

計画が挙げた `ds_array`・`ds_string`・
`vmo_index` は遅くなっていない。`ds_array` は 2 巡とも -2.5% 前後で、A/A（+0.0%）の外。
`ary_aref` の入口で毎回 `Vec` を作っていた（`float_indexes`）のをやめたので、Range や 2 引数の
`[]` を通る分は手間が減っている。ただし `vmo_calls` の -3% のように `[]` を通らないものも同じ
向きに動いていて、L1・M2 で見た配置の揺れと区別できないので、速くなったとは言わない。命令ループの
腕には触れていない（`OP_GETIDX` の Integer の速い道は VM の中で、`ary_aref` に来ない）。

## 確かめたこと

- `cargo test --workspace`（debug。fixtures、custom 37、mrbtest 111 ファイル）: 275 通過、
  0 失敗。`cargo test --release --workspace`: 275 通過。`cargo test -p sabiruby
  --no-default-features --features "std utf8"`: 161、`--features "std regexp"`: 165、どちらも
  失敗 0（バイト列のビルドでも `rc2_leftovers3` は通る。rc2 の bytes と utf8 のイメージの答えが
  同じなので、`utf8-only` にしていない）。
- `tools/check_no_std.sh --features regexp`／`--features utf8`: `no_std OK`。
  `cargo build -p sabiruby --lib --target wasm32-unknown-unknown`、`cargo doc --no-deps
  --workspace --lib` と `-p sabiruby-cli --bins`（`RUSTDOCFLAGS=-D warnings`）、`cargo check
  --workspace --all-targets`: 通過、警告なし。
- rc2 の mrbtest（`tools/mrbtest.sh`、3 ビルド）: 2346 / 2508、2269 / 2453（バイト列）、
  1981 / 2012（regexp なし）。作業前と同じ（`docs/verification/mrbtest*.md` は日付の行しか
  変わらなかったので戻した）。

## edge_compare の再計測

`bd443db` の木で `tools/edge_compare.sh`（JSON の `dirty: false`）。数は変わらない:

| | 前（`9e98db5`） | 今（`bd443db`） |
|---|---:|---:|
| SabiRuby: pass / fail / error | 855 / 36 / 34 | 855 / 36 / 34 |
| SabiRuby with C fixtures: pass / fail / error | 880 / 46 / 31 | 880 / 46 / 31 |
| ファイル: passed / failed / runtime_error（SabiRuby） | 29 / 18 / 1 | 29 / 18 / 1 |

JSON とページの差は、コミット、時刻、経過時間だけ。N2 の Hash は直していないので、891d1d9d の
`hash.rb` の vacated の 3 つの assert は前と同じく落ちる（`notes.tsv` は触っていない）。

## 気づいた点

- **VM / rc2 との差（計画の外）**: 凍結した Array の `slice!` の文言。rc2 は `can't modify
  frozen Array`、SabiRuby は `can't modify frozen Array: [1]`（`ext_array.rs` の `check_frozen`）。
  ほかの破壊的メソッドも同じ関数なら同じ差がある。確かめたのは `slice!` だけ。
- **VM / rc2 との差（計画の外）**: `s = "abcde"; s["z"] = "X"` の文言。rc2 `string not
  matched`、SabiRuby `string not matched: "z"`（`string.rs` の `str_aset`）。
- **VM**: `Array#delete_at`（`array.rs`）は `Vm::expect_int` で、rc2 の `mrb_ary_delete_at` は
  `mrb_as_int`。数でない添字の文言と NaN の例外が `slice!` の 1 引数（rc2 では同じ関数）と違う。
  今回は `slice!` だけ直した。
- **VM**: `MatchData#[]` の 2 引数の道（`ext_regexp.rs:1213-1214`）など、`expect_int` を
  `mrb_as_int` の代わりに使っている所はほかにもありそう。`expect_int` の Float の腕と最後の腕は
  CRuby の文言で、rc2 の `mrb_as_int` とは違う（前回の気づいた点 3 の続き）。
- **死んだコード**: `array.rs` の `slice!` は `ext_array.rs` の `slice!` が init で上書きする
  ので走らない（core の Array に `slice!` は無く、本家でも array-ext のもの）。
- **本の素材**: Array と String が同じ形の引数を違う形で読む（Float の添字は Array ではキャスト、
  String では `mrb_as_int`、2 引数の変換の順も逆）。notes に 1 行足した。
- **方針の判断待ち**: N2 の Hash の vacated（上の案 A〜C）。
