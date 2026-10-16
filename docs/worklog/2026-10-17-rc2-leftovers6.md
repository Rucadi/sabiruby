# 2026-10-17 rc2-leftovers6: Struct を Array として扱う Array のメソッドと、片付け

計画: `../plans/rc2-leftovers6-plan.md`。差の出どころは `2026-10-17-rc2-leftovers5.md`「気づいた点」。
方針は `../design/compatibility.md`（本家に合わせる）。正解は 4.1.0-rc2（Docker
`kishima/mruby:4.1.0-rc2`、ソースは `/home/kishima/book/ref/mruby` の `c17ffcc24`）。base は `d8cbd83`。

本家を不具合とみなして外れたものは無い（README の Known deviations は増えない）。

## S1. Array のメソッドが Struct と Data を Array として扱う

### 洗い出し

rc5 の R1 と同じく、差の根は「Struct と Data が `ObjKind::Array` の形を借りている」こと。Array の
形を読む口（`Vm::ary`、`Vm::ary_vals`、`Heap::array`、`ext_array::check_array`、`Vm::to_array`）を
`src/` で grep すると、builtins と vm に約 80 か所あった。受け手（`self`）を読むものは、Array の
メソッドが Struct を受け手に呼ばれることが無いので関係ない。引数や要素を「Array か」で分ける所だけ
を拾った。

rc2 の側は `src/*.c` と `mrbgems/*/src/*.c` で `mrb_array_p`・`mrb_check_array_type`・
`mrb_ensure_array_type`・`mrb_check_type(..., MRB_TT_ARRAY)`・`mrb_get_args` の `A`／`a` を引いた。
効くのは次の 4 つで、どれも型タグ（`MRB_TT_ARRAY`）しか見ない。

- `mrb_ensure_array_type`（`src/object.c:789`）: Array でなければ TypeError "%Y cannot be converted
  to Array"。`mrb_get_args` の `A` と `a`（`src/class.c:1437`、`:1487`）はこれ。
- `mrb_check_array_type`（`src/object.c:806`）: Array でなければ nil。**`to_ary` は送らない。**
- `mrb_check_type(x, MRB_TT_ARRAY)`（`src/object.c:485`）: "wrong argument type X (expected
  Array)"。X は nil、Integer、Symbol、他の即値は `to_s`、オブジェクトはクラス名。
- `mrb_array_p` の直の判定: `ary_eq`（`src/array.c:1949`）、`mrb_ary_cmp`（`:2049`）、
  `mrb_ary_init`（`:470`）、`mrb_ary_splice` の置き換え（`:1200`）、`join_ary`（`:1819`）、
  `mrb_ary_splat`（`:1678`）、`mrb_obj_ceqq`（`src/kernel.c:653`）、`set_backtrace`
  （`src/error.c:155`）、`flatten_internal`・`rassoc`（mruby-array-ext）、`Hash.[]`
  （mruby-hash-ext `hash_ext.c:211`）、`io_puts`（mruby-io `io.c:1031`、`:1062`）、`regexp_union`
  （`regexp.c:1066`）。`Kernel#Array` は `mrb_type_convert_check(arg, MRB_TT_ARRAY, to_a)`
  （mruby-kernel-ext `kernel.c:256`）で、型が Array でなければ `to_a`、答えが Array でなければ
  包む。

### rc2 の答えの表

スクラッチ `l6/probe.rb`（Struct・Data・Array のサブクラス × 78 の式）、`probe2.rb`（同じ式を
Integer・`to_ary` を持つオブジェクト・普通の Array で）、`probe3.rb`（Array.new、set_backtrace、
splat、Struct と Data 自身のメソッド）、`splat.rb`（`to_a` が Array でないものを返す）を rc2 と比べた。
前の SabiRuby（`d8cbd83`）との違いが出た所をまとめる（s は `Struct.new(:x, :y).new(1, 2)`、d は
`Data.define(:x, :y)` の同じ値）。

| 式 | rc2 | SabiRuby（前） |
|---|---|---|
| `[1] + s`、`[1, 2].concat(s)`、`[3, 4].replace(s)` | TypeError "S cannot be converted to Array" | 中身を使う |
| `[1, 3] - s`、`\|`、`&`、`intersect?` | 同じ TypeError | 中身を使う |
| `union`、`difference`、`intersection` | TypeError "can't convert passed argument to Array" | 中身を使う |
| `[3, 4].product(s)` | TypeError "wrong argument type S (expected Array)" | 中身を使う |
| `[s].flatten`、`flatten!` | `[s]` | `[1, 2]` |
| `[1, 2] == s`、`eql?` | false | true |
| `[3, 4] <=> s` | nil | 1 |
| `[s].assoc(1)`、`rassoc(2)` | nil | s |
| `[s, 3].join("-")` | `"#<struct S x=1, y=2>-3"` | `"1-2-3"` |
| `a[0, 2] = s`、`a[0..1] = s` | s が 1 要素で入る | 1、2 が入る |
| `Array.new(s)`、`Array.new(s, 0)` | TypeError "S cannot be converted to Integer" | `[1, 2]` |
| `[[1, 2]].include?(s)`、`index`、`delete` | false／nil／nil | true／0／`[1, 2]` |
| `[s].sum([])` | TypeError（`[] + s`） | `[1, 2]` |
| `Hash[[s]]` | ArgumentError "wrong element type S (expected array)" | `{1 => 2}` |
| `Array(s)` | `[1, 2]`（`to_a`） | s |
| `puts s` | `#<struct S x=1, y=2>` | 1 と 2 の 2 行 |
| `e.set_backtrace(s)` | TypeError "backtrace must be Array of String" | 受け取る |
| `[*d]`、`case 1; when *d`、`format("%s-%s", *d)` | `[d]`、miss、ArgumentError（Data に `to_a` は無い） | `[1, 2]`、hit、`"1-2"` |

同じ道を通る、Struct でない値の差も出た（`probe2.rb`）。

| 式 | rc2 | SabiRuby（前） |
|---|---|---|
| `[1] + 1`、`concat(1)` | TypeError "Integer cannot be converted to Array" | "no implicit conversion into Array" |
| `[3, 4].replace(1)` | 同じ TypeError | `[]`（受け手を空にしていた） |
| `[1, 2].concat([0], [3])` | ArgumentError（`MRB_ARGS_REQ(1)`） | 両方つなぐ |
| `[3].product(1)` | "wrong argument type Integer (expected Array)" | "wrong argument type (expected Array)" |
| `[1, 3] - ta`（`to_ary` を持つ）、`\|`、`&`、`union` など | TypeError（`to_ary` は送らない） | `to_ary` で変換 |
| `Array.new([1, 2], 0)`、`Array.new([1, 2]) { }` | TypeError "Array cannot be converted to Integer" | `[1, 2]` |
| `Array.new(2.5)`、`Array.new(nil)` | `[nil, nil]`、"nil cannot be converted to Integer" | "no implicit conversion into Integer" |
| `e.set_backtrace(1)`、`(nil)`、`("a")`、`(SubA[1, 2])` | TypeError "backtrace must be Array of String" | 受け取る |
| `[*obj]`、`when *obj`（`to_a` が 1 や Struct を返す） | TypeError "Integer cannot be converted to Array"／"S cannot …" | "can't convert B to Array (B#to_a gives Integer)"／Struct を展開／`when` は nil |
| `Array(obj)`（`to_a` が 1 を返す） | `[obj]` | TypeError |

計画は「Array を引数に取る所・Array かどうかで分ける所を rc2 に合わせる」なので、Struct でない
値の差も、同じ関数（`mrb_ensure_array_type` などの写し）を通る所はまとめて直した。直さなかったのは
別の関数の話になるもの（「気づいた点」）。

### 写し方

`Vm` に 3 つ足した（`src/vm.rs:1392` 付近、`pub(crate)`）。

- `plain_ary`／`plain_ary_vals`: `Heap::plain_array`（rc5 の `mrb_array_p`）を `Value` から読む。
- `ensure_array`: `mrb_ensure_array_type`。

`ext_array.rs` の `check_array` は `to_ary` を送っていた（2026-09 の移植 `1b2daa9` から。rc2 の
`mrb_check_array_type` は送らない）ので、`plain_ary_vals` だけにした。`ary_arg` は
`ensure_array`、`check_type_array` を足して `product` の 2 か所の文言を `mrb_check_type` に
合わせた。

直した所（rc2 の関数との対応はコードのコメントに 1 行ずつ）:

- `array.rs`: `==`（`ary_eq_inner`）、`<=>`、`eql?`、`initialize`（1 つの Array で block 無しだけ
  写し、他は `numeric::as_int` で大きさ）、`initialize_copy`・`replace`（`ensure_array`）、
  `concat`（引数 1 つ、`ensure_array`）、`+`、`join` の入れ子、`[]=` の置き換えの 2 か所。
- `ext_array.rs`: `check_array`、`ary_arg`、`flatten_internal`、`rassoc`、`product_fetch` と
  `__product_generate`。
- `vm.rs` の `to_array`（`mrb_ary_splat`。splat、ARYCAT、ARYSPLAT が通る）: 先の判定を
  `plain_ary_vals`、`to_a` の答えを `ensure_array`。
- `kernel.rs`: `__case_eqq`（`mrb_obj_ceqq`）、`puts_value`。
- `ext_kernel.rs`: `Kernel#Array`（`to_a` が Array でなければ包む）、`Kernel#Hash` の空の Array。
- `ext_hash.rs` と `hash.rs` の `Hash.[]`、`object.rs` の `flat_syms`、`ext_regexp.rs` の
  `Regexp.union`、`exception.rs` の `set_backtrace`（Array の要素が全部 String か）。

`Vm::ary`／`ary_vals`（ホストの API）は変えていない。Struct と Data のメソッドがこれで自分の中身を
読む。`Vm::to_array` は `pub` で答えが変わる（Struct は `to_a` で同じ中身、Data は 1 要素）。
rubevy・rubevy_games・playground を `\.to_array(` で grep すると、当たるのは bevy の
`Vec3::to_array` だけで、`Vm::to_array` を使う所は無かった。`FromRuby for Vec<T>`
（`src/convert.rs:150`、ホストの関数の引数）は `ary_vals` のままにした（「気づいた点」）。

### 確かめ

直した後、`probe.rb` の差は 4 行（`sort` の文言 2、`Struct#hash`、Data を splat した `values_at` の
文言）、`probe2.rb` は 9 行（うち 5 行は `#<TA:0x…>` のアドレス、残りは `sort` と `values_at` の
文言）、`probe3.rb` は 9 行（`raise` の 3 引数、`private` と `private_class_method`、無名クラスの
文言、`Struct#hash`）。残りはどれも `mrb_array_p` の話ではない（「気づいた点」と backlog）。

Struct と Data 自身のメソッド（`to_a`、`==`、`each`、`values`、`deconstruct`、`dig`、`values_at`、
`to_h`、`with`、`a, b = *s`）は rc2 と同じ答え。rc2 mrbtest の `gem_struct` 41/41、`gem_data` 14/14。

`tests/custom/struct_not_array_arg.rb`: Struct・Data・Array のサブクラス・Integer・`to_ary` を
持つオブジェクトの 5 つ × 34 の式と、単独の 37 行（223 行）。`.expected` は rc2 の出力
（`tools/custom.sh` の `.rc.out` と一致）。最初は `Hash(Struct.new(:a).new(nil))` を入れて、
無名クラスの `#<Class:0x…>` がアドレスごと出たので、名前のある Struct に替えて作り直した。

## S2. 片付け

### `object.rs` の 4 つ

`ext_metaprog::init`（`src/builtins/ext_metaprog.rs:255-263`）は Object のネイティブを全部 Kernel へ
移し、Kernel に同じ名前があれば Object の分を捨てる。`instance_variable_get`／`set`／`defined?`／
`instance_variables` は Kernel に `iv_check` つきで置かれる（`:94-98`）。`builtins::init` は
`object::init` の後に `ext_metaprog::init` を無条件に呼ぶ（`src/builtins/mod.rs:58-76`、feature の
囲いなし）。よって `object.rs` の 4 つは登録の直後に捨てられる。消した。

### `array.rs` の 15 個

`define_methods` の名前を `array.rs` と `ext_array.rs` で突き合わせ、重なるのが backlog の 15 個
（`&`、`-`、`|`、`assoc`、`at`、`compact`、`compact!`、`flatten`、`flatten!`、`include?`、
`member?`、`insert`、`rassoc`、`rotate`、`values_at`）と一致することを確かめた。`ext_array::init` も
無条件。消した。`flatten` の補助関数も使われなくなったので消した（コンパイラの dead_code 警告）。
`include?` の前にあった「as `index` above」のコメントも一緒に。

coverage.md の数（下）が変わらないことが、使われていなかったことの裏付けになる: 消した 19 個は
どれも同じ名前が別の定義で残っている。

### `docs/verification/coverage.md`

`tools/coverage.sh` で作り直した。前は 2026-09-16、4.1.0-rc のイメージで作られていた。差分は:
参照のイメージが rc2 になった、SabiRuby 1357 → 1359 メソッド、両方にある 1201 → 1204、
「reference にだけある」344 → 341（`Enumerable#__update_hash` と `.__update_hash`、
`Kernel#__method_recursive?` が SabiRuby にもできた。どれも 9 月 16 日以降の仕事）、
「SabiRuby にだけある」156 → 155（`Kernel#instance_variable_names`、rc5 の R4）。

## 確認

- rc2 の mrbtest（`tools/mrbtest.sh`、Docker）: 既定 2346/2508、`--bytes` 2269/2453、`--no-regexp`
  1981/2012。3 つとも前と同じ。生成された表は日付の行しか変わらなかったので戻した。
- `cargo test --workspace`: 38 のテストバイナリ、275 passed、0 failed（`custom_cases` は 1 つの
  テストで `tests/custom/*.rb` を全部回すので、新しい 1 件で数は増えない）。
- `cargo test -p sabiruby --no-default-features --features "std utf8"`: 161 passed、0 failed。
- `tools/check_no_std.sh --features regexp`、`--features utf8`: no_std OK。
- `cargo build -p sabiruby --lib --target wasm32-unknown-unknown`: 通る。
- `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --workspace --lib`、`-p sabiruby-cli --bins`: 通る。
- MSRV: `cargo +1.85 check --workspace --all-targets`（別の target）: 通る。
- `unsafe` は増やしていない。`Vm: Send + Sync`（`tests/send_sync.rs`）。新しい数は無い。

## 性能

base は `d8cbd83`（`git worktree add --detach` の別ディレクトリ、別の `CARGO_TARGET_DIR`、md5
`522c411f…`）、new は作業ツリー（md5 `d5089f38…`）。`vmstat` で 99% idle、他の cargo・sabiruby は
無し。

`tools/bench_ab.sh --core 2 --runs 5`（全体）: all 47055 → 47030 ms（-0.1%）。whole program +0.6%、
data structures -0.9%、instruction loop -0.7%、calls -1.1%、memory -0.2%。計画が挙げた Array の
速い道は `ds_array` -0.7%、`vmo_index` -3.6%。

+1.5% を超えたものを 10 巡で取り直した: `mem_retained` +1.2%、`vm_optimization_bench` -0.3%、
`ds_hash` +3.4%、`bm_mandel_term`（19 ms）+3.3%、`ds_array` +1.9%。`ds_hash` と `ds_array` を 20 巡:
`ds_hash` best +1.7%、median は 230.3 → 229.7、`ds_array` best -0.2%、median 843.3 → 830.9。
`ds_hash` は `Hash#[]=`、`[]`、`key?`、`each { |k, v| }` で、変えた所（引数の Array 判定、
splat）を通らない。ぶれの範囲と読む。速い道（`OP_AREF` 系、`idx_op`、`ary_entry`）は触っていない。

結果の tsv はコミットしていない（最近の記録と同じ。数はここに書いた）。

## edge_compare

`tools/edge_compare.sh`: C の fixture 無し pass 858、fail 33、error 34、skip 2（passed 30 ファイル）、
あり pass 883、fail 43、error 31、skip 2（passed 30）。前と同じで、変わったのは commit・日時・
`elapsed_seconds` だけだったので、JSON とページは戻した。

## 気づいた点

- **VM / rc2 との差（記録だけ、backlog）**: 調べの中で出た、`mrb_array_p` 以外の差。`sort` の比較の
  失敗の文言（rc2 "comparison failed"、SabiRuby "comparison of S with Array failed"）。`values_at` の
  Integer でない選び方（rc2 "invalid values selector: a"、SabiRuby "no implicit conversion of String
  into Integer"）。`raise RuntimeError, "m", bt`（rc2 の `raise` は `MRB_ARGS_OPT(2)` で
  ArgumentError、SabiRuby は bt を backtrace にする）。`private [:a]`（rc2 は "wrong argument type
  Array (expected Symbol)"、SabiRuby は展開する）と `private_class_method`／`public_class_method`
  （rc2 に無い、NoMethodError。`flat_syms` は `object.rs:627`）。`Struct#hash`（rc2 は
  `[1, 2].hash` と等しい）。名前の無いクラスを `%Y` で書く文言（`Regexp.union(無名の Struct)` が
  rc2 では `#<Class:0x…> cannot be converted to String`、SabiRuby は "pattern cannot …"）。
- **VM（内部、backlog）**: mrblib に上書きされて使われないネイティブがまだある。`array.rs` の
  `fill`（`[1, 2, 3].fill(0, 1)` が `[1, 0, 0]` なので mruby-array-ext の mrblib が答えている）、
  `transpose`、`to_h`、`product`（どれも rc2 の `mrbgems/mruby-array-ext/mrblib/array.rb` が
  定義する）。`object.rs` の `methods`・`public_methods`・`private_methods`・`protected_methods`・
  `singleton_methods`・`public_send` なども `ext_metaprog` に Kernel で上書きされる。
- **公開 API（記録だけ）**: `FromRuby for Vec<T>`（`src/convert.rs:150`）は `ary_vals` で、Struct を
  Array として受け取る。ホストの関数の引数の型の話で rc2 の関数には対応しないので変えていない。
  `mrb_get_args("A")` に寄せるなら `plain_ary_vals` にすることになる。
- **本の素材**: `mrb_check_array_type` は名前に反して変換をしない（`to_ary` を送らない）。
  SabiRuby の移植は 9 月にこれを CRuby の `rb_check_array_type` の感覚で `to_ary` つきに書いていて、
  `[1] - obj` が `obj.to_ary` で通っていた。本文の「暗黙の型変換（`to_ary`）をしない」
  （`book_mruby3/contents/overview.re:460`）は Array のメソッドの引数でも正しい。
