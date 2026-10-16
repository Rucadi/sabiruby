# 2026-10-16 rc2-leftovers4 — Hash の空いたエントリ（案 A）と、rc2 との小さな差

計画: `../plans/rc2-leftovers4-plan.md`（P1・P2）。方針: `../design/compatibility.md`。
著者の判断（2026-10-16）: Hash は案 A（master の答え）、小さな差は方針に従って対処。

この記録の順: P1（何を写したか、どこまで、確かめ方）、P2（項目ごと、途中で見つかったもの）、
確認（テスト、mrbtest、edge_compare）、性能、気づいた点。

## P1. Hash の探索中に `eql?` が一致したエントリを消す場合

### 本家の 3 つのコミットが直したもの

`/home/kishima/book/ref/mruby` の `git show` で読んだ（作業ツリーは rc2 のまま触っていない）。

- `a54353ecf`（`entry_key_eql`）: 鍵で探す道すべて — `ea_get_by_key`、`ar_get`、
  `ib_it_find_by_key`、`ht_set`。比べる前にエントリのキーを `stored` に取り、`obj_eql` が真を
  返したら、同じエントリがまだ `stored` を持っているか（`mrb_obj_eq`）見直す。違えば
  RuntimeError `hash modified`。
- `08a0432d1`（`entry_check_vacated` に切り出し）: 比べた後にエントリを読む走査 —
  `mrb_hash_assoc`（保存されたキーが `eql?` に答える）、`mrb_hash_rassoc`（保存された値が
  答える）、`mrb_hash_equal`／`mrb_hash_eql`（h1 のキーを h2 で探した後、h1 のエントリの値を
  読む前。見つからなければ見直さずに false）。
- `eb7693857`: 比べる以外の呼び戻しの後にエントリを読む走査 — `mrb_hash_to_s`（キーの
  `inspect` の後、値を読む前。Symbol のキーは見ない）、`mrb_hash_except_keys`（キーの `==` の
  たびに）、`ar_rehash`／`ht_rehash`（重複を探した後、動かすエントリ `r_entry` を見直す。HT の
  側は一致した方を `entry_key_eql` で、動かす方を空きのバケツでも一致でも見直す）。

### SabiRuby の形と、写し方

SabiRuby の `HashData` は穴の無い `entries` と、各エントリが本家の EA で何番目のスロットに
いるかの `slots` を持つ（`src/hashtable.rs` 冒頭）。本家の「同じアドレスのエントリがまだ同じ
キーを持つか」は、「同じスロット番号のエントリがまだ同じキーを持つか」（`HashData::slot_holds`、
`src/hashtable.rs:190`）に当たる。これは follow-master の M1 で `Found::Vacated` を見分けるのに
既に使っていた。消してから足すと新しいエントリは新しいスロット番号を取るので、見直しで
食い違う。compress（スロットの詰め直し）が挟まった場合、本家ではエントリが動いて同じアドレス
に別のキーが来るので例外、こちらでもスロット番号が付け直されて別のキーが来るので例外 —
同じ答えになる。

変えたところ:

- `Vm::h_check_vacated`（`src/hashtable.rs:496`）: `entry_check_vacated`。スロットがまだ
  `stored` を持てばその位置を、そうでなければ RuntimeError `hash modified`。
- `Vm::h_compare`（`:505`）: `entry_key_eql`。`Found::Vacated` を返していた道を例外に替えた。
  `Found::Vacated` は消し、`h_compare` は一致した位置だけを返す。`h_set`、`ht_set`、
  `h_delete`、`h_rehash`、`h_equal`、`Vm::hash_get`／`hash_index`（`src/vm.rs`）から
  `Vacated` の腕を消した。rc2 の答えを写すためだけにあった腕（`delete` が値を返して何も消さ
  ない、`[]=` が何も書かない）が無くなった。
- `Vm::h_value_checked`（`:910`）: 走査が呼び戻しの後にエントリの値を読むところ。前は
  `h_value_now` が「スロットが無ければ前に読んだ値」を返していた（rc2 の「空いたスロットを
  そのまま読む」を、範囲外を読まずに写したもの）。これを例外に替えた。
- 走査: `h_assoc`（`:937`、`eql?` が真の後）、`h_equal`（`:955`、h2 で見つかった後）、
  `h_except`（`:980`、`==` のたびに。本家と同じく `found` かどうかに関わらず）、
  `hash_inspect`（`src/builtins/hash.rs:34`、キーの `inspect` の後。Symbol のキーの道は呼び
  戻しが無いので見直しは素通り）、`h_rehash`（`:676`、重複探しの後に動かすエントリを見直す。
  前は `pos_of_slot` が無ければ `continue` で黙って飛ばしていた）。

master の 3 つのコミットの範囲だけにした。`h_merge`、`Hash#hash`、`each` 系、Set（`kh_get`）
など、3 つのコミットが触っていない走査には足していない。

### 確かめ方

スクラッチ `l4/probe.rb`（`tests/custom/hash_eql_vacated.rb` の元）を、rc2、891d1d9d、
158760853、CRuby 3.2.6（手元）と 3.4（`ruby:3.4`）、SabiRuby の前後で回した。

- 891d1d9d と 158760853 の出力は 1 バイトも違わない。
- SabiRuby（後）は master と同じ（`diff` が空）。
- SabiRuby（前）と rc2 の違いは、前の worklog の表どおり（HT と穴のある AR の `delete` で rc2
  は size が 1 減る、`__except` で rc2 は空いたキーに `hash` を送って NoMethodError、
  `rehash` で rc2 はエントリを 1 つ落とす）。
- CRuby は鍵で探す 12 通りのどれでも `eql?` を呼ばない（ハッシュ値 42 が Integer のキーの値と
  違うので比べに行かない）: nil／false／新しいエントリ、`:added` も足されない。走査は `==`、
  `eql?`、`inspect`、`rehash`（HT）が "can't add a new key into hash during iteration"、
  `assoc`／`rassoc` は nil。案 B（CRuby の答え）を採らなかった理由の補強になる: CRuby では
  この形が起きない。

### 記録（方針の 3 か所）

1. README "Known deviations" に 1 項目（rc2 の関数名、master の 3 コミット、CRuby、テスト）。
2. `tests/custom/hash_eql_vacated.rb`: `.expected` は 158760853 の出力（891d1d9d と同じ）、
   rc2 の答えはヘッダのコメントと `.rc.out`（`tools/custom.sh` で作った）。
3. `book_mruby3/docs/notes/values-and-collections.md` の「Hash が `hash` / `eql?` を呼ぶ条件」
   に 1 行（rc2 の見張りが気づかない理由、AR の先頭に穴が無いときだけ例外になる理由、master の
   直し、CRuby、SabiRuby の選択）。

`tests/edge-spec/notes.tsv` の `hash.rb` の行は消した。891d1d9d の `hash.rb` は 49 個すべて
通るようになり、説明することが残らないため（下の edge_compare）。

## P2. 小さな差

### `slice!` の凍結 Array の文言 → 直した（文言の元を直した）

調べると `slice!` だけの話ではなかった。`Vm::frozen_error`（`src/vm.rs:4933`）が
`can't modify frozen {class}: {inspect}` を作っていた。最初のコミット（`eb6aa28`）からこの
形で、CRuby の文言。rc2 の `mrb_frozen_error`（`src/error.c:654`）は
`"can't modify frozen %T"`（クラス名だけ）。スクラッチ `l4/frz.rb` で 35 通り比べると、
`frozen_error` を通る 14 通り（Array の `slice!`・`compact!`・`flatten!`・`rotate!`・`insert`、
Hash の `delete`・`merge!`・`rehash`・`compact!`、Object の ivar、`def o.x`、`extend`、
Struct、Class）で `: …` が余計だった。`inspect` を呼ぶので、利用者の `inspect` が例外の
文言を作るために走ってもいた。`frozen_error` から `inspect` を外した。

同じ表で、どのクラスを名指すかの違いも 2 つ見つかった（同じ種類の文言の差としてまとめた）:

- `def o.x`／`o.singleton_class.define_method` を凍結した Object に: rc2 は `Object`、
  SabiRuby は `Class`。rc2 の `mt_writable`（`src/class.c:1004`）は、凍結した特異クラスなら
  くっついている物の凍結を調べる（`mrb_check_frozen_value(__attached__)`）。
  `Vm::mt_frozen_error`（`src/vm.rs:4922`）を足し、メソッド表に書く 3 か所（`undef_method`、
  `def_method_nohook`、`set_visibility`。本家の `mrb_define_method_raw` と visibility の
  `mt_writable`）で使う。名指しを変えただけで、例外にするかどうかは変えていない
  （本家は「特異クラスは凍結、くっついた物は凍結していない」ときに例外にしない。この食い違いは
  `singleton_class.freeze` でしか作れないので、記録だけ）。
- `extend` を凍結した Object に: rc2 は `Class`（`mrb_include_module` が特異クラスを
  `mrb_check_frozen` する、`src/class.c:2175`）、SabiRuby は `Object`。`extend` で特異クラスを
  名指すようにした（`src/builtins/object.rs:79`）。

残った違い: 即値への `instance_variable_set` は rc2 が ArgumentError "cannot set instance
variable"、SabiRuby は FrozenError。追うと文言でなく挙動の差だった（下の「インスタンス変数」）。

### `s["z"] = …` の文言 → 直した

rc2 の `mrb_str_aset`（`src/string.c:2167`）は `str_convert_range` が `STR_OUT_OF_RANGE`
を返すと `"string not matched"`。String が見つからないときに加えて、Range が文字列の外の
ときもこの文言になる（`s[9..10] = "X"`）。SabiRuby は前者に `: "z"` を付け、後者は
`index 9..10 out of string` だった。両方を `string not matched` にした
（`src/builtins/string.rs:725`、`:757`）。`mrb_as_int` を通った添字（Rational など）が外れた
ときは本家も `str_out_of_index` なので、その腕は `index … out of string` のまま。

### `Array#delete_at` と `MatchData#[]` の整数変換 → 直した

`delete_at`（`src/builtins/array.rs:308`）と、`MatchData#[]` の 1 引数・2 引数
（`src/builtins/ext_regexp.rs:1191`、`:1214-1215`）を `numeric::as_int`（`mrb_as_int`）に
替えた。nil、String、NaN、Infinity、2**70、1.5 で rc2 と同じ（スクラッチ `l4/sa.rb`、
`tests/custom/rc2_leftovers4.rb` と `rc2_leftovers4_regexp.rb`）。計画は 2 引数の `MatchData#[]`
だけを挙げていたが、1 引数（`md_index`）も rc2 は `mrb_as_int`（nil で "nil cannot be converted
to Integer"、NaN で RangeError）だったので一緒に替えた。

### `array.rs` の `slice!` → 消した

`builtins::init`（`src/builtins/mod.rs:58`）は `array::init` の後に `ext_array::init` を
無条件に呼び、`define_methods` は同じ名前を上書きする（`src/vm.rs` の `define_method`）。
`ext_array.rs` の `slice!` は feature で囲まれていない。よって `array.rs` の `slice!` は登録の
直後に上書きされ、どこからも呼ばれない。消してもテストはすべて通る（下）。

同じように上書きされて使われない名前が `array.rs` にあと 15 ある（`&`、`-`、`|`、`assoc`、
`at`、`compact`、`compact!`、`flatten`、`flatten!`、`include?`、`member?`、`insert`、
`rassoc`、`rotate`、`values_at`）。計画が挙げたのは `slice!` だけなので、backlog に 1 行。

### 整数変換の一覧（同じ種類で数が多かったもの → backlog）

`expect_int` の残りを見るために、整数を取りそうな 84 の呼び方に nil、NaN、`"1"`、1.5、2**70
を渡して rc2 と比べた（スクラッチ `l4/conv.rb`。`cycle`、`sleep`、`upto` は nil や 2**70 で
終わらないので外した）。68 で違った。rc2 の `mrb_get_args("i")` は `mrb_as_int`
（`src/class.c:1447-1449`）で、SabiRuby の `Vm::expect_int`（`src/vm.rs:1351`）は CRuby の
文言（"no implicit conversion of nil into Integer"、FloatDomainError）なので、文言だけの差が
大半。`expect_int` の呼び出しは約 110 か所あり、`mrb_as_int` に一律に替えてよいか（
`mrb_ensure_int_type` や `to_int` を見る所が混じる）を 1 つずつ確かめる仕事になるので、
計画の「数が多ければ一覧にして backlog に残す」に従った。一覧（ラベルは試した呼び方）:

文言だけ違うもの（43）: `Array#at`、`Array#first`、`Array#last`、`Array#shift`、
`Array#rotate`、`Array#rotate!`、`Array#insert`、`Array#fetch`、`Array#take`、`Array#drop`、
`Array#flatten`、`Array#combination`、`Array#permutation`、`Array#fill(0, x)`、
`Array#sample`、`Array#dig`、`String#*`、`String#center`、`String#ljust`、`String#rjust`、
`String#insert`、`String#getbyte`、`String#setbyte`、`String#index(s, x)`、`String#rindex`、
`String#byteindex`、`Integer#round`、`Integer#digits`、`Integer#ceil`、`Float#round`、
`Random.new`、`srand`、`Range#last`、`Struct#[]`、`MatchData#begin`、`MatchData#end`、
`MatchData#offset`、`Regexp#match`、`String#match(re, x)`、`Time.new`、`Array#pack`、
`sprintf %d`、`sprintf %*d`

値や例外のクラスまで違うもの（24。文言だけの差の引数は略）:

| 呼び方 | rc2 / SabiRuby |
|---|---|
| `Array#pop` | rc2 の `pop` は引数を取らない（ArgumentError）。SabiRuby は `pop(n)` を受ける |
| `Array#*` | rc2 は 1.5 を `mrb_as_int` で 1 と読み `[1, 2]`。SabiRuby は Integer 以外を区切りの文字列として読み、"argument cannot be converted to String" |
| `Array#values_at` | rc2 は nil、NaN、`"1"`、1.5 に TypeError "invalid values selector: …"。SabiRuby は Integer として読む（1.5 → `[2]`） |
| `Array#each_slice`、`Range#first`、`Enumerable#first`／`take`／`each_slice`／`each_cons`、`Hash#first`、`Enumerator#first` | rc2 は 2**70 で RangeError "integer out of range"。SabiRuby は通す |
| `Array.new` | rc2 は NaN、2**70 で RangeError、1.5 で `[nil]`。SabiRuby は "no implicit conversion into Integer" |
| `String#setbyte(0, x)` | rc2 は 1.5 を 1 にして 1 を返す。SabiRuby は 1.5 を返す |
| `String#to_i` | 1.5: rc2 "illegal radix 1"、SabiRuby "invalid radix 1" |
| `String#split` | 1.5: rc2 `["a b"]`（上限 1）、SabiRuby `["a", "b"]` |
| `Integer#to_s` | rc2 は nil・`"1"`・2**70 で "invalid radix 0"、NaN・1.5 でビット列を基数として読む。SabiRuby は型の例外 |
| `Integer#<<` | 1.5: rc2 2、SabiRuby "can't convert Float into Float" |
| `Float#floor`／`ceil`／`truncate` | rc2 は桁を `mrb_as_int` で読む（nil で TypeError、1.5 は 1 桁で 1.5）。SabiRuby は引数を見ない |
| `Kernel#rand`、`Random#rand` | NaN: rc2 は Integer を返す、SabiRuby は Float |
| `Array#shuffle(random: x)` | rc2 は `random:` に何を渡しても通す。SabiRuby は "Random object required" |
| `Integer()` | 2**70: rc2 はそのまま返す。SabiRuby は "can't convert Integer into Integer"（下の気づいた点） |

この表の値の差は 1 つずつ本家のソースを読んではいない。rc2 と SabiRuby の出力を並べただけ。

### インスタンス変数を持てない値（止めた → backlog）

即値の文言を追う途中で、rc2 の `mrb_iv_set`（`src/variable.c:733`）は `obj_iv_p`
（`:546`。Object、Class、Module、特異クラス、Hash、Data、Exception）にしか書かないと分かった。
String、Array、Range、Proc、大きな Integer への `instance_variable_set` は ArgumentError
"cannot set instance variable"、`instance_variable_get` は nil、`instance_variables` は `[]`
（スクラッチ `l4/iv.rb`）。SabiRuby はどの heap オブジェクトにも書ける。文言でなく挙動の差で、
String や Array のサブクラスで `@x = 1` と書いたスクリプトも例外になるので、ホスト（rubevy
など）の使い方を見てから著者が決めるものとして止めた。backlog に 1 行。

## 確認

- `cargo test --workspace`: 38 スイート、275 passed、0 failed（`custom_cases` に新しい 3 件）。
- `cargo test -p sabiruby --no-default-features --features "std utf8"`: 161 passed、0 failed。
  最初は `rc2_leftovers4.rb` が MatchData を使っていて `custom_cases` が落ちた
  （"uninitialized constant Regexp"）。MatchData と Regexp の行を
  `rc2_leftovers4_regexp.rb`（`# regexp-only:`）に分けて通した。
- `tools/check_no_std.sh --features regexp`、`--features utf8`: no_std OK。
- `cargo build -p sabiruby --lib --target wasm32-unknown-unknown`: 通る。
- `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --workspace --lib` と CLI の `--bins`: 通る。
- rc2 の mrbtest（`tools/mrbtest.sh`、Docker）: 既定 2346/2508、バイト列 2269/2453、
  regexp 無し 1981/2012。3 つとも前と同じ数で、表は日付の行しか変わらなかったので
  `docs/verification/mrbtest*.md` は戻した。
- `unsafe` は増やしていない。`Vm: Send + Sync`（`tests/send_sync.rs` は上の cargo test に入る）。
- 新しい数は無い。

## edge_compare

`tools/edge_compare.sh`（891d1d9d の `test/t`、mruby/edge の数え方）:

| | 前（`499f124` の JSON） | 後 |
|---|---|---|
| C の fixture 無し | pass 855、fail 36、error 34、skip 2（passed 29 ファイル） | pass 858、fail 33、error 34、skip 2（passed 30） |
| C の fixture あり | pass 880、fail 46、error 31、skip 2（passed 29） | pass 883、fail 43、error 31、skip 2（passed 30） |

変わったのは `hash.rb` だけ（46/3 → 49/0）。vacated の 3 つの assert。JSON とページは
コミット `8c32b3b` の後にもう一度測ったもの（`dirty: false`、数は上と同じ）。

## 性能

base は `499f124`（`git worktree add --detach` の別ディレクトリ、別の `CARGO_TARGET_DIR`、md5
`8da8acaf…`）、new は作業ツリー（md5 `4ac5223e…`）。`tools/bench_ab.sh --core 2 --runs 5`、
27 本、向きを入れ替えて 2 巡、それと new とそのコピーの A/A を 1 巡。始める前の `vmstat` の idle は
98〜99%、`pgrep` で他の sabiruby・cargo・mruby は無し。表は best 同士の「new が base より何 %」:

| benchmark | 1 巡目 | 2 巡目 | A/A |
|---|---|---|---|
| ds_hash | +0.0% | -1.0% | +0.6% |
| app_json_hash | -2.0% | -0.8% | +1.3% |
| app_tak | +3.3% | +4.4% | -0.6% |
| bm_fib | +1.9% | +2.6% | -0.0% |
| call_args | +2.9% | +2.7% | -0.0% |
| bm_ao_render | +1.8% | +2.5% | -0.1% |
| bm_so_mandelbrot | +3.2% | +2.0% | +0.4% |
| 全体（27 本の合計） | +1.1% | +1.0% | -0.3% |

計画が挙げた Hash の 2 本（`ds_hash`、`app_json_hash`）は A/A の幅の中で、遅くなっていない。
P1 の見直しは、Ruby の `eql?` が真を返した後（めったに通らない道）にスロットを 1 度二分探索する
だけなので、そうなるはず。

一方、呼び出しの多い本（`app_tak`、`bm_fib`、`call_args`）が 2 巡とも 2〜4% 遅い。A/A は 0% 前後
なので、ぶれではない。`app_tak`、`bm_fib`、`call_args` だけを `--runs 9` でもう 1 巡（+3.2%、
+2.2%、+2.9%）。原因を 1 つずつ外して試した（どれも 3 本、向きを入れ替えて 2 巡、`--runs 7`）:

- **`frozen_error` の本体だけを前に戻したもの**（`inspect` を呼ぶ形。ほかの変更、P1 も
  `mt_frozen_error` もすべて入ったまま）: `app_tak` +0.0%／-0.1%、`bm_fib` -1.0%／+0.5%、
  `call_args` +0.9%／-0.1%。遅さが消えた。よって遅さは `frozen_error` から `inspect_str` を
  外したことだけから来ている。
- `frozen_error` に `#[cold] #[inline(never)]`: +2.5%／+0.1%、+3.1%／+1.0%、+1.3%／+2.8%。
  はっきり消えない。
- `#[inline(never)]` だけ: +3.7%／+2.3%、+2.3%／+1.5%、+1.4%／+2.4%。消えない。

`frozen_error` が呼ばれるのは凍結したオブジェクトへの書き込みのときだけで、ベンチでは一度も
通らない。`inline(never)` でも消えないので、`frozen_error` が命令ループに入り込んだからではなく、
`inspect_str` を呼ぶ所が 1 つ減ったことでコンパイラの他の関数のインライン化の判断が変わった
（コードの配置の差）と考えるが、確かめてはいない。どちらの属性も効かなかったので戻した
（コードは属性なし）。文言を rc2 に合わせることと 2〜3% のどちらを取るかは著者の判断として
報告する。直すなら、配置の差を追う別の仕事になる。

## 気づいた点

- **バグ（VM）**: `Integer(2**70)` が TypeError "can't convert Integer into Integer"。rc2 は
  そのまま返す。`src/builtins/ext_kernel.rs:281-300` の `Integer()` は `Value::Int` の腕しか
  Integer と見ず、大きな Integer（heap の BigInt）は最後の `mrb_ensure_integer_type` の腕に
  落ちる。再現: `p Integer(2**70)`。直していない。
- **性能**: 上の「性能」の 2〜3%（`frozen_error` の文言の変更で、呼び出しの多いベンチが遅く
  なる。原因は配置の差と考えるが未確認）。
- **VM / rc2 との差**: 整数変換の一覧と、インスタンス変数を持てない値（上、backlog に 2 行）。
- **VM / rc2 との差（記録だけ）**: 凍結した特異クラスに、凍結していない物がくっついている
  とき（`o.singleton_class.freeze`）、rc2 の `mt_writable` はメソッドの定義を通す。SabiRuby は
  特異クラスの凍結で例外にする。作るのに `singleton_class.freeze` が要るので直していない。
- **死んだコード**: `array.rs` の上書きされる 15 個（backlog に 1 行）。
- **backlog の既存の行**: 「hash modified を世代の数で見る」（`eql?` の中で 1 つ消して 1 つ
  足すと気づかない）は、消したのが今比べているエントリなら P1 で気づくようになった。別の
  エントリを消して足す場合は、master も気づかないので今のまま。行の文言は触っていない。
- **本の素材**: rc2 の Hash の見張り（`H_CHECK_MODIFIED`）が「容量・ポインタ・要素数」しか見ない
  ので、消して足すと気づかない、AR は word boxing のせいで先頭に穴があるときだけ漏れる、という
  話は notes に 1 行足した。`mrb_frozen_error` が `inspect` を呼ばない（呼び戻しを走らせない）
  のは、CRuby との違いとして本で触れる価値があるかもしれない。
