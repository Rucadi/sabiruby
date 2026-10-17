# 2026-10-17 rc2-leftovers7: 残りの小さな差を方針で片付ける

計画: `../plans/rc2-leftovers7-plan.md`。点検の表は同じ場所の `rc2-leftovers7-check.md`（以下「表」、行番号は表の #）。
方針は `../design/compatibility.md`。正解は 4.1.0-rc2（Docker `kishima/mruby:4.1.0-rc2`、ソースは
`/home/kishima/book/ref/mruby` の `c17ffcc24`。HEAD は動かしていない）。master は `kishima/mruby:891d1d9d`、
CRuby は手元の 3.2.6。base は `28cbf01`。

プローブはスクラッチの `l7/p/*.rb`（`run.sh` で SabiRuby・rc2・master・CRuby を並べる）、整数変換の表は
`l7/conv/`（表の `chk/conv.rb` と `cat.rb` をそのまま再実行）、使われないネイティブの確かめは `l7/dn/`
（下の T4）。

コミットは 2 つ: T1〜T3（`74bdf3c`）と T4・worklog（この文書と同じコミット）。book_mruby3 に 1 つ（`ba4c925`）。

## T1. rc2 に合わせる

### 整数の引数（表 #8）

`Vm::expect_int`（`src/vm.rs:1450`）は CRuby の形の誤りを出していた: nil は "no implicit conversion of nil into
Integer"、NaN は FloatDomainError "NaN"、`i64` を超える Float は "float X out of range of integer"。rc2 の
`mrb_get_args("i")` は `mrb_as_int`（`mrb_ensure_int_type`、`src/object.c:680`）で、"nil cannot be converted to
Integer"、RangeError "float NaN out of range"、"integer out of range" になる。SabiRuby には既に `numeric::as_int`
（rc2-leftovers3 で足した、`mrb_as_int` の写し）があり、`expect_int` と 2 本立てになっていた。

`expect_int` の誤りの腕を `as_int` と同じにし、`as_int` は `expect_int` を呼ぶだけにした（2 本を 1 本に）。誤りを作る
のは `numeric::not_int`・`float_out_of_range`・`int_out_of_range` の 3 つで、どれも `#[cold] #[inline(never)]`。
`expect_int` 自身は答えを返す腕（Int、Rational・Complex、幅の広い Integer、Float の切り捨て）だけを持つ。L1・M2・
rc2-leftovers4 の教訓（呼び出しの多い関数に文言の組み立てを置くと 2〜4% 動いたことがある）に従った形で、熱い道の
Int の腕は 1 つ目のまま。

表の `chk/conv.rb` を再実行すると（`l7/conv/`）、差のあるメソッドは 66 → 26 → 最後に 0 + B の 3 つ（下）になった。
最初の書き換えだけで文言だけの 41 のうち 40 が揃い、残る 1 つは `String#to_i(1.5)` の "invalid radix 1" ↔ rc2 の
"illegal radix 1" だった。前に一致していて書き換えで崩れた行が無いことは、差の行の集合（`now.txt`）が前の集合
（`before.txt`）の部分集合であることで確かめた。

値まで違う 25 は 1 つずつ rc2 の関数を読んで直した。

- `Array#pop`（`src/builtins/array.rs:263`）: `mrb_ary_pop` は `MRB_ARGS_NONE`。`pop(n)` を消した（表 #13）。
- `Array#*`（`array.rs:272`）: `mrb_ary_times` は `mrb_check_string_type` で String なら join、それ以外は
  `mrb_as_int`。String でないものを全部 `expect_str` に回していたので 1.5 が "argument cannot be converted to
  String" になっていた。
- `Integer.__ensure`（`numeric.rs:514`）: rc2 の `int_s_ensure` は `mrb_ensure_int_type` そのもの。2**70 を通し、
  NaN を "can't convert Float into Integer" にしていた。`expect_int` を呼ぶだけにして、`first(n)`・`take`・
  `each_slice`・`each_cons`・`Range#first`・`Hash#first` の 8 か所が一度に揃った（表 #15）。
- `Integer#<<`・`>>`（`numeric.rs` の `shift`）: `int_lshift` は幅を `mrb_as_int` で読む。Float を `coerce_fail` に
  回していたので "can't convert Float into Float" という壊れた文言だった（表 #14）。
- `Float#floor`・`ceil`・`truncate`（`numeric.rs:720` `float_round`）: 引数を見ていなかった。rc2 の `flo_rounding`
  （`src/numeric.c:851`）を写した: 0.0 は桁が正なら自分、そうでなければ 0。桁が正なら `fprec` を超えると自分、
  `func(f * 10^n) / 10^n` を Float で、負なら `func(f / 10^-n) * 10^-n` を Integer で、どちらも有限でなければ
  `mrb_check_num_exact` の FloatDomainError。`fprec` は rc2 の `DBL_DIG`（15）で、`f64::DIGITS` から取った（数を
  埋め込まない）。`truncate` は `flo_truncate` どおり符号ビットで `ceil` と `floor` を選ぶ。`1.5.floor(-400)` は
  `10^400` が無限大になって `0 * inf` = NaN、rc2 と同じく FloatDomainError "NaN" になる（rc2 で確かめた）（表 #12）。
- `String#setbyte`（`string.rs:550`）: rc2 は `byte & 0xff` を返す。引数をそのまま返していたので 1.5 が 1.5、321 が
  321 だった。
- `String#split(" ", 1.5)`: 1.5 の問題ではなかった。`"a b".split(" ", 1)` からもう違っていた。rc2 の
  `mrb_str_split_m` は区切りを読む前に「limit が 1 なら受け手そのものを 1 要素の Array で返す（空なら `[]`）」。
  空白で区切る道が limit 1 を「0 個切ったところで残りを足す」で扱い、先頭の語を切っていた。早期の return を足した。
  返すのは受け手そのもの（`equal?` が真）で、それも rc2 と同じ。
- `String#to_i`（`string.rs:504`）: rc2 の `mrb_str_to_i` は基数が 0..36 の外なら "illegal radix"、それ以外は
  `mrb_str_len_to_integer` に渡す。0 は接頭辞を読み（`"0x1f".to_i(0)` は 31）、1 はそこで "illegal radix 1"。
  SabiRuby は 2..36 の外を全部 "invalid radix" にしていたので `to_i(0)` が例外だった。`Integer()` のために既に
  `ext_kernel::str_to_integer`（rc2 の走査の写し）があったので、`to_i` もそれを呼ぶようにした。playground の本の
  サンプル `web/samples/book/corelib_numbers.rb` は `"0b101".to_i(0)` を使っていて、前の SabiRuby では
  "invalid radix 0 (ArgumentError)" で止まっていた（今は rc2 と同じ 5）。
- `values_at`（Array は `ext_array.rs`、Struct は `ext_struct.rs`）: rc2 の `mrb_get_values_at`（`src/range.c:433`）は
  Integer（と幅の広い Integer）なら `mrb_as_int`、そうでなければ `mrb_range_beg_len(..., FALSE)` が OK のときだけ
  Range として読み、それ以外は全部 TypeError "invalid values selector: %v"（`%v` は `to_s` なので nil は空）。
  読んでみると Float と String だけでなく、**受け手の前から始まる Range**（`[1,2,3].values_at(-9..1)`）も
  `MRB_RANGE_OUT` でこの腕に落ちる。SabiRuby は `[]` を返していた。表には無かった差で、同じ関数の話なので一緒に
  直した。文言は `ext_array::invalid_selector`（cold）（表 #2）。
- `shuffle(random:)`（`ext_random.rs:284`）: rc2 の `mrb_ary_shuffle_bang` は長さ 2 以上のときだけ `mrb_get_args`
  を呼ぶ。キーワードの検査も位置引数の数の検査もその中なので、`[1].shuffle(1)` も `[1].shuffle(foo: 1)` も通る。
  `random_kw` と `argc!` を `if len > 1` の中に入れた（表 #16）。`sample` は常に読むので今のまま。

### sort の比較の失敗（表 #1）

rc2 の `sort_cmp`（`src/array.c:2323`）は block が無いとき、同じ型の Integer・Float・String は直に比べ、それ以外は
`mrb_cmp`（`src/numeric.c:2423`）。`mrb_cmp` は `<=>` の答えが nil か Integer でなければ -2 で、`sort_cmp` は -2 を
"comparison failed" にする。block があるときは `cmpint`（`:2303`）で、nil は "comparison of %T with %T failed"、
Integer でない答えは `> 0`・`< 0` を問う。SabiRuby の `sort_values`（`array.rs`）は block の有無で分けず、nil を
"comparison of X with Y failed"、Float の答えを `> 0` で順に使っていた。プローブ（`l7/p/sort.rb`）で、block 無しで
`<=>` が 1.5 を返すクラスを並べると rc2 は "comparison failed"、SabiRuby は並べてしまうことも分かった。block が
無ければ Integer 以外の答えは全部 "comparison failed"（`sort_failed`、cold）にした。`sort_by` は mrblib が
`[key, i]` の Array を `sort!` するので、同じ直しで "comparison failed" になった。`max`・`min` は rc2 も
"comparison of X with Y failed"（`ary_cmp_ordered`）で、SabiRuby の `ext_array::cmp_ordered` は元から一致していた
（計画の「`ext_array.rs:102` の 2 か所」の片方は直す必要が無かった）。

block の nil の文言で、rc2 は `[N.new, 2].sort { nil }` を "comparison of N with Integer"、SabiRuby は
"Integer with N" と書く。これは並べ方（rc2 はヒープソート、SabiRuby はマージソート）でどの組をどの順で block に
渡すかの違いで、文言の関数の違いではない。計画の外なので触っていない（「気づいた点」）。

### raise、可視性（表 #3、#4、#4b、#5）

- `raise`（`kernel.rs:197`）: `mrb_f_raise` は `mrb_get_args(mrb, "|oo")`。`argc!(vm, a, 0, 2)` にした。`fail` と
  `Kernel.raise` も同じ関数で、どれも "wrong number of arguments (given 3, expected 0..2)"。
- `private`／`public`／`protected`（`object.rs:600` `set_vis`）と `module_function`: rc2 の `mrb_mod_visibility`
  （`src/class.c:2482`）と `mrb_mod_module_function`（`:4232`）は名前ごとに `mrb_check_type(..., MRB_TT_SYMBOL)`。
  Array を展開し String を受け付けていた `flat_syms` を `vis_syms`（`object.rs:589`）に替えた。文言は
  `mrb_check_type` の "wrong argument type X (expected Symbol)"。X の書き方（nil・Integer・Symbol はその語、他の
  即値は `to_s`、オブジェクトはクラス名）は rc6 が Array 用に書いた `check_type_array` と同じなので、そこから
  `ext_array::wrong_type`（`ext_array.rs:72`、cold）に切り出して両方で使った。返り値は rc2 では引数があっても
  無くても **モジュール自身**（`mrb_mod_private` が `mod` を返す）。名前や nil を返していたのを `s` にした。
- プローブで、**SabiRuby には top level の `private`／`public`／`protected` が無かった**ことが分かった（`private :a`
  が "undefined method 'private' for Object"）。rc2 は `top_public`／`top_private`／`top_protected`
  （`src/class.c:2569-2590`、`:4767-4769` で main の特異メソッド）で、Object に対して `mrb_mod_visibility` を呼び、
  Object を返す。表の 4b が「`top_private` が mod を返す」と書いている所で、返り値だけでなく口そのものが要った。
  main の特異クラスに 3 つ足した（`object.rs:257`）。引数なしの `private` は呼び手のフレームの既定を変える
  （`set_scope_vis` は `visibility_scope` で呼び手を探すので、Module#private と同じに働く）。
- `private_class_method`／`public_class_method`: 消した。rubevy・games・playground・tests に使用なし（grep）。

### String の引数の文言（表 #7、#7b、#7c）

`Vm::expect_str`（`vm.rs:1432`）は 2 つ目の引数（"pattern"、"separator"…）を文言に入れていた。rc2 の
`mrb_ensure_string_type` は "%Y cannot be converted to String"。文言を `not_a_string`（cold）に出し、ラベルの引数は
呼び手の読み手のために残した（呼び手 42 か所はそのまま）。`String#+` は `%T`（nil が NilClass）で書いていたので
同じ関数にした。`Object.const_get(1)` は `sym_arg` の "1 is not a symbol nor a string" だったので、Symbol でも
String でもなければ先に `not_a_string`。プローブ `l7/p/str.rb`（37 の呼び方 × 7 の値）で、違いはアドレスの 5 行だけ
になった。

### tests/custom

`rc2_leftovers7.rb`（362 行、`.expected` = `.rc.out`）と `rc2_leftovers7_regexp.rb`（`Regexp.union`／`escape`、
`# regexp-only:`）。最初は 1 本にして `Regexp.union` を `if Object.const_defined?(:Regexp)` で囲んだが、
`.expected` は regexp 入りの rc2 の出力なので、regexp なしのビルド（CI の `--no-default-features --features
"std utf8"`）では行が足りずに落ちた。行を消すのではなく regexp-only の別ファイルに分けた。base の SabiRuby
（`28cbf01`）で同じファイルを流すと 362 行のうち 285 行が違う。

## T2. rc2 が明らかな不具合（表 #9〜#11、#18）

### Float の添字と `rand` の限度（コードを直した）

rc2 の `aget_index`（`src/array.c:1322`）は Float を `(mrb_int)mrb_float(index)` で、`random_rand_impl`
（mruby-random `random.c:251`）は `(mrb_int)mrb_float(arg)` で読む。NaN・無限大・`mrb_int` を超える値ではこれは C の
未定義の動作で、x86-64 は最小の整数（`cvttsd2si` の 0x8000…）を返す。rc2 で `a[Float::NAN]` が nil、
`a[Float::NAN] = 1` が "index -9223372036854775808 out of array"、`rand(Float::NAN)` が毎回違う整数なのはそのため。
master `e2148c1b1`（2026-10-09、"range-check a Float before narrowing it to an index, limit or timeout"）が添字を
`mrb_as_int`、限度を `mrb_ensure_integer_type` に替え、テストのコメントで「AArch64 では NaN を 0 と読んで別の答えに
なる」と書いている（AArch64 は実測していない）。方針の「未定義の動作」と「本家が後で不具合として直した」の両方に
当たる。

- `aget_index`（`array.rs:355`）: Float の腕を消し、Integer 以外を全部 `as_int` に。C のキャストを写していた
  `float_index`／`float_indexes` は使われなくなったので消した。`[]=` の頭で Float を前もって狭めていた分も消し
  （`as_int` が同じことをする）、コメントで master の commit を指した。`OP_GETIDX` の速い道は Int の添字だけを
  答えるので触れていない。
- `random_rand_impl`（`ext_random.rs:145`）: Float は有限でなければ `float_out_of_range`、有限なら切り捨てて
  `float_to_int`（`i64` を超えれば幅の広い Integer）にしてから、Integer・幅の広い Integer の腕に流す。
  `mrb_ensure_integer_type` が Float を Integer（または bigint）にして先へ流すのと同じ形。`rand(1e30)` は bigint の
  限度で引き、`rand(-1e30)` は "negative value as random limit"。master と一致（`l7/p/b.rb` で 33 行すべて）。

この直しで、rc2 の C のキャストの答えを期待値にしていた 2 本の custom（`edge_misc.rb` の 2 行、
`rc2_leftovers3.rb` の 5 行）が変わった。どちらも見出しに「この行は master e2148c1b1 の答え、README の Known
deviations」と書き、`.expected` を直した。記録の本体は `float_index_out_of_range.rb`（期待値は master、`.rc.out` は
rc2。`rand` の答えは毎回変わるので `.class` で比べる形にして `.rc.out` が揺れないようにした）。

### `Integer#to_s`、`eval` の `__method__`・`super`（記録だけ）

`Integer#to_s(x)` は rc2 の `int_to_s` が `mrb_integer(arg)` で型を見ずに読む（2.5 は "invalid radix
-4607182418800017407" と負の基数になる。値の表現で答えが変わる）。SabiRuby は `mrb_as_int` で読む。1.5 は基数 1、
nil は TypeError。T1 で `expect_int` の文言が変わったので、nil の文言は "nil cannot be converted to Integer" になった。
`int_to_s_radix_type.rb`。

`eval("__method__")`、`eval("super(x)")`、`eval("defined?(super)")` は master `b0b1de54e` の答え（SabiRuby は前から
その答え）。`eval_method_super.rb`（期待値は master、CRuby とも一致）。

README の Known deviations に 3 項。book_mruby3 の `docs/notes/values-and-collections.md`（Array の添字の節）に、
キャストの未定義の動作と `int_to_s` の型を見ない読み方を 2 行。

## T3. 著者の判断

- `Struct#hash`: README の Known deviations の前の段落（意図した差）に 1 文。
- 凍結した特異クラス: 記録のためのテストを書いて、**表の前提と実際が違う**ことが分かった。表は SabiRuby を「どれも
  FrozenError」としていたが、`o.singleton_class.freeze; o.extend(Comparable)` は**通っていた**（表の `g6.rb` を
  base で流し直しても `frozen sc extend: :ok`）。`Kernel#extend`（`object.rs:71`）は受け手が凍結しているかしか見て
  いなかった。rc2 の `extend`（`mrb_include_module`）は特異クラスを見て拒むので、ここは rc2 にも CRuby にも合って
  いなかったことになる。著者の判断は「`def` も `extend` も拒む今のまま」で、意図は全部拒むことなので、`extend` で
  特異クラスの凍結も見るようにした（rc2 と同じ "can't modify frozen Class"）。`def`・`define_method` は元から拒む。
  `frozen_singleton_class.rb`（期待値 = CRuby と同じ）。
- `instance_exec`／`class_exec` の `__method__`: nil のまま。`instance_exec_method_name.rb`。最初に `.expected` を
  作ったとき、ループの出力を `head` で切ったせいで後ろの 2 本の `.expected` が書かれず、`tools/custom.sh` が作った
  rc2 の出力のまま残っていた（`:instance_exec` が SabiRuby の答えに見えた）。CLI で流し直して気づいた。

backlog: 整数の変換の行と rc6 の「`mrb_array_p` 以外の差」の行を消し、`__method__` の行から eval と instance_exec の
分を外した。T3 の 3 つは新しい節「著者が決めた本家との差」に。

## T4. 挙動の差でないもの

### 使われないネイティブ

表の 20（mrblib に上書きされる）と 20b（後のネイティブに上書きされる）。表は 37 と約 45 と書いていたが、
`deadnat/out.txt` の RUBY の行を数え直すと 38 だった（Array 16、Hash 8、String 6、Range 3、Numeric 2、Integer、
Proc、Complex）。20b は名前の突き合わせ（`string.rs` × `ext_string.rs` 18、`hash.rs` × `ext_hash.rs` 3、
`range.rs` × `ext_range.rs` 2、`numeric.rs` × `ext_numeric.rs` の `Integer#pow`、`proc_.rs` × `ext_proc.rs` 4、
`kernel.rs` の `local_variables`、`object.rs` の Object 7 と Module 8）で 44、それに `object.rs` の
`Class#class_variable_get`（`ext_metaprog::init` の最後の掃除が Class から消す）を足して 45。合わせて 83 を消した。
`Float#abs`（`numeric.rs`）は Numeric の `abs` と同じ名前だが別のクラスで、生きているので残した。regexp なしの
ビルドで生きる `string.rs` の 12（`[]`、`index`、`split` …）には触れていない。

消してよいことの確かめ方。名前の突き合わせだけでは「後から上書きされる」ことまでは分からない（上書きする側が
feature の中にあれば、そのビルドでは消した方が生きていた）。そこで、`Vm::new` + `load_mrblib` の後の全クラスの
メソッド表を `(クラス, 特異か, 名前, native/ruby/undef)` で書き出す使い捨ての crate（`l7/dn/`）を作り、4 つの
feature の組（`std utf8 regexp`、`std utf8`、`std regexp`、`std`）で消す前と後を比べた。4 つとも 1 行も違わない
（1371 行と 1308 行）。消した方が生きていたなら、その名前が表から消えるか、ruby に変わるはず。上書きした側が
消した側より**前**に登録されていた場合だけはこの比較で見えないが、`builtins::init`（`src/builtins/mod.rs:58`）は
core の `init` を全部呼んでから `ext_*` を呼び、どちらにも feature の囲いが無い（`ext_regexp` だけが
`load_mrblib` の中で cfg つき）ので、その場合は無い。

消した結果、使われなくなった関数が 4 つ（`hash_merge`、`object.rs` の `method_list`、`pad_chars`、`str_concat`）と
import 2 つ出たので消した（コンパイラの dead_code 警告）。4 つの feature の組で警告なしに建つ。`coverage.md` は
`tools/coverage.sh` で作り直し、変わったのは T1 で消した 2 つ（`private_class_method`、`public_class_method`）だけ
（1359 → 1357）。83 の削除が数を動かさないことも、使われていなかったことの裏付けになる。

`Vm::new` だけで mrblib を読まない VM では、mrblib に上書きされていた 38（`Array#sort`、`Hash#merge` など）が無く
なる。そういう使い方は `tests/malformed_rite.rs` だけで、そこは load の誤りを見るだけなので影響しない。CHANGELOG
に書いた。

### `FromRuby for Vec<T>`

`src/convert.rs` の `Vec<T>` は `ary_vals` で Struct・Data も Array として読み、"no implicit conversion into Array"
と言っていた。rustdoc は "An Array"。rc6 が足した `Vm::ensure_array`（`mrb_ensure_array_type`、`mrb_get_args` の
`A`）を呼ぶだけにした。文言は "S cannot be converted to Array"。`tests/convert.rs` の期待値を直し、Struct を渡す
1 行を足した。rubevy・rubevy_games・playground を再び grep した: `FromRuby` で `Vec<T>` を読むのは rubevy の
`tests/ask_value.rs` の `Vec<f64>`（普通の Array を渡す）だけで、Struct・Data を渡す所は無い。

## 確かめ

（数値は下の「数値」）

- `cargo test --workspace --release`、`cargo test -p sabiruby --no-default-features --features "std utf8"`（CI の
  regexp なし）、`--features "std regexp"`（bytes）: 全部通る。途中で落ちたのは期待値が文言を待っていた 3 か所
  （`tests/convert.rs`、`macros/tests/player.rs`、custom の 2 本）で、どれも新しい文言に直した。
- `tools/mrbtest.sh` の 3 ビルド: 2346/2508、2269/2453、1981/2012。どれも base と同じ（下がっていないし、上がっても
  いない）。生成した md は日付の行だけが変わったので戻した。
- `tools/check_no_std.sh --features regexp`／`--features utf8`: OK。`wasm32-unknown-unknown` の lib、`cargo check
  --workspace --all-targets`、`cargo doc` も警告なし。
- unsafe は増やしていない（`src/` の差分に `unsafe` は無い）。

## 数値

BENCH_PLACEHOLDER

## 気づいた点

- **sort に渡す組の順**（VM、`array.rs` の `sort_values`）: block が nil を返したときの文言で、rc2 は
  `[N.new, 2].sort { nil }` を "comparison of N with Integer failed"、SabiRuby は "Integer with N" と書く。rc2 は
  ヒープソート（`heapify`）、SabiRuby はマージソートで、block に渡す組とその順が違う。block が副作用を持てば
  呼ばれる回数と順も違う。文言だけを直すと並べ方とずれるので触っていない。揃えるなら並べ方を rc2 のヒープソートに
  する仕事（安定性も変わる）。
- **`Class` の `module_function`**（VM、`object.rs`）: rc2 は `Class` で `module_function` を undef する
  （`src/class.c:4761`）。SabiRuby の `Class.new { module_function }` は通る（rc2 は NoMethodError）。1 行の話だが
  計画の外なので直していない。
- **`String#hex`・`oct`**（VM、`ext_string.rs:359-360`）: まだ前の `string::to_i`（文字ごとの走査、接頭辞の扱いが
  rc2 の `mrb_str_len_to_integer` と違う）を呼ぶ。rc2 の `hex` は基数 16、`oct` は -8（接頭辞を読む）で
  `mrb_str_to_integer` を呼ぶ。`to_i` を写しに替えたので、こちらも同じ関数に替えれば揃うはず（未確認）。
- **`Float#to_i` などが引数を見ない**（VM、`numeric.rs` の `float_to_i`）: `to_i`・`to_int` は `argc!` が無く、
  `1.5.to_i(3)` が通る。rc2 は `MRB_ARGS_NONE`。計画の外。
- **表の誤り**（計画書の前提）: 表 #17 の「SabiRuby（28cbf01）: どれも FrozenError」は `extend` について誤り
  （通っていた、上の T3）。表 #20 の「37 個」は 38。
- **本の素材**: Float を整数に狭める C のキャストが CPU で答えを変える件（x86-64 と AArch64）は、移植の章で
  「同じ mruby でも機械によって答えが違う」例になる。notes に書いた（`values-and-collections.md`）。
